//! Supplied XML attributes with successfully decoded text.

use std::{borrow::Cow, ops::Range};

/// A supplied XML attribute with successfully decoded text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute<'a> {
    pub(super) name: &'a str,
    pub(super) raw: &'a str,
    pub(super) name_span: Range<usize>,
    pub(super) value_span: Range<usize>,
    pub(super) text: Cow<'a, str>,
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

    /// Borrows the successfully decoded value.
    pub fn text(&self) -> &str {
        &self.text
    }

    pub(super) fn raw_text(&self) -> &'a str {
        self.raw
    }
}
