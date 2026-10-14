//! Shared editor lifecycle for the Linux bindings (contract §§4, 8, 9).
//!
//! One [`LinuxForeignEditor`] per host editor: the platform handle, the
//! GPUI window id, the newest host bounds, and the shared host/guest
//! policies (frame throttle, resize debounce, transition-only focus and
//! hover, health counters). Instances live behind the keyed
//! [`EditorRegistry`](gpui_foreign_parent::EditorRegistry); the `AppCell`
//! pin lives in thread-local storage because `Rc` is `!Send`.
//!
//! Affinity: every entry runs on the host UI thread. The first call pins
//! the UI thread id; debug builds assert later calls match it.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread::{ThreadId, current};

use gpui::{AnyWindowHandle, AppCell, PlatformInput};
use gpui_foreign_parent::{EditorRegistry, FrameThrottle, ResizeDebounce, TransitionCell};
use gpui_linux::foreign_parent::ForeignParentPlatform;

use crate::geometry::ForeignBounds;

/// Which native parent backs an editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeignBackend {
    /// Host X11 window (VST3, CLAP-x11).
    X11,
    /// Host Wayland surface (CLAP-wayland).
    Wayland,
}

/// Opaque host handle for one editor; passed back verbatim.
pub struct LinuxForeignContext {
    editor_id: u64,
}

impl LinuxForeignContext {
    pub(crate) fn new(editor_id: u64) -> Self {
        Self { editor_id }
    }

    pub(crate) fn editor_id(&self) -> u64 {
        self.editor_id
    }
}

/// One live foreign editor. Main-thread use only; shared reads go through
/// the registry with the lock released before any callback runs.
pub(crate) struct LinuxForeignEditor {
    pub platform: Rc<ForeignParentPlatform>,
    pub window: AnyWindowHandle,
    pub backend: ForeignBackend,
    pub label: String,
    /// Newest host geometry; updated synchronously on every resize while
    /// the surface reconfigure is debounced.
    pub bounds: Cell<ForeignBounds>,
    pub throttle: FrameThrottle,
    pub debounce: ResizeDebounce<(u32, u32, f32)>,
    pub dropped_frames: AtomicUsize,
    pub active: TransitionCell,
    pub hovered: TransitionCell,
    // Host-forwarded click tracking (contract §7 click counts).
    pub last_click_ms: Cell<u64>,
    pub last_click_button: Cell<Option<gpui::MouseButton>>,
    pub last_click_at: Cell<(f32, f32)>,
    pub click_count: Cell<usize>,
}

fn assert_ui_thread() {
    static UI_THREAD: LazyLock<ThreadId> = LazyLock::new(|| current().id());
    debug_assert_eq!(
        current().id(),
        *UI_THREAD,
        "gpui-linux-foreign: cross-thread editor use; all calls must run on the host UI thread"
    );
}

static EDITORS: LazyLock<EditorRegistry<u64>> =
    LazyLock::new(|| EditorRegistry::new(assert_ui_thread));

static NEXT_EDITOR_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static APP_CELLS: RefCell<Option<HashMap<u64, Rc<AppCell>>>> = const { RefCell::new(None) };
}

/// Allocate a fresh editor id. Ids start at 1; 0 is always invalid.
pub(crate) fn alloc_editor_id() -> u64 {
    NEXT_EDITOR_ID.fetch_add(1, Ordering::Relaxed)
}

/// Register an editor state box under `id`.
///
/// # Safety
///
/// Takes ownership of `editor`; the registry holds the pointer until
/// [`unregister_editor`] removes the key. Re-registering an id without
/// unregistering first leaks the previous box.
pub(crate) unsafe fn register_editor(id: u64, editor: Box<LinuxForeignEditor>) {
    let ptr = Box::into_raw(editor).cast::<()>();
    // SAFETY: `ptr` names a live `LinuxForeignEditor` owned by the
    // registry until `unregister_editor`; UI-thread use is asserted.
    unsafe { EDITORS.register(id, ptr) };
}

/// Remove `id` from the registry and drop its state box, if present.
pub(crate) fn unregister_editor(id: u64) {
    let ptr = EDITORS.with(&id, |editor: &LinuxForeignEditor| {
        std::ptr::from_ref(editor).cast_mut()
    });
    EDITORS.unregister(&id);
    if let Some(ptr) = ptr {
        // SAFETY: `ptr` is the box leaked by `register_editor` for this
        // key, unregistered above so no further `with` can reach it.
        drop(unsafe { Box::from_raw(ptr) });
    }
}

/// Run `f` against the editor registered under `id`, if any.
pub(crate) fn with_editor<R>(id: u64, f: impl FnOnce(&LinuxForeignEditor) -> R) -> Option<R> {
    EDITORS.with(&id, f)
}

/// Backend and label of the editor registered under `id`, if any.
/// Used for destroy-path diagnostics.
pub(crate) fn describe_editor(id: u64) -> Option<(ForeignBackend, String)> {
    with_editor(id, |editor| (editor.backend, editor.label.clone()))
}

/// Pin the app state for `id` on this thread.
pub(crate) fn pin_app_cell(id: u64, cell: Rc<AppCell>) {
    APP_CELLS.with(|cells| {
        cells
            .borrow_mut()
            .get_or_insert_with(HashMap::new)
            .insert(id, cell);
    });
}

/// Release the app-state pin for `id`. False when no pin exists on this
/// thread (a cross-thread destroy, which leaks that editor's app state
/// rather than risking undefined behavior).
pub(crate) fn unpin_app_cell(id: u64) -> bool {
    APP_CELLS.with(|cells| {
        cells
            .borrow_mut()
            .as_mut()
            .and_then(|cells| cells.remove(&id))
            .is_some()
    })
}

/// Forward one normalized input to an editor. Unknown ids are ignored.
pub(crate) fn forward_input(id: u64, input: PlatformInput) {
    with_editor(id, |editor| {
        editor.platform.input_foreign(editor.window, input);
    });
}

/// Boot time in milliseconds, the clock for forwarded click counts.
pub(crate) fn boot_ms() -> u64 {
    static BOOT: std::sync::LazyLock<std::time::Instant> =
        std::sync::LazyLock::new(std::time::Instant::now);
    BOOT.elapsed().as_millis() as u64
}

/// Realtime health counters: frames dropped while windowless and host
/// ticks coalesced by the frame throttle. Unknown ids report zeros, and a
/// null context leaves outputs untouched (see `ffi`).
pub(crate) fn frame_stats(id: u64) -> (usize, usize) {
    with_editor(id, |editor| {
        (
            editor.dropped_frames.load(Ordering::Relaxed),
            editor.throttle.coalesced(),
        )
    })
    .unwrap_or((0, 0))
}
