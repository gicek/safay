//! CSV exports from password managers that include one-time passwords: Apple Passwords (`OTPAuth`), Bitwarden
//! (`login_totp`), and others with an `OTPAuth`, `totp`, or `otp` column.
//!
//! The first row is the header; the one-time password column holds an `otpauth://` link or a bare Base32 secret. Rows
//! without one are passwords only and are ignored. Only the columns Safay needs are kept, and every field read is wiped
//! once its row is done (these files also hold passwords).

use zeroize::Zeroizing;

use super::{Collector, Extras, ImportError};

const OTP_COLUMNS: &[&str] = &["otpauth", "login_totp", "totp", "otp"];
const NAME_COLUMNS: &[&str] = &["title", "name"];
const USERNAME_COLUMNS: &[&str] = &["username", "login_username", "email"];
const URL_COLUMNS: &[&str] = &["url", "login_uri", "website"];
const FAVORITE_COLUMNS: &[&str] = &["favorite"];
const FOLDER_COLUMNS: &[&str] = &["folder"];

type Record = Vec<Zeroizing<String>>;

/// RFC 4180 records: comma-separated, fields optionally in double quotes (`""` escapes a quote, quoted fields may span
/// lines), CRLF or LF line ends.
pub(super) struct Records<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl Iterator for Records<'_> {
    type Item = Result<Record, ImportError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.chars.peek()?;
        let mut record: Record = Vec::new();
        let mut field = Zeroizing::new(String::new());
        let mut quoted = false;
        let mut at_start = true;
        loop {
            let Some(c) = self.chars.next() else {
                if quoted {
                    return Some(Err(ImportError::InvalidFile));
                }
                record.push(field);
                return Some(Ok(record));
            };
            if quoted {
                if c == '"' {
                    if self.chars.peek() == Some(&'"') {
                        self.chars.next();
                        field.push('"');
                    } else {
                        quoted = false;
                    }
                } else {
                    field.push(c);
                }
                continue;
            }
            match c {
                '"' if at_start => quoted = true,
                ',' => {
                    record.push(std::mem::replace(&mut field, Zeroizing::new(String::new())));
                    at_start = true;
                    continue;
                }
                '\r' if self.chars.peek() == Some(&'\n') => {}
                '\n' => {
                    record.push(field);
                    return Some(Ok(record));
                }
                c => field.push(c),
            }
            at_start = false;
        }
    }
}

pub(super) fn records(text: &str) -> Records<'_> {
    Records { chars: text.chars().peekable() }
}

fn column(header: &[Zeroizing<String>], names: &[&str]) -> Option<usize> {
    names.iter().find_map(|name| header.iter().position(|field| field.trim().eq_ignore_ascii_case(name)))
}

/// The first row is a header with a one-time password column.
pub(super) fn is_csv(text: &str) -> bool {
    match records(text).next() {
        Some(Ok(header)) => header.len() >= 2 && column(&header, OTP_COLUMNS).is_some(),
        _ => false,
    }
}

pub(super) fn import(text: &str, collector: &mut Collector<'_>) -> Result<(), ImportError> {
    let mut rows = records(text);
    let header = rows.next().ok_or(ImportError::InvalidFile)??;
    let otp = column(&header, OTP_COLUMNS).ok_or(ImportError::InvalidFile)?;
    let (name, username, url) = (column(&header, NAME_COLUMNS), column(&header, USERNAME_COLUMNS), column(&header, URL_COLUMNS));
    let (favorite, folder) = (column(&header, FAVORITE_COLUMNS), column(&header, FOLDER_COLUMNS));
    for row in rows {
        let row = row?;
        let field = |index: Option<usize>| index.and_then(|i| row.get(i)).map(|value| value.as_str()).unwrap_or("");
        let link = field(Some(otp));
        if link.trim().is_empty() {
            continue;
        }
        let extras = Extras {
            groups: Some(field(folder)).filter(|f| !f.trim().is_empty()).map(|f| vec![f.to_owned()]).unwrap_or_default(),
            pinned: matches!(field(favorite).trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"),
            website: Some(field(url)).filter(|u| !u.trim().is_empty()).map(str::to_owned),
        };
        collector.add_link(link, field(name), field(username), extras)?;
    }
    Ok(())
}
