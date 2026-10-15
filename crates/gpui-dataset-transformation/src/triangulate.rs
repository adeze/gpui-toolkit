//! Point-set geometry: 2D Delaunay triangulation and 3D convex hull.
//!
//! Delaunay triangulates the XY projection (points are expected roughly
//! in a z = const arrangement, like scanned surfaces) with the
//! Bowyer-Watson algorithm. The convex hull is the classic incremental
//! algorithm with horizon retriangulation; both emit triangle datasets
//! over the input positions.

// Rust guideline compliant 2026-02-21

use crate::TransformError;
use gpui_mesh::{CellType, Cells, DataSet};

/// Delaunay-triangulates points by their XY projection.
///
/// # Errors
///
/// Returns an error for fewer than 3 points or a degenerate (collinear)
/// set.
pub fn delaunay_2d(points: &[[f64; 3]]) -> Result<DataSet, TransformError> {
    if points.len() < 3 {
        return Err(TransformError::Degenerate(
            "Delaunay needs at least 3 points".into(),
        ));
    }
    // Supertriangle large enough to contain every point.
    let (mut min, mut max) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in points {
        min[0] = min[0].min(p[0]);
        min[1] = min[1].min(p[1]);
        max[0] = max[0].max(p[0]);
        max[1] = max[1].max(p[1]);
    }
    let span = ((max[0] - min[0]).max(max[1] - min[1])).max(1.0) * 20.0;
    let centre = [f64::midpoint(min[0], max[0]), f64::midpoint(min[1], max[1])];
    let mut xy: Vec<[f64; 2]> = points.iter().map(|p| [p[0], p[1]]).collect();
    xy.push([centre[0] - span, centre[1] - span]);
    xy.push([centre[0], centre[1] + span]);
    xy.push([centre[0] + span, centre[1] - span]);
    let n = points.len();
    let mut triangles: Vec<[usize; 3]> = vec![[n, n + 1, n + 2]];
    for (index, point) in xy.iter().enumerate().take(n) {
        let mut bad: Vec<usize> = Vec::new();
        for (ti, tri) in triangles.iter().enumerate() {
            if in_circumcircle(xy[tri[0]], xy[tri[1]], xy[tri[2]], *point) {
                bad.push(ti);
            }
        }
        // Boundary edges of the bad region (used by exactly one bad tri).
        let mut boundary: Vec<(usize, usize)> = Vec::new();
        for &ti in &bad {
            let tri = triangles[ti];
            for e in 0..3 {
                let edge = (tri[e], tri[(e + 1) % 3]);
                let reversed = (edge.1, edge.0);
                if let Some(pos) = boundary.iter().position(|b| *b == reversed) {
                    boundary.remove(pos);
                } else {
                    boundary.push(edge);
                }
            }
        }
        let bad_triangles: Vec<[usize; 3]> = bad.iter().map(|&ti| triangles[ti]).collect();
        triangles.retain(|tri| !bad_triangles.contains(tri));
        for (a, b) in boundary {
            triangles.push([a, b, index]);
        }
    }
    let cells: Vec<(CellType, Vec<u32>)> = triangles
        .iter()
        .filter(|tri| tri.iter().all(|&v| v < n))
        .map(|tri| {
            (
                CellType::Triangle,
                vec![tri[0] as u32, tri[1] as u32, tri[2] as u32],
            )
        })
        .collect();
    if cells.is_empty() {
        return Err(TransformError::Degenerate(
            "Delaunay produced no triangles (degenerate point set)".into(),
        ));
    }
    let refs: Vec<(CellType, &[u32])> = cells.iter().map(|(t, n)| (*t, n.as_slice())).collect();
    Ok(DataSet::new(
        "delaunay",
        points.to_vec(),
        Cells::from_cells(&refs),
    ))
}

fn in_circumcircle(a: [f64; 2], b: [f64; 2], c: [f64; 2], p: [f64; 2]) -> bool {
    // In-circle determinant (positive inside for CCW triangles).
    let (ax, ay) = (a[0] - p[0], a[1] - p[1]);
    let (bx, by) = (b[0] - p[0], b[1] - p[1]);
    let (cx, cy) = (c[0] - p[0], c[1] - p[1]);
    let det = (ax * ax + ay * ay) * (bx * cy - cx * by) - (bx * bx + by * by) * (ax * cy - cx * ay)
        + (cx * cx + cy * cy) * (ax * by - bx * ay);
    let orient = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if orient > 0.0 { det > 0.0 } else { det < 0.0 }
}

