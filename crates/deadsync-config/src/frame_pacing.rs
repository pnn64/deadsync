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

/// Background redraw cap used while the window is occluded, inactive, or safely
/// throttled while unfocused.
pub const BACKGROUND_REDRAW_INTERVAL: Duration = Duration::from_millis(67);

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
pub fn window_frame_interval_state(
    vsync_enabled: bool,
    max_fps_interval: Option<Duration>,
    window_occluded: bool,
    surface_active: bool,
    window_focused: bool,
    throttle_unfocused: bool,
) -> FrameIntervalState {
    let base = (!vsync_enabled).then_some(max_fps_interval).flatten();
    let background =
        (window_occluded || !surface_active || (!window_focused && throttle_unfocused))
            .then_some(BACKGROUND_REDRAW_INTERVAL);
    match (base, background) {
        (Some(base), Some(background)) => FrameIntervalState {
            interval: Some(base.max(background)),
            reason: FrameIntervalReason::MaxFpsBackground,
        },
        (Some(interval), None) => FrameIntervalState {
            interval: Some(interval),
            reason: FrameIntervalReason::MaxFps,
        },
        (None, Some(interval)) => FrameIntervalState {
            interval: Some(interval),
            reason: FrameIntervalReason::Background,
        },
        (None, None) => FrameIntervalState {
            interval: None,
            reason: FrameIntervalReason::None,
        },
    }
}

#[inline(always)]
pub const fn foreground_input_active(window_focused: bool, surface_active: bool) -> bool {
    window_focused && surface_active
}

#[inline(always)]
pub const fn should_skip_compose_and_draw(window_occluded: bool, surface_active: bool) -> bool {
    window_occluded || !surface_active
}

