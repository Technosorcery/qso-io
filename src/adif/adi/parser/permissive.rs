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

/// Reads complete ADI sections with unchecked contents.
///
/// The parser applies the supplied [`ParseOpts`] encoding and length unit
/// throughout the input. Names and indicators must use UTF-8. Fields and
/// preambles keep decoding failures if the parser can find their boundaries.
///
/// Incorrect lengths, truncation, and missing terminators are terminal errors.
/// UTF-8 errors that prevent scalar boundary discovery are also terminal. The
/// parser returns a terminal error once. It then returns no more sections.
/// Duplicate fields remain in source order.
#[derive(Debug)]
pub struct PermissiveParser<'a> {
    sections: framing::Sections<'a, ParseOpts>,
}

impl<'a> PermissiveParser<'a> {
    /// Constructs a framing-only parser.
    ///
    /// The parser reads input when the iterator advances.
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
