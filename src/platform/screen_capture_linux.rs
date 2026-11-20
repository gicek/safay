//! Linux "Select on Screen": the desktop portal's Screenshot, interactive, so the desktop's own screenshot tool lets the
//! user pick the area (`slurp` on Hyprland, GNOME's and KDE's tools) or takes the screen. Wayland gives an app no way to
//! cover the screen with its own selection as macOS's `crate::scan` does. The picture the portal saves is read once and
//! deleted at once; only the text of the QR codes found in it is kept, and the caller keeps the 2FA ones.
use safay_app::Zeroizing;
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// How a capture ended.
pub enum Capture {
    /// The user closed the screenshot tool or declined.
    Cancelled,
    /// The QR codes' texts, one per line.
    Codes(Zeroizing<String>),
    /// The picture holds no QR code.
    NoCode,
    /// No screenshot portal, or the picture could not be read.
    Failed,
}

/// One screenshot at a time: a second request while the desktop's tool is up (the shortcut pressed twice) is dropped.
static TAKING: AtomicBool = AtomicBool::new(false);

/// Takes the screenshot and finds its QR codes. Blocks until the user is done: call off the main thread.
pub fn capture() -> Capture {
    if TAKING.swap(true, Ordering::SeqCst) {
        return Capture::Cancelled;
    }
    let capture = take();
    TAKING.store(false, Ordering::SeqCst);
    capture
}

fn take() -> Capture {
    use ashpd::desktop::screenshot::Screenshot;
    let response = futures_executor::block_on(async {
        let request = Screenshot::request().interactive(true).modal(true).send().await?;
        request.response()
    });
    let uri = match response {
        Ok(shot) => shot.uri().as_str().to_owned(),
        Err(ashpd::Error::Response(_)) => return Capture::Cancelled,
        Err(_) => return Capture::Failed,
    };
    let Some(path) = file_path(&uri) else { return Capture::Failed };
    let gray = read_gray(&path);
    // The picture may show secrets: it is not left behind, read or not.
    drop(std::fs::remove_file(&path));
    let Some(gray) = gray else { return Capture::Failed };
    let mut prepared = rqrr::PreparedImage::prepare(gray);
    let texts: Vec<Zeroizing<String>> =
        prepared.detect_grids().iter().filter_map(|grid| grid.decode().ok().map(|(_, text)| Zeroizing::new(text))).collect();
    if texts.is_empty() {
        return Capture::NoCode;
    }
    Capture::Codes(Zeroizing::new(texts.iter().map(|t| t.as_str()).collect::<Vec<_>>().join("\n")))
}

/// A `file://` URI's local path, percent-decoded; `None` for any other URI.
fn file_path(uri: &str) -> Option<PathBuf> {
    let encoded = uri.strip_prefix("file://")?;
    // `file://host/path` names another machine; only the local forms are accepted.
    let encoded = encoded.strip_prefix("localhost").unwrap_or(encoded);
    if !encoded.starts_with('/') {
        return None;
    }
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    use std::os::unix::ffi::OsStringExt;
    Some(PathBuf::from(std::ffi::OsString::from_vec(out)))
}

/// The screenshot as gray: a regular file of at most 64 MB and 16384 pixels a side (a whole multi-display desktop), as
/// macOS's display pictures (`screen_capture::read_shot`).
fn read_gray(path: &Path) -> Option<image::GrayImage> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > 64 * 1024 * 1024 {
        return None;
    }
    let bytes = Zeroizing::new(std::fs::read(path).ok()?);
    let mut reader = image::ImageReader::new(Cursor::new(&bytes[..])).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(1024 * 1024 * 1024);
    reader.limits(limits);
    Some(reader.decode().ok()?.to_luma8())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_local_file_uris_become_paths() {
        assert_eq!(file_path("file:///tmp/Screenshot%20from%202026.png"), Some(PathBuf::from("/tmp/Screenshot from 2026.png")));
        assert_eq!(file_path("file://localhost/tmp/a.png"), Some(PathBuf::from("/tmp/a.png")));
        assert_eq!(file_path("file://server/share/a.png"), None);
        assert_eq!(file_path("https://example.invalid/a.png"), None);
        assert_eq!(file_path("file:///tmp/%zz.png"), None);
    }
    #[test]
    fn a_screenshot_is_read_then_its_codes_found() {
        let qr = qrcode::QrCode::new(b"otpauth://totp/Example:alice?secret=JBSWY3DPEHPK3PXP&issuer=Example").unwrap();
        let picture = qr.render::<image::Luma<u8>>().quiet_zone(true).min_dimensions(240, 240).build();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shot.png");
        picture.save(&path).unwrap();
        let gray = read_gray(&path).unwrap();
        let mut prepared = rqrr::PreparedImage::prepare(gray);
        let (_, text) = prepared.detect_grids()[0].decode().unwrap();
        assert!(text.starts_with("otpauth://totp/Example:alice"));
    }
}
