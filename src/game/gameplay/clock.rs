use crate::engine::audio;
use log::debug;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use super::{
    SongTimeNs, State, clamp_song_time_ns, normalized_song_rate, scaled_song_delta_ns,
    scaled_song_time_ns, song_time_ns_from_seconds, song_time_ns_invalid,
    song_time_ns_span_seconds, song_time_ns_to_seconds, stream_pos_to_music_time,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct SongClockSnapshot {
    pub(crate) song_time_ns: SongTimeNs,
    pub(crate) seconds_per_second: f32,
    pub(crate) valid_at: Instant,
    pub(crate) valid_at_host_nanos: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FrameStableDisplayClock {
    current_time_ns: SongTimeNs,
    target_time_ns: SongTimeNs,
    catching_up: bool,
    error_over_threshold: bool,
}

const DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT: usize = 32;
static DISPLAY_CLOCK_STUTTER_DIAG_TRIGGER_SEQ: AtomicU64 = AtomicU64::new(0);

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
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DisplayClockDiagRing {
    events: [DisplayClockDiagEvent; DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT],
    cursor: usize,
    len: usize,
    last_trigger_seq: u64,
}

impl DisplayClockDiagRing {
    #[inline(always)]
    pub(crate) const fn new() -> Self {
        Self {
            events: [DisplayClockDiagEvent::empty(); DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT],
            cursor: 0,
            len: 0,
            last_trigger_seq: 0,
        }
    }

    #[inline(always)]
    fn push(&mut self, event: DisplayClockDiagEvent) {
        self.events[self.cursor] = event;
        self.cursor = (self.cursor + 1) % DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT;
        self.len = self
            .len
            .saturating_add(1)
            .min(DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT);
        self.last_trigger_seq =
            DISPLAY_CLOCK_STUTTER_DIAG_TRIGGER_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    }

    fn collect_recent(
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

#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayClockHealth {
    pub error_seconds: f32,
    pub catching_up: bool,
}

impl FrameStableDisplayClock {
    #[inline(always)]
    pub(crate) const fn new(time_ns: SongTimeNs) -> Self {
        Self {
            current_time_ns: time_ns,
            target_time_ns: time_ns,
            catching_up: false,
            error_over_threshold: false,
        }
    }

    #[inline(always)]
    pub(crate) fn reset(&mut self, time_ns: SongTimeNs) -> SongTimeNs {
        self.current_time_ns = time_ns;
        self.target_time_ns = time_ns;
        self.catching_up = false;
        self.error_over_threshold = false;
        time_ns
    }
}

#[inline(always)]
pub fn display_clock_health(state: &State) -> DisplayClockHealth {
    DisplayClockHealth {
        error_seconds: song_time_ns_span_seconds(
            i128::from(state.display_clock.target_time_ns)
                - i128::from(state.display_clock.current_time_ns),
        ),
        catching_up: state.display_clock.catching_up,
    }
}

#[inline(always)]
pub fn display_clock_stutter_diag_trigger_seq(state: &State) -> u64 {
    state.display_clock_diag.last_trigger_seq
}

pub fn collect_display_clock_stutter_diag_events(
    state: &State,
    now_host_nanos: u64,
    window_ns: u64,
    out: &mut Vec<DisplayClockDiagEvent>,
) {
    state
        .display_clock_diag
        .collect_recent(now_host_nanos, window_ns, out);
}

#[inline(always)]
pub(crate) fn current_song_clock_snapshot(state: &State) -> SongClockSnapshot {
    let stream_clock = audio::get_music_stream_clock_snapshot();
    let fallback_rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    if stream_clock.has_music_mapping {
        SongClockSnapshot {
            song_time_ns: stream_clock.music_nanos,
            seconds_per_second: if stream_clock.music_seconds_per_second.is_finite()
                && stream_clock.music_seconds_per_second > 0.0
            {
                stream_clock.music_seconds_per_second
            } else {
                fallback_rate
            },
            valid_at: stream_clock.valid_at,
            valid_at_host_nanos: stream_clock.valid_at_host_nanos,
        }
    } else {
        let song_time = stream_pos_to_music_time(state, stream_clock.stream_seconds);
        SongClockSnapshot {
            song_time_ns: song_time_ns_from_seconds(song_time),
            seconds_per_second: fallback_rate,
            valid_at: stream_clock.valid_at,
            valid_at_host_nanos: stream_clock.valid_at_host_nanos,
        }
    }
}

pub(crate) fn music_time_ns_from_song_clock(
    snapshot: SongClockSnapshot,
    captured_at: Instant,
    captured_host_nanos: u64,
) -> SongTimeNs {
    let slope = normalized_song_rate(snapshot.seconds_per_second);
    let snapshot_song_time = song_time_ns_to_seconds(snapshot.song_time_ns);
    if snapshot.valid_at_host_nanos != 0 && captured_host_nanos != 0 {
        let dt_nanos = captured_host_nanos as i128 - snapshot.valid_at_host_nanos as i128;
        if audio::timing_diag_enabled() {
            debug!(
                "AUDIO_DIAG snap_age_ms={:.3} path=host callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
                dt_nanos as f64 * 1e-6,
                audio::timing_diag_last_callback_gap_ns() as f64 * 1e-6,
                snapshot_song_time,
                slope,
                snapshot.valid_at_host_nanos,
                captured_host_nanos,
            );
        }
        return clamp_song_time_ns(
            i128::from(snapshot.song_time_ns) + scaled_song_delta_ns(dt_nanos, slope),
        );
    }
    let delta_host_nanos = if let Some(age) = snapshot.valid_at.checked_duration_since(captured_at)
    {
        if audio::timing_diag_enabled() {
            debug!(
                "AUDIO_DIAG snap_age_ms={:.3} path=instant callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
                -(age.as_secs_f64() * 1000.0),
                audio::timing_diag_last_callback_gap_ns() as f64 * 1e-6,
                snapshot_song_time,
                slope,
                snapshot.valid_at_host_nanos,
                captured_host_nanos,
            );
        }
        -(age.as_nanos() as i128)
    } else if let Some(lead) = captured_at.checked_duration_since(snapshot.valid_at) {
        if audio::timing_diag_enabled() {
            debug!(
                "AUDIO_DIAG snap_age_ms={:.3} path=instant callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
                lead.as_secs_f64() * 1000.0,
                audio::timing_diag_last_callback_gap_ns() as f64 * 1e-6,
                snapshot_song_time,
                slope,
                snapshot.valid_at_host_nanos,
                captured_host_nanos,
            );
        }
        lead.as_nanos() as i128
    } else {
        if audio::timing_diag_enabled() {
            debug!(
                "AUDIO_DIAG snap_age_ms=0.000 path=instant callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
                audio::timing_diag_last_callback_gap_ns() as f64 * 1e-6,
                snapshot_song_time,
                slope,
                snapshot.valid_at_host_nanos,
                captured_host_nanos,
            );
        }
        0
    };
    clamp_song_time_ns(
        i128::from(snapshot.song_time_ns) + scaled_song_delta_ns(delta_host_nanos, slope),
    )
}

const DISPLAY_CLOCK_CORRECTION_HALF_LIFE_S: f32 = 0.012;
const DISPLAY_CLOCK_MAX_LAG_S: f32 = 0.020;
const DISPLAY_CLOCK_MAX_LEAD_S: f32 = 0.006;
const DISPLAY_CLOCK_RESET_ERROR_S: f32 = 0.100;
const DISPLAY_CLOCK_MAX_STEP_S: f32 = 1.0 / 60.0;

#[inline(always)]
fn display_clock_stutter_diag_enabled() -> bool {
    log::log_enabled!(log::Level::Trace)
}

fn note_display_clock_diag_event(
    diag: &mut DisplayClockDiagRing,
    at_host_nanos: u64,
    kind: DisplayClockDiagEventKind,
    target_time_sec: f32,
    previous_time_sec: f32,
    current_time_sec: f32,
    error_seconds: f32,
    step_seconds: f32,
    limit_seconds: f32,
) {
    if !display_clock_stutter_diag_enabled() || at_host_nanos == 0 {
        return;
    }
    diag.push(DisplayClockDiagEvent {
        at_host_nanos,
        kind,
        target_time_sec,
        previous_time_sec,
        current_time_sec,
        error_seconds,
        step_seconds,
        limit_seconds,
    });
}

#[inline(always)]
pub(crate) fn frame_stable_display_music_time_ns(
    display_clock: &mut FrameStableDisplayClock,
    diag: &mut DisplayClockDiagRing,
    at_host_nanos: u64,
    target_display_time_ns: SongTimeNs,
    delta_time: f32,
    seconds_per_second: f32,
    first_update: bool,
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
    let max_error_ns = i128::from(scaled_song_time_ns(DISPLAY_CLOCK_RESET_ERROR_S, slope));
    if target_delta_ns.abs() > max_error_ns {
        note_display_clock_diag_event(
            diag,
            at_host_nanos,
            DisplayClockDiagEventKind::ResetJump,
            song_time_ns_to_seconds(target_display_time_ns),
            song_time_ns_to_seconds(previous_display_time_ns),
            song_time_ns_to_seconds(target_display_time_ns),
            song_time_ns_span_seconds(target_delta_ns),
            song_time_ns_span_seconds(target_delta_ns),
            song_time_ns_span_seconds(max_error_ns),
        );
        return display_clock.reset(target_display_time_ns);
    }

    let advanced_ns =
        i128::from(previous_display_time_ns) + i128::from(scaled_song_time_ns(delta_time, slope));
    let correction_alpha = 1.0 - f32::exp2(-delta_time / DISPLAY_CLOCK_CORRECTION_HALF_LIFE_S);
    let mut corrected_ns = advanced_ns
        + ((i128::from(target_display_time_ns) - advanced_ns) as f64 * correction_alpha as f64)
            .round() as i128;
    let max_step_ns = i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_STEP_S, slope));
    if target_delta_ns.abs() > (max_step_ns as f64 * 2.0).round() as i128 {
        note_display_clock_diag_event(
            diag,
            at_host_nanos,
            DisplayClockDiagEventKind::TargetJump,
            song_time_ns_to_seconds(target_display_time_ns),
            song_time_ns_to_seconds(previous_display_time_ns),
            song_time_ns_to_seconds(clamp_song_time_ns(corrected_ns)),
            song_time_ns_span_seconds(target_delta_ns),
            song_time_ns_span_seconds(target_delta_ns),
            song_time_ns_span_seconds((max_step_ns as f64 * 2.0).round() as i128),
        );
    }
    let step_ns = corrected_ns - i128::from(previous_display_time_ns);
    let mut clamped_step = false;
    if step_ns.abs() > (max_step_ns as f64 * 1.2).round() as i128 {
        corrected_ns = i128::from(previous_display_time_ns) + step_ns.signum() * max_step_ns;
        clamped_step = true;
    }
    let min_allowed_ns = i128::from(target_display_time_ns)
        - i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_LAG_S, slope));
    let max_allowed_ns = i128::from(target_display_time_ns)
        + i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_LEAD_S, slope));
    corrected_ns = corrected_ns
        .clamp(min_allowed_ns, max_allowed_ns)
        .max(i128::from(previous_display_time_ns));
    display_clock.current_time_ns = clamp_song_time_ns(corrected_ns);
    let error_ns = i128::from(target_display_time_ns) - corrected_ns;
    display_clock.catching_up = error_ns.abs() > (max_step_ns / 2);
    display_clock.error_over_threshold = error_ns.abs() > max_step_ns;
    if clamped_step {
        note_display_clock_diag_event(
            diag,
            at_host_nanos,
            DisplayClockDiagEventKind::ClampStep,
            song_time_ns_to_seconds(target_display_time_ns),
            song_time_ns_to_seconds(previous_display_time_ns),
            song_time_ns_to_seconds(display_clock.current_time_ns),
            song_time_ns_span_seconds(error_ns),
            song_time_ns_span_seconds(corrected_ns - i128::from(previous_display_time_ns)),
            song_time_ns_span_seconds(max_step_ns),
        );
    }
    if !previous_error_over_threshold && display_clock.error_over_threshold {
        note_display_clock_diag_event(
            diag,
            at_host_nanos,
            DisplayClockDiagEventKind::ErrorThreshold,
            song_time_ns_to_seconds(target_display_time_ns),
            song_time_ns_to_seconds(previous_display_time_ns),
            song_time_ns_to_seconds(display_clock.current_time_ns),
            song_time_ns_span_seconds(error_ns),
            song_time_ns_span_seconds(corrected_ns - i128::from(previous_display_time_ns)),
            song_time_ns_span_seconds(max_step_ns),
        );
    }
    if !previous_catching_up && display_clock.catching_up {
        note_display_clock_diag_event(
            diag,
            at_host_nanos,
            DisplayClockDiagEventKind::CatchUpStart,
            song_time_ns_to_seconds(target_display_time_ns),
            song_time_ns_to_seconds(previous_display_time_ns),
            song_time_ns_to_seconds(display_clock.current_time_ns),
            song_time_ns_span_seconds(error_ns),
            song_time_ns_span_seconds(corrected_ns - i128::from(previous_display_time_ns)),
            song_time_ns_span_seconds(max_step_ns / 2),
        );
    }
    display_clock.current_time_ns
}
