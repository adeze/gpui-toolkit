//! Geometry warping by fields.

// Rust guideline compliant 2026-02-21

use crate::TransformError;
use gpui_mesh::{Association, DataSet, FieldData};

/// Displaces points along a vector field: `p' = p + scale * v`.
///
/// # Errors
///
/// Returns an error when the field is missing or not a vector field.
pub fn warp_by_vector(
    dataset: &DataSet,
    field_name: &str,
    scale: f64,
) -> Result<DataSet, TransformError> {
    let field = dataset
        .field(Association::Point, field_name)
        .ok_or_else(|| TransformError::FieldNotFound(field_name.to_string()))?;
    let vectors = field
        .as_vector()
        .ok_or_else(|| TransformError::WrongFieldShape {
            field: field_name.to_string(),
            reason: "warp requires a 3-vector field".into(),
        })?;
    let positions: Vec<[f64; 3]> = dataset
        .positions
        .iter()
        .zip(vectors.iter())
        .map(|(p, v)| {
            [
                p[0] + scale * v[0],
                p[1] + scale * v[1],
                p[2] + scale * v[2],
            ]
        })
        .collect();
    let mut result = dataset.clone();
    result.positions = positions.into();
    Ok(result)
}

/// Displaces points along their (area-weighted) normals by a scalar field.
///
/// Normals come from the boundary triangles of the dataset, so this works
/// for surface and volume datasets alike: `p' = p + scale * value * n`.
///
/// # Errors
///
/// Returns an error when the field is missing or not scalar.
pub fn warp_by_scalar(
    dataset: &DataSet,
    field_name: &str,
    scale: f64,
) -> Result<DataSet, TransformError> {
    let field = dataset
        .field(Association::Point, field_name)
        .ok_or_else(|| TransformError::FieldNotFound(field_name.to_string()))?;
    let FieldData::Scalar(values) = &field.data else {
        return Err(TransformError::WrongFieldShape {
            field: field_name.to_string(),
            reason: "warp requires a scalar field".into(),
        });
    };
    let triangles = gpui_mesh::surface_chunks(dataset, usize::MAX);
    let mut normals = vec![[0.0f64; 3]; dataset.point_count()];
    for chunk in &triangles {
        for tri in chunk.indices.as_chunks::<3>().0 {
            let ids = [
                chunk.point_ids[tri[0] as usize] as usize,
                chunk.point_ids[tri[1] as usize] as usize,
                chunk.point_ids[tri[2] as usize] as usize,
            ];
            let [a, b, c] = [
                dataset.positions[ids[0]],
                dataset.positions[ids[1]],
                dataset.positions[ids[2]],
            ];
            let n = cross(sub(b, a), sub(c, a));
            for id in ids {
                normals[id][0] += n[0];
                normals[id][1] += n[1];
                normals[id][2] += n[2];
            }
        }
    }
    let positions: Vec<[f64; 3]> = dataset
        .positions
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let n = normals[i];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len == 0.0 {
                return *p;
            }
            let s = scale * values[i] / len;
            [p[0] + s * n[0], p[1] + s * n[1], p[2] + s * n[2]]
        })
        .collect();
    let mut result = dataset.clone();
    result.positions = positions.into();
    Ok(result)
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_mesh::Field;

    #[test]
    fn warps_along_vectors() {
        let ds = DataSet::surface(
            "s",
            vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[[0, 1, 2]],
        )
        .with_field(Field::vector("d", vec![[0.0, 0.0, 1.0]; 3]));
        let out = warp_by_vector(&ds, "d", 2.0).unwrap();
        assert_eq!(out.positions[0], [0.0, 0.0, 2.0]);
    }
}
