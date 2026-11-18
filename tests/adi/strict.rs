use super::support::*;
use qso_io::adif::adi::parser::error;
use qso_io::adif::adi::parser::error::{StructuralError, SyntaxErrorKind};
use qso_io::adif::adi::parser::options::{LengthUnit, ParseEncoding, ParseOpts};
use qso_io::adif::adi::parser::strict::{Section, StrictParser};
use qso_io::adif::{DataType, DecodeError};
use std::ops::Range;
#[path = "strict/applications.rs"]
mod applications;
#[path = "strict/declarations.rs"]
mod declarations;
#[path = "strict/definitions.rs"]
mod definitions;
#[path = "strict/lists.rs"]
mod lists;
#[path = "strict/values.rs"]
mod values;
#[test]
fn subdivision_membership_preserves_groups_without_requiring_or_inferring_dxcc() {
    use qso_io::adif::primary_subdivisions::{canada, united_states, venezuela};

    assert_eq!(
        "CA".parse::<united_states::PrimarySubdivision>()
            .expect("CA should identify California within the US group"),
        united_states::PrimarySubdivision::California
    );
    assert_eq!(
        "ca".parse::<venezuela::PrimarySubdivision>()
            .expect("CA should identify Carabobo within the Venezuela group"),
        venezuela::PrimarySubdivision::Carabobo
    );
    assert!(
        "CA".parse::<canada::PrimarySubdivision>().is_err(),
        "a shared code must not become a member of every country's enum"
    );

    for bytes in [
        &b"<STATE:2>ca<EOR>"[..],
        &b"<MY_STATE:2>ca<EOR>"[..],
        &b"<DXCC:1>1<STATE:2>ca<EOR>"[..],
        &b"<MY_DXCC:1>1<MY_STATE:2>ca<EOR>"[..],
    ] {
        let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
        let mut parser = parser_owner.into_iter();
        let parsed = record(next_section(&mut parser));
        let subdivision = parsed
            .field("STATE")
            .expect("strict parser should yield unique descriptor names")
            .or_else(|| {
                parsed
                    .field("MY_STATE")
                    .expect("strict parser should yield unique descriptor names")
            })
            .unwrap_or_else(|| panic!("expected a subdivision field in {parsed:?}"));
        assert_eq!(subdivision.text(), "ca", "input: {bytes:?}");
        assert!(
            matches!(subdivision.value(), Some(qso_io::adif::value::Value::Text(text)) if text == "ca"),
            "membership must retain text without selecting a country's variant: {subdivision:?}"
        );
        assert_eq!(
            parsed
                .field("DXCC")
                .expect("strict parser should yield unique descriptor names")
                .is_some()
                || parsed
                    .field("MY_DXCC")
                    .expect("strict parser should yield unique descriptor names")
                    .is_some(),
            bytes.starts_with(b"<DXCC") || bytes.starts_with(b"<MY_DXCC"),
            "parsing must not infer a DXCC field: {parsed:?}"
        );
        assert_exhausted(&mut parser);
    }

    let mut parser_owner = StrictParser::new(b"<STATE:3>???<EOR>", ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    let error = next_error(&mut parser);
    assert!(
        matches!(
            &error,
            StructuralError::InvalidValue {
                enumeration: Some("Primary_Administrative_Subdivision"),
                ..
            }
        ),
        "unknown codes must fail membership: {error:?}"
    );
    assert_eq!(error.span(), 9..12, "{error:?}");
    assert_exhausted(&mut parser);
}

#[test]
fn reference_types_and_parser_share_standard_membership() {
    use qso_io::adif::{AntPath, ArrlSection, Continent, QslMedium};

    assert_eq!(
        "g".parse::<AntPath>().expect("G should name grayline"),
        AntPath::G
    );
    assert_eq!(
        "eu".parse::<Continent>().expect("EU should name Europe"),
        Continent::Europe
    );
    assert_eq!(
        "lotw"
            .parse::<QslMedium>()
            .expect("LOTW should name Logbook of the World"),
        QslMedium::Lotw
    );
    let historical = "gta"
        .parse::<ArrlSection>()
        .expect("GTA should retain its historical identity");
    assert_eq!(historical.name(), "Greater Toronto Area");
    assert_eq!(
        historical
            .deleted_date()
            .map(|date| date.to_string())
            .as_deref(),
        Some("2023-01-01")
    );

    let bytes = b"<ANT_PATH:1>g<ARRL_SECT:3>gta<CONT:2>eu<CREDIT_GRANTED:8>WAS:lotw<EOR>";
    let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    let section = record(next_section(&mut parser));
    assert_eq!(
        named(&section, "ANT_PATH")
            .text()
            .parse::<AntPath>()
            .expect("accepted ANT_PATH should have a reference value"),
        AntPath::G
    );
    assert_eq!(
        named(&section, "ARRL_SECT")
            .text()
            .parse::<ArrlSection>()
            .expect("accepted section should have a reference value"),
        historical
    );
    assert_exhausted(&mut parser);

    assert!(
        " EU".parse::<Continent>().is_err(),
        "reference parsing must not trim values"
    );
    let mut parser_owner = StrictParser::new(b"<CONT:3> EU<EOR>", ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    let error = next_error(&mut parser);
    assert!(
        matches!(
            &error,
            StructuralError::InvalidValue {
                enumeration: Some("Continent"),
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn alternative_enumeration_labels_outlive_the_error() {
    let mut parser_owner =
        StrictParser::new(b"<CREDIT_GRANTED:8>AJA,NOPE<EOR>", ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    let labels: Vec<Option<&'static str>> = {
        let error = next_error(&mut parser);
        let StructuralError::NoMatchingType { failures, .. } = &error else {
            panic!("expected alternative failures, got {error:?}");
        };
        failures
            .iter()
            .map(|failure| failure.enumeration())
            .collect()
    };

    assert_eq!(labels, [Some("Credit"), Some("Award")]);
    assert_exhausted(&mut parser);
}

#[test]
fn identifier_membership_errors_retain_the_expected_enumeration() {
    let bytes = b"<IOTA:6>ZZ-001<EOR>";
    let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
    let mut parser = parser_owner.into_iter();

    let error = next_error(&mut parser);
    assert!(
        matches!(
            &error,
            StructuralError::InvalidValue {
                field_name,
                cause: qso_io::adif::value::ValueError::InvalidIOTARefNo,
                enumeration: Some("Continent"), .. } if field_name == "IOTA"
        ),
        "IOTA continent membership should identify its expected enumeration: {error:?}"
    );
    assert_eq!(error.span(), 8..10, "{error:?}");
    assert_eq!(&bytes[error.span()], b"ZZ");
    assert_exhausted(&mut parser);
}

#[test]
fn options_and_read_only_values_are_usable_by_a_consumer() {
    for (encoding, field_length, bytes, value) in [
        (
            ParseEncoding::Ascii,
            LengthUnit::Bytes,
            &b"<nAmE:3:S>Ray<EOR>"[..],
            "Ray",
        ),
        (
            ParseEncoding::Ascii,
            LengthUnit::UnicodeScalars,
            &b"<nAmE:3:S>Ray<EOR>"[..],
            "Ray",
        ),
        (
            ParseEncoding::Latin1,
            LengthUnit::Bytes,
            &b"<nAmE:4:S>Jos\xe9<EOR>"[..],
            "José",
        ),
        (
            ParseEncoding::Latin1,
            LengthUnit::UnicodeScalars,
            &b"<nAmE:4:S>Jos\xe9<EOR>"[..],
            "José",
        ),
        (
            ParseEncoding::Utf8,
            LengthUnit::Bytes,
            "<nAmE:5:S>José<EOR>".as_bytes(),
            "José",
        ),
        (
            ParseEncoding::Utf8,
            LengthUnit::UnicodeScalars,
            "<nAmE:4:S>José<EOR>".as_bytes(),
            "José",
        ),
    ] {
        let mut parser_owner = StrictParser::new(
            bytes,
            ParseOpts {
                encoding,
                field_length,
            },
        );
        let mut parser = parser_owner.into_iter();
        let record = record(next_section(&mut parser));
        let field = named(&record, "NAME");
        assert_eq!(
            field.name(),
            "nAmE",
            "{encoding:?}, {field_length:?}, input {bytes:?}"
        );
        assert_eq!(
            field.text(),
            value,
            "{encoding:?}, {field_length:?}, input {bytes:?}"
        );
        assert_eq!(
            field.data_type(),
            Some(qso_io::adif::DataType::String),
            "{encoding:?}, {field_length:?}, input {bytes:?}"
        );
        assert_eq!(
            field.declared_length(),
            if field_length == LengthUnit::Bytes {
                field.raw_data().len()
            } else {
                value.chars().count()
            },
            "{encoding:?}, {field_length:?}, input {bytes:?}"
        );
        assert_eq!(
            &bytes[field.value_span()],
            field.raw_data(),
            "{encoding:?}, {field_length:?}, input {bytes:?}"
        );
        assert_exhausted(&mut parser);
    }
}

#[test]
fn sections_have_syntactic_spans() {
    let bytes = b"P<PROGRAMID:0><EOH>\r\n<NAME:1>A<EOR>\nignore<NAME:0><EOR>\r\n";
    let mut owner = StrictParser::new(bytes, ParseOpts::default());
    let sections: Vec<_> = owner
        .into_iter()
        .collect::<Result<_, _>>()
        .unwrap_or_else(|error| {
            panic!("input {bytes:?}: expected complete sections, got {error:?}")
        });
    assert_eq!(sections.len(), 3);
    let Section::Header(header) = &sections[0] else {
        panic!("expected Section::Header(header), got {:?}", sections[0]);
    };
    assert_eq!(header.span(), 0..19);
    assert_eq!(header.text_span(), 0..1);
    assert_eq!(header.raw_text(), b"P");
    assert_eq!(header.text(), "P");
    assert_eq!(header.entries().len(), 1);
    assert_eq!(
        header
            .field("programid")
            .expect("strict parser should yield unique descriptor names")
            .expect("header should contain PROGRAMID from the supplied descriptor")
            .value_span(),
        14..14
    );
    for (section, expected) in sections.into_iter().skip(1).zip([21..35, 42..55]) {
        let record = record(section);
        assert_eq!(record.span(), expected);
        let field = named(&record, "name");
        assert_eq!(&bytes[field.value_span()], field.raw_data());
        assert!(field.descriptor_span().end <= field.value_span().start);
    }
}

#[test]
fn a_valid_prefix_is_yielded_before_one_terminal_error() {
    let bytes = b"P<EOH><NAME:1>A<EOR><NAME:1>B<EOR><NAME:3>Ra";
    let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    let section = next_section(&mut parser);
    assert!(
        matches!(section, Section::Header(_)),
        "expected header, got {section:?}"
    );
    assert_eq!(
        record(next_section(&mut parser))
            .field("NAME")
            .expect("strict parser should yield unique descriptor names")
            .expect("section should contain NAME from the supplied descriptor")
            .text(),
        "A"
    );
    assert_eq!(
        record(next_section(&mut parser))
            .field("NAME")
            .expect("strict parser should yield unique descriptor names")
            .expect("section should contain NAME from the supplied descriptor")
            .text(),
        "B"
    );
    let error = next_error(&mut parser);
    assert_eq!(error.span(), bytes.len()..bytes.len());
    assert!(
        matches!(&error, StructuralError::TruncatedValue { field_name, declared: 3, available: 2, unit: LengthUnit::Bytes, .. } if field_name == "NAME"),
        "unexpected value: {:?}",
        error
    );
    for _ in 0..3 {
        assert_exhausted(&mut parser);
    }
}

#[test]
fn duplicate_errors_keep_original_locations_and_do_not_yield_the_bad_section() {
    for bytes in [
        &b"<CALL:1>A<EOR><NAME:1>B<name:1>B<EOR>"[..],
        &b"P<NAME:1>B<name:1>B<EOH>"[..],
    ] {
        let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
        let mut parser = parser_owner.into_iter();
        if bytes.starts_with(b"<CALL") {
            let _ = next_section(&mut parser);
        }
        let error = next_error(&mut parser);
        let StructuralError::DuplicateField {
            first_span,
            second_span,
            field_name,
            ..
        } = &error
        else {
            panic!("{error:?}");
        };
        assert!(field_name.eq_ignore_ascii_case("NAME"));
        assert_eq!(&bytes[first_span.clone()], b"<NAME:1>");
        assert_eq!(&bytes[second_span.clone()], b"<name:1>");
        assert_eq!(error.span(), *second_span);
        assert_exhausted(&mut parser);
    }
}

#[test]
fn clean_exhaustion_is_fused_and_accepts_ignored_separators() {
    fn requires_fused(_: impl std::iter::FusedIterator) {}
    requires_fused(StrictParser::new(b"", ParseOpts::default()).into_iter());
    for bytes in [
        &b""[..],
        &b"<NAME:1>A<EOR>\r\n\n"[..],
        &b"<NAME:1>A<EOR> trailing text"[..],
    ] {
        let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
        let mut parser = parser_owner.into_iter();
        let records: Vec<_> = parser
            .by_ref()
            .map(|section| {
                record(section.unwrap_or_else(|error| panic!("input {bytes:?}: {error}")))
            })
            .collect();
        assert_eq!(
            records.len(),
            usize::from(!bytes.is_empty()),
            "input {bytes:?}: {records:?}"
        );
        if let Some(record) = records.first() {
            assert_eq!(
                record
                    .field("NAME")
                    .expect("strict parser should yield unique descriptor names")
                    .expect("record should retain NAME from the supplied <NAME:1>A field")
                    .text(),
                "A",
                "input {bytes:?}"
            );
        }
        assert_exhausted(&mut parser);
        assert_exhausted(&mut parser_owner.into_iter());
    }
}

#[test]
fn decoding_failures_are_locatable_and_terminal() {
    for (bytes, expected) in [
        (&b"P\xff<EOH><NAME:1>C<EOR>"[..], 1..2),
        (&b"<NAME:3>A\xffB<EOR><NAME:1>C<EOR>"[..], 9..10),
    ] {
        let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
        let mut parser = parser_owner.into_iter();
        let error = next_error(&mut parser);
        assert_eq!(error.span(), expected);
        assert!(
            matches!(
                &error,
                StructuralError::Decoding {
                    cause: DecodeError::NonAscii { .. },
                    ..
                }
            ),
            "unexpected value: {:?}",
            error
        );
        assert_exhausted(&mut parser);
    }
}

#[test]
fn scalar_utf8_errors_retain_causes_and_do_not_search_for_recovery() {
    use std::error::Error;
    for (bytes, expected_span, error_len) in [
        (&b"<NAME:1>\xff<EOR><CALL:1>A<EOR>"[..], 8..9, Some(1)),
        (&b"<NAME:1>\xe2A"[..], 8..9, Some(1)),
        (&b"<NAME:1>\xe2\x82"[..], 8..10, None),
    ] {
        let mut parser_owner = StrictParser::new(
            bytes,
            ParseOpts {
                encoding: ParseEncoding::Utf8,
                field_length: LengthUnit::UnicodeScalars,
            },
        );
        let mut parser = parser_owner.into_iter();
        let error = next_error(&mut parser);
        assert_eq!(error.span(), expected_span);
        let StructuralError::Decoding {
            cause: DecodeError::Utf8(cause),
            ..
        } = &error
        else {
            panic!("{error:?}");
        };
        assert_eq!(cause.error_len(), error_len);
        let decoding = error
            .source()
            .unwrap_or_else(|| panic!("expected decoding cause for {bytes:?}: {error:?}"));
        let utf8 = decoding
            .source()
            .unwrap_or_else(|| panic!("expected UTF-8 source for {bytes:?}: {decoding:?}"));
        assert!(
            utf8.downcast_ref::<std::str::Utf8Error>().is_some(),
            "expected UTF-8 source for {bytes:?}, got {utf8:?}"
        );
        assert_exhausted(&mut parser);
    }
}

#[test]
fn public_syntax_errors_distinguish_missing_ends_and_bad_descriptors() {
    for (bytes, reason) in [
        (
            &b"P<NAME:1>A"[..],
            (|actual: &SyntaxErrorKind| matches!(actual, SyntaxErrorKind::MissingEoh))
                as fn(&SyntaxErrorKind) -> bool,
        ),
        (
            &b"<NAME:1>A"[..],
            (|actual: &SyntaxErrorKind| matches!(actual, SyntaxErrorKind::MissingEor))
                as fn(&SyntaxErrorKind) -> bool,
        ),
        (
            &b"<NAME:x>A<EOR>"[..],
            (|actual: &SyntaxErrorKind| matches!(actual, SyntaxErrorKind::InvalidLength))
                as fn(&SyntaxErrorKind) -> bool,
        ),
        (
            &b"<NAME:1:Z>A<EOR>"[..],
            (|actual: &SyntaxErrorKind| {
                matches!(actual, SyntaxErrorKind::UnknownTypeIndicator { .. })
            }) as fn(&SyntaxErrorKind) -> bool,
        ),
        (
            &b"<NAME:1"[..],
            (|actual: &SyntaxErrorKind| matches!(actual, SyntaxErrorKind::MalformedDescriptor))
                as fn(&SyntaxErrorKind) -> bool,
        ),
    ] {
        let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
        let mut parser = parser_owner.into_iter();
        let error = next_error(&mut parser);
        assert!(
            matches!(&error, StructuralError::Syntax { reason: actual, .. } if reason(actual)),
            "input {bytes:?}: expected {reason:?}, got {error:?}"
        );
        assert_exhausted(&mut parser);
    }
}

#[test]
fn headers_distinguish_definitions_from_fields() {
    let bytes = b"P<PROGRAMID:1>A<USERDEF1:3:N>EPC<COMMENT:1>B<USERDEF2:19:E>SweaterSize,{S,M,L}<USERDEF3:15>ShoeSize,{5:20}<EOH><EPC:3>123<EOR>";
    let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    let header = match next_section(&mut parser) {
        Section::Header(header) => header,
        other => panic!("expected header, got {other:?}"),
    };
    assert_eq!(header.entries().len(), 5);
    let epc = definition(&header.entries()[1]);
    assert_eq!(epc.field_name(), "EPC");
    assert_eq!(epc.data_type(), Some(qso_io::adif::DataType::Number));
    assert_eq!(epc.text(), "EPC");
    assert_eq!(epc.constraint(), None, "declaration {epc:?}");
    assert!(
        header
            .field("USERDEF1")
            .expect("strict parser should yield unique descriptor names")
            .is_none(),
        "expected a declaration rather than an ordinary field, got {:?}",
        header
            .field("USERDEF1")
            .expect("strict parser should yield unique descriptor names")
    );
    assert!(
        header
            .entry("userdef1")
            .expect("strict parser should yield unique descriptor names")
            .is_some(),
        "expected USERDEF1 entry, got {header:?}"
    );
    let sizes = definition(&header.entries()[3]);
    assert_eq!(
        sizes.constraint(),
        Some(
            &qso_io::adif::adi::parser::strict::declarations::UserDefinitionConstraint::Enumeration(
                ["S", "M", "L"].map(str::to_owned).to_vec()
            )
        )
    );
    let range = definition(&header.entries()[4]);
    assert_eq!(range.data_type(), None);
    assert!(
        matches!(range.constraint(), Some(qso_io::adif::adi::parser::strict::declarations::UserDefinitionConstraint::Range {lower,upper}) if lower == &bigdecimal::BigDecimal::from(5) && upper == &bigdecimal::BigDecimal::from(20)),
        "unexpected value: {:?}",
        range.constraint()
    );
    for entry in header.entries() {
        if let qso_io::adif::adi::parser::strict::header::HeaderEntry::UserDefinition(def) = entry {
            assert_eq!(&bytes[def.value_span()], def.raw_data());
            assert!(bytes[def.descriptor_span()].starts_with(b"<USERDEF"));
        }
    }
    assert_eq!(
        record(next_section(&mut parser))
            .field("EPC")
            .expect("strict parser should yield unique descriptor names")
            .expect("record should contain EPC from the supplied <EPC:3>123 field")
            .text(),
        "123"
    );
}

#[test]
fn values_preserve_absence_empty_contents_and_source_spelling() {
    use qso_io::adif::DataType;
    use qso_io::adif::value::Value;
    let bytes = b"<A:0><B:0:N><C:3>123<D:3:S>123<E:8:N>001.2300<EOR>";
    let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    let rec = record(next_section(&mut parser));
    assert!(
        rec.field("X")
            .expect("strict parser should yield unique descriptor names")
            .is_none(),
        "expected X to be absent, got {:?}",
        rec.field("X")
            .expect("strict parser should yield unique descriptor names")
    );
    for name in ["A", "B"] {
        let f = named(&rec, name);
        assert_eq!(f.value(), None, "expected empty field {name:?}, got {f:?}");
        assert_eq!(f.text(), "");
    }
    assert_eq!(named(&rec, "A").data_type(), None);
    assert_eq!(named(&rec, "B").data_type(), Some(DataType::Number));
    for name in ["C", "D"] {
        assert!(
            matches!(named(&rec, name).value(),Some(Value::Text(s)) if s=="123"),
            "unexpected value: {:?}",
            named(&rec, name).value()
        );
    }
    let number = named(&rec, "E");
    assert!(
        matches!(number.value(),Some(Value::Number(n)) if n == &"1.2300".parse::<bigdecimal::BigDecimal>().expect("expected decimal should parse because its fixed test spelling uses decimal notation")),
        "unexpected value: {:?}",
        number.value()
    );
    assert_eq!(number.text(), "001.2300");
    assert_eq!(number.raw_data(), b"001.2300");
    for f in rec.fields() {
        assert_eq!(&bytes[f.value_span()], f.raw_data());
    }
}

#[test]
fn composite_record_values_outlive_the_parser() {
    use qso_io::adif::value::{Hemisphere, TimePrecision, Value};
    let bytes=b"P<USERDEF1:3:N>EPC<EOH><A:4:S>Jos\xe9<B:1:B>y<C:2:N>1.<D:8:D>20240229<E:4:T>1305<F:11:L>W122 30.125<EOR><E:6:T>130500<EPC:3>123<EOR>";
    let records = {
        let mut parser = StrictParser::new(
            bytes,
            ParseOpts {
                encoding: ParseEncoding::Latin1,
                field_length: LengthUnit::Bytes,
            },
        );
        let sections = parser
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_else(|error| {
                panic!("input {bytes:?}: expected complete sections, got {error:?}")
            });
        assert_eq!(sections.len(), 3);
        let Section::Header(header) = &sections[0] else {
            panic!("expected Section::Header(header), got {:?}", sections[0]);
        };
        let def = definition(&header.entries()[0]);
        assert_eq!(def.field_name(), "EPC");
        assert_eq!(&bytes[def.value_span()], def.raw_data());
        sections
            .into_iter()
            .filter_map(|section| match section {
                Section::Record(record) => Some(record),
                Section::Header(_) => None,
            })
            .collect::<Vec<_>>()
    };
    let first = &records[0];
    assert_eq!(first.fields().len(), 6);
    for (field, expected) in first
        .fields()
        .iter()
        .zip(["text", "boolean", "number", "date", "time", "location"])
    {
        let kind = match field
            .value()
            .unwrap_or_else(|| panic!("expected a nonempty value, got {field:?}"))
        {
            Value::Text(s) => {
                assert_eq!(s, "José");
                "text"
            }
            Value::Boolean(b) => {
                assert!(*b);
                "boolean"
            }
            Value::Integer(_) => "integer",
            Value::List(_) => "list",
            Value::Number(n) => {
                assert_eq!(n, &bigdecimal::BigDecimal::from(1));
                "number"
            }
            Value::Date(d) => {
                assert_eq!(d.to_string(), "2024-02-29");
                "date"
            }
            Value::Time(t) => {
                assert_eq!(t.precision(), TimePrecision::Minute);
                assert_eq!(t.time().to_string(), "13:05:00");
                "time"
            }
            Value::Location(l) => {
                assert_eq!(l.hemisphere(), Hemisphere::West);
                assert_eq!(l.degrees(), 122);
                assert_eq!(
                    l.minutes(),
                    &"30.125"
                        .parse::<bigdecimal::BigDecimal>()
                        .expect("expected decimal should parse because its fixed test spelling uses decimal notation")
                );
                "location"
            }
        };
        assert_eq!(kind, expected);
        assert_eq!(&bytes[field.value_span()], field.raw_data());
    }
    assert_eq!(named(first, "C").text(), "1.");
    let second = &records[1];
    let Some(Value::Time(t)) = named(second, "E").value() else {
        panic!(
            "expected Some(Value::Time(t)), got {:?}",
            named(second, "E").value()
        );
    };
    assert_eq!(t.precision(), TimePrecision::Second);
    assert_eq!(t.time().to_string(), "13:05:00");
    assert!(
        matches!(named(second, "EPC").value(),Some(Value::Number(n)) if n == &bigdecimal::BigDecimal::from(123)),
        "unexpected value: {:?}",
        named(second, "EPC").value()
    );
    assert_eq!(named(second, "EPC").text(), "123");
    assert_eq!(named(second, "EPC").type_indicator(), None);
}

#[test]
fn latin1_declaration_error_uses_original_bytes_after_an_accent() {
    use qso_io::adif::adi::parser::strict::declarations::UserDefinitionError;
    let bytes = b"P<PROGRAMID:1>A<USERDEF1:8:E>X\xe9,{A,\x85}<EOH>";
    let mut parser_owner = StrictParser::new(
        bytes,
        ParseOpts {
            encoding: ParseEncoding::Latin1,
            field_length: LengthUnit::Bytes,
        },
    );
    let mut parser = parser_owner.into_iter();
    let error = next_error(&mut parser);
    assert_eq!(error.span(), 35..36);
    assert!(
        matches!(
            &error,
            StructuralError::InvalidUserDefinition {
                cause: UserDefinitionError::InvalidEnumeration,
                ..
            }
        ),
        "unexpected value: {:?}",
        error
    );
    assert_exhausted(&mut parser);
}

#[test]
fn inline_indicator_is_distinct_from_value_presence() {
    use qso_io::adif::DataType;
    use qso_io::adif::value::Value;

    let mut parser_owner = StrictParser::new(b"<X:0:N><EOR><X:3>123<EOR>", ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    let empty = record(next_section(&mut parser));
    let field = named(&empty, "X");
    assert_eq!(field.data_type(), Some(DataType::Number));
    assert_eq!(field.type_indicator(), Some(DataType::Number));
    assert!(field.value().is_none(), "empty field: {field:?}");
    assert_eq!(field.text(), "");
    assert_eq!(field.raw_data(), b"");

    let untyped = record(next_section(&mut parser));
    let field = named(&untyped, "X");
    assert_eq!(field.data_type(), None);
    assert_eq!(field.type_indicator(), None);
    assert!(
        matches!(field.value(), Some(Value::Text(text)) if text == "123"),
        "field: {field:?}"
    );
}

#[test]
fn document_rules_and_alternative_errors_are_available_to_consumers() {
    use qso_io::adif::DataType;
    use qso_io::adif::value::Value;

    let bytes = b"P<USERDEF1:5:N>SCORE<EOH><SCORE:3>123<APP_EXAMPLE_SCORE:1:N>1<EOR><APP_EXAMPLE_SCORE:1>2<EOR><CREDIT_GRANTED:8>AJA,NOPE<EOR>";
    let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    drop(next_section(&mut parser));
    let first = record(next_section(&mut parser));
    assert_eq!(named(&first, "SCORE").data_type(), Some(DataType::Number));
    assert_eq!(named(&first, "SCORE").type_indicator(), None);
    let second = record(next_section(&mut parser));
    assert!(
        matches!(named(&second, "APP_EXAMPLE_SCORE").value(), Some(Value::Number(n)) if n == &bigdecimal::BigDecimal::from(2)),
        "{second:?}"
    );
    assert_eq!(named(&second, "APP_EXAMPLE_SCORE").type_indicator(), None);
    let error = next_error(&mut parser);
    let StructuralError::NoMatchingType { failures, .. } = &error else {
        panic!("expected both alternatives, got {error:?}");
    };
    assert_eq!(failures[0].enumeration(), Some("Credit"));
    assert_eq!(failures[1].enumeration(), Some("Award"));
    assert_eq!(&bytes[error.span()], b"AJA");
    assert_eq!(&bytes[failures[1].span()], b"NOPE");
    assert_exhausted(&mut parser);
    assert_eq!(named(&first, "SCORE").text(), "123");
}

#[test]
fn secondary_candidates_dispatch_prefixes_and_preserve_overlaps() {
    use qso_io::adif::{
        AlaskaCounty, SecondarySubdivision as S, primary_subdivisions::united_states,
    };

    let alaska: Vec<_> = S::candidates("aK,Aleutians East").collect();
    assert_eq!(alaska, [S::Alaska(AlaskaCounty::AleutiansEast)]);
    assert_eq!(AlaskaCounty::AleutiansEast.as_str(), "Aleutians East");
    for code in ["AK,Imaginary", "AK,", "HI,", "ZZ,County", " AK,Anchorage"] {
        let candidates: Vec<_> = S::candidates(code).collect();
        assert!(
            candidates.is_empty(),
            "invalid {code:?} produced {candidates:?}"
        );
    }
    assert_eq!(
        S::candidates("hI,Hawaii").collect::<Vec<_>>(),
        [S::Hawaii("Hawaii".into())]
    );
    assert_eq!(
        S::candidates("ma,Franklin").collect::<Vec<_>>(),
        [S::UnitedStates {
            state: united_states::PrimarySubdivision::Massachusetts,
            county: "Franklin".into(),
        }]
    );

    let numeric: Vec<_> = S::candidates("01001").collect();
    assert!(
        numeric
            .iter()
            .any(|v| matches!(v, S::Japan(code) if code == "01001")),
        "Japanese candidate missing: {numeric:?}"
    );
    assert!(
        numeric
            .iter()
            .any(|v| matches!(v, S::Spain(code) if code == "01001")),
        "Spanish candidate missing: {numeric:?}"
    );
    assert!(
        !numeric.iter().any(|v| matches!(v, S::NewZealand(_))),
        "five digits must not match NZ's three-digit format: {numeric:?}"
    );
    let district: Vec<_> = S::candidates("AB-01").collect();
    assert!(
        district
            .iter()
            .any(|v| matches!(v, S::AsiaticRussia(code) if code == "AB-01")),
        "Russian candidate missing: {district:?}"
    );
    assert!(
        district
            .iter()
            .any(|v| matches!(v, S::Ukraine(code) if code == "AB-01")),
        "Ukrainian candidate missing: {district:?}"
    );
}

#[test]
fn alternate_subdivisions_dispatch_to_suffix_only_members() {
    use qso_io::adif::{NzRegionDistrict, SecondarySubdivisionAlt as S};

    assert_eq!(
        S::parse_member("nz_regions:Hawkes Bay/Wairoa"),
        Some(S::NzRegions(NzRegionDistrict::HawkesBayWairoa))
    );
    assert_eq!(
        NzRegionDistrict::HawkesBayWairoa.as_str(),
        "Hawkes Bay/Wairoa"
    );
    for member in [
        "NZ_Regions:Imaginary",
        "NZ_Regions:",
        ":Unlisted",
        "OTHER:",
        "OTHER",
    ] {
        assert_eq!(
            S::parse_member(member),
            None,
            "invalid alternate member {member:?}"
        );
    }
    assert_eq!(
        S::parse_member("Other:Unlisted"),
        Some(S::Other {
            enumeration: "Other".into(),
            code: "Unlisted".into(),
        })
    );
}

#[test]
fn sponsor_components_compose_with_award_lists_and_preserve_diagnostics() {
    use qso_io::adif::AwardSponsor;
    use qso_io::adif::value::{Value, ValueError};

    let value = format!(
        "{}_PROGRAM_AWARD,cq_OTHER_UNKNOWN",
        AwardSponsor::Arrl.as_str()
    );
    let bytes = format!("<AWARD_GRANTED:{}>{value}<EOR>", value.len());
    let mut parser_owner = StrictParser::new(bytes.as_bytes(), ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    let parsed = record(next_section(&mut parser));
    let award = named(&parsed, "AWARD_GRANTED");
    assert_eq!(award.text(), value);
    assert!(
        matches!(award.value(), Some(Value::List(members)) if members == &["ARRL_PROGRAM_AWARD", "cq_OTHER_UNKNOWN"]),
        "{award:?}"
    );
    assert_exhausted(&mut parser);

    let value = "arrl_PROGRAM_AWARD,NOPE_PROGRAM_AWARD";
    let descriptor = format!("<AWARD_GRANTED:{}>", value.len());
    let bytes = format!("{descriptor}{value}<EOR><CALL:1>X<EOR>");
    let mut parser_owner = StrictParser::new(bytes.as_bytes(), ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    let error = next_error(&mut parser);
    assert!(
        matches!(&error, StructuralError::InvalidValue {
        field_name, cause: ValueError::InvalidSponsoredAwardList,
        enumeration: Some("Award_Sponsor"), ..
    } if field_name == "AWARD_GRANTED"),
        "{error:?}"
    );
    let start = descriptor.len() + "arrl_PROGRAM_AWARD,".len();
    assert_eq!(error.span(), start..start + "NOPE_".len(), "{error}");
    assert_eq!(&bytes.as_bytes()[error.span()], b"NOPE_");
    assert_exhausted(&mut parser);
}

#[test]
fn strict_indicator_diagnostics_preserve_grammar_and_error_precedence() {
    use std::error::Error;

    for bytes in [
        b"<X:0:N:extra><EOR>".as_slice(),
        b"<X:0:N1><EOR>".as_slice(),
    ] {
        let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
        let mut parser = parser_owner.into_iter();
        let error = next_error(&mut parser);
        assert_eq!(error.span(), 6..7, "input {bytes:?}: {error:?}");
        assert!(
            matches!(
                &error,
                StructuralError::Syntax {
                    reason: SyntaxErrorKind::MalformedDescriptor,
                    ..
                }
            ),
            "{error:?}"
        );
        assert!(
            error.source().is_none(),
            "malformed suffix should not become an unknown-type cause: {error:?}"
        );
        assert_exhausted(&mut parser);
    }
    let mut parser_owner = StrictParser::new(b"<X:0:Z", ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    let error = next_error(&mut parser);
    assert_eq!(error.span(), 5..6);
    assert!(
        matches!(
            &error,
            StructuralError::Syntax {
                reason: SyntaxErrorKind::UnknownTypeIndicator { .. },
                ..
            }
        ),
        "{error:?}"
    );
    assert!(
        matches!(error.source().and_then(|e| e.downcast_ref::<qso_io::adif::DataTypeError>()),
        Some(qso_io::adif::DataTypeError { supplied: code }) if code == "Z")
    );
    assert_exhausted(&mut parser);
}

#[test]
fn invalid_descriptor_names_retain_their_utf8_error_source() {
    use std::error::Error;

    let mut parser_owner = StrictParser::new(b"<\xff:0><EOR>", ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    let error = next_error(&mut parser);
    assert_eq!(error.span(), 1..2);
    let cause = error
        .source()
        .and_then(|e| e.downcast_ref::<std::str::Utf8Error>())
        .expect("invalid UTF-8 name should retain its original Utf8Error");
    assert_eq!(cause.valid_up_to(), 0);
}

#[test]
fn invalid_declared_field_values_are_structural_errors() {
    use qso_io::adif::value::ValueError;
    for (bytes, reason, span) in [
        (
            &b"<QSO_DATE:8:D>20260230"[..],
            ValueError::InvalidDate,
            14..22,
        ),
        (
            &b"<APP_TEST_VALUE:3:N>abc"[..],
            ValueError::InvalidNumber,
            20..21,
        ),
    ] {
        let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
        let mut parser = parser_owner.into_iter();
        let error = next_error(&mut parser);
        assert_eq!(error.span(), span);
        assert!(
            matches!(&error,error::StructuralError::InvalidValue {cause,..} if *cause == reason),
            "unexpected value: {:?}",
            error
        );
    }
}

#[test]
fn decoding_failures_stop_before_following_fields() {
    for (encoding, bytes, incomplete) in [
        (
            ParseEncoding::Ascii,
            &b"<NAME:2>\xc3\xa9<NAME:3>Ray"[..],
            false,
        ),
        (ParseEncoding::Ascii, &b"<NAME:1>\xff<NAME:3>Ray"[..], false),
        (ParseEncoding::Utf8, &b"<NAME:1>\xff<NAME:3>Ray"[..], false),
        (ParseEncoding::Utf8, &b"<NAME:1>\xc3<NAME:3>Ray"[..], true),
    ] {
        let mut parser_owner = StrictParser::new(
            bytes,
            ParseOpts {
                encoding,
                field_length: LengthUnit::Bytes,
            },
        );
        let mut parser = parser_owner.into_iter();
        let error = next_error(&mut parser);
        assert_eq!(error.span(), 8..9);
        match (encoding, &error) {
            (
                ParseEncoding::Ascii,
                error::StructuralError::Decoding {
                    cause: DecodeError::NonAscii { offset },
                    ..
                },
            ) => assert_eq!(*offset, 0),
            (
                ParseEncoding::Utf8,
                error::StructuralError::Decoding {
                    cause: DecodeError::Utf8(cause),
                    ..
                },
            ) => {
                assert_eq!(cause.valid_up_to(), 0);
                assert_eq!(cause.error_len(), if incomplete { None } else { Some(1) });
            }
            other => panic!("{other:?}"),
        }
        assert_exhausted(&mut parser);
    }
}

#[test]
fn non_ascii_errors_locate_the_first_offending_byte_in_the_original_document() {
    for unit in [LengthUnit::Bytes, LengthUnit::UnicodeScalars] {
        for (raw, expected_offset, document_offset) in [
            (&b"\x80AB\xff"[..], 0, 19),
            (&b"A\x80B\xff"[..], 1, 20),
            (&b"ABC\xff"[..], 3, 22),
            (&b"A\xc3\xa9!"[..], 1, 20),
        ] {
            let mut bytes = b"<CALL:3>Ray<NAME:4>".to_vec();
            bytes.extend_from_slice(raw);
            bytes.extend_from_slice(b"<QTH:3>Ann<EOR>");
            let mut parser_owner = StrictParser::new(
                &bytes,
                ParseOpts {
                    encoding: ParseEncoding::Ascii,
                    field_length: unit,
                },
            );
            let mut parser = parser_owner.into_iter();
            let error = next_error(&mut parser);
            assert_eq!(error.span(), document_offset..document_offset + 1);
            assert!(
                matches!(&error,error::StructuralError::Decoding {
                cause:DecodeError::NonAscii { offset }, .. } if *offset == expected_offset),
                "unexpected value: {:?}",
                error
            );
            assert_exhausted(&mut parser);
        }
    }
}

#[test]
fn byte_lengths_splitting_a_utf8_scalar_fail_at_the_bounded_sequence() {
    let mut parser_owner = StrictParser::new(
        "<NAME:4>José<EOR>".as_bytes(),
        ParseOpts {
            encoding: ParseEncoding::Utf8,
            field_length: LengthUnit::Bytes,
        },
    );
    let mut parser = parser_owner.into_iter();
    let error = next_error(&mut parser);
    assert_eq!(error.span(), 11..12);
    let error::StructuralError::Decoding {
        cause: DecodeError::Utf8(cause),
        ..
    } = &error
    else {
        panic!("{error:?}")
    };
    assert_eq!(cause.valid_up_to(), 3);
    assert_eq!(cause.error_len(), None);
    assert_exhausted(&mut parser);
}

#[test]
fn header_decoding_errors_are_terminal_with_relative_causes() {
    for encoding in [ParseEncoding::Ascii, ParseEncoding::Utf8] {
        let mut parser_owner = StrictParser::new(
            b"Generated \xff\n<EOH><NAME:3>Ray<EOR>",
            ParseOpts {
                encoding,
                field_length: LengthUnit::Bytes,
            },
        );
        let mut parser = parser_owner.into_iter();
        let error = next_error(&mut parser);
        assert_eq!(error.span(), 10..11);
        match (encoding, &error) {
            (
                ParseEncoding::Ascii,
                error::StructuralError::Decoding {
                    cause: DecodeError::NonAscii { offset },
                    ..
                },
            ) => assert_eq!(*offset, 10),
            (
                ParseEncoding::Utf8,
                error::StructuralError::Decoding {
                    cause: DecodeError::Utf8(cause),
                    ..
                },
            ) => assert_eq!(cause.valid_up_to(), 10),
            other => panic!("{other:?}"),
        }
        assert_exhausted(&mut parser);
    }
    let mut owner = StrictParser::new(
        b"Jos\xe9<EOH><NAME:3>Ray<EOR>",
        ParseOpts {
            encoding: ParseEncoding::Latin1,
            field_length: LengthUnit::Bytes,
        },
    );
    let file = parse_file(&mut owner);
    assert_eq!(
        file.header
            .expect("input should yield a header because it starts with a preamble")
            .text(),
        "José"
    );
}

#[test]
fn structural_errors_have_typed_categories_and_original_locations() {
    use qso_io::adif::adi::parser::error::{StructuralError, SyntaxErrorKind};
    for (bytes, reason, span) in [
        (
            &b"<NAME:x>Ray<EOR>"[..],
            (|actual: &SyntaxErrorKind| matches!(actual, SyntaxErrorKind::InvalidLength))
                as fn(&SyntaxErrorKind) -> bool,
            6..7,
        ),
        (
            &b"<NAME:3:Z>Ray<EOR>"[..],
            (|actual: &SyntaxErrorKind| {
                matches!(actual, SyntaxErrorKind::UnknownTypeIndicator { .. })
            }) as fn(&SyntaxErrorKind) -> bool,
            8..9,
        ),
        (
            &b"<NAME:3>Ray"[..],
            (|actual: &SyntaxErrorKind| matches!(actual, SyntaxErrorKind::MissingEor))
                as fn(&SyntaxErrorKind) -> bool,
            11..11,
        ),
        (
            &b"P<NAME:1>A"[..],
            (|actual: &SyntaxErrorKind| matches!(actual, SyntaxErrorKind::MissingEoh))
                as fn(&SyntaxErrorKind) -> bool,
            10..10,
        ),
    ] {
        let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
        let mut parser = parser_owner.into_iter();
        let error = next_error(&mut parser);
        assert_exhausted(&mut parser);
        assert_eq!(error.span(), span);
        assert!(
            matches!(&error, StructuralError::Syntax { reason: actual, .. } if reason(actual)),
            "input {bytes:?}: expected {reason:?}, got {error:?}"
        );
    }
    let bytes = format!("<NAME:{}0>", usize::MAX);
    let mut parser_owner = StrictParser::new(bytes.as_bytes(), ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    let error = next_error(&mut parser);
    assert_exhausted(&mut parser);
    assert!(
        matches!(
            &error,
            StructuralError::Syntax {
                reason: SyntaxErrorKind::LengthOverflow { .. },
                ..
            }
        ),
        "unexpected value: {:?}",
        error
    );
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn scalar_utf8_errors_retain_the_standard_cause_and_sequence_span() {
    use qso_io::adif::adi::parser::error::StructuralError;
    for (bytes, span, error_len) in [
        (&b"<NAME:1>\xe2A"[..], 8..9, Some(1)),
        (&b"<NAME:1>\xe2\x82"[..], 8..10, None),
        (&b"<NAME:1>\xff"[..], 8..9, Some(1)),
    ] {
        let mut parser_owner = StrictParser::new(
            bytes,
            ParseOpts {
                encoding: ParseEncoding::Utf8,
                field_length: LengthUnit::UnicodeScalars,
            },
        );
        let mut parser = parser_owner.into_iter();
        let error = next_error(&mut parser);
        assert_exhausted(&mut parser);
        assert_eq!(error.span(), span);
        let StructuralError::Decoding {
            cause: DecodeError::Utf8(cause),
            ..
        } = &error
        else {
            panic!("{error:?}");
        };
        assert_eq!(cause.error_len(), error_len);
        assert_eq!(cause.valid_up_to(), 0);
    }
}
