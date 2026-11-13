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
    _state: &State,
    section: FieldSection,
    specifier: &DataSpecifier<'_>,
    descriptor_span: Range<usize>,
) -> Result<ResolvedDefinition, AdiError> {
    let standard = specifier
        .name
        .parse::<StandardField>()
        .ok()
        .filter(|field| field.section() == section);
    let application_rule =
        standard.is_none() && section == FieldSection::Record && is_application(&specifier.name);
    let selection = if let Some(definition) = standard {
        if let Some(supplied) = specifier.type_indicator
            && !definition.data_types().contains(&supplied)
        {
            return Err(AdiError::Defined {
                span: descriptor_span,
                kind: StructuralErrorKind::TypeConflict {
                    field_name: specifier.name.to_string(),
                    expected: definition.data_types().to_vec(),
                    supplied,
                    definition_span: None,
                },
            });
        }
        if let [only] = definition.data_types() {
            TypeSelection::Single(*only)
        } else {
            TypeSelection::Alternatives(definition.data_types())
        }
    } else if application_rule {
        TypeSelection::Single(
            specifier
                .type_indicator
                .unwrap_or(DataType::MultilineString),
        )
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
        span,
        kind: StructuralErrorKind::NoMatchingType {
            field_name: framed.specifier.name.to_string(),
            failures,
        },
    })
}
