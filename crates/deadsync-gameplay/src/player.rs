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

#[inline(always)]
pub fn judgment_render_info(judgment: Judgment, started_at_screen_s: f32) -> JudgmentRenderInfo {
    JudgmentRenderInfo {
        judgment,
        started_at_screen_s,
    }
}

#[inline(always)]
pub fn mine_judgment_render_info(
    result: MineResult,
    column: usize,
    started_at_screen_s: f32,
) -> MineJudgmentRenderInfo {
    MineJudgmentRenderInfo {
        result,
        column,
        started_at_screen_s,
    }
}

#[inline(always)]
pub fn hold_judgment_render_info(
    result: HoldResult,
    started_at_screen_s: f32,
) -> HoldJudgmentRenderInfo {
    HoldJudgmentRenderInfo {
        result,
        started_at_screen_s,
    }
}

#[inline(always)]
pub const fn held_miss_render_info(started_at_screen_s: f32) -> HeldMissRenderInfo {
    HeldMissRenderInfo {
        started_at_screen_s,
    }
}

#[derive(Clone, Debug)]
pub struct PlayerRuntime {
    pub combo: u32,
    pub miss_combo: u32,
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub current_combo_window_counts: WindowCounts,
    pub first_fc_attempt_broken: bool,
    pub judgment_counts: judgment::JudgeCounts,
    pub scoring_counts: judgment::JudgeCounts,
    pub last_judgment: Option<JudgmentRenderInfo>,
    pub last_mine_judgment: Option<MineJudgmentRenderInfo>,

    pub life: f32,
    pub combo_after_miss: u32,
    pub is_failing: bool,
    pub fail_time: Option<f32>,
    pub calories_burned: f32,

    pub earned_grade_points: i32,

    pub combo_milestones: Vec<ActiveComboMilestone>,
    pub hands_achieved: u32,
    pub holds_held: u32,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub mines_hit: u32,
    pub mines_hit_for_score: u32,
    pub mines_avoided: u32,
    pub hands_holding_count_for_stats: i32,
    pub failed_ex_score_inputs: Option<ExScoreInputs>,
    pub course_submit_life: Option<deadsync_rules::life::LifeMeter>,

    pub life_history: Vec<(f32, f32)>,

    pub error_bar_mono_ticks: [Option<ErrorBarTick>; 15],
    pub error_bar_mono_next: usize,
    pub error_bar_color_ticks: [Option<ErrorBarTick>; 10],
    pub error_bar_color_next: usize,
    pub error_bar_color_bar_started_at: Option<f32>,
    pub error_bar_color_flash_early: [Option<f32>; 6],
    pub error_bar_color_flash_late: [Option<f32>; 6],
    pub error_bar_text: Option<ErrorBarText>,
    pub offset_indicator_text: Option<OffsetIndicatorText>,
    pub error_bar_avg_ticks: [Option<ErrorBarTick>; 5],
    pub error_bar_avg_next: usize,
    pub error_bar_avg_bar_started_at: Option<f32>,
    pub error_bar_avg_samples: VecDeque<(f32, f32)>,
    pub error_bar_long_avg_samples: VecDeque<(f32, f32)>,
    pub error_bar_long_avg_total: f32,
    pub error_bar_long_avg_tick: Option<ErrorBarTick>,
    pub error_bar_long_avg_visible: bool,
    pub live_timing_stats: deadsync_rules::timing::LiveTimingStats,
}

#[derive(Clone, Debug)]
pub struct GameplayPlayersRuntimeState {
    pub players: [PlayerRuntime; MAX_PLAYERS],
}

#[derive(Clone, Debug)]
pub struct GameplayProfilesRuntimeState<T> {
    pub profiles: [T; MAX_PLAYERS],
}

