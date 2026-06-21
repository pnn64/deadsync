use deadsync_core::input::{MAX_COLS, MAX_PLAYERS};
use deadsync_core::note::NoteType;
pub(crate) use deadsync_core::song_time::{
    INVALID_SONG_TIME_NS, SongTimeNs, normalized_song_rate, song_time_ns_from_seconds,
    song_time_ns_invalid, song_time_ns_to_seconds,
};
#[cfg(test)]
use deadsync_gameplay::effective_visual_effects_for_player;
pub(crate) use deadsync_gameplay::song_lua_ease_factor;
pub use deadsync_gameplay::{
    ASSIST_TICK_LOOKAHEAD_MARGIN_SECONDS, AUTOSYNC_OFFSET_SAMPLE_COUNT,
    AUTOSYNC_STDDEV_MAX_SECONDS, AccelEffects, AccelOverrides, ActiveColumnFlash,
    ActiveComboMilestone, ActiveHold, ActiveHoldAdvance, ActiveInputSlot, ActiveMineExplosion,
    ActiveTapExplosion, AppearanceEffects, AppearanceOverrides, AutoplayNoteEvent,
    AutoplayRowEventsUpdate, AutosyncMode, COLUMN_FLASH_JUDGMENT_DURATION,
    COLUMN_FLASH_MISS_DURATION, COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO,
    COMBO_HUNDRED_MILESTONE_DURATION, COMBO_THOUSAND_MILESTONE_DURATION,
    CROSSOVER_CUE_FADE_SECONDS, ChartAttackEffects, ColumnCue, ColumnCueColumn, ColumnFlashOptions,
    ColumnScrollFlags, ColumnTapJudgment, ComboMilestoneKind, CourseDisplayCarry,
    CourseDisplayTiming, CourseDisplayTotals, CrossoverRow, DRAW_DISTANCE_AFTER_TARGETS,
    DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER, DisplayClockDiagEvent, DisplayClockHealth,
    DisplayWindowCountsSources, EMPTY_ACTIVE_INPUT_SLOT, EarlyRescoreHitDecision, ExitTransition,
    ExitTransitionKind, FantasticFeedbackOptions, FantasticWindowOptions, FinalNoteResultUpdate,
    FinalizedRowOutcome, GAMEPLAY_INPUT_LATENCY_WARN_US, GameplayAction, GameplayAssistClapState,
    GameplayAttackMode, GameplayAttackRuntimeState, GameplayAudioClockState, GameplayAudioCommand,
    GameplayAudioSnapshot, GameplayAutoplayRuntimeState, GameplayAutosyncRuntimeState,
    GameplayBeatPhaseState, GameplayBoundaryRuntimeState, GameplayCapacityTraceEvent,
    GameplayCapacityTraceKind, GameplayCapacityTraceSnapshot, GameplayChartRuntimeState,
    GameplayChartTotalsState, GameplayClockRuntimeState, GameplayCommandQueue, GameplayConfig,
    GameplayControlRuntimeState, GameplayCourseDisplayState, GameplayCueRuntimeState,
    GameplayDangerFxState, GameplayDensityGraphState, GameplayDensityGraphView,
    GameplayDisplayClockState, GameplayDisplayRuntimeState, GameplayEndTimingState,
    GameplayExitInputState, GameplayExitPromptState, GameplayHoldFeedbackState,
    GameplayHoldRuntimeState, GameplayInputLatencySample, GameplayInputLatencyTrace,
    GameplayInputPlayStyle, GameplayInputPlayerSide, GameplayInputState, GameplayLaneIndexState,
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
    JudgmentRenderInfo, LeadInTiming, M_MOD_HIGH_CAP, MAX_ACTIVE_INPUT_SLOTS,
    MINE_EXPLOSION_DURATION, MINI_PERCENT_MAX, MINI_PERCENT_MIN, MineHitPlayerState,
    MineHitPlayerUpdate, MineHitSideEffectPlan, MineJudgmentRenderInfo, MiniAttackMode,
    NoteCountStat, NoteHitEval, OFFSET_ADJUST_REPEAT_DELAY, OFFSET_ADJUST_REPEAT_INTERVAL,
    OFFSET_ADJUST_STEP_SECONDS, PerspectiveEffects, PerspectiveOverrides, PlayerJudgmentTiming,
    PlayerRowScanState, PlayerRuntime, PracticePlayerCursors, ProvisionalEarlyHitPlan,
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
    TurnRng, VisibilityEffects, VisibilityOverrides, VisualEffects, VisualOverrides,
    active_hold_counts_as_pressed, active_hold_is_engaged, active_input_slot_lane_is_down,
    add_elapsed_us, add_player_step_calories, advance_active_hold_to_time, apply_combo_update,
    apply_echo_insert, apply_final_note_result, apply_gameplay_life_delta, apply_hyper_shuffle,
    apply_insert_intelligent_taps, apply_life_change, apply_mines_insert,
    apply_next_time_based_tap_miss_for_player, apply_row_finalization_player_state,
    apply_stomp_insert, apply_super_shuffle_taps, apply_turn_options, apply_turn_permutation,
    apply_uncommon_chart_transforms, apply_uncommon_masks_with_masks, apply_wide_insert,
    approach_attack_mini_percent_to_target, approach_attack_value, approach_f32,
    attack_mini_target_percent, autoplay_cursor_for_enable, autoplay_due_active_hold_resolution,
    autoplay_judgment_offset_music_ns, autoplay_random_offset_music_ns_for_window,
    autosync_mean_ns, autosync_mode_status_line, autosync_row_hits_enabled,
    autosync_stddev_seconds, blue_fantastic_window_ms, build_assist_clap_rows,
    build_column_cues_for_player, build_note_count_stats, build_player_judgment_timing,
    build_replay_input_edges, build_row_entry, build_row_grids, carried_holds_down_at_row,
    cell_has_any_note, cell_has_nonfake_note, clear_offset_adjust_hold_state,
    collect_autosync_row_hit_offsets, column_cue_is_mine, column_flash_duration,
    column_scroll_dirs_for_flags, combo_milestone_duration, completed_mine_can_be_avoided,
    completed_row_final_judgment, completed_row_flash_note_indices_and_judgment,
    completed_row_hides_note, compute_end_times_ns, convert_tap_row_to_mines,
    convert_taps_to_holds, count_held_tracks_at_row, count_nonempty_tracks_at_row,
    count_tap_or_hold_tracks_at_row, count_tap_tracks_at_row, counts_for_early_rescore,
    course_display_carry_for_player, course_display_carry_for_stage,
    course_display_carry_for_stages, course_display_totals_for_chart, crossed_mine_held_start_time,
    crossover_arrow_col, display_window_counts_current, display_window_counts_mode,
    draw_distance_after_targets, draw_distance_before_targets, effective_mini_percent,
    enforce_max_simultaneous_notes, exit_transition_alpha, fantastic_window_seconds,
    final_note_hit_judgment, final_note_hit_plan, final_note_result_effects,
    finalized_row_awards_hand, finalized_row_judgment_for_entry,
    finalized_row_outcome_for_cached_row, finalized_row_outcome_for_entry,
    first_nonempty_track_at_row, first_row_entry_index_at_or_after_time, first_tap_track_at_row,
    first_time_index_at_or_after, gameplay_input_latency_sample, gameplay_is_single_p2_side,
    gameplay_play_style_from_profile, gameplay_player_side_for_index,
    gameplay_player_side_from_profile, gameplay_player_side_index, gameplay_runtime_player_is_p2,
    gameplay_runtime_player_side, gameplay_runtime_profiles, gameplay_tick_mode_from_profile,
    gameplay_update_hot_phase, hold_explosion_active, hold_explosion_enabled_for_options,
    hold_head_render_flags, hold_result_stats_update, init_player_runtime,
    init_player_runtime_for_song, input_lane_bit, input_queue_cap, is_hold_body_at_row,
    judged_row_lookahead_time_ns, judgment_render_info, lane_edge_judges_lift,
    lane_edge_judges_tap, lane_press_started, lane_release_finished,
    late_note_resolution_window_ns, let_go_head_beat, local_column_for_field, local_player_col,
    mark_row_entry_note_finalized, mark_row_entry_provisional_early_result, max_step_distance_ns,
    measure_counter_segments_for_densities, mine_can_be_avoided, mine_can_be_hit,
    mine_hit_offset_in_window, mine_judgment_render_info, mine_window_bounds_ns,
    music_time_from_stream_position, music_time_ns_from_song_clock, next_autosync_mode,
    next_ready_row_in_lookahead, next_timing_tick_mode, note_has_displayable_hold,
    note_hit_judgment, note_tracks_held_miss, notes_row_sorted, offset_adjust_delta_for_key,
    offset_adjust_repeat_ready, offset_adjust_slot_for_key, offset_adjust_target,
    player_chart_changes_for_options, player_column_range, player_combo_state,
    player_course_display_stage, player_index_for_column, player_note_range_for_ranges,
    player_row_scan_state, player_rows, player_runtime_is_dead, practice_player_cursors,
    process_input_edges, profile_side_from_gameplay, profile_tick_mode_from_gameplay,
    quantization_index_from_beat, recent_step_calories, recent_step_tracks, receptor_glow_visual,
    record_unmapped_input_clock_warning, reference_bpm_from_display_tag,
    refresh_roll_life_for_step, register_provisional_early_note_result, remap_live_input_lane,
    remove_cell_notes, replay_edge_cap, row_entry_for_cached_row, row_entry_index_for_cached_row,
    row_final_grade_hides_note, row_finalization_plan, row_finalization_player_state,
    saturating_elapsed_us_between, scroll_receptor_y, scroll_reverse_percent_for_column,
    scroll_reverse_scale_for_column, set_added_mine_note, set_added_tap_note,
    set_row_finalization_player_state, song_audio_end_time_ns, song_lua_field_note_hidden,
    song_lua_note_hidden, song_lua_player_transforms_default, sort_player_notes,
    spacing_multiplier_for_percent, stage_music_cut, start_offset_adjust_hold_state,
    stomp_mirror_track, suppress_final_bad_rescore_visual, tick_offset_adjust_hold_state,
    timing_row_floor, toggle_flash_alpha, track_range_has_any_note, trigger_combo_milestone,
    turn_seed_for_song, update_active_input_slot, update_itg_grade_totals,
    visible_notefield_time_ns, write_player_combo_state, zmod_stream_totals_for_densities,
};
#[cfg(test)]
use deadsync_gameplay::{SongLuaEaseMaskTarget, song_lua_ease_window_value};
#[cfg(test)]
use deadsync_gameplay::{
    build_attack_mask_windows_for_player, effective_mini_percent_for_player,
    effective_scroll_effects_for_player, refresh_active_attack_masks,
    score_invalid_reason_lines_for_chart,
};
#[cfg(test)]
use deadsync_gameplay::{
    build_song_lua_column_offset_windows_for_player, build_song_lua_constant_windows_for_player,
};
use deadsync_profile as profile_data;
use deadsync_rules::judgment::{self, JudgeGrade, Judgment, TimingWindow};
#[cfg(test)]
use deadsync_rules::note::{MAX_HOLD_LIFE, TIMING_WINDOW_SECONDS_HOLD, TIMING_WINDOW_SECONDS_ROLL};

