//! ADX fields with decoded text and typed values.

use std::{borrow::Cow, collections::HashMap, ops::Range};

use crate::adif::{
    DataType, FieldSection, StandardField,
    parse::values::{self, ValueFailure},
    value::{Value, ValueError},
};

use super::super::{
    decoding::{self, TextContext},
    error::{ParseError, TypeAlternativeFailure},
    identity::FieldIdentity,
    model::local,
};
use super::{attribute::Attribute, declarations::UserDefinition, model::Element};

/// An ADX field with decoded text and an optional typed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field<'a> {
    pub(super) element: Element<'a>,
    kind: Option<DataType>,
    text: Option<Cow<'a, str>>,
    value: Option<Value<'a>>,
}

impl<'a> Field<'a> {
    /// Returns the supplied qualified XML name.
    pub fn name(&self) -> &'a str {
        self.element.name
    }

    /// Borrows the complete supplied element.
    pub fn raw_element(&self) -> &'a [u8] {
        &self.element.input[self.element.span.clone()]
    }

    /// Borrows the supplied contents, before XML decoding.
    pub fn raw_data(&self) -> &'a [u8] {
        self.element.raw_content.as_bytes()
    }

    /// Returns the complete element's byte range in the parser's input, with an
    /// exclusive end.
    pub fn element_span(&self) -> Range<usize> {
        self.element.span.clone()
    }

    /// Returns the element name's byte range in the parser's input, with an
    /// exclusive end.
    pub fn name_span(&self) -> Range<usize> {
        self.element.name_span.clone()
    }

    /// Returns the contents' byte range in the parser's input, excluding tags
    /// and with an exclusive end.
    pub fn value_span(&self) -> Range<usize> {
        self.element.content.clone()
    }

    /// Borrows supplied attributes in source order.
    pub fn attributes(&self) -> impl Iterator<Item = &Attribute<'a>> {
        self.element.attributes()
    }

    /// Returns the supplied TYPE text, if present.
    pub fn supplied_type(&self) -> Option<&str> {
        self.element.attribute("TYPE").map(Attribute::text)
    }

    /// Returns the effective ADIF type.
    pub fn data_type(&self) -> Option<DataType> {
        self.kind
    }

    /// Borrows the complete decoded text, preserving its spelling.
    pub fn text(&self) -> &str {
        match &self.value {
            Some(Value::Text(t)) => t,
            _ => self.text.as_deref().unwrap_or(""),
        }
    }

    /// Returns the typed value, or None for empty decoded text.
    pub fn value(&self) -> Option<&Value<'a>> {
        self.value.as_ref()
    }

    /// Returns the supplied complete domain identity.
    pub fn identity(&self) -> FieldIdentity<'_> {
        self.element.identity()
    }
}
pub(super) fn check_text(kind: DataType, text: &str) -> Result<(), Range<usize>> {
    let intl = matches!(kind, DataType::IntlString | DataType::IntlMultilineString);
    let multiline = matches!(
        kind,
        DataType::MultilineString | DataType::IntlMultilineString
    );
    let mut chars = text.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if multiline && c == '\r' && chars.peek().is_some_and(|(_, c)| *c == '\n') {
            chars.next();
            continue;
        }

        if multiline && c == '\n' {
            continue;
        }

        if if intl {
            c == '\r' || c == '\n'
        } else {
            !(' '..='~').contains(&c)
        } {
            return Err(i..i + c.len_utf8());
        }
    }

    Ok(())
}

