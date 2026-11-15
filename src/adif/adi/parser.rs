//! Strict and permissive parsing of length-delimited ADI input.
//!
//! [`strict::StrictParser`] reads complete ADI sections with typed values and the
//! crate's supported ADIF checks. [`permissive::PermissiveParser`] reads complete
//! sections without interpreting field contents or document declarations.
//! Both preserve original bytes and source locations and use [`ParseOpts`](crate::adif::adi::parser::options::ParseOpts)
//! for explicit encoding and length interpretation.
//!
//! Permissive results retain duplicate fields, unchecked UTF-8 indicators, and
//! cached decoding errors when value boundaries are known. Strict decoding and
//! type failures terminate iteration. Invalid framing is terminal for both
//! parsers; previously yielded sections remain available.
//!
//! Single-match lookup returns `Result<Option<_>, adi::lookup::LookupError>`: absence
//! is `Ok(None)`, and multiple matching descriptors produce an ambiguity error
//! containing every matching descriptor span. `fields_named()` iterates over
//! matches in source order. Names use ASCII case-insensitive comparison.
//!
//! [`StandardField`](crate::adif::StandardField) describes standard field definitions. Reference types such
//! as [`AntPath`](crate::adif::AntPath), [`ArrlSection`](crate::adif::ArrlSection), [`Band`](crate::adif::Band) and [`DxccEntity`](crate::adif::DxccEntity) describe enumeration
//! identities and their metadata; parser membership checks use these same types.
//!
//! # Permissive contents
//!
//! Permissive fields expose cached `text() -> Result<&str, &DecodeError>` and
//! `raw_data()` without typed-value conversion. An empty supplied value has
//! empty raw bytes and `Ok("")` text. An absent indicator is `None`; an explicitly
//! empty indicator is `Some("")`. Header preambles follow the same decoding rule.
//! UTF-8 names and indicators are independent of the selected value encoding.
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
//! // Text is retained without asserting that it is an ADIF Integer.
//! # Ok(())
//! # }
//! ```
//!
//! # Strict defined values
//!
//! Standard header and QSO fields use the bundled ADIF 3.1.8 definitions even
//! when their inline indicators are omitted. USERDEF header declarations supply
//! record-field types and constraints, taking precedence over application-field
//! names. Application fields establish their type on first occurrence; later
//! omitted indicators inherit it and explicit conflicts are terminal errors.
//!
//! [`Field::data_type`](crate::adif::adi::parser::strict::field::Field::data_type) reports the resolved type; [`Field::type_indicator`](crate::adif::adi::parser::strict::field::Field::type_indicator)
//! reports only the supplied inline indicator. Unknown untyped fields retain
//! decoded text. Supplied zero-length contents have no value and bypass value
//! constraints, while descriptor consistency still applies. Lookup distinguishes
//! an empty field from an absent field. Original spelling and bytes remain available.
//!
//! The catalog is fixed regardless of ADIF_VER. Membership checks use member data
//! supplied by ADIF, retaining historical entries; country and QSO-date
//! relationships belong to validation. Externally delegated sets, including DOKs,
//! are not checked. Partial subdivision tables do not reject unlisted values.
//! SOTA checks are limited to its text repertoire and a slash with nonempty
//! portions; external reference assignments are not looked up.
//!
//! ```
//! use qso_io::adif::adi::parser::options::{ParseOpts};
//! use qso_io::adif::adi::parser::strict::{StrictParser, Section};
//! use qso_io::adif::value::{Value};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut parser = StrictParser::new(b"<FREQ:6>14.074<NOTE:0><EOR>", ParseOpts::default());
//! let record = match parser
//!     .next()
//!     .expect("example should yield a record because it supplies fields followed by EOR")?
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
//! For strict parsing, [`ParseOpts::encoding`](crate::adif::adi::parser::options::ParseOpts::encoding) selects decoding and the character
//! repertoire for declared String (`S`), MultilineString (`M`) and Enumeration
//! (`E`) values. These are qso-io's explicit compatibility rules:
//!
//! | Encoding | String and Enumeration | MultilineString |
//! | --- | --- | --- |
//! | ASCII | U+0020–U+007E | Same characters plus CRLF pairs |
//! | Latin-1 | U+0020–U+007E and U+00A0–U+00FF | Same characters plus CRLF pairs |
//! | UTF-8 | Valid Unicode except CR and LF | Same characters plus CRLF pairs |
//!
//! Strict parsing with all encodings rejects lone CR/LF in MultilineString. UTF-8 permits other controls,
//! including TAB, NUL, DEL and C1; ASCII and Latin-1 use the narrower ranges
//! above. The UTF-8 compatibility rule follows ADIF's international-text
//! definitions. The `I` and `G` indicators themselves are unavailable in ADI.
//! Unresolved untyped contents use decoding alone. USERDEF enumeration members use
//! the String rules above, retaining each member's spelling and order.
//!
//! # User-field declarations
//!
//! Strict headers expose fields and USERDEFn declarations through
//! `entries()`. A declaration describes a record field; its optional indicator
//! and parsed constraints are retained separately from its original text.
//!
//! ```
//! use qso_io::adif::{ DataType};
//! use qso_io::adif::adi::parser::options::{ParseOpts};
//! use qso_io::adif::adi::parser::strict::header::{HeaderEntry};
//! use qso_io::adif::adi::parser::strict::{StrictParser, Section};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut parser = StrictParser::new(b"P<USERDEF1:3:N>EPC<EOH>", ParseOpts::default());
//! let header = match parser
//!     .next()
//!     .expect("example should yield a header because it starts with a preamble")?
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