pub fn init_player_runtime() -> PlayerRuntime {
    PlayerRuntime {
        combo: 0,
        miss_combo: 0,
        full_combo_grade: None,
        current_combo_grade: None,
        current_combo_window_counts: WindowCounts::default(),
        first_fc_attempt_broken: false,
        judgment_counts: [0; judgment::JUDGE_GRADE_COUNT],
        scoring_counts: [0; judgment::JUDGE_GRADE_COUNT],
        last_judgment: None,
        last_mine_judgment: None,
        life: 0.5,
        combo_after_miss: 0,
        is_failing: false,
        fail_time: None,
        calories_burned: 0.0,
        earned_grade_points: 0,
        combo_milestones: Vec::new(),
        hands_achieved: 0,
        holds_held: 0,
        holds_held_for_score: 0,
        holds_let_go_for_score: 0,
        rolls_held: 0,
        rolls_held_for_score: 0,
        rolls_let_go_for_score: 0,
        mines_hit: 0,
        mines_hit_for_score: 0,
        mines_avoided: 0,
        hands_holding_count_for_stats: 0,
        failed_ex_score_inputs: None,
        course_submit_life: None,
        life_history: Vec::with_capacity(10000),
        error_bar_mono_ticks: [None; 15],
        error_bar_mono_next: 0,
        error_bar_color_ticks: [None; 10],
        error_bar_color_next: 0,
        error_bar_color_bar_started_at: None,
        error_bar_color_flash_early: [None; 6],
        error_bar_color_flash_late: [None; 6],
        error_bar_text: None,
        offset_indicator_text: None,
        error_bar_avg_ticks: [None; 5],
        error_bar_avg_next: 0,
        error_bar_avg_bar_started_at: None,
        error_bar_avg_samples: VecDeque::with_capacity(64),
        error_bar_long_avg_samples: VecDeque::with_capacity(64),
        error_bar_long_avg_total: 0.0,
        error_bar_long_avg_tick: None,
        error_bar_long_avg_visible: false,
        live_timing_stats: deadsync_rules::timing::LiveTimingStats::default(),
    }
}

pub fn init_player_runtime_for_practice(judge_start_music_time: f32) -> PlayerRuntime {
    let mut player = init_player_runtime();
    player
        .life_history
        .push((judge_start_music_time, player.life));
    player
}

pub fn init_player_runtime_for_song(
    init_music_time: f32,
    in_course_stage: bool,
    course_carry: Option<CourseDisplayCarry>,
    carry_combo_between_songs: bool,
    replay_mode: bool,
    combo_carry: u32,
) -> PlayerRuntime {
    let mut player = init_player_runtime();
    if in_course_stage {
        player.course_submit_life = Some(deadsync_rules::life::LifeMeter::course_submit_start());
    }
    player.life = course_life_after_carry(player.life, course_carry);
    apply_course_combo_carry(
        &mut player,
        carry_combo_between_songs,
        replay_mode,
        combo_carry,
        course_carry,
    );
    player.life_history.push((init_music_time, player.life));
    player
}

#[inline(always)]
pub const fn player_life(player: &PlayerRuntime) -> f32 {
    player.life
}

#[inline(always)]
pub fn add_player_step_calories(player: &mut PlayerRuntime, calories: f32) {
    player.calories_burned += calories;
}

#[inline(always)]
pub fn tick_player_combo_milestones(player: &mut PlayerRuntime, delta_time: f32) {
    tick_combo_milestones(&mut player.combo_milestones, delta_time);
}

#[inline(always)]
pub fn record_player_live_timing_stats(player: &mut PlayerRuntime, judgment: &Judgment) {
    deadsync_rules::timing::record_live_timing_stats(&mut player.live_timing_stats, judgment);
}

#[inline(always)]
pub const fn player_mines_hit(player: &PlayerRuntime) -> u32 {
    player.mines_hit
}

#[inline(always)]
pub fn add_player_mines_avoided(player: &mut PlayerRuntime, count: u32) {
    player.mines_avoided = player.mines_avoided.saturating_add(count);
}

#[inline(always)]
pub fn set_player_mines_avoided(player: &mut PlayerRuntime, count: u32) {
    player.mines_avoided = count;
}

#[inline(always)]
pub fn set_player_last_judgment(
    player: &mut PlayerRuntime,
    judgment: Judgment,
    started_at_screen_s: f32,
) {
    player.last_judgment = Some(judgment_render_info(judgment, started_at_screen_s));
}

