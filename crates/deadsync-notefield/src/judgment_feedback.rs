use crate::{
    NoteXParams, TornadoBounds, beat_factor, compute_active_note_geometry, fill_lane_col_offsets,
    held_miss_zoom, hold_indicator_column_x, player_metric_y,
};
use deadlib_present::actors::{FlatDraw, FlatSprite, SpriteSource};
use deadlib_render_core::BlendMode;
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
    pub frame_rows: usize,
}

#[derive(Clone, Debug)]
pub struct IndicatorSprite {
    pub source: SpriteSource,
    pub frame_size: [f32; 2],
    pub frame_cols: usize,
    pub frame_rows: usize,
    pub scale: f32,
}

#[derive(Clone, Copy)]
pub(crate) struct TapJudgmentFeedback<'a> {
    pub render: &'a JudgmentRenderInfo,
    pub frame_row: usize,
    pub overlay_row: Option<usize>,
    pub rotation_deg: f32,
}

pub(crate) struct JudgmentFeedbackRequest<'a> {
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
    pub hold_sprite: Option<IndicatorSprite>,
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
pub(crate) fn compose_judgment_feedback(
    draws: &mut Vec<FlatDraw>,
    request: JudgmentFeedbackRequest<'_>,
) {
    if request.blind {
        return;
    }
    append_tap_judgment(draws, &request);
    append_hold_indicators(draws, &request);
}

