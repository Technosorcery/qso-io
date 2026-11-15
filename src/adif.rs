//! Amateur Data Interchange Format (ADIF) data types, references, and ADI support.
//!
//! [`DataType`] describes ADIF value categories. [`value`] provides stored ADIF
//! values, and [`StandardField`] describes standard field definitions.
//! Reference enums describe ADIF enumeration identities and their metadata.
//! See [`adi::parser`] for strict and permissive ADI parsing interfaces.

use std::str::Utf8Error;

use thiserror::Error;

pub use enumerations::primary as primary_subdivisions;
pub use enumerations::{
    AlaskaCounty, AntPath, ArrlSection, Award, AwardSponsor, Band, ContestId, Continent, Credit,
    DxccEntity, EqslAg, Mode, MorseKeyType, NzRegionDistrict, PropagationMode, QslMedium, QslRcvd,
    QslSent, QslVia, QsoComplete, QsoDownloadStatus, QsoUploadStatus, Region, SecondarySubdivision,
    SecondarySubdivisionAlt, Submode,
};
pub use standard_field::{FieldSection, StandardField};

/// Borrowed ADI sections, their parser, and structural diagnostics.
pub mod adi;
mod enumerations;
mod number;
mod standard_field;
mod text;
pub mod value;

/// A text-decoding failure in field data or a header preamble.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DecodeError {
    #[error("Non-ASCII byte at offset {offset}")]
    /// A byte cannot be represented by the selected ASCII encoding.
    NonAscii {
        /// Zero-based byte offset of the first non-ASCII byte within the value
        /// or preamble, not within the whole document.
        offset: usize,
    },
    #[error("UTF-8 decode: {0}")]
    /// Invalid or incomplete UTF-8 in field data or a header preamble.
    Utf8(
        /// Standard UTF-8 error; its offsets are relative to those raw bytes.
        #[from]
        Utf8Error,
    ),
}

