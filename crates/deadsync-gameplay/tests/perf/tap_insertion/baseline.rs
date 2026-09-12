// Frozen from 1d8ef2f51 (0.5.1166). Helpers imported below are unchanged.
use super::*;

pub fn apply_wide_insert(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    if notes_row_col_sorted(notes) {
        apply_wide_insert_sorted(notes, timing_player, col_offset, cols);
    } else {
        apply_wide_insert_unordered(notes, timing_player, col_offset, cols);
    }
}

fn apply_wide_insert_sorted(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
) {
    let rows_per_beat = ROWS_PER_BEAT.max(1) as usize;
    let half_beat = rows_per_beat / 2;
    let even_beat_stride = rows_per_beat.saturating_mul(2);
    notes.reserve(insert_row_reserve(notes, col_offset, cols));

    let mut active_ends = [None; MAX_COLS];
    let mut cursor = 0usize;
    while cursor < notes.len() {
        let row = notes[cursor].row_index;
        let row_start = cursor;
        cursor += 1;
        while cursor < notes.len() && notes[cursor].row_index == row {
            cursor += 1;
        }
        let summary = tap_insert_row_slice(&notes[row_start..cursor], col_offset, cols);
        let held = active_hold_mask(&active_ends, row, cols) & !summary.nonempty;
        if summary.nonempty != 0
            && row.is_multiple_of(even_beat_stride)
            && held == 0
            && summary.taps.count_ones() == 1
            && !sorted_player_range_has_note(
                notes,
                row.saturating_sub(half_beat).saturating_add(1),
                row.saturating_add(half_beat),
                Some(row),
                col_offset,
                cols,
            )
        {
            let orig_track = summary.taps.trailing_zeros() as usize;
            let mut add_track = wide_add_track(orig_track, row, rows_per_beat, cols);
            if summary.nonfake & (1u64 << add_track) != 0 {
                add_track = (add_track + 1) % cols;
            }
            let _ = set_added_tap_note_sorted(
                notes,
                timing_player,
                row,
                col_offset.saturating_add(add_track),
            );
        }

        let final_start = notes.partition_point(|note| note.row_index < row);
        cursor = notes.partition_point(|note| note.row_index <= row);
        update_active_holds(
            &notes[final_start..cursor],
            col_offset,
            cols,
            &mut active_ends,
        );
    }
}

pub fn apply_stomp_insert(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    if notes_row_col_sorted(notes) {
        apply_stomp_insert_sorted(notes, timing_player, col_offset, cols);
    } else {
        apply_stomp_insert_unordered(notes, timing_player, col_offset, cols);
    }
}

fn apply_stomp_insert_sorted(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
) {
    let half_beat = (ROWS_PER_BEAT.max(1) as usize) / 2;
    notes.reserve(insert_row_reserve(notes, col_offset, cols));

    let mut active_ends = [None; MAX_COLS];
    let mut cursor = 0usize;
    while cursor < notes.len() {
        let row = notes[cursor].row_index;
        let row_start = cursor;
        cursor += 1;
        while cursor < notes.len() && notes[cursor].row_index == row {
            cursor += 1;
        }
        let summary = tap_insert_row_slice(&notes[row_start..cursor], col_offset, cols);
        let held = active_hold_mask(&active_ends, row, cols) & !summary.nonempty;
        if summary.nonempty != 0
            && summary.taps.count_ones() == 1
            && !sorted_player_range_has_tap(
                notes,
                row.saturating_sub(half_beat).saturating_add(1),
                row.saturating_add(half_beat).saturating_sub(1),
                row,
                col_offset,
                cols,
            )
            && held == 0
        {
            let track = summary.taps.trailing_zeros() as usize;
            let _ = set_added_tap_note_sorted(
                notes,
                timing_player,
                row,
                col_offset.saturating_add(stomp_mirror_track(track, cols)),
            );
        }

        let final_start = notes.partition_point(|note| note.row_index < row);
        cursor = notes.partition_point(|note| note.row_index <= row);
        update_active_holds(
            &notes[final_start..cursor],
            col_offset,
            cols,
            &mut active_ends,
        );
    }
}

pub fn apply_echo_insert(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    if notes_row_col_sorted(notes) {
        apply_echo_insert_sorted(notes, timing_player, col_offset, cols);
    } else {
        apply_echo_insert_unordered(notes, timing_player, col_offset, cols);
    }
}

fn apply_echo_insert_sorted(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
) {
    let rows_per_interval = (ROWS_PER_BEAT.max(1) as usize) / 2;
    if rows_per_interval == 0 {
        return;
    }
    let max_row = notes
        .iter()
        .rev()
        .find_map(|note| {
            local_player_col(note.column, col_offset, cols).map(|_| note.row_index)
        })
        .unwrap_or(0);
    let end_row = max_row.saturating_add(1);
    let grid_rows = (end_row / rows_per_interval).saturating_add(1);
    notes.reserve(grid_rows.min(notes.len().max(1)));

    let mut active_ends = [None; MAX_COLS];
    let mut note_cursor = 0usize;
    let mut echo_track: Option<usize> = None;
    let mut row = 0usize;
    while row <= end_row {
        while note_cursor < notes.len() && notes[note_cursor].row_index <= row {
            let note_row = notes[note_cursor].row_index;
            let start = note_cursor;
            note_cursor += 1;
            while note_cursor < notes.len() && notes[note_cursor].row_index == note_row {
                note_cursor += 1;
            }
            update_active_holds(
                &notes[start..note_cursor],
                col_offset,
                cols,
                &mut active_ends,
            );
        }

        let summary = tap_insert_row(notes, row, col_offset, cols);
        if summary.nonempty == 0 {
            row = row.saturating_add(rows_per_interval);
            continue;
        }
        if summary.taps != 0 {
            echo_track = Some(summary.taps.trailing_zeros() as usize);
        }
        let Some(track) = echo_track else {
            row = row.saturating_add(rows_per_interval);
            continue;
        };
        let row_window_end = row.saturating_add(rows_per_interval.saturating_mul(2));
        if sorted_player_range_has_note(
            notes,
            row.saturating_add(1),
            row_window_end.saturating_sub(1),
            None,
            col_offset,
            cols,
        ) {
            row = row.saturating_add(rows_per_interval);
            continue;
        }

        let row_echo = row.saturating_add(rows_per_interval);
        let held = active_hold_mask(&active_ends, row_echo, cols);
        if held.count_ones() >= 2 || held & (1u64 << track) != 0 {
            row = row.saturating_add(rows_per_interval);
            continue;
        }
        let _ = set_added_tap_note_sorted(notes, timing_player, row_echo, col_offset + track);
        row = row.saturating_add(rows_per_interval);
    }
}
