pub const SCOREBOX_LOOP_SECONDS: f32 = 5.0;
pub const SCOREBOX_TRANSITION_SECONDS: f32 = 1.0;
pub const SCOREBOX_W: f32 = 162.0;
pub const SCOREBOX_H: f32 = 80.0;
pub const SCOREBOX_BORDER: f32 = 5.0;
pub const SCOREBOX_LOGO_MAX_W_FRAC: f32 = 0.94;
pub const SCOREBOX_LOGO_MAX_H_FRAC: f32 = 0.94;
/// Online fetches that resolve inside this window never show a loading label.
/// The box keeps its frame with blank rows until the data lands, so a fast
/// service does not flash "Loading ..." for a few frames.
pub const SCOREBOX_LOADING_GRACE_SECONDS: f32 = 0.4;
/// Rows fade in over this long once a pending fetch resolves.
pub const SCOREBOX_REVEAL_SECONDS: f32 = 0.15;

/// Where one Select Music side's online fetch is on the screen's
/// selection-animation clock. The theme advances it from each shell snapshot
/// and reads it every frame to decide between a blank pending box, a loading
/// label, and how far the rows have faded in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScoreboxFetchPhase {
    pub loading_since: Option<f32>,
    pub ready_at: Option<f32>,
}

impl ScoreboxFetchPhase {
    /// Advance from the previous shell snapshot to the current one.
    /// `previous_loading` and `current_loading` are `None` when that snapshot
    /// carried no online leaderboards at all.
    #[must_use]
    pub fn advanced(
        self,
        same_chart: bool,
        previous_loading: Option<bool>,
        current_loading: Option<bool>,
        now: f32,
    ) -> Self {
        let Some(current_loading) = current_loading else {
            return Self::default();
        };
        if !same_chart {
            return Self {
                loading_since: current_loading.then_some(now),
                ready_at: None,
            };
        }
        match (previous_loading, current_loading) {
            (Some(true), false) => Self {
                ready_at: Some(now),
                ..self
            },
            (Some(false) | None, true) => Self {
                loading_since: Some(now),
                ready_at: None,
            },
            (Some(true), true) => Self {
                loading_since: self.loading_since.or(Some(now)),
                ready_at: None,
            },
            (Some(false) | None, false) => self,
        }
    }

    /// True while a fetch is young enough that no loading label should show.
    #[inline(always)]
    #[must_use]
    pub fn within_loading_grace(self, now: f32) -> bool {
        self.loading_since
            .is_some_and(|since| now - since < SCOREBOX_LOADING_GRACE_SECONDS)
    }

    /// Row alpha for a fetch that resolved at `ready_at`; 1.0 when nothing is
    /// fading in.
    #[inline(always)]
    #[must_use]
    pub fn reveal_alpha(self, now: f32) -> f32 {
        self.ready_at
            .map_or(1.0, |ready_at| clamp01((now - ready_at) / SCOREBOX_REVEAL_SECONDS))
    }
}

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
#[must_use]
pub const fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

#[inline(always)]
#[must_use]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    let t = clamp01(t);
    (b - a).mul_add(t, a)
}

#[inline(always)]
#[must_use]
pub fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
        lerp(a[3], b[3], t),
    ]
}

#[inline(always)]
#[must_use]
pub fn color_with_alpha(mut rgba: [f32; 4], alpha: f32) -> [f32; 4] {
    rgba[3] *= clamp01(alpha);
    rgba
}

#[must_use]
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

#[must_use]
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
    let cycle_pos = (cycle_num as f32).mul_add(-cycle_len, elapsed);
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

#[must_use]
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

#[cfg(test)]
mod fetch_phase_tests {
    use super::*;

    #[test]
    fn new_chart_starts_a_fresh_phase() {
        let stale = ScoreboxFetchPhase {
            loading_since: Some(1.0),
            ready_at: Some(2.0),
        };
        assert_eq!(
            stale.advanced(false, Some(false), Some(true), 10.0),
            ScoreboxFetchPhase {
                loading_since: Some(10.0),
                ready_at: None,
            }
        );
        assert_eq!(
            stale.advanced(false, None, Some(false), 10.0),
            ScoreboxFetchPhase::default(),
            "a cached chart arrives ready and never fades in"
        );
        assert_eq!(
            stale.advanced(true, Some(true), None, 10.0),
            ScoreboxFetchPhase::default()
        );
    }

    #[test]
    fn same_chart_tracks_loading_start_and_ready_time() {
        let started = ScoreboxFetchPhase::default().advanced(true, None, Some(true), 3.0);
        assert_eq!(started.loading_since, Some(3.0));
        assert!(started.ready_at.is_none());

        let still_loading = started.advanced(true, Some(true), Some(true), 3.2);
        assert_eq!(still_loading.loading_since, Some(3.0));

        let ready = still_loading.advanced(true, Some(true), Some(false), 3.3);
        assert_eq!(ready.loading_since, Some(3.0));
        assert_eq!(ready.ready_at, Some(3.3));

        let retained = ready.advanced(true, Some(false), Some(false), 9.0);
        assert_eq!(retained, ready);

        let refetch = retained.advanced(true, Some(false), Some(true), 20.0);
        assert_eq!(
            refetch,
            ScoreboxFetchPhase {
                loading_since: Some(20.0),
                ready_at: None,
            }
        );
    }

    #[test]
    fn grace_and_reveal_follow_the_clock() {
        let phase = ScoreboxFetchPhase::default().advanced(true, None, Some(true), 5.0);
        assert!(phase.within_loading_grace(5.0));
        assert!(phase.within_loading_grace(5.0 + SCOREBOX_LOADING_GRACE_SECONDS * 0.5));
        assert!(!phase.within_loading_grace(5.0 + SCOREBOX_LOADING_GRACE_SECONDS));
        assert!(!ScoreboxFetchPhase::default().within_loading_grace(5.0));

        let ready = phase.advanced(true, Some(true), Some(false), 5.2);
        assert_eq!(ready.reveal_alpha(5.2), 0.0);
        assert!((ready.reveal_alpha(5.2 + SCOREBOX_REVEAL_SECONDS * 0.5) - 0.5).abs() < 1e-4);
        assert_eq!(ready.reveal_alpha(5.2 + SCOREBOX_REVEAL_SECONDS), 1.0);
        assert_eq!(ScoreboxFetchPhase::default().reveal_alpha(0.0), 1.0);
    }
}
