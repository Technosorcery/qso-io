//! Typed ADX user-defined field declarations.

use std::{borrow::Cow, ops::Range};

use bigdecimal::num_bigint::BigUint;

use crate::adif::value::UserDefinitionConstraint;
use crate::adif::{DataType, StandardField, UserFieldDefinition, number::parse_number};

use super::super::{
    decoding,
    error::{DeclarationError, ParseError},
};
use super::{
    attribute::Attribute,
    field::{check_text, type_attribute},
    model::Element,
};

/// A header USERDEF declaration with distinct identifier and target locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDefinition<'a> {
    pub(super) element: Element<'a>,
    target: Cow<'a, str>,
    identifier_span: Range<usize>,
    number: BigUint,
    kind: DataType,
    pub(super) constraint: Option<LocatedConstraint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LocatedConstraint {
    pub constraint: UserDefinitionConstraint,
    pub span: Range<usize>,
}

impl<'a> UserDefinition<'a> {
    /// Borrows the declaration data. The type is always `Some` because ADX
    /// requires the declaration's `TYPE` attribute.
    pub fn definition(&self) -> UserFieldDefinition<'_> {
        UserFieldDefinition {
            name: self.field_name(),
            data_type: Some(self.data_type()),
            constraint: self.constraint(),
        }
    }

    /// Returns the positive declaration identifier from FIELDID.
    pub fn number(&self) -> &BigUint {
        &self.number
    }

    /// Returns the decoded declared target name.
    pub fn field_name(&self) -> &str {
        &self.target
    }

    /// Returns the declaration's effective type.
    pub fn data_type(&self) -> DataType {
        self.kind
    }

    /// Borrows the optional declared enumeration or range.
    pub fn constraint(&self) -> Option<&UserDefinitionConstraint> {
        self.constraint
            .as_ref()
            .map(|constraint| &constraint.constraint)
    }

    /// Returns the FIELDID value's byte range in the parser's input, excluding
    /// quotation marks and with an exclusive end.
    pub fn identifier_span(&self) -> Range<usize> {
        self.identifier_span.clone()
    }

    /// Returns the target contents' byte range in the parser's input, excluding
    /// tags and with an exclusive end.
    pub fn target_span(&self) -> Range<usize> {
        self.element.content.clone()
    }

    /// Returns the ENUM or RANGE value's byte range in the parser's input, if
    /// present, excluding quotation marks and with an exclusive end.
    pub fn constraint_span(&self) -> Option<Range<usize>> {
        self.constraint
            .as_ref()
            .map(|constraint| constraint.span.clone())
    }

    /// Borrows the complete original declaration element.
    pub fn raw_element(&self) -> &'a [u8] {
        &self.element.input[self.element.span.clone()]
    }

    /// Borrows the original target name contents.
    pub fn raw_data(&self) -> &'a [u8] {
        self.element.raw_content.as_bytes()
    }

    /// Returns the complete declaration's byte range in the parser's input,
    /// with an exclusive end.
    pub fn element_span(&self) -> Range<usize> {
        self.element.span.clone()
    }

    /// Borrows supplied attributes in source order.
    pub fn attributes(&self) -> impl Iterator<Item = &Attribute<'a>> {
        self.element.attributes()
    }
}

pub(super) fn parse<'a>(
    element: Element<'a>,
    target: Cow<'a, str>,
) -> Result<UserDefinition<'a>, ParseError<'a>> {
    let identifier = element.required("FIELDID")?;
    let identifier_span = identifier.value_span();
    let id = identifier.text();
    let number = if id.bytes().all(|b| b.is_ascii_digit()) {
        id.parse::<BigUint>().ok()
    } else {
        None
    };

    let number =
        number
            .filter(|n| *n != BigUint::default())
            .ok_or_else(|| ParseError::Declaration {
                span: identifier_span.clone(),
                reason: DeclarationError::Identifier,
            })?;
    let kind = type_attribute(element.required("TYPE")?)?;
    if target.is_empty()
        || target.starts_with(' ')
        || target.ends_with(' ')
        || target.contains([',', ':', '<', '>', '{', '}'])
        || target.bytes().any(|byte| byte.is_ascii_lowercase())
        || check_text(DataType::String, &target).is_err()
    {
        return Err(ParseError::Declaration {
            span: element.content.clone(),
            reason: DeclarationError::Target,
        });
    }

    if target.parse::<StandardField>().is_ok()
        || target.strip_prefix("USERDEF").is_some_and(|s| {
            !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && s.bytes().any(|b| b != b'0')
        })
    {
        return Err(ParseError::Declaration {
            span: element.content.clone(),
            reason: DeclarationError::StandardTarget,
        });
    }

    if let (Some(_), Some(range)) = (element.attribute("ENUM"), element.attribute("RANGE")) {
        return Err(ParseError::Declaration {
            span: range.value_span(),
            reason: DeclarationError::ConflictingConstraints,
        });
    }

    let constraint = if let Some(a) = element.attribute("ENUM") {
        let text = a.text();
        let body = text
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .ok_or_else(|| ParseError::Declaration {
                span: a.value_span(),
                reason: DeclarationError::Enumeration,
            })?;
        let mut members = Vec::new();

        for member in body.split(',') {
            if member.is_empty() || check_text(DataType::String, member).is_err() {
                return Err(ParseError::Declaration {
                    span: a.value_span(),
                    reason: DeclarationError::Enumeration,
                });
            }

            members.push(member.to_owned());
        }

        Some(LocatedConstraint {
            constraint: UserDefinitionConstraint::Enumeration(members),
            span: a.value_span(),
        })
    } else if let Some(a) = element.attribute("RANGE") {
        let text = a.text();
        let body = text
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .ok_or_else(|| ParseError::Declaration {
                span: a.value_span(),
                reason: DeclarationError::Range { source: None },
            })?;
        let (left, right) = body
            .split_once(':')
            .ok_or_else(|| ParseError::Declaration {
                span: a.value_span(),
                reason: DeclarationError::Range { source: None },
            })?;
        let bound = |s: &str, offset: usize| {
            parse_number(s).map_err(|e| {
                match decoding::locate_text_range(
                    a.raw_text(),
                    a.value_span().start,
                    offset + e.span.start..offset + e.span.end,
                    decoding::TextContext::Attribute,
                ) {
                    Ok(span) => ParseError::Declaration {
                        span,
                        reason: DeclarationError::Range { source: e.cause },
                    },
                    Err(error) => error,
                }
            })
        };

        let lower = bound(left, 1)?;
        let upper = bound(right, left.len() + 2)?;
        if lower >= upper {
            return Err(ParseError::Declaration {
                span: a.value_span(),
                reason: DeclarationError::RangeOrder,
            });
        }

        Some(LocatedConstraint {
            constraint: UserDefinitionConstraint::Range { lower, upper },
            span: a.value_span(),
        })
    } else {
        None
    };

    Ok(UserDefinition {
        element,
        target,
        identifier_span,
        number,
        kind,
        constraint,
    })
}
