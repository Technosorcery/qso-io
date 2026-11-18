//! ADIF value conversion from decoded text.

use std::ops::Range;

use bigdecimal::num_bigint::BigInt;
use chrono::{Datelike, NaiveDate, NaiveTime, Timelike};

use crate::adif::value::{AdifLocation, AdifTime, Hemisphere, TimePrecision, ValueError};
use crate::adif::{
    DataType,
    number::{NumericCause, parse_number},
};

pub(crate) mod identifiers;
pub(crate) mod lists;

/// A value conversion failure with a byte range in the decoded text.
#[derive(Debug, Clone)]
pub(crate) struct ValueFailure {
    pub cause: ValueError,
    pub enumeration: Option<&'static str>,

    /// Byte range within the decoded text passed to the grammar, with an
    /// exclusive end.
    pub span: Range<usize>,

    pub numeric_cause: Option<NumericCause>,
}

fn digits(bytes: &[u8], start: usize, end: usize) -> std::result::Result<u32, Range<usize>> {
    let component = &bytes[start.min(bytes.len())..end.min(bytes.len())];

    if let Some(offset) = component.iter().position(|byte| !byte.is_ascii_digit()) {
        let index = start + offset;
        return Err(index..index + 1);
    }

    if component.len() < end - start {
        return Err(bytes.len()..bytes.len());
    }

    component.iter().try_fold(0u32, |value, byte| {
        value
            .checked_mul(10)
            .and_then(|value| value.checked_add(u32::from(byte - b'0')))
            .ok_or(start..end)
    })
}

fn width(bytes: &[u8], size: usize) -> std::result::Result<(), Range<usize>> {
    if bytes.len() > size {
        Err(size..size + 1)
    } else if bytes.len() < size {
        Err(bytes.len()..bytes.len())
    } else {
        Ok(())
    }
}

/// Checks the character repertoire of a declared string or enumeration.
pub(crate) fn check_string(
    kind: DataType,
    text: &str,
    check_text: impl Fn(DataType, &str) -> Result<(), Range<usize>>,
) -> Result<(), ValueFailure> {
    let reason = match kind {
        DataType::String | DataType::IntlString => ValueError::InvalidString,
        DataType::MultilineString | DataType::IntlMultilineString => {
            ValueError::InvalidMultilineString
        }
        _ => ValueError::InvalidEnumeration,
    };
    check_text(kind, text).map_err(|span| ValueFailure {
        cause: reason,
        span,
        numeric_cause: None,
        enumeration: None,
    })?;
    Ok(())
}

/// Parses an ADIF Boolean from decoded text.
pub(crate) fn boolean(text: &str) -> Result<bool, ValueFailure> {
    let raw = text.as_bytes();

    let value = match raw.first().copied() {
        Some(b'Y' | b'y') => true,
        Some(b'N' | b'n') => false,
        _ => {
            return Err(ValueFailure {
                cause: ValueError::InvalidBoolean,
                span: 0..raw.len().min(1),
                numeric_cause: None,
                enumeration: None,
            });
        }
    };

    width(raw, 1).map_err(|span| ValueFailure {
        cause: ValueError::InvalidBoolean,
        span,
        numeric_cause: None,
        enumeration: None,
    })?;

    Ok(value)
}

/// Parses an ADIF Integer or PositiveInteger from decoded text.
pub(crate) fn integer(text: &str, positive: bool) -> Result<BigInt, ValueFailure> {
    let raw = text.as_bytes();

    let reason = if !positive {
        ValueError::InvalidInteger
    } else {
        ValueError::InvalidPositiveInteger
    };

    for (index, byte) in raw.iter().enumerate() {
        if !byte.is_ascii_digit() && !(!positive && index == 0 && *byte == b'-') {
            return Err(ValueFailure {
                cause: reason,
                span: index..index + 1,
                numeric_cause: None,
                enumeration: None,
            });
        }
    }

    if raw == b"-" {
        return Err(ValueFailure {
            cause: reason,
            span: raw.len()..raw.len(),
            numeric_cause: None,
            enumeration: None,
        });
    }

    let integer: BigInt = text.parse().map_err(|cause| ValueFailure {
        cause: reason,
        span: 0..raw.len(),
        numeric_cause: Some(crate::adif::number::NumericCause::Integer(cause)),
        enumeration: None,
    })?;
    if positive && integer <= BigInt::default() {
        return Err(ValueFailure {
            cause: reason,
            span: 0..raw.len(),
            numeric_cause: None,
            enumeration: None,
        });
    }

    Ok(integer)
}

/// Parses eight ASCII digits as a calendar date with a year of at least 1930.
///
/// Chrono validates the calendar date. Calendar failures cover the complete
/// decoded text. Syntax failures locate invalid bytes or a length error;
/// year failures cover the first four bytes.
pub(crate) fn date(text: &str) -> Result<NaiveDate, ValueFailure> {
    let raw = text.as_bytes();
    if let Some(index) = raw.iter().position(|byte| !byte.is_ascii_digit()) {
        return Err(ValueFailure {
            cause: ValueError::InvalidDate,
            span: index..index + 1,
            numeric_cause: None,
            enumeration: None,
        });
    }
    width(raw, 8).map_err(|span| ValueFailure {
        cause: ValueError::InvalidDate,
        span,
        numeric_cause: None,
        enumeration: None,
    })?;

    let date = NaiveDate::parse_from_str(text, "%Y%m%d").map_err(|_| ValueFailure {
        cause: ValueError::InvalidDate,
        span: 0..text.len(),
        numeric_cause: None,
        enumeration: None,
    })?;
    if date.year() < 1930 {
        return Err(ValueFailure {
            cause: ValueError::InvalidDate,
            span: 0..4,
            numeric_cause: None,
            enumeration: None,
        });
    }

    Ok(date)
}

