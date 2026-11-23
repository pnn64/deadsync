use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_height, screen_width};
use deadsync_core::hud_style::{FieldFeedbackStyle, TapFeedbackTween};
use deadsync_theme::views::{
    GameplayHud, GameplayHudMotion, GameplayLifeView, GameplayScoreView, GameplayTapAtlas,
};

use crate::presentation::{Layout, WIDTH};

pub const GAMEPLAY_HUD: GameplayHud<Actor> = GameplayHud {
    standard_life: push_life_bar,
    percentage_score: push_score,
    field_feedback: &FIELD_FEEDBACK,
    default_tap_atlas: GameplayTapAtlas {
        key: "openitg/tap-results",
        frame_size: [128.0, 38.0],
        frame_cols: 2,
        frame_rows: 7,
        tween: &TapFeedbackTween {
            initial_scale: [1.3, 1.7],
            settle_s: 0.1,
            shrink_start_s: 1.1,
            end_s: 1.3,
        },
    },
};

static FIELD_FEEDBACK: FieldFeedbackStyle = FieldFeedbackStyle {
    judgment_y: [-30.0, 30.0],
    judgment_centered_add: [-20.0, 20.0],
    combo_y: [50.0, 0.0],
    combo_centered_add: [40.0, -30.0],
    combo_label: "COMBO",
    combo_label_font: "miso",
    combo_label_y: 2.0,
    combo_number_zoom: 0.9,
    combo_number_zoom_step: 0.001,
    combo_number_zoom_limit: 400,
    combo_label_zoom: 0.65,
    combo_pulse_duration: 0.05,
    combo_pulse_scale: 1.2,
};

fn slide_offset(motion: GameplayHudMotion) -> f32 {
    const DISTANCE: f32 = 100.0;
    const ENTER_DELAY: f32 = 0.5;
    const SLIDE_SECONDS: f32 = 0.8;
    match motion {
        GameplayHudMotion::Settled => 0.0,
        GameplayHudMotion::Entering { elapsed_seconds } => {
            let remaining = 1.0 - ((elapsed_seconds - ENTER_DELAY) / SLIDE_SECONDS).clamp(0.0, 1.0);
            DISTANCE * remaining * remaining
        }
        GameplayHudMotion::Leaving { elapsed_seconds } => {
            let progress = (elapsed_seconds / SLIDE_SECONDS).clamp(0.0, 1.0);
            DISTANCE * progress * progress
        }
    }
}

fn push_life_bar(actors: &mut Vec<Actor>, life: GameplayLifeView) {
    let width = screen_width();
    let canvas = Layout::centered(width, screen_height());
    let slide = slide_offset(life.motion);
    // ScreenGameplay anchors each bar to the outside edge of its player's side.
    let (screen_edge, canvas_edge, x) = if life.p2 {
        (width, WIDTH, WIDTH - 24.0 + slide)
    } else {
        (0.0, 0.0, 24.0 - slide)
    };
    let layout = canvas.translated_x(screen_edge - canvas.position(canvas_edge, 0.0)[0]);
    let frame = if life.failed {
        [0.39, 0.23, 0.26, 1.0]
    } else {
        [0.56, 0.65, 0.72, 1.0]
    };
    // The reference frame has a transparent 16 px tip in its rotated orientation.
    actors.push(layout.panel(x - 20.0, 129.0, [40.0, 298.0], frame, 90));
    actors.push(layout.panel(x - 17.0, 132.0, [34.0, 292.0], [0.07, 0.11, 0.16, 1.0], 91));
    actors.push(layout.panel(x - 9.0, 134.0, [18.0, 272.0], [0.12, 0.18, 0.23, 1.0], 92));

    if !life.failed && life.level > 0.0 {
        let height = 272.0 * life.level.min(1.0);
        let fill = if life.hot {
            [1.0, 0.31, 0.11, 1.0]
        } else {
            [0.17, 0.77, 0.87, 1.0]
        };
        actors.push(layout.panel(x - 9.0, 406.0 - height, [18.0, height], fill, 93));
        actors.push(layout.panel(
            x - 6.0,
            406.0 - height,
            [3.0, height],
            [1.0, 1.0, 1.0, 0.3],
            94,
        ));
    }

    if let Some(percent) = life.percent_label {
        let text = TextContent::inline_format(format_args!("{percent:.1}%"));
        if let Some(text) = text {
            let mut label = layout.text([x, 415.0], 0.5, text, 0.6, [0.92, 0.97, 1.0, 1.0]);
            label.z(94);
            actors.push(label.build(0));
        }
    }
}

