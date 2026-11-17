//! Aegis vault exports, plain and encrypted.
//!
//! Encrypted vaults wrap a random master key in one slot per credential. Password slots (type 1) derive a key with
//! scrypt(N, r, p, salt) from the file and unwrap the master key with AES-256-GCM (`key` + `key_params`); the vault
//! contents in `db` are AES-256-GCM with the master key and `header.params`. Tags and nonces are hex, `db` is Base64.

use aes_gcm::aead::{Aead, Nonce};
use aes_gcm::{Aes256Gcm, KeyInit};
use serde_json::{Map, Value};
use zeroize::Zeroizing;

use super::{Collector, Extras, Fields, ImportError, base64_field, bool_field, int_field, parse_plaintext, str_field};

/// Largest scrypt cost accepted from a file: N·r (memory, 128·N·r bytes) and N·r·p (work). Aegis writes N = 2^15, r = 8,
/// p = 1 (32 MiB).
const MAX_SCRYPT_NR: u64 = 1 << 21;
const MAX_SCRYPT_NRP: u64 = 1 << 22;
/// Password slots tried per file (Aegis writes one).
const MAX_PASSWORD_SLOTS: usize = 8;

/// `Some(encrypted)` for an Aegis vault.
pub(super) fn detect(root: &Map<String, Value>) -> Option<bool> {
    if !root.contains_key("version") || !root.get("header").is_some_and(Value::is_object) {
        return None;
    }
    match root.get("db")? {
        Value::String(_) => Some(true),
        Value::Object(db) if db.contains_key("entries") => Some(false),
        _ => None,
    }
}

pub(super) fn import(root: &Map<String, Value>, password: &str, collector: &mut Collector<'_>) -> Result<(), ImportError> {
    match root.get("db") {
        Some(Value::Object(db)) => read_contents(db, collector),
        Some(Value::String(_)) => {
            let plaintext = decrypt(root, password)?;
            match parse_plaintext(plaintext)? {
                Value::Object(db) => read_contents(&db, collector),
                _ => Err(ImportError::InvalidFile),
            }
        }
        _ => Err(ImportError::InvalidFile),
    }
}

fn hex(value: &str) -> Result<Vec<u8>, ImportError> {
    data_encoding::HEXLOWER_PERMISSIVE.decode(value.trim().as_bytes()).map_err(|_| ImportError::InvalidFile)
}

/// AES-256-GCM with a separate tag, as Aegis stores it. `None` when authentication fails.
fn open(key: &[u8], nonce: &[u8], ciphertext: &[u8], tag: &[u8]) -> Result<Option<Zeroizing<Vec<u8>>>, ImportError> {
    if key.len() != 32 || nonce.len() != 12 || tag.len() != 16 {
        return Err(ImportError::InvalidFile);
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| ImportError::InvalidFile)?;
    let nonce = Nonce::<Aes256Gcm>::try_from(nonce).map_err(|_| ImportError::InvalidFile)?;
    let mut sealed = Vec::with_capacity(ciphertext.len() + tag.len());
    sealed.extend_from_slice(ciphertext);
    sealed.extend_from_slice(tag);
    Ok(cipher.decrypt(&nonce, sealed.as_slice()).ok().map(Zeroizing::new))
}

fn params(object: &Map<String, Value>) -> Result<(Vec<u8>, Vec<u8>), ImportError> {
    let Some(Value::Object(params)) = object.get("params").or_else(|| object.get("key_params")) else {
        return Err(ImportError::InvalidFile);
    };
    Ok((hex(str_field(params, "nonce"))?, hex(str_field(params, "tag"))?))
}

struct PasswordSlot {
    log_n: u8,
    r: u32,
    p: u32,
    salt: Vec<u8>,
    key: Vec<u8>,
    nonce: Vec<u8>,
    tag: Vec<u8>,
}

