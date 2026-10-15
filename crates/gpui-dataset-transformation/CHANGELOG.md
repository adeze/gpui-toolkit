# Unreleased

## Added

- `solid_arrows`: closed 3D shaft+cone arrow glyphs coloured by a
  `magnitude` field; `tensor_glyphs`: ellipsoid glyphs from the
  eigendecomposition of a symmetric-tensor field.
- `particle_frames`: time-dependent particle advection (pathlines) with
  fading trails through a `TimeSeries`.
- `streamtubes` and `stream_ribbons` (parallel-transport frames),
  `dashed_lines` for animated marching-ants flow direction.
- `contour_lines` marching-triangles isolines with a per-vertex `value`
  field.
- `lic`: deterministic line-integral-convolution streak field.
- `lambda2` and `swirling_strength` vortex criteria (analytic against
  solid-body rotation and pure strain); vortex cores extract as
  `isosurface(lambda2, 0)`.

# 0.10.44

## Added

- Initial release. Pure `DataSet -> DataSet` filters with point-field
  interpolation: plane `slice` and `clip_halfspace`, scalar `threshold`,
  marching-tetrahedra `isosurface`, derived-field calculators (vector and
  complex magnitude/phase), `warp_by_vector` / `warp_by_scalar`, vector
  `glyphs`, RK2 `streamlines`, and barycentric `probe`.
- Flow diagnostics from per-tetrahedron gradients: `vorticity`,
  `divergence`, `q_criterion`, and `vorticity_magnitude`, verified against
  analytic solid-body rotation and pure strain.
- I/O: legacy VTK text, VTU XML with inline binary arrays, and XDMF with
  binary sidecars (host-resolved, no filesystem assumed).
- Optional `gpu` feature: wgpu compute diagnostics (Metal, Vulkan,
  Direct3D, WebGPU). A retained `PreparedFlow` point-reduction kernel
  uploads geometry once and streams only velocities per frame; about
  5x faster than the CPU at 110k points in local Metal benchmarks,
  matching the CPU reference to `f32` gradient precision.
