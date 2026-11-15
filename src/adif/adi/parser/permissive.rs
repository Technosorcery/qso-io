//! Framing-only ADI parsing without value interpretation.

use framing::{header::PermissiveHeader, record::PermissiveRecord};

use super::{framing, options::ParseOpts};

/// A complete framed ADI header or record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissiveSection<'a> {
    /// Header contents terminated by EOH.
    Header(PermissiveHeader<'a>),
    /// A nonempty field sequence terminated by EOR.
    Record(PermissiveRecord<'a>),
}

/// Reads complete ADI sections while preserving unchecked contents.
///
/// Applies the supplied [`ParseOpts`] encoding and length unit throughout the
/// input. Names and indicators require UTF-8. Decoding failures remain on fields
/// and preambles when their boundaries are known.
/// Invalid lengths, truncation, missing terminators, and UTF-8 errors preventing
/// scalar boundary discovery are terminal. A terminal error is yielded once,
/// followed by permanent exhaustion. Duplicate fields remain in source order.
#[derive(Debug)]
pub struct PermissiveParser<'a> {
    sections: framing::Sections<'a, ParseOpts>,
}

impl<'a> PermissiveParser<'a> {
    /// Constructs a framing-only parser; input is read when iteration advances.
    pub fn new(input: &'a [u8], options: ParseOpts) -> Self {
        Self {
            sections: framing::Sections::new(input, options),
        }
    }
}

impl<'a> Iterator for PermissiveParser<'a> {
    type Item = std::result::Result<
        crate::adif::adi::parser::permissive::PermissiveSection<'a>,
        super::error::StructuralError,
    >;

    fn next(&mut self) -> Option<Self::Item> {
        use crate::adif::adi::parser::permissive::PermissiveSection;
        self.sections.next(
            |input| framing::header::parse(input).map(PermissiveSection::Header),
            |input| framing::record::parse(input).map(PermissiveSection::Record),
        )
    }
}

impl std::iter::FusedIterator for PermissiveParser<'_> {}
