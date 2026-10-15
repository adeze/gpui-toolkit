//! Colour-bar legend, annotations, and axis-triad overlays.
//!
//! The legend is computed as data (tick values, labels, pixel layout) so
//! GPUI hosts can draw text with their own text system, while
//! [`draw_colorbar`] and [`draw_axis_triad`] paint the geometry directly
//! into a software [`Frame`] for movies and headless QA.

// Rust guideline compliant 2026-02-21

use crate::camera::Camera;
use crate::raster::Frame;
use crate::transfer::TransferFunction;

/// A text annotation anchored to normalised frame coordinates.
///
/// Coordinates run from 0 to 1 across the frame; hosts render `text` at
/// that position. The software rasterizer does not draw text (it has no
/// font), so annotations are for GPUI hosts and metadata consumers.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// Text to draw.
    pub text: String,
    /// Horizontal anchor, 0 = left edge, 1 = right edge.
    pub x: f32,
    /// Vertical anchor, 0 = top edge, 1 = bottom edge.
    pub y: f32,
}

/// Colour-bar data for one transfer function: title plus labelled ticks.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorBar {
    /// Field label, drawn as the bar title.
    pub label: String,
    /// Physical unit appended to the title, when known.
    pub unit: Option<String>,
    /// Tick values with pre-formatted labels, ascending.
    pub ticks: Vec<(f64, String)>,
}

impl ColorBar {
    /// Builds a colour bar with about five nicely rounded ticks.
    #[must_use]
    pub fn new(
        transfer: &TransferFunction,
        label: impl Into<String>,
        unit: Option<String>,
    ) -> Self {
        let ticks = nice_ticks(transfer.range, 5)
            .into_iter()
            .map(|v| (v, format_tick(v)))
            .collect();
        Self {
            label: label.into(),
            unit,
            ticks,
        }
    }

    /// Bar title: `label` plus ` (unit)` when a unit is set.
    #[must_use]
    pub fn title(&self) -> String {
        match &self.unit {
            Some(unit) => format!("{} ({})", self.label, unit),
            None => self.label.clone(),
        }
    }
}

/// An axis-aligned pixel rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left edge in pixels.
    pub x: u32,
    /// Top edge in pixels.
    pub y: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Pixel layout of a vertical colour bar inside a target rectangle.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorBarLayout {
    /// Gradient strip rectangle.
    pub bar: Rect,
    /// Tick label anchors: pixel row (centre of the tick) plus label.
    pub ticks: Vec<(u32, String)>,
    /// Title text (from [`ColorBar::title`]).
    pub title: String,
}

/// Lays out a vertical colour bar filling `area` (bar on the left of the
/// area, tick labels anchored to its right in host rendering).
#[must_use]
pub fn layout_colorbar(
    color_bar: &ColorBar,
    transfer: &TransferFunction,
    area: Rect,
) -> ColorBarLayout {
    let bar_width = area.width.min(24);
    let bar = Rect {
        x: area.x,
        y: area.y,
        width: bar_width,
        height: area.height,
    };
    let ticks = color_bar
        .ticks
        .iter()
        .map(|(value, label)| {
            let t = transfer.normalize(*value);
            let row = area.y + ((1.0 - t) * f64::from(area.height.saturating_sub(1))) as u32;
            (row, label.clone())
        })
        .collect();
    ColorBarLayout {
        bar,
        ticks,
        title: color_bar.title(),
    }
}

/// Paints the gradient strip and a dark border into a frame.
pub fn draw_colorbar(frame: &mut Frame, transfer: &TransferFunction, layout: &ColorBarLayout) {
    let bar = layout.bar;
    if bar.width == 0 || bar.height == 0 {
        return;
    }
    for row in 0..bar.height {
        let t = 1.0 - f64::from(row) / f64::from(bar.height.saturating_sub(1).max(1));
        let value = value_at(transfer, t);
        let colour = transfer.map(value);
        for col in 0..bar.width {
            set_pixel(frame, bar.x + col, bar.y + row, colour);
        }
    }
    let border = [0.05, 0.05, 0.06, 1.0];
    for col in 0..bar.width {
        set_pixel(frame, bar.x + col, bar.y, border);
        set_pixel(frame, bar.x + col, bar.y + bar.height - 1, border);
    }
    for row in 0..bar.height {
        set_pixel(frame, bar.x, bar.y + row, border);
        set_pixel(frame, bar.x + bar.width - 1, bar.y + row, border);
    }
}

