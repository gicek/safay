//! iCloud backup: one record in the CloudKit private database of the container `iCloud.com.gicek.safay` (set by
//! `scripts/package.py`), zone [`ZONE`], type `Vault`, name `vault`. `envelope` is the encrypted envelope as is; device name and envelope time are CloudKit `encryptedValues`. The remote version is the
//! record's change tag, and the server checks every conditional write, so another device's change is never overwritten.
//!
//! This module holds no CloudKit code: the app installs a [`Store`] (the system's CloudKit on macOS builds signed with
//! the iCloud entitlement), tests install [`MemoryStore`]. Without a store the iCloud location is unavailable.
use crate::sync::{Backend, Expect, Location, Remote, SyncError};
use safay_core::{backup::BackupMetadata, sync::RemoteVersion, vault};
use std::{
    sync::{Arc, Mutex, RwLock, atomic::AtomicBool, atomic::Ordering},
    time::SystemTime,
};

/// The record's zone, type and name.
pub const ZONE: &str = "Safay";
pub const RECORD_TYPE: &str = "Vault";
pub const RECORD_NAME: &str = "vault";
/// This app's subscription to backup changes (silent push).
pub const SUBSCRIPTION_ID: &str = "safay-vault-changes";

/// The iCloud account on this device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountState {
    Available,
    NoAccount,
    Restricted,
    TemporarilyUnavailable,
}

/// Why iCloud can't be used right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ICloudError {
    NotSignedIn,
    Restricted,
    TemporarilyUnavailable,
    /// The device switched iCloud accounts: this vault is not written into the other account; choose iCloud again.
    AccountChanged,
    Offline,
    QuotaExceeded,
    Busy,
    /// The build has no iCloud capability (not signed with the entitlement).
    NotEntitled,
    Service,
}

/// A store failure. `RecordChanged` is the server rejecting a conditional write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError {
    ICloud(ICloudError),
    RecordChanged,
    TooLarge,
    Corrupt,
}
impl From<StoreError> for SyncError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::ICloud(e) => SyncError::ICloud(e),
            StoreError::RecordChanged => SyncError::RemoteChanged,
            StoreError::TooLarge => SyncError::TooLarge,
            StoreError::Corrupt => SyncError::Corrupt,
        }
    }
}

/// What the server record must be for a write to proceed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Condition {
    Absent,
    Unchanged(String),
    Any,
}

/// The backup record as read. `Debug` is deliberately absent.
#[derive(Clone, PartialEq, Eq)]
pub struct Record {
    pub envelope: Vec<u8>,
    pub device: Option<String>,
    pub change_tag: String,
}

/// The narrow CloudKit interface. Calls block; they never run on the UI thread.
pub trait Store: Send + Sync {
    fn account_state(&self) -> Result<AccountState, StoreError>;
    /// The current account's user record name in this container: an opaque ID, used only to notice account switches.
    fn account_id(&self) -> Result<String, StoreError>;
    /// `None` when there is no backup (including a missing zone, or one the user deleted in Settings).
    fn fetch(&self) -> Result<Option<Record>, StoreError>;
    /// Returns the change tag after the write; `RecordChanged` when the condition does not hold.
    fn save(&self, envelope: &[u8], device: &str, vault_updated_at: SystemTime, condition: &Condition) -> Result<String, StoreError>;
    /// Deletes the backup for every device: the whole zone (already gone is fine).
    fn delete_backup(&self) -> Result<(), StoreError>;
    /// Subscribes to changes of the backup record ([`SUBSCRIPTION_ID`]), delivered as silent pushes. Saving it again
    /// replaces it.
    fn subscribe(&self) -> Result<(), StoreError>;
    /// Removes [`SUBSCRIPTION_ID`] (never subscribed is fine).
    fn unsubscribe(&self) -> Result<(), StoreError>;
}

/// Turns off iCloud backup's remote copy: deletes the backup in iCloud for all devices (the caller then unlinks).
/// Devices that still back up to iCloud upload theirs again.
pub fn delete_backup() -> Result<(), SyncError> {
    Ok(store()?.delete_backup()?)
}
/// Best effort: offline, it is tried again at the next launch.
pub fn subscribe() -> Result<(), SyncError> {
    Ok(store()?.subscribe()?)
}
pub fn unsubscribe() -> Result<(), SyncError> {
    Ok(store()?.unsubscribe()?)
}

static STORE: RwLock<Option<Arc<dyn Store>>> = RwLock::new(None);
/// Installs the process's store (once at startup, or by a test).
pub fn install(store: Arc<dyn Store>) {
    *STORE.write().unwrap_or_else(|e| e.into_inner()) = Some(store);
}
fn store() -> Result<Arc<dyn Store>, SyncError> {
    STORE.read().unwrap_or_else(|e| e.into_inner()).clone().ok_or(SyncError::ICloud(ICloudError::NotEntitled))
}
/// Whether this build can use iCloud at all (the UI disables the location otherwise).
pub fn available() -> bool {
    STORE.read().unwrap_or_else(|e| e.into_inner()).is_some()
}

fn check(state: AccountState) -> Result<(), SyncError> {
    match state {
        AccountState::Available => Ok(()),
        AccountState::NoAccount => Err(SyncError::ICloud(ICloudError::NotSignedIn)),
        AccountState::Restricted => Err(SyncError::ICloud(ICloudError::Restricted)),
        AccountState::TemporarilyUnavailable => Err(SyncError::ICloud(ICloudError::TemporarilyUnavailable)),
    }
}

