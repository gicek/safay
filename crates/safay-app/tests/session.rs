use safay_app::{
    controller::Controller,
    core::{password, vault},
    session::{Clock, Draft, Error, Session},
    storage::{MemoryRepository, Repository},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const PASSWORD: &str = "P8!nQ2@vT7#sL4$m";
const OTHER: &str = "w6*R9!jB3$qH7&zF";
const SECRET: &str = "JBSWY3DPEHPK3PXP";
#[derive(Default)]
struct Time(AtomicU64);
impl Time {
    fn advance(&self, n: u64) {
        self.0.fetch_add(n, Ordering::SeqCst);
    }
}
impl Clock for Time {
    fn wall(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1234567890 + self.0.load(Ordering::SeqCst))
    }
    fn elapsed(&self) -> Duration {
        Duration::from_secs(self.0.load(Ordering::SeqCst))
    }
}
fn setup() -> (Session, MemoryRepository, Arc<Time>, Arc<AtomicU64>) {
    let repo = MemoryRepository::default();
    let clock = Arc::new(Time::default());
    let gate = Arc::new(AtomicU64::new(0));
    (Session::new(Box::new(repo.clone()), clock.clone(), gate.clone()).unwrap(), repo, clock, gate)
}
fn draft() -> Draft {
    Draft {
        issuer: "RFC sample".into(),
        username: "fixture@example.invalid".into(),
        secret: SECRET.into(),
        website: String::new(),
        ..Default::default()
    }
}
#[test]
fn encrypted_roundtrip_and_failed_write_do_not_publish_changes() {
    let (mut s, repo, _, _) = setup();
    s.create(PASSWORD, false).unwrap();
    s.save_account(&draft()).unwrap();
    let saved = repo.read().unwrap().unwrap();
    assert!(!String::from_utf8_lossy(&saved).contains(SECRET));
    let p = s.projection("", None).unwrap();
    assert_eq!(p.rows.len(), 1);
    assert_eq!(p.account_count, 1);
    assert_eq!(s.projection("no result", None).unwrap().account_count, 1);
    assert!(!p.rows[0].code.is_empty());
    repo.0.lock().unwrap().reject_writes = true;
    let mut edit = draft();
    edit.id = Some(p.rows[0].id);
    edit.issuer = "Should not save".into();
    assert_eq!(s.save_account(&edit), Err(Error::Storage));
    assert_eq!(s.projection("", None).unwrap().rows[0].issuer, "RFC sample");
    assert_eq!(repo.read().unwrap().unwrap(), saved);
    repo.0.lock().unwrap().reject_writes = false;
    s.lock();
    assert!(s.projection("", None).unwrap().rows.is_empty());
    assert_eq!(s.projection("", None).unwrap().account_count, 0);
    s.unlock(PASSWORD).unwrap();
    assert_eq!(s.projection("", None).unwrap().rows.len(), 1);
}
#[test]
fn wrong_password_delay_persists_and_corruption_does_not_count() {
    let (mut s, repo, time, _) = setup();
    s.create(PASSWORD, false).unwrap();
    s.lock();
    for _ in 0..5 {
        assert_eq!(s.unlock("incorrect"), Err(Error::Password));
    }
    assert_eq!(repo.failures().unwrap(), 5);
    assert_eq!(s.unlock(PASSWORD), Err(Error::Wait));
    time.advance(31);
    s.unlock(PASSWORD).unwrap();
    assert_eq!(repo.failures().unwrap(), 0);
    s.lock();
    repo.0.lock().unwrap().main = Some(b"corrupt".to_vec());
    assert_eq!(s.unlock(PASSWORD), Err(Error::Corrupt));
    assert_eq!(repo.failures().unwrap(), 0);
}
#[test]
fn restart_reapplies_full_delay() {
    let (mut s, repo, time, gate) = setup();
    s.create(PASSWORD, false).unwrap();
    s.lock();
    for _ in 0..5 {
        let _ = s.unlock("incorrect");
    }
    time.advance(120);
    let mut restarted = Session::new(Box::new(repo), time.clone(), gate).unwrap();
    assert_eq!(restarted.unlock(PASSWORD), Err(Error::Wait));
    time.advance(31);
    restarted.unlock(PASSWORD).unwrap();
}
#[test]
fn lock_epoch_and_idle_timer_purge_projection() {
    let (mut s, _, clock, gate) = setup();
    s.create(PASSWORD, false).unwrap();
    s.save_account(&draft()).unwrap();
    clock.advance(299);
    assert!(s.projection("", None).unwrap().unlocked);
    s.activity();
    clock.advance(299);
    assert!(s.projection("", None).unwrap().unlocked);
    gate.fetch_add(1, Ordering::SeqCst);
    assert!(!s.projection("", None).unwrap().unlocked);
    s.unlock(PASSWORD).unwrap();
    clock.advance(301);
    assert!(!s.projection("", None).unwrap().unlocked);
}
#[test]
fn stale_controller_command_is_rejected() {
    let c = Controller::new(Box::new(MemoryRepository::default()), Arc::new(Time::default())).unwrap();
    let old = c.epoch();
    c.request_lock();
    assert_eq!(c.run(old, |s| s.create(PASSWORD, false)), Err(Error::Cancelled));
}
#[test]
fn change_password_snapshot_recovery_and_export_compatibility() {
    let (mut s, repo, _, _) = setup();
    s.create(PASSWORD, false).unwrap();
    s.save_account(&draft()).unwrap();
    let exported = s.export(PASSWORD, OTHER, false).unwrap();
    assert_eq!(vault::import_export(&exported, OTHER).unwrap().accounts.len(), 1);
    s.change_password(PASSWORD, OTHER, false).unwrap();
    s.lock();
    assert_eq!(s.unlock(PASSWORD), Err(Error::Password));
    s.unlock(OTHER).unwrap();
    repo.0.lock().unwrap().main = Some(b"damaged".to_vec());
    s.lock();
    s.restore_snapshot(PASSWORD).unwrap();
    assert_eq!(s.projection("", None).unwrap().rows.len(), 1);
}
#[test]
fn secret_reveal_requires_current_password_and_lock_refuses_operations() {
    let (mut s, _, _, _) = setup();
    s.create(PASSWORD, false).unwrap();
    s.save_account(&draft()).unwrap();
    let id = s.projection("", None).unwrap().rows[0].id;
    assert!(matches!(s.reveal(id, "incorrect", false), Err(Error::Password)));
    assert_eq!(&**s.reveal(id, PASSWORD, false).unwrap(), SECRET);
    let edit = s.edit_draft(id).unwrap();
    assert!(edit.secret.is_empty());
    s.lock();
    assert_eq!(s.copy_code(id).err(), Some(Error::Locked));
}
#[test]
fn importing_and_categories_preserve_shared_rules() {
    let (mut s, _, _, _) = setup();
    s.create(PASSWORD, false).unwrap();
    let uri = format!("otpauth://totp/Fixture:test?secret={SECRET}&issuer=Fixture");
    let other = format!("otpauth://totp/Other:test?secret={SECRET}&issuer=Other");
    assert_eq!(s.known_accounts(&format!("{uri}\n{other}")), Ok(vec![false, false]));
    assert_eq!(s.import_text(&uri), Ok(1));
    assert_eq!(s.import_text(&uri), Err(Error::Duplicate));
    // Matched by the import's rule (issuer case-insensitive), and nothing is added by asking.
    let upper = format!("otpauth://totp/FIXTURE:test?secret={SECRET}&issuer=FIXTURE");
    assert_eq!(s.known_accounts(&format!("{upper}\nnot a link\n{other}")), Ok(vec![true, false]));
    assert_eq!(s.projection("", None).unwrap().rows.len(), 1);
    s.category(None, "Personal").unwrap();
    assert_eq!(s.category(None, " personal "), Err(Error::Category));
    let p = s.projection("", None).unwrap();
    let cat = p.categories[0].id;
    let account = p.rows[0].id;
    s.membership(cat, account, true).unwrap();
    assert_eq!(s.projection("", Some(cat)).unwrap().rows.len(), 1);
    s.delete_category(cat).unwrap();
    assert_eq!(s.projection("", None).unwrap().rows.len(), 1);
}
#[test]
fn weak_password_cannot_create_a_vault() {
    let (mut s, repo, _, _) = setup();
    assert_eq!(s.create("password", true), Err(Error::Weak));
    assert!(repo.read().unwrap().is_none());
    assert!(password::is_acceptable_master_password(PASSWORD));
}
#[test]
fn external_changes_cannot_be_overwritten() {
    let (mut s, repo, _, _) = setup();
    s.create(PASSWORD, false).unwrap();
    repo.0.lock().unwrap().main = Some(b"external".to_vec());
    assert_eq!(s.save_account(&draft()), Err(Error::Changed));
    assert_eq!(repo.read().unwrap().unwrap(), b"external");
}

