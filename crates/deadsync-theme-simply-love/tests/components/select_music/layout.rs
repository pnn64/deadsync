use super::chart_panel::DIFFICULTY_SLOT_COUNT;
use super::{SongSelectLayout, StepArtistBarLayout, WidgetBounds, WidgetLayout};
use crate::ui_layout::UILayoutType;
use crate::views::{SelectMusicPresentationPolicyView, SelectMusicSessionView};

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
        assert!(layout.chart_panel.x >= 0.0);
    }
}

#[test]
fn wide_song_select_retains_reference_widget_geometry() {
    let layout = SongSelectLayout::new(853.0, 480.0, UILayoutType::Wide);
    let pane = layout.left_pane.bounds;

    assert_eq!(layout.zoom, 1.0);
    assert_eq!(layout.banner.zoom, pane.width / 418.0);
    assert_eq!(layout.banner.bounds.center_x(), pane.center_x());
    assert_eq!(layout.banner.bounds.x, pane.x);
    assert_eq!(layout.banner.bounds.width, pane.width);
    assert!((layout.banner.bounds.height - 164.0 * layout.banner.zoom).abs() < 0.001);
    assert_eq!(layout.song_details.design_width, pane.width);
    assert_eq!(
        layout.song_details.widget.bounds,
        WidgetBounds::new(
            pane.x,
            layout.banner.bounds.bottom() + 2.0,
            pane.width,
            50.0
        )
    );
    let chart_panel = layout.chart_panel;
    let density = layout.density_panel.bounds;
    let difficulty_list = layout.difficulty_list.bounds;
    assert_eq!(
        density,
        WidgetBounds::new(pane.x, density.y, pane.width - 34.0, 64.0)
    );
    assert_eq!(
        chart_panel,
        WidgetBounds::new(pane.x, density.y, pane.width, 154.0)
    );
    assert_eq!(chart_panel.y, density.y);
    assert_eq!(difficulty_list.y, density.y);
    assert_eq!(difficulty_list.bottom(), chart_panel.bottom());
    let [p1_pattern, p2_pattern] = layout.pattern_panels.map(|panel| panel.bounds);
    assert_eq!(p1_pattern, density);
    assert_eq!(p2_pattern.bottom(), chart_panel.bottom());
    assert_eq!(p2_pattern.y, density.bottom() + 24.0);
    assert!((chart_panel.bottom() - density.bottom() - 90.0).abs() < 0.001);
    let header = layout.density_graphs(StepArtistBarLayout::Legacy, 99.9)[0].step_artist_peak_nps;
    assert!(
        (header.step_header_y + layout.step_artist_header.height * 0.5 - density.y).abs() < 0.001
    );
    for (slot, center_y) in layout
        .difficulty_list
        .slot_centers_y
        .into_iter()
        .enumerate()
    {
        let bounds = layout.difficulty_list.bounds;
        let border = 2.0;
        let slot_height = (bounds.height - border * 2.0) / DIFFICULTY_SLOT_COUNT as f32;
        let expected_center_y = bounds.y + border + (slot as f32 + 0.5) * slot_height;
        assert!((center_y - expected_center_y).abs() < 0.001);
    }
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
        let pane = layout.left_pane.bounds;
        let expected_width = pane.width;

        assert_eq!(layout.banner.bounds.x, pane.x);
        assert!((layout.banner.bounds.width - expected_width).abs() < 0.001);
        assert_eq!(layout.song_details.widget.bounds.x, pane.x);
        assert_eq!(layout.song_details.widget.bounds.width, pane.width);
        assert_eq!(
            layout.song_details.widget.bounds.y,
            layout.banner.bounds.bottom() + 2.0
        );
        assert_eq!(layout.chart_panel.x, pane.x);
        assert_eq!(layout.chart_panel.width, expected_width);
        assert_eq!(layout.density_panel.bounds.width, expected_width - 34.0);
        assert!((layout.density_panel.bounds.height - 64.0 * layout.zoom).abs() < 0.001);
        assert_eq!(layout.chart_panel.height, 154.0);
        assert!(layout.chart_panel.y >= layout.song_details.widget.bounds.bottom());
        let density_graph = layout.density_graphs(StepArtistBarLayout::Legacy, 99.9)[0];
        let step_header_y = density_graph.step_artist_peak_nps.step_header_y;
        assert!(step_header_y >= layout.song_details.widget.bounds.bottom());
        assert!(
            (step_header_y + layout.step_artist_header.height * 0.5
                - layout.density_panel.bounds.y)
                .abs()
                < 0.001
        );
    }
}

