use crate::{
    NoteXParams, TornadoBounds, beat_factor, compute_invert_distances, compute_tornado_bounds,
    fill_lane_col_offsets, held_miss_zoom, hold_indicator_column_x, player_metric_y,
};
use deadlib_present::actors::{Actor, SpriteSource};
use deadlib_present::dsl::SpriteBuilder;
use deadsync_core::input::MAX_COLS;
use deadsync_gameplay::{
    HELD_MISS_TOTAL_DURATION, HOLD_JUDGMENT_TOTAL_DURATION, HeldMissRenderInfo,
    HoldJudgmentRenderInfo, JudgmentRenderInfo, VisualEffects,
};
use deadsync_rules::note::HoldResult;
use deadsync_theme::JudgmentFeedbackStyle;

#[derive(Clone, Debug)]
pub struct TapJudgmentSprite {
    pub source: SpriteSource,
    pub frame_size: [f32; 2],
    pub frame_cols: usize,
}

#[derive(Clone, Debug)]
pub struct IndicatorSprite {
    pub source: SpriteSource,
    pub scale: f32,
}

#[derive(Clone, Copy)]
pub struct TapJudgmentFeedback<'a> {
    pub render: &'a JudgmentRenderInfo,
    pub frame_row: usize,
    pub overlay_row: Option<usize>,
    pub rotation_deg: f32,
}

pub struct JudgmentFeedbackRequest<'a> {
    pub style: JudgmentFeedbackStyle,
    pub blind: bool,
    pub elapsed_screen: f32,
    pub tap: Option<TapJudgmentFeedback<'a>>,
    pub tap_sprite: Option<TapJudgmentSprite>,
    pub tap_xy: [f32; 2],
    pub judgment_back: bool,
    pub judgment_zoom: f32,
    pub held_misses: &'a [Option<HeldMissRenderInfo>],
    pub held_miss_sprite: Option<IndicatorSprite>,
    pub hold_judgments: &'a [Option<HoldJudgmentRenderInfo>],
    pub hold_sprite: Option<SpriteSource>,
    pub current_beat: f32,
    pub arrow_effect_time: f32,
    pub mini: f32,
    pub visual: VisualEffects,
    pub noteskin_column_xs: Option<&'a [i32]>,
    pub num_cols: usize,
    pub spacing_multiplier: f32,
    pub field_zoom: f32,
    pub playfield_center_x: f32,
    pub screen_center_y: f32,
    pub screen_height: f32,
    pub field_center_y: f32,
    pub column_reverse_percent: &'a [f32],
}

/// Compose tap judgments, held-miss indicators, and hold-result indicators
/// from renderer-neutral sprite sources and gameplay snapshots.
pub fn compose_judgment_feedback(actors: &mut Vec<Actor>, request: JudgmentFeedbackRequest<'_>) {
    if request.blind {
        return;
    }
    append_tap_judgment(actors, &request);
    append_hold_indicators(actors, &request);
}

fn append_tap_judgment(actors: &mut Vec<Actor>, request: &JudgmentFeedbackRequest<'_>) {
    let (Some(feedback), Some(sprite)) = (request.tap, request.tap_sprite.as_ref()) else {
        return;
    };
    let elapsed = (request.elapsed_screen - feedback.render.started_at_screen_s).max(0.0);
    let Some(zoom) = tap_judgment_zoom(elapsed, request.judgment_zoom) else {
        return;
    };
    let columns = sprite.frame_cols.max(1);
    let col = usize::from(columns > 1 && feedback.render.judgment.time_error_ms >= 0.0);
    let frame_index = (feedback.frame_row * columns + col) as u32;
    let z = if request.judgment_back {
        request.style.tap_back_z
    } else {
        request.style.tap_front_z
    };
    append_tap_sprite(
        actors,
        sprite,
        request.tap_xy,
        z,
        feedback.rotation_deg,
        frame_index,
        zoom,
        1.0,
    );
    if let Some(overlay_row) = feedback.overlay_row {
        append_tap_sprite(
            actors,
            sprite,
            request.tap_xy,
            z,
            feedback.rotation_deg,
            (overlay_row * columns + col) as u32,
            zoom,
            request.style.split_overlay_alpha,
        );
    }
}

