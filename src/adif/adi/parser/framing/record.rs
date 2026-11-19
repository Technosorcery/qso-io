//! Ordered framed records and record boundaries.

use std::ops::Range;

use winnow::{Parser as _, stream::Location, token::take_until};

use super::super::{error::Result, options::ParseOpts};
use super::field::PermissiveField;
use super::{Input, field, fields};
use crate::adif::adi::lookup::LookupError;
use crate::adif::adi::lookup::single_match;

/// A complete framed ADI record with every supplied field in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissiveRecord<'a> {
    pub(in crate::adif::adi::parser) span: Range<usize>,
    pub(in crate::adif::adi::parser) fields: Vec<PermissiveField<'a>>,
}

impl<'a> PermissiveRecord<'a> {
    /// Finds a field by ASCII case-insensitive name.
    ///
    /// Multiple matches produce an ambiguity error.
    pub fn field(&self, name: &str) -> Result<Option<&PermissiveField<'a>>, LookupError> {
        single_match(
            name,
            self.fields_named(name),
            PermissiveField::descriptor_span,
        )
    }

    /// Iterates over all matching fields in source order without a precedence
    /// rule.
    pub fn fields_named<'s>(&'s self, name: &str) -> impl Iterator<Item = &'s PermissiveField<'a>> {
        self.fields
            .iter()
            .filter(move |field| field.name().eq_ignore_ascii_case(name))
    }

    /// Borrows every field occurrence in source order.
    pub fn fields(&self) -> &[PermissiveField<'a>] {
        &self.fields
    }

    /// Returns the byte range in the parser's input from the first descriptor
    /// through EOR's closing `>`, with an exclusive end.
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }
}

pub(in crate::adif::adi::parser) fn parse<'a>(
    input: &mut Input<'a, ParseOpts>,
) -> Result<crate::adif::adi::parser::framing::record::PermissiveRecord<'a>> {
    take_until(0.., '<').void().parse_next(input)?;
    let start = input.current_token_start();
    let (fields, end_span) = fields(input, "<EOR>", 1, field::parse)?;
    Ok(
        crate::adif::adi::parser::framing::record::PermissiveRecord {
            span: start..end_span.end,
            fields,
        },
    )
}