#[inline(always)]
pub const fn queued_input_allowed(
    screen_is_gameplay: bool,
    transition_idle: bool,
    transition_fading_in: bool,
) -> bool {
    transition_idle || (screen_is_gameplay && transition_fading_in)
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

#[derive(Clone, Copy, Debug)]
pub struct RedrawRequestTiming {
    pub request_to_redraw_us: u32,
    pub reason: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct RedrawRequestState {
    requested_at: Option<Instant>,
    reason: &'static str,
}

impl RedrawRequestState {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            requested_at: None,
            reason: "none",
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    #[inline(always)]
    pub fn note_requested(&mut self, now: Instant, reason: &'static str) {
        if self.requested_at.is_none() {
            self.requested_at = Some(now);
            self.reason = reason;
        }
    }

    #[inline(always)]
    pub fn take_timing(&mut self, now: Instant) -> RedrawRequestTiming {
        let requested_at = self.requested_at.take();
        let reason = if requested_at.is_some() {
            self.reason
        } else {
            "external"
        };
        self.reason = "none";
        RedrawRequestTiming {
            request_to_redraw_us: requested_at
                .map(|at| elapsed_us_between(now, at))
                .unwrap_or_default(),
            reason,
        }
    }

    #[inline(always)]
    pub const fn pending(&self) -> bool {
        self.requested_at.is_some()
    }
}

impl Default for RedrawRequestState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameLoopModeTracker {
    last: Option<FrameLoopMode>,
}

impl FrameLoopModeTracker {
    #[inline(always)]
    pub const fn new() -> Self {
        Self { last: None }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.last = None;
    }

    #[inline(always)]
    pub fn note(&mut self, mode: FrameLoopMode) -> bool {
        if self.last == Some(mode) {
            return false;
        }
        self.last = Some(mode);
        true
    }
}

pub const STUTTER_SAMPLE_COUNT: usize = 5;
pub const STUTTER_SAMPLE_LIFETIME: f32 = 3.4;

#[derive(Clone, Copy, Debug)]
pub struct StutterSample {
    pub at_seconds: f32,
    pub frame_seconds: f32,
    pub expected_seconds: f32,
    pub severity: u8,
}

impl StutterSample {
    #[inline(always)]
    pub const fn empty() -> Self {
        Self {
            at_seconds: -1.0,
            frame_seconds: 0.0,
            expected_seconds: 0.0,
            severity: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct VisibleStutterSample {
    pub timestamp_seconds: f32,
    pub frame_ms: f32,
    pub frame_multiple: f32,
    pub severity: u8,
    pub age_seconds: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct StutterSampleRing {
    samples: [StutterSample; STUTTER_SAMPLE_COUNT],
    cursor: usize,
}

impl Default for StutterSampleRing {
    fn default() -> Self {
        Self::new()
    }
}

impl StutterSampleRing {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            samples: [StutterSample::empty(); STUTTER_SAMPLE_COUNT],
            cursor: 0,
        }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    #[inline(always)]
    pub fn push(
        &mut self,
        at_seconds: f32,
        frame_seconds: f32,
        expected_seconds: f32,
        severity: u8,
    ) {
        self.samples[self.cursor] = StutterSample {
            at_seconds,
            frame_seconds,
            expected_seconds,
            severity,
        };
        self.cursor = (self.cursor + 1) % STUTTER_SAMPLE_COUNT;
    }

    pub fn visible(self, now_seconds: f32) -> Vec<VisibleStutterSample> {
        let mut out = Vec::with_capacity(STUTTER_SAMPLE_COUNT);
        for i in 0..STUTTER_SAMPLE_COUNT {
            let sample = self.samples[(self.cursor + i) % STUTTER_SAMPLE_COUNT];
            if sample.severity == 0 {
                continue;
            }
            let age_seconds = now_seconds - sample.at_seconds;
            if !(0.0..=STUTTER_SAMPLE_LIFETIME).contains(&age_seconds) {
                continue;
            }
            let frame_multiple = if sample.expected_seconds > 0.0 {
                sample.frame_seconds / sample.expected_seconds
            } else {
                0.0
            };
            out.push(VisibleStutterSample {
                timestamp_seconds: sample.at_seconds,
                frame_ms: sample.frame_seconds * 1000.0,
                frame_multiple,
                severity: sample.severity,
                age_seconds,
            });
        }
        out
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

    /// Copy samples whose timestamp is inside `window_ns`, in chronological order.
    pub fn collect_recent_by(
        &self,
        now_host_nanos: u64,
        window_ns: u64,
        out: &mut Vec<T>,
        timestamp: impl Fn(T) -> u64,
    ) {
        out.clear();
        if N == 0 {
            return;
        }
        let start = self.cursor.saturating_add(N).saturating_sub(self.len) % N;
        for i in 0..self.len {
            let sample = self.samples[(start + i) % N];
            let sample_time = timestamp(sample);
            if sample_time != 0 && now_host_nanos.saturating_sub(sample_time) <= window_ns {
                out.push(sample);
            }
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
    fn window_frame_interval_uses_background_when_occluded() {
        let state = window_frame_interval_state(true, None, true, true, true, false);
        assert_eq!(
            state,
            FrameIntervalState {
                interval: Some(BACKGROUND_REDRAW_INTERVAL),
                reason: FrameIntervalReason::Background,
            }
        );
    }

    #[test]
    fn window_frame_interval_combines_max_fps_and_background() {
        let max_fps = Some(Duration::from_millis(5));
        let state = window_frame_interval_state(false, max_fps, false, true, false, true);
        assert_eq!(
            state,
            FrameIntervalState {
                interval: Some(BACKGROUND_REDRAW_INTERVAL),
                reason: FrameIntervalReason::MaxFpsBackground,
            }
        );
    }

    #[test]
    fn window_frame_interval_respects_unfocused_no_throttle() {
        let max_fps = Some(Duration::from_millis(5));
        let state = window_frame_interval_state(false, max_fps, false, true, false, false);
        assert_eq!(
            state,
            FrameIntervalState {
                interval: max_fps,
                reason: FrameIntervalReason::MaxFps,
            }
        );
    }

    #[test]
    fn foreground_input_requires_focus_and_surface() {
        assert!(foreground_input_active(true, true));
        assert!(!foreground_input_active(false, true));
        assert!(!foreground_input_active(true, false));
        assert!(!foreground_input_active(false, false));
    }

    #[test]
    fn compose_draw_skips_occluded_or_inactive_surface() {
        assert!(!should_skip_compose_and_draw(false, true));
        assert!(should_skip_compose_and_draw(true, true));
        assert!(should_skip_compose_and_draw(false, false));
    }

    #[test]
    fn queued_input_dispatch_allows_gameplay_fade_in_only() {
        assert!(queued_input_allowed(false, true, false));
        assert!(queued_input_allowed(true, false, true));
        assert!(!queued_input_allowed(false, false, true));
        assert!(!queued_input_allowed(true, false, false));
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
    fn redraw_request_state_latches_first_reason_until_taken() {
        let now = Instant::now();
        let mut state = RedrawRequestState::new();

        state.note_requested(now, "input");
        state.note_requested(now + Duration::from_micros(100), "chain");
        assert!(state.pending());

        let timing = state.take_timing(now + Duration::from_micros(250));

        assert_eq!(timing.request_to_redraw_us, 250);
        assert_eq!(timing.reason, "input");
        assert!(!state.pending());
    }

    #[test]
    fn redraw_request_state_marks_external_when_unrequested() {
        let mut state = RedrawRequestState::new();
        let timing = state.take_timing(Instant::now());

        assert_eq!(timing.request_to_redraw_us, 0);
        assert_eq!(timing.reason, "external");
    }

    #[test]
    fn frame_loop_mode_tracker_reports_only_changes() {
        let mut tracker = FrameLoopModeTracker::new();

        assert!(tracker.note(FrameLoopMode::Poll));
        assert!(!tracker.note(FrameLoopMode::Poll));
        assert!(tracker.note(FrameLoopMode::WaitPending));
        tracker.reset();
        assert!(tracker.note(FrameLoopMode::WaitPending));
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

    #[test]
    fn fixed_frame_stats_ring_collects_recent_samples() {
        let mut ring = FixedFrameStatsRing::<u64, 4>::new(0);
        let mut out = Vec::new();

        ring.push(10);
        ring.push(80);
        ring.push(120);
        ring.push(0);
        ring.collect_recent_by(130, 50, &mut out, |sample| sample);

        assert_eq!(out, vec![80, 120]);
    }

    #[test]
    fn stutter_sample_ring_filters_visible_samples() {
        let mut ring = StutterSampleRing::new();

        ring.push(1.0, 0.050, 0.016, 2);
        ring.push(2.0, 0.010, 0.016, 0);
        ring.push(-10.0, 0.100, 0.016, 3);

        let visible = ring.visible(3.0);

        assert_eq!(visible.len(), 1);
        assert!((visible[0].timestamp_seconds - 1.0).abs() < EPS);
        assert!((visible[0].frame_ms - 50.0).abs() < EPS);
        assert!((visible[0].frame_multiple - (0.050 / 0.016)).abs() < EPS);
        assert_eq!(visible[0].severity, 2);
        assert!((visible[0].age_seconds - 2.0).abs() < EPS);
    }

    #[test]
    fn stutter_sample_ring_overwrites_oldest_sample() {
        let mut ring = StutterSampleRing::new();

        for i in 0..(STUTTER_SAMPLE_COUNT + 1) {
            ring.push(i as f32 * 0.1, 0.040, 0.016, 1);
        }

        let visible = ring.visible(STUTTER_SAMPLE_COUNT as f32 * 0.1);

        assert_eq!(visible.len(), STUTTER_SAMPLE_COUNT);
        assert!((visible[0].timestamp_seconds - 0.1).abs() < EPS);
        assert!(
            (visible.last().unwrap().timestamp_seconds - STUTTER_SAMPLE_COUNT as f32 * 0.1).abs()
                < EPS
        );
    }
}
