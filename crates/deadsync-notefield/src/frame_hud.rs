use crate::actor_builder::{CapturedActorScratch, CapturedActorSource, share_actor_range};
use crate::combo_feedback::{ComboFeedbackRequest, ComboMilestoneAssets, compose_combo_feedback};
use crate::compose::{NotefieldComposeRequest, PreparedNotefield};
use crate::error_bar::{
    ErrorBarComposeRequest, ErrorBarState, compose_error_bar, offset_indicator_active,
};
use crate::feedback::{
    JudgmentTiltParams, TapJudgmentRowsParams, judgment_actor_zoom, judgment_tilt_rotation_deg,
    tap_judgment_rows,
};
use crate::hud::{
    CounterHudRequest, MiniIndicatorRequest, compose_counter_hud, compose_mini_indicator,
};
use crate::judgment_feedback::{
    IndicatorSprite, JudgmentFeedbackRequest, TapJudgmentFeedback, TapJudgmentSprite,
    compose_judgment_feedback,
};
use crate::mini_indicator::ZmodMeasureCounterText;
use deadlib_present::actors::{Actor, SpriteSource, TextContent};
use deadsync_gameplay::{
    ActiveComboMilestone, ErrorBarText, ErrorBarTick, HeldMissRenderInfo, HoldJudgmentRenderInfo,
    JudgmentRenderInfo, OffsetIndicatorText,
};
use deadsync_rules::stream::StreamSegment;

/// Prepared combo values and renderer-neutral assets for one HUD frame.
pub struct ComboHudFrame<'a> {
    pub milestones: &'a [ActiveComboMilestone],
    pub milestone_assets: Option<ComboMilestoneAssets>,
    pub combo: u32,
    pub miss_combo: u32,
    pub player_color: [f32; 4],
    pub combo_color: [f32; 4],
    pub font: Option<&'static str>,
    pub number_text: fn(u32) -> TextContent,
}

/// Borrowed gameplay error-bar state plus prepared theme text adapters.
pub struct ErrorBarHudFrame<'a> {
    pub mono_ticks: &'a [Option<ErrorBarTick>],
    pub color_ticks: &'a [Option<ErrorBarTick>],
    pub average_ticks: &'a [Option<ErrorBarTick>],
    pub color_bar_started_at: Option<f32>,
    pub average_bar_started_at: Option<f32>,
    pub flash_early: &'a [Option<f32>],
    pub flash_late: &'a [Option<f32>],
    pub timing_windows_s: [f32; 5],
    pub offset_indicator: Option<OffsetIndicatorText>,
    pub long_average_tick: Option<ErrorBarTick>,
    pub long_average_active: bool,
    pub text: Option<ErrorBarText>,
    pub frame_text_slot: u8,
    pub offset_text: fn(f32) -> TextContent,
    pub text_label: fn(bool, bool) -> TextContent,
}

impl ErrorBarHudFrame<'_> {
    /// Whether the retained millisecond indicator is inside its actor lifetime.
    #[inline]
    pub fn offset_active(
        indicator: Option<OffsetIndicatorText>,
        elapsed_screen: f32,
        duration: f32,
    ) -> bool {
        indicator
            .is_some_and(|indicator| offset_indicator_active(indicator, elapsed_screen, duration))
    }
}

/// Prepared measure-counter inputs for one HUD frame.
#[derive(Clone, Copy)]
pub struct CounterHudFrame<'a> {
    pub segments: &'a [StreamSegment],
    pub broken_run_lookup: &'a crate::BrokenRunLookup,
    pub current_bpm: f32,
    pub font: &'static str,
    pub counter_text: fn(ZmodMeasureCounterText) -> TextContent,
    pub timer_text: fn(i32, i32, bool) -> TextContent,
}

/// Fully resolved theme-selected mini-indicator content.
pub struct MiniHudFrame {
    pub text: TextContent,
    pub color: [f32; 4],
    pub failed: bool,
    pub font: &'static str,
}

/// A tap judgment paired with its renderer-neutral sheet description.
pub struct TapJudgmentHudFrame<'a> {
    pub render: &'a JudgmentRenderInfo,
    pub sprite: TapJudgmentSprite,
    pub frame_rows: usize,
}

impl TapJudgmentHudFrame<'_> {
    /// Whether a retained gameplay judgment can still produce a tap actor.
    #[inline]
    pub fn render_active(render: &JudgmentRenderInfo, elapsed_screen: f32) -> bool {
        crate::judgment_feedback::tap_judgment_active(render, elapsed_screen)
    }
}

