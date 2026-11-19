use qso_io::adif::{
    DataType,
    adx::parser::{
        error::{ParseError, WarningKind},
        strict::{Section, StrictParser, record::Record},
    },
    value::{TimePrecision, Value},
};

fn one_record(input: &[u8]) -> Record<'_> {
    let mut owner = StrictParser::new(input);
    let mut p = super::super::strict_sections(&mut owner);
    p.next().unwrap().unwrap();
    let Section::EndRecord(r) = p.next().unwrap().unwrap().data else {
        panic!()
    };
    r
}

#[test]
fn standard_types_use_decoded_values() {
    let input = concat!(
        "<ADX>",
        "<HEADER/>",
        "<RECORDS>",
        "<RECORD>",
        "<FREQ>14.&#x30;74</FREQ>",
        "<QSO_DATE>20261117</QSO_DATE>",
        "<TIME_ON>12&#51;4</TIME_ON>",
        "<NAME_INTL>José</NAME_INTL>",
        "<VUCC_GRIDS>EM98,fm08</VUCC_GRIDS>",
        "</RECORD>",
        "</RECORDS>",
        "</ADX>",
    );
    let mut owner = StrictParser::new(input.as_bytes());
    let mut p = super::super::strict_sections(&mut owner);
    p.next().unwrap().unwrap();
    let r = p.next().unwrap().unwrap();

    assert!(r.warnings.is_empty());

    let Section::EndRecord(r) = r.data else {
        panic!("expected record");
    };

    let f = r.field("FREQ").unwrap().unwrap();

    assert_eq!(f.text(), "14.074");
    assert_eq!(f.raw_data(), b"14.&#x30;74");
    assert!(matches!(f.value(), Some(Value::Number(n)) if n.to_string() == "14.074"));
    assert!(matches!(
        r.field("QSO_DATE").unwrap().unwrap().value(),
        Some(Value::Date(date)) if date.to_string() == "2026-11-17"
    ));

    assert!(matches!(
        r.field("TIME_ON").unwrap().unwrap().value(),
        Some(Value::Time(time))
            if time.precision() == TimePrecision::Minute
                && time.time().to_string() == "12:34:00"
    ));

    assert_eq!(r.field("NAME_INTL").unwrap().unwrap().text(), "José");
    assert!(matches!(
        r.field("VUCC_GRIDS").unwrap().unwrap().value(),
        Some(Value::List(members)) if members == &["EM98", "fm08"]
    ));

    assert!(p.next().is_none());
}

#[test]
fn strict_and_permissive_diverge_only_at_supported_checks() {
    for field in [
        "<K_INDEX>1.33</K_INDEX>",
        "<CALL><![CDATA[x]]></CALL>",
        "<CALL>&custom;</CALL>",
        "<CALL><em>x</em></CALL>",
        "<APP FIELDNAME='x'>42</APP>",
    ] {
        let input = format!("<ADX><HEADER/><RECORDS><RECORD>{field}</RECORD></RECORDS></ADX>");

        assert!(super::super::permissive_sections(input.as_bytes()).all(|r| r.is_ok()));

        let mut owner = StrictParser::new(input.as_bytes());
        let mut p = super::super::strict_sections(&mut owner);
        p.next().unwrap().unwrap();
        let f = p.next().unwrap().unwrap_err();

        assert!(f.warnings.is_empty());
        assert!(p.next().is_none());
        if field.contains("K_INDEX") {
            assert!(matches!(
                f.error,
                ParseError::Value {
                    cause: qso_io::adif::value::ValueError::InvalidInteger,
                    ..
                }
            ));
        }
    }

    let mut owner = StrictParser::new(b"<!DOCTYPE ADX><ADX><HEADER/><RECORDS/></ADX>");
    let mut p = super::super::strict_sections(&mut owner);

    assert!(p.next().unwrap().unwrap_err().warnings.is_empty());
    assert!(p.next().is_none());
}

#[test]
fn decoded_empty_is_not_missing() {
    for field in [
        "<FREQ/>",
        "<FREQ></FREQ>",
        "<FREQ><!--note--><?ignored x?></FREQ>",
    ] {
        let input = format!("<ADX><HEADER/><RECORDS><RECORD>{field}</RECORD></RECORDS></ADX>");
        let r = one_record(input.as_bytes());
        let f = r.field("FREQ").unwrap().unwrap();

        assert_eq!(f.value(), None);
        assert_eq!(f.text(), "");
        assert_eq!(f.raw_element(), field.as_bytes());
        assert!(r.field("CALL").unwrap().is_none());
    }
}

