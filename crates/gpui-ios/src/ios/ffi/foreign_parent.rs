//! Foreign-parent FFI for iOS: C entry points for host view controllers.
//!
//! FFI functions necessarily dereference raw pointers from C callers; every
//! dereference carries a SAFETY comment instead.
//!
//! Implements the host/guest boundary of
//! `docs/foreign-parent-editor-embedding.md` for `AUv3` plugin views and
//! other hosts that own the `UIView`. The host pumps frames from its
//! `CADisplayLink`, forwards resizes, and reports focus/hover; touches,
//! hardware keys, and marked-text sessions route through the shared view
//! classes with no per-event forwarding.
//!
//! Each editor instance is keyed by id in a shared registry (multi-editor
//! hosts work); the opaque context pins the `AppCell` past the synchronous
//! platform `run()`.

use super::super::window::{IosWindow, PENDING_PARENT, ParentViewInfo};
use gpui::{
    App, AppCell, AppContext, Context, Entity, IntoElement, Render, Styled, Window, WindowOptions,
    div,
};
use gpui_foreign_parent::{EditorRegistry, optional_c_string};
use objc::runtime::Object;
use std::cell::Cell;
use std::ffi::{c_char, c_void};
use std::rc::Rc;
use std::sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
};

#[cfg(not(test))]
fn assert_main_thread() {
    use objc::{class, msg_send, runtime::BOOL, sel, sel_impl};
    // SAFETY: `NSThread` and its `isMainThread` class method always
    // exist; the message takes no arguments and returns a `BOOL`.
    unsafe {
        let is_main: BOOL = msg_send![class!(NSThread), isMainThread];
        assert!(is_main, "foreign iOS windows must be used on the main thread");
    }
}

#[cfg(test)]
fn assert_main_thread() {}

/// Live foreign editor instances, keyed by editor id.
static FOREIGN_WINDOWS: OnceLock<EditorRegistry<u64>> = OnceLock::new();

/// Access the foreign window registry.
pub(crate) fn foreign_registry() -> &'static EditorRegistry<u64> {
    FOREIGN_WINDOWS.get_or_init(|| EditorRegistry::new(assert_main_thread))
}

static NEXT_EDITOR_ID: AtomicU64 = AtomicU64::new(1);

/// Opaque context handle passed to/from the host.
pub struct IosForeignContext {
    _label: String,
    editor_id: u64,
    /// Pins the `AppCell` past the synchronous foreign `run()`, mirroring
    /// the AU backend's lifetime management.
    _app_cell: Rc<AppCell>,
}

/// Neutral placeholder root for the C entry point.
///
/// Rust hosts inject real editor content through
/// [`gpui_ios_foreign_create_with`].
struct PlaceholderRootView;

impl Render for PlaceholderRootView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

fn with_foreign_window<R>(editor_id: u64, f: impl FnOnce(&IosWindow) -> R) -> Option<R> {
    foreign_registry().with(&editor_id, f)
}

/// Create a GPUI context embedded in a host `UIView`, with content.
///
/// Rust-native constructor behind [`gpui_ios_foreign_create`]: plugin
/// crates call this with their own root-view factory instead of going
/// through the C ABI. Returns null when the view is null or the window
/// fails to open.
///
/// # Safety
///
/// `parent_view` must be null or point to a live `UIView` that outlives
/// the returned context. All calls must happen on the host main thread.
pub unsafe fn gpui_ios_foreign_create_with<V: Render + 'static>(
    parent_view: *mut Object,
    width: f32,
    height: f32,
    scale: f32,
    label: &str,
    build_root_view: impl FnOnce(&mut Window, &mut App) -> Entity<V> + 'static,
) -> *mut IosForeignContext {
    if parent_view.is_null() {
        log::error!("GPUI iOS foreign: null parent view");
        return std::ptr::null_mut();
    }
    log::info!(
        "GPUI iOS foreign: create label={label} size={width}x{height} @{scale:.1}x view={parent_view:p}"
    );

    let editor_id = NEXT_EDITOR_ID.fetch_add(1, Ordering::Relaxed);

    // One-shot handoff so `open_window` routes to the foreign constructor.
    PENDING_PARENT.with(|pending| {
        *pending.borrow_mut() = Some(ParentViewInfo {
            parent_view,
            width,
            height,
            scale,
            editor_id,
        });
    });

    let platform = Rc::new(super::super::platform::IosPlatform::new_foreign());
    let app = gpui::Application::with_platform(platform);
    // The foreign `run()` invokes the launch callback synchronously, so
    // pin the app cell for the lifetime of the editor.
    let app_cell = app.clone_app_cell();

    let window_opened = Rc::new(Cell::new(false));
    let window_opened_clone = Rc::clone(&window_opened);
    app.run(move |cx: &mut App| {
        match cx.open_window(
            WindowOptions {
                window_bounds: None,
                ..Default::default()
            },
            build_root_view,
        ) {
            Ok(_handle) => {
                window_opened_clone.set(true);
            }
            Err(e) => {
                log::error!("GPUI iOS foreign: open_window failed: {e:#}");
            }
        }
    });

    if !window_opened.get() {
        log::error!("GPUI iOS foreign: returning null because open_window failed");
        PENDING_PARENT.with(|pending| *pending.borrow_mut() = None);
        foreign_registry().unregister(&editor_id);
        return std::ptr::null_mut();
    }

    Box::into_raw(Box::new(IosForeignContext {
        _label: label.to_string(),
        editor_id,
        _app_cell: app_cell,
    }))
}

