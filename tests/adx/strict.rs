mod applications;
mod declarations;
mod values;

#[test]
fn flat_events_continue_past_ignored_records_and_report_trailing_failure() {
    use qso_io::adif::adx::parser::{
        error::ParseError,
        strict::{Section, StrictParser},
    };

    let input =
        b"<ADX><HEADER/><RECORDS><RECORD/><RECORD><CALL>W1AW</CALL></RECORD></RECORDS></ADX>oops";
    let mut owner = StrictParser::new(input);
    let mut parser = owner.into_iter();
    let mut boundaries = Vec::new();

    for outcome in parser.by_ref() {
        let result = match outcome {
            Ok(result) => result,
            Err(failure) => {
                assert!(matches!(failure.error, ParseError::Xml { .. }));
                assert!(failure.warnings.is_empty());
                assert_eq!(
                    boundaries,
                    [
                        "ADX", "HEADER", "/HEADER", "RECORDS", "RECORD", "/RECORD", "RECORD",
                        "/RECORD", "/RECORDS", "/ADX"
                    ]
                );
                assert!(parser.next().is_none());
                return;
            }
        };

        assert!(result.warnings.is_empty());
        boundaries.push(match result.data {
            Section::StartAdx(_) => "ADX",
            Section::StartHeader(_) => "HEADER",
            Section::EndHeader(_) => "/HEADER",
            Section::StartRecords(_) => "RECORDS",
            Section::StartRecord(_) => "RECORD",
            Section::EndRecord(_) => "/RECORD",
            Section::EndRecords(_) => "/RECORDS",
            Section::EndAdx(_) => "/ADX",
        });
    }

    panic!("trailing input must fail after EndAdx");
}

#[test]
fn successful_exhaustion_stays_complete_across_event_cursors() {
    use qso_io::adif::adx::parser::strict::{Section, StrictParser};

    let mut parser = StrictParser::new(b"<ADX><HEADER/><RECORDS/></ADX>");
    let mut events = parser.into_iter();
    let outcomes = events.by_ref().collect::<Result<Vec<_>, _>>().unwrap();

    assert!(matches!(outcomes.last().unwrap().data, Section::EndAdx(_)));
    assert!(events.next().is_none());
    drop(events);
    assert!(parser.into_iter().next().is_none());
}
