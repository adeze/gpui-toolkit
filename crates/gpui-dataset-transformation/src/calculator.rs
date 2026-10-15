//! Derived-field calculators.

// Rust guideline compliant 2026-02-21

use crate::TransformError;
use gpui_mesh::{Association, DataSet, Field, FieldData};

fn point_field<'a>(dataset: &'a DataSet, name: &str) -> Result<&'a Field, TransformError> {
    dataset
        .field(Association::Point, name)
        .ok_or_else(|| TransformError::FieldNotFound(name.to_string()))
}

fn with_point_scalar(dataset: &DataSet, name: &str, values: Vec<f64>, out_name: &str) -> DataSet {
    let mut result = dataset.clone();
    result.point_data.retain(|f| f.name.as_ref() != out_name);
    let mut field = Field::scalar(out_name.to_string(), values);
    if let Some(source) = dataset.field(Association::Point, name) {
        field.label = source.label.clone();
    }
    result.point_data.push(field);
    result
}

/// Appends the magnitude of a vector point field as a scalar field.
///
/// # Errors
///
/// Returns an error when the field is missing or not a vector field.
pub fn vector_magnitude(
    dataset: &DataSet,
    field_name: &str,
    out_name: &str,
) -> Result<DataSet, TransformError> {
    let field = point_field(dataset, field_name)?;
    let vectors = field
        .as_vector()
        .ok_or_else(|| TransformError::WrongFieldShape {
            field: field_name.to_string(),
            reason: "expected a 3-vector field".into(),
        })?;
    let values: Vec<f64> = vectors
        .iter()
        .map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt())
        .collect();
    Ok(with_point_scalar(dataset, field_name, values, out_name))
}

/// Appends the magnitude of a complex point field as a scalar field.
///
/// # Errors
///
/// Returns an error when the field is missing or not complex.
pub fn complex_magnitude(
    dataset: &DataSet,
    field_name: &str,
    out_name: &str,
) -> Result<DataSet, TransformError> {
    let field = point_field(dataset, field_name)?;
    let FieldData::Complex(values) = &field.data else {
        return Err(TransformError::WrongFieldShape {
            field: field_name.to_string(),
            reason: "expected a complex field".into(),
        });
    };
    let values: Vec<f64> = values
        .iter()
        .map(|c| (c[0] * c[0] + c[1] * c[1]).sqrt())
        .collect();
    Ok(with_point_scalar(dataset, field_name, values, out_name))
}

/// Appends the phase, in radians, of a complex point field.
///
/// # Errors
///
/// Returns an error when the field is missing or not complex.
pub fn complex_phase(
    dataset: &DataSet,
    field_name: &str,
    out_name: &str,
) -> Result<DataSet, TransformError> {
    let field = point_field(dataset, field_name)?;
    let FieldData::Complex(values) = &field.data else {
        return Err(TransformError::WrongFieldShape {
            field: field_name.to_string(),
            reason: "expected a complex field".into(),
        });
    };
    let values: Vec<f64> = values.iter().map(|c| c[1].atan2(c[0])).collect();
    Ok(with_point_scalar(dataset, field_name, values, out_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnitude_of_velocity() {
        let ds = DataSet::surface(
            "s",
            vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[[0, 1, 2]],
        )
        .with_field(Field::vector("velocity", vec![[3.0, 4.0, 0.0]; 3]));
        let out = vector_magnitude(&ds, "velocity", "speed").unwrap();
        let speed = out.field(Association::Point, "speed").unwrap();
        assert_eq!(speed.as_scalar().unwrap(), &[5.0, 5.0, 5.0]);
    }
}
