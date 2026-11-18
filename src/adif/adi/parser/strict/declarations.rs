//! USERDEF declarations, constraints, and their interpretation.

use std::{borrow::Cow, ops::Range};

use bigdecimal::{BigDecimal, num_bigint::BigUint};
use thiserror::Error;

use super::super::{
    error::{Result, StructuralError},
    framing::field::PermissiveField,
};
use super::field::{DataSpecifier, strict_specifier};
use super::{Stream, decoding_error};
use crate::adif::DataType;
use crate::adif::{number::parse_number, text::check_text};

/// A parsed USERDEFn header declaration with its supplied spelling and bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDefinition<'a> {
    pub(super) specifier: DataSpecifier<'a>,
    pub(super) number: BigUint,
    pub(super) field_name: String,
    pub(super) constraint: Option<LocatedConstraint>,
    pub(super) target_span: Range<usize>,
    pub(super) raw_data: &'a [u8],
    pub(super) text: Cow<'a, str>,
    pub(super) descriptor_span: Range<usize>,
    pub(super) value_span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LocatedConstraint {
    pub(super) constraint: UserDefinitionConstraint,
    pub(super) span: Range<usize>,
}

impl<'a> UserDefinition<'a> {
    /// Returns the supplied USERDEFn descriptor name.
    pub fn name(&self) -> &str {
        &self.specifier.name
    }

    /// Returns the positive declaration number.
    pub fn number(&self) -> &BigUint {
        &self.number
    }

    /// Returns the declared record-field name with its supplied spelling.
    pub fn field_name(&self) -> &str {
        &self.field_name
    }

    /// Returns the supplied record-field type indicator, or None if it is
    /// absent.
    pub fn data_type(&self) -> Option<DataType> {
        self.specifier.type_indicator
    }

    /// Borrows the optional parsed enumeration or numeric range.
    pub fn constraint(&self) -> Option<&UserDefinitionConstraint> {
        self.constraint.as_ref().map(|located| &located.constraint)
    }

    /// Returns the descriptor length in the selected parsing unit.
    pub fn declared_length(&self) -> usize {
        self.specifier.length
    }

    /// Borrows the supplied declaration payload bytes.
    pub fn raw_data(&self) -> &'a [u8] {
        self.raw_data
    }

    /// Borrows the supplied decoded declaration spelling.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the descriptor's byte range in the input, with an exclusive end.
    pub fn descriptor_span(&self) -> Range<usize> {
        self.descriptor_span.clone()
    }

    /// Returns the payload's byte range in the input, with an exclusive end.
    pub fn value_span(&self) -> Range<usize> {
        self.value_span.clone()
    }
}

/// An enumeration or exact numeric range declared by USERDEFn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserDefinitionConstraint {
    /// Nonempty String members with their supplied spelling and order.
    Enumeration(Vec<String>),
    /// A range whose upper bound is strictly greater than its lower bound.
    Range {
        /// Exact lower bound.
        lower: BigDecimal,
        /// Exact upper bound.
        upper: BigDecimal,
    },
}

/// The part of a USERDEFn declaration that failed parsing.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum UserDefinitionError {
    /// The declaration number is not positive.
    #[error("invalid declaration number")]
    InvalidDeclarationNumber,
    /// The target is a standard ADIF field name.
    #[error("user-defined target is a standard ADIF field name")]
    StandardFieldName,
    /// Declaration delimiters or its constraint envelope are malformed.
    #[error("malformed declaration")]
    MalformedDeclaration,
    /// An enumeration member fails the selected String character rules.
    #[error("invalid enumeration member")]
    InvalidEnumeration,
    /// A range bound fails the ADIF Number grammar.
    #[error("invalid numeric range bound")]
    InvalidRangeBound {
        /// Underlying decimal conversion failure, when available.
        #[source]
        source: Option<bigdecimal::ParseBigDecimalError>,
    },
    /// The upper bound is equal to or smaller than the lower bound.
    #[error("upper range bound must exceed lower bound")]
    InvalidRangeOrder,
}

/// A supplied nonempty value fails its document USERDEF constraint.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum UserDefinitionValueError<'header> {
    /// Contents required for a range are not an ADIF Number.
    #[error("range value is not an ADIF Number")]
    InvalidRangeValue {
        /// Underlying decimal conversion failure, when available.
        #[source]
        source: Option<bigdecimal::ParseBigDecimalError>,
    },
    /// A Number is outside the declaration's inclusive bounds.
    #[error("value is outside inclusive range {lower} through {upper}")]
    OutOfRange {
        /// Exact inclusive lower bound borrowed from the declaration.
        lower: &'header BigDecimal,
        /// Exact inclusive upper bound borrowed from the declaration.
        upper: &'header BigDecimal,
    },
    /// The supplied spelling matches no member under default Unicode folding.
    #[error("value is not a member of the declared enumeration")]
    NotInEnumeration,
}

