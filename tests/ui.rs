//! Only RFC/public synthetic data. All application IO uses FixtureApp's explicit memory services.
use nsh_core::{
    clipboard::MemoryClipboard,
    geometry::{Size, point, px},
    time::Clock,
};
use nsh_engine::{EngineConfig, Root};
use nsh_testing::{Headless, MemoryStorage};
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};
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
const PASSWORD: &str = "P8!nQ2@vT7#sL4$m";
const SAMPLE: &str = "otpauth://totp/Fixture:public?secret=JBSWY3DPEHPK3PXP&issuer=Fixture";

fn create(app: &mut Headless) {
    app.wait_for("Start fresh").unwrap();
    app.press("role:button[Start fresh]").unwrap();
    app.type_text("id:new-password", PASSWORD).unwrap();
    app.type_text("id:confirm-password", PASSWORD).unwrap();
    app.press("role:button[Create Vault]").unwrap();
    app.wait_for_within("No 2FA accounts yet", Duration::from_secs(20)).unwrap();
}
fn paste(app: &mut Headless, links: &str) {
    app.press("role:button[Add]").unwrap();
    app.press("role:button[Paste Links]").unwrap();
    app.type_text("id:import-text", links).unwrap();
    app.wait_for("Parsed").unwrap();
    app.press("role:button[Save Account]").unwrap();
}

