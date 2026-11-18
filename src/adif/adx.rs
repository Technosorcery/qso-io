//! Incremental ADX parsing with original source bytes.
//!
//! ADX uses UTF-8 XML. The strict parser uses the ADIF 3.1.8 catalog to resolve
//! types. The permissive parser returns decoded XML text without ADIF value
//! checks. Both parsers retain the input bytes. Their source spans are byte
//! ranges in that input, with exclusive ends.
//!
//! # Typed records
//!
//! ```
//! use qso_io::adif::adx::parser::strict::{StrictParser, Section};
//! use qso_io::adif::value::Value;
//!
//! let input = b"<ADX><HEADER/><RECORDS><RECORD><FREQ>14.&#48;74</FREQ></RECORD></RECORDS></ADX>";
//!
//! let mut owner = StrictParser::new(input);
//! let mut parser = (&mut owner).into_iter();
//! let record = parser.by_ref().find_map(|outcome| {
//!     match outcome.expect("the supplied typed FREQ record should parse").data {
//!         Section::EndRecord(record) => Some(record),
//!         _ => None,
//!     }
//! }).expect("the input supplies a complete RECORD");
//!
//! let freq = record.field("FREQ")
//!     .expect("the record supplies only one FREQ field")
//!     .expect("the record supplies FREQ");
//!
//! assert_eq!(freq.text(), "14.074");
//! assert_eq!(freq.raw_data(), b"14.&#48;74");
//! assert!(matches!(freq.value(), Some(Value::Number(_))));
//! assert!(parser.by_ref().all(|outcome| outcome.is_ok()));
//! assert!(parser.next().is_none());
//! ```
//!
//! # Unchecked contents and warnings
//!
//! ```
//! use qso_io::adif::adx::parser::{error::TextError, identity::FieldIdentity};
//! use qso_io::adif::adx::parser::permissive::{PermissiveParser, PermissiveSection};
//!
//! let input = b"<ADX><HEADER/><RECORDS><RECORD><NAME_INTL>&operator;</NAME_INTL><APP PROGRAMID='logger' FIELDNAME='note'>before<em>after</em></APP></RECORD></RECORDS></ADX>";
//!
//! let mut parser = PermissiveParser::new(input);
//! let result = parser.by_ref()
//!     .map(|outcome| outcome.expect("the supplied XML is well-formed"))
//!     .find(|result| matches!(result.data, PermissiveSection::EndRecord(_)))
//!     .expect("the input supplies a complete RECORD");
//!
//! assert_eq!(result.warnings.len(), 2);
//!
//! let PermissiveSection::EndRecord(record) = result.data else {
//!     panic!("expected record");
//! };
//!
//! let name = record.field("NAME_INTL")
//!     .expect("the record supplies only one NAME_INTL field")
//!     .expect("the record supplies NAME_INTL");
//! assert!(matches!(name.text(), Err(TextError::CustomReference(_))));
//! assert_eq!(name.raw_data(), b"&operator;");
//!
//! let note = record.application_field("logger", "note")
//!     .expect("the record supplies only one logger/note application field")
//!     .expect("the record supplies the logger/note application field");
//! assert!(matches!(note.identity(), Some(FieldIdentity::Application { .. })));
//! assert!(matches!(note.text(), Err(TextError::NestedMarkup)));
//! assert_eq!(note.raw_data(), b"before<em>after</em>");
//! assert!(parser.by_ref().all(|outcome| outcome.is_ok()));
//! assert!(parser.next().is_none());
//! ```
//!
//! The parser decodes XML text, numeric references, and the five predefined XML
//! entities. Permissive parsing also returns CDATA contents with a warning.
//!
//! Custom references remain unexpanded. Nested markup remains intact and has no
//! scalar text. A missing identity attribute prevents complete identity lookup.
//!
//! Permissive parsing does not validate ADIF values. An unknown field name does
//! not produce a catalog warning.
//!
//! In permissive mode, custom references in section attributes produce warnings
//! on the start event. Strict mode rejects these references. Section attributes
//! remain available through the opening tag's source range. Namespace
//! declarations remain raw attributes and never supply APP/USERDEF identity or
//! TYPE metadata.
//!
//! DOCTYPE and literal entity definitions produce presence warnings in
//! permissive mode. Warning spans identify the original declaration bytes. The
//! parser does not apply replacement text or default attributes. It does not
//! fetch external resources or validate opaque DTD grammar.
//!
//! # Warnings before failure
//!
//! ```
//! use qso_io::adif::adx::parser::error::{ParseError, WarningKind};
//! use qso_io::adif::adx::parser::permissive::PermissiveParser;
//!
//! let input = b"<!DOCTYPE ADX><ADX><HEADER><PROGRAMID><![CDATA[logger]]></WRONG>";
//!
//! let mut parser = PermissiveParser::new(input);
//! let root = parser.next()
//!     .expect("the input supplies an ADX root")
//!     .expect("the root start precedes the malformed header");
//! assert!(matches!(root.warnings[0].kind, WarningKind::Doctype));
//!
//! let failure = parser.by_ref().find_map(Result::err)
//!     .expect("the mismatched WRONG end tag should cause a terminal error");
//! assert!(matches!(failure.error, ParseError::Xml { .. }));
//! assert!(matches!(failure.warnings[0].kind, WarningKind::Cdata));
//! assert!(parser.next().is_none());
//! ```
//!
//! Both parsers return a flat stream of section start/end events. Header and
//! record end events contain the complete parsed section. Start events and
//! collection/root end events carry tag byte ranges in the parser's input, with
//! exclusive ends. A self-closing section produces both a start event and an
//! end event. Its generated end boundary has a zero-width span at the end of
//! the tag.
//!
//! [attribute-text]: crate::adif::adx::parser::strict::attribute::Attribute::text
//!
//! Strict field and declaration attributes are borrowed through source-ordered
//! iterators. Their decoded text is available directly through
//! [`Attribute::text`][attribute-text]. Permissive attributes retain their
//! fallible text accessor.
//!
//! Reference and entity names in warnings and unavailable-text reasons borrow
//! the original input. These diagnostics can outlive the parser while that
//! input remains alive. Fixed missing-attribute names use static strings.
//!
//! Each result owns warnings encountered since the previous result, including
//! warnings outside sections. A fatal result owns all pending warnings and does
//! not return an unfinished section. Earlier results remain usable after a
//! later failure while their borrowed data remains alive.
//!
//! The strict parser owns its header. Iterating over `&mut parser` lends the
//! header and moves completed records to the caller. Header views borrow the
//! parser; records borrow only the input and own decoded text and parsed values
//! as needed. USERDEF declarations are consulted during parsing, not retained
//! in records. The permissive parser moves both headers and records to the
//! caller.
//!
//! Consume the iterator to exhaustion to check all section boundaries and
//! trailing input. Successful completion is `None`, with no completion item.
//! Failure is returned once, then the iterator permanently returns `None`.
//! Stopping early leaves the remaining input unchecked.

pub mod lookup;
pub mod parser;
