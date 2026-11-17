//! Safay platform-independent core.
//!
//! Holds only pure logic that never touches the system: account model and deduplication, Base32 / TOTP, otpauth and
//! Google migration code parsing, user categories, vault envelope encryption, export and import, Authenticator import,
//! search, password strength, sync conflict detection, and account websites and site icons. File IO, Keychain / Keystore, biometrics, clipboard, and UI
//! state stay on each platform.
//!
//! Behavior is pinned case by case by the vectors in `golden/`.
//! Before changing any observable behavior, update the golden files in the same commit and explain why in its message (golden/README.md).

pub mod authenticator;
pub mod backup;
pub mod base32;
pub mod canonical;
pub mod category;
pub mod dedup;
mod json;
pub mod merge;
pub mod model;
pub mod otpauth;
pub mod password;
pub mod search;
pub mod site_icon;
pub mod sync;
pub mod totp;
pub mod trash;
pub mod vault;

pub use model::{Account, Placeholders, TotpAlgorithm};
