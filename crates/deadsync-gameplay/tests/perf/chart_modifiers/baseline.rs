// Frozen from 76d58fdc9 (0.5.1167); shared helpers are unchanged.
use super::*;

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
        apply_insert_intelligent_taps_fallback(
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
        notes.retain(|note| beat_to_note_row(note.beat) % ROWS_PER_BEAT == 0);
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

pub fn apply_chart_attack_window(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    row_bounds: (usize, usize),
    mods: ParsedAttackMods,
    turn_seed: u64,
) {
    if notes.is_empty() || row_bounds.1 < row_bounds.0 || !mods.has_chart_effect() {
        return;
    }
    if prepare_chart_attack_order(notes) {
        apply_chart_attack_window_sorted(
            notes,
            timing_player,
            col_offset,
            cols,
            player,
            row_bounds,
            mods,
            turn_seed,
        );
    } else {
        apply_chart_attack_window_fallback(
            notes,
            timing_player,
            col_offset,
            cols,
            player,
            row_bounds,
            mods,
            turn_seed,
        );
    }
}

fn apply_chart_attack_window_sorted(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    row_bounds: (usize, usize),
    mods: ParsedAttackMods,
    turn_seed: u64,
) {
    let mut scratch = Vec::new();
    apply_chart_attack_window_sorted_with_scratch(
        notes,
        timing_player,
        col_offset,
        cols,
        player,
        row_bounds,
        mods,
        turn_seed,
        &mut scratch,
    );
}

fn apply_chart_attack_window_sorted_with_scratch(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    row_bounds: (usize, usize),
    mods: ParsedAttackMods,
    turn_seed: u64,
    in_range: &mut Vec<Note>,
) {
    let (start_row, end_row) = row_bounds;
    if notes.is_empty() || end_row < start_row || !mods.has_chart_effect() {
        return;
    }
    debug_assert!(chart_attack_notes_sorted(notes));

    let note_range = chart_attack_note_range(notes, start_row, end_row);
    if note_range.is_empty() {
        return;
    }
    if mods.insert_mask == 0 && mods.remove_mask == 0 && mods.holds_mask == 0 {
        apply_attack_turn_mod(
            &mut notes[note_range.clone()],
            col_offset,
            cols,
            mods.turn_option,
            turn_seed,
            player,
        );
        sort_attack_row_columns(&mut notes[note_range]);
        debug_assert!(chart_attack_notes_sorted(notes));
        return;
    }
    let insert_at = note_range.start;
    debug_assert!(in_range.is_empty());
    in_range.reserve(note_range.len());
    in_range.extend(notes.drain(note_range));
    apply_uncommon_masks_with_masks(
        in_range,
        mods.insert_mask,
        mods.remove_mask,
        mods.holds_mask,
        timing_player,
        col_offset,
        cols,
        notes,
        Some(row_bounds),
        player,
    );
    apply_attack_turn_mod(
        in_range,
        col_offset,
        cols,
        mods.turn_option,
        turn_seed,
        player,
    );
    if mods.turn_option != GameplayTurnOption::None {
        sort_attack_row_columns(in_range);
    }

    let mut insert_at = insert_at;
    if let (Some(first), Some(last)) = (in_range.first(), in_range.last()) {
        let first_key = (first.row_index, first.column);
        let last_key = (last.row_index, last.column);
        let merge_start = notes.partition_point(|note| (note.row_index, note.column) < first_key);
        let merge_end = notes.partition_point(|note| (note.row_index, note.column) <= last_key);
        if merge_start < merge_end {
            in_range.extend(notes.drain(merge_start..merge_end));
            sort_player_notes(in_range);
        }
        insert_at = merge_start;
    }

    // The drain retained the chart's allocation. Splice shifts only the tail
    // following the attacked rows (plus any rows reached by an insert mod) and
    // grows solely when inserted notes exceed the existing capacity.
    drop(notes.splice(insert_at..insert_at, in_range.drain(..)));
    debug_assert!(in_range.is_empty());
    debug_assert!(chart_attack_notes_sorted(notes));
}

fn apply_chart_attack_window_fallback(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    row_bounds: (usize, usize),
    mods: ParsedAttackMods,
    turn_seed: u64,
) {
    let (start_row, end_row) = row_bounds;
    if notes.is_empty() || end_row < start_row || !mods.has_chart_effect() {
        return;
    }
    let mut in_range = Vec::with_capacity(notes.len());
    let mut out_range = Vec::with_capacity(notes.len());
    for note in notes.drain(..) {
        if note.row_index >= start_row && note.row_index <= end_row {
            in_range.push(note);
        } else {
            out_range.push(note);
        }
    }
    if in_range.is_empty() {
        *notes = out_range;
        return;
    }

    apply_uncommon_masks_with_masks(
        &mut in_range,
        mods.insert_mask,
        mods.remove_mask,
        mods.holds_mask,
        timing_player,
        col_offset,
        cols,
        &out_range,
        Some(row_bounds),
        player,
    );
    apply_attack_turn_mod(
        &mut in_range,
        col_offset,
        cols,
        mods.turn_option,
        turn_seed,
        player,
    );

    out_range.extend(in_range);
    *notes = out_range;
    sort_player_notes(notes);
}
