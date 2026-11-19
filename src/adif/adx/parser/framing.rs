use std::{borrow::Cow, ops::Range};

use quick_xml::{Reader, events::Event};

use super::{
    ParseFailure, ParseResult, decoding,
    error::{ParseError, ParseWarning, TextError, WarningKind},
    model::{is_namespace, local},
    permissive::{
        PermissiveSection, attribute::Attribute, field::PermissiveField, header::PermissiveHeader,
        record::PermissiveRecord,
    },
};

#[derive(Clone)]
enum Phase {
    Prolog,
    Header,
    HeaderFields { opening: Range<usize> },
    Records,
    Record,
    RecordFields { opening: Range<usize> },
    RootEnd,
    Epilog,
}

pub(crate) struct Sections<'a> {
    input: &'a [u8],
    offset: usize,
    reader: Reader<&'a [u8]>,
    phase: Phase,
    done: bool,
    strict: bool,
    doctype: bool,
}

impl<'a> Sections<'a> {
    pub fn new(input: &'a [u8], strict: bool) -> Self {
        let offset = if input.starts_with(b"\xef\xbb\xbf") {
            3
        } else {
            0
        };

        let mut reader = Reader::from_reader(&input[offset..]);

        reader.config_mut().enable_all_checks(true);
        reader.config_mut().expand_empty_elements = true;

        Self {
            input,
            offset,
            reader,
            phase: Phase::Prolog,
            done: false,
            strict,
            doctype: false,
        }
    }

    fn event(&mut self) -> Result<(Event<'a>, Range<usize>, &'a str), ParseError<'a>> {
        let start = self.reader.buffer_position() as usize + self.offset;
        let event = self.reader.read_event().map_err(|e| {
            let end = self.reader.buffer_position() as usize + self.offset;
            let mut range = start..end;
            if matches!(&e, quick_xml::Error::Encoding(_))
                && let Err(utf8) = std::str::from_utf8(&self.input[range.clone()])
            {
                let invalid = start + utf8.valid_up_to();
                range = invalid..invalid + utf8.error_len().unwrap_or(end - invalid);
            }

            ParseError::Xml {
                span: range,
                message: "XML reader error".into(),
                source: Some(Box::new(e)),
            }
        })?;
        let end = self.reader.buffer_position() as usize + self.offset;
        let span = start..end;
        // Literal XML character checks also apply to retained opaque DTD bytes;
        // they do not interpret or validate any declaration grammar.
        let raw = std::str::from_utf8(&self.input[span.clone()]).map_err(|e| {
            let invalid = start + e.valid_up_to();
            ParseError::Xml {
                span: invalid..invalid + e.error_len().unwrap_or(end - invalid),
                message: e.to_string().into(),
                source: None,
            }
        })?;
        if !matches!(event, Event::Start(_)) {
            decoding::check_chars(raw, start)?;
        }

        match &event {
            Event::Start(e) => {
                if !decoding::name(e.name().0) {
                    return Err(ParseError::Xml {
                        span,
                        message: "invalid XML element name".into(),
                        source: None,
                    });
                }

                let name = local(e.name().0);
                if self.strict && name.chars().flat_map(char::to_uppercase).ne(name.chars()) {
                    return Err(ParseError::Prohibited {
                        span,
                        construct: WarningKind::NameCasing,
                    });
                }
            }
            Event::End(e) if !decoding::name(e.name().0) => {
                return Err(ParseError::Xml {
                    span,
                    message: "invalid end name".into(),
                    source: None,
                });
            }
            Event::Text(t) if t.contains("]]>") => {
                return Err(ParseError::Xml {
                    span,
                    message: "literal ]]> in text".into(),
                    source: None,
                });
            }
            Event::PI(p)
                if !decoding::name(p.target()) || p.target().eq_ignore_ascii_case("xml") =>
            {
                return Err(ParseError::Xml {
                    span,
                    message: "invalid processing instruction target".into(),
                    source: None,
                });
            }
            Event::Decl(_) => {
                if start != self.offset {
                    return Err(ParseError::Xml {
                        span,
                        message: "XML declaration must be first".into(),
                        source: None,
                    });
                }

                let raw = std::str::from_utf8(&self.input[span.clone()]).map_err(|e| {
                    ParseError::Xml {
                        span: span.clone(),
                        message: e.to_string().into(),
                        source: None,
                    }
                })?;
                let mut tokens = xmlparser::Tokenizer::from(raw);
                match tokens.next() {
                    Some(Ok(xmlparser::Token::Declaration {
                        version, encoding, ..
                    })) if version.as_str() == "1.0"
                        && encoding.is_none_or(|e| e.as_str().eq_ignore_ascii_case("UTF-8")) => {}
                    _ => {
                        return Err(ParseError::Xml {
                            span,
                            message: "expected XML 1.0 UTF-8 declaration".into(),
                            source: None,
                        });
                    }
                }

                if tokens.any(|t| t.is_err()) {
                    return Err(ParseError::Xml {
                        span,
                        message: "invalid XML declaration".into(),
                        source: None,
                    });
                }
            }
            Event::DocType(_) => {
                if !matches!(self.phase, Phase::Prolog) || self.doctype {
                    return Err(ParseError::Xml {
                        span,
                        message: "DOCTYPE outside prolog or repeated".into(),
                        source: None,
                    });
                }

                self.doctype = true;
            }
            _ => {}
        }

