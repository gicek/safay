//! Per-window presentation state. Only Controller owns an unlocked vault.
use crate::{
    i18n::{self, Language},
    platform,
};
use nsh_core::prelude::*;
use safay_app::{
    Uuid, Zeroizing,
    controller::Controller,
    core::authenticator::FileKind,
    session::{Choice, Detail, Draft, Error, Projection, Session},
};
use std::{collections::BTreeSet, sync::Arc, time::Duration};

/// Which window this store drives. The windows render from the app's one NSH engine; each has a `Store` of its own
/// over the one Controller, and they learn of each other's changes through its revision.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Main,
    Panel,
    Settings,
}
/// A modal over the main window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sheet {
    Add,
    Detail(Uuid),
    Qr(Uuid),
    ExportImport,
    ChangePassword,
    /// Windows and Linux show settings in the main window; macOS opens its own Settings window.
    Settings,
    Restore,
    Backup,
}
/// Operations that require the master password again, prompted in a small dialog.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Auth {
    Secret(Uuid),
    Qr(Uuid),
    Export,
    Reset,
    DisableDelay,
    /// A launcher asked to copy this account's code and Touch ID did not prove it (`crate::launcher`).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Copy(Uuid),
}
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Stage {
    #[default]
    Welcome,
    Create,
    Restore,
}
/// The Backup & Sync sheet's page.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum BackupPage {
    #[default]
    Overview,
    Choose,
    Conflict,
    Restore,
    /// Connecting a cloud storage location.
    Setup(safay_app::cloud::Provider),
}
/// The cloud setup form. The password is cleared once the connection is stored or the page is left.
#[derive(Clone, Default, PartialEq)]
pub struct CloudForm {
    pub address: String,
    pub username: String,
    pub password: Zeroizing<String>,
    /// The user accepted sending the password over plain HTTP.
    pub insecure: bool,
    pub needs_consent: bool,
    /// The system does not trust the server's certificate: its SHA-256, for the user to compare (SECURITY.md).
    pub untrusted: Option<[u8; 32]>,
    /// The certificate the user confirmed: the connection trusts only it from then on.
    pub trusted: Option<[u8; 32]>,
    pub working: bool,
    /// Waiting for the user to finish signing in in the browser (Nextcloud).
    pub waiting: bool,
    /// Connected; looking for a backup there (first-run restore), the third step.
    pub checking: bool,
    pub error: Option<SharedString>,
}
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum AddTab {
    #[default]
    Scan,
    Key,
    Text,
}
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsPage {
    #[default]
    General,
    Security,
    Shortcuts,
    Advanced,
    About,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Success,
    Error,
}
/// Footer status in a sheet, or a transient toast. Text is already localized.
#[derive(Clone, PartialEq, Eq)]
pub struct Status {
    pub tone: Tone,
    pub text: SharedString,
}
/// Inline sidebar editor for a new or renamed category.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct CategoryEdit {
    pub id: Option<Uuid>,
    pub name: String,
    pub error: Option<Error>,
    /// Account to add once a new category is created ("Add to › New Category").
    pub account: Option<Uuid>,
}
/// Validation marks kept apart from field values, so a blur never rewrites a field being typed in.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Marks {
    /// The new password field has been left at least once (problems show after blur).
    pub new_left: bool,
    /// The new password and confirmation fields have the keyboard focus (being edited).
    pub new_focused: bool,
    pub confirm_focused: bool,
    pub website_error: bool,
    pub secret_error: bool,
}
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Form {
    pub issuer: String,
    pub username: String,
    pub secret: Zeroizing<String>,
    pub website: String,
    pub website_touched: bool,
    pub advanced: bool,
    pub algorithm: u32,
    pub digits: u32,
    pub period: u32,
    pub password: Zeroizing<String>,
    pub new_password: Zeroizing<String>,
    pub confirmation: Zeroizing<String>,
    pub text: Zeroizing<String>,
    pub reveal: bool,
}
pub enum Payload {
    Silent,
    Saved,
    Imported(usize),
    Export(Vec<u8>, usize),
    Detail(Detail),
    Choices(Vec<Choice>),
    Secret(Zeroizing<String>),
    Qr(Zeroizing<String>, Zeroizing<String>),
    Copied(Uuid),
    /// A category was created or renamed; `true` selects a new, empty one.
    Created(bool),
    Restored,
    /// A launcher's copy went to the clipboard: the account's service name, for the notice.
    LauncherCopied(String),
    /// An export's password opened it, but the master password it would set is fair (`Error::Fair`, to confirm) or too
    /// weak (`Error::Weak`).
    Strength(Error),
    /// A QR code selected on screen from the shortcut or the panel: its first service, and how many accounts were new.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Captured(String, usize),
}
#[derive(Clone, Copy)]
pub struct Store {
    pub controller: Stored<Arc<Controller>>,
    clipboard: Stored<Arc<dyn safay_app::clipboard::SecretClipboard>>,
    pub surface: Surface,
    pub projection: Signal<Projection>,
    pub busy: Signal<bool>,
    pub ready: Signal<bool>,
    refreshing: Signal<bool>,
    /// A refresh asked for while another operation ran; it runs as soon as that one finishes.
    stale: Signal<bool>,
    seen_revision: Signal<u64>,
    serial: Signal<u64>,
    pub error: Signal<Option<Error>>,
    pub status: Signal<Option<Status>>,
    pub toast: Signal<Option<Status>>,
    pub query: Signal<String>,
    pub category: Signal<Option<Uuid>>,
    pub sheet: Signal<Option<Sheet>>,
    pub auth: Signal<Option<Auth>>,
    pub auth_password: Signal<Zeroizing<String>>,
    pub auth_error: Signal<Option<Error>>,
    /// The system's Touch ID prompt is up for `auth`: the master password dialog waits behind it and appears only if
    /// Touch ID does not verify (cancelled, failed, unavailable).
    pub auth_prompting: Signal<bool>,
    /// A launcher's copy waiting for the vault to be unlocked (with the master password), and since when.
    launcher_waiting: Signal<Option<(Uuid, std::time::Instant)>>,
    pub stage: Signal<Stage>,
    pub add_tab: Signal<AddTab>,
    pub import_tab: Signal<bool>,
    pub settings_page: Signal<SettingsPage>,
    pub backup_page: Signal<BackupPage>,
    pub cloud_form: Signal<CloudForm>,
    pub review: Signal<Option<safay_app::sync::Review>>,
    pub prefer_remote: Signal<BTreeSet<Uuid>>,
    pub syncing: Signal<bool>,
    /// The shortcut being recorded in Settings (0 panel, 1 main window).
    pub recording: Signal<Option<u8>>,
    pub login_item: Signal<Option<bool>>,
    /// The app updater's settings; `None` in builds that don't update themselves.
    pub updates: Signal<Option<platform::updates::Phase>>,
    /// Touch ID hardware is present and enrolled (checked when the window opens).
    pub biometrics: Signal<bool>,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    wrapping: Signal<bool>,
    pub login_error: Signal<Option<String>>,
    /// A folder chosen during first-run restore.
    pub restore_location: Signal<Option<safay_app::sync::Location>>,
    /// The cloud storage connection sheet was opened from the first-run restore page: connecting looks for a backup
    /// there instead of linking the location.
    pub cloud_restore: Signal<bool>,
    /// Cloud sign-ins made during first-run restore, reused when the service is chosen again (memory only).
    restore_sign_ins: Stored<safay_app::cloud::RestoreSignIns<CloudForm>>,
    pub form: Signal<Form>,
    pub marks: Signal<Marks>,
    /// zxcvbn runs only when the new password itself changes, never per keystroke elsewhere.
    pub strength: Memo<(u8, bool)>,
    pub detail: Signal<Option<Detail>>,
    pub choices: Signal<Vec<Choice>>,
    pub selected: Signal<BTreeSet<Uuid>>,
    pub choice_query: Signal<String>,
    pub language: Signal<Language>,
    pub cards: Signal<bool>,
    pub copied: Signal<Option<Uuid>>,
    pub editing: Signal<Option<CategoryEdit>>,
    pub sidebar: Signal<bool>,
    /// Revealed secret for the details sheet; cleared on lock, close or hide.
    pub revealed: Signal<Zeroizing<String>>,
    pub qr: Signal<Zeroizing<String>>,
    pub qr_uri: Signal<Zeroizing<String>>,
    pub file: Stored<Option<Zeroizing<Vec<u8>>>>,
    pub file_name: Signal<String>,
    /// What the chosen file is, read from its structure when it is chosen: Batch Import asks for a password only for an
    /// encrypted one and refuses an unsupported one at once.
    pub file_kind: Signal<FileKind>,
    /// Restoring an export: its password becomes the master password ("Use as Master Password").
    pub use_export_password: Signal<bool>,
    pub scanned: Signal<Zeroizing<String>>,
    pub highlight: Signal<Option<usize>>,
    pub search: Stored<ElementRef>,
    /// The owner of the window content the store was made in: tasks and timers outlive the view that started them (a Lock button
    /// unmounts as the vault locks, and must not take the refresh it started with it).
    owner: Stored<Owner>,
    pub fixture: bool,
    pub preferences: Signal<safay_app::preferences::Preferences>,
    loaded_preferences: Signal<bool>,
    system_language: Language,
}
impl Store {
    pub fn new(controller: Arc<Controller>, fixture: bool, surface: Surface) -> Self {
        let clipboard: Arc<dyn safay_app::clipboard::SecretClipboard> = if fixture {
            Arc::new(safay_app::clipboard::MemoryClipboard::new(Arc::new(crate::FixtureClock)))
        } else {
            Arc::new(platform::clipboard::NativeClipboard)
        };
        let tag = locale().to_string();
        let language = if tag.starts_with("zh") {
            if tag.contains("Hant") || tag.contains("TW") || tag.contains("HK") || tag.contains("MO") {
                Language::Hant
            } else {
                Language::Hans
            }
        } else {
            Language::En
        };
        let form = signal(Form::fresh());
        let new_password = computed(move || form.with(|f| f.new_password.clone()));
        let strength = computed(move || {
            use safay_app::core::password::{PasswordStrength as P, evaluate, is_acceptable_master_password};
            new_password.with(|p| {
                let level = match evaluate(p) {
                    P::Empty => 0,
                    P::Weak => 1,
                    P::Fair => 2,
                    P::Good => 3,
                    P::Strong => 4,
                };
                (if p.is_empty() { 0 } else { level.max(1) }, is_acceptable_master_password(p))
            })
        });
        Self {
            controller: stored(controller),
            clipboard: stored(clipboard),
            surface,
            projection: signal(Projection::default()),
            busy: signal(false),
            ready: signal(false),
            refreshing: signal(false),
            stale: signal(false),
            seen_revision: signal(0),
            serial: signal(0),
            error: signal(None),
            status: signal(None),
            toast: signal(None),
            query: signal(String::new()),
            category: signal(None),
            sheet: signal(None),
            auth: signal(None),
            auth_password: signal(Zeroizing::new(String::new())),
            auth_error: signal(None),
            auth_prompting: signal(false),
            launcher_waiting: signal(None),
            stage: signal(Stage::Welcome),
            add_tab: signal(AddTab::Scan),
            import_tab: signal(false),
            settings_page: signal(SettingsPage::General),
            backup_page: signal(BackupPage::Overview),
            cloud_form: signal(CloudForm::default()),
            review: signal(None),
            prefer_remote: signal(BTreeSet::new()),
            syncing: signal(false),
            recording: signal(None),
            login_item: signal(None),
            updates: signal(None),
            biometrics: signal(false),
            wrapping: signal(false),
            login_error: signal(None),
            restore_location: signal(None),
            cloud_restore: signal(false),
            restore_sign_ins: stored(Default::default()),
            form,
            strength,
            marks: signal(Marks::default()),
            detail: signal(None),
            choices: signal(Vec::new()),
            selected: signal(BTreeSet::new()),
            choice_query: signal(String::new()),
            language: signal(language),
            cards: signal(true),
            copied: signal(None),
            editing: signal(None),
            sidebar: signal(true),
            revealed: signal(Zeroizing::new(String::new())),
            qr: signal(Zeroizing::new(String::new())),
            qr_uri: signal(Zeroizing::new(String::new())),
            file: stored(None),
            file_name: signal(String::new()),
            file_kind: signal(FileKind::Plain),
            use_export_password: signal(true),
            scanned: signal(Zeroizing::new(String::new())),
            highlight: signal(None),
            search: stored(element_ref()),
            owner: stored(Owner::current()),
            fixture,
            preferences: signal(Default::default()),
            loaded_preferences: signal(false),
            system_language: language,
        }
    }
    /// Spawns under the window content's owner, never under the view that happened to start it.
    pub fn task(self, future: impl std::future::Future<Output = ()> + 'static) {
        self.owner.get().run(|| spawn(future));
    }
    pub fn label(self, key: &str) -> SharedString {
        i18n::tr(self.language.get(), key)
    }
    /// `{}` placeholders filled in order.
    pub fn fmt(self, key: &str, args: &[&str]) -> SharedString {
        let mut out = self.label(key).to_string();
        for arg in args {
            out = out.replacen("{}", arg, 1);
        }
        out.into()
    }
    /// Plural-aware count text: `<key>.one` / `<key>.other`.
    pub fn count(self, key: &str, n: usize) -> SharedString {
        let form = if n == 1 { format!("{key}.one") } else { format!("{key}.other") };
        self.fmt(&form, &[&n.to_string()])
    }
    pub fn unlocked(self) -> bool {
        self.projection.with(|p| p.unlocked)
    }
    fn reset_transient(self) {
        self.form.set(Form::fresh());
        self.marks.set(Marks::default());
        self.revealed.set(Zeroizing::new(String::new()));
        self.qr.set(Zeroizing::new(String::new()));
        self.qr_uri.set(Zeroizing::new(String::new()));
        self.scanned.set(Zeroizing::new(String::new()));
        self.file.replace(None);
        self.file_name.set(String::new());
        self.error.set(None);
        self.status.set(None);
    }
    pub fn open_sheet(self, sheet: Sheet) {
        if self.busy.get() {
            return;
        }
        self.reset_transient();
        self.detail.set(None);
        match sheet {
            Sheet::Detail(id) => self.dispatch(true, move |s| Ok(Payload::Detail(s.detail(id)?))),
            Sheet::ExportImport => {
                self.choice_query.set(String::new());
                self.dispatch(true, |s| Ok(Payload::Choices(s.choices()?)));
            }
            Sheet::Add => self.add_tab.set(AddTab::Scan),
            Sheet::Backup => {
                self.backup_page.set(if self.projection.with(|p| p.sync.status == safay_app::sync::Status::Conflict) {
                    BackupPage::Conflict
                } else {
                    BackupPage::Overview
                });
                self.review.set(None);
            }
            _ => {}
        }
        self.sheet.set(Some(sheet));
    }
    pub fn close_sheet(self) {
        // A nested QR sheet returns to the details sheet it came from.
        if let Some(Sheet::Qr(id)) = self.sheet.get()
            && self.detail.with(|d| d.as_ref().is_some_and(|d| d.id == id))
        {
            self.qr.set(Zeroizing::new(String::new()));
            self.qr_uri.set(Zeroizing::new(String::new()));
            self.sheet.set(Some(Sheet::Detail(id)));
            return;
        }
        if self.cloud_restore.get() {
            self.leave_cloud_setup();
            self.cloud_restore.set(false);
        }
        self.reset_transient();
        self.detail.set(None);
        self.auth.set(None);
        self.sheet.set(None);
        self.focus_search();
    }
    pub fn focus_search(self) {
        if self.surface != Surface::Settings {
            let r = self.search.get();
            set_timeout(Duration::from_millis(30), move || r.focus());
        }
    }
    pub fn show_toast(self, tone: Tone, text: SharedString) {
        let status = Status { tone, text };
        self.toast.set(Some(status.clone()));
        self.owner.get().run(|| {
            set_timeout(Duration::from_millis(2600), move || {
                if self.toast.get().as_ref() == Some(&status) {
                    self.toast.set(None);
                }
            })
        });
    }
    /// Results of capturing from the shortcut or the menu bar: a desktop notice near the top of the screen,
    /// or the window toast where there is no real desktop host.
    pub fn capture_notice(self, tone: Tone, text: SharedString) {
        #[cfg(target_os = "macos")]
        if crate::hosted() {
            crate::desktop::notice(tone, text.to_string());
            return;
        }
        self.show_toast(tone, text);
    }
    pub fn clear(self) {
        self.copied.set(None);
        self.editing.set(None);
        self.auth.set(None);
        self.auth_password.set(Zeroizing::new(String::new()));
        self.detail.set(None);
        self.choices.set(Vec::new());
        self.selected.set(BTreeSet::new());
        self.highlight.set(None);
        self.reset_transient();
    }
    pub fn lock(self) {
        self.controller.get().request_lock();
        self.after_lock();
    }
    fn after_lock(self) {
        self.clear();
        self.toast.set(None);
        self.sheet.set(None);
        self.projection.set(Projection { exists: true, ..Default::default() });
        self.category.set(None);
        self.query.set(String::new());
        self.refresh();
    }
    pub fn dispatch(self, user: bool, op: impl FnOnce(&mut Session) -> Result<Payload, Error> + Send + 'static) {
        if self.busy.get() || (!user && self.refreshing.get()) {
            if !user {
                self.stale.set(true);
            }
            return;
        }
        if user {
            self.busy.set(true);
            self.error.set(None);
        } else {
            self.refreshing.set(true);
        }
        self.serial.update(|v| *v = v.wrapping_add(1));
        let serial = self.serial.get();
        let controller = self.controller.get();
        let epoch = controller.epoch();
        let query = self.query.get();
        let category = self.category.get();
        let preference_writes = prefs::generation();
        self.owner.get().run(|| {
            spawn(async move {
                let job = blocking(move || {
                    controller.run(epoch, |session| {
                        let result = op(session);
                        let projection = session
                            .projection(&query, category)
                            .or_else(|e| if e == Error::NotFound { session.projection(&query, None) } else { Err(e) })?;
                        Ok((result, projection))
                    })
                });
                // The busy flag spans the real operation, including the KDF.
                let result = job.await;
                if user {
                    self.seen_revision.set(self.controller.get().revision() + 1);
                    self.controller.get().touch();
                }
                self.ready.set(true);
                if user {
                    self.busy.set(false);
                } else {
                    self.refreshing.set(false);
                }
                // A refresh asked for while this ran goes after this result is delivered, never before it.
                let catch_up = move || {
                    if self.stale.get() && !self.busy.get() && !self.refreshing.get() {
                        self.stale.set(false);
                        self.refresh();
                    }
                };
                if serial != self.serial.get() {
                    catch_up();
                    return;
                }
                if self.controller.get().epoch() != epoch {
                    self.clear();
                    self.sheet.set(None);
                    self.projection.set(Projection { exists: true, ..Default::default() });
                    catch_up();
                    return;
                }
                match result {
                    Ok(Ok((payload, projection))) => {
                        if !projection.unlocked && self.projection.with(|p| p.unlocked) {
                            self.clear();
                            self.sheet.set(None);
                        }
                        if self.category.get().is_some_and(|c| !projection.categories.iter().any(|x| x.id == c)) {
                            self.category.set(None);
                        }
                        // A projection read before a settings write finished would put the old choice back.
                        let current = prefs::settled(preference_writes);
                        if !self.loaded_preferences.get() || (current && projection.preferences != self.preferences.get()) {
                            self.apply_preferences(projection.preferences);
                        }
                        let due = projection.sync.due;
                        if projection.exists && !self.projection.with(|p| p.exists) {
                            self.release_restore_sign_ins(None);
                        }
                        self.projection.set(projection);
                        // Only the main window runs backups, so two windows never sync the same change twice.
                        if due && self.surface == Surface::Main && !self.syncing.get() {
                            self.sync_now(safay_app::sync::Source::Auto);
                        }
                        self.maybe_wrap_key();
                        self.queue_site_icons();
                        match payload {
                            Ok(p) => self.deliver(p),
                            Err(e) => self.fail(e),
                        }
                    }
                    Ok(Err(e)) => self.fail(e),
                    Err(_) => self.fail(Error::Storage),
                }
                catch_up();
            })
        });
    }
    fn fail(self, e: Error) {
        if self.auth.get().is_some() {
            self.auth_error.set(Some(e));
        } else if e == Error::Category && self.editing.with(|ed| ed.is_some()) {
            self.editing.update(|ed| {
                if let Some(ed) = ed {
                    ed.error = Some(e)
                }
            });
        } else {
            self.error.set(Some(e));
            // Errors from the account grid or the panel have no form to show them: use a toast.
            if self.sheet.get().is_none() && self.unlocked() && self.surface != Surface::Settings {
                self.show_toast(Tone::Error, self.label(crate::i18n::error_key(e)));
            }
        }
    }
    fn deliver(self, payload: Payload) {
        match payload {
            Payload::Silent => {}
            Payload::Saved => self.close_sheet(),
            Payload::Restored => {
                self.close_sheet();
                self.stage.set(Stage::Welcome);
            }
            Payload::Strength(e) => self.export_strength(e),
            Payload::LauncherCopied(issuer) => {
                self.cancel_auth();
                self.capture_notice(Tone::Success, self.fmt("Copied the code for {}", &[&issuer]));
            }
            Payload::Imported(n) => {
                let text = self.count("imported", n);
                if self.sheet.get() == Some(Sheet::ExportImport) {
                    self.file.replace(None);
                    self.file_name.set(String::new());
                    self.form.update(|f| f.password = Zeroizing::new(String::new()));
                    self.status.set(Some(Status { tone: Tone::Success, text }));
                } else {
                    self.close_sheet();
                    self.show_toast(Tone::Success, text);
                }
            }
            Payload::Captured(issuer, n) => {
                // The shortcut and the panel run without a visible main window: the result shows on the desktop.
                if n > 0 {
                    self.capture_notice(Tone::Success, self.fmt("Added {}", &[&issuer]));
                } else {
                    self.capture_notice(Tone::Info, self.fmt("{} already exists", &[&issuer]));
                }
            }
            Payload::Created(select) => {
                self.editing.set(None);
                if select && let Some(last) = self.projection.with(|p| p.categories.last().map(|c| c.id)) {
                    self.category.set(Some(last));
                    self.refresh();
                }
            }
            Payload::Copied(id) => {
                self.copied.set(Some(id));
                self.owner.get().run(|| {
                    set_timeout(Duration::from_millis(if self.surface == Surface::Panel { 1100 } else { 600 }), move || {
                        if self.copied.get() == Some(id) {
                            self.copied.set(None);
                        }
                    })
                });
            }
            Payload::Detail(d) => {
                self.form.update(|f| {
                    f.issuer = d.issuer.clone();
                    f.username = d.username.clone();
                    f.website = d.website.clone();
                    f.website_touched = true;
                });
                self.detail.set(Some(d));
            }
            Payload::Choices(c) => {
                self.selected.set(c.iter().map(|c| c.id).collect());
                self.choices.set(c);
            }
            Payload::Secret(secret) => {
                self.auth.set(None);
                self.auth_password.set(Zeroizing::new(String::new()));
                self.revealed.set(secret);
            }
            // The QR sheet opens only once verified, already holding the code: nothing shows behind the prompt.
            Payload::Qr(uri, image) => {
                let id = match self.auth.get() {
                    Some(Auth::Qr(id)) => Some(id),
                    _ => None,
                };
                self.auth.set(None);
                self.auth_password.set(Zeroizing::new(String::new()));
                self.qr_uri.set(uri);
                self.qr.set(image);
                if let Some(id) = id
                    && self.sheet.get() != Some(Sheet::Qr(id))
                {
                    self.sheet.set(Some(Sheet::Qr(id)));
                }
            }
            Payload::Export(data, n) => {
                self.auth.set(None);
                self.auth_password.set(Zeroizing::new(String::new()));
                self.form.update(|f| {
                    f.new_password = Zeroizing::new(String::new());
                    f.confirmation = Zeroizing::new(String::new());
                });
                self.task(async move {
                    match platform::files::save_export(data, self.label("Export Encrypted File").to_string()).await {
                        Ok(true) => self.status.set(Some(Status { tone: Tone::Success, text: self.count("exported", n) })),
                        Ok(false) => {}
                        Err(_) => self.status.set(Some(Status { tone: Tone::Error, text: self.label("Couldn’t save the file") })),
                    };
                });
            }
        }
    }
    pub fn refresh(self) {
        self.dispatch(false, |_| Ok(Payload::Silent));
    }
    pub fn unlock(self) {
        let password = self.form.with(|f| f.password.clone());
        self.form.update(|f| f.password = Zeroizing::new(String::new()));
        self.dispatch(true, move |s| {
            s.unlock(&password)?;
            Ok(Payload::Silent)
        });
    }
    /// `confirmed` is the user's answer to the fair-strength alert.
    pub fn create(self, confirmed: bool) {
        let f = self.form.get();
        self.dispatch(true, move |s| {
            s.create(&f.new_password, confirmed)?;
            Ok(Payload::Silent)
        });
    }
    /// Asks before accepting a fair-strength password, in an alert.
    pub fn confirm_fair(self, password: Zeroizing<String>, then: impl FnOnce(bool) + 'static) {
        use safay_app::core::password::{PasswordStrength, evaluate};
        if evaluate(&password) != PasswordStrength::Fair {
            then(false);
            return;
        }
        // "Choose Stronger Password" is the default button and answers Return and Escape; "Use Anyway" can only be clicked.
        let chosen = crate::alert::alert(self.label("Password Isn’t Strong Enough"))
            .description(self.label("A strong password is recommended. It keeps your data safer and makes it harder to crack."))
            .cancel("stronger", self.label("Choose Stronger Password"))
            .action("use", self.label("Use Anyway"))
            .default("stronger")
            .show();
        self.task(async move {
            if chosen.await == Some("use") {
                then(true);
            }
        });
    }
    pub fn save_new(self) {
        let f = self.form.get();
        let category = self.category.get();
        self.dispatch(true, move |s| {
            let draft = Draft {
                id: None,
                issuer: f.issuer.clone(),
                username: f.username.clone(),
                secret: f.secret.to_string(),
                algorithm: match f.algorithm {
                    1 => safay_app::core::TotpAlgorithm::Sha256,
                    2 => safay_app::core::TotpAlgorithm::Sha512,
                    _ => safay_app::core::TotpAlgorithm::Sha1,
                },
                digits: f.digits,
                period: f.period,
                website: f.website.clone(),
                category,
            };
            s.save_account(&draft)?;
            Ok(Payload::Imported(1))
        });
    }
    pub fn save_detail(self, id: Uuid) {
        let f = self.form.get();
        self.dispatch(true, move |s| {
            s.update_metadata(id, &f.issuer, &f.username, &f.website)?;
            Ok(Payload::Saved)
        });
    }
    pub fn copy(self, id: Uuid) {
        let clipboard = self.clipboard.get();
        let controller = self.controller.get();
        let epoch = controller.epoch();
        self.dispatch(true, move |s| {
            let (code, ttl) = s.copy_code(id)?;
            clipboard.copy(code, ttl, controller, epoch)?;
            Ok(Payload::Copied(id))
        });
    }
    pub fn import_text(self, text: Zeroizing<String>) {
        self.dispatch(true, move |s| Ok(Payload::Imported(s.import_text(&text)?)));
    }
    pub fn import_file(self) {
        // A file that needs no password is imported without one, whatever was typed for an earlier file.
        let password = if self.file_kind.get() == FileKind::Encrypted {
            self.form.with(|f| f.password.clone())
        } else {
            Zeroizing::new(String::new())
        };
        let bytes = self.file.get();
        self.dispatch(true, move |s| Ok(Payload::Imported(s.import_file(bytes.as_deref().ok_or(Error::Import)?, &password)?)));
    }
    pub fn choose_backup(self) {
        if self.fixture {
            return;
        }
        let epoch = self.controller.get().epoch();
        self.task(async move {
            match platform::files::choose_import(
                self.label("Choose File").to_string(),
                self.label("Safay or Authenticator backup").to_string(),
            )
            .await
            {
                Ok(Some((name, bytes))) if self.controller.get().epoch() == epoch => self.take_file(name, bytes),
                Ok(_) => {}
                Err(e) => self.error.set(Some(e)),
            };
        });
    }
    pub fn take_file(self, name: String, bytes: Vec<u8>) {
        let kind = safay_app::core::authenticator::inspect(&bytes);
        self.file_kind.set(kind);
        self.file.replace(Some(Zeroizing::new(bytes)));
        // `file_name` is what the UI follows (`has_file`): never empty for a chosen file.
        self.file_name.set(if name.is_empty() { "backup".into() } else { name });
        self.status.set(None);
        self.error.set((kind == FileKind::Unsupported).then_some(Error::ImportFile));
    }
    /// "Remove File" in Batch Import: back to choosing one, without the file, its password or what it was told.
    pub fn remove_file(self) {
        self.file.replace(None);
        self.file_name.set(String::new());
        self.form.update(|f| f.password = Zeroizing::new(String::new()));
        self.status.set(None);
        self.error.set(None);
    }
    /// Whether a backup file is chosen, reactively. The bytes live in `file`, a `Stored` (not reactive, so secret
    /// material is not copied into signals); its name is the signal set and cleared with it.
    pub fn has_file(self) -> bool {
        self.file_name.with(|n| !n.is_empty())
    }
    /// Decodes chosen images locally and adds their otpauth lines to the scan results.
    /// "Select on Screen" in the Add sheet: results join the scanned list, as images do.
    #[cfg(target_os = "macos")]
    pub fn capture_screen(self) {
        if self.fixture || self.busy.get() {
            return;
        }
        if !platform::screen_capture::has_permission() {
            platform::screen_capture::request_permission();
            self.status.set(Some(Status { tone: Tone::Info, text: self.label("Needs Screen Recording permission the first time") }));
            return;
        }
        self.status.set(None);
        let epoch = self.controller.get().epoch();
        self.task(async move {
            match platform::screen_capture::capture().await {
                Ok(Some(bytes)) if self.controller.get().epoch() == epoch => self.add_images(vec![bytes]).await,
                Ok(_) => {}
                Err(_) => self.error.set(Some(Error::Import)),
            }
        });
    }
    /// The capture shortcut and the panel's button: select a QR code on screen and import it at once.
    #[cfg(target_os = "macos")]
    pub fn capture_and_import(self) {
        if self.fixture || self.busy.get() {
            return;
        }
        if !self.projection.with(|p| p.exists) {
            self.capture_notice(Tone::Info, self.label("Set up Safay first"));
            return;
        }
        if !self.unlocked() {
            self.capture_notice(Tone::Info, self.label("Unlock Safay first"));
            platform::open_main();
            return;
        }
        if !platform::screen_capture::has_permission() {
            platform::screen_capture::request_permission();
            self.capture_notice(Tone::Info, self.label("Needs Screen Recording permission the first time"));
            return;
        }
        let epoch = self.controller.get().epoch();
        self.task(async move {
            let Ok(Some(bytes)) = platform::screen_capture::capture().await else { return };
            if self.controller.get().epoch() != epoch {
                return;
            }
            let text = match blocking(move || platform::files::decode_qr(&bytes).map(Zeroizing::new)).await {
                Ok(Ok(t)) => t,
                _ => {
                    self.capture_notice(Tone::Error, self.label("No QR code found"));
                    return;
                }
            };
            let issuer = safay_app::core::otpauth::parse_lines(&text, &safay_app::session::placeholders())
                .into_iter()
                .find(|p| p.is_valid())
                .map(|p| p.issuer);
            let Some(issuer) = issuer else {
                self.capture_notice(Tone::Error, self.label("No QR code found"));
                return;
            };
            self.dispatch(true, move |s| Ok(Payload::Captured(issuer, s.import_text(&text)?)));
        });
    }
    pub fn choose_images(self) {
        if self.fixture {
            return;
        }
        let epoch = self.controller.get().epoch();
        self.task(async move {
            let files =
                match platform::files::choose_images(self.label("Choose Images").to_string(), self.label("QR image").to_string()).await {
                    Ok(f) => f,
                    Err(e) => {
                        self.error.set(Some(e));
                        return;
                    }
                };
            if files.is_empty() || self.controller.get().epoch() != epoch {
                return;
            }
            self.add_images(files.into_iter().map(|(_, b)| b).collect()).await;
        });
    }
    /// Adds recognized QR text to the scanned list, once per line (images, the screen and the camera share it).
    pub fn append_scanned(self, text: &str) {
        self.scanned.update(|s| {
            let mut next = Zeroizing::new(String::with_capacity(s.len() + text.len() + 1));
            for line in s.lines().chain(text.lines()) {
                if !line.trim().is_empty() && !next.lines().any(|l| l == line) {
                    next.push_str(line);
                    next.push('\n');
                }
            }
            *s = next;
        });
    }
    pub async fn add_images(self, images: Vec<Vec<u8>>) {
        self.busy.set(true);
        let decoded = blocking(move || images.iter().map(|b| platform::files::decode_qr(b).map(Zeroizing::new)).collect::<Vec<_>>()).await;
        self.busy.set(false);
        let mut found = false;
        let mut error = None;
        for item in decoded.unwrap_or_default() {
            match item {
                Ok(text) => {
                    found = true;
                    self.append_scanned(&text);
                }
                Err(e) => error = Some(e),
            }
        }
        if !found {
            self.error.set(Some(error.unwrap_or(Error::Import)));
        }
    }
    /// Verifies the user before `kind`: with Touch ID set up, the system's Touch ID prompt at once (the key it releases is
    /// checked against the session), the master password dialog only if that does not verify; otherwise the master
    /// password dialog. Never the Mac login password: it proves nothing about this vault (SECURITY.md).
    pub fn reveal(self, kind: Auth) {
        self.auth_error.set(None);
        self.auth_password.set(Zeroizing::new(String::new()));
        let touch_id = self.touch_id_ready();
        self.auth_prompting.set(touch_id);
        self.auth.set(Some(kind));
        if touch_id {
            self.authorize_touch_id();
        }
    }
    /// Runs the operation the authentication dialog was opened for.
    pub fn authorize(self) {
        self.authorize_with(None)
    }
    /// Runs the action the verify dialog was opened for. With a Touch ID-released key, the key is checked against the
    /// session inside the same serialized operation, immediately before the action consumes the proof.
    fn authorize_with(self, key: Option<Zeroizing<Vec<u8>>>) {
        let Some(kind) = self.auth.get() else { return };
        let password = self.auth_password.get();
        self.auth_password.set(Zeroizing::new(String::new()));
        self.auth_error.set(None);
        let prove = move |s: &mut Session| -> Result<(), Error> {
            if let Some(k) = &key {
                s.prove_key(k)?;
            }
            Ok(())
        };
        match kind {
            Auth::Secret(id) => self.dispatch(true, move |s| {
                prove(s)?;
                Ok(Payload::Secret(s.reveal(id, &password, false)?))
            }),
            Auth::Qr(id) => self.dispatch(true, move |s| {
                prove(s)?;
                let (_, uri) = s.reveal_pair(id, &password)?;
                let image = Zeroizing::new(platform::files::qr_image(&uri)?);
                Ok(Payload::Qr(uri, image))
            }),
            Auth::Export => {
                let f = self.form.get();
                let ids: Vec<Uuid> = self.selected.get().into_iter().collect();
                self.dispatch(true, move |s| {
                    let n = ids.len();
                    prove(s)?;
                    let data = s.export_selected(&password, Some(&ids), &f.new_password, true)?;
                    Ok(Payload::Export(data, n))
                });
            }
            Auth::Reset => self.dispatch(true, move |s| {
                prove(s)?;
                s.reset(&password)?;
                Ok(Payload::Saved)
            }),
            Auth::DisableDelay => self.dispatch(true, move |s| {
                prove(s)?;
                s.set_failure_delay(false, &password)?;
                Ok(Payload::Silent)
            }),
            Auth::Copy(id) => {
                let clipboard = self.clipboard.get();
                let controller = self.controller.get();
                let epoch = controller.epoch();
                self.dispatch(true, move |s| {
                    prove(s)?;
                    let (code, ttl, issuer) = s.launcher_code_with_password(id, &password)?;
                    clipboard.copy(code, ttl, controller, epoch)?;
                    Ok(Payload::LauncherCopied(issuer))
                });
            }
        }
    }
    /// A launcher's copy that Touch ID did not prove (`crate::launcher`): unlocked, the master password dialog (Touch ID
    /// was just declined or is not set up); locked, the copy waits for the user to unlock (two minutes at most), which
    /// is the proof.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn launcher_copy_request(self, id: Uuid) {
        if self.unlocked() {
            self.auth_error.set(None);
            self.auth_password.set(Zeroizing::new(String::new()));
            self.auth_prompting.set(false);
            self.auth.set(Some(Auth::Copy(id)));
        } else {
            self.launcher_waiting.set(Some((id, std::time::Instant::now())));
        }
    }
    /// Runs a launcher's copy that waited for an unlock, once the vault is unlocked.
    pub fn launcher_copy_after_unlock(self) {
        let Some((id, since)) = self.launcher_waiting.get() else { return };
        if since.elapsed() > Duration::from_secs(120) {
            self.launcher_waiting.set(None);
            return;
        }
        if !self.unlocked() || self.busy.get() {
            return;
        }
        self.launcher_waiting.set(None);
        let clipboard = self.clipboard.get();
        let controller = self.controller.get();
        let epoch = controller.epoch();
        self.dispatch(true, move |s| {
            let (code, ttl, issuer) = s.launcher_code(id)?;
            clipboard.copy(code, ttl, controller, epoch)?;
            Ok(Payload::LauncherCopied(issuer))
        });
    }
    pub fn cancel_auth(self) {
        self.auth_prompting.set(false);
        self.auth.set(None);
        self.auth_error.set(None);
        self.auth_password.set(Zeroizing::new(String::new()));
    }
    pub fn change_password(self) {
        let f = self.form.get();
        let current = f.password.clone();
        self.confirm_fair(f.new_password.clone(), move |confirmed| {
            self.dispatch(true, move |s| {
                s.change_password(&current, &f.new_password, confirmed)?;
                Ok(Payload::Saved)
            });
        });
    }
    /// Restores an export. The session checks its password before the strength of the master password it would set, so
    /// a wrong export password is reported as such whichever master password is chosen; `confirmed` answers the
    /// fair-strength alert ([`Self::export_strength`]).
    pub fn restore_export(self, confirmed: bool) {
        let f = self.form.get();
        let bytes = self.file.get();
        let master = if self.use_export_password.get() { f.password.clone() } else { f.new_password.clone() };
        self.dispatch(true, move |session| {
            match session.restore_export(bytes.as_deref().ok_or(Error::Import)?, &f.password, &master, confirmed) {
                Ok(()) => Ok(Payload::Restored),
                Err(e @ (Error::Fair | Error::Weak)) => Ok(Payload::Strength(e)),
                Err(e) => Err(e),
            }
        });
    }
    /// The export opened, but the master password it would set is not strong enough. Fair: "Use Anyway" restores with
    /// it; "Choose Stronger Password" turns "Use as Master Password" off for a new one (or leaves the new one to
    /// change). Too weak (only the export password can be: new ones are checked as typed): a new one is needed.
    fn export_strength(self, e: Error) {
        if e == Error::Weak {
            self.use_export_password.set(false);
            self.error.set(Some(e));
            return;
        }
        let chosen = crate::alert::alert(self.label("Password Isn’t Strong Enough"))
            .description(self.label("A strong password is recommended. It keeps your data safer and makes it harder to crack."))
            .cancel("stronger", self.label("Choose Stronger Password"))
            .action("use", self.label("Use Anyway"))
            .default("stronger")
            .show();
        self.task(async move {
            match chosen.await {
                Some("use") => self.restore_export(true),
                _ => self.use_export_password.set(false),
            }
        });
    }
    pub fn restore_snapshot(self) {
        let p = self.form.with(|f| f.password.clone());
        self.form.update(|f| f.password = Zeroizing::new(String::new()));
        self.dispatch(true, move |s| {
            s.restore_snapshot(&p)?;
            Ok(Payload::Restored)
        });
    }
    pub fn delete_account(self, id: Uuid) {
        let issuer = self
            .projection
            .with(|p| p.rows.iter().find(|r| r.id == id).map(|r| r.issuer.clone()))
            .unwrap_or_else(|| self.detail.with(|d| d.as_ref().map(|d| d.issuer.clone())).unwrap_or_default());
        let chosen = crate::alert::alert(self.fmt("Delete “{}”?", &[&issuer]))
            .cancel("cancel", self.label("Cancel"))
            .destructive("delete", self.label("Delete"))
            .show();
        let epoch = self.controller.get().epoch();
        self.task(async move {
            if chosen.await == Some("delete") && self.controller.get().epoch() == epoch {
                let in_sheet = self.sheet.get().is_some();
                self.dispatch(true, move |s| {
                    s.delete_account(id)?;
                    Ok(if in_sheet { Payload::Saved } else { Payload::Silent })
                });
            }
        });
    }
    pub fn membership(self, category: Uuid, account: Uuid, add: bool) {
        self.dispatch(true, move |s| {
            s.membership(category, account, add)?;
            Ok(Payload::Silent)
        });
    }
    pub fn commit_category(self) {
        let Some(edit) = self.editing.get() else { return };
        let name = edit.name.trim().to_owned();
        if name.is_empty() {
            self.editing.set(None);
            return;
        }
        self.dispatch(true, move |s| {
            s.category(edit.id, &name)?;
            if edit.id.is_none()
                && let Some(account) = edit.account
                && let Some(created) = s.projection("", None)?.categories.last().map(|c| c.id)
            {
                s.membership(created, account, true)?;
            }
            Ok(Payload::Created(edit.id.is_none() && edit.account.is_none()))
        });
    }
    pub fn delete_category(self, id: Uuid) {
        let (name, count) =
            self.projection.with(|p| p.categories.iter().find(|c| c.id == id).map(|c| (c.name.clone(), c.count))).unwrap_or_default();
        let chosen = crate::alert::alert(self.fmt("Delete the category “{}”?", &[&name]))
            .description(self.count("category_kept", count))
            .cancel("cancel", self.label("Cancel"))
            .destructive("delete", self.label("Delete Category"))
            .show();
        let epoch = self.controller.get().epoch();
        self.task(async move {
            if chosen.await == Some("delete") && self.controller.get().epoch() == epoch {
                if self.category.get() == Some(id) {
                    self.category.set(None);
                }
                self.dispatch(true, move |s| {
                    s.delete_category(id)?;
                    Ok(Payload::Silent)
                });
            }
        });
    }
    pub fn move_row(self, id: Uuid, direction: isize) {
        let rows = self.projection.get().rows;
        let Some(index) = rows.iter().position(|a| a.id == id) else { return };
        let next = (index as isize + direction).clamp(0, rows.len().saturating_sub(1) as isize);
        if next == index as isize {
            return;
        }
        let mut ids: Vec<_> = rows.iter().map(|a| a.id).collect();
        let moved = ids.remove(index);
        ids.insert(next as usize, moved);
        self.reorder_rows(ids);
    }
    /// Saves the shown rows' new order (a finished drag); during a search the hidden accounts keep their places.
    pub fn reorder_rows(self, ids: Vec<Uuid>) {
        let category = self.category.get();
        self.dispatch(true, move |s| {
            s.reorder(category, &ids)?;
            Ok(Payload::Silent)
        });
    }
    pub fn move_category(self, id: Uuid, delta: isize) {
        let mut ids: Vec<_> = self.projection.get().categories.iter().map(|c| c.id).collect();
        let Some(index) = ids.iter().position(|c| *c == id) else { return };
        let next = (index as isize + delta).clamp(0, ids.len().saturating_sub(1) as isize) as usize;
        if next == index {
            return;
        }
        let moved = ids.remove(index);
        ids.insert(next, moved);
        self.reorder_categories(ids);
    }
    /// Saves the categories' new order (a finished drag).
    pub fn reorder_categories(self, ids: Vec<Uuid>) {
        self.dispatch(true, move |s| {
            s.reorder_categories(&ids)?;
            Ok(Payload::Silent)
        });
    }
    fn apply_preferences(self, p: safay_app::preferences::Preferences) {
        use safay_app::preferences::{Appearance as A, Language as P};
        // Only what changed: every label reads the language, so setting it again would re-render the whole window.
        let previous = self.loaded_preferences.get().then(|| self.preferences.get());
        if previous != Some(p) {
            self.preferences.set(p);
        }
        self.loaded_preferences.set(true);
        if self.cards.get() != p.cards {
            self.cards.set(p.cards);
        }
        let language = match p.language {
            P::System => self.system_language,
            P::En => Language::En,
            P::Hans => Language::Hans,
            P::Hant => Language::Hant,
        };
        if self.language.get() != language {
            self.language.set(language);
        }
        if previous.is_none_or(|q| q.appearance != p.appearance) {
            set_color_scheme(match p.appearance {
                A::System => Appearance::System,
                A::Light => Appearance::Light,
                A::Dark => Appearance::Dark,
            });
        }
        // The real host (also `--preview`, whose data stays in memory) integrates with the system; NSH test runners don't.
        #[cfg(target_os = "macos")]
        if crate::hosted() && self.surface == Surface::Main {
            crate::desktop::apply(p, language);
        }
        // The login item's state is a query to the system: read it when the window first loads (the Settings window
        // loads each time it opens), not on every change; its own switch reads it again after changing it.
        if previous.is_none() && crate::hosted() && self.surface != Surface::Panel && platform::supports::LOGIN_ITEM {
            self.task(async move {
                let enabled = nsh_core::element::desktop::launch_at_login().await;
                self.login_item.set(Some(enabled));
            });
        }
    }
    /// Settings take effect at once; the file is written in the background without blocking the window (no busy
    /// state). Only the newest choice is written, and a failed write puts back what is stored and reports it.
    pub fn update_preferences(self, update: impl FnOnce(&mut safay_app::preferences::Preferences) + Send + 'static) {
        let mut next = self.preferences.get();
        update(&mut next);
        if self.loaded_preferences.get() && next == self.preferences.get() {
            return;
        }
        self.apply_preferences(next);
        let ticket = prefs::begin();
        let controller = self.controller.get();
        let epoch = controller.epoch();
        self.owner.get().run(|| {
            spawn(async move {
                let c2 = controller.clone();
                let written = blocking(move || {
                    // A newer choice is on its way: it writes instead.
                    if !prefs::is_latest(ticket) {
                        return Ok(());
                    }
                    c2.run(epoch, |s| s.set_preferences(next))
                })
                .await;
                prefs::finish();
                match written {
                    Ok(Ok(())) => {
                        // Other windows (the main window, the panel) pick the change up.
                        self.seen_revision.set(controller.revision() + 1);
                        controller.touch();
                    }
                    Ok(Err(e)) => {
                        self.fail(e);
                        self.refresh();
                    }
                    Err(_) => {
                        self.fail(Error::Storage);
                        self.refresh();
                    }
                }
            })
        });
    }
    pub fn start(self) {
        self.check_biometrics();
        self.refresh();
        let listener = self.controller.get();
        self.task(async move {
            let mut seen = listener.epoch();
            loop {
                seen = listener.changed(seen).await;
                self.after_lock();
            }
        });
        let other = self.controller.get();
        self.task(async move {
            let mut seen = other.revision();
            loop {
                seen = other.revised(seen).await;
                // Our own operations already re-projected; refresh only for another window's change.
                if seen != self.seen_revision.get() {
                    self.refresh();
                }
            }
        });
        set_interval(Duration::from_secs(1), move || {
            self.refresh();
            self.refresh_updates();
        });
        self.refresh_updates();
        self.schedule_updates();
    }
    /// Mirrors the updater's phase (one updater per process; the main window and Settings both show it).
    pub fn refresh_updates(self) {
        if self.surface == Surface::Panel || self.fixture {
            return;
        }
        let phase = platform::update_phase();
        if self.updates.get() != phase {
            self.updates.set(phase);
        }
    }
    /// The main window of a build that updates itself: once its preferences are loaded, counts the launch, asks on the
    /// second launch whether to check automatically, and checks once a day when allowed.
    fn schedule_updates(self) {
        if self.surface != Surface::Main || self.fixture || !platform::updates_configured() {
            return;
        }
        let started = signal(false);
        effect(move || {
            if !self.loaded_preferences.get() || untracked(|| started.get()) {
                return;
            }
            started.set(true);
            untracked(|| {
                self.update_preferences(|p| p.launches = p.launches.saturating_add(1));
                set_timeout(Duration::from_secs(3), move || self.ask_about_updates());
                set_interval(Duration::from_secs(60 * 60), move || self.check_updates_if_due());
            });
        });
    }
    fn ask_about_updates(self) {
        let p = self.preferences.get();
        if p.update_checks.is_some() || p.launches < 2 {
            self.check_updates_if_due();
            return;
        }
        let chosen = crate::alert::alert(self.label("Check for updates automatically?"))
            .description(self.label("Safay can check for a new version once a day. You can change this in Settings."))
            .cancel("no", self.label("Don’t Check"))
            .action("yes", self.label("Check Automatically"))
            .show();
        self.task(async move {
            let yes = chosen.await == Some("yes");
            self.update_preferences(move |p| p.update_checks = Some(yes));
            if yes {
                self.check_updates(false);
            }
        });
    }
    fn check_updates_if_due(self) {
        let p = self.preferences.get();
        let now = safay_app::sync::to_ms(std::time::SystemTime::now());
        if p.update_checks == Some(true) && now.saturating_sub(p.last_update_check_ms) >= 24 * 60 * 60 * 1000 {
            self.check_updates(false);
        }
    }
    /// Checks for a newer version. `manual` (the menu, Settings): the result is reported and a newer version offered;
    /// otherwise a newer one downloads on its own when the user allowed it, else it is offered.
    pub fn check_updates(self, manual: bool) {
        let Some(updater) = platform::updates::updater() else { return };
        self.task(async move {
            let phase =
                blocking(move || updater.check()).await.unwrap_or(platform::updates::Phase::Failed(platform::updates::Failure::Network));
            self.refresh_updates();
            let now = safay_app::sync::to_ms(std::time::SystemTime::now());
            self.update_preferences(move |p| p.last_update_check_ms = now);
            use platform::updates::Phase;
            match phase {
                Phase::Available { version, notes } => {
                    if !manual && self.preferences.get().update_downloads {
                        self.download_update(false);
                    } else {
                        self.offer_update(version, notes);
                    }
                }
                Phase::UpToDate if manual && self.surface == Surface::Main => {
                    self.show_toast(Tone::Success, self.label("Safay is up to date"));
                }
                Phase::Failed(f) if manual && self.surface == Surface::Main => {
                    self.show_toast(Tone::Error, self.label(update_failure_key(f)));
                }
                _ => {}
            }
        });
    }
    fn offer_update(self, version: String, notes: Option<String>) {
        let mut dialog = crate::alert::alert(self.fmt("Safay {} is available", &[&version]));
        if let Some(notes) = notes.filter(|n| !n.trim().is_empty()) {
            dialog = dialog.description(notes);
        }
        let chosen = dialog.cancel("later", self.label("Later")).action("download", self.label("Download and Install")).show();
        self.task(async move {
            if chosen.await == Some("download") {
                self.download_update(true);
            }
        });
    }
    /// Downloads and verifies the offered version. `interactive`: then asks to restart now (it otherwise installs when
    /// the app quits).
    pub fn download_update(self, interactive: bool) {
        let Some(updater) = platform::updates::updater() else { return };
        self.task(async move {
            let phase =
                blocking(move || updater.download()).await.unwrap_or(platform::updates::Phase::Failed(platform::updates::Failure::Network));
            self.refresh_updates();
            match phase {
                platform::updates::Phase::Ready { version } if interactive => {
                    let chosen = crate::alert::alert(self.fmt("Safay {} is ready to install", &[&version]))
                        .description(self.label("Restart now, or it installs when you quit Safay."))
                        .cancel("later", self.label("Later"))
                        .action("restart", self.label("Restart Now"))
                        .show();
                    if chosen.await == Some("restart") {
                        self.restart_to_update();
                    }
                }
                platform::updates::Phase::Failed(f) if interactive => {
                    self.show_toast(Tone::Error, self.label(update_failure_key(f)));
                }
                _ => {}
            }
        });
    }
    /// Locks the vault, swaps in the downloaded version and restarts into it; the old app stays if the swap fails.
    pub fn restart_to_update(self) {
        let Some(updater) = platform::updates::updater() else { return };
        self.controller.get().request_lock();
        self.task(async move {
            match blocking(move || updater.install()).await {
                Ok(Ok(())) => self.owner.get().run(nsh_core::element::desktop::restart),
                _ => {
                    self.refresh_updates();
                    self.show_toast(Tone::Error, self.label(update_failure_key(platform::updates::Failure::Install)));
                }
            }
        });
    }
}
pub fn update_failure_key(f: crate::platform::updates::Failure) -> &'static str {
    use crate::platform::updates::Failure::*;
    match f {
        Network => "Can’t reach the update server. Check your internet connection.",
        Invalid => "The update couldn’t be verified, so it wasn’t installed.",
        Install => "The update couldn’t be installed. The current version is kept.",
    }
}
impl Store {
    /// Runs a sync operation off the UI thread, then re-projects every window.
    fn sync_job(
        self,
        op: impl FnOnce(&Controller, u64, &str) -> Result<(), safay_app::sync::SyncError> + Send + 'static,
        on_error: impl Fn(safay_app::sync::SyncError) + 'static,
    ) {
        if self.syncing.get() {
            return;
        }
        self.syncing.set(true);
        let controller = self.controller.get();
        let epoch = controller.epoch();
        let device = platform::device_name();
        self.task(async move {
            let c2 = controller.clone();
            let result = blocking(move || op(&c2, epoch, &device)).await;
            self.syncing.set(false);
            // Accounts a sync or restore brought in are never fetched.
            icons::rebaseline();
            controller.touch();
            match result {
                Ok(Err(e)) => on_error(e),
                Err(_) => on_error(safay_app::sync::SyncError::Storage),
                Ok(Ok(())) => {}
            }
            self.refresh();
        });
    }
    pub fn sync_now(self, source: safay_app::sync::Source) {
        self.sync_job(move |c, e, d| safay_app::sync::run(c, e, d, source).map(|_| ()), |_| {});
    }
    /// Picks a folder (iCloud Drive or a folder synced by another app) and links it after the read-only check.
    pub fn choose_folder(self) {
        if self.fixture {
            return;
        }
        self.task(async move {
            let Some(folder) =
                platform::files::choose_folder(self.label("Choose Folder").to_string(), self.label("Choose").to_string()).await
            else {
                return;
            };
            let path = safay_app::sync::FolderBackend::resolve(&folder);
            self.link(safay_app::sync::Location::Folder { path });
        });
    }
    /// Chooses iCloud: confirms this Mac is signed in and records the account, then links it like any other location
    /// (read-only check first: a backup this vault's key can't open is never overwritten).
    pub fn link_icloud(self) {
        // Fixtures have no CloudKit store unless a test installs the in-memory one.
        self.status.set(None);
        self.sync_job(
            |c, e, _| safay_app::sync::link(c, e, safay_app::icloud::location()?),
            move |e| self.status.set(Some(Status { tone: Tone::Error, text: self.label(sync_error_key(e)) })),
        );
        self.backup_page.set(BackupPage::Overview);
    }
    pub fn link(self, location: safay_app::sync::Location) {
        self.status.set(None);
        self.sync_job(
            move |c, e, _| {
                let credential = match &location {
                    safay_app::sync::Location::Cloud { credential, .. } => Some(credential.clone()),
                    _ => None,
                };
                let result = safay_app::sync::link(c, e, location);
                // A location that could not be linked keeps no stored sign-in.
                if result.is_err()
                    && let Some(credential) = credential
                {
                    safay_app::cloud::forget(&credential);
                }
                result
            },
            move |e| {
                self.status.set(Some(Status { tone: Tone::Error, text: self.label(sync_error_key(e)) }));
            },
        );
        self.backup_page.set(BackupPage::Overview);
    }
    pub fn open_cloud_setup(self, provider: safay_app::cloud::Provider) {
        self.status.set(None);
        self.cloud_form.set(CloudForm::default());
        self.backup_page.set(BackupPage::Setup(provider));
    }
    /// First run: the connection sheet for a cloud storage service, in restore mode.
    pub fn choose_restore_cloud(self, provider: safay_app::cloud::Provider) {
        if self.busy.get() {
            return;
        }
        self.reset_transient();
        self.cloud_restore.set(true);
        self.open_cloud_setup(provider);
        self.sheet.set(Some(Sheet::Backup));
        // Chosen again: WebDAV and Nextcloud show the form as submitted (Nextcloud's Connect reuses the sign-in); the
        // other services, signed in to their app folder only, look for the backup again straight away.
        if let Some(form) = self.restore_sign_ins.with(|k| k.form(provider)) {
            self.cloud_form.set(form);
        }
        if !matches!(provider, safay_app::cloud::Provider::Webdav | safay_app::cloud::Provider::Nextcloud)
            && let Some(connection) = self.restore_sign_ins.with(|k| k.connection(provider))
        {
            self.cloud_form.update(|f| {
                f.working = true;
                f.checking = true;
            });
            let epoch = self.controller.get().epoch();
            self.task(async move {
                let result = blocking(move || safay_app::cloud::reconnect(&connection)).await;
                if self.controller.get().epoch() != epoch || !self.cloud_restore.get() {
                    return;
                }
                match result {
                    Ok(Ok(location)) => self.connected(location, None),
                    Ok(Err(e)) => {
                        if e == safay_app::sync::SyncError::SignIn {
                            self.restore_sign_ins.with(|k| k.invalid(provider));
                        }
                        self.cloud_form.update(|f| {
                            f.working = false;
                            f.checking = false;
                            f.error = Some(self.label(sync_error_key(e)));
                        });
                    }
                    Err(_) => self.cloud_form.update(|f| {
                        f.working = false;
                        f.checking = false;
                    }),
                }
            });
        }
    }
    /// Revokes the restore sign-ins nothing uses once the vault exists (`in_use`: the service it was restored from).
    fn release_restore_sign_ins(self, in_use: Option<safay_app::cloud::Provider>) {
        let unused = self.restore_sign_ins.with(|k| k.release(in_use));
        if !unused.is_empty() {
            self.task(async move {
                drop(blocking(move || unused.iter().for_each(|c| safay_app::cloud::revoke(c))).await);
            });
        }
    }
    /// A restore location's cloud sign-in leaves the credential store: kept in memory for choosing the service again,
    /// not revoked.
    async fn keep_restore_sign_in(self, location: &safay_app::sync::Location) {
        if let safay_app::sync::Location::Cloud { provider, credential, .. } = location {
            let (provider, credential) = (*provider, credential.clone());
            if let Ok(Some(connection)) = blocking(move || safay_app::cloud::take(&credential)).await {
                self.restore_sign_ins.with(|k| k.keep(provider, connection));
            }
        }
    }
    /// A connection is checked and stored: restoring looks for the backup there; otherwise the location is linked.
    /// `new_sign_in`: the form of a new sign-in (`Some(None)` for the services without one); `None` reuses a kept one.
    fn connected(self, location: safay_app::sync::Location, new_sign_in: Option<Option<CloudForm>>) {
        if !self.cloud_restore.get() {
            return self.link(location);
        }
        if let (Some(form), safay_app::sync::Location::Cloud { provider, .. }) = (new_sign_in, &location)
            && let Some(replaced) = self.restore_sign_ins.with(|k| k.signed_in(*provider, form))
        {
            self.task(async move {
                drop(blocking(move || safay_app::cloud::revoke(&replaced)).await);
            });
        }
        // The sheet stays on its third step ("Find your backup") until the location has been read.
        self.cloud_form.update(|f| {
            f.working = true;
            f.waiting = false;
            f.checking = true;
        });
        let name = match &location {
            safay_app::sync::Location::Cloud { provider, .. } => provider.name(),
            _ => "",
        };
        self.probe_restore(location, self.fmt("No secure Safay backup was found in {}.", &[name]));
    }
    /// Drops the found restore location; a cloud sign-in stored only for it leaves the store, kept in memory.
    pub fn discard_restore_location(self) {
        if let Some(location) = self.restore_location.get() {
            self.task(async move { self.keep_restore_sign_in(&location).await });
        }
        self.restore_location.set(None);
    }
    /// The user compared the fingerprint and trusts this certificate: connect again, trusting only it.
    pub fn trust_certificate(self, provider: safay_app::cloud::Provider, fingerprint: [u8; 32]) {
        self.cloud_form.update(|f| {
            f.trusted = Some(fingerprint);
            f.untrusted = None;
        });
        self.connect_cloud(provider);
    }
    pub fn decline_certificate(self) {
        self.cloud_form.update(|f| f.untrusted = None);
    }
    pub fn leave_cloud_setup(self) {
        #[cfg(target_os = "macos")]
        platform::web_auth::cancel();
        self.cloud_form.set(CloudForm::default());
        self.status.set(None);
        self.backup_page.set(BackupPage::Choose);
    }
    /// During first-run restore: the Nextcloud sign-in kept for the server, certificate and consent in `form`.
    fn kept_nextcloud(self, form: &CloudForm) -> Option<Zeroizing<Vec<u8>>> {
        use safay_app::cloud::Provider;
        if !self.cloud_restore.get() {
            return None;
        }
        self.restore_sign_ins.with(|k| {
            let kept = k.form(Provider::Nextcloud)?;
            let same = kept.address.trim() == form.address.trim() && kept.trusted == form.trusted && kept.insecure == form.insecure;
            if same { k.connection(Provider::Nextcloud) } else { None }
        })
    }
    /// WebDAV: checks the address and credentials, stores the sign-in, then links the location (read-only check first).
    /// Nextcloud: opens Login Flow v2 in the browser and polls until the user allows Safay (at most 20 minutes).
    pub fn connect_cloud(self, provider: safay_app::cloud::Provider) {
        use safay_app::cloud::{self, AddressError, Provider};
        // Debug fixtures reach no server. A server named `self-signed.example.invalid` answers as one whose certificate the
        // system rejects, so tests and screenshots can show the certificate confirmation (synthetic fingerprint).
        #[cfg(debug_assertions)]
        if self.fixture && self.cloud_form.with(|f| f.address.contains("self-signed.example.invalid") && f.trusted.is_none()) {
            self.cloud_form.update(|f| f.untrusted = Some(std::array::from_fn(|i| (i as u8).wrapping_mul(37).wrapping_add(11))));
            return;
        }
        if self.fixture || self.cloud_form.with(|f| f.working) {
            return;
        }
        let form = self.cloud_form.get();
        let endpoint = match cloud::endpoint(&form.address, form.insecure) {
            Ok(e) => e,
            Err(AddressError::NeedsInsecureConsent) => {
                self.cloud_form.update(|f| f.needs_consent = true);
                return;
            }
            Err(AddressError::Invalid) => {
                self.cloud_form.update(|f| f.error = Some(self.label("Enter a server address that starts with https:// or http://")));
                return;
            }
        };
        self.cloud_form.update(|f| {
            f.working = true;
            f.error = None;
        });
        let epoch = self.controller.get().epoch();
        let trusted = form.trusted;
        // A certificate the system does not trust is offered for confirmation (once: a pinned one that changed is not).
        let fail = move |e: safay_app::sync::SyncError| {
            self.cloud_form.update(|f| {
                f.working = false;
                f.waiting = false;
                match e {
                    safay_app::sync::SyncError::CertificateUnverified(fingerprint) if f.trusted.is_none() => {
                        f.untrusted = Some(fingerprint);
                        f.error = None;
                    }
                    e => f.error = Some(self.label(sync_error_key(e))),
                }
            });
        };
        self.task(async move {
            let connected = if provider == Provider::Webdav {
                let (username, password) = (form.username.trim().to_owned(), form.password.to_string());
                blocking(move || cloud::webdav(endpoint, username, password, trusted).and_then(cloud::connect))
                    .await
                    .unwrap_or(Err(safay_app::sync::SyncError::Storage))
            } else if let Some(kept) = self.kept_nextcloud(&form) {
                // The same server as a sign-in kept during this restore: reuse it; a revoked one signs in anew.
                match blocking(move || cloud::reconnect(&kept)).await.unwrap_or(Err(safay_app::sync::SyncError::Storage)) {
                    Err(safay_app::sync::SyncError::SignIn) => {
                        self.restore_sign_ins.with(|k| k.invalid(Provider::Nextcloud));
                        self.cloud_form.update(|f| f.working = false);
                        return self.connect_cloud(provider);
                    }
                    Ok(location) => {
                        if self.controller.get().epoch() != epoch {
                            return;
                        }
                        self.cloud_form.set(CloudForm::default());
                        return self.connected(location, None);
                    }
                    Err(e) => return fail(e),
                }
            } else {
                let server = endpoint.clone();
                let flow = match blocking(move || cloud::nextcloud_begin(&endpoint, trusted)).await {
                    Ok(Ok(flow)) => flow,
                    Ok(Err(e)) => return fail(e),
                    Err(_) => return fail(safay_app::sync::SyncError::Storage),
                };
                drop(open_url(flow.login_url.clone()));
                self.cloud_form.update(|f| f.waiting = true);
                let started = std::time::Instant::now();
                loop {
                    drop(blocking(|| std::thread::sleep(Duration::from_secs(2))).await);
                    if !self.cloud_form.with(|f| f.waiting) || self.controller.get().epoch() != epoch {
                        return;
                    }
                    if started.elapsed() > Duration::from_secs(20 * 60) {
                        self.cloud_form.update(|f| {
                            f.working = false;
                            f.waiting = false;
                            f.error = Some(self.label("Sign-in timed out. Try again"));
                        });
                        return;
                    }
                    let (poll, server) = (flow.poll.clone(), server.clone());
                    match blocking(move || cloud::nextcloud_poll(&server, &poll, trusted)).await {
                        Ok(Ok(Some(connection))) => {
                            break blocking(move || cloud::connect(connection)).await.unwrap_or(Err(safay_app::sync::SyncError::Storage));
                        }
                        Ok(Ok(None)) => {}
                        Ok(Err(e)) => return fail(e),
                        Err(_) => return fail(safay_app::sync::SyncError::Storage),
                    }
                }
            };
            if self.controller.get().epoch() != epoch {
                return;
            }
            match connected {
                Ok(location) => {
                    self.cloud_form.set(CloudForm::default());
                    let submitted = CloudForm {
                        address: form.address,
                        username: form.username,
                        password: form.password,
                        insecure: form.insecure,
                        trusted,
                        ..Default::default()
                    };
                    self.connected(location, Some(Some(submitted)));
                }
                Err(e) => fail(e),
            }
        });
    }
    /// Site icons (SECURITY.md, Network): on the main window only, new accounts and changed websites since the baseline
    /// are fetched, at most four websites at a time; locking or turning the setting off drops the queue.
    fn queue_site_icons(self) {
        if self.surface != Surface::Main || self.fixture {
            return;
        }
        if !self.unlocked() || !self.preferences.with(|p| p.site_icons) {
            icons::reset();
            return;
        }
        let wanted: Vec<(Uuid, String)> = self
            .projection
            .with(|p| p.rows.iter().filter(|r| r.icon.is_none()).filter_map(|r| r.website.clone().map(|w| (r.id, w))).collect());
        let all: Vec<(Uuid, String)> =
            self.projection.with(|p| p.rows.iter().filter_map(|r| r.website.clone().map(|w| (r.id, w))).collect());
        icons::offer(&all, &wanted);
        self.drain_site_icons();
    }
    fn drain_site_icons(self) {
        while let Some((id, website)) = icons::next() {
            let controller = self.controller.get();
            let epoch = controller.epoch();
            self.task(async move {
                let site = website.clone();
                let png = blocking(move || platform::site_icons::fetch(&site)).await.ok().flatten();
                icons::finished();
                if let Some(png) = png
                    && controller.epoch() == epoch
                {
                    let applied = blocking(move || controller.run(epoch, |s| s.apply_icon(id, &website, &png))).await;
                    if matches!(applied, Ok(Ok(true))) {
                        self.controller.get().touch();
                        self.refresh();
                    }
                }
                self.drain_site_icons();
            });
        }
    }
    /// Dropbox / OneDrive / Google Drive: sign in through `ASWebAuthenticationSession`, exchange the code (the core checks
    /// the callback and `state`), check and store the connection, then link it. Safay's own app folder only.
    pub fn connect_oauth(self, provider: safay_app::cloud::Provider) {
        if self.fixture || self.cloud_form.with(|f| f.working) {
            return;
        }
        let Some(client) = crate::pages::backup::oauth_client(provider) else { return };
        let authorization = match safay_app::cloud::oauth_begin(provider, safay_app::cloudset::AccessScope::AppFolder, client) {
            Ok(a) => a,
            Err(e) => {
                self.cloud_form.update(|f| f.error = Some(self.label(sync_error_key(e))));
                return;
            }
        };
        let outcome = platform::web_sign_in(authorization.url(), &authorization.callback_scheme());
        self.cloud_form.update(|f| {
            f.working = true;
            f.waiting = true;
            f.error = None;
        });
        let epoch = self.controller.get().epoch();
        self.task(async move {
            use platform::Signed;
            let signed = blocking(move || outcome.recv().unwrap_or(Signed::Failed)).await.unwrap_or(Signed::Failed);
            let callback = match signed {
                Signed::Callback(callback) => callback,
                // Closed by the user: back to the form, nothing stored.
                Signed::Closed => {
                    return self.cloud_form.update(|f| {
                        f.working = false;
                        f.waiting = false;
                    });
                }
                Signed::Failed => {
                    return self.cloud_form.update(|f| {
                        f.working = false;
                        f.waiting = false;
                        f.error = Some(self.label("Something went wrong while syncing"));
                    });
                }
            };
            self.cloud_form.update(|f| f.waiting = false);
            let result = blocking(move || safay_app::cloud::oauth_complete(&authorization, &callback)).await;
            if self.controller.get().epoch() != epoch {
                return;
            }
            match result {
                Ok(Ok(location)) => {
                    self.cloud_form.set(CloudForm::default());
                    self.connected(location, Some(None));
                }
                Ok(Err(e)) => self.cloud_form.update(|f| {
                    f.working = false;
                    f.error = Some(self.label(sync_error_key(e)));
                }),
                Err(_) => self.cloud_form.update(|f| f.working = false),
            }
        });
    }
    /// Whether iCloud is the backup location.
    pub fn icloud_linked(self) -> bool {
        self.projection.with(|p| matches!(p.sync.location, Some(safay_app::sync::Location::ICloud { .. })))
    }
    /// Subscribes to iCloud backup changes and registers for silent pushes (best effort: offline, it is
    /// tried again at the next launch). Only the main window does it.
    pub fn subscribe_icloud(self) {
        if self.surface != Surface::Main {
            return;
        }
        #[cfg(target_os = "macos")]
        if crate::hosted() {
            platform::macos::enable_remote_push();
        }
        self.task(async move {
            drop(blocking(safay_app::icloud::subscribe).await);
        });
    }
    pub fn unsubscribe_icloud(self) {
        if self.surface != Surface::Main {
            return;
        }
        self.task(async move {
            drop(blocking(safay_app::icloud::unsubscribe).await);
        });
    }
    pub fn turn_off_sync(self) {
        let title = self.label("Turn off backup?");
        let icloud = self.icloud_linked();
        // iCloud offers to delete the backup too; elsewhere the stored backup is always kept.
        let chosen = if icloud {
            crate::alert::alert(title)
                .description(self.label(
                    "Deleting removes the encrypted backup from iCloud for all your devices. Devices that still use iCloud backup upload their copy again.",
                ))
                .cancel("cancel", self.label("Cancel"))
                .action("off", self.label("Turn Off and Keep Backup"))
                .destructive("delete", self.label("Turn Off and Delete iCloud Backup"))
                .show()
        } else {
            crate::alert::alert(title)
                .description(self.label("The backup already stored there is kept."))
                .cancel("cancel", self.label("Cancel"))
                .destructive("off", self.label("Turn Off"))
                .show()
        };
        self.task(async move {
            let action = chosen.await;
            if action == Some("delete") {
                self.turn_off_deleting_icloud_backup();
                return;
            }
            if action == Some("off") {
                // A cloud location's sign-in is revoked where possible and removed from the credential store.
                if let Some(safay_app::sync::Location::Cloud { credential, .. }) = self.projection.with(|p| p.sync.location.clone()) {
                    drop(blocking(move || safay_app::cloud::forget(&credential)).await);
                }
                self.dispatch(true, |s| {
                    s.sync_off()?;
                    Ok(Payload::Silent)
                });
            }
        });
    }
    /// Turns off sync and deletes the iCloud backup: waits for a running sync (so the backup is not uploaded again
    /// right after), deletes the backup in iCloud, then turns backup off. If deleting fails (offline, ...), backup stays on
    /// and the error shows.
    fn turn_off_deleting_icloud_backup(self) {
        self.status.set(None);
        self.task(async move {
            while self.syncing.get() {
                drop(blocking(|| std::thread::sleep(Duration::from_millis(100))).await);
            }
            match blocking(safay_app::icloud::delete_backup).await {
                Ok(Ok(())) => self.dispatch(true, |s| {
                    s.sync_off()?;
                    Ok(Payload::Silent)
                }),
                Ok(Err(e)) => self.status.set(Some(Status { tone: Tone::Error, text: self.label(sync_error_key(e)) })),
                Err(_) => self
                    .status
                    .set(Some(Status { tone: Tone::Error, text: self.label(sync_error_key(safay_app::sync::SyncError::Storage)) })),
            }
        });
    }
    pub fn load_review(self) {
        self.prefer_remote.set(BTreeSet::new());
        let controller = self.controller.get();
        let epoch = controller.epoch();
        self.task(async move {
            let review = blocking(move || controller.run(epoch, |s| s.conflict_review())).await;
            match review {
                Ok(Ok(r)) => self.review.set(Some(r)),
                _ => self
                    .status
                    .set(Some(Status { tone: Tone::Error, text: self.label("The backup can’t be compared right now. Try again.") })),
            }
        });
    }
    /// 0 merge both, 1 keep this device, 2 use the backup.
    pub fn resolve_conflict(self, choice: u8) {
        let prefer: std::collections::HashSet<Uuid> = self.prefer_remote.get().into_iter().collect();
        self.sync_job(
            move |c, e, d| match choice {
                0 => safay_app::sync::merge_both(c, e, d, prefer),
                1 => safay_app::sync::keep_local(c, e, d),
                _ => safay_app::sync::use_backup(c, e, d),
            },
            move |e| self.status.set(Some(Status { tone: Tone::Error, text: self.label(sync_error_key(e)) })),
        );
        self.review.set(None);
        self.backup_page.set(BackupPage::Overview);
    }
    pub fn restore_unreadable(self) {
        let password = self.form.with(|f| f.password.clone());
        self.form.update(|f| f.password = Zeroizing::new(String::new()));
        self.sync_job(
            move |c, e, _| safay_app::sync::restore_unreadable(c, e, &password),
            move |e| self.status.set(Some(Status { tone: Tone::Error, text: self.label(sync_error_key(e)) })),
        );
        self.backup_page.set(BackupPage::Overview);
    }
    /// First run: choose a backup folder; the backup is opened only after its password is entered.
    pub fn choose_restore_folder(self) {
        if self.fixture {
            return;
        }
        self.task(async move {
            let Some(folder) =
                platform::files::choose_folder(self.label("Choose Folder").to_string(), self.label("Choose").to_string()).await
            else {
                return;
            };
            let path = safay_app::sync::FolderBackend::resolve(&folder);
            self.probe_restore(safay_app::sync::Location::Folder { path }, self.label("No Safay backup was found in that folder."));
        });
    }
    /// First run from iCloud: confirms the account, then looks for the backup record (opened only after the password).
    pub fn choose_restore_icloud(self) {
        // Fixtures have no CloudKit store unless a test installs the in-memory one.
        self.busy.set(true);
        self.task(async move {
            let location = blocking(safay_app::icloud::location).await;
            self.busy.set(false);
            match location {
                Ok(Ok(location)) => self.probe_restore(location, self.fmt("No secure Safay backup was found in {}.", &["iCloud"])),
                Ok(Err(e)) => self.status.set(Some(Status { tone: Tone::Error, text: self.label(sync_error_key(e)) })),
                Err(_) => {}
            }
        });
    }
    /// Reads the location once: a backup there becomes the restore source; none, or an error, is reported.
    fn probe_restore(self, location: safay_app::sync::Location, missing: SharedString) {
        self.busy.set(true);
        self.task(async move {
            let probe = location.clone();
            let found = blocking(move || probe.backend().read().map(|r| r.is_some())).await;
            self.busy.set(false);
            // Opened from the connection sheet: it closes once the read is done; the result shows on the restore page.
            if self.cloud_restore.get() {
                self.cloud_restore.set(false);
                self.cloud_form.set(CloudForm::default());
                self.sheet.set(None);
            }
            // A cloud sign-in stored just for this restore leaves the store when there is nothing to restore from it; it is
            // kept in memory, so choosing the service again needs no new sign-in.
            if !matches!(found, Ok(Ok(true))) {
                self.keep_restore_sign_in(&location).await;
            }
            match found {
                Ok(Ok(true)) => {
                    self.error.set(None);
                    self.restore_location.set(Some(location));
                }
                Ok(Ok(false)) => self.status.set(Some(Status { tone: Tone::Error, text: missing })),
                Ok(Err(e)) => self.status.set(Some(Status { tone: Tone::Error, text: self.label(sync_error_key(e)) })),
                Err(_) => {}
            }
        });
    }
    pub fn restore_from_location(self) {
        let Some(folder) = self.restore_location.get() else { return };
        let password = self.form.with(|f| f.password.clone());
        let controller = self.controller.get();
        let epoch = controller.epoch();
        self.busy.set(true);
        self.task(async move {
            let c2 = controller.clone();
            let result = blocking(move || safay_app::sync::restore_first_run(&c2, epoch, folder, &password)).await;
            self.busy.set(false);
            match result {
                Ok(Ok(())) => {
                    let in_use = match &self.restore_location.get() {
                        Some(safay_app::sync::Location::Cloud { provider, .. }) => Some(*provider),
                        _ => None,
                    };
                    self.release_restore_sign_ins(in_use);
                    self.restore_location.set(None);
                    self.form.set(Form::fresh());
                    self.stage.set(Stage::Welcome);
                    controller.touch();
                }
                Ok(Err(e)) => self.status.set(Some(Status { tone: Tone::Error, text: self.label(sync_error_key(e)) })),
                Err(_) => {}
            }
            self.refresh();
        });
    }
}
impl Store {
    /// Touch ID can unlock right now: hardware, the preference, and a wrapped key.
    pub fn touch_id_ready(self) -> bool {
        self.biometrics.get() && self.preferences.with(|p| p.touch_id) && self.projection.with(|p| p.biometric)
    }
    pub fn check_biometrics(self) {
        #[cfg(target_os = "macos")]
        if crate::hosted() {
            self.biometrics.set(platform::biometric::available());
        }
    }
    /// Keeps a hardware-wrapped copy of the key while Touch ID is on (after unlock, creation, a password change).
    fn maybe_wrap_key(self) {
        #[cfg(target_os = "macos")]
        {
            let wanted = self.surface == Surface::Main
                && self.biometrics.get()
                && self.preferences.with(|p| p.touch_id)
                && self.projection.with(|p| p.unlocked && !p.biometric);
            if !wanted || self.wrapping.get() {
                return;
            }
            self.wrapping.set(true);
            let controller = self.controller.get();
            let epoch = controller.epoch();
            self.task(async move {
                let c = controller.clone();
                let _ = blocking(move || {
                    let key = c.run(epoch, |s| s.key_material())?;
                    let file = platform::biometric::seal(&key).map_err(|_| Error::Crypto)?;
                    c.run(epoch, |s| s.set_biometric(Some(&file)))
                })
                .await;
                self.wrapping.set(false);
                self.refresh();
            });
        }
    }
    /// Turning Touch ID off deletes the wrapped key in the same operation; turning it on wraps it again once unlocked.
    pub fn set_touch_id(self, on: bool) {
        self.dispatch(true, move |s| {
            let mut p = s.preferences();
            p.touch_id = on;
            s.set_preferences(p)?;
            if !on {
                s.set_biometric(None)?;
            }
            Ok(Payload::Silent)
        });
    }
    /// Removes the wrapped key (Touch ID turned off, or the key went stale).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn forget_biometric(self) {
        self.dispatch(true, |s| {
            s.set_biometric(None)?;
            Ok(Payload::Silent)
        });
    }
    /// Asks for Touch ID; `then` gets the released key. Otherwise `otherwise(cancelled)` runs, after any message:
    /// `cancelled` when the user, the app or the system cancelled the prompt (nothing is shown then), not for
    /// "Use Password…", a failure, a lockout or a stale key (which is deleted).
    fn with_touch_id(self, reason: &'static str, then: impl FnOnce(Zeroizing<Vec<u8>>) + 'static, otherwise: impl FnOnce(bool) + 'static) {
        #[cfg(target_os = "macos")]
        {
            let controller = self.controller.get();
            let epoch = controller.epoch();
            let reason = self.label(reason).to_string();
            let cancel = self.label("Cancel").to_string();
            self.task(async move {
                let c = controller.clone();
                let result = blocking(move || {
                    let file = c.run(epoch, |s| s.biometric()).ok().flatten();
                    let file = file.ok_or(platform::biometric::Failure::Stale)?;
                    platform::biometric::open(&file, &reason, &cancel)
                })
                .await;
                use platform::biometric::Failure as F;
                let key = match result {
                    Ok(Ok(key)) => return then(key),
                    Ok(Err(e)) => Some(e),
                    Err(_) => None,
                };
                match key {
                    Some(F::Cancelled) => return otherwise(true),
                    Some(F::Fallback) | None => {}
                    Some(F::Stale) => {
                        self.forget_biometric();
                        self.fail(Error::StaleKey);
                    }
                    Some(e) => {
                        let text = self.label(match e {
                            F::Lockout => "Touch ID is locked; use master password",
                            F::Unavailable => "This Mac doesn’t support Touch ID or it’s unavailable",
                            _ => "Touch ID failed; try again or enter master password",
                        });
                        if self.auth.get().is_some() {
                            self.auth_error.set(None);
                            self.status.set(Some(Status { tone: Tone::Error, text }));
                        } else {
                            self.status.set(Some(Status { tone: Tone::Error, text }));
                        }
                    }
                }
                otherwise(false);
            });
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (reason, then);
            otherwise(false);
        }
    }
    pub fn unlock_touch_id(self) {
        self.status.set(None);
        self.with_touch_id(
            "unlock Safay",
            move |key| {
                self.dispatch(true, move |s| {
                    s.unlock_with_key(&key)?;
                    Ok(Payload::Silent)
                })
            },
            |_| {},
        );
    }
    /// The verify dialog's Touch ID path: the released key must match this session's before the action runs.
    pub fn authorize_touch_id(self) {
        let reason = match self.auth.get() {
            Some(Auth::Secret(_)) => "reveal the account secret",
            Some(Auth::Qr(_)) => "show the account QR code",
            _ => "confirm it’s you",
        };
        self.with_touch_id(
            reason,
            move |key| {
                self.auth_prompting.set(false);
                self.authorize_with(Some(key))
            },
            move |cancelled| {
                // Cancelling the prompt that came first cancels the action; cancelling one asked
                // for from the dialog's Touch ID button leaves the dialog. "Use Password…" and failures go to the
                // master password dialog.
                if cancelled && self.auth_prompting.get() {
                    self.cancel_auth();
                } else {
                    self.auth_prompting.set(false);
                }
            },
        );
    }
}
pub fn sync_error_key(e: safay_app::sync::SyncError) -> &'static str {
    use safay_app::sync::SyncError::*;
    match e {
        FolderUnavailable => "Can’t access the sync folder. Choose it again.",
        Corrupt => "The backup file format is invalid.",
        NoRemote => "No Safay backup was found in that folder.",
        Unreadable => {
            "The backup here can’t be opened with this vault’s key. It belongs to another vault, or its master password was changed on another device."
        }
        RemoteChanged => "The backup changed during the sync. Try again.",
        NoLocal => "No local encrypted vault available to sync.",
        Locked => "Unlock Safay to continue.",
        TooLarge => "The file exceeds the size or image limit.",
        Storage => "The change couldn’t be saved. Your previous vault is kept.",
        Password => "Incorrect master password",
        Network => "Can’t reach the server. Check the address and your network connection",
        Certificate => "The server’s certificate isn’t trusted",
        CertificateUnverified(_) => "The server’s certificate can’t be verified",
        CertificateChanged => "The server’s certificate has changed. Set up this location again to check the new certificate",
        SignIn => "Sign in again to keep syncing",
        Forbidden => "The server denied access to this folder",
        Quota => "Not enough storage space on the server",
        RateLimited => "The server is busy. Try again later",
        Server(_) => "Something went wrong while syncing",
        Protocol => "Couldn’t read the server’s response",
        ICloud(e) => {
            use safay_app::icloud::ICloudError as I;
            match e {
                I::NotSignedIn => "Sign in to iCloud on this device to use iCloud backup.",
                I::Restricted => "iCloud is restricted on this device, for example by Screen Time or device management.",
                I::TemporarilyUnavailable => "iCloud is temporarily unavailable. Try again later.",
                I::AccountChanged => "The iCloud account on this device changed. Choose iCloud again to back up to the new account.",
                I::Offline => "Can’t reach iCloud. Check your internet connection.",
                I::QuotaExceeded => "Your iCloud storage is full.",
                I::Busy => "iCloud is busy. Safay will try again later.",
                I::NotEntitled => "This build of Safay can’t use iCloud.",
                I::Service => "Something went wrong while syncing",
            }
        }
    }
}
impl Form {
    pub fn fresh() -> Self {
        Self { digits: 6, period: 30, ..Default::default() }
    }
}

