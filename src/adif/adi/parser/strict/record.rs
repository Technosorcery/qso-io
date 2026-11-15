//! Strict records and conversion of framed record contents.

use std::collections::HashMap;
use std::ops::Range;

use winnow::{
    Parser as _, Result,
    error::{ContextError, FromExternalError},
    stream::{Location, Stream as _},
    token::take_until,
};

use super::super::{error::AdiError, framing};
use super::field::{Field, prepared_field};
use super::{Stream, definitions};

/// A complete ADI record containing at least one field.
///
/// Field names are unique within the record under ASCII case-insensitive
/// comparison. Its span starts at the first descriptor and ends after EOR's
/// closing `>`, excluding surrounding ignored text and line endings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record<'a> {
    pub(super) span: Range<usize>,
    pub(super) fields: Vec<Field<'a>>,
}

impl<'a> Record<'a> {
    /// Finds a field by ASCII case-insensitive name.
    pub fn field(&self, name: &str) -> Option<&Field<'a>> {
        self.fields
            .iter()
            .find(|f| f.name().eq_ignore_ascii_case(name))
    }

    /// Borrows all fields in their original order.
    pub fn fields(&self) -> &[Field<'a>] {
        &self.fields
    }

    /// Returns the section's syntactic byte range, excluding surrounding separators.
    ///
    /// Offsets refer to the original input slice and the end is exclusive.
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }
}

pub(super) fn unique_field<'a>(
    input: &mut Stream<'a>,
    seen: &mut HashMap<String, Range<usize>>,
) -> Result<Field<'a>> {
    let checkpoint = input.checkpoint();
    let (parsed, resolution) = prepared_field(input)?;
    let key = parsed.name().to_ascii_uppercase();

    if let Some(first_span) = seen.get(&key) {
        let cause = AdiError::DuplicateField {
            field_name: parsed.name().to_owned(),
            first_span: first_span.clone(),
            second_span: parsed.descriptor_span(),
        };
        input.reset(&checkpoint);
        return Err(ContextError::from_external_error(input, cause));
    }

    seen.insert(key, parsed.descriptor_span());
    definitions::register_application(&mut input.state, &parsed, &resolution);
    Ok(parsed)
}

pub(super) fn record<'a>(input: &mut Stream<'a>) -> Result<Record<'a>> {
    let mut seen = HashMap::new();
    take_until(0.., '<').void().parse_next(input)?;
    let start = input.current_token_start();
    let (fields, end_span) =
        framing::fields(input, "<EOR>", 1, |input| unique_field(input, &mut seen))?;
    Ok(Record {
        fields,
        span: start..end_span.end,
    })
}
