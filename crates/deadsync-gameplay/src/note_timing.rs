#[inline(always)]
pub const fn input_queue_cap(num_cols: usize) -> usize {
    // Pre-size one backlog-warning bucket per 4-panel field so live gameplay
    // does not grow the queue before crossing its first pressure threshold.
    let fields = if num_cols <= 4 {
        1
    } else {
        num_cols.div_ceil(4)
    };
    GAMEPLAY_INPUT_BACKLOG_WARN * fields
}

#[inline(always)]
pub fn replay_edge_cap(
    num_cols: usize,
    replay_cells: usize,
    replay_mode: bool,
    song_seconds: f32,
) -> usize {
    if replay_mode {
        return 0;
    }
    // Live recording stores physical press/release edges, so reserve two edges
    // per playable note cell, keep a small per-lane floor for early misses, and
    // add a duration budget so a whole-song run does not grow on dense mashing.
    let chart_cap = replay_cells.saturating_mul(2);
    let floor_cap = num_cols.saturating_mul(REPLAY_EDGE_FLOOR_PER_LANE);
    let seconds_cap = replay_seconds_cap(num_cols, song_seconds);
    chart_cap.max(floor_cap).max(seconds_cap)
}

#[inline(always)]
fn replay_seconds_cap(num_cols: usize, song_seconds: f32) -> usize {
    if !song_seconds.is_finite() || song_seconds <= 0.0 {
        return 0;
    }
    (song_seconds.ceil() as usize)
        .saturating_mul(num_cols)
        .saturating_mul(REPLAY_EDGE_RATE_PER_SEC)
}

#[inline(always)]
pub const fn lane_press_started(pressed: bool, was_down: bool, is_down: bool) -> bool {
    pressed && !was_down && is_down
}

#[inline(always)]
pub const fn lane_release_finished(pressed: bool, was_down: bool, is_down: bool) -> bool {
    !pressed && was_down && !is_down
}

#[inline(always)]
pub const fn lane_edge_judges_tap(pressed: bool, slot_was_down: bool) -> bool {
    pressed && !slot_was_down
}

#[inline(always)]
pub const fn lane_edge_judges_lift(pressed: bool, slot_was_down: bool) -> bool {
    !pressed && slot_was_down
}

#[inline(always)]
pub const fn active_hold_counts_as_pressed(live_autoplay: bool, lane_pressed: bool) -> bool {
    live_autoplay || lane_pressed
}

#[inline(always)]
pub const fn counts_for_early_rescore(note_type: NoteType) -> bool {
    matches!(
        note_type,
        NoteType::Tap | NoteType::Lift | NoteType::Hold | NoteType::Roll
    )
}

#[inline(always)]
pub const fn row_final_grade_hides_note(grade: JudgeGrade) -> bool {
    // deadsync's gameplay ruleset is ITG timing with optional FA+ visual
    // overlays, so match Simply Love ITG's MinTNSToHideNotes=W3 behavior.
    matches!(
        grade,
        JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
    )
}

#[inline(always)]
pub const fn lane_edge_matches_note_type(pressed: bool, note_type: NoteType) -> bool {
    match note_type {
        NoteType::Tap | NoteType::Hold | NoteType::Roll => pressed,
        NoteType::Lift => !pressed,
        NoteType::Fake => pressed,
        NoteType::Mine => false,
    }
}

#[inline(always)]
pub fn note_has_displayable_hold(note: &Note) -> bool {
    matches!(note.note_type, NoteType::Hold | NoteType::Roll) && note.hold.is_some()
}

#[inline(always)]
pub fn column_cue_is_mine(note: &Note) -> Option<bool> {
    if note.is_fake {
        return None;
    }
    match note.note_type {
        NoteType::Tap | NoteType::Lift | NoteType::Hold | NoteType::Roll => Some(false),
        NoteType::Mine => Some(true),
        NoteType::Fake => None,
    }
}

