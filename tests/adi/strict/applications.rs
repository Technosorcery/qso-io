use super::*;
use qso_io::adif::value::Value;

#[test]
fn application_types_inherit_and_keep_the_first_descriptor() {
    let bytes=b"<APP_EXAMPLE_SCORE:3:N>123<EOR><app_example_score:3>456<EOR><APP_EXAMPLE_SCORE:1:L>X<EOR>";
    let mut parser_owner = StrictParser::new(bytes, ParseOpts::default());
    let mut parser = parser_owner.into_iter();
    for (expected, indicator) in [("123", Some(DataType::Number)), ("456", None)] {
        let section = next_section(&mut parser);
        let Section::Record(record) = section else {
            panic!("expected Number record, got {section:?}");
        };
        let field = named(&record, "APP_EXAMPLE_SCORE");
        assert_eq!(field.data_type(), Some(DataType::Number));
        assert_eq!(field.type_indicator(), indicator);
        assert!(
            matches!(field.value(),Some(Value::Number(n)) if n.to_string()==expected),
            "{field:?}"
        );
    }
    drop(parser);
    let mut parser = parser_owner.into_iter();
    let error = next_error(&mut parser);
    let error::StructuralError::ApplicationTypeConflict {
        first_span: span,
        supplied: DataType::Location,
        ..
    } = &error
    else {
        panic!("expected conflicting Location, got {error:?}");
    };
    assert_eq!(&bytes[span.clone()], b"<APP_EXAMPLE_SCORE:3:N>");
    assert_eq!(&bytes[error.span()], b"<APP_EXAMPLE_SCORE:1:L>");
    assert_exhausted(&mut parser);
    assert_exhausted(&mut parser_owner.into_iter());
}

#[test]
fn application_empty_first_occurrence_establishes_type() {
    for first in ["<APP_EXAMPLE_SCORE:0:N>", "<APP_EXAMPLE_SCORE:0>"] {
        let bytes = format!("{first}<EOR><APP_EXAMPLE_SCORE:1:N>1<EOR>");
        let mut parser_owner = StrictParser::new(bytes.as_bytes(), ParseOpts::default());
        let mut parser = parser_owner.into_iter();
        let section = next_section(&mut parser);
        let Section::Record(record) = section else {
            panic!("expected empty record, got {section:?}");
        };
        assert!(named(&record, "APP_EXAMPLE_SCORE").value().is_none());
        if first.contains(":N>") {
            let section = next_section(&mut parser);
            let Section::Record(record) = section else {
                panic!("expected inherited Number, got {section:?}");
            };
            assert_eq!(
                named(&record, "APP_EXAMPLE_SCORE").data_type(),
                Some(DataType::Number)
            );
        } else {
            let error = next_error(&mut parser);
            assert!(
                matches!(&error,error::StructuralError::ApplicationTypeConflict {expected,..} if *expected==DataType::MultilineString),
                "{error:?}"
            );
        }
    }
}

#[test]
fn application_type_state_is_document_local_and_respects_userdef() {
    let mut first_owner = StrictParser::new(b"<APP_EXAMPLE_SCORE:1:N>1<EOR>", ParseOpts::default());
    let mut first = first_owner.into_iter();
    let _ = next_section(&mut first);
    let mut second_owner = StrictParser::new(b"<APP_EXAMPLE_SCORE:1>A<EOR>", ParseOpts::default());
    let mut second = second_owner.into_iter();
    let section = next_section(&mut second);
    let Section::Record(record) = section else {
        panic!("expected fresh parser record, got {section:?}");
    };
    assert_eq!(
        named(&record, "APP_EXAMPLE_SCORE").data_type(),
        Some(DataType::MultilineString)
    );

    for name in ["APP_EXAMPLE_SCORE", "APP__EXAMPLE_NOTE"] {
        let bytes = format!("P<USERDEF1:17:N>{name}<EOH><{name}:1>1<EOR><{name}:1:S>A<EOR>");
        let mut parser_owner = StrictParser::new(bytes.as_bytes(), ParseOpts::default());
        let mut parser = parser_owner.into_iter();
        drop(next_section(&mut parser));
        let _ = next_section(&mut parser);
        let error = next_error(&mut parser);
        let error::StructuralError::UserTypeConflict {
            declaration_span: span,
            ..
        } = &error
        else {
            panic!("expected USERDEF conflict for {name}, got {error:?}");
        };
        assert_eq!(&bytes.as_bytes()[span.clone()], b"<USERDEF1:17:N>");
    }
}

#[test]
fn rejects_an_application_field_with_an_empty_program_id() {
    let bytes = b"<APP__EXAMPLE_NOTE:1>x<EOR>";
    let mut parser = StrictParser::new(bytes, ParseOpts::default());
    let mut sections = parser.into_iter();

    let error = next_error(&mut sections);
    assert!(matches!(
        error,
        error::StructuralError::Syntax {
            reason: error::SyntaxErrorKind::EmptyApplicationProgramId,
            ..
        }
    ));
    assert_eq!(&bytes[error.span()], b"<APP__EXAMPLE_NOTE:1>");
    assert_exhausted(&mut sections);
}
