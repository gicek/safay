//! Compares against `golden/` case by case. These files are the behavior baseline; `rust-written.json` pins what the
//! Rust core writes. The baseline is never regenerated: an intentional behavior change updates the
//! implementation and the golden files in the same commit and is explained in the commit message (golden/README.md).

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use safay_core::model::{Account, Placeholders, TotpAlgorithm, provenance};
use safay_core::otpauth::{self, ParsedOtpAuth};
use safay_core::vault;
use safay_core::{authenticator, base32, canonical, dedup, password, search, totp};
use serde_json::{Value, json};
use uuid::Uuid;

/// Reads `CARGO_MANIFEST_DIR` at run time (`cargo test` sets it): a compile-time `env!` would bake the absolute path into
/// the test binary, and after the repository moves Cargo does not always rebuild, so a stale binary would look in the
/// old location.
fn golden(name: &str) -> Value {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR").expect("run through cargo test");
    let path = PathBuf::from(manifest).join("../../golden").join(format!("{name}.json"));
    let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).unwrap()
}

fn items<'a>(value: &'a Value, key: &str) -> &'a Vec<Value> {
    value[key].as_array().unwrap_or_else(|| panic!("missing array {key}"))
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap()
}

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn signed(value: &Value) -> i64 {
    value.as_i64().unwrap()
}

fn algorithm(value: &Value) -> TotpAlgorithm {
    TotpAlgorithm::from_raw_value(text(value)).unwrap()
}

fn placeholders(file: &Value) -> Placeholders {
    Placeholders {
        unknown_issuer: text(&file["placeholders"]["unknownIssuer"]).to_owned(),
        empty_username: text(&file["placeholders"]["emptyUsername"]).to_owned(),
    }
}

fn hex(bytes: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(bytes)
}

fn unhex(value: &Value) -> Vec<u8> {
    data_encoding::HEXLOWER.decode(text(value).as_bytes()).unwrap()
}

fn seconds(date: SystemTime) -> i64 {
    date.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or_else(|e| -(e.duration().as_secs() as i64))
}

fn account_json(account: &Account) -> Value {
    json!({
        "id": account.id.hyphenated().encode_upper(&mut Uuid::encode_buffer()).to_owned(),
        "issuer": account.issuer, "username": account.username, "secret": account.secret,
        "algorithm": account.algorithm.raw_value(), "digits": account.digits, "period": account.period,
        "createdAt": seconds(account.created_at), "addedBy": account.added_by,
    })
}

fn imported_json(account: &Account) -> Value {
    let mut value = account_json(account);
    let object = value.as_object_mut().unwrap();
    object.remove("id");
    object.remove("createdAt");
    value
}

fn parsed_json(parsed: &ParsedOtpAuth) -> Value {
    json!({
        "valid": parsed.is_valid(),
        "reason": parsed.failure.map(|failure| failure.l10n_key()),
        "issuer": parsed.issuer, "account": parsed.account, "secret": parsed.secret,
        "algorithm": parsed.algorithm.raw_value(), "digits": parsed.digits, "period": parsed.period,
    })
}

#[test]
fn base32_matches_mac() {
    let file = golden("base32");
    for case in items(&file, "encode") {
        assert_eq!(base32::encode(&unhex(&case["hex"])), text(&case["base32"]), "{case}");
    }
    for case in items(&file, "decode") {
        let expected = case["hex"].as_str().map(str::to_owned);
        assert_eq!(base32::decode(text(&case["input"])).map(|b| hex(&b)), expected, "{case}");
    }
}

#[test]
fn totp_matches_mac() {
    let file = golden("totp");
    let cases = file.as_array().unwrap();
    assert!(cases.len() > 500, "unexpected number of cases in totp.json: {}", cases.len());
    for case in cases {
        let date = UNIX_EPOCH + Duration::from_secs_f64(case["time"].as_f64().unwrap());
        let (digits, period) = (number(&case["digits"]), number(&case["period"]));
        let code = totp::code(text(&case["secret"]), algorithm(&case["algorithm"]), digits, period, date);
        assert_eq!(code, text(&case["code"]), "{case}");
        assert_eq!(totp::formatted(&code), text(&case["formatted"]), "{case}");
        assert_eq!(totp::seconds_remaining(period, date), number(&case["secondsRemaining"]), "{case}");
    }
}

