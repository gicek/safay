//! Backup and sync: one location at a time; the core (`safay_core::sync`)
//! decides upload / download / conflict / unreadable, and backends only read and write the envelope and metadata.
//!
//! Remote IO never runs while the session is held: an orchestration reads the remote, takes the session briefly to
//! decide (the only place the vault key is used), writes outside it, then takes it again to record marks and history.
//! A lock in between cancels the run (epoch check). Only the local key is trusted: a changed remote this key cannot open
//! is never uploaded over, downloaded, or offered as a conflict; only an explicit restore with its password replaces it.
use crate::{
    controller::Controller,
    session::{Error, Session, placeholders},
    storage::{StorageError, atomic_write, bounded_read},
};
use safay_core::{
    backup::{BackupMetadata, DEFAULT_FOLDER, METADATA_FILE_NAME, VAULT_FILE_NAME},
    merge::{self, AccountChange},
    sync::{self, LocalSnapshot, RemoteSnapshot, RemoteVersion, SyncDecision, SyncMarks},
    vault,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub const MAX_HISTORY: usize = 50;
/// Consecutive edits coalesce into one sync this long after the last save (2 seconds).
pub const DEBOUNCE: Duration = Duration::from_secs(2);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Location {
    /// A folder on disk: iCloud Drive, a Dropbox/OneDrive/Nextcloud client folder, a network share.
    Folder { path: PathBuf },
    /// Cloud storage reached over HTTPS (`cloudset`). The sign-in lives only in the system credential store under
    /// `credential`; `account` is what the status shows (an email, a user name or a host). `pinned`: the server's
    /// certificate is one the user confirmed, not one the system trusts (the status keeps saying so).
    Cloud {
        provider: crate::cloud::Provider,
        account: String,
        credential: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        pinned: bool,
    },
    /// The CloudKit private database (`crate::icloud`), with the iCloud account at link time (an opaque per-container
    /// user ID). If the device switches accounts, syncing stops until iCloud is chosen again.
    #[serde(rename = "icloud")]
    ICloud { account: String },
}
impl From<PathBuf> for Location {
    fn from(path: PathBuf) -> Self {
        Location::Folder { path }
    }
}
impl Location {
    /// A server certificate the user confirmed is pinned (WebDAV, Nextcloud): shown as "certificate not verified".
    pub fn unverified_certificate(&self) -> bool {
        matches!(self, Location::Cloud { pinned: true, .. })
    }
    /// The name shown in status and history.
    pub fn title(&self) -> String {
        match self {
            Location::Folder { path } => {
                path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
            }
            Location::Cloud { provider, .. } => provider.name().into(),
            Location::ICloud { .. } => "iCloud".into(),
        }
    }
    /// The second line. Empty for iCloud: the UI shows "Private to your Apple Account" instead.
    pub fn detail(&self) -> String {
        match self {
            Location::Folder { path } => path.display().to_string(),
            Location::Cloud { account, .. } => account.clone(),
            Location::ICloud { .. } => String::new(),
        }
    }
    pub fn backend(&self) -> Box<dyn Backend> {
        match self {
            Location::Folder { path } => Box::new(FolderBackend { folder: path.clone() }),
            Location::Cloud { credential, .. } => Box::new(crate::cloud::CloudBackend::new(credential.clone())),
            Location::ICloud { account } => Box::new(crate::icloud::ICloudBackend::new(account.clone())),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Auto,
    Manual,
    Merge,
    Restore,
}
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    Upload,
    Download,
    Unchanged,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub at_ms: i64,
    pub source: Source,
    pub direction: Option<Direction>,
    pub failed: bool,
    pub device: String,
    pub bytes: Option<u64>,
    pub location: String,
}

/// Persisted, non-sensitive backup state (`sync.json`).
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct SyncState {
    pub location: Option<Location>,
    pub last_local_updated_at_ms: Option<i64>,
    pub last_remote_version: Option<String>,
    pub last_synced_ms: Option<i64>,
    pub history: Vec<HistoryEntry>,
}
impl SyncState {
    pub fn decode(data: &[u8]) -> Self {
        serde_json::from_slice(data).unwrap_or_default()
    }
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }
    pub fn marks(&self) -> SyncMarks {
        SyncMarks {
            last_local_updated_at: self.last_local_updated_at_ms.and_then(from_ms),
            last_remote_version: self.last_remote_version.as_deref().and_then(RemoteVersion::decode),
        }
    }
    fn set_marks(&mut self, local: SystemTime, remote: Option<&RemoteVersion>) {
        self.last_local_updated_at_ms = Some(to_ms(local));
        self.last_remote_version = remote.map(RemoteVersion::encode);
    }
    fn push(&mut self, entry: HistoryEntry) {
        self.history.insert(0, entry);
        self.history.truncate(MAX_HISTORY);
    }
}

/// Runtime state shown by the status card and the sidebar dot. Not persisted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Off,
    Idle,
    Running,
    Synced,
    Conflict,
    Unreadable,
    Failed(SyncError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SyncError {
    #[error("the backup folder can't be reached")]
    FolderUnavailable,
    #[error("the backup file is invalid")]
    Corrupt,
    #[error("no Safay backup was found there")]
    NoRemote,
    #[error("the backup can't be opened with this vault's key")]
    Unreadable,
    #[error("the backup changed during the sync")]
    RemoteChanged,
    #[error("no local vault to back up")]
    NoLocal,
    #[error("the vault is locked")]
    Locked,
    #[error("the backup is larger than the file limit")]
    TooLarge,
    #[error("the local vault couldn't be saved")]
    Storage,
    #[error("wrong password for this backup")]
    Password,
    /// The server could not be reached: offline, timeout, DNS, or a TLS failure other than the certificate.
    #[error("the server can't be reached")]
    Network,
    #[error("the server's certificate isn't trusted")]
    Certificate,
    /// The system does not trust the server's certificate; its SHA-256 fingerprint, for the user to confirm while setting
    /// up the location (a linked location reports it as [`SyncError::Certificate`]).
    #[error("the server's certificate can't be verified")]
    CertificateUnverified([u8; 32]),
    #[error("the server's certificate has changed")]
    CertificateChanged,
    /// Sign in again: the token or app password is no longer valid, or the saved sign-in is missing.
    #[error("sign in again")]
    SignIn,
    #[error("the server denied access")]
    Forbidden,
    #[error("not enough storage on the server")]
    Quota,
    #[error("the server is busy")]
    RateLimited,
    #[error("the server returned an error")]
    Server(u16),
    #[error("the server's response couldn't be read")]
    Protocol,
    #[error("iCloud can't be used: {0:?}")]
    ICloud(crate::icloud::ICloudError),
}
impl From<StorageError> for SyncError {
    fn from(e: StorageError) -> Self {
        match e {
            StorageError::TooLarge => Self::TooLarge,
            _ => Self::FolderUnavailable,
        }
    }
}
impl From<Error> for SyncError {
    fn from(e: Error) -> Self {
        match e {
            Error::Locked | Error::Cancelled => Self::Locked,
            Error::Password => Self::Password,
            Error::Corrupt => Self::Corrupt,
            Error::TooLarge => Self::TooLarge,
            _ => Self::Storage,
        }
    }
}

/// A remote backup as read: ciphertext only. `Debug` is deliberately absent.
#[derive(Clone, PartialEq, Eq)]
pub struct Remote {
    pub data: Vec<u8>,
    pub version: Option<RemoteVersion>,
    pub device: Option<String>,
}
/// What the remote must still be for a write to proceed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expect {
    Any,
    Absent,
    Unchanged(RemoteVersion),
}

/// Reads and writes one location. Implementations hold no vault key and see only ciphertext.
pub trait Backend: Send + Sync {
    fn read(&self) -> Result<Option<Remote>, SyncError>;
    fn write(&self, data: &[u8], metadata: &BackupMetadata, expect: &Expect) -> Result<Option<RemoteVersion>, SyncError>;
}

/// `vault.safayvault` and `vault.metadata.json` in a folder.
/// File systems have no conditional writes: the core detects concurrent changes by modification time.
pub struct FolderBackend {
    folder: PathBuf,
}
impl FolderBackend {
    pub fn new(folder: PathBuf) -> Self {
        Self { folder }
    }
    /// The picked folder if it is the backup folder itself (named `Safay`) or already holds a backup, otherwise its
    /// `Safay/` subfolder.
    pub fn resolve(picked: &Path) -> PathBuf {
        if picked.file_name().is_some_and(|n| n == DEFAULT_FOLDER) || picked.join(VAULT_FILE_NAME).is_file() {
            picked.to_path_buf()
        } else {
            picked.join(DEFAULT_FOLDER)
        }
    }
    fn modified(path: &Path) -> Option<RemoteVersion> {
        std::fs::metadata(path).ok()?.modified().ok().map(|at| RemoteVersion::ModifiedAt { at })
    }
}
impl Backend for FolderBackend {
    fn read(&self) -> Result<Option<Remote>, SyncError> {
        if let Some(parent) = self.folder.parent()
            && !parent.as_os_str().is_empty()
            && !parent.is_dir()
        {
            return Err(SyncError::FolderUnavailable);
        }
        let path = self.folder.join(VAULT_FILE_NAME);
        let Some(data) = bounded_read(&path)? else { return Ok(None) };
        if !vault::is_valid_envelope(&data) {
            return Err(SyncError::Corrupt);
        }
        let device = bounded_read(&self.folder.join(METADATA_FILE_NAME)).ok().flatten().and_then(|m| BackupMetadata::device_name(&m));
        Ok(Some(Remote { version: Self::modified(&path), data, device }))
    }
    fn write(&self, data: &[u8], metadata: &BackupMetadata, _expect: &Expect) -> Result<Option<RemoteVersion>, SyncError> {
        std::fs::create_dir_all(&self.folder).map_err(|_| SyncError::FolderUnavailable)?;
        let path = self.folder.join(VAULT_FILE_NAME);
        atomic_write(&path, data)?;
        // Metadata is only for display; a failed write does not fail the backup.
        let _ = atomic_write(&self.folder.join(METADATA_FILE_NAME), &metadata.encode());
        Ok(Self::modified(&path))
    }
}

/// A conflict summary for the review page: names and kinds of change, never secrets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Review {
    pub local_count: usize,
    pub remote_count: usize,
    pub remote_device: Option<String>,
    pub only_local: Vec<(String, String)>,
    pub only_remote: Vec<(String, String)>,
    /// Local account ID, issuer, username and what differs.
    pub changed: Vec<(Uuid, String, String, Vec<AccountChange>)>,
    pub categories_only_local: Vec<String>,
    pub categories_only_remote: Vec<String>,
    pub categories_renamed: Vec<(String, String)>,
}

