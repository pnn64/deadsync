#[derive(Clone, Debug)]
pub struct GameplayNoteCountStatsState {
    stats: [Vec<NoteCountStat>; MAX_PLAYERS],
}

impl Default for GameplayNoteCountStatsState {
    fn default() -> Self {
        Self {
            stats: std::array::from_fn(|_| Vec::new()),
        }
    }
}

impl GameplayNoteCountStatsState {
    pub fn new(stats: [Vec<NoteCountStat>; MAX_PLAYERS]) -> Self {
        Self { stats }
    }

    #[inline(always)]
    pub fn player_stats(&self, player: usize) -> &[NoteCountStat] {
        self.stats.get(player).map_or(&[], Vec::as_slice)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameplayNoteRangeState {
    ranges: [(usize, usize); MAX_PLAYERS],
}

impl Default for GameplayNoteRangeState {
    fn default() -> Self {
        Self {
            ranges: [(0, 0); MAX_PLAYERS],
        }
    }
}

impl GameplayNoteRangeState {
    #[inline(always)]
    pub const fn new(ranges: [(usize, usize); MAX_PLAYERS]) -> Self {
        Self { ranges }
    }

    #[inline(always)]
    pub const fn ranges(&self) -> &[(usize, usize); MAX_PLAYERS] {
        &self.ranges
    }

    #[inline(always)]
    pub fn range(&self, player: usize) -> (usize, usize) {
        self.ranges.get(player).copied().unwrap_or((0, 0))
    }

    #[inline(always)]
    pub fn set_range_for_test(&mut self, player: usize, range: (usize, usize)) {
        if let Some(slot) = self.ranges.get_mut(player) {
            *slot = range;
        }
    }

    #[inline(always)]
    pub fn clear_for_test(&mut self) {
        self.ranges.fill((0, 0));
    }
}

#[derive(Clone, Debug)]
pub struct GameplayLaneIndexState {
    pub note_indices: [Vec<usize>; MAX_COLS],
    pub note_search_cursors: [LaneNoteWindowCursor; MAX_COLS],
    pub note_row_indices: [Vec<usize>; MAX_COLS],
    pub hold_indices: [Vec<usize>; MAX_COLS],
    /// Song-lifetime `beat_to_note_row` values indexed by note index.
    ///
    /// Visible-window searches read this compact immutable cache instead of
    /// chasing into the much wider `Note` array and repeating float-to-row
    /// conversion for every binary-search comparison on every frame.
    pub note_itg_rows: Vec<i32>,
    pub tap_row_hold_roll_flags: Vec<u8>,
}

impl Default for GameplayLaneIndexState {
    fn default() -> Self {
        Self {
            note_indices: std::array::from_fn(|_| Vec::new()),
            note_search_cursors: [LaneNoteWindowCursor::default(); MAX_COLS],
            note_row_indices: std::array::from_fn(|_| Vec::new()),
            hold_indices: std::array::from_fn(|_| Vec::new()),
            note_itg_rows: Vec::new(),
            tap_row_hold_roll_flags: Vec::new(),
        }
    }
}

impl GameplayLaneIndexState {
    pub fn new(
        note_indices: [Vec<usize>; MAX_COLS],
        note_row_indices: [Vec<usize>; MAX_COLS],
        hold_indices: [Vec<usize>; MAX_COLS],
        note_itg_rows: Vec<i32>,
        tap_row_hold_roll_flags: Vec<u8>,
    ) -> Self {
        Self {
            note_indices,
            note_search_cursors: [LaneNoteWindowCursor::default(); MAX_COLS],
            note_row_indices,
            hold_indices,
            note_itg_rows,
            tap_row_hold_roll_flags,
        }
    }

    #[inline(always)]
    pub fn note_indices(&self, col: usize) -> &[usize] {
        self.note_indices.get(col).map_or(&[], Vec::as_slice)
    }

    #[inline(always)]
    pub fn note_row_indices(&self, col: usize) -> &[usize] {
        self.note_row_indices.get(col).map_or(&[], Vec::as_slice)
    }

    #[inline(always)]
    pub fn hold_indices(&self, col: usize) -> &[usize] {
        self.hold_indices.get(col).map_or(&[], Vec::as_slice)
    }

    #[inline(always)]
    pub fn note_itg_rows(&self) -> &[i32] {
        &self.note_itg_rows
    }

    #[inline(always)]
    pub fn tap_row_hold_roll_flags(&self, note_index: usize) -> u8 {
        self.tap_row_hold_roll_flags
            .get(note_index)
            .copied()
            .unwrap_or(0)
    }

    #[inline(always)]
    pub fn clear_for_test(&mut self) {
        for indices in &mut self.note_indices {
            indices.clear();
        }
        self.note_search_cursors = [LaneNoteWindowCursor::default(); MAX_COLS];
        for indices in &mut self.note_row_indices {
            indices.clear();
        }
        for indices in &mut self.hold_indices {
            indices.clear();
        }
        self.note_itg_rows.clear();
        self.tap_row_hold_roll_flags.clear();
    }
}

#[derive(Clone, Debug)]
pub struct GameplayRowIndexState {
    pub row_entry_ranges: [(usize, usize); MAX_PLAYERS],
    pub judged_row_cursor: [usize; MAX_PLAYERS],
    /// Song-lifetime row-entry index aligned one-to-one with chart notes.
    ///
    /// The game thread builds this once before gameplay. Judgment and
    /// presentation use the already-known note index to read the corresponding
    /// row directly, avoiding a second dense allocation sized to the chart's
    /// largest row number. `u32::MAX` marks mines, fakes, and unjudgable notes.
    pub note_row_entry_indices: Vec<u32>,
}

impl Default for GameplayRowIndexState {
    fn default() -> Self {
        Self {
            row_entry_ranges: [(0, 0); MAX_PLAYERS],
            judged_row_cursor: [0; MAX_PLAYERS],
            note_row_entry_indices: Vec::new(),
        }
    }
}

impl GameplayRowIndexState {
    pub fn new(
        row_entry_ranges: [(usize, usize); MAX_PLAYERS],
        judged_row_cursor: [usize; MAX_PLAYERS],
        note_row_entry_indices: Vec<u32>,
    ) -> Self {
        Self {
            row_entry_ranges,
            judged_row_cursor,
            note_row_entry_indices,
        }
    }

    #[inline(always)]
    pub fn clear_for_test(&mut self) {
        self.row_entry_ranges.fill((0, 0));
        self.judged_row_cursor.fill(0);
        self.note_row_entry_indices.clear();
    }
}

#[derive(Clone, Debug)]
pub struct GameplayMineScanState {
    pub next_tap_miss_cursor: [usize; MAX_PLAYERS],
    pub next_mine_avoid_cursor: [usize; MAX_PLAYERS],
    pub mine_note_ix: [Vec<usize>; MAX_PLAYERS],
    pub mine_note_time_ns: [Vec<SongTimeNs>; MAX_PLAYERS],
    pub next_mine_ix_cursor: [usize; MAX_PLAYERS],
    /// Song-sized frame batch allocated during setup and cleared after each use.
    pub pending_mine_hit_indices: Vec<usize>,
}

impl Default for GameplayMineScanState {
    fn default() -> Self {
        Self {
            next_tap_miss_cursor: [0; MAX_PLAYERS],
            next_mine_avoid_cursor: [0; MAX_PLAYERS],
            mine_note_ix: std::array::from_fn(|_| Vec::new()),
            mine_note_time_ns: std::array::from_fn(|_| Vec::new()),
            next_mine_ix_cursor: [0; MAX_PLAYERS],
            pending_mine_hit_indices: Vec::new(),
        }
    }
}

impl GameplayMineScanState {
    pub fn new(
        note_range_start: [usize; MAX_PLAYERS],
        mine_note_ix: [Vec<usize>; MAX_PLAYERS],
        mine_note_time_ns: [Vec<SongTimeNs>; MAX_PLAYERS],
    ) -> Self {
        let mine_count = mine_note_ix.iter().map(Vec::len).sum();
        Self {
            next_tap_miss_cursor: note_range_start,
            next_mine_avoid_cursor: note_range_start,
            mine_note_ix,
            mine_note_time_ns,
            next_mine_ix_cursor: [0; MAX_PLAYERS],
            pending_mine_hit_indices: Vec::with_capacity(mine_count),
        }
    }

    #[inline(always)]
    pub fn set_next_tap_miss_cursor(&mut self, player: usize, cursor: usize) {
        if let Some(slot) = self.next_tap_miss_cursor.get_mut(player) {
            *slot = cursor;
        }
    }

    #[inline(always)]
    pub fn clear_for_test(&mut self) {
        self.next_tap_miss_cursor.fill(0);
        self.next_mine_avoid_cursor.fill(0);
        self.next_mine_ix_cursor.fill(0);
        for mine_ix in &mut self.mine_note_ix {
            mine_ix.clear();
        }
        for mine_time_ns in &mut self.mine_note_time_ns {
            mine_time_ns.clear();
        }
        self.pending_mine_hit_indices.clear();
    }
}

#[derive(Clone, Debug)]
pub struct GameplayChartRuntimeState {
    pub notes: Vec<Note>,
    /// Notes whose row-final judgment was broadcast before the player's
    /// published health state became dead. This is a song-lifetime bitset,
    /// allocated during gameplay setup and never resized on a live frame.
    pub column_judgment_eligible: Vec<bool>,
    pub note_ranges: GameplayNoteRangeState,
    pub note_count_stats: GameplayNoteCountStatsState,
    pub lane_indices: GameplayLaneIndexState,
    pub row_indices: GameplayRowIndexState,
    pub note_time_cache_ns: Vec<SongTimeNs>,
    pub hold_end_time_cache_ns: Vec<Option<SongTimeNs>>,
    /// Song-lifetime displayed-beat values for note heads and hold tails.
    ///
    /// XMod/MMod rendering reads these compact pairs instead of binary
    /// searching immutable scroll segments for every visible arrow each frame.
    pub note_displayed_beat_cache: Vec<[f32; 2]>,
    /// Whether each player's displayed-beat mapping is strictly increasing
    /// enough for visible timing cues to use binary-sliced segment ranges.
    pub displayed_beat_monotonic: [bool; MAX_PLAYERS],
    pub mine_scan: GameplayMineScanState,
    pub row_entries: Vec<RowEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct GameplayHoldRuntimeState {
    active_holds: [Option<ActiveHold>; MAX_COLS],
    active_hold_mask: LaneMask,
    pub decaying_hold_indices: Vec<usize>,
    pub hold_decay_active: Vec<bool>,
    pub tap_miss_held_window: Vec<bool>,
    pub pending_missed_hold_resolution: Vec<bool>,
    pub pending_missed_hold_indices: Vec<usize>,
    pub pump_events: Vec<PumpHoldEvent>,
    pub pump_event_cursor: usize,
    pub pump_checkpoint_hits: Vec<u32>,
    pub pump_checkpoint_misses: Vec<u32>,
    pub pump_pending_tail: Vec<bool>,
    pub pump_pending_tail_indices: Vec<usize>,
}

impl GameplayHoldRuntimeState {
    pub fn new(notes_len: usize, decaying_hold_capacity: usize) -> Self {
        Self::new_with_pump_events(notes_len, decaying_hold_capacity, Vec::new())
    }

    pub fn new_with_pump_events(
        notes_len: usize,
        decaying_hold_capacity: usize,
        pump_events: Vec<PumpHoldEvent>,
    ) -> Self {
        Self {
            active_holds: std::array::from_fn(|_| None),
            active_hold_mask: 0,
            decaying_hold_indices: Vec::with_capacity(decaying_hold_capacity),
            hold_decay_active: vec![false; notes_len],
            tap_miss_held_window: vec![false; notes_len],
            pending_missed_hold_resolution: vec![false; notes_len],
            pending_missed_hold_indices: Vec::with_capacity(decaying_hold_capacity),
            pump_events,
            pump_event_cursor: 0,
            pump_checkpoint_hits: vec![0; notes_len],
            pump_checkpoint_misses: vec![0; notes_len],
            pump_pending_tail: vec![false; notes_len],
            pump_pending_tail_indices: Vec::with_capacity(decaying_hold_capacity),
        }
    }

    #[inline(always)]
    pub fn reset_live_state(&mut self) {
        self.active_holds.fill(None);
        self.active_hold_mask = 0;
        self.decaying_hold_indices.clear();
        self.hold_decay_active.fill(false);
        self.tap_miss_held_window.fill(false);
        self.pending_missed_hold_resolution.fill(false);
        self.pending_missed_hold_indices.clear();
        self.pump_event_cursor = 0;
        self.pump_checkpoint_hits.fill(0);
        self.pump_checkpoint_misses.fill(0);
        self.pump_pending_tail.fill(false);
        self.pump_pending_tail_indices.clear();
    }

    #[inline(always)]
    pub fn clear_for_test(&mut self) {
        self.active_holds.fill(None);
        self.active_hold_mask = 0;
        self.decaying_hold_indices.clear();
        self.hold_decay_active.clear();
        self.tap_miss_held_window.clear();
        self.pending_missed_hold_resolution.clear();
        self.pending_missed_hold_indices.clear();
        self.pump_events.clear();
        self.pump_event_cursor = 0;
        self.pump_checkpoint_hits.clear();
        self.pump_checkpoint_misses.clear();
        self.pump_pending_tail.clear();
        self.pump_pending_tail_indices.clear();
    }

    #[inline(always)]
    pub fn reanchor_pump_events(&mut self, music_time_ns: SongTimeNs) {
        self.pump_event_cursor = self
            .pump_events
            .partition_point(|event| event.time_ns < music_time_ns);
    }

    #[inline(always)]
    pub fn active_hold_mask(&self) -> LaneMask {
        self.active_hold_mask
    }

    #[inline(always)]
    pub fn set_active_hold_mask(&mut self, mask: LaneMask) {
        self.active_hold_mask = mask;
    }

    #[inline(always)]
    pub fn set_active_hold(&mut self, col: usize, active: Option<ActiveHold>) {
        let Some(slot) = self.active_holds.get_mut(col) else {
            return;
        };
        let present = active.is_some();
        *slot = active;
        set_feedback_bit(&mut self.active_hold_mask, col, present);
    }

    #[inline(always)]
    pub fn sync_active_hold_col(&mut self, col: usize) {
        let present = self.active_holds.get(col).is_some_and(Option::is_some);
        set_feedback_bit(&mut self.active_hold_mask, col, present);
    }

    #[inline(always)]
    pub fn clear_active_holds(&mut self) {
        self.active_holds.fill(None);
        self.active_hold_mask = 0;
    }
}

#[derive(Clone, Debug)]
pub struct GameplayCueRuntimeState {
    measure_counter_segments: [Vec<StreamSegment>; MAX_PLAYERS],
    column_cues: [Vec<ColumnCue>; MAX_PLAYERS],
    // Number of leading regular cues whose start has been crossed. Rendering
    // reuses this cursor instead of repeating a binary search every frame.
    column_cue_cursor: [usize; MAX_PLAYERS],
    crossover_cues: [Vec<ColumnCue>; MAX_PLAYERS],
    // Per-crossover-cue fade-in times. Only the prefix ending at
    // `crossover_cue_cursor[player]` is valid; newly crossed entries overwrite
    // stale values after a rewind. The game thread owns these song-sized boxed
    // slices and allocates them at gameplay setup. Live frames never grow,
    // clear, evict, or destroy them, and the worst frame writes only the cues
    // crossed by a forward seek.
    crossover_cue_entry: [Box<[f32]>; MAX_PLAYERS],
    // Number of leading crossover cues whose start has been crossed (and thus
    // anchored) at the last evaluated time. Lets the gate anchor/rewind only the
    // cues that changed since the previous frame.
    crossover_cue_cursor: [usize; MAX_PLAYERS],
}

impl Default for GameplayCueRuntimeState {
    fn default() -> Self {
        Self {
            measure_counter_segments: std::array::from_fn(|_| Vec::new()),
            column_cues: std::array::from_fn(|_| Vec::new()),
            column_cue_cursor: [0; MAX_PLAYERS],
            crossover_cues: std::array::from_fn(|_| Vec::new()),
            crossover_cue_entry: std::array::from_fn(|_| Box::default()),
            crossover_cue_cursor: [0; MAX_PLAYERS],
        }
    }
}

impl GameplayCueRuntimeState {
    pub fn new(
        measure_counter_segments: [Vec<StreamSegment>; MAX_PLAYERS],
        column_cues: [Vec<ColumnCue>; MAX_PLAYERS],
        crossover_cues: [Vec<ColumnCue>; MAX_PLAYERS],
    ) -> Self {
        let crossover_cue_entry = std::array::from_fn(|player| {
            vec![0.0; crossover_cues[player].len()].into_boxed_slice()
        });
        Self {
            measure_counter_segments,
            column_cues,
            column_cue_cursor: [0; MAX_PLAYERS],
            crossover_cues,
            crossover_cue_entry,
            crossover_cue_cursor: [0; MAX_PLAYERS],
        }
    }

    #[inline(always)]
    pub fn measure_counter_segments(&self, player: usize) -> &[StreamSegment] {
        self.measure_counter_segments
            .get(player)
            .map_or(&[], Vec::as_slice)
    }

    #[inline(always)]
    pub fn column_cues(&self, player: usize) -> &[ColumnCue] {
        self.column_cues.get(player).map_or(&[], Vec::as_slice)
    }

    #[inline(always)]
    pub fn column_cue_cursor(&self, player: usize) -> usize {
        self.column_cue_cursor
            .get(player)
            .copied()
            .unwrap_or_default()
    }

    #[inline(always)]
    pub fn crossover_cues(&self, player: usize) -> &[ColumnCue] {
        self.crossover_cues.get(player).map_or(&[], Vec::as_slice)
    }

    // Per-cue fade-in anchor times parallel to `crossover_cues(player)`. Only
    // entries before `crossover_cue_cursor(player)` are valid.
    #[inline(always)]
    pub fn crossover_cue_entries(&self, player: usize) -> &[f32] {
        self.crossover_cue_entry
            .get(player)
            .map_or(&[], Box::as_ref)
    }

    #[inline(always)]
    pub fn crossover_cue_cursor(&self, player: usize) -> usize {
        self.crossover_cue_cursor
            .get(player)
            .copied()
            .unwrap_or_default()
    }

    // The fade-in anchor time for the crossover cue at `index`, or None when the
    // cue has not been reached yet (callers fall back to the cue's own start,
    // i.e. the natural fade-in).
    #[inline(always)]
    pub fn crossover_cue_entry_time(&self, player: usize, index: usize) -> Option<f32> {
        if index >= self.crossover_cue_cursor(player) {
            return None;
        }
        self.crossover_cue_entry
            .get(player)
            .and_then(|entry| entry.get(index).copied())
    }

    #[inline(always)]
    pub fn update_column_cue_cursor(&mut self, player: usize, current_time: f32) {
        let Some(cues) = self.column_cues.get(player) else {
            return;
        };
        self.column_cue_cursor[player] =
            column_cue_cursor_from_hint(cues, current_time, self.column_cue_cursor[player]);
    }

    // Advances the per-player crossover cue fade-in anchors to `current_time`
    // (visible music seconds). The first frame the playhead crosses a cue's
    // start, anchor it to the cue's start if it was reached within
    // CROSSOVER_CUE_SEEK_GUARD_SECONDS (normal play -> natural fade-in) or to
    // `current_time` if the start was jumped over (seek -> fade in from the
    // landing point). Anchors are cleared for cues the playhead has rewound
    // before, so a replayed section fades in naturally. Call once per player per
    // frame.
    pub fn update_crossover_cue_anchors(&mut self, player: usize, current_time: f32) {
        let Some(cues) = self.crossover_cues.get(player) else {
            return;
        };
        let Some(entry) = self.crossover_cue_entry.get_mut(player) else {
            return;
        };
        if entry.len() != cues.len() {
            debug_assert_eq!(entry.len(), cues.len(), "crossover cue anchors are pre-sized");
            return;
        }
        if cues.is_empty() {
            return;
        }
        let cursor = self.crossover_cue_cursor[player];
        let target = column_cue_cursor_from_hint(cues, current_time, cursor);
        if target > cursor {
            for i in cursor..target {
                let start = cues[i].start_time;
                entry[i] = if current_time - start < CROSSOVER_CUE_SEEK_GUARD_SECONDS {
                    start
                } else {
                    current_time
                };
            }
        }
        self.crossover_cue_cursor[player] = target;
    }

    #[inline(always)]
    pub fn set_column_cues_for_test(&mut self, player: usize, cues: Vec<ColumnCue>) {
        if let Some(slot) = self.column_cues.get_mut(player) {
            *slot = cues;
            self.column_cue_cursor[player] = 0;
        }
    }

    #[inline(always)]
    pub fn clear_for_test(&mut self) {
        for segments in &mut self.measure_counter_segments {
            segments.clear();
        }
        for cues in &mut self.column_cues {
            cues.clear();
        }
        self.column_cue_cursor = [0; MAX_PLAYERS];
        for cues in &mut self.crossover_cues {
            cues.clear();
        }
        for entry in &mut self.crossover_cue_entry {
            *entry = Box::default();
        }
        self.crossover_cue_cursor = [0; MAX_PLAYERS];
    }
}

#[derive(Clone, Debug, Default)]
pub struct GameplayHoldFeedbackState {
    hold_judgments: [Option<HoldJudgmentRenderInfo>; MAX_COLS],
    held_miss_judgments: [Option<HeldMissRenderInfo>; MAX_COLS],
    hold_mask: LaneMask,
    held_miss_mask: LaneMask,
}

impl GameplayHoldFeedbackState {
    #[inline(always)]
    pub fn hold_judgment(&self, col: usize) -> Option<HoldJudgmentRenderInfo> {
        self.hold_judgments.get(col).copied().flatten()
    }

    #[inline(always)]
    pub fn hold_judgments(
        &self,
        col_start: usize,
        num_cols: usize,
    ) -> &[Option<HoldJudgmentRenderInfo>] {
        let end = col_start.saturating_add(num_cols).min(MAX_COLS);
        self.hold_judgments.get(col_start..end).unwrap_or(&[])
    }

    #[inline(always)]
    pub fn held_miss_judgments(
        &self,
        col_start: usize,
        num_cols: usize,
    ) -> &[Option<HeldMissRenderInfo>] {
        let end = col_start.saturating_add(num_cols).min(MAX_COLS);
        self.held_miss_judgments.get(col_start..end).unwrap_or(&[])
    }

    #[inline(always)]
    pub(crate) fn set_hold_judgment(
        &mut self,
        col: usize,
        judgment: Option<HoldJudgmentRenderInfo>,
    ) {
        let Some(slot) = self.hold_judgments.get_mut(col) else {
            return;
        };
        *slot = judgment;
        set_feedback_bit(&mut self.hold_mask, col, judgment.is_some());
    }

    #[inline(always)]
    pub(crate) fn set_held_miss(&mut self, col: usize, judgment: Option<HeldMissRenderInfo>) {
        let Some(slot) = self.held_miss_judgments.get_mut(col) else {
            return;
        };
        *slot = judgment;
        set_feedback_bit(&mut self.held_miss_mask, col, judgment.is_some());
    }

    #[inline(always)]
    pub(crate) fn tick(&mut self, now: f32) {
        let mut active = self.hold_mask;
        while active != 0 {
            let col = active.trailing_zeros() as usize;
            let bit = 1 << col;
            if self.hold_judgments[col].is_some_and(|info| hold_judgment_expired_at(info, now)) {
                self.hold_judgments[col] = None;
                self.hold_mask &= !bit;
            }
            active &= active - 1;
        }
        let mut active = self.held_miss_mask;
        while active != 0 {
            let col = active.trailing_zeros() as usize;
            let bit = 1 << col;
            if self.held_miss_judgments[col]
                .is_some_and(|info| held_miss_judgment_expired_at(info, now))
            {
                self.held_miss_judgments[col] = None;
                self.held_miss_mask &= !bit;
            }
            active &= active - 1;
        }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.hold_judgments.fill(None);
        self.held_miss_judgments.fill(None);
        self.hold_mask = 0;
        self.held_miss_mask = 0;
    }
}

#[derive(Clone, Debug, Default)]
pub struct GameplayVisualFeedbackState {
    tap_explosions: [Option<ActiveTapExplosion>; MAX_COLS],
    column_flashes: [Option<ActiveColumnFlash>; MAX_COLS],
    pub last_tap_judgments: [Option<ColumnTapJudgment>; MAX_COLS],
    mine_explosions: [Option<ActiveMineExplosion>; MAX_COLS],
    tap_mask: LaneMask,
    flash_mask: LaneMask,
    mine_mask: LaneMask,
}

impl GameplayVisualFeedbackState {
    #[inline(always)]
    pub fn tap_explosions(
        &self,
        col_start: usize,
        num_cols: usize,
    ) -> &[Option<ActiveTapExplosion>] {
        let end = col_start.saturating_add(num_cols).min(MAX_COLS);
        self.tap_explosions.get(col_start..end).unwrap_or(&[])
    }

    #[inline(always)]
    pub fn column_flashes(
        &self,
        col_start: usize,
        num_cols: usize,
    ) -> &[Option<ActiveColumnFlash>] {
        let end = col_start.saturating_add(num_cols).min(MAX_COLS);
        self.column_flashes.get(col_start..end).unwrap_or(&[])
    }

    #[inline(always)]
    pub fn mine_explosions(
        &self,
        col_start: usize,
        num_cols: usize,
    ) -> &[Option<ActiveMineExplosion>] {
        let end = col_start.saturating_add(num_cols).min(MAX_COLS);
        self.mine_explosions.get(col_start..end).unwrap_or(&[])
    }

    #[inline(always)]
    pub fn last_tap_judgment(&self, col: usize) -> Option<ColumnTapJudgment> {
        self.last_tap_judgments.get(col).copied().flatten()
    }

    #[inline(always)]
    pub fn mine_started_at_screen_s(&self, col: usize) -> Option<f32> {
        self.mine_explosions
            .get(col)
            .and_then(Option::as_ref)
            .map(|mine| mine.started_at_screen_s)
    }

    #[inline(always)]
    pub(crate) fn set_tap_explosion(&mut self, col: usize, explosion: Option<ActiveTapExplosion>) {
        let Some(slot) = self.tap_explosions.get_mut(col) else {
            return;
        };
        *slot = explosion;
        set_feedback_bit(&mut self.tap_mask, col, explosion.is_some());
    }

    #[inline(always)]
    pub(crate) fn set_column_flash(&mut self, col: usize, flash: Option<ActiveColumnFlash>) {
        let Some(slot) = self.column_flashes.get_mut(col) else {
            return;
        };
        *slot = flash;
        set_feedback_bit(&mut self.flash_mask, col, flash.is_some());
    }

    #[inline(always)]
    pub(crate) fn set_mine_explosion(
        &mut self,
        col: usize,
        explosion: Option<ActiveMineExplosion>,
    ) {
        let Some(slot) = self.mine_explosions.get_mut(col) else {
            return;
        };
        let active = explosion.is_some();
        *slot = explosion;
        set_feedback_bit(&mut self.mine_mask, col, active);
    }

    #[inline(always)]
    pub(crate) fn tick(&mut self, delta_time: f32, now: f32) {
        let mut active = self.tap_mask;
        while active != 0 {
            let col = active.trailing_zeros() as usize;
            let bit = 1 << col;
            tick_tap_explosion_slot(&mut self.tap_explosions[col], delta_time);
            if self.tap_explosions[col].is_none() {
                self.tap_mask &= !bit;
            }
            active &= active - 1;
        }
        let mut active = self.mine_mask;
        while active != 0 {
            let col = active.trailing_zeros() as usize;
            let bit = 1 << col;
            tick_mine_explosion_slot(&mut self.mine_explosions[col], delta_time);
            if self.mine_explosions[col].is_none() {
                self.mine_mask &= !bit;
            }
            active &= active - 1;
        }
        let mut active = self.flash_mask;
        while active != 0 {
            let col = active.trailing_zeros() as usize;
            let bit = 1 << col;
            if self.column_flashes[col].is_some_and(|flash| column_flash_expired_at(flash, now)) {
                self.column_flashes[col] = None;
                self.flash_mask &= !bit;
            }
            active &= active - 1;
        }
    }

    #[inline(always)]
    pub fn set_tap_explosion_for_test(
        &mut self,
        col: usize,
        explosion: Option<ActiveTapExplosion>,
    ) {
        self.set_tap_explosion(col, explosion);
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.tap_explosions.fill(None);
        self.column_flashes.fill(None);
        self.mine_explosions.fill(None);
        self.tap_mask = 0;
        self.flash_mask = 0;
        self.mine_mask = 0;
    }
}

#[inline(always)]
fn set_feedback_bit(mask: &mut LaneMask, col: usize, active: bool) {
    if col >= LaneMask::BITS as usize {
        return;
    }
    let bit = 1 << col;
    if active {
        *mask |= bit;
    } else {
        *mask &= !bit;
    }
}

#[derive(Clone, Debug)]
pub struct GameplayDisplayRuntimeState {
    pub cue_runtime: GameplayCueRuntimeState,
    pub mini_indicator: GameplayMiniIndicatorRuntimeState,
    pub hold_feedback: GameplayHoldFeedbackState,
    pub beat_phase: GameplayBeatPhaseState,
    pub noteskin_effects: GameplayNoteskinEffects,
    pub active_color_index: i32,
    pub player_color_index: i32,
    pub notefield_motion: GameplayNotefieldMotionState,
    pub receptor_feedback: GameplayReceptorFeedbackState,
    pub visual_feedback: GameplayVisualFeedbackState,
    pub danger_fx: GameplayDangerFxState,
    pub density_graph: GameplayDensityGraphState,
    pub toggle_flash: GameplayToggleFlashState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitTransitionKind {
    Out,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayExit {
    Complete,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayAction {
    None,
    Navigate(GameplayExit),
    NavigateNoFade(GameplayExit),
}

#[derive(Clone, Copy, Debug)]
pub struct ExitTransition {
    pub kind: ExitTransitionKind,
    pub started_at: Instant,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GameplayExitInputState {
    pub hold_to_exit_key: Option<HoldToExitKey>,
    pub hold_to_exit_start: Option<Instant>,
    pub hold_to_exit_aborted_at: Option<Instant>,
    pub exit_transition: Option<ExitTransition>,
    pub shift_held: bool,
    pub ctrl_held: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GameplayExitPromptState {
    pub hold_to_exit_key: Option<HoldToExitKey>,
    pub hold_to_exit_start: Option<Instant>,
    pub hold_to_exit_aborted_at: Option<Instant>,
    pub exit_transition: Option<ExitTransition>,
}

impl GameplayExitInputState {
    #[inline(always)]
    pub fn prompt_state(&self) -> GameplayExitPromptState {
        GameplayExitPromptState {
            hold_to_exit_key: self.hold_to_exit_key,
            hold_to_exit_start: self.hold_to_exit_start,
            hold_to_exit_aborted_at: self.hold_to_exit_aborted_at,
            exit_transition: self.exit_transition,
        }
    }

    #[inline(always)]
    pub fn arm_hold(&mut self, key: HoldToExitKey, at: Instant) {
        self.hold_to_exit_key = Some(key);
        self.hold_to_exit_start = Some(at);
        self.hold_to_exit_aborted_at = None;
    }

    #[inline(always)]
    pub fn abort_hold(&mut self, at: Instant) {
        if self.hold_to_exit_start.is_some() {
            self.hold_to_exit_key = None;
            self.hold_to_exit_start = None;
            self.hold_to_exit_aborted_at = Some(at);
        }
    }

    #[inline(always)]
    pub fn clear_aborted_hold(&mut self) {
        self.hold_to_exit_aborted_at = None;
    }

    #[inline(always)]
    pub fn begin_exit(&mut self, kind: ExitTransitionKind, at: Instant) -> bool {
        if self.exit_transition.is_some() {
            return false;
        }
        self.hold_to_exit_key = None;
        self.hold_to_exit_start = None;
        self.hold_to_exit_aborted_at = None;
        self.exit_transition = Some(ExitTransition {
            kind,
            started_at: at,
        });
        true
    }

    #[inline(always)]
    pub fn clear_exit(&mut self) {
        self.exit_transition = None;
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.hold_to_exit_key = None;
        self.hold_to_exit_start = None;
        self.hold_to_exit_aborted_at = None;
        self.exit_transition = None;
        self.shift_held = false;
        self.ctrl_held = false;
    }
}

#[derive(Clone, Debug)]
pub struct GameplayControlRuntimeState {
    pub exit_input: GameplayExitInputState,
    pub offset_adjust_hold: GameplayOffsetAdjustHoldState,
    pub input_state: GameplayInputState,
    pub autoplay_runtime: GameplayAutoplayRuntimeState,
    pub autosync: GameplayAutosyncRuntimeState,
    pub tick_mode: GameplayTimingTickMode,
    pub assist_clap: GameplayAssistClapState,
    pub update_trace: GameplayUpdateTraceState,
}

pub struct GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta> {
    pub source: GameplaySourceRuntimeState,
    pub setup: GameplaySetupRuntimeState,
    pub boundary: GameplayBoundaryRuntimeState,
    pub timing_runtime: GameplayTimingRuntimeState,
    pub chart_runtime: GameplayChartRuntimeState,
    pub clock: GameplayClockRuntimeState,
    pub hold_runtime: GameplayHoldRuntimeState,
    pub players_runtime: GameplayPlayersRuntimeState,
    pub display: GameplayDisplayRuntimeState,
    pub progress: GameplayProgressRuntimeState,
    pub profiles_runtime: GameplayProfilesRuntimeState<Profile>,
    pub mods: GameplayModRuntimeState<OverlayActor, CapturedActor, StateDelta>,
    pub control: GameplayControlRuntimeState,
    pub pending_input: GameplayPendingInputState<GameplayInputEdge>,
}

pub fn gameplay_runtime_profiles<Profile: GameplayProfileData>(
    player_profiles: &[Profile; MAX_PLAYERS],
    session: &GameplaySession,
) -> [Profile; MAX_PLAYERS] {
    let mut runtime_profiles = (*player_profiles).clone();
    if session.p2_runtime_player() {
        runtime_profiles[0] = runtime_profiles[1].clone();
    }
    runtime_profiles
}

pub fn gameplay_runtime_charts(
    charts: &[Arc<ChartData>; MAX_PLAYERS],
    session: &GameplaySession,
) -> [Arc<ChartData>; MAX_PLAYERS] {
    let mut runtime_charts: [Arc<ChartData>; MAX_PLAYERS] =
        std::array::from_fn(|player| charts[player].clone());
    if session.p2_runtime_player() {
        runtime_charts[0] = runtime_charts[1].clone();
    }
    runtime_charts
}
