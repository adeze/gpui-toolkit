# Foreign-Parent Editor Embedding — Generic Pattern

<!-- Status: PROPOSAL. Normative contract for new work; gpui-au is the
     conformance baseline (§12). Architecture changes here require updates
     to the bound backend docs (crates/gpui-au/AGENTS.md at minimum). -->

Date: 2026-10-14. Scope decision: generic cross-platform pattern (not a
per-host catalogue). Depth: implementer contract — lifecycle, surface,
frames, input, threading, FFI conventions — plus a platform binding
matrix (§10) and a migration plan (§14).

## 1. Problem

Several hosts want GPUI-rendered editors inside windows GPUI does not
own: a DAW hosting an AUv3 / VST3 / CLAP plugin view, a mobile shell
hosting a plugin panel, a browser page hosting a canvas. Today only one
such embedding exists — `gpui-au`, which renders into a host-owned macOS
`NSView` — and its mechanisms (view handoff, frame pump, resize
debounce, input forwarding, main-thread registry) are AU-specific code
that every future host would otherwise re-derive by copy-paste.

This document defines one platform-neutral **foreign-parent embedding
contract** so that macOS, iOS, Android, and wasm hosts share the same
lifecycle, threading, and failure semantics, with only a thin
per-platform binding for the native surface handle and event source.

Direction matters. This is **GPUI-inside-a-foreign-view** (guest
renderer, host chrome). The reverse — native views inside the GPUI tree
(Flutter-style platform views, `gpui-ios/src/platform_view.rs`,
`crates/gpui-android/src/platform_view.rs`) — already exists and is
explicitly out of scope.

## 2. Definitions

- **Host**: owns the native view, the event/run loop, and the editor
  lifecycle (open, resize, close). Examples: DAW, `AUViewController`,
  Android activity, browser page.
- **Guest**: the GPUI side. Renders scenes into a surface attached to
  the host view and consumes forwarded input. Never owns the loop.
- **Foreign parent**: the host-owned native view the guest draws into
  (`NSView`, `UIView`, Android `Surface` holder, HTML canvas).
- **Editor instance**: one guest context bound to one foreign parent,
  from attach to detach. Keyed by an opaque handle the host holds.
- **Binding**: the per-platform adapter (surface creation, event
  forwarding, pump source). All policy lives in the generic core; the
  binding only translates.

## 3. Principles

1. The host owns the loop. The guest never starts, stops, or nests a
   run loop; the platform `run()` invokes the launch callback
   synchronously and returns (`AuPlatform::run` is the model).
2. The guest never blocks the host thread. GPU contention drops the
   frame and counts it (§6); instantiation defers blocking GPU setup
   off the attach path (§5).
3. The host owns geometry. Size and scale flow host → guest only; the
   guest's `resize()` is a no-op and bounds always track the latest
   host values immediately, even while surface reconfiguration lags.
4. Every boundary crossing is total: null-safe entry points, validated
   strings/blobs, and healthy counters with zero ambiguity between
   "no data" and "zero".
5. One UI thread. Every guest call happens on the host main/UI thread;
   debug builds assert it, and shared slots are locked only to copy a
   pointer, never across callbacks.

## 4. Lifecycle contract

One editor instance moves through exactly these states; the binding
exposes one entry point per transition and each is idempotent and
null-safe:

| Transition | Host call | Guest duty |
|---|---|---|
| Attach | `create(parent, size, scale, label)` | Build platform + app, stash the parent handoff, open exactly one window, return an opaque context or null on failure |
| Frame | `request_frame(ctx)` | Forward one throttled frame request; coalesce overdrive (§6) |
| Resize | `resize(ctx, w, h, scale)` | Update bounds/layer/scale synchronously; debounce only the surface reconfigure (§5) |
| Focus | `set_active(ctx, bool)`, `set_hovered(ctx, bool)` | Notify GPUI only on transition (§7) |
| Input | `mouse_*/key_*/scroll/ime` (§7) | Normalize to `PlatformInput` and dispatch |
| Detach | `destroy(ctx)` | Unregister the instance, drop the context exactly once |

Rules:

- `create` failure returns null; success returns a context the host
  passes back verbatim. The context pins the `AppCell` (or platform
  equivalent) so a synchronous `run()` cannot deallocate the app —
  see `AuContext::_app_cell`.
- `destroy` on a null context is a no-op; double destroy must be
  impossible by construction (`Box::from_raw` exactly once, slot
  cleared first).
