use crate::game::profile::{self, TimingTickMode as TickMode};
use log::debug;
use std::time::Instant;
use winit::keyboard::KeyCode;

use super::offset::{
    apply_global_offset_delta, apply_song_offset_delta, clear_offset_adjust_hold,
    start_offset_adjust_hold,
};
use super::{
    AutosyncMode, MAX_COLS, State, assist_clap_cursor_for_row, assist_row_no_offset,
    player_note_range, song_time_ns_to_seconds,
};

#[inline(always)]
fn current_music_time_from_stream(state: &State) -> f32 {
    song_time_ns_to_seconds(super::clock::current_song_clock_snapshot(state).song_time_ns)
}

#[inline(always)]
pub(super) const fn next_tick_mode(mode: TickMode) -> TickMode {
    match mode {
        TickMode::Off => TickMode::Assist,
        TickMode::Assist => TickMode::Hit,
        TickMode::Hit => TickMode::Off,
    }
}

#[inline(always)]
pub(super) const fn tick_mode_status_line(mode: TickMode) -> Option<&'static str> {
    match mode {
        TickMode::Off => None,
        TickMode::Assist => Some("Assist Tick"),
        TickMode::Hit => Some("Hit Tick"),
    }
}

#[inline(always)]
const fn tick_mode_debug_label(mode: TickMode) -> &'static str {
    match mode {
        TickMode::Off => "off",
        TickMode::Assist => "assist tick",
        TickMode::Hit => "hit tick",
    }
}

#[inline(always)]
pub fn timing_tick_status_line(state: &State) -> Option<&'static str> {
    tick_mode_status_line(state.tick_mode)
}

fn set_tick_mode(state: &mut State, mode: TickMode, now_music_time: f32) {
    if state.tick_mode == mode {
        return;
    }
    state.tick_mode = mode;
    profile::set_session_timing_tick_mode(mode);

    let song_row = assist_row_no_offset(state, now_music_time);
    state.assist_last_crossed_row = song_row;
    state.assist_clap_cursor = assist_clap_cursor_for_row(&state.assist_clap_rows, song_row);

    debug!("Timing ticks set to {} (F7).", tick_mode_debug_label(mode));
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
            let (note_start, note_end) = player_note_range(state, player);
            state.autoplay_cursor[player] = state.next_tap_miss_cursor[player]
                .max(note_start)
                .min(note_end);
        }
        debug!("Autoplay enabled (F8). Scores for this stage will not be saved.");
        return;
    }

    debug!("Autoplay disabled (F8).");
    let _ = now_music_time;
}

#[inline(always)]
pub const fn autosync_mode_status_line(mode: AutosyncMode) -> Option<&'static str> {
    match mode {
        AutosyncMode::Off => None,
        AutosyncMode::Song => Some("AutoSync Song"),
        AutosyncMode::Machine => Some("AutoSync Machine"),
    }
}

#[inline(always)]
fn cycle_autosync_mode(state: &mut State) {
    let mut next = match state.autosync_mode {
        AutosyncMode::Off => AutosyncMode::Song,
        AutosyncMode::Song => AutosyncMode::Machine,
        AutosyncMode::Machine => AutosyncMode::Off,
    };
    if state.course_display_totals.is_some() && next == AutosyncMode::Song {
        next = AutosyncMode::Machine;
    }
    state.autosync_mode = next;
}

#[inline(always)]
fn update_raw_modifier_state(state: &mut State, code: KeyCode, pressed: bool) {
    match code {
        KeyCode::ShiftLeft | KeyCode::ShiftRight => state.shift_held = pressed,
        KeyCode::ControlLeft | KeyCode::ControlRight => state.ctrl_held = pressed,
        _ => {}
    }
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
        let now_music_time = current_music_time_from_stream(state);
        set_tick_mode(state, next_tick_mode(state.tick_mode), now_music_time);
        return RawKeyAction::None;
    }

    if code == KeyCode::F8 {
        let now_music_time = current_music_time_from_stream(state);
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
