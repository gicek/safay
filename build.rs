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
/// Public OAuth client IDs from the local, untracked `cloud-keys.toml`, exposed to
/// the app as compile-time variables. A missing file or empty value hides that service (SECURITY.md, Network).
fn cloud_keys() {
    println!("cargo:rerun-if-changed=cloud-keys.toml");
    let Ok(text) = fs::read_to_string("cloud-keys.toml") else { return };
    let mut section = String::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_owned();
        } else if let Some((key, value)) = line.split_once('=') {
            let value = value.trim().trim_matches('"').trim();
            let name = match (section.as_str(), key.trim()) {
                ("dropbox", "app_key") => "SAFAY_DROPBOX_APP_KEY",
                ("dropbox", "full_app_key") => "SAFAY_DROPBOX_FULL_APP_KEY",
                ("microsoft", "client_id") => "SAFAY_MICROSOFT_CLIENT_ID",
                ("microsoft", "full_access") => "SAFAY_MICROSOFT_FULL",
                ("google", "client_id") => "SAFAY_GOOGLE_CLIENT_ID",
                ("google", "full_access") => "SAFAY_GOOGLE_FULL",
                _ => continue,
            };
            if !value.is_empty() && value != "false" && value.chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c)) {
                println!("cargo:rustc-env={name}={value}");
            }
        }
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
