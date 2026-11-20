use std::{
    cell::RefCell,
    io::{self, Write},
    rc::Rc,
};

use qso_io::adif::{
    DataType, StandardField, UserFieldDefinition,
    adi::{
        parser::{options::ParseOpts, strict::StrictParser},
        writer::{Header, HeaderField, Record, RecordField, RecordFieldName, WriteError, Writer},
    },
    value::UserDefinitionConstraint,
};

fn standard(field: StandardField, value: &str) -> RecordField<'_> {
    RecordField {
        name: RecordFieldName::Standard(field),
        value,
    }
}

fn application<'a>(
    name: &'static str,
    data_type: Option<DataType>,
    value: &'a str,
) -> RecordField<'a> {
    RecordField {
        name: RecordFieldName::ApplicationDefined {
            program_id: "EXAMPLE",
            field_name: name,
            data_type,
        },
        value,
    }
}

fn user<'a>(name: &'a str, value: &'a str) -> RecordField<'a> {
    RecordField {
        name: RecordFieldName::UserDefined { name },
        value,
    }
}

fn declaration<'a>(
    name: &'a str,
    data_type: Option<DataType>,
    constraint: Option<&'a UserDefinitionConstraint>,
) -> HeaderField<'a> {
    HeaderField::UserDefined(UserFieldDefinition {
        name,
        data_type,
        constraint,
    })
}

fn header<'a>(fields: &'a [HeaderField<'a>]) -> Header<'a> {
    Header {
        preamble: "Example ADI export",
        fields,
    }
}

fn call() -> RecordField<'static> {
    standard(StandardField::Call, "W7HEX")
}

#[test]
fn emits_standard_sections() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = Writer::new(
        Vec::new(),
        Some(header(&[HeaderField::Standard {
            field: StandardField::AdifVer,
            value: "3.1.8",
        }])),
    )?;
    writer.write_record(Record {
        fields: &[standard(StandardField::Notes, "One<EOR>\r\nTwo")],
    })?;
    let bytes = writer.finish()?;
    assert_eq!(bytes, b"Example ADI export\r\n<ADIF_VER:5>3.1.8\r\n<EOH>\r\n<NOTES:13>One<EOR>\r\nTwo\r\n<EOR>\r\n");

    let mut parser = StrictParser::new(&bytes, ParseOpts::default());
    let parsed = super::support::parse_file(&mut parser);
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(
        super::support::named(&parsed.records[0], "NOTES").text(),
        "One<EOR>\r\nTwo"
    );
    Ok(())
}

#[test]
fn emits_headerless_record() -> Result<(), WriteError> {
    let mut writer = Writer::new(Vec::new(), None)?;
    writer.write_record(Record {
        fields: &[
            standard(StandardField::Mode, "FUTURE-DIGITAL"),
            standard(StandardField::Comment, ""),
        ],
    })?;
    assert_eq!(
        writer.finish()?,
        b"<MODE:14>FUTURE-DIGITAL\r\n<COMMENT:0>\r\n<EOR>\r\n"
    );
    Ok(())
}

#[test]
fn validates_sections_before_writing() -> Result<(), WriteError> {
    for preamble in ["", "Café", "x<EOH>", "x\n"] {
        let mut output = Vec::new();
        assert!(
            Writer::new(
                &mut output,
                Some(Header {
                    preamble,
                    fields: &[]
                })
            )
            .is_err()
        );
        assert!(output.is_empty());
    }

    let version = || HeaderField::Standard {
        field: StandardField::AdifVer,
        value: "3.1.8",
    };
    assert!(Writer::new(Vec::new(), Some(header(&[version(), version()]))).is_err());
    assert!(
        Writer::new(
            Vec::new(),
            Some(header(&[HeaderField::Standard {
                field: StandardField::Call,
                value: "W7HEX",
            }]))
        )
        .is_err()
    );

    let mut writer = Writer::new(Vec::new(), Some(header(&[])))?;
    assert!(writer.write_record(Record { fields: &[] }).is_err());
    assert!(
        writer
            .write_record(Record {
                fields: &[call(), call()]
            })
            .is_err()
    );
    assert!(
        writer
            .write_record(Record {
                fields: &[standard(StandardField::ProgramId, "TEST")]
            })
            .is_err()
    );
    assert!(writer.finish().is_err());

    let mut writer = Writer::new(Vec::new(), Some(header(&[])))?;
    writer.write_record(Record { fields: &[call()] })?;
    assert_eq!(
        writer.finish()?,
        b"Example ADI export\r\n<EOH>\r\n<CALL:5>W7HEX\r\n<EOR>\r\n"
    );
    Ok(())
}

