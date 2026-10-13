# Bundled assets

This directory is a shared asset bundle, not a Cargo crate. Its font paths are
load-bearing: the consumers below embed the files with `include_bytes!`.

- `../sotf-3rdparties/gpui/crates/gpui_web/src/platform.rs` embeds all IBM Plex Sans and
  Lilex variants for the browser platform.
- `../sotf-3rdparties/gpui/crates/gpui/src/svg_renderer.rs` embeds the Regular IBM Plex
  Sans and Lilex variants for SVG-renderer tests.

Keep the `fonts/` directory structure and filenames stable, or update every
consumer in the same change. Both font families are distributed under SIL Open
Font License 1.1; retain the accompanying license files when updating them.

`gpui-miniapp` also embeds DejaVu Sans for Unicode chess pieces and registers
the complete demo font set on native and web hosts. The headless visual harness
uses this same set. DejaVu's license is retained at `fonts/dejavu/LICENSE`.
