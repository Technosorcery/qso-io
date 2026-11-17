//! Unchecked ADI descriptors, framed field contents, and field framing.

use std::{borrow::Cow, ops::Range};

use winnow::{
    Parser as _, Result,
    ascii::digit1,
    combinator::opt,
    error::{ContextError, FromExternalError, StrContext, StrContextValue},
    stream::{Location, Stream as _},
    token::{take_till, take_until},
};

use super::super::{
    error::{AdiError, SyntaxErrorKind},
    options::{LengthUnit, ParseEncoding, ParseOpts},
};
use super::{Input, decode_text};
use crate::adif::DecodeError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::adif::adi) struct Specifier<'a> {
    pub name: &'a str,
    pub length: usize,
    pub type_indicator: Option<&'a str>,
}

/// A length-framed ADI field with an unchecked indicator and cached decoding.
///
/// Names and indicators keep their UTF-8 spelling. Contents keep their input
/// bytes even when text decoding fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissiveField<'a> {
    pub(in crate::adif::adi::parser) specifier: Specifier<'a>,
    pub(in crate::adif::adi::parser) raw_data: &'a [u8],
    pub(in crate::adif::adi::parser) descriptor_span: Range<usize>,
    pub(in crate::adif::adi::parser) value_span: Range<usize>,
    pub(in crate::adif::adi::parser) text: Result<Cow<'a, str>, DecodeError>,
}

impl<'a> PermissiveField<'a> {
    /// Returns the supplied field name, which can be empty.
    pub fn name(&self) -> &str {
        self.specifier.name
    }

    /// Returns the length declared in the selected length unit.
    pub fn declared_length(&self) -> usize {
        self.specifier.length
    }

    /// Returns the unchecked indicator spelling.
    ///
    /// An explicit empty indicator gives `Some("")`.
    pub fn type_indicator(&self) -> Option<&str> {
        self.specifier.type_indicator
    }

    /// Borrows the supplied value bytes.
    pub fn raw_data(&self) -> &'a [u8] {
        self.raw_data
    }

    /// Returns the descriptor's exclusive-end byte range in the supplied input.
    pub fn descriptor_span(&self) -> Range<usize> {
        self.descriptor_span.clone()
    }

    /// Returns the value's exclusive-end byte range in the supplied input.
    pub fn value_span(&self) -> Range<usize> {
        self.value_span.clone()
    }

    /// Borrows the cached decoded text or its decoding error.
    ///
    /// Error offsets count bytes from the start of the field value.
    pub fn text(&self) -> Result<&str, &DecodeError> {
        self.text.as_deref()
    }
}

pub(in crate::adif::adi::parser) fn frame_value<'a, S: std::fmt::Debug>(
    input: &mut Input<'a, S>,
    specifier: Specifier<'a>,
    descriptor_span: Range<usize>,
    parse_opts: ParseOpts,
) -> Result<PermissiveField<'a>> {
    if specifier.length == 0 {
        return Ok(PermissiveField {
            specifier,
            descriptor_span,
            raw_data: &[],
            value_span: Range {
                start: input.current_token_start(),
                end: input.current_token_start(),
            },
            text: Ok(Cow::Borrowed("")),
        });
    }
    let truncation = |available| AdiError::TruncatedValue {
        field_name: specifier.name.to_string(),
        declared: specifier.length,
        available,
        unit: parse_opts.field_length,
    };

    let byte_count = match (parse_opts.encoding, parse_opts.field_length) {
        (ParseEncoding::Utf8, LengthUnit::UnicodeScalars) => {
            let original: &[u8] = input.input.as_ref();
            let mut remaining = original;
            let mut byte_count = 0;

            for idx in 0..specifier.length {
                let (character, width) = bstr::decode_utf8(remaining);
                if character.is_none() {
                    let _ = input.next_slice(byte_count);

                    let error = if remaining.is_empty() {
                        let mut error = ContextError::from_external_error(input, truncation(idx));
                        error.push(StrContext::Label("field data"));

                        error
                    } else {
                        let start = input.current_token_start();
                        let cause =
                            std::str::from_utf8(&original[..byte_count + remaining.len().min(4)])
                                .expect_err("prefix should be invalid UTF-8 because decode_utf8 rejected its leading scalar");
                        let mut error = ContextError::from_external_error(
                            input,
                            AdiError::Decoding {
                                span: start
                                    ..start + cause.error_len().unwrap_or(remaining.len().min(4)),
                                cause: DecodeError::Utf8(cause),
                            },
                        );
                        error.push(StrContext::Label("field data"));
                        error.push(StrContext::Expected(StrContextValue::Description(
                            "a complete, valid UTF-8 scalar",
                        )));

                        error
                    };

                    return Err(error);
                }

                byte_count += width;
                remaining = &remaining[width..];
            }
            byte_count
        }
        _ => {
            let available_bytes = input.eof_offset();
            let byte_count = specifier.length;

            if available_bytes < byte_count {
                let _ = input.next_slice(available_bytes);

                let mut error =
                    ContextError::from_external_error(input, truncation(available_bytes));
                error.push(StrContext::Label("field data"));
                return Err(error);
            }

            byte_count
        }
    };
    let (raw_data, value_span) = winnow::token::take(byte_count)
        .with_span()
        .context(StrContext::Label("field data"))
        .parse_next(input)?;

    let text = decode_text(raw_data, parse_opts.encoding);

    Ok(PermissiveField {
        specifier,
        descriptor_span,
        raw_data,
        value_span,
        text,
    })
}

