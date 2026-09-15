//! Frozen diagnostic wrapper from 263793ae5029de9e9488161808525013894e79f8.
use deadsync_core::song_time::{SongTimeNs, normalized_song_rate, song_time_ns_to_seconds};
use deadsync_gameplay::{SongClockSnapshot, song_clock_music_time_ns};
use std::time::Instant;

#[must_use]
pub fn music_time_ns_from_song_clock(
    snapshot: SongClockSnapshot,
    captured_at: Instant,
    captured_host_nanos: u64,
) -> SongTimeNs {
    let slope = normalized_song_rate(snapshot.seconds_per_second);
    let snapshot_song_time = song_time_ns_to_seconds(snapshot.song_time_ns);
    if snapshot.valid_at_host_nanos != 0 && captured_host_nanos != 0 {
        let dt_nanos = i128::from(captured_host_nanos) - i128::from(snapshot.valid_at_host_nanos);
        if snapshot.timing_diag_enabled {
            log::debug!(
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
            log::debug!(
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
            log::debug!(
                "AUDIO_DIAG snap_age_ms={:.3} path=instant callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
                lead.as_secs_f64() * 1000.0,
                snapshot.timing_diag_callback_gap_ns as f64 * 1e-6,
                snapshot_song_time,
                slope,
                snapshot.valid_at_host_nanos,
                captured_host_nanos,
            );
        }
    } else if snapshot.timing_diag_enabled {
        log::debug!(
            "AUDIO_DIAG snap_age_ms=0.000 path=instant callback_gap_ms={:.3} snapshot_song_time={:.6} slope={:.6} snapshot_host_nanos={} captured_host_nanos={}",
            snapshot.timing_diag_callback_gap_ns as f64 * 1e-6,
            snapshot_song_time,
            slope,
            snapshot.valid_at_host_nanos,
            captured_host_nanos,
        );
    }
    song_clock_music_time_ns(snapshot, captured_at, captured_host_nanos)
}
