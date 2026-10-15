//! Line-dataset geometry: streamtubes, stream ribbons, animated dashes.
//!
//! Streamlines arrive as per-segment line cells sharing points; the
//! helpers here first stitch them back into polylines by node identity.

// Rust guideline compliant 2026-02-21

use crate::TransformError;
use gpui_mesh::{CellType, Cells, DataSet};

/// Stitches line cells into polylines by shared node ids.
///
/// Cells are walked in order; a cell whose first node continues the
/// current polyline extends it, otherwise a new polyline starts.
pub(crate) fn polylines(dataset: &DataSet) -> Vec<Vec<[f64; 3]>> {
    let mut lines: Vec<Vec<u32>> = Vec::new();
    for cell in 0..dataset.cell_count() {
        if dataset.cells.types[cell] != CellType::Line {
            continue;
        }
        let nodes = dataset.cells.cell(cell).unwrap_or(&[]);
        if nodes.len() < 2 {
            continue;
        }
        match lines.last_mut() {
            Some(current) if *current.last().unwrap_or(&u32::MAX) == nodes[0] => {
                current.extend_from_slice(&nodes[1..]);
            }
            _ => lines.push(nodes.to_vec()),
        }
    }
    lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|&n| dataset.positions[n as usize])
                .collect()
        })
        .collect()
}

/// Extrudes streamlines into triangular tubes of constant radius.
///
/// Frames are parallel-transported along each polyline, so tubes do not
/// twist or pinch at bends the way fixed-up frames do. Open ends are left
/// uncapped; tubes are meant as overlays.
///
/// # Errors
///
/// Returns an error when the dataset holds no line cells or `sides < 3`.
pub fn streamtubes(lines: &DataSet, radius: f64, sides: usize) -> Result<DataSet, TransformError> {
    if sides < 3 {
        return Err(TransformError::Degenerate(
            "tubes need at least 3 sides".into(),
        ));
    }
    let paths = polylines(lines);
    if paths.is_empty() {
        return Err(TransformError::Degenerate(
            "dataset holds no line cells".into(),
        ));
    }
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut cells: Vec<(CellType, Vec<u32>)> = Vec::new();
    for path in &paths {
        if path.len() < 2 {
            continue;
        }
        let mut normal = initial_normal(tangent(path[0], path[1]));
        let mut rings: Vec<Vec<u32>> = Vec::with_capacity(path.len());
        for i in 0..path.len() {
            let tan = tangent(path[i.saturating_sub(1)], path[(i + 1).min(path.len() - 1)]);
            normal = transport(normal, tan);
            let binormal = cross(tan, normal);
            let mut ring = Vec::with_capacity(sides);
            for s in 0..sides {
                let a = s as f64 / sides as f64 * std::f64::consts::TAU;
                let offset = [
                    (normal[0] * a.cos() + binormal[0] * a.sin()) * radius,
                    (normal[1] * a.cos() + binormal[1] * a.sin()) * radius,
                    (normal[2] * a.cos() + binormal[2] * a.sin()) * radius,
                ];
                ring.push(positions.len() as u32);
                positions.push([
                    path[i][0] + offset[0],
                    path[i][1] + offset[1],
                    path[i][2] + offset[2],
                ]);
            }
            rings.push(ring);
        }
        for w in 0..rings.len() - 1 {
            for s in 0..sides {
                let s2 = (s + 1) % sides;
                cells.push((
                    CellType::Triangle,
                    vec![rings[w][s], rings[w][s2], rings[w + 1][s2]],
                ));
                cells.push((
                    CellType::Triangle,
                    vec![rings[w][s], rings[w + 1][s2], rings[w + 1][s]],
                ));
            }
        }
    }
    let refs: Vec<(CellType, &[u32])> = cells.iter().map(|(t, n)| (*t, n.as_slice())).collect();
    Ok(DataSet::new(
        format!("{}-tubes", lines.id),
        positions,
        Cells::from_cells(&refs),
    ))
}

