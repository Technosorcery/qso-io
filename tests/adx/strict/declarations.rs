use qso_io::adif::{
    DataType,
    adx::parser::{
        error::{DeclarationError, ParseError},
        strict::{Section, StrictParser, header::HeaderEntry},
    },
    value::{UserDefinitionConstraint, Value},
};

#[test]
fn header_and_completed_records_remain_usable_after_later_failure() {
    for (last, xml_failure) in [
        ("<RECORD><CALL>broken</WRONG>", true),
        (
            "<RECORD><USERDEF FIELDNAME='SIZE'>XL</USERDEF></RECORD></RECORDS></ADX>",
            false,
        ),
    ] {
        let input = format!(
            "<ADX><HEADER>\
             <USERDEF FIELDID='123456789012345678901234567890' TYPE='N' RANGE='{{5:20}}'>SCORE</USERDEF>\
             <USERDEF FIELDID='2' TYPE='E' ENUM='{{S,&#77;,L}}'>SIZE</USERDEF>\
             </HEADER><RECORDS>\
             <RECORD><USERDEF FIELDNAME='SCORE'>12.5</USERDEF>\
             <USERDEF FIELDNAME='SIZE'>M</USERDEF><NAME_INTL>Jos&#233;</NAME_INTL></RECORD>\
             <RECORD><USERDEF FIELDNAME='SIZE'>L</USERDEF></RECORD>{last}"
        );
        let mut parser = StrictParser::new(input.as_bytes());
        let mut events = parser.into_iter();
        let header = events
            .by_ref()
            .find_map(|outcome| match outcome.unwrap().data {
                Section::EndHeader(header) => Some(header),
                _ => None,
            })
            .unwrap();
        let mut records = Vec::new();
        let failure = loop {
            match events.next().expect("expected failure") {
                Ok(result) => {
                    if let Section::EndRecord(record) = result.data {
                        records.push(record);
                    }
                }
                Err(failure) => break failure,
            }
        };

        assert!(match &failure.error {
            ParseError::Xml { .. } => xml_failure,
            ParseError::ConstraintViolation { .. } => !xml_failure,
            _ => false,
        });
        assert!(failure.warnings.is_empty());
        assert!(events.next().is_none());
        drop(events);

        let HeaderEntry::UserDefinition(score) = &header.entries()[0] else {
            panic!("expected SCORE declaration");
        };
        let HeaderEntry::UserDefinition(size) = &header.entries()[1] else {
            panic!("expected SIZE declaration");
        };

        assert_eq!(score.number().to_string(), "123456789012345678901234567890");
        assert_eq!(score.field_name(), "SCORE");
        assert_eq!(score.data_type(), DataType::Number);
        assert_eq!(score.definition().name, score.field_name());
        assert_eq!(score.definition().data_type, Some(score.data_type()));
        assert_eq!(score.definition().constraint, score.constraint());
        assert_eq!(
            &input.as_bytes()[score.identifier_span()],
            b"123456789012345678901234567890"
        );
        assert_eq!(&input.as_bytes()[score.target_span()], b"SCORE");
        assert!(
            matches!(size.constraint(), Some(UserDefinitionConstraint::Enumeration(members)) if members == &["S", "M", "L"])
        );
        assert_eq!(records.len(), 2);
        assert!(
            matches!(records[0].user_field("score").unwrap().unwrap().value(), Some(Value::Number(n)) if n.to_string() == "12.5")
        );
        let name = records[0].field("NAME_INTL").unwrap().unwrap();

        assert_eq!(name.text(), "José");
        assert_eq!(name.raw_data(), b"Jos&#233;");
        assert_eq!(&input.as_bytes()[name.value_span()], b"Jos&#233;");
        assert_eq!(records[1].user_field("SIZE").unwrap().unwrap().text(), "L");
        assert!(parser.into_iter().next().is_none());
    }
}

