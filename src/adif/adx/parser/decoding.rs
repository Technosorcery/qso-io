use std::{borrow::Cow, ops::Range};

use quick_xml::{escape::resolve_xml_entity, events::BytesRef};
use xmlparser::XmlCharExt;

use super::error::{ParseError, TextError, WarningKind};

pub(crate) fn name(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_xml_name_start()) && chars.all(|c| c.is_xml_name())
}

pub(crate) fn reference<'a>(
    s: &str,
    span: Range<usize>,
) -> Result<Option<Cow<'static, str>>, ParseError<'a>> {
    let r = BytesRef::new(s);
    if let Some(c) = r.resolve_char_ref().map_err(|e| ParseError::Xml {
        span: span.clone(),
        message: e.to_string().into(),
        source: None,
    })? {
        if !c.is_xml_char() {
            return Err(ParseError::Xml {
                span,
                message: "illegal XML character reference".into(),
                source: None,
            });
        }
        return Ok(Some(Cow::Owned(c.to_string())));
    }

    if !name(s) {
        return Err(ParseError::Xml {
            span,
            message: "invalid reference name".into(),
            source: None,
        });
    }

    Ok(resolve_xml_entity(s).map(Cow::Borrowed))
}

pub(crate) fn check_chars<'a>(s: &str, start: usize) -> Result<(), ParseError<'a>> {
    for (i, c) in s.char_indices() {
        if !c.is_xml_char() {
            return Err(ParseError::Xml {
                span: start + i..start + i + c.len_utf8(),
                message: "illegal XML character".into(),
                source: None,
            });
        }
    }

    Ok(())
}

pub(crate) fn check_attribute<'a>(
    raw: &'a str,
    start: usize,
    mut custom: impl FnMut(&'a str, Range<usize>) -> Result<(), ParseError<'a>>,
) -> Result<Option<TextError<'a>>, ParseError<'a>> {
    let mut unsupported = None;
    let mut chars = raw.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        match c {
            '<' => {
                return Err(ParseError::Xml {
                    span: start + i..start + i + 1,
                    message: "literal < in attribute".into(),
                    source: None,
                });
            }
            '&' => {
                let end =
                    raw[i + 1..]
                        .find(';')
                        .map(|j| i + j + 2)
                        .ok_or_else(|| ParseError::Xml {
                            span: start + i..start + raw.len(),
                            message: "unterminated reference".into(),
                            source: None,
                        })?;
                let n = &raw[i + 1..end - 1];
                let span = start + i..start + end;
                if reference(n, span.clone())?.is_none() {
                    custom(n, span)?;
                    unsupported.get_or_insert(TextError::CustomReference(n));
                }
                while chars.next_if(|(j, _)| *j < end).is_some() {}
            }
            _ => {
                if !c.is_xml_char() {
                    return Err(ParseError::Xml {
                        span: start + i..start + i + c.len_utf8(),
                        message: "illegal XML character".into(),
                        source: None,
                    });
                }
            }
        }
    }

    Ok(unsupported)
}

pub(crate) fn attribute<'a>(
    raw: &'a str,
    start: usize,
    custom: impl FnMut(&'a str, Range<usize>) -> Result<(), ParseError<'a>>,
) -> Result<Result<Cow<'a, str>, TextError<'a>>, ParseError<'a>> {
    if let Some(e) = check_attribute(raw, start, custom)? {
        return Ok(Err(e));
    }
    normalize_attribute(raw, start).map(Ok)
}

fn normalize_attribute<'a>(raw: &'a str, start: usize) -> Result<Cow<'a, str>, ParseError<'a>> {
    let attr = quick_xml::events::attributes::Attribute {
        key: quick_xml::name::QName("value"),
        value: Cow::Borrowed(raw),
    };

    let decoded = attr
        .normalized_value_with(quick_xml::XmlVersion::Implicit1_0, 1, resolve_xml_entity)
        .map_err(|e| ParseError::Xml {
            span: start..start + raw.len(),
            message: e.to_string().into(),
            source: None,
        })?;
    Ok(decoded)
}

