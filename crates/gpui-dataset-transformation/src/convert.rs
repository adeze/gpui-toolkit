//! Point <-> cell field conversion.
//!
//! `point_data_to_cell_data` averages node values per cell (the VTK
//! `PointDataToCellData`); `cell_data_to_point_data` averages the cells
//! touching a point weighted by cell measure (volume or area), the VTK
//! `CellDataToPointData`. Scalar and vector fields are supported.

// Rust guideline compliant 2026-02-21

use crate::TransformError;
use gpui_mesh::{Association, CellType, DataSet, Field, FieldData};

/// Converts a point field to a cell field by node averaging.
///
/// # Errors
///
/// Returns an error when the field is missing or has an unsupported shape.
pub fn point_data_to_cell_data(
    dataset: &DataSet,
    field_name: &str,
    out_name: &str,
) -> Result<DataSet, TransformError> {
    let field = dataset
        .field(Association::Point, field_name)
        .ok_or_else(|| TransformError::FieldNotFound(field_name.to_string()))?;
    let mut out = dataset.clone();
    match &field.data {
        FieldData::Scalar(values) => {
            let converted: Vec<f64> = (0..dataset.cell_count())
                .map(|cell| {
                    let nodes = dataset.cells.cell(cell).unwrap_or(&[]);
                    if nodes.is_empty() {
                        return 0.0;
                    }
                    nodes.iter().map(|&n| values[n as usize]).sum::<f64>() / nodes.len() as f64
                })
                .collect();
            out.cell_data
                .push(Field::scalar(out_name.to_string(), converted).on_cells());
        }
        FieldData::Vector(values) => {
            let converted: Vec<[f64; 3]> = (0..dataset.cell_count())
                .map(|cell| {
                    let nodes = dataset.cells.cell(cell).unwrap_or(&[]);
                    if nodes.is_empty() {
                        return [0.0; 3];
                    }
                    let mut acc = [0.0; 3];
                    for &n in nodes {
                        let v = values[n as usize];
                        acc[0] += v[0];
                        acc[1] += v[1];
                        acc[2] += v[2];
                    }
                    let inv = 1.0 / nodes.len() as f64;
                    [acc[0] * inv, acc[1] * inv, acc[2] * inv]
                })
                .collect();
            out.cell_data
                .push(Field::vector(out_name.to_string(), converted).on_cells());
        }
        _ => {
            return Err(TransformError::WrongFieldShape {
                field: field_name.to_string(),
                reason: "conversion supports scalar and vector fields".into(),
            });
        }
    }
    Ok(out)
}

/// Converts a cell field to a point field, weighted by cell measure.
///
/// Volume cells weight by volume, surface cells by area; points touched
/// by no cell get zero.
///
/// # Errors
///
/// Returns an error when the field is missing or has an unsupported shape.
pub fn cell_data_to_point_data(
    dataset: &DataSet,
    field_name: &str,
    out_name: &str,
) -> Result<DataSet, TransformError> {
    let field = dataset
        .field(Association::Cell, field_name)
        .ok_or_else(|| TransformError::FieldNotFound(field_name.to_string()))?;
    let weights: Vec<f64> = (0..dataset.cell_count())
        .map(|cell| cell_measure(dataset, cell))
        .collect();
    let mut out = dataset.clone();
    match &field.data {
        FieldData::Scalar(values) => {
            let mut acc = vec![0.0f64; dataset.point_count()];
            let mut wsum = vec![0.0f64; dataset.point_count()];
            for cell in 0..dataset.cell_count() {
                let w = weights[cell].max(1e-300);
                for &n in dataset.cells.cell(cell).unwrap_or(&[]) {
                    acc[n as usize] += values[cell] * w;
                    wsum[n as usize] += w;
                }
            }
            let converted: Vec<f64> = acc
                .iter()
                .zip(wsum.iter())
                .map(|(a, w)| if *w > 0.0 { a / w } else { 0.0 })
                .collect();
            out.point_data
                .push(Field::scalar(out_name.to_string(), converted));
        }
        FieldData::Vector(values) => {
            let mut acc = vec![[0.0f64; 3]; dataset.point_count()];
            let mut wsum = vec![0.0f64; dataset.point_count()];
            for cell in 0..dataset.cell_count() {
                let w = weights[cell].max(1e-300);
                for &n in dataset.cells.cell(cell).unwrap_or(&[]) {
                    let v = values[cell];
                    acc[n as usize][0] += v[0] * w;
                    acc[n as usize][1] += v[1] * w;
                    acc[n as usize][2] += v[2] * w;
                    wsum[n as usize] += w;
                }
            }
            let converted: Vec<[f64; 3]> = acc
                .iter()
                .zip(wsum.iter())
                .map(|(a, w)| {
                    if *w > 0.0 {
                        [a[0] / w, a[1] / w, a[2] / w]
                    } else {
                        [0.0; 3]
                    }
                })
                .collect();
            out.point_data
                .push(Field::vector(out_name.to_string(), converted));
        }
        _ => {
            return Err(TransformError::WrongFieldShape {
                field: field_name.to_string(),
                reason: "conversion supports scalar and vector fields".into(),
            });
        }
    }
    Ok(out)
}

/// Cell measure: volume for volume cells, area for surface cells.
fn cell_measure(dataset: &DataSet, cell: usize) -> f64 {
    let nodes = dataset.cells.cell(cell).unwrap_or(&[]);
    if nodes.len() < 3 {
        return 0.0;
    }
    let p: Vec<[f64; 3]> = nodes
        .iter()
        .map(|&n| dataset.positions[n as usize])
        .collect();
    match dataset.cells.types[cell] {
        CellType::Triangle | CellType::Quad => {
            // Fan-triangulate and sum areas.
            let mut area = 0.0;
            for i in 1..p.len() - 1 {
                area += triangle_area(p[0], p[i], p[i + 1]);
            }
            area
        }
        kind => {
            // Volume: sum the crate's consistent tetrahedralisation.
            crate::streamline::tetrahedralize(kind, nodes)
                .iter()
                .map(|tet| {
                    tet_volume(
                        dataset.positions[tet[0] as usize],
                        dataset.positions[tet[1] as usize],
                        dataset.positions[tet[2] as usize],
                        dataset.positions[tet[3] as usize],
                    )
                })
                .sum()
        }
    }
}

fn triangle_area(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    0.5 * (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt()
}

fn tet_volume(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let ad = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
    (ab[0] * (ac[1] * ad[2] - ac[2] * ad[1]) - ab[1] * (ac[0] * ad[2] - ac[2] * ad[0])
        + ab[2] * (ac[0] * ad[1] - ac[1] * ad[0]))
        .abs()
        / 6.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_recovers_cell_average() {
        let ds = DataSet::surface(
            "s",
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[[0, 1, 2]],
        )
        .with_field(Field::scalar("p", vec![0.0, 2.0, 4.0]));
        let cells = point_data_to_cell_data(&ds, "p", "p_cell").unwrap();
        let values = cells
            .field(Association::Cell, "p_cell")
            .unwrap()
            .as_scalar()
            .unwrap();
        assert_eq!(values, &[2.0]);
        let back = cell_data_to_point_data(&cells, "p_cell", "p_back").unwrap();
        let values = back
            .field(Association::Point, "p_back")
            .unwrap()
            .as_scalar()
            .unwrap();
        assert_eq!(values, &[2.0, 2.0, 2.0]);
    }
}