#[test]
fn canonical_matches_mac() {
    let file = golden("canonical");
    let p = placeholders(&file);
    for case in items(&file, "cleanText") {
        assert_eq!(canonical::clean_text(text(&case["input"])), text(&case["output"]), "{case}");
    }
    for case in items(&file, "standardizedSecret") {
        assert_eq!(canonical::standardized_secret(text(&case["input"])).as_deref(), case["output"].as_str(), "{case}");
    }
    for case in items(&file, "stripIssuerPrefix") {
        let output = canonical::stripping_issuer_prefix(text(&case["username"]), text(&case["issuer"]));
        assert_eq!(output, text(&case["output"]), "{case}");
    }
    for case in items(&file, "fields") {
        let period = signed(&case["period"]).max(0) as u32;
        let result = canonical::fields(
            text(&case["issuer"]),
            text(&case["username"]),
            text(&case["secret"]),
            algorithm(&case["algorithm"]),
            number(&case["digits"]),
            period,
            &p,
        );
        let actual = result.map(|f| {
            json!({"issuer": f.issuer, "username": f.username, "secret": f.secret,
                   "algorithm": f.algorithm.raw_value(), "digits": f.digits, "period": f.period})
        });
        assert_eq!(actual.unwrap_or(Value::Null), case["output"], "{case}");
    }
    for case in items(&file, "dedupKey") {
        let key = dedup::deduplication_key(
            text(&case["issuer"]),
            text(&case["username"]),
            text(&case["secret"]),
            algorithm(&case["algorithm"]),
            number(&case["digits"]),
            number(&case["period"]),
            &p,
        );
        assert_eq!(key, text(&case["output"]), "{case}");
    }
    for case in items(&file, "otpauthURL") {
        let account = Account {
            id: Uuid::nil(),
            issuer: text(&case["issuer"]).into(),
            username: text(&case["username"]).into(),
            secret: text(&case["secret"]).into(),
            algorithm: algorithm(&case["algorithm"]),
            digits: number(&case["digits"]),
            period: number(&case["period"]),
            created_at: UNIX_EPOCH,
            added_by: provenance::MANUAL.into(),
            website: None,
            icon: None,
            deleted_at: None,
        };
        assert_eq!(canonical::otpauth_url(&account, &p), text(&case["url"]), "{case}");
    }
}

#[test]
fn otpauth_matches_mac() {
    let file = golden("otpauth");
    let p = placeholders(&file);
    for case in items(&file, "scan") {
        let result = otpauth::parse_scan_result(text(&case["input"]), &p);
        let actual = json!({
            "input": case["input"],
            "isGoogleMigration": result.is_google_migration,
            "batchKey": result.batch_key(),
            "accounts": result.accounts.iter().map(parsed_json).collect::<Vec<_>>(),
        });
        assert_eq!(actual, *case);
    }
    for case in items(&file, "single") {
        assert_eq!(parsed_json(&otpauth::parse(text(&case["input"]), &p)), case["account"], "{}", case["input"]);
    }
    for case in items(&file, "lines") {
        let actual: Vec<Value> = otpauth::parse_lines(text(&case["input"]), &p).iter().map(parsed_json).collect();
        assert_eq!(Value::Array(actual), case["accounts"], "{}", case["input"]);
    }
}

#[test]
fn search_matches_mac() {
    let file = golden("search");
    for case in items(&file, "matches") {
        let actual = search::matches_search(text(&case["issuer"]), text(&case["username"]), text(&case["query"]));
        assert_eq!(actual, case["expected"].as_bool().unwrap(), "{case}");
    }
    for case in items(&file, "mask") {
        assert_eq!(search::mask_username(text(&case["input"])), text(&case["output"]), "{case}");
    }
    for case in items(&file, "monogram") {
        assert_eq!(search::monogram(text(&case["input"])), text(&case["output"]), "{case}");
    }
    for case in items(&file, "palette") {
        assert_eq!(i64::from(search::palette_index(text(&case["issuer"]))), signed(&case["index"]), "{case}");
    }
}

#[test]
fn password_matches_mac() {
    let file = golden("password");
    for case in items(&file, "strength") {
        assert_eq!(password::evaluate(text(&case["input"])) as i64, signed(&case["score"]), "{case}");
    }
    for case in items(&file, "filter") {
        assert_eq!(password::filter_password_input(text(&case["input"])), text(&case["output"]), "{case}");
    }
    for case in items(&file, "throttle") {
        assert_eq!(password::lockout_seconds(number(&case["failures"])), case["seconds"].as_f64(), "{case}");
    }
}