- Detach order: unregister the instance slot, then drop GPU resources,
  then drop the context. Late host callbacks after detach resolve to
  "no instance" and no-op.
- Re-attach (common in DAWs: view destroyed and re-created) goes
  through full attach again; the registry must support removal, not
  just first-write (`Mutex<Option<…>>`, not `OnceLock` — the
  `AU_WINDOW` rationale).

## 5. Surface contract

The host provides, at attach: a native parent handle, logical size,
and backing scale. The guest attaches its own layer/surface and owns
only that:

- Attach the cheap layer on the instantiation path (e.g.
  `setWantsLayer` + `CAMetalLayer` sized to the parent bounds); defer
  blocking device/surface creation to the first frame via stashed init
  args (`PendingGpuInit` pattern). Instantiation must return to the
  host quickly.
- If device creation fails, keep the placeholder layer and retry with
  the newest size on a later frame — never fail the attach for a GPU
  reason, and never spin.
- Resize path, in order: update bounds + scale cells, update the
  native layer frame/scale, refresh deferred init args if the renderer
  does not exist yet, debounce the surface reconfigure, then fire the
  GPUI resize callback. Bounds always converge immediately; the
  surface converges within the debounce window (§6 constants).
- Drawable size is `max(1, round(logical × scale))` per axis; a zero
  host size must never produce a zero drawable.
- The guest queries host appearance (light/dark) defensively at draw
  time with a static fallback; appearance is best-effort, never fatal.

## 6. Frame contract

Frames are host-pumped (display link, `CVDisplayLink`, choreographer,
`requestAnimationFrame`, or timer — see §10). The guest:

- Throttles forwarded frame requests: ticks arriving faster than the
  minimum interval are coalesced and counted, not queued. Reference
  constants from the baseline: frame-request minimum interval 4 ms,
  surface-reconfigure debounce 16 ms.
- Draws with non-blocking locks only (`try_lock` down the whole
  renderer chain). Any contention drops the frame and increments a
  `dropped_frames` counter; a missing renderer does the same. The
  realtime/host thread never waits on the GPU.
- Applies a stashed pending surface size before drawing so a debounced
  resize always converges on the next drawn frame.
- Exposes health counters (`dropped_frames`, `coalesced_frames`) via an
  optional-out-pointer stats call; a null context leaves outputs
  untouched. Counters are the guest's realtime health signal — hosts
  use them to distinguish "idle" from "starved".
- Uses VSync-paced presentation (`PresentMode::Fifo` equivalent) so
  frames stay paced by the host display link instead of free-running.

## 7. Input contract

The host forwards raw UI events; the guest normalizes them to GPUI
`PlatformInput` at the boundary. Coordinate space is logical pixels in
the guest bounds origin; the binding converts (flip Y where the native
origin differs) before crossing.

| Host event | Guest `PlatformInput` | Notes |
|---|---|---|
| Button press | `MouseDown` | Normalize button id → `MouseButton` (unknown ids fall back to Left, never rejected) |
| Button release | `MouseUp` | Same mapping; `click_count` is informational |
| Move without button | `MouseMove { pressed_button: None }` | Track position + modifiers in the window cells |
| Move with button | `MouseMove { pressed_button: Some(..) }` | Drags are moves with a button, not a separate variant |
| Wheel/trackpad scroll | `ScrollWheel` (pixel delta) | Report the host touch phase; default to `Moved` when the host has none |
| Touch begin/move/end | `MouseDown/Move/Up` or touch inputs | Per binding (§10); must document which mapping it uses |
| Key press | `KeyDown` | Named key from the hardware code table; else the unmodified character lowercased; else the stable `"unknown"` fallback |
| Key release | `KeyUp` | Same key resolution as press |
| Key repeat | `KeyDown { is_held: true }` | Repeats are held presses, not new presses |
| Text commit | IME `replace_text_in_range(None, text)` | UTF-8 validated at the boundary |
| Marked composition | IME `replace_and_mark_text_in_range` | Clamp selection offsets into the UTF-16 length |
| Composition end | IME `unmark_text` | — |
| Backspace (IME) | Delete one UTF-16 code point behind the caret | Surrogate-pair aware; selection delete takes the selection path |
| Focus in/out | Active-status callback, on transition only | Repeated identical values are swallowed, never re-fired |
| Pointer enter/leave | Hover-status callback, on transition only | Same transition-only rule |