#[test]
fn adx_text_types_respect_xml_normalization() {
    for (name, text, want, valid) in [
        ("NAME", "José", "José", false),
        ("NAME_INTL", "José", "José", true),
        ("NAME_INTL", "a\r\nb", "a\nb", false),
        ("ADDRESS", "a\r\nb", "a\nb", true),
        ("ADDRESS_INTL", "é\r\nb", "é\nb", true),
        ("ADDRESS_INTL", "é&#13;&#10;b", "é\r\nb", true),
        ("ADDRESS", "é", "é", false),
        ("NAME", "  A  ", "  A  ", true),
    ] {
        let input = format!(
            "<ADX><HEADER/><RECORDS><RECORD><{name}>{text}</{name}></RECORD></RECORDS></ADX>"
        );
        let mut owner = StrictParser::new(input.as_bytes());
        let result = super::super::strict_sections(&mut owner).nth(1).unwrap();
        if valid {
            let Section::EndRecord(r) = result.unwrap().data else {
                panic!()
            };

            assert_eq!(r.fields()[0].text(), want);
        } else {
            assert!(result.is_err(), "{name} {text}");
        }
    }
}

#[test]
fn strict_value_failures_locate_original_segments() {
    for (field, want) in [
        ("<FREQ>1&#x30;x</FREQ>", "x"),
        ("<FREQ>10<!--gap-->x</FREQ>", "x"),
        ("<FREQ>1&#x78;</FREQ>", "&#x78;"),
        ("<FREQ>10x</FREQ>", "x"),
        ("<QSO_DATE>2024&#48;230</QSO_DATE>", "2024&#48;230"),
        ("<TIME_ON>2&#52;00</TIME_ON>", "2&#52;00"),
        ("<TIME_ON>2359&#54;0</TIME_ON>", "2359&#54;0"),
        (
            "<APP PROGRAMID='p' FIELDNAME='f' TYPE='&#x3F;'>1</APP>",
            "&#x3F;",
        ),
    ] {
        let input = format!("<ADX><HEADER/><RECORDS><RECORD>{field}</RECORD></RECORDS></ADX>");
        let mut owner = StrictParser::new(input.as_bytes());
        let e = super::super::strict_sections(&mut owner)
            .find_map(Result::err)
            .unwrap()
            .error;

        assert_eq!(&input[e.span()], want, "{field}");
    }
}

#[test]
fn allocated_list_text_outlives_parser() {
    let input = concat!(
        "<ADX>",
        "<HEADER/>",
        "<RECORDS>",
        "<RECORD>",
        "<CREDIT_GRANTED>IOTA,WAS:LOTW&amp;CARD</CREDIT_GRANTED>",
        "</RECORD>",
        "</RECORDS>",
        "</ADX>",
    )
    .as_bytes();
    let r = one_record(input);
    let f = &r.fields()[0];

    assert_eq!(f.text(), "IOTA,WAS:LOTW&CARD");
    assert_eq!(f.raw_data(), b"IOTA,WAS:LOTW&amp;CARD");
    assert!(matches!(f.value(), Some(Value::List(v)) if v == &["IOTA", "WAS:LOTW&CARD"]));
}

#[test]
fn strict_duplicate_identities_are_distinct_from_tags() {
    let input =
        b"<ADX><HEADER/><RECORDS><RECORD><FREQ>bad</FREQ><FREQ>two</FREQ></RECORD></RECORDS></ADX>";
    let mut owner = StrictParser::new(input);
    let e = super::super::strict_sections(&mut owner)
        .find_map(Result::err)
        .unwrap()
        .error;

    assert_eq!(&input[e.span()], b"<FREQ>two</FREQ>");
    assert_eq!(e.related_spans().count(), 1);
    assert_eq!(
        &input[e.related_spans().next().unwrap().clone()],
        b"<FREQ>bad</FREQ>"
    );
}

