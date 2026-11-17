//! Import of Authenticator (browser extension) backups: plain JSON and v3 encrypted JSON.
//!
//! v3 encryption: the entry with `dataType = "Key"` provides a salt and a PHC-format verification hash. The raw hash
//! (base64 without padding) derived from the password with Argon2id(t=2, m=19 MiB, p=1, 32 bytes) is used both for
//! verification and as the CryptoJS OpenSSL `Salted__` passphrase that decrypts each `EncOTPStorage`
//! (AES-256-CBC + EVP_BytesToKey(MD5)). Each keyId is verified once: Argon2id runs with the fixed parameters (19 MiB) to
//! get the candidate passphrase hash, then `verify_phc` checks it with the parameters embedded in the PHC string (usually
//! the same parameters, so that is two Argon2id runs, not one). A failed check reports a wrong password immediately,
//! without retrying entry by entry. A wrong password yields [`ImportError::WrongPassword`]; Safay's own export files go
//! through [`vault::import_export`], where a wrong password is [`vault::VaultError::InvalidPassword`] (the same errors as
//! the vault; the platform maps them to text for the import context).
//!
//! The PHC verification string in the file carries its own `m/t/p` parameters, and the file also decides how many keyIds
//! there are: both are checked against limits before any computation, and anything over the limit reports `InvalidFile`,
//! so a malicious or corrupt backup cannot stall the import.

use std::collections::HashMap;
use std::time::SystemTime;

use aes::cipher::{BlockModeDecrypt, KeyIvInit, block_padding::Pkcs7};
use argon2::password_hash::PasswordVerifier;
use argon2::password_hash::phc::PasswordHash;
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use md5::{Digest, Md5};
use serde_json::{Map, Value};
use zeroize::{Zeroize, Zeroizing};

use crate::base32;
use crate::canonical::{normalized_issuer, normalized_username, standardized_secret};
use crate::model::{Account, OtpType, Placeholders, TotpAlgorithm, provenance};
use crate::vault::{self, VaultContents};

pub use crate::import::{FileKind, ImportError};

const ARGON2_TIME_COST: u32 = 2;
const ARGON2_MEMORY_KIB: u32 = 19 * 1024;
const ARGON2_PARALLELISM: u32 = 1;
/// Maximum number of distinct keyIds verified per backup (Authenticator only ever writes one).
pub const MAX_KEY_IDS: usize = 16;

/// See [`FileKind`], for the root object of a JSON file. Entries are told apart as the import does: `Key` entries are
/// skipped, `EncOTPStorage` is encrypted, and any other entry with a `secret` is a plain one unless marked `encrypted`.
pub(crate) fn inspect_root(root: &Map<String, Value>) -> FileKind {
    let mut kind = FileKind::Unsupported;
    for value in root.values() {
        let Value::Object(item) = value else { continue };
        match string_value(item.get("dataType")).as_deref() {
            Some("Key") => {}
            Some("EncOTPStorage") => return FileKind::Encrypted,
            _ if item.contains_key("secret") => {
                if bool_value(item.get("encrypted")) {
                    return FileKind::Encrypted;
                }
                kind = FileKind::Plain;
            }
            _ => {}
        }
    }
    kind
}

/// [`import_authenticator_contents`] together with how many entries were skipped as unsupported.
pub(crate) fn import_authenticator_counting(
    data: &[u8],
    password: &str,
    now: SystemTime,
    placeholders: &Placeholders,
) -> Result<(VaultContents, usize), ImportError> {
    let (entries, skipped) = import_authenticator_entries(data, password, now, placeholders)?;
    Ok((contents_from(entries), skipped))
}

fn contents_from(entries: Vec<ImportedEntry>) -> VaultContents {
    let pinned_ids = entries.iter().filter(|(_, pinned)| *pinned).map(|(account, _)| account.id).collect();
    VaultContents { accounts: entries.into_iter().map(|(account, _)| account).collect(), categories: Vec::new(), pinned_ids }
}

