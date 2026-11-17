use super::support::*;
use qso_io::adif::adi::parser::error;
use qso_io::adif::adi::parser::error::{StructuralErrorKind, SyntaxErrorKind};
use qso_io::adif::adi::parser::options::{LengthUnit, ParseEncoding, ParseOpts};
use qso_io::adif::adi::parser::strict::{Section, StrictParser};

#[test]
fn upstream_matir_headerless_fixture_has_one_record() {
    let file = parse_file(
        fixture!("matir-adifparser/testdata/header_none.adi"),
        ParseEncoding::Ascii,
        LengthUnit::Bytes,
    );
    assert!(file.header.is_none());
    assert_eq!(file.records.len(), 1);
    assert_eq!(named(&file.records[0], "MYCALL").text(), "KF4MDV");
}

#[test]
fn upstream_matir_preamble_fixture_preserves_header_text() {
    let file = parse_file(
        fixture!("matir-adifparser/testdata/header_comment.adi"),
        ParseEncoding::Ascii,
        LengthUnit::Bytes,
    );
    assert_eq!(
        file.header
            .expect("input should yield a header because it starts with a preamble")
            .text(),
        "This is a comment at the start of the file."
    );
    assert_eq!(file.records.len(), 1);
    assert_eq!(named(&file.records[0], "MYCALL").text(), "KF4MDV");
}

#[test]
fn upstream_difa_fixture_preserves_prefix_before_invalid_mode() {
    let bytes = fixture!("difa/examples/sample.adif");
    let mut parser = StrictParser::new(bytes, ParseOpts::default());
    let Section::Header(header) = next_section(&mut parser) else {
        panic!("fixture should start with a header");
    };
    assert_eq!(
        header
            .field("ADIF_VER")
            .expect("strict parser should yield unique descriptor names")
            .expect("fixture header should contain ADIF_VER")
            .text(),
        "3.1.4"
    );
    for (call, time) in [("W1AW", "1430"), ("AB9BH", "1445")] {
        let section = next_section(&mut parser);
        let Section::Record(record) = section else {
            panic!("expected {call} record, got {section:?}");
        };
        assert_eq!(named(&record, "CALL").text(), call);
        assert_eq!(named(&record, "TIME_ON").text(), time);
    }
    let error = next_error(&mut parser);
    assert!(
        matches!(error.kind(), error::StructuralErrorKind::InvalidValue {field_name, enumeration: Some("Mode"), ..} if field_name == "MODE"),
        "{error:?}"
    );
    assert_eq!(&bytes[error.span()], b"Q65");
    assert_exhausted(&mut parser);
}

#[test]
fn upstream_wsjtx_fixture_preserves_frequency_and_signal_report_text() {
    let file = parse_file(
        fixture!("adif-parser-ts/test/data/wsjtx.adi"),
        ParseEncoding::Ascii,
        LengthUnit::Bytes,
    );
    assert_eq!(file.records.len(), 1);
    assert_eq!(named(&file.records[0], "CALL").text(), "KA9FOX");
    assert_eq!(named(&file.records[0], "FREQ").text(), "7.076150");
    assert_eq!(named(&file.records[0], "RST_SENT").text(), "-07");
    assert_eq!(named(&file.records[0], "TIME_ON").text(), "154515");
}

#[test]
fn upstream_js8call_fixture_preserves_explicit_empty_values() {
    let file = parse_file(
        fixture!("adif-parser-ts/test/data/js8call.adi"),
        ParseEncoding::Ascii,
        LengthUnit::Bytes,
    );
    assert_eq!(file.records.len(), 1);
    let record = &file.records[0];
    assert_eq!(named(record, "CALL").text(), "VE4DL");
    assert_eq!(named(record, "SUBMODE").text(), "JS8");
    for name in ["GRIDSQUARE", "RST_RCVD"] {
        let field = named(record, name);
        assert_eq!(field.declared_length(), 0);
        assert_eq!(field.raw_data(), b"");
        assert_eq!(field.text(), "");
    }
}

#[test]
fn upstream_transadif_ascii_fixture_preserves_crlf_preamble_and_values() {
    let file = parse_file(
        fixture!("transadif/test-cases/01-plain-examples/1-plain-ascii-in.adi"),
        ParseEncoding::Ascii,
        LengthUnit::Bytes,
    );
    assert!(
        file.header
            .expect("input should yield a header because it starts with a preamble")
            .raw_text()
            .starts_with(b"Plain ASCII file with no encoding specified.\r\n\r\n")
    );
    assert_eq!(file.records.len(), 1);
    assert_eq!(named(&file.records[0], "NAME").text(), "John ASCII");
    assert_eq!(named(&file.records[0], "TIME_ON").text(), "0100");
}

// Parsing must complete with exactly one record. The test does not select
// which parser consumes the fixture's separators.
#[test]
fn upstream_latin1_fixture_uses_original_single_byte_accents() {
    let record = fixture_record(
        fixture!("transadif/test-cases/01-plain-examples/2-plain-iso-in.adi"),
        ParseEncoding::Latin1,
        LengthUnit::Bytes,
    );
    let name = named(&record, "NAME");
    assert_eq!(name.raw_data(), b"Ju\xe1n Mu\xf1oz ISO");
    assert_eq!(name.text(), "Juán Muñoz ISO");
}

