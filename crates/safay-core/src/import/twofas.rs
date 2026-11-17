//! 2FAS backups (`.2fas`), plain and encrypted.
//!
//! Encrypted backups keep `services` empty and store them in `servicesEncrypted` as `ciphertext:salt:iv` (Base64 each):
//! AES-256-GCM (tag appended to the ciphertext) with a key from PBKDF2-HMAC-SHA256 (10,000 iterations, 32 bytes).
//! Groups stay in `groups` and services point at them with `groupId`.

use aes_gcm::aead::{Aead, Nonce};
use aes_gcm::{Aes256Gcm, KeyInit};
use base64::Engine;
use serde_json::{Map, Value};
use zeroize::Zeroizing;

use super::{Collector, Extras, Fields, ImportError, int_field, parse_plaintext, str_field};

const PBKDF2_ITERATIONS: u32 = 10_000;

/// `Some(encrypted)` for a 2FAS backup.
pub(super) fn detect(root: &Map<String, Value>) -> Option<bool> {
    if !root.contains_key("schemaVersion") || !root.get("services").is_some_and(Value::is_array) {
        return None;
    }
    Some(root.get("servicesEncrypted").and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty()))
}

pub(super) fn import(root: &Map<String, Value>, password: &str, collector: &mut Collector<'_>) -> Result<(), ImportError> {
    let decrypted;
    let services = match root.get("servicesEncrypted").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
        Some(encrypted) => {
            decrypted = parse_plaintext(decrypt(encrypted, password)?)?;
            decrypted.as_array().ok_or(ImportError::InvalidFile)?
        }
        None => root.get("services").and_then(Value::as_array).ok_or(ImportError::InvalidFile)?,
    };
    let groups: Vec<(&str, &str)> = match root.get("groups") {
        Some(Value::Array(groups)) => {
            groups.iter().filter_map(Value::as_object).map(|group| (str_field(group, "id"), str_field(group, "name"))).collect()
        }
        _ => Vec::new(),
    };
    let mut services: Vec<&Map<String, Value>> = services.iter().filter_map(Value::as_object).collect();
    services
        .sort_by_key(|service| service.get("order").and_then(Value::as_object).and_then(|o| int_field(o, "position")).unwrap_or(i64::MAX));
    for service in services {
        let empty = Map::new();
        let otp = service.get("otp").and_then(Value::as_object).unwrap_or(&empty);
        let name = str_field(service, "name");
        let fields = Fields {
            kind: str_field(otp, "tokenType"),
            issuer: if name.trim().is_empty() { str_field(otp, "issuer") } else { name },
            username: str_field(otp, "account"),
            secret: str_field(service, "secret"),
            algorithm: str_field(otp, "algorithm"),
            digits: int_field(otp, "digits"),
            period: int_field(otp, "period"),
        };
        let group_id = str_field(service, "groupId");
        let groups = groups.iter().filter(|(id, _)| !group_id.is_empty() && *id == group_id).map(|(_, name)| (*name).to_owned()).collect();
        collector.add_fields(&fields, Extras { groups, ..Extras::default() })?;
    }
    Ok(())
}

fn decrypt(encrypted: &str, password: &str) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    let base64 = |part: &str| base64::engine::general_purpose::STANDARD.decode(part.trim()).map_err(|_| ImportError::InvalidFile);
    let parts: Vec<&str> = encrypted.split(':').collect();
    if parts.len() < 3 {
        return Err(ImportError::InvalidFile);
    }
    let (sealed, salt, iv) = (base64(parts[0])?, base64(parts[1])?, base64(parts[2])?);
    if iv.len() != 12 || sealed.len() < 16 {
        return Err(ImportError::InvalidFile);
    }
    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password.as_bytes(), &salt, PBKDF2_ITERATIONS, key.as_mut_slice());
    let cipher = Aes256Gcm::new_from_slice(key.as_slice()).map_err(|_| ImportError::InvalidFile)?;
    let nonce = Nonce::<Aes256Gcm>::try_from(iv.as_slice()).map_err(|_| ImportError::InvalidFile)?;
    cipher.decrypt(&nonce, sealed.as_slice()).map(Zeroizing::new).map_err(|_| ImportError::WrongPassword)
}