/// Prepared judgment snapshots and renderer-neutral assets for local lanes.
pub struct JudgmentHudFrame<'a> {
    pub tap: Option<TapJudgmentHudFrame<'a>>,
    pub held_misses: &'a [Option<HeldMissRenderInfo>],
    pub held_miss_sprite: Option<IndicatorSprite>,
    pub hold_judgments: &'a [Option<HoldJudgmentRenderInfo>],
    pub hold_sprite: Option<SpriteSource>,
}

/// Theme-prepared runtime snapshots for the canonical post-chrome HUD pass.
///
/// A concrete theme inserts any theme-owned gameplay chrome before passing this
/// view to [`compose_notefield_hud`]. Lane-indexed judgment slices are
/// local-lane ordered for the player span in `PreparedNotefield::frame_plan`.
pub struct NotefieldHudFrameView<'a> {
    pub combo: Option<ComboHudFrame<'a>>,
    pub error_bar: Option<ErrorBarHudFrame<'a>>,
    pub counter: Option<CounterHudFrame<'a>>,
    pub mini: Option<MiniHudFrame>,
    pub judgment: Option<JudgmentHudFrame<'a>>,
}

impl NotefieldHudFrameView<'_> {
    /// Whether combo preparation can produce actors for this frame.
    #[inline]
    pub const fn combo_active(hide_combo: bool, blind: bool, combo_visible: bool) -> bool {
        !hide_combo && !blind && combo_visible
    }

    /// Whether graphical or currently active numeric timing feedback can emit actors.
    #[inline]
    pub const fn error_active(blind: bool, error_bar: bool, offset_active: bool) -> bool {
        !blind && (error_bar || offset_active)
    }
}

/// Proxy captures produced while composing the canonical HUD sequence.
pub struct NotefieldHudComposeResult {
    pub combo_actors: Option<CapturedActorSource>,
    pub judgment_actors: Option<CapturedActorSource>,
}

/// Compose the complete canonical HUD sequence after concrete theme chrome.
///
/// Ordering is fixed as combo, combo capture, error bar, counter, mini
/// indicator, judgment feedback, then judgment capture.
pub fn compose_notefield_hud<S>(
    actors: &mut Vec<Actor>,
    request: &NotefieldComposeRequest<'_, S>,
    prepared: &PreparedNotefield<'_, S>,
    frame: &NotefieldHudFrameView<'_>,
    capture_scratch: &mut CapturedActorScratch,
) -> NotefieldHudComposeResult {
    let combo_capture_start = actors.len();
    if let Some(combo) = frame.combo.as_ref() {
        compose_combo(actors, request, prepared, combo);
    }
    let combo_actors = request
        .capture_requests
        .combo
        .then(|| share_actor_range(actors, combo_capture_start, &mut capture_scratch.combo))
        .flatten();

    if let Some(error_bar) = frame.error_bar.as_ref() {
        compose_error(actors, request, prepared, error_bar);
    }

    if let (Some(options), Some(counter)) = (request.options.measure_counter, frame.counter) {
        compose_counter_hud(
            actors,
            CounterHudRequest {
                style: request.style.counter_hud,
                segments: counter.segments,
                broken_run_lookup: counter.broken_run_lookup,
                current_beat: prepared.current_beat,
                current_display_beat: request.visual.current_display_beat,
                current_bpm: counter.current_bpm,
                music_rate: request.chart.music_rate,
                lookahead: options.lookahead,
                multiplier: options.multiplier,
                vertical: options.vertical,
                left: options.left,
                broken_run: options.broken_run,
                run_timer: options.run_timer,
                measure_counter_y: prepared.field.hud_layout.zmod_layout.measure_counter_y,
                subtractive_scoring_y: prepared.field.hud_layout.zmod_layout.subtractive_scoring_y,
                playfield_center_x: prepared.field.playfield_center_x,
                field_zoom: prepared.field_zoom,
                font: counter.font,
                counter_text: counter.counter_text,
                timer_text: counter.timer_text,
            },
        );
    }

    if let Some(mini) = frame.mini.as_ref() {
        let layout = prepared.field.hud_layout.zmod_layout;
        compose_mini_indicator(
            actors,
            MiniIndicatorRequest {
                style: request.style.mini_indicator,
                text: mini.text.clone(),
                color: mini.color,
                failed: mini.failed,
                position: request.options.mini_indicator_position,
                counter_left: request.options.counter_left,
                playfield_center_x: prepared.field.playfield_center_x,
                field_zoom: prepared.field_zoom,
                layout_add_x: layout.subtractive_scoring_addx,
                y: layout.subtractive_scoring_y,
                zoom: request.options.mini_indicator_zoom,
                font: mini.font,
            },
        );
    }

    let judgment_capture_start = actors.len();
    if let Some(judgment) = frame.judgment.as_ref() {
        compose_judgment(actors, request, prepared, judgment);
    }
    let judgment_actors = request
        .capture_requests
        .judgment
        .then(|| {
            share_actor_range(
                actors,
                judgment_capture_start,
                &mut capture_scratch.judgment,
            )
        })
        .flatten();

    NotefieldHudComposeResult {
        combo_actors,
        judgment_actors,
    }
}

