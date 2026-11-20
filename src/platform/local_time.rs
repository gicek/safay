//! Wall-clock time on this device: every date and time the UI shows, and the default name of an export file.
//!
//! The system gives the offset from UTC at the instant in question (so a date before a daylight-saving change shows as
//! it did then); the calendar fields are computed here, without a time-zone database of Safay's own.
use std::time::{SystemTime, UNIX_EPOCH};

/// Year, month, day, hour and minute of an instant, as the device's clock shows it.
pub fn fields(at: SystemTime) -> (i64, i64, i64, i64, i64) {
    let secs = at.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    fields_at(secs, utc_offset(secs))
}

/// `yyyy-MM-dd HH:mm` on this device.
pub fn date_time(at: SystemTime) -> String {
    let (y, m, d, hour, minute) = fields(at);
    format!("{y:04}-{m:02}-{d:02} {hour:02}:{minute:02}")
}

/// The calendar fields `offset` seconds east of UTC.
pub fn fields_at(unix_secs: i64, offset: i64) -> (i64, i64, i64, i64, i64) {
    let secs = unix_secs + offset;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d, rem / 3600, rem % 3600 / 60)
}

/// Seconds east of UTC on this device at an instant; UTC when the system cannot tell.
#[cfg(unix)]
fn utc_offset(unix_secs: i64) -> i64 {
    let time = unix_secs as libc::time_t;
    // SAFETY: `localtime_r` writes only the `tm` it is given and reads the time-zone settings of the process.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let filled = unsafe { libc::localtime_r(&raw const time, &raw mut tm) };
    if filled.is_null() { 0 } else { i64::from(tm.tm_gmtoff as i32) }
}

#[cfg(windows)]
fn utc_offset(unix_secs: i64) -> i64 {
    time::OffsetDateTime::from_unix_timestamp(unix_secs)
        .ok()
        .and_then(|at| time::UtcOffset::local_offset_at(at).ok())
        .map_or(0, |offset| i64::from(offset.whole_seconds()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_follow_the_offset_across_days_and_years() {
        // 2026-11-05 01:01 UTC.
        let at = 1_793_840_460;
        assert_eq!(fields_at(at, 0), (2026, 11, 5, 1, 1));
        assert_eq!(fields_at(at, 8 * 3600), (2026, 11, 5, 9, 1));
        assert_eq!(fields_at(at, -5 * 3600), (2026, 11, 4, 20, 1));
        assert_eq!(fields_at(at, 5 * 3600 + 45 * 60), (2026, 11, 5, 6, 46));
        // New Year's Eve in UTC is already the new year further east, and a leap day stays one.
        assert_eq!(fields_at(1_798_761_540, 9 * 3600), (2027, 1, 1, 8, 59));
        assert_eq!(fields_at(1_709_208_000, 0), (2024, 2, 29, 12, 0));
        assert_eq!(fields_at(0, -3600), (1969, 12, 31, 23, 0));
    }

    #[test]
    fn the_device_offset_is_a_real_one() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let offset = utc_offset(now);
        assert!((-12 * 3600..=14 * 3600).contains(&offset) && offset % 60 == 0, "{offset}");
        let (y, m, d, hour, minute) = fields(UNIX_EPOCH + std::time::Duration::from_secs(now as u64));
        assert_eq!((y, m, d, hour, minute), fields_at(now, offset));
        assert_eq!(date_time(UNIX_EPOCH).len(), "1970-01-01 00:00".len());
    }
}
