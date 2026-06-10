use crate::error;
use crate::validation::spec::{ConstraintConfig, FieldSpec, FieldType};
use crate::validation::types::ValidationReport;

fn check_bounds<T: Into<f64> + Copy>(value: &T, constraint: &ConstraintConfig) -> bool {
    let val: f64 = Into::<f64>::into(value.to_owned());
    if let Some(min) = constraint.min {
        if val < min {
            eprintln!("Value {} is less than minimum {}", val, min);
            return false;
        }
    }
    if let Some(max) = constraint.max {
        if val > max {
            eprintln!("Value {} is greater than maximum {}", val, max);
            return false;
        }
    }
    if let Some(len) = constraint.len {
        if val.to_string().len() > len {
            eprintln!("Value {} is longer than {}", val, len);
            return false;
        }
    }
    if let Some(enum_values) = &constraint.enum_values {
        if !enum_values.contains(&val.to_string().to_lowercase()) {
            // eprintln!("Value {} is not in {:?}", val, enum_values);
            return false;
        }
    }
    true
}

fn check_string_bounds(value: &String, constraint: &ConstraintConfig) -> bool {
    if let Some(len) = constraint.len {
        if value.len() > len {
            eprintln!("Value {} is longer than {}", value, len);
            return false;
        }
    }
    if let Some(enum_values) = &constraint.enum_values {
        if !enum_values.contains(&value.to_lowercase()) {
            // eprintln!("Value {} is not in {:?}", value, enum_values);
            return false;
        }
    }
    true
}

// fn apply_constraint(value: &mut GHubsConfig, constraint: &ConstraintConfig) {}

fn validate_double(value: &String, constraint: &ConstraintConfig) -> Result<bool, error::Error> {
    let v = value
        .parse::<f64>()
        .map_err(|_| error::Error::ParseError(format!("Invalid double value: {}", value)))?;
    Ok(check_bounds(&v, constraint))
}

pub fn apply_validation(value: String, field: &FieldSpec) -> Result<bool, error::Error> {
    let constraint = match field.constraint.to_owned() {
        Some(c) => c,
        None => ConstraintConfig {
            ..Default::default()
        },
    };
    let field_type = &field.field_type;
    let valid = match field_type {
        FieldType::Byte => {
            let dot_pos = value.find(".").unwrap_or(value.len());
            let v = value[..dot_pos]
                .parse::<i8>()
                .map_err(|_| error::Error::ParseError(format!("Invalid byte value: {}", value)))?;
            check_bounds(&v, &constraint)
        }
        FieldType::Date => true,
        FieldType::Double => validate_double(&value, &constraint)?,

        FieldType::Float => {
            let v = value
                .parse::<f32>()
                .map_err(|_| error::Error::ParseError(format!("Invalid float value: {}", value)))?;
            check_bounds(&v, &constraint)
        }
        FieldType::GeoPoint => true,
        FieldType::HalfFloat => {
            let v = value.parse::<f32>().map_err(|_| {
                error::Error::ParseError(format!("Invalid half_float value: {}", value))
            })?;
            check_bounds(&v, &constraint)
        }
        FieldType::Keyword => {
            let v = value.parse::<String>().map_err(|_| {
                error::Error::ParseError(format!("Invalid keyword value: {}", value))
            })?;
            check_string_bounds(&v, &constraint)
        }
        FieldType::Integer => {
            let dot_pos = value.find(".").unwrap_or(value.len());
            let v = value[..dot_pos].parse::<i32>().map_err(|_| {
                error::Error::ParseError(format!("Invalid integer value: {}", value))
            })?;
            check_bounds(&v, &constraint)
        }
        FieldType::Long => {
            let dot_pos = value.find(".").unwrap_or(value.len());
            value[..dot_pos]
                .parse::<i64>()
                .map_err(|_| error::Error::ParseError(format!("Invalid long value: {}", value)))?;
            validate_double(&value, &constraint)?
        }
        FieldType::Short => {
            let dot_pos = value.find(".").unwrap_or(value.len());
            let v = value[..dot_pos]
                .parse::<i16>()
                .map_err(|_| error::Error::ParseError(format!("Invalid short value: {}", value)))?;
            check_bounds(&v, &constraint)
        }
        FieldType::OneDP => true,
        FieldType::TwoDP => true,
        FieldType::ThreeDP => true,
        FieldType::FourDP => true,
    };
    Ok(valid)
}

pub trait RowValidator {
    /// Validate one parsed row (column->value). Return a ValidationReport.
    fn validate_row(
        &mut self,
        row: &std::collections::HashMap<String, String>,
    ) -> Result<ValidationReport, error::Error>;
}

pub trait DocumentValidator {
    fn validate_document<D: crate::index::es::models::IndexDocument>(
        &self,
        doc: &D,
    ) -> Result<bool, crate::error::Error>;
}
