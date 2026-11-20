use std::collections::HashMap;

use crate::adif::value::{UserConstraintViolation, Value, check_user_constraint};

use super::super::{
    decoding::{TextContext, locate_text_range},
    error::{DeclarationError, ParseError},
};
use super::{
    declarations::UserDefinition,
    field::Field,
    header::{Header, HeaderEntry},
};

pub(super) fn check_header<'a>(header: &Header<'a>) -> Result<(), ParseError<'a>> {
    let mut ids = HashMap::new();
    let mut targets = HashMap::new();

    for entry in header.entries() {
        let HeaderEntry::UserDefinition(definition) = entry else {
            continue;
        };

        if let Some(first_span) = ids.insert(definition.number(), definition.identifier_span()) {
            return Err(ParseError::Declaration {
                span: definition.identifier_span(),
                reason: DeclarationError::Duplicate { first_span },
            });
        }

        if let Some(first_span) = targets.insert(definition.field_name(), definition.target_span())
        {
            return Err(ParseError::Declaration {
                span: definition.target_span(),
                reason: DeclarationError::Duplicate { first_span },
            });
        }
    }

    Ok(())
}

impl UserDefinition<'_> {
    pub(super) fn check<'a>(&self, f: &Field<'a>) -> Result<(), ParseError<'a>> {
        if f.value().is_none() {
            return Ok(());
        }

        let Some(constraint) = &self.constraint else {
            return Ok(());
        };
        let fail = |span, source| ParseError::ConstraintViolation {
            span,
            declaration_span: self.element_span(),
            constraint_span: constraint.span.clone(),
            source,
        };

        let number = match f.value() {
            Some(Value::Number(number)) => Some(number),
            _ => None,
        };
        match check_user_constraint(&constraint.constraint, f.text(), number) {
            Ok(()) => Ok(()),
            Err(UserConstraintViolation::InvalidNumber(error)) => {
                let span = locate_text_range(
                    f.element.raw_content,
                    f.value_span().start,
                    error.span,
                    TextContext::Content,
                )?;
                Err(fail(span, error.cause))
            }
            Err(
                UserConstraintViolation::NotInEnumeration
                | UserConstraintViolation::OutOfRange { .. },
            ) => Err(fail(f.value_span(), None)),
        }
    }
}
