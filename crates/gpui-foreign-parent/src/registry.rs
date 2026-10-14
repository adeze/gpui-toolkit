//! Keyed registry of live foreign-parent editor instances.
//!
//! Generalizes the single global window slot (contract §9): each editor
//! instance registers the pointer to its guest window under a key, and
//! host-pump entry points resolve the key back to the instance. Removal
//! (not just first-write) is required so hosts can destroy and re-create
//! views.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;

/// No-op UI-thread assertion for tests and single-threaded hosts.
pub fn no_thread_assertion() {}

/// Raw editor-instance pointer stored in the registry.
///
/// The pointee is the binding's guest window type, owned by the GPUI
/// platform layer and registered on the host UI thread.
#[derive(Debug, Clone, Copy)]
struct InstancePtr(*const ());

// SAFETY: instances are registered, resolved, and unregistered on the
// host UI thread only; every registry method runs the configured
// UI-thread assertion first, and `with` dereferences only the live
// registered pointer for the requested key.
unsafe impl Send for InstancePtr {}

// SAFETY: same as `Send` above; the `Mutex` only guards the map, the
// UI-thread assertion guards the pointee.
unsafe impl Sync for InstancePtr {}

/// Keyed slots for live editor instances.
///
/// `K` is the binding's editor key (context address, explicit editor
/// id, …). All methods assert the UI thread through the function given
/// to [`EditorRegistry::new`]; pass [`no_thread_assertion`] when the
/// host guarantees single-threaded use some other way.
#[derive(Debug)]
pub struct EditorRegistry<K> {
    slots: Mutex<HashMap<K, InstancePtr>>,
    assert_ui_thread: fn(),
}

impl<K> EditorRegistry<K> {
    /// Create an empty registry with the given UI-thread assertion.
    #[must_use]
    pub fn new(assert_ui_thread: fn()) -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            assert_ui_thread,
        }
    }
}

impl<K: Eq + Hash> EditorRegistry<K> {
    /// Register `instance` under `key`.
    ///
    /// Replaces any previous entry. Call on the host UI thread.
    ///
    /// # Safety
    ///
    /// `instance` must be non-null, point to the binding's guest window
    /// type `T` used with [`EditorRegistry::with`], and stay valid and
    /// unaliased for shared reads until [`EditorRegistry::unregister`]
    /// removes the key.
    ///
    /// # Panics
    ///
    /// Debug builds panic when `instance` is null.
    pub unsafe fn register(&self, key: K, instance: *const ()) {
        debug_assert!(!instance.is_null(), "registry instance must be non-null");
        (self.assert_ui_thread)();
        if let Ok(mut slots) = self.slots.lock() {
            slots.insert(key, InstancePtr(instance));
        }
    }

    /// Remove `key` from the registry, if present. Call on the host UI
    /// thread, before dropping the instance.
    pub fn unregister(&self, key: &K) {
        (self.assert_ui_thread)();
        if let Ok(mut slots) = self.slots.lock() {
            slots.remove(key);
        }
    }

    /// Run `f` against the instance registered under `key`.
    ///
    /// Returns `None` when no instance is registered (including after
    /// detach), when the slot lock is poisoned, or when the stored
    /// pointer is null. The lock is released before `f` runs so
    /// re-entrant register/unregister calls cannot deadlock.
    pub fn with<T, F, R>(&self, key: &K, f: F) -> Option<R>
    where
        F: FnOnce(&T) -> R,
    {
        (self.assert_ui_thread)();
        let ptr = self.slots.lock().ok()?.get(key)?.0.cast::<T>();
        if ptr.is_null() {
            return None;
        }
        // SAFETY: `ptr` is the non-null instance pointer stored by
        // `register`, which the caller guarantees names a live `T` until
        // `unregister`; UI-thread use is asserted above.
        let instance = unsafe { &*ptr };
        Some(f(instance))
    }

    /// Number of registered instances. For tests and diagnostics.
    #[must_use]
    pub fn len(&self) -> usize {
        (self.assert_ui_thread)();
        self.slots.lock().map_or(0, |slots| slots.len())
    }

    /// True when no instance is registered. For tests and diagnostics.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeWindow {
        value: u32,
    }

    fn registry() -> EditorRegistry<u64> {
        EditorRegistry::new(no_thread_assertion)
    }

    #[test]
    fn with_returns_none_when_unregistered() {
        let registry = registry();
        let result = registry.with(&7, |_: &FakeWindow| 42);
        assert!(result.is_none());
        assert!(registry.is_empty());
    }

    #[test]
    fn register_resolve_unregister_round_trip() {
        let registry = registry();
        let window = FakeWindow { value: 11 };
        let ptr = (&raw const window).cast::<()>();
        // SAFETY: `window` is a live `FakeWindow` outliving the registry
        // use below, unregistered before it drops.
        unsafe { registry.register(1, ptr) };
        assert_eq!(registry.len(), 1);
        let seen = registry.with(&1, |window: &FakeWindow| window.value);
        assert_eq!(seen, Some(11));
        registry.unregister(&1);
        assert!(registry.is_empty());
        let gone = registry.with(&1, |_: &FakeWindow| 0);
        assert!(gone.is_none());
    }

    #[test]
    fn destroy_and_reattach_replaces_the_slot() {
        let registry = registry();
        let first = FakeWindow { value: 1 };
        let second = FakeWindow { value: 2 };
        // SAFETY: both locals outlive the registry use below; each key is
        // unregistered before its pointee drops.
        unsafe { registry.register(1, (&raw const first).cast::<()>()) };
        registry.unregister(&1);
        // SAFETY: same as above for `second`.
        unsafe { registry.register(1, (&raw const second).cast::<()>()) };
        let seen = registry.with(&1, |window: &FakeWindow| window.value);
        assert_eq!(seen, Some(2));
    }

    #[test]
    fn keys_are_independent() {
        let registry = registry();
        let left = FakeWindow { value: 10 };
        let right = FakeWindow { value: 20 };
        // SAFETY: both locals outlive the registry use below.
        unsafe { registry.register(1, (&raw const left).cast::<()>()) };
        // SAFETY: same as above for `right`.
        unsafe { registry.register(2, (&raw const right).cast::<()>()) };
        let sum = registry.with(&1, |w: &FakeWindow| w.value).unwrap_or(0)
            + registry.with(&2, |w: &FakeWindow| w.value).unwrap_or(0);
        assert_eq!(sum, 30);
        registry.unregister(&1);
        assert!(registry.with(&1, |_: &FakeWindow| 0).is_none());
        assert!(registry.with(&2, |_: &FakeWindow| 0).is_some());
    }
}
