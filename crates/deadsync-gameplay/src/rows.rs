#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FinalizedRowOutcome {
    pub final_grade: JudgeGrade,
}

#[derive(Clone, Debug)]
pub struct RowEntry {
    pub row_index: usize,
    pub time_ns: SongTimeNs,
    // Non-mine, non-fake, judgable notes on this row.
    pub nonmine_note_indices: [usize; MAX_COLS],
    pub nonmine_note_count: u8,
    pub rescore_track_count: u8,
    pub unresolved_count: u8,
    pub unresolved_nonlift_count: u8,
    pub had_provisional_early_hit: bool,
    pub final_outcome: Option<FinalizedRowOutcome>,
}

impl RowEntry {
    #[inline(always)]
    pub fn note_indices(&self) -> &[usize] {
        &self.nonmine_note_indices[..usize::from(self.nonmine_note_count)]
    }
}

pub fn score_rows_finalized_for_players(
    row_entries: &[RowEntry],
    row_entry_ranges: &[(usize, usize); MAX_PLAYERS],
    num_players: usize,
) -> bool {
    let players = num_players.min(MAX_PLAYERS);
    row_entry_ranges[..players].iter().all(|&(start, end)| {
        let end = end.min(row_entries.len());
        let start = start.min(end);
        row_entries[start..end]
            .iter()
            .all(|row| row.final_outcome.is_some())
    })
}

#[inline(always)]
pub fn first_time_index_at_or_after(
    times_ns: &[SongTimeNs],
    range: (usize, usize),
    time_ns: SongTimeNs,
) -> usize {
    let end = range.1.min(times_ns.len());
    let start = range.0.min(end);
    start + times_ns[start..end].partition_point(|&t| t < time_ns)
}

