//! Ciphertext-only persistence. All paths are supplied by the launcher.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    #[error("storage is unavailable")]
    Unavailable,
    #[error("file exceeds the size limit")]
    TooLarge,
    #[error("another process changed the vault")]
    Changed,
    #[error("another instance is using this vault")]
    InUse,
}

/// Expected ciphertext is a compare-and-swap precondition. Failed commits leave the main file unchanged.
pub trait Repository: Send {
    fn preferences(&self) -> Result<crate::preferences::Preferences, StorageError>;
    fn set_preferences(&mut self, value: crate::preferences::Preferences) -> Result<(), StorageError>;
    fn exists(&self) -> Result<bool, StorageError> {
        Ok(self.read()?.is_some())
    }
    fn has_snapshot(&self) -> Result<bool, StorageError> {
        Ok(self.snapshot()?.is_some())
    }
    /// Explicit, password-verified recovery can replace an unreadable main file without loading it.
    fn restore(&mut self, next: &[u8]) -> Result<(), StorageError>;
    fn read(&self) -> Result<Option<Vec<u8>>, StorageError>;
    fn snapshot(&self) -> Result<Option<Vec<u8>>, StorageError>;
    fn commit(&mut self, expected: Option<&[u8]>, next: &[u8], previous_good: Option<&[u8]>) -> Result<(), StorageError>;
    fn failures(&self) -> Result<u32, StorageError>;
    fn set_failures(&mut self, count: u32) -> Result<(), StorageError>;
    fn reset(&mut self, expected: &[u8]) -> Result<(), StorageError>;
    /// Non-sensitive backup settings: the location, sync marks and history (`sync.json`). Never ciphertext or credentials.
    fn sync_state(&self) -> Result<Option<Vec<u8>>, StorageError>;
    fn set_sync_state(&mut self, value: Option<&[u8]>) -> Result<(), StorageError>;
    /// The vault key wrapped by the platform's hardware (Secure Enclave). Opaque here; useless on another device.
    fn biometric(&self) -> Result<Option<Vec<u8>>, StorageError>;
    fn set_biometric(&mut self, value: Option<&[u8]>) -> Result<(), StorageError>;
}

#[derive(Default)]
pub struct MemoryState {
    pub preferences: crate::preferences::Preferences,
    pub main: Option<Vec<u8>>,
    pub snapshot: Option<Vec<u8>>,
    pub failures: u32,
    pub reject_writes: bool,
    pub sync: Option<Vec<u8>>,
    pub biometric: Option<Vec<u8>>,
}

#[derive(Clone, Default)]
pub struct MemoryRepository(pub Arc<Mutex<MemoryState>>);
impl Repository for MemoryRepository {
    fn preferences(&self) -> Result<crate::preferences::Preferences, StorageError> {
        Ok(self.0.lock().map_err(|_| StorageError::Unavailable)?.preferences)
    }
    fn set_preferences(&mut self, value: crate::preferences::Preferences) -> Result<(), StorageError> {
        let mut s = self.0.lock().map_err(|_| StorageError::Unavailable)?;
        if s.reject_writes {
            return Err(StorageError::Unavailable);
        }
        s.preferences = value;
        Ok(())
    }

    fn restore(&mut self, next: &[u8]) -> Result<(), StorageError> {
        let mut state = self.0.lock().map_err(|_| StorageError::Unavailable)?;
        if state.reject_writes {
            return Err(StorageError::Unavailable);
        }
        state.main = Some(next.to_vec());
        Ok(())
    }

