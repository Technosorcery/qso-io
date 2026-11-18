//! Read Amateur Data Interchange Format (ADIF) files.
//!
//! The crate reads ADI text and ADX XML. The parsers keep original field bytes
//! and report byte ranges in the input. The field definitions follow
//! ADIF 3.1.8.
//!
//! # Select a parser
//!
//! Use `StrictParser` to check values for supported ADIF types.
//! It returns typed values.
//! Use `PermissiveParser` to check file structure and decode text without ADIF
//! value checks.
//!
//! # Read ADI
//!
//! This example reads `K_INDEX` without an ADIF value check. It keeps `1.33`,
//! although `K_INDEX` requires an integer.
//!
//! ```rust
//! use qso_io::adif::adi::parser::options::ParseOpts;
//! use qso_io::adif::adi::parser::permissive::{
//!     PermissiveParser, PermissiveSection,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let input = b"<K_INDEX:4>1.33<EOR>";
//! let mut parser = PermissiveParser::new(input, ParseOpts::default());
//! let record = match parser.next() {
//!     Some(Ok(PermissiveSection::Record(record))) => record,
//!     other => panic!("expected the complete record in the input, got {other:?}"),
//! };
//! let field = record.field("K_INDEX")?
//!     .expect("the input supplies K_INDEX");
//! let text = field.text().expect("K_INDEX contains only ASCII bytes");
//!
//! assert_eq!(text, "1.33");
//! assert_eq!(field.raw_data(), b"1.33");
//! # Ok(())
//! # }
//! ```
//!
//! Use [`ParseOpts`](adif::adi::parser::options::ParseOpts) to select an ADI
//! text encoding and the unit for field lengths.
//!
//! For typed ADI values, use
//! [`StrictParser`](adif::adi::parser::strict::StrictParser).
//! Iterate over `&mut parser` with a `for` loop.
//!
//! # Read ADX
//!
//! ADX input must use UTF-8.
//! ADX parsers return start and end events for document sections.
//! Each successful result contains `data` and `warnings`.
//! This example reads complete records with the strict parser:
//!
//! ```rust
//! use qso_io::adif::adx::parser::strict::{Section, StrictParser};
//!
//! let input = concat!(
//!     "<ADX><HEADER/><RECORDS>",
//!     "<RECORD><CALL>W1AW</CALL></RECORD>",
//!     "</RECORDS></ADX>",
//! );
//! let mut parser = StrictParser::new(input.as_bytes());
//!
//! for outcome in &mut parser {
//!     let result = outcome.expect("the input is valid ADX");
//!     if let Section::EndRecord(record) = result.data {
//!         let call = record.field("CALL")
//!             .expect("the input supplies one CALL field")
//!             .expect("the input supplies CALL");
//!         assert_eq!(call.text(), "W1AW");
//!     }
//! }
//! ```
//!
//! Read all events to check the complete document.
//! The permissive ADX parser returns warnings with an event or error.
//!
//! # API reference
//!
//! - [ADI parsers](adif::adi::parser): text encoding, field lengths, and
//!   errors.
//! - [ADX parsers](adif::adx): XML decoding, section events, and warnings.
//! - [`StandardField`](adif::StandardField): standard field definitions.
//! - [`value`](adif::value): typed ADIF values.

pub mod adif;
