pub mod app;

mod command;
mod course;
mod diagnostics;
mod dynamic_media;
mod frame_loop;
mod frame_pacing_trace;
mod frame_stats;
mod frame_stutter;
mod gameplay_entry;
mod gameplay_prewarm;
mod graphics;
mod input;
mod input_backend;
mod input_trace;
mod interaction;
mod lighting;
mod navigation;
mod offset_prompt;
mod pad_config;
mod player_options;
mod profile_session;
mod restart;
mod runtime;
mod screen_flow;
mod screenshot;
mod session;
mod session_results;
mod smx_config;
mod stutter_diag;
mod transition_effects;
mod window;
mod window_state;

pub use command::{
    BannerSlot, Command, CommandExecutionResult, CommandKind, CommandTimingLog,
    CommandTimingResult, DeferredCommand, DeferredCommandApplyPlan, DeferredCommandEffect,
    DeferredCommandProcessPlan, DeferredCommandResourceContext, DeferredCommandRootEffect,
    DynamicBackgroundMediaResult, WHITE_GRAPH_KEY, apply_banner_media, apply_cdtitle_media,
    apply_deferred_command_process_plan, apply_deferred_command_resources,
    apply_dynamic_background_media, apply_pack_banner_media, apply_wheel_item_backgrounds_media,
    banner_slot, build_density_graph_mesh, command_label, command_logs_frame_cost,
    command_timing_log, command_timing_result, deferred_command_apply_plan,
    deferred_command_process_plan, execute_command_resources, execute_shell_command,
    fallback_banner_key, log_command_timing_for_screen,
};
pub use course::{
    CourseRunState, CourseStageRuntime, build_course_graph_stages, build_course_run_from_selection,
    build_course_summary_score_info, build_course_summary_stage, course_display_timing_for_run,
    course_total_seconds, merge_course_score_columns, score_info_from_stage,
};
pub use deadsync_screens::diagnostics::{
    FrameStatsSample, FrameStatsSummary, HISTOGRAM_BINS, OverlayAnchor, OverlayStyle, TimingHealth,
    VisibleStutterSample, histogram,
};
pub use diagnostics::timing_health;
pub use dynamic_media::DynamicMedia;
pub use frame_loop::{
    FrameLoopState, FrameScreenStepContext, FrameScreenStepPlan, FrameWaitControl, FrameWaitPlan,
    frame_screen_step_plan,
};
pub use frame_pacing_trace::GameplayPacingTrace;
pub use frame_stats::{
    DecayingHist, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR, EwmaStats, FrameStatsController, FrameStatsLong,
    FrameStatsMetrics, FrameStatsSummaryContext, FrameStatsView, default_overlay_anchor,
    frame_stats_summary, frame_stats_target_us, frame_stats_two_player, next_overlay_anchor,
    percentile_us,
};
pub use frame_stutter::{ComposeBreakdown, trace_frame_stutter};
pub use gameplay_entry::{
    GameplayChartEntryPlan, gameplay_chart_entry_plan, gameplay_last_played_commands,
};
pub use gameplay_prewarm::{
    gameplay_overlay_video_paths, gameplay_song_lua_video_paths, prewarm_gameplay_assets,
    prewarm_gameplay_sfx,
};
pub use graphics::{
    ExistingDisplayChange, GraphicsChangeContext, GraphicsChangePlan, GraphicsChangeRequest,
    GraphicsDisplaySync, GraphicsRuntimeSettings, GraphicsRuntimeSettingsResult,
    GraphicsRuntimeUpdate, GraphicsWindowPlan, RecreateDisplayChange, RecreateDisplayState,
    RendererInitConfig, RendererInitResult, RendererStartupConfig, RendererStartupResult,
    RendererStartupSettings, RendererSwitchBeginPlan, RendererSwitchFailurePlan,
    RendererSwitchPlan, RendererSwitchRequest, RendererSwitchResourceResetPlan,
    RendererSwitchRestoreState, RendererSwitchSuccessPlan, RendererSwitchWindowConfig,
    RendererSwitchWindowResult, apply_app_window_setup_state, apply_display_mode_result,
    apply_graphics_runtime_settings, apply_recreate_display_change, apply_renderer_started,
    apply_renderer_switch_restore_display, apply_renderer_switch_restore_state,
    apply_renderer_switch_window_state, apply_resolution_result, apply_runtime_display_mode,
    apply_runtime_resolution, available_monitor_specs, begin_renderer_switch, dispose_renderer,
    graphics_change_context, graphics_change_context_from_monitor, graphics_change_plan,
    graphics_runtime_updates, initialize_renderer, prepare_renderer_switch_window,
    recreate_display_sync, refresh_present_config, renderer_startup_config,
    renderer_switch_begin_plan, renderer_switch_failure_plan, renderer_switch_needed,
    renderer_switch_plan, renderer_switch_resource_reset_commands,
    renderer_switch_resource_reset_plan, renderer_switch_success_plan,
    renderer_switch_window_config, reset_renderer_switch_clock, restore_display_sync,
    runtime_display_mode_change, runtime_display_mode_sync, runtime_resolution_change,
    start_renderer_runtime, startup_display_sync, sync_renderer_window_size,
};
pub use input::{
    AppRawKeyShortcut, EvaluationRawKeyShortcut, GamepadSystemEventPlan,
    GameplayRawKeyRouteContext, GameplayRawKeyRoutePlan, PreScreenInputContext,
    PreScreenInputRoute, QueuedInputBatchState, QueuedInputEventRoute, QueuedInputFlushPlan,
    RawKeyScreenRoute, RawKeyTextRoute, RawPadScreenRoute, SmxPanelPressFeedback, UserEvent,
    allowed_gameplay_raw_action, app_raw_key_shortcut, evaluation_raw_key_shortcut,
    gamepad_system_event_plan, gameplay_dispatch_continues, gameplay_raw_key_route_plan,
    practice_reload_shortcut, pre_screen_input_route, queued_input_flush_plan, raw_key_alt_f4_quit,
    raw_key_screen_route, raw_key_text_route, raw_keyboard_capture_enabled,
    raw_keyboard_restart_screen, raw_pad_screen_route, screen_accepts_queued_input,
    smx_panel_press_feedback_plan,
};
pub use input_backend::{InputBackendConfig, launch_input_backends};
pub use input_trace::GameplayInputTrace;
pub use interaction::{
    ExitIntent, HeldControls, ProcessExitPlan, ProcessExitRequest, ShellInteractionState,
};
pub use lighting::{
    GameplayLightSyncTarget, LightInputRoute, LightingFramePlan, OperatorMenuButtonRoute,
    SmxAnimationSyncKey, SmxPadGifFramePlan, SmxPanelDriver, SmxResultContext,
    gameplay_light_sync_target, hide_flags_for_profiles, hide_flags_from_profile,
    light_input_route, lighting_frame_plan, lighting_screen_mode, load_cabinet_light_chart,
    operator_menu_button_route, screen_light_context, smx_background_role, smx_pad_blackout,
    smx_pad_gif_frame_plan, smx_result_context,
};
pub use navigation::{
    FadeCompletionEffect, FadeCompletionExitPlan, FadeCompletionPlan,
    NavigationTransitionEffectPlan, NavigationTransitionStart, ProcessExitNavigationEffect,
    ProcessExitNavigationLog, ProcessExitNavigationPlan, ScreenChangePlan, TransitionAudioPlan,
    TransitionCompletion, TransitionFramePlan, TransitionMusicPaths, TransitionState,
    actor_entry_transition, actor_fade_out_transition, actor_transition_music_commands,
    apply_actor_entry_transition, apply_actor_fade_out_transition, apply_global_entry_transition,
    apply_global_fade_out_transition, fade_completion_exit_plan, fade_completion_plan,
    global_entry_transition, global_fade_out_transition, is_actor_fade_screen,
    is_actor_only_transition, machine_flow_screen, navigation_transition_effect_plan,
    navigation_transition_start, process_exit_navigation_plan, screen_change_plan,
    screen_from_machine_flow, transition_audio_plan, write_current_screen_file,
};
pub use offset_prompt::{
    GameplayOffsetSavePrompt, GameplayOffsetSaveTargets, GameplayOffsetSnapshot, OffsetPromptInput,
    gameplay_offset_changed, gameplay_offset_prompt_needed, gameplay_offset_prompt_text,
    gameplay_offset_save_targets, gameplay_offset_saveable_changed, route_offset_prompt_input,
};
pub use pad_config::{
    PadConfigFsrPlan, PadConfigFsrTarget, apply_pad_commands, pad_config_fsr_plan,
    pad_config_profile_cursor, pad_config_profile_entries,
};
pub use player_options::{PlayerOptionsPersistPlan, player_options_persist_plan};
pub use profile_session::{
    ComboCarryUpdate, GameplayComboCarryContext, course_last_played_sides,
    gameplay_combo_carry_updates, persist_gameplay_combo_carry, record_last_played_course,
    reset_operator_profile_session,
};
pub use restart::{
    FastGameplayRestartPlan, GameplayReloadSource, GameplayRestartRoute, RestartPayload,
    RestartPrepareSource, fast_gameplay_restart_plan, gameplay_reload_source,
    gameplay_restart_prepare_source, gameplay_restart_route, practice_from_eval_allowed,
    practice_reload_allowed, practice_restart_prepare_source, restart_chart_steps,
    restart_payload_from_eval,
};
pub use runtime::ShellState;
pub use screen_flow::{
    LateJoinContext, NavigationRoutePlan, OnlineProfileLinkPlan, ProfileSelectionContext,
    ProfileSelectionPlan, ScreenActionEffect, ScreenActionEffectPlan, ScreenActionRouteContext,
    ScreenActionRoutePlan, SelectMusicJoinContext, SelectMusicJoinPlan,
    evaluation_summary_return_to, late_join_side, navigation_route_plan, profile_selection_plan,
    screen_action_effect_plan, screen_action_route_plan, select_music_join_plan,
};
pub use screenshot::{
    AutoScreenshotEvalResult, AutoScreenshotFrameContext, AutoScreenshotFramePlan,
    PendingScreenshotResult, SavedScreenshot, ScreenshotFlowError, ScreenshotSongInfo,
    append_screenshot_overlay_actors, auto_screenshot_eval_matches_results,
    auto_screenshot_frame_plan, capture_pending_screenshot, capture_screenshot,
    replace_screenshot_preview_texture, screenshot_preview_target, screenshot_preview_visible,
    screenshot_song_info,
};
pub use session::SessionState;
pub use session_results::{post_select_display_stages, stage_summary_from_score_info};
pub use smx_config::{
    SmxAssignmentPlan, SmxAssignmentSource, SmxAutopromptPlan, SmxLightBrightnessPlan,
    resolve_smx_pad_config, smx_assignment_plan, smx_autoprompt_plan, smx_light_brightness_plan,
    smx_light_preview_restore_auto, smx_options_light_preview_active,
    smx_player_options_light_preview_allowed, smx_runtime_assignment_plan,
};
pub use stutter_diag::{
    STUTTER_DIAG_FRAME_CAPACITY, STUTTER_DIAG_WINDOW_NS, StutterDiagDumpContext,
    StutterDiagFrameSample, StutterDiagRecorder, stutter_diag_dump_lines,
};
pub use transition_effects::{
    PlayerOptionsTransition, TransitionEffectContext, TransitionEffectPlan, transition_effect_plan,
};
pub use window::{
    AppWindowConfig, AppWindowSetup, DisplayModeChange, DisplayModeResult, ResolutionChange,
    ResolutionResult, apply_window_display_mode, apply_window_resolution, create_app_window,
    effective_fullscreen_type, transition_fullscreen_type,
};
pub use window_state::{
    ShellWindowEventPlan, WindowMinimizePlan, apply_shell_surface_active, apply_shell_window_focus,
    apply_shell_window_occlusion, exclusive_fullscreen_focus_plan,
};
