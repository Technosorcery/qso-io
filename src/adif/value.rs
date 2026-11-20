//! ADIF field-value representations and conversion failure categories.

use std::borrow::Cow;

use bigdecimal::{BigDecimal, num_bigint::BigInt};
use chrono::{NaiveDate, NaiveTime};
use thiserror::Error;

use super::number::{NumberFailure, parse_number};

/// An ADIF field value.
///
/// This enum is exhaustive. Untyped contents and declared text types use the
/// same text representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value<'a> {
    /// Decoded text with its supplied spelling.
    Text(Cow<'a, str>),
    /// Top-level list members with their source order and compound member
    /// spelling.
    List(Vec<Cow<'a, str>>),
    /// The Boolean value represented by ADIF Y/y or N/n.
    Boolean(bool),
    /// An exact Integer or PositiveInteger value.
    Integer(BigInt),
    /// An exact ADIF decimal Number.
    Number(BigDecimal),
    /// A valid ADIF calendar date expressing a UTC date component.
    Date(NaiveDate),
    /// A UTC clock component with the supplied precision.
    Time(AdifTime),
    /// ADIF direction, degrees and exact decimal minutes.
    Location(AdifLocation),
}

/// An ADIF clock value and whether the source supplied seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdifTime {
    pub(super) time: NaiveTime,
    pub(super) precision: TimePrecision,
}

impl AdifTime {
    /// Returns the parsed clock value, with zero seconds for minute precision.
    pub fn time(&self) -> NaiveTime {
        self.time
    }

    /// Returns the precision explicitly supplied by the input.
    pub fn precision(&self) -> TimePrecision {
        self.precision
    }
}

/// Precision of an ADIF HHMM or HHMMSS clock spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimePrecision {
    /// The input supplied hours and minutes.
    Minute,
    /// The input also supplied seconds. The seconds can be 00.
    Second,
}

/// A Location value with ADIF's direction, degrees and decimal minutes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdifLocation {
    pub(super) hemisphere: Hemisphere,
    pub(super) degrees: u16,
    pub(super) minutes: BigDecimal,
}

impl AdifLocation {
    /// Returns the supplied direction, also for a zero coordinate.
    pub fn hemisphere(&self) -> Hemisphere {
        self.hemisphere
    }

    /// Returns the generic ADIF degree component in 0..=180.
    pub fn degrees(&self) -> u16 {
        self.degrees
    }

    /// Borrows the exact decimal minute component in 0..=59.999.
    pub fn minutes(&self) -> &BigDecimal {
        &self.minutes
    }
}

/// Direction of an ADIF Location value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hemisphere {
    /// North (N).
    North,
    /// South (S).
    South,
    /// East (E).
    East,
    /// West (W).
    West,
}

/// The reason an ADIF value violates its type grammar or applicable member set.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ValueError {
    /// Invalid AwardList grammar, cardinality, or applicable membership.
    #[error("invalid AwardList")]
    InvalidAwardList,
    /// Invalid CreditList grammar, cardinality, or applicable membership.
    #[error("invalid CreditList")]
    InvalidCreditList,
    /// Invalid SponsoredAwardList grammar, cardinality, or applicable
    /// membership.
    #[error("invalid SponsoredAwardList")]
    InvalidSponsoredAwardList,
    /// Invalid GridSquareList grammar, cardinality, or applicable membership.
    #[error("invalid GridSquareList")]
    InvalidGridSquareList,
    /// Invalid POTARefList grammar, cardinality, or applicable membership.
    #[error("invalid POTARefList")]
    InvalidPOTARefList,
    /// Invalid SecondarySubdivisionList grammar, cardinality, or applicable
    /// membership.
    #[error("invalid SecondarySubdivisionList")]
    InvalidSecondarySubdivisionList,
    /// Invalid SecondaryAdministrativeSubdivisionListAlt grammar, cardinality,
    /// or applicable membership.
    #[error("invalid SecondaryAdministrativeSubdivisionListAlt")]
    InvalidSecondaryAdministrativeSubdivisionListAlt,

    /// Invalid signed ASCII integer grammar.
    #[error("invalid Integer")]
    InvalidInteger,
    /// Invalid unsigned integer grammar or a value not greater than zero.
    #[error("invalid PositiveInteger")]
    InvalidPositiveInteger,
    /// Invalid Maidenhead locator.
    #[error("invalid GridSquare")]
    InvalidGridSquare,
    /// Invalid additional Maidenhead locator pairs.
    #[error("invalid GridSquareExt")]
    InvalidGridSquareExt,
    /// Invalid continent or island-group reference.
    #[error("invalid IOTARefNo")]
    InvalidIOTARefNo,
    /// Invalid POTA reference format.
    #[error("invalid POTARef")]
    InvalidPOTARef,
    /// Invalid SOTA text repertoire or missing slash/component.
    #[error("invalid SOTARef")]
    InvalidSOTARef,
    /// Invalid WWFF reference format.
    #[error("invalid WWFFRef")]
    InvalidWWFFRef,

    /// Invalid Boolean spelling.
    #[error("invalid Boolean")]
    InvalidBoolean,
    /// Invalid exact decimal Number syntax.
    #[error("invalid Number")]
    InvalidNumber,
    /// Invalid date spelling or ADIF calendar component.
    #[error("invalid Date")]
    InvalidDate,
    /// Invalid HHMM/HHMMSS spelling or clock component.
    #[error("invalid Time")]
    InvalidTime,
    /// Invalid direction, degree/minute spelling or generic component range.
    #[error("invalid Location")]
    InvalidLocation,
    /// A String character fails the selected repertoire rules.
    #[error("invalid String")]
    InvalidString,
    /// A MultilineString character or line break fails the selected rules.
    #[error("invalid MultilineString")]
    InvalidMultilineString,
    /// An Enumeration fails its text repertoire or applicable member set.
    #[error("invalid Enumeration")]
    InvalidEnumeration,
}

/// An enumeration or exact numeric range declared for a user-defined ADIF
/// field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserDefinitionConstraint {
    /// Nonempty String members with their supplied spelling and order.
    Enumeration(Vec<String>),
    /// An inclusive range whose upper bound is strictly greater than its lower bound.
    Range {
        /// Exact lower bound.
        lower: BigDecimal,
        /// Exact upper bound.
        upper: BigDecimal,
    },
}

pub(crate) enum UserConstraintViolation<'a> {
    NotInEnumeration,
    InvalidNumber(NumberFailure),
    OutOfRange {
        lower: &'a BigDecimal,
        upper: &'a BigDecimal,
    },
}

pub(crate) fn check_user_constraint<'a>(
    constraint: &'a UserDefinitionConstraint,
    text: &str,
    parsed_number: Option<&BigDecimal>,
) -> Result<(), UserConstraintViolation<'a>> {
    if text.is_empty() {
        return Ok(());
    }
    match constraint {
        UserDefinitionConstraint::Enumeration(members) => {
            if !members
                .iter()
                .any(|member| caseless::default_caseless_match_str(member, text))
            {
                return Err(UserConstraintViolation::NotInEnumeration);
            }
        }
        UserDefinitionConstraint::Range { lower, upper } => {
            let parsed;
            let number = match parsed_number {
                Some(number) => number,
                None => {
                    parsed = parse_number(text).map_err(UserConstraintViolation::InvalidNumber)?;
                    &parsed
                }
            };
            if number < lower || number > upper {
                return Err(UserConstraintViolation::OutOfRange { lower, upper });
            }
        }
    }
    Ok(())
}
