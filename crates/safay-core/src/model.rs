use std::time::SystemTime;

use uuid::Uuid;

/// TOTP digest algorithm. The persisted values (`SHA1` / `SHA256` / `SHA512`) match Mac and must not be renamed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TotpAlgorithm {
    Sha1,
    Sha256,
    Sha512,
}

impl TotpAlgorithm {
    pub const ALL: [TotpAlgorithm; 3] = [TotpAlgorithm::Sha1, TotpAlgorithm::Sha256, TotpAlgorithm::Sha512];

    pub fn raw_value(self) -> &'static str {
        match self {
            TotpAlgorithm::Sha1 => "SHA1",
            TotpAlgorithm::Sha256 => "SHA256",
            TotpAlgorithm::Sha512 => "SHA512",
        }
    }

    /// Parses the persisted value strictly (case-sensitive).
    pub fn from_raw_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|algorithm| algorithm.raw_value() == value)
    }
}

/// How an account's code is made. The persisted values (`totp` / `steam` / `battle`) must not be renamed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum OtpType {
    /// RFC 6238 TOTP with the account's own algorithm, digits, and period.
    #[default]
    Totp,
    /// Steam Guard: TOTP written as five characters from Steam's alphabet ([`crate::totp`]).
    Steam,
    /// Battle.net: TOTP with eight digits.
    BattleNet,
}

impl OtpType {
    pub const ALL: [OtpType; 3] = [OtpType::Totp, OtpType::Steam, OtpType::BattleNet];

    pub fn raw_value(self) -> &'static str {
        match self {
            OtpType::Totp => "totp",
            OtpType::Steam => "steam",
            OtpType::BattleNet => "battle",
        }
    }

    /// Parses the persisted value strictly (case-sensitive).
    pub fn from_raw_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|otp_type| otp_type.raw_value() == value)
    }

    /// The algorithm, digits, and period every account of this type uses, or `None` for TOTP, whose parameters are
    /// its own.
    pub fn fixed_parameters(self) -> Option<(TotpAlgorithm, u32, u32)> {
        match self {
            OtpType::Totp => None,
            OtpType::Steam => Some((TotpAlgorithm::Sha1, 5, 30)),
            OtpType::BattleNet => Some((TotpAlgorithm::Sha1, 8, 30)),
        }
    }
}

/// Persisted values for an account's provenance.
pub mod provenance {
    pub const MANUAL: &str = "manual";
    pub const IMPORTED: &str = "import";
}

/// `Debug` does not print the secret (see [`redacted_secret`]).
#[derive(Clone, PartialEq, Eq)]
pub struct Account {
    pub id: Uuid,
    pub issuer: String,
    pub username: String,
    pub secret: String,
    pub otp_type: OtpType,
    pub algorithm: TotpAlgorithm,
    pub digits: u32,
    pub period: u32,
    pub created_at: SystemTime,
    pub added_by: String,
    /// Normalized host name ([`crate::site_icon::normalized_website`]); `None` when unknown.
    pub website: Option<String>,
    /// PNG site icon ([`crate::site_icon::is_valid_icon`]); only present together with `website`.
    pub icon: Option<Vec<u8>>,
    /// When the user deleted it: it waits in Recently Deleted until [`crate::trash::RETENTION`] has passed
    /// ([`crate::trash`]); `None` for an account in use.
    pub deleted_at: Option<SystemTime>,
}

impl Account {
    /// Deleted and waiting in Recently Deleted.
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// Stands in for the secret in hand-written `Debug` impls of types that hold a TOTP secret, so `{:?}` never leaks it
/// into logs or crash reports.
pub fn redacted_secret(secret: &str) -> &'static str {
    if secret.is_empty() { "" } else { "<redacted>" }
}

impl std::fmt::Debug for Account {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Account")
            .field("id", &self.id)
            .field("issuer", &self.issuer)
            .field("username", &self.username)
            .field("secret", &redacted_secret(&self.secret))
            .field("otp_type", &self.otp_type)
            .field("algorithm", &self.algorithm)
            .field("digits", &self.digits)
            .field("period", &self.period)
            .field("created_at", &self.created_at)
            .field("added_by", &self.added_by)
            .field("website", &self.website)
            .field("icon", &self.icon.as_ref().map(Vec::len))
            .field("deleted_at", &self.deleted_at)
            .finish()
    }
}

/// Placeholder text written when the issuer or account name is empty.
///
/// Mac stores the localized placeholder directly in the account field, and it takes part in the dedup key, so the
/// platform passes it in for the current UI language; the core has no built-in text in any language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placeholders {
    pub unknown_issuer: String,
    pub empty_username: String,
}
