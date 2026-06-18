use log::debug;
use std::time::Instant;

use super::{
    DisplayClockDiagEvent, DisplayClockDiagRing, DisplayClockHealth, DisplayClockStepEvent,
    FrameStableDisplayClock, SongClockSnapshot, SongTimeNs, State, frame_stable_display_clock_step,
    normalized_song_rate, song_clock_music_time_ns, song_time_ns_to_seconds,
};

#[inline(always)]
pub fn display_clock_health(state: &State) -> DisplayClockHealth {
    state.display_clock.health()
}

#[inline(always)]
pub fn display_clock_stutter_diag_trigger_seq(state: &State) -> u64 {
    state.display_clock_diag.last_trigger_seq()
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
        return song_clock_music_time_ns(snapshot, captured_at, captured_host_nanos);
    }
    if let Some(age) = snapshot.valid_at.checked_duration_since(captured_at) {
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
    }
    song_clock_music_time_ns(snapshot, captured_at, captured_host_nanos)
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
    diag.push(DisplayClockDiagEvent::from_step_event(at_host_nanos, event));
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
