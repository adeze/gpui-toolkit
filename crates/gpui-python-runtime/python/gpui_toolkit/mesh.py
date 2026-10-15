"""VTK-class mesh filters over structured grids (gpui-mesh /
gpui-dataset-transformation), exposed through the native extension.

Build a dataset spec with :func:`structured`, then run a chain of filter
ops with :func:`run`. Every op mirrors one Rust filter: ``slice``,
``clip``, ``threshold``, ``isosurface``, ``vorticity``,
``vorticity_magnitude``, ``divergence``, ``q_criterion``, ``lambda2``,
``swirling_strength`` and ``magnitude``. Geometry-producing ops
(``solid_arrows``, ``tensor_glyphs``, ``streamtubes``, ``stream_ribbons``,
``dashed_lines``, ``contour_lines``, ``lic``) run through
:func:`geometry`, which returns points, cells and fields;
:func:`particles` advects seeds through time snapshots, and
:func:`streamlines` integrates seeded RK2 streamlines in both
directions. File exchange mirrors the Rust I/O family:
:func:`read_mesh` / :func:`write_mesh` handle VTU (optionally
zlib-compressed), OBJ, PLY and STL files, and :func:`read_pvd` loads a
PVD time series with parallel parts merged. Without a built wheel,
:func:`problem` reports that the native extension is missing and the
run helpers raise ``RuntimeError``.
"""

from __future__ import annotations

import json
from typing import Any, Sequence

try:
    from ._native import (
        mesh_filters as _mesh_filters,
        mesh_ftle as _mesh_ftle,
        mesh_geometry as _mesh_geometry,
        mesh_particles as _mesh_particles,
        mesh_read as _mesh_read,
        mesh_read_pvd as _mesh_read_pvd,
        mesh_statistics as _mesh_statistics,
        mesh_problem as _mesh_problem,
        mesh_streamlines as _mesh_streamlines,
        mesh_write as _mesh_write,
    )
except ImportError:  # Source tree without a built wheel.
    _mesh_filters = None  # type: ignore[assignment]
    _mesh_ftle = None  # type: ignore[assignment]
    _mesh_geometry = None  # type: ignore[assignment]
    _mesh_particles = None  # type: ignore[assignment]
    _mesh_read = None  # type: ignore[assignment]
    _mesh_read_pvd = None  # type: ignore[assignment]
    _mesh_statistics = None  # type: ignore[assignment]
    _mesh_problem = None  # type: ignore[assignment]
    _mesh_streamlines = None  # type: ignore[assignment]
    _mesh_write = None  # type: ignore[assignment]


def _native(name: str):
    fn = {"mesh_filters": _mesh_filters, "mesh_ftle": _mesh_ftle,
          "mesh_geometry": _mesh_geometry, "mesh_particles": _mesh_particles,
          "mesh_problem": _mesh_problem, "mesh_read": _mesh_read,
          "mesh_read_pvd": _mesh_read_pvd,
          "mesh_statistics": _mesh_statistics,
          "mesh_streamlines": _mesh_streamlines,
          "mesh_write": _mesh_write}[name]
    if fn is None:
        raise RuntimeError("gpui_toolkit native extension is not installed")
    return fn


def structured(
    dims: Sequence[int],
    *,
    origin: Sequence[float] = (0.0, 0.0, 0.0),
    spacing: Sequence[float] = (1.0, 1.0, 1.0),
    id: str = "mesh",
    scalars: dict[str, Sequence[float]] | None = None,
    vectors: dict[str, Sequence[Sequence[float]]] | None = None,
    tensors: dict[str, Sequence[Sequence[float]]] | None = None,
) -> dict[str, Any]:
    """Dataset spec for a structured hexahedral grid with named fields."""
    fields: list[dict[str, Any]] = []
    for name, values in (scalars or {}).items():
        fields.append({"name": name, "kind": "scalar",
                       "values": [float(v) for v in values]})
    for name, values in (vectors or {}).items():
        fields.append({"name": name, "kind": "vector",
                       "vectors": [[float(c) for c in v] for v in values]})
    for name, values in (tensors or {}).items():
        fields.append({"name": name, "kind": "tensor",
                       "tensors": [[float(c) for c in v] for v in values]})
    return {
        "id": id,
        "structured": {
            "dims": [int(d) for d in dims],
            "origin": [float(c) for c in origin],
            "spacing": [float(c) for c in spacing],
        },
        "fields": fields,
    }


