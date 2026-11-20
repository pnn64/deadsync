#[inline(always)]
const fn build_quantization_table() -> [u8; 48] {
    let mut table = [QUANT_192ND; 48];
    let mut row = 0usize;
    while row < table.len() {
        table[row] = if row % 48 == 0 {
            QUANT_4TH
        } else if row % 24 == 0 {
            QUANT_8TH
        } else if row % 16 == 0 {
            QUANT_12TH
        } else if row % 12 == 0 {
            QUANT_16TH
        } else if row % 8 == 0 {
            QUANT_24TH
        } else if row % 6 == 0 {
            QUANT_32ND
        } else if row % 4 == 0 {
            QUANT_48TH
        } else if row % 3 == 0 {
            QUANT_64TH
        } else {
            QUANT_192ND
        };
        row += 1;
    }
    table
}

const QUANTIZATION_BY_ROW: [u8; 48] = build_quantization_table();

#[inline(always)]
#[must_use]
pub fn quantization_index_from_beat(beat: f32) -> u8 {
    // Widen the f32 product so the half bias cannot cross a rounding boundary.
    // The integer cast supplies truncation, saturation and NaN-to-zero behavior.
    let scaled = f64::from(beat * 48.0);
    let row = (scaled + 0.5f64.copysign(scaled)) as i32;
    QUANTIZATION_BY_ROW[row.rem_euclid(48) as usize]
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

#[derive(Clone, Copy, Debug)]
struct ReplayInputTransition {
    raw_music_time_ns: SongTimeNs,
    reported_music_time_ns: SongTimeNs,
    pressed: bool,
}

/// Legacy replay edges retain source/lane identity and raw song time, but not
/// physical binding IDs, original delivery time, pauses, or preference history.
/// Keep the available identities separate and use playback delivery time for
/// cleanup; ages themselves always come from the recorded raw timestamp.
#[derive(Clone, Debug)]
struct ReplayInputAgeState {
    current: [[Option<ReplayInputTransition>; 2]; MAX_COLS],
    snapshot: [[Option<ReplayInputTransition>; 2]; MAX_COLS],
}

impl Default for ReplayInputAgeState {
    fn default() -> Self {
        Self {
            current: [[None; 2]; MAX_COLS],
            snapshot: [[None; 2]; MAX_COLS],
        }
    }
}

impl ReplayInputAgeState {
    fn clear(&mut self) {
        self.current.fill([None; 2]);
        self.snapshot.fill([None; 2]);
    }

    fn record(&mut self, edge: RecordedLaneEdge, reported_music_time_ns: SongTimeNs) {
        if song_time_ns_invalid(edge.event_music_time_ns)
            || song_time_ns_invalid(reported_music_time_ns)
        {
            return;
        }
        let Some(lane) = self.current.get_mut(edge.lane_index as usize) else {
            return;
        };
        let source = match edge.source {
            InputSource::Keyboard => 0,
            InputSource::Gamepad => 1,
        };
        lane[source] = Some(ReplayInputTransition {
            raw_music_time_ns: edge.event_music_time_ns,
            reported_music_time_ns,
            pressed: edge.pressed,
        });
    }

    fn begin_snapshot(
        &mut self,
        music_time_ns: SongTimeNs,
        music_rate: f32,
        debounce_seconds: f32,
    ) {
        if song_time_ns_invalid(music_time_ns) {
            self.snapshot.fill([None; 2]);
            return;
        }
        self.snapshot = self.current;
        let rate = normalized_song_rate(music_rate);
        let debounce = if debounce_seconds.is_finite() {
            debounce_seconds.max(0.0)
        } else {
            0.0
        };
        for lane in &mut self.current {
            for transition in lane {
                if transition.is_some_and(|input| {
                    !input.pressed
                        && song_time_ns_to_seconds(
                            music_time_ns.saturating_sub(input.reported_music_time_ns),
                        ) / rate > debounce
                }) {
                    // Native MakeButtonStateList precedes map pruning, so this
                    // released timestamp remains visible for this whole update.
                    *transition = None;
                }
            }
        }
    }

    fn age_seconds(&self, column: usize, music_time_ns: SongTimeNs, music_rate: f32) -> f32 {
        if song_time_ns_invalid(music_time_ns) {
            return 0.0;
        }
        let Some(lane) = self.snapshot.get(column) else {
            return 0.0;
        };
        let rate = normalized_song_rate(music_rate);
        lane.iter().flatten().fold(0.0_f32, |age, input| {
            age.max(
                song_time_ns_to_seconds(music_time_ns.saturating_sub(input.raw_music_time_ns))
                    .max(0.0)
                    / rate,
            )
        })
    }
}

impl<Profile, OverlayActor, CapturedActor, StateDelta>
    GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>
where
    Profile: GameplayProfileData,
{
    fn record_replay_input_age(&mut self, edge: RecordedLaneEdge) {
        let reported = self.clock.song_position.current_music_time_ns;
        self.control.input_state.replay_input_ages.record(edge, reported);
        let width = self.setup.cols_per_player;
        if self.setup.session.play_style.is_routine() && (edge.lane_index as usize) < width {
            self.control.input_state.replay_input_ages.record(
                RecordedLaneEdge {
                    lane_index: edge.lane_index + width as u8,
                    ..edge
                },
                reported,
            );
        }
    }

    /// Snapshot replay input after processing its edges and before crossing rows.
    /// The format has no recorded debounce preference, so use the playback value.
    pub fn begin_replay_input_snapshot(
        &mut self,
        music_time_ns: SongTimeNs,
        input_debounce_seconds: f32,
    ) {
        let rate = self.music_rate();
        self.control.input_state.replay_input_ages.begin_snapshot(
            music_time_ns,
            rate,
            input_debounce_seconds,
        );
    }

    #[must_use]
    pub fn replay_input_age_seconds(&self, column: usize, music_time_ns: SongTimeNs) -> f32 {
        self.control.input_state.replay_input_ages.age_seconds(
            column,
            music_time_ns,
            self.music_rate(),
        )
    }
}

#[must_use]
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

/// Prepare an owned replay without allocating a second edge buffer. Both edge
/// types have the same size/alignment, so the consuming filter/map can reuse the input
/// allocation. Equal-time edges retain their original order when sorting.
#[must_use]
pub fn build_replay_input_edges_owned(
    replay_edges: Vec<ReplayInputEdge>,
    num_players: usize,
    cols_per_player: usize,
    num_cols: usize,
    recorded_beat0_time_ns: SongTimeNs,
    current_beat0_time_ns: [SongTimeNs; MAX_PLAYERS],
) -> Vec<RecordedLaneEdge> {
    let shifts = current_beat0_time_ns.map(|current| {
        if song_time_ns_invalid(recorded_beat0_time_ns) || song_time_ns_invalid(current) {
            0
        } else {
            current.saturating_sub(recorded_beat0_time_ns)
        }
    });
    let mut previous = None;
    let mut out_of_order = false;
    let mut replay_input: Vec<_> = replay_edges
        .into_iter()
        .filter_map(|edge| {
            let lane = edge.lane_index as usize;
            if lane >= num_cols || song_time_ns_invalid(edge.event_music_time_ns) {
                return None;
            }
            let player = player_index_for_column(num_players, cols_per_player, lane);
            let time = edge.event_music_time_ns.saturating_add(shifts[player]);
            out_of_order |= previous.is_some_and(|previous| time < previous);
            previous = Some(time);
            Some(RecordedLaneEdge {
                lane_index: edge.lane_index,
                pressed: edge.pressed,
                source: edge.source,
                event_music_time_ns: time,
            })
        })
        .collect();
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
    #[must_use]
    pub const fn new(input: Vec<RecordedLaneEdge>) -> Self {
        Self { input, cursor: 0 }
    }

    #[inline(always)]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.input.is_empty()
    }

    #[inline(always)]
    pub const fn reset_cursor(&mut self) {
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
    #[must_use]
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
    pub const fn disable_replay_mode(&mut self) {
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
    #[must_use]
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
    pub cancelled: bool,
    pub autoplay_enabled: bool,
    pub autoplay_used: bool,
    pub score_valid: [bool; MAX_PLAYERS],
    pub score_missed_holds_rolls: [bool; MAX_PLAYERS],
}

impl GameplayStageRuntimeState {
    #[inline(always)]
    #[must_use]
    pub const fn new(
        autoplay_enabled: bool,
        autoplay_used: bool,
        score_valid: [bool; MAX_PLAYERS],
        score_missed_holds_rolls: [bool; MAX_PLAYERS],
    ) -> Self {
        Self {
            song_completed_naturally: false,
            cancelled: false,
            autoplay_enabled,
            autoplay_used,
            score_valid,
            score_missed_holds_rolls,
        }
    }

    #[inline(always)]
    pub const fn disable_score(&mut self) {
        self.score_valid = [false; MAX_PLAYERS];
    }

    #[inline(always)]
    pub const fn reset_for_practice(&mut self) {
        self.song_completed_naturally = false;
        self.cancelled = false;
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
    #[must_use]
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
    #[must_use]
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

    #[inline(always)]
    #[must_use]
    pub fn stage_totals(&self, player_idx: usize) -> CourseDisplayTotals {
        self.display_totals(None, player_idx)
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

#[derive(Clone, Copy, Debug)]
pub struct GameplayVisibleTimingState {
    pub global_visual_delay_seconds: f32,
    pub player_visual_delay_seconds: [f32; MAX_PLAYERS],
    pub current_music_time_ns: [SongTimeNs; MAX_PLAYERS],
    pub current_music_time: [f32; MAX_PLAYERS],
    pub current_beat: [f32; MAX_PLAYERS],
    /// Fixed per-player song-position snapshots produced by the existing
    /// song-lifetime timing caches. The game thread replaces each active slot
    /// once per clock update; readers perform O(1) copies with no allocation,
    /// synchronization, lookup miss, eviction, or independent timing walk.
    pub notefield_song_position: [BeatInfo; MAX_PLAYERS],
}

impl PartialEq for GameplayVisibleTimingState {
    fn eq(&self, other: &Self) -> bool {
        self.global_visual_delay_seconds == other.global_visual_delay_seconds
            && self.player_visual_delay_seconds == other.player_visual_delay_seconds
            && self.current_music_time_ns == other.current_music_time_ns
            && self.current_music_time == other.current_music_time
            && self.current_beat == other.current_beat
            && self
                .notefield_song_position
                .iter()
                .zip(other.notefield_song_position.iter())
                .all(|(a, b)| {
                    a.beat == b.beat
                        && a.bpm == b.bpm
                        && a.is_in_freeze == b.is_in_freeze
                        && a.is_in_delay == b.is_in_delay
                })
    }
}

impl Default for GameplayVisibleTimingState {
    fn default() -> Self {
        Self {
            global_visual_delay_seconds: 0.0,
            player_visual_delay_seconds: [0.0; MAX_PLAYERS],
            current_music_time_ns: [0; MAX_PLAYERS],
            current_music_time: [0.0; MAX_PLAYERS],
            current_beat: [0.0; MAX_PLAYERS],
            notefield_song_position: [BeatInfo::default(); MAX_PLAYERS],
        }
    }
}

impl GameplayVisibleTimingState {
    #[inline(always)]
    #[must_use]
    pub fn visual_delay_seconds(&self, player: usize) -> f32 {
        self.global_visual_delay_seconds
            + self
                .player_visual_delay_seconds
                .get(player)
                .copied()
                .unwrap_or(0.0)
    }

    #[inline(always)]
    pub const fn set_player_time(
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
    reverse_scroll: [bool; MAX_PLAYERS],
    column_scroll_dirs: [f32; MAX_COLS],
    refresh_bpm_bits: u32,
    refresh_dirty: bool,
}

impl Default for GameplayNotefieldMotionState {
    fn default() -> Self {
        Self {
            scroll_speed: [ScrollSpeedSetting::default(); MAX_PLAYERS],
            scroll_reference_bpm: 0.0,
            field_zoom: [1.0; MAX_PLAYERS],
            scroll_pixels_per_second: [0.0; MAX_PLAYERS],
            scroll_travel_time: [0.0; MAX_PLAYERS],
            reverse_scroll: [false; MAX_PLAYERS],
            column_scroll_dirs: [1.0; MAX_COLS],
            refresh_bpm_bits: 0,
            refresh_dirty: true,
        }
    }
}

impl GameplayNotefieldMotionState {
    #[inline(always)]
    #[must_use]
    pub const fn new(
        scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
        scroll_reference_bpm: f32,
        field_zoom: [f32; MAX_PLAYERS],
        scroll_pixels_per_second: [f32; MAX_PLAYERS],
        scroll_travel_time: [f32; MAX_PLAYERS],
        reverse_scroll: [bool; MAX_PLAYERS],
        column_scroll_dirs: [f32; MAX_COLS],
    ) -> Self {
        Self {
            scroll_speed,
            scroll_reference_bpm,
            field_zoom,
            scroll_pixels_per_second,
            scroll_travel_time,
            reverse_scroll,
            column_scroll_dirs,
            refresh_bpm_bits: 0,
            refresh_dirty: true,
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn refresh_needed(&self, current_bpm: f32, dynamic_motion: bool) -> bool {
        self.refresh_dirty || dynamic_motion || self.refresh_bpm_bits != current_bpm.to_bits()
    }

    #[inline(always)]
    pub const fn mark_refreshed(&mut self, current_bpm: f32) {
        self.refresh_bpm_bits = current_bpm.to_bits();
        self.refresh_dirty = false;
    }

    #[inline(always)]
    pub const fn mark_refresh_dirty(&mut self) {
        self.refresh_dirty = true;
    }

    #[inline(always)]
    #[must_use]
    pub fn scroll_speed(&self, player: usize) -> ScrollSpeedSetting {
        self.scroll_speed.get(player).copied().unwrap_or_default()
    }

    #[inline(always)]
    #[must_use]
    pub const fn scroll_reference_bpm(&self) -> f32 {
        self.scroll_reference_bpm
    }

    #[inline(always)]
    #[must_use]
    pub fn field_zoom(&self, player: usize) -> f32 {
        self.field_zoom.get(player).copied().unwrap_or(1.0)
    }

    #[inline(always)]
    #[must_use]
    pub fn scroll_pixels_per_second(&self, player: usize) -> f32 {
        self.scroll_pixels_per_second
            .get(player)
            .copied()
            .unwrap_or(0.0)
    }

    #[inline(always)]
    #[must_use]
    pub fn scroll_travel_time(&self, player: usize) -> f32 {
        self.scroll_travel_time.get(player).copied().unwrap_or(0.0)
    }

    #[inline(always)]
    #[must_use]
    pub fn reverse_scroll(&self, player: usize) -> bool {
        self.reverse_scroll.get(player).copied().unwrap_or(false)
    }

    #[inline(always)]
    #[must_use]
    pub fn column_scroll_dir(&self, col: usize) -> f32 {
        self.column_scroll_dirs.get(col).copied().unwrap_or(1.0)
    }

    #[inline(always)]
    #[must_use]
    pub const fn column_scroll_dir_count(&self) -> usize {
        MAX_COLS
    }

    #[inline(always)]
    pub const fn set_reverse_scroll(&mut self, player: usize, reverse: bool) {
        if player < MAX_PLAYERS {
            self.reverse_scroll[player] = reverse;
        }
    }

    #[inline(always)]
    pub const fn set_column_scroll_dir(&mut self, col: usize, direction: f32) {
        if col < MAX_COLS {
            self.column_scroll_dirs[col] = direction;
        }
    }

    #[inline(always)]
    pub const fn set_player_motion(
        &mut self,
        player: usize,
        scroll_pixels_per_second: f32,
        field_zoom: f32,
        scroll_travel_time: f32,
    ) {
        if player >= MAX_PLAYERS {
            return;
        }
        self.scroll_pixels_per_second[player] = scroll_pixels_per_second;
        self.field_zoom[player] = field_zoom;
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

#[cfg(test)]
mod replay_input_age_tests {
    use super::*;

    fn edge(source: InputSource, pressed: bool, raw_music_time_ns: SongTimeNs) -> RecordedLaneEdge {
        RecordedLaneEdge {
            lane_index: 0,
            pressed,
            source,
            event_music_time_ns: raw_music_time_ns,
        }
    }

    #[test]
    fn replay_input_age_uses_recorded_time_and_playback_rate() {
        let mut ages = ReplayInputAgeState::default();
        ages.record(edge(InputSource::Keyboard, true, 1_000_000_000), 1_500_000_000);
        ages.begin_snapshot(2_000_000_000, 2.0, 0.02);
        assert_eq!(ages.age_seconds(0, 2_000_000_000, 2.0), 0.5);
        assert_eq!(ages.age_seconds(0, 2_000_000_000, 0.5), 2.0);
        assert_eq!(ages.age_seconds(1, 2_000_000_000, 2.0), 0.0);
        assert_eq!(ages.age_seconds(0, 500_000_000, 2.0), 0.0);
    }

    #[test]
    fn replay_input_age_maxes_sources_and_replaces_release_timestamp() {
        let mut ages = ReplayInputAgeState::default();
        ages.record(edge(InputSource::Keyboard, true, 1_000_000_000), 1_000_000_000);
        ages.record(edge(InputSource::Gamepad, true, 1_500_000_000), 1_500_000_000);
        ages.begin_snapshot(2_000_000_000, 1.0, 0.02);
        assert_eq!(ages.age_seconds(0, 2_000_000_000, 1.0), 1.0);

        ages.record(edge(InputSource::Keyboard, false, 1_900_000_000), 2_000_000_000);
        ages.begin_snapshot(2_000_000_000, 1.0, 0.02);
        assert_eq!(ages.age_seconds(0, 2_000_000_000, 1.0), 0.5);
        ages.record(edge(InputSource::Gamepad, false, 1_950_000_000), 2_000_000_000);
        ages.begin_snapshot(2_000_000_000, 1.0, 0.02);
        assert_eq!(ages.age_seconds(0, 2_000_000_000, 1.0), 0.1);
    }

    #[test]
    fn replay_release_cleanup_uses_report_time_and_retains_pruning_snapshot() {
        let mut ages = ReplayInputAgeState::default();
        ages.record(edge(InputSource::Keyboard, false, 10_000_000), 20_000_000);
        ages.begin_snapshot(40_000_000, 1.0, 0.02);
        assert_eq!(ages.age_seconds(0, 40_000_000, 1.0), 0.03);
        ages.begin_snapshot(41_000_000, 1.0, 0.02);
        assert_eq!(ages.age_seconds(0, 41_000_000, 1.0), 0.031);
        assert_eq!(ages.age_seconds(0, 41_000_000, 1.0), 0.031);
        ages.begin_snapshot(42_000_000, 1.0, 0.02);
        assert_eq!(ages.age_seconds(0, 42_000_000, 1.0), 0.0);
    }

    #[test]
    fn replay_release_cleanup_obeys_configured_debounce_and_rate() {
        let mut ages = ReplayInputAgeState::default();
        ages.record(edge(InputSource::Keyboard, false, 10_000_000), 20_000_000);
        ages.begin_snapshot(120_000_000, 2.0, 0.05);
        assert_eq!(ages.age_seconds(0, 120_000_000, 2.0), 0.055);
        ages.begin_snapshot(122_000_000, 2.0, 0.05);
        assert_eq!(ages.age_seconds(0, 122_000_000, 2.0), 0.056);
        ages.begin_snapshot(124_000_000, 2.0, 0.05);
        assert_eq!(ages.age_seconds(0, 124_000_000, 2.0), 0.0);
    }

    #[test]
    fn replay_legacy_edges_use_source_lane_identity_and_reset_cleanly() {
        let mut ages = ReplayInputAgeState::default();
        ages.record(edge(InputSource::Keyboard, true, 1_000_000_000), 1_000_000_000);
        // Legacy records cannot distinguish two keyboard bindings on this lane.
        // The later reported timestamp replaces the earlier fallback identity.
        ages.record(edge(InputSource::Keyboard, true, 1_500_000_000), 1_500_000_000);
        ages.begin_snapshot(2_000_000_000, 1.0, 0.02);
        assert_eq!(ages.age_seconds(0, 2_000_000_000, 1.0), 0.5);
        ages.clear();
        assert_eq!(ages.age_seconds(0, 2_000_000_000, 1.0), 0.0);
        ages.begin_snapshot(3_000_000_000, 1.0, 0.02);
        assert_eq!(ages.age_seconds(0, 3_000_000_000, 1.0), 0.0);
    }

    #[test]
    fn replay_input_age_rejects_invalid_times_and_columns() {
        let mut ages = ReplayInputAgeState::default();
        ages.record(edge(InputSource::Keyboard, true, INVALID_SONG_TIME_NS), 0);
        ages.record(edge(InputSource::Gamepad, true, 0), INVALID_SONG_TIME_NS);
        ages.begin_snapshot(1_000_000_000, 1.0, 0.02);
        assert_eq!(ages.age_seconds(0, 1_000_000_000, 1.0), 0.0);
        assert_eq!(ages.age_seconds(MAX_COLS, 1_000_000_000, 1.0), 0.0);
        assert_eq!(ages.age_seconds(0, INVALID_SONG_TIME_NS, 1.0), 0.0);
    }
}
