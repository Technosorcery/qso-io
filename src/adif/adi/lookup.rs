//! Name lookup and ambiguity diagnostics for ADI fields and entries.

use std::ops::Range;

/// Failure to select a single field or header entry by name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LookupError {
    /// Multiple descriptors match the requested name under ASCII case-insensitive comparison.
    #[error("ambiguous lookup for {name:?}: matching descriptors at {descriptor_spans:?}")]
    Ambiguous {
        /// The requested lookup spelling.
        name: String,
        /// Every matching descriptor's exclusive-end byte range in source order.
        descriptor_spans: Vec<Range<usize>>,
    },
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
    Err(LookupError::Ambiguous {
        name: name.to_owned(),
        descriptor_spans,
    })
}
