use deadsync_rules::judgment;
use deadsync_rules::timing;

use super::{
    FinalizedRowOutcome, State, active_hold_is_engaged, advance_judged_row_cursor,
    apply_autosync_for_row_hits, apply_life_change, apply_row_combo_state, autoplay_blocks_scoring,
    capture_failed_ex_score_inputs, current_music_time_s, error_bar_register_tap,
    finalized_row_awards_hand, is_player_dead, judge_life_delta, max_step_distance_ns,
    next_ready_row_in_lookahead, player_col_range, player_row_scan_state,
    record_current_combo_window_count, record_display_window_counts, set_last_judgment,
    suppress_final_bad_rescore_visual, update_itg_grade_totals,
};

pub(super) fn finalize_row_judgment(
    state: &mut State,
    player: usize,
    row_index: usize,
    row_entry_index: usize,
    skip_life_change: bool,
) {
    let (col_start, col_end) = player_col_range(state, player);
    let mut player_row_note_count = 0u32;
    let row_notes = state.row_entries[row_entry_index].note_indices();
    let Some(final_judgment) =
        judgment::aggregate_row_final_judgment(row_notes.iter().filter_map(|&note_index| {
            let judgment = state.notes[note_index].result.as_ref()?;
            player_row_note_count = player_row_note_count.saturating_add(1);
            Some(judgment)
        }))
        .copied()
    else {
        return;
    };
    let scoring_blocked = autoplay_blocks_scoring(state);
    apply_autosync_for_row_hits(state, row_entry_index);
    let final_grade = final_judgment.grade;
    state.row_entries[row_entry_index].final_outcome = Some(FinalizedRowOutcome { final_grade });
    timing::record_live_timing_stats(
        &mut state.players[player].live_timing_stats,
        &final_judgment,
    );
    let suppress_final_early_bad_visual =
        suppress_final_bad_rescore_visual(skip_life_change, final_grade);
    if scoring_blocked {
        if !suppress_final_early_bad_visual {
            set_last_judgment(state, player, final_judgment);
            error_bar_register_tap(state, player, &final_judgment, current_music_time_s(state));
        }
        return;
    }
    record_display_window_counts(state, player, &final_judgment);
    let show_final_visual = !suppress_final_early_bad_visual;
    let current_music_time = current_music_time_s(state);
    {
        let p = &mut state.players[player];
        let grade_ix = judgment::display_judge_ix(final_grade);
        p.judgment_counts[grade_ix] = p.judgment_counts[grade_ix].saturating_add(1);
        if !is_player_dead(p) {
            p.scoring_counts[grade_ix] = p.scoring_counts[grade_ix].saturating_add(1);
            update_itg_grade_totals(p);
        }
        let life_delta = judge_life_delta(final_grade);
        if !skip_life_change {
            apply_life_change(p, current_music_time, life_delta);
        }
        record_current_combo_window_count(p, &final_judgment);
        apply_row_combo_state(p, final_grade, player_row_note_count, 1);
        let carried_holds_down: usize = state.active_holds[col_start..col_end]
            .iter()
            .filter_map(|a| a.as_ref())
            .filter(|a| active_hold_is_engaged(a))
            .filter(|a| {
                let note = &state.notes[a.note_index];
                if note.row_index >= row_index {
                    return false;
                }
                if let Some(h) = note.hold.as_ref() {
                    h.last_held_row_index >= row_index
                } else {
                    false
                }
            })
            .count();
        if finalized_row_awards_hand(final_grade, player_row_note_count, carried_holds_down) {
            p.hands_achieved = p.hands_achieved.saturating_add(1);
        }
    }
    if show_final_visual {
        // Arrow Cloud's gameplay HUD uses the row-final JudgmentMessage for
        // offset/error-bar visuals, not individual note hits inside a chord.
        set_last_judgment(state, player, final_judgment);
        error_bar_register_tap(state, player, &final_judgment, current_music_time_s(state));
    }
    if !skip_life_change {
        capture_failed_ex_score_inputs(state, player);
    }
}

pub(super) fn update_judged_rows(state: &mut State) {
    let lookahead_time_ns = state
        .current_music_time_ns
        .saturating_add(max_step_distance_ns(
            &state.timing_profile,
            state.music_rate,
        ));
    for player in 0..state.num_players {
        let (row_start, row_end) = state.row_entry_ranges[player];
        let row_count = row_end;
        let mut scan_start = state.judged_row_cursor[player].max(row_start);
        while let Some((row_entry_index, row_index, skip_life_change)) =
            next_ready_row_in_lookahead(scan_start, row_count, |idx| {
                player_row_scan_state(&state.row_entries, idx, lookahead_time_ns)
            })
        {
            finalize_row_judgment(state, player, row_index, row_entry_index, skip_life_change);
            scan_start = row_entry_index + 1;
        }
        state.judged_row_cursor[player] = advance_judged_row_cursor(
            state.judged_row_cursor[player].max(row_start),
            row_count,
            |idx| player_row_scan_state(&state.row_entries, idx, lookahead_time_ns),
        );
    }
}
