use deadsync_core::input::{InputSource, Lane};
use deadsync_input::{INPUT_SLOT_INVALID, InputEdge, InputEvent, VirtualAction, lane_from_action};
use deadsync_profile as profile_data;
use log::{debug, warn};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Instant;

use super::{
    ASSIST_TICK_SFX_PATH, ActiveInputSlot, COMBO_HUNDRED_MILESTONE_DURATION,
    COMBO_THOUSAND_MILESTONE_DURATION, ComboMilestoneKind, ExitTransitionKind,
    GAMEPLAY_INPUT_BACKLOG_WARN, GAMEPLAY_INPUT_LATENCY_WARN_US, GameplayAction,
    GameplayReceptorGlowBehavior, GameplayReceptorStepBehavior, GameplayUpdatePhaseTimings,
    HELD_MISS_TOTAL_DURATION, HOLD_JUDGMENT_TOTAL_DURATION, HoldToExitKey, INVALID_SONG_TIME_NS,
    MAX_ACTIVE_INPUT_SLOTS, RECEPTOR_GLOW_DURATION, RecordedLaneEdge, SongClockSnapshot,
    SongTimeNs, State, TickMode, abort_hold_to_exit, active_hold_counts_as_pressed, add_elapsed_us,
    begin_exit_transition, column_flash_duration, current_music_time_s, elapsed_us_between,
    gameplay_input_log_enabled, integrate_active_hold_to_time, judge_a_lift, judge_a_tap,
    lane_edge_judges_lift, lane_edge_judges_tap, lane_press_started, lane_release_finished,
    live_autoplay_enabled, music_time_ns_from_song_clock, queue_preloaded_assist_tick,
    record_step_calories, refresh_roll_life_on_step, song_time_ns_invalid, song_time_ns_to_seconds,
};

const UNMAPPED_INPUT_CLOCK_WARN_INTERVAL_NS: SongTimeNs = 1_000_000_000;
static LAST_UNMAPPED_INPUT_CLOCK_WARN_NS: AtomicI64 = AtomicI64::new(i64::MIN);

#[inline(always)]
fn should_warn_unmapped_input_clock(song_time_ns: SongTimeNs) -> bool {
    let last = LAST_UNMAPPED_INPUT_CLOCK_WARN_NS.load(Ordering::Relaxed);
    let should_warn = last == i64::MIN
        || song_time_ns < last
        || song_time_ns.saturating_sub(last) >= UNMAPPED_INPUT_CLOCK_WARN_INTERVAL_NS;
    if should_warn {
        LAST_UNMAPPED_INPUT_CLOCK_WARN_NS.store(song_time_ns, Ordering::Relaxed);
    }
    should_warn
}

#[inline(always)]
fn receptor_glow_duration_for_col(state: &State, col: usize) -> f32 {
    let player = if state.num_players <= 1 || state.cols_per_player == 0 {
        0
    } else {
        (col / state.cols_per_player).min(state.num_players.saturating_sub(1))
    };
    let behavior = state
        .noteskin_effects
        .receptor_glow_behavior_for_player(player);
    Some(behavior.duration)
        .filter(|d| *d > f32::EPSILON)
        .unwrap_or(RECEPTOR_GLOW_DURATION)
}

#[inline(always)]
fn receptor_glow_behavior_for_col(state: &State, col: usize) -> GameplayReceptorGlowBehavior {
    let player = if state.num_players <= 1 || state.cols_per_player == 0 {
        0
    } else {
        (col / state.cols_per_player).min(state.num_players.saturating_sub(1))
    };
    state
        .noteskin_effects
        .receptor_glow_behavior_for_player(player)
}

#[inline(always)]
fn receptor_step_behavior_for_col(
    state: &State,
    col: usize,
    window: Option<&str>,
) -> GameplayReceptorStepBehavior {
    let player = if state.num_players <= 1 || state.cols_per_player == 0 {
        0
    } else {
        (col / state.cols_per_player).min(state.num_players.saturating_sub(1))
    };
    let local_col = if state.cols_per_player == 0 {
        col
    } else {
        col % state.cols_per_player
    };
    state
        .noteskin_effects
        .receptor_step_behavior_for_col(player, local_col, window)
}

#[inline(always)]
pub(super) fn lane_is_pressed(state: &State, col: usize) -> bool {
    state.input_lane_counts[col] != 0
}

