// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

//! Date and time RANGE validation.
//!
//! Both older ports matched date/time values on SHAPE alone:
//! `^\d\d\d\d-\d\d-\d\d` says nothing about whether 13 is a month or 32 a
//! day. Each mishandled the result in its own way, and range-checking is
//! what makes every port agree, because it removes the value they were
//! disagreeing about rather than choosing between them.

use std::sync::OnceLock;

use regex::Regex;

const MONTH_DAYS: [i32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

fn days_in_month(year: i32, month: i32) -> i32 {
    if 2 != month {
        // Callers check `1 <= month <= 12` first, so the index is in range.
        return MONTH_DAYS[(month - 1) as usize];
    }
    // Proleptic Gregorian, the calendar RFC 3339 specifies. 2100 is NOT a
    // leap year, which is exactly what the corpus's feb-29 document tests.
    let leap = (0 == year % 4 && 0 != year % 100) || 0 == year % 400;
    if leap {
        29
    } else {
        28
    }
}

/// Seconds may be 60: RFC 3339 permits a positive leap second, and TOML
/// inherits its date-time grammar from it. 61 is the corpus's second-over.
fn time_in_range(hour: i32, minute: i32, second: i32) -> bool {
    hour <= 23 && minute <= 59 && second <= 60
}

/// The capture forms of the isodate and localtime patterns. Anchored at
/// BOTH ends so they can only agree with what those patterns already
/// matched; a mismatch means the two have drifted apart, and the value is
/// let through rather than silently rejected on a shape this code does not
/// actually understand.
fn isodate_parts() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(\d{4})-(\d{2})-(\d{2})(?:[Tt ](\d{2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?(?:[Zz]|[-+](\d{2}):(\d{2}))?)?$",
        )
        .expect("the isodate capture pattern is a literal and compiles")
    })
}

fn localtime_parts() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(\d{2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?$")
            .expect("the localtime capture pattern is a literal and compiles")
    })
}

/// A capture group the pattern has already constrained to digits. An
/// absent group is reported as -1, so a caller can tell it apart from a
/// real zero.
fn num(group: Option<regex::Match<'_>>) -> i32 {
    group
        .map(|found| found.as_str())
        .filter(|text| !text.is_empty())
        .and_then(|text| text.parse::<i32>().ok())
        .unwrap_or(-1)
}

/// Whether a date or date-time whose SHAPE matched denotes a real instant.
pub fn isodate_in_range(text: &str) -> bool {
    let Some(parts) = isodate_parts().captures(text) else {
        return true;
    };

    let (year, month, day) = (num(parts.get(1)), num(parts.get(2)), num(parts.get(3)));
    if !(1..=12).contains(&month) || day < 1 || days_in_month(year, month) < day {
        return false;
    }

    // No time part: the date alone is in range.
    if parts.get(4).is_none() {
        return true;
    }
    let second = num(parts.get(6)).max(0);
    if !time_in_range(num(parts.get(4)), num(parts.get(5)), second) {
        return false;
    }

    // Offset, when written as +hh:mm rather than Z.
    parts.get(7).is_none() || (num(parts.get(7)) <= 23 && num(parts.get(8)) <= 59)
}

/// Whether a local time whose SHAPE matched denotes a real instant.
pub fn localtime_in_range(text: &str) -> bool {
    let Some(parts) = localtime_parts().captures(text) else {
        return true;
    };
    let second = num(parts.get(3)).max(0);
    time_in_range(num(parts.get(1)), num(parts.get(2)), second)
}
