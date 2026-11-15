//! Shared framing requirements, observed through both public parsing interfaces.
use super::support::*;
use qso_io::adif::adi::parser::error::{StructuralErrorKind, SyntaxErrorKind};
use qso_io::adif::adi::parser::options::{LengthUnit, ParseEncoding, ParseOpts};
use qso_io::adif::adi::parser::permissive::{PermissiveParser, PermissiveSection};
use qso_io::adif::adi::parser::strict::StrictParser;
use qso_io::adif::{DataType, DecodeError};

#[test]
fn descriptors_preserve_spelling_leading_zero_lengths_and_explicit_empty_values() {
    let bytes = b"<nAmE:003>Ray<EMPTY:0><QTH:3>Ann<EOR>";
    let mut strict = StrictParser::new(bytes, ParseOpts::default());
    let r = record(next_section(&mut strict));
    assert_eq!(
        r.fields().iter().map(|f| f.name()).collect::<Vec<_>>(),
        ["nAmE", "EMPTY", "QTH"]
    );
    assert_eq!(r.fields()[0].declared_length(), 3);
    assert_eq!(r.fields()[0].descriptor_span(), 0..10);
    assert_eq!(r.fields()[0].value_span(), 10..13);
    assert_eq!(r.fields()[0].text(), "Ray");
    assert_eq!(r.fields()[1].value(), None);
    assert_eq!(r.fields()[1].raw_data(), b"");
    assert_eq!(r.fields()[1].value_span(), 22..22);
    assert_eq!(r.fields()[2].text(), "Ann");
    assert_exhausted(&mut strict);
    let mut permissive = PermissiveParser::new(bytes, ParseOpts::default());
    let Some(Ok(PermissiveSection::Record(r))) = permissive.next() else {
        panic!("expected complete record: {permissive:?}")
    };
    assert_eq!(
        r.fields().iter().map(|f| f.name()).collect::<Vec<_>>(),
        ["nAmE", "EMPTY", "QTH"]
    );
    assert_eq!(r.fields()[0].declared_length(), 3);
    assert_eq!(r.fields()[1].text(), Ok(""));
    assert_eq!(r.fields()[2].text(), Ok("Ann"));
    assert_exhausted(&mut permissive);
}

#[test]
fn strict_indicator_case_and_adi_eligibility_are_public_behavior() {
    for (indicator, expected) in [
        ('B', DataType::Boolean),
        ('N', DataType::Number),
        ('D', DataType::Date),
        ('T', DataType::Time),
        ('S', DataType::String),
        ('M', DataType::MultilineString),
        ('E', DataType::Enumeration),
        ('L', DataType::Location),
        ('I', DataType::IntlString),
        ('G', DataType::IntlMultilineString),
    ] {
        for supplied in [indicator, indicator.to_ascii_lowercase()] {
            let input = format!("<APP_TEST_VALUE:0:{supplied}><EOR>");
            let mut parser = StrictParser::new(input.as_bytes(), ParseOpts::default());
            if matches!(
                expected,
                DataType::IntlString | DataType::IntlMultilineString
            ) {
                let error = next_error(&mut parser);
                assert!(
                    matches!(error.kind(),StructuralErrorKind::TypeUnavailableInAdi{data_type}if *data_type==expected),
                    "{input}: {error:?}"
                );
                assert_eq!(error.span(), 18..19);
            } else {
                let r = record(next_section(&mut parser));
                let f = named(&r, "APP_TEST_VALUE");
                assert_eq!(f.type_indicator(), Some(expected), "{input}");
                assert_eq!(f.value(), None);
            }
            assert_exhausted(&mut parser);
        }
    }
}