def slice_(*, normal: Sequence[float],
           point: Sequence[float] = (0.0, 0.0, 0.0)) -> dict[str, Any]:
    """Op builder: plane slice through ``point`` with ``normal``."""
    return {"op": "slice", "normal": list(normal), "point": list(point)}


def clip(*, normal: Sequence[float],
         point: Sequence[float] = (0.0, 0.0, 0.0),
         keep: str = "positive") -> dict[str, Any]:
    """Op builder: keep the ``positive``/``negative`` half-space."""
    return {"op": "clip", "normal": list(normal), "point": list(point),
            "keep": keep}


def arrows(*, field: str = "velocity", scale: float = 1.0, stride: int = 1,
           radius_fraction: float = 0.05) -> dict[str, Any]:
    """Op builder: solid 3D arrow glyphs (geometry via :func:`geometry`)."""
    return {"op": "solid_arrows", "field": field, "scale": scale,
            "stride": stride, "radius_fraction": radius_fraction}


def tensor_glyphs(*, field: str, scale: float = 1.0,
                  stride: int = 1) -> dict[str, Any]:
    """Op builder: ellipsoid glyphs for a symmetric-tensor field."""
    return {"op": "tensor_glyphs", "field": field, "scale": scale,
            "stride": stride}


def tubes(*, radius: float, sides: int = 6) -> dict[str, Any]:
    """Op builder: streamtubes around the current line dataset."""
    return {"op": "streamtubes", "radius": radius, "sides": sides}


def ribbons(*, width: float) -> dict[str, Any]:
    """Op builder: flat stream ribbons around the current line dataset."""
    return {"op": "stream_ribbons", "width": width}


def dashes(*, phase: float = 0.0, dash: float, gap: float) -> dict[str, Any]:
    """Op builder: dashed lines; animate `phase` for marching ants."""
    return {"op": "dashed_lines", "phase": phase, "dash": dash, "gap": gap}


def contours(*, field: str, levels: Sequence[float]) -> dict[str, Any]:
    """Op builder: contour lines of a scalar field at `levels`."""
    return {"op": "contour_lines", "field": field,
            "levels": [float(v) for v in levels]}


def lic(*, field: str = "velocity", kernel_steps: int = 10, step: float,
        seed: int = 0) -> dict[str, Any]:
    """Op builder: append the LIC streak field (summary via :func:`run`)."""
    return {"op": "lic", "field": field, "kernel_steps": kernel_steps,
            "step": step, "seed": seed}


def streamline_op(seeds: Sequence[Sequence[float]], *,
                  field: str = "velocity", step: float = 0.05,
                  max_steps: int = 500) -> dict[str, Any]:
    """Op builder: streamlines as a chain step before tubes/dashes."""
    return {"op": "streamlines", "field": field,
            "seeds": [[float(c) for c in seed] for seed in seeds],
            "step": step, "max_steps": max_steps}


def geometry(dataset: dict[str, Any],
             ops: Sequence[dict[str, Any]]) -> dict[str, Any]:
    """Run ops natively; returns points, cells, triangles, lines, fields."""
    request = json.dumps({"dataset": dataset, "ops": list(ops)})
    return json.loads(_native("mesh_geometry")(request))


def particles(
    series: Sequence[dict[str, Any]],
    times: Sequence[float],
    seeds: Sequence[Sequence[float]],
    *,
    velocity_field: str = "velocity",
    dt: float = 1.0 / 24.0,
    frames: int = 120,
    trail: int = 24,
) -> list[dict[str, Any]]:
    """Advect particles through dataset snapshots; one geometry per frame."""
    request = json.dumps({
        "series": list(series),
        "times": [float(t) for t in times],
        "velocity_field": velocity_field,
        "seeds": [[float(c) for c in seed] for seed in seeds],
        "dt": float(dt),
        "frames": int(frames),
        "trail": int(trail),
    })
    return json.loads(_native("mesh_particles")(request))["frames"]


def statistics(dataset: dict[str, Any], *, field: str,
               association: str = "point",
               bins: int | None = None) -> dict[str, Any]:
    """Field statistics (count/min/max/mean/std/median, optional histogram)."""
    request: dict[str, Any] = {"dataset": dataset, "field": field,
                               "association": association}
    if bins is not None:
        request["bins"] = bins
    return json.loads(_native("mesh_statistics")(json.dumps(request)))


