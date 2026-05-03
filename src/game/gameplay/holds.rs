use crate::game::note::{HoldData, HoldResult, NoteType};
use crate::game::timing::TimingData;

use super::{
    ActiveHold, COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO, HoldJudgmentRenderInfo, LIFE_HELD,
    LIFE_LET_GO, MAX_COLS, MAX_HOLD_LIFE, SongTimeNs, State, apply_hold_success_combo_state,
    apply_life_change, autoplay_blocks_scoring, break_combo_state, capture_failed_ex_score_inputs,
    clear_full_combo_state, current_music_time_s, is_state_dead, player_for_col,
    song_time_ns_delta_seconds, song_time_ns_from_seconds, song_time_ns_invalid,
    song_time_ns_to_seconds, sync_active_hold_pressed_state, trigger_hold_explosion,
    update_itg_grade_totals,
};

pub(super) fn handle_hold_let_go(
    state: &mut State,
    column: usize,
    note_index: usize,
    let_go_time_ns: SongTimeNs,
) {
    let player = player_for_col(state, column);
    let scoring_blocked = autoplay_blocks_scoring(state);
    let mut updated_possible_scoring = false;
    if let Some(hold) = state.notes[note_index].hold.as_mut() {
        if hold.result == Some(HoldResult::LetGo) {
            return;
        }
        hold.result = Some(HoldResult::LetGo);
        begin_hold_life_decay(
            hold,
            &mut state.hold_decay_active,
            &mut state.decaying_hold_indices,
            note_index,
            let_go_time_ns,
        );
    }
    if !scoring_blocked && !is_state_dead(state, player) {
        match state.notes[note_index].note_type {
            NoteType::Hold => {
                state.players[player].holds_let_go_for_score = state.players[player]
                    .holds_let_go_for_score
                    .saturating_add(1);
                updated_possible_scoring = true;
            }
            NoteType::Roll => {
                state.players[player].rolls_let_go_for_score = state.players[player]
                    .rolls_let_go_for_score
                    .saturating_add(1);
                updated_possible_scoring = true;
            }
            _ => {}
        }
    }
    if state.players[player].hands_holding_count_for_stats > 0 {
        state.players[player].hands_holding_count_for_stats -= 1;
    }
    state.hold_judgments[column] = Some(HoldJudgmentRenderInfo {
        result: HoldResult::LetGo,
        started_at_screen_s: state.total_elapsed_in_screen,
    });
    if !scoring_blocked {
        let current_music_time = current_music_time_s(state);
        apply_life_change(&mut state.players[player], current_music_time, LIFE_LET_GO);
        capture_failed_ex_score_inputs(state, player);
    }
    if updated_possible_scoring && !is_state_dead(state, player) {
        update_itg_grade_totals(&mut state.players[player]);
    }
    if !scoring_blocked {
        if COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO {
            break_combo_state(&mut state.players[player], 1);
        } else {
            clear_full_combo_state(&mut state.players[player]);
        }
    }
    state.receptor_glow_timers[column] = 0.0;
}

