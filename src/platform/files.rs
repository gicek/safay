use base64::Engine;
use nsh_core::{
    file_dialog::{FileFilter, OpenDialog, SaveDialog},
    prelude::{pick_paths, pick_save_path},
};
use safay_app::{
    session::Error,
    storage::{atomic_write, bounded_read},
};
use std::{io::Cursor, path::PathBuf};

// The system's file dialogs. NSH's (`pick_paths`, `pick_save_path`: the file chooser portal on Linux, the shell's dialogs
// on Windows, a sheet on macOS) everywhere but in the app itself on macOS, where the app presents the same sheet itself
// (`super::macos::file_panel`): NSH's appears about 0.6 s late, without its animation (docs/ARCHITECTURE.md). `Err`: the
// dialog could not show (Linux without a portal); `Ok(None)`: cancelled.

async fn open_paths(dialog: OpenDialog) -> Result<Option<Vec<PathBuf>>, Error> {
    #[cfg(target_os = "macos")]
    if crate::hosted() {
        let kind = super::macos::FilePanel::Open { folders: dialog.picks_folders(), multiple: dialog.multiple };
        let extensions = nsh_core::file_dialog::extensions(&dialog.filters);
        let chosen = super::macos::file_panel(kind, dialog.title.clone(), dialog.button_label.clone(), extensions);
        if let Ok(Ok(Ok(paths))) = nsh_core::prelude::blocking(move || chosen.recv()).await {
            return Ok(paths);
        }
    }
    pick_paths(dialog).await.map_err(|_| Error::Storage)
}
async fn save_path(dialog: SaveDialog) -> Result<Option<PathBuf>, Error> {
    #[cfg(target_os = "macos")]
    if crate::hosted() {
        let kind = super::macos::FilePanel::Save { name: dialog.file_name.clone() };
        let extensions = nsh_core::file_dialog::extensions(&dialog.filters);
        let chosen = super::macos::file_panel(kind, dialog.title.clone(), dialog.button_label.clone(), extensions);
        if let Ok(Ok(Ok(paths))) = nsh_core::prelude::blocking(move || chosen.recv()).await {
            return Ok(paths.and_then(|p| p.into_iter().next()));
        }
    }
    pick_save_path(dialog).await.map_err(|_| Error::Storage)
}

/// A chosen backup: its display name and bounded contents.
pub async fn choose_import(title: String, filter: String) -> Result<Option<(String, Vec<u8>)>, Error> {
    let dialog = OpenDialog::files().title(title).filter(FileFilter::new(filter, ["json", "safayvault", "txt", "2fas", "csv"]));
    let Some(path) = open_paths(dialog).await?.and_then(|p| p.into_iter().next()) else {
        return Ok(None);
    };
    read_named(path).await.map(Some)
}
/// Several QR images at once.
pub async fn choose_images(title: String, filter: String) -> Result<Vec<(String, Vec<u8>)>, Error> {
    let dialog = OpenDialog::files().multiple(true).title(title).filter(FileFilter::new(filter, ["png", "jpg", "jpeg"]));
    let Some(paths) = open_paths(dialog).await? else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for path in paths.into_iter().take(20) {
        out.push(read_named(path).await?);
    }
    Ok(out)
}
/// A folder (backup location, or the first-run restore source). `message` is the line the sheet shows; `button` its
/// confirm button. `None` when cancelled or the dialog could not show.
pub async fn choose_folder(message: String, button: String) -> Option<PathBuf> {
    let dialog = OpenDialog::folders().title(message).button_label(button);
    open_paths(dialog).await.ok().flatten().and_then(|p| p.into_iter().next())
}
/// Reads a dropped or chosen path with the same size limit as the pickers.
pub async fn read_named(path: std::path::PathBuf) -> Result<(String, Vec<u8>), Error> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let bytes = nsh_core::prelude::blocking(move || bounded_read(&path)).await.map_err(|_| Error::Storage)??.ok_or(Error::Missing)?;
    Ok((name, bytes))
}
/// `Safay-export-yyyyMMdd-HHmm.json` in UTC, without a time-zone database.
pub fn export_file_name(now: std::time::SystemTime) -> String {
    let secs = now.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("Safay-export-{y:04}{m:02}{d:02}-{:02}{:02}.json", rem / 3600, rem % 3600 / 60)
}
pub async fn save_export(data: Vec<u8>, title: String) -> Result<bool, Error> {
    let name = export_file_name(std::time::SystemTime::now());
    let dialog = SaveDialog::new().title(title).file_name(name).filter(FileFilter::new("JSON", ["json"]));
    let Some(path) = save_path(dialog).await? else { return Ok(false) };
    nsh_core::prelude::blocking(move || atomic_write(&path, &data)).await.map_err(|_| Error::Storage)??;
    Ok(true)
}