pub type State = GameplayRuntimeState<
    profile_data::Profile,
    deadsync_song_lua::SongLuaOverlayActor<crate::game::parsing::song_lua::SongLuaOverlayKind>,
    deadsync_song_lua::SongLuaCapturedActor,
    deadsync_song_lua::SongLuaOverlayStateDelta,
>;

#[cfg(test)]
mod tests {
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
    use super::{
        ExitTransitionKind, FinalizedRowOutcome, GameplayAudioCommand, GameplaySessionCommand,
        GameplayTimingTickMode, HELD_MISS_TOTAL_DURATION, HeldMissRenderInfo,
        HoldJudgmentRenderInfo, HoldToExitKey, MAX_COLS, MAX_PLAYERS, OFFSET_ADJUST_STEP_SECONDS,
        RowEntry, ScrollSpeedSetting, SongLuaNoteHideWindowRuntime, TIMING_WINDOW_SECONDS_HOLD,
        build_attack_mask_windows_for_player, build_row_entry, crossed_mine_held_start_time,
        effective_mini_percent_for_player, effective_scroll_effects_for_player,
        effective_visual_effects_for_player, max_step_distance_ns, process_input_edges,
        refresh_active_attack_masks, score_invalid_reason_lines_for_chart,
        song_time_ns_from_seconds,
    };
    use crate::game::parsing::noteskin::{self, Noteskin, Style};
    use crate::game::parsing::song_lua::{SongLuaOverlayKind, compile_song_lua};
    use crate::game::profile;
    use crate::screens::gameplay as screen_gameplay;
    use deadsync_chart::SongData;
    use deadsync_chart::notes::ParsedNote;
    use deadsync_chart::{ArrowStats, ChartData, GameplayChartData, StaminaCounts, TechCounts};
    use deadsync_core::input::{InputSource, Lane};
    use deadsync_core::note::NoteType;
    use deadsync_core::timing::{ROWS_PER_BEAT, beat_to_note_row};
    use deadsync_input::{InputEvent, VirtualAction};
    use deadsync_profile as profile_data;
    use deadsync_rules::judgment::{self, JudgeGrade, Judgment, TimingWindow};
    use deadsync_rules::note::{HoldData, HoldResult, MineResult, Note};
    use deadsync_rules::timing::{DelaySegment, TimingData, TimingSegments};
    use deadsync_song_lua::{
        SongLuaColumnOffsetWindow, SongLuaEaseTarget, SongLuaEaseWindow, SongLuaMessageEvent,
        SongLuaModWindow, SongLuaOverlayCommandBlock, SongLuaOverlayEase,
        SongLuaOverlayMessageCommand, SongLuaOverlayState, SongLuaSpanMode, SongLuaTimeUnit,
    };
    use std::sync::{Arc, LazyLock, Mutex};
    use std::time::Instant;
    use std::{fs, path::PathBuf};
    use winit::keyboard::KeyCode;

