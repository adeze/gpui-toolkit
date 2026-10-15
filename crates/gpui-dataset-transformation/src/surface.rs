//! Surface geometry: point normals and feature edges.
//!
//! Normals are angle-weighted averages of boundary-triangle normals (the
//! standard smooth shading normals). Feature edges are boundary mesh
//! edges whose adjacent faces meet above an angle threshold, plus all
//! boundary edges — the classic wireframe-of-features overlay.

// Rust guideline compliant 2026-02-21

use crate::TransformError;
use gpui_mesh::{CellType, Cells, DataSet, Field, boundary_triangles};
use std::collections::BTreeMap;

/// Appends a `normal` vector point field from boundary triangles.
///
/// Points not touched by any boundary triangle get a zero normal.
///
/// # Errors
///
/// Returns an error when the dataset is invalid.
pub fn surface_normals(dataset: &DataSet) -> Result<DataSet, TransformError> {
    dataset.validate()?;
    let mut normals = vec![[0.0f64; 3]; dataset.point_count()];
    for tri in boundary_triangles(dataset) {
        let p: Vec<[f64; 3]> = tri.iter().map(|&n| dataset.positions[n as usize]).collect();
        let n = face_normal(p[0], p[1], p[2]);
        // Angle weighting: corner angle at each vertex.
        for corner in 0..3 {
            let a = p[corner];
            let b = p[(corner + 1) % 3];
            let c = p[(corner + 2) % 3];
            let angle = angle_at(a, b, c);
            let target = &mut normals[tri[corner] as usize];
            target[0] += n[0] * angle;
            target[1] += n[1] * angle;
            target[2] += n[2] * angle;
        }
    }
    for n in &mut normals {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 0.0 {
            n[0] /= len;
            n[1] /= len;
            n[2] /= len;
        }
    }
    let mut out = dataset.clone();
    out.point_data.retain(|f| f.name.as_ref() != "normal");
    out.point_data.push(Field::vector("normal", normals));
    Ok(out)
}

/// Extracts feature edges: mesh edges where the dihedral angle exceeds
/// `angle_degrees`, plus every boundary edge.
///
/// Output is a line dataset over the input points.
///
/// # Errors
///
/// Returns an error when the dataset is invalid.
pub fn feature_edges(dataset: &DataSet, angle_degrees: f64) -> Result<DataSet, TransformError> {
    dataset.validate()?;
    let triangles = boundary_triangles(dataset);
    // Map each undirected edge to its adjacent face normals.
    let mut edges: BTreeMap<(u32, u32), Vec<[f64; 3]>> = BTreeMap::new();
    for tri in &triangles {
        let p: Vec<[f64; 3]> = tri.iter().map(|&n| dataset.positions[n as usize]).collect();
        let n = face_normal(p[0], p[1], p[2]);
        for e in 0..3 {
            let (a, b) = (tri[e], tri[(e + 1) % 3]);
            let key = if a < b { (a, b) } else { (b, a) };
            edges.entry(key).or_default().push(n);
        }
    }
    let threshold = angle_degrees.to_radians().cos();
    let mut cells: Vec<(CellType, Vec<u32>)> = Vec::new();
    for ((a, b), normals) in &edges {
        let keep = if normals.len() == 1 {
            true // boundary edge
        } else {
            // Sharp when any adjacent pair exceeds the threshold.
            normals.windows(2).any(|pair| {
                pair[0][0] * pair[1][0] + pair[0][1] * pair[1][1] + pair[0][2] * pair[1][2]
                    < threshold
            })
        };
        if keep {
            cells.push((CellType::Line, vec![*a, *b]));
        }
    }
    let refs: Vec<(CellType, &[u32])> = cells.iter().map(|(t, n)| (*t, n.as_slice())).collect();
    Ok(DataSet::new(
        format!("{}-feature-edges", dataset.id),
        dataset.positions.as_ref().to_vec(),
        Cells::from_cells(&refs),
    ))
}

fn face_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len == 0.0 {
        return [0.0; 3];
    }
    [n[0] / len, n[1] / len, n[2] / len]
}

fn angle_at(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let la = (ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2]).sqrt();
    let lc = (ac[0] * ac[0] + ac[1] * ac[1] + ac[2] * ac[2]).sqrt();
    if la == 0.0 || lc == 0.0 {
        return 0.0;
    }
    ((ab[0] * ac[0] + ab[1] * ac[1] + ab[2] * ac[2]) / (la * lc))
        .clamp(-1.0, 1.0)
        .acos()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> DataSet {
        DataSet::structured_grid("c", [2, 2, 2], [0.0; 3], [1.0; 3]).unwrap()
    }

    #[test]
    fn cube_normals_point_outward() {
        let out = surface_normals(&cube()).unwrap();
        let normals = out
            .field(gpui_mesh::Association::Point, "normal")
            .unwrap()
            .as_vector()
            .unwrap();
        // Corner (0,0,0) touches three faces: normal is (-1,-1,-1)/sqrt(3).
        let corner = out
            .positions
            .iter()
            .position(|p| *p == [0.0, 0.0, 0.0])
            .unwrap();
        let n = normals[corner];
        assert!(n.iter().all(|c| *c < -0.4), "corner normal {n:?}");
        assert!(
            normals
                .iter()
                .all(|v| { (v[0] * v[0] + v[1] * v[1] + v[2] * v[2] - 1.0).abs() < 1e-9 })
        );
    }

    #[test]
    fn cube_feature_edges_are_the_twelve_cube_edges() {
        let edges = feature_edges(&cube(), 30.0).unwrap();
        assert_eq!(edges.cell_count(), 12);
        // A flat sheet has no sharp edges beyond its boundary.
        let flat = DataSet::surface(
            "f",
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            &[[0, 1, 2], [0, 2, 3]],
        );
        let edges = feature_edges(&flat, 30.0).unwrap();
        assert_eq!(edges.cell_count(), 4);
    }
}
