//! macOS "Select on Screen": the system's own interactive screenshot (`/usr/sbin/screencapture -i -x`)
//! into a private temporary directory (0700), read once with the usual size limit and deleted at once. Nothing runs in the
//! background. Screen Recording permission (TCC) is checked first; NSH has no capture API.
use safay_app::{session::Error, storage::bounded_read};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// Whether this app may read the screen.
pub fn has_permission() -> bool {
    // SAFETY: a plain query without arguments.
    unsafe { CGPreflightScreenCaptureAccess() }
}
/// Asks once with the system prompt; when it was already declined, opens the Screen Recording pane instead.
pub fn request_permission() {
    // SAFETY: a plain request without arguments; the system shows its prompt at most once.
    if !unsafe { CGRequestScreenCaptureAccess() } {
        open_settings();
    }
}
pub fn open_settings() {
    drop(nsh_core::prelude::open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"));
}

/// The selected area as PNG bytes; `None` when the user pressed Esc.
pub async fn capture() -> Result<Option<Vec<u8>>, Error> {
    nsh_core::prelude::blocking(|| {
        let dir = tempfile::Builder::new().prefix("safay-capture-").tempdir().map_err(|_| Error::Storage)?;
        let path = dir.path().join("capture.png");
        let status =
            std::process::Command::new("/usr/sbin/screencapture").arg("-i").arg("-x").arg(&path).status().map_err(|_| Error::Storage)?;
        if !status.success() || !path.exists() {
            return Ok(None);
        }
        let bytes = bounded_read(&path)?;
        drop(std::fs::remove_file(&path));
        Ok(bytes)
    })
    .await
    .map_err(|_| Error::Storage)?
}
