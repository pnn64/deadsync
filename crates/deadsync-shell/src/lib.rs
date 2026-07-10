mod command;
mod course;
mod dynamic_media;
mod frame_loop;
mod frame_pacing_trace;
mod frame_stats;
mod frame_stutter;
mod input;
mod input_trace;
mod interaction;
mod lighting;
mod navigation;
mod offset_prompt;
mod restart;
mod runtime;
mod screenshot;
mod session_results;
mod stutter_diag;
mod window;

pub use command::{
    BannerSlot, Command, CommandKind, CommandTimingLog, banner_slot, build_density_graph_mesh,
    command_label, command_logs_frame_cost, command_timing_log, fallback_banner_key,
    spawn_online_grade_fetch,
};
pub use course::{
    CourseRunState, CourseStageRuntime, build_course_graph_stages, build_course_run_from_selection,
    build_course_summary_score_info, build_course_summary_stage, course_display_timing_for_run,
    course_total_seconds, merge_course_score_columns, score_info_from_stage,
};
pub use dynamic_media::DynamicMedia;
pub use frame_loop::FrameLoopState;
pub use frame_pacing_trace::GameplayPacingTrace;
pub use frame_stats::{
    DecayingHist, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR, EwmaStats, FrameStatsController, FrameStatsLong,
    FrameStatsMetrics, FrameStatsSample, FrameStatsSummary, FrameStatsView, HISTOGRAM_BINS,
    OverlayAnchor, OverlayStyle, default_overlay_anchor, histogram, next_overlay_anchor,
    percentile_us,
};
pub use frame_stutter::{ComposeBreakdown, trace_frame_stutter};
pub use input::{
    UserEvent, allowed_gameplay_raw_action, gameplay_dispatch_continues,
    raw_keyboard_capture_enabled, raw_keyboard_restart_screen, screen_accepts_queued_input,
};
pub use input_trace::GameplayInputTrace;
pub use interaction::{ExitIntent, HeldControls, ShellInteractionState};
pub use lighting::{
    SmxAnimationSyncKey, SmxPanelDriver, hide_flags_for_profiles, hide_flags_from_profile,
    load_cabinet_light_chart, screen_light_context,
};
pub use navigation::{
    TransitionAudioPlan, TransitionMusicAction, TransitionMusicPaths, TransitionState,
    actor_entry_transition, actor_fade_out_transition, global_entry_transition,
    global_fade_out_transition, is_actor_fade_screen, is_actor_only_transition,
    machine_flow_screen, menu_exit_uses_fade, screen_from_machine_flow, transition_audio_plan,
    transition_music_action, write_current_screen_file,
};
pub use offset_prompt::{
    GameplayOffsetSavePrompt, GameplayOffsetSaveTargets, GameplayOffsetSnapshot, OffsetPromptInput,
    gameplay_offset_changed, gameplay_offset_prompt_needed, gameplay_offset_prompt_text,
    gameplay_offset_save_targets, gameplay_offset_saveable_changed, route_offset_prompt_input,
};
pub use restart::{RestartPayload, restart_chart_steps, restart_payload_from_eval};
pub use runtime::ShellState;
pub use screenshot::{
    SavedScreenshot, ScreenshotFlowError, append_screenshot_overlay_actors, capture_screenshot,
    replace_screenshot_preview_texture, screenshot_preview_target,
};
pub use session_results::{post_select_display_stages, stage_summary_from_score_info};
pub use stutter_diag::{
    STUTTER_DIAG_FRAME_CAPACITY, STUTTER_DIAG_WINDOW_NS, StutterDiagFrameSample,
    StutterDiagRecorder,
};
pub use window::{
    AppWindowConfig, AppWindowSetup, DisplayModeChange, DisplayModeResult, ResolutionChange,
    ResolutionResult, apply_window_display_mode, apply_window_resolution, create_app_window,
    effective_fullscreen_type, transition_fullscreen_type,
};
