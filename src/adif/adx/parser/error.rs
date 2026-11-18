//! ADX structure, decoding and value diagnostics.

use std::{borrow::Cow, fmt, ops::Range};
use thiserror::Error;

/// The reason complete decoded text is unavailable.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TextError<'a> {
    /// The contents include an unexpanded custom reference.
    #[error("unsupported custom reference: {0}")]
    CustomReference(&'a str),

    /// Field contents include nested elements.
    #[error("nested markup has no scalar text")]
    NestedMarkup,
}

/// A terminal parsing failure with byte ranges in the parser's input and
/// context.
#[derive(Debug)]
pub enum ParseError<'a> {
    /// Malformed supported XML syntax.
    Xml {
        /// Byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// The XML requirement that failed.
        message: Cow<'static, str>,
        /// Underlying XML reader failure, when available.
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// XML contents do not form the required ADX envelope.
    Envelope {
        /// Byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// The required section placement or boundary.
        message: &'static str,
    },

    /// A construct prohibited by ADX in strict mode.
    Prohibited {
        /// Byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// The prohibited structure or decoding construct.
        construct: WarningKind<'a>,
    },

    /// A logical identity occurred more than once in one section.
    Duplicate {
        /// Repeated field's byte range in the parser's input, with an exclusive
        /// end.
        span: Range<usize>,
        /// First field's byte range in the parser's input, with an exclusive
        /// end.
        first_span: Range<usize>,
        /// The repeated identity, with ADIF comparison casing normalized.
        identity: super::identity::FieldIdentity<'static>,
    },

    /// A required ADX attribute was absent or empty.
    MissingAttribute {
        /// Byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// The required attribute's name.
        attribute: &'static str,
    },

    /// A TYPE attribute is not allowed here or has an unsupported value.
    InvalidType {
        /// Byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// Supplied decoded TYPE text.
        supplied: String,
        /// The restriction on the TYPE attribute.
        message: &'static str,
    },

    /// An ADX user declaration failed its metadata or constraint rules.
    Declaration {
        /// Byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// The declaration rule and its associated context.
        reason: DeclarationError,
    },

    /// A user reference has no matching header declaration.
    UndefinedUser {
        /// Byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// The referenced target, with comparison casing normalized.
        name: String,
    },

    /// Supplied type disagrees with the established document type.
    TypeConflict {
        /// Attempted type's byte range in the parser's input, with an exclusive
        /// end.
        span: Range<usize>,
        /// First field's byte range in the parser's input, with an exclusive
        /// end.
        first_span: Range<usize>,
        /// The logical identity involved.
        identity: super::identity::FieldIdentity<'static>,
        /// The previously established type.
        expected: crate::adif::DataType,
        /// The attempted new type.
        supplied: crate::adif::DataType,
    },

    /// A decoded value violates a declared enumeration or exact numeric range.
    ConstraintViolation {
        /// Byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// Declaration's byte range in the parser's input, with an exclusive
        /// end.
        declaration_span: Range<usize>,
        /// Constraint's byte range in the parser's input, with an exclusive
        /// end.
        constraint_span: Range<usize>,
        /// Underlying numeric conversion failure, when available.
        source: Option<bigdecimal::ParseBigDecimalError>,
    },

    /// Invalid decoded contents for a resolved ADIF type.
    Value {
        /// Byte range in the parser's input, with an exclusive end.
        span: Range<usize>,
        /// Supplied field name.
        field_name: String,
        /// The failed semantic rule.
        cause: crate::adif::value::ValueError,
        /// Maintained enumeration, where applicable.
        enumeration: Option<&'static str>,
        /// Underlying numeric conversion failure, when available.
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Every supported standard type alternative failed.
    NoMatchingType {
        /// First candidate's byte range in the parser's input, with an
        /// exclusive end.
        span: Range<usize>,
        /// Supplied field name.
        field_name: String,
        /// Ordered candidate failures.
        failures: Vec<TypeAlternativeFailure<'a>>,
    },
}

impl ParseError<'_> {
    /// Returns the failure's byte range in the parser's input, with an
    /// exclusive end.
    pub fn span(&self) -> Range<usize> {
        match self {
            Self::Xml { span, .. }
            | Self::Envelope { span, .. }
            | Self::Prohibited { span, .. }
            | Self::Duplicate { span, .. }
            | Self::MissingAttribute { span, .. }
            | Self::InvalidType { span, .. }
            | Self::Declaration { span, .. }
            | Self::UndefinedUser { span, .. }
            | Self::TypeConflict { span, .. }
            | Self::ConstraintViolation { span, .. }
            | Self::Value { span, .. }
            | Self::NoMatchingType { span, .. } => span.clone(),
        }
    }

    /// Borrows related definition or first-occurrence byte ranges in the
    /// parser's input, with exclusive ends.
    ///
    /// Constraint failures yield the declaration before the constraint
    /// location.
    pub fn related_spans(&self) -> impl Iterator<Item = &Range<usize>> {
        let spans = match self {
            Self::Duplicate { first_span, .. }
            | Self::TypeConflict { first_span, .. }
            | Self::Declaration {
                reason: DeclarationError::Duplicate { first_span },
                ..
            } => [Some(first_span), None],
            Self::ConstraintViolation {
                declaration_span,
                constraint_span,
                ..
            } => [Some(declaration_span), Some(constraint_span)],
            _ => [None, None],
        };

        spans.into_iter().flatten()
    }
}

