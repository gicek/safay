//! Writes a Safay encrypted export of made-up accounts, for trying imports of many accounts by hand or in UI tests
//! (`scripts/android_ui_test.py`).
//!
//!     cargo run -p safay-core --example export_fixture -- <count> <export password> <output file>
//!
//! Account `n` (from 1) is "<service> <nnn>", `user<nnn>@example.com`, with a random secret; every seventh has 8
//! digits and every eleventh a 60-second period. They are in three categories by `n % 3`: 1 "Work", 2 "Personal",
//! 0 "Finance".
use safay_core::{Account, OtpType, TotpAlgorithm, base32, category::Category, model::provenance, vault};
use std::time::{Duration, SystemTime};

const SERVICES: [&str; 20] = [
    "GitHub",
    "Google",
    "Dropbox",
    "AWS",
    "Microsoft",
    "Slack",
    "Stripe",
    "Cloudflare",
    "GitLab",
    "Discord",
    "Twitter",
    "Reddit",
    "Proton",
    "Fastmail",
    "Linear",
    "Notion",
    "Figma",
    "Vercel",
    "Heroku",
    "DigitalOcean",
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [count, password, out] = args.as_slice() else {
        eprintln!("usage: export_fixture <count> <export password> <output file>");
        std::process::exit(2);
    };
    let count: usize = count.parse().expect("a number of accounts");
    let now = SystemTime::now();
    let accounts: Vec<Account> = (1..=count)
        .map(|n| {
            let mut key = [0u8; 20];
            getrandom::fill(&mut key).expect("random bytes");
            Account {
                id: uuid::Uuid::new_v4(),
                issuer: format!("{} {n:03}", SERVICES[(n - 1) % SERVICES.len()]),
                username: format!("user{n:03}@example.com"),
                secret: base32::encode(&key),
                otp_type: OtpType::Totp,
                algorithm: TotpAlgorithm::Sha1,
                digits: if n % 7 == 1 { 8 } else { 6 },
                period: if n % 11 == 1 { 60 } else { 30 },
                // Oldest first, as if added one a minute.
                created_at: now - Duration::from_secs((count - n + 1) as u64 * 60),
                added_by: provenance::MANUAL.to_owned(),
                website: None,
                icon: None,
                deleted_at: None,
            }
        })
        .collect();
    let categories: Vec<Category> = [(1, "Work"), (2, "Personal"), (0, "Finance")]
        .into_iter()
        .map(|(rest, name)| Category {
            id: uuid::Uuid::new_v4(),
            name: name.to_owned(),
            account_ids: accounts.iter().enumerate().filter(|(i, _)| (i + 1) % 3 == rest).map(|(_, a)| a.id).collect(),
        })
        .collect();
    let data = vault::export(&accounts, &categories, password, now).expect("export");
    std::fs::write(out, data).expect("write the export file");
    println!("{out}: {count} accounts");
}
