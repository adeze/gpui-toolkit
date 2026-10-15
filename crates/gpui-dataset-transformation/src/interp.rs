//! Point-field interpolation from sparse per-point weights.

// Rust guideline compliant 2026-02-21

use gpui_mesh::{DataSet, Field, FieldData};

/// Sparse interpolation weights for one output point.
pub(crate) type Weights = Vec<(u32, f64)>;

/// Single-source weights for an unmodified dataset point.
pub(crate) fn single(point: u32) -> Weights {
    vec![(point, 1.0)]
}

/// Blends two weight sets at factor `t` (0 = `a`, 1 = `b`).
pub(crate) fn blend(a: &Weights, b: &Weights, t: f64) -> Weights {
    let mut out: Weights = a.iter().map(|(i, w)| (*i, w * (1.0 - t))).collect();
    for (i, w) in b {
        let scaled = w * t;
        if let Some(entry) = out.iter_mut().find(|(j, _)| j == i) {
            entry.1 += scaled;
        } else {
            out.push((*i, scaled));
        }
    }
    out
}

/// Interpolates every point field of `dataset` onto output points.
///
/// Cell fields cannot survive a filter that creates new points, so only
/// point fields are produced. A validity mask entry is false when any
/// contributing source entry is masked.
pub(crate) fn interpolate_point_fields(dataset: &DataSet, weights: &[Weights]) -> Vec<Field> {
    dataset
        .point_data
        .iter()
        .map(|field| {
            let data = match &field.data {
                FieldData::Scalar(values) => FieldData::Scalar(
                    weights
                        .iter()
                        .map(|w| w.iter().map(|(i, f)| f * values[*i as usize]).sum())
                        .collect::<Vec<_>>()
                        .into(),
                ),
                FieldData::Vector(values) => FieldData::Vector(
                    weights
                        .iter()
                        .map(|w| {
                            let mut out = [0.0; 3];
                            for (i, f) in w {
                                let v = values[*i as usize];
                                out[0] += f * v[0];
                                out[1] += f * v[1];
                                out[2] += f * v[2];
                            }
                            out
                        })
                        .collect::<Vec<_>>()
                        .into(),
                ),
                FieldData::Tensor(values) => FieldData::Tensor(
                    weights
                        .iter()
                        .map(|w| {
                            let mut out = [0.0; 6];
                            for (i, f) in w {
                                let v = values[*i as usize];
                                for (c, component) in out.iter_mut().enumerate() {
                                    *component += f * v[c];
                                }
                            }
                            out
                        })
                        .collect::<Vec<_>>()
                        .into(),
                ),
                FieldData::Complex(values) => FieldData::Complex(
                    weights
                        .iter()
                        .map(|w| {
                            let mut out = [0.0; 2];
                            for (i, f) in w {
                                let v = values[*i as usize];
                                out[0] += f * v[0];
                                out[1] += f * v[1];
                            }
                            out
                        })
                        .collect::<Vec<_>>()
                        .into(),
                ),
            };
            let valid = field.valid.as_ref().map(|mask| {
                weights
                    .iter()
                    .map(|w| w.iter().all(|(i, _)| mask[*i as usize]))
                    .collect::<Vec<_>>()
                    .into()
            });
            Field {
                name: field.name.clone(),
                label: field.label.clone(),
                unit: field.unit.clone(),
                association: field.association,
                data,
                valid,
            }
        })
        .collect()
}
