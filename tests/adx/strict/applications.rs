use qso_io::adif::adx::parser::strict::{Section, StrictParser};

#[test]
fn application_identity_is_a_pair() {
    let input = b"<ADX><HEADER/><RECORDS><RECORD><APP PROGRAMID='A_B' FIELDNAME='C' TYPE='N'>1</APP><APP PROGRAMID='A' FIELDNAME='B_C' TYPE='I'>Jos\xc3\xa9</APP></RECORD></RECORDS></ADX>";
    let mut owner = StrictParser::new(input);
    let mut p = super::super::strict_sections(&mut owner);
    p.next().unwrap().unwrap();
    let Section::EndRecord(r) = p.next().unwrap().unwrap().data else {
        panic!()
    };

    assert_eq!(
        r.application_field("a_b", "c").unwrap().unwrap().text(),
        "1"
    );

    assert_eq!(
        r.application_field("a", "b_c").unwrap().unwrap().text(),
        "José"
    );
}

#[test]
fn application_type_history_is_document_local() {
    for (second, valid) in [
        ("<APP PROGRAMID='p' FIELDNAME='f' TYPE='n'>2</APP>", true),
        ("<APP PROGRAMID='p' FIELDNAME='f' TYPE='S'>2</APP>", false),
        ("<APP PROGRAMID='p' FIELDNAME='f'>2</APP>", false),
    ] {
        let input = format!(
            "<ADX><HEADER/><RECORDS><RECORD><APP PROGRAMID='P' FIELDNAME='F' TYPE='N'>1</APP></RECORD><RECORD>{second}</RECORD></RECORDS></ADX>"
        );
        let mut owner = StrictParser::new(input.as_bytes());
        {
            let mut p = super::super::strict_sections(&mut owner);
            p.next().unwrap().unwrap();
            p.next().unwrap().unwrap();
        }

        let mut p = super::super::strict_sections(&mut owner);
        let result = p.next().unwrap();

        assert_eq!(result.is_ok(), valid, "{result:?}");
        if second.contains("TYPE='S'") {
            let e = result.unwrap_err().error;

            assert_eq!(e.related_spans().count(), 1);
        }
    }

    let input = concat!(
        "<ADX>",
        "<HEADER/>",
        "<RECORDS>",
        "<RECORD>",
        "<APP PROGRAMID='P' FIELDNAME='F' TYPE='S'>one</APP>",
        "</RECORD>",
        "</RECORDS>",
        "</ADX>",
    )
    .as_bytes();

    let mut owner = StrictParser::new(input);
    assert!(super::super::strict_sections(&mut owner).all(|r| r.is_ok()));
}

#[test]
fn lookup_results_outlive_temporary_queries() {
    let input = concat!(
        "<ADX><HEADER><PROGRAMID>logger</PROGRAMID>",
        "<USERDEF FIELDID='1' TYPE='N'>SCORE</USERDEF></HEADER>",
        "<RECORDS><RECORD><CALL>W1AW</CALL>",
        "<APP PROGRAMID='logger' FIELDNAME='note' TYPE='S'>record note</APP>",
        "<USERDEF FIELDNAME='SCORE'>42</USERDEF>",
        "</RECORD></RECORDS></ADX>",
    );
    let mut owner = StrictParser::new(input.as_bytes());
    let mut parser = super::super::strict_sections(&mut owner);
    let Section::EndHeader(header) = parser.next().unwrap().unwrap().data else {
        panic!("expected header");
    };

    let Section::EndRecord(record) = parser.next().unwrap().unwrap().data else {
        panic!("expected record");
    };

    let (header_program, call, note, score) = {
        let program_name = String::from("PROGRAMID");
        let call_name = String::from("CALL");
        let program = String::from("logger");
        let note_name = String::from("note");
        let score_name = String::from("SCORE");

        (
            header.fields_named(&program_name).next().unwrap(),
            record.fields_named(&call_name).next().unwrap(),
            record
                .application_fields(&program, &note_name)
                .next()
                .unwrap(),
            record.user_fields(&score_name).next().unwrap(),
        )
    };

    assert_eq!(header_program.text(), "logger");
    assert_eq!(call.text(), "W1AW");
    assert_eq!(note.text(), "record note");
    assert_eq!(score.text(), "42");
}

#[test]
fn escaped_metadata_and_ordered_attribute_views_survive_the_parser() {
    use qso_io::adif::adx::parser::{identity::FieldIdentity, strict::header::HeaderEntry};

    let input = b"<ADX><HEADER><USERDEF FIELDID='&#49;' TYPE='S' ENUM='{A&amp;B,C}'>FAV_NOTE</USERDEF></HEADER><RECORDS><RECORD><APP EXTRA='first' FIELDNAME='no&#116;e' TYPE='S' PROGRAMID='A&amp;B' xmlns:other='urn:other' other:PROGRAMID='ignored' OTHER='last'>ok</APP><USERDEF FIELDNAME='FAV_NOTE'>A&amp;B</USERDEF></RECORD></RECORDS></ADX>";
    let record = {
        let mut owner = StrictParser::new(input);
        let mut sections = super::super::strict_sections(&mut owner);
        let header = sections
            .next()
            .expect("the supplied HEADER should yield a complete section")
            .expect("the supplied declaration should parse with predefined XML references");
        let Section::EndHeader(header) = header.data else {
            panic!("expected header");
        };
        let HeaderEntry::UserDefinition(definition) = &header.entries()[0] else {
            panic!("expected USERDEF declaration");
        };

        assert_eq!(definition.number().to_string(), "1");
        assert_eq!(&input[definition.identifier_span()], b"&#49;");
        assert_eq!(
            definition
                .attributes()
                .map(|a| (a.name(), a.text()))
                .collect::<Vec<_>>(),
            [("FIELDID", "1"), ("TYPE", "S"), ("ENUM", "{A&B,C}")]
        );

        let record = sections
            .next()
            .expect("the supplied RECORD should yield a complete section")
            .expect("the supplied record should satisfy its declaration");
        let Section::EndRecord(record) = record.data else {
            panic!("expected record");
        };
        assert!(sections.next().is_none());
        record
    };
    let field = record
        .application_field("a&b", "note")
        .expect("the record supplies one application field for this identity")
        .expect("the application field should match its decoded identifiers");

    assert!(
        matches!(field.identity(), FieldIdentity::Application { program_id, field_name }
        if program_id == "A&B" && field_name == "note")
    );
    assert_eq!(
        field
            .attributes()
            .map(|a| (a.name(), a.text(), &input[a.value_span()]))
            .collect::<Vec<_>>(),
        [
            ("EXTRA", "first", b"first".as_slice()),
            ("FIELDNAME", "note", b"no&#116;e".as_slice()),
            ("TYPE", "S", b"S".as_slice()),
            ("PROGRAMID", "A&B", b"A&amp;B".as_slice()),
            ("xmlns:other", "urn:other", b"urn:other".as_slice()),
            ("other:PROGRAMID", "ignored", b"ignored".as_slice()),
            ("OTHER", "last", b"last".as_slice())
        ]
    );
    assert_eq!(
        record
            .user_field("FAV_NOTE")
            .expect("the record supplies one FAV_NOTE reference")
            .expect("FAV_NOTE should retain its declared field")
            .text(),
        "A&B"
    );
}
