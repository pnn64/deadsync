use crate::game::timing::TimingData;
use std::sync::Arc;
use std::time::Instant;
use winit::keyboard::KeyCode;

use super::{
    OFFSET_ADJUST_REPEAT_DELAY, OFFSET_ADJUST_REPEAT_INTERVAL, OFFSET_ADJUST_STEP_SECONDS, State,
    compute_end_times_ns, quantize_offset_seconds,
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
        let player = if num_players <= 1 || cols_per_player == 0 {
            0
        } else {
            (note.column / cols_per_player).min(num_players.saturating_sub(1))
        };
        *time_ns = state.timing_players[player].get_time_for_beat_ns(note.beat);
    }
    for (time_opt_ns, note) in state.hold_end_time_cache_ns.iter_mut().zip(&state.notes) {
        let player = if num_players <= 1 || cols_per_player == 0 {
            0
        } else {
            (note.column / cols_per_player).min(num_players.saturating_sub(1))
        };
        *time_opt_ns = note
            .hold
            .as_ref()
            .map(|h| state.timing_players[player].get_time_for_beat_ns(h.end_beat));
    }
    for row_entry in &mut state.row_entries {
        if let Some(&note_index) = row_entry.nonmine_note_indices.first() {
            row_entry.time_ns = state.note_time_cache_ns[note_index];
        }
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
    );
    state.notes_end_time_ns = notes_end_time_ns;
    state.music_end_time_ns = music_end_time_ns;
}

#[inline(always)]
fn quantized_offset_change_line(label: &str, start: f32, new: f32) -> Option<String> {
    let start_q = quantize_offset_seconds(start);
    let new_q = quantize_offset_seconds(new);
    let delta_q = new_q - start_q;
    if delta_q.abs() < 0.000_1_f32 {
        return None;
    }
    let direction = if delta_q > 0.0 { "earlier" } else { "later" };
    Some(format!(
        "{label} from {start_q:+.3} to {new_q:+.3} (notes {direction})"
    ))
}

#[inline(always)]
fn refresh_sync_overlay_message(state: &mut State) {
    let mut message = String::new();
    if let Some(global_line) = quantized_offset_change_line(
        "Global Offset",
        state.initial_global_offset_seconds,
        state.global_offset_seconds,
    ) {
        message.push_str(&global_line);
    }
    if let Some(song_line) = quantized_offset_change_line(
        "Song offset",
        state.initial_song_offset_seconds,
        state.song_offset_seconds,
    ) {
        if !message.is_empty() {
            message.push('\n');
        }
        message.push_str(&song_line);
    }
    if message.is_empty() {
        state.sync_overlay_message = None;
    } else {
        state.sync_overlay_message = Some(Arc::<str>::from(message));
    }
}

#[inline(always)]
fn offset_adjust_slot(code: KeyCode) -> Option<usize> {
    match code {
        KeyCode::F11 => Some(0),
        KeyCode::F12 => Some(1),
        _ => None,
    }
}

#[inline(always)]
fn offset_adjust_delta(code: KeyCode) -> Option<f32> {
    match code {
        KeyCode::F11 => Some(-OFFSET_ADJUST_STEP_SECONDS),
        KeyCode::F12 => Some(OFFSET_ADJUST_STEP_SECONDS),
        _ => None,
    }
}

#[inline(always)]
pub(super) fn clear_offset_adjust_hold(state: &mut State, code: KeyCode) -> bool {
    let Some(slot) = offset_adjust_slot(code) else {
        return false;
    };
    state.offset_adjust_held_since[slot] = None;
    state.offset_adjust_last_at[slot] = None;
    true
}

#[inline(always)]
pub(super) fn start_offset_adjust_hold(
    state: &mut State,
    code: KeyCode,
    at: Instant,
) -> Option<f32> {
    let slot = offset_adjust_slot(code)?;
    state.offset_adjust_held_since[slot] = Some(at);
    state.offset_adjust_last_at[slot] = Some(at);
    offset_adjust_delta(code)
}

#[inline(always)]
pub(super) fn update_offset_adjust_hold(state: &mut State) {
    let now = Instant::now();
    for code in [KeyCode::F11, KeyCode::F12] {
        let Some(slot) = offset_adjust_slot(code) else {
            continue;
        };
        let (Some(held_since), Some(last_at)) = (
            state.offset_adjust_held_since[slot],
            state.offset_adjust_last_at[slot],
        ) else {
            continue;
        };
        if now.duration_since(held_since) < OFFSET_ADJUST_REPEAT_DELAY
            || now.duration_since(last_at) < OFFSET_ADJUST_REPEAT_INTERVAL
        {
            continue;
        }
        let Some(delta) = offset_adjust_delta(code) else {
            continue;
        };
        if state.shift_held {
            let _ = apply_global_offset_delta(state, delta);
        } else if state.course_display_totals.is_none() {
            let _ = apply_song_offset_delta(state, delta);
        }
        state.offset_adjust_last_at[slot] = Some(now);
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
    refresh_sync_overlay_message(state);
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
    refresh_sync_overlay_message(state);
    true
}
