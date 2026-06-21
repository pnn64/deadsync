use deadsync_chart::song::sync_pref_offset;
use deadsync_chart::{ChartData, GameplayChartData, SongData, SyncPref};
use deadsync_core::input::{InputSource, Lane, MAX_COLS, MAX_PLAYERS};
use deadsync_core::note::NoteType;
pub(crate) use deadsync_core::song_time::{
    INVALID_SONG_TIME_NS, SongTimeNs, normalized_song_rate, song_time_ns_delta_seconds,
    song_time_ns_from_seconds, song_time_ns_invalid, song_time_ns_to_seconds,
};
use deadsync_core::timing::beat_to_note_row;
pub(crate) use deadsync_gameplay::song_lua_ease_factor;
pub use deadsync_gameplay::{
    ASSIST_TICK_LOOKAHEAD_MARGIN_SECONDS, AUTOSYNC_OFFSET_SAMPLE_COUNT,
    AUTOSYNC_STDDEV_MAX_SECONDS, AccelEffects, AccelOverrides, ActiveColumnFlash,
    ActiveComboMilestone, ActiveHold, ActiveHoldAdvance, ActiveHoldResolution, ActiveInputSlot,
    ActiveMineExplosion, ActiveTapExplosion, AppearanceEffects, AppearanceOverrides,
    AutoplayNoteAction, AutoplayNoteEvent, AutoplayRowEventsUpdate,
    AutosyncMode, COLUMN_FLASH_JUDGMENT_DURATION, COLUMN_FLASH_MISS_DURATION,
    COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO, COMBO_HUNDRED_MILESTONE_DURATION,
    COMBO_THOUSAND_MILESTONE_DURATION, CROSSOVER_CUE_FADE_SECONDS, ChartAttackEffects,
    ColumnCue, ColumnCueColumn, ColumnFlashOptions, ColumnScrollFlags,
    ColumnTapJudgment, ComboMilestoneKind, CourseDisplayCarry, CourseDisplayTiming,
    CourseDisplayTotals, CrossoverRow, DRAW_DISTANCE_AFTER_TARGETS,
    DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER, DensityGraphWindow, DisplayClockDiagEvent,
    DisplayClockHealth, DisplayWindowCountsSources, EMPTY_ACTIVE_INPUT_SLOT,
    EarlyRescoreHitDecision, ErrorBarText, ErrorBarTick, ExitTransition, ExitTransitionKind,
    FantasticFeedbackOptions, FantasticWindowOptions, FinalNoteResultUpdate,
    FinalizedRowOutcome, GAMEPLAY_INPUT_BACKLOG_WARN, GAMEPLAY_INPUT_LATENCY_WARN_US,
    GIVE_UP_ABORT_TEXT_SECONDS, GameplayAction, GameplayAssistClapState, GameplayAttackMode,
    GameplayAttackRuntimeState, GameplayAudioClockState, GameplayAudioCommand,
    GameplayAudioSnapshot, GameplayAutoplayRuntimeState, GameplayAutosyncRuntimeState,
    GameplayBeatPhaseState, GameplayBoundaryRuntimeState, GameplayCapacityTraceEvent,
    GameplayCapacityTraceKind, GameplayCapacityTraceSnapshot, GameplayChartRuntimeState,
    GameplayChartTotalsState, GameplayClockRuntimeState, GameplayCommandQueue, GameplayConfig,
    GameplayControlRuntimeState, GameplayCourseDisplayState, GameplayCueRuntimeState,
    GameplayDangerFxState, GameplayDensityGraphState, GameplayDensityGraphView,
    GameplayDisplayClockState, GameplayDisplayRuntimeState, GameplayEndTimingState, GameplayExit,
    GameplayExitInputState, GameplayExitPromptState, GameplayFailType, GameplayHoldFeedbackState,
    GameplayHoldRuntimeState, GameplayInputLatencySample, GameplayInputLatencyTrace,
    GameplayInputPlayStyle, GameplayInputPlayerSide, GameplayInputState, GameplayLaneIndexState,
    gameplay_player_side_from_profile, gameplay_play_style_from_profile,
    gameplay_tick_mode_from_profile,
    GameplayLifeDeltaUpdate, GameplayMenuInput, GameplayMenuInputPlan, GameplayMineScanState,
    GameplayMiniIndicatorData, GameplayMiniIndicatorMode, GameplayMiniIndicatorOptions,
    GameplayMiniIndicatorRuntimeState, GameplayMusicCut, GameplayMusicRateState,
    GameplayNoteCountStatsState, GameplayNoteRangeState, GameplayNotefieldMotionState,
    GameplayNoteskinData, GameplayNoteskinEffects, GameplayOffsetAdjustHoldState,
    GameplayOffsetAdjustKey, GameplayOffsetAdjustTarget, GameplayOffsetState,
    GameplayPendingInputState, GameplayPlayersRuntimeState, GameplayProfilesRuntimeState,
    GameplayProgressRuntimeState, GameplayRawKeyInput, GameplayRawModifierKey,
    GameplayReceptorFeedbackState, GameplayReceptorGlowBehavior, GameplayReceptorGlowTimers,
    GameplayReceptorStepBehavior, GameplayReplayInputState, GameplayReplayRuntimeState,
    GameplayRowIndexState, GameplayRuntimeState, GameplayScoreDisplayMode, GameplaySession,
    GameplaySessionCommand, GameplaySetupRuntimeState, GameplaySongPositionState,
    GameplaySourceRuntimeState, GameplayStageRuntimeState, GameplayStreamClockSnapshot,
    GameplayTimingRuntimeState, GameplayTimingTickMode, GameplayToggleFlashState,
    GameplayTurnOption, GameplayTween, GameplayUpdatePhaseTimings, GameplayUpdateTraceState,
    GameplayUpdateTraceSummary, GameplayViewport, GameplayVisibleTimingState,
    GameplayVisualFeedbackState, GameplayWindowCountsState, HELD_MISS_TOTAL_DURATION,
    HOLD_JUDGMENT_TOTAL_DURATION, HOLDS_MASK_BIT_FLOORED, HOLDS_MASK_BIT_HOLDS_TO_ROLLS,
    HOLDS_MASK_BIT_NO_ROLLS, HOLDS_MASK_BIT_PLANTED, HOLDS_MASK_BIT_TWISTER, HealthState,
    HeldMissRenderInfo, HitActiveHoldStart, HoldJudgmentRenderInfo, HoldResolutionPlayerState,
    HoldResolutionPlayerUpdate, HoldResultStatsState, HoldToExitKey, INITIAL_HOLD_LIFE,
    INSERT_MASK_BIT_BIG, INSERT_MASK_BIT_BMRIZE, INSERT_MASK_BIT_ECHO, INSERT_MASK_BIT_MINES,
    INSERT_MASK_BIT_QUICK, INSERT_MASK_BIT_SKIPPY, INSERT_MASK_BIT_STOMP, INSERT_MASK_BIT_WIDE,
    JudgmentRenderInfo, LaneInputUpdate, LeadInTiming, M_MOD_HIGH_CAP, MAX_ACTIVE_INPUT_SLOTS,
    MINE_EXPLOSION_DURATION, MINI_PERCENT_MAX, MINI_PERCENT_MIN, MineHitPlayerState,
    MineHitPlayerUpdate, MineHitSideEffectPlan, MineJudgmentRenderInfo, MiniAttackMode,
    NoteCountStat, NoteHitEval, OFFSET_ADJUST_REPEAT_DELAY, OFFSET_ADJUST_REPEAT_INTERVAL,
    OFFSET_ADJUST_STEP_SECONDS, OffsetIndicatorText, PendingMissedHoldResolution,
    PerspectiveEffects, PerspectiveOverrides, PlayerJudgmentTiming, PlayerRowScanState,
    PlayerRuntime, PracticePlayerCursors, ProvisionalEarlyHitPlan,
    ProvisionalEarlyNoteResultUpdate, RECEPTOR_GLOW_DURATION, RECEPTOR_STEP_WINDOWS,
    RECEPTOR_Y_OFFSET_FROM_CENTER, RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE, REMOVE_MASK_BIT_LITTLE,
    REMOVE_MASK_BIT_NO_FAKES, REMOVE_MASK_BIT_NO_HANDS, REMOVE_MASK_BIT_NO_HOLDS,
    REMOVE_MASK_BIT_NO_JUMPS, REMOVE_MASK_BIT_NO_LIFTS, REMOVE_MASK_BIT_NO_MINES,
    REMOVE_MASK_BIT_NO_QUADS, REPLAY_EDGE_RATE_PER_SEC, RawKeyAction, RecordedLaneEdge,
    ReplayInputEdge, ReplayOffsetSnapshot, RowEntry, RowFinalizationPlan,
    RowFinalizationPlayerState, RowFinalizationPlayerUpdate, RowGrid, SPACING_PERCENT_MAX,
    SPACING_PERCENT_MIN, ScrollEffects, ScrollOverrides, ScrollReverseOptions, SongClockSnapshot,
    SongLuaColumnOffsetWindowRuntime, SongLuaNoteHideWindowRuntime, SongLuaOverlayMessageRuntime,
    TAP_EXPLOSION_WINDOWS, TOGGLE_FLASH_DURATION, TOGGLE_FLASH_FADE_START, TapExplosionOptions,
    TurnRng, VisibilityEffects, VisibilityOverrides,
    VisualEffects, VisualOverrides, active_hold_counts_as_pressed, active_hold_is_engaged,
    active_input_slot_lane_is_down, add_elapsed_us, add_player_mines_avoided,
    add_player_step_calories, advance_active_hold_to_time, advance_judged_row_cursor_for_entries,
    apply_combo_update, apply_echo_insert, apply_final_note_result,
    apply_final_note_result_to_rows, apply_gameplay_life_delta, apply_hold_let_go_player_state,
    apply_hold_let_go_update, apply_hold_resolution_player_state, apply_hold_success_player_state,
    apply_hold_success_update, apply_hyper_shuffle, apply_insert_intelligent_taps,
    apply_mine_hit_player_state, apply_mine_hit_player_update, apply_mines_insert,
    apply_life_change, apply_next_time_based_tap_miss_for_player,
    apply_provisional_early_note_result, apply_row_finalization_player_state, apply_stomp_insert,
    apply_super_shuffle_taps, apply_time_based_mine_avoidance_for_players, apply_turn_options,
    apply_turn_permutation, apply_uncommon_chart_transforms, apply_uncommon_masks_with_masks,
    apply_wide_insert, approach_attack_mini_percent_to_target, approach_attack_value, approach_f32,
    assist_clap_cursor_for_row, assist_clap_music_seconds_for_row, assist_clap_schedule_update,
    assist_lookahead_future_row, attack_mini_target_percent, autoplay_cursor_for_enable,
    autoplay_due_active_hold_resolution, autoplay_judgment_offset_music_ns,
    autoplay_random_offset_music_ns_for_window, autosync_mean_ns, autosync_mode_status_line,
    autosync_row_hits_enabled, autosync_stddev_seconds, blue_fantastic_window_ms,
    build_assist_clap_rows, build_column_cues_for_player,
    build_note_count_stats,
    build_player_judgment_timing, build_replay_input_edges, build_row_entry,
    build_row_grids, carried_holds_down_at_row, cell_has_any_note, cell_has_nonfake_note,
    clear_offset_adjust_hold_state, closest_lane_note_search, collect_active_autoplay_roll_columns,
    collect_autosync_row_hit_offsets, collect_due_autoplay_active_hold_resolutions,
    collect_edge_judge_indices, collect_next_autoplay_row_events, collect_pending_mine_hit_events,
    collect_pending_missed_hold_resolutions, collect_ready_judged_row_events,
    collect_time_based_tap_misses_for_players, column_cue_is_mine, column_flash_duration,
    column_flash_expired_at, column_scroll_dirs_for_flags,
    combo_milestone_duration, completed_mine_can_be_avoided, completed_row_final_judgment,
    completed_row_flash_note_indices_and_judgment, completed_row_hides_note,
    completed_row_tap_feedback_plan, compute_end_times_ns, convert_tap_row_to_mines,
    convert_taps_to_holds, count_held_tracks_at_row, count_nonempty_tracks_at_row,
    count_rescore_tracks_on_row, count_tap_or_hold_tracks_at_row, count_tap_tracks_at_row,
    counts_for_early_rescore, course_display_carry_for_player, course_display_carry_for_stage,
    course_display_carry_for_stages, course_display_totals_for_chart, crossed_mine_held_start_time,
    crossover_arrow_col, current_song_clock_snapshot, decay_let_go_hold_life_for_indices,
    density_graph_life_catch_up_steps, density_graph_life_sample_x, density_graph_u0_for_time,
    display_window_counts_current, display_window_counts_mode, draw_distance_after_targets,
    draw_distance_before_targets, early_rescore_hit_decision, effective_mini_percent,
    elapsed_us_since, enforce_max_simultaneous_notes, error_bar_average_offset_s,
    error_bar_long_term_offset_s, error_bar_push_tick, error_bar_window_ix, exit_total_seconds,
    exit_transition_alpha, fantastic_window_seconds, final_note_hit_judgment, final_note_hit_plan,
    final_note_result_effects, finalize_completed_mine_avoidance_for_players,
    finalized_row_awards_hand, finalized_row_judgment_for_entry,
    finalized_row_outcome_for_cached_row, finalized_row_outcome_for_entry,
    first_nonempty_track_at_row, first_row_entry_index_at_or_after_time, first_tap_track_at_row,
    first_time_index_at_or_after, gameplay_exit_for_kind, gameplay_input_latency_sample,
    gameplay_is_single_p2_side, gameplay_menu_input_plan, gameplay_player_side_for_index,
    gameplay_player_side_index, gameplay_runtime_player_is_p2,
    gameplay_runtime_player_side, gameplay_update_hot_phase, grade_to_window,
    frame_stable_display_music_time_ns,
    held_miss_judgment_expired_at, held_miss_render_info, hit_active_hold_start,
    hold_explosion_active, hold_explosion_enabled_for_options, hold_head_render_flags,
    hold_judgment_expired_at, hold_judgment_render_info, hold_resolution_player_state,
    hold_resolution_updates_grade_totals, hold_result_stats_update, hold_to_exit_seconds,
    init_player_runtime, init_player_runtime_for_song, input_lane_bit, input_queue_cap,
    integrate_active_hold_column, is_hold_body_at_row, judged_row_lookahead_time_ns,
    judgment_render_info, lane_edge_judges_lift, lane_edge_judges_tap, lane_edge_matches_note_type,
    lane_press_started, lane_release_finished, late_note_resolution_window_ns, let_go_head_beat,
    live_input_lane_for_queue, local_column_for_field, local_player_col,
    mark_crossed_held_mine_candidates, mark_mine_hit_candidate, mark_row_entry_note_finalized,
    mark_row_entry_provisional_early_result, max_step_distance_ns,
    measure_counter_segments_for_densities, mine_can_be_avoided, mine_can_be_hit,
    mine_hit_offset_in_window, mine_hit_player_state, mine_hit_side_effect_plan,
    mine_judgment_render_info, mine_window_bounds_ns,
    missed_note_cutoff_rows_for_players, music_time_from_stream_position, next_autosync_mode,
    next_ready_row_in_lookahead, next_timing_tick_mode, note_has_displayable_hold,
    note_hit_judgment, note_tracks_held_miss, notes_row_sorted,
    offset_adjust_delta_for_key, offset_adjust_repeat_ready, offset_adjust_slot_for_key,
    offset_adjust_target, player_chart_changes_for_options, player_column_range,
    player_combo_state, player_course_display_stage,
    player_health_state, player_index_for_column, player_life, player_mines_hit,
    player_note_range_for_ranges, player_row_scan_state, player_rows, player_runtime_is_dead,
    practice_player_cursors, provisional_early_hit_plan, push_density_life_point,
    quantization_index_from_beat, queue_pending_missed_hold_resolution, recent_step_calories,
    profile_side_from_gameplay, profile_tick_mode_from_gameplay, recent_step_tracks,
    receptor_glow_visual, record_player_live_timing_stats, record_unmapped_input_clock_warning,
    reference_bpm_from_display_tag, refresh_roll_life_for_active_column,
    refresh_roll_life_for_step, register_provisional_early_note_result, remap_live_input_lane,
    remove_cell_notes, replay_edge_cap, row_entry_for_cached_row, row_entry_index_for_cached_row,
    row_final_grade_hides_note, row_finalization_plan, row_finalization_plan_for_entry,
    row_finalization_player_state, saturating_elapsed_us_between, score_rows_finalized_for_players,
    scroll_receptor_y, scroll_reverse_percent_for_column, scroll_reverse_scale_for_column,
    set_added_mine_note, set_added_tap_note, set_player_last_judgment,
    set_player_last_mine_judgment, set_player_mines_avoided, set_row_finalization_player_state,
    settle_replaced_active_hold_column, song_audio_end_time_ns,
    music_time_ns_from_song_clock, song_lua_field_note_hidden, song_lua_note_hidden,
    song_lua_player_transforms_default, sort_player_notes, spacing_multiplier_for_percent,
    stage_music_cut, start_active_hold_column, start_offset_adjust_hold_state, stomp_mirror_track,
    suppress_final_bad_rescore_visual,
    tick_mine_explosion_slot, tick_offset_adjust_hold_state, tick_player_combo_milestones,
    tick_tap_explosion_slot,
    timing_row_floor, toggle_flash_alpha,
    track_held_miss_windows_for_players, track_range_has_any_note, trigger_combo_milestone,
    turn_seed_for_song, update_active_hold_columns, update_active_input_slot,
    update_itg_grade_totals, visible_notefield_time_ns, write_player_combo_state,
    zmod_stream_totals_for_densities,
};
use deadsync_gameplay::{
    AttackMaskWindow, base_appearance_effects,
    apply_chart_attacks_transforms, build_crossover_cues_for_player,
    chart_effects_from_profile, gameplay_attack_mode,
    gameplay_turn_option_from_profile, effective_mini_percent_for_player,
    effective_mini_value_with_visual_mask,
    effective_perspective_effects_for_player, effective_scroll_effects_for_player,
    effective_visual_mask_for_player,
    player_draw_scale_for_tilt_with_visual_mask, refresh_active_attack_masks,
    needs_stream_data, mini_indicator_mode, player_changes_chart, build_attack_mask_windows_for_player,
    gameplay_target_score_setting, score_invalid_reason_lines_for_chart, resolve_target_score_percent,
    gameplay_input_log_enabled,
    log_tap_judge_candidate, log_timing_hit_detail, song_lua_hides_note_visual,
    step_stats_play_style,
    step_stats_density_graph_width as gameplay_step_stats_density_graph_width,
    step_stats_upper_density_graph_width, timing_hit_log_enabled,
};
#[cfg(test)]
use deadsync_gameplay::effective_visual_effects_for_player;
#[cfg(test)]
use deadsync_gameplay::{
    SongLuaEaseMaskTarget,
    song_lua_ease_window_value,
};
use deadsync_input::{
    INPUT_SLOT_INVALID, InputEdge, InputEvent, VirtualAction, lane_from_action, lane_from_column,
};
use deadsync_profile as profile_data;
#[cfg(test)]
use deadsync_gameplay::{
    build_song_lua_column_offset_windows_for_player, build_song_lua_constant_windows_for_player,
};
use deadsync_rules::judgment::{self, JudgeGrade, Judgment, TimingWindow};
use deadsync_rules::note::{HoldData, HoldResult, MineResult, Note, recompute_player_totals};
#[cfg(test)]
use deadsync_rules::note::{MAX_HOLD_LIFE, TIMING_WINDOW_SECONDS_HOLD, TIMING_WINDOW_SECONDS_ROLL};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::stream::{StreamSegment, measure_densities};
use deadsync_rules::timing::{BeatInfoCache, TimingData, TimingProfile};
use log::{debug, info, trace, warn};
use std::sync::Arc;
use std::time::Instant;
use winit::keyboard::KeyCode;

pub(crate) type SongLuaOverlayEaseWindowRuntime =
    deadsync_gameplay::SongLuaOverlayEaseWindowRuntime<
        deadsync_song_lua::SongLuaOverlayStateDelta,
    >;

pub(crate) type GameplayModRuntimeState = deadsync_gameplay::GameplayModRuntimeState<
    crate::game::parsing::song_lua::SongLuaOverlayActor,
    deadsync_song_lua::SongLuaCapturedActor,
    deadsync_song_lua::SongLuaOverlayStateDelta,
>;

pub(crate) type CompiledSongLua =
    deadsync_song_lua::CompiledSongLua<crate::game::parsing::song_lua::SongLuaOverlayActor>;

pub(crate) type GameplayCompiledSongLua =
    deadsync_gameplay::GameplayCompiledSongLua<CompiledSongLua>;

pub(crate) type GameplaySongLuaLayer =
    deadsync_gameplay::GameplaySongLuaLayer<CompiledSongLua>;

pub(crate) type GameplaySongLuaData = deadsync_gameplay::GameplaySongLuaData<CompiledSongLua>;
const ASSIST_TICK_SFX_PATH: &str = "assets/sounds/assist_tick.ogg";

pub type State = GameplayRuntimeState<
    profile_data::Profile,
    InputEdge,
    crate::game::parsing::song_lua::SongLuaOverlayActor,
    deadsync_song_lua::SongLuaCapturedActor,
    deadsync_song_lua::SongLuaOverlayStateDelta,
