use super::*;
use qso_io::adif::value::Value;

fn document(name: &str, raw: &[u8]) -> Vec<u8> {
    let mut bytes = format!("<{name}:{}>", raw.len()).into_bytes();
    bytes.extend_from_slice(raw);
    bytes.extend_from_slice(b"<EOR>");
    bytes
}

#[test]
fn list_members_keep_top_level_boundaries_and_spelling() {
    for (name, text, expected) in [
        (
            "CREDIT_GRANTED",
            "IOTA,WAS:LOTW&CARD,DXCC:CARD",
            vec!["IOTA", "WAS:LOTW&CARD", "DXCC:CARD"],
        ),
        (
            "USACA_COUNTIES",
            "MA,Franklin:MA,Hampshire",
            vec!["MA,Franklin", "MA,Hampshire"],
        ),
        (
            "AWARD_GRANTED",
            "aRrL_PROGRAM_AWARD,cq_OTHER_UNKNOWN",
            vec!["aRrL_PROGRAM_AWARD", "cq_OTHER_UNKNOWN"],
        ),
        ("VUCC_GRIDS", "EM98,fm08", vec!["EM98", "fm08"]),
        ("POTA_REF", "K-4562@US-CA", vec!["K-4562@US-CA"]),
    ] {
        let bytes = document(name, text.as_bytes());
        let mut owner = StrictParser::new(
            &bytes,
            ParseOpts {
                encoding: ParseEncoding::Ascii,
                field_length: LengthUnit::Bytes,
            },
        );
        let field =
            parsed_field(&mut owner, name).unwrap_or_else(|e| panic!("{name:?} {text}: {e}"));
        let Some(Value::List(items)) = field.value() else {
            panic!("expected List, got {field:?}");
        };
        assert_eq!(items, &expected, "{name:?}");
        assert_eq!(field.text(), text);
        assert_eq!(field.raw_data(), text.as_bytes());
    }
}

#[test]
fn list_member_checks_obey_adif_source_coverage() {
    for (name, text, bad) in [
        ("CREDIT_GRANTED", "IOTA,WAS:NOPE", "NOPE"),
        ("CREDIT_GRANTED", "NOPE", "NOPE"),
        ("USACA_COUNTIES", "AK,Imaginary:MA,Franklin", "AK,Imaginary"),
        (
            "CNTY_ALT",
            "NZ_Regions:Hawkes Bay/Wairoa;NZ_Regions:Hawkes Bay/Wairoa",
            "NZ_Regions:Hawkes Bay/Wairoa",
        ),
        ("CNTY_ALT", "NZ_Regions:Imaginary", "NZ_Regions:Imaginary"),
        ("AWARD_GRANTED", "NOPE_PROGRAM_AWARD", "NOPE_"),
        ("AWARD_GRANTED", "ARRL__AWARD", "ARRL__AWARD"),
        ("AWARD_GRANTED", "ARRL_PROGRAM_", "ARRL_PROGRAM_"),
        ("AWARD_GRANTED", "ARRL_PROGRAM", "ARRL_PROGRAM"),
        ("VUCC_GRIDS", "EM98,", ""),
    ] {
        let bytes = document(name, text.as_bytes());
        let mut owner = StrictParser::new(
            &bytes,
            ParseOpts {
                encoding: ParseEncoding::Ascii,
                field_length: LengthUnit::Bytes,
            },
        );
        let error = parsed_field(&mut owner, name).expect_err("invalid list should fail");
        assert_eq!(
            &bytes[error.span()],
            bad.as_bytes(),
            "{name:?} {text}: {error:?}"
        );
    }
    for text in ["NZ_Regions:Hawkes Bay/Wairoa", "OTHER:Unlisted"] {
        let bytes = document("CNTY_ALT", text.as_bytes());
        let mut owner = StrictParser::new(
            &bytes,
            ParseOpts {
                encoding: ParseEncoding::Ascii,
                field_length: LengthUnit::Bytes,
            },
        );
        parsed_field(&mut owner, "CNTY_ALT")
            .unwrap_or_else(|e| panic!("source-bounded {text}: {e}"));
    }
}

#[test]
fn latin1_list_members_own_their_text_safely() {
    let bytes = document("USACA_COUNTIES", b"MA,Jos\xe9:MA,Franklin");
    let mut owner = StrictParser::new(
        &bytes,
        ParseOpts {
            encoding: ParseEncoding::Latin1,
            field_length: LengthUnit::Bytes,
        },
    );
    let fields = vec![
        parsed_field(&mut owner, "USACA_COUNTIES").unwrap_or_else(|e| panic!("Latin-1 list: {e}")),
    ];
    for field in fields {
        assert_eq!(field.text(), "MA,José:MA,Franklin");
        let Some(Value::List(items)) = field.value() else {
            panic!("expected List, got {field:?}");
        };
        assert_eq!(items, &["MA,José", "MA,Franklin"]);
        assert_eq!(field.raw_data(), b"MA,Jos\xe9:MA,Franklin");
    }
}
