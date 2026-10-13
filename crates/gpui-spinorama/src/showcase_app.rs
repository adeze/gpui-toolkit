use super::speaker::{CEA2034_SERIES, SpeakerModel, angle_color};
use super::spinorama_section::SpinoramaSection;
use gpui::*;
use gpui_px::{
    ChartAnnotation, ColorScale, LegendPosition, ScaleType, contour, heatmap, line,
};
use gpui_ui_kit::theme::{Theme, ThemeExt};

const COMPACT_BREAKPOINT: f32 = 600.0;
const SIDEBAR_WIDTH: f32 = 220.0;

const HORIZONTAL_ANGLES: &[f64] = &[-90.0, -60.0, -30.0, 0.0, 30.0, 60.0, 90.0];
const VERTICAL_ANGLES: &[f64] = &[-90.0, -45.0, 0.0, 45.0, 90.0];

pub struct SpinoramaApp {
    current_section: SpinoramaSection,
    speaker: SpeakerModel,
    scroll_handle: ScrollHandle,
}

impl SpinoramaApp {
    fn section_key(value: &str) -> String {
        value
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|character| character.to_ascii_lowercase())
            .collect()
    }

    /// Optional initial-section override for browser capture, matched
    /// case-insensitively against `SpinoramaSection::label()` or its slug.
    fn initial_section() -> SpinoramaSection {
        #[cfg(target_family = "wasm")]
        let requested = gpui_miniapp::web_query_param("section");
        #[cfg(not(target_family = "wasm"))]
        let requested = std::env::var("SPINORAMA_SHOWCASE_SECTION").ok();

        requested
            .and_then(|name| {
                SpinoramaSection::all()
                    .iter()
                    .copied()
                    .find(|s| Self::section_key(s.label()) == Self::section_key(name.trim()))
            })
            .unwrap_or_default()
    }

    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            current_section: Self::initial_section(),
            speaker: SpeakerModel::build(),
            scroll_handle: ScrollHandle::new(),
        }
    }

    fn select_section(&mut self, section: SpinoramaSection) {
        self.current_section = section;
        self.scroll_handle
            .set_offset(Point::new(px(0.0), px(0.0)));
    }

    fn section_header(&self, section: SpinoramaSection, theme: &Theme) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .mb_4()
            .child(
                div()
                    .text_size(px(22.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .child(section.label()),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(theme.text_secondary)
                    .child(section.description()),
            )
    }

    fn chart_width(&self, window: &Window) -> f32 {
        let viewport = window.viewport_size().width.as_f32();
        let compact = viewport < COMPACT_BREAKPOINT;
        let available = if compact {
            viewport - 32.0
        } else {
            viewport - SIDEBAR_WIDTH - 64.0
        };
        available.clamp(280.0, 720.0)
    }

    fn render_overview(&self, theme: &Theme) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.section_header(SpinoramaSection::Overview, theme))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(theme.text_secondary)
                    .max_w(px(640.0))
                    .child(
                        "A deterministic demo speaker rendered with gpui-px charts: CEA-2034 response curves plus horizontal and vertical dispersion. The model combines an on-axis response with ripple and narrowing high-frequency directivity, using the same formulas as python/examples/spinorama_demo.py.",
                    ),
            )
            .child(
                div()
                    .p_4()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text_muted)
                            .child("DEMO MODEL"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_muted)
                            .child("64 log-spaced frequencies (20 Hz - 20 kHz) x 37 angles (-180 to +180 degrees) - 35 to 95 dB SPL window"),
                    ),
            )
    }

    fn render_cea2034(&self, theme: &Theme, chart_width: f32, compact: bool) -> Div {
        let speaker = &self.speaker;
        let first = &speaker.cea2034[0].1;
        let mut chart = line(&speaker.frequencies, first)
            .title("CEA-2034 response")
            .x_label("Frequency (Hz)")
            .y_label("SPL (dB)")
            .label(CEA2034_SERIES[0].1)
            .color(CEA2034_SERIES[0].2)
            .stroke_width(2.0)
            .x_scale(ScaleType::Log)
            .x_range(20.0, 20_000.0)
            .y_range(35.0, 95.0)
            .annotation(
                ChartAnnotation::x_value("reference-frequency", "1 kHz", 1000.0).color(0x94a3b8),
            )
            .size(chart_width, chart_width * 0.62);
        if compact {
            chart = chart.legend_position(LegendPosition::Bottom);
        }
        for ((id, values), (series_id, label, color)) in
            speaker.cea2034.iter().skip(1).zip(CEA2034_SERIES.iter().skip(1))
        {
            debug_assert_eq!(id.as_str(), *series_id);
            chart = chart.add_series(values, Some(*label), *color, 2.0, 1.0);
        }
        let chart = chart.build().unwrap();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.section_header(SpinoramaSection::Cea2034, theme))
            .child(chart)
    }

    fn render_angle_lines(
        &self,
        section: SpinoramaSection,
        angles: &[f64],
        title: &str,
        chart_width: f32,
        compact: bool,
        theme: &Theme,
    ) -> Div {
        let speaker = &self.speaker;
        let first_curve = speaker.curve_at_angle(angles[0]);
        let mut chart = line(&speaker.frequencies, &first_curve)
            .title(title.to_string())
            .x_label("Frequency (Hz)")
            .y_label("SPL (dB)")
            .label(format!("{:+.0}°", angles[0]))
            .color(angle_color(angles[0]))
            .stroke_width(2.0)
            .x_scale(ScaleType::Log)
            .x_range(20.0, 20_000.0)
            .y_range(35.0, 95.0)
            .size(chart_width, chart_width * 0.62);
        if compact {
            chart = chart.legend_position(LegendPosition::Bottom);
        }
        for angle in &angles[1..] {
            let curve = speaker.curve_at_angle(*angle);
            chart = chart.add_series(
                &curve,
                Some(format!("{angle:+.0}°")),
                angle_color(*angle),
                2.0,
                1.0,
            );
        }
        let chart = chart.build().unwrap();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.section_header(section, theme))
            .child(chart)
    }

    fn render_dispersion(&self, theme: &Theme, chart_width: f32) -> Div {
        let speaker = &self.speaker;
        let chart = heatmap(
            &speaker.directivity_grid,
            speaker.frequencies.len(),
            speaker.angles.len(),
        )
        .title("Horizontal directivity")
        .x(&speaker.frequencies)
        .y(&speaker.angles)
        .x_scale(ScaleType::Log)
        .color_scale(ColorScale::Viridis)
        .size(chart_width, chart_width * 0.62)
        .build()
        .unwrap();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.section_header(SpinoramaSection::Dispersion, theme))
            .child(chart)
    }

    fn render_contour(&self, theme: &Theme, chart_width: f32) -> Div {
        let speaker = &self.speaker;
        let chart = contour(
            &speaker.directivity_grid,
            speaker.frequencies.len(),
            speaker.angles.len(),
        )
        .title("Horizontal directivity contour")
        .x(&speaker.frequencies)
        .y(&speaker.angles)
        .x_scale(ScaleType::Log)
        .thresholds(vec![40.0, 50.0, 60.0, 70.0, 80.0, 90.0])
        .color_scale(ColorScale::Viridis)
        .aspect_ratio(1.45)
        .size(chart_width, chart_width * 0.62)
        .build()
        .unwrap();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.section_header(SpinoramaSection::Contour, theme))
            .child(chart)
    }

    fn render_section(&self, window: &Window, theme: &Theme) -> Div {
        let chart_width = self.chart_width(window);
        let compact = window.viewport_size().width.as_f32() < COMPACT_BREAKPOINT;
        match self.current_section {
            SpinoramaSection::Overview => self.render_overview(theme),
            SpinoramaSection::Cea2034 => self.render_cea2034(theme, chart_width, compact),
            SpinoramaSection::Horizontal => self.render_angle_lines(
                SpinoramaSection::Horizontal,
                HORIZONTAL_ANGLES,
                "Horizontal SPL",
                chart_width,
                compact,
                theme,
            ),
            SpinoramaSection::Vertical => self.render_angle_lines(
                SpinoramaSection::Vertical,
                VERTICAL_ANGLES,
                "Vertical SPL",
                chart_width,
                compact,
                theme,
            ),
            SpinoramaSection::Dispersion => self.render_dispersion(theme, chart_width),
            SpinoramaSection::Contour => self.render_contour(theme, chart_width),
        }
    }
}

