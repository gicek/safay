//! Imports from backup and export files: Safay's own encrypted export, Authenticator (browser extension), Aegis, 2FAS,
//! Ente Auth, Proton Authenticator, Bitwarden, password-manager CSV exports (Apple Passwords, Bitwarden, and others with
//! a one-time password column), and plain text with one `otpauth://` link per line.
//!
//! [`inspect`] tells the format and whether it needs a password from the file's structure alone; [`import_file`] reads it.
//! Every format goes through one set of rules ([`Collector`]): only TOTP with SHA-1, SHA-256, or SHA-512 and 6 or 8 digits
//! is imported, and anything else (HOTP, Steam, Yandex, mOTP, other digit counts, entries in a trash) is skipped and
//! counted instead of failing the file. Groups, tags, and folders become categories, and favorite or pinned entries go
//! into `pinned_ids`.
//!
//! Key derivation parameters read from a file are checked against limits before anything is computed, so a malicious or
//! corrupt file cannot stall the import.

mod aegis;
mod bitwarden;
mod csv;
mod ente;
mod proton;
mod text;
mod twofas;

use std::time::SystemTime;

use serde_json::{Map, Value};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::authenticator;
use crate::canonical::{clean_text, normalized_issuer, normalized_username, standardized_secret};
use crate::category::{self, Category, MAX_NAME_CHARS};
use crate::model::{Account, Placeholders, TotpAlgorithm, provenance};
use crate::otpauth;
use crate::site_icon;
use crate::vault::{self, VaultContents};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    #[error("a password is required")]
    PasswordRequired,
    #[error("not a supported backup file")]
    InvalidFile,
    #[error("wrong password")]
    WrongPassword,
    #[error("no supported accounts")]
    NoSupportedAccounts,
    #[error("safay export: {0}")]
    Vault(vault::VaultError),
}

impl ImportError {
    pub fn l10n_key(self) -> &'static str {
        match self {
            ImportError::PasswordRequired => "err.import.password_required",
            ImportError::InvalidFile => "err.import.invalid_file",
            ImportError::WrongPassword => "err.import.wrong_password",
            ImportError::NoSupportedAccounts => "err.import.no_supported",
            ImportError::Vault(error) => error.l10n_key(),
        }
    }
}

/// The file formats [`import_file`] reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    SafayExport,
    Authenticator,
    Aegis,
    TwoFas,
    /// Ente Auth's encrypted export (its plain export is [`Format::Text`]).
    Ente,
    Proton,
    Bitwarden,
    /// A CSV export with a one-time password column.
    Csv,
    /// One `otpauth://` or `otpauth-migration://` link per line.
    Text,
}

/// What a file chosen to import is, from its structure alone (no key derivation, nothing decrypted), as [`import_file`]
/// would treat it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// It needs a password.
    Encrypted,
    /// It opens without a password.
    Plain,
    /// Not a supported format: [`import_file`] reports `InvalidFile` for it.
    Unsupported,
}

/// What [`import_file`] read: the accounts (with categories and pinned entries) and how many entries it skipped because
/// Safay does not support them.
#[derive(Clone, Debug, Default)]
pub struct Imported {
    pub contents: VaultContents,
    pub skipped: usize,
}

/// The format of `data` and whether it is encrypted, or `None` when no supported format matches.
pub fn detect(data: &[u8]) -> Option<(Format, bool)> {
    if vault::is_safay_export(data) {
        return Some((Format::SafayExport, true));
    }
    if let Ok(Value::Object(root)) = serde_json::from_slice::<Value>(data) {
        return detect_json(&root);
    }
    let text = std::str::from_utf8(data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data)).ok()?;
    if csv::is_csv(text) {
        return Some((Format::Csv, false));
    }
    text::is_text(text).then_some((Format::Text, false))
}

fn detect_json(root: &Map<String, Value>) -> Option<(Format, bool)> {
    if let Some(encrypted) = aegis::detect(root) {
        return Some((Format::Aegis, encrypted));
    }
    if let Some(encrypted) = twofas::detect(root) {
        return Some((Format::TwoFas, encrypted));
    }
    if ente::detect(root) {
        return Some((Format::Ente, true));
    }
    if let Some(encrypted) = proton::detect(root) {
        return Some((Format::Proton, encrypted));
    }
    if bitwarden::detect(root) {
        return Some((Format::Bitwarden, false));
    }
    match authenticator::inspect_root(root) {
        FileKind::Encrypted => Some((Format::Authenticator, true)),
        FileKind::Plain => Some((Format::Authenticator, false)),
        FileKind::Unsupported => None,
    }
}

/// See [`FileKind`].
pub fn inspect(data: &[u8]) -> FileKind {
    match detect(data) {
        Some((_, true)) => FileKind::Encrypted,
        Some((_, false)) => FileKind::Plain,
        None => FileKind::Unsupported,
    }
}

