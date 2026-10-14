//! Foreign-parent embedding for Windows: GPUI editors in host `HWND`s.
//!
//! Implements the Windows binding of
//! `docs/foreign-parent-editor-embedding.md` (§10, VST3 row). A host (DAW)
//! hands a parent `HWND` to [`gpui_win_create`], pumps frames from a
//! timer/DWM callback through [`gpui_win_request_frame`], and forwards
//! resizes and focus. Each editor gets its own `Application` (own
//! `AppCell`), keyed in a shared [`EditorRegistry`] by editor id so
//! multi-editor hosts work.
//!
//! The host owns the message loop: mouse, keyboard, wheel, and IME traffic
//! arrives at the `WS_CHILD` window as ordinary Win32 messages dispatched
//! by the host `GetMessage` loop, so — unlike the AU binding — this crate
//! needs no input-forwarding FFI. What the host must drive explicitly:
//!
//! - Frames: call [`gpui_win_request_frame`] on the UI thread (a steady
//!   timer or DWM-driven tick). Ticks faster than the 4 ms minimum
//!   interval coalesce instead of queueing.
//! - Resize: call [`gpui_win_resize`]; bounds track the newest host size
//!   immediately while the swapchain reconfigure debounces (16 ms).
//! - Focus/hover: call [`gpui_win_set_active`]/[`gpui_win_set_hovered`];
//!   child windows never see `WM_ACTIVATE`, and delivery is
//!   transition-only.
//! - Detach: call [`gpui_win_destroy`], which destroys the child `HWND`
//!   synchronously before dropping GPU state and the app pin.
//!
//! Threading: every entry point must run on the host UI thread (the
//! thread that created the parent `HWND`). Non-test builds debug-assert
//! this; the registry releases its lock before entering any GPUI or host
//! callback so re-entry cannot deadlock.
//!
//! Touch mapping: native Win32 pointer/keyboard messages flow to the
//! child `WndProc` unchanged (touch-as-pointer via the OS); the key
//! tables are the backend's existing `keyboard.rs` maps.
#![allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "C ABI entries stay safe by contract: nulls rejected up front, every dereference documented"
)]

use std::cell::Cell;
use std::ffi::{c_char, c_void};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Once, OnceLock};

use gpui::{
    App, AppCell, AppContext, Context, Entity, IntoElement, Render, Styled, Window, WindowOptions,
    div,
};
use gpui_foreign_parent::{
    EditorRegistry, FrameThrottle, ResizeDebounce, TransitionCell, optional_c_string,
};
use gpui_windows::{
    ForeignParentInfo, ForeignWindow, WindowsPlatform, clear_pending_foreign,
    stash_pending_foreign, take_opened_foreign,
};

use crate::policy::{ForeignGeometry, allocate_editor_id, validate_geometry};

static INIT_LOGGER: Once = Once::new();

fn init_logger() {
    INIT_LOGGER.call_once(|| {
        let _ = env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Info)
            .try_init();
    });
}

/// Host UI thread, captured at the first attach.
static UI_THREAD: OnceLock<std::thread::ThreadId> = OnceLock::new();

#[cfg(not(test))]
fn assert_ui_thread() {
    if let Some(expected) = UI_THREAD.get() {
        debug_assert!(
            *expected == std::thread::current().id(),
            "Windows foreign-parent calls must stay on the host UI thread"
        );
    }
}

#[cfg(test)]
fn assert_ui_thread() {}

// ── Registry ─────────────────────────────────────────────────────────────────

/// Live editors by id. Keyed (not single-slot) so multi-editor hosts can
/// attach, destroy, and re-attach views independently.
static WIN_EDITORS: OnceLock<EditorRegistry<u64>> = OnceLock::new();

fn editors() -> &'static EditorRegistry<u64> {
    WIN_EDITORS.get_or_init(|| EditorRegistry::new(assert_ui_thread))
}