impl Render for SpinoramaApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let compact = window.viewport_size().width.as_f32() < COMPACT_BREAKPOINT;
        let current = self.current_section;

        let nav_item = |section: SpinoramaSection, horizontal: bool| {
            let is_selected = section == current;
            let mut item = div()
                .id(ElementId::Name(
                    format!("spinorama-nav-{}", section.label()).into(),
                ))
                .px_3()
                .py_2()
                .rounded_md()
                .cursor_pointer()
                .text_sm()
                .child(section.label());
            if horizontal {
                item = item.flex_none().whitespace_nowrap();
            } else {
                item = item.w_full();
            }
            if is_selected {
                item = item
                    .bg(theme.accent)
                    .text_color(theme.text_on_accent)
                    .font_weight(FontWeight::SEMIBOLD);
            } else {
                let hover = theme.surface_hover;
                item = item
                    .text_color(theme.text_secondary)
                    .hover(move |s| s.bg(hover));
            }
            item.on_click(cx.listener(move |this, _, _, _| {
                this.select_section(section);
            }))
        };

        let sidebar = div()
            .flex()
            .flex_col()
            .gap_1()
            .p_4()
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .mb_2()
                    .child("Spinorama"),
            )
            .children(
                SpinoramaSection::all()
                    .iter()
                    .copied()
                    .map(|section| nav_item(section, false)),
            );

        let top_nav = div().flex_none().border_b_1().border_color(theme.border).child(
            div()
                .id("spinorama-mobile-nav")
                .w_full()
                .flex()
                .gap_1()
                .p_2()
                .overflow_x_scroll()
                .children(
                    SpinoramaSection::all()
                        .iter()
                        .copied()
                        .map(|section| nav_item(section, true)),
                ),
        );

        let content = div()
            .id("spinorama-content-scroll")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .p_4()
            .child(self.render_section(window, &theme));

        let root = div()
            .id("spinorama-root")
            .w_full()
            .h_full()
            .bg(theme.background)
            .text_color(theme.text_secondary)
            .flex();
        if compact {
            root.flex_col().child(top_nav).child(content)
        } else {
            root.child(
                div()
                    .id("spinorama-sidebar-scroll")
                    .w(px(SIDEBAR_WIDTH))
                    .flex_none()
                    .h_full()
                    .bg(theme.surface)
                    .border_r_1()
                    .border_color(theme.border)
                    .overflow_y_scroll()
                    .child(sidebar),
            )
            .child(content)
        }
    }
}
