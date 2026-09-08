/// Hold-`Tab` fast-forward / hold-`Backquote` slow-down multipliers, `ITGmania` parity.
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
#[must_use]
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
#[must_use]
pub const fn queued_input_allowed(
    screen_is_gameplay: bool,
    transition_idle: bool,
    transition_fading_in: bool,
) -> bool {
    transition_idle || (screen_is_gameplay && transition_fading_in)
}

#[inline(always)]
#[must_use]
pub fn seconds_to_us_u32(seconds: f32) -> u32 {
    let micros = (seconds * 1_000_000.0).max(0.0);
    if micros > u32::MAX as f32 {
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
#[must_use]
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
    #[must_use]
    pub const fn from_code(mode: u8) -> Self {
        match mode {
            1 => Self::Fps,
            2 => Self::FpsAndStutter,
            3 => Self::FpsStutterTiming,
            _ => Self::Off,
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::Fps,
            Self::Fps => Self::FpsAndStutter,
            Self::FpsAndStutter => Self::FpsStutterTiming,
            Self::FpsStutterTiming => Self::Off,
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn shows_fps(self) -> bool {
        !matches!(self, Self::Off)
    }

    #[inline(always)]
    #[must_use]
    pub const fn shows_stutter(self) -> bool {
        matches!(self, Self::FpsAndStutter | Self::FpsStutterTiming)
    }

    #[inline(always)]
    #[must_use]
    pub const fn shows_timing(self) -> bool {
        matches!(self, Self::FpsStutterTiming)
    }

    #[inline(always)]
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Fps => "FPS",
            Self::FpsAndStutter => "FPS+STUTTER",
            Self::FpsStutterTiming => "FPS+STUTTER+TIMING",
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Fps => 1,
            Self::FpsAndStutter => 2,
            Self::FpsStutterTiming => 3,
        }
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
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            at_seconds: -1.0,
            frame_seconds: 0.0,
            expected_seconds: 0.0,
            severity: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisibleStutterSample {
    pub timestamp_seconds: f32,
    pub frame_ms: f32,
    pub frame_multiple: f32,
    pub severity: u8,
    pub age_seconds: f32,
}

const EMPTY_VISIBLE_STUTTER: VisibleStutterSample = VisibleStutterSample {
    timestamp_seconds: 0.0,
    frame_ms: 0.0,
    frame_multiple: 0.0,
    severity: 0,
    age_seconds: 0.0,
};

/// Fixed-capacity visible view of the five-entry stutter ring.
#[derive(Clone, Copy, Debug)]
pub struct VisibleStutterSamples {
    samples: [VisibleStutterSample; STUTTER_SAMPLE_COUNT],
    len: u8,
}

impl VisibleStutterSamples {
    #[inline(always)]
    const fn new() -> Self {
        Self {
            samples: [EMPTY_VISIBLE_STUTTER; STUTTER_SAMPLE_COUNT],
            len: 0,
        }
    }

    #[inline(always)]
    fn push(&mut self, sample: VisibleStutterSample) {
        debug_assert!((self.len as usize) < STUTTER_SAMPLE_COUNT);
        self.samples[self.len as usize] = sample;
        self.len += 1;
    }
}

impl std::ops::Deref for VisibleStutterSamples {
    type Target = [VisibleStutterSample];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.samples[..self.len as usize]
    }
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
    #[must_use]
    pub const fn new() -> Self {
        Self {
            samples: [StutterSample::empty(); STUTTER_SAMPLE_COUNT],
            cursor: 0,
        }
    }

    #[inline(always)]
    pub const fn clear(&mut self) {
        *self = Self::new();
    }

    #[inline(always)]
    pub const fn push(
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

    #[must_use]
    pub fn visible(self, now_seconds: f32) -> VisibleStutterSamples {
        let mut out = VisibleStutterSamples::new();
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
    pub const fn clear(&mut self) {
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
        assert!(dt.mul_add(-4.0, out).abs() < EPS, "got {out}");
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
        assert!(4.0f32.mul_add(-dt, out).abs() < EPS);
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
            (STUTTER_SAMPLE_COUNT as f32)
                .mul_add(-0.1, visible.last().unwrap().timestamp_seconds)
                .abs()
                < EPS
        );
    }
}
