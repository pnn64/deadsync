use deadsync_rules::timing::TimingData;
use std::sync::Arc;
use std::time::Instant;

use super::{
    GameplayOffsetAdjustKey, MAX_PLAYERS, State, clear_offset_adjust_hold_state,
    compute_end_times_ns, offset_adjust_target, offset_delta_target_seconds,
    refresh_timing_caches_for_offset_change, start_offset_adjust_hold_state,
    tick_offset_adjust_hold_state,
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
    let timing_players: [&_; MAX_PLAYERS] =
        std::array::from_fn(|player| state.timing_players[player].as_ref());
    refresh_timing_caches_for_offset_change(
        &state.notes,
        &timing_players,
        state.num_players,
        state.cols_per_player,
        &mut state.note_time_cache_ns,
        &mut state.hold_end_time_cache_ns,
        &mut state.row_entries,
        &state.mine_note_ix,
        &mut state.mine_note_time_ns,
    );
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
pub(super) fn clear_offset_adjust_hold_key(state: &mut State, key: GameplayOffsetAdjustKey) {
    clear_offset_adjust_hold_state(
        &mut state.offset_adjust_held_since,
        &mut state.offset_adjust_last_at,
        key,
    );
}

#[inline(always)]
pub(super) fn start_offset_adjust_hold_key(
    state: &mut State,
    key: GameplayOffsetAdjustKey,
    at: Instant,
) -> f32 {
    start_offset_adjust_hold_state(
        &mut state.offset_adjust_held_since,
        &mut state.offset_adjust_last_at,
        key,
        at,
    )
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
        match offset_adjust_target(state.shift_held, state.course_display_totals.is_some()) {
            super::GameplayOffsetAdjustTarget::Global => {
                let _ = apply_global_offset_delta(state, delta);
            }
            super::GameplayOffsetAdjustTarget::Song => {
                let _ = apply_song_offset_delta(state, delta);
            }
            super::GameplayOffsetAdjustTarget::None => {}
        }
    }
}

#[inline(always)]
pub(super) fn apply_global_offset_delta(state: &mut State, delta: f32) -> bool {
    let Some(new_offset) = offset_delta_target_seconds(state.global_offset_seconds, delta) else {
        return false;
    };
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
    let Some(new_offset) = offset_delta_target_seconds(state.song_offset_seconds, delta) else {
        return false;
    };

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
