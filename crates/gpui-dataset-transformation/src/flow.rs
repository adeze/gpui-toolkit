//! Flow diagnostics: velocity gradient, vorticity, Q-criterion, divergence.
//!
//! Gradients are computed per tetrahedron (constant over the element from
//! its shape functions) and averaged to points weighted by element volume.
//! Hexahedra, wedges, and pyramids are tetrahedralized consistently with
//! the isosurface filter, so all diagnostics share one decomposition.

// Rust guideline compliant 2026-02-21

use crate::TransformError;
use gpui_mesh::{Association, CellType, DataSet, Field, FieldData};

fn tetrahedralize(kind: CellType, n: &[u32]) -> Vec<[u32; 4]> {
    match kind {
        CellType::Tetra => vec![[n[0], n[1], n[2], n[3]]],
        CellType::Hexahedron => vec![
            [n[0], n[1], n[2], n[6]],
            [n[0], n[2], n[3], n[6]],
            [n[0], n[3], n[7], n[6]],
            [n[0], n[7], n[4], n[6]],
            [n[0], n[4], n[5], n[6]],
            [n[0], n[5], n[1], n[6]],
        ],
        CellType::Wedge => vec![
            [n[0], n[1], n[2], n[3]],
            [n[1], n[2], n[3], n[4]],
            [n[2], n[3], n[4], n[5]],
        ],
        CellType::Pyramid => vec![[n[0], n[1], n[2], n[4]], [n[0], n[2], n[3], n[4]]],
        _ => Vec::new(),
    }
}

/// Solves a 3x3 system by Cramer's rule; `None` for a singular matrix.
fn solve3(m: [[f64; 3]; 3], rhs: [f64; 3]) -> Option<[f64; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-300 {
        return None;
    }
    let replace = |col: usize| {
        let mut mm = m;
        for row in 0..3 {
            mm[row][col] = rhs[row];
        }
        (mm[0][0] * (mm[1][1] * mm[2][2] - mm[1][2] * mm[2][1])
            - mm[0][1] * (mm[1][0] * mm[2][2] - mm[1][2] * mm[2][0])
            + mm[0][2] * (mm[1][0] * mm[2][1] - mm[1][1] * mm[2][0]))
            / det
    };
    Some([replace(0), replace(1), replace(2)])
}

/// Per-point velocity Jacobian `J[component][axis]`, volume-weighted.
///
/// Returns `(jacobian, volume)` per point; volume is the accumulated
/// element weight and is zero for points outside every volume cell.
fn point_jacobian(dataset: &DataSet, vectors: &[[f64; 3]]) -> Vec<([[f64; 3]; 3], f64)> {
    let mut acc = vec![([[0.0; 3]; 3], 0.0f64); dataset.point_count()];
    for cell in 0..dataset.cell_count() {
        let kind = dataset.cells.types[cell];
        if !kind.is_volume() {
            continue;
        }
        let nodes = dataset.cells.cell(cell).unwrap_or(&[]);
        for tet in tetrahedralize(kind, nodes) {
            let p = [
                dataset.positions[tet[0] as usize],
                dataset.positions[tet[1] as usize],
                dataset.positions[tet[2] as usize],
                dataset.positions[tet[3] as usize],
            ];
            // Edge matrix columns are p_i - p_0; its determinant is 6*volume.
            let m = [
                [p[1][0] - p[0][0], p[2][0] - p[0][0], p[3][0] - p[0][0]],
                [p[1][1] - p[0][1], p[2][1] - p[0][1], p[3][1] - p[0][1]],
                [p[1][2] - p[0][2], p[2][2] - p[0][2], p[3][2] - p[0][2]],
            ];
            let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
            let volume = det.abs() / 6.0;
            if volume < 1e-300 {
                continue;
            }
            // Gradient of each component: solve m^T g = dv (m rows are axes).
            let mut jac = [[0.0; 3]; 3];
            let mut valid = true;
            for component in 0..3 {
                let v0 = vectors[tet[0] as usize][component];
                let dv = [
                    vectors[tet[1] as usize][component] - v0,
                    vectors[tet[2] as usize][component] - v0,
                    vectors[tet[3] as usize][component] - v0,
                ];
                // m is stored [axis][edge]; gradient solves sum_e m[axis][e] * x_e = dv[axis]
                // with x holding derivatives along edges, then g_axis = x combined
                // through the transpose system: solve m^T * g = dv.
                let mt = [
                    [m[0][0], m[1][0], m[2][0]],
                    [m[0][1], m[1][1], m[2][1]],
                    [m[0][2], m[1][2], m[2][2]],
                ];
                if let Some(g) = solve3(mt, dv) {
                    jac[component] = g;
                } else {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }
            for &node in &tet {
                let entry = &mut acc[node as usize];
                for (component, row) in jac.iter().enumerate() {
                    for (axis, value) in row.iter().enumerate() {
                        entry.0[component][axis] += value * volume;
                    }
                }
                entry.1 += volume;
            }
        }
    }
    for (jac, volume) in &mut acc {
        if *volume > 0.0 {
            for row in jac.iter_mut() {
                for value in row.iter_mut() {
                    *value /= *volume;
                }
            }
        }
    }
    acc
}

fn velocity_field<'a>(
    dataset: &'a DataSet,
    field_name: &str,
) -> Result<&'a [[f64; 3]], TransformError> {
    let field = dataset
        .field(Association::Point, field_name)
        .ok_or_else(|| TransformError::FieldNotFound(field_name.to_string()))?;
    field
        .as_vector()
        .ok_or_else(|| TransformError::WrongFieldShape {
            field: field_name.to_string(),
            reason: "flow diagnostics require a 3-vector field".into(),
        })
}

