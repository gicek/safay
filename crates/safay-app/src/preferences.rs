//! Only non-sensitive, per-device preferences belong outside the encrypted vault.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    #[default]
    System,
    En,
    Hans,
    Hant,
}
/// When the Dock (macOS) shows the app; other desktops always show a taskbar entry while a window is open.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DockIcon {
    Always,
    #[default]
    MainWindow,
    Never,
}
/// A global shortcut: a platform virtual key code and modifier bits (macOS: Carbon `cmdKey` 0x100, `shiftKey` 0x200,
/// `optionKey` 0x800, `controlKey` 0x1000).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkey {
    pub key: u16,
    pub modifiers: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub language: Language,
    pub appearance: Appearance,
    pub cards: bool,
    pub auto_lock_minutes: u64,
    /// The core's wait curve after repeated wrong passwords. Turning it off needs the master password.
    pub failure_delay: bool,
    /// Menu bar (macOS) or notification-area (Windows/Linux) icon.
    pub tray_icon: bool,
    pub dock_icon: DockIcon,
    /// Start without opening the main window; only the tray entry appears.
    pub background_launch: bool,
    /// Touch ID unlock where the hardware is present (default on).
    pub touch_id: bool,
    /// Opens the menu bar panel (default ⌥⌘Z).
    pub hotkey_panel: Option<Hotkey>,
    /// Brings up the main window (default ⌥⌘X).
    pub hotkey_main: Option<Hotkey>,
    /// Selects a QR code on screen and imports it (default ⌥⌘A).
    pub hotkey_capture: Option<Hotkey>,
    /// The menu bar panel's account scope: one category, or all accounts. A deleted category reads as all.
    pub menu_bar_category: Option<uuid::Uuid>,
    /// Fetch icons from websites when accounts are added or their website changes (default on).
    pub site_icons: bool,
    /// Daily update checks: `None` until the user answers the question asked on the second launch.
    pub update_checks: Option<bool>,
    /// A newer version downloads in the background and installs when the app quits.
    pub update_downloads: bool,
    /// Launches so far (the update question waits for the second).
    pub launches: u32,
    /// When updates were last checked (Unix ms).
    pub last_update_check_ms: i64,
    /// The third-party plugin interface (Alfred, Raycast, Tinycast through AppleScript; macOS). Off by default.
    pub launcher: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: Language::System,
            appearance: Appearance::System,
            cards: true,
            auto_lock_minutes: 5,
            failure_delay: true,
            tray_icon: true,
            dock_icon: DockIcon::MainWindow,
            background_launch: false,
            touch_id: true,
            hotkey_panel: Some(Hotkey { key: 6, modifiers: 0x100 | 0x800 }),
            hotkey_main: Some(Hotkey { key: 7, modifiers: 0x100 | 0x800 }),
            hotkey_capture: Some(Hotkey { key: 0, modifiers: 0x100 | 0x800 }),
            menu_bar_category: None,
            site_icons: true,
            update_checks: None,
            update_downloads: false,
            launches: 0,
            last_update_check_ms: 0,
            launcher: false,
        }
    }
}
impl Preferences {
    /// Out-of-range values fall back to defaults; the tray and the Dock icon are never both hidden.
    pub fn validated(self) -> Self {
        let mut next = if [1, 5, 30].contains(&self.auto_lock_minutes) { self } else { Self { auto_lock_minutes: 5, ..self } };
        if !next.tray_icon && next.dock_icon == DockIcon::Never {
            next.dock_icon = DockIcon::MainWindow;
        }
        if !next.tray_icon {
            next.background_launch = false;
        }
        // Downloading on its own only follows automatic checks.
        if next.update_checks != Some(true) {
            next.update_downloads = false;
        }
        // A shortcut needs at least one of Command, Option or Control, and the two cannot be the same.
        for h in [&mut next.hotkey_panel, &mut next.hotkey_main, &mut next.hotkey_capture] {
            if h.is_some_and(|k| k.modifiers & (0x100 | 0x800 | 0x1000) == 0 || k.key > 0x7f) {
                *h = None;
            }
        }
        if next.hotkey_panel.is_some() && next.hotkey_panel == next.hotkey_main {
            next.hotkey_main = None;
        }
        if next.hotkey_capture.is_some() && (next.hotkey_capture == next.hotkey_panel || next.hotkey_capture == next.hotkey_main) {
            next.hotkey_capture = None;
        }
        next
    }
}

#[cfg(test)]
mod capture_and_scope_tests {
    use super::*;
    #[test]
    fn older_files_read_with_new_defaults() {
        let p: Preferences = serde_json::from_str(r#"{"cards":false}"#).unwrap();
        assert!(!p.cards);
        assert_eq!(p.hotkey_capture, Some(Hotkey { key: 0, modifiers: 0x100 | 0x800 }));
        assert_eq!(p.menu_bar_category, None);
    }
    #[test]
    fn capture_shortcut_never_duplicates_another() {
        let mut p = Preferences::default();
        p.hotkey_capture = p.hotkey_panel;
        assert_eq!(p.validated().hotkey_capture, None);
        let p = Preferences { hotkey_capture: Some(Hotkey { key: 0, modifiers: 0x200 }), ..Default::default() };
        assert_eq!(p.validated().hotkey_capture, None, "Shift alone is not enough");
    }
    #[test]
    fn menu_bar_scope_round_trips() {
        let id = uuid::Uuid::from_u128(7);
        let p = Preferences { menu_bar_category: Some(id), ..Default::default() };
        let back: Preferences = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.menu_bar_category, Some(id));
    }
}

#[cfg(test)]
mod update_tests {
    use super::*;
    #[test]
    fn update_settings_default_to_asking_and_need_checks_for_downloads() {
        let p: Preferences = serde_json::from_str(r#"{"cards":true}"#).unwrap();
        assert_eq!((p.update_checks, p.update_downloads, p.launches), (None, false, 0));
        let p = Preferences { update_checks: Some(false), update_downloads: true, ..Default::default() }.validated();
        assert!(!p.update_downloads);
        let p = Preferences { update_checks: Some(true), update_downloads: true, ..Default::default() }.validated();
        assert!(p.update_downloads);
    }
}
