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
    if segments.is_empty() {
        default_time_signature()
    } else {
        valid_sig(segments[index])
    }
}

fn sig_count(segments: &[TimeSignatureSegment]) -> usize {
    segments.len().max(1)
}

fn bar_step_rows(sig: TimeSignatureSegment) -> i32 {
    (beat_to_note_row(valid_sig(sig).denominator as f32 / 4.0) / 4).max(1)
}

fn measure_frequency(sig: TimeSignatureSegment) -> i32 {
    valid_sig(sig).numerator.saturating_mul(4).max(1)
}

fn bars_in_segment(start_row: i32, end_row: i32, sig: TimeSignatureSegment) -> i64 {
    if end_row <= start_row {
        return 0;
    }
    let step = i64::from(bar_step_rows(sig));
    let freq = i64::from(measure_frequency(sig));
    let bars = (i64::from(end_row) - i64::from(start_row) - 1) / step + 1;
    (bars - 1) / freq + 1
}

fn measure_index_before(segments: &[TimeSignatureSegment], index: usize) -> i64 {
    let mut total = 0;
    for i in 0..index {
        let sig = sig_at(segments, i);
        let next_sig = sig_at(segments, i + 1);
        total += bars_in_segment(
            beat_to_note_row(sig.beat),
            beat_to_note_row(next_sig.beat),
            sig,
        );
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
    if row < 0 {
        return None;
    }

    let idx = sig_index_at_row(segments, row);
    let sig = sig_at(segments, idx);
    let start_row = beat_to_note_row(sig.beat);
    if row < start_row {
        return None;
    }

    let rel = row - start_row;
    let step = bar_step_rows(sig);
    if step <= 0 || rel % step != 0 {
        return None;
    }

    let bars_drawn = rel / step;
    let measure_frequency = measure_frequency(sig);
    let is_measure = bars_drawn % measure_frequency == 0;
    let frame = if is_measure {
        0
    } else if bars_drawn % 4 == 0 {
        1
    } else if bars_drawn % 2 == 0 {
        2
    } else {
        3
    };
    let measure_index = is_measure
        .then(|| measure_index_before(segments, idx) + i64::from(bars_drawn / measure_frequency));
    Some(EditBeatBarInfo {
        frame,
        measure_index,
    })
}

fn gcd(a: i32, b: i32) -> i32 {
    let mut a = i64::from(a).abs();
    let mut b = i64::from(b).abs();
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a.clamp(1, i64::from(i32::MAX)) as i32
}

pub fn edit_bar_candidate_step_rows(segments: &[TimeSignatureSegment]) -> i32 {
    let mut out = bar_step_rows(sig_at(segments, 0));
    for i in 0..sig_count(segments) {
        let sig = sig_at(segments, i);
        out = gcd(out, bar_step_rows(sig));
        out = gcd(out, beat_to_note_row(sig.beat));
    }
    out.max(1)
}

pub fn edit_bar_scroll_speed(speed: ScrollSpeedSetting, current_bpm: f32, music_rate: f32) -> f32 {
    match speed {
        ScrollSpeedSetting::XMod(multiplier) => multiplier,
        ScrollSpeedSetting::MMod(_) => speed.beat_multiplier(current_bpm, music_rate),
        ScrollSpeedSetting::CMod(_) => 4.0,
    }
    .max(0.0)
}

pub fn beat_scroll_travel(note_beat: f32, current_beat: f32, scroll_speed: f32) -> f32 {
    (note_beat - current_beat) * ScrollSpeedSetting::ARROW_SPACING * scroll_speed
}

pub fn edit_beat_scroll_travel(note_beat: f32, current_beat: f32) -> f32 {
    beat_scroll_travel(note_beat, current_beat, 1.0)
}

pub fn scaled_edit_bar_alpha(scroll_speed: f32, visible_at: f32, full_at: f32) -> f32 {
    ((scroll_speed - visible_at) / (full_at - visible_at)).clamp(0.0, 1.0)
}
