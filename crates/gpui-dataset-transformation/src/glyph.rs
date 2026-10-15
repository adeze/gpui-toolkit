//! Vector glyphs: arrows placed at points, scaled by a vector field.

// Rust guideline compliant 2026-02-21

use crate::TransformError;
use gpui_mesh::{Association, CellType, Cells, DataSet};

/// Glyph shape emitted per point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphKind {
    /// A line from the point along the vector, with a small head.
    Arrow,
    /// A line in both directions (for undirected orientation fields).
    Line,
}

/// Builds a glyph dataset for a vector point field.
///
/// Every `stride`-th point receives one glyph, scaled by `scale`. The
/// output holds only line cells, so it renders cheaply even for dense
/// fields and overlays directly on the source surface.
///
/// # Errors
///
/// Returns an error when the field is missing or not a vector field.
pub fn glyphs(
    dataset: &DataSet,
    field_name: &str,
    scale: f64,
    stride: usize,
    kind: GlyphKind,
) -> Result<DataSet, TransformError> {
    let field = dataset
        .field(Association::Point, field_name)
        .ok_or_else(|| TransformError::FieldNotFound(field_name.to_string()))?;
    let vectors = field
        .as_vector()
        .ok_or_else(|| TransformError::WrongFieldShape {
            field: field_name.to_string(),
            reason: "glyphs require a 3-vector field".into(),
        })?;
    let stride = stride.max(1);
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut cells: Vec<(CellType, Vec<u32>)> = Vec::new();
    let push = |p: [f64; 3], positions: &mut Vec<[f64; 3]>| -> u32 {
        let id = positions.len() as u32;
        positions.push(p);
        id
    };
    for (i, (origin, v)) in dataset.positions.iter().zip(vectors.iter()).enumerate() {
        if i % stride != 0 {
            continue;
        }
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len == 0.0 || !len.is_finite() {
            continue;
        }
        let dir = [v[0] / len, v[1] / len, v[2] / len];
        let tip = [
            origin[0] + scale * v[0],
            origin[1] + scale * v[1],
            origin[2] + scale * v[2],
        ];
        match kind {
            GlyphKind::Arrow => {
                let base = push(*origin, &mut positions);
                let tip_id = push(tip, &mut positions);
                cells.push((CellType::Line, vec![base, tip_id]));
                // Arrow head: two short segments at 150 degrees in an
                // arbitrary plane containing the direction.
                let side = perpendicular(dir);
                let head = scale * len * 0.25;
                for s in [1.0, -1.0] {
                    let back = [
                        tip[0] - dir[0] * head + s * side[0] * head * 0.5,
                        tip[1] - dir[1] * head + s * side[1] * head * 0.5,
                        tip[2] - dir[2] * head + s * side[2] * head * 0.5,
                    ];
                    let back_id = push(back, &mut positions);
                    cells.push((CellType::Line, vec![tip_id, back_id]));
                }
            }
            GlyphKind::Line => {
                let a = [
                    origin[0] - scale * v[0] * 0.5,
                    origin[1] - scale * v[1] * 0.5,
                    origin[2] - scale * v[2] * 0.5,
                ];
                let a_id = push(a, &mut positions);
                let b_id = push(tip, &mut positions);
                cells.push((CellType::Line, vec![a_id, b_id]));
            }
        }
    }
    let refs: Vec<(CellType, &[u32])> = cells.iter().map(|(t, n)| (*t, n.as_slice())).collect();
    Ok(DataSet::new(
        format!("{}-glyphs", dataset.id),
        positions,
        Cells::from_cells(&refs),
    ))
}

fn perpendicular(dir: [f64; 3]) -> [f64; 3] {
    let a = if dir[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let c = [
        dir[1] * a[2] - dir[2] * a[1],
        dir[2] * a[0] - dir[0] * a[2],
        dir[0] * a[1] - dir[1] * a[0],
    ];
    let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    if len == 0.0 {
        return [0.0; 3];
    }
    [c[0] / len, c[1] / len, c[2] / len]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_mesh::Field;

    #[test]
    fn arrows_emit_three_segments_per_point() {
        let ds = DataSet::surface(
            "s",
            vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[[0, 1, 2]],
        )
        .with_field(Field::vector("v", vec![[1.0, 0.0, 0.0]; 3]));
        let out = glyphs(&ds, "v", 0.5, 1, GlyphKind::Arrow).unwrap();
        assert_eq!(out.cell_count(), 9);
        assert_eq!(out.validate(), Ok(()));
    }
}