/// Imports only the Authenticator format, wrapped in [`VaultContents`]: pinned (`pinned`) entries go into `pinned_ids` in
/// order of appearance, with no categories. [`crate::import::import_file`] reads Authenticator backups this way.
pub fn import_authenticator_contents(
    data: &[u8],
    password: &str,
    now: SystemTime,
    placeholders: &Placeholders,
) -> Result<VaultContents, ImportError> {
    Ok(contents_from(import_authenticator_entries(data, password, now, placeholders)?.0))
}

#[cfg(test)]
thread_local! {
    static POSSIBLE_HASH_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Authenticator's passphrase hash: base64 (without padding) of the raw Argon2id output, i.e. the hash part of the PHC
/// string.
pub fn possible_hash(password: &str, salt: &str) -> Option<String> {
    #[cfg(test)]
    POSSIBLE_HASH_CALLS.with(|calls| calls.set(calls.get() + 1));
    let raw = vault::argon2id_raw(password.as_bytes(), salt.as_bytes(), ARGON2_TIME_COST, ARGON2_MEMORY_KIB, ARGON2_PARALLELISM, 32)?;
    Some(base64::engine::general_purpose::STANDARD_NO_PAD.encode(raw.as_slice()))
}

/// The PHC verification string parses and its embedded `m/t/p` are within [`vault::argon2_params_within_limits`].
pub fn phc_within_limits(encoded_hash: &str) -> bool {
    PasswordHash::new(encoded_hash)
        .ok()
        .and_then(|hash| Params::try_from(&hash).ok())
        .is_some_and(|params| vault::argon2_params_within_limits(params.t_cost(), params.m_cost(), params.p_cost()))
}

/// Verifies a PHC-format Argon2id hash (`$argon2id$v=19$m=...,t=...,p=...$salt$hash`). Returns `false` straight away when
/// the parameters exceed the limits.
pub fn verify_phc(value: &str, encoded_hash: &str) -> bool {
    let Ok(hash) = PasswordHash::new(encoded_hash) else { return false };
    if !phc_within_limits(encoded_hash) {
        return false;
    }
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default());
    argon2.verify_password(value.as_bytes(), &hash).is_ok()
}

/// Decrypts the OpenSSL-compatible format of CryptoJS `AES.encrypt(text, passphrase)`.
pub fn cryptojs_decrypt(encrypted: &str, passphrase: &str) -> Option<String> {
    const HEADER: &[u8] = b"Salted__";
    let data = base64::engine::general_purpose::STANDARD.decode(encrypted.trim()).ok()?;
    if data.len() <= HEADER.len() + 8 || &data[..HEADER.len()] != HEADER {
        return None;
    }
    let salt = &data[HEADER.len()..HEADER.len() + 8];
    let ciphertext = &data[HEADER.len() + 8..];
    let material = evp_bytes_to_key(passphrase.as_bytes(), salt, 48);
    let decryptor = cbc::Decryptor::<aes::Aes256>::new_from_slices(&material[..32], &material[32..48]).ok()?;
    let plaintext = Zeroizing::new(decryptor.decrypt_padded_vec::<Pkcs7>(ciphertext).ok()?);
    // Validate before copying out: if decoding fails, the plaintext buffer is wiped by `Zeroizing`.
    std::str::from_utf8(&plaintext).ok().map(str::to_owned)
}

/// OpenSSL `EVP_BytesToKey` (MD5, 1 round). The returned key and IV material is wiped when it goes out of scope.
fn evp_bytes_to_key(password: &[u8], salt: &[u8], length: usize) -> Zeroizing<Vec<u8>> {
    let mut derived = Zeroizing::new(Vec::with_capacity(length + 16));
    let mut previous = Zeroizing::new(Vec::new());
    while derived.len() < length {
        let mut hasher = Md5::new();
        hasher.update(&previous);
        hasher.update(password);
        hasher.update(salt);
        let mut digest = hasher.finalize();
        previous.clear();
        previous.extend_from_slice(&digest);
        digest.as_mut_slice().zeroize();
        derived.extend_from_slice(&previous);
    }
    derived.truncate(length);
    derived
}

