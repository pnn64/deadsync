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

impl SongSelectLayout {
    pub fn new(screen_width: f32, screen_height: f32, layout_type: UILayoutType) -> Self {
        let is_wide = layout_type != UILayoutType::Squareish;
        let zoom = if layout_type == UILayoutType::Squareish {
            (screen_width / LEFT_PANEL_REFERENCE_WIDTH).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let center_x = screen_width * 0.5;
        let center_y = screen_height * 0.5;
        let transform_x = |x: f32| center_x + (x - center_x) * zoom;

        let (banner_zoom, banner_offset_x) = if is_wide {
            (0.7655, -170.0)
        } else {
            (0.75, -166.0)
        };
        let banner_zoom = banner_zoom * zoom;
        let banner_center_x = transform_x(center_x + banner_offset_x);
        let banner = WidgetLayout::from_center(
            banner_center_x,
            96.0,
            418.0 * banner_zoom,
            164.0 * banner_zoom,
            banner_zoom,
        );

        let (details_width, details_offset_x) = if is_wide {
            (320.0, -170.0)
        } else {
            (310.0, -165.0)
        };
        let details_widget = WidgetLayout {
            bounds: WidgetBounds::new(
                transform_x(center_x + details_offset_x) - details_width * zoom * 0.5,
                center_y - 55.0 - SONG_DETAILS_HEIGHT * 0.5,
                details_width * zoom,
                SONG_DETAILS_HEIGHT,
            ),
            zoom,
        };
        let song_details = SongDetailsLayout::new(details_widget, details_width);

        let (chart_width, chart_offset_x) = if is_wide {
            (286.0, -187.0)
        } else {
            (276.0, -182.0)
        };
        let chart_width = chart_width * zoom;
        let chart_center_x = transform_x(center_x + chart_offset_x);
        let chart_panel = WidgetLayout {
            bounds: WidgetBounds::new(chart_center_x - chart_width * 0.5, 0.0, chart_width, 64.0),
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
        assert_eq!(layout.banner.zoom, 0.7655);
        assert_eq!(layout.song_details.design_width, 320.0);
        assert_eq!(
            layout.song_details.widget.bounds,
            WidgetBounds::new(96.5, 160.0, 320.0, 50.0)
        );
        assert_eq!(
            layout.chart_panel.bounds,
            WidgetBounds::new(96.5, 0.0, 286.0, 64.0)
        );
    }

    #[test]
    fn song_details_background_stays_centered_on_legacy_anchor_across_aspect_ratios() {
        for (screen_width, screen_height, layout_type, offset_x, design_width) in [
            (640.0, 480.0, UILayoutType::Squareish, -165.0, 310.0),
            (600.0, 480.0, UILayoutType::Squareish, -165.0, 310.0),
            (853.0, 480.0, UILayoutType::Wide, -170.0, 320.0),
            (960.0, 540.0, UILayoutType::Wide, -170.0, 320.0),
            (1147.0, 480.0, UILayoutType::UltraWide, -170.0, 320.0),
        ] {
            let details =
                SongSelectLayout::new(screen_width, screen_height, layout_type).song_details;
            let zoom = details.widget.zoom;
            let expected_center_x = screen_width.mul_add(0.5, offset_x * zoom);
            let expected_center_y = screen_height * 0.5 - 55.0;
            let bounds = details.widget.bounds;

            assert!((bounds.center_x() - expected_center_x).abs() < 0.001);
            assert!((bounds.center_y() - expected_center_y).abs() < 0.001);
            assert_eq!(details.design_width, design_width);
        }
    }

    #[test]
    fn song_details_text_is_positioned_inside_the_legacy_box_bounds() {
        let details = SongSelectLayout::new(853.0, 480.0, UILayoutType::Wide).song_details;
        let bounds = details.widget.bounds;

        assert_eq!(details.artist_label.bounds.right(), 50.0);
        assert_eq!(details.artist_value.bounds.x, 55.0);
        assert_eq!(details.bpm_label.bounds.right(), 50.0);
        assert_eq!(details.bpm_value.bounds.x, 55.0);
        assert_eq!(details.length_label.bounds.right(), 240.0);
        assert_eq!(details.length_value.bounds.x, 245.0);
        assert_eq!(bounds.x + details.artist_label.bounds.right(), 146.5);
        assert_eq!(bounds.x + details.length_label.bounds.right(), 336.5);
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