/// The process's one site icon queue (only the main window uses it).
mod icons {
    use safay_app::Uuid;
    use std::{
        collections::{HashSet, VecDeque},
        sync::Mutex,
    };
    #[derive(Default)]
    struct Queue {
        /// (account, website) pairs already handled or present at the baseline.
        seen: HashSet<(Uuid, String)>,
        baselined: bool,
        pending: VecDeque<(Uuid, String)>,
        running: u32,
    }
    static QUEUE: Mutex<Option<Queue>> = Mutex::new(None);
    fn with<R>(f: impl FnOnce(&mut Queue) -> R) -> R {
        let mut guard = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        f(guard.get_or_insert_with(Queue::default))
    }
    pub fn reset() {
        with(|q| *q = Queue { running: q.running, ..Default::default() });
    }
    /// The next projection becomes the baseline again (after a sync or restore).
    pub fn rebaseline() {
        with(|q| {
            q.baselined = false;
            q.pending.clear();
        });
    }
    /// `all`: every account with a website; `wanted`: those still without an icon.
    pub fn offer(all: &[(Uuid, String)], wanted: &[(Uuid, String)]) {
        with(|q| {
            if !q.baselined {
                q.seen.extend(all.iter().cloned());
                q.baselined = true;
                return;
            }
            for item in wanted {
                if q.seen.insert(item.clone()) {
                    q.pending.push_back(item.clone());
                }
            }
        });
    }
    pub fn next() -> Option<(Uuid, String)> {
        with(|q| {
            if q.running >= safay_app::core::site_icon::MAX_CONCURRENT_HOSTS {
                return None;
            }
            let item = q.pending.pop_front()?;
            q.running += 1;
            Some(item)
        })
    }
    pub fn finished() {
        with(|q| q.running = q.running.saturating_sub(1));
    }
}

/// Settings writes in flight, process-wide (all windows share one preferences file).
mod prefs {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LATEST: AtomicU64 = AtomicU64::new(0);
    static PENDING: AtomicU64 = AtomicU64::new(0);
    static FINISHED: AtomicU64 = AtomicU64::new(0);
    /// A write starts; returns its ticket.
    pub fn begin() -> u64 {
        PENDING.fetch_add(1, Ordering::SeqCst);
        LATEST.fetch_add(1, Ordering::SeqCst) + 1
    }
    pub fn is_latest(ticket: u64) -> bool {
        LATEST.load(Ordering::SeqCst) == ticket
    }
    pub fn finish() {
        FINISHED.fetch_add(1, Ordering::SeqCst);
        PENDING.fetch_sub(1, Ordering::SeqCst);
    }
    /// Taken when a projection starts.
    pub fn generation() -> u64 {
        LATEST.load(Ordering::SeqCst) + FINISHED.load(Ordering::SeqCst)
    }
    /// The projection started with `generation` reflects every settings write: none pending, none since.
    pub fn settled(generation: u64) -> bool {
        PENDING.load(Ordering::SeqCst) == 0 && self::generation() == generation
    }
}
