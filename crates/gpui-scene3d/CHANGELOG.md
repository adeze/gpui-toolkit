# Unreleased

# 0.10.44

## Added

- Initial release. Rendering for `gpui-mesh` datasets: orbit/pan/dolly
  camera with fitted projection, color transfer functions (named ramps,
  linear/log scaling, colour-blind-safe palettes), scenes with per-actor
  clip planes and opacity, and GPU preparation that rebases to the dataset
  centre so large real-world coordinates stay precise.
- Software rasterizer: per-fragment clip-plane testing, painter-sorted
  transparency, wireframe and points modes, double-sided lighting, and
  polylines rendered as shaded tubes. Maximum-intensity-projection volume
  raycasting with analytic ray/box clipping.
- WGSL surface, volume, and picking shaders ready for a wgpu host.
- Animation and movies: `Player` (explicit caller-supplied clock) and
  `png_sequence` at movie resolution; native-only `mp4` feature muxes the
  sequence to H.264 with a system `ffmpeg` and returns the bytes.
- Optional `gpui` feature: `SceneElement`, a ready-to-paint element for
  GPUI applications.
- Examples: `pcfan_airflow`, `speaker_port`, `necking_bar`, and the
  `interactive_viewer` (rotate/zoom/pan, live cutting plane), with
  1920x1080 / 120-frame movies under `docs/movies/`.
