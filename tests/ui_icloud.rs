//! iCloud through the UI with the in-memory CloudKit store (its own test binary: the store is process-wide).
//! Only synthetic data; nothing reaches iCloud.
use nsh_core::{
    clipboard::MemoryClipboard,
    geometry::{Size, px},
    time::Clock,
};
use nsh_engine::{EngineConfig, Root};
use nsh_testing::{Headless, MemoryStorage};
use safay_app::icloud::{self, MemoryStore};
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

const PASSWORD: &str = "P8!nQ2@vT7#sL4$m";

fn open() -> Headless {
    let mut config = EngineConfig::new(Root::of::<safay::FixtureApp>());
    config.viewport = Size::new(px(1040.0), px(720.0));
    config.storage = Some(MemoryStorage::new(std::iter::empty::<(String, String)>()));
    config.clipboard = Some(Arc::new(MemoryClipboard::default()));
    config.clock = Clock::manual(SystemTime::UNIX_EPOCH);
    config.network = None;
    config.data_dir = None;
    config.cache = nsh_engine::CacheOptions::off();
    Headless::start(config).unwrap()
}

#[test]
fn choosing_icloud_links_and_backs_up() {
    let store = Arc::new(MemoryStore::new("_fixture-account"));
    icloud::install(store.clone());
    let mut app = open();
    app.wait_for("Start fresh").unwrap();
    app.press("role:button[Start fresh]").unwrap();
    app.type_text("id:new-password", PASSWORD).unwrap();
    app.type_text("id:confirm-password", PASSWORD).unwrap();
    app.press("role:button[Create Vault]").unwrap();
    app.wait_for_within("No 2FA accounts yet", Duration::from_secs(20)).unwrap();

    app.press("role:button[Backup & Sync]").unwrap();
    app.press("role:button[Set Up]").unwrap();
    app.wait_for("iCloud").unwrap();
    assert!(!app.screen().texts().contains(&"Not available in this build of Safay"));
    app.press("role:button[iCloud]").unwrap();
    app.wait_for_within("Private to your Apple Account", Duration::from_secs(20)).unwrap();
    // The first sync runs right after linking: only the envelope reaches the store.
    for _ in 0..100 {
        if store.envelope().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(store.envelope().is_some(), "linking uploads the encrypted vault");
    // Changes from other devices arrive as silent pushes: linking subscribes to them.
    let subscribed = || store.subscribed.load(std::sync::atomic::Ordering::SeqCst);
    app.wait_until("subscribed", Duration::from_secs(10), |_| subscribed()).unwrap();

    // Turning off can delete the backup in iCloud too (for every device): the record goes, and so does the subscription.
    app.on_dialog(|_| nsh_testing::DialogAnswer::Press("Turn Off and Delete iCloud Backup".into()));
    app.press("role:button[Turn Off Backup]").unwrap();
    app.wait_for_within("Backup is off", Duration::from_secs(20)).unwrap();
    assert!(store.envelope().is_none(), "the backup in iCloud is deleted");
    app.wait_until("unsubscribed", Duration::from_secs(10), |_| !subscribed()).unwrap();

    // Signed out of iCloud: choosing it again reports why.
    *store.state.lock().unwrap() = icloud::AccountState::NoAccount;
    app.press("role:button[Set Up]").unwrap();
    app.press("role:button[iCloud]").unwrap();
    app.wait_for_within("Sign in to iCloud on this device to use iCloud backup.", Duration::from_secs(20)).unwrap();
}
