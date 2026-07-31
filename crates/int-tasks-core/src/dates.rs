//! Calendar arithmetic on `YYYY-MM-DD` strings.
//!
//! The core deliberately carries no date library. Timezones are the caller's
//! problem — it passes in today's date and its own UTC offset — and everything
//! here is plain arithmetic, which keeps these functions deterministic under test.

/// Days since the Unix epoch for a `YYYY-MM-DD` date (Howard Hinnant's algorithm).
pub fn civil_days(date: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || parts.next().is_some() {
        return None;
    }

    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

/// The inverse: a `YYYY-MM-DD` date from days since the epoch.
pub fn civil_date(days: i64) -> String {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

/// Whole days from `from` to `to`.
pub fn days_between(from: &str, to: &str) -> Option<i64> {
    Some(civil_days(to)? - civil_days(from)?)
}

/// The local calendar date of a timestamp, given the caller's UTC offset.
///
/// Sessions are stamped in UTC milliseconds, but a streak is about which day it
/// felt like to the person working — so the offset has to come from them.
pub fn local_date(millis: u64, utc_offset_seconds: i32) -> String {
    let seconds = millis as i64 / 1000 + utc_offset_seconds as i64;
    // Floor division, so times before midnight UTC land on the previous day.
    let days = seconds.div_euclid(86_400);
    civil_date(days)
}

/// The date `n` days before the given one.
pub fn days_before(date: &str, n: i64) -> Option<String> {
    civil_days(date).map(|days| civil_date(days - n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_is_day_zero() {
        assert_eq!(civil_days("1970-01-01"), Some(0));
        assert_eq!(civil_date(0), "1970-01-01");
    }

    #[test]
    fn dates_round_trip() {
        for date in ["2026-07-31", "2000-02-29", "1999-12-31", "2024-02-29", "2100-03-01"] {
            let days = civil_days(date).expect(date);
            assert_eq!(civil_date(days), date, "round trip failed for {date}");
        }
    }

    #[test]
    fn leap_years_are_handled() {
        // 2000 is a leap year, 1900 and 2100 are not.
        assert_eq!(days_between("2000-02-28", "2000-03-01"), Some(2));
        assert_eq!(days_between("2100-02-28", "2100-03-01"), Some(1));
    }

    #[test]
    fn differences_cross_month_and_year_boundaries() {
        assert_eq!(days_between("2026-07-31", "2026-08-01"), Some(1));
        assert_eq!(days_between("2026-12-31", "2027-01-01"), Some(1));
        assert_eq!(days_between("2026-08-01", "2026-07-31"), Some(-1));
    }

    #[test]
    fn malformed_dates_are_rejected() {
        assert!(civil_days("not-a-date").is_none());
        assert!(civil_days("2026-13-01").is_none());
        assert!(civil_days("2026-07").is_none());
        assert!(civil_days("2026-07-31-01").is_none());
    }

    #[test]
    fn local_dates_respect_the_offset() {
        // 2026-07-31T23:30:00Z
        let millis = 1_785_540_600_000;
        assert_eq!(local_date(millis, 0), "2026-07-31");
        // Two hours ahead: already the next day locally.
        assert_eq!(local_date(millis, 2 * 3600), "2026-08-01");
        // Behind UTC: still the same day.
        assert_eq!(local_date(millis, -5 * 3600), "2026-07-31");
    }

    #[test]
    fn stepping_back_crosses_boundaries() {
        assert_eq!(days_before("2026-01-01", 1).as_deref(), Some("2025-12-31"));
        assert_eq!(days_before("2026-03-01", 1).as_deref(), Some("2026-02-28"));
    }
}