/// ADIF value categories and their optional descriptor indicators.
///
/// See [ADIF 3.1.8 data types](https://www.adif.org.uk/318/ADIF_318.htm#Data_Types).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DataType {
    /// a comma-delimited list of members of the Award enumeration
    AwardList,
    /// a comma-delimited list where each list item is either:
    ///
    /// * A member of the Credit enumeration.
    /// * A member of the Credit enumeration followed by a colon and an
    ///   ampersand-delimited list of members of the QSL_Medium enumeration.
    ///
    /// For example IOTA,WAS:LOTW&CARD,DXCC:CARD
    CreditList,
    /// a comma-delimited list of members of the Sponsored_Award enumeration
    SponsoredAwardList,
    /// * if True, the single ASCII character Y or y
    /// * if False, the single ASCII character N or n
    Boolean,
    /// an ASCII character whose code lies in the range of 48 through 57,
    /// inclusive
    Digit,
    /// a sequence of one or more Digits representing a decimal integer,
    /// optionally preceded by a minus sign (ASCII code 45).  Leading zeroes are
    /// allowed.
    Integer,
    /// a sequence of one or more Digits representing a decimal number,
    /// optionally preceded by a minus sign (ASCII code 45) and optionally
    /// including a single decimal point  (ASCII  code 46)
    Number,
    /// an unsigned sequence of one or more Digits representing a decimal
    /// integer that has a value greater than 0.  Leading zeroes are allowed.
    PositiveInteger,
    /// an ASCII character whose code lies in the range of 32 through 126,
    /// inclusive
    Character,
    /// a Unicode character (encoded with UTF-8) excluding line break CR
    /// (code 13) and LF (code 10) characters
    IntlCharacter,
    /// 8 Digits representing a UTC date in YYYYMMDD format, where
    ///
    /// * YYYY is a 4-Digit year specifier, where 1930 <= YYYY
    /// * MM is a 2-Digit month specifier, where 1 <= MM <= 12 [use leading
    ///   zeroes]
    /// * DD is a 2-Digit day specifier, where 1 <= DD <= DaysInMonth(MM)  [use
    ///   leading zeroes]
    Date,
    /// 6 Digits representing a UTC time in HHMMSS format or 4 Digits
    /// representing a time in HHMM format, where
    ///
    /// * HH is a 2-Digit hour specifier, where 0 <= HH <= 23  [use leading
    ///   zeroes]
    /// * MM is a 2-Digit minute specifier, where 0 <= MM <= 59  [use leading
    ///   zeroes]
    /// * SS is a 2-Digit second specifier, where 0 <= SS <= 59  [use leading
    ///   zeroes]
    Time,
    /// IOTA designator, in format CC-XXX, where
    ///
    /// * CC is a member of the Continent enumeration
    /// * XXX is the island group designator, where 1 <= XXX <= 999  [use
    ///   leading zeroes]
    IOTARefNo,
    /// a sequence of Characters
    String,
    /// a sequence of International Characters.  Fields of type IntlString must
    /// only be used in ADX files
    IntlString,
    /// a sequence of Characters and line-breaks, where a line break is an ASCII
    /// CR (code 13) followed immediately by an ASCII LF (code 10)
    MultilineString,
    /// a sequence of International Characters and line breaks.  Fields of type
    /// IntlMultilineString must only be used in ADX files
    IntlMultilineString,
    /// an explicit list of legal case-insensitive values represented in ASCII
    /// set forth in set notation, e.g. {A, B, C, D}, or defined in a table,
    /// from which a single value may be selected.
    Enumeration,
    /// a case-insensitive 2-character, 4-character, 6-character, or 8-character
    /// Maidenhead locator.
    ///
    /// Specific fields impose additional restrictions on the number of
    /// characters; see the field descriptions for the allowed numbers of
    /// characters.
    GridSquare,
    /// For a 10-character Maidenhead locator, contains characters 9 and 10.
    /// For a 12-character Maidenhead locator, contains characters 9, 10, 11 and
    /// 12.
    ///
    /// Characters 9 and 10 are case-insensitive ASCII letters in the range A-X.
    /// Characters 11 and 12 are Digits in the range 0-9.
    GridSquareExt,
    /// a comma-delimited list of GridSquare items
    GridSquareList,
    /// a sequence of 11 characters representing a latitude or longitude in XDDD
    /// MM.MMM format, where
    ///
    /// * X is a directional Character from the set {E, W, N, S}
    /// * DDD is a 3-Digit degrees specifier, where 0 <= DDD <= 180 [use leading
    ///   zeroes]
    /// * There is a single space character in between DDD and MM.MMM
    /// * MM.MMM is an unsigned Number minutes specifier with its decimal point
    ///   in the third position, where 00.000 <= MM.MMM <= 59.999  [use leading
    ///   and trailing zeroes]
    Location,
    /// a sequence of case-insensitive Characters representing a Parks on the
    /// Air park reference in the form `xxxx-nnnnn[@yyyyyy]` comprising 6 to 17
    /// characters where:

    /// * xxxx is the POTA national program and is 1 to 4 characters in length,
    ///   typically the default callsign prefix of the national program (rather
    ///   than the DX entity)
    /// * nnnnn represents the unique number within the national program and is
    ///   either 4 or 5 characters in length (use the exact format listed on the
    ///   POTA website)
    /// * yyyyyy **Optional** is the 4 to 6 character ISO 3166-2 code to
    ///   differentiate which state/province/prefecture/primary administration
    ///   location the contact represents, in the case that the park reference
    ///   spans more than one location (such as a trail).
    POTARef,
    /// a comma-delimited list of one or more POTARef items.
    POTARefList,
    /// a colon-delimited list of two or more members of the
    /// `Secondary_Administrative_Subdivision` enumeration.  E.g.:
    ///
    /// `MA,Franklin:MA,Hampshire`
    SecondarySubdivisionList,
    /// An unordered, semicolon-delimited list of one or more alternate secondary
    /// administrative subdivisions, each written as `enumeration-name:enumeration-code`.
    ///
    /// A code covering multiple localities separates them with `/`. Each
    /// enumeration name may occur only once and must apply to the DXCC entity.
    /// For example: `NZ_Regions:Hawkes Bay/Wairoa`.
    ///
    /// Used by `CNTY_ALT` and `MY_CNTY_ALT`; this type has no descriptor indicator.
    /// See [ADIF 3.1.8 data types](https://www.adif.org.uk/318/ADIF_318.htm#Data_Types).
    SecondaryAdministrativeSubdivisionListAlt,
    /// An International SOTA reference formed from an ITU prefix, an optional
    /// SOTA subdivision, `/`, and a SOTA reference number, in that order.
    ///
    /// Examples: `W2/WE-003`, `G/LD-003`.
    SOTARef,
    /// A case-insensitive International WWFF reference using `xxFF-nnnn`.
    ///
    /// The national program identifier `xx` occupies 1–4 characters, followed
    /// by the literal `FF-` and a four-digit program-local number padded with
    /// leading zeros. The complete reference occupies 8–11 characters.
    ///
    /// Examples: `KFF-4655`, `3DAFF-0002`.
    WWFFRef,
}

