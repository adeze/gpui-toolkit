#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum SpinoramaSection {
    #[default]
    Overview,
    Cea2034,
    Horizontal,
    Vertical,
    Dispersion,
    Contour,
}

impl SpinoramaSection {
    pub(super) fn all() -> &'static [SpinoramaSection] {
        &[
            SpinoramaSection::Overview,
            SpinoramaSection::Cea2034,
            SpinoramaSection::Horizontal,
            SpinoramaSection::Vertical,
            SpinoramaSection::Dispersion,
            SpinoramaSection::Contour,
        ]
    }

    pub(super) fn label(&self) -> &'static str {
        match self {
            SpinoramaSection::Overview => "Overview",
            SpinoramaSection::Cea2034 => "CEA-2034",
            SpinoramaSection::Horizontal => "Horizontal SPL",
            SpinoramaSection::Vertical => "Vertical SPL",
            SpinoramaSection::Dispersion => "Dispersion",
            SpinoramaSection::Contour => "Contour",
        }
    }

    pub(super) fn description(&self) -> &'static str {
        match self {
            SpinoramaSection::Overview => {
                "Spinorama-style speaker measurements rendered with gpui-px charts."
            }
            SpinoramaSection::Cea2034 => {
                "CEA-2034 response curves: on-axis, listening window, early reflections, sound power, and predicted in-room response."
            }
            SpinoramaSection::Horizontal => {
                "Horizontal off-axis SPL from -90 to +90 degrees; high frequencies narrow while bass stays omnidirectional."
            }
            SpinoramaSection::Vertical => {
                "Vertical off-axis SPL showing the symmetric narrowing of the demo speaker."
            }
            SpinoramaSection::Dispersion => {
                "Full horizontal dispersion map: SPL over frequency and angle."
            }
            SpinoramaSection::Contour => {
                "Directivity contour bands at 10 dB steps from 40 to 90 dB SPL."
            }
        }
    }
}

/// Emit the capture inventory consumed by the shared browser gallery tool.
/// Keep this beside the enum so adding a section automatically adds it
/// to the generated snapshot matrix.
#[cfg(not(target_family = "wasm"))]
pub(super) fn visual_manifest_json() -> String {
    use std::fmt::Write;

    const VIEWPORTS: &[(&str, &str, u32, u32, u32)] = &[
        ("desktop", "Desktop release viewport", 1200, 900, 1),
        ("narrow", "Narrow responsive viewport", 390, 844, 2),
    ];

    let mut json = String::from(
        "{\n  \"schema_version\": 1,\n  \"report_type\": \"gpui-spinorama-visual-capture-manifest\",\n  \"crate_name\": \"gpui-spinorama\",\n  \"crate_version\": \"",
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

    let sections = SpinoramaSection::all();
    let capture_count = sections.len() * VIEWPORTS.len();
    let mut capture_index = 0;
    for section in sections {
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
                "    {{ \"id\": \"{capture_id}\", \"group\": \"Spinorama\", \"section\": \"{section_slug}\", \"section_label\": \"{}\", \"viewport_id\": \"{viewport_id}\", \"viewport_label\": \"{viewport_label}\", \"width\": {width}, \"height\": {height}, \"scale_factor\": {scale_factor} }}{comma}",
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
    fn visual_manifest_covers_every_section_and_viewport() {
        let manifest = visual_manifest_json();
        assert_eq!(
            manifest.matches("\"viewport_id\"").count(),
            SpinoramaSection::all().len() * 2
        );
        assert!(manifest.contains("\"id\": \"cea-2034-desktop\""));
        assert!(manifest.contains("\"id\": \"dispersion-narrow\""));
    }
}
