#[inline(always)]
pub fn quantization_index_from_beat(beat: f32) -> u8 {
    // Match ITG's BeatToNoteType path: round beat->row at 48 rows/beat,
    // then classify by measure-subdivision divisibility.
    let row = (beat * 48.0).round() as i32;
    if row.rem_euclid(48) == 0 {
        QUANT_4TH
    } else if row.rem_euclid(24) == 0 {
        QUANT_8TH
    } else if row.rem_euclid(16) == 0 {
        QUANT_12TH
    } else if row.rem_euclid(12) == 0 {
        QUANT_16TH
    } else if row.rem_euclid(8) == 0 {
        QUANT_24TH
    } else if row.rem_euclid(6) == 0 {
        QUANT_32ND
    } else if row.rem_euclid(4) == 0 {
        QUANT_48TH
    } else if row.rem_euclid(3) == 0 {
        QUANT_64TH
    } else {
        QUANT_192ND
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordedLaneEdge {
    pub lane_index: u8,
    pub pressed: bool,
    pub source: InputSource,
    pub event_music_time_ns: SongTimeNs,
}

#[derive(Clone, Copy, Debug)]
pub struct ReplayInputEdge {
    pub lane_index: u8,
    pub pressed: bool,
    pub source: InputSource,
    pub event_music_time_ns: SongTimeNs,
}

#[derive(Clone, Copy, Debug)]
pub struct ReplayOffsetSnapshot {
    pub beat0_time_ns: SongTimeNs,
}

pub fn build_replay_input_edges(
    replay_edges: &[ReplayInputEdge],
    num_players: usize,
    cols_per_player: usize,
    num_cols: usize,
    recorded_beat0_time_ns: SongTimeNs,
    current_beat0_time_ns: [SongTimeNs; MAX_PLAYERS],
) -> Vec<RecordedLaneEdge> {
    let mut replay_input = Vec::with_capacity(replay_edges.len());
    let mut out_of_order = false;
    let mut prev_time_ns = None;

    for edge in replay_edges {
        let lane = edge.lane_index as usize;
        if lane >= num_cols || song_time_ns_invalid(edge.event_music_time_ns) {
            continue;
        }

        let player = player_index_for_column(num_players, cols_per_player, lane);
        let player_beat0_time_ns = current_beat0_time_ns[player];
        let replay_beat0_shift_ns = if song_time_ns_invalid(recorded_beat0_time_ns)
            || song_time_ns_invalid(player_beat0_time_ns)
        {
            0
        } else {
            player_beat0_time_ns.saturating_sub(recorded_beat0_time_ns)
        };
        let event_music_time_ns = edge
            .event_music_time_ns
            .saturating_add(replay_beat0_shift_ns);

        if prev_time_ns.is_some_and(|prev| event_music_time_ns < prev) {
            out_of_order = true;
        }
        prev_time_ns = Some(event_music_time_ns);
        replay_input.push(RecordedLaneEdge {
            lane_index: edge.lane_index,
            pressed: edge.pressed,
            source: edge.source,
            event_music_time_ns,
        });
    }

    if out_of_order {
        replay_input.sort_by_key(|edge| edge.event_music_time_ns);
    }
    replay_input
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameplayReplayInputState {
    input: Vec<RecordedLaneEdge>,
    cursor: usize,
}

impl GameplayReplayInputState {
    #[inline(always)]
    pub fn new(input: Vec<RecordedLaneEdge>) -> Self {
        Self { input, cursor: 0 }
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.input.is_empty()
    }

    #[inline(always)]
    pub fn reset_cursor(&mut self) {
        self.cursor = 0;
    }

    #[inline(always)]
    pub fn collect_ready(
        &mut self,
        current_music_time_ns: SongTimeNs,
        num_cols: usize,
        events: &mut [Option<RecordedLaneEdge>],
    ) -> usize {
        collect_ready_replay_edges(
            &self.input,
            &mut self.cursor,
            current_music_time_ns,
            num_cols,
            events,
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameplayReplayRuntimeState {
    pub mode: bool,
    pub capture_enabled: bool,
    pub input: GameplayReplayInputState,
    pub edges: Vec<RecordedLaneEdge>,
}

impl GameplayReplayRuntimeState {
    #[inline(always)]
    pub fn new(
        input: GameplayReplayInputState,
        capture_enabled: bool,
        edge_capacity: usize,
    ) -> Self {
        let mode = !input.is_empty();
        Self {
            mode,
            capture_enabled,
            input,
            edges: Vec::with_capacity(edge_capacity),
        }
    }

    #[inline(always)]
    pub fn disable_replay_mode(&mut self) {
        self.mode = false;
        self.capture_enabled = false;
    }

    #[inline(always)]
    pub fn reset_for_restart(&mut self) {
        self.edges.clear();
        self.input.reset_cursor();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameplayToggleFlashState {
    pub text: Option<&'static str>,
    pub timer: f32,
}

impl GameplayToggleFlashState {
    #[inline(always)]
    pub fn visible_text(&self) -> Option<(&'static str, f32)> {
        toggle_flash_alpha(self.timer).and_then(|alpha| self.text.map(|text| (text, alpha)))
    }

    #[inline(always)]
    pub fn tick(&mut self, delta_time: f32) {
        tick_positive_timer(&mut self.timer, delta_time);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameplayStageRuntimeState {
    pub song_completed_naturally: bool,
    pub autoplay_enabled: bool,
    pub autoplay_used: bool,
    pub score_valid: [bool; MAX_PLAYERS],
    pub score_missed_holds_rolls: [bool; MAX_PLAYERS],
}

impl GameplayStageRuntimeState {
    #[inline(always)]
    pub const fn new(
        autoplay_enabled: bool,
        autoplay_used: bool,
        score_valid: [bool; MAX_PLAYERS],
        score_missed_holds_rolls: [bool; MAX_PLAYERS],
    ) -> Self {
        Self {
            song_completed_naturally: false,
            autoplay_enabled,
            autoplay_used,
            score_valid,
            score_missed_holds_rolls,
        }
    }

    #[inline(always)]
    pub fn disable_score(&mut self) {
        self.score_valid = [false; MAX_PLAYERS];
    }

    #[inline(always)]
    pub fn reset_for_practice(&mut self) {
        self.song_completed_naturally = false;
        self.autoplay_used = false;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayAutosyncRuntimeState {
    pub mode: AutosyncMode,
    pub offset_samples: [SongTimeNs; AUTOSYNC_OFFSET_SAMPLE_COUNT],
    pub offset_sample_count: usize,
    pub standard_deviation: f32,
}

impl Default for GameplayAutosyncRuntimeState {
    fn default() -> Self {
        Self {
            mode: AutosyncMode::Off,
            offset_samples: [0; AUTOSYNC_OFFSET_SAMPLE_COUNT],
            offset_sample_count: 0,
            standard_deviation: 0.0,
        }
    }
}

impl GameplayAutosyncRuntimeState {
    #[inline(always)]
    pub fn apply_offset_sample(&mut self, note_off_by_ns: SongTimeNs) -> AutosyncSampleResult {
        let result = apply_autosync_offset_sample(
            &mut self.offset_samples,
            &mut self.offset_sample_count,
            self.mode,
            note_off_by_ns,
        );
        if let Some(stddev) = result.standard_deviation {
            self.standard_deviation = stddev;
        }
        result
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayChartTotalsState {
    pub possible_grade_points: [i32; MAX_PLAYERS],
    pub total_steps: [u32; MAX_PLAYERS],
    pub holds_total: [u32; MAX_PLAYERS],
    pub rolls_total: [u32; MAX_PLAYERS],
    pub mines_total: [u32; MAX_PLAYERS],
    pub hands_total: [u32; MAX_PLAYERS],
}

impl GameplayChartTotalsState {
    #[inline(always)]
    pub const fn new(
        possible_grade_points: [i32; MAX_PLAYERS],
        total_steps: [u32; MAX_PLAYERS],
        holds_total: [u32; MAX_PLAYERS],
        rolls_total: [u32; MAX_PLAYERS],
        mines_total: [u32; MAX_PLAYERS],
        hands_total: [u32; MAX_PLAYERS],
    ) -> Self {
        Self {
            possible_grade_points,
            total_steps,
            holds_total,
            rolls_total,
            mines_total,
            hands_total,
        }
    }

    #[inline(always)]
    pub fn display_totals(
        &self,
        totals: Option<&[CourseDisplayTotals; MAX_PLAYERS]>,
        player_idx: usize,
    ) -> CourseDisplayTotals {
        course_display_totals_for_player(
            totals,
            &self.possible_grade_points,
            &self.total_steps,
            &self.holds_total,
            &self.rolls_total,
            &self.mines_total,
            player_idx,
        )
    }
}

#[derive(Clone, Debug)]
pub struct GameplayProgressRuntimeState {
    pub chart_totals: GameplayChartTotalsState,
    pub stage: GameplayStageRuntimeState,
    pub replay: GameplayReplayRuntimeState,
    pub course_display: GameplayCourseDisplayState,
    pub window_counts: GameplayWindowCountsState,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayVisibleTimingState {
    pub global_visual_delay_seconds: f32,
    pub player_visual_delay_seconds: [f32; MAX_PLAYERS],
    pub current_music_time_ns: [SongTimeNs; MAX_PLAYERS],
    pub current_music_time: [f32; MAX_PLAYERS],
    pub current_beat: [f32; MAX_PLAYERS],
}

impl Default for GameplayVisibleTimingState {
    fn default() -> Self {
        Self {
            global_visual_delay_seconds: 0.0,
            player_visual_delay_seconds: [0.0; MAX_PLAYERS],
            current_music_time_ns: [0; MAX_PLAYERS],
            current_music_time: [0.0; MAX_PLAYERS],
            current_beat: [0.0; MAX_PLAYERS],
        }
    }
}

impl GameplayVisibleTimingState {
    #[inline(always)]
    pub fn visual_delay_seconds(&self, player: usize) -> f32 {
        self.global_visual_delay_seconds
            + self
                .player_visual_delay_seconds
                .get(player)
                .copied()
                .unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn set_player_time(
        &mut self,
        player: usize,
        music_time_ns: SongTimeNs,
        music_time_seconds: f32,
        beat: f32,
    ) {
        if player >= MAX_PLAYERS {
            return;
        }
        self.current_music_time_ns[player] = music_time_ns;
        self.current_music_time[player] = music_time_seconds;
        self.current_beat[player] = beat;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayNotefieldMotionState {
    scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
    scroll_reference_bpm: f32,
    field_zoom: [f32; MAX_PLAYERS],
    scroll_pixels_per_second: [f32; MAX_PLAYERS],
    scroll_travel_time: [f32; MAX_PLAYERS],
    draw_distance_before_targets: [f32; MAX_PLAYERS],
    draw_distance_after_targets: [f32; MAX_PLAYERS],
    reverse_scroll: [bool; MAX_PLAYERS],
    column_scroll_dirs: [f32; MAX_COLS],
}

impl Default for GameplayNotefieldMotionState {
    fn default() -> Self {
        Self {
            scroll_speed: [ScrollSpeedSetting::default(); MAX_PLAYERS],
            scroll_reference_bpm: 0.0,
            field_zoom: [1.0; MAX_PLAYERS],
            scroll_pixels_per_second: [0.0; MAX_PLAYERS],
            scroll_travel_time: [0.0; MAX_PLAYERS],
            draw_distance_before_targets: [0.0; MAX_PLAYERS],
            draw_distance_after_targets: [0.0; MAX_PLAYERS],
            reverse_scroll: [false; MAX_PLAYERS],
            column_scroll_dirs: [1.0; MAX_COLS],
        }
    }
}

impl GameplayNotefieldMotionState {
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn new(
        scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
        scroll_reference_bpm: f32,
        field_zoom: [f32; MAX_PLAYERS],
        scroll_pixels_per_second: [f32; MAX_PLAYERS],
        scroll_travel_time: [f32; MAX_PLAYERS],
        draw_distance_before_targets: [f32; MAX_PLAYERS],
        draw_distance_after_targets: [f32; MAX_PLAYERS],
        reverse_scroll: [bool; MAX_PLAYERS],
        column_scroll_dirs: [f32; MAX_COLS],
    ) -> Self {
        Self {
            scroll_speed,
            scroll_reference_bpm,
            field_zoom,
            scroll_pixels_per_second,
            scroll_travel_time,
            draw_distance_before_targets,
            draw_distance_after_targets,
            reverse_scroll,
            column_scroll_dirs,
        }
    }

    #[inline(always)]
    pub fn scroll_speed(&self, player: usize) -> ScrollSpeedSetting {
        self.scroll_speed.get(player).copied().unwrap_or_default()
    }

    #[inline(always)]
    pub fn scroll_reference_bpm(&self) -> f32 {
        self.scroll_reference_bpm
    }

    #[inline(always)]
    pub fn field_zoom(&self, player: usize) -> f32 {
        self.field_zoom.get(player).copied().unwrap_or(1.0)
    }

    #[inline(always)]
    pub fn scroll_pixels_per_second(&self, player: usize) -> f32 {
        self.scroll_pixels_per_second
            .get(player)
            .copied()
            .unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn scroll_travel_time(&self, player: usize) -> f32 {
        self.scroll_travel_time.get(player).copied().unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn draw_distance_before_targets(&self, player: usize) -> f32 {
        self.draw_distance_before_targets
            .get(player)
            .copied()
            .unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn draw_distance_after_targets(&self, player: usize) -> f32 {
        self.draw_distance_after_targets
            .get(player)
            .copied()
            .unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn reverse_scroll(&self, player: usize) -> bool {
        self.reverse_scroll.get(player).copied().unwrap_or(false)
    }

    #[inline(always)]
    pub fn column_scroll_dir(&self, col: usize) -> f32 {
        self.column_scroll_dirs.get(col).copied().unwrap_or(1.0)
    }

    #[inline(always)]
    pub const fn column_scroll_dir_count(&self) -> usize {
        MAX_COLS
    }

    #[inline(always)]
    pub fn set_reverse_scroll(&mut self, player: usize, reverse: bool) {
        if player < MAX_PLAYERS {
            self.reverse_scroll[player] = reverse;
        }
    }

    #[inline(always)]
    pub fn set_column_scroll_dir(&mut self, col: usize, direction: f32) {
        if col < MAX_COLS {
            self.column_scroll_dirs[col] = direction;
        }
    }

    #[inline(always)]
    pub fn set_player_motion(
        &mut self,
        player: usize,
        scroll_pixels_per_second: f32,
        field_zoom: f32,
        draw_distance_before_targets: f32,
        draw_distance_after_targets: f32,
        scroll_travel_time: f32,
    ) {
        if player >= MAX_PLAYERS {
            return;
        }
        self.scroll_pixels_per_second[player] = scroll_pixels_per_second;
        self.field_zoom[player] = field_zoom;
        self.draw_distance_before_targets[player] = draw_distance_before_targets;
        self.draw_distance_after_targets[player] = draw_distance_after_targets;
        self.scroll_travel_time[player] = scroll_travel_time;
    }
}

pub fn next_ready_replay_edge(
    replay_input: &[RecordedLaneEdge],
    replay_cursor: &mut usize,
    current_music_time_ns: SongTimeNs,
) -> Option<RecordedLaneEdge> {
    let edge = replay_input.get(*replay_cursor).copied()?;
    if edge.event_music_time_ns > current_music_time_ns {
        return None;
    }
    *replay_cursor = replay_cursor.saturating_add(1);
    Some(edge)
}

pub fn collect_ready_replay_edges(
    replay_input: &[RecordedLaneEdge],
    replay_cursor: &mut usize,
    current_music_time_ns: SongTimeNs,
    num_cols: usize,
    events: &mut [Option<RecordedLaneEdge>],
) -> usize {
    let mut event_count = 0usize;
    while event_count < events.len() {
        let Some(edge) = next_ready_replay_edge(replay_input, replay_cursor, current_music_time_ns)
        else {
            break;
        };
        if edge.lane_index as usize >= num_cols {
            continue;
        }
        events[event_count] = Some(edge);
        event_count += 1;
    }
    event_count
}

