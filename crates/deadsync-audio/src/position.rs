use crate::MusicMapSeg;
use std::collections::VecDeque;

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
        MUSIC_POS_MAP_BACKLOG_FRAMES, PlaybackPosMap, fallback_music_position,
        music_clock_seed_enabled, music_nanos_from_seconds, normalized_music_rate,
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
