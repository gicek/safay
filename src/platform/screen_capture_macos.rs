//! macOS "Select on Screen": one still picture of each display (`/usr/sbin/screencapture -x -R`) into a private
//! temporary directory (0700), read once and deleted at once, then kept in memory as grayscale only while the user picks
//! a code ([`crate::scan`]). 2FA QR codes are found in those pictures; nothing else is read from them, and nothing runs
//! in the background. Screen Recording permission (TCC) is checked first; NSH has no capture API.
use safay_app::{session::Error, storage::bounded_read};
use std::{io::Cursor, path::Path};

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

/// A rectangle in points: left, top, width, height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
    pub fn inset(&self, by: f32) -> Rect {
        Rect { x: self.x + by, y: self.y + by, w: (self.w - 2.0 * by).max(0.0), h: (self.h - 2.0 * by).max(0.0) }
    }
}

/// One display's picture: its pixels in gray and how many of them a point is.
pub struct Shot {
    gray: image::GrayImage,
    scale: f32,
}

/// A 2FA QR code on a display: where it is (points from the display's top left) and what it adds. Names only, never
/// secrets.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub rect: Rect,
    /// The first account's issuer (may be empty).
    pub issuer: String,
    /// How many accounts it holds (a migration code holds several).
    pub accounts: usize,
    /// The vault already has every one of them.
    pub existing: bool,
}

/// The pictures of the displays at `bounds` (screen points, top left origin, as NSH's displays): `None` for one that
/// could not be read.
pub fn snapshot(bounds: &[Rect]) -> Result<Vec<Option<Shot>>, Error> {
    let dir = tempfile::Builder::new().prefix("safay-capture-").tempdir().map_err(|_| Error::Storage)?;
    let mut shots = Vec::with_capacity(bounds.len());
    for (i, b) in bounds.iter().enumerate() {
        let path = dir.path().join(format!("display-{i}.png"));
        let region = format!("-R{},{},{},{}", b.x.round(), b.y.round(), b.w.round(), b.h.round());
        let status = std::process::Command::new("/usr/sbin/screencapture").arg("-x").arg(region).arg(&path).status();
        let shot = match status {
            Ok(s) if s.success() => read_shot(&path, b.w),
            _ => None,
        };
        drop(std::fs::remove_file(&path));
        shots.push(shot);
    }
    Ok(shots)
}

/// A display's PNG, at most 64 MB and 16384 pixels a side, as gray.
fn read_shot(path: &Path, points_wide: f32) -> Option<Shot> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > 64 * 1024 * 1024 {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(1024 * 1024 * 1024);
    reader.limits(limits);
    let gray = reader.decode().ok()?.to_luma8();
    let scale = if points_wide > 0.0 { gray.width() as f32 / points_wide } else { 1.0 };
    Some(Shot { gray, scale })
}

/// The 2FA QR codes in a display's picture, top to bottom. Codes that hold no valid account are left out. `known` tells,
/// for a code's text, which of its accounts the vault already has (`Session::known_accounts`).
pub fn find(shot: &Shot, known: &dyn Fn(&str) -> Vec<bool>) -> Vec<Found> {
    let mut prepared = rqrr::PreparedImage::prepare(shot.gray.clone());
    let placeholders = safay_app::session::placeholders();
    let mut found: Vec<Found> = prepared
        .detect_grids()
        .into_iter()
        .filter_map(|grid| {
            let corners = grid.bounds;
            let (_, text) = grid.decode().ok()?;
            let text = zeroize::Zeroizing::new(text);
            let valid: Vec<_> = safay_app::core::otpauth::parse_lines(&text, &placeholders).into_iter().filter(|p| p.is_valid()).collect();
            let first = valid.first()?;
            let existing = known(&text);
            let existing = !existing.is_empty() && existing.iter().all(|k| *k);
            let xs = corners.iter().map(|p| p.x as f32);
            let ys = corners.iter().map(|p| p.y as f32);
            let (left, right) = xs.fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
            let (top, bottom) = ys.fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
            let s = shot.scale;
            Some(Found {
                rect: Rect { x: left / s, y: top / s, w: (right - left) / s, h: (bottom - top) / s },
                issuer: first.issuer.clone(),
                accounts: valid.len(),
                existing,
            })
        })
        .collect();
    found.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y).then(a.rect.x.total_cmp(&b.rect.x)));
    found.dedup_by(|a, b| a.rect == b.rect);
    found
}

