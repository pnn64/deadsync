use deadsync_rules::timing::TimingData;
use std::sync::Arc;
use std::time::Instant;
use winit::keyboard::KeyCode;

use super::{
    GameplayOffsetAdjustKey, State, clear_offset_adjust_hold_state, compute_end_times_ns,
    player_index_for_column, start_offset_adjust_hold_state, tick_offset_adjust_hold_state,
};

#[inline(always)]
pub(super) fn mutate_timing_arc(
    timing: &mut Arc<TimingData>,
    mut apply: impl FnMut(&mut TimingData),
) {
    if let Some(inner) = Arc::get_mut(timing) {
        apply(inner);
        return;
    }
    let mut cloned = (**timing).clone();
    apply(&mut cloned);
    *timing = Arc::new(cloned);
}

#[inline(always)]
pub(super) fn refresh_timing_after_offset_change(state: &mut State) {
    let num_players = state.num_players;
    let cols_per_player = state.cols_per_player;
    for (time_ns, note) in state.note_time_cache_ns.iter_mut().zip(&state.notes) {
        let player = player_index_for_column(num_players, cols_per_player, note.column);
        *time_ns = state.timing_players[player].get_time_for_beat_ns(note.beat);
    }
    for (time_opt_ns, note) in state.hold_end_time_cache_ns.iter_mut().zip(&state.notes) {
        let player = player_index_for_column(num_players, cols_per_player, note.column);
        *time_opt_ns = note
            .hold
            .as_ref()
            .map(|h| state.timing_players[player].get_time_for_beat_ns(h.end_beat));
    }
    for row_entry in &mut state.row_entries {
        row_entry.time_ns = state.note_time_cache_ns[row_entry.note_indices()[0]];
    }
    for player in 0..state.num_players {
        let mine_note_time_ns = &mut state.mine_note_time_ns[player];
        mine_note_time_ns.clear();
        mine_note_time_ns.extend(
            state.mine_note_ix[player]
                .iter()
                .map(|&note_index| state.note_time_cache_ns[note_index]),
        );
    }
    state.beat_info_cache.reset(&state.timing);

    let (notes_end_time_ns, music_end_time_ns) = compute_end_times_ns(
        &state.notes,
        &state.note_time_cache_ns,
        &state.hold_end_time_cache_ns,
        state.music_rate,
        state.audio_end_time_ns,
    );
    state.notes_end_time_ns = notes_end_time_ns;
    state.music_end_time_ns = music_end_time_ns;
}

#[inline(always)]
fn offset_adjust_key(code: KeyCode) -> Option<GameplayOffsetAdjustKey> {
    match code {
        KeyCode::F11 => Some(GameplayOffsetAdjustKey::Decrease),
        KeyCode::F12 => Some(GameplayOffsetAdjustKey::Increase),
        _ => None,
    }
}

#[inline(always)]
pub(super) fn clear_offset_adjust_hold(state: &mut State, code: KeyCode) -> bool {
    let Some(key) = offset_adjust_key(code) else {
        return false;
    };
    clear_offset_adjust_hold_state(
        &mut state.offset_adjust_held_since,
        &mut state.offset_adjust_last_at,
        key,
    );
    true
}

#[inline(always)]
pub(super) fn start_offset_adjust_hold(
    state: &mut State,
    code: KeyCode,
    at: Instant,
) -> Option<f32> {
    let key = offset_adjust_key(code)?;
    Some(start_offset_adjust_hold_state(
        &mut state.offset_adjust_held_since,
        &mut state.offset_adjust_last_at,
        key,
        at,
    ))
}

#[inline(always)]
pub(super) fn update_offset_adjust_hold(state: &mut State) {
    let now = Instant::now();
    for key in [
        GameplayOffsetAdjustKey::Decrease,
        GameplayOffsetAdjustKey::Increase,
    ] {
        let Some(delta) = tick_offset_adjust_hold_state(
            &state.offset_adjust_held_since,
            &mut state.offset_adjust_last_at,
            key,
            now,
        ) else {
            continue;
        };
        if state.shift_held {
            let _ = apply_global_offset_delta(state, delta);
        } else if state.course_display_totals.is_none() {
            let _ = apply_song_offset_delta(state, delta);
        }
    }
}

#[inline(always)]
pub(super) fn apply_global_offset_delta(state: &mut State, delta: f32) -> bool {
    let old_offset = state.global_offset_seconds;
    let new_offset = old_offset + delta;
    if (new_offset - old_offset).abs() < 0.000_001_f32 {
        return false;
    }
    mutate_timing_arc(&mut state.timing, |timing| {
        timing.set_global_offset_seconds(new_offset)
    });
    for (player_idx, timing) in state.timing_players.iter_mut().enumerate() {
        let effective_offset = new_offset + state.player_global_offset_shift_seconds[player_idx];
        mutate_timing_arc(timing, |timing| {
            timing.set_global_offset_seconds(effective_offset)
        });
    }
    refresh_timing_after_offset_change(state);
    state.global_offset_seconds = new_offset;
    true
}

#[inline(always)]
pub(super) fn apply_song_offset_delta(state: &mut State, delta: f32) -> bool {
    let old_offset = state.song_offset_seconds;
    let new_offset = old_offset + delta;
    if (new_offset - old_offset).abs() < 0.000_001_f32 {
        return false;
    }

    mutate_timing_arc(&mut state.timing, |timing| {
        timing.shift_song_offset_seconds(delta)
    });
    for timing in &mut state.timing_players {
        mutate_timing_arc(timing, |timing| timing.shift_song_offset_seconds(delta));
    }
    refresh_timing_after_offset_change(state);
    state.song_offset_seconds = new_offset;
    true
}
