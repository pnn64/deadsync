use deadsync_rules::note::HoldResult;

use super::{
    ActiveHoldAdvance, ActiveHoldResolution, HoldJudgmentRenderInfo, HoldResultStatsState,
    HoldResultStatsUpdate, LIFE_HELD, LIFE_LET_GO, MAX_COLS, PlayerRuntime, SongTimeNs, State,
    advance_active_hold_to_time, apply_hold_let_go_combo_state, apply_hold_let_go_result,
    apply_hold_result_stats_update, apply_hold_success_combo_state, apply_hold_success_result,
    apply_life_change, autoplay_blocks_scoring, capture_failed_ex_score_inputs,
    current_music_time_s, hold_result_stats_update, is_state_dead, player_for_col,
    refresh_roll_life_for_step, replaced_active_hold_settle_time, song_time_ns_invalid,
    started_active_hold_state, sync_active_hold_pressed_state, trigger_hold_explosion,
    update_itg_grade_totals,
};

fn hold_result_stats_state(player: &PlayerRuntime) -> HoldResultStatsState {
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

fn set_hold_result_stats_state(player: &mut PlayerRuntime, stats: HoldResultStatsState) {
    player.hands_holding_count_for_stats = stats.hands_holding_count_for_stats;
    player.holds_held = stats.holds_held;
    player.holds_held_for_score = stats.holds_held_for_score;
    player.holds_let_go_for_score = stats.holds_let_go_for_score;
    player.rolls_held = stats.rolls_held;
    player.rolls_held_for_score = stats.rolls_held_for_score;
    player.rolls_let_go_for_score = stats.rolls_let_go_for_score;
}

fn apply_hold_result_stats_to_player(player: &mut PlayerRuntime, update: HoldResultStatsUpdate) {
    let mut stats = hold_result_stats_state(player);
    apply_hold_result_stats_update(&mut stats, update);
    set_hold_result_stats_state(player, stats);
}

pub(super) fn handle_hold_let_go(
    state: &mut State,
    column: usize,
    note_index: usize,
    let_go_time_ns: SongTimeNs,
) {
    let player = player_for_col(state, column);
    let scoring_blocked = autoplay_blocks_scoring(state);
    if !apply_hold_let_go_result(
        state.notes[note_index].hold.as_mut(),
        &mut state.hold_decay_active,
        &mut state.decaying_hold_indices,
        note_index,
        let_go_time_ns,
    ) {
        return;
    }
    let stats_update = hold_result_stats_update(
        state.notes[note_index].note_type,
        HoldResult::LetGo,
        scoring_blocked,
        is_state_dead(state, player),
    );
    apply_hold_result_stats_to_player(&mut state.players[player], stats_update);
    state.hold_judgments[column] = Some(HoldJudgmentRenderInfo {
        result: HoldResult::LetGo,
        started_at_screen_s: state.total_elapsed_in_screen,
    });
    if !scoring_blocked {
        let current_music_time = current_music_time_s(state);
        apply_life_change(&mut state.players[player], current_music_time, LIFE_LET_GO);
        capture_failed_ex_score_inputs(state, player);
    }
    if stats_update.update_grade_totals && !is_state_dead(state, player) {
        update_itg_grade_totals(&mut state.players[player]);
    }
    if !scoring_blocked {
        apply_hold_let_go_combo_state(&mut state.players[player]);
    }
    state.receptor_glow_timers[column] = 0.0;
}

#[inline(always)]
fn resolve_active_hold(state: &mut State, column: usize, resolution: ActiveHoldResolution) {
    match resolution {
        ActiveHoldResolution::LetGo {
            note_index,
            time_ns,
        } => handle_hold_let_go(state, column, note_index, time_ns),
        ActiveHoldResolution::Success { note_index } => {
            handle_hold_success(state, column, note_index)
        }
    }
}

#[inline(always)]
pub(super) fn start_active_hold(
    state: &mut State,
    column: usize,
    note_index: usize,
    start_time_ns: SongTimeNs,
    end_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
) {
    settle_replaced_active_hold(state, column, note_index, start_time_ns);
    let note_type = state.notes[note_index].note_type;
    state.active_holds[column] = Some(started_active_hold_state(
        state.notes[note_index].hold.as_mut(),
        note_index,
        note_type,
        start_time_ns,
        end_time_ns,
        current_time_ns,
    ));
}

#[inline(always)]
fn settle_replaced_active_hold(
    state: &mut State,
    column: usize,
    next_note_index: usize,
    next_start_time_ns: SongTimeNs,
) {
    let Some(active) = state.active_holds[column].as_ref() else {
        return;
    };
    let Some(settle_time_ns) = replaced_active_hold_settle_time(
        active.note_index,
        active.end_time_ns,
        next_note_index,
        next_start_time_ns,
    ) else {
        return;
    };
    // A fast same-column hold jack can hit the next head early while the
    // previous hold is still alive. ITG stores hold state per TapNote; settle
    // the previous non-overlapping hold before replacing this column slot.
    integrate_active_hold_to_time(state, column, settle_time_ns);
}

#[inline(always)]
pub(super) fn integrate_active_hold_to_time(
    state: &mut State,
    column: usize,
    target_time_ns: SongTimeNs,
) {
    if column >= state.num_cols || song_time_ns_invalid(target_time_ns) {
        return;
    }

    let player = player_for_col(state, column);
    let timing = state.timing_players[player].clone();
    let music_rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };

    let advance = {
        let (active_holds, notes) = (&mut state.active_holds, &mut state.notes);
        let Some(active) = active_holds[column].as_mut() else {
            return;
        };
        let note_index = active.note_index;
        if let Some(note) = notes.get_mut(note_index) {
            if let Some(hold) = note.hold.as_mut() {
                advance_active_hold_to_time(
                    active,
                    hold,
                    &timing,
                    note.row_index,
                    note.beat,
                    target_time_ns,
                    music_rate,
                )
            } else {
                ActiveHoldAdvance {
                    clear_active: true,
                    resolution: None,
                }
            }
        } else {
            ActiveHoldAdvance {
                clear_active: true,
                resolution: None,
            }
        }
    };

    if advance.clear_active {
        state.active_holds[column] = None;
    }
    if let Some(resolution) = advance.resolution {
        resolve_active_hold(state, column, resolution);
    }
}

