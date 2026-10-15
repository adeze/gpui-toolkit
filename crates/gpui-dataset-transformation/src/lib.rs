//! Dataset-to-dataset transformations.
//!
//! This crate owns exactly one job: pure filters that consume a
//! [`gpui_mesh::DataSet`] and produce a new `DataSet`. It has no GPUI, wgpu,
//! file-I/O, threading, or time source dependency, so every filter runs
//! identically on native targets and `wasm32-unknown-unknown`, and can be
//! unit-tested headlessly. Filters never mutate their input; large inputs
//! are processed cell by cell without materializing a second copy of the
//! input geometry.

// Rust guideline compliant 2026-02-21

mod base64;
mod calculator;
mod clip;
mod contours;
mod eigen;
mod flow;
mod glyph;
#[cfg(feature = "gpu")]
pub mod gpu;
mod interp;
mod isosurface;
mod lic;
mod lines;
mod particles;
mod plane;
mod probe;
mod streamline;
mod threshold;
mod vtk_io;
mod vtu;
mod warp;
mod xdmf;

pub use calculator::{complex_magnitude, complex_phase, vector_magnitude};
pub use clip::{clip_halfspace, slice};
pub use contours::contour_lines;
pub use flow::{
    divergence, lambda2, q_criterion, swirling_strength, vorticity, vorticity_magnitude,
};
pub use glyph::{GlyphKind, glyphs, solid_arrows, tensor_glyphs};
pub use isosurface::isosurface;
pub use lic::lic;
pub use lines::{dashed_lines, stream_ribbons, streamtubes};
pub use particles::{ParticleOptions, particle_frames};
pub use plane::Plane;
pub use probe::probe;
pub use streamline::streamlines;
pub use threshold::{ThresholdMode, threshold};
pub use vtk_io::{read_vtk, write_vtk};
pub use vtu::{read_vtu, write_vtu};
pub use warp::{warp_by_scalar, warp_by_vector};
pub use xdmf::{XdmfBundle, read_xdmf, write_xdmf};

use gpui_mesh::CellType;

/// Failures reported by transformations.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TransformError {
    /// The named field does not exist with the requested association.
    #[error("field '{0}' not found")]
    FieldNotFound(String),
    /// The named field has the wrong component shape for this filter.
    #[error("field '{field}' has the wrong shape: {reason}")]
    WrongFieldShape {
        /// Field name.
        field: String,
        /// Why the shape is unsuitable.
        reason: String,
    },
    /// The filter does not handle this cell kind yet.
    #[error("cell kind {0:?} is not supported by this filter")]
    UnsupportedCell(CellType),
    /// A geometric input is degenerate.
    #[error("degenerate input: {0}")]
    Degenerate(String),
    /// The input dataset failed validation.
    #[error(transparent)]
    Mesh(#[from] gpui_mesh::MeshError),
}
