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