/// Convex hull of a 3D point set (incremental, horizon retriangulation).
///
/// # Errors
///
/// Returns an error for fewer than 4 points or a coplanar set.
pub fn convex_hull(points: &[[f64; 3]]) -> Result<DataSet, TransformError> {
    if points.len() < 4 {
        return Err(TransformError::Degenerate(
            "convex hull needs at least 4 points".into(),
        ));
    }
    // Seed: first four non-coplanar points form a tetrahedron.
    let mut seed: Vec<usize> = vec![0];
    for (i, p) in points.iter().enumerate().skip(1) {
        if seed.len() == 1 && dist(*p, points[seed[0]]) > 1e-12 {
            seed.push(i);
        } else if seed.len() == 2 {
            let a = points[seed[0]];
            let b = points[seed[1]];
            let ab = sub(b, a);
            let ap = sub(*p, a);
            if norm(cross(ab, ap)) > 1e-12 {
                seed.push(i);
            }
        } else if seed.len() == 3 {
            let volume = orient(points[seed[0]], points[seed[1]], points[seed[2]], *p);
            if volume.abs() > 1e-12 {
                seed.push(i);
            }
        }
        if seed.len() == 4 {
            break;
        }
    }
    if seed.len() < 4 {
        return Err(TransformError::Degenerate(
            "point set is coplanar or collinear".into(),
        ));
    }
    let [a, b, c, d] = [seed[0], seed[1], seed[2], seed[3]];
    // Tetrahedron faces oriented outward.
    let mut faces: Vec<[usize; 3]> = Vec::new();
    let add_face = |faces: &mut Vec<[usize; 3]>, x: usize, y: usize, z: usize, opposite: usize| {
        if orient(points[x], points[y], points[z], points[opposite]) > 0.0 {
            faces.push([x, z, y]);
        } else {
            faces.push([x, y, z]);
        }
    };
    add_face(&mut faces, a, b, c, d);
    add_face(&mut faces, a, b, d, c);
    add_face(&mut faces, a, c, d, b);
    add_face(&mut faces, b, c, d, a);
    let inside: Vec<bool> = {
        let mut marks = vec![false; points.len()];
        for &s in &seed {
            marks[s] = true;
        }
        marks
    };
    for (index, point) in points.iter().enumerate() {
        if inside[index] {
            continue;
        }
        // Visible faces: point strictly outside their plane.
        let visible: Vec<bool> = faces
            .iter()
            .map(|f| orient(points[f[0]], points[f[1]], points[f[2]], *point) > 1e-12)
            .collect();
        if visible.iter().all(|v| !v) {
            continue; // interior point
        }
        // Horizon edges: shared by one visible and one hidden face.
        let mut horizon: Vec<(usize, usize)> = Vec::new();
        for (fi, f) in faces.iter().enumerate() {
            if !visible[fi] {
                continue;
            }
            for e in 0..3 {
                let edge = (f[e], f[(e + 1) % 3]);
                // Counts if the neighbour across the reversed edge is hidden.
                let neighbour_visible = faces.iter().enumerate().any(|(gi, g)| {
                    gi != fi && visible[gi] && g.contains(&edge.1) && g.contains(&edge.0)
                });
                if !neighbour_visible {
                    horizon.push(edge);
                }
            }
        }
        faces = faces
            .iter()
            .zip(visible.iter())
            .filter(|(_, v)| !**v)
            .map(|(f, _)| *f)
            .collect();
        for (x, y) in horizon {
            faces.push([x, y, index]);
        }
    }
    let cells: Vec<(CellType, Vec<u32>)> = faces
        .iter()
        .map(|f| {
            (
                CellType::Triangle,
                vec![f[0] as u32, f[1] as u32, f[2] as u32],
            )
        })
        .collect();
    let refs: Vec<(CellType, &[u32])> = cells.iter().map(|(t, n)| (*t, n.as_slice())).collect();
    Ok(DataSet::new(
        "convex-hull",
        points.to_vec(),
        Cells::from_cells(&refs),
    ))
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    norm(sub(a, b))
}

/// Six times the signed volume of tetrahedron (a, b, c, d).
fn orient(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> f64 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ad = sub(d, a);
    ab[0] * (ac[1] * ad[2] - ac[2] * ad[1]) - ab[1] * (ac[0] * ad[2] - ac[2] * ad[0])
        + ab[2] * (ac[0] * ad[1] - ac[1] * ad[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delaunay_covers_the_point_set() {
        let mut points = Vec::new();
        for j in 0..4 {
            for i in 0..4 {
                points.push([i as f64, j as f64, 0.0]);
            }
        }
        let out = delaunay_2d(&points).unwrap();
        // Total triangle area equals the 3x3 domain area.
        let area: f64 = (0..out.cell_count())
            .map(|cell| {
                let n = out.cells.cell(cell).unwrap();
                let (a, b, c) = (
                    out.positions[n[0] as usize],
                    out.positions[n[1] as usize],
                    out.positions[n[2] as usize],
                );
                0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs()
            })
            .sum();
        assert!((area - 9.0).abs() < 1e-9, "area {area}");
        assert_eq!(out.validate(), Ok(()));
    }

    #[test]
    fn hull_of_cube_corners_has_unit_volume() {
        let mut points = Vec::new();
        for i in 0..8 {
            points.push([
                f64::from(i & 1),
                f64::from((i >> 1) & 1),
                f64::from((i >> 2) & 1),
            ]);
        }
        points.push([0.5, 0.5, 0.5]); // interior: must not appear as vertex use
        let out = convex_hull(&points).unwrap();
        assert_eq!(out.cell_count(), 12);
        // Signed volume via the divergence theorem.
        let volume: f64 = (0..out.cell_count())
            .map(|cell| {
                let n = out.cells.cell(cell).unwrap();
                let (a, b, c) = (
                    out.positions[n[0] as usize],
                    out.positions[n[1] as usize],
                    out.positions[n[2] as usize],
                );
                orient([0.0; 3], a, b, c) / 6.0
            })
            .sum();
        assert!((volume.abs() - 1.0).abs() < 1e-9, "volume {volume}");
    }
}