/// Choosing iCloud: confirms this device is signed in, then records the current account in the location to link (the
/// link itself still reads first and refuses a backup this vault's key cannot open).
pub fn location() -> Result<Location, SyncError> {
    let store = store()?;
    check(store.account_state()?)?;
    Ok(Location::ICloud { account: store.account_id()? })
}

/// Reads and writes the backup record. Before its first read or write it confirms the account is available and is
/// still the one linked.
pub struct ICloudBackend {
    account: String,
    verified: AtomicBool,
}
impl ICloudBackend {
    pub fn new(account: String) -> Self {
        Self { account, verified: AtomicBool::new(false) }
    }
    fn verified(&self) -> Result<Arc<dyn Store>, SyncError> {
        let store = store()?;
        if !self.verified.load(Ordering::SeqCst) {
            check(store.account_state()?)?;
            if store.account_id()? != self.account {
                return Err(SyncError::ICloud(ICloudError::AccountChanged));
            }
            self.verified.store(true, Ordering::SeqCst);
        }
        Ok(store)
    }
}
impl Backend for ICloudBackend {
    fn read(&self) -> Result<Option<Remote>, SyncError> {
        let Some(record) = self.verified()?.fetch()? else { return Ok(None) };
        if record.envelope.len() as u64 > crate::storage::MAX_FILE_BYTES || !vault::is_valid_envelope(&record.envelope) {
            return Err(SyncError::Corrupt);
        }
        Ok(Some(Remote {
            data: record.envelope,
            version: Some(RemoteVersion::Revision { value: record.change_tag }),
            device: record.device,
        }))
    }
    fn write(&self, data: &[u8], metadata: &BackupMetadata, expect: &Expect) -> Result<Option<RemoteVersion>, SyncError> {
        let store = self.verified()?;
        let condition = match expect {
            Expect::Any => Condition::Any,
            Expect::Absent => Condition::Absent,
            Expect::Unchanged(RemoteVersion::Revision { value }) => Condition::Unchanged(value.clone()),
            // A version from another kind of location: treat the remote as changed so it is read again.
            Expect::Unchanged(RemoteVersion::ModifiedAt { .. }) => return Err(SyncError::RemoteChanged),
        };
        let tag = store.save(data, &metadata.device_name, metadata.vault_updated_at, &condition)?;
        Ok(Some(RemoteVersion::Revision { value: tag }))
    }
}

/// An in-memory CloudKit private database for tests: one record, change tags that advance on every save, and a
/// switchable account.
pub struct MemoryStore {
    pub state: Mutex<AccountState>,
    pub account: Mutex<String>,
    record: Mutex<Option<(Vec<u8>, String, u64)>>,
    pub offline: AtomicBool,
    saves: Mutex<u64>,
    /// Whether [`SUBSCRIPTION_ID`] is saved.
    pub subscribed: AtomicBool,
}
impl MemoryStore {
    pub fn new(account: &str) -> Self {
        Self {
            state: Mutex::new(AccountState::Available),
            account: Mutex::new(account.into()),
            record: Mutex::new(None),
            offline: AtomicBool::new(false),
            saves: Mutex::new(0),
            subscribed: AtomicBool::new(false),
        }
    }
    /// The stored envelope (to check that only ciphertext leaves the device).
    pub fn envelope(&self) -> Option<Vec<u8>> {
        self.record.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|r| r.0.clone())
    }
    fn online(&self) -> Result<(), StoreError> {
        if self.offline.load(Ordering::SeqCst) { Err(StoreError::ICloud(ICloudError::Offline)) } else { Ok(()) }
    }
}
impl Store for MemoryStore {
    fn account_state(&self) -> Result<AccountState, StoreError> {
        self.online()?;
        Ok(*self.state.lock().unwrap_or_else(|e| e.into_inner()))
    }
    fn account_id(&self) -> Result<String, StoreError> {
        self.online()?;
        Ok(self.account.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }
    fn fetch(&self) -> Result<Option<Record>, StoreError> {
        self.online()?;
        Ok(self.record.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|(envelope, device, tag)| Record {
            envelope: envelope.clone(),
            device: Some(device.clone()),
            change_tag: tag.to_string(),
        }))
    }
    fn save(&self, envelope: &[u8], device: &str, _at: SystemTime, condition: &Condition) -> Result<String, StoreError> {
        self.online()?;
        let mut record = self.record.lock().unwrap_or_else(|e| e.into_inner());
        let current = record.as_ref().map(|r| r.2.to_string());
        let allowed = match condition {
            Condition::Any => true,
            Condition::Absent => current.is_none(),
            Condition::Unchanged(tag) => current.as_deref() == Some(tag.as_str()),
        };
        if !allowed {
            return Err(StoreError::RecordChanged);
        }
        let mut saves = self.saves.lock().unwrap_or_else(|e| e.into_inner());
        *saves += 1;
        *record = Some((envelope.to_vec(), device.to_owned(), *saves));
        Ok(saves.to_string())
    }
    fn delete_backup(&self) -> Result<(), StoreError> {
        self.online()?;
        *self.record.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(())
    }
    fn subscribe(&self) -> Result<(), StoreError> {
        self.online()?;
        self.subscribed.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn unsubscribe(&self) -> Result<(), StoreError> {
        self.online()?;
        self.subscribed.store(false, Ordering::SeqCst);
        Ok(())
    }
}
