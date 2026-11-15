//! Strict headers, entries, and conversion of framed header contents.

use std::collections::HashMap;
use std::{borrow::Cow, ops::Range};

use winnow::{
    Parser as _, Result,
    error::{ContextError, FromExternalError, StrContext},
    stream::Stream as _,
    token::take_till,
};

use super::super::{error::AdiError, framing};
use super::declarations::UserDefinition;
use super::field::{Field, strict_descriptor, strict_frame_value};
use super::{Stream, declarations, decode_located, definitions};
use crate::adif::FieldSection;

/// A header field or USERDEF declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderEntry<'a> {
    /// A header field.
    Field(Field<'a>),
    /// A declaration for a user-defined record field.
    UserDefinition(UserDefinition<'a>),
}

impl HeaderEntry<'_> {
    /// Returns the original descriptor name, including its supplied casing.
    pub fn name(&self) -> &str {
        match self {
            Self::Field(f) => f.name(),
            Self::UserDefinition(d) => d.name(),
        }
    }

    /// Returns the descriptor's original byte range, with an exclusive end.
    pub fn descriptor_span(&self) -> Range<usize> {
        match self {
            Self::Field(f) => f.descriptor_span(),
            Self::UserDefinition(d) => d.descriptor_span(),
        }
    }

    /// Returns the payload's original byte range, with an exclusive end.
    pub fn value_span(&self) -> Range<usize> {
        match self {
            Self::Field(f) => f.value_span(),
            Self::UserDefinition(d) => d.value_span(),
        }
    }
}

/// A complete ADI header with its original preamble and ordered fields.
///
/// Its source span ends after EOH's closing `>` and excludes following
/// separators. Its preamble and fields have successfully decoded text.
/// Field names are unique within the header under ASCII case-insensitive
/// comparison. USERDEF target names are also unique under that comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header<'a> {
    pub(super) span: Range<usize>,
    pub(super) text_bytes: &'a [u8],
    pub(super) text_span: Range<usize>,
    pub(super) text: Cow<'a, str>,
    pub(super) entries: Vec<HeaderEntry<'a>>,
}

impl<'a> Header<'a> {
    /// Finds a header entry by ASCII case-insensitive name.
    pub fn entry(&self, name: &str) -> Option<&HeaderEntry<'a>> {
        self.entries
            .iter()
            .find(|e| e.name().eq_ignore_ascii_case(name))
    }
    /// Finds an ordinary header field; USERDEF entries use entry lookup.
    pub fn field(&self, name: &str) -> Option<&Field<'a>> {
        match self.entry(name)? {
            HeaderEntry::Field(f) => Some(f),
            HeaderEntry::UserDefinition(_) => None,
        }
    }

    /// Borrows header fields and declarations in their original order.
    pub fn entries(&self) -> &[HeaderEntry<'a>] {
        &self.entries
    }

    /// Borrows the original preamble bytes, including line endings and blanks.
    pub fn raw_text(&self) -> &'a [u8] {
        self.text_bytes
    }

    /// Returns the preamble's original byte range, ending before the first `<`.
    pub fn text_span(&self) -> Range<usize> {
        self.text_span.clone()
    }

    /// Returns the byte range from the preamble's start through EOH's closing `>`.
    ///
    /// Offsets refer to the original input slice and the end is exclusive.
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }

    /// Borrows the original decoded preamble retained during parsing.
    pub fn text(&self) -> &str {
        &self.text
    }
}

pub(super) fn header<'a>(input: &mut Stream<'a>) -> Result<Header<'a>> {
    let mut seen = HashMap::new();
    let parse_opts = input.state.parser_options;

    let (text_bytes, text_span) = take_till(0.., b'<')
        .with_span()
        .context(StrContext::Label("header text"))
        .parse_next(input)?;

    let text = decode_located(input, text_bytes, parse_opts.encoding, text_span.start)?;

    let (fields, end_span) = framing::fields(input, "<EOH>", 0, |input| {
        unique_header_entry(input, &mut seen)
    })?;
    take_till(0.., b'<').void().parse_next(input)?;

    Ok(Header {
        span: text_span.start..end_span.end,
        text_bytes,
        text_span,
        text,
        entries: fields,
    })
}

pub(super) fn header_entry<'a>(input: &mut Stream<'a>) -> Result<HeaderEntry<'a>> {
    let (raw_specifier, specifier, span) = strict_descriptor(input)?;
    if declarations::is_declaration(&specifier.name) {
        let f = strict_frame_value(input, raw_specifier, span)?;
        declarations::parse(input, f).map(HeaderEntry::UserDefinition)
    } else {
        let resolution =
            definitions::resolve(&input.state, FieldSection::Header, &specifier, span.clone())
                .map_err(|error| ContextError::from_external_error(input, error))?;
        let f = strict_frame_value(input, raw_specifier, span)?;
        definitions::parse_resolved(f, &resolution, input.state.parser_options.encoding)
            .map(HeaderEntry::Field)
            .map_err(|error| ContextError::from_external_error(input, error))
    }
}

pub(super) fn unique_header_entry<'a>(
    input: &mut Stream<'a>,
    seen: &mut HashMap<String, Range<usize>>,
) -> Result<HeaderEntry<'a>> {
    let checkpoint = input.checkpoint();
    let entry = header_entry(input)?;
    let key = entry.name().to_ascii_uppercase();

    if let Some(first) = seen.get(&key) {
        let cause = AdiError::DuplicateField {
            field_name: entry.name().to_owned(),
            first_span: first.clone(),
            second_span: entry.descriptor_span(),
        };
        input.reset(&checkpoint);
        return Err(ContextError::from_external_error(input, cause));
    }

    seen.insert(key, entry.descriptor_span());
    if let HeaderEntry::UserDefinition(definition) = &entry {
        definitions::register_user_definition(&mut input.state, definition)
            .map_err(|error| ContextError::from_external_error(input, error))?;
    }
    Ok(entry)
}
