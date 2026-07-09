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

#[inline(always)]
pub fn elapsed_us_since(started: Instant) -> u32 {
    micros_to_u32(started.elapsed().as_micros())
}

#[inline(always)]
pub fn elapsed_us_between(later: Instant, earlier: Instant) -> u32 {
    micros_to_u32(
        later
            .checked_duration_since(earlier)
            .unwrap_or(Duration::ZERO)
            .as_micros(),
    )
}

#[inline(always)]
pub fn seconds_to_us_u32(seconds: f32) -> u32 {
    let micros = (seconds * 1_000_000.0).max(0.0);
    if micros > u32::MAX as f32 {
        u32::MAX
    } else {
        micros as u32
    }
}

#[inline(always)]
const fn micros_to_u32(micros: u128) -> u32 {
    if micros > u32::MAX as u128 {
        u32::MAX
    } else {
        micros as u32
    }
}

/// Slow-decay "worst recent frame" hold for max readouts and graph scale.
/// New highs latch instantly and hold briefly; afterwards the value eases down
/// geometrically toward the current frame so it tracks recovery without snapping.
#[inline(always)]
pub fn update_frame_stats_spike_hold(spike_us: &mut u32, ttl: &mut u16, frame_us: u32) {
    const SPIKE_HOLD_FRAMES: u16 = 90;
    if frame_us >= *spike_us {
        *spike_us = frame_us;
        *ttl = SPIKE_HOLD_FRAMES;
    } else if *ttl > 0 {
        *ttl -= 1;
    } else {
        let decayed = (u64::from(*spike_us) * 31 / 32) as u32;
        *spike_us = decayed.max(frame_us);
    }
}

