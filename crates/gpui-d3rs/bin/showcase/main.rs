#![allow(
    clippy::too_many_arguments,
    reason = "showcase chart helpers mirror plotting formulas and visual geometry"
)]
#![cfg_attr(target_family = "wasm", no_main)]

//! d3rs Showcase - Unified demo application
//!
//! Demonstrates all d3rs functionality in a single application with tabbed navigation.
//!
//! Build/serve the wasm version via `just wasm-serve-d3rs` (Trunk, port 8083).

use gpui::prelude::*;
use gpui_miniapp::{MiniApp, MiniAppConfig};

mod showcase_modules;

#[path = "main/contour_render_mode.rs"]
mod contour_render_mode;
#[path = "main/demo_section.rs"]
mod demo_section;
#[path = "main/geo_projection_type.rs"]
mod geo_projection_type;
#[path = "main/showcase_app.rs"]
mod showcase_app;

pub use contour_render_mode::*;
pub use demo_section::*;
pub use geo_projection_type::*;
pub use showcase_app::*;

fn run_app() {
    let config = MiniAppConfig::new("d3rs Showcase")
        .size(1000.0, 800.0)
        .with_theme(true)
        .with_i18n(true);
    #[cfg(target_family = "wasm")]
    let config = config.initial_theme(gpui_miniapp::web_initial_theme());
    #[cfg(target_family = "wasm")]
    let config = config.initial_design(gpui_miniapp::web_initial_design_style());
    MiniApp::run(config, |cx| cx.new(ShowcaseApp::new));
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    let mut args = std::env::args().skip(1);
    if let Some(arg) = args.next() {
        match arg.as_str() {
            "--visual-manifest" => {
                if args.any(|arg| arg == "--json") {
                    println!("{}", demo_section::visual_manifest_json());
                } else {
                    println!("gpui-d3rs visual capture manifest");
                    for section in demo_section::DemoSection::all() {
                        println!("- {}", section.label());
                    }
                }
                return;
            }
            "--help" | "-h" => {
                println!("Usage: d3rs-showcase [--visual-manifest [--json]]");
                return;
            }
            _ => {}
        }
    }
    run_app();
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    gpui_miniapp::web_init();
    run_app();
}
