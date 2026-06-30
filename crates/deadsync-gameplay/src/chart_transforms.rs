pub fn enforce_max_simultaneous_notes(
    notes: &mut Vec<Note>,
    max_simultaneous: usize,
    col_offset: usize,
    cols: usize,
) {
    if notes.is_empty() || cols == 0 || cols > MAX_COLS {
        return;
    }
    debug_assert!(notes_row_sorted(notes));

    let mut remove_idx = vec![false; notes.len()];
    let mut active_hold_ends: [Option<usize>; MAX_COLS] = [None; MAX_COLS];
    let mut row_candidates = Vec::<(usize, usize)>::with_capacity(MAX_COLS);

    let mut row_start = 0usize;
    while row_start < notes.len() {
        let row = notes[row_start].row_index;
        let mut row_end = row_start + 1;
        while row_end < notes.len() && notes[row_end].row_index == row {
            row_end += 1;
        }

        for held in active_hold_ends.iter_mut().take(cols) {
            if held.is_some_and(|end| end < row) {
                *held = None;
            }
        }

        let active_holds = active_hold_ends
            .iter()
            .take(cols)
            .filter(|end| end.is_some())
            .count();

        row_candidates.clear();
        for (offset, note) in notes[row_start..row_end].iter().enumerate() {
            let idx = row_start + offset;
            if note.column < col_offset {
                continue;
            }
            let local_col = note.column - col_offset;
            if local_col >= cols || !note_counts_for_simultaneous_limit(note) {
                continue;
            }
            row_candidates.push((local_col, idx));
        }

        if row_candidates.is_empty() {
            row_start = row_end;
            continue;
        }

        row_candidates.sort_unstable_by_key(|(local_col, _)| *local_col);
        let mut tracks_to_remove = active_holds
            .saturating_add(row_candidates.len())
            .saturating_sub(max_simultaneous);

        if tracks_to_remove > 0 {
            for &(_, idx) in &row_candidates {
                if tracks_to_remove == 0 {
                    break;
                }
                remove_idx[idx] = true;
                tracks_to_remove -= 1;
            }
        }

        for &(local_col, idx) in &row_candidates {
            if remove_idx[idx] || !matches!(notes[idx].note_type, NoteType::Hold | NoteType::Roll) {
                continue;
            }
            let end_row = notes[idx]
                .hold
                .as_ref()
                .map(|hold| hold.end_row_index)
                .unwrap_or(row);
            if active_hold_ends[local_col].is_none_or(|current| current < end_row) {
                active_hold_ends[local_col] = Some(end_row);
            }
        }

        row_start = row_end;
    }

    if remove_idx.iter().all(|remove| !*remove) {
        return;
    }

    let mut idx = 0usize;
    notes.retain(|_| {
        let keep = !remove_idx[idx];
        idx += 1;
        keep
    });
}

#[inline(always)]
pub fn local_player_col(column: usize, col_offset: usize, cols: usize) -> Option<usize> {
    if column < col_offset {
        return None;
    }
    let local = column - col_offset;
    (local < cols).then_some(local)
}

#[inline(always)]
pub const fn player_index_for_column(
    num_players: usize,
    cols_per_player: usize,
    column: usize,
) -> usize {
    if num_players <= 1 || cols_per_player == 0 {
        return 0;
    }
    let player = column / cols_per_player;
    let last_player = num_players.saturating_sub(1);
    if player > last_player {
        last_player
    } else {
        player
    }
}

#[inline(always)]
pub const fn player_column_range(cols_per_player: usize, player: usize) -> (usize, usize) {
    let start = player * cols_per_player;
    (start, start + cols_per_player)
}

#[inline(always)]
pub fn player_note_range_for_ranges(
    note_ranges: &[(usize, usize)],
    num_players: usize,
    player: usize,
) -> (usize, usize) {
    if player >= num_players {
        return (0, 0);
    }
    note_ranges.get(player).copied().unwrap_or((0, 0))
}

#[inline(always)]
pub const fn local_column_for_field(cols_per_player: usize, column: usize) -> usize {
    if cols_per_player == 0 {
        column
    } else {
        column % cols_per_player
    }
}

pub fn sort_player_notes(notes: &mut [Note]) {
    notes.sort_unstable_by_key(|note| (note.row_index, note.column));
}

pub fn player_rows(notes: &[Note], col_offset: usize, cols: usize) -> Vec<usize> {
    let mut rows = Vec::with_capacity(notes.len());
    for note in notes {
        if local_player_col(note.column, col_offset, cols).is_some() {
            rows.push(note.row_index);
        }
    }
    rows.sort_unstable();
    rows.dedup();
    rows
}

