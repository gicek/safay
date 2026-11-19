//! OS services. Tests supply in-memory services and never call these paths.
#[cfg(target_os = "macos")]
pub mod automation;
pub mod clipboard;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod credentials;
pub mod files;
pub mod http;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod site_icons;
pub mod tls;
pub mod updates;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod biometric;
#[cfg(target_os = "macos")]
pub mod camera;
// The same interface on Linux, through the desktop portal and GStreamer.
#[cfg(target_os = "linux")]
#[path = "camera_linux.rs"]
pub mod camera;
#[cfg(target_os = "macos")]
pub mod hotkeys;
#[cfg(target_os = "macos")]
pub mod icloud;
#[cfg(target_os = "macos")]
pub mod screen_capture;
/// Linux's "Select on Screen": the desktop portal's screenshot (`screen_capture` is macOS's own selection).
#[cfg(target_os = "linux")]
pub mod screenshot;
#[cfg(target_os = "macos")]
pub mod symbols;
#[cfg(target_os = "macos")]
pub mod web_auth;
/// How a web sign-in (OAuth backup services) ended.
pub enum Signed {
    /// The provider redirected to the callback URL.
    Callback(String),
    /// The user closed the page.
    Closed,
    /// The page could not be shown, or the system reported an error.
    Failed,
}
/// Opens the provider's sign-in page; the receiver gets how it ended. Without `supports::OAUTH` it fails at once.
pub fn web_sign_in(url: &str, callback_scheme: &str) -> std::sync::mpsc::Receiver<Signed> {
    #[cfg(target_os = "macos")]
    return web_auth::start(url, callback_scheme);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (url, callback_scheme);
        let (sender, receiver) = std::sync::mpsc::channel();
        let _ = sender.send(Signed::Failed);
        receiver
    }
}

use std::sync::Mutex;

