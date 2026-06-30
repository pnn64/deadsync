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
    if high < 0 {
        return Some((0, 0));
    }
    let low = low.max(0);
    Some((
        indices.partition_point(|&note_index| note_itg_row(&notes[note_index]) < low),
        indices.partition_point(|&note_index| note_itg_row(&notes[note_index]) <= high),
    ))
}

pub fn lane_hold_window_bounds_by_note_row(
    notes: &[Note],
    indices: &[usize],
    range: Option<(i32, i32)>,
) -> Option<(usize, usize)> {
    let (low, _) = range?;
    let (mut start, end) = lane_window_bounds_by_note_row(notes, indices, range)?;
    let low = low.max(0);
    while start > 0 {
        let prev_note_index = indices[start - 1];
        let prev_end_row = notes[prev_note_index]
            .hold
            .as_ref()
            .map_or(note_itg_row(&notes[prev_note_index]), |hold| {
                beat_to_note_row(hold.end_beat)
            });
        if prev_end_row < low {
            break;
        }
        start -= 1;
    }
    Some((start, end))
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
    let Some((start, end)) = lane_window_bounds_by_note_row(notes, indices, Some((low, high)))
    else {
        return;
    };
    for &i in &indices[start..end] {
        f(i);
    }
}

pub fn for_each_visible_hold_index<F: FnMut(usize)>(
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
    let Some((start, end)) = lane_hold_window_bounds_by_note_row(notes, indices, Some((low, high)))
    else {
        return;
    };
    for &i in &indices[start..end] {
        f(i);
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
    high >= 0 && end >= low.max(0) && start <= high
}

fn note_count_at(stats: &[NoteCountStat], beat: f32) -> NoteCountStat {
    let ix = stats
        .partition_point(|stat| stat.beat <= beat)
        .saturating_sub(1);
    stats.get(ix).copied().unwrap_or(NoteCountStat {
        beat: 0.0,
        notes_lower: 0,
        notes_upper: 0,
    })
}

fn note_count_range(stats: &[NoteCountStat], low: f32, high: f32) -> usize {
    let low = note_count_at(stats, low);
    let high = note_count_at(stats, high);
    high.notes_upper.saturating_sub(low.notes_lower)
}

pub fn find_first_displayed_beat<F: FnMut(f32) -> f32>(
    current_beat: f32,
    draw_distance: f32,
    stats: &[NoteCountStat],
    mut y_for_beat: F,
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance.is_finite() {
        return None;
    }
    let mut high = current_beat.max(0.0);
    let has_cache = !stats.is_empty();
    let mut low = if has_cache { 0.0 } else { high - 4.0 };
    let mut first = low;
    for _ in 0..24 {
        let mid = (low + high) * 0.5;
        if y_for_beat(mid) < -draw_distance
            || (has_cache && note_count_range(stats, mid, current_beat) > MAX_NOTES_AFTER)
        {
            first = mid;
            low = mid;
        } else {
            high = mid;
        }
    }
    Some(first)
}

pub fn find_last_displayed_beat<F: FnMut(f32) -> (f32, bool)>(
    current_beat: f32,
    draw_distance: f32,
    displayed_speed_percent: f32,
    boomerang: bool,
    mut y_for_beat: F,
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance.is_finite() {
        return None;
    }
    let mut search_distance = 10.0;
    let mut last = current_beat + search_distance;
    for _ in 0..20 {
        let (y_offset, before_peak) = y_for_beat(last);
        if boomerang && !before_peak {
            last += search_distance;
        } else if y_offset > draw_distance {
            last -= search_distance;
        } else {
            last += search_distance;
        }
        search_distance *= 0.5;
    }
    if displayed_speed_percent < 0.75 {
        last = last.min(current_beat + 16.0);
    }
    Some(last)
}

pub const fn mine_hides_after_resolution(mine_result: Option<MineResult>) -> bool {
    mine_result.is_some()
}

use crate::style::MAX_NOTES_AFTER;
