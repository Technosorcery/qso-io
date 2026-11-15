use qso_io::adif::adi::parser::error::StructuralError;
use qso_io::adif::adi::parser::options::{LengthUnit, ParseEncoding, ParseOpts};
use qso_io::adif::adi::parser::strict::declarations::UserDefinition;
use qso_io::adif::adi::parser::strict::field::Field;
use qso_io::adif::adi::parser::strict::header::{Header, HeaderEntry};
use qso_io::adif::adi::parser::strict::record::Record;
use qso_io::adif::adi::parser::strict::{Section, StrictParser};

#[derive(Debug)]
pub(super) struct ParsedSections<'a> {
    pub header: Option<Header<'a>>,
    pub records: Vec<Record<'a>>,
}

#[track_caller]
pub(super) fn next_section<'a>(parser: &mut StrictParser<'a>) -> Section<'a> {
    match parser.next() {
        Some(Ok(s)) => s,
        other => panic!("expected complete section, got {other:?}; parser: {parser:?}"),
    }
}
#[track_caller]
pub(super) fn next_error<P, S>(parser: &mut P) -> StructuralError
where
    P: Iterator<Item = Result<S, StructuralError>> + std::fmt::Debug,
    S: std::fmt::Debug,
{
    match parser.next() {
        Some(Err(e)) => e,
        other => panic!("expected terminal error, got {other:?}; parser: {parser:?}"),
    }
}
#[track_caller]
pub(super) fn assert_exhausted<P: Iterator + std::fmt::Debug>(parser: &mut P) {
    for _ in 0..2 {
        assert!(
            parser.next().is_none(),
            "expected permanent exhaustion; parser: {parser:?}"
        );
    }
}
#[track_caller]
pub(super) fn record(section: Section<'_>) -> Record<'_> {
    match section {
        Section::Record(r) => r,
        other => panic!("expected record, got {other:?}"),
    }
}
#[track_caller]
pub(super) fn named<'r, 'a>(record: &'r Record<'a>, name: &str) -> &'r Field<'a> {
    match record.field(name) {
        Some(f) => f,
        other => panic!("expected unique field {name:?}, got {other:?}; record: {record:?}"),
    }
}
#[track_caller]
pub(super) fn definition<'r, 'a>(entry: &'r HeaderEntry<'a>) -> &'r UserDefinition<'a> {
    match entry {
        HeaderEntry::UserDefinition(d) => d,
        other => panic!("expected USERDEF, got {other:?}"),
    }
}
#[track_caller]
pub(super) fn parse_file(
    bytes: &[u8],
    encoding: ParseEncoding,
    field_length: LengthUnit,
) -> ParsedSections<'_> {
    let mut parsed = ParsedSections {
        header: None,
        records: Vec::new(),
    };
    for section in StrictParser::new(
        bytes,
        ParseOpts {
            encoding,
            field_length,
        },
    ) {
        match section {
            Ok(Section::Header(h)) => {
                assert!(parsed.header.is_none(), "unexpected second header");
                parsed.header = Some(h);
            }
            Ok(Section::Record(r)) => parsed.records.push(r),
            Err(e) => panic!(
                "expected successful document ({encoding:?}/{field_length:?}), input {bytes:?}: {e:?}"
            ),
        }
    }
    parsed
}
#[track_caller]
pub(super) fn parsed_field<'a>(
    bytes: &'a [u8],
    name: &str,
    encoding: ParseEncoding,
    field_length: LengthUnit,
) -> Result<Field<'a>, StructuralError> {
    let mut parser = StrictParser::new(
        bytes,
        ParseOpts {
            encoding,
            field_length,
        },
    );
    match parser.next() {
        Some(Ok(Section::Record(r))) => Ok(named(&r, name).clone()),
        Some(Err(e)) => Err(e),
        other => panic!("expected a record or its parsing error, got {other:?}; input {bytes:?}"),
    }
}
#[track_caller]
pub(super) fn fixture_record(
    bytes: &[u8],
    encoding: ParseEncoding,
    unit: LengthUnit,
) -> Record<'_> {
    let parsed = parse_file(bytes, encoding, unit);
    assert!(parsed.header.is_some(), "expected fixture header");
    assert_eq!(parsed.records.len(), 1, "{parsed:?}");
    parsed
        .records
        .into_iter()
        .next()
        .expect("record should exist because its count was checked as one")
}
macro_rules! fixture {
    ($path:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/",
            $path
        ))
        .as_slice()
    };
}
