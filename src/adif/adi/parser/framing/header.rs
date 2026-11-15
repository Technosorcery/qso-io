//! Framed headers and preamble decoding results.

use std::{borrow::Cow, ops::Range};

use winnow::Result;

use super::field::PermissiveField;
use crate::adif::DecodeError;

/// A framed header with ordered fields and a cached preamble decoding result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissiveHeader<'a> {
    pub(in crate::adif::adi::parser) span: Range<usize>,
    pub(in crate::adif::adi::parser) text_bytes: &'a [u8],
    pub(in crate::adif::adi::parser) text_span: Range<usize>,
    pub(in crate::adif::adi::parser) text: Result<Cow<'a, str>, DecodeError>,
    pub(in crate::adif::adi::parser) fields: Vec<PermissiveField<'a>>,
}

impl<'a> PermissiveHeader<'a> {
    /// Borrows every supplied header field in source order, including USERDEFn fields.
    pub fn fields(&self) -> &[PermissiveField<'a>] {
        &self.fields
    }

    /// Borrows original preamble bytes, regardless of decoding success.
    pub fn raw_text(&self) -> &'a [u8] {
        self.text_bytes
    }

    /// Returns the preamble's exclusive-end byte range.
    pub fn text_span(&self) -> Range<usize> {
        self.text_span.clone()
    }

    /// Returns the header's byte range through the closing EOH bracket.
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }

    /// Borrows cached preamble text or its error, relative to [`Self::raw_text`].
    pub fn text(&self) -> Result<&str, &DecodeError> {
        self.text.as_deref()
    }
}
