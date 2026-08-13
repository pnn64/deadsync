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
    if num_players == 2 {
        return (column >= cols_per_player) as usize;
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

#[inline(always)]
fn notes_row_col_sorted(notes: &[Note]) -> bool {
    notes.windows(2).all(|pair| {
        (pair[0].row_index, pair[0].column) <= (pair[1].row_index, pair[1].column)
    })
}

pub fn player_rows(notes: &[Note], col_offset: usize, cols: usize) -> Vec<usize> {
    let mut rows = Vec::with_capacity(notes.len());
    let mut ordered = true;
    for note in notes {
        if local_player_col(note.column, col_offset, cols).is_some() {
            match rows.last().copied() {
                Some(last) if last == note.row_index => {}
                Some(last) => {
                    ordered &= last < note.row_index;
                    rows.push(note.row_index);
                }
                None => rows.push(note.row_index),
            }
        }
    }
    if !ordered {
        rows.sort_unstable();
        rows.dedup();
    }
    rows
}

fn player_rows_rescan(notes: &[Note], col_offset: usize, cols: usize) -> Vec<usize> {
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

#[cfg(any(test, feature = "bench-support"))]
pub fn player_rows_reference(notes: &[Note], col_offset: usize, cols: usize) -> Vec<usize> {
    player_rows_rescan(notes, col_offset, cols)
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
    let Some(note) = added_tap_note(timing_player, row, column) else {
        return false;
    };
    remove_cell_notes(notes, row, column);
    notes.push(note);
    true
}

fn added_tap_note(timing_player: &TimingData, row: usize, column: usize) -> Option<Note> {
    let beat = timing_player.get_beat_for_row(row)?;
    Some(Note {
        beat,
        quantization_idx: quantization_index_from_beat(beat),
        column,
        note_type: NoteType::Tap,
        row_index: row,
        result: None,
        early_result: None,
        hold: None,
        mine_result: None,
        is_fake: false,
        can_be_judged: timing_player.is_judgable_at_beat(beat),
    })
}

pub fn set_added_mine_note(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    row: usize,
    column: usize,
) -> bool {
    let Some(note) = added_mine_note(timing_player, row, column) else {
        return false;
    };
    remove_cell_notes(notes, row, column);
    notes.push(note);
    true
}

fn added_mine_note(timing_player: &TimingData, row: usize, column: usize) -> Option<Note> {
    let beat = timing_player.get_beat_for_row(row)?;
    Some(Note {
        beat,
        quantization_idx: quantization_index_from_beat(beat),
        column,
        note_type: NoteType::Mine,
        row_index: row,
        result: None,
        early_result: None,
        hold: None,
        mine_result: None,
        is_fake: false,
        can_be_judged: timing_player.is_judgable_at_beat(beat),
    })
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

fn sorted_track_range_has_any_note(
    notes: &[Note],
    column: usize,
    start_row: usize,
    end_row: usize,
) -> bool {
    if end_row < start_row {
        return false;
    }
    debug_assert!(notes_row_sorted(notes));
    let start = notes.partition_point(|note| note.row_index < start_row);
    let end = notes.partition_point(|note| note.row_index <= end_row);
    notes[start..end].iter().any(|note| note.column == column)
}

#[cfg(feature = "bench-support")]
pub fn sorted_track_range_has_any_note_bench(
    notes: &[Note],
    column: usize,
    start_row: usize,
    end_row: usize,
) -> bool {
    sorted_track_range_has_any_note(notes, column, start_row, end_row)
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
    debug_assert!(notes_row_sorted(notes));
    debug_assert!(notes_row_sorted(context_notes));

    let original_len = notes.len();
    let mut row_count = 0usize;
    let mut place_every_rows = 6usize;
    let mut row_start = 0usize;
    while row_start < original_len {
        let row = notes[row_start].row_index;
        let mut row_end = row_start + 1;
        while row_end < original_len && notes[row_end].row_index == row {
            row_end += 1;
        }
        if row >= start_row
            && row <= end_row
            && notes[row_start..row_end]
                .iter()
                .any(|note| local_player_col(note.column, col_offset, cols).is_some())
        {
            row_count = row_count.saturating_add(1);
            if row_count >= place_every_rows {
                convert_tap_row_to_mines(&mut notes[row_start..row_end], row);
                row_count = 0;
                place_every_rows = if place_every_rows == 6 { 7 } else { 6 };
            }
        }
        row_start = row_end;
    }

    let half_beat_rows = (ROWS_PER_BEAT.max(1) / 2) as usize;
    for note_index in 0..original_len {
        let Some((column, end_row_index)) = (|| {
            let note = &notes[note_index];
            matches!(note.note_type, NoteType::Hold | NoteType::Roll)
                .then_some((note.column, note.hold.as_ref()?.end_row_index))
        })() else {
            continue;
        };
        let mine_row = end_row_index.saturating_add(half_beat_rows);
        if mine_row < start_row || mine_row > end_row {
            continue;
        }
        let range_start = mine_row.saturating_sub(half_beat_rows).saturating_add(1);
        let range_end = mine_row.saturating_add(half_beat_rows).saturating_sub(1);
        if sorted_track_range_has_any_note(context_notes, column, range_start, range_end)
            || sorted_track_range_has_any_note(
                &notes[..original_len],
                column,
                range_start,
                range_end,
            )
            || track_range_has_any_note(
                &notes[original_len..],
                column,
                range_start,
                range_end,
            )
        {
            continue;
        }
        let Some(mine) = added_mine_note(timing_player, mine_row, column) else {
            continue;
        };
        let mine_start = notes[..original_len].partition_point(|note| note.row_index < mine_row);
        let mine_end = notes[..original_len].partition_point(|note| note.row_index <= mine_row);
        convert_tap_row_to_mines(&mut notes[mine_start..mine_end], mine_row);
        notes.push(mine);
    }
}

#[cfg(any(test, feature = "bench-support"))]
pub fn apply_mines_insert_reference(
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

    let player_rows = player_rows_reference(notes, col_offset, cols);
    let hold_heads: Vec<(usize, usize)> = notes
        .iter()
        .filter_map(|note| {
            matches!(note.note_type, NoteType::Hold | NoteType::Roll)
                .then_some((note.column, note.hold.as_ref()?.end_row_index))
        })
        .collect();
    let mut mine_rows = Vec::with_capacity(player_rows.len() / 6 + hold_heads.len() + 1);
    let mut row_count = 0usize;
    let mut place_every_rows = 6usize;
    for row in player_rows {
        if row < start_row || row > end_row {
            continue;
        }
        row_count = row_count.saturating_add(1);
        if row_count < place_every_rows {
            continue;
        }
        mine_rows.push(row);
        row_count = 0;
        place_every_rows = if place_every_rows == 6 { 7 } else { 6 };
    }

    let half_beat_rows = (ROWS_PER_BEAT.max(1) / 2) as usize;
    for (column, end_row_index) in hold_heads {
        let mine_row = end_row_index.saturating_add(half_beat_rows);
        if mine_row < start_row || mine_row > end_row {
            continue;
        }
        let range_start = mine_row.saturating_sub(half_beat_rows).saturating_add(1);
        let range_end = mine_row.saturating_add(half_beat_rows).saturating_sub(1);
        if track_range_has_any_note(context_notes, column, range_start, range_end)
            || track_range_has_any_note(notes, column, range_start, range_end)
        {
            continue;
        }
        if !set_added_mine_note(notes, timing_player, mine_row, column) {
            continue;
        }
        mine_rows.push(mine_row);
    }

    mine_rows.sort_unstable();
    mine_rows.dedup();
    for note in notes {
        if note.note_type == NoteType::Tap && mine_rows.binary_search(&note.row_index).is_ok() {
            note.note_type = NoteType::Mine;
            note.hold = None;
            note.mine_result = None;
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

#[derive(Clone, Copy, Default)]
struct IntelligentRowSummary {
    nonempty: u64,
    tap_or_hold: u64,
}

impl IntelligentRowSummary {
    #[inline(always)]
    fn single_endpoint(self) -> bool {
        self.nonempty.count_ones() == 1 && self.tap_or_hold.count_ones() == 1
    }

    #[inline(always)]
    fn first_track(self) -> Option<usize> {
        (self.nonempty != 0).then(|| self.nonempty.trailing_zeros() as usize)
    }
}

fn intelligent_row_summary(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> IntelligentRowSummary {
    let start = notes.partition_point(|note| note.row_index < row);
    let end = notes.partition_point(|note| note.row_index <= row);
    intelligent_row_summary_slice(&notes[start..end], col_offset, cols)
}

fn intelligent_row_summary_slice(
    notes: &[Note],
    col_offset: usize,
    cols: usize,
) -> IntelligentRowSummary {
    notes.iter().fold(
        IntelligentRowSummary::default(),
        |mut summary, note| {
            let Some(local) = local_player_col(note.column, col_offset, cols) else {
                return summary;
            };
            let bit = 1u64 << local;
            summary.nonempty |= bit;
            if matches!(
                note.note_type,
                NoteType::Tap | NoteType::Lift | NoteType::Hold | NoteType::Roll
            ) {
                summary.tap_or_hold |= bit;
            }
            summary
        },
    )
}

fn intelligent_range_has_note(
    notes: &[Note],
    start_row: usize,
    end_row: usize,
    col_offset: usize,
    cols: usize,
) -> bool {
    if end_row < start_row {
        return false;
    }
    let start = notes.partition_point(|note| note.row_index < start_row);
    let end = notes.partition_point(|note| note.row_index <= end_row);
    notes[start..end]
        .iter()
        .any(|note| local_player_col(note.column, col_offset, cols).is_some())
}

fn intelligent_candidate_count(
    notes: &[Note],
    col_offset: usize,
    cols: usize,
    window_stride_rows: usize,
) -> usize {
    let mut count = 0usize;
    let mut row_start = 0usize;
    while row_start < notes.len() {
        let row = notes[row_start].row_index;
        let mut row_end = row_start + 1;
        while row_end < notes.len() && notes[row_end].row_index == row {
            row_end += 1;
        }
        if row.is_multiple_of(window_stride_rows)
            && notes[row_start..row_end]
                .iter()
                .any(|note| local_player_col(note.column, col_offset, cols).is_some())
        {
            count += 1;
        }
        row_start = row_end;
    }
    count
}

fn intelligent_add_track(
    earlier_track: Option<usize>,
    later_track: usize,
    cols: usize,
    skippy_mode: bool,
) -> usize {
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
    }
}

fn set_added_tap_note_sorted(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    row: usize,
    column: usize,
) -> bool {
    let Some(note) = added_tap_note(timing_player, row, column) else {
        return false;
    };
    let row_start = notes.partition_point(|note| note.row_index < row);
    let mut row_end = notes.partition_point(|note| note.row_index <= row);
    let mut index = row_start;
    while index < row_end {
        if notes[index].column == column {
            notes.remove(index);
            row_end -= 1;
        } else {
            index += 1;
        }
    }
    let insert = row_start
        + notes[row_start..row_end].partition_point(|existing| existing.column <= column);
    notes.insert(insert, note);
    true
}

#[allow(clippy::too_many_arguments)]
fn apply_insert_intelligent_taps_rescan(
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
        if !row.is_multiple_of(window_stride_rows) {
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
        let track_to_add = intelligent_add_track(earlier_track, later_track, cols, skippy_mode);

        let _ = set_added_tap_note(
            notes,
            timing_player,
            row_to_add,
            col_offset.saturating_add(track_to_add),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_insert_intelligent_taps_sorted(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    window_size_rows: usize,
    insert_offset_rows: usize,
    window_stride_rows: usize,
    skippy_mode: bool,
) {
    debug_assert!(notes_row_sorted(notes));
    let candidate_count =
        intelligent_candidate_count(notes, col_offset, cols, window_stride_rows);
    notes.reserve(candidate_count);

    let require_begin = !skippy_mode;
    let mut row_cursor = 0usize;
    let mut hold_cursor = 0usize;
    let mut latest = [usize::MAX; MAX_COLS];
    while row_cursor < notes.len() {
        let row = notes[row_cursor].row_index;
        let row_start = row_cursor;
        row_cursor += 1;
        while row_cursor < notes.len() && notes[row_cursor].row_index == row {
            row_cursor += 1;
        }
        let earlier =
            intelligent_row_summary_slice(&notes[row_start..row_cursor], col_offset, cols);
        if earlier.nonempty == 0 || !row.is_multiple_of(window_stride_rows) {
            continue;
        }

        let row_later = row.saturating_add(window_size_rows);
        let later = intelligent_row_summary(notes, row_later, col_offset, cols);
        if (require_begin && !earlier.single_endpoint()) || !later.single_endpoint() {
            continue;
        }

        let body_row = row.saturating_add(1);
        let body_cells = advance_latest_notes(
            notes,
            &mut hold_cursor,
            body_row,
            col_offset,
            cols,
            &mut latest,
        );
        let body_mask = tracks_down_mask(notes, &latest, body_row, cols) & !body_cells;
        if body_mask != 0
            || intelligent_range_has_note(
                notes,
                body_row,
                row_later.saturating_sub(1),
                col_offset,
                cols,
            )
        {
            continue;
        }

        let Some(later_track) = later.first_track() else {
            continue;
        };
        let track_to_add =
            intelligent_add_track(earlier.first_track(), later_track, cols, skippy_mode);
        let _ = set_added_tap_note_sorted(
            notes,
            timing_player,
            row.saturating_add(insert_offset_rows),
            col_offset.saturating_add(track_to_add),
        );
    }
}

#[allow(clippy::too_many_arguments)]
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
    let can_stream = window_stride_rows != 0
        && insert_offset_rows > 1
        && !insert_offset_rows.is_multiple_of(window_stride_rows)
        && notes_row_col_sorted(notes)
        && notes.last().is_none_or(|note| {
            note.row_index.checked_add(window_size_rows).is_some()
                && note.row_index.checked_add(insert_offset_rows).is_some()
        });
    if can_stream {
        apply_insert_intelligent_taps_sorted(
            notes,
            timing_player,
            col_offset,
            cols,
            window_size_rows,
            insert_offset_rows,
            window_stride_rows,
            skippy_mode,
        );
    } else {
        apply_insert_intelligent_taps_rescan(
            notes,
            timing_player,
            col_offset,
            cols,
            window_size_rows,
            insert_offset_rows,
            window_stride_rows,
            skippy_mode,
        );
    }
}

#[cfg(any(test, feature = "bench-support"))]
#[allow(clippy::too_many_arguments)]
pub fn apply_insert_intelligent_taps_reference(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    window_size_rows: usize,
    insert_offset_rows: usize,
    window_stride_rows: usize,
    skippy_mode: bool,
) {
    apply_insert_intelligent_taps_rescan(
        notes,
        timing_player,
        col_offset,
        cols,
        window_size_rows,
        insert_offset_rows,
        window_stride_rows,
        skippy_mode,
    );
}

#[cfg(feature = "bench-support")]
pub fn intelligent_candidate_count_bench(
    notes: &[Note],
    col_offset: usize,
    cols: usize,
    window_stride_rows: usize,
) -> usize {
    intelligent_candidate_count(notes, col_offset, cols, window_stride_rows)
}

#[cfg(feature = "bench-support")]
pub fn intelligent_candidate_count_reference_bench(
    notes: &[Note],
    col_offset: usize,
    cols: usize,
    window_stride_rows: usize,
) -> usize {
    player_rows(notes, col_offset, cols)
        .into_iter()
        .filter(|row| row % window_stride_rows == 0)
        .count()
}

#[cfg(feature = "bench-support")]
pub fn intelligent_endpoint_checksum_bench(
    notes: &[Note],
    rows: &[usize],
    window_size_rows: usize,
    col_offset: usize,
    cols: usize,
) -> u64 {
    rows.iter().fold(0u64, |checksum, &row| {
        let earlier = intelligent_row_summary(notes, row, col_offset, cols);
        let later = intelligent_row_summary(
            notes,
            row.saturating_add(window_size_rows),
            col_offset,
            cols,
        );
        let value = u64::from(earlier.nonempty.count_ones())
            ^ u64::from(earlier.tap_or_hold.count_ones()).rotate_left(8)
            ^ u64::from(later.nonempty.count_ones()).rotate_left(16)
            ^ u64::from(later.tap_or_hold.count_ones()).rotate_left(24);
        checksum.rotate_left(7) ^ value
    })
}

#[cfg(feature = "bench-support")]
pub fn intelligent_endpoint_checksum_reference_bench(
    notes: &[Note],
    rows: &[usize],
    window_size_rows: usize,
    col_offset: usize,
    cols: usize,
) -> u64 {
    rows.iter().fold(0u64, |checksum, &row| {
        let later = row.saturating_add(window_size_rows);
        let value = count_nonempty_tracks_at_row(notes, row, col_offset, cols) as u64
            ^ (count_tap_or_hold_tracks_at_row(notes, row, col_offset, cols) as u64).rotate_left(8)
            ^ (count_nonempty_tracks_at_row(notes, later, col_offset, cols) as u64).rotate_left(16)
            ^ (count_tap_or_hold_tracks_at_row(notes, later, col_offset, cols) as u64)
                .rotate_left(24);
        checksum.rotate_left(7) ^ value
    })
}

#[cfg(feature = "bench-support")]
pub fn intelligent_window_checksum_bench(
    notes: &[Note],
    rows: &[usize],
    window_size_rows: usize,
    col_offset: usize,
    cols: usize,
) -> u64 {
    let mut cursor = 0usize;
    let mut latest = [usize::MAX; MAX_COLS];
    rows.iter().fold(0u64, |checksum, &row| {
        let body_row = row.saturating_add(1);
        let cells = advance_latest_notes(
            notes,
            &mut cursor,
            body_row,
            col_offset,
            cols,
            &mut latest,
        );
        let body = tracks_down_mask(notes, &latest, body_row, cols) & !cells;
        let middle = intelligent_range_has_note(
            notes,
            body_row,
            row.saturating_add(window_size_rows).saturating_sub(1),
            col_offset,
            cols,
        );
        checksum.rotate_left(7) ^ body ^ ((middle as u64) << 63)
    })
}

#[cfg(feature = "bench-support")]
pub fn intelligent_window_checksum_reference_bench(
    notes: &[Note],
    rows: &[usize],
    window_size_rows: usize,
    col_offset: usize,
    cols: usize,
) -> u64 {
    rows.iter().fold(0u64, |checksum, &row| {
        let body_row = row.saturating_add(1);
        let body = (0..cols).fold(0u64, |mask, local| {
            mask | ((is_hold_body_at_row(notes, body_row, col_offset + local) as u64) << local)
        });
        let middle = notes.iter().any(|note| {
            local_player_col(note.column, col_offset, cols).is_some()
                && note.row_index >= body_row
                && note.row_index <= row.saturating_add(window_size_rows).saturating_sub(1)
        });
        checksum.rotate_left(7) ^ body ^ ((middle as u64) << 63)
    })
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

fn fill_row_taps(
    notes: &[Note],
    note_range: std::ops::Range<usize>,
    col_offset: usize,
    cols: usize,
    taps: &mut [usize; MAX_COLS],
) {
    taps[..cols].fill(usize::MAX);
    for note_index in note_range {
        let note = &notes[note_index];
        let Some(local) = local_player_col(note.column, col_offset, cols) else {
            continue;
        };
        if taps[local] == usize::MAX && note.note_type == NoteType::Tap && !note.is_fake {
            taps[local] = note_index;
        }
    }
}

fn advance_latest_notes(
    notes: &[Note],
    cursor: &mut usize,
    row: usize,
    col_offset: usize,
    cols: usize,
    latest: &mut [usize; MAX_COLS],
) -> u64 {
    let mut row_mask = 0u64;
    while *cursor < notes.len() && notes[*cursor].row_index <= row {
        let note = &notes[*cursor];
        if let Some(local) = local_player_col(note.column, col_offset, cols) {
            latest[local] = *cursor;
            if note.row_index == row {
                row_mask |= 1u64 << local;
            }
        }
        *cursor += 1;
    }
    row_mask
}

#[derive(Clone, Copy)]
struct HoldScanRow {
    row: usize,
    note_start: usize,
    note_end: usize,
    cell_mask: u64,
}

fn next_hold_scan_row(
    notes: &[Note],
    cursor: &mut usize,
    col_offset: usize,
    cols: usize,
    latest: &mut [usize; MAX_COLS],
) -> Option<HoldScanRow> {
    while *cursor < notes.len() {
        let row = notes[*cursor].row_index;
        let note_start = *cursor;
        let cell_mask = advance_latest_notes(notes, cursor, row, col_offset, cols, latest);
        if cell_mask != 0 {
            return Some(HoldScanRow {
                row,
                note_start,
                note_end: *cursor,
                cell_mask,
            });
        }
    }
    None
}

fn tracks_down_mask(
    notes: &[Note],
    latest: &[usize; MAX_COLS],
    row: usize,
    cols: usize,
) -> u64 {
    latest[..cols]
        .iter()
        .enumerate()
        .fold(0u64, |mask, (local, &note_index)| {
            let Some(note) = notes.get(note_index) else {
                return mask;
            };
            let down = note.row_index == row
                || (note.row_index < row
                    && matches!(note.note_type, NoteType::Hold | NoteType::Roll)
                    && note
                        .hold
                        .as_ref()
                        .is_some_and(|hold| hold.end_row_index >= row));
            mask | ((down as u64) << local)
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
    if !notes_row_sorted(notes) {
        convert_taps_to_holds_rescan(
            notes,
            timing_player,
            col_offset,
            cols,
            simultaneous_holds,
        );
        return;
    }
    debug_assert!(notes_row_sorted(notes));
    let rows_per_beat = ROWS_PER_BEAT.max(1) as usize;
    let mut latest = [usize::MAX; MAX_COLS];
    let mut note_cursor = 0usize;
    let mut taps = [usize::MAX; MAX_COLS];

    while let Some(scan_row) =
        next_hold_scan_row(notes, &mut note_cursor, col_offset, cols, &mut latest)
    {
        let row = scan_row.row;
        fill_row_taps(
            notes,
            scan_row.note_start..scan_row.note_end,
            col_offset,
            cols,
            &mut taps,
        );

        let mut added_this_row = 0usize;
        for (local, &head_idx) in taps[..cols].iter().enumerate() {
            if added_this_row > simultaneous_holds {
                break;
            }
            if head_idx == usize::MAX {
                continue;
            }
            let mut taps_left = simultaneous_holds as isize;
            let mut end_row = row.saturating_add(1);
            let mut add_hold = true;
            let mut scan_latest = latest;
            let mut scan_cursor = note_cursor;

            while let Some(next) =
                next_hold_scan_row(notes, &mut scan_cursor, col_offset, cols, &mut scan_latest)
            {
                end_row = next.row;
                if next.cell_mask & (1u64 << local) != 0 {
                    add_hold = false;
                    break;
                }

                taps_left -= tracks_down_mask(notes, &scan_latest, next.row, cols).count_ones()
                    as isize;
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

fn convert_taps_to_holds_rescan(
    notes: &mut [Note],
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    simultaneous_holds: usize,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    let rows = player_rows_rescan(notes, col_offset, cols);
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

#[cfg(any(test, feature = "bench-support"))]
pub fn convert_taps_to_holds_reference(
    notes: &mut [Note],
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    simultaneous_holds: usize,
) {
    convert_taps_to_holds_rescan(
        notes,
        timing_player,
        col_offset,
        cols,
        simultaneous_holds,
    );
}

#[cfg(feature = "bench-support")]
pub fn hold_rows_reference_bench(notes: &[Note], col_offset: usize, cols: usize) -> u64 {
    player_rows_reference(notes, col_offset, cols)
        .into_iter()
        .fold(0u64, |checksum, row| {
            checksum
                .wrapping_mul(0x9E37_79B1)
                .wrapping_add(row as u64)
        })
}

#[cfg(feature = "bench-support")]
pub fn hold_rows_bench(notes: &[Note], col_offset: usize, cols: usize) -> u64 {
    let mut cursor = 0usize;
    let mut latest = [usize::MAX; MAX_COLS];
    let mut checksum = 0u64;
    while let Some(row) = next_hold_scan_row(notes, &mut cursor, col_offset, cols, &mut latest) {
        checksum = checksum
            .wrapping_mul(0x9E37_79B1)
            .wrapping_add(row.row as u64);
    }
    checksum
}

#[cfg(feature = "bench-support")]
pub fn hold_row_local_reference_bench(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> u64 {
    (0..cols).fold(0u64, |checksum, local| {
        let column = col_offset + local;
        let tap = find_tap_index(notes, row, column).unwrap_or(usize::MAX) as u64;
        checksum
            .wrapping_mul(0x9E37_79B1)
            .wrapping_add(tap)
            .wrapping_add((cell_has_any_note(notes, row, column) as u64) << 63)
    })
}

#[cfg(feature = "bench-support")]
pub fn hold_row_local_bench(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> u64 {
    let start = notes.partition_point(|note| note.row_index < row);
    let end = notes.partition_point(|note| note.row_index <= row);
    let mut taps = [usize::MAX; MAX_COLS];
    fill_row_taps(notes, start..end, col_offset, cols, &mut taps);
    let cell_mask = notes[start..end].iter().fold(0u64, |mask, note| {
        let Some(local) = local_player_col(note.column, col_offset, cols) else {
            return mask;
        };
        mask | (1u64 << local)
    });
    taps[..cols]
        .iter()
        .enumerate()
        .fold(0u64, |checksum, (local, &tap)| {
            checksum
                .wrapping_mul(0x9E37_79B1)
                .wrapping_add(tap as u64)
                .wrapping_add(((cell_mask >> local) & 1) << 63)
        })
}

#[cfg(feature = "bench-support")]
pub fn hold_body_masks_reference_bench(
    notes: &[Note],
    rows: &[usize],
    col_offset: usize,
    cols: usize,
) -> u64 {
    rows.iter().fold(0u64, |checksum, &row| {
        let mask = (0..cols).fold(0u64, |mask, local| {
            mask | ((is_hold_body_at_row(notes, row, col_offset + local) as u64) << local)
        });
        checksum.rotate_left(7) ^ mask
    })
}

#[cfg(feature = "bench-support")]
pub fn hold_body_masks_bench(
    notes: &[Note],
    rows: &[usize],
    col_offset: usize,
    cols: usize,
) -> u64 {
    let mut cursor = 0usize;
    let mut latest = [usize::MAX; MAX_COLS];
    rows.iter().fold(0u64, |checksum, &row| {
        let cell_mask = advance_latest_notes(
            notes,
            &mut cursor,
            row,
            col_offset,
            cols,
            &mut latest,
        );
        let mask = tracks_down_mask(notes, &latest, row, cols) & !cell_mask;
        checksum.rotate_left(7) ^ mask
    })
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
        if !notes_row_sorted(notes) {
            sort_player_notes(notes);
        }
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

    if holds_mask & (HOLDS_MASK_BIT_PLANTED | HOLDS_MASK_BIT_FLOORED | HOLDS_MASK_BIT_TWISTER) != 0
        && !notes_row_sorted(notes)
    {
        sort_player_notes(notes);
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

    if !notes_row_col_sorted(notes) {
        sort_player_notes(notes);
    }
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

    if num_players == 1 {
        let (start, end) = note_ranges[0];
        let end = end.min(notes.len());
        let start = start.min(end);
        if start == 0 && end == notes.len() {
            // This is the normal single-player shape. The transform already
            // accepts the owned buffer, so rebuilding it through two copies
            // only adds chart-sized allocation and memory traffic.
            let effects = player_effects[0];
            apply_uncommon_masks_with_masks(
                notes,
                effects.insert_mask,
                effects.remove_mask,
                effects.holds_mask,
                timing_players[0],
                0,
                cols_per_player,
                &[],
                None,
                0,
            );
            note_ranges[0] = (0, notes.len());
            note_ranges[1] = note_ranges[0];
            return;
        }
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

#[cfg(any(test, feature = "bench-support"))]
pub fn apply_uncommon_chart_transforms_reference(
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

fn fill_turn_take_from(
    turn: GameplayTurnOption,
    cols: usize,
    seed: u64,
    out: &mut [usize; MAX_COLS],
) -> Option<usize> {
    // Gameplay has a fixed MAX_COLS lane domain. Keeping both permutations in
    // fixed arrays removes two tiny heap operations from every ordinary turn.
    if cols == 0 || cols > MAX_COLS {
        return None;
    }
    let fixed: Option<&[usize]> = match (turn, cols) {
        (GameplayTurnOption::None, _) => None,
        (GameplayTurnOption::Mirror, 5) => Some(&[3, 4, 2, 0, 1]),
        (GameplayTurnOption::Mirror, 10) => Some(&[8, 9, 7, 5, 6, 3, 4, 2, 0, 1]),
        (GameplayTurnOption::Mirror, _) => {
            for (value, source) in out[..cols].iter_mut().zip((0..cols).rev()) {
                *value = source;
            }
            return Some(cols);
        }
        (GameplayTurnOption::LRMirror, 5) => Some(&[4, 3, 2, 1, 0]),
        (GameplayTurnOption::LRMirror, 10) => Some(&[9, 8, 7, 6, 5, 4, 3, 2, 1, 0]),
        (GameplayTurnOption::LRMirror, 4) => Some(&[3, 1, 2, 0]),
        (GameplayTurnOption::LRMirror, 8) => Some(&[7, 5, 6, 4, 3, 1, 2, 0]),
        (GameplayTurnOption::UDMirror, 4) => Some(&[0, 2, 1, 3]),
        (GameplayTurnOption::UDMirror, 8) => Some(&[0, 2, 1, 3, 4, 6, 5, 7]),
        (GameplayTurnOption::UDMirror, 5) => Some(&[1, 0, 2, 4, 3]),
        (GameplayTurnOption::UDMirror, 10) => Some(&[1, 0, 2, 4, 3, 6, 5, 7, 9, 8]),
        (GameplayTurnOption::Left, 4) => Some(&[2, 0, 3, 1]),
        (GameplayTurnOption::Left, 8) => Some(&[2, 0, 3, 1, 6, 4, 7, 5]),
        (GameplayTurnOption::Left, 5) => Some(&[1, 3, 2, 4, 0]),
        (GameplayTurnOption::Left, 10) => Some(&[8, 9, 7, 5, 6, 3, 4, 2, 0, 1]),
        (GameplayTurnOption::Right, 4) => Some(&[1, 3, 0, 2]),
        (GameplayTurnOption::Right, 8) => Some(&[1, 3, 0, 2, 5, 7, 4, 6]),
        (GameplayTurnOption::Right, 5) => Some(&[4, 0, 2, 1, 3]),
        (GameplayTurnOption::Right, 10) => Some(&[8, 9, 7, 5, 6, 3, 4, 2, 0, 1]),
        (GameplayTurnOption::Shuffle, _) => {
            for (value, source) in out[..cols].iter_mut().zip(0..cols) {
                *value = source;
            }
            let mut attempt_seed = seed as u32;
            loop {
                let mut rng = TurnRng::new(u64::from(attempt_seed));
                rng.shuffle(&mut out[..cols]);
                if cols <= 1 || out[..cols].iter().copied().ne(0..cols) {
                    return Some(cols);
                }
                attempt_seed = attempt_seed.wrapping_add(1);
            }
        }
        _ => None,
    };
    let fixed = fixed?;
    out[..fixed.len()].copy_from_slice(fixed);
    Some(fixed.len())
}

fn turn_take_from(turn: GameplayTurnOption, cols: usize, seed: u64) -> Option<Vec<usize>> {
    if cols <= MAX_COLS {
        let mut out = [0usize; MAX_COLS];
        let len = fill_turn_take_from(turn, cols, seed, &mut out)?;
        return Some(out[..len].to_vec());
    }
    match turn {
        GameplayTurnOption::Mirror => Some((0..cols).rev().collect()),
        GameplayTurnOption::Shuffle => {
            let mut out = (0..cols).collect::<Vec<_>>();
            let mut attempt_seed = seed as u32;
            loop {
                let mut rng = TurnRng::new(u64::from(attempt_seed));
                rng.shuffle(&mut out);
                if out.iter().copied().ne(0..cols) {
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
    if cols <= MAX_COLS {
        let mut take_from = [0usize; MAX_COLS];
        let Some(len) = fill_turn_take_from(turn, cols, seed, &mut take_from) else {
            return;
        };
        if len != cols {
            return;
        }
        let mut old_to_new = [0usize; MAX_COLS];
        for (new_col, &old_col) in take_from[..len].iter().enumerate() {
            if old_col < cols {
                old_to_new[old_col] = new_col;
            }
        }
        let (start, end) = note_range;
        for note in &mut notes[start..end] {
            if note.column < col_offset {
                continue;
            }
            let local = note.column - col_offset;
            if local < cols {
                note.column = col_offset + old_to_new[local];
            }
        }
        return;
    }

    let Some(take_from) = turn_take_from(turn, cols, seed) else {
        return;
    };
    let mut old_to_new = vec![0usize; cols];
    for (new_col, &old_col) in take_from.iter().enumerate() {
        old_to_new[old_col] = new_col;
    }
    let (start, end) = note_range;
    for note in &mut notes[start..end] {
        if note.column < col_offset {
            continue;
        }
        let local = note.column - col_offset;
        if local < cols {
            note.column = col_offset + old_to_new[local];
        }
    }
}

#[cfg(any(test, feature = "bench-support"))]
pub fn apply_turn_permutation_reference(
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
    for note in &mut notes[start..end] {
        if note.column < col_offset {
            continue;
        }
        let local = note.column - col_offset;
        if local < cols {
            note.column = col_offset + old_to_new[local];
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
    let (start, end) = note_range;
    debug_assert!(start <= end && end <= notes.len());
    debug_assert!(notes_row_sorted(&notes[start..end]));
    let mut row_cursor = start;
    let mut rng = TurnRng::new(seed);
    let mut hold_end_row: [Option<usize>; MAX_COLS] = [None; MAX_COLS];

    while let Some(row_grid) = next_row_grid(notes, &mut row_cursor, end, col_offset, cols) {
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
    let (start, end) = note_range;
    debug_assert!(start <= end && end <= notes.len());
    debug_assert!(notes_row_sorted(&notes[start..end]));
    let mut row_cursor = start;
    let mut rng = TurnRng::new(seed);
    let mut hold_end_row: [Option<usize>; MAX_COLS] = [None; MAX_COLS];

    while let Some(row_grid) = next_row_grid(notes, &mut row_cursor, end, col_offset, cols) {
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

#[cfg(any(test, feature = "bench-support"))]
pub fn apply_hyper_shuffle_reference(
    notes: &mut [Note],
    note_range: (usize, usize),
    col_offset: usize,
    cols: usize,
    seed: u64,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    let row_grids = build_row_grids_reference(notes, note_range, col_offset, cols);
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
            if hold_end_row[col].is_some() || idx == usize::MAX {
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
                .map(|hold| hold.end_row_index)
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