fn tap_judgment_zoom(elapsed: f32, zoom_mod: f32) -> Option<f32> {
    if elapsed >= 0.9 {
        return None;
    }
    let zoom = if elapsed < 0.1 {
        let t = elapsed / 0.1;
        let ease = 1.0 - (1.0 - t).powi(2);
        0.8 + (0.75 - 0.8) * ease
    } else if elapsed < 0.7 {
        0.75
    } else {
        let t = (elapsed - 0.7) / 0.2;
        0.75 * (1.0 - t.powi(2))
    };
    Some(zoom * zoom_mod)
}

#[allow(clippy::too_many_arguments)]
fn append_tap_sprite(
    actors: &mut Vec<Actor>,
    sprite: &TapJudgmentSprite,
    xy: [f32; 2],
    z: i16,
    rotation_deg: f32,
    frame_index: u32,
    zoom: f32,
    alpha: f32,
) {
    let mut actor = SpriteBuilder::with_source(sprite.source.clone());
    actor.align(0.5, 0.5);
    actor.xy(xy[0], xy[1]);
    actor.z(z);
    actor.rotationz(rotation_deg);
    actor.size(sprite.frame_size[0], sprite.frame_size[1]);
    actor.setstate(frame_index);
    actor.zoom(zoom);
    actor.alpha(alpha);
    actors.push(actor.build(0));
}

fn append_hold_indicators(actors: &mut Vec<Actor>, request: &JudgmentFeedbackRequest<'_>) {
    if request.held_miss_sprite.is_none() && request.hold_sprite.is_none() {
        return;
    }
    let num_cols = request
        .num_cols
        .min(MAX_COLS)
        .min(request.column_reverse_percent.len());
    let mut col_offsets = [0.0_f32; MAX_COLS];
    fill_lane_col_offsets(
        &mut col_offsets,
        request.noteskin_column_xs,
        num_cols,
        request.spacing_multiplier,
        request.field_zoom,
    );
    let mut invert = [0.0_f32; MAX_COLS];
    compute_invert_distances(&col_offsets[..num_cols], &mut invert[..num_cols]);
    let mut tornado = [TornadoBounds::default(); MAX_COLS];
    compute_tornado_bounds(&col_offsets[..num_cols], &mut tornado[..num_cols]);
    let beat_push = beat_factor(request.current_beat);

    if let Some(sprite) = request.held_miss_sprite.as_ref() {
        for (i, feedback) in request.held_misses.iter().take(num_cols).enumerate() {
            let Some(feedback) = feedback else { continue };
            let elapsed = (request.elapsed_screen - feedback.started_at_screen_s).max(0.0);
            if elapsed >= HELD_MISS_TOTAL_DURATION {
                continue;
            }
            let (zoom_x, zoom_y) = held_miss_zoom(elapsed, request.mini);
            let zoom = [zoom_x * sprite.scale, zoom_y * sprite.scale];
            if zoom[0] <= f32::EPSILON || zoom[1] <= f32::EPSILON {
                continue;
            }
            let xy = [
                indicator_x(request, i, beat_push, &col_offsets, &invert, &tornado),
                player_metric_y(
                    request.screen_center_y,
                    request.field_center_y,
                    request.column_reverse_percent[i],
                    request.style.held_miss_normal_y,
                    request.style.held_miss_reverse_y,
                ),
            ];
            append_indicator_sprite(
                actors,
                sprite.source.clone(),
                xy,
                request.style.held_miss_z,
                0,
                zoom,
            );
        }
    }

    if let Some(source) = request.hold_sprite.as_ref() {
        for (i, feedback) in request.hold_judgments.iter().take(num_cols).enumerate() {
            let Some(feedback) = feedback else { continue };
            let elapsed = (request.elapsed_screen - feedback.started_at_screen_s).max(0.0);
            if elapsed >= HOLD_JUDGMENT_TOTAL_DURATION {
                continue;
            }
            let progress = (elapsed / 0.3).clamp(0.0, 1.0);
            let zoom = (request.style.hold_initial_zoom
                + progress * (request.style.hold_final_zoom - request.style.hold_initial_zoom))
                * request.judgment_zoom;
            let frame_index = match feedback.result {
                HoldResult::Held => 0,
                HoldResult::LetGo | HoldResult::Missed => 1,
            };
            let xy = [
                indicator_x(request, i, beat_push, &col_offsets, &invert, &tornado),
                player_metric_y(
                    request.screen_center_y,
                    request.field_center_y,
                    request.column_reverse_percent[i],
                    request.style.hold_normal_y,
                    request.style.hold_reverse_y,
                ),
            ];
            append_indicator_sprite(
                actors,
                source.clone(),
                xy,
                request.style.hold_z,
                frame_index,
                [zoom, zoom],
            );
        }
    }
}