#[inline(always)]
pub fn first_row_entry_index_at_or_after_time(
    row_entries: &[RowEntry],
    range: (usize, usize),
    time_ns: SongTimeNs,
) -> usize {
    let end = range.1.min(row_entries.len());
    let start = range.0.min(end);
    start + row_entries[start..end].partition_point(|row| row.time_ns < time_ns)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PracticePlayerCursors {
    pub note_cursor: usize,
    pub row_cursor: usize,
    pub mine_ix_cursor: usize,
    pub mine_avoid_cursor: usize,
}

pub fn practice_player_cursors(
    note_time_cache_ns: &[SongTimeNs],
    note_range: (usize, usize),
    row_entries: &[RowEntry],
    row_range: (usize, usize),
    mine_note_time_ns: &[SongTimeNs],
    mine_note_ix: &[usize],
    judge_start_ns: SongTimeNs,
) -> PracticePlayerCursors {
    let note_cursor = first_time_index_at_or_after(note_time_cache_ns, note_range, judge_start_ns);
    let row_cursor = first_row_entry_index_at_or_after_time(row_entries, row_range, judge_start_ns);
    let mine_ix_cursor = mine_note_time_ns.partition_point(|&t| t < judge_start_ns);
    let note_end = note_range.1.min(note_time_cache_ns.len());
    let mine_avoid_cursor = mine_note_ix
        .get(mine_ix_cursor)
        .copied()
        .unwrap_or(note_end);

    PracticePlayerCursors {
        note_cursor,
        row_cursor,
        mine_ix_cursor,
        mine_avoid_cursor,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PracticePlaybackCursors {
    pub note_cursor: [usize; MAX_PLAYERS],
    pub row_cursor: [usize; MAX_PLAYERS],
    pub mine_ix_cursor: [usize; MAX_PLAYERS],
    pub mine_avoid_cursor: [usize; MAX_PLAYERS],
}

pub fn practice_cursors_for_players(
    note_time_cache_ns: &[SongTimeNs],
    note_ranges: &[(usize, usize)],
    row_entries: &[RowEntry],
    row_entry_ranges: &[(usize, usize)],
    mine_note_time_ns: [&[SongTimeNs]; MAX_PLAYERS],
    mine_note_ix: [&[usize]; MAX_PLAYERS],
    num_players: usize,
    judge_start_ns: SongTimeNs,
) -> PracticePlaybackCursors {
    let active_players = num_players.min(MAX_PLAYERS);
    let mut cursors = PracticePlaybackCursors::default();
    for player in 0..active_players {
        let player_cursors = practice_player_cursors(
            note_time_cache_ns,
            player_note_range_for_ranges(note_ranges, active_players, player),
            row_entries,
            row_entry_ranges.get(player).copied().unwrap_or((0, 0)),
            mine_note_time_ns[player],
            mine_note_ix[player],
            judge_start_ns,
        );
        cursors.note_cursor[player] = player_cursors.note_cursor;
        cursors.row_cursor[player] = player_cursors.row_cursor;
        cursors.mine_ix_cursor[player] = player_cursors.mine_ix_cursor;
        cursors.mine_avoid_cursor[player] = player_cursors.mine_avoid_cursor;
    }
    cursors
}

#[inline(always)]
pub fn count_rescore_tracks_on_row(row_entry: &RowEntry) -> usize {
    usize::from(row_entry.rescore_track_count)
}

pub fn build_row_entry(
    row_index: usize,
    nonmine_note_indices: [usize; MAX_COLS],
    nonmine_note_count: u8,
    notes: &[Note],
    note_time_cache_ns: &[SongTimeNs],
) -> RowEntry {
    debug_assert!(nonmine_note_count != 0);
    let time_ns = note_time_cache_ns[nonmine_note_indices[0]];
    let mut rescore_track_count = 0u8;
    let mut unresolved_count = 0u8;
    let mut unresolved_nonlift_count = 0u8;
    let mut had_provisional_early_hit = false;
    for &note_index in &nonmine_note_indices[..usize::from(nonmine_note_count)] {
        let note = &notes[note_index];
        if counts_for_early_rescore(note.note_type) {
            rescore_track_count = rescore_track_count.saturating_add(1);
        }
        if note.result.is_none() {
            unresolved_count = unresolved_count.saturating_add(1);
            if note.note_type != NoteType::Lift {
                unresolved_nonlift_count = unresolved_nonlift_count.saturating_add(1);
            }
        }
        had_provisional_early_hit |= note.early_result.is_some();
    }
    RowEntry {
        row_index,
        time_ns,
        nonmine_note_indices,
        nonmine_note_count,
        rescore_track_count,
        unresolved_count,
        unresolved_nonlift_count,
        had_provisional_early_hit,
        final_outcome: None,
    }
}

pub fn reset_practice_notes_and_rows(
    notes: &mut [Note],
    row_entries: &mut [RowEntry],
    note_time_cache_ns: &[SongTimeNs],
) {
    for note in notes.iter_mut() {
        note.result = None;
        note.early_result = None;
        note.mine_result = None;
        if let Some(hold) = note.hold.as_mut() {
            hold.result = None;
            hold.life = MAX_HOLD_LIFE;
            hold.let_go_started_at = None;
            hold.let_go_starting_life = MAX_HOLD_LIFE;
            hold.last_held_row_index = note.row_index;
            hold.last_held_beat = note.beat;
        }
    }

    for row_entry in row_entries {
        *row_entry = build_row_entry(
            row_entry.row_index,
            row_entry.nonmine_note_indices,
            row_entry.nonmine_note_count,
            notes,
            note_time_cache_ns,
        );
    }
}

pub fn refresh_timing_caches_for_offset_change(
    notes: &[Note],
    timing_players: &[&TimingData; MAX_PLAYERS],
    num_players: usize,
    cols_per_player: usize,
    note_time_cache_ns: &mut [SongTimeNs],
    hold_end_time_cache_ns: &mut [Option<SongTimeNs>],
    row_entries: &mut [RowEntry],
    mine_note_ix: &[Vec<usize>; MAX_PLAYERS],
    mine_note_time_ns: &mut [Vec<SongTimeNs>; MAX_PLAYERS],
) {
    for (time_ns, note) in note_time_cache_ns.iter_mut().zip(notes) {
        let player = player_index_for_column(num_players, cols_per_player, note.column);
        *time_ns = timing_players[player].get_time_for_beat_ns(note.beat);
    }
    for (time_opt_ns, note) in hold_end_time_cache_ns.iter_mut().zip(notes) {
        let player = player_index_for_column(num_players, cols_per_player, note.column);
        *time_opt_ns = note
            .hold
            .as_ref()
            .map(|hold| timing_players[player].get_time_for_beat_ns(hold.end_beat));
    }
    for row_entry in row_entries {
        row_entry.time_ns = note_time_cache_ns[row_entry.note_indices()[0]];
    }
    for player in 0..num_players.min(MAX_PLAYERS) {
        let mine_note_time_ns = &mut mine_note_time_ns[player];
        mine_note_time_ns.clear();
        mine_note_time_ns.extend(
            mine_note_ix[player]
                .iter()
                .map(|&note_index| note_time_cache_ns[note_index]),
        );
    }
}

#[inline(always)]
pub fn mark_row_entry_provisional_early_result(
    row_entries: &mut [RowEntry],
    note_row_entry_indices: &[u32],
    note_index: usize,
) -> bool {
    let Some(&row_entry_index) = note_row_entry_indices.get(note_index) else {
        return false;
    };
    if row_entry_index == u32::MAX {
        return false;
    }
    let Some(row_entry) = row_entries.get_mut(row_entry_index as usize) else {
        return false;
    };
    row_entry.had_provisional_early_hit = true;
    true
}

#[inline(always)]
pub fn mark_row_entry_note_finalized(
    row_entries: &mut [RowEntry],
    note_row_entry_indices: &[u32],
    note_index: usize,
    note_type: NoteType,
) -> bool {
    let Some(&row_entry_index) = note_row_entry_indices.get(note_index) else {
        return false;
    };
    if row_entry_index == u32::MAX {
        return false;
    }
    let Some(row_entry) = row_entries.get_mut(row_entry_index as usize) else {
        return false;
    };
    row_entry.unresolved_count = row_entry.unresolved_count.saturating_sub(1);
    if note_type != NoteType::Lift {
        row_entry.unresolved_nonlift_count = row_entry.unresolved_nonlift_count.saturating_sub(1);
    }
    true
}

#[inline(always)]
pub fn row_entry_index_for_cached_row(row_map_cache: &[u32], row_index: usize) -> Option<usize> {
    let pos = *row_map_cache.get(row_index)?;
    if pos == u32::MAX {
        return None;
    }
    Some(pos as usize)
}

#[inline(always)]
pub fn finalized_row_outcome_for_entry(
    row_entries: &[RowEntry],
    row_entry_index: usize,
) -> Option<FinalizedRowOutcome> {
    row_entries
        .get(row_entry_index)
        .and_then(|row_entry| row_entry.final_outcome)
}

#[inline(always)]
pub fn finalized_row_outcome_for_cached_row(
    row_entries: &[RowEntry],
    row_map_cache: &[u32],
    row_index: usize,
) -> Option<FinalizedRowOutcome> {
    let row_entry_index = row_entry_index_for_cached_row(row_map_cache, row_index)?;
    finalized_row_outcome_for_entry(row_entries, row_entry_index)
}

#[inline(always)]
pub(crate) fn completed_row_hides_note(
    row_entries: &[RowEntry],
    row_map_cache: &[u32],
    row_index: usize,
) -> bool {
    finalized_row_outcome_for_cached_row(row_entries, row_map_cache, row_index)
        .is_some_and(|outcome| row_final_grade_hides_note(outcome.final_grade))
}

/// Allocation-free read view of one player's finalized chart rows.
///
/// Presentation code uses this snapshot to decide whether a resolved tap note
/// remains visible without depending on `GameplayRuntime` or copying a
/// per-frame row mask.
#[derive(Clone, Copy, Debug, Default)]
pub struct CompletedRowVisibility<'a> {
    row_entries: &'a [RowEntry],
    row_map_cache: &'a [u32],
}

impl<'a> CompletedRowVisibility<'a> {
    #[inline(always)]
    pub const fn new(row_entries: &'a [RowEntry], row_map_cache: &'a [u32]) -> Self {
        Self {
            row_entries,
            row_map_cache,
        }
    }

    #[inline(always)]
    pub fn hides_note(self, row_index: usize) -> bool {
        completed_row_hides_note(self.row_entries, self.row_map_cache, row_index)
    }
}

#[inline(always)]
pub fn row_entry_for_cached_row<'a>(
    row_entries: &'a [RowEntry],
    row_map_cache: &[u32],
    row_index: usize,
) -> Option<&'a RowEntry> {
    let pos = row_entry_index_for_cached_row(row_map_cache, row_index)?;
    let row_entry = row_entries.get(pos as usize)?;
    debug_assert_eq!(row_entry.row_index, row_index);
    Some(row_entry)
}

