//! Picking the backup's folder on a WebDAV server (Nextcloud is WebDAV underneath), against `cloudset`'s in-memory
//! server: the listing checks the sign-in, a folder is made and used, the backup goes there, and a location set up before
//! folders could be picked keeps its `Safay` folder. Only synthetic data; no network, no keychain.
use cloudset::{
    CertificatePin, Connection, Endpoint, HttpRequest, HttpResponse, RemotePath, Transport,
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
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

const PASSWORD: &str = "P8!nQ2@vT7#sL4$m";
const AUTH: &str = "Basic YWxpY2U6cHc="; // alice:pw

struct Server(Mutex<MemoryDav>);
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
struct Fixed;
impl Clock for Fixed {
    fn wall(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_700_000_000)
    }
    fn elapsed(&self) -> Duration {
        Duration::ZERO
    }
}

fn path(text: &str) -> RemotePath {
    RemotePath::parse(text).unwrap()
}
fn connection(password: &str) -> Connection {
    Connection::webdav(Endpoint::parse(BASE, Transport::RequireHttps).unwrap(), "alice", password)
}
/// A new vault with one account, linked to `location` and backed up.
fn back_up_to(location: Location) -> Result<Status, SyncError> {
    let c = Controller::new(Box::new(MemoryRepository::default()), Arc::new(Fixed)).unwrap();
    c.run(0, |s| s.create(PASSWORD, false)).unwrap();
    c.run(c.epoch(), |s| {
        let mut d = Draft::default();
        d.issuer = "Alpha".into();
        d.secret = "JBSWY3DPEHPK3PXP".into();
        s.save_account(&d)
    })
    .unwrap();
    sync::link(&c, c.epoch(), location)?;
    sync::run(&c, c.epoch(), "Device", Source::Auto)
}

#[test]
fn webdav_backs_up_to_the_folder_picked() {
    let server = Arc::new(Server(Mutex::new(MemoryDav::new(AUTH))));
    let credentials = Arc::new(Memory::default());
    cloud::install(Services { http: server.clone(), credentials: credentials.clone(), user_agent: "safay-test/1".into() });

    // Listing the top checks the sign-in; nothing is stored while browsing.
    assert_eq!(cloud::list_folder(&mut connection("wrong"), &RemotePath::root()), Err(SyncError::SignIn));
    let mut held = connection("pw");
    let top = cloud::list_folder(&mut held, &RemotePath::root()).unwrap();
    assert!(top.empty && top.folders.is_empty() && !top.has_backup);
    assert!(credentials.0.lock().unwrap().is_empty(), "browsing stores no sign-in");

    // Folders made at the top and inside one, listed sorted.
    cloud::create_folder(&mut held, &path("Backups")).unwrap();
    cloud::create_folder(&mut held, &path("Archive")).unwrap();
    cloud::create_folder(&mut held, &path("Backups/2FA")).unwrap();
    assert_eq!(cloud::list_folder(&mut held, &RemotePath::root()).unwrap().folders, vec!["Archive", "Backups"]);

    // Using a folder stores the sign-in and keeps the folder on the location, shown after the account.
    let location = cloud::use_folder(held.clone(), path("Backups/2FA")).unwrap();
    let Location::Cloud { provider, account, credential, folder, .. } = &location else { panic!("a cloud location") };
    assert_eq!(*provider, Provider::Webdav);
    assert_eq!(folder.as_deref(), Some("Backups/2FA"));
    assert!(account.ends_with(" › Backups › 2FA"), "{account}");
    assert!(credentials.load(credential).is_some());

    // The backup goes into that folder, nowhere else; the folder then lists it.
    assert_eq!(back_up_to(location.clone()), Ok(Status::Synced));
    let dav = || server.0.lock().unwrap();
    assert!(dav().data("Backups/2FA/vault.safayvault").is_some());
    assert!(dav().data("Backups/2FA/vault.metadata.json").is_some());
    assert!(dav().data("Safay/vault.safayvault").is_none() && !dav().is_dir("Safay"));
    let picked = cloud::list_folder(&mut held, &path("Backups/2FA")).unwrap();
    assert!(picked.has_backup && picked.usable() && !picked.empty);

    // The location keeps its folder through settings; one saved before folders could be picked has none.
    let json = serde_json::to_string(&location).unwrap();
    assert_eq!(serde_json::from_str::<Location>(&json).unwrap(), location);
    let legacy = r#"{"kind":"cloud","provider":"webdav","account":"alice","credential":"old"}"#;
    let Location::Cloud { folder, .. } = serde_json::from_str::<Location>(legacy).unwrap() else { panic!("a cloud location") };
    assert_eq!(folder, None);

    // Such a location keeps backing up to its `Safay` folder at the top, made when missing.
    credentials.save("legacy", &connection("pw").to_bytes()).unwrap();
    let old =
        Location::Cloud { provider: Provider::Webdav, account: "alice".into(), credential: "legacy".into(), pinned: false, folder: None };
    assert_eq!(back_up_to(old), Ok(Status::Synced));
    assert!(dav().data("Safay/vault.safayvault").is_some());

    // The top itself can take the backup on WebDAV (unlike a NAS's shared folders): no folder shows after the account.
    let mut empty = Server(Mutex::new(MemoryDav::new(AUTH)));
    std::mem::swap(&mut *server.0.lock().unwrap(), &mut *empty.0.get_mut().unwrap());
    let top = cloud::use_folder(connection("pw"), RemotePath::root()).unwrap();
    let Location::Cloud { account, folder, .. } = &top else { panic!("a cloud location") };
    assert_eq!(folder.as_deref(), Some(""));
    assert!(!account.contains('›'), "{account}");
    assert_eq!(back_up_to(top), Ok(Status::Synced));
    assert!(dav().data("vault.safayvault").is_some());
}
