use std::sync::{Arc, OnceLock};

use deadlib_present::actors::{Actor, InlineText, TextContent};
use deadlib_present::dsl::SpriteBuilder;
use deadlib_present::color::rgba_hex;
use deadlib_present::space::{screen_center_x, screen_height, screen_width};
use deadsync_core::hud_style::{FieldFeedbackStyle, HoldFeedbackStyle, TapFeedbackTween};
use deadsync_theme::views::{
    GameplayBackgroundWarningView, GameplayDifficultyView, GameplayHoldAtlas, GameplayHud,
    GameplayHudMotion, GameplayLifeView, GameplayScoreView, GameplaySongMeterView,
    GameplayStageBadgeView, GameplayTapAtlas,
};

use crate::presentation::{Layout, WIDTH, sprite};
use crate::resources::{GAMEPLAY_SCORE_FONT, MENU_NORMAL_FONT};

pub const GAMEPLAY_HUD: GameplayHud<Actor> = GameplayHud {
    standard_life: push_life_bar,
    percentage_score: push_score,
    song_meter: push_song_meter,
    difficulty: push_difficulty,
    stage_badge: push_stage_badge,
    danger_threshold: 0.3,
    background_warning: push_background_warning,
    fallback_background: push_fallback_background,
    unjoined_seat: push_unjoined_seat,
    autoplay_label: push_autoplay_label,
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
    default_hold_atlas: GameplayHoldAtlas {
        key: "openitg/hold-results",
        frame_size: [78.0, 44.0],
        style: &HoldFeedbackStyle {
            initial_zoom: 1.25,
            final_zoom: 1.0,
            release_rotation_deg: 20.0,
            show_missed_head: false,
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
    const ROTATION: f32 = 90.0;
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
    let center = layout.position(x, 270.0);
    let mut background = sprite(
        "openitg/gameplay-life-background.png",
        center,
        layout.size([272.0, 18.0]),
        [1.0; 4],
        90,
    );
    background.rotationz(ROTATION);
    actors.push(background.build(0));

    if !life.failed {
        // Two strips of the same texture move along the bar every two song beats.
        let key = if life.hot {
            "openitg/gameplay-life-hot.png"
        } else {
            "openitg/gameplay-life-normal.png"
        };
        let phase = (life.song_beat * 0.25).rem_euclid(0.5);
        for offset in [-0.5, 0.0, 0.5] {
            let strip = phase + offset;
            let start = strip.max(0.0);
            let end = (strip + 0.5).min(life.level.min(1.0));
            if end > start {
                let y = 406.0 - (start + end) * 136.0;
                let mut stream = sprite(
                    key,
                    layout.position(x, y),
                    layout.size([(end - start) * 272.0, 18.0]),
                    [1.0, 1.0, 1.0, 0.85],
                    91,
                );
                stream.rotationz(ROTATION);
                stream.customtexturerect([(start - strip) * 2.0, 0.0, (end - strip) * 2.0, 1.0]);
                actors.push(stream.build(0));
            }
        }
    }
    let mut frame = sprite(
        "openitg/gameplay-life-frame.png",
        center,
        layout.size([314.0, 40.0]),
        [1.0; 4],
        92,
    );
    frame.rotationz(ROTATION);
    actors.push(frame.build(0));

    if let Some(percent) = life.percent_label {
        let text = TextContent::inline_format(format_args!("{percent:.1}%"));
        if let Some(text) = text {
            let mut label = layout.text([x, 415.0], 0.5, text, 0.6, [0.92, 0.97, 1.0, 1.0]);
            label.font(MENU_NORMAL_FONT);
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
    } else {
        const P1: [f32; 4] = rgba_hex("#FBBE03");
        const P2: [f32; 4] = rgba_hex("#56FF48");
        let mut rgba = if score.p2 { P2 } else { P1 };
        rgba[3] = score.rgba[3];
        rgba
    };
    let centi = (score.percent.max(0.0) * 100.0) as u32;
    let whole = centi / 100;
    let fraction = centi % 100;
    if let Some(text) = TextContent::inline_format(format_args!("{whole}.{fraction:02}%")) {
        let y = 56.0 - slide_offset(score.motion);
        let inset = if score.p2 { -20.0 } else { 20.0 };
        // The native percentage pads a single-digit score with one blank glyph.
        let half_space = if whole < 10 { 12.0 } else { 0.0 };
        let mut label = layout.text(
            [WIDTH * 0.5 + offset + inset + half_space, y],
            0.5,
            text,
            1.0,
            rgba,
        );
        label.font(GAMEPLAY_SCORE_FONT);
        label.shadowlength(4.0);
        label.z(90);
        actors.push(label.build(0));
    }
}

fn push_song_meter(actors: &mut Vec<Actor>, meter: GameplaySongMeterView<'_>) {
    const X: f32 = WIDTH * 0.5;
    const STREAM_WIDTH: f32 = 390.0;
    let layout = Layout::centered(screen_width(), screen_height());
    let y = 24.0 - slide_offset(meter.motion);
    let center = layout.position(X, y);
    if meter.progress > 0.0 {
        let mut stream = sprite(
            "openitg/gameplay-meter-stream.png",
            center,
            layout.size([STREAM_WIDTH, 18.0]),
            [1.0; 4],
            90,
        );
        stream.cropright(1.0 - meter.progress);
        actors.push(stream.build(0));
    }
    let tip_x = X + (meter.progress - 0.5) * STREAM_WIDTH;
    actors.push(
        sprite(
            "openitg/gameplay-tip.png",
            layout.position(tip_x, y),
            layout.size([4.0, 16.0]),
            [1.0, 1.0, 1.0, 0.8],
            90,
        )
        .build(0),
    );
    actors.push(
        sprite(
            "openitg/gameplay-meter-frame.png",
            center,
            layout.size([408.0, 34.0]),
            [1.0; 4],
            91,
        )
        .build(0),
    );
    let mut title = layout.text([X, y + 1.0], 0.5, meter.title.into(), 0.5, [1.0; 4]);
    title.font(MENU_NORMAL_FONT);
    title.maxwidth(layout.size([STREAM_WIDTH - 8.0, 0.0])[0]);
    title.z(92);
    actors.push(title.build(0));
    if !meter.rate.is_empty() {
        let mut rate = layout.text([X, y + 27.0], 0.5, meter.rate.into(), 0.5, [1.0; 4]);
        rate.font(MENU_NORMAL_FONT);
        rate.z(92);
        actors.push(rate.build(0));
    }
}

fn push_difficulty(actors: &mut Vec<Actor>, view: GameplayDifficultyView<'_>) {
    const LABELS: [&str; 6] = ["Novice", "Easy", "Medium", "Hard", "Expert", "Edit"];
    const COLORS: [[f32; 4]; 6] = [
        rgba_hex("#E2ABF5"),
        rgba_hex("#B2FFB5"),
        rgba_hex("#F2F2AA"),
        rgba_hex("#EBA4AB"),
        rgba_hex("#AADCF2"),
        rgba_hex("#F7F7F7"),
    ];
    let (row, _, _) = crate::select_music::difficulty_style(view.difficulty);
    let label = LABELS[row];
    let width = screen_width();
    let direction = if view.p2 { -1.0 } else { 1.0 };
    let position = if view.p2 { 0.91 } else { 0.09 };
    let shift = (position - 0.5) * width - direction * slide_offset(view.motion) * width / 300.0;
    let layout = Layout::centered(width, screen_height()).translated_x(shift);
    let x = WIDTH * 0.5;
    let mut frame = sprite(
        "openitg/gameplay-difficulty.png",
        layout.position(x, 25.0),
        layout.size([114.0, 40.0]),
        [1.0; 4],
        90,
    );
    frame.customtexturerect([0.0, row as f32 / 6.0, 1.0, (row + 1) as f32 / 6.0]);
    frame.zoomx(direction);
    actors.push(frame.build(0));
    let text = if row == 5 && !view.description.is_empty() {
        let end = view.description.floor_char_boundary(InlineText::CAPACITY);
        TextContent::inline_str(&view.description[..end]).unwrap_or_else(|| label.into())
    } else {
        label.into()
    };
    let rgba = COLORS[row];
    let mut name = layout.text([x - direction * 14.0, 25.0], 0.5, text, 0.5, rgba);
    name.font(MENU_NORMAL_FONT);
    name.maxwidth(layout.size([61.0, 0.0])[0]);
    name.shadowlength(2.0);
    name.z(91);
    actors.push(name.build(0));
    let text = TextContent::inline_u32(view.meter);
    let mut meter = layout.text([x + direction * 30.0, 25.0], 0.5, text, 0.5, rgba);
    meter.font(MENU_NORMAL_FONT);
    meter.shadowlength(2.0);
    meter.z(91);
    actors.push(meter.build(0));
}

pub(crate) fn round_artwork(round: usize) -> Option<(&'static str, [f32; 2])> {
    match round {
        1 => Some(("openitg/gameplay-stage-1.png", [498.0, 236.0])),
        2 => Some(("openitg/gameplay-stage-2.png", [554.0, 232.0])),
        3 => Some(("openitg/gameplay-stage-3.png", [562.0, 236.0])),
        4 => Some(("openitg/gameplay-stage-4.png", [556.0, 226.0])),
        5 => Some(("openitg/gameplay-stage-5.png", [556.0, 236.0])),
        6 => Some(("openitg/gameplay-stage-6.png", [552.0, 228.0])),
        _ => None,
    }
}

fn push_stage_badge(actors: &mut Vec<Actor>, view: GameplayStageBadgeView<'_>) {
    let artwork = match (view.round, view.text.as_ref()) {
        (Some(round), _) => round_artwork(round),
        (None, "FINAL STAGE") => Some(("openitg/gameplay-stage-final.png", [470.0, 284.0])),
        (None, "EVENT") => Some(("openitg/gameplay-stage-event.png", [430.0, 280.0])),
        (None, text) => text.strip_prefix("STAGE ").and_then(|round| {
            round
                .parse()
                .ok()
                .filter(|_| round.len() == 1)
                .and_then(round_artwork)
        }),
    };
    let height = screen_height();
    let layout = Layout::centered(screen_width(), height);
    let footer_y = height - layout.size([0.0, 40.0])[1];
    let (y, zoom) = match view.motion {
        GameplayHudMotion::Entering { elapsed_seconds } => {
            let progress = ((elapsed_seconds - 1.2) / 0.3).clamp(0.0, 1.0);
            let center_y = height * 0.5;
            (
                (footer_y - center_y).mul_add(progress, center_y),
                1.0 - 0.75 * progress,
            )
        }
        GameplayHudMotion::Leaving { .. } => (
            footer_y + layout.size([0.0, slide_offset(view.motion) * 1.5])[1],
            0.25,
        ),
        GameplayHudMotion::Settled => (footer_y, 0.25),
    };
    let center_x = screen_center_x();
    if let Some((texture, size)) = artwork {
        actors.push(
            sprite(
                texture,
                [center_x, y],
                layout.size(size.map(|axis| axis * zoom)),
                [1.0; 4],
                105,
            )
            .build(0),
        );
    } else {
        let text = view
            .round
            .and_then(|round| TextContent::inline_format(format_args!("ROUND {round}")))
            .unwrap_or_else(|| view.text.into());
        let mut label = layout.text([0.0, 0.0], 0.5, text, 2.6 * zoom, [1.0; 4]);
        label.xy(center_x, y);
        label.maxwidth(screen_width());
        label.font(MENU_NORMAL_FONT);
        label.shadowlength(2.0);
        label.z(105);
        actors.push(label.build(0));
    }
}

fn push_background_warning(actors: &mut Vec<Actor>, view: GameplayBackgroundWarningView) {
    let width = screen_width();
    let height = screen_height();
    let half = width * 0.5;
    let (x, w, fade_left, fade_right) = if view.both_sides {
        (0.0, width, 0.1, 0.1)
    } else if view.p2 {
        (half, half, 0.1, 0.0)
    } else {
        (0.0, half, 0.0, 0.1)
    };
    let brightness = view.brightness.clamp(0.0, 1.0);
    let color = if view.failed {
        [0.0, 0.0, 0.0, 0.5]
    } else {
        let phase = view.danger_seconds.rem_euclid(1.0) * std::f32::consts::TAU;
        [brightness, 0.0, 0.0, 0.55 - 0.25 * phase.cos()]
    };
    let mut backdrop = SpriteBuilder::solid();
    backdrop.align(0.0, 0.0);
    backdrop.xy(x, 0.0);
    backdrop.size(w, height);
    backdrop.fadeleft(fade_left);
    backdrop.faderight(fade_right);
    backdrop.diffuse(color);
    backdrop.z(-99);
    actors.push(backdrop.build(0));
    if view.failed {
        return;
    }

    let layout = Layout::centered(width, height);
    let x = match (view.both_sides, view.p2) {
        (true, _) => WIDTH * 0.5,
        (false, true) => WIDTH * 0.5 + 160.0,
        (false, false) => WIDTH * 0.5 - 160.0,
    };
    let position = [layout.position(x, 0.0)[0], height * 0.5];
    let size = layout.size([244.0, 64.0]);
    let tint = [brightness, brightness, brightness, 1.0];
    actors.push(
        sprite("openitg/gameplay-danger-text.png", position, size, tint, -99).build(0),
    );
    let pulse_time = view.danger_seconds.rem_euclid(2.0);
    if (0.5..1.2).contains(&pulse_time) {
        let scan = (pulse_time - 0.5) / 0.7;
        let mut glow = sprite("openitg/gameplay-danger-glow.png", position, size, tint, -99);
        glow.cropleft((1.3 * scan - 0.3).max(0.0));
        glow.cropright((1.0 - 1.3 * scan).max(0.0));
        glow.fadeleft(0.1);
        glow.faderight(0.1);
        actors.push(glow.build(0));
    }
}

fn push_fallback_background(actors: &mut Vec<Actor>, brightness: f32) {
    static KEY: OnceLock<Arc<str>> = OnceLock::new();
    let height = screen_height();
    let mut image = deadlib_assets::present_dsl::cover_sprite(
        KEY.get_or_init(|| Arc::from("openitg/gameplay-fallback.png"))
            .clone(),
        screen_center_x(),
        height * 0.5,
        screen_width(),
        height,
        1.0,
        -100,
    );
    if let Actor::Sprite { tint, .. } = &mut image {
        *tint = [1.0, 1.0, 1.0, brightness];
    }
    actors.push(image);
}

fn push_unjoined_seat(actors: &mut Vec<Actor>, p2: bool) {
    let width = screen_width();
    let height = screen_height();
    let layout = Layout::centered(width, height);
    let scale = layout.size([1.0, 1.0])[0];
    let mut label = layout.text(
        [WIDTH * 0.5, 468.0],
        if p2 { 1.0 } else { 0.0 },
        "NOT PRESENT".into(),
        0.5,
        [1.0; 4],
    );
    let x = if p2 {
        width - 42.0 * scale
    } else {
        42.0 * scale
    };
    label.xy(x, height - 9.0 * scale);
    label.valign(1.0);
    label.font(MENU_NORMAL_FONT);
    label.z(110);
    actors.push(label.build(0));
}

fn push_autoplay_label(actors: &mut Vec<Actor>) {
    let layout = Layout::centered(screen_width(), screen_height());
    let mut label = layout.text([WIDTH * 0.5, 382.0], 0.5, "AutoPlay".into(), 0.85, [1.0; 4]);
    label.font(MENU_NORMAL_FONT);
    label.shadowlength(2.0);
    label.z(110);
    actors.push(label.build(0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::{SizeSpec, SpriteSource};
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
                        song_beat: 0.0,
                        percent_label: Some(25.0),
                        motion: GameplayHudMotion::Settled,
                    };
                    let mut actors = Vec::new();
                    push_life_bar(&mut actors, life);
                    assert!(matches!(
                        &actors[0],
                        Actor::Sprite {
                            offset,
                            size: [SizeSpec::Px(272.0), SizeSpec::Px(18.0)],
                            ..
                        } if *offset == [x, 270.0]
                    ));
                    assert!(matches!(
                        &actors[1],
                        Actor::Sprite {
                            offset,
                            size: [SizeSpec::Px(68.0), SizeSpec::Px(18.0)],
                            rot_z_deg: 90.0,
                            ..
                        } if *offset == [x, 372.0]
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
                    assert_eq!(actors.len(), 2);
                }
            });
        }
    }

    #[test]
    fn song_meter_crops_its_stream_and_places_the_tip_at_the_same_progress() {
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            let mut actors = Vec::new();
            push_song_meter(
                &mut actors,
                GameplaySongMeterView {
                    title: &Arc::from("Quartz"),
                    rate: &Arc::from(""),
                    progress: 0.75,
                    motion: GameplayHudMotion::Settled,
                },
            );
            assert!(matches!(
                &actors[0],
                Actor::Sprite {
                    cropright: 0.25,
                    ..
                }
            ));
            assert!(matches!(
                &actors[1],
                Actor::Sprite { offset, .. } if *offset == [417.5, 24.0]
            ));
        });
    }

    #[test]
    fn round_badge_uses_the_cabinet_stage_number() {
        with_current_metrics(Metrics::centered(640.0, 480.0), || {
            let mut actors = Vec::new();
            push_stage_badge(
                &mut actors,
                GameplayStageBadgeView {
                    text: &Arc::from("EVENT"),
                    round: Some(1),
                    motion: GameplayHudMotion::Settled,
                },
            );
            assert!(matches!(
                &actors[0],
                Actor::Sprite { source: SpriteSource::TextureStatic("openitg/gameplay-stage-1.png"), offset, .. }
                    if *offset == [320.0, 440.0]
            ));
        });
    }

    #[test]
    fn stage_badge_moves_from_the_center_to_the_footer() {
        for width in [640.0, 854.0] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                for (elapsed_seconds, expected_y, expected_width) in [
                    (0.0, 240.0, 498.0),
                    (1.2, 240.0, 498.0),
                    (1.35, 340.0, 311.25),
                    (1.5, 440.0, 124.5),
                ] {
                    let mut actors = Vec::new();
                    push_stage_badge(
                        &mut actors,
                        GameplayStageBadgeView {
                            text: &Arc::from("EVENT"),
                            round: Some(1),
                            motion: GameplayHudMotion::Entering { elapsed_seconds },
                        },
                    );
                    assert!(matches!(
                        actors.as_slice(),
                        [Actor::Sprite { offset, size: [SizeSpec::Px(w), _], .. }]
                            if offset[0] == width * 0.5
                                && (offset[1] - expected_y).abs() < 0.001
                                && (*w - expected_width).abs() < 0.001
                    ));
                }
            });
        }
    }

    #[test]
    fn warning_uses_the_physical_side_and_failed_dimming_uses_both_sides() {
        with_current_metrics(Metrics::centered(854.0, 480.0), || {
            let view = GameplayBackgroundWarningView {
                p2: true,
                both_sides: false,
                failed: false,
                danger_seconds: 0.75,
                brightness: 0.8,
            };
            let mut actors = Vec::new();
            push_background_warning(&mut actors, view);
            assert!(matches!(
                actors.as_slice(),
                [
                    Actor::Sprite { offset: [427.0, 0.0], fadeleft: 0.1, tint, .. },
                    Actor::Sprite { offset: [587.0, 240.0], source: SpriteSource::TextureStatic("openitg/gameplay-danger-text.png"), .. },
                    Actor::Sprite { source: SpriteSource::TextureStatic("openitg/gameplay-danger-glow.png"), .. }
                ] if *tint == [0.8, 0.0, 0.0, 0.55]
            ));
            actors.clear();
            push_background_warning(
                &mut actors,
                GameplayBackgroundWarningView { both_sides: true, failed: true, ..view },
            );
            assert!(matches!(
                actors.as_slice(),
                [Actor::Sprite {
                    offset: [0.0, 0.0],
                    size: [SizeSpec::Px(854.0), SizeSpec::Px(480.0)],
                    fadeleft: 0.1,
                    faderight: 0.1,
                    tint: [0.0, 0.0, 0.0, 0.5],
                    ..
                }]
            ));
        });
    }

    #[test]
    fn percentage_follows_screen_width_and_keeps_stage_score_and_ex_tint() {
        for (width, p2, doubles, x) in [
            (640.0, false, false, 160.0),
            (640.0, true, false, 478.0),
            (854.0, false, false, 213.5),
            (854.0, true, false, 638.5),
            (854.0, false, true, 307.0),
            (854.0, true, true, 547.0),
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
                for (p2, x, direction) in [(false, 24.0, -1.0), (true, 616.0, 1.0)] {
                    let mut actors = Vec::new();
                    push_life_bar(
                        &mut actors,
                        GameplayLifeView {
                            p2,
                            level: 0.5,
                            failed: false,
                            hot: false,
                            song_beat: 0.0,
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
