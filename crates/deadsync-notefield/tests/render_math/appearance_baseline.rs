// Frozen from 0.5.1217 (d77a218a1).
use crate::style::*;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NoteAlphaParams {
    pub hidden: f32,
    pub hidden_offset: f32,
    pub sudden: f32,
    pub sudden_offset: f32,
    pub stealth: f32,
    pub blink: f32,
    pub random_vanish: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NoteAppearanceCache {
    identity: bool,
    path: AppearancePath,
    center_line: f32,
    hidden_active: bool,
    hidden: f32,
    hidden_end: f32,
    hidden_start: f32,
    hidden_denom: f32,
    hidden_degenerate: bool,
    hidden_bounds_finite: bool,
    sudden_active: bool,
    sudden: f32,
    sudden_end: f32,
    sudden_start: f32,
    sudden_denom: f32,
    sudden_degenerate: bool,
    sudden_bounds_finite: bool,
    stealth_active: bool,
    stealth: f32,
    blink_adjust: f32,
    random_vanish_active: bool,
    random_vanish: f32,
    combined_fade_low_y: f32,
    combined_fade_high_y: f32,
    combined_fade_low_alpha: f32,
    combined_fade_high_alpha: f32,
}

#[derive(Clone, Copy, Debug)]
enum AppearancePath {
    General,
    HiddenOnly,
    SuddenOnly,
    StealthOnly,
    BlinkOnly,
    HiddenSuddenOnly,
    StealthBlinkOnly,
    HiddenStealthOnly,
    SuddenStealthOnly,
    HiddenSuddenStealthOnly,
    HiddenBlinkOnly,
    SuddenBlinkOnly,
    HiddenSuddenBlinkOnly,
    HiddenStealthBlinkOnly,
    SuddenStealthBlinkOnly,
    HiddenSuddenStealthBlinkOnly,
}

pub(crate) fn sm_scale(v: f32, in0: f32, in1: f32, out0: f32, out1: f32) -> f32 {
    let denom = in1 - in0;
    if denom.abs() < 1e-6 {
        return out1;
    }
    ((v - in0) / denom).mul_add(out1 - out0, out0)
}

pub(crate) fn quantize_step(v: f32, step: f32) -> f32 {
    if !v.is_finite() || !step.is_finite() || step == 0.0 {
        0.0
    } else {
        (step.mul_add(0.5, v) / step).trunc() * step
    }
}

#[inline(always)]
pub(crate) fn appearance_note_alpha_is_identity(params: NoteAlphaParams) -> bool {
    params.hidden == 0.0
        && params.sudden == 0.0
        && params.stealth == 0.0
        && params.blink == 0.0
        && params.random_vanish == 0.0
}

#[inline(always)]
pub(crate) fn note_appearance_cache(
    elapsed: f32,
    mini: f32,
    params: NoteAlphaParams,
) -> NoteAppearanceCache {
    if appearance_note_alpha_is_identity(params) {
        return NoteAppearanceCache {
            identity: true,
            path: AppearancePath::General,
            center_line: 0.0,
            hidden_active: false,
            hidden: 0.0,
            hidden_end: 0.0,
            hidden_start: 0.0,
            hidden_denom: 0.0,
            hidden_degenerate: false,
            hidden_bounds_finite: true,
            sudden_active: false,
            sudden: 0.0,
            sudden_end: 0.0,
            sudden_start: 0.0,
            sudden_denom: 0.0,
            sudden_degenerate: false,
            sudden_bounds_finite: true,
            stealth_active: false,
            stealth: 0.0,
            blink_adjust: 0.0,
            random_vanish_active: false,
            random_vanish: 0.0,
            combined_fade_low_y: 0.0,
            combined_fade_high_y: 0.0,
            combined_fade_low_alpha: 1.0,
            combined_fade_high_alpha: 1.0,
        };
    }
    let zoom = mini.mul_add(-0.5, 1.0).abs().max(0.01);
    let center_line = CENTER_LINE_Y / zoom;
    let hidden_sudden = params.hidden * params.sudden;
    let hidden_end = FADE_DIST_Y
        .mul_add(sm_scale(hidden_sudden, 0.0, 1.0, -1.0, -1.25), center_line)
        + center_line * params.hidden_offset;
    let hidden_start = FADE_DIST_Y
        .mul_add(sm_scale(hidden_sudden, 0.0, 1.0, 0.0, -0.25), center_line)
        + center_line * params.hidden_offset;
    let sudden_end = FADE_DIST_Y.mul_add(sm_scale(hidden_sudden, 0.0, 1.0, 0.0, 0.25), center_line)
        + center_line * params.sudden_offset;
    let sudden_start = FADE_DIST_Y
        .mul_add(sm_scale(hidden_sudden, 0.0, 1.0, 1.0, 1.25), center_line)
        + center_line * params.sudden_offset;
    let blink_adjust = if params.blink > f32::EPSILON {
        let blink = quantize_step((elapsed * 10.0).sin(), BLINK_MOD_FREQUENCY);
        sm_scale(blink, 0.0, 1.0, -1.0, 0.0)
    } else {
        0.0
    };
    let hidden_denom = hidden_end - hidden_start;
    let sudden_denom = sudden_end - sudden_start;
    let hidden_bounds_finite =
        hidden_end.is_finite() && hidden_start.is_finite() && hidden_denom.is_finite();
    let sudden_bounds_finite =
        sudden_end.is_finite() && sudden_start.is_finite() && sudden_denom.is_finite();
    let hidden_active = params.hidden > f32::EPSILON;
    let sudden_active = params.sudden > f32::EPSILON;
    let stealth_active = params.stealth > f32::EPSILON;
    let blink_active = params.blink > f32::EPSILON;
    let random_vanish_active = params.random_vanish > f32::EPSILON;
    let mut combined_fade_low_adjust = 0.0;
    combined_fade_low_adjust = params.hidden.mul_add(-1.0, combined_fade_low_adjust);
    combined_fade_low_adjust = params.sudden.mul_add(0.0, combined_fade_low_adjust);
    combined_fade_low_adjust -= params.stealth;
    combined_fade_low_adjust += blink_adjust;
    let mut combined_fade_high_adjust = 0.0;
    combined_fade_high_adjust = params.hidden.mul_add(0.0, combined_fade_high_adjust);
    combined_fade_high_adjust = params.sudden.mul_add(-1.0, combined_fade_high_adjust);
    combined_fade_high_adjust -= params.stealth;
    combined_fade_high_adjust += blink_adjust;
    let path = match (
        hidden_active,
        sudden_active,
        stealth_active,
        blink_active,
        random_vanish_active,
    ) {
        (true, false, false, false, false) => AppearancePath::HiddenOnly,
        (false, true, false, false, false) => AppearancePath::SuddenOnly,
        (false, false, true, false, false) => AppearancePath::StealthOnly,
        (false, false, false, true, false) => AppearancePath::BlinkOnly,
        (true, true, false, false, false)
            if hidden_bounds_finite
                && sudden_bounds_finite
                && hidden_denom.abs() >= 1e-6
                && sudden_denom.abs() >= 1e-6 =>
        {
            AppearancePath::HiddenSuddenOnly
        }
        (false, false, true, true, false) => AppearancePath::StealthBlinkOnly,
        (true, false, true, false, false) => AppearancePath::HiddenStealthOnly,
        (false, true, true, false, false) => AppearancePath::SuddenStealthOnly,
        (true, true, true, false, false)
            if hidden_bounds_finite
                && sudden_bounds_finite
                && hidden_denom.abs() >= 1e-6
                && sudden_denom.abs() >= 1e-6 =>
        {
            AppearancePath::HiddenSuddenStealthOnly
        }
        (true, false, false, true, false) => AppearancePath::HiddenBlinkOnly,
        (false, true, false, true, false) => AppearancePath::SuddenBlinkOnly,
        (true, true, false, true, false)
            if hidden_bounds_finite
                && sudden_bounds_finite
                && hidden_denom.abs() >= 1e-6
                && sudden_denom.abs() >= 1e-6 =>
        {
            AppearancePath::HiddenSuddenBlinkOnly
        }
        (true, false, true, true, false) if hidden_bounds_finite && hidden_denom.abs() >= 1e-6 => {
            AppearancePath::HiddenStealthBlinkOnly
        }
        (false, true, true, true, false) if sudden_bounds_finite && sudden_denom.abs() >= 1e-6 => {
            AppearancePath::SuddenStealthBlinkOnly
        }
        (true, true, true, true, false)
            if hidden_bounds_finite
                && sudden_bounds_finite
                && hidden_denom.abs() >= 1e-6
                && sudden_denom.abs() >= 1e-6 =>
        {
            AppearancePath::HiddenSuddenStealthBlinkOnly
        }
        _ => AppearancePath::General,
    };
    NoteAppearanceCache {
        identity: false,
        path,
        center_line,
        hidden_active,
        hidden: params.hidden,
        hidden_end,
        hidden_start,
        hidden_denom,
        hidden_degenerate: hidden_denom.abs() < 1e-6,
        hidden_bounds_finite,
        sudden_active,
        sudden: params.sudden,
        sudden_end,
        sudden_start,
        sudden_denom,
        sudden_degenerate: sudden_denom.abs() < 1e-6,
        sudden_bounds_finite,
        stealth_active,
        stealth: params.stealth,
        blink_adjust,
        random_vanish_active,
        random_vanish: params.random_vanish,
        combined_fade_low_y: hidden_end.min(sudden_end),
        combined_fade_high_y: hidden_start.max(sudden_start),
        combined_fade_low_alpha: (1.0 + combined_fade_low_adjust).clamp(0.0, 1.0),
        combined_fade_high_alpha: (1.0 + combined_fade_high_adjust).clamp(0.0, 1.0),
    }
}

#[inline(always)]
pub(crate) fn appearance_note_alpha_glow_cached(y: f32, cache: &NoteAppearanceCache) -> (f32, f32) {
    let percent_visible = appearance_note_alpha_cached(y, cache);
    (
        appearance_note_actor_alpha_from_alpha(percent_visible),
        appearance_note_glow_from_alpha(percent_visible),
    )
}

#[inline(always)]
fn hidden_fade_scaled_bounded(y: f32, cache: &NoteAppearanceCache) -> f32 {
    if cache.hidden_degenerate {
        -1.0
    } else if !cache.hidden_bounds_finite {
        ((y - cache.hidden_start) / cache.hidden_denom).mul_add(-1.0, 0.0)
    } else if y <= cache.hidden_end {
        -1.0
    } else if y >= cache.hidden_start {
        0.0
    } else {
        ((y - cache.hidden_start) / cache.hidden_denom).mul_add(-1.0, 0.0)
    }
}

#[inline(always)]
fn sudden_fade_scaled_bounded(y: f32, cache: &NoteAppearanceCache) -> f32 {
    if cache.sudden_degenerate {
        0.0
    } else if !cache.sudden_bounds_finite {
        ((y - cache.sudden_start) / cache.sudden_denom).mul_add(1.0, -1.0)
    } else if y <= cache.sudden_end {
        0.0
    } else if y >= cache.sudden_start {
        -1.0
    } else {
        ((y - cache.sudden_start) / cache.sudden_denom).mul_add(1.0, -1.0)
    }
}

#[inline(always)]
fn hidden_fade_scaled_finite(y: f32, cache: &NoteAppearanceCache) -> f32 {
    if y <= cache.hidden_end {
        -1.0
    } else if y >= cache.hidden_start {
        0.0
    } else {
        ((y - cache.hidden_start) / cache.hidden_denom).mul_add(-1.0, 0.0)
    }
}

#[inline(always)]
fn sudden_fade_scaled_finite(y: f32, cache: &NoteAppearanceCache) -> f32 {
    if y <= cache.sudden_end {
        0.0
    } else if y >= cache.sudden_start {
        -1.0
    } else {
        ((y - cache.sudden_start) / cache.sudden_denom).mul_add(1.0, -1.0)
    }
}

#[inline(always)]
pub(crate) fn appearance_note_alpha_cached(y: f32, cache: &NoteAppearanceCache) -> f32 {
    if cache.identity || y < 0.0 {
        return 1.0;
    }
    match cache.path {
        AppearancePath::HiddenOnly => {
            let scaled = hidden_fade_scaled_bounded(y, cache);
            let visible_adjust = cache.hidden.mul_add(scaled.clamp(-1.0, 0.0), 0.0);
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::SuddenOnly => {
            let scaled = sudden_fade_scaled_bounded(y, cache);
            let visible_adjust = cache.sudden.mul_add(scaled.clamp(-1.0, 0.0), 0.0);
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::StealthOnly => {
            let mut visible_adjust = 0.0;
            visible_adjust -= cache.stealth;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::BlinkOnly => {
            let mut visible_adjust = 0.0;
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenSuddenOnly => {
            if y <= cache.combined_fade_low_y {
                return cache.combined_fade_low_alpha;
            }
            if y >= cache.combined_fade_high_y {
                return cache.combined_fade_high_alpha;
            }
            let mut visible_adjust = 0.0;
            let hidden_scaled = hidden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(hidden_scaled.clamp(-1.0, 0.0), visible_adjust);
            let sudden_scaled = sudden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(sudden_scaled.clamp(-1.0, 0.0), visible_adjust);
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::StealthBlinkOnly => {
            let mut visible_adjust = 0.0;
            visible_adjust -= cache.stealth;
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenStealthOnly => {
            let mut visible_adjust = 0.0;
            let scaled = hidden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust -= cache.stealth;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::SuddenStealthOnly => {
            let mut visible_adjust = 0.0;
            let scaled = sudden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust -= cache.stealth;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenSuddenStealthOnly => {
            if y <= cache.combined_fade_low_y {
                return cache.combined_fade_low_alpha;
            }
            if y >= cache.combined_fade_high_y {
                return cache.combined_fade_high_alpha;
            }
            let mut visible_adjust = 0.0;
            let hidden_scaled = hidden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(hidden_scaled.clamp(-1.0, 0.0), visible_adjust);
            let sudden_scaled = sudden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(sudden_scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust -= cache.stealth;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenBlinkOnly => {
            let mut visible_adjust = 0.0;
            let scaled = hidden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::SuddenBlinkOnly => {
            let mut visible_adjust = 0.0;
            let scaled = sudden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenSuddenBlinkOnly => {
            if y <= cache.combined_fade_low_y {
                return cache.combined_fade_low_alpha;
            }
            if y >= cache.combined_fade_high_y {
                return cache.combined_fade_high_alpha;
            }
            let mut visible_adjust = 0.0;
            let hidden_scaled = hidden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(hidden_scaled.clamp(-1.0, 0.0), visible_adjust);
            let sudden_scaled = sudden_fade_scaled_bounded(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(sudden_scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenStealthBlinkOnly => {
            let mut visible_adjust = 0.0;
            let scaled = hidden_fade_scaled_finite(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust -= cache.stealth;
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::SuddenStealthBlinkOnly => {
            let mut visible_adjust = 0.0;
            let scaled = sudden_fade_scaled_finite(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust -= cache.stealth;
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::HiddenSuddenStealthBlinkOnly => {
            if y <= cache.combined_fade_low_y {
                return cache.combined_fade_low_alpha;
            }
            if y >= cache.combined_fade_high_y {
                return cache.combined_fade_high_alpha;
            }
            let mut visible_adjust = 0.0;
            let hidden_scaled = hidden_fade_scaled_finite(y, cache);
            visible_adjust = cache
                .hidden
                .mul_add(hidden_scaled.clamp(-1.0, 0.0), visible_adjust);
            let sudden_scaled = sudden_fade_scaled_finite(y, cache);
            visible_adjust = cache
                .sudden
                .mul_add(sudden_scaled.clamp(-1.0, 0.0), visible_adjust);
            visible_adjust -= cache.stealth;
            visible_adjust += cache.blink_adjust;
            return (1.0 + visible_adjust).clamp(0.0, 1.0);
        }
        AppearancePath::General => {}
    }
    appearance_note_alpha_general(y, cache)
}

#[inline(always)]
fn appearance_note_alpha_general(y: f32, cache: &NoteAppearanceCache) -> f32 {
    let mut visible_adjust = 0.0;
    if cache.hidden_active {
        let scaled = if cache.hidden_degenerate {
            -1.0
        } else {
            ((y - cache.hidden_start) / cache.hidden_denom).mul_add(-1.0, 0.0)
        };
        visible_adjust = cache
            .hidden
            .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
    }
    if cache.sudden_active {
        let scaled = if cache.sudden_degenerate {
            0.0
        } else {
            ((y - cache.sudden_start) / cache.sudden_denom).mul_add(1.0, -1.0)
        };
        visible_adjust = cache
            .sudden
            .mul_add(scaled.clamp(-1.0, 0.0), visible_adjust);
    }
    if cache.stealth_active {
        visible_adjust -= cache.stealth;
    }
    visible_adjust += cache.blink_adjust;
    if cache.random_vanish_active {
        let dist = (y - cache.center_line).abs();
        visible_adjust += sm_scale(dist, 80.0, 160.0, -1.0, 0.0) * cache.random_vanish;
    }
    (1.0 + visible_adjust).clamp(0.0, 1.0)
}
#[inline(always)]
pub(crate) fn appearance_note_glow_from_alpha(percent_visible: f32) -> f32 {
    sm_scale((percent_visible - 0.5).abs(), 0.0, 0.5, 1.3, 0.0).max(0.0)
}

#[inline(always)]
pub(crate) fn appearance_note_actor_alpha_from_alpha(percent_visible: f32) -> f32 {
    if percent_visible > 0.5 { 1.0 } else { 0.0 }
}