fn append(dataset: &DataSet, field: Field) -> DataSet {
    let mut result = dataset.clone();
    result.point_data.retain(|f| f.name != field.name);
    result.point_data.push(field);
    result
}

/// Appends vorticity `curl(v)` as a vector point field.
///
/// # Errors
///
/// Returns an error when the field is missing, not a vector field, or the
/// dataset holds no volume cells.
pub fn vorticity(
    dataset: &DataSet,
    field_name: &str,
    out_name: &str,
) -> Result<DataSet, TransformError> {
    let vectors = velocity_field(dataset, field_name)?;
    let jacobian = point_jacobian(dataset, vectors);
    let values: Vec<[f64; 3]> = jacobian
        .iter()
        .map(|(j, _)| [j[2][1] - j[1][2], j[0][2] - j[2][0], j[1][0] - j[0][1]])
        .collect();
    let mut field = Field::vector(out_name.to_string(), values);
    field.label = "Vorticity".into();
    field.unit = Some("1/s".into());
    Ok(append(dataset, field))
}

/// Appends divergence `tr(J)` as a scalar point field.
///
/// # Errors
///
/// Returns an error when the field is missing or not a vector field.
pub fn divergence(
    dataset: &DataSet,
    field_name: &str,
    out_name: &str,
) -> Result<DataSet, TransformError> {
    let vectors = velocity_field(dataset, field_name)?;
    let jacobian = point_jacobian(dataset, vectors);
    let values: Vec<f64> = jacobian
        .iter()
        .map(|(j, _)| j[0][0] + j[1][1] + j[2][2])
        .collect();
    let mut field = Field::scalar(out_name.to_string(), values);
    field.label = "Divergence".into();
    field.unit = Some("1/s".into());
    Ok(append(dataset, field))
}

/// Appends the Q-criterion `0.5*(||Omega||^2 - ||S||^2)` as a scalar field.
///
/// Positive Q marks rotation-dominated regions (vortex cores), which is
/// the standard field for Navier-Stokes vortex movies.
///
/// # Errors
///
/// Returns an error when the field is missing or not a vector field.
pub fn q_criterion(
    dataset: &DataSet,
    field_name: &str,
    out_name: &str,
) -> Result<DataSet, TransformError> {
    let vectors = velocity_field(dataset, field_name)?;
    let jacobian = point_jacobian(dataset, vectors);
    let values: Vec<f64> = jacobian
        .iter()
        .map(|(j, _)| {
            let mut omega2 = 0.0;
            let mut strain2 = 0.0;
            for (a, row) in j.iter().enumerate() {
                for (b, &jab) in row.iter().enumerate() {
                    let omega = 0.5 * (jab - j[b][a]);
                    let strain = f64::midpoint(jab, j[b][a]);
                    omega2 += omega * omega;
                    strain2 += strain * strain;
                }
            }
            0.5 * (omega2 - strain2)
        })
        .collect();
    let mut field = Field::scalar(out_name.to_string(), values);
    field.label = "Q-criterion".into();
    field.unit = Some("1/s^2".into());
    Ok(append(dataset, field))
}

