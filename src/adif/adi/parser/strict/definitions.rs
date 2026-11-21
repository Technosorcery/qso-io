use std::collections::HashMap;
use std::ops::Range;

use super::super::{error::SyntaxErrorKind, options::ParseOpts};
use super::error::StructuralError;
use super::{declarations::UserDefinition, values};
use crate::adif::DataType;
use crate::adif::adi::parser::options::ParseEncoding;
use crate::adif::adi::parser::strict::field::{DataSpecifier, Field};
use crate::adif::adi::{application::resolve_application_type, reserved_user_field_name};
use crate::adif::value::{UserConstraintViolation, check_user_constraint};
use crate::adif::{FieldSection, StandardField};

#[derive(Debug, Default)]
pub(super) struct State {
    pub(super) parser_options: ParseOpts,
    pub(super) application_types: HashMap<String, EstablishedApplicationType>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum TypeSelection {
    Untyped,
    Single(DataType),
    Alternatives(&'static [DataType]),
}

pub(super) struct ResolvedDefinition {
    pub selection: TypeSelection,
    pub application_rule: bool,
    pub standard: Option<StandardField>,
}

fn is_application(name: &str) -> Result<bool, SyntaxErrorKind> {
    let components = name
        .split_at_checked(4)
        .filter(|(prefix, _)| prefix.eq_ignore_ascii_case("APP_"))
        .and_then(|(_, tail)| tail.split_once('_'));
    match components {
        Some(("", _)) => Err(SyntaxErrorKind::EmptyApplicationProgramId),
        Some((_, field)) => Ok(!field.is_empty()),
        None => Ok(false),
    }
}

pub(super) fn resolve(
    state: &State,
    users: &HashMap<String, &UserDefinition<'_>>,
    section: FieldSection,
    specifier: &DataSpecifier<'_>,
    descriptor_span: Range<usize>,
) -> Result<ResolvedDefinition, StructuralError<'static>> {
    let standard = specifier
        .name
        .parse::<StandardField>()
        .ok()
        .filter(|field| field.section() == section);
    let user = if section == FieldSection::Record {
        users.get(&specifier.name.to_ascii_uppercase())
    } else {
        None
    };
    let application_rule = standard.is_none()
        && user.is_none()
        && section == FieldSection::Record
        && is_application(&specifier.name).map_err(|reason| StructuralError::Syntax {
            reason,
            span: descriptor_span.clone(),
        })?;
    let selection = if let Some(definition) = standard {
        if let Some(supplied) = specifier.type_indicator
            && !definition.data_types().contains(&supplied)
        {
            return Err(StructuralError::StandardTypeConflict {
                span: descriptor_span,
                field_name: specifier.name.to_string(),
                expected: definition.data_types(),
                supplied,
            });
        }
        if let [only] = definition.data_types() {
            TypeSelection::Single(*only)
        } else {
            TypeSelection::Alternatives(definition.data_types())
        }
    } else if let Some(rule) = user {
        if let Some(expected) = rule.data_type() {
            if let Some(supplied) = specifier.type_indicator
                && supplied != expected
            {
                return Err(StructuralError::UserTypeConflict {
                    span: descriptor_span,
                    field_name: specifier.name.to_string(),
                    expected,
                    supplied,
                    declaration_span: rule.descriptor_span(),
                });
            }
            TypeSelection::Single(expected)
        } else {
            specifier
                .type_indicator
                .map_or(TypeSelection::Untyped, TypeSelection::Single)
        }
    } else if application_rule {
        let established = state
            .application_types
            .get(&specifier.name.to_ascii_uppercase());
        let data_type = resolve_application_type(
            established.map(|first| (first.data_type, &first.first_descriptor_span)),
            specifier.type_indicator,
        )
        .map_err(|conflict| StructuralError::ApplicationTypeConflict {
            span: descriptor_span.clone(),
            field_name: specifier.name.to_string(),
            expected: conflict.established,
            supplied: conflict.supplied,
            first_span: conflict.first.clone(),
        })?;
        TypeSelection::Single(data_type)
    } else {
        specifier
            .type_indicator
            .map_or(TypeSelection::Untyped, TypeSelection::Single)
    };
    if let TypeSelection::Single(
        kind @ (DataType::IntlCharacter | DataType::IntlString | DataType::IntlMultilineString),
    ) = selection
    {
        return Err(StructuralError::TypeUnavailableInAdi {
            data_type: kind,
            span: descriptor_span,
        });
    }

    Ok(ResolvedDefinition {
        selection,
        application_rule,
        standard,
    })
}

pub(super) fn check_user_definition(
    targets: &mut HashMap<String, Range<usize>>,
    definition: &UserDefinition<'_>,
) -> Result<(), StructuralError<'static>> {
    let target = definition.field_name();
    if reserved_user_field_name(target) {
        return Err(StructuralError::InvalidUserDefinition {
            field_name: definition.name().to_owned(),
            span: definition.target_span.clone(),
            cause: crate::adif::adi::parser::strict::declarations::UserDefinitionError::StandardFieldName,
        });
    }
    let key = target.to_ascii_uppercase();
    if let Some(first) = targets.get(&key) {
        return Err(StructuralError::DuplicateUserDefinition {
            field_name: target.to_owned(),
            first_span: first.clone(),
            second_span: definition.target_span.clone(),
        });
    }
    targets.insert(key, definition.target_span.clone());
    Ok(())
}

pub(super) fn check_user_constraints<'header>(
    users: &HashMap<String, &'header UserDefinition<'_>>,
    field: &Field<'_>,
    encoding: ParseEncoding,
) -> Result<(), StructuralError<'header>> {
    use super::declarations::UserDefinitionValueError;
    if field.value().is_none() {
        return Ok(());
    }

    let Some(&rule) = users.get(&field.name().to_ascii_uppercase()) else {
        return Ok(());
    };
    let Some(located) = &rule.constraint else {
        return Ok(());
    };

    let fail = |cause, span| StructuralError::UserDefinitionConstraintViolation {
        span,
        field_name: field.name().to_owned(),
        declaration_span: rule.descriptor_span(),
        constraint_span: located.span.clone(),
        cause,
    };

    match check_user_constraint(&located.constraint, field.text(), None) {
        Ok(()) => Ok(()),
        Err(UserConstraintViolation::NotInEnumeration) => Err(fail(
            UserDefinitionValueError::NotInEnumeration,
            field.value_span(),
        )),
        Err(UserConstraintViolation::InvalidNumber(error)) => {
            let start = field.value_span().start;
            let span = values::source_range(field.text(), error.span, encoding);
            Err(fail(
                UserDefinitionValueError::InvalidRangeValue {
                    source: error.cause,
                },
                start + span.start..start + span.end,
            ))
        }
        Err(UserConstraintViolation::OutOfRange { lower, upper }) => Err(fail(
            UserDefinitionValueError::OutOfRange { lower, upper },
            field.value_span(),
        )),
    }
}

#[derive(Debug)]
pub(super) struct EstablishedApplicationType {
    data_type: DataType,
    first_descriptor_span: Range<usize>,
}

pub(super) fn register_application(
    state: &mut State,
    field: &Field<'_>,
    resolution: &ResolvedDefinition,
) {
    if resolution.application_rule
        && let Some(data_type) = field.data_type()
    {
        state
            .application_types
            .entry(field.name().to_ascii_uppercase())
            .or_insert_with(|| EstablishedApplicationType {
                data_type,
                first_descriptor_span: field.descriptor_span(),
            });
    }
}
