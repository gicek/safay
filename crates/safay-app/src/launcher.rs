//! What launchers (Alfred, Raycast, Tinycast) may see: an in-memory search index of the accounts' names, never a secret
//! or a code (SECURITY.md, Launcher integration).
//!
//! The index is built while the vault is unlocked and the third-party plugin interface is on, and kept when the vault
//! locks, so a launcher can search a locked Safay. It is cleared when the
//! interface is turned off and when the vault is reset, and dies with the process; it is never written anywhere.
//! Searching reads only the index, never the session, so a launcher never waits for a key derivation.
use crate::core::search;
use serde::Serialize;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;
use zeroize::Zeroize;

/// The most accounts one search returns.
pub const MAX_RESULTS: usize = 50;

struct Entry {
    id: Uuid,
    issuer: String,
    username: String,
}
impl Drop for Entry {
    fn drop(&mut self) {
        self.issuer.zeroize();
        self.username.zeroize();
    }
}

/// Shared by the session (which keeps it current) and the launcher commands (which read it).
#[derive(Default)]
pub struct Launcher {
    enabled: AtomicBool,
    /// `None` until the vault is first unlocked with the interface on.
    index: Mutex<Option<Vec<Entry>>>,
}

/// A search's answer, as the launchers read it (JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Found {
    /// The third-party plugin interface is on.
    pub enabled: bool,
    /// The vault was unlocked since Safay started (with the interface on): there is something to search.
    pub ready: bool,
    pub accounts: Vec<Match>,
    /// Safay's language (`en`, `zh-Hans`, `zh-Hant`), so a launcher's text matches the app's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<&'static str>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Match {
    pub id: Uuid,
    pub issuer: String,
    /// Masked (`search::mask_username`); the full name is only matched against, never returned.
    pub username: String,
}
impl Found {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"enabled":false,"ready":false,"accounts":[]}"#.into())
    }
}

impl Launcher {
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }
    /// Turning the interface off forgets the index at once.
    pub(crate) fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        if !enabled {
            self.clear();
        }
    }
    pub(crate) fn clear(&self) {
        if let Ok(mut index) = self.index.lock() {
            *index = None;
        }
    }
    /// The accounts as they are now, in the vault's order; ignored while the interface is off.
    pub(crate) fn rebuild<'a>(&self, accounts: impl Iterator<Item = (Uuid, &'a str, &'a str)>) {
        if !self.enabled() {
            return;
        }
        let next = accounts.map(|(id, issuer, username)| Entry { id, issuer: issuer.to_owned(), username: username.to_owned() }).collect();
        if let Ok(mut index) = self.index.lock() {
            *index = Some(next);
        }
    }
    /// Matches as the main window's search does, against the full names.
    pub fn find(&self, query: &str) -> Found {
        let enabled = self.enabled();
        let index = self.index.lock().ok();
        let entries = index.as_ref().and_then(|i| i.as_ref()).filter(|_| enabled);
        let accounts = entries
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| search::matches_search(&e.issuer, &e.username, query))
                    .take(MAX_RESULTS)
                    .map(|e| Match { id: e.id, issuer: e.issuer.clone(), username: search::mask_username(&e.username) })
                    .collect()
            })
            .unwrap_or_default();
        Found { enabled, ready: entries.is_some(), accounts, language: None }
    }
    /// The service name of an indexed account (for the Touch ID prompt and the notice); `None` when the interface is off,
    /// nothing is indexed yet, or the ID is unknown.
    pub fn issuer(&self, id: Uuid) -> Option<String> {
        if !self.enabled() {
            return None;
        }
        let index = self.index.lock().ok()?;
        index.as_ref()?.iter().find(|e| e.id == id).map(|e| e.issuer.clone())
    }
    /// Whether anything is indexed (the vault was unlocked with the interface on).
    pub fn ready(&self) -> bool {
        self.enabled() && self.index.lock().is_ok_and(|i| i.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(l: &Launcher) -> (Uuid, Uuid) {
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
        l.rebuild([(a, "GitHub", "alex@example.com"), (b, "Google", "12345678901")].into_iter());
        (a, b)
    }
    #[test]
    fn off_by_default_and_never_indexes_while_off() {
        let l = Launcher::default();
        sample(&l);
        assert_eq!(l.find(""), Found { enabled: false, ready: false, accounts: vec![], language: None });
        assert!(l.issuer(Uuid::from_u128(1)).is_none());
    }
    #[test]
    fn matches_full_names_and_returns_them_masked() {
        let l = Launcher::default();
        l.set_enabled(true);
        assert_eq!(l.find("git"), Found { enabled: true, ready: false, accounts: vec![], language: None });
        let (a, b) = sample(&l);
        let found = l.find("alex");
        assert!(found.ready);
        assert_eq!(found.accounts, vec![Match { id: a, issuer: "GitHub".into(), username: "al***@example.com".into() }]);
        assert_eq!(l.find("goo").accounts[0].username, "123****01");
        assert_eq!(l.find("").accounts.len(), 2);
        assert_eq!(l.issuer(b).as_deref(), Some("Google"));
        let json = l.find("").to_json();
        assert!(!json.contains("alex@") && !json.contains("12345678901"), "{json}");
    }
    #[test]
    fn turning_off_forgets_the_index() {
        let l = Launcher::default();
        l.set_enabled(true);
        sample(&l);
        l.set_enabled(false);
        l.set_enabled(true);
        assert!(!l.find("").ready);
    }
    #[test]
    fn at_most_fifty_results() {
        let l = Launcher::default();
        l.set_enabled(true);
        let ids: Vec<Uuid> = (0..80).map(Uuid::from_u128).collect();
        l.rebuild(ids.iter().map(|id| (*id, "Service", "name")));
        assert_eq!(l.find("").accounts.len(), MAX_RESULTS);
    }
}