fn push_score(actors: &mut Vec<Actor>, score: GameplayScoreView) {
    let width = screen_width();
    let (shift, offset) = match (score.p2, score.doubles) {
        (false, false) => (-0.25 * width, -20.0),
        (false, true) => (0.0, -140.0),
        (true, false) => (0.25 * width, 18.0),
        (true, true) => (0.0, 140.0),
    };
    let layout = Layout::centered(width, screen_height()).translated_x(shift);
    let rgba = if score.ex {
        score.rgba
    } else if score.p2 {
        [0.40, 0.96, 0.37, score.rgba[3]]
    } else {
        [0.98, 0.75, 0.10, score.rgba[3]]
    };
    let centi = (score.percent.max(0.0) * 100.0) as u32;
    let whole = centi / 100;
    let fraction = centi % 100;
    if let Some(text) = TextContent::inline_format(format_args!("{whole}.{fraction:02}%")) {
        let y = 56.0 - slide_offset(score.motion);
        let mut label = layout.text([WIDTH * 0.5 + offset, y], 0.5, text, 1.1, rgba);
        label.z(90);
        actors.push(label.build(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::SizeSpec;
    use deadlib_present::space::{Metrics, with_current_metrics};

    #[test]
    fn standard_life_anchors_to_screen_edges_and_fills_from_the_bottom() {
        for width in [640.0, 854.0, 1138.0] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                for (p2, x) in [(false, 24.0), (true, width - 24.0)] {
                    let life = GameplayLifeView {
                        p2,
                        level: 0.25,
                        failed: false,
                        hot: false,
                        percent_label: Some(25.0),
                        motion: GameplayHudMotion::Settled,
                    };
                    let mut actors = Vec::new();
                    push_life_bar(&mut actors, life);
                    assert!(matches!(
                        &actors[0],
                        Actor::Sprite {
                            offset,
                            size: [SizeSpec::Px(40.0), SizeSpec::Px(298.0)],
                            ..
                        } if *offset == [x - 20.0, 129.0]
                    ));
                    assert!(matches!(
                        &actors[3],
                        Actor::Sprite {
                            offset,
                            size: [SizeSpec::Px(18.0), SizeSpec::Px(68.0)],
                            ..
                        } if *offset == [x - 9.0, 338.0]
                    ));
                    assert!(matches!(
                        actors.last(),
                        Some(Actor::Text { offset, content, .. })
                            if *offset == [x, 415.0] && content.as_str() == "25.0%"
                    ));

                    actors.clear();
                    push_life_bar(
                        &mut actors,
                        GameplayLifeView {
                            failed: true,
                            percent_label: None,
                            ..life
                        },
                    );
                    assert_eq!(actors.len(), 3);
                }
            });
        }
    }

    #[test]
    fn percentage_follows_screen_width_and_keeps_stage_score_and_ex_tint() {
        for (width, p2, doubles, x) in [
            (640.0, false, false, 140.0),
            (640.0, true, false, 498.0),
            (854.0, false, false, 193.5),
            (854.0, true, false, 658.5),
            (854.0, false, true, 287.0),
            (854.0, true, true, 567.0),
        ] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                let mut actors = Vec::new();
                let score = GameplayScoreView {
                    p2,
                    doubles,
                    percent: 98.7899,
                    rgba: [0.2, 0.5, 1.0, 0.65],
                    ex: true,
                    motion: GameplayHudMotion::Settled,
                };
                push_score(&mut actors, score);
                assert!(matches!(
                    actors.as_slice(),
                    [Actor::Text { offset, content, color, .. }]
                        if *offset == [x, 56.0] && *color == score.rgba && content.as_str() == "98.78%"
                ));
            });
        }
    }

    #[test]
    fn life_and_score_follow_the_same_entry_and_exit_clock() {
        use GameplayHudMotion::{Entering, Leaving};

        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            for (motion, shift) in [
                (
                    Entering {
                        elapsed_seconds: 0.0,
                    },
                    100.0,
                ),
                (
                    Entering {
                        elapsed_seconds: 0.5,
                    },
                    100.0,
                ),
                (
                    Entering {
                        elapsed_seconds: 0.7,
                    },
                    56.25,
                ),
                (
                    Entering {
                        elapsed_seconds: 1.3,
                    },
                    0.0,
                ),
                (
                    Leaving {
                        elapsed_seconds: 0.2,
                    },
                    6.25,
                ),
                (
                    Leaving {
                        elapsed_seconds: 0.8,
                    },
                    100.0,
                ),
            ] {
                for (p2, x, direction) in [(false, 4.0, -1.0), (true, 596.0, 1.0)] {
                    let mut actors = Vec::new();
                    push_life_bar(
                        &mut actors,
                        GameplayLifeView {
                            p2,
                            level: 0.5,
                            failed: false,
                            hot: false,
                            percent_label: None,
                            motion,
                        },
                    );
                    push_score(
                        &mut actors,
                        GameplayScoreView {
                            p2,
                            doubles: false,
                            percent: 50.0,
                            rgba: [1.0; 4],
                            ex: false,
                            motion,
                        },
                    );
                    assert!(matches!(
                        &actors[0],
                        Actor::Sprite { offset, .. }
                            if (offset[0] - (x + direction * shift)).abs() < 0.001
                    ));
                    assert!(matches!(
                        actors.last(),
                        Some(Actor::Text { offset, .. })
                            if (offset[1] - (56.0 - shift)).abs() < 0.001
                    ));
                }
            }
        });
    }
}
