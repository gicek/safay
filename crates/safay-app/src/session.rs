//! Single-owner vault state. All operations run off the UI thread and commit before publishing changes.
use crate::storage::{MAX_FILE_BYTES, Repository, StorageError};
use safay_core::{
    Account, Placeholders, TotpAlgorithm, authenticator, canonical, category, dedup, otpauth, password, search, site_icon, totp, trash,
    vault::{self, VaultContents, VaultError, VaultKey},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

pub trait Clock: Send + Sync {
    fn wall(&self) -> SystemTime;
    fn elapsed(&self) -> Duration;
}
pub struct SystemClock(Instant);
impl Default for SystemClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl Clock for SystemClock {
    fn wall(&self) -> SystemTime {
        SystemTime::now()
    }
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("vault is locked")]
    Locked,
    #[error("vault already exists")]
    Exists,
    #[error("no vault exists")]
    Missing,
    #[error("wrong password")]
    Password,
    #[error("vault is corrupt or unsupported")]
    Corrupt,
    #[error("choose a stronger password")]
    Weak,
    #[error("confirm use of a fair-strength password")]
    Fair,
    #[error("wait before trying again")]
    Wait,
    #[error("account fields are invalid")]
    Fields,
    #[error("account already exists")]
    Duplicate,
    #[error("item no longer exists")]
    NotFound,
    #[error("category name is invalid or already used")]
    Category,
    #[error("import contains no supported accounts")]
    Import,
    #[error("the file is encrypted and needs its password")]
    FilePasswordRequired,
    #[error("wrong password for the file")]
    FilePassword,
    #[error("not a supported backup file")]
    ImportFile,
    #[error("the file has no accounts to import")]
    ImportEmpty,
    #[error("operation was cancelled by locking")]
    Cancelled,
    #[error("storage is unavailable")]
    Storage,
    #[error("another process changed the vault; lock and reopen it")]
    Changed,
    #[error("file is too large")]
    TooLarge,
    #[error("system cryptography failed")]
    Crypto,
    #[error("the saved key no longer opens the vault")]
    StaleKey,
}
impl From<StorageError> for Error {
    fn from(e: StorageError) -> Self {
        match e {
            StorageError::Changed => Self::Changed,
            StorageError::TooLarge => Self::TooLarge,
            _ => Self::Storage,
        }
    }
}
impl From<VaultError> for Error {
    fn from(e: VaultError) -> Self {
        match e {
            VaultError::InvalidPassword | VaultError::EmptyPassword => Self::Password,
            VaultError::CorruptVault | VaultError::InvalidKey => Self::Corrupt,
            VaultError::UnacceptablePassword => Self::Weak,
            _ => Self::Crypto,
        }
    }
}

/// A file's own password and format (an import, or an export restored on first launch), told apart from the master
/// password's and the vault's.
fn file_error(e: VaultError) -> Error {
    match e {
        VaultError::InvalidPassword => Error::FilePassword,
        VaultError::EmptyPassword => Error::FilePasswordRequired,
        VaultError::CorruptVault | VaultError::InvalidKey => Error::ImportFile,
        e => e.into(),
    }
}
fn import_error(e: authenticator::ImportError) -> Error {
    use authenticator::ImportError;
    match e {
        ImportError::PasswordRequired => Error::FilePasswordRequired,
        ImportError::WrongPassword => Error::FilePassword,
        ImportError::InvalidFile => Error::ImportFile,
        ImportError::NoSupportedAccounts => Error::ImportEmpty,
        ImportError::Vault(e) => file_error(e),
    }
}

/// Drop application-owned account strings, including cloned transaction contents.
pub(crate) struct Sensitive(pub(crate) VaultContents);
impl Drop for Sensitive {
    fn drop(&mut self) {
        for a in &mut self.0.accounts {
            a.secret.zeroize();
            a.issuer.zeroize();
            a.username.zeroize();
            a.website.zeroize();
            a.icon.zeroize();
        }
        for c in &mut self.0.categories {
            c.name.zeroize();
        }
    }
}