fn compose_combo<S>(
    actors: &mut Vec<Actor>,
    request: &NotefieldComposeRequest<'_, S>,
    prepared: &PreparedNotefield<'_, S>,
    frame: &ComboHudFrame<'_>,
) {
    let show = NotefieldHudFrameView::combo_active(
        request.view.hide_combo,
        prepared.blind_active,
        request.options.frame_features.combo_visible,
    );
    let milestone_assets =
        (show && !request.options.hide_combo_explosions && !frame.milestones.is_empty())
            .then_some(frame.milestone_assets.as_ref())
            .flatten();
    let player_color = if milestone_assets.is_some() {
        frame.player_color
    } else {
        [1.0; 4]
    };
    let combo_color = if show
        && frame.miss_combo < request.style.combo_feedback.threshold
        && frame.combo >= request.style.combo_feedback.threshold
    {
        frame.combo_color
    } else {
        [1.0; 4]
    };
    let field = prepared.field;
    compose_combo_feedback(
        actors,
        ComboFeedbackRequest {
            style: request.style.combo_feedback,
            show,
            milestone_assets,
            milestones: frame.milestones,
            combo: frame.combo,
            miss_combo: frame.miss_combo,
            number_xy: [field.combo_x, field.hud_layout.zmod_layout.combo_y],
            milestone_xy: [
                field.playfield_center_x,
                field.hud_layout.zmod_layout.combo_y,
            ],
            mini: prepared.mini,
            player_color,
            combo_color,
            font: frame.font,
            number_text: frame.number_text,
        },
    );
}

fn compose_error<S>(
    actors: &mut Vec<Actor>,
    request: &NotefieldComposeRequest<'_, S>,
    prepared: &PreparedNotefield<'_, S>,
    frame: &ErrorBarHudFrame<'_>,
) {
    let field = prepared.field;
    let layout = field.hud_layout;
    let num_cols = prepared
        .frame_plan
        .num_cols
        .min(field.column_receptor_ys.len());
    let average_y = if num_cols == 0 {
        0.0
    } else {
        field.column_receptor_ys[..num_cols].iter().sum::<f32>() / num_cols as f32
    };
    let show = request.options.frame_features.error_bar;
    compose_error_bar(
        actors,
        ErrorBarComposeRequest {
            style: request.style.error_bar,
            modes: request.options.error_bar_modes,
            state: ErrorBarState {
                mono_ticks: frame.mono_ticks,
                color_ticks: frame.color_ticks,
                average_ticks: frame.average_ticks,
                color_bar_started_at: frame.color_bar_started_at,
                average_bar_started_at: frame.average_bar_started_at,
                flash_early: frame.flash_early,
                flash_late: frame.flash_late,
            },
            visible: !prepared.blind_active && show,
            elapsed_s: request.visual.elapsed_screen_s,
            position: [field.error_bar_x, layout.error_bar_y],
            average_y,
            max_height: layout.error_bar_max_h,
            mini: prepared.mini,
            timing_windows_s: frame.timing_windows_s,
            blue_fantastic_window_s: Some(request.options.blue_fantastic_window_s),
            max_window_ix: request.options.error_bar_max_window_ix,
            show_fa_plus: request.options.show_fa_plus_window,
            judgment_back: request.options.judgment_back,
            monochrome_background: request.options.monochrome_background,
            multi_tick: request.options.error_bar_multi_tick,
            short_average: request.options.short_average_error_bar,
            center_tick: request.options.center_tick,
            has_error_bar: show,
            offset_indicator: frame.offset_indicator,
            offset_indicator_visible: !prepared.blind_active && request.options.error_ms_display,
            offset_indicator_position: [
                field.playfield_center_x,
                request.geometry.screen_center_y + field.notefield_offset_y,
            ],
            offset_text: frame.offset_text,
            long_average_tick: frame.long_average_tick,
            long_average_visible: !prepared.blind_active
                && show
                && request.options.long_error_bar_enabled
                && frame.long_average_active,
            long_average_intensity: request.options.long_error_bar_intensity,
            text: frame.text,
            frame_text_slot: frame.frame_text_slot,
            text_visible: !prepared.blind_active
                && show
                && request.options.frame_features.error_bar_text,
            text_label: frame.text_label,
        },
    );
}