/// Run `f` against the editor registered under `editor_id`, if any.
/// The registry lock is released before `f` runs. Returns whether an
/// editor was found; late host callbacks after detach resolve to
/// "no instance" and no-op.
fn with_editor<F>(editor_id: u64, f: F) -> bool
where
    F: FnOnce(&WinEditor),
{
    editors().with(&editor_id, f).is_some()
}

/// Resolve a possibly-null context to its editor id.
fn editor_id_of(context: *mut WinContext) -> Option<u64> {
    if context.is_null() {
        return None;
    }
    // SAFETY: non-null contexts are live `WinContext`s by the host
    // contract (each destroyed at most once, never used after destroy);
    // only the `Copy` id is read out, so no aliasing is created.
    Some(unsafe { (*context).editor_id })
}

// ── Editor ───────────────────────────────────────────────────────────────────

/// One live foreign editor: the child window plus its host/guest policy.
struct WinEditor {
    foreign: ForeignWindow,
    throttle: FrameThrottle,
    debounce: ResizeDebounce<(i32, i32)>,
    /// Newest host geometry; tracks immediately while the surface
    /// reconfigure lags within the debounce window (contract §5).
    latest: Cell<ForeignGeometry>,
    active: TransitionCell,
    hovered: TransitionCell,
    dropped_frames: AtomicUsize,
}

impl WinEditor {
    fn new(foreign: ForeignWindow, geometry: ForeignGeometry) -> Self {
        Self {
            foreign,
            throttle: FrameThrottle::default(),
            debounce: ResizeDebounce::default(),
            latest: Cell::new(geometry),
            // A fresh child is neither active nor hovered, matching the
            // backend cells (`is_active` compares against the top-level
            // foreground window, which a child never is).
            active: TransitionCell::new(false),
            hovered: TransitionCell::new(false),
            dropped_frames: AtomicUsize::new(0),
        }
    }
}

/// Opaque context handle passed to/from the host.
pub struct WinContext {
    editor_id: u64,
    _label: String,
    editor: Box<WinEditor>,
    /// Pins the `AppCell` past the synchronous `run()` return: the
    /// foreign platform invokes the launch callback and returns, so
    /// without this pin the app state would deallocate at attach.
    app_cell: Rc<AppCell>,
}

/// Neutral placeholder root for the C entry point.
///
/// The C ABI cannot carry a Rust factory, so [`gpui_win_create`] renders
/// an empty root. Hosts that need real editor content inject it through
/// [`gpui_win_create_with`].
struct PlaceholderRootView;

impl Render for PlaceholderRootView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

// ── Lifecycle ────────────────────────────────────────────────────────────────

/// Create a GPUI context embedded in a host `HWND`, with injected content.
///
/// Rust-native constructor behind [`gpui_win_create`]: plugin crates call
/// this with their own root-view factory instead of going through the C
/// ABI. Returns null when the parent is null, the geometry is invalid, or
/// the window fails to open.
///
/// # Safety
///
/// `parent` must be null or point to a live host `HWND` (as
/// `*mut c_void`) that outlives the returned context. All calls must
/// happen on the host UI thread.
pub unsafe fn gpui_win_create_with<V: Render + 'static>(
    parent: *mut c_void,
    width: f32,
    height: f32,
    scale: f32,
    label: &str,
    build_root_view: impl FnOnce(&mut Window, &mut App) -> Entity<V> + 'static,
) -> *mut WinContext {
    init_logger();
    assert_ui_thread();

    if parent.is_null() {
        log::error!("GPUI Windows foreign: null parent HWND");
        return std::ptr::null_mut();
    }
    let Some(geometry) = validate_geometry(width, height, scale) else {
        log::error!("GPUI Windows foreign: invalid geometry {width}x{height} @{scale}");
        return std::ptr::null_mut();
    };
    let _ = UI_THREAD.set(std::thread::current().id());

    log::info!(
        "GPUI Windows foreign: create label={label} size={}x{} @{:.1}x parent={parent:p}",
        geometry.width,
        geometry.height,
        geometry.scale,
    );

    let editor_id = allocate_editor_id();
    stash_pending_foreign(ForeignParentInfo {
        parent: parent as isize,
        width: geometry.width,
        height: geometry.height,
        scale: geometry.scale,
    });

    let platform = match WindowsPlatform::new_foreign() {
        Ok(platform) => Rc::new(platform),
        Err(e) => {
            clear_pending_foreign();
            log::error!("GPUI Windows foreign: platform init failed: {e:#}");
            return std::ptr::null_mut();
        }
    };
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
                log::error!("GPUI Windows foreign: open_window failed: {e:#}");
            }
        }
    });

    if !window_opened.get() {
        clear_pending_foreign();
        log::error!("GPUI Windows foreign: returning null because open_window failed");
        return std::ptr::null_mut();
    }
    let Some(foreign) = take_opened_foreign() else {
        clear_pending_foreign();
        log::error!("GPUI Windows foreign: open_window did not yield a foreign window");
        return std::ptr::null_mut();
    };

    let editor = Box::new(WinEditor::new(foreign, geometry));
    // SAFETY: `editor` names the live `WinEditor` the new context owns;
    // `destroy` unregisters this id before the box drops, so the pointer
    // stays valid while registered. All registry use is UI-thread gated.
    unsafe {
        editors().register(editor_id, (&raw const *editor).cast::<()>());
    };
    log::info!("GPUI Windows foreign: editor {editor_id} ready ({label})");
    let context = Box::new(WinContext {
        editor_id,
        _label: label.to_string(),
        editor,
        app_cell,
    });
    Box::into_raw(context)
}

