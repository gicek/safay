//! macOS iCloud backup: the `safay_app::icloud::Store` over the system CloudKit framework. NSH has no
//! CloudKit API. Calls block on CloudKit's completion handlers (never on the UI thread: sync runs in `blocking`).
//!
//! The container comes from `SafayICloudContainer` in Info.plist, which `scripts/package.py` writes only when it signs
//! the bundle with a provisioning profile carrying the iCloud entitlement. `CKContainer` terminates a process that lacks
//! the entitlement, so without that key the store is never created and iCloud stays unavailable.
use nsh_platform_macos::{
    objc2::{
        AnyThread,
        rc::Retained,
        runtime::{AnyObject, ProtocolObject},
    },
    objc2_foundation::{NSArray, NSBundle, NSDate, NSError, NSString, NSURL},
};
use objc2_cloud_kit::{
    CKAccountStatus, CKAsset, CKContainer, CKCurrentUserDefaultName, CKDatabase, CKDatabaseSubscription, CKErrorCode, CKErrorDomain,
    CKModifyRecordsOperation, CKNotificationInfo, CKPartialErrorsByItemIDKey, CKRecord, CKRecordID, CKRecordKeyValueSetting,
    CKRecordSavePolicy, CKRecordZone, CKRecordZoneID, CKSubscription,
};
use safay_app::icloud::{AccountState, Condition, ICloudError, RECORD_NAME, RECORD_TYPE, Record, SUBSCRIPTION_ID, Store, StoreError, ZONE};
use std::{
    io::Write,
    sync::{Mutex, mpsc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// CloudKit retries internally; past this a call is reported as unreachable rather than left hanging.
const TIMEOUT: Duration = Duration::from_secs(90);

/// The container this build is entitled to, if any.
pub fn container_identifier() -> Option<String> {
    let value = NSBundle::mainBundle().objectForInfoDictionaryKey(&NSString::from_str("SafayICloudContainer"))?;
    let value = value.downcast_ref::<NSString>()?.to_string();
    value.starts_with("iCloud.").then_some(value)
}

pub struct CloudKitStore {
    container: Retained<CKContainer>,
    database: Retained<CKDatabase>,
    /// The record as last fetched or saved: conditional writes modify it (the server checks its change tag).
    last: Mutex<Option<Retained<CKRecord>>>,
}
// SAFETY: CKContainer, CKDatabase and CKRecord are used from one sync thread at a time (calls are serialized by the
// session's sync orchestration and `last` is behind a mutex); CloudKit's own objects are thread-safe for these calls.
unsafe impl Send for CloudKitStore {}
unsafe impl Sync for CloudKitStore {}

impl CloudKitStore {
    /// `None` when the bundle carries no iCloud container (unsigned or development-host builds).
    pub fn new() -> Option<Self> {
        let identifier = container_identifier()?;
        // SAFETY: the entitlement for this container is present (see the module note).
        let container = unsafe { CKContainer::containerWithIdentifier(&NSString::from_str(&identifier)) };
        let database = unsafe { container.privateCloudDatabase() };
        Some(Self { container, database, last: Mutex::new(None) })
    }
    fn zone_id() -> Retained<CKRecordZoneID> {
        // SAFETY: plain initializers with valid strings.
        unsafe { CKRecordZoneID::initWithZoneName_ownerName(CKRecordZoneID::alloc(), &NSString::from_str(ZONE), CKCurrentUserDefaultName) }
    }
    fn record_id() -> Retained<CKRecordID> {
        unsafe { CKRecordID::initWithRecordName_zoneID(CKRecordID::alloc(), &NSString::from_str(RECORD_NAME), &Self::zone_id()) }
    }
    fn new_record() -> Retained<CKRecord> {
        unsafe { CKRecord::initWithRecordType_recordID(CKRecord::alloc(), &NSString::from_str(RECORD_TYPE), &Self::record_id()) }
    }

    /// Fetches the record; `Ok(None)` when it, or its zone, does not exist.
    fn fetch_record(&self) -> Result<Option<Retained<CKRecord>>, Failure> {
        let (tx, rx) = mpsc::channel();
        let handler = block2::RcBlock::new(move |record: *mut CKRecord, error: *mut NSError| {
            // SAFETY: CloudKit passes a valid record or error (or null).
            let outcome = match unsafe { error.as_ref() } {
                Some(e) => Err(Failure::of(e)),
                None => Ok(unsafe { Retained::retain(record) }),
            };
            let _ = tx.send(outcome);
        });
        unsafe { self.database.fetchRecordWithID_completionHandler(&Self::record_id(), &handler) };
        match wait(rx)? {
            Ok(record) => Ok(record),
            Err(f) if f.is_missing() => Ok(None),
            Err(f) => Err(f),
        }
    }
    fn save_zone(&self) -> Result<(), Failure> {
        let (tx, rx) = mpsc::channel();
        let handler = block2::RcBlock::new(move |_zone: *mut CKRecordZone, error: *mut NSError| {
            let _ = tx.send(unsafe { error.as_ref() }.map(Failure::of));
        });
        let zone = unsafe { CKRecordZone::initWithZoneID(CKRecordZone::alloc(), &Self::zone_id()) };
        unsafe { self.database.saveRecordZone_completionHandler(&zone, &handler) };
        match wait(rx)? {
            Some(f) => Err(f),
            None => Ok(()),
        }
    }
    fn save_once(&self, envelope: &[u8], device: &str, at: SystemTime, condition: &Condition) -> Result<String, Failure> {
        let (record, policy) = match condition {
            // A new record means "must not exist": if it does, the server answers serverRecordChanged.
            Condition::Absent => (Self::new_record(), CKRecordSavePolicy::IfServerRecordUnchanged),
            Condition::Unchanged(tag) => (self.server_record(tag)?, CKRecordSavePolicy::IfServerRecordUnchanged),
            Condition::Any => (self.fetch_record()?.unwrap_or_else(Self::new_record), CKRecordSavePolicy::AllKeys),
        };
        // The asset file is only the encrypted envelope (owner-only), removed when this call returns.
        let mut file = tempfile::Builder::new().prefix("safay-").suffix(".safayvault").tempfile().map_err(|_| Failure::Service)?;
        file.write_all(envelope).and_then(|_| file.flush()).map_err(|_| Failure::Service)?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(&file.path().to_string_lossy()));
        let seconds = at.duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        unsafe {
            let asset = CKAsset::initWithFileURL(CKAsset::alloc(), &url);
            record.setObject_forKey(Some(ProtocolObject::from_ref(&*asset)), &NSString::from_str("envelope"));
            let values = record.encryptedValues();
            let name = NSString::from_str(device);
            values.setObject_forKey(Some(ProtocolObject::from_ref(&*name)), &NSString::from_str("deviceName"));
            let date = NSDate::dateWithTimeIntervalSince1970(seconds);
            values.setObject_forKey(Some(ProtocolObject::from_ref(&*date)), &NSString::from_str("vaultUpdatedAt"));
        }
        let (tx, rx) = mpsc::channel();
        let handler =
            block2::RcBlock::new(move |saved: *mut NSArray<CKRecord>, _deleted: *mut NSArray<CKRecordID>, error: *mut NSError| {
                let outcome = match unsafe { error.as_ref() } {
                    Some(e) => Err(Failure::of(e)),
                    None => unsafe { saved.as_ref() }.and_then(|a| a.firstObject()).ok_or(Failure::Service),
                };
                let _ = tx.send(outcome);
            });
        let operation = unsafe {
            CKModifyRecordsOperation::initWithRecordsToSave_recordIDsToDelete(
                CKModifyRecordsOperation::alloc(),
                Some(&NSArray::from_retained_slice(&[record])),
                None,
            )
        };
        unsafe {
            operation.setSavePolicy(policy);
            operation.setAtomic(true);
            operation.setModifyRecordsCompletionBlock(Some(&handler));
            self.database.addOperation(&operation);
        }
        let saved = wait(rx)??;
        let tag = unsafe { saved.recordChangeTag() }.map(|t| t.to_string()).unwrap_or_default();
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Some(saved);
        drop(file);
        Ok(tag)
    }
    /// The server record at this version: the cached copy if it matches, otherwise fetched again and compared.
    fn server_record(&self, tag: &str) -> Result<Retained<CKRecord>, Failure> {
        if let Some(last) = self.last.lock().unwrap_or_else(|e| e.into_inner()).clone()
            && unsafe { last.recordChangeTag() }.is_some_and(|t| t.to_string() == tag)
        {
            return Ok(last);
        }
        match self.fetch_record()? {
            Some(record) if unsafe { record.recordChangeTag() }.is_some_and(|t| t.to_string() == tag) => Ok(record),
            _ => Err(Failure::Store(StoreError::RecordChanged)),
        }
    }
}

impl Store for CloudKitStore {
    fn account_state(&self) -> Result<AccountState, StoreError> {
        let (tx, rx) = mpsc::channel();
        let handler = block2::RcBlock::new(move |status: CKAccountStatus, error: *mut NSError| {
            let _ = tx.send(match unsafe { error.as_ref() } {
                Some(e) => Err(Failure::of(e)),
                None => Ok(status),
            });
        });
        unsafe { self.container.accountStatusWithCompletionHandler(&handler) };
        let status = wait(rx).and_then(|r| r).map_err(Failure::into_store)?;
        Ok(match status {
            CKAccountStatus::Available => AccountState::Available,
            CKAccountStatus::NoAccount => AccountState::NoAccount,
            CKAccountStatus::Restricted => AccountState::Restricted,
            _ => AccountState::TemporarilyUnavailable,
        })
    }
    fn account_id(&self) -> Result<String, StoreError> {
        let (tx, rx) = mpsc::channel();
        let handler = block2::RcBlock::new(move |id: *mut CKRecordID, error: *mut NSError| {
            let _ = tx.send(match (unsafe { id.as_ref() }, unsafe { error.as_ref() }) {
                (_, Some(e)) => Err(Failure::of(e)),
                (Some(id), None) => Ok(unsafe { id.recordName() }.to_string()),
                (None, None) => Err(Failure::Service),
            });
        });
        unsafe { self.container.fetchUserRecordIDWithCompletionHandler(&handler) };
        wait(rx).and_then(|r| r).map_err(Failure::into_store)
    }
    fn fetch(&self) -> Result<Option<Record>, StoreError> {
        let record = match self.fetch_record() {
            Ok(Some(record)) => record,
            Ok(None) => {
                *self.last.lock().unwrap_or_else(|e| e.into_inner()) = None;
                return Ok(None);
            }
            Err(f) => return Err(f.into_store()),
        };
        let asset = unsafe { record.objectForKey(&NSString::from_str("envelope")) };
        let path = asset
            .as_deref()
            .and_then(|a| AsRef::<AnyObject>::as_ref(a).downcast_ref::<CKAsset>())
            .and_then(|a| unsafe { a.fileURL() })
            .and_then(|u| u.path())
            .ok_or(StoreError::Corrupt)?;
        let path = std::path::PathBuf::from(path.to_string());
        // Never read more than the file limit into memory.
        let size = std::fs::metadata(&path).map_err(|_| StoreError::Corrupt)?.len();
        if size > safay_app::storage::MAX_FILE_BYTES {
            return Err(StoreError::Corrupt);
        }
        let envelope = std::fs::read(&path).map_err(|_| StoreError::Corrupt)?;
        let device = unsafe { record.encryptedValues().objectForKey(&NSString::from_str("deviceName")) }
            .as_deref()
            .and_then(|v| AsRef::<AnyObject>::as_ref(v).downcast_ref::<NSString>().map(|s| s.to_string()));
        let change_tag = unsafe { record.recordChangeTag() }.map(|t| t.to_string()).unwrap_or_default();
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Some(record);
        Ok(Some(Record { envelope, device, change_tag }))
    }
    fn save(&self, envelope: &[u8], device: &str, at: SystemTime, condition: &Condition) -> Result<String, StoreError> {
        match self.save_once(envelope, device, at, condition) {
            // First write, or the user deleted the data under Manage Storage: create the zone and write again.
            Err(f) if f.is_missing_zone() => {
                self.save_zone().map_err(Failure::into_store)?;
                *self.last.lock().unwrap_or_else(|e| e.into_inner()) = None;
                self.save_once(envelope, device, at, condition).map_err(Failure::into_store)
            }
            other => other.map_err(Failure::into_store),
        }
    }
    fn delete_backup(&self) -> Result<(), StoreError> {
        let (tx, rx) = mpsc::channel();
        let handler = block2::RcBlock::new(move |_zone: *mut CKRecordZoneID, error: *mut NSError| {
            // SAFETY: CloudKit passes a valid error or null.
            let _ = tx.send(unsafe { error.as_ref() }.map(Failure::of));
        });
        // SAFETY: a valid zone ID and a completion block that only sends on a channel.
        unsafe { self.database.deleteRecordZoneWithID_completionHandler(&Self::zone_id(), &handler) };
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = None;
        match wait(rx).map_err(Failure::into_store)? {
            // Already gone.
            Some(f) if f.is_missing() => Ok(()),
            Some(f) => Err(f.into_store()),
            None => Ok(()),
        }
    }
    fn subscribe(&self) -> Result<(), StoreError> {
        // SAFETY: plain initializers and setters with valid objects; the record type is the backup's.
        let subscription = unsafe {
            let subscription =
                CKDatabaseSubscription::initWithSubscriptionID(CKDatabaseSubscription::alloc(), &NSString::from_str(SUBSCRIPTION_ID));
            subscription.setRecordType(Some(&NSString::from_str(RECORD_TYPE)));
            // A silent push: it only wakes the app to sync, nothing is shown.
            let info = CKNotificationInfo::new();
            info.setShouldSendContentAvailable(true);
            subscription.setNotificationInfo(Some(&info));
            subscription
        };
        let (tx, rx) = mpsc::channel();
        let handler = block2::RcBlock::new(move |_saved: *mut CKSubscription, error: *mut NSError| {
            let _ = tx.send(unsafe { error.as_ref() }.map(Failure::of));
        });
        // SAFETY: as above.
        unsafe { self.database.saveSubscription_completionHandler(&subscription, &handler) };
        match wait(rx).map_err(Failure::into_store)? {
            Some(f) => Err(f.into_store()),
            None => Ok(()),
        }
    }
    fn unsubscribe(&self) -> Result<(), StoreError> {
        let (tx, rx) = mpsc::channel();
        let handler = block2::RcBlock::new(move |_id: *mut NSString, error: *mut NSError| {
            let _ = tx.send(unsafe { error.as_ref() }.map(Failure::of));
        });
        // SAFETY: as above.
        unsafe { self.database.deleteSubscriptionWithID_completionHandler(&NSString::from_str(SUBSCRIPTION_ID), &handler) };
        match wait(rx).map_err(Failure::into_store)? {
            // Never subscribed.
            Some(f) if f.is_missing() => Ok(()),
            Some(f) => Err(f.into_store()),
            None => Ok(()),
        }
    }
}

/// Whether a remote notification (its `userInfo`) is this app's backup subscription firing: another device changed the
/// backup, matched by this app's subscription ID.
pub fn is_backup_change(user_info: &nsh_platform_macos::objc2_foundation::NSDictionary) -> bool {
    use objc2_cloud_kit::CKNotification;
    // SAFETY: `user_info` is the dictionary AppKit delivered with the notification.
    let notification = unsafe { CKNotification::notificationFromRemoteNotificationDictionary(user_info) };
    notification.and_then(|n| unsafe { n.subscriptionID() }).is_some_and(|id| id.to_string() == SUBSCRIPTION_ID)
}

/// A CloudKit failure, classified.
#[derive(Clone, Copy, Debug)]
enum Failure {
    Code(isize),
    Store(StoreError),
    Service,
}
impl Failure {
    fn of(error: &NSError) -> Self {
        if error.domain().to_string() != unsafe { CKErrorDomain }.to_string() {
            return Failure::Service;
        }
        let code = error.code();
        if code == CKErrorCode::PartialFailure.0 {
            // The first per-item error decides.
            let partial = error
                .userInfo()
                .objectForKey(unsafe { CKPartialErrorsByItemIDKey })
                .and_then(|d| d.downcast::<nsh_platform_macos::objc2_foundation::NSDictionary>().ok())
                .and_then(|d| d.allValues().firstObject())
                .and_then(|e| e.downcast::<NSError>().ok());
            return partial.map(|e| Failure::of(&e)).unwrap_or(Failure::Service);
        }
        Failure::Code(code)
    }
    fn is_missing_zone(self) -> bool {
        matches!(self, Failure::Code(c) if c == CKErrorCode::ZoneNotFound.0 || c == CKErrorCode::UserDeletedZone.0)
    }
    fn is_missing(self) -> bool {
        self.is_missing_zone() || matches!(self, Failure::Code(c) if c == CKErrorCode::UnknownItem.0)
    }
    fn into_store(self) -> StoreError {
        use ICloudError as I;
        let code = match self {
            Failure::Store(e) => return e,
            Failure::Service => return StoreError::ICloud(I::Service),
            Failure::Code(c) => CKErrorCode(c),
        };
        StoreError::ICloud(match code {
            CKErrorCode::NotAuthenticated => I::NotSignedIn,
            CKErrorCode::ManagedAccountRestricted => I::Restricted,
            CKErrorCode::AccountTemporarilyUnavailable => I::TemporarilyUnavailable,
            CKErrorCode::NetworkUnavailable | CKErrorCode::NetworkFailure => I::Offline,
            CKErrorCode::QuotaExceeded => I::QuotaExceeded,
            CKErrorCode::ServiceUnavailable | CKErrorCode::RequestRateLimited | CKErrorCode::ZoneBusy => I::Busy,
            CKErrorCode::MissingEntitlement | CKErrorCode::BadContainer | CKErrorCode::PermissionFailure => I::NotEntitled,
            CKErrorCode::ServerRecordChanged => return StoreError::RecordChanged,
            _ => I::Service,
        })
    }
}

fn wait<T>(rx: mpsc::Receiver<T>) -> Result<T, Failure> {
    rx.recv_timeout(TIMEOUT).map_err(|_| Failure::Code(CKErrorCode::NetworkFailure.0))
}

/// `--icloud-check` (debug builds): prints whether this bundle can reach CloudKit. Never prints the account ID or data.
#[cfg(debug_assertions)]
pub fn check() -> i32 {
    let Some(container) = container_identifier() else {
        println!("no SafayICloudContainer in Info.plist: this build has no iCloud");
        return 1;
    };
    let Some(store) = CloudKitStore::new() else { return 1 };
    println!("container: {container}");
    match store.account_state() {
        Ok(state) => println!("account: {state:?}"),
        Err(e) => {
            println!("account state failed: {e:?}");
            return 1;
        }
    }
    match store.account_id() {
        Ok(id) => println!("account id: {} characters", id.len()),
        Err(e) => println!("account id failed: {e:?}"),
    }
    match store.fetch() {
        Ok(Some(record)) => println!(
            "backup record: present, {} bytes, valid envelope: {}",
            record.envelope.len(),
            safay_app::core::vault::is_valid_envelope(&record.envelope)
        ),
        Ok(None) => println!("backup record: none"),
        Err(e) => println!("fetch failed: {e:?}"),
    }
    0
}
