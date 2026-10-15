//! Time playback over a `TimeSeries`, driven by host-supplied times.

// Rust guideline compliant 2026-02-21

use gpui_mesh::{Association, DataSet, Field, FieldData, TimeSeries};

/// Playback state for a time series.
///
/// The host owns the clock: it calls [`Player::advance`] with elapsed
/// seconds (from GPUI, `requestAnimationFrame`, or a test), which keeps
/// this crate free of wall-clock and platform timer dependencies.
#[derive(Debug, Clone)]
pub struct Player {
    series: TimeSeries,
    /// Current physical time.
    pub time: f64,
    /// Whether playback advances on `advance`.
    pub playing: bool,
    /// Playback speed multiplier.
    pub speed: f64,
    /// Whether playback wraps at the end.
    pub looping: bool,
}

impl Player {
    /// Creates a paused player at the first step.
    pub fn new(series: TimeSeries) -> Self {
        let time = series.times.first().copied().unwrap_or(0.0);
        Self {
            series,
            time,
            playing: false,
            speed: 1.0,
            looping: true,
        }
    }

    /// Starts or resumes playback.
    pub fn play(&mut self) {
        self.playing = true;
    }

    /// Pauses playback.
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Seeks to a physical time, clamped to the series range.
    pub fn seek(&mut self, time: f64) {
        let first = self.series.times.first().copied().unwrap_or(0.0);
        let last = self.series.times.last().copied().unwrap_or(first);
        self.time = time.clamp(first, last);
    }

    /// Advances by elapsed seconds when playing; wraps or stops at the end.
    pub fn advance(&mut self, elapsed_seconds: f64) {
        if !self.playing {
            return;
        }
        let first = self.series.times.first().copied().unwrap_or(0.0);
        let last = self.series.times.last().copied().unwrap_or(first);
        let mut time = self.time + elapsed_seconds * self.speed;
        if time > last {
            if self.looping && last > first {
                time = first + (time - first) % (last - first);
            } else {
                time = last;
                self.playing = false;
            }
        }
        self.time = time;
    }

    /// The interpolated dataset at the current time.
    ///
    /// Positions and scalar/vector fields are linearly interpolated
    /// between the bracketing steps; topology and cell fields come from
    /// the earlier step. This is the per-frame state a movie or a live
    /// flow view renders.
    #[must_use]
    pub fn current(&self) -> DataSet {
        let (lo, hi, t) = self.series.locate(self.time);
        if lo == hi || t == 0.0 {
            return self.series.steps[lo].clone();
        }
        let a = &self.series.steps[lo];
        let b = &self.series.steps[hi];
        let positions: Vec<[f64; 3]> = a
            .positions
            .iter()
            .zip(b.positions.iter())
            .map(|(pa, pb)| {
                [
                    pa[0] + (pb[0] - pa[0]) * t,
                    pa[1] + (pb[1] - pa[1]) * t,
                    pa[2] + (pb[2] - pa[2]) * t,
                ]
            })
            .collect();
        let mut result = a.clone();
        result.positions = positions.into();
        for (index, field) in a.point_data.iter().enumerate() {
            let Some(other) = b.point_data.get(index) else {
                continue;
            };
            if other.name != field.name {
                continue;
            }
            result.point_data[index] = interpolate(field, other, t);
        }
        result
    }

    /// Renders `frames` evenly spaced frames over the whole series.
    ///
    /// Each frame pairs its physical time with a rendered image; hosts
    /// encode them (PNG sequence, then an external muxer for MP4/WebM).
    ///
    /// # Errors
    ///
    /// Propagates render errors from [`crate::render`].
    pub fn movie_frames(
        &self,
        scene_for: impl Fn(DataSet) -> crate::Scene,
        camera: &crate::Camera,
        width: u32,
        height: u32,
        frames: usize,
    ) -> Result<Vec<(f64, crate::Frame)>, crate::SceneError> {
        let first = self.series.times.first().copied().unwrap_or(0.0);
        let last = self.series.times.last().copied().unwrap_or(first);
        let mut player = self.clone();
        let mut out = Vec::with_capacity(frames);
        for i in 0..frames.max(1) {
            let time = if frames <= 1 {
                first
            } else {
                first + (last - first) * (i as f64 / (frames - 1) as f64)
            };
            player.seek(time);
            let scene = scene_for(player.current());
            out.push((time, crate::render(&scene, camera, width, height)?));
        }
        Ok(out)
    }
}

fn interpolate(a: &Field, b: &Field, t: f64) -> Field {
    let data = match (&a.data, &b.data) {
        (FieldData::Scalar(x), FieldData::Scalar(y)) => {
            FieldData::Scalar(lerp_slice(x, y, t).into())
        }
        (FieldData::Vector(x), FieldData::Vector(y)) => {
            let values: Vec<[f64; 3]> = x
                .iter()
                .zip(y.iter())
                .map(|(pa, pb)| {
                    [
                        pa[0] + (pb[0] - pa[0]) * t,
                        pa[1] + (pb[1] - pa[1]) * t,
                        pa[2] + (pb[2] - pa[2]) * t,
                    ]
                })
                .collect();
            FieldData::Vector(values.into())
        }
        _ => a.data.clone(),
    };
    Field {
        name: a.name.clone(),
        label: a.label.clone(),
        unit: a.unit.clone(),
        association: match a.association {
            Association::Point => Association::Point,
            Association::Cell => Association::Cell,
        },
        data,
        valid: a.valid.clone(),
    }
}

fn lerp_slice(a: &[f64], b: &[f64], t: f64) -> Vec<f64> {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| x + (y - x) * t)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_mesh::CellType;
    use gpui_mesh::Cells;

    fn step(z: f64, p: f64) -> DataSet {
        DataSet::new(
            "s",
            vec![[0.0, 0.0, z], [1.0, 0.0, z], [0.0, 1.0, z]],
            Cells::from_cells(&[(CellType::Triangle, &[0, 1, 2][..])]),
        )
        .with_field(Field::scalar("p", vec![p; 3]))
    }

    #[test]
    fn interpolates_positions_and_fields() {
        let series =
            TimeSeries::new(vec![step(0.0, 0.0), step(2.0, 10.0)], vec![0.0, 2.0]).unwrap();
        let mut player = Player::new(series);
        player.seek(1.0);
        let current = player.current();
        assert_eq!(current.positions[0], [0.0, 0.0, 1.0]);
        assert_eq!(
            current
                .field(Association::Point, "p")
                .unwrap()
                .as_scalar()
                .unwrap(),
            &[5.0, 5.0, 5.0]
        );
    }

    #[test]
    fn advance_wraps_when_looping() {
        let series =
            TimeSeries::new(vec![step(0.0, 0.0), step(2.0, 10.0)], vec![0.0, 2.0]).unwrap();
        let mut player = Player::new(series);
        player.play();
        player.advance(3.0);
        assert_eq!(player.time, 1.0);
        assert!(player.playing);
    }

    #[test]
    fn movie_frames_cover_the_series() {
        let series =
            TimeSeries::new(vec![step(0.0, 0.0), step(2.0, 10.0)], vec![0.0, 2.0]).unwrap();
        let player = Player::new(series);
        let camera = crate::Camera::default();
        let frames = player
            .movie_frames(
                |ds| {
                    crate::Scene::new().with_actor(
                        crate::Actor::new(ds)
                            .coloured_by("p", crate::TransferFunction::viridis([0.0, 10.0])),
                    )
                },
                &camera,
                16,
                16,
                3,
            )
            .unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].0, 0.0);
        assert_eq!(frames[2].0, 2.0);
    }
}