#[inline(always)]
pub fn set_player_last_mine_judgment(
    player: &mut PlayerRuntime,
    result: MineResult,
    column: usize,
    started_at_screen_s: f32,
) {
    player.last_mine_judgment = Some(mine_judgment_render_info(
        result,
        column,
        started_at_screen_s,
    ));
}

#[inline(always)]
pub fn player_combo_state(player: &PlayerRuntime) -> ComboState {
    ComboState {
        combo: player.combo,
        miss_combo: player.miss_combo,
        full_combo_grade: player.full_combo_grade,
        current_combo_grade: player.current_combo_grade,
        first_fc_attempt_broken: player.first_fc_attempt_broken,
    }
}

#[inline(always)]
pub fn write_player_combo_state(player: &mut PlayerRuntime, state: ComboState) {
    player.combo = state.combo;
    player.miss_combo = state.miss_combo;
    player.full_combo_grade = state.full_combo_grade;
    player.current_combo_grade = state.current_combo_grade;
    player.first_fc_attempt_broken = state.first_fc_attempt_broken;
}

#[inline(always)]
pub fn apply_combo_update(player: &mut PlayerRuntime, update: ComboUpdate) {
    apply_combo_update_feedback(
        &mut player.current_combo_window_counts,
        &mut player.combo_milestones,
        update,
    );
}

#[inline(always)]
pub fn update_itg_grade_totals(player: &mut PlayerRuntime) {
    player.earned_grade_points = judgment::calculate_itg_grade_points_from_counts(
        &player.scoring_counts,
        player.holds_held_for_score,
        player.rolls_held_for_score,
        player.mines_hit_for_score,
    );
}

#[inline(always)]
pub fn apply_course_combo_carry(
    player: &mut PlayerRuntime,
    carry_combo_between_songs: bool,
    replay_mode: bool,
    combo_carry: u32,
    course_carry: Option<CourseDisplayCarry>,
) {
    let mut state = CourseComboCarryState {
        combo: player.combo,
        full_combo_grade: player.full_combo_grade,
        current_combo_grade: player.current_combo_grade,
        current_combo_window_counts: player.current_combo_window_counts,
        first_fc_attempt_broken: player.first_fc_attempt_broken,
    };
    apply_course_combo_carry_state(
        &mut state,
        carry_combo_between_songs,
        replay_mode,
        combo_carry,
        course_carry,
    );
    player.combo = state.combo;
    player.full_combo_grade = state.full_combo_grade;
    player.current_combo_grade = state.current_combo_grade;
    player.current_combo_window_counts = state.current_combo_window_counts;
    player.first_fc_attempt_broken = state.first_fc_attempt_broken;
}

#[inline(always)]
pub fn player_score_stage(player: &PlayerRuntime) -> ItgScoreStage {
    ItgScoreStage {
        scoring_counts: player.scoring_counts,
        holds_held_for_score: player.holds_held_for_score,
        holds_let_go_for_score: player.holds_let_go_for_score,
        rolls_held_for_score: player.rolls_held_for_score,
        rolls_let_go_for_score: player.rolls_let_go_for_score,
        mines_hit_for_score: player.mines_hit_for_score,
    }
}

#[inline(always)]
pub fn player_display_judgment_count(
    player: &PlayerRuntime,
    carry: CourseDisplayCarry,
    grade: JudgeGrade,
) -> u32 {
    display_judgment_count_for_grade(player.judgment_counts, carry, grade)
}

#[inline(always)]
pub fn player_live_timing_snapshot(
    player: &PlayerRuntime,
) -> deadsync_rules::timing::LiveTimingSnapshot {
    deadsync_rules::timing::live_timing_stats_snapshot(&player.live_timing_stats)
}

#[inline(always)]
pub fn player_course_submit_life_eligible(player: &PlayerRuntime) -> bool {
    course_submit_life_eligible(player.course_submit_life.as_ref())
}

#[inline(always)]
pub fn capture_player_failed_ex_score_inputs(
    player: &mut PlayerRuntime,
    live: ExScoreInputs,
) -> bool {
    capture_failed_ex_score_inputs(&mut player.failed_ex_score_inputs, player.fail_time, live)
}

