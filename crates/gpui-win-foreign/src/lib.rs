//! Foreign-parent editor embedding for Windows.
//!
//! GPUI editors hosted inside host-owned `HWND`s (VST3 plugin editors).
//! The window-creation hook lives in the `gpui_windows` fork backend
//! (`WS_CHILD` path); this crate owns the lifecycle registry, frame
//! pump, resize policy, and the C ABI the host calls.
//!
//! `policy` holds the host-portable geometry/id validation with
//! unit tests; the Windows-only `foreign_parent` module owns the live
//! editors.
//!
//! Contract: `docs/foreign-parent-editor-embedding.md` §10 (Windows row).

pub mod policy;

#[cfg(target_os = "windows")]
mod foreign_parent;

#[cfg(target_os = "windows")]
pub use foreign_parent::*;