pub fn to_ms(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    }
}
fn from_ms(ms: i64) -> Option<SystemTime> {
    if ms >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_millis(ms as u64))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_millis(ms.unsigned_abs()))
    }
}

// ------------------------------------------------------------------ session side

impl Session {
    pub fn sync_state(&self) -> &SyncState {
        &self.sync
    }
    pub fn sync_status(&self) -> Status {
        if self.sync.location.is_none() { Status::Off } else { self.sync_status }
    }
    fn save_sync(&mut self) -> Result<(), SyncError> {
        let data = self.sync.encode();
        self.repo.set_sync_state(Some(&data)).map_err(|_| SyncError::Storage)
    }
    /// A sync should start now: a location is set, the vault is open, and a save (or unlock) is older than the debounce.
    pub fn sync_due(&self) -> bool {
        self.sync.location.is_some()
            && self.contents.is_some()
            && self.sync_status != Status::Running
            && self.sync_dirty.is_some_and(|(at, now)| now || self.clock.elapsed().saturating_sub(at) >= DEBOUNCE)
    }
    /// Unlocking or linking syncs at once; saves wait for the debounce.
    pub(crate) fn sync_soon(&mut self, immediately: bool) {
        if self.sync.location.is_some() {
            self.sync_dirty = Some((self.clock.elapsed(), immediately));
        }
    }
    fn begin(&mut self) -> Result<Location, SyncError> {
        if self.contents.is_none() {
            return Err(SyncError::Locked);
        }
        let location = self.sync.location.clone().ok_or(SyncError::NoLocal)?;
        self.sync_status = Status::Running;
        self.sync_dirty = None;
        Ok(location)
    }
    fn local(&self) -> Option<LocalSnapshot> {
        let data = self.envelope.clone()?;
        let info = vault::envelope_info(&data).ok()?;
        Some(LocalSnapshot { data, vault_updated_at: info.updated_at })
    }
    fn opens(&self, data: &[u8]) -> bool {
        self.key.as_ref().is_some_and(|key| vault::open(data, key).is_ok())
    }
    fn record(&mut self, source: Source, direction: Option<Direction>, device: Option<String>, bytes: Option<u64>) {
        let now = self.clock.wall();
        let location = self.sync.location.as_ref().map(Location::title).unwrap_or_default();
        let device = device.unwrap_or_else(|| location.clone());
        if direction.is_some() {
            self.sync.last_synced_ms = Some(to_ms(now));
        }
        self.sync.push(HistoryEntry { at_ms: to_ms(now), source, direction, failed: false, device, bytes, location });
    }
    fn fail(&mut self, error: SyncError, source: Source, device: &str) {
        if error == SyncError::Unreadable {
            self.sync_status = Status::Unreadable;
            return;
        }
        if error == SyncError::Locked {
            self.sync_status = Status::Idle;
            return;
        }
        self.sync_status = Status::Failed(error);
        let location = self.sync.location.as_ref().map(Location::title).unwrap_or_default();
        self.sync.push(HistoryEntry {
            at_ms: to_ms(self.clock.wall()),
            source,
            direction: None,
            failed: true,
            device: device.to_owned(),
            bytes: None,
            location,
        });
        let _ = self.save_sync();
    }
    /// The remote replaces the local vault. It must open with the current key (a decided download always does).
    fn adopt_remote(&mut self, remote: &Remote) -> Result<SystemTime, SyncError> {
        let key = self.key.as_ref().ok_or(SyncError::Locked)?;
        let contents = crate::session::Sensitive(vault::open(&remote.data, key).map_err(|_| SyncError::Unreadable)?);
        let updated = vault::envelope_info(&remote.data).map_err(|_| SyncError::Corrupt)?.updated_at;
        self.replace_with(remote.data.clone(), contents, None).map_err(SyncError::from)?;
        Ok(updated)
    }
    fn plan(&mut self, remote: Option<&Remote>) -> Result<Plan, SyncError> {
        let local = self.local();
        let readable = remote.is_some_and(|r| self.opens(&r.data));
        let snapshot = remote.map(|r| RemoteSnapshot { data: r.data.clone(), version: r.version.clone(), readable });
        let decision = sync::decide(local.as_ref(), snapshot.as_ref(), &self.sync.marks());
        Ok(match (decision, local, remote) {
            (SyncDecision::Upload, Some(local), _) => {
                let expect = match remote {
                    None => Expect::Absent,
                    Some(r) => r.version.clone().map(Expect::Unchanged).unwrap_or(Expect::Any),
                };
                Plan::Upload(local, expect)
            }
            (SyncDecision::Download, _, Some(remote)) => {
                let updated = self.adopt_remote(remote)?;
                self.sync.set_marks(updated, remote.version.as_ref());
                Plan::Downloaded
            }
            (SyncDecision::UpToDate, Some(local), _) => {
                self.sync.set_marks(local.vault_updated_at, remote.and_then(|r| r.version.as_ref()));
                Plan::UpToDate
            }
            (SyncDecision::Conflict, _, Some(remote)) => {
                self.conflict = Some(remote.clone());
                Plan::Conflict
            }
            (SyncDecision::Unreadable, _, _) => Plan::Unreadable,
            _ => return Err(SyncError::NoLocal),
        })
    }
    fn finish_upload(
        &mut self,
        local: &LocalSnapshot,
        version: Option<&RemoteVersion>,
        source: Source,
        device: &str,
    ) -> Result<(), SyncError> {
        self.sync.set_marks(local.vault_updated_at, version);
        self.conflict = None;
        self.record(source, Some(Direction::Upload), Some(device.to_owned()), Some(local.data.len() as u64));
        self.sync_status = Status::Synced;
        self.save_sync()
    }
    /// The conflict page: what differs between this vault and the backup that caused the conflict.
    pub fn conflict_review(&self) -> Result<Review, Error> {
        let remote = self.conflict.as_ref().ok_or(Error::NotFound)?;
        let local = self.contents.as_ref().ok_or(Error::Locked)?;
        let key = self.key.as_ref().ok_or(Error::Locked)?;
        let theirs = crate::session::Sensitive(vault::open(&remote.data, key).map_err(|_| Error::Crypto)?);
        let diff = merge::diff(&local.0, &theirs.0, &placeholders());
        let pair = |a: &safay_core::Account| (a.issuer.clone(), a.username.clone());
        Ok(Review {
            local_count: local.0.accounts.len(),
            remote_count: theirs.0.accounts.len(),
            remote_device: remote.device.clone(),
            only_local: diff.only_local.iter().map(pair).collect(),
            only_remote: diff.only_remote.iter().map(pair).collect(),
            changed: diff
                .changed
                .iter()
                .map(|c| (c.local.id, c.local.issuer.clone(), c.local.username.clone(), c.changes.clone()))
                .collect(),
            categories_only_local: diff.categories_only_local.clone(),
            categories_only_remote: diff.categories_only_remote.clone(),
            categories_renamed: diff.categories_renamed.clone(),
        })
    }
    /// Turns backup off. Remote backups are never deleted; history stays for reference.
    pub fn sync_off(&mut self) -> Result<(), Error> {
        self.sync.location = None;
        self.sync.last_local_updated_at_ms = None;
        self.sync.last_remote_version = None;
        self.sync_status = Status::Off;
        self.sync_dirty = None;
        self.conflict = None;
        self.save_sync().map_err(|_| Error::Storage)
    }
}

