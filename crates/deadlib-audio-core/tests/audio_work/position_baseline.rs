// Frozen from 0.5.1215 (a907beb1d).
use crate::MusicMapSeg;
use deadlib_audio_core::MUSIC_POS_MAP_BACKLOG_FRAMES;
use std::collections::VecDeque;

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
            let expected_music_start = last
                .music_sec_per_frame
                .mul_add(last.frames as f64, last.music_start_sec);
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
            front.music_start_sec = front
                .music_sec_per_frame
                .mul_add(drop as f64, front.music_start_sec);
            front.frames -= drop;
            self.backlog_frames -= drop;
            if front.frames <= 0 {
                self.queue.pop_front();
            }
        }
    }

    #[must_use]
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
                    diff.mul_add(seg.music_sec_per_frame, seg.music_start_sec),
                    seg.music_sec_per_frame,
                ));
            }
            let start_dist = (stream_frame - start).abs();
            if start_dist < closest_dist {
                closest_dist = start_dist;
                closest = Some((
                    (stream_frame - start).mul_add(seg.music_sec_per_frame, seg.music_start_sec),
                    seg.music_sec_per_frame,
                ));
            }
            let end_music = seg
                .music_sec_per_frame
                .mul_add(seg.frames as f64, seg.music_start_sec);
            let end_dist = (stream_frame - end).abs();
            if end_dist < closest_dist {
                closest_dist = end_dist;
                closest = Some((
                    (stream_frame - end).mul_add(seg.music_sec_per_frame, end_music),
                    seg.music_sec_per_frame,
                ));
            }
        }
        closest
    }

    /// Inverse of [`search`]: given a music position in seconds, return the
    /// track-relative stream frame at which it plays. Prefers the segment that
    /// contains `music_seconds`; otherwise extrapolates from the nearest segment.
    #[must_use]
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
            let end_sec = sec_per_frame.mul_add(seg.frames as f64, start_sec);
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

pub(super) fn state(map: &PlaybackPosMap) -> (usize, usize, i64) {
    (map.queue.len(), map.queue.capacity(), map.backlog_frames)
}
