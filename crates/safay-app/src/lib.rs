//! Desktop application services. No NSH, keychain or implicit production IO; backup IO goes only through `sync::Backend`.
pub mod session;
pub mod storage;
pub use cloudset;
pub use safay_core as core;
pub use uuid::Uuid;
pub use zeroize::Zeroizing;
pub mod clipboard;
pub mod cloud;
pub mod controller;
pub mod icloud;
pub mod launcher;
pub mod preferences;
pub mod sync;