#[test]
fn create_add_by_key_copy_search_lock_and_unlock() {
    let mut app = open();
    create(&mut app);
    app.press("role:button[Add]").unwrap();
    app.press("role:button[Enter Key]").unwrap();
    app.type_text("id:service", "RFC sample").unwrap();
    app.type_text("id:username", "fixture@example.invalid").unwrap();
    app.type_text("id:secret", "JBSWY3DPEHPK3PXP").unwrap();
    app.press("role:button[Save Account]").unwrap();
    app.wait_for("role:button[Copy code for RFC sample]").unwrap();
    // Added by hand: the toast names the account.
    app.wait_for("Added RFC sample").unwrap();
    app.press("role:button[Copy code for RFC sample]").unwrap();
    app.wait_for("Copied").unwrap();
    app.type_text("id:search", "no matching account").unwrap();
    app.wait_for("No matching accounts").unwrap();
    app.press("role:button[Lock]").unwrap();
    app.wait_for("Safay is locked").unwrap();
    assert!(!app.screen().texts().contains(&"RFC sample"));
    app.type_text("id:password", PASSWORD).unwrap();
    app.submit("id:password").unwrap();
    app.wait_for_within("RFC sample", Duration::from_secs(20)).unwrap();
}
#[test]
fn wrong_password_is_reported_and_keeps_the_vault_locked() {
    let mut app = open();
    create(&mut app);
    app.press("role:button[Lock]").unwrap();
    app.wait_for("Safay is locked").unwrap();
    app.type_text("id:password", "not the password").unwrap();
    app.submit("id:password").unwrap();
    app.wait_for_within("Incorrect master password", Duration::from_secs(20)).unwrap();
    assert!(!app.screen().texts().contains(&"No 2FA accounts yet"));
}
#[test]
fn refresh_does_not_clear_a_password_being_typed() {
    let mut app = open();
    app.wait_for("Start fresh").unwrap();
    app.press("role:button[Start fresh]").unwrap();
    app.type_text("id:new-password", PASSWORD).unwrap();
    app.run_for(Duration::from_secs(2)).unwrap();
    app.type_text("id:confirm-password", PASSWORD).unwrap();
    app.press("role:button[Create Vault]").unwrap();
    app.wait_for_within("No 2FA accounts yet", Duration::from_secs(20)).unwrap();
}
#[test]
fn mismatched_confirmation_disables_create() {
    let mut app = open();
    app.wait_for("Start fresh").unwrap();
    app.press("role:button[Start fresh]").unwrap();
    app.type_text("id:new-password", PASSWORD).unwrap();
    app.type_text("id:confirm-password", "P8!nQ2@vT7#sL4$x").unwrap();
    app.wait_for("Passwords do not match").unwrap();
    assert!(app.press("role:button[Create Vault]").is_err());
}
/// The confirmation is checked as it is typed: nothing while the new password is too short (its own problem shows), nor
/// while the confirmation is a prefix being typed; leaving it incomplete or typing a wrong character says so.
#[test]
fn confirmation_mismatch_waits_for_a_wrong_character_or_leaving_the_field() {
    const MISMATCH: &str = "Passwords do not match";
    let mut app = open();
    app.wait_for("Start fresh").unwrap();
    app.press("role:button[Start fresh]").unwrap();
    app.type_text("id:new-password", "short").unwrap();
    app.type_text("id:confirm-password", "x").unwrap();
    app.wait_for("Password needs at least 8 characters").unwrap();
    assert!(!app.screen().texts().contains(&MISMATCH), "the new password's own problem comes first");

    app.clear("id:new-password").unwrap();
    app.type_text("id:new-password", PASSWORD).unwrap();
    app.clear("id:confirm-password").unwrap();
    app.type_text("id:confirm-password", &PASSWORD[..6]).unwrap();
    app.run_for(Duration::from_millis(200)).unwrap();
    assert!(!app.screen().texts().contains(&MISMATCH), "still typing a prefix");
    app.submit("id:confirm-password").unwrap(); // Return leaves the field; nothing is created
    assert!(!app.screen().texts().contains(&"No 2FA accounts yet"));
    app.wait_for(MISMATCH).unwrap();

    app.type_text("id:confirm-password", "").unwrap();
    app.run_for(Duration::from_millis(200)).unwrap();
    assert!(!app.screen().texts().contains(&MISMATCH), "back in the field with a prefix");
    app.type_text("id:confirm-password", "#").unwrap();
    app.wait_for(MISMATCH).unwrap();
}
#[test]
fn generated_runner_cannot_implicitly_open_a_real_vault() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<safay::App>())).unwrap();
    app.wait_for("NSH UI harness. Launch the local vault with cargo run.").unwrap();
}
#[test]
fn secret_needs_the_master_password_and_lock_removes_it() {
    let mut app = open();
    create(&mut app);
    paste(&mut app, SAMPLE);
    app.wait_for("Fixture").unwrap();
    app.press("role:button[Details]").unwrap();
    app.wait_for("Account Details").unwrap();
    // The details load after the sheet opens.
    app.wait_for("Show Secret").unwrap();
    app.press("role:button[Show Secret]").unwrap();
    app.wait_for("Verify It’s You").unwrap();
    assert!(!app.screen().texts().contains(&"JBSWY3DPEHPK3PXP"));
    app.type_text("id:auth-password", "wrong password here").unwrap();
    app.press("role:button[Continue]").unwrap();
    app.wait_for_within("Incorrect master password", Duration::from_secs(20)).unwrap();
    app.clear("id:auth-password").unwrap();
    app.type_text("id:auth-password", PASSWORD).unwrap();
    app.press("role:button[Continue]").unwrap();
    app.wait_for_within("JBSWY3DPEHPK3PXP", Duration::from_secs(20)).unwrap();
    // The sheet covers the toolbar's Lock button (NSH refuses a press on a covered view): close it as a user would.
    app.press("role:button[Done]").unwrap();
    app.wait_for_gone("Account Details", Duration::from_secs(5)).unwrap();
    app.press("role:button[Lock]").unwrap();
    app.wait_for("Safay is locked").unwrap();
    assert!(!app.screen().texts().contains(&"JBSWY3DPEHPK3PXP"));
    // Unlocking does not bring the shown secret back: it needs the master password again.
    app.type_text("id:password", PASSWORD).unwrap();
    app.submit("id:password").unwrap();
    app.wait_for_within("role:button[Details]", Duration::from_secs(20)).unwrap();
    app.press("role:button[Details]").unwrap();
    app.wait_for("Account Details").unwrap();
    app.wait_for("role:button[Show Secret]").unwrap();
    assert!(!app.screen().texts().contains(&"JBSWY3DPEHPK3PXP"));
}
#[test]
fn compact_settings_switch_language_with_large_text() {
    let mut app = open();
    create(&mut app);
    app.set_viewport(Size::new(px(880.0), px(620.0))).unwrap();
    app.set_environment("large text", |env| env.font_scale = 1.5).unwrap();
    app.press("role:button[Settings]").unwrap();
    app.press("role:combobox[Language]").unwrap();
    app.press("简体中文").unwrap(); // i18n-guard: allow
    app.wait_for("外观").unwrap(); // i18n-guard: allow
    app.press("role:button[关闭]").unwrap(); // i18n-guard: allow
    // Settings covers the toolbar until it has closed.
    app.wait_for_gone("外观", Duration::from_secs(5)).unwrap(); // i18n-guard: allow
    app.press("role:button[锁定]").unwrap(); // i18n-guard: allow
    app.wait_for("Safay 已锁定").unwrap(); // i18n-guard: allow
}
#[test]
fn category_inline_create_and_delete_confirmation_keeps_accounts() {
    use nsh_testing::DialogAnswer;
    let mut app = open();
    create(&mut app);
    paste(&mut app, SAMPLE);
    app.wait_for("Fixture").unwrap();
    app.press("role:button[New Category]").unwrap();
    app.type_text("id:category-name", "Work").unwrap();
    app.submit("id:category-name").unwrap();
    app.wait_for("No accounts in “Work” yet").unwrap();
    app.on_dialog(|_| DialogAnswer::Press("Cancel".into()));
    app.press("role:button[All]").unwrap();
    app.wait_for("Fixture").unwrap();
    assert!(app.screen().texts().contains(&"Work"));
    app.on_dialog(|_| DialogAnswer::Press("Delete Category".into()));
    app.press("role:button[Work]").unwrap();
    app.wait_for("No accounts in “Work” yet").unwrap();
}
#[test]
fn deleted_accounts_wait_in_recently_deleted() {
    use nsh_testing::DialogAnswer;
    let mut app = open();
    create(&mut app);
    paste(&mut app, SAMPLE);
    app.wait_for("Fixture").unwrap();
    // Deleting asks nothing: the account moves to Recently Deleted, which the sidebar then shows.
    app.press("role:button[Details]").unwrap();
    app.press("role:button[Delete Account]").unwrap();
    app.wait_for("Moved “Fixture” to Recently Deleted").unwrap();
    app.wait_for("No 2FA accounts yet").unwrap();
    app.press("role:button[Recently Deleted]").unwrap();
    app.wait_for("30 days").unwrap();
    assert!(app.screen().texts().contains(&"Fixture"));
    app.press("role:button[Recover]").unwrap();
    app.wait_for("Recovered “Fixture”").unwrap();
    // Emptied, Recently Deleted goes away and All shows again.
    app.wait_for("role:button[Details]").unwrap();
    app.press("role:button[Details]").unwrap();
    app.wait_for("Account Details").unwrap();
    app.press("role:button[Delete Account]").unwrap();
    app.wait_for("role:button[Recently Deleted]").unwrap();
    app.press("role:button[Recently Deleted]").unwrap();
    app.wait_for("30 days").unwrap();
    app.on_dialog(|_| DialogAnswer::Press("Delete All".into()));
    app.press("role:button[Delete All]").unwrap();
    app.wait_for("No 2FA accounts yet").unwrap();
    app.wait_for_gone("Recently Deleted", Duration::from_secs(5)).unwrap();
}
#[test]
fn duplicate_import_is_reported_in_the_sheet() {
    let mut app = open();
    create(&mut app);
    paste(&mut app, SAMPLE);
    app.wait_for("Fixture").unwrap();
    paste(&mut app, SAMPLE);
    app.wait_for("Account already exists").unwrap();
}
#[test]
fn backup_sheet_starts_off_and_offers_a_folder() {
    let mut app = open();
    create(&mut app);
    app.press("role:button[Backup & Sync]").unwrap();
    app.wait_for("Backup is off").unwrap();
    app.wait_for("No backup history yet").unwrap();
    // The overview stays still (only its history scrolls): nothing around the status card scrolls.
    let still = app.scroll("Backup is off", px(50.0)).unwrap_err();
    assert!(still.contains("no scroll view"), "{still}");
    app.press("role:button[Set Up]").unwrap();
    app.wait_for("Other Folder").unwrap();
    // No CloudKit store in this build: iCloud is listed, disabled, with the reason.
    app.wait_for("iCloud").unwrap();
    app.wait_for("Not available in this build of Safay").unwrap();
    app.press("role:button[Back]").unwrap();
    app.wait_for("Backup is off").unwrap();
}
#[cfg(target_os = "macos")]
#[test]
fn settings_show_default_global_shortcuts_and_login_options() {
    let mut app = open();
    create(&mut app);
    app.press("role:button[Settings]").unwrap();
    app.wait_for("Start in Menu Bar").unwrap();
    app.press("role:button[Shortcuts]").unwrap();
    app.wait_for("Show Menu Bar Panel").unwrap();
    app.wait_for("⌥⌘Z").unwrap();
    app.wait_for("⌥⌘X").unwrap();
}
#[cfg(target_os = "macos")]
#[test]
fn about_shows_updates_disabled_without_an_update_feed() {
    let mut app = open();
    create(&mut app);
    app.press("role:button[Settings]").unwrap();
    app.press("role:button[About]").unwrap();
    app.wait_for("Check for Updates Automatically").unwrap();
    app.wait_for("Download and Install Automatically").unwrap();
    app.wait_for("Software Update").unwrap();
    app.wait_for("Only release builds check for updates").unwrap();
    app.wait_for("role:button[Check Now]").unwrap();
}
#[test]
fn settings_choice_applies_at_once_and_is_kept() {
    let mut app = open();
    create(&mut app);
    app.press("role:button[Settings]").unwrap();
    app.press("role:button[Security]").unwrap();
    app.wait_for("5 minutes").unwrap();
    app.press("role:combobox[Auto-Lock]").unwrap();
    app.press("1 minute").unwrap();
    // Shown at once, while the write runs in the background; the page stays usable (no busy state).
    app.wait_for("1 minute").unwrap();
    // Periodic refreshes and a lock/unlock re-read the stored preferences: the choice stays.
    app.press("role:button[Close]").unwrap();
    app.press("role:button[Lock]").unwrap();
    app.wait_for("Safay is locked").unwrap();
    app.type_text("id:password", PASSWORD).unwrap();
    app.submit("id:password").unwrap();
    app.wait_for_within("No 2FA accounts yet", Duration::from_secs(20)).unwrap();
    app.press("role:button[Settings]").unwrap();
    app.press("role:button[Security]").unwrap();
    app.wait_for("1 minute").unwrap();
    assert!(!app.screen().texts().contains(&"5 minutes"));
}
/// First-run restore lists the cloud storage services; their connection sheet opens over onboarding and Back closes it
/// without linking anything.
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn restore_offers_cloud_storage_and_back_leaves_it() {
    let mut app = open();
    app.wait_for("Restore a backup").unwrap();
    app.press("role:button[Restore a backup]").unwrap();
    app.wait_for("Safay Exported File").unwrap();
    app.press("role:button[WebDAV]").unwrap();
    app.wait_for("Connect to WebDAV").unwrap();
    // Restoring only reads (the restore wording).
    app.wait_for("Safay only reads your encrypted backup from your server.").unwrap();
    // The onboarding page's back button is under the sheet; the sheet's comes first.
    app.press("role:button[Back] >> nth=0").unwrap();
    app.wait_for_gone("Connect to WebDAV", Duration::from_secs(5)).unwrap();
    app.wait_for("Safay Exported File").unwrap();
}
/// The eye in a password field is not a Tab stop: Tab goes from the new password straight to its confirmation.
#[test]
fn tab_skips_the_reveal_button() {
    // macOS, where Tab stops at buttons too (the headless default is a phone, where it moves between fields only).
    let mut config = EngineConfig::new(Root::of::<safay::FixtureApp>());
    config.viewport = Size::new(px(1040.0), px(720.0));
    config.storage = Some(MemoryStorage::new(std::iter::empty::<(String, String)>()));
    config.clipboard = Some(Arc::new(MemoryClipboard::default()));
    config.clock = Clock::manual(SystemTime::UNIX_EPOCH);
    config.network = None;
    config.data_dir = None;
    config.cache = nsh_engine::CacheOptions::off();
    config.environment.os = nsh_core::platform::Os::MacOs;
    config.environment.idiom = nsh_core::platform::Idiom::Desktop;
    let mut app = Headless::start(config).unwrap();
    app.wait_for("Start fresh").unwrap();
    app.press("role:button[Start fresh]").unwrap();
    app.type_text("id:new-password", "abc").unwrap();
    app.press_key("tab").unwrap();
    app.press_key("x").unwrap();
    assert_eq!(app.screen().input_text("id:confirm-password").unwrap(), Some("x"));
    assert_eq!(app.screen().input_text("id:new-password").unwrap(), Some("abc"));
    // Still a labeled button for the pointer and screen readers.
    app.press("role:button[Show] >> nth=0").unwrap();
}

