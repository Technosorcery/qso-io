//! Domain identities for ADX fields.

use std::borrow::Cow;

/// The domain identity of a supplied field.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FieldIdentity<'a> {
    /// An ordinary ADIF field name.
    Ordinary(Cow<'a, str>),

    /// Separate application program and field names.
    Application {
        /// Supplied program identifier.
        program_id: Cow<'a, str>,
        /// Supplied field name.
        field_name: Cow<'a, str>,
    },

    /// A record reference to a user-defined field name.
    User(Cow<'a, str>),

    /// A header declaration identifier, distinct from its target name.
    Declaration(Cow<'a, str>),
}

impl<'a> FieldIdentity<'a> {
    pub(crate) fn key(self) -> Self {
        let key = |s: Cow<'a, str>| {
            if s.bytes().any(|byte| byte.is_ascii_lowercase()) {
                let mut text = s.into_owned();
                text.make_ascii_uppercase();
                Cow::Owned(text)
            } else {
                s
            }
        };

        match self {
            Self::Ordinary(s) => Self::Ordinary(key(s)),
            Self::Application {
                program_id,
                field_name,
            } => Self::Application {
                program_id: key(program_id),
                field_name: key(field_name),
            },
            Self::User(s) => Self::User(key(s)),
            Self::Declaration(s) => Self::Declaration(key(s)),
        }
    }

    pub(crate) fn into_owned(self) -> FieldIdentity<'static> {
        match self {
            Self::Ordinary(s) => FieldIdentity::Ordinary(Cow::Owned(s.into_owned())),
            Self::Application {
                program_id,
                field_name,
            } => FieldIdentity::Application {
                program_id: Cow::Owned(program_id.into_owned()),
                field_name: Cow::Owned(field_name.into_owned()),
            },
            Self::User(s) => FieldIdentity::User(Cow::Owned(s.into_owned())),
            Self::Declaration(s) => FieldIdentity::Declaration(Cow::Owned(s.into_owned())),
        }
    }
}
