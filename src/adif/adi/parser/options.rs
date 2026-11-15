//! Encoding and declared-length options for ADI parsing.

/// Explicit text encoding and declared-length interpretation.
///
/// Defaults to ASCII and byte lengths.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct ParseOpts {
    /// Encoding for field values and header preamble text.
    pub encoding: ParseEncoding,
    /// Unit used to interpret each descriptor's declared value length.
    pub field_length: LengthUnit,
}

/// Character encoding of ADI field values and header preamble text.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum ParseEncoding {
    /// Seven-bit ASCII, with byte values 0x00–0x7F.
    #[default]
    Ascii,
    /// ISO-8859-1, with bytes corresponding directly to U+0000–U+00FF,
    /// including C1 controls.
    ///
    /// This is ISO-8859-1/Latin-1 mapping, not Windows-1252.
    Latin1,
    /// UTF-8 encoding of Unicode scalar values.
    Utf8,
}

/// Unit of the value length declared in an ADI field descriptor.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum LengthUnit {
    /// Original input bytes.
    #[default]
    Bytes,
    /// Unicode scalar values, rather than grapheme clusters or UTF-16 code units.
    ///
    /// A scalar occupies one byte in ASCII or Latin-1 and one to four bytes
    /// in UTF-8.
    UnicodeScalars,
}
