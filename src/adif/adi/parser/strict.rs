//! Typed ADI parsing and internal conversion from shared framing results.

use std::{borrow::Cow, collections::HashMap, sync::OnceLock};

use definitions::State;
use header::Header;
use record::Record;
use winnow::{LocatingSlice, Stateful};

use super::error::{Result, StructuralError};
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

/// A complete header borrowed from its parser or a record owning its fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Section<'a, 'header> {
    /// The parser-owned decoded header terminated by EOH.
    Header(&'header Header<'a>),
    /// A nonempty field sequence terminated by EOR.
    Record(Record<'a>),
}

type Stream<'is> = Stateful<LocatingSlice<&'is [u8]>, State>;

/// Reads complete ADI sections from a borrowed byte slice.
///
/// The parser applies the supplied [`ParseOpts`] throughout the input. Its
/// encoding selects text decoding and the character repertoire of declared text
/// types. See the [declared text table][text]. Its `field_length` selects the
/// unit for descriptor lengths.
///
/// [text]: crate::adif::adi::parser#strict-declared-text-and-encodings
///
/// Earlier sections remain usable if a later section fails while the source and
/// any borrowed header remain alive. The parser returns a structural error
/// once. It then returns no more sections. Decoding errors are terminal, also
/// in field values and header preambles.
///
/// Before it returns a section, the parser validates resolved standard,
/// USERDEF, and application-field types. The parser owns the header and lends
/// it through [`IntoIterator::into_iter`].
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
/// let mut sections = (&mut parser).into_iter();
/// let record = match sections
///     .next()
///     .expect("example should yield a record because it supplies CALL followed by EOR")
///     .expect("example supplies a valid CALL record")
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
/// assert!(sections.next().is_none());
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct StrictParser<'a> {
    sections: framing::Sections<'a, State>,
    header: OnceLock<Header<'a>>,
}

impl<'a> StrictParser<'a> {
    /// Constructs a parser with the supplied input and options.
    ///
    /// The parser starts when the iterator advances.
    pub fn new(input: &'a [u8], options: ParseOpts) -> Self {
        Self {
            header: OnceLock::new(),
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

impl<'a, 'header> IntoIterator for &'header mut StrictParser<'a> {
    type Item = std::result::Result<Section<'a, 'header>, error::StructuralError<'header>>;
    type IntoIter = Sections<'a, 'header>;

    /// Creates an iterator over the remaining complete ADI sections.
    ///
    /// Headers and constraint bounds in terminal errors borrow this parser's
    /// header. Completed records move to the caller and borrow only the source.
    /// Earlier results remain usable after later sections and failures while
    /// their borrowed data remains alive.
    ///
    /// Dropping the cursor preserves the current position and application-field
    /// history. A terminal error is returned once; later calls return None.
    /// Successful completion returns None.
    fn into_iter(self) -> Self::IntoIter {
        let mut users = HashMap::new();
        if let Some(header) = self.header.get() {
            for entry in header.entries() {
                if let header::HeaderEntry::UserDefinition(definition) = entry {
                    users.insert(definition.field_name().to_ascii_uppercase(), definition);
                }
            }
        }

        Sections {
            inner: &mut self.sections,
            users,
            header: &self.header,
        }
    }
}

/// An iterator over complete ADI sections borrowing its parser's header and
/// state.
#[derive(Debug)]
pub struct Sections<'a, 'header> {
    inner: &'header mut framing::Sections<'a, State>,
    users: HashMap<String, &'header declarations::UserDefinition<'a>>,
    header: &'header OnceLock<Header<'a>>,
}

impl<'a, 'header> Iterator for Sections<'a, 'header> {
    type Item = std::result::Result<Section<'a, 'header>, error::StructuralError<'header>>;

    /// Reads the next complete section or returns a terminal failure.
    ///
    /// A terminal error is returned once; later calls return None. Successful
    /// completion returns None.
    fn next(&mut self) -> Option<Self::Item> {
        let users = &mut self.users;
        let header = self.header;

        self.inner
            .next(
                |input| {
                    let parsed = header::header(input)?;
                    let header = header.get_or_init(|| parsed);

                    Ok(Section::Header(header))
                },
                |input| record::record(input, users).map(Section::Record),
            )
            .inspect(|result| {
                if let Ok(Section::Header(header)) = result {
                    for entry in header.entries() {
                        if let header::HeaderEntry::UserDefinition(definition) = entry {
                            users.insert(definition.field_name().to_ascii_uppercase(), definition);
                        }
                    }
                }
            })
    }
}

impl std::iter::FusedIterator for Sections<'_, '_> {}

fn decode_located<'a>(
    bytes: &'a [u8],
    encoding: ParseEncoding,
    start: usize,
) -> Result<Cow<'a, str>> {
    framing::decode_text(bytes, encoding).map_err(|cause| decoding_error(bytes, start, cause))
}

fn decoding_error(bytes: &[u8], start: usize, cause: DecodeError) -> StructuralError<'static> {
    let (offset, width) = match &cause {
        DecodeError::NonAscii { offset } => (*offset, 1),
        DecodeError::Utf8(error) => (
            error.valid_up_to(),
            error
                .error_len()
                .unwrap_or(bytes.len() - error.valid_up_to()),
        ),
    };
    StructuralError::Decoding {
        span: start + offset..start + offset + width,
        cause,
    }
}