/// Extrudes streamlines into flat ribbons of constant width.
///
/// The ribbon width direction is the cross product of the tangent with a
/// world up vector (switched when nearly parallel), so ribbons lie flat
/// against the flow like dye sheets; their apparent twist shows swirl.
///
/// # Errors
///
/// Returns an error when the dataset holds no line cells.
pub fn stream_ribbons(lines: &DataSet, width: f64) -> Result<DataSet, TransformError> {
    let paths = polylines(lines);
    if paths.is_empty() {
        return Err(TransformError::Degenerate(
            "dataset holds no line cells".into(),
        ));
    }
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut cells: Vec<(CellType, Vec<u32>)> = Vec::new();
    for path in &paths {
        if path.len() < 2 {
            continue;
        }
        let base = positions.len() as u32;
        for i in 0..path.len() {
            let tan = tangent(path[i.saturating_sub(1)], path[(i + 1).min(path.len() - 1)]);
            let side = side_direction(tan);
            for s in [-0.5, 0.5] {
                positions.push([
                    path[i][0] + side[0] * width * s,
                    path[i][1] + side[1] * width * s,
                    path[i][2] + side[2] * width * s,
                ]);
            }
        }
        for i in 0..path.len() - 1 {
            let a = base + (2 * i) as u32;
            cells.push((CellType::Triangle, vec![a, a + 1, a + 3]));
            cells.push((CellType::Triangle, vec![a, a + 3, a + 2]));
        }
    }
    let refs: Vec<(CellType, &[u32])> = cells.iter().map(|(t, n)| (*t, n.as_slice())).collect();
    Ok(DataSet::new(
        format!("{}-ribbons", lines.id),
        positions,
        Cells::from_cells(&refs),
    ))
}

/// Keeps only the dash portions of polylines, offset by `phase`.
///
/// Animating `phase` from 0 to `dash + gap` marches the dashes along the
/// lines at a constant apparent speed, encoding flow direction. Dash and
/// gap are arc lengths in dataset units; `phase` wraps over one period.
///
/// # Errors
///
/// Returns an error for non-positive dash/gap or no line cells.
pub fn dashed_lines(
    lines: &DataSet,
    phase: f64,
    dash: f64,
    gap: f64,
) -> Result<DataSet, TransformError> {
    if dash <= 0.0 || gap <= 0.0 {
        return Err(TransformError::Degenerate(
            "dash and gap must be positive".into(),
        ));
    }
    let paths = polylines(lines);
    if paths.is_empty() {
        return Err(TransformError::Degenerate(
            "dataset holds no line cells".into(),
        ));
    }
    let period = dash + gap;
    let shift = phase.rem_euclid(period);
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut cells: Vec<(CellType, Vec<u32>)> = Vec::new();
    for path in &paths {
        // Arc-length parameterise, then keep sub-intervals whose shifted
        // arc length falls inside a dash window.
        let mut lengths = vec![0.0];
        for w in path.windows(2) {
            lengths.push(lengths.last().copied().unwrap_or(0.0) + dist(w[0], w[1]));
        }
        let total = lengths.last().copied().unwrap_or(0.0);
        // Dash windows [k*period - shift, +dash] intersected with the line.
        let mut intervals: Vec<(f64, f64)> = Vec::new();
        let mut k = 0;
        loop {
            let start = f64::from(k) * period - shift;
            if start > total {
                break;
            }
            if start + dash > 0.0 {
                intervals.push((start.max(0.0), (start + dash).min(total)));
            }
            k += 1;
        }
        let sample = |s: f64| -> [f64; 3] {
            let seg = lengths
                .partition_point(|&l| l <= s)
                .clamp(1, path.len() - 1);
            let (s0, s1) = (lengths[seg - 1], lengths[seg]);
            let t = if s1 > s0 { (s - s0) / (s1 - s0) } else { 0.0 };
            [
                path[seg - 1][0] + (path[seg][0] - path[seg - 1][0]) * t,
                path[seg - 1][1] + (path[seg][1] - path[seg - 1][1]) * t,
                path[seg - 1][2] + (path[seg][2] - path[seg - 1][2]) * t,
            ]
        };
        for (start, end) in intervals {
            if end - start < 1e-12 {
                continue;
            }
            let a = positions.len() as u32;
            positions.push(sample(start));
            positions.push(sample(end));
            cells.push((CellType::Line, vec![a, a + 1]));
        }
    }
    let refs: Vec<(CellType, &[u32])> = cells.iter().map(|(t, n)| (*t, n.as_slice())).collect();
    Ok(DataSet::new(
        format!("{}-dashed", lines.id),
        positions,
        Cells::from_cells(&refs),
    ))
}

