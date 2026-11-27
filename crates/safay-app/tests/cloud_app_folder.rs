//! Cloud backup to Dropbox, OneDrive and Google Drive through `cloudset`'s in-memory servers: the backup lives at the
//! root of Safay's app folder, not in a folder inside it. Only synthetic data; no network, no keychain.
//!
//! A test binary of its own: `cloud::install` sets process-wide services.
use cloudset::{
    AccessScope, CertificatePin, ClientConfig, HttpRequest, HttpResponse, OAuthClient, ProviderKind,
    testing::oauth::{FakeProvider, GOOGLE_APP_FOLDER, MemoryCloud},
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
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

const PASSWORD: &str = "vX9#qL2!mR7@tZ4$wK8&nB3^pJ6";
const CREDENTIAL: &str = "test-app-folder";

struct Server(Mutex<MemoryCloud>);
impl Http for Server {
    fn execute(&self, request: HttpRequest, _pin: Option<&CertificatePin>) -> Result<HttpResponse, TransportError> {
        Ok(self.0.lock().unwrap().handle(request))
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
fn device(offset: u64) -> Arc<Controller> {
    device_at(Arc::new(Time(AtomicU64::new(offset))))
}
fn device_at(clock: Arc<Time>) -> Arc<Controller> {
    Controller::new(Box::new(MemoryRepository::default()), clock).unwrap()
}
fn issuers(c: &Controller) -> Vec<String> {
    c.run(c.epoch(), |s| Ok(s.projection("", None)?.rows.into_iter().map(|r| r.issuer).collect::<Vec<_>>())).unwrap()
}

fn add(c: &Controller, issuer: &str) {
    c.run(c.epoch(), |s| {
        let mut draft = Draft::default();
        draft.issuer = issuer.into();
        draft.secret = "JBSWY3DPEHPK3PXP".into();
        s.save_account(&draft)
    })
    .unwrap();
}

/// Signs in to the in-memory provider with the app-folder scope, as Safay does, and returns the stored connection.
fn sign_in(server: &mut MemoryCloud, kind: ProviderKind) -> Vec<u8> {
    let client = OAuthClient::new("client", "com.example.app.oauth://callback").with_folder_name(GOOGLE_APP_FOLDER);
    let authorization = cloudset::oauth::begin(kind, AccessScope::AppFolder, client, &[7; 64]).unwrap();
    let callback = server.authorize(authorization.url());
    let mut exchange = authorization.complete(&callback, &ClientConfig::new("safay-test/1")).unwrap();
    let connection = cloudset::run(exchange.as_mut(), |request| server.handle(request)).unwrap().value;
    connection.to_bytes().to_vec()
}

#[test]
fn backups_live_at_the_root_of_the_app_folder() {
    let cases = [
        (FakeProvider::Dropbox, ProviderKind::Dropbox, Provider::Dropbox),
        (FakeProvider::OneDrive, ProviderKind::OneDrive, Provider::Onedrive),
        (FakeProvider::GoogleDrive, ProviderKind::GoogleDrive, Provider::GoogleDrive),
    ];
    for (fake, kind, provider) in cases {
        let mut memory = MemoryCloud::new(fake);
        let connection = sign_in(&mut memory, kind);
        let server = Arc::new(Server(Mutex::new(memory)));
        let credentials = Arc::new(Memory::default());
        credentials.save(CREDENTIAL, &connection).unwrap();
        cloud::install(Services { http: server.clone(), credentials, user_agent: "safay-test/1".into() });
        let location = Location::Cloud { provider, account: "alice".into(), credential: CREDENTIAL.into(), pinned: false, folder: None };

        let clock = Arc::new(Time::default());
        let a = device_at(clock.clone());
        // A later edit is a later vault.
        let add = |c: &Controller, issuer: &str| {
            clock.0.fetch_add(60, Ordering::SeqCst);
            add(c, issuer);
        };
        a.run(0, |s| s.create(PASSWORD, false)).unwrap();
        add(&a, "Alpha");
        sync::link(&a, a.epoch(), location.clone()).unwrap();
        // The first backup replaces nothing, so it is uploaded even where the write cannot be checked.
        assert_eq!(sync::run(&a, a.epoch(), "Device", Source::Auto), Ok(Status::Synced), "{provider:?}");

        let memory = server.0.lock().unwrap();
        let at = |name: &str| memory.storage_path(name);
        assert!(memory.data(&at("vault.safayvault")).is_some(), "{provider:?}: the backup is at the app folder's root");
        assert!(memory.data(&at("vault.metadata.json")).is_some(), "{provider:?}");
        assert!(!memory.exists(&at("Safay")), "{provider:?}: no Safay folder inside the app folder");
        drop(memory);

        // Another device finds it there.
        let b = device(1000);
        sync::restore_first_run(&b, b.epoch(), location.clone(), PASSWORD).unwrap();
        assert_eq!(issuers(&b), vec!["Alpha"], "{provider:?}");

        // A later change is uploaded on its own everywhere: where the provider can't hold the write to the backup it
        // replaces (Google Drive), Safay checks just before the upload and reads the result back.
        add(&a, "Beta");
        assert_eq!(sync::run(&a, a.epoch(), "Device", Source::Auto), Ok(Status::Synced), "{provider:?}");
        assert_eq!(sync::run(&b, b.epoch(), "Other", Source::Auto), Ok(Status::Synced), "{provider:?}");
        assert_eq!(issuers(&b), vec!["Beta", "Alpha"], "{provider:?}");

        // Both devices change; the other one uploads first. This one finds the backup changed and asks the user instead
        // of replacing it.
        let stored = || {
            let memory = server.0.lock().unwrap();
            memory.data(&memory.storage_path("vault.safayvault"))
        };
        add(&a, "Gamma");
        add(&b, "Delta");
        assert_eq!(sync::run(&b, b.epoch(), "Other", Source::Auto), Ok(Status::Synced), "{provider:?}");
        let theirs = stored();
        assert_eq!(sync::run(&a, a.epoch(), "Device", Source::Auto), Ok(Status::Conflict), "{provider:?}");
        assert_eq!(stored(), theirs, "{provider:?}: the other device's backup is still there");

        // The other device uploads between this one's read and its write: the write is refused, and nothing changes.
        let backend = location.backend();
        let seen = backend.read().unwrap().unwrap();
        let meta =
            safay_app::core::backup::BackupMetadata { device_name: "Device".into(), vault_updated_at: UNIX_EPOCH, saved_at: UNIX_EPOCH };
        let other = [seen.data.as_slice(), b"other device"].concat();
        backend.write(&other, &meta, &sync::Expect::Any).unwrap();
        let theirs = stored();
        let expect = sync::Expect::Unchanged(seen.version.clone().unwrap());
        assert_eq!(backend.write(&seen.data, &meta, &expect), Err(SyncError::RemoteChanged), "{provider:?}");
        assert_eq!(stored(), theirs, "{provider:?}");
    }
}

#[test]
fn a_failed_replayed_request_still_saves_rotated_credentials() {
    struct Interrupted {
        cloud: Mutex<MemoryCloud>,
        refreshed: std::sync::atomic::AtomicBool,
    }
    impl Http for Interrupted {
        fn execute(&self, request: HttpRequest, _: Option<&CertificatePin>) -> Result<HttpResponse, TransportError> {
            if self.refreshed.load(Ordering::SeqCst) {
                return Err(TransportError::Unreachable);
            }
            if request.url.ends_with("/token") {
                self.refreshed.store(true, Ordering::SeqCst);
            }
            Ok(self.cloud.lock().unwrap().handle(request))
        }
    }
    let mut memory = MemoryCloud::new(FakeProvider::OneDrive);
    let original = sign_in(&mut memory, ProviderKind::OneDrive);
    memory.expire_access_token();
    let server = Arc::new(Interrupted { cloud: Mutex::new(memory), refreshed: false.into() });
    let credentials = Arc::new(Memory::default());
    credentials.save(CREDENTIAL, &original).unwrap();
    let services = Services { http: server.clone(), credentials: credentials.clone(), user_agent: "test".into() };
    let client = cloudset::Client::new(cloudset::Connection::from_bytes(&original).unwrap(), ClientConfig::new("test"));
    assert_eq!(cloud::run(&services, Some(CREDENTIAL), client.stat(cloudset::RemotePath::root()), None), Err(SyncError::Network));
    let saved = credentials.load(CREDENTIAL).unwrap();
    assert!(*saved != original);
    let client = client.with_connection(cloudset::Connection::from_bytes(&saved).unwrap());
    let mut memory = server.cloud.lock().unwrap();
    let before = memory.requests.len();
    cloudset::run(client.stat(cloudset::RemotePath::root()).as_mut(), |req| memory.handle(req)).unwrap();
    assert!(memory.requests[before..].iter().all(|req| !req.url.ends_with("/token")));
}