enum Plan {
    Upload(LocalSnapshot, Expect),
    Downloaded,
    UpToDate,
    Conflict,
    Unreadable,
}

// ------------------------------------------------------------------ orchestration

fn metadata(device: &str, local: &LocalSnapshot, now: SystemTime) -> BackupMetadata {
    BackupMetadata { device_name: device.to_owned(), vault_updated_at: local.vault_updated_at, saved_at: now }
}
fn session_error(e: Error) -> SyncError {
    e.into()
}

/// Links a location after a read-only check: a different backup there that this key cannot open is refused, so another
/// vault's backup is never overwritten. On success the first sync runs right away (and may become a conflict).
pub fn link(controller: &Controller, epoch: u64, location: Location) -> Result<(), SyncError> {
    let remote = location.backend().read()?;
    controller
        .run(epoch, |s| {
            if s.contents.is_none() {
                return Ok(Err(SyncError::Locked));
            }
            if let Some(remote) = &remote
                && s.envelope.as_deref() != Some(&remote.data[..])
                && !s.opens(&remote.data)
            {
                return Ok(Err(SyncError::Unreadable));
            }
            s.sync.location = Some(location);
            s.sync.last_local_updated_at_ms = None;
            s.sync.last_remote_version = None;
            s.sync_status = Status::Idle;
            s.conflict = None;
            s.sync_soon(true);
            Ok(s.save_sync())
        })
        .map_err(session_error)?
}