#[inline(always)]
const fn lane_bit(lane_idx: usize) -> u8 {
    1u8 << lane_idx
}

#[inline(always)]
fn normalized_input_slot(lane: Lane, input_slot: u32) -> u32 {
    if input_slot == INPUT_SLOT_INVALID {
        lane.index() as u32
    } else {
        input_slot
    }
}

#[inline(always)]
fn find_input_slot(state: &State, source: InputSource, input_slot: u32) -> Option<usize> {
    state.input_slots[..state.input_slot_count]
        .iter()
        .position(|slot| slot.source == source && slot.input_slot == input_slot)
}

#[inline(always)]
fn insert_input_slot(state: &mut State, source: InputSource, input_slot: u32) -> Option<usize> {
    if let Some(idx) = find_input_slot(state, source, input_slot) {
        return Some(idx);
    }
    if state.input_slot_count >= MAX_ACTIVE_INPUT_SLOTS {
        debug!(
            "Gameplay active input slot table full; dropping held-state edge for {:?} slot {}",
            source, input_slot
        );
        return None;
    }
    let idx = state.input_slot_count;
    state.input_slots[idx] = ActiveInputSlot {
        source,
        input_slot,
        lane_mask: 0,
    };
    state.input_slot_count += 1;
    Some(idx)
}

#[inline(always)]
fn remove_input_slot_if_empty(state: &mut State, idx: usize) {
    if state.input_slots[idx].lane_mask != 0 {
        return;
    }
    state.input_slot_count = state.input_slot_count.saturating_sub(1);
    if idx < state.input_slot_count {
        state.input_slots[idx] = state.input_slots[state.input_slot_count];
    }
}

