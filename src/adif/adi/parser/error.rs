//! Located ADI syntax, decoding and value-conversion diagnostics.

use std::{error::Error as StdError, fmt, num::ParseIntError, ops::Range};

use thiserror::Error;
use winnow::error::{AddContext, FromExternalError, ParserError, StrContext, StrContextValue};
use winnow::stream::{Location, Stream};

use super::framing::Input;
use super::options::LengthUnit;
use super::strict::declarations::UserDefinitionError;
use crate::adif::value::ValueError;
use crate::adif::{DataType, DataTypeError, DecodeError};

/// A failed ADI syntax requirement and its underlying cause where applicable.
#[derive(Debug, Error)]
pub enum SyntaxErrorKind {
    /// A field descriptor is incomplete or does not match ADI syntax.
    #[error("malformed data descriptor")]
    MalformedDescriptor,
    /// An application-defined record field has an empty program ID.
    #[error("empty program ID in application-defined field name")]
    EmptyApplicationProgramId,
    /// The required unsigned decimal length is missing or malformed.
    #[error("invalid field length")]
    InvalidLength,
    /// The supplied length exceeds the supported integer range.
    #[error("field length exceeds the supported integer range")]
    LengthOverflow {
        /// The integer conversion failure.
        #[source]
        source: ParseIntError,
    },
    /// An inline indicator is not recognized.
    #[error("unknown data type indicator")]
    UnknownTypeIndicator {
        /// The supplied indicator and its conversion failure.
        #[source]
        source: DataTypeError,
    },
    /// Input ended before the header terminator.
    #[error("expected <EOH> before EOF")]
    MissingEoh,
    /// Input ended before the record terminator.
    #[error("expected <EOR> before EOF")]
    MissingEor,
    /// A field name is not valid UTF-8.
    #[error("invalid UTF-8 in field name")]
    InvalidNameEncoding {
        /// UTF-8 validation failure with byte offsets within the field name.
        #[source]
        source: std::str::Utf8Error,
    },
    /// An inline indicator is not valid UTF-8.
    #[error("invalid UTF-8 in type indicator")]
    InvalidIndicatorEncoding {
        /// UTF-8 validation failure with byte offsets within the type
        /// indicator.
        #[source]
        source: std::str::Utf8Error,
    },
}

/// An ADI parsing failure with byte ranges in the parser's input and context.
///
/// Out-of-range bounds borrow the parser-owned header. Other diagnostic data is
/// owned or static. Underlying conversion and decoding errors are available
/// through [`StdError::source`]; constraint reasons are stored in their
/// variant.
#[derive(Debug)]
pub enum StructuralError<'header> {
    /// Two USERDEF declarations name the same record field under ASCII
    /// case-insensitive comparison.
    DuplicateUserDefinition {
        /// Supplied repeated target name.
        field_name: String,
        /// First target name's byte range in the parser's input, with an
        /// exclusive end.
        first_span: Range<usize>,
        /// Second target name's byte range in the parser's input and primary
        /// failure location, with an exclusive end.
        second_span: Range<usize>,
    },
    /// A nonempty value violates a document USERDEF constraint.
    UserDefinitionConstraintViolation {
        /// Primary byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// Supplied record-field name.
        field_name: String,
        /// Declaration descriptor's byte range in the parser's input, with an
        /// exclusive end.
        declaration_span: Range<usize>,
        /// Constraint's byte range in the parser's input, including braces and
        /// with an exclusive end.
        constraint_span: Range<usize>,
        /// Constraint-specific reason with exact bounds where applicable.
        cause: crate::adif::adi::parser::strict::declarations::UserDefinitionValueError<'header>,
    },
    /// An inline indicator is outside a standard field's permitted types.
    StandardTypeConflict {
        /// Supplied descriptor's byte range in the parser's input, with an
        /// exclusive end.
        span: Range<usize>,
        /// Supplied field name.
        field_name: String,
        /// Catalog types in preference order.
        expected: &'static [DataType],
        /// Supplied inline type.
        supplied: DataType,
    },
    /// An inline indicator conflicts with a USERDEF declaration.
    UserTypeConflict {
        /// Supplied descriptor's byte range in the parser's input, with an
        /// exclusive end.
        span: Range<usize>,
        /// Supplied target name.
        field_name: String,
        /// The declared type.
        expected: DataType,
        /// Supplied inline type.
        supplied: DataType,
        /// Declaration's byte range in the parser's input, with an exclusive
        /// end.
        declaration_span: Range<usize>,
    },
    /// An inline indicator conflicts with an established application-field
    /// type.
    ApplicationTypeConflict {
        /// Supplied descriptor's byte range in the parser's input, with an
        /// exclusive end.
        span: Range<usize>,
        /// Supplied application-field name.
        field_name: String,
        /// The established type.
        expected: DataType,
        /// Supplied inline type.
        supplied: DataType,
        /// First descriptor's byte range in the parser's input, with an
        /// exclusive end.
        first_span: Range<usize>,
    },
    /// No permitted standard type accepts the supplied value.
    NoMatchingType {
        /// Primary byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// Supplied field name.
        field_name: String,
        /// Failures in preference order. The first supplies the primary span.
        failures: Vec<TypeAlternativeFailure>,
    },
    /// Malformed syntax or a required section terminator missing at EOF.
    Syntax {
        /// Primary byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// The specific syntactic requirement that was not satisfied.
        reason: SyntaxErrorKind,
    },
    /// Input ended before a field supplied its declared number of units.
    TruncatedValue {
        /// Primary byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// Field name as supplied in the affected descriptor.
        field_name: String,
        /// Length declared by the descriptor, measured in `unit`.
        declared: usize,
        /// Complete units present before EOF, measured in `unit`.
        available: usize,
        /// Selected length interpretation. Both counts use this unit.
        unit: LengthUnit,
    },
    /// A header or record has duplicate field names under ASCII
    /// case-insensitive comparison.
    DuplicateField {
        /// Field name as supplied in the affected descriptor.
        field_name: String,
        /// First descriptor's byte range in the parser's input, with an
        /// exclusive end.
        first_span: Range<usize>,
        /// Repeated descriptor's byte range in the parser's input and primary
        /// failure location, with an exclusive end.
        second_span: Range<usize>,
    },
    /// Nonempty contents fail their resolved ADIF type grammar or membership.
    InvalidValue {
        /// Underlying numeric conversion failure, when available.
        source: Option<Box<dyn StdError + Send + Sync>>,
        /// Primary byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// Supplied field name.
        field_name: String,
        /// Declared type whose grammar failed.
        cause: ValueError,
        /// Expected standard enumeration when membership failed.
        enumeration: Option<&'static str>,
    },
    /// An invalid USERDEF declaration.
    InvalidUserDefinition {
        /// Primary byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// Supplied descriptor name.
        field_name: String,
        /// Declaration-specific reason.
        cause: UserDefinitionError,
    },
    /// An ADIF data type is unavailable in ADI.
    TypeUnavailableInAdi {
        /// Primary byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// Recognized ADIF data type.
        data_type: DataType,
    },
    /// Text decoding failed in a field or header preamble.
    Decoding {
        /// Primary byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// Decoding cause, with byte offsets within the field's raw value or
        /// header preamble rather than the complete parser input.
        cause: DecodeError,
    },
}