#[test]
fn oversized_main_can_be_recovered_without_reading_it() {
    use safay_app::storage::{DiskRepository, MAX_FILE_BYTES};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault.safayvault");
    let (data, _) = vault::create(PASSWORD, &vault::VaultContents::default(), UNIX_EPOCH).unwrap();
    std::fs::write(directory.path().join("last-good.safayvault"), &data).unwrap();
    std::fs::File::create(&path).unwrap().set_len(MAX_FILE_BYTES + 1).unwrap();
    let mut s = Session::new(
        Box::new(DiskRepository::open(directory.path().to_path_buf()).unwrap()),
        Arc::new(Time::default()),
        Arc::new(AtomicU64::new(0)),
    )
    .unwrap();
    assert!(s.projection("", None).unwrap().has_snapshot);
    assert_eq!(s.unlock(PASSWORD), Err(Error::TooLarge));
    s.restore_snapshot(PASSWORD).unwrap();
    assert!(s.projection("", None).unwrap().unlocked);
    assert_eq!(std::fs::read(path).unwrap(), data);
}

#[test]
fn stale_clipboard_requests_are_refused_and_memory_expiry_is_controlled() {
    use safay_app::{
        Zeroizing,
        clipboard::{MemoryClipboard, SecretClipboard},
    };
    let time = Arc::new(Time::default());
    let c = Controller::new(Box::new(MemoryRepository::default()), time.clone()).unwrap();
    let clipboard = MemoryClipboard::new(time.clone());
    let epoch = c.epoch();
    clipboard.copy(Zeroizing::new("public-code".into()), Duration::from_secs(30), c.clone(), epoch).unwrap();
    assert_eq!(clipboard.text().as_deref().map(|v| v.as_str()), Some("public-code"));
    time.advance(31);
    assert!(clipboard.text().is_none());
    c.request_lock();
    assert_eq!(clipboard.copy(Zeroizing::new("late-code".into()), Duration::from_secs(30), c, epoch), Err(Error::Cancelled));
    assert!(clipboard.text().is_none());
}

