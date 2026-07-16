#[inline(always)]
pub fn mine_window_bounds_ns(
    mine_times_ns: &[SongTimeNs],
    start_t_ns: SongTimeNs,
    end_t_ns: SongTimeNs,
) -> (usize, usize) {
    (
        mine_times_ns.partition_point(|&t| t < start_t_ns),
        mine_times_ns.partition_point(|&t| t <= end_t_ns),
    )
}

#[inline(always)]
pub fn lane_note_window_bounds_ns(
    note_indices: &[usize],
    note_times_ns: &[SongTimeNs],
    start_t_ns: SongTimeNs,
    end_t_ns: SongTimeNs,
) -> (usize, usize) {
    (
        note_indices.partition_point(|&note_index| note_times_ns[note_index] < start_t_ns),
        note_indices.partition_point(|&note_index| note_times_ns[note_index] <= end_t_ns),
    )
}

#[inline(always)]
pub fn lane_note_window_bounds_rows(
    note_indices: &[usize],
    notes: &[Note],
    start_row: usize,
    end_row: usize,
) -> (usize, usize) {
    (
        note_indices.partition_point(|&note_index| notes[note_index].row_index < start_row),
        note_indices.partition_point(|&note_index| notes[note_index].row_index < end_row),
    )
}

#[inline(always)]
pub fn timing_row_nearest(timing: &TimingData, beat: f32) -> usize {
    timing.get_row_for_beat(beat).unwrap_or(0)
}

