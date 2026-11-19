use std::error::Error;

use qso_io::adif::adx::parser::{
    error::{ParseError, WarningKind},
    permissive::{PermissiveParser, PermissiveSection},
};

#[test]
fn section_events_preserve_source_and_finish_with_none() {
    let input = b"<ADX><HEADER/><RECORDS><RECORD><CALL>W1AW</CALL></RECORD></RECORDS></ADX>";
    let mut parser = PermissiveParser::new(input);

    let start = parser.next().unwrap().unwrap();

    assert!(start.warnings.is_empty());
    assert!(
        matches!(start.data, PermissiveSection::StartAdx(span) if &input[span.clone()] == b"<ADX>")
    );
    assert!(matches!(parser.next().unwrap().unwrap().data,
        PermissiveSection::StartHeader(span) if &input[span.clone()] == b"<HEADER/>"));

    let PermissiveSection::EndHeader(header) = parser.next().unwrap().unwrap().data else {
        panic!("expected complete header");
    };

    assert_eq!(&input[header.span()], b"<HEADER/>");

    assert!(matches!(parser.next().unwrap().unwrap().data,
        PermissiveSection::StartRecords(span) if &input[span.clone()] == b"<RECORDS>"));
    assert!(matches!(parser.next().unwrap().unwrap().data,
        PermissiveSection::StartRecord(span) if &input[span.clone()] == b"<RECORD>"));

    let PermissiveSection::EndRecord(record) = parser.next().unwrap().unwrap().data else {
        panic!("expected complete record");
    };

    let field = &record.fields()[0];

    assert_eq!(field.name(), "CALL");
    assert_eq!(field.raw_data(), b"W1AW");
    assert_eq!(field.text(), Ok("W1AW"));
    assert_eq!(&input[field.element_span()], b"<CALL>W1AW</CALL>");
    assert_eq!(&input[record.span()], b"<RECORD><CALL>W1AW</CALL></RECORD>");

    assert!(matches!(parser.next().unwrap().unwrap().data,
        PermissiveSection::EndRecords(span) if &input[span.clone()] == b"</RECORDS>"));
    assert!(matches!(parser.next().unwrap().unwrap().data,
        PermissiveSection::EndAdx(span) if &input[span.clone()] == b"</ADX>"));
    assert!(parser.next().is_none());
    assert!(parser.next().is_none());
}

#[test]
fn warnings_follow_events_or_failure_once() {
    let mut p = PermissiveParser::new(b"<!DOCTYPE ADX><ADX><HEADER/><RECORDS><RECORD><CALL><![CDATA[A]]></CALL><X></Y></RECORD></RECORDS></ADX>");

    assert_eq!(p.next().unwrap().unwrap().warnings.len(), 1);

    for result in p.by_ref() {
        match result {
            Ok(result) => assert!(result.warnings.is_empty()),
            Err(f) => {
                assert_eq!(f.warnings.len(), 1);
                assert_eq!(f.warnings[0].kind, WarningKind::Cdata);
                assert!(matches!(f.error, ParseError::Xml { .. }));
                assert!(f.error.source().is_some());
                assert!(p.next().is_none());
                assert!(p.next().is_none());
                return;
            }
        }
    }

    panic!("expected terminal XML failure");
}

#[test]
fn returns_prefix_before_late_failures() {
    let base = b"<ADX><HEADER/><RECORDS><RECORD><CALL>W1AW</CALL></RECORD>";
    for suffix in [
        b"</RECORDS></ADX>\xff".as_slice(),
        b"",
        b"</RECORDS></ADX><OTHER/>",
    ] {
        let input = [base.as_slice(), suffix].concat();
        let mut p = super::permissive_sections(&input);
        p.next().unwrap().unwrap();
        let r = p.next().unwrap().unwrap();

        assert!(p.next().unwrap().is_err(), "{suffix:?}");
        assert!(p.next().is_none());
        drop(p);
        let PermissiveSection::EndRecord(r) = r.data else {
            panic!()
        };

        assert_eq!(r.fields()[0].text(), Ok("W1AW"));
    }

    let mut p = super::permissive_sections(b"<ADX><HEADER/><RECORDS/></ADX>oops");
    p.next().unwrap().unwrap();

    assert!(p.next().unwrap().is_err());
}

#[test]
fn utf8_bom_preserves_original_offsets() {
    let input = b"\xef\xbb\xbf<?xml version='1.0' encoding='UTF-8'?><ADX><HEADER><CALL>W1AW</CALL></HEADER><RECORDS/></ADX>";
    let mut p = super::permissive_sections(input);
    let h = p.next().unwrap().unwrap();
    let PermissiveSection::EndHeader(h) = h.data else {
        panic!()
    };

    assert_eq!(&input[h.fields()[0].value_span()], b"W1AW");
    assert_eq!(&input[h.span()], b"<HEADER><CALL>W1AW</CALL></HEADER>");
    assert!(p.next().is_none());
}