fn declared_length<S: std::fmt::Debug>(input: &mut Input<'_, S>) -> Result<usize> {
    let checkpoint = input.checkpoint();
    let start = input.current_token_start();
    let digits: &[u8] = match digit1::<_, ContextError>.parse_next(input) {
        Ok(digits) => digits,
        Err(_) => {
            let span = start..start + usize::from(input.eof_offset() != 0);
            let mut error = ContextError::from_external_error(
                input,
                AdiError::Syntax {
                    reason: SyntaxErrorKind::InvalidLength,
                    span,
                    cause: None,
                },
            );
            error.push(StrContext::Label("length"));
            return Err(error);
        }
    };
    // digit1 on this byte stream returns ASCII digits only.
    let number = std::str::from_utf8(digits)
        .expect("length should be UTF-8 because digit1 accepted only ASCII digits");
    number.parse::<usize>().map_err(|cause| {
        input.reset(&checkpoint);
        let mut error = ContextError::from_external_error(
            input,
            AdiError::Syntax {
                reason: SyntaxErrorKind::LengthOverflow,
                span: start..start + digits.len(),
                cause: Some(cause),
            },
        );
        error.push(StrContext::Label("length"));
        error
    })
}

pub(in crate::adif::adi::parser) fn data_specifier<'a, S: std::fmt::Debug>(
    input: &mut Input<'a, S>,
) -> Result<Specifier<'a>> {
    descriptor(input, |_, _, _| Ok(()))
}

pub(in crate::adif::adi::parser) fn descriptor<'a, S: std::fmt::Debug>(
    input: &mut Input<'a, S>,
    mut check_indicator: impl FnMut(&mut Input<'a, S>, &'a str, Range<usize>) -> Result<()>,
) -> Result<Specifier<'a>> {
    '<'.parse_next(input)?;
    let (name, name_span) = take_until(0.., ":").with_span().parse_next(input)?;
    let name = descriptor_text(
        input,
        name,
        name_span.start,
        SyntaxErrorKind::InvalidNameEncoding,
    )?;
    ':'.parse_next(input)?;
    let length = declared_length(input)?;
    let type_indicator = if opt(':').parse_next(input)?.is_some() {
        let (bytes, span) = take_till(0.., b'>').with_span().parse_next(input)?;
        let code = descriptor_text(
            input,
            bytes,
            span.start,
            SyntaxErrorKind::InvalidIndicatorEncoding,
        )?;
        check_indicator(input, code, span)?;
        Some(code)
    } else {
        None
    };
    '>'.parse_next(input)?;
    Ok(Specifier {
        name,
        length,
        type_indicator,
    })
}

pub(in crate::adif::adi::parser) fn descriptor_text<'a, S: std::fmt::Debug>(
    input: &Input<'a, S>,
    bytes: &'a [u8],
    start: usize,
    reason: SyntaxErrorKind,
) -> Result<&'a str> {
    str::from_utf8(bytes).map_err(|cause| {
        ContextError::from_external_error(
            input,
            AdiError::DescriptorEncoding {
                reason,
                span: start + cause.valid_up_to()
                    ..start
                        + cause.valid_up_to()
                        + cause
                            .error_len()
                            .unwrap_or(bytes.len() - cause.valid_up_to()),
                cause: String::from_utf8(bytes.to_vec()).expect_err(
                    "descriptor bytes should fail String decoding because the same bytes failed UTF-8 validation"),
            },
        )
    })
}

pub(in crate::adif::adi::parser) fn parse<'a>(
    input: &mut Input<'a, ParseOpts>,
) -> Result<PermissiveField<'a>> {
    let (specifier, span) = data_specifier.with_span().parse_next(input)?;
    let options = input.state;
    frame_value(input, specifier, span, options)
}