    type SongLuaOverlayActor = deadsync_song_lua::SongLuaOverlayActor<SongLuaOverlayKind>;
    type CompiledSongLua = deadsync_song_lua::CompiledSongLua<SongLuaOverlayActor>;

    static SESSION_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[inline(always)]
    fn gameplay_menu_input(action: VirtualAction) -> Option<super::GameplayMenuInput> {
        match action {
            VirtualAction::p1_start => Some(super::GameplayMenuInput::P1Start),
            VirtualAction::p2_start => Some(super::GameplayMenuInput::P2Start),
            VirtualAction::p1_back => Some(super::GameplayMenuInput::P1Back),
            VirtualAction::p2_back => Some(super::GameplayMenuInput::P2Back),
            _ => None,
        }
    }

    fn queue_input_edge(
        state: &mut super::State,
        source: InputSource,
        lane: Lane,
        input_slot: u32,
        pressed: bool,
        timestamp: Instant,
        timestamp_host_nanos: u64,
        stored_at: Instant,
        emitted_at: Instant,
    ) {
        state.queue_live_input_edge(
            lane,
            |lane, queued_at, event_music_time_ns, record_replay| InputEdge {
                lane,
                input_slot,
                pressed,
                source,
                record_replay,
                captured_at: timestamp,
                captured_host_nanos: timestamp_host_nanos,
                stored_at,
                emitted_at,
                queued_at,
                event_music_time_ns,
            },
        );
    }

