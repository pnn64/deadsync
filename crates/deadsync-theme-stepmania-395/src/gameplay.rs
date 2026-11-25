use std::sync::Arc;

use deadlib_present::actors::{Actor, TextContent};
use deadsync_assets::gameplay_presentation::{
    GameplayPresentationInit,
    standard::{GameplayHudLabel, StandardGameplay, StandardGameplaySettings, StandardGameplayStyle},
};
use deadsync_assets::gameplay_stage::{ActiveGameplayStage, GameplayStageAssets};
use deadsync_core::hud_style::{
    ComboArtworkStyle, FieldFeedbackStyle, HoldFeedbackStyle, TapFeedbackTween,
};
use deadsync_gameplay::HoldToExitKey;
use deadsync_theme::views::{
    GameplayBackgroundWarningView, GameplayDifficultyView, GameplayHoldAtlas, GameplayHud,
    GameplayHudMotion, GameplayLifeView, GameplayScoreView, GameplaySongMeterView,
    GameplayStageBadgeView, GameplayStageIntro, GameplayStageView, GameplayTapAtlas,
};

use crate::presentation::Layout;
use crate::resources::{COMBO_FONT, NORMAL_FONT, PERCENT_FONT, SCORE_FONT};

impl crate::StepManiaTheme {
    pub fn mount_gameplay<L>(
        stage: &ActiveGameplayStage,
        mut assets: GameplayStageAssets,
        view: &GameplayStageView,
        settings: StandardGameplaySettings,
        init: Option<GameplayPresentationInit>,
        lobby_code: fn(&L) -> Option<&str>,
    ) -> StandardGameplay<L> {
        let (round, badge) = match view.intro {
            GameplayStageIntro::Song { round, ref event_name } => (
                round,
                event_name.clone().unwrap_or_else(|| Arc::from("EVENT")),
            ),
            _ => (None, Arc::from("")),
        };
        let replay_status = view.replay.as_ref().map(|replay| {
            Arc::from(format!(
                "Autoplay - {} {:.2}%",
                replay.name,
                replay.score_10000 / 100.0
            ))
        });
        if let Some(course) = &view.course {
            assets.pack_banner_path.clone_from(&course.banner_path);
        }
        StandardGameplay::new(
            stage,
            assets,
            settings,
            init,
            StandardGameplayStyle {
                hud: GAMEPLAY_HUD,
                badge,
                round,
                replay_status,
                push_label,
                lobby_code,
            },
        )
    }
}

fn push_label(actors: &mut Vec<Actor>, label: GameplayHudLabel<'_>) {
    let (at, text) = match label {
        GameplayHudLabel::ExitPrompt(key) => (
            [320.0, 364.0],
            TextContent::Static(match key {
                HoldToExitKey::Start => "Continue holding START to give up",
                HoldToExitKey::Back => "Continue holding BACK to give up",
            }),
        ),
        GameplayHudLabel::Replay(status) => {
            ([320.0, 360.0], TextContent::Shared(Arc::clone(status)))
        }
        GameplayHudLabel::Profile { .. } => return,
        GameplayHudLabel::LobbyCode(code) => {
            let Some(text) = TextContent::inline_str(code) else {
                return;
            };
            ([320.0, 28.0], text)
        }
    };
    let mut label = Layout::current().text(NORMAL_FONT, text, at, 0.7);
    label.z(1000);
    actors.push(label.build(0));
}

// Player fields occupy depths through 1050 on scripted songs. Keep the
// cabinet artwork above them and below song foreground artwork at 1100.
const HUD_Z: i16 = 1060;

pub const GAMEPLAY_HUD: GameplayHud<Actor> = GameplayHud {
    standard_life: push_life,
    percentage_score: push_score,
    song_meter: push_frames,
    difficulty: push_difficulty,
    stage_badge: push_stage,
    danger_threshold: 0.2,
    warning_scope: deadsync_theme::views::GameplayWarningScope::AllPlayers,
    background_warning: push_warning,
    fallback_background: push_background,
    unjoined_seat: push_unjoined,
    profile_footer: Some(push_profile_footer),
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
    let x = if view.p2 { 500.0 } else { 140.0 };
    let y = 28.0 - slide(view.motion);
    push_life_at(actors, view, [x, y]);
}

pub(crate) fn push_life_at(actors: &mut Vec<Actor>, view: GameplayLifeView, [x, y]: [f32; 2]) {
    let layout = Layout::current();
    actors.push(layout.solid([x, y], [260.0, 24.0], [0.0, 0.0, 0.0, 1.0], HUD_Z));
    if view.level < 0.2 && !view.failed {
        let alpha = 0.4 - 0.4 * (view.song_beat * std::f32::consts::TAU).cos();
        actors.push(layout.solid([x, y], [260.0, 24.0], [1.0, 0.0, 0.0, alpha], HUD_Z + 1));
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
                    HUD_Z + 2,
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
                HUD_Z + 4,
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
    text.z(HUD_Z + 6);
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
                HUD_Z + 3,
            )
            .build(0),
    );
    actors.push(
        layout
            .sprite(
                "stepmania-395/gameplay-score-frame",
                [320.0, 448.0 + shift],
                [640.0, 64.0],
                HUD_Z + 3,
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
        HUD_Z + 5,
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
            .sprite(
                key,
                [320.0, 54.0 - slide(view.motion)],
                [width, 16.0],
                HUD_Z + 5,
            )
            .build(0),
    );
}