/// The main window's sidebar follows the macOS release, as the system's own: a floating panel inset 8 pt on macOS 26,
/// a flush column on macOS 27 (and 15 and earlier).
#[test]
fn sidebar_follows_the_macos_release() {
    for (version, origin, height) in [((26, 4), (8.0, 8.0), 680.0 - 16.0), ((27, 2), (0.0, 0.0), 680.0), ((15, 7), (0.0, 0.0), 680.0)] {
        let mut config = EngineConfig::new(Root::of::<safay::FixtureApp>());
        config.viewport = Size::new(px(980.0), px(680.0));
        config.storage = Some(MemoryStorage::new(std::iter::empty::<(String, String)>()));
        config.clipboard = Some(Arc::new(MemoryClipboard::default()));
        config.clock = Clock::manual(SystemTime::UNIX_EPOCH);
        config.network = None;
        config.data_dir = None;
        config.cache = nsh_engine::CacheOptions::off();
        config.environment.os = nsh_core::platform::Os::MacOs;
        config.environment.idiom = nsh_core::platform::Idiom::Desktop;
        config.environment.os_version = version;
        let mut app = Headless::start(config).unwrap();
        create(&mut app);
        let panel = app.screen().layout_nodes().into_iter().find(|n| n.test_id.as_deref() == Some("sidebar-panel")).expect("the sidebar");
        assert_eq!((panel.bounds.origin.x, panel.bounds.origin.y), (px(origin.0), px(origin.1)), "macOS {version:?}");
        assert_eq!((panel.bounds.size.width, panel.bounds.size.height), (px(210.0), px(height)), "macOS {version:?}");
    }
}

