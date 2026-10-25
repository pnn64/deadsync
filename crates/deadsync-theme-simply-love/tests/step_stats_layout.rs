use deadsync_profile::PlayerSide;
use deadsync_theme_simply_love::scorebox::SCOREBOX_W;
use deadsync_theme_simply_love::step_stats::{
    STEP_STATS_BANNER_H, STEP_STATS_BANNER_W, StepStatsPaneParams, density_graph_rect,
    judgment_count_center_x, pack_banner_placement, pane_layout, pane_rect, scorebox_frame,
    song_banner_placement, song_info_slide, song_info_text_placement, song_info_text_zoom,
    step_stats_aspect_enabled,
};

#[derive(Clone, Copy)]
struct Rect {
    left: f32,
    right: f32,
}

#[test]
fn judgment_count_anchors_stay_in_the_player_side_pane_at_4_3_and_16_9() {
    assert!(step_stats_aspect_enabled(640.0, 480.0));
    assert!(step_stats_aspect_enabled(1280.0, 720.0));
    assert!(!step_stats_aspect_enabled(720.0, 1280.0));

    for (screen_w, screen_h, notefield_center_x, wide) in
        [(640.0, 480.0, 160.0, false), (1280.0, 720.0, 320.0, true)]
    {
        let screen_center_x = screen_w * 0.5;
        for player_side in [PlayerSide::P1, PlayerSide::P2] {
            let playfield_center_x = if player_side == PlayerSide::P1 {
                notefield_center_x
            } else {
                screen_w - notefield_center_x
            };
            let layout = pane_layout(StepStatsPaneParams {
                screen_w,
                screen_h,
                screen_center_x,
                screen_center_y: screen_h * 0.5,
                playfield_center_x,
                player_side,
                num_players: 1,
                notefield_width: Some(256.0),
                wide,
            });
            let sidepane = pane_rect(layout, screen_h);
            let graph = density_graph_rect(0.0, layout);
            let banner = song_banner_placement(layout, wide, player_side, 1);
            let slide = song_info_slide(layout, player_side, wide, 1, true, 3, graph.y);
            let pack_banner = pack_banner_placement(slide);
            let text = song_info_text_placement(slide, true, song_info_text_zoom(layout));
            let note_field = Rect {
                left: playfield_center_x - 128.0,
                right: playfield_center_x + 128.0,
            };
            let count_center_x = judgment_count_center_x(layout, player_side, wide, 1);

            assert!(count_center_x >= sidepane.left && count_center_x <= sidepane.right);
            assert!(graph.x >= sidepane.left && graph.x + graph.w <= sidepane.right);
            assert!(graph.y + 105.0 <= sidepane.bottom);
            let banner_half_w = STEP_STATS_BANNER_W * banner.zoom * 0.5;
            let banner_half_h = STEP_STATS_BANNER_H * banner.zoom * 0.5;
            assert!(banner.x - banner_half_w >= sidepane.left);
            assert!(banner.x + banner_half_w <= sidepane.right);
            assert!(banner.y - banner_half_h >= sidepane.top);
            assert!(banner.y + banner_half_h <= sidepane.bottom);
            let timing_frame = scorebox_frame(layout, wide, player_side, 1);
            let timing_half_width = SCOREBOX_W * timing_frame.zoom * 0.5;
            let expected_left = sidepane
                .left
                .max(count_center_x.min(timing_frame.center_x - timing_half_width));
            let expected_right = sidepane
                .right
                .min(count_center_x.max(timing_frame.center_x + timing_half_width));
            assert!((slide.left - expected_left).abs() < 0.001);
            assert!((slide.left + slide.width - expected_right).abs() < 0.001);
            assert!(pack_banner.x - STEP_STATS_BANNER_W * pack_banner.zoom * 0.5 >= sidepane.left);
            assert!(
                pack_banner.x + STEP_STATS_BANNER_W * pack_banner.zoom * 0.5
                    <= slide.left + slide.banner_width
            );
            assert!(text.labels_x >= pack_banner.x + STEP_STATS_BANNER_W * pack_banner.zoom * 0.5);
            assert!(text.values_x + text.values_max_width <= expected_right);
            assert!((slide.center_y + slide.height * 0.5 - graph.y).abs() < 0.001);
            assert!(sidepane.right <= note_field.left || sidepane.left >= note_field.right);
            match player_side {
                PlayerSide::P1 => assert!(count_center_x > screen_center_x),
                PlayerSide::P2 => assert!(count_center_x < screen_center_x),
            }
        }
    }
}