fn append_tap_judgment(draws: &mut Vec<FlatDraw>, request: &JudgmentFeedbackRequest<'_>) {
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
        draws,
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
            draws,
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

const TAP_JUDGMENT_DURATION_S: f32 = 0.9;

/// Whether the most recent tap judgment is still inside its actor lifetime.
#[inline]
pub(crate) fn tap_judgment_active(render: &JudgmentRenderInfo, elapsed_screen: f32) -> bool {
    (elapsed_screen - render.started_at_screen_s).max(0.0) < TAP_JUDGMENT_DURATION_S
}

fn tap_judgment_zoom(elapsed: f32, zoom_mod: f32) -> Option<f32> {
    if elapsed >= TAP_JUDGMENT_DURATION_S {
        return None;
    }
    let zoom = if elapsed < 0.1 {
        let t = elapsed / 0.1;
        let ease = (1.0 - t).mul_add(-(1.0 - t), 1.0);
        (0.75_f32 - 0.8).mul_add(ease, 0.8)
    } else if elapsed < 0.7 {
        0.75
    } else {
        let t = (elapsed - 0.7) / 0.2;
        0.75 * t.mul_add(-t, 1.0)
    };
    Some(zoom * zoom_mod)
}

#[allow(clippy::too_many_arguments)]
fn append_tap_sprite(
    draws: &mut Vec<FlatDraw>,
    sprite: &TapJudgmentSprite,
    xy: [f32; 2],
    z: i16,
    rotation_deg: f32,
    frame_index: u32,
    zoom: f32,
    alpha: f32,
) {
    append_sprite(
        draws,
        sprite.source.clone(),
        xy,
        [sprite.frame_size[0] * zoom, sprite.frame_size[1] * zoom],
        z,
        rotation_deg,
        frame_uv(frame_index, sprite.frame_cols, sprite.frame_rows),
        [1.0, 1.0, 1.0, alpha],
    );
}

fn append_hold_indicators(draws: &mut Vec<FlatDraw>, request: &JudgmentFeedbackRequest<'_>) {
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
    let mut tornado = [TornadoBounds::default(); MAX_COLS];
    compute_active_note_geometry(
        &request.visual,
        &col_offsets[..num_cols],
        &mut invert[..num_cols],
        &mut tornado[..num_cols],
    );
    let beat_push = beat_factor(request.current_beat);

    if let Some(sprite) = request.held_miss_sprite.as_ref() {
        for (i, feedback) in request.held_misses.iter().take(num_cols).enumerate() {
            let Some(feedback) = feedback else { continue };
            let elapsed = (request.elapsed_screen - feedback.started_at_screen_s).max(0.0);
            if elapsed >= HELD_MISS_TOTAL_DURATION {
                continue;
            }
            let (zoom_x, zoom_y) = held_miss_zoom(elapsed, request.mini);
            let zoom = [zoom_x, zoom_y];
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
            append_indicator_sprite(draws, sprite, xy, request.style.held_miss_z, 0, zoom);
        }
    }

    if let Some(sprite) = request.hold_sprite.as_ref() {
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
                draws,
                sprite,
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
        beat_push,
        request.arrow_effect_time,
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
    draws: &mut Vec<FlatDraw>,
    sprite: &IndicatorSprite,
    xy: [f32; 2],
    z: i16,
    frame_index: u32,
    zoom: [f32; 2],
) {
    append_sprite(
        draws,
        sprite.source.clone(),
        xy,
        [
            sprite.frame_size[0] * zoom[0] * sprite.scale,
            sprite.frame_size[1] * zoom[1] * sprite.scale,
        ],
        z,
        0.0,
        frame_uv(frame_index, sprite.frame_cols, sprite.frame_rows),
        [1.0; 4],
    );
}

fn append_sprite(
    draws: &mut Vec<FlatDraw>,
    source: SpriteSource,
    center: [f32; 2],
    size: [f32; 2],
    z: i16,
    rot_z_deg: f32,
    uv_rect: [f32; 4],
    tint: [f32; 4],
) {
    draws.push(FlatDraw::Sprite(FlatSprite {
        center,
        world_z: 0.0,
        size,
        source,
        tint,
        glow: [1.0, 1.0, 1.0, 0.0],
        uv_rect,
        flip_x: false,
        flip_y: false,
        fade: [0.0; 4],
        blend: BlendMode::Alpha,
        rot_y_deg: 0.0,
        rot_z_deg,
        z,
    }));
}

fn frame_uv(frame_index: u32, frame_cols: usize, frame_rows: usize) -> [f32; 4] {
    let cols = frame_cols.max(1) as u32;
    let rows = frame_rows.max(1) as u32;
    let col = frame_index % cols;
    let row = (frame_index / cols).min(rows - 1);
    let cell = [1.0 / cols as f32, 1.0 / rows as f32];
    let left = col as f32 * cell[0];
    let top = row as f32 * cell[1];
    [left, top, left + cell[0], top + cell[1]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::SpriteSource;
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

    fn assert_sprite(
        draw: &FlatDraw,
        key: &str,
        center: [f32; 2],
        size: [f32; 2],
        tint: [f32; 4],
        uv_rect: [f32; 4],
        rot_z_deg: f32,
        z: i16,
    ) {
        match draw {
            FlatDraw::Sprite(sprite) => {
                assert_eq!(sprite.center, center);
                assert_eq!(sprite.world_z, 0.0);
                for (actual, expected) in sprite.size.into_iter().zip(size) {
                    assert!(
                        (actual - expected).abs() <= 1e-5,
                        "expected size component {expected}, got {actual}"
                    );
                }
                assert_eq!(sprite.source.texture_key(), Some(key));
                assert_eq!(sprite.tint, tint);
                assert_eq!(sprite.glow, [1.0, 1.0, 1.0, 0.0]);
                for (actual, expected) in sprite.uv_rect.into_iter().zip(uv_rect) {
                    assert!((actual - expected).abs() <= 1e-6);
                }
                assert!(!sprite.flip_x);
                assert!(!sprite.flip_y);
                assert_eq!(sprite.fade, [0.0; 4]);
                assert_eq!(sprite.blend, BlendMode::Alpha);
                assert_eq!(sprite.rot_y_deg, 0.0);
                assert_eq!(sprite.rot_z_deg, rot_z_deg);
                assert_eq!(sprite.z, z);
            }
            other => panic!("expected direct judgment sprite, got {other:?}"),
        }
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
    fn tap_judgment_draw_fingerprint_preserves_sheet_and_overlay() {
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
            frame_rows: 7,
        });
        let mut draws = Vec::new();

        compose_judgment_feedback(&mut draws, request);

        assert_eq!(draws.len(), 2);
        assert_sprite(
            &draws[0],
            "judgment",
            [320.0, 150.0],
            [150.0, 21.0],
            [1.0; 4],
            [0.0, 3.0 / 7.0, 0.5, 4.0 / 7.0],
            -7.5,
            200,
        );
        assert_sprite(
            &draws[1],
            "judgment",
            [320.0, 150.0],
            [150.0, 21.0],
            [1.0, 1.0, 1.0, 0.5],
            [0.0, 1.0 / 7.0, 0.5, 2.0 / 7.0],
            -7.5,
            200,
        );
    }

    #[test]
    fn tap_judgment_activity_matches_actor_lifetime_boundary() {
        let info = judgment_info(2.0);
        assert!(tap_judgment_active(&info, 1.5));
        assert!(tap_judgment_active(&info, 2.899));
        assert!(!tap_judgment_active(&info, 2.9));
        assert!(!tap_judgment_active(&info, 3.5));
    }

    #[test]
    fn hold_indicator_draw_fingerprint_preserves_lane_and_reverse_metrics() {
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
            frame_size: [100.0, 40.0],
            frame_cols: 1,
            frame_rows: 1,
            scale: 0.5,
        });
        request.hold_sprite = Some(IndicatorSprite {
            source: source("hold-judgment"),
            frame_size: [120.0, 30.0],
            frame_cols: 1,
            frame_rows: 2,
            scale: 1.0,
        });
        let mut draws = Vec::new();

        compose_judgment_feedback(&mut draws, request);

        assert_eq!(draws.len(), 3);
        for (draw, center) in draws[..2].iter().zip([[224.0, 195.0], [288.0, 355.0]]) {
            assert_sprite(
                draw,
                "held-miss",
                center,
                [37.5, 15.0],
                [1.0; 4],
                [0.0, 0.0, 1.0, 1.0],
                0.0,
                196,
            );
        }
        let hold_zoom = 28.8 / 140.0;
        assert_sprite(
            &draws[2],
            "hold-judgment",
            [288.0, 335.0],
            [120.0 * hold_zoom, 30.0 * hold_zoom],
            [1.0; 4],
            [0.0, 0.5, 1.0, 1.0],
            0.0,
            195,
        );
    }

    #[test]
    fn ten_column_graphical_maximum_is_twenty_two_draws() {
        let held_misses = [Some(HeldMissRenderInfo {
            started_at_screen_s: 2.0,
        }); MAX_COLS];
        let hold_judgments = [Some(HoldJudgmentRenderInfo {
            result: HoldResult::Held,
            started_at_screen_s: 2.0,
        }); MAX_COLS];
        let reverse = [0.0; MAX_COLS];
        let info = judgment_info(2.0);
        let mut request = empty_request(&held_misses, &hold_judgments);
        request.tap = Some(TapJudgmentFeedback {
            render: &info,
            frame_row: 3,
            overlay_row: Some(1),
            rotation_deg: 0.0,
        });
        request.tap_sprite = Some(TapJudgmentSprite {
            source: source("judgment"),
            frame_size: [200.0, 28.0],
            frame_cols: 2,
            frame_rows: 7,
        });
        request.held_miss_sprite = Some(IndicatorSprite {
            source: source("held-miss"),
            frame_size: [100.0, 40.0],
            frame_cols: 1,
            frame_rows: 1,
            scale: 0.5,
        });
        request.hold_sprite = Some(IndicatorSprite {
            source: source("hold-judgment"),
            frame_size: [120.0, 30.0],
            frame_cols: 1,
            frame_rows: 2,
            scale: 1.0,
        });
        request.noteskin_column_xs = None;
        request.num_cols = MAX_COLS;
        request.column_reverse_percent = &reverse;
        let mut draws = Vec::new();

        compose_judgment_feedback(&mut draws, request);

        assert_eq!(draws.len(), 2 + MAX_COLS * 2);
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
            frame_size: [100.0, 40.0],
            frame_cols: 1,
            frame_rows: 1,
            scale: 1.0,
        });
        let mut draws = Vec::new();
        compose_judgment_feedback(&mut draws, request);
        assert!(draws.is_empty());
    }

    #[test]
    fn hold_indicator_routes_beat_factor_before_arrow_time() {
        let mut request = empty_request(&[], &[]);
        request.arrow_effect_time = 0.37;
        request.visual.beat = 1.0;
        request.visual.drunk = 1.0;
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let beat_push = 12.0;

        let actual = indicator_x(&request, 1, beat_push, &col_offsets, &invert, &tornado);
        let params = NoteXParams {
            screen_height: request.screen_height,
            tornado: request.visual.tornado,
            drunk: request.visual.drunk,
            flip: request.visual.flip,
            invert: request.visual.invert,
            beat: request.visual.beat,
        };
        let expected = hold_indicator_column_x(
            request.playfield_center_x,
            1,
            beat_push,
            request.arrow_effect_time,
            &col_offsets,
            &invert,
            &tornado,
            &request.visual.move_x_cols,
            params,
            request.visual.tiny,
        );
        let swapped = hold_indicator_column_x(
            request.playfield_center_x,
            1,
            request.arrow_effect_time,
            beat_push,
            &col_offsets,
            &invert,
            &tornado,
            &request.visual.move_x_cols,
            params,
            request.visual.tiny,
        );

        assert!((actual - expected).abs() <= 1e-6);
        assert!((actual - swapped).abs() > 1e-3);
    }
}
