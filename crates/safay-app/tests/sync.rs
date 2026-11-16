//! Folder backup between simulated devices. Only synthetic data in a temporary directory.
use safay_app::{
    controller::Controller,
    session::{Clock, Draft},
    storage::MemoryRepository,
    sync::{self, Direction, Location, Source, Status, SyncError},
};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const PASSWORD: &str = "P8!nQ2@vT7#sL4$m";
const OTHER: &str = "w6*R9!jB3$qH7&zF";

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
static DEVICES: AtomicU64 = AtomicU64::new(0);
impl Device {
    /// Device clocks are apart, as real ones are; files written by different devices are apart by more than the
    /// core's modification-time tolerance (0.5 s).
    fn new() -> Self {
        let time = Arc::new(Time(AtomicU64::new(DEVICES.fetch_add(1, Ordering::SeqCst) * 1000)));
        Self { c: Controller::new(Box::new(MemoryRepository::default()), time.clone()).unwrap(), time }
    }
    /// Each save happens a few seconds later, so envelope timestamps differ as on real devices.
    fn tick(&self) {
        self.time.0.fetch_add(3, Ordering::SeqCst);
    }
    fn add(&self, issuer: &str, secret: &str) {
        self.tick();
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
    fn status(&self) -> Status {
        self.c.run(self.c.epoch(), |s| Ok(s.sync_status())).unwrap()
    }
    fn run(&self) -> Result<Status, SyncError> {
        std::thread::sleep(Duration::from_millis(600));
        sync::run(&self.c, self.c.epoch(), "Device", Source::Auto)
    }
}
fn folder() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Safay");
    (dir, path)
}

#[test]
fn first_backup_restore_on_another_device_and_one_sided_changes() {
    let (_dir, path) = folder();
    let a = Device::new();
    a.c.run(0, |s| s.create(PASSWORD, false)).unwrap();
    a.add("Alpha", "JBSWY3DPEHPK3PXP");
    sync::link(&a.c, a.c.epoch(), Location::Folder { path: path.clone() }).unwrap();
    assert!(a.c.run(a.c.epoch(), |s| Ok(s.sync_due())).unwrap(), "linking syncs at once");
    assert_eq!(a.run(), Ok(Status::Synced));
    assert!(path.join("vault.safayvault").is_file());
    let raw = std::fs::read(path.join("vault.safayvault")).unwrap();
    assert!(!String::from_utf8_lossy(&raw).contains("JBSWY3DPEHPK3PXP"), "only ciphertext leaves the device");

    let b = Device::new();
    assert_eq!(sync::restore_first_run(&b.c, b.c.epoch(), path.clone(), "wrong password"), Err(SyncError::Password));
    sync::restore_first_run(&b.c, b.c.epoch(), path.clone(), PASSWORD).unwrap();
    assert_eq!(b.issuers(), vec!["Alpha"]);

    // B adds, backs up; A downloads it.
    b.add("Beta", "GEZDGNBVGY3TQOJQ");
    assert_eq!(b.run(), Ok(Status::Synced));
    assert_eq!(a.run(), Ok(Status::Synced));
    assert_eq!(a.issuers(), vec!["Alpha", "Beta"]);
    let history = a.c.run(a.c.epoch(), |s| Ok(s.sync_state().history.clone())).unwrap();
    assert_eq!(history[0].direction, Some(Direction::Download));
    // Nothing changed: an automatic sync is up to date and records no history.
    assert_eq!(a.run(), Ok(Status::Synced));
    assert_eq!(a.c.run(a.c.epoch(), |s| Ok(s.sync_state().history.len())).unwrap(), history.len());
}

#[test]
fn both_sides_changed_is_a_conflict_resolved_by_merging() {
    let (_dir, path) = folder();
    let a = Device::new();
    a.c.run(0, |s| s.create(PASSWORD, false)).unwrap();
    a.add("Alpha", "JBSWY3DPEHPK3PXP");
    sync::link(&a.c, a.c.epoch(), Location::Folder { path: path.clone() }).unwrap();
    a.run().unwrap();
    let b = Device::new();
    sync::restore_first_run(&b.c, b.c.epoch(), path.clone(), PASSWORD).unwrap();
    b.add("Beta", "GEZDGNBVGY3TQOJQ");
    b.run().unwrap();
    a.add("Gamma", "MFRGGZDFMZTWQ2LK");
    assert_eq!(a.run(), Ok(Status::Conflict));
    assert_eq!(a.status(), Status::Conflict);
    let review = a.c.run(a.c.epoch(), |s| s.conflict_review()).unwrap();
    assert_eq!(review.only_local.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), vec!["Gamma"]);
    assert_eq!(review.only_remote.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), vec!["Beta"]);
    sync::merge_both(&a.c, a.c.epoch(), "A", HashSet::new()).unwrap();
    assert_eq!(a.issuers(), vec!["Alpha", "Beta", "Gamma"]);
    assert_eq!(b.run(), Ok(Status::Synced));
    assert_eq!(b.issuers(), vec!["Alpha", "Beta", "Gamma"]);
}

