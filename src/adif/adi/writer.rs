//! Write an ADI document to a synchronous [output sink](std::io::Write).
//!
//! Create a [`Writer`] with an output destination and an optional [`Header`].
//! Write at least one [`Record`] with [`Writer::write_record`].
//! [`Writer::finish`] flushes the output and returns the destination passed
//! to [`Writer::new`].
//!
//! Supply the text that goes inside each field. For example, pass
//! `"14.074000"` as the value for `FREQ` to emit `<FREQ:9>14.074000`.
//! The writer generates the tags, lengths, section terminators, and CRLF
//! separators. Field values use printable ASCII. Multiline values may
//! also contain CRLF pairs.
//!
//! The writer checks field names, placement, character repertoires,
//! application type consistency, and constraints declared in the header.
//! Callers decide which values meet catalog and receiving-service rules.
//! For example, the writer accepts a printable `MODE` value outside the
//! catalog, which the [strict ADI reader](super::parser::strict::StrictParser)
//! would reject.
//!
//! # Example
//!
//! Write a header and a record with all three kinds of field.
//!
//! ```rust
//! use qso_io::adif::{
//!     DataType, StandardField,
//!     adi::writer::{
//!         Header, HeaderField, Record, RecordField, RecordFieldName,
//!         UserFieldDefinition, Writer,
//!     },
//! };
//!
//! # fn main() -> Result<(), qso_io::adif::adi::writer::WriteError> {
//! let header_fields = [
//!     HeaderField::Standard {
//!         field: StandardField::AdifVer,
//!         value: "3.1.8",
//!     },
//!     HeaderField::Standard {
//!         field: StandardField::ProgramId,
//!         value: "EXAMPLE",
//!     },
//!     HeaderField::UserDefined(UserFieldDefinition {
//!         name: "EPC",
//!         data_type: Some(DataType::Number),
//!         constraint: None,
//!     }),
//! ];
//! let header = Header {
//!     preamble: "Example ADI export",
//!     fields: &header_fields,
//! };
//! let record_fields = [
//!     RecordField {
//!         name: RecordFieldName::Standard(StandardField::Call),
//!         value: "W7HEX",
//!     },
//!     RecordField {
//!         name: RecordFieldName::ApplicationDefined {
//!             program_id: "EXAMPLE",
//!             field_name: "SESSION",
//!             data_type: Some(DataType::String),
//!         },
//!         value: "hill-1",
//!     },
//!     RecordField {
//!         name: RecordFieldName::UserDefined { name: "EPC" },
//!         value: "217",
//!     },
//! ];
//!
//! let mut writer = Writer::new(Vec::new(), Some(header))?;
//! writer.write_record(Record { fields: &record_fields })?;
//! let bytes = writer.finish()?;
//! assert_eq!(
//!     bytes,
//!     b"Example ADI export\r\n<ADIF_VER:5>3.1.8\r\n<PROGRAMID:7>EXAMPLE\r\n\
//!       <USERDEF1:3:N>EPC\r\n<EOH>\r\n<CALL:5>W7HEX\r\n\
//!       <APP_EXAMPLE_SESSION:6:S>hill-1\r\n<EPC:3>217\r\n<EOR>\r\n"
//! );
//! # Ok(())
//! # }
//! ```
//!

use std::{
    collections::{HashMap, HashSet},
    io::{self, Write},
};

use thiserror::Error;

pub use crate::adif::UserFieldDefinition;
use crate::adif::{DataType, FieldSection, StandardField, text::check_text};

use super::{application::resolve_application_type, parser::options::ParseEncoding};

mod declarations;
use declarations::Declarations;

/// An ADI header with a preamble and ordered fields.
#[derive(Debug, Clone, Copy)]
pub struct Header<'a> {
    /// Nonempty printable ASCII without `<`. The writer adds the separator
    /// before the first tag.
    pub preamble: &'a str,
    /// Standard metadata and user declarations in output order.
    pub fields: &'a [HeaderField<'a>],
}

/// A standard header field or a declaration for a user-defined record field.
#[derive(Debug, Clone, Copy)]
pub enum HeaderField<'a> {
    /// A standard field defined for the header.
    Standard {
        /// Catalog entry for the header field.
        field: StandardField,
        /// Text to emit as the field value.
        value: &'a str,
    },
    /// A declaration for a user-defined record field.
    ///
    /// The writer numbers user declarations consecutively from 1 in header
    /// order. Enumeration members that contain a colon require
    /// [`DataType::Enumeration`]. A range declaration can omit its type.
    UserDefined(UserFieldDefinition<'a>),
}

/// A QSO record in an ADI document.
#[derive(Debug, Clone, Copy)]
pub struct Record<'a> {
    /// One or more fields in output order.
    pub fields: &'a [RecordField<'a>],
}