// Value rules of JSONSerialization + NSNumber bridging: numbers, booleans, and strings convert into each other.
fn string_value(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(string) => Some(string.clone()),
        Value::Bool(flag) => Some(if *flag { "1" } else { "0" }.to_owned()),
        Value::Number(number) => Some(match (number.as_i64(), number.as_u64(), number.as_f64()) {
            (Some(int), _, _) => int.to_string(),
            (_, Some(uint), _) => uint.to_string(),
            (_, _, Some(float)) if float.fract() == 0.0 && float.abs() < 1e15 => format!("{}", float as i64),
            (_, _, Some(float)) => float.to_string(),
            _ => number.to_string(),
        }),
        _ => None,
    }
}

fn int_value(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64().or_else(|| number.as_f64().map(|float| float.trunc() as i64)),
        Value::Bool(flag) => Some(i64::from(*flag)),
        Value::String(string) => string.parse::<i64>().ok(),
        _ => None,
    }
}

fn bool_value(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|float| float != 0.0),
        Some(Value::String(string)) => matches!(string.to_lowercase().as_str(), "true" | "1" | "yes"),
        _ => false,
    }
}

fn normalized_type(value: Option<&Value>) -> String {
    match string_value(value).map(|raw| raw.to_lowercase()) {
        Some(raw) => match raw.as_str() {
            "1" => "totp".to_owned(),
            "2" => "hotp".to_owned(),
            "3" => "battle".to_owned(),
            "4" => "steam".to_owned(),
            "5" => "hex".to_owned(),
            "6" => "hhex".to_owned(),
            _ => raw,
        },
        None => "totp".to_owned(),
    }
}

fn normalized_algorithm(value: Option<&Value>) -> Option<TotpAlgorithm> {
    let raw = string_value(value).map(|raw| raw.to_uppercase()).unwrap_or_default();
    match raw.as_str() {
        "" | "1" | "SHA1" => Some(TotpAlgorithm::Sha1),
        "2" | "SHA256" => Some(TotpAlgorithm::Sha256),
        "3" | "SHA512" => Some(TotpAlgorithm::Sha512),
        _ => None,
    }
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    data_encoding::HEXLOWER_PERMISSIVE.decode(value.trim().as_bytes()).ok()
}

type Entry<'a> = (&'a String, &'a Map<String, Value>);