pub(super) fn convert<'a>(
    element: Element<'a>,
    text: Cow<'a, str>,
    section: FieldSection,
    users: &HashMap<&str, &UserDefinition<'a>>,
    apps: &mut HashMap<(String, String), (DataType, Range<usize>)>,
) -> Result<Field<'a>, ParseError<'a>> {
    if local(element.name) == "APP" && section != FieldSection::Record {
        return Err(ParseError::Envelope {
            span: element.name_span.clone(),
            message: "APP is a QSO field",
        });
    }

    if !matches!(local(element.name), "APP" | "USERDEF")
        && let Some(a) = element.attribute("TYPE")
    {
        return Err(ParseError::InvalidType {
            span: a.value_span(),
            supplied: a.text().to_owned(),
            message: "TYPE is only supplied on APP and USERDEF",
        });
    }

    let standard = local(element.name)
        .parse::<StandardField>()
        .ok()
        .filter(|f| f.section() == section);
    let mut application = None;
    let mut user_definition = None;
    let single_kind;
    let kinds = match element.identity().key() {
        FieldIdentity::Application {
            program_id,
            field_name,
        } => {
            element.required("PROGRAMID")?;
            element.required("FIELDNAME")?;
            let attribute = element.required("TYPE")?;
            let kind = type_attribute(attribute)?;
            let key = (program_id.into_owned(), field_name.into_owned());

            if let Some((expected, first)) = apps.get(&key)
                && *expected != kind
            {
                return Err(ParseError::TypeConflict {
                    span: attribute.value_span(),
                    first_span: first.clone(),
                    identity: FieldIdentity::Application {
                        program_id: Cow::Owned(key.0),
                        field_name: Cow::Owned(key.1),
                    },
                    expected: *expected,
                    supplied: kind,
                });
            }

            application = Some((key, kind));
            single_kind = kind;
            std::slice::from_ref(&single_kind)
        }
        FieldIdentity::User(name) => {
            let attribute = element.required("FIELDNAME")?;
            let definition =
                users
                    .get(name.as_ref())
                    .copied()
                    .ok_or_else(|| ParseError::UndefinedUser {
                        span: attribute.value_span(),
                        name: name.into_owned(),
                    })?;
            user_definition = Some(definition);
            single_kind = definition.data_type();
            std::slice::from_ref(&single_kind)
        }
        _ => standard.map_or(&[][..], StandardField::data_types),
    };

    let f = 'conversion: {
        if text.is_empty() {
            break 'conversion Field {
                element,
                kind: kinds.first().copied(),
                text: None,
                value: None,
            };
        }
        let mut failures = Vec::new();
        for &kind in kinds {
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
                            && let Some(standard) = standard
                        {
                            standard
                                .check_membership(&text)
                                .map_err(|set| ValueFailure {
                                    cause: ValueError::InvalidEnumeration,
                                    enumeration: Some(set),
                                    span: 0..text.len(),
                                    numeric_cause: None,
                                })?;
                        }
                        Ok(())
                    });
                    match checked {
                        Ok(()) => {
                            break 'conversion Field {
                                element,
                                kind: Some(kind),
                                text: None,
                                value: Some(Value::Text(text)),
                            };
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
                    return Err(ParseError::InvalidType {
                        span: element.name_span,
                        supplied: format!("{kind:?}"),
                        message: "component types are not standalone fields",
                    });
                }
            };
            match attempt {
                Ok(value) => {
                    break 'conversion Field {
                        element,
                        kind: Some(kind),
                        text: Some(text),
                        value: Some(value),
                    };
                }
                Err(error) => {
                    let span = decoding::locate_text_range(
                        element.raw_content,
                        element.content.start,
                        error.span,
                        TextContext::Content,
                    )?;
                    let error = ParseError::Value {
                        span,
                        field_name: element.name.to_owned(),
                        cause: error.cause,
                        enumeration: error.enumeration,
                        source: error
                            .numeric_cause
                            .map(crate::adif::number::NumericCause::boxed),
                    };
                    if kinds.len() == 1 {
                        return Err(error);
                    }
                    failures.push(TypeAlternativeFailure {
                        data_type: kind,
                        error,
                    });
                }
            }
        }
        if let Some(first) = failures.first() {
            return Err(ParseError::NoMatchingType {
                span: first.error.span(),
                field_name: element.name.to_owned(),
                failures,
            });
        }
        // No resolved types: retain unchecked text directly.
        break 'conversion Field {
            element,
            kind: None,
            text: None,
            value: Some(Value::Text(text)),
        };
    };

    if let Some(definition) = user_definition {
        definition.check(&f)?;
    }
    if let Some((key, kind)) = application {
        apps.entry(key).or_insert_with(|| (kind, f.element_span()));
    }
    Ok(f)
}

pub(super) fn type_attribute<'a>(attribute: &Attribute<'a>) -> Result<DataType, ParseError<'a>> {
    match DataType::try_from(attribute.text()) {
        Ok(kind) => Ok(kind),
        Err(error) => {
            let span = decoding::locate_text_range(
                attribute.raw_text(),
                attribute.value_span().start,
                0..attribute.text().len(),
                TextContext::Attribute,
            )?;
            Err(ParseError::InvalidType {
                span,
                supplied: error.supplied,
                message: "unknown ADX type indicator",
            })
        }
    }
}
