//! Typed values and document-local rules over the shared ADX XML path.

use std::{
    borrow::Cow,
    collections::{HashMap, hash_map::Entry},
    iter::FusedIterator,
    ops::Range,
    sync::OnceLock,
};

use crate::adif::{DataType, FieldSection};

use super::{
    ParseFailure, ParseResult,
    error::ParseError,
    framing::Sections,
    permissive::{PermissiveSection, field::PermissiveField},
};

pub mod attribute;
pub mod declarations;
mod definitions;
pub mod field;
pub mod header;
mod model;
pub mod record;

use declarations::UserDefinition;
use header::{Header, HeaderEntry};
use record::Record;

/// A section boundary in an ADX document.
///
/// Start events carry the opening tag's byte range in the parser's input, with
/// an exclusive end. EndAdx and EndRecords carry the closing tag's range in the
/// same input, or a zero-width span at the end of a self-closing tag.
///
/// EndHeader borrows the complete header from the parser. EndRecord owns the
/// complete record, whose data may borrow the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Section<'a, 'header> {
    /// The opening ADX tag.
    StartAdx(Range<usize>),

    /// The opening HEADER tag.
    StartHeader(Range<usize>),

    /// The complete document header.
    EndHeader(&'header Header<'a>),

    /// The opening RECORDS tag.
    StartRecords(Range<usize>),

    /// The opening RECORD tag.
    StartRecord(Range<usize>),

    /// One complete QSO record.
    EndRecord(Record<'a>),

    /// The end of the RECORDS collection.
    EndRecords(Range<usize>),

    /// The end of the ADX root.
    EndAdx(Range<usize>),
}

/// Owns the document header and parsing state for typed ADX events.
///
/// Iterate over `&mut parser` to read section events.
pub struct StrictParser<'a> {
    inner: Sections<'a>,
    header: OnceLock<Header<'a>>,
    apps: HashMap<(String, String), (DataType, Range<usize>)>,
    done: bool,
}

impl<'a> StrictParser<'a> {
    /// Creates a lazy parser over the original UTF-8 bytes.
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            inner: Sections::new(input, true),
            header: OnceLock::new(),
            apps: HashMap::new(),
            done: false,
        }
    }
}

impl<'a, 'header> IntoIterator for &'header mut StrictParser<'a> {
    type Item = Result<ParseResult<'a, Section<'a, 'header>>, ParseFailure<'a>>;
    type IntoIter = Events<'a, 'header>;

    /// Creates an iterator over the remaining section events.
    ///
    /// Header events borrow this parser's header. Completed records move to the
    /// caller and borrow only the source. Earlier results remain usable after a
    /// later failure while their borrowed data remains alive. Dropping the
    /// iterator leaves the parser at its current position.
    fn into_iter(self) -> Self::IntoIter {
        let mut users = HashMap::new();
        if let Some(header) = self.header.get() {
            for entry in header.entries() {
                if let HeaderEntry::UserDefinition(definition) = entry {
                    users.insert(definition.field_name(), definition);
                }
            }
        }

        Events {
            inner: &mut self.inner,
            users,
            apps: &mut self.apps,
            header: &self.header,
            done: &mut self.done,
        }
    }
}

/// An iterator over typed ADX events, borrowing its parser's header and state.
pub struct Events<'a, 'header> {
    inner: &'header mut Sections<'a>,
    users: HashMap<&'header str, &'header UserDefinition<'a>>,
    apps: &'header mut HashMap<(String, String), (DataType, Range<usize>)>,
    header: &'header OnceLock<Header<'a>>,
    done: &'header mut bool,
}

impl<'a, 'header> Iterator for Events<'a, 'header> {
    type Item = Result<ParseResult<'a, Section<'a, 'header>>, ParseFailure<'a>>;

    /// Reads the next section event or returns a terminal failure.
    ///
    /// Each outcome includes warnings found since the previous outcome.
    /// Warnings outside sections accompany the next event or fatal error. A
    /// fatal error is returned once. Later calls return None. Successful
    /// completion returns None after the document and trailing input are
    /// checked. Stopping early leaves the remaining input unchecked.
    ///
    /// Header USERDEF declarations determine the types of record user fields. A
    /// supplied TYPE attribute on a record USERDEF does not change its declared
    /// type.
    fn next(&mut self) -> Option<Self::Item> {
        if *self.done {
            return None;
        }

        let r = self.inner.next_section()?;
        let r = match r {
            Ok(r) => r,
            Err(e) => {
                *self.done = true;
                return Some(Err(e));
            }
        };

        let data = match r.data {
            PermissiveSection::StartAdx(span) => Ok(Section::StartAdx(span)),
            PermissiveSection::StartHeader(span) => Ok(Section::StartHeader(span)),
            PermissiveSection::StartRecords(span) => Ok(Section::StartRecords(span)),
            PermissiveSection::StartRecord(span) => Ok(Section::StartRecord(span)),
            PermissiveSection::EndRecords(span) => Ok(Section::EndRecords(span)),
            PermissiveSection::EndAdx(span) => Ok(Section::EndAdx(span)),
            PermissiveSection::EndHeader(h) => prepare(h.fields)
                .and_then(|fields| {
                    fields
                        .into_iter()
                        .map(|(element, text)| {
                            if super::model::local(element.name) == "USERDEF" {
                                declarations::parse(element, text).map(HeaderEntry::UserDefinition)
                            } else {
                                field::convert(
                                    element,
                                    text,
                                    FieldSection::Header,
                                    &self.users,
                                    self.apps,
                                )
                                .map(HeaderEntry::Field)
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .and_then(|entries| {
                    let header = Header {
                        entries,
                        span: h.span,
                    };

                    definitions::check_header(&header)?;

                    let header = self.header.get_or_init(|| header);

                    for entry in header.entries() {
                        if let HeaderEntry::UserDefinition(definition) = entry {
                            self.users.insert(definition.field_name(), definition);
                        }
                    }

                    Ok(Section::EndHeader(header))
                }),
            PermissiveSection::EndRecord(r) => prepare(r.fields)
                .and_then(|fields| {
                    fields
                        .into_iter()
                        .map(|(element, text)| {
                            field::convert(
                                element,
                                text,
                                FieldSection::Record,
                                &self.users,
                                self.apps,
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .map(|fields| {
                    Section::EndRecord(Record {
                        fields,
                        span: r.span,
                    })
                }),
        };

        Some(match data {
            Ok(data) => Ok(ParseResult {
                data,
                warnings: r.warnings,
            }),
            Err(error) => {
                *self.done = true;
                Err(ParseFailure {
                    error,
                    warnings: r.warnings,
                })
            }
        })
    }
}

impl FusedIterator for Events<'_, '_> {}

fn prepare<'a>(
    fields: Vec<PermissiveField<'a>>,
) -> Result<Vec<(model::Element<'a>, Cow<'a, str>)>, ParseError<'a>> {
    let fields = fields
        .into_iter()
        .map(model::prepare)
        .collect::<Result<Vec<_>, _>>()?;
    let mut seen = HashMap::new();

    for (f, _) in &fields {
        let identity = f.identity().key();
        match seen.entry(identity) {
            Entry::Vacant(entry) => {
                entry.insert(f.span.clone());
            }
            Entry::Occupied(entry) => {
                let (identity, first) = entry.remove_entry();
                return Err(ParseError::Duplicate {
                    span: f.span.clone(),
                    first_span: first,
                    identity: identity.into_owned(),
                });
            }
        }
    }

    Ok(fields)
}
