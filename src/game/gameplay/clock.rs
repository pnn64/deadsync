use deadsync_core::song_time::clamp_song_time_ns;
use log::debug;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use super::{
    DisplayClockDiagEventKind, DisplayClockHealth, DisplayClockStepEvent, FrameStableDisplayClock,
    SongClockSnapshot, SongTimeNs, State, frame_stable_display_clock_step, normalized_song_rate,
    scaled_song_delta_ns, song_time_ns_to_seconds,
};

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

#[inline(always)]
pub fn display_clock_health(state: &State) -> DisplayClockHealth {
    state.display_clock.health()
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

pub(crate) fn music_time_ns_from_song_clock(
    snapshot: SongClockSnapshot,
    captured_at: Instant,
    captured_host_nanos: u64,
) -> SongTimeNs {
    let slope = normalized_song_rate(snapshot.seconds_per_second);
    let snapshot_song_time = song_time_ns_to_seconds(snapshot.song_time_ns);
    if snapshot.valid_at_host_nanos != 0 && captured_host_nanos != 0 {
        let dt_nanos = captured_host_nanos as i128 - snapshot.valid_at_host_nanos as i128;
        if snapshot.timing_diag_enabled {
            debug!(
                "AUDIO_DIAG snap_age_ms={:.3} path=host callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
                dt_nanos as f64 * 1e-6,
                snapshot.timing_diag_callback_gap_ns as f64 * 1e-6,
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
        if snapshot.timing_diag_enabled {
            debug!(
                "AUDIO_DIAG snap_age_ms={:.3} path=instant callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
                -(age.as_secs_f64() * 1000.0),
                snapshot.timing_diag_callback_gap_ns as f64 * 1e-6,
                snapshot_song_time,
                slope,
                snapshot.valid_at_host_nanos,
                captured_host_nanos,
            );
        }
        -(age.as_nanos() as i128)
    } else if let Some(lead) = captured_at.checked_duration_since(snapshot.valid_at) {
        if snapshot.timing_diag_enabled {
            debug!(
                "AUDIO_DIAG snap_age_ms={:.3} path=instant callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
                lead.as_secs_f64() * 1000.0,
                snapshot.timing_diag_callback_gap_ns as f64 * 1e-6,
                snapshot_song_time,
                slope,
                snapshot.valid_at_host_nanos,
                captured_host_nanos,
            );
        }
        lead.as_nanos() as i128
    } else {
        if snapshot.timing_diag_enabled {
            debug!(
                "AUDIO_DIAG snap_age_ms=0.000 path=instant callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
                snapshot.timing_diag_callback_gap_ns as f64 * 1e-6,
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

#[inline(always)]
fn display_clock_stutter_diag_enabled() -> bool {
    log::log_enabled!(log::Level::Trace)
}

fn note_display_clock_diag_event(
    diag: &mut DisplayClockDiagRing,
    at_host_nanos: u64,
    event: DisplayClockStepEvent,
) {
    if !display_clock_stutter_diag_enabled() || at_host_nanos == 0 {
        return;
    }
    diag.push(DisplayClockDiagEvent {
        at_host_nanos,
        kind: event.kind,
        target_time_sec: event.target_time_sec,
        previous_time_sec: event.previous_time_sec,
        current_time_sec: event.current_time_sec,
        error_seconds: event.error_seconds,
        step_seconds: event.step_seconds,
        limit_seconds: event.limit_seconds,
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
    frame_stable_display_clock_step(
        display_clock,
        target_display_time_ns,
        delta_time,
        seconds_per_second,
        first_update,
        |event| note_display_clock_diag_event(diag, at_host_nanos, event),
    )
}
