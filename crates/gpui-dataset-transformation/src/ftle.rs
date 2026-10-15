//! Finite-time Lyapunov exponent (FTLE) ridges.
//!
//! For each point of the step at `t0`, the flow map to `t0 + duration` is
//! differentiated by central differences (six neighbour advects), the
//! Cauchy-Green tensor `F^T F` is formed, and the FTLE
//! `ln(sqrt(lambda_max)) / |duration|` is appended as a scalar field.
//! Ridges of this field are the standard Lagrangian coherent structures.

// Rust guideline compliant 2026-02-21

use crate::TransformError;
use crate::eigen::symmetric_eigen;
use crate::particles::TimeSampler;
use gpui_mesh::{DataSet, Field, TimeSeries};

/// Appends an `ftle` scalar field computed over `[t0, t0 + duration]`.
///
/// Neighbour offset `epsilon` should be a fraction of the local cell
/// size. Points whose neighbourhood leaves the domain get FTLE 0.
/// Sub-stepping uses `steps` RK2 steps over the duration.
///
/// # Errors
///
/// Returns an error when the field is missing, `t0` has no matching step,
/// or options are degenerate.
pub fn ftle(
    series: &TimeSeries,
    field_name: &str,
    t0: f64,
    duration: f64,
    epsilon: f64,
    steps: usize,
) -> Result<DataSet, TransformError> {
    if duration == 0.0 || epsilon <= 0.0 || steps == 0 {
        return Err(TransformError::Degenerate(
            "FTLE requires duration != 0, epsilon > 0, steps > 0".into(),
        ));
    }
    let sampler = TimeSampler::new(series, field_name)?;
    let (lo, _, _) = series.locate(t0);
    let base = &series.steps[lo];
    let dt = duration / steps as f64;
    let advect = |mut p: [f64; 3]| -> Option<[f64; 3]> {
        let mut time = t0;
        for _ in 0..steps {
            p = sampler.rk2(p, time, dt)?;
            time += dt;
        }
        Some(p)
    };
    let mut values = Vec::with_capacity(base.point_count());
    for &point in base.positions.iter() {
        let mut gradient = [[0.0f64; 3]; 3];
        let mut valid = true;
        for axis in 0..3 {
            let mut plus = point;
            let mut minus = point;
            plus[axis] += epsilon;
            minus[axis] -= epsilon;
            if let (Some(a), Some(b)) = (advect(plus), advect(minus)) {
                for component in 0..3 {
                    gradient[component][axis] = (a[component] - b[component]) / (2.0 * epsilon);
                }
            } else {
                valid = false;
                break;
            }
        }
        if !valid {
            values.push(0.0);
            continue;
        }
        // Cauchy-Green C = F^T F (symmetric).
        let mut c = [[0.0; 3]; 3];
        for (i, row) in c.iter_mut().enumerate() {
            for (j, value) in row.iter_mut().enumerate() {
                *value = (0..3).map(|k| gradient[k][i] * gradient[k][j]).sum();
            }
        }
        let (eigenvalues, _) = symmetric_eigen(c);
        let lambda_max = eigenvalues.iter().copied().fold(0.0, f64::max);
        values.push(lambda_max.max(1e-300).sqrt().ln() / duration.abs());
    }
    let mut out = base.clone();
    out.point_data.retain(|f| f.name.as_ref() != "ftle");
    let mut field = Field::scalar("ftle", values);
    field.label = "FTLE".into();
    field.unit = Some("1/s".into());
    out.point_data.push(field);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series_with_velocity(make: fn([f64; 3]) -> [f64; 3]) -> TimeSeries {
        let mut steps = Vec::new();
        for _ in 0..2 {
            let ds = DataSet::structured_grid("g", [5, 5, 5], [0.0; 3], [0.5; 3]).unwrap();
            let vectors: Vec<[f64; 3]> = ds.positions.iter().map(|p| make(*p)).collect();
            steps.push(ds.with_field(Field::vector("v", vectors)));
        }
        TimeSeries::new(steps, vec![0.0, 10.0]).unwrap()
    }

    #[test]
    fn uniform_flow_has_zero_ftle_and_stretch_has_exact_rate() {
        let uniform = series_with_velocity(|_| [1.0, 0.0, 0.0]);
        let out = ftle(&uniform, "v", 0.0, 0.5, 0.05, 4).unwrap();
        let values = out
            .field(gpui_mesh::Association::Point, "ftle")
            .unwrap()
            .as_scalar()
            .unwrap();
        assert!(values.iter().all(|v| v.abs() < 1e-9));

        // v = (x, 0, 0): flow map x -> x * e^t, FTLE = 1 along x.
        let stretch = series_with_velocity(|p| [p[0], 0.0, 0.0]);
        let out = ftle(&stretch, "v", 0.0, 0.5, 0.05, 16).unwrap();
        let values = out
            .field(gpui_mesh::Association::Point, "ftle")
            .unwrap()
            .as_scalar()
            .unwrap();
        let centre = out
            .positions
            .iter()
            .position(|p| *p == [1.0, 1.0, 1.0])
            .unwrap();
        assert!(
            (values[centre] - 1.0).abs() < 0.05,
            "ftle {}",
            values[centre]
        );
    }
}