/// A record field and its supplied output text.
#[derive(Debug, Clone, Copy)]
pub struct RecordField<'a> {
    /// Field identity and any supplied application type.
    pub name: RecordFieldName<'a>,
    /// Text to emit as the field value.
    pub value: &'a str,
}

/// The identity of a field in an ADI record.
#[derive(Debug, Clone, Copy)]
pub enum RecordFieldName<'a> {
    /// A standard QSO field from the ADIF catalog.
    Standard(StandardField),
    /// An application field emitted as `APP_<program_id>_<field_name>`.
    ///
    /// The first successful occurrence establishes the type for that name,
    /// matched without regard to ASCII case. Later explicit types must match.
    /// The writer rejects application names that match a user declaration.
    ApplicationDefined {
        /// Nonempty program component without underscores. qso-io uses the
        /// first underscore after `APP_` as the separator.
        program_id: &'a str,
        /// Nonempty field component, which may contain underscores.
        field_name: &'a str,
        /// Type with an ADI indicator. With `None` the first occurrence
        /// defaults to multiline string. Later occurrences inherit the
        /// established type, and the writer emits its indicator if that
        /// type differs from multiline string.
        data_type: Option<DataType>,
    },
    /// A field whose type and optional constraint come from a header declaration.
    ///
    /// The record descriptor contains the field name and length. An untyped
    /// declaration allows printable ASCII and CRLF pairs. Nonempty values
    /// must match a declared enumeration without regard to case or fall
    /// inside its inclusive numeric range.
    UserDefined {
        /// Target of a header declaration, matched without regard to ASCII case.
        name: &'a str,
    },
}

/// A document validation or output failure.
#[derive(Debug, Error)]
pub enum WriteError {
    /// The supplied preamble is empty, contains a tag opener, or uses invalid characters.
    #[error("the header preamble must be nonempty printable ASCII without '<'")]
    InvalidPreamble,
    /// An ADI record requires at least one field.
    #[error("an ADI record must contain at least one field")]
    EmptyRecord,
    /// A complete ADI document requires at least one record.
    #[error("an ADI document must contain at least one record")]
    NoRecords,
    /// A standard field was supplied in the wrong section.
    #[error("{} is not defined for this ADI section", field.as_str())]
    WrongSection {
        /// The field's catalog entry.
        field: StandardField,
    },
    /// A section contains another field with the same case-insensitive name.
    #[error("duplicate field {field:?}")]
    DuplicateField {
        /// Emitted field name.
        field: String,
    },
    /// A name cannot be emitted under the requested field identity.
    #[error("invalid field name {field:?}")]
    InvalidFieldName {
        /// Supplied or composed field name.
        field: String,
    },
    /// An application field shares the emitted name of a user declaration.
    #[error("application field {field:?} matches a user declaration")]
    ApplicationNameConflict {
        /// Composed name shared by both identities.
        field: String,
    },
    /// The indicated type is unavailable for this ADI field.
    #[error("type {data_type:?} cannot be emitted for {field:?}")]
    InvalidType {
        /// Field or declaration target name.
        field: String,
        /// Type that cannot be emitted.
        data_type: DataType,
    },
    /// A supplied application-field type conflicts with an earlier occurrence.
    #[error("application field {field:?} has type {established:?}, but {supplied:?} was supplied")]
    ApplicationTypeConflict {
        /// Composed application field name.
        field: String,
        /// Type established in a successfully written record.
        established: DataType,
        /// Type supplied in the rejected record.
        supplied: DataType,
    },
    /// A field's value contains a byte outside its character repertoire.
    #[error("invalid ADI character in {field:?} at byte {offset}")]
    InvalidCharacter {
        /// Field name.
        field: String,
        /// Byte offset within the supplied value.
        offset: usize,
    },
    /// A user declaration's enumeration member contains an invalid byte.
    #[error("invalid ADI character in {field:?}, member index {member_index}, at byte {offset}")]
    InvalidEnumerationCharacter {
        /// Declared field name.
        field: String,
        /// Zero-based index of the member within the enumeration.
        member_index: usize,
        /// Zero-based byte offset within the supplied member.
        offset: usize,
    },
    /// A user declaration repeats a target name.
    #[error("duplicate user declaration for {field:?}")]
    DuplicateDeclaration {
        /// Repeated target name.
        field: String,
    },
    /// A record refers to a missing user declaration.
    #[error("no user declaration for {field:?}")]
    MissingDeclaration {
        /// Referenced name.
        field: String,
    },
    /// A user constraint cannot be faithfully written in ADI.
    #[error("invalid user constraint for {field:?}: {reason}")]
    InvalidConstraint {
        /// Declared field name.
        field: String,
        /// Reason the declaration cannot be emitted.
        reason: &'static str,
    },
    /// A value fails a caller-declared constraint.
    #[error("{field:?} violates its declared {constraint}")]
    ConstraintViolation {
        /// Referenced field name.
        field: String,
        /// Kind of value constraint that was violated.
        constraint: &'static str,
    },
    /// The output sink failed.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// A previous write may have left partial output in the sink.
    #[error("the ADI writer is terminal after an output error")]
    Terminal,
}

