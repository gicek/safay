//! Cloud backup to a Synology NAS through `cloudset`'s in-memory device: found with its QuickConnect ID, signed in with
//! two-step verification, the backup in a `Safay` folder of the shared folder, and the address looked up again when the
//! NAS stops answering where it was. Only synthetic data; no network, no keychain.
//!
//! A test binary of its own: `cloud::install` sets process-wide services.
use cloudset::{
    CertificatePin, HttpRequest, HttpResponse,
    synology::quickconnect::Region,
    testing::synology::{LAN_ADDRESS, MemorySynology, PUBLIC_ADDRESS, QUICKCONNECT_ID, SHARE},
};
use safay_app::{
    cloud::{self, Credentials, Http, Provider, Services, SynologyAddress, SynologyLogin, TransportError},
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

struct Server(Mutex<MemorySynology>);
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
fn device(clock: Arc<Time>) -> Arc<Controller> {
    Controller::new(Box::new(MemoryRepository::default()), clock).unwrap()
}
fn issuers(c: &Controller) -> Vec<String> {
    c.run(c.epoch(), |s| Ok(s.projection("", None)?.rows.into_iter().map(|r| r.issuer).collect::<Vec<_>>())).unwrap()
}
fn add(c: &Controller, clock: &Time, issuer: &str) {
    clock.0.fetch_add(60, Ordering::SeqCst);
    c.run(c.epoch(), |s| {
        let mut draft = Draft::default();
        draft.issuer = issuer.into();
        draft.secret = "JBSWY3DPEHPK3PXP".into();
        s.save_account(&draft)
    })
    .unwrap();
}

#[test]
fn quickconnect_sign_in_backup_and_a_moved_nas() {
    let mut memory = MemorySynology::new(Region::ChinaMainland);
    memory.otp_code = Some("246810".into());
    let server = Arc::new(Server(Mutex::new(memory)));
    let credentials = Arc::new(Memory::default());
    cloud::install(Services { http: server.clone(), credentials: credentials.clone(), user_agent: "safay-test/1".into() });

    // The typed ID names no region: the global control host redirects to the one that knows the NAS.
    let SynologyAddress::QuickConnect(id, region) = cloud::synology_address(QUICKCONNECT_ID, false).unwrap() else {
        panic!("a QuickConnect ID")
    };
    assert_eq!(region, None);
    let (region, endpoint) = cloud::synology_find(&id, Region::Global).unwrap();
    assert_eq!((region, endpoint.as_str()), (Region::ChinaMainland, LAN_ADDRESS), "the local network address answers first");
    // An address is not an ID.
    assert!(matches!(cloud::synology_address("nas.example.com:5001", false), Ok(SynologyAddress::Direct(_))));

    let login = |otp_code: Option<&str>| SynologyLogin {
        endpoint: endpoint.clone(),
        quickconnect: Some((id.clone(), region)),
        username: "alice".into(),
        password: Zeroizing::new("secret".into()),
        otp_code: otp_code.map(Into::into),
        trusted: None,
    };
    assert_eq!(cloud::synology_sign_in(login(None)).err(), Some(SyncError::OtpRequired));
    let mut connection = cloud::synology_sign_in(login(Some("246810"))).unwrap();

    // The user picks the folder: the shared folders first, then one made inside one for the backup.
    let path = |p: &str| cloudset::RemotePath::parse(p).unwrap();
    server.0.lock().unwrap().put_external("home/notes.txt", b"n");
    server.0.lock().unwrap().mkdir_external("home/#recycle");
    let top = cloud::synology_list(&mut connection, &path("")).unwrap();
    assert_eq!(top.folders, vec![SHARE.to_owned()]);
    let home = cloud::synology_list(&mut connection, &path(SHARE)).unwrap();
    assert_eq!((home.folders.is_empty(), home.empty, home.usable()), (true, false, false), "a file is there; #recycle is hidden");
    assert_eq!(cloud::synology_create_folder(&mut connection, &path("Backups")), Err(SyncError::Forbidden), "no new shared folders");
    cloud::synology_create_folder(&mut connection, &path("home/Safay")).unwrap();
    let folder = cloud::synology_list(&mut connection, &path("home/Safay")).unwrap();
    assert!(folder.empty && folder.usable() && !folder.has_backup);
    assert_eq!(cloud::synology_use_folder(connection.clone(), path("")).err(), Some(SyncError::Forbidden));
    let location = cloud::synology_use_folder(connection.clone(), path("home/Safay")).unwrap();
    let Location::Cloud { provider, account, .. } = &location else { panic!("a cloud location") };
    assert_eq!((*provider, account.as_str()), (Provider::Synology, "alice · cloudset-nas › home/Safay"));

    let clock = Arc::new(Time::default());
    let a = device(clock.clone());
    a.run(0, |s| s.create(PASSWORD, false)).unwrap();
    add(&a, &clock, "Alpha");
    sync::link(&a, a.epoch(), location.clone()).unwrap();
    // The first backup replaces nothing: uploaded without asking, into the folder picked.
    assert_eq!(sync::run(&a, a.epoch(), "Device", Source::Auto), Ok(Status::Synced));
    let again = cloud::synology_list(&mut connection, &path("home/Safay")).unwrap();
    assert!(again.has_backup && again.usable() && !again.empty, "a folder with a backup can be picked again");
    assert!(server.0.lock().unwrap().data("home/Safay/vault.safayvault").is_some());
    assert!(server.0.lock().unwrap().data("home/Safay/vault.metadata.json").is_some());

    // Another device restores it; the session renews without a new code.
    server.0.lock().unwrap().expire_sessions();
    let b = device(Arc::new(Time(AtomicU64::new(1000))));
    sync::restore_first_run(&b, b.epoch(), location.clone(), PASSWORD).unwrap();
    assert_eq!(issuers(&b), vec!["Alpha"]);

    // DSM can't hold a write to the backup it replaces: Safay checks just before the upload and reads it back, so a
    // later change still goes up on its own.
    add(&a, &clock, "Beta");
    assert_eq!(sync::run(&a, a.epoch(), "Device", Source::Auto), Ok(Status::Synced));

    // Another device writes between this one's read and its upload: the upload is refused and the backup stays theirs.
    let backend = location.backend();
    let seen = backend.read().unwrap().unwrap();
    server.0.lock().unwrap().put_external("home/Safay/vault.safayvault", &[seen.data.as_slice(), b"other"].concat());
    let theirs = server.0.lock().unwrap().data("home/Safay/vault.safayvault");
    let meta = safay_app::core::backup::BackupMetadata { device_name: "Device".into(), vault_updated_at: UNIX_EPOCH, saved_at: UNIX_EPOCH };
    let expect = sync::Expect::Unchanged(seen.version.clone().unwrap());
    assert_eq!(backend.write(&seen.data, &meta, &expect), Err(SyncError::RemoteChanged));
    assert_eq!(server.0.lock().unwrap().data("home/Safay/vault.safayvault"), theirs);
    server.0.lock().unwrap().put_external("home/Safay/vault.safayvault", &seen.data);

    // The laptop left the home network: only the public address answers now. The sync finds it and goes on there.
    {
        let mut memory = server.0.lock().unwrap();
        memory.reachable = vec![PUBLIC_ADDRESS.to_owned()];
    }
    assert_eq!(sync::run(&b, b.epoch(), "Other", Source::Auto), Ok(Status::Synced));
    assert_eq!(issuers(&b), vec!["Beta", "Alpha"]);
    let Location::Cloud { credential, .. } = &location else { unreachable!() };
    let stored = cloudset::Connection::from_bytes(&credentials.load(credential).unwrap()).unwrap();
    assert_eq!(stored.endpoint().map(|e| e.as_str()), Some(PUBLIC_ADDRESS), "the new address is kept");

    // No NAS with that ID.
    let SynologyAddress::QuickConnect(unknown, _) = cloud::synology_address("quickconnect.to/someone-else", false).unwrap() else {
        panic!("a QuickConnect ID")
    };
    assert_eq!(cloud::synology_find(&unknown, Region::Global).err(), Some(SyncError::NoRemote));
}
