use crate::lookup_music_position;
use deadlib_platform::host_time::instant_nanos;
#[cfg(windows)]
use deadlib_platform::windows_rt::current_qpc_nanos;
use deadsync_audio::{
    CallbackClockSource, CallbackClockWindow, MusicStreamClockSnapshot,
    fallback_stream_position_frames, music_nanos_from_seconds, music_track_has_started,
    music_track_start_frame, seeded_music_position,
    stream_position_frames_from_window as audio_stream_position_frames_from_window,
};
use log::debug;
use std::time::Instant;

#[inline(always)]
pub fn timing_diag_enabled() -> bool {
    log::log_enabled!(log::Level::Debug)
}

#[inline(always)]
fn current_callback_clock_nanos(valid_at: Instant, source: CallbackClockSource) -> Option<u64> {
    match source {
        CallbackClockSource::Instant => Some(instant_nanos(valid_at)),
        #[cfg(windows)]
        CallbackClockSource::Qpc => current_qpc_nanos(),
    }
}

fn load_callback_clock_snapshot_now() -> (Instant, u64, CallbackClockSource, CallbackClockWindow) {
    deadsync_audio::load_callback_clock_snapshot_now(current_callback_clock_nanos)
}

#[inline(always)]
fn stream_position_frames_from_window(
    sample_rate: u32,
    start_frame: u64,
    at_nanos: u64,
    window: CallbackClockWindow,
) -> f64 {
    if let Some(frames) =
        audio_stream_position_frames_from_window(sample_rate, start_frame, at_nanos, window)
    {
        return frames;
    }
    if timing_diag_enabled() {
        debug!(
            "AUDIO_DIAG stream_pos_fallback sample_rate_hz={} at_nanos={} last_nanos={} last_base_frames={} last_callback_frames={} prev_nanos={} prev_base_frames={} prev_callback_frames={} total_frames={} start_frame={}",
            sample_rate,
            at_nanos,
            window.last_nanos,
            window.last_base_frames,
            window.last_callback_frames,
            window.prev_nanos,
            window.prev_base_frames,
            window.prev_callback_frames,
            window.total_frames,
            start_frame,
        );
    }
    fallback_stream_position_frames(start_frame, window)
}

#[inline(always)]
fn music_stream_clock_snapshot_at_nanos(
    sample_rate: u32,
    start: u64,
    valid_at: Instant,
    at_nanos: u64,
    source: CallbackClockSource,
    window: CallbackClockWindow,
) -> MusicStreamClockSnapshot {
    let stream_frames = stream_position_frames_from_window(sample_rate, start, at_nanos, window);
    let stream_seconds = (stream_frames / sample_rate as f64) as f32;
    let (music_seconds, music_seconds_per_second, has_music_mapping) =
        match lookup_music_position(stream_frames, sample_rate) {
            Some((music_seconds, slope)) => (music_seconds, slope, true),
            None => match seeded_music_position(stream_seconds) {
                Some((music_seconds, slope)) => (music_seconds, slope, true),
                None => (stream_seconds, 1.0, false),
            },
        };
    MusicStreamClockSnapshot {
        stream_seconds,
        music_seconds,
        music_nanos: music_nanos_from_seconds(music_seconds as f64),
        music_seconds_per_second,
        has_music_mapping,
        valid_at,
        valid_at_host_nanos: match source {
            #[cfg(windows)]
            CallbackClockSource::Qpc => at_nanos,
            #[cfg(windows)]
            CallbackClockSource::Instant => 0,
            #[cfg(not(windows))]
            CallbackClockSource::Instant => at_nanos,
        },
    }
}

/// Returns the current stream position and the `Instant` it is valid for.
pub fn music_stream_clock_snapshot(sample_rate: u32) -> MusicStreamClockSnapshot {
    let sample_rate = sample_rate.max(1);
    if !music_track_has_started() {
        if let Some((music_seconds, slope)) = seeded_music_position(0.0) {
            return MusicStreamClockSnapshot {
                stream_seconds: 0.0,
                music_seconds,
                music_nanos: music_nanos_from_seconds(f64::from(music_seconds)),
                music_seconds_per_second: slope,
                has_music_mapping: true,
                valid_at: Instant::now(),
                valid_at_host_nanos: 0,
            };
        }
        return MusicStreamClockSnapshot {
            stream_seconds: 0.0,
            music_seconds: 0.0,
            music_nanos: 0,
            music_seconds_per_second: 1.0,
            has_music_mapping: false,
            valid_at: Instant::now(),
            valid_at_host_nanos: 0,
        };
    }
    let start = music_track_start_frame();
    let (valid_at, at_nanos, source, window) = load_callback_clock_snapshot_now();
    music_stream_clock_snapshot_at_nanos(sample_rate, start, valid_at, at_nanos, source, window)
}