pub fn count_nonempty_tracks_at_row(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> usize {
    let mut seen = [false; MAX_COLS];
    for note in notes {
        if note.row_index != row {
            continue;
        }
        if let Some(local) = local_player_col(note.column, col_offset, cols) {
            seen[local] = true;
        }
    }
    seen[..cols].iter().filter(|&&on| on).count()
}

pub fn count_tap_or_hold_tracks_at_row(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> usize {
    let mut seen = [false; MAX_COLS];
    for note in notes {
        if note.row_index != row {
            continue;
        }
        if !matches!(
            note.note_type,
            NoteType::Tap | NoteType::Lift | NoteType::Hold | NoteType::Roll
        ) {
            continue;
        }
        if let Some(local) = local_player_col(note.column, col_offset, cols) {
            seen[local] = true;
        }
    }
    seen[..cols].iter().filter(|&&on| on).count()
}

pub fn count_tap_tracks_at_row(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> usize {
    let mut seen = [false; MAX_COLS];
    for note in notes {
        if note.row_index != row
            || !matches!(note.note_type, NoteType::Tap | NoteType::Lift)
            || note.is_fake
        {
            continue;
        }
        if let Some(local) = local_player_col(note.column, col_offset, cols) {
            seen[local] = true;
        }
    }
    seen[..cols].iter().filter(|&&on| on).count()
}

pub fn first_nonempty_track_at_row(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> Option<usize> {
    let mut first: Option<usize> = None;
    for note in notes {
        if note.row_index != row {
            continue;
        }
        let Some(local) = local_player_col(note.column, col_offset, cols) else {
            continue;
        };
        first = Some(match first {
            Some(curr) => curr.min(local),
            None => local,
        });
    }
    first
}

pub fn first_tap_track_at_row(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> Option<usize> {
    let mut first: Option<usize> = None;
    for note in notes {
        if note.row_index != row
            || !matches!(note.note_type, NoteType::Tap | NoteType::Lift)
            || note.is_fake
        {
            continue;
        }
        let Some(local) = local_player_col(note.column, col_offset, cols) else {
            continue;
        };
        first = Some(match first {
            Some(curr) => curr.min(local),
            None => local,
        });
    }
    first
}

pub fn cell_has_any_note(notes: &[Note], row: usize, column: usize) -> bool {
    notes
        .iter()
        .any(|note| note.row_index == row && note.column == column)
}

pub fn cell_has_nonfake_note(notes: &[Note], row: usize, column: usize) -> bool {
    notes
        .iter()
        .any(|note| note.row_index == row && note.column == column && !note.is_fake)
}

pub fn remove_cell_notes(notes: &mut Vec<Note>, row: usize, column: usize) {
    notes.retain(|note| !(note.row_index == row && note.column == column));
}

pub fn is_hold_body_at_row(notes: &[Note], row: usize, column: usize) -> bool {
    let mut latest: Option<&Note> = None;
    for note in notes {
        if note.column != column || note.row_index > row {
            continue;
        }
        if latest.is_none_or(|curr| note.row_index >= curr.row_index) {
            latest = Some(note);
        }
    }
    let Some(note) = latest else {
        return false;
    };
    if !matches!(note.note_type, NoteType::Hold | NoteType::Roll) || note.row_index >= row {
        return false;
    }
    note.hold
        .as_ref()
        .is_some_and(|hold| hold.end_row_index >= row)
}

pub fn count_held_tracks_at_row(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> usize {
    (0..cols)
        .filter(|local| is_hold_body_at_row(notes, row, col_offset + *local))
        .count()
}

pub fn set_added_tap_note(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    row: usize,
    column: usize,
) -> bool {
    let Some(beat) = timing_player.get_beat_for_row(row) else {
        return false;
    };
    remove_cell_notes(notes, row, column);
    let quantization_idx = quantization_index_from_beat(beat);
    notes.push(Note {
        beat,
        quantization_idx,
        column,
        note_type: NoteType::Tap,
        row_index: row,
        result: None,
        early_result: None,
        hold: None,
        mine_result: None,
        is_fake: false,
        can_be_judged: timing_player.is_judgable_at_beat(beat),
    });
    true
}

pub fn set_added_mine_note(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    row: usize,
    column: usize,
) -> bool {
    let Some(beat) = timing_player.get_beat_for_row(row) else {
        return false;
    };
    remove_cell_notes(notes, row, column);
    let quantization_idx = quantization_index_from_beat(beat);
    notes.push(Note {
        beat,
        quantization_idx,
        column,
        note_type: NoteType::Mine,
        row_index: row,
        result: None,
        early_result: None,
        hold: None,
        mine_result: None,
        is_fake: false,
        can_be_judged: timing_player.is_judgable_at_beat(beat),
    });
    true
}

pub fn convert_tap_row_to_mines(notes: &mut [Note], row: usize) {
    for note in notes.iter_mut() {
        if note.row_index == row && note.note_type == NoteType::Tap {
            note.note_type = NoteType::Mine;
            note.hold = None;
            note.mine_result = None;
        }
    }
}

pub fn track_range_has_any_note(
    notes: &[Note],
    column: usize,
    start_row: usize,
    end_row: usize,
) -> bool {
    notes.iter().any(|note| {
        note.column == column && note.row_index >= start_row && note.row_index <= end_row
    })
}

pub fn apply_mines_insert(
    notes: &mut Vec<Note>,
    context_notes: &[Note],
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    start_row: usize,
    end_row: usize,
) {
    if cols == 0 || cols > MAX_COLS || end_row < start_row {
        return;
    }

    let mut row_count = 0usize;
    let mut place_every_rows = 6usize;
    for row in player_rows(notes, col_offset, cols) {
        if row < start_row || row > end_row {
            continue;
        }
        row_count = row_count.saturating_add(1);
        if row_count < place_every_rows {
            continue;
        }
        convert_tap_row_to_mines(notes, row);
        row_count = 0;
        place_every_rows = if place_every_rows == 6 { 7 } else { 6 };
    }

    let half_beat_rows = (ROWS_PER_BEAT.max(1) / 2) as usize;
    let hold_heads: Vec<(usize, usize)> = notes
        .iter()
        .filter_map(|note| {
            matches!(note.note_type, NoteType::Hold | NoteType::Roll)
                .then_some((note.column, note.hold.as_ref()?.end_row_index))
        })
        .collect();
    let mut full_context = Vec::with_capacity(context_notes.len() + notes.len() + hold_heads.len());
    full_context.extend_from_slice(context_notes);
    full_context.extend(notes.iter().cloned());
    for (column, end_row_index) in hold_heads {
        let mine_row = end_row_index.saturating_add(half_beat_rows);
        if mine_row < start_row || mine_row > end_row {
            continue;
        }
        let range_start = mine_row.saturating_sub(half_beat_rows).saturating_add(1);
        let range_end = mine_row.saturating_add(half_beat_rows).saturating_sub(1);
        if track_range_has_any_note(&full_context, column, range_start, range_end) {
            continue;
        }
        if !set_added_mine_note(notes, timing_player, mine_row, column) {
            continue;
        }
        convert_tap_row_to_mines(notes, mine_row);
        if let Some(note) = notes
            .iter()
            .find(|note| note.column == column && note.row_index == mine_row)
        {
            full_context.push(note.clone());
        }
    }
}

#[inline(always)]
pub fn stomp_mirror_track(local_track: usize, cols: usize) -> usize {
    match cols {
        4 => [3, 2, 1, 0][local_track],
        8 => [1, 0, 3, 2, 5, 4, 7, 6][local_track],
        _ => cols.saturating_sub(1).saturating_sub(local_track),
    }
}

pub fn apply_insert_intelligent_taps(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    window_size_rows: usize,
    insert_offset_rows: usize,
    window_stride_rows: usize,
    skippy_mode: bool,
) {
    if cols == 0 || cols > MAX_COLS || insert_offset_rows > window_size_rows {
        return;
    }
    let rows = player_rows(notes, col_offset, cols);
    let require_begin = !skippy_mode;
    let require_end = true;
    for &row in &rows {
        if row % window_stride_rows != 0 {
            continue;
        }
        let row_earlier = row;
        let row_later = row_earlier.saturating_add(window_size_rows);
        let row_to_add = row_earlier.saturating_add(insert_offset_rows);

        if require_begin
            && (count_nonempty_tracks_at_row(notes, row_earlier, col_offset, cols) != 1
                || count_tap_or_hold_tracks_at_row(notes, row_earlier, col_offset, cols) != 1)
        {
            continue;
        }
        if require_end
            && (count_nonempty_tracks_at_row(notes, row_later, col_offset, cols) != 1
                || count_tap_or_hold_tracks_at_row(notes, row_later, col_offset, cols) != 1)
        {
            continue;
        }

        let mut note_in_middle = false;
        for local in 0..cols {
            if is_hold_body_at_row(notes, row_earlier.saturating_add(1), col_offset + local) {
                note_in_middle = true;
                break;
            }
        }
        if !note_in_middle {
            for note in notes.iter() {
                if local_player_col(note.column, col_offset, cols).is_none() {
                    continue;
                }
                if note.row_index >= row_earlier.saturating_add(1)
                    && note.row_index <= row_later.saturating_sub(1)
                {
                    note_in_middle = true;
                    break;
                }
            }
        }
        if note_in_middle {
            continue;
        }

        let earlier_track = first_nonempty_track_at_row(notes, row_earlier, col_offset, cols);
        let later_track = first_nonempty_track_at_row(notes, row_later, col_offset, cols);
        let Some(later_track) = later_track else {
            continue;
        };
        let track_to_add =
            if skippy_mode && earlier_track.is_some() && earlier_track != Some(later_track) {
                earlier_track.unwrap_or(0)
            } else if let Some(earlier_track) = earlier_track {
                if earlier_track.abs_diff(later_track) >= 2 {
                    earlier_track.min(later_track).saturating_add(1)
                } else if earlier_track.min(later_track) >= 1 {
                    earlier_track.min(later_track) - 1
                } else if earlier_track.max(later_track).saturating_add(1) < cols {
                    earlier_track.max(later_track).saturating_add(1)
                } else {
                    0
                }
            } else {
                0
            };

        let _ = set_added_tap_note(
            notes,
            timing_player,
            row_to_add,
            col_offset.saturating_add(track_to_add),
        );
    }
}

pub fn apply_wide_insert(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    let rows = player_rows(notes, col_offset, cols);
    let rows_per_beat = ROWS_PER_BEAT.max(1) as usize;
    let half_beat = rows_per_beat / 2;
    let even_beat_stride = rows_per_beat.saturating_mul(2);
    for row in rows {
        if row % even_beat_stride != 0 {
            continue;
        }
        if count_held_tracks_at_row(notes, row, col_offset, cols) > 0 {
            continue;
        }
        if count_tap_tracks_at_row(notes, row, col_offset, cols) != 1 {
            continue;
        }
        let mut has_space = true;
        for note in notes.iter() {
            if local_player_col(note.column, col_offset, cols).is_none() {
                continue;
            }
            if note.row_index >= row.saturating_sub(half_beat).saturating_add(1)
                && note.row_index <= row.saturating_add(half_beat)
                && note.row_index != row
            {
                has_space = false;
                break;
            }
        }
        if !has_space {
            continue;
        }
        let Some(orig_track) = first_tap_track_at_row(notes, row, col_offset, cols) else {
            continue;
        };
        let beat_i = ((row as f32) / (rows_per_beat as f32)).round() as i32;
        let mut add_track = (orig_track as i32) + (beat_i % 5) - 2;
        add_track = add_track.clamp(0, cols.saturating_sub(1) as i32);
        if add_track as usize == orig_track {
            add_track = (add_track + 1).clamp(0, cols.saturating_sub(1) as i32);
        }
        if add_track as usize == orig_track {
            add_track = (add_track - 1).clamp(0, cols.saturating_sub(1) as i32);
        }
        let mut add_track = add_track as usize;
        if cell_has_nonfake_note(notes, row, col_offset.saturating_add(add_track)) {
            add_track = (add_track + 1) % cols;
        }
        let _ = set_added_tap_note(
            notes,
            timing_player,
            row,
            col_offset.saturating_add(add_track),
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
    let rows = player_rows(notes, col_offset, cols);
    let half_beat = (ROWS_PER_BEAT.max(1) as usize) / 2;
    for row in rows {
        if count_tap_tracks_at_row(notes, row, col_offset, cols) != 1 {
            continue;
        }
        let mut tap_in_middle = false;
        let row_begin = row.saturating_sub(half_beat);
        let row_end = row.saturating_add(half_beat);
        for note in notes.iter() {
            if local_player_col(note.column, col_offset, cols).is_none()
                || !matches!(note.note_type, NoteType::Tap | NoteType::Lift)
                || note.is_fake
                || note.row_index == row
            {
                continue;
            }
            if note.row_index > row_begin && note.row_index < row_end {
                tap_in_middle = true;
                break;
            }
        }
        if tap_in_middle || count_held_tracks_at_row(notes, row, col_offset, cols) >= 1 {
            continue;
        }
        let Some(track) = first_tap_track_at_row(notes, row, col_offset, cols) else {
            continue;
        };
        let add_track = stomp_mirror_track(track, cols);
        let _ = set_added_tap_note(
            notes,
            timing_player,
            row,
            col_offset.saturating_add(add_track),
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
    let rows_per_interval = (ROWS_PER_BEAT.max(1) as usize) / 2;
    if rows_per_interval == 0 {
        return;
    }
    let max_row = player_rows(notes, col_offset, cols)
        .into_iter()
        .max()
        .unwrap_or(0);
    let end_row = max_row.saturating_add(1);
    let mut echo_track: Option<usize> = None;
    let mut row = 0usize;
    while row <= end_row {
        if count_nonempty_tracks_at_row(notes, row, col_offset, cols) == 0 {
            row = row.saturating_add(rows_per_interval);
            continue;
        }
        if let Some(track) = first_tap_track_at_row(notes, row, col_offset, cols) {
            echo_track = Some(track);
        }
        let Some(track) = echo_track else {
            row = row.saturating_add(rows_per_interval);
            continue;
        };
        let row_window_end = row.saturating_add(rows_per_interval.saturating_mul(2));
        let mut note_in_middle = false;
        for note in notes.iter() {
            if local_player_col(note.column, col_offset, cols).is_none() {
                continue;
            }
            if note.row_index > row && note.row_index < row_window_end {
                note_in_middle = true;
                break;
            }
        }
        if note_in_middle {
            row = row.saturating_add(rows_per_interval);
            continue;
        }

        let row_echo = row.saturating_add(rows_per_interval);
        if count_held_tracks_at_row(notes, row_echo, col_offset, cols) >= 2
            || is_hold_body_at_row(notes, row_echo, col_offset + track)
        {
            row = row.saturating_add(rows_per_interval);
            continue;
        }
        let _ = set_added_tap_note(notes, timing_player, row_echo, col_offset + track);
        row = row.saturating_add(rows_per_interval);
    }
}

fn find_tap_index(notes: &[Note], row: usize, column: usize) -> Option<usize> {
    notes.iter().position(|note| {
        note.row_index == row
            && note.column == column
            && note.note_type == NoteType::Tap
            && !note.is_fake
    })
}

pub fn convert_taps_to_holds(
    notes: &mut [Note],
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    simultaneous_holds: usize,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    let rows = player_rows(notes, col_offset, cols);
    let rows_per_beat = ROWS_PER_BEAT.max(1) as usize;

    for &row in &rows {
        let mut added_this_row = 0usize;
        for local in 0..cols {
            if added_this_row > simultaneous_holds {
                break;
            }
            let col = col_offset + local;
            let Some(head_idx) = find_tap_index(notes, row, col) else {
                continue;
            };
            let mut taps_left = simultaneous_holds as isize;
            let mut end_row = row.saturating_add(1);
            let mut add_hold = true;

            for &next_row in rows.iter().filter(|&&r| r > row) {
                end_row = next_row;
                if cell_has_any_note(notes, next_row, col) {
                    add_hold = false;
                    break;
                }

                let mut tracks_down = 0usize;
                for check_local in 0..cols {
                    let check_col = col_offset + check_local;
                    if is_hold_body_at_row(notes, next_row, check_col)
                        || cell_has_any_note(notes, next_row, check_col)
                    {
                        tracks_down = tracks_down.saturating_add(1);
                    }
                }

                taps_left -= tracks_down as isize;
                if taps_left == 0 {
                    break;
                }
                if taps_left < 0 {
                    add_hold = false;
                    break;
                }
            }

            if !add_hold {
                continue;
            }
            if end_row == row.saturating_add(1) {
                end_row = row.saturating_add(rows_per_beat);
            }

            let Some(end_beat) = timing_player.get_beat_for_row(end_row) else {
                continue;
            };
            let head_beat = notes[head_idx].beat;
            notes[head_idx].note_type = NoteType::Hold;
            notes[head_idx].hold = Some(HoldData {
                end_row_index: end_row,
                end_beat,
                result: None,
                life: INITIAL_HOLD_LIFE,
                let_go_started_at: None,
                let_go_starting_life: 0.0,
                last_held_row_index: row,
                last_held_beat: head_beat,
            });
            added_this_row = added_this_row.saturating_add(1);
        }
    }
}

pub fn apply_uncommon_masks_with_masks(
    notes: &mut Vec<Note>,
    insert_mask: u8,
    remove_mask: u8,
    holds_mask: u8,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    context_notes: &[Note],
    row_bounds: Option<(usize, usize)>,
    _player: usize,
) {
    if (remove_mask & REMOVE_MASK_BIT_LITTLE) != 0 {
        let rows_per_beat = ROWS_PER_BEAT.max(1) as usize;
        notes.retain(|note| note.row_index % rows_per_beat == 0);
    }

    if (holds_mask & HOLDS_MASK_BIT_NO_ROLLS) != 0 {
        for note in notes.iter_mut() {
            if note.note_type == NoteType::Roll {
                note.note_type = NoteType::Hold;
            }
        }
    }

    if (remove_mask & REMOVE_MASK_BIT_NO_HOLDS) != 0 {
        for note in notes.iter_mut() {
            if note.note_type == NoteType::Hold {
                note.note_type = NoteType::Tap;
                note.hold = None;
            }
        }
    }

    if (remove_mask & REMOVE_MASK_BIT_NO_MINES) != 0 {
        notes.retain(|note| !matches!(note.note_type, NoteType::Mine));
    }

    if (remove_mask & REMOVE_MASK_BIT_NO_JUMPS) != 0 {
        enforce_max_simultaneous_notes(notes, 1, col_offset, cols);
    }

    if (remove_mask & REMOVE_MASK_BIT_NO_FAKES) != 0 {
        notes.retain(|note| note.can_be_judged && !note.is_fake);
    }

    if (remove_mask & REMOVE_MASK_BIT_NO_HANDS) != 0 {
        enforce_max_simultaneous_notes(notes, 2, col_offset, cols);
    }

    if (remove_mask & REMOVE_MASK_BIT_NO_QUADS) != 0 {
        enforce_max_simultaneous_notes(notes, 3, col_offset, cols);
    }

    if (insert_mask & INSERT_MASK_BIT_BIG) != 0 {
        apply_insert_intelligent_taps(
            notes,
            timing_player,
            col_offset,
            cols,
            ROWS_PER_BEAT.max(1) as usize,
            (ROWS_PER_BEAT.max(1) / 2) as usize,
            ROWS_PER_BEAT.max(1) as usize,
            false,
        );
    }
    if (insert_mask & INSERT_MASK_BIT_QUICK) != 0 {
        apply_insert_intelligent_taps(
            notes,
            timing_player,
            col_offset,
            cols,
            (ROWS_PER_BEAT.max(1) / 2) as usize,
            (ROWS_PER_BEAT.max(1) / 4) as usize,
            ROWS_PER_BEAT.max(1) as usize,
            false,
        );
    }
    if (insert_mask & INSERT_MASK_BIT_BMRIZE) != 0 {
        apply_insert_intelligent_taps(
            notes,
            timing_player,
            col_offset,
            cols,
            ROWS_PER_BEAT.max(1) as usize,
            (ROWS_PER_BEAT.max(1) / 2) as usize,
            ROWS_PER_BEAT.max(1) as usize,
            false,
        );
        apply_insert_intelligent_taps(
            notes,
            timing_player,
            col_offset,
            cols,
            (ROWS_PER_BEAT.max(1) / 2) as usize,
            (ROWS_PER_BEAT.max(1) / 4) as usize,
            ROWS_PER_BEAT.max(1) as usize,
            false,
        );
    }
    if (insert_mask & INSERT_MASK_BIT_SKIPPY) != 0 {
        apply_insert_intelligent_taps(
            notes,
            timing_player,
            col_offset,
            cols,
            ROWS_PER_BEAT.max(1) as usize,
            ((ROWS_PER_BEAT.max(1) * 3) / 4) as usize,
            ROWS_PER_BEAT.max(1) as usize,
            true,
        );
    }
    if (insert_mask & INSERT_MASK_BIT_MINES) != 0
        && let Some((start_row, end_row)) = row_bounds
    {
        apply_mines_insert(
            notes,
            context_notes,
            timing_player,
            col_offset,
            cols,
            start_row,
            end_row,
        );
    }
    if (insert_mask & INSERT_MASK_BIT_ECHO) != 0 {
        apply_echo_insert(notes, timing_player, col_offset, cols);
    }
    if (insert_mask & INSERT_MASK_BIT_WIDE) != 0 {
        apply_wide_insert(notes, timing_player, col_offset, cols);
    }
    if (insert_mask & INSERT_MASK_BIT_STOMP) != 0 {
        apply_stomp_insert(notes, timing_player, col_offset, cols);
    }

    if (holds_mask & HOLDS_MASK_BIT_PLANTED) != 0 {
        convert_taps_to_holds(notes, timing_player, col_offset, cols, 1);
    }
    if (holds_mask & HOLDS_MASK_BIT_FLOORED) != 0 {
        convert_taps_to_holds(notes, timing_player, col_offset, cols, 2);
    }
    if (holds_mask & HOLDS_MASK_BIT_TWISTER) != 0 {
        convert_taps_to_holds(notes, timing_player, col_offset, cols, 3);
    }

    if (holds_mask & HOLDS_MASK_BIT_HOLDS_TO_ROLLS) != 0 {
        for note in notes.iter_mut() {
            if note.note_type == NoteType::Hold {
                note.note_type = NoteType::Roll;
            }
        }
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_LIFTS) != 0 {
        notes.retain(|note| note.note_type != NoteType::Lift);
    }

    sort_player_notes(notes);
}

pub fn apply_uncommon_chart_transforms(
    notes: &mut Vec<Note>,
    note_ranges: &mut [(usize, usize); MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    player_effects: &[ChartAttackEffects; MAX_PLAYERS],
    timing_players: &[&TimingData; MAX_PLAYERS],
) {
    if num_players == 0
        || !player_effects
            .iter()
            .take(num_players)
            .any(|effects| effects.has_note_masks())
    {
        return;
    }

    let mut transformed = Vec::with_capacity(notes.len());
    let mut transformed_ranges = [(0usize, 0usize); MAX_PLAYERS];

    for player in 0..num_players {
        let (start, end) = note_ranges[player];
        let slice_end = end.min(notes.len());
        let slice_start = start.min(slice_end);
        let out_start = transformed.len();
        let effects = player_effects[player];
        if !effects.has_note_masks() {
            transformed.extend_from_slice(&notes[slice_start..slice_end]);
            transformed_ranges[player] = (out_start, transformed.len());
            continue;
        }

        let mut player_notes = notes[slice_start..slice_end].to_vec();
        apply_uncommon_masks_with_masks(
            &mut player_notes,
            effects.insert_mask,
            effects.remove_mask,
            effects.holds_mask,
            timing_players[player],
            player.saturating_mul(cols_per_player),
            cols_per_player,
            &[],
            None,
            player,
        );
        transformed.extend(player_notes);
        transformed_ranges[player] = (out_start, transformed.len());
    }

    if num_players == 1 {
        transformed_ranges[1] = transformed_ranges[0];
    }

    *notes = transformed;
    *note_ranges = transformed_ranges;
}

fn turn_take_from(turn: GameplayTurnOption, cols: usize, seed: u64) -> Option<Vec<usize>> {
    if cols == 0 {
        return None;
    }
    match (turn, cols) {
        (GameplayTurnOption::None, _) => None,
        (GameplayTurnOption::Mirror, _) => Some((0..cols).rev().collect()),
        (GameplayTurnOption::LRMirror, 4) => Some(vec![3, 1, 2, 0]),
        (GameplayTurnOption::LRMirror, 8) => Some(vec![7, 5, 6, 4, 3, 1, 2, 0]),
        (GameplayTurnOption::UDMirror, 4) => Some(vec![0, 2, 1, 3]),
        (GameplayTurnOption::UDMirror, 8) => Some(vec![0, 2, 1, 3, 4, 6, 5, 7]),
        (GameplayTurnOption::Left, 4) => Some(vec![2, 0, 3, 1]),
        (GameplayTurnOption::Left, 8) => Some(vec![2, 0, 3, 1, 6, 4, 7, 5]),
        (GameplayTurnOption::Right, 4) => Some(vec![1, 3, 0, 2]),
        (GameplayTurnOption::Right, 8) => Some(vec![1, 3, 0, 2, 5, 7, 4, 6]),
        (GameplayTurnOption::Shuffle, _) => {
            let orig: Vec<usize> = (0..cols).collect();
            let mut attempt_seed = seed as u32;
            loop {
                let mut out = orig.clone();
                let mut rng = TurnRng::new(u64::from(attempt_seed));
                rng.shuffle(&mut out);
                if cols <= 1 || out != orig {
                    return Some(out);
                }
                attempt_seed = attempt_seed.wrapping_add(1);
            }
        }
        _ => None,
    }
}

pub fn apply_turn_permutation(
    notes: &mut [Note],
    note_range: (usize, usize),
    col_offset: usize,
    cols: usize,
    turn: GameplayTurnOption,
    seed: u64,
) {
    let Some(take_from) = turn_take_from(turn, cols, seed) else {
        return;
    };
    if take_from.len() != cols {
        return;
    }
    let mut old_to_new = vec![0usize; cols];
    for (new_col, &old_col) in take_from.iter().enumerate() {
        if old_col < cols {
            old_to_new[old_col] = new_col;
        }
    }
    let (start, end) = note_range;
    for n in &mut notes[start..end] {
        if n.column < col_offset {
            continue;
        }
        let local = n.column - col_offset;
        if local < cols {
            n.column = col_offset + old_to_new[local];
        }
    }
}

fn update_active_turn_holds_for_row(
    notes: &[Note],
    row_index: usize,
    grid: &[usize; MAX_COLS],
    cols: usize,
    hold_end_row: &mut [Option<usize>; MAX_COLS],
) {
    for hold_end in hold_end_row.iter_mut().take(cols.min(MAX_COLS)) {
        if let Some(end) = *hold_end
            && row_index > end
        {
            *hold_end = None;
        }
    }

    for (col, &idx) in grid.iter().enumerate().take(cols.min(MAX_COLS)) {
        if idx == usize::MAX {
            continue;
        }
        if matches!(notes[idx].note_type, NoteType::Hold | NoteType::Roll) {
            let end = notes[idx]
                .hold
                .as_ref()
                .map(|h| h.end_row_index)
                .unwrap_or(row_index);
            hold_end_row[col] = Some(end);
        }
    }
}

pub fn apply_super_shuffle_taps(
    notes: &mut [Note],
    note_range: (usize, usize),
    col_offset: usize,
    cols: usize,
    seed: u64,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    let row_grids = build_row_grids(notes, note_range, col_offset, cols);
    let mut rng = TurnRng::new(seed);
    let mut hold_end_row: [Option<usize>; MAX_COLS] = [None; MAX_COLS];

    for row_grid in row_grids {
        let row = row_grid.row_index;
        let mut grid = row_grid.note_indices;
        update_active_turn_holds_for_row(notes, row, &grid, cols, &mut hold_end_row);

        for t1 in 0..cols {
            if hold_end_row[t1].is_some() {
                continue;
            }
            let idx1 = grid[t1];
            if idx1 == usize::MAX {
                continue;
            }
            if matches!(notes[idx1].note_type, NoteType::Hold | NoteType::Roll) {
                continue;
            }

            let mut tried_mask: u16 = 0;
            for _ in 0..4 {
                let t2 = rng.gen_range(cols);
                let bit = 1u16 << (t2 as u32);
                if (tried_mask & bit) != 0 {
                    continue;
                }
                tried_mask |= bit;
                if t1 == t2 {
                    break;
                }
                if hold_end_row[t2].is_some() {
                    continue;
                }
                let idx2 = grid[t2];
                if idx2 != usize::MAX
                    && matches!(notes[idx2].note_type, NoteType::Hold | NoteType::Roll)
                {
                    continue;
                }

                if idx2 == usize::MAX {
                    notes[idx1].column = col_offset + t2;
                    grid[t2] = idx1;
                    grid[t1] = usize::MAX;
                } else {
                    notes[idx1].column = col_offset + t2;
                    notes[idx2].column = col_offset + t1;
                    grid.swap(t1, t2);
                }
                break;
            }
        }
    }
}

pub fn apply_hyper_shuffle(
    notes: &mut [Note],
    note_range: (usize, usize),
    col_offset: usize,
    cols: usize,
    seed: u64,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    let row_grids = build_row_grids(notes, note_range, col_offset, cols);
    let mut rng = TurnRng::new(seed);
    let mut hold_end_row: [Option<usize>; MAX_COLS] = [None; MAX_COLS];

    for row_grid in row_grids {
        let row = row_grid.row_index;
        let grid = row_grid.note_indices;
        for hold_end in hold_end_row.iter_mut().take(cols) {
            if let Some(end) = *hold_end
                && row > end
            {
                *hold_end = None;
            }
        }

        let mut free_cols = [0usize; MAX_COLS];
        let mut free_len = 0usize;
        for (col, hold_end) in hold_end_row.iter().enumerate().take(cols) {
            if hold_end.is_none() {
                free_cols[free_len] = col;
                free_len += 1;
            }
        }
        if free_len == 0 {
            continue;
        }

        let mut row_notes = [usize::MAX; MAX_COLS];
        let mut notes_len = 0usize;
        for (col, &idx) in grid.iter().enumerate().take(cols) {
            if hold_end_row[col].is_some() {
                continue;
            }
            if idx == usize::MAX {
                continue;
            }
            row_notes[notes_len] = idx;
            notes_len += 1;
        }
        if notes_len == 0 {
            continue;
        }

        rng.shuffle(&mut free_cols[..free_len]);
        let place_len = notes_len.min(free_len);
        for (&idx, &col) in row_notes.iter().zip(free_cols.iter()).take(place_len) {
            notes[idx].column = col_offset + col;
        }

        for &idx in row_notes.iter().take(place_len) {
            if !matches!(notes[idx].note_type, NoteType::Hold | NoteType::Roll) {
                continue;
            }
            let local = notes[idx].column.saturating_sub(col_offset);
            if local >= cols {
                continue;
            }
            let end = notes[idx]
                .hold
                .as_ref()
                .map(|h| h.end_row_index)
                .unwrap_or(row);
            hold_end_row[local] = Some(end);
        }
    }
}

pub fn apply_turn_options(
    notes: &mut [Note],
    note_ranges: [(usize, usize); MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    player_turn_options: [GameplayTurnOption; MAX_PLAYERS],
    base_seed: u64,
) {
    for (player, turn) in player_turn_options
        .iter()
        .copied()
        .enumerate()
        .take(num_players.min(MAX_PLAYERS))
    {
        let note_range = note_ranges[player];
        let col_offset = player * cols_per_player;
        match turn {
            GameplayTurnOption::None => {}
            GameplayTurnOption::Blender => {
                apply_turn_permutation(
                    notes,
                    note_range,
                    col_offset,
                    cols_per_player,
                    GameplayTurnOption::Shuffle,
                    base_seed,
                );
                apply_super_shuffle_taps(
                    notes,
                    note_range,
                    col_offset,
                    cols_per_player,
                    base_seed ^ (0xD00D_F00D_u64.wrapping_mul(player as u64 + 1)),
                );
            }
            GameplayTurnOption::Random => {
                apply_hyper_shuffle(
                    notes,
                    note_range,
                    col_offset,
                    cols_per_player,
                    base_seed ^ (0xA5A5_5A5A_u64.wrapping_mul(player as u64 + 1)),
                );
            }
            other => {
                apply_turn_permutation(
                    notes,
                    note_range,
                    col_offset,
                    cols_per_player,
                    other,
                    base_seed,
                );
            }
        }
    }
}