Modifier normalization is a per-platform bit table documented in the
binding (model: `modifiers_from_ns_event` — control/alt/shift/platform/
function from `NSEvent` modifier flags). Unknown hardware key codes are
not useful binding identifiers: resolve to `"unknown"` and keep the
commit-char path, with no per-event diagnostic allocation on repeats.

## 8. Threading and reentrancy contract

- Affinity: every guest entry point and every registry access runs on
  the host main/UI thread. Non-test builds assert this
  (`AuWindowPtr::assert_main_thread` model: `NSThread.isMainThread`
  equivalent per platform); test builds compile the assert out.
- Lock discipline: the instance slot lock is held only to copy the raw
  pointer, then released before entering any GPUI or host callback, so
  re-entry cannot deadlock on the slot (`with_au_window` model).
- Callback storage: `RefCell<Option<Box<dyn FnMut…>>>` slots with
  take–invoke–restore: take the callback, invoke it outside the
  borrow, restore it only if the slot is still empty (a re-entrant
  `open_window`-style re-registration during the callback wins).
- Interior mutability split, by access pattern: `Cell` for
  main-thread-only state (bounds, scale, mouse, modifiers,
  focus/hover, debounce timestamps), atomics for cross-observation
  counters (dropped/coalesced frames), `Mutex` only for the renderer
  handle and deferred init args — and even those are touched via
  `try_lock` on the frame path.
- Raw native pointers cross into Rust as `Send + Sync` wrappers with
  documented `SAFETY` justifications (lifetime owned by the host view
  controller; dereferenced on the UI thread only). Small handle structs
  (`AuRawWindow` model) exist so GPU-surface creation bounds (`Send +
  Sync + Clone + 'static`) do not leak into the window type.

## 9. Lifetime and instance registry

The baseline keeps a single global slot because each AU appex is its
own process. The generic pattern generalizes this to a **keyed editor
registry** so multi-editor hosts (VST3/CLAP, multi-view DAWs) work:

- Registry maps an editor key (the opaque context address or an
  explicit id) to the instance pointer. Single-editor bindings may use
  a one-slot specialization, but must keep removal semantics.
- The handoff problem is generic: `open_window()` constructs the
  window without parameters for the parent, so the binding stashes a
  `PendingViewInfo`-style record (parent handle + size + scale) in
  thread-local storage during `create`, and the window constructor
  takes it. The take is one-shot; a constructor that finds no record
  builds a bounded placeholder (fixed default size, no renderer)
  rather than failing.
- The context object owns one strong reference that keeps the app cell
  alive past the synchronous `run()` return; `destroy` releases it.
- `Rc` (not `Arc`) for the renderer handle is permitted only where
  every use is gated on the UI thread; document the gate next to the
  alias.

## 10. Platform bindings

Normative core (§§4–9) is platform-independent. Each binding supplies
exactly: parent handle type, surface/layer attach, frame pump source,
input event source, main-thread assertion, modifier/key tables.

| Host | Parent handle | Surface | Pump | Input source | Status |
|---|---|---|---|---|---|
| macOS plugin view (AUv3; later VST3/CLAP) | `NSView *` | `CAMetalLayer` + wgpu Metal | `CVDisplayLink`/timer → `request_frame` | `NSView`/`NSEvent` tracking + `NSTextInputClient` | Baseline exists (`gpui-au`) |
| iOS plugin view (AUv3) | `UIView *` | `CAMetalLayer` + wgpu Metal | `CADisplayLink` → `request_frame` | Shared view classes (no per-event FFI): taps → deferred mouse down/up, drags → scroll + mouse move, two-finger → native `Pinch`; `UIKeyInput`/`UITextInput` as on the app path | Implemented (`gpui-ios`, M3) |
| Android embedded view | `Surface` holder (`ANativeWindow`) | wgpu Vulkan/GLES surface | `Choreographer` → `request_frame` | Per-pointer `MotionEvent` → native `Pointer` pipeline via JNI (no history batching); keys as down/up + unicode; IME via the shared queue; hover reports active (adapter parity); renderer init is synchronous on create like the app path, so hosts should create off the UI thread | Implemented (`gpui-android`, M5a; host sample `GpuiForeign.java`) |
| Browser canvas | `HtmlCanvasElement` | `wgpu` WebGPU canvas surface | `requestAnimationFrame` → `request_frame` | `key.js` code tables + pointer/keyboard/composition dispatch (pure-DOM `input.rs`, no `gpui_web` fork yet) | Implemented as host shell + renderer seam (`gpui-web-foreign`, M5b; headless until the §16 `gpui_web` fork patch lands). WebGPU-only with async device init and one-frame-behind readback (see AGENTS.md testing notes) — the binding must not assume blocking `device.poll` |
| Windows plugin view (VST3) | `HWND` | `WS_CHILD` + DirectX renderer | Host timer/DWM → `request_frame` | Host `GetMessage` loop dispatches to the child `WndProc` (no input FFI); focus/hover forwarded transition-only | Implemented (`gpui-win-foreign` + `gpui_windows` `WS_CHILD` path, M6) |
| Linux plugin view (VST3/CLAP) | X11 `Window` / Wayland `wl_surface` | X11 child window / `wl_subsurface` + wgpu surface | Host timer → `request_frame` | X11: child-window events via the shared X11 client; Wayland: host-pumped `frame` callbacks; host input-forwarding FFI for both | Implemented (`gpui-linux-foreign` + `gpui_linux` child/subsurface paths, M6; Wayland gaps: touch, key-repeat, text-input, cursor-shape, clipboard, output tracking) |

