use qso_io::adif::adx::parser::{
    error::TextError, error::WarningKind, permissive::PermissiveSection,
};

fn record(
    input: &[u8],
) -> (
    qso_io::adif::adx::parser::permissive::record::PermissiveRecord<'_>,
    Vec<qso_io::adif::adx::parser::error::ParseWarning<'_>>,
) {
    let mut p = super::permissive_sections(input);
    p.next().unwrap().unwrap();
    let result = p.next().unwrap().unwrap();
    let PermissiveSection::EndRecord(r) = result.data else {
        panic!("record")
    };
    (r, result.warnings)
}

#[test]
fn ordinary_decoding_preserves_raw_spelling() {
    let input = concat!(
        "<ADX>",
        "<HEADER/>",
        "<RECORDS>",
        "<RECORD>",
        "<CALL X='x\r\ny&#xA;z'> A&amp;B&#xE9;\r\n</CALL>",
        "</RECORD>",
        "</RECORDS>",
        "</ADX>",
    )
    .as_bytes();
    let (r, w) = record(input);

    assert!(w.is_empty());

    let f = &r.fields()[0];

    assert_eq!(f.text(), Ok(" A&Bé\n"));
    assert_eq!(f.raw_data(), b" A&amp;B&#xE9;\r\n");
    assert_eq!(f.attributes()[0].text(), Ok("x y\nz"));
    assert_eq!(&input[f.attributes()[0].value_span()], b"x\r\ny&#xA;z");
}

#[test]
fn prohibited_content_warns_without_interpretation() {
    let input = b"<!DOCTYPE ADX [<!ENTITY operator '<RECORD/>'><!ATTLIST CALL extra CDATA 'default'>]><ADX><HEADER><NAME_INTL>Alice</NAME_INTL></HEADER><RECORDS><RECORD><CALL>Hello &operator;</CALL><NAME_INTL><![CDATA[Jos\xc3\xa9]]></NAME_INTL></RECORD></RECORDS></ADX>";
    let mut events = qso_io::adif::adx::parser::permissive::PermissiveParser::new(input);
    let root = events.next().unwrap().unwrap();

    assert_eq!(root.warnings.len(), 2);
    assert!(matches!(root.warnings[0].kind, WarningKind::Doctype));
    assert!(matches!(&root.warnings[1].kind, WarningKind::EntityDefinition(n) if *n == "operator"));
    assert_eq!(
        &input[root.warnings[1].span.clone()],
        b"<!ENTITY operator '<RECORD/>'>"
    );

    let mut p = events.filter(|result| {
        matches!(
            result,
            Err(_)
                | Ok(qso_io::adif::adx::parser::ParseResult {
                    data: PermissiveSection::EndHeader(_) | PermissiveSection::EndRecord(_),
                    ..
                })
        )
    });

    let h = p.next().unwrap().unwrap();

    assert!(h.warnings.is_empty());
    let PermissiveSection::EndHeader(h) = h.data else {
        panic!()
    };

    assert_eq!(h.fields()[0].text(), Ok("Alice"));

    let r = p.next().unwrap().unwrap();

    assert_eq!(r.warnings.len(), 2);
    assert_eq!(&input[r.warnings[0].span.clone()], b"&operator;");

    let PermissiveSection::EndRecord(r) = r.data else {
        panic!()
    };

    assert!(matches!(r.fields()[0].text(), Err(TextError::CustomReference(n)) if *n == "operator"));
    assert!(r.fields()[0].attributes().is_empty());
    assert_eq!(r.fields()[0].raw_data(), b"Hello &operator;");
    assert_eq!(r.fields()[1].text(), Ok("José"));
    assert!(p.next().is_none());
}

#[test]
fn nested_markup_is_not_scalar_or_new_fields() {
    let input = concat!(
        "<ADX>",
        "<HEADER/>",
        "<RECORDS>",
        "<RECORD>",
        "<NAME_INTL>before<em>José<RECORD/>",
        "</em>after</NAME_INTL>",
        "<CALL>W1AW</CALL>",
        "</RECORD>",
        "</RECORDS>",
        "</ADX>",
    );
    let (r, w) = record(input.as_bytes());

    assert!(!w.is_empty());
    assert_eq!(r.fields().len(), 2);
    assert_eq!(
        r.fields()[0].raw_data(),
        "before<em>José<RECORD/></em>after".as_bytes()
    );

    assert_eq!(r.fields()[0].text(), Err(&TextError::NestedMarkup));
    assert_eq!(r.fields()[1].text(), Ok("W1AW"));
}

#[test]
fn missing_identity_is_preserved_without_guessing() {
    let (r, w) = record(b"<ADX><HEADER/><RECORDS><RECORD><APP FIELDNAME='x' TYPE='N'>42</APP><USERDEF>42</USERDEF></RECORD></RECORDS></ADX>");

    assert_eq!(w.len(), 2);
    for f in r.fields() {
        assert_eq!(f.text(), Ok("42"));
        assert!(f.identity().is_none());
    }

    assert_eq!(r.fields()[0].attributes().len(), 2);
}

#[test]
fn unknown_names_and_invalid_adif_values_are_unchecked() {
    let (r, w) = record(b"<ADX><HEADER/><RECORDS><RECORD><FUTURE_FIELD>abc</FUTURE_FIELD><K_INDEX>1.33</K_INDEX></RECORD></RECORDS></ADX>");

    assert!(w.is_empty());
    assert_eq!(r.fields()[0].text(), Ok("abc"));
    assert_eq!(r.fields()[1].text(), Ok("1.33"));
}

#[test]
fn namespace_declarations_do_not_supply_field_metadata() {
    let input = concat!(
        "<ADX>",
        "<HEADER/>",
        "<RECORDS>",
        "<RECORD>",
        "<APP xmlns:PROGRAMID='urn:example' FIELDNAME='F' TYPE='S'>foo</APP>",
        "</RECORD>",
        "</RECORDS>",
        "</ADX>",
    )
    .as_bytes();
    let (r, w) = record(input);
    let f = &r.fields()[0];

    assert!(f.identity().is_none());
    assert!(f.attribute("PROGRAMID").is_none());
    assert_eq!(f.attributes().len(), 3);
    assert!(matches!(w[0].kind, WarningKind::MissingAttribute(n) if n == "PROGRAMID"));
}