#[test]
fn standard_definition_alternatives_and_membership() {
    for (text, kind, decoded, expected_members) in [
        (
            "DXCC,IOTA",
            DataType::CreditList,
            "DXCC,IOTA",
            &["DXCC", "IOTA"][..],
        ),
        ("A&#74;A", DataType::AwardList, "AJA", &["AJA"][..]),
    ] {
        let input = format!(
            "<ADX><HEADER/><RECORDS><RECORD><CREDIT_GRANTED>{text}</CREDIT_GRANTED><STATE>MA</STATE><FUTURE>abc</FUTURE></RECORD></RECORDS></ADX>"
        );
        let r = one_record(input.as_bytes());

        assert_eq!(r.fields()[0].data_type(), Some(kind));
        assert!(
            matches!(r.fields()[0].value(), Some(Value::List(members)) if members.as_slice() == expected_members)
        );
        assert_eq!(r.fields()[0].text(), decoded);
        assert_eq!(r.fields()[2].data_type(), None);
    }

    let input = concat!(
        "<ADX>",
        "<HEADER/>",
        "<RECORDS>",
        "<RECORD>",
        "<CREDIT_GRANTED>A&#74;A,N&#79;PE</CREDIT_GRANTED>",
        "</RECORD>",
        "</RECORDS>",
        "</ADX>",
    )
    .as_bytes();
    let mut owner = StrictParser::new(input);
    let e = super::super::strict_sections(&mut owner)
        .find_map(Result::err)
        .unwrap()
        .error;
    let ParseError::NoMatchingType { failures, .. } = e else {
        panic!("{e:?}")
    };

    assert_eq!(failures.len(), 2);
    assert_eq!(&input[failures[0].error.span()], b"A&#74;A");
    assert_eq!(&input[failures[1].error.span()], b"N&#79;PE");

    let input = b"<ADX><HEADER/><RECORDS><RECORD><MODE>bad</MODE></RECORD></RECORDS></ADX>";

    let mut owner = StrictParser::new(input);
    assert!(matches!(
        super::super::strict_sections(&mut owner)
            .find_map(Result::err)
            .unwrap()
            .error,
        ParseError::Value {
            enumeration: Some("Mode"),
            ..
        }
    ));
}

#[test]
fn adx_supplied_syntax_rules() {
    for field in [
        "<call>W1AW</call>",
        "<CALL type='S'>W1AW</CALL>",
        "<CALL TYPE='S'/>",
        "<FUTURE TYPE='N'>1</FUTURE>",
        "<APP PROGRAMID='p' FIELDNAME='f' TYPE='?'/>",
    ] {
        let input = format!("<ADX><HEADER/><RECORDS><RECORD>{field}</RECORD></RECORDS></ADX>");

        let mut owner = StrictParser::new(input.as_bytes());
        assert!(
            super::super::strict_sections(&mut owner)
                .nth(1)
                .unwrap()
                .is_err(),
            "{field}"
        );

        assert!(super::super::permissive_sections(input.as_bytes()).all(|r| r.is_ok()));
    }

    let input =
        b"<ADX><HEADER><APP PROGRAMID='p' FIELDNAME='f' TYPE='S'>x</APP></HEADER><RECORDS/></ADX>";

    let mut owner = StrictParser::new(input);
    assert!(
        super::super::strict_sections(&mut owner)
            .next()
            .unwrap()
            .is_err()
    );

    let input = concat!(
        "<ADX>",
        "<HEADER/>",
        "<RECORDS>",
        "<RECORD>",
        "<CALL xmlns:TYPE='urn:foo'>W1AW</CALL>",
        "</RECORD>",
        "</RECORDS>",
        "</ADX>",
    )
    .as_bytes();

    let mut owner = StrictParser::new(input);
    assert!(super::super::strict_sections(&mut owner).all(|r| r.is_ok()));

    let input = b"<ADX><HEADER/><RECORDS X='&custom;'/></ADX>";
    let mut owner = StrictParser::new(input);
    let mut strict = super::super::strict_sections(&mut owner);
    strict.next().unwrap().unwrap();

    assert!(matches!(
        strict.next().unwrap().unwrap_err().error,
        ParseError::Prohibited { construct: WarningKind::CustomReference(name), .. } if name == "custom"
    ));
    let input = concat!(
        "<n:ADX xmlns:n='urn:x'>",
        "<n:HEADER/>",
        "<n:RECORDS>",
        "<n:RECORD>",
        "<n:CALL>W1AW</n:CALL>",
        "</n:RECORD>",
        "</n:RECORDS>",
        "</n:ADX>",
    )
    .as_bytes();
    let r = one_record(input);

    assert_eq!(r.field("CALL").unwrap().unwrap().text(), "W1AW");
}

#[test]
fn leading_field_feff_is_text_when_locating_errors() {
    for prefix in ["\u{feff}", "\u{feff}\u{feff}"] {
        let input = format!(
            "<ADX><HEADER/><RECORDS><RECORD><NAME_INTL>{prefix}AB\r\nCD</NAME_INTL></RECORD></RECORDS></ADX>"
        );
        let mut owner = StrictParser::new(input.as_bytes());
        let f = super::super::strict_sections(&mut owner)
            .find_map(Result::err)
            .unwrap();

        assert_eq!(&input.as_bytes()[f.error.span()], b"\r\n");
    }
}
