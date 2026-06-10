use crate::MusicMapSeg;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

pub const MUSIC_POS_MAP_BACKLOG_FRAMES: i64 = 80_000;
const NANOS_PER_SECOND: f64 = 1_000_000_000.0;

#[inline(always)]
pub fn music_nanos_from_seconds(seconds: f64) -> i64 {
    if !seconds.is_finite() {
        return 0;
    }
    let nanos = (seconds * NANOS_PER_SECOND).round();
    nanos.clamp(i64::MIN as f64, i64::MAX as f64) as i64
}

#[inline(always)]
pub fn normalized_music_rate(rate: f32) -> f32 {
    if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    }
}

#[inline(always)]
pub fn fallback_music_position(stream_seconds: f32, cut_start_sec: f64, rate: f32) -> (f32, f32) {
    let rate = normalized_music_rate(rate);
    let stream_seconds = if stream_seconds.is_finite() {
        stream_seconds.max(0.0)
    } else {
        0.0
    };
    let cut_start_sec = if cut_start_sec.is_finite() {
        cut_start_sec
    } else {
        0.0
    };
    if cut_start_sec < 0.0 {
        let lead_in = (-cut_start_sec) as f32;
        if stream_seconds < lead_in {
            return ((cut_start_sec + f64::from(stream_seconds)) as f32, 1.0);
        }
        return ((stream_seconds - lead_in) * rate, rate);
    }
    (
        (cut_start_sec + f64::from(stream_seconds * rate)) as f32,
        rate,
    )
}

#[inline(always)]
pub fn music_clock_seed_enabled(cut_start_sec: f64) -> bool {
    cut_start_sec.is_finite() && cut_start_sec > 0.0
}