    fn handle_input(state: &mut super::State, ev: &InputEvent) -> super::GameplayAction {
        if state.exit_transition_active() {
            return super::GameplayAction::None;
        }
        if let Some(lane) = deadsync_input::lane_from_action(ev.action) {
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
            return super::GameplayAction::None;
        }
        if let Some(input) = gameplay_menu_input(ev.action) {
            state.handle_gameplay_menu_input(input, ev.pressed, ev.timestamp);
        }
        super::GameplayAction::None
    }

    #[inline(always)]
    fn init(
        song: Arc<SongData>,
        charts: [Arc<ChartData>; MAX_PLAYERS],
        gameplay_charts: [Arc<GameplayChartData>; MAX_PLAYERS],
        viewport: super::GameplayViewport,
        session: super::GameplaySession,
        config: super::GameplayConfig,
        pack_sync_pref: deadsync_chart::SyncPref,
        mini_indicator_data: super::GameplayMiniIndicatorData,
        noteskin_data: super::GameplayNoteskinData,
        song_lua_data: deadsync_gameplay::GameplaySongLuaData<CompiledSongLua>,
        active_color_index: i32,
        music_rate: f32,
        scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
        player_profiles: [profile_data::Profile; MAX_PLAYERS],
        replay_edges: Option<Vec<super::ReplayInputEdge>>,
        replay_offsets: Option<super::ReplayOffsetSnapshot>,
        lead_in_timing: Option<super::LeadInTiming>,
        course_display_carry: Option<[super::CourseDisplayCarry; MAX_PLAYERS]>,
        course_display_totals: Option<[super::CourseDisplayTotals; MAX_PLAYERS]>,
        course_display_timing: Option<super::CourseDisplayTiming>,
        combo_carry: [u32; MAX_PLAYERS],
    ) -> super::State {
        deadsync_gameplay::init_gameplay_runtime::<SongLuaOverlayKind>(
            song,
            charts,
            gameplay_charts,
            viewport,
            session,
            config,
            pack_sync_pref,
            mini_indicator_data,
            noteskin_data,
            song_lua_data,
            active_color_index,
            music_rate,
            scroll_speed,
            player_profiles,
            replay_edges,
            replay_offsets,
            lead_in_timing,
            course_display_carry,
            course_display_totals,
            course_display_timing,
            combo_carry,
        )
    }

    #[inline(always)]
    fn gameplay_raw_modifier_key(code: KeyCode) -> Option<super::GameplayRawModifierKey> {
        match code {
            KeyCode::ShiftLeft | KeyCode::ShiftRight => Some(super::GameplayRawModifierKey::Shift),
            KeyCode::ControlLeft | KeyCode::ControlRight => {
                Some(super::GameplayRawModifierKey::Ctrl)
            }
            _ => None,
        }
    }

    #[inline(always)]
    fn gameplay_raw_key_input(code: KeyCode) -> super::GameplayRawKeyInput {
        match code {
            KeyCode::KeyR => super::GameplayRawKeyInput::Restart,
            KeyCode::F6 => super::GameplayRawKeyInput::Autosync,
            KeyCode::F7 => super::GameplayRawKeyInput::TimingTick,
            KeyCode::F8 => super::GameplayRawKeyInput::Autoplay,
            KeyCode::F11 => {
                super::GameplayRawKeyInput::OffsetAdjust(super::GameplayOffsetAdjustKey::Decrease)
            }
            KeyCode::F12 => {
                super::GameplayRawKeyInput::OffsetAdjust(super::GameplayOffsetAdjustKey::Increase)
            }
            _ => super::GameplayRawKeyInput::Other,
        }
    }

