# gpui-dataset-transformation

Dataset-to-dataset transformations for the GPUI toolkit: pure filters that
consume a `gpui_mesh::DataSet` and produce a new `DataSet`.

Filters: plane `slice` (cut), `clip_halfspace`, scalar `threshold`,
marching-tetrahedra `isosurface`, derived-field calculators (vector and
complex magnitude/phase), `warp_by_vector` / `warp_by_scalar`, vector
`glyphs`, and RK2 `streamlines`. Visualisation geometry: `solid_arrows`
(closed 3D shaft+cone arrows), `tensor_glyphs` (ellipsoids from the
eigendecomposition), `streamtubes` / `stream_ribbons` (parallel-transport
extrusions), `dashed_lines` (animated marching-ants dashes), and
`contour_lines` (marching-triangles isolines). Flow diagnostics
(`vorticity`, `divergence`, `q_criterion`, `lambda2`,
`swirling_strength`) use per-tetrahedron gradients averaged to points;
vortex cores are `isosurface(lambda2, 0)`. `lic` appends a
line-integral-convolution streak field, and `particle_frames` advects
seeded particles through a `TimeSeries` with fading trails; the `gpu` feature runs the same diagnostics through wgpu compute
(Metal on Apple Silicon, Vulkan/CUDA-class adapters elsewhere), with a
prepared point-reduction kernel that is about 5x faster than the CPU at
110k points in local benchmarks. I/O covers legacy VTK text, VTU XML with
inline binary arrays, and XDMF with binary sidecars.

No GPUI, wgpu, file-I/O, threading, or clock dependency: every filter runs
identically on native targets and `wasm32-unknown-unknown` and is testable
headlessly. Filters never mutate their input and process large inputs cell
by cell.

```rust
use gpui_dataset_transformation::{slice, Plane};
use gpui_mesh::DataSet;

let room = DataSet::structured_grid("room", [6, 5, 4], [0.0; 3], [0.6, 0.56, 0.6]).unwrap();
let cut = slice(&room, &Plane::axis(2, 0.9).unwrap()).unwrap();
assert_eq!(cut.validate(), Ok(()));
```
