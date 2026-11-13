use std::ops::Range;

use super::error::{AdiError, StructuralErrorKind, TypeAlternativeFailure};
use super::{DataSpecifier, DataType, Field, FramedField, State, values};
use crate::adif::{FieldSection, ParseEncoding, StandardField, ValueError};

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
    section: FieldSection,
    specifier: &DataSpecifier<'_>,
    descriptor_span: Range<usize>,
) -> Result<ResolvedDefinition, AdiError> {
    let standard = specifier
        .name
        .parse::<StandardField>()
        .ok()
        .filter(|field| field.section() == section);
    let user = if section == FieldSection::Record {
        state
            .user_definitions
            .get(&specifier.name.to_ascii_uppercase())
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
            return Err(AdiError::Defined {
                numeric_cause: None,
                span: descriptor_span,
                kind: Box::new(StructuralErrorKind::TypeConflict {
                    field_name: specifier.name.to_string(),
                    expected: definition.data_types().to_vec(),
                    supplied,
                    definition_span: None,
                }),
            });
        }
        if let [only] = definition.data_types() {
            TypeSelection::Single(*only)
        } else {
            TypeSelection::Alternatives(definition.data_types())
        }
    } else if let Some(rule) = user {
        if let Some(expected) = rule.data_type {
            if let Some(supplied) = specifier.type_indicator
                && supplied != expected
            {
                return Err(AdiError::Defined {
                    numeric_cause: None,
                    span: descriptor_span,
                    kind: Box::new(StructuralErrorKind::TypeConflict {
                        field_name: specifier.name.to_string(),
                        expected: vec![expected],
                        supplied,
                        definition_span: Some(rule.declaration_span.clone()),
                    }),
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
                return Err(AdiError::Defined {
                    numeric_cause: None,
                    span: descriptor_span,
                    kind: Box::new(StructuralErrorKind::TypeConflict {
                        field_name: specifier.name.to_string(),
                        expected: vec![established.data_type],
                        supplied,
                        definition_span: Some(established.first_descriptor_span.clone()),
                    }),
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
        return Err(AdiError::TypeUnavailableInAdi {
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

fn parse_type<'a>(
    framed: FramedField<'a>,
    kind: Option<DataType>,
    standard: Option<StandardField>,
    encoding: ParseEncoding,
) -> Result<Field<'a>, AdiError> {
    let field = values::parse(framed, kind, encoding)?;
    if kind == Some(DataType::Enumeration)
        && field.value().is_some()
        && let Some(definition) = standard
    {
        definition
            .check_membership(field.text())
            .map_err(|set| AdiError::InvalidValue {
                field_name: field.name().to_owned(),
                span: field.value_span(),
                cause: ValueError::InvalidEnumeration,
                enumeration: Some(set),
                numeric_cause: None,
            })?;
    }
    Ok(field)
}

pub(super) fn parse_resolved<'a>(
    framed: FramedField<'a>,
    resolution: &ResolvedDefinition,
    encoding: ParseEncoding,
) -> Result<Field<'a>, AdiError> {
    let alternatives = match resolution.selection {
        TypeSelection::Untyped => return parse_type(framed, None, resolution.standard, encoding),
        TypeSelection::Single(kind) => {
            return parse_type(framed, Some(kind), resolution.standard, encoding);
        }
        TypeSelection::Alternatives(types) => types,
    };
    let mut failures = Vec::new();
    for kind in alternatives {
        match parse_type(framed.clone(), Some(*kind), resolution.standard, encoding) {
            Ok(field) => return Ok(field),
            Err(AdiError::InvalidValue {
                span,
                cause,
                enumeration,
                ..
            }) => {
                failures.push(TypeAlternativeFailure {
                    data_type: *kind,
                    span,
                    cause,
                    enumeration,
                });
            }
            Err(error) => return Err(error),
        }
    }
    let span = failures
        .first()
        .map_or_else(|| framed.value_span.clone(), |f| f.span.clone());
    Err(AdiError::Defined {
        numeric_cause: None,
        span,
        kind: Box::new(StructuralErrorKind::NoMatchingType {
            field_name: framed.specifier.name.to_string(),
            failures,
        }),
    })
}

#[derive(Debug)]
pub(super) struct UserFieldRule {
    data_type: Option<DataType>,
    constraint: Option<(crate::adif::adi::UserDefinitionConstraint, Range<usize>)>,
    declaration_span: Range<usize>,
    target_span: Range<usize>,
}

pub(super) fn register_user_definition(
    state: &mut State,
    definition: &crate::adif::adi::UserDefinition<'_>,
) -> Result<(), AdiError> {
    let target = definition.field_name();
    let standard_name = target.parse::<StandardField>().is_ok()
        || (super::declarations::is_declaration(target)
            && target.as_bytes()[7..].iter().any(|byte| *byte != b'0'));
    if standard_name {
        return Err(AdiError::InvalidUserDefinition {
            field_name: definition.name().to_owned(),
            span: definition.target_span.clone(),
            cause: crate::adif::adi::UserDefinitionError::StandardFieldName,
            numeric_cause: None,
        });
    }
    let key = target.to_ascii_uppercase();
    if let Some(first) = state.user_definitions.get(&key) {
        return Err(AdiError::Defined {
            span: definition.target_span.clone(),
            numeric_cause: None,
            kind: Box::new(StructuralErrorKind::DuplicateUserDefinition {
                field_name: target.to_owned(),
                first_span: first.target_span.clone(),
                second_span: definition.target_span.clone(),
            }),
        });
    }
    state.user_definitions.insert(
        key,
        UserFieldRule {
            data_type: definition.data_type(),
            constraint: definition
                .constraint()
                .cloned()
                .zip(definition.constraint_span.clone()),
            declaration_span: definition.descriptor_span(),
            target_span: definition.target_span.clone(),
        },
    );
    Ok(())
}

pub(super) fn check_user_constraints(state: &State, field: &Field<'_>) -> Result<(), AdiError> {
    use crate::adif::adi::{UserDefinitionConstraint, UserDefinitionValueError};

    if field.value().is_none() {
        return Ok(());
    }
    let Some(rule) = state
        .user_definitions
        .get(&field.name().to_ascii_uppercase())
    else {
        return Ok(());
    };
    let Some((constraint, constraint_span)) = &rule.constraint else {
        return Ok(());
    };
    let fail = |cause, span, numeric_cause| AdiError::Defined {
        span,
        numeric_cause,
        kind: Box::new(StructuralErrorKind::UserDefinitionConstraintViolation {
            field_name: field.name().to_owned(),
            declaration_span: rule.declaration_span.clone(),
            constraint_span: constraint_span.clone(),
            cause,
        }),
    };
    match constraint {
        UserDefinitionConstraint::Enumeration(members) => {
            if !members
                .iter()
                .any(|member| caseless::default_caseless_match_str(member, field.text()))
            {
                return Err(fail(
                    UserDefinitionValueError::NotInEnumeration,
                    field.value_span(),
                    None,
                ));
            }
        }
        UserDefinitionConstraint::Range { lower, upper } => {
            let value = crate::adif::number::parse_number(field.raw_data()).map_err(|error| {
                let start = field.value_span().start;
                fail(
                    UserDefinitionValueError::InvalidRangeValue {
                        cause: ValueError::InvalidNumber,
                    },
                    start + error.span.start..start + error.span.end,
                    error.cause.map(Into::into),
                )
            })?;
            if value < *lower || value > *upper {
                return Err(fail(
                    UserDefinitionValueError::OutOfRange {
                        lower: lower.clone(),
                        upper: upper.clone(),
                    },
                    field.value_span(),
                    None,
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