#[test]
fn preferences_survive_restart_without_sensitive_fields() {
    use safay_app::preferences::{Appearance, Language};
    let (mut s, repo, time, gate) = setup();
    s.create(PASSWORD, false).unwrap();
    let mut p = s.preferences();
    p.language = Language::Hans;
    p.appearance = Appearance::Dark;
    p.auto_lock_minutes = 1;
    p.cards = false;
    s.set_preferences(p).unwrap();
    let mut reopened = Session::new(Box::new(repo), time.clone(), gate).unwrap();
    assert_eq!(reopened.preferences(), p);
    reopened.unlock(PASSWORD).unwrap();
    time.advance(61);
    assert!(!reopened.projection("", None).unwrap().unlocked);
    let json = serde_json::to_string(&p).unwrap();
    assert!(!json.contains(PASSWORD));
    assert!(!json.contains(SECRET));
}
#[test]
fn first_launch_restore_requires_authenticated_export_and_independent_password() {
    let (mut source, _, _, _) = setup();
    source.create(PASSWORD, false).unwrap();
    source.save_account(&draft()).unwrap();
    let data = source.export(PASSWORD, OTHER, false).unwrap();
    let (mut target, repo, _, _) = setup();
    assert_eq!(target.restore_export(&data, "wrong", PASSWORD, false), Err(Error::FilePassword));
    assert!(repo.read().unwrap().is_none());
    target.restore_export(&data, OTHER, PASSWORD, false).unwrap();
    assert_eq!(target.projection("", None).unwrap().rows.len(), 1);
    target.lock();
    target.unlock(PASSWORD).unwrap();
    assert_eq!(target.restore_export(&data, OTHER, PASSWORD, false), Err(Error::Exists));
}
#[test]
fn restoring_an_export_checks_its_password_before_the_new_master_password() {
    const FAIR: &str = "Safay2026!";
    let (mut source, _, _, _) = setup();
    source.create(PASSWORD, false).unwrap();
    source.save_account(&draft()).unwrap();
    let data = source.export(PASSWORD, FAIR, true).unwrap();
    let (mut target, repo, _, _) = setup();
    // A wrong export password is reported as such, not as the strength of the master password it would set.
    assert_eq!(target.restore_export(&data, "wrong", FAIR, false), Err(Error::FilePassword));
    assert_eq!(target.restore_export(&data, "wrong", "short", false), Err(Error::FilePassword));
    // The export opens: a fair master password asks for confirmation, a weak one is refused, and neither writes a vault.
    assert_eq!(target.restore_export(&data, FAIR, FAIR, false), Err(Error::Fair));
    assert_eq!(target.restore_export(&data, FAIR, "short", false), Err(Error::Weak));
    assert!(repo.read().unwrap().is_none());
    target.restore_export(&data, FAIR, FAIR, true).unwrap();
    target.lock();
    target.unlock(FAIR).unwrap();
}
/// An import says what is wrong with the file: its password missing or wrong, the file unreadable, or no account in it,
/// not the master password nor a generic "no supported accounts". Synthetic golden Authenticator files and a Safay export.
#[test]
fn import_errors_tell_the_file_apart() {
    let golden: serde_json::Value = serde_json::from_str(include_str!("../../../golden/authenticator.json")).unwrap();
    let case = |name: &str| {
        let c = golden["imports"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap();
        (c["file"].as_str().unwrap().as_bytes().to_vec(), c["password"].as_str().unwrap().to_owned())
    };
    let (mut source, _, _, _) = setup();
    source.create(PASSWORD, false).unwrap();
    source.save_account(&draft()).unwrap();
    let export = source.export(PASSWORD, OTHER, false).unwrap();
    let (mut s, _, _, _) = setup();
    s.create(PASSWORD, false).unwrap();
    let import = |s: &mut Session, (file, password): (Vec<u8>, String)| s.import_file(&file, &password);
    assert_eq!(import(&mut s, case("encrypted-no-password")), Err(Error::FilePasswordRequired));
    assert_eq!(import(&mut s, case("encrypted-wrong-password")), Err(Error::FilePassword));
    assert_eq!(import(&mut s, case("not-json")), Err(Error::ImportFile));
    assert_eq!(import(&mut s, case("only-unsupported")), Err(Error::ImportEmpty));
    assert_eq!(s.import_file(&export, ""), Err(Error::FilePasswordRequired));
    assert_eq!(s.import_file(&export, "wrong"), Err(Error::FilePassword));
    assert_eq!(s.import_file(&export, OTHER), Ok(1));
    // The vault stays unlocked: a wrong file password is not a failed master password attempt.
    assert!(s.projection("", None).unwrap().unlocked);
}
/// Launchers (crate::launcher): nothing is indexed while the interface is off; once on, the index follows the unlocked
/// vault and survives locking (searching a locked Safay); turning it off or resetting forgets it. A copy needs proof
/// every time: a Touch ID-released key (which also unlocks) or the master password, never neither.
#[test]
fn launchers_search_a_locked_vault_and_copy_only_with_proof() {
    use safay_app::{Uuid, preferences::Preferences};
    let (mut s, _, _, _) = setup();
    s.create(PASSWORD, false).unwrap();
    s.save_account(&draft()).unwrap();
    let launcher = s.launcher();
    assert!(!s.projection("", None).unwrap().preferences.launcher);
    assert!(!launcher.find("").ready, "off by default: nothing indexed");
    s.set_preferences(Preferences { launcher: true, ..s.preferences() }).unwrap();
    let found = launcher.find("rfc");
    assert!(found.enabled && found.ready);
    let id = found.accounts[0].id;
    assert_eq!(found.accounts[0].username, "fi***@example.invalid");
    let key = s.key_material().unwrap();
    s.lock();
    assert_eq!(launcher.find("fixture").accounts.len(), 1, "a locked vault stays searchable");
    assert_eq!(launcher.issuer(id).as_deref(), Some("RFC sample"));
    // Locked: no code without proof; a wrong key neither unlocks nor copies; the right key unlocks and copies.
    assert_eq!(s.launcher_code(id).map(|_| ()), Err(Error::Locked));
    assert_eq!(s.launcher_code_with_key(id, &[0u8; 32]).map(|_| ()), Err(Error::StaleKey));
    assert!(!s.projection("", None).unwrap().unlocked);
    let (code, ttl, issuer) = s.launcher_code_with_key(id, &key).unwrap();
    assert_eq!((code.len(), ttl.as_secs(), issuer.as_str()), (6, 30, "RFC sample"));
    // Unlocked: still proof every time.
    assert_eq!(s.launcher_code_with_key(id, &[0u8; 32]).map(|_| ()), Err(Error::StaleKey));
    assert_eq!(s.launcher_code_with_password(id, "wrong").map(|_| ()), Err(Error::Password));
    s.launcher_code_with_password(id, PASSWORD).unwrap();
    assert_eq!(s.launcher_code_with_key(Uuid::from_u128(9), &key).map(|_| ()), Err(Error::NotFound));
    // Off forgets; reset turns it off.
    s.set_preferences(Preferences { launcher: false, ..s.preferences() }).unwrap();
    assert!(!launcher.find("").ready && launcher.issuer(id).is_none());
    s.set_preferences(Preferences { launcher: true, ..s.preferences() }).unwrap();
    assert!(launcher.find("").ready);
    s.reset(PASSWORD).unwrap();
    assert!(!launcher.find("").enabled && !launcher.find("").ready);
}
#[test]
fn failed_preference_write_leaves_previous_policy() {
    let (mut s, repo, _, _) = setup();
    let before = s.preferences();
    repo.0.lock().unwrap().reject_writes = true;
    assert_eq!(s.set_auto_lock(1), Err(Error::Storage));
    assert_eq!(s.preferences(), before);
}

#[test]
fn selected_export_keeps_only_chosen_accounts_and_their_categories() {
    let (mut s, _, _, _) = setup();
    s.create(PASSWORD, false).unwrap();
    s.save_account(&draft()).unwrap();
    let mut other = draft();
    other.issuer = "Other".into();
    other.secret = "GEZDGNBVGY3TQOJQ".into();
    s.save_account(&other).unwrap();
    s.category(None, "Work").unwrap();
    let p = s.projection("", None).unwrap();
    let work = p.categories[0].id;
    let (keep, drop) = (p.rows[0].id, p.rows[1].id);
    s.membership(work, keep, true).unwrap();
    s.membership(work, drop, true).unwrap();
    assert_eq!(s.export_selected(PASSWORD, Some(&[]), OTHER, false), Err(Error::Fields));
    // A weak export password is refused before the master password is spent on a check.
    assert_eq!(s.export_selected("wrong", Some(&[keep]), "short", false), Err(Error::Weak));
    assert_eq!(s.export_selected("wrong", Some(&[keep]), OTHER, false), Err(Error::Password));
    let data = s.export_selected(PASSWORD, Some(&[keep]), OTHER, false).unwrap();
    let contents = vault::import_export(&data, OTHER).unwrap();
    assert_eq!(contents.accounts.len(), 1);
    assert_eq!(contents.accounts[0].id, keep);
    assert_eq!(contents.categories[0].account_ids, vec![keep]);
}
#[test]
fn metadata_edits_keep_the_secret_and_details_never_carry_it() {
    let (mut s, _, _, _) = setup();
    s.create(PASSWORD, false).unwrap();
    s.save_account(&draft()).unwrap();
    let id = s.projection("", None).unwrap().rows[0].id;
    let before = s.projection("", None).unwrap().rows[0].code.clone();
    s.update_metadata(id, "Renamed", "new@example.invalid", "example.com").unwrap();
    let d = s.detail(id).unwrap();
    assert_eq!((d.issuer.as_str(), d.username.as_str(), d.website.as_str()), ("Renamed", "new@example.invalid", "example.com"));
    assert!(!d.imported);
    assert_eq!(s.projection("", None).unwrap().rows[0].code, before);
    assert_eq!(s.update_metadata(id, "Renamed", "x", "not a website"), Err(Error::Fields));
    // Search also matches the website.
    assert_eq!(s.projection("example.com", None).unwrap().rows.len(), 1);
}
#[test]
fn turning_off_the_failure_delay_needs_the_master_password() {
    let (mut s, _, _, _) = setup();
    s.create(PASSWORD, false).unwrap();
    let mut p = s.preferences();
    p.failure_delay = false;
    assert_eq!(s.set_preferences(p), Err(Error::Locked));
    assert_eq!(s.set_failure_delay(false, "wrong"), Err(Error::Password));
    assert!(s.preferences().failure_delay);
    s.set_failure_delay(false, PASSWORD).unwrap();
    assert!(!s.preferences().failure_delay);
    s.set_failure_delay(true, "").unwrap();
    assert!(s.preferences().failure_delay);
}
#[test]
fn preferences_never_hide_both_the_tray_and_the_dock() {
    use safay_app::preferences::{DockIcon, Preferences};
    let p = Preferences { tray_icon: false, dock_icon: DockIcon::Never, background_launch: true, ..Default::default() }.validated();
    assert_eq!(p.dock_icon, DockIcon::MainWindow);
    assert!(!p.background_launch);
}
#[test]
fn shortcuts_need_a_command_option_or_control_key_and_must_differ() {
    use safay_app::preferences::{Hotkey, Preferences};
    let p = Preferences { hotkey_panel: Some(Hotkey { key: 6, modifiers: 0x200 }), ..Default::default() }.validated();
    assert_eq!(p.hotkey_panel, None);
    let same = Some(Hotkey { key: 6, modifiers: 0x900 });
    let p = Preferences { hotkey_panel: same, hotkey_main: same, ..Default::default() }.validated();
    assert_eq!((p.hotkey_panel, p.hotkey_main), (same, None));
    assert_eq!(Preferences::default().hotkey_panel, Some(Hotkey { key: 6, modifiers: 0x900 }));
}
#[test]
fn a_released_key_unlocks_and_proves_identity_once_and_goes_stale_with_a_new_password() {
    let (mut s, _, _, _) = setup();
    s.create(PASSWORD, false).unwrap();
    s.save_account(&draft()).unwrap();
    let id = s.projection("", None).unwrap().rows[0].id;
    let key = s.key_material().unwrap();
    s.set_biometric(Some(b"wrapped")).unwrap();
    assert!(s.projection("", None).unwrap().biometric);
    s.lock();
    s.unlock_with_key(&key).unwrap();
    assert!(s.projection("", None).unwrap().unlocked);
    // Proof: a wrong key is refused; a right one lets exactly one identity check pass without the password.
    assert_eq!(s.prove_key(&[0u8; 32]), Err(Error::StaleKey));
    s.prove_key(&key).unwrap();
    assert_eq!(s.reveal(id, "", false).unwrap().as_str(), SECRET);
    assert_eq!(s.reveal(id, "", false), Err(Error::Password));
    // A new master password makes the wrapped key stale and deletes it.
    s.change_password(PASSWORD, OTHER, false).unwrap();
    assert!(!s.projection("", None).unwrap().biometric);
    s.lock();
    assert_eq!(s.unlock_with_key(&key), Err(Error::StaleKey));
    assert!(!s.projection("", None).unwrap().unlocked);
}
#[test]
fn deleted_accounts_wait_in_recently_deleted_then_go() {
    let (mut s, _, time, _) = setup();
    s.create(PASSWORD, false).unwrap();
    s.save_account(&draft()).unwrap();
    let mut other = draft();
    other.issuer = "Other".into();
    other.secret = "GEZDGNBVGY3TQOJQ".into();
    s.save_account(&other).unwrap();
    s.category(None, "Work").unwrap();
    let p = s.projection("", None).unwrap();
    let (work, sample) = (p.categories[0].id, p.rows.iter().find(|r| r.issuer == "RFC sample").unwrap().id);
    s.membership(work, sample, true).unwrap();

    // Deleting moves it out of every list, count and duplicate check, keeping its category for recovery.
    s.delete_account(sample).unwrap();
    let p = s.projection("", None).unwrap();
    assert_eq!((p.account_count, p.rows.len(), p.deleted_count), (1, 1, 1));
    assert_eq!((p.deleted[0].issuer.as_str(), p.deleted[0].days_left), ("RFC sample", 30));
    assert_eq!(p.categories[0].count, 0);
    assert!(s.projection("", Some(work)).unwrap().rows.is_empty());
    assert_eq!(s.projection("rfc", None).unwrap().deleted.len(), 1);
    assert_eq!(s.projection("nothing", None).unwrap().deleted.len(), 0);
    assert_eq!(s.copy_code(sample).unwrap_err(), Error::NotFound);
    assert_eq!(s.delete_account(sample), Err(Error::NotFound));
    assert_eq!(s.choices().unwrap().len(), 1);

    // It survives locking, and comes back where it was.
    s.lock();
    s.unlock(PASSWORD).unwrap();
    assert_eq!(s.projection("", None).unwrap().deleted_count, 1);
    s.recover_account(sample).unwrap();
    let p = s.projection("", None).unwrap();
    assert_eq!((p.account_count, p.deleted_count, p.categories[0].count), (2, 0, 1));

    // Removing it for good, one or all.
    s.delete_account(sample).unwrap();
    s.remove_deleted(Some(&[sample])).unwrap();
    assert_eq!(s.projection("", None).unwrap().deleted_count, 0);
    assert_eq!(s.remove_deleted(None), Err(Error::NotFound));

    // A re-added account is new while the deleted one waits; after thirty days the deleted one goes on its own.
    s.save_account(&draft()).unwrap();
    let again = s.projection("", None).unwrap().rows.iter().find(|r| r.issuer == "RFC sample").unwrap().id;
    s.delete_account(again).unwrap();
    s.save_account(&draft()).unwrap();
    assert_eq!(s.projection("", None).unwrap().account_count, 2);
    assert!(!s.purge_deleted().unwrap());
    time.advance(30 * 24 * 60 * 60);
    // Thirty days later auto-lock has long passed.
    s.lock();
    s.unlock(PASSWORD).unwrap();
    assert!(s.purge_deleted().unwrap());
    let p = s.projection("", None).unwrap();
    assert_eq!((p.account_count, p.deleted_count), (2, 0));
}