fn sort_entries(entries: &mut [Entry<'_>]) {
    entries.sort_by(|left, right| {
        let left_index = int_value(left.1.get("index")).unwrap_or(i64::MAX);
        let right_index = int_value(right.1.get("index")).unwrap_or(i64::MAX);
        left_index.cmp(&right_index).then_with(|| left.0.cmp(right.0))
    });
}

/// One Authenticator entry: the account and whether it is pinned.
type ImportedEntry = (Account, bool);

fn account_from_raw(
    item: &Map<String, Value>,
    password: &str,
    now: SystemTime,
    placeholders: &Placeholders,
) -> Result<Option<ImportedEntry>, ImportError> {
    let Some(stored) = string_value(item.get("secret")).map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let secret = if bool_value(item.get("encrypted")) {
        if password.is_empty() {
            return Err(ImportError::PasswordRequired);
        }
        Zeroizing::new(cryptojs_decrypt(&stored, password).ok_or(ImportError::WrongPassword)?)
    } else {
        Zeroizing::new(stored)
    };
    let (otp_type, secret) = match normalized_type(item.get("type")).as_str() {
        "totp" => (OtpType::Totp, secret),
        "steam" => (OtpType::Steam, secret),
        "battle" => (OtpType::BattleNet, secret),
        "hex" => {
            let Some(bytes) = hex_decode(&secret).map(Zeroizing::new) else { return Ok(None) };
            (OtpType::Totp, Zeroizing::new(base32::encode(&bytes)))
        }
        _ => return Ok(None),
    };
    let Some(secret) = standardized_secret(&secret) else { return Ok(None) };
    // Steam Guard and Battle.net use fixed parameters, whatever the backup states.
    let (algorithm, digits, period) = match otp_type.fixed_parameters() {
        Some((algorithm, digits, period)) => (algorithm, i64::from(digits), i64::from(period)),
        None => {
            let Some(algorithm) = normalized_algorithm(item.get("algorithm")) else { return Ok(None) };
            let digits = int_value(item.get("digits")).unwrap_or(6);
            if digits != 6 && digits != 8 {
                return Ok(None);
            }
            (algorithm, digits, int_value(item.get("period")).unwrap_or(30).clamp(1, i64::from(u32::MAX)))
        }
    };
    let account = Account {
        id: vault::random_uuid().map_err(ImportError::Vault)?,
        issuer: normalized_issuer(&string_value(item.get("issuer")).unwrap_or_default(), placeholders),
        username: normalized_username(&string_value(item.get("account")).unwrap_or_default(), placeholders),
        secret,
        otp_type,
        algorithm,
        digits: digits as u32,
        period: period as u32,
        created_at: now,
        added_by: provenance::IMPORTED.to_owned(),
        website: None,
        icon: None,
        deleted_at: None,
    };
    Ok(Some((account, bool_value(item.get("pinned")))))
}

/// Accounts only, without pinned flags (for unit tests and golden files; imports use [`import_authenticator_contents`]).
pub fn import_authenticator(
    data: &[u8],
    password: &str,
    now: SystemTime,
    placeholders: &Placeholders,
) -> Result<Vec<Account>, ImportError> {
    Ok(import_authenticator_entries(data, password, now, placeholders)?.0.into_iter().map(|(account, _)| account).collect())
}

fn import_authenticator_entries(
    data: &[u8],
    password: &str,
    now: SystemTime,
    placeholders: &Placeholders,
) -> Result<(Vec<ImportedEntry>, usize), ImportError> {
    let root: Map<String, Value> = match serde_json::from_slice(data) {
        Ok(Value::Object(map)) => map,
        _ => return Err(ImportError::InvalidFile),
    };

    let mut keys: HashMap<String, (String, String)> = HashMap::new();
    let mut encrypted: Vec<Entry<'_>> = Vec::new();
    let mut raw: Vec<Entry<'_>> = Vec::new();
    for (hash, value) in &root {
        let Value::Object(item) = value else { continue };
        match string_value(item.get("dataType")).as_deref() {
            Some("Key") => {
                if let (Some(id), Some(salt), Some(verification)) =
                    (string_value(item.get("id")), string_value(item.get("salt")), string_value(item.get("hash")))
                {
                    keys.insert(id, (salt, verification));
                }
            }
            Some("EncOTPStorage") => encrypted.push((hash, item)),
            _ if item.contains_key("secret") => raw.push((hash, item)),
            _ => {}
        }
    }
    if encrypted.is_empty() && raw.is_empty() {
        return Err(ImportError::InvalidFile);
    }
    if !encrypted.is_empty() && password.is_empty() {
        return Err(ImportError::PasswordRequired);
    }
    sort_entries(&mut raw);
    sort_entries(&mut encrypted);

    let mut accounts: Vec<ImportedEntry> = Vec::new();
    let mut skipped = 0usize;
    for (_, item) in &raw {
        match account_from_raw(item, password, now, placeholders)? {
            Some(entry) => accounts.push(entry),
            None => skipped += 1,
        }
    }

    // First settle which keyIds to verify (in order of first appearance, deduplicated, each with a matching Key entry); both
    // their count and parameters pass the limit checks before anything is computed.
    let mut key_ids: Vec<String> = Vec::new();
    for (_, item) in &encrypted {
        if let Some(key_id) = string_value(item.get("keyId")).filter(|id| keys.contains_key(id) && !key_ids.contains(id)) {
            key_ids.push(key_id);
        }
    }
    if key_ids.len() > MAX_KEY_IDS || key_ids.iter().any(|id| !phc_within_limits(&keys[id].1)) {
        return Err(ImportError::InvalidFile);
    }

    // Each keyId is verified once (one Argon2id run each for possible_hash and verify_phc); a failed check reports a wrong
    // password immediately.
    let mut unlocked: HashMap<String, Zeroizing<String>> = HashMap::new();
    for key_id in key_ids {
        let (salt, verification) = &keys[&key_id];
        let candidate = Zeroizing::new(possible_hash(password, salt).ok_or(ImportError::InvalidFile)?);
        if !verify_phc(&candidate, verification) {
            return Err(ImportError::WrongPassword);
        }
        unlocked.insert(key_id, candidate);
    }

    let mut failed = 0usize;
    let mut decrypted = 0usize;
    for (_, item) in &encrypted {
        let passphrase = string_value(item.get("keyId")).and_then(|id| unlocked.get(&id).cloned());
        let (Some(passphrase), Some(ciphertext)) = (passphrase, string_value(item.get("data"))) else {
            failed += 1;
            continue;
        };
        let Some(Value::Object(mut raw_item)) =
            cryptojs_decrypt(&ciphertext, &passphrase).map(Zeroizing::new).and_then(|text| serde_json::from_str(&text).ok())
        else {
            failed += 1;
            continue;
        };
        decrypted += 1;
        raw_item.insert("encrypted".to_owned(), Value::Bool(false));
        match account_from_raw(&raw_item, password, now, placeholders) {
            Ok(Some(entry)) => accounts.push(entry),
            Ok(None) => skipped += 1,
            Err(_) => failed += 1,
        }
    }

    if accounts.is_empty() && !encrypted.is_empty() && decrypted == 0 && failed == encrypted.len() {
        return Err(ImportError::WrongPassword);
    }
    if accounts.is_empty() {
        return Err(ImportError::NoSupportedAccounts);
    }
    Ok((accounts, skipped + failed))
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::import::{import_file, inspect};
    use crate::otpauth::tests::placeholders;

    #[test]
    fn imports_plaintext_backup() {
        let data = serde_json::json!({
            "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa": {
                "account": " alex%40example.com%0A ", "algorithm": "SHA256", "dataType": "OTPStorage",
                "digits": 8, "encrypted": false, "index": 0, "issuer": " GitHub%20Inc%0A ",
                "period": 60, "secret": "mzxw 6ytb-oi%0A", "type": "totp"
            },
            "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb": {
                "account": "hex@example.com", "encrypted": false, "index": 1, "issuer": "Hex",
                "secret": "666f6f626172", "type": "hex"
            },
            "cccccccc-cccc-4ccc-8ccc-cccccccccccc": {
                "account": "hotp@example.com", "encrypted": false, "index": 2, "issuer": "HOTP",
                "secret": "MZXW6YTBOI", "type": "hotp"
            },
            "dddddddd-dddd-4ddd-8ddd-dddddddddddd": {
                "account": "gamer", "algorithm": "SHA256", "digits": 8, "encrypted": false, "index": 3, "issuer": "Steam",
                "period": 60, "secret": "MZXW6YTBOI", "type": "steam"
            },
            "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee": {
                "account": "hero", "algorithm": "SHA256", "encrypted": false, "index": 4, "issuer": "Battle.net",
                "period": 60, "secret": "MZXW6YTBOI", "type": "battle"
            },
            "ffffffff-ffff-4fff-8fff-ffffffffffff": {
                "account": "old", "encrypted": false, "index": 5, "issuer": "Battle.net", "secret": "MZXW6YTBOI", "type": "3"
            }
        });
        let accounts = import_authenticator(data.to_string().as_bytes(), "", SystemTime::UNIX_EPOCH, &placeholders()).unwrap();
        assert_eq!(accounts.len(), 5);
        // Steam Guard's parameters are fixed, whatever the backup states.
        assert_eq!((accounts[2].algorithm, accounts[2].digits, accounts[2].period), (TotpAlgorithm::Sha1, 5, 30));
        assert_eq!(accounts[2].otp_type, OtpType::Steam);
        for battle in &accounts[3..] {
            assert_eq!((battle.otp_type, battle.algorithm, battle.digits, battle.period), (OtpType::BattleNet, TotpAlgorithm::Sha1, 8, 30));
        }
        assert_eq!(accounts[0].issuer, "GitHub Inc");
        assert_eq!(accounts[0].username, "alex@example.com");
        assert_eq!(accounts[0].secret, "MZXW6YTBOI");
        assert_eq!(accounts[0].algorithm, TotpAlgorithm::Sha256);
        assert_eq!((accounts[0].digits, accounts[0].period), (8, 60));
        assert_eq!(accounts[1].secret, "MZXW6YTBOI");
    }

    #[test]
    fn reports_invalid_and_password_errors() {
        let p = placeholders();
        assert_eq!(import_authenticator(b"[]", "", SystemTime::UNIX_EPOCH, &p).unwrap_err(), ImportError::InvalidFile);
        assert_eq!(import_authenticator(b"{\"x\":{}}", "", SystemTime::UNIX_EPOCH, &p).unwrap_err(), ImportError::InvalidFile);
        let encrypted = serde_json::json!({"e": {"dataType": "EncOTPStorage", "keyId": "k", "data": "x"}});
        assert_eq!(
            import_authenticator(encrypted.to_string().as_bytes(), "", SystemTime::UNIX_EPOCH, &p).unwrap_err(),
            ImportError::PasswordRequired
        );
        assert_eq!(
            import_authenticator(encrypted.to_string().as_bytes(), "pw", SystemTime::UNIX_EPOCH, &p).unwrap_err(),
            ImportError::WrongPassword
        );
    }

    /// With several encrypted records under one keyId, a wrong password costs one candidate hash (plus one PHC check)
    /// before the error, without retrying record by record.
    #[test]
    fn wrong_password_verifies_each_key_once() {
        let mut root = golden_encrypted_backup();
        for index in 0..5 {
            root.insert(
                format!("extra-{index}"),
                serde_json::json!({"dataType": "EncOTPStorage", "keyId": "key-1", "index": 10 + index, "data": "not-decrypted"}),
            );
        }
        let data = Value::Object(root).to_string();
        POSSIBLE_HASH_CALLS.with(|calls| calls.set(0));
        let error = import_authenticator(data.as_bytes(), "wrong-password", SystemTime::UNIX_EPOCH, &placeholders()).unwrap_err();
        assert_eq!(error, ImportError::WrongPassword);
        assert_eq!(POSSIBLE_HASH_CALLS.with(std::cell::Cell::get), 1);
    }

    /// `inspect` agrees with `import_file` given no password: `Encrypted` exactly where it reports `PasswordRequired`,
    /// `Unsupported` exactly where it reports `InvalidFile`, for every golden file; a Safay export is `Encrypted`.
    #[test]
    fn inspect_matches_import_file() {
        let golden: Value = serde_json::from_str(include_str!("../../../golden/authenticator.json")).unwrap();
        for case in golden["imports"].as_array().unwrap() {
            let file = case["file"].as_str().unwrap().as_bytes();
            let expected = match import_file(file, "", SystemTime::UNIX_EPOCH, &placeholders()) {
                Err(ImportError::PasswordRequired) => FileKind::Encrypted,
                Err(ImportError::InvalidFile) => FileKind::Unsupported,
                _ => FileKind::Plain,
            };
            assert_eq!(inspect(file), expected, "{}", case["name"]);
        }
        let export = vault::export(&[], &[], "export-pw", SystemTime::UNIX_EPOCH).unwrap();
        assert_eq!(inspect(&export), FileKind::Encrypted);
        assert_eq!(inspect(b"%PDF-1.7 not a backup"), FileKind::Unsupported);
    }

    /// `import_file` dispatch: a Safay export file needs a password first (an empty string reports `PasswordRequired`; all
    /// whitespace reaches the core and reports `EmptyPassword`); anything that is not an export file is parsed as
    /// Authenticator, with pinned entries in `pinned_ids`.
    #[test]
    fn import_file_dispatches_between_safay_export_and_authenticator() {
        use crate::model::TotpAlgorithm;
        let p = placeholders();
        let account = Account {
            id: Uuid::from_u128(1),
            issuer: "GitHub".into(),
            username: "alex".into(),
            secret: "MZXW6YTBOI".into(),
            otp_type: crate::model::OtpType::Totp,
            algorithm: TotpAlgorithm::Sha1,
            digits: 6,
            period: 30,
            created_at: SystemTime::UNIX_EPOCH,
            added_by: provenance::MANUAL.into(),
            website: None,
            icon: None,
            deleted_at: None,
        };
        let export = vault::export(&[account], &[], "export-pw", SystemTime::UNIX_EPOCH).unwrap();
        assert_eq!(import_file(&export, "", SystemTime::UNIX_EPOCH, &p).unwrap_err(), ImportError::PasswordRequired);
        assert_eq!(
            import_file(&export, "   ", SystemTime::UNIX_EPOCH, &p).unwrap_err(),
            ImportError::Vault(vault::VaultError::EmptyPassword)
        );
        assert_eq!(
            import_file(&export, "wrong", SystemTime::UNIX_EPOCH, &p).unwrap_err(),
            ImportError::Vault(vault::VaultError::InvalidPassword),
            "a wrong password on a Safay export is InvalidPassword"
        );
        assert_eq!(import_file(&export, "export-pw", SystemTime::UNIX_EPOCH, &p).unwrap().contents.accounts[0].secret, "MZXW6YTBOI");

        let backup = serde_json::json!({
            "a": {"account": "one", "encrypted": false, "index": 1, "issuer": "A", "secret": "MZXW6YTBOI", "pinned": true},
            "b": {"account": "two", "encrypted": false, "index": 0, "issuer": "B", "secret": "MZXW6YTBOI"},
        })
        .to_string();
        let imported = import_file(backup.as_bytes(), "", SystemTime::UNIX_EPOCH, &p).unwrap().contents;
        assert_eq!(imported.accounts.iter().map(|a| a.issuer.as_str()).collect::<Vec<_>>(), vec!["B", "A"]);
        assert_eq!(imported.pinned_ids, vec![imported.accounts[1].id], "pinned entries go into pinned_ids");
        assert!(imported.categories.is_empty());
        let direct = import_authenticator_contents(backup.as_bytes(), "", SystemTime::UNIX_EPOCH, &p).unwrap();
        assert_eq!(direct.pinned_ids, vec![direct.accounts[1].id], "import_file takes the same path");
        assert_eq!(import_file(b"[]", "", SystemTime::UNIX_EPOCH, &p).unwrap_err(), ImportError::InvalidFile);
        let encrypted = serde_json::json!({"e": {"dataType": "EncOTPStorage", "keyId": "k", "data": "x"}}).to_string();
        assert_eq!(
            import_file(encrypted.as_bytes(), "pw", SystemTime::UNIX_EPOCH, &p).unwrap_err(),
            ImportError::WrongPassword,
            "a wrong password on an Authenticator backup is WrongPassword"
        );
    }

    fn golden_encrypted_backup() -> Map<String, Value> {
        let golden: Value = serde_json::from_str(include_str!("../../../golden/authenticator.json")).unwrap();
        let sample = golden["imports"].as_array().unwrap().iter().find(|s| s["name"] == "encrypted").unwrap();
        serde_json::from_str(sample["file"].as_str().unwrap()).unwrap()
    }

    /// Parameters embedded in the PHC string that exceed the limits make the file invalid without running anything
    /// (t = 4 billion would hang for years).
    #[test]
    fn rejects_phc_parameters_beyond_limits() {
        let mut root = golden_encrypted_backup();
        for item in root.values_mut() {
            if item["dataType"] == "Key" {
                let hash = item["hash"].as_str().unwrap().replace("t=2", "t=4000000000");
                item["hash"] = hash.into();
            }
        }
        let data = Value::Object(root).to_string();
        let started = std::time::Instant::now();
        let error = import_authenticator(data.as_bytes(), "any", SystemTime::UNIX_EPOCH, &placeholders()).unwrap_err();
        assert_eq!(error, ImportError::InvalidFile);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert!(!phc_within_limits("$argon2id$v=19$m=4294967295,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA"));
        assert!(!verify_phc("x", "$argon2id$v=19$m=19456,t=4000000000,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA"));
    }

    /// The file decides the number of keyIds, and each one costs an Argon2id run: more than the limit makes the file invalid.
    #[test]
    fn rejects_too_many_key_ids() {
        let mut root = golden_encrypted_backup();
        let key = root.values().find(|item| item["dataType"] == "Key").unwrap().clone();
        for index in 0..MAX_KEY_IDS {
            let mut extra_key = key.clone();
            extra_key["id"] = format!("extra-key-{index}").into();
            root.insert(format!("key-entry-{index}"), extra_key);
            root.insert(
                format!("entry-{index}"),
                serde_json::json!({"dataType": "EncOTPStorage", "keyId": format!("extra-key-{index}"), "data": "x"}),
            );
        }
        let data = Value::Object(root).to_string();
        POSSIBLE_HASH_CALLS.with(|calls| calls.set(0));
        let error = import_authenticator(data.as_bytes(), "any", SystemTime::UNIX_EPOCH, &placeholders()).unwrap_err();
        assert_eq!(error, ImportError::InvalidFile);
        assert_eq!(POSSIBLE_HASH_CALLS.with(std::cell::Cell::get), 0);
    }

    const SALT: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

    #[test]
    fn evp_bytes_to_key_matches_openssl() {
        // OpenSSL 3.6: `openssl enc -aes-256-cbc -md md5 -P -S 0102030405060708 -pass pass:secret`
        let material = evp_bytes_to_key(b"secret", &SALT, 48);
        let hex = data_encoding::HEXUPPER.encode(&material);
        assert_eq!(&hex[..64], "C9E5A1BD216DBE1317E230CEF48F38EE7F0E17AD64022144BCCEC4A1AA2879AB");
        assert_eq!(&hex[64..], "E24B32BBBC4EF02ECBCB6576523AD893");
    }

    #[test]
    fn decrypts_cryptojs_openssl_format() {
        // `openssl enc -aes-256-cbc -md md5 -S 0102030405060708 -pass pass:possible-hash-value -base64 -A`
        // has no Salted__ header (OpenSSL 3 omits it when -S is given explicitly), so it is added here in CryptoJS format.
        let body =
            base64::engine::general_purpose::STANDARD.decode("jR2cFAoruF4Q45t5SqmIE56P5DUVbCVmgzCAi83j/+q8RO4y/oCbSe2pAJCHeQou").unwrap();
        let mut framed = b"Salted__".to_vec();
        framed.extend_from_slice(&SALT);
        framed.extend_from_slice(&body);
        let encoded = base64::engine::general_purpose::STANDARD.encode(framed);
        assert_eq!(cryptojs_decrypt(&encoded, "possible-hash-value").as_deref(), Some("{\"secret\":\"MZXW6YTBOI\",\"type\":\"totp\"}"));
        assert_eq!(cryptojs_decrypt(&encoded, "wrong"), None);
    }
}
