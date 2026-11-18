use std::{borrow::Cow, ops::Range};

use super::attribute::Attribute;

use super::super::{
    error::{ParseError, TextError, WarningKind},
    identity::FieldIdentity,
    model::{is_namespace, local},
    permissive::field::PermissiveField,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Identity<'a> {
    Ordinary,
    Application {
        program: Attribute<'a>,
        field: Attribute<'a>,
    },
    User(Attribute<'a>),
    Declaration(Attribute<'a>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Element<'a> {
    pub input: &'a [u8],
    pub name: &'a str,
    pub span: Range<usize>,
    pub name_span: Range<usize>,
    pub content: Range<usize>,
    pub raw_content: &'a str,
    attributes: Vec<Attribute<'a>>,
    identity: Identity<'a>,
}

impl<'a> Element<'a> {
    pub fn identity(&self) -> FieldIdentity<'_> {
        match &self.identity {
            Identity::Ordinary => FieldIdentity::Ordinary(Cow::Borrowed(local(self.name))),
            Identity::Application { program, field } => FieldIdentity::Application {
                program_id: Cow::Borrowed(program.text()),
                field_name: Cow::Borrowed(field.text()),
            },
            Identity::User(field) => FieldIdentity::User(Cow::Borrowed(field.text())),
            Identity::Declaration(id) => FieldIdentity::Declaration(Cow::Borrowed(id.text())),
        }
    }

    pub fn attributes(&self) -> impl Iterator<Item = &Attribute<'a>> {
        let mut identity = match &self.identity {
            Identity::Ordinary => [None, None],
            Identity::Application { program, field } => [Some(program), Some(field)],
            Identity::User(field) | Identity::Declaration(field) => [Some(field), None],
        };

        identity.sort_by_key(|attribute| attribute.map(|a| a.name_span.start));
        let mut identity = identity.into_iter().flatten().peekable();
        let mut others = self.attributes.iter().peekable();

        std::iter::from_fn(move || match (identity.peek(), others.peek()) {
            (Some(a), Some(b)) if a.name_span.start < b.name_span.start => identity.next(),
            (Some(_), None) => identity.next(),
            _ => others.next(),
        })
    }

    pub fn attribute(&self, name: &str) -> Option<&Attribute<'a>> {
        self.attributes()
            .find(|a| !is_namespace(a.name) && local(a.name) == name)
    }

    pub fn required(&self, name: &'static str) -> Result<&Attribute<'a>, ParseError<'a>> {
        match self.attribute(name) {
            Some(attribute) if !attribute.text().is_empty() => Ok(attribute),
            Some(attribute) => Err(ParseError::MissingAttribute {
                span: attribute.value_span(),
                attribute: name,
            }),
            None => Err(ParseError::MissingAttribute {
                span: self.name_span.clone(),
                attribute: name,
            }),
        }
    }
}

pub(super) fn prepare<'a>(
    raw: PermissiveField<'a>,
) -> Result<(Element<'a>, Cow<'a, str>), ParseError<'a>> {
    let mut program = None;
    let mut field = None;
    let mut identifier = None;
    let mut attributes = Vec::new();

    for a in raw.attrs {
        let text = a.text.map_err(|reason| ParseError::Prohibited {
            span: a.value_span.clone(),
            construct: match reason {
                TextError::CustomReference(name) => WarningKind::CustomReference(name),
                TextError::NestedMarkup => WarningKind::NestedMarkup,
            },
        })?;
        let attribute = Attribute {
            name: a.name,
            raw: a.raw,
            name_span: a.name_span,
            value_span: a.value_span,
            text,
        };
        if is_namespace(attribute.name) {
            attributes.push(attribute);
            continue;
        }
        let name = local(attribute.name);

        match (local(raw.name), raw.header, name) {
            ("APP", _, "PROGRAMID") if program.is_none() => program = Some(attribute),
            ("APP", _, "FIELDNAME") | ("USERDEF", false, "FIELDNAME") if field.is_none() => {
                field = Some(attribute)
            }
            ("USERDEF", true, "FIELDID") if identifier.is_none() => identifier = Some(attribute),
            _ => attributes.push(attribute),
        }
    }

    let identity = match (local(raw.name), raw.header) {
        ("APP", _) => Identity::Application {
            program: program.ok_or_else(|| ParseError::MissingAttribute {
                span: raw.name_span.clone(),
                attribute: "PROGRAMID",
            })?,
            field: field.ok_or_else(|| ParseError::MissingAttribute {
                span: raw.name_span.clone(),
                attribute: "FIELDNAME",
            })?,
        },
        ("USERDEF", false) => {
            Identity::User(field.ok_or_else(|| ParseError::MissingAttribute {
                span: raw.name_span.clone(),
                attribute: "FIELDNAME",
            })?)
        }
        ("USERDEF", true) => {
            Identity::Declaration(identifier.ok_or_else(|| ParseError::MissingAttribute {
                span: raw.name_span.clone(),
                attribute: "FIELDID",
            })?)
        }
        _ => Identity::Ordinary,
    };
    let text = raw.text.map_err(|reason| ParseError::Prohibited {
        span: raw.content.clone(),
        construct: match reason {
            TextError::CustomReference(name) => WarningKind::CustomReference(name),
            TextError::NestedMarkup => WarningKind::NestedMarkup,
        },
    })?;

    Ok((
        Element {
            input: raw.input,
            name: raw.name,
            span: raw.element,
            name_span: raw.name_span,
            content: raw.content,
            raw_content: raw.raw_content,
            attributes,
            identity,
        },
        text,
    ))
}