pub fn build_column_cues_for_player(
    notes: &[Note],
    note_range: (usize, usize),
    note_time_cache_ns: &[SongTimeNs],
    col_start: usize,
    col_end: usize,
    first_visible_time: f32,
) -> Vec<ColumnCue> {
    let (start, end) = note_range;
    if start >= end || col_start >= col_end {
        return Vec::new();
    }

    let mut column_times: Vec<(f32, Vec<ColumnCueColumn>)> = Vec::with_capacity(end - start);
    let mut i = start;
    while i < end {
        let row = notes[i].row_index;
        let mut row_time = 0.0_f32;
        let mut has_row_time = false;
        let mut columns = Vec::with_capacity(4);
        while i < end && notes[i].row_index == row {
            let note = &notes[i];
            if note.column >= col_start
                && note.column < col_end
                && let Some(is_mine) = column_cue_is_mine(note)
            {
                if !has_row_time {
                    row_time = song_time_ns_to_seconds(note_time_cache_ns[i]);
                    has_row_time = true;
                }
                columns.push(ColumnCueColumn {
                    column: note.column,
                    is_mine,
                });
            }
            i += 1;
        }
        if has_row_time {
            columns.sort_unstable_by_key(|c| c.column);
            columns.dedup_by_key(|c| c.column);
            column_times.push((row_time, columns));
        }
    }

    let mut cues = Vec::with_capacity(column_times.len());
    let mut prev_time = 0.0_f32;
    for (time, columns) in column_times {
        let duration = time - prev_time;
        if duration >= COLUMN_CUE_MIN_SECONDS || prev_time == 0.0 {
            cues.push(ColumnCue {
                start_time: prev_time,
                duration,
                columns,
            });
        }
        prev_time = time;
    }

    if first_visible_time < 0.0
        && let Some(first) = cues.first_mut()
    {
        first.duration -= first_visible_time;
        first.start_time += first_visible_time;
    }
    cues
}

#[inline(always)]
pub fn late_note_resolution_window_ns(timing_profile: &TimingProfile, rate: f32) -> SongTimeNs {
    // Mirror ITG's shared late-resolution window from Player::GetMaxStepDistanceSeconds():
    // late taps, missed hold heads, and avoided mines all wait for the largest
    // relevant gameplay window instead of resolving on their own local window.
    let profile_music_ns = TimingProfileNs::from_profile_scaled(timing_profile, rate);
    profile_music_ns
        .windows_ns
        .into_iter()
        .fold(0, i64::max)
        .max(profile_music_ns.mine_window_ns)
        .max(scaled_song_time_ns(TIMING_WINDOW_SECONDS_HOLD, rate))
        .max(scaled_song_time_ns(TIMING_WINDOW_SECONDS_ROLL, rate))
}

#[inline(always)]
pub fn max_step_distance_ns(timing_profile: &TimingProfile, rate: f32) -> SongTimeNs {
    late_note_resolution_window_ns(timing_profile, rate)
        .saturating_add(song_time_ns_from_seconds(MAX_INPUT_LATENCY_SECONDS))
}

#[inline(always)]
pub fn judged_row_lookahead_time_ns(
    current_music_time_ns: SongTimeNs,
    timing_profile: &TimingProfile,
    rate: f32,
) -> SongTimeNs {
    current_music_time_ns.saturating_add(max_step_distance_ns(timing_profile, rate))
}

