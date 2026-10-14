//! Host-to-GPUI input normalization (contract §7).
//!
//! Pure mapping tables for host-forwarded input: X11 button details and
//! modifier state masks, Wayland evdev button codes, X11 keysym-to-key
//! resolution with the `"unknown"` fallback, and UTF-16 editing helpers.
//! They compile on every host so the tables are unit-tested without a
//! display.
//!
//! Touch mapping choice (§10): the guest seat reports pointer and keyboard
//! only; touch is not bound in M6 and hosts must not synthesize touch from
//! these tables. Key-table source: X11 keysyms below; the Wayland guest
//! seat resolves through the compositor keymap instead.

use gpui::{Capslock, Modifiers, MouseButton, NavigationDirection};

/// X11 state-mask bits (`xproto::KeyButMask` values, stable ABI).
const X11_SHIFT: u16 = 0x0001;
const X11_LOCK: u16 = 0x0002;
const X11_CONTROL: u16 = 0x0004;
const X11_MOD1_ALT: u16 = 0x0008;
const X11_MOD4_SUPER: u16 = 0x0040;

/// X11 button details to GPUI buttons.
///
/// Details 1/2/3 are left/middle/right, 8/9 navigate back/forward.
/// Details 4–7 are the scroll wheels: hosts must translate those to scroll
/// events, never to buttons — anything else falls back to Left (contract
/// §7: unknown ids are never rejected).
#[must_use]
pub fn x11_button_to_gpui(detail: u32) -> MouseButton {
    match detail {
        1 => MouseButton::Left,
        2 => MouseButton::Middle,
        3 => MouseButton::Right,
        8 => MouseButton::Navigate(NavigationDirection::Back),
        9 => MouseButton::Navigate(NavigationDirection::Forward),
        _ => MouseButton::Left,
    }
}

/// X11 modifier state mask to GPUI modifiers.
///
/// `Shift`, `Control`, `Mod1` (`Alt`), and `Mod4` (`Super` → platform) map
/// over; `Lock` (`CapsLock`), `NumLock`, button bits, and the vendor
/// `Mod3`/`Mod5` bits are not modifiers here (`CapsLock` has its own
/// [`x11_state_capslock`]).
#[must_use]
pub fn x11_state_to_modifiers(state: u16) -> Modifiers {
    Modifiers {
        shift: state & X11_SHIFT != 0,
        control: state & X11_CONTROL != 0,
        alt: state & X11_MOD1_ALT != 0,
        platform: state & X11_MOD4_SUPER != 0,
        function: false,
    }
}

/// X11 `Lock` bit to GPUI `CapsLock`.
#[must_use]
pub fn x11_state_capslock(state: u16) -> Capslock {
    Capslock {
        on: state & X11_LOCK != 0,
    }
}

/// Wayland evdev button codes (`linux/input-event-codes.h`) to GPUI buttons.
///
/// Unknown codes fall back to Left (contract §7), matching the guest seat.
#[must_use]
pub fn wayland_button_to_gpui(button: u32) -> MouseButton {
    const BTN_LEFT: u32 = 0x110;
    const BTN_RIGHT: u32 = 0x111;
    const BTN_MIDDLE: u32 = 0x112;
    const BTN_SIDE: u32 = 0x113;
    const BTN_EXTRA: u32 = 0x114;
    const BTN_FORWARD: u32 = 0x115;
    const BTN_BACK: u32 = 0x116;

    match button {
        BTN_LEFT => MouseButton::Left,
        BTN_RIGHT => MouseButton::Right,
        BTN_MIDDLE => MouseButton::Middle,
        BTN_BACK | BTN_SIDE => MouseButton::Navigate(NavigationDirection::Back),
        BTN_FORWARD | BTN_EXTRA => MouseButton::Navigate(NavigationDirection::Forward),
        _ => MouseButton::Left,
    }
}

/// Decode the C-ABI modifier bitpack: 0x01 shift, 0x02 control, 0x04
/// alt, 0x08 platform. Used by the Wayland forwarded-input entries, which
/// have no X11 state mask.
#[must_use]
pub fn modifiers_from_bits(bits: u8) -> Modifiers {
    Modifiers {
        shift: bits & 0x01 != 0,
        control: bits & 0x02 != 0,
        alt: bits & 0x04 != 0,
        platform: bits & 0x08 != 0,
        function: false,
    }
}

/// Fallback key name for unresolvable keys (contract §7).
pub const UNKNOWN_KEY: &str = "unknown";