fn tangent(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len == 0.0 {
        return [1.0, 0.0, 0.0];
    }
    [d[0] / len, d[1] / len, d[2] / len]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn initial_normal(tan: [f64; 3]) -> [f64; 3] {
    let reference = if tan[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let c = cross(reference, tan);
    let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    [c[0] / len, c[1] / len, c[2] / len]
}

/// Projects `normal` onto the plane perpendicular to `tan` and normalises.
fn transport(normal: [f64; 3], tan: [f64; 3]) -> [f64; 3] {
    let dot = normal[0] * tan[0] + normal[1] * tan[1] + normal[2] * tan[2];
    let p = [
        normal[0] - dot * tan[0],
        normal[1] - dot * tan[1],
        normal[2] - dot * tan[2],
    ];
    let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
    if len < 1e-12 {
        return initial_normal(tan);
    }
    [p[0] / len, p[1] / len, p[2] / len]
}

fn side_direction(tan: [f64; 3]) -> [f64; 3] {
    let up = if tan[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let c = cross(tan, up);
    let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    [c[0] / len, c[1] / len, c[2] / len]
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight_lines() -> DataSet {
        // One polyline (0,0,0)-(1,0,0)-(2,0,0) as two line cells.
        DataSet::new(
            "l",
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            Cells::from_cells(&[
                (CellType::Line, &[0u32, 1][..]),
                (CellType::Line, &[1u32, 2][..]),
            ]),
        )
    }

    #[test]
    fn tubes_wrap_the_polyline() {
        let tubes = streamtubes(&straight_lines(), 0.1, 4).unwrap();
        assert_eq!(tubes.point_count(), 12);
        assert_eq!(tubes.cell_count(), 16);
        assert_eq!(tubes.validate(), Ok(()));
        // Every tube vertex sits at distance `radius` from the x axis.
        for p in tubes.positions.iter() {
            let r = (p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((r - 0.1).abs() < 1e-9, "radius {r}");
        }
    }

    #[test]
    fn ribbons_have_constant_width() {
        let ribbons = stream_ribbons(&straight_lines(), 0.4).unwrap();
        assert_eq!(ribbons.cell_count(), 4);
        let width = (ribbons.positions[1][1] - ribbons.positions[0][1]).abs()
            + (ribbons.positions[1][2] - ribbons.positions[0][2]).abs();
        assert!(width > 0.0);
        assert_eq!(ribbons.validate(), Ok(()));
    }

    #[test]
    fn dashes_cover_the_dash_fraction_and_march_with_phase() {
        let line = straight_lines();
        let total = |ds: &DataSet| -> f64 {
            (0..ds.cell_count())
                .map(|c| {
                    let n = ds.cells.cell(c).unwrap();
                    dist(ds.positions[n[0] as usize], ds.positions[n[1] as usize])
                })
                .sum()
        };
        let a = dashed_lines(&line, 0.0, 0.5, 0.5).unwrap();
        let b = dashed_lines(&line, 0.5, 0.5, 0.5).unwrap();
        assert!((total(&a) - 1.0).abs() < 1e-9, "dash total {}", total(&a));
        assert!((total(&b) - 1.0).abs() < 1e-9);
        // Phase shift moves the pattern: different first segment start.
        assert_ne!(a.positions[0], b.positions[0]);
    }
}
