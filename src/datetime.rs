//! TOML 1.1 date and time types.
//!
//! TOML distinguishes four temporal value types, each represented by its own
//! struct here:
//!
//! | TOML type            | Rust struct          |
//! |----------------------|----------------------|
//! | `local-date`         | [`LocalDate`]        |
//! | `local-time`         | [`LocalTime`]        |
//! | `local-date-time`    | [`LocalDateTime`]    |
//! | `offset-date-time`   | [`OffsetDateTime`]   |
//!
//! When the `chrono` crate is available, bidirectional `From` conversions are
//! provided for all four types.

use std::fmt;

/// A calendar date without any time-of-day or timezone component.
///
/// Corresponds to the TOML `local-date` type (e.g. `1979-05-27`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalDate {
    /// Full proleptic Gregorian year (e.g. `2024`).
    pub year: i32,
    /// Month of the year, 1-based (1 = January … 12 = December).
    pub month: u8,
    /// Day of the month, 1-based.
    pub day: u8,
}

/// A wall-clock time without any date or timezone component.
///
/// Corresponds to the TOML `local-time` type (e.g. `07:32:00`).
/// Per TOML 1.1, the seconds field is optional on input and defaults to `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    /// Hour of the day, 0–23.
    pub hour: u8,
    /// Minute of the hour, 0–59.
    pub minute: u8,
    /// Second of the minute, 0–59.  Defaults to `0` because TOML 1.1 makes
    /// the seconds component optional.
    pub second: u8,
    /// Sub-second precision in nanoseconds, 0–999_999_999.
    pub nanosecond: u32,
}

/// A combined date and time without any timezone component.
///
/// Corresponds to the TOML `local-date-time` type (e.g. `1979-05-27T07:32:00`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDateTime {
    /// The date part.
    pub date: LocalDate,
    /// The time part.
    pub time: LocalTime,
}

/// A date-time with an explicit UTC offset.
///
/// Corresponds to the TOML `offset-date-time` type
/// (e.g. `1979-05-27T07:32:00Z` or `1979-05-27T07:32:00+05:30`).
///
/// The sentinel value [`OffsetDateTime::UTC_OFFSET`] (`i32::MIN`) is used to
/// represent the `Z` suffix (UTC without a numeric offset), so that the
/// original TOML spelling can be round-tripped faithfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffsetDateTime {
    /// The date part.
    pub date: LocalDate,
    /// The time part.
    pub time: LocalTime,
    /// UTC offset expressed in whole minutes.
    ///
    /// Positive values are east of UTC, negative values are west.
    /// The special sentinel [`OffsetDateTime::UTC_OFFSET`] (`i32::MIN`)
    /// represents the literal `Z` suffix.
    pub offset_minutes: i32,
}

impl OffsetDateTime {
    /// Sentinel value for `offset_minutes` that represents the literal `Z`
    /// (UTC) suffix in TOML source, as opposed to an explicit `+00:00` offset.
    pub const UTC_OFFSET: i32 = i32::MIN;

    /// Returns `true` when this datetime carries the `Z` (UTC) sentinel offset.
    pub fn is_utc(self) -> bool {
        self.offset_minutes == Self::UTC_OFFSET
    }
}

// ── Display implementations ──────────────────────────────────────────────────

impl fmt::Display for LocalDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl fmt::Display for LocalTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.nanosecond == 0 {
            write!(f, "{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
        } else {
            // Trim trailing zeros from nanosecond fractional part
            let ns = self.nanosecond;
            let (frac, digits) = trim_nanoseconds(ns);
            write!(
                f,
                "{:02}:{:02}:{:02}.{:0width$}",
                self.hour,
                self.minute,
                self.second,
                frac,
                width = digits
            )
        }
    }
}

/// Returns (trimmed_value, digit_count) for a nanosecond value with trailing zeros removed.
fn trim_nanoseconds(ns: u32) -> (u32, usize) {
    if ns == 0 {
        return (0, 1);
    }
    let s = format!("{:09}", ns);
    let trimmed = s.trim_end_matches('0');
    let digits = trimmed.len();
    let val: u32 = trimmed.parse().unwrap_or(0);
    (val, digits)
}

