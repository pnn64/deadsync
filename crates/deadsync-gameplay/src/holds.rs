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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveHoldResolution {
    LetGo {
        note_index: usize,
        time_ns: SongTimeNs,
    },
    Success {
        note_index: usize,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActiveHoldAdvance {
    pub clear_active: bool,
    pub resolution: Option<ActiveHoldResolution>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HoldResultStatsUpdate {
    pub decrement_hands_holding: bool,
    pub holds_held: u32,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub update_grade_totals: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HoldResultStatsState {
    pub hands_holding_count_for_stats: i32,
    pub holds_held: u32,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
}

pub fn apply_hold_result_stats_update(
    state: &mut HoldResultStatsState,
    update: HoldResultStatsUpdate,
) {
    if update.decrement_hands_holding && state.hands_holding_count_for_stats > 0 {
        state.hands_holding_count_for_stats -= 1;
    }
    state.holds_held = state.holds_held.saturating_add(update.holds_held);
    state.holds_held_for_score = state
        .holds_held_for_score
        .saturating_add(update.holds_held_for_score);
    state.holds_let_go_for_score = state
        .holds_let_go_for_score
        .saturating_add(update.holds_let_go_for_score);
    state.rolls_held = state.rolls_held.saturating_add(update.rolls_held);
    state.rolls_held_for_score = state
        .rolls_held_for_score
        .saturating_add(update.rolls_held_for_score);
    state.rolls_let_go_for_score = state
        .rolls_let_go_for_score
        .saturating_add(update.rolls_let_go_for_score);
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HoldResolutionPlayerState {
    pub stats: HoldResultStatsState,
    pub combo: ComboState,
}

pub fn hold_result_stats_state(player: &PlayerRuntime) -> HoldResultStatsState {
    HoldResultStatsState {
        hands_holding_count_for_stats: player.hands_holding_count_for_stats,
        holds_held: player.holds_held,
        holds_held_for_score: player.holds_held_for_score,
        holds_let_go_for_score: player.holds_let_go_for_score,
        rolls_held: player.rolls_held,
        rolls_held_for_score: player.rolls_held_for_score,
        rolls_let_go_for_score: player.rolls_let_go_for_score,
    }
}

pub fn set_hold_result_stats_state(player: &mut PlayerRuntime, stats: HoldResultStatsState) {
    player.hands_holding_count_for_stats = stats.hands_holding_count_for_stats;
    player.holds_held = stats.holds_held;
    player.holds_held_for_score = stats.holds_held_for_score;
    player.holds_let_go_for_score = stats.holds_let_go_for_score;
    player.rolls_held = stats.rolls_held;
    player.rolls_held_for_score = stats.rolls_held_for_score;
    player.rolls_let_go_for_score = stats.rolls_let_go_for_score;
}

pub fn hold_resolution_player_state(player: &PlayerRuntime) -> HoldResolutionPlayerState {
    HoldResolutionPlayerState {
        stats: hold_result_stats_state(player),
        combo: player_combo_state(player),
    }
}

pub fn set_hold_resolution_player_state(
    player: &mut PlayerRuntime,
    state: HoldResolutionPlayerState,
) {
    set_hold_result_stats_state(player, state.stats);
    write_player_combo_state(player, state.combo);
}

pub fn apply_hold_resolution_player_state(
    player: &mut PlayerRuntime,
    state: HoldResolutionPlayerState,
) {
    set_hold_resolution_player_state(player, state);
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HoldResolutionPlayerUpdate {
    pub stats_update: HoldResultStatsUpdate,
    pub combo_update: ComboUpdate,
    pub life_delta: f32,
    pub apply_life_change: bool,
    pub capture_failed_ex_score_inputs: bool,
}

pub fn apply_hold_let_go_player_state(
    state: &mut HoldResolutionPlayerState,
    stats_update: HoldResultStatsUpdate,
    scoring_blocked: bool,
) -> HoldResolutionPlayerUpdate {
    apply_hold_result_stats_update(&mut state.stats, stats_update);
    let combo_update = if scoring_blocked {
        ComboUpdate::default()
    } else {
        apply_hold_let_go_combo_policy(&mut state.combo)
    };
    HoldResolutionPlayerUpdate {
        stats_update,
        combo_update,
        life_delta: deadsync_rules::life::LIFE_LET_GO,
        apply_life_change: !scoring_blocked,
        capture_failed_ex_score_inputs: !scoring_blocked,
    }
}

pub fn apply_hold_success_player_state(
    state: &mut HoldResolutionPlayerState,
    stats_update: HoldResultStatsUpdate,
    scoring_blocked: bool,
) -> HoldResolutionPlayerUpdate {
    apply_hold_result_stats_update(&mut state.stats, stats_update);
    let combo_update = if scoring_blocked {
        ComboUpdate::default()
    } else {
        apply_hold_success_combo_policy(&mut state.combo)
    };
    HoldResolutionPlayerUpdate {
        stats_update,
        combo_update,
        life_delta: deadsync_rules::life::LIFE_HELD,
        apply_life_change: !scoring_blocked,
        capture_failed_ex_score_inputs: !scoring_blocked,
    }
}

#[inline(always)]
pub const fn replaced_active_hold_settle_time(
    active_note_index: usize,
    active_end_time_ns: SongTimeNs,
    next_note_index: usize,
    next_start_time_ns: SongTimeNs,
) -> Option<SongTimeNs> {
    if active_note_index == next_note_index || active_end_time_ns > next_start_time_ns {
        None
    } else {
        Some(active_end_time_ns)
    }
}

#[inline(always)]
pub fn begin_hold_life_decay(
    hold: &mut HoldData,
    hold_decay_active: &mut [bool],
    decaying_hold_indices: &mut Vec<usize>,
    note_index: usize,
    start_time_ns: SongTimeNs,
) {
    if hold.let_go_started_at.is_none() {
        hold.let_go_started_at = Some(start_time_ns);
        hold.let_go_starting_life = hold.life.clamp(0.0, MAX_HOLD_LIFE);
    }
    if note_index < hold_decay_active.len() && !hold_decay_active[note_index] {
        hold_decay_active[note_index] = true;
        decaying_hold_indices.push(note_index);
    }
}

pub fn apply_hold_let_go_result(
    hold: Option<&mut HoldData>,
    hold_decay_active: &mut [bool],
    decaying_hold_indices: &mut Vec<usize>,
    note_index: usize,
    let_go_time_ns: SongTimeNs,
) -> bool {
    let Some(hold) = hold else {
        return true;
    };
    if hold.result == Some(HoldResult::LetGo) {
        return false;
    }
    hold.result = Some(HoldResult::LetGo);
    begin_hold_life_decay(
        hold,
        hold_decay_active,
        decaying_hold_indices,
        note_index,
        let_go_time_ns,
    );
    true
}

pub fn apply_hold_success_result(
    hold: Option<&mut HoldData>,
    hold_decay_active: &mut [bool],
    note_index: usize,
) -> bool {
    let Some(hold) = hold else {
        return true;
    };
    if hold.result == Some(HoldResult::Held) {
        return false;
    }
    hold.result = Some(HoldResult::Held);
    hold.life = MAX_HOLD_LIFE;
    hold.let_go_started_at = None;
    hold.let_go_starting_life = 0.0;
    hold.last_held_row_index = hold.end_row_index;
    hold.last_held_beat = hold.end_beat;
    if note_index < hold_decay_active.len() {
        hold_decay_active[note_index] = false;
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoldResolutionEffects {
    pub show_judgment: bool,
    pub reset_receptor_glow: bool,
    pub trigger_hold_explosion: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoldResolutionUpdate {
    pub result: HoldResult,
    pub stats_update: HoldResultStatsUpdate,
    pub effects: HoldResolutionEffects,
}

pub fn apply_hold_let_go_update(
    hold: Option<&mut HoldData>,
    hold_decay_active: &mut [bool],
    decaying_hold_indices: &mut Vec<usize>,
    note_index: usize,
    note_type: NoteType,
    let_go_time_ns: SongTimeNs,
    scoring_blocked: bool,
    player_dead: bool,
) -> Option<HoldResolutionUpdate> {
    if !apply_hold_let_go_result(
        hold,
        hold_decay_active,
        decaying_hold_indices,
        note_index,
        let_go_time_ns,
    ) {
        return None;
    }
    let result = HoldResult::LetGo;
    Some(HoldResolutionUpdate {
        result,
        stats_update: hold_result_stats_update(note_type, result, scoring_blocked, player_dead),
        effects: HoldResolutionEffects {
            show_judgment: true,
            reset_receptor_glow: true,
            trigger_hold_explosion: false,
        },
    })
}

pub fn apply_hold_success_update(
    hold: Option<&mut HoldData>,
    hold_decay_active: &mut [bool],
    note_index: usize,
    note_type: NoteType,
    scoring_blocked: bool,
    player_dead: bool,
) -> Option<HoldResolutionUpdate> {
    if !apply_hold_success_result(hold, hold_decay_active, note_index) {
        return None;
    }
    let result = HoldResult::Held;
    Some(HoldResolutionUpdate {
        result,
        stats_update: hold_result_stats_update(note_type, result, scoring_blocked, player_dead),
        effects: HoldResolutionEffects {
            show_judgment: true,
            reset_receptor_glow: false,
            trigger_hold_explosion: true,
        },
    })
}

pub fn apply_time_based_hold_miss_result(
    hold: Option<&mut HoldData>,
    hold_decay_active: &mut [bool],
    decaying_hold_indices: &mut Vec<usize>,
    note_index: usize,
    miss_time_ns: SongTimeNs,
    judgment_grade: JudgeGrade,
    score_missed_holds_rolls: bool,
) -> bool {
    if judgment_grade != JudgeGrade::Miss {
        return false;
    }
    let Some(hold) = hold else {
        return false;
    };
    if hold.result == Some(HoldResult::Held) {
        return false;
    }
    if !score_missed_holds_rolls {
        hold.result = Some(HoldResult::Missed);
    }
    begin_hold_life_decay(
        hold,
        hold_decay_active,
        decaying_hold_indices,
        note_index,
        miss_time_ns,
    );
    true
}

pub fn decay_let_go_hold_life_step(
    hold: &mut HoldData,
    note_type: NoteType,
    current_time_ns: SongTimeNs,
    music_rate: f32,
) -> bool {
    if hold.result == Some(HoldResult::Held) {
        return false;
    }
    let Some(start_time_ns) = hold.let_go_started_at else {
        return false;
    };
    let elapsed_ns = if current_time_ns > start_time_ns {
        current_time_ns - start_time_ns
    } else {
        0
    };
    let advance = advance_hold_life_ns(
        note_type,
        hold.let_go_starting_life,
        false,
        elapsed_ns,
        music_rate,
    );
    hold.life = advance.life_after;
    hold.life > f32::EPSILON
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HoldLifeDecayUpdate {
    pub remaining_count: usize,
    pub removed_count: usize,
}

pub fn decay_let_go_hold_life_for_indices(
    notes: &mut [Note],
    hold_decay_active: &mut [bool],
    decaying_hold_indices: &mut Vec<usize>,
    current_time_ns: SongTimeNs,
    music_rate: f32,
) -> HoldLifeDecayUpdate {
    let original_count = decaying_hold_indices.len();
    let mut i = 0usize;
    while i < decaying_hold_indices.len() {
        let note_index = decaying_hold_indices[i];
        let Some(note) = notes.get_mut(note_index) else {
            decaying_hold_indices.swap_remove(i);
            continue;
        };
        let Some(hold) = note.hold.as_mut() else {
            if let Some(active) = hold_decay_active.get_mut(note_index) {
                *active = false;
            }
            decaying_hold_indices.swap_remove(i);
            continue;
        };
        if !decay_let_go_hold_life_step(hold, note.note_type, current_time_ns, music_rate) {
            if let Some(active) = hold_decay_active.get_mut(note_index) {
                *active = false;
            }
            decaying_hold_indices.swap_remove(i);
            continue;
        }
        i += 1;
    }

    HoldLifeDecayUpdate {
        remaining_count: decaying_hold_indices.len(),
        removed_count: original_count.saturating_sub(decaying_hold_indices.len()),
    }
}

pub fn queue_pending_missed_hold_resolution(
    pending_resolution: &mut [bool],
    pending_indices: &mut Vec<usize>,
    note_index: usize,
) -> bool {
    if note_index >= pending_resolution.len() || pending_resolution[note_index] {
        return false;
    }
    pending_resolution[note_index] = true;
    pending_indices.push(note_index);
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingMissedHoldResolution {
    None,
    ShowMissedFeedback,
    ScoreLetGo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingMissedHoldResolutionStep {
    Wait,
    Remove,
    Resolve(PendingMissedHoldResolution),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingMissedHoldResolutionEvent {
    pub note_index: usize,
    pub column: usize,
    pub end_time_ns: SongTimeNs,
    pub resolution: PendingMissedHoldResolution,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PendingMissedHoldResolutionUpdate {
    pub event_count: usize,
    pub finished: bool,
}

#[inline(always)]
pub const fn pending_missed_hold_resolution_action(
    hold_result: Option<HoldResult>,
    note_result_grade: Option<JudgeGrade>,
    score_missed_holds_rolls: bool,
) -> PendingMissedHoldResolution {
    if matches!(hold_result, Some(HoldResult::Missed)) {
        PendingMissedHoldResolution::ShowMissedFeedback
    } else if hold_result.is_none()
        && matches!(note_result_grade, Some(JudgeGrade::Miss))
        && score_missed_holds_rolls
    {
        PendingMissedHoldResolution::ScoreLetGo
    } else {
        PendingMissedHoldResolution::None
    }
}

pub fn pending_missed_hold_resolution_for_note(
    note: Option<&Note>,
    hold_end_time_ns: Option<SongTimeNs>,
    current_time_ns: SongTimeNs,
    num_cols: usize,
    score_missed_holds_rolls: bool,
) -> PendingMissedHoldResolutionStep {
    let Some(end_time_ns) = hold_end_time_ns else {
        return PendingMissedHoldResolutionStep::Remove;
    };
    if current_time_ns < end_time_ns {
        return PendingMissedHoldResolutionStep::Wait;
    }
    let Some(note) = note else {
        return PendingMissedHoldResolutionStep::Remove;
    };
    if note.column >= num_cols {
        return PendingMissedHoldResolutionStep::Remove;
    }
    PendingMissedHoldResolutionStep::Resolve(pending_missed_hold_resolution_action(
        note.hold.as_ref().and_then(|hold| hold.result),
        note.result.as_ref().map(|judgment| judgment.grade),
        score_missed_holds_rolls,
    ))
}

pub fn collect_pending_missed_hold_resolutions(
    notes: &[Note],
    hold_end_time_cache_ns: &[Option<SongTimeNs>],
    pending_resolution: &mut [bool],
    pending_indices: &mut Vec<usize>,
    current_time_ns: SongTimeNs,
    score_missed_holds_rolls_by_column: &[bool],
    events: &mut [Option<PendingMissedHoldResolutionEvent>],
) -> PendingMissedHoldResolutionUpdate {
    let mut i = 0usize;
    let mut event_count = 0usize;
    while i < pending_indices.len() {
        let note_index = pending_indices[i];
        let end_time_ns = hold_end_time_cache_ns
            .get(note_index)
            .and_then(|time| *time);
        let note = notes.get(note_index);
        let score_missed_holds_rolls = note
            .and_then(|note| score_missed_holds_rolls_by_column.get(note.column))
            .copied()
            .unwrap_or(false);
        let step = pending_missed_hold_resolution_for_note(
            note,
            end_time_ns,
            current_time_ns,
            score_missed_holds_rolls_by_column.len(),
            score_missed_holds_rolls,
        );
        match step {
            PendingMissedHoldResolutionStep::Wait => {
                i += 1;
                continue;
            }
            PendingMissedHoldResolutionStep::Remove
            | PendingMissedHoldResolutionStep::Resolve(PendingMissedHoldResolution::None) => {}
            PendingMissedHoldResolutionStep::Resolve(resolution) => {
                if event_count >= events.len() {
                    return PendingMissedHoldResolutionUpdate {
                        event_count,
                        finished: false,
                    };
                }
                let note = note.expect("resolved missed hold event has a live note");
                events[event_count] = Some(PendingMissedHoldResolutionEvent {
                    note_index,
                    column: note.column,
                    end_time_ns: end_time_ns.expect("resolved missed hold event has an end time"),
                    resolution,
                });
                event_count += 1;
            }
        }
        if let Some(pending) = pending_resolution.get_mut(note_index) {
            *pending = false;
        }
        pending_indices.swap_remove(i);
    }

    PendingMissedHoldResolutionUpdate {
        event_count,
        finished: true,
    }
}

pub const fn hold_result_stats_update(
    note_type: NoteType,
    result: HoldResult,
    scoring_blocked: bool,
    player_dead: bool,
) -> HoldResultStatsUpdate {
    let mut update = HoldResultStatsUpdate {
        decrement_hands_holding: true,
        ..HoldResultStatsUpdate::ZERO
    };
    if scoring_blocked {
        return update;
    }

    match (note_type, result, player_dead) {
        (NoteType::Hold, HoldResult::Held, false) => {
            update.holds_held = 1;
            update.holds_held_for_score = 1;
            update.update_grade_totals = true;
        }
        (NoteType::Hold, HoldResult::Held, true) => {
            update.holds_held = 1;
        }
        (NoteType::Hold, HoldResult::LetGo, false) => {
            update.holds_let_go_for_score = 1;
            update.update_grade_totals = true;
        }
        (NoteType::Roll, HoldResult::Held, false) => {
            update.rolls_held = 1;
            update.rolls_held_for_score = 1;
            update.update_grade_totals = true;
        }
        (NoteType::Roll, HoldResult::Held, true) => {
            update.rolls_held = 1;
        }
        (NoteType::Roll, HoldResult::LetGo, false) => {
            update.rolls_let_go_for_score = 1;
            update.update_grade_totals = true;
        }
        _ => {}
    }
    update
}

impl HoldResultStatsUpdate {
    pub const ZERO: Self = Self {
        decrement_hands_holding: false,
        holds_held: 0,
        holds_held_for_score: 0,
        holds_let_go_for_score: 0,
        rolls_held: 0,
        rolls_held_for_score: 0,
        rolls_let_go_for_score: 0,
        update_grade_totals: false,
    };
}

#[inline(always)]
pub const fn hold_resolution_updates_grade_totals(
    result: HoldResult,
    stats_update: HoldResultStatsUpdate,
    player_dead_after_life: bool,
) -> bool {
    if !stats_update.update_grade_totals {
        return false;
    }
    match result {
        HoldResult::LetGo => !player_dead_after_life,
        HoldResult::Held => true,
        HoldResult::Missed => false,
    }
}

pub fn started_active_hold_state(
    hold: Option<&mut HoldData>,
    note_index: usize,
    note_type: NoteType,
    start_time_ns: SongTimeNs,
    end_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
) -> ActiveHold {
    if let Some(hold) = hold {
        hold.life = MAX_HOLD_LIFE;
        hold.let_go_started_at = None;
        hold.let_go_starting_life = 0.0;
    }
    ActiveHold {
        note_index,
        start_time_ns,
        end_time_ns,
        note_type,
        let_go: false,
        is_pressed: true,
        life: MAX_HOLD_LIFE,
        last_update_time_ns: current_time_ns,
    }
}

pub fn refresh_roll_life_for_step(
    active: &mut ActiveHold,
    hold: &mut HoldData,
    event_time_ns: SongTimeNs,
) -> bool {
    if !matches!(active.note_type, NoteType::Roll)
        || active.let_go
        || active.life <= 0.0
        || song_time_ns_invalid(event_time_ns)
        || event_time_ns < active.start_time_ns
        || matches!(hold.result, Some(HoldResult::LetGo | HoldResult::Missed))
    {
        return false;
    }

    active.life = MAX_HOLD_LIFE;
    active.last_update_time_ns = active
        .last_update_time_ns
        .max(event_time_ns.min(active.end_time_ns));
    hold.life = MAX_HOLD_LIFE;
    hold.let_go_started_at = None;
    hold.let_go_starting_life = 0.0;
    true
}

pub fn sync_active_hold_pressed_column(
    active_holds: &mut [Option<ActiveHold>],
    column: usize,
    live_autoplay: bool,
    lane_pressed: bool,
) -> bool {
    let Some(active) = active_holds.get_mut(column).and_then(Option::as_mut) else {
        return false;
    };
    active.is_pressed = active_hold_counts_as_pressed(live_autoplay, lane_pressed);
    true
}

pub fn refresh_roll_life_for_active_column(
    active_holds: &mut [Option<ActiveHold>],
    notes: &mut [Note],
    column: usize,
    event_time_ns: SongTimeNs,
) -> bool {
    let Some(active) = active_holds.get_mut(column).and_then(Option::as_mut) else {
        return false;
    };
    let Some(note) = notes.get_mut(active.note_index) else {
        return false;
    };
    let Some(hold) = note.hold.as_mut() else {
        return false;
    };
    refresh_roll_life_for_step(active, hold, event_time_ns)
}

pub fn advance_active_hold_to_time(
    active: &mut ActiveHold,
    hold: &mut HoldData,
    timing: &TimingData,
    note_start_row: usize,
    note_start_beat: f32,
    target_time_ns: SongTimeNs,
    music_rate: f32,
) -> ActiveHoldAdvance {
    let note_index = active.note_index;
    let from_time_ns = active.last_update_time_ns;
    let final_time_ns = target_time_ns.max(from_time_ns).min(active.end_time_ns);
    let mut resolution = None;

    if !active.let_go && active.life <= 0.0 {
        active.let_go = true;
        resolution = Some(ActiveHoldResolution::LetGo {
            note_index,
            time_ns: from_time_ns.max(active.start_time_ns),
        });
    } else if final_time_ns > from_time_ns && !active.let_go {
        let body_from_ns = from_time_ns.max(active.start_time_ns);
        let body_to_ns = final_time_ns.max(active.start_time_ns);
        if body_to_ns > body_from_ns && active.life > 0.0 {
            let advance = advance_hold_life_ns(
                active.note_type,
                active.life,
                active.is_pressed,
                body_to_ns.saturating_sub(body_from_ns),
                music_rate,
            );
            // ITG updates iLastHeldRow before subtracting hold life for the
            // frame. If this interval drains life to zero, keep the visual
            // last-held row at the frame target while still resolving LetGo at
            // the exact crossing.
            let progress_time = song_time_ns_to_seconds(body_to_ns);
            if body_to_ns > body_from_ns && progress_time.is_finite() {
                let current_beat = timing.get_beat_for_time(progress_time);
                advance_hold_last_held(hold, timing, current_beat, note_start_row, note_start_beat);
            }
            active.life = advance.life_after;
            hold.life = active.life;
            if let Some(zero_elapsed_music_ns) = advance.zero_elapsed_music_ns {
                active.let_go = true;
                resolution = Some(ActiveHoldResolution::LetGo {
                    note_index,
                    time_ns: body_from_ns.saturating_add(zero_elapsed_music_ns),
                });
            }
        }
        active.last_update_time_ns = final_time_ns;
    }

    if !active.let_go {
        hold.let_go_started_at = None;
        hold.let_go_starting_life = 0.0;
    }
    if resolution.is_none() && !active.let_go && final_time_ns >= active.end_time_ns {
        resolution = Some(ActiveHoldResolution::Success { note_index });
    }

    ActiveHoldAdvance {
        clear_active: resolution.is_some() || active.let_go,
        resolution,
    }
}

pub fn integrate_active_hold_column(
    active_holds: &mut [Option<ActiveHold>],
    notes: &mut [Note],
    column: usize,
    timing: &TimingData,
    target_time_ns: SongTimeNs,
    music_rate: f32,
) -> Option<ActiveHoldResolution> {
    if column >= active_holds.len() || song_time_ns_invalid(target_time_ns) {
        return None;
    }
    let music_rate = normalized_song_rate(music_rate);

    let advance = {
        let Some(active) = active_holds[column].as_mut() else {
            return None;
        };
        let note_index = active.note_index;
        let Some(note) = notes.get_mut(note_index) else {
            active_holds[column] = None;
            return None;
        };
        let Some(hold) = note.hold.as_mut() else {
            active_holds[column] = None;
            return None;
        };
        advance_active_hold_to_time(
            active,
            hold,
            timing,
            note.row_index,
            note.beat,
            target_time_ns,
            music_rate,
        )
    };

    if advance.clear_active {
        active_holds[column] = None;
    }
    advance.resolution
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveHoldColumnResolution {
    pub column: usize,
    pub resolution: ActiveHoldResolution,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActiveHoldColumnsUpdate {
    pub columns_scanned: usize,
    pub event_count: usize,
    pub stopped: bool,
}

pub fn update_active_hold_columns(
    active_holds: &mut [Option<ActiveHold>],
    notes: &mut [Note],
    inputs: &[bool; MAX_COLS],
    num_cols: usize,
    cols_per_player: usize,
    num_players: usize,
    timing_players: &[&TimingData; MAX_PLAYERS],
    target_time_ns: SongTimeNs,
    music_rate: f32,
    live_autoplay: bool,
    events: &mut [Option<ActiveHoldColumnResolution>],
) -> ActiveHoldColumnsUpdate {
    let columns = num_cols.min(MAX_COLS).min(active_holds.len());
    let mut event_count = 0usize;
    for column in 0..columns {
        if event_count >= events.len() {
            return ActiveHoldColumnsUpdate {
                columns_scanned: column,
                event_count,
                stopped: true,
            };
        }
        sync_active_hold_pressed_column(active_holds, column, live_autoplay, inputs[column]);
        let player = player_index_for_column(num_players, cols_per_player, column);
        let Some(resolution) = integrate_active_hold_column(
            active_holds,
            notes,
            column,
            timing_players[player],
            target_time_ns,
            music_rate,
        ) else {
            continue;
        };
        events[event_count] = Some(ActiveHoldColumnResolution { column, resolution });
        event_count += 1;
    }
    ActiveHoldColumnsUpdate {
        columns_scanned: columns,
        event_count,
        stopped: false,
    }
}

pub fn collect_due_autoplay_active_hold_resolutions(
    active_holds: &mut [Option<ActiveHold>],
    num_cols: usize,
    cutoff_time_ns: SongTimeNs,
    events: &mut [Option<ActiveHoldColumnResolution>],
) -> ActiveHoldColumnsUpdate {
    let columns = num_cols.min(MAX_COLS).min(active_holds.len());
    let mut event_count = 0usize;
    for column in 0..columns {
        if event_count >= events.len() {
            return ActiveHoldColumnsUpdate {
                columns_scanned: column,
                event_count,
                stopped: true,
            };
        }
        let Some(resolution) = active_holds[column]
            .as_ref()
            .and_then(|active| autoplay_due_active_hold_resolution(active, cutoff_time_ns))
        else {
            continue;
        };
        active_holds[column] = None;
        events[event_count] = Some(ActiveHoldColumnResolution { column, resolution });
        event_count += 1;
    }
    ActiveHoldColumnsUpdate {
        columns_scanned: columns,
        event_count,
        stopped: false,
    }
}

pub fn settle_replaced_active_hold_column(
    active_holds: &mut [Option<ActiveHold>],
    notes: &mut [Note],
    column: usize,
    next_note_index: usize,
    next_start_time_ns: SongTimeNs,
    timing: &TimingData,
    music_rate: f32,
) -> Option<ActiveHoldColumnResolution> {
    let active = active_holds.get(column).and_then(Option::as_ref)?;
    let settle_time_ns = replaced_active_hold_settle_time(
        active.note_index,
        active.end_time_ns,
        next_note_index,
        next_start_time_ns,
    )?;
    let resolution = integrate_active_hold_column(
        active_holds,
        notes,
        column,
        timing,
        settle_time_ns,
        music_rate,
    )?;
    Some(ActiveHoldColumnResolution { column, resolution })
}

pub fn start_active_hold_column(
    active_holds: &mut [Option<ActiveHold>],
    notes: &mut [Note],
    column: usize,
    note_index: usize,
    start_time_ns: SongTimeNs,
    end_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
) -> bool {
    let Some(active_slot) = active_holds.get_mut(column) else {
        return false;
    };
    let Some(note) = notes.get_mut(note_index) else {
        return false;
    };
    *active_slot = Some(started_active_hold_state(
        note.hold.as_mut(),
        note_index,
        note.note_type,
        start_time_ns,
        end_time_ns,
        current_time_ns,
    ));
    true
}

#[derive(Clone, Copy, Debug)]
pub struct TurnRng {
    state: u64,
}

pub fn turn_seed_for_song(song: &SongData) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(song.simfile_path.to_string_lossy().as_bytes());
    hasher.finish()
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
fn random_range_song_time_ns(rng: &mut TurnRng, min: SongTimeNs, max: SongTimeNs) -> SongTimeNs {
    if max <= min {
        return min;
    }
    let span = i128::from(max) - i128::from(min);
    let offset = (span as f64 * f64::from(rng.next_f32_unit())).floor() as i128;
    clamp_song_time_ns(i128::from(min) + offset)
}

#[inline(always)]
pub fn autoplay_random_offset_music_ns_for_window(
    rng: &mut TurnRng,
    timing_profile: TimingProfileNs,
    window: TimingWindow,
) -> SongTimeNs {
    let w0 = timing_profile.fa_plus_window_ns.unwrap_or(0);
    let (inner, outer) = match window {
        TimingWindow::W0 => (0, w0),
        TimingWindow::W1 => (w0, timing_profile.windows_ns[0]),
        TimingWindow::W2 => (timing_profile.windows_ns[0], timing_profile.windows_ns[1]),
        TimingWindow::W3 => (timing_profile.windows_ns[1], timing_profile.windows_ns[2]),
        TimingWindow::W4 => (timing_profile.windows_ns[2], timing_profile.windows_ns[3]),
        TimingWindow::W5 => (timing_profile.windows_ns[3], timing_profile.windows_ns[4]),
    };
    if outer <= 0 {
        return 0;
    }
    if inner <= 0 || inner >= outer {
        return random_range_song_time_ns(rng, -outer, outer);
    }
    if rng.next_u32() & 1 == 0 {
        random_range_song_time_ns(rng, -outer, -inner)
    } else {
        random_range_song_time_ns(rng, inner, outer)
    }
}

#[inline(always)]
pub fn autoplay_judgment_offset_music_ns(
    live_autoplay: bool,
    rng: &mut TurnRng,
    timing_profile: TimingProfileNs,
    window: TimingWindow,
    measured_offset_music_ns: SongTimeNs,
) -> SongTimeNs {
    if !live_autoplay {
        return measured_offset_music_ns;
    }
    autoplay_random_offset_music_ns_for_window(rng, timing_profile, window)
}

#[inline(always)]
pub const fn live_autoplay_enabled_from_flags(autoplay_enabled: bool, replay_mode: bool) -> bool {
    autoplay_enabled && !replay_mode
}

#[inline(always)]
pub const fn autoplay_blocks_scoring_from_flags(autoplay_enabled: bool, replay_mode: bool) -> bool {
    live_autoplay_enabled_from_flags(autoplay_enabled, replay_mode)
}

#[inline(always)]
pub const fn autoplay_cursor_for_enable(
    next_tap_miss_cursor: usize,
    note_range: (usize, usize),
) -> usize {
    let start = note_range.0;
    let end = note_range.1;
    if end < start {
        return start;
    }
    if next_tap_miss_cursor < start {
        start
    } else if next_tap_miss_cursor > end {
        end
    } else {
        next_tap_miss_cursor
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayAutoplayRuntimeState {
    rng: TurnRng,
    cursors: [usize; MAX_PLAYERS],
}

impl GameplayAutoplayRuntimeState {
    #[inline(always)]
    pub const fn from_rng_and_cursors(rng: TurnRng, cursors: [usize; MAX_PLAYERS]) -> Self {
        Self { rng, cursors }
    }

    #[inline(always)]
    pub fn new(seed: u64, cursors: [usize; MAX_PLAYERS]) -> Self {
        Self::from_rng_and_cursors(TurnRng::new(seed), cursors)
    }

    #[inline(always)]
    pub fn judgment_offset_music_ns(
        &mut self,
        live_autoplay: bool,
        timing_profile: TimingProfileNs,
        window: TimingWindow,
        measured_offset_music_ns: SongTimeNs,
    ) -> SongTimeNs {
        autoplay_judgment_offset_music_ns(
            live_autoplay,
            &mut self.rng,
            timing_profile,
            window,
            measured_offset_music_ns,
        )
    }

    #[inline(always)]
    pub fn cursor(self, player: usize) -> usize {
        self.cursors.get(player).copied().unwrap_or(0)
    }

    #[inline(always)]
    pub fn set_cursor(&mut self, player: usize, cursor: usize) {
        if let Some(slot) = self.cursors.get_mut(player) {
            *slot = cursor;
        }
    }

    #[inline(always)]
    pub fn set_cursor_for_enable(
        &mut self,
        player: usize,
        next_tap_miss_cursor: usize,
        note_range: (usize, usize),
    ) {
        self.set_cursor(
            player,
            autoplay_cursor_for_enable(next_tap_miss_cursor, note_range),
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoplayNoteAction {
    Tap,
    Lift,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoplayNoteEvent {
    pub note_index: usize,
    pub column: usize,
    pub action: AutoplayNoteAction,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AutoplayRowEventsUpdate {
    pub cursor: usize,
    pub row_time_ns: SongTimeNs,
    pub event_count: usize,
    pub row_ready: bool,
}

pub fn collect_next_autoplay_row_events(
    notes: &[Note],
    note_time_cache_ns: &[SongTimeNs],
    note_range: (usize, usize),
    cursor: usize,
    num_cols: usize,
    now_music_time_ns: SongTimeNs,
    events: &mut [Option<AutoplayNoteEvent>],
) -> AutoplayRowEventsUpdate {
    let note_start = note_range.0.min(notes.len()).min(note_time_cache_ns.len());
    let note_end = note_range
        .1
        .min(notes.len())
        .min(note_time_cache_ns.len())
        .max(note_start);
    let mut cursor = cursor.max(note_start).min(note_end);
    while cursor < note_end && notes[cursor].result.is_some() {
        cursor += 1;
    }
    if cursor >= note_end {
        return AutoplayRowEventsUpdate {
            cursor,
            ..AutoplayRowEventsUpdate::default()
        };
    }

    let row = notes[cursor].row_index;
    let mut row_end = cursor + 1;
    while row_end < note_end && notes[row_end].row_index == row {
        row_end += 1;
    }
    let row_time_ns = note_time_cache_ns[cursor];
    if row_time_ns > now_music_time_ns {
        return AutoplayRowEventsUpdate {
            cursor,
            row_time_ns,
            ..AutoplayRowEventsUpdate::default()
        };
    }

    let mut event_count = 0usize;
    for (offset, note) in notes[cursor..row_end].iter().enumerate() {
        let idx = cursor + offset;
        if note.result.is_some()
            || note.is_fake
            || !note.can_be_judged
            || note.column >= num_cols
            || event_count >= events.len()
        {
            continue;
        }
        let action = match note.note_type {
            NoteType::Lift => AutoplayNoteAction::Lift,
            NoteType::Tap | NoteType::Hold | NoteType::Roll => AutoplayNoteAction::Tap,
            NoteType::Mine | NoteType::Fake => continue,
        };
        events[event_count] = Some(AutoplayNoteEvent {
            note_index: idx,
            column: note.column,
            action,
        });
        event_count += 1;
    }

    AutoplayRowEventsUpdate {
        cursor: row_end,
        row_time_ns,
        event_count,
        row_ready: true,
    }
}

pub fn collect_active_autoplay_roll_columns(
    active_holds: &[Option<ActiveHold>],
    num_cols: usize,
    columns: &mut [usize],
) -> usize {
    let cols = num_cols.min(MAX_COLS).min(active_holds.len());
    let mut count = 0usize;
    for (column, active) in active_holds.iter().take(cols).enumerate() {
        if count >= columns.len() {
            break;
        }
        if active
            .as_ref()
            .is_some_and(|active| matches!(active.note_type, NoteType::Roll) && !active.let_go)
        {
            columns[count] = column;
            count += 1;
        }
    }
    count
}

#[inline(always)]
pub fn active_hold_is_engaged(active: &ActiveHold) -> bool {
    !active.let_go && active.life > 0.0
}

#[inline(always)]
pub fn autoplay_due_active_hold_resolution(
    active: &ActiveHold,
    cutoff_time_ns: SongTimeNs,
) -> Option<ActiveHoldResolution> {
    if active.end_time_ns > cutoff_time_ns {
        return None;
    }
    if active_hold_is_engaged(active) {
        Some(ActiveHoldResolution::Success {
            note_index: active.note_index,
        })
    } else {
        Some(ActiveHoldResolution::LetGo {
            note_index: active.note_index,
            time_ns: active.end_time_ns,
        })
    }
}

#[inline(always)]
pub fn hold_head_render_flags(
    active_state: Option<&ActiveHold>,
    current_beat: f32,
    note_beat: f32,
) -> (bool, bool) {
    let reached_receptor = current_beat >= note_beat;
    let engaged = reached_receptor && active_state.is_some_and(active_hold_is_engaged);
    let use_active = engaged
        && active_state.is_some_and(|h| matches!(h.note_type, NoteType::Roll) || h.is_pressed);
    (engaged, use_active)
}

#[inline(always)]
pub fn hold_explosion_active(
    active_state: Option<&ActiveHold>,
    current_beat: f32,
    note_beat: f32,
) -> bool {
    current_beat >= note_beat && active_state.is_some_and(active_hold_is_engaged)
}

#[inline(always)]
pub fn let_go_head_beat(
    note_beat: f32,
    end_beat: f32,
    last_held_beat: f32,
    visible_beat: f32,
) -> f32 {
    last_held_beat
        .clamp(note_beat, end_beat)
        .min(visible_beat.max(note_beat))
}