/// Account input is never Debug or serialized outside the encrypted vault.
pub struct Draft {
    pub id: Option<Uuid>,
    pub issuer: String,
    pub username: String,
    pub secret: String,
    pub algorithm: TotpAlgorithm,
    pub digits: u32,
    pub period: u32,
    pub website: String,
    pub category: Option<Uuid>,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            id: None,
            issuer: String::new(),
            username: String::new(),
            secret: String::new(),
            algorithm: TotpAlgorithm::Sha1,
            digits: 6,
            period: 30,
            website: String::new(),
            category: None,
        }
    }
}
impl Drop for Draft {
    fn drop(&mut self) {
        self.secret.zeroize();
        self.issuer.zeroize();
        self.username.zeroize();
        self.website.zeroize();
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Row {
    pub id: Uuid,
    pub issuer: String,
    pub username: String,
    pub code: String,
    pub remaining: u32,
    pub period: u32,
    pub digits: u32,
    /// Added within the last 30 minutes (the "recently added" dot).
    pub recent: bool,
    pub website: Option<String>,
    /// The stored site icon (PNG, checked by the core), shared so projections stay cheap to clone.
    pub icon: Option<std::sync::Arc<[u8]>>,
    /// Categories this account belongs to, in category order.
    pub categories: Vec<Uuid>,
}
/// Non-secret account facts for the details sheet.
#[derive(Clone, PartialEq, Eq)]
pub struct Detail {
    pub id: Uuid,
    pub issuer: String,
    pub username: String,
    pub website: String,
    pub algorithm: TotpAlgorithm,
    pub digits: u32,
    pub period: u32,
    pub imported: bool,
    pub created_at: SystemTime,
}
/// Export choices; the list is only issuer/username and never secrets.
#[derive(Clone, PartialEq, Eq)]
pub struct Choice {
    pub id: Uuid,
    pub issuer: String,
    pub username: String,
}
/// An account in Recently Deleted: what its row shows, never a code.
#[derive(Clone, PartialEq, Eq)]
pub struct DeletedRow {
    pub id: Uuid,
    pub issuer: String,
    pub username: String,
    /// Whole days before it is removed for good.
    pub days_left: u32,
    pub website: Option<String>,
    pub icon: Option<std::sync::Arc<[u8]>>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct CategoryRow {
    pub id: Uuid,
    pub name: String,
    pub count: usize,
}
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Projection {
    pub account_count: usize,
    pub preferences: crate::preferences::Preferences,
    pub exists: bool,
    pub unlocked: bool,
    pub rows: Vec<Row>,
    pub categories: Vec<CategoryRow>,
    /// Recently Deleted, most recently deleted first (narrowed by the search, as the rows are).
    pub deleted: Vec<DeletedRow>,
    /// How many accounts are in Recently Deleted, whatever the search.
    pub deleted_count: usize,
    pub waiting: u64,
    pub has_snapshot: bool,
    pub sync: SyncSummary,
    /// A hardware-wrapped key is stored (Touch ID can unlock).
    pub biometric: bool,
}
/// What the Backup & Sync sheet and the sidebar dot show; no secrets, only the location and outcomes.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SyncSummary {
    pub location: Option<crate::sync::Location>,
    pub status: crate::sync::Status,
    pub last_synced_ms: Option<i64>,
    pub history: Vec<crate::sync::HistoryEntry>,
    pub due: bool,
}

pub struct Session {
    pub(crate) repo: Box<dyn Repository>,
    pub(crate) clock: Arc<dyn Clock>,
    gate: Arc<AtomicU64>,
    epoch: u64,
    pub(crate) contents: Option<Sensitive>,
    pub(crate) key: Option<VaultKey>,
    pub(crate) envelope: Option<Vec<u8>>,
    pub(crate) sync: crate::sync::SyncState,
    pub(crate) sync_status: crate::sync::Status,
    /// When the vault last changed with a backup location set; a sync runs after the debounce.
    pub(crate) sync_dirty: Option<(Duration, bool)>,
    /// The backup that caused the current conflict (ciphertext only).
    pub(crate) conflict: Option<crate::sync::Remote>,
    /// A hardware-released key matched the session key: the next identity check passes once.
    key_proof: bool,
    exists: bool,
    has_snapshot: bool,
    failures: u32,
    wait_started: Duration,
    last_activity: Duration,
    auto_lock: Duration,
    preferences: crate::preferences::Preferences,
    /// What launchers may search; kept current here, kept when locking (crate::launcher).
    launcher: Arc<crate::launcher::Launcher>,
}
impl Session {
    pub fn new(repo: Box<dyn Repository>, clock: Arc<dyn Clock>, gate: Arc<AtomicU64>) -> Result<Self, Error> {
        let failures = repo.failures()?;
        let preferences = repo.preferences()?.validated();
        let exists = repo.exists()?;
        let has_snapshot = repo.has_snapshot()?;
        let now = clock.elapsed();
        let sync = repo.sync_state()?.map(|d| crate::sync::SyncState::decode(&d)).unwrap_or_default();
        Ok(Self {
            sync_status: if sync.location.is_some() { crate::sync::Status::Idle } else { crate::sync::Status::Off },
            sync,
            sync_dirty: None,
            conflict: None,
            key_proof: false,
            exists,
            has_snapshot,
            repo,
            clock,
            epoch: gate.load(Ordering::SeqCst),
            gate,
            contents: None,
            key: None,
            envelope: None,
            failures,
            wait_started: now,
            last_activity: now,
            auto_lock: Duration::from_secs(preferences.auto_lock_minutes * 60),
            preferences,
            launcher: {
                let launcher = Arc::new(crate::launcher::Launcher::default());
                launcher.set_enabled(preferences.launcher);
                launcher
            },
        })
    }
    pub fn lock(&mut self) {
        self.contents = None;
        self.key = None;
        self.envelope = None;
        self.conflict = None;
        self.key_proof = false;
        self.sync_dirty = None;
        if self.sync_status == crate::sync::Status::Running || self.sync_status == crate::sync::Status::Conflict {
            self.sync_status = crate::sync::Status::Idle;
        }
        self.epoch = self.gate.load(Ordering::SeqCst);
    }
    pub fn tick(&mut self) {
        if self.epoch != self.gate.load(Ordering::SeqCst) || self.clock.elapsed().saturating_sub(self.last_activity) >= self.auto_lock {
            self.lock();
        }
    }
    fn guard(&mut self) -> Result<(), Error> {
        self.tick();
        if self.contents.is_none() { Err(Error::Locked) } else { Ok(()) }
    }
    fn valid_epoch(&self, start: u64) -> Result<(), Error> {
        if self.gate.load(Ordering::SeqCst) != start { Err(Error::Cancelled) } else { Ok(()) }
    }
    pub fn activity(&mut self) {
        self.tick();
        if self.contents.is_some() {
            self.last_activity = self.clock.elapsed();
        }
    }
    pub fn preferences(&self) -> crate::preferences::Preferences {
        self.preferences
    }
    pub fn set_preferences(&mut self, next: crate::preferences::Preferences) -> Result<(), Error> {
        let next = next.validated();
        if self.preferences.failure_delay && !next.failure_delay {
            return Err(Error::Locked);
        }
        self.repo.set_preferences(next)?;
        self.preferences = next;
        self.auto_lock = Duration::from_secs(next.auto_lock_minutes * 60);
        self.tick();
        self.launcher.set_enabled(next.launcher);
        self.index_for_launchers();
        Ok(())
    }
    /// What launchers search (crate::launcher), shared with the controller.
    pub fn launcher(&self) -> Arc<crate::launcher::Launcher> {
        self.launcher.clone()
    }
    /// Brings the launchers' index up to date while unlocked; a locked session leaves it as it was.
    fn index_for_launchers(&self) {
        if let Some(contents) = self.contents.as_ref() {
            self.launcher
                .rebuild(contents.0.accounts.iter().filter(|a| !a.is_deleted()).map(|a| (a.id, a.issuer.as_str(), a.username.as_str())));
        }
    }
    /// A launcher's copy, after the user proved it is them now (Touch ID or the master password): the account's code, how
    /// long it stays on the clipboard, and its service name for the notice.
    pub fn launcher_code(&mut self, id: Uuid) -> Result<(Zeroizing<String>, Duration, String), Error> {
        self.guard()?;
        let a = self
            .contents
            .as_ref()
            .ok_or(Error::Locked)?
            .0
            .accounts
            .iter()
            .find(|a| a.id == id && !a.is_deleted())
            .ok_or(Error::NotFound)?;
        let result = (Zeroizing::new(totp::account_code(a, self.clock.wall())), Duration::from_secs(u64::from(a.period)), a.issuer.clone());
        self.activity();
        Ok(result)
    }
    /// A launcher's copy proven by Touch ID: the released key unlocks a locked vault, or must equal an unlocked one's key.
    pub fn launcher_code_with_key(&mut self, id: Uuid, bytes: &[u8]) -> Result<(Zeroizing<String>, Duration, String), Error> {
        self.tick();
        if self.contents.is_none() {
            self.unlock_with_key(bytes)?;
        } else {
            let current = self.key.as_ref().ok_or(Error::Locked)?.as_bytes();
            let same = current.len() == bytes.len() && current.iter().zip(bytes).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0;
            if !same {
                return Err(Error::StaleKey);
            }
        }
        self.launcher_code(id)
    }
    /// A launcher's copy proven by the master password while unlocked.
    pub fn launcher_code_with_password(&mut self, id: Uuid, password: &str) -> Result<(Zeroizing<String>, Duration, String), Error> {
        self.verify(password)?;
        self.launcher_code(id)
    }
    /// Turning the failed-attempt delay off is a protection change, so it needs the master password.
    pub fn set_failure_delay(&mut self, enabled: bool, password: &str) -> Result<(), Error> {
        if !enabled {
            self.verify(password)?;
        }
        let next = crate::preferences::Preferences { failure_delay: enabled, ..self.preferences }.validated();
        self.repo.set_preferences(next)?;
        self.preferences = next;
        Ok(())
    }
    pub fn set_auto_lock(&mut self, minutes: u64) -> Result<(), Error> {
        if ![1, 5, 30].contains(&minutes) {
            return Err(Error::Fields);
        }
        self.set_preferences(crate::preferences::Preferences { auto_lock_minutes: minutes, ..self.preferences })
    }
    pub fn waiting(&self) -> u64 {
        if !self.preferences.failure_delay {
            return 0;
        }
        password::lockout_seconds(self.failures)
            .map(|s| {
                Duration::from_secs_f64(s)
                    .saturating_sub(self.clock.elapsed().saturating_sub(self.wait_started))
                    .as_secs()
                    .saturating_add(1)
            })
            .filter(|_| {
                self.clock.elapsed().saturating_sub(self.wait_started)
                    < Duration::from_secs_f64(password::lockout_seconds(self.failures).unwrap_or(0.0))
            })
            .unwrap_or(0)
    }
    fn before_password(&self) -> Result<(), Error> {
        if self.waiting() > 0 { Err(Error::Wait) } else { Ok(()) }
    }
    fn failed(&mut self) -> Result<(), Error> {
        self.failures = self.failures.saturating_add(1);
        self.wait_started = self.clock.elapsed();
        self.repo.set_failures(self.failures)?;
        Ok(())
    }
    fn succeeded(&mut self) -> Result<(), Error> {
        self.repo.set_failures(0)?;
        self.failures = 0;
        Ok(())
    }
    fn acceptable(password: &str, confirmed: bool) -> Result<(), Error> {
        if !password::is_acceptable_master_password(password) {
            return Err(Error::Weak);
        }
        if password::evaluate(password) == password::PasswordStrength::Fair && !confirmed {
            return Err(Error::Fair);
        }
        Ok(())
    }
    fn adopt(&mut self, data: Vec<u8>, contents: Sensitive, key: VaultKey, start: u64) -> Result<(), Error> {
        self.valid_epoch(start)?;
        self.epoch = start;
        self.contents = Some(contents);
        self.key = Some(key);
        self.envelope = Some(data);
        self.last_activity = self.clock.elapsed();
        self.sync_soon(true);
        Ok(())
    }
    /// Replaces the local vault with another envelope (a downloaded or restored backup) and keeps the session open.
    /// `key` is the backup's key when it differs from the current one.
    pub(crate) fn replace_with(&mut self, data: Vec<u8>, contents: Sensitive, key: Option<VaultKey>) -> Result<(), Error> {
        self.guard()?;
        let start = self.epoch;
        let current = self.envelope.clone().ok_or(Error::Locked)?;
        self.repo.commit(Some(&current), &data, Some(&current))?;
        self.has_snapshot = true;
        let key = match key {
            Some(k) => {
                // A different key: the hardware-wrapped copy is stale.
                let _ = self.repo.set_biometric(None);
                k
            }
            None => self.key.take().ok_or(Error::Locked)?,
        };
        self.valid_epoch(start)?;
        self.epoch = start;
        self.contents = Some(contents);
        self.key = Some(key);
        self.envelope = Some(data);
        self.last_activity = self.clock.elapsed();
        Ok(())
    }
    /// Saves merged contents under the current key, as any edit does, without scheduling another sync.
    pub(crate) fn persist_sync(&mut self, next: Sensitive) -> Result<(), Error> {
        self.persist(next)?;
        self.sync_dirty = None;
        Ok(())
    }
    /// First run: a backup becomes this device's vault once its master password opens it.
    pub(crate) fn restore_backup(&mut self, data: &[u8], password: &str) -> Result<(), Error> {
        if self.repo.exists()? {
            return Err(Error::Exists);
        }
        let start = self.gate.load(Ordering::SeqCst);
        let opened = vault::unlock(data, password)?;
        let contents = Sensitive(opened.contents);
        self.valid_epoch(start)?;
        self.repo.commit(None, data, None)?;
        self.exists = true;
        self.succeeded()?;
        self.adopt(data.to_vec(), contents, opened.key, start)?;
        self.sync_dirty = None;
        Ok(())
    }
    // ------------------------------------------------------------ hardware-protected key (Touch ID)

    /// The vault key for the platform to wrap in hardware. Only while unlocked; the caller zeroizes it.
    pub fn key_material(&mut self) -> Result<Zeroizing<Vec<u8>>, Error> {
        self.guard()?;
        Ok(Zeroizing::new(self.key.as_ref().ok_or(Error::Locked)?.as_bytes().to_vec()))
    }
    pub fn biometric(&self) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.repo.biometric()?)
    }
    pub fn set_biometric(&mut self, file: Option<&[u8]>) -> Result<(), Error> {
        Ok(self.repo.set_biometric(file)?)
    }
    /// Unlocks with a key the hardware released after Touch ID. A key that no longer opens the vault (the master password
    /// changed on another device) is reported as stale for the caller to delete; it never counts as a wrong password.
    pub fn unlock_with_key(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let start = self.gate.load(Ordering::SeqCst);
        let data = self.repo.read()?.ok_or(Error::Missing)?;
        let key = VaultKey::from_bytes(bytes).map_err(|_| Error::StaleKey)?;
        let contents =
            Sensitive(vault::open(&data, &key).map_err(|e| if e == VaultError::CorruptVault { Error::Corrupt } else { Error::StaleKey })?);
        self.valid_epoch(start)?;
        self.succeeded()?;
        self.adopt(data, contents, key, start)
    }
    /// Re-verification by Touch ID: the released key must equal this session's key; then the next `verify` passes once.
    pub fn prove_key(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.guard()?;
        let current = self.key.as_ref().ok_or(Error::Locked)?.as_bytes();
        let same = current.len() == bytes.len() && current.iter().zip(bytes).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0;
        if !same {
            return Err(Error::StaleKey);
        }
        self.key_proof = true;
        Ok(())
    }
    pub fn create(&mut self, password: &str, confirmed: bool) -> Result<(), Error> {
        if self.repo.read()?.is_some() {
            return Err(Error::Exists);
        }
        Self::acceptable(password, confirmed)?;
        let start = self.gate.load(Ordering::SeqCst);
        let contents = Sensitive(VaultContents::default());
        let (data, key) = vault::create(password, &contents.0, self.clock.wall())?;
        let _ = Sensitive(vault::open(&data, &key)?);
        self.valid_epoch(start)?;
        self.repo.commit(None, &data, None)?;
        self.exists = true;
        self.succeeded()?;
        self.adopt(data, contents, key, start)
    }
    pub fn restore_export(&mut self, data: &[u8], export_password: &str, new_password: &str, confirmed: bool) -> Result<(), Error> {
        if self.repo.exists()? {
            return Err(Error::Exists);
        }
        if data.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::TooLarge);
        }
        if !vault::is_safay_export(data) {
            return Err(Error::ImportFile);
        }
        let start = self.gate.load(Ordering::SeqCst);
        // The export password first: whether the new master password is strong enough is asked only of one that opens it.
        let contents = Sensitive(vault::import_export(data, export_password).map_err(file_error)?);
        Self::acceptable(new_password, confirmed)?;
        let (encrypted, key) = vault::create(new_password, &contents.0, self.clock.wall())?;
        let _ = Sensitive(vault::open(&encrypted, &key)?);
        self.valid_epoch(start)?;
        self.repo.commit(None, &encrypted, None)?;
        self.exists = true;
        self.succeeded()?;
        self.adopt(encrypted, contents, key, start)
    }
    pub fn unlock(&mut self, password: &str) -> Result<(), Error> {
        self.before_password()?;
        let start = self.gate.load(Ordering::SeqCst);
        let data = self.repo.read()?.ok_or(Error::Missing)?;
        let opened = match vault::unlock(&data, password) {
            Ok(v) => v,
            Err(e) => {
                if e == VaultError::InvalidPassword {
                    self.failed()?;
                }
                return Err(e.into());
            }
        };
        let contents = Sensitive(opened.contents);
        self.valid_epoch(start)?;
        self.succeeded()?;
        self.adopt(data, contents, opened.key, start)
    }
    pub fn verify(&mut self, password: &str) -> Result<(), Error> {
        self.guard()?;
        if std::mem::take(&mut self.key_proof) {
            if self.repo.read()?.as_deref() != self.envelope.as_deref() {
                return Err(Error::Changed);
            }
            self.activity();
            return Ok(());
        }
        self.before_password()?;
        let start = self.epoch;
        let opened = match vault::unlock(self.envelope.as_deref().ok_or(Error::Locked)?, password) {
            Ok(v) => v,
            Err(e) => {
                if e == VaultError::InvalidPassword {
                    self.failed()?;
                }
                return Err(e.into());
            }
        };
        let _contents = Sensitive(opened.contents);
        self.valid_epoch(start)?;
        if self.repo.read()?.as_deref() != self.envelope.as_deref() {
            return Err(Error::Changed);
        }
        self.succeeded()?;
        self.activity();
        Ok(())
    }
    fn persist(&mut self, next: Sensitive) -> Result<(), Error> {
        self.guard()?;
        let start = self.epoch;
        let current = self.envelope.as_ref().ok_or(Error::Locked)?;
        let key = self.key.as_ref().ok_or(Error::Locked)?;
        let data = vault::reseal(current, key, &next.0, self.clock.wall())?;
        let _ = Sensitive(vault::open(&data, key)?);
        self.valid_epoch(start)?;
        self.repo.commit(Some(current), &data, Some(current))?;
        self.has_snapshot = true;
        self.envelope = Some(data);
        self.contents = Some(next);
        self.sync_soon(false);
        self.activity();
        if self.valid_epoch(start).is_err() {
            self.lock();
            return Err(Error::Cancelled);
        }
        Ok(())
    }
    pub fn projection(&mut self, query: &str, category: Option<Uuid>) -> Result<Projection, Error> {
        self.tick();
        let exists = self.exists;
        let has_snapshot = self.has_snapshot;
        let mut p = Projection { exists, has_snapshot, waiting: self.waiting(), preferences: self.preferences, ..Default::default() };
        p.biometric = self.repo.biometric().ok().flatten().is_some();
        p.sync = SyncSummary {
            location: self.sync.location.clone(),
            status: self.sync_status(),
            last_synced_ms: self.sync.last_synced_ms,
            history: self.sync.history.iter().take(20).cloned().collect(),
            due: self.sync_due(),
        };
        let Some(contents) = self.contents.as_ref() else {
            return Ok(p);
        };
        self.index_for_launchers();
        p.unlocked = true;
        let active = |id: &Uuid| contents.0.accounts.iter().any(|a| a.id == *id && !a.is_deleted());
        p.account_count = contents.0.accounts.iter().filter(|a| !a.is_deleted()).count();
        p.categories = contents
            .0
            .categories
            .iter()
            .map(|c| CategoryRow { id: c.id, name: c.name.clone(), count: c.account_ids.iter().filter(|id| active(id)).count() })
            .collect();
        let accounts: Vec<&Account> = match category {
            Some(id) => {
                let cat = contents.0.categories.iter().find(|c| c.id == id).ok_or(Error::NotFound)?;
                cat.account_ids.iter().filter_map(|id| contents.0.accounts.iter().find(|a| a.id == *id)).collect()
            }
            None => contents.0.accounts.iter().collect(),
        };
        let now = self.clock.wall();
        let cats = &contents.0.categories;
        let found = |a: &&Account| {
            search::matches_search(&a.issuer, &a.username, query)
                || a.website.as_deref().is_some_and(|w| search::matches_search(w, "", query))
        };
        let mut deleted: Vec<&Account> = contents.0.accounts.iter().filter(|a| a.is_deleted()).collect();
        p.deleted_count = deleted.len();
        deleted.sort_by_key(|a| std::cmp::Reverse(a.deleted_at));
        p.deleted = deleted
            .into_iter()
            .filter(found)
            .map(|a| DeletedRow {
                id: a.id,
                issuer: a.issuer.clone(),
                username: a.username.clone(),
                days_left: a.deleted_at.map_or(0, |at| trash::days_left(at, now)),
                website: a.website.clone(),
                icon: a.icon.as_deref().map(std::sync::Arc::from),
            })
            .collect();
        p.rows = accounts
            .into_iter()
            .filter(|a| !a.is_deleted())
            .filter(found)
            .map(|a| Row {
                id: a.id,
                issuer: a.issuer.clone(),
                username: a.username.clone(),
                code: totp::formatted(&totp::account_code(a, now)),
                remaining: totp::seconds_remaining(a.period, now),
                period: a.period,
                digits: a.digits,
                recent: now.duration_since(a.created_at).is_ok_and(|age| age < Duration::from_secs(30 * 60)),
                website: a.website.clone(),
                icon: a.icon.as_deref().map(std::sync::Arc::from),
                categories: cats.iter().filter(|c| c.account_ids.contains(&a.id)).map(|c| c.id).collect(),
            })
            .collect();
        Ok(p)
    }
    pub fn save_account(&mut self, draft: &Draft) -> Result<(), Error> {
        self.guard()?;
        let mut next = Sensitive(self.contents.as_ref().ok_or(Error::Locked)?.0.clone());
        let ph = placeholders();
        if draft.period == 0 {
            return Err(Error::Fields);
        }
        let old = draft.id.and_then(|id| next.0.accounts.iter().find(|a| a.id == id));
        if draft.id.is_some() && old.is_none() {
            return Err(Error::NotFound);
        }
        let secret = if draft.secret.is_empty() { old.map(|a| a.secret.as_str()).unwrap_or("") } else { &draft.secret };
        let fields = canonical::fields(&draft.issuer, &draft.username, secret, draft.algorithm, draft.digits, draft.period, &ph)
            .ok_or(Error::Fields)?;
        let website =
            if draft.website.trim().is_empty() { None } else { Some(site_icon::normalized_website(&draft.website).ok_or(Error::Fields)?) };
        let wanted =
            dedup::deduplication_key(&fields.issuer, &fields.username, &fields.secret, fields.algorithm, fields.digits, fields.period, &ph);
        if next.0.accounts.iter().any(|a| Some(a.id) != draft.id && !a.is_deleted() && dedup::account_deduplication_key(a, &ph) == wanted) {
            return Err(Error::Duplicate);
        }
        if let Some(id) = draft.id {
            let a = next.0.accounts.iter_mut().find(|a| a.id == id).ok_or(Error::NotFound)?;
            a.secret.zeroize();
            a.issuer = fields.issuer;
            a.username = fields.username;
            a.secret = fields.secret;
            a.algorithm = fields.algorithm;
            a.digits = fields.digits;
            a.period = fields.period;
            if a.website != website {
                a.icon = None
            }
            a.website = website;
        } else {
            let id = vault::random_uuid()?;
            next.0.accounts.insert(
                0,
                Account {
                    id,
                    issuer: fields.issuer,
                    username: fields.username,
                    secret: fields.secret,
                    algorithm: fields.algorithm,
                    digits: fields.digits,
                    period: fields.period,
                    created_at: self.clock.wall(),
                    added_by: "manual".into(),
                    website,
                    icon: None,
                    deleted_at: None,
                },
            );
            if let Some(cat) = draft.category {
                next.0.categories = category::add_account(&next.0.categories, cat, id).map_err(|_| Error::Category)?;
            }
        }
        self.persist(next)
    }
    pub fn detail(&mut self, id: Uuid) -> Result<Detail, Error> {
        self.guard()?;
        let a = self
            .contents
            .as_ref()
            .ok_or(Error::Locked)?
            .0
            .accounts
            .iter()
            .find(|a| a.id == id && !a.is_deleted())
            .ok_or(Error::NotFound)?;
        Ok(Detail {
            id,
            issuer: a.issuer.clone(),
            username: a.username.clone(),
            website: a.website.clone().unwrap_or_default(),
            algorithm: a.algorithm,
            digits: a.digits,
            period: a.period,
            imported: a.added_by != "manual",
            created_at: a.created_at,
        })
    }
    /// Renames an account or changes its website; the secret and code parameters stay as they are.
    pub fn update_metadata(&mut self, id: Uuid, issuer: &str, username: &str, website: &str) -> Result<(), Error> {
        let mut draft = self.edit_draft(id)?;
        draft.issuer = issuer.to_owned();
        draft.username = username.to_owned();
        draft.website = website.to_owned();
        self.save_account(&draft)
    }
    /// Stores a fetched site icon if the account still has that website and no icon.
    /// Returns whether it was applied. The icon lives only in the encrypted payload.
    pub fn apply_icon(&mut self, id: Uuid, website: &str, png: &[u8]) -> Result<bool, Error> {
        self.guard()?;
        if !site_icon::is_valid_icon(png) {
            return Ok(false);
        }
        let mut next = Sensitive(self.contents.as_ref().ok_or(Error::Locked)?.0.clone());
        let Some(account) = next.0.accounts.iter_mut().find(|a| a.id == id) else { return Ok(false) };
        if account.icon.is_some() || account.website.as_deref() != Some(website) {
            return Ok(false);
        }
        account.icon = Some(png.to_vec());
        self.persist(next)?;
        Ok(true)
    }
    /// Accounts with a website but no icon: what a fetch queue may still try.
    pub fn missing_icons(&mut self) -> Result<Vec<(Uuid, String)>, Error> {
        self.guard()?;
        Ok(self
            .contents
            .as_ref()
            .ok_or(Error::Locked)?
            .0
            .accounts
            .iter()
            .filter(|a| a.icon.is_none() && !a.is_deleted())
            .filter_map(|a| a.website.clone().map(|w| (a.id, w)))
            .collect())
    }
    pub fn choices(&mut self) -> Result<Vec<Choice>, Error> {
        self.guard()?;
        Ok(self
            .contents
            .as_ref()
            .ok_or(Error::Locked)?
            .0
            .accounts
            .iter()
            .filter(|a| !a.is_deleted())
            .map(|a| Choice { id: a.id, issuer: a.issuer.clone(), username: a.username.clone() })
            .collect())
    }
    pub fn edit_draft(&mut self, id: Uuid) -> Result<Draft, Error> {
        self.guard()?;
        let a = self
            .contents
            .as_ref()
            .ok_or(Error::Locked)?
            .0
            .accounts
            .iter()
            .find(|a| a.id == id && !a.is_deleted())
            .ok_or(Error::NotFound)?;
        Ok(Draft {
            id: Some(id),
            issuer: a.issuer.clone(),
            username: a.username.clone(),
            website: a.website.clone().unwrap_or_default(),
            secret: String::new(),
            algorithm: a.algorithm,
            digits: a.digits,
            period: a.period,
            ..Default::default()
        })
    }
    /// Moves an account to Recently Deleted: it keeps its secret, place and categories until it is recovered or removed.
    pub fn delete_account(&mut self, id: Uuid) -> Result<(), Error> {
        self.guard()?;
        let mut next = Sensitive(self.contents.as_ref().ok_or(Error::Locked)?.0.clone());
        let now = self.clock.wall();
        let a = next.0.accounts.iter_mut().find(|a| a.id == id && !a.is_deleted()).ok_or(Error::NotFound)?;
        a.deleted_at = Some(now);
        self.persist(next)
    }
    /// Puts an account from Recently Deleted back where it was.
    pub fn recover_account(&mut self, id: Uuid) -> Result<(), Error> {
        self.guard()?;
        let mut next = Sensitive(self.contents.as_ref().ok_or(Error::Locked)?.0.clone());
        let a = next.0.accounts.iter_mut().find(|a| a.id == id && a.is_deleted()).ok_or(Error::NotFound)?;
        a.deleted_at = None;
        self.persist(next)
    }
    /// Removes accounts from Recently Deleted for good (all of them when `ids` is `None`): their secrets are wiped.
    pub fn remove_deleted(&mut self, ids: Option<&[Uuid]>) -> Result<(), Error> {
        self.guard()?;
        let contents = &self.contents.as_ref().ok_or(Error::Locked)?.0;
        let gone: Vec<Uuid> =
            contents.accounts.iter().filter(|a| a.is_deleted() && ids.is_none_or(|ids| ids.contains(&a.id))).map(|a| a.id).collect();
        if gone.is_empty() {
            return Err(Error::NotFound);
        }
        let next = Sensitive(trash::removing(contents, &gone));
        self.persist(next)
    }
    /// Removes the accounts that have been in Recently Deleted for [`trash::RETENTION`]; `false` when none were due.
    pub fn purge_deleted(&mut self) -> Result<bool, Error> {
        self.guard()?;
        let contents = &self.contents.as_ref().ok_or(Error::Locked)?.0;
        let due = trash::expired(&contents.accounts, self.clock.wall());
        if due.is_empty() {
            return Ok(false);
        }
        let next = Sensitive(trash::removing(contents, &due));
        self.persist(next)?;
        Ok(true)
    }
    pub fn category(&mut self, id: Option<Uuid>, name: &str) -> Result<(), Error> {
        self.guard()?;
        let mut next = Sensitive(self.contents.as_ref().ok_or(Error::Locked)?.0.clone());
        next.0.categories = match id {
            Some(id) => category::rename(&next.0.categories, id, name),
            None => category::create(&next.0.categories, name, vault::random_uuid()?),
        }
        .map_err(|_| Error::Category)?;
        self.persist(next)
    }
    pub fn delete_category(&mut self, id: Uuid) -> Result<(), Error> {
        self.guard()?;
        let mut next = Sensitive(self.contents.as_ref().ok_or(Error::Locked)?.0.clone());
        next.0.categories = category::delete(&next.0.categories, id);
        self.persist(next)
    }
    pub fn membership(&mut self, category_id: Uuid, account: Uuid, add: bool) -> Result<(), Error> {
        self.guard()?;
        let mut next = Sensitive(self.contents.as_ref().ok_or(Error::Locked)?.0.clone());
        if !next.0.accounts.iter().any(|a| a.id == account) {
            return Err(Error::NotFound);
        }
        next.0.categories = if add {
            category::add_account(&next.0.categories, category_id, account)
        } else {
            category::remove_account(&next.0.categories, category_id, account)
        }
        .map_err(|_| Error::Category)?;
        self.persist(next)
    }
    pub fn reorder_categories(&mut self, ids: &[Uuid]) -> Result<(), Error> {
        self.guard()?;
        let mut next = Sensitive(self.contents.as_ref().ok_or(Error::Locked)?.0.clone());
        let current: std::collections::HashSet<_> = next.0.categories.iter().map(|c| c.id).collect();
        if ids.len() != current.len() || ids.iter().copied().collect::<std::collections::HashSet<_>>() != current {
            return Err(Error::Fields);
        }
        next.0.categories = category::reorder(&next.0.categories, ids);
        self.persist(next)
    }
    /// Sets the order of a section's accounts. `ids` may be only the shown ones (a search narrows the rows): they take
    /// the places those accounts held, in the new order, and the hidden ones keep theirs.
    pub fn reorder(&mut self, category_id: Option<Uuid>, ids: &[Uuid]) -> Result<(), Error> {
        self.guard()?;
        let mut next = Sensitive(self.contents.as_ref().ok_or(Error::Locked)?.0.clone());
        if let Some(id) = category_id {
            let members = next.0.categories.iter().find(|c| c.id == id).ok_or(Error::Category)?.account_ids.clone();
            let ordered = in_slots(&members, ids).ok_or(Error::Fields)?;
            next.0.categories = category::set_account_order(&next.0.categories, id, &ordered).map_err(|_| Error::Category)?
        } else {
            let current: Vec<_> = next.0.accounts.iter().map(|a| a.id).collect();
            let ordered = in_slots(&current, ids).ok_or(Error::Fields)?;
            next.0.accounts.sort_by_key(|a| ordered.iter().position(|id| *id == a.id));
        }
        self.persist(next)
    }
    pub fn reveal(&mut self, id: Uuid, password: &str, uri: bool) -> Result<Zeroizing<String>, Error> {
        self.verify(password)?;
        let a = self
            .contents
            .as_ref()
            .ok_or(Error::Locked)?
            .0
            .accounts
            .iter()
            .find(|a| a.id == id && !a.is_deleted())
            .ok_or(Error::NotFound)?;
        Ok(Zeroizing::new(if uri { canonical::otpauth_url(a, &placeholders()) } else { a.secret.clone() }))
    }
    /// One password challenge protects both views of the same account.
    pub fn reveal_pair(&mut self, id: Uuid, password: &str) -> Result<(Zeroizing<String>, Zeroizing<String>), Error> {
        self.verify(password)?;
        let account = self
            .contents
            .as_ref()
            .ok_or(Error::Locked)?
            .0
            .accounts
            .iter()
            .find(|a| a.id == id && !a.is_deleted())
            .ok_or(Error::NotFound)?;
        Ok((Zeroizing::new(account.secret.clone()), Zeroizing::new(canonical::otpauth_url(account, &placeholders()))))
    }
    pub fn copy_code(&mut self, id: Uuid) -> Result<(Zeroizing<String>, Duration), Error> {
        self.guard()?;
        let a = self
            .contents
            .as_ref()
            .ok_or(Error::Locked)?
            .0
            .accounts
            .iter()
            .find(|a| a.id == id && !a.is_deleted())
            .ok_or(Error::NotFound)?;
        let result = (Zeroizing::new(totp::account_code(a, self.clock.wall())), Duration::from_secs(u64::from(a.period)));
        self.activity();
        Ok(result)
    }
    pub fn change_password(&mut self, current: &str, new: &str, confirmed: bool) -> Result<(), Error> {
        self.verify(current)?;
        Self::acceptable(new, confirmed)?;
        let start = self.epoch;
        let next = Sensitive(self.contents.as_ref().ok_or(Error::Locked)?.0.clone());
        let (data, key) = vault::create(new, &next.0, self.clock.wall())?;
        let _ = Sensitive(vault::open(&data, &key)?);
        self.valid_epoch(start)?;
        let old = self.envelope.as_deref().ok_or(Error::Locked)?;
        self.repo.commit(Some(old), &data, Some(old))?;
        self.has_snapshot = true;
        // The wrapped key belongs to the old password; the platform wraps the new one after this returns.
        let _ = self.repo.set_biometric(None);
        self.adopt(data, next, key, start)
    }
    pub fn export(&mut self, current: &str, password: &str, confirmed: bool) -> Result<Vec<u8>, Error> {
        self.export_selected(current, None, password, confirmed)
    }
    /// Exports the chosen accounts (all when `ids` is `None`), keeping only their category memberships.
    pub fn export_selected(&mut self, current: &str, ids: Option<&[Uuid]>, password: &str, confirmed: bool) -> Result<Vec<u8>, Error> {
        Self::acceptable(password, confirmed)?;
        self.verify(current)?;
        let contents = &self.contents.as_ref().ok_or(Error::Locked)?.0;
        if ids.is_some_and(<[Uuid]>::is_empty) {
            return Err(Error::Fields);
        }
        // Accounts in Recently Deleted are never exported.
        let wanted = |a: &Account| !a.is_deleted() && ids.is_none_or(|ids| ids.contains(&a.id));
        let chosen = Sensitive(VaultContents {
            accounts: contents.accounts.iter().filter(|a| wanted(a)).cloned().collect(),
            categories: category::removing_accounts(
                &contents.categories,
                &contents.accounts.iter().filter(|a| !wanted(a)).map(|a| a.id).collect::<Vec<_>>(),
            ),
            ..Default::default()
        });
        Ok(vault::export(&chosen.0.accounts, &chosen.0.categories, password, self.clock.wall())?)
    }
    pub fn import_file(&mut self, data: &[u8], password: &str) -> Result<usize, Error> {
        self.guard()?;
        if data.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::TooLarge);
        }
        let imported = Sensitive(
            authenticator::import_file(data, password, self.clock.wall(), &placeholders())
                .map_err(import_error)?
                .adopting_pinned("Pinned", vault::random_uuid)?,
        );
        self.merge(&imported.0)
    }
    pub fn import_text(&mut self, text: &str) -> Result<usize, Error> {
        self.guard()?;
        if text.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::TooLarge);
        }
        let scans: Vec<_> =
            text.lines().filter(|line| !line.trim().is_empty()).map(|line| otpauth::parse_scan_result(line, &placeholders())).collect();
        let mut groups: std::collections::HashMap<(i64, i64), std::collections::HashSet<i64>> = std::collections::HashMap::new();
        for scan in &scans {
            if scan.is_google_migration && scan.batch_size.unwrap_or(1) > 1 {
                let size = scan.batch_size.ok_or(Error::Import)?;
                let index = scan.batch_index.ok_or(Error::Import)?;
                if size > 10000 || index < 0 || index >= size {
                    return Err(Error::Import);
                }
                groups.entry((scan.batch_id.unwrap_or(0), size)).or_default().insert(index);
            }
        }
        if groups.iter().any(|((_, size), indices)| indices.len() as i64 != *size) {
            return Err(Error::Import);
        }
        let parsed: Vec<_> = scans.into_iter().flat_map(|scan| scan.accounts).collect();
        if parsed.is_empty() || parsed.iter().any(|p| !p.is_valid()) {
            return Err(Error::Import);
        }
        let accounts = parsed
            .into_iter()
            .map(|p| Account {
                id: Uuid::nil(),
                issuer: p.issuer,
                username: p.account,
                secret: p.secret,
                algorithm: p.algorithm,
                digits: p.digits,
                period: p.period,
                created_at: self.clock.wall(),
                added_by: "import".into(),
                website: None,
                icon: None,
                deleted_at: None,
            })
            .collect();
        self.merge(&Sensitive(VaultContents { accounts, ..Default::default() }).0)
    }
    /// For each valid account in `text` (otpauth lines and migration codes, in [`otpauth::parse_lines`] order): whether
    /// the vault already has it, by the same rule the import skips duplicates with. Nothing is stored or changed.
    pub fn known_accounts(&mut self, text: &str) -> Result<Vec<bool>, Error> {
        self.guard()?;
        if text.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::TooLarge);
        }
        let placeholders = placeholders();
        let contents = self.contents.as_ref().ok_or(Error::Locked)?;
        let mut keys: std::collections::HashSet<String> =
            contents.0.accounts.iter().filter(|a| !a.is_deleted()).map(|a| dedup::account_deduplication_key(a, &placeholders)).collect();
        let known = otpauth::parse_lines(text, &placeholders)
            .into_iter()
            .filter(|p| p.is_valid())
            .map(|mut p| {
                let key = Zeroizing::new(dedup::deduplication_key(
                    &p.issuer,
                    &p.account,
                    &p.secret,
                    p.algorithm,
                    p.digits,
                    p.period,
                    &placeholders,
                ));
                p.secret.zeroize();
                keys.contains(key.as_str())
            })
            .collect();
        // The keys hold the secrets.
        for mut key in keys.drain() {
            key.zeroize();
        }
        Ok(known)
    }
    fn merge(&mut self, imported: &VaultContents) -> Result<usize, Error> {
        let mut next = Sensitive(self.contents.as_ref().ok_or(Error::Locked)?.0.clone());
        let candidates: Vec<_> = imported
            .accounts
            .iter()
            .map(|a| dedup::Candidate {
                issuer: a.issuer.clone(),
                username: a.username.clone(),
                secret: a.secret.clone(),
                algorithm: a.algorithm,
                digits: a.digits,
                period: a.period,
                category_names: imported.categories.iter().filter(|c| c.account_ids.contains(&a.id)).map(|c| c.name.clone()).collect(),
                created_at: Some(a.created_at),
                website: a.website.clone(),
                icon: a.icon.clone(),
            })
            .collect();
        let result =
            dedup::insert_accounts(&next.0.accounts, &next.0.categories, &candidates, "import", self.clock.wall(), &placeholders());
        for mut c in candidates {
            c.secret.zeroize();
        }
        let result = result?;
        let count = result.inserted_ids.len();
        if count == 0 {
            return Err(Error::Duplicate);
        }
        next.0.accounts = result.accounts;
        next.0.categories = result.categories;
        self.persist(next)?;
        Ok(count)
    }
    pub fn restore_snapshot(&mut self, password: &str) -> Result<(), Error> {
        self.before_password()?;
        let start = self.gate.load(Ordering::SeqCst);
        let data = self.repo.snapshot()?.ok_or(Error::Missing)?;
        let opened = match vault::unlock(&data, password) {
            Ok(v) => v,
            Err(e) => {
                if e == VaultError::InvalidPassword {
                    self.failed()?;
                }
                return Err(e.into());
            }
        };
        let contents = Sensitive(opened.contents);
        self.valid_epoch(start)?;
        self.repo.restore(&data)?;
        let _ = self.repo.set_biometric(None);
        self.exists = true;
        self.succeeded()?;
        self.adopt(data, contents, opened.key, start)
    }
    pub fn reset(&mut self, password: &str) -> Result<(), Error> {
        self.verify(password)?;
        self.repo.reset(self.envelope.as_deref().ok_or(Error::Locked)?)?;
        self.gate.fetch_add(1, Ordering::SeqCst);
        self.lock();
        self.failures = 0;
        self.exists = false;
        self.has_snapshot = false;
        self.preferences = Default::default();
        self.auto_lock = Duration::from_secs(300);
        self.sync = Default::default();
        self.sync_status = crate::sync::Status::Off;
        self.launcher.set_enabled(false);
        Ok(())
    }
}
pub fn placeholders() -> Placeholders {
    Placeholders { unknown_issuer: "Unknown service".into(), empty_username: "No account name".into() }
}

