# Unreleased

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
