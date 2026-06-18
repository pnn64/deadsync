use deadsync_profile::TimingTickMode as TickMode;
use log::debug;
use std::time::Instant;
use winit::keyboard::KeyCode;

use super::offset::{
    apply_global_offset_delta, apply_song_offset_delta, clear_offset_adjust_hold,
    start_offset_adjust_hold,
};
use super::{
    GameplaySessionCommand, MAX_COLS, State, assist_clap_cursor_for_row, assist_row_no_offset,
    autoplay_cursor_for_enable, gameplay_tick_mode_from_profile, next_autosync_mode,
    next_timing_tick_mode, player_note_range, profile_tick_mode_from_gameplay,
    queue_session_command, timing_tick_mode_debug_label as gameplay_tick_mode_debug_label,
    timing_tick_mode_status_line as gameplay_tick_mode_status_line,
};

#[inline(always)]
pub fn timing_tick_status_line(state: &State) -> Option<&'static str> {
    gameplay_tick_mode_status_line(gameplay_tick_mode_from_profile(state.tick_mode))
}

fn set_tick_mode(state: &mut State, mode: TickMode, now_music_time: f32) {
    if state.tick_mode == mode {
        return;
    }
    state.tick_mode = mode;
    queue_session_command(
        state,
        GameplaySessionCommand::SetTimingTickMode(gameplay_tick_mode_from_profile(mode)),
    );

    let song_row = assist_row_no_offset(state, now_music_time);
    state.assist_last_crossed_row = song_row;
    state.assist_clap_cursor = assist_clap_cursor_for_row(&state.assist_clap_rows, song_row);

    debug!(
        "Timing ticks set to {} (F7).",
        gameplay_tick_mode_debug_label(gameplay_tick_mode_from_profile(mode))
    );
}

fn set_autoplay_enabled(state: &mut State, enabled: bool, now_music_time: f32) {
    if state.autoplay_enabled == enabled {
        return;
    }
    state.autoplay_enabled = enabled;

    if enabled {
        state.input_slot_count = 0;
        state.input_lane_counts = [0; MAX_COLS];
        state.prev_inputs = [false; MAX_COLS];
        state.receptor_glow_timers = [0.0; MAX_COLS];
        state.receptor_glow_press_timers = [0.0; MAX_COLS];
        state.receptor_glow_lift_start_alpha = [0.0; MAX_COLS];
        state.receptor_glow_lift_start_zoom = [1.0; MAX_COLS];
        state.pending_edges.clear();
        for player in 0..state.num_players {
            state.autoplay_cursor[player] = autoplay_cursor_for_enable(
                state.next_tap_miss_cursor[player],
                player_note_range(state, player),
            );
        }
        debug!("Autoplay enabled (F8). Scores for this stage will not be saved.");
        return;
    }

    debug!("Autoplay disabled (F8).");
    let _ = now_music_time;
}

#[inline(always)]
fn cycle_autosync_mode(state: &mut State) {
    state.autosync_mode =
        next_autosync_mode(state.autosync_mode, state.course_display_totals.is_some());
}

#[inline(always)]
fn update_raw_modifier_state(state: &mut State, code: KeyCode, pressed: bool) {
    match code {
        KeyCode::ShiftLeft | KeyCode::ShiftRight => state.shift_held = pressed,
        KeyCode::ControlLeft | KeyCode::ControlRight => state.ctrl_held = pressed,
        _ => {}
    }
}

pub fn sync_queued_raw_modifiers(state: &mut State, shift_held: bool, ctrl_held: bool) {
    state.shift_held = shift_held;
    state.ctrl_held = ctrl_held;
}

pub enum RawKeyAction {
    None,
    Restart,
}

pub fn handle_queued_raw_key(
    state: &mut State,
    code: KeyCode,
    pressed: bool,
    timestamp: Instant,
    now_music_time: f32,
    allow_commands: bool,
) -> RawKeyAction {
    update_raw_modifier_state(state, code, pressed);
    if !pressed {
        let _ = clear_offset_adjust_hold(state, code);
        return RawKeyAction::None;
    }
    if !allow_commands {
        return RawKeyAction::None;
    }
    if code == KeyCode::KeyR && state.ctrl_held {
        return RawKeyAction::Restart;
    }
    if code == KeyCode::F6 {
        cycle_autosync_mode(state);
        return RawKeyAction::None;
    }

    if code == KeyCode::F7 {
        let next_mode = profile_tick_mode_from_gameplay(next_timing_tick_mode(
            gameplay_tick_mode_from_profile(state.tick_mode),
        ));
        set_tick_mode(state, next_mode, now_music_time);
        return RawKeyAction::None;
    }

    if code == KeyCode::F8 {
        set_autoplay_enabled(state, !state.autoplay_enabled, now_music_time);
        return RawKeyAction::None;
    }
    let Some(delta) = start_offset_adjust_hold(state, code, timestamp) else {
        return RawKeyAction::None;
    };

    if state.shift_held {
        let _ = apply_global_offset_delta(state, delta);
        return RawKeyAction::None;
    }
    if state.course_display_totals.is_none() {
        let _ = apply_song_offset_delta(state, delta);
    }
    RawKeyAction::None
}
