use deadsync_chart::{SongData, SyncPref};
use deadsync_core::input::{InputSource, MAX_COLS, MAX_PLAYERS};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::{
    SongTimeNs, scaled_song_time_ns, song_time_ns_add_seconds, song_time_ns_from_seconds,
    song_time_ns_invalid, song_time_ns_to_seconds,
};
use deadsync_core::timing::{ROWS_PER_BEAT, beat_to_note_row};
use deadsync_rules::judgment::{self, JudgeGrade, Judgment, TimingWindow};
use deadsync_rules::note::{
    HoldData, HoldResult, MineResult, Note, TIMING_WINDOW_SECONDS_HOLD, TIMING_WINDOW_SECONDS_ROLL,
};
use deadsync_rules::timing::{TimingData, TimingProfile, TimingProfileNs};
use std::time::Instant;

// ITGmania ScreenGameplay MinSecondsToStep/MinSecondsToMusic defaults.
const MIN_SECONDS_TO_STEP: f32 = 6.0;
const MIN_SECONDS_TO_MUSIC: f32 = 2.0;
// Simply Love: ScreenGameplay GiveUpSeconds=0.33.
pub const GIVE_UP_HOLD_SECONDS: f32 = 0.33;
// Mirrors ScreenGameplay::AbortGiveUpText tween duration (1/2 second).
pub const GIVE_UP_ABORT_TEXT_SECONDS: f32 = 0.5;
pub const BACK_OUT_HOLD_SECONDS: f32 = 1.0;
// Simply Love: ScreenGameplay out.lua (sleep 0.5, linear 1.0).
const GIVE_UP_OUT_FADE_DELAY_SECONDS: f32 = 0.5;
const GIVE_UP_OUT_FADE_SECONDS: f32 = 1.0;
// Simply Love: _fade out normal.lua (sleep 0.1, linear 0.4).
const BACK_OUT_FADE_DELAY_SECONDS: f32 = 0.1;
const BACK_OUT_FADE_SECONDS: f32 = 0.4;
pub const GAMEPLAY_INPUT_BACKLOG_WARN: usize = 128;
const REPLAY_EDGE_FLOOR_PER_LANE: usize = 64;
pub const REPLAY_EDGE_RATE_PER_SEC: usize = 256;
pub const INITIAL_HOLD_LIFE: f32 = 1.0;
// ITG's MaxInputLatencySeconds preference defaults to 0.0.
const MAX_INPUT_LATENCY_SECONDS: f32 = 0.0;
// ITGmania Player::Step searches a wide row range first, then scores the
// selected note against the active timing window.
const STEP_SEARCH_DISTANCE_SECONDS: f32 = 1.0;
const COLUMN_CUE_MIN_SECONDS: f32 = 1.5;
const STEP_CAL_JUMP_WINDOW_S: f32 = 0.25;
pub const ASSIST_TICK_LOOKAHEAD_MARGIN_SECONDS: f32 = 0.050;
const QUANT_4TH: u8 = 0;
const QUANT_8TH: u8 = 1;
const QUANT_12TH: u8 = 2;
const QUANT_16TH: u8 = 3;
const QUANT_24TH: u8 = 4;
const QUANT_32ND: u8 = 5;
const QUANT_48TH: u8 = 6;
const QUANT_64TH: u8 = 7;
const QUANT_192ND: u8 = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayViewport {
    width: f32,
    height: f32,
}

impl GameplayViewport {
    pub const fn design() -> Self {
        Self {
            width: 854.0,
            height: 480.0,
        }
    }

    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width: if width.is_finite() && width > 0.0 {
                width
            } else {
                Self::design().width
            },
            height: if height.is_finite() && height > 0.0 {
                height
            } else {
                Self::design().height
            },
        }
    }

    #[inline(always)]
    pub const fn width(self) -> f32 {
        self.width
    }

    #[inline(always)]
    pub const fn height(self) -> f32 {
        self.height
    }

    #[inline(always)]
    pub const fn center_x(self) -> f32 {
        self.width * 0.5
    }

    #[inline(always)]
    pub const fn center_y(self) -> f32 {
        self.height * 0.5
    }

    #[inline(always)]
    pub fn is_wide(self) -> bool {
        self.width / self.height >= 1.6
    }
}