/// A failed value conversion for one permitted standard field type.
#[derive(Debug, Clone)]
pub struct TypeAlternativeFailure {
    pub(super) data_type: DataType,
    pub(super) span: Range<usize>,
    pub(super) cause: ValueError,
    pub(super) enumeration: Option<&'static str>,
}

impl TypeAlternativeFailure {
    /// Returns the attempted ADIF type.
    pub fn data_type(&self) -> DataType {
        self.data_type
    }

    /// Returns the failure's byte range in the parser's input, with an
    /// exclusive end.
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }

    /// Borrows the type-specific failure reason.
    pub fn cause(&self) -> &ValueError {
        &self.cause
    }

    /// Returns the expected standard enumeration, when membership failed.
    ///
    /// Callers can keep the static label independently of this failure.
    pub fn enumeration(&self) -> Option<&'static str> {
        self.enumeration
    }
}

pub(super) type Result<T, E = StructuralError<'static>> = std::result::Result<T, E>;

impl StructuralError<'_> {
    /// Returns the primary byte range in the parser's input, with an exclusive
    /// end.
    pub fn span(&self) -> Range<usize> {
        match self {
            Self::DuplicateField { second_span, .. }
            | Self::DuplicateUserDefinition { second_span, .. } => second_span.clone(),
            Self::UserDefinitionConstraintViolation { span, .. }
            | Self::StandardTypeConflict { span, .. }
            | Self::UserTypeConflict { span, .. }
            | Self::ApplicationTypeConflict { span, .. }
            | Self::NoMatchingType { span, .. }
            | Self::Syntax { span, .. }
            | Self::TruncatedValue { span, .. }
            | Self::InvalidValue { span, .. }
            | Self::InvalidUserDefinition { span, .. }
            | Self::TypeUnavailableInAdi { span, .. }
            | Self::Decoding { span, .. } => span.clone(),
        }
    }
}

impl StdError for StructuralError<'_> {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        use super::strict::declarations::UserDefinitionValueError;

        match self {
            Self::Syntax { reason, .. } => reason.source(),
            Self::Decoding { cause, .. } => Some(cause),
            Self::InvalidValue {
                source: Some(source),
                ..
            } => Some(source.as_ref()),
            Self::InvalidUserDefinition {
                cause:
                    UserDefinitionError::InvalidRangeBound {
                        source: Some(source),
                    },
                ..
            }
            | Self::UserDefinitionConstraintViolation {
                cause:
                    UserDefinitionValueError::InvalidRangeValue {
                        source: Some(source),
                    },
                ..
            } => Some(source),
            Self::InvalidValue { cause, .. } => Some(cause),
            Self::InvalidUserDefinition { cause, .. } => Some(cause),
            Self::UserDefinitionConstraintViolation { .. } => None,
            _ => None,
        }
    }
}