#[test]
fn malformed_lengths_and_descriptors_fail_through_both_interfaces() {
    for bytes in [
        b"<NAME>Ray".as_slice(),
        b"<NAME:>Ray",
        b"<NAME:-1>Ray",
        b"<NAME:+3>Ray",
        b"<NAME:x>Ray",
        b"<NAME:3x>Ray",
        b"<NAME:3",
        b"<EOR>",
    ] {
        let mut strict = StrictParser::new(bytes, ParseOpts::default());
        let mut permissive = PermissiveParser::new(bytes, ParseOpts::default());
        for error in [next_error(&mut strict), next_error(&mut permissive)] {
            assert!(
                matches!(error.kind(), StructuralErrorKind::Syntax { .. }),
                "input {bytes:?}: {error:?}"
            );
        }
        assert_exhausted(&mut strict);
        assert_exhausted(&mut permissive);
    }
    for code in ["Z", "z", "", "SS", "1"] {
        let bytes = format!("<NAME:3:{code}>Ray<EOR>");
        let mut parser = StrictParser::new(bytes.as_bytes(), ParseOpts::default());
        assert!(
            matches!(
                next_error(&mut parser).kind(),
                StructuralErrorKind::Syntax {
                    reason: SyntaxErrorKind::UnknownTypeIndicator
                }
            ),
            "{bytes}"
        );
        assert_exhausted(&mut parser);
    }
}

#[test]
fn header_detection_and_preamble_preservation_use_the_first_byte() {
    for (bytes, expected) in [
        (b"<NAME:3>Ray<EOR>".as_slice(), None),
        (
            b"Generated<eOh><NAME:3>Ray<eOr>",
            Some(b"Generated".as_slice()),
        ),
        (
            b"First\r\nSecond\r\n\r\n<EOH><NAME:3>Ray<EOR>",
            Some(b"First\r\nSecond\r\n\r\n".as_slice()),
        ),
        (b" \n<EOH><NAME:3>Ray<EOR>", Some(b" \n".as_slice())),
    ] {
        let parsed = parse_file(bytes, ParseEncoding::Ascii, LengthUnit::Bytes);
        assert_eq!(parsed.header.as_ref().map(|h| h.raw_text()), expected);
        assert_eq!(parsed.records.len(), 1);
        assert_eq!(named(&parsed.records[0], "NAME").text(), "Ray");
        let mut p = PermissiveParser::new(bytes, ParseOpts::default());
        if let Some(expected) = expected {
            match p.next() {
                Some(Ok(PermissiveSection::Header(h))) => {
                    assert_eq!(h.raw_text(), expected);
                    assert_eq!(h.text().map(str::as_bytes), Ok(expected));
                    assert!(h.fields().is_empty());
                }
                other => panic!("expected header, got {other:?}"),
            }
        }
        assert!(matches!(p.next(), Some(Ok(PermissiveSection::Record(_)))));
        assert_exhausted(&mut p);
    }
    for bytes in [
        b"Generated".as_slice(),
        b"Generated<PROGRAMID:3>abc",
        b"Generated<PROGRAMID:x>abc<EOH><NAME:3>Ray<EOR>",
        b"\n<NAME:3>Ray<EOR>",
    ] {
        let mut s = StrictParser::new(bytes, ParseOpts::default());
        let mut p = PermissiveParser::new(bytes, ParseOpts::default());
        let _ = next_error(&mut s);
        let _ = next_error(&mut p);
        assert_exhausted(&mut s);
        assert_exhausted(&mut p);
    }
}

#[test]
fn ignored_text_preserves_order_record_boundaries_and_terminal_line_breaks() {
    for gap in ["", "\n", "\r\n", "arbitrary record separator "] {
        for end in ["", "\n", "\r\n"] {
            let bytes = format!(
                "<NAME:3>Ray # arbitrary text\r\n<QSO_DATE:8>19960513 text <qth:3>Ann tail <eOr>{gap}<NAME:3>Bob<EOR>{end}"
            );
            let parsed = parse_file(bytes.as_bytes(), ParseEncoding::Ascii, LengthUnit::Bytes);
            assert_eq!(parsed.records.len(), 2);
            assert_eq!(
                parsed.records[0]
                    .fields()
                    .iter()
                    .map(|f| (f.name(), f.text()))
                    .collect::<Vec<_>>(),
                [("NAME", "Ray"), ("QSO_DATE", "19960513"), ("qth", "Ann")]
            );
            assert_eq!(named(&parsed.records[1], "NAME").text(), "Bob");
            let sections = PermissiveParser::new(bytes.as_bytes(), ParseOpts::default())
                .collect::<Result<Vec<_>, _>>()
                .expect("supplied complete records and separators should frame");
            assert_eq!(sections.len(), 2, "{bytes}");
        }
    }
    assert!(
        StrictParser::new(b"", ParseOpts::default())
            .next()
            .is_none()
    );
    assert!(
        PermissiveParser::new(b"", ParseOpts::default())
            .next()
            .is_none()
    );
}

