const DISPLAY_CLOCK_CORRECTION_HALF_LIFE_S: f32 = 0.012;
const DISPLAY_CLOCK_MAX_LAG_S: f32 = 0.020;
const DISPLAY_CLOCK_MAX_LEAD_S: f32 = 0.006;
const DISPLAY_CLOCK_RESET_ERROR_S: f32 = 0.100;
const DISPLAY_CLOCK_MAX_STEP_S: f32 = 1.0 / 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayClockDiagEventKind {
    ResetJump,
    TargetJump,
    ClampStep,
    ErrorThreshold,
    CatchUpStart,
}

impl std::fmt::Display for DisplayClockDiagEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::ResetJump => "reset_jump",
            Self::TargetJump => "target_jump",
            Self::ClampStep => "clamp_step",
            Self::ErrorThreshold => "error_threshold",
            Self::CatchUpStart => "catch_up_start",
        };
        f.write_str(label)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DisplayClockStepEvent {
    pub kind: DisplayClockDiagEventKind,
    pub target_time_sec: f32,
    pub previous_time_sec: f32,
    pub current_time_sec: f32,
    pub error_seconds: f32,
    pub step_seconds: f32,
    pub limit_seconds: f32,
}

const DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT: usize = 32;
static DISPLAY_CLOCK_STUTTER_DIAG_TRIGGER_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug)]
pub struct DisplayClockDiagEvent {
    pub at_host_nanos: u64,
    pub kind: DisplayClockDiagEventKind,
    pub target_time_sec: f32,
    pub previous_time_sec: f32,
    pub current_time_sec: f32,
    pub error_seconds: f32,
    pub step_seconds: f32,
    pub limit_seconds: f32,
}

