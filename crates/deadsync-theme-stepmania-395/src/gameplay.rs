use deadlib_present::actors::{Actor, TextContent};
use deadsync_core::hud_style::{
    ComboArtworkStyle, FieldFeedbackStyle, HoldFeedbackStyle, TapFeedbackTween,
};
use deadsync_theme::views::{
    GameplayBackgroundWarningView, GameplayDifficultyView, GameplayHoldAtlas, GameplayHud,
    GameplayHudMotion, GameplayLifeView, GameplayScoreView, GameplaySongMeterView,
    GameplayStageBadgeView, GameplayTapAtlas,
};

use crate::presentation::Layout;
use crate::resources::{COMBO_FONT, NORMAL_FONT, PERCENT_FONT, SCORE_FONT};

pub const GAMEPLAY_HUD: GameplayHud<Actor> = GameplayHud {
    standard_life: push_life,
    percentage_score: push_score,
    song_meter: push_frames,
    difficulty: push_difficulty,
    stage_badge: push_stage,
    danger_threshold: 0.2,
    background_warning: push_warning,
    fallback_background: push_background,
    unjoined_seat: push_unjoined,
    autoplay_label: push_autoplay,
    field_feedback: &FEEDBACK,
    default_tap_atlas: GameplayTapAtlas {
        key: "stepmania-395/gameplay-judgments",
        frame_size: [168.0, 28.0],
        frame_cols: 1,
        frame_rows: 6,
        tween: &TapFeedbackTween {
            initial_scale: [1.3; 2],
            settle_s: 0.05,
            shrink_start_s: 0.85,
            end_s: 0.85,
        },
    },
    default_hold_atlas: GameplayHoldAtlas {
        key: "stepmania-395/gameplay-holds",
        frame_size: [62.0, 28.0],
        style: &HoldFeedbackStyle {
            initial_zoom: 1.25,
            final_zoom: 1.0,
            release_rotation_deg: 0.0,
            show_missed_head: false,
        },
    },
};

static FEEDBACK: FieldFeedbackStyle = FieldFeedbackStyle {
    combo_artwork: Some(&ComboArtworkStyle {
        number_font: COMBO_FONT,
        number_offset: [0.0, 20.0],
        number_align: [1.0, 1.0],
        label_texture: "stepmania-395/gameplay-combo",
        label_size: [96.0, 32.0],
        label_offset: [6.0, 20.0],
        label_align: [0.0, 1.0],
        tint: [1.0; 4],
        show_miss_combo: false,
        show_milestones: false,
        shadow_len: 4.0,
    }),
    judgment_y: [-30.0, 30.0],
    judgment_centered_add: [-20.0, 20.0],
    combo_y: [30.0, -30.0],
    combo_centered_add: [40.0, -30.0],
    combo_label: "",
    combo_label_font: NORMAL_FONT,
    combo_label_y: 20.0,
    combo_number_zoom: 0.5,
    combo_number_zoom_step: 0.4 / 300.0,
    combo_number_zoom_limit: 300,
    combo_label_zoom: 1.0,
    combo_pulse_duration: 0.05,
    combo_pulse_scale: 1.2,
};

fn slide(motion: GameplayHudMotion) -> f32 {
    match motion {
        GameplayHudMotion::Settled => 0.0,
        GameplayHudMotion::Entering { elapsed_seconds } => {
            100.0 * (1.0 - (elapsed_seconds - 0.5).clamp(0.0, 1.0))
        }
        GameplayHudMotion::Leaving { elapsed_seconds } => 100.0 * elapsed_seconds.clamp(0.0, 1.0),
    }
}

fn push_life(actors: &mut Vec<Actor>, view: GameplayLifeView) {
    let layout = Layout::current();
    let x = if view.p2 { 500.0 } else { 140.0 };
    let y = 28.0 - slide(view.motion);
    actors.push(layout.solid([x, y], [260.0, 24.0], [0.0, 0.0, 0.0, 1.0], 90));
    if view.level < 0.2 && !view.failed {
        let alpha = 0.4 - 0.4 * (view.song_beat * std::f32::consts::TAU).cos();
        actors.push(layout.solid([x, y], [260.0, 24.0], [1.0, 0.0, 0.0, alpha], 91));
    }
    if !view.failed {
        let key = if view.hot {
            "stepmania-395/gameplay-life-hot"
        } else {
            "stepmania-395/gameplay-life-normal"
        };
        let phase = (view.song_beat * 0.25).rem_euclid(0.5);
        for offset in [-0.5, 0.0, 0.5] {
            let start = (phase + offset).max(0.0);
            let end = (phase + offset + 0.5).min(view.level.clamp(0.0, 1.0));
            if end > start {
                let mut strip = layout.sprite(
                    key,
                    [x - 130.0 + (start + end) * 130.0, y],
                    [(end - start) * 260.0, 24.0],
                    92,
                );
                strip.customtexturerect([
                    (start - phase - offset) * 2.0,
                    0.0,
                    (end - phase - offset) * 2.0,
                    1.0,
                ]);
                actors.push(strip.build(0));
            }
        }
    }
    actors.push(
        layout
            .sprite(
                "stepmania-395/gameplay-life-border",
                [x, y],
                [282.0, 28.0],
                94,
            )
            .build(0),
    );
}

