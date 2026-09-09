//! UTC timestamps in RFC 3339 form, from the standard library alone.
//!
//! Records carry a creation timestamp, and the lock file records when its
//! holder started. Both need one fixed, sortable rendering. The conversion
//! from a Unix second to a civil date is short enough that a date-and-time
//! dependency would buy nothing here.

use std::time::{SystemTime, UNIX_EPOCH};

/// The current time as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// A clock before the Unix epoch is not a condition the archive design gives
/// a code for, so it renders as the epoch itself rather than failing a write.
#[must_use]
pub fn now_rfc3339() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    rfc3339_from_unix_seconds(seconds)
}

/// Render a Unix second count as an RFC 3339 UTC timestamp.
#[must_use]
pub fn rfc3339_from_unix_seconds(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let time_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second) = (
        time_of_day / 3_600,
        (time_of_day % 3_600) / 60,
        time_of_day % 60,
    );
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Convert days since 1970-01-01 into a proleptic Gregorian civil date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = u32::try_from(day_of_year - (153 * month_index + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    })
    .unwrap_or(1);
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants_render_as_rfc3339_utc() {
        assert_eq!(rfc3339_from_unix_seconds(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_from_unix_seconds(1), "1970-01-01T00:00:01Z");
        assert_eq!(
            rfc3339_from_unix_seconds(951_782_400),
            "2000-02-29T00:00:00Z"
        );
        assert_eq!(
            rfc3339_from_unix_seconds(1_768_000_000),
            "2026-01-09T23:06:40Z"
        );
    }

    #[test]
    fn the_current_time_has_the_recorded_shape() {
        let now = now_rfc3339();
        assert_eq!(now.len(), 20);
        assert!(now.ends_with('Z'));
        assert!(now.as_str() > "2020-01-01T00:00:00Z", "clock is plausible");
    }
}