impl Default for GameplayViewport {
    fn default() -> Self {
        Self::design()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GameplayFailType {
    Immediate,
    ImmediateContinue,
}

impl Default for GameplayFailType {
    fn default() -> Self {
        Self::ImmediateContinue
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldToExitKey {
    Start,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutosyncMode {
    Off,
    Song,
    Machine,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayTurnOption {
    #[default]
    None,
    Mirror,
    LRMirror,
    UDMirror,
    Left,
    Right,
    Shuffle,
    Blender,
    Random,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayConfig {
    pub mine_hit_sound: bool,
    pub default_fail_type: GameplayFailType,
    pub global_offset_seconds: f32,
    pub visual_delay_seconds: f32,
    pub machine_pack_ini_offsets: bool,
    pub machine_default_sync_pref: SyncPref,
    pub machine_allow_per_player_global_offsets: bool,
    pub machine_enable_replays: bool,
    pub center_1player_notefield: bool,
    pub delayed_back: bool,
}

impl Default for GameplayConfig {
    fn default() -> Self {
        Self {
            mine_hit_sound: true,
            default_fail_type: GameplayFailType::ImmediateContinue,
            global_offset_seconds: -0.008,
            visual_delay_seconds: 0.0,
            machine_pack_ini_offsets: false,
            machine_default_sync_pref: SyncPref::Null,
            machine_allow_per_player_global_offsets: false,
            machine_enable_replays: true,
            center_1player_notefield: false,
            delayed_back: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GameplayMiniIndicatorData {
    pub personal_best_percent: [Option<f64>; MAX_PLAYERS],
    pub machine_best_percent: [Option<f64>; MAX_PLAYERS],
}

impl Default for GameplayMiniIndicatorData {
    fn default() -> Self {
        Self {
            personal_best_percent: [None; MAX_PLAYERS],
            machine_best_percent: [None; MAX_PLAYERS],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LeadInTiming {
    pub min_seconds_to_step: f32,
    pub min_seconds_to_music: f32,
}

impl Default for LeadInTiming {
    fn default() -> Self {
        Self {
            min_seconds_to_step: MIN_SECONDS_TO_STEP,
            min_seconds_to_music: MIN_SECONDS_TO_MUSIC,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayStreamClockSnapshot {
    pub stream_seconds: f32,
    pub music_nanos: SongTimeNs,
    pub music_seconds_per_second: f32,
    pub has_music_mapping: bool,
    pub valid_at: Instant,
    pub valid_at_host_nanos: u64,
}

impl Default for GameplayStreamClockSnapshot {
    fn default() -> Self {
        Self {
            stream_seconds: 0.0,
            music_nanos: 0,
            music_seconds_per_second: 1.0,
            has_music_mapping: false,
            valid_at: Instant::now(),
            valid_at_host_nanos: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GameplayAudioSnapshot {
    pub stream_clock: GameplayStreamClockSnapshot,
    pub assist_sfx_generation: u64,
    pub output_delay_seconds: f32,
    pub timing_diag_enabled: bool,
    pub timing_diag_callback_gap_ns: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayMusicCut {
    pub start_sec: f64,
    pub length_sec: f64,
    pub fade_in_sec: f64,
    pub fade_out_sec: f64,
}

impl Default for GameplayMusicCut {
    fn default() -> Self {
        Self {
            start_sec: 0.0,
            length_sec: f64::INFINITY,
            fade_in_sec: 0.0,
            fade_out_sec: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ColumnCueColumn {
    pub column: usize,
    pub is_mine: bool,
}

#[derive(Clone, Debug)]
pub struct ColumnCue {
    pub start_time: f32,
    pub duration: f32,
    pub columns: Vec<ColumnCueColumn>,
}

#[derive(Clone, Debug)]
pub struct JudgmentRenderInfo {
    pub judgment: Judgment,
    pub started_at_screen_s: f32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct MineJudgmentRenderInfo {
    pub result: MineResult,
    pub column: usize,
    pub started_at_screen_s: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct HoldJudgmentRenderInfo {
    pub result: HoldResult,
    pub started_at_screen_s: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct HeldMissRenderInfo {
    pub started_at_screen_s: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ActiveTapExplosion {
    pub window: &'static str,
    pub bright: bool,
    pub elapsed: f32,
    pub duration: f32,
    pub start_beat: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ActiveColumnFlash {
    pub grade: JudgeGrade,
    pub blue_fantastic: bool,
    pub started_at_screen_s: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ColumnTapJudgment {
    pub grade: JudgeGrade,
    pub blue_fantastic: bool,
    pub at_screen_s: f32,
}

#[derive(Clone, Debug)]
pub struct ActiveMineExplosion {
    pub elapsed: f32,
    pub duration: f32,
    pub started_at_screen_s: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComboMilestoneKind {
    Hundred,
    Thousand,
}

#[derive(Clone, Debug)]
pub struct ActiveComboMilestone {
    pub kind: ComboMilestoneKind,
    pub elapsed: f32,
}

#[derive(Clone, Debug)]
pub struct ActiveHold {
    pub note_index: usize,
    pub start_time_ns: SongTimeNs,
    pub end_time_ns: SongTimeNs,
    pub note_type: NoteType,
    pub let_go: bool,
    pub is_pressed: bool,
    pub life: f32,
    pub last_update_time_ns: SongTimeNs,
}

#[derive(Clone, Copy, Debug)]
pub struct TurnRng {
    state: u64,
}

impl TurnRng {
    #[inline(always)]
    pub fn new(seed: u64) -> Self {
        let seed = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        Self { state: seed }
    }

    #[inline(always)]
    pub fn next_u32(&mut self) -> u32 {
        // xorshift64*
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        (x >> 32) as u32
    }

    #[inline(always)]
    pub fn next_f32_unit(&mut self) -> f32 {
        (self.next_u32() as f32) * (1.0 / 4_294_967_296.0)
    }

    #[inline(always)]
    pub fn gen_range(&mut self, upper_exclusive: usize) -> usize {
        if upper_exclusive <= 1 {
            0
        } else {
            (self.next_u32() as usize) % upper_exclusive
        }
    }

    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        if slice.len() <= 1 {
            return;
        }
        for i in (1..slice.len()).rev() {
            let j = self.gen_range(i + 1);
            slice.swap(i, j);
        }
    }
}

#[inline(always)]
pub fn active_hold_is_engaged(active: &ActiveHold) -> bool {
    !active.let_go && active.life > 0.0
}

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

#[inline(always)]
pub fn missed_note_cutoff_row_for_timing(timing: &TimingData, cutoff_time_ns: SongTimeNs) -> usize {
    let beat_info = timing.get_beat_info_from_time_ns(cutoff_time_ns);
    let mut cutoff_note_row = beat_to_note_row(beat_info.beat);
    if beat_info.is_in_freeze && !beat_info.is_in_delay {
        cutoff_note_row = cutoff_note_row.saturating_add(1);
    }
    timing.cutoff_row_for_note_row(cutoff_note_row)
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
pub fn assist_clap_cursor_for_row(rows: &[usize], row: i32) -> usize {
    if row < 0 {
        0
    } else {
        rows.partition_point(|&r| r <= row as usize)
    }
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

#[inline(always)]
pub fn completed_row_flash_note_indices_and_judgment(
    notes: &[Note],
    row_entry: &RowEntry,
) -> Option<([usize; MAX_COLS], usize, Judgment)> {
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
    Some((out, len, *final_judgment))
}

#[inline(always)]
pub const fn suppress_final_bad_rescore_visual(
    row_had_provisional_early_hit: bool,
    final_grade: JudgeGrade,
) -> bool {
    row_had_provisional_early_hit && matches!(final_grade, JudgeGrade::Decent | JudgeGrade::WayOff)
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

#[derive(Clone, Copy, Debug)]
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
pub struct ErrorBarTick {
    pub started_at: f32,
    pub offset_s: f32,
    pub window: TimingWindow,
}

#[derive(Clone, Copy, Debug)]
pub struct ErrorBarText {
    pub started_at: f32,
    pub early: bool,
    pub offset_ms: f32,
    pub scaled: bool,
    pub scale_start_ms: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct OffsetIndicatorText {
    pub started_at: f32,
    pub offset_ms: f32,
    pub window: TimingWindow,
}

#[derive(Clone, Copy, Debug)]
pub struct NoteCountStat {
    pub beat: f32,
    pub notes_lower: usize,
    pub notes_upper: usize,
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

#[inline(always)]
pub const fn hold_to_exit_seconds(key: HoldToExitKey) -> f32 {
    match key {
        HoldToExitKey::Start => GIVE_UP_HOLD_SECONDS,
        HoldToExitKey::Back => BACK_OUT_HOLD_SECONDS,
    }
}

#[inline(always)]
pub const fn exit_total_seconds(kind: ExitTransitionKind) -> f32 {
    match kind {
        ExitTransitionKind::Out => GIVE_UP_OUT_FADE_DELAY_SECONDS + GIVE_UP_OUT_FADE_SECONDS,
        ExitTransitionKind::Cancel => BACK_OUT_FADE_DELAY_SECONDS + BACK_OUT_FADE_SECONDS,
    }
}

#[inline(always)]
pub fn exit_transition_alpha_elapsed(kind: ExitTransitionKind, elapsed_s: f32) -> f32 {
    let (delay, fade) = match kind {
        ExitTransitionKind::Out => (GIVE_UP_OUT_FADE_DELAY_SECONDS, GIVE_UP_OUT_FADE_SECONDS),
        ExitTransitionKind::Cancel => (BACK_OUT_FADE_DELAY_SECONDS, BACK_OUT_FADE_SECONDS),
    };
    if fade <= 0.0 {
        return 1.0;
    }
    let alpha = if elapsed_s <= delay {
        0.0
    } else {
        (elapsed_s - delay) / fade
    };
    alpha.clamp(0.0, 1.0)
}

#[inline(always)]
pub fn exit_transition_alpha(exit: &ExitTransition) -> f32 {
    exit_transition_alpha_elapsed(exit.kind, exit.started_at.elapsed().as_secs_f32())
}

#[inline(always)]
pub const fn gameplay_exit_for_kind(kind: ExitTransitionKind) -> GameplayExit {
    match kind {
        ExitTransitionKind::Out => GameplayExit::Complete,
        ExitTransitionKind::Cancel => GameplayExit::Cancel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::song_time::{
        INVALID_SONG_TIME_NS, song_time_ns_from_seconds, song_time_ns_to_seconds,
    };
    use deadsync_core::timing::ROWS_PER_BEAT;
    use deadsync_rules::note::{HoldData, Note};
    use deadsync_rules::timing::{DelaySegment, FakeSegment, StopSegment, TimingSegments};
    use std::path::PathBuf;

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.000_001,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn exit_timing_matches_screen_policy() {
        assert_eq!(hold_to_exit_seconds(HoldToExitKey::Start), 0.33);
        assert_eq!(hold_to_exit_seconds(HoldToExitKey::Back), 1.0);

        assert_eq!(exit_total_seconds(ExitTransitionKind::Out), 1.5);
        assert_eq!(exit_total_seconds(ExitTransitionKind::Cancel), 0.5);

        assert_eq!(
            gameplay_exit_for_kind(ExitTransitionKind::Out),
            GameplayExit::Complete
        );
        assert_eq!(
            gameplay_exit_for_kind(ExitTransitionKind::Cancel),
            GameplayExit::Cancel
        );
    }

    #[test]
    fn exit_alpha_respects_delay_and_fade() {
        assert_near(
            exit_transition_alpha_elapsed(ExitTransitionKind::Out, 0.5),
            0.0,
        );
        assert_near(
            exit_transition_alpha_elapsed(ExitTransitionKind::Out, 1.0),
            0.5,
        );
        assert_near(
            exit_transition_alpha_elapsed(ExitTransitionKind::Cancel, 0.3),
            0.5,
        );
        assert_near(
            exit_transition_alpha_elapsed(ExitTransitionKind::Cancel, 9.0),
            1.0,
        );
    }

    #[test]
    fn input_queue_capacity_scales_by_field_count() {
        assert_eq!(input_queue_cap(0), GAMEPLAY_INPUT_BACKLOG_WARN);
        assert_eq!(input_queue_cap(4), GAMEPLAY_INPUT_BACKLOG_WARN);
        assert_eq!(input_queue_cap(5), GAMEPLAY_INPUT_BACKLOG_WARN * 2);
        assert_eq!(input_queue_cap(8), GAMEPLAY_INPUT_BACKLOG_WARN * 2);
    }

    #[test]
    fn replay_capacity_uses_recording_budget() {
        assert_eq!(replay_edge_cap(4, 0, true, 120.0), 0);
        assert_eq!(replay_edge_cap(4, 0, false, 0.0), 4 * 64);
        assert_eq!(
            replay_edge_cap(4, 0, false, 2.0),
            4 * 2 * REPLAY_EDGE_RATE_PER_SEC
        );
        assert_eq!(
            replay_edge_cap(4, 120, false, 2.0),
            4 * 2 * REPLAY_EDGE_RATE_PER_SEC
        );
        assert_eq!(replay_edge_cap(4, 4000, false, 2.0), 8000);
        assert_eq!(
            replay_edge_cap(8, 1000, false, 1.0),
            8 * REPLAY_EDGE_RATE_PER_SEC
        );
    }

    #[test]
    fn lane_edges_classify_press_and_release() {
        assert!(lane_press_started(true, false, true));
        assert!(!lane_press_started(true, true, true));
        assert!(lane_release_finished(false, true, false));
        assert!(!lane_release_finished(false, true, true));

        assert!(lane_edge_judges_tap(true, false));
        assert!(!lane_edge_judges_tap(true, true));
        assert!(lane_edge_judges_lift(false, true));
        assert!(!lane_edge_judges_lift(false, false));
    }

    #[test]
    fn autoplay_keeps_active_holds_pressed() {
        assert!(active_hold_counts_as_pressed(true, false));
        assert!(active_hold_counts_as_pressed(true, true));
        assert!(active_hold_counts_as_pressed(false, true));
        assert!(!active_hold_counts_as_pressed(false, false));
    }

    fn test_note(note_type: NoteType, hold: Option<HoldData>, is_fake: bool) -> Note {
        test_note_at(note_type, hold, is_fake, 0, 0.0)
    }

    fn test_note_at(
        note_type: NoteType,
        hold: Option<HoldData>,
        is_fake: bool,
        row_index: usize,
        beat: f32,
    ) -> Note {
        Note {
            beat,
            quantization_idx: 0,
            column: 0,
            note_type,
            row_index,
            result: None,
            early_result: None,
            hold,
            mine_result: None,
            is_fake,
            can_be_judged: true,
        }
    }

    fn test_hold() -> HoldData {
        HoldData {
            end_row_index: 48,
            end_beat: 1.0,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 1.0,
            last_held_row_index: 0,
            last_held_beat: 0.0,
        }
    }

    fn test_judgment(grade: JudgeGrade) -> Judgment {
        Judgment {
            time_error_ms: 0.0,
            time_error_music_ns: 0,
            grade,
            window: match grade {
                JudgeGrade::Fantastic => Some(TimingWindow::W1),
                JudgeGrade::Excellent => Some(TimingWindow::W2),
                JudgeGrade::Great => Some(TimingWindow::W3),
                JudgeGrade::Decent => Some(TimingWindow::W4),
                JudgeGrade::WayOff => Some(TimingWindow::W5),
                JudgeGrade::Miss => None,
            },
            miss_because_held: false,
        }
    }

    fn test_row_to_beat(last_row: usize) -> Vec<f32> {
        (0..=last_row)
            .map(|row| row as f32 / ROWS_PER_BEAT as f32)
            .collect()
    }

    fn test_timing(last_row: usize) -> TimingData {
        TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(last_row),
        )
    }

    fn test_song(chart_end: f32, audio_len: f32) -> SongData {
        SongData {
            simfile_path: PathBuf::from("song.ssc"),
            title: String::new(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
            normalized_bpms: String::new(),
            music_length_seconds: audio_len,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: chart_end,
            charts: Vec::new(),
        }
    }

    #[test]
    fn note_types_define_rescore_and_edge_matching() {
        for note_type in [
            NoteType::Tap,
            NoteType::Lift,
            NoteType::Hold,
            NoteType::Roll,
        ] {
            assert!(counts_for_early_rescore(note_type));
        }
        assert!(!counts_for_early_rescore(NoteType::Mine));
        assert!(!counts_for_early_rescore(NoteType::Fake));

        assert!(lane_edge_matches_note_type(true, NoteType::Tap));
        assert!(!lane_edge_matches_note_type(false, NoteType::Tap));
        assert!(!lane_edge_matches_note_type(true, NoteType::Lift));
        assert!(lane_edge_matches_note_type(false, NoteType::Lift));
        assert!(!lane_edge_matches_note_type(true, NoteType::Mine));
        assert!(!lane_edge_matches_note_type(false, NoteType::Mine));
    }

    #[test]
    fn note_display_predicates_match_gameplay_visual_rules() {
        assert!(row_final_grade_hides_note(JudgeGrade::Fantastic));
        assert!(row_final_grade_hides_note(JudgeGrade::Excellent));
        assert!(row_final_grade_hides_note(JudgeGrade::Great));
        assert!(!row_final_grade_hides_note(JudgeGrade::Decent));
        assert!(!row_final_grade_hides_note(JudgeGrade::WayOff));
        assert!(!row_final_grade_hides_note(JudgeGrade::Miss));

        assert!(note_has_displayable_hold(&test_note(
            NoteType::Hold,
            Some(test_hold()),
            false,
        )));
        assert!(note_has_displayable_hold(&test_note(
            NoteType::Roll,
            Some(test_hold()),
            false,
        )));
        assert!(!note_has_displayable_hold(&test_note(
            NoteType::Hold,
            None,
            false,
        )));
        assert!(!note_has_displayable_hold(&test_note(
            NoteType::Tap,
            Some(test_hold()),
            false,
        )));
    }

    #[test]
    fn column_cues_skip_fake_notes_and_mark_mines() {
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Tap, None, false)),
            Some(false)
        );
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Lift, None, false)),
            Some(false)
        );
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Mine, None, false)),
            Some(true)
        );
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Mine, None, true)),
            None
        );
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Fake, None, false)),
            None
        );
    }

    #[test]
    fn column_cue_builder_filters_fakes_and_preserves_timed_gaps() {
        let mut first = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        first.column = 0;
        let mut fake = test_note_at(NoteType::Tap, None, true, 96, 2.0);
        fake.column = 1;
        fake.can_be_judged = false;
        let mut later = test_note_at(NoteType::Tap, None, false, 192, 4.0);
        later.column = 2;
        let notes = [first, fake, later];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(4.0),
        ];

        let cues = build_column_cues_for_player(&notes, (0, notes.len()), &note_times, 0, 4, 0.0);

        assert_eq!(cues.len(), 2);
        assert_near(cues[0].start_time, 0.0);
        assert_near(cues[0].duration, 1.0);
        assert_eq!(cues[0].columns.len(), 1);
        assert_eq!(cues[0].columns[0].column, 0);
        assert_eq!(cues[0].columns[0].is_mine, false);
        assert_near(cues[1].start_time, 1.0);
        assert_near(cues[1].duration, 3.0);
        assert_eq!(cues[1].columns.len(), 1);
        assert_eq!(cues[1].columns[0].column, 2);
    }

    #[test]
    fn column_cue_builder_sorts_dedups_and_offsets_first_visible_time() {
        let mut first = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        first.column = 2;
        let mut duplicate = test_note_at(NoteType::Lift, None, false, 48, 1.0);
        duplicate.column = 2;
        let mut mine = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        mine.column = 0;
        let notes = [first, duplicate, mine];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];

        let cues = build_column_cues_for_player(&notes, (0, notes.len()), &note_times, 0, 4, -0.5);

        assert_eq!(cues.len(), 1);
        assert_near(cues[0].start_time, -0.5);
        assert_near(cues[0].duration, 1.5);
        assert_eq!(cues[0].columns.len(), 2);
        assert_eq!(cues[0].columns[0].column, 0);
        assert_eq!(cues[0].columns[0].is_mine, true);
        assert_eq!(cues[0].columns[1].column, 2);
        assert_eq!(cues[0].columns[1].is_mine, false);
    }

    #[test]
    fn late_resolution_uses_largest_gameplay_window() {
        let timing_profile = TimingProfile::default_itg_with_fa_plus();
        let seconds = song_time_ns_to_seconds(late_note_resolution_window_ns(&timing_profile, 1.0));
        assert!((seconds - 0.3515).abs() <= 1e-6);
    }

    #[test]
    fn max_step_distance_scales_with_music_rate() {
        let timing_profile = TimingProfile::default_itg_with_fa_plus();
        let seconds = song_time_ns_to_seconds(max_step_distance_ns(&timing_profile, 1.5));
        assert!((seconds - 0.52725).abs() <= 1e-6);
    }

    #[test]
    fn song_audio_end_time_uses_positive_chart_or_audio_end() {
        assert_eq!(
            song_audio_end_time_ns(&test_song(5.0, 10.0)),
            song_time_ns_from_seconds(5.0)
        );
        assert_eq!(
            song_audio_end_time_ns(&test_song(f32::NAN, 10.0)),
            song_time_ns_from_seconds(10.0)
        );
        assert_eq!(
            song_audio_end_time_ns(&test_song(5.0, 0.0)),
            song_time_ns_from_seconds(5.0)
        );
        assert_eq!(song_audio_end_time_ns(&test_song(0.0, 0.0)), 0);
    }

    #[test]
    fn stage_music_cut_uses_negative_lead_in() {
        let cut = stage_music_cut(2.5);
        assert_eq!(cut.start_sec, -2.5);
        assert!(cut.length_sec.is_infinite());
        assert_eq!(cut.fade_in_sec, 0.0);
        assert_eq!(cut.fade_out_sec, 0.0);

        let clamped = stage_music_cut(-1.0);
        assert_eq!(clamped.start_sec, 0.0);
    }

    #[test]
    fn recent_step_tracks_count_current_press_inside_jump_window() {
        let mut pressed_since_ns = [None; MAX_COLS];
        pressed_since_ns[0] = Some(song_time_ns_from_seconds(10.0));
        pressed_since_ns[1] = Some(song_time_ns_from_seconds(9.9));
        pressed_since_ns[2] = Some(song_time_ns_from_seconds(9.74));
        pressed_since_ns[4] = Some(song_time_ns_from_seconds(10.0));

        assert_eq!(
            recent_step_tracks(&pressed_since_ns, 0, 4, song_time_ns_from_seconds(10.0)),
            2
        );
        assert_eq!(
            recent_step_tracks(&pressed_since_ns, 4, 8, song_time_ns_from_seconds(10.0)),
            1
        );
        assert_eq!(
            recent_step_tracks(&pressed_since_ns, 0, 4, INVALID_SONG_TIME_NS),
            0
        );
    }

    #[test]
    fn visible_notefield_time_subtracts_visual_delay() {
        let music_time_ns = song_time_ns_from_seconds(100.0);
        let visible = song_time_ns_to_seconds(visible_notefield_time_ns(music_time_ns, 0.010));

        assert!((visible - 99.990).abs() < 0.000_5);
    }

    #[test]
    fn assist_clap_rows_include_judgable_lifts_and_skip_fakes() {
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, true, 24, 0.5),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
            test_note_at(NoteType::Roll, Some(test_hold()), false, 144, 3.0),
        ];
        notes[3]
            .hold
            .as_mut()
            .expect("roll has hold data")
            .end_row_index = 192;

        assert_eq!(
            build_assist_clap_rows(&notes, (0, notes.len())),
            vec![48, 144]
        );
        assert_eq!(build_assist_clap_rows(&notes, (2, 2)), Vec::<usize>::new());
    }

    #[test]
    fn assist_clap_cursor_skips_rows_at_or_before_current_row() {
        let rows = [48, 96, 144];

        assert_eq!(assist_clap_cursor_for_row(&rows, -1), 0);
        assert_eq!(assist_clap_cursor_for_row(&rows, 47), 0);
        assert_eq!(assist_clap_cursor_for_row(&rows, 48), 1);
        assert_eq!(assist_clap_cursor_for_row(&rows, 120), 2);
        assert_eq!(assist_clap_cursor_for_row(&rows, 144), 3);
    }

    #[test]
    fn assist_lookahead_horizon_adds_margin_and_scales_by_slope() {
        let h = assist_lookahead_music_horizon_seconds(0.020, 1.0);
        assert!((h - 0.070).abs() <= 1e-6, "h={h}");

        let h2 = assist_lookahead_music_horizon_seconds(0.020, 2.0);
        assert!((h2 - 0.140).abs() <= 1e-6, "h2={h2}");

        assert!(
            (assist_lookahead_music_horizon_seconds(0.0, f32::NAN)
                - ASSIST_TICK_LOOKAHEAD_MARGIN_SECONDS)
                .abs()
                <= 1e-6
        );
        assert!(assist_lookahead_music_horizon_seconds(-1.0, 1.0) >= 0.0);
    }

    #[test]
    fn end_times_wait_for_audio_tail() {
        let notes = [test_note_at(NoteType::Tap, None, false, 96, 2.0)];
        let note_times = [song_time_ns_from_seconds(2.0)];
        let hold_end_times = [None];
        let audio_end_time_ns = song_time_ns_from_seconds(10.0);

        let (notes_end_time_ns, music_end_time_ns) =
            compute_end_times_ns(&notes, &note_times, &hold_end_times, 1.0, audio_end_time_ns);

        assert!(notes_end_time_ns < audio_end_time_ns);
        assert_eq!(music_end_time_ns, audio_end_time_ns);
    }

    #[test]
    fn end_times_use_judgable_and_relevant_tails_separately() {
        let mut fake = test_note_at(NoteType::Fake, None, true, 240, 5.0);
        fake.can_be_judged = false;
        let notes = [test_note_at(NoteType::Tap, None, false, 48, 1.0), fake];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(5.0),
        ];
        let hold_end_times = [None, None];

        let (notes_end_time_ns, music_end_time_ns) =
            compute_end_times_ns(&notes, &note_times, &hold_end_times, 1.0, 0);

        assert!(notes_end_time_ns < note_times[1]);
        assert!(music_end_time_ns > note_times[1]);
    }

    #[test]
    fn missed_note_cutoff_row_matches_stop_delay_rules() {
        let row_to_beat = test_row_to_beat(ROWS_PER_BEAT as usize * 4);
        let stop_timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                stops: vec![StopSegment {
                    beat: 1.0,
                    duration: 2.0,
                }],
                ..TimingSegments::default()
            },
            &row_to_beat,
        );
        let stop_cutoff_time = stop_timing
            .get_time_for_beat_ns(1.0)
            .saturating_add(song_time_ns_from_seconds(0.5));
        assert_eq!(
            missed_note_cutoff_row_for_timing(&stop_timing, stop_cutoff_time),
            ROWS_PER_BEAT as usize + 1
        );

        let delay_timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                delays: vec![DelaySegment {
                    beat: 1.0,
                    duration: 2.0,
                }],
                ..TimingSegments::default()
            },
            &row_to_beat,
        );
        let delay_cutoff_time = delay_timing
            .get_time_for_beat_ns(1.0)
            .saturating_sub(song_time_ns_from_seconds(0.5));
        assert_eq!(
            missed_note_cutoff_row_for_timing(&delay_timing, delay_cutoff_time),
            ROWS_PER_BEAT as usize
        );
    }

    #[test]
    fn missed_note_cutoff_row_uses_chart_row_indices() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                ..TimingSegments::default()
            },
            &[0.0, 4.0, 8.0],
        );

        assert_eq!(
            missed_note_cutoff_row_for_timing(&timing, timing.get_time_for_beat_ns(3.0)),
            1
        );
        assert_eq!(
            missed_note_cutoff_row_for_timing(&timing, timing.get_time_for_beat_ns(4.0)),
            1
        );
        assert_eq!(
            missed_note_cutoff_row_for_timing(&timing, timing.get_time_for_beat_ns(4.1)),
            2
        );
    }

    #[test]
    fn note_count_stats_group_rows_and_clamp_range() {
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];

        let stats = build_note_count_stats(&notes, (0, 99));

        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].beat, 1.0);
        assert_eq!(stats[0].notes_lower, 0);
        assert_eq!(stats[0].notes_upper, 2);
        assert_eq!(stats[1].beat, 2.0);
        assert_eq!(stats[1].notes_lower, 2);
        assert_eq!(stats[1].notes_upper, 3);
    }

    #[test]
    fn row_entry_counts_unresolved_notes_and_rescore_tracks() {
        let mut judged = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        judged.result = Some(Judgment {
            time_error_ms: 4.0,
            time_error_music_ns: 4_000_000,
            grade: JudgeGrade::Great,
            window: Some(TimingWindow::W3),
            miss_because_held: false,
        });
        judged.early_result = Some(Judgment {
            time_error_ms: -12.0,
            time_error_music_ns: -12_000_000,
            grade: JudgeGrade::Decent,
            window: Some(TimingWindow::W4),
            miss_because_held: false,
        });
        let notes = [
            judged,
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
        ];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        note_indices[1] = 1;
        let row_entry = build_row_entry(48, note_indices, 2, &notes, &note_times);

        assert_eq!(row_entry.row_index, 48);
        assert_eq!(row_entry.time_ns, note_times[0]);
        assert_eq!(row_entry.note_indices(), &[0, 1]);
        assert_eq!(count_rescore_tracks_on_row(&row_entry), 2);
        assert_eq!(row_entry.unresolved_count, 1);
        assert_eq!(row_entry.unresolved_nonlift_count, 0);
        assert!(row_entry.had_provisional_early_hit);
        assert_eq!(row_entry.final_outcome, None);
    }

    #[test]
    fn row_entry_counts_unresolved_nonlift_notes() {
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
        ];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        note_indices[1] = 1;
        let row_entry = build_row_entry(48, note_indices, 2, &notes, &note_times);

        assert_eq!(row_entry.unresolved_count, 2);
        assert_eq!(row_entry.unresolved_nonlift_count, 1);
    }

    #[test]
    fn cached_row_lookup_uses_row_map_and_final_outcome() {
        let mut note = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        note.result = Some(test_judgment(JudgeGrade::Great));
        let notes = [note];
        let note_times = [song_time_ns_from_seconds(1.0)];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });
        let mut row_map_cache = vec![u32::MAX; 49];
        row_map_cache[48] = 0;

        let row_entry =
            row_entry_for_cached_row(&row_entries, &row_map_cache, 48).expect("cached row");
        let outcome = finalized_row_outcome_for_cached_row(&row_entries, &row_map_cache, 48)
            .expect("finalized row outcome");

        assert_eq!(row_entry.note_indices(), &[0]);
        assert_eq!(outcome.final_grade, JudgeGrade::Great);
        assert_eq!(row_entry_index_for_cached_row(&row_map_cache, 47), None);
    }

    #[test]
    fn completed_row_judgment_waits_for_all_notes_and_returns_indices() {
        let mut judged = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        judged.result = Some(test_judgment(JudgeGrade::Great));
        let pending = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        let notes = [judged, pending];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        note_indices[1] = 1;
        let row_entry = build_row_entry(48, note_indices, 2, &notes, &note_times);
        assert!(completed_row_final_judgment(&notes, &row_entry).is_none());

        let mut notes = notes;
        notes[1].result = Some(test_judgment(JudgeGrade::Great));
        let judgment =
            completed_row_final_judgment(&notes, &row_entry).expect("completed row judgment");
        let (indices, len, flash_judgment) =
            completed_row_flash_note_indices_and_judgment(&notes, &row_entry)
                .expect("completed row flash judgment");

        assert_eq!(judgment.grade, JudgeGrade::Great);
        assert_eq!(flash_judgment.grade, JudgeGrade::Great);
        assert_eq!(&indices[..len], &[0, 1]);
    }

    #[test]
    fn judged_row_cursor_skips_finalized_and_finds_ready_rows() {
        let mut row1_note = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        row1_note.result = Some(test_judgment(JudgeGrade::Great));
        let row2_note = test_note_at(NoteType::Tap, None, false, 96, 2.0);
        let mut row3_note = test_note_at(NoteType::Tap, None, false, 144, 3.0);
        row3_note.result = Some(test_judgment(JudgeGrade::Great));
        row3_note.early_result = Some(test_judgment(JudgeGrade::Decent));
        let notes = [row1_note, row2_note, row3_note];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(3.0),
        ];
        let mut row1_indices = [usize::MAX; MAX_COLS];
        let mut row2_indices = [usize::MAX; MAX_COLS];
        let mut row3_indices = [usize::MAX; MAX_COLS];
        row1_indices[0] = 0;
        row2_indices[0] = 1;
        row3_indices[0] = 2;
        let mut row_entries = vec![
            build_row_entry(48, row1_indices, 1, &notes, &note_times),
            build_row_entry(96, row2_indices, 1, &notes, &note_times),
            build_row_entry(144, row3_indices, 1, &notes, &note_times),
        ];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });

