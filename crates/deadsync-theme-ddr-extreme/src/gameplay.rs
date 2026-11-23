use deadlib_present::actors::{Actor, TextContent};
use deadsync_theme::stage::StageOutroPresentation;
use deadsync_theme::views::{
    GameplayBackgroundWarningView, GameplayDifficultyView, GameplayHoldAtlas, GameplayHud,
    GameplayHudMotion, GameplayLifeView, GameplayScoreView, GameplaySongMeterView,
    GameplayStageBadgeView, GameplayTapAtlas,
};
use deadsync_theme::{FieldFeedbackStyle, HoldFeedbackStyle, TapFeedbackTween, UiCue};

use crate::resources::FONT;
use crate::scene::{AMBER, MINT, MUTED, PAPER, PITCH, Scene};
use crate::stage::number_text;

const DANGER: [f32; 3] = [1.0, 0.33, 0.38];
const HUD_PANEL: [f32; 3] = [0.055, 0.12, 0.17];
const OUTRO_FADE_SECONDS: f32 = 0.32;

pub(crate) const HUD: GameplayHud<Actor> = GameplayHud {
    standard_life: push_life,
    percentage_score: push_score,
    song_meter: push_song_meter,
    difficulty: push_difficulty,
    stage_badge: push_stage_badge,
    danger_threshold: 0.25,
    background_warning: push_warning,
    fallback_background: push_backdrop,
    unjoined_seat: push_unjoined_seat,
    autoplay_label: push_autoplay,
    field_feedback: &FieldFeedbackStyle {
        judgment_y: [-28.0, 28.0],
        judgment_centered_add: [-8.0, 8.0],
        combo_y: [37.0, -34.0],
        combo_centered_add: [14.0, -15.0],
        combo_label: "COMBO",
        combo_label_font: FONT,
        combo_label_y: 15.0,
        combo_number_zoom: 0.83,
        combo_number_zoom_step: 0.0008,
        combo_number_zoom_limit: 180,
        combo_label_zoom: 0.38,
        combo_pulse_duration: 0.12,
        combo_pulse_scale: 1.14,
    },
    default_tap_atlas: GameplayTapAtlas {
        key: "ddr-extreme/tap-results",
        frame_size: [170.0, 34.0],
        frame_cols: 2,
        frame_rows: 7,
        tween: &TapFeedbackTween {
            initial_scale: [1.19, 1.34],
            settle_s: 0.12,
            shrink_start_s: 0.58,
            end_s: 0.8,
        },
    },
    default_hold_atlas: GameplayHoldAtlas {
        key: "ddr-extreme/hold-results",
        frame_size: [56.0, 24.0],
        style: &HoldFeedbackStyle {
            initial_zoom: 1.18,
            final_zoom: 1.0,
            release_rotation_deg: 0.0,
            show_missed_head: false,
        },
    },
};

fn seat_color(p2: bool) -> [f32; 3] {
    if p2 { AMBER } else { MINT }
}

fn hud_alpha(motion: GameplayHudMotion) -> f32 {
    match motion {
        GameplayHudMotion::Settled => 1.0,
        GameplayHudMotion::Entering { elapsed_seconds } => elapsed_seconds / 0.33,
        GameplayHudMotion::Leaving { elapsed_seconds } => {
            1.0 - elapsed_seconds / OUTRO_FADE_SECONDS
        }
    }
    .clamp(0.0, 1.0)
}

