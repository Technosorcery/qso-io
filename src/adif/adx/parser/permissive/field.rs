//! Unchecked ADX fields with their original XML source.

use std::{borrow::Cow, ops::Range};

use super::attribute::Attribute;

use super::super::{
    error::TextError,
    identity::FieldIdentity,
    model::{is_namespace, local},
};

/// A field with unchecked ADIF contents and its supplied XML source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissiveField<'a> {
    pub(crate) input: &'a [u8],
    pub(crate) name: &'a str,
    pub(crate) element: Range<usize>,
    pub(crate) name_span: Range<usize>,
    pub(crate) content: Range<usize>,
    pub(crate) raw_content: &'a str,
    pub(crate) attrs: Vec<Attribute<'a>>,
    pub(crate) text: Result<Cow<'a, str>, TextError<'a>>,
    pub(crate) header: bool,
}

impl<'a> PermissiveField<'a> {
    /// Returns the supplied qualified element name.
    pub fn name(&self) -> &'a str {
        self.name
    }

    /// Borrows the whole original element including tags.
    pub fn raw_element(&self) -> &'a [u8] {
        &self.input[self.element.clone()]
    }

    /// Borrows the original contents between tags.
    pub fn raw_data(&self) -> &'a [u8] {
        &self.input[self.content.clone()]
    }

    /// Returns the complete element's byte range in the parser's input, with an
    /// exclusive end.
    pub fn element_span(&self) -> Range<usize> {
        self.element.clone()
    }

    /// Returns the element name's byte range in the parser's input, with an
    /// exclusive end.
    pub fn name_span(&self) -> Range<usize> {
        self.name_span.clone()
    }

    /// Returns the contents' byte range in the parser's input, excluding tags
    /// and with an exclusive end.
    pub fn value_span(&self) -> Range<usize> {
        self.content.clone()
    }

    /// Borrows supplied attributes in source order.
    pub fn attributes(&self) -> &[Attribute<'a>] {
        &self.attrs
    }

    /// Returns the complete decoded text, or the reason it is unavailable.
    pub fn text(&self) -> Result<&str, &TextError<'a>> {
        self.text.as_deref()
    }

    /// Finds a supplied attribute by its exact local name.
    ///
    /// Namespace declarations are excluded from this lookup.
    pub fn attribute(&self, name: &str) -> Option<&Attribute<'a>> {
        self.attrs
            .iter()
            .find(|a| !is_namespace(a.name) && local(a.name) == name)
    }

    /// Returns the complete domain identity, if supplied and decodable.
    pub fn identity(&self) -> Option<FieldIdentity<'_>> {
        let attr = |name| self.attribute(name)?.text().ok().map(Cow::Borrowed);
        Some(match local(self.name) {
            "APP" => FieldIdentity::Application {
                program_id: attr("PROGRAMID")?,
                field_name: attr("FIELDNAME")?,
            },
            "USERDEF" if self.header => FieldIdentity::Declaration(attr("FIELDID")?),
            "USERDEF" => FieldIdentity::User(attr("FIELDNAME")?),
            name => FieldIdentity::Ordinary(Cow::Borrowed(name)),
        })
    }
}
