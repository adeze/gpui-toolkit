//! VTU (XML VTK unstructured grid) binary I/O.
//!
//! Arrays use inline `format="binary"`: base64 of a little-endian `UInt64`
//! byte-count header followed by the raw array bytes. That is the layout
//! `ParaView` and VTK itself write when compression is off, so files are
//! interchangeable in both directions for the supported array types.

// Rust guideline compliant 2026-02-21

use crate::base64;
use gpui_mesh::{Association, CellType, Cells, DataSet, Field, FieldData};
use std::fmt::Write as _;

fn vtk_code(kind: CellType) -> u32 {
    match kind {
        CellType::Vertex => 1,
        CellType::Line => 3,
        CellType::Triangle => 5,
        CellType::Quad => 9,
        CellType::Tetra => 10,
        CellType::Hexahedron => 12,
        CellType::Wedge => 13,
        CellType::Pyramid => 14,
    }
}

fn from_vtk_code(code: u32) -> Option<CellType> {
    Some(match code {
        1 => CellType::Vertex,
        3 => CellType::Line,
        5 => CellType::Triangle,
        9 => CellType::Quad,
        10 => CellType::Tetra,
        12 => CellType::Hexahedron,
        13 => CellType::Wedge,
        14 => CellType::Pyramid,
        _ => return None,
    })
}

fn binary_block(raw: &[u8]) -> String {
    let mut payload = Vec::with_capacity(8 + raw.len());
    payload.extend_from_slice(&(raw.len() as u64).to_le_bytes());
    payload.extend_from_slice(raw);
    base64::encode(&payload)
}

fn f64_block(values: &[f64]) -> String {
    let mut raw = Vec::with_capacity(values.len() * 8);
    for v in values {
        raw.extend_from_slice(&v.to_le_bytes());
    }
    binary_block(&raw)
}

fn data_array(out: &mut String, ty: &str, name: Option<&str>, components: usize, encoded: &str) {
    let name_attr = name.map_or(String::new(), |n| format!(" Name=\"{n}\""));
    let _ = write!(
        out,
        "<DataArray type=\"{ty}\"{name_attr} NumberOfComponents=\"{components}\" format=\"binary\">{encoded}</DataArray>"
    );
}

fn write_field_arrays(out: &mut String, fields: &[Field]) {
    for field in fields {
        match &field.data {
            FieldData::Scalar(values) => {
                data_array(out, "Float64", Some(&field.name), 1, &f64_block(values));
            }
            FieldData::Vector(values) => {
                let flat: Vec<f64> = values.iter().flatten().copied().collect();
                data_array(out, "Float64", Some(&field.name), 3, &f64_block(&flat));
            }
            FieldData::Tensor(values) => {
                let flat: Vec<f64> = values.iter().flatten().copied().collect();
                data_array(out, "Float64", Some(&field.name), 6, &f64_block(&flat));
            }
            FieldData::Complex(values) => {
                let flat: Vec<f64> = values.iter().flatten().copied().collect();
                data_array(out, "Float64", Some(&field.name), 2, &f64_block(&flat));
            }
        }
    }
}

/// Serializes a dataset as VTU XML text with inline binary arrays.
#[must_use]
pub fn write_vtu(dataset: &DataSet) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "<VTKFile type=\"UnstructuredGrid\" version=\"1.0\" byte_order=\"LittleEndian\" header_type=\"UInt64\">\
         <UnstructuredGrid><Piece NumberOfPoints=\"{}\" NumberOfCells=\"{}\">",
        dataset.point_count(),
        dataset.cell_count()
    );
    out.push_str("<Points>");
    let points: Vec<f64> = dataset.positions.iter().flatten().copied().collect();
    data_array(&mut out, "Float64", None, 3, &f64_block(&points));
    out.push_str("</Points><Cells>");
    let mut conn_raw = Vec::with_capacity(dataset.cells.connectivity.len() * 4);
    for index in dataset.cells.connectivity.iter() {
        conn_raw.extend_from_slice(&index.to_le_bytes());
    }
    data_array(
        &mut out,
        "Int32",
        Some("connectivity"),
        1,
        &binary_block(&conn_raw),
    );
    let mut offsets_raw = Vec::with_capacity(dataset.cells.offsets.len() * 8);
    for offset in dataset.cells.offsets.iter().skip(1) {
        offsets_raw.extend_from_slice(&offset.to_le_bytes());
    }
    data_array(
        &mut out,
        "Int64",
        Some("offsets"),
        1,
        &binary_block(&offsets_raw),
    );
    let types: Vec<u8> = dataset
        .cells
        .types
        .iter()
        .map(|t| vtk_code(*t) as u8)
        .collect();
    data_array(&mut out, "UInt8", Some("types"), 1, &binary_block(&types));
    out.push_str("</Cells><PointData>");
    write_field_arrays(&mut out, &dataset.point_data);
    out.push_str("</PointData><CellData>");
    write_field_arrays(&mut out, &dataset.cell_data);
    out.push_str("</CellData></Piece></UnstructuredGrid></VTKFile>");
    out
}