#[test]
fn kdf_matches_mac() {
    let file = golden("kdf");
    for case in items(&file, "argon2id") {
        let hash = vault::argon2id_raw(
            text(&case["password"]).as_bytes(),
            &unhex(&case["saltHex"]),
            number(&case["timeCost"]),
            number(&case["memoryKiB"]),
            number(&case["parallelism"]),
            32,
        )
        .unwrap();
        assert_eq!(hex(&hash), text(&case["hashHex"]), "{case}");
    }
    for case in items(&file, "authenticatorPossibleHash") {
        let hash = authenticator::possible_hash(text(&case["password"]), text(&case["salt"])).unwrap();
        assert_eq!(hash, text(&case["hash"]), "{case}");
    }
    for case in items(&file, "phcVerify") {
        let actual = authenticator::verify_phc(text(&case["value"]), text(&case["encoded"]));
        assert_eq!(actual, case["expected"].as_bool().unwrap(), "{case}");
    }
    for case in items(&file, "cryptojs") {
        let plaintext = authenticator::cryptojs_decrypt(text(&case["ciphertext"]), text(&case["passphrase"]));
        assert_eq!(plaintext.as_deref(), Some(text(&case["plaintext"])), "{case}");
    }
}

#[test]
fn opens_mac_vaults_and_exports() {
    let file = golden("vault");
    for sample in items(&file, "vaults") {
        let data = text(&sample["file"]).as_bytes();
        let unlocked = vault::unlock(data, text(&sample["password"])).unwrap_or_else(|e| panic!("{}: {e}", sample["name"]));
        let accounts: Vec<Value> = unlocked.contents.accounts.iter().map(account_json).collect();
        assert_eq!(Value::Array(accounts), sample["accounts"], "{}", sample["name"]);
        // Fixtures without `categories` read as having no categories.
        assert!(unlocked.contents.categories.is_empty(), "{}", sample["name"]);
        assert!(unlocked.contents.pinned_ids.is_empty(), "{}", sample["name"]);
        assert_eq!(vault::unlock(data, "wrong password").unwrap_err(), vault::VaultError::InvalidPassword);
        // Reseal with the same key, then unlock with the master password: the contents are unchanged.
        let resealed = vault::reseal(data, &unlocked.key, &unlocked.contents, SystemTime::now()).unwrap();
        let reopened = vault::unlock(&resealed, text(&sample["password"])).unwrap().contents;
        assert_eq!(reopened, unlocked.contents, "{}", sample["name"]);
    }
    for sample in items(&file, "exports") {
        let imported = vault::import_export(text(&sample["file"]).as_bytes(), text(&sample["password"])).unwrap();
        assert_eq!(Value::Array(imported.accounts.iter().map(account_json).collect()), sample["accounts"], "{}", sample["name"]);
        assert!(imported.categories.is_empty(), "{}", sample["name"]);
        assert!(imported.pinned_ids.is_empty(), "{}", sample["name"]);
    }
}

/// Sample written by Rust: both the vault and the export open, the accounts match, and a wrong password reports `InvalidPassword`. The file itself is Rust output, so it
/// also pins the write format of that time.
#[test]
fn opens_rust_written_vault_and_export() {
    let file = golden("rust-written");
    let sample = &file["vault"];
    let data = text(&sample["file"]).as_bytes();
    let unlocked = vault::unlock(data, text(&sample["password"])).unwrap();
    assert_eq!(Value::Array(unlocked.contents.accounts.iter().map(account_json).collect()), sample["accounts"]);
    assert!(unlocked.contents.categories.is_empty(), "the reverse sample has no categories");
    assert!(unlocked.contents.pinned_ids.is_empty());
    assert_eq!(unlocked.info.kdf, vault::KDF);
    assert_eq!(vault::unlock(data, "wrong password").unwrap_err(), vault::VaultError::InvalidPassword);
    assert_eq!(vault::open(data, &unlocked.key).unwrap(), unlocked.contents);

    let sample = &file["export"];
    let data = text(&sample["file"]).as_bytes();
    assert!(vault::is_safay_export(data));
    let imported = vault::import_export(data, text(&sample["password"])).unwrap();
    assert_eq!(Value::Array(imported.accounts.iter().map(account_json).collect()), sample["accounts"]);
    assert!(imported.categories.is_empty());
    assert_eq!(vault::import_export(data, "wrong password").unwrap_err(), vault::VaultError::InvalidPassword);
}

