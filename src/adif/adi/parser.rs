//! Strict and permissive parsing of length-delimited ADI input.
//!
//! [`strict::StrictParser`] reads complete ADI sections with typed values and
//! the crate's supported ADIF checks. [`permissive::PermissiveParser`] reads
//! complete sections without interpreting field contents or document
//! declarations. Both keep input bytes and source locations. They use
//! [`ParseOpts`](crate::adif::adi::parser::options::ParseOpts) for explicit
//! encoding and length interpretation.
//!
//! Permissive results keep duplicate fields, unchecked UTF-8 indicators, and
//! cached decoding errors when value boundaries are known. Strict decoding and
//! type failures terminate iteration. Invalid framing is terminal for both
//! parsers. Previously returned sections remain available.
//!
//! Terminal failures are [`error::StructuralError`] enum variants carrying
//! byte ranges in the parser's input and relevant context. Match variants
//! directly or use [`error::StructuralError::span`] for the primary location.
//! Standard, USERDEF and application type conflicts retain their distinct
//! origins. Underlying conversion and decoding causes remain available through
//! [`std::error::Error::source`].
//!
//! The strict parser owns one header. Iterating over `&mut parser` lends that
//! header and moves completed records to the caller. Range failures borrow
//! exact declaration bounds from the same header. Keep the parser alive while
//! using those header references or diagnostics; records borrow only the
//! source. The cursor returns a fatal error once, then None. Dropping a cursor
//! preserves the remaining input, declarations and application-field history.
//!
//! Single-match lookup returns `Result<Option<_>, adi::lookup::LookupError>`:
//! absence is `Ok(None)`, and multiple matching descriptors produce an
//! ambiguity error containing every matching descriptor span. `fields_named()`
//! iterates over matches in source order. Names use ASCII case-insensitive
//! comparison.
//!
//! [`StandardField`](crate::adif::StandardField) describes standard field
//! definitions. Reference types such as [`AntPath`](crate::adif::AntPath),
//! [`ArrlSection`](crate::adif::ArrlSection), [`Band`](crate::adif::Band) and
//! [`DxccEntity`](crate::adif::DxccEntity) describe enumeration identities and
//! their metadata. Parser membership checks use these same types.
//!
//! # Permissive contents
//!
//! Permissive fields provide cached `text() -> Result<&str, &DecodeError>` and
//! `raw_data()` without typed-value conversion. An empty supplied value has
//! empty raw bytes and `Ok("")` text. An absent indicator is `None`. An
//! explicitly empty indicator is `Some("")`. Header preambles follow the same
//! decoding rule. UTF-8 names and indicators are independent of the selected
//! value encoding.
//!
//! ```
//! use qso_io::adif::adi::parser::options::{ParseOpts};
//! use qso_io::adif::adi::parser::permissive::{PermissiveParser, PermissiveSection};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut parser = PermissiveParser::new(b"<K_INDEX:4>1.33<EOR>", ParseOpts::default());
//! let record = match parser.next() {
//!     Some(Ok(PermissiveSection::Record(record))) => record,
//!     other => panic!("expected the supplied complete record, got {other:?}"),
//! };
//! let field = record.field("K_INDEX")?
//!     .expect("record should contain K_INDEX because the input supplies that descriptor");
//! assert_eq!(field.text().map_err(Clone::clone)?, "1.33");
//! assert_eq!(field.raw_data(), b"1.33");
//! // The field keeps text without an ADIF Integer check.
//! # Ok(())
//! # }
//! ```
//!
//! # Strict defined values
//!
//! Standard header and QSO fields use the bundled ADIF 3.1.8 definitions even
//! without inline indicators. USERDEF header declarations supply record-field
//! types and constraints. These declarations take precedence over
//! application-field names. Application fields establish their type on first
//! occurrence. Later occurrences inherit that type if they have no indicator.
//! Explicit conflicts are terminal errors.
//!
//! [`Field::data_type`][strict::field::Field::data_type] reports the resolved
//! type. [`Field::type_indicator`][strict::field::Field::type_indicator]
//! reports only the supplied inline indicator. Unknown untyped fields keep
//! decoded text. Supplied zero-length contents have no value and bypass value
//! constraints, while descriptor consistency still applies. Lookup
//! distinguishes an empty field from an absent field. Supplied spelling and
//! bytes remain available.
//!
//! ADIF_VER does not change the fixed catalog. Membership checks use ADIF
//! member data, with historical entries. Country and QSO-date relationships
//! belong to validation. The parser does not check externally delegated sets,
//! such as DOKs. Partial subdivision tables do not reject unlisted values.
//!
//! SOTA checks cover only its text repertoire and a slash with nonempty
//! portions. The parser does not look up external reference assignments.
//!
//! ```
//! use qso_io::adif::adi::parser::options::{ParseOpts};
//! use qso_io::adif::adi::parser::strict::{StrictParser, Section};
//! use qso_io::adif::value::{Value};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut parser = StrictParser::new(b"<FREQ:6>14.074<NOTE:0><EOR>", ParseOpts::default());
//! let mut sections = (&mut parser).into_iter();
//! let record = match sections
//!     .next()
//!     .expect("example should yield a record because it supplies fields followed by EOR")
//!     .expect("example supplies valid typed fields")
//! {
//!     Section::Record(record) => record,
//!     other => panic!("expected record, got {other:?}"),
//! };
//! let frequency = record
//!     .field("freq")?
//!     .expect("record should contain FREQ from the supplied descriptor");
//! assert_eq!(frequency.data_type(), Some(qso_io::adif::DataType::Number));
//! assert_eq!(frequency.type_indicator(), None);
//! assert!(matches!(frequency.value(), Some(Value::Number(_))), "expected Number, got {:?}", frequency.value());
//! assert_eq!(frequency.text(), "14.074");
//! assert!(
//!     record
//!         .field("NOTE")?
//!         .expect("record should contain NOTE because the example supplies <NOTE:0>")
//!         .value()
//!         .is_none()
//! );
//! assert!(record.field("MISSING")?.is_none());
//! # Ok(())
//! # }
//! ```
//!
//! # Strict declared text and encodings
//!
//! For strict parsing, [`ParseOpts::encoding`][options::ParseOpts::encoding]
//! selects decoding and the character repertoire for declared String (`S`),
//! MultilineString (`M`) and Enumeration (`E`) values. These are qso-io's
//! explicit compatibility rules:
//!
//! | Encoding | String and Enumeration | MultilineString |
//! | --- | --- | --- |
//! | ASCII | U+0020–U+007E | Same characters plus CRLF pairs |
//! | Latin-1 | U+0020–U+007E and U+00A0–U+00FF | Same characters plus CRLF pairs |
//! | UTF-8 | Valid Unicode except CR and LF | Same characters plus CRLF pairs |
//!
//! Strict parsing with all encodings rejects lone CR/LF in MultilineString.
//! UTF-8 permits other controls, such as TAB, NUL, DEL and C1. ASCII and
//! Latin-1 use the narrower ranges above. The UTF-8 compatibility rule follows
//! ADIF's international-text definitions. The `I` and `G` indicators themselves
//! are unavailable in ADI. Contents with no resolved type use decoding alone.
//! USERDEF enumeration members use the String rules above. They keep each
//! member's spelling and order.
//!
//! # User-field declarations
//!
//! Strict headers provide fields and USERDEFn declarations through `entries()`.
//! A declaration describes a record field. It keeps its optional indicator and
//! parsed constraints separate from its supplied text.
//!
//! ```
//! use qso_io::adif::{ DataType};
//! use qso_io::adif::adi::parser::options::{ParseOpts};
//! use qso_io::adif::adi::parser::strict::header::{HeaderEntry};
//! use qso_io::adif::adi::parser::strict::{StrictParser, Section};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut parser = StrictParser::new(b"P<USERDEF1:3:N>EPC<EOH>", ParseOpts::default());
//! let mut sections = (&mut parser).into_iter();
//! let header = match sections
//!     .next()
//!     .expect("example should yield a header because it starts with a preamble")
//!     .expect("example supplies a valid declaration")
//! {
//!     Section::Header(header) => header,
//!     other => panic!("expected header, got {other:?}"),
//! };
//! let definition = match &header.entries()[0] {
//!     HeaderEntry::UserDefinition(definition) => definition,
//!     other => panic!("expected USERDEF declaration, got {other:?}"),
//! };
//! assert_eq!(definition.field_name(), "EPC");
//! assert_eq!(definition.data_type(), Some(DataType::Number));
//! assert_eq!(definition.text(), "EPC");
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod framing;
pub mod options;
pub mod permissive;
pub mod strict;
