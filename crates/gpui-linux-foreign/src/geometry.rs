//! Host geometry helpers (contract §5).
//!
//! The host owns size and scale; these pure helpers convert the host
//! record into validated logical bounds and drawable sizes. They compile
//! on every host so the lifecycle math is unit-tested without a display.

/// True for a usable backing scale: finite and positive.
#[must_use]
pub fn validate_scale(scale: f32) -> bool {
    scale.is_finite() && scale > 0.0
}

/// Drawable size in device pixels: `max(1, round(logical × scale))` per
/// axis, so a zero host size never produces a zero drawable. `None` for an
/// invalid scale.
#[must_use]
pub fn drawable_size(logical_width: u32, logical_height: u32, scale: f32) -> Option<(u32, u32)> {
    if !validate_scale(scale) {
        return None;
    }
    let width = (f64::from(logical_width) * f64::from(scale)).round() as u32;
    let height = (f64::from(logical_height) * f64::from(scale)).round() as u32;
    Some((width.max(1), height.max(1)))
}

/// Validated host geometry: logical size plus backing scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForeignBounds {
    width: u32,
    height: u32,
    scale: f32,
}

impl ForeignBounds {
    /// Build validated bounds; `None` for zero size or an invalid scale.
    #[must_use]
    pub fn new(width: u32, height: u32, scale: f32) -> Option<Self> {
        if width == 0 || height == 0 || !validate_scale(scale) {
            return None;
        }
        Some(Self {
            width,
            height,
            scale,
        })
    }

    /// Logical size in CSS pixels.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Backing scale (device pixels per logical pixel).
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Drawable size in device pixels (never zero on any axis).
    #[must_use]
    pub fn drawable(&self) -> (u32, u32) {
        drawable_size(self.width, self.height, self.scale)
            .expect("ForeignBounds is always valid by construction")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_must_be_finite_and_positive() {
        assert!(validate_scale(1.0));
        assert!(validate_scale(2.0));
        assert!(!validate_scale(0.0));
        assert!(!validate_scale(-1.0));
        assert!(!validate_scale(f32::NAN));
        assert!(!validate_scale(f32::INFINITY));
    }

    #[test]
    fn drawable_rounds_and_never_returns_zero() {
        assert_eq!(drawable_size(800, 600, 1.0), Some((800, 600)));
        assert_eq!(drawable_size(800, 600, 2.0), Some((1600, 1200)));
        assert_eq!(drawable_size(0, 0, 1.0), Some((1, 1)));
        assert_eq!(drawable_size(0, 600, 2.0), Some((1, 1200)));
        assert_eq!(drawable_size(101, 101, 1.5), Some((152, 152)));
        assert_eq!(drawable_size(800, 600, 0.0), None);
        assert_eq!(drawable_size(800, 600, f32::NAN), None);
    }

    #[test]
    fn bounds_reject_degenerate_hosts() {
        assert!(ForeignBounds::new(800, 600, 1.0).is_some());
        assert!(ForeignBounds::new(0, 600, 1.0).is_none());
        assert!(ForeignBounds::new(800, 0, 1.0).is_none());
        assert!(ForeignBounds::new(800, 600, 0.0).is_none());
        assert!(ForeignBounds::new(800, 600, -2.0).is_none());
    }

    #[test]
    fn bounds_drawable_matches_the_free_function() {
        let bounds = ForeignBounds::new(101, 203, 1.5).expect("valid bounds");
        assert_eq!(bounds.drawable(), drawable_size(101, 203, 1.5).expect("valid scale"));
        assert_eq!(bounds.size(), (101, 203));
        assert_eq!(bounds.scale(), 1.5);
    }
}
