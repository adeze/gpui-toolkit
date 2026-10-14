//! Pure DOM-input mapping tables (contract §7).
//!
//! Kept DOM-free so `cargo test -p gpui-web-foreign` pins every mapping
//! without a browser. Constants mirror `gpui_web`'s app-path handlers so a
//! page-owned editor behaves exactly like the full-page shell:
//! button order, modifier bits, the key table, wheel negation, and the
//! 400 ms / 5 px click window.

use gpui::{Modifiers, MouseButton, NavigationDirection, Pixels, Point, ScrollDelta, point, px};

/// Map a DOM `MouseEvent.button` to a GPUI button.
///
/// Mirrors `gpui_web`: 0 left, 1 middle, 2 right, 3–4 navigate
/// back/forward; unknown ids fall back to left, never rejected.
#[must_use]
pub fn dom_button_to_mouse(button: i16) -> MouseButton {
    match button {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        3 => MouseButton::Navigate(NavigationDirection::Back),
        4 => MouseButton::Navigate(NavigationDirection::Forward),
        _ => MouseButton::Left,
    }
}

/// DOM modifier flag bits, as read from mouse, pointer, wheel, or
/// keyboard events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModifierFlags {
    /// `Control` / `ctrlKey`.
    pub control: bool,
    /// `Alt` / `altKey`.
    pub alt: bool,
    /// `Shift` / `shiftKey`.
    pub shift: bool,
    /// `Meta` / `metaKey` (the platform modifier).
    pub platform: bool,
}

/// Build GPUI modifiers from DOM flag bits. `function` is always false.
#[must_use]
pub fn modifiers_from_flags(flags: ModifierFlags) -> Modifiers {
    Modifiers {
        control: flags.control,
        alt: flags.alt,
        shift: flags.shift,
        platform: flags.platform,
        function: false,
    }
}

/// Map a DOM `KeyboardEvent.key` to a GPUI key name.
///
/// Mirrors `gpui_web`: named keys normalize to lowercase GPUI names,
/// `F1`–`F35` become `f<n>`, and anything else lowercases through.
#[must_use]
pub fn dom_key_to_gpui_key(key: &str) -> String {
    match key {
        "Enter" => "enter".to_string(),
        "Backspace" => "backspace".to_string(),
        "Tab" => "tab".to_string(),
        "Escape" => "escape".to_string(),
        "Delete" => "delete".to_string(),
        " " => "space".to_string(),
        "ArrowLeft" => "left".to_string(),
        "ArrowRight" => "right".to_string(),
        "ArrowUp" => "up".to_string(),
        "ArrowDown" => "down".to_string(),
        "Home" => "home".to_string(),
        "End" => "end".to_string(),
        "PageUp" => "pageup".to_string(),
        "PageDown" => "pagedown".to_string(),
        "Insert" => "insert".to_string(),
        "Control" => "control".to_string(),
        "Alt" => "alt".to_string(),
        "Shift" => "shift".to_string(),
        "Meta" => "platform".to_string(),
        "CapsLock" => "capslock".to_string(),
        other => {
            if let Some(rest) = other.strip_prefix('F')
                && let Ok(number) = rest.parse::<u8>()
                && (1..=35).contains(&number)
            {
                return format!("f{number}");
            }
            other.to_lowercase()
        }
    }
}

/// True for modifier-only GPUI key names, which produce no key event.
#[must_use]
pub fn is_modifier_only_key(key: &str) -> bool {
    matches!(
        key,
        "control" | "alt" | "shift" | "platform" | "capslock" | "compose" | "process"
    )
}

/// Compute the `key_char` for a keystroke, mirroring `gpui_web`.
///
/// Modifier-held and modifier-only presses carry no character; `space`
/// maps to `" "`, and single-character keys pass through.
#[must_use]
pub fn compute_key_char(raw_key: &str, gpui_key: &str, modifiers: &Modifiers) -> Option<String> {
    if modifiers.platform || modifiers.control {
        return None;
    }
    if is_modifier_only_key(gpui_key) {
        return None;
    }
    if gpui_key == "space" {
        return Some(" ".to_string());
    }
    if raw_key.len() == 1 {
        return Some(raw_key.to_string());
    }
    None
}

