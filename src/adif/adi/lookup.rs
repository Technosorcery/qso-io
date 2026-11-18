//! Name lookup and ambiguity diagnostics for ADI fields and entries.

use std::ops::Range;

/// Failure to select a single field or header entry by name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("ambiguous lookup for {name:?}: matching descriptors at {descriptor_spans:?}")]
pub struct LookupError {
    /// The requested lookup spelling.
    pub name: String,
    /// Byte ranges of matching descriptors in the parser's input, in source
    /// order and with exclusive ends.
    pub descriptor_spans: Vec<Range<usize>>,
}

pub(super) fn single_match<'s, T: 's>(
    name: &str,
    mut matches: impl Iterator<Item = &'s T>,
    span: impl Fn(&T) -> Range<usize>,
) -> Result<Option<&'s T>, LookupError> {
    let Some(first) = matches.next() else {
        return Ok(None);
    };
    let Some(second) = matches.next() else {
        return Ok(Some(first));
    };
    let mut descriptor_spans = vec![span(first), span(second)];
    descriptor_spans.extend(matches.map(span));
    Err(LookupError {
        name: name.to_owned(),
        descriptor_spans,
    })
}
