//! Line-integral convolution (LIC) flow texture.
//!
//! Each point's value is the average of a deterministic white-noise field
//! sampled along the streamline through it (box filter, both directions).
//! Colouring a slice or surface by the resulting `lic` scalar field shows
//! flow-aligned streaks — the classic oil-flow look — without any texture
//! assets. The noise is a hash of integer lattice coordinates, so results
//! are identical on every platform and run.

// Rust guideline compliant 2026-02-21

use crate::TransformError;
use crate::streamline::Tracer;
use gpui_mesh::{Association, DataSet, Field};

/// Appends a `lic` scalar field: noise averaged along streamlines.
///
/// `kernel_steps` RK2 steps of `step` dataset units are taken in each
/// direction from every point, and the noise (one cell per `step`) is
/// box-averaged. `seed` decorrelates the noise between runs and fields.
/// Points whose velocity vanishes keep the raw noise value.
///
/// # Errors
///
/// Returns an error when the field is missing, not a vector field, or the
/// dataset holds no volume cells.
pub fn lic(
    dataset: &DataSet,
    field_name: &str,
    kernel_steps: usize,
    step: f64,
    seed: u64,
) -> Result<DataSet, TransformError> {
    let field = dataset
        .field(Association::Point, field_name)
        .ok_or_else(|| TransformError::FieldNotFound(field_name.to_string()))?;
    let vectors = field
        .as_vector()
        .ok_or_else(|| TransformError::WrongFieldShape {
            field: field_name.to_string(),
            reason: "LIC requires a 3-vector field".into(),
        })?;
    if step <= 0.0 {
        return Err(TransformError::Degenerate(
            "LIC step must be positive".into(),
        ));
    }
    let tracer = Tracer::new(dataset, vectors);
    if tracer.tets.is_empty() {
        return Err(TransformError::Degenerate(
            "dataset holds no volume cells".into(),
        ));
    }
    let values: Vec<f64> = dataset
        .positions
        .iter()
        .map(|&origin| {
            let mut sum = noise(origin, step, seed);
            let mut count = 1.0;
            for direction in [1.0, -1.0] {
                let mut p = origin;
                for _ in 0..kernel_steps {
                    let Some(v) = tracer.velocity(p) else { break };
                    let speed = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                    if speed < 1e-300 {
                        break;
                    }
                    let next = [
                        p[0] + direction * step * v[0] / speed,
                        p[1] + direction * step * v[1] / speed,
                        p[2] + direction * step * v[2] / speed,
                    ];
                    if tracer.velocity(next).is_none() {
                        break;
                    }
                    p = next;
                    sum += noise(p, step, seed);
                    count += 1.0;
                }
            }
            sum / count
        })
        .collect();
    let mut result = dataset.clone();
    result.point_data.retain(|f| f.name.as_ref() != "lic");
    result.point_data.push(Field::scalar("lic", values));
    Ok(result)
}

/// Deterministic value noise in `[0, 1)`: hashed lattice, trilinear blend.
fn noise(p: [f64; 3], cell: f64, seed: u64) -> f64 {
    let scaled = [p[0] / cell, p[1] / cell, p[2] / cell];
    let base = [
        scaled[0].floor() as i64,
        scaled[1].floor() as i64,
        scaled[2].floor() as i64,
    ];
    let frac = [
        scaled[0] - base[0] as f64,
        scaled[1] - base[1] as f64,
        scaled[2] - base[2] as f64,
    ];
    // Smoothstep weights keep streaks free of lattice creases.
    let w = frac.map(|f| f * f * (3.0 - 2.0 * f));
    let mut value = 0.0;
    for (i, wi) in [(0, 1.0 - w[0]), (1, w[0])] {
        for (j, wj) in [(0, 1.0 - w[1]), (1, w[1])] {
            for (k, wk) in [(0, 1.0 - w[2]), (1, w[2])] {
                let corner = hash(base[0] + i, base[1] + j, base[2] + k, seed);
                value += wi * wj * wk * corner;
            }
        }
    }
    value
}

/// Splitmix-style integer hash to `[0, 1)`.
fn hash(x: i64, y: i64, z: i64, seed: u64) -> f64 {
    let mut h = seed
        ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (z as u64).wrapping_mul(0x1656_6679_19E3_779F);
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uniform_grid() -> DataSet {
        let ds = DataSet::structured_grid("g", [9, 9, 3], [0.0; 3], [0.5, 0.5, 0.5]).unwrap();
        let count = ds.point_count();
        ds.with_field(Field::vector("v", vec![[1.0, 0.0, 0.0]; count]))
    }

    #[test]
    fn lic_is_deterministic_bounded_and_seed_dependent() {
        let ds = uniform_grid();
        let a = lic(&ds, "v", 8, 0.25, 7).unwrap();
        let b = lic(&ds, "v", 8, 0.25, 7).unwrap();
        let c = lic(&ds, "v", 8, 0.25, 8).unwrap();
        let get = |d: &DataSet| {
            d.field(Association::Point, "lic")
                .unwrap()
                .as_scalar()
                .unwrap()
                .to_vec()
        };
        assert_eq!(get(&a), get(&b));
        assert_ne!(get(&a), get(&c));
        assert!(get(&a).iter().all(|v| (0.0..1.0).contains(v)));
    }

    #[test]
    fn lic_streaks_align_with_the_flow() {
        // In uniform x-flow, neighbouring points along x must agree more
        // than neighbours along y: the box filter smears noise along x.
        let ds = uniform_grid();
        let out = lic(&ds, "v", 12, 0.25, 3).unwrap();
        let values = out
            .field(Association::Point, "lic")
            .unwrap()
            .as_scalar()
            .unwrap();
        let dims = [9usize, 9, 3];
        let idx = |i: usize, j: usize, k: usize| k * dims[0] * dims[1] + j * dims[0] + i;
        let (mut along, mut across, mut n) = (0.0, 0.0, 0.0);
        for k in 0..dims[2] {
            for j in 1..dims[1] - 1 {
                for i in 1..dims[0] - 1 {
                    along += (values[idx(i, j, k)] - values[idx(i + 1, j, k)]).abs();
                    across += (values[idx(i, j, k)] - values[idx(i, j + 1, k)]).abs();
                    n += 1.0;
                }
            }
        }
        assert!(
            along / n < across / n,
            "along {} vs across {}",
            along / n,
            across / n
        );
    }
}
