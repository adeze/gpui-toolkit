//! JSON bridge from the Python wrapper to `gpui-mesh` and
//! `gpui-dataset-transformation`.
//!
//! The Python facade (`gpui_toolkit.mesh`) builds a plain JSON spec: a
//! structured grid plus named scalar/vector fields, followed by a chain of
//! filter ops. The functions here parse that spec, run the real Rust
//! filters, and return a JSON summary (or streamline polylines). Errors are
//! returned as messages; an unknown op is a hard error, never a silent
//! no-op.

// Rust guideline compliant 2026-02-21

use gpui_dataset_transformation::{
    Plane, ThresholdMode, clip_halfspace, divergence, isosurface, lambda2, q_criterion, slice,
    streamlines, swirling_strength, threshold, vector_magnitude, vorticity, vorticity_magnitude,
};
use gpui_mesh::{Association, DataSet, Field};
use serde::Deserialize;
use serde_json::{Value, json};

/// Parses one dataset spec (`structured` grid + `fields`) into a [`DataSet`].
///
/// # Errors
///
/// Returns a message when the JSON is malformed, the grid dimensions are
/// invalid, a field length does not match the point count, or a field kind
/// is unknown.
pub fn dataset_from_spec(spec: &Value) -> Result<DataSet, String> {
    let grid: GridSpec =
        serde_json::from_value(spec.get("structured").cloned().unwrap_or(Value::Null))
            .map_err(|e| format!("invalid 'structured' grid: {e}"))?;
    let mut dataset = DataSet::structured_grid(
        spec.get("id").and_then(Value::as_str).unwrap_or("mesh"),
        grid.dims,
        grid.origin,
        grid.spacing,
    )
    .map_err(|e| e.to_string())?;
    let points = dataset.point_count();
    if let Some(fields) = spec.get("fields").and_then(Value::as_array) {
        for field_spec in fields {
            let field: FieldSpec =
                serde_json::from_value(field_spec.clone()).map_err(|e| e.to_string())?;
            let count = match field.kind.as_str() {
                "scalar" => field.values.len(),
                "vector" => field.vectors.as_ref().map_or(0, Vec::len),
                other => return Err(format!("unknown field kind '{other}'")),
            };
            if count != points {
                return Err(format!(
                    "field '{}' has {count} entries but the grid has {points} points",
                    field.name
                ));
            }
            let built = match field.kind.as_str() {
                "scalar" => Field::scalar(field.name.clone(), field.values),
                "vector" => Field::vector(field.name.clone(), field.vectors.unwrap_or_default()),
                _ => unreachable!(),
            };
            dataset = dataset.with_field(built);
        }
    }
    dataset.validate().map_err(|e| e.to_string())?;
    Ok(dataset)
}

/// Applies one filter op, returning the transformed dataset.
///
/// # Errors
///
/// Returns a message for unknown ops, missing fields, or filter failures.
pub fn apply_op(dataset: &DataSet, op: &Value) -> Result<DataSet, String> {
    let name = op.get("op").and_then(Value::as_str).unwrap_or("");
    let err = |e: gpui_dataset_transformation::TransformError| e.to_string();
    let plane = |op: &Value| -> Result<Plane, String> {
        let normal = vec3(op, "normal")?;
        let point = vec3(op, "point").unwrap_or([0.0; 3]);
        Plane::new(point, normal).map_err(|e| e.to_string())
    };
    match name {
        "slice" => slice(dataset, &plane(op)?).map_err(err),
        "clip" => {
            let keep = op.get("keep").and_then(Value::as_str).unwrap_or("positive");
            let keep_positive = match keep {
                "positive" => true,
                "negative" => false,
                other => return Err(format!("unknown clip keep mode '{other}'")),
            };
            clip_halfspace(dataset, &plane(op)?, keep_positive).map_err(err)
        }
        "threshold" => {
            let mode = match op.get("mode").and_then(Value::as_str).unwrap_or("all") {
                "all" => ThresholdMode::All,
                "any" => ThresholdMode::Any,
                other => return Err(format!("unknown threshold mode '{other}'")),
            };
            threshold(
                dataset,
                str_field(op, "field")?,
                Association::Point,
                f64_field(op, "min")?,
                f64_field(op, "max")?,
                mode,
            )
            .map_err(err)
        }
        "isosurface" => {
            isosurface(dataset, str_field(op, "field")?, f64_field(op, "value")?).map_err(err)
        }
        "vorticity" => vorticity(dataset, str_field(op, "field")?, "vorticity").map_err(err),
        "vorticity_magnitude" => {
            vorticity_magnitude(dataset, str_field(op, "field")?, "vorticity_magnitude")
                .map_err(err)
        }
        "divergence" => divergence(dataset, str_field(op, "field")?, "divergence").map_err(err),
        "q_criterion" => q_criterion(dataset, str_field(op, "field")?, "q_criterion").map_err(err),
        "lambda2" => lambda2(dataset, str_field(op, "field")?, "lambda2").map_err(err),
        "swirling_strength" => {
            swirling_strength(dataset, str_field(op, "field")?, "swirling_strength").map_err(err)
        }
        "magnitude" => {
            let out = op
                .get("output")
                .and_then(Value::as_str)
                .unwrap_or("magnitude");
            vector_magnitude(dataset, str_field(op, "field")?, out).map_err(err)
        }
        other => Err(format!("unknown mesh op '{other}'")),
    }
}