/// Draws the world-axis triad (X red, Y green, Z blue) around a centre.
///
/// Axes are drawn back-to-front so the axis pointing most toward the
/// viewer ends up on top. Endpoints are returned as
/// `(axis, x, y)` pixel positions so hosts can place the X/Y/Z letters.
pub fn draw_axis_triad(
    frame: &mut Frame,
    camera: &Camera,
    centre: (u32, u32),
    radius: u32,
) -> [(char, u32, u32); 3] {
    let right = camera.right();
    let up = camera.up();
    let axes: [(char, [f32; 3], [f32; 4]); 3] = [
        ('X', [1.0, 0.0, 0.0], [0.9, 0.25, 0.25, 1.0]),
        ('Y', [0.0, 1.0, 0.0], [0.35, 0.85, 0.35, 1.0]),
        ('Z', [0.0, 0.0, 1.0], [0.3, 0.45, 0.95, 1.0]),
    ];
    let mut projected: Vec<(f32, char, i64, i64, [f32; 4])> = axes
        .iter()
        .map(|(name, axis, colour)| {
            let sx = axis[0] * right[0] + axis[1] * right[1] + axis[2] * right[2];
            let sy = axis[0] * up[0] + axis[1] * up[1] + axis[2] * up[2];
            let ex = i64::from(centre.0) + (sx * radius as f32) as i64;
            let ey = i64::from(centre.1) - (sy * radius as f32) as i64;
            // View-space z (right x up = backward): larger faces the
            // viewer and is drawn last.
            let vz = axis[0] * (right[1] * up[2] - right[2] * up[1])
                + axis[1] * (right[2] * up[0] - right[0] * up[2])
                + axis[2] * (right[0] * up[1] - right[1] * up[0]);
            (vz, *name, ex, ey, *colour)
        })
        .collect();
    projected.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut endpoints = [('X', centre.0, centre.1); 3];
    for (i, (_, name, ex, ey, colour)) in projected.iter().enumerate() {
        draw_line(
            frame,
            i64::from(centre.0),
            i64::from(centre.1),
            *ex,
            *ey,
            *colour,
        );
        endpoints[i] = (
            *name,
            (*ex).clamp(0, i64::from(frame.width.saturating_sub(1))) as u32,
            (*ey).clamp(0, i64::from(frame.height.saturating_sub(1))) as u32,
        );
    }
    endpoints
}

fn value_at(transfer: &TransferFunction, t: f64) -> f64 {
    let [lo, hi] = transfer.range;
    if transfer.log_scale && lo > 0.0 && hi > lo {
        (lo.ln() + t * (hi.ln() - lo.ln())).exp()
    } else {
        lo + t * (hi - lo)
    }
}

fn set_pixel(frame: &mut Frame, x: u32, y: u32, colour: [f32; 4]) {
    if x >= frame.width || y >= frame.height {
        return;
    }
    let idx = ((y * frame.width + x) * 4) as usize;
    frame.rgba[idx] = (colour[0].clamp(0.0, 1.0) * 255.0) as u8;
    frame.rgba[idx + 1] = (colour[1].clamp(0.0, 1.0) * 255.0) as u8;
    frame.rgba[idx + 2] = (colour[2].clamp(0.0, 1.0) * 255.0) as u8;
    frame.rgba[idx + 3] = (colour[3].clamp(0.0, 1.0) * 255.0) as u8;
}