#[inline(always)]
pub(super) fn begin_hold_life_decay(
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

#[inline(always)]
const fn hold_window_seconds(note_type: NoteType) -> f32 {
    match note_type {
        NoteType::Roll => super::TIMING_WINDOW_SECONDS_ROLL,
        _ => super::TIMING_WINDOW_SECONDS_HOLD,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct HoldLifeAdvance {
    pub life_after: f32,
    pub zero_elapsed_music_ns: Option<SongTimeNs>,
}

#[inline(always)]
pub(super) fn advance_hold_life_ns(
    note_type: NoteType,
    life: f32,
    pressed: bool,
    music_elapsed_ns: SongTimeNs,
    music_rate: f32,
) -> HoldLifeAdvance {
    let life = life.clamp(0.0, MAX_HOLD_LIFE);
    if music_elapsed_ns <= 0 {
        return HoldLifeAdvance {
            life_after: life,
            zero_elapsed_music_ns: None,
        };
    }
    if matches!(note_type, NoteType::Hold) && pressed {
        return HoldLifeAdvance {
            life_after: MAX_HOLD_LIFE,
            zero_elapsed_music_ns: None,
        };
    }

    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    let window = hold_window_seconds(note_type);
    if !window.is_finite() || window <= 0.0 {
        return HoldLifeAdvance {
            life_after: 0.0,
            zero_elapsed_music_ns: Some(0),
        };
    }

    let music_elapsed_s = song_time_ns_delta_seconds(music_elapsed_ns, 0);
    let real_elapsed_s = music_elapsed_s / rate;
    let life_drop = real_elapsed_s / window;
    if life_drop < life {
        return HoldLifeAdvance {
            life_after: (life - life_drop).max(0.0),
            zero_elapsed_music_ns: None,
        };
    }

    HoldLifeAdvance {
        life_after: 0.0,
        zero_elapsed_music_ns: Some(song_time_ns_from_seconds(
            (life * window * rate).clamp(0.0, music_elapsed_s),
        )),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ActiveHoldResolution {
    LetGo {
        note_index: usize,
        time_ns: SongTimeNs,
    },
    Success {
        note_index: usize,
    },
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
    if let Some(hold) = state.notes[note_index].hold.as_mut() {
        hold.life = MAX_HOLD_LIFE;
        hold.let_go_started_at = None;
        hold.let_go_starting_life = 0.0;
    }
    state.active_holds[column] = Some(ActiveHold {
        note_index,
        start_time_ns,
        end_time_ns,
        note_type: state.notes[note_index].note_type,
        let_go: false,
        is_pressed: true,
        life: MAX_HOLD_LIFE,
        last_update_time_ns: current_time_ns,
    });
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

    let mut resolution = None;
    let clear_active = {
        let (active_holds, notes) = (&mut state.active_holds, &mut state.notes);
        let Some(active) = active_holds[column].as_mut() else {
            return;
        };
        let note_index = active.note_index;
        if let Some(note) = notes.get_mut(note_index) {
            if let Some(hold) = note.hold.as_mut() {
                let from_time_ns = active.last_update_time_ns;
                let final_time_ns = target_time_ns.max(from_time_ns).min(active.end_time_ns);
                let note_start_row = note.row_index;
                let note_start_beat = note.beat;

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
                        let progress_time_ns = match advance.zero_elapsed_music_ns {
                            Some(zero_elapsed_music_ns) => {
                                body_from_ns.saturating_add(zero_elapsed_music_ns)
                            }
                            None => body_to_ns,
                        };
                        let progress_time = song_time_ns_to_seconds(progress_time_ns);
                        if progress_time_ns > body_from_ns && progress_time.is_finite() {
                            let current_beat = timing.get_beat_for_time(progress_time);
                            advance_hold_last_held(
                                hold,
                                &timing,
                                current_beat,
                                note_start_row,
                                note_start_beat,
                            );
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
                resolution.is_some() || active.let_go
            } else {
                true
            }
        } else {
            true
        }
    };

    if clear_active {
        state.active_holds[column] = None;
    }
    if let Some(resolution) = resolution {
        resolve_active_hold(state, column, resolution);
    }
}

pub(super) fn handle_hold_success(state: &mut State, column: usize, note_index: usize) {
    let player = player_for_col(state, column);
    let scoring_blocked = autoplay_blocks_scoring(state);
    if let Some(hold) = state.notes[note_index].hold.as_mut() {
        if hold.result == Some(HoldResult::Held) {
            return;
        }
        hold.result = Some(HoldResult::Held);
        hold.life = MAX_HOLD_LIFE;
        hold.let_go_started_at = None;
        hold.let_go_starting_life = 0.0;
        hold.last_held_row_index = hold.end_row_index;
        hold.last_held_beat = hold.end_beat;
    }
    if note_index < state.hold_decay_active.len() && state.hold_decay_active[note_index] {
        state.hold_decay_active[note_index] = false;
    }
    if state.players[player].hands_holding_count_for_stats > 0 {
        state.players[player].hands_holding_count_for_stats -= 1;
    }
    let mut updated_scoring = false;
    match state.notes[note_index].note_type {
        NoteType::Hold => {
            if !scoring_blocked {
                state.players[player].holds_held =
                    state.players[player].holds_held.saturating_add(1);
            }
            if !scoring_blocked && !is_state_dead(state, player) {
                state.players[player].holds_held_for_score =
                    state.players[player].holds_held_for_score.saturating_add(1);
                updated_scoring = true;
            }
        }
        NoteType::Roll => {
            if !scoring_blocked {
                state.players[player].rolls_held =
                    state.players[player].rolls_held.saturating_add(1);
            }
            if !scoring_blocked && !is_state_dead(state, player) {
                state.players[player].rolls_held_for_score =
                    state.players[player].rolls_held_for_score.saturating_add(1);
                updated_scoring = true;
            }
        }
        _ => {}
    }
    if !scoring_blocked {
        let current_music_time = current_music_time_s(state);
        apply_life_change(&mut state.players[player], current_music_time, LIFE_HELD);
        capture_failed_ex_score_inputs(state, player);
    }
    if updated_scoring {
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
    if !matches!(active.note_type, NoteType::Roll)
        || active.let_go
        || active.life <= 0.0
        || song_time_ns_invalid(event_time_ns)
        || event_time_ns < active.start_time_ns
    {
        return;
    }
    let Some(note) = state.notes.get_mut(active.note_index) else {
        return;
    };
    let Some(hold) = note.hold.as_mut() else {
        return;
    };
    if matches!(hold.result, Some(HoldResult::LetGo | HoldResult::Missed)) {
        return;
    }
    active.life = MAX_HOLD_LIFE;
    active.last_update_time_ns = active
        .last_update_time_ns
        .max(event_time_ns.min(active.end_time_ns));
    hold.life = MAX_HOLD_LIFE;
    hold.let_go_started_at = None;
    hold.let_go_starting_life = 0.0;
}

#[inline(always)]
pub(super) fn advance_hold_last_held(
    hold: &mut HoldData,
    timing: &TimingData,
    current_beat: f32,
    note_start_row: usize,
    note_start_beat: f32,
) {
    let prev_row = hold.last_held_row_index;
    let prev_beat = hold.last_held_beat.clamp(note_start_beat, hold.end_beat);
    let current_beat = current_beat.clamp(note_start_beat, hold.end_beat);
    let mut current_row = timing
        .get_row_for_beat(current_beat)
        .unwrap_or(note_start_row);
    current_row = current_row.clamp(note_start_row, hold.end_row_index);
    let final_row = prev_row.max(current_row);
    if final_row == prev_row {
        hold.last_held_beat = prev_beat.max(current_beat);
        return;
    }
    hold.last_held_row_index = final_row;
    // Keep the row bookkeeping snapped like ITG, but preserve the exact beat for
    // rendering so a let-go head doesn't visibly jump to a neighboring row.
    hold.last_held_beat = prev_beat.max(current_beat);
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
