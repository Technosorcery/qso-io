//! Supplied XML attributes and their decoding results.

use std::{borrow::Cow, ops::Range};

use super::super::error::TextError;

/// A supplied XML attribute with original bytes and decoded text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute<'a> {
    pub(crate) name: &'a str,
    pub(crate) raw: &'a str,
    pub(crate) name_span: Range<usize>,
    pub(crate) value_span: Range<usize>,
    pub(crate) text: Result<Cow<'a, str>, TextError<'a>>,
}

impl<'a> Attribute<'a> {
    /// Returns the supplied qualified name.
    pub fn name(&self) -> &'a str {
        self.name
    }

    /// Borrows the original value without quotation marks.
    pub fn raw_data(&self) -> &'a [u8] {
        self.raw.as_bytes()
    }

    /// Returns the name's byte range in the parser's input, with an exclusive
    /// end.
    pub fn name_span(&self) -> Range<usize> {
        self.name_span.clone()
    }

    /// Returns the value's byte range in the parser's input, excluding
    /// quotation marks and with an exclusive end.
    pub fn value_span(&self) -> Range<usize> {
        self.value_span.clone()
    }

    /// Returns the complete decoded value, or the reason it is unavailable.
    pub fn text(&self) -> Result<&str, &TextError<'a>> {
        self.text.as_deref()
    }
}
