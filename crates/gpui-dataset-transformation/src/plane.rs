//! Cutting-plane geometry.

// Rust guideline compliant 2026-02-21

use crate::TransformError;

/// An oriented plane: points with positive signed distance are kept by a
/// positive half-space clip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    /// A point on the plane.
    pub origin: [f64; 3],
    /// Unit normal.
    pub normal: [f64; 3],
}

impl Plane {
    /// Creates a plane, normalizing the normal.
    ///
    /// # Errors
    ///
    /// Returns [`TransformError::Degenerate`] for a zero or non-finite normal.
    pub fn new(origin: [f64; 3], normal: [f64; 3]) -> Result<Self, TransformError> {
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if !len.is_finite() || len == 0.0 {
            return Err(TransformError::Degenerate("plane normal is zero".into()));
        }
        Ok(Self {
            origin,
            normal: [normal[0] / len, normal[1] / len, normal[2] / len],
        })
    }

    /// Axis-aligned plane through the origin, for example `Plane::axis(2, 0.5)`.
    ///
    /// # Errors
    ///
    /// Returns [`TransformError::Degenerate`] when `axis` exceeds 2.
    pub fn axis(axis: usize, offset: f64) -> Result<Self, TransformError> {
        let mut normal = [0.0; 3];
        let mut origin = [0.0; 3];
        match axis {
            0..=2 => {
                normal[axis] = 1.0;
                origin[axis] = offset;
            }
            _ => {
                return Err(TransformError::Degenerate(format!(
                    "axis {axis} out of range"
                )));
            }
        }
        Ok(Self { origin, normal })
    }

    /// Signed distance of a point; positive on the normal side.
    #[must_use]
    pub fn distance(&self, point: [f64; 3]) -> f64 {
        (point[0] - self.origin[0]) * self.normal[0]
            + (point[1] - self.origin[1]) * self.normal[1]
            + (point[2] - self.origin[2]) * self.normal[2]
    }

    /// Linear interpolation factor where segment `a`-`b` crosses the plane.
    #[must_use]
    pub fn crossing(da: f64, db: f64) -> f64 {
        da / (da - db)
    }

    /// Point at factor `t` along segment `a`-`b`.
    #[must_use]
    pub fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_measures_distance() {
        let plane = Plane::new([0.0, 0.0, 1.0], [0.0, 0.0, 2.0]).unwrap();
        assert_eq!(plane.distance([0.0, 0.0, 3.0]), 2.0);
        assert_eq!(plane.distance([5.0, 5.0, 1.0]), 0.0);
    }
}