fn push_life(actors: &mut Vec<Actor>, view: GameplayLifeView) {
    let scene = Scene::new(hud_alpha(view.motion));
    let x = if view.p2 { 389.0 } else { 26.0 };
    let low = view.failed || view.level < HUD.danger_threshold;
    let accent = if low { DANGER } else { seat_color(view.p2) };
    let width = 225.0;
    scene.rect(actors, [x, 11.0], [width, 24.0], HUD_PANEL, 0.95, 88);
    scene.rect(actors, [x, 11.0], [width, 1.0], accent, 0.7, 89);
    let player = if view.p2 { "P2" } else { "P1" };
    push_label(
        scene,
        actors,
        [x + 19.0, 23.0],
        [0.54, 31.0],
        player,
        accent,
        92,
    );
    let filled = if view.failed {
        0.0
    } else {
        view.level.clamp(0.0, 1.0) * 152.0
    };
    let start = x + 40.0;
    let pulse = if view.hot {
        0.83 + view.song_beat.sin() * 0.14
    } else {
        0.92
    };
    for index in 0..8 {
        let offset = index as f32 * 19.0;
        let x = start + offset;
        scene.rect(actors, [x, 18.0], [17.0, 10.0], MUTED, 0.22, 89);
        let section = (filled - offset).clamp(0.0, 17.0);
        if section > 0.0 {
            scene.rect(actors, [x, 18.0], [section, 10.0], accent, pulse, 90);
            scene.rect(actors, [x, 18.0], [section, 2.0], PAPER, 0.56, 91);
        }
    }
    if let Some(label) = view.percent_label {
        push_label(
            scene,
            actors,
            [x + 207.0, 23.0],
            [0.34, 31.0],
            number_text(format_args!("{label:.0}%")),
            PAPER,
            92,
        );
    }
}

fn push_score(actors: &mut Vec<Actor>, view: GameplayScoreView) {
    let scene = Scene::new(hud_alpha(view.motion) * view.rgba[3]);
    let x = if view.p2 { 501.0 } else { 138.0 };
    let ink = if view.ex {
        [view.rgba[0], view.rgba[1], view.rgba[2]]
    } else {
        seat_color(view.p2)
    };
    let mut score = scene.text_builder(
        [x, 53.0],
        1.05,
        number_text(format_args!("{:.2}%", view.percent)),
        ink,
    );
    score.align(0.5, 0.5);
    score.maxwidth(163.0);
    score.z(92);
    actors.push(score.build(0));
}

fn push_song_meter(actors: &mut Vec<Actor>, view: GameplaySongMeterView<'_>) {
    let scene = Scene::new(hud_alpha(view.motion));
    scene.rect(actors, [205.0, 441.0], [230.0, 31.0], HUD_PANEL, 0.94, 86);
    push_label(
        scene,
        actors,
        [320.0, 451.0],
        [0.47, 208.0],
        view.title.clone(),
        PAPER,
        90,
    );
    scene.rect(actors, [215.0, 462.0], [210.0, 3.0], MUTED, 0.32, 89);
    scene.rect(
        actors,
        [215.0, 462.0],
        [210.0 * view.progress.clamp(0.0, 1.0), 3.0],
        MINT,
        1.0,
        90,
    );
    if !view.rate.is_empty() {
        push_label(
            scene,
            actors,
            [320.0, 429.0],
            [0.4, 145.0],
            view.rate.clone(),
            PAPER,
            90,
        );
    }
}

fn push_difficulty(actors: &mut Vec<Actor>, view: GameplayDifficultyView<'_>) {
    let x = if view.p2 { 447.0 } else { 26.0 };
    let scene = Scene::new(hud_alpha(view.motion));
    let accent = seat_color(view.p2);
    scene.rect(actors, [x, 441.0], [167.0, 31.0], HUD_PANEL, 0.94, 86);
    scene.rect(actors, [x, 441.0], [2.0, 31.0], accent, 0.92, 87);
    push_label(
        scene,
        actors,
        [x + 29.0, 457.0],
        [0.78, 38.0],
        number_text(format_args!("{:02}", view.meter)),
        accent,
        90,
    );
    if view.difficulty.eq_ignore_ascii_case("edit") && !view.description.is_empty() {
        push_label(
            scene,
            actors,
            [x + 104.0, 457.0],
            [0.51, 113.0],
            TextContent::from(view.description.to_owned()),
            PAPER,
            90,
        );
    } else {
        push_label(
            scene,
            actors,
            [x + 104.0, 457.0],
            [0.56, 113.0],
            number_text(format_args!("{}", view.difficulty)),
            PAPER,
            90,
        );
    }
}

