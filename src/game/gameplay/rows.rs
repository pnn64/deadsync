use crate::game::judgment::{self, JudgeGrade};
use crate::game::timing;

use super::{
    FinalizedRowOutcome, RowEntry, SongTimeNs, State, active_hold_is_engaged,
    apply_autosync_for_row_hits, apply_life_change, apply_row_combo_state, autoplay_blocks_scoring,
    capture_failed_ex_score_inputs, current_music_time_s, display_judge_ix, error_bar_register_tap,
    is_player_dead, judge_life_delta, max_step_distance_ns, player_col_range,
    record_display_window_counts, set_last_judgment, update_itg_grade_totals,
};

#[inline(always)]
pub(super) const fn suppress_final_bad_rescore_visual(
    row_had_provisional_early_hit: bool,
    final_grade: JudgeGrade,
) -> bool {
    row_had_provisional_early_hit && matches!(final_grade, JudgeGrade::Decent | JudgeGrade::WayOff)
}

pub(super) fn finalize_row_judgment(
    state: &mut State,
    player: usize,
    row_index: usize,
    row_entry_index: usize,
    skip_life_change: bool,
) {
    let (col_start, col_end) = player_col_range(state, player);
    let mut row_has_miss = false;
    let mut row_has_wayoff = false;
    let mut player_row_note_count = 0u32;
    let row_notes = state.row_entries[row_entry_index].note_indices();
    let Some(final_judgment) =
        judgment::aggregate_row_final_judgment(row_notes.iter().filter_map(|&note_index| {
            let judgment = state.notes[note_index].result.as_ref()?;
            player_row_note_count = player_row_note_count.saturating_add(1);
            row_has_miss |= judgment.grade == JudgeGrade::Miss;
            row_has_wayoff |= judgment.grade == JudgeGrade::WayOff;
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
    record_display_window_counts(state, player, &final_judgment);
    timing::record_live_timing_stats(
        &mut state.players[player].live_timing_stats,
        &final_judgment,
    );
    let suppress_final_early_bad_visual =
        suppress_final_bad_rescore_visual(skip_life_change, final_grade);
    if scoring_blocked {
        if !suppress_final_early_bad_visual {
            set_last_judgment(state, player, final_judgment);
        }
        return;
    }
    let show_final_visual = !suppress_final_early_bad_visual;
    let current_music_time = current_music_time_s(state);
    {
        let p = &mut state.players[player];
        let grade_ix = display_judge_ix(final_grade);
        p.judgment_counts[grade_ix] = p.judgment_counts[grade_ix].saturating_add(1);
        if !is_player_dead(p) {
            p.scoring_counts[grade_ix] = p.scoring_counts[grade_ix].saturating_add(1);
            update_itg_grade_totals(p);
        }
        let life_delta = judge_life_delta(final_grade);
        if !skip_life_change {
            apply_life_change(p, current_music_time, life_delta);
        }
        apply_row_combo_state(p, final_grade, player_row_note_count, 1);
        if !row_has_miss && !row_has_wayoff {
            let notes_on_row_count = player_row_note_count as usize;
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
            if notes_on_row_count + carried_holds_down >= 3 {
                p.hands_achieved = p.hands_achieved.saturating_add(1);
            }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlayerRowScanState {
    BeyondLookahead,
    Pending,
    Ready {
        row_index: usize,
        skip_life_change: bool,
    },
    Finalized,
}

#[inline(always)]
pub(super) fn player_row_scan_state(
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
pub(super) fn next_ready_row_in_lookahead<F>(
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
pub(super) fn advance_judged_row_cursor<F>(
    cursor: usize,
    row_count: usize,
    mut row_state: F,
) -> usize
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