/// Automatic or "Back Up Now" sync. Retries once if the remote changed between the decision and the write.
pub fn run(controller: &Controller, epoch: u64, device: &str, source: Source) -> Result<Status, SyncError> {
    let location = controller.run(epoch, |s| Ok(s.begin())).map_err(session_error)??;
    let backend = location.backend();
    let result = (|| {
        for attempt in 0..2 {
            let remote = backend.read()?;
            let plan = controller.run(epoch, |s| Ok(s.plan(remote.as_ref()))).map_err(session_error)??;
            match plan {
                Plan::Upload(local, expect) => {
                    let now = SystemTime::now();
                    match backend.write(&local.data, &metadata(device, &local, now), &expect) {
                        Ok(version) => {
                            controller
                                .run(epoch, |s| Ok(s.finish_upload(&local, version.as_ref(), source, device)))
                                .map_err(session_error)??;
                            return Ok(Status::Synced);
                        }
                        Err(SyncError::RemoteChanged) if attempt == 0 => continue,
                        Err(e) => return Err(e),
                    }
                }
                Plan::Downloaded => {
                    let bytes = remote.as_ref().map(|r| r.data.len() as u64);
                    let device = remote.as_ref().and_then(|r| r.device.clone());
                    controller
                        .run(epoch, |s| {
                            s.record(source, Some(Direction::Download), device, bytes);
                            s.sync_status = Status::Synced;
                            Ok(s.save_sync())
                        })
                        .map_err(session_error)??;
                    return Ok(Status::Synced);
                }
                Plan::UpToDate => {
                    let bytes = remote.as_ref().map(|r| r.data.len() as u64);
                    let device = remote.as_ref().and_then(|r| r.device.clone());
                    controller
                        .run(epoch, |s| {
                            // Unlocking into an identical backup records nothing; a user's "Back Up Now" gets a result.
                            if source != Source::Auto {
                                s.record(source, Some(Direction::Unchanged), device, bytes);
                            } else {
                                s.sync.last_synced_ms = Some(to_ms(s.clock.wall()));
                            }
                            s.sync_status = Status::Synced;
                            Ok(s.save_sync())
                        })
                        .map_err(session_error)??;
                    return Ok(Status::Synced);
                }
                Plan::Conflict => {
                    controller
                        .run(epoch, |s| {
                            s.sync_status = Status::Conflict;
                            Ok(())
                        })
                        .map_err(session_error)?;
                    return Ok(Status::Conflict);
                }
                Plan::Unreadable => return Err(SyncError::Unreadable),
            }
        }
        Err(SyncError::RemoteChanged)
    })();
    match result {
        Ok(status) => Ok(status),
        Err(e) => {
            let _ = controller.run(epoch, |s| {
                s.fail(e, source, device);
                Ok(())
            });
            Err(e)
        }
    }
}

