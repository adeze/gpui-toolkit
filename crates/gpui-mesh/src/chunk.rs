//! Boundary-surface extraction and bounded-size chunking for large datasets.

// Rust guideline compliant 2026-02-21

use crate::dataset::{CellType, DataSet};
use std::collections::HashMap;

/// One bounded chunk of extracted surface triangles.
///
/// Indices are local to the chunk (`u32`), positions keep the dataset's
/// `f64` coordinates, and `point_ids` maps each local vertex back to the
/// dataset point index so field lookups and picking stay exact. A chunk
/// never exceeds the caller's triangle budget, which is what lets a
/// multi-million-cell dataset upload to a GPU buffer (or a wasm heap)
/// in pieces instead of as one monolithic allocation.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceChunk {
    /// Dataset point index for each local vertex, in first-use order.
    pub point_ids: Vec<u32>,
    /// Local triangle indices, three per triangle.
    pub indices: Vec<u32>,
}

/// Face tables for volume cells, as local node-index quads/triangles.
/// Hexahedron faces follow VTK node ordering.
fn volume_faces(kind: CellType) -> &'static [[usize; 4]] {
    match kind {
        CellType::Hexahedron => &[
            [0, 3, 2, 1],
            [4, 5, 6, 7],
            [0, 1, 5, 4],
            [1, 2, 6, 5],
            [2, 3, 7, 6],
            [3, 0, 4, 7],
        ],
        CellType::Wedge => &[
            [0, 2, 1, 1],
            [3, 4, 5, 5],
            [0, 1, 4, 3],
            [1, 2, 5, 4],
            [2, 0, 3, 5],
        ],
        CellType::Pyramid => &[
            [0, 3, 2, 1],
            [0, 1, 4, 4],
            [1, 2, 4, 4],
            [2, 3, 4, 4],
            [3, 0, 4, 4],
        ],
        _ => &[],
    }
}

/// Extracts boundary triangles as dataset point-index triples.
///
/// Surface cells contribute directly (quads split on the 0-2 diagonal).
/// Volume cells contribute only faces owned by a single cell, so interior
/// faces between two volume cells are culled. Face identity is the sorted
/// node set of the whole face polygon, so the culling does not depend on
/// which diagonal each owner would have chosen; surviving faces are
/// triangulated from the owning cell's node order.
#[must_use]
pub fn boundary_triangles(dataset: &DataSet) -> Vec<[u32; 3]> {
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    // Sorted-node face key -> owning polygon (None once a second owner appears).
    let mut faces: HashMap<Vec<u32>, Option<Vec<u32>>> = HashMap::new();
    for cell in 0..dataset.cell_count() {
        let Some(nodes) = dataset.cells.cell(cell) else {
            continue;
        };
        let kind = dataset.cells.types[cell];
        match kind {
            CellType::Triangle => {
                triangles.push([nodes[0], nodes[1], nodes[2]]);
            }
            CellType::Quad => {
                triangles.push([nodes[0], nodes[1], nodes[2]]);
                triangles.push([nodes[0], nodes[2], nodes[3]]);
            }
            CellType::Polygon => {
                // Fan triangulation around the first node.
                for i in 1..nodes.len() - 1 {
                    triangles.push([nodes[0], nodes[i], nodes[i + 1]]);
                }
            }
            CellType::Tetra | CellType::Hexahedron | CellType::Wedge | CellType::Pyramid => {
                for face in volume_faces(kind) {
                    // Triangular faces are stored with a repeated last node.
                    let mut polygon: Vec<u32> = face.iter().map(|&i| nodes[i]).collect();
                    polygon.dedup();
                    if polygon.len() < 3 {
                        continue;
                    }
                    let mut key = polygon.clone();
                    key.sort_unstable();
                    match faces.get(&key) {
                        None => {
                            faces.insert(key, Some(polygon));
                        }
                        Some(_) => {
                            faces.insert(key, None);
                        }
                    }
                }
                if kind == CellType::Tetra {
                    // Tetra faces in `volume_faces` are empty; use its triangles.
                    for face in [[0usize, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]] {
                        let polygon: Vec<u32> = face.iter().map(|&i| nodes[i]).collect();
                        let mut key = polygon.clone();
                        key.sort_unstable();
                        match faces.get(&key) {
                            None => {
                                faces.insert(key, Some(polygon));
                            }
                            Some(_) => {
                                faces.insert(key, None);
                            }
                        }
                    }
                }
            }
            CellType::Vertex | CellType::Line => {}
        }
    }
    for polygon in faces.into_values().flatten() {
        triangles.push([polygon[0], polygon[1], polygon[2]]);
        if polygon.len() == 4 {
            triangles.push([polygon[0], polygon[2], polygon[3]]);
        }
    }
    triangles
}

/// Splits the boundary surface into chunks of at most `max_triangles` each.
///
/// # Panics
///
/// Panics when `max_triangles` is zero, which can never produce a chunk.
#[must_use]
pub fn surface_chunks(dataset: &DataSet, max_triangles: usize) -> Vec<SurfaceChunk> {
    assert!(max_triangles > 0, "max_triangles must be at least 1");
    let triangles = boundary_triangles(dataset);
    let mut chunks = Vec::new();
    for group in triangles.chunks(max_triangles) {
        let mut local: HashMap<u32, u32> = HashMap::new();
        let mut point_ids: Vec<u32> = Vec::new();
        let mut indices: Vec<u32> = Vec::with_capacity(group.len() * 3);
        for tri in group {
            for &point in tri {
                let id = *local.entry(point).or_insert_with(|| {
                    let id = point_ids.len() as u32;
                    point_ids.push(point);
                    id
                });
                indices.push(id);
            }
        }
        chunks.push(SurfaceChunk { point_ids, indices });
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::Cells;

    #[test]
    fn single_hexahedron_has_twelve_boundary_triangles() {
        let ds = DataSet::structured_grid("g", [2, 2, 2], [0.0; 3], [1.0; 3]).unwrap();
        assert_eq!(boundary_triangles(&ds).len(), 12);
    }

    #[test]
    fn two_hexahedra_cull_the_shared_face() {
        let ds = DataSet::structured_grid("g", [3, 2, 2], [0.0; 3], [1.0; 3]).unwrap();
        // Boundary quads: 2*6 - 2 for the culled shared face = 10 quads = 20 triangles.
        assert_eq!(boundary_triangles(&ds).len(), 20);
    }

    #[test]
    fn chunks_respect_budget_and_round_trip_points() {
        let ds = DataSet::structured_grid("g", [5, 5, 5], [0.0; 3], [1.0; 3]).unwrap();
        let total = boundary_triangles(&ds).len();
        let chunks = surface_chunks(&ds, 10);
        assert!(chunks.iter().all(|c| c.indices.len() / 3 <= 10));
        assert_eq!(
            chunks.iter().map(|c| c.indices.len() / 3).sum::<usize>(),
            total
        );
        for chunk in &chunks {
            for &local in &chunk.indices {
                assert!((local as usize) < chunk.point_ids.len());
            }
        }
        let _ = Cells::triangles(&[[0, 1, 2]]);
    }
}
