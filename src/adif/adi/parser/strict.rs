//! Typed ADI parsing and internal conversion from shared framing results.

use std::borrow::Cow;

use definitions::State;
use header::Header;
use record::Record;
use winnow::{
    LocatingSlice, Result, Stateful,
    error::{ContextError, FromExternalError},
};

use super::error::AdiError;
use super::{
    error, framing,
    options::{ParseEncoding, ParseOpts},
};
use crate::adif::DecodeError;

pub mod declarations;
mod definitions;
pub mod field;
pub mod header;
pub mod record;
mod values;

/// A complete header or record from an ADI input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Section<'a> {
    /// A complete decoded header terminated by EOH.
    Header(Header<'a>),
    /// A nonempty field sequence terminated by EOR.
    Record(Record<'a>),
}

type Stream<'is> = Stateful<LocatingSlice<&'is [u8]>, State>;

/// Reads complete ADI sections from a borrowed byte slice.
///
/// Applies the supplied [`ParseOpts`] throughout the input. Uses its encoding
/// for text decoding and the character repertoire of declared text types; see
/// the [declared text table](crate::adif::adi::parser#strict-declared-text-and-encodings).
/// Uses its `field_length` to interpret descriptor lengths.
///
/// Earlier sections remain usable if a later section fails. A structural error
/// is yielded once, followed by permanent exhaustion. Decoding errors are
/// terminal, including in field values and header preambles. Resolved standard,
/// USERDEF and application-field types are validated before a section is yielded.
/// Document rules belong to this parser independently of returned headers.
///
/// Historical enumeration members are accepted independently of QSO-date
/// applicability. Primary subdivision membership spans the supplied country
/// groups independently of record DXCC, without identifying or inferring a
/// country. Recommended enumerations for String fields such as CONTEST_ID remain
/// advisory during parsing.
///
/// Decoding rejects invalid input without replacement. ASCII failures identify
/// the first non-ASCII byte; UTF-8 failures identify the invalid or incomplete
/// sequence. Byte-length values are framed before decoding, while UTF-8 scalar
/// lengths require valid scalar traversal to locate the value boundary.
///
/// A nonempty input whose first byte is not `<` starts with a header preamble
/// and requires an EOH terminator. Input starting with `<` is parsed as records
/// without a header. Text between fields or sections is ignored up to the next
/// `<`; declared field-value lengths take precedence over embedded delimiters.
///
/// ```
/// use qso_io::adif::adi::parser::options::{ParseOpts, ParseEncoding, LengthUnit};
/// use qso_io::adif::adi::parser::strict::{StrictParser, Section};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let options = ParseOpts { encoding: ParseEncoding::Latin1, field_length: LengthUnit::Bytes };
/// let mut parser = StrictParser::new(b"<CALL:4>W1AW<EOR>", options);
/// let record = match parser
///     .next()
///     .expect("example should yield a record because it supplies CALL followed by EOR")?
/// {
///     Section::Record(record) => record,
///     other => panic!("expected record, got {other:?}"),
/// };
/// assert_eq!(
///     record
///         .field("call")
///         .expect("record should contain CALL from the supplied descriptor")
///         .text(),
///     "W1AW"
/// );
/// assert!(parser.next().is_none());
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct StrictParser<'a> {
    sections: framing::Sections<'a, State>,
}

impl<'a> StrictParser<'a> {
    /// Constructs a parser using the supplied input and options.
    ///
    /// Parsing begins when the iterator advances.
    pub fn new(input: &'a [u8], options: ParseOpts) -> Self {
        Self {
            sections: framing::Sections::new(
                input,
                State {
                    parser_options: options,
                    ..State::default()
                },
            ),
        }
    }
}

impl<'a> Iterator for StrictParser<'a> {
    type Item = std::result::Result<Section<'a>, error::StructuralError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.sections.next(
            |input| header::header(input).map(Section::Header),
            |input| record::record(input).map(Section::Record),
        )
    }
}

impl std::iter::FusedIterator for StrictParser<'_> {}

fn decode_located<'a>(
    input: &Stream<'a>,
    bytes: &'a [u8],
    encoding: ParseEncoding,
    start: usize,
) -> Result<Cow<'a, str>> {
    framing::decode_text(bytes, encoding).map_err(|cause| {
        ContextError::from_external_error(input, decoding_error(bytes, start, cause))
    })
}

fn decoding_error(bytes: &[u8], start: usize, cause: DecodeError) -> AdiError {
    let (offset, width) = match &cause {
        DecodeError::NonAscii { offset } => (*offset, 1),
        DecodeError::Utf8(error) => (
            error.valid_up_to(),
            error
                .error_len()
                .unwrap_or(bytes.len() - error.valid_up_to()),
        ),
    };
    AdiError::Decoding {
        span: start + offset..start + offset + width,
        cause,
    }
}