#[test]
fn retries_after_value_validation() -> Result<(), WriteError> {
    let mut writer = Writer::new(Vec::new(), None)?;
    for value in ["x\r\ny", "x\t", "x\ry", "x\ny", "xé"] {
        let result = writer.write_record(Record {
            fields: &[call(), standard(StandardField::Comment, value)],
        });
        assert!(
            matches!(result, Err(WriteError::InvalidCharacter { ref field, offset: 1 }) if field == "COMMENT"),
            "{result:?}"
        );
    }
    assert!(
        writer
            .write_record(Record {
                fields: &[standard(StandardField::CommentIntl, "ASCII")]
            })
            .is_err()
    );
    assert!(
        writer
            .write_record(Record {
                fields: &[standard(StandardField::Notes, "x\ny")]
            })
            .is_err()
    );
    writer.write_record(Record { fields: &[call()] })?;
    assert_eq!(writer.finish()?, b"<CALL:5>W7HEX\r\n<EOR>\r\n");
    Ok(())
}

#[derive(Default)]
struct SinkState {
    bytes: Vec<u8>,
    remaining: Option<usize>,
    writes: usize,
    flushes: usize,
    fail_flush: bool,
}

struct Sink(Rc<RefCell<SinkState>>);

impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self.0.borrow_mut();
        state.writes += 1;
        let length = if let Some(remaining) = &mut state.remaining {
            if *remaining == 0 {
                return Err(io::Error::other("injected write failure"));
            }
            let length = bytes.len().min(*remaining);
            *remaining -= length;
            length
        } else {
            bytes.len()
        };
        state.bytes.extend_from_slice(&bytes[..length]);
        Ok(length)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        state.flushes += 1;
        if state.fail_flush {
            return Err(io::Error::other("injected flush failure"));
        }
        Ok(())
    }
}

#[test]
fn reports_terminal_io_failure() -> Result<(), WriteError> {
    let state = Rc::new(RefCell::new(SinkState::default()));
    let record = || Record {
        fields: &[RecordField {
            name: RecordFieldName::Standard(StandardField::Call),
            value: "W7HEX",
        }],
    };
    let mut writer = Writer::new(Sink(state.clone()), None)?;
    writer.write_record(record())?;
    state.borrow_mut().remaining = Some(4);
    assert!(matches!(
        writer.write_record(record()),
        Err(WriteError::Io(_))
    ));
    let writes = state.borrow().writes;
    assert!(matches!(
        writer.write_record(record()),
        Err(WriteError::Terminal)
    ));
    assert!(matches!(writer.finish(), Err(WriteError::Terminal)));
    assert_eq!(state.borrow().bytes, b"<CALL:5>W7HEX\r\n<EOR>\r\n<CAL");
    assert_eq!(state.borrow().writes, writes);
    assert_eq!(state.borrow().flushes, 0);

    state.borrow_mut().remaining = Some(0);
    assert!(matches!(
        Writer::new(Sink(state.clone()), Some(header(&[]))),
        Err(WriteError::Io(_))
    ));

    state.borrow_mut().remaining = None;
    state.borrow_mut().fail_flush = true;
    let mut writer = Writer::new(Sink(state.clone()), None)?;
    writer.write_record(record())?;
    assert!(matches!(writer.finish(), Err(WriteError::Io(_))));
    assert_eq!(state.borrow().flushes, 1);
    Ok(())
}

#[test]
fn emits_all_field_kinds_and_shares_parsed_declaration() -> Result<(), Box<dyn std::error::Error>> {
    let codes = UserDefinitionConstraint::Enumeration(vec!["A".into(), "B".into()]);
    let mut writer = Writer::new(
        Vec::new(),
        Some(header(&[
            declaration("EPC", Some(DataType::Number), None),
            HeaderField::Standard {
                field: StandardField::ProgramId,
                value: "EXAMPLE",
            },
            declaration("Code", Some(DataType::Enumeration), Some(&codes)),
        ])),
    )?;
    writer.write_record(Record {
        fields: &[
            call(),
            application("SESSION", Some(DataType::String), "hill-1"),
            user("epc", "217"),
        ],
    })?;
    let bytes = writer.finish()?;
    assert_eq!(bytes, b"Example ADI export\r\n<USERDEF1:3:N>EPC\r\n<PROGRAMID:7>EXAMPLE\r\n<USERDEF2:10:E>Code,{A,B}\r\n<EOH>\r\n<CALL:5>W7HEX\r\n<APP_EXAMPLE_SESSION:6:S>hill-1\r\n<epc:3>217\r\n<EOR>\r\n");
    let mut parser = StrictParser::new(&bytes, ParseOpts::default());
    let parsed = super::support::parse_file(&mut parser);
    let parsed_header = parsed.header.expect("output includes an explicit header");
    let code = super::support::definition(&parsed_header.entries()[2]).definition();
    assert_eq!(code.name, "Code");
    assert_eq!(code.data_type, Some(DataType::Enumeration));
    assert_eq!(code.constraint, Some(&codes));
    assert_eq!(
        super::support::named(&parsed.records[0], "EPC").type_indicator(),
        None
    );
    Ok(())
}