/// Imports any supported file. A password is only used by encrypted formats; an empty one reports `PasswordRequired`
/// for them.
pub fn import_file(data: &[u8], password: &str, now: SystemTime, placeholders: &Placeholders) -> Result<Imported, ImportError> {
    let Some((format, encrypted)) = detect(data) else { return Err(ImportError::InvalidFile) };
    if encrypted && password.is_empty() {
        return Err(ImportError::PasswordRequired);
    }
    let mut collector = Collector::new(now, placeholders);
    match format {
        Format::SafayExport => {
            let contents = vault::import_export(data, password).map_err(ImportError::Vault)?;
            return Ok(Imported { contents, skipped: 0 });
        }
        Format::Authenticator => {
            let (contents, skipped) = authenticator::import_authenticator_counting(data, password, now, placeholders)?;
            return Ok(Imported { contents, skipped });
        }
        Format::Aegis => aegis::import(&json_root(data)?, password, &mut collector)?,
        Format::TwoFas => twofas::import(&json_root(data)?, password, &mut collector)?,
        Format::Ente => ente::import(&json_root(data)?, password, &mut collector)?,
        Format::Proton => proton::import(&json_root(data)?, password, &mut collector)?,
        Format::Bitwarden => bitwarden::import(&json_root(data)?, &mut collector)?,
        Format::Csv => csv::import(utf8(data)?, &mut collector)?,
        Format::Text => text::import(utf8(data)?, &mut collector)?,
    }
    collector.finish()
}

fn json_root(data: &[u8]) -> Result<Map<String, Value>, ImportError> {
    match serde_json::from_slice(data) {
        Ok(Value::Object(root)) => Ok(root),
        _ => Err(ImportError::InvalidFile),
    }
}

fn utf8(data: &[u8]) -> Result<&str, ImportError> {
    std::str::from_utf8(data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data)).map_err(|_| ImportError::InvalidFile)
}

/// One entry's one-time password parameters, as a file states them. Empty or missing values take the otpauth defaults.
#[derive(Default)]
pub(crate) struct Fields<'a> {
    /// `totp` (any case) or empty for TOTP; anything else is skipped.
    pub kind: &'a str,
    pub issuer: &'a str,
    pub username: &'a str,
    pub secret: &'a str,
    pub algorithm: &'a str,
    pub digits: Option<i64>,
    pub period: Option<i64>,
}

/// What an entry carries besides its code: the groups (category names) it belongs to, whether it is pinned or a
/// favorite, and a website from a password manager.
#[derive(Default)]
pub(crate) struct Extras {
    pub groups: Vec<String>,
    pub pinned: bool,
    pub website: Option<String>,
}

/// Collects accounts from any format under the same rules, counting what it skips.
pub(crate) struct Collector<'a> {
    now: SystemTime,
    pub placeholders: &'a Placeholders,
    accounts: Vec<Account>,
    categories: Vec<Category>,
    pinned_ids: Vec<Uuid>,
    skipped: usize,
}

impl<'a> Collector<'a> {
    fn new(now: SystemTime, placeholders: &'a Placeholders) -> Self {
        Collector { now, placeholders, accounts: Vec::new(), categories: Vec::new(), pinned_ids: Vec::new(), skipped: 0 }
    }

    pub fn skip(&mut self) {
        self.skipped += 1;
    }

    /// Adds an entry given as separate fields; unsupported ones are skipped.
    pub fn add_fields(&mut self, fields: &Fields<'_>, extras: Extras) -> Result<(), ImportError> {
        let kind = fields.kind.trim();
        if !kind.is_empty() && !kind.eq_ignore_ascii_case("totp") {
            self.skip();
            return Ok(());
        }
        let algorithm = match fields.algorithm.trim().to_uppercase().replace('-', "").as_str() {
            "" | "SHA1" => TotpAlgorithm::Sha1,
            "SHA256" => TotpAlgorithm::Sha256,
            "SHA512" => TotpAlgorithm::Sha512,
            _ => {
                self.skip();
                return Ok(());
            }
        };
        let digits = fields.digits.unwrap_or(6);
        let period = fields.period.unwrap_or(30);
        let Some(secret) = standardized_secret(fields.secret).map(Zeroizing::new) else {
            self.skip();
            return Ok(());
        };
        if (digits != 6 && digits != 8) || period <= 0 || period > i64::from(u32::MAX) {
            self.skip();
            return Ok(());
        }
        let account = Account {
            id: vault::random_uuid().map_err(ImportError::Vault)?,
            issuer: normalized_issuer(fields.issuer, self.placeholders),
            username: normalized_username(fields.username, self.placeholders),
            secret: secret.to_string(),
            algorithm,
            digits: digits as u32,
            period: period as u32,
            created_at: self.now,
            added_by: provenance::IMPORTED.to_owned(),
            website: None,
            icon: None,
            deleted_at: None,
        };
        self.push(account, extras)
    }