#[derive(Clone, Copy, Debug)]
pub struct MusicStreamClockSnapshot {
    pub stream_seconds: f32,
    pub music_seconds: f32,
    pub music_nanos: i64,
    pub music_seconds_per_second: f32,
    pub has_music_mapping: bool,
    pub valid_at: Instant,
    // Host/QPC clock for `valid_at` when the backend publishes one; 0 means
    // the snapshot only has a local `Instant` anchor.
    pub valid_at_host_nanos: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CallbackClockSource {
    Instant = 1,
    #[cfg(windows)]
    Qpc = 2,
}

impl CallbackClockSource {
    #[inline(always)]
    fn load() -> Self {
        match CALLBACK_CLOCK_SOURCE.load(Ordering::Relaxed) {
            #[cfg(windows)]
            2 => Self::Qpc,
            _ => Self::Instant,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CallbackClockWindow {
    pub total_frames: u64,
    pub last_nanos: u64,
    pub last_base_frames: u64,
    pub last_callback_frames: u64,
    pub prev_nanos: u64,
    pub prev_base_frames: u64,
    pub prev_callback_frames: u64,
}

// Global playback position tracking for the current music stream.
// All counters are in device frames, not interleaved samples.
static MUSIC_TOTAL_FRAMES: AtomicU64 = AtomicU64::new(0);
static MUSIC_TRACK_START_FRAME: AtomicU64 = AtomicU64::new(0);
static MUSIC_TRACK_HAS_STARTED: AtomicBool = AtomicBool::new(false);
static MUSIC_TRACK_ACTIVE: AtomicBool = AtomicBool::new(false);
// Song-time fallback for the current/pending music stream. The precise
// packet map is published by the audio callback, but gameplay can query the
// clock before the first mapped packet has been consumed.
static MUSIC_CLOCK_SEEDED: AtomicBool = AtomicBool::new(false);
static MUSIC_CLOCK_CUT_START_BITS: AtomicU64 = AtomicU64::new(0.0f64.to_bits());
static MUSIC_CLOCK_RATE_BITS: AtomicU32 = AtomicU32::new(1.0f32.to_bits());
// Per-play monotonic id used to associate asynchronous ReplayGain results
// with the track that requested them.
static MUSIC_TRACK_ID: AtomicU64 = AtomicU64::new(0);
// Target linear gain for the music stream. The mixer interpolates its
// current gain toward this value, so cache-miss to cache-hit ReplayGain
// transitions do not produce an audible step.
static MUSIC_TARGET_GAIN_BITS: AtomicU32 = AtomicU32::new(1.0f32.to_bits());
// Generation counter incremented whenever a track boundary should snap the
// mixer's interpolated gain to its target instantly.
static MUSIC_GAIN_SNAP_GEN: AtomicU64 = AtomicU64::new(0);

// Last audio callback timing, used to interpolate playback position between
// callback invocations so reported stream time advances continuously.
static CALLBACK_CLOCK_SEQ: AtomicU64 = AtomicU64::new(0);
static CALLBACK_CLOCK_SOURCE: AtomicU8 = AtomicU8::new(CallbackClockSource::Instant as u8);
// Stored as elapsed nanos + 1 from the shared process host-clock epoch; 0 means
// "no callback yet".
static LAST_CALLBACK_ELAPSED_NANOS: AtomicU64 = AtomicU64::new(0);
static LAST_CALLBACK_BASE_FRAMES: AtomicU64 = AtomicU64::new(0);
static LAST_CALLBACK_FRAMES: AtomicU64 = AtomicU64::new(0);
static PREV_CALLBACK_ELAPSED_NANOS: AtomicU64 = AtomicU64::new(0);
static PREV_CALLBACK_BASE_FRAMES: AtomicU64 = AtomicU64::new(0);
static PREV_CALLBACK_FRAMES: AtomicU64 = AtomicU64::new(0);
static AUDIO_TIMING_DIAG_LAST_SOURCE: AtomicU8 = AtomicU8::new(0);
static AUDIO_TIMING_DIAG_LAST_NANOS: AtomicU64 = AtomicU64::new(0);
static AUDIO_TIMING_DIAG_LAST_GAP_NS: AtomicU64 = AtomicU64::new(0);

#[inline(always)]
pub fn seed_music_stream_clock(cut_start_sec: f64, rate: f32) {
    MUSIC_CLOCK_CUT_START_BITS.store(cut_start_sec.to_bits(), Ordering::Relaxed);
    MUSIC_CLOCK_RATE_BITS.store(normalized_music_rate(rate).to_bits(), Ordering::Relaxed);
    MUSIC_CLOCK_SEEDED.store(music_clock_seed_enabled(cut_start_sec), Ordering::Release);
}

#[inline(always)]
pub fn clear_music_stream_clock_seed() {
    MUSIC_CLOCK_CUT_START_BITS.store(0.0f64.to_bits(), Ordering::Relaxed);
    MUSIC_CLOCK_RATE_BITS.store(1.0f32.to_bits(), Ordering::Relaxed);
    MUSIC_CLOCK_SEEDED.store(false, Ordering::Release);
}

#[inline(always)]
pub fn set_music_clock_rate(rate: f32) {
    MUSIC_CLOCK_RATE_BITS.store(normalized_music_rate(rate).to_bits(), Ordering::Relaxed);
}

#[inline(always)]
pub fn seeded_music_position(stream_seconds: f32) -> Option<(f32, f32)> {
    if !MUSIC_CLOCK_SEEDED.load(Ordering::Acquire) {
        return None;
    }
    let cut_start_sec = f64::from_bits(MUSIC_CLOCK_CUT_START_BITS.load(Ordering::Relaxed));
    let rate = f32::from_bits(MUSIC_CLOCK_RATE_BITS.load(Ordering::Relaxed));
    Some(fallback_music_position(stream_seconds, cut_start_sec, rate))
}

#[inline(always)]
pub fn reset_music_stream_clock_state() {
    let total = MUSIC_TOTAL_FRAMES.load(Ordering::Acquire);
    MUSIC_TRACK_START_FRAME.store(total, Ordering::Release);
    MUSIC_TRACK_HAS_STARTED.store(false, Ordering::Release);
    MUSIC_TRACK_ACTIVE.store(false, Ordering::Release);
    clear_music_stream_clock_seed();
}

#[inline(always)]
pub fn activate_music_track() {
    MUSIC_TRACK_ACTIVE.store(true, Ordering::Relaxed);
    MUSIC_TRACK_HAS_STARTED.store(false, Ordering::Relaxed);
}

#[inline(always)]
pub fn stop_music_track() {
    MUSIC_TRACK_ACTIVE.store(false, Ordering::Relaxed);
    MUSIC_TRACK_HAS_STARTED.store(false, Ordering::Relaxed);
}

#[inline(always)]
pub fn mark_music_track_started(total_before: u64) {
    if MUSIC_TRACK_ACTIVE.load(Ordering::Relaxed)
        && !MUSIC_TRACK_HAS_STARTED.load(Ordering::Acquire)
    {
        MUSIC_TRACK_START_FRAME.store(total_before, Ordering::Release);
        MUSIC_TRACK_HAS_STARTED.store(true, Ordering::Release);
    }
}

#[inline(always)]
pub fn music_track_active() -> bool {
    MUSIC_TRACK_ACTIVE.load(Ordering::Acquire)
}

#[inline(always)]
pub fn music_track_active_relaxed() -> bool {
    MUSIC_TRACK_ACTIVE.load(Ordering::Relaxed)
}

#[inline(always)]
pub fn music_track_has_started() -> bool {
    MUSIC_TRACK_HAS_STARTED.load(Ordering::Acquire)
}

#[inline(always)]
pub fn music_track_start_frame() -> u64 {
    MUSIC_TRACK_START_FRAME.load(Ordering::Acquire)
}

#[inline(always)]
pub fn music_total_frames() -> u64 {
    MUSIC_TOTAL_FRAMES.load(Ordering::Relaxed)
}

#[inline(always)]
pub fn next_music_track_id() -> u64 {
    MUSIC_TRACK_ID
        .fetch_add(1, Ordering::AcqRel)
        .wrapping_add(1)
}

#[inline(always)]
pub fn active_music_track_id() -> u64 {
    MUSIC_TRACK_ID.load(Ordering::Acquire)
}

#[inline(always)]
pub fn set_music_target_gain(gain: f32) {
    let gain = if gain.is_finite() && gain > 0.0 {
        gain
    } else {
        1.0
    };
    MUSIC_TARGET_GAIN_BITS.store(gain.to_bits(), Ordering::Relaxed);
}

#[inline(always)]
pub fn reset_music_target_gain() {
    MUSIC_TARGET_GAIN_BITS.store(1.0f32.to_bits(), Ordering::Relaxed);
}

#[inline(always)]
pub fn music_target_gain() -> f32 {
    f32::from_bits(MUSIC_TARGET_GAIN_BITS.load(Ordering::Relaxed))
}

#[inline(always)]
pub fn snap_music_gain_generation() {
    MUSIC_GAIN_SNAP_GEN.fetch_add(1, Ordering::Release);
}

#[inline(always)]
pub fn music_gain_snap_generation() -> u64 {
    MUSIC_GAIN_SNAP_GEN.load(Ordering::Acquire)
}

#[inline(always)]
pub fn timing_diag_last_callback_gap_ns() -> u64 {
    AUDIO_TIMING_DIAG_LAST_GAP_NS.load(Ordering::Relaxed)
}

#[inline(always)]
pub fn note_timing_diag_callback_gap(
    anchor_nanos: u64,
    source: CallbackClockSource,
) -> Option<u64> {
    if anchor_nanos == 0 {
        return None;
    }
    let source_id = source as u8;
    let prev_source = AUDIO_TIMING_DIAG_LAST_SOURCE.swap(source_id, Ordering::Relaxed);
    let prev_nanos = if prev_source == source_id {
        AUDIO_TIMING_DIAG_LAST_NANOS.swap(anchor_nanos, Ordering::Relaxed)
    } else {
        AUDIO_TIMING_DIAG_LAST_NANOS.store(anchor_nanos, Ordering::Relaxed);
        0
    };
    if prev_nanos == 0 || anchor_nanos < prev_nanos {
        return None;
    }
    let gap_ns = anchor_nanos - prev_nanos;
    AUDIO_TIMING_DIAG_LAST_GAP_NS.store(gap_ns, Ordering::Relaxed);
    Some(gap_ns)
}

#[inline(always)]
fn begin_callback_clock_write() {
    CALLBACK_CLOCK_SEQ.fetch_add(1, Ordering::AcqRel);
}

#[inline(always)]
fn end_callback_clock_write() {
    CALLBACK_CLOCK_SEQ.fetch_add(1, Ordering::Release);
}

#[inline(always)]
pub fn publish_callback_window_start_nanos(
    total_before: u64,
    anchor_nanos: u64,
    source: CallbackClockSource,
) {
    begin_callback_clock_write();
    CALLBACK_CLOCK_SOURCE.store(source as u8, Ordering::Relaxed);
    PREV_CALLBACK_BASE_FRAMES.store(
        LAST_CALLBACK_BASE_FRAMES.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    PREV_CALLBACK_FRAMES.store(
        LAST_CALLBACK_FRAMES.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    PREV_CALLBACK_ELAPSED_NANOS.store(
        LAST_CALLBACK_ELAPSED_NANOS.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    LAST_CALLBACK_BASE_FRAMES.store(total_before, Ordering::Relaxed);
    LAST_CALLBACK_FRAMES.store(0, Ordering::Relaxed);
    LAST_CALLBACK_ELAPSED_NANOS.store(
        anchor_nanos.min(u64::MAX - 1).saturating_add(1),
        Ordering::Relaxed,
    );
    end_callback_clock_write();
}

#[inline(always)]
pub fn publish_callback_window_end(total_before: u64, frames: u64) {
    begin_callback_clock_write();
    LAST_CALLBACK_FRAMES.store(frames, Ordering::Relaxed);
    MUSIC_TOTAL_FRAMES.store(total_before.saturating_add(frames), Ordering::Relaxed);
    end_callback_clock_write();
}

pub fn load_callback_clock_snapshot_now<F>(
    mut clock_nanos: F,
) -> (Instant, u64, CallbackClockSource, CallbackClockWindow)
where
    F: FnMut(Instant, CallbackClockSource) -> Option<u64>,
{
    loop {
        let seq_start = CALLBACK_CLOCK_SEQ.load(Ordering::Acquire);
        if seq_start & 1 != 0 {
            std::hint::spin_loop();
            continue;
        }
        let source = CallbackClockSource::load();
        let valid_at = Instant::now();
        let at_nanos = clock_nanos(valid_at, source);
        let window = CallbackClockWindow {
            total_frames: MUSIC_TOTAL_FRAMES.load(Ordering::Relaxed),
            last_nanos: LAST_CALLBACK_ELAPSED_NANOS.load(Ordering::Relaxed),
            last_base_frames: LAST_CALLBACK_BASE_FRAMES.load(Ordering::Relaxed),
            last_callback_frames: LAST_CALLBACK_FRAMES.load(Ordering::Relaxed),
            prev_nanos: PREV_CALLBACK_ELAPSED_NANOS.load(Ordering::Relaxed),
            prev_base_frames: PREV_CALLBACK_BASE_FRAMES.load(Ordering::Relaxed),
            prev_callback_frames: PREV_CALLBACK_FRAMES.load(Ordering::Relaxed),
        };
        let seq_end = CALLBACK_CLOCK_SEQ.load(Ordering::Acquire);
        if seq_start == seq_end {
            let at_nanos = at_nanos.unwrap_or(window.last_nanos.saturating_sub(1));
            return (valid_at, at_nanos, source, window);
        }
    }
}

#[inline(always)]
fn stream_position_frames_from_callback(
    sample_rate: u32,
    start_frame: u64,
    at_nanos: u64,
    cb_nanos_plus_one: u64,
    base_frames: u64,
    buf_frames: u64,
) -> Option<f64> {
    if cb_nanos_plus_one == 0 {
        return None;
    }
    let cb_nanos = cb_nanos_plus_one.saturating_sub(1);
    if at_nanos < cb_nanos {
        return None;
    }
    let dt = (at_nanos.saturating_sub(cb_nanos) as f64) * 1e-9;
    let frames_since_cb = (dt * sample_rate as f64).clamp(0.0, buf_frames as f64);
    let frames_now = base_frames as f64 + frames_since_cb;
    Some((frames_now.max(start_frame as f64) - start_frame as f64).max(0.0))
}

#[inline(always)]
fn stream_position_frames_from_anchor_pair(
    start_frame: u64,
    at_nanos: u64,
    earlier_nanos_plus_one: u64,
    earlier_base_frames: u64,
    later_nanos_plus_one: u64,
    later_base_frames: u64,
) -> Option<f64> {
    if earlier_nanos_plus_one == 0 || later_nanos_plus_one == 0 {
        return None;
    }
    let earlier_nanos = earlier_nanos_plus_one.saturating_sub(1);
    let later_nanos = later_nanos_plus_one.saturating_sub(1);
    if later_nanos <= earlier_nanos || later_base_frames <= earlier_base_frames {
        return None;
    }
    let nanos_span = later_nanos.saturating_sub(earlier_nanos) as f64;
    if nanos_span <= 0.0 {
        return None;
    }
    let frames_per_ns = (later_base_frames - earlier_base_frames) as f64 / nanos_span;
    if !frames_per_ns.is_finite() || frames_per_ns <= 0.0 {
        return None;
    }
    let dt_ns = at_nanos as f64 - later_nanos as f64;
    let frames_now = later_base_frames as f64 + dt_ns * frames_per_ns;
    Some((frames_now.max(start_frame as f64) - start_frame as f64).max(0.0))
}

#[inline(always)]
pub fn stream_position_frames_from_window(
    sample_rate: u32,
    start_frame: u64,
    at_nanos: u64,
    window: CallbackClockWindow,
) -> Option<f64> {
    stream_position_frames_from_callback(
        sample_rate,
        start_frame,
        at_nanos,
        window.last_nanos,
        window.last_base_frames,
        window.last_callback_frames,
    )
    .or_else(|| {
        stream_position_frames_from_callback(
            sample_rate,
            start_frame,
            at_nanos,
            window.prev_nanos,
            window.prev_base_frames,
            window.prev_callback_frames,
        )
    })
    .or_else(|| {
        stream_position_frames_from_anchor_pair(
            start_frame,
            at_nanos,
            window.prev_nanos,
            window.prev_base_frames,
            window.last_nanos,
            window.last_base_frames,
        )
    })
}

#[inline(always)]
pub fn fallback_stream_position_frames(start_frame: u64, window: CallbackClockWindow) -> f64 {
    window.total_frames.saturating_sub(start_frame) as f64
}

#[derive(Default)]
pub struct PlaybackPosMap {
    queue: VecDeque<MusicMapSeg>,
    backlog_frames: i64,
}

impl PlaybackPosMap {
    pub fn clear(&mut self) {
        self.queue.clear();
        self.backlog_frames = 0;
    }

    pub fn insert(&mut self, seg: MusicMapSeg) {
        if seg.frames <= 0
            || !seg.music_start_sec.is_finite()
            || !seg.music_sec_per_frame.is_finite()
        {
            return;
        }
        if let Some(last) = self.queue.back_mut() {
            let contiguous_stream = last.stream_frame_start + last.frames == seg.stream_frame_start;
            let ratio_match = (last.music_sec_per_frame - seg.music_sec_per_frame).abs() <= 1e-9;
            let expected_music_start =
                last.music_start_sec + last.music_sec_per_frame * last.frames as f64;
            let music_contiguous = (expected_music_start - seg.music_start_sec).abs()
                <= seg.music_sec_per_frame.abs().max(1e-9);
            if contiguous_stream && ratio_match && music_contiguous {
                last.frames += seg.frames;
                self.backlog_frames = self.backlog_frames.saturating_add(seg.frames);
                self.cleanup();
                return;
            }
        }
        self.backlog_frames = self.backlog_frames.saturating_add(seg.frames);
        self.queue.push_back(seg);
        self.cleanup();
    }

    fn cleanup(&mut self) {
        while self.backlog_frames > MUSIC_POS_MAP_BACKLOG_FRAMES {
            let Some(front) = self.queue.front_mut() else {
                self.backlog_frames = 0;
                break;
            };
            let excess = self.backlog_frames - MUSIC_POS_MAP_BACKLOG_FRAMES;
            let drop = excess.min(front.frames);
            front.stream_frame_start += drop;
            front.music_start_sec += front.music_sec_per_frame * drop as f64;
            front.frames -= drop;
            self.backlog_frames -= drop;
            if front.frames <= 0 {
                self.queue.pop_front();
            }
        }
    }

    pub fn search(&self, stream_frame: f64) -> Option<(f64, f64)> {
        if self.queue.is_empty() || !stream_frame.is_finite() {
            return None;
        }
        let mut closest = None;
        let mut closest_dist = f64::INFINITY;
        for seg in &self.queue {
            let start = seg.stream_frame_start as f64;
            let end = start + seg.frames as f64;
            if stream_frame >= start && stream_frame < end {
                let diff = stream_frame - start;
                return Some((
                    seg.music_start_sec + diff * seg.music_sec_per_frame,
                    seg.music_sec_per_frame,
                ));
            }
            let start_dist = (stream_frame - start).abs();
            if start_dist < closest_dist {
                closest_dist = start_dist;
                closest = Some((
                    seg.music_start_sec + (stream_frame - start) * seg.music_sec_per_frame,
                    seg.music_sec_per_frame,
                ));
            }
            let end_music = seg.music_start_sec + seg.music_sec_per_frame * seg.frames as f64;
            let end_dist = (stream_frame - end).abs();
            if end_dist < closest_dist {
                closest_dist = end_dist;
                closest = Some((
                    end_music + (stream_frame - end) * seg.music_sec_per_frame,
                    seg.music_sec_per_frame,
                ));
            }
        }
        closest
    }

    /// Inverse of [`search`]: given a music position in seconds, return the
    /// track-relative stream frame at which it plays. Prefers the segment that
    /// contains `music_seconds`; otherwise extrapolates from the nearest segment.
    pub fn invert(&self, music_seconds: f64) -> Option<f64> {
        if self.queue.is_empty() || !music_seconds.is_finite() {
            return None;
        }
        let mut closest = None;
        let mut closest_dist = f64::INFINITY;
        for seg in &self.queue {
            let sec_per_frame = seg.music_sec_per_frame;
            if !sec_per_frame.is_finite() || sec_per_frame == 0.0 {
                continue;
            }
            let start_sec = seg.music_start_sec;
            let end_sec = start_sec + sec_per_frame * seg.frames as f64;
            let (lo, hi) = if start_sec <= end_sec {
                (start_sec, end_sec)
            } else {
                (end_sec, start_sec)
            };
            let frame = seg.stream_frame_start as f64 + (music_seconds - start_sec) / sec_per_frame;
            if music_seconds >= lo && music_seconds < hi {
                return Some(frame);
            }
            let clamped = music_seconds.clamp(lo, hi);
            let dist = (music_seconds - clamped).abs();
            if dist < closest_dist {
                closest_dist = dist;
                closest = Some(frame);
            }
        }
        closest
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CallbackClockWindow, MUSIC_POS_MAP_BACKLOG_FRAMES, PlaybackPosMap, fallback_music_position,
        music_clock_seed_enabled, music_nanos_from_seconds, normalized_music_rate,
        stream_position_frames_from_window,
    };
    use crate::MusicMapSeg;

    #[test]
    fn fallback_music_position_uses_positive_cut_origin() {
        let (music_sec, slope) = fallback_music_position(0.25, 37.5, 1.5);

        assert!((music_sec - 37.875).abs() <= 0.000_01);
        assert!((slope - 1.5).abs() <= 0.000_01);
    }

    #[test]
    fn fallback_music_position_keeps_negative_lead_in_unscaled() {
        let (lead_music_sec, lead_slope) = fallback_music_position(0.75, -1.0, 2.0);
        let (song_music_sec, song_slope) = fallback_music_position(1.25, -1.0, 2.0);

        assert!((lead_music_sec - -0.25).abs() <= 0.000_01);
        assert!((lead_slope - 1.0).abs() <= 0.000_01);
        assert!((song_music_sec - 0.5).abs() <= 0.000_01);
        assert!((song_slope - 2.0).abs() <= 0.000_01);
    }

    #[test]
    fn normalized_music_rate_uses_unity_for_invalid_rate() {
        assert_eq!(normalized_music_rate(1.5), 1.5);
        assert_eq!(normalized_music_rate(0.0), 1.0);
        assert_eq!(normalized_music_rate(-1.0), 1.0);
        assert_eq!(normalized_music_rate(f32::NAN), 1.0);
    }

    #[test]
    fn music_clock_seed_is_only_for_positive_cuts() {
        assert!(music_clock_seed_enabled(0.001));
        assert!(!music_clock_seed_enabled(0.0));
        assert!(!music_clock_seed_enabled(-1.0));
        assert!(!music_clock_seed_enabled(f64::NAN));
    }

    #[test]
    fn music_nanos_from_seconds_rounds_and_rejects_non_finite() {
        assert_eq!(music_nanos_from_seconds(1.25), 1_250_000_000);
        assert_eq!(music_nanos_from_seconds(-0.5), -500_000_000);
        assert_eq!(music_nanos_from_seconds(f64::NAN), 0);
    }

    #[test]
    fn stream_clock_extrapolates_back_before_future_callback_anchor() {
        let frames = stream_position_frames_from_window(
            48_000,
            1_000,
            7_000_000,
            CallbackClockWindow {
                total_frames: 1_720,
                last_nanos: 15_000_001,
                last_base_frames: 1_480,
                last_callback_frames: 240,
                prev_nanos: 10_000_001,
                prev_base_frames: 1_240,
                prev_callback_frames: 240,
            },
        )
        .unwrap();

        assert!((frames - 96.0).abs() <= 1e-6, "frames={frames}");
    }

    #[test]
    fn playback_pos_map_extrapolates_past_last_segment() {
        let mut map = PlaybackPosMap::default();
        map.insert(MusicMapSeg {
            stream_frame_start: 0,
            frames: 48_000,
            music_start_sec: 0.0,
            music_sec_per_frame: 1.0 / 48_000.0,
        });

        let (music_sec, sec_per_frame) = map.search(60_000.0).unwrap();
        assert!((music_sec - 1.25).abs() <= 1e-9, "music_sec={music_sec}");
        assert!(
            (sec_per_frame - (1.0 / 48_000.0)).abs() <= 1e-12,
            "sec_per_frame={sec_per_frame}"
        );
    }

    #[test]
    fn playback_pos_map_trims_large_segment_without_emptying() {
        let mut map = PlaybackPosMap::default();
        map.insert(MusicMapSeg {
            stream_frame_start: 0,
            frames: 48_000,
            music_start_sec: 0.0,
            music_sec_per_frame: 1.0 / 48_000.0,
        });
        map.insert(MusicMapSeg {
            stream_frame_start: 48_000,
            frames: 48_000,
            music_start_sec: 1.0,
            music_sec_per_frame: 1.0 / 48_000.0,
        });

        assert_eq!(map.backlog_frames, MUSIC_POS_MAP_BACKLOG_FRAMES);
        assert_eq!(map.queue.len(), 1);
        let seg = map.queue.front().unwrap();
        assert_eq!(seg.stream_frame_start, 16_000);
        assert_eq!(seg.frames, MUSIC_POS_MAP_BACKLOG_FRAMES);

        let (music_sec, _) = map.search(95_000.0).unwrap();
        assert!((music_sec - (95_000.0 / 48_000.0)).abs() <= 1e-9);
    }

    #[test]
    fn playback_pos_map_invert_round_trips_within_segment() {
        let mut map = PlaybackPosMap::default();
        map.insert(MusicMapSeg {
            stream_frame_start: 1_000,
            frames: 48_000,
            music_start_sec: 2.0,
            music_sec_per_frame: 1.0 / 48_000.0,
        });

        let frame = map.invert(2.5).unwrap();
        assert!((frame - 25_000.0).abs() <= 1e-6, "frame={frame}");

        let (music_sec, _) = map.search(frame).unwrap();
        assert!((music_sec - 2.5).abs() <= 1e-9, "music_sec={music_sec}");
    }

    #[test]
    fn playback_pos_map_invert_extrapolates_past_last_segment() {
        let mut map = PlaybackPosMap::default();
        map.insert(MusicMapSeg {
            stream_frame_start: 0,
            frames: 48_000,
            music_start_sec: 0.0,
            music_sec_per_frame: 1.0 / 48_000.0,
        });

        let frame = map.invert(1.25).unwrap();
        assert!((frame - 60_000.0).abs() <= 1e-6, "frame={frame}");
    }

    #[test]
    fn playback_pos_map_invert_rejects_empty_and_non_finite() {
        let empty = PlaybackPosMap::default();
        assert!(empty.invert(1.0).is_none());

        let mut map = PlaybackPosMap::default();
        map.insert(MusicMapSeg {
            stream_frame_start: 0,
            frames: 48_000,
            music_start_sec: 0.0,
            music_sec_per_frame: 1.0 / 48_000.0,
        });
        assert!(map.invert(f64::NAN).is_none());
    }
}
