// Frozen from 8da49e3d5, before query optimizations.
use std::collections::VecDeque;
pub fn search(queue: &VecDeque<MusicMapSeg>, stream_frame: f64) -> Option<(f64, f64)> {
    if queue.is_empty() || !stream_frame.is_finite() {
        return None;
    }
    let mut closest = None;
    let mut closest_dist = f64::INFINITY;
    for seg in queue {
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
            closest = Some((seg, start, false));
        }
        let end_dist = (stream_frame - end).abs();
        if end_dist < closest_dist {
            closest_dist = end_dist;
            closest = Some((seg, end, true));
        }
    }
    // Only extrapolate if no segment contained the query, retaining the
    // chosen endpoint and its arithmetic order.
    closest.map(|(seg, anchor, at_end)| {
        let music = if at_end {
            seg.music_sec_per_frame
                .mul_add(seg.frames as f64, seg.music_start_sec)
        } else {
            seg.music_start_sec
        };
        (
            (stream_frame - anchor).mul_add(seg.music_sec_per_frame, music),
            seg.music_sec_per_frame,
        )
    })
}

pub fn invert(queue: &VecDeque<MusicMapSeg>, music_seconds: f64) -> Option<f64> {
    if queue.is_empty() || !music_seconds.is_finite() {
        return None;
    }
    let mut closest = None;
    let mut closest_dist = f64::INFINITY;
    for seg in queue {
        let sec_per_frame = seg.music_sec_per_frame;
        // Insertion rejects non-finite slopes; zero slopes remain non-invertible.
        if sec_per_frame == 0.0 {
            continue;
        }
        let start_sec = seg.music_start_sec;
        let end_sec = sec_per_frame.mul_add(seg.frames as f64, start_sec);
        let (lo, hi) = if start_sec <= end_sec {
            (start_sec, end_sec)
        } else {
            (end_sec, start_sec)
        };
        if music_seconds >= lo && music_seconds < hi {
            return Some(
                seg.stream_frame_start as f64 + (music_seconds - start_sec) / sec_per_frame,
            );
        }
        let clamped = music_seconds.clamp(lo, hi);
        let dist = (music_seconds - clamped).abs();
        if dist < closest_dist {
            closest_dist = dist;
            closest = Some(seg);
        }
    }
    // Distance selection needs no division. Keep the first nearest segment
    // on ties and convert only the selected result back to a stream frame.
    closest.map(|seg| {
        seg.stream_frame_start as f64
            + (music_seconds - seg.music_start_sec) / seg.music_sec_per_frame
    })
}
