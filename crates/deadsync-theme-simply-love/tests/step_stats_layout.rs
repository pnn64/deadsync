use deadsync_profile::PlayerSide;
use deadsync_theme_simply_love::step_stats::{
    StepStatsPaneParams, density_graph_rect, judgment_count_center_x, pane_layout, pane_rect,
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
            let note_field = Rect {
                left: playfield_center_x - 128.0,
                right: playfield_center_x + 128.0,
            };
            let count_center_x = judgment_count_center_x(layout, player_side, wide, 1);

            assert!(count_center_x >= sidepane.left && count_center_x <= sidepane.right);
            assert!(graph.x >= sidepane.left && graph.x + graph.w <= sidepane.right);
            assert!(graph.y + 105.0 <= sidepane.bottom);
            assert!(sidepane.right <= note_field.left || sidepane.left >= note_field.right);
            match player_side {
                PlayerSide::P1 => assert!(count_center_x > screen_center_x),
                PlayerSide::P2 => assert!(count_center_x < screen_center_x),
            }
        }
    }
}
