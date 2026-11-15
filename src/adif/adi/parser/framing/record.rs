//! Ordered framed records and record boundaries.

use std::ops::Range;

use super::field::PermissiveField;

/// A complete framed ADI record preserving every supplied field in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissiveRecord<'a> {
    pub(in crate::adif::adi::parser) span: Range<usize>,
    pub(in crate::adif::adi::parser) fields: Vec<PermissiveField<'a>>,
}

impl<'a> PermissiveRecord<'a> {
    /// Borrows every field occurrence in source order.
    pub fn fields(&self) -> &[PermissiveField<'a>] {
        &self.fields
    }

    /// Returns the byte range from the first descriptor through the closing EOR bracket.
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }
}