#[test]
fn conflict_can_keep_either_copy() {
    let (_dir, path) = folder();
    let a = Device::new();
    a.c.run(0, |s| s.create(PASSWORD, false)).unwrap();
    sync::link(&a.c, a.c.epoch(), Location::Folder { path: path.clone() }).unwrap();
    a.run().unwrap();
    let b = Device::new();
    sync::restore_first_run(&b.c, b.c.epoch(), path.clone(), PASSWORD).unwrap();
    b.add("Beta", "GEZDGNBVGY3TQOJQ");
    b.run().unwrap();
    a.add("Alpha", "JBSWY3DPEHPK3PXP");
    assert_eq!(a.run(), Ok(Status::Conflict));
    sync::use_backup(&a.c, a.c.epoch(), "A").unwrap();
    assert_eq!(a.issuers(), vec!["Beta"]);
    a.add("Again", "JBSWY3DPEHPK3PXP");
    b.add("Other", "MFRGGZDFMZTWQ2LK");
    b.run().unwrap();
    assert_eq!(a.run(), Ok(Status::Conflict));
    sync::keep_local(&a.c, a.c.epoch(), "A").unwrap();
    assert_eq!(b.run(), Ok(Status::Synced));
    assert_eq!(b.issuers(), vec!["Again", "Beta"]);
}

#[test]
fn another_vaults_backup_is_never_linked_or_overwritten_but_can_be_restored_with_its_password() {
    let (_dir, path) = folder();
    let a = Device::new();
    a.c.run(0, |s| s.create(PASSWORD, false)).unwrap();
    a.add("Alpha", "JBSWY3DPEHPK3PXP");
    sync::link(&a.c, a.c.epoch(), Location::Folder { path: path.clone() }).unwrap();
    a.run().unwrap();
    let before = std::fs::read(path.join("vault.safayvault")).unwrap();
    let c = Device::new();
    c.c.run(0, |s| s.create(OTHER, false)).unwrap();
    assert_eq!(sync::link(&c.c, c.c.epoch(), Location::Folder { path: path.clone() }), Err(SyncError::Unreadable));
    assert_eq!(std::fs::read(path.join("vault.safayvault")).unwrap(), before);
    assert_eq!(c.status(), Status::Off);

    // A changes its master password: its next upload is unreadable to an old key, but A keeps syncing.
    a.tick();
    a.c.run(a.c.epoch(), |s| s.change_password(PASSWORD, OTHER, false)).unwrap();
    assert_eq!(a.run(), Ok(Status::Synced));
    let b = Device::new();
    sync::restore_first_run(&b.c, b.c.epoch(), path.clone(), OTHER).unwrap();
    assert_eq!(b.issuers(), vec!["Alpha"]);
}

#[test]
fn unreadable_backup_blocks_sync_until_restored_with_its_password() {
    let (_dir, path) = folder();
    let a = Device::new();
    a.c.run(0, |s| s.create(PASSWORD, false)).unwrap();
    sync::link(&a.c, a.c.epoch(), Location::Folder { path: path.clone() }).unwrap();
    a.run().unwrap();
    let b = Device::new();
    sync::restore_first_run(&b.c, b.c.epoch(), path.clone(), PASSWORD).unwrap();
    b.add("Beta", "GEZDGNBVGY3TQOJQ");
    b.tick();
    b.c.run(b.c.epoch(), |s| s.change_password(PASSWORD, OTHER, false)).unwrap();
    b.run().unwrap();
    assert_eq!(a.run(), Err(SyncError::Unreadable));
    assert_eq!(a.status(), Status::Unreadable);
    assert_eq!(sync::restore_unreadable(&a.c, a.c.epoch(), PASSWORD), Err(SyncError::Password));
    assert!(a.issuers().is_empty());
    sync::restore_unreadable(&a.c, a.c.epoch(), OTHER).unwrap();
    assert_eq!(a.issuers(), vec!["Beta"]);
    assert_eq!(a.status(), Status::Synced);
    // The restored vault now saves under the backup's key.
    a.add("Gamma", "MFRGGZDFMZTWQ2LK");
    assert_eq!(a.run(), Ok(Status::Synced));
}

#[test]
fn missing_folder_fails_and_turning_off_keeps_the_backup() {
    let a = Device::new();
    a.c.run(0, |s| s.create(PASSWORD, false)).unwrap();
    let gone = PathBuf::from("/nonexistent-safay-test-root/sub/Safay");
    sync::link(&a.c, a.c.epoch(), Location::Folder { path: gone }).unwrap_err();
    let (_dir, path) = folder();
    sync::link(&a.c, a.c.epoch(), Location::Folder { path: path.clone() }).unwrap();
    a.run().unwrap();
    a.c.run(a.c.epoch(), |s| s.sync_off()).unwrap();
    assert_eq!(a.status(), Status::Off);
    assert!(path.join("vault.safayvault").is_file());
    // Locked: nothing runs.
    a.c.request_lock();
    sync::link(&a.c, a.c.epoch(), Location::Folder { path }).unwrap_err();
}