fn push_stage_badge(actors: &mut Vec<Actor>, view: GameplayStageBadgeView<'_>) {
    let GameplayHudMotion::Entering { elapsed_seconds } = view.motion else {
        return;
    };
    let alpha = ((1.22 - elapsed_seconds) / 0.29).clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let scene = Scene::new(alpha);
    scene.rect(actors, [168.0, 199.0], [304.0, 74.0], PITCH, 0.98, 250);
    scene.rect(actors, [168.0, 199.0], [304.0, 2.0], MINT, 1.0, 251);
    let title = view.round.map_or_else(
        || TextContent::from(view.text.clone()),
        |round| number_text(format_args!("ROUND {round:02}")),
    );
    push_label(
        scene,
        actors,
        [320.0, 225.0],
        [1.05, 278.0],
        title,
        PAPER,
        253,
    );
    push_label(
        scene,
        actors,
        [320.0, 252.0],
        [0.47, 225.0],
        "READY",
        MINT,
        253,
    );
}

fn push_warning(actors: &mut Vec<Actor>, view: GameplayBackgroundWarningView) {
    let center = if view.p2 { 470.0 } else { 170.0 };
    let scene = Scene::new(0.45 + view.danger_seconds.sin().abs() * 0.25);
    scene.rect(
        actors,
        [center - 81.0, 301.0],
        [162.0, 25.0],
        DANGER,
        0.23,
        -83,
    );
    let caption = if view.failed {
        "LIFE EMPTY"
    } else {
        "LOW LIFE"
    };
    push_label(
        scene,
        actors,
        [center, 314.0],
        [0.54, 154.0],
        caption,
        DANGER,
        -81,
    );
}

fn push_backdrop(actors: &mut Vec<Actor>, brightness: f32) {
    let scene = Scene::new(brightness);
    scene.fill_screen(actors, PITCH, -99);
    for y in [84.0, 164.0, 244.0, 324.0, 404.0] {
        scene.rect(actors, [0.0, y], [640.0, 1.0], MUTED, 0.12, -98);
    }
}

fn push_unjoined_seat(actors: &mut Vec<Actor>, p2: bool) {
    let x = if p2 { 501.0 } else { 138.0 };
    push_label(
        Scene::new(0.56),
        actors,
        [x, 23.0],
        [0.42, 135.0],
        "STANDBY",
        MUTED,
        90,
    );
}

fn push_autoplay(actors: &mut Vec<Actor>) {
    push_label(
        Scene::new(1.0),
        actors,
        [320.0, 23.0],
        [0.41, 117.0],
        "AUTOPLAY",
        AMBER,
        90,
    );
}

fn push_label(
    scene: Scene,
    actors: &mut Vec<Actor>,
    at: [f32; 2],
    [zoom, width]: [f32; 2],
    text: impl Into<TextContent>,
    ink: [f32; 3],
    layer: i16,
) {
    let mut label = scene.text_builder(at, zoom, text.into(), ink);
    label.align(0.5, 0.5);
    label.maxwidth(width);
    label.z(layer);
    actors.push(label.build(0));
}

pub(crate) struct Outro {
    pub failed: bool,
}

impl StageOutroPresentation<Actor> for Outro {
    fn duration(&self) -> f32 {
        if self.failed { 1.3 } else { 0.92 }
    }

    fn cue(&self) -> Option<UiCue> {
        self.failed.then_some(UiCue::StageFailed)
    }

    fn covers_stage(&self, elapsed: f32) -> bool {
        elapsed >= OUTRO_FADE_SECONDS
    }

    fn stop_tail_audio_when_frozen(&self) -> bool {
        self.failed
    }

    fn push_actors(&self, actors: &mut Vec<Actor>, elapsed: f32) {
        let progress = (elapsed / OUTRO_FADE_SECONDS).clamp(0.0, 1.0);
        let scene = Scene::new(progress);
        scene.fill_screen(actors, PITCH, 900);
        let accent = if self.failed { DANGER } else { MINT };
        let rule = 510.0 * progress;
        for y in [192.0, 287.0] {
            scene.rect(
                actors,
                [320.0 - rule * 0.5, y],
                [rule, 2.0],
                accent,
                1.0,
                901,
            );
        }
        let (heading, state) = if self.failed {
            ("STAGE FAILED", "LIFE EMPTY")
        } else {
            ("STAGE CLEAR", "PLAY COMPLETE")
        };
        push_label(
            scene,
            actors,
            [320.0, 231.0],
            [1.57, 495.0],
            heading,
            PAPER,
            902,
        );
        push_label(
            scene,
            actors,
            [320.0, 264.0],
            [0.52, 300.0],
            state,
            accent,
            902,
        );
    }
}
