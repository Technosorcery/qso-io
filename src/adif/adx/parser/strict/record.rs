//! Typed ADX QSO records.

use std::ops::Range;

use crate::adif::adx::lookup::{LookupError, single};

use super::super::identity::FieldIdentity;
use super::field::Field;

/// A complete typed ADX record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record<'a> {
    pub(crate) fields: Vec<Field<'a>>,
    pub(crate) span: Range<usize>,
}

impl<'a> Record<'a> {
    /// Returns the supplied fields in source order.
    pub fn fields(&self) -> &[Field<'a>] {
        &self.fields
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
        self.fields.iter().filter(move |field| {
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

    /// Returns fields with the requested program and field names in source
    /// order.
    ///
    /// Names use ASCII case-insensitive comparison.
    pub fn application_fields<'s>(
        &'s self,
        program: &str,
        name: &str,
    ) -> impl Iterator<Item = &'s Field<'a>> {
        self.fields.iter().filter(move |field| {
            matches!(
                field.identity(),
                FieldIdentity::Application { program_id, field_name }
                    if program_id.eq_ignore_ascii_case(program) && field_name.eq_ignore_ascii_case(name)
            )
        })
    }

    /// Returns one matching application field, or None if none matches.
    ///
    /// Returns an error if more than one field matches.
    pub fn application_field(
        &self,
        program: &str,
        name: &str,
    ) -> Result<Option<&Field<'a>>, LookupError> {
        single(self.application_fields(program, name), Field::element_span)
    }

    /// Returns user references with the requested target name in source order.
    ///
    /// Names use ASCII case-insensitive comparison.
    pub fn user_fields<'s>(&'s self, name: &str) -> impl Iterator<Item = &'s Field<'a>> {
        self.fields.iter().filter(move |field| {
            matches!(
                field.identity(),
                FieldIdentity::User(field_name)
                    if field_name.eq_ignore_ascii_case(name)
            )
        })
    }

    /// Returns one matching user reference, or None if none matches.
    ///
    /// Returns an error if more than one field matches.
    pub fn user_field(&self, name: &str) -> Result<Option<&Field<'a>>, LookupError> {
        single(self.user_fields(name), Field::element_span)
    }
}