fn compose_judgment<S>(
    actors: &mut Vec<Actor>,
    request: &NotefieldComposeRequest<'_, S>,
    prepared: &PreparedNotefield<'_, S>,
    frame: &JudgmentHudFrame<'_>,
) {
    let options = request.options;
    let (tap, tap_sprite) = if prepared.blind_active {
        (None, None)
    } else if let Some(frame) = frame.tap.as_ref() {
        let judgment = &frame.render.judgment;
        let (frame_row, overlay_row) = tap_judgment_rows(TapJudgmentRowsParams {
            grade: judgment.grade,
            window: judgment.window,
            time_error_ms: judgment.time_error_ms,
            frame_rows: frame.frame_rows,
            show_fa_plus_window: options.show_fa_plus_window,
            fa_plus_10ms_blue_window: options.fa_plus_10ms_blue_window,
            split_15_10ms: options.split_15_10ms,
            custom_fantastic_window: options.custom_fantastic_window,
        });
        let rotation_deg = judgment_tilt_rotation_deg(JudgmentTiltParams {
            enabled: options.judgment_tilt_enabled,
            grade: judgment.grade,
            time_error_ms: judgment.time_error_ms,
            min_threshold_ms: options.judgment_tilt_min_ms,
            max_threshold_ms: options.judgment_tilt_max_ms,
            multiplier: options.judgment_tilt_multiplier,
        });
        (
            Some(TapJudgmentFeedback {
                render: frame.render,
                frame_row,
                overlay_row,
                rotation_deg,
            }),
            Some(frame.sprite.clone()),
        )
    } else {
        (None, None)
    };
    let held_miss_sprite = (!prepared.blind_active
        && frame.held_misses.iter().any(Option::is_some))
    .then(|| frame.held_miss_sprite.clone())
    .flatten();
    let hold_sprite = (!prepared.blind_active && frame.hold_judgments.iter().any(Option::is_some))
        .then(|| frame.hold_sprite.clone())
        .flatten();
    let field = prepared.field;
    let noteskin_column_xs = prepared
        .notes
        .as_ref()
        .map(|notes| notes.base.column_xs.as_slice());
    compose_judgment_feedback(
        actors,
        JudgmentFeedbackRequest {
            style: request.style.judgment_feedback,
            blind: prepared.blind_active,
            elapsed_screen: request.visual.elapsed_screen_s,
            tap,
            tap_sprite,
            tap_xy: [field.judgment_x, field.hud_layout.judgment_y],
            judgment_back: options.judgment_back,
            judgment_zoom: judgment_actor_zoom(
                prepared.mini,
                options.judgment_back,
                request.visual.perspective.tilt,
                request.visual.perspective.skew,
            ),
            held_misses: frame.held_misses,
            held_miss_sprite,
            hold_judgments: frame.hold_judgments,
            hold_sprite,
            current_beat: prepared.current_beat,
            arrow_effect_time: request.arrow_effect_time_s,
            mini: prepared.mini,
            visual: request.visual.visual,
            noteskin_column_xs,
            num_cols: prepared.frame_plan.num_cols,
            spacing_multiplier: request.visual.spacing_multiplier,
            field_zoom: prepared.field_zoom,
            playfield_center_x: field.playfield_center_x,
            screen_center_y: request.geometry.screen_center_y,
            screen_height: request.geometry.screen_height,
            field_center_y: field.notefield_offset_y,
            column_reverse_percent: &field.column_reverse_percent[..prepared.frame_plan.num_cols],
        },
    );
}

#[cfg(feature = "bench-support")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HudOptionGateBenchFrame {
    pub checksum: u64,
    pub samples: usize,
}

#[cfg(feature = "bench-support")]
pub struct HudOptionGateBench {
    receptor_ys: [f32; 8],
    tap: JudgmentRenderInfo,
}

