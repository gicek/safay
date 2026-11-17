//! Bitwarden JSON exports (the password manager and Bitwarden Authenticator write the same structure). Only unencrypted
//! exports are read.
//!
//! Each item may carry `login.totp`: an `otpauth://` link, a `steam://` link, or a bare Base32 secret. Items without one
//! are passwords only and are ignored. Folders become categories and favorites are pinned.

use serde_json::{Map, Value};

use super::{Collector, Extras, ImportError, bool_field, str_field};

pub(super) fn detect(root: &Map<String, Value>) -> bool {
    root.get("items").is_some_and(Value::is_array) && !bool_field(root, "encrypted")
}

pub(super) fn import(root: &Map<String, Value>, collector: &mut Collector<'_>) -> Result<(), ImportError> {
    let Some(Value::Array(items)) = root.get("items") else { return Err(ImportError::InvalidFile) };
    let folders: Vec<(&str, &str)> = match root.get("folders") {
        Some(Value::Array(folders)) => {
            folders.iter().filter_map(Value::as_object).map(|folder| (str_field(folder, "id"), str_field(folder, "name"))).collect()
        }
        _ => Vec::new(),
    };
    for item in items.iter().filter_map(Value::as_object) {
        let Some(login) = item.get("login").and_then(Value::as_object) else { continue };
        let totp = str_field(login, "totp");
        if totp.trim().is_empty() {
            continue;
        }
        let folder_id = str_field(item, "folderId");
        let extras = Extras {
            groups: folders
                .iter()
                .filter(|(id, _)| !folder_id.is_empty() && *id == folder_id)
                .map(|(_, name)| (*name).to_owned())
                .collect(),
            pinned: bool_field(item, "favorite"),
            website: login
                .get("uris")
                .and_then(Value::as_array)
                .and_then(|uris| uris.iter().filter_map(Value::as_object).map(|uri| str_field(uri, "uri")).find(|uri| !uri.is_empty()))
                .map(str::to_owned),
        };
        collector.add_link(totp, str_field(item, "name"), str_field(login, "username"), extras)?;
    }
    Ok(())
}