#[inline(always)]
fn input_slot_lane_is_down(
    state: &State,
    lane: Lane,
    source: InputSource,
    input_slot: u32,
) -> bool {
    let input_slot = normalized_input_slot(lane, input_slot);
    let bit = lane_bit(lane.index());
    find_input_slot(state, source, input_slot)
        .is_some_and(|idx| state.input_slots[idx].lane_mask & bit != 0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LaneInputUpdate {
    pub(super) was_down: bool,
    pub(super) is_down: bool,
    pub(super) slot_was_down: bool,
}

pub(super) fn update_lane_input_slot(
    state: &mut State,
    lane: Lane,
    source: InputSource,
    input_slot: u32,
    pressed: bool,
) -> LaneInputUpdate {
    let lane_idx = lane.index();
    let input_slot = normalized_input_slot(lane, input_slot);
    let bit = lane_bit(lane_idx);
    let was_down = lane_is_pressed(state, lane_idx);
    let mut slot_was_down = false;

    if pressed {
        if let Some(idx) = insert_input_slot(state, source, input_slot) {
            slot_was_down = state.input_slots[idx].lane_mask & bit != 0;
            if !slot_was_down {
                state.input_slots[idx].lane_mask |= bit;
                state.input_lane_counts[lane_idx] =
                    state.input_lane_counts[lane_idx].saturating_add(1);
            }
        }
    } else if let Some(idx) = find_input_slot(state, source, input_slot) {
        slot_was_down = state.input_slots[idx].lane_mask & bit != 0;
        if slot_was_down {
            state.input_slots[idx].lane_mask &= !bit;
            state.input_lane_counts[lane_idx] = state.input_lane_counts[lane_idx].saturating_sub(1);
            remove_input_slot_if_empty(state, idx);
        }
    }

    LaneInputUpdate {
        was_down,
        is_down: lane_is_pressed(state, lane_idx),
        slot_was_down,
    }
}

#[inline(always)]
pub(super) fn trigger_receptor_glow_pulse(state: &mut State, col: usize) {
    let behavior = receptor_glow_behavior_for_col(state, col);
    state.receptor_glow_press_timers[col] = 0.0;
    state.receptor_glow_lift_start_alpha[col] = behavior.press_alpha_start;
    state.receptor_glow_lift_start_zoom[col] = behavior.press_zoom_start;
    state.receptor_glow_timers[col] = receptor_glow_duration_for_col(state, col);
}

#[inline(always)]
fn trigger_receptor_step_command(state: &mut State, col: usize, window: Option<&str>) {
    if col >= state.num_cols {
        return;
    }
    start_receptor_glow_press(state, col);
    let behavior = receptor_step_behavior_for_col(state, col, window);
    if behavior.duration > f32::EPSILON || behavior.interrupts {
        state.receptor_bop_behaviors[col] = behavior;
        state.receptor_bop_timers[col] = behavior.duration.max(0.0);
    }
}

#[inline(always)]
pub(super) fn trigger_receptor_step_pulse(state: &mut State, col: usize) {
    trigger_receptor_step_command(state, col, None);
}

#[inline(always)]
pub(super) fn trigger_receptor_score_pulse(state: &mut State, col: usize, window: &'static str) {
    trigger_receptor_step_command(state, col, Some(window));
}

#[inline(always)]
fn start_receptor_glow_press(state: &mut State, col: usize) {
    let behavior = receptor_glow_behavior_for_col(state, col);
    state.receptor_glow_timers[col] = 0.0;
    state.receptor_glow_press_timers[col] = behavior.press_duration;
    state.receptor_glow_lift_start_alpha[col] = behavior.press_alpha_end;
    state.receptor_glow_lift_start_zoom[col] = behavior.press_zoom_end;
}

#[inline(always)]
fn release_receptor_glow(state: &mut State, col: usize) {
    let behavior = receptor_glow_behavior_for_col(state, col);
    let (alpha, zoom) = if state.receptor_glow_press_timers[col] > f32::EPSILON
        && behavior.press_duration > f32::EPSILON
    {
        behavior.sample_press(state.receptor_glow_press_timers[col])
    } else {
        (behavior.press_alpha_end, behavior.press_zoom_end)
    };
    state.receptor_glow_press_timers[col] = 0.0;
    state.receptor_glow_lift_start_alpha[col] = alpha;
    state.receptor_glow_lift_start_zoom[col] = zoom;
    state.receptor_glow_timers[col] = receptor_glow_duration_for_col(state, col);
}

#[inline(always)]
pub fn receptor_glow_visual_for_col(state: &State, col: usize) -> Option<(f32, f32)> {
    if col >= state.num_cols {
        return None;
    }
    let behavior = receptor_glow_behavior_for_col(state, col);
    if state.receptor_glow_press_timers[col] > f32::EPSILON
        && behavior.press_duration > f32::EPSILON
    {
        return Some(behavior.sample_press(state.receptor_glow_press_timers[col]));
    }
    if lane_is_pressed(state, col) {
        return Some((behavior.press_alpha_end, behavior.press_zoom_end));
    }
    if state.receptor_glow_timers[col] > f32::EPSILON {
        return Some(behavior.sample_lift(
            state.receptor_glow_timers[col],
            state.receptor_glow_lift_start_alpha[col],
            state.receptor_glow_lift_start_zoom[col],
        ));
    }
    None
}

#[inline(always)]
pub(super) fn sync_active_hold_pressed_state(state: &mut State, column: usize, lane_pressed: bool) {
    let live_autoplay = live_autoplay_enabled(state);
    let Some(active) = state.active_holds[column].as_mut() else {
        return;
    };
    active.is_pressed = active_hold_counts_as_pressed(live_autoplay, lane_pressed);
}

pub fn queue_input_edge(
    state: &mut State,
    source: InputSource,
    lane: Lane,
    input_slot: u32,
    pressed: bool,
    timestamp: Instant,
    timestamp_host_nanos: u64,
    stored_at: Instant,
    emitted_at: Instant,
) {
    if state.autoplay_enabled {
        return;
    }
    let lane = match (state.session.play_style, state.session.player_side, lane) {
        // Single-player: reject the "other side" entirely so only one set of bindings can play.
        (
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            Lane::P2Left | Lane::P2Down | Lane::P2Up | Lane::P2Right,
        ) => return,
        (
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P2,
            Lane::Left | Lane::Down | Lane::Up | Lane::Right,
        ) => return,
        // P2-only single: remap P2 lanes into the 4-col field.
        (profile_data::PlayStyle::Single, profile_data::PlayerSide::P2, Lane::P2Left) => Lane::Left,
        (profile_data::PlayStyle::Single, profile_data::PlayerSide::P2, Lane::P2Down) => Lane::Down,
        (profile_data::PlayStyle::Single, profile_data::PlayerSide::P2, Lane::P2Up) => Lane::Up,
        (profile_data::PlayStyle::Single, profile_data::PlayerSide::P2, Lane::P2Right) => {
            Lane::Right
        }
        _ => lane,
    };
    if lane.index() >= state.num_cols {
        return;
    }

    let queued_at = Instant::now();
    // Live input keeps the physical timestamp and is converted against the
    // frame's authoritative song clock when processed. Do not pre-resolve it
    // through the raw audio stream clock.
    push_input_edge_timed(
        state,
        source,
        lane,
        input_slot,
        pressed,
        timestamp,
        timestamp_host_nanos,
        stored_at,
        emitted_at,
        queued_at,
        INVALID_SONG_TIME_NS,
        state.replay_capture_enabled,
    );
}

#[inline(always)]
pub fn set_replay_capture_enabled(state: &mut State, enabled: bool) {
    state.replay_capture_enabled = enabled;
}

#[inline(always)]
pub fn replay_capture_enabled(state: &State) -> bool {
    state.replay_capture_enabled
}

#[inline(always)]
pub(super) fn push_input_edge(
    state: &mut State,
    source: InputSource,
    lane: Lane,
    input_slot: u32,
    pressed: bool,
    event_music_time_ns: SongTimeNs,
    record_replay: bool,
) {
    let now = Instant::now();
    push_input_edge_timed(
        state,
        source,
        lane,
        input_slot,
        pressed,
        now,
        0,
        now,
        now,
        now,
        event_music_time_ns,
        record_replay,
    );
}

#[inline(always)]
fn push_input_edge_timed(
    state: &mut State,
    source: InputSource,
    lane: Lane,
    input_slot: u32,
    pressed: bool,
    captured_at: Instant,
    captured_host_nanos: u64,
    stored_at: Instant,
    emitted_at: Instant,
    queued_at: Instant,
    event_music_time_ns: SongTimeNs,
    record_replay: bool,
) {
    if lane.index() >= state.num_cols {
        return;
    }
    state.pending_edges.push_back(InputEdge {
        lane,
        input_slot,
        pressed,
        source,
        record_replay,
        captured_at,
        captured_host_nanos,
        stored_at,
        emitted_at,
        queued_at,
        event_music_time_ns,
    });
    if log::log_enabled!(log::Level::Debug) {
        let pending_len = state.pending_edges.len();
        if pending_len >= GAMEPLAY_INPUT_BACKLOG_WARN {
            debug!(
                "Gameplay input queue pressure: pending_edges={}, num_cols={}, music_time={:.3}",
                pending_len,
                state.num_cols,
                current_music_time_s(state)
            );
        }
    }
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> GameplayAction {
    if state.exit_transition.is_some() {
        return GameplayAction::None;
    }
    if let Some(lane) = lane_from_action(ev.action) {
        queue_input_edge(
            state,
            ev.source,
            lane,
            ev.input_slot,
            ev.pressed,
            ev.timestamp,
            ev.timestamp_host_nanos,
            ev.stored_at,
            ev.emitted_at,
        );
        return GameplayAction::None;
    }
    let p2_runtime_player = state.session.p2_runtime_player();
    let p1_menu_active = state.num_players > 1 || !p2_runtime_player;
    let p2_menu_active = state.num_players > 1 || p2_runtime_player;
    match ev.action {
        VirtualAction::p1_start if p1_menu_active => {
            if ev.pressed {
                state.hold_to_exit_key = Some(HoldToExitKey::Start);
                state.hold_to_exit_start = Some(ev.timestamp);
                state.hold_to_exit_aborted_at = None;
            } else if state.hold_to_exit_key == Some(HoldToExitKey::Start) {
                abort_hold_to_exit(state, ev.timestamp);
            }
        }
        VirtualAction::p2_start if p2_menu_active => {
            if ev.pressed {
                state.hold_to_exit_key = Some(HoldToExitKey::Start);
                state.hold_to_exit_start = Some(ev.timestamp);
                state.hold_to_exit_aborted_at = None;
            } else if state.hold_to_exit_key == Some(HoldToExitKey::Start) {
                abort_hold_to_exit(state, ev.timestamp);
            }
        }
        VirtualAction::p1_back if p1_menu_active => {
            if ev.pressed {
                if !state.config.delayed_back {
                    begin_exit_transition(state, ExitTransitionKind::Cancel);
                } else {
                    state.hold_to_exit_key = Some(HoldToExitKey::Back);
                    state.hold_to_exit_start = Some(ev.timestamp);
                    state.hold_to_exit_aborted_at = None;
                }
            } else if state.hold_to_exit_key == Some(HoldToExitKey::Back) {
                abort_hold_to_exit(state, ev.timestamp);
            }
        }
        VirtualAction::p2_back if p2_menu_active => {
            if ev.pressed {
                if !state.config.delayed_back {
                    begin_exit_transition(state, ExitTransitionKind::Cancel);
                } else {
                    state.hold_to_exit_key = Some(HoldToExitKey::Back);
                    state.hold_to_exit_start = Some(ev.timestamp);
                    state.hold_to_exit_aborted_at = None;
                }
            } else if state.hold_to_exit_key == Some(HoldToExitKey::Back) {
                abort_hold_to_exit(state, ev.timestamp);
            }
        }
        _ => {}
    }
    GameplayAction::None
}

#[inline(always)]
pub(super) fn process_input_edges(
    state: &mut State,
    trace_enabled: bool,
    phase_timings: &mut GameplayUpdatePhaseTimings,
    song_clock: SongClockSnapshot,
) {
    if state.pending_edges.is_empty() {
        return;
    }

    let input_log = gameplay_input_log_enabled();
    while let Some(mut edge) = state.pending_edges.pop_front() {
        let lane_idx = edge.lane.index();
        if lane_idx >= state.num_cols {
            if input_log {
                debug!(
                    "GAMEPLAY INPUT EDGE DROP: reason=lane_out_of_range lane={} num_cols={} source={:?} slot={} pressed={}",
                    lane_idx, state.num_cols, edge.source, edge.input_slot, edge.pressed,
                );
            }
            continue;
        }
        let mut event_time_source = "precomputed";
        let mut resolved_from_song_clock = false;
        if song_time_ns_invalid(edge.event_music_time_ns) {
            edge.event_music_time_ns = music_time_ns_from_song_clock(
                song_clock,
                edge.captured_at,
                edge.captured_host_nanos,
            );
            event_time_source = "song_clock";
            resolved_from_song_clock = true;
        }
        if song_time_ns_invalid(edge.event_music_time_ns) {
            if input_log {
                debug!(
                    "GAMEPLAY INPUT EDGE DROP: reason=invalid_song_time lane={} source={:?} slot={} pressed={} captured_host_nanos={} pending={}",
                    lane_idx,
                    edge.source,
                    edge.input_slot,
                    edge.pressed,
                    edge.captured_host_nanos,
                    state.pending_edges.len(),
                );
            }
            continue;
        }
        let lane_was_down = lane_is_pressed(state, lane_idx);
        let slot_was_down = input_slot_lane_is_down(state, edge.lane, edge.source, edge.input_slot);
        let edge_judges_tap = lane_edge_judges_tap(edge.pressed, slot_was_down);
        let edge_judges_lift = lane_edge_judges_lift(edge.pressed, slot_was_down);
        if resolved_from_song_clock
            && !song_clock.mapped_audio
            && should_warn_unmapped_input_clock(edge.event_music_time_ns)
        {
            warn!(
                "GAMEPLAY INPUT CLOCK WARNING: reason=audio_map_unavailable lane={} source={:?} slot={} pressed={} edge_time_s={:.6} song_clock_time_s={:.6} captured_host_nanos={} current_time_s={:.6}",
                lane_idx,
                edge.source,
                edge.input_slot,
                edge.pressed,
                song_time_ns_to_seconds(edge.event_music_time_ns),
                song_time_ns_to_seconds(song_clock.song_time_ns),
                edge.captured_host_nanos,
                current_music_time_s(state),
            );
        }
        if input_log {
            let event_music_time = song_time_ns_to_seconds(edge.event_music_time_ns);
            if resolved_from_song_clock && !song_clock.mapped_audio {
                debug!(
                    "GAMEPLAY INPUT CLOCK FALLBACK: reason=audio_map_unavailable lane={} source={:?} slot={} pressed={} edge_time_s={:.6} song_clock_time_s={:.6} captured_host_nanos={}",
                    lane_idx,
                    edge.source,
                    edge.input_slot,
                    edge.pressed,
                    event_music_time,
                    song_time_ns_to_seconds(song_clock.song_time_ns),
                    edge.captured_host_nanos,
                );
            }
            let processed_at = Instant::now();
            let capture_to_queue_us = elapsed_us_between(edge.queued_at, edge.captured_at);
            let queue_to_process_us = elapsed_us_between(processed_at, edge.queued_at);
            let capture_to_process_us = elapsed_us_between(processed_at, edge.captured_at);
            debug!(
                concat!(
                    "GAMEPLAY INPUT EDGE: lane={} source={:?} slot={} pressed={} ",
                    "lane_was_down={} slot_was_down={} judges_tap={} judges_lift={} ",
                    "time_source={} song_clock_mapped={} edge_time_s={:.6} current_time_s={:.6} ",
                    "capture_queue_us={} queue_process_us={} capture_process_us={} pending={}"
                ),
                lane_idx,
                edge.source,
                edge.input_slot,
                edge.pressed,
                lane_was_down,
                slot_was_down,
                edge_judges_tap,
                edge_judges_lift,
                event_time_source,
                song_clock.mapped_audio,
                event_music_time,
                current_music_time_s(state),
                capture_to_queue_us,
                queue_to_process_us,
                capture_to_process_us,
                state.pending_edges.len(),
            );
        }
        if edge_judges_tap {
            refresh_roll_life_on_step(state, lane_idx, edge.event_music_time_ns);
        }
        integrate_active_hold_to_time(state, lane_idx, edge.event_music_time_ns);
        if edge.record_replay {
            state.replay_edges.push(RecordedLaneEdge {
                lane_index: lane_idx as u8,
                pressed: edge.pressed,
                source: edge.source,
                event_music_time_ns: edge.event_music_time_ns,
            });
        }
        if trace_enabled {
            let processed_at = Instant::now();
            let capture_to_store_us = elapsed_us_between(edge.stored_at, edge.captured_at);
            let store_to_emit_us = elapsed_us_between(edge.emitted_at, edge.stored_at);
            let emit_to_queue_us = elapsed_us_between(edge.queued_at, edge.emitted_at);
            let capture_to_queue_us = elapsed_us_between(edge.queued_at, edge.captured_at);
            let capture_to_process_us = elapsed_us_between(processed_at, edge.captured_at);
            let queue_to_process_us = elapsed_us_between(processed_at, edge.queued_at);
            state.update_trace.summary_input_latency.record(
                capture_to_store_us,
                store_to_emit_us,
                emit_to_queue_us,
                capture_to_process_us,
                queue_to_process_us,
            );
            if capture_to_process_us >= GAMEPLAY_INPUT_LATENCY_WARN_US {
                debug!(
                    "Gameplay input latency spike: lane={} pressed={} source={:?} capture_store_us={} store_emit_us={} emit_queue_us={} queue_process_us={} capture_queue_us={} capture_process_us={} pending={} now_t={:.3} edge_t={:.3}",
                    lane_idx,
                    edge.pressed,
                    edge.source,
                    capture_to_store_us,
                    store_to_emit_us,
                    emit_to_queue_us,
                    queue_to_process_us,
                    capture_to_queue_us,
                    capture_to_process_us,
                    state.pending_edges.len() + 1,
                    current_music_time_s(state),
                    song_time_ns_to_seconds(edge.event_music_time_ns),
                );
            }
        }

        let state_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        let lane_update =
            update_lane_input_slot(state, edge.lane, edge.source, edge.input_slot, edge.pressed);
        debug_assert_eq!(lane_update.slot_was_down, slot_was_down);
        if let Some(started) = state_started {
            add_elapsed_us(&mut phase_timings.input_state_us, started);
        }

        let press_started =
            lane_press_started(edge.pressed, lane_update.was_down, lane_update.is_down);
        let release_finished =
            lane_release_finished(edge.pressed, lane_update.was_down, lane_update.is_down);
        sync_active_hold_pressed_state(state, lane_idx, lane_update.is_down);

        if press_started {
            state.lane_pressed_since_ns[lane_idx] = Some(edge.event_music_time_ns);
            record_step_calories(state, lane_idx, edge.event_music_time_ns);
            if trace_enabled {
                let started = Instant::now();
                start_receptor_glow_press(state, lane_idx);
                add_elapsed_us(&mut phase_timings.input_glow_us, started);
            } else {
                start_receptor_glow_press(state, lane_idx);
            }
        } else if release_finished {
            state.lane_pressed_since_ns[lane_idx] = None;
            if trace_enabled {
                let started = Instant::now();
                release_receptor_glow(state, lane_idx);
                add_elapsed_us(&mut phase_timings.input_glow_us, started);
            } else {
                release_receptor_glow(state, lane_idx);
            }
        }

        if edge_judges_tap {
            let event_music_time_ns = edge.event_music_time_ns;
            let hit_note = if trace_enabled {
                let started = Instant::now();
                let hit_note = judge_a_tap(state, lane_idx, event_music_time_ns);
                add_elapsed_us(&mut phase_timings.input_judge_us, started);
                hit_note
            } else {
                judge_a_tap(state, lane_idx, event_music_time_ns)
            };
            if trace_enabled {
                let started = Instant::now();
                refresh_roll_life_on_step(state, lane_idx, event_music_time_ns);
                add_elapsed_us(&mut phase_timings.input_roll_us, started);
            } else {
                refresh_roll_life_on_step(state, lane_idx, event_music_time_ns);
            }
            if hit_note {
                if state.tick_mode == TickMode::Hit {
                    queue_preloaded_assist_tick(state, ASSIST_TICK_SFX_PATH);
                }
            } else {
                trigger_receptor_step_pulse(state, lane_idx);
            }
        } else if edge_judges_lift {
            let hit_lift = judge_a_lift(state, lane_idx, edge.event_music_time_ns);
            if hit_lift && state.tick_mode == TickMode::Hit {
                queue_preloaded_assist_tick(state, ASSIST_TICK_SFX_PATH);
            }
        }
    }
}

#[inline(always)]
pub(super) fn tick_visual_effects(state: &mut State, delta_time: f32) {
    for col in 0..state.num_cols {
        if lane_is_pressed(state, col) {
            state.receptor_glow_timers[col] = 0.0;
            state.receptor_glow_press_timers[col] =
                (state.receptor_glow_press_timers[col] - delta_time).max(0.0);
        } else if state.receptor_glow_press_timers[col] > f32::EPSILON {
            if state.receptor_glow_press_timers[col] <= delta_time {
                release_receptor_glow(state, col);
            } else {
                state.receptor_glow_press_timers[col] -= delta_time;
            }
        } else {
            state.receptor_glow_timers[col] =
                (state.receptor_glow_timers[col] - delta_time).max(0.0);
        }
    }
    for timer in &mut state.receptor_bop_timers {
        *timer = (*timer - delta_time).max(0.0);
    }
    if state.toggle_flash_timer > 0.0 {
        state.toggle_flash_timer = (state.toggle_flash_timer - delta_time).max(0.0);
    }
    for player in 0..state.num_players {
        state.players[player]
            .combo_milestones
            .retain_mut(|milestone| {
                milestone.elapsed += delta_time;
                let max_duration = match milestone.kind {
                    ComboMilestoneKind::Hundred => COMBO_HUNDRED_MILESTONE_DURATION,
                    ComboMilestoneKind::Thousand => COMBO_THOUSAND_MILESTONE_DURATION,
                };
                milestone.elapsed < max_duration
            });
    }
    for explosion in &mut state.tap_explosions {
        if let Some(active) = explosion {
            active.elapsed += delta_time;
            if active.duration <= 0.0 || active.elapsed >= active.duration {
                *explosion = None;
            }
        }
    }
    for explosion in &mut state.mine_explosions {
        if let Some(active) = explosion {
            active.elapsed += delta_time;
            if active.duration <= 0.0 || active.elapsed >= active.duration {
                *explosion = None;
            }
        }
    }
    for slot in &mut state.column_flashes {
        if let Some(active) = slot
            && state.total_elapsed_in_screen - active.started_at_screen_s
                >= column_flash_duration(active.grade)
        {
            *slot = None;
        }
    }
    for slot in &mut state.hold_judgments {
        if let Some(render_info) = slot
            && state.total_elapsed_in_screen - render_info.started_at_screen_s
                >= HOLD_JUDGMENT_TOTAL_DURATION
        {
            *slot = None;
        }
    }
    for slot in &mut state.held_miss_judgments {
        if let Some(render_info) = slot
            && state.total_elapsed_in_screen - render_info.started_at_screen_s
                >= HELD_MISS_TOTAL_DURATION
        {
            *slot = None;
        }
    }
}
