//! Shared ADI descriptor, value, and section framing.

use std::{borrow::Cow, ops::Range};

use winnow::{
    LocatingSlice, Parser as _, Stateful,
    combinator::opt,
    error::{StrContext, StrContextValue},
    stream::Stream as _,
    token::take_till,
};

use super::{
    error::{Result, StructuralError},
    options::ParseEncoding,
};
use crate::adif::DecodeError;

pub mod field;
pub mod header;
pub mod record;

pub(super) type Input<'a, S> = Stateful<LocatingSlice<&'a [u8]>, S>;

pub(super) fn decode_text<'a>(
    bytes: &'a [u8],
    encoding: ParseEncoding,
) -> Result<Cow<'a, str>, DecodeError> {
    match encoding {
        ParseEncoding::Ascii => {
            if let Some(offset) = bytes.iter().position(|byte| !byte.is_ascii()) {
                return Err(DecodeError::NonAscii { offset });
            }
            str::from_utf8(bytes).map(Cow::Borrowed).map_err(Into::into)
        }
        ParseEncoding::Utf8 => str::from_utf8(bytes).map(Cow::Borrowed).map_err(Into::into),
        ParseEncoding::Latin1 => Ok(encoding_rs::mem::decode_latin1(bytes)),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Phase {
    Start,
    Records,
    Finished,
}

#[derive(Debug)]
pub(super) struct Sections<'a, S> {
    input: Input<'a, S>,
    phase: Phase,
}

impl<'a, S: std::fmt::Debug> Sections<'a, S> {
    pub(super) fn new(bytes: &'a [u8], state: S) -> Self {
        Self {
            input: Stateful {
                input: LocatingSlice::new(bytes),
                state,
            },
            phase: Phase::Start,
        }
    }

    pub(super) fn next<'header, O>(
        &mut self,
        mut header: impl FnMut(&mut Input<'a, S>) -> Result<O, StructuralError<'header>>,
        mut record: impl FnMut(&mut Input<'a, S>) -> Result<O, StructuralError<'header>>,
    ) -> Option<std::result::Result<O, StructuralError<'header>>> {
        if self.phase == Phase::Finished {
            return None;
        }
        let result = (|| {
            if self.phase == Phase::Start {
                self.phase = Phase::Records;
                if self.input.peek_token().is_some_and(|b| b != b'<') {
                    return header(&mut self.input).map(Some);
                }
            }
            take_till(0.., b'<').void().parse_next(&mut self.input)?;
            if self.input.peek_token().is_none() {
                return Ok(None);
            }
            record(&mut self.input).map(Some)
        })();
        match result {
            Ok(Some(section)) => Some(Ok(section)),
            Ok(None) => {
                self.phase = Phase::Finished;
                None
            }
            Err(error) => {
                self.phase = Phase::Finished;
                Some(Err(error))
            }
        }
    }
}

pub(super) fn fields<'a, 'header, S: std::fmt::Debug, O>(
    input: &mut Input<'a, S>,
    marker: &'static str,
    minimum: usize,
    mut entry: impl FnMut(&mut Input<'a, S>) -> Result<O, StructuralError<'header>>,
) -> Result<(Vec<O>, Range<usize>), StructuralError<'header>> {
    use winnow::{
        ascii::{Caseless, line_ending},
        combinator::{eof, not, preceded, repeat_till, terminated},
    };
    let label = if marker == "<EOH>" {
        "header"
    } else {
        "record"
    };
    let terminator = |input: &mut Input<'a, S>| {
        let span = Caseless(marker).span().parse_next(input)?;
        if marker == "<EOR>" {
            opt(line_ending).void().parse_next(input)?;
        }
        Ok(span)
    };
    repeat_till(
        minimum..,
        terminated(
            preceded(
                not(eof)
                    .context(StrContext::Label(label))
                    .context(StrContext::Expected(StrContextValue::StringLiteral(marker))),
                &mut entry,
            ),
            take_till(0.., b'<'),
        ),
        terminator,
    )
    .context(StrContext::Label(label))
    .parse_next(input)
}