#[inline(always)]
pub fn completed_row_final_judgment<'a>(
    notes: &'a [Note],
    row_entry: &RowEntry,
) -> Option<&'a Judgment> {
    let mut row_judgments: [Option<&Judgment>; MAX_COLS] = [None; MAX_COLS];
    let mut row_judgment_count = 0usize;

    for &note_index in row_entry.note_indices() {
        let judgment = notes[note_index].result.as_ref()?;
        debug_assert!(row_judgment_count < row_judgments.len());
        row_judgments[row_judgment_count] = Some(judgment);
        row_judgment_count += 1;
    }

    judgment::aggregate_row_final_judgment(
        row_judgments[..row_judgment_count]
            .iter()
            .filter_map(|judgment| *judgment),
    )
}

#[derive(Clone, Copy, Debug)]
pub struct FinalizedRowJudgment {
    pub judgment: Judgment,
    pub note_count: u32,
    pub outcome: FinalizedRowOutcome,
}

pub fn finalized_row_judgment_for_entry(
    notes: &[Note],
    row_entry: &RowEntry,
) -> Option<FinalizedRowJudgment> {
    let mut row_judgments: [Option<&Judgment>; MAX_COLS] = [None; MAX_COLS];
    let mut row_judgment_count = 0usize;

    for &note_index in row_entry.note_indices() {
        let judgment = notes.get(note_index)?.result.as_ref()?;
        debug_assert!(row_judgment_count < row_judgments.len());
        row_judgments[row_judgment_count] = Some(judgment);
        row_judgment_count += 1;
    }

    let judgment = *judgment::aggregate_row_final_judgment(
        row_judgments[..row_judgment_count]
            .iter()
            .filter_map(|judgment| *judgment),
    )?;
    Some(FinalizedRowJudgment {
        judgment,
        note_count: row_judgment_count as u32,
        outcome: FinalizedRowOutcome {
            final_grade: judgment.grade,
        },
    })
}

