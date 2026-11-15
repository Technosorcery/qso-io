//! Public framing-only contracts for imperfect ADI input.

use qso_io::adif::DecodeError;
use qso_io::adif::adi::parser::error::{StructuralError, StructuralErrorKind, SyntaxErrorKind};
use qso_io::adif::adi::parser::framing::header::PermissiveHeader;
use qso_io::adif::adi::parser::framing::record::PermissiveRecord;
use qso_io::adif::adi::parser::options::{LengthUnit, ParseEncoding, ParseOpts};
use qso_io::adif::adi::parser::permissive::{PermissiveParser, PermissiveSection};

#[track_caller]
fn next_section<'a>(parser: &mut PermissiveParser<'a>) -> PermissiveSection<'a> {
    match parser.next() {
        Some(Ok(section)) => section,
        other => panic!("expected complete permissive section, got {other:?}; parser: {parser:?}"),
    }
}

#[track_caller]
fn record(section: PermissiveSection<'_>) -> PermissiveRecord<'_> {
    match section {
        PermissiveSection::Record(record) => record,
        other => panic!("expected record, got {other:?}"),
    }
}

#[track_caller]
fn header(section: PermissiveSection<'_>) -> PermissiveHeader<'_> {
    match section {
        PermissiveSection::Header(header) => header,
        other => panic!("expected header, got {other:?}"),
    }
}

#[track_caller]
fn terminal_error(parser: &mut PermissiveParser<'_>) -> StructuralError {
    let error = match parser.next() {
        Some(Err(error)) => error,
        other => panic!("expected terminal error, got {other:?}; parser: {parser:?}"),
    };
    for _ in 0..2 {
        assert!(
            parser.next().is_none(),
            "parser must remain exhausted after {error:?}"
        );
    }
    error
}

fn options(encoding: ParseEncoding, field_length: LengthUnit) -> ParseOpts {
    ParseOpts {
        encoding,
        field_length,
    }
}

#[test]
fn preserves_unvalidated_contents_and_indicators() {
    let bytes = b"<K_INDEX:4>1.33<X:3:Z>abc<Y:3:N:extra>def<Z:3:I>ghi<EOR>";
    let mut parser = PermissiveParser::new(bytes, ParseOpts::default());
    let record = record(next_section(&mut parser));
    assert_eq!(record.span(), 0..bytes.len());
    let fields = record.fields();
    assert_eq!(fields.len(), 4);
    for (field, text, indicator) in [
        (&fields[0], "1.33", None),
        (&fields[1], "abc", Some("Z")),
        (&fields[2], "def", Some("N:extra")),
        (&fields[3], "ghi", Some("I")),
    ] {
        assert_eq!(field.text(), Ok(text));
        assert_eq!(field.type_indicator(), indicator);
        assert_eq!(field.raw_data(), &bytes[field.value_span()]);
        assert_eq!(bytes[field.descriptor_span().start], b'<');
    }
    assert!(parser.next().is_none());
}

#[test]
fn preserves_empty_descriptor_components_and_values() {
    let mut parser = PermissiveParser::new(b"<:0><X:0:><EOR>", ParseOpts::default());
    let record = record(next_section(&mut parser));
    for (field, name, indicator, span) in [
        (&record.fields()[0], "", None, 4..4),
        (&record.fields()[1], "X", Some(""), 10..10),
    ] {
        assert_eq!(field.name(), name);
        assert_eq!(field.type_indicator(), indicator);
        assert_eq!(field.declared_length(), 0);
        assert_eq!(field.raw_data(), b"");
        assert_eq!(field.text(), Ok(""));
        assert_eq!(field.value_span(), span);
    }
}

#[test]
fn preserves_uninterpreted_header_declarations() {
    let input = b"H<USERDEF1:1:Z>{<USERDEF1:1>N<USERDEF2:1>N<USERDEF3:4>CALL<EOH><X:1>a<EOR>";
    let mut parser = PermissiveParser::new(input, ParseOpts::default());
    let header = header(next_section(&mut parser));
    assert_eq!(header.text(), Ok("H"));
    assert_eq!(
        header
            .fields()
            .iter()
            .map(|f| (f.name(), f.text()))
            .collect::<Vec<_>>(),
        [
            ("USERDEF1", Ok("{")),
            ("USERDEF1", Ok("N")),
            ("USERDEF2", Ok("N")),
            ("USERDEF3", Ok("CALL"))
        ]
    );
    assert_eq!(
        record(next_section(&mut parser)).fields()[0].text(),
        Ok("a")
    );
}

#[test]
fn retains_known_boundary_decode_failures() {
    let bytes = b"<X:2>a\xff<Y:1>b<EOR><Z:1>c<EOR>";
    for encoding in [ParseEncoding::Ascii, ParseEncoding::Utf8] {
        let mut parser = PermissiveParser::new(bytes, options(encoding, LengthUnit::Bytes));
        let record = record(next_section(&mut parser));
        let field = &record.fields()[0];
        assert_eq!(field.raw_data(), b"a\xff");
        assert_eq!(field.value_span(), 5..7);
        match (encoding, field.text()) {
            (ParseEncoding::Ascii, Err(DecodeError::NonAscii { offset: 1 })) => {}
            (ParseEncoding::Utf8, Err(DecodeError::Utf8(cause))) => {
                assert_eq!(cause.valid_up_to(), 1)
            }
            other => panic!("expected located decoding failure, got {other:?}"),
        }
        assert!(std::ptr::eq(
            field
                .text()
                .expect_err("invalid bytes should retain a decoding error"),
            field
                .text()
                .expect_err("repeated access should borrow the cached error")
        ));
        assert_eq!(record.fields()[1].text(), Ok("b"));
        let later = next_section(&mut parser);
        assert!(
            matches!(later, PermissiveSection::Record(_)),
            "expected later record, got {later:?}"
        );
        assert!(parser.next().is_none());
    }
}

#[test]
fn retains_undecodable_header_preamble() {
    let bytes = b"\xff<EOH><X:1>a<EOR>";
    let mut parser = PermissiveParser::new(bytes, options(ParseEncoding::Utf8, LengthUnit::Bytes));
    let header = header(next_section(&mut parser));
    assert_eq!(header.raw_text(), b"\xff");
    assert_eq!(header.text_span(), 0..1);
    assert_eq!(header.span(), 0..6);
    assert!(matches!(header.text(), Err(DecodeError::Utf8(_))));
    assert_eq!(
        record(next_section(&mut parser)).fields()[0].text(),
        Ok("a")
    );
}

#[test]
fn scalar_boundary_failures_remain_terminal() {
    let opts = options(ParseEncoding::Utf8, LengthUnit::UnicodeScalars);
    let mut parser = PermissiveParser::new(b"<X:2>a\xff<EOR>", opts);
    let error = terminal_error(&mut parser);
    assert_eq!(error.span(), 6..7);
    assert!(matches!(
        error.kind(),
        StructuralErrorKind::Decoding {
            cause: DecodeError::Utf8(_)
        }
    ));

    let input = "<X:2>é".as_bytes();
    let mut parser = PermissiveParser::new(input, opts);
    let error = terminal_error(&mut parser);
    assert_eq!(error.span(), input.len()..input.len());
    assert!(matches!(
        error.kind(),
        StructuralErrorKind::TruncatedValue {
            declared: 2,
            available: 1,
            unit: LengthUnit::UnicodeScalars,
            ..
        }
    ));
}

#[test]
fn preserves_encoding_and_length_boundaries() {
    for (input, encoding, unit, raw, text) in [
        (
            "<X:4>José<EOR>".as_bytes(),
            ParseEncoding::Utf8,
            LengthUnit::UnicodeScalars,
            "José".as_bytes(),
            "José",
        ),
        (
            "<X:5>José<EOR>".as_bytes(),
            ParseEncoding::Utf8,
            LengthUnit::Bytes,
            "José".as_bytes(),
            "José",
        ),
        (
            &b"<X:4>Jos\xe9<EOR>"[..],
            ParseEncoding::Latin1,
            LengthUnit::UnicodeScalars,
            &b"Jos\xe9"[..],
            "José",
        ),
        (
            &b"<X:4>Jos\xe9<EOR>"[..],
            ParseEncoding::Latin1,
            LengthUnit::Bytes,
            &b"Jos\xe9"[..],
            "José",
        ),
    ] {
        let mut parser = PermissiveParser::new(input, options(encoding, unit));
        let record = record(next_section(&mut parser));
        assert_eq!(record.fields()[0].raw_data(), raw);
        assert_eq!(record.fields()[0].text(), Ok(text));
    }
    let mut parser = PermissiveParser::new(
        "<X:4>José<EOR>".as_bytes(),
        options(ParseEncoding::Utf8, LengthUnit::Bytes),
    );
    let record = record(next_section(&mut parser));
    assert_eq!(record.fields()[0].raw_data(), b"Jos\xc3");
    assert!(matches!(
        record.fields()[0].text(),
        Err(DecodeError::Utf8(_))
    ));
}

#[test]
fn reports_required_framing_failures() {
    for (bytes, reason) in [
        (&b"H<X:1>a"[..], SyntaxErrorKind::MissingEoh),
        (&b"<X:1>a"[..], SyntaxErrorKind::MissingEor),
        (&b"<X:>"[..], SyntaxErrorKind::InvalidLength),
        (&b"<X:-1>a"[..], SyntaxErrorKind::InvalidLength),
        (
            &b"<X:999999999999999999999999999999999999999>"[..],
            SyntaxErrorKind::LengthOverflow,
        ),
        (&b"<\xff:0><EOR>"[..], SyntaxErrorKind::InvalidNameEncoding),
        (
            &b"<X:0:\xff><EOR>"[..],
            SyntaxErrorKind::InvalidIndicatorEncoding,
        ),
    ] {
        let mut parser = PermissiveParser::new(bytes, ParseOpts::default());
        let error = terminal_error(&mut parser);
        assert!(
            matches!(error.kind(), StructuralErrorKind::Syntax { reason: actual } if *actual == reason),
            "input {bytes:?}: expected {reason:?}, got {error:?}"
        );
        if matches!(
            reason,
            SyntaxErrorKind::MissingEoh | SyntaxErrorKind::MissingEor
        ) {
            assert_eq!(error.span(), bytes.len()..bytes.len());
        }
    }
    let bytes = b"<X:1>a<EOR><Y:3>ab";
    let mut parser = PermissiveParser::new(bytes, ParseOpts::default());
    let first = record(next_section(&mut parser));
    let error = terminal_error(&mut parser);
    assert_eq!(first.fields()[0].text(), Ok("a"));
    assert_eq!(error.span(), bytes.len()..bytes.len());
    assert!(matches!(
        error.kind(),
        StructuralErrorKind::TruncatedValue {
            declared: 3,
            available: 2,
            unit: LengthUnit::Bytes,
            ..
        }
    ));
}

#[test]
fn honors_existing_section_boundaries() {
    let bytes = b"<X:5><EOR>ignored<eOr>arbitrary<Y:1>a<EOR>\r\n";
    let mut parser = PermissiveParser::new(bytes, ParseOpts::default());
    assert_eq!(
        record(next_section(&mut parser)).fields()[0].text(),
        Ok("<EOR>")
    );
    assert_eq!(
        record(next_section(&mut parser)).fields()[0].text(),
        Ok("a")
    );
    assert!(parser.next().is_none());
    assert!(
        PermissiveParser::new(b"", ParseOpts::default())
            .next()
            .is_none()
    );
}

#[test]
fn duplicate_lookup_reports_all_occurrences() {
    use qso_io::adif::adi::lookup::LookupError;

    let bytes = b"<CALL:1>a<X:1>x<call:1>b<CALL:1>c<EOR><Z:1>d<EOR>";
    let mut parser = PermissiveParser::new(bytes, ParseOpts::default());
    let record = record(next_section(&mut parser));
    let error = record
        .field("CaLl")
        .expect_err("three CALL descriptors should make lookup ambiguous");
    assert_eq!(
        error,
        LookupError::Ambiguous {
            name: "CaLl".into(),
            descriptor_spans: vec![0..8, 15..23, 24..32]
        }
    );
    assert_eq!(
        record
            .fields_named("call")
            .map(|f| f.text())
            .collect::<Vec<_>>(),
        [Ok("a"), Ok("b"), Ok("c")]
    );
    assert_eq!(
        record
            .field("x")
            .expect("X lookup should be unambiguous")
            .map(|f| f.text()),
        Some(Ok("x"))
    );
    assert!(matches!(
        next_section(&mut parser),
        PermissiveSection::Record(_)
    ));
}

#[test]
fn header_lookup_reports_all_duplicate_declarations() {
    use qso_io::adif::adi::lookup::LookupError;

    let bytes = b"H<USERDEF1:1>a<X:1>x<userdef1:1>b<USERDEF1:1>c<EOH>";
    let mut parser = PermissiveParser::new(bytes, ParseOpts::default());
    let header = header(next_section(&mut parser));
    let error = header
        .field("USERDEF1")
        .expect_err("three USERDEF1 descriptors should make lookup ambiguous");
    assert_eq!(
        error,
        LookupError::Ambiguous {
            name: "USERDEF1".into(),
            descriptor_spans: vec![1..13, 20..32, 33..45]
        }
    );
    assert_eq!(
        header
            .fields_named("UsErDeF1")
            .map(|f| f.text())
            .collect::<Vec<_>>(),
        [Ok("a"), Ok("b"), Ok("c")]
    );
    assert!(
        header
            .field("absent")
            .expect("absent name cannot be ambiguous")
            .is_none()
    );
}

#[test]
fn lookup_distinguishes_missing_empty_and_undecodable() {
    let mut parser = PermissiveParser::new(b"<EMPTY:0><BAD:1>\xff<EOR>", ParseOpts::default());
    let record = record(next_section(&mut parser));
    assert!(
        record
            .field("missing")
            .expect("missing name cannot be ambiguous")
            .is_none()
    );
    assert_eq!(record.fields_named("missing").count(), 0);
    let empty = record
        .field("empty")
        .expect("EMPTY has one occurrence")
        .expect("EMPTY descriptor is present");
    assert_eq!(empty.text(), Ok(""));
    let bad = record
        .field("bad")
        .expect("BAD has one occurrence")
        .expect("BAD descriptor is present");
    assert!(matches!(
        bad.text(),
        Err(DecodeError::NonAscii { offset: 0 })
    ));
}