#[cfg(feature = "bench-support")]
const HUD_GATE_BENCH_SAMPLES: usize = 256;

#[cfg(feature = "bench-support")]
impl Default for HudOptionGateBench {
    fn default() -> Self {
        Self {
            receptor_ys: [-144.0, -96.0, -48.0, 0.0, 0.0, 48.0, 96.0, 144.0],
            tap: JudgmentRenderInfo {
                judgment: deadsync_rules::judgment::Judgment {
                    time_error_ms: -12.0,
                    time_error_music_ns: -12_000_000,
                    grade: deadsync_rules::judgment::JudgeGrade::Fantastic,
                    window: Some(deadsync_rules::judgment::TimingWindow::W1),
                    miss_because_held: false,
                },
                started_at_screen_s: 1.0,
            },
        }
    }
}

#[cfg(feature = "bench-support")]
impl HudOptionGateBench {
    pub fn old_hidden_combo_frame(&self, frame: usize) -> HudOptionGateBenchFrame {
        let mut samples = 0;
        for sample in 0..HUD_GATE_BENCH_SAMPLES {
            let show = NotefieldHudFrameView::combo_active(true, false, true);
            samples += std::hint::black_box(bench_combo_output(show, frame + sample)).samples;
        }
        bench_hud_output(frame, samples)
    }

    pub fn new_hidden_combo_frame(&self, frame: usize) -> HudOptionGateBenchFrame {
        let mut samples = 0;
        for sample in 0..HUD_GATE_BENCH_SAMPLES {
            if NotefieldHudFrameView::combo_active(true, false, true) {
                samples += bench_combo_output(true, frame + sample).samples;
            }
        }
        bench_hud_output(frame, samples)
    }

    pub fn old_disabled_error_frame(&self, frame: usize) -> HudOptionGateBenchFrame {
        let mut samples = 0;
        for sample in 0..HUD_GATE_BENCH_SAMPLES {
            let average_y = self.receptor_ys.iter().sum::<f32>() / self.receptor_ys.len() as f32;
            samples +=
                std::hint::black_box(bench_error_output(false, false, average_y, frame + sample))
                    .samples;
        }
        bench_hud_output(frame, samples)
    }

    pub fn new_disabled_error_frame(&self, frame: usize) -> HudOptionGateBenchFrame {
        let mut samples = 0;
        for sample in 0..HUD_GATE_BENCH_SAMPLES {
            if NotefieldHudFrameView::error_active(false, false, false) {
                let average_y =
                    self.receptor_ys.iter().sum::<f32>() / self.receptor_ys.len() as f32;
                samples += bench_error_output(false, false, average_y, frame + sample).samples;
            }
        }
        bench_hud_output(frame, samples)
    }

    pub fn old_expired_offset_frame(&self, frame: usize) -> HudOptionGateBenchFrame {
        let mut samples = 0;
        for sample in 0..HUD_GATE_BENCH_SAMPLES {
            if NotefieldHudFrameView::error_active(false, false, true) {
                let average_y =
                    self.receptor_ys.iter().sum::<f32>() / self.receptor_ys.len() as f32;
                std::hint::black_box(average_y);
                samples += bench_error_output(false, false, average_y, frame + sample).samples;
            }
        }
        bench_hud_output(frame, samples)
    }

    pub fn new_expired_offset_frame(&self, frame: usize) -> HudOptionGateBenchFrame {
        let mut samples = 0;
        for sample in 0..HUD_GATE_BENCH_SAMPLES {
            let indicator = Some(OffsetIndicatorText {
                started_at: 1.0,
                offset_ms: -12.0,
                window: deadsync_rules::judgment::TimingWindow::W1,
            });
            let offset_active = ErrorBarHudFrame::offset_active(indicator, 2.0, 0.5);
            if NotefieldHudFrameView::error_active(false, false, offset_active) {
                let average_y =
                    self.receptor_ys.iter().sum::<f32>() / self.receptor_ys.len() as f32;
                samples += bench_error_output(false, false, average_y, frame + sample).samples;
            }
        }
        bench_hud_output(frame, samples)
    }