/// What this desktop's adapter implements. The UI shows a feature only when it is true here: no switch for a capability
/// that does nothing. Gaps and plans for each OS are in docs/PLATFORMS.md.
pub mod supports {
    /// Menu bar / tray icon with the quick panel.
    pub const TRAY: bool = cfg!(target_os = "macos");
    /// System-wide shortcuts and their recorder.
    pub const GLOBAL_HOTKEYS: bool = cfg!(target_os = "macos");
    /// Launch at login and background-only launch.
    pub const LOGIN_ITEM: bool = cfg!(target_os = "macos");
    /// Hardware-protected key release (Touch ID; Windows Hello later).
    pub const BIOMETRIC: bool = cfg!(target_os = "macos");
    /// Settings in a window of its own (elsewhere a dialog over the main window).
    pub const SETTINGS_WINDOW: bool = cfg!(target_os = "macos");
    /// Selecting a QR code on screen (macOS: Safay's selection over a picture of each display, Screen Recording
    /// permission; Linux: the desktop portal's interactive screenshot).
    pub const SCREEN_CAPTURE: bool = cfg!(any(target_os = "macos", target_os = "linux"));
    /// Scanning with the camera (macOS: AVFoundation, needs the packaged app's camera usage description; Linux: the
    /// desktop portal's Camera and PipeWire).
    pub const CAMERA: bool = cfg!(any(target_os = "macos", target_os = "linux"));
    /// Cloud storage backup: needs a system credential store (Linux's Secret Service is not wired yet).
    pub const CLOUD: bool = cfg!(any(target_os = "macos", target_os = "windows"));
    /// OAuth sign-in for Dropbox, OneDrive and Google Drive (macOS: `ASWebAuthenticationSession`; Windows not yet).
    pub const OAUTH: bool = cfg!(target_os = "macos");
    /// Updating the app itself (NSH's `nsh-updater`; active only in packages that carry an update source).
    pub const UPDATES: bool = cfg!(target_os = "macos");
    /// Launchers (Alfred, Raycast, Tinycast) through AppleScript: the third-party plugin interface.
    pub const AUTOMATION: bool = cfg!(target_os = "macos");
}
/// Whether this build updates itself (fixed per build: the package carries an update source).
pub fn updates_configured() -> bool {
    updates::updater().is_some()
}
/// The updater's phase; `None` in builds that don't update themselves.
pub fn update_phase() -> Option<updates::Phase> {
    #[cfg(debug_assertions)]
    if let Some(phase) = updates::preview_phase() {
        return Some(phase);
    }
    updates::updater().map(updates::Updater::phase)
}
static REQUEST: Mutex<Option<crate::store::Sheet>> = Mutex::new(None);
/// A capture asked for: 0 none, 1 from the menu bar panel, 2 from the capture shortcut.
static CAPTURE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
/// The menu bar panel asks the main window to select a QR code on screen and import it (only while unlocked).
pub fn request_capture() {
    CAPTURE.store(1, std::sync::atomic::Ordering::SeqCst);
}
/// The capture shortcut asks the same; while locked, the code is selected first and the vault unlocked after.
pub fn request_capture_from_shortcut() {
    CAPTURE.store(2, std::sync::atomic::Ordering::SeqCst);
}
/// The menu bar icon's menu asks the main window to check for updates.
static UPDATE_CHECK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn request_update_check() {
    UPDATE_CHECK.store(true, std::sync::atomic::Ordering::SeqCst);
}
pub fn take_update_check_request() -> bool {
    UPDATE_CHECK.swap(false, std::sync::atomic::Ordering::SeqCst)
}
/// The capture asked for, if any: `Some(true)` from the shortcut.
pub fn take_capture_request() -> Option<bool> {
    match CAPTURE.swap(0, std::sync::atomic::Ordering::SeqCst) {
        0 => None,
        n => Some(n == 2),
    }
}
/// A sheet another window asked the main window to show. Only a sheet kind crosses stores.
pub fn request_sheet(sheet: crate::store::Sheet) {
    if let Ok(mut r) = REQUEST.lock() {
        *r = Some(sheet);
    }
}
pub fn take_request() -> Option<crate::store::Sheet> {
    REQUEST.lock().ok().and_then(|mut r| r.take())
}
static LAUNCHER_COPY: Mutex<Option<safay_app::Uuid>> = Mutex::new(None);
/// A launcher's copy that Touch ID could not prove: the main window asks for the master password (or, while locked,
/// copies once the user unlocks).
pub fn request_launcher_copy(id: safay_app::Uuid) {
    if let Ok(mut r) = LAUNCHER_COPY.lock() {
        *r = Some(id);
    }
}
pub fn take_launcher_copy() -> Option<safay_app::Uuid> {
    LAUNCHER_COPY.lock().ok().and_then(|mut r| r.take())
}
/// Brings the main window forward.
pub fn open_main() {
    #[cfg(target_os = "macos")]
    crate::desktop::open_main();
}
/// Settings: a window of its own where `supports::SETTINGS_WINDOW`, else a dialog over the main window.
pub fn open_settings_window() {
    #[cfg(target_os = "macos")]
    crate::desktop::open_settings();
    #[cfg(not(target_os = "macos"))]
    request_sheet(crate::store::Sheet::Settings);
}
/// Asks the main window to present a sheet (Change Password, Restore) and brings it forward.
pub fn open_in_main(sheet: crate::store::Sheet) {
    request_sheet(sheet);
    open_main();
}
/// What the app's own launcher lets NSH do: what an app without a declaration may, and the desktop notice's overlay
/// window (click-through, above other apps), which NSH grants only when declared.
pub fn capabilities() -> nsh_core::access::Capabilities {
    let mut capabilities = nsh_core::access::Capabilities::all();
    let mut windows = nsh_core::access::WindowsGrant::default();
    windows.overlay = true;
    capabilities.windows = Some(windows);
    capabilities
}
/// Quits as the user's quit does (NSH's runner returns to `main`, which locks and installs a downloaded update).
pub fn quit() {
    nsh_core::element::desktop::quit();
}
/// The app's bundle ID: debug builds are a separate app (`.debug`), with their own data directory, keychain items and
/// signing, so testing never touches a release build's vault.
pub const APP_ID: &str = if cfg!(debug_assertions) { "com.gicek.safay.debug" } else { "com.gicek.safay" };
/// The URL scheme the app claims (`safay://`); a link of it brings the main window forward.
pub const URL_SCHEME: &str = "safay";
/// The app's private data directory (named by [`APP_ID`]): separate from the other build's.
pub fn data_directory() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    #[cfg(target_os = "macos")]
    return std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support").join(APP_ID));
    #[cfg(target_os = "windows")]
    return std::env::var_os("LOCALAPPDATA").map(|root| PathBuf::from(root).join(APP_ID));
    #[cfg(target_os = "linux")]
    return std::env::var_os("XDG_DATA_HOME")
        .filter(|p| PathBuf::from(p).is_absolute())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|root| root.join(APP_ID));
}
/// The data directory as shown in Settings, with the home directory abbreviated.
pub fn data_location() -> Option<String> {
    let dir = data_directory()?;
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(std::path::PathBuf::from);
    Some(match home.and_then(|h| dir.strip_prefix(h).ok().map(|p| p.to_path_buf())) {
        Some(rest) => format!("~/{}", rest.display()),
        None => dir.display().to_string(),
    })
}

