//! Section events, unchecked fields and incremental XML decoding.

use std::{iter::FusedIterator, ops::Range};

use super::{ParseFailure, ParseResult, framing::Sections};

pub mod attribute;
pub mod field;
pub mod header;
pub mod record;

use header::PermissiveHeader;
use record::PermissiveRecord;

/// A section boundary in an ADX document.
///
/// Start events carry the opening tag's byte range in the parser's input, with
/// an exclusive end. EndAdx and EndRecords carry the closing tag's range in the
/// same input, or a zero-width span at the end of a self-closing tag.
///
/// EndHeader and EndRecord contain the complete section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissiveSection<'a> {
    /// The opening ADX tag.
    StartAdx(Range<usize>),

    /// The opening HEADER tag.
    StartHeader(Range<usize>),

    /// The complete document header.
    EndHeader(PermissiveHeader<'a>),

    /// The opening RECORDS tag.
    StartRecords(Range<usize>),

    /// The opening RECORD tag.
    StartRecord(Range<usize>),

    /// One complete QSO record.
    EndRecord(PermissiveRecord<'a>),

    /// The end of the RECORDS collection.
    EndRecords(Range<usize>),

    /// The end of the ADX root.
    EndAdx(Range<usize>),
}

/// Reads ADX section events with unchecked ADIF values.
///
/// Each outcome includes warnings found since the previous outcome. Warnings
/// outside sections accompany the next event or fatal error. A fatal error is
/// returned once. Later calls return None. Successful completion returns None
/// after the document and trailing input are checked. Stopping early leaves the
/// remaining input unchecked.
pub struct PermissiveParser<'a> {
    inner: Sections<'a>,
}

impl<'a> PermissiveParser<'a> {
    /// Creates a lazy parser over the original UTF-8 bytes.
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            inner: Sections::new(input, false),
        }
    }
}

impl<'a> Iterator for PermissiveParser<'a> {
    type Item = Result<ParseResult<'a, PermissiveSection<'a>>, ParseFailure<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next_section()
    }
}

impl FusedIterator for PermissiveParser<'_> {}