        let lookahead = song_time_ns_from_seconds(3.5);
        let cursor = advance_judged_row_cursor(0, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, lookahead)
        });
        let ready = next_ready_row_in_lookahead(cursor, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, lookahead)
        });

        assert_eq!(cursor, 1);
        assert_eq!(ready, Some((2, 144, true)));
        assert!(suppress_final_bad_rescore_visual(true, JudgeGrade::Decent));
        assert!(!suppress_final_bad_rescore_visual(true, JudgeGrade::Great));
    }

    #[test]
    fn row_grids_group_sorted_rows_and_ignore_out_of_range_columns() {
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];
        let mut notes = notes;
        notes[0].column = 2;
        notes[1].column = 0;
        notes[2].column = 3;
        notes[3].column = 5;

        let rows = build_row_grids(&notes, (0, notes.len()), 0, 4);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].row_index, 48);
        assert_eq!(rows[0].note_indices[0], 1);
        assert_eq!(rows[0].note_indices[2], 0);
        assert_eq!(rows[1].row_index, 96);
        assert_eq!(rows[1].note_indices[3], 2);
        assert_eq!(rows[1].note_indices[0], usize::MAX);
    }

    #[test]
    fn player_rows_filter_by_column_range_and_sort_unique() {
        let mut notes = [
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 144, 3.0),
        ];
        notes[0].column = 5;
        notes[1].column = 2;
        notes[2].column = 3;
        notes[3].column = 9;

        assert_eq!(local_player_col(5, 2, 4), Some(3));
        assert_eq!(local_player_col(1, 2, 4), None);
        assert_eq!(player_rows(&notes, 2, 4), vec![48, 96]);

        sort_player_notes(&mut notes);
        let rows_and_cols: Vec<(usize, usize)> = notes
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        assert_eq!(rows_and_cols, vec![(48, 2), (48, 3), (96, 5), (144, 9)]);
    }

    #[test]
    fn simultaneous_limit_counts_active_holds_before_row_taps() {
        let mut hold = test_note_at(NoteType::Hold, Some(test_hold()), false, 0, 0.0);
        hold.column = 0;
        hold.hold
            .as_mut()
            .expect("hold has hold data")
            .end_row_index = 96;
        let mut tap1 = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        tap1.column = 1;
        let mut tap2 = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        tap2.column = 2;
        let mut notes = vec![hold, tap1, tap2];

        enforce_max_simultaneous_notes(&mut notes, 2, 0, 4);

        assert_eq!(notes.len(), 2);
        assert_eq!((notes[0].column, notes[0].row_index), (0, 0));
        assert_eq!((notes[1].column, notes[1].row_index), (2, 48));
    }

    #[test]
    fn row_track_helpers_count_taps_holds_and_first_tracks() {
        let mut notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, true, 48, 1.0),
            test_note_at(NoteType::Hold, Some(test_hold()), false, 24, 0.5),
        ];
        notes[0].column = 2;
        notes[1].column = 0;
        notes[2].column = 1;
        notes[3].column = 3;
        notes[3]
            .hold
            .as_mut()
            .expect("hold has hold data")
            .end_row_index = 96;

        assert_eq!(count_nonempty_tracks_at_row(&notes, 48, 0, 4), 3);
        assert_eq!(count_tap_or_hold_tracks_at_row(&notes, 48, 0, 4), 3);
        assert_eq!(count_tap_tracks_at_row(&notes, 48, 0, 4), 2);
        assert_eq!(first_nonempty_track_at_row(&notes, 48, 0, 4), Some(0));
        assert_eq!(first_tap_track_at_row(&notes, 48, 0, 4), Some(0));
        assert!(is_hold_body_at_row(&notes, 48, 3));
        assert_eq!(count_held_tracks_at_row(&notes, 48, 0, 4), 1);
        assert!(cell_has_any_note(&notes, 48, 1));
        assert!(!cell_has_nonfake_note(&notes, 48, 1));
        assert_eq!(stomp_mirror_track(1, 4), 2);
    }

    #[test]
    fn added_notes_replace_existing_cell_and_use_timing() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 2),
        );
        let mut notes = vec![test_note_at(NoteType::Tap, None, false, 48, 1.0)];

        assert!(set_added_mine_note(&mut notes, &timing, 48, 0));
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].note_type, NoteType::Mine);
        assert_eq!(notes[0].beat, 1.0);

        assert!(set_added_tap_note(&mut notes, &timing, 96, 1));
        assert!(cell_has_any_note(&notes, 96, 1));
        remove_cell_notes(&mut notes, 96, 1);
        assert!(!cell_has_any_note(&notes, 96, 1));
    }

    #[test]
    fn mines_insert_converts_every_sixth_nonempty_row() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(5 * ROWS_PER_BEAT as usize),
        );
        let mut notes = (0..6)
            .map(|i| {
                test_note_at(
                    NoteType::Tap,
                    None,
                    false,
                    i * ROWS_PER_BEAT as usize,
                    i as f32,
                )
            })
            .collect::<Vec<_>>();

        apply_mines_insert(
            &mut notes,
            &[],
            &timing,
            0,
            4,
            0,
            5 * ROWS_PER_BEAT as usize,
        );

        assert!(notes.iter().any(|note| {
            note.row_index == 5 * ROWS_PER_BEAT as usize && note.note_type == NoteType::Mine
        }));
    }

    #[test]
    fn mines_insert_adds_mine_half_beat_after_hold_end() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(3 * ROWS_PER_BEAT as usize),
        );
        let mut hold = test_note_at(NoteType::Hold, Some(test_hold()), false, 0, 0.0);
        hold.column = 1;
        hold.hold
            .as_mut()
            .expect("hold has hold data")
            .end_row_index = 2 * ROWS_PER_BEAT as usize;
        let mut notes = vec![hold];

        apply_mines_insert(
            &mut notes,
            &[],
            &timing,
            0,
            4,
            0,
            3 * ROWS_PER_BEAT as usize,
        );

        assert!(notes.iter().any(|note| {
            note.row_index == 2 * ROWS_PER_BEAT as usize + (ROWS_PER_BEAT as usize / 2)
                && note.column == 1
                && note.note_type == NoteType::Mine
        }));
    }

    #[test]
    fn intelligent_insert_adds_middle_tap_between_matching_endpoints() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 2),
        );
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
        ];
        notes[0].column = 0;
        notes[1].column = 2;

        apply_insert_intelligent_taps(
            &mut notes,
            &timing,
            0,
            4,
            ROWS_PER_BEAT as usize,
            (ROWS_PER_BEAT / 2) as usize,
            ROWS_PER_BEAT as usize,
            false,
        );

        assert!(notes.iter().any(|note| {
            note.row_index == (ROWS_PER_BEAT / 2) as usize
                && note.column == 1
                && note.note_type == NoteType::Tap
        }));
    }

    #[test]
    fn wide_stomp_and_echo_insert_expected_taps() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 4),
        );

        let mut wide = vec![test_note_at(NoteType::Tap, None, false, 0, 0.0)];
        wide[0].column = 1;
        apply_wide_insert(&mut wide, &timing, 0, 4);
        assert!(
            wide.iter()
                .any(|note| note.row_index == 0 && note.column != 1)
        );

        let mut stomp = vec![test_note_at(NoteType::Tap, None, false, 0, 0.0)];
        stomp[0].column = 1;
        apply_stomp_insert(&mut stomp, &timing, 0, 4);
        assert!(
            stomp
                .iter()
                .any(|note| note.row_index == 0 && note.column == 2)
        );

        let mut echo = vec![test_note_at(NoteType::Tap, None, false, 0, 0.0)];
        echo[0].column = 3;
        apply_echo_insert(&mut echo, &timing, 0, 4);
        assert!(
            echo.iter()
                .any(|note| { note.row_index == (ROWS_PER_BEAT / 2) as usize && note.column == 3 })
        );
    }

    #[test]
    fn convert_taps_to_holds_sets_hold_metadata() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 2),
        );
        let mut notes = vec![test_note_at(NoteType::Tap, None, false, 0, 0.0)];
        notes[0].column = 0;

        convert_taps_to_holds(&mut notes, &timing, 0, 4, 1);

        assert_eq!(notes[0].note_type, NoteType::Hold);
        let hold = notes[0].hold.as_ref().expect("tap converted to hold");
        assert_eq!(hold.end_row_index, ROWS_PER_BEAT as usize);
        assert_eq!(hold.life, INITIAL_HOLD_LIFE);
        assert_eq!(hold.last_held_row_index, 0);
    }

    #[test]
    fn notes_row_sorted_allows_equal_rows_only_in_order() {
        let sorted = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];
        let unsorted = [
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
        ];

        assert!(notes_row_sorted(&sorted));
        assert!(!notes_row_sorted(&unsorted));
    }

    #[test]
    fn turn_options_mirror_only_player_range_columns() {
        let mut notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
        ];
        for (col, note) in notes.iter_mut().enumerate() {
            note.column = col;
        }

        let turns = [GameplayTurnOption::Mirror, GameplayTurnOption::None];
        apply_turn_options(&mut notes, [(0, 4), (4, 8)], 4, 2, turns, 123);

        let columns: Vec<usize> = notes.iter().map(|note| note.column).collect();
        assert_eq!(columns, vec![3, 2, 1, 0, 4, 5, 6, 7]);
    }

    #[test]
    fn turn_options_left_maps_four_panel_columns() {
        let mut notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
        ];
        for (col, note) in notes.iter_mut().enumerate() {
            note.column = col;
        }

        let turns = [GameplayTurnOption::Left, GameplayTurnOption::None];
        let notes_len = notes.len();
        apply_turn_options(&mut notes, [(0, notes_len), (0, 0)], 4, 1, turns, 123);

        let columns: Vec<usize> = notes.iter().map(|note| note.column).collect();
        assert_eq!(columns, vec![1, 3, 0, 2]);
    }

    #[test]
    fn note_and_mine_window_bounds_use_left_open_right_closed_time() {
        let times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.5),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(2.5),
        ];
        let note_indices = [0, 1, 2, 3];
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 144, 3.0),
            test_note_at(NoteType::Tap, None, false, 192, 4.0),
        ];

        assert_eq!(mine_window_bounds_ns(&times, times[0], times[2]), (0, 3));
        assert_eq!(crossed_mine_bounds_ns(&times, times[0], times[2]), (1, 3));
        assert_eq!(
            lane_note_window_bounds_ns(&note_indices, &times, times[0], times[2]),
            (0, 3)
        );
        assert_eq!(
            lane_note_window_bounds_rows(&note_indices, &notes, 96, 192),
            (1, 3)
        );
    }

    #[test]
    fn step_search_bounds_expand_one_second_plus_one_beat() {
        let timing = test_timing(144);
        assert_eq!(
            step_search_row_bounds(&timing, song_time_ns_from_seconds(1.0), 48),
            (0, 144)
        );
    }

    #[test]
    fn step_search_bounds_saturate_before_song_start() {
        let timing = test_timing(144);
        assert_eq!(
            step_search_row_bounds(&timing, song_time_ns_from_seconds(0.0), 0),
            (0, 96)
        );
    }

    #[test]
    fn crossed_mine_held_start_tracks_existing_or_new_hold() {
        let previous = song_time_ns_from_seconds(1.0);
        let pressed_before = song_time_ns_from_seconds(0.9);
        let pressed_after = song_time_ns_from_seconds(1.25);
        let current = song_time_ns_from_seconds(1.5);

        assert_eq!(
            crossed_mine_held_start_time(true, true, None, previous, current),
            Some(previous)
        );
        assert_eq!(
            crossed_mine_held_start_time(true, false, Some(pressed_before), previous, current),
            Some(previous)
        );
        assert_eq!(
            crossed_mine_held_start_time(true, false, Some(pressed_after), previous, current),
            Some(pressed_after)
        );
        assert_eq!(
            crossed_mine_held_start_time(false, true, Some(previous), previous, current),
            None
        );
        assert_eq!(
            crossed_mine_held_start_time(
                true,
                false,
                Some(INVALID_SONG_TIME_NS),
                previous,
                current,
            ),
            None
        );
    }

    #[test]
    fn edge_judge_indices_use_lead_note_only() {
        assert_eq!(collect_edge_judge_indices(0, 7), None);

        let (indices, count) = collect_edge_judge_indices(3, 7).expect("row has notes");
        assert_eq!(count, 1);
        assert_eq!(indices[0], 7);
        assert!(indices[1..].iter().all(|index| *index == usize::MAX));
    }

    #[test]
    fn quantization_index_matches_note_row_subdivision() {
        assert_eq!(quantization_index_from_beat(0.0), QUANT_4TH);
        assert_eq!(quantization_index_from_beat(0.5), QUANT_8TH);
        assert_eq!(quantization_index_from_beat(1.0 / 3.0), QUANT_12TH);
        assert_eq!(quantization_index_from_beat(0.25), QUANT_16TH);
        assert_eq!(quantization_index_from_beat(1.0 / 6.0), QUANT_24TH);
        assert_eq!(quantization_index_from_beat(0.125), QUANT_32ND);
        assert_eq!(quantization_index_from_beat(1.0 / 12.0), QUANT_48TH);
        assert_eq!(quantization_index_from_beat(1.0 / 16.0), QUANT_64TH);
        assert_eq!(quantization_index_from_beat(1.0 / 48.0), QUANT_192ND);
    }

    #[test]
    fn closest_note_breaks_ties_toward_future_note() {
        let timing = test_timing(144);
        let notes = vec![
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 50, 50.0 / ROWS_PER_BEAT as f32),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [1_000_000_000_i64, 1_020_000_000_i64];
        let (note_index, err_ns) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            1_010_000_000_i64,
            49,
            0,
            note_indices.len(),
        )
        .expect("expected an equidistant closest note");

        assert_eq!(note_index, 1);
        assert_eq!(err_ns, -10_000_000);
    }

    #[test]
    fn closest_note_prefers_row_distance_over_time_error() {
        let timing = test_timing(144);
        let notes = vec![
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 60, 60.0 / ROWS_PER_BEAT as f32),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [
            song_time_ns_from_seconds(1.020),
            song_time_ns_from_seconds(1.028),
        ];
        let current_time_ns = song_time_ns_from_seconds(1.030);
        let (note_index, err_ns) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            current_time_ns,
            50,
            0,
            note_indices.len(),
        )
        .expect("expected the nearer row to win");

        assert_eq!(note_index, 0);
        assert_eq!(err_ns, current_time_ns - note_times_ns[note_index]);
    }

    #[test]
    fn closest_note_skips_fake_segment_taps_and_judged_mines() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                fakes: vec![FakeSegment {
                    beat: 1.0,
                    length: 0.01,
                }],
                ..TimingSegments::default()
            },
            &test_row_to_beat(144),
        );
        let mut fake_segment_tap = test_note_at(NoteType::Tap, None, true, 48, 1.0);
        fake_segment_tap.can_be_judged = false;
        let mut judged_mine = test_note_at(NoteType::Mine, None, false, 49, 49.0 / 48.0);
        judged_mine.mine_result = Some(MineResult::Hit);
        let notes = vec![
            fake_segment_tap,
            judged_mine,
            test_note_at(NoteType::Tap, None, false, 60, 60.0 / 48.0),
        ];
        let note_indices = [0usize, 1, 2];
        let note_times_ns = [
            song_time_ns_from_seconds(1.000),
            song_time_ns_from_seconds(1.010),
            song_time_ns_from_seconds(1.120),
        ];

        let (note_index, _) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            song_time_ns_from_seconds(1.030),
            50,
            0,
            note_indices.len(),
        )
        .expect("expected the unjudged tap to remain hittable");

        assert_eq!(note_index, 2);
    }
}
