pub mod accounts;
pub mod backup;
pub mod graphic;
pub mod quick;
pub mod settings;
pub mod sheets;
pub mod trash;
pub mod unlock;
// The desktops' update window.
#[cfg_attr(any(target_os = "android", target_os = "ios"), allow(dead_code))]
pub mod update;
