//! Strict records and conversion of framed record contents.

use std::collections::HashMap;
use std::ops::Range;

use winnow::{
    Parser as _,
    stream::{Location, Stream as _},
    token::take_until,
};

use super::super::{
    error::{Result, StructuralError},
    framing,
};
use super::field::{Field, prepared_field};
use super::{Stream, declarations::UserDefinition, definitions};
use crate::adif::adi::lookup::{LookupError, single_match};

/// A complete ADI record containing at least one field.
///
/// Field names are unique within the record under ASCII case-insensitive
/// comparison. Its span starts at the first descriptor and ends after EOR's
/// closing `>`. The span excludes surrounding ignored text and line endings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record<'a> {
    pub(super) span: Range<usize>,
    pub(super) fields: Vec<Field<'a>>,
}

impl<'a> Record<'a> {
    /// Borrows all fields in their source order.
    pub fn fields(&self) -> &[Field<'a>] {
        &self.fields
    }

    /// Finds a field by ASCII case-insensitive name.
    ///
    /// Multiple matches produce an ambiguity error.
    pub fn field(&self, name: &str) -> Result<Option<&Field<'a>>, LookupError> {
        single_match(name, self.fields_named(name), Field::descriptor_span)
    }

    /// Iterates over matching fields in source order without a precedence rule.
    pub fn fields_named<'s>(&'s self, name: &str) -> impl Iterator<Item = &'s Field<'a>> {
        self.fields
            .iter()
            .filter(move |field| field.name().eq_ignore_ascii_case(name))
    }

    /// Returns the section's syntactic byte range without surrounding
    /// separators.
    ///
    /// Offsets refer to the input slice. The end is exclusive.
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }
}

pub(super) fn unique_field<'a, 'header>(
    input: &mut Stream<'a>,
    seen: &mut HashMap<String, Range<usize>>,
    users: &HashMap<String, &'header UserDefinition<'a>>,
) -> Result<Field<'a>, StructuralError<'header>> {
    let checkpoint = input.checkpoint();
    let (parsed, resolution) = prepared_field(input, users)?;
    let key = parsed.name().to_ascii_uppercase();

    if let Some(first_span) = seen.get(&key) {
        let cause = StructuralError::DuplicateField {
            field_name: parsed.name().to_owned(),
            first_span: first_span.clone(),
            second_span: parsed.descriptor_span(),
        };
        input.reset(&checkpoint);
        return Err(cause);
    }

    seen.insert(key, parsed.descriptor_span());
    definitions::register_application(&mut input.state, &parsed, &resolution);
    Ok(parsed)
}

pub(super) fn record<'a, 'header>(
    input: &mut Stream<'a>,
    users: &HashMap<String, &'header UserDefinition<'a>>,
) -> Result<Record<'a>, StructuralError<'header>> {
    let mut seen = HashMap::new();
    take_until(0.., '<').void().parse_next(input)?;
    let start = input.current_token_start();
    let (fields, end_span) = framing::fields(input, "<EOR>", 1, |input| {
        unique_field(input, &mut seen, users)
    })?;
    Ok(Record {
        fields,
        span: start..end_span.end,
    })
}
