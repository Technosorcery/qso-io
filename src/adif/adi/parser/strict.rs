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
/// The parser applies the supplied [`ParseOpts`] throughout the input. Its
/// encoding selects text decoding and the character repertoire of declared text
/// types. See the [declared text table](crate::adif::adi::parser#strict-declared-text-and-encodings).
/// Its `field_length` selects the unit for descriptor lengths.
///
/// Earlier sections remain usable if a later section fails. The parser returns
/// a structural error once. It then returns no more sections. Decoding errors
/// are terminal, also in field values and header preambles.
///
/// Before it returns a section, the parser validates resolved standard, USERDEF,
/// and application-field types. The parser keeps document rules independently
/// of the returned headers.
///
/// Standard enumeration membership uses ASCII case-insensitive comparison
/// without trimming or Unicode normalization. The parser accepts historical
/// enumeration members independently of QSO-date applicability. Primary
/// subdivision membership uses the supplied country groups independently of
/// record DXCC. It does not identify or infer a country.
///
/// Recommended enumerations for String fields such as CONTEST_ID remain
/// advisory. The parser first tries CREDIT fields as CreditList. If CreditList
/// grammar or membership checks fail, it tries the legacy AwardList.
///
/// Decoding rejects incorrect input without replacement characters. ASCII
/// failures identify the first non-ASCII byte. UTF-8 failures identify the
/// incorrect or incomplete sequence. The parser frames byte-length values
/// before decoding. For UTF-8 scalar lengths, it traverses complete Unicode
/// scalar values to find the value boundary.
///
/// A nonempty input whose first byte is not `<` starts with a header preamble.
/// It must have an EOH terminator. The parser reads input that starts with `<`
/// as records without a header. It ignores text between fields or sections up
/// to the next `<`. Declared field-value lengths take precedence over embedded
/// delimiters.
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
///         .field("call")?
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
    /// Constructs a parser with the supplied input and options.
    ///
    /// The parser starts when the iterator advances.
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
