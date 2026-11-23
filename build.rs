//! Stage immutable public assets for direct cargo runs, using NSH's asset namespace.
use std::{env, fs, path::Path};
fn copy_tree(source: &Path, dest: &Path) {
    fs::create_dir_all(dest).expect("asset directory");
    for entry in fs::read_dir(source).expect("public assets") {
        let entry = entry.expect("asset entry");
        let target = dest.join(entry.file_name());
        if entry.file_type().expect("asset type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("stage asset");
        }
    }
}
/// Public OAuth client IDs and their redirect URIs from the local, untracked `cloud-keys.toml`
/// (`cloud-keys.example.toml` describes it), exposed to the app as compile-time variables. A missing file or empty
/// client ID hides that service (SECURITY.md, Network); a client ID without the values that go with it stops the
/// build.
fn cloud_keys() {
    println!("cargo:rerun-if-changed=cloud-keys.toml");
    let Ok(text) = fs::read_to_string("cloud-keys.toml") else { return };
    let mut section = String::new();
    let mut seen = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_owned();
        } else if let Some((key, value)) = line.split_once('=') {
            let value = value.trim().trim_matches('"').trim();
            let name = match (section.as_str(), key.trim()) {
                ("dropbox", "app_key") => "SAFAY_DROPBOX_APP_KEY",
                ("microsoft", "client_id") => "SAFAY_MICROSOFT_CLIENT_ID",
                ("google", "client_id") => "SAFAY_GOOGLE_CLIENT_ID",
                ("dropbox", "redirect_uri") => "SAFAY_DROPBOX_REDIRECT_URI",
                ("microsoft", "redirect_uri") => "SAFAY_MICROSOFT_REDIRECT_URI",
                ("google", "redirect_uri") => "SAFAY_GOOGLE_REDIRECT_URI",
                ("google", "folder_name") => "SAFAY_GOOGLE_FOLDER_NAME",
                ("google.desktop", "client_id") => "SAFAY_GOOGLE_DESKTOP_CLIENT_ID",
                ("google.desktop", "client_secret") => "SAFAY_GOOGLE_DESKTOP_CLIENT_SECRET",
                ("google.desktop", "redirect_uri") => "SAFAY_GOOGLE_DESKTOP_REDIRECT_URI",
                _ => continue,
            };
            // A redirect URI also has its scheme separator and path; a folder name may have spaces.
            let allowed = match name {
                "SAFAY_GOOGLE_FOLDER_NAME" => "-._ ",
                name if name.ends_with("_REDIRECT_URI") => "-._:/",
                _ => "-._",
            };
            if !value.is_empty() && value != "false" && value.chars().all(|c| c.is_ascii_alphanumeric() || allowed.contains(c)) {
                println!("cargo:rustc-env={name}={value}");
                seen.push(name);
            }
        }
    }
    for (id, needed, entry, key) in [
        ("SAFAY_DROPBOX_APP_KEY", "SAFAY_DROPBOX_REDIRECT_URI", "[dropbox] app_key", "a redirect_uri"),
        ("SAFAY_MICROSOFT_CLIENT_ID", "SAFAY_MICROSOFT_REDIRECT_URI", "[microsoft] client_id", "a redirect_uri"),
        ("SAFAY_GOOGLE_CLIENT_ID", "SAFAY_GOOGLE_REDIRECT_URI", "[google] client_id", "a redirect_uri"),
        ("SAFAY_GOOGLE_CLIENT_ID", "SAFAY_GOOGLE_FOLDER_NAME", "[google] client_id", "[google] folder_name"),
        ("SAFAY_GOOGLE_DESKTOP_CLIENT_ID", "SAFAY_GOOGLE_DESKTOP_CLIENT_SECRET", "[google.desktop] client_id", "client_secret"),
        ("SAFAY_GOOGLE_DESKTOP_CLIENT_ID", "SAFAY_GOOGLE_DESKTOP_REDIRECT_URI", "[google.desktop] client_id", "redirect_uri"),
        ("SAFAY_GOOGLE_DESKTOP_CLIENT_ID", "SAFAY_GOOGLE_FOLDER_NAME", "[google.desktop] client_id", "[google] folder_name"),
    ] {
        assert!(
            !seen.contains(&id) || seen.contains(&needed),
            "cloud-keys.toml: {entry} is set without {key} (see cloud-keys.example.toml)"
        );
    }
}
/// The NSH commit Cargo.lock pins (NSH is tracked from main), shown in Settings → About.
fn nsh_revision() {
    println!("cargo:rerun-if-changed=Cargo.lock");
    let lock = fs::read_to_string("Cargo.lock").unwrap_or_default();
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line == "name = \"nsh-core\"" {
            let version = lines.next().and_then(|l| l.strip_prefix("version = \"")).map(|v| v.trim_end_matches('"')).unwrap_or("");
            let commit = lines.next().and_then(|l| l.rsplit_once('#')).map(|(_, c)| c.trim_end_matches('"')).unwrap_or("");
            let short = &commit[..commit.len().min(7)];
            let shown = if short.is_empty() { version.to_owned() } else { format!("{version} ({short})") };
            println!("cargo:rustc-env=SAFAY_NSH_REVISION={shown}");
            return;
        }
    }
}
/// Windows (MSVC): the app's icon as the executable's first icon group (`RT_GROUP_ICON` 1), which NSH's window class
/// loads (`LoadIconW(hinstance, 1)`) and Explorer, the taskbar and shortcuts show. NSH's own packager writes the same
/// resource into its runner; Safay builds its executable with cargo, so the icon is linked in as a compiled resource
/// file (`.res`), which the MSVC linker takes directly. Images as NSH makes them: 32-bit DIBs from 16 to 64 pixels and a
/// 256-pixel PNG.
fn windows_icon() {
    let icon = "assets/icon.png";
    println!("cargo:rerun-if-changed={icon}");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    use image::imageops::FilterType;
    const RT_ICON: u16 = 3;
    const RT_GROUP_ICON: u16 = 14;
    let source = image::open(icon).expect("app icon");
    let mut images: Vec<(u32, Vec<u8>)> = Vec::new();
    for size in [16, 20, 24, 32, 40, 48, 64] {
        let rgba = source.resize_exact(size, size, FilterType::Lanczos3).into_rgba8();
        // BITMAPINFOHEADER of twice the height (image and mask), BGRA rows bottom up, then an all-zero 1-bit mask.
        let mask_row = size.div_ceil(32) * 4;
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&(size as i32).to_le_bytes());
        dib.extend_from_slice(&(size as i32 * 2).to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&[0; 24]);
        for y in (0..size).rev() {
            for x in 0..size {
                let [r, g, b, a] = rgba.get_pixel(x, y).0;
                dib.extend_from_slice(&[b, g, r, a]);
            }
        }
        dib.resize(dib.len() + (mask_row * size) as usize, 0);
        images.push((size, dib));
    }
    let mut png = Vec::new();
    source
        .resize_exact(256, 256, FilterType::Lanczos3)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("encode the icon");
    images.push((256, png));
    // GRPICONDIR and one GRPICONDIRENTRY per image; the images are RT_ICON 1, 2, ...
    let mut group = Vec::new();
    group.extend_from_slice(&[0, 0, 1, 0]);
    group.extend_from_slice(&(images.len() as u16).to_le_bytes());
    for (index, (size, data)) in images.iter().enumerate() {
        let side = if *size >= 256 { 0 } else { *size as u8 };
        group.extend_from_slice(&[side, side, 0, 0]);
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&32u16.to_le_bytes());
        group.extend_from_slice(&(data.len() as u32).to_le_bytes());
        group.extend_from_slice(&(index as u16 + 1).to_le_bytes());
    }
    // A .res file: an empty entry, then each resource's header (numeric type and name: 32 bytes) and its data, each
    // padded to four bytes. Language neutral; moveable, pure and discardable as rc.exe marks icons.
    fn entry(res: &mut Vec<u8>, kind: u16, id: u16, flags: u16, data: &[u8]) {
        res.extend_from_slice(&(data.len() as u32).to_le_bytes());
        res.extend_from_slice(&32u32.to_le_bytes());
        res.extend_from_slice(&[0xff, 0xff]);
        res.extend_from_slice(&kind.to_le_bytes());
        res.extend_from_slice(&[0xff, 0xff]);
        res.extend_from_slice(&id.to_le_bytes());
        res.extend_from_slice(&0u32.to_le_bytes());
        res.extend_from_slice(&flags.to_le_bytes());
        res.extend_from_slice(&0u16.to_le_bytes());
        res.extend_from_slice(&[0; 8]);
        res.extend_from_slice(data);
        res.resize(res.len().next_multiple_of(4), 0);
    }
    let mut res = vec![0, 0, 0, 0, 0x20, 0, 0, 0, 0xff, 0xff, 0, 0, 0xff, 0xff, 0, 0];
    res.resize(32, 0);
    for (index, (_, data)) in images.iter().enumerate() {
        entry(&mut res, RT_ICON, index as u16 + 1, 0x1010, data);
    }
    entry(&mut res, RT_GROUP_ICON, 1, 0x1030, &group);
    let path = Path::new(&env::var_os("OUT_DIR").expect("Cargo output")).join("icon.res");
    fs::write(&path, res).expect("write the icon resource");
    println!("cargo:rustc-link-arg-bins={}", path.display());
}
fn main() {
    nsh_revision();
    cloud_keys();
    windows_icon();
    println!("cargo:rerun-if-changed=assets");
    let staged = std::path::PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output")).join("assets/@safay");
    copy_tree(Path::new("assets"), &staged);
}