/// A server certificate the system does not trust: setup shows its fingerprint for the user to compare, and Cancel
/// leaves the form as it was (the fixture's `self-signed.example.invalid` answers as such a server).
#[test]
fn webdav_setup_shows_an_untrusted_certificate_for_confirmation() {
    let mut app = open();
    create(&mut app);
    app.press("role:button[Backup & Sync]").unwrap();
    app.press("role:button[Set Up]").unwrap();
    app.press("role:button[WebDAV]").unwrap();
    app.type_text("id:cloud-address", "https://self-signed.example.invalid/dav").unwrap();
    app.type_text("id:cloud-username", "fixture").unwrap();
    app.type_text("id:cloud-password", "synthetic").unwrap();
    app.press("role:button[Connect]").unwrap();
    app.wait_for("The server’s certificate can’t be verified").unwrap();
    let shown = app.screen().texts().iter().find(|t| t.len() == 95 && t.matches(':').count() == 31).map(|t| t.to_string());
    assert_eq!(shown.as_deref().map(|t| &t[..8]), Some("0B:30:55"), "the SHA-256 in upper-case hex pairs");
    app.press("role:button[Cancel] >> nth=0").unwrap();
    app.wait_for_gone("The server’s certificate can’t be verified", Duration::from_secs(5)).unwrap();
    // Only the card closed: the form keeps what was typed.
    assert_eq!(app.screen().input_text("id:cloud-address").unwrap(), Some("https://self-signed.example.invalid/dav"));
}
/// A fair password asks first; "Choose Stronger Password" is the alert's default button (the last regular action, which
/// takes Return), "Use Anyway" only a click away, and choosing the stronger one creates nothing.
#[test]
fn fair_password_alert_defaults_to_choosing_a_stronger_one() {
    use nsh_core::DialogButtonStyle;
    use safay_app::core::password::{PasswordStrength, evaluate};
    use std::sync::Mutex;
    let fair = ["Safay2026!", "Bluebird77!", "summer-cat9", "Orange-tree1"]
        .into_iter()
        .find(|p| evaluate(p) == PasswordStrength::Fair)
        .expect("a fair sample");
    let mut app = open();
    let shown = Arc::new(Mutex::new(Vec::new()));
    let seen = shown.clone();
    app.on_dialog(move |dialog| {
        *seen.lock().unwrap() = dialog.buttons.iter().map(|b| (b.text.clone(), b.style)).collect();
        nsh_testing::DialogAnswer::Press("Choose Stronger Password".into())
    });
    app.wait_for("Start fresh").unwrap();
    app.press("role:button[Start fresh]").unwrap();
    app.type_text("id:new-password", fair).unwrap();
    app.type_text("id:confirm-password", fair).unwrap();
    app.press("role:button[Create Vault]").unwrap();
    app.run_for(Duration::from_millis(300)).unwrap();
    let buttons = shown.lock().unwrap().clone();
    assert_eq!(
        buttons,
        vec![("Use Anyway".to_owned(), DialogButtonStyle::Default), ("Choose Stronger Password".to_owned(), DialogButtonStyle::Default),]
    );
    assert!(app.screen().texts().contains(&"Create Vault"), "nothing was created");
}
/// Showing and hiding the master password switches between two stacked fields (docs/ARCHITECTURE.md): what was typed before
/// stays, typing goes on in the field showing, and the whole password unlocks.
#[test]
fn revealing_the_master_password_keeps_what_was_typed() {
    let mut app = open();
    create(&mut app);
    app.press("role:button[Lock]").unwrap();
    app.wait_for("Safay is locked").unwrap();
    app.type_text("id:password", &PASSWORD[..6]).unwrap();
    app.press("role:button[Show]").unwrap();
    app.type_text("id:password-shown", &PASSWORD[6..]).unwrap();
    app.press("role:button[Hide]").unwrap();
    app.submit("id:password").unwrap();
    app.wait_for_within("No 2FA accounts yet", Duration::from_secs(20)).unwrap();
}

