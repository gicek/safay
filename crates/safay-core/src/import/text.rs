//! Plain text with one `otpauth://` or `otpauth-migration://` link per line: Ente Auth's plain export, link lists
//! exported by other apps, and pasted text saved to a file.
//!
//! Ente Auth adds a `codeDisplay` query parameter holding JSON: its `tags` become categories, `pinned` entries are
//! pinned, and `trashed` entries are skipped.

use serde_json::Value;

use super::{Collector, Extras, ImportError};
use crate::otpauth;

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim).filter(|line| !line.is_empty())
}

fn is_link(line: &str) -> bool {
    let lower = line.get(..20).unwrap_or(line).to_ascii_lowercase();
    lower.starts_with("otpauth://") || lower.starts_with("otpauth-migration://")
}

/// At least one line is an `otpauth://` or `otpauth-migration://` link.
pub(super) fn is_text(text: &str) -> bool {
    lines(text).any(is_link)
}

pub(super) fn import(text: &str, collector: &mut Collector<'_>) -> Result<(), ImportError> {
    let links: Vec<&str> = lines(text).filter(|line| is_link(line)).collect();
    let migrations: Vec<_> = links
        .iter()
        .filter(|line| line.to_ascii_lowercase().starts_with("otpauth-migration://"))
        .map(|line| otpauth::parse_scan_result(line, collector.placeholders))
        .collect();
    if !otpauth::batches_complete(&migrations) {
        return Err(ImportError::InvalidFile);
    }
    let mut migrations = migrations.into_iter();
    for line in links {
        if line.to_ascii_lowercase().starts_with("otpauth-migration://") {
            let Some(scan) = migrations.next() else { continue };
            for parsed in scan.accounts {
                if parsed.is_valid() {
                    collector.add_parsed(parsed, Extras::default())?;
                } else {
                    collector.skip();
                }
            }
            continue;
        }
        let display = code_display(line);
        if display.as_ref().is_some_and(|d| d.get("trashed").and_then(Value::as_bool) == Some(true)) {
            collector.skip();
            continue;
        }
        let extras = Extras {
            groups: display
                .as_ref()
                .and_then(|d| d.get("tags"))
                .and_then(Value::as_array)
                .map(|tags| tags.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                .unwrap_or_default(),
            pinned: display.as_ref().and_then(|d| d.get("pinned")).and_then(Value::as_bool) == Some(true),
            website: None,
        };
        collector.add_link(line, "", "", extras)?;
    }
    Ok(())
}

/// Ente Auth's `codeDisplay` parameter, decoded.
fn code_display(line: &str) -> Option<Value> {
    let query = line.split_once('?')?.1;
    let raw = query.split('&').find_map(|item| item.strip_prefix("codeDisplay="))?;
    let decoded = percent_encoding::percent_decode_str(raw).decode_utf8().ok()?;
    serde_json::from_str(&decoded).ok()
}