def ftle(
    series: Sequence[dict[str, Any]],
    times: Sequence[float],
    *,
    velocity_field: str = "velocity",
    t0: float = 0.0,
    duration: float = 1.0,
    epsilon: float = 0.05,
    steps: int = 16,
) -> dict[str, Any]:
    """FTLE summary of a time series (ridges = coherent structures)."""
    request = json.dumps({
        "series": list(series),
        "times": [float(t) for t in times],
        "velocity_field": velocity_field,
        "t0": float(t0),
        "duration": float(duration),
        "epsilon": float(epsilon),
        "steps": int(steps),
    })
    return json.loads(_native("mesh_ftle")(request))


def run(dataset: dict[str, Any], ops: Sequence[dict[str, Any]]) -> dict[str, Any]:
    """Run filter ops natively; returns the summary of the final dataset."""
    request = json.dumps({"dataset": dataset, "ops": list(ops)})
    return json.loads(_native("mesh_filters")(request))


def problem(dataset: dict[str, Any]) -> str:
    """Empty string when the dataset spec is valid, else the problem."""
    if _mesh_problem is None:
        return "gpui_toolkit native extension is not installed"
    return _mesh_problem(json.dumps({"dataset": dataset}))


def streamlines(
    dataset: dict[str, Any],
    seeds: Sequence[Sequence[float]],
    *,
    velocity_field: str = "velocity",
    step: float = 0.05,
    max_steps: int = 500,
) -> list[list[list[float]]]:
    """Seeded streamlines as polylines (both directions per seed)."""
    request = json.dumps({
        "dataset": dataset,
        "velocity_field": velocity_field,
        "seeds": [[float(c) for c in seed] for seed in seeds],
        "step": float(step),
        "max_steps": int(max_steps),
    })
    return json.loads(_native("mesh_streamlines")(request))["lines"]


_FORMAT_BY_SUFFIX = {".vtu": "vtu", ".obj": "obj", ".ply": "ply",
                     ".stl": "stl"}


def _format_for(path: str) -> str:
    import os
    suffix = os.path.splitext(path)[1].lower()
    if suffix not in _FORMAT_BY_SUFFIX:
        raise ValueError(
            f"unsupported mesh file suffix '{suffix}' "
            f"(expected one of {sorted(_FORMAT_BY_SUFFIX)})")
    return _FORMAT_BY_SUFFIX[suffix]


def read_mesh(path: str) -> dict[str, Any]:
    """Read a mesh file (.vtu/.obj/.ply/.stl); returns geometry JSON.

    The result has the same shape as :func:`geometry`: points, typed
    cells, triangle/line index lists, and fields.
    """
    import base64
    with open(path, "rb") as handle:
        data = base64.b64encode(handle.read()).decode("ascii")
    request = json.dumps({"format": _format_for(path), "data_b64": data})
    return json.loads(_native("mesh_read")(request))


def write_mesh(dataset: dict[str, Any], path: str, *,
               compress: bool = False) -> None:
    """Write a dataset spec to `path` (.vtu/.obj/.ply/.stl).

    ``compress=True`` selects the zlib-compressed VTU variant and only
    applies to ``.vtu`` paths.
    """
    import base64
    fmt = _format_for(path)
    if compress:
        if fmt != "vtu":
            raise ValueError("compress=True only applies to .vtu files")
        fmt = "vtu-zlib"
    request = json.dumps({"dataset": dataset, "format": fmt})
    reply = json.loads(_native("mesh_write")(request))
    with open(path, "wb") as handle:
        handle.write(base64.b64decode(reply["data_b64"]))


def read_pvd(path: str) -> dict[str, Any]:
    """Read a PVD collection next to `path`; returns times + step geometry.

    Parallel parts of one timestep are merged; steps share the series
    topology, matching the native ``TimeSeries`` contract.
    """
    import base64
    import os
    import re
    with open(path, encoding="utf-8") as handle:
        xml = handle.read()
    base = os.path.dirname(os.path.abspath(path))
    files: dict[str, str] = {}
    for name in re.findall(r'file="([^"]+)"', xml):
        with open(os.path.join(base, name), "rb") as handle:
            files[name] = base64.b64encode(handle.read()).decode("ascii")
    request = json.dumps({"pvd": xml, "files": files})
    return json.loads(_native("mesh_read_pvd")(request))


__all__ = [
    "arrows", "clip", "contours", "dashes", "ftle", "geometry", "lic",
    "particles", "problem", "read_mesh", "read_pvd", "ribbons", "run",
    "slice_", "statistics", "streamline_op", "streamlines", "structured",
    "tensor_glyphs", "tubes", "write_mesh",
]