fn draw_line(frame: &mut Frame, x0: i64, y0: i64, x1: i64, y1: i64, colour: [f32; 4]) {
    // Bresenham; frames are small overlays so clarity beats cleverness.
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);
    loop {
        if x >= 0 && y >= 0 {
            set_pixel(frame, x as u32, y as u32, colour);
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Nicely rounded tick values inside `range` (1/2/5 × 10^k steps).
#[must_use]
pub fn nice_ticks(range: [f64; 2], target: usize) -> Vec<f64> {
    let [lo, hi] = range;
    if hi <= lo || target == 0 {
        return vec![lo];
    }
    let span = hi - lo;
    let raw_step = span / target as f64;
    let magnitude = 10f64.powf(raw_step.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * magnitude)
        .find(|s| span / s <= target as f64 * 1.5)
        .unwrap_or(10.0 * magnitude);
    let first = (lo / step).ceil() * step;
    let mut ticks = Vec::new();
    let mut value = first;
    while value <= hi + step * 1e-9 {
        ticks.push(value);
        value += step;
    }
    if ticks.is_empty() {
        ticks.push(lo);
    }
    ticks
}

/// Compact tick label: integers stay integers, small values keep digits.
#[must_use]
pub fn format_tick(value: f64) -> String {
    if value == value.trunc() && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else if value.abs() >= 100.0 {
        format!("{value:.0}")
    } else if value.abs() >= 1.0 {
        format!("{value:.2}")
    } else if value.abs() >= 0.01 {
        format!("{value:.3}")
    } else if value == 0.0 {
        "0".to_string()
    } else {
        format!("{value:.2e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_mesh::{Bounds, DataSet};

    #[test]
    fn nice_ticks_hit_round_numbers() {
        let ticks = nice_ticks([0.0, 97.0], 5);
        assert!(ticks.contains(&0.0) || ticks.contains(&20.0));
        assert!(ticks.iter().all(|t| (0.0..=97.0).contains(t)));
        assert!(ticks.len() >= 4);
        assert_eq!(nice_ticks([2.0, 2.0], 5), vec![2.0]);
    }

    #[test]
    fn colorbar_paints_gradient_and_reports_title() {
        let transfer = TransferFunction::viridis([0.0, 100.0]);
        let bar = ColorBar::new(&transfer, "Pressure", Some("Pa".into()));
        assert_eq!(bar.title(), "Pressure (Pa)");
        let layout = layout_colorbar(
            &bar,
            &transfer,
            Rect {
                x: 4,
                y: 4,
                width: 60,
                height: 100,
            },
        );
        let mut frame = Frame {
            width: 80,
            height: 120,
            rgba: vec![0; 80 * 120 * 4],
        };
        draw_colorbar(&mut frame, &transfer, &layout);
        // Interior rows: bottom is the low (dark violet) viridis end.
        let idx = (((4 + 97) * 80 + 10) * 4) as usize;
        let low = frame.rgba[idx];
        let top_idx = ((6 * 80 + 10) * 4) as usize;
        let high = frame.rgba[top_idx];
        assert!(high > low, "gradient brightens upward: {high} vs {low}");
        assert!(!layout.ticks.is_empty());
    }

    #[test]
    fn axis_triad_endpoints_stay_in_frame() {
        let ds = DataSet::structured_grid("g", [3, 3, 3], [0.0; 3], [1.0, 1.0, 1.0]).unwrap();
        let bounds: Bounds = ds.bounds().unwrap();
        let camera = Camera::fitted(bounds);
        let mut frame = Frame {
            width: 100,
            height: 100,
            rgba: vec![0; 100 * 100 * 4],
        };
        let endpoints = draw_axis_triad(&mut frame, &camera, (50, 50), 30);
        for (_, x, y) in endpoints {
            assert!(x < 100 && y < 100);
        }
        // At least the red X axis painted something.
        assert!(frame.rgba.chunks(4).any(|px| px[0] > 100 && px[3] == 255));
    }
}
