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
    pub fn set_range_for_benchmark(&mut self, player: usize, range: (usize, usize)) {
        if let Some(slot) = self.ranges.get_mut(player) {
            *slot = range;
        }
    }

    #[inline(always)]
    pub fn clear_for_benchmark(&mut self) {
        self.ranges.fill((0, 0));
    }
}

#[derive(Clone, Debug)]
pub struct GameplayLaneIndexState {
    pub note_indices: [Vec<usize>; MAX_COLS],
    pub note_row_indices: [Vec<usize>; MAX_COLS],
    pub hold_indices: [Vec<usize>; MAX_COLS],
    pub tap_row_hold_roll_flags: Vec<u8>,
}

impl Default for GameplayLaneIndexState {
    fn default() -> Self {
        Self {
            note_indices: std::array::from_fn(|_| Vec::new()),
            note_row_indices: std::array::from_fn(|_| Vec::new()),
            hold_indices: std::array::from_fn(|_| Vec::new()),
            tap_row_hold_roll_flags: Vec::new(),
        }
    }
}

impl GameplayLaneIndexState {
    pub fn new(
        note_indices: [Vec<usize>; MAX_COLS],
        note_row_indices: [Vec<usize>; MAX_COLS],
        hold_indices: [Vec<usize>; MAX_COLS],
        tap_row_hold_roll_flags: Vec<u8>,
    ) -> Self {
        Self {
            note_indices,
            note_row_indices,
            hold_indices,
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
    pub fn tap_row_hold_roll_flags(&self, note_index: usize) -> u8 {
        self.tap_row_hold_roll_flags
            .get(note_index)
            .copied()
            .unwrap_or(0)
    }

    #[inline(always)]
    pub fn clear_for_benchmark(&mut self) {
        for indices in &mut self.note_indices {
            indices.clear();
        }
        for indices in &mut self.note_row_indices {
            indices.clear();
        }
        for indices in &mut self.hold_indices {
            indices.clear();
        }
        self.tap_row_hold_roll_flags.clear();
    }
}

#[derive(Clone, Debug)]
pub struct GameplayRowIndexState {
    pub row_entry_ranges: [(usize, usize); MAX_PLAYERS],
    pub judged_row_cursor: [usize; MAX_PLAYERS],
    pub row_map_cache: [Vec<u32>; MAX_PLAYERS],
    pub note_row_entry_indices: Vec<u32>,
}

impl Default for GameplayRowIndexState {
    fn default() -> Self {
        Self {
            row_entry_ranges: [(0, 0); MAX_PLAYERS],
            judged_row_cursor: [0; MAX_PLAYERS],
            row_map_cache: std::array::from_fn(|_| Vec::new()),
            note_row_entry_indices: Vec::new(),
        }
    }
}

impl GameplayRowIndexState {
    pub fn new(
        row_entry_ranges: [(usize, usize); MAX_PLAYERS],
        judged_row_cursor: [usize; MAX_PLAYERS],
        row_map_cache: [Vec<u32>; MAX_PLAYERS],
        note_row_entry_indices: Vec<u32>,
    ) -> Self {
        Self {
            row_entry_ranges,
            judged_row_cursor,
            row_map_cache,
            note_row_entry_indices,
        }
    }

    #[inline(always)]
    pub fn clear_for_benchmark(&mut self) {
        self.row_entry_ranges.fill((0, 0));
        self.judged_row_cursor.fill(0);
        for row_map_cache in &mut self.row_map_cache {
            row_map_cache.clear();
        }
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
        Self {
            next_tap_miss_cursor: note_range_start,
            next_mine_avoid_cursor: note_range_start,
            mine_note_ix,
            mine_note_time_ns,
            next_mine_ix_cursor: [0; MAX_PLAYERS],
            pending_mine_hit_indices: Vec::new(),
        }
    }

    #[inline(always)]
    pub fn set_next_tap_miss_cursor(&mut self, player: usize, cursor: usize) {
        if let Some(slot) = self.next_tap_miss_cursor.get_mut(player) {
            *slot = cursor;
        }
    }

    #[inline(always)]
    pub fn clear_for_benchmark(&mut self) {
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
    pub note_ranges: GameplayNoteRangeState,
    pub note_count_stats: GameplayNoteCountStatsState,
    pub lane_indices: GameplayLaneIndexState,
    pub row_indices: GameplayRowIndexState,
    pub note_time_cache_ns: Vec<SongTimeNs>,
    pub hold_end_time_cache_ns: Vec<Option<SongTimeNs>>,
    pub mine_scan: GameplayMineScanState,
    pub row_entries: Vec<RowEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct GameplayHoldRuntimeState {
    pub active_holds: [Option<ActiveHold>; MAX_COLS],
    pub decaying_hold_indices: Vec<usize>,
    pub hold_decay_active: Vec<bool>,
    pub tap_miss_held_window: Vec<bool>,
    pub pending_missed_hold_resolution: Vec<bool>,
    pub pending_missed_hold_indices: Vec<usize>,
}

impl GameplayHoldRuntimeState {
    pub fn new(notes_len: usize, decaying_hold_capacity: usize) -> Self {
        Self {
            active_holds: std::array::from_fn(|_| None),
            decaying_hold_indices: Vec::with_capacity(decaying_hold_capacity),
            hold_decay_active: vec![false; notes_len],
            tap_miss_held_window: vec![false; notes_len],
            pending_missed_hold_resolution: vec![false; notes_len],
            pending_missed_hold_indices: Vec::new(),
        }
    }

    #[inline(always)]
    pub fn reset_live_state(&mut self) {
        self.active_holds.fill(None);
        self.decaying_hold_indices.clear();
        self.hold_decay_active.fill(false);
        self.tap_miss_held_window.fill(false);
        self.pending_missed_hold_resolution.fill(false);
        self.pending_missed_hold_indices.clear();
    }

    #[inline(always)]
    pub fn clear_for_benchmark(&mut self) {
        self.active_holds.fill(None);
        self.decaying_hold_indices.clear();
        self.hold_decay_active.clear();
        self.tap_miss_held_window.clear();
        self.pending_missed_hold_resolution.clear();
        self.pending_missed_hold_indices.clear();
    }
}

#[derive(Clone, Debug)]
pub struct GameplayCueRuntimeState {
    measure_counter_segments: [Vec<StreamSegment>; MAX_PLAYERS],
    column_cues: [Vec<ColumnCue>; MAX_PLAYERS],
    crossover_cues: [Vec<ColumnCue>; MAX_PLAYERS],
}

impl Default for GameplayCueRuntimeState {
    fn default() -> Self {
        Self {
            measure_counter_segments: std::array::from_fn(|_| Vec::new()),
            column_cues: std::array::from_fn(|_| Vec::new()),
            crossover_cues: std::array::from_fn(|_| Vec::new()),
        }
    }
}

impl GameplayCueRuntimeState {
    pub fn new(
        measure_counter_segments: [Vec<StreamSegment>; MAX_PLAYERS],
        column_cues: [Vec<ColumnCue>; MAX_PLAYERS],
        crossover_cues: [Vec<ColumnCue>; MAX_PLAYERS],
    ) -> Self {
        Self {
            measure_counter_segments,
            column_cues,
            crossover_cues,
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
    pub fn crossover_cues(&self, player: usize) -> &[ColumnCue] {
        self.crossover_cues.get(player).map_or(&[], Vec::as_slice)
    }

    #[inline(always)]
    pub fn set_column_cues_for_benchmark(&mut self, player: usize, cues: Vec<ColumnCue>) {
        if let Some(slot) = self.column_cues.get_mut(player) {
            *slot = cues;
        }
    }

    #[inline(always)]
    pub fn clear_for_benchmark(&mut self) {
        for segments in &mut self.measure_counter_segments {
            segments.clear();
        }
        for cues in &mut self.column_cues {
            cues.clear();
        }
        for cues in &mut self.crossover_cues {
            cues.clear();
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GameplayHoldFeedbackState {
    pub hold_judgments: [Option<HoldJudgmentRenderInfo>; MAX_COLS],
    pub held_miss_judgments: [Option<HeldMissRenderInfo>; MAX_COLS],
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
    pub fn clear(&mut self) {
        self.hold_judgments.fill(None);
        self.held_miss_judgments.fill(None);
    }
}

#[derive(Clone, Debug, Default)]
pub struct GameplayVisualFeedbackState {
    pub tap_explosions: [Option<ActiveTapExplosion>; MAX_COLS],
    pub column_flashes: [Option<ActiveColumnFlash>; MAX_COLS],
    pub last_tap_judgments: [Option<ColumnTapJudgment>; MAX_COLS],
    pub mine_explosions: [Option<ActiveMineExplosion>; MAX_COLS],
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
    pub fn set_tap_explosion_for_benchmark(
        &mut self,
        col: usize,
        explosion: Option<ActiveTapExplosion>,
    ) {
        if let Some(slot) = self.tap_explosions.get_mut(col) {
            *slot = explosion;
        }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.tap_explosions.fill(None);
        self.column_flashes.fill(None);
        self.mine_explosions.fill(None);
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