fn password_slots(header: &Map<String, Value>) -> Result<Vec<PasswordSlot>, ImportError> {
    let Some(Value::Array(slots)) = header.get("slots") else { return Err(ImportError::InvalidFile) };
    let mut result = Vec::new();
    for slot in slots {
        let Value::Object(slot) = slot else { continue };
        if int_field(slot, "type") != Some(1) {
            continue;
        }
        let (Some(n), Some(r), Some(p)) = (int_field(slot, "n"), int_field(slot, "r"), int_field(slot, "p")) else {
            return Err(ImportError::InvalidFile);
        };
        let (Ok(n), Ok(r), Ok(p)) = (u64::try_from(n), u32::try_from(r), u32::try_from(p)) else {
            return Err(ImportError::InvalidFile);
        };
        if n < 2 || !n.is_power_of_two() || r == 0 || p == 0 {
            return Err(ImportError::InvalidFile);
        }
        let nr = n.checked_mul(u64::from(r)).ok_or(ImportError::InvalidFile)?;
        if nr > MAX_SCRYPT_NR || nr.saturating_mul(u64::from(p)) > MAX_SCRYPT_NRP {
            return Err(ImportError::InvalidFile);
        }
        let Some(Value::Object(key_params)) = slot.get("key_params") else { return Err(ImportError::InvalidFile) };
        result.push(PasswordSlot {
            log_n: n.trailing_zeros() as u8,
            r,
            p,
            salt: hex(str_field(slot, "salt"))?,
            key: hex(str_field(slot, "key"))?,
            nonce: hex(str_field(key_params, "nonce"))?,
            tag: hex(str_field(key_params, "tag"))?,
        });
    }
    if result.is_empty() || result.len() > MAX_PASSWORD_SLOTS {
        return Err(ImportError::InvalidFile);
    }
    Ok(result)
}

fn decrypt(root: &Map<String, Value>, password: &str) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    let Some(Value::Object(header)) = root.get("header") else { return Err(ImportError::InvalidFile) };
    let slots = password_slots(header)?;
    let (nonce, tag) = params(header)?;
    let db = base64_field(root, "db")?;
    for slot in slots {
        let params = scrypt::Params::new(slot.log_n, slot.r, slot.p).map_err(|_| ImportError::InvalidFile)?;
        let mut derived = Zeroizing::new([0u8; 32]);
        scrypt::scrypt(password.as_bytes(), &slot.salt, &params, derived.as_mut_slice()).map_err(|_| ImportError::InvalidFile)?;
        let Some(master) = open(derived.as_slice(), &slot.nonce, &slot.key, &slot.tag)? else { continue };
        // The password unwrapped the master key: a failure from here on is a corrupt file, not a wrong password.
        return open(&master, &nonce, &db, &tag)?.ok_or(ImportError::InvalidFile);
    }
    Err(ImportError::WrongPassword)
}

fn read_contents(db: &Map<String, Value>, collector: &mut Collector<'_>) -> Result<(), ImportError> {
    let Some(Value::Array(entries)) = db.get("entries") else { return Err(ImportError::InvalidFile) };
    let groups: Vec<(&str, &str)> = match db.get("groups") {
        Some(Value::Array(groups)) => {
            groups.iter().filter_map(Value::as_object).map(|group| (str_field(group, "uuid"), str_field(group, "name"))).collect()
        }
        _ => Vec::new(),
    };
    for entry in entries {
        let Value::Object(entry) = entry else {
            collector.skip();
            continue;
        };
        let Some(Value::Object(info)) = entry.get("info") else {
            collector.skip();
            continue;
        };
        // Content version 3 lists group UUIDs; older versions name a single group.
        let mut names: Vec<String> = match entry.get("groups") {
            Some(Value::Array(ids)) => ids
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|id| groups.iter().find(|(uuid, _)| uuid.eq_ignore_ascii_case(id)).map(|(_, name)| (*name).to_owned()))
                .collect(),
            _ => Vec::new(),
        };
        if let Some(Value::String(group)) = entry.get("group") {
            names.push(group.clone());
        }
        let fields = Fields {
            kind: str_field(entry, "type"),
            issuer: str_field(entry, "issuer"),
            username: str_field(entry, "name"),
            secret: str_field(info, "secret"),
            algorithm: str_field(info, "algo"),
            digits: int_field(info, "digits"),
            period: int_field(info, "period"),
        };
        collector.add_fields(&fields, Extras { groups: names, pinned: bool_field(entry, "favorite"), website: None })?;
    }
    Ok(())
}