/// One parsed `DataArray` element.
struct RawArray {
    name: Option<String>,
    ty: String,
    components: usize,
    format: String,
    body: String,
}

fn parse_arrays(section: &str) -> Vec<RawArray> {
    let mut arrays = Vec::new();
    let mut rest = section;
    while let Some(start) = rest.find("<DataArray") {
        let after = &rest[start..];
        let Some(tag_end) = after.find('>') else {
            break;
        };
        let tag = &after[..tag_end];
        let attr = |key: &str| -> Option<String> {
            let pattern = format!("{key}=\"");
            let idx = tag.find(&pattern)? + pattern.len();
            let end = tag[idx..].find('"')? + idx;
            Some(tag[idx..end].to_string())
        };
        let self_closing = tag.ends_with('/');
        let (body, advance) = if self_closing {
            (String::new(), tag_end + 1)
        } else {
            let close = after[tag_end..]
                .find("</DataArray>")
                .map_or(after.len(), |c| tag_end + c);
            (
                after[tag_end + 1..close].trim().to_string(),
                close + "</DataArray>".len().min(after.len() - close),
            )
        };
        arrays.push(RawArray {
            name: attr("Name"),
            ty: attr("type").unwrap_or_default(),
            components: attr("NumberOfComponents")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1),
            format: attr("format").unwrap_or_else(|| "ascii".into()),
            body,
        });
        rest = &after[advance.min(after.len())..];
        if advance == 0 {
            break;
        }
    }
    arrays
}

fn section<'a>(xml: &'a str, open: &str, close: &str) -> &'a str {
    let start = xml.find(open).map_or(0, |i| i + open.len());
    let end = xml[start..].find(close).map_or(xml.len(), |i| start + i);
    &xml[start..end]
}

fn decode_array(array: &RawArray) -> Result<Vec<u8>, String> {
    match array.format.as_str() {
        "binary" => {
            let payload = base64::decode(&array.body)?;
            if payload.len() < 8 {
                return Err("binary DataArray shorter than its header".into());
            }
            let declared = u64::from_le_bytes(
                payload[..8]
                    .try_into()
                    .map_err(|_| "bad header".to_string())?,
            ) as usize;
            let raw = &payload[8..];
            if raw.len() < declared {
                return Err(format!(
                    "binary DataArray truncated: header says {declared} bytes, found {}",
                    raw.len()
                ));
            }
            Ok(raw[..declared].to_vec())
        }
        "ascii" => Err("ascii VTU arrays are not supported by this reader".into()),
        other => Err(format!("unsupported DataArray format '{other}'")),
    }
}

fn array_f64(array: &RawArray) -> Result<Vec<f64>, String> {
    let raw = decode_array(array)?;
    match array.ty.as_str() {
        "Float64" | "Double" => Ok(raw
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| f64::from_le_bytes(*c))
            .collect()),
        "Float32" | "Float" => Ok(raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f64::from(f32::from_le_bytes(*c)))
            .collect()),
        other => Err(format!("expected a float array, found type '{other}'")),
    }
}

fn array_i64(array: &RawArray) -> Result<Vec<i64>, String> {
    let raw = decode_array(array)?;
    match array.ty.as_str() {
        "Int64" => Ok(raw
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| i64::from_le_bytes(*c))
            .collect()),
        "Int32" => Ok(raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| i64::from(i32::from_le_bytes(*c)))
            .collect()),
        "UInt32" => Ok(raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| i64::from(u32::from_le_bytes(*c)))
            .collect()),
        "UInt8" => Ok(raw.iter().map(|b| i64::from(*b)).collect()),
        other => Err(format!("expected an integer array, found type '{other}'")),
    }
}

fn field_from_array(array: &RawArray, association: Association) -> Result<Option<Field>, String> {
    let Some(name) = &array.name else {
        return Ok(None);
    };
    let values = array_f64(array)?;
    let mut field = match array.components {
        1 => Field::scalar(name.clone(), values),
        3 => {
            if values.len() % 3 != 0 {
                return Err(format!(
                    "vector field '{name}' length is not a multiple of 3"
                ));
            }
            Field::vector(
                name.clone(),
                values
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| [c[0], c[1], c[2]])
                    .collect::<Vec<_>>(),
            )
        }
        6 => {
            if values.len() % 6 != 0 {
                return Err(format!(
                    "tensor field '{name}' length is not a multiple of 6"
                ));
            }
            let mut field = Field::scalar(name.clone(), Vec::<f64>::new());
            field.data = FieldData::Tensor(
                values
                    .as_chunks::<6>()
                    .0
                    .iter()
                    .map(|c| [c[0], c[1], c[2], c[3], c[4], c[5]])
                    .collect::<Vec<_>>()
                    .into(),
            );
            field
        }
        2 => {
            if values.len() % 2 != 0 {
                return Err(format!("complex field '{name}' length is not even"));
            }
            let mut field = Field::scalar(name.clone(), Vec::<f64>::new());
            field.data = FieldData::Complex(
                values
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| [c[0], c[1]])
                    .collect::<Vec<_>>()
                    .into(),
            );
            field
        }
        other => {
            return Err(format!(
                "field '{name}' has unsupported component count {other}"
            ));
        }
    };
    field.association = association;
    Ok(Some(field))
}

