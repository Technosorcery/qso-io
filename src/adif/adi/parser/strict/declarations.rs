//! USERDEF declarations, constraints, and their interpretation.

use std::{borrow::Cow, ops::Range};

use bigdecimal::{BigDecimal, num_bigint::BigUint};
use thiserror::Error;
use winnow::{
    Result,
    error::{ContextError, FromExternalError},
};

use super::super::{error::AdiError, framing::field::PermissiveField};
use super::field::{DataSpecifier, strict_specifier};
use super::{Stream, decode_located, decoding_error};
use crate::adif::DataType;
use crate::adif::{number::parse_number, text::check_text};

/// A parsed USERDEFn header declaration, retaining its original spelling and bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDefinition<'a> {
    pub(super) specifier: DataSpecifier<'a>,
    pub(super) number: BigUint,
    pub(super) field_name: String,
    pub(super) constraint: Option<UserDefinitionConstraint>,
    pub(super) target_span: Range<usize>,
    pub(super) constraint_span: Option<Range<usize>>,
    pub(super) raw_data: &'a [u8],
    pub(super) text: Cow<'a, str>,
    pub(super) descriptor_span: Range<usize>,
    pub(super) value_span: Range<usize>,
}

impl<'a> UserDefinition<'a> {
    /// Returns the original USERDEFn descriptor name.
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

    /// Returns the supplied record-field type indicator, or None when omitted.
    pub fn data_type(&self) -> Option<DataType> {
        self.specifier.type_indicator
    }

    /// Borrows the optional parsed enumeration or numeric range.
    pub fn constraint(&self) -> Option<&UserDefinitionConstraint> {
        self.constraint.as_ref()
    }

    /// Returns the descriptor length in the selected parsing unit.
    pub fn declared_length(&self) -> usize {
        self.specifier.length
    }

    /// Borrows the original declaration payload bytes.
    pub fn raw_data(&self) -> &'a [u8] {
        self.raw_data
    }

    /// Borrows the original decoded declaration spelling.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the descriptor's original byte range, with an exclusive end.
    pub fn descriptor_span(&self) -> Range<usize> {
        self.descriptor_span.clone()
    }

    /// Returns the payload's original byte range, with an exclusive end.
    pub fn value_span(&self) -> Range<usize> {
        self.value_span.clone()
    }
}

/// An enumeration or exact numeric range declared by USERDEFn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserDefinitionConstraint {
    /// String members in supplied order, preserving spelling; members must be nonempty.
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
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
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
    InvalidRangeBound,
    /// The upper bound is equal to or smaller than the lower bound.
    #[error("upper range bound must exceed lower bound")]
    InvalidRangeOrder,
}

/// A supplied nonempty value fails its document USERDEF constraint.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum UserDefinitionValueError {
    /// Contents required for a range are not an ADIF Number.
    #[error("range value is not an ADIF Number: {cause}")]
    InvalidRangeValue {
        /// Type-specific numeric grammar reason.
        cause: crate::adif::value::ValueError,
    },
    /// A Number falls outside the declaration's inclusive bounds.
    #[error("value is outside inclusive range {lower} through {upper}")]
    OutOfRange {
        /// Exact inclusive lower bound.
        lower: BigDecimal,
        /// Exact inclusive upper bound.
        upper: BigDecimal,
    },
    /// The original spelling matches no member under default Unicode folding.
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
    let specifier = strict_specifier(framed.specifier, framed.descriptor_span.clone())
        .map_err(|cause| ContextError::from_external_error(input, cause))?;
    let failure = |cause, span| {
        ContextError::from_external_error(
            input,
            AdiError::InvalidUserDefinition {
                field_name: framed.specifier.name.to_string(),
                span,
                cause,
                numeric_cause: None,
            },
        )
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

    let bytes = framed.raw_data;
    let comma = bytes.iter().position(|b| *b == b',');
    let name_end = comma.unwrap_or(bytes.len());
    let field_name = decode_located(
        input,
        &bytes[..name_end],
        input.state.parser_options.encoding,
        framed.value_span.start,
    )?
    .into_owned();
    let start = framed.value_span.start;

    let constraint = if let Some(comma) = comma {
        let open = comma + 1;

        if bytes.get(open) != Some(&b'{') {
            return Err(failure(
                UserDefinitionError::MalformedDeclaration,
                start + open..start + (open + 1).min(bytes.len()),
            ));
        }
        if bytes.last() != Some(&b'}') || bytes.len() <= open + 1 {
            return Err(failure(
                UserDefinitionError::MalformedDeclaration,
                start + bytes.len()..start + bytes.len(),
            ));
        }

        let body = &bytes[open + 1..bytes.len() - 1];
        let body_start = start + open + 1;

        if specifier.type_indicator != Some(DataType::Enumeration)
            && let Some(colon) = body.iter().position(|b| *b == b':')
        {
            let bound = |part: &[u8], offset| {
                parse_number(part).map_err(|error| {
                    ContextError::from_external_error(
                        input,
                        AdiError::InvalidUserDefinition {
                            field_name: framed.specifier.name.to_string(),
                            span: offset + error.span.start..offset + error.span.end,
                            cause: UserDefinitionError::InvalidRangeBound,
                            numeric_cause: error.cause.map(Into::into),
                        },
                    )
                })
            };

            let lower = bound(&body[..colon], body_start)?;
            let upper = bound(&body[colon + 1..], body_start + colon + 1)?;

            if upper <= lower {
                return Err(failure(
                    UserDefinitionError::InvalidRangeOrder,
                    body_start + colon + 1..body_start + body.len(),
                ));
            }

            Some(UserDefinitionConstraint::Range { lower, upper })
        } else {
            let mut items = Vec::new();
            let mut offset = body_start;

            for part in body.split(|b| *b == b',') {
                if part.is_empty() {
                    return Err(failure(
                        UserDefinitionError::InvalidEnumeration,
                        offset..offset,
                    ));
                }
                if let Err(span) = check_text(part, input.state.parser_options.encoding, false) {
                    return Err(failure(
                        UserDefinitionError::InvalidEnumeration,
                        offset + span.start..offset + span.end,
                    ));
                }

                items.push(
                    decode_located(input, part, input.state.parser_options.encoding, offset)?
                        .into_owned(),
                );
                offset += part.len() + 1;
            }

            Some(UserDefinitionConstraint::Enumeration(items))
        }
    } else {
        None
    };

    Ok(UserDefinition {
        target_span: start..start + name_end,
        constraint_span: comma.map(|offset| start + offset + 1..framed.value_span.end),
        specifier,
        number,
        field_name,
        constraint,
        raw_data: framed.raw_data,
        text: framed.text.map_err(|cause| {
            ContextError::from_external_error(
                input,
                decoding_error(framed.raw_data, framed.value_span.start, cause),
            )
        })?,
        descriptor_span: framed.descriptor_span,
        value_span: framed.value_span,
    })
}
