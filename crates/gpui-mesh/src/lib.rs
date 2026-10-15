//! Mesh data model for 2D and 3D visualization.
//!
//! This crate owns exactly one job: representing datasets. It has no GPUI,
//! wgpu, file-I/O, threading, or time source dependency, so it compiles and
//! behaves identically on native targets and `wasm32-unknown-unknown`.
//!
//! Positions are `f64` at every public boundary. Large datasets are held in
//! shared `Arc` slices and consumed through chunk iterators, so callers never
//! need to materialize a second monolithic copy for rendering.

// Rust guideline compliant 2026-02-21

mod chunk;
mod dataset;
mod field;
mod time_series;

pub use chunk::{SurfaceChunk, surface_chunks};
pub use dataset::{Bounds, CellType, Cells, DataSet, DataSetKind, MeshError};
pub use field::{Association, Field, FieldData};
pub use time_series::TimeSeries;