/// Appends vorticity magnitude as a scalar field (convenience composition).
///
/// # Errors
///
/// Returns an error when the field is missing or not a vector field.
pub fn vorticity_magnitude(
    dataset: &DataSet,
    field_name: &str,
    out_name: &str,
) -> Result<DataSet, TransformError> {
    let with_vorticity = vorticity(dataset, field_name, "__vorticity_tmp")?;
    let field = with_vorticity
        .field(Association::Point, "__vorticity_tmp")
        .ok_or_else(|| TransformError::FieldNotFound("__vorticity_tmp".into()))?;
    let FieldData::Vector(vectors) = &field.data else {
        return Err(TransformError::WrongFieldShape {
            field: field_name.to_string(),
            reason: "expected a vector field".into(),
        });
    };
    let values: Vec<f64> = vectors
        .iter()
        .map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt())
        .collect();
    let mut result = with_vorticity;
    result
        .point_data
        .retain(|f| f.name.as_ref() != "__vorticity_tmp");
    let mut field = Field::scalar(out_name.to_string(), values);
    field.label = "Vorticity magnitude".into();
    field.unit = Some("1/s".into());
    result.point_data.push(field);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Solid-body rotation v = Omega x r with Omega = (0,0,2):
    /// vorticity is exactly (0,0,4), divergence 0, Q = |Omega|^2 = 4...
    /// For v = (-2y, 2x, 0): J = [[0,-2,0],[2,0,0],[0,0,0]], Omega tensor
    /// equals J antisymmetric part, ||Omega||^2 = 8, S = 0, so Q = 4.
    fn rotating() -> DataSet {
        let mut ds = DataSet::structured_grid("g", [4, 4, 4], [-1.0; 3], [0.5; 3]).unwrap();
        let velocity: Vec<[f64; 3]> = ds
            .positions
            .iter()
            .map(|p| [-2.0 * p[1], 2.0 * p[0], 0.0])
            .collect();
        ds.point_data.push(Field::vector("velocity", velocity));
        ds
    }

    #[test]
    fn solid_rotation_has_exact_vorticity_and_q() {
        let ds = rotating();
        let out = vorticity(&ds, "velocity", "vorticity").unwrap();
        let values = out
            .field(Association::Point, "vorticity")
            .unwrap()
            .as_vector()
            .unwrap();
        // Interior points average symmetric tets; check the centre point.
        let centre = ds
            .positions
            .iter()
            .position(|p| *p == [0.0, 0.0, 0.0])
            .unwrap();
        assert!(
            (values[centre][2] - 4.0).abs() < 1e-9,
            "got {:?}",
            values[centre]
        );
        assert!(
            values
                .iter()
                .all(|v| v[2] > 3.5 && v[0].abs() < 0.6 && v[1].abs() < 0.6)
        );

        let q = q_criterion(&ds, "velocity", "q").unwrap();
        let qv = q
            .field(Association::Point, "q")
            .unwrap()
            .as_scalar()
            .unwrap();
        assert!((qv[centre] - 4.0).abs() < 1e-9, "got {}", qv[centre]);

        let div = divergence(&ds, "velocity", "div").unwrap();
        let dv = div
            .field(Association::Point, "div")
            .unwrap()
            .as_scalar()
            .unwrap();
        assert!(dv.iter().all(|v| v.abs() < 1e-9));
    }

    #[test]
    fn pure_strain_has_negative_q() {
        // v = (2x, -2y, 0): strain only, Q = -||S||^2 /... = -4.
        let mut ds = DataSet::structured_grid("g", [4, 4, 4], [-1.0; 3], [0.5; 3]).unwrap();
        let velocity: Vec<[f64; 3]> = ds
            .positions
            .iter()
            .map(|p| [2.0 * p[0], -2.0 * p[1], 0.0])
            .collect();
        ds.point_data.push(Field::vector("velocity", velocity));
        let q = q_criterion(&ds, "velocity", "q").unwrap();
        let qv = q
            .field(Association::Point, "q")
            .unwrap()
            .as_scalar()
            .unwrap();
        let centre = ds
            .positions
            .iter()
            .position(|p| *p == [0.0, 0.0, 0.0])
            .unwrap();
        assert!((qv[centre] + 4.0).abs() < 1e-9, "got {}", qv[centre]);
    }
}
