//! Proton Authenticator exports, plain and encrypted.
//!
//! Plain exports list `entries`, each with `content.uri` (an `otpauth://` or `steam://` link) and `content.name`.
//! Encrypted exports hold `salt` and `content` (Base64): the key is Argon2id (t = 2, m = 19 MiB, p = 1, 32 bytes) of the
//! password and salt, and `content` is a 12-byte nonce followed by AES-256-GCM ciphertext with the associated data
//! `proton.authenticator.export.v1`.

use aes_gcm::aead::{Aead, Nonce, Payload};
use aes_gcm::{Aes256Gcm, KeyInit};
use serde_json::{Map, Value};
use zeroize::Zeroizing;

use super::{Collector, Extras, ImportError, base64_field, int_field, parse_plaintext, str_field};
use crate::vault;

const AAD: &[u8] = b"proton.authenticator.export.v1";

/// `Some(encrypted)` for a Proton Authenticator export.
pub(super) fn detect(root: &Map<String, Value>) -> Option<bool> {
    if !root.contains_key("version") {
        return None;
    }
    if root.get("entries").is_some_and(Value::is_array) {
        return Some(false);
    }
    (root.get("salt").is_some_and(Value::is_string) && root.get("content").is_some_and(Value::is_string)).then_some(true)
}

pub(super) fn import(root: &Map<String, Value>, password: &str, collector: &mut Collector<'_>) -> Result<(), ImportError> {
    if root.get("entries").is_some_and(Value::is_array) {
        return read_entries(root, collector);
    }
    if int_field(root, "version") != Some(1) {
        return Err(ImportError::InvalidFile);
    }
    let salt = base64_field(root, "salt")?;
    let content = base64_field(root, "content")?;
    if content.len() < 12 + 16 {
        return Err(ImportError::InvalidFile);
    }
    let key = vault::argon2id_raw(password.as_bytes(), &salt, 2, 19 * 1024, 1, 32).ok_or(ImportError::InvalidFile)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| ImportError::InvalidFile)?;
    let nonce = Nonce::<Aes256Gcm>::try_from(&content[..12]).map_err(|_| ImportError::InvalidFile)?;
    let plaintext =
        cipher.decrypt(&nonce, Payload { msg: &content[12..], aad: AAD }).map(Zeroizing::new).map_err(|_| ImportError::WrongPassword)?;
    match parse_plaintext(plaintext)? {
        Value::Object(decrypted) => read_entries(&decrypted, collector),
        _ => Err(ImportError::InvalidFile),
    }
}

fn read_entries(root: &Map<String, Value>, collector: &mut Collector<'_>) -> Result<(), ImportError> {
    let Some(Value::Array(entries)) = root.get("entries") else { return Err(ImportError::InvalidFile) };
    for entry in entries {
        let Some(content) = entry.get("content").and_then(Value::as_object) else {
            collector.skip();
            continue;
        };
        let uri = str_field(content, "uri");
        if uri.trim().is_empty() {
            collector.skip();
            continue;
        }
        collector.add_link(uri, "", str_field(content, "name"), Extras::default())?;
    }
    Ok(())
}
