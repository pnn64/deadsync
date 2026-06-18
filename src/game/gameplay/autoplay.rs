use deadsync_input::{INPUT_SLOT_INVALID, lane_from_column};

use super::input::push_input_edge;
use super::{
    ActiveHoldResolution, AutoplayNoteAction, MAX_COLS, SongTimeNs, State,
    autoplay_blocks_scoring_from_flags, collect_active_autoplay_roll_columns,
    collect_due_autoplay_active_hold_resolutions, collect_next_autoplay_row_events,
    collect_ready_replay_edges, handle_hold_let_go, handle_hold_success, judge_a_lift, judge_a_tap,
    live_autoplay_enabled_from_flags, player_note_range, refresh_roll_life_on_step,
};

#[inline(always)]
pub(super) fn autoplay_blocks_scoring(state: &State) -> bool {
    autoplay_blocks_scoring_from_flags(state.autoplay_enabled, state.replay_mode)
}

#[inline(always)]
pub(super) fn live_autoplay_enabled(state: &State) -> bool {
    live_autoplay_enabled_from_flags(state.autoplay_enabled, state.replay_mode)
}

#[inline(always)]
fn settle_due_autoplay_active_holds(state: &mut State, cutoff_time_ns: SongTimeNs) {
    let mut events = [None; MAX_COLS];
    let update = collect_due_autoplay_active_hold_resolutions(
        &mut state.active_holds,
        state.num_cols,
        cutoff_time_ns,
        &mut events,
    );
    for event in events.iter().take(update.event_count).flatten() {
        match event.resolution {
            ActiveHoldResolution::Success { note_index } => {
                handle_hold_success(state, event.column, note_index);
            }
            ActiveHoldResolution::LetGo {
                note_index,
                time_ns,
            } => handle_hold_let_go(state, event.column, note_index, time_ns),
        }
    }
}

pub(super) fn run_autoplay(state: &mut State, now_music_time_ns: SongTimeNs) {
    if !state.autoplay_enabled {
        return;
    }

    for player in 0..state.num_players {
        let note_range = player_note_range(state, player);
        let mut cursor = state.autoplay_cursor[player].max(note_range.0);
        loop {
            let mut events = [None; MAX_COLS];
            let update = collect_next_autoplay_row_events(
                &state.notes,
                &state.note_time_cache_ns,
                note_range,
                cursor,
                state.num_cols,
                now_music_time_ns,
                &mut events,
            );
            cursor = update.cursor;
            if !update.row_ready {
                break;
            }
            // Finalize any already-ended autoplay holds before a new warped
            // row on the same lane can replace the active hold slot.
            settle_due_autoplay_active_holds(state, update.row_time_ns);
            for event in events.iter().take(update.event_count).flatten() {
                state.autoplay_used = true;
                match event.action {
                    AutoplayNoteAction::Lift => {
                        let _ = judge_a_lift(state, event.column, update.row_time_ns);
                    }
                    AutoplayNoteAction::Tap => {
                        let _ = judge_a_tap(state, event.column, update.row_time_ns);
                    }
                }
            }
        }
        state.autoplay_cursor[player] = cursor;
    }

    let mut roll_cols = [usize::MAX; MAX_COLS];
    let roll_count =
        collect_active_autoplay_roll_columns(&state.active_holds, state.num_cols, &mut roll_cols);
    for col in roll_cols.into_iter().take(roll_count) {
        refresh_roll_life_on_step(state, col, state.current_music_time_ns);
    }
}

pub(super) fn run_replay(state: &mut State) {
    if !state.autoplay_enabled || !state.replay_mode {
        return;
    }
    let mut events = [None; MAX_COLS];
    let event_count = collect_ready_replay_edges(
        &state.replay_input,
        &mut state.replay_cursor,
        state.current_music_time_ns,
        state.num_cols,
        &mut events,
    );
    for edge in events.into_iter().take(event_count).flatten() {
        let col = edge.lane_index as usize;
        let Some(lane) = lane_from_column(col) else {
            continue;
        };
        push_input_edge(
            state,
            edge.source,
            lane,
            INPUT_SLOT_INVALID,
            edge.pressed,
            edge.event_music_time_ns,
            false,
        );
        state.autoplay_used = true;
    }
}
