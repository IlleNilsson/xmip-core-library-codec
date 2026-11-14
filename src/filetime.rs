//! Windows' `FILETIME`: hundred-nanosecond ticks since 1601-01-01 in UTC,
//! the count SMB's times, NTLM's timestamp and OPC UA's `DateTime` keep,
//! to and from nanoseconds since the Unix epoch, the unit the estate keeps
//! a moment in (`xcore::Clock`). Each of the three carried the epoch as a
//! constant of its own until 2026-11-14 (ADR-0044, amendment 2026-11-14).

/// The Unix epoch as a `FILETIME`: the ticks from 1601-01-01 to it.
pub const AT_UNIX_EPOCH: i128 = 116_444_736_000_000_000;

/// The moment `unix_nanos` after the Unix epoch as a `FILETIME`; a part of
/// a tick is cut.
#[must_use]
pub const fn from_unix_nanos(unix_nanos: i128) -> i128 {
    unix_nanos.div_euclid(100).saturating_add(AT_UNIX_EPOCH)
}

/// The `FILETIME` `ticks` as nanoseconds since the Unix epoch.
#[must_use]
pub const fn to_unix_nanos(ticks: i128) -> i128 {
    ticks.saturating_sub(AT_UNIX_EPOCH).saturating_mul(100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::civil::{CivilTime, NANOS_A_SECOND};

    #[test]
    fn a_filetime_counts_hundred_nanoseconds_from_1601_both_ways() {
        // The Unix epoch's FILETIME as Microsoft documents it, and the
        // values SMB, NTLM and OPC UA each wrote with their own constant.
        assert_eq!(from_unix_nanos(0), 116_444_736_000_000_000);
        let then = 1_800_000_000 * NANOS_A_SECOND;
        assert_eq!(from_unix_nanos(then), 134_444_736_000_000_000);
        assert_eq!(
            from_unix_nanos(then + 199),
            134_444_736_000_000_001,
            "a part cut"
        );
        assert_eq!(to_unix_nanos(134_444_736_000_000_001), then + 100);
        let first = to_unix_nanos(0).div_euclid(NANOS_A_SECOND);
        let first = CivilTime::from_unix(i64::try_from(first).expect("seconds"));
        assert_eq!(
            first,
            CivilTime::new(1601, 1, 1, 0, 0, 0).expect("a moment")
        );
    }
}