#[test]
fn attribute_checks_continue_after_custom_reference() {
    let input =
        b"<ADX><HEADER/><RECORDS><RECORD><CALL X='&custom;&;'>a</CALL></RECORD></RECORDS></ADX>";
    let mut p = super::permissive_sections(input);
    p.next().unwrap().unwrap();
    let f = p.next().unwrap().unwrap_err();

    assert!(matches!(f.error, ParseError::Xml { .. }));
    assert_eq!(f.warnings.len(), 1);
    assert_eq!(&input[f.warnings[0].span.clone()], b"&custom;");
}

#[test]
fn supported_xml_checks_are_terminal() {
    let fields = [
        "<1BAD/>",
        "<CALL A='1' A='2'/>",
        "<CALL X='<'>a</CALL>",
        "<CALL>]]></CALL>",
        "<CALL>&#1;</CALL>",
        "<CALL>\u{1}</CALL>",
        "<CALL X='&#xFFFF;'/>",
        "<CALL><?1bad?></CALL>",
        "<CALL><?XML foo?></CALL>",
        "<CALL><?xml version='1.0'?></CALL>",
    ];
    for field in fields {
        let input = format!("<ADX><HEADER/><RECORDS><RECORD>{field}</RECORD></RECORDS></ADX>");
        let f = super::permissive_sections(input.as_bytes())
            .find_map(Result::err)
            .unwrap_or_else(|| panic!("accepted {field:?}"));

        assert!(matches!(f.error, ParseError::Xml { .. }));
    }

    for input in [
        "<ADX><HEADER/><RECORDS/></ADX><ADX/>",
        "<ADX><HEADER/><RECORDS/></ADX><![CDATA[x]]>",
        " <?xml version='1.0'?><ADX><HEADER/><RECORDS/></ADX>",
        "<?xml version='1.1'?><ADX><HEADER/><RECORDS/></ADX>",
        "<?xml version='1.0' other='x'?><ADX><HEADER/><RECORDS/></ADX>",
        "<?xml version='1.0' encoding='Latin-1'?><ADX><HEADER/><RECORDS/></ADX>",
    ] {
        assert!(
            super::permissive_sections(input.as_bytes()).any(|r| r.is_err()),
            "accepted {input}"
        );
    }

    let input =
        b"<ADX><HEADER/><RECORDS><RECORD><CALL X='&lt;'>]]&gt;&lt;</CALL></RECORD></RECORDS></ADX>";

    assert!(super::permissive_sections(input).all(|r| r.is_ok()));
}

#[test]
fn envelope_errors_are_not_xml_errors() {
    for input in [
        b"<adx><header/><records/></adx>".as_slice(),
        b"<ADX><RECORD/></ADX>",
    ] {
        let f = super::permissive_sections(input)
            .next()
            .unwrap()
            .unwrap_err();

        assert!(matches!(f.error, ParseError::Envelope { .. }));
    }

    let mut p = super::permissive_sections(b"<ADX><HEADER/><RECORDS></RECORDZ></ADX>");
    p.next().unwrap().unwrap();

    assert!(matches!(
        p.next().unwrap().unwrap_err().error,
        ParseError::Xml { .. }
    ));
}

#[test]
fn missing_identity_warning_precedes_later_field_failure() {
    let mut p = super::permissive_sections(b"<ADX><HEADER/><RECORDS><RECORD><APP FIELDNAME='x'>before<X></Y></APP></RECORD></RECORDS></ADX>");
    p.next().unwrap().unwrap();
    let f = p.next().unwrap().unwrap_err();

    assert!(matches!(
        f.warnings.first().map(|warning| &warning.kind),
        Some(WarningKind::MissingAttribute(name)) if *name == "PROGRAMID"
    ));
}

#[test]
fn direct_envelope_elements_cannot_be_fields() {
    for name in ["ADX", "HEADER", "RECORDS", "RECORD", "n:RECORD"] {
        for container in ["HEADER", "RECORD"] {
            let field = format!("<{name}/>");
            let input = if container == "HEADER" {
                format!("<ADX><HEADER>{field}</HEADER><RECORDS/></ADX>")
            } else {
                format!("<ADX><HEADER/><RECORDS><RECORD>{field}</RECORD></RECORDS></ADX>")
            };

            let f = super::permissive_sections(input.as_bytes())
                .find_map(Result::err)
                .unwrap_or_else(|| panic!("accepted {input}"));

            assert!(matches!(f.error, ParseError::Envelope { .. }));
        }
    }
}

#[test]
fn attribute_whitespace_is_checked_at_every_depth() {
    for input in [
        "<ADX A='x'B='y'><HEADER/><RECORDS/></ADX>",
        "<ADX><HEADER/><RECORDS><RECORD><APP PROGRAMID='A'FIELDNAME='F'TYPE='S'>foo</APP></RECORD></RECORDS></ADX>",
        "<ADX><HEADER/><RECORDS><RECORD><X><Y A='x'B='y'/></X></RECORD></RECORDS></ADX>",
    ] {
        let f = super::permissive_sections(input.as_bytes())
            .find_map(Result::err)
            .unwrap_or_else(|| panic!("accepted {input}"));

        assert!(matches!(f.error, ParseError::Xml { .. }));
    }
}