impl fmt::Display for LocalDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}T{}", self.date, self.time)
    }
}

impl fmt::Display for OffsetDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_utc() {
            write!(f, "{}T{}Z", self.date, self.time)
        } else {
            let off = self.offset_minutes;
            let sign = if off >= 0 { '+' } else { '-' };
            let abs = off.unsigned_abs();
            let hours = abs / 60;
            let mins = abs % 60;
            write!(f, "{}T{}{}{:02}:{:02}", self.date, self.time, sign, hours, mins)
        }
    }
}

// ── Chrono conversions ────────────────────────────────────────────────────────

impl From<chrono::NaiveDate> for LocalDate {
    fn from(d: chrono::NaiveDate) -> Self {
        use chrono::Datelike;
        Self {
            year: d.year(),
            month: d.month() as u8,
            day: d.day() as u8,
        }
    }
}

impl From<LocalDate> for chrono::NaiveDate {
    fn from(d: LocalDate) -> Self {
        chrono::NaiveDate::from_ymd_opt(d.year, d.month as u32, d.day as u32)
            .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
    }
}

impl From<chrono::NaiveTime> for LocalTime {
    fn from(t: chrono::NaiveTime) -> Self {
        use chrono::Timelike;
        Self {
            hour: t.hour() as u8,
            minute: t.minute() as u8,
            second: t.second() as u8,
            nanosecond: t.nanosecond(),
        }
    }
}

impl From<LocalTime> for chrono::NaiveTime {
    fn from(t: LocalTime) -> Self {
        chrono::NaiveTime::from_hms_nano_opt(
            t.hour as u32,
            t.minute as u32,
            t.second as u32,
            t.nanosecond,
        )
        .unwrap_or_else(|| chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap())
    }
}

impl From<chrono::NaiveDateTime> for LocalDateTime {
    fn from(dt: chrono::NaiveDateTime) -> Self {
        Self {
            date: dt.date().into(),
            time: dt.time().into(),
        }
    }
}

impl From<LocalDateTime> for chrono::NaiveDateTime {
    fn from(dt: LocalDateTime) -> Self {
        let date: chrono::NaiveDate = dt.date.into();
        let time: chrono::NaiveTime = dt.time.into();
        date.and_time(time)
    }
}

impl From<chrono::DateTime<chrono::FixedOffset>> for OffsetDateTime {
    fn from(dt: chrono::DateTime<chrono::FixedOffset>) -> Self {
        use chrono::{Datelike, Timelike};
        let offset_secs = dt.offset().local_minus_utc();
        let offset_minutes = if offset_secs == 0 {
            Self::UTC_OFFSET
        } else {
            offset_secs / 60
        };
        Self {
            date: LocalDate {
                year: dt.year(),
                month: dt.month() as u8,
                day: dt.day() as u8,
            },
            time: LocalTime {
                hour: dt.hour() as u8,
                minute: dt.minute() as u8,
                second: dt.second() as u8,
                nanosecond: dt.nanosecond(),
            },
            offset_minutes,
        }
    }
}

impl From<OffsetDateTime> for chrono::DateTime<chrono::FixedOffset> {
    fn from(dt: OffsetDateTime) -> Self {
        let offset_secs = if dt.is_utc() {
            0
        } else {
            dt.offset_minutes * 60
        };
        let offset = chrono::FixedOffset::east_opt(offset_secs).unwrap_or_else(|| {
            chrono::FixedOffset::east_opt(0).unwrap()
        });
        let naive_date = chrono::NaiveDate::from_ymd_opt(
            dt.date.year,
            dt.date.month as u32,
            dt.date.day as u32,
        )
        .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap());
        let naive_time = chrono::NaiveTime::from_hms_nano_opt(
            dt.time.hour as u32,
            dt.time.minute as u32,
            dt.time.second as u32,
            dt.time.nanosecond,
        )
        .unwrap_or_else(|| chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let naive_dt = naive_date.and_time(naive_time);
        chrono::DateTime::from_naive_utc_and_offset(naive_dt, offset)
    }
}
