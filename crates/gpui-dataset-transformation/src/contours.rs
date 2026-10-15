//! Contour (isoline) extraction on triangle surfaces.
//!
//! Pair with a banded transfer function in `gpui-scene3d` for filled
//! contours: colour a slice with `TransferFunction::banded` and overlay
//! these lines at the band boundaries.

// Rust guideline compliant 2026-02-21

use crate::TransformError;
use gpui_mesh::{Association, CellType, Cells, DataSet, Field};

/// Extracts isolines of a scalar point field at the given levels.
///
/// Operates on triangle cells (quads are split on their diagonal); volume
/// cells are ignored, so slice or extract a surface first. The output
/// holds line cells plus a scalar `value` point field carrying the level
/// of each vertex, ready for colouring.
///
/// # Errors
///
/// Returns an error when the field is missing or not scalar.
pub fn contour_lines(
    dataset: &DataSet,
    field_name: &str,
    levels: &[f64],
) -> Result<DataSet, TransformError> {
    let field = dataset
        .field(Association::Point, field_name)
        .ok_or_else(|| TransformError::FieldNotFound(field_name.to_string()))?;
    let values = field
        .as_scalar()
        .ok_or_else(|| TransformError::WrongFieldShape {
            field: field_name.to_string(),
            reason: "contours require a scalar field".into(),
        })?;
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut level_values: Vec<f64> = Vec::new();
    let mut cells: Vec<(CellType, Vec<u32>)> = Vec::new();
    let push = |p: [f64; 3], positions: &mut Vec<[f64; 3]>| -> u32 {
        let id = positions.len() as u32;
        positions.push(p);
        id
    };
    for cell in 0..dataset.cell_count() {
        let nodes = dataset.cells.cell(cell).unwrap_or(&[]);
        let triangles: Vec<[u32; 3]> = match dataset.cells.types[cell] {
            CellType::Triangle if nodes.len() == 3 => vec![[nodes[0], nodes[1], nodes[2]]],
            CellType::Quad if nodes.len() == 4 => {
                vec![
                    [nodes[0], nodes[1], nodes[2]],
                    [nodes[0], nodes[2], nodes[3]],
                ]
            }
            _ => Vec::new(),
        };
        for tri in triangles {
            let pts: Vec<[f64; 3]> = tri.iter().map(|&n| dataset.positions[n as usize]).collect();
            let vals: Vec<f64> = tri.iter().map(|&n| values[n as usize]).collect();
            for &level in levels {
                // Marching triangles: collect edge crossings; a segment
                // exists when exactly two edges cross the level.
                let mut crossings: Vec<[f64; 3]> = Vec::new();
                for e in 0..3 {
                    let (a, b) = (e, (e + 1) % 3);
                    let (va, vb) = (vals[a], vals[b]);
                    if (va < level && vb >= level) || (vb < level && va >= level) {
                        let t = (level - va) / (vb - va);
                        crossings.push([
                            pts[a][0] + (pts[b][0] - pts[a][0]) * t,
                            pts[a][1] + (pts[b][1] - pts[a][1]) * t,
                            pts[a][2] + (pts[b][2] - pts[a][2]) * t,
                        ]);
                    }
                }
                if crossings.len() == 2 {
                    let a = push(crossings[0], &mut positions);
                    let b = push(crossings[1], &mut positions);
                    level_values.push(level);
                    level_values.push(level);
                    cells.push((CellType::Line, vec![a, b]));
                }
            }
        }
    }
    let refs: Vec<(CellType, &[u32])> = cells.iter().map(|(t, n)| (*t, n.as_slice())).collect();
    Ok(DataSet::new(
        format!("{}-contours", dataset.id),
        positions,
        Cells::from_cells(&refs),
    )
    .with_field(Field::scalar("value", level_values)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_level_crosses_both_triangles() {
        let ds = DataSet::surface(
            "s",
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            &[[0, 1, 2], [0, 2, 3]],
        )
        .with_field(Field::scalar("s", vec![0.0, 1.0, 1.0, 0.0]));
        let out = contour_lines(&ds, "s", &[0.5]).unwrap();
        assert_eq!(out.cell_count(), 2);
        for p in out.positions.iter() {
            assert!((p[0] - 0.5).abs() < 1e-12);
        }
        let value = out
            .field(Association::Point, "value")
            .unwrap()
            .as_scalar()
            .unwrap();
        assert!(value.iter().all(|&v| v == 0.5));
        assert_eq!(out.validate(), Ok(()));
    }

    #[test]
    fn level_outside_range_emits_nothing() {
        let ds = DataSet::surface(
            "s",
            vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[[0, 1, 2]],
        )
        .with_field(Field::scalar("s", vec![0.0, 1.0, 1.0]));
        let out = contour_lines(&ds, "s", &[2.0]).unwrap();
        assert_eq!(out.cell_count(), 0);
    }
}
