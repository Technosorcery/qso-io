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
mod parse;
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
        /// Standard UTF-8 error with offsets relative to those raw bytes.
        #[from]
        Utf8Error,
    ),
}

/// ADIF value categories and their optional descriptor indicators.
///
/// See [ADIF 3.1.8 data types](https://www.adif.org.uk/318/ADIF_318.htm#Data_Types).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DataType {
    /// A comma-delimited list of members of the Award enumeration
    AwardList,
    /// A comma-delimited list where each list item is either:
    ///
    /// * A member of the Credit enumeration.
    /// * A member of the Credit enumeration followed by a colon and an
    ///   ampersand-delimited list of members of the QSL_Medium enumeration.
    ///
    /// For example IOTA,WAS:LOTW&CARD,DXCC:CARD
    CreditList,
    /// A comma-delimited list of members of the Sponsored_Award enumeration
    SponsoredAwardList,
    /// * If True, the single ASCII character Y or y
    /// * If False, the single ASCII character N or n
    Boolean,
    /// An ASCII character whose code lies in the range of 48 through 57,
    /// inclusive
    Digit,
    /// A sequence of one or more Digits that represents a decimal integer.
    ///
    /// An optional minus sign (ASCII code 45) precedes the Digits. Leading zeros
    /// are permitted.
    Integer,
    /// A sequence of one or more Digits that represents a decimal number.
    ///
    /// An optional minus sign (ASCII code 45) precedes the Digits. The sequence
    /// can include one decimal point (ASCII code 46).
    Number,
    /// An unsigned sequence of one or more Digits with an integer value greater
    /// than 0. Leading zeros are permitted.
    PositiveInteger,
    /// An ASCII character whose code lies in the range of 32 through 126,
    /// inclusive
    Character,
    /// A Unicode character in UTF-8, except CR (code 13) and LF (code 10).
    IntlCharacter,
    /// 8 Digits for a UTC date in YYYYMMDD format, where
    ///
    /// * YYYY is a 4-Digit year specifier, where 1930 <= YYYY
    /// * MM is a 2-Digit month specifier, where 1 <= MM <= 12 [use leading
    ///   zeroes]
    /// * DD is a 2-Digit day specifier, where 1 <= DD <= DaysInMonth(MM)  [use
    ///   leading zeroes]
    Date,
    /// 6 Digits for a UTC time in HHMMSS format or 4 Digits for HHMM, where
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
    /// A sequence of Characters
    String,
    /// A sequence of International Characters, for use only in ADX files.
    IntlString,
    /// A sequence of Characters and line-breaks, where a line break is an ASCII
    /// CR (code 13) followed immediately by an ASCII LF (code 10)
    MultilineString,
    /// A sequence of International Characters and line breaks, for use only in
    /// ADX files.
    IntlMultilineString,
    /// A set of permitted case-insensitive ASCII values.
    ///
    /// Set notation, for example {A, B, C, D}, or a table defines the set. A field
    /// contains one value from that set.
    Enumeration,
    /// A case-insensitive 2-character, 4-character, 6-character, or 8-character
    /// Maidenhead locator.
    ///
    /// Specific fields impose additional restrictions on the number of
    /// characters. See the field descriptions for the permitted numbers of
    /// characters.
    GridSquare,
    /// For a 10-character Maidenhead locator, contains characters 9 and 10.
    /// For a 12-character Maidenhead locator, contains characters 9, 10, 11 and
    /// 12.
    ///
    /// Characters 9 and 10 are case-insensitive ASCII letters in the range A-X.
    /// Characters 11 and 12 are Digits in the range 0-9.
    GridSquareExt,
    /// A comma-delimited list of GridSquare items
    GridSquareList,
    /// A sequence of 11 characters for a latitude or longitude in XDDD
    /// MM.MMM format, where
    ///
    /// * X is a directional Character from the set {E, W, N, S}
    /// * DDD is a 3-Digit degrees specifier, where 0 <= DDD <= 180 [use leading
    ///   zeroes]
    /// * There is a single space character in between DDD and MM.MMM
    /// * MM.MMM is an unsigned Number minutes specifier, where
    ///   00.000 <= MM.MMM <= 59.999. The decimal point is in the third position.
    ///   The format has leading and trailing zeros.
    Location,
    /// A case-insensitive Parks on the Air park reference of 6 to 17 Characters.
    ///
    /// The form is `xxxx-nnnnn[@yyyyyy]`, with these components:

    /// * xxxx identifies the POTA national program in 1 to 4 characters.
    ///   Usually, it is the national program's default callsign prefix, not the
    ///   DX entity's prefix.
    /// * nnnnn is the unique number within the national program, with 4 or 5
    ///   characters. Its exact format follows the POTA website.
    /// * yyyyyy is an optional ISO 3166-2 code of 4 to 6 characters. For a park
    ///   reference with multiple locations (such as a trail), it identifies the
    ///   contact's state, province, prefecture, or primary administration location.
    POTARef,
    /// A comma-delimited list of one or more POTARef items.
    POTARefList,
    /// A colon-delimited list of two or more members of the
    /// `Secondary_Administrative_Subdivision` enumeration. For example:
    ///
    /// `MA,Franklin:MA,Hampshire`
    SecondarySubdivisionList,
    /// An unordered, semicolon-delimited list of one or more alternate secondary
    /// administrative subdivisions, each written as `enumeration-name:enumeration-code`.
    ///
    /// A code covering multiple localities separates them with `/`. Each
    /// enumeration name can occur only once and must apply to the DXCC entity.
    /// For example: `NZ_Regions:Hawkes Bay/Wairoa`.
    ///
    /// `CNTY_ALT` and `MY_CNTY_ALT` use this type. It has no descriptor indicator.
    /// See [ADIF 3.1.8 data types](https://www.adif.org.uk/318/ADIF_318.htm#Data_Types).
    SecondaryAdministrativeSubdivisionListAlt,
    /// An International SOTA reference formed from an ITU prefix, an optional
    /// SOTA subdivision, `/`, and a SOTA reference number, in that order.
    ///
    /// Examples: `W2/WE-003`, `G/LD-003`.
    SOTARef,
    /// A case-insensitive International WWFF reference with the form `xxFF-nnnn`.
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
        /// Supplied unrecognized indicator text, with its supplied casing.
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
