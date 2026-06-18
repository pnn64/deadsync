use deadsync_profile::TimingTickMode as TickMode;
use log::debug;
use std::time::Instant;
use winit::keyboard::KeyCode;

use super::offset::{
    apply_global_offset_delta, apply_song_offset_delta, clear_offset_adjust_hold_key,
    start_offset_adjust_hold_key,
};
use super::{
    GameplayOffsetAdjustKey, GameplayOffsetAdjustTarget, GameplayRawKeyInput, GameplayRawKeyPlan,
    GameplaySessionCommand, MAX_COLS, State, assist_clap_cursor_for_row, assist_row_no_offset,
    autoplay_cursor_for_enable, gameplay_raw_key_plan, gameplay_tick_mode_from_profile,
    player_note_range, profile_tick_mode_from_gameplay, queue_session_command,
    timing_tick_mode_debug_label as gameplay_tick_mode_debug_label,
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
fn update_raw_modifier_state(state: &mut State, code: KeyCode, pressed: bool) {
    match code {
        KeyCode::ShiftLeft | KeyCode::ShiftRight => state.shift_held = pressed,
        KeyCode::ControlLeft | KeyCode::ControlRight => state.ctrl_held = pressed,
        _ => {}
    }
}

#[inline(always)]
fn gameplay_raw_key_input(code: KeyCode) -> GameplayRawKeyInput {
    match code {
        KeyCode::KeyR => GameplayRawKeyInput::Restart,
        KeyCode::F6 => GameplayRawKeyInput::Autosync,
        KeyCode::F7 => GameplayRawKeyInput::TimingTick,
        KeyCode::F8 => GameplayRawKeyInput::Autoplay,
        KeyCode::F11 => GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Decrease),
        KeyCode::F12 => GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Increase),
        _ => GameplayRawKeyInput::Other,
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
    let plan = gameplay_raw_key_plan(
        gameplay_raw_key_input(code),
        pressed,
        allow_commands,
        state.ctrl_held,
        state.shift_held,
        state.autosync_mode,
        state.course_display_totals.is_some(),
        gameplay_tick_mode_from_profile(state.tick_mode),
        state.autoplay_enabled,
    );
    match plan {
        GameplayRawKeyPlan::Restart => return RawKeyAction::Restart,
        GameplayRawKeyPlan::SetAutosyncMode(mode) => state.autosync_mode = mode,
        GameplayRawKeyPlan::SetTimingTickMode(mode) => {
            set_tick_mode(state, profile_tick_mode_from_gameplay(mode), now_music_time)
        }
        GameplayRawKeyPlan::SetAutoplayEnabled(enabled) => {
            set_autoplay_enabled(state, enabled, now_music_time)
        }
        GameplayRawKeyPlan::StartOffsetAdjust { key, target } => {
            let delta = start_offset_adjust_hold_key(state, key, timestamp);
            match target {
                GameplayOffsetAdjustTarget::Global => {
                    let _ = apply_global_offset_delta(state, delta);
                }
                GameplayOffsetAdjustTarget::Song => {
                    let _ = apply_song_offset_delta(state, delta);
                }
                GameplayOffsetAdjustTarget::None => {}
            }
        }
        GameplayRawKeyPlan::ClearOffsetAdjust(key) => clear_offset_adjust_hold_key(state, key),
        GameplayRawKeyPlan::None => {}
    }
    RawKeyAction::None
}