/// Conflict: keep this device's copy. Refuses when the backup changed since the last sync and this key can't open it.
pub fn keep_local(controller: &Controller, epoch: u64, device: &str) -> Result<(), SyncError> {
    let location = controller.run(epoch, |s| Ok(s.begin())).map_err(session_error)??;
    let backend = location.backend();
    let result = (|| {
        let remote = backend.read()?;
        let local = controller
            .run(epoch, |s| {
                let local = s.local().ok_or(SyncError::NoLocal);
                let Ok(local) = local else { return Ok(local) };
                if let Some(r) = &remote
                    && (r.version.is_none() || r.version.as_ref().map(RemoteVersion::encode) != s.sync.last_remote_version)
                    && r.data != local.data
                    && !s.opens(&r.data)
                {
                    return Ok(Err(SyncError::Unreadable));
                }
                Ok(Ok(local))
            })
            .map_err(session_error)??;
        let version = backend.write(&local.data, &metadata(device, &local, SystemTime::now()), &Expect::Any)?;
        controller.run(epoch, |s| Ok(s.finish_upload(&local, version.as_ref(), Source::Manual, device))).map_err(session_error)?
    })();
    if let Err(e) = result {
        let _ = controller.run(epoch, |s| {
            s.fail(e, Source::Manual, device);
            Ok(())
        });
    }
    result
}

