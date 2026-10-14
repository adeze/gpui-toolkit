use super::au_window::AuWindow;
use gpui_foreign_parent::EditorRegistry;
use std::sync::OnceLock;

#[cfg(not(test))]
fn assert_main_thread() {
    use objc::{
        class, msg_send,
        runtime::{BOOL, YES},
        sel, sel_impl,
    };
    // SAFETY: `NSThread` and its `isMainThread` class method always
    // exist; the message takes no arguments and returns a `BOOL`.
    unsafe {
        let is_main: BOOL = msg_send![class!(NSThread), isMainThread];
        assert!(
            is_main == YES,
            "AuWindow must only be used on the main thread"
        );
    }
}

#[cfg(test)]
fn assert_main_thread() {}

/// Global window registry, used by FFI entry points to find the window.
/// Single-instance specialization of the shared keyed registry: only one
/// AU GPUI window per process (each AU appex is its own process).
/// Uses a registry with removal (not `OnceLock` alone) to support view
/// destruction and re-creation (common in DAWs).
static AU_WINDOW: OnceLock<EditorRegistry<()>> = OnceLock::new();

/// Access the global AU window registry.
pub(super) fn au_registry() -> &'static EditorRegistry<()> {
    AU_WINDOW.get_or_init(|| EditorRegistry::new(assert_main_thread))
}

/// Execute a callback with a reference to the current AU window, if any.
///
/// Registration, unregistration, and all dereferences are serialized on
/// the host main thread. The registry lock is released before entering
/// arbitrary GPUI or host callbacks so re-entry cannot deadlock.
pub(crate) fn with_au_window<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&AuWindow) -> R,
{
    au_registry().with(&(), f)
}

/// Register the window in the global slot.
/// Called from `AuPlatform::open_window` after Boxing.
pub(crate) fn register_au_window(boxed: &AuWindow) {
    let ptr: *const AuWindow = boxed;
    // SAFETY: `boxed` names the live `AuWindow` outliving registration;
    // the host main thread owns all registry use (asserted inside).
    unsafe { au_registry().register((), ptr.cast::<()>()) };
    let msg = format!("SOTF AuWindow: registered at {ptr:p}");
    crate::helpers::nslog_verbose(msg.as_bytes());
}

/// Unregister the current AU window (called during destroy).
pub(crate) fn unregister_au_window() {
    au_registry().unregister(&());
}