pub fn qr_image(uri: &str) -> Result<String, Error> {
    let qr = qrcode::QrCode::new(uri.as_bytes()).map_err(|_| Error::Fields)?;
    let buffer = qr.render::<image::Luma<u8>>().quiet_zone(true).min_dimensions(280, 280).build();
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(buffer).write_to(&mut output, image::ImageFormat::Png).map_err(|_| Error::Fields)?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(output.into_inner())))
}
pub fn decode_qr(data: &[u8]) -> Result<String, Error> {
    if data.len() > 16 * 1024 * 1024 {
        return Err(Error::TooLarge);
    }
    let reader = image::ImageReader::new(Cursor::new(data)).with_guessed_format().map_err(|_| Error::Import)?;
    let (w, h) = reader.into_dimensions().map_err(|_| Error::Import)?;
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return Err(Error::TooLarge);
    }
    let mut reader = image::ImageReader::new(Cursor::new(data)).with_guessed_format().map_err(|_| Error::Import)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let gray = reader.decode().map_err(|_| Error::Import)?.to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(gray);
    let mut texts = Vec::new();
    for grid in prepared.detect_grids() {
        if let Ok((_, value)) = grid.decode() {
            texts.push(value);
        }
    }
    if texts.is_empty() {
        return Err(Error::Import);
    }
    Ok(texts.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_name_matches_its_pattern() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_793_840_460);
        assert_eq!(export_file_name(t), "Safay-export-20261105-0101.json");
    }
    #[test]
    fn public_otp_qr_round_trip_is_local() {
        let uri = "otpauth://totp/RFC:public?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=RFC";
        let encoded = qr_image(uri).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded.strip_prefix("data:image/png;base64,").unwrap()).unwrap();
        assert_eq!(decode_qr(&bytes).unwrap(), uri);
    }
    #[test]
    fn qr_input_rejects_oversized_and_malformed_data() {
        assert!(matches!(decode_qr(&vec![0; 16 * 1024 * 1024 + 1]), Err(Error::TooLarge)));
        assert!(matches!(decode_qr(b"not an image"), Err(Error::Import)));
        let buffer = image::GrayImage::new(4097, 1);
        let mut output = Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(buffer).write_to(&mut output, image::ImageFormat::Png).unwrap();
        assert!(matches!(decode_qr(output.get_ref()), Err(Error::TooLarge)));
    }
}

/// The path a file reference URL (`file:///.file/id=…`, what Finder drags carry) refers to, through
/// `NSURL.filePathURL`; `None` when the file is gone.
#[cfg(target_os = "macos")]
pub fn reference_path(uri: &str) -> Option<std::path::PathBuf> {
    use nsh_platform_macos::objc2_foundation::{NSString, NSURL};
    let url = NSURL::URLWithString(&NSString::from_str(uri))?;
    let path = url.filePathURL()?.path()?;
    Some(std::path::PathBuf::from(path.to_string()))
}

#[cfg(all(test, target_os = "macos"))]
mod reference_tests {
    /// Finder drags carry `file:///.file/id=…` URLs; the drop targets must read the file they refer to.
    #[test]
    fn a_file_reference_url_resolves_to_its_path() {
        use nsh_platform_macos::objc2_foundation::{NSString, NSURL};
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("dropped backup.json");
        std::fs::write(&file, b"{}").unwrap();
        let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
        let reference = url.fileReferenceURL().unwrap().absoluteString().unwrap().to_string();
        assert!(reference.starts_with("file:///.file/id="), "{reference}");
        let resolved = crate::pages::unlock::file_path(&reference).unwrap();
        assert_eq!(resolved.canonicalize().unwrap(), file.canonicalize().unwrap());
    }
}
