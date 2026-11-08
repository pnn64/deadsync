use super::{TimeSignatureSegment, beat_to_note_row, default_time_signature};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditBeatBarInfo {
    pub frame: u32,
    pub measure_index: Option<i64>,
}

pub(super) const fn valid_sig(sig: TimeSignatureSegment) -> TimeSignatureSegment {
    if sig.numerator > 0 && sig.denominator > 0 {
        sig
    } else {
        default_time_signature()
    }
}

pub(super) const fn sig_at(
    segments: &[TimeSignatureSegment],
    index: usize,
) -> TimeSignatureSegment {
    if segments.is_empty() {
        default_time_signature()
    } else {
        valid_sig(segments[index])
    }
}

pub(super) fn sig_count(segments: &[TimeSignatureSegment]) -> usize {
    segments.len().max(1)
}

pub(super) fn bar_step_rows(sig: TimeSignatureSegment) -> i32 {
    (beat_to_note_row(valid_sig(sig).denominator as f32 / 4.0) / 4).max(1)
}

pub(super) fn measure_frequency(sig: TimeSignatureSegment) -> i32 {
    valid_sig(sig).numerator.saturating_mul(4).max(1)
}

pub(super) fn bars_in_segment(start_row: i32, end_row: i32, sig: TimeSignatureSegment) -> i64 {
    if end_row <= start_row {
        return 0;
    }
    let step = i64::from(bar_step_rows(sig));
    let freq = i64::from(measure_frequency(sig));
    let bars = (i64::from(end_row) - i64::from(start_row) - 1) / step + 1;
    (bars - 1) / freq + 1
}

pub(super) fn measure_index_before(segments: &[TimeSignatureSegment], index: usize) -> i64 {
    if index == 0 {
        return 0;
    }
    let mut total = 0;
    // Adjacent segments share their boundary row and validated signature.
    let mut sig = sig_at(segments, 0);
    let mut start_row = beat_to_note_row(sig.beat);
    for i in 0..index {
        let next_sig = sig_at(segments, i + 1);
        let end_row = beat_to_note_row(next_sig.beat);
        total += bars_in_segment(start_row, end_row, sig);
        sig = next_sig;
        start_row = end_row;
    }
    total
}

/// Stack-only cursor for a monotonic or seeking sequence of edit-bar rows.
#[derive(Clone, Copy)]
pub(super) struct EditBeatBarSegmentCache {
    start_row: i32,
    step_rows: i32,
    measure_frequency: i32,
}

pub(super) fn edit_beat_bar_segment_cache(
    segments: &[TimeSignatureSegment],
    index: usize,
) -> EditBeatBarSegmentCache {
    let sig = sig_at(segments, index);
    EditBeatBarSegmentCache {
        start_row: beat_to_note_row(sig.beat),
        step_rows: bar_step_rows(sig),
        measure_frequency: measure_frequency(sig),
    }
}

pub(super) fn next_edit_beat_bar_start_row(segments: &[TimeSignatureSegment], index: usize) -> i32 {
    segments
        .get(index + 1)
        .map_or(i32::MAX, |sig| beat_to_note_row(sig.beat))
}

#[derive(Clone, Copy)]
pub struct EditBeatBarCursor<'a> {
    segments: &'a [TimeSignatureSegment],
    index: usize,
    measure_index_before: i64,
    segment: EditBeatBarSegmentCache,
    next_start_row: i32,
}

impl<'a> EditBeatBarCursor<'a> {
    #[must_use]
    pub fn new(row: i32, segments: &'a [TimeSignatureSegment]) -> Self {
        let index = sig_index_at_row(segments, row);
        Self {
            segments,
            index,
            measure_index_before: measure_index_before(segments, index),
            segment: edit_beat_bar_segment_cache(segments, index),
            next_start_row: next_edit_beat_bar_start_row(segments, index),
        }
    }

    fn advance_to(&mut self, row: i32) {
        while self.index + 1 < self.segments.len() && row >= self.next_start_row {
            self.measure_index_before += bars_in_segment(
                self.segment.start_row,
                self.next_start_row,
                sig_at(self.segments, self.index),
            );
            self.index += 1;
            self.segment = edit_beat_bar_segment_cache(self.segments, self.index);
            self.next_start_row = next_edit_beat_bar_start_row(self.segments, self.index);
        }
        while self.index > 0 && row < self.segment.start_row {
            let previous = self.index - 1;
            let sig = sig_at(self.segments, previous);
            let previous_segment = edit_beat_bar_segment_cache(self.segments, previous);
            self.measure_index_before -=
                bars_in_segment(previous_segment.start_row, self.segment.start_row, sig);
            self.index = previous;
            self.segment = previous_segment;
            self.next_start_row = next_edit_beat_bar_start_row(self.segments, self.index);
        }
    }

    pub fn info_for_row(&mut self, row: i32) -> Option<EditBeatBarInfo> {
        if row < 0 {
            return None;
        }
        self.advance_to(row);
        edit_beat_bar_info_cached(row, self.segment, self.measure_index_before)
    }
}

pub(super) fn sig_index_at_row(segments: &[TimeSignatureSegment], row: i32) -> usize {
    if segments.is_empty() {
        return 0;
    }
    segments
        .partition_point(|sig| row >= beat_to_note_row(sig.beat))
        .saturating_sub(1)
}

pub(super) fn edit_beat_bar_info_cached(
    row: i32,
    segment: EditBeatBarSegmentCache,
    measure_index_before: i64,
) -> Option<EditBeatBarInfo> {
    if row < segment.start_row {
        return None;
    }

    let rel = row - segment.start_row;
    let step = segment.step_rows;
    if step <= 0 || rel % step != 0 {
        return None;
    }

    let bars_drawn = rel / step;
    let measure_frequency = segment.measure_frequency;
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
    let measure_index =
        is_measure.then(|| measure_index_before + i64::from(bars_drawn / measure_frequency));
    Some(EditBeatBarInfo {
        frame,
        measure_index,
    })
}

pub(super) fn gcd(a: i32, b: i32) -> i32 {
    let mut a = i64::from(a).abs();
    let mut b = i64::from(b).abs();
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a.clamp(1, i64::from(i32::MAX)) as i32
}

pub(crate) fn edit_bar_candidate_step_rows(segments: &[TimeSignatureSegment]) -> i32 {
    let mut out = bar_step_rows(sig_at(segments, 0));
    for i in 0..sig_count(segments) {
        let sig = sig_at(segments, i);
        out = gcd(out, bar_step_rows(sig));
        out = gcd(out, beat_to_note_row(sig.beat));
    }
    out.max(1)
}

pub(super) fn cursor_state(cursor: &EditBeatBarCursor<'_>) -> (usize, i64, i32, i32, i32, i32) {
    (
        cursor.index,
        cursor.measure_index_before,
        cursor.segment.start_row,
        cursor.segment.step_rows,
        cursor.segment.measure_frequency,
        cursor.next_start_row,
    )
}
