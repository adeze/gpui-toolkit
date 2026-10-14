//! Callback storage with take–invoke–restore semantics (§8).
//!
//! Slots hold `FnMut` callbacks behind a `RefCell`. Emitting takes the
//! callback, invokes it outside the borrow, and restores it only if the
//! slot is still empty — so a re-entrant re-registration during the
//! callback wins instead of being overwritten.

use std::cell::RefCell;

/// Re-entrancy-safe callback slot; main-thread use only.
///
/// `R` is the callback's return type (`()` for notification-style
/// callbacks). Emitting takes the callback, invokes it outside the
/// borrow, and restores it only if the slot is still empty, so a
/// re-entrant `set` during the callback wins instead of being
/// overwritten.
#[derive(Default)]
pub struct CallbackSlot<A, R = ()> {
    inner: RefCell<SlotCallback<A, R>>,
}

/// Installed callback, if any.
type SlotCallback<A, R> = Option<Box<dyn FnMut(A) -> R>>;

impl<A, R> std::fmt::Debug for CallbackSlot<A, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallbackSlot")
            .field("is_set", &self.is_set())
            .finish()
    }
}

impl<A, R> CallbackSlot<A, R> {
    /// Create an empty slot.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(None),
        }
    }

    /// Install (or replace) the callback.
    pub fn set(&self, callback: impl FnMut(A) -> R + 'static) {
        *self.inner.borrow_mut() = Some(Box::new(callback));
    }

    /// True when a callback is installed.
    #[must_use]
    pub fn is_set(&self) -> bool {
        self.inner.borrow().is_some()
    }

    /// Take the callback out of the slot, if one is installed.
    fn take(&self) -> SlotCallback<A, R> {
        self.inner.borrow_mut().take()
    }

    /// Restore a taken callback, unless a re-entrant `set` already
    /// installed a replacement.
    fn restore(&self, callback: Box<dyn FnMut(A) -> R>) {
        let mut slot = self.inner.borrow_mut();
        if slot.is_none() {
            *slot = Some(callback);
        }
    }
}

impl<A> CallbackSlot<A, ()> {
    /// Invoke the callback, if one is installed. Re-entrant `set` calls
    /// during the callback are preserved.
    pub fn emit(&self, arg: A) {
        if let Some(mut callback) = self.take() {
            callback(arg);
            self.restore(callback);
        }
    }
}

impl<A, R> CallbackSlot<A, R> {
    /// Invoke the callback and return its value, or `None` when no
    /// callback is installed. Re-entrant `set` calls during the
    /// callback are preserved.
    pub fn emit_with(&self, arg: A) -> Option<R> {
        let mut callback = self.take()?;
        let result = callback(arg);
        self.restore(callback);
        Some(result)
    }
}

/// Boolean state that notifies only on transition (§7).
///
/// Models host focus (`set_active`) and pointer-in-view (`set_hovered`):
/// repeated identical values are swallowed, never re-fired.
#[derive(Debug)]
pub struct TransitionCell {
    current: std::cell::Cell<bool>,
    callback: CallbackSlot<bool>,
}

impl TransitionCell {
    /// Create a transition cell with an initial state.
    #[must_use]
    pub fn new(initial: bool) -> Self {
        Self {
            current: std::cell::Cell::new(initial),
            callback: CallbackSlot::new(),
        }
    }

    /// Install (or replace) the transition callback.
    pub fn on_change(&self, callback: impl FnMut(bool) + 'static) {
        self.callback.set(callback);
    }

    /// Update the state, notifying only when it actually changed.
    pub fn update(&self, value: bool) {
        if self.current.replace(value) == value {
            return;
        }
        self.callback.emit(value);
    }

    /// Current state.
    #[must_use]
    pub fn get(&self) -> bool {
        self.current.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn emit_restores_the_callback() {
        let slot = CallbackSlot::new();
        let calls = Rc::new(RefCell::new(0u32));
        let capture = Rc::clone(&calls);
        slot.set(move |()| *capture.borrow_mut() += 1);
        slot.emit(());
        slot.emit(());
        assert_eq!(*calls.borrow(), 2);
        assert!(slot.is_set());
    }

    #[test]
    fn emit_without_callback_is_a_noop() {
        let slot = CallbackSlot::<u32>::new();
        slot.emit(1);
        assert!(!slot.is_set());
    }

    #[test]
    fn emit_with_returns_the_callback_value() {
        let slot = CallbackSlot::new();
        slot.set(|value: u32| value * 2);
        assert_eq!(slot.emit_with(21), Some(42));
        // Still installed after emitting.
        assert_eq!(slot.emit_with(1), Some(2));
        let empty = CallbackSlot::<u32, u32>::new();
        assert_eq!(empty.emit_with(1), None);
    }

    #[test]
    fn reentrant_set_wins_over_restore() {
        let slot = Rc::new(CallbackSlot::new());
        let capture = Rc::clone(&slot);
        let second_calls = Rc::new(RefCell::new(0u32));
        let second_capture = Rc::clone(&second_calls);
        slot.set(move |(): ()| {
            let value = second_capture.clone();
            capture.set(move |(): ()| *value.borrow_mut() += 1);
        });
        slot.emit(());
        // The re-entrant registration survived; the first callback did
        // not overwrite it on restore.
        slot.emit(());
        assert_eq!(*second_calls.borrow(), 1);
    }

    #[test]
    fn transitions_notify_once_on_change_only() {
        let cell = TransitionCell::new(true);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let capture = Rc::clone(&seen);
        cell.on_change(move |value| capture.borrow_mut().push(value));
        cell.update(false);
        cell.update(false);
        cell.update(true);
        assert_eq!(&*seen.borrow(), &[false, true]);
        assert!(cell.get());
    }
}