fn push_score(actors: &mut Vec<Actor>, view: GameplayScoreView) {
    let layout = Layout::current();
    let (font, content) = if view.ex {
        (
            PERCENT_FONT,
            TextContent::inline_format(format_args!("{:.2}%", view.percent.max(0.0))),
        )
    } else {
        (
            SCORE_FONT,
            TextContent::inline_format(format_args!("{:09}", view.numeric_score)),
        )
    };
    let Some(content) = content else { return };
    let mut text = layout.text(
        font,
        content,
        [
            if view.p2 { 534.0 } else { 106.0 },
            444.0 + slide(view.motion),
        ],
        1.0,
    );
    let tint = if view.ex {
        view.rgba
    } else if view.p2 {
        [1.0, 0.5, 0.2, view.rgba[3]]
    } else {
        [0.4, 1.0, 0.8, view.rgba[3]]
    };
    text.diffuse(tint);
    text.maxwidth(200.0 * layout.scale);
    text.z(96);
    actors.push(text.build(0));
}

fn push_frames(actors: &mut Vec<Actor>, view: GameplaySongMeterView<'_>) {
    let layout = Layout::current();
    let shift = slide(view.motion);
    actors.push(
        layout
            .sprite(
                "stepmania-395/gameplay-life-frame",
                [320.0, 36.0 - shift],
                [640.0, 76.0],
                93,
            )
            .build(0),
    );
    actors.push(
        layout
            .sprite(
                "stepmania-395/gameplay-score-frame",
                [320.0, 448.0 + shift],
                [640.0, 64.0],
                93,
            )
            .build(0),
    );
    if let GameplayHudMotion::Entering { elapsed_seconds } = view.motion {
        crate::gameplay_transition::push_entry(actors, elapsed_seconds);
    }
}

pub(crate) fn difficulty_index(difficulty: &str) -> usize {
    ["Beginner", "Easy", "Medium", "Hard", "Challenge", "Edit"]
        .iter()
        .position(|name| name.eq_ignore_ascii_case(difficulty))
        .unwrap_or(5)
}

fn push_difficulty(actors: &mut Vec<Actor>, view: GameplayDifficultyView<'_>) {
    let layout = Layout::current();
    let row = difficulty_index(view.difficulty);
    let shift = slide(view.motion) * 2.0 * if view.p2 { 1.0 } else { -1.0 };
    let x = if view.p2 { 580.0 } else { 60.0 };
    let y = if view.reverse { 58.0 } else { 410.0 };
    let mut icon = layout.sprite(
        "stepmania-395/gameplay-difficulty",
        [x + shift, y],
        [122.0, 24.0],
        95,
    );
    let col = if view.p2 { 0.5 } else { 0.0 };
    icon.customtexturerect([col, row as f32 / 6.0, col + 0.5, (row + 1) as f32 / 6.0]);
    actors.push(icon.build(0));
}

pub(crate) fn stage_artwork(number: usize) -> (&'static str, f32) {
    match number {
        1 => ("stepmania-395/music-stage-1", 48.0),
        2 => ("stepmania-395/music-stage-2", 54.0),
        3 => ("stepmania-395/music-stage-3", 54.0),
        4 => ("stepmania-395/music-stage-4", 54.0),
        5 => ("stepmania-395/music-stage-5", 54.0),
        _ => ("stepmania-395/music-stage-6", 54.0),
    }
}

fn push_stage(actors: &mut Vec<Actor>, view: GameplayStageBadgeView<'_>) {
    let (key, width) = if let Some(round) = view.round {
        stage_artwork(round)
    } else if view.text.as_ref() == "EVENT" {
        ("stepmania-395/music-stage-event", 88.0)
    } else if view.text.as_ref() == "FINAL STAGE" {
        ("stepmania-395/music-stage-final", 64.0)
    } else {
        return;
    };
    actors.push(
        Layout::current()
            .sprite(key, [320.0, 54.0 - slide(view.motion)], [width, 16.0], 95)
            .build(0),
    );
}

fn push_warning(_actors: &mut Vec<Actor>, _view: GameplayBackgroundWarningView) {}
fn push_unjoined(_actors: &mut Vec<Actor>, _p2: bool) {}
fn push_background(actors: &mut Vec<Actor>, _brightness: f32) {
    actors.push(Layout::current().solid(
        [320.0, 240.0],
        [640.0, 480.0],
        [0.0, 0.0, 0.0, 1.0],
        -100,
    ));
}
fn push_autoplay(actors: &mut Vec<Actor>) {
    let mut label = Layout::current().text(NORMAL_FONT, "AutoPlay".into(), [320.0, 360.0], 1.0);
    label.z(110);
    actors.push(label.build(0));
}
