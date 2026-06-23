use deadsync_core::timing::beat_to_note_row;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::{TimeSignatureSegment, default_time_signature};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditBeatBarInfo {
    pub frame: u32,
    pub measure_index: Option<i64>,
}

fn valid_sig(sig: TimeSignatureSegment) -> TimeSignatureSegment {
    if sig.numerator > 0 && sig.denominator > 0 {
        sig
    } else {
        default_time_signature()
    }
}

fn sig_at(segments: &[TimeSignatureSegment], index: usize) -> TimeSignatureSegment {
    segments
        .get(index)
        .copied()
        .map(valid_sig)
        .unwrap_or_else(default_time_signature)
}

fn sig_count(segments: &[TimeSignatureSegment]) -> usize {
    segments.len().max(1)
}

fn bar_step_rows(sig: TimeSignatureSegment) -> i32 {
    let sig = valid_sig(sig);
    beat_to_note_row(4.0 / sig.denominator as f32).max(1)
}

fn measure_rows(sig: TimeSignatureSegment) -> i32 {
    bar_step_rows(sig) * valid_sig(sig).numerator.max(1) as i32
}

fn bars_in_segment(start_row: i32, end_row: i32, sig: TimeSignatureSegment) -> i64 {
    let rows = measure_rows(sig).max(1);
    ((end_row - start_row).max(0) / rows) as i64
}

fn measure_index_before(segments: &[TimeSignatureSegment], index: usize) -> i64 {
    let mut total = 0;
    for i in 0..index.min(sig_count(segments)) {
        let sig = sig_at(segments, i);
        let start = beat_to_note_row(if i == 0 { 0.0 } else { segments[i].beat });
        let end = segments
            .get(i + 1)
            .map(|s| beat_to_note_row(s.beat))
            .unwrap_or(start);
        total += bars_in_segment(start, end, sig);
    }
    total
}

fn sig_index_at_row(segments: &[TimeSignatureSegment], row: i32) -> usize {
    if segments.is_empty() {
        return 0;
    }
    let mut idx = 0;
    for (i, sig) in segments.iter().enumerate() {
        if row >= beat_to_note_row(sig.beat) {
            idx = i;
        }
    }
    idx
}

pub fn edit_beat_bar_info_for_row(
    row: i32,
    segments: &[TimeSignatureSegment],
) -> Option<EditBeatBarInfo> {
    let idx = sig_index_at_row(segments, row);
    let sig = sig_at(segments, idx);
    let step = bar_step_rows(sig);
    if step <= 0 || row.rem_euclid(step) != 0 {
        return None;
    }
    let start_row = if segments.is_empty() {
        0
    } else {
        beat_to_note_row(segments[idx].beat)
    };
    let rel = row - start_row;
    if rel < 0 {
        return None;
    }
    let frame = (rel / step).rem_euclid(sig.numerator.max(1) as i32) as u32;
    let measure_index = (frame == 0)
        .then(|| measure_index_before(segments, idx) + (rel / measure_rows(sig).max(1)) as i64);
    Some(EditBeatBarInfo {
        frame,
        measure_index,
    })
}

fn gcd(mut a: i32, mut b: i32) -> i32 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a.max(1)
}

pub fn edit_bar_candidate_step_rows(segments: &[TimeSignatureSegment]) -> i32 {
    let mut out = bar_step_rows(default_time_signature());
    if segments.is_empty() {
        return out;
    }
    for sig in segments.iter().copied() {
        out = gcd(out, bar_step_rows(sig));
    }
    out.max(1)
}

pub fn edit_bar_scroll_speed(speed: ScrollSpeedSetting, current_bpm: f32, music_rate: f32) -> f32 {
    let base = speed.pixels_per_second(current_bpm, current_bpm.max(1.0), music_rate)
        / ScrollSpeedSetting::ARROW_SPACING;
    if music_rate.is_finite() && music_rate > 0.0 {
        base / music_rate
    } else {
        base
    }
}

pub fn beat_scroll_travel(note_beat: f32, current_beat: f32, scroll_speed: f32) -> f32 {
    (note_beat - current_beat) * ScrollSpeedSetting::ARROW_SPACING * scroll_speed
}

pub fn edit_beat_scroll_travel(note_beat: f32, current_beat: f32) -> f32 {
    beat_scroll_travel(note_beat, current_beat, 1.0)
}

pub fn scaled_edit_bar_alpha(scroll_speed: f32, visible_at: f32, full_at: f32) -> f32 {
    if !scroll_speed.is_finite()
        || !visible_at.is_finite()
        || !full_at.is_finite()
        || full_at <= visible_at
    {
        return 1.0;
    }
    ((scroll_speed - visible_at) / (full_at - visible_at)).clamp(0.0, 1.0)
}