#[test]
fn scalar_lengths_preserve_all_utf8_widths_combining_scalars_and_following_fields() {
    for (input, expected) in [
        ("<NAME:4>Aé€😀<QTH:3>Ray<EOR>", "Aé€😀"),
        ("<NAME:2>e\u{301}<QTH:3>Ray<EOR>", "e\u{301}"),
    ] {
        let opts = ParseOpts {
            encoding: ParseEncoding::Utf8,
            field_length: LengthUnit::UnicodeScalars,
        };
        let mut s = StrictParser::new(input.as_bytes(), opts);
        let r = record(next_section(&mut s));
        assert_eq!(named(&r, "NAME").raw_data(), expected.as_bytes());
        assert_eq!(named(&r, "NAME").text(), expected);
        assert_eq!(named(&r, "QTH").text(), "Ray");
        assert_exhausted(&mut s);
        let mut p = PermissiveParser::new(input.as_bytes(), opts);
        match p.next() {
            Some(Ok(PermissiveSection::Record(r))) => {
                assert_eq!(r.fields()[0].raw_data(), expected.as_bytes());
                assert_eq!(r.fields()[1].text(), Ok("Ray"));
            }
            other => panic!("expected complete scalar-framed record, got {other:?}"),
        }
        assert_exhausted(&mut p);
    }
}

#[test]
fn missing_terminators_and_truncation_retain_public_counts_and_locations() {
    for encoding in [
        ParseEncoding::Ascii,
        ParseEncoding::Latin1,
        ParseEncoding::Utf8,
    ] {
        for field_length in [LengthUnit::Bytes, LengthUnit::UnicodeScalars] {
            let opts = ParseOpts {
                encoding,
                field_length,
            };
            for (bytes, name, declared, available) in [
                (b"<NAME:3>Ra".as_slice(), "NAME", 3, 2),
                (b"<NAME:1>", "NAME", 1, 0),
                (b"Generated<PROGRAMID:3>ab", "PROGRAMID", 3, 2),
            ] {
                let mut s = StrictParser::new(bytes, opts);
                let mut p = PermissiveParser::new(bytes, opts);
                for error in [next_error(&mut s), next_error(&mut p)] {
                    assert_eq!(error.span(), bytes.len()..bytes.len());
                    assert!(
                        matches!(error.kind(),StructuralErrorKind::TruncatedValue{field_name,declared:d,available:a,unit} if field_name==name&&*d==declared&&*a==available&&*unit==field_length),
                        "{encoding:?}/{field_length:?}: {error:?}"
                    );
                }
                assert_exhausted(&mut s);
                assert_exhausted(&mut p);
            }
        }
    }
    for (bytes, reason) in [
        (b"<NAME:3>Ray".as_slice(), SyntaxErrorKind::MissingEor),
        (
            b"<NAME:3>Ray trailing text\r\n",
            SyntaxErrorKind::MissingEor,
        ),
        (b"<NAME:0>", SyntaxErrorKind::MissingEor),
        (b"Generated<PROGRAMID:0>", SyntaxErrorKind::MissingEoh),
        (
            b"Generated<PROGRAMID:3>abc trailing\n",
            SyntaxErrorKind::MissingEoh,
        ),
    ] {
        let mut s = StrictParser::new(bytes, ParseOpts::default());
        let mut p = PermissiveParser::new(bytes, ParseOpts::default());
        for error in [next_error(&mut s), next_error(&mut p)] {
            assert_eq!(error.span(), bytes.len()..bytes.len());
            assert!(
                matches!(error.kind(),StructuralErrorKind::Syntax{reason:r}if *r==reason),
                "{error:?}"
            );
        }
    }
    for (input, unit, available) in [
        ("<NAME:4>Ré", LengthUnit::Bytes, 3),
        ("<NAME:4>Ré", LengthUnit::UnicodeScalars, 2),
    ] {
        let opts = ParseOpts {
            encoding: ParseEncoding::Utf8,
            field_length: unit,
        };
        let mut s = StrictParser::new(input.as_bytes(), opts);
        let mut p = PermissiveParser::new(input.as_bytes(), opts);
        for error in [next_error(&mut s), next_error(&mut p)] {
            assert_eq!(error.span(), input.len()..input.len());
            assert!(
                matches!(error.kind(),StructuralErrorKind::TruncatedValue{declared:4,available:a,unit:u,..}if *a==available&&*u==unit),
                "{error:?}"
            );
        }
    }
}

