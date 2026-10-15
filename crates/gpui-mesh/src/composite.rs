//! Composite (multi-block) datasets: named blocks with AMR levels.
//!
//! A composite is a flat list of blocks; each block is a full [`DataSet`]
//! plus a name and a refinement `level` (0 = coarsest). This covers VTK
//! multi-block assemblies and patch-based AMR hierarchies at the data
//! level; filters and renderers consume blocks one at a time.

// Rust guideline compliant 2026-02-21

use crate::dataset::{Bounds, DataSet};

/// One named block of a composite dataset.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Block name (patch or part name).
    pub name: String,
    /// Refinement level for AMR patches; 0 for plain assemblies.
    pub level: u32,
    /// The block's dataset.
    pub dataset: DataSet,
}

/// A multi-block dataset: an ordered set of named blocks.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompositeDataSet {
    /// Blocks in assembly order.
    pub blocks: Vec<Block>,
}

impl CompositeDataSet {
    /// Creates an empty composite.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a block at refinement level 0.
    #[must_use]
    pub fn with_block(mut self, name: impl Into<String>, dataset: DataSet) -> Self {
        self.blocks.push(Block {
            name: name.into(),
            level: 0,
            dataset,
        });
        self
    }

    /// Adds an AMR patch at the given refinement level.
    #[must_use]
    pub fn with_patch(mut self, name: impl Into<String>, level: u32, dataset: DataSet) -> Self {
        self.blocks.push(Block {
            name: name.into(),
            level,
            dataset,
        });
        self
    }

    /// Number of blocks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Whether the composite holds no blocks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Blocks at one refinement level, in assembly order.
    pub fn blocks_at_level(&self, level: u32) -> impl Iterator<Item = &Block> {
        self.blocks.iter().filter(move |b| b.level == level)
    }

    /// Highest refinement level present (0 when empty).
    #[must_use]
    pub fn max_level(&self) -> u32 {
        self.blocks.iter().map(|b| b.level).max().unwrap_or(0)
    }

    /// Total point count over all blocks.
    #[must_use]
    pub fn point_count(&self) -> usize {
        self.blocks.iter().map(|b| b.dataset.point_count()).sum()
    }

    /// Total cell count over all blocks.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.blocks.iter().map(|b| b.dataset.cell_count()).sum()
    }

    /// Union of block bounds; `None` when every block is empty.
    #[must_use]
    pub fn bounds(&self) -> Option<Bounds> {
        let mut out: Option<Bounds> = None;
        for block in &self.blocks {
            if let Some(bounds) = block.dataset.bounds() {
                out = Some(match out {
                    None => bounds,
                    Some(mut acc) => {
                        for axis in 0..3 {
                            acc.min[axis] = acc.min[axis].min(bounds.min[axis]);
                            acc.max[axis] = acc.max[axis].max(bounds.max[axis]);
                        }
                        acc
                    }
                });
            }
        }
        out
    }

    /// Total resident bytes over all blocks.
    #[must_use]
    pub fn byte_len(&self) -> u64 {
        self.blocks.iter().map(|b| b.dataset.byte_len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_aggregates_blocks_and_levels() {
        let a = DataSet::structured_grid("a", [3, 3, 3], [0.0; 3], [1.0; 3]).unwrap();
        let b = DataSet::structured_grid("b", [3, 3, 3], [5.0, 0.0, 0.0], [0.5; 3]).unwrap();
        let composite = CompositeDataSet::new()
            .with_block("coarse", a)
            .with_patch("refined", 2, b);
        assert_eq!(composite.len(), 2);
        assert_eq!(composite.max_level(), 2);
        assert_eq!(composite.blocks_at_level(2).count(), 1);
        assert_eq!(composite.point_count(), 54);
        let bounds = composite.bounds().unwrap();
        assert_eq!(bounds.min, [0.0, 0.0, 0.0]);
        assert_eq!(bounds.max[0], 6.0);
    }
}