#[inline(always)]
pub fn step_search_row_bounds(
    timing: &TimingData,
    current_time_ns: SongTimeNs,
    current_row_index: usize,
) -> (usize, usize) {
    let forward_time_ns = song_time_ns_add_seconds(current_time_ns, STEP_SEARCH_DISTANCE_SECONDS);
    let backward_time_ns = song_time_ns_add_seconds(current_time_ns, -STEP_SEARCH_DISTANCE_SECONDS);
    let forward_row = timing_row_nearest(timing, timing.get_beat_for_time_ns(forward_time_ns));
    let backward_row = timing_row_nearest(timing, timing.get_beat_for_time_ns(backward_time_ns));
    let step_rows = forward_row
        .saturating_sub(current_row_index)
        .max(current_row_index.saturating_sub(backward_row))
        .saturating_add(ROWS_PER_BEAT.max(1) as usize);
    (
        current_row_index.saturating_sub(step_rows),
        current_row_index.saturating_add(step_rows),
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaneNoteSearch {
    pub current_row_index: usize,
    pub search_start_row: usize,
    pub search_end_row: usize,
    pub search_start_idx: usize,
    pub search_end_idx: usize,
    pub candidate: Option<(usize, SongTimeNs)>,
}

pub fn closest_lane_note_search(
    note_indices: &[usize],
    notes: &[Note],
    note_times_ns: &[SongTimeNs],
    timing: &TimingData,
    current_time_ns: SongTimeNs,
) -> LaneNoteSearch {
    let rows = lane_search_rows_for_timing(timing, current_time_ns);
    closest_lane_note_search_with_rows(
        note_indices,
        notes,
        note_times_ns,
        timing,
        current_time_ns,
        rows,
    )
}

fn closest_lane_note_search_with_rows(
    note_indices: &[usize],
    notes: &[Note],
    note_times_ns: &[SongTimeNs],
    timing: &TimingData,
    current_time_ns: SongTimeNs,
    rows: LaneSearchRows,
) -> LaneNoteSearch {
    let current_row_index = rows.current;
    let search_start_row = rows.start;
    let search_end_row = rows.end;
    let (search_start_idx, search_end_idx) =
        lane_note_window_bounds_rows(note_indices, notes, search_start_row, search_end_row);
    let candidate = closest_lane_note_ns(
        note_indices,
        notes,
        note_times_ns,
        timing,
        current_time_ns,
        current_row_index,
        search_start_idx,
        search_end_idx,
    );

    LaneNoteSearch {
        current_row_index,
        search_start_row,
        search_end_row,
        search_start_idx,
        search_end_idx,
        candidate,
    }
}

#[inline(always)]
pub fn closest_lane_note_ns(
    note_indices: &[usize],
    notes: &[Note],
    note_times_ns: &[SongTimeNs],
    timing: &TimingData,
    current_time_ns: SongTimeNs,
    current_row_index: usize,
    search_start_idx: usize,
    search_end_idx: usize,
) -> Option<(usize, SongTimeNs)> {
    let mut best: Option<(usize, SongTimeNs)> = None;
    let mut best_row_distance = usize::MAX;
    let mut best_row_index = 0usize;
    for &note_index in &note_indices[search_start_idx..search_end_idx] {
        let note = &notes[note_index];
        let mine_already_judged =
            matches!(note.note_type, NoteType::Mine) && note.mine_result.is_some();
        let fake_note_blocks = note.is_fake && timing.is_judgable_at_beat(note.beat);
        if note.result.is_some() || mine_already_judged || !(note.can_be_judged || fake_note_blocks)
        {
            continue;
        }
        let row_distance = current_row_index.abs_diff(note.row_index);
        let signed_err_music = current_time_ns as i128 - note_times_ns[note_index] as i128;
        // Match ITGmania Player::GetClosestNote: choose by row proximity, and
        // break exact ties toward the later row.
        match best {
            Some(_) if row_distance > best_row_distance => {}
            Some(_) if row_distance == best_row_distance && note.row_index <= best_row_index => {}
            _ => {
                best = Some((note_index, signed_err_music as SongTimeNs));
                best_row_distance = row_distance;
                best_row_index = note.row_index;
            }
        }
    }
    best
}

#[inline(always)]
pub fn crossed_mine_bounds_ns(
    mine_times_ns: &[SongTimeNs],
    prev_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
) -> (usize, usize) {
    (
        mine_times_ns.partition_point(|&t| t <= prev_time_ns),
        mine_times_ns.partition_point(|&t| t <= current_time_ns),
    )
}

#[inline(always)]
pub fn crossed_held_mine_can_hit(note: &Note, column: usize) -> bool {
    matches!(note.note_type, NoteType::Mine)
        && note.can_be_judged
        && note.mine_result.is_none()
        && !note.is_fake
        && note.column == column
}

#[inline(always)]
pub fn mine_hit_offset_in_window(
    time_error_music_ns: SongTimeNs,
    mine_window_music_ns: SongTimeNs,
) -> bool {
    i128::from(time_error_music_ns).abs() <= i128::from(mine_window_music_ns)
}

#[inline(always)]
pub fn mine_can_be_hit(note: &Note) -> bool {
    note.mine_result.is_none() && !note.is_fake && note.can_be_judged
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MineHitMark {
    pub row_index: usize,
    pub column: usize,
    pub beat: f32,
    pub note_time_ns: SongTimeNs,
    pub hit_time_ns: SongTimeNs,
    pub time_error_ms: f32,
}

#[inline(always)]
pub fn apply_mine_hit_result(
    note: &mut Note,
    time_error_music_ns: SongTimeNs,
    mine_window_music_ns: SongTimeNs,
) -> bool {
    if !mine_hit_offset_in_window(time_error_music_ns, mine_window_music_ns)
        || !mine_can_be_hit(note)
    {
        return false;
    }
    note.mine_result = Some(MineResult::Hit);
    true
}

#[inline(always)]
pub fn mark_mine_hit_candidate(
    note: &mut Note,
    note_time_ns: SongTimeNs,
    time_error_music_ns: SongTimeNs,
    mine_window_music_ns: SongTimeNs,
    music_rate: f32,
) -> Option<MineHitMark> {
    if !apply_mine_hit_result(note, time_error_music_ns, mine_window_music_ns) {
        return None;
    }
    Some(MineHitMark {
        row_index: note.row_index,
        column: note.column,
        beat: note.beat,
        note_time_ns,
        hit_time_ns: note_time_ns.saturating_add(time_error_music_ns),
        time_error_ms: judgment::judgment_time_error_ms_from_music_ns(
            time_error_music_ns,
            music_rate,
        ),
    })
}

#[inline(always)]
pub fn pending_mine_hit_ready(note: &Note) -> bool {
    note.mine_result == Some(MineResult::Hit) && !note.is_fake && note.can_be_judged
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingMineHitEvent {
    pub note_index: usize,
    pub column: usize,
    pub player: usize,
}

#[inline(always)]
pub fn pending_mine_hit_event(
    notes: &[Note],
    note_index: usize,
    num_players: usize,
    cols_per_player: usize,
) -> Option<PendingMineHitEvent> {
    let note = notes.get(note_index)?;
    if !pending_mine_hit_ready(note) {
        return None;
    }
    Some(PendingMineHitEvent {
        note_index,
        column: note.column,
        player: player_index_for_column(num_players, cols_per_player, note.column),
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PendingMineHitCollectionUpdate {
    pub next_cursor: usize,
    pub event_count: usize,
    pub stopped: bool,
}

pub fn collect_pending_mine_hit_events(
    notes: &[Note],
    pending_indices: &[usize],
    cursor: usize,
    num_players: usize,
    cols_per_player: usize,
    events: &mut [Option<PendingMineHitEvent>],
) -> PendingMineHitCollectionUpdate {
    let mut cursor = cursor.min(pending_indices.len());
    let mut event_count = 0usize;
    while cursor < pending_indices.len() {
        if event_count >= events.len() {
            return PendingMineHitCollectionUpdate {
                next_cursor: cursor,
                event_count,
                stopped: true,
            };
        }
        let note_index = pending_indices[cursor];
        cursor += 1;
        let Some(event) = pending_mine_hit_event(notes, note_index, num_players, cols_per_player)
        else {
            continue;
        };
        events[event_count] = Some(event);
        event_count += 1;
    }
    PendingMineHitCollectionUpdate {
        next_cursor: cursor,
        event_count,
        stopped: false,
    }
}

pub fn mark_crossed_held_mine_candidates(
    notes: &mut [Note],
    mine_note_ix: &[usize],
    mine_note_time_ns: &[SongTimeNs],
    column: usize,
    prev_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
    mine_window_music_ns: SongTimeNs,
    music_rate: f32,
    mut on_mark: impl FnMut(usize, MineHitMark),
) -> bool {
    if song_time_ns_invalid(prev_time_ns)
        || song_time_ns_invalid(current_time_ns)
        || current_time_ns <= prev_time_ns
    {
        return false;
    }

    let (start_idx, end_idx) =
        crossed_mine_bounds_ns(mine_note_time_ns, prev_time_ns, current_time_ns);
    let mut hit_any = false;
    for i in start_idx..end_idx {
        let Some(&note_index) = mine_note_ix.get(i) else {
            continue;
        };
        let Some(note) = notes.get_mut(note_index) else {
            continue;
        };
        if !crossed_held_mine_can_hit(note, column) {
            continue;
        }
        let Some(&note_time_ns) = mine_note_time_ns.get(i) else {
            continue;
        };
        let Some(mark) =
            mark_mine_hit_candidate(note, note_time_ns, 0, mine_window_music_ns, music_rate)
        else {
            continue;
        };
        on_mark(note_index, mark);
        hit_any = true;
    }
    hit_any
}

#[inline(always)]
pub fn mine_avoid_cursor_end(
    notes: &[Note],
    mine_note_ix: &[usize],
    mine_cursor: usize,
    cutoff_row: usize,
) -> usize {
    let mut mine_end = mine_cursor.min(mine_note_ix.len());
    while mine_end < mine_note_ix.len() {
        if notes[mine_note_ix[mine_end]].row_index >= cutoff_row {
            break;
        }
        mine_end += 1;
    }
    mine_end
}

#[inline(always)]
pub fn mine_can_be_avoided(note: &Note) -> bool {
    note.can_be_judged && note.mine_result.is_none()
}

#[inline(always)]
pub fn apply_mine_avoid_result(note: &mut Note) -> bool {
    if !mine_can_be_avoided(note) {
        return false;
    }
    note.mine_result = Some(MineResult::Avoided);
    true
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MineAvoidedEvent {
    pub note_index: usize,
    pub row_index: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MineAvoidancePlayerUpdate {
    pub mine_end: usize,
    pub next_mine_avoid_cursor: usize,
    pub avoided_count: u32,
    pub last_avoided: Option<MineAvoidedEvent>,
}

pub fn apply_time_based_mine_avoidance_for_player(
    notes: &mut [Note],
    mine_note_ix: &[usize],
    mine_cursor: usize,
    cutoff_row: usize,
    note_range: (usize, usize),
) -> MineAvoidancePlayerUpdate {
    let mine_end = mine_avoid_cursor_end(notes, mine_note_ix, mine_cursor, cutoff_row);
    let mut avoided_count = 0u32;
    let mut last_avoided = None;
    for &note_idx in &mine_note_ix[mine_cursor.min(mine_note_ix.len())..mine_end] {
        let note = &mut notes[note_idx];
        if apply_mine_avoid_result(note) {
            avoided_count = avoided_count.saturating_add(1);
            last_avoided = Some(MineAvoidedEvent {
                note_index: note_idx,
                row_index: note.row_index,
                column: note.column,
            });
        }
    }

    let next_mine_avoid_cursor = if mine_end < mine_note_ix.len() {
        mine_note_ix[mine_end]
    } else {
        note_range.1.min(notes.len())
    };

    MineAvoidancePlayerUpdate {
        mine_end,
        next_mine_avoid_cursor,
        avoided_count,
        last_avoided,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MineAvoidancePlayersUpdate {
    pub players_scanned: usize,
    pub updates: [MineAvoidancePlayerUpdate; MAX_PLAYERS],
}

pub fn apply_time_based_mine_avoidance_for_players(
    notes: &mut [Note],
    mine_note_ix: &[Vec<usize>],
    mine_cursors: &[usize],
    cutoff_rows: &[usize],
    note_ranges: &[(usize, usize)],
    num_players: usize,
) -> MineAvoidancePlayersUpdate {
    let active_players = num_players.min(MAX_PLAYERS);
    let mut updates = [MineAvoidancePlayerUpdate::default(); MAX_PLAYERS];
    for player in 0..active_players {
        let mine_ix = mine_note_ix
            .get(player)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mine_cursor = mine_cursors.get(player).copied().unwrap_or(0);
        let cutoff_row = cutoff_rows.get(player).copied().unwrap_or(0);
        let note_range = player_note_range_for_ranges(note_ranges, active_players, player);
        updates[player] = apply_time_based_mine_avoidance_for_player(
            notes,
            mine_ix,
            mine_cursor,
            cutoff_row,
            note_range,
        );
    }
    MineAvoidancePlayersUpdate {
        players_scanned: active_players,
        updates,
    }
}

#[inline(always)]
pub fn completed_mine_can_be_avoided(note: &Note) -> bool {
    matches!(note.note_type, NoteType::Mine)
        && note.can_be_judged
        && !note.is_fake
        && note.mine_result.is_none()
}

#[inline(always)]
pub fn apply_completed_mine_avoid_result(note: &mut Note) -> bool {
    if !completed_mine_can_be_avoided(note) {
        return false;
    }
    note.mine_result = Some(MineResult::Avoided);
    true
}

pub fn finalize_completed_mine_avoidance_for_player(
    notes: &mut [Note],
    note_range: (usize, usize),
    mines_total: u32,
    mines_hit: u32,
) -> u32 {
    let end = note_range.1.min(notes.len());
    let start = note_range.0.min(end);
    for note in &mut notes[start..end] {
        apply_completed_mine_avoid_result(note);
    }

    mines_total.saturating_sub(mines_hit.min(mines_total))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompletedMineFinalizationUpdate {
    pub players_finalized: usize,
    pub mines_avoided: [u32; MAX_PLAYERS],
}

pub fn finalize_completed_mine_avoidance_for_players(
    notes: &mut [Note],
    note_ranges: &[(usize, usize)],
    mines_total: &[u32],
    mines_hit: &[u32],
    num_players: usize,
) -> CompletedMineFinalizationUpdate {
    let active_players = num_players.min(MAX_PLAYERS);
    let mut mines_avoided = [0; MAX_PLAYERS];
    for player in 0..active_players {
        mines_avoided[player] = finalize_completed_mine_avoidance_for_player(
            notes,
            player_note_range_for_ranges(note_ranges, active_players, player),
            mines_total.get(player).copied().unwrap_or(0),
            mines_hit.get(player).copied().unwrap_or(0),
        );
    }
    CompletedMineFinalizationUpdate {
        players_finalized: active_players,
        mines_avoided,
    }
}

#[inline(always)]
pub fn crossed_mine_held_start_time(
    now_down: bool,
    was_down: bool,
    pressed_since_ns: Option<SongTimeNs>,
    previous_music_time_ns: SongTimeNs,
    current_music_time_ns: SongTimeNs,
) -> Option<SongTimeNs> {
    if !now_down
        || song_time_ns_invalid(previous_music_time_ns)
        || song_time_ns_invalid(current_music_time_ns)
        || current_music_time_ns <= previous_music_time_ns
    {
        return None;
    }
    if was_down {
        return Some(previous_music_time_ns);
    }
    let pressed_since_ns = pressed_since_ns?;
    if song_time_ns_invalid(pressed_since_ns) || pressed_since_ns >= current_music_time_ns {
        return None;
    }
    Some(pressed_since_ns.max(previous_music_time_ns))
}

#[inline(always)]
pub const fn note_tracks_held_miss(note_type: NoteType) -> bool {
    matches!(note_type, NoteType::Tap | NoteType::Hold | NoteType::Roll)
}

pub fn track_held_miss_window_for_player(
    notes: &[Note],
    note_times_ns: &[SongTimeNs],
    tap_miss_held_window: &mut [bool],
    note_range: (usize, usize),
    col_range: (usize, usize),
    next_tap_miss_cursor: usize,
    inputs: &[bool; MAX_COLS],
    music_time_ns: SongTimeNs,
    largest_window_ns: SongTimeNs,
) {
    if largest_window_ns <= 0 {
        return;
    }
    let note_end = note_range
        .1
        .min(notes.len())
        .min(note_times_ns.len())
        .min(tap_miss_held_window.len());
    let mut cursor = next_tap_miss_cursor.max(note_range.0.min(note_end));
    let col_start = col_range.0.min(MAX_COLS);
    let col_end = col_range.1.min(MAX_COLS).max(col_start);
    let future_cutoff_time_ns = music_time_ns.saturating_add(largest_window_ns);
    let mut seen_tracks = [false; MAX_COLS];

    while cursor < note_end {
        let note_time_ns = note_times_ns[cursor];
        if note_time_ns > future_cutoff_time_ns {
            break;
        }
        let note = &notes[cursor];
        if !note.can_be_judged
            || note.result.is_some()
            || note.column < col_start
            || note.column >= col_end
            || !note_tracks_held_miss(note.note_type)
        {
            cursor += 1;
            continue;
        }
        let local_track = note.column - col_start;
        if seen_tracks[local_track] {
            cursor += 1;
            continue;
        }
        let offset_ns = (note_time_ns as i128 - music_time_ns as i128).unsigned_abs();
        if offset_ns > largest_window_ns as u128 {
            cursor += 1;
            continue;
        }
        seen_tracks[local_track] = true;
        if inputs[note.column] {
            tap_miss_held_window[cursor] = true;
        }
        cursor += 1;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeldMissWindowUpdate {
    pub players_scanned: usize,
}

pub fn track_held_miss_windows_for_players(
    notes: &[Note],
    note_times_ns: &[SongTimeNs],
    tap_miss_held_window: &mut [bool],
    note_ranges: &[(usize, usize)],
    next_tap_miss_cursor: &[usize],
    largest_windows_ns: &[SongTimeNs],
    num_players: usize,
    cols_per_player: usize,
    inputs: &[bool; MAX_COLS],
    music_time_ns: SongTimeNs,
) -> HeldMissWindowUpdate {
    let active_players = num_players.min(MAX_PLAYERS);
    let mut players_scanned = 0usize;
    for player in 0..active_players {
        let largest_window_ns = largest_windows_ns.get(player).copied().unwrap_or(0);
        if largest_window_ns <= 0 {
            continue;
        }
        let note_range = player_note_range_for_ranges(note_ranges, active_players, player);
        let col_range = player_column_range(cols_per_player, player);
        let next_cursor = next_tap_miss_cursor
            .get(player)
            .copied()
            .unwrap_or(note_range.0);
        track_held_miss_window_for_player(
            notes,
            note_times_ns,
            tap_miss_held_window,
            note_range,
            col_range,
            next_cursor,
            inputs,
            music_time_ns,
            largest_window_ns,
        );
        players_scanned += 1;
    }
    HeldMissWindowUpdate { players_scanned }
}

#[inline(always)]
pub fn collect_edge_judge_indices(
    row_note_count: usize,
    lead_note_index: usize,
) -> Option<([usize; MAX_COLS], usize)> {
    if row_note_count == 0 {
        return None;
    }
    let mut judge_indices = [usize::MAX; MAX_COLS];
    judge_indices[0] = lead_note_index;
    Some((judge_indices, 1))
}