>;

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
        state.setup.num_players > 0 && state.setup.num_players <= MAX_PLAYERS,
        "invalid num_players={}",
        state.setup.num_players
    );
    debug_assert!(
        state.setup.num_cols > 0 && state.setup.num_cols <= MAX_COLS,
        "invalid num_cols={}",
        state.setup.num_cols
    );
    debug_assert!(
        state.setup.cols_per_player > 0 && state.setup.cols_per_player <= MAX_COLS,
        "invalid cols_per_player={}",
        state.setup.cols_per_player
    );
    debug_assert_eq!(
        state.chart_runtime.notes.len(),
        state.chart_runtime.note_time_cache_ns.len()
    );
    debug_assert_eq!(
        state.chart_runtime.notes.len(),
        state.chart_runtime.hold_end_time_cache_ns.len()
    );
    debug_assert_eq!(
        state.chart_runtime.notes.len(),
        state.hold_runtime.hold_decay_active.len()
    );
    debug_assert_eq!(
        state.chart_runtime.notes.len(),
        state.chart_runtime.row_indices.note_row_entry_indices.len()
    );
    for player in 0..state.setup.num_players {
        let (start, end) = state.chart_runtime.note_ranges.range(player);
        debug_assert!(start <= end && end <= state.chart_runtime.notes.len());
        let (row_start, row_end) = state.chart_runtime.row_indices.row_entry_ranges[player];
        debug_assert!(row_start <= row_end && row_end <= state.chart_runtime.row_entries.len());
        debug_assert!(
            state.chart_runtime.row_indices.judged_row_cursor[player] >= row_start
                && state.chart_runtime.row_indices.judged_row_cursor[player] <= row_end
        );
        debug_assert!(
            state.chart_runtime.mine_scan.next_tap_miss_cursor[player] >= start
                && state.chart_runtime.mine_scan.next_tap_miss_cursor[player] <= end
        );
        debug_assert!(
            state.chart_runtime.mine_scan.next_mine_avoid_cursor[player] >= start
                && state.chart_runtime.mine_scan.next_mine_avoid_cursor[player] <= end
        );
        debug_assert_eq!(
            state.chart_runtime.mine_scan.mine_note_ix[player].len(),
            state.chart_runtime.mine_scan.mine_note_time_ns[player].len()
        );
        debug_assert!(
            state.chart_runtime.mine_scan.next_mine_ix_cursor[player]
                <= state.chart_runtime.mine_scan.mine_note_ix[player].len()
        );
    }
    for player in 0..state.setup.num_players {
        let (start, end) = state.chart_runtime.note_ranges.range(player);
        debug_assert!(
            state.chart_runtime.mine_scan.mine_note_time_ns[player]
                .windows(2)
                .all(|pair| pair[0] <= pair[1])
        );
        for &note_index in &state.chart_runtime.mine_scan.mine_note_ix[player] {
            debug_assert!(note_index >= start && note_index < end);
            debug_assert!(matches!(
                state.chart_runtime.notes[note_index].note_type,
                NoteType::Mine
            ));
        }
    }
    for note in &state.chart_runtime.notes {
        if note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
            let player = state.player_for_col(note.column);
            debug_assert!(
                row_entry_for_cached_row(
                    &state.chart_runtime.row_entries,
                    &state.chart_runtime.row_indices.row_map_cache[player],
                    note.row_index
                )
                .is_some()
            );
        }
    }
    for (row_entry_index, row_entry) in state.chart_runtime.row_entries.iter().enumerate() {
        let first_note_index = row_entry.note_indices()[0];
        let player = state.player_for_col(state.chart_runtime.notes[first_note_index].column);
        debug_assert!(
            row_entry_index >= state.chart_runtime.row_indices.row_entry_ranges[player].0
                && row_entry_index < state.chart_runtime.row_indices.row_entry_ranges[player].1
        );
        debug_assert_eq!(
            state.chart_runtime.row_indices.row_map_cache[player]
                .get(row_entry.row_index)
                .copied(),
            Some(row_entry_index as u32)
        );
        for &note_index in row_entry.note_indices() {
            debug_assert!(note_index < state.chart_runtime.notes.len());
            debug_assert_eq!(
                state.chart_runtime.row_indices.note_row_entry_indices[note_index],
                row_entry_index as u32
            );
            let note = &state.chart_runtime.notes[note_index];
            debug_assert_eq!(note.row_index, row_entry.row_index);
            debug_assert!(note.can_be_judged);
            debug_assert!(!note.is_fake);
            debug_assert!(!matches!(note.note_type, NoteType::Mine));
        }
    }
    for col in 0..state.setup.num_cols {
        debug_assert!(
            state
                .display
                .notefield_motion
                .column_scroll_dir(col)
                .is_finite()
        );
        debug_assert!(
            state.chart_runtime.lane_indices.note_indices[col]
                .windows(2)
                .all(|pair| {
                    let left = pair[0];
                    let right = pair[1];
                    left < right
                        && state.chart_runtime.note_time_cache_ns[left]
                            <= state.chart_runtime.note_time_cache_ns[right]
                })
        );
        for &note_index in &state.chart_runtime.lane_indices.note_indices[col] {
            debug_assert!(note_index < state.chart_runtime.notes.len());
            debug_assert_eq!(state.chart_runtime.notes[note_index].column, col);
        }
        debug_assert_eq!(
            state.chart_runtime.lane_indices.note_row_indices[col].len(),
            state.chart_runtime.lane_indices.note_indices[col].len()
        );
        debug_assert!(
            state.chart_runtime.lane_indices.note_row_indices[col]
                .windows(2)
                .all(|pair| {
                    let left = pair[0];
                    let right = pair[1];
                    (beat_to_note_row(state.chart_runtime.notes[left].beat), left)
                        <= (
                            beat_to_note_row(state.chart_runtime.notes[right].beat),
                            right,
                        )
                })
        );
        for &note_index in &state.chart_runtime.lane_indices.note_row_indices[col] {
            debug_assert!(note_index < state.chart_runtime.notes.len());
            debug_assert_eq!(state.chart_runtime.notes[note_index].column, col);
        }
        debug_assert!(
            state.chart_runtime.lane_indices.hold_indices[col]
                .windows(2)
                .all(|pair| {
                    let left = pair[0];
                    let right = pair[1];
                    left < right
                        && state.chart_runtime.note_time_cache_ns[left]
                            <= state.chart_runtime.note_time_cache_ns[right]
                })
        );
        for &note_index in &state.chart_runtime.lane_indices.hold_indices[col] {
            debug_assert!(note_index < state.chart_runtime.notes.len());
            debug_assert_eq!(state.chart_runtime.notes[note_index].column, col);
            debug_assert!(matches!(
                state.chart_runtime.notes[note_index].note_type,
                NoteType::Hold | NoteType::Roll
            ));
        }
    }
    for col in state.setup.num_cols..MAX_COLS {
        debug_assert!(state.chart_runtime.lane_indices.note_indices[col].is_empty());
        debug_assert!(state.chart_runtime.lane_indices.note_row_indices[col].is_empty());
        debug_assert!(state.chart_runtime.lane_indices.hold_indices[col].is_empty());
    }
    let mut lane_positions = [0usize; MAX_COLS];
    for (note_index, note) in state.chart_runtime.notes.iter().enumerate() {
        if note.column >= state.setup.num_cols {
            continue;
        }
        let lane_pos = lane_positions[note.column];
        debug_assert_eq!(
            state.chart_runtime.lane_indices.note_indices[note.column]
                .get(lane_pos)
                .copied(),
            Some(note_index)
        );
        lane_positions[note.column] += 1;
    }
    for col in 0..state.setup.num_cols {
        debug_assert_eq!(
            lane_positions[col],
            state.chart_runtime.lane_indices.note_indices[col].len()
        );
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
    deadsync_gameplay::trace_gameplay_update(
        state,
        delta_time,
        music_time_sec,
        total_us,
        phase_timings,
    );
}

fn refresh_live_notefield_options(state: &mut State, current_bpm: f32) {
    for player in 0..state.setup.num_players {
        let scroll = effective_scroll_effects_for_player(state, player);
        state.display.notefield_motion.set_reverse_scroll(
            player,
            scroll.reverse_percent_for_column(0, state.setup.cols_per_player) > 0.5,
        );
        let start = player.saturating_mul(state.setup.cols_per_player);
        let end = (start + state.setup.cols_per_player)
            .min(state.setup.num_cols)
            .min(MAX_COLS);
        for (local_col, col) in (start..end).enumerate() {
            state.display.notefield_motion.set_column_scroll_dir(
                col,
                scroll.reverse_scale_for_column(local_col, state.setup.cols_per_player),
            );
        }
    }
    for player in 0..state.setup.num_players {
        let scroll_speed = state.effective_scroll_speed_for_player(player);
        let reference_bpm = state.display.notefield_motion.scroll_reference_bpm();
        let mut dynamic_speed =
            scroll_speed.pixels_per_second(current_bpm, reference_bpm, state.music_rate());
        if !dynamic_speed.is_finite() || dynamic_speed <= 0.0 {
            dynamic_speed = ScrollSpeedSetting::default().pixels_per_second(
                current_bpm,
                reference_bpm,
                state.music_rate(),
            );
        }
        let scroll = effective_scroll_effects_for_player(state, player);
        let visual_mask = effective_visual_mask_for_player(state, player);
        let mini_percent = effective_mini_percent_for_player(state, player);
        let mini = effective_mini_value_with_visual_mask(
            &state.profiles_runtime.profiles[player],
            visual_mask,
            mini_percent,
        );
        let mut field_zoom = 1.0 - mini * 0.5;
        if field_zoom.abs() < 0.01 {
            field_zoom = 0.01;
        }

        let perspective = effective_perspective_effects_for_player(state, player);
        let draw_scale = player_draw_scale_for_tilt_with_visual_mask(
            perspective.tilt,
            &state.profiles_runtime.profiles[player],
            visual_mask,
            mini_percent,
        );
        let draw_distance_before =
            draw_distance_before_targets(state.setup.viewport.height(), draw_scale);
        let draw_distance_after =
            draw_distance_after_targets(state.setup.viewport.height(), draw_scale, scroll.centered);

        let mut travel_time = scroll_speed.travel_time_seconds(
            draw_distance_before,
            current_bpm,
            reference_bpm,
            state.music_rate(),
        );
        if !travel_time.is_finite() || travel_time <= 0.0 {
            travel_time = draw_distance_before / dynamic_speed.max(f32::EPSILON);
        }
        state.display.notefield_motion.set_player_motion(
            player,
            dynamic_speed,
            field_zoom,
            draw_distance_before,
            draw_distance_after,
            travel_time,
        );
    }
}

fn update_density_graph(
    state: &mut State,
    current_music_time: f32,
    trace_enabled: bool,
    phase_timings: &mut GameplayUpdatePhaseTimings,
) {
    let graph_w = state.display.density_graph.graph_w;
    let graph_h = state.display.density_graph.graph_h;
    let scaled_width = state.display.density_graph.scaled_width;
    state.display.density_graph.u0 = density_graph_u0_for_time(
        DensityGraphWindow {
            first_second: state.display.density_graph.first_second,
            last_second: state.display.density_graph.last_second,
            duration: state.display.density_graph.duration,
            graph_w,
            graph_h,
            scaled_width,
            u_window: state.display.density_graph.u_window,
        },
        current_music_time,
    );
    if graph_w <= 0.0_f32 || graph_h <= 0.0_f32 || scaled_width <= 0.0_f32 {
        return;
    }

    let next_t = state.display.density_graph.life_next_update_elapsed;
    let catch_up_steps = density_graph_life_catch_up_steps(
        state.boundary.total_elapsed_in_screen,
        next_t,
        state.display.density_graph.life_update_rate,
    );
    if catch_up_steps > 0 {
        let sample_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        let rate = state.display.density_graph.life_update_rate;
        state.display.density_graph.life_next_update_elapsed += rate * catch_up_steps as f32;

        if let Some(x) = density_graph_life_sample_x(
            current_music_time,
            state.display.density_graph.first_second,
            state.display.density_graph.last_second,
            state.display.density_graph.duration,
            state.display.density_graph.scaled_width,
        ) {
            for player in 0..state.setup.num_players {
                let life = player_life(&state.players_runtime.players[player]);
                let y = (1.0_f32 - life).clamp(0.0_f32, 1.0_f32) * graph_h;
                let points = &mut state.display.density_graph.life_points[player];
                if push_density_life_point(points, x, y) {
                    state.display.density_graph.life_dirty[player] = true;
                }
            }
        }
        if let Some(started) = sample_started {
            add_elapsed_us(&mut phase_timings.density_sample_us, started);
        }
    }
}

fn refresh_seek_dependent_state(state: &mut State) {
    refresh_active_attack_masks(state, 0.0);
    let current_bpm = state
        .timing_runtime
        .timing
        .get_bpm_for_beat(state.clock.song_position.current_beat);
    refresh_live_notefield_options(state, current_bpm);
}

pub(crate) fn start_stage_music(state: &mut State) {
    let lead_in = state.clock.audio_clock.positive_lead_in_seconds();
    debug!("Starting music with a preroll delay of {lead_in:.2}s");
    state.start_stage_music();
    refresh_seek_dependent_state(state);
}

/// Updates the music rate on a live gameplay state, rebuilding the
/// rate-dependent caches (judgment timing windows and end-time markers) so
/// later judging and completion checks remain consistent. Returns `true` when
/// the rate actually changed.
///
/// This does not touch the audio engine or the session-stored rate; callers
/// (e.g. practice-mode hotkeys) are responsible for keeping `audio::set_music_rate`
/// and `profile::set_session_music_rate` in sync.
pub(crate) fn set_music_rate(state: &mut State, rate: f32) -> bool {
    let normalized = normalized_song_rate(rate);
    let timing_profile = state.timing_runtime.timing_profile;
    let player_judgment_timing = std::array::from_fn(|player| {
        build_player_judgment_timing(
            timing_profile,
            &state.profiles_runtime.profiles[player],
            normalized,
        )
    });
    state.set_music_rate_with_player_judgment_timing(rate, player_judgment_timing)
}

pub(crate) fn seek_practice_display(state: &mut State, music_time: f32) {
    state.seek_practice_display(music_time);
    refresh_seek_dependent_state(state);
}

