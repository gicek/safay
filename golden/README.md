# Golden Vectors

Safay's behavior baseline. `crates/safay-core/tests/golden.rs` compares every entry.

| File | Contents | Kind |
|---|---|---|
| `base32.json` | Encoding and lenient decoding | Deterministic |
| `totp.json` | 3 algorithms × 4 digit/period pairs × 8 timestamps × 7 secrets (including the RFC 6238 vectors and invalid secrets) | Deterministic |
| `canonical.json` | Text cleanup, secret normalization, `issuer:` prefixes, field normalization, dedup keys, otpauth URI encoding | Deterministic |
| `otpauth.json` | Parsing single entries, scan results, and multi-line text, including Google migration codes and every failure reason | Deterministic |
| `search.json` | Search matching, account name masking, initials, color indexes | Deterministic |
| `password.json` | Password strength, unlock delay seconds | Deterministic |
| `kdf.json` | Argon2id, Authenticator password hashes, PHC verification, CryptoJS decryption | Deterministic |
| `vault.json` | v2 and empty vault fixtures, a v2 encrypted export, and their expected decrypted contents | Random sample |
| `authenticator.json` | Expected results for Authenticator plaintext and v3 encrypted backups and their errors | Random sample |
| `rust-written.json` | Vaults and exports written by Rust | Random sample |
| `categories.json` | Category samples written by Rust: multiple memberships, order within a category, and empty categories (exports omit empty categories) | Random sample |
| `imports.json` | Imports from Aegis, 2FAS, Ente Auth, Proton Authenticator, Bitwarden, CSV, and text files: detected format, accounts with pinned flag, categories and website, skipped count, and errors. Encrypted samples were written with PyNaCl, cryptography, and argon2-cffi | Random sample |
| `site-icons.json` | Website normalization and suggestion, stored icon validation, icon link ranking, fetch steps, tile or glyph badge layout (alpha maps as `#` / `+` / `.` rows); plus a vault and export written by Rust with `website` and `icon` | Deterministic, plus random samples |

- Deterministic vectors: each platform recomputes them and compares every field with the file. Random samples: each platform only checks that they decrypt and match the expected contents.
- Samples without `categories` read as having no categories. Expected usernames are the raw values from the file.
- Placeholder text (unknown service, no accounts) uses the UI language at generation time (English) and is stored in each file's `placeholders` field.
- **Never regenerate**: the files are fixed fixtures, not outputs of the current code. To change behavior on purpose, update the implementation and the matching entries in the same commit and explain why in the commit message. Never edit them to make a test pass.
