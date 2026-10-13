//! Full D3rs Spinorama viewer with live speaker measurements, polar and 3D views.
//!
//! Build/serve the wasm version via `just wasm-serve-spinorama` (Trunk, port 8084).
#![cfg_attr(target_family = "wasm", no_main)]

use gpui::*;
use gpui_miniapp::{MiniApp, MiniAppConfig};

mod api;
mod app;
mod render;
mod spinorama_section;
mod types;
mod utils;

use app::SpinoramaApp;

fn run_app() {
    let config = MiniAppConfig::new("Spinorama Showcase")
        .size(1200.0, 800.0)
        .with_theme(true)
        .scrollable(false);
    #[cfg(target_family = "wasm")]
    let config = config.initial_theme(gpui_miniapp::web_initial_theme());
    #[cfg(target_family = "wasm")]
    let config = config.initial_design(gpui_miniapp::web_initial_design_style());
    MiniApp::run(config, |cx| cx.new(SpinoramaApp::new));
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    let mut args = std::env::args().skip(1);
    if let Some(arg) = args.next() {
        match arg.as_str() {
            "--visual-manifest" => {
                if args.any(|arg| arg == "--json") {
                    println!("{}", spinorama_section::visual_manifest_json());
                } else {
                    println!("gpui-spinorama visual capture manifest");
                    for section in spinorama_section::SpinoramaSection::all() {
                        println!("- {}", section.label());
                    }
                }
                return;
            }
            "--help" | "-h" => {
                println!("Usage: spinorama-showcase [--visual-manifest [--json]]");
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
