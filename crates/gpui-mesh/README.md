# gpui-mesh

Mesh data model for 2D and 3D visualization in the GPUI toolkit.

This crate owns exactly one job: representing datasets. It holds positions,
VTK-style cells (vertex, line, triangle, quad, tetrahedron, hexahedron,
wedge, pyramid), named point/cell fields (scalar, vector, symmetric tensor,
complex), structured-grid construction, boundary-surface extraction, and
bounded-size surface chunking for large datasets.

It has no GPUI, wgpu, file-I/O, threading, or time source dependency, so it
compiles and behaves identically on native targets and
`wasm32-unknown-unknown`. Positions are `f64` at every public boundary;
renderers rebase and convert to `f32` per chunk.

```rust
use gpui_mesh::{DataSet, Field};

let dataset = DataSet::structured_grid("room", [10, 8, 5], [0.0; 3], [0.4, 0.35, 0.5])
    .unwrap()
    .with_field(Field::scalar("pressure", vec![0.0; 400]).labelled("Pressure", "Pa"));
assert_eq!(dataset.validate(), Ok(()));
```
