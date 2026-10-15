//! Time-varying datasets with shared or deforming topology.

// Rust guideline compliant 2026-02-21

use crate::dataset::{DataSet, MeshError};

/// A sequence of dataset states at increasing times.
///
/// States may deform (positions change between steps, as in Navier-Stokes
/// or crash simulations) but must share cell storage, which lets a renderer
/// upload connectivity once and stream only positions and fields per step.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeSeries {
    /// State at each time step.
    pub steps: Vec<DataSet>,
    /// Physical time of each step, strictly increasing.
    pub times: Vec<f64>,
}

impl TimeSeries {
    /// Creates a time series.
    ///
    /// # Errors
    ///
    /// Returns [`MeshError::InvalidDims`]-style misuse as a plain message via
    /// [`TimeSeriesError`] is avoided; structural problems are reported with
    /// the closest [`MeshError`] variant or a dedicated description.
    pub fn new(steps: Vec<DataSet>, times: Vec<f64>) -> Result<Self, MeshError> {
        if steps.len() != times.len() || steps.is_empty() {
            return Err(MeshError::InvalidDims {
                dims: [steps.len(), times.len(), 0],
            });
        }
        if times.windows(2).any(|w| w[1] <= w[0]) || times.iter().any(|t| !t.is_finite()) {
            return Err(MeshError::InvalidDims {
                dims: [times.len(), 0, 0],
            });
        }
        if let Some(first) = steps.first() {
            for step in &steps[1..] {
                if step.cells.types != first.cells.types
                    || step.cells.offsets != first.cells.offsets
                    || step.cells.connectivity != first.cells.connectivity
                {
                    return Err(MeshError::InvalidOffsets);
                }
            }
        }
        Ok(Self { steps, times })
    }

    /// Number of steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether the series has no steps (unreachable through `new`).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Step index and blend factor for a physical time, clamped to the range.
    #[must_use]
    pub fn locate(&self, time: f64) -> (usize, usize, f64) {
        if time <= self.times[0] {
            return (0, 0, 0.0);
        }
        let last = self.times.len() - 1;
        if time >= self.times[last] {
            return (last, last, 0.0);
        }
        let upper = self.times.partition_point(|t| *t <= time);
        let lo = upper - 1;
        let span = self.times[upper] - self.times[lo];
        (lo, upper, (time - self.times[lo]) / span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::Field;

    fn step(z: f64, p: f64) -> DataSet {
        DataSet::surface(
            "s",
            vec![[0.0, 0.0, z], [1.0, 0.0, z], [0.0, 1.0, z]],
            &[[0, 1, 2]],
        )
        .with_field(Field::scalar("pressure", vec![p, p, p]))
    }

    #[test]
    fn locates_and_clamps_time() {
        let series = TimeSeries::new(vec![step(0.0, 0.0), step(1.0, 2.0)], vec![0.0, 2.0]).unwrap();
        assert_eq!(series.locate(-1.0), (0, 0, 0.0));
        assert_eq!(series.locate(1.0), (0, 1, 0.5));
        assert_eq!(series.locate(9.0), (1, 1, 0.0));
    }

    #[test]
    fn rejects_topology_change() {
        let other = DataSet::surface(
            "s",
            vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]],
            &[[0, 1, 2], [1, 3, 2]],
        );
        TimeSeries::new(vec![step(0.0, 0.0), other], vec![0.0, 1.0]).unwrap_err();
    }
}
