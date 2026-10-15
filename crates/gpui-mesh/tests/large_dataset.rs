//! Large-dataset behaviour: chunking stays bounded and counts stay exact.

// Rust guideline compliant 2026-02-21

use gpui_mesh::{DataSet, Field, surface_chunks};

#[test]
fn large_structured_volume_chunks_with_bounded_budgets() {
    // 51^3 points, 125k hexahedra: large enough that a monolithic f32
    // conversion would be wasteful, small enough for a debug test run.
    let dataset = DataSet::structured_grid("large", [51, 51, 51], [0.0; 3], [0.02; 3])
        .unwrap()
        .with_field(Field::scalar("pressure", vec![1.0; 51 * 51 * 51]));
    assert_eq!(dataset.validate(), Ok(()));
    assert!(dataset.byte_len() > 3_000_000);

    let chunks = surface_chunks(&dataset, 5_000);
    // Boundary: 6 faces * 50 * 50 quads * 2 triangles.
    let expected: usize = 6 * 50 * 50 * 2;
    let total: usize = chunks.iter().map(|c| c.indices.len() / 3).sum();
    assert_eq!(total, expected);
    assert!(chunks.len() > 1);
    assert!(chunks.iter().all(|c| c.indices.len() / 3 <= 5_000));
    // Chunk-local indices never escape the chunk vertex range.
    for chunk in &chunks {
        let max = chunk.indices.iter().max().copied().unwrap_or(0) as usize;
        assert!(max < chunk.point_ids.len());
    }
}
