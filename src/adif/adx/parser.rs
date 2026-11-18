//! ADX section events with located warnings and terminal failures.

mod decoding;
mod framing;
mod model;

pub mod error;
pub mod identity;
pub mod permissive;

use error::{ParseError, ParseWarning};

/// Parsed data and its associated warnings.
#[derive(Debug, Clone)]
pub struct ParseResult<'a, T> {
    /// The parsed data.
    pub data: T,

    /// The associated structure and decoding warnings.
    pub warnings: Vec<ParseWarning<'a>>,
}

/// A terminal failure and warnings encountered before it.
#[derive(Debug)]
pub struct ParseFailure<'a> {
    /// The fatal condition.
    pub error: ParseError<'a>,

    /// Structure and decoding warnings in this failure outcome.
    pub warnings: Vec<ParseWarning<'a>>,
}