/// Map DOM wheel deltas to a GPUI scroll delta.
///
/// Mirrors `gpui_web`: line mode (`deltaMode == 1`) yields line deltas,
/// everything else yields pixel deltas; both negate, matching the
/// platform scroll convention.
#[must_use]
pub fn scroll_delta(delta_x: f64, delta_y: f64, delta_mode: u32) -> ScrollDelta {
    if delta_mode == 1 {
        ScrollDelta::Lines(point(-delta_x as f32, -delta_y as f32))
    } else {
        ScrollDelta::Pixels(point(px(-delta_x as f32), px(-delta_y as f32)))
    }
}

/// Multi-click tracker: clicks within 400 ms and 5 px chain.
#[derive(Debug)]
pub struct ClickState {
    last_position: Point<Pixels>,
    last_time: f64,
    current_count: usize,
}

impl Default for ClickState {
    fn default() -> Self {
        Self {
            last_position: Point::default(),
            last_time: 0.0,
            current_count: 0,
        }
    }
}

impl ClickState {
    /// Register a press, returning its click count (1-based).
    pub fn register_click(&mut self, position: Point<Pixels>, time: f64) -> usize {
        let distance = ((f32::from(position.x) - f32::from(self.last_position.x)).powi(2)
            + (f32::from(position.y) - f32::from(self.last_position.y)).powi(2))
        .sqrt();

        if (time - self.last_time) < 400.0 && distance < 5.0 {
            self.current_count += 1;
        } else {
            self.current_count = 1;
        }

        self.last_position = position;
        self.last_time = time;
        self.current_count
    }

    /// Count of the in-progress click chain (0 before any press).
    #[must_use]
    pub fn current_count(&self) -> usize {
        self.current_count
    }
}

/// IME composition operation from the hidden input element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionOp {
    /// An in-progress composition updated its marked text.
    SetMarked(String),
    /// The composition committed its text.
    Commit(String),
}

/// Canvas-id validation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasIdError {
    /// The id is empty after trimming.
    Empty,
}

impl std::fmt::Display for CanvasIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "canvas id is empty"),
        }
    }
}

impl std::error::Error for CanvasIdError {}

