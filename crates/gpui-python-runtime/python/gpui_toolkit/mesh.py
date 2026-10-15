"""VTK-class mesh filters over structured grids (gpui-mesh /
gpui-dataset-transformation), exposed through the native extension.

Build a dataset spec with :func:`structured`, then run a chain of filter
ops with :func:`run`. Every op mirrors one Rust filter: ``slice``,
``clip``, ``threshold``, ``isosurface``, ``vorticity``,
``vorticity_magnitude``, ``divergence``, ``q_criterion`` and
``magnitude``. :func:`streamlines` integrates seeded RK2 streamlines in
both directions. Without a built wheel, :func:`problem` reports that the
native extension is missing and the run helpers raise ``RuntimeError``.
"""

from __future__ import annotations

import json
from typing import Any, Sequence

try:
    from ._native import (
        mesh_filters as _mesh_filters,
        mesh_problem as _mesh_problem,
        mesh_streamlines as _mesh_streamlines,
    )
except ImportError:  # Source tree without a built wheel.
    _mesh_filters = None  # type: ignore[assignment]
    _mesh_problem = None  # type: ignore[assignment]
    _mesh_streamlines = None  # type: ignore[assignment]


def _native(name: str):
    fn = {"mesh_filters": _mesh_filters, "mesh_problem": _mesh_problem,
          "mesh_streamlines": _mesh_streamlines}[name]
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
) -> dict[str, Any]:
    """Dataset spec for a structured hexahedral grid with named fields."""
    fields: list[dict[str, Any]] = []
    for name, values in (scalars or {}).items():
        fields.append({"name": name, "kind": "scalar",
                       "values": [float(v) for v in values]})
    for name, values in (vectors or {}).items():
        fields.append({"name": name, "kind": "vector",
                       "vectors": [[float(c) for c in v] for v in values]})
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


__all__ = ["clip", "problem", "run", "slice_", "streamlines", "structured"]