/// Parses ADIF HHMM or HHMMSS text, retaining its supplied precision.
///
/// Chrono validates the clock. Clock and leap second failures cover the complete
/// decoded text; syntax failures locate invalid bytes or a length error.
pub(crate) fn time(text: &str) -> Result<AdifTime, ValueFailure> {
    let raw = text.as_bytes();
    if let Some(index) = raw.iter().position(|byte| !byte.is_ascii_digit()) {
        return Err(ValueFailure {
            cause: ValueError::InvalidTime,
            span: index..index + 1,
            numeric_cause: None,
            enumeration: None,
        });
    }
    let size = if raw.len() <= 4 { 4 } else { 6 };
    width(raw, size).map_err(|span| ValueFailure {
        cause: ValueError::InvalidTime,
        span,
        numeric_cause: None,
        enumeration: None,
    })?;

    let precision = if size == 4 {
        TimePrecision::Minute
    } else {
        TimePrecision::Second
    };
    let time = match precision {
        TimePrecision::Minute => NaiveTime::parse_from_str(text, "%H%M"),
        TimePrecision::Second => NaiveTime::parse_from_str(text, "%H%M%S"),
    }
    .map_err(|_| ValueFailure {
        cause: ValueError::InvalidTime,
        span: 0..text.len(),
        numeric_cause: None,
        enumeration: None,
    })?;

    // Chrono represents second 60 with at least one billion nanoseconds.
    // ADIF limits seconds to 00 through 59.
    if time.nanosecond() >= 1_000_000_000 {
        return Err(ValueFailure {
            cause: ValueError::InvalidTime,
            span: 0..text.len(),
            numeric_cause: None,
            enumeration: None,
        });
    }

    Ok(AdifTime { time, precision })
}

/// Parses an ADIF Location from decoded text.
pub(crate) fn location(text: &str) -> Result<AdifLocation, ValueFailure> {
    let raw = text.as_bytes();

    let reason = ValueError::InvalidLocation;

    let hemisphere = match raw.first().copied() {
        Some(b'N') => Hemisphere::North,
        Some(b'S') => Hemisphere::South,
        Some(b'E') => Hemisphere::East,
        Some(b'W') => Hemisphere::West,
        _ => {
            return Err(ValueFailure {
                cause: reason,
                span: 0..raw.len().min(1),
                numeric_cause: None,
                enumeration: None,
            });
        }
    };

    let degrees = digits(raw, 1, 4).map_err(|span| ValueFailure {
        cause: reason,
        span,
        numeric_cause: None,
        enumeration: None,
    })?;
    if raw.get(4) != Some(&b' ') {
        return Err(ValueFailure {
            cause: reason,
            span: 4.min(raw.len())..5.min(raw.len()),
            numeric_cause: None,
            enumeration: None,
        });
    }

    let minutes = digits(raw, 5, 7).map_err(|span| ValueFailure {
        cause: reason,
        span,
        numeric_cause: None,
        enumeration: None,
    })?;
    if raw.get(7) != Some(&b'.') {
        return Err(ValueFailure {
            cause: reason,
            span: 7.min(raw.len())..8.min(raw.len()),
            numeric_cause: None,
            enumeration: None,
        });
    }

    digits(raw, 8, 11).map_err(|span| ValueFailure {
        cause: reason,
        span,
        numeric_cause: None,
        enumeration: None,
    })?;
    width(raw, 11).map_err(|span| ValueFailure {
        cause: reason,
        span,
        numeric_cause: None,
        enumeration: None,
    })?;

    if degrees > 180 {
        return Err(ValueFailure {
            cause: reason,
            span: 1..4,
            numeric_cause: None,
            enumeration: None,
        });
    }

    if minutes > 59 {
        return Err(ValueFailure {
            cause: reason,
            span: 5..11,
            numeric_cause: None,
            enumeration: None,
        });
    }

    let minutes = parse_number(&text.as_bytes()[5..11]).map_err(|e| ValueFailure {
        cause: reason,
        span: 5 + e.span.start..5 + e.span.end,
        numeric_cause: e.cause.map(Into::into),
        enumeration: None,
    })?;

    Ok(AdifLocation {
        hemisphere,
        degrees: degrees as u16,
        minutes,
    })
}

/// Parses an exact ADIF number from decoded text.
pub(crate) fn number(text: &str) -> Result<bigdecimal::BigDecimal, ValueFailure> {
    parse_number(text.as_bytes()).map_err(|error| ValueFailure {
        cause: ValueError::InvalidNumber,
        span: error.span,
        numeric_cause: error.cause.map(Into::into),
        enumeration: None,
    })
}