/// `current` with the accounts listed in `ordered` put back into the places they held, in `ordered`'s order; `None` when
/// `ordered` names an account not in `current`, or one twice.
fn in_slots(current: &[Uuid], ordered: &[Uuid]) -> Option<Vec<Uuid>> {
    let listed: std::collections::HashSet<Uuid> = ordered.iter().copied().collect();
    if listed.len() != ordered.len() || !ordered.iter().all(|id| current.contains(id)) {
        return None;
    }
    let mut next = ordered.iter().copied();
    Some(current.iter().map(|id| if listed.contains(id) { next.next().unwrap_or(*id) } else { *id }).collect())
}

#[cfg(test)]
mod order_tests {
    use super::*;
    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }
    #[test]
    fn shown_accounts_take_the_places_they_held() {
        let all = [id(1), id(2), id(3), id(4), id(5)];
        assert_eq!(in_slots(&all, &[id(4), id(2)]).unwrap(), vec![id(1), id(4), id(3), id(2), id(5)]);
        assert_eq!(in_slots(&all, &[id(5), id(4), id(3), id(2), id(1)]).unwrap(), vec![id(5), id(4), id(3), id(2), id(1)]);
        assert!(in_slots(&all, &[id(2), id(2)]).is_none());
        assert!(in_slots(&all, &[id(9)]).is_none());
    }
}
