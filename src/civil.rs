//! The civil calendar: seconds since the Unix epoch to a UTC date and time
//! of day in the proleptic Gregorian calendar, and back.
//!
//! By the era arithmetic every calendar library uses (Howard Hinnant's
//! `days_from_civil` and `civil_from_days`): four-hundred-year eras of
//! 146 097 days, the year within the era, the day within a year that
//! starts on the first of March. Until 2026-09-24 the SAML and Kerberos
//! readers, AS4, HTTP, syslog and the archive each carried one direction
//! of it, and HTTP and the location authorizer each their own weekday.
//! How a moment is written as text — RFC 1123, `x-amz-date`, a SAML
//! `dateTime` — stays with the protocol; RFC 3339, which four of them
//! write, is here.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds in a civil day; a leap second is not one of them.
pub const SECONDS_A_DAY: i64 = 86_400;

/// A moment in UTC, to the second.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CivilTime {
    /// The year; 0 is 1 BC.
    pub year: i64,
    /// 1 to 12.
    pub month: u32,
    /// 1 to the month's length.
    pub day: u32,
    /// 0 to 23.
    pub hour: u32,
    /// 0 to 59.
    pub minute: u32,
    /// 0 to 59, or 60 for a leap second.
    pub second: u32,
}

impl CivilTime {
    /// The moment these fields name, or `None` where one is out of its
    /// range: the thirtieth of February, the twenty-fourth hour.
    #[must_use]
    pub fn new(
        year: i64,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
    ) -> Option<Self> {
        let valid = (1..=12).contains(&month)
            && day >= 1
            && day <= days_in_month(year, month)
            && hour < 24
            && minute < 60
            && second <= 60;
        valid.then_some(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
        })
    }

    /// The moment `seconds` after the epoch.
    #[must_use]
    pub fn from_unix(seconds: i64) -> Self {
        let (year, month, day) = civil_from_days(seconds.div_euclid(SECONDS_A_DAY));
        let of_day = seconds.rem_euclid(SECONDS_A_DAY);
        let part = |value: i64| u32::try_from(value).unwrap_or(0);
        Self {
            year,
            month,
            day,
            hour: part(of_day / 3_600),
            minute: part(of_day % 3_600 / 60),
            second: part(of_day % 60),
        }
    }

    /// The moment `at`, to the whole second. A moment before the epoch,
    /// which no clock this runs under reports, is the epoch itself.
    #[must_use]
    pub fn from_system_time(at: SystemTime) -> Self {
        Self::from_unix(unix_seconds(at))
    }

    /// Now, to the whole second.
    #[must_use]
    pub fn now() -> Self {
        Self::from_system_time(SystemTime::now())
    }

    /// Seconds since the epoch; a leap second is the second after it.
    #[must_use]
    pub fn unix(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day) * SECONDS_A_DAY
            + i64::from(self.hour) * 3_600
            + i64::from(self.minute) * 60
            + i64::from(self.second)
    }

    /// The day of the week, 0 Monday to 6 Sunday, as ISO 8601 orders them.
    #[must_use]
    pub fn weekday(&self) -> u32 {
        weekday(days_from_civil(self.year, self.month, self.day))
    }

    /// RFC 3339 in UTC to the whole second: `2026-09-10T12:00:00Z`.
    #[must_use]
    pub fn rfc3339(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// Whole seconds from the epoch to `at`, zero for a moment before it.
#[must_use]
pub fn unix_seconds(at: SystemTime) -> i64 {
    at.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or(0)
}

/// Days from 1970-01-01 to the civil date `year`-`month`-`day`, negative
/// before it. The date is taken as given; [`CivilTime::new`] checks one.
#[must_use]
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let (month, day) = (i64::from(month), i64::from(day));
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The civil date `days` after 1970-01-01: year, month 1 to 12, day 1 to 31.
#[must_use]
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let part = |value: i64| u32::try_from(value).unwrap_or(0);
    (year, part(month), part(day))
}

/// The day of the week `days` after 1970-01-01, a Thursday: 0 Monday to 6
/// Sunday.
#[must_use]
pub fn weekday(days: i64) -> u32 {
    u32::try_from((days + 3).rem_euclid(7)).unwrap_or(0)
}

/// Days in `month` of `year`.
#[must_use]
pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(year: i64, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> CivilTime {
        CivilTime::new(year, month, day, hour, minute, second).expect("a real moment")
    }

    #[test]
    fn known_moments_are_their_seconds_since_the_epoch_both_ways() {
        let known = [
            (at(1970, 1, 1, 0, 0, 0), 0),
            (at(1994, 11, 6, 8, 49, 37), 784_111_777),
            (at(2000, 2, 29, 1, 1, 1), 951_782_400 + 3_661),
            (at(2020, 9, 13, 12, 26, 40), 1_600_000_000),
            (at(2024, 2, 29, 0, 0, 0), 1_709_164_800),
            (at(2027, 1, 15, 8, 0, 0), 1_800_000_000),
            (at(1969, 12, 31, 23, 59, 59), -1),
            (at(1900, 3, 1, 0, 0, 0), -2_203_891_200),
        ];
        for (moment, seconds) in known {
            assert_eq!(moment.unix(), seconds, "{moment:?}");
            assert_eq!(CivilTime::from_unix(seconds), moment, "{seconds}");
        }
    }

    #[test]
    fn every_day_for_eight_centuries_reads_back_as_itself() {
        let first = days_from_civil(1600, 1, 1);
        let last = days_from_civil(2400, 12, 31);
        let mut previous = civil_from_days(first - 1);
        for days in first..=last {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days);
            assert!(day <= days_in_month(year, month));
            assert!((year, month, day) > previous);
            previous = (year, month, day);
        }
    }

    #[test]
    fn a_field_out_of_its_range_is_no_moment() {
        assert!(CivilTime::new(2027, 13, 15, 8, 0, 0).is_none());
        assert!(CivilTime::new(2027, 2, 29, 8, 0, 0).is_none());
        assert!(CivilTime::new(2028, 2, 29, 8, 0, 0).is_some());
        assert!(CivilTime::new(1900, 2, 29, 0, 0, 0).is_none());
        assert!(CivilTime::new(2000, 2, 29, 0, 0, 0).is_some());
        assert!(CivilTime::new(2027, 4, 31, 0, 0, 0).is_none());
        assert!(CivilTime::new(2027, 1, 0, 0, 0, 0).is_none());
        assert!(CivilTime::new(2027, 1, 1, 24, 0, 0).is_none());
        assert!(CivilTime::new(2027, 1, 1, 0, 60, 0).is_none());
        assert!(CivilTime::new(2016, 12, 31, 23, 59, 60).is_some());
        assert!(CivilTime::new(2016, 12, 31, 23, 59, 61).is_none());
    }

    #[test]
    fn the_epoch_was_a_thursday_and_rfc_3339_is_written_whole_seconds() {
        assert_eq!(weekday(0), 3);
        assert_eq!(at(1994, 11, 6, 8, 49, 37).weekday(), 6, "a Sunday");
        assert_eq!(at(2026, 9, 24, 0, 0, 0).weekday(), 3, "a Thursday");
        assert_eq!(weekday(-1), 2);
        assert_eq!(at(2026, 9, 10, 12, 0, 0).rfc3339(), "2026-09-10T12:00:00Z");
        let before = UNIX_EPOCH
            .checked_sub(Duration::from_secs(5))
            .expect("earlier");
        assert_eq!(CivilTime::from_system_time(before), at(1970, 1, 1, 0, 0, 0));
        assert!(CivilTime::now().year >= 2026);
    }
}