/// Runs the full `{"dataset": ..., "ops": [...]}` request and returns a JSON
/// summary of the final dataset.
///
/// # Errors
///
/// Returns a message for malformed JSON or any failing op.
pub fn mesh_filters(request_json: &str) -> Result<String, String> {
    let request: Value = serde_json::from_str(request_json).map_err(|e| e.to_string())?;
    let mut dataset = dataset_from_spec(request.get("dataset").unwrap_or(&Value::Null))?;
    if let Some(ops) = request.get("ops").and_then(Value::as_array) {
        for op in ops {
            dataset = apply_op(&dataset, op)?;
        }
    }
    Ok(serde_json::to_string(&summarize(&dataset)).unwrap_or_default())
}

/// Validates a dataset spec without running filters. Returns an empty
/// string when valid, else the problem description.
#[must_use]
pub fn mesh_problem(spec_json: &str) -> String {
    let parsed: Result<Value, _> = serde_json::from_str(spec_json);
    match parsed {
        Ok(value) => match dataset_from_spec(value.get("dataset").unwrap_or(&value)) {
            Ok(_) => String::new(),
            Err(message) => message,
        },
        Err(e) => e.to_string(),
    }
}

/// Integrates streamlines and returns `{"lines": [[[x, y, z], ...], ...]}`.
///
/// # Errors
///
/// Returns a message for malformed JSON or integration failures.
pub fn mesh_streamlines(request_json: &str) -> Result<String, String> {
    let request: Value = serde_json::from_str(request_json).map_err(|e| e.to_string())?;
    let dataset = dataset_from_spec(request.get("dataset").unwrap_or(&Value::Null))?;
    let field = request
        .get("velocity_field")
        .and_then(Value::as_str)
        .unwrap_or("velocity");
    let seeds: Vec<[f64; 3]> =
        serde_json::from_value(request.get("seeds").cloned().unwrap_or(Value::Null))
            .map_err(|e| format!("invalid 'seeds': {e}"))?;
    let step = request.get("step").and_then(Value::as_f64).unwrap_or(0.05);
    let max_steps = request
        .get("max_steps")
        .and_then(Value::as_u64)
        .unwrap_or(500) as usize;
    let lines = streamlines(&dataset, field, &seeds, step, max_steps).map_err(|e| e.to_string())?;
    // The filter emits one line cell per integrated segment; stitch cells
    // that share an endpoint back into one polyline per seed.
    let mut polylines: Vec<Vec<[f64; 3]>> = Vec::new();
    for cell in 0..lines.cell_count() {
        let nodes = lines.cells.cell(cell).unwrap_or(&[]);
        let segment: Vec<[f64; 3]> = nodes.iter().map(|&n| lines.positions[n as usize]).collect();
        match polylines.last_mut() {
            Some(current)
                if !current.is_empty()
                    && !segment.is_empty()
                    && current[current.len() - 1] == segment[0] =>
            {
                current.extend_from_slice(&segment[1..]);
            }
            _ => polylines.push(segment),
        }
    }
    Ok(serde_json::to_string(&json!({ "lines": polylines })).unwrap_or_default())
}

fn summarize(dataset: &DataSet) -> Value {
    let fields: Vec<Value> = dataset
        .point_data
        .iter()
        .chain(dataset.cell_data.iter())
        .map(|field| {
            let (kind, min, max) = match &field.data {
                gpui_mesh::FieldData::Scalar(values) => (
                    "scalar",
                    values.iter().copied().fold(f64::INFINITY, f64::min),
                    values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                ),
                gpui_mesh::FieldData::Vector(values) => {
                    let magnitudes: Vec<f64> = values
                        .iter()
                        .map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt())
                        .collect();
                    (
                        "vector",
                        magnitudes.iter().copied().fold(f64::INFINITY, f64::min),
                        magnitudes.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    )
                }
                gpui_mesh::FieldData::Tensor(_) => ("tensor", 0.0, 0.0),
                gpui_mesh::FieldData::Complex(_) => ("complex", 0.0, 0.0),
            };
            json!({
                "name": field.name.as_ref(),
                "association": if field.association == Association::Point { "point" } else { "cell" },
                "kind": kind,
                "min": min,
                "max": max,
            })
        })
        .collect();
    let bounds = dataset.bounds();
    json!({
        "points": dataset.point_count(),
        "cells": dataset.cell_count(),
        "bounds": bounds.map(|b| [b.min, b.max]),
        "fields": fields,
    })
}

