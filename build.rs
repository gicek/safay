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
fn main() {
    nsh_revision();
    cloud_keys();
    println!("cargo:rerun-if-changed=assets");
    let staged = std::path::PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output")).join("assets/@safay");
    copy_tree(Path::new("assets"), &staged);
}