/// A synchronous writer for one ADI document.
pub struct Writer<W> {
    output: W,
    declarations: Declarations,
    application_types: HashMap<String, DataType>,
    status: Status,
}

#[derive(PartialEq, Eq)]
enum Status {
    Empty,
    HasRecords,
    Terminal,
}

impl<W: Write> Writer<W> {
    /// Creates an ADI writer that writes to `output`.
    ///
    /// Pass a file, a buffer such as `Vec<u8>`, or a mutable reference to a
    /// value that implements [`Write`]. The writer stores the value you pass.
    /// For example, passing `&mut file` lets the writer use the file while
    /// you retain ownership of it.
    ///
    /// If you supply a header, this method validates it before writing it to
    /// `output`.
    ///
    /// # Errors
    ///
    /// Returns a validation error for an invalid header or an I/O error
    /// while writing the header. An I/O error may leave a partial header.
    /// Either error drops an owned output or releases a mutable borrow.
    pub fn new(mut output: W, header: Option<Header<'_>>) -> Result<Self, WriteError> {
        let mut declarations = Declarations::default();
        if let Some(header) = header {
            let bytes = prepare_header(header, &mut declarations)?;
            output.write_all(&bytes)?;
        }
        Ok(Self {
            output,
            declarations,
            application_types: HashMap::new(),
            status: Status::Empty,
        })
    }

    /// Writes a complete record.
    ///
    /// A validation error writes zero bytes and permits a retry. An I/O error
    /// makes the writer terminal and can leave a partial record in the sink.
    /// New application types become visible to later records only after the
    /// entire record has been written.
    ///
    /// # Errors
    ///
    /// Returns a validation, output, or terminal state error.
    pub fn write_record(&mut self, record: Record<'_>) -> Result<(), WriteError> {
        if self.status == Status::Terminal {
            return Err(WriteError::Terminal);
        }
        if record.fields.is_empty() {
            return Err(WriteError::EmptyRecord);
        }

        let mut bytes = Vec::new();
        let mut seen = HashSet::new();
        let mut new_types = Vec::new();

        for field in record.fields {
            match field.name {
                RecordFieldName::Standard(standard) => {
                    let name = standard.as_str();
                    check_standard(standard, FieldSection::Record, field.value)?;
                    insert_name(&mut seen, name)?;
                    append_field(&mut bytes, name, None, field.value);
                }
                RecordFieldName::UserDefined { name } => {
                    let key = name.to_ascii_uppercase();
                    self.declarations.check_value(&key, name, field.value)?;
                    insert_key(&mut seen, key, name)?;
                    append_field(&mut bytes, name, None, field.value);
                }
                RecordFieldName::ApplicationDefined {
                    program_id,
                    field_name,
                    data_type,
                } => {
                    let name = format!("APP_{program_id}_{field_name}");
                    if program_id.is_empty()
                        || program_id.contains('_')
                        || field_name.is_empty()
                        || name.ends_with(' ')
                        || invalid_custom_name(&name)
                    {
                        return Err(WriteError::InvalidFieldName { field: name });
                    }
                    let key = name.to_ascii_uppercase();
                    if self.declarations.contains(&key) {
                        return Err(WriteError::ApplicationNameConflict { field: name });
                    }

                    let established = self.application_types.get(&key).copied();
                    let effective =
                        resolve_application_type(established.map(|kind| (kind, ())), data_type)
                            .map_err(|conflict| WriteError::ApplicationTypeConflict {
                                field: name.clone(),
                                established: conflict.established,
                                supplied: conflict.supplied,
                            })?;
                    let indicator = if data_type.is_some() || effective != DataType::MultilineString
                    {
                        Some(check_indicator(&name, effective)?)
                    } else {
                        None
                    };
                    check_value(&name, effective == DataType::MultilineString, field.value)?;
                    insert_key(&mut seen, key.clone(), &name)?;
                    append_field(&mut bytes, &name, indicator, field.value);
                    if established.is_none() {
                        new_types.push((key, effective));
                    }
                }
            }
        }

        bytes.extend_from_slice(b"<EOR>\r\n");
        if let Err(error) = self.output.write_all(&bytes) {
            self.status = Status::Terminal;
            return Err(error.into());
        }
        self.application_types.extend(new_types);
        self.status = Status::HasRecords;
        Ok(())
    }

