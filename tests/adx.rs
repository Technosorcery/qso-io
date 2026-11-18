mod adx {
    use qso_io::adif::adx::parser::{
        ParseFailure, ParseResult,
        permissive::{PermissiveParser, PermissiveSection},
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

    mod framing;
    mod lookup;
    mod permissive;
}