    fn read(&self) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.0.lock().map_err(|_| StorageError::Unavailable)?.main.clone())
    }
    fn snapshot(&self) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.0.lock().map_err(|_| StorageError::Unavailable)?.snapshot.clone())
    }
    fn commit(&mut self, expected: Option<&[u8]>, next: &[u8], previous_good: Option<&[u8]>) -> Result<(), StorageError> {
        let mut s = self.0.lock().map_err(|_| StorageError::Unavailable)?;
        if s.reject_writes {
            return Err(StorageError::Unavailable);
        }
        if s.main.as_deref() != expected {
            return Err(StorageError::Changed);
        }
        if let Some(previous) = previous_good {
            s.snapshot = Some(previous.to_vec());
        }
        s.main = Some(next.to_vec());
        Ok(())
    }
    fn failures(&self) -> Result<u32, StorageError> {
        Ok(self.0.lock().map_err(|_| StorageError::Unavailable)?.failures)
    }
    fn set_failures(&mut self, count: u32) -> Result<(), StorageError> {
        let mut s = self.0.lock().map_err(|_| StorageError::Unavailable)?;
        if s.reject_writes {
            return Err(StorageError::Unavailable);
        }
        s.failures = count;
        Ok(())
    }
    fn reset(&mut self, expected: &[u8]) -> Result<(), StorageError> {
        let mut s = self.0.lock().map_err(|_| StorageError::Unavailable)?;
        if s.reject_writes {
            return Err(StorageError::Unavailable);
        }
        if s.main.as_deref() != Some(expected) {
            return Err(StorageError::Changed);
        }
        s.main = None;
        s.snapshot = None;
        s.failures = 0;
        s.preferences = Default::default();
        s.sync = None;
        s.biometric = None;
        Ok(())
    }
    fn biometric(&self) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.0.lock().map_err(|_| StorageError::Unavailable)?.biometric.clone())
    }
    fn set_biometric(&mut self, value: Option<&[u8]>) -> Result<(), StorageError> {
        let mut s = self.0.lock().map_err(|_| StorageError::Unavailable)?;
        if s.reject_writes {
            return Err(StorageError::Unavailable);
        }
        s.biometric = value.map(<[u8]>::to_vec);
        Ok(())
    }
    fn sync_state(&self) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.0.lock().map_err(|_| StorageError::Unavailable)?.sync.clone())
    }
    fn set_sync_state(&mut self, value: Option<&[u8]>) -> Result<(), StorageError> {
        let mut s = self.0.lock().map_err(|_| StorageError::Unavailable)?;
        if s.reject_writes {
            return Err(StorageError::Unavailable);
        }
        s.sync = value.map(<[u8]>::to_vec);
        Ok(())
    }
}

pub struct DiskRepository {
    directory: PathBuf,
    _lock: File,
}
impl DiskRepository {
    pub fn open(directory: PathBuf) -> Result<Self, StorageError> {
        if directory.exists() && fs::symlink_metadata(&directory).map_err(|_| StorageError::Unavailable)?.file_type().is_symlink() {
            return Err(StorageError::Unavailable);
        }
        fs::create_dir_all(&directory).map_err(|_| StorageError::Unavailable)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).map_err(|_| StorageError::Unavailable)?;
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(directory.join("instance.lock")).map_err(|_| StorageError::Unavailable)?;
        lock.try_lock().map_err(|_| StorageError::InUse)?;
        Ok(Self { directory, _lock: lock })
    }
    fn path(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }
    fn write(&self, name: &str, data: &[u8]) -> Result<(), StorageError> {
        atomic_write(&self.path(name), data)
    }
    fn put(&self, name: &str, value: Option<&[u8]>) -> Result<(), StorageError> {
        match value {
            Some(data) => self.write(name, data),
            None => match fs::remove_file(self.path(name)) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err(StorageError::Unavailable),
            },
        }
    }
}

/// Reads limit + 1 through an already opened handle, guarding growth after metadata inspection too.
pub fn bounded_read(path: &Path) -> Result<Option<Vec<u8>>, StorageError> {
    match fs::symlink_metadata(path) {
        Ok(m) if !m.is_file() || m.file_type().is_symlink() => {
            return Err(StorageError::Unavailable);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(StorageError::Unavailable),
        _ => {}
    }
    let file = File::open(path).map_err(|_| StorageError::Unavailable)?;
    if file.metadata().map_err(|_| StorageError::Unavailable)?.len() > MAX_FILE_BYTES {
        return Err(StorageError::TooLarge);
    }
    let mut data = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut data).map_err(|_| StorageError::Unavailable)?;
    if data.len() as u64 > MAX_FILE_BYTES {
        return Err(StorageError::TooLarge);
    }
    Ok(Some(data))
}