/// Create a GPUI context embedded in a host `UIView`.
///
/// Null pointers are rejected; otherwise `parent_view` must be a live
/// `UIView` and `label` a valid C string. Renders the neutral placeholder
/// root; Rust hosts inject real content through
/// [`gpui_ios_foreign_create_with`].
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_foreign_create(
    parent_view: *mut c_void,
    width: f32,
    height: f32,
    scale: f32,
    label: *const c_char,
) -> *mut IosForeignContext {
    if parent_view.is_null() || label.is_null() {
        log::error!("GPUI iOS foreign: null pointer argument");
        return std::ptr::null_mut();
    }
    // SAFETY: `label` is null-checked above; the host passes a valid C
    // string whose bytes are copied out immediately.
    let Some(label) = (unsafe { optional_c_string(label) }) else {
        log::error!("GPUI iOS foreign: invalid UTF-8 label");
        return std::ptr::null_mut();
    };
    // SAFETY: `parent_view` is null-checked above and documented as a
    // live `UIView` outliving the context; hosts call on the main thread.
    unsafe {
        gpui_ios_foreign_create_with(
            parent_view.cast::<Object>(),
            width,
            height,
            scale,
            &label,
            |_, cx| cx.new(|_| PlaceholderRootView),
        )
    }
}

/// Destroy a foreign GPUI context. Null-safe; each context is destroyed
/// at most once.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_foreign_destroy(context: *mut IosForeignContext) {
    if context.is_null() {
        return;
    }
    // SAFETY: `context` came from `Box::into_raw` in the foreign create
    // entry point and the host destroys each context at most once.
    let context = unsafe { Box::from_raw(context) };
    foreign_registry().unregister(&context.editor_id);
}

/// Request one frame of GPUI rendering.
/// Call from the host `CADisplayLink` callback on the main thread.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_foreign_request_frame(context: *mut IosForeignContext) {
    if context.is_null() {
        return;
    }
    // SAFETY: non-null context created by the foreign create entry point;
    // only the id is read.
    let editor_id = unsafe { (*context).editor_id };
    with_foreign_window(editor_id, IosWindow::request_frame_foreign);
}

/// Handle view resize from the host.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_foreign_resize(
    context: *mut IosForeignContext,
    width: f32,
    height: f32,
    scale: f32,
) {
    if context.is_null() {
        return;
    }
    // SAFETY: non-null context created by the foreign create entry point;
    // only the id is read.
    let editor_id = unsafe { (*context).editor_id };
    with_foreign_window(editor_id, |window| {
        window.resize_foreign(width, height, scale);
    });
}

/// Report host focus changes for GPUI window activation state.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_foreign_set_active(context: *mut IosForeignContext, is_active: bool) {
    if context.is_null() {
        return;
    }
    // SAFETY: non-null context created by the foreign create entry point;
    // only the id is read.
    let editor_id = unsafe { (*context).editor_id };
    with_foreign_window(editor_id, |window| window.set_active_foreign(is_active));
}

/// Report whether the pointer is currently inside the host view.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_foreign_set_hovered(context: *mut IosForeignContext, is_hovered: bool) {
    if context.is_null() {
        return;
    }
    // SAFETY: non-null context created by the foreign create entry point;
    // only the id is read.
    let editor_id = unsafe { (*context).editor_id };
    with_foreign_window(editor_id, |window| {
        window.set_hovered_foreign(is_hovered);
    });
}

/// Realtime health counters: frames dropped on a busy renderer and
/// display-link ticks coalesced by the frame throttle. Each out-pointer is
/// optional; a null context leaves all outputs untouched.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_foreign_frame_stats(
    context: *mut IosForeignContext,
    dropped: *mut usize,
    coalesced: *mut usize,
) {
    if context.is_null() {
        return;
    }
    // SAFETY: non-null context created by the foreign create entry point;
    // only the id is read.
    let editor_id = unsafe { (*context).editor_id };
    let (dropped_frames, coalesced_frames) =
        with_foreign_window(editor_id, IosWindow::foreign_frame_stats).unwrap_or((0, 0));
    if !dropped.is_null() {
        // SAFETY: `dropped` is null-checked; the host passes a writable
        // out counter.
        unsafe {
            *dropped = dropped_frames;
        }
    }
    if !coalesced.is_null() {
        // SAFETY: `coalesced` is null-checked; the host passes a writable
        // out counter.
        unsafe {
            *coalesced = coalesced_frames;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreign_entry_points_are_null_safe() {
        let context = std::ptr::null_mut();
        gpui_ios_foreign_destroy(context);
        gpui_ios_foreign_request_frame(context);
        gpui_ios_foreign_resize(context, 320.0, 200.0, 2.0);
        gpui_ios_foreign_set_active(context, true);
        gpui_ios_foreign_set_hovered(context, true);
        let (mut dropped, mut coalesced) = (usize::MAX, usize::MAX);
        gpui_ios_foreign_frame_stats(context, &raw mut dropped, &raw mut coalesced);
        assert_eq!((dropped, coalesced), (usize::MAX, usize::MAX));
        gpui_ios_foreign_frame_stats(context, std::ptr::null_mut(), std::ptr::null_mut());
        assert!(
            gpui_ios_foreign_create(
                std::ptr::null_mut(),
                320.0,
                200.0,
                2.0,
                std::ptr::null()
            )
            .is_null()
        );
    }

    #[test]
    fn foreign_create_with_rejects_null_parent() {
        // SAFETY: a null parent is explicitly rejected without dereference.
        let context = unsafe {
            gpui_ios_foreign_create_with(
                std::ptr::null_mut(),
                320.0,
                200.0,
                2.0,
                "test",
                |_, cx| cx.new(|_| PlaceholderRootView),
            )
        };
        assert!(context.is_null());
    }
}
