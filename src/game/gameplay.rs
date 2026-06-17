use crate::game::parsing::song_lua::{
    SongLuaCapturedActor, SongLuaNoteHideWindow, SongLuaOverlayActor,
};
use deadsync_chart::song::sync_pref_offset;
use deadsync_chart::{ChartData, GameplayChartData, SongData, SyncPref};
use deadsync_core::input::{InputSource, MAX_COLS, MAX_PLAYERS};
use deadsync_core::note::NoteType;
pub(crate) use deadsync_core::song_time::SongTimeNs;
use deadsync_core::timing::beat_to_note_row;
pub(crate) use deadsync_gameplay::VISUAL_MASK_BIT_BIG;
pub(crate) use deadsync_gameplay::song_lua_ease_factor;
#[cfg(test)]
use deadsync_gameplay::step_stats_notefield_width;
pub use deadsync_gameplay::{
    ASSIST_TICK_LOOKAHEAD_MARGIN_SECONDS, AccelEffects, AccelOverrides, ActiveColumnFlash,
    ActiveComboMilestone, ActiveHold, ActiveMineExplosion, ActiveTapExplosion, AppearanceEffects,
    AppearanceOverrides, AutosyncMode, COLUMN_FLASH_JUDGMENT_DURATION, COLUMN_FLASH_MISS_DURATION,
    COMBO_HUNDRED_MILESTONE_DURATION, COMBO_THOUSAND_MILESTONE_DURATION, ChartAttackEffects,
    ColumnCue, ColumnCueColumn, ColumnScrollFlags, ColumnTapJudgment, ComboMilestoneKind,
    DRAW_DISTANCE_AFTER_TARGETS, DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER, DangerFx, ErrorBarText,
    ErrorBarTick, ExitTransition, ExitTransitionKind, FantasticFeedbackOptions,
    FinalizedRowOutcome, GAMEPLAY_INPUT_BACKLOG_WARN, GIVE_UP_ABORT_TEXT_SECONDS, GameplayAction,
    GameplayAudioCommand, GameplayAudioSnapshot, GameplayConfig, GameplayExit, GameplayFailType,
    GameplayMiniIndicatorData, GameplayMusicCut, GameplayNoteskinData, GameplayNoteskinEffects,
    GameplayReceptorGlowBehavior, GameplayReceptorStepBehavior, GameplayStreamClockSnapshot,
    GameplayTurnOption, GameplayTween, GameplayViewport, HELD_MISS_TOTAL_DURATION,
    HOLD_JUDGMENT_TOTAL_DURATION, HOLDS_MASK_BIT_FLOORED, HOLDS_MASK_BIT_HOLDS_TO_ROLLS,
    HOLDS_MASK_BIT_NO_ROLLS, HOLDS_MASK_BIT_PLANTED, HOLDS_MASK_BIT_TWISTER, HealthState,
    HeldMissRenderInfo, HoldJudgmentRenderInfo, HoldToExitKey, INITIAL_HOLD_LIFE,
    INSERT_MASK_BIT_BIG, INSERT_MASK_BIT_BMRIZE, INSERT_MASK_BIT_ECHO, INSERT_MASK_BIT_MINES,
    INSERT_MASK_BIT_QUICK, INSERT_MASK_BIT_SKIPPY, INSERT_MASK_BIT_STOMP, INSERT_MASK_BIT_WIDE,
    JudgmentRenderInfo, LeadInTiming, MINE_EXPLOSION_DURATION, MINI_PERCENT_MAX, MINI_PERCENT_MIN,
    MineJudgmentRenderInfo, MiniAttackMode, NoteCountStat, OffsetIndicatorText, PerspectiveEffects,
    PerspectiveOverrides, PlayerRowScanState, RECEPTOR_GLOW_DURATION, RECEPTOR_STEP_WINDOWS,
    RECEPTOR_Y_OFFSET_FROM_CENTER, RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE, REMOVE_MASK_BIT_LITTLE,
    REMOVE_MASK_BIT_NO_FAKES, REMOVE_MASK_BIT_NO_HANDS, REMOVE_MASK_BIT_NO_HOLDS,
    REMOVE_MASK_BIT_NO_JUMPS, REMOVE_MASK_BIT_NO_LIFTS, REMOVE_MASK_BIT_NO_MINES,
    REMOVE_MASK_BIT_NO_QUADS, REPLAY_EDGE_RATE_PER_SEC, RecordedLaneEdge, ReplayInputEdge,
    ReplayOffsetSnapshot, RowEntry, RowGrid, SPACING_PERCENT_MAX, SPACING_PERCENT_MIN,
    ScrollEffects, ScrollOverrides, ScrollReverseOptions, TAP_EXPLOSION_WINDOWS,
    TOGGLE_FLASH_DURATION, TOGGLE_FLASH_FADE_START, TurnRng, VisibilityEffects,
    VisibilityOverrides, VisualEffects, VisualOverrides, active_hold_counts_as_pressed,
    active_hold_is_engaged, advance_judged_row_cursor, apply_echo_insert, apply_hyper_shuffle,
    apply_insert_intelligent_taps, apply_mines_insert, apply_stomp_insert,
    apply_super_shuffle_taps, apply_turn_options, apply_turn_permutation,
    apply_uncommon_masks_with_masks, apply_wide_insert, approach_attack_mini_percent_to_target,
    approach_attack_value, approach_f32, assist_clap_cursor_for_row,
    assist_lookahead_music_horizon_seconds, assist_row_no_offset_for_timing,
    attack_mini_target_percent, autoplay_random_offset_music_ns_for_window, build_assist_clap_rows,
    build_column_cues_for_player, build_note_count_stats, build_row_entry, build_row_grids,
    cell_has_any_note, cell_has_nonfake_note, closest_lane_note_ns, collect_edge_judge_indices,
    column_cue_is_mine, column_flash_duration, column_scroll_dirs_for_flags,
    completed_row_final_judgment, completed_row_flash_note_indices_and_judgment,
    compute_end_times_ns, convert_tap_row_to_mines, convert_taps_to_holds,
    count_held_tracks_at_row, count_nonempty_tracks_at_row, count_rescore_tracks_on_row,
    count_tap_or_hold_tracks_at_row, count_tap_tracks_at_row, counts_for_early_rescore,
    crossed_mine_bounds_ns, crossed_mine_held_start_time, danger_fx_rgba, danger_health_state,
    draw_distance_after_targets, draw_distance_before_targets, effective_mini_percent,
    enforce_max_simultaneous_notes, error_bar_average_offset_s, error_bar_long_term_offset_s,
    error_bar_push_tick, error_bar_window_ix, exit_total_seconds, exit_transition_alpha,
    finalized_row_outcome_for_cached_row, finalized_row_outcome_for_entry,
    first_nonempty_track_at_row, first_row_entry_index_at_or_after_time, first_tap_track_at_row,
    first_time_index_at_or_after, gameplay_exit_for_kind, grade_to_window, hold_to_exit_seconds,
    input_queue_cap, is_hold_body_at_row, lane_edge_judges_lift, lane_edge_judges_tap,
    lane_edge_matches_note_type, lane_note_window_bounds_ns, lane_note_window_bounds_rows,
    lane_press_started, lane_release_finished, late_note_resolution_window_ns, local_player_col,
    max_step_distance_ns, mine_window_bounds_ns, mini_value_for_percent,
    missed_note_cutoff_row_for_timing, music_time_from_stream_position,
    next_ready_row_in_lookahead, note_has_displayable_hold, note_tracks_held_miss,
    notes_row_sorted, player_draw_scale_for_mini, player_row_scan_state, player_rows,
    quantization_index_from_beat, recent_step_tracks, remove_cell_notes, replay_edge_cap,
    row_entry_for_cached_row, row_entry_index_for_cached_row, row_final_grade_hides_note,
    scroll_receptor_y, scroll_reverse_percent_for_column, scroll_reverse_scale_for_column,
    set_added_mine_note, set_added_tap_note, song_audio_end_time_ns, sort_player_notes,
    spacing_multiplier_for_percent, stage_music_cut, step_search_row_bounds, stomp_mirror_track,
    suppress_final_bad_rescore_visual, tap_judgment_uses_bright_explosion_for_options,
    timing_row_floor, timing_row_nearest, toggle_flash_alpha, track_held_miss_window_for_player,
    track_range_has_any_note, trigger_combo_milestone, turn_seed_for_song,
    update_danger_fx_for_health, visible_notefield_time_ns,
};
use deadsync_gameplay::{
    StepStatsPlayStyle, step_stats_density_graph_width as gameplay_step_stats_density_graph_width,
    step_stats_upper_density_graph_width,
};
use deadsync_input::InputEdge;
use deadsync_profile as profile_data;
use deadsync_profile::TimingTickMode as TickMode;
use deadsync_rules::combo::{
    self as combo_rules, ComboState, apply_row_combo_state as apply_rules_row_combo_state,
};
use deadsync_rules::judgment::{
    self, JudgeGrade, Judgment, TimingWindow, judgment_time_error_ms_from_music_ns,
};
#[cfg(test)]
use deadsync_rules::life::{
    LIFE_DECENT, LIFE_GREAT, MAX_REGEN_COMBO_AFTER_MISS, REGEN_COMBO_AFTER_MISS,
};
use deadsync_rules::life::{LIFE_HELD, LIFE_HIT_MINE, LIFE_LET_GO, judge_life_delta};
use deadsync_rules::note::{
    HoldData, HoldResult, MAX_HOLD_LIFE, MineResult, Note, TIMING_WINDOW_SECONDS_HOLD,
    TIMING_WINDOW_SECONDS_ROLL, recompute_player_totals,
};
#[cfg(test)]
use deadsync_rules::note::{HoldLifeAdvance, advance_hold_last_held, advance_hold_life_ns};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::stream::{
    StreamSegment, measure_densities, stream_sequences_threshold, zmod_stream_totals_full_measures,
};
use deadsync_rules::timing::{BeatInfoCache, TimingData, TimingProfile, TimingProfileNs};
use deadsync_simfile::timing::rssp_timing_segments_from_deadsync;
use log::{debug, info, trace, warn};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "gameplay/attacks.rs"]
mod attacks;
#[path = "gameplay/autoplay.rs"]
mod autoplay;
#[path = "gameplay/autosync.rs"]
mod autosync;
#[path = "gameplay/clock.rs"]
mod clock;
#[path = "gameplay/controls.rs"]
mod controls;
#[path = "gameplay/display.rs"]
mod display;
#[path = "gameplay/holds.rs"]
mod holds;
#[path = "gameplay/input.rs"]
mod input;
#[path = "gameplay/judging.rs"]
mod judging;
#[path = "gameplay/life.rs"]
mod life;
#[path = "gameplay/note_result.rs"]
mod note_result;
#[path = "gameplay/offset.rs"]
mod offset;
#[path = "gameplay/rows.rs"]
mod rows;
#[path = "gameplay/stats.rs"]
mod stats;
#[path = "gameplay/time.rs"]
mod time;

pub(crate) use self::attacks::song_lua_compile_context;
#[cfg(test)]
use self::attacks::song_lua_ease_window_value;
use self::attacks::{
    AttackMaskWindow, SongLuaEaseMaskWindow, apply_chart_attacks_transforms,
    base_appearance_effects, begin_outro_attack_clear, build_attack_mask_windows_for_player,
    build_song_lua_runtime_windows, effective_visual_mask_for_player, player_changes_chart,
    refresh_active_attack_masks,
};
pub use self::attacks::{
    GameplayCompiledSongLua, GameplaySongLuaData, GameplaySongLuaLayer,
    SongLuaColumnOffsetWindowRuntime, SongLuaOverlayEaseWindowRuntime,
    SongLuaOverlayMessageRuntime, SongLuaVisualLayerRuntime,
    active_chart_attack_effects_for_player, effective_accel_effects_for_player,
    effective_appearance_effects_for_player, effective_mini_percent_for_player,
    effective_perspective_effects_for_player, effective_scroll_effects_for_player,
    effective_scroll_speed_for_player, effective_spacing_multiplier_for_player,
    effective_visibility_effects_for_player, effective_visual_effects_for_player,
};
#[cfg(test)]
use self::attacks::{
    build_song_lua_column_offset_windows_for_player, build_song_lua_constant_windows_for_player,
    build_song_lua_ease_windows_for_player, build_song_lua_overlay_ease_windows, parse_attack_mods,
    parse_song_lua_runtime_mods, turn_option_bits,
};
#[cfg(test)]
use self::autoplay::live_autoplay_enabled_from_flags;
use self::autoplay::{autoplay_blocks_scoring, live_autoplay_enabled, run_autoplay, run_replay};
use self::autosync::apply_autosync_for_row_hits;
pub use self::clock::{
    DisplayClockDiagEvent, DisplayClockDiagEventKind, DisplayClockHealth,
    collect_display_clock_stutter_diag_events, display_clock_health,
    display_clock_stutter_diag_trigger_seq,
};
use self::clock::{
    DisplayClockDiagRing, FrameStableDisplayClock, SongClockSnapshot, current_song_clock_snapshot,
    frame_stable_display_music_time_ns, music_time_ns_from_song_clock,
};
pub use self::controls::{
    RawKeyAction, autosync_mode_status_line, handle_queued_raw_key, sync_queued_raw_modifiers,
    timing_tick_status_line,
};
#[cfg(test)]
use self::controls::{next_tick_mode, tick_mode_status_line};
#[cfg(test)]
use self::display::effective_ex_score_inputs;
use self::display::{
    capture_failed_ex_score_inputs, record_current_combo_window_count, record_display_window_counts,
};
pub use self::display::{
    display_carry_for_player, display_ex_score_percent, display_gameplay_ex_score_percent,
    display_gameplay_hard_ex_score_percent, display_gameplay_itg_score_percent,
    display_hard_ex_score_percent, display_itg_score_percent, display_judgment_count,
    display_live_timing_stats, display_predictive_itg_score_percent, display_totals_for_player,
    display_window_counts,
};
pub(crate) use self::display::{display_ex_score_data, display_scored_ex_score_data};
use self::holds::{begin_hold_life_decay, start_active_hold, update_active_holds};
use self::holds::{
    handle_hold_let_go, handle_hold_success, integrate_active_hold_to_time,
    refresh_roll_life_on_step,
};
pub use self::input::{
    handle_input, queue_input_edge, receptor_glow_visual_for_col, replay_capture_enabled,
    set_replay_capture_enabled,
};
use self::input::{
    lane_is_pressed, process_input_edges, sync_active_hold_pressed_state, tick_visual_effects,
    trigger_receptor_glow_pulse, trigger_receptor_score_pulse,
};
#[cfg(test)]
use self::input::{trigger_receptor_step_pulse, update_lane_input_slot};
use self::judging::{
    PlayerJudgmentTiming, build_final_note_hit_judgment, build_player_judgment_timing,
    effective_player_global_offset_seconds, note_hit_eval, player_largest_tap_window_ns,
};
pub use self::judging::{player_blue_window_ms, player_fa_plus_window_s};
pub use self::life::course_stage_life_submit_eligible;
use self::life::{
    all_joined_players_failed, apply_life_change, init_course_submit_life, is_player_dead,
    is_state_dead,
};
use self::note_result::{register_provisional_early_result, set_final_note_result};
use self::offset::update_offset_adjust_hold;
#[cfg(test)]
use self::offset::{
    apply_global_offset_delta, apply_song_offset_delta, mutate_timing_arc,
    refresh_timing_after_offset_change,
};
#[cfg(test)]
use self::rows::finalize_row_judgment;
use self::rows::update_judged_rows;
pub use self::stats::{
    CourseDisplayTotals, course_display_carry_from_state, course_display_totals_for_chart,
    score_invalid_reason_lines_for_chart, stream_segments_for_results,
};
use self::stats::{mini_indicator_mode, needs_stream_data, target_score_setting_percent};
use self::time::{
    INVALID_SONG_TIME_NS, current_music_time_s, normalized_song_rate, scaled_song_delta_ns,
    scaled_song_time_ns, song_time_ns_add_seconds, song_time_ns_delta_seconds,
    song_time_ns_span_seconds,
};
pub(crate) use self::time::{
    song_time_ns_from_seconds, song_time_ns_invalid, song_time_ns_to_seconds,
};

// Simply Love ScreenGameplay in/default.lua keeps intro cover actors alive for 2.0s.
pub const TRANSITION_IN_DURATION: f32 = 2.0;
/// SL/zmod parity: when re-entering Gameplay as a restart, skip the splode +
/// stage-text in-transition (`ScreenGameplay in/default.lua` calls
/// `Hide` immediately when `SL.Global.GameplayReloadCheck` is true). Use a
/// short fade-from-black so the new gameplay frame doesn't pop in.
pub const TRANSITION_IN_RESTART_DURATION: f32 = 0.2;
// Simply Love ScreenGameplay out.lua: sleep(0.5), linear(1.0).
pub const TRANSITION_OUT_DELAY: f32 = 0.5;
pub const TRANSITION_OUT_FADE_DURATION: f32 = 1.0;
pub const TRANSITION_OUT_DURATION: f32 = TRANSITION_OUT_DELAY + TRANSITION_OUT_FADE_DURATION;
const MAX_ACTIVE_INPUT_SLOTS: usize = 128;
// ITGmania _fallback and Simply Love keep mine hits from incrementing miss combo.
const MINE_HIT_INCREMENTS_MISS_COMBO: bool = false;
const OFFSET_ADJUST_STEP_SECONDS: f32 = 0.001;
const OFFSET_ADJUST_REPEAT_DELAY: Duration = Duration::from_millis(300);
const OFFSET_ADJUST_REPEAT_INTERVAL: Duration = Duration::from_millis(50);
const M_MOD_HIGH_CAP: f32 = 600.0;
pub const SCOREBOX_NUM_ENTRIES: usize = 5;

// ITGmania _fallback defaults this off, and Simply Love relies on that dance parity.
const COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO: bool = false;

const ASSIST_TICK_SFX_PATH: &str = "assets/sounds/assist_tick.ogg";
pub const AUTOSYNC_OFFSET_SAMPLE_COUNT: usize = 24;
const AUTOSYNC_STDDEV_MAX_SECONDS: f32 = 0.03;
const GAMEPLAY_TRACE_SUMMARY_INTERVAL_S: f32 = 1.0;
const GAMEPLAY_TRACE_SLOW_FRAME_US: u32 = 4_000;
const GAMEPLAY_TRACE_PHASE_SPIKE_US: u32 = 1_000;
const GAMEPLAY_INPUT_LATENCY_WARN_US: u32 = 2_000;

#[inline(always)]
pub(crate) fn scroll_effects_from_option(scroll: profile_data::ScrollOption) -> ScrollEffects {
    use profile_data::ScrollOption;
    ScrollEffects::from_flags(
        scroll.contains(ScrollOption::Reverse),
        scroll.contains(ScrollOption::Split),
        scroll.contains(ScrollOption::Alternate),
        scroll.contains(ScrollOption::Cross),
        scroll.contains(ScrollOption::Centered),
    )
}

#[inline(always)]
fn perspective_effects_from_profile(perspective: profile_data::Perspective) -> PerspectiveEffects {
    let (tilt, skew) = perspective.tilt_skew();
    PerspectiveEffects { tilt, skew }
}

#[inline(always)]
fn effective_mini_value_with_visual_mask(
    profile: &profile_data::Profile,
    visual_mask: u16,
    mini_percent: f32,
) -> f32 {
    mini_value_for_percent(
        mini_percent,
        profile.mini_percent as f32,
        (visual_mask & VISUAL_MASK_BIT_BIG) != 0,
    )
}

#[inline(always)]
fn effective_mini_value(profile: &profile_data::Profile) -> f32 {
    let visual_mask = profile.visual_effects_active_mask.bits();
    effective_mini_value_with_visual_mask(profile, visual_mask, profile.mini_percent as f32)
}

#[inline(always)]
fn player_draw_scale_for_tilt_with_visual_mask(
    tilt: f32,
    profile: &profile_data::Profile,
    visual_mask: u16,
    mini_percent: f32,
) -> f32 {
    let mini = effective_mini_value_with_visual_mask(profile, visual_mask, mini_percent);
    player_draw_scale_for_mini(tilt, mini)
}

#[inline(always)]
fn player_draw_scale_with_visual_mask(
    profile: &profile_data::Profile,
    visual_mask: u16,
    mini_percent: f32,
) -> f32 {
    player_draw_scale_for_tilt_with_visual_mask(
        profile.perspective.tilt_skew().0,
        profile,
        visual_mask,
        mini_percent,
    )
}

#[inline(always)]
fn player_draw_scale(profile: &profile_data::Profile) -> f32 {
    let visual_mask = profile.visual_effects_active_mask.bits();
    player_draw_scale_with_visual_mask(profile, visual_mask, 0.0)
}

#[inline(always)]
pub fn row_hides_completed_note(state: &State, player: usize, row_index: usize) -> bool {
    finalized_row_outcome_for_cached_row(
        &state.row_entries,
        &state.row_map_cache[player],
        row_index,
    )
    .is_some_and(|outcome| row_final_grade_hides_note(outcome.final_grade))
}

#[inline(always)]
fn song_lua_hides_note_visual(state: &State, player: usize, column: usize, beat: f32) -> bool {
    const EPS: f32 = 1.0e-4;
    let local_col = if state.cols_per_player == 0 {
        column
    } else {
        column % state.cols_per_player
    };
    state.song_lua_note_hides[player].iter().any(|window| {
        window.column == local_col
            && beat + EPS >= window.start_beat
            && beat <= window.end_beat + EPS
    })
}

#[inline(always)]
fn trigger_completed_row_tap_explosions(state: &mut State, player: usize, row_index: usize) {
    let Some((flash_note_indices, flash_count, flash_judgment)) = ({
        let Some(row_entry) =
            row_entry_for_cached_row(&state.row_entries, &state.row_map_cache[player], row_index)
        else {
            return;
        };
        completed_row_flash_note_indices_and_judgment(&state.notes, row_entry)
    }) else {
        return;
    };

    for &note_index in &flash_note_indices[..flash_count] {
        let note = &state.notes[note_index];
        let column = note.column;
        if song_lua_hides_note_visual(state, player, column, note.beat) {
            if let Some(window_key) = grade_to_window(flash_judgment.grade) {
                trigger_receptor_score_pulse(state, column, window_key);
            }
            continue;
        }
        trigger_tap_judgment_explosion(state, player, column, &flash_judgment);
    }
}

fn gameplay_turn_option_from_profile(turn: profile_data::TurnOption) -> GameplayTurnOption {
    match turn {
        profile_data::TurnOption::None => GameplayTurnOption::None,
        profile_data::TurnOption::Mirror => GameplayTurnOption::Mirror,
        profile_data::TurnOption::LRMirror => GameplayTurnOption::LRMirror,
        profile_data::TurnOption::UDMirror => GameplayTurnOption::UDMirror,
        profile_data::TurnOption::Left => GameplayTurnOption::Left,
        profile_data::TurnOption::Right => GameplayTurnOption::Right,
        profile_data::TurnOption::Shuffle => GameplayTurnOption::Shuffle,
        profile_data::TurnOption::Blender => GameplayTurnOption::Blender,
        profile_data::TurnOption::Random => GameplayTurnOption::Random,
    }
}

#[inline(always)]
fn assist_row_no_offset(state: &State, music_time: f32) -> i32 {
    assist_row_no_offset_ns(state, song_time_ns_from_seconds(music_time))
}

#[inline(always)]
fn assist_row_no_offset_ns(state: &State, music_time_ns: SongTimeNs) -> i32 {
    assist_row_no_offset_for_timing(&state.timing, state.global_offset_seconds, music_time_ns)
}

fn apply_uncommon_masks_for_player(
    notes: &mut Vec<Note>,
    player_profile: &profile_data::Profile,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
) {
    apply_uncommon_masks_with_masks(
        notes,
        player_profile.insert_active_mask.bits(),
        player_profile.remove_active_mask.bits(),
        player_profile.holds_active_mask.bits(),
        timing_player,
        col_offset,
        cols,
        &[],
        None,
        player,
    );
}

#[inline(always)]
fn has_uncommon_masks(profile: &profile_data::Profile) -> bool {
    !profile.insert_active_mask.is_empty()
        || !profile.remove_active_mask.is_empty()
        || !profile.holds_active_mask.is_empty()
}

fn apply_uncommon_chart_transforms(
    notes: &mut Vec<Note>,
    note_ranges: &mut [(usize, usize); MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    player_profiles: &[profile_data::Profile; MAX_PLAYERS],
    timing_players: &[Arc<TimingData>; MAX_PLAYERS],
) {
    if num_players == 0
        || !player_profiles
            .iter()
            .take(num_players)
            .any(has_uncommon_masks)
    {
        return;
    }

    let mut transformed = Vec::with_capacity(notes.len());
    let mut transformed_ranges = [(0usize, 0usize); MAX_PLAYERS];

    for player in 0..num_players {
        let (start, end) = note_ranges[player];
        let slice_end = end.min(notes.len());
        let slice_start = start.min(slice_end);
        let out_start = transformed.len();
        if !has_uncommon_masks(&player_profiles[player]) {
            transformed.extend_from_slice(&notes[slice_start..slice_end]);
            transformed_ranges[player] = (out_start, transformed.len());
            continue;
        }
        let mut player_notes = notes[slice_start..slice_end].to_vec();
        apply_uncommon_masks_for_player(
            &mut player_notes,
            &player_profiles[player],
            timing_players[player].as_ref(),
            player.saturating_mul(cols_per_player),
            cols_per_player,
            player,
        );

        transformed.extend(player_notes);
        transformed_ranges[player] = (out_start, transformed.len());
    }

    if num_players == 1 {
        transformed_ranges[1] = transformed_ranges[0];
    }

    *notes = transformed;
    *note_ranges = transformed_ranges;
}

/// Lead-in/out fade applied to every crossover cue.
const CROSSOVER_CUE_FADE_SECONDS: f32 = 0.075;

/// Lowest matching lane wins so results are deterministic; `pos % 4` keeps it
/// working for the second pad of doubles, not just the left pad.
fn crossover_arrow_col(column_mask: u8, want_outer: bool) -> Option<usize> {
    let mut m = column_mask;
    while m != 0 {
        let c = m.trailing_zeros() as usize;
        m &= m - 1;
        let pos = c % 4;
        let is_outer = pos == 0 || pos == 3;
        if is_outer == want_outer {
            return Some(c);
        }
    }
    None
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct CrossoverRow {
    beat: f32,
    /// Occupancy bitmask of the foot-bearing columns this row (0-indexed).
    column_mask: u8,
    /// Whether the parity solver flagged this row as a crossover.
    crossover: bool,
    /// Kept raw (rather than pre-filtered) so the cue builder can honour the
    /// per-player "include brackets" toggle.
    bracket: bool,
}

impl CrossoverRow {
    /// A bracket crossover only counts when the player opts brackets in.
    #[inline]
    fn is_active_crossover(&self, include_brackets: bool) -> bool {
        self.crossover && (include_brackets || !self.bracket)
    }
}

impl From<&rssp::RowAnnotation> for CrossoverRow {
    #[inline]
    fn from(anno: &rssp::RowAnnotation) -> Self {
        Self {
            beat: anno.beat,
            column_mask: anno.column_mask,
            crossover: anno.row_tech.crossovers > 0,
            bracket: anno.foot_count() > 1,
        }
    }
}

/// Re-encodes notes into `rssp`'s parity row format. Keyed by note row so hold
/// tails (`3`) merge with coincident taps and foot occupancy stays correct.
fn build_crossover_rows<const LANES: usize>(
    notes: &[Note],
    note_range: (usize, usize),
    col_start: usize,
) -> (Vec<[u8; LANES]>, Vec<f32>, Vec<usize>) {
    use std::collections::BTreeMap;
    let (start, end) = note_range;
    let mut rows: BTreeMap<usize, ([u8; LANES], f32)> = BTreeMap::new();
    for note in &notes[start..end] {
        if note.column < col_start || note.column - col_start >= LANES {
            continue;
        }
        if note.is_fake {
            continue;
        }
        let lane = note.column - col_start;
        let ch = match note.note_type {
            NoteType::Tap | NoteType::Lift => b'1',
            NoteType::Hold => b'2',
            NoteType::Roll => b'4',
            NoteType::Mine | NoteType::Fake => continue,
        };
        let entry = rows
            .entry(note.row_index)
            .or_insert(([b'0'; LANES], note.beat));
        entry.0[lane] = ch;
        if let Some(hold) = note.hold.as_ref() {
            let tail = rows
                .entry(hold.end_row_index)
                .or_insert(([b'0'; LANES], hold.end_beat));
            if tail.0[lane] == b'0' {
                tail.0[lane] = b'3';
            }
        }
    }
    let mut row_arrays = Vec::with_capacity(rows.len());
    let mut row_to_beat = Vec::with_capacity(rows.len());
    let mut row_indices = Vec::with_capacity(rows.len());
    for (row_index, (arr, beat)) in rows {
        row_arrays.push(arr);
        row_to_beat.push(beat);
        row_indices.push(row_index);
    }
    (row_arrays, row_to_beat, row_indices)
}

/// Uses the player's base `TimingData` (not rate-scaled) so cue times share the
/// mine-cue frame; rate scaling is reapplied at render time.
#[allow(clippy::too_many_arguments)]
fn build_crossover_cues_from_annotations(
    annos: &[CrossoverRow],
    timing_player: &TimingData,
    col_start: usize,
    duration_ms: u16,
    quantization: u8,
    include_brackets: bool,
    first_visible_time: f32,
) -> Vec<ColumnCue> {
    let arrow_time =
        |beat: f32| -> f32 { song_time_ns_to_seconds(timing_player.get_time_for_beat_ns(beat)) };
    build_crossover_cues_core(
        annos,
        arrow_time,
        col_start,
        duration_ms,
        quantization,
        include_brackets,
        first_visible_time,
    )
}

/// Split from the public entry so the beat→seconds mapping can be faked in
/// unit tests without a full `TimingData`.
#[allow(clippy::too_many_arguments)]
fn build_crossover_cues_core(
    annos: &[CrossoverRow],
    arrow_time: impl Fn(f32) -> f32,
    col_start: usize,
    duration_ms: u16,
    quantization: u8,
    include_brackets: bool,
    first_visible_time: f32,
) -> Vec<ColumnCue> {
    if annos.len() < 2 {
        return Vec::new();
    }
    let duration = f32::from(duration_ms) / 1000.0;
    let fade = CROSSOVER_CUE_FADE_SECONDS;
    let quant = if quantization == 0 {
        1.0
    } else {
        f32::from(quantization)
    };
    let spacing_threshold = 4.0 / quant + 0.001;

    let mut cues: Vec<ColumnCue> = Vec::new();
    for i in 1..annos.len() {
        let current = &annos[i];
        let prev = &annos[i - 1];
        // Consecutive crossovers: `current` is a scooby already emitted while
        // processing `prev`, so skip it here.
        if !current.is_active_crossover(include_brackets)
            || prev.is_active_crossover(include_brackets)
        {
            continue;
        }
        let next = annos.get(i + 1);
        let next_next = annos.get(i + 2);
        let is_scooby = next.is_some_and(|a| a.is_active_crossover(include_brackets));
        let first_condition = current.beat - prev.beat <= spacing_threshold;
        let second_condition = next.is_some_and(|n| n.beat - current.beat <= spacing_threshold);
        let third_condition = match (next, next_next) {
            (Some(n), Some(nn)) => nn.beat - n.beat <= spacing_threshold,
            _ => false,
        };
        if !(first_condition || second_condition || third_condition) {
            continue;
        }
        let (Some(prev_col), Some(curr_col)) = (
            crossover_arrow_col(prev.column_mask, false),
            crossover_arrow_col(current.column_mask, true),
        ) else {
            continue;
        };
        let prev_arrow_time = arrow_time(prev.beat);
        let cur_arrow_time = arrow_time(current.beat);
        let mut columns = vec![
            ColumnCueColumn {
                column: col_start + curr_col,
                is_mine: false,
            },
            ColumnCueColumn {
                column: col_start + prev_col,
                is_mine: false,
            },
        ];
        let mut start_time = prev_arrow_time - duration;
        let mut cue_duration = duration + fade;
        // Large gap before the crossover: keep the cue lit until the crossover
        // actually happens, not just until the arrow before it.
        if !first_condition {
            cue_duration += cur_arrow_time - prev_arrow_time;
        }
        if is_scooby
            && let Some(next_anno) = next
            && let Some(next_col) = crossover_arrow_col(next_anno.column_mask, true)
        {
            columns.push(ColumnCueColumn {
                column: col_start + next_col,
                is_mine: true,
            });
        }
        // Keep consecutive cues from overlapping.
        if let Some(last) = cues.last() {
            let prev_end = last.start_time + last.duration;
            if start_time < prev_end {
                let duration_difference = prev_end - start_time;
                start_time = prev_end - fade;
                cue_duration = cue_duration - duration_difference + fade;
            }
        }
        cues.push(ColumnCue {
            start_time,
            duration: cue_duration,
            columns,
        });
    }

    // Without this guard, a first crossover deep in the song gets pulled
    // earlier and over-inflated — a spurious, over-long first cue. Only cues
    // already due before t=0 should absorb the pre-roll, hence the
    // `start_time <= 0` gate.
    if first_visible_time < 0.0
        && let Some(first) = cues.first_mut()
        && first.start_time <= 0.0
    {
        first.duration -= first_visible_time;
        first.start_time += first_visible_time;
    }
    cues
}

/// Bails on non-4/8-panel layouts because `rssp` parity only models those.
#[allow(clippy::too_many_arguments)]
fn build_crossover_cues_for_player(
    notes: &[Note],
    note_range: (usize, usize),
    timing_segments: &deadsync_rules::timing::TimingSegments,
    timing_player: &TimingData,
    cols_per_player: usize,
    col_start: usize,
    duration_ms: u16,
    quantization: u8,
    include_brackets: bool,
    first_visible_time: f32,
) -> Vec<ColumnCue> {
    let (start, end) = note_range;
    if start >= end {
        return Vec::new();
    }
    let rssp_segments = rssp_timing_segments_from_deadsync(timing_segments);
    let rssp_timing = rssp::timing::timing_data_from_segments(0.0, 0.0, &rssp_segments);
    let annos: Vec<CrossoverRow> = match cols_per_player {
        4 => {
            let (rows, row_to_beat, _row_indices) =
                build_crossover_rows::<4>(notes, note_range, col_start);
            let Some(mut scratch) = rssp::step_parity::timing_rows_scratch::<4>() else {
                return Vec::new();
            };
            rssp::step_parity::annotate_timing_rows::<4>(
                &rows,
                &row_to_beat,
                &rssp_timing,
                &mut scratch,
            )
        }
        8 => {
            let (rows, row_to_beat, _row_indices) =
                build_crossover_rows::<8>(notes, note_range, col_start);
            let Some(mut scratch) = rssp::step_parity::timing_rows_scratch::<8>() else {
                return Vec::new();
            };
            rssp::step_parity::annotate_timing_rows::<8>(
                &rows,
                &row_to_beat,
                &rssp_timing,
                &mut scratch,
            )
        }
        _ => return Vec::new(),
    }
    .iter()
    .map(CrossoverRow::from)
    .collect();
    build_crossover_cues_from_annotations(
        &annos,
        timing_player,
        col_start,
        duration_ms,
        quantization,
        include_brackets,
        first_visible_time,
    )
}

/// Reduces one `rssp` row annotation to a single foot placement for the
/// by-foot evaluation scatter: left if any column uses a left foot, right if any
/// uses a right foot, both when a row uses both feet (a jump), or `None` when no
/// foot steps on the row (so the caller can skip it).
#[inline]
fn parity_foot_from_annotation(
    anno: &rssp::RowAnnotation,
) -> Option<deadsync_rules::timing::ScatterFoot> {
    use deadsync_rules::timing::ScatterFoot;
    let mut uses_left = false;
    let mut uses_right = false;
    for &foot in anno.feet() {
        match foot {
            rssp::Foot::LeftHeel | rssp::Foot::LeftToe => uses_left = true,
            rssp::Foot::RightHeel | rssp::Foot::RightToe => uses_right = true,
            rssp::Foot::None => {}
        }
    }
    match (uses_left, uses_right) {
        (true, true) => Some(ScatterFoot::Both),
        (true, false) => Some(ScatterFoot::Left),
        (false, true) => Some(ScatterFoot::Right),
        (false, false) => None,
    }
}

fn foot_parity_map<const LANES: usize>(
    notes: &[Note],
    note_range: (usize, usize),
    col_start: usize,
    rssp_timing: &rssp::timing::TimingData,
) -> std::collections::HashMap<usize, deadsync_rules::timing::ScatterFoot> {
    use std::collections::HashMap;
    let (rows, row_to_beat, row_indices) = build_crossover_rows::<LANES>(notes, note_range, col_start);
    let Some(mut scratch) = rssp::step_parity::timing_rows_scratch::<LANES>() else {
        return HashMap::new();
    };
    let annos = rssp::step_parity::annotate_timing_rows::<LANES>(
        &rows,
        &row_to_beat,
        rssp_timing,
        &mut scratch,
    );
    let mut map = HashMap::with_capacity(annos.len());
    for (anno, &row_index) in annos.iter().zip(row_indices.iter()) {
        if let Some(placement) = parity_foot_from_annotation(anno) {
            map.insert(row_index, placement);
        }
    }
    map
}

/// Per-row left/right/both foot placement from `rssp` parity, keyed by note
/// `row_index`, for the evaluation by-foot scatter. Returns an empty map on
/// non-4/8-panel layouts (the only layouts `rssp` parity models), in which case
/// the scatter falls back to plotting those rows black.
pub fn foot_parity_by_row_for_results(
    state: &State,
    player: usize,
) -> std::collections::HashMap<usize, deadsync_rules::timing::ScatterFoot> {
    use std::collections::HashMap;
    if player >= state.num_players {
        return HashMap::new();
    }
    let cols_per_player = state.cols_per_player;
    let note_range = state.note_ranges[player];
    if note_range.0 >= note_range.1 {
        return HashMap::new();
    }
    let col_start = player.saturating_mul(cols_per_player);
    let timing_segments = &state.gameplay_charts[player].timing_segments;
    let rssp_segments = rssp_timing_segments_from_deadsync(timing_segments);
    let rssp_timing = rssp::timing::timing_data_from_segments(0.0, 0.0, &rssp_segments);
    match cols_per_player {
        4 => foot_parity_map::<4>(&state.notes, note_range, col_start, &rssp_timing),
        8 => foot_parity_map::<8>(&state.notes, note_range, col_start, &rssp_timing),
        _ => HashMap::new(),
    }
}

#[inline(always)]
fn compute_column_scroll_dirs(
    scroll_option: profile_data::ScrollOption,
    num_cols: usize,
) -> [f32; MAX_COLS] {
    use profile_data::ScrollOption;
    column_scroll_dirs_for_flags(
        ColumnScrollFlags {
            reverse: scroll_option.contains(ScrollOption::Reverse),
            split: scroll_option.contains(ScrollOption::Split),
            alternate: scroll_option.contains(ScrollOption::Alternate),
            cross: scroll_option.contains(ScrollOption::Cross),
        },
        num_cols,
    )
}

#[derive(Clone, Copy, Debug, Default)]
struct ExScoreInputs {
    counts: deadsync_rules::timing::WindowCounts,
    counts_10ms: deadsync_rules::timing::WindowCounts,
    holds_held_for_score: u32,
    holds_let_go_for_score: u32,
    rolls_held_for_score: u32,
    rolls_let_go_for_score: u32,
    mines_hit_for_score: u32,
}

type CourseSubmitLife = deadsync_rules::life::LifeMeter;

#[derive(Clone, Debug)]
pub struct PlayerRuntime {
    pub combo: u32,
    pub miss_combo: u32,
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub current_combo_window_counts: deadsync_rules::timing::WindowCounts,
    pub first_fc_attempt_broken: bool,
    pub judgment_counts: judgment::JudgeCounts,
    pub scoring_counts: judgment::JudgeCounts,
    pub last_judgment: Option<JudgmentRenderInfo>,
    pub last_mine_judgment: Option<MineJudgmentRenderInfo>,

    pub life: f32,
    pub combo_after_miss: u32,
    pub is_failing: bool,
    pub fail_time: Option<f32>,
    pub calories_burned: f32,

    pub earned_grade_points: i32,

    pub combo_milestones: Vec<ActiveComboMilestone>,
    pub hands_achieved: u32,
    pub holds_held: u32,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub mines_hit: u32,
    pub mines_hit_for_score: u32,
    pub mines_avoided: u32,
    hands_holding_count_for_stats: i32,
    failed_ex_score_inputs: Option<ExScoreInputs>,
    course_submit_life: Option<CourseSubmitLife>,

    pub life_history: Vec<(f32, f32)>, // (time, life_value)

    pub error_bar_mono_ticks: [Option<ErrorBarTick>; 15],
    pub error_bar_mono_next: usize,
    pub error_bar_color_ticks: [Option<ErrorBarTick>; 10],
    pub error_bar_color_next: usize,
    pub error_bar_color_bar_started_at: Option<f32>,
    pub error_bar_color_flash_early: [Option<f32>; 6],
    pub error_bar_color_flash_late: [Option<f32>; 6],
    pub error_bar_text: Option<ErrorBarText>,
    pub offset_indicator_text: Option<OffsetIndicatorText>,
    pub error_bar_avg_ticks: [Option<ErrorBarTick>; 5],
    pub error_bar_avg_next: usize,
    pub error_bar_avg_bar_started_at: Option<f32>,
    pub error_bar_avg_samples: VecDeque<(f32, f32)>,
    pub error_bar_long_avg_samples: VecDeque<(f32, f32)>,
    pub error_bar_long_avg_total: f32,
    pub error_bar_long_avg_tick: Option<ErrorBarTick>,
    pub error_bar_long_avg_visible: bool,
    pub live_timing_stats: deadsync_rules::timing::LiveTimingStats,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseDisplayCarry {
    // ITGmania keeps the same lifemeter alive between nonstop course songs.
    pub life: f32,
    pub judgment_counts: [u32; 6],
    pub scoring_counts: [u32; 6],
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub current_combo_window_counts: deadsync_rules::timing::WindowCounts,
    pub first_fc_attempt_broken: bool,
    // Canonical FA+ split (15ms) used for EX scoring/evaluation.
    pub window_counts: deadsync_rules::timing::WindowCounts,
    // Canonical 10ms split used for H.EX scoring/evaluation.
    pub window_counts_10ms_blue: deadsync_rules::timing::WindowCounts,
    // Display split used by gameplay counters (legacy 10ms or custom ms option).
    pub window_counts_display_blue: deadsync_rules::timing::WindowCounts,
    pub holds_held: u32,
    pub rolls_held: u32,
    pub mines_avoided: u32,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub mines_hit_for_score: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseDisplayTiming {
    pub elapsed_seconds: f32,
    pub total_seconds: f32,
}

fn init_player_runtime() -> PlayerRuntime {
    PlayerRuntime {
        combo: 0,
        miss_combo: 0,
        full_combo_grade: None,
        current_combo_grade: None,
        current_combo_window_counts: deadsync_rules::timing::WindowCounts::default(),
        first_fc_attempt_broken: false,
        judgment_counts: [0; judgment::JUDGE_GRADE_COUNT],
        scoring_counts: [0; judgment::JUDGE_GRADE_COUNT],
        last_judgment: None,
        last_mine_judgment: None,
        life: 0.5,
        combo_after_miss: 0,
        is_failing: false,
        fail_time: None,
        calories_burned: 0.0,
        earned_grade_points: 0,
        combo_milestones: Vec::new(),
        hands_achieved: 0,
        holds_held: 0,
        holds_held_for_score: 0,
        holds_let_go_for_score: 0,
        rolls_held: 0,
        rolls_held_for_score: 0,
        rolls_let_go_for_score: 0,
        mines_hit: 0,
        mines_hit_for_score: 0,
        mines_avoided: 0,
        hands_holding_count_for_stats: 0,
        failed_ex_score_inputs: None,
        course_submit_life: None,
        life_history: Vec::with_capacity(10000),
        error_bar_mono_ticks: [None; 15],
        error_bar_mono_next: 0,
        error_bar_color_ticks: [None; 10],
        error_bar_color_next: 0,
        error_bar_color_bar_started_at: None,
        error_bar_color_flash_early: [None; 6],
        error_bar_color_flash_late: [None; 6],
        error_bar_text: None,
        offset_indicator_text: None,
        error_bar_avg_ticks: [None; 5],
        error_bar_avg_next: 0,
        error_bar_avg_bar_started_at: None,
        error_bar_avg_samples: VecDeque::with_capacity(64),
        error_bar_long_avg_samples: VecDeque::with_capacity(64),
        error_bar_long_avg_total: 0.0,
        error_bar_long_avg_tick: None,
        error_bar_long_avg_visible: false,
        live_timing_stats: deadsync_rules::timing::LiveTimingStats::default(),
    }
}

#[inline(always)]
fn apply_course_life_carry(player: &mut PlayerRuntime, course_carry: Option<CourseDisplayCarry>) {
    let Some(carry) = course_carry else {
        return;
    };
    if carry.life.is_finite() {
        player.life = carry.life.clamp(0.0, 1.0);
    }
}

#[inline(always)]
fn apply_course_combo_carry(
    player: &mut PlayerRuntime,
    carry_combo_between_songs: bool,
    replay_mode: bool,
    combo_carry: u32,
    course_carry: Option<CourseDisplayCarry>,
) {
    if carry_combo_between_songs && !replay_mode {
        player.combo = combo_carry;
        if let Some(carry) = course_carry {
            if combo_carry > 0 {
                player.full_combo_grade = carry.full_combo_grade;
                player.current_combo_grade = carry.current_combo_grade;
                player.current_combo_window_counts = carry.current_combo_window_counts;
                player.first_fc_attempt_broken = carry.first_fc_attempt_broken;
            } else {
                player.first_fc_attempt_broken =
                    carry.first_fc_attempt_broken || carry.full_combo_grade.is_some();
            }
        }
    } else if course_carry.is_some() {
        player.first_fc_attempt_broken = true;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveInputSlot {
    source: InputSource,
    input_slot: u32,
    lane_mask: u8,
}

const EMPTY_ACTIVE_INPUT_SLOT: ActiveInputSlot = ActiveInputSlot {
    source: InputSource::Keyboard,
    input_slot: 0,
    lane_mask: 0,
};

#[derive(Clone, Copy, Debug, Default)]
struct GameplayUpdatePhaseTimings {
    pre_notes_us: u32,
    autoplay_us: u32,
    input_edges_us: u32,
    input_queue_us: u32,
    input_state_us: u32,
    input_glow_us: u32,
    input_judge_us: u32,
    input_roll_us: u32,
    held_mines_us: u32,
    active_holds_us: u32,
    hold_decay_us: u32,
    visuals_us: u32,
    spawn_arrows_us: u32,
    mine_avoid_us: u32,
    tap_miss_us: u32,
    cull_us: u32,
    judged_rows_us: u32,
    density_us: u32,
    density_sample_us: u32,
    danger_us: u32,
    untracked_us: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct GameplayInputLatencyTrace {
    samples: u32,
    capture_to_store_total_us: u64,
    store_to_emit_total_us: u64,
    emit_to_queue_total_us: u64,
    capture_to_process_total_us: u64,
    queue_to_process_total_us: u64,
    capture_to_store_max_us: u32,
    store_to_emit_max_us: u32,
    emit_to_queue_max_us: u32,
    capture_to_process_max_us: u32,
    queue_to_process_max_us: u32,
}

impl GameplayInputLatencyTrace {
    #[inline(always)]
    fn record(
        &mut self,
        capture_to_store_us: u32,
        store_to_emit_us: u32,
        emit_to_queue_us: u32,
        capture_to_process_us: u32,
        queue_to_process_us: u32,
    ) {
        self.samples = self.samples.saturating_add(1);
        self.capture_to_store_total_us = self
            .capture_to_store_total_us
            .saturating_add(u64::from(capture_to_store_us));
        self.store_to_emit_total_us = self
            .store_to_emit_total_us
            .saturating_add(u64::from(store_to_emit_us));
        self.emit_to_queue_total_us = self
            .emit_to_queue_total_us
            .saturating_add(u64::from(emit_to_queue_us));
        self.capture_to_process_total_us = self
            .capture_to_process_total_us
            .saturating_add(u64::from(capture_to_process_us));
        self.queue_to_process_total_us = self
            .queue_to_process_total_us
            .saturating_add(u64::from(queue_to_process_us));
        self.capture_to_store_max_us = self.capture_to_store_max_us.max(capture_to_store_us);
        self.store_to_emit_max_us = self.store_to_emit_max_us.max(store_to_emit_us);
        self.emit_to_queue_max_us = self.emit_to_queue_max_us.max(emit_to_queue_us);
        self.capture_to_process_max_us = self.capture_to_process_max_us.max(capture_to_process_us);
        self.queue_to_process_max_us = self.queue_to_process_max_us.max(queue_to_process_us);
    }

    #[inline(always)]
    fn avg_us(total_us: u64, samples: u32) -> f32 {
        if samples == 0 {
            0.0
        } else {
            total_us as f32 / samples as f32
        }
    }
}

#[derive(Clone, Debug)]
struct GameplayUpdateTraceState {
    frame_counter: u64,
    summary_elapsed_s: f32,
    summary_frames: u32,
    summary_slow_frames: u32,
    summary_max_total_us: u32,
    summary_max_phase: GameplayUpdatePhaseTimings,
    summary_input_latency: GameplayInputLatencyTrace,
    summary_peak_pending_edges: usize,
    pending_edges_capacity: usize,
    replay_edges_capacity: usize,
    decaying_hold_capacity: usize,
    density_life_capacity: [usize; MAX_PLAYERS],
}

impl Default for GameplayUpdateTraceState {
    fn default() -> Self {
        Self {
            frame_counter: 0,
            summary_elapsed_s: 0.0,
            summary_frames: 0,
            summary_slow_frames: 0,
            summary_max_total_us: 0,
            summary_max_phase: GameplayUpdatePhaseTimings::default(),
            summary_input_latency: GameplayInputLatencyTrace::default(),
            summary_peak_pending_edges: 0,
            pending_edges_capacity: 0,
            replay_edges_capacity: 0,
            decaying_hold_capacity: 0,
            density_life_capacity: [0; MAX_PLAYERS],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameplaySession {
    pub play_style: profile_data::PlayStyle,
    pub player_side: profile_data::PlayerSide,
    pub joined_sides: [bool; MAX_PLAYERS],
    pub active_profile_ids: [Option<String>; MAX_PLAYERS],
    pub tick_mode: TickMode,
}

impl GameplaySession {
    pub fn active_profile_id_for_side(&self, side: profile_data::PlayerSide) -> Option<String> {
        self.active_profile_ids[profile_data::player_side_index(side)].clone()
    }

    #[inline(always)]
    pub const fn side_joined(&self, side: profile_data::PlayerSide) -> bool {
        self.joined_sides[profile_data::player_side_index(side)]
    }

    #[inline(always)]
    pub const fn p2_runtime_player(&self) -> bool {
        profile_data::runtime_player_is_p2(self.play_style, self.player_side)
    }

    #[inline(always)]
    pub const fn runtime_player_side(&self, player_idx: usize) -> profile_data::PlayerSide {
        profile_data::runtime_player_side(self.play_style, self.player_side, player_idx)
    }
}

impl Default for GameplaySession {
    fn default() -> Self {
        Self {
            play_style: profile_data::PlayStyle::Single,
            player_side: profile_data::PlayerSide::P1,
            joined_sides: [true, false],
            active_profile_ids: [None, None],
            tick_mode: TickMode::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplaySessionCommand {
    SetTimingTickMode(TickMode),
}

pub struct State {
    pub song: Arc<SongData>,
    pub charts: [Arc<ChartData>; MAX_PLAYERS],
    pub gameplay_charts: [Arc<GameplayChartData>; MAX_PLAYERS],
    pub num_cols: usize,
    pub cols_per_player: usize,
    pub num_players: usize,
    pub viewport: GameplayViewport,
    pub session: GameplaySession,
    pub config: GameplayConfig,
    audio_commands: Vec<GameplayAudioCommand>,
    session_commands: Vec<GameplaySessionCommand>,
    pub timing: Arc<TimingData>,
    pub timing_players: [Arc<TimingData>; MAX_PLAYERS],
    pub beat_info_cache: BeatInfoCache,
    pub timing_profile: TimingProfile,
    player_judgment_timing: [PlayerJudgmentTiming; MAX_PLAYERS],
    pub notes: Vec<Note>,
    pub note_ranges: [(usize, usize); MAX_PLAYERS],
    pub note_count_stats: [Vec<NoteCountStat>; MAX_PLAYERS],
    pub audio_lead_in_seconds: f32,
    pub audio_stream_position_seconds: f32,
    pub audio_output_delay_seconds: f32,
    pub current_beat: f32,
    pub current_music_time_ns: SongTimeNs,
    pub current_beat_display: f32,
    pub current_music_time_display: f32,
    display_clock: FrameStableDisplayClock,
    display_clock_diag: DisplayClockDiagRing,
    pub lane_note_indices: [Vec<usize>; MAX_COLS],
    // Render candidates are keyed like ITG NoteData rows. Note::row_index is
    // Dead Sync's dense RSSP row and is not comparable to BeatToNoteRow spans.
    pub lane_note_row_indices: [Vec<usize>; MAX_COLS],
    pub lane_hold_indices: [Vec<usize>; MAX_COLS],
    pub row_entry_ranges: [(usize, usize); MAX_PLAYERS],
    pub judged_row_cursor: [usize; MAX_PLAYERS],
    pub note_time_cache_ns: Vec<SongTimeNs>,
    pub hold_end_time_cache_ns: Vec<Option<SongTimeNs>>,
    pub notes_end_time_ns: SongTimeNs,
    pub music_end_time_ns: SongTimeNs,
    audio_end_time_ns: SongTimeNs,
    pub music_rate: f32,
    pub play_mine_sounds: bool,
    pub default_fail_type: GameplayFailType,
    pub global_offset_seconds: f32,
    pub initial_global_offset_seconds: f32,
    pub player_global_offset_shift_seconds: [f32; MAX_PLAYERS],
    pub song_offset_seconds: f32,
    pub initial_song_offset_seconds: f32,
    pub autosync_mode: AutosyncMode,
    pub autosync_offset_samples: [SongTimeNs; AUTOSYNC_OFFSET_SAMPLE_COUNT],
    pub autosync_offset_sample_count: usize,
    pub autosync_standard_deviation: f32,
    pub global_visual_delay_seconds: f32,
    pub player_visual_delay_seconds: [f32; MAX_PLAYERS],
    pub current_music_time_visible_ns: [SongTimeNs; MAX_PLAYERS],
    pub current_music_time_visible: [f32; MAX_PLAYERS],
    pub current_beat_visible: [f32; MAX_PLAYERS],
    pub next_tap_miss_cursor: [usize; MAX_PLAYERS],
    pub next_mine_avoid_cursor: [usize; MAX_PLAYERS],
    pub mine_note_ix: [Vec<usize>; MAX_PLAYERS],
    pub mine_note_time_ns: [Vec<SongTimeNs>; MAX_PLAYERS],
    pub next_mine_ix_cursor: [usize; MAX_PLAYERS],
    pub pending_mine_hit_indices: Vec<usize>,
    pub row_entries: Vec<RowEntry>,
    pub measure_counter_segments: [Vec<StreamSegment>; MAX_PLAYERS],
    pub column_cues: [Vec<ColumnCue>; MAX_PLAYERS],
    pub crossover_cues: [Vec<ColumnCue>; MAX_PLAYERS],
    pub mini_indicator_stream_segments: [Vec<StreamSegment>; MAX_PLAYERS],
    pub mini_indicator_total_stream_measures: [f32; MAX_PLAYERS],
    pub mini_indicator_target_score_percent: [f64; MAX_PLAYERS],
    pub mini_indicator_rival_score_percent: [f64; MAX_PLAYERS],

    // Optimization: Per-player direct row lookup instead of HashMap
    pub row_map_cache: [Vec<u32>; MAX_PLAYERS],
    pub note_row_entry_indices: Vec<u32>,
    // Bit flags per note index:
    // bit0 => same row contains a hold start, bit1 => same row contains a roll start.
    pub tap_row_hold_roll_flags: Vec<u8>,

    pub decaying_hold_indices: Vec<usize>,
    pub hold_decay_active: Vec<bool>,
    pub tap_miss_held_window: Vec<bool>,
    pending_missed_hold_resolution: Vec<bool>,
    pending_missed_hold_indices: Vec<usize>,

    pub players: [PlayerRuntime; MAX_PLAYERS],
    pub hold_judgments: [Option<HoldJudgmentRenderInfo>; MAX_COLS],
    pub held_miss_judgments: [Option<HeldMissRenderInfo>; MAX_COLS],
    pub is_in_freeze: bool,
    pub is_in_delay: bool,

    pub possible_grade_points: [i32; MAX_PLAYERS],
    pub song_completed_naturally: bool,
    pub autoplay_enabled: bool,
    pub autoplay_used: bool,
    pub score_valid: [bool; MAX_PLAYERS],
    score_missed_holds_rolls: [bool; MAX_PLAYERS],
    replay_mode: bool,
    replay_capture_enabled: bool,
    pub course_display_carry: Option<[CourseDisplayCarry; MAX_PLAYERS]>,
    pub course_display_totals: Option<[CourseDisplayTotals; MAX_PLAYERS]>,
    pub course_display_timing: Option<CourseDisplayTiming>,
    pub live_window_counts: [deadsync_rules::timing::WindowCounts; MAX_PLAYERS],
    pub live_window_counts_10ms_blue: [deadsync_rules::timing::WindowCounts; MAX_PLAYERS],
    pub live_window_counts_display_blue: [deadsync_rules::timing::WindowCounts; MAX_PLAYERS],

    pub player_profiles: [profile_data::Profile; MAX_PLAYERS],
    attack_mask_windows: [Vec<AttackMaskWindow>; MAX_PLAYERS],
    song_lua_ease_windows: [Vec<SongLuaEaseMaskWindow>; MAX_PLAYERS],
    pub song_lua_overlays: Vec<SongLuaOverlayActor>,
    // Gameplay-thread song-lua caches built at song load and read every frame by
    // render, so overlay evaluation stays local to each overlay.
    pub song_lua_overlay_eases: Vec<SongLuaOverlayEaseWindowRuntime>,
    pub song_lua_overlay_ease_ranges: Vec<std::ops::Range<usize>>,
    pub song_lua_overlay_events: Vec<Vec<SongLuaOverlayMessageRuntime>>,
    pub song_lua_background_visual_layers: Vec<SongLuaVisualLayerRuntime>,
    pub song_lua_foreground_visual_layers: Vec<SongLuaVisualLayerRuntime>,
    pub song_lua_player_actors: [SongLuaCapturedActor; MAX_PLAYERS],
    pub song_lua_player_events: [Vec<SongLuaOverlayMessageRuntime>; MAX_PLAYERS],
    pub song_lua_song_foreground: SongLuaCapturedActor,
    pub song_lua_song_foreground_events: Vec<SongLuaOverlayMessageRuntime>,
    pub song_lua_hidden_players: [bool; MAX_PLAYERS],
    pub song_lua_note_hides: [Vec<SongLuaNoteHideWindow>; MAX_PLAYERS],
    pub song_lua_column_offsets: [Vec<SongLuaColumnOffsetWindowRuntime>; MAX_PLAYERS],
    pub song_lua_screen_width: f32,
    pub song_lua_screen_height: f32,
    pub song_lua_player_x: [Option<f32>; MAX_PLAYERS],
    pub song_lua_player_y: [Option<f32>; MAX_PLAYERS],
    pub song_lua_player_z: [f32; MAX_PLAYERS],
    pub song_lua_player_rotation_x: [f32; MAX_PLAYERS],
    pub song_lua_player_rotation_z: [f32; MAX_PLAYERS],
    pub song_lua_player_rotation_y: [f32; MAX_PLAYERS],
    pub song_lua_player_skew_x: [f32; MAX_PLAYERS],
    pub song_lua_player_skew_y: [f32; MAX_PLAYERS],
    pub song_lua_player_zoom_x: [f32; MAX_PLAYERS],
    pub song_lua_player_zoom_y: [f32; MAX_PLAYERS],
    pub song_lua_player_zoom_z: [f32; MAX_PLAYERS],
    pub song_lua_player_confusion_y_offset: [f32; MAX_PLAYERS],
    active_attack_clear_all: [bool; MAX_PLAYERS],
    active_attack_chart: [ChartAttackEffects; MAX_PLAYERS],
    active_attack_accel: [AccelOverrides; MAX_PLAYERS],
    active_attack_visual: [VisualOverrides; MAX_PLAYERS],
    attacks_cleared_for_outro: bool,
    outro_attack_visual: [VisualOverrides; MAX_PLAYERS],
    attack_current_appearance: [AppearanceEffects; MAX_PLAYERS],
    attack_target_appearance: [AppearanceEffects; MAX_PLAYERS],
    attack_speed_appearance: [AppearanceEffects; MAX_PLAYERS],
    active_attack_appearance: [AppearanceEffects; MAX_PLAYERS],
    active_attack_visibility: [VisibilityOverrides; MAX_PLAYERS],
    active_attack_scroll: [ScrollOverrides; MAX_PLAYERS],
    active_attack_perspective: [PerspectiveOverrides; MAX_PLAYERS],
    active_attack_scroll_speed: [Option<ScrollSpeedSetting>; MAX_PLAYERS],
    active_attack_mini_percent: [Option<f32>; MAX_PLAYERS],
    pub noteskin_effects: GameplayNoteskinEffects,
    pub active_color_index: i32,
    pub player_color_index: i32,
    pub scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
    pub scroll_reference_bpm: f32,
    pub field_zoom: [f32; MAX_PLAYERS],
    pub scroll_pixels_per_second: [f32; MAX_PLAYERS],
    pub scroll_travel_time: [f32; MAX_PLAYERS],
    pub draw_distance_before_targets: [f32; MAX_PLAYERS],
    pub draw_distance_after_targets: [f32; MAX_PLAYERS],
    pub reverse_scroll: [bool; MAX_PLAYERS],
    pub column_scroll_dirs: [f32; MAX_COLS],
    pub receptor_glow_timers: [f32; MAX_COLS],
    receptor_glow_press_timers: [f32; MAX_COLS],
    receptor_glow_lift_start_alpha: [f32; MAX_COLS],
    receptor_glow_lift_start_zoom: [f32; MAX_COLS],
    pub receptor_bop_timers: [f32; MAX_COLS],
    pub receptor_bop_behaviors: [GameplayReceptorStepBehavior; MAX_COLS],
    pub tap_explosions: [Option<ActiveTapExplosion>; MAX_COLS],
    pub column_flashes: [Option<ActiveColumnFlash>; MAX_COLS],
    /// Ungated per-column tap judgement (see `ColumnTapJudgment`), for pad/panel lighting.
    pub last_tap_judgments: [Option<ColumnTapJudgment>; MAX_COLS],
    pub mine_explosions: [Option<ActiveMineExplosion>; MAX_COLS],
    pub active_holds: [Option<ActiveHold>; MAX_COLS],

    pub holds_total: [u32; MAX_PLAYERS],
    pub rolls_total: [u32; MAX_PLAYERS],
    pub mines_total: [u32; MAX_PLAYERS],
    pub total_steps: [u32; MAX_PLAYERS],
    pub hands_total: [u32; MAX_PLAYERS],

    pub total_elapsed_in_screen: f32,

    danger_fx: [DangerFx; MAX_PLAYERS],

    pub density_graph_first_second: f32,
    pub density_graph_last_second: f32,
    pub density_graph_duration: f32,
    pub density_graph_graph_w: f32,
    pub density_graph_graph_h: f32,
    pub density_graph_scaled_width: f32,
    pub density_graph_u0: f32,
    pub density_graph_u_window: f32,
    pub density_graph_life_update_rate: f32,
    pub density_graph_life_next_update_elapsed: f32,
    pub density_graph_life_points: [Vec<[f32; 2]>; MAX_PLAYERS],
    pub density_graph_life_dirty: [bool; MAX_PLAYERS],
    pub density_graph_top_h: f32,
    pub density_graph_top_w: [f32; MAX_PLAYERS],
    pub density_graph_top_scale_y: [f32; MAX_PLAYERS],

    pub hold_to_exit_key: Option<HoldToExitKey>,
    pub hold_to_exit_start: Option<Instant>,
    pub hold_to_exit_aborted_at: Option<Instant>,
    pub exit_transition: Option<ExitTransition>,
    shift_held: bool,
    ctrl_held: bool,
    offset_adjust_held_since: [Option<Instant>; 2],
    offset_adjust_last_at: [Option<Instant>; 2],
    prev_inputs: [bool; MAX_COLS],
    input_slots: [ActiveInputSlot; MAX_ACTIVE_INPUT_SLOTS],
    input_slot_count: usize,
    input_lane_counts: [u16; MAX_COLS],
    lane_pressed_since_ns: [Option<SongTimeNs>; MAX_COLS],
    pending_edges: VecDeque<InputEdge>,
    autoplay_rng: TurnRng,
    autoplay_cursor: [usize; MAX_PLAYERS],
    tick_mode: TickMode,
    assist_clap_rows: Vec<usize>,
    assist_clap_cursor: usize,
    assist_last_crossed_row: i32,
    assist_sfx_gen_seen: u64,
    toggle_flash_text: Option<&'static str>,
    toggle_flash_timer: f32,
    replay_input: Vec<RecordedLaneEdge>,
    replay_cursor: usize,
    pub replay_edges: Vec<RecordedLaneEdge>,

    update_trace: GameplayUpdateTraceState,
}

#[inline(always)]
pub fn drain_audio_commands(state: &mut State) -> std::vec::Drain<'_, GameplayAudioCommand> {
    state.audio_commands.drain(..)
}

#[inline(always)]
pub(super) fn queue_audio_command(state: &mut State, command: GameplayAudioCommand) {
    state.audio_commands.push(command);
}

#[inline(always)]
pub fn drain_session_commands(state: &mut State) -> std::vec::Drain<'_, GameplaySessionCommand> {
    state.session_commands.drain(..)
}

#[inline(always)]
pub(super) fn queue_session_command(state: &mut State, command: GameplaySessionCommand) {
    state.session_commands.push(command);
}

#[inline(always)]
fn queue_stop_music(state: &mut State) {
    queue_audio_command(state, GameplayAudioCommand::StopMusic);
}

#[inline(always)]
fn queue_play_music(state: &mut State, path: PathBuf, cut: GameplayMusicCut, rate: f32) {
    queue_audio_command(
        state,
        GameplayAudioCommand::PlayMusic {
            path,
            cut,
            looping: false,
            rate,
        },
    );
}

#[inline(always)]
pub(super) fn queue_preloaded_sfx(state: &mut State, path: &'static str) {
    queue_audio_command(state, GameplayAudioCommand::PlayPreloadedSfx(path));
}

#[inline(always)]
pub(super) fn queue_preloaded_assist_tick(state: &mut State, path: &'static str) {
    queue_audio_command(state, GameplayAudioCommand::PlayPreloadedAssistTick(path));
}

#[inline(always)]
fn queue_assist_tick_at_music_time(state: &mut State, path: &'static str, music_seconds: f64) {
    queue_audio_command(
        state,
        GameplayAudioCommand::PlayAssistTickAtMusicTime {
            path,
            music_seconds,
        },
    );
}

impl GameplayUpdateTraceState {
    #[inline(always)]
    fn from_state(state: &State) -> Self {
        let mut trace = Self::default();
        trace.pending_edges_capacity = state.pending_edges.capacity();
        trace.replay_edges_capacity = state.replay_edges.capacity();
        trace.decaying_hold_capacity = state.decaying_hold_indices.capacity();
        for player in 0..state.num_players.min(MAX_PLAYERS) {
            trace.density_life_capacity[player] =
                state.density_graph_life_points[player].capacity();
        }
        trace
    }
}

#[inline(always)]
fn elapsed_us_since(started: Instant) -> u32 {
    let elapsed = started.elapsed().as_micros();
    if elapsed > u128::from(u32::MAX) {
        u32::MAX
    } else {
        elapsed as u32
    }
}

#[inline(always)]
fn elapsed_us_between(later: Instant, earlier: Instant) -> u32 {
    let elapsed = later
        .checked_duration_since(earlier)
        .unwrap_or(Duration::ZERO)
        .as_micros();
    if elapsed > u128::from(u32::MAX) {
        u32::MAX
    } else {
        elapsed as u32
    }
}

#[inline(always)]
fn add_elapsed_us(dst: &mut u32, started: Instant) {
    *dst = dst.saturating_add(elapsed_us_since(started));
}

#[inline(always)]
fn max_phase_name_and_us(phases: &GameplayUpdatePhaseTimings) -> (&'static str, u32) {
    let mut best = ("pre_notes", phases.pre_notes_us);
    if phases.autoplay_us > best.1 {
        best = ("autoplay", phases.autoplay_us);
    }
    if phases.input_edges_us > best.1 {
        best = ("input_edges", phases.input_edges_us);
    }
    if phases.held_mines_us > best.1 {
        best = ("held_mines", phases.held_mines_us);
    }
    if phases.active_holds_us > best.1 {
        best = ("active_holds", phases.active_holds_us);
    }
    if phases.hold_decay_us > best.1 {
        best = ("hold_decay", phases.hold_decay_us);
    }
    if phases.visuals_us > best.1 {
        best = ("visuals", phases.visuals_us);
    }
    if phases.spawn_arrows_us > best.1 {
        best = ("spawn_arrows", phases.spawn_arrows_us);
    }
    if phases.mine_avoid_us > best.1 {
        best = ("mine_avoid", phases.mine_avoid_us);
    }
    if phases.tap_miss_us > best.1 {
        best = ("tap_miss", phases.tap_miss_us);
    }
    if phases.cull_us > best.1 {
        best = ("cull", phases.cull_us);
    }
    if phases.judged_rows_us > best.1 {
        best = ("judged_rows", phases.judged_rows_us);
    }
    if phases.density_us > best.1 {
        best = ("density", phases.density_us);
    }
    if phases.danger_us > best.1 {
        best = ("danger", phases.danger_us);
    }
    if phases.untracked_us > best.1 {
        best = ("untracked", phases.untracked_us);
    }
    best
}

#[inline(always)]
fn accumulate_phase_max(dst: &mut GameplayUpdatePhaseTimings, src: &GameplayUpdatePhaseTimings) {
    dst.pre_notes_us = dst.pre_notes_us.max(src.pre_notes_us);
    dst.autoplay_us = dst.autoplay_us.max(src.autoplay_us);
    dst.input_edges_us = dst.input_edges_us.max(src.input_edges_us);
    dst.input_queue_us = dst.input_queue_us.max(src.input_queue_us);
    dst.input_state_us = dst.input_state_us.max(src.input_state_us);
    dst.input_glow_us = dst.input_glow_us.max(src.input_glow_us);
    dst.input_judge_us = dst.input_judge_us.max(src.input_judge_us);
    dst.input_roll_us = dst.input_roll_us.max(src.input_roll_us);
    dst.held_mines_us = dst.held_mines_us.max(src.held_mines_us);
    dst.active_holds_us = dst.active_holds_us.max(src.active_holds_us);
    dst.hold_decay_us = dst.hold_decay_us.max(src.hold_decay_us);
    dst.visuals_us = dst.visuals_us.max(src.visuals_us);
    dst.spawn_arrows_us = dst.spawn_arrows_us.max(src.spawn_arrows_us);
    dst.mine_avoid_us = dst.mine_avoid_us.max(src.mine_avoid_us);
    dst.tap_miss_us = dst.tap_miss_us.max(src.tap_miss_us);
    dst.cull_us = dst.cull_us.max(src.cull_us);
    dst.judged_rows_us = dst.judged_rows_us.max(src.judged_rows_us);
    dst.density_us = dst.density_us.max(src.density_us);
    dst.density_sample_us = dst.density_sample_us.max(src.density_sample_us);
    dst.danger_us = dst.danger_us.max(src.danger_us);
    dst.untracked_us = dst.untracked_us.max(src.untracked_us);
}

#[inline(always)]
fn tracked_phase_total_us(phases: &GameplayUpdatePhaseTimings) -> u32 {
    phases
        .pre_notes_us
        .saturating_add(phases.autoplay_us)
        .saturating_add(phases.input_edges_us)
        .saturating_add(phases.held_mines_us)
        .saturating_add(phases.active_holds_us)
        .saturating_add(phases.hold_decay_us)
        .saturating_add(phases.visuals_us)
        .saturating_add(phases.spawn_arrows_us)
        .saturating_add(phases.mine_avoid_us)
        .saturating_add(phases.tap_miss_us)
        .saturating_add(phases.cull_us)
        .saturating_add(phases.judged_rows_us)
        .saturating_add(phases.density_us)
        .saturating_add(phases.danger_us)
}

fn trace_capacity_growth(state: &mut State) {
    let num_players = state.num_players.min(MAX_PLAYERS);
    let frame = state.update_trace.frame_counter;
    let pending_cap = state.pending_edges.capacity();
    if pending_cap > state.update_trace.pending_edges_capacity {
        debug!(
            "Gameplay vec growth frame={frame}: pending_edges capacity {} -> {} (len={})",
            state.update_trace.pending_edges_capacity,
            pending_cap,
            state.pending_edges.len()
        );
        state.update_trace.pending_edges_capacity = pending_cap;
    }
    let replay_cap = state.replay_edges.capacity();
    if replay_cap > state.update_trace.replay_edges_capacity {
        debug!(
            "Gameplay vec growth frame={frame}: replay_edges capacity {} -> {} (len={})",
            state.update_trace.replay_edges_capacity,
            replay_cap,
            state.replay_edges.len()
        );
        state.update_trace.replay_edges_capacity = replay_cap;
    }
    let decaying_cap = state.decaying_hold_indices.capacity();
    if decaying_cap > state.update_trace.decaying_hold_capacity {
        debug!(
            "Gameplay vec growth frame={frame}: decaying_hold_indices capacity {} -> {} (len={})",
            state.update_trace.decaying_hold_capacity,
            decaying_cap,
            state.decaying_hold_indices.len()
        );
        state.update_trace.decaying_hold_capacity = decaying_cap;
    }
    for player in 0..num_players {
        let new_cap = state.density_graph_life_points[player].capacity();
        let old_cap = state.update_trace.density_life_capacity[player];
        if new_cap > old_cap {
            debug!(
                "Gameplay vec growth frame={frame}: density_graph_life_points[{player}] capacity {old_cap} -> {new_cap} (len={})",
                state.density_graph_life_points[player].len()
            );
            state.update_trace.density_life_capacity[player] = new_cap;
        }
    }
}

fn trace_gameplay_update(
    state: &mut State,
    delta_time: f32,
    music_time_sec: f32,
    total_us: u32,
    mut phases: GameplayUpdatePhaseTimings,
) {
    phases.untracked_us = total_us.saturating_sub(tracked_phase_total_us(&phases));
    let pending_len = state.pending_edges.len();
    let replay_edges_len = state.replay_edges.len();
    let decaying_len = state.decaying_hold_indices.len();
    let frame_counter = {
        let trace_state = &mut state.update_trace;
        trace_state.frame_counter = trace_state.frame_counter.wrapping_add(1);
        trace_state.summary_elapsed_s += delta_time.max(0.0);
        trace_state.summary_frames = trace_state.summary_frames.saturating_add(1);
        trace_state.summary_max_total_us = trace_state.summary_max_total_us.max(total_us);
        accumulate_phase_max(&mut trace_state.summary_max_phase, &phases);
        trace_state.summary_peak_pending_edges =
            trace_state.summary_peak_pending_edges.max(pending_len);
        trace_state.frame_counter
    };

    if pending_len >= GAMEPLAY_INPUT_BACKLOG_WARN {
        debug!(
            "Gameplay input backlog: frame={}, pending_edges={}, replay_edges={}",
            frame_counter, pending_len, replay_edges_len
        );
    }

    let (hot_name, hot_us) = max_phase_name_and_us(&phases);
    let is_slow =
        total_us >= GAMEPLAY_TRACE_SLOW_FRAME_US || hot_us >= GAMEPLAY_TRACE_PHASE_SPIKE_US;
    if is_slow {
        state.update_trace.summary_slow_frames =
            state.update_trace.summary_slow_frames.saturating_add(1);
        debug!(
            "Gameplay slow frame={} t={:.3}s total={:.3}ms hot={}({:.3}ms) pending={} decays={} phases_ms=[pre:{:.3} auto:{:.3} input:{:.3} held:{:.3} holds:{:.3} decay:{:.3} vis:{:.3} spawn:{:.3} mine:{:.3} tmiss:{:.3} cull:{:.3} judged:{:.3} density:{:.3} danger:{:.3} other:{:.3}] input_sub_ms=[queue:{:.3} state:{:.3} glow:{:.3} judge:{:.3} roll:{:.3}] density_sub_ms=[sample:{:.3}]",
            frame_counter,
            music_time_sec,
            total_us as f32 / 1000.0,
            hot_name,
            hot_us as f32 / 1000.0,
            pending_len,
            decaying_len,
            phases.pre_notes_us as f32 / 1000.0,
            phases.autoplay_us as f32 / 1000.0,
            phases.input_edges_us as f32 / 1000.0,
            phases.held_mines_us as f32 / 1000.0,
            phases.active_holds_us as f32 / 1000.0,
            phases.hold_decay_us as f32 / 1000.0,
            phases.visuals_us as f32 / 1000.0,
            phases.spawn_arrows_us as f32 / 1000.0,
            phases.mine_avoid_us as f32 / 1000.0,
            phases.tap_miss_us as f32 / 1000.0,
            phases.cull_us as f32 / 1000.0,
            phases.judged_rows_us as f32 / 1000.0,
            phases.density_us as f32 / 1000.0,
            phases.danger_us as f32 / 1000.0,
            phases.untracked_us as f32 / 1000.0,
            phases.input_queue_us as f32 / 1000.0,
            phases.input_state_us as f32 / 1000.0,
            phases.input_glow_us as f32 / 1000.0,
            phases.input_judge_us as f32 / 1000.0,
            phases.input_roll_us as f32 / 1000.0,
            phases.density_sample_us as f32 / 1000.0
        );
    }

    if log::log_enabled!(log::Level::Trace)
        && state.update_trace.summary_elapsed_s >= GAMEPLAY_TRACE_SUMMARY_INTERVAL_S
    {
        let summary_frames = state.update_trace.summary_frames;
        let summary_slow_frames = state.update_trace.summary_slow_frames;
        let summary_max_total_us = state.update_trace.summary_max_total_us;
        let summary_max_phase = state.update_trace.summary_max_phase;
        let summary_input_latency = state.update_trace.summary_input_latency;
        let summary_peak_pending_edges = state.update_trace.summary_peak_pending_edges;
        let (summary_hot_name, summary_hot_us) = max_phase_name_and_us(&summary_max_phase);
        trace!(
            "Gameplay trace summary: frames={} slow={} max_total={:.3}ms max_hot={}({:.3}ms) peak_pending={} input_sub_max_ms=[queue:{:.3} state:{:.3} glow:{:.3} judge:{:.3} roll:{:.3}] input_latency_us=[samples:{} cap_store_avg:{:.1} cap_store_max:{} store_emit_avg:{:.1} store_emit_max:{} emit_queue_avg:{:.1} emit_queue_max:{} queue_proc_avg:{:.1} queue_proc_max:{} cap_proc_avg:{:.1} cap_proc_max:{}] density_sub_max_ms=[sample:{:.3}] other_max={:.3}",
            summary_frames,
            summary_slow_frames,
            summary_max_total_us as f32 / 1000.0,
            summary_hot_name,
            summary_hot_us as f32 / 1000.0,
            summary_peak_pending_edges,
            summary_max_phase.input_queue_us as f32 / 1000.0,
            summary_max_phase.input_state_us as f32 / 1000.0,
            summary_max_phase.input_glow_us as f32 / 1000.0,
            summary_max_phase.input_judge_us as f32 / 1000.0,
            summary_max_phase.input_roll_us as f32 / 1000.0,
            summary_input_latency.samples,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.capture_to_store_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.capture_to_store_max_us,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.store_to_emit_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.store_to_emit_max_us,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.emit_to_queue_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.emit_to_queue_max_us,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.queue_to_process_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.queue_to_process_max_us,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.capture_to_process_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.capture_to_process_max_us,
            summary_max_phase.density_sample_us as f32 / 1000.0,
            summary_max_phase.untracked_us as f32 / 1000.0
        );
        state.update_trace.summary_elapsed_s = 0.0;
        state.update_trace.summary_frames = 0;
        state.update_trace.summary_slow_frames = 0;
        state.update_trace.summary_max_total_us = 0;
        state.update_trace.summary_max_phase = GameplayUpdatePhaseTimings::default();
        state.update_trace.summary_input_latency = GameplayInputLatencyTrace::default();
        state.update_trace.summary_peak_pending_edges = 0;
    }

    trace_capacity_growth(state);
}

#[cfg(test)]
fn assert_valid_hot_state_for_tests(state: &State, delta_time: f32, music_time_sec: f32) {
    debug_assert!(
        delta_time.is_finite() && delta_time >= 0.0,
        "invalid delta_time={delta_time}"
    );
    debug_assert!(
        music_time_sec.is_finite(),
        "invalid music_time_sec={music_time_sec}"
    );
    debug_assert!(
        state.num_players > 0 && state.num_players <= MAX_PLAYERS,
        "invalid num_players={}",
        state.num_players
    );
    debug_assert!(
        state.num_cols > 0 && state.num_cols <= MAX_COLS,
        "invalid num_cols={}",
        state.num_cols
    );
    debug_assert!(
        state.cols_per_player > 0 && state.cols_per_player <= MAX_COLS,
        "invalid cols_per_player={}",
        state.cols_per_player
    );
    debug_assert_eq!(state.notes.len(), state.note_time_cache_ns.len());
    debug_assert_eq!(state.notes.len(), state.hold_end_time_cache_ns.len());
    debug_assert_eq!(state.notes.len(), state.hold_decay_active.len());
    debug_assert_eq!(state.notes.len(), state.note_row_entry_indices.len());
    for player in 0..state.num_players {
        let (start, end) = state.note_ranges[player];
        debug_assert!(start <= end && end <= state.notes.len());
        let (row_start, row_end) = state.row_entry_ranges[player];
        debug_assert!(row_start <= row_end && row_end <= state.row_entries.len());
        debug_assert!(
            state.judged_row_cursor[player] >= row_start
                && state.judged_row_cursor[player] <= row_end
        );
        debug_assert!(
            state.next_tap_miss_cursor[player] >= start
                && state.next_tap_miss_cursor[player] <= end
        );
        debug_assert!(
            state.next_mine_avoid_cursor[player] >= start
                && state.next_mine_avoid_cursor[player] <= end
        );
        debug_assert_eq!(
            state.mine_note_ix[player].len(),
            state.mine_note_time_ns[player].len()
        );
        debug_assert!(state.next_mine_ix_cursor[player] <= state.mine_note_ix[player].len());
    }
    for player in 0..state.num_players {
        let (start, end) = state.note_ranges[player];
        debug_assert!(
            state.mine_note_time_ns[player]
                .windows(2)
                .all(|pair| pair[0] <= pair[1])
        );
        for &note_index in &state.mine_note_ix[player] {
            debug_assert!(note_index >= start && note_index < end);
            debug_assert!(matches!(state.notes[note_index].note_type, NoteType::Mine));
        }
    }
    for note in &state.notes {
        if note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
            let player = player_for_col(state, note.column);
            debug_assert!(
                row_entry_for_cached_row(
                    &state.row_entries,
                    &state.row_map_cache[player],
                    note.row_index
                )
                .is_some()
            );
        }
    }
    for (row_entry_index, row_entry) in state.row_entries.iter().enumerate() {
        let first_note_index = row_entry.note_indices()[0];
        let player = player_for_col(state, state.notes[first_note_index].column);
        debug_assert!(
            row_entry_index >= state.row_entry_ranges[player].0
                && row_entry_index < state.row_entry_ranges[player].1
        );
        debug_assert_eq!(
            state.row_map_cache[player]
                .get(row_entry.row_index)
                .copied(),
            Some(row_entry_index as u32)
        );
        for &note_index in row_entry.note_indices() {
            debug_assert!(note_index < state.notes.len());
            debug_assert_eq!(
                state.note_row_entry_indices[note_index],
                row_entry_index as u32
            );
            let note = &state.notes[note_index];
            debug_assert_eq!(note.row_index, row_entry.row_index);
            debug_assert!(note.can_be_judged);
            debug_assert!(!note.is_fake);
            debug_assert!(!matches!(note.note_type, NoteType::Mine));
        }
    }
    for col in 0..state.num_cols {
        debug_assert!(state.column_scroll_dirs[col].is_finite());
        debug_assert!(state.lane_note_indices[col].windows(2).all(|pair| {
            let left = pair[0];
            let right = pair[1];
            left < right && state.note_time_cache_ns[left] <= state.note_time_cache_ns[right]
        }));
        for &note_index in &state.lane_note_indices[col] {
            debug_assert!(note_index < state.notes.len());
            debug_assert_eq!(state.notes[note_index].column, col);
        }
        debug_assert_eq!(
            state.lane_note_row_indices[col].len(),
            state.lane_note_indices[col].len()
        );
        debug_assert!(state.lane_note_row_indices[col].windows(2).all(|pair| {
            let left = pair[0];
            let right = pair[1];
            (beat_to_note_row(state.notes[left].beat), left)
                <= (beat_to_note_row(state.notes[right].beat), right)
        }));
        for &note_index in &state.lane_note_row_indices[col] {
            debug_assert!(note_index < state.notes.len());
            debug_assert_eq!(state.notes[note_index].column, col);
        }
        debug_assert!(state.lane_hold_indices[col].windows(2).all(|pair| {
            let left = pair[0];
            let right = pair[1];
            left < right && state.note_time_cache_ns[left] <= state.note_time_cache_ns[right]
        }));
        for &note_index in &state.lane_hold_indices[col] {
            debug_assert!(note_index < state.notes.len());
            debug_assert_eq!(state.notes[note_index].column, col);
            debug_assert!(matches!(
                state.notes[note_index].note_type,
                NoteType::Hold | NoteType::Roll
            ));
        }
    }
    for col in state.num_cols..MAX_COLS {
        debug_assert!(state.lane_note_indices[col].is_empty());
        debug_assert!(state.lane_note_row_indices[col].is_empty());
        debug_assert!(state.lane_hold_indices[col].is_empty());
    }
    let mut lane_positions = [0usize; MAX_COLS];
    for (note_index, note) in state.notes.iter().enumerate() {
        if note.column >= state.num_cols {
            continue;
        }
        let lane_pos = lane_positions[note.column];
        debug_assert_eq!(
            state.lane_note_indices[note.column].get(lane_pos).copied(),
            Some(note_index)
        );
        lane_positions[note.column] += 1;
    }
    for col in 0..state.num_cols {
        debug_assert_eq!(lane_positions[col], state.lane_note_indices[col].len());
    }
}

#[inline(always)]
fn finalize_update_trace(
    state: &mut State,
    delta_time: f32,
    music_time_sec: f32,
    frame_trace_started: Option<Instant>,
    phase_timings: GameplayUpdatePhaseTimings,
) {
    let Some(started) = frame_trace_started else {
        return;
    };
    let total_us = elapsed_us_since(started);
    trace_gameplay_update(state, delta_time, music_time_sec, total_us, phase_timings);
}

fn refresh_live_notefield_options(state: &mut State, current_bpm: f32) {
    for player in 0..state.num_players {
        let scroll = effective_scroll_effects_for_player(state, player);
        state.reverse_scroll[player] =
            scroll.reverse_percent_for_column(0, state.cols_per_player) > 0.5;
        let start = player.saturating_mul(state.cols_per_player);
        let end = (start + state.cols_per_player)
            .min(state.num_cols)
            .min(MAX_COLS);
        for (local_col, col) in (start..end).enumerate() {
            state.column_scroll_dirs[col] =
                scroll.reverse_scale_for_column(local_col, state.cols_per_player);
        }
    }
    for player in 0..state.num_players {
        let scroll_speed = effective_scroll_speed_for_player(state, player);
        let mut dynamic_speed = scroll_speed.pixels_per_second(
            current_bpm,
            state.scroll_reference_bpm,
            state.music_rate,
        );
        if !dynamic_speed.is_finite() || dynamic_speed <= 0.0 {
            dynamic_speed = ScrollSpeedSetting::default().pixels_per_second(
                current_bpm,
                state.scroll_reference_bpm,
                state.music_rate,
            );
        }
        state.scroll_pixels_per_second[player] = dynamic_speed;

        let scroll = effective_scroll_effects_for_player(state, player);
        let visual_mask = effective_visual_mask_for_player(state, player);
        let mini_percent = effective_mini_percent_for_player(state, player);
        let mini = effective_mini_value_with_visual_mask(
            &state.player_profiles[player],
            visual_mask,
            mini_percent,
        );
        let mut field_zoom = 1.0 - mini * 0.5;
        if field_zoom.abs() < 0.01 {
            field_zoom = 0.01;
        }
        state.field_zoom[player] = field_zoom;

        let perspective = effective_perspective_effects_for_player(state, player);
        let draw_scale = player_draw_scale_for_tilt_with_visual_mask(
            perspective.tilt,
            &state.player_profiles[player],
            visual_mask,
            mini_percent,
        );
        state.draw_distance_before_targets[player] =
            draw_distance_before_targets(state.viewport.height(), draw_scale);
        state.draw_distance_after_targets[player] =
            draw_distance_after_targets(state.viewport.height(), draw_scale, scroll.centered);

        let mut travel_time = scroll_speed.travel_time_seconds(
            state.draw_distance_before_targets[player],
            current_bpm,
            state.scroll_reference_bpm,
            state.music_rate,
        );
        if !travel_time.is_finite() || travel_time <= 0.0 {
            travel_time =
                state.draw_distance_before_targets[player] / dynamic_speed.max(f32::EPSILON);
        }
        state.scroll_travel_time[player] = travel_time;
    }
}

pub fn toggle_flash_text(state: &State) -> Option<(&'static str, f32)> {
    toggle_flash_alpha(state.toggle_flash_timer)
        .and_then(|alpha| state.toggle_flash_text.map(|t| (t, alpha)))
}

#[inline(always)]
fn live_autoplay_judgment_offset_music_ns(
    state: &mut State,
    player_idx: usize,
    window: TimingWindow,
    measured_offset_music_ns: SongTimeNs,
) -> SongTimeNs {
    if !live_autoplay_enabled(state) {
        return measured_offset_music_ns;
    }
    let timing_profile = if player_idx < state.num_players {
        state.player_judgment_timing[player_idx].profile_music_ns
    } else {
        TimingProfileNs::from_profile_scaled(&state.timing_profile, state.music_rate)
    };
    autoplay_random_offset_music_ns_for_window(&mut state.autoplay_rng, timing_profile, window)
}

#[inline(always)]
fn abort_hold_to_exit(state: &mut State, at: Instant) {
    if state.hold_to_exit_start.is_some() {
        state.hold_to_exit_key = None;
        state.hold_to_exit_start = None;
        state.hold_to_exit_aborted_at = Some(at);
    }
}

#[inline(always)]
fn begin_exit_transition(state: &mut State, kind: ExitTransitionKind) {
    if state.exit_transition.is_some() {
        return;
    }
    state.hold_to_exit_key = None;
    state.hold_to_exit_start = None;
    state.hold_to_exit_aborted_at = None;
    state.exit_transition = Some(ExitTransition {
        kind,
        started_at: Instant::now(),
    });
    queue_stop_music(state);
}

/// SL/zmod parity: trigger the fast Cancel exit fade (~0.5s) used by BACK,
/// so an in-progress song can hand off to the next gameplay entry without
/// playing the long ~1.5s gameplay out-transition. The app shell intercepts
/// the resulting `Cancel` navigation and re-enters Gameplay.
pub fn begin_restart_exit(state: &mut State) {
    begin_exit_transition(state, ExitTransitionKind::Cancel);
}

pub fn danger_overlay_rgba(state: &State, player: usize) -> Option<[f32; 4]> {
    if player >= state.num_players {
        return None;
    }
    if state.player_profiles[player].hide_lifebar {
        return None;
    }
    let rgba = danger_fx_rgba(&state.danger_fx[player], state.total_elapsed_in_screen);
    if rgba[3] > 0.0 { Some(rgba) } else { None }
}

/// Whether a lane is physically pressed right now (live input state). Exposed
/// for the gameplay HUD's optional SMX pad-input display, which mirrors the
/// input tester by lighting panels straight from the inputs we receive.
pub fn lane_pressed(state: &State, col: usize) -> bool {
    col < state.num_cols && lane_is_pressed(state, col)
}

#[inline(always)]
fn player_for_col(state: &State, col: usize) -> usize {
    if state.num_players <= 1 || state.cols_per_player == 0 {
        return 0;
    }
    (col / state.cols_per_player).min(state.num_players.saturating_sub(1))
}

#[inline(always)]
const fn player_col_range(state: &State, player: usize) -> (usize, usize) {
    let start = player * state.cols_per_player;
    (start, start + state.cols_per_player)
}

#[inline(always)]
fn record_step_calories(state: &mut State, lane_idx: usize, event_music_time_ns: SongTimeNs) {
    if song_time_ns_invalid(event_music_time_ns) {
        return;
    }
    let player = player_for_col(state, lane_idx);
    let (start, end) = player_col_range(state, player);
    let tracks = recent_step_tracks(
        &state.lane_pressed_since_ns,
        start,
        end,
        event_music_time_ns,
    );
    let weight_pounds = state.player_profiles[player].calculated_weight_pounds();
    state.players[player].calories_burned += judgment::step_calories(weight_pounds, tracks);
}

#[inline(always)]
fn player_note_range(state: &State, player: usize) -> (usize, usize) {
    if player >= state.num_players {
        return (0, 0);
    }
    state.note_ranges[player]
}

#[inline(always)]
fn push_density_life_point(points: &mut Vec<[f32; 2]>, x: f32, y: f32) -> bool {
    const EPS: f32 = 0.000_1_f32;
    const ANGLE_SIN2_MAX: f32 = 0.032_f32; // sin(0.18rad)^2

    if let Some(last) = points.last_mut()
        && x <= last[0] + EPS
    {
        if (y - last[1]).abs() <= EPS {
            return false;
        }
        last[1] = y;
        return true;
    }

    if points.len() >= 2 {
        let a = points[points.len() - 2];
        let b = points[points.len() - 1];
        let abx = b[0] - a[0];
        let aby = b[1] - a[1];
        let bcx = x - b[0];
        let bcy = y - b[1];
        let ab_len_sq = abx.mul_add(abx, aby * aby);
        let bc_len_sq = bcx.mul_add(bcx, bcy * bcy);
        let dot = abx.mul_add(bcx, aby * bcy);
        if dot > 0.0_f32 && ab_len_sq > EPS && bc_len_sq > EPS {
            let cross = abx.mul_add(bcy, -(aby * bcx));
            let cross_sq = cross * cross;
            if cross_sq <= ANGLE_SIN2_MAX * ab_len_sq * bc_len_sq {
                let last_ix = points.len() - 1;
                points[last_ix] = [x, y];
                return true;
            }
        }
    }

    points.push([x, y]);
    true
}

fn update_density_graph(
    state: &mut State,
    current_music_time: f32,
    trace_enabled: bool,
    phase_timings: &mut GameplayUpdatePhaseTimings,
) {
    let graph_w = state.density_graph_graph_w;
    let graph_h = state.density_graph_graph_h;
    let scaled_width = state.density_graph_scaled_width;
    if graph_w <= 0.0_f32 || graph_h <= 0.0_f32 || scaled_width <= 0.0_f32 {
        state.density_graph_u0 = 0.0_f32;
        return;
    }

    let duration = state.density_graph_duration.max(0.001_f32);
    let u_window = state.density_graph_u_window.clamp(0.0_f32, 1.0_f32);
    let max_u0 = (1.0_f32 - u_window).max(0.0_f32);
    let mut u0 = 0.0_f32;

    if max_u0 > 0.0_f32 {
        let max_seconds = (u_window * duration).max(0.0_f32);
        if max_seconds > 0.0_f32 {
            let first_second = state.density_graph_first_second;
            let last_second = state.density_graph_last_second;
            if current_music_time > last_second - (max_seconds * 0.75_f32) {
                u0 = max_u0;
            } else {
                let seconds_past_one_fourth =
                    (current_music_time - first_second) - (max_seconds * 0.25_f32);
                if seconds_past_one_fourth > 0.0_f32 {
                    u0 = (seconds_past_one_fourth / duration).clamp(0.0_f32, max_u0);
                }
            }
        }
    }

    state.density_graph_u0 = u0;

    let next_t = state.density_graph_life_next_update_elapsed;
    if state.density_graph_life_update_rate > 0.0_f32 && state.total_elapsed_in_screen >= next_t {
        let sample_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        let rate = state.density_graph_life_update_rate;
        let elapsed = (state.total_elapsed_in_screen - next_t).max(0.0_f32);
        let mut catch_up_steps = ((elapsed / rate).floor() as u32).saturating_add(1);
        if catch_up_steps > 64 {
            catch_up_steps = 64;
        }
        state.density_graph_life_next_update_elapsed += rate * catch_up_steps as f32;

        if current_music_time > 0.0_f32 && current_music_time <= state.density_graph_last_second {
            let denom = state.density_graph_duration.max(0.001_f32);
            let x = (((current_music_time - state.density_graph_first_second) / denom)
                * state.density_graph_scaled_width)
                .clamp(0.0_f32, state.density_graph_scaled_width);
            if x.is_finite() {
                for player in 0..state.num_players {
                    let life = state.players[player].life;
                    let y = (1.0_f32 - life).clamp(0.0_f32, 1.0_f32) * graph_h;
                    let points = &mut state.density_graph_life_points[player];
                    if push_density_life_point(points, x, y) {
                        state.density_graph_life_dirty[player] = true;
                    }
                }
            }
        }
        if let Some(started) = sample_started {
            add_elapsed_us(&mut phase_timings.density_sample_us, started);
        }
    }
}

#[inline(always)]
fn stream_pos_to_music_time(state: &State, stream_pos: f32) -> f32 {
    music_time_from_stream_position(
        stream_pos,
        state.audio_lead_in_seconds,
        state.global_offset_seconds,
        state.music_rate,
    )
}

fn set_current_music_time_ns(state: &mut State, music_time_ns: SongTimeNs) {
    state.current_music_time_ns = music_time_ns;
    let display_time_ns = state.display_clock.reset(music_time_ns);
    state.current_music_time_display = song_time_ns_to_seconds(display_time_ns);

    let beat_info = state
        .timing
        .get_beat_info_from_time_ns_cached(music_time_ns, &mut state.beat_info_cache);
    state.current_beat = beat_info.beat;
    state.current_beat_display = state.timing.get_beat_for_time_ns(display_time_ns);
    state.is_in_freeze = beat_info.is_in_freeze;
    state.is_in_delay = beat_info.is_in_delay;

    for player in 0..state.num_players {
        let delay = state.global_visual_delay_seconds + state.player_visual_delay_seconds[player];
        let visible_time_ns = visible_notefield_time_ns(music_time_ns, delay);
        state.current_music_time_visible_ns[player] = visible_time_ns;
        state.current_music_time_visible[player] = song_time_ns_to_seconds(visible_time_ns);
        state.current_beat_visible[player] =
            state.timing_players[player].get_beat_for_time_ns(visible_time_ns);
    }

    refresh_active_attack_masks(state, 0.0);
    let current_bpm = state.timing.get_bpm_for_beat(state.current_beat);
    refresh_live_notefield_options(state, current_bpm);
}

fn start_stage_music_audio(state: &mut State) {
    let Some(music_path) = state.charts[0].music_path.clone() else {
        return;
    };
    let lead_in = state.audio_lead_in_seconds.max(0.0);
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    debug!("Starting music with a preroll delay of {lead_in:.2}s");
    queue_play_music(state, music_path, stage_music_cut(lead_in), rate);
}

pub fn start_stage_music(state: &mut State) {
    let start_time = -state.audio_lead_in_seconds.max(0.0);
    set_current_music_time_ns(state, song_time_ns_from_seconds(start_time));
    state.total_elapsed_in_screen = 0.0;
    start_stage_music_audio(state);
}

#[inline(always)]
pub fn music_time_for_beat(state: &State, beat: f32) -> f32 {
    state.timing.get_time_for_beat(beat)
}

#[inline(always)]
pub fn beat_for_music_time(state: &State, music_time: f32) -> f32 {
    state.timing.get_beat_for_time(music_time)
}

#[inline(always)]
pub fn current_music_time_seconds(state: &State) -> f32 {
    song_time_ns_to_seconds(state.current_music_time_ns)
}

#[inline(always)]
pub fn music_time_from_audio_snapshot(state: &State, audio_snapshot: GameplayAudioSnapshot) -> f32 {
    song_time_ns_to_seconds(current_song_clock_snapshot(state, audio_snapshot).song_time_ns)
}

pub fn seek_practice_display(state: &mut State, music_time: f32) {
    set_current_music_time_ns(state, song_time_ns_from_seconds(music_time));
}

pub fn disable_score_for_practice(state: &mut State) {
    state.score_valid.fill(false);
    state.replay_capture_enabled = false;
    state.replay_mode = false;
}

/// Updates the music rate on a live gameplay state, rebuilding the
/// rate-dependent caches (judgment timing windows and end-time markers) so
/// later judging and completion checks remain consistent. Returns `true` when
/// the rate actually changed.
///
/// This does not touch the audio engine or the session-stored rate; callers
/// (e.g. practice-mode hotkeys) are responsible for keeping `audio::set_music_rate`
/// and `profile::set_session_music_rate` in sync.
pub fn set_music_rate(state: &mut State, rate: f32) -> bool {
    let normalized = if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    };
    if (normalized - state.music_rate).abs() <= f32::EPSILON {
        return false;
    }
    state.music_rate = normalized;
    let timing_profile = state.timing_profile;
    state.player_judgment_timing = std::array::from_fn(|player| {
        build_player_judgment_timing(timing_profile, &state.player_profiles[player], normalized)
    });
    let (notes_end_time_ns, music_end_time_ns) = compute_end_times_ns(
        &state.notes,
        &state.note_time_cache_ns,
        &state.hold_end_time_cache_ns,
        normalized,
        state.audio_end_time_ns,
    );
    state.notes_end_time_ns = notes_end_time_ns;
    state.music_end_time_ns = music_end_time_ns;
    true
}

fn first_note_index_at_or_after_time(state: &State, player: usize, time_ns: SongTimeNs) -> usize {
    first_time_index_at_or_after(
        &state.note_time_cache_ns,
        player_note_range(state, player),
        time_ns,
    )
}

fn first_row_entry_at_or_after_time(state: &State, player: usize, time_ns: SongTimeNs) -> usize {
    first_row_entry_index_at_or_after_time(
        &state.row_entries,
        state.row_entry_ranges[player],
        time_ns,
    )
}

fn reset_practice_note_results(state: &mut State) {
    for note in &mut state.notes {
        note.result = None;
        note.early_result = None;
        note.mine_result = None;
        if let Some(hold) = note.hold.as_mut() {
            hold.result = None;
            hold.life = 1.0;
            hold.let_go_started_at = None;
            hold.let_go_starting_life = 1.0;
            hold.last_held_row_index = note.row_index;
            hold.last_held_beat = note.beat;
        }
    }

    for row_ix in 0..state.row_entries.len() {
        let row_index = state.row_entries[row_ix].row_index;
        let nonmine_note_indices = state.row_entries[row_ix].nonmine_note_indices;
        let nonmine_note_count = state.row_entries[row_ix].nonmine_note_count;
        state.row_entries[row_ix] = build_row_entry(
            row_index,
            nonmine_note_indices,
            nonmine_note_count,
            &state.notes,
            &state.note_time_cache_ns,
        );
    }
}

pub fn reset_practice_playback(state: &mut State, judge_start_music_time: f32) {
    let judge_start_ns = song_time_ns_from_seconds(judge_start_music_time);
    reset_practice_note_results(state);
    disable_score_for_practice(state);

    state.song_completed_naturally = false;
    state.autoplay_used = false;
    state.hold_judgments = [None; MAX_COLS];
    state.held_miss_judgments = [None; MAX_COLS];
    state.tap_explosions = std::array::from_fn(|_| None);
    state.column_flashes = std::array::from_fn(|_| None);
    state.mine_explosions = std::array::from_fn(|_| None);
    state.active_holds = std::array::from_fn(|_| None);
    state.receptor_glow_timers.fill(0.0);
    state.receptor_glow_press_timers.fill(0.0);
    state.receptor_glow_lift_start_alpha.fill(0.0);
    state.receptor_glow_lift_start_zoom.fill(0.0);
    state.receptor_bop_timers.fill(0.0);
    state
        .receptor_bop_behaviors
        .fill(GameplayReceptorStepBehavior::identity());
    state.decaying_hold_indices.clear();
    state.hold_decay_active.fill(false);
    state.tap_miss_held_window.fill(false);
    state.pending_missed_hold_resolution.fill(false);
    state.pending_missed_hold_indices.clear();
    state.prev_inputs.fill(false);
    state.input_slot_count = 0;
    state.input_lane_counts.fill(0);
    state.lane_pressed_since_ns.fill(None);
    state.pending_edges.clear();
    state.replay_edges.clear();
    state.pending_mine_hit_indices.clear();
    state.replay_cursor = 0;
    state.hold_to_exit_key = None;
    state.hold_to_exit_start = None;
    state.hold_to_exit_aborted_at = None;
    state.exit_transition = None;
    state.live_window_counts = [Default::default(); MAX_PLAYERS];
    state.live_window_counts_10ms_blue = [Default::default(); MAX_PLAYERS];
    state.live_window_counts_display_blue = [Default::default(); MAX_PLAYERS];

    for player in 0..state.num_players {
        state.players[player] = init_player_runtime();
        let life = state.players[player].life;
        state.players[player]
            .life_history
            .push((judge_start_music_time, life));

        let note_cursor = first_note_index_at_or_after_time(state, player, judge_start_ns);
        state.next_tap_miss_cursor[player] = note_cursor;
        state.autoplay_cursor[player] = note_cursor;
        state.judged_row_cursor[player] =
            first_row_entry_at_or_after_time(state, player, judge_start_ns);

        let mine_cursor = state.mine_note_time_ns[player].partition_point(|&t| t < judge_start_ns);
        let (_, note_end) = player_note_range(state, player);
        state.next_mine_ix_cursor[player] = mine_cursor;
        state.next_mine_avoid_cursor[player] = state.mine_note_ix[player]
            .get(mine_cursor)
            .copied()
            .unwrap_or(note_end);
    }

    let song_row = assist_row_no_offset(state, judge_start_music_time);
    state.assist_clap_cursor = assist_clap_cursor_for_row(&state.assist_clap_rows, song_row);
    state.assist_last_crossed_row = song_row;
    state.total_elapsed_in_screen = 0.0;
}

pub fn start_practice_music_at(
    state: &mut State,
    playback_music_time: f32,
    judge_start_music_time: f32,
) {
    reset_practice_playback(state, judge_start_music_time);

    let Some(music_path) = state.charts[0].music_path.clone() else {
        state.audio_lead_in_seconds = (-playback_music_time).max(0.0);
        set_current_music_time_ns(state, song_time_ns_from_seconds(playback_music_time));
        return;
    };
    state.audio_lead_in_seconds = (-playback_music_time).max(0.0) as f32;
    set_current_music_time_ns(state, song_time_ns_from_seconds(playback_music_time));

    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    queue_play_music(
        state,
        music_path,
        GameplayMusicCut {
            start_sec: f64::from(playback_music_time),
            length_sec: f64::INFINITY,
            ..Default::default()
        },
        rate,
    );
}

fn get_reference_bpm_from_display_tag(
    chart: &ChartData,
    song_display_bpm_str: &str,
) -> Option<f32> {
    // 1. Try chart-level display BPM
    match &chart.display_bpm {
        Some(deadsync_chart::ChartDisplayBpm::Specified { max, .. }) => {
            let v = *max as f32;
            if v.is_finite() && v > 0.0 {
                return Some(v);
            }
        }
        Some(deadsync_chart::ChartDisplayBpm::Random) => return None,
        None => {}
    }
    // 2. Fall back to song-level display BPM string
    let s = song_display_bpm_str.trim();
    if s.is_empty() || s == "*" {
        return None;
    }
    if let Some((_, max_str)) = s.split_once(':') {
        return max_str.trim().parse::<f32>().ok();
    }
    s.parse::<f32>().ok()
}

fn step_stats_play_style(play_style: profile_data::PlayStyle) -> StepStatsPlayStyle {
    match play_style {
        profile_data::PlayStyle::Single => StepStatsPlayStyle::Single,
        profile_data::PlayStyle::Double => StepStatsPlayStyle::Double,
        profile_data::PlayStyle::Versus => StepStatsPlayStyle::Versus,
    }
}

fn upper_density_graph_width(play_style: profile_data::PlayStyle) -> f32 {
    step_stats_upper_density_graph_width(step_stats_play_style(play_style))
}

fn step_stats_density_graph_width(
    play_style: profile_data::PlayStyle,
    cols_per_player: usize,
    num_players: usize,
    screen_w: f32,
    screen_h: f32,
    wide: bool,
    center_1player_notefield: bool,
) -> f32 {
    gameplay_step_stats_density_graph_width(
        step_stats_play_style(play_style),
        cols_per_player,
        num_players,
        screen_w,
        screen_h,
        wide,
        center_1player_notefield,
    )
}

pub fn init(
    song: Arc<SongData>,
    charts: [Arc<ChartData>; MAX_PLAYERS],
    gameplay_charts: [Arc<GameplayChartData>; MAX_PLAYERS],
    viewport: GameplayViewport,
    session: GameplaySession,
    config: GameplayConfig,
    pack_sync_pref: SyncPref,
    mini_indicator_data: GameplayMiniIndicatorData,
    noteskin_data: GameplayNoteskinData,
    song_lua_data: GameplaySongLuaData,
    active_color_index: i32,
    music_rate: f32,
    mut scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
    mut player_profiles: [profile_data::Profile; MAX_PLAYERS],
    replay_edges: Option<Vec<ReplayInputEdge>>,
    replay_offsets: Option<ReplayOffsetSnapshot>,
    lead_in_timing: Option<LeadInTiming>,
    course_display_carry: Option<[CourseDisplayCarry; MAX_PLAYERS]>,
    course_display_totals: Option<[CourseDisplayTotals; MAX_PLAYERS]>,
    course_display_timing: Option<CourseDisplayTiming>,
    mut combo_carry: [u32; MAX_PLAYERS],
) -> State {
    debug!("Initializing Gameplay Screen...");
    let init_started = Instant::now();
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };

    let play_style = session.play_style;
    let p2_runtime_player = session.p2_runtime_player();
    let cols_per_player = play_style.cols_per_player();
    let num_players = play_style.player_count();
    let num_cols = play_style.total_cols();
    let replay_edges = replay_edges.unwrap_or_default();
    let mut charts = charts;
    let mut gameplay_charts = gameplay_charts;
    if p2_runtime_player {
        scroll_speed[0] = scroll_speed[1];
        player_profiles[0] = player_profiles[1].clone();
        charts[0] = charts[1].clone();
        gameplay_charts[0] = gameplay_charts[1].clone();
        combo_carry[0] = combo_carry[1];
    }
    let player_color_index = if p2_runtime_player {
        active_color_index - 2
    } else {
        active_color_index
    };

    let GameplayNoteskinData {
        effects: noteskin_effects,
    } = noteskin_data;

    let field_zoom: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return 1.0;
        }
        let mini_value = effective_mini_value(&player_profiles[player]);
        let mut z = 1.0 - mini_value * 0.5;
        if z.abs() < 0.01 {
            z = 0.01;
        }
        z
    });

    let pack_sync_offset_seconds = if config.machine_pack_ini_offsets {
        sync_pref_offset(pack_sync_pref, config.machine_default_sync_pref)
    } else {
        0.0
    };
    let player_global_offset_shift_seconds: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if !config.machine_allow_per_player_global_offsets || player >= num_players {
            return 0.0;
        }
        player_profiles[player]
            .global_offset_shift_ms
            .clamp(-100, 100) as f32
            / 1000.0
    });
    let mut timing_base = gameplay_charts[0].timing.clone();
    timing_base.shift_song_offset_seconds(pack_sync_offset_seconds);
    timing_base.set_global_offset_seconds(config.global_offset_seconds);
    let timing = Arc::new(timing_base);
    let mut timing_players: [Arc<TimingData>; MAX_PLAYERS] = std::array::from_fn(|player| {
        let mut t = gameplay_charts[player].timing.clone();
        t.shift_song_offset_seconds(pack_sync_offset_seconds);
        t.set_global_offset_seconds(
            config.global_offset_seconds + player_global_offset_shift_seconds[player],
        );
        Arc::new(t)
    });
    if num_players == 1 {
        timing_players[1] = timing_players[0].clone();
    }
    let mut replay_input = Vec::with_capacity(replay_edges.len());
    let replay_offsets = replay_offsets.unwrap_or(ReplayOffsetSnapshot {
        beat0_time_ns: timing_players[0].get_time_for_beat_ns(0.0),
    });
    let mut replay_out_of_order = false;
    let mut replay_prev_time_ns = INVALID_SONG_TIME_NS;
    for edge in replay_edges {
        let lane = edge.lane_index as usize;
        if lane >= num_cols || song_time_ns_invalid(edge.event_music_time_ns) {
            continue;
        }
        let player = if num_players <= 1 || cols_per_player == 0 {
            0
        } else {
            (lane / cols_per_player).min(num_players.saturating_sub(1))
        };
        let replay_beat0_shift_ns = if song_time_ns_invalid(replay_offsets.beat0_time_ns) {
            0
        } else {
            timing_players[player]
                .get_time_for_beat_ns(0.0)
                .saturating_sub(replay_offsets.beat0_time_ns)
        };
        let event_music_time_ns = edge
            .event_music_time_ns
            .saturating_add(replay_beat0_shift_ns);
        if !song_time_ns_invalid(replay_prev_time_ns) && event_music_time_ns < replay_prev_time_ns {
            replay_out_of_order = true;
        }
        replay_prev_time_ns = event_music_time_ns;
        replay_input.push(RecordedLaneEdge {
            lane_index: edge.lane_index,
            pressed: edge.pressed,
            source: edge.source,
            event_music_time_ns,
        });
    }
    if replay_out_of_order {
        replay_input.sort_by(|a, b| a.event_music_time_ns.cmp(&b.event_music_time_ns));
    }
    let replay_mode = !replay_input.is_empty();
    if replay_mode {
        debug!(
            "Gameplay replay mode enabled: {} recorded edges loaded.",
            replay_input.len(),
        );
    }
    let beat_info_cache = BeatInfoCache::new(&timing);
    let setup_ms = init_started.elapsed().as_secs_f64() * 1000.0;

    let note_build_started = Instant::now();
    let notes_cap: usize = (0..num_players)
        .map(|player| gameplay_charts[player].parsed_notes.len())
        .sum();
    let mut notes: Vec<Note> = Vec::with_capacity(notes_cap);
    let mut note_ranges = [(0usize, 0usize); MAX_PLAYERS];
    let mut holds_total: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut rolls_total: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut mines_total: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut max_row_index = 0usize;

    for player in 0..num_players {
        let timing_player = &timing_players[player];
        let parsed_notes = &gameplay_charts[player].parsed_notes;
        let start = notes.len();
        let col_offset = player.saturating_mul(cols_per_player);
        for parsed in parsed_notes {
            let row_index = parsed.row_index;
            max_row_index = max_row_index.max(row_index);

            let Some(beat) = timing_player.get_beat_for_row(row_index) else {
                continue;
            };
            let explicit_fake_tap = matches!(parsed.note_type, NoteType::Fake);
            let fake_by_segment = timing_player.is_fake_at_beat(beat);
            let is_fake = explicit_fake_tap || fake_by_segment;
            let note_type = if explicit_fake_tap {
                NoteType::Tap
            } else {
                parsed.note_type
            };

            // Pre-calculate judgability to avoid binary searches during gameplay
            let judgable_by_timing = timing_player.is_judgable_at_beat(beat);
            let can_be_judged = !is_fake && judgable_by_timing;

            if can_be_judged {
                match note_type {
                    NoteType::Hold => {
                        holds_total[player] = holds_total[player].saturating_add(1);
                    }
                    NoteType::Roll => {
                        rolls_total[player] = rolls_total[player].saturating_add(1);
                    }
                    NoteType::Mine => {
                        mines_total[player] = mines_total[player].saturating_add(1);
                    }
                    NoteType::Tap | NoteType::Lift => {}
                    NoteType::Fake => {}
                }
            }

            let hold = match (note_type, parsed.tail_row_index) {
                (NoteType::Hold | NoteType::Roll, Some(tail_row)) => timing_player
                    .get_beat_for_row(tail_row)
                    .map(|end_beat| HoldData {
                        end_row_index: tail_row,
                        end_beat,
                        result: None,
                        life: INITIAL_HOLD_LIFE,
                        let_go_started_at: None,
                        let_go_starting_life: 0.0,
                        last_held_row_index: row_index,
                        last_held_beat: beat,
                    }),
                _ => None,
            };

            let quantization_idx = quantization_index_from_beat(beat);
            notes.push(Note {
                beat,
                quantization_idx,
                column: parsed.column.saturating_add(col_offset),
                note_type,
                row_index,
                result: None,
                early_result: None,
                hold,
                mine_result: None,
                is_fake,
                can_be_judged,
            });
        }
        let end = notes.len();
        note_ranges[player] = (start, end);
    }
    let note_build_ms = note_build_started.elapsed().as_secs_f64() * 1000.0;

    let transform_started = Instant::now();
    apply_uncommon_chart_transforms(
        &mut notes,
        &mut note_ranges,
        cols_per_player,
        num_players,
        &player_profiles,
        &timing_players,
    );

    let song_seed = turn_seed_for_song(&song);
    let mut attack_song_length_seconds = song.precise_last_second();
    if !attack_song_length_seconds.is_finite() || attack_song_length_seconds <= 0.0 {
        attack_song_length_seconds = song.total_length_seconds.max(0) as f32;
    }
    let player_turn_options: [GameplayTurnOption; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player < num_players {
            gameplay_turn_option_from_profile(player_profiles[player].turn_option)
        } else {
            GameplayTurnOption::None
        }
    });
    apply_turn_options(
        &mut notes,
        note_ranges,
        cols_per_player,
        num_players,
        player_turn_options,
        song_seed,
    );
    apply_chart_attacks_transforms(
        &mut notes,
        &mut note_ranges,
        &gameplay_charts,
        cols_per_player,
        num_players,
        &player_profiles,
        &timing_players,
        song_seed,
        attack_song_length_seconds,
    );

    let mut score_valid = [true; MAX_PLAYERS];
    let mut score_missed_holds_rolls = [false; MAX_PLAYERS];
    for player in 0..num_players {
        let invalid_reasons = score_invalid_reason_lines_for_chart(
            &charts[player],
            &player_profiles[player],
            scroll_speed[player],
            rate,
        );
        score_valid[player] = invalid_reasons.is_empty();
        if !score_valid[player] {
            debug!(
                "Score validity disabled for player {} ({}): {}.",
                player + 1,
                charts[player].short_hash,
                invalid_reasons.join("; ")
            );
        }
        score_missed_holds_rolls[player] =
            judgment::score_missed_holds_and_rolls(&charts[player].chart_type);
    }

    let chart_layout_changed = (0..num_players)
        .any(|player| player_changes_chart(&gameplay_charts[player], &player_profiles[player]));
    let mut total_steps = [0u32; MAX_PLAYERS];
    let mut hands_total = [0u32; MAX_PLAYERS];
    let mut possible_grade_points = [0i32; MAX_PLAYERS];
    if chart_layout_changed {
        holds_total = [0; MAX_PLAYERS];
        rolls_total = [0; MAX_PLAYERS];
        mines_total = [0; MAX_PLAYERS];
        for player in 0..num_players {
            let totals = recompute_player_totals(&notes, note_ranges[player]);
            total_steps[player] = totals.steps;
            holds_total[player] = totals.holds;
            rolls_total[player] = totals.rolls;
            mines_total[player] = totals.mines;
            hands_total[player] = totals.hands;
            possible_grade_points[player] = judgment::max_grade_points(
                &notes,
                note_ranges[player],
                holds_total[player],
                rolls_total[player],
                charts[player].possible_grade_points,
            );
        }
    } else {
        for player in 0..num_players {
            total_steps[player] = charts[player].stats.total_steps;
            holds_total[player] = charts[player].holds_total;
            rolls_total[player] = charts[player].rolls_total;
            mines_total[player] = charts[player].mines_total;
            hands_total[player] = charts[player].stats.hands;
            possible_grade_points[player] = charts[player].possible_grade_points;
        }
    }
    if num_players == 1 {
        possible_grade_points[1] = possible_grade_points[0];
        holds_total[1] = holds_total[0];
        rolls_total[1] = rolls_total[0];
        mines_total[1] = mines_total[0];
        total_steps[1] = total_steps[0];
        hands_total[1] = hands_total[0];
        score_valid[1] = score_valid[0];
        score_missed_holds_rolls[1] = score_missed_holds_rolls[0];
        note_ranges[1] = note_ranges[0];
    }
    let note_count_stats: [Vec<NoteCountStat>; MAX_PLAYERS] =
        std::array::from_fn(|player| build_note_count_stats(&notes, note_ranges[player]));
    let transform_ms = transform_started.elapsed().as_secs_f64() * 1000.0;

    let note_player_for_col = |col: usize| -> usize {
        if num_players <= 1 || cols_per_player == 0 {
            0
        } else {
            (col / cols_per_player).min(num_players.saturating_sub(1))
        }
    };

    let cache_build_started = Instant::now();
    let mut note_time_cache_ns = Vec::with_capacity(notes.len());
    let mut hold_end_time_cache_ns = Vec::with_capacity(notes.len());
    for note in &notes {
        let timing_player = &timing_players[note_player_for_col(note.column)];
        let note_time_ns = timing_player.get_time_for_beat_ns(note.beat);
        note_time_cache_ns.push(note_time_ns);
        if let Some(hold) = note.hold.as_ref() {
            let end_time_ns = timing_player.get_time_for_beat_ns(hold.end_beat);
            hold_end_time_cache_ns.push(Some(end_time_ns));
        } else {
            hold_end_time_cache_ns.push(None);
        }
    }

    debug!("Parsed {} notes from chart data.", notes.len());

    let mut row_entries: Vec<RowEntry> = Vec::with_capacity(notes.len() / 2);
    let mut row_entry_ranges = [(0usize, 0usize); MAX_PLAYERS];
    let mut row_map_cache: [Vec<u32>; MAX_PLAYERS] =
        std::array::from_fn(|_| vec![u32::MAX; max_row_index + 1]);
    let mut note_row_entry_indices = vec![u32::MAX; notes.len()];
    let mut tap_row_hold_roll_flags = vec![0u8; notes.len()];
    for player in 0..num_players {
        let row_range_start = row_entries.len();
        let (note_start, note_end) = note_ranges[player];
        let mut cursor = note_start;
        while cursor < note_end {
            let row_index = notes[cursor].row_index;
            let row_start = cursor;
            let mut row_flags = 0u8;
            let mut nonmine_note_indices = [usize::MAX; MAX_COLS];
            let mut nonmine_note_count = 0u8;
            while cursor < note_end && notes[cursor].row_index == row_index {
                let note = &notes[cursor];
                match note.note_type {
                    NoteType::Hold => row_flags |= 0b01,
                    NoteType::Roll => row_flags |= 0b10,
                    _ => {}
                }
                if note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
                    let count = usize::from(nonmine_note_count);
                    debug_assert!(count < MAX_COLS);
                    nonmine_note_indices[count] = cursor;
                    nonmine_note_count += 1;
                }
                cursor += 1;
            }
            if nonmine_note_count != 0 {
                let row_entry_index = row_entries.len() as u32;
                row_map_cache[player][row_index] = row_entry_index;
                for &note_index in &nonmine_note_indices[..usize::from(nonmine_note_count)] {
                    note_row_entry_indices[note_index] = row_entry_index;
                }
                row_entries.push(build_row_entry(
                    row_index,
                    nonmine_note_indices,
                    nonmine_note_count,
                    &notes,
                    &note_time_cache_ns,
                ));
            }
            tap_row_hold_roll_flags[row_start..cursor].fill(row_flags);
        }
        row_entry_ranges[player] = (row_range_start, row_entries.len());
    }
    let cache_build_ms = cache_build_started.elapsed().as_secs_f64() * 1000.0;

    let timing_prep_started = Instant::now();
    let first_second = notes
        .iter()
        .zip(&note_time_cache_ns)
        .filter_map(|(n, &t_ns)| n.can_be_judged.then_some(song_time_ns_to_seconds(t_ns)))
        .reduce(f32::min)
        .unwrap_or(0.0);
    // ITGmania's ScreenGameplay::StartPlayingSong uses theme metrics
    // MinSecondsToStep / MinSecondsToMusic. Simply Love scales both by
    // MusicRate, so we apply the same here to keep real-world lead-in time
    // consistent across rates.
    let lead_in_timing = lead_in_timing.unwrap_or_default();
    let min_time_to_notes = lead_in_timing.min_seconds_to_step.max(0.0) * rate;
    let min_time_to_music = lead_in_timing.min_seconds_to_music.max(0.0) * rate;
    let mut start_delay = min_time_to_notes - first_second;
    if start_delay < min_time_to_music {
        start_delay = min_time_to_music;
    }
    if start_delay < 0.0 {
        start_delay = 0.0;
    }

    let first_note_beat = timing.get_beat_for_time(first_second);
    let initial_bpm = timing.get_bpm_for_beat(first_note_beat);

    let mut reference_bpm = get_reference_bpm_from_display_tag(&charts[0], &song.display_bpm)
        .unwrap_or_else(|| {
            let mut actual_max = timing.get_capped_max_bpm(Some(M_MOD_HIGH_CAP));
            if !actual_max.is_finite() || actual_max <= 0.0 {
                actual_max = initial_bpm.max(120.0);
            }
            actual_max
        });
    if !reference_bpm.is_finite() || reference_bpm <= 0.0 {
        reference_bpm = initial_bpm.max(120.0);
    }

    let pixels_per_second: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        let mut pps = scroll_speed[player].pixels_per_second(initial_bpm, reference_bpm, rate);
        if !pps.is_finite() || pps <= 0.0 {
            pps = ScrollSpeedSetting::default().pixels_per_second(initial_bpm, reference_bpm, rate);
        }
        pps
    });
    let draw_distance_before_targets: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return draw_distance_before_targets(viewport.height(), 1.0);
        }
        let draw_scale = player_draw_scale(&player_profiles[player]);
        draw_distance_before_targets(viewport.height(), draw_scale)
    });
    let draw_distance_after_targets: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return draw_distance_after_targets(viewport.height(), 1.0, 0.0);
        }
        let draw_scale = player_draw_scale(&player_profiles[player]);
        let centered_percent = if player_profiles[player]
            .scroll_option
            .contains(profile_data::ScrollOption::Centered)
        {
            1.0
        } else {
            0.0
        };
        draw_distance_after_targets(viewport.height(), draw_scale, centered_percent)
    });

    let travel_time: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        let mut tt = scroll_speed[player].travel_time_seconds(
            draw_distance_before_targets[player],
            initial_bpm,
            reference_bpm,
            rate,
        );
        if !tt.is_finite() || tt <= 0.0 {
            tt = draw_distance_before_targets[player] / pixels_per_second[player];
        }
        tt
    });

    let timing_profile = TimingProfile::default_itg_with_fa_plus();
    let player_judgment_timing = std::array::from_fn(|player| {
        build_player_judgment_timing(timing_profile, &player_profiles[player], rate)
    });
    let audio_end_time_ns = song_audio_end_time_ns(&song);
    let (notes_end_time_ns, music_end_time_ns) = compute_end_times_ns(
        &notes,
        &note_time_cache_ns,
        &hold_end_time_cache_ns,
        rate,
        audio_end_time_ns,
    );
    let notes_len = notes.len();
    let mut column_scroll_dirs = [1.0_f32; MAX_COLS];
    for (player, player_profile) in player_profiles.iter().enumerate().take(num_players) {
        let start = player * cols_per_player;
        let end = (start + cols_per_player).min(num_cols).min(MAX_COLS);
        let local_dirs = compute_column_scroll_dirs(player_profile.scroll_option, cols_per_player);
        for (offset, column_scroll_dir) in column_scroll_dirs[start..end].iter_mut().enumerate() {
            *column_scroll_dir = local_dirs[offset];
        }
    }

    let note_range_start: [usize; MAX_PLAYERS] =
        std::array::from_fn(|player| note_ranges[player].0);
    let row_entry_range_start: [usize; MAX_PLAYERS] =
        std::array::from_fn(|player| row_entry_ranges[player].0);
    let mut mine_note_ix: [Vec<usize>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    let mut mine_note_time_ns: [Vec<SongTimeNs>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    for player in 0..num_players {
        let (start, end) = note_ranges[player];
        let mut mine_ix = Vec::with_capacity(mines_total[player] as usize);
        let mut mine_times_ns = Vec::with_capacity(mines_total[player] as usize);
        for note_idx in start..end {
            if matches!(notes[note_idx].note_type, NoteType::Mine) {
                mine_ix.push(note_idx);
                mine_times_ns.push(note_time_cache_ns[note_idx]);
            }
        }
        mine_note_ix[player] = mine_ix;
        mine_note_time_ns[player] = mine_times_ns;
    }
    let next_mine_ix_cursor: [usize; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut lane_note_counts = [0usize; MAX_COLS];
    let mut lane_hold_counts = [0usize; MAX_COLS];
    let mut replay_cells = 0usize;
    for note in &notes {
        let col = note.column;
        if col < num_cols && col < MAX_COLS {
            lane_note_counts[col] = lane_note_counts[col].saturating_add(1);
            if note_has_displayable_hold(note) {
                lane_hold_counts[col] = lane_hold_counts[col].saturating_add(1);
            }
        }
        if note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
            replay_cells = replay_cells.saturating_add(1);
        }
    }
    let mut lane_note_indices: [Vec<usize>; MAX_COLS] =
        std::array::from_fn(|col| Vec::with_capacity(lane_note_counts[col]));
    let mut lane_hold_indices: [Vec<usize>; MAX_COLS] =
        std::array::from_fn(|col| Vec::with_capacity(lane_hold_counts[col]));
    for (note_index, note) in notes.iter().enumerate() {
        let col = note.column;
        if col < num_cols && col < MAX_COLS {
            lane_note_indices[col].push(note_index);
            if note_has_displayable_hold(note) {
                lane_hold_indices[col].push(note_index);
            }
        }
    }
    let mut lane_note_row_indices = lane_note_indices.clone();
    for indices in lane_note_row_indices
        .iter_mut()
        .take(num_cols.min(MAX_COLS))
    {
        indices.sort_unstable_by_key(|&note_index| {
            (beat_to_note_row(notes[note_index].beat), note_index)
        });
    }
    let pending_edges_capacity = input_queue_cap(num_cols);
    let replay_seconds = (song_time_ns_to_seconds(music_end_time_ns) + start_delay)
        .max(song_time_ns_to_seconds(notes_end_time_ns) + start_delay);
    let replay_capture_enabled = !replay_mode && config.machine_enable_replays;
    let replay_edges_capacity = [
        0,
        replay_edge_cap(num_cols, replay_cells, replay_mode, replay_seconds),
    ][replay_capture_enabled as usize];
    let decaying_hold_capacity = (0..num_players).fold(0usize, |acc, player| {
        acc.saturating_add(holds_total[player] as usize + rolls_total[player] as usize)
    });
    let timing_prep_ms = timing_prep_started.elapsed().as_secs_f64() * 1000.0;

    let hud_prep_started = Instant::now();
    let global_visual_delay_seconds = config.visual_delay_seconds;
    let player_visual_delay_seconds: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return 0.0;
        }
        let ms = player_profiles[player].visual_delay_ms.clamp(-100, 100);
        ms as f32 / 1000.0
    });
    let init_music_time = -start_delay;
    let init_music_time_ns = song_time_ns_from_seconds(init_music_time);
    let init_beat = timing.get_beat_for_time_ns(init_music_time_ns);
    let current_music_time_visible_ns: [SongTimeNs; MAX_PLAYERS] = std::array::from_fn(|player| {
        let delay = global_visual_delay_seconds + player_visual_delay_seconds[player];
        visible_notefield_time_ns(init_music_time_ns, delay)
    });
    let current_music_time_visible: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        song_time_ns_to_seconds(current_music_time_visible_ns[player])
    });
    let current_beat_visible: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        timing_players[player].get_beat_for_time_ns(current_music_time_visible_ns[player])
    });
    let (
        song_lua_mask_windows,
        song_lua_ease_windows,
        song_lua_overlays,
        song_lua_overlay_eases,
        song_lua_overlay_ease_ranges,
        song_lua_overlay_events,
        song_lua_background_visual_layers,
        song_lua_foreground_visual_layers,
        song_lua_player_actors,
        song_lua_player_events,
        song_lua_song_foreground,
        song_lua_song_foreground_events,
        song_lua_hidden_players,
        song_lua_note_hides,
        song_lua_column_offsets,
        song_lua_screen_width,
        song_lua_screen_height,
    ) = build_song_lua_runtime_windows(
        &song,
        &timing_players,
        num_players,
        &player_profiles,
        config.global_offset_seconds,
        viewport,
        &session,
        config.center_1player_notefield,
        &player_global_offset_shift_seconds,
        song_lua_data,
    );
    let attack_mask_windows: [Vec<AttackMaskWindow>; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return Vec::new();
        }
        let mut windows = if player_profiles[player].attack_mode == profile_data::AttackMode::Off {
            Vec::new()
        } else {
            build_attack_mask_windows_for_player(
                gameplay_charts[player].chart_attacks.as_deref(),
                player_profiles[player].attack_mode,
                player,
                song_seed,
                attack_song_length_seconds,
            )
        };
        windows.extend(song_lua_mask_windows[player].iter().copied());
        windows
    });
    let reverse_scroll: [bool; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return false;
        }
        player_profiles[player].reverse_scroll
    });
    let mut column_cues: [Vec<ColumnCue>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    for player in 0..num_players {
        if !player_profiles[player].column_cues {
            continue;
        }
        let col_start = player.saturating_mul(cols_per_player);
        let col_end = (col_start + cols_per_player).min(num_cols);
        column_cues[player] = build_column_cues_for_player(
            &notes,
            note_ranges[player],
            &note_time_cache_ns,
            col_start,
            col_end,
            current_music_time_visible[player],
        );
    }
    if num_players == 1 {
        let (first, second) = column_cues.split_at_mut(1);
        second[0].clone_from(&first[0]);
    }

    let mut crossover_cues: [Vec<ColumnCue>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    for player in 0..num_players {
        if !player_profiles[player].crossover_cues {
            continue;
        }
        let col_start = player.saturating_mul(cols_per_player);
        crossover_cues[player] = build_crossover_cues_for_player(
            &notes,
            note_ranges[player],
            &gameplay_charts[player].timing_segments,
            &timing_players[player],
            cols_per_player,
            col_start,
            player_profiles[player].crossover_cue_duration_ms,
            player_profiles[player].crossover_cue_quantization,
            player_profiles[player].crossover_cue_brackets,
            current_music_time_visible[player],
        );
    }
    if num_players == 1 {
        let (first, second) = crossover_cues.split_at_mut(1);
        second[0].clone_from(&first[0]);
    }

    let measure_densities: [Vec<usize>; MAX_PLAYERS] = std::array::from_fn(|p| {
        if p >= num_players || !needs_stream_data(&player_profiles[p]) {
            return Vec::new();
        }
        measure_densities(&gameplay_charts[p].notes, cols_per_player)
    });

    let measure_counter_segments: [Vec<StreamSegment>; MAX_PLAYERS] = std::array::from_fn(|p| {
        if p >= num_players {
            return Vec::new();
        }
        let Some(threshold) = player_profiles[p].measure_counter.notes_threshold() else {
            return Vec::new();
        };
        stream_sequences_threshold(&measure_densities[p], threshold)
    });

    let mut mini_indicator_stream_segments: [Vec<StreamSegment>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut mini_indicator_total_stream_measures = [0.0_f32; MAX_PLAYERS];
    let mut mini_indicator_target_score_percent = [89.0_f64; MAX_PLAYERS];
    let mut mini_indicator_rival_score_percent = [0.0_f64; MAX_PLAYERS];

    for p in 0..num_players {
        if mini_indicator_mode(&player_profiles[p]) == profile_data::MiniIndicator::None {
            continue;
        }
        let constant_bpm = !timing_players[p].has_bpm_changes();
        let (stream_segments, total_stream, _total_break) =
            zmod_stream_totals_full_measures(&measure_densities[p], constant_bpm);
        mini_indicator_total_stream_measures[p] = total_stream.max(0.0);
        mini_indicator_stream_segments[p] = stream_segments;

        let personal_best = mini_indicator_data.personal_best_percent[p];
        let machine_best = mini_indicator_data.machine_best_percent[p];

        let target = match player_profiles[p].target_score {
            profile_data::TargetScoreSetting::MachineBest => machine_best.or(personal_best),
            profile_data::TargetScoreSetting::PersonalBest => personal_best,
            setting => target_score_setting_percent(setting),
        }
        .unwrap_or(89.0);
        mini_indicator_target_score_percent[p] = target;

        mini_indicator_rival_score_percent[p] = machine_best
            .unwrap_or(0.0)
            .max(personal_best.unwrap_or(0.0));
    }

    let hud_prep_ms = hud_prep_started.elapsed().as_secs_f64() * 1000.0;

    let graph_prep_started = Instant::now();
    let wants_density_graph = player_profiles.iter().take(num_players).any(|p| {
        p.step_statistics
            .contains(profile_data::StepStatisticsMask::DENSITY_GRAPH)
    });
    let wide = viewport.is_wide();
    let density_graph_enabled = wide && wants_density_graph;
    let sw = viewport.width();
    let sh = viewport.height().max(1.0_f32);
    let density_graph_graph_h = if density_graph_enabled {
        105.0_f32
    } else {
        0.0_f32
    };
    let density_graph_graph_w = if density_graph_enabled {
        step_stats_density_graph_width(
            play_style,
            cols_per_player,
            num_players,
            sw,
            sh,
            wide,
            config.center_1player_notefield,
        )
    } else {
        0.0_f32
    };
    let density_graph_first_second = timing.get_time_for_beat(0.0).min(0.0_f32);
    let density_graph_last_second = song.precise_last_second();
    let density_graph_duration =
        (density_graph_last_second - density_graph_first_second).max(0.001_f32);

    const DENSITY_GRAPH_MAX_SECONDS: f32 = 4.0 * 60.0;
    let density_graph_scaled_width =
        if density_graph_enabled && density_graph_duration > DENSITY_GRAPH_MAX_SECONDS {
            (density_graph_graph_w * (density_graph_duration / DENSITY_GRAPH_MAX_SECONDS))
                .round()
                .max(density_graph_graph_w)
        } else {
            density_graph_graph_w
        };
    let density_graph_u_window =
        if density_graph_enabled && density_graph_duration > DENSITY_GRAPH_MAX_SECONDS {
            (DENSITY_GRAPH_MAX_SECONDS / density_graph_duration).clamp(0.0_f32, 1.0_f32)
        } else {
            1.0_f32
        };
    let density_graph_u0 = 0.0_f32;
    let density_graph_top_h = 30.0_f32;
    let density_graph_top_w: [f32; MAX_PLAYERS] = std::array::from_fn(|p| {
        if p >= num_players || !player_profiles[p].nps_graph_at_top {
            return 0.0;
        }
        upper_density_graph_width(play_style)
    });
    let density_graph_top_scale_y: [f32; MAX_PLAYERS] = {
        let mut scale = [1.0_f32; MAX_PLAYERS];
        if num_players == 2
            && player_profiles[0].nps_graph_at_top
            && player_profiles[1].nps_graph_at_top
        {
            let p1_peak = charts[0].max_nps as f32;
            let p2_peak = charts[1].max_nps as f32;
            if p1_peak.is_finite() && p2_peak.is_finite() && p1_peak > 0.0 && p2_peak > 0.0 {
                if p1_peak < p2_peak {
                    scale[0] = (p1_peak / p2_peak).clamp(0.0, 1.0);
                } else if p2_peak < p1_peak {
                    scale[1] = (p2_peak / p1_peak).clamp(0.0, 1.0);
                }
            }
        }
        scale
    };
    let mut density_graph_life_update_rate = 0.25_f32;
    if density_graph_enabled && !timing.has_bpm_changes() {
        let bpm = timing.first_bpm();
        if bpm.is_finite() && bpm >= 60.0_f32 {
            let interval_8th = (60.0_f32 / bpm) * 0.5_f32;
            if interval_8th.is_finite() && interval_8th > 0.0_f32 {
                density_graph_life_update_rate =
                    interval_8th * (density_graph_life_update_rate / interval_8th).ceil();
            }
        }
    }
    if !density_graph_life_update_rate.is_finite() || density_graph_life_update_rate <= 0.0_f32 {
        density_graph_life_update_rate = 0.25_f32;
    }
    let density_graph_life_next_update_elapsed = 0.0_f32;
    let density_graph_life_points: [Vec<[f32; 2]>; MAX_PLAYERS] = std::array::from_fn(|p| {
        if density_graph_enabled && p < num_players {
            Vec::with_capacity(1024)
        } else {
            Vec::new()
        }
    });
    let density_graph_life_dirty: [bool; MAX_PLAYERS] = [false; MAX_PLAYERS];
    let graph_prep_ms = graph_prep_started.elapsed().as_secs_f64() * 1000.0;

    let finalize_started = Instant::now();
    let mut players = std::array::from_fn(|_| init_player_runtime());
    let in_course_stage = course_display_totals.is_some();
    for p in 0..num_players {
        if in_course_stage {
            init_course_submit_life(&mut players[p]);
        }
        let course_carry = course_display_carry.as_ref().map(|carry| carry[p]);
        apply_course_life_carry(&mut players[p], course_carry);
        apply_course_combo_carry(
            &mut players[p],
            player_profiles[p].carry_combo_between_songs,
            replay_mode,
            combo_carry[p],
            course_carry,
        );
        let life = players[p].life;
        players[p].life_history.push((init_music_time, life));
    }
    let assist_clap_rows = build_assist_clap_rows(&notes, note_ranges[0]);
    let song_offset_seconds = song.offset;
    let base_attack_appearance = std::array::from_fn(|player| {
        if player < num_players {
            base_appearance_effects(&player_profiles[player])
        } else {
            AppearanceEffects::default()
        }
    });
    let tick_mode = session.tick_mode;

    let mut state = State {
        song,
        charts,
        gameplay_charts,
        num_cols,
        cols_per_player,
        num_players,
        viewport,
        session,
        config,
        audio_commands: Vec::with_capacity(8),
        session_commands: Vec::with_capacity(2),
        timing,
        timing_players,
        beat_info_cache,
        timing_profile,
        player_judgment_timing,
        notes,
        note_ranges,
        note_count_stats,
        audio_lead_in_seconds: start_delay,
        audio_stream_position_seconds: 0.0,
        audio_output_delay_seconds: 0.0,
        current_beat: init_beat,
        current_music_time_ns: song_time_ns_from_seconds(init_music_time),
        current_beat_display: init_beat,
        current_music_time_display: init_music_time,
        display_clock: FrameStableDisplayClock::new(song_time_ns_from_seconds(init_music_time)),
        display_clock_diag: DisplayClockDiagRing::new(),
        lane_note_indices,
        lane_note_row_indices,
        lane_hold_indices,
        row_entry_ranges,
        judged_row_cursor: row_entry_range_start,
        note_time_cache_ns,
        hold_end_time_cache_ns,
        notes_end_time_ns,
        music_end_time_ns,
        audio_end_time_ns,
        music_rate: rate,
        play_mine_sounds: config.mine_hit_sound,
        default_fail_type: config.default_fail_type,
        global_offset_seconds: config.global_offset_seconds,
        initial_global_offset_seconds: config.global_offset_seconds,
        player_global_offset_shift_seconds,
        song_offset_seconds,
        initial_song_offset_seconds: song_offset_seconds,
        autosync_mode: AutosyncMode::Off,
        autosync_offset_samples: [0; AUTOSYNC_OFFSET_SAMPLE_COUNT],
        autosync_offset_sample_count: 0,
        autosync_standard_deviation: 0.0,
        global_visual_delay_seconds,
        player_visual_delay_seconds,
        current_music_time_visible_ns,
        current_music_time_visible,
        current_beat_visible,
        next_tap_miss_cursor: note_range_start,
        next_mine_avoid_cursor: note_range_start,
        mine_note_ix,
        mine_note_time_ns,
        next_mine_ix_cursor,
        pending_mine_hit_indices: Vec::new(),
        row_entries,
        measure_counter_segments,
        column_cues,
        crossover_cues,
        mini_indicator_stream_segments,
        mini_indicator_total_stream_measures,
        mini_indicator_target_score_percent,
        mini_indicator_rival_score_percent,
        row_map_cache,
        note_row_entry_indices,
        tap_row_hold_roll_flags,
        decaying_hold_indices: Vec::with_capacity(decaying_hold_capacity),
        hold_decay_active: vec![false; notes_len],
        tap_miss_held_window: vec![false; notes_len],
        pending_missed_hold_resolution: vec![false; notes_len],
        pending_missed_hold_indices: Vec::new(),
        players,
        hold_judgments: Default::default(),
        held_miss_judgments: Default::default(),
        is_in_freeze: false,
        is_in_delay: false,
        possible_grade_points,
        song_completed_naturally: false,
        autoplay_enabled: replay_mode,
        autoplay_used: replay_mode,
        score_valid,
        score_missed_holds_rolls,
        replay_mode,
        replay_capture_enabled,
        course_display_carry,
        course_display_totals,
        course_display_timing,
        live_window_counts: [deadsync_rules::timing::WindowCounts::default(); MAX_PLAYERS],
        live_window_counts_10ms_blue: [deadsync_rules::timing::WindowCounts::default();
            MAX_PLAYERS],
        live_window_counts_display_blue: [deadsync_rules::timing::WindowCounts::default();
            MAX_PLAYERS],
        player_profiles,
        attack_mask_windows,
        song_lua_ease_windows,
        song_lua_overlays,
        song_lua_overlay_eases,
        song_lua_overlay_ease_ranges,
        song_lua_overlay_events,
        song_lua_background_visual_layers,
        song_lua_foreground_visual_layers,
        song_lua_player_actors,
        song_lua_player_events,
        song_lua_song_foreground,
        song_lua_song_foreground_events,
        song_lua_hidden_players,
        song_lua_note_hides,
        song_lua_column_offsets,
        song_lua_screen_width,
        song_lua_screen_height,
        song_lua_player_x: [None; MAX_PLAYERS],
        song_lua_player_y: [None; MAX_PLAYERS],
        song_lua_player_z: [0.0; MAX_PLAYERS],
        song_lua_player_rotation_x: [0.0; MAX_PLAYERS],
        song_lua_player_rotation_z: [0.0; MAX_PLAYERS],
        song_lua_player_rotation_y: [0.0; MAX_PLAYERS],
        song_lua_player_skew_x: [0.0; MAX_PLAYERS],
        song_lua_player_skew_y: [0.0; MAX_PLAYERS],
        song_lua_player_zoom_x: [1.0; MAX_PLAYERS],
        song_lua_player_zoom_y: [1.0; MAX_PLAYERS],
        song_lua_player_zoom_z: [1.0; MAX_PLAYERS],
        song_lua_player_confusion_y_offset: [0.0; MAX_PLAYERS],
        active_attack_clear_all: [false; MAX_PLAYERS],
        active_attack_chart: [ChartAttackEffects::default(); MAX_PLAYERS],
        active_attack_accel: [AccelOverrides::default(); MAX_PLAYERS],
        active_attack_visual: [VisualOverrides::default(); MAX_PLAYERS],
        attacks_cleared_for_outro: false,
        outro_attack_visual: [VisualOverrides::default(); MAX_PLAYERS],
        attack_current_appearance: base_attack_appearance,
        attack_target_appearance: base_attack_appearance,
        attack_speed_appearance: [AppearanceEffects::approach_speeds(); MAX_PLAYERS],
        active_attack_appearance: base_attack_appearance,
        active_attack_visibility: [VisibilityOverrides::default(); MAX_PLAYERS],
        active_attack_scroll: [ScrollOverrides::default(); MAX_PLAYERS],
        active_attack_perspective: [PerspectiveOverrides::default(); MAX_PLAYERS],
        active_attack_scroll_speed: [None; MAX_PLAYERS],
        active_attack_mini_percent: [None; MAX_PLAYERS],
        noteskin_effects,
        active_color_index,
        player_color_index,
        scroll_speed,
        scroll_reference_bpm: reference_bpm,
        field_zoom,
        scroll_pixels_per_second: pixels_per_second,
        scroll_travel_time: travel_time,
        draw_distance_before_targets,
        draw_distance_after_targets,
        reverse_scroll,
        column_scroll_dirs,
        receptor_glow_timers: [0.0; MAX_COLS],
        receptor_glow_press_timers: [0.0; MAX_COLS],
        receptor_glow_lift_start_alpha: [0.0; MAX_COLS],
        receptor_glow_lift_start_zoom: [1.0; MAX_COLS],
        receptor_bop_timers: [0.0; MAX_COLS],
        receptor_bop_behaviors: [GameplayReceptorStepBehavior::identity(); MAX_COLS],
        tap_explosions: Default::default(),
        column_flashes: Default::default(),
        last_tap_judgments: Default::default(),
        mine_explosions: Default::default(),
        active_holds: Default::default(),
        holds_total,
        rolls_total,
        mines_total,
        total_steps,
        hands_total,
        total_elapsed_in_screen: 0.0,
        danger_fx: std::array::from_fn(|_| DangerFx::default()),
        density_graph_first_second,
        density_graph_last_second,
        density_graph_duration,
        density_graph_graph_w,
        density_graph_graph_h,
        density_graph_scaled_width,
        density_graph_u0,
        density_graph_u_window,
        density_graph_life_update_rate,
        density_graph_life_next_update_elapsed,
        density_graph_life_points,
        density_graph_life_dirty,
        density_graph_top_h,
        density_graph_top_w,
        density_graph_top_scale_y,
        hold_to_exit_key: None,
        hold_to_exit_start: None,
        hold_to_exit_aborted_at: None,
        exit_transition: None,
        shift_held: false,
        ctrl_held: false,
        offset_adjust_held_since: [None; 2],
        offset_adjust_last_at: [None; 2],
        prev_inputs: [false; MAX_COLS],
        input_slots: [EMPTY_ACTIVE_INPUT_SLOT; MAX_ACTIVE_INPUT_SLOTS],
        input_slot_count: 0,
        input_lane_counts: [0; MAX_COLS],
        lane_pressed_since_ns: [None; MAX_COLS],
        pending_edges: VecDeque::with_capacity(pending_edges_capacity),
        autoplay_rng: TurnRng::new(song_seed ^ 0xA17F_0FF5_EED5_1EED),
        autoplay_cursor: note_range_start,
        tick_mode,
        assist_clap_rows,
        assist_clap_cursor: 0,
        assist_last_crossed_row: -1,
        assist_sfx_gen_seen: 0,
        toggle_flash_text: None,
        toggle_flash_timer: 0.0,
        replay_input,
        replay_cursor: 0,
        replay_edges: Vec::with_capacity(replay_edges_capacity),
        update_trace: GameplayUpdateTraceState::default(),
    };
    state.update_trace = GameplayUpdateTraceState::from_state(&state);
    refresh_active_attack_masks(&mut state, 0.0);
    let current_bpm = state.timing.get_bpm_for_beat(state.current_beat);
    refresh_live_notefield_options(&mut state, current_bpm);
    let finalize_ms = finalize_started.elapsed().as_secs_f64() * 1000.0;
    let total_ms = init_started.elapsed().as_secs_f64() * 1000.0;
    if total_ms >= 50.0 {
        info!(
            "Gameplay init timing: song='{}' notes={} players={} density_graph={} setup_ms={setup_ms:.3} note_build_ms={note_build_ms:.3} transform_ms={transform_ms:.3} cache_ms={cache_build_ms:.3} timing_ms={timing_prep_ms:.3} hud_ms={hud_prep_ms:.3} graph_ms={graph_prep_ms:.3} finalize_ms={finalize_ms:.3} elapsed_ms={total_ms:.3}",
            state.song.title,
            state.notes.len(),
            state.num_players,
            density_graph_enabled,
        );
    } else {
        debug!(
            "Gameplay init timing: song='{}' notes={} players={} density_graph={} setup_ms={setup_ms:.3} note_build_ms={note_build_ms:.3} transform_ms={transform_ms:.3} cache_ms={cache_build_ms:.3} timing_ms={timing_prep_ms:.3} hud_ms={hud_prep_ms:.3} graph_ms={graph_prep_ms:.3} finalize_ms={finalize_ms:.3} elapsed_ms={total_ms:.3}",
            state.song.title,
            state.notes.len(),
            state.num_players,
            density_graph_enabled,
        );
    }
    state
}

fn update_itg_grade_totals(p: &mut PlayerRuntime) {
    p.earned_grade_points = judgment::calculate_itg_grade_points_from_counts(
        &p.scoring_counts,
        p.holds_held_for_score,
        p.rolls_held_for_score,
        p.mines_hit_for_score,
    );
}

#[inline(always)]
fn timing_hit_log_enabled() -> bool {
    log::log_enabled!(log::Level::Debug)
}

#[inline(always)]
pub(super) fn gameplay_input_log_enabled() -> bool {
    log::log_enabled!(log::Level::Debug)
}

#[inline(always)]
fn log_tap_judge_candidate(
    enabled: bool,
    reason: &str,
    player: usize,
    column: usize,
    current_row_index: usize,
    current_time_ns: SongTimeNs,
    note_index: usize,
    note: &Note,
    note_time_ns: SongTimeNs,
    rate: f32,
) {
    if !enabled {
        return;
    }
    let offset_music_ns = current_time_ns.saturating_sub(note_time_ns);
    debug!(
        concat!(
            "GAMEPLAY TAP JUDGE: reason={}, player={}, lane={}, note_index={}, ",
            "note_col={}, note_type={:?}, note_row={}, current_row={}, beat={:.3}, ",
            "quant={}, fake={}, can_be_judged={}, result_set={}, early_result_set={}, ",
            "note_time_s={:.6}, event_time_s={:.6}, offset_ms={:.2}, rate={:.3}"
        ),
        reason,
        player,
        column,
        note_index,
        note.column,
        note.note_type,
        note.row_index,
        current_row_index,
        note.beat,
        note.quantization_idx,
        note.is_fake,
        note.can_be_judged,
        note.result.is_some(),
        note.early_result.is_some(),
        song_time_ns_to_seconds(note_time_ns),
        song_time_ns_to_seconds(current_time_ns),
        judgment_time_error_ms_from_music_ns(offset_music_ns, rate),
        rate,
    );
}

#[inline(always)]
fn log_timing_hit_detail(
    enabled: bool,
    stream_pos_s: f32,
    grade: JudgeGrade,
    row_index: usize,
    col: usize,
    beat: f32,
    song_offset_s: f32,
    global_offset_s: f32,
    note_time_ns: SongTimeNs,
    event_time_ns: SongTimeNs,
    music_now_s: f32,
    rate: f32,
    lead_in_s: f32,
) {
    if !enabled {
        return;
    }
    let note_time_s = song_time_ns_to_seconds(note_time_ns);
    let event_time_s = song_time_ns_to_seconds(event_time_ns);
    let expected_stream_for_note_s =
        note_time_s / rate + lead_in_s + global_offset_s * (1.0 - rate) / rate;
    let expected_stream_for_hit_s =
        event_time_s / rate + lead_in_s + global_offset_s * (1.0 - rate) / rate;
    let stream_delta_note_ms = (stream_pos_s - expected_stream_for_note_s) * 1000.0;
    let stream_delta_hit_ms = (stream_pos_s - expected_stream_for_hit_s) * 1000.0;
    debug!(
        concat!(
            "TIMING HIT: grade={:?}, row={}, col={}, beat={:.3}, ",
            "song_offset_s={:.4}, global_offset_s={:.4}, ",
            "note_time_s={:.6}, event_time_s={:.6}, music_now_s={:.6}, ",
            "offset_ms={:.2}, rate={:.3}, lead_in_s={:.4}, ",
            "stream_pos_s={:.6}, stream_note_s={:.6}, stream_delta_note_ms={:.2}, ",
            "stream_hit_s={:.6}, stream_delta_hit_ms={:.2}"
        ),
        grade,
        row_index,
        col,
        beat,
        song_offset_s,
        global_offset_s,
        note_time_s,
        event_time_s,
        music_now_s,
        ((event_time_s - note_time_s) / rate) * 1000.0,
        rate,
        lead_in_s,
        stream_pos_s,
        expected_stream_for_note_s,
        stream_delta_note_ms,
        expected_stream_for_hit_s,
        stream_delta_hit_ms,
    );
}

fn tap_judgment_uses_bright_explosion(state: &State, player: usize, judgment: &Judgment) -> bool {
    let Some(profile) = state.player_profiles.get(player) else {
        return false;
    };
    tap_judgment_uses_bright_explosion_for_options(
        FantasticFeedbackOptions {
            show_fa_plus_window: profile.show_fa_plus_window,
            fa_plus_10ms_blue_window: profile.fa_plus_10ms_blue_window,
            split_15_10ms: profile.split_15_10ms,
            custom_fantastic_window: profile.custom_fantastic_window,
        },
        judgment,
    )
}

#[inline(always)]
fn trigger_column_flash(state: &mut State, column: usize, grade: JudgeGrade, blue_fantastic: bool) {
    if column >= state.column_flashes.len() {
        return;
    }
    // Record the judgement unconditionally for feedback consumers (SMX panel lighting),
    // before the on-screen column-flash mask gate below.
    state.last_tap_judgments[column] = Some(ColumnTapJudgment {
        grade,
        blue_fantastic,
        at_screen_s: state.total_elapsed_in_screen,
    });
    let player = player_for_col(state, column);
    let Some(profile) = state.player_profiles.get(player) else {
        return;
    };
    if !profile.column_flash_on_miss
        || !profile_data::column_flash_mask_enabled(
            profile.column_flash_mask,
            grade,
            blue_fantastic,
        )
    {
        return;
    }
    state.column_flashes[column] = Some(ActiveColumnFlash {
        grade,
        blue_fantastic,
        started_at_screen_s: state.total_elapsed_in_screen,
    });
}

#[inline(always)]
pub(super) fn trigger_column_flash_for_grade(state: &mut State, column: usize, grade: JudgeGrade) {
    trigger_column_flash(state, column, grade, false);
}

#[inline(always)]
fn trigger_column_flash_for_judgment(
    state: &mut State,
    player: usize,
    column: usize,
    judgment: &Judgment,
) {
    let blue_fantastic = judgment.grade == JudgeGrade::Fantastic
        && !tap_judgment_uses_bright_explosion(state, player, judgment);
    trigger_column_flash(state, column, judgment.grade, blue_fantastic);
}

fn trigger_tap_judgment_explosion(
    state: &mut State,
    player: usize,
    column: usize,
    judgment: &Judgment,
) {
    trigger_column_flash_for_judgment(state, player, column, judgment);
    let Some(window_key) = grade_to_window(judgment.grade) else {
        return;
    };
    let bright = tap_judgment_uses_bright_explosion(state, player, judgment);
    spawn_tap_explosion(state, column, window_key, bright);
}

#[cfg(test)]
fn trigger_tap_explosion(state: &mut State, column: usize, grade: JudgeGrade) {
    trigger_column_flash_for_grade(state, column, grade);
    spawn_tap_explosion_for_grade(state, column, grade, false);
}

fn spawn_tap_explosion_for_grade(
    state: &mut State,
    column: usize,
    grade: JudgeGrade,
    bright: bool,
) {
    let Some(window_key) = grade_to_window(grade) else {
        return;
    };
    spawn_tap_explosion(state, column, window_key, bright);
}

pub(super) fn trigger_hold_explosion(state: &mut State, column: usize) {
    // Hold success uses the noteskin's `HeldCommand` (matching ITGMania), which
    // is plumbed through the parser as the "Held" pseudo-window.
    spawn_tap_explosion(state, column, "Held", false);
}

fn spawn_tap_explosion(state: &mut State, column: usize, window_key: &'static str, bright: bool) {
    let player = player_for_col(state, column);
    if !state.player_profiles[player].tap_explosion_window_enabled(window_key) {
        return;
    }
    let local_col = if state.cols_per_player == 0 {
        column
    } else {
        column % state.cols_per_player
    };
    let spawn_duration = state
        .noteskin_effects
        .tap_explosion_duration(player, local_col, window_key, bright);
    if let Some(duration) = spawn_duration {
        state.tap_explosions[column] = Some(ActiveTapExplosion {
            window: window_key,
            bright,
            elapsed: 0.0,
            duration,
            start_beat: state.current_beat,
        });
    }
}

fn trigger_mine_explosion(state: &mut State, column: usize) {
    let player = player_for_col(state, column);
    let duration = state.noteskin_effects.mine_explosion_duration(player);
    state.mine_explosions[column] = Some(ActiveMineExplosion {
        elapsed: 0.0,
        duration,
        started_at_screen_s: state.total_elapsed_in_screen,
    });
    if state.play_mine_sounds {
        queue_preloaded_sfx(state, "assets/sounds/boom.ogg");
    }
}

#[inline(always)]
fn player_combo_state(p: &PlayerRuntime) -> ComboState {
    ComboState {
        combo: p.combo,
        miss_combo: p.miss_combo,
        full_combo_grade: p.full_combo_grade,
        current_combo_grade: p.current_combo_grade,
        first_fc_attempt_broken: p.first_fc_attempt_broken,
    }
}

#[inline(always)]
fn write_player_combo_state(p: &mut PlayerRuntime, state: ComboState) {
    p.combo = state.combo;
    p.miss_combo = state.miss_combo;
    p.full_combo_grade = state.full_combo_grade;
    p.current_combo_grade = state.current_combo_grade;
    p.first_fc_attempt_broken = state.first_fc_attempt_broken;
}

#[inline(always)]
fn apply_combo_update(p: &mut PlayerRuntime, update: combo_rules::ComboUpdate) {
    if update.combo_broken {
        p.current_combo_window_counts = deadsync_rules::timing::WindowCounts::default();
    }
    if update.hit_thousand_milestone {
        trigger_combo_milestone(&mut p.combo_milestones, ComboMilestoneKind::Thousand);
    }
    if update.hit_hundred_milestone {
        trigger_combo_milestone(&mut p.combo_milestones, ComboMilestoneKind::Hundred);
    }
}

#[inline(always)]
fn clear_full_combo_state(p: &mut PlayerRuntime) {
    let mut state = player_combo_state(p);
    combo_rules::clear_full_combo_state(&mut state);
    write_player_combo_state(p, state);
}

#[inline(always)]
fn break_combo_state(p: &mut PlayerRuntime, miss_combo_delta: u32) {
    let mut state = player_combo_state(p);
    let update = combo_rules::break_combo_state(&mut state, miss_combo_delta);
    write_player_combo_state(p, state);
    apply_combo_update(p, update);
}

#[inline(always)]
fn apply_row_combo_state(
    p: &mut PlayerRuntime,
    final_grade: JudgeGrade,
    row_combo_count: u32,
    miss_combo_count: u32,
) {
    let mut state = player_combo_state(p);
    let update =
        apply_rules_row_combo_state(&mut state, final_grade, row_combo_count, miss_combo_count);
    write_player_combo_state(p, state);
    apply_combo_update(p, update);
}

#[inline(always)]
fn apply_mine_hit_combo_state(p: &mut PlayerRuntime) {
    if MINE_HIT_INCREMENTS_MISS_COMBO {
        break_combo_state(p, 1);
    }
}

#[inline(always)]
fn apply_hold_success_combo_state(_p: &mut PlayerRuntime) {
    // ITG dance/pump scoring does not let Held / Roll Held reset miss combo.
}

fn hit_mine(
    state: &mut State,
    column: usize,
    note_index: usize,
    time_error_music_ns: SongTimeNs,
) -> bool {
    let player = player_for_col(state, column);
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let mine_window_music_ns = state.player_judgment_timing[player]
        .profile_music_ns
        .mine_window_ns;
    if i128::from(time_error_music_ns).abs() > i128::from(mine_window_music_ns) {
        return false;
    }
    if state.notes[note_index].mine_result.is_some() || state.notes[note_index].is_fake {
        return false;
    }
    if !state.notes[note_index].can_be_judged {
        return false;
    }

    state.notes[note_index].mine_result = Some(MineResult::Hit);
    state.pending_mine_hit_indices.push(note_index);
    let note_time_ns = state.note_time_cache_ns[note_index];
    let hit_time_ns = note_time_ns.saturating_add(time_error_music_ns);
    debug!(
        "JUDGE MINE HIT MARKED: row={}, col={}, beat={:.3}, note_time={:.4}s, hit_time={:.4}s, offset_ms={:.2}, rate={:.3}",
        state.notes[note_index].row_index,
        column,
        state.notes[note_index].beat,
        song_time_ns_to_seconds(note_time_ns),
        song_time_ns_to_seconds(hit_time_ns),
        judgment_time_error_ms_from_music_ns(time_error_music_ns, rate),
        rate
    );
    true
}

fn apply_pending_mine_hits(state: &mut State) {
    if state.pending_mine_hit_indices.is_empty() {
        return;
    }

    let pending = std::mem::take(&mut state.pending_mine_hit_indices);
    let scoring_blocked = autoplay_blocks_scoring(state);
    let current_music_time = current_music_time_s(state);

    for note_index in pending {
        let Some(note) = state.notes.get(note_index) else {
            continue;
        };
        if note.mine_result != Some(MineResult::Hit) || note.is_fake || !note.can_be_judged {
            continue;
        }

        let column = note.column;
        let player = player_for_col(state, column);
        if !scoring_blocked {
            state.players[player].mines_hit = state.players[player].mines_hit.saturating_add(1);
        }

        let mut updated_scoring = false;
        if !scoring_blocked {
            apply_life_change(
                &mut state.players[player],
                current_music_time,
                LIFE_HIT_MINE,
            );
            capture_failed_ex_score_inputs(state, player);
            if !is_state_dead(state, player) {
                state.players[player].mines_hit_for_score =
                    state.players[player].mines_hit_for_score.saturating_add(1);
                updated_scoring = true;
            }
            apply_mine_hit_combo_state(&mut state.players[player]);
        }

        state.receptor_glow_timers[column] = 0.0;
        trigger_mine_explosion(state, column);
        set_last_mine_judgment(state, player, column, MineResult::Hit);
        if updated_scoring {
            update_itg_grade_totals(&mut state.players[player]);
        }
    }
}

#[inline(always)]
fn try_hit_crossed_mines_while_held(
    state: &mut State,
    column: usize,
    prev_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
) -> bool {
    if song_time_ns_invalid(prev_time_ns)
        || song_time_ns_invalid(current_time_ns)
        || current_time_ns <= prev_time_ns
    {
        return false;
    }
    let player = player_for_col(state, column);
    // ITG checks held mines as rows are crossed. Match that by only considering
    // mines whose note time crossed between previous and current music time.
    let (start_idx, end_idx) = crossed_mine_bounds_ns(
        &state.mine_note_time_ns[player],
        prev_time_ns,
        current_time_ns,
    );
    let mut hit_any = false;
    for i in start_idx..end_idx {
        let note_index = {
            let mine_ix = &state.mine_note_ix[player];
            mine_ix[i]
        };
        let (is_mine, can_be_judged, already_scored, is_fake, note_column) = {
            let note = &state.notes[note_index];
            (
                matches!(note.note_type, NoteType::Mine),
                note.can_be_judged,
                note.mine_result.is_some(),
                note.is_fake,
                note.column,
            )
        };
        if !is_mine || !can_be_judged || already_scored || is_fake || note_column != column {
            continue;
        }
        if hit_mine(state, column, note_index, 0) {
            hit_any = true;
        }
    }
    hit_any
}

#[inline(always)]
fn error_bar_register_tap(
    state: &mut State,
    player: usize,
    judgment: &Judgment,
    tap_music_time_s: f32,
) {
    let prof = &state.player_profiles[player];
    let mut error_bar_mask = prof.error_bar_active_mask;
    if error_bar_mask.is_empty() {
        error_bar_mask =
            profile_data::error_bar_mask_from_style(prof.error_bar, prof.error_bar_text);
    }
    let show_text = error_bar_mask.contains(profile_data::ErrorBarMask::TEXT);
    let show_monochrome = error_bar_mask.contains(profile_data::ErrorBarMask::MONOCHROME);
    let show_colorful = error_bar_mask.contains(profile_data::ErrorBarMask::COLORFUL);
    let show_highlight = error_bar_mask.contains(profile_data::ErrorBarMask::HIGHLIGHT);
    let show_average = error_bar_mask.contains(profile_data::ErrorBarMask::AVERAGE);
    let show_text_scalable = prof.text_error_bar_scalable;
    let text_error_bar_threshold_ms =
        profile_data::clamp_text_error_bar_threshold_ms(prof.text_error_bar_threshold_ms);
    let show_fa_plus_window = prof.show_fa_plus_window;
    let blue_fantastic_window_s = player_blue_window_ms(state, player) / 1000.0;
    let error_bar_trim = prof.error_bar_trim;
    let error_bar_multi_tick = prof.error_bar_multi_tick;
    let error_ms_display = prof.error_ms_display;
    let short_avg_enabled = prof.short_average_error_bar_enabled;
    let long_avg_enabled = prof.long_error_bar_enabled;
    let long_avg_threshold_s =
        profile_data::clamp_long_error_bar_threshold_ms(prof.long_error_bar_threshold_ms) as f32
            / 1000.0;
    let long_avg_min_samples =
        profile_data::clamp_long_error_bar_min_samples(prof.long_error_bar_min_samples) as usize;
    let average_interval_ms =
        profile_data::clamp_average_error_bar_interval_ms(prof.average_error_bar_interval_ms);
    let Some(window) = judgment.window else {
        return;
    };

    let now = state.total_elapsed_in_screen;
    let offset_s = judgment.time_error_ms / 1000.0;
    let p = &mut state.players[player];

    if error_ms_display {
        p.offset_indicator_text = Some(OffsetIndicatorText {
            started_at: now,
            offset_ms: judgment.time_error_ms,
            window,
        });
    }

    if show_text {
        let threshold_s = if show_text_scalable {
            text_error_bar_threshold_ms as f32 / 1000.0
        } else if show_fa_plus_window {
            blue_fantastic_window_s
        } else {
            state.timing_profile.windows_s[0]
        };
        if offset_s.abs() > threshold_s {
            p.error_bar_text = Some(ErrorBarText {
                started_at: now,
                early: offset_s < 0.0,
                offset_ms: judgment.time_error_ms.abs(),
                scaled: show_text_scalable,
                scale_start_ms: text_error_bar_threshold_ms as f32,
            });
        } else {
            p.error_bar_text = None;
        }
    } else {
        p.error_bar_text = None;
    }

    if !(show_monochrome || show_colorful || show_highlight || show_average) {
        return;
    }

    let max_window_ix = match error_bar_trim {
        profile_data::ErrorBarTrim::Off => 4,
        profile_data::ErrorBarTrim::Fantastic => 0,
        profile_data::ErrorBarTrim::Excellent => 1,
        profile_data::ErrorBarTrim::Great => 2,
    };
    let max_offset_s = state.timing_profile.windows_s[max_window_ix];
    let clamped_offset_s = if max_offset_s.is_finite() && max_offset_s > 0.0 {
        offset_s.clamp(-max_offset_s, max_offset_s)
    } else {
        offset_s
    };

    let tick = ErrorBarTick {
        started_at: now,
        offset_s: clamped_offset_s,
        window,
    };

    if show_monochrome {
        error_bar_push_tick(
            &mut p.error_bar_mono_ticks,
            &mut p.error_bar_mono_next,
            error_bar_multi_tick,
            tick,
        );
    }

    if show_colorful || show_highlight {
        error_bar_push_tick(
            &mut p.error_bar_color_ticks,
            &mut p.error_bar_color_next,
            error_bar_multi_tick,
            tick,
        );
        p.error_bar_color_bar_started_at = Some(now);
    }

    if show_highlight {
        let is_top = if show_fa_plus_window {
            window == TimingWindow::W0
        } else {
            window == TimingWindow::W1
        };
        let flash_window = if offset_s.abs() > max_offset_s {
            match max_window_ix {
                0 => TimingWindow::W1,
                1 => TimingWindow::W2,
                2 => TimingWindow::W3,
                3 => TimingWindow::W4,
                _ => TimingWindow::W5,
            }
        } else {
            window
        };
        let wi = error_bar_window_ix(flash_window);
        if is_top {
            p.error_bar_color_flash_early[wi] = Some(now);
            p.error_bar_color_flash_late[wi] = Some(now);
        } else if offset_s < 0.0 {
            p.error_bar_color_flash_early[wi] = Some(now);
        } else {
            p.error_bar_color_flash_late[wi] = Some(now);
        }
    }

    if show_average {
        if short_avg_enabled {
            let avg = error_bar_average_offset_s(
                &mut p.error_bar_avg_samples,
                tap_music_time_s,
                offset_s,
                average_interval_ms,
            );
            let avg_clamped = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                avg.clamp(-max_offset_s, max_offset_s)
            } else {
                avg
            };
            error_bar_push_tick(
                &mut p.error_bar_avg_ticks,
                &mut p.error_bar_avg_next,
                error_bar_multi_tick,
                ErrorBarTick {
                    started_at: now,
                    offset_s: avg_clamped,
                    window,
                },
            );
            p.error_bar_avg_bar_started_at = Some(now);
        }

        if long_avg_enabled {
            let (long_mean, long_len) = error_bar_long_term_offset_s(
                &mut p.error_bar_long_avg_samples,
                &mut p.error_bar_long_avg_total,
                tap_music_time_s,
                offset_s,
                average_interval_ms,
            );
            if long_len >= long_avg_min_samples && long_mean.abs() >= long_avg_threshold_s {
                p.error_bar_long_avg_tick = Some(ErrorBarTick {
                    started_at: now,
                    offset_s: long_mean,
                    window,
                });
                p.error_bar_long_avg_visible = true;
            } else {
                p.error_bar_long_avg_visible = false;
            }
        } else {
            p.error_bar_long_avg_visible = false;
        }
    }
}

#[inline(always)]
fn set_last_judgment(state: &mut State, player: usize, judgment: Judgment) {
    state.players[player].last_judgment = Some(JudgmentRenderInfo {
        judgment,
        started_at_screen_s: state.total_elapsed_in_screen,
    });
}

#[inline(always)]
fn set_last_mine_judgment(state: &mut State, player: usize, column: usize, result: MineResult) {
    state.players[player].last_mine_judgment = Some(MineJudgmentRenderInfo {
        result,
        column,
        started_at_screen_s: state.total_elapsed_in_screen,
    });
}

#[inline(always)]
pub(super) fn render_provisional_early_rescore_feedback(
    state: &mut State,
    player: usize,
    column: usize,
    judgment: &Judgment,
    current_time: f32,
    hide_early_dw_judgments: bool,
    hide_early_dw_flash: bool,
    hide_early_dw_column_flash: bool,
) {
    if !hide_early_dw_judgments {
        set_last_judgment(state, player, *judgment);
        error_bar_register_tap(state, player, judgment, current_time);
    }

    if !hide_early_dw_flash {
        trigger_receptor_glow_pulse(state, column);
        spawn_tap_explosion_for_grade(state, column, judgment.grade, false);
    }

    if !hide_early_dw_column_flash {
        trigger_column_flash_for_judgment(state, player, column, judgment);
    }
}

pub fn judge_a_tap(state: &mut State, column: usize, current_time_ns: SongTimeNs) -> bool {
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let timing_hit_log = timing_hit_log_enabled();
    let input_log = gameplay_input_log_enabled();
    let player = player_for_col(state, column);
    let rescore_early_hits = state.player_profiles[player].rescore_early_hits;
    let hide_early_dw_judgments = state.player_profiles[player].hide_early_dw_judgments;
    let hide_early_dw_flash = state.player_profiles[player].hide_early_dw_flash;
    let hide_early_dw_column_flash = state.player_profiles[player].hide_early_dw_column_flash;
    let scoring_blocked = autoplay_blocks_scoring(state);
    let lane_notes = &state.lane_note_indices[column];
    let current_row_index = timing_row_nearest(
        &state.timing_players[player],
        state.timing_players[player].get_beat_for_time_ns(current_time_ns),
    );
    let (search_start_row, search_end_row) = step_search_row_bounds(
        &state.timing_players[player],
        current_time_ns,
        current_row_index,
    );
    let (search_start_idx, search_end_idx) =
        lane_note_window_bounds_rows(lane_notes, &state.notes, search_start_row, search_end_row);
    if let Some((note_index, _)) = closest_lane_note_ns(
        lane_notes,
        &state.notes,
        &state.note_time_cache_ns,
        &state.timing_players[player],
        current_time_ns,
        current_row_index,
        search_start_idx,
        search_end_idx,
    ) {
        let note_row_index = state.notes[note_index].row_index;
        let note_type = state.notes[note_index].note_type;
        let time_error_music_ns =
            current_time_ns.saturating_sub(state.note_time_cache_ns[note_index]);

        if matches!(note_type, NoteType::Mine) {
            if state.notes[note_index].is_fake {
                log_tap_judge_candidate(
                    input_log,
                    "fake_mine_ignored",
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    note_index,
                    &state.notes[note_index],
                    state.note_time_cache_ns[note_index],
                    rate,
                );
                return false;
            }
            let hit = hit_mine(state, column, note_index, time_error_music_ns);
            log_tap_judge_candidate(
                input_log,
                if hit {
                    "mine_hit"
                } else {
                    "mine_outside_window"
                },
                player,
                column,
                current_row_index,
                current_time_ns,
                note_index,
                &state.notes[note_index],
                state.note_time_cache_ns[note_index],
                rate,
            );
            return hit;
        }
        if !lane_edge_matches_note_type(true, note_type) {
            log_tap_judge_candidate(
                input_log,
                "note_type_mismatch",
                player,
                column,
                current_row_index,
                current_time_ns,
                note_index,
                &state.notes[note_index],
                state.note_time_cache_ns[note_index],
                rate,
            );
            return false;
        }

        let Some(hit) = note_hit_eval(
            state,
            player,
            state.note_time_cache_ns[note_index],
            current_time_ns,
        ) else {
            log_tap_judge_candidate(
                input_log,
                "outside_tap_window",
                player,
                column,
                current_row_index,
                current_time_ns,
                note_index,
                &state.notes[note_index],
                state.note_time_cache_ns[note_index],
                rate,
            );
            return false;
        };
        let (song_offset_s, global_offset_s, lead_in_s, stream_pos_s) = if timing_hit_log {
            (
                state.song_offset_seconds,
                effective_player_global_offset_seconds(state, player),
                state.audio_lead_in_seconds.max(0.0),
                state.audio_stream_position_seconds,
            )
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
        if state.notes[note_index].is_fake {
            log_tap_judge_candidate(
                input_log,
                "fake_hit",
                player,
                column,
                current_row_index,
                current_time_ns,
                note_index,
                &state.notes[note_index],
                state.note_time_cache_ns[note_index],
                rate,
            );
            let (judgment, judgment_event_time) =
                build_final_note_hit_judgment(state, player, hit, rate);
            set_final_note_result(state, note_index, judgment);
            log_timing_hit_detail(
                timing_hit_log,
                stream_pos_s,
                hit.grade,
                note_row_index,
                state.notes[note_index].column,
                state.notes[note_index].beat,
                song_offset_s,
                global_offset_s,
                hit.note_time_ns,
                judgment_event_time,
                current_music_time_s(state),
                rate,
                lead_in_s,
            );
            trigger_receptor_glow_pulse(state, column);
            return true;
        }
        let Some(row_entry) = row_entry_for_cached_row(
            &state.row_entries,
            &state.row_map_cache[player],
            note_row_index,
        ) else {
            debug_assert!(false, "missing row cache for row {note_row_index}");
            return false;
        };
        let row_rescore_track_count = count_rescore_tracks_on_row(row_entry);
        let row_note_count = usize::from(row_entry.unresolved_nonlift_count);

        if rescore_early_hits && row_rescore_track_count == 1 {
            let note_col = state.notes[note_index].column;
            let is_early = hit.measured_offset_music_ns < 0;
            let is_bad = matches!(hit.grade, JudgeGrade::Decent | JudgeGrade::WayOff);

            if is_early && is_bad {
                log_tap_judge_candidate(
                    input_log,
                    if state.notes[note_index].early_result.is_some() {
                        "provisional_early_duplicate"
                    } else {
                        "provisional_early_hit"
                    },
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    note_index,
                    &state.notes[note_index],
                    state.note_time_cache_ns[note_index],
                    rate,
                );
                if state.notes[note_index].early_result.is_none() {
                    let judgment = Judgment {
                        time_error_ms: judgment_time_error_ms_from_music_ns(
                            hit.measured_offset_music_ns,
                            rate,
                        ),
                        time_error_music_ns: hit.measured_offset_music_ns,
                        grade: hit.grade,
                        window: Some(hit.window),
                        miss_because_held: false,
                    };
                    register_provisional_early_result(state, note_index, judgment);
                    let life_delta = judge_life_delta(hit.grade);
                    let current_music_time = current_music_time_s(state);
                    {
                        let p = &mut state.players[player];
                        if !scoring_blocked {
                            apply_life_change(p, current_music_time, life_delta);
                        }
                    }
                    if !scoring_blocked {
                        capture_failed_ex_score_inputs(state, player);
                    }
                    render_provisional_early_rescore_feedback(
                        state,
                        player,
                        note_col,
                        &judgment,
                        song_time_ns_to_seconds(current_time_ns),
                        hide_early_dw_judgments,
                        hide_early_dw_flash,
                        hide_early_dw_column_flash,
                    );
                    // Zmod parity: provisional early W4/W5 (with Rescore Early Hits enabled)
                    // should immediately drive EarlyHit-style visuals, but the later finalized
                    // W4/W5 should not produce a second bad popup/tick.
                    log_timing_hit_detail(
                        timing_hit_log,
                        stream_pos_s,
                        hit.grade,
                        note_row_index,
                        note_col,
                        state.notes[note_index].beat,
                        song_offset_s,
                        global_offset_s,
                        hit.note_time_ns,
                        current_time_ns,
                        current_music_time_s(state),
                        rate,
                        lead_in_s,
                    );

                    if let Some(end_time_ns) = state.hold_end_time_cache_ns[note_index]
                        && matches!(
                            state.notes[note_index].note_type,
                            NoteType::Hold | NoteType::Roll
                        )
                    {
                        start_active_hold(
                            state,
                            note_col,
                            note_index,
                            hit.note_time_ns,
                            end_time_ns,
                            current_time_ns,
                        );
                    }
                }
                return true;
            }

            if state.notes[note_index].early_result.is_some()
                && !matches!(
                    hit.grade,
                    JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
                )
            {
                log_tap_judge_candidate(
                    input_log,
                    "provisional_bad_rehit_ignored",
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    note_index,
                    &state.notes[note_index],
                    state.note_time_cache_ns[note_index],
                    rate,
                );
                return true;
            }

            log_tap_judge_candidate(
                input_log,
                "hit",
                player,
                column,
                current_row_index,
                current_time_ns,
                note_index,
                &state.notes[note_index],
                state.note_time_cache_ns[note_index],
                rate,
            );
            let (judgment, judgment_event_time) =
                build_final_note_hit_judgment(state, player, hit, rate);
            let receptor_window = grade_to_window(judgment.grade);
            set_final_note_result(state, note_index, judgment);

            log_timing_hit_detail(
                timing_hit_log,
                stream_pos_s,
                hit.grade,
                note_row_index,
                note_col,
                state.notes[note_index].beat,
                song_offset_s,
                global_offset_s,
                hit.note_time_ns,
                judgment_event_time,
                current_music_time_s(state),
                rate,
                lead_in_s,
            );

            trigger_completed_row_tap_explosions(state, player, note_row_index);
            if let Some(window_key) = receptor_window {
                trigger_receptor_score_pulse(state, note_col, window_key);
            }
            if let Some(end_time_ns) = state.hold_end_time_cache_ns[note_index]
                && matches!(
                    state.notes[note_index].note_type,
                    NoteType::Hold | NoteType::Roll
                )
            {
                start_active_hold(
                    state,
                    note_col,
                    note_index,
                    hit.note_time_ns,
                    end_time_ns,
                    current_time_ns,
                );
            }
            return true;
        }

        let Some((judge_indices, judge_count)) =
            collect_edge_judge_indices(row_note_count, note_index)
        else {
            log_tap_judge_candidate(
                input_log,
                "no_row_judge_indices",
                player,
                column,
                current_row_index,
                current_time_ns,
                note_index,
                &state.notes[note_index],
                state.note_time_cache_ns[note_index],
                rate,
            );
            return false;
        };

        for &idx in &judge_indices[..judge_count] {
            let note_col = state.notes[idx].column;
            let Some(hit) = note_hit_eval(
                state,
                player,
                state.note_time_cache_ns[idx],
                current_time_ns,
            ) else {
                log_tap_judge_candidate(
                    input_log,
                    "row_sibling_outside_tap_window",
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    idx,
                    &state.notes[idx],
                    state.note_time_cache_ns[idx],
                    rate,
                );
                continue;
            };
            log_tap_judge_candidate(
                input_log,
                "hit",
                player,
                column,
                current_row_index,
                current_time_ns,
                idx,
                &state.notes[idx],
                state.note_time_cache_ns[idx],
                rate,
            );
            let (judgment, judgment_event_time) =
                build_final_note_hit_judgment(state, player, hit, rate);
            let receptor_window = grade_to_window(judgment.grade);
            set_final_note_result(state, idx, judgment);

            log_timing_hit_detail(
                timing_hit_log,
                stream_pos_s,
                hit.grade,
                note_row_index,
                note_col,
                state.notes[idx].beat,
                song_offset_s,
                global_offset_s,
                hit.note_time_ns,
                judgment_event_time,
                current_music_time_s(state),
                rate,
                lead_in_s,
            );

            trigger_completed_row_tap_explosions(state, player, note_row_index);
            if let Some(window_key) = receptor_window {
                trigger_receptor_score_pulse(state, note_col, window_key);
            }
            if let Some(end_time_ns) = state.hold_end_time_cache_ns[idx]
                && matches!(state.notes[idx].note_type, NoteType::Hold | NoteType::Roll)
            {
                start_active_hold(
                    state,
                    note_col,
                    idx,
                    hit.note_time_ns,
                    end_time_ns,
                    current_time_ns,
                );
            }
        }
        return true;
    }
    if input_log {
        debug!(
            concat!(
                "GAMEPLAY TAP JUDGE: reason=no_candidate, player={}, lane={}, ",
                "current_row={}, search_rows={}..{}, search_indices={}..{}, ",
                "lane_notes={}, event_time_s={:.6}, current_time_s={:.6}"
            ),
            player,
            column,
            current_row_index,
            search_start_row,
            search_end_row,
            search_start_idx,
            search_end_idx,
            lane_notes.len(),
            song_time_ns_to_seconds(current_time_ns),
            current_music_time_s(state),
        );
    }
    false
}

/// Judge lift notes on button release. Mirrors tap judging's per-note path but
/// only matches NoteType::Lift.
pub fn judge_a_lift(state: &mut State, column: usize, current_time_ns: SongTimeNs) -> bool {
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let timing_hit_log = timing_hit_log_enabled();
    let player = player_for_col(state, column);
    let rescore_early_hits = state.player_profiles[player].rescore_early_hits;
    let hide_early_dw_judgments = state.player_profiles[player].hide_early_dw_judgments;
    let hide_early_dw_flash = state.player_profiles[player].hide_early_dw_flash;
    let hide_early_dw_column_flash = state.player_profiles[player].hide_early_dw_column_flash;
    let scoring_blocked = autoplay_blocks_scoring(state);
    let lane_notes = &state.lane_note_indices[column];
    let current_row_index = timing_row_nearest(
        &state.timing_players[player],
        state.timing_players[player].get_beat_for_time_ns(current_time_ns),
    );
    let (search_start_row, search_end_row) = step_search_row_bounds(
        &state.timing_players[player],
        current_time_ns,
        current_row_index,
    );
    let (search_start_idx, search_end_idx) =
        lane_note_window_bounds_rows(lane_notes, &state.notes, search_start_row, search_end_row);
    let Some((note_index, _)) = closest_lane_note_ns(
        lane_notes,
        &state.notes,
        &state.note_time_cache_ns,
        &state.timing_players[player],
        current_time_ns,
        current_row_index,
        search_start_idx,
        search_end_idx,
    ) else {
        return false;
    };
    if !lane_edge_matches_note_type(false, state.notes[note_index].note_type) {
        return false;
    }

    let Some(hit) = note_hit_eval(
        state,
        player,
        state.note_time_cache_ns[note_index],
        current_time_ns,
    ) else {
        return false;
    };
    let (song_offset_s, global_offset_s, lead_in_s, stream_pos_s) = if timing_hit_log {
        (
            state.song_offset_seconds,
            effective_player_global_offset_seconds(state, player),
            state.audio_lead_in_seconds.max(0.0),
            state.audio_stream_position_seconds,
        )
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    let note_col = state.notes[note_index].column;
    let note_row_index = state.notes[note_index].row_index;
    let note_beat = state.notes[note_index].beat;

    if rescore_early_hits {
        let Some(row_entry) = row_entry_for_cached_row(
            &state.row_entries,
            &state.row_map_cache[player],
            note_row_index,
        ) else {
            debug_assert!(false, "missing row cache for row {note_row_index}");
            return false;
        };
        let row_rescore_track_count = count_rescore_tracks_on_row(row_entry);
        let is_early = hit.measured_offset_music_ns < 0;
        let is_bad = matches!(hit.grade, JudgeGrade::Decent | JudgeGrade::WayOff);

        if row_rescore_track_count == 1 && is_early && is_bad {
            if state.notes[note_index].early_result.is_none() {
                let judgment = Judgment {
                    time_error_ms: judgment_time_error_ms_from_music_ns(
                        hit.measured_offset_music_ns,
                        rate,
                    ),
                    time_error_music_ns: hit.measured_offset_music_ns,
                    grade: hit.grade,
                    window: Some(hit.window),
                    miss_because_held: false,
                };
                register_provisional_early_result(state, note_index, judgment);
                let life_delta = judge_life_delta(hit.grade);
                let current_music_time = current_music_time_s(state);
                if !scoring_blocked {
                    let p = &mut state.players[player];
                    apply_life_change(p, current_music_time, life_delta);
                    capture_failed_ex_score_inputs(state, player);
                }
                render_provisional_early_rescore_feedback(
                    state,
                    player,
                    note_col,
                    &judgment,
                    song_time_ns_to_seconds(current_time_ns),
                    hide_early_dw_judgments,
                    hide_early_dw_flash,
                    hide_early_dw_column_flash,
                );

                log_timing_hit_detail(
                    timing_hit_log,
                    stream_pos_s,
                    hit.grade,
                    note_row_index,
                    note_col,
                    note_beat,
                    song_offset_s,
                    global_offset_s,
                    hit.note_time_ns,
                    current_time_ns,
                    current_music_time_s(state),
                    rate,
                    lead_in_s,
                );
            }
            return true;
        }

        if row_rescore_track_count == 1
            && state.notes[note_index].early_result.is_some()
            && !matches!(
                hit.grade,
                JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
            )
        {
            return true;
        }
    }

    let (judgment, judgment_event_time) = build_final_note_hit_judgment(state, player, hit, rate);
    let receptor_window = grade_to_window(judgment.grade);
    set_final_note_result(state, note_index, judgment);

    log_timing_hit_detail(
        timing_hit_log,
        stream_pos_s,
        hit.grade,
        note_row_index,
        note_col,
        note_beat,
        song_offset_s,
        global_offset_s,
        hit.note_time_ns,
        judgment_event_time,
        current_music_time_s(state),
        rate,
        lead_in_s,
    );

    trigger_completed_row_tap_explosions(state, player, note_row_index);
    if let Some(window_key) = receptor_window {
        trigger_receptor_score_pulse(state, note_col, window_key);
    }
    true
}

#[inline(always)]
fn run_assist_clap(
    state: &mut State,
    current_row: i32,
    music_time_ns: SongTimeNs,
    slope: f32,
    assist_sfx_generation: u64,
) {
    let song_row = current_row.max(0);

    // Detect an audio timeline reset (stop / seek / track change). On reset the
    // mixer drops every scheduled tick from the old timeline, so the scheduling
    // cursor must re-anchor to the new audible position.
    let timeline_reset = assist_sfx_generation != state.assist_sfx_gen_seen;
    if timeline_reset {
        state.assist_sfx_gen_seen = assist_sfx_generation;
    }

    if state.tick_mode != TickMode::Assist {
        // Keep the cursor abreast of the audible position so enabling assist
        // later doesn't replay already-passed rows.
        state.assist_clap_cursor = assist_clap_cursor_for_row(&state.assist_clap_rows, song_row);
        state.assist_last_crossed_row = song_row;
        return;
    }

    if timeline_reset {
        state.assist_clap_cursor = assist_clap_cursor_for_row(&state.assist_clap_rows, song_row);
        state.assist_last_crossed_row = song_row;
    } else if song_row > state.assist_last_crossed_row {
        state.assist_last_crossed_row = song_row;
    }
    // Minor backward audible jitter (song_row < last_crossed without a timeline
    // reset) deliberately does NOT rewind the cursor: those rows are already
    // queued, so re-scheduling them would double-fire.

    let future_row = assist_lookahead_future_row(state, music_time_ns, slope, song_row);
    let rows_len = state.assist_clap_rows.len();
    while state.assist_clap_cursor < rows_len {
        let clap_row = state.assist_clap_rows[state.assist_clap_cursor];
        if clap_row as i64 > i64::from(future_row) {
            break;
        }
        schedule_assist_clap_row(state, clap_row);
        state.assist_clap_cursor += 1;
    }
}

/// Highest assist row whose no-offset music time falls within the look-ahead
/// horizon ahead of the audible position.
#[inline(always)]
fn assist_lookahead_future_row(
    state: &State,
    music_time_ns: SongTimeNs,
    slope: f32,
    song_row: i32,
) -> i32 {
    let music_horizon =
        assist_lookahead_music_horizon_seconds(state.audio_output_delay_seconds, slope);
    let future_time = song_time_ns_add_seconds(music_time_ns, music_horizon);
    assist_row_no_offset_ns(state, future_time).max(song_row)
}

/// Schedules a single assist clap row by its absolute stream frame so the mixer
/// can place the onset sample-accurately. Falls back to immediate playback when
/// the row has no usable stream-frame mapping (e.g. during lead-in).
#[inline(always)]
fn schedule_assist_clap_row(state: &mut State, clap_row: usize) {
    let Some(beat) = state.timing.get_beat_for_row(clap_row) else {
        queue_preloaded_assist_tick(state, ASSIST_TICK_SFX_PATH);
        return;
    };
    let row_time_ns = state.timing.get_time_for_beat_no_offset_ns(beat);
    let music_seconds = row_time_ns as f64 * 1e-9;
    queue_assist_tick_at_music_time(state, ASSIST_TICK_SFX_PATH, music_seconds);
}

#[inline(always)]
fn decay_let_go_hold_life(state: &mut State) {
    let mut i = 0;
    while i < state.decaying_hold_indices.len() {
        let note_index = state.decaying_hold_indices[i];
        let Some(note) = state.notes.get_mut(note_index) else {
            state.decaying_hold_indices.swap_remove(i);
            continue;
        };
        let Some(hold) = note.hold.as_mut() else {
            state.hold_decay_active[note_index] = false;
            state.decaying_hold_indices.swap_remove(i);
            continue;
        };
        if hold.result == Some(HoldResult::Held) || hold.let_go_started_at.is_none() {
            state.hold_decay_active[note_index] = false;
            state.decaying_hold_indices.swap_remove(i);
            continue;
        }
        let window = match note.note_type {
            NoteType::Roll => TIMING_WINDOW_SECONDS_ROLL,
            _ => TIMING_WINDOW_SECONDS_HOLD,
        };
        if window <= 0.0 {
            hold.life = 0.0;
            state.hold_decay_active[note_index] = false;
            state.decaying_hold_indices.swap_remove(i);
            continue;
        }
        let start_time = hold.let_go_started_at.unwrap();
        let base_life = hold.let_go_starting_life.clamp(0.0, MAX_HOLD_LIFE);
        if base_life <= 0.0 {
            hold.life = 0.0;
            state.hold_decay_active[note_index] = false;
            state.decaying_hold_indices.swap_remove(i);
            continue;
        }
        let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
            state.music_rate
        } else {
            1.0
        };
        let elapsed_music =
            song_time_ns_delta_seconds(state.current_music_time_ns, start_time).max(0.0);
        let elapsed_real = elapsed_music / rate;
        hold.life = (base_life - elapsed_real / window).max(0.0);
        if hold.life <= f32::EPSILON {
            state.hold_decay_active[note_index] = false;
            state.decaying_hold_indices.swap_remove(i);
            continue;
        }
        i += 1;
    }
}

#[inline(always)]
fn queue_missed_hold_resolution(state: &mut State, note_index: usize) {
    if note_index >= state.pending_missed_hold_resolution.len()
        || state.pending_missed_hold_resolution[note_index]
    {
        return;
    }
    state.pending_missed_hold_resolution[note_index] = true;
    state.pending_missed_hold_indices.push(note_index);
}

#[inline(always)]
fn resolve_pending_missed_holds(state: &mut State, current_time_ns: SongTimeNs) {
    let mut i = 0usize;
    while i < state.pending_missed_hold_indices.len() {
        let note_index = state.pending_missed_hold_indices[i];
        let Some(end_time_ns) = state
            .hold_end_time_cache_ns
            .get(note_index)
            .and_then(|t| *t)
        else {
            state.pending_missed_hold_resolution[note_index] = false;
            state.pending_missed_hold_indices.swap_remove(i);
            continue;
        };
        if current_time_ns < end_time_ns {
            i += 1;
            continue;
        }
        state.pending_missed_hold_resolution[note_index] = false;
        let Some(note) = state.notes.get(note_index) else {
            state.pending_missed_hold_indices.swap_remove(i);
            continue;
        };
        let column = note.column;
        if column >= state.num_cols {
            state.pending_missed_hold_indices.swap_remove(i);
            continue;
        }
        let hold_result = note.hold.as_ref().and_then(|hold| hold.result);
        if hold_result == Some(HoldResult::Missed) {
            state.hold_judgments[column] = Some(HoldJudgmentRenderInfo {
                result: HoldResult::Missed,
                started_at_screen_s: state.total_elapsed_in_screen,
            });
        } else if hold_result.is_none()
            && note
                .result
                .as_ref()
                .is_some_and(|judgment| judgment.grade == JudgeGrade::Miss)
            && state.score_missed_holds_rolls[player_for_col(state, column)]
        {
            handle_hold_let_go(state, column, note_index, end_time_ns);
        }
        state.pending_missed_hold_indices.swap_remove(i);
    }
}

#[inline(always)]
fn track_held_miss_windows(
    state: &mut State,
    inputs: &[bool; MAX_COLS],
    music_time_ns: SongTimeNs,
) {
    for player in 0..state.num_players {
        let largest_window_ns = player_largest_tap_window_ns(state, player);
        if largest_window_ns <= 0 {
            continue;
        }
        let (col_start, col_end) = player_col_range(state, player);
        let (note_start, note_end) = player_note_range(state, player);
        track_held_miss_window_for_player(
            &state.notes,
            &state.note_time_cache_ns,
            &mut state.tap_miss_held_window,
            (note_start, note_end),
            (col_start, col_end),
            state.next_tap_miss_cursor[player],
            inputs,
            music_time_ns,
            largest_window_ns,
        );
    }
}

#[inline(always)]
fn mine_avoid_log_enabled() -> bool {
    log::log_enabled!(log::Level::Trace)
}

#[inline(always)]
fn missed_note_cutoff_row(state: &State, player: usize, music_time_ns: SongTimeNs) -> usize {
    let cutoff_time_ns = music_time_ns.saturating_sub(max_step_distance_ns(
        &state.timing_profile,
        state.music_rate,
    ));
    missed_note_cutoff_row_for_timing(&state.timing_players[player], cutoff_time_ns)
}

#[inline(always)]
fn apply_time_based_mine_avoidance(state: &mut State, music_time_ns: SongTimeNs) {
    let music_time_sec = song_time_ns_to_seconds(music_time_ns);
    let log_mine_avoid = mine_avoid_log_enabled();
    for player in 0..state.num_players {
        let mines_len = state.mine_note_ix[player].len();
        let mine_cursor = state.next_mine_ix_cursor[player].min(mines_len);
        let cutoff_row = missed_note_cutoff_row(state, player, music_time_ns);
        let mut mine_end = mine_cursor;
        while mine_end < mines_len {
            let note_idx = state.mine_note_ix[player][mine_end];
            if state.notes[note_idx].row_index >= cutoff_row {
                break;
            }
            mine_end += 1;
        }
        let mut avoided_count = 0u32;
        for cursor in mine_cursor..mine_end {
            let note_idx = state.mine_note_ix[player][cursor];
            let note = &mut state.notes[note_idx];
            if note.can_be_judged && note.mine_result.is_none() {
                let row_index = note.row_index;
                let column = note.column;
                note.mine_result = Some(MineResult::Avoided);
                set_last_mine_judgment(state, player, column, MineResult::Avoided);
                avoided_count = avoided_count.saturating_add(1);
                if log_mine_avoid {
                    trace!(
                        "MINE AVOIDED: Row {row_index}, Col {column}, Time: {music_time_sec:.2}s"
                    );
                }
            }
        }
        if avoided_count > 0 {
            state.players[player].mines_avoided = state.players[player]
                .mines_avoided
                .saturating_add(avoided_count);
        }
        state.next_mine_ix_cursor[player] = mine_end;
        let (_, note_end) = player_note_range(state, player);
        state.next_mine_avoid_cursor[player] = if mine_end < mines_len {
            state.mine_note_ix[player][mine_end]
        } else {
            note_end
        };
    }
}

fn finalize_completed_mines(state: &mut State) {
    for player in 0..state.num_players {
        let (note_start, note_end) = player_note_range(state, player);
        for note in &mut state.notes[note_start..note_end] {
            if matches!(note.note_type, NoteType::Mine)
                && note.can_be_judged
                && !note.is_fake
                && note.mine_result.is_none()
            {
                note.mine_result = Some(MineResult::Avoided);
            }
        }

        let total = state.mines_total[player];
        let hit = state.players[player].mines_hit.min(total);
        state.players[player].mines_avoided = total.saturating_sub(hit);
    }
}

#[inline(always)]
fn apply_time_based_tap_misses(state: &mut State, music_time_ns: SongTimeNs) {
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let music_time_sec = song_time_ns_to_seconds(music_time_ns);
    for player in 0..state.num_players {
        let (note_start, note_end) = player_note_range(state, player);
        let should_score_miss = state.score_missed_holds_rolls[player];
        let cutoff_row = missed_note_cutoff_row(state, player, music_time_ns);
        let mut cursor = state.next_tap_miss_cursor[player].max(note_start);
        while cursor < note_end {
            let note_time_ns = state.note_time_cache_ns[cursor];
            let (row, col, beat) = {
                let note = &state.notes[cursor];
                if note.row_index >= cutoff_row {
                    break;
                }
                if matches!(note.note_type, NoteType::Mine)
                    || !note.can_be_judged
                    || note.result.is_some()
                {
                    cursor += 1;
                    continue;
                }
                (note.row_index, note.column, note.beat)
            };
            {
                let miss_offset_music_ns = music_time_ns.saturating_sub(note_time_ns);
                let miss_because_held = state
                    .tap_miss_held_window
                    .get(cursor)
                    .copied()
                    .unwrap_or(false);
                let miss = Judgment {
                    time_error_ms: judgment_time_error_ms_from_music_ns(miss_offset_music_ns, rate),
                    time_error_music_ns: miss_offset_music_ns,
                    grade: JudgeGrade::Miss,
                    window: None,
                    miss_because_held,
                };
                let judgment = state.notes[cursor].early_result.unwrap_or(miss);
                let judgment_grade = judgment.grade;
                let judgment_time_error_ms = judgment.time_error_ms;
                let mut queue_missed_hold = false;
                if judgment_grade == JudgeGrade::Miss
                    && let Some(hold) = state.notes[cursor].hold.as_mut()
                    && hold.result != Some(HoldResult::Held)
                {
                    if should_score_miss {
                        queue_missed_hold = true;
                    } else {
                        hold.result = Some(HoldResult::Missed);
                        queue_missed_hold = true;
                    }
                    begin_hold_life_decay(
                        hold,
                        &mut state.hold_decay_active,
                        &mut state.decaying_hold_indices,
                        cursor,
                        music_time_ns,
                    );
                }
                if queue_missed_hold {
                    queue_missed_hold_resolution(state, cursor);
                }
                set_final_note_result(state, cursor, judgment);
                if log::log_enabled!(log::Level::Debug) {
                    let note_time = song_time_ns_to_seconds(note_time_ns);
                    let song_offset_s = state.song_offset_seconds;
                    let global_offset_s = effective_player_global_offset_seconds(state, player);
                    let lead_in_s = state.audio_lead_in_seconds.max(0.0);
                    let stream_pos_s = state.audio_stream_position_seconds;
                    let expected_stream_for_note_s =
                        note_time / rate + lead_in_s + global_offset_s * (1.0 - rate) / rate;
                    let expected_stream_for_miss_s =
                        music_time_sec / rate + lead_in_s + global_offset_s * (1.0 - rate) / rate;
                    let stream_delta_note_ms = (stream_pos_s - expected_stream_for_note_s) * 1000.0;
                    let stream_delta_miss_ms = (stream_pos_s - expected_stream_for_miss_s) * 1000.0;

                    debug!(
                        concat!(
                            "TIMING MISS: row={}, col={}, beat={:.3}, ",
                            "song_offset_s={:.4}, global_offset_s={:.4}, ",
                            "note_time_s={:.6}, miss_time_s={:.6}, ",
                            "offset_ms={:.2}, miss_because_held={}, rate={:.3}, lead_in_s={:.4}, ",
                            "stream_pos_s={:.6}, stream_note_s={:.6}, stream_delta_note_ms={:.2}, ",
                            "stream_miss_s={:.6}, stream_delta_miss_ms={:.2}"
                        ),
                        row,
                        col,
                        beat,
                        song_offset_s,
                        global_offset_s,
                        note_time,
                        music_time_sec,
                        judgment_time_error_ms,
                        miss_because_held,
                        rate,
                        lead_in_s,
                        stream_pos_s,
                        expected_stream_for_note_s,
                        stream_delta_note_ms,
                        expected_stream_for_miss_s,
                        stream_delta_miss_ms,
                    );
                }
                debug!("MISSED (time-based): Row {row}");
            }
            cursor += 1;
        }
        state.next_tap_miss_cursor[player] = cursor;
    }
}

#[inline(always)]
fn score_rows_finalized(state: &State) -> bool {
    (0..state.num_players).all(|player| {
        let (start, end) = state.row_entry_ranges[player];
        state.row_entries[start..end]
            .iter()
            .all(|row| row.final_outcome.is_some())
    })
}

#[inline(always)]
fn settle_completion_rows(state: &mut State) -> bool {
    update_judged_rows(state);
    score_rows_finalized(state)
}

pub fn update(
    state: &mut State,
    delta_time: f32,
    audio_snapshot: GameplayAudioSnapshot,
) -> GameplayAction {
    if let Some(exit) = state.exit_transition {
        state.total_elapsed_in_screen += delta_time;
        if exit.started_at.elapsed().as_secs_f32() >= exit_total_seconds(exit.kind) {
            state.exit_transition = None;
            return GameplayAction::NavigateNoFade(gameplay_exit_for_kind(exit.kind));
        }
        return GameplayAction::None;
    }

    let trace_enabled = log::log_enabled!(log::Level::Trace);
    let frame_trace_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    let mut phase_timings = GameplayUpdatePhaseTimings::default();
    state.audio_stream_position_seconds = audio_snapshot.stream_clock.stream_seconds;
    state.audio_output_delay_seconds = audio_snapshot.output_delay_seconds.max(0.0);

    if let Some(at) = state.hold_to_exit_aborted_at
        && at.elapsed().as_secs_f32() >= GIVE_UP_ABORT_TEXT_SECONDS
    {
        state.hold_to_exit_aborted_at = None;
    }

    // Music time driven directly by the audio device clock, interpolated
    // between callbacks for smooth, continuous motion.
    let song_clock = current_song_clock_snapshot(state, audio_snapshot);
    let lead_in = state.audio_lead_in_seconds.max(0.0);
    let previous_music_time_ns = state.current_music_time_ns;
    let mut music_time_ns = song_clock.song_time_ns;
    let is_first_update = state.total_elapsed_in_screen <= f32::EPSILON;
    if is_first_update {
        const STARTUP_MAX_FORWARD_JUMP_NS: SongTimeNs = 1_000_000_000;
        let jump_ns = music_time_ns.saturating_sub(previous_music_time_ns);
        if jump_ns > STARTUP_MAX_FORWARD_JUMP_NS {
            let previous_music_time = song_time_ns_to_seconds(previous_music_time_ns);
            let music_time_sec = song_time_ns_to_seconds(music_time_ns);
            let jump_s = song_time_ns_delta_seconds(music_time_ns, previous_music_time_ns);
            warn!(
                "Discarding anomalous first-frame music time jump ({jump_s:.3}s): prev={previous_music_time:.3}, now={music_time_sec:.3}, lead_in={lead_in:.3}"
            );
            music_time_ns = previous_music_time_ns;
        }
    }
    let music_time_sec = song_time_ns_to_seconds(music_time_ns);
    state.current_music_time_ns = music_time_ns;
    let display_diag_host_nanos = if song_clock.valid_at_host_nanos != 0 {
        song_clock.valid_at_host_nanos
    } else {
        deadsync_platform::host_time::instant_nanos(Instant::now())
    };
    let display_music_time_ns = frame_stable_display_music_time_ns(
        &mut state.display_clock,
        &mut state.display_clock_diag,
        display_diag_host_nanos,
        music_time_ns,
        delta_time,
        song_clock.seconds_per_second,
        is_first_update,
    );
    state.current_music_time_display = song_time_ns_to_seconds(display_music_time_ns);

    if let (Some(key), Some(start_time)) = (state.hold_to_exit_key, state.hold_to_exit_start) {
        let hold_s = hold_to_exit_seconds(key);
        if start_time.elapsed().as_secs_f32() >= hold_s {
            if key == HoldToExitKey::Start && music_time_ns >= state.notes_end_time_ns {
                state.song_completed_naturally = true;
                finalize_completed_mines(state);
            }
            match key {
                HoldToExitKey::Start => {
                    begin_exit_transition(state, ExitTransitionKind::Out);
                }
                HoldToExitKey::Back => {
                    begin_exit_transition(state, ExitTransitionKind::Cancel);
                }
            }
            finalize_update_trace(
                state,
                delta_time,
                music_time_sec,
                frame_trace_started,
                phase_timings,
            );
            return GameplayAction::None;
        }
    }
    state.total_elapsed_in_screen += delta_time;

    let pre_notes_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    {
        let beat_info = state
            .timing
            .get_beat_info_from_time_ns_cached(music_time_ns, &mut state.beat_info_cache);
        state.current_beat = beat_info.beat;
        state.current_beat_display = state.timing.get_beat_for_time_ns(display_music_time_ns);
        state.is_in_freeze = beat_info.is_in_freeze;
        state.is_in_delay = beat_info.is_in_delay;
        let song_row = assist_row_no_offset_ns(state, music_time_ns);
        run_assist_clap(
            state,
            song_row,
            music_time_ns,
            song_clock.seconds_per_second,
            audio_snapshot.assist_sfx_generation,
        );

        for player in 0..state.num_players {
            let delay =
                state.global_visual_delay_seconds + state.player_visual_delay_seconds[player];
            let visible_time_ns = visible_notefield_time_ns(music_time_ns, delay);
            state.current_music_time_visible_ns[player] = visible_time_ns;
            state.current_music_time_visible[player] = song_time_ns_to_seconds(visible_time_ns);
            state.current_beat_visible[player] =
                state.timing_players[player].get_beat_for_time_ns(visible_time_ns);
        }
        refresh_active_attack_masks(state, delta_time);

        let current_bpm = state.timing.get_bpm_for_beat(state.current_beat);
        refresh_live_notefield_options(state, current_bpm);
    }
    if let Some(started) = pre_notes_started {
        phase_timings.pre_notes_us = elapsed_us_since(started);
    }

    let autoplay_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    if state.replay_mode {
        run_replay(state);
    } else {
        run_autoplay(state, music_time_ns);
    }
    if let Some(started) = autoplay_started {
        phase_timings.autoplay_us = elapsed_us_since(started);
    }

    let input_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    update_offset_adjust_hold(state);
    process_input_edges(state, trace_enabled, &mut phase_timings, song_clock);
    if let Some(started) = input_started {
        phase_timings.input_edges_us = elapsed_us_since(started);
    }

    let held_mines_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    let num_cols = state.num_cols;
    let current_inputs: [bool; MAX_COLS] = std::array::from_fn(|i| {
        if i >= num_cols {
            return false;
        }
        lane_is_pressed(state, i)
    });
    let prev_inputs = state.prev_inputs;
    if !live_autoplay_enabled(state) {
        for (col, (now_down, was_down)) in
            current_inputs.iter().copied().zip(prev_inputs).enumerate()
        {
            if let Some(crossed_from_ns) = crossed_mine_held_start_time(
                now_down,
                was_down,
                state.lane_pressed_since_ns[col],
                previous_music_time_ns,
                music_time_ns,
            ) {
                let _ =
                    try_hit_crossed_mines_while_held(state, col, crossed_from_ns, music_time_ns);
            }
        }
    }
    track_held_miss_windows(state, &current_inputs, music_time_ns);
    state.prev_inputs = current_inputs;
    if let Some(started) = held_mines_started {
        phase_timings.held_mines_us = elapsed_us_since(started);
    }

    let active_holds_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    update_active_holds(state, &current_inputs, music_time_ns);
    if let Some(started) = active_holds_started {
        phase_timings.active_holds_us = elapsed_us_since(started);
    }
    apply_pending_mine_hits(state);

    let hold_decay_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    decay_let_go_hold_life(state);
    resolve_pending_missed_holds(state, music_time_ns);
    if let Some(started) = hold_decay_started {
        phase_timings.hold_decay_us = elapsed_us_since(started);
    }

    let visuals_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    tick_visual_effects(state, delta_time);
    if let Some(started) = visuals_started {
        phase_timings.visuals_us = elapsed_us_since(started);
    }

    let judged_rows_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    // ITGmania resolves already-complete rows before it promotes overdue
    // mines/taps to avoids/misses.
    update_judged_rows(state);
    if let Some(started) = judged_rows_started {
        phase_timings.judged_rows_us = elapsed_us_since(started);
    }

    let mine_avoid_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    apply_time_based_mine_avoidance(state, music_time_ns);
    if let Some(started) = mine_avoid_started {
        phase_timings.mine_avoid_us = elapsed_us_since(started);
    }

    let tap_miss_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    apply_time_based_tap_misses(state, music_time_ns);
    if let Some(started) = tap_miss_started {
        phase_timings.tap_miss_us = elapsed_us_since(started);
    }

    let density_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    update_density_graph(state, music_time_sec, trace_enabled, &mut phase_timings);
    if let Some(started) = density_started {
        phase_timings.density_us = elapsed_us_since(started);
    }

    let danger_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    update_danger_fx(state);
    if let Some(started) = danger_started {
        phase_timings.danger_us = elapsed_us_since(started);
    }

    // Match ITG's end-of-song ordering: resolve the frame's late taps, hold
    // ends, and misses before leaving gameplay, otherwise the last frame can
    // cut to evaluation before final judgments land.
    if state.current_music_time_ns >= state.music_end_time_ns {
        if !settle_completion_rows(state) && trace_enabled {
            trace!("Music end time reached with pending score rows; completing gameplay.");
        }
        debug!("Music end time reached. Transitioning to evaluation.");
        state.song_completed_naturally = true;
        begin_outro_attack_clear(state);
        finalize_completed_mines(state);
        finalize_update_trace(
            state,
            delta_time,
            music_time_sec,
            frame_trace_started,
            phase_timings,
        );
        return GameplayAction::Navigate(GameplayExit::Complete);
    }

    if matches!(state.default_fail_type, GameplayFailType::Immediate)
        && all_joined_players_failed(state)
    {
        debug!("All joined players failed. Transitioning to evaluation.");
        state.song_completed_naturally = false;
        queue_stop_music(state);
        finalize_update_trace(
            state,
            delta_time,
            music_time_sec,
            frame_trace_started,
            phase_timings,
        );
        return GameplayAction::Navigate(GameplayExit::Complete);
    }

    finalize_update_trace(
        state,
        delta_time,
        music_time_sec,
        frame_trace_started,
        phase_timings,
    );
    GameplayAction::None
}

fn update_danger_fx(state: &mut State) {
    let now = state.total_elapsed_in_screen;
    for player in 0..state.num_players {
        if state.player_profiles[player].hide_lifebar {
            state.danger_fx[player] = DangerFx::default();
            continue;
        }

        let health =
            danger_health_state(state.players[player].life, state.players[player].is_failing);
        let hide_danger = state.player_profiles[player].hide_danger;
        update_danger_fx_for_health(&mut state.danger_fx[player], health, now, hide_danger);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO, DisplayClockDiagRing, ExitTransitionKind,
        FinalizedRowOutcome, FrameStableDisplayClock, GAMEPLAY_INPUT_BACKLOG_WARN,
        GameplayAudioCommand, GameplaySessionCommand, GameplayTurnOption, HELD_MISS_TOTAL_DURATION,
        HeldMissRenderInfo, HoldJudgmentRenderInfo, HoldToExitKey, INSERT_MASK_BIT_MINES, MAX_COLS,
        MAX_PLAYERS, OFFSET_ADJUST_STEP_SECONDS, REPLAY_EDGE_RATE_PER_SEC, RowEntry, ScrollEffects,
        ScrollSpeedSetting, SongClockSnapshot, TIMING_WINDOW_SECONDS_HOLD, TickMode,
        active_hold_counts_as_pressed, advance_hold_last_held, advance_hold_life_ns,
        advance_judged_row_cursor, apply_autosync_for_row_hits, apply_global_offset_delta,
        apply_mines_insert, apply_pending_mine_hits, apply_song_offset_delta,
        apply_time_based_mine_avoidance, apply_time_based_tap_misses,
        assist_lookahead_music_horizon_seconds, begin_outro_attack_clear, build_assist_clap_rows,
        build_attack_mask_windows_for_player, build_column_cues_for_player,
        build_player_judgment_timing, build_row_entry, build_row_grids, closest_lane_note_ns,
        collect_edge_judge_indices, completed_row_final_judgment,
        completed_row_flash_note_indices_and_judgment, compute_end_times_ns,
        count_rescore_tracks_on_row, crossed_mine_bounds_ns, crossed_mine_held_start_time,
        drain_audio_commands, drain_session_commands, effective_appearance_effects_for_player,
        effective_mini_percent_for_player, effective_player_global_offset_seconds,
        effective_scroll_effects_for_player, effective_visibility_effects_for_player,
        effective_visual_effects_for_player, enforce_max_simultaneous_notes,
        error_bar_register_tap, finalize_completed_mines, finalize_row_judgment,
        finalized_row_outcome_for_cached_row, frame_stable_display_music_time_ns, grade_to_window,
        handle_input, handle_queued_raw_key, hit_mine, input_queue_cap,
        integrate_active_hold_to_time, judge_a_tap, lane_edge_judges_lift, lane_edge_judges_tap,
        lane_edge_matches_note_type, lane_note_window_bounds_ns, lane_note_window_bounds_rows,
        lane_press_started, lane_release_finished, late_note_resolution_window_ns,
        live_autoplay_enabled_from_flags, max_step_distance_ns, mine_window_bounds_ns,
        missed_note_cutoff_row_for_timing, music_time_ns_from_song_clock, mutate_timing_arc,
        next_ready_row_in_lookahead, next_tick_mode, note_has_displayable_hold, note_hit_eval,
        parse_attack_mods, parse_song_lua_runtime_mods,
        player_draw_scale_for_tilt_with_visual_mask, player_row_scan_state, process_input_edges,
        recent_step_tracks, recompute_player_totals, refresh_active_attack_masks,
        refresh_timing_after_offset_change, render_provisional_early_rescore_feedback,
        replay_edge_cap, resolve_pending_missed_holds, row_entry_for_cached_row,
        row_final_grade_hides_note, score_invalid_reason_lines_for_chart, set_final_note_result,
        settle_completion_rows, song_time_ns_from_seconds, song_time_ns_to_seconds,
        stage_music_cut, start_active_hold, step_stats_density_graph_width,
        step_stats_notefield_width, suppress_final_bad_rescore_visual, sync_queued_raw_modifiers,
        tap_judgment_uses_bright_explosion, tick_mode_status_line, tick_visual_effects,
        trigger_completed_row_tap_explosions, trigger_hold_explosion, trigger_mine_explosion,
        trigger_receptor_step_pulse, trigger_tap_explosion, try_hit_crossed_mines_while_held,
        turn_option_bits, update_active_holds, update_judged_rows, update_lane_input_slot,
        visible_notefield_time_ns,
    };
    use crate::game::parsing::noteskin::{self, Noteskin, Style};
    use crate::game::parsing::song_lua::SongLuaNoteHideWindow;
    use crate::game::profile;
    use crate::screens::gameplay as screen_gameplay;
    use deadsync_chart::SongData;
    use deadsync_chart::notes::ParsedNote;
    use deadsync_chart::{ArrowStats, ChartData, GameplayChartData, StaminaCounts, TechCounts};
    use deadsync_core::input::{InputSource, Lane};
    use deadsync_core::note::NoteType;
    use deadsync_core::timing::ROWS_PER_BEAT;
    use deadsync_input::{InputEdge, InputEvent, VirtualAction};
    use deadsync_profile as profile_data;
    use deadsync_rules::judgment::{self, JudgeGrade, Judgment, TimingWindow};
    use deadsync_rules::note::{HoldData, HoldResult, MineResult, Note};
    use deadsync_rules::timing::{
        DelaySegment, FakeSegment, StopSegment, TimingData, TimingProfile, TimingSegments,
    };
    use std::sync::{Arc, LazyLock, Mutex};
    use std::time::{Duration, Instant};
    use std::{fs, path::PathBuf};
    use winit::keyboard::KeyCode;

    static SESSION_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn step_stats_notefield_width_matches_sl_style_widths() {
        assert_eq!(step_stats_notefield_width(4), Some(256.0));
        assert_eq!(step_stats_notefield_width(8), Some(512.0));
        assert_eq!(step_stats_notefield_width(0), None);
    }

    #[test]
    fn step_stats_density_graph_width_matches_sl_double() {
        let width = step_stats_density_graph_width(
            profile_data::PlayStyle::Double,
            8,
            1,
            854.0,
            480.0,
            true,
            false,
        );
        let expected = ((854.0 - 512.0) * 0.5) * 0.95;
        assert!((width - expected).abs() <= 0.000_1);
    }

    struct SessionRestore {
        play_style: profile_data::PlayStyle,
        player_side: profile_data::PlayerSide,
        p1_joined: bool,
        p2_joined: bool,
    }

    impl Drop for SessionRestore {
        fn drop(&mut self) {
            profile::set_session_play_style(self.play_style);
            profile::set_session_player_side(self.player_side);
            profile::set_session_joined(self.p1_joined, self.p2_joined);
        }
    }

    fn with_session<R>(
        play_style: profile_data::PlayStyle,
        player_side: profile_data::PlayerSide,
        p1_joined: bool,
        p2_joined: bool,
        f: impl FnOnce() -> R,
    ) -> R {
        let _lock = SESSION_TEST_LOCK.lock().expect("session test lock");
        let _restore = SessionRestore {
            play_style: profile::get_session_play_style(),
            player_side: profile::get_session_player_side(),
            p1_joined: profile::is_session_side_joined(profile_data::PlayerSide::P1),
            p2_joined: profile::is_session_side_joined(profile_data::PlayerSide::P2),
        };
        profile::set_session_play_style(play_style);
        profile::set_session_player_side(player_side);
        profile::set_session_joined(p1_joined, p2_joined);
        f()
    }

    fn test_row_to_beat(last_row: usize) -> Vec<f32> {
        (0..=last_row)
            .map(|row| row as f32 / ROWS_PER_BEAT as f32)
            .collect()
    }

    fn test_timing(last_row: usize) -> TimingData {
        TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(last_row),
        )
    }

    fn test_note(column: usize, row_index: usize, note_type: NoteType) -> Note {
        Note {
            beat: row_index as f32 / ROWS_PER_BEAT as f32,
            quantization_idx: 0,
            column,
            note_type,
            row_index,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    fn test_hold(column: usize, row_index: usize, end_row_index: usize) -> Note {
        let mut note = test_note(column, row_index, NoteType::Hold);
        note.hold = Some(HoldData {
            end_row_index,
            end_beat: end_row_index as f32 / ROWS_PER_BEAT as f32,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 0.0,
            last_held_row_index: row_index,
            last_held_beat: row_index as f32 / ROWS_PER_BEAT as f32,
        });
        note
    }

    fn test_roll(column: usize, row_index: usize, end_row_index: usize) -> Note {
        let mut note = test_hold(column, row_index, end_row_index);
        note.note_type = NoteType::Roll;
        note
    }

    fn judged_note(column: usize, row_index: usize, note_type: NoteType) -> Note {
        let mut note = test_note(column, row_index, note_type);
        note.result = Some(Judgment {
            time_error_ms: 0.0,
            time_error_music_ns: 0,
            grade: JudgeGrade::Great,
            window: None,
            miss_because_held: false,
        });
        note
    }

    fn note_with_judgment(
        column: usize,
        row_index: usize,
        note_type: NoteType,
        grade: JudgeGrade,
        time_error_ms: f32,
    ) -> Note {
        let mut note = test_note(column, row_index, note_type);
        note.result = Some(Judgment {
            time_error_ms,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(time_error_ms, 1.0),
            grade,
            window: None,
            miss_because_held: false,
        });
        note
    }

    fn gameplay_regression_chart() -> ChartData {
        ChartData {
            chart_type: "dance-double".to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 12,
            step_artist: String::new(),
            music_path: None,
            short_hash: "double-p2-regression".to_string(),
            stats: ArrowStats {
                total_arrows: 2,
                left: 0,
                down: 0,
                up: 0,
                right: 0,
                total_steps: 2,
                jumps: 0,
                hands: 0,
                mines: 0,
                holds: 0,
                rolls: 0,
                lifts: 0,
                fakes: 0,
                holding: 0,
            },
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 2.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 150.0,
            max_bpm: 150.0,
        }
    }

    fn gameplay_regression_song() -> SongData {
        SongData {
            simfile_path: PathBuf::from("songs/Tests/double-p2-regression.ssc"),
            title: "Double P2 Regression".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: "Tests".to_string(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: "150".to_string(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 150.0,
            max_bpm: 150.0,
            normalized_bpms: "0.000=150.000".to_string(),
            music_length_seconds: 60.0,
            first_second: 0.0,
            total_length_seconds: 60,
            precise_last_second_seconds: 60.0,
            charts: vec![gameplay_regression_chart()],
        }
    }

    fn gameplay_regression_payload() -> GameplayChartData {
        let parsed_notes = vec![
            ParsedNote {
                row_index: 48,
                column: 0,
                note_type: NoteType::Tap,
                tail_row_index: None,
            },
            ParsedNote {
                row_index: 96,
                column: 7,
                note_type: NoteType::Tap,
                tail_row_index: None,
            },
        ];
        let row_to_beat = test_row_to_beat(96);
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 150.0)],
            ..TimingSegments::default()
        };
        let timing = TimingData::from_segments(0.0, 0.0, &timing_segments, &row_to_beat);
        GameplayChartData {
            notes: Vec::new(),
            parsed_notes,
            row_to_beat,
            timing_segments,
            timing,
            chart_attacks: None,
        }
    }

    #[inline(always)]
    fn test_gameplay_tween(tween: noteskin::TweenType) -> super::GameplayTween {
        match tween {
            noteskin::TweenType::Linear => super::GameplayTween::Linear,
            noteskin::TweenType::Accelerate => super::GameplayTween::Accelerate,
            noteskin::TweenType::Decelerate => super::GameplayTween::Decelerate,
        }
    }

    #[inline(always)]
    fn test_gameplay_receptor_glow_behavior(
        behavior: noteskin::ReceptorGlowBehavior,
    ) -> super::GameplayReceptorGlowBehavior {
        super::GameplayReceptorGlowBehavior {
            press_duration: behavior.press_duration,
            press_alpha_start: behavior.press_alpha_start,
            press_alpha_end: behavior.press_alpha_end,
            press_zoom_start: behavior.press_zoom_start,
            press_zoom_end: behavior.press_zoom_end,
            press_tween: test_gameplay_tween(behavior.press_tween),
            duration: behavior.duration,
            alpha_start: behavior.alpha_start,
            alpha_end: behavior.alpha_end,
            zoom_start: behavior.zoom_start,
            zoom_end: behavior.zoom_end,
            tween: test_gameplay_tween(behavior.tween),
            blend_add: behavior.blend_add,
        }
    }

    #[inline(always)]
    fn test_gameplay_receptor_step_behavior(
        behavior: noteskin::ReceptorStepBehavior,
    ) -> super::GameplayReceptorStepBehavior {
        super::GameplayReceptorStepBehavior {
            duration: behavior.duration,
            zoom_start: behavior.zoom_start,
            zoom_end: behavior.zoom_end,
            tween: test_gameplay_tween(behavior.tween),
            interrupts: behavior.interrupts,
        }
    }

    fn test_noteskin_data(
        cols_per_player: usize,
        num_players: usize,
        player_profiles: &[profile_data::Profile; MAX_PLAYERS],
        session: &super::GameplaySession,
    ) -> super::GameplayNoteskinData {
        let style = Style {
            num_cols: cols_per_player,
            num_players: 1,
        };
        let mut runtime_profiles = (*player_profiles).clone();
        if session.p2_runtime_player() {
            runtime_profiles[0] = runtime_profiles[1].clone();
        }
        let noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] = std::array::from_fn(|player| {
            if player >= num_players {
                return None;
            }
            let skin = runtime_profiles[player].noteskin.to_string();
            noteskin::load_itg_skin_cached(&style, &skin).ok()
        });
        let mine_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] = std::array::from_fn(|player| {
            if player >= num_players {
                return None;
            }
            let skin = runtime_profiles[player]
                .resolved_mine_noteskin()
                .to_string();
            noteskin::load_itg_skin_cached(&style, &skin)
                .ok()
                .or_else(|| noteskin[player].clone())
        });
        let receptor_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] =
            std::array::from_fn(|player| {
                if player >= num_players {
                    return None;
                }
                let skin = runtime_profiles[player]
                    .resolved_receptor_noteskin()
                    .to_string();
                noteskin::load_itg_skin_cached(&style, &skin)
                    .ok()
                    .or_else(|| noteskin[player].clone())
            });
        let tap_explosion_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] =
            std::array::from_fn(|player| {
                if player >= num_players {
                    return None;
                }
                let Some(skin) = runtime_profiles[player].resolved_tap_explosion_noteskin() else {
                    return None;
                };
                noteskin::load_itg_skin_cached(&style, skin.as_str())
                    .ok()
                    .or_else(|| noteskin[player].clone())
            });
        let mut effects = super::GameplayNoteskinEffects::default();
        let cols = cols_per_player.min(MAX_COLS);
        for player in 0..num_players.min(MAX_PLAYERS) {
            let receptor_ns = receptor_noteskin[player]
                .as_deref()
                .or_else(|| noteskin[player].as_deref());
            if let Some(ns) = receptor_ns {
                effects.set_receptor_glow_behavior(
                    player,
                    test_gameplay_receptor_glow_behavior(ns.receptor_glow_behavior),
                );
                for col in 0..cols {
                    for window in super::RECEPTOR_STEP_WINDOWS {
                        effects.set_receptor_step_behavior(
                            player,
                            col,
                            window,
                            test_gameplay_receptor_step_behavior(
                                ns.receptor_step_behavior_for_col(col, window),
                            ),
                        );
                    }
                }
            }

            let tap_ns = if runtime_profiles[player].tap_explosion_noteskin_hidden() {
                None
            } else {
                tap_explosion_noteskin[player]
                    .as_deref()
                    .or_else(|| noteskin[player].as_deref())
            };
            if let Some(ns) = tap_ns {
                for col in 0..cols {
                    for window in super::TAP_EXPLOSION_WINDOWS {
                        for bright in [false, true] {
                            effects.set_tap_explosion_duration(
                                player,
                                col,
                                window,
                                bright,
                                ns.tap_explosion_for_col_with_bright(col, window, bright)
                                    .map(|explosion| explosion.duration()),
                            );
                        }
                    }
                }
            }

            let duration = mine_noteskin[player]
                .as_deref()
                .or_else(|| noteskin[player].as_deref())
                .and_then(|ns| ns.mine_hit_explosion.as_ref())
                .map_or(super::MINE_EXPLOSION_DURATION, |explosion| {
                    explosion.duration()
                });
            effects.set_mine_explosion_duration(player, duration);
        }
        super::GameplayNoteskinData { effects }
    }

    fn regression_state(player_profiles: [profile_data::Profile; MAX_PLAYERS]) -> super::State {
        let song = Arc::new(gameplay_regression_song());
        let chart = Arc::new(song.charts[0].clone());
        let charts = [chart.clone(), chart];
        let gameplay_chart = Arc::new(gameplay_regression_payload());
        let gameplay_charts = [gameplay_chart.clone(), gameplay_chart];
        let session = super::GameplaySession::default();
        let noteskin_data = test_noteskin_data(
            session.play_style.cols_per_player(),
            session.play_style.player_count(),
            &player_profiles,
            &session,
        );
        super::init(
            song,
            charts,
            gameplay_charts,
            super::GameplayViewport::default(),
            session,
            super::GameplayConfig::default(),
            deadsync_chart::SyncPref::Default,
            super::GameplayMiniIndicatorData::default(),
            noteskin_data,
            super::GameplaySongLuaData::default(),
            5,
            1.0,
            [
                player_profiles[0].scroll_speed,
                player_profiles[1].scroll_speed,
            ],
            player_profiles,
            None,
            None,
            None,
            None,
            None,
            None,
            [0; MAX_PLAYERS],
        )
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("deadsync-gameplay-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn generated_runtime_mod_lua() -> &'static str {
        r#"
mods = {
    {0, 9999, "*1000 no beat, *1000 no drunk, *1000 no tipsy, *1000 no invert, *1000 no flip, *1000 no dizzy", "end"},
}
mod_time = {
    {0.00, 999, "*1 0 Dark1, *1 0 Dark2, *1 0 Dark3, *1 0 Dark4, *1 0 PulseOuter, *1 0 PulseOffset, *1 0 Wave, *1 0 Bumpy3, *1 0 BumpyPeriod, *1 0 Stealth, *1 0 Blind, *1 0 Sudden, *1 0 Tipsy, *1 0 Drunk, *1 0 Dark", "len"},
}
mods_ease = {}

local l = "len"
local function me(...)
    table.insert(mods_ease, {...})
end

me(4, 0.75, 250, 0, "Bumpy1", l, ease.outQuad)
me(4, 0.75, -125, 0, "BumpyPeriod", l, ease.outQuad)
me(4, 0.75, 75, 0, "Wave", l, ease.outElastic)
me(8, 0.75, 250, 0, "Bumpy2", l, ease.outQuad)
me(12, 0.75, 250, 0, "Bumpy3", l, ease.outQuad)
me(16, 0.75, 250, 0, "Bumpy4", l, ease.outQuad)
me(20, 1.5, 50, 1, "hidden", l, ease.outInQuad)
me(24, 0.5, 25, 0, "beat", l, ease.outBounce)

return Def.ActorFrame{}
"#
    }

    fn generated_lua_song_simfile() -> &'static str {
        r#"#VERSION:0.83;
#TITLE:Generated Lua Regression;
#MUSIC:;
#OFFSET:0.000;
#BPMS:0.000=120.000;
#FGCHANGES:0.000=lua/default.lua=1.000=0=0=0=StretchNoLoop====;

#NOTEDATA:;
#STEPSTYPE:dance-single;
#DESCRIPTION:Generated;
#DIFFICULTY:Challenge;
#METER:12;
#RADARVALUES:0,0,0,0,0;
#NOTES:
0000
0000
0000
1000
,
0100
0000
0010
0001
,
1000
0100
0010
0001
,
0010
0001
1000
0100
,
0001
0010
0100
1000
,
1000
0000
0100
0000
,
0010
0000
0001
0000
;
"#
    }

    fn write_generated_lua_song_fixture() -> PathBuf {
        let song_dir = test_dir("generated-lua-song");
        let lua_dir = song_dir.join("lua");
        fs::create_dir_all(&lua_dir).unwrap();
        fs::write(lua_dir.join("default.lua"), generated_runtime_mod_lua()).unwrap();
        let simfile = song_dir.join("generated_lua_regression.ssc");
        fs::write(&simfile, generated_lua_song_simfile()).unwrap();
        simfile
    }

    #[test]
    fn gameplay_handles_generated_song_lua_actor_build() {
        let simfile = write_generated_lua_song_fixture();
        const SONG_LUA_TEST_STACK: usize = 16 * 1024 * 1024;
        std::thread::Builder::new()
            .name("song-lua-actor-build-regression".to_string())
            .stack_size(SONG_LUA_TEST_STACK)
            .spawn(move || {
                let song = Arc::new(
                    crate::game::parsing::simfile::parse_song_for_test(&simfile, 0.0)
                        .expect("generated lua simfile should parse"),
                );
                let chart_ix = song
                    .charts
                    .iter()
                    .position(|chart| chart.difficulty.eq_ignore_ascii_case("challenge"))
                    .unwrap_or(0);
                let gameplay_chart = Arc::new(
                    crate::game::parsing::simfile::load_gameplay_charts(&song, &[chart_ix], 0.0)
                        .expect("generated lua gameplay chart should load")
                        .remove(0),
                );
                let chart = Arc::new(song.charts[chart_ix].clone());
                let mut player_profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                player_profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                player_profiles[1].scroll_speed = ScrollSpeedSetting::CMod(516.0);

                with_session(
                    profile_data::PlayStyle::Single,
                    profile_data::PlayerSide::P1,
                    true,
                    false,
                    || {
                        let session = super::GameplaySession::default();
                        let charts = [chart.clone(), chart];
                        let gameplay_charts = [gameplay_chart.clone(), gameplay_chart];
                        let scroll_speed = [
                            player_profiles[0].scroll_speed,
                            player_profiles[1].scroll_speed,
                        ];
                        let noteskin_data = test_noteskin_data(
                            session.play_style.cols_per_player(),
                            session.play_style.player_count(),
                            &player_profiles,
                            &session,
                        );
                        let runtime_profiles =
                            screen_gameplay::gameplay_runtime_profiles(&player_profiles, &session);
                        let noteskin_assets = screen_gameplay::gameplay_noteskin_assets(
                            session.play_style.cols_per_player(),
                            session.play_style.player_count(),
                            &runtime_profiles,
                        );
                        let context = super::song_lua_compile_context(
                            song.as_ref(),
                            &charts,
                            session.play_style.player_count(),
                            &player_profiles,
                            &scroll_speed,
                            1.0,
                            0.0,
                            super::GameplayViewport::default(),
                            &session,
                            false,
                        );
                        let primary = song
                            .foreground_lua_changes
                            .iter()
                            .find(|change| change.start_beat <= 0.0 && change.path.is_file())
                            .map(|change| {
                                crate::game::parsing::song_lua::compile_song_lua(
                                    &change.path,
                                    &context,
                                )
                                .expect("generated song lua should compile")
                            })
                            .map(|compiled| super::GameplayCompiledSongLua {
                                compiled,
                                compile_ms: 0.0,
                            });
                        let song_lua_data = super::GameplaySongLuaData {
                            primary,
                            ..Default::default()
                        };
                        let mut state = screen_gameplay::State::from_gameplay(
                            super::init(
                                song,
                                charts,
                                gameplay_charts,
                                super::GameplayViewport::default(),
                                session,
                                super::GameplayConfig::default(),
                                deadsync_chart::SyncPref::Default,
                                super::GameplayMiniIndicatorData::default(),
                                noteskin_data,
                                song_lua_data,
                                5,
                                1.0,
                                scroll_speed,
                                player_profiles,
                                None,
                                None,
                                None,
                                None,
                                None,
                                None,
                                [0; MAX_PLAYERS],
                            ),
                            noteskin_assets,
                        );
                        assert!(!state.song_lua_ease_windows[0].is_empty());

                        let mut times = vec![0.0, state.current_music_time_display];
                        for window in &state.song_lua_ease_windows[0] {
                            times.push(window.start_second);
                            times.push((window.start_second + window.end_second) * 0.5);
                            times.push(window.end_second);
                            times.push(window.sustain_end_second);
                        }
                        times.sort_by(f32::total_cmp);
                        times.dedup_by(|a, b| (*a - *b).abs() <= 0.001);

                        let assets = crate::assets::AssetManager::new();
                        for time in times {
                            state.current_music_time_display = time;
                            state.current_music_time_visible = [time; MAX_PLAYERS];
                            state.current_beat = state.timing.get_beat_for_time(time);
                            refresh_active_attack_masks(&mut state.gameplay, 0.0);
                            let mut actors = Vec::new();
                            screen_gameplay::push_actors(
                                &mut actors,
                                &mut state,
                                &assets,
                                screen_gameplay::ActorViewOverride::default(),
                            );
                        }
                    },
                );
            })
            .expect("song-lua actor build regression thread should spawn")
            .join()
            .expect("song-lua actor build regression thread should finish");
    }

    fn set_regression_mine(
        state: &mut super::State,
        note_index: usize,
        column: usize,
        row_index: usize,
        time_ns: super::SongTimeNs,
    ) {
        state.notes[note_index] = test_note(column, row_index, NoteType::Mine);
        state.note_time_cache_ns[note_index] = time_ns;
        state.mine_note_ix[0] = vec![note_index];
        state.mine_note_time_ns[0] = vec![time_ns];
        state.next_mine_ix_cursor[0] = 0;
        state.next_mine_avoid_cursor[0] = note_index;
        state.mines_total[0] = 1;
    }

    fn set_state_timing(state: &mut super::State, timing: Arc<TimingData>) {
        state.timing = Arc::clone(&timing);
        state.timing_players[0] = Arc::clone(&timing);
        state.timing_players[1] = timing;
    }

    #[test]
    fn regression_state_passes_hot_state_audit() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let state = regression_state(profiles);
        super::assert_valid_hot_state_for_tests(&state, 0.0, state.current_music_time_display);
    }

    fn test_row_entry(
        notes: &[Note],
        row_index: usize,
        nonmine_note_indices: Vec<usize>,
    ) -> RowEntry {
        let note_time_cache_ns = vec![0; notes.len()];
        let mut row_note_indices = [usize::MAX; MAX_COLS];
        let nonmine_note_count = nonmine_note_indices.len() as u8;
        for (i, note_index) in nonmine_note_indices.into_iter().enumerate() {
            row_note_indices[i] = note_index;
        }
        build_row_entry(
            row_index,
            row_note_indices,
            nonmine_note_count,
            notes,
            &note_time_cache_ns,
        )
    }

    fn test_row_entry_with_times(
        notes: &[Note],
        note_time_cache_ns: &[super::SongTimeNs],
        row_index: usize,
        nonmine_note_indices: Vec<usize>,
    ) -> RowEntry {
        let mut row_note_indices = [usize::MAX; MAX_COLS];
        let nonmine_note_count = nonmine_note_indices.len() as u8;
        for (i, note_index) in nonmine_note_indices.into_iter().enumerate() {
            row_note_indices[i] = note_index;
        }
        build_row_entry(
            row_index,
            row_note_indices,
            nonmine_note_count,
            notes,
            note_time_cache_ns,
        )
    }

    fn test_input_event(action: VirtualAction) -> InputEvent {
        test_input_event_with_source(action, true, InputSource::Keyboard)
    }

    fn test_input_event_with_source(
        action: VirtualAction,
        pressed: bool,
        source: InputSource,
    ) -> InputEvent {
        let now = Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed,
            source,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    fn test_input_edge_at(
        lane: Lane,
        pressed: bool,
        event_music_time_ns: super::SongTimeNs,
    ) -> InputEdge {
        let now = Instant::now();
        InputEdge {
            lane,
            input_slot: 0,
            pressed,
            source: InputSource::Keyboard,
            record_replay: false,
            captured_at: now,
            captured_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
            queued_at: now,
            event_music_time_ns,
        }
    }

    #[test]
    fn gameplay_init_uses_p2_modifiers_for_double_p2() {
        with_session(
            profile_data::PlayStyle::Double,
            profile_data::PlayerSide::P2,
            false,
            true,
            || {
                let mut p1 = profile_data::Profile::default();
                p1.display_name = "P1 runtime".to_string();
                p1.scroll_speed = ScrollSpeedSetting::XMod(1.5);
                p1.perspective = profile_data::Perspective::Overhead;
                p1.judgment_graphic = profile_data::JudgmentGraphic::new("Love");

                let mut p2 = profile_data::Profile::default();
                p2.display_name = "P2 runtime".to_string();
                p2.scroll_speed = ScrollSpeedSetting::CMod(777.0);
                p2.perspective = profile_data::Perspective::Space;
                p2.judgment_graphic = profile_data::JudgmentGraphic::new("Bebas");

                let state = regression_state([p1, p2.clone()]);

                assert_eq!(state.num_players, 1);
                assert_eq!(state.scroll_speed[0], ScrollSpeedSetting::CMod(777.0));
                assert_eq!(state.player_profiles[0].display_name, "P2 runtime");
                assert_eq!(
                    state.player_profiles[0].perspective,
                    profile_data::Perspective::Space
                );
                assert_eq!(
                    state.player_profiles[0].judgment_graphic,
                    p2.judgment_graphic
                );
                assert_eq!(state.player_color_index, 3);
            },
        );
    }

    #[test]
    fn gameplay_handle_input_uses_p2_menu_buttons_for_double_p2() {
        with_session(
            profile_data::PlayStyle::Double,
            profile_data::PlayerSide::P2,
            false,
            true,
            || {
                let state_profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                let mut state = regression_state(state_profiles);

                handle_input(&mut state, &test_input_event(VirtualAction::p1_start));
                assert_eq!(state.hold_to_exit_key, None);

                handle_input(&mut state, &test_input_event(VirtualAction::p2_start));
                assert_eq!(state.hold_to_exit_key, Some(HoldToExitKey::Start));
                assert!(state.hold_to_exit_start.is_some());
            },
        );
    }

    #[test]
    fn gameplay_handle_input_uses_p2_menu_buttons_for_versus() {
        with_session(
            profile_data::PlayStyle::Versus,
            profile_data::PlayerSide::P1,
            true,
            true,
            || {
                let state_profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];

                let mut start_state = regression_state(state_profiles.clone());
                assert_eq!(start_state.num_players, 2);
                handle_input(&mut start_state, &test_input_event(VirtualAction::p2_start));
                assert_eq!(start_state.hold_to_exit_key, Some(HoldToExitKey::Start));
                assert!(start_state.hold_to_exit_start.is_some());

                let mut back_state = regression_state(state_profiles);
                assert_eq!(back_state.num_players, 2);
                handle_input(&mut back_state, &test_input_event(VirtualAction::p2_back));
                assert_eq!(back_state.hold_to_exit_key, Some(HoldToExitKey::Back));
                assert!(back_state.hold_to_exit_start.is_some());
            },
        );
    }

    #[test]
    fn gameplay_lane_input_keeps_back_hold_active() {
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let state_profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                let mut state = regression_state(state_profiles);

                handle_input(&mut state, &test_input_event(VirtualAction::p1_back));
                let hold_start = state.hold_to_exit_start;

                handle_input(
                    &mut state,
                    &test_input_event_with_source(
                        VirtualAction::p1_left,
                        true,
                        InputSource::Gamepad,
                    ),
                );
                handle_input(
                    &mut state,
                    &test_input_event_with_source(
                        VirtualAction::p1_left,
                        false,
                        InputSource::Gamepad,
                    ),
                );

                assert_eq!(state.hold_to_exit_key, Some(HoldToExitKey::Back));
                assert_eq!(state.hold_to_exit_start, hold_start);
                assert_eq!(state.hold_to_exit_aborted_at, None);
            },
        );
    }

    #[test]
    fn delayed_back_false_exits_song_on_first_press() {
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let state_profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                let mut state = regression_state(state_profiles);
                state.config.delayed_back = false;

                handle_input(&mut state, &test_input_event(VirtualAction::p1_back));

                let exit = state.exit_transition;
                let hold_key = state.hold_to_exit_key;

                assert!(
                    exit.is_some(),
                    "exit_transition should be armed immediately when delayed_back is false"
                );
                assert_eq!(
                    exit.unwrap().kind,
                    ExitTransitionKind::Cancel,
                    "BACK should trigger a Cancel exit transition"
                );
                assert_eq!(
                    hold_key, None,
                    "hold_to_exit_key should remain unset in instant-back mode"
                );
            },
        );
    }

    #[test]
    fn delayed_back_true_preserves_hold_arming() {
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let state_profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                let mut state = regression_state(state_profiles);
                state.config.delayed_back = true;

                handle_input(&mut state, &test_input_event(VirtualAction::p1_back));

                let hold_key = state.hold_to_exit_key;
                let hold_start = state.hold_to_exit_start;
                let exit = state.exit_transition;

                assert_eq!(hold_key, Some(HoldToExitKey::Back));
                assert!(hold_start.is_some());
                assert!(
                    exit.is_none(),
                    "exit_transition should not fire until the hold elapses"
                );
            },
        );
    }

    #[test]
    fn begin_restart_exit_arms_cancel_transition_like_back_out() {
        let state_profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(state_profiles);
        assert!(state.exit_transition.is_none());

        super::begin_restart_exit(&mut state);

        let exit = state
            .exit_transition
            .expect("begin_restart_exit should arm an exit_transition");
        assert_eq!(
            exit.kind,
            ExitTransitionKind::Cancel,
            "restart should reuse the fast Cancel out-fade for SL/zmod parity"
        );
        assert_eq!(
            drain_audio_commands(&mut state).collect::<Vec<_>>(),
            vec![GameplayAudioCommand::StopMusic]
        );
    }

    #[test]
    fn begin_restart_exit_is_idempotent_when_already_exiting() {
        let state_profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(state_profiles);

        // Pretend a give-up exit is already in flight.
        super::begin_exit_transition(&mut state, ExitTransitionKind::Out);
        let original = state.exit_transition.expect("primed exit");

        super::begin_restart_exit(&mut state);
        let after = state.exit_transition.expect("still exiting");
        assert_eq!(
            after.kind, original.kind,
            "begin_restart_exit must not overwrite an in-flight exit transition"
        );
    }

    #[test]
    fn positive_song_offset_delta_moves_notes_earlier_like_global_offset() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut song_state = regression_state(profiles.clone());
        let mut global_state = regression_state(profiles);

        let song_offset_before = song_state.song_offset_seconds;
        let global_offset_before = global_state.global_offset_seconds;
        let song_before = song_state.note_time_cache_ns[0];
        let global_before = global_state.note_time_cache_ns[0];

        assert!(apply_song_offset_delta(&mut song_state, 0.010));
        assert!(apply_global_offset_delta(&mut global_state, 0.010));

        let song_after = song_state.note_time_cache_ns[0];
        let global_after = global_state.note_time_cache_ns[0];
        let expected_delta_ns = song_time_ns_from_seconds(0.010);
        let song_delta_ns = song_before - song_after;
        let global_delta_ns = global_before - global_after;

        assert!((song_state.song_offset_seconds - (song_offset_before + 0.010)).abs() <= 1e-6);
        assert!(
            (global_state.global_offset_seconds - (global_offset_before + 0.010)).abs() <= 1e-6
        );
        assert!((song_delta_ns - expected_delta_ns).abs() <= 1);
        assert!((global_delta_ns - expected_delta_ns).abs() <= 1);
        assert!((song_delta_ns - global_delta_ns).abs() <= 1);
    }

    #[test]
    fn global_offset_delta_preserves_player_shift() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        let shift = 0.015_f32;

        state.player_global_offset_shift_seconds[0] = shift;
        mutate_timing_arc(&mut state.timing_players[0], |timing| {
            timing.set_global_offset_seconds(state.global_offset_seconds + shift)
        });
        refresh_timing_after_offset_change(&mut state);

        let machine_before = state.global_offset_seconds;
        let effective_before = effective_player_global_offset_seconds(&state, 0);
        let note_before = state.note_time_cache_ns[0];

        assert!((effective_before - (machine_before + shift)).abs() <= 1e-6);
        assert!(apply_global_offset_delta(&mut state, 0.010));

        let effective_after = effective_player_global_offset_seconds(&state, 0);
        let note_after = state.note_time_cache_ns[0];

        assert!((state.global_offset_seconds - (machine_before + 0.010)).abs() <= 1e-6);
        assert!((effective_after - (state.global_offset_seconds + shift)).abs() <= 1e-6);
        assert_eq!(note_before - note_after, song_time_ns_from_seconds(0.010));
    }

    #[test]
    fn synced_raw_modifier_makes_first_offset_key_use_global_offset() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);

        let song_before = state.song_offset_seconds;
        let global_before = state.global_offset_seconds;

        sync_queued_raw_modifiers(&mut state, true, false);
        let _ = handle_queued_raw_key(&mut state, KeyCode::F12, true, Instant::now(), 0.0, true);

        assert!((state.song_offset_seconds - song_before).abs() <= 1e-6);
        assert!(
            (state.global_offset_seconds - (global_before + OFFSET_ADJUST_STEP_SECONDS)).abs()
                <= 1e-6
        );
    }

    #[test]
    fn advance_hold_last_held_keeps_progressing_after_release_while_life_remains() {
        let timing =
            TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &test_row_to_beat(96));
        let mut hold = test_hold(0, 0, 96).hold.expect("test hold has hold data");
        hold.last_held_row_index = 24;
        hold.last_held_beat = 24.0 / ROWS_PER_BEAT as f32;

        advance_hold_last_held(&mut hold, &timing, 1.0, 0, 0.0);

        assert_eq!(hold.last_held_row_index, 48);
        assert!((hold.last_held_beat - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn advance_hold_last_held_keeps_exact_beat_between_rows() {
        let timing =
            TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &test_row_to_beat(96));
        let mut hold = test_hold(0, 0, 96).hold.expect("test hold has hold data");
        hold.last_held_row_index = 24;
        hold.last_held_beat = 24.0 / ROWS_PER_BEAT as f32;

        advance_hold_last_held(&mut hold, &timing, 0.99, 0, 0.0);

        assert_eq!(hold.last_held_row_index, 48);
        assert!((hold.last_held_beat - 0.99).abs() <= 1e-6);
    }

    fn test_chart(
        stats: ArrowStats,
        timing_segments: TimingSegments,
        chart_attacks: Option<&str>,
    ) -> ChartData {
        let mines_nonfake = stats.mines;
        let (raw_min_bpm, raw_max_bpm) = timing_segments.bpms.iter().fold(
            (f32::INFINITY, 0.0_f32),
            |(min_bpm, max_bpm), &(_, bpm)| {
                if !bpm.is_finite() || bpm <= 0.0 {
                    (min_bpm, max_bpm)
                } else {
                    (min_bpm.min(bpm), max_bpm.max(bpm))
                }
            },
        );
        let (min_bpm, max_bpm) = if raw_min_bpm.is_finite() {
            (raw_min_bpm as f64, raw_max_bpm as f64)
        } else {
            (0.0, 0.0)
        };
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 10,
            step_artist: String::new(),
            music_path: None,
            short_hash: String::new(),
            stats,
            tech_counts: TechCounts::default(),
            mines_nonfake,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: chart_attacks.is_some_and(|attacks| !attacks.trim().is_empty()),
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm,
            max_bpm,
        }
    }

    #[test]
    fn tick_mode_cycles() {
        let mode = next_tick_mode(TickMode::Off);
        assert_eq!(mode, TickMode::Assist);
        assert_eq!(next_tick_mode(mode), TickMode::Hit);
        assert_eq!(next_tick_mode(TickMode::Hit), TickMode::Off);
    }

    #[test]
    fn timing_tick_key_queues_session_command() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);

        let action =
            handle_queued_raw_key(&mut state, KeyCode::F7, true, Instant::now(), 0.0, true);

        assert!(matches!(action, super::RawKeyAction::None));
        assert_eq!(super::timing_tick_status_line(&state), Some("Assist Tick"));
        assert_eq!(
            drain_session_commands(&mut state).collect::<Vec<_>>(),
            vec![GameplaySessionCommand::SetTimingTickMode(TickMode::Assist)]
        );
    }

    #[test]
    fn lane_press_counts_hold_until_last_alias_release() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);

        let mut transitions = Vec::new();
        for (source, slot, pressed) in [
            (InputSource::Keyboard, 10, true),
            (InputSource::Keyboard, 11, true),
            (InputSource::Keyboard, 10, false),
            (InputSource::Keyboard, 11, false),
            (InputSource::Gamepad, 7, true),
            (InputSource::Keyboard, 12, true),
            (InputSource::Gamepad, 7, false),
            (InputSource::Keyboard, 12, false),
        ] {
            let update = update_lane_input_slot(&mut state, Lane::Left, source, slot, pressed);
            transitions.push((update.was_down, update.is_down, update.slot_was_down));
        }

        assert_eq!(
            transitions,
            vec![
                (false, true, false),
                (true, true, false),
                (true, true, true),
                (true, false, true),
                (false, true, false),
                (true, true, false),
                (true, true, true),
                (true, false, true),
            ]
        );
        assert_eq!(state.input_slot_count, 0);
        assert_eq!(state.input_lane_counts[Lane::Left.index()], 0);
    }

    #[test]
    fn physical_edges_still_judge_while_lane_is_logically_held() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);

        let mut tap_edges = Vec::new();
        let mut lift_edges = Vec::new();
        let mut glow_edges = Vec::new();
        for (slot, pressed) in [
            (10, true),
            (10, true),
            (11, true),
            (10, false),
            (10, false),
            (11, false),
        ] {
            let update = update_lane_input_slot(
                &mut state,
                Lane::Left,
                InputSource::Keyboard,
                slot,
                pressed,
            );
            tap_edges.push(lane_edge_judges_tap(pressed, update.slot_was_down));
            lift_edges.push(lane_edge_judges_lift(pressed, update.slot_was_down));
            glow_edges.push((
                lane_press_started(pressed, update.was_down, update.is_down),
                lane_release_finished(pressed, update.was_down, update.is_down),
            ));
        }

        assert_eq!(tap_edges, vec![true, false, true, false, false, false]);
        assert_eq!(lift_edges, vec![false, false, false, true, false, true]);
        assert_eq!(
            glow_edges,
            vec![
                (true, false),
                (false, false),
                (false, false),
                (false, false),
                (false, false),
                (false, true)
            ]
        );
    }

    #[test]
    fn live_autoplay_helper_excludes_replays() {
        assert!(live_autoplay_enabled_from_flags(true, false));
        assert!(!live_autoplay_enabled_from_flags(true, true));
        assert!(!live_autoplay_enabled_from_flags(false, false));
    }

    #[test]
    fn live_autoplay_forces_active_hold_pressed_state() {
        assert!(active_hold_counts_as_pressed(true, false));
        assert!(active_hold_counts_as_pressed(true, true));
        assert!(active_hold_counts_as_pressed(false, true));
        assert!(!active_hold_counts_as_pressed(false, false));
    }

    #[test]
    fn hold_life_advance_keeps_pressed_holds_full() {
        let advanced = advance_hold_life_ns(
            NoteType::Hold,
            0.25,
            true,
            song_time_ns_from_seconds(0.2),
            1.0,
        );
        assert_eq!(
            advanced,
            super::HoldLifeAdvance {
                life_after: super::MAX_HOLD_LIFE,
                zero_elapsed_music_ns: None,
            }
        );
    }

    #[test]
    fn hold_life_advance_reports_exact_zero_cross_time() {
        let advanced = advance_hold_life_ns(
            NoteType::Hold,
            0.25,
            false,
            song_time_ns_from_seconds(0.2),
            1.0,
        );
        assert_eq!(advanced.life_after, 0.0);
        let zero_elapsed = advanced
            .zero_elapsed_music_ns
            .expect("hold should cross zero");
        assert!((song_time_ns_to_seconds(zero_elapsed) - 0.080375).abs() <= 1e-6);
    }

    #[test]
    fn hold_life_advance_split_intervals_match_single_interval() {
        let whole = advance_hold_life_ns(
            NoteType::Hold,
            1.0,
            false,
            song_time_ns_from_seconds(0.16),
            1.0,
        );
        let first = advance_hold_life_ns(
            NoteType::Hold,
            1.0,
            false,
            song_time_ns_from_seconds(0.05),
            1.0,
        );
        let split = advance_hold_life_ns(
            NoteType::Hold,
            first.life_after,
            false,
            song_time_ns_from_seconds(0.11),
            1.0,
        );

        assert!((whole.life_after - split.life_after).abs() <= 1e-6);
        assert_eq!(whole.zero_elapsed_music_ns, split.zero_elapsed_music_ns);
    }

    #[test]
    fn active_hold_let_go_visual_row_uses_frame_target() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        let timing = Arc::new(test_timing(ROWS_PER_BEAT as usize * 4));
        state.timing = timing.clone();
        state.timing_players = [timing.clone(), timing];

        let hold_end_ns = song_time_ns_from_seconds(2.0);
        state.notes[0] = test_hold(0, 0, ROWS_PER_BEAT as usize * 2);
        state.hold_end_time_cache_ns[0] = Some(hold_end_ns);
        state.notes[0].hold.as_mut().expect("test hold").life = 0.25;
        state.active_holds[0] = Some(super::ActiveHold {
            note_index: 0,
            start_time_ns: 0,
            end_time_ns: hold_end_ns,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: false,
            life: 0.25,
            last_update_time_ns: 0,
        });

        let target_ns = song_time_ns_from_seconds(0.2);
        integrate_active_hold_to_time(&mut state, 0, target_ns);

        let hold = state.notes[0].hold.as_ref().expect("test hold");
        assert_eq!(hold.result, Some(HoldResult::LetGo));
        assert!(state.active_holds[0].is_none());
        assert!((hold.last_held_beat - 0.2).abs() <= 1e-6);
        assert!(hold.last_held_beat > TIMING_WINDOW_SECONDS_HOLD * 0.25 + f32::EPSILON);
    }

    #[test]
    fn early_next_hold_start_settles_previous_same_column_hold() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        let previous_end_ns = song_time_ns_from_seconds(1.0);
        let next_start_ns = song_time_ns_from_seconds(1.09375);
        let next_end_ns = song_time_ns_from_seconds(1.375);

        state.notes[0] = test_hold(0, 0, ROWS_PER_BEAT as usize);
        state.notes[1] = test_hold(0, ROWS_PER_BEAT as usize + 12, ROWS_PER_BEAT as usize * 2);
        state.hold_end_time_cache_ns[0] = Some(previous_end_ns);
        state.hold_end_time_cache_ns[1] = Some(next_end_ns);
        state.active_holds[0] = Some(super::ActiveHold {
            note_index: 0,
            start_time_ns: 0,
            end_time_ns: previous_end_ns,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: true,
            life: super::MAX_HOLD_LIFE,
            last_update_time_ns: song_time_ns_from_seconds(0.95),
        });

        start_active_hold(
            &mut state,
            0,
            1,
            next_start_ns,
            next_end_ns,
            song_time_ns_from_seconds(0.95),
        );

        assert_eq!(
            state.notes[0].hold.as_ref().and_then(|hold| hold.result),
            Some(HoldResult::Held)
        );
        assert_eq!(
            state.active_holds[0]
                .as_ref()
                .map(|active| active.note_index),
            Some(1)
        );
    }

    #[test]
    fn roll_life_advance_scales_zero_cross_with_music_rate() {
        let advanced = advance_hold_life_ns(
            NoteType::Roll,
            0.5,
            false,
            song_time_ns_from_seconds(0.4),
            2.0,
        );
        assert_eq!(advanced.life_after, 0.0);
        let zero_elapsed = advanced
            .zero_elapsed_music_ns
            .expect("roll should cross zero");
        assert!((song_time_ns_to_seconds(zero_elapsed) - 0.3515).abs() <= 1e-6);
    }

    #[test]
    fn roll_step_refreshes_before_event_time_decay() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        let mut roll = test_roll(0, 0, ROWS_PER_BEAT as usize * 4);
        roll.result = Some(Judgment {
            time_error_ms: 0.0,
            time_error_music_ns: 0,
            grade: JudgeGrade::Fantastic,
            window: Some(TimingWindow::W1),
            miss_because_held: false,
        });
        state.notes[0] = roll;

        let event_time_ns = song_time_ns_from_seconds(super::TIMING_WINDOW_SECONDS_ROLL + 0.01);
        state.active_holds[0] = Some(super::ActiveHold {
            note_index: 0,
            start_time_ns: 0,
            end_time_ns: song_time_ns_from_seconds(2.0),
            note_type: NoteType::Roll,
            let_go: false,
            is_pressed: false,
            life: super::MAX_HOLD_LIFE,
            last_update_time_ns: 0,
        });
        state
            .pending_edges
            .push_back(test_input_edge_at(Lane::Left, true, event_time_ns));

        let now = Instant::now();
        let clock = super::SongClockSnapshot {
            song_time_ns: event_time_ns,
            seconds_per_second: 1.0,
            mapped_audio: true,
            valid_at: now,
            valid_at_host_nanos: 0,
            timing_diag_enabled: false,
            timing_diag_callback_gap_ns: 0,
        };
        let mut phase_timings = super::GameplayUpdatePhaseTimings::default();
        process_input_edges(&mut state, false, &mut phase_timings, clock);

        let active = state.active_holds[0]
            .as_ref()
            .expect("roll should remain active after the body step");
        assert_eq!(active.life, super::MAX_HOLD_LIFE);
        assert_eq!(active.last_update_time_ns, event_time_ns);
        let hold = state.notes[0].hold.as_ref().expect("roll hold data");
        assert_eq!(hold.result, None);
        assert_eq!(hold.life, super::MAX_HOLD_LIFE);
    }

    #[test]
    fn live_input_resolves_invalid_edge_time_from_song_clock() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        let event_time_ns = song_time_ns_from_seconds(12.345);
        let edge = test_input_edge_at(Lane::Left, true, super::INVALID_SONG_TIME_NS);
        let captured_at = edge.captured_at;
        state.pending_edges.push_back(edge);

        let clock = super::SongClockSnapshot {
            song_time_ns: event_time_ns,
            seconds_per_second: 1.0,
            mapped_audio: true,
            valid_at: captured_at,
            valid_at_host_nanos: 0,
            timing_diag_enabled: false,
            timing_diag_callback_gap_ns: 0,
        };
        let mut phase_timings = super::GameplayUpdatePhaseTimings::default();
        process_input_edges(&mut state, false, &mut phase_timings, clock);

        assert_eq!(state.lane_pressed_since_ns[0], Some(event_time_ns));
    }

    #[test]
    fn empty_live_press_steps_receptor() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        let event_time_ns = song_time_ns_from_seconds(12.345);
        state
            .pending_edges
            .push_back(test_input_edge_at(Lane::Left, true, event_time_ns));

        let now = Instant::now();
        let clock = super::SongClockSnapshot {
            song_time_ns: event_time_ns,
            seconds_per_second: 1.0,
            mapped_audio: true,
            valid_at: now,
            valid_at_host_nanos: 0,
            timing_diag_enabled: false,
            timing_diag_callback_gap_ns: 0,
        };
        let mut phase_timings = super::GameplayUpdatePhaseTimings::default();
        process_input_edges(&mut state, false, &mut phase_timings, clock);

        assert!(state.receptor_bop_timers[0] > 0.0);
    }

    #[test]
    fn rescore_track_count_keeps_chord_rows_multi_note_after_partial_judgment() {
        let row_index = 48usize;
        let notes = vec![
            judged_note(0, row_index, NoteType::Tap),
            test_note(1, row_index, NoteType::Tap),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        assert_eq!(count_rescore_tracks_on_row(&row_entry), 2);
    }

    #[test]
    fn rescore_track_count_includes_lifts_on_row() {
        let row_index = 48usize;
        let notes = vec![
            test_note(0, row_index, NoteType::Tap),
            test_note(1, row_index, NoteType::Lift),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        assert_eq!(count_rescore_tracks_on_row(&row_entry), 2);
    }

    #[test]
    fn cached_row_entry_lookup_uses_row_map_cache() {
        let row_index = 48usize;
        let notes = vec![
            test_note(0, row_index, NoteType::Tap),
            test_note(1, row_index, NoteType::Tap),
        ];
        let row_entries = vec![test_row_entry(&notes, row_index, vec![0, 1])];
        let mut row_map_cache = vec![u32::MAX; row_index + 1];
        row_map_cache[row_index] = 0;

        let row_entry = row_entry_for_cached_row(&row_entries, &row_map_cache, row_index)
            .expect("expected cached row entry");

        assert_eq!(row_entry.row_index, row_index);
        assert_eq!(row_entry.note_indices(), &[0, 1]);
    }

    #[test]
    fn cached_row_entry_lookup_keeps_duplicate_rows_player_specific() {
        let row_index = 48usize;
        let notes = vec![
            test_note(0, row_index, NoteType::Tap),
            test_note(1, row_index, NoteType::Tap),
            test_note(4, row_index, NoteType::Tap),
            test_note(5, row_index, NoteType::Tap),
        ];
        let row_entries = vec![
            test_row_entry(&notes, row_index, vec![0, 1]),
            test_row_entry(&notes, row_index, vec![2, 3]),
        ];
        let mut row_map_cache: [Vec<u32>; MAX_PLAYERS] =
            std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        row_map_cache[0][row_index] = 0;
        row_map_cache[1][row_index] = 1;

        let p1 = row_entry_for_cached_row(&row_entries, &row_map_cache[0], row_index)
            .expect("expected cached p1 row entry");
        let p2 = row_entry_for_cached_row(&row_entries, &row_map_cache[1], row_index)
            .expect("expected cached p2 row entry");

        assert_eq!(p1.note_indices(), &[0, 1]);
        assert_eq!(p2.note_indices(), &[2, 3]);
    }

    #[test]
    fn finalized_row_outcome_lookup_uses_row_map_cache() {
        let row_index = 48usize;
        let notes = vec![test_note(0, row_index, NoteType::Tap)];
        let mut row_entries = vec![test_row_entry(&notes, row_index, vec![0])];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });
        let mut row_map_cache = vec![u32::MAX; row_index + 1];
        row_map_cache[row_index] = 0;

        let outcome = finalized_row_outcome_for_cached_row(&row_entries, &row_map_cache, row_index)
            .expect("expected cached finalized row outcome");

        assert_eq!(outcome.final_grade, JudgeGrade::Great);
    }

    #[test]
    fn judged_row_scan_finds_later_ready_row_past_pending_middle_row() {
        let row1 = 48usize;
        let row2 = 96usize;
        let row3 = 144usize;
        let notes = vec![
            note_with_judgment(0, row1, NoteType::Tap, JudgeGrade::Great, -8.0),
            note_with_judgment(2, row1, NoteType::Tap, JudgeGrade::Great, 8.0),
            note_with_judgment(1, row2, NoteType::Tap, JudgeGrade::Great, -8.0),
            test_note(3, row2, NoteType::Tap),
            note_with_judgment(0, row3, NoteType::Tap, JudgeGrade::Great, -6.0),
            note_with_judgment(2, row3, NoteType::Tap, JudgeGrade::Excellent, 4.0),
        ];
        let note_time_cache_ns = vec![
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(3.0),
            song_time_ns_from_seconds(3.0),
        ];
        let mut row_entries = vec![
            test_row_entry_with_times(&notes, &note_time_cache_ns, row1, vec![0, 1]),
            test_row_entry_with_times(&notes, &note_time_cache_ns, row2, vec![2, 3]),
            test_row_entry_with_times(&notes, &note_time_cache_ns, row3, vec![4, 5]),
        ];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });

        let cursor = advance_judged_row_cursor(0, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, song_time_ns_from_seconds(3.5))
        });
        assert_eq!(cursor, 1);

        let ready = next_ready_row_in_lookahead(cursor, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, song_time_ns_from_seconds(3.5))
        });
        assert_eq!(ready, Some((2, row3, false)));
    }

    #[test]
    fn judged_row_cursor_stays_on_earliest_pending_row_until_it_finishes() {
        let row1 = 48usize;
        let row2 = 96usize;
        let row3 = 144usize;
        let notes = vec![
            note_with_judgment(0, row1, NoteType::Tap, JudgeGrade::Great, -8.0),
            note_with_judgment(2, row1, NoteType::Tap, JudgeGrade::Great, 8.0),
            note_with_judgment(1, row2, NoteType::Tap, JudgeGrade::Great, -8.0),
            test_note(3, row2, NoteType::Tap),
            note_with_judgment(0, row3, NoteType::Tap, JudgeGrade::Great, -6.0),
            note_with_judgment(2, row3, NoteType::Tap, JudgeGrade::Excellent, 4.0),
        ];
        let note_time_cache_ns = vec![
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(3.0),
            song_time_ns_from_seconds(3.0),
        ];
        let mut row_entries = vec![
            test_row_entry_with_times(&notes, &note_time_cache_ns, row1, vec![0, 1]),
            test_row_entry_with_times(&notes, &note_time_cache_ns, row2, vec![2, 3]),
            test_row_entry_with_times(&notes, &note_time_cache_ns, row3, vec![4, 5]),
        ];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });
        row_entries[2].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });

        let pending_cursor = advance_judged_row_cursor(0, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, song_time_ns_from_seconds(3.5))
        });
        assert_eq!(pending_cursor, 1);

        row_entries[1].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });
        let advanced_cursor = advance_judged_row_cursor(0, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, song_time_ns_from_seconds(3.5))
        });
        assert_eq!(advanced_cursor, 3);
    }

    #[test]
    fn completed_row_final_judgment_waits_for_full_jump() {
        let row_index = 48usize;
        let notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            test_note(1, row_index, NoteType::Tap),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        assert!(completed_row_final_judgment(&notes, &row_entry).is_none());
    }

    #[test]
    fn completed_row_final_judgment_uses_last_hit_on_jump() {
        let row_index = 48usize;
        let notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            note_with_judgment(1, row_index, NoteType::Tap, JudgeGrade::Excellent, 8.0),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        let judgment = completed_row_final_judgment(&notes, &row_entry)
            .expect("completed jump should have a final row judgment");

        assert_eq!(judgment.grade, JudgeGrade::Excellent);
        assert!(row_final_grade_hides_note(judgment.grade));
    }

    #[test]
    fn completed_row_final_judgment_keeps_w4_w5_rows_visible() {
        let row_index = 48usize;
        let decent_notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            note_with_judgment(1, row_index, NoteType::Tap, JudgeGrade::Decent, 96.0),
        ];
        let wayoff_notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            note_with_judgment(1, row_index, NoteType::Tap, JudgeGrade::WayOff, 140.0),
        ];
        let row_entry = test_row_entry(&decent_notes, row_index, vec![0, 1]);

        let decent = completed_row_final_judgment(&decent_notes, &row_entry)
            .expect("completed row should produce a final Decent");
        let wayoff = completed_row_final_judgment(&wayoff_notes, &row_entry)
            .expect("completed row should produce a final Way Off");

        assert_eq!(decent.grade, JudgeGrade::Decent);
        assert_eq!(wayoff.grade, JudgeGrade::WayOff);
        assert!(!row_final_grade_hides_note(decent.grade));
        assert!(!row_final_grade_hides_note(wayoff.grade));
    }

    #[test]
    fn jump_row_finalization_uses_row_judgment_for_error_bar_hud() {
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let mut p1 = profile_data::Profile::default();
                p1.error_ms_display = true;
                p1.error_bar_text = true;
                p1.error_bar_active_mask = profile_data::ErrorBarMask::TEXT;

                let mut state = regression_state([p1, profile_data::Profile::default()]);
                let row_index = 48usize;
                state.notes = vec![
                    test_note(0, row_index, NoteType::Tap),
                    test_note(1, row_index, NoteType::Tap),
                ];
                state.note_time_cache_ns = vec![
                    song_time_ns_from_seconds(1.0),
                    song_time_ns_from_seconds(1.0),
                ];
                state.row_entries = vec![test_row_entry_with_times(
                    &state.notes,
                    &state.note_time_cache_ns,
                    row_index,
                    vec![0, 1],
                )];
                state.row_entry_ranges = [(0, 1), (0, 0)];
                state.row_map_cache = std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
                state.row_map_cache[0][row_index] = 0;
                state.note_row_entry_indices = vec![0, 0];
                state.judged_row_cursor = [0; MAX_PLAYERS];
                state.current_music_time_ns = song_time_ns_from_seconds(1.096);
                state.total_elapsed_in_screen = 12.0;

                set_final_note_result(
                    &mut state,
                    0,
                    Judgment {
                        time_error_ms: -12.0,
                        time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(
                            -12.0, 1.0,
                        ),
                        grade: JudgeGrade::Great,
                        window: Some(TimingWindow::W3),
                        miss_because_held: false,
                    },
                );
                set_final_note_result(
                    &mut state,
                    1,
                    Judgment {
                        time_error_ms: 96.0,
                        time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(
                            96.0, 1.0,
                        ),
                        grade: JudgeGrade::Decent,
                        window: Some(TimingWindow::W4),
                        miss_because_held: false,
                    },
                );

                assert!(state.players[0].offset_indicator_text.is_none());
                assert!(state.players[0].error_bar_text.is_none());

                finalize_row_judgment(&mut state, 0, row_index, 0, false);

                let offset = state.players[0]
                    .offset_indicator_text
                    .expect("row-final judgment should drive the offset indicator");
                assert_eq!(offset.started_at, 12.0);
                assert_eq!(offset.offset_ms, 96.0);
                assert_eq!(offset.window, TimingWindow::W4);

                let early_late = state.players[0]
                    .error_bar_text
                    .expect("row-final judgment should drive the early/late text");
                assert_eq!(early_late.started_at, 12.0);
                assert!(!early_late.early);
                assert_eq!(early_late.offset_ms, 96.0);
                assert!(!early_late.scaled);

                let last = state.players[0]
                    .last_judgment
                    .as_ref()
                    .expect("row-final judgment should update the judgment sprite");
                assert_eq!(last.judgment.grade, JudgeGrade::Decent);
                assert_eq!(last.judgment.time_error_ms, 96.0);
                assert_eq!(last.started_at_screen_s, 12.0);
            },
        );
    }

    #[test]
    fn error_bar_text_uses_10ms_blue_fantastic_threshold() {
        let p1 = profile_data::Profile {
            show_fa_plus_window: true,
            fa_plus_10ms_blue_window: true,
            custom_fantastic_window: false,
            error_bar_text: true,
            error_bar_active_mask: profile_data::ErrorBarMask::TEXT,
            ..profile_data::Profile::default()
        };

        let mut state = regression_state([p1, profile_data::Profile::default()]);
        state.total_elapsed_in_screen = 4.0;

        error_bar_register_tap(
            &mut state,
            0,
            &Judgment {
                time_error_ms: 8.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(8.0, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W0),
                miss_because_held: false,
            },
            1.0,
        );
        assert!(state.players[0].error_bar_text.is_none());

        error_bar_register_tap(
            &mut state,
            0,
            &Judgment {
                time_error_ms: 12.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(12.0, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W0),
                miss_because_held: false,
            },
            1.1,
        );

        let text = state.players[0]
            .error_bar_text
            .expect("12ms should exceed Arrow Cloud's 10ms blue window");
        assert_eq!(text.started_at, 4.0);
        assert!(!text.early);
        assert_eq!(text.offset_ms, 12.0);
        assert!(!text.scaled);
    }

    #[test]
    fn text_error_bar_scalable_mode_surfaces_default_window_fantastics() {
        let p1 = profile_data::Profile {
            show_fa_plus_window: false,
            error_bar_text: true,
            text_error_bar_scalable: true,
            error_bar_active_mask: profile_data::ErrorBarMask::TEXT,
            ..profile_data::Profile::default()
        };

        let mut state = regression_state([p1, profile_data::Profile::default()]);
        state.total_elapsed_in_screen = 4.0;

        error_bar_register_tap(
            &mut state,
            0,
            &Judgment {
                time_error_ms: 10.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(10.0, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
            1.0,
        );
        assert!(
            state.players[0].error_bar_text.is_none(),
            "10ms exactly should remain hidden"
        );

        error_bar_register_tap(
            &mut state,
            0,
            &Judgment {
                time_error_ms: -10.1,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(-10.1, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
            1.1,
        );

        let text = state.players[0]
            .error_bar_text
            .expect(">10ms should show even inside the default Fantastic window");
        assert_eq!(text.started_at, 4.0);
        assert!(text.early);
        assert_eq!(text.offset_ms, 10.1);
        assert!(text.scaled);
        assert_eq!(text.scale_start_ms, 10.0);
    }

    #[test]
    fn text_error_bar_scalable_mode_uses_custom_threshold() {
        let p1 = profile_data::Profile {
            show_fa_plus_window: false,
            error_bar_text: true,
            text_error_bar_scalable: true,
            text_error_bar_threshold_ms: 17,
            error_bar_active_mask: profile_data::ErrorBarMask::TEXT,
            ..profile_data::Profile::default()
        };

        let mut state = regression_state([p1, profile_data::Profile::default()]);
        state.total_elapsed_in_screen = 4.0;

        error_bar_register_tap(
            &mut state,
            0,
            &Judgment {
                time_error_ms: 16.9,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(16.9, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
            1.0,
        );
        assert!(
            state.players[0].error_bar_text.is_none(),
            "custom threshold should hide hits at or below the selected ms value"
        );

        error_bar_register_tap(
            &mut state,
            0,
            &Judgment {
                time_error_ms: 17.1,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(17.1, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
            1.1,
        );

        let text = state.players[0]
            .error_bar_text
            .expect("> custom threshold should show inside the default Fantastic window");
        assert!(!text.early);
        assert_eq!(text.offset_ms, 17.1);
        assert!(text.scaled);
        assert_eq!(text.scale_start_ms, 17.0);
    }

    #[test]
    fn text_error_bar_window_mode_preserves_default_threshold() {
        let p1 = profile_data::Profile {
            show_fa_plus_window: false,
            error_bar_text: true,
            text_error_bar_scalable: false,
            error_bar_active_mask: profile_data::ErrorBarMask::TEXT,
            ..profile_data::Profile::default()
        };

        let mut state = regression_state([p1, profile_data::Profile::default()]);
        state.total_elapsed_in_screen = 4.0;

        error_bar_register_tap(
            &mut state,
            0,
            &Judgment {
                time_error_ms: 12.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(12.0, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
            1.0,
        );
        assert!(
            state.players[0].error_bar_text.is_none(),
            "legacy Text mode should keep the active-window threshold"
        );

        error_bar_register_tap(
            &mut state,
            0,
            &Judgment {
                time_error_ms: 24.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(24.0, 1.0),
                grade: JudgeGrade::Excellent,
                window: Some(TimingWindow::W2),
                miss_because_held: false,
            },
            1.1,
        );

        let text = state.players[0]
            .error_bar_text
            .expect("legacy Text mode should still show hits outside the active window");
        assert_eq!(text.started_at, 4.0);
        assert!(!text.early);
        assert_eq!(text.offset_ms, 24.0);
        assert!(!text.scaled);
    }

    #[test]
    fn average_error_bar_can_show_long_term_only() {
        let p1 = profile_data::Profile {
            error_bar_active_mask: profile_data::ErrorBarMask::AVERAGE,
            short_average_error_bar_enabled: false,
            long_error_bar_enabled: true,
            long_error_bar_threshold_ms: 1,
            long_error_bar_min_samples: 4,
            ..profile_data::Profile::default()
        };
        let mut state = regression_state([p1, profile_data::Profile::default()]);
        state.total_elapsed_in_screen = 4.0;

        for i in 0..4 {
            error_bar_register_tap(
                &mut state,
                0,
                &Judgment {
                    time_error_ms: 10.0,
                    time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(10.0, 1.0),
                    grade: JudgeGrade::Fantastic,
                    window: Some(TimingWindow::W1),
                    miss_because_held: false,
                },
                i as f32 * 0.1,
            );
        }

        let player = &state.players[0];
        assert!(player.error_bar_avg_bar_started_at.is_none());
        assert!(player.error_bar_avg_ticks.iter().all(Option::is_none));
        assert!(player.error_bar_long_avg_visible);
        let long_tick = player
            .error_bar_long_avg_tick
            .expect("long-only Average should still emit the blue tick");
        assert!((long_tick.offset_s - 0.010).abs() <= 1e-6);
    }

    #[test]
    fn autosync_row_hits_use_music_time_offsets_at_rate() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        let row_index = 48usize;
        let autosync_offset_ns = song_time_ns_from_seconds(0.015);

        state.music_rate = 1.5;
        state.autosync_mode = super::AutosyncMode::Song;
        state.notes = vec![test_note(0, row_index, NoteType::Tap)];
        state.notes[0].result = Some(Judgment {
            time_error_ms: -10.0,
            time_error_music_ns: -autosync_offset_ns,
            grade: JudgeGrade::Great,
            window: Some(TimingWindow::W3),
            miss_because_held: false,
        });
        state.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.row_entries = vec![test_row_entry_with_times(
            &state.notes,
            &state.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.autosync_offset_samples = [autosync_offset_ns; super::AUTOSYNC_OFFSET_SAMPLE_COUNT];
        state.autosync_offset_sample_count = super::AUTOSYNC_OFFSET_SAMPLE_COUNT - 1;

        apply_autosync_for_row_hits(&mut state, 0);

        assert!((state.song_offset_seconds - 0.015).abs() <= 1e-6);
        assert_eq!(state.autosync_offset_sample_count, 0);
    }

    #[test]
    fn hold_judgment_cleanup_uses_screen_time_boundary() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        state.total_elapsed_in_screen = 5.0;
        state.hold_judgments[0] = Some(HoldJudgmentRenderInfo {
            result: HoldResult::Held,
            started_at_screen_s: 4.201,
        });
        tick_visual_effects(&mut state, 0.0);
        assert!(state.hold_judgments[0].is_some());

        state.hold_judgments[0] = Some(HoldJudgmentRenderInfo {
            result: HoldResult::Held,
            started_at_screen_s: 4.2,
        });
        tick_visual_effects(&mut state, 0.0);
        assert!(state.hold_judgments[0].is_none());
    }

    #[test]
    fn held_miss_feedback_records_column_and_cleans_up() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        state.total_elapsed_in_screen = 5.0;
        state.notes = vec![test_note(2, 48, NoteType::Tap)];
        state.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.row_entries = vec![test_row_entry_with_times(
            &state.notes,
            &state.note_time_cache_ns,
            48,
            vec![0],
        )];
        state.note_row_entry_indices = vec![0];

        set_final_note_result(
            &mut state,
            0,
            Judgment {
                time_error_ms: 180.0,
                time_error_music_ns: song_time_ns_from_seconds(0.18),
                grade: JudgeGrade::Miss,
                window: None,
                miss_because_held: true,
            },
        );

        assert!(state.held_miss_judgments[0].is_none());
        assert!(state.held_miss_judgments[1].is_none());
        assert_eq!(
            state.held_miss_judgments[2]
                .as_ref()
                .map(|info| info.started_at_screen_s),
            Some(5.0)
        );

        state.held_miss_judgments[2] = Some(HeldMissRenderInfo {
            started_at_screen_s: 5.0 - HELD_MISS_TOTAL_DURATION,
        });
        tick_visual_effects(&mut state, 0.0);
        assert!(state.held_miss_judgments[2].is_none());
    }

    #[test]
    fn mine_judgment_feedback_records_result_column_and_time() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        state.total_elapsed_in_screen = 9.25;

        super::set_last_mine_judgment(&mut state, 0, 2, MineResult::Avoided);

        let info = state.players[0]
            .last_mine_judgment
            .expect("mine judgment should be recorded");
        assert_eq!(info.result, MineResult::Avoided);
        assert_eq!(info.column, 2);
        assert_eq!(info.started_at_screen_s, 9.25);
    }

    #[test]
    fn hidden_song_lua_tap_steps_receptor_without_core_flash() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        let row_index = 48usize;
        let column = 1usize;
        trigger_receptor_step_pulse(&mut state, 0);
        let supports_press_tween = state.receptor_glow_press_timers[0] > f32::EPSILON;
        state.receptor_glow_press_timers.fill(0.0);
        state.receptor_glow_timers.fill(0.0);
        state.receptor_bop_timers.fill(0.0);
        state.notes = vec![note_with_judgment(
            column,
            row_index,
            NoteType::Tap,
            JudgeGrade::Great,
            0.0,
        )];
        state.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.row_entries = vec![test_row_entry_with_times(
            &state.notes,
            &state.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.row_map_cache = std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.row_map_cache[0][row_index] = 0;
        state.song_lua_note_hides[0].push(SongLuaNoteHideWindow {
            player: 0,
            column,
            start_beat: 0.0,
            end_beat: 2.0,
        });

        trigger_completed_row_tap_explosions(&mut state, 0, row_index);

        assert!(state.tap_explosions[column].is_none());
        assert_eq!(state.receptor_bop_timers[column], 0.0);
        assert_eq!(state.receptor_bop_behaviors[column].duration, 0.0);
        if supports_press_tween {
            assert!(state.receptor_glow_press_timers[column] > 0.0);
            assert_eq!(state.receptor_glow_timers[column], 0.0);
        }
    }

    #[test]
    fn visible_tap_hit_uses_score_receptor_command_with_core_flash() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        let row_index = 48usize;
        let column = 1usize;
        let note_time = song_time_ns_from_seconds(1.0);
        state.notes = vec![test_note(column, row_index, NoteType::Tap)];
        state.note_time_cache_ns = vec![note_time];
        state.lane_note_indices[column].push(0);
        state.lane_note_row_indices[column].push(0);
        state.note_row_entry_indices = vec![0];
        state.row_entries = vec![test_row_entry_with_times(
            &state.notes,
            &state.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.row_entry_ranges = [(0, 1), (0, 0)];
        state.row_map_cache = std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.row_map_cache[0][row_index] = 0;

        assert!(judge_a_tap(&mut state, column, note_time));
        assert!(state.tap_explosions[column].is_some());
        assert_eq!(state.receptor_bop_timers[column], 0.0);
        assert_eq!(state.receptor_bop_behaviors[column].duration, 0.0);
    }

    #[test]
    fn devcel_visible_tap_hit_uses_score_receptor_command() {
        let mut profile = profile_data::Profile::default();
        profile.noteskin = profile_data::NoteSkin::new("devcel-2024");
        let mut state = regression_state([profile, profile_data::Profile::default()]);
        let row_index = 48usize;
        let column = 1usize;
        let note_time = song_time_ns_from_seconds(1.0);
        state.notes = vec![test_note(column, row_index, NoteType::Tap)];
        state.note_time_cache_ns = vec![note_time];
        state.lane_note_indices[column].push(0);
        state.lane_note_row_indices[column].push(0);
        state.note_row_entry_indices = vec![0];
        state.row_entries = vec![test_row_entry_with_times(
            &state.notes,
            &state.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.row_entry_ranges = [(0, 1), (0, 0)];
        state.row_map_cache = std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.row_map_cache[0][row_index] = 0;

        assert!(judge_a_tap(&mut state, column, note_time));
        assert!(state.tap_explosions[column].is_some());
        assert_eq!(state.receptor_bop_timers[column], 0.0);
        assert_eq!(state.receptor_bop_behaviors[column].duration, 0.0);
    }

    #[test]
    fn tap_explosion_mask_disables_selected_tap_window() {
        let column = 1usize;
        let mut enabled = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        trigger_tap_explosion(&mut enabled, column, JudgeGrade::Great);
        assert!(enabled.tap_explosions[column].is_some());

        let mut profile = profile_data::Profile::default();
        profile
            .tap_explosion_active_mask
            .remove(profile_data::TapExplosionMask::GREAT);
        let mut disabled = regression_state([profile, profile_data::Profile::default()]);
        trigger_tap_explosion(&mut disabled, column, JudgeGrade::Great);
        assert!(disabled.tap_explosions[column].is_none());
    }

    #[test]
    fn column_flash_mask_gates_completed_row_flash() {
        let row_index = 48usize;
        let column = 1usize;
        let build_state = |mask| {
            let mut profile = profile_data::Profile::default();
            profile.column_flash_on_miss = true;
            profile.column_flash_mask = mask;
            let mut state = regression_state([profile, profile_data::Profile::default()]);
            state.notes = vec![note_with_judgment(
                column,
                row_index,
                NoteType::Tap,
                JudgeGrade::Great,
                0.0,
            )];
            state.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
            state.row_entries = vec![test_row_entry_with_times(
                &state.notes,
                &state.note_time_cache_ns,
                row_index,
                vec![0],
            )];
            state.row_map_cache = std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
            state.row_map_cache[0][row_index] = 0;
            state
        };

        let mut disabled = build_state(profile_data::ColumnFlashMask::EXCELLENT);
        trigger_completed_row_tap_explosions(&mut disabled, 0, row_index);
        assert!(disabled.column_flashes[column].is_none());
        // The ungated SMX feedback record is written even when the on-screen column flash is
        // masked off; the pad lighting relies on this decoupling.
        let judged = disabled.last_tap_judgments[column]
            .expect("masked column flash should still record the ungated tap judgment");
        assert_eq!(judged.grade, JudgeGrade::Great);

        let mut enabled = build_state(profile_data::ColumnFlashMask::GREAT);
        trigger_completed_row_tap_explosions(&mut enabled, 0, row_index);
        let flash = enabled.column_flashes[column].expect("Great should trigger column flash");
        assert_eq!(flash.grade, JudgeGrade::Great);
    }

    #[test]
    fn mine_hit_records_screen_time_and_refreshes_on_rehit() {
        let column = 1usize;
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        state.play_mine_sounds = false;
        // A hit records the current screen time on the explosion.
        state.total_elapsed_in_screen = 1.0;
        trigger_mine_explosion(&mut state, column);
        let first = state.mine_explosions[column]
            .as_ref()
            .expect("mine hit should set an explosion");
        assert_eq!(first.started_at_screen_s, 1.0);
        // A later hit on the same column refreshes the timestamp even while the explosion is
        // still present, which is what lets the SMX panel diff tell consecutive hits apart.
        state.total_elapsed_in_screen = 1.5;
        trigger_mine_explosion(&mut state, column);
        let second = state.mine_explosions[column]
            .as_ref()
            .expect("re-hit should keep an explosion");
        assert_eq!(second.started_at_screen_s, 1.5);
    }

    fn fantastic_row_state(
        mask: profile_data::ColumnFlashMask,
        time_error_ms: f32,
        window: TimingWindow,
    ) -> (super::State, usize, usize) {
        let row_index = 48usize;
        let column = 1usize;
        let mut profile = profile_data::Profile::default();
        profile.noteskin = profile_data::NoteSkin::new(profile_data::NoteSkin::CEL_NAME);
        profile.show_fa_plus_window = true;
        profile.column_flash_on_miss = true;
        profile.column_flash_mask = mask;
        let mut state = regression_state([profile, profile_data::Profile::default()]);
        let mut note = note_with_judgment(
            column,
            row_index,
            NoteType::Tap,
            JudgeGrade::Fantastic,
            time_error_ms,
        );
        note.result
            .as_mut()
            .expect("test note should carry a judgment")
            .window = Some(window);
        state.notes = vec![note];
        state.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.row_entries = vec![test_row_entry_with_times(
            &state.notes,
            &state.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.row_map_cache = std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.row_map_cache[0][row_index] = 0;
        (state, row_index, column)
    }

    #[test]
    fn white_fantastic_column_flash_uses_only_white_mask() {
        let (mut disabled, row_index, column) = fantastic_row_state(
            profile_data::ColumnFlashMask::BLUE_FANTASTIC,
            18.0,
            TimingWindow::W1,
        );
        trigger_completed_row_tap_explosions(&mut disabled, 0, row_index);
        assert!(disabled.column_flashes[column].is_none());

        let (mut enabled, row_index, column) = fantastic_row_state(
            profile_data::ColumnFlashMask::WHITE_FANTASTIC,
            18.0,
            TimingWindow::W1,
        );
        trigger_completed_row_tap_explosions(&mut enabled, 0, row_index);
        let flash = enabled.column_flashes[column].expect("white Fantastic should flash");
        assert_eq!(flash.grade, JudgeGrade::Fantastic);
        assert!(!flash.blue_fantastic);
    }

    #[test]
    fn blue_fantastic_column_flash_uses_only_blue_mask() {
        let (mut disabled, row_index, column) = fantastic_row_state(
            profile_data::ColumnFlashMask::WHITE_FANTASTIC,
            4.0,
            TimingWindow::W0,
        );
        trigger_completed_row_tap_explosions(&mut disabled, 0, row_index);
        assert!(disabled.column_flashes[column].is_none());

        let (mut enabled, row_index, column) = fantastic_row_state(
            profile_data::ColumnFlashMask::BLUE_FANTASTIC,
            4.0,
            TimingWindow::W0,
        );
        trigger_completed_row_tap_explosions(&mut enabled, 0, row_index);
        let flash = enabled.column_flashes[column].expect("blue Fantastic should flash");
        assert_eq!(flash.grade, JudgeGrade::Fantastic);
        assert!(flash.blue_fantastic);
    }

    #[test]
    fn early_dw_column_flash_hide_is_independent_from_notefield_flash() {
        let column = 1usize;
        let judgment = Judgment {
            time_error_ms: -120.0,
            time_error_music_ns: song_time_ns_from_seconds(-0.12),
            grade: JudgeGrade::Decent,
            window: Some(TimingWindow::W4),
            miss_because_held: false,
        };
        let build_state = || {
            let mut profile = profile_data::Profile::default();
            profile.column_flash_on_miss = true;
            profile.column_flash_mask = profile_data::ColumnFlashMask::DECENT;
            regression_state([profile, profile_data::Profile::default()])
        };

        let mut hide_notefield = build_state();
        render_provisional_early_rescore_feedback(
            &mut hide_notefield,
            0,
            column,
            &judgment,
            1.0,
            true,
            true,
            false,
        );
        assert!(hide_notefield.tap_explosions[column].is_none());
        assert!(hide_notefield.column_flashes[column].is_some());

        let mut hide_column = build_state();
        render_provisional_early_rescore_feedback(
            &mut hide_column,
            0,
            column,
            &judgment,
            1.0,
            true,
            false,
            true,
        );
        assert!(hide_column.tap_explosions[column].is_some());
        assert!(hide_column.column_flashes[column].is_none());
    }

    #[test]
    fn miss_grade_maps_to_miss_tap_explosion_window() {
        assert_eq!(grade_to_window(JudgeGrade::Miss), Some("Miss"));
    }

    #[test]
    fn tap_explosion_mask_disables_held_success_flash() {
        let column = 1usize;
        let mut enabled = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        trigger_hold_explosion(&mut enabled, column);
        assert!(enabled.tap_explosions[column].is_some());

        let mut profile = profile_data::Profile::default();
        profile
            .tap_explosion_active_mask
            .remove(profile_data::TapExplosionMask::HELD);
        let mut disabled = regression_state([profile, profile_data::Profile::default()]);
        trigger_hold_explosion(&mut disabled, column);
        assert!(disabled.tap_explosions[column].is_none());
    }

    #[test]
    fn white_fantastic_row_uses_bright_tap_explosion() {
        let mut profile = profile_data::Profile::default();
        profile.noteskin = profile_data::NoteSkin::new(profile_data::NoteSkin::CEL_NAME);
        profile.show_fa_plus_window = true;
        let mut state = regression_state([profile, profile_data::Profile::default()]);
        let row_index = 48usize;
        let column = 1usize;
        let mut note = note_with_judgment(
            column,
            row_index,
            NoteType::Tap,
            JudgeGrade::Fantastic,
            18.0,
        );
        note.result.as_mut().unwrap().window = Some(TimingWindow::W1);
        state.notes = vec![note];
        state.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.row_entries = vec![test_row_entry_with_times(
            &state.notes,
            &state.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.row_map_cache = std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.row_map_cache[0][row_index] = 0;

        trigger_completed_row_tap_explosions(&mut state, 0, row_index);

        let active = state.tap_explosions[column].expect("white Fantastic should flash");
        assert_eq!(active.window, "W1");
        assert!(active.bright);
    }

    #[test]
    fn blue_fantastic_row_uses_dim_tap_explosion() {
        let mut profile = profile_data::Profile::default();
        profile.noteskin = profile_data::NoteSkin::new(profile_data::NoteSkin::CEL_NAME);
        profile.show_fa_plus_window = true;
        let mut state = regression_state([profile, profile_data::Profile::default()]);
        let row_index = 48usize;
        let column = 1usize;
        let mut note =
            note_with_judgment(column, row_index, NoteType::Tap, JudgeGrade::Fantastic, 4.0);
        note.result.as_mut().unwrap().window = Some(TimingWindow::W0);
        state.notes = vec![note];
        state.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.row_entries = vec![test_row_entry_with_times(
            &state.notes,
            &state.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.row_map_cache = std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.row_map_cache[0][row_index] = 0;

        trigger_completed_row_tap_explosions(&mut state, 0, row_index);

        let active = state.tap_explosions[column].expect("blue Fantastic should flash");
        assert_eq!(active.window, "W1");
        assert!(!active.bright);
    }

    #[test]
    fn ten_ms_blue_window_uses_bright_tap_explosion_above_10ms() {
        let mut profile = profile_data::Profile::default();
        profile.show_fa_plus_window = true;
        profile.fa_plus_10ms_blue_window = true;
        let state = regression_state([profile, profile_data::Profile::default()]);
        let judgment = Judgment {
            time_error_ms: 12.0,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(12.0, 1.0),
            grade: JudgeGrade::Fantastic,
            window: Some(TimingWindow::W0),
            miss_because_held: false,
        };

        assert!(tap_judgment_uses_bright_explosion(&state, 0, &judgment));
    }

    #[test]
    fn split_15_10ms_keeps_dim_tap_explosion_above_10ms() {
        let mut profile = profile_data::Profile::default();
        profile.show_fa_plus_window = true;
        profile.fa_plus_10ms_blue_window = true;
        profile.split_15_10ms = true;
        let state = regression_state([profile, profile_data::Profile::default()]);
        let judgment = Judgment {
            time_error_ms: 12.0,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(12.0, 1.0),
            grade: JudgeGrade::Fantastic,
            window: Some(TimingWindow::W0),
            miss_because_held: false,
        };

        assert!(!tap_judgment_uses_bright_explosion(&state, 0, &judgment));
    }

    #[test]
    fn synthetic_receptor_step_survives_until_lift() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        let column = 0usize;

        trigger_receptor_step_pulse(&mut state, column);
        let started_press = state.receptor_glow_press_timers[column];
        if started_press <= f32::EPSILON {
            assert!(state.receptor_bop_timers[column] > 0.0);
            return;
        }
        tick_visual_effects(&mut state, 0.01);

        if started_press > 0.01 {
            assert!(state.receptor_glow_press_timers[column] > 0.0);
            assert!(state.receptor_glow_press_timers[column] < started_press);
        }
        tick_visual_effects(&mut state, started_press.max(0.01));

        assert_eq!(state.receptor_glow_press_timers[column], 0.0);
        assert!(state.receptor_glow_timers[column] > 0.0);
    }

    #[test]
    fn completed_row_hidden_note_indices_wait_for_full_jump() {
        let row_index = 48usize;
        let notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            test_note(1, row_index, NoteType::Tap),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        assert!(completed_row_flash_note_indices_and_judgment(&notes, &row_entry).is_none());
    }

    #[test]
    fn completed_row_hidden_note_indices_hide_whole_jump_on_great_or_better() {
        let row_index = 48usize;
        let notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            note_with_judgment(1, row_index, NoteType::Tap, JudgeGrade::Excellent, 8.0),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        let (hide_indices, hide_count, final_judgment) =
            completed_row_flash_note_indices_and_judgment(&notes, &row_entry)
                .expect("completed jump should produce a row-final grade");

        assert!(row_final_grade_hides_note(final_judgment.grade));
        assert_eq!(hide_count, 2);
        assert_eq!(hide_indices[0], 0);
        assert_eq!(hide_indices[1], 1);
    }

    #[test]
    fn completed_row_hidden_note_indices_keep_w4_w5_rows_visible() {
        let row_index = 48usize;
        let notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            note_with_judgment(1, row_index, NoteType::Tap, JudgeGrade::Decent, 96.0),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        let (_, _, final_judgment) =
            completed_row_flash_note_indices_and_judgment(&notes, &row_entry)
                .expect("completed jump should produce a row-final grade");
        assert!(!row_final_grade_hides_note(final_judgment.grade));
    }

    #[test]
    fn completed_row_flash_note_indices_use_final_jump_grade_for_all_lanes() {
        let row_index = 2002usize;
        let notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Decent, -96.0),
            note_with_judgment(1, row_index, NoteType::Tap, JudgeGrade::Great, 42.0),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        let (flash_indices, flash_count, flash_judgment) =
            completed_row_flash_note_indices_and_judgment(&notes, &row_entry)
                .expect("completed jump should flash every lane with the final row grade");

        assert_eq!(flash_judgment.grade, JudgeGrade::Great);
        assert_eq!(flash_count, 2);
        assert_eq!(flash_indices[0], 0);
        assert_eq!(flash_indices[1], 1);

        assert!(row_final_grade_hides_note(flash_judgment.grade));
    }

    #[test]
    fn edge_judge_indices_only_use_the_triggering_note_on_jumps() {
        let (judge_indices, judge_count) = collect_edge_judge_indices(2, 1)
            .expect("jump rows should still judge the triggering note");

        assert_eq!(judge_count, 1);
        assert_eq!(judge_indices[0], 1);
        assert_eq!(judge_indices[1], usize::MAX);
    }

    #[test]
    fn tick_status_matches_mode() {
        assert_eq!(tick_mode_status_line(TickMode::Off), None);
        assert_eq!(tick_mode_status_line(TickMode::Assist), Some("Assist Tick"));
        assert_eq!(tick_mode_status_line(TickMode::Hit), Some("Hit Tick"));
    }

    #[test]
    fn song_clock_reconstructs_past_edge_time() {
        let base = Instant::now();
        let snapshot = SongClockSnapshot {
            song_time_ns: song_time_ns_from_seconds(120.0),
            seconds_per_second: 1.5,
            mapped_audio: true,
            valid_at: base + Duration::from_millis(24),
            valid_at_host_nanos: 0,
            timing_diag_enabled: false,
            timing_diag_callback_gap_ns: 0,
        };
        let edge_time = song_time_ns_to_seconds(music_time_ns_from_song_clock(snapshot, base, 0));
        assert!((edge_time - 119.964).abs() < 0.000_5);
    }

    #[test]
    fn song_clock_handles_future_edge_time() {
        let base = Instant::now();
        let snapshot = SongClockSnapshot {
            song_time_ns: song_time_ns_from_seconds(64.0),
            seconds_per_second: 2.0,
            mapped_audio: true,
            valid_at: base,
            valid_at_host_nanos: 0,
            timing_diag_enabled: false,
            timing_diag_callback_gap_ns: 0,
        };
        let edge_time = song_time_ns_to_seconds(music_time_ns_from_song_clock(
            snapshot,
            base + Duration::from_millis(5),
            0,
        ));
        assert!((edge_time - 64.01).abs() < 0.000_5);
    }

    #[test]
    fn song_clock_prefers_host_clock_when_available() {
        let snapshot = SongClockSnapshot {
            song_time_ns: song_time_ns_from_seconds(32.0),
            seconds_per_second: 1.0,
            mapped_audio: true,
            valid_at: Instant::now(),
            valid_at_host_nanos: 2_000_000_000,
            timing_diag_enabled: false,
            timing_diag_callback_gap_ns: 0,
        };
        let edge_time = song_time_ns_to_seconds(music_time_ns_from_song_clock(
            snapshot,
            Instant::now(),
            1_997_000_000,
        ));
        assert!((edge_time - 31.997).abs() < 0.000_5);
    }

    #[test]
    fn display_clock_snaps_on_first_update() {
        let mut display_clock = FrameStableDisplayClock::new(song_time_ns_from_seconds(10.0));
        let mut diag = DisplayClockDiagRing::new();
        let display_time = song_time_ns_to_seconds(frame_stable_display_music_time_ns(
            &mut display_clock,
            &mut diag,
            1,
            song_time_ns_from_seconds(12.5),
            0.001,
            1.0,
            true,
        ));
        assert!((display_time - 12.5).abs() < 0.000_5);
    }

    #[test]
    fn display_clock_advances_smoothly_toward_target() {
        let mut display_clock = FrameStableDisplayClock::new(song_time_ns_from_seconds(100.0));
        let mut diag = DisplayClockDiagRing::new();
        let display_time = song_time_ns_to_seconds(frame_stable_display_music_time_ns(
            &mut display_clock,
            &mut diag,
            1,
            song_time_ns_from_seconds(100.004),
            0.001,
            1.0,
            false,
        ));
        assert!(display_time > 100.0);
        assert!(display_time < 100.004);
    }

    #[test]
    fn display_clock_snaps_back_when_far_from_target() {
        let mut display_clock = FrameStableDisplayClock::new(song_time_ns_from_seconds(100.0));
        let mut diag = DisplayClockDiagRing::new();
        let display_time = song_time_ns_to_seconds(frame_stable_display_music_time_ns(
            &mut display_clock,
            &mut diag,
            1,
            song_time_ns_from_seconds(100.250),
            0.001,
            1.0,
            false,
        ));
        assert!((display_time - 100.250).abs() < 0.000_5);
    }

    #[test]
    fn visible_notefield_time_uses_simulation_clock_plus_delay() {
        let music_time_ns = song_time_ns_from_seconds(100.0);
        let delay = 0.010;
        let visible = song_time_ns_to_seconds(visible_notefield_time_ns(music_time_ns, delay));

        assert!((visible - 99.990).abs() < 0.000_5);
    }

    #[test]
    fn assist_clap_rows_include_lifts() {
        let notes = vec![Note {
            beat: 1.0,
            quantization_idx: 0,
            column: 0,
            note_type: NoteType::Lift,
            row_index: 48,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }];
        assert_eq!(build_assist_clap_rows(&notes, (0, 1)), vec![48]);
    }

    #[test]
    fn assist_lookahead_horizon_adds_margin_and_scales_by_slope() {
        // At unit slope the horizon is just delay + margin.
        let h = assist_lookahead_music_horizon_seconds(0.020, 1.0);
        assert!((h - 0.070).abs() <= 1e-6, "h={h}");

        // A 2x music rate (slope = 2.0) doubles the music-second horizon.
        let h2 = assist_lookahead_music_horizon_seconds(0.020, 2.0);
        assert!((h2 - 0.140).abs() <= 1e-6, "h2={h2}");
    }

    #[test]
    fn assist_lookahead_horizon_guards_bad_slope_and_negative_delay() {
        // Non-positive / non-finite slope falls back to 1.0.
        let margin = super::ASSIST_TICK_LOOKAHEAD_MARGIN_SECONDS;
        assert!((assist_lookahead_music_horizon_seconds(0.0, 0.0) - margin).abs() <= 1e-6);
        assert!((assist_lookahead_music_horizon_seconds(0.0, -1.0) - margin).abs() <= 1e-6);
        assert!((assist_lookahead_music_horizon_seconds(0.0, f32::NAN) - margin).abs() <= 1e-6);

        // Horizon never goes negative even if delay somehow reads negative.
        let h = assist_lookahead_music_horizon_seconds(-1.0, 1.0);
        assert!(h >= 0.0, "h={h}");
    }

    #[test]
    fn row_grids_group_sorted_rows_and_ignore_out_of_range_columns() {
        let notes = vec![
            test_note(2, 48, NoteType::Tap),
            test_note(0, 48, NoteType::Lift),
            test_note(3, 96, NoteType::Tap),
            test_note(5, 96, NoteType::Tap),
        ];

        let rows = build_row_grids(&notes, (0, notes.len()), 0, 4);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].row_index, 48);
        assert_eq!(rows[0].note_indices[0], 1);
        assert_eq!(rows[0].note_indices[2], 0);
        assert_eq!(rows[1].row_index, 96);
        assert_eq!(rows[1].note_indices[3], 2);
        assert_eq!(rows[1].note_indices[0], usize::MAX);
    }

    #[test]
    fn max_simultaneous_counts_active_holds_before_row_taps() {
        let mut notes = vec![
            test_hold(0, 0, 96),
            test_note(1, 48, NoteType::Tap),
            test_note(2, 48, NoteType::Tap),
        ];

        enforce_max_simultaneous_notes(&mut notes, 2, 0, 4);

        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].column, 0);
        assert_eq!(notes[0].row_index, 0);
        assert_eq!(notes[1].column, 2);
        assert_eq!(notes[1].row_index, 48);
    }

    #[test]
    fn scored_hold_totals_with_carry_include_prior_let_go() {
        assert_eq!(judgment::scored_hold_totals_with_carry(3, 2, 4, 5), (7, 14));
    }

    #[test]
    fn immediate_hold_let_go_does_not_break_combo_by_default() {
        assert!(!COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO);
    }

    #[test]
    fn late_note_resolution_window_matches_itg_max_step_distance_window() {
        let timing_profile = TimingProfile::default_itg_with_fa_plus();

        assert!(
            (song_time_ns_to_seconds(late_note_resolution_window_ns(&timing_profile, 1.0))
                - 0.3515)
                .abs()
                <= 1e-6
        );
    }

    #[test]
    fn max_step_distance_scales_with_music_rate() {
        let timing_profile = TimingProfile::default_itg_with_fa_plus();

        assert!(
            (song_time_ns_to_seconds(max_step_distance_ns(&timing_profile, 1.5)) - 0.52725).abs()
                <= 1e-6
        );
    }

    #[test]
    fn music_end_time_waits_for_audio_tail() {
        let notes = vec![test_note(0, ROWS_PER_BEAT as usize * 2, NoteType::Tap)];
        let note_time_cache_ns = vec![song_time_ns_from_seconds(2.0)];
        let hold_end_time_cache_ns = vec![None];
        let audio_end_time_ns = song_time_ns_from_seconds(10.0);

        let (notes_end_time_ns, music_end_time_ns) = compute_end_times_ns(
            &notes,
            &note_time_cache_ns,
            &hold_end_time_cache_ns,
            1.0,
            audio_end_time_ns,
        );

        assert!(notes_end_time_ns < audio_end_time_ns);
        assert_eq!(music_end_time_ns, audio_end_time_ns);
    }

    #[test]
    fn mine_hits_preserve_combo_when_miss_combo_metric_is_disabled() {
        let mut player = super::init_player_runtime();
        player.combo = 50;
        player.miss_combo = 3;
        player.full_combo_grade = Some(JudgeGrade::Great);
        player.current_combo_grade = Some(JudgeGrade::Great);

        super::apply_mine_hit_combo_state(&mut player);

        assert_eq!(player.combo, 50);
        assert_eq!(player.miss_combo, 3);
        assert_eq!(player.full_combo_grade, Some(JudgeGrade::Great));
        assert_eq!(player.current_combo_grade, Some(JudgeGrade::Great));
        assert!(!player.first_fc_attempt_broken);
    }

    #[test]
    fn hold_success_preserves_existing_miss_combo() {
        let mut player = super::init_player_runtime();
        player.miss_combo = 4;

        super::apply_hold_success_combo_state(&mut player);

        assert_eq!(player.miss_combo, 4);
    }

    #[test]
    fn successful_rows_clear_miss_combo_and_extend_combo() {
        let mut player = super::init_player_runtime();
        player.combo = 20;
        player.miss_combo = 4;

        super::apply_row_combo_state(&mut player, JudgeGrade::Great, 2, 1);

        assert_eq!(player.combo, 22);
        assert_eq!(player.miss_combo, 0);
        assert_eq!(player.full_combo_grade, Some(JudgeGrade::Great));
        assert_eq!(player.current_combo_grade, Some(JudgeGrade::Great));
    }

    #[test]
    fn course_combo_carry_restores_combo_color_state_when_combo_carries() {
        let mut player = super::init_player_runtime();
        let carry = super::CourseDisplayCarry {
            full_combo_grade: Some(JudgeGrade::Excellent),
            current_combo_grade: Some(JudgeGrade::Excellent),
            current_combo_window_counts: deadsync_rules::timing::WindowCounts {
                w0: 7,
                ..Default::default()
            },
            ..Default::default()
        };

        super::apply_course_combo_carry(&mut player, true, false, 37, Some(carry));

        assert_eq!(player.combo, 37);
        assert_eq!(player.full_combo_grade, Some(JudgeGrade::Excellent));
        assert_eq!(player.current_combo_grade, Some(JudgeGrade::Excellent));
        assert_eq!(player.current_combo_window_counts.w0, 7);
        assert!(!player.first_fc_attempt_broken);
    }

    #[test]
    fn course_life_carry_restores_lifemeter_between_songs() {
        let mut player = super::init_player_runtime();
        let carry = super::CourseDisplayCarry {
            life: 0.32,
            ..Default::default()
        };

        super::apply_course_life_carry(&mut player, Some(carry));

        assert!((player.life - 0.32).abs() <= f32::EPSILON);
    }

    #[test]
    fn course_submit_life_starts_at_normal_song_life() {
        let mut player = super::init_player_runtime();
        super::init_course_submit_life(&mut player);
        let carry = super::CourseDisplayCarry {
            life: 1.0,
            ..Default::default()
        };

        super::apply_course_life_carry(&mut player, Some(carry));

        let submit_life = player.course_submit_life.expect("course submit life");
        assert!((player.life - 1.0).abs() <= f32::EPSILON);
        assert!((submit_life.life - 0.5).abs() <= f32::EPSILON);
        assert!(!submit_life.is_failing);
    }

    #[test]
    fn course_submit_life_can_fail_while_course_life_survives() {
        let mut player = super::init_player_runtime();
        player.life = 1.0;
        super::init_course_submit_life(&mut player);

        super::apply_life_change(&mut player, 12.0, -0.6);

        let submit_life = player.course_submit_life.expect("course submit life");
        assert!((player.life - 0.4).abs() <= 0.000_001);
        assert!(!player.is_failing);
        assert_eq!(player.fail_time, None);
        assert_eq!(submit_life.life, 0.0);
        assert!(submit_life.is_failing);
        assert_eq!(submit_life.fail_time, Some(12.0));
    }

    #[test]
    fn course_display_carry_captures_current_life() {
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        state.players[0].life = 0.32;

        let carry = super::course_display_carry_from_state(&state);

        assert!((carry[0].life - 0.32).abs() <= f32::EPSILON);
    }

    #[test]
    fn course_combo_carry_keeps_prior_break_from_coloring_full_combo() {
        let mut player = super::init_player_runtime();
        let carry = super::CourseDisplayCarry {
            current_combo_grade: Some(JudgeGrade::Fantastic),
            current_combo_window_counts: deadsync_rules::timing::WindowCounts {
                w1: 1,
                ..Default::default()
            },
            first_fc_attempt_broken: true,
            ..Default::default()
        };

        super::apply_course_combo_carry(&mut player, true, false, 12, Some(carry));

        assert_eq!(player.combo, 12);
        assert!(player.full_combo_grade.is_none());
        assert_eq!(player.current_combo_grade, Some(JudgeGrade::Fantastic));
        assert_eq!(player.current_combo_window_counts.w1, 1);
        assert!(player.first_fc_attempt_broken);

        super::apply_row_combo_state(&mut player, JudgeGrade::Fantastic, 1, 0);

        assert!(player.full_combo_grade.is_none());
        assert_eq!(player.current_combo_grade, Some(JudgeGrade::Fantastic));
    }

    #[test]
    fn course_combo_carry_without_combo_disables_full_combo_reseed() {
        let mut player = super::init_player_runtime();
        let carry = super::CourseDisplayCarry {
            full_combo_grade: Some(JudgeGrade::Fantastic),
            current_combo_grade: Some(JudgeGrade::Fantastic),
            current_combo_window_counts: deadsync_rules::timing::WindowCounts {
                w0: 9,
                ..Default::default()
            },
            ..Default::default()
        };

        super::apply_course_combo_carry(&mut player, true, false, 0, Some(carry));

        assert_eq!(player.combo, 0);
        assert!(player.full_combo_grade.is_none());
        assert!(player.current_combo_grade.is_none());
        assert_eq!(player.current_combo_window_counts.w0, 0);
        assert_eq!(player.current_combo_window_counts.w1, 0);
        assert!(player.first_fc_attempt_broken);

        super::apply_row_combo_state(&mut player, JudgeGrade::Fantastic, 1, 0);

        assert!(player.full_combo_grade.is_none());
        assert_eq!(player.current_combo_grade, Some(JudgeGrade::Fantastic));
    }

    #[test]
    fn bad_first_row_breaks_full_combo_attempt() {
        let mut player = super::init_player_runtime();

        super::apply_row_combo_state(&mut player, JudgeGrade::Decent, 1, 1);

        assert_eq!(player.combo, 0);
        assert!(player.full_combo_grade.is_none());
        assert!(player.current_combo_grade.is_none());
        assert!(player.first_fc_attempt_broken);

        super::apply_row_combo_state(&mut player, JudgeGrade::Fantastic, 1, 0);

        assert_eq!(player.combo, 1);
        assert!(player.full_combo_grade.is_none());
        assert_eq!(player.current_combo_grade, Some(JudgeGrade::Fantastic));
    }

    #[test]
    fn decent_rows_break_combo_without_clearing_existing_miss_combo() {
        let mut player = super::init_player_runtime();
        player.combo = 20;
        player.miss_combo = 4;
        player.full_combo_grade = Some(JudgeGrade::Great);
        player.current_combo_grade = Some(JudgeGrade::Great);

        super::apply_row_combo_state(&mut player, JudgeGrade::Decent, 2, 1);

        assert_eq!(player.combo, 0);
        assert_eq!(player.miss_combo, 4);
        assert!(player.full_combo_grade.is_none());
        assert!(player.current_combo_grade.is_none());
        assert!(player.first_fc_attempt_broken);
    }

    #[test]
    fn miss_rows_increment_existing_miss_combo() {
        let mut player = super::init_player_runtime();
        player.combo = 20;
        player.miss_combo = 4;

        super::apply_row_combo_state(&mut player, JudgeGrade::Miss, 2, 1);

        assert_eq!(player.combo, 0);
        assert_eq!(player.miss_combo, 5);
    }

    #[test]
    fn zero_life_events_burn_down_regen_lock() {
        let mut player = super::init_player_runtime();
        player.life = 0.5;
        player.combo_after_miss = super::REGEN_COMBO_AFTER_MISS;

        for _ in 0..super::REGEN_COMBO_AFTER_MISS {
            super::apply_life_change(&mut player, 0.0, super::LIFE_DECENT);
        }

        assert_eq!(player.combo_after_miss, 0);
        assert!((player.life - 0.5).abs() <= 1e-6);

        super::apply_life_change(&mut player, 0.0, super::LIFE_GREAT);

        assert!((player.life - 0.504).abs() <= 1e-6);
    }

    #[test]
    fn repeated_negative_life_events_stack_regen_lock_to_maximum() {
        let mut player = super::init_player_runtime();
        player.combo_after_miss = super::REGEN_COMBO_AFTER_MISS;

        super::apply_life_change(&mut player, 0.0, super::LIFE_HIT_MINE);
        assert_eq!(player.combo_after_miss, super::MAX_REGEN_COMBO_AFTER_MISS);

        super::apply_life_change(&mut player, 0.0, super::LIFE_HIT_MINE);
        assert_eq!(player.combo_after_miss, super::MAX_REGEN_COMBO_AFTER_MISS);
    }

    #[test]
    fn hot_life_penalty_clamps_negative_events_to_ten_percent() {
        let mut player = super::init_player_runtime();
        player.life = 1.0;

        super::apply_life_change(&mut player, 0.0, super::LIFE_HIT_MINE);

        assert!((player.life - 0.9).abs() <= 1e-6);
    }

    #[test]
    fn final_bad_rescore_visuals_are_suppressed_only_for_bad_rows() {
        assert!(suppress_final_bad_rescore_visual(true, JudgeGrade::Decent));
        assert!(suppress_final_bad_rescore_visual(true, JudgeGrade::WayOff));
        assert!(!suppress_final_bad_rescore_visual(true, JudgeGrade::Great));
        assert!(!suppress_final_bad_rescore_visual(
            false,
            JudgeGrade::Decent
        ));
    }

    #[test]
    fn effective_ex_score_inputs_use_live_values_before_fail() {
        let player = super::init_player_runtime();
        let live = super::ExScoreInputs {
            counts: deadsync_rules::timing::WindowCounts {
                w1: 3,
                ..deadsync_rules::timing::WindowCounts::default()
            },
            counts_10ms: deadsync_rules::timing::WindowCounts {
                w0: 2,
                ..deadsync_rules::timing::WindowCounts::default()
            },
            holds_held_for_score: 4,
            holds_let_go_for_score: 1,
            rolls_held_for_score: 2,
            rolls_let_go_for_score: 1,
            mines_hit_for_score: 5,
        };

        let selected = super::effective_ex_score_inputs(&player, live);

        assert_eq!(selected.counts.w1, 3);
        assert_eq!(selected.counts_10ms.w0, 2);
        assert_eq!(selected.holds_held_for_score, 4);
        assert_eq!(selected.mines_hit_for_score, 5);
    }

    #[test]
    fn effective_ex_score_inputs_freeze_on_fail_snapshot() {
        let mut player = super::init_player_runtime();
        player.failed_ex_score_inputs = Some(super::ExScoreInputs {
            counts: deadsync_rules::timing::WindowCounts {
                w2: 7,
                ..deadsync_rules::timing::WindowCounts::default()
            },
            counts_10ms: deadsync_rules::timing::WindowCounts {
                w0: 1,
                ..deadsync_rules::timing::WindowCounts::default()
            },
            holds_held_for_score: 6,
            holds_let_go_for_score: 2,
            rolls_held_for_score: 4,
            rolls_let_go_for_score: 1,
            mines_hit_for_score: 3,
        });
        let live = super::ExScoreInputs {
            counts: deadsync_rules::timing::WindowCounts {
                w2: 9,
                ..deadsync_rules::timing::WindowCounts::default()
            },
            counts_10ms: deadsync_rules::timing::WindowCounts {
                w0: 5,
                ..deadsync_rules::timing::WindowCounts::default()
            },
            holds_held_for_score: 10,
            holds_let_go_for_score: 4,
            rolls_held_for_score: 8,
            rolls_let_go_for_score: 2,
            mines_hit_for_score: 7,
        };

        let selected = super::effective_ex_score_inputs(&player, live);

        assert_eq!(selected.counts.w2, 7);
        assert_eq!(selected.counts_10ms.w0, 1);
        assert_eq!(selected.holds_held_for_score, 6);
        assert_eq!(selected.holds_let_go_for_score, 2);
        assert_eq!(selected.rolls_held_for_score, 4);
        assert_eq!(selected.mines_hit_for_score, 3);
    }

    #[test]
    fn autoplay_rows_do_not_record_ex_counts() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        let row_index = 48usize;
        state.notes = vec![test_note(0, row_index, NoteType::Tap)];
        state.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.row_entries = vec![test_row_entry_with_times(
            &state.notes,
            &state.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.row_entry_ranges = [(0, 1), (0, 0)];
        state.note_row_entry_indices = vec![0];
        state.autoplay_enabled = true;

        set_final_note_result(
            &mut state,
            0,
            Judgment {
                time_error_ms: 0.0,
                time_error_music_ns: 0,
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W0),
                miss_because_held: false,
            },
        );
        finalize_row_judgment(&mut state, 0, row_index, 0, false);

        assert_eq!(state.live_window_counts[0].w0, 0);
        assert_eq!(super::display_ex_score_percent(&state, 0), 0.0);
        assert_eq!(super::display_itg_score_percent(&state, 0), 0.0);
        assert!(state.players[0].last_judgment.is_some());
    }

    #[test]
    fn missed_holds_and_rolls_are_not_scored_for_dance_or_pump() {
        assert!(!judgment::score_missed_holds_and_rolls("dance-single"));
        assert!(!judgment::score_missed_holds_and_rolls("pump-single"));
        assert!(!judgment::score_missed_holds_and_rolls(" Dance-single "));
        assert!(judgment::score_missed_holds_and_rolls("kb7-single"));
    }

    #[test]
    fn recompute_totals_count_three_note_row_as_hand() {
        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_note(1, 48, NoteType::Tap),
            test_note(2, 48, NoteType::Tap),
        ];

        let totals = recompute_player_totals(&notes, (0, notes.len()));

        assert_eq!(totals.steps, 1);
        assert_eq!(totals.hands, 1);
    }

    #[test]
    fn recompute_totals_count_hold_assisted_hand() {
        let notes = vec![
            test_hold(0, 0, 96),
            test_note(1, 48, NoteType::Tap),
            test_note(2, 48, NoteType::Tap),
        ];

        let totals = recompute_player_totals(&notes, (0, notes.len()));

        assert_eq!(totals.holds, 1);
        assert_eq!(totals.steps, 2);
        assert_eq!(totals.hands, 1);
    }

    #[test]
    fn max_grade_points_keeps_removed_notes_in_denominator() {
        let notes = vec![test_note(0, 48, NoteType::Tap)];

        let points = judgment::max_grade_points(&notes, (0, notes.len()), 0, 0, 15);

        assert_eq!(points, 15);
    }

    #[test]
    fn max_grade_points_counts_inserted_notes() {
        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_note(1, 96, NoteType::Tap),
        ];

        let points = judgment::max_grade_points(&notes, (0, notes.len()), 0, 0, 5);

        assert_eq!(points, 10);
    }

    #[test]
    fn score_valid_rejects_nohands_when_chart_has_hands() {
        let mut profile = profile_data::Profile::default();
        profile.remove_active_mask =
            profile_data::RemoveMask::from_bits_truncate(super::REMOVE_MASK_BIT_NO_HANDS);
        let chart = test_chart(
            ArrowStats {
                hands: 4,
                ..ArrowStats::default()
            },
            TimingSegments::default(),
            None,
        );

        assert!(
            !score_invalid_reason_lines_for_chart(&chart, &profile, profile.scroll_speed, 1.0)
                .is_empty()
        );
    }

    #[test]
    fn score_valid_keeps_turn_options_rankable() {
        let mut profile = profile_data::Profile::default();
        profile.turn_option = profile_data::TurnOption::Mirror;
        let chart = test_chart(ArrowStats::default(), TimingSegments::default(), None);

        assert!(
            score_invalid_reason_lines_for_chart(&chart, &profile, profile.scroll_speed, 1.0)
                .is_empty()
        );
    }

    #[test]
    fn score_valid_keeps_cmod_rankable_on_timing_changes() {
        let mut profile = profile_data::Profile::default();
        profile.scroll_speed = ScrollSpeedSetting::CMod(600.0);
        let chart = test_chart(
            ArrowStats::default(),
            TimingSegments {
                bpms: vec![(0.0, 120.0), (32.0, 128.5)],
                ..TimingSegments::default()
            },
            None,
        );

        assert!(
            score_invalid_reason_lines_for_chart(&chart, &profile, profile.scroll_speed, 1.0)
                .is_empty()
        );
    }

    #[test]
    fn score_valid_rejects_disabled_chart_attacks() {
        let mut profile = profile_data::Profile::default();
        profile.attack_mode = profile_data::AttackMode::Off;
        let chart = test_chart(
            ArrowStats::default(),
            TimingSegments::default(),
            Some("TIME=1.0:LEN=2.0:MODS=mirror"),
        );

        assert!(
            !score_invalid_reason_lines_for_chart(&chart, &profile, profile.scroll_speed, 1.0)
                .is_empty()
        );
    }

    #[test]
    fn cmod_stop_lane_window_uses_time_not_frozen_beat() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 185.0)],
                stops: vec![StopSegment {
                    beat: 32.0,
                    duration: 0.973,
                }],
                ..TimingSegments::default()
            },
            &[],
        );
        let stop_beat = 32.0;
        let note_time = timing.get_time_for_beat(stop_beat);
        let lookahead_time = note_time + 0.5;
        let lookahead_beat = timing.get_beat_for_time(lookahead_time);
        let note_times_ns = [song_time_ns_from_seconds(note_time)];
        let note_indices = [0usize];

        assert!((lookahead_beat - stop_beat).abs() < 0.000_5);
        assert_eq!(
            lane_note_window_bounds_ns(
                &note_indices,
                &note_times_ns,
                0,
                song_time_ns_from_seconds(lookahead_time),
            ),
            (0, 1)
        );
        assert!(!(stop_beat < lookahead_beat));
    }

    #[test]
    fn attack_mod_parser_keeps_scroll_override_and_partial_levels() {
        let mods = parse_attack_mods("0.5x,20% flip,50% hidden,30% blink,25% mini");
        assert_eq!(mods.scroll_speed, Some(ScrollSpeedSetting::XMod(0.5)));
        assert_eq!(mods.visual.flip, Some(0.2));
        assert_eq!(mods.appearance.hidden, Some(0.5));
        assert_eq!(mods.appearance.blink, Some(0.3));
        assert_eq!(mods.mini_percent, Some(25.0));
    }

    #[test]
    fn attack_mod_parser_accepts_stepmania_speed_strings() {
        let mods = parse_attack_mods("C600,150% drunk,200% expand");
        assert_eq!(mods.scroll_speed, Some(ScrollSpeedSetting::CMod(600.0)));
        assert_eq!(mods.visual.drunk, Some(1.5));
        assert_eq!(mods.accel.expand, Some(2.0));
    }

    #[test]
    fn attack_mod_parser_clearall_discards_prior_mods_and_no_prefix_zeroes_levels() {
        let mods = parse_attack_mods("drunk,clearall,30% blink,no hidden");
        assert!(mods.clear_all);
        assert_eq!(mods.visual.drunk, None);
        assert_eq!(mods.appearance.blink, Some(0.3));
        assert_eq!(mods.appearance.hidden, Some(0.0));
    }

    #[test]
    fn attack_mod_parser_handles_star_prefix_offsets() {
        let mods =
            parse_attack_mods("*1000 sudden,*1000 -125% suddenoffset,*2.4 150% hiddenoffset");
        assert_eq!(mods.appearance.sudden, Some(1.0));
        assert_eq!(mods.appearance.sudden_offset, Some(-1.25));
        assert_eq!(mods.appearance.hidden_offset, Some(1.5));
        assert_eq!(mods.appearance_speed.sudden, Some(1000.0));
        assert_eq!(mods.appearance_speed.sudden_offset, Some(1000.0));
        assert_eq!(mods.appearance_speed.hidden_offset, Some(2.4));
    }

    #[test]
    fn chart_attack_sudden_offset_approaches_instead_of_snapping() {
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        state.attack_mask_windows[0] = build_attack_mask_windows_for_player(
            Some(
                "TIME=0.000:LEN=3.000:MODS=*1000 sudden,*1000 -125% suddenoffset\
                 :TIME=0.083:LEN=3.000:MODS=*2.4 150% suddenoffset",
            ),
            profile_data::AttackMode::On,
            0,
            0x1234,
            10.0,
        );

        state.current_music_time_visible[0] = 0.01;
        refresh_active_attack_masks(&mut state, 0.01);
        let start = effective_appearance_effects_for_player(&state, 0);
        assert!((start.sudden - 1.0).abs() <= 1e-6);
        assert!((start.sudden_offset + 1.25).abs() <= 1e-6);

        state.current_music_time_visible[0] = 0.10;
        refresh_active_attack_masks(&mut state, 0.09);
        let mid = effective_appearance_effects_for_player(&state, 0);
        assert!(mid.sudden_offset > -1.25);
        assert!(mid.sudden_offset < 1.5);

        state.current_music_time_visible[0] = 1.10;
        refresh_active_attack_masks(&mut state, 1.0);
        let late = effective_appearance_effects_for_player(&state, 0);
        assert!(late.sudden_offset > mid.sudden_offset);
        assert!(late.sudden_offset < 1.5);
    }

    #[test]
    fn chart_attack_runtime_mods_stop_after_len() {
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        state.attack_mask_windows[0] = build_attack_mask_windows_for_player(
            Some("TIME=0.000:LEN=1.000:MODS=50% drunk"),
            profile_data::AttackMode::On,
            0,
            0x1234,
            10.0,
        );

        state.current_music_time_visible[0] = 2.0;
        refresh_active_attack_masks(&mut state, 0.0);

        let visual = effective_visual_effects_for_player(&state, 0);
        assert!(visual.drunk.abs() <= 0.000_1);
    }

    #[test]
    fn outro_attack_clear_phases_out_song_lua_visual_mods() {
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        state.active_attack_visual[0].confusion_offset = Some(-12.56);
        state.active_attack_visual[0].tipsy = Some(0.75);
        state.active_attack_visibility[0].dark = Some(1.0);

        begin_outro_attack_clear(&mut state);
        refresh_active_attack_masks(&mut state, 0.5);

        let visual = effective_visual_effects_for_player(&state, 0);
        let visibility = effective_visibility_effects_for_player(&state, 0);
        assert!(visual.confusion_offset > -12.56);
        assert!(visual.confusion_offset < -12.0);
        assert!(visual.tipsy > 0.0);
        assert!(visual.tipsy < 0.75);
        assert!((visibility.dark - 1.0).abs() <= 0.0001);

        refresh_active_attack_masks(&mut state, 20.0);

        let cleared = effective_visual_effects_for_player(&state, 0);
        let visibility = effective_visibility_effects_for_player(&state, 0);
        assert!(cleared.confusion_offset.abs() <= 0.0001);
        assert!(cleared.tipsy.abs() <= 0.0001);
        assert!(state.active_attack_visual[0].confusion_offset.is_none());
        assert!(state.active_attack_visual[0].tipsy.is_none());
        assert!((visibility.dark - 1.0).abs() <= 0.0001);
    }

    #[test]
    fn outro_attack_clear_keeps_player_rotationz_eases_alive() {
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            eases: vec![crate::game::parsing::song_lua::SongLuaEaseWindow {
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 1.0,
                limit: 1.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                from: 0.0,
                to: 5.0,
                target: crate::game::parsing::song_lua::SongLuaEaseTarget::PlayerRotationZ,
                easing: Some("linear".to_string()),
                player: Some(1),
                sustain: Some(4.0),
                opt1: None,
                opt2: None,
            }],
            ..Default::default()
        };
        let (windows, unsupported) =
            super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);
        assert_eq!(unsupported, 0);
        state.song_lua_ease_windows[0] = windows;
        state.current_music_time_visible[0] = 2.5;

        begin_outro_attack_clear(&mut state);
        refresh_active_attack_masks(&mut state, 0.0);

        assert!((state.song_lua_player_rotation_z[0] - 5.0).abs() <= 0.0001);
    }

    #[test]
    fn attack_mod_parser_accepts_scroll_and_perspective_overrides() {
        let mods = parse_attack_mods("30% reverse,centered,50% incoming,dark,50% blind,75% cover");
        assert_eq!(mods.scroll.reverse, Some(0.3));
        assert_eq!(mods.scroll.centered, Some(1.0));
        assert_eq!(mods.perspective.tilt, Some(-0.5));
        assert_eq!(mods.perspective.skew, Some(0.5));
        assert_eq!(mods.visibility.dark, Some(1.0));
        assert_eq!(mods.visibility.blind, Some(0.5));
        assert_eq!(mods.visibility.cover, Some(0.75));
    }

    #[test]
    fn song_lua_mod_parser_accepts_star_prefix_and_aliases() {
        let mods = parse_song_lua_runtime_mods(
            "*9999 25 invert,*9999 no hidden,*9999 3x,*9999 -25 tiny,*9999 25 mini,*9999 50 incoming,*9999 15 bumpy3,*9999 250 tiny2,*9999 -125 bumpyperiod,*9999 100 pulseouter",
        );
        assert_eq!(mods.visual.invert, Some(0.25));
        assert_eq!(mods.appearance.hidden, Some(0.0));
        assert_eq!(mods.scroll_speed, Some(ScrollSpeedSetting::XMod(3.0)));
        assert_eq!(mods.visual.tiny, Some(-0.25));
        assert_eq!(mods.mini_percent, Some(25.0));
        assert_eq!(mods.perspective.tilt, Some(-0.5));
        assert_eq!(mods.perspective.skew, Some(0.5));
        assert_eq!(mods.visual.bumpy, None);
        assert_eq!(mods.visual.bumpy_cols[2], Some(0.15));
        assert_eq!(mods.visual.tiny_cols[1], Some(2.5));
        assert_eq!(mods.visual.bumpy_period, Some(-1.25));
        assert_eq!(mods.visual.pulse_outer, Some(1.0));
    }

    #[test]
    fn song_lua_mod_parser_accepts_itgmania_column_moves() {
        let mods = parse_song_lua_runtime_mods(
            "*10000 -80 movey1,*10000 40 movex2,*10000 -314 confusionoffset3,*10000 -80 tiny",
        );

        assert_eq!(mods.visual.move_y_cols[0], Some(-0.8));
        assert_eq!(mods.visual.move_x_cols[1], Some(0.4));
        assert_eq!(mods.visual.confusion_offset_cols[2], Some(-3.14));
        assert_eq!(mods.visual.tiny, Some(-0.8));
        assert_eq!(mods.mini_percent, None);
    }

    #[test]
    fn song_lua_overlay_eases_stop_after_later_message_blocks() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(8 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            overlays: vec![crate::game::parsing::song_lua::SongLuaOverlayActor {
                kind: crate::game::parsing::song_lua::SongLuaOverlayKind::Quad,
                name: None,
                parent_index: None,
                initial_state: crate::game::parsing::song_lua::SongLuaOverlayState::default(),
                message_commands: vec![
                    crate::game::parsing::song_lua::SongLuaOverlayMessageCommand {
                        message: "ResetBlack".to_string(),
                        blocks: vec![crate::game::parsing::song_lua::SongLuaOverlayCommandBlock {
                            start: 0.0,
                            duration: 0.0,
                            easing: None,
                            opt1: None,
                            opt2: None,
                            delta: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                                diffuse: Some([1.0, 1.0, 1.0, 0.0]),
                                ..Default::default()
                            },
                        }],
                    },
                ],
            }],
            overlay_eases: vec![crate::game::parsing::song_lua::SongLuaOverlayEase {
                overlay_index: 0,
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 8.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                from: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                    diffuse: Some([1.0, 1.0, 1.0, 0.0]),
                    ..Default::default()
                },
                to: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                    diffuse: Some([1.0, 1.0, 1.0, 1.0]),
                    ..Default::default()
                },
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            }],
            messages: vec![crate::game::parsing::song_lua::SongLuaMessageEvent {
                beat: 4.0,
                message: "ResetBlack".to_string(),
                persists: true,
            }],
            ..Default::default()
        };

        let windows = super::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].cutoff_second, Some(4.0));
        assert_eq!(windows[0].end_second, 8.0);
    }

    #[test]
    fn song_lua_overlay_eases_ignore_same_timestamp_setup_blocks() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(8 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            overlays: vec![crate::game::parsing::song_lua::SongLuaOverlayActor {
                kind: crate::game::parsing::song_lua::SongLuaOverlayKind::ActorFrame,
                name: None,
                parent_index: None,
                initial_state: crate::game::parsing::song_lua::SongLuaOverlayState::default(),
                message_commands: vec![
                    crate::game::parsing::song_lua::SongLuaOverlayMessageCommand {
                        message: "SetupZoom".to_string(),
                        blocks: vec![crate::game::parsing::song_lua::SongLuaOverlayCommandBlock {
                            start: 0.0,
                            duration: 0.0,
                            easing: None,
                            opt1: None,
                            opt2: None,
                            delta: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                                zoom: Some(1.5),
                                ..Default::default()
                            },
                        }],
                    },
                ],
            }],
            overlay_eases: vec![crate::game::parsing::song_lua::SongLuaOverlayEase {
                overlay_index: 0,
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 8.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                from: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                    zoom: Some(1.5),
                    ..Default::default()
                },
                to: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                    zoom: Some(1.0),
                    ..Default::default()
                },
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            }],
            messages: vec![crate::game::parsing::song_lua::SongLuaMessageEvent {
                beat: 0.0,
                message: "SetupZoom".to_string(),
                persists: true,
            }],
            ..Default::default()
        };

        let windows = super::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].cutoff_second, None);
        assert_eq!(windows[0].end_second, 8.0);
    }

    #[test]
    fn song_lua_overlay_eases_stop_persisting_after_later_reset_messages() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(8 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            overlays: vec![crate::game::parsing::song_lua::SongLuaOverlayActor {
                kind: crate::game::parsing::song_lua::SongLuaOverlayKind::Quad,
                name: None,
                parent_index: None,
                initial_state: crate::game::parsing::song_lua::SongLuaOverlayState::default(),
                message_commands: vec![
                    crate::game::parsing::song_lua::SongLuaOverlayMessageCommand {
                        message: "ResetBlack".to_string(),
                        blocks: vec![crate::game::parsing::song_lua::SongLuaOverlayCommandBlock {
                            start: 0.0,
                            duration: 0.0,
                            easing: None,
                            opt1: None,
                            opt2: None,
                            delta: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                                diffuse: Some([0.0, 0.0, 0.0, 0.0]),
                                ..Default::default()
                            },
                        }],
                    },
                ],
            }],
            overlay_eases: vec![crate::game::parsing::song_lua::SongLuaOverlayEase {
                overlay_index: 0,
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 2.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                from: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                    diffuse: Some([0.0, 0.0, 0.0, 0.0]),
                    ..Default::default()
                },
                to: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                    diffuse: Some([0.0, 0.0, 0.0, 1.0]),
                    ..Default::default()
                },
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            }],
            messages: vec![crate::game::parsing::song_lua::SongLuaMessageEvent {
                beat: 4.0,
                message: "ResetBlack".to_string(),
                persists: true,
            }],
            ..Default::default()
        };

        let windows = super::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].end_second, 2.0);
        assert_eq!(windows[0].cutoff_second, Some(4.0));
    }

    #[test]
    fn song_lua_eases_persist_until_later_override() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            eases: vec![
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 4.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::PlayerZoomY,
                    from: 1.0,
                    to: 0.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 8.0,
                    limit: 4.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::PlayerZoomY,
                    from: 0.0,
                    to: 1.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 1.0,
                    limit: 0.25,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::Mod(
                        "dark".to_string(),
                    ),
                    from: 0.0,
                    to: 100.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 4.0,
                    limit: 2.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::Mod(
                        "dark".to_string(),
                    ),
                    from: 100.0,
                    to: 0.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
            ],
            ..Default::default()
        };

        let (windows, unsupported) =
            super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);

        assert_eq!(unsupported, 0);
        assert_eq!(windows.len(), 4);
        assert_eq!(windows[0].sustain_end_second, 8.0);
        assert!(
            super::song_lua_ease_window_value(&windows[0], 6.0)
                .is_some_and(|value| (value - 0.0).abs() <= 0.000_1)
        );
        assert_eq!(windows[1].sustain_end_second, f32::MAX);
        assert!(
            super::song_lua_ease_window_value(&windows[1], 20.0)
                .is_some_and(|value| (value - 1.0).abs() <= 0.000_1)
        );
        assert_eq!(windows[2].sustain_end_second, 4.0);
        assert!(
            super::song_lua_ease_window_value(&windows[2], 3.0)
                .is_some_and(|value| (value - 1.0).abs() <= 0.000_1)
        );
        assert_eq!(windows[3].sustain_end_second, f32::MAX);
        assert!(
            super::song_lua_ease_window_value(&windows[3], 7.0)
                .is_some_and(|value| value.abs() <= 0.000_1)
        );
    }

    #[test]
    fn song_lua_constant_mod_cuts_prior_ease_tail() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            eases: vec![crate::game::parsing::song_lua::SongLuaEaseWindow {
                player: Some(1),
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 4.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                target: crate::game::parsing::song_lua::SongLuaEaseTarget::Mod("flip".to_string()),
                from: 0.0,
                to: -400.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            }],
            beat_mods: vec![crate::game::parsing::song_lua::SongLuaModWindow {
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 4.0,
                limit: 1.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                mods: "*100 0 flip".to_string(),
                player: Some(1),
            }],
            ..Default::default()
        };

        let constants =
            super::build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);
        let (windows, unsupported) =
            super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &constants);

        assert_eq!(unsupported, 0);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].sustain_end_second, 4.0);
        assert!(
            super::song_lua_ease_window_value(&windows[0], 3.5)
                .is_some_and(|value| (value + 3.5).abs() <= 0.000_1)
        );
        assert!(super::song_lua_ease_window_value(&windows[0], 4.25).is_none());
    }

    #[test]
    fn song_lua_active_reset_cuts_overlapping_ease_tail() {
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            beat_mods: vec![crate::game::parsing::song_lua::SongLuaModWindow {
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 999.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                mods: "*1 0 Stealth, *1 0 PulseOuter".to_string(),
                player: Some(1),
            }],
            eases: vec![
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 4.0,
                    limit: 2.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::Mod(
                        "Stealth".to_string(),
                    ),
                    from: 0.0,
                    to: 45.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 4.0,
                    limit: 2.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::Mod(
                        "PulseOuter".to_string(),
                    ),
                    from: 0.0,
                    to: 80.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 4.0,
                    limit: 2.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::Mod(
                        "PulsePeriod".to_string(),
                    ),
                    from: 0.0,
                    to: -80.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
            ],
            ..Default::default()
        };

        let constants =
            super::build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);
        let (windows, unsupported) =
            super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &constants);

        assert_eq!(unsupported, 0);
        let stealth = windows
            .iter()
            .find(|window| {
                matches!(
                    window.target,
                    super::attacks::SongLuaEaseMaskTarget::AppearanceStealth
                )
            })
            .unwrap();
        let pulse_outer = windows
            .iter()
            .find(|window| {
                matches!(
                    window.target,
                    super::attacks::SongLuaEaseMaskTarget::VisualPulseOuter
                )
            })
            .unwrap();
        let pulse_period = windows
            .iter()
            .find(|window| {
                matches!(
                    window.target,
                    super::attacks::SongLuaEaseMaskTarget::VisualPulsePeriod
                )
            })
            .unwrap();

        assert_eq!(stealth.sustain_end_second, 6.0);
        assert_eq!(pulse_outer.sustain_end_second, 6.0);
        assert_eq!(pulse_period.sustain_end_second, f32::MAX);
        assert!(super::song_lua_ease_window_value(stealth, 5.0).is_some());
        assert!(super::song_lua_ease_window_value(pulse_outer, 5.0).is_some());
        assert!(super::song_lua_ease_window_value(stealth, 6.25).is_none());
        assert!(super::song_lua_ease_window_value(pulse_outer, 6.25).is_none());

        state.attack_mask_windows[0] = constants;
        state.song_lua_ease_windows[0] = windows;
        state.current_music_time_visible[0] = 5.99;
        refresh_active_attack_masks(&mut state, 0.0);
        let eased_stealth = effective_appearance_effects_for_player(&state, 0).stealth;
        assert!(eased_stealth > 0.4);
        assert!(effective_visual_effects_for_player(&state, 0).pulse_outer > 0.0);

        state.current_music_time_visible[0] = 6.016;
        refresh_active_attack_masks(&mut state, 0.026);
        let fading_stealth = effective_appearance_effects_for_player(&state, 0).stealth;
        assert!(fading_stealth > 0.0);
        assert!(fading_stealth < eased_stealth);

        state.current_music_time_visible[0] = 7.0;
        refresh_active_attack_masks(&mut state, 0.984);
        assert!(
            effective_appearance_effects_for_player(&state, 0)
                .stealth
                .abs()
                <= 0.000_1
        );
        assert!(
            effective_visual_effects_for_player(&state, 0)
                .pulse_outer
                .abs()
                <= 0.000_1
        );
    }

    #[test]
    fn song_lua_constant_mods_persist_after_attack_window() {
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            beat_mods: vec![crate::game::parsing::song_lua::SongLuaModWindow {
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 1.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                mods: "*100 314 confusionoffset".to_string(),
                player: Some(1),
            }],
            ..Default::default()
        };
        state.attack_mask_windows[0] =
            super::build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);

        state.current_music_time_visible[0] = 2.0;
        refresh_active_attack_masks(&mut state, 0.0);

        let visual = effective_visual_effects_for_player(&state, 0);
        assert!((visual.confusion_offset - 3.14).abs() <= 0.000_1);
    }

    #[test]
    fn song_lua_constant_visual_scroll_and_mini_mods_approach() {
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            beat_mods: vec![crate::game::parsing::song_lua::SongLuaModWindow {
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 3.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                mods: "*10 50% flip, *10 10% reverse, *10 -100% mini".to_string(),
                player: Some(1),
            }],
            ..Default::default()
        };
        state.attack_mask_windows[0] =
            super::build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);

        state.current_music_time_visible[0] = 0.016;
        refresh_active_attack_masks(&mut state, 0.016);
        let visual = effective_visual_effects_for_player(&state, 0);
        let scroll = effective_scroll_effects_for_player(&state, 0);
        let mini = effective_mini_percent_for_player(&state, 0);
        assert!(visual.flip > 0.0);
        assert!(visual.flip < 0.5);
        assert!((scroll.reverse - 0.1).abs() <= 0.000_1);
        assert!(mini < 0.0);
        assert!(mini > -100.0);

        state.current_music_time_visible[0] = 1.016;
        refresh_active_attack_masks(&mut state, 1.0);
        let visual = effective_visual_effects_for_player(&state, 0);
        let mini = effective_mini_percent_for_player(&state, 0);
        assert!((visual.flip - 0.5).abs() <= 0.000_1);
        assert!((mini + 100.0).abs() <= 0.000_1);
    }

    #[test]
    fn song_lua_constant_mini_layers_on_profile_mini() {
        let mut profiles = std::array::from_fn(|_| profile_data::Profile::default());
        profiles[0].mini_percent = 50;
        let mut state = regression_state(profiles);
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            beat_mods: vec![crate::game::parsing::song_lua::SongLuaModWindow {
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 3.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                mods: "*10 -100% mini".to_string(),
                player: Some(1),
            }],
            ..Default::default()
        };
        state.attack_mask_windows[0] =
            super::build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);

        state.current_music_time_visible[0] = 0.016;
        refresh_active_attack_masks(&mut state, 0.016);
        let mini = effective_mini_percent_for_player(&state, 0);
        assert!((mini - 34.0).abs() <= 0.000_1);

        state.current_music_time_visible[0] = 1.016;
        refresh_active_attack_masks(&mut state, 1.0);
        let mini = effective_mini_percent_for_player(&state, 0);
        assert!((mini + 50.0).abs() <= 0.000_1);
    }

    #[test]
    fn song_lua_eased_mini_layers_on_profile_mini() {
        let mut profiles = std::array::from_fn(|_| profile_data::Profile::default());
        profiles[0].mini_percent = 50;
        let mut state = regression_state(profiles);
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            eases: vec![crate::game::parsing::song_lua::SongLuaEaseWindow {
                player: Some(1),
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 4.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                target: crate::game::parsing::song_lua::SongLuaEaseTarget::Mod("mini".to_string()),
                from: 0.0,
                to: -100.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            }],
            ..Default::default()
        };
        let (windows, unsupported) =
            super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);
        assert_eq!(unsupported, 0);
        state.song_lua_ease_windows[0] = windows;

        state.current_music_time_visible[0] = 2.0;
        refresh_active_attack_masks(&mut state, 0.0);

        let mini = effective_mini_percent_for_player(&state, 0);
        assert!(mini.abs() <= 0.000_1);
    }

    #[test]
    fn chart_attack_mini_overrides_profile_mini() {
        let mut profiles = std::array::from_fn(|_| profile_data::Profile::default());
        profiles[0].mini_percent = 50;
        let mut state = regression_state(profiles);
        state.attack_mask_windows[0] = build_attack_mask_windows_for_player(
            Some("TIME=0.000:LEN=3.000:MODS=*1000 25% mini"),
            profile_data::AttackMode::On,
            0,
            0x1234,
            10.0,
        );

        state.current_music_time_visible[0] = 1.0;
        refresh_active_attack_masks(&mut state, 1.0);

        let mini = effective_mini_percent_for_player(&state, 0);
        assert!((mini - 25.0).abs() <= 0.000_1);
    }

    #[test]
    fn song_lua_active_reset_overrides_ended_constant_mods() {
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            beat_mods: vec![
                crate::game::parsing::song_lua::SongLuaModWindow {
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 9999.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::End,
                    mods: "*1000 no invert, *1000 no flip".to_string(),
                    player: Some(1),
                },
                crate::game::parsing::song_lua::SongLuaModWindow {
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 0.25,
                    limit: 0.25,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    mods: "*1000 invert".to_string(),
                    player: Some(1),
                },
                crate::game::parsing::song_lua::SongLuaModWindow {
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 0.5,
                    limit: 0.25,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    mods: "*1000 flip".to_string(),
                    player: Some(1),
                },
            ],
            ..Default::default()
        };
        state.attack_mask_windows[0] =
            super::build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);

        state.current_music_time_visible[0] = 0.6;
        refresh_active_attack_masks(&mut state, 0.0);
        let visual = effective_visual_effects_for_player(&state, 0);
        assert!((visual.flip - 1.0).abs() <= 0.000_1);
        assert!(visual.invert.abs() <= 0.000_1);

        state.current_music_time_visible[0] = 1.1;
        refresh_active_attack_masks(&mut state, 0.0);
        let reset = effective_visual_effects_for_player(&state, 0);
        assert!(reset.flip.abs() <= 0.000_1);
        assert!(reset.invert.abs() <= 0.000_1);
    }

    #[test]
    fn riddle_beat_70_confusion_offset_reaches_visual_state_if_present() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let Some(root) = [
            manifest.join("../lua-songs/Riddle"),
            manifest.join("songs/lua-songs/Riddle"),
        ]
        .into_iter()
        .find(|root| root.join("lua/default.lua").is_file()) else {
            return;
        };
        let entry = root.join("lua/default.lua");
        let mut context =
            crate::game::parsing::song_lua::SongLuaCompileContext::new(&root, "Riddle");
        context.style_name = "double".to_string();
        context.players = [
            crate::game::parsing::song_lua::SongLuaPlayerContext {
                enabled: true,
                difficulty: crate::game::parsing::song_lua::SongLuaDifficulty::Challenge,
                speedmod: crate::game::parsing::song_lua::SongLuaSpeedMod::X(2.0),
                ..crate::game::parsing::song_lua::SongLuaPlayerContext::default()
            },
            crate::game::parsing::song_lua::SongLuaPlayerContext {
                enabled: false,
                difficulty: crate::game::parsing::song_lua::SongLuaDifficulty::Challenge,
                speedmod: crate::game::parsing::song_lua::SongLuaSpeedMod::X(2.0),
                ..crate::game::parsing::song_lua::SongLuaPlayerContext::default()
            },
        ];
        let compiled = crate::game::parsing::song_lua::compile_song_lua(&entry, &context).unwrap();
        assert!(compiled.beat_mods.iter().any(|window| {
            (window.start - 70.5).abs() <= 0.001 && window.mods.contains("80% confusionoffset")
        }));

        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 128.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.036, 0.0, &timing_segments, &test_row_to_beat(72 * 48));
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        state.attack_mask_windows[0] =
            super::build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);

        state.current_music_time_visible[0] = timing.get_time_for_beat(70.75);
        refresh_active_attack_masks(&mut state, 0.0);
        let tilted = effective_visual_effects_for_player(&state, 0);
        assert!((tilted.confusion_offset - 0.8).abs() <= 0.000_1);

        state.current_music_time_visible[0] = timing.get_time_for_beat(71.25);
        refresh_active_attack_masks(&mut state, 0.0);
        let reset = effective_visual_effects_for_player(&state, 0);
        assert!(reset.confusion_offset.abs() <= 0.000_1);
    }

    #[test]
    fn kenpo_flash_mods_reach_runtime_masks_if_present() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let Some(root) = [
            manifest.join("../lua-songs/[11] KENPO SAITO (DX) [Scrypts]"),
            manifest.join("songs/ITL Online 2026/[11] KENPO SAITO (DX) [Scrypts]"),
            manifest.join("songs/lua-songs/[11] KENPO SAITO (DX) [Scrypts]"),
        ]
        .into_iter()
        .find(|root| root.join("template/main.lua").is_file()) else {
            return;
        };
        let entry = root.join("template/main.lua");
        let mut context =
            crate::game::parsing::song_lua::SongLuaCompileContext::new(&root, "KENPO SAITO");
        context.style_name = "double".to_string();
        context.players = [
            crate::game::parsing::song_lua::SongLuaPlayerContext {
                enabled: true,
                difficulty: crate::game::parsing::song_lua::SongLuaDifficulty::Challenge,
                speedmod: crate::game::parsing::song_lua::SongLuaSpeedMod::X(2.0),
                ..crate::game::parsing::song_lua::SongLuaPlayerContext::default()
            },
            crate::game::parsing::song_lua::SongLuaPlayerContext {
                enabled: false,
                difficulty: crate::game::parsing::song_lua::SongLuaDifficulty::Challenge,
                speedmod: crate::game::parsing::song_lua::SongLuaSpeedMod::X(2.0),
                ..crate::game::parsing::song_lua::SongLuaPlayerContext::default()
            },
        ];
        let compiled = crate::game::parsing::song_lua::compile_song_lua(&entry, &context).unwrap();
        assert!(compiled.eases.iter().any(|window| {
            matches!(
                window.target,
                crate::game::parsing::song_lua::SongLuaEaseTarget::Mod(ref name)
                    if name == "tiny"
            ) && (window.start - 26.5).abs() <= 0.001
                && (window.to + 200.0).abs() <= 0.001
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(
                window.target,
                crate::game::parsing::song_lua::SongLuaEaseTarget::Mod(ref name)
                    if name == "flip"
            ) && (window.start - 26.5).abs() <= 0.001
                && (window.to - 50.0).abs() <= 0.001
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(
                window.target,
                crate::game::parsing::song_lua::SongLuaEaseTarget::Mod(ref name)
                    if name == "dark"
            ) && (window.start - 28.0).abs() <= 0.001
                && (window.to - 100.0).abs() <= 0.001
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(
                window.target,
                crate::game::parsing::song_lua::SongLuaEaseTarget::Mod(ref name)
                    if name == "skewx"
            ) && (window.start - 166.0).abs() <= 0.001
                && (window.to.abs() - 3.0).abs() <= 0.001
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(
                window.target,
                crate::game::parsing::song_lua::SongLuaEaseTarget::Mod(ref name)
                    if name == "skewx"
            ) && (window.start - 182.0).abs() <= 0.001
                && (window.to.abs() - 3.0).abs() <= 0.001
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(
                window.target,
                crate::game::parsing::song_lua::SongLuaEaseTarget::PlayerRotationX
            ) && (window.start - 189.0).abs() <= 0.001
                && (window.to - 20.0).abs() <= 0.001
        }));

        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 77.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(200 * 48));
        let constants =
            super::build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);
        let (windows, unsupported) =
            super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &constants);
        assert_eq!(unsupported, 0);
        assert!(windows.iter().any(|window| {
            matches!(
                window.target,
                super::attacks::SongLuaEaseMaskTarget::PlayerSkewX
            ) && (window.start_second - timing.get_time_for_beat(166.0)).abs() <= 0.001
                && (window.to.abs() - 0.03).abs() <= 0.000_1
        }));
        assert!(windows.iter().any(|window| {
            matches!(
                window.target,
                super::attacks::SongLuaEaseMaskTarget::PlayerSkewX
            ) && (window.start_second - timing.get_time_for_beat(182.0)).abs() <= 0.001
                && (window.to.abs() - 0.03).abs() <= 0.000_1
        }));
        assert!(windows.iter().any(|window| {
            matches!(
                window.target,
                super::attacks::SongLuaEaseMaskTarget::PlayerRotationX
            ) && (window.start_second - timing.get_time_for_beat(189.0)).abs() <= 0.001
                && (window.to - 20.0).abs() <= 0.000_1
        }));

        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        state.attack_mask_windows[0] = constants;
        state.song_lua_ease_windows[0] = windows;

        state.current_music_time_visible[0] = timing.get_time_for_beat(27.25);
        refresh_active_attack_masks(&mut state, 0.0);
        let pre_flash_visual = effective_visual_effects_for_player(&state, 0);
        assert!((pre_flash_visual.tiny + 1.0).abs() <= 0.000_1);
        assert!((pre_flash_visual.flip - 0.25).abs() <= 0.000_1);

        state.current_music_time_visible[0] = timing.get_time_for_beat(29.0);
        refresh_active_attack_masks(&mut state, 0.0);
        let hidden_visibility = effective_visibility_effects_for_player(&state, 0);
        let reset_visual = effective_visual_effects_for_player(&state, 0);
        assert!((hidden_visibility.dark - 1.0).abs() <= 0.000_1);
        assert!(reset_visual.tiny.abs() <= 0.000_1);
        assert!(reset_visual.flip.abs() <= 0.000_1);

        state.current_music_time_visible[0] = timing.get_time_for_beat(31.0);
        refresh_active_attack_masks(&mut state, 0.0);
        let fading_visibility = effective_visibility_effects_for_player(&state, 0);
        assert!((fading_visibility.dark - 0.5).abs() <= 0.000_1);
    }

    #[test]
    fn song_lua_column_offsets_persist_until_next_column_offset() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            column_offsets: vec![
                crate::game::parsing::song_lua::SongLuaColumnOffsetWindow {
                    player: 0,
                    column: 2,
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 0.5,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    from_y: 33.75,
                    to_y: 0.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaColumnOffsetWindow {
                    player: 0,
                    column: 2,
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 2.0,
                    limit: 0.5,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    from_y: 0.0,
                    to_y: 33.75,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
            ],
            ..Default::default()
        };

        let windows =
            super::build_song_lua_column_offset_windows_for_player(&compiled, &timing, 0, 0.0);

        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].sustain_end_second, 2.0);
        assert_eq!(windows[1].sustain_end_second, f32::MAX);
    }

    #[test]
    fn song_lua_builds_playerxy_playerz_rotationx_skewy_zoom_and_zoomz_runtime_targets() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            eases: vec![
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 1.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::PlayerX,
                    from: 320.0,
                    to: 360.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 1.0,
                    limit: 1.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::PlayerY,
                    from: 240.0,
                    to: 210.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 2.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::PlayerZ,
                    from: 0.0,
                    to: -120.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 4.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::PlayerRotationX,
                    from: 0.0,
                    to: 20.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 4.0,
                    limit: 2.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::PlayerSkewY,
                    from: 0.0,
                    to: 0.25,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 6.0,
                    limit: 2.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::PlayerZoom,
                    from: 1.0,
                    to: 0.75,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 8.0,
                    limit: 2.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::PlayerZoomZ,
                    from: 1.0,
                    to: 1.25,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
            ],
            ..Default::default()
        };

        let (windows, unsupported) =
            super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);

        assert_eq!(unsupported, 0);
        assert_eq!(windows.len(), 7);
        assert!(matches!(
            windows[0].target,
            super::attacks::SongLuaEaseMaskTarget::PlayerX
        ));
        assert!(matches!(
            windows[1].target,
            super::attacks::SongLuaEaseMaskTarget::PlayerY
        ));
        assert!(matches!(
            windows[2].target,
            super::attacks::SongLuaEaseMaskTarget::PlayerZ
        ));
        assert!(matches!(
            windows[3].target,
            super::attacks::SongLuaEaseMaskTarget::PlayerRotationX
        ));
        assert!(matches!(
            windows[4].target,
            super::attacks::SongLuaEaseMaskTarget::PlayerSkewY
        ));
        assert!(matches!(
            windows[5].target,
            super::attacks::SongLuaEaseMaskTarget::PlayerZoom
        ));
        assert!(matches!(
            windows[6].target,
            super::attacks::SongLuaEaseMaskTarget::PlayerZoomZ
        ));
        assert!(
            super::song_lua_ease_window_value(&windows[0], 0.5)
                .is_some_and(|value| (value - 340.0).abs() <= 0.000_1)
        );
        assert!(
            super::song_lua_ease_window_value(&windows[1], 1.5)
                .is_some_and(|value| (value - 225.0).abs() <= 0.000_1)
        );
        assert!(
            super::song_lua_ease_window_value(&windows[2], 1.0)
                .is_some_and(|value| (value + 60.0).abs() <= 0.000_1)
        );
        assert!(
            super::song_lua_ease_window_value(&windows[3], 2.0)
                .is_some_and(|value| (value - 10.0).abs() <= 0.000_1)
        );
        assert!(
            super::song_lua_ease_window_value(&windows[4], 5.0)
                .is_some_and(|value| (value - 0.125).abs() <= 0.000_1)
        );
        assert!(
            super::song_lua_ease_window_value(&windows[5], 7.0)
                .is_some_and(|value| (value - 0.875).abs() <= 0.000_1)
        );
        assert!(
            super::song_lua_ease_window_value(&windows[6], 9.0)
                .is_some_and(|value| (value - 1.125).abs() <= 0.000_1)
        );
    }

    #[test]
    fn song_lua_skew_mod_eases_scale_to_player_skews() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            eases: vec![
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 1.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::Mod(
                        "skewx".to_string(),
                    ),
                    from: 0.0,
                    to: 3.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 1.0,
                    limit: 1.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::Mod(
                        "skewy".to_string(),
                    ),
                    from: 0.0,
                    to: -4.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
            ],
            ..Default::default()
        };

        let (windows, unsupported) =
            super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);

        assert_eq!(unsupported, 0);
        assert_eq!(windows.len(), 2);
        assert!(matches!(
            windows[0].target,
            super::attacks::SongLuaEaseMaskTarget::PlayerSkewX
        ));
        assert!(matches!(
            windows[1].target,
            super::attacks::SongLuaEaseMaskTarget::PlayerSkewY
        ));
        assert!((windows[0].to - 0.03).abs() <= 0.000_1);
        assert!((windows[1].to + 0.04).abs() <= 0.000_1);
    }

    #[test]
    fn parse_song_lua_runtime_mods_scales_confusion_offset_like_itgmania() {
        let mods = parse_song_lua_runtime_mods("*10000 -628 confusionoffset");
        assert_eq!(mods.visual.confusion_offset, Some(-6.28));
    }

    #[test]
    fn song_lua_confusion_offset_ease_scales_like_itgmania() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            eases: vec![crate::game::parsing::song_lua::SongLuaEaseWindow {
                player: Some(1),
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 4.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                target: crate::game::parsing::song_lua::SongLuaEaseTarget::Mod(
                    "confusionoffset".to_string(),
                ),
                from: -85.0,
                to: 0.0,
                easing: Some("outQuad".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            }],
            ..Default::default()
        };

        let (windows, unsupported) =
            super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);

        assert_eq!(unsupported, 0);
        assert_eq!(windows.len(), 1);
        assert!(matches!(
            windows[0].target,
            super::attacks::SongLuaEaseMaskTarget::VisualConfusionOffset
        ));
        assert!((windows[0].from + 0.85).abs() <= 0.000_1);
        assert!(windows[0].to.abs() <= 0.000_1);
    }

    #[test]
    fn attack_windows_keep_chart_only_effects_for_live_state() {
        let windows = build_attack_mask_windows_for_player(
            Some("TIME=1.0:LEN=2.0:MODS=mirror,mines"),
            profile_data::AttackMode::On,
            0,
            123,
            10.0,
        );
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].chart.insert_mask, INSERT_MASK_BIT_MINES);
        assert_eq!(
            windows[0].chart.turn_bits,
            turn_option_bits(GameplayTurnOption::Mirror)
        );
    }

    #[test]
    fn scroll_effects_reverse_percent_matches_itg_column_rules() {
        let scroll = ScrollEffects {
            reverse: 1.0,
            split: 1.0,
            alternate: 1.0,
            cross: 0.0,
            centered: 0.0,
        };
        assert!((scroll.reverse_percent_for_column(0, 4) - 1.0).abs() <= 1e-6);
        assert!(scroll.reverse_percent_for_column(1, 4).abs() <= 1e-6);
        assert!(scroll.reverse_percent_for_column(2, 4).abs() <= 1e-6);
        assert!((scroll.reverse_percent_for_column(3, 4) - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn player_draw_scale_helper_uses_supplied_tilt() {
        let profile = profile_data::Profile::default();
        let base = player_draw_scale_for_tilt_with_visual_mask(0.0, &profile, 0, 0.0);
        let tilted = player_draw_scale_for_tilt_with_visual_mask(-1.0, &profile, 0, 0.0);
        assert!((base - 1.0).abs() <= 1e-6);
        assert!((tilted - 1.5).abs() <= 1e-6);
    }

    #[test]
    fn mines_insert_converts_every_sixth_nonempty_row() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(5 * 48),
        );
        let mut notes = (0..6)
            .map(|i| {
                let row = i * 48;
                Note {
                    beat: row as f32 / 48.0,
                    quantization_idx: 0,
                    column: 0,
                    note_type: NoteType::Tap,
                    row_index: row,
                    result: None,
                    early_result: None,
                    hold: None,
                    mine_result: None,
                    is_fake: false,
                    can_be_judged: true,
                }
            })
            .collect::<Vec<_>>();
        apply_mines_insert(&mut notes, &[], &timing, 0, 4, 0, 5 * 48);
        assert!(
            notes
                .iter()
                .any(|note| note.row_index == 5 * 48 && note.note_type == NoteType::Mine)
        );
    }

    #[test]
    fn mines_insert_adds_mine_half_beat_after_hold_end() {
        let timing =
            TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &test_row_to_beat(144));
        let mut notes = vec![Note {
            beat: 0.0,
            quantization_idx: 0,
            column: 1,
            note_type: NoteType::Hold,
            row_index: 0,
            result: None,
            early_result: None,
            hold: Some(HoldData {
                end_row_index: 96,
                end_beat: 2.0,
                result: None,
                life: 1.0,
                let_go_started_at: None,
                let_go_starting_life: 0.0,
                last_held_row_index: 0,
                last_held_beat: 0.0,
            }),
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }];
        apply_mines_insert(&mut notes, &[], &timing, 0, 4, 0, 144);
        assert!(notes.iter().any(|note| note.row_index == 120
            && note.column == 1
            && note.note_type == NoteType::Mine));
    }

    #[test]
    fn mine_window_bounds_exclude_left_edge_and_include_right_edge() {
        let mine_times_ns = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.5),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(2.5),
        ];
        assert_eq!(
            mine_window_bounds_ns(
                &mine_times_ns,
                song_time_ns_from_seconds(1.5),
                song_time_ns_from_seconds(2.0),
            ),
            (1, 3)
        );
    }

    #[test]
    fn crossed_mine_bounds_skip_previous_frame_boundary() {
        let mine_times_ns = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.5),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(2.5),
        ];
        assert_eq!(
            crossed_mine_bounds_ns(
                &mine_times_ns,
                song_time_ns_from_seconds(1.5),
                song_time_ns_from_seconds(2.0),
            ),
            (2, 3)
        );
    }

    #[test]
    fn missed_note_cutoff_row_matches_itg_stop_delay_rules() {
        let row_to_beat = test_row_to_beat(ROWS_PER_BEAT as usize * 4);
        let stop_timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                stops: vec![StopSegment {
                    beat: 1.0,
                    duration: 2.0,
                }],
                ..TimingSegments::default()
            },
            &row_to_beat,
        );
        let stop_cutoff_time = stop_timing
            .get_time_for_beat_ns(1.0)
            .saturating_add(song_time_ns_from_seconds(0.5));
        assert_eq!(
            missed_note_cutoff_row_for_timing(&stop_timing, stop_cutoff_time),
            ROWS_PER_BEAT as usize + 1
        );

        let delay_timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                delays: vec![DelaySegment {
                    beat: 1.0,
                    duration: 2.0,
                }],
                ..TimingSegments::default()
            },
            &row_to_beat,
        );
        let delay_cutoff_time = delay_timing
            .get_time_for_beat_ns(1.0)
            .saturating_sub(song_time_ns_from_seconds(0.5));
        assert_eq!(
            missed_note_cutoff_row_for_timing(&delay_timing, delay_cutoff_time),
            ROWS_PER_BEAT as usize
        );
    }

    #[test]
    fn missed_note_cutoff_row_advances_past_final_row() {
        let final_row = ROWS_PER_BEAT as usize;
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                ..TimingSegments::default()
            },
            &test_row_to_beat(final_row),
        );
        let cutoff_time = timing
            .get_time_for_beat_ns(1.0)
            .saturating_add(song_time_ns_from_seconds(0.1));

        assert!(missed_note_cutoff_row_for_timing(&timing, cutoff_time) > final_row);
    }

    #[test]
    fn missed_note_cutoff_row_uses_chart_row_indices() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                ..TimingSegments::default()
            },
            &[0.0, 4.0, 8.0],
        );

        assert_eq!(
            missed_note_cutoff_row_for_timing(&timing, timing.get_time_for_beat_ns(3.0)),
            1
        );
        assert_eq!(
            missed_note_cutoff_row_for_timing(&timing, timing.get_time_for_beat_ns(4.0)),
            1
        );
        assert_eq!(
            missed_note_cutoff_row_for_timing(&timing, timing.get_time_for_beat_ns(4.1)),
            2
        );
    }

    #[test]
    fn delayed_rows_do_not_time_miss_or_avoid_until_delay_finishes() {
        let timing = Arc::new(TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                delays: vec![DelaySegment {
                    beat: 1.0,
                    duration: 2.0,
                }],
                ..TimingSegments::default()
            },
            &test_row_to_beat(ROWS_PER_BEAT as usize * 4),
        ));
        let note_time_ns = timing.get_time_for_beat_ns(1.0);

        let mut tap_state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        set_state_timing(&mut tap_state, Arc::clone(&timing));
        tap_state.note_time_cache_ns[0] = note_time_ns;
        let miss_distance_ns =
            max_step_distance_ns(&tap_state.timing_profile, tap_state.music_rate);
        let inside_delay_music_time = note_time_ns
            .saturating_add(miss_distance_ns)
            .saturating_add(song_time_ns_from_seconds(0.5));
        apply_time_based_tap_misses(&mut tap_state, inside_delay_music_time);
        assert!(tap_state.notes[0].result.is_none());
        assert_eq!(tap_state.next_tap_miss_cursor[0], 0);

        let after_delay_music_time = note_time_ns
            .saturating_add(miss_distance_ns)
            .saturating_add(song_time_ns_from_seconds(2.1));
        apply_time_based_tap_misses(&mut tap_state, after_delay_music_time);
        assert_eq!(
            tap_state.notes[0].result.as_ref().map(|j| j.grade),
            Some(JudgeGrade::Miss)
        );

        let mut mine_state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        set_state_timing(&mut mine_state, Arc::clone(&timing));
        set_regression_mine(&mut mine_state, 0, 0, ROWS_PER_BEAT as usize, note_time_ns);
        let mine_distance_ns =
            max_step_distance_ns(&mine_state.timing_profile, mine_state.music_rate);
        let inside_delay_music_time = note_time_ns
            .saturating_add(mine_distance_ns)
            .saturating_add(song_time_ns_from_seconds(0.5));
        apply_time_based_mine_avoidance(&mut mine_state, inside_delay_music_time);
        assert_eq!(mine_state.notes[0].mine_result, None);
        assert_eq!(mine_state.next_mine_ix_cursor[0], 0);

        let after_delay_music_time = note_time_ns
            .saturating_add(mine_distance_ns)
            .saturating_add(song_time_ns_from_seconds(2.1));
        apply_time_based_mine_avoidance(&mut mine_state, after_delay_music_time);
        assert_eq!(mine_state.notes[0].mine_result, Some(MineResult::Avoided));
    }

    #[test]
    fn completed_song_counts_last_mine_as_avoided_at_end_cutoff() {
        let timing = Arc::new(TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                ..TimingSegments::default()
            },
            &test_row_to_beat(ROWS_PER_BEAT as usize * 4),
        ));
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        set_state_timing(&mut state, Arc::clone(&timing));

        let mine_row = ROWS_PER_BEAT as usize;
        let mine_time_ns = timing.get_time_for_beat_ns(1.0);
        set_regression_mine(&mut state, 0, 0, mine_row, mine_time_ns);
        let end_time_ns = mine_time_ns.saturating_add(max_step_distance_ns(
            &state.timing_profile,
            state.music_rate,
        ));

        apply_time_based_mine_avoidance(&mut state, end_time_ns);
        assert_eq!(state.players[0].mines_avoided, 0);
        assert_eq!(state.notes[0].mine_result, None);

        finalize_completed_mines(&mut state);
        assert_eq!(state.players[0].mines_avoided, 1);
        assert_eq!(state.notes[0].mine_result, Some(MineResult::Avoided));
    }

    #[test]
    fn completed_song_finalizes_last_tap_miss_before_eval() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        assert_eq!(state.num_players, 1);

        let (note_start, note_end) = state.note_ranges[0];
        let first_note = note_start;
        let last_note = note_end - 1;
        let first_row_entry = state.note_row_entry_indices[first_note] as usize;
        let last_row_entry = state.note_row_entry_indices[last_note] as usize;
        let miss_ix = judgment::judge_grade_ix(JudgeGrade::Miss);

        set_final_note_result(
            &mut state,
            first_note,
            Judgment {
                time_error_ms: 0.0,
                time_error_music_ns: 0,
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
        );
        state.current_music_time_ns = state.note_time_cache_ns[first_note].saturating_add(
            max_step_distance_ns(&state.timing_profile, state.music_rate),
        );
        assert!(!settle_completion_rows(&mut state));
        assert!(state.row_entries[first_row_entry].final_outcome.is_some());
        assert!(state.row_entries[last_row_entry].final_outcome.is_none());

        let miss_time_ns = state.note_time_cache_ns[last_note]
            .saturating_add(max_step_distance_ns(
                &state.timing_profile,
                state.music_rate,
            ))
            .saturating_add(song_time_ns_from_seconds(0.1));
        state.current_music_time_ns = miss_time_ns;

        // The normal frame order has already scanned rows before overdue taps
        // are promoted to misses.
        update_judged_rows(&mut state);
        apply_time_based_tap_misses(&mut state, miss_time_ns);
        assert_eq!(
            state.notes[last_note].result.as_ref().map(|j| j.grade),
            Some(JudgeGrade::Miss)
        );
        assert!(state.row_entries[last_row_entry].final_outcome.is_none());
        assert_eq!(state.players[0].judgment_counts[miss_ix], 0);

        assert!(settle_completion_rows(&mut state));
        assert_eq!(
            state.row_entries[last_row_entry].final_outcome,
            Some(FinalizedRowOutcome {
                final_grade: JudgeGrade::Miss,
            })
        );
        assert_eq!(state.players[0].judgment_counts[miss_ix], 1);
    }

    #[test]
    fn crossed_mine_held_start_accepts_new_press_during_frame() {
        assert_eq!(
            crossed_mine_held_start_time(
                true,
                false,
                Some(song_time_ns_from_seconds(1.25)),
                song_time_ns_from_seconds(1.0),
                song_time_ns_from_seconds(1.5),
            ),
            Some(song_time_ns_from_seconds(1.25))
        );
        assert_eq!(
            crossed_mine_held_start_time(
                true,
                false,
                Some(song_time_ns_from_seconds(1.6)),
                song_time_ns_from_seconds(1.0),
                song_time_ns_from_seconds(1.5),
            ),
            None
        );
    }

    #[test]
    fn crossed_held_mine_hits_even_when_frame_offset_exceeds_mine_window() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        let mine_time_ns = song_time_ns_from_seconds(1.0);
        set_regression_mine(&mut state, 0, 0, 48, mine_time_ns);

        assert!(try_hit_crossed_mines_while_held(
            &mut state,
            0,
            song_time_ns_from_seconds(0.9),
            song_time_ns_from_seconds(1.2),
        ));

        assert_eq!(state.notes[0].mine_result, Some(MineResult::Hit));
        assert_eq!(state.pending_mine_hit_indices, vec![0]);
        assert_eq!(state.players[0].mines_hit, 0);
        assert_eq!(state.players[0].mines_hit_for_score, 0);

        apply_pending_mine_hits(&mut state);

        assert_eq!(state.players[0].mines_hit, 1);
        assert_eq!(state.players[0].mines_hit_for_score, 1);
    }

    #[test]
    fn mine_hit_side_effects_wait_until_after_active_holds() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        let hold_end_ns = song_time_ns_from_seconds(1.0);
        state.notes[0] = test_hold(0, 0, ROWS_PER_BEAT as usize);
        state.hold_end_time_cache_ns[0] = Some(hold_end_ns);
        set_regression_mine(&mut state, 1, 1, ROWS_PER_BEAT as usize, hold_end_ns);
        state.players[0].life = 0.04;
        state.active_holds[0] = Some(super::ActiveHold {
            note_index: 0,
            start_time_ns: 0,
            end_time_ns: hold_end_ns,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: true,
            life: super::MAX_HOLD_LIFE,
            last_update_time_ns: 0,
        });

        assert!(hit_mine(&mut state, 1, 1, 0));
        assert_eq!(state.notes[1].mine_result, Some(MineResult::Hit));
        assert_eq!(state.players[0].mines_hit, 0);
        assert!(!state.players[0].is_failing);

        let inputs = std::array::from_fn(|col| col == 0);
        update_active_holds(&mut state, &inputs, hold_end_ns);
        assert_eq!(
            state.notes[0].hold.as_ref().and_then(|hold| hold.result),
            Some(HoldResult::Held)
        );
        assert_eq!(state.players[0].holds_held_for_score, 1);
        assert!(!state.players[0].is_failing);

        apply_pending_mine_hits(&mut state);
        assert_eq!(state.players[0].mines_hit, 1);
        assert_eq!(state.players[0].mines_hit_for_score, 0);
        assert!(state.players[0].is_failing);
    }

    #[test]
    fn scored_missed_hold_resolves_let_go_at_hold_end() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        let note_time_ns = song_time_ns_from_seconds(1.0);
        let hold_end_ns = song_time_ns_from_seconds(2.0);
        state.score_missed_holds_rolls[0] = true;
        state.notes[0] = test_hold(0, 48, 96);
        state.note_time_cache_ns[0] = note_time_ns;
        state.hold_end_time_cache_ns[0] = Some(hold_end_ns);
        state.notes[1].can_be_judged = false;

        let miss_time_ns = note_time_ns
            .saturating_add(max_step_distance_ns(
                &state.timing_profile,
                state.music_rate,
            ))
            .saturating_add(song_time_ns_from_seconds(0.1));
        apply_time_based_tap_misses(&mut state, miss_time_ns);

        assert_eq!(
            state.notes[0]
                .result
                .as_ref()
                .map(|judgment| judgment.grade),
            Some(JudgeGrade::Miss)
        );
        assert_eq!(state.notes[0].hold.as_ref().and_then(|h| h.result), None);
        assert_eq!(state.players[0].holds_let_go_for_score, 0);

        resolve_pending_missed_holds(&mut state, hold_end_ns.saturating_sub(1));
        assert_eq!(state.notes[0].hold.as_ref().and_then(|h| h.result), None);
        assert_eq!(state.players[0].holds_let_go_for_score, 0);

        resolve_pending_missed_holds(&mut state, hold_end_ns);

        assert_eq!(
            state.notes[0].hold.as_ref().and_then(|hold| hold.result),
            Some(HoldResult::LetGo)
        );
        assert_eq!(state.players[0].holds_let_go_for_score, 1);
        assert_eq!(
            state.hold_judgments[0].as_ref().map(|info| info.result),
            Some(HoldResult::LetGo)
        );
    }

    #[test]
    fn unscored_missed_hold_emits_missed_feedback_at_hold_end() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        let note_time_ns = song_time_ns_from_seconds(1.0);
        let hold_end_ns = song_time_ns_from_seconds(2.0);
        state.score_missed_holds_rolls[0] = false;
        state.notes[0] = test_hold(0, 48, 96);
        state.note_time_cache_ns[0] = note_time_ns;
        state.hold_end_time_cache_ns[0] = Some(hold_end_ns);
        state.notes[1].can_be_judged = false;

        let miss_time_ns = note_time_ns
            .saturating_add(max_step_distance_ns(
                &state.timing_profile,
                state.music_rate,
            ))
            .saturating_add(song_time_ns_from_seconds(0.1));
        apply_time_based_tap_misses(&mut state, miss_time_ns);

        assert_eq!(
            state.notes[0].hold.as_ref().and_then(|hold| hold.result),
            Some(HoldResult::Missed)
        );
        assert_eq!(state.players[0].holds_let_go_for_score, 0);
        assert!(state.hold_judgments[0].is_none());

        resolve_pending_missed_holds(&mut state, hold_end_ns);

        assert_eq!(state.players[0].holds_let_go_for_score, 0);
        assert_eq!(
            state.hold_judgments[0].as_ref().map(|info| info.result),
            Some(HoldResult::Missed)
        );
    }

    #[test]
    fn crossed_held_mine_new_press_excludes_rows_before_press() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        set_regression_mine(&mut state, 0, 0, 48, song_time_ns_from_seconds(1.0));
        let crossed_from_ns = crossed_mine_held_start_time(
            true,
            false,
            Some(song_time_ns_from_seconds(1.1)),
            song_time_ns_from_seconds(0.9),
            song_time_ns_from_seconds(1.2),
        )
        .expect("new press should produce a crossed-row start");

        assert!(!try_hit_crossed_mines_while_held(
            &mut state,
            0,
            crossed_from_ns,
            song_time_ns_from_seconds(1.2),
        ));
        assert_eq!(state.notes[0].mine_result, None);
    }

    #[test]
    fn lane_note_window_bounds_exclude_left_edge_and_include_right_edge() {
        let note_indices = [4usize, 9, 15];
        let mut note_times_ns = [0; 16];
        note_times_ns[4] = song_time_ns_from_seconds(1.0);
        note_times_ns[9] = song_time_ns_from_seconds(1.5);
        note_times_ns[15] = song_time_ns_from_seconds(2.0);
        assert_eq!(
            lane_note_window_bounds_ns(
                &note_indices,
                &note_times_ns,
                song_time_ns_from_seconds(1.5),
                song_time_ns_from_seconds(2.0),
            ),
            (1, 3)
        );
    }

    #[test]
    fn lane_note_row_window_excludes_step_search_end_row() {
        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_note(0, 96, NoteType::Tap),
            test_note(0, 144, NoteType::Tap),
        ];
        let note_indices = [0usize, 1, 2];

        assert_eq!(
            lane_note_window_bounds_rows(&note_indices, &notes, 48, 144),
            (0, 2)
        );
    }

    #[test]
    fn displayable_hold_requires_runtime_hold_data() {
        assert!(!note_has_displayable_hold(&test_note(
            0,
            48,
            NoteType::Hold
        )));
        assert!(note_has_displayable_hold(&test_hold(0, 48, 96)));
    }

    #[test]
    fn closest_lane_note_keeps_nearer_lift_visible_to_press_edges() {
        let timing = test_timing(144);
        let notes = vec![
            test_note(0, 48, NoteType::Lift),
            test_note(0, 49, NoteType::Tap),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [
            song_time_ns_from_seconds(1.000),
            song_time_ns_from_seconds(1.012),
        ];
        let (start_idx, end_idx) = lane_note_window_bounds_ns(
            &note_indices,
            &note_times_ns,
            song_time_ns_from_seconds(0.9),
            song_time_ns_from_seconds(1.1),
        );
        let (note_index, _) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            song_time_ns_from_seconds(1.004),
            48,
            start_idx,
            end_idx,
        )
        .expect("expected a closest note");

        assert_eq!(note_index, 0);
        assert!(!lane_edge_matches_note_type(
            true,
            notes[note_index].note_type
        ));
    }

    #[test]
    fn closest_lane_note_keeps_nearer_tap_visible_to_release_edges() {
        let timing = test_timing(144);
        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_note(0, 49, NoteType::Lift),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [
            song_time_ns_from_seconds(1.000),
            song_time_ns_from_seconds(1.012),
        ];
        let (start_idx, end_idx) = lane_note_window_bounds_ns(
            &note_indices,
            &note_times_ns,
            song_time_ns_from_seconds(0.9),
            song_time_ns_from_seconds(1.1),
        );
        let (note_index, _) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            song_time_ns_from_seconds(1.004),
            48,
            start_idx,
            end_idx,
        )
        .expect("expected a closest note");

        assert_eq!(note_index, 0);
        assert!(!lane_edge_matches_note_type(
            false,
            notes[note_index].note_type
        ));
    }

    #[test]
    fn closest_lane_note_keeps_explicit_fake_visible_to_press_edges() {
        let timing = test_timing(144);
        let mut fake = test_note(0, 48, NoteType::Tap);
        fake.is_fake = true;
        fake.can_be_judged = false;
        let notes = vec![fake, test_note(0, 49, NoteType::Tap)];
        let note_indices = [0usize, 1];
        let note_times_ns = [
            song_time_ns_from_seconds(1.000),
            song_time_ns_from_seconds(1.012),
        ];
        let (start_idx, end_idx) = lane_note_window_bounds_ns(
            &note_indices,
            &note_times_ns,
            song_time_ns_from_seconds(0.9),
            song_time_ns_from_seconds(1.1),
        );
        let (note_index, _) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            song_time_ns_from_seconds(1.004),
            48,
            start_idx,
            end_idx,
        )
        .expect("expected the explicit fake note to block the real tap");

        assert_eq!(note_index, 0);
        assert!(lane_edge_matches_note_type(
            true,
            notes[note_index].note_type
        ));
    }

    #[test]
    fn closest_lane_note_skips_taps_marked_fake_by_timing() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                fakes: vec![FakeSegment {
                    beat: 1.0,
                    length: 0.01,
                }],
                ..TimingSegments::default()
            },
            &test_row_to_beat(144),
        );
        let mut fake_segment_tap = test_note(0, 48, NoteType::Tap);
        fake_segment_tap.is_fake = true;
        fake_segment_tap.can_be_judged = false;
        let notes = vec![fake_segment_tap, test_note(0, 49, NoteType::Tap)];
        let note_indices = [0usize, 1];
        let note_times_ns = [
            song_time_ns_from_seconds(1.000),
            song_time_ns_from_seconds(1.012),
        ];
        let (start_idx, end_idx) = lane_note_window_bounds_ns(
            &note_indices,
            &note_times_ns,
            song_time_ns_from_seconds(0.9),
            song_time_ns_from_seconds(1.1),
        );
        let (note_index, _) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            song_time_ns_from_seconds(1.004),
            48,
            start_idx,
            end_idx,
        )
        .expect("expected the real tap to remain hittable");

        assert_eq!(note_index, 1);
    }

    #[test]
    fn closest_lane_note_breaks_exact_tie_toward_future_note() {
        let timing = test_timing(144);
        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_note(0, 50, NoteType::Tap),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [1_000_000_000_i64, 1_020_000_000_i64];
        let (start_idx, end_idx) = lane_note_window_bounds_ns(
            &note_indices,
            &note_times_ns,
            song_time_ns_from_seconds(0.9),
            song_time_ns_from_seconds(1.1),
        );
        let (note_index, abs_err_ns) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            1_010_000_000_i64,
            49,
            start_idx,
            end_idx,
        )
        .expect("expected an equidistant closest note");

        assert_eq!(note_index, 1);
        assert!((song_time_ns_to_seconds(abs_err_ns.abs()) - 0.010).abs() <= 1e-6);
    }

    #[test]
    fn closest_lane_note_prefers_nearer_row_over_nearer_time() {
        let timing = test_timing(144);
        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_note(0, 60, NoteType::Tap),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [
            song_time_ns_from_seconds(1.020),
            song_time_ns_from_seconds(1.028),
        ];
        let (start_idx, end_idx) = lane_note_window_bounds_ns(
            &note_indices,
            &note_times_ns,
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.1),
        );
        let (note_index, abs_err_ns) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            song_time_ns_from_seconds(1.030),
            50,
            start_idx,
            end_idx,
        )
        .expect("expected the nearer note in row to win");

        assert_eq!(note_index, 0);
        assert!((song_time_ns_to_seconds(abs_err_ns.abs()) - 0.010).abs() <= 1e-6);
    }

    #[test]
    fn closest_lane_note_keeps_out_of_window_nearer_row_blocker() {
        let timing = test_timing(144);
        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_note(0, 60, NoteType::Tap),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [
            song_time_ns_from_seconds(0.500),
            song_time_ns_from_seconds(1.010),
        ];
        let (start_idx, end_idx) = lane_note_window_bounds_rows(&note_indices, &notes, 0, 144);
        let (note_index, abs_err_ns) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            song_time_ns_from_seconds(1.000),
            50,
            start_idx,
            end_idx,
        )
        .expect("expected the nearer row to block the farther hittable row");

        assert_eq!(note_index, 0);
        assert!((song_time_ns_to_seconds(abs_err_ns.abs()) - 0.500).abs() <= 1e-6);
    }

    #[test]
    fn closest_lane_note_skips_already_judged_mines() {
        let timing = test_timing(144);
        let mut notes = vec![
            test_note(0, 48, NoteType::Mine),
            test_note(0, 60, NoteType::Tap),
        ];
        notes[0].mine_result = Some(MineResult::Hit);
        let note_indices = [0usize, 1];
        let note_times_ns = [
            song_time_ns_from_seconds(1.000),
            song_time_ns_from_seconds(1.120),
        ];
        let (start_idx, end_idx) = lane_note_window_bounds_ns(
            &note_indices,
            &note_times_ns,
            song_time_ns_from_seconds(0.9),
            song_time_ns_from_seconds(1.2),
        );
        let (note_index, _) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            song_time_ns_from_seconds(1.030),
            50,
            start_idx,
            end_idx,
        )
        .expect("expected the unjudged tap to remain hittable");

        assert_eq!(note_index, 1);
    }

    #[test]
    fn input_queue_cap_scales_with_fields() {
        assert_eq!(input_queue_cap(0), GAMEPLAY_INPUT_BACKLOG_WARN);
        assert_eq!(input_queue_cap(4), GAMEPLAY_INPUT_BACKLOG_WARN);
        assert_eq!(input_queue_cap(5), GAMEPLAY_INPUT_BACKLOG_WARN * 2);
        assert_eq!(input_queue_cap(8), GAMEPLAY_INPUT_BACKLOG_WARN * 2);
    }

    #[test]
    fn stage_music_cut_uses_negative_lead_in() {
        let cut = stage_music_cut(2.5);
        assert!((cut.start_sec + 2.5).abs() <= 1e-9);
        assert!(cut.length_sec.is_infinite());
        assert_eq!(cut.fade_in_sec, 0.0);
        assert_eq!(cut.fade_out_sec, 0.0);

        let clamped = stage_music_cut(-1.0);
        assert_eq!(clamped.start_sec, 0.0);
    }

    #[test]
    fn replay_edge_cap_scales_with_chart_and_skips_replay_mode() {
        assert_eq!(replay_edge_cap(4, 0, true, 120.0), 0);
        assert_eq!(replay_edge_cap(4, 0, false, 0.0), 4 * 64);
        assert_eq!(
            replay_edge_cap(4, 0, false, 2.0),
            4 * 2 * REPLAY_EDGE_RATE_PER_SEC
        );
        assert_eq!(
            replay_edge_cap(4, 120, false, 2.0),
            4 * 2 * REPLAY_EDGE_RATE_PER_SEC
        );
        assert_eq!(replay_edge_cap(4, 4000, false, 2.0), 8000);
        assert_eq!(
            replay_edge_cap(8, 1000, false, 1.0),
            8 * REPLAY_EDGE_RATE_PER_SEC
        );
    }

    #[test]
    fn step_calories_matches_itg_formula() {
        assert!((judgment::step_calories(120, 1) - 0.0266).abs() <= 1e-6);
        assert!((judgment::step_calories(120, 2) - 0.0882).abs() <= 1e-6);
        assert!((judgment::step_calories(120, 3) - 0.1498).abs() <= 1e-6);
    }

    #[test]
    fn recent_step_tracks_counts_current_press_inside_jump_window() {
        let mut pressed_since_ns = [None; MAX_COLS];
        pressed_since_ns[0] = Some(song_time_ns_from_seconds(10.0));
        pressed_since_ns[1] = Some(song_time_ns_from_seconds(9.9));
        pressed_since_ns[2] = Some(song_time_ns_from_seconds(9.74));
        pressed_since_ns[4] = Some(song_time_ns_from_seconds(10.0));

        assert_eq!(
            recent_step_tracks(&pressed_since_ns, 0, 4, song_time_ns_from_seconds(10.0)),
            2
        );
        assert_eq!(
            recent_step_tracks(&pressed_since_ns, 4, 8, song_time_ns_from_seconds(10.0)),
            1
        );
    }

    #[test]
    fn column_cues_ignore_notes_marked_fake_by_timing() {
        let mut fake_note = test_note(1, 96, NoteType::Tap);
        fake_note.is_fake = true;
        fake_note.can_be_judged = false;

        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            fake_note,
            test_note(2, 192, NoteType::Tap),
        ];
        let note_time_cache_ns = [1_000_000_000_i64, 2_000_000_000, 4_000_000_000];

        let cues =
            build_column_cues_for_player(&notes, (0, notes.len()), &note_time_cache_ns, 0, 4, 0.0);

        assert_eq!(cues.len(), 2);
        assert!((cues[0].start_time - 0.0).abs() <= 1e-6);
        assert!((cues[0].duration - 1.0).abs() <= 1e-6);
        assert_eq!(cues[0].columns.len(), 1);
        assert_eq!(cues[0].columns[0].column, 0);
        assert!((cues[1].start_time - 1.0).abs() <= 1e-6);
        assert!((cues[1].duration - 3.0).abs() <= 1e-6);
        assert_eq!(cues[1].columns.len(), 1);
        assert_eq!(cues[1].columns[0].column, 2);
    }

    #[test]
    fn note_hit_eval_scales_windows_in_music_time_ns() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        state.music_rate = 1.5;
        state.player_judgment_timing = std::array::from_fn(|player| {
            build_player_judgment_timing(
                state.timing_profile,
                &state.player_profiles[player],
                state.music_rate,
            )
        });

        let note_time_ns = song_time_ns_from_seconds(2.0);
        let great_edge_ns = state.player_judgment_timing[0].profile_music_ns.windows_ns[2];
        let way_off_edge_ns = state.player_judgment_timing[0].profile_music_ns.windows_ns[4];

        let on_great_edge = note_hit_eval(&state, 0, note_time_ns, note_time_ns + great_edge_ns)
            .expect("great edge should still judge");
        assert_eq!(on_great_edge.grade, JudgeGrade::Great);
        assert_eq!(on_great_edge.window, TimingWindow::W3);

        assert!(
            note_hit_eval(&state, 0, note_time_ns, note_time_ns + way_off_edge_ns + 1).is_none(),
            "offsets beyond the scaled way-off edge should miss",
        );
    }

    #[test]
    fn note_hit_eval_matches_tap_and_lift_zero_offsets() {
        let state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        let tap_time_ns = song_time_ns_from_seconds(1.0);
        let lift_time_ns = song_time_ns_from_seconds(2.0);

        let tap_hit =
            note_hit_eval(&state, 0, tap_time_ns, tap_time_ns).expect("tap hit should judge");
        let lift_hit =
            note_hit_eval(&state, 0, lift_time_ns, lift_time_ns).expect("lift hit should judge");

        assert_eq!(tap_hit.grade, lift_hit.grade);
        assert_eq!(tap_hit.window, lift_hit.window);
        assert_eq!(tap_hit.measured_offset_music_ns, 0);
        assert_eq!(lift_hit.measured_offset_music_ns, 0);
    }

    #[test]
    fn set_music_rate_rebuilds_judgment_and_end_times() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        let baseline_great_ns = state.player_judgment_timing[0].profile_music_ns.windows_ns[2];
        let baseline_notes_end = state.notes_end_time_ns;
        let baseline_music_end = state.music_end_time_ns;

        assert!(super::set_music_rate(&mut state, 1.5));
        assert!((state.music_rate - 1.5).abs() < 1e-6);

        let scaled_great_ns = state.player_judgment_timing[0].profile_music_ns.windows_ns[2];
        // Scaled timing windows are larger in music time when the rate is faster.
        assert!(
            scaled_great_ns > baseline_great_ns,
            "music-rate=1.5 should widen the W3 window in song-time ns ({} vs {})",
            scaled_great_ns,
            baseline_great_ns,
        );
        assert!(
            state.notes_end_time_ns > baseline_notes_end,
            "music-rate=1.5 should also widen the late-resolution slack on the note end time \
             ({} vs {})",
            state.notes_end_time_ns,
            baseline_notes_end,
        );
        assert_eq!(state.music_end_time_ns, baseline_music_end);

        // Calling with the same rate is a no-op.
        assert!(!super::set_music_rate(&mut state, 1.5));

        // Non-finite or non-positive inputs are normalized to 1.0.
        assert!(super::set_music_rate(&mut state, f32::NAN));
        assert!((state.music_rate - 1.0).abs() < 1e-6);

        assert!(super::set_music_rate(&mut state, 1.5));
        assert!(super::set_music_rate(&mut state, -2.0));
        assert!((state.music_rate - 1.0).abs() < 1e-6);
    }

    fn xover_anno(
        beat: f32,
        note_count: u8,
        column_mask: u8,
        is_crossover: bool,
    ) -> super::CrossoverRow {
        debug_assert_eq!(
            u32::from(note_count),
            column_mask.count_ones(),
            "xover_anno note_count must equal the number of set columns",
        );
        super::CrossoverRow {
            beat,
            column_mask,
            crossover: is_crossover,
            bracket: note_count > 1,
        }
    }

    // 120 BPM: beat b occurs at b * 0.5 seconds.
    fn xover_time(beat: f32) -> f32 {
        beat * 0.5
    }

    #[test]
    fn crossover_arrow_col_picks_outer_and_inner_panels() {
        // Single-pad panels: col0/col3 outer, col1/col2 inner.
        assert_eq!(super::crossover_arrow_col(0b0001, true), Some(0));
        assert_eq!(super::crossover_arrow_col(0b0001, false), None);
        assert_eq!(super::crossover_arrow_col(0b1000, true), Some(3));
        assert_eq!(super::crossover_arrow_col(0b0010, false), Some(1));
        assert_eq!(super::crossover_arrow_col(0b0010, true), None);
        assert_eq!(super::crossover_arrow_col(0b0100, false), Some(2));
        // Lowest matching lane wins when several are set.
        assert_eq!(super::crossover_arrow_col(0b1001, true), Some(0));
        assert_eq!(super::crossover_arrow_col(0b0110, false), Some(1));
        // Second pad of doubles repeats the pattern (col4 outer, col5 inner).
        assert_eq!(super::crossover_arrow_col(1 << 4, true), Some(4));
        assert_eq!(super::crossover_arrow_col(1 << 5, false), Some(5));
    }

    #[test]
    fn crossover_cue_single_event() {
        // prev = inner pivot tap, current = outer crossover, closely spaced.
        let annos = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
        ];
        let cues = super::build_crossover_cues_core(&annos, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 1);
        let cue = &cues[0];
        assert!((cue.start_time - (-0.5)).abs() < 1e-4, "start {}", cue.start_time);
        assert!((cue.duration - 0.575).abs() < 1e-4, "dur {}", cue.duration);
        assert_eq!(cue.columns.len(), 2);
        assert_eq!(cue.columns[0].column, 0);
        assert!(!cue.columns[0].is_mine);
        assert_eq!(cue.columns[1].column, 1);
        assert!(!cue.columns[1].is_mine);
    }

    #[test]
    fn crossover_cue_scooby_appends_red_column() {
        let annos = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
            xover_anno(1.0, 1, 0b1000, true),
        ];
        let cues = super::build_crossover_cues_core(&annos, xover_time, 0, 500, 8, false, 0.0);
        // Only one cue: the second crossover is consumed as the scooby.
        assert_eq!(cues.len(), 1);
        let cue = &cues[0];
        assert_eq!(cue.columns.len(), 3);
        assert_eq!(cue.columns[2].column, 3);
        assert!(cue.columns[2].is_mine, "scooby column flagged red");
    }

    #[test]
    fn crossover_cue_quantization_threshold_skips_isolated() {
        // Lone crossover far from any neighbour and no following note: none of
        // the spacing conditions hold, so no cue is produced.
        let annos = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(5.0, 1, 0b0001, true),
        ];
        let cues = super::build_crossover_cues_core(&annos, xover_time, 0, 500, 8, false, 0.0);
        assert!(cues.is_empty());
    }

    #[test]
    fn crossover_cue_extends_across_large_gap() {
        // Large gap before the crossover (first condition false) but a close
        // following note (second condition true): the cue stretches to cover
        // the gap.
        let annos = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(2.0, 1, 0b0001, true),
            xover_anno(2.4, 1, 0b0010, false),
        ];
        let cues = super::build_crossover_cues_core(&annos, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 1);
        let cue = &cues[0];
        // base 0.575 + (cur_arrow 1.0 - prev_arrow 0.0) = 1.575
        assert!((cue.duration - 1.575).abs() < 1e-4, "dur {}", cue.duration);
        assert!((cue.start_time - (-0.5)).abs() < 1e-4, "start {}", cue.start_time);
    }

    #[test]
    fn crossover_cue_clamps_overlapping_consecutive() {
        let annos = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
            xover_anno(0.6, 1, 0b0100, false),
            xover_anno(0.7, 1, 0b1000, true),
        ];
        let cues = super::build_crossover_cues_core(&annos, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 2);
        // First cue ends at -0.5 + 0.575 = 0.075; the second would start at
        // -0.2, so it is clamped to start at prev_end - fade = 0.0 with its
        // duration shortened accordingly.
        assert!((cues[1].start_time - 0.0).abs() < 1e-4, "start {}", cues[1].start_time);
        assert!((cues[1].duration - 0.375).abs() < 1e-4, "dur {}", cues[1].duration);
    }

    #[test]
    fn crossover_cue_negative_first_visible_adjust() {
        let annos = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
        ];
        let cues = super::build_crossover_cues_core(&annos, xover_time, 0, 500, 8, false, -0.3);
        assert_eq!(cues.len(), 1);
        let cue = &cues[0];
        // First cue genuinely starts before t=0 (start_time -0.5), so the
        // pre-roll compensation applies: start += fvt, duration -= fvt (fvt
        // negative => earlier start, longer duration).
        assert!((cue.start_time - (-0.8)).abs() < 1e-4, "start {}", cue.start_time);
        assert!((cue.duration - 0.875).abs() < 1e-4, "dur {}", cue.duration);
    }

    #[test]
    fn crossover_cue_positive_first_start_not_adjusted() {
        // First crossover occurs well into the song: prev arrow at beat 2.0
        // (t=1.0s) minus the 500ms lead gives a positive start_time of 0.5s.
        // Even with a negative first-visible (pre-roll) time, the cue must NOT
        // be shifted earlier or have its duration inflated — only cues already
        // due before t=0 get the pre-roll compensation. Regression for a
        // spurious, over-long first crossover cue.
        let annos = [
            xover_anno(2.0, 1, 0b0010, false),
            xover_anno(2.5, 1, 0b0001, true),
        ];
        let cues = super::build_crossover_cues_core(&annos, xover_time, 0, 500, 8, false, -0.3);
        assert_eq!(cues.len(), 1);
        let cue = &cues[0];
        assert!((cue.start_time - 0.5).abs() < 1e-4, "start {}", cue.start_time);
        assert!((cue.duration - 0.575).abs() < 1e-4, "dur {}", cue.duration);
    }

    #[test]
    fn crossover_cue_respects_col_start_offset() {
        let annos = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
        ];
        let cues = super::build_crossover_cues_core(&annos, xover_time, 4, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].columns[0].column, 4);
        assert_eq!(cues[0].columns[1].column, 5);
    }

    #[test]
    fn crossover_bracket_suppressed_when_excluded() {
        // A two-foot (bracket) crossover. With brackets excluded — the
        // default — it is not an active crossover, so no cue.
        let annos = [
            xover_anno(0.0, 1, 0b0100, false),
            xover_anno(0.5, 2, 0b0011, true),
        ];
        let cues = super::build_crossover_cues_core(&annos, xover_time, 0, 500, 8, false, 0.0);
        assert!(cues.is_empty(), "bracket crossover suppressed when excluded");
    }

    #[test]
    fn crossover_bracket_emitted_when_included() {
        // Same bracket crossover, but with brackets included it schedules a cue.
        let annos = [
            xover_anno(0.0, 1, 0b0100, false),
            xover_anno(0.5, 2, 0b0011, true),
        ];
        let cues = super::build_crossover_cues_core(&annos, xover_time, 0, 500, 8, true, 0.0);
        assert_eq!(cues.len(), 1, "bracket crossover emitted when included");
    }

    #[test]
    fn crossover_bracket_scooby_respects_toggle() {
        // A single-foot crossover followed by a bracket crossover. The bracket
        // only chains as a scooby when brackets are included.
        let annos = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
            xover_anno(1.0, 2, 0b1100, true),
        ];

        // Excluded: the bracket is not an active crossover, so it is neither a
        // scooby nor its own cue — one plain cue with no appended red column.
        let excluded = super::build_crossover_cues_core(&annos, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(excluded.len(), 1);
        assert_eq!(excluded[0].columns.len(), 2, "no scooby when bracket excluded");

        // Included: the bracket chains as a scooby and is appended in red.
        let included = super::build_crossover_cues_core(&annos, xover_time, 0, 500, 8, true, 0.0);
        assert_eq!(included.len(), 1);
        assert_eq!(included[0].columns.len(), 3, "scooby appended when bracket included");
        assert!(included[0].columns[2].is_mine, "scooby column flagged red");
    }
}
