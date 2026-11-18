use std::ops::Range;

use super::definitions::{ResolvedDefinition, TypeSelection};
use crate::adif::adi::parser::error::{StructuralErrorKind, TypeAlternativeFailure};
use crate::adif::{DataType, parse::values, value::Value};

use super::super::{error::AdiError, framing::field::PermissiveField, options::ParseEncoding};
use super::{
    decoding_error,
    field::{Field, strict_specifier},
};

/// Maps a byte range in decoded `text` to a byte range in its encoded source.
///
/// The input range uses offsets within `text`; the returned range uses offsets
/// within the encoded value. Both ranges have exclusive ends. `encoding` is the
/// source encoding.
fn source_range(text: &str, range: Range<usize>, encoding: ParseEncoding) -> Range<usize> {
    if encoding != ParseEncoding::Latin1 {
        return range;
    }
    // A grammar error may identify one byte within a multibyte character in
    // decoded `text`. Include that character's source byte in the result.
    let start = text
        .char_indices()
        .take_while(|(i, _)| *i < range.start)
        .count();
    let start = if text.is_char_boundary(range.start) {
        start
    } else {
        start.saturating_sub(1)
    };

    let end = text
        .char_indices()
        .take_while(|(i, _)| *i < range.end)
        .count();
    start..end
}

pub(super) fn parse<'a>(
    f: PermissiveField<'a>,
    resolution: &ResolvedDefinition,
    encoding: ParseEncoding,
) -> Result<Field<'a>, AdiError> {
    let specifier = strict_specifier(f.specifier, f.descriptor_span.clone())?;
    let single_kind;
    let kinds = match resolution.selection {
        TypeSelection::Untyped => &[][..],
        TypeSelection::Single(kind) => {
            single_kind = kind;
            std::slice::from_ref(&single_kind)
        }
        TypeSelection::Alternatives(kinds) => kinds,
    };
    let text = f
        .text
        .map_err(|cause| decoding_error(f.raw_data, f.value_span.start, cause))?;
    let check_text = |kind, text: &str| {
        let multiline = kind == DataType::MultilineString;
        let mut chars = text.char_indices().peekable();

        while let Some((i, c)) = chars.next() {
            if multiline && c == '\r' && chars.peek().is_some_and(|(_, c)| *c == '\n') {
                chars.next();
                continue;
            }

            let valid = match encoding {
                ParseEncoding::Ascii => (' '..='~').contains(&c),
                ParseEncoding::Latin1 => {
                    (' '..='~').contains(&c) || ('\u{a0}'..='\u{ff}').contains(&c)
                }
                ParseEncoding::Utf8 => c != '\r' && c != '\n',
            };

            if !valid {
                return Err(i..i + c.len_utf8());
            }
        }

        Ok(())
    };
    let mut failures = Vec::new();

    for &kind in kinds {
        if matches!(
            kind,
            DataType::Digit
                | DataType::Character
                | DataType::IntlCharacter
                | DataType::IntlString
                | DataType::IntlMultilineString
        ) && !f.raw_data.is_empty()
        {
            return Err(AdiError::TypeUnavailableInAdi {
                data_type: kind,
                span: f.descriptor_span,
            });
        }
        if text.is_empty() {
            return Ok(Field {
                specifier,
                data_type: Some(kind),
                raw_data: f.raw_data,
                descriptor_span: f.descriptor_span,
                value_span: f.value_span,
                text: None,
                value: None,
            });
        }
        let attempt = match kind {
            kind @ (DataType::String
            | DataType::MultilineString
            | DataType::Enumeration
            | DataType::IntlString
            | DataType::IntlMultilineString
            | DataType::GridSquare
            | DataType::GridSquareExt
            | DataType::IOTARefNo
            | DataType::POTARef
            | DataType::SOTARef
            | DataType::WWFFRef) => {
                let checked = match kind {
                    DataType::String
                    | DataType::MultilineString
                    | DataType::Enumeration
                    | DataType::IntlString
                    | DataType::IntlMultilineString => {
                        values::check_string(kind, &text, check_text)
                    }
                    _ => values::identifiers::check(kind, &text, &check_text),
                }
                .and_then(|()| {
                    if kind == DataType::Enumeration
                        && let Some(standard) = resolution.standard
                    {
                        standard
                            .check_membership(&text)
                            .map_err(|set| values::ValueFailure {
                                cause: crate::adif::value::ValueError::InvalidEnumeration,
                                enumeration: Some(set),
                                span: 0..text.len(),
                                numeric_cause: None,
                            })?;
                    }
                    Ok(())
                });
                match checked {
                    Ok(()) => {
                        return Ok(Field {
                            specifier,
                            data_type: Some(kind),
                            raw_data: f.raw_data,
                            descriptor_span: f.descriptor_span,
                            value_span: f.value_span,
                            text: None,
                            value: Some(Value::Text(text)),
                        });
                    }
                    Err(error) => Err(error),
                }
            }
            DataType::Boolean => values::boolean(&text).map(Value::Boolean),
            DataType::Integer => values::integer(&text, false).map(Value::Integer),
            DataType::PositiveInteger => values::integer(&text, true).map(Value::Integer),
            DataType::Number => values::number(&text).map(Value::Number),
            DataType::Date => values::date(&text).map(Value::Date),
            DataType::Time => values::time(&text).map(Value::Time),
            DataType::Location => values::location(&text).map(Value::Location),
            kind @ (DataType::AwardList
            | DataType::CreditList
            | DataType::SponsoredAwardList
            | DataType::GridSquareList
            | DataType::POTARefList
            | DataType::SecondarySubdivisionList
            | DataType::SecondaryAdministrativeSubdivisionListAlt) => {
                values::lists::parse(kind, &text, &check_text).map(Value::List)
            }
            DataType::Digit | DataType::Character | DataType::IntlCharacter => {
                return Err(AdiError::TypeUnavailableInAdi {
                    data_type: kind,
                    span: f.descriptor_span,
                });
            }
        };
        match attempt {
            Ok(value) => {
                return Ok(Field {
                    specifier,
                    data_type: Some(kind),
                    raw_data: f.raw_data,
                    descriptor_span: f.descriptor_span,
                    value_span: f.value_span,
                    text: Some(text),
                    value: Some(value),
                });
            }
            Err(error) => {
                let span = source_range(&text, error.span, encoding);
                let span = f.value_span.start + span.start..f.value_span.start + span.end;
                if !matches!(resolution.selection, TypeSelection::Alternatives(_)) {
                    return Err(AdiError::InvalidValue {
                        field_name: specifier.name.into_owned(),
                        cause: error.cause,
                        span,
                        numeric_cause: error.numeric_cause,
                        enumeration: error.enumeration,
                    });
                }
                failures.push(TypeAlternativeFailure {
                    data_type: kind,
                    span,
                    cause: error.cause,
                    enumeration: error.enumeration,
                });
            }
        }
    }
    if let Some(first) = failures.first() {
        return Err(AdiError::Defined {
            span: first.span.clone(),
            numeric_cause: None,
            kind: Box::new(StructuralErrorKind::NoMatchingType {
                field_name: specifier.name.into_owned(),
                failures,
            }),
        });
    }
    // No resolved types: retain unchecked text directly.
    let value = if text.is_empty() {
        None
    } else {
        Some(Value::Text(text))
    };
    Ok(Field {
        specifier,
        data_type: None,
        raw_data: f.raw_data,
        descriptor_span: f.descriptor_span,
        value_span: f.value_span,
        text: None,
        value,
    })
}
