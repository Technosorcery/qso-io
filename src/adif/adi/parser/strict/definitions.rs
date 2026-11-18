use std::collections::HashMap;
use std::ops::Range;

use super::super::options::ParseOpts;
use super::error::StructuralError;
use super::{declarations::UserDefinition, values};
use crate::adif::DataType;
use crate::adif::adi::parser::options::ParseEncoding;
use crate::adif::adi::parser::strict::field::{DataSpecifier, Field};
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

pub(super) fn is_application(name: &str) -> bool {
    name.get(..4)
        .is_some_and(|p| p.eq_ignore_ascii_case("APP_"))
        && name.get(4..).is_some_and(|tail| {
            tail.split_once('_')
                .is_some_and(|(program, field)| !program.is_empty() && !field.is_empty())
        })
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
        && is_application(&specifier.name);
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
        if let Some(established) = state
            .application_types
            .get(&specifier.name.to_ascii_uppercase())
        {
            if let Some(supplied) = specifier.type_indicator
                && supplied != established.data_type
            {
                return Err(StructuralError::ApplicationTypeConflict {
                    span: descriptor_span,
                    field_name: specifier.name.to_string(),
                    expected: established.data_type,
                    supplied,
                    first_span: established.first_descriptor_span.clone(),
                });
            }
            TypeSelection::Single(established.data_type)
        } else {
            TypeSelection::Single(
                specifier
                    .type_indicator
                    .unwrap_or(DataType::MultilineString),
            )
        }
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
    let standard_name = target.parse::<StandardField>().is_ok()
        || (super::declarations::is_declaration(target)
            && target.as_bytes()[7..].iter().any(|byte| *byte != b'0'));
    if standard_name {
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
    use super::declarations::{UserDefinitionConstraint, UserDefinitionValueError};

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

    match &located.constraint {
        UserDefinitionConstraint::Enumeration(members) => {
            if !members
                .iter()
                .any(|member| caseless::default_caseless_match_str(member, field.text()))
            {
                return Err(fail(
                    UserDefinitionValueError::NotInEnumeration,
                    field.value_span(),
                ));
            }
        }
        UserDefinitionConstraint::Range { lower, upper } => {
            let value = crate::adif::number::parse_number(field.text()).map_err(|error| {
                let start = field.value_span().start;
                let span = values::source_range(field.text(), error.span, encoding);
                fail(
                    UserDefinitionValueError::InvalidRangeValue {
                        source: error.cause,
                    },
                    start + span.start..start + span.end,
                )
            })?;
            if value < *lower || value > *upper {
                return Err(fail(
                    UserDefinitionValueError::OutOfRange { lower, upper },
                    field.value_span(),
                ));
            }
        }
    }
    Ok(())
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