impl<'a, 'header, S: fmt::Debug> ParserError<Input<'a, S>> for StructuralError<'header> {
    type Inner = Self;

    fn from_input(input: &Input<'a, S>) -> Self {
        let start = input.current_token_start();
        Self::Syntax {
            span: start..start + usize::from(input.eof_offset() != 0),
            reason: SyntaxErrorKind::MalformedDescriptor,
        }
    }

    fn into_inner(self) -> Result<Self, Self> {
        Ok(self)
    }
}

impl<'a, 'header, S: fmt::Debug> AddContext<Input<'a, S>, StrContext> for StructuralError<'header> {
    fn add_context(
        mut self,
        _input: &Input<'a, S>,
        _start: &<Input<'a, S> as Stream>::Checkpoint,
        context: StrContext,
    ) -> Self {
        if let Self::Syntax {
            reason: SyntaxErrorKind::MalformedDescriptor,
            span,
        } = self
        {
            let reason = match context {
                StrContext::Expected(StrContextValue::StringLiteral("<EOH>")) => {
                    SyntaxErrorKind::MissingEoh
                }
                StrContext::Expected(StrContextValue::StringLiteral("<EOR>")) => {
                    SyntaxErrorKind::MissingEor
                }
                _ => SyntaxErrorKind::MalformedDescriptor,
            };
            self = Self::Syntax { reason, span };
        }
        self
    }
}

impl<'a, 'header, S: fmt::Debug> FromExternalError<Input<'a, S>, DataTypeError>
    for StructuralError<'header>
{
    fn from_external_error(input: &Input<'a, S>, error: DataTypeError) -> Self {
        let indicator = &error.supplied;
        let start = input.current_token_start();
        let width = indicator.len().max(1).min(input.eof_offset());
        Self::Syntax {
            span: start..start + width,
            reason: SyntaxErrorKind::UnknownTypeIndicator { source: error },
        }
    }
}

impl fmt::Display for StructuralError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let span = self.span();
        write!(f, "ADI error at bytes {}..{}: ", span.start, span.end)?;
        match self {
            StructuralError::DuplicateUserDefinition {
                field_name,
                first_span,
                ..
            } => write!(
                f,
                "duplicate USERDEF target {field_name}; first declared at bytes {}..{}",
                first_span.start, first_span.end
            ),
            StructuralError::UserDefinitionConstraintViolation {
                field_name,
                cause,
                constraint_span,
                ..
            } => write!(
                f,
                "USERDEF constraint failed for {field_name}: {cause}; constraint at bytes {}..{}",
                constraint_span.start, constraint_span.end
            ),
            StructuralError::StandardTypeConflict {
                field_name,
                expected,
                supplied,
                ..
            } => write!(
                f,
                "type conflict for {field_name}: expected {expected:?}, supplied {supplied:?}"
            ),
            StructuralError::UserTypeConflict {
                field_name,
                expected,
                supplied,
                ..
            }
            | StructuralError::ApplicationTypeConflict {
                field_name,
                expected,
                supplied,
                ..
            } => write!(
                f,
                "type conflict for {field_name}: expected {expected:?}, supplied {supplied:?}"
            ),
            StructuralError::NoMatchingType {
                field_name,
                failures,
                ..
            } => {
                write!(f, "no permitted type accepts {field_name}")?;
                for failure in failures {
                    write!(
                        f,
                        "; {:?}: {} at bytes {}..{}",
                        failure.data_type, failure.cause, failure.span.start, failure.span.end
                    )?;
                }
                Ok(())
            }
            StructuralError::Syntax { reason, .. } => write!(f, "{reason}"),
            StructuralError::TruncatedValue {
                field_name,
                declared,
                available,
                unit,
                ..
            } => write!(
                f,
                "truncated value for {field_name}: declared {declared}, available {available} ({unit:?})"
            ),
            StructuralError::DuplicateField {
                field_name,
                first_span,
                ..
            } => write!(
                f,
                "duplicate field {field_name}; first descriptor at bytes {}..{}",
                first_span.start, first_span.end
            ),
            StructuralError::InvalidValue {
                field_name, cause, ..
            } => {
                write!(f, "invalid value for {field_name}: {cause}")
            }
            StructuralError::InvalidUserDefinition {
                field_name, cause, ..
            } => {
                write!(f, "invalid declaration {field_name}: {cause}")
            }
            StructuralError::TypeUnavailableInAdi { data_type, .. } => {
                write!(f, "type {data_type:?} is unavailable in ADI")
            }
            StructuralError::Decoding { cause, .. } => {
                write!(f, "text decoding failed: {cause}")
            }
        }
    }
}