pub fn compute_end_times_ns(
    notes: &[Note],
    note_time_cache_ns: &[SongTimeNs],
    hold_end_time_cache_ns: &[Option<SongTimeNs>],
    rate: f32,
    audio_end_time_ns: SongTimeNs,
) -> (SongTimeNs, SongTimeNs) {
    let mut last_judgable_time_ns = 0;
    let mut last_relevant_time_ns = 0;
    for (i, note) in notes.iter().enumerate() {
        let start_time_ns = note_time_cache_ns[i];
        if song_time_ns_invalid(start_time_ns) {
            continue;
        }
        let end_time_ns = hold_end_time_cache_ns[i]
            .filter(|&time_ns| !song_time_ns_invalid(time_ns))
            .unwrap_or(start_time_ns);
        last_relevant_time_ns = last_relevant_time_ns.max(end_time_ns);
        if note.can_be_judged {
            last_judgable_time_ns = last_judgable_time_ns.max(end_time_ns);
        }
    }

    let timing_profile = TimingProfile::default_itg_with_fa_plus();
    let max_step_distance_ns = max_step_distance_ns(&timing_profile, rate);
    (
        last_judgable_time_ns.saturating_add(max_step_distance_ns),
        last_relevant_time_ns
            .saturating_add(max_step_distance_ns)
            .max(audio_end_time_ns),
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayEndTimingState {
    notes_end_time_ns: SongTimeNs,
    music_end_time_ns: SongTimeNs,
    audio_end_time_ns: SongTimeNs,
}

impl GameplayEndTimingState {
    #[inline(always)]
    pub const fn new(
        notes_end_time_ns: SongTimeNs,
        music_end_time_ns: SongTimeNs,
        audio_end_time_ns: SongTimeNs,
    ) -> Self {
        Self {
            notes_end_time_ns,
            music_end_time_ns,
            audio_end_time_ns,
        }
    }

    #[inline(always)]
    pub const fn notes_end_time_ns(&self) -> SongTimeNs {
        self.notes_end_time_ns
    }

    #[inline(always)]
    pub const fn music_end_time_ns(&self) -> SongTimeNs {
        self.music_end_time_ns
    }

    #[inline(always)]
    pub const fn audio_end_time_ns(&self) -> SongTimeNs {
        self.audio_end_time_ns
    }

    #[inline(always)]
    pub fn set_note_and_music_end_times(
        &mut self,
        notes_end_time_ns: SongTimeNs,
        music_end_time_ns: SongTimeNs,
    ) {
        self.notes_end_time_ns = notes_end_time_ns;
        self.music_end_time_ns = music_end_time_ns;
    }
}

#[inline(always)]
pub fn song_audio_end_time_ns(song: &SongData) -> SongTimeNs {
    let chart_end = song.precise_last_second();
    let audio_len = song.music_length_seconds;
    let end_seconds = match (
        chart_end.is_finite() && chart_end > 0.0,
        audio_len.is_finite() && audio_len > 0.0,
    ) {
        (true, true) => chart_end.min(audio_len),
        (true, false) => chart_end,
        (false, true) => audio_len,
        (false, false) => return 0,
    };
    song_time_ns_from_seconds(end_seconds)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaneSearchRows {
    pub current: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LaneSearchMemo {
    time_ns: SongTimeNs,
    rows: LaneSearchRows,
}

#[derive(Clone, Debug)]
struct LaneSearchTimingCache {
    current: BeatInfoCache,
    forward: BeatInfoCache,
    backward: BeatInfoCache,
    memo: Option<LaneSearchMemo>,
}

impl LaneSearchTimingCache {
    fn new(timing: &TimingData) -> Self {
        Self {
            current: BeatInfoCache::new(timing),
            forward: BeatInfoCache::new(timing),
            backward: BeatInfoCache::new(timing),
            memo: None,
        }
    }

    fn reset(&mut self, timing: &TimingData) {
        self.current.reset(timing);
        self.forward.reset(timing);
        self.backward.reset(timing);
        self.memo = None;
    }

    fn rows(&mut self, timing: &TimingData, time_ns: SongTimeNs) -> LaneSearchRows {
        if let Some(memo) = self.memo
            && memo.time_ns == time_ns
        {
            return memo.rows;
        }

        let forward_time_ns = song_time_ns_add_seconds(time_ns, STEP_SEARCH_DISTANCE_SECONDS);
        let backward_time_ns = song_time_ns_add_seconds(time_ns, -STEP_SEARCH_DISTANCE_SECONDS);
        let current_beat = timing
            .get_beat_info_from_time_ns_cached(time_ns, &mut self.current)
            .beat;
        let forward_beat = timing
            .get_beat_info_from_time_ns_cached(forward_time_ns, &mut self.forward)
            .beat;
        let backward_beat = timing
            .get_beat_info_from_time_ns_cached(backward_time_ns, &mut self.backward)
            .beat;
        let rows = lane_search_rows_from_beats(timing, current_beat, forward_beat, backward_beat);
        self.memo = Some(LaneSearchMemo { time_ns, rows });
        rows
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CutoffRowsMemo {
    cutoff_time_ns: SongTimeNs,
    active_players: usize,
    rows: [usize; MAX_PLAYERS],
}

/// Game-thread, song-lifetime caches for monotonic time-to-beat query streams.
///
/// The cache has fixed capacity, allocates nothing, and is warmed when gameplay
/// state is built. Rewinds and timing-offset edits reset every stream together;
/// normal monotonic queries only walk timing events crossed since the last query.
/// There is no eviction or deferred destruction. Exact-timestamp cutoff and input
/// memos stop inserting by replacement and make repeated same-frame work O(1).
/// The `timing_cache` benchmark is the instrumentation point; runtime counters are
/// deliberately omitted from the hot path. Worst case after a reset is one full
/// uncached-equivalent traversal per stream, outside steady-state gameplay.
#[derive(Clone, Debug)]
pub struct GameplayTimeToBeatCaches {
    song: BeatInfoCache,
    display: BeatInfoCache,
    visible: [BeatInfoCache; MAX_PLAYERS],
    cutoff: [BeatInfoCache; MAX_PLAYERS],
    assist: BeatInfoCache,
    assist_future: BeatInfoCache,
    input: [LaneSearchTimingCache; MAX_PLAYERS],
    cutoff_memo: Option<CutoffRowsMemo>,
}

impl GameplayTimeToBeatCaches {
    pub fn new(
        timing: &TimingData,
        timing_players: &[&TimingData; MAX_PLAYERS],
    ) -> Self {
        Self {
            song: BeatInfoCache::new(timing),
            display: BeatInfoCache::new(timing),
            visible: std::array::from_fn(|player| BeatInfoCache::new(timing_players[player])),
            cutoff: std::array::from_fn(|player| BeatInfoCache::new(timing_players[player])),
            assist: BeatInfoCache::new(timing),
            assist_future: BeatInfoCache::new(timing),
            input: std::array::from_fn(|player| LaneSearchTimingCache::new(timing_players[player])),
            cutoff_memo: None,
        }
    }

    pub fn reset(
        &mut self,
        timing: &TimingData,
        timing_players: &[&TimingData; MAX_PLAYERS],
    ) {
        self.song.reset(timing);
        self.display.reset(timing);
        self.assist.reset(timing);
        self.assist_future.reset(timing);
        for (((visible, cutoff), input), timing_player) in self
            .visible
            .iter_mut()
            .zip(&mut self.cutoff)
            .zip(&mut self.input)
            .zip(timing_players)
        {
            visible.reset(timing_player);
            cutoff.reset(timing_player);
            input.reset(timing_player);
        }
        self.cutoff_memo = None;
    }

    #[inline(always)]
    pub fn song_info(&mut self, timing: &TimingData, time_ns: SongTimeNs) -> BeatInfo {
        timing.get_beat_info_from_time_ns_cached(time_ns, &mut self.song)
    }

    #[inline(always)]
    pub fn display_beat(&mut self, timing: &TimingData, time_ns: SongTimeNs) -> f32 {
        timing
            .get_beat_info_from_time_ns_cached(time_ns, &mut self.display)
            .beat
    }

    #[inline(always)]
    pub fn visible_beat(
        &mut self,
        player: usize,
        timing: &TimingData,
        time_ns: SongTimeNs,
    ) -> f32 {
        timing
            .get_beat_info_from_time_ns_cached(time_ns, &mut self.visible[player])
            .beat
    }

    pub fn missed_note_cutoff_rows(
        &mut self,
        timing_profile: &TimingProfile,
        timing_players: &[&TimingData; MAX_PLAYERS],
        music_rate: f32,
        music_time_ns: SongTimeNs,
        num_players: usize,
    ) -> [usize; MAX_PLAYERS] {
        let cutoff_time_ns =
            music_time_ns.saturating_sub(max_step_distance_ns(timing_profile, music_rate));
        let active_players = num_players.min(MAX_PLAYERS);
        if let Some(memo) = self.cutoff_memo
            && memo.cutoff_time_ns == cutoff_time_ns
            && memo.active_players == active_players
        {
            return memo.rows;
        }

        let mut rows = [0; MAX_PLAYERS];
        for (player, (timing_player, cache)) in timing_players
            .iter()
            .zip(&mut self.cutoff)
            .take(active_players)
            .enumerate()
        {
            let info = timing_player.get_beat_info_from_time_ns_cached(cutoff_time_ns, cache);
            rows[player] = missed_note_cutoff_row_from_info(timing_player, info);
        }
        self.cutoff_memo = Some(CutoffRowsMemo {
            cutoff_time_ns,
            active_players,
            rows,
        });
        rows
    }

    #[inline(always)]
    pub fn assist_row_no_offset(
        &mut self,
        timing: &TimingData,
        global_offset_seconds: f32,
        music_time_ns: SongTimeNs,
    ) -> i32 {
        assist_row_no_offset_cached(
            timing,
            global_offset_seconds,
            music_time_ns,
            &mut self.assist,
        )
    }

    #[inline(always)]
    pub fn assist_future_row(
        &mut self,
        timing: &TimingData,
        global_offset_seconds: f32,
        audio_output_delay_seconds: f32,
        music_time_ns: SongTimeNs,
        slope: f32,
        song_row: i32,
    ) -> i32 {
        let horizon = assist_lookahead_music_horizon_seconds(audio_output_delay_seconds, slope);
        assist_row_no_offset_cached(
            timing,
            global_offset_seconds,
            song_time_ns_add_seconds(music_time_ns, horizon),
            &mut self.assist_future,
        )
        .max(song_row)
    }

    #[inline(always)]
    pub fn lane_search_rows(
        &mut self,
        player: usize,
        timing: &TimingData,
        time_ns: SongTimeNs,
    ) -> LaneSearchRows {
        self.input[player].rows(timing, time_ns)
    }
}

#[inline(always)]
fn lane_search_rows_from_beats(
    timing: &TimingData,
    current_beat: f32,
    forward_beat: f32,
    backward_beat: f32,
) -> LaneSearchRows {
    let current = timing_row_nearest(timing, current_beat);
    let forward = timing_row_nearest(timing, forward_beat);
    let backward = timing_row_nearest(timing, backward_beat);
    let distance = forward
        .saturating_sub(current)
        .max(current.saturating_sub(backward))
        .saturating_add(ROWS_PER_BEAT.max(1) as usize);
    LaneSearchRows {
        current,
        start: current.saturating_sub(distance),
        end: current.saturating_add(distance),
    }
}

#[inline(always)]
pub fn lane_search_rows_for_timing(
    timing: &TimingData,
    time_ns: SongTimeNs,
) -> LaneSearchRows {
    let forward_time_ns = song_time_ns_add_seconds(time_ns, STEP_SEARCH_DISTANCE_SECONDS);
    let backward_time_ns = song_time_ns_add_seconds(time_ns, -STEP_SEARCH_DISTANCE_SECONDS);
    lane_search_rows_from_beats(
        timing,
        timing.get_beat_for_time_ns(time_ns),
        timing.get_beat_for_time_ns(forward_time_ns),
        timing.get_beat_for_time_ns(backward_time_ns),
    )
}

#[inline(always)]
fn missed_note_cutoff_row_from_info(timing: &TimingData, beat_info: BeatInfo) -> usize {
    let mut cutoff_note_row = beat_to_note_row(beat_info.beat);
    if beat_info.is_in_freeze && !beat_info.is_in_delay {
        cutoff_note_row = cutoff_note_row.saturating_add(1);
    }
    timing.cutoff_row_for_note_row(cutoff_note_row)
}

#[inline(always)]
pub fn missed_note_cutoff_row_for_timing(timing: &TimingData, cutoff_time_ns: SongTimeNs) -> usize {
    missed_note_cutoff_row_from_info(
        timing,
        timing.get_beat_info_from_time_ns(cutoff_time_ns),
    )
}

#[inline(always)]
pub fn missed_note_cutoff_row_for_music_time(
    timing_profile: &TimingProfile,
    timing: &TimingData,
    music_rate: f32,
    music_time_ns: SongTimeNs,
) -> usize {
    let cutoff_time_ns =
        music_time_ns.saturating_sub(max_step_distance_ns(timing_profile, music_rate));
    missed_note_cutoff_row_for_timing(timing, cutoff_time_ns)
}

pub fn missed_note_cutoff_rows_for_players(
    timing_profile: &TimingProfile,
    timing_players: &[&TimingData; MAX_PLAYERS],
    music_rate: f32,
    music_time_ns: SongTimeNs,
    num_players: usize,
) -> [usize; MAX_PLAYERS] {
    let active_players = num_players.min(MAX_PLAYERS);
    let mut cutoff_rows = [0; MAX_PLAYERS];
    for player in 0..active_players {
        cutoff_rows[player] = missed_note_cutoff_row_for_music_time(
            timing_profile,
            timing_players[player],
            music_rate,
            music_time_ns,
        );
    }
    cutoff_rows
}

#[inline(always)]
pub fn timing_row_floor(timing: &TimingData, beat: f32) -> usize {
    let Some(mut row) = timing.get_row_for_beat(beat) else {
        return 0;
    };
    if row > 0
        && timing
            .get_beat_for_row(row)
            .is_some_and(|row_beat| row_beat > beat)
    {
        row -= 1;
    }
    row
}

#[inline(always)]
pub fn assist_row_no_offset_for_timing(
    timing: &TimingData,
    global_offset_seconds: f32,
    music_time_ns: SongTimeNs,
) -> i32 {
    // ITG parity: assist clap/metronome uses no global-offset timing.
    // TimingData::get_beat_for_time_ns() applies global offset internally, so
    // feed (time - offset) to cancel it out.
    let beat_no_offset = timing.get_beat_for_time_ns(song_time_ns_add_seconds(
        music_time_ns,
        -global_offset_seconds,
    ));
    timing_row_floor(timing, beat_no_offset).min(i32::MAX as usize) as i32
}

#[inline(always)]
fn assist_row_no_offset_cached(
    timing: &TimingData,
    global_offset_seconds: f32,
    music_time_ns: SongTimeNs,
    cache: &mut BeatInfoCache,
) -> i32 {
    let target_time_ns = song_time_ns_add_seconds(music_time_ns, -global_offset_seconds);
    let beat_no_offset = timing
        .get_beat_info_from_time_ns_cached(target_time_ns, cache)
        .beat;
    timing_row_floor(timing, beat_no_offset).min(i32::MAX as usize) as i32
}

#[inline(always)]
pub fn recent_step_tracks(
    pressed_since_ns: &[Option<SongTimeNs>; MAX_COLS],
    start: usize,
    end: usize,
    event_music_time_ns: SongTimeNs,
) -> usize {
    if song_time_ns_invalid(event_music_time_ns) {
        return 0;
    }
    let jump_window_ns = song_time_ns_from_seconds(STEP_CAL_JUMP_WINDOW_S);
    pressed_since_ns[start..end]
        .iter()
        .filter(|pressed_at| {
            pressed_at.is_some_and(|pressed_at_ns| {
                let age_ns = event_music_time_ns.saturating_sub(pressed_at_ns);
                age_ns >= 0 && age_ns < jump_window_ns
            })
        })
        .count()
}

#[inline(always)]
pub fn recent_step_calories(
    pressed_since_ns: &[Option<SongTimeNs>; MAX_COLS],
    start: usize,
    end: usize,
    event_music_time_ns: SongTimeNs,
    weight_pounds: i32,
) -> f32 {
    if song_time_ns_invalid(event_music_time_ns) {
        return 0.0;
    }
    let tracks = recent_step_tracks(pressed_since_ns, start, end, event_music_time_ns);
    judgment::step_calories(weight_pounds, tracks)
}

#[inline(always)]
pub fn stage_music_cut(lead_in_seconds: f32) -> GameplayMusicCut {
    GameplayMusicCut {
        start_sec: f64::from(-lead_in_seconds.max(0.0)),
        length_sec: f64::INFINITY,
        ..Default::default()
    }
}

#[inline(always)]
pub fn visible_notefield_time_ns(
    music_time_ns: SongTimeNs,
    visual_delay_seconds: f32,
) -> SongTimeNs {
    song_time_ns_add_seconds(music_time_ns, -visual_delay_seconds)
}

#[inline(always)]
pub fn music_time_from_stream_position(
    stream_position_seconds: f32,
    lead_in_seconds: f32,
    global_offset_seconds: f32,
    rate: f32,
) -> f32 {
    let rate = if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    };
    let lead_in = lead_in_seconds.max(0.0);
    let anchor = -global_offset_seconds;
    (stream_position_seconds - lead_in).mul_add(rate, anchor * (1.0 - rate))
}

#[inline(always)]
pub fn assist_clap_cursor_for_row(rows: &[usize], row: i32) -> usize {
    if row < 0 {
        0
    } else {
        rows.partition_point(|&r| r <= row as usize)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssistClapScheduleUpdate {
    pub cursor: usize,
    pub last_crossed_row: i32,
    pub schedule_start: usize,
    pub schedule_end: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameplayAssistClapState {
    pub rows: Vec<usize>,
    cursor: usize,
    last_crossed_row: i32,
    sfx_generation_seen: u64,
}

impl GameplayAssistClapState {
    #[inline(always)]
    pub fn new(rows: Vec<usize>) -> Self {
        Self {
            rows,
            cursor: 0,
            last_crossed_row: -1,
            sfx_generation_seen: 0,
        }
    }

    #[inline(always)]
    pub fn reset_for_row(&mut self, row: i32) {
        self.cursor = assist_clap_cursor_for_row(&self.rows, row);
        self.last_crossed_row = row;
    }

    #[inline(always)]
    pub fn note_sfx_generation(&mut self, sfx_generation: u64) -> bool {
        let timeline_reset = self.sfx_generation_seen != sfx_generation;
        if timeline_reset {
            self.sfx_generation_seen = sfx_generation;
        }
        timeline_reset
    }

    #[inline(always)]
    pub fn schedule_update(
        &mut self,
        song_row: i32,
        future_row: i32,
        assist_enabled: bool,
        timeline_reset: bool,
    ) -> AssistClapScheduleUpdate {
        let update = assist_clap_schedule_update(
            &self.rows,
            self.cursor,
            self.last_crossed_row,
            song_row,
            future_row,
            assist_enabled,
            timeline_reset,
        );
        self.cursor = update.cursor;
        self.last_crossed_row = update.last_crossed_row;
        update
    }
}

pub fn assist_clap_schedule_update(
    rows: &[usize],
    cursor: usize,
    last_crossed_row: i32,
    song_row: i32,
    future_row: i32,
    assist_enabled: bool,
    timeline_reset: bool,
) -> AssistClapScheduleUpdate {
    let song_row = song_row.max(0);
    if !assist_enabled {
        return AssistClapScheduleUpdate {
            cursor: assist_clap_cursor_for_row(rows, song_row),
            last_crossed_row: song_row,
            schedule_start: 0,
            schedule_end: 0,
        };
    }

    let mut cursor = if timeline_reset {
        assist_clap_cursor_for_row(rows, song_row)
    } else {
        cursor.min(rows.len())
    };
    let last_crossed_row = if timeline_reset {
        song_row
    } else {
        song_row.max(last_crossed_row)
    };
    let schedule_start = cursor;
    while cursor < rows.len() {
        let clap_row = rows[cursor];
        if clap_row as i64 > i64::from(future_row) {
            break;
        }
        cursor += 1;
    }

    AssistClapScheduleUpdate {
        cursor,
        last_crossed_row,
        schedule_start,
        schedule_end: cursor,
    }
}

#[inline(always)]
pub fn assist_clap_music_seconds_for_row(timing: &TimingData, row: usize) -> Option<f64> {
    let beat = timing.get_beat_for_row(row)?;
    Some(timing.get_time_for_beat_no_offset_ns(beat) as f64 * 1.0e-9)
}

pub fn build_assist_clap_rows(notes: &[Note], note_range: (usize, usize)) -> Vec<usize> {
    let (start, end) = note_range;
    if start >= end {
        return Vec::new();
    }

    let mut rows = Vec::with_capacity(end - start);
    let mut i = start;
    while i < end {
        let row = notes[i].row_index;
        let mut has_clap = false;
        while i < end && notes[i].row_index == row {
            let note = &notes[i];
            if note.can_be_judged
                && !note.is_fake
                && matches!(
                    note.note_type,
                    NoteType::Tap | NoteType::Lift | NoteType::Hold | NoteType::Roll
                )
            {
                has_clap = true;
            }
            i += 1;
        }
        if has_clap {
            rows.push(row);
        }
    }
    rows
}

#[inline(always)]
pub fn assist_lookahead_music_horizon_seconds(delay_seconds: f32, slope: f32) -> f32 {
    let horizon_real = (delay_seconds + ASSIST_TICK_LOOKAHEAD_MARGIN_SECONDS).max(0.0);
    let slope = if slope.is_finite() && slope > 0.0 {
        slope
    } else {
        1.0
    };
    horizon_real * slope
}

/// Highest assist row whose no-offset music time falls within the look-ahead
/// horizon ahead of the audible position.
#[inline(always)]
pub fn assist_lookahead_future_row(
    timing: &TimingData,
    global_offset_seconds: f32,
    audio_output_delay_seconds: f32,
    music_time_ns: SongTimeNs,
    slope: f32,
    song_row: i32,
) -> i32 {
    let music_horizon = assist_lookahead_music_horizon_seconds(audio_output_delay_seconds, slope);
    let future_time = song_time_ns_add_seconds(music_time_ns, music_horizon);
    assist_row_no_offset_for_timing(timing, global_offset_seconds, future_time).max(song_row)
}

pub fn build_note_count_stats(notes: &[Note], note_range: (usize, usize)) -> Vec<NoteCountStat> {
    let (start, end) = note_range;
    let mut cursor = start.min(notes.len());
    let end = end.min(notes.len());
    let mut count = 0usize;
    let mut stats = Vec::new();

    while cursor < end {
        let row_index = notes[cursor].row_index;
        let beat = notes[cursor].beat;
        let notes_lower = count;
        while cursor < end && notes[cursor].row_index == row_index {
            count = count.saturating_add(1);
            cursor += 1;
        }
        stats.push(NoteCountStat {
            beat,
            notes_lower,
            notes_upper: count,
        });
    }

    stats
}