/// Resolve an X11 keysym plus optional unmodified text to a GPUI key name.
///
/// Order (contract §7): the named table below, else the ASCII keysym
/// lowered, else the first text character lowered, else [`UNKNOWN_KEY`].
/// Names match the app path (`keystroke_from_xkb`): lowercase, `"enter"`
/// for Return, `"pageup"`/`"pagedown"`, `"space"` for U+0020.
#[must_use]
pub fn resolve_key_name(keysym: u32, text: Option<&str>) -> String {
    if let Some(name) = named_key_name(keysym) {
        return name.to_owned();
    }
    if let Some(ascii) = char::from_u32(keysym)
        && ascii.is_ascii_graphic()
    {
        return ascii.to_ascii_lowercase().to_string();
    }
    if keysym == 0x20 {
        return "space".to_owned();
    }
    if let Some(first) = text.and_then(|text| text.chars().next()) {
        return first.to_lowercase().to_string();
    }
    UNKNOWN_KEY.to_owned()
}

/// Named keysym table (X11 keysym values, stable ABI).
fn named_key_name(keysym: u32) -> Option<&'static str> {
    match keysym {
        0xff08 => Some("backspace"),
        0xff09 | 0xfe20 => Some("tab"),
        0xff0d | 0xff8d => Some("enter"),
        0xff1b => Some("escape"),
        0xff50 => Some("home"),
        0xff51 => Some("left"),
        0xff52 => Some("up"),
        0xff53 => Some("right"),
        0xff54 => Some("down"),
        0xff55 | 0xff9a => Some("pageup"),
        0xff56 | 0xff9b => Some("pagedown"),
        0xff57 => Some("end"),
        0xff63 => Some("insert"),
        0xffff => Some("delete"),
        0xffbe => Some("f1"),
        0xffbf => Some("f2"),
        0xffc0 => Some("f3"),
        0xffc1 => Some("f4"),
        0xffc2 => Some("f5"),
        0xffc3 => Some("f6"),
        0xffc4 => Some("f7"),
        0xffc5 => Some("f8"),
        0xffc6 => Some("f9"),
        0xffc7 => Some("f10"),
        0xffc8 => Some("f11"),
        0xffc9 => Some("f12"),
        0xffe1 | 0xffe2 => Some("shift"),
        0xffe3 | 0xffe4 => Some("control"),
        0xffe9 | 0xffea => Some("alt"),
        0xffeb | 0xffec => Some("super"),
        _ => None,
    }
}

/// Double-click window, shared with the platform backends.
pub const DOUBLE_CLICK_INTERVAL_MS: u64 = 400;
/// Double-click travel limit in logical pixels.
pub const DOUBLE_CLICK_DISTANCE_PX: f32 = 5.0;

/// Next click count for a press: increments when the same button repeats
/// inside the time window and travel limit, else restarts at 1.
#[must_use]
pub fn next_click_count(
    previous_count: usize,
    previous_button: Option<MouseButton>,
    button: MouseButton,
    elapsed_ms: u64,
    distance_px: f32,
) -> usize {
    if elapsed_ms < DOUBLE_CLICK_INTERVAL_MS
        && previous_button == Some(button)
        && distance_px <= DOUBLE_CLICK_DISTANCE_PX
    {
        previous_count.saturating_add(1).max(1)
    } else {
        1
    }
}

/// UTF-16 code units a backspace deletes: 2 after an astral character
/// (surrogate pair), 1 otherwise, 0 at the start of the buffer.
#[must_use]
pub fn utf16_backspace_len(text: &str) -> usize {
    match text.chars().next_back() {
        None => 0,
        Some(last) if last.len_utf16() == 2 => 2,
        Some(_) => 1,
    }
}