/// tempfile uses exclusive creation and an overwrite-capable atomic rename on each supported OS.
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<(), StorageError> {
    if data.len() as u64 > MAX_FILE_BYTES {
        return Err(StorageError::TooLarge);
    }
    let dir = path.parent().ok_or(StorageError::Unavailable)?;
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|_| StorageError::Unavailable)?;
    temp.write_all(data).and_then(|_| temp.as_file().sync_all()).map_err(|_| StorageError::Unavailable)?;
    temp.persist(path).map_err(|_| StorageError::Unavailable)?;
    // A directory sync improves crash durability on Unix. A failure after rename cannot be reported as an uncommitted write.
    #[cfg(unix)]
    if let Ok(directory) = File::open(dir) {
        let _ = directory.sync_all();
    }
    Ok(())
}
impl Repository for DiskRepository {
    fn preferences(&self) -> Result<crate::preferences::Preferences, StorageError> {
        Ok(bounded_read(&self.path("preferences.json"))
            .ok()
            .flatten()
            .and_then(|data| serde_json::from_slice::<crate::preferences::Preferences>(&data).ok())
            .unwrap_or_default()
            .validated())
    }
    fn set_preferences(&mut self, value: crate::preferences::Preferences) -> Result<(), StorageError> {
        self.write("preferences.json", &serde_json::to_vec(&value).map_err(|_| StorageError::Unavailable)?)
    }

    fn exists(&self) -> Result<bool, StorageError> {
        file_exists(&self.path("vault.safayvault"))
    }
    fn has_snapshot(&self) -> Result<bool, StorageError> {
        file_exists(&self.path("last-good.safayvault"))
    }
    fn restore(&mut self, next: &[u8]) -> Result<(), StorageError> {
        self.write("vault.safayvault", next)
    }

    fn read(&self) -> Result<Option<Vec<u8>>, StorageError> {
        bounded_read(&self.path("vault.safayvault"))
    }
    fn snapshot(&self) -> Result<Option<Vec<u8>>, StorageError> {
        bounded_read(&self.path("last-good.safayvault"))
    }
    fn commit(&mut self, expected: Option<&[u8]>, next: &[u8], previous_good: Option<&[u8]>) -> Result<(), StorageError> {
        if self.read()?.as_deref() != expected {
            return Err(StorageError::Changed);
        }
        if let Some(previous) = previous_good {
            self.write("last-good.safayvault", previous)?;
        }
        self.write("vault.safayvault", next)
    }
    fn failures(&self) -> Result<u32, StorageError> {
        match bounded_read(&self.path("attempts.json"))? {
            None => Ok(0),
            Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| StorageError::Unavailable),
        }
    }
    fn set_failures(&mut self, count: u32) -> Result<(), StorageError> {
        self.write("attempts.json", count.to_string().as_bytes())
    }
    fn reset(&mut self, expected: &[u8]) -> Result<(), StorageError> {
        if self.read()?.as_deref() != Some(expected) {
            return Err(StorageError::Changed);
        }
        // Remove recovery material first; an interrupted reset retains a working main vault.
        for name in ["biometric-key.json", "last-good.safayvault", "attempts.json", "preferences.json", "sync.json", "vault.safayvault"] {
            match fs::remove_file(self.path(name)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(StorageError::Unavailable),
            }
        }
        Ok(())
    }
    fn sync_state(&self) -> Result<Option<Vec<u8>>, StorageError> {
        bounded_read(&self.path("sync.json"))
    }
    fn set_sync_state(&mut self, value: Option<&[u8]>) -> Result<(), StorageError> {
        self.put("sync.json", value)
    }
    fn biometric(&self) -> Result<Option<Vec<u8>>, StorageError> {
        bounded_read(&self.path("biometric-key.json"))
    }
    fn set_biometric(&mut self, value: Option<&[u8]>) -> Result<(), StorageError> {
        self.put("biometric-key.json", value)
    }
}

fn file_exists(path: &Path) -> Result<bool, StorageError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(StorageError::Unavailable),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(StorageError::Unavailable),
    }
}