fn push_warning(actors: &mut Vec<Actor>, view: GameplayBackgroundWarningView) {
    if view.failed || !view.both_sides || view.danger_seconds.rem_euclid(1.0) >= 0.5 {
        return;
    }
    let layout = Layout::current();
    let elapsed = view.danger_seconds.max(0.0);
    // The reference updates this animation only during the visible half of each blink.
    let time = elapsed.floor() * 0.5 + elapsed.fract();
    let brightness = view.brightness.clamp(0.0, 1.0);
    let frames = [0, 2, 4, 6, 1, 3, 5, 7, 5, 3, 1, 6, 4, 2];
    let frame = frames[(time / 0.05) as usize % frames.len()];
    let col = (frame % 2) as f32 * 0.5;
    let row = (frame / 2) as f32 * 0.25;
    for y in 0..5 {
        for x in 0..6 {
            let center = [
                20.0 + x as f32 * 120.0,
                20.0 + y as f32 * 128.0 * 440.0 / 480.0,
            ];
            let size = [120.0, 128.0 * 440.0 / 480.0];
            let mut tile = layout.sprite("stepmania-395/gameplay-danger-tiles", center, size, -99);
            tile.customtexturerect([col, row, col + 0.5, row + 0.25]);
            tile.cropleft(((20.0 - center[0]) / size[0] + 0.5).max(0.0));
            tile.cropright(((center[0] - 620.0) / size[0] + 0.5).max(0.0));
            tile.croptop(((20.0 - center[1]) / size[1] + 0.5).max(0.0));
            tile.cropbottom(((center[1] - 460.0) / size[1] + 0.5).max(0.0));
            tile.diffuse([brightness, brightness, brightness, 1.0]);
            actors.push(tile.build(0));
        }
    }
    let phase = (time * std::f32::consts::PI).cos() * 0.5 + 0.5;
    let mut glow = layout.sprite(
        "stepmania-395/gameplay-danger-glow",
        [320.0, 240.0],
        [480.0, 128.0 * 440.0 / 480.0],
        -98,
    );
    glow.diffuse([brightness, brightness, brightness, phase]);
    actors.push(glow.build(0));
    let mut text = layout.sprite(
        "stepmania-395/gameplay-danger-text",
        [320.0, 240.0],
        [480.0, 128.0 * 440.0 / 480.0],
        -97,
    );
    let rainbow = std::array::from_fn::<_, 3, _>(|index| {
        brightness
            * ((phase * std::f32::consts::TAU + index as f32 * std::f32::consts::TAU / 3.0).cos()
                * 0.5
                + 0.5)
    });
    text.diffuse([rainbow[0], rainbow[1], rainbow[2], 1.0]);
    actors.push(text.build(0));
}
fn push_unjoined(_actors: &mut Vec<Actor>, _p2: bool) {}
fn push_profile_footer(
    actors: &mut Vec<Actor>,
    view: deadsync_theme::views::GameplayProfileFooterView<'_>,
) {
    let layout = Layout::current();
    for (index, name) in view.player_names.into_iter().enumerate() {
        let Some(name) = name.filter(|name| !name.is_empty()) else {
            continue;
        };
        let content =
            TextContent::inline_str(name).unwrap_or_else(|| TextContent::Owned(name.to_owned()));
        let mut label = layout.text(
            crate::resources::CREDITS_FONT,
            content,
            [if index == 0 { 120.0 } else { 520.0 }, 474.0],
            1.0,
        );
        label.shadowlength(0.0);
        label.z(2101);
        actors.push(label.build(0));
    }
}
fn push_background(actors: &mut Vec<Actor>, brightness: f32) {
    actors.push(Layout::current().solid(
        [320.0, 240.0],
        [640.0, 480.0],
        [0.0, 0.0, 0.0, 1.0],
        -101,
    ));
    let brightness = brightness.clamp(0.0, 1.0);
    let mut image = Layout::current().sprite(
        "stepmania-395/gameplay-fallback",
        [320.0, 240.0],
        [600.0, 440.0],
        -100,
    );
    image.diffuse([brightness, brightness, brightness, 1.0]);
    actors.push(image.build(0));
}
fn push_autoplay(actors: &mut Vec<Actor>) {
    let mut label = Layout::current().text(NORMAL_FONT, "AutoPlay".into(), [320.0, 360.0], 1.0);
    label.z(HUD_Z + 20);
    actors.push(label.build(0));
}
