use super::{SongSelectLayout, StepArtistBarY, WidgetBounds, WidgetLayout};
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
    assert_eq!(
        layout.chart_panel.steps_display_list.bounds,
        WidgetBounds::new(384.5, 231.0, 32.0, 152.0)
    );
    assert_eq!(
        layout.chart_panel.steps_display_list.slot_centers_y,
        [247.0, 277.0, 307.0, 337.0, 367.0]
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
        for (vertical_layout, graph_offset, header_gap, height_scale, artist_x_offset) in [
            (StepArtistBarY::Default, 23.0, 32.0, 1.0 / 56.0, 49.5),
            (StepArtistBarY::Expanded, 62.0, 80.0, 0.0, 45.0),
        ] {
            let density_graphs = layout
                .chart_panel
                .compose(screen_height, vertical_layout, 99.9);
            for (index, p2_offset) in [0.0, 88.0].into_iter().enumerate() {
                let graph = density_graphs[index];
                let line = graph.step_artist_peak_nps;
                let gap = 8.0 * layout.zoom;
                let graph_center_y = screen_height * 0.5 + graph_offset;
                let graph_center_y = graph_center_y + p2_offset;
                let expected_header_y = graph_center_y - header_gap - screen_height * height_scale;
                let graph_top = graph.bounds.y;

                assert!((graph.bounds.center_y() - graph_center_y).abs() < f32::EPSILON);
                assert!((graph.body_height - 47.0 * layout.zoom).abs() < f32::EPSILON);
                assert!(
                    (graph.breakdown_text_y - (panel.height - 8.5 * layout.zoom)).abs()
                        < f32::EPSILON
                );
                assert!((graph.breakdown_text_zoom - 0.8 * layout.zoom).abs() < f32::EPSILON);
                assert!((line.step_header_y - expected_header_y).abs() < f32::EPSILON);
                assert!(line.nps_text_bounds.x >= 0.0);
                assert!((line.nps_text_bounds.right() - (panel.width - 5.0)).abs() < f32::EPSILON);
                assert!(
                    (graph_top + line.nps_text_bounds.y - line.step_header_y).abs() < f32::EPSILON
                );
                assert!(
                    (line.artist_background_width + line.nps_text_bounds.width + gap + 5.0
                        - panel.width)
                        .abs()
                        < f32::EPSILON
                );
                assert_eq!(line.artist_x_offset, artist_x_offset);
                assert!(line.artist_max_width > 0.0);
                assert!(line.nps_text_zoom > 0.0);
                assert!(line.nps_text_max_width > 0.0);
            }
        }
    }
}

#[test]
fn ultra_wide_song_select_uses_a_fixed_virtual_left_pane() {
    let layout = SongSelectLayout::new(1147.0, 480.0, UILayoutType::UltraWide);
    let pane = layout.left_pane.bounds;

    assert_eq!(pane, WidgetBounds::new(243.5, 0.0, 320.0, 480.0));
    assert_eq!(layout.banner.bounds.x, pane.x);
    assert_eq!(layout.banner.bounds.width, pane.width);
    assert_eq!(layout.banner.zoom, pane.width / 418.0);
    assert_eq!(layout.song_details.widget.bounds.x, pane.x);
    assert_eq!(layout.song_details.widget.bounds.width, pane.width);
    assert_eq!(layout.chart_panel.bounds.x, pane.x);
    assert_eq!(layout.chart_panel.bounds.width, pane.width - 34.0);
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
