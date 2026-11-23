//! UI text keyed by its English source string. `en.json` holds only
//! the keys whose English differs from the key (plurals, error codes); the Chinese tables hold every key.
use nsh_core::prelude::SharedString;
use std::{collections::HashMap, sync::OnceLock};
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    En,
    Hans,
    Hant,
}
fn dictionaries() -> &'static [HashMap<String, String>; 3] {
    static DICTS: OnceLock<[HashMap<String, String>; 3]> = OnceLock::new();
    DICTS.get_or_init(|| {
        [include_str!("../locales/en.json"), include_str!("../locales/zh-Hans.json"), include_str!("../locales/zh-Hant.json")]
            .map(|s| serde_json::from_str(s).expect("bundled translations"))
    })
}
/// The language's code as launchers read it (`find accounts`).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn code(language: Language) -> &'static str {
    match language {
        Language::En => "en",
        Language::Hans => "zh-Hans",
        Language::Hant => "zh-Hant",
    }
}
pub fn tr(language: Language, key: &str) -> SharedString {
    let d = dictionaries();
    d[language as usize].get(key).or_else(|| d[0].get(key)).map(|s| SharedString::from(s.clone())).unwrap_or_else(|| key.to_owned().into())
}
pub fn error_key(e: safay_app::session::Error) -> &'static str {
    use safay_app::session::Error::*;
    match e {
        Password => "Incorrect master password",
        Corrupt => "The vault is damaged or unsupported. Try the last good copy.",
        Weak => "Too easy to crack. Use a longer password, or a few unrelated words.",
        Fair => "Password Isn’t Strong Enough",
        Wait => "Too many attempts. Wait before trying again.",
        Fields => "Check the secret, website, digits and period.",
        Duplicate | Exists => "All accounts in this file already exist",
        Category => "Choose a non-empty, unique category name (up to 40 characters).",
        Import => "No supported accounts were found. Check the file or links.",
        FilePasswordRequired => "File password required",
        FilePassword => "Incorrect file password",
        ImportFile => "Not a supported backup file",
        ImportEmpty => "No accounts to import in this file",
        Storage => "The change couldn’t be saved. Your previous vault is kept.",
        Changed => "The vault changed outside this session. Lock and reopen it.",
        TooLarge => "The file exceeds the size or image limit.",
        Crypto => "System cryptography is unavailable. Nothing was saved.",
        Locked | Cancelled => "Unlock Safay to continue.",
        Missing | NotFound => "That item is no longer available.",
        StaleKey if cfg!(target_os = "android") => {
            "Biometric unlock needs your master password once more. Unlock with it to turn biometric unlock back on."
        }
        StaleKey => "Touch ID needs your master password once more. Unlock with it to turn Touch ID back on.",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    /// Every key in either Chinese table exists in the other; English falls back to the key itself.
    #[test]
    fn chinese_tables_have_identical_keys() {
        let maps = dictionaries();
        let mut a: Vec<_> = maps[1].keys().collect();
        let mut b: Vec<_> = maps[2].keys().collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
        for key in maps[0].keys() {
            assert!(maps[1].contains_key(key), "missing zh-Hans for {key}");
        }
    }
    /// Every literal key the code passes to `label` or `fmt` (in the first argument, branches included) is translated:
    /// a key missing from both Chinese tables would show in English. Names and formats that read the same in every
    /// language are exempt.
    #[test]
    fn every_key_in_the_code_is_translated() {
        const SAME_IN_EVERY_LANGUAGE: [&str; 3] = ["Steam", "Battle.net", "{} ({})"];
        fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        /// The string literals of a call's first argument, from just after its opening parenthesis.
        fn first_argument_literals(rest: &str) -> Vec<String> {
            let (mut depth, mut literal, mut found) = (0_i32, None::<String>, Vec::new());
            let mut chars = rest.chars();
            while let Some(c) = chars.next() {
                if let Some(text) = literal.as_mut() {
                    match c {
                        '\\' => {
                            if let Some(next) = chars.next() {
                                text.push(next);
                            }
                        }
                        '"' => found.push(literal.take().unwrap_or_default()),
                        _ => text.push(c),
                    }
                    continue;
                }
                match c {
                    '"' => literal = Some(String::new()),
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' if depth == 0 => break,
                    ')' | ']' | '}' => depth -= 1,
                    ',' if depth == 0 => break,
                    _ => {}
                }
            }
            found
        }
        let mut files = Vec::new();
        sources(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let hans = &dictionaries()[1];
        let mut missing = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            for receiver in ["s.", "self.", "store."] {
                for call in ["label(", "fmt("] {
                    let opening = format!("{receiver}{call}");
                    for (at, _) in text.match_indices(&opening) {
                        if text[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            continue;
                        }
                        for key in first_argument_literals(&text[at + opening.len()..]) {
                            if !key.is_empty() && !hans.contains_key(&key) && !SAME_IN_EVERY_LANGUAGE.contains(&key.as_str()) {
                                missing.push(format!("{}:{}: {key}", file.display(), text[..at].lines().count()));
                            }
                        }
                    }
                }
            }
        }
        assert!(missing.is_empty(), "untranslated keys:\n{}", missing.join("\n"));
    }
}