#[test]
fn user_constraints_and_record_type_rules() {
    for declaration in [
        "<USERDEF FIELDID='1' TYPE='N' ENUM='{a,b}' RANGE='{1:2}'>X</USERDEF>",
        "<USERDEF FIELDID='0' TYPE='N'>X</USERDEF>",
        "<USERDEF FIELDID='1' TYPE='S'>CALL</USERDEF>",
        "<USERDEF FIELDID='1' TYPE='N' RANGE='{2:1}'>X</USERDEF>",
        "<USERDEF FIELDID='1' TYPE='N' RANGE='{1:1e2}'>X</USERDEF>",
        "<USERDEF FIELDID='1' TYPE='E' ENUM='{a,}'>X</USERDEF>",
        "<USERDEF FIELDID='1' TYPE='S'> X</USERDEF>",
        "<USERDEF FIELDID='1' TYPE='S'>lower</USERDEF>",
        "<USERDEF FIELDID='1'>X</USERDEF>",
        "<USERDEF FIELDID='1' TYPE='S'>X</USERDEF><USERDEF FIELDID='01' TYPE='S'>Y</USERDEF>",
        "<USERDEF FIELDID='1' TYPE='S'>X</USERDEF><USERDEF FIELDID='2' TYPE='S'>X</USERDEF>",
    ] {
        let input = format!("<ADX><HEADER>{declaration}</HEADER><RECORDS/></ADX>");

        let mut owner = StrictParser::new(input.as_bytes());
        assert!(
            super::super::strict_sections(&mut owner)
                .next()
                .unwrap()
                .is_err(),
            "{declaration}"
        );
    }

    for (decl, record, valid) in [
        (
            "<USERDEF FIELDID='1' TYPE='N' RANGE='{5:20}'>SCORE</USERDEF>",
            "<USERDEF FIELDNAME='score' TYPE='garbage'>12.5</USERDEF>",
            true,
        ),
        (
            "<USERDEF FIELDID='1' TYPE='N' RANGE='{5:20}'>SCORE</USERDEF>",
            "<USERDEF FIELDNAME='SCORE'>21</USERDEF>",
            false,
        ),
        (
            "<USERDEF FIELDID='1' TYPE='E' ENUM='{A,B}'>SIZE</USERDEF>",
            "<USERDEF FIELDNAME='SIZE'>b</USERDEF>",
            true,
        ),
        (
            "<USERDEF FIELDID='1' TYPE='E' ENUM='{A,B}'>SIZE</USERDEF>",
            "<USERDEF FIELDNAME='SIZE'>c</USERDEF>",
            false,
        ),
        ("", "<USERDEF FIELDNAME='SCORE'>1</USERDEF>", false),
    ] {
        let input = format!(
            "<ADX><HEADER>{decl}</HEADER><RECORDS><RECORD>{record}</RECORD></RECORDS></ADX>"
        );
        let mut owner = StrictParser::new(input.as_bytes());
        {
            let mut p = super::super::strict_sections(&mut owner);
            p.next().unwrap().unwrap();
        }

        let mut p = super::super::strict_sections(&mut owner);
        let result = p.next().unwrap();

        assert_eq!(result.is_ok(), valid, "{result:?}");
        if valid {
            let Section::EndRecord(r) = result.unwrap().data else {
                panic!()
            };

            assert_eq!(
                r.fields()[0].supplied_type(),
                if record.contains("garbage") {
                    Some("garbage")
                } else {
                    None
                }
            );
        }
    }
}

#[test]
fn duplicate_identifiers_use_numeric_identity_and_original_locations() {
    let input = b"<ADX><HEADER><USERDEF FIELDID='123456789012345678901234567890' TYPE='S'>FIRST</USERDEF><USERDEF FIELDID='0&#49;23456789012345678901234567890' TYPE='S'>SECOND</USERDEF></HEADER><RECORDS/></ADX>";
    let mut owner = StrictParser::new(input);
    let mut parser = owner.into_iter();
    let failure = parser.by_ref().find_map(Result::err).unwrap();

    assert!(matches!(
        failure.error,
        ParseError::Declaration {
            reason: DeclarationError::Duplicate { .. },
            ..
        }
    ));
    assert_eq!(
        &input[failure.error.span()],
        b"0&#49;23456789012345678901234567890"
    );
    assert_eq!(failure.error.related_spans().count(), 1);
    assert_eq!(
        &input[failure.error.related_spans().next().unwrap().clone()],
        b"123456789012345678901234567890"
    );
    assert!(parser.next().is_none());
}
