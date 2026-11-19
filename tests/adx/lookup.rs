use qso_io::adif::adx::parser::permissive::PermissiveSection;

#[test]
fn application_identity_is_a_pair() {
    let input = concat!(
        "<ADX>",
        "<HEADER/>",
        "<RECORDS>",
        "<RECORD>",
        "<APP PROGRAMID='A_B' FIELDNAME='C' TYPE='S'>one</APP>",
        "<APP PROGRAMID='A' FIELDNAME='B_C' TYPE='S'>two</APP>",
        "</RECORD>",
        "</RECORDS>",
        "</ADX>",
    )
    .as_bytes();
    let mut p = super::permissive_sections(input);
    p.next().unwrap().unwrap();
    let PermissiveSection::EndRecord(r) = p.next().unwrap().unwrap().data else {
        panic!()
    };
    drop(p);

    assert_eq!(
        r.application_field("a_b", "c").unwrap().unwrap().text(),
        Ok("one")
    );

    assert_eq!(
        r.application_field("A", "B_C").unwrap().unwrap().text(),
        Ok("two")
    );

    assert!(r.field("APP").unwrap().is_none());
}

#[test]
fn lookup_preserves_duplicates_empty_and_partial_identity() {
    let input = concat!(
        "<ADX>",
        "<HEADER/>",
        "<RECORDS>",
        "<RECORD>",
        "<CALL/>",
        "<call>two</call>",
        "<APP FIELDNAME='x'>partial</APP>",
        "<USERDEF FIELDNAME='u'>1</USERDEF>",
        "<USERDEF FIELDNAME='U'>2</USERDEF>",
        "<APP PROGRAMID='p' FIELDNAME='f'/>",
        "<APP PROGRAMID='P' FIELDNAME='F'>x</APP>",
        "</RECORD>",
        "</RECORDS>",
        "</ADX>",
    )
    .as_bytes();
    let mut p = super::permissive_sections(input);
    p.next().unwrap().unwrap();
    let PermissiveSection::EndRecord(r) = p.next().unwrap().unwrap().data else {
        panic!()
    };
    drop(p);

    assert_eq!(r.fields_named("call").count(), 2);
    assert_eq!(
        r.field("call").unwrap_err().spans,
        vec![r.fields()[0].element_span(), r.fields()[1].element_span()]
    );

    assert_eq!(r.fields()[0].text(), Ok(""));
    assert!(r.field("missing").unwrap().is_none());
    assert!(r.application_field("", "x").unwrap().is_none());
    assert_eq!(r.user_fields("u").count(), 2);
    assert_eq!(r.user_field("u").unwrap_err().spans.len(), 2);
    assert_eq!(r.application_fields("p", "f").count(), 2);
    assert_eq!(r.application_field("p", "f").unwrap_err().spans.len(), 2);
}

#[test]
fn lookup_results_outlive_temporary_queries() {
    let input = concat!(
        "<ADX><HEADER><PROGRAMID>logger</PROGRAMID>",
        "<APP PROGRAMID='logger' FIELDNAME='note'>header note</APP></HEADER>",
        "<RECORDS><RECORD><CALL>W1AW</CALL>",
        "<APP PROGRAMID='logger' FIELDNAME='note'>record note</APP>",
        "<USERDEF FIELDNAME='SCORE'>42</USERDEF>",
        "</RECORD></RECORDS></ADX>",
    );
    let mut parser = super::permissive_sections(input.as_bytes());
    let PermissiveSection::EndHeader(header) = parser.next().unwrap().unwrap().data else {
        panic!("expected header");
    };

    let PermissiveSection::EndRecord(record) = parser.next().unwrap().unwrap().data else {
        panic!("expected record");
    };

    let (header_program, header_note, call, record_note, score) = {
        let program = String::from("logger");
        let note = String::from("note");
        let call_name = String::from("CALL");
        let score_name = String::from("SCORE");
        let program_name = String::from("PROGRAMID");

        (
            header.fields_named(&program_name).next().unwrap(),
            header.application_fields(&program, &note).next().unwrap(),
            record.fields_named(&call_name).next().unwrap(),
            record.application_fields(&program, &note).next().unwrap(),
            record.user_fields(&score_name).next().unwrap(),
        )
    };

    assert_eq!(header_program.text(), Ok("logger"));
    assert_eq!(header_note.text(), Ok("header note"));
    assert_eq!(call.text(), Ok("W1AW"));
    assert_eq!(record_note.text(), Ok("record note"));
    assert_eq!(score.text(), Ok("42"));
}
