//! Deterministic demo speaker model.
//!
//! Ports the formulas from `python/examples/spinorama_demo.py` so the Rust
//! showcase renders the same CEA-2034-style story without measurement files:
//! an on-axis response with ripple plus narrowing high-frequency directivity.

/// Log-spaced measurement frequencies, 20 Hz to 20 kHz.
pub const FREQUENCY_COUNT: usize = 64;
/// Horizontal angles, -180 to +180 degrees in 10-degree steps.
pub const ANGLE_COUNT: usize = 37;

/// Named CEA-2034-style curves with their legend colors.
pub const CEA2034_SERIES: &[(&str, &str, u32)] = &[
    ("on_axis", "On-axis", 0xf97316),
    ("listening_window", "Listening window", 0x22c55e),
    ("early_reflections", "Early reflections", 0x38bdf8),
    ("sound_power", "Sound power", 0xa78bfa),
    ("predicted_in_room", "Predicted in-room", 0xfacc15),
];

fn base_response(frequency: f64) -> f64 {
    let log_frequency = (frequency / 1000.0).log10();
    86.0 + 1.8 * (log_frequency * 3.2).sin() + 0.8 * (log_frequency * 10.0).sin()
        - 1.5 * (0.0f64).max(-log_frequency - 1.0)
}

fn directivity_response(frequency: f64, angle: f64) -> f64 {
    let log_frequency = (frequency / 250.0).log10();
    let beam_width = (0.32f64).max(1.35 - 0.32 * (0.0f64).max(log_frequency));
    let angle_radians = angle.to_radians();
    let directivity = (-angle_radians.powi(2) / (2.0 * beam_width.powi(2))).exp();
    base_response(frequency) - 38.0 * (1.0 - directivity)
}

/// Precomputed speaker data shared by every showcase section.
#[derive(Debug, Clone)]
pub struct SpeakerModel {
    /// Measurement frequencies in Hz (length [`FREQUENCY_COUNT`]).
    pub frequencies: Vec<f64>,
    /// Measurement angles in degrees (length [`ANGLE_COUNT`]).
    pub angles: Vec<f64>,
    /// CEA-2034 curves keyed by series id, each [`FREQUENCY_COUNT`] levels.
    pub cea2034: Vec<(String, Vec<f64>)>,
    /// Row-major angle-by-frequency SPL grid (`angles x frequencies`).
    pub directivity_grid: Vec<f64>,
}

impl SpeakerModel {
    pub fn build() -> Self {
        let frequencies: Vec<f64> = (0..FREQUENCY_COUNT)
            .map(|index| 20.0f64 * 1000.0f64.powf(index as f64 / 63.0))
            .collect();
        let angles: Vec<f64> = (0..ANGLE_COUNT).map(|index| -180.0 + 10.0 * index as f64).collect();

        let on_axis: Vec<f64> = frequencies.iter().map(|f| base_response(*f)).collect();
        let listening_window: Vec<f64> = frequencies
            .iter()
            .zip(&on_axis)
            .map(|(f, level)| level - 0.7 + 0.35 * (f.log10() * 2.0).sin())
            .collect();
        let early_reflections: Vec<f64> = frequencies
            .iter()
            .zip(&listening_window)
            .map(|(f, level)| level - 1.8 - 0.45 * (0.0f64).max((f / 1000.0).log10()))
            .collect();
        let sound_power: Vec<f64> = frequencies
            .iter()
            .zip(&listening_window)
            .map(|(f, level)| level - 3.5 - 0.25 * (0.0f64).max((f / 1000.0).log10()))
            .collect();
        let predicted_in_room: Vec<f64> = frequencies
            .iter()
            .zip(&listening_window)
            .map(|(f, level)| level - 0.4 + 0.2 * (f.log10() * 1.7).sin())
            .collect();
        let cea2034 = vec![
            ("on_axis".to_string(), on_axis),
            ("listening_window".to_string(), listening_window),
            ("early_reflections".to_string(), early_reflections),
            ("sound_power".to_string(), sound_power),
            ("predicted_in_room".to_string(), predicted_in_room),
        ];

        let mut directivity_grid = Vec::with_capacity(ANGLE_COUNT * FREQUENCY_COUNT);
        for angle in &angles {
            for frequency in &frequencies {
                directivity_grid.push(directivity_response(*frequency, *angle));
            }
        }

        Self {
            frequencies,
            angles,
            cea2034,
            directivity_grid,
        }
    }

    /// SPL curve across [`Self::frequencies`] at one horizontal angle.
    pub fn curve_at_angle(&self, angle: f64) -> Vec<f64> {
        self.frequencies
            .iter()
            .map(|f| directivity_response(*f, angle))
            .collect()
    }
}

/// Legend color for an off-axis angle, matching the Python demo palette.
pub fn angle_color(angle: f64) -> u32 {
    match angle as i32 {
        0 => 0xf97316,
        30 | -30 => 0xa78bfa,
        60 | -60 => 0x60a5fa,
        90 | -90 => 0x38bdf8,
        _ => 0x94a3b8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaker_model_shapes_match_the_python_demo() {
        let model = SpeakerModel::build();
        assert_eq!(model.frequencies.len(), FREQUENCY_COUNT);
        assert_eq!(model.angles.len(), ANGLE_COUNT);
        assert_eq!(model.directivity_grid.len(), ANGLE_COUNT * FREQUENCY_COUNT);
        assert_eq!(model.cea2034.len(), CEA2034_SERIES.len());
        assert!((model.frequencies[0] - 20.0).abs() < f64::EPSILON);
        assert!((model.frequencies[63] - 20_000.0).abs() < 1.0);
        assert!((model.angles[0] + 180.0).abs() < f64::EPSILON);
        assert!((model.angles[36] - 180.0).abs() < f64::EPSILON);
    }

    #[test]
    fn directivity_narrows_off_axis_at_high_frequency() {
        let model = SpeakerModel::build();
        let on_axis = model.curve_at_angle(0.0);
        let off_axis = model.curve_at_angle(90.0);
        // Levels stay in the charted 35-95 dB window.
        for level in on_axis.iter().chain(off_axis.iter()) {
            assert!((35.0..95.0).contains(level), "level {level} out of range");
        }
        // High frequencies beam: 90 degrees is far quieter than on-axis.
        assert!(off_axis[63] < on_axis[63] - 20.0);
        // Directivity narrows with frequency: the low-frequency off-axis
        // drop is much smaller than the high-frequency one.
        let low_drop = on_axis[0] - off_axis[0];
        let high_drop = on_axis[63] - off_axis[63];
        assert!(low_drop > 0.0 && low_drop < high_drop);
    }
}