#[test]
fn artist_peak_line_reserves_peak_width_and_aligns_both_elements() {
    for (screen_width, screen_height, layout_type) in [
        (480.0, 480.0, UILayoutType::Squareish),
        (853.0, 480.0, UILayoutType::Wide),
        (1147.0, 480.0, UILayoutType::UltraWide),
    ] {
        for (vertical_layout, artist_x_offset) in [
            (StepArtistBarLayout::Legacy, 49.5),
            (StepArtistBarLayout::Expanded, 45.0),
        ] {
            let mut presentation = SelectMusicPresentationPolicyView::default();
            presentation.step_artist_expanded =
                matches!(vertical_layout, StepArtistBarLayout::Expanded);
            let layout = SongSelectLayout::new_with_context(
                screen_width,
                screen_height,
                layout_type,
                presentation,
                SelectMusicSessionView::default(),
            );
            let panel = layout.density_panel.bounds;
            assert_eq!(layout.chart_panel.height, 154.0);
            assert!((layout.difficulty_list.bounds.height - 154.0).abs() < 0.001);
            assert!(
                (layout.pattern_panels[1].bounds.bottom() - layout.difficulty_list.bounds.bottom())
                    .abs()
                    < 0.001
            );
            let density_graphs = layout.density_graphs(vertical_layout, 99.9);
            for index in 0..density_graphs.len() {
                let graph = density_graphs[index];
                let line = graph.step_artist_peak_nps;
                let gap = 8.0 * layout.zoom;
                let graph_center_y = if index == 0 {
                    layout.density_panel.bounds.center_y()
                } else {
                    layout.chart_panel.bottom() - graph.bounds.height * 0.5
                };
                let expected_header_y = match vertical_layout {
                    StepArtistBarLayout::Legacy => {
                        graph.bounds.y - layout.step_artist_header.height * 0.5
                    }
                    StepArtistBarLayout::Expanded => layout.step_artist_header.center_y(),
                };
                let graph_top = graph.bounds.y;

                assert!((graph.bounds.center_y() - graph_center_y).abs() < f32::EPSILON);
                if index == 1 {
                    assert!((graph.bounds.bottom() - layout.chart_panel.bottom()).abs() < 0.001);
                }
                assert!((graph.body_height - 47.0 * layout.zoom).abs() < f32::EPSILON);
                assert!(
                    (graph.breakdown_text_y - (panel.height - 8.5 * layout.zoom)).abs()
                        < f32::EPSILON
                );
                assert!((graph.breakdown_text_zoom - 0.8 * layout.zoom).abs() < f32::EPSILON);
                assert!((line.step_header_y - expected_header_y).abs() < f32::EPSILON);
                assert!(layout.step_artist_header.y >= layout.song_details.widget.bounds.bottom());
                assert!(
                    (line.artist_background_height - layout.step_artist_header.height).abs()
                        < 0.001
                );
                if matches!(vertical_layout, StepArtistBarLayout::Expanded) {
                    assert_eq!(layout.chart_panel.y, layout.step_artist_header.bottom());
                    assert_eq!(layout.density_panel.bounds.y, layout.chart_panel.y);
                    assert_eq!(
                        layout.pattern_panels[1].bounds.y,
                        layout.density_panel.bounds.bottom() + 24.0
                    );
                    assert_eq!(line.artist_background_y, layout.step_artist_header.y);
                    assert!(
                        (line.artist_background_y + line.artist_background_height
                            - layout.density_panel.bounds.y)
                            .abs()
                            < 0.001
                    );
                    assert!(line.artist_text_y < line.step_header_y);
                }
                assert!(line.nps_text_bounds.x >= 0.0);
                assert!((line.nps_text_bounds.right() - (panel.width - gap)).abs() < f32::EPSILON);
                assert!(
                    (graph_top + line.nps_text_bounds.y - line.step_header_y).abs() < f32::EPSILON
                );
                assert!(
                    (line.artist_background_width + line.nps_text_bounds.width + gap * 2.0
                        - panel.width)
                        .abs()
                        < 0.001
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
    assert_eq!(layout.song_details.widget.bounds.x, layout.banner.bounds.x);
    assert_eq!(
        layout.song_details.widget.bounds.y,
        layout.banner.bounds.bottom() + 2.0
    );
    assert_eq!(
        layout.song_details.widget.bounds.width,
        layout.banner.bounds.width
    );
    assert_eq!(layout.chart_panel.x, pane.x);
    assert_eq!(layout.chart_panel.width, pane.width);
}

#[test]
fn ultra_wide_song_details_background_stays_centered_on_legacy_anchor() {
    let layout = SongSelectLayout::new(1147.0, 480.0, UILayoutType::UltraWide);
    let details = layout.song_details;
    let bounds = details.widget.bounds;

    assert_eq!(bounds.center_x(), 403.5);
    assert_eq!(bounds.y, layout.banner.bounds.bottom() + 2.0);
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
    assert_eq!(
        bounds.x + details.artist_label.bounds.right(),
        bounds.x + 50.0
    );
    assert_eq!(
        bounds.x + details.length_label.bounds.right(),
        bounds.x + details.design_width - 80.0
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
