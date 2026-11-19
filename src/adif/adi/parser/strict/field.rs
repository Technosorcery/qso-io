//! Strict field representation and conversion from framed contents.

use std::{borrow::Cow, collections::HashMap, ops::Range};

use winnow::{
    error::{FromExternalError, ParserError},
    stream::{Location, Stream as _},
};

use super::super::framing::field::PermissiveField;
use super::super::{
    error::{self, Result, StructuralError},
    framing,
};
use super::{Stream, declarations::UserDefinition, decoding_error, definitions};
use crate::adif::value::Value;
use crate::adif::{DataType, FieldSection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DataSpecifier<'a> {
    pub(super) name: Cow<'a, str>,
    pub(super) length: usize,
    pub(super) type_indicator: Option<DataType>,
}

/// A complete length-framed field with immutable bytes and cached text
/// decoding.
///
/// The field keeps name casing, raw value bytes, and decoded spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field<'a> {
    pub(super) specifier: DataSpecifier<'a>,
    pub(super) data_type: Option<DataType>,
    pub(super) raw_data: &'a [u8],
    pub(super) descriptor_span: Range<usize>,
    pub(super) value_span: Range<usize>,
    pub(super) text: Option<Cow<'a, str>>,
    pub(super) value: Option<Value<'a>>,
}

impl<'a> Field<'a> {
    /// Returns the field name with its supplied casing.
    pub fn name(&self) -> &str {
        &self.specifier.name
    }

    /// Returns the descriptor's length in the unit selected by
    /// [`ParseOpts`](crate::adif::adi::parser::options::ParseOpts).
    pub fn declared_length(&self) -> usize {
        self.specifier.length
    }

    /// Returns the resolved ADIF type, or None if the field has no resolved
    /// type.
    ///
    /// Standard definitions, USERDEF declarations, application-field history,
    /// or an inline indicator can establish this type, independently of
    /// emptiness.
    pub fn data_type(&self) -> Option<DataType> {
        self.data_type
    }

    /// Returns the type named by the optional inline descriptor indicator.
    ///
    /// If the descriptor has no indicator, this method returns None. A
    /// definition or previous application-field occurrence can still establish
    /// the type.
    pub fn type_indicator(&self) -> Option<DataType> {
        self.specifier.type_indicator
    }

    /// Borrows the supplied value bytes.
    pub fn raw_data(&self) -> &'a [u8] {
        self.raw_data
    }

    /// Returns the descriptor's byte range from `<` through `>` in the input.
    ///
    /// The range excludes the end position and the value bytes.
    pub fn descriptor_span(&self) -> Range<usize> {
        self.descriptor_span.clone()
    }

    /// Returns the value's byte range in the parser's input, with an exclusive
    /// end.
    ///
    /// An empty value has an empty range at its actual input position, and
    /// spans always count bytes even when the declared length counts scalars.
    pub fn value_span(&self) -> Range<usize> {
        self.value_span.clone()
    }

    /// Borrows the decoded text with its supplied spelling.
    pub fn text(&self) -> &str {
        match &self.value {
            Some(Value::Text(text)) => text,
            _ => self.text.as_deref().unwrap_or(""),
        }
    }

    /// Borrows the stored value, or returns None for supplied zero-length
    /// contents.
    pub fn value(&self) -> Option<&Value<'a>> {
        self.value.as_ref()
    }
}

pub(super) fn prepared_field<'a, 'header>(
    input: &mut Stream<'a>,
    users: &HashMap<String, &'header UserDefinition<'a>>,
) -> Result<(Field<'a>, definitions::ResolvedDefinition), StructuralError<'header>> {
    let (raw_specifier, specifier, span) = strict_descriptor(input)?;
    let resolution = definitions::resolve(
        &input.state,
        users,
        FieldSection::Record,
        &specifier,
        span.clone(),
    )?;
    let f = strict_frame_value(input, raw_specifier, span)?;
    let parsed = super::values::parse(f, &resolution, input.state.parser_options.encoding)?;
    definitions::check_user_constraints(users, &parsed, input.state.parser_options.encoding)?;
    Ok((parsed, resolution))
}

pub(super) fn strict_frame_value<'a>(
    input: &mut Stream<'a>,
    specifier: framing::field::Specifier<'a>,
    span: Range<usize>,
) -> Result<PermissiveField<'a>> {
    let options = input.state.parser_options;
    let field = framing::field::frame_value(input, specifier, span, options)?;
    if let Err(cause) = field.text {
        return Err(decoding_error(
            field.raw_data,
            field.value_span.start,
            cause,
        ));
    }
    Ok(field)
}

pub(super) fn strict_descriptor<'a>(
    input: &mut Stream<'a>,
) -> Result<(
    framing::field::Specifier<'a>,
    DataSpecifier<'a>,
    Range<usize>,
)> {
    let checkpoint = input.checkpoint();
    let start = input.current_token_start();
    let (raw, span) = framing::field::descriptor(input, |input, code, span| {
        let alpha_len = code.bytes().take_while(u8::is_ascii_alphabetic).count();
        let kind = match DataType::try_from(&code[..alpha_len]) {
            Ok(kind) => kind,
            Err(cause) => {
                input.reset(&checkpoint);
                let _ = input.next_slice(span.start - start);
                let error = StructuralError::from_external_error(input, cause);

                return Err(error);
            }
        };
        if matches!(kind, DataType::IntlString | DataType::IntlMultilineString) {
            return Err(StructuralError::TypeUnavailableInAdi {
                data_type: kind,
                span: span.start..span.start + alpha_len,
            });
        }
        if alpha_len != code.len() {
            input.reset(&checkpoint);
            let _ = input.next_slice(span.start - start + alpha_len);
            let error = StructuralError::from_input(input);

            return Err(error);
        }
        Ok(())
    })
    .map(|raw| {
        let end = input.current_token_start();
        (raw, start..end)
    })?;
    let typed = strict_specifier(raw, span.clone())?;
    Ok((raw, typed, span))
}

pub(super) fn strict_specifier<'a>(
    raw: framing::field::Specifier<'a>,
    span: Range<usize>,
) -> std::result::Result<DataSpecifier<'a>, StructuralError<'static>> {
    let type_indicator = if let Some(code) = raw.type_indicator {
        let start = span.end - 1 - code.len();
        let kind = DataType::try_from(code).map_err(|source| StructuralError::Syntax {
            reason: error::SyntaxErrorKind::UnknownTypeIndicator { source },
            span: start..start + code.len().max(1),
        })?;
        if matches!(kind, DataType::IntlString | DataType::IntlMultilineString) {
            return Err(StructuralError::TypeUnavailableInAdi {
                data_type: kind,
                span: start..span.end - 1,
            });
        }
        Some(kind)
    } else {
        None
    };
    Ok(DataSpecifier {
        name: Cow::Borrowed(raw.name),
        length: raw.length,
        type_indicator,
    })
}
