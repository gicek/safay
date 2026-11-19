//! Cloud backup through `cloudset` against its in-memory WebDAV server. Only synthetic data; no network, no keychain.
use cloudset::{
    CertificatePin, Connection, Endpoint, HttpRequest, HttpResponse, Transport,
    testing::{BASE, MemoryDav},
};
use safay_app::{
    cloud::{self, Credentials, Http, Provider, Services, TransportError},
    controller::Controller,
    session::{Clock, Draft},
    storage::MemoryRepository,
    sync::{self, Location, Source, Status, SyncError},
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

const PASSWORD: &str = "P8!nQ2@vT7#sL4$m";
const AUTH: &str = "Basic YWxpY2U6cHc="; // alice:pw

struct Server {
    dav: Mutex<MemoryDav>,
    offline: AtomicBool,
    /// A certificate the system does not trust, by its SHA-256: as the real executor, only a pin covering the request
    /// with exactly it gets through.
    self_signed: Mutex<Option<[u8; 32]>>,
}
impl Http for Server {
    fn execute(&self, request: HttpRequest, pin: Option<&CertificatePin>) -> Result<HttpResponse, TransportError> {
        if self.offline.load(Ordering::SeqCst) {
            return Err(TransportError::Unreachable);
        }
        if let Some(certificate) = *self.self_signed.lock().unwrap() {
            match pin.filter(|p| p.applies_to(&request.url)) {
                None => return Err(TransportError::Untrusted(Some(certificate))),
                Some(p) if *p.certificate().as_bytes() != certificate => return Err(TransportError::PinMismatch),
                Some(_) => {}
            }
        }
        Ok(self.dav.lock().unwrap().handle(request))
    }
}
#[derive(Default)]
struct Memory(Mutex<HashMap<String, Vec<u8>>>);
impl Credentials for Memory {
    fn load(&self, id: &str) -> Option<Zeroizing<Vec<u8>>> {
        self.0.lock().unwrap().get(id).cloned().map(Zeroizing::new)
    }
    fn save(&self, id: &str, blob: &[u8]) -> Result<(), SyncError> {
        self.0.lock().unwrap().insert(id.into(), blob.to_vec());
        Ok(())
    }
    fn delete(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }
}

#[derive(Default)]
struct Time(AtomicU64);
impl Clock for Time {
    fn wall(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_700_000_000 + self.0.load(Ordering::SeqCst))
    }
    fn elapsed(&self) -> Duration {
        Duration::from_secs(self.0.load(Ordering::SeqCst))
    }
}
struct Device {
    c: Arc<Controller>,
    time: Arc<Time>,
}
impl Device {
    fn new(offset: u64) -> Self {
        let time = Arc::new(Time(AtomicU64::new(offset)));
        Self { c: Controller::new(Box::new(MemoryRepository::default()), time.clone()).unwrap(), time }
    }
    fn add(&self, issuer: &str, secret: &str) {
        self.time.0.fetch_add(3, Ordering::SeqCst);
        self.c
            .run(self.c.epoch(), |s| {
                let mut d = Draft::default();
                d.issuer = issuer.into();
                d.username = "fixture".into();
                d.secret = secret.into();
                s.save_account(&d)
            })
            .unwrap();
    }
    fn issuers(&self) -> Vec<String> {
        let mut v: Vec<_> =
            self.c.run(self.c.epoch(), |s| Ok(s.projection("", None)?.rows.into_iter().map(|r| r.issuer).collect::<Vec<_>>())).unwrap();
        v.sort();
        v
    }
    fn run(&self) -> Result<Status, SyncError> {
        sync::run(&self.c, self.c.epoch(), "Device", Source::Auto)
    }
}

fn location() -> Location {
    Location::Cloud { provider: Provider::Webdav, account: "alice".into(), credential: "test-webdav".into(), pinned: false }
}

