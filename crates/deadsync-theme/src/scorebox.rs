pub const SCOREBOX_LOOP_SECONDS: f32 = 5.0;
pub const SCOREBOX_TRANSITION_SECONDS: f32 = 1.0;
pub const SCOREBOX_W: f32 = 162.0;
pub const SCOREBOX_H: f32 = 80.0;
pub const SCOREBOX_BORDER: f32 = 5.0;
pub const SCOREBOX_LOGO_MAX_W_FRAC: f32 = 0.94;
pub const SCOREBOX_LOGO_MAX_H_FRAC: f32 = 0.94;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoreboxCycleState {
    pub cur_idx: usize,
    pub next_idx: usize,
    pub border_mix: f32,
    pub cur_alpha: f32,
    pub next_alpha: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoreboxLogoFit {
    pub width: f32,
    pub height: f32,
}

#[inline(always)]
pub fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

#[inline(always)]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    let t = clamp01(t);
    a + (b - a) * t
}

#[inline(always)]
pub fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
        lerp(a[3], b[3], t),
    ]
}

#[inline(always)]
pub fn color_with_alpha(mut rgba: [f32; 4], alpha: f32) -> [f32; 4] {
    rgba[3] *= clamp01(alpha);
    rgba
}

pub fn logo_alpha(
    cycle: ScoreboxCycleState,
    cur_on: bool,
    next_on: bool,
    target: f32,
    enter_in_second_half: bool,
) -> f32 {
    if cycle.cur_idx == cycle.next_idx {
        return if cur_on { target } else { 0.0 };
    }

    let t = cycle.border_mix;
    let start = if cur_on { target } else { 0.0 };
    if enter_in_second_half {
        if next_on {
            if t < 0.5 {
                start
            } else {
                lerp(start, target, (t - 0.5) * 2.0)
            }
        } else if t < 0.5 {
            lerp(start, 0.0, t * 2.0)
        } else {
            0.0
        }
    } else if next_on {
        if t < 0.5 {
            lerp(start, target, t * 2.0)
        } else {
            target
        }
    } else if t < 0.5 {
        start
    } else {
        lerp(start, 0.0, (t - 0.5) * 2.0)
    }
}

pub fn scorebox_cycle_state(num_panes: usize, elapsed_seconds: f32) -> ScoreboxCycleState {
    if num_panes <= 1 {
        return ScoreboxCycleState {
            cur_idx: 0,
            next_idx: 0,
            border_mix: 0.0,
            cur_alpha: 1.0,
            next_alpha: 0.0,
        };
    }

    let cycle_len = SCOREBOX_LOOP_SECONDS + SCOREBOX_TRANSITION_SECONDS;
    let elapsed = elapsed_seconds.max(0.0);
    let cycle_num = (elapsed / cycle_len).floor() as usize;
    let cycle_pos = elapsed - (cycle_num as f32) * cycle_len;
    let cur_idx = cycle_num % num_panes;

    if cycle_pos < SCOREBOX_LOOP_SECONDS {
        return ScoreboxCycleState {
            cur_idx,
            next_idx: cur_idx,
            border_mix: 0.0,
            cur_alpha: 1.0,
            next_alpha: 0.0,
        };
    }

    let next_idx = (cur_idx + 1) % num_panes;
    let t = clamp01((cycle_pos - SCOREBOX_LOOP_SECONDS) / SCOREBOX_TRANSITION_SECONDS);
    let (cur_alpha, next_alpha) = if t < 0.5 {
        (1.0 - t * 2.0, 0.0)
    } else {
        (0.0, (t - 0.5) * 2.0)
    };

    ScoreboxCycleState {
        cur_idx,
        next_idx,
        border_mix: t,
        cur_alpha,
        next_alpha,
    }
}

pub fn fit_scorebox_logo(
    texture_w: u32,
    texture_h: u32,
    sprite_zoom: f32,
    zoom: f32,
) -> ScoreboxLogoFit {
    let mut width = texture_w.max(1) as f32 * sprite_zoom * zoom;
    let mut height = texture_h.max(1) as f32 * sprite_zoom * zoom;
    let max_width = SCOREBOX_W * SCOREBOX_LOGO_MAX_W_FRAC * zoom;
    let max_height = SCOREBOX_H * SCOREBOX_LOGO_MAX_H_FRAC * zoom;
    if width > 0.0 && height > 0.0 {
        let fit = (max_width / width).min(max_height / height).min(1.0);
        width *= fit;
        height *= fit;
    }
    ScoreboxLogoFit { width, height }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scorebox_cycle_state_holds_and_transitions() {
        assert_eq!(
            scorebox_cycle_state(1, 99.0),
            ScoreboxCycleState {
                cur_idx: 0,
                next_idx: 0,
                border_mix: 0.0,
                cur_alpha: 1.0,
                next_alpha: 0.0,
            }
        );

        assert_eq!(scorebox_cycle_state(3, 4.5).cur_idx, 0);
        let mid = scorebox_cycle_state(3, 5.25);
        assert_eq!(mid.cur_idx, 0);
        assert_eq!(mid.next_idx, 1);
        assert_eq!(mid.border_mix, 0.25);
        assert_eq!(mid.cur_alpha, 0.5);
        assert_eq!(mid.next_alpha, 0.0);

        let late = scorebox_cycle_state(3, 5.75);
        assert_eq!(late.border_mix, 0.75);
        assert_eq!(late.cur_alpha, 0.0);
        assert_eq!(late.next_alpha, 0.5);
        assert_eq!(scorebox_cycle_state(3, 6.0).cur_idx, 1);
    }

    #[test]
    fn logo_alpha_respects_half_timing() {
        let cycle = ScoreboxCycleState {
            cur_idx: 0,
            next_idx: 1,
            border_mix: 0.25,
            cur_alpha: 0.5,
            next_alpha: 0.0,
        };

        assert_eq!(logo_alpha(cycle, true, false, 0.8, true), 0.4);
        assert_eq!(logo_alpha(cycle, false, true, 0.8, true), 0.0);
        assert_eq!(logo_alpha(cycle, false, true, 0.8, false), 0.4);
    }

    #[test]
    fn logo_fit_caps_to_scorebox_bounds() {
        let fit = fit_scorebox_logo(1000, 500, 1.0, 1.0);
        assert_eq!(fit.height, SCOREBOX_H * SCOREBOX_LOGO_MAX_H_FRAC);
        assert_eq!(fit.width, fit.height * 2.0);

        let tiny = fit_scorebox_logo(10, 10, 0.5, 2.0);
        assert_eq!(tiny.width, 10.0);
        assert_eq!(tiny.height, 10.0);
    }
}