pub(super) fn is_declaration(name: &str) -> bool {
    name.get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("USERDEF"))
        && name.len() > 7
        && name.as_bytes()[7..].iter().all(u8::is_ascii_digit)
}

pub(super) fn parse<'a>(
    input: &Stream<'a>,
    framed: PermissiveField<'a>,
) -> Result<UserDefinition<'a>> {
    let specifier = strict_specifier(framed.specifier, framed.descriptor_span.clone())?;
    let failure = |cause, span| StructuralError::InvalidUserDefinition {
        field_name: framed.specifier.name.to_string(),
        span,
        cause,
    };

    let suffix = framed.specifier.name.get(7..).ok_or_else(|| {
        failure(
            UserDefinitionError::InvalidDeclarationNumber,
            framed.descriptor_span.clone(),
        )
    })?;
    let number_span = framed.descriptor_span.start + 8
        ..framed.descriptor_span.start + 1 + framed.specifier.name.len();
    let number: BigUint = suffix.parse().map_err(|_| {
        failure(
            UserDefinitionError::InvalidDeclarationNumber,
            number_span.clone(),
        )
    })?;

    if number == BigUint::default() {
        return Err(failure(
            UserDefinitionError::InvalidDeclarationNumber,
            number_span,
        ));
    }

    let text = framed
        .text
        .map_err(|cause| decoding_error(framed.raw_data, framed.value_span.start, cause))?;
    let encoding = input.state.parser_options.encoding;
    let start = framed.value_span.start;
    let source_span = |span| {
        let span = super::values::source_range(&text, span, encoding);
        start + span.start..start + span.end
    };
    let comma = text.find(',');
    let name_end = comma.unwrap_or(text.len());
    let field_name = text[..name_end].to_owned();
    let target_span = source_span(0..name_end);

    let constraint = if let Some(comma) = comma {
        let open = comma + 1;

        if text.as_bytes().get(open) != Some(&b'{') {
            return Err(failure(
                UserDefinitionError::MalformedDeclaration,
                source_span(open..(open + 1).min(text.len())),
            ));
        }

        if !text.ends_with('}') || text.len() <= open + 1 {
            return Err(failure(
                UserDefinitionError::MalformedDeclaration,
                source_span(text.len()..text.len()),
            ));
        }

        let body = &text[open + 1..text.len() - 1];
        let body_start = open + 1;

        let constraint = if specifier.type_indicator != Some(DataType::Enumeration)
            && let Some(colon) = body.find(':')
        {
            let bound = |part: &str, offset| {
                parse_number(part).map_err(|error| StructuralError::InvalidUserDefinition {
                    field_name: framed.specifier.name.to_string(),
                    span: source_span(offset + error.span.start..offset + error.span.end),
                    cause: UserDefinitionError::InvalidRangeBound {
                        source: error.cause,
                    },
                })
            };

            let lower = bound(&body[..colon], body_start)?;
            let upper = bound(&body[colon + 1..], body_start + colon + 1)?;

            if upper <= lower {
                return Err(failure(
                    UserDefinitionError::InvalidRangeOrder,
                    source_span(body_start + colon + 1..body_start + body.len()),
                ));
            }

            UserDefinitionConstraint::Range { lower, upper }
        } else {
            let mut items = Vec::new();
            let mut offset = body_start;

            for part in body.split(',') {
                if part.is_empty() {
                    return Err(failure(
                        UserDefinitionError::InvalidEnumeration,
                        source_span(offset..offset),
                    ));
                }

                let span = source_span(offset..offset + part.len());
                let raw_part = &framed.raw_data[span.start - start..span.end - start];
                if let Err(invalid) = check_text(raw_part, encoding, false) {
                    return Err(failure(
                        UserDefinitionError::InvalidEnumeration,
                        span.start + invalid.start..span.start + invalid.end,
                    ));
                }

                items.push(part.to_owned());
                offset += part.len() + 1;
            }

            UserDefinitionConstraint::Enumeration(items)
        };
        Some(LocatedConstraint {
            constraint,
            span: source_span(open..text.len()),
        })
    } else {
        None
    };

    Ok(UserDefinition {
        target_span,
        specifier,
        number,
        field_name,
        constraint,
        raw_data: framed.raw_data,
        text,
        descriptor_span: framed.descriptor_span,
        value_span: framed.value_span,
    })
}
