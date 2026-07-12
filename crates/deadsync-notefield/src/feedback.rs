use crate::*;
use deadlib_present::actors::Actor;
use deadlib_present::dsl::{SpriteBuilder, TextBuilder};
use deadsync_gameplay::{ActiveColumnFlash, ColumnCue, active_column_cue, column_flash_duration};
use deadsync_rules::judgment::{JudgeGrade, TimingWindow};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_theme::{ColumnCueStyle, ColumnFlashLayoutStyle, ColumnFlashStyle, NotefieldStyle};
use std::sync::Arc;

const COLUMN_CUE_FADE_TIME: f32 = 0.15;

#[derive(Clone, Copy, Debug)]
pub struct JudgmentTiltParams {
    pub enabled: bool,
    pub grade: JudgeGrade,
    pub time_error_ms: f32,
    pub min_threshold_ms: f32,
    pub max_threshold_ms: f32,
    pub multiplier: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct TapJudgmentRowsParams {
    pub grade: JudgeGrade,
    pub window: Option<TimingWindow>,
    pub time_error_ms: f32,
    pub frame_rows: usize,
    pub show_fa_plus_window: bool,
    pub fa_plus_10ms_blue_window: bool,
    pub split_15_10ms: bool,
    pub custom_fantastic_window: bool,
}

pub(crate) const fn column_flash_layout(
    style: ColumnFlashStyle,
    compact: bool,
) -> ColumnFlashLayoutStyle {
    if compact {
        style.compact_layout
    } else {
        style.default_layout
    }
}

pub(crate) fn column_flash_height(screen_height: f32, layout: ColumnFlashLayoutStyle) -> f32 {
    (screen_height - layout.top_y - layout.height_trim).max(0.0)
}

pub(crate) fn column_flash_reverse_top_y(
    style: ColumnFlashStyle,
    layout: ColumnFlashLayoutStyle,
    lane_width: f32,
    height: f32,
    center_y: f32,
    receptor_reverse_y: f32,
) -> f32 {
    center_y + receptor_reverse_y - lane_width * 0.5 - height + style.reverse_anchor_y
        - layout.reverse_trim
}

pub(crate) fn column_flash_alpha_at(
    started_at: f32,
    current_time: f32,
    duration: f32,
    base_alpha: f32,
) -> f32 {
    if !current_time.is_finite() || duration <= 0.0 || current_time < started_at {
        return 0.0;
    }
    let t = (current_time - started_at) / duration;
    if t >= 1.0 {
        0.0
    } else {
        base_alpha * (1.0 - t * t)
    }
}

pub(crate) const fn column_flash_base_alpha(style: ColumnFlashStyle, dimmed: bool) -> f32 {
    if dimmed {
        style.dimmed_alpha
    } else {
        style.normal_alpha
    }
}

pub(crate) fn column_flash_alpha(
    style: ColumnFlashStyle,
    started_at: f32,
    current_time: f32,
    duration: f32,
    dimmed: bool,
) -> f32 {
    column_flash_alpha_at(
        started_at,
        current_time,
        duration,
        column_flash_base_alpha(style, dimmed),
    )
}

pub(crate) fn column_flash_color(
    style: ColumnFlashStyle,
    grade: JudgeGrade,
    blue_fantastic: bool,
    alpha: f32,
) -> [f32; 4] {
    let rgb = match grade {
        JudgeGrade::Miss => style.miss_color,
        JudgeGrade::Decent => style.decent_color,
        JudgeGrade::WayOff => style.way_off_color,
        JudgeGrade::Great => style.great_color,
        JudgeGrade::Excellent => style.excellent_color,
        JudgeGrade::Fantastic => {
            if blue_fantastic {
                style.fantastic_blue_color
            } else {
                style.fantastic_color
            }
        }
    };
    [rgb[0], rgb[1], rgb[2], alpha]
}

pub(crate) fn field_effect_height(screen_height: f32, tilt: f32) -> f32 {
    screen_height + tilt.abs() * 200.0
}

pub fn itg_actor_glow_alpha(alpha: f32) -> f32 {
    if alpha.is_finite() {
        alpha.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub const fn hold_glow_color(alpha: f32) -> [f32; 4] {
    [1.0, 1.0, 1.0, alpha]
}

pub(crate) fn column_cue_height(style: ColumnCueStyle, screen_height: f32) -> f32 {
    (screen_height - style.top_y).max(0.0)
}

pub(crate) fn crossover_cue_height(style: ColumnCueStyle, screen_height: f32) -> f32 {
    (column_cue_height(style, screen_height) - style.crossover_height_trim).max(0.0)
}

pub(crate) fn column_cue_reverse_top_y(
    style: ColumnCueStyle,
    lane_width: f32,
    height: f32,
    center_y: f32,
    receptor_reverse_y: f32,
) -> f32 {
    center_y + receptor_reverse_y - lane_width * 0.5 - height + style.reverse_anchor_y
}

pub(crate) fn column_cue_alpha(elapsed_real: f32, duration_real: f32) -> f32 {
    if !elapsed_real.is_finite() || !duration_real.is_finite() {
        return 0.0;
    }
    if elapsed_real < 0.0 || elapsed_real > duration_real {
        return 0.0;
    }
    if duration_real <= COLUMN_CUE_FADE_TIME * 2.0 {
        return 0.0;
    }
    if elapsed_real < COLUMN_CUE_FADE_TIME {
        let t = (elapsed_real / COLUMN_CUE_FADE_TIME).clamp(0.0, 1.0);
        return 1.0 - (1.0 - t) * (1.0 - t);
    }
    if elapsed_real > duration_real - COLUMN_CUE_FADE_TIME {
        let t = ((elapsed_real - (duration_real - COLUMN_CUE_FADE_TIME)) / COLUMN_CUE_FADE_TIME)
            .clamp(0.0, 1.0);
        return 1.0 - t * t;
    }
    1.0
}

#[derive(Clone, Copy)]
pub struct ColumnFeedbackRequest<'a> {
    pub style: NotefieldStyle,
    pub column_cues: Option<&'a [ColumnCue]>,
    pub crossover_cues: Option<&'a [ColumnCue]>,
    pub column_flashes: Option<&'a [Option<ActiveColumnFlash>]>,
    pub regular_countdown: bool,
    pub crossover_countdown: bool,
    pub current_music_time: f32,
    pub current_screen_time: f32,
    pub music_rate: f32,
    pub col_start: usize,
    pub num_cols: usize,
    pub column_xs: &'a [f32],
    pub column_dirs: &'a [f32],
    pub spacing_multiplier: f32,
    pub field_zoom: f32,
    pub playfield_center_x: f32,
    pub field_center_y: f32,
    pub screen_height: f32,
    pub compact_flashes: bool,
    pub dim_flashes: bool,
    pub countdown_font: &'static str,
    pub countdown_text: fn(i32) -> Arc<str>,
}

#[derive(Clone, Copy)]
enum ColumnCueKind {
    Regular,
    Crossover,
}

/// Compose canonical column cues, crossover cues, and miss flashes from
/// gameplay snapshots. Concrete themes supply only style values and font/text
/// resolution; the notefield owns placement, timing, fades, and actor shape.
pub fn compose_column_feedback(
    actors: &mut Vec<Actor>,
    hud_actors: &mut Vec<Actor>,
    request: ColumnFeedbackRequest<'_>,
) {
    if let Some(cues) = request.column_cues {
        compose_column_cue(
            actors,
            hud_actors,
            request,
            cues,
            ColumnCueKind::Regular,
            request.regular_countdown,
        );
    }
    if let Some(cues) = request.crossover_cues {
        compose_column_cue(
            actors,
            hud_actors,
            request,
            cues,
            ColumnCueKind::Crossover,
            request.crossover_countdown,
        );
    }
    if let Some(flashes) = request.column_flashes {
        compose_column_flashes(actors, request, flashes);
    }
}

fn compose_column_cue(
    actors: &mut Vec<Actor>,
    hud_actors: &mut Vec<Actor>,
    request: ColumnFeedbackRequest<'_>,
    cues: &[ColumnCue],
    kind: ColumnCueKind,
    show_countdown: bool,
) {
    let Some(cue) = active_column_cue(cues, request.current_music_time) else {
        return;
    };
    let rate = if request.music_rate.is_finite() && request.music_rate > 0.0 {
        request.music_rate
    } else {
        1.0
    };
    let duration_real = cue.duration / rate;
    let elapsed_real = (request.current_music_time - cue.start_time) / rate;
    let alpha_mul = column_cue_alpha(elapsed_real, duration_real);
    if alpha_mul <= 0.0 {
        return;
    }

    let style = request.style.column_cue;
    let lane_width = ScrollSpeedSetting::ARROW_SPACING * request.field_zoom;
    let cue_height = match kind {
        ColumnCueKind::Regular => column_cue_height(style, request.screen_height),
        ColumnCueKind::Crossover => crossover_cue_height(style, request.screen_height),
    };
    let num_cols = request
        .num_cols
        .min(request.column_xs.len())
        .min(request.column_dirs.len());

    for col_cue in &cue.columns {
        let local_col = col_cue.column.saturating_sub(request.col_start);
        if local_col >= num_cols {
            continue;
        }
        let x = request.playfield_center_x
            + request.column_xs[local_col] * request.spacing_multiplier * request.field_zoom;
        let alpha = style.base_alpha * alpha_mul;
        let rgb = if col_cue.is_mine {
            style.mine_color
        } else {
            style.normal_color
        };
        let reverse = request.column_dirs[local_col] < 0.0;
        let y = if reverse {
            column_cue_reverse_top_y(
                style,
                lane_width,
                cue_height,
                request.field_center_y,
                request.style.receptor_reverse_y,
            )
        } else {
            style.top_y + request.field_center_y
        };
        append_column_quad(
            actors,
            x,
            y,
            lane_width,
            cue_height,
            reverse,
            style.body_fade,
            [rgb[0], rgb[1], rgb[2], alpha],
            style.body_z,
        );
    }

    if !show_countdown || duration_real < 5.0 {
        return;
    }
    let remaining = duration_real - elapsed_real;
    if remaining <= 0.5 {
        return;
    }
    let Some(last_col) = cue.columns.last() else {
        return;
    };
    let local_col = last_col.column.saturating_sub(request.col_start);
    if local_col >= num_cols {
        return;
    }
    let x = request.playfield_center_x
        + request.column_xs[local_col] * request.spacing_multiplier * request.field_zoom;
    let y = request.field_center_y
        + if request.column_dirs[local_col] < 0.0 {
            style.countdown_reverse_y
        } else {
            style.countdown_normal_y
        };
    let mut text = TextBuilder::new();
    text.font(request.countdown_font);
    text.settext((request.countdown_text)(remaining.round() as i32).into());
    text.align(0.5, 0.5);
    text.xy(x, y);
    text.zoom(style.countdown_zoom);
    text.z(style.countdown_z);
    text.diffuse([
        style.countdown_color[0],
        style.countdown_color[1],
        style.countdown_color[2],
        alpha_mul,
    ]);
    hud_actors.push(text.build(0));
}

fn compose_column_flashes(
    actors: &mut Vec<Actor>,
    request: ColumnFeedbackRequest<'_>,
    flashes: &[Option<ActiveColumnFlash>],
) {
    let style = request.style.column_flash;
    let layout = column_flash_layout(style, request.compact_flashes);
    let lane_width = ScrollSpeedSetting::ARROW_SPACING * request.field_zoom;
    let height = column_flash_height(request.screen_height, layout);
    let num_cols = request
        .num_cols
        .min(request.column_xs.len())
        .min(request.column_dirs.len())
        .min(flashes.len());

    for (i, flash) in flashes.iter().take(num_cols).enumerate() {
        let Some(flash) = flash else {
            continue;
        };
        let alpha = column_flash_alpha(
            style,
            flash.started_at_screen_s,
            request.current_screen_time,
            column_flash_duration(flash.grade),
            request.dim_flashes,
        );
        if alpha <= 0.0 {
            continue;
        }
        let x = request.playfield_center_x
            + request.column_xs[i] * request.spacing_multiplier * request.field_zoom;
        let reverse = request.column_dirs[i] < 0.0;
        let y = if reverse {
            column_flash_reverse_top_y(
                style,
                layout,
                lane_width,
                height,
                request.field_center_y,
                request.style.receptor_reverse_y,
            )
        } else {
            layout.top_y + request.field_center_y
        };
        append_column_quad(
            actors,
            x,
            y,
            lane_width,
            height,
            reverse,
            layout.fade,
            column_flash_color(style, flash.grade, flash.blue_fantastic, alpha),
            style.z,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn append_column_quad(
    actors: &mut Vec<Actor>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    reverse: bool,
    fade: f32,
    color: [f32; 4],
    z: i16,
) {
    let mut quad = SpriteBuilder::solid();
    quad.align(0.5, 0.0);
    quad.xy(x, y);
    // A solid actor's native size is 1x1. Keep the same scale representation
    // produced by the theme DSL's `zoomto` command.
    quad.zoomx(width);
    quad.zoomy(height);
    if reverse {
        quad.fadetop(fade);
    } else {
        quad.fadebottom(fade);
    }
    quad.diffuse(color);
    quad.z(z);
    actors.push(quad.build(0));
}

pub fn judgment_tilt_rotation_deg(params: JudgmentTiltParams) -> f32 {
    if !params.enabled || params.grade == JudgeGrade::Miss {
        return 0.0;
    }
    if !params.time_error_ms.is_finite() || !params.multiplier.is_finite() {
        return 0.0;
    }
    let min_ms = params.min_threshold_ms;
    let max_ms = params.max_threshold_ms.max(params.min_threshold_ms);
    let active_ms = params.time_error_ms.abs().min(max_ms) - min_ms;
    if active_ms <= 0.0 {
        return 0.0;
    }
    let dir = if params.time_error_ms < 0.0 {
        1.0
    } else {
        -1.0
    };
    dir * active_ms * 0.3 * params.multiplier
}

pub fn judgment_actor_zoom(mini: f32, judgment_back: bool, _tilt: f32, _skew: f32) -> f32 {
    if !judgment_back {
        return combo_actor_zoom(mini);
    }
    if mini <= 0.0 || !mini.is_finite() {
        1.0
    } else {
        (1.0 - mini * 0.5).max(0.35)
    }
}

pub fn tap_judgment_rows(params: TapJudgmentRowsParams) -> (usize, Option<usize>) {
    if params.frame_rows < 7 {
        let base = match params.grade {
            JudgeGrade::Fantastic => 0,
            JudgeGrade::Excellent => 1,
            JudgeGrade::Great => 2,
            JudgeGrade::Decent => 3,
            JudgeGrade::WayOff => 4,
            JudgeGrade::Miss => 5,
        };
        return (base, None);
    }

    let abs_error_ms = params.time_error_ms.abs();
    let uses_10ms_blue_mode =
        params.fa_plus_10ms_blue_window && !params.split_15_10ms && !params.custom_fantastic_window;
    let split_15_10ms_active = params.show_fa_plus_window
        && params.split_15_10ms
        && !params.custom_fantastic_window
        && params.grade == JudgeGrade::Fantastic
        && abs_error_ms > deadsync_rules::timing::FA_PLUS_W010_MS
        && abs_error_ms <= deadsync_rules::timing::FA_PLUS_W0_MS;

    let base = match params.grade {
        JudgeGrade::Fantastic => {
            let blue_fantastic = !params.show_fa_plus_window
                || if uses_10ms_blue_mode {
                    abs_error_ms <= deadsync_rules::timing::FA_PLUS_W010_MS
                } else {
                    params.window == Some(TimingWindow::W0)
                };
            if split_15_10ms_active || blue_fantastic {
                0
            } else {
                1
            }
        }
        JudgeGrade::Excellent => 2,
        JudgeGrade::Great => 3,
        JudgeGrade::Decent => 4,
        JudgeGrade::WayOff => 5,
        JudgeGrade::Miss => 6,
    };
    let overlay = split_15_10ms_active && params.frame_rows >= 7;
    (base, overlay.then_some(1))
}

pub(crate) fn held_miss_zoom(elapsed: f32, mini: f32) -> (f32, f32) {
    let mini_scale = (1.0 - mini * 0.5).max(0.0);
    if elapsed < 0.1 {
        let t = (elapsed / 0.1).clamp(0.0, 1.0);
        let ease_t = 1.0 - (1.0 - t).powi(2);
        let zoom_x = 0.8 + (0.75 - 0.8) * ease_t;
        return (zoom_x * mini_scale, 0.75 * mini_scale);
    }
    if elapsed < 0.3 {
        return (0.75 * mini_scale, 0.75 * mini_scale);
    }
    let t = ((elapsed - 0.3) / 0.2).clamp(0.0, 1.0);
    let zoom = 0.75 * mini_scale * (1.0 - t.powi(2));
    (zoom, zoom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::{Actor, SizeSpec, SpriteSource};
    use deadsync_gameplay::ColumnCueColumn;
    use deadsync_theme::{
        ColumnFlashLayoutStyle, ColumnFlashStyle, ComboFeedbackStyle, CounterHudStyle,
        ErrorBarLayers, ErrorBarPalette, ErrorBarStyle, JudgmentFeedbackStyle, MiniIndicatorStyle,
        ReceptorStyle,
    };

    fn style() -> NotefieldStyle {
        NotefieldStyle {
            layout_width_min: 640.0,
            layout_width_max: 854.0,
            side_center_x_ratio: 0.25,
            receptor_normal_y: -125.0,
            receptor_reverse_y: 145.0,
            receptor: ReceptorStyle {
                target_z: 100,
                press_glow_z: 105,
                hold_explosion_z: 145,
            },
            judgment_normal_y: -30.0,
            judgment_reverse_y: 30.0,
            judgment_centered_y: 95.0,
            combo_normal_y: 30.0,
            combo_reverse_y: -30.0,
            combo_centered_y: 155.0,
            judgment_height: 40.0,
            error_bar_offset_y: 25.0,
            measure_line_overscan_y: 400.0,
            measure_line_z: 80,
            measure_cue_scroll_color: [0.824, 0.706, 0.549],
            measure_cue_bpm_color: [1.0, 1.0, 0.0],
            measure_cue_delay_color: [1.0, 0.45, 0.75],
            measure_cue_stop_color: [1.0, 0.0, 0.0],
            measure_cue_alpha: 0.7,
            edit_measure_number_font: "edit-font",
            column_cue: ColumnCueStyle {
                top_y: 80.0,
                reverse_anchor_y: 304.0,
                crossover_height_trim: 270.0,
                body_fade: 0.333,
                base_alpha: 0.12,
                normal_color: [0.3, 1.0, 1.0],
                mine_color: [1.0, 0.0, 0.0],
                countdown_normal_y: 160.0,
                countdown_reverse_y: 340.0,
                countdown_color: [1.0, 1.0, 1.0],
                countdown_zoom: 0.5,
                body_z: 90,
                countdown_z: 200,
            },
            column_flash: ColumnFlashStyle {
                default_layout: ColumnFlashLayoutStyle {
                    top_y: 80.0,
                    height_trim: 0.0,
                    reverse_trim: 0.0,
                    fade: 0.333,
                },
                compact_layout: ColumnFlashLayoutStyle {
                    top_y: 70.0,
                    height_trim: 270.0,
                    reverse_trim: 30.0,
                    fade: 0.2,
                },
                reverse_anchor_y: 304.0,
                normal_alpha: 0.66,
                dimmed_alpha: 0.3,
                miss_color: [1.0, 0.0, 0.0],
                decent_color: [0.70, 0.36, 1.0],
                way_off_color: [0.788, 0.522, 0.369],
                great_color: [0.4, 0.788, 0.333],
                excellent_color: [0.886, 0.612, 0.094],
                fantastic_color: [1.0, 1.0, 1.0],
                fantastic_blue_color: [0.129, 0.8, 0.91],
                z: 91,
            },
            counter_hud: CounterHudStyle {
                text_z: 85,
                shadow_len: 1.0,
                base_zoom: 0.35,
                lookahead_zoom_step: 0.05,
                vertical_step_y: 20.0,
                left_column_scale: 4.0 / 3.0,
                horizontal_span: 2.0,
                break_lookahead_color: [0.4, 0.4, 0.4, 1.0],
                break_current_color: [0.5, 0.5, 0.5, 1.0],
                stream_lookahead_color: [0.45, 0.45, 0.45, 1.0],
                ratio_color: [1.0, 1.0, 1.0, 1.0],
                total_color: [0.5, 0.5, 0.5, 1.0],
                broken_y_offset: 15.0,
                broken_vertical_y_offset: -15.0,
                broken_vertical_x_scale: 4.0 / 3.0,
                broken_color: [1.0, 1.0, 1.0, 0.7],
                run_active_color: [1.0, 1.0, 1.0, 1.0],
                run_inactive_color: [0.5, 0.5, 0.5, 1.0],
            },
            mini_indicator: MiniIndicatorStyle {
                column_offset: 1.0,
                under_up_x_offset: -45.0,
                unanchored_x_offset: -12.0,
                failed_color: [0.5, 0.5, 0.5],
                shadow_len: 1.0,
                text_z: 85,
            },
            judgment_feedback: JudgmentFeedbackStyle {
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
            },
            combo_feedback: ComboFeedbackStyle {
                threshold: 4,
                milestone_z: 89,
                number_z: 90,
                number_zoom: 0.75,
                shadow_len: 1.0,
                miss_color: [1.0, 0.0, 0.0, 1.0],
                burst_duration: 0.5,
                burst_start_zoom: 2.0,
                burst_end_zoom: 1.0,
                burst_start_alpha: 0.5,
                burst_rotation_deg: 90.0,
                hundred_start_zoom: 0.25,
                hundred_end_zoom: 2.0,
                hundred_start_alpha: 0.6,
                hundred_start_rotation_deg: 10.0,
                mini_duration: 0.4,
                mini_start_zoom: 0.25,
                mini_end_zoom: 1.8,
                mini_start_alpha: 1.0,
                mini_start_rotation_deg: 10.0,
                thousand_start_zoom: 0.25,
                thousand_end_zoom: 3.0,
                thousand_start_alpha: 0.7,
                thousand_x_travel: 100.0,
            },
            error_bar: ErrorBarStyle {
                colorful_width: 160.0,
                colorful_height: 10.0,
                average_width: 325.0,
                average_height: 7.0,
                monochrome_width: 240.0,
                tick_width: 2.0,
                colorful_border_size: 4.0,
                average_tick_padding: 4.0,
                monochrome_border_size: 2.0,
                monochrome_center_width: 2.0,
                monochrome_line_width: 1.0,
                colorful_tick_duration: 0.5,
                monochrome_tick_duration: 0.75,
                average_tick_extra_height: 75.0,
                monochrome_background_alpha: 0.5,
                line_alpha: 0.3,
                lines_fade_start: 2.5,
                lines_fade_duration: 0.5,
                label_fade_duration: 0.5,
                label_hold: 2.0,
                label_x_ratio: 0.25,
                label_zoom: 0.7,
                center_tick_width: 1.0,
                highlight_inactive_alpha: 0.3,
                offset_indicator_duration: 0.5,
                offset_indicator_gap: 6.0,
                offset_indicator_zoom: 0.25,
                offset_indicator_shadow_len: 1.0,
                long_average_tick_duration: 0.5,
                long_average_tick_extra_height: 65.0,
                long_average_tick_width: 1.0,
                text_duration: 0.5,
                text_x_offset: 40.0,
                text_zoom: 0.25,
                text_shadow_len: 1.0,
                background_color: [0.0, 0.0, 0.0, 1.0],
                monochrome_center_color: [0.5, 0.5, 0.5, 1.0],
                monochrome_line_color: [1.0, 1.0, 1.0, 1.0],
                label_color: [1.0, 1.0, 1.0, 1.0],
                colorful_tick_color: [0.698, 0.0, 0.0, 1.0],
                average_center_tick_color: [1.0, 1.0, 1.0, 0.3],
                long_average_tick_color: [0.0, 0.0, 1.0, 1.0],
                text_early_color: [0.024, 0.416, 0.957, 1.0],
                text_late_color: [1.0, 0.353, 0.306, 1.0],
                text_scaled_early_color: [0.0, 0.318, 0.859, 1.0],
                text_scaled_late_color: [1.0, 0.086, 0.02, 1.0],
                palette: ErrorBarPalette {
                    fantastic_blue: [0.129, 0.8, 0.91, 1.0],
                    fa_plus_white: [1.0, 1.0, 1.0, 1.0],
                    excellent: [0.886, 0.612, 0.094, 1.0],
                    great: [0.4, 0.788, 0.333, 1.0],
                    decent: [0.706, 0.361, 1.0, 1.0],
                    way_off: [0.788, 0.522, 0.369, 1.0],
                },
                label_font: "game",
                offset_indicator_font: "wendy",
                text_font: "wendy",
                early_label: "Early",
                late_label: "Late",
                front_layers: ErrorBarLayers {
                    background: 180,
                    band: 181,
                    line: 182,
                    tick: 183,
                    text: 184,
                },
                back_layers: ErrorBarLayers {
                    background: 86,
                    band: 87,
                    line: 88,
                    tick: 89,
                    text: 90,
                },
                average_z: 88,
            },
        }
    }

    fn countdown_text(value: i32) -> Arc<str> {
        Arc::from(value.to_string())
    }

    fn request<'a>(
        column_cues: Option<&'a [ColumnCue]>,
        crossover_cues: Option<&'a [ColumnCue]>,
        column_flashes: Option<&'a [Option<ActiveColumnFlash>]>,
    ) -> ColumnFeedbackRequest<'a> {
        ColumnFeedbackRequest {
            style: style(),
            column_cues,
            crossover_cues,
            column_flashes,
            regular_countdown: true,
            crossover_countdown: false,
            current_music_time: 2.0,
            current_screen_time: 0.08,
            music_rate: 1.0,
            col_start: 0,
            num_cols: 4,
            column_xs: &[-96.0, -32.0, 32.0, 96.0],
            column_dirs: &[1.0, -1.0, 1.0, -1.0],
            spacing_multiplier: 1.0,
            field_zoom: 1.0,
            playfield_center_x: 320.0,
            field_center_y: 5.0,
            screen_height: 480.0,
            compact_flashes: true,
            dim_flashes: true,
            countdown_font: "countdown-font",
            countdown_text,
        }
    }

    fn assert_quad(
        actor: &Actor,
        offset: [f32; 2],
        scale: [f32; 2],
        tint: [f32; 4],
        reverse: bool,
        fade: f32,
        z: i16,
    ) {
        match actor {
            Actor::Sprite {
                align,
                offset: actual_offset,
                size,
                source,
                tint: actual_tint,
                z: actual_z,
                fadetop,
                fadebottom,
                scale: actual_scale,
                ..
            } => {
                assert_eq!(*align, [0.5, 0.0]);
                assert_eq!(*actual_offset, offset);
                assert!(matches!(size, [SizeSpec::Px(0.0), SizeSpec::Px(0.0)]));
                assert!(matches!(source, SpriteSource::Solid));
                assert_eq!(*actual_scale, scale);
                for (actual, expected) in actual_tint.iter().zip(tint) {
                    assert!((*actual - expected).abs() <= 1e-6);
                }
                assert_eq!(*actual_z, z);
                assert_eq!(*fadetop, if reverse { fade } else { 0.0 });
                assert_eq!(*fadebottom, if reverse { 0.0 } else { fade });
            }
            other => panic!("expected column feedback quad, got {other:?}"),
        }
    }

    #[test]
    fn regular_cue_actor_fingerprint_preserves_reverse_and_countdown() {
        let cues = [ColumnCue {
            start_time: 0.0,
            duration: 6.0,
            columns: vec![
                ColumnCueColumn {
                    column: 0,
                    is_mine: false,
                },
                ColumnCueColumn {
                    column: 1,
                    is_mine: true,
                },
            ],
        }];
        let mut actors = Vec::new();
        let mut hud = Vec::new();

        compose_column_feedback(&mut actors, &mut hud, request(Some(&cues), None, None));

        assert_eq!(actors.len(), 2);
        assert_quad(
            &actors[0],
            [224.0, 85.0],
            [64.0, 400.0],
            [0.3, 1.0, 1.0, 0.12],
            false,
            0.333,
            90,
        );
        assert_quad(
            &actors[1],
            [288.0, 22.0],
            [64.0, 400.0],
            [1.0, 0.0, 0.0, 0.12],
            true,
            0.333,
            90,
        );
        assert_eq!(hud.len(), 1);
        match &hud[0] {
            Actor::Text {
                align,
                offset,
                color,
                font,
                content,
                z,
                scale,
                ..
            } => {
                assert_eq!(*align, [0.5, 0.5]);
                assert_eq!(*offset, [288.0, 345.0]);
                assert_eq!(*color, [1.0, 1.0, 1.0, 1.0]);
                assert_eq!(*font, "countdown-font");
                assert_eq!(content.as_str(), "4");
                assert_eq!(*z, 200);
                assert_eq!(*scale, [0.5, 0.5]);
            }
            other => panic!("expected cue countdown text, got {other:?}"),
        }
    }

    #[test]
    fn crossover_cue_uses_trimmed_height_and_countdown_gate() {
        let cues = [ColumnCue {
            start_time: 0.0,
            duration: 6.0,
            columns: vec![ColumnCueColumn {
                column: 2,
                is_mine: false,
            }],
        }];
        let mut actors = Vec::new();
        let mut hud = Vec::new();

        compose_column_feedback(&mut actors, &mut hud, request(None, Some(&cues), None));

        assert_eq!(actors.len(), 1);
        assert_quad(
            &actors[0],
            [352.0, 85.0],
            [64.0, 130.0],
            [0.3, 1.0, 1.0, 0.12],
            false,
            0.333,
            90,
        );
        assert!(hud.is_empty());
    }

    #[test]
    fn compact_dim_flash_actor_fingerprint_preserves_both_directions() {
        let flashes = [
            Some(ActiveColumnFlash {
                grade: JudgeGrade::Miss,
                blue_fantastic: false,
                started_at_screen_s: 0.0,
            }),
            Some(ActiveColumnFlash {
                grade: JudgeGrade::Miss,
                blue_fantastic: false,
                started_at_screen_s: 0.0,
            }),
            None,
            None,
        ];
        let mut actors = Vec::new();
        let mut hud = Vec::new();

        compose_column_feedback(&mut actors, &mut hud, request(None, None, Some(&flashes)));

        assert_eq!(actors.len(), 2);
        assert_quad(
            &actors[0],
            [224.0, 75.0],
            [64.0, 140.0],
            [1.0, 0.0, 0.0, 0.225_000_01],
            false,
            0.2,
            91,
        );
        assert_quad(
            &actors[1],
            [288.0, 252.0],
            [64.0, 140.0],
            [1.0, 0.0, 0.0, 0.225_000_01],
            true,
            0.2,
            91,
        );
        assert!(hud.is_empty());
    }

    #[test]
    fn expired_feedback_and_out_of_range_columns_emit_nothing() {
        let cues = [ColumnCue {
            start_time: 0.0,
            duration: 1.0,
            columns: vec![ColumnCueColumn {
                column: 9,
                is_mine: false,
            }],
        }];
        let flashes = [Some(ActiveColumnFlash {
            grade: JudgeGrade::Miss,
            blue_fantastic: false,
            started_at_screen_s: 0.0,
        })];
        let mut req = request(Some(&cues), None, Some(&flashes));
        req.current_music_time = 2.0;
        req.current_screen_time = 2.0;
        req.music_rate = f32::NAN;
        req.col_start = 4;
        let mut actors = Vec::new();
        let mut hud = Vec::new();

        compose_column_feedback(&mut actors, &mut hud, req);

        assert!(actors.is_empty());
        assert!(hud.is_empty());
    }
}