/// The part of a display's picture under `rect` (points) as a gray PNG, at most 4096 pixels a side, for the QR reader.
pub fn crop(shot: &Shot, rect: Rect) -> Result<Vec<u8>, Error> {
    let (w, h) = shot.gray.dimensions();
    let s = shot.scale;
    let x0 = ((rect.x * s).floor().max(0.0) as u32).min(w);
    let y0 = ((rect.y * s).floor().max(0.0) as u32).min(h);
    let x1 = (((rect.x + rect.w) * s).ceil().max(0.0) as u32).min(w);
    let y1 = (((rect.y + rect.h) * s).ceil().max(0.0) as u32).min(h);
    if x1 <= x0 || y1 <= y0 {
        return Err(Error::Import);
    }
    let mut part = image::imageops::crop_imm(&shot.gray, x0, y0, x1 - x0, y1 - y0).to_image();
    let longest = part.width().max(part.height());
    if longest > 4096 {
        let f = 4096.0 / longest as f32;
        let (nw, nh) = (((part.width() as f32 * f) as u32).max(1), ((part.height() as f32 * f) as u32).max(1));
        part = image::imageops::resize(&part, nw, nh, image::imageops::FilterType::Triangle);
    }
    let mut out = Cursor::new(Vec::new());
    part.write_to(&mut out, image::ImageFormat::Png).map_err(|_| Error::Import)?;
    Ok(out.into_inner())
}

/// The system's own interactive screenshot (`screencapture -i -x`): the selected area as PNG bytes, `None` when the
/// user pressed Esc. Used when the displays cannot be read for Safay's own selection.
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

#[cfg(test)]
mod tests {
    use super::*;
    /// A Retina "screen" (2 pixels a point) with a public sample code and a QR code that holds no account.
    fn screen() -> Shot {
        let code = |uri: &str| {
            let qr = qrcode::QrCode::new(uri.as_bytes()).unwrap();
            qr.render::<image::Luma<u8>>().quiet_zone(true).min_dimensions(240, 240).build()
        };
        let mut gray = image::GrayImage::from_pixel(1600, 1000, image::Luma([255]));
        image::imageops::overlay(
            &mut gray,
            &code("otpauth://totp/Example:alice@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example"),
            900,
            400,
        );
        image::imageops::overlay(&mut gray, &code("https://example.com/"), 100, 100);
        Shot { gray, scale: 2.0 }
    }
    #[test]
    fn finds_only_2fa_codes_in_points() {
        let found = find(&screen(), &|_| vec![false]);
        assert_eq!(found.len(), 1);
        let f = &found[0];
        assert_eq!((f.issuer.as_str(), f.accounts), ("Example", 1));
        // The code's modules start inside its 240 px picture at (900, 400): in points, past (450, 200).
        assert!(f.rect.x > 450.0 && f.rect.x < 480.0 && f.rect.y > 200.0 && f.rect.y < 230.0, "{:?}", f.rect);
        assert!(f.rect.w > 80.0 && f.rect.w < 120.0, "{:?}", f.rect);
        assert!(!f.existing);
        assert!(find(&screen(), &|_| vec![true])[0].existing);
    }
    #[test]
    fn a_crop_reads_back() {
        let shot = screen();
        let png = crop(&shot, Rect { x: 440.0, y: 190.0, w: 140.0, h: 140.0 }).unwrap();
        let text = crate::platform::files::decode_qr(&png).unwrap();
        assert!(text.starts_with("otpauth://totp/Example:"));
        assert!(crop(&shot, Rect { x: 2000.0, y: 0.0, w: 10.0, h: 10.0 }).is_err());
    }
}