/// Detach order (contract §4): unregister the slot, destroy GPU
/// resources (the child `HWND`), then drop the `AppCell` pin last.
fn destroy_context(context: WinContext) {
    editors().unregister(&context.editor_id);
    let editor_id = context.editor_id;
    // Destructure to control the drop order: the pin is dropped
    // explicitly after the child is gone, and the label drops last.
    let WinContext {
        editor,
        app_cell: pin,
        ..
    } = context;
    // Destroy the child `HWND` synchronously, then drop the policy
    // state and the `AppCell` pin.
    let WinEditor { foreign, .. } = *editor;
    foreign.destroy();
    drop(pin);
    log::info!("GPUI Windows foreign: editor {editor_id} destroyed");
}

// ── C ABI ────────────────────────────────────────────────────────────────────

/// Create a GPUI context embedded in a host `HWND`.
///
/// `parent` must be a live host window (as `*mut c_void`) and `label`
/// must be a valid C string; nulls are rejected. Renders the neutral
/// placeholder root; Rust hosts inject real content through
/// [`gpui_win_create_with`]. Returns null on any failure.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_win_create(
    parent: *mut c_void,
    width: f32,
    height: f32,
    scale: f32,
    label: *const c_char,
) -> *mut WinContext {
    init_logger();

    if parent.is_null() || label.is_null() {
        log::error!("GPUI Windows foreign: null pointer argument to create");
        return std::ptr::null_mut();
    }
    // SAFETY: `label` is null-checked above and documented as a valid C
    // string; invalid UTF-8 is rejected without further dereference.
    let label_text = unsafe { optional_c_string(label) };
    let Some(label_text) = label_text else {
        log::error!("GPUI Windows foreign: invalid UTF-8 label");
        return std::ptr::null_mut();
    };
    // SAFETY: `parent` is null-checked above and documented as a live
    // host `HWND` outliving the context; hosts call on the UI thread.
    unsafe {
        gpui_win_create_with(parent, width, height, scale, &label_text, |_, cx| {
            cx.new(|_| PlaceholderRootView)
        })
    }
}

/// Destroy a GPUI Windows context. A null context is a no-op; each live
/// context must be destroyed at most once, on the host UI thread.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_win_destroy(context: *mut WinContext) {
    if context.is_null() {
        return;
    }
    assert_ui_thread();
    // SAFETY: `context` is non-null, came from `Box::into_raw` in create,
    // and the host destroys each context at most once.
    let context = unsafe { Box::from_raw(context) };
    destroy_context(*context);
}

