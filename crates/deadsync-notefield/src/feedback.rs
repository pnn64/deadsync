use crate::style::*;
use crate::*;
use deadsync_rules::judgment::{JudgeGrade, TimingWindow};

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnFlashLayout {
    pub y_offset: f32,
    pub height_trim: f32,
    pub fade: f32,
}

pub const fn column_flash_layout(compact: bool) -> ColumnFlashLayout {
    if compact {
        ColumnFlashLayout {
            y_offset: COLUMN_FLASH_COMPACT_Y_OFFSET,
            height_trim: COLUMN_FLASH_COMPACT_HEIGHT_TRIM,
            fade: COLUMN_FLASH_COMPACT_FADE,
        }
    } else {
        ColumnFlashLayout {
            y_offset: COLUMN_FLASH_DEFAULT_Y_OFFSET,
            height_trim: 0.0,
            fade: COLUMN_FLASH_DEFAULT_FADE,
        }
    }
}

pub fn column_flash_height(screen_height: f32, layout: ColumnFlashLayout) -> f32 {
    (screen_height - layout.y_offset - layout.height_trim).max(0.0)
}

pub fn column_flash_reverse_bottom_y(
    layout: ColumnFlashLayout,
    lane_width: f32,
    height: f32,
    center_y: f32,
    receptor_reverse_y: f32,
) -> f32 {
    column_flash_reverse_top_y(layout, lane_width, height, center_y, receptor_reverse_y) + height
}

pub fn column_flash_reverse_top_y(
    layout: ColumnFlashLayout,
    lane_width: f32,
    height: f32,
    center_y: f32,
    receptor_reverse_y: f32,
) -> f32 {
    center_y + receptor_reverse_y - lane_width * 0.5 - height + 304.0 - layout.height_trim / 9.0
}

pub fn column_flash_alpha_at(
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

pub const fn column_flash_base_alpha(dimmed: bool) -> f32 {
    if dimmed {
        COLUMN_FLASH_DIMMED_ALPHA
    } else {
        COLUMN_FLASH_NORMAL_ALPHA
    }
}

pub fn column_flash_alpha(started_at: f32, current_time: f32, duration: f32, dimmed: bool) -> f32 {
    column_flash_alpha_at(
        started_at,
        current_time,
        duration,
        column_flash_base_alpha(dimmed),
    )
}

pub fn column_flash_color(grade: JudgeGrade, blue_fantastic: bool, alpha: f32) -> [f32; 4] {
    let mut color = match grade {
        JudgeGrade::Miss => [1.0, 0.0, 0.0, alpha],
        JudgeGrade::Decent => [0.70, 0.36, 1.00, alpha],
        JudgeGrade::WayOff => [WAY_OFF_RGBA[0], WAY_OFF_RGBA[1], WAY_OFF_RGBA[2], alpha],
        JudgeGrade::Great => [GREAT_RGBA[0], GREAT_RGBA[1], GREAT_RGBA[2], alpha],
        JudgeGrade::Excellent => [
            EXCELLENT_RGBA[0],
            EXCELLENT_RGBA[1],
            EXCELLENT_RGBA[2],
            alpha,
        ],
        JudgeGrade::Fantastic => {
            if blue_fantastic {
                [
                    FANTASTIC_BLUE_RGBA[0],
                    FANTASTIC_BLUE_RGBA[1],
                    FANTASTIC_BLUE_RGBA[2],
                    alpha,
                ]
            } else {
                [1.0, 1.0, 1.0, alpha]
            }
        }
    };
    color[3] = alpha;
    color
}

pub fn field_effect_height(screen_height: f32, tilt: f32) -> f32 {
    screen_height + tilt.abs() * 200.0
}

pub fn signed_effect_active(value: f32) -> bool {
    value.is_finite() && value.abs() > f32::EPSILON
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

pub fn column_cue_height(screen_height: f32) -> f32 {
    (screen_height - COLUMN_CUE_Y_OFFSET).max(0.0)
}

pub fn crossover_cue_height(screen_height: f32) -> f32 {
    (column_cue_height(screen_height) - CROSSOVER_CUE_HEIGHT_REDUCTION).max(0.0)
}

pub fn column_cue_reverse_bottom_y(
    lane_width: f32,
    height: f32,
    center_y: f32,
    receptor_reverse_y: f32,
) -> f32 {
    column_cue_reverse_top_y(lane_width, height, center_y, receptor_reverse_y) + height
}

pub fn column_cue_reverse_top_y(
    lane_width: f32,
    height: f32,
    center_y: f32,
    receptor_reverse_y: f32,
) -> f32 {
    center_y + receptor_reverse_y - lane_width * 0.5 - height + 304.0
}

pub fn column_cue_alpha(elapsed_real: f32, duration_real: f32) -> f32 {
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

    let base = match params.grade {
        JudgeGrade::Fantastic => {
            if params.custom_fantastic_window {
                params
                    .window
                    .map(|w| w as usize)
                    .unwrap_or(0)
                    .min(params.frame_rows.saturating_sub(1))
            } else if params.show_fa_plus_window
                && params.fa_plus_10ms_blue_window
                && !params.split_15_10ms
                && params.time_error_ms.abs() > deadsync_rules::timing::FA_PLUS_W010_MS
            {
                1
            } else {
                0
            }
        }
        JudgeGrade::Excellent => 2,
        JudgeGrade::Great => 3,
        JudgeGrade::Decent => 4,
        JudgeGrade::WayOff => 5,
        JudgeGrade::Miss => 6,
    };
    let overlay = params.show_fa_plus_window
        && params.split_15_10ms
        && !params.custom_fantastic_window
        && params.frame_rows >= 7
        && params.window == Some(TimingWindow::W0)
        && params.time_error_ms.abs() > deadsync_rules::timing::FA_PLUS_W010_MS;
    (base, overlay.then_some(1))
}

pub fn held_miss_zoom(elapsed: f32, mini: f32) -> (f32, f32) {
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
