//! 3D scene rendering for datasets.
//!
//! This crate owns exactly one job: turning datasets into pictures. It
//! holds scenes, cameras, transfer functions, chunked GPU-ready buffers,
//! a software rasterizer, and time animation. It has no GPUI, wgpu,
//! file-I/O, threading, or wall-clock dependency by default, so the full
//! pipeline runs identically on native targets and
//! `wasm32-unknown-unknown`; hosts drive playback by supplying times.
//! GPU hosts consume [`prepare::GpuChunk`] buffers directly; the software
//! rasterizer covers headless QA, movie frames, and wasm fallback.

// Rust guideline compliant 2026-02-21

pub mod animate;
pub mod camera;
#[cfg(feature = "gpui")]
pub mod element;
pub mod legend;
pub mod movie;
pub mod prepare;
pub mod raster;
pub mod scene;
pub mod shaders;
pub mod transfer;
pub mod volume;

pub use animate::Player;
pub use camera::{Camera, Projection};
pub use legend::{
    Annotation, ColorBar, ColorBarLayout, Rect, draw_axis_triad, draw_colorbar, layout_colorbar,
    nice_ticks,
};
pub use movie::{MovieError, TimedFrame, frame_rate, validate_frames};
pub use prepare::{GpuChunk, prepare_chunks};
pub use raster::{Frame, render};
pub use scene::{Actor, ClipBox, ClipPlane, Scene};
pub use transfer::TransferFunction;
pub use volume::{VolumeOptions, VolumeRenderer, raycast_volume};

/// Failures reported by scene preparation and rendering.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SceneError {
    /// The actor references a field that does not exist.
    #[error("field '{0}' not found")]
    FieldNotFound(String),
    /// The dataset failed validation.
    #[error(transparent)]
    Mesh(#[from] gpui_mesh::MeshError),
    /// A render target dimension is zero.
    #[error("render size must be non-zero, got {0}x{1}")]
    InvalidSize(u32, u32),
}