/// Parses VTU XML text with inline binary arrays.
///
/// # Errors
///
/// Returns a description of the first structural problem encountered.
pub fn read_vtu(xml: &str) -> Result<DataSet, String> {
    if !xml.contains("UnstructuredGrid") {
        return Err("not a VTU unstructured-grid file".into());
    }
    // Points.
    let points_section = section(xml, "<Points>", "</Points>");
    let point_arrays = parse_arrays(points_section);
    let point_array = point_arrays
        .first()
        .ok_or("VTU file has no Points DataArray")?;
    let flat = array_f64(point_array)?;
    if flat.len() % 3 != 0 {
        return Err("points array length is not a multiple of 3".into());
    }
    let positions: Vec<[f64; 3]> = flat
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| [c[0], c[1], c[2]])
        .collect();

    // Cells.
    let cells_section = section(xml, "<Cells>", "</Cells>");
    let cell_arrays = parse_arrays(cells_section);
    let find = |name: &str| {
        cell_arrays
            .iter()
            .find(|a| a.name.as_deref() == Some(name))
            .ok_or_else(|| format!("VTU Cells section lacks '{name}'"))
    };
    let connectivity: Vec<u32> = array_i64(find("connectivity")?)?
        .into_iter()
        .map(|v| v as u32)
        .collect();
    let mut offsets: Vec<u64> = vec![0];
    for value in array_i64(find("offsets")?)? {
        offsets.push(value as u64);
    }
    let types: Vec<CellType> = array_i64(find("types")?)?
        .into_iter()
        .map(|code| {
            from_vtk_code(code as u32).ok_or_else(|| format!("unsupported VTK cell type {code}"))
        })
        .collect::<Result<_, _>>()?;
    let cells = Cells {
        types: types.into(),
        offsets: offsets.into(),
        connectivity: connectivity.into(),
    };

    let mut dataset = DataSet::new("vtu", positions, cells);
    for array in parse_arrays(section(xml, "<PointData>", "</PointData>")) {
        if let Some(field) = field_from_array(&array, Association::Point)? {
            dataset.point_data.push(field);
        }
    }
    for array in parse_arrays(section(xml, "<CellData>", "</CellData>")) {
        if let Some(field) = field_from_array(&array, Association::Cell)? {
            dataset.cell_data.push(field);
        }
    }
    dataset
        .validate()
        .map_err(|e| format!("parsed dataset is invalid: {e}"))?;
    Ok(dataset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_binary_vtu() {
        let dataset = DataSet::structured_grid("room", [3, 2, 2], [0.0; 3], [0.5; 3])
            .unwrap()
            .with_field(Field::scalar("pressure", vec![1.5; 12]))
            .with_field(Field::vector("velocity", vec![[1.0, 2.0, 3.0]; 12]))
            .with_field(Field::scalar("cell_id", vec![0.0, 1.0]).on_cells());
        let xml = write_vtu(&dataset);
        assert!(xml.contains("format=\"binary\""));
        let parsed = read_vtu(&xml).unwrap();
        assert_eq!(parsed.point_count(), 12);
        assert_eq!(parsed.cell_count(), 2);
        assert_eq!(parsed.cells.types[0], CellType::Hexahedron);
        assert_eq!(
            parsed
                .field(Association::Point, "pressure")
                .unwrap()
                .as_scalar()
                .unwrap(),
            &[1.5; 12]
        );
        assert_eq!(
            parsed
                .field(Association::Point, "velocity")
                .unwrap()
                .as_vector()
                .unwrap()[0],
            [1.0, 2.0, 3.0]
        );
        assert_eq!(
            parsed
                .field(Association::Cell, "cell_id")
                .unwrap()
                .as_scalar()
                .unwrap(),
            &[0.0, 1.0]
        );
        assert_eq!(parsed.validate(), Ok(()));
    }

    #[test]
    fn rejects_non_vtu_input() {
        let err = read_vtu("<html></html>").unwrap_err();
        assert!(err.contains("not a VTU"));
    }
}