#[test]
fn upstream_utf8_fixture_respects_explicit_byte_or_scalar_lengths() {
    let bytes = fixture!("transadif/test-cases/01-plain-examples/3-plain-utf-in.adi");
    for (unit, expected) in [
        (LengthUnit::Bytes, "Juán Muñoz U"),
        (LengthUnit::UnicodeScalars, "Juán Muñoz UTF"),
    ] {
        let record = fixture_record(bytes, ParseEncoding::Utf8, unit);
        assert_eq!(named(&record, "NAME").raw_data(), expected.as_bytes());
        assert_eq!(named(&record, "NAME").text(), expected);
        assert_eq!(named(&record, "QSO_DATE").text(), "20250901");
    }
}

#[test]
fn upstream_overcount_fixture_has_utf8_bytes_despite_its_preamble_label() {
    let record = fixture_record(
        fixture!("transadif/test-cases/02-field-length/2-overcount-in.adi"),
        ParseEncoding::Utf8,
        LengthUnit::Bytes,
    );
    assert_eq!(
        named(&record, "NAME").raw_data(),
        "Juán Muñoz Marín OVER".as_bytes()
    );
    assert_eq!(named(&record, "NAME").text(), "Juán Muñoz Marín OVER");
    assert_eq!(named(&record, "QSO_DATE").text(), "20250902");
}

#[test]
fn upstream_unterminated_fixture_has_two_complete_records_before_failure() {
    let bytes = fixture!("matir-adifparser/testdata/readrecord.adi");
    let mut parser = StrictParser::new(bytes, ParseOpts::default());
    let first = record(next_section(&mut parser));
    let second = record(next_section(&mut parser));
    assert_eq!(named(&first, "MYCALL").text(), "KF4MDV");
    assert_eq!(named(&second, "MYCALL").text(), "KG4JEL");
    let error = next_error(&mut parser);
    assert!(
        matches!(
            error.kind(),
            StructuralErrorKind::Syntax {
                reason: SyntaxErrorKind::MissingEor
            }
        ),
        "{error:?}"
    );
    assert_eq!(error.span(), bytes.len()..bytes.len());
    assert_exhausted(&mut parser);
}

#[test]
fn undercount_fixture_retains_declared_contents_and_strict_rejection_locations() {
    use qso_io::adif::adi::parser::permissive::{PermissiveParser, PermissiveSection};
    let bytes = fixture!("transadif/test-cases/02-field-length/1-undercount-in.adi");
    for (field_length, name) in [
        (LengthUnit::Bytes, "Juán Muñoz UNDER"),
        (LengthUnit::UnicodeScalars, "Juán Muñoz UNDER\r\n"),
    ] {
        let opts = ParseOpts {
            encoding: ParseEncoding::Utf8,
            field_length,
        };
        let mut p = PermissiveParser::new(bytes, opts);
        assert!(matches!(p.next(), Some(Ok(PermissiveSection::Header(_)))));
        let r = match p.next() {
            Some(Ok(PermissiveSection::Record(r))) => r,
            other => panic!("expected framed fixture record, got {other:?}"),
        };
        assert_eq!(
            r.field("name")
                .expect("fixture names should be unique")
                .expect("fixture supplies NAME")
                .raw_data(),
            name.as_bytes()
        );
        for n in ["notes", "comment"] {
            assert_eq!(
                r.field(n)
                    .expect("fixture names should be unique")
                    .expect("fixture supplies this field")
                    .raw_data(),
                b"UTF with undercount if read as ISO\r"
            );
        }
        assert_exhausted(&mut p);
        let mut s = StrictParser::new(bytes, opts);
        let _ = next_section(&mut s);
        let e = next_error(&mut s);
        let name = if field_length == LengthUnit::Bytes {
            "notes"
        } else {
            "name"
        };
        assert!(
            matches!(e.kind(),StructuralErrorKind::InvalidValue{field_name,..}if field_name==name),
            "{e:?}"
        );
        assert_eq!(&bytes[e.span()], b"\r");
        assert_exhausted(&mut s);
    }
}

#[test]
fn overcount_scalar_fixture_preserves_the_consumed_opening_bracket() {
    use qso_io::adif::adi::parser::permissive::{PermissiveParser, PermissiveSection};
    let bytes = fixture!("transadif/test-cases/02-field-length/2-overcount-in.adi");
    let opts = ParseOpts {
        encoding: ParseEncoding::Utf8,
        field_length: LengthUnit::UnicodeScalars,
    };
    let mut p = PermissiveParser::new(bytes, opts);
    assert!(matches!(p.next(), Some(Ok(PermissiveSection::Header(_)))));
    let r = match p.next() {
        Some(Ok(PermissiveSection::Record(r))) => r,
        other => panic!("expected overcount-framed record, got {other:?}"),
    };
    assert_eq!(
        r.field("name")
            .expect("fixture names should be unique")
            .expect("fixture supplies NAME")
            .raw_data(),
        "Juán Muñoz Marín OVER\r\n<".as_bytes()
    );
    assert!(
        r.field("qso_date")
            .expect("lookup should not be ambiguous")
            .is_none()
    );
    assert_eq!(
        r.field("time_on")
            .expect("fixture names should be unique")
            .expect("fixture supplies TIME_ON")
            .text(),
        Ok("0200")
    );
    assert_exhausted(&mut p);
}

#[test]
fn missing_length_fixture_is_rejected_as_an_invalid_descriptor() {
    let bytes = fixture!("matir-adifparser/testdata/header_version.adi");
    let mut p = StrictParser::new(bytes, ParseOpts::default());
    let error = next_error(&mut p);
    assert!(
        matches!(error.kind(), StructuralErrorKind::Syntax { .. }),
        "{error:?}"
    );
    assert_exhausted(&mut p);
}
