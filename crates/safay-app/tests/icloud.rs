//! iCloud backup against the in-memory CloudKit store. Only synthetic data; no iCloud, no network.
use safay_app::{
    controller::Controller,
    icloud::{self, AccountState, ICloudError, MemoryStore},
    session::{Clock, Draft},
    storage::MemoryRepository,
    sync::{self, Location, Source, Status, SyncError},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const PASSWORD: &str = "P8!nQ2@vT7#sL4$m";

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

#[test]
fn icloud_backup_restore_conflict_and_account_changes() {
    // No store installed: this build can't use iCloud, and choosing it says so.
    assert!(!icloud::available());
    assert_eq!(icloud::location(), Err(SyncError::ICloud(ICloudError::NotEntitled)));

    let store = Arc::new(MemoryStore::new("_user-a"));
    icloud::install(store.clone());
    assert!(icloud::available());

    // Signed out: choosing iCloud asks to sign in.
    *store.state.lock().unwrap() = AccountState::NoAccount;
    assert_eq!(icloud::location(), Err(SyncError::ICloud(ICloudError::NotSignedIn)));
    *store.state.lock().unwrap() = AccountState::Available;
    let location = icloud::location().unwrap();
    assert_eq!(location, Location::ICloud { account: "_user-a".into() });
    assert_eq!(location.title(), "iCloud");

    // First device: link, back up; only ciphertext is stored.
    let a = Device::new(0);
    a.c.run(0, |s| s.create(PASSWORD, false)).unwrap();
    a.add("Alpha", "JBSWY3DPEHPK3PXP");
    sync::link(&a.c, a.c.epoch(), location.clone()).unwrap();
    assert_eq!(a.run(), Ok(Status::Synced));
    let stored = store.envelope().expect("uploaded");
    assert!(!String::from_utf8_lossy(&stored).contains("JBSWY3DPEHPK3PXP"), "only ciphertext leaves the device");

    // A second device restores, then both change: the server rejects the stale write, the core reports a conflict.
    let b = Device::new(1000);
    sync::restore_first_run(&b.c, b.c.epoch(), location.clone(), PASSWORD).unwrap();
    assert_eq!(b.issuers(), vec!["Alpha"]);
    b.add("Beta", "GEZDGNBVGY3TQOJQ");
    assert_eq!(b.run(), Ok(Status::Synced));
    a.add("Gamma", "MFRGGZDFMZTWQ2LK");
    assert_eq!(a.run(), Ok(Status::Conflict));
    sync::merge_both(&a.c, a.c.epoch(), "A", Default::default()).unwrap();
    assert_eq!(a.issuers(), vec!["Alpha", "Beta", "Gamma"]);
    assert_eq!(b.run(), Ok(Status::Synced));
    assert_eq!(b.issuers(), vec!["Alpha", "Beta", "Gamma"]);

    // Offline: nothing changes locally, and the next run catches up.
    store.offline.store(true, Ordering::SeqCst);
    a.add("Delta", "JBSWY3DPEHPK3PXP");
    assert_eq!(a.run(), Err(SyncError::ICloud(ICloudError::Offline)));
    store.offline.store(false, Ordering::SeqCst);
    assert_eq!(a.run(), Ok(Status::Synced));

    // Another vault's backup in this account can't be opened with this key: linking refuses it.
    let other = Device::new(2000);
    other.c.run(0, |s| s.create("Z9#kR4!pW2@xQ7$v", false)).unwrap();
    assert_eq!(sync::link(&other.c, other.c.epoch(), location.clone()), Err(SyncError::Unreadable));

    // The device switches iCloud accounts: this vault is never written into the other account.
    *store.account.lock().unwrap() = "_user-b".into();
    let before = store.envelope();
    a.add("Epsilon", "GEZDGNBVGY3TQOJQ");
    assert_eq!(a.run(), Err(SyncError::ICloud(ICloudError::AccountChanged)));
    assert!(store.envelope() == before, "nothing was written to the other account");
}