    /// Consumes the ADI writer and returns the output passed to [`Self::new`].
    ///
    /// This calls [`Write::flush`] on the output before returning it. If the
    /// output was a mutable reference, the result contains that reference.
    ///
    /// # Errors
    ///
    /// Returns an error before flushing if no record was written or a
    /// previous I/O failure made the writer terminal. The output may
    /// already contain a header or a partial record. A flush error is
    /// returned as an I/O error.
    ///
    /// On error, an owned output is dropped and a mutable borrow is released.
    /// An owned adapter such as [`BufWriter`](std::io::BufWriter) can attempt
    /// to write buffered data when dropped. Pass a mutable reference to the
    /// adapter if you need to control what happens to it after an error.
    pub fn finish(mut self) -> Result<W, WriteError> {
        match self.status {
            Status::Empty => Err(WriteError::NoRecords),
            Status::Terminal => Err(WriteError::Terminal),
            Status::HasRecords => {
                self.output.flush()?;
                Ok(self.output)
            }
        }
    }
}

fn prepare_header(
    header: Header<'_>,
    declarations: &mut Declarations,
) -> Result<Vec<u8>, WriteError> {
    if header.preamble.is_empty()
        || header.preamble.contains('<')
        || check_text(header.preamble.as_bytes(), ParseEncoding::Ascii, false).is_err()
    {
        return Err(WriteError::InvalidPreamble);
    }

    let mut bytes = header.preamble.as_bytes().to_vec();
    bytes.extend_from_slice(b"\r\n");
    let mut seen = HashSet::new();
    let mut number = 0;
    for field in header.fields {
        match *field {
            HeaderField::Standard { field, value } => {
                let name = field.as_str();
                check_standard(field, FieldSection::Header, value)?;
                insert_name(&mut seen, name)?;
                append_field(&mut bytes, name, None, value);
            }
            HeaderField::UserDefined(definition) => {
                number += 1;
                declarations.append(&mut bytes, number, definition)?;
            }
        }
    }
    bytes.extend_from_slice(b"<EOH>\r\n");
    Ok(bytes)
}

fn check_standard(
    field: StandardField,
    section: FieldSection,
    value: &str,
) -> Result<(), WriteError> {
    if field.section() != section {
        return Err(WriteError::WrongSection { field });
    }
    // The catalog lists the current export type first.
    let data_type = field.data_types().first().copied();
    if let Some(data_type) = data_type {
        check_adi_type(field.as_str(), data_type)?;
    }
    check_value(
        field.as_str(),
        data_type == Some(DataType::MultilineString),
        value,
    )
}

fn check_adi_type(field: &str, data_type: DataType) -> Result<(), WriteError> {
    if matches!(
        data_type,
        DataType::IntlCharacter | DataType::IntlString | DataType::IntlMultilineString
    ) {
        return Err(WriteError::InvalidType {
            field: field.into(),
            data_type,
        });
    }
    Ok(())
}

fn check_indicator(field: &str, data_type: DataType) -> Result<&'static str, WriteError> {
    check_adi_type(field, data_type)?;
    data_type
        .type_indicator()
        .ok_or_else(|| WriteError::InvalidType {
            field: field.into(),
            data_type,
        })
}

fn check_value(field: &str, multiline: bool, value: &str) -> Result<(), WriteError> {
    check_text(value.as_bytes(), ParseEncoding::Ascii, multiline).map_err(|range| {
        WriteError::InvalidCharacter {
            field: field.into(),
            offset: range.start,
        }
    })
}

fn invalid_custom_name(name: &str) -> bool {
    check_text(name.as_bytes(), ParseEncoding::Ascii, false).is_err()
        || name.contains([',', ':', '<', '>', '{', '}'])
}

fn insert_name(seen: &mut HashSet<String>, name: &str) -> Result<(), WriteError> {
    insert_key(seen, name.to_ascii_uppercase(), name)
}

fn insert_key(seen: &mut HashSet<String>, key: String, name: &str) -> Result<(), WriteError> {
    if !seen.insert(key) {
        return Err(WriteError::DuplicateField { field: name.into() });
    }
    Ok(())
}

fn append_field(output: &mut Vec<u8>, name: &str, indicator: Option<&str>, value: &str) {
    let len = value.len();
    let descriptor = match indicator {
        Some(indicator) => format!("<{name}:{len}:{indicator}>"),
        None => format!("<{name}:{len}>"),
    };
    output.extend_from_slice(descriptor.as_bytes());
    output.extend_from_slice(value.as_bytes());
    output.extend_from_slice(b"\r\n");
}