const THREE: &str = "otpauth://totp/Alpha:a?secret=JBSWY3DPEHPK3PXP&issuer=Alpha\notpauth://totp/Bravo:b?secret=JBSWY3DPEHPK3PXQ&issuer=Bravo\notpauth://totp/Charlie:c?secret=JBSWY3DPEHPK3PXR&issuer=Charlie";
fn order(app: &Headless) -> Vec<String> {
    app.screen().texts().into_iter().filter(|t| ["Alpha", "Bravo", "Charlie"].contains(t)).map(String::from).collect()
}
#[test]
fn cards_and_rows_reorder_by_dragging() {
    let mut app = open();
    create(&mut app);
    app.press("role:button[Add]").unwrap();
    app.press("role:button[Paste Links]").unwrap();
    app.type_text("id:import-text", THREE).unwrap();
    app.wait_for("Parsed").unwrap();
    app.press("role:button[Add 3 Accounts]").unwrap();
    app.wait_for("Charlie").unwrap();
    assert_eq!(order(&app), ["Alpha", "Bravo", "Charlie"]);
    // Cards: onto the next card to the right; a drag does not copy.
    app.drag("role:button[Copy code for Alpha]", point(px(250.0), px(0.0))).unwrap();
    assert_eq!(order(&app), ["Bravo", "Alpha", "Charlie"]);
    assert!(!app.screen().texts().contains(&"Copied"));
    // Released outside the list (over the toolbar): the order comes back.
    app.drag("role:button[Copy code for Charlie]", point(px(0.0), px(-400.0))).unwrap();
    assert_eq!(order(&app), ["Bravo", "Alpha", "Charlie"]);
    // Rows: dragged down two rows.
    app.press("role:button[List]").unwrap();
    app.drag("role:button[Copy code for Bravo]", point(px(0.0), px(120.0))).unwrap();
    assert_eq!(order(&app), ["Alpha", "Charlie", "Bravo"]);
    // While searching only the shown rows move; the hidden one keeps its place.
    app.type_text("id:search", "r").unwrap();
    app.wait_for_gone("Alpha", Duration::from_secs(5)).unwrap();
    app.drag("role:button[Copy code for Bravo]", point(px(0.0), px(-60.0))).unwrap();
    assert_eq!(order(&app), ["Bravo", "Charlie"]);
    app.clear("id:search").unwrap();
    app.wait_for("Alpha").unwrap();
    assert_eq!(order(&app), ["Alpha", "Bravo", "Charlie"]);
    // A click still copies.
    app.press("role:button[Copy code for Alpha]").unwrap();
    app.wait_for("Copied").unwrap();
}