#[inline(always)]
pub fn stutter_severity(frame_seconds: f32, expected_seconds: f32) -> u8 {
    if expected_seconds <= 0.0 {
        return 0;
    }
    let thresholds = [expected_seconds * 2.0, expected_seconds * 4.0, 0.1];
    let mut severity: u8 = 0;
    while usize::from(severity) < thresholds.len()
        && frame_seconds > thresholds[usize::from(severity)]
    {
        severity = severity.saturating_add(1);
    }
    severity
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayMode {
    Off,
    Fps,
    FpsAndStutter,
    FpsStutterTiming,
}

impl OverlayMode {
    #[inline(always)]
    pub const fn from_code(mode: u8) -> Self {
        match mode {
            1 => Self::Fps,
            2 => Self::FpsAndStutter,
            3 => Self::FpsStutterTiming,
            _ => Self::Off,
        }
    }

    #[inline(always)]
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::Fps,
            Self::Fps => Self::FpsAndStutter,
            Self::FpsAndStutter => Self::FpsStutterTiming,
            Self::FpsStutterTiming => Self::Off,
        }
    }

    #[inline(always)]
    pub const fn shows_fps(self) -> bool {
        !matches!(self, Self::Off)
    }

    #[inline(always)]
    pub const fn shows_stutter(self) -> bool {
        matches!(self, Self::FpsAndStutter | Self::FpsStutterTiming)
    }

    #[inline(always)]
    pub const fn shows_timing(self) -> bool {
        matches!(self, Self::FpsStutterTiming)
    }

    #[inline(always)]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Fps => "FPS",
            Self::FpsAndStutter => "FPS+STUTTER",
            Self::FpsStutterTiming => "FPS+STUTTER+TIMING",
        }
    }

    #[inline(always)]
    pub const fn code(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Fps => 1,
            Self::FpsAndStutter => 2,
            Self::FpsStutterTiming => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameIntervalReason {
    None,
    MaxFps,
    Background,
    MaxFpsBackground,
}

impl FrameIntervalReason {
    #[inline(always)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::MaxFps => "max_fps",
            Self::Background => "background",
            Self::MaxFpsBackground => "max_fps+background",
        }
    }

    #[inline(always)]
    pub const fn redraw_reason(self) -> &'static str {
        match self {
            Self::None => "scheduled",
            Self::MaxFps => "scheduled_maxfps",
            Self::Background => "scheduled_background",
            Self::MaxFpsBackground => "scheduled_maxfps_background",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameIntervalState {
    pub interval: Option<Duration>,
    pub reason: FrameIntervalReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameLoopMode {
    Poll,
    WaitPending,
    Scheduled(FrameIntervalReason, Duration),
}

#[derive(Clone, Copy)]
pub struct GameplayPacingTrace {
    pub started_at: Instant,
    pub frames: u32,
    pub chain_frames: u32,
    pub other_frames: u32,
    pub dt_sum_us: u64,
    pub dt_max_us: u32,
    pub redraw_late_sum_us: u64,
    pub redraw_late_max_us: u32,
    pub redraw_delivery_sum_us: u64,
    pub redraw_delivery_max_us: u32,
    pub redraw_delivery_over_1ms: u32,
    pub redraw_delivery_over_2ms: u32,
    pub draw_sum_us: u64,
    pub draw_max_us: u32,
    pub present_sum_us: u64,
    pub present_max_us: u32,
    pub present_over_1ms: u32,
    pub present_over_3ms: u32,
    pub draw_setup_sum_us: u64,
    pub draw_prepare_sum_us: u64,
    pub draw_record_sum_us: u64,
    pub display_error_abs_sum_us: u64,
    pub display_error_abs_max_us: u32,
    pub display_error_last_us: i32,
    pub display_catching_up_frames: u32,
    pub display_catching_up_last: bool,
    pub present_last_mode: deadlib_render::PresentModeTrace,
    pub present_display_clock_last: deadlib_render::ClockDomainTrace,
    pub present_host_clock_last: deadlib_render::ClockDomainTrace,
    pub present_inflight_sum: u64,
    pub present_inflight_max: u8,
    pub present_image_wait_frames: u32,
    pub present_back_pressure_frames: u32,
    pub present_queue_idle_frames: u32,
    pub present_suboptimal_frames: u32,
    pub present_host_mapped_frames: u32,
    pub present_calibration_error_sum_ns: u64,
    pub present_calibration_error_max_ns: u64,
    pub present_interval_sum_ns: u64,
    pub present_interval_max_ns: u64,
    pub present_interval_samples: u32,
    pub present_margin_sum_ns: u64,
    pub present_margin_max_ns: u64,
    pub present_margin_samples: u32,
}

impl GameplayPacingTrace {
    #[inline(always)]
    pub fn new(now: Instant) -> Self {
        Self {
            started_at: now,
            frames: 0,
            chain_frames: 0,
            other_frames: 0,
            dt_sum_us: 0,
            dt_max_us: 0,
            redraw_late_sum_us: 0,
            redraw_late_max_us: 0,
            redraw_delivery_sum_us: 0,
            redraw_delivery_max_us: 0,
            redraw_delivery_over_1ms: 0,
            redraw_delivery_over_2ms: 0,
            draw_sum_us: 0,
            draw_max_us: 0,
            present_sum_us: 0,
            present_max_us: 0,
            present_over_1ms: 0,
            present_over_3ms: 0,
            draw_setup_sum_us: 0,
            draw_prepare_sum_us: 0,
            draw_record_sum_us: 0,
            display_error_abs_sum_us: 0,
            display_error_abs_max_us: 0,
            display_error_last_us: 0,
            display_catching_up_frames: 0,
            display_catching_up_last: false,
            present_last_mode: deadlib_render::PresentModeTrace::Unknown,
            present_display_clock_last: deadlib_render::ClockDomainTrace::Unknown,
            present_host_clock_last: deadlib_render::ClockDomainTrace::Unknown,
            present_inflight_sum: 0,
            present_inflight_max: 0,
            present_image_wait_frames: 0,
            present_back_pressure_frames: 0,
            present_queue_idle_frames: 0,
            present_suboptimal_frames: 0,
            present_host_mapped_frames: 0,
            present_calibration_error_sum_ns: 0,
            present_calibration_error_max_ns: 0,
            present_interval_sum_ns: 0,
            present_interval_max_ns: 0,
            present_interval_samples: 0,
            present_margin_sum_ns: 0,
            present_margin_max_ns: 0,
            present_margin_samples: 0,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self, now: Instant) {
        *self = Self::new(now);
    }
}

/// Fixed-size copy ring for frame diagnostic samples.
///
/// The app owns when samples are produced; this type owns the storage policy:
/// no heap allocation, overwrite oldest on overflow, snapshot oldest-first.
#[derive(Clone, Copy)]
pub struct FixedFrameStatsRing<T: Copy, const N: usize> {
    samples: [T; N],
    empty: T,
    cursor: usize,
    len: usize,
}

impl<T: Copy, const N: usize> FixedFrameStatsRing<T, N> {
    #[inline(always)]
    pub const fn new(empty: T) -> Self {
        Self {
            samples: [empty; N],
            empty,
            cursor: 0,
            len: 0,
        }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.samples = [self.empty; N];
        self.cursor = 0;
        self.len = 0;
    }

    #[inline(always)]
    pub fn push(&mut self, sample: T) {
        if N == 0 {
            return;
        }
        self.samples[self.cursor] = sample;
        self.cursor = (self.cursor + 1) % N;
        self.len = self.len.saturating_add(1).min(N);
    }

    /// Copy the ring into `out` in chronological order.
    pub fn snapshot(&self, out: &mut Vec<T>) {
        out.clear();
        if N == 0 {
            return;
        }
        let start = self.cursor.saturating_add(N).saturating_sub(self.len) % N;
        for i in 0..self.len {
            out.push(self.samples[(start + i) % N]);
        }
    }
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

    #[test]
    fn fixed_frame_stats_ring_snapshots_oldest_first() {
        let mut ring = FixedFrameStatsRing::<u8, 3>::new(0);
        let mut out = Vec::new();

        ring.push(1);
        ring.push(2);
        ring.push(3);
        ring.push(4);
        ring.snapshot(&mut out);

        assert_eq!(out, vec![2, 3, 4]);
        ring.clear();
        ring.snapshot(&mut out);
        assert!(out.is_empty());
    }
}
