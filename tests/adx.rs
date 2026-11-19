mod adx {
    use qso_io::adif::adx::parser::{
        ParseFailure, ParseResult,
        permissive::{PermissiveParser, PermissiveSection},
        strict::{Section, StrictParser},
    };

    fn permissive_sections<'a>(
        input: &'a [u8],
    ) -> impl Iterator<Item = Result<ParseResult<'a, PermissiveSection<'a>>, ParseFailure<'a>>>
    {
        PermissiveParser::new(input).filter(|result| {
            matches!(
                result,
                Err(_)
                    | Ok(ParseResult {
                        data: PermissiveSection::EndHeader(_) | PermissiveSection::EndRecord(_),
                        ..
                    })
            )
        })
    }

    fn strict_sections<'source, 'parser>(
        parser: &'parser mut StrictParser<'source>,
    ) -> impl Iterator<
        Item = Result<ParseResult<'source, Section<'source, 'parser>>, ParseFailure<'source>>,
    > {
        parser.into_iter().filter(|result| {
            matches!(
                result,
                Err(_)
                    | Ok(ParseResult {
                        data: Section::EndHeader(_) | Section::EndRecord(_),
                        ..
                    })
            )
        })
    }

    mod framing;
    mod lookup;
    mod permissive;
    mod strict;
}
