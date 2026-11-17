use std::time::{Duration, Instant};

use super::*;
use crate::otpauth::tests::placeholders;

fn golden() -> Vec<(String, Vec<u8>, String)> {
    let golden: Value = serde_json::from_str(include_str!("../../../../golden/imports.json")).unwrap();
    golden["imports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_owned(),
                s["file"].as_str().unwrap().as_bytes().to_vec(),
                s["password"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn sample(name: &str) -> Vec<u8> {
    golden().into_iter().find(|(n, _, _)| n == name).unwrap().1
}

/// `inspect` agrees with `import_file` given no password: `Encrypted` exactly where it reports `PasswordRequired`, and
/// `Unsupported` exactly where nothing is detected.
#[test]
fn inspect_matches_import_file() {
    for (name, data, _) in golden() {
        let kind = inspect(&data);
        match import_file(&data, "", SystemTime::UNIX_EPOCH, &placeholders()) {
            Err(ImportError::PasswordRequired) => assert_eq!(kind, FileKind::Encrypted, "{name}"),
            Err(ImportError::InvalidFile) if detect(&data).is_none() => assert_eq!(kind, FileKind::Unsupported, "{name}"),
            _ => assert_eq!(kind, FileKind::Plain, "{name}"),
        }
    }
}

/// Changing any byte of an Ente secretstream message (tag byte, ciphertext, or MAC) fails authentication.
#[test]
fn secretstream_rejects_tampering() {
    let root: Value = serde_json::from_slice(&sample("ente encrypted length 3")).unwrap();
    let decode = |key: &str| base64::Engine::decode(&base64::engine::general_purpose::STANDARD, key).unwrap();
    let header = decode(root["encryptionNonce"].as_str().unwrap());
    let sealed = decode(root["encryptedData"].as_str().unwrap());
    let key = [7u8; 32];
    assert!(ente::secretstream_pull(&key, &header, &sealed).is_none(), "wrong key");
    let mut short = sealed.clone();
    short.truncate(16);
    assert!(ente::secretstream_pull(&key, &header, &short).is_none());
    // With the right key (derived as the import does), every single-byte change is caught.
    let salt = decode(root["kdfParams"]["salt"].as_str().unwrap());
    let key = vault::argon2id_raw(b"p", &salt, 1, 8, 1, 32).unwrap();
    assert!(ente::secretstream_pull(&key, &header, &sealed).is_some());
    for index in 0..sealed.len() {
        let mut changed = sealed.clone();
        changed[index] ^= 0x01;
        assert!(ente::secretstream_pull(&key, &header, &changed).is_none(), "byte {index}");
    }
}

/// Key derivation parameters beyond the limits make the file invalid before anything is computed.
#[test]
fn rejects_costly_parameters_at_once() {
    let started = Instant::now();
    for name in ["aegis scrypt beyond limits", "ente kdf beyond limits"] {
        assert_eq!(import_file(&sample(name), "p", SystemTime::UNIX_EPOCH, &placeholders()).unwrap_err(), ImportError::InvalidFile);
    }
    let mut ente: Value = serde_json::from_slice(&sample("ente encrypted")).unwrap();
    ente["kdfParams"]["opsLimit"] = 4_000_000_000u64.into();
    let data = ente.to_string();
    assert_eq!(import_file(data.as_bytes(), "p", SystemTime::UNIX_EPOCH, &placeholders()).unwrap_err(), ImportError::InvalidFile);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn csv_records_follow_rfc_4180() {
    let text = "a,\"b,1\",\"say \"\"hi\"\"\"\r\n\"multi\nline\",,\n\nlast";
    let rows: Vec<Vec<String>> = csv_rows(text);
    assert_eq!(
        rows,
        vec![
            vec!["a".to_owned(), "b,1".into(), "say \"hi\"".into()],
            vec!["multi\nline".into(), String::new(), String::new()],
            vec![String::new()],
            vec!["last".into()],
        ]
    );
}

fn csv_rows(text: &str) -> Vec<Vec<String>> {
    // Through the importer's own detection: a header with an OTP column, then rows.
    let with_header = format!("name,otpauth\n{text}");
    assert!(csv::is_csv(&with_header));
    let mut rows = Vec::new();
    for line in csv::records(text) {
        rows.push(line.unwrap().iter().map(|field| field.to_string()).collect());
    }
    rows
}

/// A text file with an incomplete Google Authenticator migration batch is refused, as the Add sheet refuses it.
#[test]
fn text_refuses_incomplete_migration_batches() {
    let first = crate::otpauth::tests::migration_url(&[(b"foobar", "a", "A", 1, 1, 2)], Some((2, 0, 9)));
    assert_eq!(import_file(first.as_bytes(), "", SystemTime::UNIX_EPOCH, &placeholders()).unwrap_err(), ImportError::InvalidFile);
    let second = crate::otpauth::tests::migration_url(&[(b"foobaz", "b", "B", 1, 1, 2)], Some((2, 1, 9)));
    let both = format!("{first}\n{second}\n");
    let imported = import_file(both.as_bytes(), "", SystemTime::UNIX_EPOCH, &placeholders()).unwrap();
    assert_eq!(imported.contents.accounts.iter().map(|a| a.issuer.as_str()).collect::<Vec<_>>(), vec!["A", "B"]);
}

/// Group names longer than a category allows are shortened, and blank ones are ignored.
#[test]
fn group_names_fit_categories() {
    let long = "x".repeat(60);
    let file = serde_json::json!({
        "version": 1, "header": {"slots": null, "params": null},
        "db": {"version": 3, "groups": [{"uuid": "g", "name": long}, {"uuid": "b", "name": "   "}], "entries": [
            {"type": "totp", "name": "n", "issuer": "i", "info": {"secret": "MZXW6YTBOI"}, "groups": ["g", "b"]}
        ]}
    });
    let imported = import_file(file.to_string().as_bytes(), "", SystemTime::UNIX_EPOCH, &placeholders()).unwrap();
    assert_eq!(imported.contents.categories.len(), 1);
    assert_eq!(imported.contents.categories[0].name.chars().count(), MAX_NAME_CHARS);
}
