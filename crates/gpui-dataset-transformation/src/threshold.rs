//! Scalar-range cell selection.

// Rust guideline compliant 2026-02-21

use crate::TransformError;
use gpui_mesh::{Association, Cells, DataSet, Field};

/// How point values decide whether a cell survives a threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdMode {
    /// Every point (or the cell value) must lie in range.
    All,
    /// At least one point (or the cell value) must lie in range.
    Any,
}

/// Keeps cells whose field values lie in `[lower, upper]`.
///
/// Cell fields test the single cell value. Point fields test the cell's
/// nodes under `mode`. Retained cells are remapped onto the points they
/// still use; point and cell fields are subset consistently.
///
/// # Errors
///
/// Returns an error when the field is missing, not scalar, or the dataset
/// is invalid.
pub fn threshold(
    dataset: &DataSet,
    field_name: &str,
    association: Association,
    lower: f64,
    upper: f64,
    mode: ThresholdMode,
) -> Result<DataSet, TransformError> {
    dataset.validate()?;
    let field: &Field = dataset
        .field(association, field_name)
        .ok_or_else(|| TransformError::FieldNotFound(field_name.to_string()))?;
    let values = field
        .as_scalar()
        .ok_or_else(|| TransformError::WrongFieldShape {
            field: field_name.to_string(),
            reason: "threshold requires a scalar field".into(),
        })?;
    let in_range = |v: f64| v >= lower && v <= upper;

    let mut keep = vec![false; dataset.cell_count()];
    for cell in 0..dataset.cell_count() {
        keep[cell] = match association {
            Association::Cell => in_range(values[cell]),
            Association::Point => {
                let nodes = dataset.cells.cell(cell).unwrap_or(&[]);
                match mode {
                    ThresholdMode::All => nodes.iter().all(|&n| in_range(values[n as usize])),
                    ThresholdMode::Any => nodes.iter().any(|&n| in_range(values[n as usize])),
                }
            }
        };
    }
    Ok(subset(dataset, &keep))
}

/// Extracts the sub-dataset of kept cells, remapping points and fields.
pub(crate) fn subset(dataset: &DataSet, keep: &[bool]) -> DataSet {
    let mut remap = vec![u32::MAX; dataset.point_count()];
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut types = Vec::new();
    let mut cell_nodes: Vec<Vec<u32>> = Vec::new();
    for (cell, kept) in keep.iter().enumerate() {
        if !kept {
            continue;
        }
        types.push(dataset.cells.types[cell]);
        let nodes = dataset.cells.cell(cell).unwrap_or(&[]);
        let mapped: Vec<u32> = nodes
            .iter()
            .map(|&n| {
                if remap[n as usize] == u32::MAX {
                    remap[n as usize] = positions.len() as u32;
                    positions.push(dataset.positions[n as usize]);
                }
                remap[n as usize]
            })
            .collect();
        cell_nodes.push(mapped);
    }
    let refs: Vec<(gpui_mesh::CellType, &[u32])> = types
        .iter()
        .copied()
        .zip(cell_nodes.iter().map(Vec::as_slice))
        .collect();
    let mut result = DataSet::new(
        format!("{}-subset", dataset.id),
        positions,
        Cells::from_cells(&refs),
    );
    for field in &dataset.point_data {
        result.point_data.push(subset_field(field, &remap));
    }
    let kept_indices: Vec<usize> = keep
        .iter()
        .enumerate()
        .filter_map(|(i, k)| k.then_some(i))
        .collect();
    for field in &dataset.cell_data {
        result
            .cell_data
            .push(subset_cell_field(field, &kept_indices));
    }
    result.id = dataset.id.clone();
    result
}

fn subset_field(field: &Field, remap: &[u32]) -> Field {
    let mut order: Vec<usize> = (0..remap.len()).filter(|&i| remap[i] != u32::MAX).collect();
    order.sort_by_key(|&i| remap[i]);
    pick(field, &order)
}

fn subset_cell_field(field: &Field, kept: &[usize]) -> Field {
    pick(field, kept)
}

fn pick(field: &Field, order: &[usize]) -> Field {
    let data = match &field.data {
        gpui_mesh::FieldData::Scalar(v) => {
            gpui_mesh::FieldData::Scalar(order.iter().map(|&i| v[i]).collect::<Vec<_>>().into())
        }
        gpui_mesh::FieldData::Vector(v) => {
            gpui_mesh::FieldData::Vector(order.iter().map(|&i| v[i]).collect::<Vec<_>>().into())
        }
        gpui_mesh::FieldData::Tensor(v) => {
            gpui_mesh::FieldData::Tensor(order.iter().map(|&i| v[i]).collect::<Vec<_>>().into())
        }
        gpui_mesh::FieldData::Complex(v) => {
            gpui_mesh::FieldData::Complex(order.iter().map(|&i| v[i]).collect::<Vec<_>>().into())
        }
    };
    let valid = field
        .valid
        .as_ref()
        .map(|mask| order.iter().map(|&i| mask[i]).collect::<Vec<_>>().into());
    Field {
        name: field.name.clone(),
        label: field.label.clone(),
        unit: field.unit.clone(),
        association: field.association,
        data,
        valid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_mesh::Field;

    #[test]
    fn thresholds_cells_by_point_field() {
        let ds = DataSet::surface(
            "s",
            vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [2.0, 0.0, 0.0]],
            &[[0, 1, 2], [1, 3, 2]],
        )
        .with_field(Field::scalar("p", vec![0.0, 1.0, 2.0, 10.0]));
        let out = threshold(&ds, "p", Association::Point, 0.0, 2.0, ThresholdMode::All).unwrap();
        assert_eq!(out.cell_count(), 1);
        assert_eq!(out.validate(), Ok(()));
    }
}