#[derive(Deserialize)]
struct GridSpec {
    dims: [usize; 3],
    #[serde(default)]
    origin: [f64; 3],
    #[serde(default = "unit_spacing")]
    spacing: [f64; 3],
}

fn unit_spacing() -> [f64; 3] {
    [1.0, 1.0, 1.0]
}

#[derive(Deserialize)]
struct FieldSpec {
    name: String,
    kind: String,
    #[serde(default)]
    values: Vec<f64>,
    vectors: Option<Vec<[f64; 3]>>,
}

fn vec3(op: &Value, key: &str) -> Result<[f64; 3], String> {
    serde_json::from_value(op.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|e| format!("invalid '{key}': {e}"))
}

fn str_field<'a>(op: &'a Value, key: &str) -> Result<&'a str, String> {
    op.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("op is missing '{key}'"))
}

fn f64_field(op: &Value, key: &str) -> Result<f64, String> {
    op.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("op is missing numeric '{key}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rotation_request() -> String {
        // 5x5x5 solid-body rotation v = (-y, x, 0): vorticity is (0, 0, 2).
        let mut scalars = Vec::new();
        let mut vectors = Vec::new();
        for k in 0..5u32 {
            for j in 0..5u32 {
                for i in 0..5u32 {
                    let (x, y) = (f64::from(i) * 0.25, f64::from(j) * 0.25);
                    scalars.push(x + y + f64::from(k) * 0.25);
                    vectors.push([-y, x, 0.0]);
                }
            }
        }
        json!({
            "dataset": {
                "id": "rotation",
                "structured": {"dims": [5, 5, 5], "origin": [0, 0, 0], "spacing": [0.25, 0.25, 0.25]},
                "fields": [
                    {"name": "s", "kind": "scalar", "values": scalars},
                    {"name": "velocity", "kind": "vector", "vectors": vectors},
                ],
            },
            "ops": [],
        })
        .to_string()
    }

    #[test]
    fn builds_structured_dataset_and_summarizes() {
        let summary = mesh_filters(&rotation_request()).unwrap();
        let value: Value = serde_json::from_str(&summary).unwrap();
        assert_eq!(value["points"], 125);
        assert_eq!(value["cells"], 64);
        assert!(mesh_problem(&rotation_request()).is_empty());
    }

    #[test]
    fn vorticity_of_solid_rotation_is_two() {
        let mut request: Value = serde_json::from_str(&rotation_request()).unwrap();
        request["ops"] = json!([{"op": "vorticity_magnitude", "field": "velocity"}]);
        let summary = mesh_filters(&request.to_string()).unwrap();
        let value: Value = serde_json::from_str(&summary).unwrap();
        let field = value["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == "vorticity_magnitude")
            .unwrap();
        assert!((field["min"].as_f64().unwrap() - 2.0).abs() < 1e-9);
        assert!((field["max"].as_f64().unwrap() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn slice_reduces_to_surface_and_streamlines_run() {
        let mut request: Value = serde_json::from_str(&rotation_request()).unwrap();
        request["ops"] = json!([{"op": "slice", "normal": [0, 0, 1], "point": [0, 0, 0.5]}]);
        let summary = mesh_filters(&request.to_string()).unwrap();
        let value: Value = serde_json::from_str(&summary).unwrap();
        assert_eq!(value["cells"], 32);

        let stream_request = json!({
            "dataset": request["dataset"],
            "velocity_field": "velocity",
            "seeds": [[0.5, 0.1, 0.5]],
            "step": 0.05,
            "max_steps": 40,
        });
        let out = mesh_streamlines(&stream_request.to_string()).unwrap();
        let lines: Value = serde_json::from_str(&out).unwrap();
        // One seed traces both directions, yielding two polylines.
        assert_eq!(lines["lines"].as_array().unwrap().len(), 2);
        assert!(lines["lines"][0].as_array().unwrap().len() > 10);
    }

    #[test]
    fn unknown_op_and_bad_field_are_errors() {
        let mut request: Value = serde_json::from_str(&rotation_request()).unwrap();
        request["ops"] = json!([{"op": "explode"}]);
        assert!(
            mesh_filters(&request.to_string())
                .unwrap_err()
                .contains("explode")
        );
        request["ops"] = json!([{"op": "threshold", "field": "nope", "min": 0.0, "max": 1.0}]);
        assert!(
            mesh_filters(&request.to_string())
                .unwrap_err()
                .contains("nope")
        );
        assert!(!mesh_problem("{\"dataset\": {\"structured\": {\"dims\": [0, 2, 2]}}}").is_empty());
    }
}