Each binding documents its touch mapping choice (touch-as-mouse vs
native touch inputs), its key-table source, and any host threading
deviations (e.g. Android UI thread vs `NativeActivity` looper).

## 11. What stays platform-specific (non-goals)

- Parameter/state bridges (`AuParameterTree`, `fullState` save/load):
  plugin-format business, not embedding. Lives in the binding crate.
- Text systems (CoreText vs platform shaper), clipboard/pasteboard,
  appearance sources, app-level services (menus, dialogs, prompts,
  URL opening, dock/taskbar): stubbed to "unavailable" in extension
  contexts, per binding.
- The demo root view (`AuRootView`, now in
  `crates/gpui-au/examples/demo_root_view.rs`): product sample code,
  not contract. Bindings must accept guest content injection rather
  than hard-coding a root view — the baseline injects through
  `gpui_au_create_with` while the C entry renders a neutral
  placeholder (M4 done).
- Accessibility bridges stay per-platform (`accessibility.rs` in each
  backend); the generic contract only requires that focus/hover
  transitions exist to drive them.

## 12. Reference implementation notes (gpui-au baseline)

File map for reviewers checking contract coverage:

- Lifecycle/FFI: `crates/gpui-au/src/ffi.rs` — `gpui_au_create`,
  `gpui_au_destroy`, `gpui_au_request_frame`, `gpui_au_resize`,
  `gpui_au_set_active/hovered`, mouse/key/IME entry points,
  parameter + state entry points, `AuContext` (app-cell pin).
- Window: `crates/gpui-au/src/window/au_window.rs` — lazy renderer
  init, debounced reconfigure, throttled frame forward, transition-
  only focus/hover, IME helpers, `PlatformWindow` impl (host-driven
  `resize` is a no-op; prompts/dialogs return "unavailable").
- Handoff: `crates/gpui-au/src/window.rs` (`PENDING_VIEW`
  thread-local) + `types.rs` (`PendingViewInfo`).
- Registry: `au_window_ptr.rs` (single-key specialization of the
  shared keyed registry with removal, main-thread assert compiled out
  under `cfg(test)`). Frame throttle, resize debounce, callback slots,
  and transition cells likewise come from the shared core (M2 done).