        Ok((event, span, raw))
    }

    fn advance(
        &mut self,
        warnings: &mut Vec<ParseWarning<'a>>,
    ) -> Result<Option<PermissiveSection<'a>>, ParseError<'a>> {
        match self.phase.clone() {
            Phase::HeaderFields { opening } => {
                let fields = self.fields(warnings, true)?;
                let span = opening.start..self.reader.buffer_position() as usize + self.offset;
                self.phase = Phase::Records;

                return Ok(Some(PermissiveSection::EndHeader(PermissiveHeader {
                    fields,
                    span,
                })));
            }
            Phase::RecordFields { opening } => {
                let fields = self.fields(warnings, false)?;
                let span = opening.start..self.reader.buffer_position() as usize + self.offset;
                self.phase = Phase::Record;

                return Ok(Some(PermissiveSection::EndRecord(PermissiveRecord {
                    fields,
                    span,
                })));
            }
            _ => {}
        }

        loop {
            let (event, span, _) = self.event()?;
            match event {
                Event::Start(e) => {
                    let name = e.name().0;
                    let n = local(name);
                    self.attributes(warnings, &e, span.clone(), false)?;

                    if matches!(self.phase, Phase::Epilog) {
                        return Err(ParseError::Xml {
                            span,
                            message: "multiple XML roots".into(),
                            source: None,
                        });
                    }

                    match (self.phase.clone(), n) {
                        (Phase::Prolog, "ADX") => {
                            self.phase = Phase::Header;
                            return Ok(Some(PermissiveSection::StartAdx(span)));
                        }
                        (Phase::Header, "HEADER") => {
                            self.phase = Phase::HeaderFields {
                                opening: span.clone(),
                            };
                            return Ok(Some(PermissiveSection::StartHeader(span)));
                        }
                        (Phase::Records, "RECORDS") => {
                            self.phase = Phase::Record;
                            return Ok(Some(PermissiveSection::StartRecords(span)));
                        }
                        (Phase::Record, "RECORD") => {
                            self.phase = Phase::RecordFields {
                                opening: span.clone(),
                            };
                            return Ok(Some(PermissiveSection::StartRecord(span)));
                        }
                        _ => {
                            return Err(ParseError::Envelope {
                                span,
                                message: "expected ADX / HEADER / RECORDS / RECORD hierarchy",
                            });
                        }
                    }
                }
                Event::End(_) => {
                    let data = match self.phase.clone() {
                        Phase::Record => {
                            self.phase = Phase::RootEnd;
                            PermissiveSection::EndRecords(span)
                        }
                        Phase::RootEnd => {
                            self.phase = Phase::Epilog;
                            PermissiveSection::EndAdx(span)
                        }
                        _ => {
                            return Err(ParseError::Envelope {
                                span,
                                message: "incomplete ADX envelope",
                            });
                        }
                    };

                    return Ok(Some(data));
                }

                Event::Text(e) if e.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')) => {}
                Event::DocType(e) => {
                    self.issue(warnings, WarningKind::Doctype, span.clone())?;
                    // Inspect literal declaration tokens only. Stop on opaque input;
                    // no declaration registry, defaults or replacement processing.
                    let raw = std::str::from_utf8(&self.input[span.clone()]).map_err(|e| {
                        ParseError::Xml {
                            span: span.clone(),
                            message: e.to_string().into(),
                            source: None,
                        }
                    })?;
                    let _ = e;
                    for token in xmlparser::Tokenizer::from(raw) {
                        match token {
                            Ok(xmlparser::Token::EntityDeclaration {
                                name, span: decl, ..
                            }) => self.issue(
                                warnings,
                                WarningKind::EntityDefinition(name.as_str()),
                                span.start + decl.start()..span.start + decl.end(),
                            )?,
                            Err(_) => break,
                            _ => {}
                        }
                    }
                }
                Event::Comment(_) | Event::PI(_) | Event::Decl(_) => {}
                Event::Eof if matches!(self.phase, Phase::Epilog) => return Ok(None),
                Event::Eof => {
                    return Err(ParseError::Xml {
                        span,
                        message: "incomplete XML document".into(),
                        source: None,
                    });
                }
                _ => {
                    return Err(ParseError::Xml {
                        span,
                        message: "unexpected XML content outside field".into(),
                        source: None,
                    });
                }
            }
        }
    }

    fn issue(
        &self,
        warnings: &mut Vec<ParseWarning<'a>>,
        kind: WarningKind<'a>,
        span: Range<usize>,
    ) -> Result<(), ParseError<'a>> {
        if self.strict {
            return Err(ParseError::Prohibited {
                span,
                construct: kind,
            });
        }

        warnings.push(ParseWarning { kind, span });
        Ok(())
    }

    fn attributes(
        &mut self,
        warnings: &mut Vec<ParseWarning<'a>>,
        e: &quick_xml::events::BytesStart<'_>,
        span: Range<usize>,
        decode: bool,
    ) -> Result<Vec<Attribute<'a>>, ParseError<'a>> {
        let mut attrs = Vec::new();
        let base = e.as_ref().as_ptr() as usize;
        let mut previous = e.name().0.len();
        for a in e.attributes() {
            let a = a.map_err(|err| ParseError::Xml {
                span: span.clone(),
                message: err.to_string().into(),
                source: None,
            })?;
            let key_offset = a.key.0.as_ptr() as usize - base;
            if key_offset == previous
                || !e.as_ref()[previous..key_offset]
                    .bytes()
                    .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            {
                return Err(ParseError::Xml {
                    span: span.start + 1 + key_offset..span.start + 1 + key_offset + a.key.0.len(),
                    message: "XML attributes require separating whitespace".into(),
                    source: None,
                });
            }
            previous = a.value.as_ptr() as usize - base + a.value.len() + 1;
            let ns = span.start + 1 + key_offset;
            if !decoding::name(a.key.0) {
                return Err(ParseError::Xml {
                    span: ns..ns + a.key.0.len(),
                    message: "invalid XML attribute name".into(),
                    source: None,
                });
            }

            let attribute_name = local(a.key.0);
            if self.strict
                && !is_namespace(a.key.0)
                && attribute_name
                    .chars()
                    .flat_map(char::to_uppercase)
                    .ne(attribute_name.chars())
            {
                return Err(ParseError::Prohibited {
                    span: ns..ns + a.key.0.len(),
                    construct: WarningKind::NameCasing,
                });
            }

            let vs = span.start + 1 + (a.value.as_ptr() as usize - base);
            let name = std::str::from_utf8(&self.input[ns..ns + a.key.0.len()]).map_err(|err| {
                ParseError::Xml {
                    span: span.clone(),
                    message: err.to_string().into(),
                    source: None,
                }
            })?;
            let raw = std::str::from_utf8(&self.input[vs..vs + a.value.len()]).map_err(|err| {
                ParseError::Xml {
                    span: span.clone(),
                    message: err.to_string().into(),
                    source: None,
                }
            })?;
            if !decode {
                decoding::check_attribute(raw, vs, |n, s| {
                    self.issue(warnings, WarningKind::CustomReference(n), s)
                })?;
                continue;
            }

            let text = decoding::attribute(raw, vs, |n, s| {
                self.issue(warnings, WarningKind::CustomReference(n), s)
            })?;
            attrs.push(Attribute {
                name,
                raw,
                name_span: ns..ns + name.len(),
                value_span: vs..vs + raw.len(),
                text,
            });
        }

        Ok(attrs)
    }

    fn field(
        &mut self,
        warnings: &mut Vec<ParseWarning<'a>>,
        e: quick_xml::events::BytesStart<'a>,
        span: Range<usize>,
        header: bool,
    ) -> Result<PermissiveField<'a>, ParseError<'a>> {
        let name_span = span.start + 1..span.start + 1 + e.name().0.len();
        let name =
            std::str::from_utf8(&self.input[name_span.clone()]).map_err(|e| ParseError::Xml {
                span: name_span.clone(),
                message: e.to_string().into(),
                source: None,
            })?;
        let attrs = self.attributes(warnings, &e, span.clone(), true)?;
        let required: &[&str] = match local(name) {
            "APP" => &["PROGRAMID", "FIELDNAME"],
            "USERDEF" if header => &["FIELDID"],
            "USERDEF" => &["FIELDNAME"],
            _ => &[],
        };

        for &name in required {
            if attrs
                .iter()
                .find(|a| !is_namespace(a.name()) && local(a.name()) == name)
                .is_none()
            {
                self.issue(
                    warnings,
                    WarningKind::MissingAttribute(name),
                    name_span.clone(),
                )?;
            }
        }

        let start = span.end;
        let end;
        let mut text = Cow::Borrowed("");
        let mut unavailable = None;
        let mut depth = 0;

        loop {
            let (ev, s, raw) = self.event()?;
            match ev {
                Event::End(_) if depth == 0 => {
                    end = s.start;
                    break;
                }
                Event::End(_) => depth -= 1,
                Event::Start(n) => {
                    self.attributes(warnings, &n, s.clone(), true)?;
                    self.issue(warnings, WarningKind::NestedMarkup, s.clone())?;
                    unavailable = Some(TextError::NestedMarkup);
                    depth += 1;
                }
                Event::Text(t) => {
                    let decoded = t.xml10_content();
                    if text.is_empty() {
                        text = decoded;
                    } else {
                        text.to_mut().push_str(&decoded);
                    }
                }
                Event::CData(t) => {
                    self.issue(warnings, WarningKind::Cdata, s)?;
                    let decoded = t.xml10_content();
                    if text.is_empty() {
                        text = decoded;
                    } else {
                        text.to_mut().push_str(&decoded);
                    }
                }
                Event::GeneralRef(r) => {
                    if let Some(decoded) = decoding::reference(&r, s.clone())? {
                        text.to_mut().push_str(&decoded);
                    } else {
                        let name = raw
                            .strip_prefix('&')
                            .and_then(|name| name.strip_suffix(';'))
                            .ok_or_else(|| ParseError::Xml {
                                span: s.clone(),
                                message: "invalid XML reference".into(),
                                source: None,
                            })?;
                        self.issue(warnings, WarningKind::CustomReference(name), s)?;
                        if unavailable.is_none() {
                            unavailable = Some(TextError::CustomReference(name));
                        }
                    }
                }
                Event::Comment(_) | Event::PI(_) => {}
                _ => {
                    return Err(ParseError::Xml {
                        span: s,
                        message: "unexpected field content".into(),
                        source: None,
                    });
                }
            }
        }

        let raw_content =
            std::str::from_utf8(&self.input[start..end]).map_err(|error| ParseError::Xml {
                span: start..end,
                message: error.to_string().into(),
                source: None,
            })?;
        let field = PermissiveField {
            input: self.input,
            name,
            element: span.start..self.reader.buffer_position() as usize + self.offset,
            name_span,
            content: start..end,
            raw_content,
            attrs,
            text: unavailable.map_or(Ok(text), Err),
            header,
        };

        Ok(field)
    }

    fn fields(
        &mut self,
        warnings: &mut Vec<ParseWarning<'a>>,
        header: bool,
    ) -> Result<Vec<PermissiveField<'a>>, ParseError<'a>> {
        let mut fields = Vec::new();

        loop {
            let (event, span, _) = self.event()?;
            match event {
                Event::Start(e) => {
                    if matches!(local(e.name().0), "ADX" | "HEADER" | "RECORDS" | "RECORD") {
                        return Err(ParseError::Envelope {
                            span,
                            message: "misplaced ADX envelope element",
                        });
                    }
                    fields.push(self.field(warnings, e, span, header)?);
                }
                Event::End(_) => return Ok(fields),
                Event::Text(e) if e.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')) => {}
                Event::Comment(_) | Event::PI(_) => {}
                _ => {
                    return Err(ParseError::Xml {
                        span,
                        message: "unexpected section content".into(),
                        source: None,
                    });
                }
            }
        }
    }

    pub fn next_section(
        &mut self,
    ) -> Option<Result<ParseResult<'a, PermissiveSection<'a>>, ParseFailure<'a>>> {
        if self.done {
            return None;
        }

        let mut warnings = Vec::new();

        match self.advance(&mut warnings) {
            Ok(Some(data)) => Some(Ok(ParseResult { data, warnings })),
            Ok(None) => {
                self.done = true;
                None
            }

            Err(error) => {
                self.done = true;
                Some(Err(ParseFailure { error, warnings }))
            }
        }
    }
}
