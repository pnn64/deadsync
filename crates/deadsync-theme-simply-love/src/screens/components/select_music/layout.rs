use crate::ui_layout::UILayoutType;

const LEFT_PANEL_REFERENCE_WIDTH: f32 = 720.0;
const SONG_DETAILS_HEIGHT: f32 = 50.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WidgetBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl WidgetBounds {
    const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn center_x(self) -> f32 {
        self.x + self.width * 0.5
    }

    pub fn center_y(self) -> f32 {
        self.y + self.height * 0.5
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WidgetLayout {
    pub bounds: WidgetBounds,
    pub zoom: f32,
}

impl WidgetLayout {
    fn child(self, bounds: WidgetBounds) -> Self {
        let zoom = self.zoom;
        Self {
            bounds: WidgetBounds::new(
                bounds.x * zoom,
                bounds.y * zoom,
                bounds.width * zoom,
                bounds.height * zoom,
            ),
            zoom,
        }
    }

    fn from_center(center_x: f32, center_y: f32, width: f32, height: f32, zoom: f32) -> Self {
        Self {
            bounds: WidgetBounds::new(
                center_x - width * 0.5,
                center_y - height * 0.5,
                width,
                height,
            ),
            zoom,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SongDetailsLayout {
    pub widget: WidgetLayout,
    pub artist_label: WidgetLayout,
    pub artist_value: WidgetLayout,
    pub bpm_label: WidgetLayout,
    pub bpm_value: WidgetLayout,
    pub length_label: WidgetLayout,
    pub length_value: WidgetLayout,
    pub design_width: f32,
}

impl SongDetailsLayout {
    fn new(widget: WidgetLayout, design_width: f32) -> Self {
        let child = |x, y, width, height| widget.child(WidgetBounds::new(x, y, width, height));
        let length_label_x = design_width - 130.0;
        let length_value_x = design_width - 75.0;
        Self {
            widget,
            artist_label: child(6.0, 5.0, 44.0, 18.0),
            artist_value: child(55.0, 5.0, design_width - 60.0, 18.0),
            bpm_label: child(16.0, 27.0, 34.0, 18.0),
            bpm_value: child(55.0, 27.0, length_label_x - 55.0, 18.0),
            length_label: child(length_label_x, 27.0, 50.0, 18.0),
            length_value: child(length_value_x, 27.0, design_width - length_value_x, 18.0),
            design_width,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SongSelectLayout {
    pub zoom: f32,
    pub banner: WidgetLayout,
    pub song_details: SongDetailsLayout,
    pub chart_panel: WidgetLayout,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ArtistPeakLineLayout {
    pub step_header_y: f32,
    pub artist_background_width: f32,
    pub artist_x_offset: f32,
    pub artist_max_width: f32,
    pub peak_bounds: WidgetBounds,
    pub peak_text_zoom: f32,
    pub peak_text_max_width: f32,
}

impl SongSelectLayout {
    pub fn current() -> Self {
        Self::new(
            deadlib_present::space::screen_width(),
            deadlib_present::space::screen_height(),
            crate::ui_layout::UILayoutType::current(),
        )
    }

    pub fn artist_peak_line(
        self,
        screen_height: f32,
        is_p2_layout: bool,
        expanded: bool,
        peak_text_logical_width: f32,
    ) -> ArtistPeakLineLayout {
        let panel = self.chart_panel.bounds;
        let p2_offset = if is_p2_layout { 88.0 } else { 0.0 };
        let center_y = screen_height * 0.5;
        let step_header_y = if expanded {
            center_y - 18.0 + p2_offset
        } else {
            center_y - screen_height / 56.0 - 9.0 + p2_offset
        };
        let graph_center_y = center_y + if expanded { 62.0 } else { 23.0 } + p2_offset;
        let graph_top = graph_center_y - panel.height * 0.5;
        let peak_text_zoom = 0.8 * self.zoom;
        let peak_width = (peak_text_logical_width * peak_text_zoom).clamp(0.0, panel.width);
        let peak_bounds = WidgetBounds::new(
            panel.width - peak_width,
            step_header_y - graph_top,
            peak_width,
            0.0,
        );
        let gap = 8.0 * self.zoom;
        let artist_background_width = (panel.width - peak_width - gap).max(0.0);
        let artist_x_offset = if expanded { 45.0 } else { 49.5 };
        let artist_max_width =
            (artist_background_width - artist_x_offset - gap).max(0.0) * self.zoom;

        ArtistPeakLineLayout {
            step_header_y,
            artist_background_width,
            artist_x_offset,
            artist_max_width,
            peak_bounds,
            peak_text_zoom,
            peak_text_max_width: peak_width / peak_text_zoom,
        }
    }

    pub fn new(screen_width: f32, screen_height: f32, layout_type: UILayoutType) -> Self {
        let is_ultra_wide = layout_type == UILayoutType::UltraWide;
        let use_full_left_pane = !is_ultra_wide;
        let zoom = if layout_type == UILayoutType::Squareish {
            (screen_width / LEFT_PANEL_REFERENCE_WIDTH).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let center_x = screen_width * 0.5;
        let center_y = screen_height * 0.5;
        let transform_x = |x: f32| center_x + (x - center_x) * zoom;
        let left_pane_width = screen_width * 0.5 - 10.0;

        let (banner_zoom, banner_center_x) = if use_full_left_pane {
            (left_pane_width / 418.0, left_pane_width * 0.5)
        } else {
            let (zoom, offset_x) = (0.7655, -170.0);
            (zoom, transform_x(center_x + offset_x))
        };
        let banner = WidgetLayout::from_center(
            banner_center_x,
            96.0,
            418.0 * banner_zoom,
            164.0 * banner_zoom,
            banner_zoom,
        );

        let (details_design_width, details_center_x) = if use_full_left_pane {
            (left_pane_width / zoom, left_pane_width * 0.5)
        } else {
            (320.0, transform_x(center_x - 170.0))
        };
        let details_width = details_design_width * zoom;
        let details_widget = WidgetLayout {
            bounds: WidgetBounds::new(
                details_center_x - details_width * 0.5,
                center_y - 55.0 - SONG_DETAILS_HEIGHT * 0.5,
                details_width,
                SONG_DETAILS_HEIGHT,
            ),
            zoom,
        };
        let song_details = SongDetailsLayout::new(details_widget, details_design_width);

        let (chart_width, chart_center_x) = if use_full_left_pane {
            // Reserve the Steps Display List column at the right edge of the stats pane.
            (left_pane_width - 32.0, (left_pane_width - 32.0) * 0.5)
        } else {
            (286.0, transform_x(center_x - 187.0))
        };
        let chart_panel = WidgetLayout {
            bounds: WidgetBounds::new(
                chart_center_x - chart_width * 0.5,
                0.0,
                chart_width,
                64.0 * zoom,
            ),
            zoom,
        };

        Self {
            zoom,
            banner,
            song_details,
            chart_panel,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SongSelectLayout, WidgetBounds, WidgetLayout};
    use crate::ui_layout::UILayoutType;

    #[test]
    fn song_details_children_fit_their_widget_at_squareish_sizes() {
        for width in [480.0, 600.0] {
            let layout = SongSelectLayout::new(width, 480.0, UILayoutType::Squareish);
            let details = layout.song_details;
            let parent_width = details.widget.bounds.width;
            for child in [
                details.artist_label,
                details.artist_value,
                details.bpm_label,
                details.bpm_value,
                details.length_label,
                details.length_value,
            ] {
                assert!(child.bounds.x >= 0.0);
                assert!(child.bounds.right() <= parent_width + f32::EPSILON);
            }
            assert!(layout.banner.bounds.x >= 0.0);
            assert!(layout.chart_panel.bounds.x >= 0.0);
        }
    }

    #[test]
    fn wide_song_select_retains_reference_widget_geometry() {
        let layout = SongSelectLayout::new(853.0, 480.0, UILayoutType::Wide);

        assert_eq!(layout.zoom, 1.0);
        assert_eq!(layout.banner.zoom, 416.5 / 418.0);
        assert_eq!(
            layout.banner.bounds,
            WidgetBounds::new(
                0.0,
                96.0 - 82.0 * (416.5 / 418.0),
                416.5,
                164.0 * (416.5 / 418.0)
            )
        );
        assert_eq!(layout.song_details.design_width, 416.5);
        assert_eq!(
            layout.song_details.widget.bounds,
            WidgetBounds::new(0.0, 160.0, 416.5, 50.0)
        );
        assert_eq!(
            layout.chart_panel.bounds,
            WidgetBounds::new(0.0, 0.0, 384.5, 64.0)
        );
    }

    #[test]
    fn non_ultra_wide_left_widgets_share_the_stats_pane_bounds() {
        for (screen_width, screen_height, layout_type) in [
            (640.0, 480.0, UILayoutType::Squareish),
            (600.0, 480.0, UILayoutType::Squareish),
            (853.0, 480.0, UILayoutType::Wide),
            (960.0, 540.0, UILayoutType::Wide),
        ] {
            let layout = SongSelectLayout::new(screen_width, screen_height, layout_type);
            let expected_width = screen_width * 0.5 - 10.0;

            assert_eq!(layout.banner.bounds.x, 0.0);
            assert_eq!(layout.banner.bounds.width, expected_width);
            assert_eq!(layout.song_details.widget.bounds.x, 0.0);
            assert_eq!(layout.song_details.widget.bounds.width, expected_width);
            assert_eq!(layout.chart_panel.bounds.x, 0.0);
            assert_eq!(layout.chart_panel.bounds.width, expected_width - 32.0);
            assert_eq!(layout.chart_panel.bounds.height, 64.0 * layout.zoom);
        }
    }

    #[test]
    fn artist_peak_line_reserves_peak_width_and_aligns_both_elements() {
        for (screen_width, screen_height, layout_type) in [
            (480.0, 480.0, UILayoutType::Squareish),
            (853.0, 480.0, UILayoutType::Wide),
            (1147.0, 480.0, UILayoutType::UltraWide),
        ] {
            let layout = SongSelectLayout::new(screen_width, screen_height, layout_type);
            let panel = layout.chart_panel.bounds;
            for p2_offset in [0.0, 88.0] {
                for expanded in [false, true] {
                    let line =
                        layout.artist_peak_line(screen_height, p2_offset > 0.0, expanded, 99.9);
                    let gap = 8.0 * layout.zoom;
                    let expected_header_y = screen_height * 0.5
                        + if expanded {
                            -18.0
                        } else {
                            -screen_height / 56.0 - 9.0
                        }
                        + p2_offset;
                    let graph_offset = if expanded { 62.0 } else { 23.0 };
                    let graph_top =
                        screen_height * 0.5 + graph_offset + p2_offset - panel.height * 0.5;

                    assert!((line.step_header_y - expected_header_y).abs() < f32::EPSILON);
                    assert!(line.peak_bounds.x >= 0.0);
                    assert!((line.peak_bounds.right() - panel.width).abs() < f32::EPSILON);
                    assert!(
                        (graph_top + line.peak_bounds.y - line.step_header_y).abs() < f32::EPSILON
                    );
                    assert!(
                        (line.artist_background_width + line.peak_bounds.width + gap - panel.width)
                            .abs()
                            < f32::EPSILON
                    );
                    assert!(line.artist_max_width > 0.0);
                    assert!(line.peak_text_zoom > 0.0);
                    assert!(line.peak_text_max_width > 0.0);
                }
            }
        }
    }

    #[test]
    fn ultra_wide_song_select_keeps_its_inset_left_panel() {
        let layout = SongSelectLayout::new(1147.0, 480.0, UILayoutType::UltraWide);

        assert!((layout.banner.bounds.x - 243.5).abs() < 0.02);
        assert_eq!(layout.song_details.widget.bounds.x, 243.5);
        assert_eq!(layout.chart_panel.bounds.x, 243.5);
    }

    #[test]
    fn ultra_wide_song_details_background_stays_centered_on_legacy_anchor() {
        let details = SongSelectLayout::new(1147.0, 480.0, UILayoutType::UltraWide).song_details;
        let bounds = details.widget.bounds;

        assert_eq!(bounds.center_x(), 403.5);
        assert_eq!(bounds.center_y(), 185.0);
        assert_eq!(details.design_width, 320.0);
    }

    #[test]
    fn song_details_text_is_positioned_inside_the_legacy_box_bounds() {
        let details = SongSelectLayout::new(853.0, 480.0, UILayoutType::Wide).song_details;
        let bounds = details.widget.bounds;

        assert_eq!(details.artist_label.bounds.right(), 50.0);
        assert_eq!(details.artist_value.bounds.x, 55.0);
        assert_eq!(details.bpm_label.bounds.right(), 50.0);
        assert_eq!(details.bpm_value.bounds.x, 55.0);
        assert_eq!(
            details.length_label.bounds.right(),
            details.design_width - 80.0
        );
        assert_eq!(details.length_value.bounds.x, details.design_width - 75.0);
        assert_eq!(bounds.x + details.artist_label.bounds.right(), 50.0);
        assert_eq!(
            bounds.x + details.length_label.bounds.right(),
            details.design_width - 80.0
        );
    }

    #[test]
    fn nested_widget_layout_scales_child_bounds_and_carries_zoom() {
        let parent = WidgetLayout {
            bounds: WidgetBounds::new(0.0, 0.0, 100.0, 40.0),
            zoom: 0.5,
        };
        let child = parent.child(WidgetBounds::new(10.0, 4.0, 30.0, 12.0));

        assert_eq!(child.bounds, WidgetBounds::new(5.0, 2.0, 15.0, 6.0));
        assert_eq!(child.zoom, 0.5);
    }
}
