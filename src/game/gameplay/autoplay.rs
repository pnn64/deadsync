use crate::game::note::NoteType;

use super::{
    MAX_COLS, SongTimeNs, State, handle_hold_let_go, handle_hold_success, judge_a_lift,
    judge_a_tap, lane_from_column, player_note_range, push_input_edge, refresh_roll_life_on_step,
    song_time_ns_to_seconds,
};

#[inline(always)]
pub(super) fn autoplay_blocks_scoring(state: &State) -> bool {
    live_autoplay_enabled(state)
}

#[inline(always)]
pub(super) const fn live_autoplay_enabled_from_flags(
    autoplay_enabled: bool,
    replay_mode: bool,
) -> bool {
    autoplay_enabled && !replay_mode
}

#[inline(always)]
pub(super) fn live_autoplay_enabled(state: &State) -> bool {
    live_autoplay_enabled_from_flags(state.autoplay_enabled, state.replay_mode)
}

#[inline(always)]
fn settle_due_autoplay_active_holds(state: &mut State, cutoff_time_ns: SongTimeNs) {
    for column in 0..state.num_cols {
        let Some(active) = state.active_holds[column].as_ref() else {
            continue;
        };
        if active.end_time_ns > cutoff_time_ns {
            continue;
        }
        let note_index = active.note_index;
        let end_time_ns = active.end_time_ns;
        let hold_succeeded = !active.let_go && active.life > 0.0;
        state.active_holds[column] = None;
        if hold_succeeded {
            handle_hold_success(state, column, note_index);
        } else {
            handle_hold_let_go(state, column, note_index, end_time_ns);
        }
    }
}

pub(super) fn run_autoplay(state: &mut State, now_music_time_ns: SongTimeNs) {
    if !state.autoplay_enabled {
        return;
    }

    for player in 0..state.num_players {
        let (note_start, note_end) = player_note_range(state, player);
        let mut cursor = state.autoplay_cursor[player].max(note_start);
        while cursor < note_end {
            while cursor < note_end && state.notes[cursor].result.is_some() {
                cursor += 1;
            }
            if cursor >= note_end {
                break;
            }

            let row = state.notes[cursor].row_index;
            let mut row_end = cursor + 1;
            while row_end < note_end && state.notes[row_end].row_index == row {
                row_end += 1;
            }
            let row_time_ns = state.note_time_cache_ns[cursor];
            if row_time_ns > now_music_time_ns {
                break;
            }
            let row_time = song_time_ns_to_seconds(row_time_ns);
            // Finalize any already-ended autoplay holds before a new warped
            // row on the same lane can replace the active hold slot.
            settle_due_autoplay_active_holds(state, row_time_ns);
            for idx in cursor..row_end {
                let (result_is_some, is_fake, can_be_judged, note_type, col) = {
                    let note = &state.notes[idx];
                    (
                        note.result.is_some(),
                        note.is_fake,
                        note.can_be_judged,
                        note.note_type,
                        note.column,
                    )
                };
                if result_is_some
                    || is_fake
                    || !can_be_judged
                    || matches!(note_type, NoteType::Mine)
                {
                    continue;
                }

                if col >= state.num_cols {
                    continue;
                }

                state.autoplay_used = true;
                match note_type {
                    NoteType::Lift => {
                        let _ = judge_a_lift(state, col, row_time, row_time_ns);
                    }
                    NoteType::Tap | NoteType::Hold | NoteType::Roll => {
                        let _ = judge_a_tap(state, col, row_time, row_time_ns);
                    }
                    NoteType::Mine | NoteType::Fake => {}
                }
            }

            cursor = row_end;
        }
        state.autoplay_cursor[player] = cursor;
    }

    let mut roll_cols = [usize::MAX; MAX_COLS];
    let mut roll_count = 0usize;
    for col in 0..state.num_cols {
        if state.active_holds[col]
            .as_ref()
            .is_some_and(|active| matches!(active.note_type, NoteType::Roll) && !active.let_go)
            && roll_count < MAX_COLS
        {
            roll_cols[roll_count] = col;
            roll_count += 1;
        }
    }
    for col in roll_cols.into_iter().take(roll_count) {
        refresh_roll_life_on_step(state, col, state.current_music_time_ns);
    }
}

pub(super) fn run_replay(state: &mut State) {
    if !state.autoplay_enabled || !state.replay_mode {
        return;
    }
    while state.replay_cursor < state.replay_input.len() {
        let edge = state.replay_input[state.replay_cursor];
        if edge.event_music_time_ns > state.current_music_time_ns {
            break;
        }
        state.replay_cursor += 1;
        let col = edge.lane_index as usize;
        if col >= state.num_cols {
            continue;
        }
        let Some(lane) = lane_from_column(col) else {
            continue;
        };
        push_input_edge(
            state,
            edge.source,
            lane,
            edge.pressed,
            edge.event_music_time_ns,
            false,
        );
        state.autoplay_used = true;
    }
}