#[inline(always)]
fn settle_due_autoplay_active_holds(state: &mut State, cutoff_time_ns: SongTimeNs) {
    let mut events = [None; MAX_COLS];
    let update = collect_due_autoplay_active_hold_resolutions(
        &mut state.hold_runtime.active_holds,
        state.setup.num_cols,
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
    if !state.stage_autoplay_enabled() {
        return;
    }

    for player in 0..state.setup.num_players {
        let note_range = state.note_range_for_player(player);
        let mut cursor = state.autoplay_cursor(player).max(note_range.0);
        loop {
            let mut events = [None; MAX_COLS];
            let update = collect_next_autoplay_row_events(
                &state.chart_runtime.notes,
                &state.chart_runtime.note_time_cache_ns,
                note_range,
                cursor,
                state.setup.num_cols,
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
                state.mark_autoplay_used();
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
        state.set_autoplay_cursor(player, cursor);
    }

    let mut roll_cols = [usize::MAX; MAX_COLS];
    let roll_count = collect_active_autoplay_roll_columns(
        &state.hold_runtime.active_holds,
        state.setup.num_cols,
        &mut roll_cols,
    );
    for col in roll_cols.into_iter().take(roll_count) {
        refresh_roll_life_on_step(state, col, state.clock.song_position.current_music_time_ns);
    }
}

pub(super) fn run_replay(state: &mut State) {
    if !state.stage_autoplay_enabled() || !state.progress.replay.mode {
        return;
    }
    let mut events = [None; MAX_COLS];
    let event_count = state.progress.replay.input.collect_ready(
        state.clock.song_position.current_music_time_ns,
        state.setup.num_cols,
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
        state.mark_autoplay_used();
    }
}

#[inline(always)]
fn update_raw_modifier_state(state: &mut State, code: KeyCode, pressed: bool) {
    match code {
        KeyCode::ShiftLeft | KeyCode::ShiftRight => {
            state.set_raw_modifier_key(GameplayRawModifierKey::Shift, pressed)
        }
        KeyCode::ControlLeft | KeyCode::ControlRight => {
            state.set_raw_modifier_key(GameplayRawModifierKey::Ctrl, pressed)
        }
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

pub(crate) fn handle_queued_raw_key(
    state: &mut State,
    code: KeyCode,
    pressed: bool,
    timestamp: Instant,
    now_music_time: f32,
    allow_commands: bool,
) -> RawKeyAction {
    update_raw_modifier_state(state, code, pressed);
    let plan = state.raw_key_plan(gameplay_raw_key_input(code), pressed, allow_commands);
    state.apply_raw_key_plan(plan, timestamp, now_music_time)
}

#[inline(always)]
fn gameplay_menu_input(action: VirtualAction) -> Option<GameplayMenuInput> {
    match action {
        VirtualAction::p1_start => Some(GameplayMenuInput::P1Start),
        VirtualAction::p2_start => Some(GameplayMenuInput::P2Start),
        VirtualAction::p1_back => Some(GameplayMenuInput::P1Back),
        VirtualAction::p2_back => Some(GameplayMenuInput::P2Back),
        _ => None,
    }
}

pub(super) fn update_lane_input_slot(
    state: &mut State,
    lane: Lane,
    source: InputSource,
    input_slot: u32,
    pressed: bool,
) -> LaneInputUpdate {
    let lane_idx = lane.index();
    let input_slot =
        state.normalized_input_slot_for_lane(lane_idx, input_slot, INPUT_SLOT_INVALID);
    let update = state.update_input_slot(lane_idx, source, input_slot, pressed);
    if update.slot_table_full {
        debug!(
            "Gameplay active input slot table full; dropping held-state edge for {:?} slot {}",
            source, input_slot
        );
    }
    update
}

fn queue_input_edge(
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
    let Some(lane) = live_input_lane_for_queue(
        state.progress.stage.autoplay_enabled,
        state.setup.session.play_style,
        state.setup.session.player_side,
        lane,
        state.setup.num_cols,
    ) else {
        return;
    };

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
        state.progress.replay.capture_enabled,
    );
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
#[allow(clippy::too_many_arguments)]
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
    if lane.index() >= state.num_cols() {
        return;
    }
    state.push_pending_input_edge(InputEdge {
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
        let pending_len = state.pending_input_len();
        if pending_len >= GAMEPLAY_INPUT_BACKLOG_WARN {
            debug!(
                "Gameplay input queue pressure: pending_edges={}, num_cols={}, music_time={:.3}",
                pending_len,
                state.num_cols(),
                state.current_music_time_seconds()
            );
        }
    }
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> GameplayAction {
    if state.exit_transition_active() {
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
    let p2_runtime_player = state.setup.session.p2_runtime_player();
    let p1_menu_active = state.setup.num_players > 1 || !p2_runtime_player;
    let p2_menu_active = state.setup.num_players > 1 || p2_runtime_player;
    if let Some(input) = gameplay_menu_input(ev.action) {
        let plan = gameplay_menu_input_plan(
            input,
            ev.pressed,
            p1_menu_active,
            p2_menu_active,
            state.setup.config.delayed_back,
            state.control.exit_input.hold_to_exit_key,
        );
        state.apply_menu_input_plan(plan, ev.timestamp);
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
    if state.pending_input_is_empty() {
        return;
    }

    let input_log = gameplay_input_log_enabled();
    while let Some(mut edge) = state.pop_pending_input_edge() {
        let lane_idx = edge.lane.index();
        if lane_idx >= state.setup.num_cols {
            if input_log {
                debug!(
                    "GAMEPLAY INPUT EDGE DROP: reason=lane_out_of_range lane={} num_cols={} source={:?} slot={} pressed={}",
                    lane_idx, state.setup.num_cols, edge.source, edge.input_slot, edge.pressed,
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
                    state.pending_input_len(),
                );
            }
            continue;
        }
        let lane_was_down = state.lane_is_pressed(lane_idx);
        let slot_was_down = state.input_slot_lane_is_down_normalized(
            lane_idx,
            edge.source,
            edge.input_slot,
            INPUT_SLOT_INVALID,
        );
        let edge_judges_tap = lane_edge_judges_tap(edge.pressed, slot_was_down);
        let edge_judges_lift = lane_edge_judges_lift(edge.pressed, slot_was_down);
        if resolved_from_song_clock
            && !song_clock.mapped_audio
            && record_unmapped_input_clock_warning(edge.event_music_time_ns)
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
                state.current_music_time_seconds(),
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
            let latency = gameplay_input_latency_sample(
                edge.captured_at,
                edge.stored_at,
                edge.emitted_at,
                edge.queued_at,
                processed_at,
            );
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
                state.current_music_time_seconds(),
                latency.capture_to_queue_us,
                latency.queue_to_process_us,
                latency.capture_to_process_us,
                state.pending_input_len(),
            );
        }
        if edge_judges_tap {
            refresh_roll_life_on_step(state, lane_idx, edge.event_music_time_ns);
        }
        integrate_active_hold_to_time(state, lane_idx, edge.event_music_time_ns);
        if edge.record_replay {
            state.push_recorded_replay_edge(RecordedLaneEdge {
                lane_index: lane_idx as u8,
                pressed: edge.pressed,
                source: edge.source,
                event_music_time_ns: edge.event_music_time_ns,
            });
        }
        if trace_enabled {
            let processed_at = Instant::now();
            let latency = gameplay_input_latency_sample(
                edge.captured_at,
                edge.stored_at,
                edge.emitted_at,
                edge.queued_at,
                processed_at,
            );
            state
                .control
                .update_trace
                .summary
                .record_input_latency(latency);
            if latency.capture_to_process_us >= GAMEPLAY_INPUT_LATENCY_WARN_US {
                debug!(
                    "Gameplay input latency spike: lane={} pressed={} source={:?} capture_store_us={} store_emit_us={} emit_queue_us={} queue_process_us={} capture_queue_us={} capture_process_us={} pending={} now_t={:.3} edge_t={:.3}",
                    lane_idx,
                    edge.pressed,
                    edge.source,
                    latency.capture_to_store_us,
                    latency.store_to_emit_us,
                    latency.emit_to_queue_us,
                    latency.queue_to_process_us,
                    latency.capture_to_queue_us,
                    latency.capture_to_process_us,
                    state.pending_input_len() + 1,
                    state.current_music_time_seconds(),
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
        state.sync_active_hold_pressed_state(lane_idx, lane_update.is_down);

        if press_started {
            state.press_input_lane(lane_idx, edge.event_music_time_ns);
            state.record_step_calories_for_lane(lane_idx, edge.event_music_time_ns);
            if trace_enabled {
                let started = Instant::now();
                state.start_receptor_glow_press(lane_idx);
                add_elapsed_us(&mut phase_timings.input_glow_us, started);
            } else {
                state.start_receptor_glow_press(lane_idx);
            }
        } else if release_finished {
            state.release_input_lane(lane_idx);
            if trace_enabled {
                let started = Instant::now();
                state.release_receptor_glow(lane_idx);
                add_elapsed_us(&mut phase_timings.input_glow_us, started);
            } else {
                state.release_receptor_glow(lane_idx);
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
                if state.tick_mode() == GameplayTimingTickMode::Hit {
                    state.push_audio_command(GameplayAudioCommand::PlayPreloadedAssistTick(
                        ASSIST_TICK_SFX_PATH,
                    ));
                }
            } else {
                state.trigger_receptor_step_pulse(lane_idx);
            }
        } else if edge_judges_lift {
            let hit_lift = judge_a_lift(state, lane_idx, edge.event_music_time_ns);
            if hit_lift && state.tick_mode() == GameplayTimingTickMode::Hit {
                state.push_audio_command(GameplayAudioCommand::PlayPreloadedAssistTick(
                    ASSIST_TICK_SFX_PATH,
                ));
            }
        }
    }
}

#[inline(always)]
pub(super) fn tick_visual_effects(state: &mut State, delta_time: f32) {
    let lane_counts = *state.lane_input_counts();
    state.display.receptor_feedback.tick(
        &state.display.noteskin_effects,
        state.setup.num_cols,
        state.setup.num_players,
        state.setup.cols_per_player,
        &lane_counts,
        delta_time,
    );
    state.display.toggle_flash.tick(delta_time);
    for player in 0..state.setup.num_players {
        tick_player_combo_milestones(&mut state.players_runtime.players[player], delta_time);
    }
    for slot in &mut state.display.visual_feedback.tap_explosions {
        tick_tap_explosion_slot(slot, delta_time);
    }
    for slot in &mut state.display.visual_feedback.mine_explosions {
        tick_mine_explosion_slot(slot, delta_time);
    }
    for slot in &mut state.display.visual_feedback.column_flashes {
        if let Some(active) = slot
            && column_flash_expired_at(*active, state.boundary.total_elapsed_in_screen)
        {
            *slot = None;
        }
    }
    for slot in &mut state.display.hold_feedback.hold_judgments {
        if let Some(render_info) = slot
            && hold_judgment_expired_at(*render_info, state.boundary.total_elapsed_in_screen)
        {
            *slot = None;
        }
    }
    for slot in &mut state.display.hold_feedback.held_miss_judgments {
        if let Some(render_info) = slot
            && held_miss_judgment_expired_at(*render_info, state.boundary.total_elapsed_in_screen)
        {
            *slot = None;
        }
    }
}

pub(crate) fn start_practice_music_at(
    state: &mut State,
    playback_music_time: f32,
    judge_start_music_time: f32,
) {
    state.start_practice_music_at(playback_music_time, judge_start_music_time);
    refresh_seek_dependent_state(state);
}

pub(crate) fn init(
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
    let rate = normalized_song_rate(music_rate);

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
        let visual_mask = player_profiles[player].visual_effects_active_mask.bits();
        let mini_value = effective_mini_value_with_visual_mask(
            &player_profiles[player],
            visual_mask,
            player_profiles[player].mini_percent as f32,
        );
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
    let replay_offsets = replay_offsets.unwrap_or(ReplayOffsetSnapshot {
        beat0_time_ns: timing_players[0].get_time_for_beat_ns(0.0),
    });
    let replay_beat0_times = std::array::from_fn(|player| {
        timing_players[player.min(MAX_PLAYERS - 1)].get_time_for_beat_ns(0.0)
    });
    let replay_input = build_replay_input_edges(
        &replay_edges,
        num_players,
        cols_per_player,
        num_cols,
        replay_offsets.beat0_time_ns,
        replay_beat0_times,
    );
    let replay_mode = !replay_input.is_empty();
    if replay_mode {
        debug!(
            "Gameplay replay mode enabled: {} recorded edges loaded.",
            replay_input.len(),
        );
    }
    let replay_input = GameplayReplayInputState::new(replay_input);
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
    let uncommon_effects = std::array::from_fn(|player| {
        if player < num_players {
            chart_effects_from_profile(&player_profiles[player])
        } else {
            ChartAttackEffects::default()
        }
    });
    let timing_player_refs: [&TimingData; MAX_PLAYERS] =
        std::array::from_fn(|player| timing_players[player].as_ref());
    apply_uncommon_chart_transforms(
        &mut notes,
        &mut note_ranges,
        cols_per_player,
        num_players,
        &uncommon_effects,
        &timing_player_refs,
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
    let player_attack_modes: [GameplayAttackMode; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player < num_players {
            gameplay_attack_mode(player_profiles[player].attack_mode)
        } else {
            GameplayAttackMode::Off
        }
    });
    apply_chart_attacks_transforms(
        &mut notes,
        &mut note_ranges,
        &gameplay_charts,
        cols_per_player,
        num_players,
        &player_attack_modes,
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

    let note_player_for_col =
        |col: usize| -> usize { player_index_for_column(num_players, cols_per_player, col) };

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

    let mut reference_bpm =
        reference_bpm_from_display_tag(charts[0].display_bpm.as_ref(), &song.display_bpm)
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
    let initial_draw_scale: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return 1.0;
        }
        let profile = &player_profiles[player];
        player_draw_scale_for_tilt_with_visual_mask(
            profile.perspective.tilt_skew().0,
            profile,
            profile.visual_effects_active_mask.bits(),
            0.0,
        )
    });
    let draw_distance_before_targets: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return draw_distance_before_targets(viewport.height(), 1.0);
        }
        draw_distance_before_targets(viewport.height(), initial_draw_scale[player])
    });
    let draw_distance_after_targets: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return draw_distance_after_targets(viewport.height(), 1.0, 0.0);
        }
        let centered_percent = if player_profiles[player]
            .scroll_option
            .contains(profile_data::ScrollOption::Centered)
        {
            1.0
        } else {
            0.0
        };
        draw_distance_after_targets(
            viewport.height(),
            initial_draw_scale[player],
            centered_percent,
        )
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
        let local_dirs = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                reverse: player_profile
                    .scroll_option
                    .contains(profile_data::ScrollOption::Reverse),
                split: player_profile
                    .scroll_option
                    .contains(profile_data::ScrollOption::Split),
                alternate: player_profile
                    .scroll_option
                    .contains(profile_data::ScrollOption::Alternate),
                cross: player_profile
                    .scroll_option
                    .contains(profile_data::ScrollOption::Cross),
            },
            cols_per_player,
        );
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
    let (song_lua_mask_windows, song_lua_ease_windows, song_lua_visuals) =
        deadsync_gameplay::build_song_lua_runtime_windows_for_data(
            deadsync_gameplay::build_song_lua_runtime_window_build(
                song.title.as_str(),
                &timing_players,
                num_players,
                &player_profiles,
                config.global_offset_seconds,
                viewport,
                &session,
                config.center_1player_notefield,
                &player_global_offset_shift_seconds,
            ),
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
        measure_counter_segments_for_densities(
            &measure_densities[p],
            player_profiles[p].measure_counter.notes_threshold(),
        )
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
            zmod_stream_totals_for_densities(&measure_densities[p], constant_bpm);
        mini_indicator_total_stream_measures[p] = total_stream.max(0.0);
        mini_indicator_stream_segments[p] = stream_segments;

        let personal_best = mini_indicator_data.personal_best_percent[p];
        let machine_best = mini_indicator_data.machine_best_percent[p];

        mini_indicator_target_score_percent[p] = resolve_target_score_percent(
            gameplay_target_score_setting(player_profiles[p].target_score),
            personal_best,
            machine_best,
        );

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
    let gameplay_play_style = step_stats_play_style(play_style);
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
        gameplay_step_stats_density_graph_width(
            gameplay_play_style,
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
        step_stats_upper_density_graph_width(gameplay_play_style)
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
        let course_carry = course_display_carry.as_ref().map(|carry| carry[p]);
        players[p] = init_player_runtime_for_song(
            init_music_time,
            in_course_stage,
            course_carry,
            player_profiles[p].carry_combo_between_songs,
            replay_mode,
            combo_carry[p],
        );
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
        source: GameplaySourceRuntimeState {
            song,
            charts,
            gameplay_charts,
        },
        setup: GameplaySetupRuntimeState {
            num_cols,
            cols_per_player,
            num_players,
            viewport,
            session,
            config,
        },
        boundary: GameplayBoundaryRuntimeState::new(8, 2),
        timing_runtime: GameplayTimingRuntimeState {
            timing,
            timing_players,
            beat_info_cache,
            timing_profile,
            player_judgment_timing,
        },
        chart_runtime: GameplayChartRuntimeState {
            notes,
            note_ranges: GameplayNoteRangeState::new(note_ranges),
            note_count_stats: GameplayNoteCountStatsState::new(note_count_stats),
            lane_indices: GameplayLaneIndexState::new(
                lane_note_indices,
                lane_note_row_indices,
                lane_hold_indices,
                tap_row_hold_roll_flags,
            ),
            row_indices: GameplayRowIndexState::new(
                row_entry_ranges,
                row_entry_range_start,
                row_map_cache,
                note_row_entry_indices,
            ),
            note_time_cache_ns,
            hold_end_time_cache_ns,
            mine_scan: GameplayMineScanState::new(
                note_range_start,
                mine_note_ix,
                mine_note_time_ns,
            ),
            row_entries,
        },
        clock: GameplayClockRuntimeState {
            audio_clock: GameplayAudioClockState::new(start_delay, 0.0, 0.0),
            song_position: GameplaySongPositionState::new(
                init_beat,
                song_time_ns_from_seconds(init_music_time),
                init_beat,
                init_music_time,
            ),
            display_clock: GameplayDisplayClockState::new(song_time_ns_from_seconds(
                init_music_time,
            )),
            end_timing: GameplayEndTimingState::new(
                notes_end_time_ns,
                music_end_time_ns,
                audio_end_time_ns,
            ),
            music_rate: GameplayMusicRateState::new(rate),
            offsets: GameplayOffsetState::new(
                config.global_offset_seconds,
                player_global_offset_shift_seconds,
                song_offset_seconds,
            ),
            visible_timing: GameplayVisibleTimingState {
                global_visual_delay_seconds,
                player_visual_delay_seconds,
                current_music_time_ns: current_music_time_visible_ns,
                current_music_time: current_music_time_visible,
                current_beat: current_beat_visible,
            },
        },
        hold_runtime: GameplayHoldRuntimeState::new(notes_len, decaying_hold_capacity),
        players_runtime: GameplayPlayersRuntimeState { players },
        display: GameplayDisplayRuntimeState {
            cue_runtime: GameplayCueRuntimeState::new(
                measure_counter_segments,
                column_cues,
                crossover_cues,
            ),
            mini_indicator: GameplayMiniIndicatorRuntimeState::new(
                mini_indicator_stream_segments,
                mini_indicator_total_stream_measures,
                mini_indicator_target_score_percent,
                mini_indicator_rival_score_percent,
            ),
            hold_feedback: GameplayHoldFeedbackState::default(),
            beat_phase: GameplayBeatPhaseState::default(),
            noteskin_effects,
            active_color_index,
            player_color_index,
            notefield_motion: GameplayNotefieldMotionState::new(
                scroll_speed,
                reference_bpm,
                field_zoom,
                pixels_per_second,
                travel_time,
                draw_distance_before_targets,
                draw_distance_after_targets,
                reverse_scroll,
                column_scroll_dirs,
            ),
            receptor_feedback: GameplayReceptorFeedbackState::default(),
            visual_feedback: GameplayVisualFeedbackState::default(),
            danger_fx: GameplayDangerFxState::default(),
            density_graph: GameplayDensityGraphState {
                first_second: density_graph_first_second,
                last_second: density_graph_last_second,
                duration: density_graph_duration,
                graph_w: density_graph_graph_w,
                graph_h: density_graph_graph_h,
                scaled_width: density_graph_scaled_width,
                u0: density_graph_u0,
                u_window: density_graph_u_window,
                life_update_rate: density_graph_life_update_rate,
                life_next_update_elapsed: density_graph_life_next_update_elapsed,
                life_points: density_graph_life_points,
                life_dirty: density_graph_life_dirty,
                top_h: density_graph_top_h,
                top_w: density_graph_top_w,
                top_scale_y: density_graph_top_scale_y,
            },
            toggle_flash: GameplayToggleFlashState::default(),
        },
        progress: GameplayProgressRuntimeState {
            chart_totals: GameplayChartTotalsState::new(
                possible_grade_points,
                total_steps,
                holds_total,
                rolls_total,
                mines_total,
                hands_total,
            ),
            stage: GameplayStageRuntimeState::new(
                replay_mode,
                replay_mode,
                score_valid,
                score_missed_holds_rolls,
            ),
            replay: GameplayReplayRuntimeState::new(
                replay_input,
                replay_capture_enabled,
                replay_edges_capacity,
            ),
            course_display: GameplayCourseDisplayState::new(
                course_display_carry,
                course_display_totals,
                course_display_timing,
            ),
            window_counts: GameplayWindowCountsState::default(),
        },
        profiles_runtime: GameplayProfilesRuntimeState {
            profiles: player_profiles,
        },
        mods: GameplayModRuntimeState {
            song_lua_visuals,
            song_lua_player_transforms: song_lua_player_transforms_default(),
            attacks: GameplayAttackRuntimeState {
                mask_windows: attack_mask_windows,
                song_lua_ease_windows,
                current_appearance: base_attack_appearance,
                target_appearance: base_attack_appearance,
                speed_appearance: [AppearanceEffects::approach_speeds(); MAX_PLAYERS],
                appearance: base_attack_appearance,
                ..GameplayAttackRuntimeState::default()
            },
        },
        control: GameplayControlRuntimeState {
            exit_input: GameplayExitInputState::default(),
            offset_adjust_hold: GameplayOffsetAdjustHoldState::default(),
            input_state: GameplayInputState::default(),
            autoplay_runtime: GameplayAutoplayRuntimeState::new(
                song_seed ^ 0xA17F_0FF5_EED5_1EED,
                note_range_start,
            ),
            autosync: GameplayAutosyncRuntimeState::default(),
            tick_mode,
            assist_clap: GameplayAssistClapState::new(assist_clap_rows),
            update_trace: GameplayUpdateTraceState::default(),
        },
        pending_input: GameplayPendingInputState::with_capacity(pending_edges_capacity),
    };
    state.control.update_trace =
        GameplayUpdateTraceState::from_capacity_snapshot(&state.capacity_trace_snapshot());
    refresh_active_attack_masks(&mut state, 0.0);
    let current_bpm = state
        .timing_runtime
        .timing
        .get_bpm_for_beat(state.clock.song_position.current_beat);
    refresh_live_notefield_options(&mut state, current_bpm);
    let finalize_ms = finalize_started.elapsed().as_secs_f64() * 1000.0;
    let total_ms = init_started.elapsed().as_secs_f64() * 1000.0;
    if total_ms >= 50.0 {
        info!(
            "Gameplay init timing: song='{}' notes={} players={} density_graph={} setup_ms={setup_ms:.3} note_build_ms={note_build_ms:.3} transform_ms={transform_ms:.3} cache_ms={cache_build_ms:.3} timing_ms={timing_prep_ms:.3} hud_ms={hud_prep_ms:.3} graph_ms={graph_prep_ms:.3} finalize_ms={finalize_ms:.3} elapsed_ms={total_ms:.3}",
            state.source.song.title,
            state.chart_runtime.notes.len(),
            state.setup.num_players,
            density_graph_enabled,
        );
    } else {
        debug!(
            "Gameplay init timing: song='{}' notes={} players={} density_graph={} setup_ms={setup_ms:.3} note_build_ms={note_build_ms:.3} transform_ms={transform_ms:.3} cache_ms={cache_build_ms:.3} timing_ms={timing_prep_ms:.3} hud_ms={hud_prep_ms:.3} graph_ms={graph_prep_ms:.3} finalize_ms={finalize_ms:.3} elapsed_ms={total_ms:.3}",
            state.source.song.title,
            state.chart_runtime.notes.len(),
            state.setup.num_players,
            density_graph_enabled,
        );
    }
    state
}

#[inline(always)]
pub(super) fn register_provisional_early_result(
    state: &mut State,
    note_index: usize,
    judgment: Judgment,
) {
    apply_provisional_early_note_result(
        &mut state.chart_runtime.notes,
        &mut state.chart_runtime.row_entries,
        &state.chart_runtime.row_indices.note_row_entry_indices,
        note_index,
        judgment,
    );
}

#[inline(always)]
pub(super) fn set_final_note_result(state: &mut State, note_index: usize, judgment: Judgment) {
    let update = apply_final_note_result_to_rows(
        &mut state.chart_runtime.notes,
        &mut state.chart_runtime.row_entries,
        &state.chart_runtime.row_indices.note_row_entry_indices,
        note_index,
        judgment,
        MAX_COLS,
    );
    let effects = update.effects;
    if let Some(column) = effects.trigger_miss_flash_column {
        state.trigger_column_flash_for_grade(column, judgment.grade);
    }
    if let Some(column) = effects.held_miss_column {
        state.display.hold_feedback.held_miss_judgments[column] = Some(held_miss_render_info(
            state.boundary.total_elapsed_in_screen,
        ));
    }
}

fn trigger_tap_judgment_explosion(
    state: &mut State,
    player: usize,
    column: usize,
    judgment: &Judgment,
) {
    state.trigger_column_flash_for_judgment(player, column, judgment);
    let Some(window_key) = grade_to_window(judgment.grade) else {
        return;
    };
    let bright = state.tap_judgment_uses_bright_explosion(player, judgment);
    spawn_tap_explosion(state, column, window_key, bright);
}

#[inline(always)]
fn trigger_completed_row_tap_explosions(
    state: &mut State,
    player: usize,
    row_index: usize,
) {
    let Some(plan) = ({
        let Some(row_entry) = row_entry_for_cached_row(
            &state.chart_runtime.row_entries,
            &state.chart_runtime.row_indices.row_map_cache[player],
            row_index,
        ) else {
            return;
        };
        completed_row_tap_feedback_plan(&state.chart_runtime.notes, row_entry)
    }) else {
        return;
    };

    for &note_index in &plan.note_indices[..plan.note_count] {
        let note = &state.chart_runtime.notes[note_index];
        let column = note.column;
        if song_lua_hides_note_visual(state, player, column, note.beat) {
            if let Some(window_key) = plan.receptor_window {
                state.trigger_receptor_score_pulse(column, window_key);
            }
            continue;
        }
        trigger_tap_judgment_explosion(state, player, column, &plan.judgment);
    }
}

#[cfg(test)]
fn trigger_tap_explosion(state: &mut State, column: usize, grade: JudgeGrade) {
    state.trigger_column_flash_for_grade(column, grade);
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
    let player = state.player_for_col(column);
    if !state.tap_explosion_enabled_for_player(player, window_key) {
        return;
    }
    let local_col = local_column_for_field(state.setup.cols_per_player, column);
    let spawn_duration = state
        .display
        .noteskin_effects
        .tap_explosion_duration(player, local_col, window_key, bright);
    if let Some(duration) = spawn_duration {
        state.display.visual_feedback.tap_explosions[column] = Some(ActiveTapExplosion {
            window: window_key,
            bright,
            elapsed: 0.0,
            duration,
            start_beat: state.clock.song_position.current_beat,
        });
    }
}

fn trigger_mine_explosion(state: &mut State, column: usize) {
    let player = state.player_for_col(column);
    let duration = state
        .display
        .noteskin_effects
        .mine_explosion_duration(player);
    state.display.visual_feedback.mine_explosions[column] = Some(ActiveMineExplosion {
        elapsed: 0.0,
        duration,
        started_at_screen_s: state.boundary.total_elapsed_in_screen,
    });
    if state.setup.config.mine_hit_sound {
        state.push_audio_command(GameplayAudioCommand::PlayPreloadedSfx(
            "assets/sounds/boom.ogg",
        ));
    }
}

pub(super) fn handle_hold_let_go(
    state: &mut State,
    column: usize,
    note_index: usize,
    let_go_time_ns: SongTimeNs,
) {
    let player = state.player_for_col(column);
    let scoring_blocked = state.autoplay_blocks_scoring();
    let note_type = state.chart_runtime.notes[note_index].note_type;
    let player_dead = state.player_is_dead(player);
    let Some(update) = apply_hold_let_go_update(
        state.chart_runtime.notes[note_index].hold.as_mut(),
        &mut state.hold_runtime.hold_decay_active,
        &mut state.hold_runtime.decaying_hold_indices,
        note_index,
        note_type,
        let_go_time_ns,
        scoring_blocked,
        player_dead,
    ) else {
        return;
    };
    let mut player_state = hold_resolution_player_state(&state.players_runtime.players[player]);
    let player_update =
        apply_hold_let_go_player_state(&mut player_state, update.stats_update, scoring_blocked);
    apply_hold_resolution_player_state(&mut state.players_runtime.players[player], player_state);
    if update.effects.show_judgment {
        state.display.hold_feedback.hold_judgments[column] = Some(hold_judgment_render_info(
            update.result,
            state.boundary.total_elapsed_in_screen,
        ));
    }
    if player_update.apply_life_change {
        let current_music_time = state.current_music_time_seconds();
        apply_life_change(
            &mut state.players_runtime.players[player],
            current_music_time,
            player_update.life_delta,
        );
    }
    if player_update.capture_failed_ex_score_inputs {
        state.capture_failed_ex_score_inputs(player, state.player_blue_window_ms(player));
    }
    if hold_resolution_updates_grade_totals(
        update.result,
        player_update.stats_update,
        state.player_is_dead(player),
    ) {
        update_itg_grade_totals(&mut state.players_runtime.players[player]);
    }
    apply_combo_update(
        &mut state.players_runtime.players[player],
        player_update.combo_update,
    );
    if update.effects.reset_receptor_glow {
        state.display.receptor_feedback.clear_lift_glow(column);
    }
}

#[inline(always)]
fn resolve_active_hold(state: &mut State, column: usize, resolution: ActiveHoldResolution) {
    match resolution {
        ActiveHoldResolution::LetGo {
            note_index,
            time_ns,
        } => handle_hold_let_go(state, column, note_index, time_ns),
        ActiveHoldResolution::Success { note_index } => {
            handle_hold_success(state, column, note_index)
        }
    }
}

#[inline(always)]
pub(super) fn start_active_hold(
    state: &mut State,
    column: usize,
    note_index: usize,
    start_time_ns: SongTimeNs,
    end_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
) {
    if column >= state.setup.num_cols {
        return;
    }
    let player = state.player_for_col(column);
    let rate = state.music_rate();
    // A fast same-column hold jack can hit the next head early while the
    // previous hold is still alive. ITG stores hold state per TapNote; settle
    // the previous non-overlapping hold before replacing this column slot.
    if let Some(event) = settle_replaced_active_hold_column(
        &mut state.hold_runtime.active_holds,
        &mut state.chart_runtime.notes,
        column,
        note_index,
        start_time_ns,
        &state.timing_runtime.timing_players[player],
        rate,
    ) {
        resolve_active_hold(state, event.column, event.resolution);
    }
    start_active_hold_column(
        &mut state.hold_runtime.active_holds,
        &mut state.chart_runtime.notes,
        column,
        note_index,
        start_time_ns,
        end_time_ns,
        current_time_ns,
    );
}

#[inline(always)]
pub(super) fn integrate_active_hold_to_time(
    state: &mut State,
    column: usize,
    target_time_ns: SongTimeNs,
) {
    if column >= state.setup.num_cols || song_time_ns_invalid(target_time_ns) {
        return;
    }

    let player = state.player_for_col(column);
    let rate = state.music_rate();
    if let Some(resolution) = integrate_active_hold_column(
        &mut state.hold_runtime.active_holds,
        &mut state.chart_runtime.notes,
        column,
        &state.timing_runtime.timing_players[player],
        target_time_ns,
        rate,
    ) {
        resolve_active_hold(state, column, resolution);
    }
}

pub(super) fn handle_hold_success(state: &mut State, column: usize, note_index: usize) {
    let player = state.player_for_col(column);
    let scoring_blocked = state.autoplay_blocks_scoring();
    let note_type = state.chart_runtime.notes[note_index].note_type;
    let player_dead = state.player_is_dead(player);
    let Some(update) = apply_hold_success_update(
        state.chart_runtime.notes[note_index].hold.as_mut(),
        &mut state.hold_runtime.hold_decay_active,
        note_index,
        note_type,
        scoring_blocked,
        player_dead,
    ) else {
        return;
    };
    let mut player_state = hold_resolution_player_state(&state.players_runtime.players[player]);
    let player_update =
        apply_hold_success_player_state(&mut player_state, update.stats_update, scoring_blocked);
    apply_hold_resolution_player_state(&mut state.players_runtime.players[player], player_state);
    if player_update.apply_life_change {
        let current_music_time = state.current_music_time_seconds();
        apply_life_change(
            &mut state.players_runtime.players[player],
            current_music_time,
            player_update.life_delta,
        );
    }
    if player_update.capture_failed_ex_score_inputs {
        state.capture_failed_ex_score_inputs(player, state.player_blue_window_ms(player));
    }
    if hold_resolution_updates_grade_totals(
        update.result,
        player_update.stats_update,
        state.player_is_dead(player),
    ) {
        update_itg_grade_totals(&mut state.players_runtime.players[player]);
    }
    apply_combo_update(
        &mut state.players_runtime.players[player],
        player_update.combo_update,
    );
    if update.effects.trigger_hold_explosion {
        trigger_hold_explosion(state, column);
    }
    if update.effects.show_judgment {
        state.display.hold_feedback.hold_judgments[column] = Some(hold_judgment_render_info(
            update.result,
            state.boundary.total_elapsed_in_screen,
        ));
    }
}

pub(super) fn refresh_roll_life_on_step(
    state: &mut State,
    column: usize,
    event_time_ns: SongTimeNs,
) {
    refresh_roll_life_for_active_column(
        &mut state.hold_runtime.active_holds,
        &mut state.chart_runtime.notes,
        column,
        event_time_ns,
    );
}

pub(super) fn update_active_holds(
    state: &mut State,
    inputs: &[bool; MAX_COLS],
    current_time_ns: SongTimeNs,
) {
    let timing_players: [&_; MAX_PLAYERS] =
        std::array::from_fn(|player| state.timing_runtime.timing_players[player].as_ref());
    let live_autoplay = state.live_autoplay_enabled();
    let rate = state.music_rate();
    let mut events = [None; MAX_COLS];
    let update = update_active_hold_columns(
        &mut state.hold_runtime.active_holds,
        &mut state.chart_runtime.notes,
        inputs,
        state.setup.num_cols,
        state.setup.cols_per_player,
        state.setup.num_players,
        &timing_players,
        current_time_ns,
        rate,
        live_autoplay,
        &mut events,
    );
    for event in events.iter().take(update.event_count).flatten() {
        resolve_active_hold(state, event.column, event.resolution);
    }
}

pub(super) fn finalize_row_judgment(
    state: &mut State,
    player: usize,
    row_index: usize,
    row_entry_index: usize,
    skip_life_change: bool,
) {
    let (col_start, col_end) = state.player_col_range(player);
    let Some(plan) = row_finalization_plan_for_entry(
        &state.chart_runtime.notes,
        &state.chart_runtime.row_entries[row_entry_index],
        state.autoplay_blocks_scoring(),
        skip_life_change,
    ) else {
        return;
    };
    state.apply_autosync_for_row_hits(row_entry_index);
    let final_judgment = plan.judgment;
    state.chart_runtime.row_entries[row_entry_index].final_outcome = Some(plan.outcome);
    record_player_live_timing_stats(&mut state.players_runtime.players[player], &final_judgment);
    if plan.record_display_window_counts {
        state.progress.window_counts.record_judgment(
            player,
            &final_judgment,
            state.player_blue_window_ms(player),
        );
    }
    let current_music_time = state.current_music_time_seconds();
    if plan.apply_player_state {
        let p = &mut state.players_runtime.players[player];
        let player_dead = player_runtime_is_dead(p);
        let carried_holds_down = carried_holds_down_at_row(
            &state.chart_runtime.notes,
            &state.hold_runtime.active_holds,
            (col_start, col_end),
            row_index,
        );
        let mut row_state = row_finalization_player_state(p);
        let update = apply_row_finalization_player_state(
            &mut row_state,
            &final_judgment,
            plan.note_count,
            carried_holds_down,
            player_dead,
        );
        set_row_finalization_player_state(p, row_state);
        if update.update_grade_totals {
            update_itg_grade_totals(p);
        }
        if plan.apply_life_change {
            apply_life_change(p, current_music_time, plan.life_delta);
        }
        apply_combo_update(p, update.combo_update);
    }
    if plan.show_final_visual {
        // Arrow Cloud's gameplay HUD uses the row-final JudgmentMessage for
        // offset/error-bar visuals, not individual note hits inside a chord.
        set_last_judgment(state, player, final_judgment);
        error_bar_register_tap(
            state,
            player,
            &final_judgment,
            state.current_music_time_seconds(),
        );
    }
    if plan.capture_failed_ex_score_inputs {
        state.capture_failed_ex_score_inputs(player, state.player_blue_window_ms(player));
    }
}

pub(super) fn update_judged_rows(state: &mut State) {
    let lookahead_time_ns = judged_row_lookahead_time_ns(
        state.clock.song_position.current_music_time_ns,
        &state.timing_runtime.timing_profile,
        state.music_rate(),
    );
    for player in 0..state.setup.num_players {
        let (row_start, row_end) = state.chart_runtime.row_indices.row_entry_ranges[player];
        let row_count = row_end;
        let mut scan_start =
            state.chart_runtime.row_indices.judged_row_cursor[player].max(row_start);
        let mut events = [None; 8];
        loop {
            let update = collect_ready_judged_row_events(
                &state.chart_runtime.row_entries,
                (row_start, row_end),
                scan_start,
                lookahead_time_ns,
                &mut events,
            );
            scan_start = update.next_scan_start;
            for event in events.iter().take(update.event_count).flatten() {
                finalize_row_judgment(
                    state,
                    player,
                    event.row_index,
                    event.row_entry_index,
                    event.skip_life_change,
                );
            }
            if !update.stopped || update.event_count == 0 {
                break;
            }
        }
        state.chart_runtime.row_indices.judged_row_cursor[player] =
            advance_judged_row_cursor_for_entries(
                &state.chart_runtime.row_entries,
                (row_start, row_count),
                state.chart_runtime.row_indices.judged_row_cursor[player],
                lookahead_time_ns,
            );
    }
}

fn hit_mine(
    state: &mut State,
    column: usize,
    note_index: usize,
    time_error_music_ns: SongTimeNs,
) -> bool {
    let player = state.player_for_col(column);
    let rate = normalized_song_rate(state.music_rate());
    let mine_window_music_ns = state.timing_runtime.player_judgment_timing[player]
        .profile_music_ns
        .mine_window_ns;
    let Some(mark) = mark_mine_hit_candidate(
        &mut state.chart_runtime.notes[note_index],
        state.chart_runtime.note_time_cache_ns[note_index],
        time_error_music_ns,
        mine_window_music_ns,
        rate,
    ) else {
        return false;
    };

    state
        .chart_runtime
        .mine_scan
        .pending_mine_hit_indices
        .push(note_index);
    debug!(
        "JUDGE MINE HIT MARKED: row={}, col={}, beat={:.3}, note_time={:.4}s, hit_time={:.4}s, offset_ms={:.2}, rate={:.3}",
        mark.row_index,
        mark.column,
        mark.beat,
        song_time_ns_to_seconds(mark.note_time_ns),
        song_time_ns_to_seconds(mark.hit_time_ns),
        mark.time_error_ms,
        rate
    );
    true
}

fn apply_pending_mine_hits(state: &mut State) {
    if state
        .chart_runtime
        .mine_scan
        .pending_mine_hit_indices
        .is_empty()
    {
        return;
    }

    let pending = std::mem::take(&mut state.chart_runtime.mine_scan.pending_mine_hit_indices);
    let scoring_blocked = state.autoplay_blocks_scoring();
    let current_music_time = state.current_music_time_seconds();

    let mut cursor = 0usize;
    let mut events = [None; 8];
    loop {
        let update = collect_pending_mine_hit_events(
            &state.chart_runtime.notes,
            &pending,
            cursor,
            state.setup.num_players,
            state.setup.cols_per_player,
            &mut events,
        );
        cursor = update.next_cursor;

        for event in events.iter().take(update.event_count).flatten() {
            let column = event.column;
            let player = event.player;

            let side_effect_plan = mine_hit_side_effect_plan(scoring_blocked);
            if side_effect_plan.apply_life_change {
                apply_life_change(
                    &mut state.players_runtime.players[player],
                    current_music_time,
                    side_effect_plan.life_delta,
                );
            }
            if side_effect_plan.capture_failed_ex_score_inputs {
                state.capture_failed_ex_score_inputs(player, state.player_blue_window_ms(player));
            }
            let mut player_state = mine_hit_player_state(&state.players_runtime.players[player]);
            let player_update = apply_mine_hit_player_state(
                &mut player_state,
                scoring_blocked,
                state.player_is_dead(player),
            );
            apply_mine_hit_player_update(
                &mut state.players_runtime.players[player],
                player_state,
                player_update,
            );

            state.display.receptor_feedback.clear_lift_glow(column);
            trigger_mine_explosion(state, column);
            set_last_mine_judgment(state, player, column, MineResult::Hit);
            if player_update.counted_for_score {
                update_itg_grade_totals(&mut state.players_runtime.players[player]);
            }
        }
        if !update.stopped || update.event_count == 0 {
            break;
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
    let player = state.player_for_col(column);
    let rate = normalized_song_rate(state.music_rate());
    let mine_window_music_ns = state.timing_runtime.player_judgment_timing[player]
        .profile_music_ns
        .mine_window_ns;
    let notes = &mut state.chart_runtime.notes;
    let mine_note_ix = &state.chart_runtime.mine_scan.mine_note_ix[player];
    let mine_note_time_ns = &state.chart_runtime.mine_scan.mine_note_time_ns[player];
    let pending_mine_hit_indices = &mut state.chart_runtime.mine_scan.pending_mine_hit_indices;

    mark_crossed_held_mine_candidates(
        notes,
        mine_note_ix,
        mine_note_time_ns,
        column,
        prev_time_ns,
        current_time_ns,
        mine_window_music_ns,
        rate,
        |note_index, mark| {
            pending_mine_hit_indices.push(note_index);
            debug!(
                "JUDGE MINE HIT MARKED: row={}, col={}, beat={:.3}, note_time={:.4}s, hit_time={:.4}s, offset_ms={:.2}, rate={:.3}",
                mark.row_index,
                mark.column,
                mark.beat,
                song_time_ns_to_seconds(mark.note_time_ns),
                song_time_ns_to_seconds(mark.hit_time_ns),
                mark.time_error_ms,
                rate
            );
        },
    )
}

#[inline(always)]
fn error_bar_register_tap(
    state: &mut State,
    player: usize,
    judgment: &Judgment,
    tap_music_time_s: f32,
) {
    let prof = &state.profiles_runtime.profiles[player];
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
    let blue_fantastic_window_s = state.player_blue_window_ms(player) / 1000.0;
    let error_bar_trim = prof.error_bar_trim;
    let error_bar_multi_tick = prof.error_bar_multi_tick;
    let error_ms_display = prof.error_ms_display;
    let short_avg_enabled = prof.short_average_error_bar_enabled;
    let short_avg_intensity =
        profile_data::clamp_average_error_bar_intensity(prof.average_error_bar_intensity);
    let long_avg_enabled = prof.long_error_bar_enabled;
    let long_avg_threshold_s =
        profile_data::clamp_long_error_bar_threshold_ms(prof.long_error_bar_threshold_ms) as f32
            / 1000.0;
    let long_avg_intensity =
        profile_data::clamp_long_error_bar_intensity(prof.long_error_bar_intensity);
    let long_avg_min_samples =
        profile_data::clamp_long_error_bar_min_samples(prof.long_error_bar_min_samples) as usize;
    let average_interval_ms =
        profile_data::clamp_average_error_bar_interval_ms(prof.average_error_bar_interval_ms);
    let Some(window) = judgment.window else {
        return;
    };

    let now = state.boundary.total_elapsed_in_screen;
    let offset_s = judgment.time_error_ms / 1000.0;
    let p = &mut state.players_runtime.players[player];

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
            state.timing_runtime.timing_profile.windows_s[0]
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
    let max_offset_s = state.timing_runtime.timing_profile.windows_s[max_window_ix];
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
            let (avg_raw, avg_count) = error_bar_average_offset_s(
                &mut p.error_bar_avg_samples,
                tap_music_time_s,
                offset_s,
                average_interval_ms,
            );
            let mut avg = avg_raw * short_avg_intensity;
            if max_offset_s.is_finite() && max_offset_s > 0.0 {
                avg = avg.clamp(-max_offset_s, max_offset_s);
            }
            if avg_count == 1 {
                avg *= 0.75;
            }
            error_bar_push_tick(
                &mut p.error_bar_avg_ticks,
                &mut p.error_bar_avg_next,
                error_bar_multi_tick,
                ErrorBarTick {
                    started_at: now,
                    offset_s: avg,
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
            if long_len >= long_avg_min_samples
                && long_mean.abs() * long_avg_intensity >= long_avg_threshold_s
            {
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
    set_player_last_judgment(
        &mut state.players_runtime.players[player],
        judgment,
        state.boundary.total_elapsed_in_screen,
    );
}

#[inline(always)]
fn set_last_mine_judgment(state: &mut State, player: usize, column: usize, result: MineResult) {
    set_player_last_mine_judgment(
        &mut state.players_runtime.players[player],
        result,
        column,
        state.boundary.total_elapsed_in_screen,
    );
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
        state.trigger_receptor_glow_pulse(column);
        spawn_tap_explosion_for_grade(state, column, judgment.grade, false);
    }

    if !hide_early_dw_column_flash {
        state.trigger_column_flash_for_judgment(player, column, judgment);
    }
}

#[inline(always)]
fn start_active_hold_for_hit(
    state: &mut State,
    note_index: usize,
    column: usize,
    hit: NoteHitEval,
    current_time_ns: SongTimeNs,
) {
    let hold_end_time_ns = state
        .chart_runtime
        .hold_end_time_cache_ns
        .get(note_index)
        .copied()
        .flatten();
    if let Some(plan) = hit_active_hold_start(
        state.chart_runtime.notes[note_index].note_type,
        note_index,
        column,
        hit.note_time_ns,
        hold_end_time_ns,
        current_time_ns,
    ) {
        start_active_hold(
            state,
            plan.column,
            plan.note_index,
            plan.start_time_ns,
            plan.end_time_ns,
            plan.current_time_ns,
        );
    }
}

fn judge_a_tap(state: &mut State, column: usize, current_time_ns: SongTimeNs) -> bool {
    let rate = normalized_song_rate(state.music_rate());
    let timing_hit_log = timing_hit_log_enabled();
    let input_log = gameplay_input_log_enabled();
    let player = state.player_for_col(column);
    let rescore_early_hits = state.profiles_runtime.profiles[player].rescore_early_hits;
    let hide_early_dw_judgments = state.profiles_runtime.profiles[player].hide_early_dw_judgments;
    let hide_early_dw_flash = state.profiles_runtime.profiles[player].hide_early_dw_flash;
    let hide_early_dw_column_flash =
        state.profiles_runtime.profiles[player].hide_early_dw_column_flash;
    let scoring_blocked = state.autoplay_blocks_scoring();
    let lane_notes = &state.chart_runtime.lane_indices.note_indices[column];
    let search = closest_lane_note_search(
        lane_notes,
        &state.chart_runtime.notes,
        &state.chart_runtime.note_time_cache_ns,
        &state.timing_runtime.timing_players[player],
        current_time_ns,
    );
    let current_row_index = search.current_row_index;
    if let Some((note_index, _)) = search.candidate {
        let note_row_index = state.chart_runtime.notes[note_index].row_index;
        let note_type = state.chart_runtime.notes[note_index].note_type;
        let time_error_music_ns =
            current_time_ns.saturating_sub(state.chart_runtime.note_time_cache_ns[note_index]);

        if matches!(note_type, NoteType::Mine) {
            if state.chart_runtime.notes[note_index].is_fake {
                log_tap_judge_candidate(
                    input_log,
                    "fake_mine_ignored",
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    note_index,
                    &state.chart_runtime.notes[note_index],
                    state.chart_runtime.note_time_cache_ns[note_index],
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
                &state.chart_runtime.notes[note_index],
                state.chart_runtime.note_time_cache_ns[note_index],
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
                &state.chart_runtime.notes[note_index],
                state.chart_runtime.note_time_cache_ns[note_index],
                rate,
            );
            return false;
        }

        let Some(hit) = state.note_hit_eval(
            player,
            state.chart_runtime.note_time_cache_ns[note_index],
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
                &state.chart_runtime.notes[note_index],
                state.chart_runtime.note_time_cache_ns[note_index],
                rate,
            );
            return false;
        };
        let (song_offset_s, global_offset_s, lead_in_s, stream_pos_s) = if timing_hit_log {
            (
                state.clock.offsets.song_offset_seconds(),
                state.effective_player_global_offset_seconds(player),
                state.clock.audio_clock.positive_lead_in_seconds(),
                state.clock.audio_clock.stream_position_seconds(),
            )
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
        if state.chart_runtime.notes[note_index].is_fake {
            log_tap_judge_candidate(
                input_log,
                "fake_hit",
                player,
                column,
                current_row_index,
                current_time_ns,
                note_index,
                &state.chart_runtime.notes[note_index],
                state.chart_runtime.note_time_cache_ns[note_index],
                rate,
            );
            let hit_plan = state.build_final_note_hit_plan(player, hit, rate);
            let judgment = hit_plan.judgment;
            set_final_note_result(state, note_index, judgment);
            log_timing_hit_detail(
                timing_hit_log,
                stream_pos_s,
                hit.grade,
                note_row_index,
                state.chart_runtime.notes[note_index].column,
                state.chart_runtime.notes[note_index].beat,
                song_offset_s,
                global_offset_s,
                hit.note_time_ns,
                hit_plan.judgment_event_time_ns,
                state.current_music_time_seconds(),
                rate,
                lead_in_s,
            );
            state.trigger_receptor_glow_pulse(column);
            return true;
        }
        let Some(row_entry) = row_entry_for_cached_row(
            &state.chart_runtime.row_entries,
            &state.chart_runtime.row_indices.row_map_cache[player],
            note_row_index,
        ) else {
            debug_assert!(false, "missing row cache for row {note_row_index}");
            return false;
        };
        let row_rescore_track_count = count_rescore_tracks_on_row(row_entry);
        let row_note_count = usize::from(row_entry.unresolved_nonlift_count);

        if rescore_early_hits
            && let Some(early_rescore_decision) = early_rescore_hit_decision(
                row_rescore_track_count,
                hit,
                state.chart_runtime.notes[note_index].early_result.is_some(),
            )
        {
            let note_col = state.chart_runtime.notes[note_index].column;

            if matches!(
                early_rescore_decision,
                EarlyRescoreHitDecision::Provisional
                    | EarlyRescoreHitDecision::DuplicateProvisional
            ) {
                log_tap_judge_candidate(
                    input_log,
                    match early_rescore_decision {
                        EarlyRescoreHitDecision::DuplicateProvisional => {
                            "provisional_early_duplicate"
                        }
                        _ => "provisional_early_hit",
                    },
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    note_index,
                    &state.chart_runtime.notes[note_index],
                    state.chart_runtime.note_time_cache_ns[note_index],
                    rate,
                );
                if early_rescore_decision == EarlyRescoreHitDecision::Provisional {
                    let plan = provisional_early_hit_plan(hit, rate, scoring_blocked);
                    let judgment = plan.judgment;
                    register_provisional_early_result(state, note_index, judgment);
                    let current_music_time = state.current_music_time_seconds();
                    if plan.apply_life_change {
                        apply_life_change(
                            &mut state.players_runtime.players[player],
                            current_music_time,
                            plan.life_delta,
                        );
                    }
                    if plan.capture_failed_ex_score_inputs {
                        state.capture_failed_ex_score_inputs(player, state.player_blue_window_ms(player));
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
                        state.chart_runtime.notes[note_index].beat,
                        song_offset_s,
                        global_offset_s,
                        hit.note_time_ns,
                        current_time_ns,
                        state.current_music_time_seconds(),
                        rate,
                        lead_in_s,
                    );

                    start_active_hold_for_hit(state, note_index, note_col, hit, current_time_ns);
                }
                return true;
            }

            if early_rescore_decision == EarlyRescoreHitDecision::IgnoreBadRehit {
                log_tap_judge_candidate(
                    input_log,
                    "provisional_bad_rehit_ignored",
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    note_index,
                    &state.chart_runtime.notes[note_index],
                    state.chart_runtime.note_time_cache_ns[note_index],
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
                &state.chart_runtime.notes[note_index],
                state.chart_runtime.note_time_cache_ns[note_index],
                rate,
            );
            let hit_plan = state.build_final_note_hit_plan(player, hit, rate);
            let judgment = hit_plan.judgment;
            set_final_note_result(state, note_index, judgment);

            log_timing_hit_detail(
                timing_hit_log,
                stream_pos_s,
                hit.grade,
                note_row_index,
                note_col,
                state.chart_runtime.notes[note_index].beat,
                song_offset_s,
                global_offset_s,
                hit.note_time_ns,
                hit_plan.judgment_event_time_ns,
                state.current_music_time_seconds(),
                rate,
                lead_in_s,
            );

            trigger_completed_row_tap_explosions(state, player, note_row_index);
            if let Some(window_key) = hit_plan.receptor_window {
                state.trigger_receptor_score_pulse(note_col, window_key);
            }
            start_active_hold_for_hit(state, note_index, note_col, hit, current_time_ns);
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
                &state.chart_runtime.notes[note_index],
                state.chart_runtime.note_time_cache_ns[note_index],
                rate,
            );
            return false;
        };

        for &idx in &judge_indices[..judge_count] {
            let note_col = state.chart_runtime.notes[idx].column;
            let Some(hit) = state.note_hit_eval(
                player,
                state.chart_runtime.note_time_cache_ns[idx],
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
                    &state.chart_runtime.notes[idx],
                    state.chart_runtime.note_time_cache_ns[idx],
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
                &state.chart_runtime.notes[idx],
                state.chart_runtime.note_time_cache_ns[idx],
                rate,
            );
            let hit_plan = state.build_final_note_hit_plan(player, hit, rate);
            let judgment = hit_plan.judgment;
            set_final_note_result(state, idx, judgment);

            log_timing_hit_detail(
                timing_hit_log,
                stream_pos_s,
                hit.grade,
                note_row_index,
                note_col,
                state.chart_runtime.notes[idx].beat,
                song_offset_s,
                global_offset_s,
                hit.note_time_ns,
                hit_plan.judgment_event_time_ns,
                state.current_music_time_seconds(),
                rate,
                lead_in_s,
            );

            trigger_completed_row_tap_explosions(state, player, note_row_index);
            if let Some(window_key) = hit_plan.receptor_window {
                state.trigger_receptor_score_pulse(note_col, window_key);
            }
            start_active_hold_for_hit(state, idx, note_col, hit, current_time_ns);
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
            search.search_start_row,
            search.search_end_row,
            search.search_start_idx,
            search.search_end_idx,
            lane_notes.len(),
            song_time_ns_to_seconds(current_time_ns),
            state.current_music_time_seconds(),
        );
    }
    false
}

/// Judge lift notes on button release. Mirrors tap judging's per-note path but
/// only matches NoteType::Lift.
fn judge_a_lift(state: &mut State, column: usize, current_time_ns: SongTimeNs) -> bool {
    let rate = normalized_song_rate(state.music_rate());
    let timing_hit_log = timing_hit_log_enabled();
    let player = state.player_for_col(column);
    let rescore_early_hits = state.profiles_runtime.profiles[player].rescore_early_hits;
    let hide_early_dw_judgments = state.profiles_runtime.profiles[player].hide_early_dw_judgments;
    let hide_early_dw_flash = state.profiles_runtime.profiles[player].hide_early_dw_flash;
    let hide_early_dw_column_flash =
        state.profiles_runtime.profiles[player].hide_early_dw_column_flash;
    let scoring_blocked = state.autoplay_blocks_scoring();
    let lane_notes = &state.chart_runtime.lane_indices.note_indices[column];
    let search = closest_lane_note_search(
        lane_notes,
        &state.chart_runtime.notes,
        &state.chart_runtime.note_time_cache_ns,
        &state.timing_runtime.timing_players[player],
        current_time_ns,
    );
    let Some((note_index, _)) = search.candidate else {
        return false;
    };
    if !lane_edge_matches_note_type(false, state.chart_runtime.notes[note_index].note_type) {
        return false;
    }

    let Some(hit) = state.note_hit_eval(
        player,
        state.chart_runtime.note_time_cache_ns[note_index],
        current_time_ns,
    ) else {
        return false;
    };
    let (song_offset_s, global_offset_s, lead_in_s, stream_pos_s) = if timing_hit_log {
        (
            state.clock.offsets.song_offset_seconds(),
            state.effective_player_global_offset_seconds(player),
            state.clock.audio_clock.positive_lead_in_seconds(),
            state.clock.audio_clock.stream_position_seconds(),
        )
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    let note_col = state.chart_runtime.notes[note_index].column;
    let note_row_index = state.chart_runtime.notes[note_index].row_index;
    let note_beat = state.chart_runtime.notes[note_index].beat;

    if rescore_early_hits {
        let Some(row_entry) = row_entry_for_cached_row(
            &state.chart_runtime.row_entries,
            &state.chart_runtime.row_indices.row_map_cache[player],
            note_row_index,
        ) else {
            debug_assert!(false, "missing row cache for row {note_row_index}");
            return false;
        };
        let row_rescore_track_count = count_rescore_tracks_on_row(row_entry);
        if let Some(early_rescore_decision) = early_rescore_hit_decision(
            row_rescore_track_count,
            hit,
            state.chart_runtime.notes[note_index].early_result.is_some(),
        ) {
            if early_rescore_decision == EarlyRescoreHitDecision::Provisional {
                let plan = provisional_early_hit_plan(hit, rate, scoring_blocked);
                let judgment = plan.judgment;
                register_provisional_early_result(state, note_index, judgment);
                let current_music_time = state.current_music_time_seconds();
                if plan.apply_life_change {
                    apply_life_change(
                        &mut state.players_runtime.players[player],
                        current_music_time,
                        plan.life_delta,
                    );
                }
                if plan.capture_failed_ex_score_inputs {
                    state.capture_failed_ex_score_inputs(player, state.player_blue_window_ms(player));
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
                    state.current_music_time_seconds(),
                    rate,
                    lead_in_s,
                );
                return true;
            }

            if matches!(
                early_rescore_decision,
                EarlyRescoreHitDecision::DuplicateProvisional
                    | EarlyRescoreHitDecision::IgnoreBadRehit
            ) {
                return true;
            }
        }
    }

    let hit_plan = state.build_final_note_hit_plan(player, hit, rate);
    let judgment = hit_plan.judgment;
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
        hit_plan.judgment_event_time_ns,
        state.current_music_time_seconds(),
        rate,
        lead_in_s,
    );

    trigger_completed_row_tap_explosions(state, player, note_row_index);
    if let Some(window_key) = hit_plan.receptor_window {
        state.trigger_receptor_score_pulse(note_col, window_key);
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
    let timeline_reset = state
        .control
        .assist_clap
        .note_sfx_generation(assist_sfx_generation);

    let future_row = assist_lookahead_future_row(
        &state.timing_runtime.timing,
        state.clock.offsets.global_offset_seconds(),
        state.clock.audio_clock.output_delay_seconds(),
        music_time_ns,
        slope,
        song_row,
    );
    let update = state.control.assist_clap.schedule_update(
        song_row,
        future_row,
        state.control.tick_mode == GameplayTimingTickMode::Assist,
        timeline_reset,
    );
    for ix in update.schedule_start..update.schedule_end {
        let clap_row = state.control.assist_clap.rows[ix];
        schedule_assist_clap_row(state, clap_row);
    }
}

/// Schedules a single assist clap row by its absolute stream frame so the mixer
/// can place the onset sample-accurately. Falls back to immediate playback when
/// the row has no usable stream-frame mapping (e.g. during lead-in).
#[inline(always)]
fn schedule_assist_clap_row(state: &mut State, clap_row: usize) {
    let Some(music_seconds) =
        assist_clap_music_seconds_for_row(&state.timing_runtime.timing, clap_row)
    else {
        state.push_audio_command(GameplayAudioCommand::PlayPreloadedAssistTick(
            ASSIST_TICK_SFX_PATH,
        ));
        return;
    };
    state.push_audio_command(GameplayAudioCommand::PlayAssistTickAtMusicTime {
        path: ASSIST_TICK_SFX_PATH,
        music_seconds,
    });
}

#[inline(always)]
fn decay_let_go_hold_life(state: &mut State) {
    let rate = state.music_rate();
    decay_let_go_hold_life_for_indices(
        &mut state.chart_runtime.notes,
        &mut state.hold_runtime.hold_decay_active,
        &mut state.hold_runtime.decaying_hold_indices,
        state.clock.song_position.current_music_time_ns,
        rate,
    );
}

#[inline(always)]
fn resolve_pending_missed_holds(state: &mut State, current_time_ns: SongTimeNs) {
    let mut score_missed_holds_rolls_by_column = [false; MAX_COLS];
    for col in 0..state.setup.num_cols.min(MAX_COLS) {
        score_missed_holds_rolls_by_column[col] =
            state.progress.stage.score_missed_holds_rolls[state.player_for_col(col)];
    }
    let mut events = [None; 8];
    loop {
        let update = collect_pending_missed_hold_resolutions(
            &state.chart_runtime.notes,
            &state.chart_runtime.hold_end_time_cache_ns,
            &mut state.hold_runtime.pending_missed_hold_resolution,
            &mut state.hold_runtime.pending_missed_hold_indices,
            current_time_ns,
            &score_missed_holds_rolls_by_column[..state.setup.num_cols.min(MAX_COLS)],
            &mut events,
        );
        for event in events.iter().take(update.event_count).flatten() {
            let event = *event;
            match event.resolution {
                PendingMissedHoldResolution::None => {}
                PendingMissedHoldResolution::ShowMissedFeedback => {
                    let column = event.column;
                    state.display.hold_feedback.hold_judgments[column] =
                        Some(HoldJudgmentRenderInfo {
                            result: HoldResult::Missed,
                            started_at_screen_s: state.boundary.total_elapsed_in_screen,
                        });
                }
                PendingMissedHoldResolution::ScoreLetGo => {
                    handle_hold_let_go(state, event.column, event.note_index, event.end_time_ns);
                }
            }
        }
        if update.finished || update.event_count == 0 {
            break;
        }
    }
}

#[inline(always)]
fn track_held_miss_windows(
    state: &mut State,
    inputs: &[bool; MAX_COLS],
    music_time_ns: SongTimeNs,
) {
    let mut largest_windows_ns = [0; MAX_PLAYERS];
    for player in 0..state.setup.num_players.min(MAX_PLAYERS) {
        largest_windows_ns[player] = state.player_largest_tap_window_ns(player);
    }
    track_held_miss_windows_for_players(
        &state.chart_runtime.notes,
        &state.chart_runtime.note_time_cache_ns,
        &mut state.hold_runtime.tap_miss_held_window,
        state.chart_runtime.note_ranges.ranges(),
        &state.chart_runtime.mine_scan.next_tap_miss_cursor,
        &largest_windows_ns,
        state.setup.num_players,
        state.setup.cols_per_player,
        inputs,
        music_time_ns,
    );
}

#[inline(always)]
fn apply_time_based_mine_avoidance(state: &mut State, music_time_ns: SongTimeNs) {
    let music_time_sec = song_time_ns_to_seconds(music_time_ns);
    let log_mine_avoid = log::log_enabled!(log::Level::Trace);
    let timing_players: [&_; MAX_PLAYERS] =
        std::array::from_fn(|player| state.timing_runtime.timing_players[player].as_ref());
    let cutoff_rows = missed_note_cutoff_rows_for_players(
        &state.timing_runtime.timing_profile,
        &timing_players,
        state.music_rate(),
        music_time_ns,
        state.setup.num_players,
    );
    let player_updates = apply_time_based_mine_avoidance_for_players(
        &mut state.chart_runtime.notes,
        &state.chart_runtime.mine_scan.mine_note_ix,
        &state.chart_runtime.mine_scan.next_mine_ix_cursor,
        &cutoff_rows,
        state.chart_runtime.note_ranges.ranges(),
        state.setup.num_players,
    );
    for player in 0..player_updates.players_scanned {
        let update = player_updates.updates[player];
        if let Some(event) = update.last_avoided {
            if log_mine_avoid {
                let row_index = event.row_index;
                let column = event.column;
                trace!("MINE AVOIDED: Row {row_index}, Col {column}, Time: {music_time_sec:.2}s");
            }
            set_last_mine_judgment(state, player, event.column, MineResult::Avoided);
        }
        if update.avoided_count > 0 {
            add_player_mines_avoided(
                &mut state.players_runtime.players[player],
                update.avoided_count,
            );
        }
        state.chart_runtime.mine_scan.next_mine_ix_cursor[player] = update.mine_end;
        state.chart_runtime.mine_scan.next_mine_avoid_cursor[player] =
            update.next_mine_avoid_cursor;
    }
}

fn finalize_completed_mines(state: &mut State) {
    let mines_hit: [u32; MAX_PLAYERS] = std::array::from_fn(|player| {
        state
            .players_runtime
            .players
            .get(player)
            .map(player_mines_hit)
            .unwrap_or(0)
    });
    let update = finalize_completed_mine_avoidance_for_players(
        &mut state.chart_runtime.notes,
        state.chart_runtime.note_ranges.ranges(),
        &state.progress.chart_totals.mines_total,
        &mines_hit,
        state.setup.num_players,
    );
    for player in 0..update.players_finalized {
        set_player_mines_avoided(
            &mut state.players_runtime.players[player],
            update.mines_avoided[player],
        );
    }
}

#[inline(always)]
fn apply_time_based_tap_misses(state: &mut State, music_time_ns: SongTimeNs) {
    let rate = normalized_song_rate(state.music_rate());
    let music_time_sec = song_time_ns_to_seconds(music_time_ns);
    let timing_players: [&_; MAX_PLAYERS] =
        std::array::from_fn(|player| state.timing_runtime.timing_players[player].as_ref());
    let cutoff_rows = missed_note_cutoff_rows_for_players(
        &state.timing_runtime.timing_profile,
        &timing_players,
        state.music_rate(),
        music_time_ns,
        state.setup.num_players,
    );
    let mut miss_events = [None; 16];
    loop {
        let update = collect_time_based_tap_misses_for_players(
            &mut state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            &state.hold_runtime.tap_miss_held_window,
            &mut state.hold_runtime.hold_decay_active,
            &mut state.hold_runtime.decaying_hold_indices,
            &mut state.chart_runtime.mine_scan.next_tap_miss_cursor,
            state.chart_runtime.note_ranges.ranges(),
            &cutoff_rows,
            music_time_ns,
            rate,
            &state.progress.stage.score_missed_holds_rolls,
            state.setup.num_players,
            &mut miss_events,
        );
        for player_event in miss_events.iter().take(update.event_count).flatten() {
            let player = player_event.player;
            let event = player_event.event;
            if event.queue_missed_hold_resolution {
                queue_pending_missed_hold_resolution(
                    &mut state.hold_runtime.pending_missed_hold_resolution,
                    &mut state.hold_runtime.pending_missed_hold_indices,
                    event.note_index,
                );
            }
            set_final_note_result(state, event.note_index, event.judgment);
            {
                let judgment_time_error_ms = event.judgment.time_error_ms;
                if log::log_enabled!(log::Level::Debug) {
                    let note_time = song_time_ns_to_seconds(event.note_time_ns);
                    let song_offset_s = state.clock.offsets.song_offset_seconds();
                    let global_offset_s = state.effective_player_global_offset_seconds(player);
                    let lead_in_s = state.clock.audio_clock.positive_lead_in_seconds();
                    let stream_pos_s = state.clock.audio_clock.stream_position_seconds();
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
                        event.row_index,
                        event.column,
                        event.beat,
                        song_offset_s,
                        global_offset_s,
                        note_time,
                        music_time_sec,
                        judgment_time_error_ms,
                        event.miss_because_held,
                        rate,
                        lead_in_s,
                        stream_pos_s,
                        expected_stream_for_note_s,
                        stream_delta_note_ms,
                        expected_stream_for_miss_s,
                        stream_delta_miss_ms,
                    );
                }
                debug!("MISSED (time-based): Row {}", event.row_index);
            }
        }
        if !update.stopped || update.event_count == 0 {
            break;
        }
    }
}

#[inline(always)]
fn settle_completion_rows(state: &mut State) -> bool {
    update_judged_rows(state);
    score_rows_finalized_for_players(
        &state.chart_runtime.row_entries,
        &state.chart_runtime.row_indices.row_entry_ranges,
        state.setup.num_players,
    )
}

pub fn update(
    state: &mut State,
    delta_time: f32,
    audio_snapshot: GameplayAudioSnapshot,
) -> GameplayAction {
    if let Some(exit) = state.control.exit_input.exit_transition {
        state.boundary.total_elapsed_in_screen += delta_time;
        if exit.started_at.elapsed().as_secs_f32() >= exit_total_seconds(exit.kind) {
            state.control.exit_input.clear_exit();
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
    state.clock.audio_clock.set_audio_snapshot(audio_snapshot);

    if let Some(at) = state.control.exit_input.hold_to_exit_aborted_at
        && at.elapsed().as_secs_f32() >= GIVE_UP_ABORT_TEXT_SECONDS
    {
        state.control.exit_input.clear_aborted_hold();
    }

    // Music time driven directly by the audio device clock, interpolated
    // between callbacks for smooth, continuous motion.
    let song_clock = current_song_clock_snapshot(
        audio_snapshot,
        state.music_rate(),
        state.clock.audio_clock.lead_in_seconds(),
        state.clock.offsets.global_offset_seconds(),
    );
    let lead_in = state.clock.audio_clock.positive_lead_in_seconds();
    let previous_music_time_ns = state.clock.song_position.current_music_time_ns;
    let mut music_time_ns = song_clock.song_time_ns;
    let is_first_update = state.boundary.total_elapsed_in_screen <= f32::EPSILON;
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
    state.clock.song_position.current_music_time_ns = music_time_ns;
    let display_diag_host_nanos = if song_clock.valid_at_host_nanos != 0 {
        song_clock.valid_at_host_nanos
    } else {
        deadlib_platform::host_time::instant_nanos(Instant::now())
    };
    let display_music_time_ns = frame_stable_display_music_time_ns(
        &mut state.clock.display_clock,
        display_diag_host_nanos,
        music_time_ns,
        delta_time,
        song_clock.seconds_per_second,
        is_first_update,
    );
    state.clock.song_position.current_music_time_display =
        song_time_ns_to_seconds(display_music_time_ns);

    if let (Some(key), Some(start_time)) = (
        state.control.exit_input.hold_to_exit_key,
        state.control.exit_input.hold_to_exit_start,
    ) {
        let hold_s = hold_to_exit_seconds(key);
        if start_time.elapsed().as_secs_f32() >= hold_s {
            if key == HoldToExitKey::Start && music_time_ns >= state.notes_end_time_ns() {
                state.progress.stage.song_completed_naturally = true;
                finalize_completed_mines(state);
            }
            match key {
                HoldToExitKey::Start => {
                    state.begin_exit_transition(ExitTransitionKind::Out);
                }
                HoldToExitKey::Back => {
                    state.begin_exit_transition(ExitTransitionKind::Cancel);
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
    state.boundary.total_elapsed_in_screen += delta_time;

    let pre_notes_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    {
        let beat_info = state
            .timing_runtime
            .timing
            .get_beat_info_from_time_ns_cached(
                music_time_ns,
                &mut state.timing_runtime.beat_info_cache,
            );
        state.clock.song_position.current_beat = beat_info.beat;
        state.clock.song_position.current_beat_display = state
            .timing_runtime
            .timing
            .get_beat_for_time_ns(display_music_time_ns);
        state
            .display
            .beat_phase
            .set(beat_info.is_in_freeze, beat_info.is_in_delay);
        let song_row = state.assist_row_no_offset_ns(music_time_ns);
        run_assist_clap(
            state,
            song_row,
            music_time_ns,
            song_clock.seconds_per_second,
            audio_snapshot.assist_sfx_generation,
        );

        for player in 0..state.setup.num_players {
            let delay = state.clock.visible_timing.visual_delay_seconds(player);
            let visible_time_ns = visible_notefield_time_ns(music_time_ns, delay);
            state.clock.visible_timing.set_player_time(
                player,
                visible_time_ns,
                song_time_ns_to_seconds(visible_time_ns),
                state.timing_runtime.timing_players[player].get_beat_for_time_ns(visible_time_ns),
            );
        }
        refresh_active_attack_masks(state, delta_time);

        let current_bpm = state
            .timing_runtime
            .timing
            .get_bpm_for_beat(state.clock.song_position.current_beat);
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
    if state.progress.replay.mode {
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
    state.update_offset_adjust_hold(Instant::now());
    process_input_edges(state, trace_enabled, &mut phase_timings, song_clock);
    if let Some(started) = input_started {
        phase_timings.input_edges_us = elapsed_us_since(started);
    }

    let held_mines_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    let current_inputs = state.current_lane_inputs();
    if !state.live_autoplay_enabled() {
        for (col, crossed_from_ns) in state
            .held_mine_crossing_start_times(&current_inputs, previous_music_time_ns, music_time_ns)
            .into_iter()
            .enumerate()
        {
            if let Some(crossed_from_ns) = crossed_from_ns {
                let _ =
                    try_hit_crossed_mines_while_held(state, col, crossed_from_ns, music_time_ns);
            }
        }
    }
    track_held_miss_windows(state, &current_inputs, music_time_ns);
    state.set_previous_lane_inputs(current_inputs);
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
    if state.clock.song_position.current_music_time_ns >= state.music_end_time_ns() {
        if !settle_completion_rows(state) && trace_enabled {
            trace!("Music end time reached with pending score rows; completing gameplay.");
        }
        debug!("Music end time reached. Transitioning to evaluation.");
        state.progress.stage.song_completed_naturally = true;
        state.begin_outro_attack_clear();
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

    if matches!(
        state.setup.config.default_fail_type,
        GameplayFailType::Immediate
    ) && state.all_joined_players_failed()
    {
        debug!("All joined players failed. Transitioning to evaluation.");
        state.progress.stage.song_completed_naturally = false;
        state.push_audio_command(GameplayAudioCommand::StopMusic);
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
    let now = state.boundary.total_elapsed_in_screen;
    for player in 0..state.setup.num_players {
        if state.profiles_runtime.profiles[player].hide_lifebar {
            state.display.danger_fx.reset_player(player);
            continue;
        }

        let health = player_health_state(&state.players_runtime.players[player]);
        let hide_danger = state.profiles_runtime.profiles[player].hide_danger;
        state
            .display
            .danger_fx
            .update_player(player, health, now, hide_danger);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExitTransitionKind, FinalizedRowOutcome, GameplayAudioCommand, GameplaySessionCommand,
        GameplayTimingTickMode, HELD_MISS_TOTAL_DURATION, HeldMissRenderInfo,
        HoldJudgmentRenderInfo, HoldToExitKey, MAX_COLS, MAX_PLAYERS, OFFSET_ADJUST_STEP_SECONDS,
        RowEntry, ScrollSpeedSetting, SongLuaNoteHideWindowRuntime, TIMING_WINDOW_SECONDS_HOLD,
        apply_pending_mine_hits, apply_time_based_mine_avoidance,
        apply_time_based_tap_misses, build_attack_mask_windows_for_player, build_row_entry,
        crossed_mine_held_start_time, effective_mini_percent_for_player,
        effective_scroll_effects_for_player, effective_visual_effects_for_player,
        error_bar_register_tap, finalize_completed_mines, finalize_row_judgment, handle_input,
        handle_queued_raw_key, hit_mine, integrate_active_hold_to_time, judge_a_tap,
        max_step_distance_ns, process_input_edges, refresh_active_attack_masks,
        render_provisional_early_rescore_feedback, resolve_pending_missed_holds,
        score_invalid_reason_lines_for_chart, set_final_note_result, settle_completion_rows,
        song_time_ns_from_seconds, start_active_hold, tick_visual_effects,
        trigger_completed_row_tap_explosions, trigger_hold_explosion, trigger_mine_explosion,
        trigger_tap_explosion, try_hit_crossed_mines_while_held, update_active_holds,
        update_judged_rows,
    };
    use crate::game::parsing::noteskin::{self, Noteskin, Style};
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
    use deadsync_rules::timing::{DelaySegment, TimingData, TimingSegments};
    use std::sync::{Arc, LazyLock, Mutex};
    use std::time::Instant;
    use std::{fs, path::PathBuf};
    use winit::keyboard::KeyCode;

    static SESSION_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

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
        let session = super::GameplaySession {
            play_style: super::gameplay_play_style_from_profile(profile::get_session_play_style()),
            player_side: super::gameplay_player_side_from_profile(
                profile::get_session_player_side(),
            ),
            joined_sides: [
                profile::is_session_side_joined(profile_data::PlayerSide::P1),
                profile::is_session_side_joined(profile_data::PlayerSide::P2),
            ],
            ..super::GameplaySession::default()
        };
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
                        let context = deadsync_gameplay::build_song_lua_compile_context(
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
                        assert!(!state.mods.attacks.song_lua_ease_windows[0].is_empty());

                        let mut times =
                            vec![0.0, state.clock.song_position.current_music_time_display];
                        for window in &state.mods.attacks.song_lua_ease_windows[0] {
                            times.push(window.start_second);
                            times.push((window.start_second + window.end_second) * 0.5);
                            times.push(window.end_second);
                            times.push(window.sustain_end_second);
                        }
                        times.sort_by(f32::total_cmp);
                        times.dedup_by(|a, b| (*a - *b).abs() <= 0.001);

                        let assets = crate::assets::AssetManager::new();
                        for time in times {
                            state.clock.song_position.current_music_time_display = time;
                            state.clock.visible_timing.current_music_time = [time; MAX_PLAYERS];
                            state.clock.song_position.current_beat =
                                state.timing_runtime.timing.get_beat_for_time(time);
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
        state.chart_runtime.notes[note_index] = test_note(column, row_index, NoteType::Mine);
        state.chart_runtime.note_time_cache_ns[note_index] = time_ns;
        state.chart_runtime.mine_scan.mine_note_ix[0] = vec![note_index];
        state.chart_runtime.mine_scan.mine_note_time_ns[0] = vec![time_ns];
        state.chart_runtime.mine_scan.next_mine_ix_cursor[0] = 0;
        state.chart_runtime.mine_scan.next_mine_avoid_cursor[0] = note_index;
        state.progress.chart_totals.mines_total[0] = 1;
    }

    fn set_state_timing(state: &mut super::State, timing: Arc<TimingData>) {
        state.timing_runtime.timing = Arc::clone(&timing);
        state.timing_runtime.timing_players[0] = Arc::clone(&timing);
        state.timing_runtime.timing_players[1] = timing;
    }

    #[test]
    fn regression_state_passes_hot_state_audit() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let state = regression_state(profiles);
        super::assert_valid_hot_state_for_tests(
            &state,
            0.0,
            state.clock.song_position.current_music_time_display,
        );
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

                assert_eq!(state.setup.num_players, 1);
                assert_eq!(
                    state.scroll_speed_for_player(0),
                    ScrollSpeedSetting::CMod(777.0)
                );
                assert_eq!(
                    state.profiles_runtime.profiles[0].display_name,
                    "P2 runtime"
                );
                assert_eq!(
                    state.profiles_runtime.profiles[0].perspective,
                    profile_data::Perspective::Space
                );
                assert_eq!(
                    state.profiles_runtime.profiles[0].judgment_graphic,
                    p2.judgment_graphic
                );
                assert_eq!(state.display.player_color_index, 3);
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
                assert_eq!(state.control.exit_input.hold_to_exit_key, None);

                handle_input(&mut state, &test_input_event(VirtualAction::p2_start));
                assert_eq!(
                    state.control.exit_input.hold_to_exit_key,
                    Some(HoldToExitKey::Start)
                );
                assert!(state.control.exit_input.hold_to_exit_start.is_some());
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
                assert_eq!(start_state.setup.num_players, 2);
                handle_input(&mut start_state, &test_input_event(VirtualAction::p2_start));
                assert_eq!(
                    start_state.control.exit_input.hold_to_exit_key,
                    Some(HoldToExitKey::Start)
                );
                assert!(start_state.control.exit_input.hold_to_exit_start.is_some());

                let mut back_state = regression_state(state_profiles);
                assert_eq!(back_state.setup.num_players, 2);
                handle_input(&mut back_state, &test_input_event(VirtualAction::p2_back));
                assert_eq!(
                    back_state.control.exit_input.hold_to_exit_key,
                    Some(HoldToExitKey::Back)
                );
                assert!(back_state.control.exit_input.hold_to_exit_start.is_some());
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
                let hold_start = state.control.exit_input.hold_to_exit_start;

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

                assert_eq!(
                    state.control.exit_input.hold_to_exit_key,
                    Some(HoldToExitKey::Back)
                );
                assert_eq!(state.control.exit_input.hold_to_exit_start, hold_start);
                assert_eq!(state.control.exit_input.hold_to_exit_aborted_at, None);
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
                state.setup.config.delayed_back = false;

                handle_input(&mut state, &test_input_event(VirtualAction::p1_back));

                let exit = state.control.exit_input.exit_transition;
                let hold_key = state.control.exit_input.hold_to_exit_key;

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
                state.setup.config.delayed_back = true;

                handle_input(&mut state, &test_input_event(VirtualAction::p1_back));

                let hold_key = state.control.exit_input.hold_to_exit_key;
                let hold_start = state.control.exit_input.hold_to_exit_start;
                let exit = state.control.exit_input.exit_transition;

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
        assert!(state.control.exit_input.exit_transition.is_none());

        state.begin_restart_exit();

        let exit = state
            .control
            .exit_input
            .exit_transition
            .expect("begin_restart_exit should arm an exit_transition");
        assert_eq!(
            exit.kind,
            ExitTransitionKind::Cancel,
            "restart should reuse the fast Cancel out-fade for SL/zmod parity"
        );
        assert_eq!(
            state.drain_audio_commands().collect::<Vec<_>>(),
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
        state.begin_exit_transition(ExitTransitionKind::Out);
        let original = state
            .control
            .exit_input
            .exit_transition
            .expect("primed exit");

        state.begin_restart_exit();
        let after = state
            .control
            .exit_input
            .exit_transition
            .expect("still exiting");
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

        let song_offset_before = song_state.song_offset_seconds();
        let global_offset_before = global_state.global_offset_seconds();
        let song_before = song_state.chart_runtime.note_time_cache_ns[0];
        let global_before = global_state.chart_runtime.note_time_cache_ns[0];

        assert!(song_state.apply_song_offset_delta(0.010));
        assert!(global_state.apply_global_offset_delta(0.010));

        let song_after = song_state.chart_runtime.note_time_cache_ns[0];
        let global_after = global_state.chart_runtime.note_time_cache_ns[0];
        let expected_delta_ns = song_time_ns_from_seconds(0.010);
        let song_delta_ns = song_before - song_after;
        let global_delta_ns = global_before - global_after;

        assert!((song_state.song_offset_seconds() - (song_offset_before + 0.010)).abs() <= 1e-6);
        assert!(
            (global_state.global_offset_seconds() - (global_offset_before + 0.010)).abs() <= 1e-6
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

        state
            .clock
            .offsets
            .set_player_global_offset_shift_seconds(0, shift);
        let effective_offset = state
            .clock
            .offsets
            .effective_player_global_offset_seconds(0);
        Arc::make_mut(&mut state.timing_runtime.timing_players[0])
            .set_global_offset_seconds(effective_offset);
        state.refresh_timing_after_offset_change();

        let machine_before = state.global_offset_seconds();
        let effective_before = state.effective_player_global_offset_seconds(0);
        let note_before = state.chart_runtime.note_time_cache_ns[0];

        assert!((effective_before - (machine_before + shift)).abs() <= 1e-6);
        assert!(state.apply_global_offset_delta(0.010));

        let effective_after = state.effective_player_global_offset_seconds(0);
        let note_after = state.chart_runtime.note_time_cache_ns[0];

        assert!((state.global_offset_seconds() - (machine_before + 0.010)).abs() <= 1e-6);
        assert!((effective_after - (state.global_offset_seconds() + shift)).abs() <= 1e-6);
        assert_eq!(note_before - note_after, song_time_ns_from_seconds(0.010));
    }

    #[test]
    fn synced_raw_modifier_makes_first_offset_key_use_global_offset() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);

        let song_before = state.song_offset_seconds();
        let global_before = state.global_offset_seconds();

        state.set_raw_modifier_state(true, false);
        let _ = handle_queued_raw_key(&mut state, KeyCode::F12, true, Instant::now(), 0.0, true);

        assert!((state.song_offset_seconds() - song_before).abs() <= 1e-6);
        assert!(
            (state.global_offset_seconds() - (global_before + OFFSET_ADJUST_STEP_SECONDS)).abs()
                <= 1e-6
        );
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
    fn timing_tick_key_queues_session_command() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);

        let action =
            handle_queued_raw_key(&mut state, KeyCode::F7, true, Instant::now(), 0.0, true);

        assert!(matches!(action, super::RawKeyAction::None));
        assert_eq!(state.timing_tick_status_line(), Some("Assist Tick"));
        assert_eq!(
            state.drain_session_commands().collect::<Vec<_>>(),
            vec![GameplaySessionCommand::SetTimingTickMode(
                GameplayTimingTickMode::Assist
            )]
        );
    }

    #[test]
    fn active_hold_let_go_visual_row_uses_frame_target() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        let timing = Arc::new(test_timing(ROWS_PER_BEAT as usize * 4));
        state.timing_runtime.timing = timing.clone();
        state.timing_runtime.timing_players = [timing.clone(), timing];

        let hold_end_ns = song_time_ns_from_seconds(2.0);
        state.chart_runtime.notes[0] = test_hold(0, 0, ROWS_PER_BEAT as usize * 2);
        state.chart_runtime.hold_end_time_cache_ns[0] = Some(hold_end_ns);
        state.chart_runtime.notes[0]
            .hold
            .as_mut()
            .expect("test hold")
            .life = 0.25;
        state.hold_runtime.active_holds[0] = Some(super::ActiveHold {
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

        let hold = state.chart_runtime.notes[0]
            .hold
            .as_ref()
            .expect("test hold");
        assert_eq!(hold.result, Some(HoldResult::LetGo));
        assert!(state.hold_runtime.active_holds[0].is_none());
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

        state.chart_runtime.notes[0] = test_hold(0, 0, ROWS_PER_BEAT as usize);
        state.chart_runtime.notes[1] =
            test_hold(0, ROWS_PER_BEAT as usize + 12, ROWS_PER_BEAT as usize * 2);
        state.chart_runtime.hold_end_time_cache_ns[0] = Some(previous_end_ns);
        state.chart_runtime.hold_end_time_cache_ns[1] = Some(next_end_ns);
        state.hold_runtime.active_holds[0] = Some(super::ActiveHold {
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
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|hold| hold.result),
            Some(HoldResult::Held)
        );
        assert_eq!(
            state.hold_runtime.active_holds[0]
                .as_ref()
                .map(|active| active.note_index),
            Some(1)
        );
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
        state.chart_runtime.notes[0] = roll;

        let event_time_ns = song_time_ns_from_seconds(super::TIMING_WINDOW_SECONDS_ROLL + 0.01);
        state.hold_runtime.active_holds[0] = Some(super::ActiveHold {
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
            .pending_input
            .edges
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

        let active = state.hold_runtime.active_holds[0]
            .as_ref()
            .expect("roll should remain active after the body step");
        assert_eq!(active.life, super::MAX_HOLD_LIFE);
        assert_eq!(active.last_update_time_ns, event_time_ns);
        let hold = state.chart_runtime.notes[0]
            .hold
            .as_ref()
            .expect("roll hold data");
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
        state.pending_input.edges.push_back(edge);

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

        assert_eq!(
            state.control.input_state.lane_pressed_since_ns[0],
            Some(event_time_ns)
        );
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
            .pending_input
            .edges
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

        assert!(state.display.receptor_feedback.bop_timers[0] > 0.0);
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
                state.chart_runtime.notes = vec![
                    test_note(0, row_index, NoteType::Tap),
                    test_note(1, row_index, NoteType::Tap),
                ];
                state.chart_runtime.note_time_cache_ns = vec![
                    song_time_ns_from_seconds(1.0),
                    song_time_ns_from_seconds(1.0),
                ];
                state.chart_runtime.row_entries = vec![test_row_entry_with_times(
                    &state.chart_runtime.notes,
                    &state.chart_runtime.note_time_cache_ns,
                    row_index,
                    vec![0, 1],
                )];
                state.chart_runtime.row_indices.row_entry_ranges = [(0, 1), (0, 0)];
                state.chart_runtime.row_indices.row_map_cache =
                    std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
                state.chart_runtime.row_indices.row_map_cache[0][row_index] = 0;
                state.chart_runtime.row_indices.note_row_entry_indices = vec![0, 0];
                state.chart_runtime.row_indices.judged_row_cursor = [0; MAX_PLAYERS];
                state.clock.song_position.current_music_time_ns = song_time_ns_from_seconds(1.096);
                state.boundary.total_elapsed_in_screen = 12.0;

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

                assert!(
                    state.players_runtime.players[0]
                        .offset_indicator_text
                        .is_none()
                );
                assert!(state.players_runtime.players[0].error_bar_text.is_none());

                finalize_row_judgment(&mut state, 0, row_index, 0, false);

                let offset = state.players_runtime.players[0]
                    .offset_indicator_text
                    .expect("row-final judgment should drive the offset indicator");
                assert_eq!(offset.started_at, 12.0);
                assert_eq!(offset.offset_ms, 96.0);
                assert_eq!(offset.window, TimingWindow::W4);

                let early_late = state.players_runtime.players[0]
                    .error_bar_text
                    .expect("row-final judgment should drive the early/late text");
                assert_eq!(early_late.started_at, 12.0);
                assert!(!early_late.early);
                assert_eq!(early_late.offset_ms, 96.0);
                assert!(!early_late.scaled);

                let last = state.players_runtime.players[0]
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
        state.boundary.total_elapsed_in_screen = 4.0;

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
        assert!(state.players_runtime.players[0].error_bar_text.is_none());

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

        let text = state.players_runtime.players[0]
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
        state.boundary.total_elapsed_in_screen = 4.0;

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
            state.players_runtime.players[0].error_bar_text.is_none(),
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

        let text = state.players_runtime.players[0]
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
        state.boundary.total_elapsed_in_screen = 4.0;

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
            state.players_runtime.players[0].error_bar_text.is_none(),
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

        let text = state.players_runtime.players[0]
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
        state.boundary.total_elapsed_in_screen = 4.0;

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
            state.players_runtime.players[0].error_bar_text.is_none(),
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

        let text = state.players_runtime.players[0]
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
        state.boundary.total_elapsed_in_screen = 4.0;

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

        let player = &state.players_runtime.players[0];
        assert!(player.error_bar_avg_bar_started_at.is_none());
        assert!(player.error_bar_avg_ticks.iter().all(Option::is_none));
        assert!(player.error_bar_long_avg_visible);
        let long_tick = player
            .error_bar_long_avg_tick
            .expect("long-only Average should still emit the blue tick");
        assert!((long_tick.offset_s - 0.010).abs() <= 1e-6);
    }

    #[test]
    fn long_error_bar_threshold_accounts_for_intensity() {
        // Mean error of 2.5ms is below the 4ms threshold on its own, but with a
        // 2x intensity the effective offset is 5ms, which should show the bar.
        let p1 = profile_data::Profile {
            error_bar_active_mask: profile_data::ErrorBarMask::AVERAGE,
            short_average_error_bar_enabled: false,
            long_error_bar_enabled: true,
            long_error_bar_threshold_ms: 4,
            long_error_bar_min_samples: 4,
            long_error_bar_intensity: 2.0,
            ..profile_data::Profile::default()
        };
        let mut state = regression_state([p1, profile_data::Profile::default()]);
        state.boundary.total_elapsed_in_screen = 4.0;

        for i in 0..4 {
            error_bar_register_tap(
                &mut state,
                0,
                &Judgment {
                    time_error_ms: 2.5,
                    time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(2.5, 1.0),
                    grade: JudgeGrade::Fantastic,
                    window: Some(TimingWindow::W1),
                    miss_because_held: false,
                },
                i as f32 * 0.1,
            );
        }

        let player = &state.players_runtime.players[0];
        assert!(
            player.error_bar_long_avg_visible,
            "intensity should scale the long-term mean past the threshold"
        );
        let long_tick = player
            .error_bar_long_avg_tick
            .expect("blue long-term tick should be emitted once intensity-scaled");
        // The stored offset stays raw; intensity is re-applied at render time.
        assert!((long_tick.offset_s - 0.0025).abs() <= 1e-6);
    }

    #[test]
    fn long_error_bar_stays_hidden_below_intensity_scaled_threshold() {
        // 2.5ms mean with 1x intensity stays below the 4ms threshold.
        let p1 = profile_data::Profile {
            error_bar_active_mask: profile_data::ErrorBarMask::AVERAGE,
            short_average_error_bar_enabled: false,
            long_error_bar_enabled: true,
            long_error_bar_threshold_ms: 4,
            long_error_bar_min_samples: 4,
            long_error_bar_intensity: 1.0,
            ..profile_data::Profile::default()
        };
        let mut state = regression_state([p1, profile_data::Profile::default()]);
        state.boundary.total_elapsed_in_screen = 4.0;

        for i in 0..4 {
            error_bar_register_tap(
                &mut state,
                0,
                &Judgment {
                    time_error_ms: 2.5,
                    time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(2.5, 1.0),
                    grade: JudgeGrade::Fantastic,
                    window: Some(TimingWindow::W1),
                    miss_because_held: false,
                },
                i as f32 * 0.1,
            );
        }

        assert!(!state.players_runtime.players[0].error_bar_long_avg_visible);
    }

    #[test]
    fn short_average_error_bar_applies_single_sample_correction_after_clamp() {
        // Faithful to Average.lua: a single sample is scaled by intensity,
        // clamped to the trim window, and only then multiplied by 0.75. With a
        // large offset the intensity-scaled value saturates the clamp, so the
        // stored offset must be clamp * 0.75 (not a pre-clamp 0.75).
        let p1 = profile_data::Profile {
            error_bar_active_mask: profile_data::ErrorBarMask::AVERAGE,
            short_average_error_bar_enabled: true,
            long_error_bar_enabled: false,
            error_bar_trim: profile_data::ErrorBarTrim::Off,
            average_error_bar_intensity: 2.0,
            ..profile_data::Profile::default()
        };
        let mut state = regression_state([p1, profile_data::Profile::default()]);
        state.boundary.total_elapsed_in_screen = 1.0;
        let max_offset_s = state.timing_runtime.timing_profile.windows_s[4];

        error_bar_register_tap(
            &mut state,
            0,
            &Judgment {
                time_error_ms: 500.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(500.0, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
            0.0,
        );

        let tick = state.players_runtime.players[0]
            .error_bar_avg_ticks
            .iter()
            .flatten()
            .next()
            .copied()
            .expect("a single tap should register a short average tick");
        let expected = max_offset_s * 0.75;
        assert!(
            (tick.offset_s - expected).abs() <= 1e-6,
            "single-sample offset {} should equal clamp(offset * intensity) * 0.75 = {}",
            tick.offset_s,
            expected
        );
    }

    #[test]
    fn autosync_row_hits_use_music_time_offsets_at_rate() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        let row_index = 48usize;
        let autosync_offset_ns = song_time_ns_from_seconds(0.015);

        assert!(super::set_music_rate(&mut state, 1.5));
        state.control.autosync.mode = super::AutosyncMode::Song;
        state.chart_runtime.notes = vec![test_note(0, row_index, NoteType::Tap)];
        state.chart_runtime.notes[0].result = Some(Judgment {
            time_error_ms: -10.0,
            time_error_music_ns: -autosync_offset_ns,
            grade: JudgeGrade::Great,
            window: Some(TimingWindow::W3),
            miss_because_held: false,
        });
        state.chart_runtime.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.control.autosync.offset_samples =
            [autosync_offset_ns; super::AUTOSYNC_OFFSET_SAMPLE_COUNT];
        state.control.autosync.offset_sample_count = super::AUTOSYNC_OFFSET_SAMPLE_COUNT - 1;

        state.apply_autosync_for_row_hits(0);

        assert!((state.song_offset_seconds() - 0.015).abs() <= 1e-6);
        assert_eq!(state.control.autosync.offset_sample_count, 0);
    }

    #[test]
    fn hold_judgment_cleanup_uses_screen_time_boundary() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        state.boundary.total_elapsed_in_screen = 5.0;
        state.display.hold_feedback.hold_judgments[0] = Some(HoldJudgmentRenderInfo {
            result: HoldResult::Held,
            started_at_screen_s: 4.201,
        });
        tick_visual_effects(&mut state, 0.0);
        assert!(state.display.hold_feedback.hold_judgments[0].is_some());

        state.display.hold_feedback.hold_judgments[0] = Some(HoldJudgmentRenderInfo {
            result: HoldResult::Held,
            started_at_screen_s: 4.2,
        });
        tick_visual_effects(&mut state, 0.0);
        assert!(state.display.hold_feedback.hold_judgments[0].is_none());
    }

    #[test]
    fn held_miss_feedback_records_column_and_cleans_up() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        state.boundary.total_elapsed_in_screen = 5.0;
        state.chart_runtime.notes = vec![test_note(2, 48, NoteType::Tap)];
        state.chart_runtime.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            48,
            vec![0],
        )];
        state.chart_runtime.row_indices.note_row_entry_indices = vec![0];

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

        assert!(state.display.hold_feedback.held_miss_judgments[0].is_none());
        assert!(state.display.hold_feedback.held_miss_judgments[1].is_none());
        assert_eq!(
            state.display.hold_feedback.held_miss_judgments[2]
                .as_ref()
                .map(|info| info.started_at_screen_s),
            Some(5.0)
        );

        state.display.hold_feedback.held_miss_judgments[2] = Some(HeldMissRenderInfo {
            started_at_screen_s: 5.0 - HELD_MISS_TOTAL_DURATION,
        });
        tick_visual_effects(&mut state, 0.0);
        assert!(state.display.hold_feedback.held_miss_judgments[2].is_none());
    }

    #[test]
    fn mine_judgment_feedback_records_result_column_and_time() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        state.boundary.total_elapsed_in_screen = 9.25;

        super::set_last_mine_judgment(&mut state, 0, 2, MineResult::Avoided);

        let info = state.players_runtime.players[0]
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
        state.trigger_receptor_step_pulse(0);
        let supports_press_tween =
            state.display.receptor_feedback.glow_press_timers[0] > f32::EPSILON;
        state.display.receptor_feedback.glow_press_timers.fill(0.0);
        state.display.receptor_feedback.glow_lift_timers.fill(0.0);
        state.display.receptor_feedback.bop_timers.fill(0.0);
        state.chart_runtime.notes = vec![note_with_judgment(
            column,
            row_index,
            NoteType::Tap,
            JudgeGrade::Great,
            0.0,
        )];
        state.chart_runtime.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.chart_runtime.row_indices.row_map_cache =
            std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.chart_runtime.row_indices.row_map_cache[0][row_index] = 0;
        state.mods.song_lua_visuals.note_hides[0].push(SongLuaNoteHideWindowRuntime {
            column,
            start_beat: 0.0,
            end_beat: 2.0,
        });

        trigger_completed_row_tap_explosions(&mut state, 0, row_index);

        assert!(state.display.visual_feedback.tap_explosions[column].is_none());
        assert_eq!(state.display.receptor_feedback.bop_timers[column], 0.0);
        assert_eq!(
            state.display.receptor_feedback.bop_behaviors[column].duration,
            0.0
        );
        if supports_press_tween {
            assert!(state.display.receptor_feedback.glow_press_timers[column] > 0.0);
            assert_eq!(
                state.display.receptor_feedback.glow_lift_timers[column],
                0.0
            );
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
        state.chart_runtime.notes = vec![test_note(column, row_index, NoteType::Tap)];
        state.chart_runtime.note_time_cache_ns = vec![note_time];
        state.chart_runtime.lane_indices.note_indices[column].push(0);
        state.chart_runtime.lane_indices.note_row_indices[column].push(0);
        state.chart_runtime.row_indices.note_row_entry_indices = vec![0];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.chart_runtime.row_indices.row_entry_ranges = [(0, 1), (0, 0)];
        state.chart_runtime.row_indices.row_map_cache =
            std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.chart_runtime.row_indices.row_map_cache[0][row_index] = 0;

        assert!(judge_a_tap(&mut state, column, note_time));
        assert!(state.display.visual_feedback.tap_explosions[column].is_some());
        assert_eq!(state.display.receptor_feedback.bop_timers[column], 0.0);
        assert_eq!(
            state.display.receptor_feedback.bop_behaviors[column].duration,
            0.0
        );
    }

    #[test]
    fn devcel_visible_tap_hit_uses_score_receptor_command() {
        let mut profile = profile_data::Profile::default();
        profile.noteskin = profile_data::NoteSkin::new("devcel-2024");
        let mut state = regression_state([profile, profile_data::Profile::default()]);
        let row_index = 48usize;
        let column = 1usize;
        let note_time = song_time_ns_from_seconds(1.0);
        state.chart_runtime.notes = vec![test_note(column, row_index, NoteType::Tap)];
        state.chart_runtime.note_time_cache_ns = vec![note_time];
        state.chart_runtime.lane_indices.note_indices[column].push(0);
        state.chart_runtime.lane_indices.note_row_indices[column].push(0);
        state.chart_runtime.row_indices.note_row_entry_indices = vec![0];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.chart_runtime.row_indices.row_entry_ranges = [(0, 1), (0, 0)];
        state.chart_runtime.row_indices.row_map_cache =
            std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.chart_runtime.row_indices.row_map_cache[0][row_index] = 0;

        assert!(judge_a_tap(&mut state, column, note_time));
        assert!(state.display.visual_feedback.tap_explosions[column].is_some());
        assert_eq!(state.display.receptor_feedback.bop_timers[column], 0.0);
        assert_eq!(
            state.display.receptor_feedback.bop_behaviors[column].duration,
            0.0
        );
    }

    #[test]
    fn tap_explosion_mask_disables_selected_tap_window() {
        let column = 1usize;
        let mut enabled = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        trigger_tap_explosion(&mut enabled, column, JudgeGrade::Great);
        assert!(enabled.display.visual_feedback.tap_explosions[column].is_some());

        let mut profile = profile_data::Profile::default();
        profile
            .tap_explosion_active_mask
            .remove(profile_data::TapExplosionMask::GREAT);
        let mut disabled = regression_state([profile, profile_data::Profile::default()]);
        trigger_tap_explosion(&mut disabled, column, JudgeGrade::Great);
        assert!(disabled.display.visual_feedback.tap_explosions[column].is_none());
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
            state.chart_runtime.notes = vec![note_with_judgment(
                column,
                row_index,
                NoteType::Tap,
                JudgeGrade::Great,
                0.0,
            )];
            state.chart_runtime.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
            state.chart_runtime.row_entries = vec![test_row_entry_with_times(
                &state.chart_runtime.notes,
                &state.chart_runtime.note_time_cache_ns,
                row_index,
                vec![0],
            )];
            state.chart_runtime.row_indices.row_map_cache =
                std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
            state.chart_runtime.row_indices.row_map_cache[0][row_index] = 0;
            state
        };

        let mut disabled = build_state(profile_data::ColumnFlashMask::EXCELLENT);
        trigger_completed_row_tap_explosions(&mut disabled, 0, row_index);
        assert!(disabled.display.visual_feedback.column_flashes[column].is_none());
        // The ungated SMX feedback record is written even when the on-screen column flash is
        // masked off; the pad lighting relies on this decoupling.
        let judged = disabled.display.visual_feedback.last_tap_judgments[column]
            .expect("masked column flash should still record the ungated tap judgment");
        assert_eq!(judged.grade, JudgeGrade::Great);

        let mut enabled = build_state(profile_data::ColumnFlashMask::GREAT);
        trigger_completed_row_tap_explosions(&mut enabled, 0, row_index);
        let flash = enabled.display.visual_feedback.column_flashes[column]
            .expect("Great should trigger column flash");
        assert_eq!(flash.grade, JudgeGrade::Great);
    }

    #[test]
    fn mine_hit_records_screen_time_and_refreshes_on_rehit() {
        let column = 1usize;
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        state.setup.config.mine_hit_sound = false;
        // A hit records the current screen time on the explosion.
        state.boundary.total_elapsed_in_screen = 1.0;
        trigger_mine_explosion(&mut state, column);
        let first = state.display.visual_feedback.mine_explosions[column]
            .as_ref()
            .expect("mine hit should set an explosion");
        assert_eq!(first.started_at_screen_s, 1.0);
        // A later hit on the same column refreshes the timestamp even while the explosion is
        // still present, which is what lets the SMX panel diff tell consecutive hits apart.
        state.boundary.total_elapsed_in_screen = 1.5;
        trigger_mine_explosion(&mut state, column);
        let second = state.display.visual_feedback.mine_explosions[column]
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
        state.chart_runtime.notes = vec![note];
        state.chart_runtime.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.chart_runtime.row_indices.row_map_cache =
            std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.chart_runtime.row_indices.row_map_cache[0][row_index] = 0;
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
        assert!(disabled.display.visual_feedback.column_flashes[column].is_none());

        let (mut enabled, row_index, column) = fantastic_row_state(
            profile_data::ColumnFlashMask::WHITE_FANTASTIC,
            18.0,
            TimingWindow::W1,
        );
        trigger_completed_row_tap_explosions(&mut enabled, 0, row_index);
        let flash = enabled.display.visual_feedback.column_flashes[column]
            .expect("white Fantastic should flash");
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
        assert!(disabled.display.visual_feedback.column_flashes[column].is_none());

        let (mut enabled, row_index, column) = fantastic_row_state(
            profile_data::ColumnFlashMask::BLUE_FANTASTIC,
            4.0,
            TimingWindow::W0,
        );
        trigger_completed_row_tap_explosions(&mut enabled, 0, row_index);
        let flash = enabled.display.visual_feedback.column_flashes[column]
            .expect("blue Fantastic should flash");
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
        assert!(hide_notefield.display.visual_feedback.tap_explosions[column].is_none());
        assert!(hide_notefield.display.visual_feedback.column_flashes[column].is_some());

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
        assert!(hide_column.display.visual_feedback.tap_explosions[column].is_some());
        assert!(hide_column.display.visual_feedback.column_flashes[column].is_none());
    }

    #[test]
    fn tap_explosion_mask_disables_held_success_flash() {
        let column = 1usize;
        let mut enabled = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        trigger_hold_explosion(&mut enabled, column);
        assert!(enabled.display.visual_feedback.tap_explosions[column].is_some());

        let mut profile = profile_data::Profile::default();
        profile
            .tap_explosion_active_mask
            .remove(profile_data::TapExplosionMask::HELD);
        let mut disabled = regression_state([profile, profile_data::Profile::default()]);
        trigger_hold_explosion(&mut disabled, column);
        assert!(disabled.display.visual_feedback.tap_explosions[column].is_none());
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
        state.chart_runtime.notes = vec![note];
        state.chart_runtime.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.chart_runtime.row_indices.row_map_cache =
            std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.chart_runtime.row_indices.row_map_cache[0][row_index] = 0;

        trigger_completed_row_tap_explosions(&mut state, 0, row_index);

        let active = state.display.visual_feedback.tap_explosions[column]
            .expect("white Fantastic should flash");
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
        state.chart_runtime.notes = vec![note];
        state.chart_runtime.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.chart_runtime.row_indices.row_map_cache =
            std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.chart_runtime.row_indices.row_map_cache[0][row_index] = 0;

        trigger_completed_row_tap_explosions(&mut state, 0, row_index);

        let active = state.display.visual_feedback.tap_explosions[column]
            .expect("blue Fantastic should flash");
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

        assert!(state.tap_judgment_uses_bright_explosion(0, &judgment));
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

        assert!(!state.tap_judgment_uses_bright_explosion(0, &judgment));
    }

    #[test]
    fn synthetic_receptor_step_survives_until_lift() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        let column = 0usize;

        state.trigger_receptor_step_pulse(column);
        let started_press = state.display.receptor_feedback.glow_press_timers[column];
        if started_press <= f32::EPSILON {
            assert!(state.display.receptor_feedback.bop_timers[column] > 0.0);
            return;
        }
        tick_visual_effects(&mut state, 0.01);

        if started_press > 0.01 {
            assert!(state.display.receptor_feedback.glow_press_timers[column] > 0.0);
            assert!(state.display.receptor_feedback.glow_press_timers[column] < started_press);
        }
        tick_visual_effects(&mut state, started_press.max(0.01));

        assert_eq!(
            state.display.receptor_feedback.glow_press_timers[column],
            0.0
        );
        assert!(state.display.receptor_feedback.glow_lift_timers[column] > 0.0);
    }

    #[test]
    fn course_display_carry_captures_current_life() {
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        state.players_runtime.players[0].life = 0.32;

        let carry = state.course_display_carry();

        assert!((carry[0].life - 0.32).abs() <= f32::EPSILON);
    }

    #[test]
    fn autoplay_rows_do_not_record_ex_counts() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        let row_index = 48usize;
        state.chart_runtime.notes = vec![test_note(0, row_index, NoteType::Tap)];
        state.chart_runtime.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.chart_runtime.row_indices.row_entry_ranges = [(0, 1), (0, 0)];
        state.chart_runtime.row_indices.note_row_entry_indices = vec![0];
        state.progress.stage.autoplay_enabled = true;

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

        assert_eq!(state.live_window_counts(0).w0, 0);
        let blue_window_ms = state.player_blue_window_ms(0);
        assert_eq!(state.display_ex_score_percent(0, blue_window_ms), 0.0);
        assert_eq!(state.display_itg_score_percent(0), 0.0);
        assert!(state.players_runtime.players[0].last_judgment.is_some());
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
    fn chart_attack_sudden_offset_approaches_instead_of_snapping() {
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        state.mods.attacks.mask_windows[0] = build_attack_mask_windows_for_player(
            Some(
                "TIME=0.000:LEN=3.000:MODS=*1000 sudden,*1000 -125% suddenoffset\
                 :TIME=0.083:LEN=3.000:MODS=*2.4 150% suddenoffset",
            ),
            profile_data::AttackMode::On,
            0,
            0x1234,
            10.0,
        );

        state.clock.visible_timing.current_music_time[0] = 0.01;
        refresh_active_attack_masks(&mut state, 0.01);
        let start = state.effective_appearance_effects_for_player(0);
        assert!((start.sudden - 1.0).abs() <= 1e-6);
        assert!((start.sudden_offset + 1.25).abs() <= 1e-6);

        state.clock.visible_timing.current_music_time[0] = 0.10;
        refresh_active_attack_masks(&mut state, 0.09);
        let mid = state.effective_appearance_effects_for_player(0);
        assert!(mid.sudden_offset > -1.25);
        assert!(mid.sudden_offset < 1.5);

        state.clock.visible_timing.current_music_time[0] = 1.10;
        refresh_active_attack_masks(&mut state, 1.0);
        let late = state.effective_appearance_effects_for_player(0);
        assert!(late.sudden_offset > mid.sudden_offset);
        assert!(late.sudden_offset < 1.5);
    }

    #[test]
    fn chart_attack_runtime_mods_stop_after_len() {
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        state.mods.attacks.mask_windows[0] = build_attack_mask_windows_for_player(
            Some("TIME=0.000:LEN=1.000:MODS=50% drunk"),
            profile_data::AttackMode::On,
            0,
            0x1234,
            10.0,
        );

        state.clock.visible_timing.current_music_time[0] = 2.0;
        refresh_active_attack_masks(&mut state, 0.0);

        let visual = effective_visual_effects_for_player(&state, 0);
        assert!(visual.drunk.abs() <= 0.000_1);
    }

    #[test]
    fn outro_attack_clear_phases_out_song_lua_visual_mods() {
        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        state.mods.attacks.visual[0].confusion_offset = Some(-12.56);
        state.mods.attacks.visual[0].tipsy = Some(0.75);
        state.mods.attacks.visibility[0].dark = Some(1.0);

        state.begin_outro_attack_clear();
        refresh_active_attack_masks(&mut state, 0.5);

        let visual = effective_visual_effects_for_player(&state, 0);
        let visibility = state.effective_visibility_effects_for_player(0);
        assert!(visual.confusion_offset > -12.56);
        assert!(visual.confusion_offset < -12.0);
        assert!(visual.tipsy > 0.0);
        assert!(visual.tipsy < 0.75);
        assert!((visibility.dark - 1.0).abs() <= 0.0001);

        refresh_active_attack_masks(&mut state, 20.0);

        let cleared = effective_visual_effects_for_player(&state, 0);
        let visibility = state.effective_visibility_effects_for_player(0);
        assert!(cleared.confusion_offset.abs() <= 0.0001);
        assert!(cleared.tipsy.abs() <= 0.0001);
        assert!(state.mods.attacks.visual[0].confusion_offset.is_none());
        assert!(state.mods.attacks.visual[0].tipsy.is_none());
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
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);
        assert_eq!(unsupported, 0);
        state.mods.attacks.song_lua_ease_windows[0] = windows;
        state.clock.visible_timing.current_music_time[0] = 2.5;

        state.begin_outro_attack_clear();
        refresh_active_attack_masks(&mut state, 0.0);

        assert!((state.mods.song_lua_player_transforms[0].rotation_z - 5.0).abs() <= 0.0001);
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
                            delta: deadsync_song_lua::SongLuaOverlayStateDelta {
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
                from: deadsync_song_lua::SongLuaOverlayStateDelta {
                    diffuse: Some([1.0, 1.0, 1.0, 0.0]),
                    ..Default::default()
                },
                to: deadsync_song_lua::SongLuaOverlayStateDelta {
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

        let windows = deadsync_gameplay::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

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
                            delta: deadsync_song_lua::SongLuaOverlayStateDelta {
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
                from: deadsync_song_lua::SongLuaOverlayStateDelta {
                    zoom: Some(1.5),
                    ..Default::default()
                },
                to: deadsync_song_lua::SongLuaOverlayStateDelta {
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

        let windows = deadsync_gameplay::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

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
                            delta: deadsync_song_lua::SongLuaOverlayStateDelta {
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
                from: deadsync_song_lua::SongLuaOverlayStateDelta {
                    diffuse: Some([0.0, 0.0, 0.0, 0.0]),
                    ..Default::default()
                },
                to: deadsync_song_lua::SongLuaOverlayStateDelta {
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

        let windows = deadsync_gameplay::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

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
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);

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
            build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);
        let (windows, unsupported) =
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &constants);

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
            build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);
        let (windows, unsupported) =
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &constants);

        assert_eq!(unsupported, 0);
        let stealth = windows
            .iter()
            .find(|window| {
                matches!(
                    window.target,
                    super::SongLuaEaseMaskTarget::AppearanceStealth
                )
            })
            .unwrap();
        let pulse_outer = windows
            .iter()
            .find(|window| {
                matches!(
                    window.target,
                    super::SongLuaEaseMaskTarget::VisualPulseOuter
                )
            })
            .unwrap();
        let pulse_period = windows
            .iter()
            .find(|window| {
                matches!(
                    window.target,
                    super::SongLuaEaseMaskTarget::VisualPulsePeriod
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

        state.mods.attacks.mask_windows[0] = constants;
        state.mods.attacks.song_lua_ease_windows[0] = windows;
        state.clock.visible_timing.current_music_time[0] = 5.99;
        refresh_active_attack_masks(&mut state, 0.0);
        let eased_stealth = state.effective_appearance_effects_for_player(0).stealth;
        assert!(eased_stealth > 0.4);
        assert!(effective_visual_effects_for_player(&state, 0).pulse_outer > 0.0);

        state.clock.visible_timing.current_music_time[0] = 6.016;
        refresh_active_attack_masks(&mut state, 0.026);
        let fading_stealth = state.effective_appearance_effects_for_player(0).stealth;
        assert!(fading_stealth > 0.0);
        assert!(fading_stealth < eased_stealth);

        state.clock.visible_timing.current_music_time[0] = 7.0;
        refresh_active_attack_masks(&mut state, 0.984);
        assert!(
            state
                .effective_appearance_effects_for_player(0)
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
        state.mods.attacks.mask_windows[0] =
            build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);

        state.clock.visible_timing.current_music_time[0] = 2.0;
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
        state.mods.attacks.mask_windows[0] =
            build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);

        state.clock.visible_timing.current_music_time[0] = 0.016;
        refresh_active_attack_masks(&mut state, 0.016);
        let visual = effective_visual_effects_for_player(&state, 0);
        let scroll = effective_scroll_effects_for_player(&state, 0);
        let mini = effective_mini_percent_for_player(&state, 0);
        assert!(visual.flip > 0.0);
        assert!(visual.flip < 0.5);
        assert!((scroll.reverse - 0.1).abs() <= 0.000_1);
        assert!(mini < 0.0);
        assert!(mini > -100.0);

        state.clock.visible_timing.current_music_time[0] = 1.016;
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
        state.mods.attacks.mask_windows[0] =
            build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);

        state.clock.visible_timing.current_music_time[0] = 0.016;
        refresh_active_attack_masks(&mut state, 0.016);
        let mini = effective_mini_percent_for_player(&state, 0);
        assert!((mini - 34.0).abs() <= 0.000_1);

        state.clock.visible_timing.current_music_time[0] = 1.016;
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
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);
        assert_eq!(unsupported, 0);
        state.mods.attacks.song_lua_ease_windows[0] = windows;

        state.clock.visible_timing.current_music_time[0] = 2.0;
        refresh_active_attack_masks(&mut state, 0.0);

        let mini = effective_mini_percent_for_player(&state, 0);
        assert!(mini.abs() <= 0.000_1);
    }

    #[test]
    fn chart_attack_mini_overrides_profile_mini() {
        let mut profiles = std::array::from_fn(|_| profile_data::Profile::default());
        profiles[0].mini_percent = 50;
        let mut state = regression_state(profiles);
        state.mods.attacks.mask_windows[0] = build_attack_mask_windows_for_player(
            Some("TIME=0.000:LEN=3.000:MODS=*1000 25% mini"),
            profile_data::AttackMode::On,
            0,
            0x1234,
            10.0,
        );

        state.clock.visible_timing.current_music_time[0] = 1.0;
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
        state.mods.attacks.mask_windows[0] =
            build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);

        state.clock.visible_timing.current_music_time[0] = 0.6;
        refresh_active_attack_masks(&mut state, 0.0);
        let visual = effective_visual_effects_for_player(&state, 0);
        assert!((visual.flip - 1.0).abs() <= 0.000_1);
        assert!(visual.invert.abs() <= 0.000_1);

        state.clock.visible_timing.current_music_time[0] = 1.1;
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
        let context = deadsync_gameplay::test_song_lua_double_context(&root, "Riddle");
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
        state.mods.attacks.mask_windows[0] =
            build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);

        state.clock.visible_timing.current_music_time[0] = timing.get_time_for_beat(70.75);
        refresh_active_attack_masks(&mut state, 0.0);
        let tilted = effective_visual_effects_for_player(&state, 0);
        assert!((tilted.confusion_offset - 0.8).abs() <= 0.000_1);

        state.clock.visible_timing.current_music_time[0] = timing.get_time_for_beat(71.25);
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
        let context = deadsync_gameplay::test_song_lua_double_context(&root, "KENPO SAITO");
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
            build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);
        let (windows, unsupported) =
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &constants);
        assert_eq!(unsupported, 0);
        assert!(windows.iter().any(|window| {
            matches!(window.target, super::SongLuaEaseMaskTarget::PlayerSkewX)
                && (window.start_second - timing.get_time_for_beat(166.0)).abs() <= 0.001
                && (window.to.abs() - 0.03).abs() <= 0.000_1
        }));
        assert!(windows.iter().any(|window| {
            matches!(window.target, super::SongLuaEaseMaskTarget::PlayerSkewX)
                && (window.start_second - timing.get_time_for_beat(182.0)).abs() <= 0.001
                && (window.to.abs() - 0.03).abs() <= 0.000_1
        }));
        assert!(windows.iter().any(|window| {
            matches!(window.target, super::SongLuaEaseMaskTarget::PlayerRotationX)
                && (window.start_second - timing.get_time_for_beat(189.0)).abs() <= 0.001
                && (window.to - 20.0).abs() <= 0.000_1
        }));

        let mut state = regression_state(std::array::from_fn(|_| profile_data::Profile::default()));
        state.mods.attacks.mask_windows[0] = constants;
        state.mods.attacks.song_lua_ease_windows[0] = windows;

        state.clock.visible_timing.current_music_time[0] = timing.get_time_for_beat(27.25);
        refresh_active_attack_masks(&mut state, 0.0);
        let pre_flash_visual = effective_visual_effects_for_player(&state, 0);
        assert!((pre_flash_visual.tiny + 1.0).abs() <= 0.000_1);
        assert!((pre_flash_visual.flip - 0.25).abs() <= 0.000_1);

        state.clock.visible_timing.current_music_time[0] = timing.get_time_for_beat(29.0);
        refresh_active_attack_masks(&mut state, 0.0);
        let hidden_visibility = state.effective_visibility_effects_for_player(0);
        let reset_visual = effective_visual_effects_for_player(&state, 0);
        assert!((hidden_visibility.dark - 1.0).abs() <= 0.000_1);
        assert!(reset_visual.tiny.abs() <= 0.000_1);
        assert!(reset_visual.flip.abs() <= 0.000_1);

        state.clock.visible_timing.current_music_time[0] = timing.get_time_for_beat(31.0);
        refresh_active_attack_masks(&mut state, 0.0);
        let fading_visibility = state.effective_visibility_effects_for_player(0);
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
            build_song_lua_column_offset_windows_for_player(&compiled, &timing, 0, 0.0);

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
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);

        assert_eq!(unsupported, 0);
        assert_eq!(windows.len(), 7);
        assert!(matches!(
            windows[0].target,
            super::SongLuaEaseMaskTarget::PlayerX
        ));
        assert!(matches!(
            windows[1].target,
            super::SongLuaEaseMaskTarget::PlayerY
        ));
        assert!(matches!(
            windows[2].target,
            super::SongLuaEaseMaskTarget::PlayerZ
        ));
        assert!(matches!(
            windows[3].target,
            super::SongLuaEaseMaskTarget::PlayerRotationX
        ));
        assert!(matches!(
            windows[4].target,
            super::SongLuaEaseMaskTarget::PlayerSkewY
        ));
        assert!(matches!(
            windows[5].target,
            super::SongLuaEaseMaskTarget::PlayerZoom
        ));
        assert!(matches!(
            windows[6].target,
            super::SongLuaEaseMaskTarget::PlayerZoomZ
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
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);

        assert_eq!(unsupported, 0);
        assert_eq!(windows.len(), 2);
        assert!(matches!(
            windows[0].target,
            super::SongLuaEaseMaskTarget::PlayerSkewX
        ));
        assert!(matches!(
            windows[1].target,
            super::SongLuaEaseMaskTarget::PlayerSkewY
        ));
        assert!((windows[0].to - 0.03).abs() <= 0.000_1);
        assert!((windows[1].to + 0.04).abs() <= 0.000_1);
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
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0, &[]);

        assert_eq!(unsupported, 0);
        assert_eq!(windows.len(), 1);
        assert!(matches!(
            windows[0].target,
            super::SongLuaEaseMaskTarget::VisualConfusionOffset
        ));
        assert!((windows[0].from + 0.85).abs() <= 0.000_1);
        assert!(windows[0].to.abs() <= 0.000_1);
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
        tap_state.chart_runtime.note_time_cache_ns[0] = note_time_ns;
        let miss_distance_ns = max_step_distance_ns(
            &tap_state.timing_runtime.timing_profile,
            tap_state.music_rate(),
        );
        let inside_delay_music_time = note_time_ns
            .saturating_add(miss_distance_ns)
            .saturating_add(song_time_ns_from_seconds(0.5));
        apply_time_based_tap_misses(&mut tap_state, inside_delay_music_time);
        assert!(tap_state.chart_runtime.notes[0].result.is_none());
        assert_eq!(tap_state.chart_runtime.mine_scan.next_tap_miss_cursor[0], 0);

        let after_delay_music_time = note_time_ns
            .saturating_add(miss_distance_ns)
            .saturating_add(song_time_ns_from_seconds(2.1));
        apply_time_based_tap_misses(&mut tap_state, after_delay_music_time);
        assert_eq!(
            tap_state.chart_runtime.notes[0]
                .result
                .as_ref()
                .map(|j| j.grade),
            Some(JudgeGrade::Miss)
        );

        let mut mine_state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        set_state_timing(&mut mine_state, Arc::clone(&timing));
        set_regression_mine(&mut mine_state, 0, 0, ROWS_PER_BEAT as usize, note_time_ns);
        let mine_distance_ns = max_step_distance_ns(
            &mine_state.timing_runtime.timing_profile,
            mine_state.music_rate(),
        );
        let inside_delay_music_time = note_time_ns
            .saturating_add(mine_distance_ns)
            .saturating_add(song_time_ns_from_seconds(0.5));
        apply_time_based_mine_avoidance(&mut mine_state, inside_delay_music_time);
        assert_eq!(mine_state.chart_runtime.notes[0].mine_result, None);
        assert_eq!(mine_state.chart_runtime.mine_scan.next_mine_ix_cursor[0], 0);

        let after_delay_music_time = note_time_ns
            .saturating_add(mine_distance_ns)
            .saturating_add(song_time_ns_from_seconds(2.1));
        apply_time_based_mine_avoidance(&mut mine_state, after_delay_music_time);
        assert_eq!(
            mine_state.chart_runtime.notes[0].mine_result,
            Some(MineResult::Avoided)
        );
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
            &state.timing_runtime.timing_profile,
            state.music_rate(),
        ));

        apply_time_based_mine_avoidance(&mut state, end_time_ns);
        assert_eq!(state.players_runtime.players[0].mines_avoided, 0);
        assert_eq!(state.chart_runtime.notes[0].mine_result, None);

        finalize_completed_mines(&mut state);
        assert_eq!(state.players_runtime.players[0].mines_avoided, 1);
        assert_eq!(
            state.chart_runtime.notes[0].mine_result,
            Some(MineResult::Avoided)
        );
    }

    #[test]
    fn completed_song_finalizes_last_tap_miss_before_eval() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        assert_eq!(state.setup.num_players, 1);

        let (note_start, note_end) = state.note_range_for_player(0);
        let first_note = note_start;
        let last_note = note_end - 1;
        let first_row_entry =
            state.chart_runtime.row_indices.note_row_entry_indices[first_note] as usize;
        let last_row_entry =
            state.chart_runtime.row_indices.note_row_entry_indices[last_note] as usize;
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
        state.clock.song_position.current_music_time_ns =
            state.chart_runtime.note_time_cache_ns[first_note].saturating_add(
                max_step_distance_ns(&state.timing_runtime.timing_profile, state.music_rate()),
            );
        assert!(!settle_completion_rows(&mut state));
        assert!(
            state.chart_runtime.row_entries[first_row_entry]
                .final_outcome
                .is_some()
        );
        assert!(
            state.chart_runtime.row_entries[last_row_entry]
                .final_outcome
                .is_none()
        );

        let miss_time_ns = state.chart_runtime.note_time_cache_ns[last_note]
            .saturating_add(max_step_distance_ns(
                &state.timing_runtime.timing_profile,
                state.music_rate(),
            ))
            .saturating_add(song_time_ns_from_seconds(0.1));
        state.clock.song_position.current_music_time_ns = miss_time_ns;

        // The normal frame order has already scanned rows before overdue taps
        // are promoted to misses.
        update_judged_rows(&mut state);
        apply_time_based_tap_misses(&mut state, miss_time_ns);
        assert_eq!(
            state.chart_runtime.notes[last_note]
                .result
                .as_ref()
                .map(|j| j.grade),
            Some(JudgeGrade::Miss)
        );
        assert!(
            state.chart_runtime.row_entries[last_row_entry]
                .final_outcome
                .is_none()
        );
        assert_eq!(state.players_runtime.players[0].judgment_counts[miss_ix], 0);

        assert!(settle_completion_rows(&mut state));
        assert_eq!(
            state.chart_runtime.row_entries[last_row_entry].final_outcome,
            Some(FinalizedRowOutcome {
                final_grade: JudgeGrade::Miss,
            })
        );
        assert_eq!(state.players_runtime.players[0].judgment_counts[miss_ix], 1);
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

        assert_eq!(
            state.chart_runtime.notes[0].mine_result,
            Some(MineResult::Hit)
        );
        assert_eq!(
            state.chart_runtime.mine_scan.pending_mine_hit_indices,
            vec![0]
        );
        assert_eq!(state.players_runtime.players[0].mines_hit, 0);
        assert_eq!(state.players_runtime.players[0].mines_hit_for_score, 0);

        apply_pending_mine_hits(&mut state);

        assert_eq!(state.players_runtime.players[0].mines_hit, 1);
        assert_eq!(state.players_runtime.players[0].mines_hit_for_score, 1);
    }

    #[test]
    fn mine_hit_side_effects_wait_until_after_active_holds() {
        let profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        let mut state = regression_state(profiles);
        let hold_end_ns = song_time_ns_from_seconds(1.0);
        state.chart_runtime.notes[0] = test_hold(0, 0, ROWS_PER_BEAT as usize);
        state.chart_runtime.hold_end_time_cache_ns[0] = Some(hold_end_ns);
        set_regression_mine(&mut state, 1, 1, ROWS_PER_BEAT as usize, hold_end_ns);
        state.players_runtime.players[0].life = 0.04;
        state.hold_runtime.active_holds[0] = Some(super::ActiveHold {
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
        assert_eq!(
            state.chart_runtime.notes[1].mine_result,
            Some(MineResult::Hit)
        );
        assert_eq!(state.players_runtime.players[0].mines_hit, 0);
        assert!(!state.players_runtime.players[0].is_failing);

        let inputs = std::array::from_fn(|col| col == 0);
        update_active_holds(&mut state, &inputs, hold_end_ns);
        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|hold| hold.result),
            Some(HoldResult::Held)
        );
        assert_eq!(state.players_runtime.players[0].holds_held_for_score, 1);
        assert!(!state.players_runtime.players[0].is_failing);

        apply_pending_mine_hits(&mut state);
        assert_eq!(state.players_runtime.players[0].mines_hit, 1);
        assert_eq!(state.players_runtime.players[0].mines_hit_for_score, 0);
        assert!(state.players_runtime.players[0].is_failing);
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
        state.progress.stage.score_missed_holds_rolls[0] = true;
        state.chart_runtime.notes[0] = test_hold(0, 48, 96);
        state.chart_runtime.note_time_cache_ns[0] = note_time_ns;
        state.chart_runtime.hold_end_time_cache_ns[0] = Some(hold_end_ns);
        state.chart_runtime.notes[1].can_be_judged = false;

        let miss_time_ns = note_time_ns
            .saturating_add(max_step_distance_ns(
                &state.timing_runtime.timing_profile,
                state.music_rate(),
            ))
            .saturating_add(song_time_ns_from_seconds(0.1));
        apply_time_based_tap_misses(&mut state, miss_time_ns);

        assert_eq!(
            state.chart_runtime.notes[0]
                .result
                .as_ref()
                .map(|judgment| judgment.grade),
            Some(JudgeGrade::Miss)
        );
        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|h| h.result),
            None
        );
        assert_eq!(state.players_runtime.players[0].holds_let_go_for_score, 0);

        resolve_pending_missed_holds(&mut state, hold_end_ns.saturating_sub(1));
        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|h| h.result),
            None
        );
        assert_eq!(state.players_runtime.players[0].holds_let_go_for_score, 0);

        resolve_pending_missed_holds(&mut state, hold_end_ns);

        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|hold| hold.result),
            Some(HoldResult::LetGo)
        );
        assert_eq!(state.players_runtime.players[0].holds_let_go_for_score, 1);
        assert_eq!(
            state.display.hold_feedback.hold_judgments[0]
                .as_ref()
                .map(|info| info.result),
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
        state.progress.stage.score_missed_holds_rolls[0] = false;
        state.chart_runtime.notes[0] = test_hold(0, 48, 96);
        state.chart_runtime.note_time_cache_ns[0] = note_time_ns;
        state.chart_runtime.hold_end_time_cache_ns[0] = Some(hold_end_ns);
        state.chart_runtime.notes[1].can_be_judged = false;

        let miss_time_ns = note_time_ns
            .saturating_add(max_step_distance_ns(
                &state.timing_runtime.timing_profile,
                state.music_rate(),
            ))
            .saturating_add(song_time_ns_from_seconds(0.1));
        apply_time_based_tap_misses(&mut state, miss_time_ns);

        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|hold| hold.result),
            Some(HoldResult::Missed)
        );
        assert_eq!(state.players_runtime.players[0].holds_let_go_for_score, 0);
        assert!(state.display.hold_feedback.hold_judgments[0].is_none());

        resolve_pending_missed_holds(&mut state, hold_end_ns);

        assert_eq!(state.players_runtime.players[0].holds_let_go_for_score, 0);
        assert_eq!(
            state.display.hold_feedback.hold_judgments[0]
                .as_ref()
                .map(|info| info.result),
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
        assert_eq!(state.chart_runtime.notes[0].mine_result, None);
    }

    #[test]
    fn set_music_rate_rebuilds_judgment_and_end_times() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        let baseline_great_ns = state.timing_runtime.player_judgment_timing[0]
            .profile_music_ns
            .windows_ns[2];
        let baseline_notes_end = state.notes_end_time_ns();
        let baseline_music_end = state.music_end_time_ns();

        assert!(super::set_music_rate(&mut state, 1.5));
        assert!((state.music_rate() - 1.5).abs() < 1e-6);

        let scaled_great_ns = state.timing_runtime.player_judgment_timing[0]
            .profile_music_ns
            .windows_ns[2];
        // Scaled timing windows are larger in music time when the rate is faster.
        assert!(
            scaled_great_ns > baseline_great_ns,
            "music-rate=1.5 should widen the W3 window in song-time ns ({} vs {})",
            scaled_great_ns,
            baseline_great_ns,
        );
        assert!(
            state.notes_end_time_ns() > baseline_notes_end,
            "music-rate=1.5 should also widen the late-resolution slack on the note end time \
             ({} vs {})",
            state.notes_end_time_ns(),
            baseline_notes_end,
        );
        assert_eq!(state.music_end_time_ns(), baseline_music_end);

        // Calling with the same rate is a no-op.
        assert!(!super::set_music_rate(&mut state, 1.5));

        // Non-finite or non-positive inputs are normalized to 1.0.
        assert!(super::set_music_rate(&mut state, f32::NAN));
        assert!((state.music_rate() - 1.0).abs() < 1e-6);

        assert!(super::set_music_rate(&mut state, 1.5));
        assert!(super::set_music_rate(&mut state, -2.0));
        assert!((state.music_rate() - 1.0).abs() < 1e-6);
    }
}