/// Conflict: use the backup's copy. A conflicting backup always opens with this key.
pub fn use_backup(controller: &Controller, epoch: u64, device: &str) -> Result<(), SyncError> {
    let location = controller.run(epoch, |s| Ok(s.begin())).map_err(session_error)??;
    let result = (|| {
        let remote = location.backend().read()?.ok_or(SyncError::NoRemote)?;
        controller
            .run(epoch, |s| {
                let updated = s.adopt_remote(&remote);
                let Ok(updated) = updated else { return Ok(updated.map(|_| ())) };
                s.sync.set_marks(updated, remote.version.as_ref());
                s.conflict = None;
                s.record(Source::Restore, Some(Direction::Download), remote.device.clone(), Some(remote.data.len() as u64));
                s.sync_status = Status::Synced;
                Ok(s.save_sync())
            })
            .map_err(session_error)?
    })();
    if let Err(e) = result {
        let _ = controller.run(epoch, |s| {
            s.fail(e, Source::Restore, device);
            Ok(())
        });
    }
    result
}

/// Conflict: merge both. Accounts on both sides that differ use the backup's version for `prefer_remote`. Saves locally
/// first, then writes the backup only if it is still the compared copy; otherwise the next sync decides again.
pub fn merge_both(controller: &Controller, epoch: u64, device: &str, prefer_remote: HashSet<Uuid>) -> Result<(), SyncError> {
    let location = controller.run(epoch, |s| Ok(s.begin())).map_err(session_error)??;
    let result = (|| {
        let (local, expect) = controller
            .run(epoch, |s| {
                let remote = s.conflict.clone().ok_or(Error::NotFound)?;
                let key = s.key.as_ref().ok_or(Error::Locked)?;
                let theirs = crate::session::Sensitive(vault::open(&remote.data, key).map_err(|_| Error::Crypto)?);
                let ours = &s.contents.as_ref().ok_or(Error::Locked)?.0;
                let merged = crate::session::Sensitive(merge::merge(ours, &theirs.0, &prefer_remote, &placeholders()));
                s.persist_sync(merged)?;
                let expect = remote.version.clone().map(Expect::Unchanged).unwrap_or(Expect::Any);
                Ok((s.local(), expect))
            })
            .map_err(session_error)?;
        let local = local.ok_or(SyncError::NoLocal)?;
        let version = location.backend().write(&local.data, &metadata(device, &local, SystemTime::now()), &expect)?;
        controller
            .run(epoch, |s| {
                s.sync.set_marks(local.vault_updated_at, version.as_ref());
                s.conflict = None;
                s.record(Source::Merge, Some(Direction::Upload), Some(device.to_owned()), Some(local.data.len() as u64));
                s.sync_status = Status::Synced;
                Ok(s.save_sync())
            })
            .map_err(session_error)?
    })();
    if let Err(e) = result {
        let _ = controller.run(epoch, |s| {
            if e == SyncError::RemoteChanged {
                s.sync_soon(true);
                s.sync_status = Status::Idle;
            } else {
                s.fail(e, Source::Merge, device);
            }
            Ok(())
        });
    }
    result
}

