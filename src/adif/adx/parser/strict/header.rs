//! Typed ADX headers and their entries.

use std::ops::Range;

use crate::adif::adx::lookup::{LookupError, single};

use super::super::identity::FieldIdentity;
use super::declarations::UserDefinition;
use super::field::Field;

/// A complete typed ADX header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header<'a> {
    pub(crate) entries: Vec<HeaderEntry<'a>>,
    pub(crate) span: Range<usize>,
}

impl<'a> Header<'a> {
    /// Returns ordinary metadata fields in source order.
    pub fn fields(&self) -> impl Iterator<Item = &Field<'a>> {
        self.entries.iter().filter_map(|entry| match entry {
            HeaderEntry::Field(field) => Some(field),
            HeaderEntry::UserDefinition(_) => None,
        })
    }

    /// Returns metadata fields and declarations in source order.
    pub fn entries(&self) -> &[HeaderEntry<'a>] {
        &self.entries
    }

    /// Returns the section's byte range in the parser's input, including its
    /// tags and with an exclusive end.
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }

    /// Returns ordinary fields with the requested ADIF name in source order.
    ///
    /// Names use ASCII case-insensitive comparison.
    pub fn fields_named<'s>(&'s self, name: &str) -> impl Iterator<Item = &'s Field<'a>> {
        self.fields().filter(move |field| {
            matches!(
                field.identity(),
                FieldIdentity::Ordinary(field_name)
                    if field_name.eq_ignore_ascii_case(name)
            )
        })
    }

    /// Returns one matching ordinary field, or None if none matches.
    ///
    /// Returns an error if more than one field matches.
    pub fn field(&self, name: &str) -> Result<Option<&Field<'a>>, LookupError> {
        single(self.fields_named(name), Field::element_span)
    }
}

/// One header metadata field or user declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderEntry<'a> {
    /// An ordinary metadata field.
    Field(Field<'a>),

    /// A user declaration with separate identifier and target name.
    UserDefinition(UserDefinition<'a>),
}
