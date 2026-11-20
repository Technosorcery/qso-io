use std::collections::{HashMap, hash_map::Entry};

use bigdecimal::{BigDecimal, num_bigint::Sign};

use crate::adif::{
    DataType, UserFieldDefinition,
    adi::{parser::options::ParseEncoding, reserved_user_field_name},
    text::check_text,
    value::{UserConstraintViolation, UserDefinitionConstraint, check_user_constraint},
};

use super::{WriteError, append_field, check_indicator, check_value, invalid_custom_name};

#[derive(Default)]
pub(super) struct Declarations {
    fields: HashMap<String, DeclaredField>,
}

struct DeclaredField {
    data_type: Option<DataType>,
    constraint: Option<UserDefinitionConstraint>,
}

impl Declarations {
    pub(super) fn contains(&self, key: &str) -> bool {
        self.fields.contains_key(key)
    }

    pub(super) fn append(
        &mut self,
        output: &mut Vec<u8>,
        number: usize,
        definition: UserFieldDefinition<'_>,
    ) -> Result<(), WriteError> {
        let UserFieldDefinition {
            name,
            data_type,
            constraint,
        } = definition;
        let key = name.to_ascii_uppercase();
        if name.is_empty()
            || name.starts_with(' ')
            || name.ends_with(' ')
            || invalid_custom_name(name)
            || reserved_user_field_name(name)
        {
            return Err(WriteError::InvalidFieldName { field: name.into() });
        }

        let Entry::Vacant(entry) = self.fields.entry(key) else {
            return Err(WriteError::DuplicateDeclaration { field: name.into() });
        };

        let indicator = data_type
            .map(|kind| check_indicator(name, kind))
            .transpose()?;
        let mut payload = name.to_owned();
        if let Some(constraint) = constraint {
            append_constraint(&mut payload, name, data_type, constraint)?;
        }
        append_field(output, &format!("USERDEF{number}"), indicator, &payload);
        entry.insert(DeclaredField {
            data_type,
            constraint: constraint.cloned(),
        });
        Ok(())
    }

    pub(super) fn check_value(&self, key: &str, name: &str, value: &str) -> Result<(), WriteError> {
        let definition = self
            .fields
            .get(key)
            .ok_or_else(|| WriteError::MissingDeclaration { field: name.into() })?;
        let multiline = matches!(definition.data_type, None | Some(DataType::MultilineString));
        check_value(name, multiline, value)?;
        if let Some(constraint) = &definition.constraint {
            check_user_constraint(constraint, value, None).map_err(|violation| {
                let constraint = match violation {
                    UserConstraintViolation::NotInEnumeration => "enumeration",
                    UserConstraintViolation::InvalidNumber(_)
                    | UserConstraintViolation::OutOfRange { .. } => "numeric range",
                };
                WriteError::ConstraintViolation {
                    field: name.into(),
                    constraint,
                }
            })?;
        }
        Ok(())
    }
}

fn append_constraint(
    output: &mut String,
    field: &str,
    data_type: Option<DataType>,
    constraint: &UserDefinitionConstraint,
) -> Result<(), WriteError> {
    let invalid = |reason| WriteError::InvalidConstraint {
        field: field.into(),
        reason,
    };
    output.push_str(",{");
    match constraint {
        UserDefinitionConstraint::Enumeration(members) => {
            if members.is_empty() {
                return Err(invalid("an enumeration requires at least one member"));
            }
            for (index, member) in members.iter().enumerate() {
                if member.is_empty() || member.contains(',') {
                    return Err(invalid(
                        "an enumeration member must be nonempty and comma-free",
                    ));
                }
                if data_type != Some(DataType::Enumeration) && member.contains(':') {
                    return Err(invalid("a colon requires the enumeration type indicator"));
                }
                check_text(member.as_bytes(), ParseEncoding::Ascii, false).map_err(|range| {
                    WriteError::InvalidEnumerationCharacter {
                        field: field.into(),
                        member_index: index,
                        offset: range.start,
                    }
                })?;
                if index > 0 {
                    output.push(',');
                }
                output.push_str(member);
            }
        }
        UserDefinitionConstraint::Range { lower, upper } => {
            if data_type == Some(DataType::Enumeration) {
                return Err(invalid("a range cannot use the enumeration type indicator"));
            }
            if lower >= upper {
                return Err(invalid(
                    "the upper bound must be greater than the lower bound",
                ));
            }
            append_decimal(output, lower)
                .map_err(|()| invalid("the lower bound is too large to write"))?;
            output.push(':');
            append_decimal(output, upper)
                .map_err(|()| invalid("the upper bound is too large to write"))?;
        }
    }
    output.push('}');
    Ok(())
}

fn append_decimal(output: &mut String, number: &BigDecimal) -> Result<(), ()> {
    let sign = number.sign();
    if sign == Sign::NoSign {
        output.push('0');
        return Ok(());
    }

    // ADI forbids exponent notation. Reserve space before formatting decimals
    // whose small coefficient and large scale would expand to many bytes.
    let digits = i128::from(number.digits());
    let scale = i128::from(number.fractional_digit_count());
    let length = if scale <= 0 {
        digits - scale
    } else {
        digits.max(scale + 1) + 1
    };
    let length = usize::try_from(length + i128::from(sign == Sign::Minus)).map_err(|_| ())?;
    output.try_reserve(length).map_err(|_| ())?;
    number.write_plain_string(output).map_err(|_| ())
}