/// "Restore from Backup" for an unreadable backup: open it in memory with its own password first; only then does it
/// replace this vault and its key. A wrong password changes nothing and does not count toward the unlock delay: the
/// ciphertext already sits in the backup location, so a counter here could not stop offline guessing anyway.
pub fn restore_unreadable(controller: &Controller, epoch: u64, password: &str) -> Result<(), SyncError> {
    let location = controller.run(epoch, |s| s.sync.location.clone().ok_or(Error::NotFound)).map_err(session_error)?;
    let remote = location.backend().read()?.ok_or(SyncError::NoRemote)?;
    let opened = vault::unlock(&remote.data, password)
        .map_err(|e| if e == vault::VaultError::InvalidPassword { SyncError::Password } else { SyncError::Corrupt })?;
    controller
        .run(epoch, |s| {
            let contents = crate::session::Sensitive(opened.contents);
            s.replace_with(remote.data.clone(), contents, Some(opened.key))?;
            s.sync.set_marks(opened.info.updated_at, remote.version.as_ref());
            s.conflict = None;
            s.record(Source::Restore, Some(Direction::Download), remote.device.clone(), Some(remote.data.len() as u64));
            s.sync_status = Status::Synced;
            s.save_sync().map_err(|_| Error::Storage)
        })
        .map_err(session_error)
}

/// First run: restores a vault from a folder backup with its master password and links that folder.
pub fn restore_first_run(controller: &Controller, epoch: u64, location: impl Into<Location>, password: &str) -> Result<(), SyncError> {
    let location = location.into();
    let remote = location.backend().read()?.ok_or(SyncError::NoRemote)?;
    controller
        .run(epoch, |s| {
            s.restore_backup(&remote.data, password)?;
            let updated = vault::envelope_info(&remote.data).map_err(|_| Error::Corrupt)?.updated_at;
            s.sync.location = Some(location);
            s.sync.set_marks(updated, remote.version.as_ref());
            s.record(Source::Restore, Some(Direction::Download), remote.device.clone(), Some(remote.data.len() as u64));
            s.sync_status = Status::Synced;
            s.save_sync().map_err(|_| Error::Storage)
        })
        .map_err(session_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_choice_uses_a_safay_subfolder_unless_picked_directly() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(FolderBackend::resolve(dir.path()), dir.path().join("Safay"));
        let safay = dir.path().join("Safay");
        assert_eq!(FolderBackend::resolve(&safay), safay);
        std::fs::write(dir.path().join(VAULT_FILE_NAME), b"x").unwrap();
        assert_eq!(FolderBackend::resolve(dir.path()), dir.path());
    }
    #[test]
    fn state_round_trips_and_unreadable_state_is_default() {
        let mut s = SyncState { location: Some(Location::Folder { path: "/tmp/Safay".into() }), ..Default::default() };
        s.set_marks(UNIX_EPOCH + Duration::from_secs(5), Some(&RemoteVersion::Revision { value: "abc".into() }));
        assert_eq!(SyncState::decode(&s.encode()), s);
        assert_eq!(s.marks().last_remote_version, Some(RemoteVersion::Revision { value: "abc".into() }));
        assert_eq!(SyncState::decode(b"not json"), SyncState::default());
    }
}
