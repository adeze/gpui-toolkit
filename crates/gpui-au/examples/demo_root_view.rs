//! Demo editor content for foreign-parent embedding (macOS `AUv3`).
//!
//! Shows how a Rust host injects its own root view through
//! `gpui_au_create_with` instead of rendering the C entry point's neutral
//! placeholder. Real hosts pass a live `NSView`; a null view is rejected
//! with a null context instead of crashing.

#[cfg(target_os = "macos")]
use gpui_au::ffi::gpui_au_create_with;
#[cfg(target_os = "macos")]
use gpui_au::gpui::{
    AppContext, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Window, div, px, rgb,
};

/// Sample injected root view: plugin label plus a click counter.
#[cfg(target_os = "macos")]
struct AuRootView {
    plugin_label: SharedString,
    click_count: usize,
    click_label: SharedString,
}

#[cfg(target_os = "macos")]
impl AuRootView {
    fn new(plugin_type: impl AsRef<str>) -> Self {
        let plugin_label = SharedString::from(format!("SOTF: {}", plugin_type.as_ref()));
        Self {
            plugin_label,
            click_count: 0,
            click_label: SharedString::from("Clicks: 0"),
        }
    }

    fn refresh_click_label(&mut self) {
        self.click_label = SharedString::from(format!("Clicks: {}", self.click_count));
    }
}

#[cfg(target_os = "macos")]
impl Render for AuRootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x1a1a2e))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_color(rgb(0xffffff))
                    .text_xl()
                    .child(self.plugin_label.clone()),
            )
            .child(
                div()
                    .id(ElementId::Name("click-target".into()))
                    .mt(px(16.0))
                    .px(px(16.0))
                    .py(px(8.0))
                    .bg(rgb(0x3366ff))
                    .text_color(rgb(0xffffff))
                    .child(self.click_label.clone())
                    .on_click(cx.listener(|this, _event, _window, _cx| {
                        this.click_count += 1;
                        this.refresh_click_label();
                    })),
            )
    }
}

#[cfg(target_os = "macos")]
fn main() {
    // SAFETY: a null view is explicitly rejected without dereference; a
    // real host passes a live `NSView` on the main thread instead.
    let context = unsafe {
        gpui_au_create_with(std::ptr::null_mut(), 320.0, 200.0, 2.0, "demo", |_, cx| {
            cx.new(|_| AuRootView::new("demo"))
        })
    };
    assert!(context.is_null());
    println!("injection call shape OK; null view rejected as designed");
}

#[cfg(not(target_os = "macos"))]
fn main() {}