#[inline(always)]
pub fn completed_row_tap_feedback_plan(
    notes: &[Note],
    row_entry: &RowEntry,
) -> Option<CompletedRowTapFeedbackPlan> {
    let Some(final_judgment) = completed_row_final_judgment(notes, row_entry) else {
        return None;
    };

    let mut out = [usize::MAX; MAX_COLS];
    let mut len = 0usize;
    for &note_index in row_entry.note_indices() {
        debug_assert!(len < out.len());
        out[len] = note_index;
        len += 1;
    }
    let judgment = *final_judgment;
    Some(CompletedRowTapFeedbackPlan {
        note_indices: out,
        note_count: len,
        judgment,
        receptor_window: grade_to_window(judgment.grade),
    })
}

#[derive(Clone, Copy, Debug)]
pub struct CompletedRowTapFeedbackPlan {
    pub note_indices: [usize; MAX_COLS],
    pub note_count: usize,
    pub judgment: Judgment,
    pub receptor_window: Option<&'static str>,
}

#[inline(always)]
pub fn completed_row_flash_note_indices_and_judgment(
    notes: &[Note],
    row_entry: &RowEntry,
) -> Option<([usize; MAX_COLS], usize, Judgment)> {
    let plan = completed_row_tap_feedback_plan(notes, row_entry)?;
    Some((plan.note_indices, plan.note_count, plan.judgment))
}

