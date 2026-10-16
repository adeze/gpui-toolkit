//! Showcase sections for the mesh demo.
//!
//! Each section is one deterministic scene preset over the same analytic
//! Taylor-Green vortex field (see `scenes.rs`). Keep this beside the
//! manifest emitter so adding a section automatically adds it to the
//! generated snapshot matrix.

/// Mesh showcase section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(super) enum MeshSection {
    /// Lambda-2 vortex cores, streamtubes, ribbons, and solid arrows.
    #[default]
    Vortices,
    /// Mid-plane slice with arrows and contour lines.
    Slice,
    /// Banded pressure contours with contour lines on a slice.
    Contours,
    /// Positive and negative pressure isosurface shells.
    Shells,
}

impl MeshSection {
    /// Every section, in sidebar and capture order.
    pub(super) fn all() -> Vec<Self> {
        vec![
            MeshSection::Vortices,
            MeshSection::Slice,
            MeshSection::Contours,
            MeshSection::Shells,
        ]
    }

    /// Human-readable label shown in the sidebar and gallery.
    pub(super) fn label(&self) -> &'static str {
        match self {
            MeshSection::Vortices => "Vortices",
            MeshSection::Slice => "Slice",

            MeshSection::Contours => "Contours",
            MeshSection::Shells => "Shells",
        }
    }

    /// Short caption describing what the section demonstrates.
    pub(super) fn caption(&self) -> &'static str {
        match self {
            MeshSection::Vortices => {
                "Vortex cores (lambda-2 isosurface) with streamtubes, \
                 stream ribbons, and solid arrows over a Taylor-Green field."
            }
            MeshSection::Slice => {
                "Mid-plane slice: pressure surface with solid velocity arrows \
                 and pressure contour lines."
            }
            MeshSection::Contours => {
                "Banded filled pressure contours with contour lines on the \
                 mid-plane slice."
            }
            MeshSection::Shells => {
                "Pressure isosurface shells at symmetric positive and negative \
                 levels, coloured by pressure."
            }
        }
    }
}

/// Emit the capture inventory consumed by the shared browser gallery tool.
/// Keep this beside the enum so adding a mesh section automatically adds it
/// to the generated snapshot matrix.
#[cfg(not(target_family = "wasm"))]
pub(super) fn visual_manifest_json() -> String {
    use std::fmt::Write;

    const VIEWPORTS: &[(&str, &str, u32, u32, u32)] = &[
        ("desktop", "Desktop release viewport", 1200, 900, 1),
        ("narrow", "Narrow responsive viewport", 390, 844, 2),
    ];

    let mut json = String::from(
        "{\n  \"schema_version\": 1,\n  \"report_type\": \"gpui-scene3d-visual-capture-manifest\",\n  \"crate_name\": \"gpui-scene3d\",\n  \"crate_version\": \"",
    );
    let _ = write!(json, "{}", env!("CARGO_PKG_VERSION"));
    json.push_str("\",\n  \"viewports\": [\n");
    for (index, (id, label, width, height, scale_factor)) in VIEWPORTS.iter().enumerate() {
        let comma = if index + 1 == VIEWPORTS.len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(
            json,
            "    {{ \"id\": \"{id}\", \"label\": \"{label}\", \"width\": {width}, \"height\": {height}, \"scale_factor\": {scale_factor} }}{comma}"
        );
    }
    json.push_str("  ],\n  \"captures\": [\n");

    let sections = MeshSection::all();
    let capture_count = sections.len() * VIEWPORTS.len();
    let mut capture_index = 0;
    for section in &sections {
        let section_slug = slug(section.label());
        for (viewport_id, viewport_label, width, height, scale_factor) in VIEWPORTS {
            capture_index += 1;
            let comma = if capture_index == capture_count {
                ""
            } else {
                ","
            };
            let capture_id = format!("{section_slug}-{viewport_id}");
            let _ = writeln!(
                json,
                "    {{ \"id\": \"{capture_id}\", \"group\": \"Mesh\", \"section\": \"{section_slug}\", \"section_label\": \"{}\", \"viewport_id\": \"{viewport_id}\", \"viewport_label\": \"{viewport_label}\", \"width\": {width}, \"height\": {height}, \"scale_factor\": {scale_factor}, \"renderer\": \"software\", \"renderer_query\": \"software\", \"renderer_qa_queries\": [\"software\"] }}{comma}",
                escape_json(section.label())
            );
        }
    }
    json.push_str("  ]\n}\n");
    json
}

#[cfg(not(target_family = "wasm"))]
fn slug(label: &str) -> String {
    let mut output = String::new();
    let mut pending_dash = false;
    for character in label.chars() {
        if character.is_ascii_alphanumeric() {
            if pending_dash && !output.is_empty() {
                output.push('-');
            }
            output.push(character.to_ascii_lowercase());
            pending_dash = false;
        } else {
            pending_dash = true;
        }
    }
    output
}

#[cfg(not(target_family = "wasm"))]
fn escape_json(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_manifest_covers_every_mesh_section_and_viewport() {
        let manifest = visual_manifest_json();
        assert_eq!(
            manifest.matches("\"viewport_id\"").count(),
            MeshSection::all().len() * 2
        );
        assert!(manifest.contains("\"id\": \"vortices-desktop\""));
        assert!(manifest.contains("\"id\": \"slice-narrow\""));
        assert!(manifest.contains("\"id\": \"contours-desktop\""));
        assert!(manifest.contains("\"id\": \"shells-desktop\""));
        assert!(manifest.contains("\"renderer\": \"software\""));
        assert!(manifest.contains("\"renderer_query\": \"software\""));
    }
}
