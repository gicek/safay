//! Cloud sign-ins in the system credential store (macOS Keychain, Windows Credential Manager, the Secret Service on
//! Linux: GNOME Keyring or KWallet) under this app's own service name. Only the serialized `cloudset::Connection` is
//! stored; it never touches disk otherwise.
use safay_app::{Zeroizing, cloud::Credentials, sync::SyncError};

/// `<bundle ID>.cloud`: the release and debug builds keep separate sign-ins.
const SERVICE: &str = if cfg!(debug_assertions) { "com.gicek.safay.debug.cloud" } else { "com.gicek.safay.cloud" };

pub struct Keychain;
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