#[inline(always)]
pub const fn suppress_final_bad_rescore_visual(
    row_had_provisional_early_hit: bool,
    final_grade: JudgeGrade,
) -> bool {
    row_had_provisional_early_hit && matches!(final_grade, JudgeGrade::Decent | JudgeGrade::WayOff)
}

#[inline(always)]
pub const fn finalized_row_awards_hand(
    final_grade: JudgeGrade,
    note_count: u32,
    carried_holds_down: usize,
) -> bool {
    if matches!(final_grade, JudgeGrade::Miss | JudgeGrade::WayOff) {
        return false;
    }
    note_count as usize + carried_holds_down >= 3
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RowFinalizationPlayerState {
    pub combo: ComboState,
    pub current_combo_window_counts: WindowCounts,
    pub judgment_counts: judgment::JudgeCounts,
    pub scoring_counts: judgment::JudgeCounts,
    pub hands_achieved: u32,
}

pub fn row_finalization_player_state(player: &PlayerRuntime) -> RowFinalizationPlayerState {
    RowFinalizationPlayerState {
        combo: player_combo_state(player),
        current_combo_window_counts: player.current_combo_window_counts,
        judgment_counts: player.judgment_counts,
        scoring_counts: player.scoring_counts,
        hands_achieved: player.hands_achieved,
    }
}

pub fn set_row_finalization_player_state(
    player: &mut PlayerRuntime,
    state: RowFinalizationPlayerState,
) {
    write_player_combo_state(player, state.combo);
    player.current_combo_window_counts = state.current_combo_window_counts;
    player.judgment_counts = state.judgment_counts;
    player.scoring_counts = state.scoring_counts;
    player.hands_achieved = state.hands_achieved;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowFinalizationPlayerUpdate {
    pub combo_update: ComboUpdate,
    pub update_grade_totals: bool,
    pub awarded_hand: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct RowFinalizationPlan {
    pub judgment: Judgment,
    pub life_delta: f32,
    pub note_count: u32,
    pub outcome: FinalizedRowOutcome,
    pub show_final_visual: bool,
    pub record_display_window_counts: bool,
    pub apply_player_state: bool,
    pub apply_life_change: bool,
    pub capture_failed_ex_score_inputs: bool,
}

pub fn row_finalization_plan(
    row_judgment: FinalizedRowJudgment,
    scoring_blocked: bool,
    skip_life_change: bool,
) -> RowFinalizationPlan {
    let suppress_final_visual =
        suppress_final_bad_rescore_visual(skip_life_change, row_judgment.judgment.grade);
    let apply_scoring_effects = !scoring_blocked;
    let judgment = row_judgment.judgment;
    RowFinalizationPlan {
        judgment,
        life_delta: deadsync_rules::life::judge_life_delta(judgment.grade),
        note_count: row_judgment.note_count,
        outcome: row_judgment.outcome,
        show_final_visual: !suppress_final_visual,
        record_display_window_counts: apply_scoring_effects,
        apply_player_state: apply_scoring_effects,
        apply_life_change: apply_scoring_effects && !skip_life_change,
        capture_failed_ex_score_inputs: apply_scoring_effects && !skip_life_change,
    }
}

pub fn row_finalization_plan_for_entry(
    notes: &[Note],
    row_entry: &RowEntry,
    scoring_blocked: bool,
    skip_life_change: bool,
) -> Option<RowFinalizationPlan> {
    let row_judgment = finalized_row_judgment_for_entry(notes, row_entry)?;
    Some(row_finalization_plan(
        row_judgment,
        scoring_blocked,
        skip_life_change,
    ))
}

pub fn apply_row_finalization_player_state(
    state: &mut RowFinalizationPlayerState,
    judgment: &Judgment,
    note_count: u32,
    carried_holds_down: usize,
    player_dead: bool,
) -> RowFinalizationPlayerUpdate {
    let final_grade = judgment.grade;
    let grade_ix = judgment::display_judge_ix(final_grade);
    state.judgment_counts[grade_ix] = state.judgment_counts[grade_ix].saturating_add(1);
    let update_grade_totals = !player_dead;
    if update_grade_totals {
        state.scoring_counts[grade_ix] = state.scoring_counts[grade_ix].saturating_add(1);
    }
    record_combo_window_count_for_judgment(&mut state.current_combo_window_counts, judgment);
    let combo_update = combo::apply_row_combo_state(&mut state.combo, final_grade, note_count, 1);
    let awarded_hand = finalized_row_awards_hand(final_grade, note_count, carried_holds_down);
    if awarded_hand {
        state.hands_achieved = state.hands_achieved.saturating_add(1);
    }
    RowFinalizationPlayerUpdate {
        combo_update,
        update_grade_totals,
        awarded_hand,
    }
}

pub fn carried_holds_down_at_row(
    notes: &[Note],
    active_holds: &[Option<ActiveHold>],
    col_range: (usize, usize),
    row_index: usize,
) -> usize {
    let start = col_range.0.min(active_holds.len());
    let end = col_range.1.min(active_holds.len());
    if start >= end {
        return 0;
    }
    active_holds[start..end]
        .iter()
        .filter_map(|active| active.as_ref())
        .filter(|active| active_hold_is_engaged(active))
        .filter(|active| {
            let Some(note) = notes.get(active.note_index) else {
                return false;
            };
            note.row_index < row_index
                && note
                    .hold
                    .as_ref()
                    .is_some_and(|hold| hold.last_held_row_index >= row_index)
        })
        .count()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerRowScanState {
    BeyondLookahead,
    Pending,
    Ready {
        row_index: usize,
        skip_life_change: bool,
    },
    Finalized,
}

#[inline(always)]
pub fn player_row_scan_state(
    row_entries: &[RowEntry],
    row_entry_index: usize,
    lookahead_time_ns: SongTimeNs,
) -> PlayerRowScanState {
    let row_entry = &row_entries[row_entry_index];
    if row_entry.final_outcome.is_some() {
        return PlayerRowScanState::Finalized;
    }
    if row_entry.time_ns > lookahead_time_ns {
        return PlayerRowScanState::BeyondLookahead;
    }
    if row_entry.unresolved_count != 0 {
        return PlayerRowScanState::Pending;
    }
    PlayerRowScanState::Ready {
        row_index: row_entry.row_index,
        skip_life_change: row_entry.had_provisional_early_hit,
    }
}

#[inline(always)]
pub fn next_ready_row_in_lookahead<F>(
    start: usize,
    row_count: usize,
    mut row_state: F,
) -> Option<(usize, usize, bool)>
where
    F: FnMut(usize) -> PlayerRowScanState,
{
    let mut row_entry_index = start;
    while row_entry_index < row_count {
        match row_state(row_entry_index) {
            PlayerRowScanState::BeyondLookahead => break,
            PlayerRowScanState::Ready {
                row_index,
                skip_life_change,
            } => return Some((row_entry_index, row_index, skip_life_change)),
            PlayerRowScanState::Pending | PlayerRowScanState::Finalized => {}
        }
        row_entry_index += 1;
    }
    None
}

#[inline(always)]
pub fn advance_judged_row_cursor<F>(cursor: usize, row_count: usize, mut row_state: F) -> usize
where
    F: FnMut(usize) -> PlayerRowScanState,
{
    let mut next_cursor = cursor;
    while next_cursor < row_count {
        match row_state(next_cursor) {
            PlayerRowScanState::Finalized => {
                next_cursor += 1;
            }
            PlayerRowScanState::BeyondLookahead
            | PlayerRowScanState::Pending
            | PlayerRowScanState::Ready { .. } => break,
        }
    }
    next_cursor
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadyJudgedRowEvent {
    pub row_entry_index: usize,
    pub row_index: usize,
    pub skip_life_change: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadyJudgedRowsUpdate {
    pub next_scan_start: usize,
    pub event_count: usize,
    pub stopped: bool,
}

pub fn collect_ready_judged_row_events(
    row_entries: &[RowEntry],
    row_range: (usize, usize),
    cursor: usize,
    lookahead_time_ns: SongTimeNs,
    events: &mut [Option<ReadyJudgedRowEvent>],
) -> ReadyJudgedRowsUpdate {
    let row_start = row_range.0.min(row_entries.len());
    let row_count = row_range.1.min(row_entries.len()).max(row_start);
    let mut scan_start = cursor.max(row_start).min(row_count);
    let mut event_count = 0usize;
    while let Some((row_entry_index, row_index, skip_life_change)) =
        next_ready_row_in_lookahead(scan_start, row_count, |idx| {
            player_row_scan_state(row_entries, idx, lookahead_time_ns)
        })
    {
        if event_count >= events.len() {
            return ReadyJudgedRowsUpdate {
                next_scan_start: row_entry_index,
                event_count,
                stopped: true,
            };
        }
        events[event_count] = Some(ReadyJudgedRowEvent {
            row_entry_index,
            row_index,
            skip_life_change,
        });
        event_count += 1;
        scan_start = row_entry_index + 1;
    }
    ReadyJudgedRowsUpdate {
        next_scan_start: scan_start,
        event_count,
        stopped: false,
    }
}

pub fn advance_judged_row_cursor_for_entries(
    row_entries: &[RowEntry],
    row_range: (usize, usize),
    cursor: usize,
    lookahead_time_ns: SongTimeNs,
) -> usize {
    let row_start = row_range.0.min(row_entries.len());
    let row_count = row_range.1.min(row_entries.len()).max(row_start);
    advance_judged_row_cursor(cursor.max(row_start).min(row_count), row_count, |idx| {
        player_row_scan_state(row_entries, idx, lookahead_time_ns)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowGrid {
    pub row_index: usize,
    pub note_indices: [usize; MAX_COLS],
}

#[inline(always)]
pub fn notes_row_sorted(notes: &[Note]) -> bool {
    notes
        .windows(2)
        .all(|pair| pair[0].row_index <= pair[1].row_index)
}

pub fn build_row_grids(
    notes: &[Note],
    note_range: (usize, usize),
    col_offset: usize,
    cols: usize,
) -> Vec<RowGrid> {
    let (start, end) = note_range;
    debug_assert!(start <= end && end <= notes.len());
    debug_assert!(notes_row_sorted(&notes[start..end]));

    let mut rows = Vec::<RowGrid>::new();
    for (offset, note) in notes[start..end].iter().enumerate() {
        let note_idx = start + offset;
        if note.column < col_offset {
            continue;
        }
        let local = note.column - col_offset;
        if local >= cols || local >= MAX_COLS {
            continue;
        }
        if !matches!(rows.last(), Some(row) if row.row_index == note.row_index) {
            rows.push(RowGrid {
                row_index: note.row_index,
                note_indices: [usize::MAX; MAX_COLS],
            });
        }
        rows.last_mut()
            .expect("row grid inserted for current note")
            .note_indices[local] = note_idx;
    }
    rows
}

#[inline(always)]
fn note_counts_for_simultaneous_limit(note: &Note) -> bool {
    match note.note_type {
        NoteType::Tap | NoteType::Lift => !note.is_fake,
        NoteType::Hold | NoteType::Roll => true,
        NoteType::Mine | NoteType::Fake => false,
    }
}
