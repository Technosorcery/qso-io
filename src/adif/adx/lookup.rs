//! Identity-specific lookup and ambiguity reporting.
use std::ops::Range;
use thiserror::Error;

/// Several supplied fields matched one requested identity.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("ambiguous field identity at {spans:?}")]
pub struct LookupError {
    /// Byte ranges of matching elements in the parser's input, in source order
    /// and with exclusive ends.
    pub spans: Vec<Range<usize>>,
}

pub(crate) fn single<'s, T: 's>(
    mut items: impl Iterator<Item = &'s T>,
    span: impl Fn(&T) -> Range<usize>,
) -> Result<Option<&'s T>, LookupError> {
    let Some(first) = items.next() else {
        return Ok(None);
    };
    let Some(second) = items.next() else {
        return Ok(Some(first));
    };
    let mut spans = vec![span(first), span(second)];
    spans.extend(items.map(span));
    Err(LookupError { spans })
}