/// Category format: vaults and exports written by Rust read back with categories, order, and membership intact.
#[test]
fn opens_category_vaults_and_exports() {
    let file = golden("categories");
    let categories_json = |contents: &vault::VaultContents| {
        json!(
            contents
                .categories
                .iter()
                .map(|c| json!({
                    "id": c.id.to_string().to_uppercase(),
                    "name": c.name,
                    "accountIDs": c.account_ids.iter().map(|id| id.to_string().to_uppercase()).collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>()
        )
    };
    let ids_json =
        |contents: &vault::VaultContents| json!(contents.accounts.iter().map(|a| a.id.to_string().to_uppercase()).collect::<Vec<_>>());
    let sample = &file["vault"];
    let contents = vault::unlock(text(&sample["file"]).as_bytes(), text(&sample["password"])).unwrap().contents;
    assert_eq!(ids_json(&contents), sample["accountIDs"]);
    assert_eq!(categories_json(&contents), sample["categories"]);
    assert!(contents.pinned_ids.is_empty());
    let sample = &file["export"];
    let imported = vault::import_export(text(&sample["file"]).as_bytes(), text(&sample["password"])).unwrap();
    assert_eq!(ids_json(&imported), sample["accountIDs"]);
    assert_eq!(categories_json(&imported), sample["categories"]);
}

#[test]
fn authenticator_imports_match_mac() {
    let file = golden("authenticator");
    let p = Placeholders { unknown_issuer: "Unknown service".into(), empty_username: "No account".into() };
    for sample in items(&file, "imports") {
        let outcome = match authenticator::import_authenticator_contents(
            text(&sample["file"]).as_bytes(),
            text(&sample["password"]),
            SystemTime::now(),
            &p,
        ) {
            Ok(contents) => json!({"accounts": contents.accounts.iter().map(|account| {
                let mut value = imported_json(account);
                value["pinned"] = contents.pinned_ids.contains(&account.id).into();
                value
            }).collect::<Vec<_>>()}),
            Err(error) => json!({"error": error.l10n_key()}),
        };
        assert_eq!(outcome, sample["expected"], "{}", sample["name"]);
    }
}

/// Imports from other apps: the detected format, the accounts with their pinned flag, categories and website, the
/// skipped count, or the error. Encrypted samples were written with independent implementations (golden/README.md).
#[test]
fn imports_match_golden() {
    use safay_core::import::{self, Format};

    let file = golden("imports");
    let p = Placeholders {
        unknown_issuer: text(&file["placeholders"]["unknownIssuer"]).into(),
        empty_username: text(&file["placeholders"]["emptyUsername"]).into(),
    };
    for sample in items(&file, "imports") {
        let data = text(&sample["file"]).as_bytes();
        let detected = import::detect(data).map(|(format, encrypted)| {
            let format = match format {
                Format::SafayExport => "safay",
                Format::Authenticator => "authenticator",
                Format::Aegis => "aegis",
                Format::TwoFas => "twofas",
                Format::Ente => "ente",
                Format::Proton => "proton",
                Format::Bitwarden => "bitwarden",
                Format::Csv => "csv",
                Format::Text => "text",
            };
            json!({"format": format, "encrypted": encrypted})
        });
        assert_eq!(detected.unwrap_or(Value::Null), sample["detect"], "{}", sample["name"]);
        let outcome = match import::import_file(data, text(&sample["password"]), SystemTime::now(), &p) {
            Ok(imported) => {
                let contents = &imported.contents;
                json!({
                    "accounts": contents.accounts.iter().map(|account| {
                        let mut value = imported_json(account);
                        value["pinned"] = contents.pinned_ids.contains(&account.id).into();
                        value["categories"] = contents.categories.iter()
                            .filter(|c| c.account_ids.contains(&account.id))
                            .map(|c| c.name.clone())
                            .collect::<Vec<_>>()
                            .into();
                        value["website"] = account.website.clone().into();
                        value
                    }).collect::<Vec<_>>(),
                    "skipped": imported.skipped,
                })
            }
            Err(error) => json!({"error": error.l10n_key()}),
        };
        assert_eq!(outcome, sample["expected"], "{}", sample["name"]);
    }
}

/// Websites and site icons: normalization, suggestions, icon validation, icon link ranking, fetch steps, and a vault and
/// export written by Rust with `website` and `icon`.
#[test]
fn site_icons_match_golden() {
    use base64::Engine;
    use safay_core::site_icon::{self, SiteIconFetch, Step};

    let file = golden("site-icons");
    let base64 = |value: &Value| base64::engine::general_purpose::STANDARD.decode(text(value)).unwrap();
    for case in items(&file, "websites") {
        assert_eq!(site_icon::normalized_website(text(&case["input"])).as_deref(), case["expected"].as_str(), "{case}");
    }
    for case in items(&file, "suggestions") {
        assert_eq!(site_icon::suggested_website(text(&case["issuer"])).as_deref(), case["expected"].as_str(), "{case}");
    }
    for case in items(&file, "icons") {
        assert_eq!(site_icon::is_valid_icon(&base64(&case["base64"])), case["valid"].as_bool().unwrap(), "{}", case["name"]);
    }
    for case in items(&file, "links") {
        let base = url::Url::parse(text(&case["base"])).unwrap();
        let links: Vec<String> = site_icon::icon_links(text(&case["html"]), &base).iter().map(ToString::to_string).collect();
        assert_eq!(json!(links), case["expected"]);
    }
    for case in items(&file, "tiles") {
        let rows: Vec<&str> = items(case, "rows").iter().map(text).collect();
        let alpha: Vec<u8> = rows
            .iter()
            .flat_map(|row| {
                row.chars().map(|c| match c {
                    '#' => 255,
                    '+' => 128,
                    _ => 0,
                })
            })
            .collect();
        let bounds = site_icon::tile_bounds(&alpha, rows[0].len() as u32, rows.len() as u32)
            .map(|b| json!({ "x": b.x, "y": b.y, "width": b.width, "height": b.height }));
        assert_eq!(bounds.unwrap_or(Value::Null), case["expected"], "{}", case["name"]);
    }
    let step_json = |step: Step| match step {
        Step::Fetch(request) => json!({ "fetch": request.url, "maxBytes": request.max_bytes }),
        Step::Decode => json!("decode"),
        Step::Finished => json!("finished"),
    };
    for case in items(&file, "fetches") {
        let mut fetch: Option<SiteIconFetch> = None;
        for step in items(case, "steps") {
            let actual = if step["start"].as_bool() == Some(true) {
                match SiteIconFetch::start(text(&case["website"])) {
                    Some((started, request)) => {
                        fetch = Some(started);
                        step_json(Step::Fetch(request))
                    }
                    None => json!("invalid"),
                }
            } else {
                let fetch = fetch.as_mut().unwrap();
                match step["event"].as_str() {
                    Some("decodeFailed") => step_json(fetch.decode_failed()),
                    Some("transportFailed") => step_json(fetch.transport_failed()),
                    _ => {
                        let response = &step["response"];
                        let body = match (response["text"].as_str(), response.get("base64")) {
                            (Some(body), _) => body.as_bytes().to_vec(),
                            (None, Some(encoded)) => base64(encoded),
                            _ => Vec::new(),
                        };
                        step_json(fetch.handle_response(response["status"].as_u64().unwrap() as u16, response["location"].as_str(), &body))
                    }
                }
            };
            assert_eq!(actual, step["expect"], "{} {step}", case["website"]);
        }
    }

    let accounts_json = |accounts: &[Account]| {
        Value::Array(
            accounts
                .iter()
                .map(|account| {
                    let mut value = account_json(account);
                    let object = value.as_object_mut().unwrap();
                    object.insert("website".into(), json!(account.website));
                    object.insert(
                        "icon".into(),
                        json!(account.icon.as_ref().map(|icon| base64::engine::general_purpose::STANDARD.encode(icon))),
                    );
                    value
                })
                .collect(),
        )
    };
    let sample = &file["vault"];
    let unlocked = vault::unlock(text(&sample["file"]).as_bytes(), text(&sample["password"])).unwrap();
    assert_eq!(accounts_json(&unlocked.contents.accounts), sample["accounts"]);
    let sample = &file["export"];
    let imported = vault::import_export(text(&sample["file"]).as_bytes(), text(&sample["password"])).unwrap();
    assert_eq!(accounts_json(&imported.accounts), sample["accounts"]);
    let candidates = dedup::candidates_from_import(&imported);
    let inserted =
        dedup::insert_accounts(&[], &[], &candidates, provenance::IMPORTED, UNIX_EPOCH, &placeholders(&golden("canonical"))).unwrap();
    let kept: Vec<(Option<&str>, bool)> =
        inserted.accounts.iter().map(|account| (account.website.as_deref(), account.icon.is_some())).collect();
    assert_eq!(kept, [(Some("github.com"), true), (Some("sso.example.org"), false), (None, false)], "imports keep website and icon");
}