impl fmt::Display for ParseError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Xml {
                span,
                message,
                source: Some(source),
            } => return write!(f, "{message}: {source} at {span:?}"),
            Self::Xml { message, .. } => message.as_ref(),
            Self::Envelope { message, .. } | Self::InvalidType { message, .. } => message,
            Self::Prohibited { span, construct } => {
                return write!(f, "ADX prohibits {construct:?} at {span:?}");
            }
            Self::Duplicate { .. } => "duplicate ADX field identity",
            Self::MissingAttribute { .. } => "required ADX attribute",
            Self::Declaration { .. } => "invalid ADX user declaration",
            Self::UndefinedUser { .. } => "undeclared user field",
            Self::TypeConflict { .. } => "application type changed",
            Self::ConstraintViolation { .. } => "value violates user declaration",
            Self::Value { .. } => "invalid ADIF value",
            Self::NoMatchingType { .. } => "no supported ADIF type matched",
        };

        write!(f, "{message} at {:?}", self.span())
    }
}

impl std::error::Error for ParseError<'_> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Xml { source, .. } | Self::Value { source, .. } => source
                .as_deref()
                .map(|source| source as &(dyn std::error::Error + 'static)),
            Self::ConstraintViolation { source, .. }
            | Self::Declaration {
                reason: DeclarationError::Range { source },
                ..
            } => source
                .as_ref()
                .map(|source| source as &dyn std::error::Error),
            _ => None,
        }
    }
}

/// An XML or ADX structure or decoding issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningKind<'a> {
    /// Element or attribute names do not use ADX uppercase spelling.
    NameCasing,

    /// CDATA appeared in field contents.
    Cdata,

    /// A document type declaration appeared.
    Doctype,

    /// A literal entity definition appeared.
    EntityDefinition(&'a str),

    /// A supplied custom reference was not expanded.
    CustomReference(&'a str),

    /// A field contains nested markup.
    NestedMarkup,

    /// Required identity metadata was absent.
    MissingAttribute(&'static str),
}

/// An issue and its byte range in the parser's input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseWarning<'a> {
    /// The reported structure or decoding issue.
    pub kind: WarningKind<'a>,

    /// Byte range in the parser's input, with an exclusive end.
    pub span: Range<usize>,
}

/// One failed candidate in a standard field's ordered type alternatives.
#[derive(Debug)]
pub struct TypeAlternativeFailure<'a> {
    /// The attempted ADIF type.
    pub data_type: crate::adif::DataType,

    /// Its located conversion failure and underlying cause.
    pub error: ParseError<'a>,
}

/// The part of an ADX USERDEF declaration that failed.
#[derive(Debug, Clone, PartialEq)]
pub enum DeclarationError {
    /// FIELDID is not a positive unsigned integer.
    Identifier,

    /// The target name violates the ADIF name rules or ADX uppercase spelling.
    Target,

    /// A target conflicts with a standard ADIF name.
    StandardTarget,

    /// Both ENUM and RANGE were supplied.
    ConflictingConstraints,

    /// ENUM is not a braced nonempty list of String members.
    Enumeration,

    /// RANGE does not contain two exact Number bounds.
    Range {
        /// Underlying decimal conversion failure, when available.
        source: Option<bigdecimal::ParseBigDecimalError>,
    },

    /// The upper range bound is not larger than the lower bound.
    RangeOrder,

    /// A declaration identifier or target is repeated.
    Duplicate {
        /// First identifier or target's byte range in the parser's input, with
        /// an exclusive end.
        first_span: Range<usize>,
    },
}