fn categories(app: &Headless) -> Vec<String> {
    app.screen().texts().into_iter().filter(|t| ["Work", "Home", "Misc"].contains(t)).map(String::from).collect()
}
#[test]
fn categories_reorder_by_dragging() {
    let mut app = open();
    create(&mut app);
    paste(&mut app, SAMPLE);
    app.wait_for("Fixture").unwrap();
    for name in ["Work", "Home", "Misc"] {
        app.press("role:button[New Category]").unwrap();
        app.type_text("id:category-name", name).unwrap();
        app.submit("id:category-name").unwrap();
        app.wait_for(&format!("No accounts in “{name}” yet")).unwrap();
    }
    app.press("role:button[All]").unwrap();
    app.wait_for("Fixture").unwrap();
    assert_eq!(categories(&app), ["Work", "Home", "Misc"]);
    // Down past two rows; the drag does not open the category.
    app.drag("role:button[Work]", point(px(0.0), px(64.0))).unwrap();
    assert_eq!(categories(&app), ["Home", "Misc", "Work"]);
    assert!(app.screen().texts().contains(&"Fixture"));
    // Dropped on "All" (outside the categories' block): the order comes back.
    app.drag("role:button[Misc]", point(px(0.0), px(-120.0))).unwrap();
    assert_eq!(categories(&app), ["Home", "Misc", "Work"]);
    // Up one row.
    app.drag("role:button[Misc]", point(px(0.0), px(-32.0))).unwrap();
    assert_eq!(categories(&app), ["Misc", "Home", "Work"]);
}
#[test]
fn qr_sheet_opens_only_after_verification() {
    let mut app = open();
    create(&mut app);
    paste(&mut app, SAMPLE);
    app.wait_for("Fixture").unwrap();
    app.press("role:button[Details]").unwrap();
    app.wait_for("Account Details").unwrap();
    // Cancelled: no QR sheet, back on the details.
    app.wait_for("role:button[Show QR Code]").unwrap();
    app.press("role:button[Show QR Code]").unwrap();
    app.wait_for("Verify It’s You").unwrap();
    assert!(!app.screen().texts().contains(&"Copy URI"));
    app.press_key("Escape").unwrap();
    app.wait_for_gone("Verify It’s You", Duration::from_secs(5)).unwrap();
    assert!(app.screen().texts().contains(&"Account Details"));
    assert!(!app.screen().texts().contains(&"Copy URI"));
    // A wrong password keeps the sheet closed; the right one opens it with the code.
    app.press("role:button[Show QR Code]").unwrap();
    app.type_text("id:auth-password", "wrong password here").unwrap();
    app.press("role:button[Continue]").unwrap();
    app.wait_for_within("Incorrect master password", Duration::from_secs(20)).unwrap();
    assert!(!app.screen().texts().contains(&"Copy URI"));
    app.clear("id:auth-password").unwrap();
    app.type_text("id:auth-password", PASSWORD).unwrap();
    app.press("role:button[Continue]").unwrap();
    app.wait_for_within("Copy URI", Duration::from_secs(20)).unwrap();
    assert!(!app.screen().texts().contains(&"Verify It’s You"));
}
/// Pasted lines that fail say why in words (not the core's key), and their list is six rows high with the rest scrolling
/// inside it; the sheet holds no more than it shows at the window's minimum height. (The macOS bug this round fixed, the
/// text field's native measure of its whole text widening the sheet's scroll range, does not show headless: there the
/// field measures as laid out. It was checked on the packaged app.)
#[test]
fn pasted_lines_scroll_inside_their_list() {
    let mut config = EngineConfig::new(Root::of::<safay::FixtureApp>());
    config.viewport = Size::new(px(980.0), px(620.0));
    config.storage = Some(MemoryStorage::new(std::iter::empty::<(String, String)>()));
    config.clipboard = Some(Arc::new(MemoryClipboard::default()));
    config.clock = Clock::manual(SystemTime::UNIX_EPOCH);
    config.network = None;
    config.data_dir = None;
    config.cache = nsh_engine::CacheOptions::off();
    let mut app = Headless::start(config).unwrap();
    create(&mut app);
    app.press("role:button[Add]").unwrap();
    app.press("role:button[Paste Links]").unwrap();
    let text = (0..20).map(|i| format!("not a link {i}")).collect::<Vec<_>>().join("\n");
    app.type_text("id:import-text", &text).unwrap();
    app.wait_for("Not an otpauth:// URL").unwrap();
    assert!(!app.screen().texts().contains(&"otp.err.not_otpauth"));
    let scrolls: Vec<_> = app.screen().layout_nodes().into_iter().filter(|n| n.scroll.is_some()).collect();
    let list = scrolls.last().expect("the parsed list");
    assert_eq!(list.bounds.size.height, px(6.0 * 36.0 + 5.0));
    assert!(list.scroll.as_ref().unwrap().content_size.unwrap().height > list.bounds.size.height);
    // The sheet's scroll view (the one around the list) holds no more than it shows.
    let sheet = scrolls.iter().rev().nth(1).expect("the sheet's scroll view");
    let content = sheet.scroll.as_ref().unwrap().content_size.unwrap().height;
    assert!(content <= sheet.bounds.size.height + px(1.0), "{content:?} > {:?}", sheet.bounds.size.height);
}
