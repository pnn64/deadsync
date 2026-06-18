use deadsync_rules::timing;

use super::{
    RowFinalizationPlayerState, State, advance_judged_row_cursor_for_entries,
    apply_autosync_for_row_hits, apply_combo_update, apply_life_change,
    apply_row_finalization_player_state, autoplay_blocks_scoring, capture_failed_ex_score_inputs,
    carried_holds_down_at_row, collect_ready_judged_row_events, current_music_time_s,
    error_bar_register_tap, is_player_dead, judged_row_lookahead_time_ns, player_col_range,
    player_combo_state, record_display_window_counts, row_finalization_plan_for_entry,
    set_last_judgment, update_itg_grade_totals, write_player_combo_state,
};

fn row_finalization_player_state(player: &super::PlayerRuntime) -> RowFinalizationPlayerState {
    RowFinalizationPlayerState {
        combo: player_combo_state(player),
        current_combo_window_counts: player.current_combo_window_counts,
        judgment_counts: player.judgment_counts,
        scoring_counts: player.scoring_counts,
        hands_achieved: player.hands_achieved,
    }
}

fn set_row_finalization_player_state(
    player: &mut super::PlayerRuntime,
    state: RowFinalizationPlayerState,
) {
    write_player_combo_state(player, state.combo);
    player.current_combo_window_counts = state.current_combo_window_counts;
    player.judgment_counts = state.judgment_counts;
    player.scoring_counts = state.scoring_counts;
    player.hands_achieved = state.hands_achieved;
}

pub(super) fn finalize_row_judgment(
    state: &mut State,
    player: usize,
    row_index: usize,
    row_entry_index: usize,
    skip_life_change: bool,
) {
    let (col_start, col_end) = player_col_range(state, player);
    let Some(plan) = row_finalization_plan_for_entry(
        &state.notes,
        &state.row_entries[row_entry_index],
        autoplay_blocks_scoring(state),
        skip_life_change,
    ) else {
        return;
    };
    apply_autosync_for_row_hits(state, row_entry_index);
    let final_judgment = plan.judgment;
    state.row_entries[row_entry_index].final_outcome = Some(plan.outcome);
    timing::record_live_timing_stats(
        &mut state.players[player].live_timing_stats,
        &final_judgment,
    );
    if plan.record_display_window_counts {
        record_display_window_counts(state, player, &final_judgment);
    }
    let current_music_time = current_music_time_s(state);
    if plan.apply_player_state {
        let p = &mut state.players[player];
        let player_dead = is_player_dead(p);
        let carried_holds_down = carried_holds_down_at_row(
            &state.notes,
            &state.active_holds,
            (col_start, col_end),
            row_index,
        );
        let mut row_state = row_finalization_player_state(p);
        let update = apply_row_finalization_player_state(
            &mut row_state,
            &final_judgment,
            plan.note_count,
            carried_holds_down,
            player_dead,
        );
        set_row_finalization_player_state(p, row_state);
        if update.update_grade_totals {
            update_itg_grade_totals(p);
        }
        if plan.apply_life_change {
            apply_life_change(p, current_music_time, plan.life_delta);
        }
        apply_combo_update(p, update.combo_update);
    }
    if plan.show_final_visual {
        // Arrow Cloud's gameplay HUD uses the row-final JudgmentMessage for
        // offset/error-bar visuals, not individual note hits inside a chord.
        set_last_judgment(state, player, final_judgment);
        error_bar_register_tap(state, player, &final_judgment, current_music_time_s(state));
    }
    if plan.capture_failed_ex_score_inputs {
        capture_failed_ex_score_inputs(state, player);
    }
}

pub(super) fn update_judged_rows(state: &mut State) {
    let lookahead_time_ns = judged_row_lookahead_time_ns(
        state.current_music_time_ns,
        &state.timing_profile,
        state.music_rate,
    );
    for player in 0..state.num_players {
        let (row_start, row_end) = state.row_entry_ranges[player];
        let row_count = row_end;
        let mut scan_start = state.judged_row_cursor[player].max(row_start);
        let mut events = [None; 8];
        loop {
            let update = collect_ready_judged_row_events(
                &state.row_entries,
                (row_start, row_end),
                scan_start,
                lookahead_time_ns,
                &mut events,
            );
            scan_start = update.next_scan_start;
            for event in events.iter().take(update.event_count).flatten() {
                finalize_row_judgment(
                    state,
                    player,
                    event.row_index,
                    event.row_entry_index,
                    event.skip_life_change,
                );
            }
            if !update.stopped || update.event_count == 0 {
                break;
            }
        }
        state.judged_row_cursor[player] = advance_judged_row_cursor_for_entries(
            &state.row_entries,
            (row_start, row_count),
            state.judged_row_cursor[player],
            lookahead_time_ns,
        );
    }
}