/// Validate a page canvas id: trim it and reject empties.
pub fn validate_canvas_id(id: &str) -> Result<&str, CanvasIdError> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(CanvasIdError::Empty);
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_follow_dom_order_with_left_fallback() {
        assert_eq!(dom_button_to_mouse(0), MouseButton::Left);
        assert_eq!(dom_button_to_mouse(1), MouseButton::Middle);
        assert_eq!(dom_button_to_mouse(2), MouseButton::Right);
        assert_eq!(
            dom_button_to_mouse(3),
            MouseButton::Navigate(NavigationDirection::Back)
        );
        assert_eq!(
            dom_button_to_mouse(4),
            MouseButton::Navigate(NavigationDirection::Forward)
        );
        assert_eq!(dom_button_to_mouse(5), MouseButton::Left);
        assert_eq!(dom_button_to_mouse(-1), MouseButton::Left);
    }

    #[test]
    fn modifier_flags_map_directly() {
        let modifiers = modifiers_from_flags(ModifierFlags {
            control: true,
            alt: false,
            shift: true,
            platform: false,
        });
        assert!(modifiers.control);
        assert!(!modifiers.alt);
        assert!(modifiers.shift);
        assert!(!modifiers.platform);
        assert!(!modifiers.function);
    }

    #[test]
    fn named_keys_normalize() {
        assert_eq!(dom_key_to_gpui_key("Enter"), "enter");
        assert_eq!(dom_key_to_gpui_key("Backspace"), "backspace");
        assert_eq!(dom_key_to_gpui_key("Tab"), "tab");
        assert_eq!(dom_key_to_gpui_key("Escape"), "escape");
        assert_eq!(dom_key_to_gpui_key("Delete"), "delete");
        assert_eq!(dom_key_to_gpui_key(" "), "space");
        assert_eq!(dom_key_to_gpui_key("ArrowLeft"), "left");
        assert_eq!(dom_key_to_gpui_key("ArrowRight"), "right");
        assert_eq!(dom_key_to_gpui_key("ArrowUp"), "up");
        assert_eq!(dom_key_to_gpui_key("ArrowDown"), "down");
        assert_eq!(dom_key_to_gpui_key("Home"), "home");
        assert_eq!(dom_key_to_gpui_key("End"), "end");
        assert_eq!(dom_key_to_gpui_key("PageUp"), "pageup");
        assert_eq!(dom_key_to_gpui_key("PageDown"), "pagedown");
        assert_eq!(dom_key_to_gpui_key("Insert"), "insert");
        assert_eq!(dom_key_to_gpui_key("Meta"), "platform");
        assert_eq!(dom_key_to_gpui_key("CapsLock"), "capslock");
    }

    #[test]
    fn function_and_character_keys_fall_through() {
        assert_eq!(dom_key_to_gpui_key("F1"), "f1");
        assert_eq!(dom_key_to_gpui_key("F12"), "f12");
        assert_eq!(dom_key_to_gpui_key("F35"), "f35");
        assert_eq!(dom_key_to_gpui_key("F36"), "f36");
        assert_eq!(dom_key_to_gpui_key("A"), "a");
        assert_eq!(dom_key_to_gpui_key("a"), "a");
        assert_eq!(dom_key_to_gpui_key("1"), "1");
        assert_eq!(dom_key_to_gpui_key("Dead"), "dead");
    }

    #[test]
    fn modifier_only_keys_are_recognized() {
        for key in [
            "control", "alt", "shift", "platform", "capslock", "compose", "process",
        ] {
            assert!(is_modifier_only_key(key), "{key} should be modifier-only");
        }
        assert!(!is_modifier_only_key("a"));
        assert!(!is_modifier_only_key("enter"));
    }

    #[test]
    fn key_char_follows_gpui_web_rules() {
        let plain = modifiers_from_flags(ModifierFlags::default());
        let held = modifiers_from_flags(ModifierFlags {
            control: true,
            ..ModifierFlags::default()
        });
        assert_eq!(compute_key_char("a", "a", &plain).as_deref(), Some("a"));
        assert_eq!(compute_key_char("A", "a", &plain).as_deref(), Some("A"));
        assert_eq!(compute_key_char(" ", "space", &plain).as_deref(), Some(" "));
        assert_eq!(compute_key_char("Enter", "enter", &plain), None);
        assert_eq!(compute_key_char("Control", "control", &plain), None);
        assert_eq!(compute_key_char("a", "a", &held), None);
    }

    #[test]
    fn wheel_modes_negate_into_lines_or_pixels() {
        match scroll_delta(0.0, 3.0, 1) {
            ScrollDelta::Lines(delta) => {
                assert_eq!((delta.x, delta.y), (0.0, -3.0));
            }
            ScrollDelta::Pixels(_) => panic!("line mode must yield line deltas"),
        }
        match scroll_delta(1.5, -2.5, 0) {
            ScrollDelta::Pixels(delta) => {
                assert_eq!(delta, point(px(-1.5), px(2.5)));
            }
            ScrollDelta::Lines(_) => panic!("pixel mode must yield pixel deltas"),
        }
        match scroll_delta(0.0, 1.0, 2) {
            ScrollDelta::Pixels(delta) => {
                assert_eq!(delta, point(px(0.0), px(-1.0)));
            }
            ScrollDelta::Lines(_) => panic!("page mode must yield pixel deltas"),
        }
    }

    #[test]
    fn clicks_chain_within_time_and_distance() {
        let mut clicks = ClickState::default();
        assert_eq!(clicks.current_count(), 0);
        let at = point(px(10.0), px(10.0));
        assert_eq!(clicks.register_click(at, 1000.0), 1);
        assert_eq!(clicks.register_click(at, 1200.0), 2);
        // Too far away: new chain.
        assert_eq!(
            clicks.register_click(point(px(100.0), px(100.0)), 1300.0),
            1
        );
        // Too late: new chain.
        assert_eq!(
            clicks.register_click(point(px(100.0), px(100.0)), 2000.0),
            1
        );
    }

    #[test]
    fn canvas_ids_must_be_non_empty() {
        assert_eq!(validate_canvas_id("gpui-editor"), Ok("gpui-editor"));
        assert_eq!(validate_canvas_id("  gpui-editor  "), Ok("gpui-editor"));
        assert_eq!(validate_canvas_id(""), Err(CanvasIdError::Empty));
        assert_eq!(validate_canvas_id("   "), Err(CanvasIdError::Empty));
    }
}
