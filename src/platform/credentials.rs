//! Cloud sign-ins in the system credential store (the Keychain on macOS and iOS, Windows Credential Manager, the Secret
//! Service on Linux: GNOME Keyring or KWallet) under this app's own service name. Only the serialized
//! `cloudset::Connection` is stored; it never touches disk otherwise.
use safay_app::{Zeroizing, cloud::Credentials, sync::SyncError};

/// `<bundle ID>.cloud`: the release and debug builds keep separate sign-ins.
#[cfg_attr(target_os = "android", allow(dead_code))]
const SERVICE: &str = if cfg!(debug_assertions) { "com.gicek.safay.debug.cloud" } else { "com.gicek.safay.cloud" };

pub struct Keychain;
// Android: a file for each sign-in in the app's own directory, sealed by a key of the hardware keystore
// (`com.gicek.safay.Secrets`). The file name is the id's SHA-256, so an id never names a path.
#[cfg(target_os = "android")]
fn file(id: &str) -> Option<std::path::PathBuf> {
    use sha2::{Digest, Sha256};
    let name: String = Sha256::digest(id.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect();
    Some(super::android::private_directory()?.join("cloud").join(name))
}
#[cfg(target_os = "android")]
impl Credentials for Keychain {
    fn load(&self, id: &str) -> Option<Zeroizing<Vec<u8>>> {
        let sealed = std::fs::read(file(id)?).ok()?;
        super::android::unseal(&sealed)
    }
    fn save(&self, id: &str, blob: &[u8]) -> Result<(), SyncError> {
        let path = file(id).ok_or(SyncError::Storage)?;
        let sealed = super::android::seal(blob).ok_or(SyncError::Storage)?;
        std::fs::create_dir_all(path.parent().ok_or(SyncError::Storage)?).map_err(|_| SyncError::Storage)?;
        // Written beside it and moved into place: a sign-in is whole or absent.
        let partial = path.with_extension("new");
        std::fs::write(&partial, sealed).and_then(|()| std::fs::rename(&partial, &path)).map_err(|_| SyncError::Storage)
    }
    fn delete(&self, id: &str) {
        if let Some(path) = file(id) {
            let _ = std::fs::remove_file(path);
        }
    }
}
#[cfg(not(target_os = "android"))]
impl Credentials for Keychain {
    fn load(&self, id: &str) -> Option<Zeroizing<Vec<u8>>> {
        keyring::Entry::new(SERVICE, id).ok()?.get_secret().ok().map(Zeroizing::new)
    }
    fn save(&self, id: &str, blob: &[u8]) -> Result<(), SyncError> {
        keyring::Entry::new(SERVICE, id).and_then(|e| e.set_secret(blob)).map_err(|_| SyncError::Storage)
    }
    fn delete(&self, id: &str) {
        if let Ok(entry) = keyring::Entry::new(SERVICE, id) {
            drop(entry.delete_credential());
        }
    }
}

#[cfg(test)]
mod tests {
    /// Round-trips a sign-in through the real store (Keychain, Credential Manager, Secret Service) under a service name of
    /// its own, and removes it. Not run by default, as it touches the user's store: `cargo test -- --ignored credentials`.
    #[test]
    #[ignore = "uses the system credential store"]
    fn credentials_round_trip_in_the_system_store() {
        let entry = || keyring::Entry::new("com.gicek.safay.test", "round-trip").unwrap();
        entry().set_secret(b"synthetic").unwrap();
        assert_eq!(entry().get_secret().unwrap(), b"synthetic");
        entry().delete_credential().unwrap();
        assert!(entry().get_secret().is_err());
    }
}