#[derive(Clone, Copy)]
pub(crate) enum TextContext {
    Content,
    Attribute,
}

fn literal(
    text: &str,
    base: usize,
    context: TextContext,
    emit: &mut impl FnMut(&str, Range<usize>),
) {
    let mut it = text.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let mut end = i + c.len_utf8();
        let mut c = c;
        if c == '\r' {
            if let Some((next, _)) = it.next_if(|(_, c)| *c == '\n') {
                end = next + 1;
            }
            c = '\n';
        }

        if matches!(context, TextContext::Attribute) && matches!(c, '\t' | '\n') {
            c = ' ';
        }

        emit(c.encode_utf8(&mut [0; 4]), base + i..base + end);
    }
}

/// Maps a byte range in the decoded XML text to a byte range in the parser's
/// input.
///
/// `decoded` addresses the text obtained by decoding `raw` in `context`.
/// `source_start` is the byte offset of `raw` in the parser's input. The
/// returned range uses that input's offsets. Both ranges have exclusive ends.
pub(crate) fn locate_text_range<'a>(
    raw: &'a str,
    source_start: usize,
    decoded: Range<usize>,
    context: TextContext,
) -> Result<Range<usize>, ParseError<'a>> {
    let source_end = source_start + raw.len();

    let mut position = 0;
    let mut first = None;
    let mut last = source_end;
    let mut emit = |text: &str, source: Range<usize>| {
        let end = position + text.len();
        if (position < decoded.end && end > decoded.start)
            || (decoded.is_empty() && position <= decoded.start && decoded.start < end)
        {
            first.get_or_insert(source.start);
            last = source.end;
        }
        position = end;
    };

    if matches!(context, TextContext::Attribute) {
        let mut i = 0;

        while i < raw.len() {
            if raw.as_bytes()[i] == b'&' {
                let end = raw[i..]
                    .find(';')
                    .map(|j| i + j + 1)
                    .ok_or_else(|| ParseError::Xml {
                        span: source_start + i..source_end,
                        message: "unterminated reference".into(),
                        source: None,
                    })?;
                let source = source_start + i..source_start + end;
                let name = &raw[i + 1..end - 1];
                match reference(name, source.clone())? {
                    Some(text) => emit(&text, source),
                    None => {
                        return Err(ParseError::Prohibited {
                            span: source,
                            construct: WarningKind::CustomReference(name),
                        });
                    }
                }
                i = end;
            } else {
                let end = raw[i..].find('&').map_or(raw.len(), |j| i + j);
                literal(&raw[i..end], source_start + i, context, &mut emit);
                i = end;
            }
        }
    } else {
        // Keep leading U+FEFF characters as field text. The fragment reader
        // would otherwise remove the first one as a document BOM.
        let tail = raw.trim_start_matches('\u{feff}');
        let prefix = raw.len() - tail.len();
        literal(&raw[..prefix], source_start, context, &mut emit);
        let mut reader = quick_xml::Reader::from_str(tail);

        loop {
            let start = reader.buffer_position() as usize + prefix;
            let event = reader.read_event().map_err(|e| ParseError::Xml {
                span: source_start + start
                    ..source_start + prefix + reader.buffer_position() as usize,
                message: "XML reader error".into(),
                source: Some(Box::new(e)),
            })?;
            let end = reader.buffer_position() as usize + prefix;
            match event {
                quick_xml::events::Event::Text(t) => {
                    literal(&t, source_start + start, context, &mut emit)
                }
                quick_xml::events::Event::GeneralRef(r) => {
                    let source = source_start + start..source_start + end;
                    match reference(&r, source.clone())? {
                        Some(text) => emit(&text, source),
                        None => {
                            return Err(ParseError::Prohibited {
                                span: source,
                                construct: WarningKind::CustomReference(&raw[start + 1..end - 1]),
                            });
                        }
                    }
                }
                quick_xml::events::Event::Eof => break,
                _ => {}
            }
        }
    }
    Ok(first.map_or(source_end..source_end, |start| start..last))
}