#[test]
fn rejects_unrepresentable_declarations() -> Result<(), Box<dyn std::error::Error>> {
    let range = UserDefinitionConstraint::Range {
        lower: 5.into(),
        upper: 20.into(),
    };
    let inverted = UserDefinitionConstraint::Range {
        lower: 20.into(),
        upper: 5.into(),
    };
    let ambiguous = UserDefinitionConstraint::Enumeration(vec!["5:20".into()]);
    let too_large = UserDefinitionConstraint::Range {
        lower: 0.into(),
        upper: bigdecimal::BigDecimal::new(1.into(), i64::MIN),
    };
    for invalid in [
        declaration("CALL", None, None),
        declaration("USERDEF2", None, None),
        declaration("BAD,NAME", None, None),
        declaration("BAD", Some(DataType::IntlString), None),
        declaration("BAD", None, Some(&inverted)),
        declaration("BAD", Some(DataType::Enumeration), Some(&range)),
        declaration("BAD", None, Some(&ambiguous)),
        declaration("BAD", None, Some(&too_large)),
    ] {
        let mut output = Vec::new();
        assert!(Writer::new(&mut output, Some(header(&[invalid]))).is_err());
        assert!(output.is_empty());
    }
    assert!(
        Writer::new(
            Vec::new(),
            Some(header(&[
                declaration("EPC", None, None),
                declaration("epc", None, None)
            ]))
        )
        .is_err()
    );
    let mut writer = Writer::new(Vec::new(), None)?;
    assert!(
        writer
            .write_record(Record {
                fields: &[user("EPC", "1")]
            })
            .is_err()
    );
    Ok(())
}

#[test]
fn enforces_user_constraints() -> Result<(), Box<dyn std::error::Error>> {
    let lower = "1.0000000000000000001";
    let upper = "1.0000000000000000002";
    let range = UserDefinitionConstraint::Range {
        lower: lower.parse()?,
        upper: upper.parse()?,
    };
    let enum_values = UserDefinitionConstraint::Enumeration(vec!["A".into(), "B".into()]);
    let mut writer = Writer::new(
        Vec::new(),
        Some(header(&[
            declaration("Score", None, Some(&range)),
            declaration("Code", Some(DataType::Enumeration), Some(&enum_values)),
        ])),
    )?;
    for (score, code) in [(lower, "a"), (upper, "")] {
        writer.write_record(Record {
            fields: &[user("Score", score), user("Code", code)],
        })?;
    }
    for (field, invalid) in [
        ("Score", "1.0000000000000000003"),
        ("Score", "1e0"),
        ("Code", "C"),
    ] {
        assert!(
            writer
                .write_record(Record {
                    fields: &[user(field, invalid)]
                })
                .is_err()
        );
    }
    let bytes = writer.finish()?;
    let mut parser = StrictParser::new(&bytes, ParseOpts::default());
    let parsed = super::support::parse_file(&mut parser);
    let parsed_header = parsed
        .header
        .expect("output includes the untyped range declaration");
    let definition = super::support::definition(&parsed_header.entries()[0]);
    assert_eq!(definition.definition().data_type, None);
    assert_eq!(definition.definition().constraint, Some(&range));
    assert_eq!(parsed.records.len(), 2);
    Ok(())
}

#[test]
fn validates_untyped_user_text() -> Result<(), WriteError> {
    let mut writer = Writer::new(
        Vec::new(),
        Some(header(&[
            declaration("Free", None, None),
            declaration("Line", Some(DataType::String), None),
        ])),
    )?;
    for (field, invalid) in [("Free", "a\nb"), ("Line", "a\r\nb")] {
        assert!(
            writer
                .write_record(Record {
                    fields: &[user(field, invalid)]
                })
                .is_err()
        );
    }
    writer.write_record(Record {
        fields: &[user("Free", "a\r\nb")],
    })?;
    assert!(writer.finish()?.ends_with(b"<Free:4>a\r\nb\r\n<EOR>\r\n"));
    Ok(())
}