- Surface handles: `au_raw_window.rs` (small `Send + Sync` handle for
  `WgpuRenderer::new`, mirroring `gpui_linux`'s `RawWindow`).
- Platform: `src/platform.rs` (synchronous `run`, stubbed app-level
  services, `NSPasteboard` clipboard).

Known baseline gaps the migration must close (not contract changes):
G1. ~~single global slot instead of a keyed registry (§9)~~ — closed by
M2 (single-key specialization of the shared registry); G2. ~~root view
hard-coded in `ffi.rs` instead of injected content~~ — closed by M4
(`gpui_au_create_with` injection; demo moved to the example); G3. ~~stale
`PENDING_VIEW` description in `crates/gpui-au/AGENTS.md`~~ — fixed
(thread-local `RefCell`); G4. `close_callback` stays a local `FnOnce`
slot (the shared core is `FnMut`-only by design).

## 13. Conformance checklist

A binding claims conformance when all of these hold, each with a test
or a documented host-side reason it cannot be tested in-tree:

- [ ] Every entry point is null-safe (model:
  `exported_host_entry_points_are_null_safe` — null context leaves
  all out-pointers untouched, null strings rejected).
- [ ] Frame overdrive is coalesced and counted (model:
  `frame_request_overdrive_is_coalesced`).
- [ ] Rapid resize converges: bounds track the newest host size
  immediately, surface reconfigures within the debounce window
  (model: `rapid_resize_stashes_pending_size_until_debounce_elapses`).
- [ ] Focus/hover callbacks fire on transition only (model:
  `host_focus_and_hover_transitions_update_state_once`).
- [ ] Draw with no renderer drops and counts, never panics or blocks
  (model: `draw_without_renderer_drops_frame_without_panicking`).
- [ ] Registry supports destroy + re-attach (model: `Mutex`-slot
  removal; `with_au_window` returns `None` when unregistered).
- [ ] Main-thread affinity asserted on every entry path in non-test
  builds.
- [ ] Key/IME edge cases: `"unknown"` fallback, surrogate-pair
  backspace, clamped mark ranges (models:
  `mac_key_codes_and_modifiers_map_to_gpui_keystrokes`,
  `modified_characters_keep_the_unmodified_binding_key`,
  `backspace_moves_over_a_complete_utf16_code_point`).
- [ ] State transfer is two-phase (query size with null buffer, then
  fill) and versioned/magic-checked; corrupt payloads rejected.
- [ ] Host-side pump/forwarding sample exists (Swift/Kotlin/JS) showing
  attach → pump → resize → detach wiring.

## 14. Migration plan

- M1. ~~Extract a shared `foreign-parent` core (new module or crate):
  keyed editor registry, frame-throttle + resize-debounce policy
  types, take–invoke–restore callback slots, null-safe FFI
  conventions, two-phase state helpers.~~ Done: new
  `crates/gpui-foreign-parent` (`registry`, `throttle`, `debounce`,
  `slot`, `ffi`, plus `sync` `Sync` variants for Android); 27 lib
  tests, clippy clean.
- M2. ~~Rebase `gpui-au` onto the core.~~ Done: single-key
  registry, shared throttle/debounce/slots; 42 lib tests pass, no
  host-visible behavior change.
- M3. ~~Add the iOS AUv3 binding (`UIView` + `CADisplayLink`, touch and
  `UITextInput` mapping table) against the same core.~~ Done:
  `IosWindow::new_in_parent_view` + foreign FFI in `gpui-ios`;
  touch/keyboard route through the shared view classes. First
  multi-binding proof of the generic contract.
- M4. ~~Inject guest content at `create` (closure or factory) and
  demote `AuRootView` to an example.~~ Done:
  `gpui_au_create_with` takes the root-view factory; the C entry
  renders a neutral placeholder.
- M5a. ~~Android binding per §10.~~ Done: `android/foreign_parent.rs`
  (keyed editors, `Sync` core policies, JNI entries, surface re-attach)
  plus the `GpuiForeign.java` host sample.
- M5b. ~~wasm binding per §10 (page-owned canvas).~~ Done:
  new `crates/gpui-web-foreign` (pure-DOM `input.rs` tables +
  `host.rs` page-owned canvas shell with renderer seam); 9 host
  tests pass, wasm target check clean. Headless until the §16
  `gpui_web` fork patch lands.
- M6. ~~Windows/Linux bindings when a backend exists in-tree.~~ Done:
  fork patches (`gpui_windows` `WS_CHILD` path, `gpui_linux` X11-child +
  Wayland-subsurface paths) plus thin `gpui-win-foreign` /
  `gpui-linux-foreign` crates; 8 + 13 host tests pass, win/linux
  target checks clean. Runtime behavior awaits host validation
  (Gitea runners + real DAW pump).
- Ordering constraint: M1 before M3; M2 may land incrementally
  behind the existing FFI surface (no host-visible signature changes
  until M4).

## 15. Open questions

1. Keyed-registry key type: context address vs explicit host-provided
   editor id? (Address is zero-change for hosts; explicit ids survive
   context reallocation.)
2. Should the core own the modifier/key-table *shape* (a shared
   `KeyResolution` type) or leave key strings fully per-binding?
3. Touch-as-mouse vs native touch inputs for the iOS/Android
   bindings — decide per binding in its RFC, not here.
4. ~~wasm binding ownership: page-owned canvas vs GPUI-owned canvas?~~
   Resolved: page-owned, confirmed by the M5b `host.rs` shell design.
5. Realtime-audio proximity: the AU frame path already avoids
   blocking; do we need a documented upper bound on `draw()` work
   (e.g. scene-complexity budget) for low-latency hosts?