pub(super) fn handle_hold_success(state: &mut State, column: usize, note_index: usize) {
    let player = player_for_col(state, column);
    let scoring_blocked = autoplay_blocks_scoring(state);
    if !apply_hold_success_result(
        state.notes[note_index].hold.as_mut(),
        &mut state.hold_decay_active,
        note_index,
    ) {
        return;
    }
    let stats_update = hold_result_stats_update(
        state.notes[note_index].note_type,
        HoldResult::Held,
        scoring_blocked,
        is_state_dead(state, player),
    );
    apply_hold_result_stats_to_player(&mut state.players[player], stats_update);
    if !scoring_blocked {
        let current_music_time = current_music_time_s(state);
        apply_life_change(&mut state.players[player], current_music_time, LIFE_HELD);
        capture_failed_ex_score_inputs(state, player);
    }
    if stats_update.update_grade_totals {
        update_itg_grade_totals(&mut state.players[player]);
    }
    if !scoring_blocked {
        apply_hold_success_combo_state(&mut state.players[player]);
    }
    trigger_hold_explosion(state, column);
    state.hold_judgments[column] = Some(HoldJudgmentRenderInfo {
        result: HoldResult::Held,
        started_at_screen_s: state.total_elapsed_in_screen,
    });
}

pub(super) fn refresh_roll_life_on_step(
    state: &mut State,
    column: usize,
    event_time_ns: SongTimeNs,
) {
    let Some(active) = state.active_holds[column].as_mut() else {
        return;
    };
    let Some(note) = state.notes.get_mut(active.note_index) else {
        return;
    };
    let Some(hold) = note.hold.as_mut() else {
        return;
    };
    refresh_roll_life_for_step(active, hold, event_time_ns);
}

pub(super) fn update_active_holds(
    state: &mut State,
    inputs: &[bool; MAX_COLS],
    current_time_ns: SongTimeNs,
) {
    for (column, lane_pressed) in inputs.iter().copied().enumerate().take(state.num_cols) {
        sync_active_hold_pressed_state(state, column, lane_pressed);
        integrate_active_hold_to_time(state, column, current_time_ns);
    }
}