#[test]
fn webdav_backup_restore_conflict_and_failures() {
    let server = Arc::new(Server { dav: Mutex::new(MemoryDav::new(AUTH)), offline: AtomicBool::new(false), self_signed: Mutex::new(None) });
    let credentials = Arc::new(Memory::default());
    cloud::install(Services { http: server.clone(), credentials: credentials.clone(), user_agent: "safay-test/1".into() });

    // No saved sign-in yet: linking asks to sign in, nothing is sent.
    let a = Device::new(0);
    a.c.run(0, |s| s.create(PASSWORD, false)).unwrap();
    assert_eq!(sync::link(&a.c, a.c.epoch(), location()), Err(SyncError::SignIn));
    assert!(server.dav.lock().unwrap().requests.is_empty());

    let endpoint = Endpoint::parse(BASE, Transport::RequireHttps).unwrap();
    // Setting up: a wrong password is caught before anything is stored; plain HTTP needs consent.
    assert_eq!(cloud::connect(Connection::webdav(endpoint.clone(), "alice", "wrong")), Err(SyncError::SignIn));
    assert_eq!(cloud::endpoint("192.168.1.20/dav", false), Err(cloud::AddressError::NeedsInsecureConsent));
    assert!(cloud::endpoint("192.168.1.20/dav", true).is_ok());
    assert!(cloud::endpoint("dav.example.com/dav", false).unwrap().is_secure());
    let linked = cloud::connect(Connection::webdav(endpoint.clone(), "alice", "pw")).unwrap();
    assert!(matches!(&linked, Location::Cloud { provider: Provider::Webdav, .. }));
    if let Location::Cloud { credential, .. } = &linked {
        assert!(credentials.load(credential).is_some());
        cloud::forget(credential);
    }
    credentials.save("test-webdav", &Connection::webdav(endpoint, "alice", "pw").to_bytes()).unwrap();
    a.add("Alpha", "JBSWY3DPEHPK3PXP");
    sync::link(&a.c, a.c.epoch(), location()).unwrap();
    assert_eq!(a.run(), Ok(Status::Synced));
    let stored = server.dav.lock().unwrap().data("Safay/vault.safayvault").expect("uploaded");
    assert!(!String::from_utf8_lossy(&stored).contains("JBSWY3DPEHPK3PXP"), "only ciphertext leaves the device");
    assert!(server.dav.lock().unwrap().data("Safay/vault.metadata.json").is_some());
    for request in &server.dav.lock().unwrap().requests {
        assert!(request.url.starts_with(BASE.trim_end_matches('/')), "requests go only to the chosen server");
    }

    // A second device restores from the same location, then both change: a conflict, merged.
    let b = Device::new(1000);
    sync::restore_first_run(&b.c, b.c.epoch(), location(), PASSWORD).unwrap();
    assert_eq!(b.issuers(), vec!["Alpha"]);
    b.add("Beta", "GEZDGNBVGY3TQOJQ");
    assert_eq!(b.run(), Ok(Status::Synced));
    a.add("Gamma", "MFRGGZDFMZTWQ2LK");
    assert_eq!(a.run(), Ok(Status::Conflict));
    sync::merge_both(&a.c, a.c.epoch(), "A", Default::default()).unwrap();
    assert_eq!(a.issuers(), vec!["Alpha", "Beta", "Gamma"]);
    assert_eq!(b.run(), Ok(Status::Synced));
    assert_eq!(b.issuers(), vec!["Alpha", "Beta", "Gamma"]);

    // Offline: a transport failure, not a protocol error; nothing changes locally.
    server.offline.store(true, Ordering::SeqCst);
    a.add("Delta", "JBSWY3DPEHPK3PXP");
    assert_eq!(a.run(), Err(SyncError::Network));
    server.offline.store(false, Ordering::SeqCst);
    assert_eq!(a.run(), Ok(Status::Synced));

    // Revoked credentials: sign in again.
    server.dav.lock().unwrap().authorization = "Basic other".into();
    a.add("Epsilon", "GEZDGNBVGY3TQOJQ");
    assert_eq!(a.run(), Err(SyncError::SignIn));

    // Signing out forgets the stored connection.
    cloud::forget("test-webdav");
    assert!(credentials.load("test-webdav").is_none());

    // First-run restore keeps a sign-in in memory: taking it out of the store sends nothing, and it connects again later
    // under a new credential without a new sign-in.
    server.dav.lock().unwrap().authorization = AUTH.into();
    let endpoint = Endpoint::parse(BASE, Transport::RequireHttps).unwrap();
    credentials.save("test-restore", &Connection::webdav(endpoint, "alice", "pw").to_bytes()).unwrap();
    let sent = server.dav.lock().unwrap().requests.len();
    let kept = cloud::take("test-restore").expect("stored");
    assert!(credentials.load("test-restore").is_none());
    assert_eq!(server.dav.lock().unwrap().requests.len(), sent, "taking a sign-in out does not revoke it");
    let again = cloud::reconnect(&kept).unwrap();
    let Location::Cloud { credential, .. } = &again else { panic!("a cloud location") };
    assert_eq!(credentials.load(credential).as_deref(), Some(&*kept));
    cloud::forget(credential);

    // A self-signed server: setup reports its fingerprint; once the user confirms it, only that certificate is trusted,
    // and the location says so. A pinned location whose certificate changes is refused.
    server.dav.lock().unwrap().authorization = AUTH.into();
    let certificate = [7u8; 32];
    *server.self_signed.lock().unwrap() = Some(certificate);
    let endpoint = Endpoint::parse(BASE, Transport::RequireHttps).unwrap();
    let plain = cloud::webdav(endpoint.clone(), "alice".into(), "pw".into(), None).unwrap();
    assert_eq!(cloud::connect(plain), Err(SyncError::CertificateUnverified(certificate)));
    let trusted = cloud::webdav(endpoint.clone(), "alice".into(), "pw".into(), Some(certificate)).unwrap();
    let pinned = cloud::connect(trusted).unwrap();
    assert!(pinned.unverified_certificate());
    assert!(!location().unverified_certificate());
    let other = cloud::webdav(endpoint, "alice".into(), "pw".into(), Some([8u8; 32])).unwrap();
    assert_eq!(cloud::connect(other), Err(SyncError::CertificateChanged));
    if let Location::Cloud { credential, .. } = &pinned {
        cloud::forget(credential);
    }
    // The flag stays out of `sync.json` for the locations that don't need it, so earlier files read the same.
    let json = serde_json::to_string(&location()).unwrap();
    assert!(!json.contains("pinned"), "{json}");
    assert!(serde_json::to_string(&pinned).unwrap().contains("\"pinned\":true"));
}
