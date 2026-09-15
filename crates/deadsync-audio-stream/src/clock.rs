use crate::MusicClock;
use deadlib_audio_core::{
    CallbackClockSource, CallbackClockWindow, MusicStreamClockSnapshot,
    fallback_stream_position_frames, music_nanos_from_seconds, music_track_has_started,
    music_track_start_frame, seeded_music_position,
    stream_position_frames_from_window as audio_stream_position_frames_from_window,
};
use deadlib_platform::host_time::instant_nanos;
#[cfg(windows)]
use deadlib_platform::windows_rt::current_qpc_nanos;
use log::debug;
use std::time::Instant;

#[inline(always)]
#[must_use]
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
    deadlib_audio_core::load_callback_clock_snapshot_now(current_callback_clock_nanos)
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
    clock: &MusicClock,
    sample_rate: u32,
    start: u64,
    valid_at: Instant,
    at_nanos: u64,
    source: CallbackClockSource,
    window: CallbackClockWindow,
) -> MusicStreamClockSnapshot {
    let stream_frames = stream_position_frames_from_window(sample_rate, start, at_nanos, window);
    let stream_seconds = stream_frames / f64::from(sample_rate);
    let (music_seconds, music_seconds_per_second, has_music_mapping) =
        match clock.lookup(stream_frames) {
            Some((music_seconds, slope)) => (music_seconds, slope, true),
            None => match seeded_music_position(stream_seconds) {
                Some((music_seconds, slope)) => (music_seconds, slope, true),
                None => (stream_seconds, 1.0, false),
            },
        };
    MusicStreamClockSnapshot {
        stream_seconds: stream_seconds as f32,
        music_nanos: music_nanos_from_seconds(music_seconds),
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

impl MusicClock {
    /// Returns the current stream position and the `Instant` it is valid for.
    pub fn snapshot(&mut self) -> MusicStreamClockSnapshot {
        let sample_rate = self.sample_rate();
        if !music_track_has_started() {
            if let Some((music_seconds, slope)) = seeded_music_position(0.0) {
                return MusicStreamClockSnapshot {
                    stream_seconds: 0.0,
                    music_nanos: music_nanos_from_seconds(music_seconds),
                    music_seconds_per_second: slope,
                    has_music_mapping: true,
                    valid_at: Instant::now(),
                    valid_at_host_nanos: 0,
                };
            }
            return MusicStreamClockSnapshot {
                stream_seconds: 0.0,
                music_nanos: 0,
                music_seconds_per_second: 1.0,
                has_music_mapping: false,
                valid_at: Instant::now(),
                valid_at_host_nanos: 0,
            };
        }
        let start = music_track_start_frame();
        let (valid_at, at_nanos, source, window) = load_callback_clock_snapshot_now();
        music_stream_clock_snapshot_at_nanos(
            self,
            sample_rate,
            start,
            valid_at,
            at_nanos,
            source,
            window,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music_map::clear_music_pos_map;
    use deadlib_audio_core::{
        CallbackInfo, MixControls, MusicBlockTiming, OutputBufferMut, RenderState,
        activate_music_track, clear_music_stream_clock_seed, music_map_generation, music_transport,
        reset_music_stream_clock_state, seed_music_stream_clock,
    };
    use std::sync::Arc;

    const SAMPLE_RATE: u32 = 48_000;
    const ANCHOR_NANOS: u64 = 1_000_000_000;

    fn snapshot_at(clock: &MusicClock, elapsed_nanos: u64) -> MusicStreamClockSnapshot {
        let start = music_track_start_frame();
        let whole_seconds = elapsed_nanos / 1_000_000_000;
        music_stream_clock_snapshot_at_nanos(
            clock,
            SAMPLE_RATE,
            start,
            Instant::now(),
            ANCHOR_NANOS + elapsed_nanos % 1_000_000_000,
            CallbackClockSource::Instant,
            CallbackClockWindow {
                last_nanos: ANCHOR_NANOS + 1,
                last_base_frames: start + whole_seconds * u64::from(SAMPLE_RATE),
                last_callback_frames: 256,
                ..Default::default()
            },
        )
    }

    #[test]
    fn snapshots_preserve_precision_across_playback_states() {
        // Keep the global clock/transport transitions in one test. Other tests
        // in this crate do not use the process-global playback clock.
        for (origin, origin_nanos) in [
            (0.000_000_1, 100i64),
            (3_600.000_1, 3_600_000_100_000),
            (8_192.000_4, 8_192_000_400_000),
            (-3_600.000_1, -3_600_000_100_000),
        ] {
            for rate in [0.5f32, 1.0, 1.5, 2.0] {
                reset_music_stream_clock_state();
                clear_music_pos_map();
                let (mut stream, render_handle) = music_transport(SAMPLE_RATE, 2);
                let mut clock = MusicClock::new(stream.played_map, SAMPLE_RATE);

                let unstarted = clock.snapshot();
                assert_eq!(unstarted.music_nanos, 0);
                assert!(!unstarted.has_music_mapping);

                seed_music_stream_clock(origin, rate);
                let seeded = clock.snapshot();
                assert_eq!(seeded.music_nanos, origin_nanos, "seed origin={origin}");
                assert_eq!(seeded.music_seconds_per_second, rate);
                assert!(seeded.has_music_mapping);
                assert_eq!(seeded.valid_at_host_nanos, 0);

                // An empty played map must preserve both the cut origin and
                // fractional stream time, even after hours of elapsed playback.
                activate_music_track();
                for elapsed_nanos in [100_000, 3_600_000_100_000, 8_192_000_400_000] {
                    let fallback = snapshot_at(&clock, elapsed_nanos);
                    let delta = (elapsed_nanos as f64 * f64::from(rate)).round() as i64;
                    assert_eq!(fallback.music_nanos, origin_nanos + delta);
                    assert_eq!(fallback.music_seconds_per_second, rate);
                    assert!(fallback.has_music_mapping);
                }

                clear_music_stream_clock_seed();
                let unmapped = snapshot_at(&clock, 3_600_000_100_000);
                assert_eq!(unmapped.music_nanos, 3_600_000_100_000);
                assert_eq!(unmapped.music_seconds_per_second, 1.0);
                assert!(!unmapped.has_music_mapping);

                let generation = music_map_generation();
                assert_eq!(
                    stream.writer.try_push(
                        &[0; 512],
                        MusicBlockTiming {
                            generation,
                            music_start_sec: origin,
                            music_sec_per_frame: f64::from(rate) / f64::from(SAMPLE_RATE),
                        },
                    ),
                    512,
                );
                stream.writer.publish_generation_ready(generation);
                let mut render = RenderState::new(render_handle, Arc::new(MixControls::new()), 2);
                render.render(
                    OutputBufferMut::F32(&mut [0.0; 512]),
                    CallbackInfo {
                        anchor_nanos: ANCHOR_NANOS,
                        clock: CallbackClockSource::Instant,
                    },
                    [],
                );
                // Once the callback supplies a map, it takes precedence over
                // the seed, including its playback slope.
                seed_music_stream_clock(123.0, 3.0);
                for elapsed_nanos in [0, 10_000, 20_834, 100_000, 400_000] {
                    let mapped = snapshot_at(&clock, elapsed_nanos);
                    let delta = (elapsed_nanos as f64 * f64::from(rate)).round() as i64;
                    assert_eq!(mapped.music_nanos, origin_nanos + delta);
                    assert_eq!(mapped.music_seconds_per_second, rate);
                    assert!(mapped.has_music_mapping);
                }
            }
        }
        reset_music_stream_clock_state();
        clear_music_pos_map();
    }
}