/// Clamp marked-composition offsets into the UTF-16 length.
#[must_use]
pub fn clamp_mark_range(start: usize, end: usize, len_utf16: usize) -> (usize, usize) {
    (start.min(len_utf16), end.min(len_utf16))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_buttons_map_with_left_fallback() {
        assert_eq!(x11_button_to_gpui(1), MouseButton::Left);
        assert_eq!(x11_button_to_gpui(2), MouseButton::Middle);
        assert_eq!(x11_button_to_gpui(3), MouseButton::Right);
        assert_eq!(
            x11_button_to_gpui(8),
            MouseButton::Navigate(NavigationDirection::Back)
        );
        assert_eq!(
            x11_button_to_gpui(9),
            MouseButton::Navigate(NavigationDirection::Forward)
        );
        // Wheels (4-7) and anything unknown fall back to Left, never rejected.
        assert_eq!(x11_button_to_gpui(4), MouseButton::Left);
        assert_eq!(x11_button_to_gpui(0), MouseButton::Left);
        assert_eq!(x11_button_to_gpui(99), MouseButton::Left);
    }

    #[test]
    fn x11_state_bits_map_to_modifiers_and_capslock() {
        let modifiers = x11_state_to_modifiers(0x0001 | 0x0004 | 0x0008 | 0x0040);
        assert!(modifiers.shift && modifiers.control && modifiers.alt && modifiers.platform);
        assert!(!modifiers.function);
        let plain = x11_state_to_modifiers(0);
        assert!(!plain.shift && !plain.control && !plain.alt && !plain.platform);
        // Lock and button bits are not modifiers.
        let locked = x11_state_to_modifiers(0x0002 | 0x0100);
        assert!(!locked.shift && !locked.control && !locked.alt && !locked.platform);
        assert!(x11_state_capslock(0x0002).on);
        assert!(!x11_state_capslock(0x0001).on);
    }

    #[test]
    fn modifier_bits_decode() {
        let modifiers = modifiers_from_bits(0x01 | 0x02 | 0x04 | 0x08);
        assert!(modifiers.shift && modifiers.control && modifiers.alt && modifiers.platform);
        let plain = modifiers_from_bits(0);
        assert!(!plain.shift && !plain.control && !plain.alt && !plain.platform);
        assert!(!modifiers_from_bits(0xf0).shift);
    }

    #[test]
    fn wayland_buttons_map_with_left_fallback() {
        assert_eq!(wayland_button_to_gpui(0x110), MouseButton::Left);
        assert_eq!(wayland_button_to_gpui(0x111), MouseButton::Right);
        assert_eq!(wayland_button_to_gpui(0x112), MouseButton::Middle);
        assert_eq!(
            wayland_button_to_gpui(0x116),
            MouseButton::Navigate(NavigationDirection::Back)
        );
        assert_eq!(
            wayland_button_to_gpui(0x115),
            MouseButton::Navigate(NavigationDirection::Forward)
        );
        assert_eq!(wayland_button_to_gpui(0x100), MouseButton::Left);
    }

    #[test]
    fn named_keysyms_match_the_app_path() {
        let no_text = None;
        assert_eq!(resolve_key_name(0xff0d, no_text), "enter");
        assert_eq!(resolve_key_name(0xff1b, no_text), "escape");
        assert_eq!(resolve_key_name(0xff09, no_text), "tab");
        assert_eq!(resolve_key_name(0xff08, no_text), "backspace");
        assert_eq!(resolve_key_name(0xffff, no_text), "delete");
        assert_eq!(resolve_key_name(0xff51, no_text), "left");
        assert_eq!(resolve_key_name(0xff55, no_text), "pageup");
        assert_eq!(resolve_key_name(0xffc1, no_text), "f4");
        assert_eq!(resolve_key_name(0xffc9, no_text), "f12");
    }

    #[test]
    fn ascii_and_text_fall_back_before_unknown() {
        assert_eq!(resolve_key_name(0x0041, None), "a");
        assert_eq!(resolve_key_name(0x0031, None), "1");
        assert_eq!(resolve_key_name(0x0020, None), "space");
        assert_eq!(resolve_key_name(0x0, Some("Q")), "q");
        assert_eq!(resolve_key_name(0x0, Some("")), "unknown");
        assert_eq!(resolve_key_name(0x0, None), "unknown");
        assert_eq!(resolve_key_name(0xffffff, None), "unknown");
    }

    #[test]
    fn click_counts_chain_inside_the_window() {
        assert_eq!(next_click_count(1, Some(MouseButton::Left), MouseButton::Left, 100, 2.0), 2);
        assert_eq!(next_click_count(2, Some(MouseButton::Left), MouseButton::Left, 100, 2.0), 3);
        // Different button, slow repeat, or travel restarts at 1.
        assert_eq!(next_click_count(3, Some(MouseButton::Left), MouseButton::Right, 100, 0.0), 1);
        assert_eq!(next_click_count(3, Some(MouseButton::Left), MouseButton::Left, 900, 0.0), 1);
        assert_eq!(next_click_count(3, Some(MouseButton::Left), MouseButton::Left, 100, 9.0), 1);
        assert_eq!(next_click_count(0, None, MouseButton::Left, 0, 0.0), 1);
    }

    #[test]
    fn backspace_moves_over_a_complete_utf16_code_point() {
        assert_eq!(utf16_backspace_len(""), 0);
        assert_eq!(utf16_backspace_len("a"), 1);
        assert_eq!(utf16_backspace_len("é"), 1);
        // U+1D11E needs a surrogate pair.
        assert_eq!(utf16_backspace_len("𝄞"), 2);
        assert_eq!(utf16_backspace_len("a𝄞"), 2);
    }

    #[test]
    fn mark_ranges_clamp_into_the_utf16_length() {
        assert_eq!(clamp_mark_range(1, 3, 8), (1, 3));
        assert_eq!(clamp_mark_range(0, 99, 8), (0, 8));
        assert_eq!(clamp_mark_range(99, 100, 8), (8, 8));
    }
}
