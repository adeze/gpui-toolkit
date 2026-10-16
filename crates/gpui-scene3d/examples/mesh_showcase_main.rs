//! Mesh Showcase - Interactive 3D mesh and field visualisation.
//!
//! Four deterministic scenes over one analytic Taylor-Green vortex field:
//! vortex cores with streamtubes, an LIC slice, banded contours, and tensor
//! glyphs. Navigate sections with the sidebar; `?section=` selects one for
//! browser captures.
//!
//! Build/serve the wasm version via `just wasm-serve-mesh` (Trunk, port 8085).
#![cfg_attr(target_family = "wasm", no_main)]

#[path = "mesh_showcase/mesh_section.rs"]
mod mesh_section;
#[path = "mesh_showcase/scenes.rs"]
mod scenes;
#[path = "mesh_showcase/showcase_app.rs"]
mod showcase_app;

use gpui::AppContext;
use gpui_miniapp::{MiniApp, MiniAppConfig};
use showcase_app::ShowcaseApp;

fn run_app() {
    let config = MiniAppConfig::new("Mesh Showcase").size(1200.0, 800.0);
    MiniApp::run(config, |cx| cx.new(ShowcaseApp::new));
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    let mut args = std::env::args().skip(1);
    if let Some(arg) = args.next() {
        match arg.as_str() {
            "--visual-manifest" => {
                if args.any(|arg| arg == "--json") {
                    println!("{}", mesh_section::visual_manifest_json());
                } else {
                    println!("mesh visual capture manifest");
                    for section in mesh_section::MeshSection::all() {
                        println!("- {}", section.label());
                    }
                }
                return;
            }
            "--help" | "-h" => {
                println!("Usage: mesh-showcase [--visual-manifest [--json]]");
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