impl DataType {
    /// Reports whether this data type is defined only for importing legacy data.
    ///
    /// Returns `true` for [`Self::AwardList`] and `false` for other types.
    pub fn import_only(self) -> bool {
        self == DataType::AwardList
    }

    /// Returns the uppercase ADIF indicator, or `None` for types without one.
    ///
    /// The international string indicators `I` and `G` designate ADX types.
    pub fn type_indicator(self) -> Option<&'static str> {
        match self {
            DataType::Boolean => Some("B"),
            DataType::Number => Some("N"),
            DataType::Date => Some("D"),
            DataType::Time => Some("T"),
            DataType::String => Some("S"),
            DataType::IntlString => Some("I"),
            DataType::MultilineString => Some("M"),
            DataType::IntlMultilineString => Some("G"),
            DataType::Enumeration => Some("E"),
            DataType::Location => Some("L"),
            DataType::AwardList
            | DataType::CreditList
            | DataType::SponsoredAwardList
            | DataType::Digit
            | DataType::Integer
            | DataType::PositiveInteger
            | DataType::Character
            | DataType::IntlCharacter
            | DataType::IOTARefNo
            | DataType::GridSquare
            | DataType::GridSquareExt
            | DataType::GridSquareList
            | DataType::POTARef
            | DataType::POTARefList
            | DataType::SecondarySubdivisionList
            | DataType::SecondaryAdministrativeSubdivisionListAlt
            | DataType::SOTARef
            | DataType::WWFFRef => None,
        }
    }
}

/// Failure to recognize a supplied ADIF type indicator.
#[derive(Debug, Error)]
pub enum DataTypeError {
    #[error("Unknown type specifier: {0}")]
    /// The supplied text is not a recognized single type indicator.
    UnknownTypeSpecifier(
        /// Original unrecognized indicator text, including its supplied casing.
        String,
    ),
}

impl TryFrom<&str> for DataType {
    type Error = DataTypeError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "B" | "b" => Ok(DataType::Boolean),
            "N" | "n" => Ok(DataType::Number),
            "D" | "d" => Ok(DataType::Date),
            "T" | "t" => Ok(DataType::Time),
            "S" | "s" => Ok(DataType::String),
            "I" | "i" => Ok(DataType::IntlString),
            "M" | "m" => Ok(DataType::MultilineString),
            "G" | "g" => Ok(DataType::IntlMultilineString),
            "E" | "e" => Ok(DataType::Enumeration),
            "L" | "l" => Ok(DataType::Location),
            u => Err(DataTypeError::UnknownTypeSpecifier(u.to_owned())),
        }
    }
}