/// The Alfred workflow the package ships, signed with the app (`scripts/integrations.py --alfred`); `None` outside a
/// package. Installing only opens this file, which Alfred imports.
pub fn alfred_workflow() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.parent()?.parent()?.join("Resources/Safay.alfredworkflow");
    path.is_file().then_some(path)
}

/// NSH expects asset:/@package/path under an explicitly supplied asset directory.
/// Installed hosts resolve beside their executable; cargo runs use build-staged public assets.
pub fn assets_directory() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        #[cfg(target_os = "macos")]
        let packaged = parent.join("../Resources/assets");
        #[cfg(not(target_os = "macos"))]
        let packaged = parent.join("assets");
        if packaged.join("@safay/icon.png").is_file() {
            return packaged;
        }
    }
    std::path::PathBuf::from(env!("OUT_DIR")).join("assets")
}

/// This computer's name for backup metadata and history ("Safay" when unknown). Never sent anywhere else.
pub fn device_name() -> String {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        #[cfg(target_os = "macos")]
        let name = std::process::Command::new("/usr/sbin/scutil")
            .args(["--get", "ComputerName"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok());
        #[cfg(target_os = "windows")]
        let name = std::env::var("COMPUTERNAME").ok();
        #[cfg(target_os = "linux")]
        let name = std::fs::read_to_string("/etc/hostname").ok();
        name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty() && n.len() <= 120).unwrap_or_else(|| "Safay".to_owned())
    })
    .clone()
}

/// Installs the cloud backup services: the real executor, and the system credential store (memory in `--preview`, so a
/// preview never touches the keychain). Linux has no credential store wired yet; cloud locations stay hidden there.
pub fn install_network(preview: bool) {
    use safay_app::cloud::{Credentials, Services};
    let credentials: std::sync::Arc<dyn Credentials> = if preview || !supports::CLOUD {
        std::sync::Arc::new(MemoryCredentials::default())
    } else {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            std::sync::Arc::new(credentials::Keychain)
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            std::sync::Arc::new(MemoryCredentials::default())
        }
    };
    safay_app::cloud::install(Services {
        http: std::sync::Arc::new(http::Executor::storage()),
        credentials,
        user_agent: format!("Safay/{}", env!("CARGO_PKG_VERSION")),
    });
    // iCloud: the system's CloudKit only in a bundle signed with the iCloud entitlement; a preview gets an in-memory
    // account so it never reaches iCloud. Elsewhere no store is installed and the location stays unavailable.
    if preview {
        safay_app::icloud::install(std::sync::Arc::new(safay_app::icloud::MemoryStore::new("_preview")));
    } else {
        #[cfg(target_os = "macos")]
        if let Some(store) = icloud::CloudKitStore::new() {
            safay_app::icloud::install(std::sync::Arc::new(store));
        }
    }
}
/// Sign-ins held only for this process (previews and platforms without a credential store).
#[derive(Default)]
pub struct MemoryCredentials(Mutex<std::collections::HashMap<String, safay_app::Zeroizing<Vec<u8>>>>);
impl safay_app::cloud::Credentials for MemoryCredentials {
    fn load(&self, id: &str) -> Option<safay_app::Zeroizing<Vec<u8>>> {
        self.0.lock().ok()?.get(id).cloned()
    }
    fn save(&self, id: &str, blob: &[u8]) -> Result<(), safay_app::sync::SyncError> {
        self.0.lock().map_err(|_| safay_app::sync::SyncError::Storage)?.insert(id.into(), safay_app::Zeroizing::new(blob.to_vec()));
        Ok(())
    }
    fn delete(&self, id: &str) {
        if let Ok(mut map) = self.0.lock() {
            map.remove(id);
        }
    }
}