#[test]
fn malformed_fields_keep_their_diagnostics_in_unfinished_sections() {
    for (bytes, reason, span) in [
        (b"<NAME:".as_slice(), SyntaxErrorKind::InvalidLength, 6..6),
        (
            b"Generated<PROGRAMID:",
            SyntaxErrorKind::InvalidLength,
            20..20,
        ),
        (
            b"<NAME:3:Z>Ray",
            SyntaxErrorKind::UnknownTypeIndicator,
            8..9,
        ),
        (
            b"Generated<PROGRAMID:3:Z>abc",
            SyntaxErrorKind::UnknownTypeIndicator,
            22..23,
        ),
    ] {
        let mut s = StrictParser::new(bytes, ParseOpts::default());
        let error = next_error(&mut s);
        assert_eq!(error.span(), span);
        assert!(
            matches!(error.kind(),StructuralErrorKind::Syntax{reason:r}if *r==reason),
            "{error:?}"
        );
        assert_exhausted(&mut s);
    }
    for (bytes, start) in [
        (b"<NAME:4>Jos\xff".as_slice(), 11),
        (b"<NAME:4>Jos\xc3", 11),
        (b"Generated<PROGRAMID:4>Jos\xff", 25),
        (b"Generated<PROGRAMID:4>Jos\xc3", 25),
    ] {
        let opts = ParseOpts {
            encoding: ParseEncoding::Utf8,
            field_length: LengthUnit::UnicodeScalars,
        };
        let mut s = StrictParser::new(bytes, opts);
        let mut p = PermissiveParser::new(bytes, opts);
        for error in [next_error(&mut s), next_error(&mut p)] {
            assert_eq!(error.span().start, start);
            assert!(
                matches!(
                    error.kind(),
                    StructuralErrorKind::Decoding {
                        cause: DecodeError::Utf8(_)
                    }
                ),
                "{error:?}"
            );
        }
    }
}

#[test]
fn decoding_is_confined_to_framed_values_and_preserves_untyped_repertoires() {
    let mut s = StrictParser::new(
        b"<NAME:1>A\xff<EOR>",
        ParseOpts {
            encoding: ParseEncoding::Utf8,
            field_length: LengthUnit::UnicodeScalars,
        },
    );
    assert_eq!(named(&record(next_section(&mut s)), "NAME").text(), "A");
    assert_exhausted(&mut s);
    let parsed = parse_file(
        b"<UNKNOWN:4>\0\r\n\x7f<EOR>",
        ParseEncoding::Ascii,
        LengthUnit::Bytes,
    );
    assert_eq!(
        named(&parsed.records[0], "UNKNOWN").raw_data(),
        b"\0\r\n\x7f"
    );
    let parsed = parse_file(
        b"<TEST:3>\x80\xe9\xff<EOR>",
        ParseEncoding::Latin1,
        LengthUnit::Bytes,
    );
    assert_eq!(named(&parsed.records[0], "TEST").text(), "\u{80}éÿ");
    assert_eq!(
        named(&parsed.records[0], "TEST").raw_data(),
        b"\x80\xe9\xff"
    );
}
