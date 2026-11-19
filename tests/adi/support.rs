use qso_io::adif::adi::parser::error::StructuralError;
use qso_io::adif::adi::parser::options::{LengthUnit, ParseEncoding, ParseOpts};
use qso_io::adif::adi::parser::strict::declarations::UserDefinition;
use qso_io::adif::adi::parser::strict::field::Field;
use qso_io::adif::adi::parser::strict::header::{Header, HeaderEntry};
use qso_io::adif::adi::parser::strict::record::Record;
use qso_io::adif::adi::parser::strict::{Section, Sections, StrictParser};

#[derive(Debug)]
pub(super) struct ParsedSections<'a, 'header> {
    pub header: Option<&'header Header<'a>>,
    pub records: Vec<Record<'a>>,
}

#[track_caller]
pub(super) fn next_section<'a, 'header>(
    parser: &mut Sections<'a, 'header>,
) -> Section<'a, 'header> {
    match parser.next() {
        Some(Ok(s)) => s,
        other => panic!("expected complete section, got {other:?}; parser: {parser:?}"),
    }
}

#[track_caller]
pub(super) fn next_error<'header, P, S>(parser: &mut P) -> StructuralError<'header>
where
    P: Iterator<Item = Result<S, StructuralError<'header>>> + std::fmt::Debug,
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
pub(super) fn record<'a>(section: Section<'a, '_>) -> Record<'a> {
    match section {
        Section::Record(r) => r,
        other => panic!("expected record, got {other:?}"),
    }
}

#[track_caller]
pub(super) fn named<'r, 'a>(record: &'r Record<'a>, name: &str) -> &'r Field<'a> {
    match record.field(name) {
        Ok(Some(f)) => f,
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
pub(super) fn parse_file<'a, 'header>(
    parser: &'header mut StrictParser<'a>,
) -> ParsedSections<'a, 'header> {
    let mut parsed = ParsedSections {
        header: None,
        records: Vec::new(),
    };

    for section in parser {
        match section {
            Ok(Section::Header(h)) => {
                assert!(parsed.header.is_none(), "unexpected second header");
                parsed.header = Some(h);
            }
            Ok(Section::Record(r)) => parsed.records.push(r),
            Err(e) => panic!("expected successful document: {e:?}"),
        }
    }

    parsed
}

#[track_caller]
pub(super) fn parsed_field<'a, 'header>(
    owner: &'header mut StrictParser<'a>,
    name: &str,
) -> Result<Field<'a>, StructuralError<'header>> {
    let mut parser = owner.into_iter();
    match parser.next() {
        Some(Ok(Section::Record(r))) => Ok(named(&r, name).clone()),
        Some(Err(e)) => Err(e),
        other => panic!("expected a record or its parsing error, got {other:?}"),
    }
}

#[track_caller]
pub(super) fn fixture_record(
    bytes: &[u8],
    encoding: ParseEncoding,
    unit: LengthUnit,
) -> Record<'_> {
    let mut owner = StrictParser::new(
        bytes,
        ParseOpts {
            encoding,
            field_length: unit,
        },
    );
    let parsed = parse_file(&mut owner);
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