#[test]
fn empty_records_deliver_collection_warnings_once() {
    let input = b"<!DOCTYPE ADX [<!ENTITY operator 'Alice'>]><ADX><HEADER/><RECORDS NOTE='&operator;'/></ADX>";
    let mut parser = PermissiveParser::new(input);
    let root = parser.next().unwrap().unwrap();

    assert_eq!(root.warnings.len(), 2);
    assert_eq!(root.warnings[0].kind, WarningKind::Doctype);
    assert_eq!(
        root.warnings[1].kind,
        WarningKind::EntityDefinition("operator")
    );

    parser.next().unwrap().unwrap();
    parser.next().unwrap().unwrap();
    let records = parser.next().unwrap().unwrap();

    assert!(
        matches!(records.data, PermissiveSection::StartRecords(ref span)
        if &input[span.clone()] == b"<RECORDS NOTE='&operator;'/>")
    );
    assert_eq!(records.warnings.len(), 1);
    assert_eq!(
        records.warnings[0].kind,
        WarningKind::CustomReference("operator")
    );
    assert_eq!(&input[records.warnings[0].span.clone()], b"&operator;");

    let records_end = match records.data {
        PermissiveSection::StartRecords(span) => span.end,
        _ => unreachable!(),
    };
    let end = parser.next().unwrap().unwrap();

    assert!(matches!(end.data, PermissiveSection::EndRecords(span)
        if span == (records_end..records_end)));
    assert!(end.warnings.is_empty());
    assert!(matches!(
        parser.next().unwrap().unwrap().data,
        PermissiveSection::EndAdx(_)
    ));
    assert!(parser.next().is_none());
    assert!(parser.next().is_none());
}

#[test]
fn attribute_warning_precedes_later_attribute_syntax_failure() {
    for suffix in ["B/>", "B='\u{1}'/>"] {
        let input = format!(
            "<ADX><HEADER/><RECORDS><RECORD><X A='&custom;' {suffix}</RECORD></RECORDS></ADX>"
        );
        let mut p = super::permissive_sections(input.as_bytes());
        p.next().unwrap().unwrap();
        let f = p.next().unwrap().unwrap_err();

        assert!(matches!(f.error, ParseError::Xml { .. }));
        assert_eq!(f.warnings.len(), 1);
        assert_eq!(&input.as_bytes()[f.warnings[0].span.clone()], b"&custom;");
    }
}

#[test]
fn opaque_doctype_still_checks_literal_xml_characters() {
    let input = b"<!DOCTYPE ADX [<!--a\x01b-->]><ADX><HEADER/><RECORDS/></ADX>";
    let f = super::permissive_sections(input)
        .next()
        .unwrap()
        .unwrap_err();

    assert!(matches!(f.error, ParseError::Xml { .. }));
    assert_eq!(&input[f.error.span()], b"\x01");
}

#[test]
fn invalid_utf8_locations_identify_original_sequence() {
    for input in [
        b"<ADX><HEADER/><RECORDS><RECORD><CALL>abc\xffdef</CALL></RECORD></RECORDS></ADX>"
            .as_slice(),
        b"<ADX><HEADER/><RECORDS><RECORD><CALL A='foo\xffbar'/></RECORD></RECORDS></ADX>",
        b"<ADX><HEADER/><RECORDS><RECORD><CALL>abc\xe2\x82</CALL></RECORD></RECORDS></ADX>",
    ] {
        let f = super::permissive_sections(input)
            .find_map(Result::err)
            .unwrap();

        assert!(matches!(f.error, ParseError::Xml { .. }));

        let wanted = if input.contains(&0xff) {
            b"\xff".as_slice()
        } else {
            b"\xe2\x82"
        };

        assert_eq!(&input[f.error.span()], wanted);
    }
}

#[test]
fn collection_attribute_warnings_survive_later_xml_failure() {
    for value in ["&custom;&;", "&custom;<", "&custom;&#1;"] {
        let input = format!("<ADX><HEADER/><RECORDS X='{value}'/></ADX>");
        let mut parser = PermissiveParser::new(input.as_bytes());
        let failure = parser.by_ref().find_map(Result::err).unwrap();

        assert!(matches!(failure.error, ParseError::Xml { .. }));
        assert_eq!(failure.warnings.len(), 1);
        assert_eq!(
            failure.warnings[0].kind,
            WarningKind::CustomReference("custom")
        );
        assert_eq!(
            &input.as_bytes()[failure.warnings[0].span.clone()],
            b"&custom;"
        );
        assert!(parser.next().is_none());
    }
}