fn indicator_x(
    request: &JudgmentFeedbackRequest<'_>,
    local_col: usize,
    beat_push: f32,
    col_offsets: &[f32],
    invert: &[f32],
    tornado: &[TornadoBounds],
) -> f32 {
    hold_indicator_column_x(
        request.playfield_center_x,
        local_col,
        request.arrow_effect_time,
        beat_push,
        col_offsets,
        invert,
        tornado,
        &request.visual.move_x_cols,
        NoteXParams {
            screen_height: request.screen_height,
            tornado: request.visual.tornado,
            drunk: request.visual.drunk,
            flip: request.visual.flip,
            invert: request.visual.invert,
            beat: request.visual.beat,
        },
        request.visual.tiny,
    )
}

fn append_indicator_sprite(
    actors: &mut Vec<Actor>,
    source: SpriteSource,
    xy: [f32; 2],
    z: i16,
    frame_index: u32,
    zoom: [f32; 2],
) {
    let mut actor = SpriteBuilder::with_source(source);
    actor.align(0.5, 0.5);
    actor.xy(xy[0], xy[1]);
    actor.z(z);
    actor.setstate(frame_index);
    actor.zoomx(zoom[0]);
    actor.zoomy(zoom[1]);
    actor.alpha(1.0);
    actors.push(actor.build(0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::{SizeSpec, SpriteSource};
    use deadsync_rules::judgment::{JudgeGrade, Judgment, TimingWindow};
    use std::sync::Arc;

    fn style() -> JudgmentFeedbackStyle {
        JudgmentFeedbackStyle {
            tap_front_z: 200,
            tap_back_z: 95,
            split_overlay_alpha: 0.5,
            held_miss_normal_y: -50.0,
            held_miss_reverse_y: 110.0,
            held_miss_z: 196,
            hold_normal_y: -90.0,
            hold_reverse_y: 90.0,
            hold_z: 195,
            hold_initial_zoom: 25.6 / 140.0,
            hold_final_zoom: 32.0 / 140.0,
        }
    }

    fn judgment_info(started_at_screen_s: f32) -> JudgmentRenderInfo {
        JudgmentRenderInfo {
            judgment: Judgment {
                time_error_ms: -12.0,
                time_error_music_ns: -12_000_000,
                grade: JudgeGrade::Great,
                window: Some(TimingWindow::W3),
                miss_because_held: false,
            },
            started_at_screen_s,
        }
    }

    fn source(name: &str) -> SpriteSource {
        SpriteSource::Texture(Arc::from(name))
    }

    fn empty_request<'a>(
        held_misses: &'a [Option<HeldMissRenderInfo>],
        hold_judgments: &'a [Option<HoldJudgmentRenderInfo>],
    ) -> JudgmentFeedbackRequest<'a> {
        JudgmentFeedbackRequest {
            style: style(),
            blind: false,
            elapsed_screen: 2.2,
            tap: None,
            tap_sprite: None,
            tap_xy: [320.0, 150.0],
            judgment_back: false,
            judgment_zoom: 1.0,
            held_misses,
            held_miss_sprite: None,
            hold_judgments,
            hold_sprite: None,
            current_beat: 4.0,
            arrow_effect_time: 10.0,
            mini: 0.0,
            visual: VisualEffects::default(),
            noteskin_column_xs: Some(&[-96, -32, 32, 96]),
            num_cols: 4,
            spacing_multiplier: 1.0,
            field_zoom: 1.0,
            playfield_center_x: 320.0,
            screen_center_y: 240.0,
            screen_height: 480.0,
            field_center_y: 5.0,
            column_reverse_percent: &[0.0, 1.0, 0.0, 1.0],
        }
    }

    #[test]
    fn tap_judgment_actor_fingerprint_preserves_sheet_and_overlay() {
        let info = judgment_info(2.0);
        let mut request = empty_request(&[], &[]);
        request.tap = Some(TapJudgmentFeedback {
            render: &info,
            frame_row: 3,
            overlay_row: Some(1),
            rotation_deg: -7.5,
        });
        request.tap_sprite = Some(TapJudgmentSprite {
            source: source("judgment"),
            frame_size: [200.0, 28.0],
            frame_cols: 2,
        });
        let mut actors = Vec::new();

        compose_judgment_feedback(&mut actors, request);

        assert_eq!(actors.len(), 2);
        for (actor, cell, alpha) in [(&actors[0], 6, 1.0), (&actors[1], 2, 0.5)] {
            match actor {
                Actor::Sprite {
                    align,
                    offset,
                    size,
                    source,
                    tint,
                    z,
                    cell: actual_cell,
                    rot_z_deg,
                    scale,
                    ..
                } => {
                    assert_eq!(*align, [0.5, 0.5]);
                    assert_eq!(*offset, [320.0, 150.0]);
                    assert!(matches!(
                        size,
                        [SizeSpec::Px(w), SizeSpec::Px(h)]
                            if (*w - 150.0).abs() <= 1e-6 && (*h - 21.0).abs() <= 1e-6
                    ));
                    assert_eq!(source.texture_key(), Some("judgment"));
                    assert_eq!(*tint, [1.0, 1.0, 1.0, alpha]);
                    assert_eq!(*z, 200);
                    assert_eq!(*actual_cell, Some((cell, u32::MAX)));
                    assert_eq!(*rot_z_deg, -7.5);
                    assert_eq!(*scale, [1.0, 1.0]);
                }
                other => panic!("expected tap judgment sprite, got {other:?}"),
            }
        }
    }

    #[test]
    fn hold_indicator_actor_fingerprint_preserves_lane_and_reverse_metrics() {
        let held_misses = [
            Some(HeldMissRenderInfo {
                started_at_screen_s: 2.0,
            }),
            Some(HeldMissRenderInfo {
                started_at_screen_s: 2.0,
            }),
        ];
        let hold_judgments = [
            None,
            Some(HoldJudgmentRenderInfo {
                result: HoldResult::LetGo,
                started_at_screen_s: 2.05,
            }),
        ];
        let mut request = empty_request(&held_misses, &hold_judgments);
        request.held_miss_sprite = Some(IndicatorSprite {
            source: source("held-miss"),
            scale: 0.5,
        });
        request.hold_sprite = Some(source("hold-judgment"));
        let mut actors = Vec::new();

        compose_judgment_feedback(&mut actors, request);

        assert_eq!(actors.len(), 3);
        let expected = [
            ("held-miss", [224.0, 195.0], 196, 0, [0.375, 0.375]),
            ("held-miss", [288.0, 355.0], 196, 0, [0.375, 0.375]),
            (
                "hold-judgment",
                [288.0, 335.0],
                195,
                1,
                [(28.8 / 140.0), (28.8 / 140.0)],
            ),
        ];
        for (actor, (key, offset, expected_z, cell, zoom)) in actors.iter().zip(expected) {
            match actor {
                Actor::Sprite {
                    offset: actual_offset,
                    source,
                    z,
                    cell: actual_cell,
                    scale,
                    ..
                } => {
                    assert_eq!(*actual_offset, offset);
                    assert_eq!(source.texture_key(), Some(key));
                    assert_eq!(*z, expected_z);
                    assert_eq!(*actual_cell, Some((cell, u32::MAX)));
                    assert!((scale[0] - zoom[0]).abs() <= 1e-6);
                    assert!((scale[1] - zoom[1]).abs() <= 1e-6);
                }
                other => panic!("expected hold feedback sprite, got {other:?}"),
            }
        }
    }

    #[test]
    fn blind_and_expired_feedback_emit_nothing() {
        let held_misses = [Some(HeldMissRenderInfo {
            started_at_screen_s: 0.0,
        })];
        let mut request = empty_request(&held_misses, &[]);
        request.blind = true;
        request.held_miss_sprite = Some(IndicatorSprite {
            source: source("held-miss"),
            scale: 1.0,
        });
        let mut actors = Vec::new();
        compose_judgment_feedback(&mut actors, request);
        assert!(actors.is_empty());
    }
}
