use std::time::{Duration, Instant};

/// Hold-Tab fast-forward / hold-` slow-down multipliers, ITGmania parity.
///
/// `Tab` alone -> `TAB_FAST_MULTIPLIER`x engine update rate.
/// `` ` `` alone -> `1.0 / TAB_SLOW_DIVISOR`x rate.
/// Both held -> `0.0` (halt).
pub const TAB_FAST_MULTIPLIER: f32 = 4.0;
pub const TAB_SLOW_DIVISOR: f32 = 4.0;

/// Upper bound on the post-acceleration logic dt fed to screens.
///
/// Prevents catastrophic stalls from injecting absurd dt values into
/// per-screen update accumulators when fast-forward is held.
pub const MAX_LOGIC_DT_PER_FRAME: f32 = 0.25;

#[inline]
pub fn apply_tab_acceleration(
    wall_dt: f32,
    acceleration_allowed: bool,
    fast: bool,
    slow: bool,
    enabled: bool,
) -> f32 {
    if !enabled || !acceleration_allowed {
        return wall_dt;
    }
    let scaled = match (fast, slow) {
        (true, true) => 0.0,
        (true, false) => wall_dt * TAB_FAST_MULTIPLIER,
        (false, true) => wall_dt / TAB_SLOW_DIVISOR,
        (false, false) => wall_dt,
    };
    scaled.clamp(0.0, MAX_LOGIC_DT_PER_FRAME)
}

#[inline(always)]
pub fn frame_interval_for_max_fps(max_fps: u16) -> Option<Duration> {
    if max_fps == 0 {
        None
    } else {
        Some(Duration::from_secs_f64(1.0 / f64::from(max_fps)))
    }
}

#[inline(always)]
pub fn advance_redraw_deadline(deadline: Instant, now: Instant, interval: Duration) -> Instant {
    if deadline > now {
        return deadline;
    }
    let step_ns = interval.as_nanos();
    if step_ns == 0 {
        return now;
    }
    let overdue_ns = now.duration_since(deadline).as_nanos();
    let steps = overdue_ns / step_ns + 1;
    if steps <= u128::from(u32::MAX)
        && let Some(delta) = interval.checked_mul(steps as u32)
        && let Some(next) = deadline.checked_add(delta)
    {
        return next;
    }
    now.checked_add(interval).unwrap_or(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-6;

    #[test]
    fn tab_accel_no_modifier_is_passthrough() {
        let dt = 0.016_f32;
        assert!((apply_tab_acceleration(dt, true, false, false, true) - dt).abs() < EPS);
    }

    #[test]
    fn tab_accel_fast_multiplies_by_four() {
        let dt = 0.016_f32;
        let out = apply_tab_acceleration(dt, true, true, false, true);
        assert!((out - dt * 4.0).abs() < EPS, "got {out}");
    }

    #[test]
    fn tab_accel_slow_divides_by_four() {
        let dt = 0.016_f32;
        let out = apply_tab_acceleration(dt, true, false, true, true);
        assert!((out - dt / 4.0).abs() < EPS, "got {out}");
    }

    #[test]
    fn tab_accel_both_held_halts() {
        let dt = 0.016_f32;
        let out = apply_tab_acceleration(dt, true, true, true, true);
        assert_eq!(out, 0.0);
    }

    #[test]
    fn tab_accel_disallowed_never_scales() {
        let dt = 0.016_f32;
        for (fast, slow) in [(false, false), (true, false), (false, true), (true, true)] {
            let out = apply_tab_acceleration(dt, false, fast, slow, true);
            assert!((out - dt).abs() < EPS);
        }
    }

    #[test]
    fn tab_accel_disabled_never_scales() {
        let dt = 0.016_f32;
        for (fast, slow) in [(false, false), (true, false), (false, true), (true, true)] {
            let out = apply_tab_acceleration(dt, true, fast, slow, false);
            assert!((out - dt).abs() < EPS);
        }
    }

    #[test]
    fn tab_accel_clamps_to_max_logic_dt() {
        let out = apply_tab_acceleration(1.0, true, true, false, true);
        assert_eq!(out, MAX_LOGIC_DT_PER_FRAME);
    }

    #[test]
    fn tab_accel_clamp_does_not_affect_normal_frames() {
        let dt = 0.016_f32;
        let out = apply_tab_acceleration(dt, true, true, false, true);
        assert!(out < MAX_LOGIC_DT_PER_FRAME);
        assert!((out - 4.0 * dt).abs() < EPS);
    }

    #[test]
    fn max_fps_zero_has_no_interval() {
        assert_eq!(frame_interval_for_max_fps(0), None);
    }

    #[test]
    fn max_fps_interval_uses_fps_period() {
        assert_eq!(
            frame_interval_for_max_fps(60),
            Some(Duration::from_secs_f64(1.0 / 60.0))
        );
    }

    #[test]
    fn redraw_deadline_keeps_future_deadline() {
        let now = Instant::now();
        let deadline = now + Duration::from_millis(16);
        assert_eq!(
            advance_redraw_deadline(deadline, now, Duration::from_millis(16)),
            deadline
        );
    }

    #[test]
    fn redraw_deadline_advances_past_now() {
        let now = Instant::now();
        let deadline = now - Duration::from_millis(33);
        let next = advance_redraw_deadline(deadline, now, Duration::from_millis(16));
        assert!(next > now);
    }
}