/// Request one frame of GPUI rendering.
/// Call from a host timer/DWM tick on the UI thread (§10 pump).
#[unsafe(no_mangle)]
pub extern "C" fn gpui_win_request_frame(context: *mut WinContext) {
    let Some(editor_id) = editor_id_of(context) else {
        return;
    };
    assert_ui_thread();
    with_editor(editor_id, |editor| {
        // Converge a debounced resize before drawing (§6).
        editor.debounce.poll(|(width, height)| {
            editor
                .foreign
                .resize_foreign(width, height, editor.latest.get().scale);
            true
        });
        if !editor.throttle.should_forward() {
            return;
        }
        if !editor.foreign.request_frame() {
            editor.dropped_frames.fetch_add(1, Ordering::Relaxed);
        }
    });
}

/// Handle host resize: logical size plus backing scale.
///
/// Bounds track the newest host size immediately; only the surface
/// reconfigure (`SetWindowPos` → swapchain resize) is debounced (§5).
/// Non-finite input or a bad scale ignores the call.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_win_resize(context: *mut WinContext, width: f32, height: f32, scale: f32) {
    let Some(editor_id) = editor_id_of(context) else {
        return;
    };
    assert_ui_thread();
    let Some(geometry) = validate_geometry(width, height, scale) else {
        log::warn!("GPUI Windows foreign: ignoring resize with invalid geometry");
        return;
    };
    with_editor(editor_id, |editor| {
        editor.latest.set(geometry);
        editor
            .debounce
            .submit((geometry.width, geometry.height), |(w, h)| {
                editor.foreign.resize_foreign(w, h, geometry.scale);
                true
            });
    });
}

/// Report host focus changes for GPUI window activation state.
/// Fires the GPUI callback on transition only (§7).
#[unsafe(no_mangle)]
pub extern "C" fn gpui_win_set_active(context: *mut WinContext, is_active: bool) {
    let Some(editor_id) = editor_id_of(context) else {
        return;
    };
    assert_ui_thread();
    with_editor(editor_id, |editor| {
        if editor.active.get() != is_active {
            editor.active.update(is_active);
            editor.foreign.set_foreign_active(is_active);
        }
    });
}

/// Report whether the pointer is currently inside the child view.
/// Fires the GPUI callback on transition only (§7).
#[unsafe(no_mangle)]
pub extern "C" fn gpui_win_set_hovered(context: *mut WinContext, is_hovered: bool) {
    let Some(editor_id) = editor_id_of(context) else {
        return;
    };
    assert_ui_thread();
    with_editor(editor_id, |editor| {
        if editor.hovered.get() != is_hovered {
            editor.hovered.update(is_hovered);
            editor.foreign.set_foreign_hovered(is_hovered);
        }
    });
}

/// Realtime health counters: frames dropped on a gone child window and
/// host ticks coalesced by the frame throttle. Each out-pointer is
/// optional; a null context leaves all outputs untouched.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_win_frame_stats(
    context: *mut WinContext,
    dropped: *mut usize,
    coalesced: *mut usize,
) {
    if context.is_null() {
        return;
    }
    assert_ui_thread();
    let Some(editor_id) = editor_id_of(context) else {
        return;
    };
    let (dropped_frames, coalesced_frames) = editors()
        .with(&editor_id, |editor: &WinEditor| {
            (
                editor.dropped_frames.load(Ordering::Relaxed),
                editor.throttle.coalesced(),
            )
        })
        .unwrap_or((0, 0));
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

/// Query the GPUI child `HWND` (as `*mut c_void`) for host-side plumbing:
/// z-order, explicit show/hide, or diagnostics. Returns null for a null
/// context or a gone editor.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_win_child_hwnd(context: *mut WinContext) -> *mut c_void {
    let Some(editor_id) = editor_id_of(context) else {
        return std::ptr::null_mut();
    };
    assert_ui_thread();
    editors()
        .with(&editor_id, |editor: &WinEditor| {
            editor.foreign.child_hwnd() as *mut c_void
        })
        .unwrap_or(std::ptr::null_mut())
}
