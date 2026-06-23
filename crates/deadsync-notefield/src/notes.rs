use deadsync_core::timing::beat_to_note_row;
use deadsync_rules::note::{MineResult, Note, NoteCountStat};

pub fn note_itg_row(note: &Note) -> i32 {
    beat_to_note_row(note.beat)
}

pub fn lane_window_bounds_by_note_row(
    notes: &[Note],
    indices: &[usize],
    range: Option<(i32, i32)>,
) -> Option<(usize, usize)> {
    let (low, high) = range?;
    let start = indices
        .iter()
        .position(|&i| note_itg_row(&notes[i]) >= low)
        .unwrap_or(indices.len());
    let end = indices
        .iter()
        .rposition(|&i| note_itg_row(&notes[i]) <= high)
        .map(|i| i + 1)
        .unwrap_or(start);
    Some((start, end.max(start)))
}

pub fn lane_hold_window_bounds_by_note_row(
    notes: &[Note],
    indices: &[usize],
    range: Option<(i32, i32)>,
) -> Option<(usize, usize)> {
    let mut first = None;
    let mut last = None;
    for (pos, &ix) in indices.iter().enumerate() {
        if hold_overlaps_visible_window(ix, notes, range) {
            first.get_or_insert(pos);
            last = Some(pos + 1);
        }
    }
    Some((first.unwrap_or(0), last.unwrap_or(0)))
}

pub fn for_each_visible_note_index<F: FnMut(usize)>(
    indices: &[usize],
    notes: &[Note],
    range: Option<(i32, i32)>,
    mut f: F,
) {
    let Some((low, high)) = range else {
        for &i in indices {
            f(i);
        }
        return;
    };
    for &i in indices {
        let row = note_itg_row(&notes[i]);
        if row >= low && row <= high {
            f(i);
        }
    }
}

pub fn for_each_visible_hold_index<F: FnMut(usize)>(
    indices: &[usize],
    notes: &[Note],
    range: Option<(i32, i32)>,
    mut f: F,
) {
    for &i in indices {
        if hold_overlaps_visible_window(i, notes, range) {
            f(i);
        }
    }
}

pub fn hold_overlaps_visible_window(
    note_index: usize,
    notes: &[Note],
    range: Option<(i32, i32)>,
) -> bool {
    let Some(note) = notes.get(note_index) else {
        return false;
    };
    let Some((low, high)) = range else {
        return true;
    };
    let start = note_itg_row(note);
    let end = note
        .hold
        .as_ref()
        .map(|h| beat_to_note_row(h.end_beat))
        .unwrap_or(start);
    start <= high && end >= low
}

fn note_count_at(stats: &[NoteCountStat], beat: f32) -> NoteCountStat {
    stats
        .iter()
        .rev()
        .find(|s| s.beat <= beat)
        .copied()
        .unwrap_or(NoteCountStat {
            beat: 0.0,
            notes_lower: 0,
            notes_upper: 0,
        })
}

pub fn find_first_displayed_beat<F: Fn(f32) -> f32>(
    current_beat: f32,
    draw_distance: f32,
    stats: &[NoteCountStat],
    y_for_beat: F,
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance.is_finite() {
        return None;
    }
    if !stats.is_empty() {
        let total = note_count_at(stats, current_beat).notes_upper;
        let cutoff = total.saturating_sub(MAX_NOTES_AFTER);
        if let Some(stat) = stats.iter().find(|s| s.notes_lower >= cutoff) {
            return Some(stat.beat);
        }
    }
    let estimate = current_beat - draw_distance / 30.0;
    let mut beat = estimate;
    while beat <= current_beat {
        if y_for_beat(beat).abs() <= draw_distance {
            return Some(beat);
        }
        beat += 0.001;
    }
    Some(estimate)
}

pub fn find_last_displayed_beat<F: Fn(f32) -> (f32, bool)>(
    current_beat: f32,
    draw_distance: f32,
    scroll_speed: f32,
    boomerang: bool,
    y_for_beat: F,
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance.is_finite() {
        return None;
    }
    let max_lookahead = if scroll_speed < 1.0 { 16.0 } else { 64.0 };
    if boomerang {
        return Some(current_beat + max_lookahead);
    }
    let mut beat = current_beat;
    let step = 0.001_f32.max(1.0 / 192.0);
    while beat < current_beat + max_lookahead {
        let (y, before_peak) = y_for_beat(beat);
        if y.abs() >= draw_distance && (!boomerang || !before_peak) {
            return Some(beat);
        }
        beat += step;
    }
    Some(current_beat + max_lookahead)
}

pub const fn mine_hides_after_resolution(mine_result: Option<MineResult>) -> bool {
    mine_result.is_some()
}

use crate::style::MAX_NOTES_AFTER;