    pub fn old_expired_judgment_frame(&self, frame: usize) -> HudOptionGateBenchFrame {
        let mut samples = 0;
        for sample in 0..HUD_GATE_BENCH_SAMPLES {
            let tap = std::hint::black_box(&self.tap);
            let judgment = &tap.judgment;
            let rows = tap_judgment_rows(TapJudgmentRowsParams {
                grade: judgment.grade,
                window: judgment.window,
                time_error_ms: judgment.time_error_ms,
                frame_rows: 14,
                show_fa_plus_window: true,
                fa_plus_10ms_blue_window: false,
                split_15_10ms: false,
                custom_fantastic_window: false,
            });
            let rotation = judgment_tilt_rotation_deg(JudgmentTiltParams {
                enabled: true,
                grade: judgment.grade,
                time_error_ms: judgment.time_error_ms,
                min_threshold_ms: 5.0,
                max_threshold_ms: 100.0,
                multiplier: 1.0,
            });
            samples += std::hint::black_box(bench_judgment_output(
                TapJudgmentHudFrame::render_active(tap, 2.0 + (frame + sample) as f32 / 120.0),
                rows,
                rotation,
                frame + sample,
            ))
            .samples;
        }
        bench_hud_output(frame, samples)
    }

    pub fn new_expired_judgment_frame(&self, frame: usize) -> HudOptionGateBenchFrame {
        let mut samples = 0;
        for sample in 0..HUD_GATE_BENCH_SAMPLES {
            let tap = std::hint::black_box(&self.tap);
            if TapJudgmentHudFrame::render_active(tap, 2.0 + (frame + sample) as f32 / 120.0) {
                samples += self.old_expired_judgment_frame(frame + sample).samples;
            }
        }
        bench_hud_output(frame, samples)
    }
}

#[cfg(feature = "bench-support")]
#[inline(never)]
fn bench_combo_output(show: bool, frame: usize) -> HudOptionGateBenchFrame {
    if !show {
        return bench_hud_output(frame, 0);
    }
    bench_hud_output(frame, usize::from(frame.is_multiple_of(257)) + 1)
}

#[cfg(feature = "bench-support")]
#[inline(never)]
fn bench_error_output(
    error_bar: bool,
    error_ms: bool,
    average_y: f32,
    frame: usize,
) -> HudOptionGateBenchFrame {
    let samples = if error_bar || error_ms {
        usize::from(average_y.is_finite())
    } else {
        0
    };
    bench_hud_output(frame, samples)
}

#[cfg(feature = "bench-support")]
#[inline(never)]
fn bench_judgment_output(
    active: bool,
    rows: (usize, Option<usize>),
    rotation: f32,
    frame: usize,
) -> HudOptionGateBenchFrame {
    let samples = if active {
        rows.0 + rows.1.unwrap_or_default() + usize::from(rotation.is_finite())
    } else {
        0
    };
    bench_hud_output(frame, samples)
}

#[cfg(feature = "bench-support")]
#[inline(always)]
fn bench_hud_output(frame: usize, samples: usize) -> HudOptionGateBenchFrame {
    HudOptionGateBenchFrame {
        checksum: (frame as u64).rotate_left(9) ^ samples as u64,
        samples,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combo_hud_gate_requires_every_visibility_input() {
        let cases = [
            ((false, false, true), true),
            ((true, false, true), false),
            ((false, true, true), false),
            ((false, false, false), false),
        ];
        for ((hide_combo, blind, combo_visible), expected) in cases {
            assert_eq!(
                NotefieldHudFrameView::combo_active(hide_combo, blind, combo_visible),
                expected
            );
        }
    }

    #[test]
    fn error_hud_gate_accepts_either_renderable_output() {
        let cases = [
            ((false, false, false), false),
            ((false, true, false), true),
            ((false, false, true), true),
            ((true, true, true), false),
        ];
        for ((blind, error_bar, error_ms), expected) in cases {
            assert_eq!(
                NotefieldHudFrameView::error_active(blind, error_bar, error_ms),
                expected
            );
        }
    }

    #[test]
    fn offset_indicator_gate_matches_half_open_actor_lifetime() {
        let indicator = Some(OffsetIndicatorText {
            started_at: 1.0,
            offset_ms: 12.0,
            window: deadsync_rules::judgment::TimingWindow::W2,
        });
        assert!(!ErrorBarHudFrame::offset_active(indicator, 0.99, 0.5));
        assert!(ErrorBarHudFrame::offset_active(indicator, 1.0, 0.5));
        assert!(ErrorBarHudFrame::offset_active(indicator, 1.499, 0.5));
        assert!(!ErrorBarHudFrame::offset_active(indicator, 1.5, 0.5));
        assert!(!ErrorBarHudFrame::offset_active(None, 1.0, 0.5));
    }
}
