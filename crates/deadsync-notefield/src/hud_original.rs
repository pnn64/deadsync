use super::{JudgeGrade, TapJudgmentRowsParams, TimingWindow};

pub(crate) const fn itg_actor_glow_alpha(alpha: f32) -> f32 {
    if alpha.is_finite() {
        alpha.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub(crate) fn judgment_actor_zoom(mini: f32, judgment_back: bool, _tilt: f32, _skew: f32) -> f32 {
    if !judgment_back {
        return combo_actor_zoom(mini);
    }
    if mini <= 0.0 || !mini.is_finite() {
        1.0
    } else {
        mini.mul_add(-0.5, 1.0).max(0.35)
    }
}

pub(crate) fn tap_judgment_rows(params: TapJudgmentRowsParams) -> (usize, Option<usize>) {
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

pub(crate) fn error_bar_tick_alpha(age: f32, dur: f32, multi_tick: bool) -> f32 {
    if !age.is_finite() || age < 0.0 {
        return 0.0;
    }
    if multi_tick {
        if age < 0.03 {
            1.0
        } else if age < dur {
            1.0 - (age - 0.03) / (dur - 0.03).max(0.000_001)
        } else {
            0.0
        }
    } else if age < dur {
        1.0
    } else {
        0.0
    }
}

pub fn error_bar_boundaries_s(
    windows: [f32; 5],
    fa_plus_s: Option<f32>,
    show_fa_plus: bool,
    max_window_ix: usize,
) -> ([f32; 6], usize) {
    let mut out = [0.0; 6];
    let mut len = 0;
    if show_fa_plus && let Some(v) = fa_plus_s.filter(|v| v.is_finite() && *v > 0.0) {
        out[len] = v;
        len += 1;
    }
    let max = max_window_ix.min(4);
    for w in windows.iter().take(max + 1).copied() {
        out[len] = w;
        len += 1;
    }
    (out, len)
}

pub(crate) fn combo_actor_zoom(mini: f32) -> f32 {
    if !mini.is_finite() || mini <= 0.0 {
        1.0
    } else {
        0.5_f32.powf(mini)
    }
}
