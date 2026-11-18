use std::{borrow::Cow, ops::Range};

use quick_xml::{escape::resolve_xml_entity, events::BytesRef};
use xmlparser::XmlCharExt;

use super::error::{ParseError, TextError};

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
