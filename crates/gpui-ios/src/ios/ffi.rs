//! FFI module for iOS — C-compatible functions called from Objective-C app delegate.

mod app_callback_cell;
mod asset_source_cell;
mod boxed_asset_source;
mod consts;
// FFI functions necessarily dereference raw pointers from C callers; every
// dereference carries a SAFETY comment instead (mirrors gpui-au).
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "FFI boundary: see SAFETY comments"
)]
mod foreign_parent;
mod gpui_mod;
mod ios_app_state;
mod misc;
mod set;
mod take;
#[cfg(test)]
mod tests;
mod window_list_wrapper;

pub(crate) use consts::*;
pub use foreign_parent::*;
pub use gpui_mod::*;
pub use ios_app_state::*;
pub(crate) use misc::request_frame_for_window;
pub use set::*;
