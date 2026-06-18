use super::{
    ActiveHoldResolution, HoldResolutionPlayerState, HoldResultStatsState, MAX_COLS, MAX_PLAYERS,
    PlayerRuntime, SongTimeNs, State, apply_combo_update, apply_hold_let_go_player_state,
    apply_hold_let_go_update, apply_hold_success_player_state, apply_hold_success_update,
    apply_life_change, autoplay_blocks_scoring, capture_failed_ex_score_inputs,
    current_music_time_s, hold_judgment_render_info, hold_resolution_updates_grade_totals,
    integrate_active_hold_column, is_state_dead, live_autoplay_enabled, player_combo_state,
    player_for_col, refresh_roll_life_for_active_column, settle_replaced_active_hold_column,
    song_time_ns_invalid, start_active_hold_column, trigger_hold_explosion,
    update_active_hold_columns, update_itg_grade_totals, write_player_combo_state,
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

fn hold_resolution_player_state(player: &PlayerRuntime) -> HoldResolutionPlayerState {
    HoldResolutionPlayerState {
        stats: hold_result_stats_state(player),
        combo: player_combo_state(player),
    }
}

fn set_hold_resolution_player_state(player: &mut PlayerRuntime, state: HoldResolutionPlayerState) {
    set_hold_result_stats_state(player, state.stats);
    write_player_combo_state(player, state.combo);
}

fn apply_hold_resolution_player_state(
    player: &mut PlayerRuntime,
    state: HoldResolutionPlayerState,
) {
    set_hold_resolution_player_state(player, state);
}

pub(super) fn handle_hold_let_go(
    state: &mut State,
    column: usize,
    note_index: usize,
    let_go_time_ns: SongTimeNs,
) {
    let player = player_for_col(state, column);
    let scoring_blocked = autoplay_blocks_scoring(state);
    let note_type = state.notes[note_index].note_type;
    let player_dead = is_state_dead(state, player);
    let Some(update) = apply_hold_let_go_update(
        state.notes[note_index].hold.as_mut(),
        &mut state.hold_decay_active,
        &mut state.decaying_hold_indices,
        note_index,
        note_type,
        let_go_time_ns,
        scoring_blocked,
        player_dead,
    ) else {
        return;
    };
    let mut player_state = hold_resolution_player_state(&state.players[player]);
    let player_update =
        apply_hold_let_go_player_state(&mut player_state, update.stats_update, scoring_blocked);
    apply_hold_resolution_player_state(&mut state.players[player], player_state);
    if update.effects.show_judgment {
        state.hold_judgments[column] = Some(hold_judgment_render_info(
            update.result,
            state.total_elapsed_in_screen,
        ));
    }
    if player_update.apply_life_change {
        let current_music_time = current_music_time_s(state);
        apply_life_change(
            &mut state.players[player],
            current_music_time,
            player_update.life_delta,
        );
    }
    if player_update.capture_failed_ex_score_inputs {
        capture_failed_ex_score_inputs(state, player);
    }
    if hold_resolution_updates_grade_totals(
        update.result,
        player_update.stats_update,
        is_state_dead(state, player),
    ) {
        update_itg_grade_totals(&mut state.players[player]);
    }
    apply_combo_update(&mut state.players[player], player_update.combo_update);
    if update.effects.reset_receptor_glow {
        state.receptor_glow_timers[column] = 0.0;
    }
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
    if column >= state.num_cols {
        return;
    }
    let player = player_for_col(state, column);
    // A fast same-column hold jack can hit the next head early while the
    // previous hold is still alive. ITG stores hold state per TapNote; settle
    // the previous non-overlapping hold before replacing this column slot.
    if let Some(event) = settle_replaced_active_hold_column(
        &mut state.active_holds,
        &mut state.notes,
        column,
        note_index,
        start_time_ns,
        &state.timing_players[player],
        state.music_rate,
    ) {
        resolve_active_hold(state, event.column, event.resolution);
    }
    start_active_hold_column(
        &mut state.active_holds,
        &mut state.notes,
        column,
        note_index,
        start_time_ns,
        end_time_ns,
        current_time_ns,
    );
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
    if let Some(resolution) = integrate_active_hold_column(
        &mut state.active_holds,
        &mut state.notes,
        column,
        &state.timing_players[player],
        target_time_ns,
        state.music_rate,
    ) {
        resolve_active_hold(state, column, resolution);
    }
}

pub(super) fn handle_hold_success(state: &mut State, column: usize, note_index: usize) {
    let player = player_for_col(state, column);
    let scoring_blocked = autoplay_blocks_scoring(state);
    let note_type = state.notes[note_index].note_type;
    let player_dead = is_state_dead(state, player);
    let Some(update) = apply_hold_success_update(
        state.notes[note_index].hold.as_mut(),
        &mut state.hold_decay_active,
        note_index,
        note_type,
        scoring_blocked,
        player_dead,
    ) else {
        return;
    };
    let mut player_state = hold_resolution_player_state(&state.players[player]);
    let player_update =
        apply_hold_success_player_state(&mut player_state, update.stats_update, scoring_blocked);
    apply_hold_resolution_player_state(&mut state.players[player], player_state);
    if player_update.apply_life_change {
        let current_music_time = current_music_time_s(state);
        apply_life_change(
            &mut state.players[player],
            current_music_time,
            player_update.life_delta,
        );
    }
    if player_update.capture_failed_ex_score_inputs {
        capture_failed_ex_score_inputs(state, player);
    }
    if hold_resolution_updates_grade_totals(
        update.result,
        player_update.stats_update,
        is_state_dead(state, player),
    ) {
        update_itg_grade_totals(&mut state.players[player]);
    }
    apply_combo_update(&mut state.players[player], player_update.combo_update);
    if update.effects.trigger_hold_explosion {
        trigger_hold_explosion(state, column);
    }
    if update.effects.show_judgment {
        state.hold_judgments[column] = Some(hold_judgment_render_info(
            update.result,
            state.total_elapsed_in_screen,
        ));
    }
}

pub(super) fn refresh_roll_life_on_step(
    state: &mut State,
    column: usize,
    event_time_ns: SongTimeNs,
) {
    refresh_roll_life_for_active_column(
        &mut state.active_holds,
        &mut state.notes,
        column,
        event_time_ns,
    );
}

pub(super) fn update_active_holds(
    state: &mut State,
    inputs: &[bool; MAX_COLS],
    current_time_ns: SongTimeNs,
) {
    let timing_players: [&_; MAX_PLAYERS] =
        std::array::from_fn(|player| state.timing_players[player].as_ref());
    let live_autoplay = live_autoplay_enabled(state);
    let mut events = [None; MAX_COLS];
    let update = update_active_hold_columns(
        &mut state.active_holds,
        &mut state.notes,
        inputs,
        state.num_cols,
        state.cols_per_player,
        state.num_players,
        &timing_players,
        current_time_ns,
        state.music_rate,
        live_autoplay,
        &mut events,
    );
    for event in events.iter().take(update.event_count).flatten() {
        resolve_active_hold(state, event.column, event.resolution);
    }
}