    fn handle_queued_raw_key(
        state: &mut super::State,
        code: KeyCode,
        pressed: bool,
        timestamp: Instant,
        now_music_time: f32,
        allow_commands: bool,
    ) -> super::RawKeyAction {
        state.handle_queued_raw_key_input(
            gameplay_raw_key_input(code),
            gameplay_raw_modifier_key(code),
            pressed,
            timestamp,
            now_music_time,
            allow_commands,
        )
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
        init(
            song,
            charts,
            gameplay_charts,
            super::GameplayViewport::default(),
            session,
            super::GameplayConfig::default(),
            deadsync_chart::SyncPref::Default,
            super::GameplayMiniIndicatorData::default(),
            noteskin_data,
            deadsync_gameplay::GameplaySongLuaData::<CompiledSongLua>::default(),
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
                            gameplay_runtime_profiles(&player_profiles, &session);
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
                                compile_song_lua(&change.path, &context)
                                    .expect("generated song lua should compile")
                            })
                            .map(|compiled| deadsync_gameplay::GameplayCompiledSongLua {
                                compiled,
                                compile_ms: 0.0,
                            });
                        let song_lua_data =
                            deadsync_gameplay::GameplaySongLuaData::<CompiledSongLua> {
                                primary,
                                ..Default::default()
                            };
                        let mut state = screen_gameplay::State::from_gameplay(
                            init(
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
        assert_valid_hot_state_for_tests(
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
        state.integrate_active_hold_to_time(0, target_ns);

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

        state.start_active_hold(
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

                state.set_final_note_result(
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
                state.set_final_note_result(
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

                state.finalize_row_judgment(0, row_index, 0, false);

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

        state.error_bar_register_tap(
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

        state.error_bar_register_tap(
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

        state.error_bar_register_tap(
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

        state.error_bar_register_tap(
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

        state.error_bar_register_tap(
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

        state.error_bar_register_tap(
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

        state.error_bar_register_tap(
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

        state.error_bar_register_tap(
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
            state.error_bar_register_tap(
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
            state.error_bar_register_tap(
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
            state.error_bar_register_tap(
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

        state.error_bar_register_tap(
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
        state.tick_visual_effects(0.0);
        assert!(state.display.hold_feedback.hold_judgments[0].is_some());

        state.display.hold_feedback.hold_judgments[0] = Some(HoldJudgmentRenderInfo {
            result: HoldResult::Held,
            started_at_screen_s: 4.2,
        });
        state.tick_visual_effects(0.0);
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

        state.set_final_note_result(
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
        state.tick_visual_effects(0.0);
        assert!(state.display.hold_feedback.held_miss_judgments[2].is_none());
    }

    #[test]
    fn mine_judgment_feedback_records_result_column_and_time() {
        let mut state = regression_state([
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ]);
        state.boundary.total_elapsed_in_screen = 9.25;

        state.set_last_mine_judgment(0, 2, MineResult::Avoided);

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

        state.trigger_completed_row_tap_explosions(0, row_index);

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

        assert!(state.judge_a_tap(column, note_time));
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

        assert!(state.judge_a_tap(column, note_time));
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
        enabled.spawn_tap_explosion_for_grade(column, JudgeGrade::Great, false);
        assert!(enabled.display.visual_feedback.tap_explosions[column].is_some());

        let mut profile = profile_data::Profile::default();
        profile
            .tap_explosion_active_mask
            .remove(profile_data::TapExplosionMask::GREAT);
        let mut disabled = regression_state([profile, profile_data::Profile::default()]);
        disabled.spawn_tap_explosion_for_grade(column, JudgeGrade::Great, false);
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
        disabled.trigger_completed_row_tap_explosions(0, row_index);
        assert!(disabled.display.visual_feedback.column_flashes[column].is_none());
        // The ungated SMX feedback record is written even when the on-screen column flash is
        // masked off; the pad lighting relies on this decoupling.
        let judged = disabled.display.visual_feedback.last_tap_judgments[column]
            .expect("masked column flash should still record the ungated tap judgment");
        assert_eq!(judged.grade, JudgeGrade::Great);

        let mut enabled = build_state(profile_data::ColumnFlashMask::GREAT);
        enabled.trigger_completed_row_tap_explosions(0, row_index);
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
        state.trigger_mine_explosion(column);
        let first = state.display.visual_feedback.mine_explosions[column]
            .as_ref()
            .expect("mine hit should set an explosion");
        assert_eq!(first.started_at_screen_s, 1.0);
        // A later hit on the same column refreshes the timestamp even while the explosion is
        // still present, which is what lets the SMX panel diff tell consecutive hits apart.
        state.boundary.total_elapsed_in_screen = 1.5;
        state.trigger_mine_explosion(column);
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
        disabled.trigger_completed_row_tap_explosions(0, row_index);
        assert!(disabled.display.visual_feedback.column_flashes[column].is_none());

        let (mut enabled, row_index, column) = fantastic_row_state(
            profile_data::ColumnFlashMask::WHITE_FANTASTIC,
            18.0,
            TimingWindow::W1,
        );
        enabled.trigger_completed_row_tap_explosions(0, row_index);
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
        disabled.trigger_completed_row_tap_explosions(0, row_index);
        assert!(disabled.display.visual_feedback.column_flashes[column].is_none());

        let (mut enabled, row_index, column) = fantastic_row_state(
            profile_data::ColumnFlashMask::BLUE_FANTASTIC,
            4.0,
            TimingWindow::W0,
        );
        enabled.trigger_completed_row_tap_explosions(0, row_index);
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
        hide_notefield.render_provisional_early_rescore_feedback(
            0, column, &judgment, 1.0, true, true, false,
        );
        assert!(hide_notefield.display.visual_feedback.tap_explosions[column].is_none());
        assert!(hide_notefield.display.visual_feedback.column_flashes[column].is_some());

        let mut hide_column = build_state();
        hide_column.render_provisional_early_rescore_feedback(
            0, column, &judgment, 1.0, true, false, true,
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
        enabled.trigger_hold_explosion(column);
        assert!(enabled.display.visual_feedback.tap_explosions[column].is_some());

        let mut profile = profile_data::Profile::default();
        profile
            .tap_explosion_active_mask
            .remove(profile_data::TapExplosionMask::HELD);
        let mut disabled = regression_state([profile, profile_data::Profile::default()]);
        disabled.trigger_hold_explosion(column);
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

        state.trigger_completed_row_tap_explosions(0, row_index);

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

        state.trigger_completed_row_tap_explosions(0, row_index);

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
        state.tick_visual_effects(0.01);

        if started_press > 0.01 {
            assert!(state.display.receptor_feedback.glow_press_timers[column] > 0.0);
            assert!(state.display.receptor_feedback.glow_press_timers[column] < started_press);
        }
        state.tick_visual_effects(started_press.max(0.01));

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

        state.set_final_note_result(
            0,
            Judgment {
                time_error_ms: 0.0,
                time_error_music_ns: 0,
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W0),
                miss_because_held: false,
            },
        );
        state.finalize_row_judgment(0, row_index, 0, false);

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
        let compiled = CompiledSongLua {
            eases: vec![SongLuaEaseWindow {
                unit: SongLuaTimeUnit::Beat,
                start: 1.0,
                limit: 1.0,
                span_mode: SongLuaSpanMode::Len,
                from: 0.0,
                to: 5.0,
                target: SongLuaEaseTarget::PlayerRotationZ,
                easing: Some("linear".to_string()),
                player: Some(1),
                sustain: Some(4.0),
                opt1: None,
                opt2: None,
            }],
            ..Default::default()
        };
        let (windows, unsupported) =
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(
                &compiled,
                &timing,
                0,
                0.0,
                &[],
            );
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
        let compiled = CompiledSongLua {
            overlays: vec![SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Quad,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: vec![SongLuaOverlayMessageCommand {
                    message: "ResetBlack".to_string(),
                    blocks: vec![SongLuaOverlayCommandBlock {
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
                }],
            }],
            overlay_eases: vec![SongLuaOverlayEase {
                overlay_index: 0,
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 8.0,
                span_mode: SongLuaSpanMode::Len,
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
            messages: vec![SongLuaMessageEvent {
                beat: 4.0,
                message: "ResetBlack".to_string(),
                persists: true,
            }],
            ..Default::default()
        };

        let windows =
            deadsync_gameplay::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

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
        let compiled = CompiledSongLua {
            overlays: vec![SongLuaOverlayActor {
                kind: SongLuaOverlayKind::ActorFrame,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: vec![SongLuaOverlayMessageCommand {
                    message: "SetupZoom".to_string(),
                    blocks: vec![SongLuaOverlayCommandBlock {
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
                }],
            }],
            overlay_eases: vec![SongLuaOverlayEase {
                overlay_index: 0,
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 8.0,
                span_mode: SongLuaSpanMode::Len,
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
            messages: vec![SongLuaMessageEvent {
                beat: 0.0,
                message: "SetupZoom".to_string(),
                persists: true,
            }],
            ..Default::default()
        };

        let windows =
            deadsync_gameplay::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

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
        let compiled = CompiledSongLua {
            overlays: vec![SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Quad,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: vec![SongLuaOverlayMessageCommand {
                    message: "ResetBlack".to_string(),
                    blocks: vec![SongLuaOverlayCommandBlock {
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
                }],
            }],
            overlay_eases: vec![SongLuaOverlayEase {
                overlay_index: 0,
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 2.0,
                span_mode: SongLuaSpanMode::Len,
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
            messages: vec![SongLuaMessageEvent {
                beat: 4.0,
                message: "ResetBlack".to_string(),
                persists: true,
            }],
            ..Default::default()
        };

        let windows =
            deadsync_gameplay::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

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
        let compiled = CompiledSongLua {
            eases: vec![
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 4.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::PlayerZoomY,
                    from: 1.0,
                    to: 0.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 8.0,
                    limit: 4.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::PlayerZoomY,
                    from: 0.0,
                    to: 1.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 1.0,
                    limit: 0.25,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::Mod("dark".to_string()),
                    from: 0.0,
                    to: 100.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 4.0,
                    limit: 2.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::Mod("dark".to_string()),
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
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(
                &compiled,
                &timing,
                0,
                0.0,
                &[],
            );

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
        let compiled = CompiledSongLua {
            eases: vec![SongLuaEaseWindow {
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 4.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::Mod("flip".to_string()),
                from: 0.0,
                to: -400.0,
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            }],
            beat_mods: vec![SongLuaModWindow {
                unit: SongLuaTimeUnit::Beat,
                start: 4.0,
                limit: 1.0,
                span_mode: SongLuaSpanMode::Len,
                mods: "*100 0 flip".to_string(),
                player: Some(1),
            }],
            ..Default::default()
        };

        let constants = build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);
        let (windows, unsupported) =
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(
                &compiled, &timing, 0, 0.0, &constants,
            );

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
        let compiled = CompiledSongLua {
            beat_mods: vec![SongLuaModWindow {
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 999.0,
                span_mode: SongLuaSpanMode::Len,
                mods: "*1 0 Stealth, *1 0 PulseOuter".to_string(),
                player: Some(1),
            }],
            eases: vec![
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 4.0,
                    limit: 2.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::Mod("Stealth".to_string()),
                    from: 0.0,
                    to: 45.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 4.0,
                    limit: 2.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::Mod("PulseOuter".to_string()),
                    from: 0.0,
                    to: 80.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 4.0,
                    limit: 2.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::Mod("PulsePeriod".to_string()),
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

        let constants = build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);
        let (windows, unsupported) =
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(
                &compiled, &timing, 0, 0.0, &constants,
            );

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
        let compiled = CompiledSongLua {
            beat_mods: vec![SongLuaModWindow {
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 1.0,
                span_mode: SongLuaSpanMode::Len,
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
        let compiled = CompiledSongLua {
            beat_mods: vec![SongLuaModWindow {
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 3.0,
                span_mode: SongLuaSpanMode::Len,
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
        let compiled = CompiledSongLua {
            beat_mods: vec![SongLuaModWindow {
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 3.0,
                span_mode: SongLuaSpanMode::Len,
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
        let compiled = CompiledSongLua {
            eases: vec![SongLuaEaseWindow {
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 4.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::Mod("mini".to_string()),
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
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(
                &compiled,
                &timing,
                0,
                0.0,
                &[],
            );
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
        let compiled = CompiledSongLua {
            beat_mods: vec![
                SongLuaModWindow {
                    unit: SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 9999.0,
                    span_mode: SongLuaSpanMode::End,
                    mods: "*1000 no invert, *1000 no flip".to_string(),
                    player: Some(1),
                },
                SongLuaModWindow {
                    unit: SongLuaTimeUnit::Beat,
                    start: 0.25,
                    limit: 0.25,
                    span_mode: SongLuaSpanMode::Len,
                    mods: "*1000 invert".to_string(),
                    player: Some(1),
                },
                SongLuaModWindow {
                    unit: SongLuaTimeUnit::Beat,
                    start: 0.5,
                    limit: 0.25,
                    span_mode: SongLuaSpanMode::Len,
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
        let compiled = compile_song_lua(&entry, &context).unwrap();
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
        let compiled = compile_song_lua(&entry, &context).unwrap();
        assert!(compiled.eases.iter().any(|window| {
            matches!(
                window.target,
                SongLuaEaseTarget::Mod(ref name)
                    if name == "tiny"
            ) && (window.start - 26.5).abs() <= 0.001
                && (window.to + 200.0).abs() <= 0.001
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(
                window.target,
                SongLuaEaseTarget::Mod(ref name)
                    if name == "flip"
            ) && (window.start - 26.5).abs() <= 0.001
                && (window.to - 50.0).abs() <= 0.001
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(
                window.target,
                SongLuaEaseTarget::Mod(ref name)
                    if name == "dark"
            ) && (window.start - 28.0).abs() <= 0.001
                && (window.to - 100.0).abs() <= 0.001
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(
                window.target,
                SongLuaEaseTarget::Mod(ref name)
                    if name == "skewx"
            ) && (window.start - 166.0).abs() <= 0.001
                && (window.to.abs() - 3.0).abs() <= 0.001
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(
                window.target,
                SongLuaEaseTarget::Mod(ref name)
                    if name == "skewx"
            ) && (window.start - 182.0).abs() <= 0.001
                && (window.to.abs() - 3.0).abs() <= 0.001
        }));
        assert!(compiled.eases.iter().any(|window| {
            matches!(window.target, SongLuaEaseTarget::PlayerRotationX)
                && (window.start - 189.0).abs() <= 0.001
                && (window.to - 20.0).abs() <= 0.001
        }));

        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 77.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(200 * 48));
        let constants = build_song_lua_constant_windows_for_player(&compiled, &timing, 0, 0.0);
        let (windows, unsupported) =
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(
                &compiled, &timing, 0, 0.0, &constants,
            );
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
        let compiled = CompiledSongLua {
            column_offsets: vec![
                SongLuaColumnOffsetWindow {
                    player: 0,
                    column: 2,
                    unit: SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 0.5,
                    span_mode: SongLuaSpanMode::Len,
                    from_y: 33.75,
                    to_y: 0.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                SongLuaColumnOffsetWindow {
                    player: 0,
                    column: 2,
                    unit: SongLuaTimeUnit::Beat,
                    start: 2.0,
                    limit: 0.5,
                    span_mode: SongLuaSpanMode::Len,
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

        let windows = build_song_lua_column_offset_windows_for_player(&compiled, &timing, 0, 0.0);

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
        let compiled = CompiledSongLua {
            eases: vec![
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 1.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::PlayerX,
                    from: 320.0,
                    to: 360.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 1.0,
                    limit: 1.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::PlayerY,
                    from: 240.0,
                    to: 210.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 2.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::PlayerZ,
                    from: 0.0,
                    to: -120.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 4.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::PlayerRotationX,
                    from: 0.0,
                    to: 20.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 4.0,
                    limit: 2.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::PlayerSkewY,
                    from: 0.0,
                    to: 0.25,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 6.0,
                    limit: 2.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::PlayerZoom,
                    from: 1.0,
                    to: 0.75,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 8.0,
                    limit: 2.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::PlayerZoomZ,
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
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(
                &compiled,
                &timing,
                0,
                0.0,
                &[],
            );

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
        let compiled = CompiledSongLua {
            eases: vec![
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 1.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::Mod("skewx".to_string()),
                    from: 0.0,
                    to: 3.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                SongLuaEaseWindow {
                    player: Some(1),
                    unit: SongLuaTimeUnit::Beat,
                    start: 1.0,
                    limit: 1.0,
                    span_mode: SongLuaSpanMode::Len,
                    target: SongLuaEaseTarget::Mod("skewy".to_string()),
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
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(
                &compiled,
                &timing,
                0,
                0.0,
                &[],
            );

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
        let compiled = CompiledSongLua {
            eases: vec![SongLuaEaseWindow {
                player: Some(1),
                unit: SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 4.0,
                span_mode: SongLuaSpanMode::Len,
                target: SongLuaEaseTarget::Mod("confusionoffset".to_string()),
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
            deadsync_gameplay::build_compiled_song_lua_ease_windows_for_player(
                &compiled,
                &timing,
                0,
                0.0,
                &[],
            );

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
        tap_state.apply_time_based_tap_misses(inside_delay_music_time);
        assert!(tap_state.chart_runtime.notes[0].result.is_none());
        assert_eq!(tap_state.chart_runtime.mine_scan.next_tap_miss_cursor[0], 0);

        let after_delay_music_time = note_time_ns
            .saturating_add(miss_distance_ns)
            .saturating_add(song_time_ns_from_seconds(2.1));
        tap_state.apply_time_based_tap_misses(after_delay_music_time);
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
        mine_state.apply_time_based_mine_avoidance(inside_delay_music_time);
        assert_eq!(mine_state.chart_runtime.notes[0].mine_result, None);
        assert_eq!(mine_state.chart_runtime.mine_scan.next_mine_ix_cursor[0], 0);

        let after_delay_music_time = note_time_ns
            .saturating_add(mine_distance_ns)
            .saturating_add(song_time_ns_from_seconds(2.1));
        mine_state.apply_time_based_mine_avoidance(after_delay_music_time);
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

        state.apply_time_based_mine_avoidance(end_time_ns);
        assert_eq!(state.players_runtime.players[0].mines_avoided, 0);
        assert_eq!(state.chart_runtime.notes[0].mine_result, None);

        state.finalize_completed_mines();
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

        state.set_final_note_result(
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
        assert!(!state.settle_completion_rows());
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
        state.update_judged_rows();
        state.apply_time_based_tap_misses(miss_time_ns);
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

        assert!(state.settle_completion_rows());
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

        assert!(state.try_hit_crossed_mines_while_held(
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

        state.apply_pending_mine_hits();

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

        assert!(state.hit_mine(1, 1, 0));
        assert_eq!(
            state.chart_runtime.notes[1].mine_result,
            Some(MineResult::Hit)
        );
        assert_eq!(state.players_runtime.players[0].mines_hit, 0);
        assert!(!state.players_runtime.players[0].is_failing);

        let inputs = std::array::from_fn(|col| col == 0);
        state.update_active_holds(&inputs, hold_end_ns);
        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|hold| hold.result),
            Some(HoldResult::Held)
        );
        assert_eq!(state.players_runtime.players[0].holds_held_for_score, 1);
        assert!(!state.players_runtime.players[0].is_failing);

        state.apply_pending_mine_hits();
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
        state.apply_time_based_tap_misses(miss_time_ns);

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

        state.resolve_pending_missed_holds(hold_end_ns.saturating_sub(1));
        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|h| h.result),
            None
        );
        assert_eq!(state.players_runtime.players[0].holds_let_go_for_score, 0);

        state.resolve_pending_missed_holds(hold_end_ns);

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
        state.apply_time_based_tap_misses(miss_time_ns);

        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|hold| hold.result),
            Some(HoldResult::Missed)
        );
        assert_eq!(state.players_runtime.players[0].holds_let_go_for_score, 0);
        assert!(state.display.hold_feedback.hold_judgments[0].is_none());

        state.resolve_pending_missed_holds(hold_end_ns);

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

        assert!(!state.try_hit_crossed_mines_while_held(
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