impl DisplayClockDiagEvent {
    #[inline(always)]
    const fn empty() -> Self {
        Self {
            at_host_nanos: 0,
            kind: DisplayClockDiagEventKind::ResetJump,
            target_time_sec: 0.0,
            previous_time_sec: 0.0,
            current_time_sec: 0.0,
            error_seconds: 0.0,
            step_seconds: 0.0,
            limit_seconds: 0.0,
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn from_step_event(at_host_nanos: u64, event: DisplayClockStepEvent) -> Self {
        Self {
            at_host_nanos,
            kind: event.kind,
            target_time_sec: event.target_time_sec,
            previous_time_sec: event.previous_time_sec,
            current_time_sec: event.current_time_sec,
            error_seconds: event.error_seconds,
            step_seconds: event.step_seconds,
            limit_seconds: event.limit_seconds,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DisplayClockDiagRing {
    events: [DisplayClockDiagEvent; DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT],
    cursor: usize,
    len: usize,
    last_trigger_seq: u64,
}

impl Default for DisplayClockDiagRing {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayClockDiagRing {
    #[inline(always)]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            events: [DisplayClockDiagEvent::empty(); DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT],
            cursor: 0,
            len: 0,
            last_trigger_seq: 0,
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn last_trigger_seq(&self) -> u64 {
        self.last_trigger_seq
    }

    #[inline(always)]
    pub fn push(&mut self, event: DisplayClockDiagEvent) {
        self.events[self.cursor] = event;
        self.cursor = (self.cursor + 1) % DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT;
        self.len = self
            .len
            .saturating_add(1)
            .min(DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT);
        self.last_trigger_seq =
            DISPLAY_CLOCK_STUTTER_DIAG_TRIGGER_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    }

    pub fn collect_recent(
        &self,
        now_host_nanos: u64,
        window_ns: u64,
        out: &mut Vec<DisplayClockDiagEvent>,
    ) {
        let start = self
            .cursor
            .saturating_add(DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT)
            .saturating_sub(self.len)
            % DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT;
        for i in 0..self.len {
            let event = self.events[(start + i) % DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT];
            if event.at_host_nanos == 0 {
                continue;
            }
            if now_host_nanos.saturating_sub(event.at_host_nanos) <= window_ns {
                out.push(event);
            }
        }
    }
}

#[inline(always)]
pub fn apply_chart_attacks_transforms(
    notes: &mut Vec<Note>,
    note_ranges: &mut [(usize, usize); MAX_PLAYERS],
    gameplay_charts: &[Arc<GameplayChartData>; MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    player_attack_modes: &[GameplayAttackMode; MAX_PLAYERS],
    timing_players: &[Arc<TimingData>; MAX_PLAYERS],
    base_seed: u64,
    song_length_seconds: f32,
    course_modifiers: Option<&str>,
) {
    if let Some(modifiers) = course_modifiers.filter(|mods| !mods.trim().is_empty()) {
        let course_attack = format!(
            "TIME=-1000000:LEN={}:MODS={modifiers}",
            song_length_seconds.max(0.0) + 2_000_000.0
        );
        let players = std::array::from_fn(|player| ChartAttackTransformPlayer {
            chart_attacks: Some(course_attack.as_str()),
            attack_mode: GameplayAttackMode::On,
            timing_player: timing_players[player].as_ref(),
        });
        apply_chart_attack_transforms(
            notes,
            note_ranges,
            cols_per_player,
            num_players,
            &players,
            base_seed,
            song_length_seconds,
        );
    }
    let players = std::array::from_fn(|player| ChartAttackTransformPlayer {
        chart_attacks: gameplay_charts[player].chart_attacks.as_deref(),
        attack_mode: player_attack_modes[player],
        timing_player: timing_players[player].as_ref(),
    });
    apply_chart_attack_transforms(
        notes,
        note_ranges,
        cols_per_player,
        num_players,
        &players,
        base_seed,
        song_length_seconds,
    );
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayClockHealth {
    pub error_seconds: f32,
    pub catching_up: bool,
}

#[derive(Clone, Copy, Debug)]
struct DisplayClockScaledLimits {
    max_error_ns: i128,
    max_step_ns: i128,
    max_lag_ns: i128,
    max_lead_ns: i128,
}

#[derive(Clone, Copy, Debug)]
struct DisplayClockStepThresholds {
    target_jump_ns: i128,
    clamp_step_ns: i128,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameStableDisplayClock {
    current_time_ns: SongTimeNs,
    target_time_ns: SongTimeNs,
    catching_up: bool,
    error_over_threshold: bool,
    correction_delta_bits: u32,
    correction_alpha: f32,
    scaled_limits_slope_bits: u32,
    scaled_limits: DisplayClockScaledLimits,
    step_thresholds_slope_bits: u32,
    step_thresholds: DisplayClockStepThresholds,
}

impl FrameStableDisplayClock {
    #[inline(always)]
    #[must_use]
    pub const fn new(time_ns: SongTimeNs) -> Self {
        Self {
            current_time_ns: time_ns,
            target_time_ns: time_ns,
            catching_up: false,
            error_over_threshold: false,
            correction_delta_bits: u32::MAX,
            correction_alpha: 0.0,
            scaled_limits_slope_bits: u32::MAX,
            scaled_limits: DisplayClockScaledLimits {
                max_error_ns: 0,
                max_step_ns: 0,
                max_lag_ns: 0,
                max_lead_ns: 0,
            },
            step_thresholds_slope_bits: u32::MAX,
            step_thresholds: DisplayClockStepThresholds {
                target_jump_ns: 0,
                clamp_step_ns: 0,
            },
        }
    }

    #[inline(always)]
    pub const fn reset(&mut self, time_ns: SongTimeNs) -> SongTimeNs {
        self.current_time_ns = time_ns;
        self.target_time_ns = time_ns;
        self.catching_up = false;
        self.error_over_threshold = false;
        time_ns
    }

    #[inline(always)]
    #[must_use]
    pub fn health(self) -> DisplayClockHealth {
        DisplayClockHealth {
            error_seconds: song_time_ns_span_seconds(
                i128::from(self.target_time_ns) - i128::from(self.current_time_ns),
            ),
            catching_up: self.catching_up,
        }
    }

    #[inline(always)]
    fn correction_alpha(&mut self, delta_time: f32) -> f32 {
        let bits = delta_time.to_bits();
        if self.correction_delta_bits != bits {
            self.correction_delta_bits = bits;
            self.correction_alpha =
                1.0 - f32::exp2(-delta_time / DISPLAY_CLOCK_CORRECTION_HALF_LIFE_S);
        }
        self.correction_alpha
    }

    #[inline(always)]
    fn scaled_limits(&mut self, slope: f32) -> DisplayClockScaledLimits {
        let bits = slope.to_bits();
        if self.scaled_limits_slope_bits != bits {
            self.scaled_limits_slope_bits = bits;
            self.scaled_limits = DisplayClockScaledLimits {
                max_error_ns: i128::from(scaled_song_time_ns(
                    DISPLAY_CLOCK_RESET_ERROR_S,
                    slope,
                )),
                max_step_ns: i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_STEP_S, slope)),
                max_lag_ns: i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_LAG_S, slope)),
                max_lead_ns: i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_LEAD_S, slope)),
            };
        }
        self.scaled_limits
    }

    #[inline(always)]
    fn step_thresholds(&mut self, slope: f32) -> DisplayClockStepThresholds {
        let bits = slope.to_bits();
        if self.step_thresholds_slope_bits != bits {
            self.step_thresholds_slope_bits = bits;
            let max_step_ns = i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_STEP_S, slope));
            self.step_thresholds = DisplayClockStepThresholds {
                target_jump_ns: (max_step_ns as f64 * 2.0).round() as i128,
                clamp_step_ns: (max_step_ns as f64 * 1.2).round() as i128,
            };
        }
        self.step_thresholds
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayDisplayClockState {
    clock: FrameStableDisplayClock,
    diag: DisplayClockDiagRing,
}

impl GameplayDisplayClockState {
    #[inline(always)]
    #[must_use]
    pub const fn new(time_ns: SongTimeNs) -> Self {
        Self {
            clock: FrameStableDisplayClock::new(time_ns),
            diag: DisplayClockDiagRing::new(),
        }
    }

    #[inline(always)]
    pub const fn reset(&mut self, time_ns: SongTimeNs) -> SongTimeNs {
        self.clock.reset(time_ns)
    }

    #[inline(always)]
    #[must_use]
    pub fn health(self) -> DisplayClockHealth {
        self.clock.health()
    }

    #[inline(always)]
    #[must_use]
    pub const fn diag_trigger_seq(&self) -> u64 {
        self.diag.last_trigger_seq()
    }

    #[inline(always)]
    pub fn collect_diag_events(
        &self,
        now_host_nanos: u64,
        window_ns: u64,
        out: &mut Vec<DisplayClockDiagEvent>,
    ) {
        self.diag.collect_recent(now_host_nanos, window_ns, out);
    }

    #[inline(always)]
    pub fn step(
        &mut self,
        at_host_nanos: u64,
        target_display_time_ns: SongTimeNs,
        delta_time: f32,
        seconds_per_second: f32,
        first_update: bool,
        diag_enabled: bool,
    ) -> SongTimeNs {
        let clock = &mut self.clock;
        let diag = &mut self.diag;
        frame_stable_display_clock_step(
            clock,
            target_display_time_ns,
            delta_time,
            seconds_per_second,
            first_update,
            |event| {
                if diag_enabled && at_host_nanos != 0 {
                    diag.push(DisplayClockDiagEvent::from_step_event(at_host_nanos, event));
                }
            },
        )
    }
}

#[inline(always)]
pub fn frame_stable_display_music_time_ns(
    display_clock_state: &mut GameplayDisplayClockState,
    at_host_nanos: u64,
    target_display_time_ns: SongTimeNs,
    delta_time: f32,
    seconds_per_second: f32,
    first_update: bool,
) -> SongTimeNs {
    display_clock_state.step(
        at_host_nanos,
        target_display_time_ns,
        delta_time,
        seconds_per_second,
        first_update,
        log::log_enabled!(log::Level::Trace),
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayBeatPhaseState {
    is_in_freeze: bool,
    is_in_delay: bool,
}

impl GameplayBeatPhaseState {
    #[inline(always)]
    #[must_use]
    pub const fn new(is_in_freeze: bool, is_in_delay: bool) -> Self {
        Self {
            is_in_freeze,
            is_in_delay,
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn is_in_freeze(&self) -> bool {
        self.is_in_freeze
    }

    #[inline(always)]
    #[must_use]
    pub const fn is_in_delay(&self) -> bool {
        self.is_in_delay
    }

    #[inline(always)]
    #[must_use]
    pub const fn paused(&self) -> bool {
        self.is_in_freeze || self.is_in_delay
    }

    #[inline(always)]
    pub const fn set(&mut self, is_in_freeze: bool, is_in_delay: bool) {
        self.is_in_freeze = is_in_freeze;
        self.is_in_delay = is_in_delay;
    }
}

#[inline(always)]
fn display_clock_step_event(
    kind: DisplayClockDiagEventKind,
    target_time_ns: SongTimeNs,
    previous_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
    error_ns: i128,
    step_ns: i128,
    limit_ns: i128,
) -> DisplayClockStepEvent {
    DisplayClockStepEvent {
        kind,
        target_time_sec: song_time_ns_to_seconds(target_time_ns),
        previous_time_sec: song_time_ns_to_seconds(previous_time_ns),
        current_time_sec: song_time_ns_to_seconds(current_time_ns),
        error_seconds: song_time_ns_span_seconds(error_ns),
        step_seconds: song_time_ns_span_seconds(step_ns),
        limit_seconds: song_time_ns_span_seconds(limit_ns),
    }
}

pub fn frame_stable_display_clock_step(
    display_clock: &mut FrameStableDisplayClock,
    target_display_time_ns: SongTimeNs,
    delta_time: f32,
    seconds_per_second: f32,
    first_update: bool,
    mut note_event: impl FnMut(DisplayClockStepEvent),
) -> SongTimeNs {
    display_clock.target_time_ns = target_display_time_ns;
    if first_update
        || song_time_ns_invalid(display_clock.current_time_ns)
        || song_time_ns_invalid(target_display_time_ns)
        || !delta_time.is_finite()
        || delta_time <= 0.0
    {
        return display_clock.reset(target_display_time_ns);
    }

    let slope = normalized_song_rate(seconds_per_second);
    let previous_display_time_ns = display_clock.current_time_ns;
    let previous_catching_up = display_clock.catching_up;
    let previous_error_over_threshold = display_clock.error_over_threshold;
    let target_delta_ns = i128::from(target_display_time_ns) - i128::from(previous_display_time_ns);
    let limits = display_clock.scaled_limits(slope);
    if target_delta_ns.abs() > limits.max_error_ns {
        note_event(display_clock_step_event(
            DisplayClockDiagEventKind::ResetJump,
            target_display_time_ns,
            previous_display_time_ns,
            target_display_time_ns,
            target_delta_ns,
            target_delta_ns,
            limits.max_error_ns,
        ));
        return display_clock.reset(target_display_time_ns);
    }

    let advanced_ns =
        i128::from(previous_display_time_ns) + i128::from(scaled_song_time_ns(delta_time, slope));
    let correction_alpha = display_clock.correction_alpha(delta_time);
    let mut corrected_ns = advanced_ns
        + ((i128::from(target_display_time_ns) - advanced_ns) as f64 * f64::from(correction_alpha))
            .round() as i128;
    let thresholds = display_clock.step_thresholds(slope);
    if target_delta_ns.abs() > thresholds.target_jump_ns {
        note_event(display_clock_step_event(
            DisplayClockDiagEventKind::TargetJump,
            target_display_time_ns,
            previous_display_time_ns,
            clamp_song_time_ns(corrected_ns),
            target_delta_ns,
            target_delta_ns,
            thresholds.target_jump_ns,
        ));
    }
    let step_ns = corrected_ns - i128::from(previous_display_time_ns);
    let mut clamped_step = false;
    if step_ns.abs() > thresholds.clamp_step_ns {
        corrected_ns =
            i128::from(previous_display_time_ns) + step_ns.signum() * limits.max_step_ns;
        clamped_step = true;
    }
    let min_allowed_ns = i128::from(target_display_time_ns) - limits.max_lag_ns;
    let max_allowed_ns = i128::from(target_display_time_ns) + limits.max_lead_ns;
    corrected_ns = corrected_ns
        .clamp(min_allowed_ns, max_allowed_ns)
        .max(i128::from(previous_display_time_ns));
    display_clock.current_time_ns = clamp_song_time_ns(corrected_ns);
    let error_ns = i128::from(target_display_time_ns) - corrected_ns;
    display_clock.catching_up = error_ns.abs() > (limits.max_step_ns / 2);
    display_clock.error_over_threshold = error_ns.abs() > limits.max_step_ns;
    if clamped_step {
        note_event(display_clock_step_event(
            DisplayClockDiagEventKind::ClampStep,
            target_display_time_ns,
            previous_display_time_ns,
            display_clock.current_time_ns,
            error_ns,
            corrected_ns - i128::from(previous_display_time_ns),
            limits.max_step_ns,
        ));
    }
    if !previous_error_over_threshold && display_clock.error_over_threshold {
        note_event(display_clock_step_event(
            DisplayClockDiagEventKind::ErrorThreshold,
            target_display_time_ns,
            previous_display_time_ns,
            display_clock.current_time_ns,
            error_ns,
            corrected_ns - i128::from(previous_display_time_ns),
            limits.max_step_ns,
        ));
    }
    if !previous_catching_up && display_clock.catching_up {
        note_event(display_clock_step_event(
            DisplayClockDiagEventKind::CatchUpStart,
            target_display_time_ns,
            previous_display_time_ns,
            display_clock.current_time_ns,
            error_ns,
            corrected_ns - i128::from(previous_display_time_ns),
            limits.max_step_ns / 2,
        ));
    }
    display_clock.current_time_ns
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub mod display_clock_bench_support {
    use std::hint::black_box;

    use super::*;

    fn scaled_limits_checksum(limits: DisplayClockScaledLimits) -> u64 {
        (limits.max_error_ns as u64)
            ^ (limits.max_step_ns as u64).rotate_left(13)
            ^ (limits.max_lag_ns as u64).rotate_left(29)
            ^ (limits.max_lead_ns as u64).rotate_left(47)
    }

    fn thresholds_checksum(thresholds: DisplayClockStepThresholds) -> u64 {
        (thresholds.target_jump_ns as u64)
            ^ (thresholds.clamp_step_ns as u64).rotate_left(31)
    }

    #[must_use]
    pub fn correction_old(evaluations: usize, delta_time: f32) -> u64 {
        let mut checksum = 0_u64;
        for _ in 0..evaluations {
            let delta_time = black_box(delta_time);
            let alpha = 1.0 - f32::exp2(-delta_time / DISPLAY_CLOCK_CORRECTION_HALF_LIFE_S);
            checksum = checksum.wrapping_add(u64::from(alpha.to_bits()));
        }
        checksum
    }

    #[must_use]
    pub fn correction_new(evaluations: usize, delta_time: f32) -> u64 {
        let mut clock = FrameStableDisplayClock::new(0);
        let mut checksum = 0_u64;
        for _ in 0..evaluations {
            let alpha = clock.correction_alpha(black_box(delta_time));
            checksum = checksum.wrapping_add(u64::from(alpha.to_bits()));
        }
        checksum
    }

    #[must_use]
    pub fn scaled_limits_old(evaluations: usize, slope: f32) -> u64 {
        let mut checksum = 0_u64;
        for _ in 0..evaluations {
            let slope = black_box(slope);
            let limits = DisplayClockScaledLimits {
                max_error_ns: i128::from(scaled_song_time_ns(
                    DISPLAY_CLOCK_RESET_ERROR_S,
                    slope,
                )),
                max_step_ns: i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_STEP_S, slope)),
                max_lag_ns: i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_LAG_S, slope)),
                max_lead_ns: i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_LEAD_S, slope)),
            };
            checksum = checksum.wrapping_add(scaled_limits_checksum(limits));
        }
        checksum
    }

    #[must_use]
    pub fn scaled_limits_new(evaluations: usize, slope: f32) -> u64 {
        let mut clock = FrameStableDisplayClock::new(0);
        let mut checksum = 0_u64;
        for _ in 0..evaluations {
            let limits = clock.scaled_limits(black_box(slope));
            checksum = checksum.wrapping_add(scaled_limits_checksum(limits));
        }
        checksum
    }

    #[must_use]
    pub fn step_thresholds_old(evaluations: usize, slope: f32) -> u64 {
        let mut checksum = 0_u64;
        for _ in 0..evaluations {
            let slope = black_box(slope);
            let max_step_ns = i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_STEP_S, slope));
            let thresholds = DisplayClockStepThresholds {
                target_jump_ns: (max_step_ns as f64 * 2.0).round() as i128,
                clamp_step_ns: (max_step_ns as f64 * 1.2).round() as i128,
            };
            checksum = checksum.wrapping_add(thresholds_checksum(thresholds));
        }
        checksum
    }

    #[must_use]
    pub fn step_thresholds_new(evaluations: usize, slope: f32) -> u64 {
        let mut clock = FrameStableDisplayClock::new(0);
        let mut checksum = 0_u64;
        for _ in 0..evaluations {
            let thresholds = clock.step_thresholds(black_box(slope));
            checksum = checksum.wrapping_add(thresholds_checksum(thresholds));
        }
        checksum
    }
}