#[test]
fn inherits_application_type() -> Result<(), WriteError> {
    let mut writer = Writer::new(Vec::new(), None)?;
    writer.write_record(Record {
        fields: &[
            application("NOTE", None, "a\r\nb"),
            application("SCORE", Some(DataType::Number), "7"),
        ],
    })?;
    writer.write_record(Record {
        fields: &[
            application("NOTE", None, "c\r\nd"),
            application("SCORE", None, "8"),
        ],
    })?;
    assert!(
        writer
            .write_record(Record {
                fields: &[application("score", Some(DataType::String), "9")]
            })
            .is_err()
    );
    assert!(
        writer
            .write_record(Record {
                fields: &[application("SCORE", None, "9\r\n")]
            })
            .is_err()
    );
    assert_eq!(writer.finish()?, b"<APP_EXAMPLE_NOTE:4>a\r\nb\r\n<APP_EXAMPLE_SCORE:1:N>7\r\n<EOR>\r\n<APP_EXAMPLE_NOTE:4>c\r\nd\r\n<APP_EXAMPLE_SCORE:1:N>8\r\n<EOR>\r\n");
    Ok(())
}

#[test]
fn validates_application_identity() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = Writer::new(
        Vec::new(),
        Some(header(&[declaration("APP_EXAMPLE_SCORE", None, None)])),
    )?;
    writer.write_record(Record {
        fields: &[user("APP_EXAMPLE_SCORE", "original")],
    })?;
    for (name, kind) in [
        ("BAD:NAME", None),
        ("", None),
        ("score", None),
        ("INT", Some(DataType::Integer)),
    ] {
        assert!(
            writer
                .write_record(Record {
                    fields: &[application(name, kind, "new")]
                })
                .is_err()
        );
    }
    let bytes = writer.finish()?;
    let mut parser = StrictParser::new(&bytes, ParseOpts::default());
    let parsed = super::support::parse_file(&mut parser);
    assert_eq!(parsed.records.len(), 1);
    let field = super::support::named(&parsed.records[0], "APP_EXAMPLE_SCORE");
    assert_eq!(field.data_type(), None);
    assert_eq!(field.text(), "original");
    Ok(())
}

#[test]
fn rejected_record_does_not_establish_type() -> Result<(), WriteError> {
    let mut writer = Writer::new(Vec::new(), None)?;
    assert!(
        writer
            .write_record(Record {
                fields: &[
                    application("SCORE", Some(DataType::Number), "7"),
                    standard(StandardField::Comment, "x\n"),
                ]
            })
            .is_err()
    );
    writer.write_record(Record {
        fields: &[application("SCORE", Some(DataType::String), "ok")],
    })?;
    assert_eq!(writer.finish()?, b"<APP_EXAMPLE_SCORE:2:S>ok\r\n<EOR>\r\n");
    Ok(())
}

#[test]
fn application_name_uses_one_program_separator() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = Writer::new(Vec::new(), None)?;
    for program_id in ["_EXAMPLE", "EXAMPLE_LOG"] {
        let result = writer.write_record(Record {
            fields: &[RecordField {
                name: RecordFieldName::ApplicationDefined {
                    program_id,
                    field_name: "NOTE",
                    data_type: None,
                },
                value: "first\r\nsecond",
            }],
        });
        assert!(matches!(result, Err(WriteError::InvalidFieldName { .. })));
    }
    writer.write_record(Record {
        fields: &[application("LOG_NOTE", None, "first\r\nsecond")],
    })?;
    let bytes = writer.finish()?;
    assert_eq!(
        bytes,
        b"<APP_EXAMPLE_LOG_NOTE:13>first\r\nsecond\r\n<EOR>\r\n"
    );

    let mut parser = StrictParser::new(&bytes, ParseOpts::default());
    let parsed = super::support::parse_file(&mut parser);
    let field = super::support::named(&parsed.records[0], "APP_EXAMPLE_LOG_NOTE");
    assert_eq!(field.data_type(), Some(DataType::MultilineString));
    assert_eq!(field.text(), "first\r\nsecond");
    Ok(())
}

#[test]
fn reports_invalid_enumeration_member_index() {
    let constraint = UserDefinitionConstraint::Enumeration(vec!["A".into(), "abcé".into()]);
    let declarations = [declaration(
        "Code",
        Some(DataType::Enumeration),
        Some(&constraint),
    )];
    let Err(error) = Writer::new(Vec::new(), Some(header(&declarations))) else {
        panic!("the second enumeration member contains a non-ASCII character");
    };
    assert!(
        matches!(&error, WriteError::InvalidEnumerationCharacter {
            field,
            member_index: 1,
            offset: 3,
        } if field == "Code"),
        "{error:?}",
    );
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("\"Code\""), "{diagnostic}");
    assert!(diagnostic.contains("member index 1"), "{diagnostic}");
    assert!(diagnostic.contains("byte 3"), "{diagnostic}");
}