    /// Adds an entry stored as an `otpauth://` link, or as a bare Base32 secret (password managers store either). `name`
    /// and `username` fill in an issuer or account the link leaves out. Other links (`steam://`, HOTP, …) are skipped.
    pub fn add_link(&mut self, link: &str, name: &str, username: &str, extras: Extras) -> Result<(), ImportError> {
        let link = link.trim();
        if link.is_empty() {
            return Ok(());
        }
        let lower = link.to_ascii_lowercase();
        if !lower.contains("://") {
            return self.add_fields(&Fields { issuer: name, username, secret: link, ..Fields::default() }, extras);
        }
        if !lower.starts_with("otpauth://") {
            self.skip();
            return Ok(());
        }
        let mut parsed = otpauth::parse(link, self.placeholders);
        if !parsed.is_valid() {
            zeroize::Zeroize::zeroize(&mut parsed.secret);
            self.skip();
            return Ok(());
        }
        if parsed.issuer == self.placeholders.unknown_issuer && !clean_text(name).is_empty() {
            parsed.issuer = normalized_issuer(name, self.placeholders);
        }
        if parsed.account == self.placeholders.empty_username && !clean_text(username).is_empty() {
            parsed.account = normalized_username(username, self.placeholders);
        }
        self.add_parsed(parsed, extras)
    }

    /// Adds a valid parsed link.
    pub fn add_parsed(&mut self, mut parsed: otpauth::ParsedOtpAuth, extras: Extras) -> Result<(), ImportError> {
        let account = Account {
            id: vault::random_uuid().map_err(ImportError::Vault)?,
            issuer: std::mem::take(&mut parsed.issuer),
            username: std::mem::take(&mut parsed.account),
            secret: std::mem::take(&mut parsed.secret),
            algorithm: parsed.algorithm,
            digits: parsed.digits,
            period: parsed.period,
            created_at: self.now,
            added_by: provenance::IMPORTED.to_owned(),
            website: None,
            icon: None,
            deleted_at: None,
        };
        self.push(account, extras)
    }

    fn push(&mut self, mut account: Account, extras: Extras) -> Result<(), ImportError> {
        account.website = extras.website.as_deref().and_then(site_icon::normalized_website);
        for group in &extras.groups {
            let name: String = clean_text(group).chars().take(MAX_NAME_CHARS).collect();
            if !name.trim().is_empty() {
                self.categories =
                    category::adding_to_named(&self.categories, &name, &[account.id], vault::random_uuid).map_err(ImportError::Vault)?;
            }
        }
        if extras.pinned {
            self.pinned_ids.push(account.id);
        }
        self.accounts.push(account);
        Ok(())
    }

    fn finish(mut self) -> Result<Imported, ImportError> {
        if self.accounts.is_empty() {
            return Err(ImportError::NoSupportedAccounts);
        }
        Ok(Imported {
            contents: VaultContents {
                accounts: std::mem::take(&mut self.accounts),
                categories: std::mem::take(&mut self.categories),
                pinned_ids: std::mem::take(&mut self.pinned_ids),
            },
            skipped: self.skipped,
        })
    }
}

impl Drop for Collector<'_> {
    fn drop(&mut self) {
        for account in &mut self.accounts {
            zeroize::Zeroize::zeroize(&mut account.secret);
        }
    }
}

/// Lenient JSON value readers shared by the formats: numbers and strings convert into each other.
pub(crate) fn str_field<'v>(object: &'v Map<String, Value>, key: &str) -> &'v str {
    object.get(key).and_then(Value::as_str).unwrap_or("")
}

pub(crate) fn int_field(object: &Map<String, Value>, key: &str) -> Option<i64> {
    match object.get(key)? {
        Value::Number(number) => number.as_i64().or_else(|| number.as_f64().map(|float| float.trunc() as i64)),
        Value::String(string) => string.trim().parse().ok(),
        _ => None,
    }
}

pub(crate) fn bool_field(object: &Map<String, Value>, key: &str) -> bool {
    match object.get(key) {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|float| float != 0.0),
        Some(Value::String(string)) => matches!(string.trim().to_lowercase().as_str(), "true" | "1" | "yes"),
        _ => false,
    }
}

pub(crate) fn base64_field(object: &Map<String, Value>, key: &str) -> Result<Vec<u8>, ImportError> {
    use base64::Engine;
    let value = object.get(key).and_then(Value::as_str).ok_or(ImportError::InvalidFile)?;
    base64::engine::general_purpose::STANDARD.decode(value.trim()).map_err(|_| ImportError::InvalidFile)
}

/// Parses decrypted JSON, wiping the plaintext buffer afterwards.
pub(crate) fn parse_plaintext(plaintext: Zeroizing<Vec<u8>>) -> Result<Value, ImportError> {
    serde_json::from_slice(&plaintext).map_err(|_| ImportError::InvalidFile)
}

#[cfg(test)]
pub(crate) mod tests;