#[inline(always)]
pub fn player_effective_ex_score_inputs(
    player: &PlayerRuntime,
    live: ExScoreInputs,
) -> ExScoreInputs {
    effective_ex_score_inputs(live, player.failed_ex_score_inputs)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FinalNoteResultEffects {
    pub mark_row_finalized: bool,
    pub trigger_miss_flash_column: Option<usize>,
    pub held_miss_column: Option<usize>,
}

#[inline(always)]
pub const fn final_note_result_effects(
    was_unjudged: bool,
    judgment: &Judgment,
    column: usize,
    column_count: usize,
) -> FinalNoteResultEffects {
    if !was_unjudged {
        return FinalNoteResultEffects {
            mark_row_finalized: false,
            trigger_miss_flash_column: None,
            held_miss_column: None,
        };
    }
    let is_miss = matches!(judgment.grade, JudgeGrade::Miss);
    FinalNoteResultEffects {
        mark_row_finalized: true,
        trigger_miss_flash_column: if is_miss { Some(column) } else { None },
        held_miss_column: if is_miss && judgment.miss_because_held && column < column_count {
            Some(column)
        } else {
            None
        },
    }
}

pub fn register_provisional_early_note_result(note: &mut Note, judgment: Judgment) -> bool {
    if note.early_result.is_some() {
        return false;
    }
    note.early_result = Some(judgment);
    true
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProvisionalEarlyNoteResultUpdate {
    pub registered: bool,
    pub marked_row_entry: bool,
}

pub fn apply_provisional_early_note_result(
    notes: &mut [Note],
    row_entries: &mut [RowEntry],
    note_row_entry_indices: &[u32],
    note_index: usize,
    judgment: Judgment,
) -> ProvisionalEarlyNoteResultUpdate {
    let Some(note) = notes.get_mut(note_index) else {
        return ProvisionalEarlyNoteResultUpdate::default();
    };
    let registered = register_provisional_early_note_result(note, judgment);
    let marked_row_entry = registered
        && mark_row_entry_provisional_early_result(row_entries, note_row_entry_indices, note_index);
    ProvisionalEarlyNoteResultUpdate {
        registered,
        marked_row_entry,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeBasedTapMissScan {
    Stop,
    Skip,
    Miss,
}

pub const fn time_based_tap_miss_scan(note: &Note, cutoff_row: usize) -> TimeBasedTapMissScan {
    if note.row_index >= cutoff_row {
        TimeBasedTapMissScan::Stop
    } else if matches!(note.note_type, NoteType::Mine)
        || !note.can_be_judged
        || note.result.is_some()
    {
        TimeBasedTapMissScan::Skip
    } else {
        TimeBasedTapMissScan::Miss
    }
}

pub fn time_based_tap_miss_judgment(
    early_result: Option<Judgment>,
    note_time_ns: SongTimeNs,
    music_time_ns: SongTimeNs,
    music_rate: f32,
    miss_because_held: bool,
) -> Judgment {
    if let Some(judgment) = early_result {
        return judgment;
    }
    let miss_offset_music_ns = music_time_ns.saturating_sub(note_time_ns);
    Judgment {
        time_error_ms: judgment::judgment_time_error_ms_from_music_ns(
            miss_offset_music_ns,
            music_rate,
        ),
        time_error_music_ns: miss_offset_music_ns,
        grade: JudgeGrade::Miss,
        window: None,
        miss_because_held,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TimeBasedTapMissEvent {
    pub note_index: usize,
    pub row_index: usize,
    pub column: usize,
    pub beat: f32,
    pub note_time_ns: SongTimeNs,
    pub judgment: Judgment,
    pub miss_because_held: bool,
    pub queue_missed_hold_resolution: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct TimeBasedTapMissStep {
    pub next_cursor: usize,
    pub event: Option<TimeBasedTapMissEvent>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimeBasedTapMissPlayerUpdate {
    pub next_cursor: usize,
    pub event_count: usize,
    pub stopped: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct TimeBasedTapMissPlayerEvent {
    pub player: usize,
    pub event: TimeBasedTapMissEvent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimeBasedTapMissPlayersUpdate {
    pub players_scanned: usize,
    pub event_count: usize,
    pub stopped: bool,
}

pub fn apply_next_time_based_tap_miss_for_player(
    notes: &mut [Note],
    note_time_cache_ns: &[SongTimeNs],
    tap_miss_held_window: &[bool],
    hold_decay_active: &mut [bool],
    decaying_hold_indices: &mut Vec<usize>,
    cursor: usize,
    note_range: (usize, usize),
    cutoff_row: usize,
    music_time_ns: SongTimeNs,
    music_rate: f32,
    score_missed_holds_rolls: bool,
) -> TimeBasedTapMissStep {
    let end = note_range.1.min(notes.len()).min(note_time_cache_ns.len());
    let mut cursor = cursor.max(note_range.0.min(end));
    while cursor < end {
        let note_time_ns = note_time_cache_ns[cursor];
        let (row_index, column, beat) = {
            let note = &notes[cursor];
            match time_based_tap_miss_scan(note, cutoff_row) {
                TimeBasedTapMissScan::Stop => {
                    return TimeBasedTapMissStep {
                        next_cursor: cursor,
                        event: None,
                    };
                }
                TimeBasedTapMissScan::Skip => {
                    cursor += 1;
                    continue;
                }
                TimeBasedTapMissScan::Miss => (note.row_index, note.column, note.beat),
            }
        };

        let miss_because_held = tap_miss_held_window.get(cursor).copied().unwrap_or(false);
        let judgment = time_based_tap_miss_judgment(
            notes[cursor].early_result,
            note_time_ns,
            music_time_ns,
            music_rate,
            miss_because_held,
        );
        let queue_missed_hold_resolution = apply_time_based_hold_miss_result(
            notes[cursor].hold.as_mut(),
            hold_decay_active,
            decaying_hold_indices,
            cursor,
            music_time_ns,
            judgment.grade,
            score_missed_holds_rolls,
        );
        return TimeBasedTapMissStep {
            next_cursor: cursor + 1,
            event: Some(TimeBasedTapMissEvent {
                note_index: cursor,
                row_index,
                column,
                beat,
                note_time_ns,
                judgment,
                miss_because_held,
                queue_missed_hold_resolution,
            }),
        };
    }

    TimeBasedTapMissStep {
        next_cursor: cursor,
        event: None,
    }
}

pub fn collect_time_based_tap_misses_for_player(
    notes: &mut [Note],
    note_time_cache_ns: &[SongTimeNs],
    tap_miss_held_window: &[bool],
    hold_decay_active: &mut [bool],
    decaying_hold_indices: &mut Vec<usize>,
    cursor: usize,
    note_range: (usize, usize),
    cutoff_row: usize,
    music_time_ns: SongTimeNs,
    music_rate: f32,
    score_missed_holds_rolls: bool,
    events: &mut [Option<TimeBasedTapMissEvent>],
) -> TimeBasedTapMissPlayerUpdate {
    let end = note_range.1.min(notes.len()).min(note_time_cache_ns.len());
    let mut cursor = cursor.max(note_range.0.min(end));
    let mut event_count = 0usize;

    while cursor < end && event_count < events.len() {
        let step = apply_next_time_based_tap_miss_for_player(
            notes,
            note_time_cache_ns,
            tap_miss_held_window,
            hold_decay_active,
            decaying_hold_indices,
            cursor,
            note_range,
            cutoff_row,
            music_time_ns,
            music_rate,
            score_missed_holds_rolls,
        );
        cursor = step.next_cursor;
        let Some(event) = step.event else {
            return TimeBasedTapMissPlayerUpdate {
                next_cursor: cursor,
                event_count,
                stopped: true,
            };
        };
        events[event_count] = Some(event);
        event_count += 1;
    }

    TimeBasedTapMissPlayerUpdate {
        next_cursor: cursor,
        event_count,
        stopped: cursor >= end,
    }
}

pub fn collect_time_based_tap_misses_for_players(
    notes: &mut [Note],
    note_time_cache_ns: &[SongTimeNs],
    tap_miss_held_window: &[bool],
    hold_decay_active: &mut [bool],
    decaying_hold_indices: &mut Vec<usize>,
    next_cursors: &mut [usize],
    note_ranges: &[(usize, usize)],
    cutoff_rows: &[usize],
    music_time_ns: SongTimeNs,
    music_rate: f32,
    score_missed_holds_rolls: &[bool],
    num_players: usize,
    events: &mut [Option<TimeBasedTapMissPlayerEvent>],
) -> TimeBasedTapMissPlayersUpdate {
    let active_players = num_players.min(MAX_PLAYERS);
    let mut event_count = 0usize;
    let mut players_scanned = 0usize;

    for player in 0..active_players {
        let note_range = player_note_range_for_ranges(note_ranges, active_players, player);
        let end = note_range.1.min(notes.len()).min(note_time_cache_ns.len());
        let mut cursor = next_cursors
            .get(player)
            .copied()
            .unwrap_or(note_range.0)
            .max(note_range.0.min(end));
        while cursor < end {
            if event_count >= events.len() {
                if let Some(next_cursor) = next_cursors.get_mut(player) {
                    *next_cursor = cursor;
                }
                return TimeBasedTapMissPlayersUpdate {
                    players_scanned: player + 1,
                    event_count,
                    stopped: true,
                };
            }
            let step = apply_next_time_based_tap_miss_for_player(
                notes,
                note_time_cache_ns,
                tap_miss_held_window,
                hold_decay_active,
                decaying_hold_indices,
                cursor,
                note_range,
                cutoff_rows.get(player).copied().unwrap_or(0),
                music_time_ns,
                music_rate,
                score_missed_holds_rolls
                    .get(player)
                    .copied()
                    .unwrap_or(false),
            );
            cursor = step.next_cursor;
            if let Some(next_cursor) = next_cursors.get_mut(player) {
                *next_cursor = cursor;
            }
            let Some(event) = step.event else {
                break;
            };
            events[event_count] = Some(TimeBasedTapMissPlayerEvent { player, event });
            event_count += 1;
        }
        players_scanned = player + 1;
    }

    TimeBasedTapMissPlayersUpdate {
        players_scanned,
        event_count,
        stopped: false,
    }
}

pub fn apply_final_note_result(
    note: &mut Note,
    judgment: Judgment,
    column_count: usize,
) -> FinalNoteResultEffects {
    let effects =
        final_note_result_effects(note.result.is_none(), &judgment, note.column, column_count);
    note.result = Some(judgment);
    effects
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FinalNoteResultUpdate {
    pub effects: FinalNoteResultEffects,
    pub marked_row_entry: bool,
}

pub fn apply_final_note_result_to_rows(
    notes: &mut [Note],
    row_entries: &mut [RowEntry],
    note_row_entry_indices: &[u32],
    note_index: usize,
    judgment: Judgment,
    column_count: usize,
) -> FinalNoteResultUpdate {
    let Some(note) = notes.get_mut(note_index) else {
        return FinalNoteResultUpdate::default();
    };
    let note_type = note.note_type;
    let effects = apply_final_note_result(note, judgment, column_count);
    let marked_row_entry = effects.mark_row_finalized
        && mark_row_entry_note_finalized(
            row_entries,
            note_row_entry_indices,
            note_index,
            note_type,
        );
    FinalNoteResultUpdate {
        effects,
        marked_row_entry,
    }
}

pub const HOLD_JUDGMENT_TOTAL_DURATION: f32 = 0.8;
pub const HELD_MISS_TOTAL_DURATION: f32 = 0.5;
pub const RECEPTOR_GLOW_DURATION: f32 = 0.2;
pub const COLUMN_FLASH_MISS_DURATION: f32 = 0.16;
pub const COLUMN_FLASH_JUDGMENT_DURATION: f32 = 0.33;
pub const COMBO_HUNDRED_MILESTONE_DURATION: f32 = 0.6;
pub const COMBO_THOUSAND_MILESTONE_DURATION: f32 = 0.7;

