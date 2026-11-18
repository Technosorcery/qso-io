//! Unchecked ADI descriptors, framed field contents, and field framing.

use std::{borrow::Cow, ops::Range};

use winnow::{
    Parser as _,
    ascii::digit1,
    combinator::opt,
    error::StrContext,
    stream::{Location, Stream as _},
    token::{take_till, take_until},
};

use super::super::{
    error::{Result, StructuralError, SyntaxErrorKind},
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
    let end = input.current_token_start() + input.eof_offset();
    let truncation = |available| StructuralError::TruncatedValue {
        span: end..end,
        field_name: specifier.name.to_string(),
        declared: specifier.length,
        available,
        unit: parse_opts.field_length,
    };

    let byte_count = match (parse_opts.encoding, parse_opts.field_length) {
        (ParseEncoding::Utf8, LengthUnit::UnicodeScalars) => {
            let original: &[u8] = input.input.as_ref();
            let prefix = &original[..specifier.length.saturating_mul(4).min(original.len())];
            let chunk = prefix.utf8_chunks().next();
            let valid = chunk.as_ref().map_or("", |chunk| chunk.valid());
            let mut characters = valid.char_indices();
            let mut byte_count = 0;

            for idx in 0..specifier.length {
                let Some((offset, character)) = characters.next() else {
                    let _ = input.next_slice(byte_count);
                    std::str::from_utf8(prefix).map_err(|cause| {
                        let start = input.current_token_start();
                        StructuralError::Decoding {
                            span: start
                                ..start + cause.error_len().unwrap_or(prefix.len() - byte_count),
                            cause: DecodeError::Utf8(cause),
                        }
                    })?;
                    return Err(truncation(idx));
                };
                byte_count = offset + character.len_utf8();
            }
            byte_count
        }
        _ => {
            let available_bytes = input.eof_offset();
            let byte_count = specifier.length;

            if available_bytes < byte_count {
                let _ = input.next_slice(available_bytes);

                let error = truncation(available_bytes);

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
    let digits: &[u8] = match digit1::<_, StructuralError>.parse_next(input) {
        Ok(digits) => digits,
        Err(_) => {
            let span = start..start + usize::from(input.eof_offset() != 0);
            let error = StructuralError::Syntax {
                reason: SyntaxErrorKind::InvalidLength,
                span,
            };

            return Err(error);
        }
    };
    let length = digits.iter().try_fold(0usize, |value, byte| {
        value
            .checked_mul(10)
            .and_then(|value| value.checked_add(usize::from(byte - b'0')))
    });
    if let Some(length) = length {
        return Ok(length);
    }

    // Preserve the standard integer error from the supplied spelling on overflow.
    let number = descriptor_text(digits, start, |source| {
        SyntaxErrorKind::InvalidNameEncoding { source }
    })?;
    number.parse::<usize>().map_err(|cause| {
        input.reset(&checkpoint);
        StructuralError::Syntax {
            reason: SyntaxErrorKind::LengthOverflow { source: cause },
            span: start..start + digits.len(),
        }
    })
}

pub(in crate::adif::adi::parser) fn descriptor<'a, S: std::fmt::Debug>(
    input: &mut Input<'a, S>,
    mut check_indicator: impl FnMut(&mut Input<'a, S>, &'a str, Range<usize>) -> Result<()>,
) -> Result<Specifier<'a>> {
    '<'.parse_next(input)?;
    let (name, name_span) = take_until(0.., ":").with_span().parse_next(input)?;
    let name = descriptor_text(name, name_span.start, |source| {
        SyntaxErrorKind::InvalidNameEncoding { source }
    })?;
    ':'.parse_next(input)?;
    let length = declared_length(input)?;
    let type_indicator = if opt(':').parse_next(input)?.is_some() {
        let (bytes, span) = take_till(0.., b'>').with_span().parse_next(input)?;
        let code = descriptor_text(bytes, span.start, |source| {
            SyntaxErrorKind::InvalidIndicatorEncoding { source }
        })?;
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

pub(in crate::adif::adi::parser) fn descriptor_text(
    bytes: &[u8],
    start: usize,
    reason: impl FnOnce(std::str::Utf8Error) -> SyntaxErrorKind,
) -> Result<&str> {
    str::from_utf8(bytes).map_err(|cause| StructuralError::Syntax {
        reason: reason(cause),
        span: start + cause.valid_up_to()
            ..start
                + cause.valid_up_to()
                + cause
                    .error_len()
                    .unwrap_or(bytes.len() - cause.valid_up_to()),
    })
}

pub(in crate::adif::adi::parser) fn parse<'a>(
    input: &mut Input<'a, ParseOpts>,
) -> Result<PermissiveField<'a>> {
    let (specifier, span) =
        (|input: &mut Input<'a, ParseOpts>| descriptor(input, |_, _, _| Ok(())))
            .with_span()
            .parse_next(input)?;
    let options = input.state;
    frame_value(input, specifier, span, options)
}
