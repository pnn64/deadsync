use deadsync_profile as profile_data;
use deadsync_profile::pad_config as pad_profile_data;
use deadsync_score as score_data;
use deadsync_score::stage_stats;
mod commands;
mod dynamic_media;
mod graphics;
mod input_routing;
mod screen_nav;
mod screenshot;
mod smx_panel_fx;

use self::commands::Command;
use self::dynamic_media::DynamicMedia;
use self::screen_nav::TransitionState;
use self::screenshot::should_auto_screenshot_eval;
use crate::act;
use crate::assets::{AssetManager, PRESENT_TEXTURE_CONTEXT, TextureUploadBudget, visual_styles};
use crate::config::{
    self, DisplayMode, FixedFrameStatsRing, FrameIntervalState, FrameLoopMode,
    FrameLoopModeTracker, GameplayEventBatchTrace, GameplayEventTrace, GameplayPacingTrace,
    OverlayMode, RedrawRequestState, StutterSampleRing, elapsed_us_between, elapsed_us_since,
    seconds_to_us_u32, stutter_severity, update_frame_stats_spike_hold,
};
use crate::screens::{
    DensityGraphSlot, DensityGraphSource, Screen as CurrentScreen, ScreenAction, credits,
    evaluation, evaluation_summary, gameover, gameplay, init, initials, input as input_screen,
    manage_local_profiles, mappings, menu, options, overscan_adjustment, player_options, practice,
    profile_load, sandbox, select_color, select_course, select_mode, select_music, select_profile,
    select_style, test_lights,
};
use crate::{
    GameplayCoreState, gameplay_config_from_config, gameplay_play_style_from_profile,
    gameplay_player_side_from_profile, gameplay_tick_mode_from_profile,
};
use deadlib_platform::dirs;
use deadlib_platform::display;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use deadlib_platform::host_time;
use deadlib_present::color;
use deadlib_present::compose;
use deadlib_present::space::{self as space, Metrics};
use deadlib_render as renderer;
use deadlib_render::{BackendType, PresentModePolicy, SamplerDesc};
use deadlib_renderer as renderer_backend;
use deadsync_assets::media_cache;
use deadsync_online::score_compat as scores;
use deadsync_profile::compat as profile;
use deadsync_profile::pad_config_sync;
use deadsync_simfile::{app_runtime as song_loading, sync_offset};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::Window,
};

use log::{debug, error, info, trace, warn};
use std::borrow::Cow;
use std::collections::HashSet;
use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

#[cfg(all(
    not(windows),
    not(target_os = "linux"),
    not(target_os = "freebsd"),
    not(target_os = "macos")
))]
compile_error!(
    "deadsync control input requires a raw keyboard backend; only Windows, Linux, FreeBSD, and macOS are wired for full app input"
);

use deadlib_present::actors::{Actor, actor_tree_stats};
use deadsync_assets::screenshot::ScreenshotRuntimeState;
use deadsync_chart::STANDARD_DIFFICULTY_COUNT;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    CourseDisplayTiming, CourseDisplayTotals, GameplayQueuedEvent, GameplaySession,
    GameplayViewport, LeadInTiming, ReplayInputEdge, ReplayOffsetSnapshot,
    course_display_timing_for_stages, course_display_totals_for_chart,
    gameplay_offset_prompt_choice_delta, gameplay_raw_key_event,
};
use deadsync_input as logical_input;
use deadsync_input::RawKeyboardEvent;
use deadsync_input::{InputEvent, PadEvent, VirtualAction};
use deadsync_input_fsr as fsr_input;
use deadsync_input_native::{GpSystemEvent, PadBackend};
use deadsync_lights::cabinet_chart::{
    CabinetLightEvent, GameplayLightChartKey, cabinet_light_chart_from_loaded, cabinet_light_key,
    cabinet_light_plan,
};
use deadsync_lights::{self as lights, HideFlags};
#[cfg(test)]
use deadsync_rules::judgment as judgment_rules;
use deadsync_rules::scroll::ScrollSpeedSetting;
#[cfg(test)]
use deadsync_rules::timing as timing_rules;

/* -------------------- user events -------------------- */
#[derive(Debug, Clone)]
pub enum UserEvent {
    Pad(PadEvent),
    Key(RawKeyboardEvent),
    GamepadSystem(GpSystemEvent),
}

/// Imperative effects to be executed by the shell.
/* -------------------- transition timing constants -------------------- */
const FADE_OUT_DURATION: f32 = 0.4;
const MENU_TO_SELECT_COLOR_OUT_DURATION: f32 = 1.0;
const MENU_ACTORS_FADE_DURATION: f32 = 0.65;
const COURSE_MIN_SECONDS_TO_STEP_NEXT_SONG: f32 = 4.0;
const COURSE_MIN_SECONDS_TO_MUSIC_NEXT_SONG: f32 = 0.0;
const GAMEPLAY_OFFSET_PROMPT_Z_BACKDROP: i16 = 31990;
const GAMEPLAY_OFFSET_PROMPT_Z_CURSOR: i16 = 31991;
const GAMEPLAY_OFFSET_PROMPT_Z_TEXT: i16 = 31993;
const GAMEPLAY_PACING_LOG_INTERVAL: Duration = Duration::from_secs(5);
const SCHEDULED_REDRAW_POLL_GUARD: Duration = Duration::from_micros(1_000);
const GAMEPLAY_REDRAW_DELIVERY_SLOW_US: u32 = 1_000;
const GAMEPLAY_REDRAW_DELIVERY_BAD_US: u32 = 2_000;
const GAMEPLAY_PRESENT_SLOW_US: u32 = 1_000;
const GAMEPLAY_PRESENT_SPIKE_US: u32 = 3_000;
const GAMEPLAY_EVENT_TRACE_INTERVAL: Duration = Duration::from_secs(1);
const GAMEPLAY_EVENT_BATCH_SLOW_US: u32 = 1_000;
const GAMEPLAY_EVENT_BATCH_BURST_KEYS: u32 = 8;
const UI_TEXT_LAYOUT_CACHE_LIMIT: usize = 4_096;
const GAMEPLAY_TEXT_LAYOUT_CACHE_LIMIT: usize = 131_072;
const LIVE_TEXTURE_UPLOAD_MAX_OPS: usize = 2;
const LIVE_TEXTURE_UPLOAD_MAX_BYTES: usize = 8 * 1024 * 1024;
const STUTTER_DIAG_DUMP_WINDOW_NS: u64 = 500_000_000;
const STUTTER_DIAG_MIN_DUMP_GAP_NS: u64 = 250_000_000;
const STUTTER_DIAG_FRAME_SAMPLE_COUNT: usize = 128;
const FRAME_STATS_SAMPLE_COUNT: usize = 128;
const SERVICE_SWITCH_PRESSED: &str = "Service switch pressed";

fn gameplay_viewport(metrics: Metrics) -> GameplayViewport {
    GameplayViewport::new(metrics.right - metrics.left, metrics.top - metrics.bottom)
}

fn gameplay_session() -> GameplaySession {
    GameplaySession {
        play_style: gameplay_play_style_from_profile(profile::get_session_play_style()),
        player_side: gameplay_player_side_from_profile(profile::get_session_player_side()),
        joined_sides: std::array::from_fn(|idx| {
            profile::is_session_side_joined(profile_data::player_side_for_index(idx))
        }),
        active_profile_ids: std::array::from_fn(|idx| {
            profile::active_local_profile_id_for_side(profile_data::player_side_for_index(idx))
        }),
        tick_mode: gameplay_tick_mode_from_profile(profile::get_session_timing_tick_mode()),
    }
}

fn load_cabinet_light_chart(
    song: &deadsync_chart::SongData,
    plan: &deadsync_lights::cabinet_chart::CabinetLightPlan,
    global_offset_seconds: f32,
    pack_sync_offset_seconds: f32,
) -> Result<(GameplayLightChartKey, Vec<CabinetLightEvent>), String> {
    let charts =
        song_loading::load_gameplay_charts(song, &plan.request_chart_ixs(), global_offset_seconds)?;
    Ok(cabinet_light_chart_from_loaded(
        song,
        plan,
        &charts,
        global_offset_seconds,
        pack_sync_offset_seconds,
    ))
}

const fn screen_light_context(screen: CurrentScreen) -> lights::ScreenLightContext {
    match screen {
        CurrentScreen::Init => lights::ScreenLightContext::Init,
        CurrentScreen::Gameplay | CurrentScreen::Practice => lights::ScreenLightContext::Gameplay,
        CurrentScreen::TestLights => lights::ScreenLightContext::TestLights,
        CurrentScreen::OverscanAdjustment => lights::ScreenLightContext::OverscanAdjustment,
        CurrentScreen::Evaluation | CurrentScreen::EvaluationSummary | CurrentScreen::Initials => {
            lights::ScreenLightContext::Results
        }
        CurrentScreen::GameOver => lights::ScreenLightContext::GameOver,
        CurrentScreen::Options => lights::ScreenLightContext::Options,
        CurrentScreen::Mappings | CurrentScreen::Input => {
            lights::ScreenLightContext::OperatorLocked
        }
        CurrentScreen::SmxAssignPads => lights::ScreenLightContext::SmxAssignPads,
        CurrentScreen::SelectMusic | CurrentScreen::SelectCourse => {
            lights::ScreenLightContext::SongSelect
        }
        CurrentScreen::Menu
        | CurrentScreen::Credits
        | CurrentScreen::ManageLocalProfiles
        | CurrentScreen::SelectProfile
        | CurrentScreen::ArrowCloudLogin
        | CurrentScreen::GrooveStatsLogin
        | CurrentScreen::SelectColor
        | CurrentScreen::SelectStyle
        | CurrentScreen::SelectPlayMode
        | CurrentScreen::ProfileLoad
        | CurrentScreen::Sandbox
        | CurrentScreen::PlayerOptions
        | CurrentScreen::ConfigurePads => lights::ScreenLightContext::Menu,
    }
}

/// Dedup key for the per-frame SMX background/judgement sync: registry and
/// filesystem lookups in `sync_smx_pad_gifs` only run when one of these
/// fields changed since the last frame.
#[derive(Clone, Copy, PartialEq)]
struct SmxBgKey {
    enabled: bool,
    role: Option<&'static str>,
    bg_packs: [config::SmxPackName; 2],
    judge_packs: [config::SmxPackName; 2],
    /// Current song identity: its `Arc` pointer, cheap to compute per frame
    /// with no allocation. Cleared alongside the scoped cache on a rescan.
    song_id: Option<usize>,
    /// On results screens, the shown grade (as its sprite-state index) and the
    /// difficulty of the chart that earned it, so a new result re-resolves a
    /// grade/difficulty-specific gif even when role and song are unchanged.
    eval_grade: Option<u32>,
    eval_difficulty: Option<&'static str>,
}

fn hide_flags_for_gameplay(state: &GameplayCoreState) -> [HideFlags; 2] {
    std::array::from_fn(|player| hide_flags_from_profile(state.profiles()[player].hide_light_type))
}

const fn hide_flags_from_profile(hide: profile_data::HideLightType) -> HideFlags {
    match hide {
        profile_data::HideLightType::NoHideLights => HideFlags {
            all: false,
            marquee: false,
            bass: false,
        },
        profile_data::HideLightType::HideAllLights => HideFlags {
            all: true,
            marquee: true,
            bass: true,
        },
        profile_data::HideLightType::HideMarqueeLights => HideFlags {
            all: false,
            marquee: true,
            bass: false,
        },
        profile_data::HideLightType::HideBassLights => HideFlags {
            all: false,
            marquee: false,
            bass: true,
        },
    }
}

#[derive(Clone, Copy, Debug)]
struct GameplayOffsetSavePrompt {
    target: CurrentScreen,
    navigate_no_fade: bool,
    active_choice: u8, // 0 = Yes, 1 = No
}

#[derive(Clone)]
struct CourseStageRuntime {
    song: Arc<deadsync_chart::SongData>,
    steps_index: [usize; MAX_PLAYERS],
    preferred_difficulty_index: [usize; MAX_PLAYERS],
}

#[derive(Clone)]
struct CourseRunState {
    path: PathBuf,
    name: String,
    banner_path: Option<PathBuf>,
    score_hash: String,
    course_difficulty_name: String,
    course_meter: Option<u32>,
    course_stepchart_label: String,
    song_stub: Arc<deadsync_chart::SongData>,
    stages: Vec<CourseStageRuntime>,
    course_display_totals: [CourseDisplayTotals; MAX_PLAYERS],
    next_stage_index: usize,
    stage_summaries: Vec<stage_stats::StageSummary>,
    stage_eval_pages: Vec<evaluation::State>,
}

#[inline(always)]
fn stutter_diag_enabled() -> bool {
    log::log_enabled!(log::Level::Trace)
}

#[derive(Clone, Copy, Debug)]
struct StutterDiagFrameSample {
    host_nanos: u64,
    screen: CurrentScreen,
    redraw_request_reason: &'static str,
    frame_us: u32,
    expected_us: u32,
    pre_redraw_gap_us: u32,
    request_to_redraw_us: u32,
    input_us: u32,
    update_us: u32,
    compose_us: u32,
    upload_us: u32,
    draw_us: u32,
    acquire_us: u32,
    submit_us: u32,
    present_us: u32,
    gpu_wait_us: u32,
    draw_setup_us: u32,
    draw_prepare_us: u32,
    draw_record_us: u32,
    display_error_us: i32,
    display_catching_up: bool,
    present_mode: renderer::PresentModeTrace,
    present_display_clock: renderer::ClockDomainTrace,
    present_host_clock: renderer::ClockDomainTrace,
    in_flight_images: u8,
    waited_for_image: bool,
    applied_back_pressure: bool,
    queue_idle_waited: bool,
    suboptimal: bool,
}

impl StutterDiagFrameSample {
    #[inline(always)]
    const fn empty() -> Self {
        Self {
            host_nanos: 0,
            screen: CurrentScreen::Init,
            redraw_request_reason: "none",
            frame_us: 0,
            expected_us: 0,
            pre_redraw_gap_us: 0,
            request_to_redraw_us: 0,
            input_us: 0,
            update_us: 0,
            compose_us: 0,
            upload_us: 0,
            draw_us: 0,
            acquire_us: 0,
            submit_us: 0,
            present_us: 0,
            gpu_wait_us: 0,
            draw_setup_us: 0,
            draw_prepare_us: 0,
            draw_record_us: 0,
            display_error_us: 0,
            display_catching_up: false,
            present_mode: renderer::PresentModeTrace::Unknown,
            present_display_clock: renderer::ClockDomainTrace::Unknown,
            present_host_clock: renderer::ClockDomainTrace::Unknown,
            in_flight_images: 0,
            waited_for_image: false,
            applied_back_pressure: false,
            queue_idle_waited: false,
            suboptimal: false,
        }
    }
}

type StutterDiagRing = FixedFrameStatsRing<StutterDiagFrameSample, STUTTER_DIAG_FRAME_SAMPLE_COUNT>;
type FrameStatsSample = crate::screens::components::shared::frame_stats_overlay::FrameStatsSample;
type FrameStatsRing = FixedFrameStatsRing<FrameStatsSample, FRAME_STATS_SAMPLE_COUNT>;

#[derive(Clone, Copy, Debug, Default)]
struct ComposeBreakdown {
    actor_build_us: u32,
    build_screen_us: u32,
    resolve_textures_us: u32,
    render_objects: u32,
    render_cameras: u32,
    text_layout: compose::TextLayoutFrameStats,
}

#[inline(always)]
const fn saturating_u32(value: usize) -> u32 {
    if value > u32::MAX as usize {
        u32::MAX
    } else {
        value as u32
    }
}

#[inline(always)]
const fn should_background_throttle_unfocused(screen: CurrentScreen) -> bool {
    !matches!(screen, CurrentScreen::Gameplay | CurrentScreen::Practice)
}

/// Shell-level state: timing, window, renderer flags.
pub struct ShellState {
    frame_count: u32,
    last_title_update: Instant,
    last_frame_time: Instant,
    last_frame_end_time: Instant,
    start_time: Instant,
    vsync_enabled: bool,
    frame_interval: Option<Duration>,
    present_mode_policy: PresentModePolicy,
    next_redraw_at: Instant,
    redraw_request: RedrawRequestState,
    frame_loop_mode: FrameLoopModeTracker,
    gameplay_pacing_trace: GameplayPacingTrace,
    gameplay_event_batch_trace: GameplayEventBatchTrace,
    gameplay_event_trace: GameplayEventTrace,
    display_mode: DisplayMode,
    display_monitor: usize,
    metrics: Metrics,
    last_fps: f32,
    last_vpf: u32,
    last_present_stats: renderer::PresentStats,
    current_frame_vpf: u32,
    overlay_mode: OverlayMode,
    stutter_samples: StutterSampleRing,
    stutter_diag_frames: StutterDiagRing,
    stutter_diag_last_audio_trigger_seq: u64,
    stutter_diag_last_display_trigger_seq: u64,
    stutter_diag_last_dump_host_nanos: u64,
    frame_stats: FrameStatsRing,
    frame_stats_scratch: Vec<FrameStatsSample>,
    frame_stats_long: crate::screens::components::shared::frame_stats_overlay::FrameStatsLong,
    frame_stats_spike_us: u32,
    frame_stats_spike_ttl: u16,
    /// EWMA-smoothed audio callback gap (ms) so the readout stops bouncing frame-to-frame.
    /// 0.0 = uninitialized (seeds from the first sample). Reset when the overlay is disabled.
    frame_stats_audio_gap_ms: f32,
    frame_stats_overlay_enabled: bool,
    frame_stats_overlay_anchor:
        crate::screens::components::shared::frame_stats_overlay::OverlayAnchor,
    /// True once the user has explicitly positioned the overlay (via the move-corner key or a
    /// remembered config value). While false the anchor follows the play-context default.
    frame_stats_overlay_anchor_user_set: bool,
    frame_stats_overlay_style:
        crate::screens::components::shared::frame_stats_overlay::OverlayStyle,
    transition: TransitionState,
    display_width: u32,
    display_height: u32,
    pending_window_position: Option<PhysicalPosition<i32>>,
    gamepad_overlay_state: Option<(String, Instant)>,
    pending_exit: bool,
    pending_shutdown: bool,
    shift_held: bool,
    ctrl_held: bool,
    alt_held: bool,
    fast_forward_held: bool,
    slow_down_held: bool,
    tab_acceleration_enabled: bool,
    window_focused: bool,
    window_occluded: bool,
    surface_active: bool,
    screenshot: ScreenshotRuntimeState<profile_data::PlayerSide>,
}

/// Active screen data bundle.
pub struct ScreensState {
    current_screen: CurrentScreen,
    menu_state: menu::State,
    gameplay_state: Option<gameplay::State>,
    practice_state: Option<practice::State>,
    options_state: options::State,
    credits_state: credits::State,
    manage_local_profiles_state: manage_local_profiles::State,
    mappings_state: mappings::State,
    input_state: input_screen::State,
    pad_config_state: crate::screens::pad_config::State,
    test_lights_state: test_lights::State,
    overscan_adjustment_state: overscan_adjustment::State,
    smx_assign_state: crate::screens::smx_assign::State,
    /// Latched while a same-jumper SMX conflict is being auto-prompted, so the
    /// assignment screen is only opened once per conflict episode (cleared when
    /// the conflict resolves). See `App::maybe_autoprompt_smx_assign`.
    smx_autoprompt_latched: bool,
    smx_options_light_preview: deadsync_smx::OptionsLightPreview,
    smx_po_light_preview: deadsync_smx::PlayerOptionsLightPreview,
    player_options_state: Option<player_options::State>,
    init_state: init::State,
    select_profile_state: select_profile::State,
    select_color_state: select_color::State,
    arrowcloud_login_state: crate::screens::arrowcloud_login::State,
    groovestats_login_state: crate::screens::groovestats_login::State,
    select_style_state: select_style::State,
    select_play_mode_state: select_mode::State,
    profile_load_state: profile_load::State,
    select_music_state: select_music::State,
    select_course_state: select_course::State,
    sandbox_state: sandbox::State,
    evaluation_state: evaluation::State,
    evaluation_summary_state: evaluation_summary::State,
    initials_state: initials::State,
    gameover_state: gameover::State,
}

/// Session-wide values that survive screen swaps.
pub struct SessionState {
    preferred_difficulty_index: usize,
    session_start_time: Option<Instant>,
    played_stages: Vec<stage_stats::StageSummary>,
    pending_post_select_summary_exit: bool,
    course_individual_stage_indices: Vec<usize>,
    combo_carry: [u32; MAX_PLAYERS],
    gameplay_restart_count: u32,
    /// SL/zmod parity: when a restart key is pressed mid-gameplay, the gameplay
    /// state runs its fast Cancel exit. This flag intercepts the resulting
    /// `NavigateNoFade(SelectMusic)` and redirects it back to Gameplay so the
    /// player skips the long out-transition.
    restart_pending: bool,
    course_run: Option<CourseRunState>,
    course_eval_pages: Vec<evaluation::State>,
    course_eval_page_index: usize,
    last_course_wheel_path: Option<PathBuf>,
    last_course_wheel_difficulty_name: Option<String>,
}

/// Pure-ish container for the high-level game state.
/// This keeps screen flow, timing and UI state separate from the window/renderer shell.
pub struct AppState {
    shell: ShellState,
    screens: ScreensState,
    session: SessionState,
    gameplay_offset_save_prompt: Option<GameplayOffsetSavePrompt>,
}

impl ShellState {
    fn new(cfg: &config::Config, overlay_mode: u8) -> Self {
        let metrics = space::metrics_for_window(cfg.display_width, cfg.display_height);
        let now = Instant::now();
        let frame_interval = config::frame_interval_for_max_fps(cfg.max_fps);
        Self {
            frame_count: 0,
            last_title_update: now,
            last_frame_time: now,
            last_frame_end_time: now,
            start_time: now,
            vsync_enabled: cfg.vsync,
            frame_interval,
            present_mode_policy: cfg.present_mode_policy,
            next_redraw_at: now,
            redraw_request: RedrawRequestState::new(),
            frame_loop_mode: FrameLoopModeTracker::new(),
            gameplay_pacing_trace: GameplayPacingTrace::new(now),
            gameplay_event_batch_trace: GameplayEventBatchTrace::new(now),
            gameplay_event_trace: GameplayEventTrace::new(now),
            display_mode: cfg.display_mode(),
            metrics,
            last_fps: 0.0,
            last_vpf: 0,
            last_present_stats: renderer::PresentStats::default(),
            current_frame_vpf: 0,
            overlay_mode: OverlayMode::from_code(overlay_mode),
            stutter_samples: StutterSampleRing::new(),
            stutter_diag_frames: StutterDiagRing::new(StutterDiagFrameSample::empty()),
            stutter_diag_last_audio_trigger_seq: 0,
            stutter_diag_last_display_trigger_seq: 0,
            stutter_diag_last_dump_host_nanos: 0,
            frame_stats: FrameStatsRing::new(FrameStatsSample::empty()),
            frame_stats_scratch: Vec::new(),
            frame_stats_long:
                crate::screens::components::shared::frame_stats_overlay::FrameStatsLong::new(),
            frame_stats_spike_us: 0,
            frame_stats_spike_ttl: 0,
            frame_stats_audio_gap_ms: 0.0,
            frame_stats_overlay_enabled: false,
            frame_stats_overlay_anchor:
                crate::screens::components::shared::frame_stats_overlay::OverlayAnchor::from_key(
                    cfg.frame_stats_overlay_anchor,
                )
                .unwrap_or(
                    crate::screens::components::shared::frame_stats_overlay::OverlayAnchor::TopLeft,
                ),
            frame_stats_overlay_anchor_user_set:
                crate::screens::components::shared::frame_stats_overlay::OverlayAnchor::from_key(
                    cfg.frame_stats_overlay_anchor,
                )
                .is_some(),
            frame_stats_overlay_style:
                crate::screens::components::shared::frame_stats_overlay::OverlayStyle::from_key(
                    cfg.frame_stats_overlay_style,
                ),
            transition: TransitionState::Idle,
            display_width: cfg.display_width,
            display_height: cfg.display_height,
            display_monitor: cfg.display_monitor,
            pending_window_position: None,
            gamepad_overlay_state: None,
            pending_exit: false,
            pending_shutdown: false,
            shift_held: false,
            ctrl_held: false,
            alt_held: false,
            fast_forward_held: false,
            slow_down_held: false,
            tab_acceleration_enabled: cfg.tab_acceleration,
            // Default to unfocused so background input backends (Win32 RawInput,
            // evdev, IOHID) drop globally-observed key events until the window
            // is created and proven focused.
            window_focused: false,
            window_occluded: false,
            surface_active: cfg.display_width > 0 && cfg.display_height > 0,
            screenshot: ScreenshotRuntimeState::new(),
        }
    }

    #[inline(always)]
    fn set_max_fps(&mut self, max_fps: u16) {
        self.frame_interval = config::frame_interval_for_max_fps(max_fps);
        self.next_redraw_at = Instant::now();
        self.frame_loop_mode.reset();
    }

    #[inline(always)]
    fn set_present_mode_policy(&mut self, policy: PresentModePolicy) {
        self.present_mode_policy = policy;
        self.next_redraw_at = Instant::now();
        self.frame_loop_mode.reset();
    }

    #[inline(always)]
    fn reset_frame_clock(&mut self, now: Instant) {
        self.last_frame_time = now;
        self.last_frame_end_time = now;
        self.next_redraw_at = now;
        self.redraw_request.reset();
        self.frame_loop_mode.reset();
        self.gameplay_pacing_trace.reset(now);
        self.gameplay_event_batch_trace.reset(now);
        self.gameplay_event_trace.reset(now);
        self.stutter_diag_frames.clear();
        self.stutter_diag_last_dump_host_nanos = 0;
    }

    #[inline(always)]
    fn note_redraw_requested(&mut self, now: Instant, reason: &'static str) {
        self.redraw_request.note_requested(now, reason);
    }

    #[inline(always)]
    fn take_redraw_request_timing(&mut self, now: Instant) -> (u32, &'static str) {
        let timing = self.redraw_request.take_timing(now);
        (timing.request_to_redraw_us, timing.reason)
    }

    #[inline(always)]
    fn redraw_pending(&self) -> bool {
        self.redraw_request.pending()
    }

    #[inline(always)]
    fn frame_interval_state(&self, screen: CurrentScreen) -> FrameIntervalState {
        config::window_frame_interval_state(
            self.vsync_enabled,
            self.frame_interval,
            self.window_occluded,
            self.surface_active,
            self.window_focused,
            should_background_throttle_unfocused(screen),
        )
    }

    #[inline(always)]
    fn note_frame_loop_mode(&mut self, mode: FrameLoopMode) -> bool {
        self.frame_loop_mode.note(mode)
    }

    #[inline(always)]
    fn note_new_events(&mut self, now: Instant) {
        self.gameplay_event_batch_trace.reset(now);
    }

    #[inline(always)]
    fn note_gameplay_key_handler(&mut self, gameplay_screen: bool, repeat: bool, handler_us: u32) {
        if !gameplay_screen {
            return;
        }
        let trace = &mut self.gameplay_event_batch_trace;
        trace.gameplay_seen = true;
        trace.key_events = trace.key_events.saturating_add(1);
        trace.key_repeat_events = trace.key_repeat_events.saturating_add(repeat as u32);
        trace.app_handler_sum_us = trace
            .app_handler_sum_us
            .saturating_add(u64::from(handler_us));
        trace.app_handler_max_us = trace.app_handler_max_us.max(handler_us);
    }

    #[inline(always)]
    fn note_gameplay_pad_handler(&mut self, gameplay_screen: bool, handler_us: u32) {
        if !gameplay_screen {
            return;
        }
        let trace = &mut self.gameplay_event_batch_trace;
        trace.gameplay_seen = true;
        trace.pad_events = trace.pad_events.saturating_add(1);
        trace.app_handler_sum_us = trace
            .app_handler_sum_us
            .saturating_add(u64::from(handler_us));
        trace.app_handler_max_us = trace.app_handler_max_us.max(handler_us);
    }

    #[inline(always)]
    fn note_gameplay_queued_input(&mut self) {
        let trace = &mut self.gameplay_event_batch_trace;
        trace.gameplay_seen = true;
        trace.queued_events = trace.queued_events.saturating_add(1);
    }

    fn finish_gameplay_event_batch(&mut self, now: Instant, screen: CurrentScreen) {
        let trace = &mut self.gameplay_event_batch_trace;
        if !trace.gameplay_seen
            || (trace.key_events == 0 && trace.pad_events == 0 && trace.queued_events == 0)
        {
            if now.duration_since(self.gameplay_event_trace.started_at)
                >= GAMEPLAY_EVENT_TRACE_INTERVAL
            {
                self.gameplay_event_trace.reset(now);
            }
            trace.reset(now);
            return;
        }

        let batch_us = elapsed_us_between(now, trace.started_at);
        let app_handler_sum_us = trace.app_handler_sum_us.min(u64::from(u32::MAX)) as u32;
        let dispatch_overhead_us = batch_us.saturating_sub(app_handler_sum_us);
        if batch_us >= GAMEPLAY_EVENT_BATCH_SLOW_US
            || trace.key_events >= GAMEPLAY_EVENT_BATCH_BURST_KEYS
        {
            trace!(
                "Gameplay event batch: screen={:?} keys={} repeats={} pads={} queued={} batch_ms={:.3} app_ms={:.3} dispatch_ms={:.3} app_max_ms={:.3}",
                screen,
                trace.key_events,
                trace.key_repeat_events,
                trace.pad_events,
                trace.queued_events,
                batch_us as f32 / 1000.0,
                app_handler_sum_us as f32 / 1000.0,
                dispatch_overhead_us as f32 / 1000.0,
                trace.app_handler_max_us as f32 / 1000.0
            );
        }

        let summary = &mut self.gameplay_event_trace;
        summary.batches = summary.batches.saturating_add(1);
        summary.key_events = summary.key_events.saturating_add(trace.key_events);
        summary.key_repeat_events = summary
            .key_repeat_events
            .saturating_add(trace.key_repeat_events);
        summary.pad_events = summary.pad_events.saturating_add(trace.pad_events);
        summary.queued_events = summary.queued_events.saturating_add(trace.queued_events);
        summary.batch_sum_us = summary.batch_sum_us.saturating_add(u64::from(batch_us));
        summary.batch_max_us = summary.batch_max_us.max(batch_us);
        summary.app_handler_sum_us = summary
            .app_handler_sum_us
            .saturating_add(trace.app_handler_sum_us);
        summary.app_handler_max_us = summary.app_handler_max_us.max(trace.app_handler_max_us);
        summary.dispatch_overhead_sum_us = summary
            .dispatch_overhead_sum_us
            .saturating_add(u64::from(dispatch_overhead_us));
        summary.dispatch_overhead_max_us =
            summary.dispatch_overhead_max_us.max(dispatch_overhead_us);
        summary.slow_batches = summary
            .slow_batches
            .saturating_add((batch_us >= GAMEPLAY_EVENT_BATCH_SLOW_US) as u32);

        if now.duration_since(summary.started_at) >= GAMEPLAY_EVENT_TRACE_INTERVAL {
            let batches = summary.batches.max(1);
            trace!(
                "Gameplay raw input: batches={} keys={} repeats={} pads={} queued={} batch_ms=[avg:{:.3} max:{:.3}] app_ms=[avg:{:.3} max:{:.3}] dispatch_ms=[avg:{:.3} max:{:.3}] slow_batches={}",
                summary.batches,
                summary.key_events,
                summary.key_repeat_events,
                summary.pad_events,
                summary.queued_events,
                summary.batch_sum_us as f32 / batches as f32 / 1000.0,
                summary.batch_max_us as f32 / 1000.0,
                summary.app_handler_sum_us as f32 / batches as f32 / 1000.0,
                summary.app_handler_max_us as f32 / 1000.0,
                summary.dispatch_overhead_sum_us as f32 / batches as f32 / 1000.0,
                summary.dispatch_overhead_max_us as f32 / 1000.0,
                summary.slow_batches
            );
            summary.reset(now);
        }
        trace.reset(now);
    }

    #[inline(always)]
    fn set_window_focus(&mut self, focused: bool, now: Instant) -> bool {
        if self.window_focused == focused {
            return false;
        }
        self.window_focused = focused;
        self.reset_frame_clock(now);
        true
    }

    #[inline(always)]
    fn set_window_occluded(&mut self, occluded: bool, now: Instant) -> bool {
        if self.window_occluded == occluded {
            return false;
        }
        self.window_occluded = occluded;
        self.reset_frame_clock(now);
        true
    }

    #[inline(always)]
    fn set_surface_active(&mut self, active: bool, now: Instant) -> bool {
        if self.surface_active == active {
            return false;
        }
        self.surface_active = active;
        self.reset_frame_clock(now);
        true
    }

    #[inline(always)]
    fn background_frame_interval(&self, screen: CurrentScreen) -> Option<Duration> {
        self.frame_interval_state(screen).interval
    }

    #[inline(always)]
    fn should_skip_compose_and_draw(&self) -> bool {
        config::should_skip_compose_and_draw(self.window_occluded, self.surface_active)
    }

    #[inline(always)]
    fn set_overlay_mode(&mut self, mode: u8) {
        let next = OverlayMode::from_code(mode);
        if self.overlay_mode.shows_stutter() && !next.shows_stutter() {
            self.clear_stutter_samples();
        }
        self.overlay_mode = next;
    }

    #[inline(always)]
    fn cycle_overlay_mode(&mut self) -> u8 {
        let prev = self.overlay_mode;
        self.overlay_mode = self.overlay_mode.next();
        if prev.shows_stutter() && !self.overlay_mode.shows_stutter() {
            self.clear_stutter_samples();
        }
        self.overlay_mode.code()
    }

    #[inline(always)]
    fn toggle_frame_stats_overlay(&mut self) -> bool {
        self.frame_stats_overlay_enabled = !self.frame_stats_overlay_enabled;
        if !self.frame_stats_overlay_enabled {
            self.frame_stats.clear();
            self.frame_stats_long.reset();
            self.frame_stats_spike_us = 0;
            self.frame_stats_spike_ttl = 0;
            self.frame_stats_audio_gap_ms = 0.0;
        }
        self.frame_stats_overlay_enabled
    }

    fn cycle_frame_stats_overlay_anchor(
        &mut self,
        compact: bool,
    ) -> crate::screens::components::shared::frame_stats_overlay::OverlayAnchor {
        use crate::screens::components::shared::frame_stats_overlay as fso;
        self.frame_stats_overlay_anchor =
            fso::next_anchor(self.frame_stats_overlay_anchor, compact);
        // The user explicitly positioned it: remember this corner across toggles + restarts
        // instead of snapping back to the play-context default.
        self.frame_stats_overlay_anchor_user_set = true;
        config::update_frame_stats_overlay_anchor(self.frame_stats_overlay_anchor.to_key());
        self.frame_stats_overlay_anchor
    }

    #[inline(always)]
    fn toggle_frame_stats_overlay_style(
        &mut self,
    ) -> crate::screens::components::shared::frame_stats_overlay::OverlayStyle {
        self.frame_stats_overlay_style = self.frame_stats_overlay_style.toggle();
        config::update_frame_stats_overlay_style(self.frame_stats_overlay_style.label());
        self.frame_stats_overlay_style
    }

    #[inline(always)]
    fn push_stutter_sample(
        &mut self,
        at_seconds: f32,
        frame_seconds: f32,
        expected_seconds: f32,
        severity: u8,
    ) {
        self.stutter_samples
            .push(at_seconds, frame_seconds, expected_seconds, severity);
    }

    #[inline(always)]
    fn clear_stutter_samples(&mut self) {
        self.stutter_samples.clear();
    }

    fn update_gamepad_overlay(&mut self, now: Instant) {
        if let Some((_, start_time)) = self.gamepad_overlay_state {
            const HOLD_DURATION: f32 = 3.33;
            const FADE_OUT_DURATION: f32 = 0.25;
            const TOTAL_DURATION: f32 = HOLD_DURATION + FADE_OUT_DURATION;
            if now.duration_since(start_time).as_secs_f32() > TOTAL_DURATION {
                self.gamepad_overlay_state = None;
            }
        }
    }
}

impl SessionState {
    fn new(preferred_difficulty_index: usize, combo_carry: [u32; MAX_PLAYERS]) -> Self {
        Self {
            preferred_difficulty_index,
            session_start_time: None,
            played_stages: Vec::new(),
            pending_post_select_summary_exit: false,
            course_individual_stage_indices: Vec::new(),
            combo_carry,
            gameplay_restart_count: 0,
            restart_pending: false,
            course_run: None,
            course_eval_pages: Vec::new(),
            course_eval_page_index: 0,
            last_course_wheel_path: None,
            last_course_wheel_difficulty_name: None,
        }
    }
}

fn course_stage_runtime_from_plan(
    plan: &select_course::CourseStagePlan,
    chart_type: &str,
) -> Option<CourseStageRuntime> {
    let steps_idx = plan
        .song
        .steps_index_for_chart_hash(chart_type, plan.chart_hash.as_str())?;
    Some(CourseStageRuntime {
        song: plan.song.clone(),
        steps_index: [steps_idx; MAX_PLAYERS],
        preferred_difficulty_index: [steps_idx; MAX_PLAYERS],
    })
}

fn build_course_run_from_selection(
    selection: select_course::SelectedCoursePlan,
) -> Option<CourseRunState> {
    let chart_type = profile::get_session_play_style().chart_type();
    let mut stages = Vec::with_capacity(selection.stages.len());
    for stage in &selection.stages {
        if let Some(runtime) = course_stage_runtime_from_plan(stage, chart_type) {
            stages.push(runtime);
        }
    }
    if stages.is_empty() {
        return None;
    }
    let mut course_display_totals = [CourseDisplayTotals::default(); MAX_PLAYERS];
    for stage in &stages {
        for (player_idx, total) in course_display_totals.iter_mut().enumerate() {
            let Some(chart) = stage
                .song
                .chart_for_steps_index(chart_type, stage.steps_index[player_idx])
            else {
                continue;
            };
            let add = course_display_totals_for_chart(chart);
            total.possible_grade_points = total
                .possible_grade_points
                .saturating_add(add.possible_grade_points);
            total.total_steps = total.total_steps.saturating_add(add.total_steps);
            total.holds_total = total.holds_total.saturating_add(add.holds_total);
            total.rolls_total = total.rolls_total.saturating_add(add.rolls_total);
            total.mines_total = total.mines_total.saturating_add(add.mines_total);
        }
    }
    Some(CourseRunState {
        path: selection.path.clone(),
        name: selection.name,
        banner_path: selection.banner_path,
        score_hash: select_course::course_score_hash(selection.path.as_path()),
        course_difficulty_name: selection.course_difficulty_name,
        course_meter: selection.course_meter,
        course_stepchart_label: selection.course_stepchart_label,
        song_stub: selection.song_stub,
        stages,
        course_display_totals,
        next_stage_index: 0,
        stage_summaries: Vec::new(),
        stage_eval_pages: Vec::new(),
    })
}

fn build_course_graph_stages(
    course: &CourseRunState,
) -> [Vec<evaluation::CourseGraphStage>; MAX_PLAYERS] {
    let chart_type = profile::get_session_play_style().chart_type();
    std::array::from_fn(|player_idx| {
        let mut out = Vec::with_capacity(course.stages.len());
        for stage in &course.stages {
            let Some(chart) = stage
                .song
                .chart_for_steps_index(chart_type, stage.steps_index[player_idx])
            else {
                continue;
            };
            out.push(evaluation::CourseGraphStage {
                chart: Arc::new(chart.clone()),
                song_last_second: stage.song.precise_last_second(),
            });
        }
        out
    })
}

fn course_stage_seconds(stage: &CourseStageRuntime) -> f32 {
    let seconds = stage.song.precise_last_second();
    if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    }
}

fn course_total_seconds(course: &CourseRunState) -> f32 {
    course.stages.iter().map(course_stage_seconds).sum()
}

fn course_display_timing_for_run(course: &CourseRunState) -> CourseDisplayTiming {
    course_display_timing_for_stages(&course.stages, course.next_stage_index, |stage| {
        stage.song.music_length_seconds
    })
}

fn build_course_summary_stage(course: &CourseRunState) -> Option<stage_stats::StageSummary> {
    let totals = course
        .course_display_totals
        .map(|total| stage_stats::CourseSummaryTotals {
            possible_grade_points: total.possible_grade_points,
            total_steps: total.total_steps,
            holds_total: total.holds_total,
            rolls_total: total.rolls_total,
            mines_total: total.mines_total,
        });
    stage_stats::build_course_summary_stage(stage_stats::CourseSummaryInput {
        path: course.path.as_path(),
        name: course.name.as_str(),
        banner_path: course.banner_path.as_deref(),
        score_hash: course.score_hash.as_str(),
        difficulty_name: course.course_difficulty_name.as_str(),
        meter: course.course_meter,
        song_stub: course.song_stub.as_ref(),
        course_total_seconds: course_total_seconds(course),
        totals,
        stage_summaries: course.stage_summaries.as_slice(),
    })
}

fn score_info_from_stage(
    stage: &stage_stats::StageSummary,
    side: profile_data::PlayerSide,
) -> Option<evaluation::ScoreInfo> {
    let idx = profile_data::player_side_index(side);
    let player = stage.players[idx].as_ref()?;
    let judgment_counts = [
        player
            .window_counts
            .w0
            .saturating_add(player.window_counts.w1),
        player.window_counts.w2,
        player.window_counts.w3,
        player.window_counts.w4,
        player.window_counts.w5,
        player.window_counts.miss,
    ];

    let chart_hash = player.chart.short_hash.as_str();
    let machine_records = scores::get_machine_leaderboard_local(chart_hash, usize::MAX);
    let personal_records =
        scores::get_personal_leaderboard_local_for_side(chart_hash, side, usize::MAX);
    let machine_record_highlight_rank =
        score_data::leaderboard_rank_for_score(machine_records.as_slice(), player.score_percent);
    let personal_record_highlight_rank =
        score_data::leaderboard_rank_for_score(personal_records.as_slice(), player.score_percent);
    let local_score_valid = player.score_valid && !player.disqualified;
    let earned_machine_record =
        local_score_valid && machine_record_highlight_rank.is_some_and(|rank| rank <= 10);
    let earned_top2_personal =
        local_score_valid && personal_record_highlight_rank.is_some_and(|rank| rank <= 2);
    let machine_record_highlight_rank = local_score_valid
        .then_some(machine_record_highlight_rank)
        .flatten();
    let personal_record_highlight_rank = local_score_valid
        .then_some(personal_record_highlight_rank)
        .flatten();

    Some(evaluation::ScoreInfo {
        song: stage.song.clone(),
        chart: player.chart.clone(),
        course_graph_stages: Vec::new(),
        side,
        profile_name: player.profile_name.clone(),
        score_valid: player.score_valid,
        disqualified: player.disqualified,
        expected_groovestats_submit: false,
        expected_arrowcloud_submit: false,
        groovestats: player.groovestats.clone(),
        itl: player.itl.clone(),
        judgment_counts,
        score_percent: player.score_percent,
        earned_grade_points: player.earned_grade_points,
        possible_grade_points: player.possible_grade_points,
        grade: player.grade,
        speed_mod: profile::get_for_side(side).scroll_speed,
        mods_text: {
            let profile = profile::get_for_side(side);
            profile_data::evaluation_mods_text(&profile, profile.scroll_speed)
        },
        hands_achieved: player.hands_achieved,
        hands_total: player.hands_total,
        holds_held: player.holds_held,
        holds_held_for_score: player.holds_held_for_score,
        holds_total: player.holds_total,
        rolls_held: player.rolls_held,
        rolls_held_for_score: player.rolls_held_for_score,
        rolls_total: player.rolls_total,
        mines_hit_for_score: player.mines_hit_for_score,
        mines_avoided: player.mines_avoided,
        mines_total: player.mines_total,
        timing: player.timing,
        arrow_timing: player.arrow_timing.clone(),
        scatter: player.scatter.clone(),
        scatter_worst_window_ms: player.scatter_worst_window_ms,
        histogram: player.histogram.clone(),
        graph_first_second: player.graph_first_second,
        graph_last_second: player.graph_last_second,
        music_rate: if stage.music_rate.is_finite() && stage.music_rate > 0.0 {
            stage.music_rate
        } else {
            1.0
        },
        life_history: player.life_history.clone(),
        fail_time: player.fail_time.or_else(|| {
            (player.grade == score_data::Grade::Failed).then_some(stage.duration_seconds)
        }),
        window_counts: player.window_counts,
        window_counts_10ms: player.window_counts_10ms,
        ex_score_percent: player.ex_score_percent,
        hard_ex_score_percent: player.hard_ex_score_percent,
        calories_burned: player.calories_burned,
        column_judgments: Vec::new(),
        noteskin: None,
        show_fa_plus_window: player.show_w0,
        show_ex_score: player.show_ex_score,
        show_hard_ex_score: player.show_hard_ex_score,
        show_fa_plus_pane: player.show_fa_plus_pane,
        track_early_judgments: player.track_early_judgments,
        disabled_timing_windows: profile::get_for_side(side)
            .timing_windows
            .disabled_windows(),
        machine_records,
        machine_record_highlight_rank,
        personal_records,
        personal_record_highlight_rank,
        show_machine_personal_split: !earned_machine_record && earned_top2_personal,
    })
}

#[inline(always)]
fn add_column_judgments(dst: &mut evaluation::ColumnJudgments, src: evaluation::ColumnJudgments) {
    dst.w0 = dst.w0.saturating_add(src.w0);
    dst.w1 = dst.w1.saturating_add(src.w1);
    dst.w2 = dst.w2.saturating_add(src.w2);
    dst.w3 = dst.w3.saturating_add(src.w3);
    dst.w4 = dst.w4.saturating_add(src.w4);
    dst.w5 = dst.w5.saturating_add(src.w5);
    dst.miss = dst.miss.saturating_add(src.miss);
    dst.early_w1 = dst.early_w1.saturating_add(src.early_w1);
    dst.early_w2 = dst.early_w2.saturating_add(src.early_w2);
    dst.early_w3 = dst.early_w3.saturating_add(src.early_w3);
    dst.early_w4 = dst.early_w4.saturating_add(src.early_w4);
    dst.early_w5 = dst.early_w5.saturating_add(src.early_w5);
    dst.early_total_w0 = dst.early_total_w0.saturating_add(src.early_total_w0);
    dst.early_total_w1 = dst.early_total_w1.saturating_add(src.early_total_w1);
    dst.early_total_w2 = dst.early_total_w2.saturating_add(src.early_total_w2);
    dst.early_total_w3 = dst.early_total_w3.saturating_add(src.early_total_w3);
    dst.early_total_w4 = dst.early_total_w4.saturating_add(src.early_total_w4);
    dst.early_total_w5 = dst.early_total_w5.saturating_add(src.early_total_w5);
    dst.held_miss = dst.held_miss.saturating_add(src.held_miss);
}

fn merge_column_judgments(
    dst: &mut Vec<evaluation::ColumnJudgments>,
    src: &[evaluation::ColumnJudgments],
) {
    if dst.len() < src.len() {
        dst.resize(src.len(), evaluation::ColumnJudgments::default());
    }
    for (dst, src) in dst.iter_mut().zip(src.iter().copied()) {
        add_column_judgments(dst, src);
    }
}

fn score_info_for_side(
    score_info: &[Option<evaluation::ScoreInfo>; MAX_PLAYERS],
    side: profile_data::PlayerSide,
) -> Option<&evaluation::ScoreInfo> {
    score_info.iter().flatten().find(|si| si.side == side)
}

fn apply_course_summary_column_judgments(
    course_page: &mut evaluation::State,
    song_pages: &[evaluation::State],
) {
    for summary in course_page.score_info.iter_mut().flatten() {
        let mut columns = Vec::new();
        let mut noteskin = None;
        for page in song_pages {
            let Some(song) = score_info_for_side(&page.score_info, summary.side) else {
                continue;
            };
            merge_column_judgments(&mut columns, &song.column_judgments);
            if noteskin.is_none() && song.noteskin.is_some() {
                noteskin.clone_from(&song.noteskin);
            }
        }
        summary.column_judgments = columns;
        if summary.noteskin.is_none() {
            summary.noteskin = noteskin;
        }
    }
}

fn build_course_summary_eval_state(
    stage: &stage_stats::StageSummary,
    course_graph_stages: &[Vec<evaluation::CourseGraphStage>; MAX_PLAYERS],
    active_color_index: i32,
    session_elapsed: f32,
    gameplay_elapsed: f32,
) -> evaluation::State {
    let mut score_info: [Option<evaluation::ScoreInfo>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    match profile::get_session_play_style() {
        profile_data::PlayStyle::Versus => {
            for side in [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2] {
                let idx = profile_data::player_side_index(side);
                score_info[idx] = score_info_from_stage(stage, side);
                if let Some(si) = score_info[idx].as_mut() {
                    si.course_graph_stages.clone_from(&course_graph_stages[idx]);
                }
            }
        }
        profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
            let side = profile::get_session_player_side();
            let idx = profile_data::player_side_index(side);
            score_info[0] = score_info_from_stage(stage, side);
            if let Some(si) = score_info[0].as_mut() {
                si.course_graph_stages.clone_from(&course_graph_stages[idx]);
            }
        }
    }
    let mut state = evaluation::init_from_score_info(score_info, stage.duration_seconds);
    state.active_color_index = active_color_index;
    state.session_elapsed = session_elapsed;
    state.gameplay_elapsed = gameplay_elapsed;
    state.return_to_course = true;
    state.allow_online_panes = false;
    state
}

fn gameplay_song_lua_video_paths(state: &gameplay::State) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    let song_lua_visuals = state.song_lua_visuals();
    deadsync_song_lua::push_song_lua_video_paths(&song_lua_visuals.overlays, &mut seen, &mut paths);
    for layer in &song_lua_visuals.background_visual_layers {
        deadsync_song_lua::push_song_lua_video_paths(&layer.overlays, &mut seen, &mut paths);
    }
    for layer in &song_lua_visuals.foreground_visual_layers {
        deadsync_song_lua::push_song_lua_video_paths(&layer.overlays, &mut seen, &mut paths);
    }
    paths
}

fn gameplay_overlay_video_paths(state: &gameplay::State) -> Vec<PathBuf> {
    let mut paths = gameplay_song_lua_video_paths(state);
    if let Some(path) = state.song().active_foreground_path(state.current_beat())
        && deadlib_assets::dynamic::is_dynamic_video_path(path)
        && !paths.iter().any(|existing| existing == path)
    {
        paths.push(path.clone());
    }
    paths
}

fn prewarm_gameplay_assets(
    assets: &mut AssetManager,
    backend: &mut renderer_backend::Backend,
    state: &gameplay::State,
) {
    fn prewarm_model_texture_key(
        assets: &mut AssetManager,
        backend: &mut renderer_backend::Backend,
        seen: &mut HashSet<String>,
        seen_model_textures: &mut HashSet<String>,
        key: &str,
    ) {
        let key = crate::assets::canonical_texture_key(key);
        if !seen_model_textures.insert(key.clone()) {
            return;
        }
        assets.ensure_texture_for_key_with_sampler(
            backend,
            &key,
            deadsync_assets::textures::model_texture_sampler(&key),
        );
        seen.insert(key);
    }

    fn prewarm_noteskin_textures(
        assets: &mut AssetManager,
        backend: &mut renderer_backend::Backend,
        seen: &mut HashSet<String>,
        seen_model_textures: &mut HashSet<String>,
        noteskin: &deadsync_assets::noteskin::Noteskin,
    ) {
        noteskin.for_each_slot(|slot| {
            let key = slot.texture_key();
            if seen.insert(key.to_owned()) {
                assets.ensure_texture_for_key(backend, key);
            }
        });
        noteskin.for_each_slot(|slot| {
            if slot.model.is_some() {
                prewarm_model_texture_key(
                    assets,
                    backend,
                    seen,
                    seen_model_textures,
                    slot.texture_key(),
                )
            }
        });
    }

    let mut seen = HashSet::<String>::with_capacity(256);
    let mut seen_model_textures = HashSet::<String>::with_capacity(64);
    let mut seen_song_lua_fonts = HashSet::<&'static str>::with_capacity(8);
    for noteskin in state.noteskin_assets.noteskin.iter().flatten() {
        prewarm_noteskin_textures(
            assets,
            backend,
            &mut seen,
            &mut seen_model_textures,
            noteskin,
        );
    }
    for noteskin in state.noteskin_assets.mine_noteskin.iter().flatten() {
        prewarm_noteskin_textures(
            assets,
            backend,
            &mut seen,
            &mut seen_model_textures,
            noteskin,
        );
    }
    for noteskin in state.noteskin_assets.receptor_noteskin.iter().flatten() {
        prewarm_noteskin_textures(
            assets,
            backend,
            &mut seen,
            &mut seen_model_textures,
            noteskin,
        );
    }
    for noteskin in state
        .noteskin_assets
        .tap_explosion_noteskin
        .iter()
        .flatten()
    {
        prewarm_noteskin_textures(
            assets,
            backend,
            &mut seen,
            &mut seen_model_textures,
            noteskin,
        );
    }
    let song = state.song();
    let mut media_paths = Vec::with_capacity(
        deadsync_assets::dynamic_media::gameplay_media_paths_capacity(
            song,
            &state.background_changes,
        ),
    );
    deadsync_assets::dynamic_media::push_gameplay_media_paths(
        &mut media_paths,
        song,
        &state.background_changes,
    );
    for path in media_paths {
        let key = path.to_string_lossy().into_owned();
        if seen.insert(key) {
            media_cache::ensure_banner_texture(assets, backend, path);
        }
    }
    let mut prewarm_song_lua_overlays =
        |overlays: &[deadsync_assets::song_lua::SongLuaOverlayActor]| {
            for overlay in overlays {
                match &overlay.kind {
                    deadsync_assets::song_lua::SongLuaOverlayKind::BitmapText {
                        font_name,
                        font_path,
                        ..
                    } => {
                        if seen_song_lua_fonts.insert(*font_name)
                            && assets.with_font(font_name, |_| ()).is_none()
                            && let Err(err) =
                                assets.load_font_from_ini_path(backend, *font_name, font_path)
                        {
                            warn!(
                                "Failed to load song lua bitmap font '{}': {}",
                                font_path.display(),
                                err
                            );
                        }
                    }
                    deadsync_assets::song_lua::SongLuaOverlayKind::Sprite {
                        texture_path,
                        texture_key,
                    } => {
                        let key = texture_key.as_ref();
                        let first_seen = seen.insert(key.to_owned());
                        let sampler = deadsync_assets::song_lua::overlay_sampler(overlay);
                        if sampler != SamplerDesc::default() {
                            match media_cache::load_banner_source_rgba(texture_path) {
                                Ok(rgba) => {
                                    if let Err(e) = assets.update_texture_for_key_with_sampler(
                                        backend, key, &rgba, sampler,
                                    ) {
                                        warn!(
                                            "Failed to create custom-sampled GPU texture for image {texture_path:?}: {e}. Skipping."
                                        );
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to load song lua texture source {texture_path:?}: {e}. Skipping."
                                    );
                                }
                            }
                        } else if first_seen {
                            media_cache::ensure_banner_texture(assets, backend, texture_path);
                        }
                    }
                    deadsync_assets::song_lua::SongLuaOverlayKind::ActorMultiVertex {
                        texture_path: Some(texture_path),
                        texture_key: Some(texture_key),
                        ..
                    } => {
                        let key = texture_key.as_ref();
                        let first_seen = seen.insert(key.to_owned());
                        let sampler = deadsync_assets::song_lua::overlay_sampler(overlay);
                        if sampler != SamplerDesc::default() {
                            match media_cache::load_banner_source_rgba(texture_path) {
                                Ok(rgba) => {
                                    if let Err(e) = assets.update_texture_for_key_with_sampler(
                                        backend, key, &rgba, sampler,
                                    ) {
                                        warn!(
                                            "Failed to create custom-sampled GPU texture for image {texture_path:?}: {e}. Skipping."
                                        );
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to load song lua texture source {texture_path:?}: {e}. Skipping."
                                    );
                                }
                            }
                        } else if first_seen {
                            media_cache::ensure_banner_texture(assets, backend, texture_path);
                        }
                    }
                    deadsync_assets::song_lua::SongLuaOverlayKind::Model { layers } => {
                        for layer in layers.iter() {
                            prewarm_model_texture_key(
                                assets,
                                backend,
                                &mut seen,
                                &mut seen_model_textures,
                                layer.texture_key.as_ref(),
                            );
                        }
                    }
                    deadsync_assets::song_lua::SongLuaOverlayKind::NoteskinActor { slots } => {
                        for slot in slots.iter() {
                            if slot.model.is_some() {
                                prewarm_model_texture_key(
                                    assets,
                                    backend,
                                    &mut seen,
                                    &mut seen_model_textures,
                                    slot.texture_key(),
                                );
                            } else if seen.insert(slot.texture_key().to_owned()) {
                                assets.ensure_texture_for_key(backend, slot.texture_key());
                            }
                        }
                    }
                    _ => {}
                }
            }
        };
    let song_lua_visuals = state.song_lua_visuals();
    prewarm_song_lua_overlays(&song_lua_visuals.overlays);
    for layer in &song_lua_visuals.background_visual_layers {
        prewarm_song_lua_overlays(&layer.overlays);
    }
    for layer in &song_lua_visuals.foreground_visual_layers {
        prewarm_song_lua_overlays(&layer.overlays);
    }
}

fn prewarm_gameplay_sfx(state: &gameplay::State) {
    deadsync_audio_stream::preload_sfx("assets/sounds/boom.ogg");
    deadsync_audio_stream::preload_sfx("assets/sounds/assist_tick.ogg");

    let mut sound_paths = Vec::<PathBuf>::with_capacity(state.song_lua_sound_paths.len());
    let mut seen = HashSet::<String>::with_capacity(state.song_lua_sound_paths.len());
    let mut prewarm_sound_overlays =
        |overlays: &[deadsync_assets::song_lua::SongLuaOverlayActor]| {
            deadsync_song_lua::push_song_lua_overlay_sound_paths(
                overlays,
                &mut seen,
                &mut sound_paths,
            );
        };

    let song_lua_visuals = state.song_lua_visuals();
    prewarm_sound_overlays(&song_lua_visuals.overlays);
    for layer in &song_lua_visuals.background_visual_layers {
        prewarm_sound_overlays(&layer.overlays);
    }
    for layer in &song_lua_visuals.foreground_visual_layers {
        prewarm_sound_overlays(&layer.overlays);
    }
    deadsync_song_lua::push_unique_song_lua_sound_paths(
        &state.song_lua_sound_paths,
        &mut seen,
        &mut sound_paths,
    );
    for sound_path in sound_paths {
        let key = sound_path.to_string_lossy();
        deadsync_audio_stream::preload_sfx(key.as_ref());
    }
}

fn prewarm_gameplay_text_layout_cache(
    assets: &AssetManager,
    metrics: &Metrics,
    cache: &mut compose::TextLayoutCache,
    state: &mut gameplay::State,
) {
    let started = Instant::now();
    // Gameplay prewarm owns the whole cache for the next song, so start from an
    // empty working set instead of scan-pruning stale entries from older screens.
    cache.clear();
    cache.configure(GAMEPLAY_TEXT_LAYOUT_CACHE_LIMIT);
    cache.begin_frame_stats();

    let fonts = assets.fonts();
    crate::screens::components::gameplay::gameplay_stats::refresh_density_graph_meshes(state);
    let mut actors = Vec::with_capacity(256);
    gameplay::push_actors(
        &mut actors,
        state,
        assets,
        gameplay::ActorViewOverride::default(),
    );
    let _ = compose::build_screen_cached_with_texture_context(
        &actors,
        [0.0, 0.0, 0.0, 1.0],
        metrics,
        fonts,
        0.0,
        cache,
        &PRESENT_TEXTURE_CONTEXT,
    );
    gameplay::prewarm_text_layout(cache, fonts, state);
    crate::screens::components::gameplay::gameplay_stats::prewarm_text_layout(
        cache, fonts, assets, state,
    );
    crate::screens::components::gameplay::notefield::prewarm_text_layout(cache, fonts, state);
    // Freeze the gameplay cache after prewarm so live-song misses saturate instead
    // of triggering prune work on a frame.
    cache.lock_growth();

    let stats = cache.frame_stats();
    debug!(
        "Gameplay text cache prewarm: entries={} shared={} elapsed_ms={:.3}",
        stats.owned_entries,
        stats.shared_aliases,
        started.elapsed().as_secs_f64() * 1000.0,
    );
}

fn gameplay_media_keys(state: &gameplay::State) -> Vec<String> {
    deadsync_assets::dynamic_media::gameplay_media_keys(state.song(), &state.background_changes)
}

#[inline(always)]
const fn evaluation_summary_return_to(
    prev: CurrentScreen,
    pending_post_select_summary_exit: bool,
) -> CurrentScreen {
    if pending_post_select_summary_exit {
        return CurrentScreen::Initials;
    }
    match prev {
        CurrentScreen::SelectMusic => CurrentScreen::SelectMusic,
        CurrentScreen::SelectCourse => CurrentScreen::SelectCourse,
        _ => CurrentScreen::Initials,
    }
}

fn stage_summary_from_eval(eval: &evaluation::State) -> Option<stage_stats::StageSummary> {
    let play_style = profile::get_session_play_style();
    let player_side = profile::get_session_player_side();

    let mut song_opt: Option<Arc<deadsync_chart::SongData>> = None;
    let mut music_rate: f32 = 1.0;
    let mut players: [Option<stage_stats::PlayerStageSummary>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);

    let notes_hit = |si: &evaluation::ScoreInfo| -> u32 {
        let mut total: u32 = 0;
        for c in &si.column_judgments {
            total = total
                .saturating_add(c.w0)
                .saturating_add(c.w1)
                .saturating_add(c.w2)
                .saturating_add(c.w3)
                .saturating_add(c.w4)
                .saturating_add(c.w5);
        }
        total
    };

    let to_player = |si: &evaluation::ScoreInfo| stage_stats::PlayerStageSummary {
        profile_name: si.profile_name.clone(),
        chart: si.chart.clone(),
        score_valid: si.score_valid,
        disqualified: si.disqualified,
        groovestats: si.groovestats.clone(),
        itl: si.itl.clone(),
        grade: si.grade,
        score_percent: si.score_percent,
        earned_grade_points: si.earned_grade_points,
        possible_grade_points: si.possible_grade_points,
        ex_score_percent: si.ex_score_percent,
        hard_ex_score_percent: si.hard_ex_score_percent,
        hands_achieved: si.hands_achieved,
        hands_total: si.hands_total,
        holds_held: si.holds_held,
        holds_held_for_score: si.holds_held_for_score,
        holds_total: si.holds_total,
        rolls_held: si.rolls_held,
        rolls_held_for_score: si.rolls_held_for_score,
        rolls_total: si.rolls_total,
        mines_hit_for_score: si.mines_hit_for_score,
        mines_avoided: si.mines_avoided,
        mines_total: si.mines_total,
        notes_hit: notes_hit(si),
        calories_burned: si.calories_burned,
        window_counts: si.window_counts,
        window_counts_10ms: si.window_counts_10ms,
        timing: si.timing,
        arrow_timing: si.arrow_timing.clone(),
        scatter: si.scatter.clone(),
        scatter_worst_window_ms: si.scatter_worst_window_ms,
        histogram: si.histogram.clone(),
        graph_first_second: si.graph_first_second,
        graph_last_second: si.graph_last_second,
        life_history: si.life_history.clone(),
        fail_time: si.fail_time,
        show_w0: (si.show_fa_plus_window && si.show_fa_plus_pane) || si.show_ex_score,
        show_fa_plus_pane: si.show_fa_plus_pane,
        show_ex_score: si.show_ex_score,
        show_hard_ex_score: si.show_hard_ex_score,
        track_early_judgments: si.track_early_judgments,
    };

    match play_style {
        profile_data::PlayStyle::Versus => {
            for (idx, side) in [
                (0, profile_data::PlayerSide::P1),
                (1, profile_data::PlayerSide::P2),
            ] {
                let Some(si) = eval.score_info.get(idx).and_then(|s| s.as_ref()) else {
                    continue;
                };
                song_opt = Some(si.song.clone());
                music_rate = si.music_rate;
                players[profile_data::player_side_index(side)] = Some(to_player(si));
            }
        }
        profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
            let si = eval.score_info.first().and_then(|s| s.as_ref())?;
            song_opt = Some(si.song.clone());
            music_rate = si.music_rate;
            players[profile_data::player_side_index(player_side)] = Some(to_player(si));
        }
    }

    let song = song_opt?;
    Some(stage_stats::StageSummary {
        song,
        music_rate: if music_rate.is_finite() && music_rate > 0.0 {
            music_rate
        } else {
            1.0
        },
        duration_seconds: eval.stage_duration_seconds,
        players,
    })
}

fn restart_payload_from_eval(
    score_info: &[Option<evaluation::ScoreInfo>; MAX_PLAYERS],
) -> Option<(
    Arc<deadsync_chart::SongData>,
    [String; MAX_PLAYERS],
    f32,
    [ScrollSpeedSetting; MAX_PLAYERS],
)> {
    let mut song = None;
    let mut chart_hashes = std::array::from_fn(|_| String::new());
    let mut scroll_speed = [ScrollSpeedSetting::default(); MAX_PLAYERS];
    let mut music_rate = None;

    for entry in score_info.iter().flatten() {
        song.get_or_insert_with(|| entry.song.clone());
        let side = profile_data::player_side_index(entry.side);
        chart_hashes[side] = entry.chart.short_hash.clone();
        scroll_speed[side] = entry.speed_mod;
        if music_rate.is_none() && entry.music_rate.is_finite() && entry.music_rate > 0.0 {
            music_rate = Some(entry.music_rate);
        }
    }

    song.map(|song| (song, chart_hashes, music_rate.unwrap_or(1.0), scroll_speed))
}

impl ScreensState {
    fn new(color_index: i32, preferred_difficulty_index: usize) -> Self {
        let mut menu_state = menu::init();
        menu_state.active_color_index = color_index;

        let mut select_profile_state = select_profile::init();
        select_profile_state.active_color_index = color_index;

        let mut select_color_state = select_color::init();
        select_color_state.active_color_index = color_index;
        select_color::snap_scroll_to_active(&mut select_color_state);
        select_color_state.bg_from_index = color_index;
        select_color_state.bg_to_index = color_index;

        let mut arrowcloud_login_state = crate::screens::arrowcloud_login::init();
        arrowcloud_login_state.active_color_index = color_index;

        let mut groovestats_login_state = crate::screens::groovestats_login::init();
        groovestats_login_state.active_color_index = color_index;

        let mut select_music_state = select_music::init_placeholder();
        select_music_state.active_color_index = color_index;
        select_music_state.preferred_difficulty_index = preferred_difficulty_index;
        select_music_state.selected_steps_index = preferred_difficulty_index;

        let mut select_course_state = select_course::init();
        select_course_state.active_color_index = color_index;

        let mut select_style_state = select_style::init();
        select_style_state.active_color_index = color_index;

        let mut select_play_mode_state = select_mode::init();
        select_play_mode_state.active_color_index = color_index;

        let mut profile_load_state = profile_load::init();
        profile_load_state.active_color_index = color_index;

        let mut options_state = options::init();
        options_state.active_color_index = color_index;

        let mut credits_state = credits::init();
        credits_state.active_color_index = color_index;

        let mut manage_local_profiles_state = manage_local_profiles::init();
        manage_local_profiles_state.active_color_index = color_index;

        let mut mappings_state = mappings::init();
        mappings_state.active_color_index = color_index;

        let mut input_state = input_screen::init();
        input_state.active_color_index = color_index;

        let mut test_lights_state = test_lights::init();
        test_lights_state.active_color_index = color_index;

        let mut overscan_adjustment_state = overscan_adjustment::init();
        overscan_adjustment_state.active_color_index = color_index;

        let mut smx_assign_state = crate::screens::smx_assign::init();
        smx_assign_state.active_color_index = color_index;

        let mut init_state = init::init();
        init_state.active_color_index = color_index;

        let mut evaluation_state = evaluation::init(None);
        evaluation_state.active_color_index = color_index;

        let mut evaluation_summary_state = evaluation_summary::init();
        evaluation_summary_state.active_color_index = color_index;

        let mut initials_state = initials::init();
        initials_state.active_color_index = color_index;

        let mut gameover_state = gameover::init_blank();
        gameover_state.active_color_index = color_index;

        Self {
            current_screen: CurrentScreen::Init,
            menu_state,
            gameplay_state: None,
            practice_state: None,
            options_state,
            credits_state,
            manage_local_profiles_state,
            mappings_state,
            input_state,
            pad_config_state: {
                let mut s = crate::screens::pad_config::init();
                s.active_color_index = color_index;
                s
            },
            test_lights_state,
            overscan_adjustment_state,
            smx_assign_state,
            smx_autoprompt_latched: false,
            smx_options_light_preview: deadsync_smx::OptionsLightPreview::default(),
            smx_po_light_preview: deadsync_smx::PlayerOptionsLightPreview::default(),
            player_options_state: None,
            init_state,
            select_profile_state,
            select_color_state,
            arrowcloud_login_state,
            groovestats_login_state,
            select_style_state,
            select_play_mode_state,
            profile_load_state,
            select_music_state,
            select_course_state,
            sandbox_state: sandbox::init(),
            evaluation_state,
            evaluation_summary_state,
            initials_state,
            gameover_state,
        }
    }

    fn step_idle(
        &mut self,
        delta_time: f32,
        now: Instant,
        session: &SessionState,
        asset_manager: &AssetManager,
    ) -> (Option<ScreenAction>, bool) {
        match self.current_screen {
            CurrentScreen::Gameplay => self
                .gameplay_state
                .as_mut()
                .map(|gs| gameplay::update(gs, delta_time))
                .map_or((None, false), |action| (Some(action), false)),
            CurrentScreen::Practice => self
                .practice_state
                .as_mut()
                .map(|ps| practice::update(ps, delta_time))
                .map_or((None, false), |action| (Some(action), false)),
            CurrentScreen::Init => (Some(init::update(&mut self.init_state, delta_time)), false),
            CurrentScreen::Options => (
                options::update(&mut self.options_state, delta_time, asset_manager),
                false,
            ),
            CurrentScreen::Credits => {
                credits::update(&mut self.credits_state, delta_time);
                (None, false)
            }
            CurrentScreen::ManageLocalProfiles => (
                manage_local_profiles::update(&mut self.manage_local_profiles_state, delta_time),
                false,
            ),
            CurrentScreen::Mappings => {
                mappings::update(&mut self.mappings_state, delta_time);
                (None, false)
            }
            CurrentScreen::Input => (
                input_screen::update(&mut self.input_state, delta_time),
                false,
            ),
            CurrentScreen::ConfigurePads => (
                crate::screens::pad_config::update(&mut self.pad_config_state, delta_time),
                false,
            ),
            CurrentScreen::TestLights => (
                test_lights::update(&mut self.test_lights_state, delta_time),
                false,
            ),
            CurrentScreen::OverscanAdjustment => (
                overscan_adjustment::update(&mut self.overscan_adjustment_state, delta_time),
                false,
            ),
            CurrentScreen::SmxAssignPads => (
                crate::screens::smx_assign::update(&mut self.smx_assign_state, delta_time),
                false,
            ),
            CurrentScreen::PlayerOptions => (
                self.player_options_state
                    .as_mut()
                    .and_then(|pos| player_options::update(pos, delta_time, asset_manager)),
                false,
            ),
            CurrentScreen::Sandbox => {
                sandbox::update(&mut self.sandbox_state, delta_time);
                (None, false)
            }
            CurrentScreen::SelectProfile => {
                select_profile::update(&mut self.select_profile_state, delta_time);
                (None, false)
            }
            CurrentScreen::SelectColor => {
                select_color::update(&mut self.select_color_state, delta_time);
                (None, false)
            }
            CurrentScreen::ArrowCloudLogin => {
                crate::screens::arrowcloud_login::update(
                    &mut self.arrowcloud_login_state,
                    delta_time,
                );
                (None, false)
            }
            CurrentScreen::GrooveStatsLogin => {
                crate::screens::groovestats_login::update(
                    &mut self.groovestats_login_state,
                    delta_time,
                );
                (None, false)
            }
            CurrentScreen::SelectStyle => (
                select_style::update(&mut self.select_style_state, delta_time),
                false,
            ),
            CurrentScreen::SelectPlayMode => (
                select_mode::update(&mut self.select_play_mode_state, delta_time),
                false,
            ),
            CurrentScreen::ProfileLoad => {
                let action = profile_load::update(&mut self.profile_load_state, delta_time);
                if matches!(
                    action,
                    Some(ScreenAction::Navigate(CurrentScreen::SelectMusic))
                ) && let Some(sm) =
                    profile_load::take_prepared_select_music(&mut self.profile_load_state)
                {
                    self.select_music_state = sm;
                    self.select_music_state.active_color_index =
                        self.profile_load_state.active_color_index;

                    let preferred = session.preferred_difficulty_index;
                    self.select_music_state.selected_steps_index = preferred;
                    self.select_music_state.preferred_difficulty_index = preferred;

                    let p2_pref = profile::preferred_difficulty_for_side(
                        profile_data::PlayerSide::P2,
                        profile::get_session_play_style(),
                    );
                    self.select_music_state.p2_selected_steps_index = p2_pref;
                    self.select_music_state.p2_preferred_difficulty_index = p2_pref;

                    // Treat the initial selection as already "settled" so preview/graphs can start
                    // immediately after the transition, matching ITG/Simply Love behavior.
                    select_music::trigger_immediate_refresh(&mut self.select_music_state);
                } else if matches!(
                    action,
                    Some(ScreenAction::Navigate(CurrentScreen::SelectCourse))
                ) && let Some(sc) =
                    profile_load::take_prepared_select_course(&mut self.profile_load_state)
                {
                    self.select_course_state = sc;
                    self.select_course_state.active_color_index =
                        self.profile_load_state.active_color_index;
                    select_course::trigger_immediate_refresh(&mut self.select_course_state);
                }
                (action, false)
            }
            CurrentScreen::Evaluation => {
                if let Some(start) = session.session_start_time {
                    self.evaluation_state.session_elapsed = now.duration_since(start).as_secs_f32();
                }
                self.evaluation_state.gameplay_elapsed =
                    stage_stats::total_stage_duration_seconds(&session.played_stages);
                evaluation::update(&mut self.evaluation_state, delta_time);
                let action = if let Some(delay) = self.evaluation_state.auto_advance_seconds
                    && self.evaluation_state.screen_elapsed >= delay
                    && self.player_options_state.is_some()
                {
                    Some(ScreenAction::Navigate(CurrentScreen::Gameplay))
                } else {
                    None
                };
                (action, false)
            }
            CurrentScreen::EvaluationSummary => {
                evaluation_summary::update(&mut self.evaluation_summary_state, delta_time);
                (None, false)
            }
            CurrentScreen::Initials => (
                initials::update(&mut self.initials_state, delta_time),
                false,
            ),
            CurrentScreen::GameOver => (
                gameover::update(&mut self.gameover_state, delta_time),
                false,
            ),
            CurrentScreen::SelectMusic => {
                if let Some(start) = session.session_start_time {
                    self.select_music_state.session_elapsed =
                        now.duration_since(start).as_secs_f32();
                }
                self.select_music_state.gameplay_elapsed =
                    stage_stats::total_stage_duration_seconds(&session.played_stages);
                (
                    Some(select_music::update(
                        &mut self.select_music_state,
                        delta_time,
                    )),
                    false,
                )
            }
            CurrentScreen::SelectCourse => {
                if let Some(start) = session.session_start_time {
                    self.select_course_state.session_elapsed =
                        now.duration_since(start).as_secs_f32();
                }
                (
                    Some(select_course::update(
                        &mut self.select_course_state,
                        delta_time,
                    )),
                    false,
                )
            }
            CurrentScreen::Menu => (None, false),
        }
    }
}

impl AppState {
    fn new(
        cfg: config::Config,
        profile_data: profile_data::Profile,
        overlay_mode: u8,
        color_index: i32,
    ) -> Self {
        let play_style = profile::get_session_play_style();
        let preferred = deadsync_profile::preferred_difficulty_index(&profile_data, play_style);

        let shell = ShellState::new(&cfg, overlay_mode);
        let session = SessionState::new(preferred, profile::combo_carry());
        let screens = ScreensState::new(color_index, preferred);

        Self {
            shell,
            screens,
            session,
            gameplay_offset_save_prompt: None,
        }
    }
}

pub struct App {
    window: Option<Arc<Window>>,
    backend: Option<renderer_backend::Backend>,
    backend_type: BackendType,
    _idle_inhibitor: deadlib_platform::idle_inhibit::IdleInhibitor,
    fsr_monitor: fsr_input::Monitor,
    /// Whether the Configure Pads screen currently has FSR live-reads enabled,
    /// so `set_active` only toggles on screen enter/leave (not every frame).
    fsr_pads_active: bool,
    /// App-owned source of truth for SMX managed-config resolution + the active
    /// marker (mirrored to the Song Select screen each frame). See
    /// [`pad_config_sync`].
    pad_config_sync: pad_config_sync::PadConfigSync,
    lights: lights::Manager,
    gameplay_lights: lights::gameplay::GameplayLightTracker,
    smx_panels: smx_panel_fx::SmxPanelDriver,
    /// Last per-slot pad-light brightness pushed to the SMX crate (`[P1, P2]`),
    /// cached so the resolve-and-push only fires when the value actually changes.
    smx_light_brightness: [u8; 2],
    /// Preloaded SMX pad GIF animations, decoded once on first use (the pad-gifs
    /// option toggling on). `None` until then; never loaded on the gameplay path.
    smx_gifs: Option<std::sync::Arc<deadsync_smx::gifs::GifRegistry>>,
    /// Background state last pushed to `smx_panels` (see `SmxBgKey`), so the
    /// per-frame sync only does lookups when the toggle, screen role, pack, or
    /// current song change. Reset on a song rescan, so a recycled `Arc`
    /// pointer can't be mistaken for the same song.
    smx_bg_synced: Option<SmxBgKey>,
    /// Per-slot blackout state last sent to the SMX lights worker; `[P1_slot, P2_slot]`.
    smx_blackout_synced: [bool; 2],
    /// Decoded per-song / per-pack pad background variants, keyed by their
    /// `smx-pad-lights/` folder and role. An empty vec is the negative entry
    /// (folder scanned, no matching gif) so a folder is touched only once; the
    /// per-song BPM variant is selected from the list per resolution. Cleared
    /// when the song cache is rescanned. Only grows with folders visited.
    smx_scoped_bg_cache: std::collections::HashMap<
        (PathBuf, &'static str),
        Vec<deadsync_smx::gifs::BackgroundVariant>,
    >,
    /// Song-cache generation the scoped cache was built against; a change means
    /// a rescan, so the scoped cache is dropped (files may have changed).
    smx_scoped_bg_generation: u64,
    /// Cache of `MatchColorToDifficulty`-tinted backgrounds, keyed by (pack,
    /// role, difficulty). Holds the theme color index the tint was computed
    /// against alongside the `Arc`, so a stale entry (the player changed their
    /// theme color) is detected and recomputed on next use rather than pruned
    /// eagerly; at most one entry lives per (pack, role, difficulty) at a time.
    smx_difficulty_tint_cache: std::collections::HashMap<
        (String, &'static str, &'static str),
        (i32, std::sync::Arc<deadsync_smx::gifs::FullPadAnim>),
    >,
    asset_manager: AssetManager,
    dynamic_media: DynamicMedia,
    ui_text_layout_cache: compose::TextLayoutCache,
    gameplay_text_layout_cache: compose::TextLayoutCache,
    ui_compose_scratch: compose::ComposeScratch,
    gameplay_compose_scratch: compose::ComposeScratch,
    actor_scratch: Vec<Actor>,
    state: AppState,
    software_renderer_threads: u8,
    gfx_debug_enabled: bool,
}

impl App {
    #[inline(always)]
    fn effective_frame_interval(&self) -> Option<Duration> {
        self.state
            .shell
            .background_frame_interval(self.state.screens.current_screen)
    }

    #[inline(always)]
    fn redraw_interval_state(&self, _window: &Window) -> FrameIntervalState {
        self.state
            .shell
            .frame_interval_state(self.state.screens.current_screen)
    }

    #[inline(always)]
    fn apply_present_back_pressure(&self) -> bool {
        if self.state.shell.vsync_enabled {
            return false;
        }
        #[cfg(target_os = "macos")]
        if self.backend_type == BackendType::OpenGL {
            return true;
        }
        self.state.shell.present_mode_policy == PresentModePolicy::Mailbox
    }

    #[inline(always)]
    fn accepts_live_input(&self) -> bool {
        config::foreground_input_active(
            self.state.shell.window_focused,
            self.state.shell.surface_active,
        )
    }

    /// Apply a window focus change to all subsystems that care about it.
    ///
    /// Used by both the `WindowEvent::Focused` handler and the initial focus
    /// seed performed in `init_graphics` (and on renderer-switch window
    /// recreation). Always pushes the new focus state to the raw input
    /// backends so their gating flag stays in sync with the shell, and only
    /// runs the change-only side effects (capture sync, modifier reset,
    /// debounce/queue clear, redraw) when the shell focus actually toggled.
    pub(super) fn apply_window_focus_change(
        &mut self,
        focused: bool,
        now: Instant,
        window: Option<&Arc<Window>>,
    ) {
        deadsync_input_native::set_raw_keyboard_window_focused(focused);
        if !self.state.shell.set_window_focus(focused, now) {
            return;
        }
        self.sync_gameplay_input_capture();
        debug!(
            "Window focus changed: focused={} screen={:?}",
            focused, self.state.screens.current_screen
        );
        if !focused {
            self.state.shell.shift_held = false;
            self.state.shell.ctrl_held = false;
            self.state.shell.alt_held = false;
            self.state.shell.fast_forward_held = false;
            self.state.shell.slow_down_held = false;
            logical_input::clear_debounce_state();
            self.lights.clear_button_pressed();
            self.clear_gameplay_input_events();
        } else if let Some(w) = window {
            self.request_redraw(w, "focus");
        }
    }

    /// Drive the Configure Pads screen: enable live FSR reads while it's open,
    /// apply queued threshold edits, and refresh the pad snapshot.
    #[inline(always)]
    fn sync_pad_config_fsr(&mut self) {
        use crate::screens::pad_config;
        let screen = self.state.screens.current_screen;
        // Fast path: the FSR monitor is only driven on the Configure Pads screen and the
        // Song Select pad-config overlay. Everywhere else (gameplay included) there is
        // nothing to do unless a previously-active monitor still needs releasing, so skip
        // the config read and all the per-screen work entirely.
        if !matches!(
            screen,
            CurrentScreen::ConfigurePads | CurrentScreen::SelectMusic
        ) && !self.fsr_pads_active
        {
            return;
        }
        let cfg = config::get();
        let use_fsrs = cfg.use_fsrs;
        let on_screen = screen == CurrentScreen::ConfigurePads && use_fsrs;
        let on_overlay = screen == CurrentScreen::SelectMusic
            && self
                .state
                .screens
                .select_music_state
                .pad_config_overlay_visible
            && use_fsrs;

        // `target` is the screen state we're driving: the standalone Configure
        // Pads screen, or the Song Select overlay. A macro (not a method) so each
        // use re-borrows inline — the borrow has to release between phases so the
        // disjoint `self.pad_config_sync` access in between is allowed.
        macro_rules! target {
            () => {
                if on_screen {
                    &mut self.state.screens.pad_config_state
                } else {
                    &mut self.state.screens.select_music_state.pad_config_overlay
                }
            };
        }

        if on_screen || on_overlay {
            if !self.fsr_pads_active {
                self.fsr_monitor.set_active(true);
                self.fsr_pads_active = true;
            }
            let pads = self.fsr_monitor.poll_pads();
            // Drain queued edits in a short-lived borrow so we can touch
            // `smx_applied` (a sibling of `target`) below without a borrow clash.
            let commands = {
                let target = target!();
                pad_config::take_commands(target)
            };
            self.apply_pad_commands(commands);
            let target = target!();
            pad_config::set_pads(target, pads);
            pad_config::set_managed_active(target, cfg.smx_manages_pad_config);

            // Saving / profile management is only offered in-session, for a cursor
            // pad that is an SMX pad mapped to a joined local profile (the Options
            // screen never has a profile). Resolve it once and reuse for both the
            // save gate and the management list. Capture the cursor device (Copy)
            // so the later `smx_applied` read doesn't alias the `target` borrow.
            let cursor_dev = if on_overlay {
                pad_config::selected_device(target)
                    .filter(|dev| dev.backend == deadsync_input::fsr::BackendKind::Smx)
            } else {
                None
            };
            let cursor_profile = cursor_dev.and_then(|dev| {
                // Slot is the source of truth for player side (the SDK orders
                // slot 0 = P1, slot 1 = P2 per the pad→player assignment), so map
                // the config by slot, not the raw jumper bit.
                profile::active_local_profile_id_for_pad(dev.index == 1)
            });
            pad_config::set_save_available(target, cursor_profile.is_some());
            // Mark the config currently applied to the cursor pad's slot, read
            // straight from the authoritative controller (no screen-state alias).
            let active_name = cursor_dev.and_then(|dev| {
                self.pad_config_sync.applied[dev.index]
                    .as_ref()
                    .filter(|a| !a.preset)
                    .map(|a| a.name.clone())
            });
            // Cursor pad identity (Copy device → safe to read alongside the
            // controller borrow below). The config *list* only depends on the
            // profile + sensor type; `is_default` is per-serial, computed per entry.
            let cursor_pad_type = cursor_dev
                .and_then(|dev| deadsync_smx::pad_sensor_type(dev.index))
                .map(|t| t.as_str().to_owned());
            let cursor_serial = cursor_dev.map(|dev| deadsync_smx::get_info(dev.index).serial);
            // Cache + markers are keyed by pad slot (always unambiguous, unlike the
            // player side, which two pads can share).
            let cursor_slot = cursor_dev.map(|dev| dev.index);

            // Refresh the cached config list only when its inputs changed — no
            // per-frame `padconfig.ini` read. Management edits clear the cache via an
            // Invalidate intent (drained in `apply_smx_managed_preset`).
            if let (Some(pid), Some(pad)) = (cursor_profile.as_deref(), cursor_slot)
                && self
                    .pad_config_sync
                    .profiles_stale(pad, Some(pid), cursor_pad_type.as_deref())
            {
                let list = deadsync_profile::compat::load_pad_configs(pid)
                    .into_iter()
                    .filter(|c| {
                        pad_profile_data::config_matches(
                            c,
                            deadsync_smx::BACKEND_ID,
                            cursor_pad_type.as_deref(),
                        )
                    })
                    .collect();
                self.pad_config_sync.store_profiles(
                    pad,
                    Some(pid.to_owned()),
                    cursor_pad_type.clone(),
                    list,
                );
            }

            // Build the overlay list from the cache; active/default are derived live
            // (cheap, no I/O) since they depend on the marker / this pad's serial.
            let profiles: Vec<pad_config::ProfileListEntry> = match cursor_slot {
                Some(pad) if cursor_profile.is_some() => self
                    .pad_config_sync
                    .profiles_for(pad)
                    .iter()
                    .map(|c| pad_config::ProfileListEntry {
                        is_active: active_name.as_deref() == Some(c.name.as_str()),
                        is_default: cursor_serial
                            .as_deref()
                            .is_some_and(|s| pad_profile_data::is_default_for(c, s)),
                        name: c.name.clone(),
                    })
                    .collect(),
                _ => Vec::new(),
            };
            // Re-borrow target (released for the controller access above).
            let target = target!();
            pad_config::set_profiles(target, profiles);
        } else if self.fsr_pads_active {
            self.fsr_monitor.set_active(false);
            self.fsr_pads_active = false;
        }
    }

    /// Apply queued pad-config edits to hardware. A manual edit diverges the pad
    /// from any saved config, so each touched SMX pad drops its active marker
    /// (markers are keyed by pad slot, which is `device.index`).
    fn apply_pad_commands(&mut self, commands: Vec<crate::screens::pad_config::PadCommand>) {
        use crate::screens::pad_config::PadCommand;
        for cmd in commands {
            let device = match &cmd {
                PadCommand::Threshold { device, .. }
                | PadCommand::ThresholdPair { device, .. }
                | PadCommand::SensorEnabled { device, .. }
                | PadCommand::AutoRecalibration { device, .. }
                | PadCommand::Debounce { device, .. } => *device,
            };
            match cmd {
                PadCommand::Threshold {
                    device,
                    button,
                    sensor,
                    value,
                } => {
                    let _ = self
                        .fsr_monitor
                        .set_threshold(device, button, sensor, value);
                }
                PadCommand::ThresholdPair {
                    device,
                    button,
                    press,
                    release,
                } => {
                    let _ = self
                        .fsr_monitor
                        .set_threshold_pair(device, button, press, release);
                }
                PadCommand::SensorEnabled {
                    device,
                    button,
                    sensor,
                    enabled,
                } => {
                    let _ = self
                        .fsr_monitor
                        .set_sensor_enabled(device, button, sensor, enabled);
                }
                PadCommand::AutoRecalibration { device, enabled } => {
                    let _ = self.fsr_monitor.set_auto_recalibration(device, enabled);
                }
                PadCommand::Debounce { device, micros } => {
                    let _ = self.fsr_monitor.set_debounce_micros(device, micros);
                }
            }
            if device.backend == deadsync_input::fsr::BackendKind::Smx {
                self.pad_config_sync.mark_diverged(device.index);
            }
        }
    }

    /// Resolve and write the config for one managed SMX pad: the pad's saved
    /// default → a global default → the built-in `preset` (also the Guest /
    /// no-profile fallback). Returns the write-ack and the marker describing what
    /// was applied.
    fn resolve_pad_config(
        pad: usize,
        profile_id: Option<&str>,
        pad_type: Option<&str>,
        serial: &str,
        preset: crate::config::SmxPadPreset,
    ) -> (bool, pad_config_sync::AppliedPadConfig) {
        use pad_config_sync::AppliedPadConfig;
        let preset_label = AppliedPadConfig {
            preset: true,
            name: preset.as_str().to_owned(),
        };
        let Some(id) = profile_id else {
            return (deadsync_smx::apply_preset(pad, preset), preset_label);
        };
        let configs = deadsync_profile::compat::load_pad_configs(id);
        match pad_profile_data::resolve(&configs, deadsync_smx::BACKEND_ID, pad_type, serial)
            .and_then(|c| {
                deadsync_smx::PadConfigData::from_settings(&c.settings).map(|d| (c.name.clone(), d))
            }) {
            Some((name, data)) => (
                deadsync_smx::apply_config_data(pad, &data),
                AppliedPadConfig {
                    preset: false,
                    name,
                },
            ),
            // No matching/default config (or corrupt) → machine preset.
            None => (deadsync_smx::apply_preset(pad, preset), preset_label),
        }
    }

    /// Drain UI intents, then (when "DeadSync manages pad config" is on) resolve
    /// and apply the right pad config to each connected StepManiaX pad: this pad's
    /// per-pad default → a global default → the machine built-in preset (also the
    /// fallback for Guest / no-config players). Reactive: when the active player
    /// changes, a no-config/guest player resets the pad to the machine preset. A
    /// cheap per-pad signature avoids loading config files or rewriting the pad
    /// unless something relevant changed (so manual edits aren't clobbered).
    /// Finally mirror the markers to the screen. Off → DeadSync writes nothing.
    /// Auto-save the pad→player assignment when none is saved yet:
    /// - **Two pads, distinct jumpers:** persist the jumper-derived P1/P2 map.
    /// - **Single pad:** auto-promote to P1 regardless of jumper (the 99%
    ///   single-stage case; manual `SmxP2Serial` in `deadsync.ini` covers the
    ///   rare P2-only need).
    /// The ambiguous same-jumper-two-pad case is left for the user to assign.
    fn reconcile_smx_assignment(&mut self) {
        if matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        ) {
            return;
        }
        if !config::get().smx_input {
            return;
        }
        // Only auto-save when no assignment exists yet.
        let (p1, p2) = config::smx_pad_assignment();
        if p1.is_some() || p2.is_some() {
            return;
        }
        let a = deadsync_smx::get_info(0);
        let b = deadsync_smx::get_info(1);
        // SDK orders slot 0 = P1-jumpered, slot 1 = P2-jumpered, so the pair is
        // already in (P1, P2) order when the jumpers are distinct.
        if let Some((p1, p2)) = deadsync_smx::jumper_derived_pair(&a, &b) {
            log::info!("SMX: auto-saving pad assignment from jumpers (P1={p1}, P2={p2})");
            config::update_smx_pad_assignment(Some(p1), Some(p2));
            return;
        }
        // Single pad connected with no saved assignment: pin it to its hardware
        // jumper side (a P1-jumpered stage becomes P1, a P2-jumpered stage becomes
        // P2). This makes the assignment explicit for profile resolution without
        // overriding the jumper; the user can still flip it in the StepManiaX
        // options Pad Player picker.
        let single = match (a.connected, b.connected) {
            (true, false) if a.has_serial_number && !a.serial.is_empty() => Some(&a),
            (false, true) if b.has_serial_number && !b.serial.is_empty() => Some(&b),
            _ => None,
        };
        if let Some(pad) = single {
            log::info!(
                "SMX: single pad connected, auto-assigning to its jumper side P{} (serial={})",
                if pad.is_player2 { 2 } else { 1 },
                pad.serial
            );
            let serial = Some(pad.serial.clone());
            if pad.is_player2 {
                config::update_smx_pad_assignment(None, serial);
            } else {
                config::update_smx_pad_assignment(serial, None);
            }
        }
    }

    /// From the main Menu, if two pads share a P1/P2 jumper and no assignment
    /// resolves them, open the assignment screen automatically (once per conflict
    /// episode). Cancelling won't re-prompt until the conflict clears and returns.
    fn maybe_autoprompt_smx_assign(&mut self) {
        if self.state.screens.current_screen != CurrentScreen::Menu
            || !matches!(self.state.shell.transition, TransitionState::Idle)
        {
            return;
        }
        if !(config::get().smx_input && deadsync_smx::conflict_warning_active()) {
            // No unresolved conflict, so re-arm for the next episode.
            self.state.screens.smx_autoprompt_latched = false;
            return;
        }
        if self.state.screens.smx_autoprompt_latched {
            return;
        }
        self.state.screens.smx_autoprompt_latched = true;
        crate::screens::smx_assign::set_pending_return(CurrentScreen::Menu);
        self.handle_navigation_action(CurrentScreen::SmxAssignPads);
    }

    /// While the StepManiaX options page is open, light the pads blue (P1) / red
    /// (P2), white when ambiguous, so the user can see the assignment, and so a
    /// live Swap is reflected on the pads immediately. Also holds the underglow
    /// strips on a test colour (red with Theme Underglow on, blue with it off)
    /// so the GRB wire-order switch can be judged by eye. Restores auto-lighting
    /// and the theme underglow on leaving the page, unless the assignment screen
    /// is taking the lights over. (Driven from the app loop so the lifecycle is
    /// in one place.)
    fn drive_smx_options_lights(&mut self, dt: f32) {
        let active = self.state.screens.current_screen == CurrentScreen::Options
            && config::get().smx_input
            && options::is_smx_config_view(&self.state.screens.options_state);

        let cfg = config::get();
        let restore_underglow = self.state.screens.smx_options_light_preview.update(
            active,
            dt,
            deadsync_smx::player_indicator_colors(),
            cfg.smx_default_light_brightness,
            (cfg.smx_underglow_theme, cfg.smx_underglow_grb),
            self.state.screens.current_screen == CurrentScreen::SmxAssignPads,
        );
        // Put the strips back on the theme colour (no-op when underglow is off;
        // auto-lighting above restores the firmware default there).
        if restore_underglow {
            config::send_smx_underglow_color();
        }
    }

    /// While a side's cursor is on the Player Options "Pad Light Brightness" row,
    /// drive that side's pad with a slow rainbow scaled by the live percent, so
    /// the user previews the brightness they're picking. Restores auto-lighting
    /// once no side is previewing (or on leaving the page). Sent every frame; the
    /// SDK coalesces light writes to the pad's refresh rate.
    fn drive_smx_player_options_lights(&mut self, dt: f32) {
        let preview = (self.state.screens.current_screen == CurrentScreen::PlayerOptions
            && config::get().smx_input)
            .then(|| {
                self.state
                    .screens
                    .player_options_state
                    .as_ref()
                    .map(player_options::pad_light_brightness_preview)
            })
            .flatten()
            .filter(|p| p.iter().any(Option::is_some));

        self.state.screens.smx_po_light_preview.update(
            preview,
            dt,
            !matches!(
                self.state.screens.current_screen,
                CurrentScreen::Gameplay | CurrentScreen::Practice
            ),
        );
    }

    fn apply_smx_managed_preset(&mut self) {
        use pad_config_sync::PadConfigSignature;

        // Skip entirely on the gameplay hot path. Pad config can't change mid-song
        // (the UI that touches it isn't reachable here, so no intents queue up), and
        // rewriting pad thresholds while a chart is playing would be disruptive — a
        // mid-song hot-plug just re-resolves on the first non-gameplay frame via the
        // signature compare. The marker mirror is for the song-select UI, which is
        // hidden during gameplay, so there's nothing to refresh either.
        if matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        ) {
            return;
        }

        // Drain UI requests (manual recall/apply/save → Override; default edit /
        // overwrite / delete / style switch → Invalidate) into the controller.
        let intents = std::mem::take(&mut self.state.screens.select_music_state.pad_config_intents);
        for intent in intents {
            self.pad_config_sync.apply_intent(intent);
        }

        let cfg = config::get();
        // Only query the SMX manager when the managed-config feature is actually on.
        // With it off (or SMX input disabled) there is nothing to resolve or write, so
        // skip the per-pad `get_info` lock entirely and just clear the cached signature.
        // The marker mirror below still runs so a screen rebuild can't lose stale markers.
        let managing = cfg.smx_input && cfg.smx_manages_pad_config;
        for pad in 0..2 {
            if !managing {
                self.pad_config_sync.signature[pad] = None;
                continue;
            }
            let info = deadsync_smx::get_info(pad);
            if !info.connected {
                self.pad_config_sync.signature[pad] = None;
                continue;
            }
            // In Doubles both pads belong to the one joined player; otherwise the
            // pad maps to its own side. Side is the slot (the SDK orders slot 0 =
            // P1, slot 1 = P2 per the pad→player assignment), not the raw jumper.
            let profile_id = profile::active_local_profile_id_for_pad(pad == 1);
            let pad_type = deadsync_smx::pad_sensor_type(pad).map(|t| t.as_str().to_owned());
            // Compare against the cached signature by borrow: the steady-state
            // path allocates nothing just to find that nothing changed. The owned
            // `Sig` is built (by moving these values) only when we re-resolve.
            if self.pad_config_sync.signature_matches(
                pad,
                cfg.smx_default_pad_config,
                &info.serial,
                profile_id.as_deref(),
                pad_type.as_deref(),
            ) {
                continue; // nothing relevant changed — no file I/O, no rewrite
            }
            let (applied, label) = Self::resolve_pad_config(
                pad,
                profile_id.as_deref(),
                pad_type.as_deref(),
                &info.serial,
                cfg.smx_default_pad_config,
            );
            // One line per actual (re)resolve — fires only past the signature
            // short-circuit above (connect, profile/style switch, preset change,
            // pad type becoming known), not every frame. The primary diagnostic for
            // "why did this pad get this config" on hardware we can't test here.
            log::debug!(
                "SMX: pad {pad} resolved {} '{}' (serial={}, fw={}, type={}, profile={:?}, applied={applied})",
                if label.preset { "preset" } else { "config" },
                label.name,
                info.serial,
                info.firmware_version,
                pad_type.as_deref().unwrap_or("unknown"),
                profile_id.as_deref(),
            );
            // Record what deadsync resolved so the UI can flag the active
            // preset/config. NOT gated on the write ACK: the resolution is what we
            // intend for the pad; gating it on a momentarily-unavailable config
            // (right after connect) would leave the marker blank. The write itself
            // retries until it lands (signature only saved on success).
            self.pad_config_sync.applied[pad] = Some(label);
            if applied {
                // Move (don't clone) the resolved inputs into the cached signature.
                self.pad_config_sync.signature[pad] = Some(PadConfigSignature {
                    preset: cfg.smx_default_pad_config,
                    serial: info.serial,
                    profile_id,
                    pad_type,
                });
            }
        }

        // Mirror the authoritative markers to the screen for display. Checked every
        // frame so a screen rebuild (which resets the mirror to None) can't lose them,
        // but only cloned when they actually differ — the equality check is a couple of
        // small string compares, whereas the clone heap-allocates the config name(s)
        // every frame an SMX pad is connected. Steady state: compare, no allocation.
        if self.state.screens.select_music_state.smx_applied != self.pad_config_sync.applied {
            self.state.screens.select_music_state.smx_applied = self.pad_config_sync.snapshot();
        }
    }

    /// Resolve each pad slot's user brightness from the player on that side and push
    /// it to the SMX crate, which scales every outgoing light frame by it. Cached so
    /// the push only fires on change. Skipped on the gameplay hot path: brightness is
    /// a per-player profile value that can't change mid-song, so the value resolved on
    /// the last non-gameplay frame stays valid and the profile lock stays off the
    /// gameplay loop. With SMX input off there are no light sends, so hold at full.
    fn drive_smx_light_brightness(&mut self) {
        if matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        ) {
            return;
        }
        let resolved = if config::get().smx_input {
            [
                profile::pad_light_brightness_for_pad(false),
                profile::pad_light_brightness_for_pad(true),
            ]
        } else {
            [100, 100]
        };
        if resolved != self.smx_light_brightness {
            self.smx_light_brightness = resolved;
            deadsync_smx::set_light_brightness(resolved);
        }
    }

    fn sync_lights(&mut self, delta_time: f32, elapsed_seconds: f32) {
        let config = config::get();
        self.lights
            .set_driver(config.lights_driver, config.lights_com_port.as_str());
        self.lights
            .set_gameplay_pad_lights(config.lights_gameplay_pad_lights);
        let screen = self.state.screens.current_screen;
        if screen != CurrentScreen::TestLights {
            self.lights
                .set_mode(lights::screen_light_mode(screen_light_context(screen)));
        }
        self.lights.set_joined([
            profile::is_session_side_joined(profile_data::PlayerSide::P1),
            profile::is_session_side_joined(profile_data::PlayerSide::P2),
        ]);
        self.lights.set_hide_flags(self.current_light_hide_flags());
        // Panel lights are a sub-feature of SMX input: without smx_input there is no
        // SDK/manager to drive, so gate on both to keep the per-frame panel diff off the
        // gameplay path when StepManiaX is disabled.
        self.sync_gameplay_light_blinks(
            config.lights_simplify_bass,
            config.smx_input && config.smx_panel_lights,
        );
        let smx_gifs_enabled = config.smx_input && config.smx_panel_lights;
        // Per-player pack overrides, resolved per pad slot so that in versus
        // mode P1's pad uses P1's pack and P2's pad uses P2's pack. One profile
        // lock, no clones; skipped entirely while the feature is off.
        let (bg_packs, judge_packs) = if smx_gifs_enabled {
            profile::smx_gif_packs(config.smx_pad_gifs_pack, config.smx_judge_gifs_pack)
        } else {
            let none = [config::SmxPackName::default(); 2];
            (none, none)
        };
        self.sync_smx_pad_gifs(smx_gifs_enabled, bg_packs, judge_packs);
        self.sync_smx_pad_blackout(smx_gifs_enabled);
        if smx_gifs_enabled && self.state.screens.current_screen == CurrentScreen::SelectMusic {
            // One f32 per frame; the driver drops it unless the background is
            // actually beat-locked.
            self.smx_panels
                .set_beat(crate::screens::select_music::selection_anim_beat(
                    &self.state.screens.select_music_state,
                ));
        }
        self.lights.tick(delta_time, elapsed_seconds);
    }

    fn sync_light_input(&mut self, ev: &InputEvent) {
        let Some(source) = lights::button_source_from_action(ev.action) else {
            return;
        };
        match source {
            lights::ButtonSource::Pad(player, button) => {
                self.lights.set_button_pressed(player, button, ev.pressed);
            }
            lights::ButtonSource::Menu(player, button) => {
                self.lights
                    .set_menu_button_pressed(player, button, ev.pressed);
            }
        }
    }

    fn current_light_hide_flags(&self) -> [HideFlags; 2] {
        let screen = self.state.screens.current_screen;
        let gameplay_state = match screen {
            CurrentScreen::Gameplay => self.state.screens.gameplay_state.as_ref(),
            CurrentScreen::Practice => self
                .state
                .screens
                .practice_state
                .as_ref()
                .map(|state| &state.gameplay),
            _ => None,
        };
        gameplay_state.map_or([HideFlags::default(); 2], |state| {
            hide_flags_for_gameplay(state)
        })
    }

    fn sync_gameplay_light_blinks(&mut self, simplify_bass: bool, smx_enabled: bool) {
        match self.state.screens.current_screen {
            CurrentScreen::Gameplay => {
                if let Some(gs) = self.state.screens.gameplay_state.as_ref() {
                    self.gameplay_lights
                        .queue_blinks(&mut self.lights, gs, simplify_bass);
                    if smx_enabled {
                        self.smx_panels.update(gs);
                    } else {
                        self.smx_panels.deactivate();
                    }
                    return;
                }
            }
            CurrentScreen::Practice => {
                if let Some(ps) = self.state.screens.practice_state.as_ref() {
                    self.gameplay_lights.queue_blinks(
                        &mut self.lights,
                        &ps.gameplay,
                        simplify_bass,
                    );
                    if smx_enabled {
                        self.smx_panels.update(&ps.gameplay);
                    } else {
                        self.smx_panels.deactivate();
                    }
                    return;
                }
            }
            _ => {}
        }
        self.gameplay_lights.clear();
        self.smx_panels.deactivate();
    }

    /// Keep the SMX pad GIF state in step with the options and the current screen:
    /// the full-pad background follows the screen role (and the current song's
    /// per-song/per-pack override, if any), and the judgement animation set
    /// follows the (enabled, pack) pair. Cheap per frame: lookups only happen
    /// when the toggle, screen role, pack, or current song folder changes, and
    /// the driver deduplicates the rest.
    fn sync_smx_pad_gifs(
        &mut self,
        enabled: bool,
        bg_packs: [config::SmxPackName; 2],
        judge_packs: [config::SmxPackName; 2],
    ) {
        // The StepManiaX options page lights the pads blue/red to preview the
        // player assignment (`drive_smx_options_lights`), writing the pad
        // directly. Suppress the gif background there so the two don't fight
        // over `set_lights`; the assignment preview wins.
        let assignment_preview = self.state.screens.current_screen == CurrentScreen::Options
            && config::get().smx_input
            && options::is_smx_config_view(&self.state.screens.options_state);
        let role = if enabled && !assignment_preview {
            lights::screen_smx_background_role(screen_light_context(
                self.state.screens.current_screen,
            ))
        } else {
            None
        };

        // A song rescan may have changed per-song/per-pack files; drop the
        // scoped cache and force a re-resolve (a recycled `Arc` pointer must not
        // read as the same song).
        let generation = deadsync_simfile::runtime_cache::song_cache_generation();
        if generation != self.smx_scoped_bg_generation {
            self.smx_scoped_bg_generation = generation;
            self.smx_scoped_bg_cache.clear();
            self.smx_bg_synced = None;
        }

        // The song whose per-song/per-pack background applies here: the playing
        // song in gameplay, the highlighted song on song select. Identified by
        // `Arc` pointer so the per-frame dedup never allocates.
        let song = role.and_then(|_| self.current_smx_song());
        let song_id = song.as_ref().map(|s| std::sync::Arc::as_ptr(s) as usize);

        // On results screens, include the grade (and the difficulty of the chart
        // that earned it) in the key so a new result re-resolves to a
        // grade/difficulty-specific gif even when the role and song haven't changed.
        let eval_grade_and_difficulty = if matches!(
            self.state.screens.current_screen,
            CurrentScreen::Evaluation | CurrentScreen::EvaluationSummary | CurrentScreen::Initials
        ) {
            self.state
                .screens
                .evaluation_state
                .score_info
                .iter()
                .flatten()
                .map(|si| {
                    (
                        si.grade,
                        deadlib_present::color::difficulty_gif_tag(&si.chart.difficulty),
                    )
                })
                .min_by_key(|(g, _)| g.to_sprite_state())
        } else {
            None
        };
        let eval_grade = eval_grade_and_difficulty.map(|(g, _)| g);
        let eval_difficulty = eval_grade_and_difficulty.map(|(_, d)| d);

        let synced = Some(SmxBgKey {
            enabled,
            role,
            bg_packs,
            judge_packs,
            song_id,
            eval_grade: eval_grade.map(|g| g.to_sprite_state()),
            eval_difficulty,
        });
        if self.smx_bg_synced == synced {
            return;
        }
        let pack_changed = self.smx_bg_synced.is_none_or(|prev| {
            prev.enabled != enabled || prev.bg_packs != bg_packs || prev.judge_packs != judge_packs
        });
        self.smx_bg_synced = synced;

        let song_dir = song
            .as_ref()
            .and_then(|s| s.simfile_path.parent())
            .map(Path::to_path_buf);
        // The song's tempo selects among BPM variants of a role (denser gif at
        // low tempo, sparser at high). `max_bpm` is the conservative pick: the
        // chosen variant stays under the pad's 30fps even at the song's fastest.
        let song_bpm = song
            .as_ref()
            .map(|s| s.max_bpm as f32)
            .filter(|b| b.is_finite() && *b > 0.0);

        let on_select_music = self.state.screens.current_screen == CurrentScreen::SelectMusic;

        // Resolve and push the background for each pad slot independently so P1 and P2
        // can show different packs in versus mode. Scoped (per-song/per-pack folder) gifs
        // are pack-independent and may resolve to the same Arc for both slots; the driver
        // deduplicates by pointer so no redundant work reaches the worker.
        for pad in 0..deadsync_smx::panels::PADS {
            let pack_str = (!bg_packs[pad].is_empty()).then_some(bg_packs[pad].as_str());
            let anim = role.and_then(|role| {
                // Resolution order: the song's own background, then its pack's, then
                // the global pack (selected -> basic), then the global `default`
                // role. `_25` is the baseline both pad layouts render; 16-LED pads
                // show its outer ring. Each tier picks the BPM-best variant.
                let scoped = song_dir
                    .as_deref()
                    .and_then(|dir| self.resolve_scoped_smx_background(dir, role, song_bpm));
                scoped.or_else(|| {
                    let registry = self.smx_gif_registry();
                    let size = deadsync_smx::gifs::PadSize::Leds25;
                    let try_role =
                        |role_str: &str| registry.background(pack_str, role_str, size, song_bpm);
                    // On results screens, try grade- and difficulty-specific roles
                    // before the plain role; `results_role_candidates` documents and
                    // tests the exact order.
                    let grade_anim = if role == "results" {
                        eval_grade.and_then(|grade| {
                            deadsync_smx::panel_fx::results_role_candidates(grade, eval_difficulty)
                                .iter()
                                .find_map(|r| try_role(r))
                        })
                    } else {
                        None
                    };
                    let resolved = grade_anim
                        .or_else(|| registry.background(pack_str, role, size, song_bpm))
                        .or_else(|| registry.background(pack_str, "default", size, song_bpm));
                    // Only pack-resolved gifs get tinted; a per-song/pack scoped gif
                    // (the `scoped` branch above, handled outside this closure) is
                    // fully authored by the song and left as-is.
                    resolved.map(|anim| {
                        self.maybe_tint_smx_background(pack_str, role, eval_difficulty, anim)
                    })
                })
            });
            let background = anim.map(|anim| {
                // A beat-suffixed gif beat-locks on song select, the one screen with
                // a live beat source (the music preview); elsewhere it plays realtime
                // rather than freezing on a stale beat.
                let clock = match anim.beats_per_loop {
                    Some(beats_per_loop) if on_select_music => {
                        deadsync_smx::panels::Clock::BeatLocked { beats_per_loop }
                    }
                    _ => deadsync_smx::panels::Clock::Realtime,
                };
                (anim, clock)
            });
            self.smx_panels.set_background_for_pad(pad, background);
        }

        if pack_changed {
            for pad in 0..deadsync_smx::panels::PADS {
                let gifs = if enabled {
                    let registry = self.smx_gif_registry().clone();
                    let judge_pack_str =
                        (!judge_packs[pad].is_empty()).then_some(judge_packs[pad].as_str());
                    deadsync_smx::panel_fx::JudgementGifs::resolve(&registry, judge_pack_str)
                } else {
                    deadsync_smx::panel_fx::JudgementGifs::default()
                };
                self.smx_panels.set_judgement_gifs_for_pad(pad, gifs);
            }
        }
    }

    /// Black out the unused pad slot when a single player is in game mode.
    /// In Versus / Doubles both pads are in use; in Single play the non-session slot
    /// gets solid black so only the pad the player stands on is lit.
    fn sync_smx_pad_blackout(&mut self, enabled: bool) {
        let in_game = enabled
            && !matches!(
                self.state.screens.current_screen,
                CurrentScreen::Menu
                    | CurrentScreen::Init
                    | CurrentScreen::SmxAssignPads
                    | CurrentScreen::TestLights
                    | CurrentScreen::ManageLocalProfiles
                    | CurrentScreen::Credits
                    | CurrentScreen::OverscanAdjustment
                    | CurrentScreen::Mappings
                    | CurrentScreen::Options
                    | CurrentScreen::PlayerOptions
                    | CurrentScreen::ConfigurePads
                    | CurrentScreen::Input
            );
        let blackout: [bool; 2] = if in_game {
            let play_style = profile::get_session_play_style();
            let session_side = profile::get_session_player_side();
            if matches!(play_style, profile_data::PlayStyle::Single) {
                let used = profile_data::player_side_index(session_side);
                std::array::from_fn(|slot| slot != used)
            } else {
                [false; 2]
            }
        } else {
            [false; 2]
        };
        if blackout != self.smx_blackout_synced {
            self.smx_blackout_synced = blackout;
            for (pad, &on) in blackout.iter().enumerate() {
                self.smx_panels.set_pad_blackout(pad, on);
            }
        }
    }

    /// The song whose per-song/per-pack SMX background applies on the current
    /// screen: the playing song in gameplay/practice, the highlighted song on
    /// song select. `None` elsewhere or with no song. A cheap `Arc` clone.
    fn current_smx_song(&self) -> Option<std::sync::Arc<deadsync_chart::SongData>> {
        let screens = &self.state.screens;
        match screens.current_screen {
            CurrentScreen::Gameplay => screens.gameplay_state.as_ref().map(|gs| gs.song_arc()),
            CurrentScreen::Practice => screens
                .practice_state
                .as_ref()
                .map(|ps| ps.gameplay.song_arc()),
            CurrentScreen::SelectMusic => {
                select_music::highlighted_song(&screens.select_music_state)
            }
            _ => None,
        }
    }

    /// Resolve a per-song then per-pack background for `role` from the song
    /// folder's and pack folder's `smx-pad-lights/` subfolders, decoding once
    /// and caching the variant list (an empty list is the negative "no gif
    /// here" entry) so a folder is touched at most once per song-cache
    /// generation. Picks the BPM-best variant for `song_bpm`.
    fn resolve_scoped_smx_background(
        &mut self,
        song_dir: &Path,
        role: &'static str,
        song_bpm: Option<f32>,
    ) -> Option<std::sync::Arc<deadsync_smx::gifs::FullPadAnim>> {
        let song_scope = song_dir.join("smx-pad-lights");
        let pack_scope = song_dir.parent().map(|p| p.join("smx-pad-lights"));
        for dir in std::iter::once(song_scope).chain(pack_scope) {
            let variants = self
                .smx_scoped_bg_cache
                .entry((dir.clone(), role))
                .or_insert_with(|| {
                    deadsync_smx::gifs::load_scoped_background(
                        &dir,
                        role,
                        deadsync_smx::gifs::PadSize::Leds25,
                    )
                });
            if let Some(anim) = deadsync_smx::gifs::select_variant(variants, song_bpm) {
                return Some(anim);
            }
        }
        None
    }

    /// Decode the SMX GIF assets on first use. A cold path: runs when the pad-gifs
    /// option first resolves a background, never per frame.
    fn smx_gif_registry(&mut self) -> &std::sync::Arc<deadsync_smx::gifs::GifRegistry> {
        self.smx_gifs.get_or_insert_with(|| {
            let root = dirs::app_dirs().resolve_asset_path("assets");
            std::sync::Arc::new(deadsync_smx::gifs::GifRegistry::load(&root))
        })
    }

    /// Recolor a resolved background to the played chart's difficulty color if
    /// `pack` declares `role` under `MatchColorToDifficulty` in its
    /// `gifpack.ini`; otherwise returns `anim` unchanged. Reuses the same
    /// theme-relative color the rest of the UI uses for difficulty (see
    /// `deadlib_present::color::difficulty_rgba`), so `Challenge` always tints
    /// to the player's current theme color and easier difficulties step back
    /// around the same hue wheel. Cheap: a cache keyed by (pack, role,
    /// difficulty) holds one tinted `Arc` per combination, recomputed only
    /// when the player's theme color changes (never per frame).
    fn maybe_tint_smx_background(
        &mut self,
        pack_str: Option<&str>,
        role: &'static str,
        difficulty: Option<&'static str>,
        anim: std::sync::Arc<deadsync_smx::gifs::FullPadAnim>,
    ) -> std::sync::Arc<deadsync_smx::gifs::FullPadAnim> {
        let Some(difficulty) = difficulty else {
            return anim;
        };
        if !self
            .smx_gif_registry()
            .background_wants_difficulty_tint(pack_str, role)
        {
            return anim;
        }
        let theme_index = config::get().simply_love_color;
        let cache_key = (
            pack_str
                .unwrap_or(deadsync_smx::gifs::DEFAULT_PACK)
                .to_owned(),
            role,
            difficulty,
        );
        if let Some((cached_theme, cached_anim)) = self.smx_difficulty_tint_cache.get(&cache_key)
            && *cached_theme == theme_index
        {
            return cached_anim.clone();
        }
        let target_rgba = deadlib_present::color::difficulty_rgba(difficulty, theme_index)
            .map(|c| (c * 255.0).round() as u8);
        // The palette is sRGB for screen use; the pad LEDs are linear, so the
        // pastel difficulty colors read as off-white without this correction.
        let target_rgb =
            deadsync_smx::gifs::saturate_for_leds([target_rgba[0], target_rgba[1], target_rgba[2]]);
        let tinted = std::sync::Arc::new(deadsync_smx::gifs::tint_full_pad(&anim, target_rgb));
        self.smx_difficulty_tint_cache
            .insert(cache_key, (theme_index, tinted.clone()));
        tinted
    }

    #[inline(always)]
    fn stats_overlay_audio(
        &self,
    ) -> Option<crate::screens::components::shared::stats_overlay::AudioHealth> {
        let audio = deadsync_audio_stream::get_output_timing_snapshot();
        if !audio.has_measurement() {
            return None;
        }
        Some(
            crate::screens::components::shared::stats_overlay::AudioHealth {
                backend: audio.backend,
                requested_output_mode: audio.requested_output_mode,
                fallback_from_native: audio.fallback_from_native,
                timing_clock: audio.timing_clock,
                timing_quality: audio.timing_quality,
                sample_rate_hz: audio.sample_rate_hz,
                device_period_ns: audio.device_period_ns,
                stream_latency_ns: audio.stream_latency_ns,
                buffer_frames: audio.buffer_frames,
                padding_frames: audio.padding_frames,
                queued_frames: audio.queued_frames,
                estimated_output_delay_ns: audio.estimated_output_delay_ns,
                clock_fallback_count: audio.clock_fallback_count,
                timing_sanity_failure_count: audio.timing_sanity_failure_count,
                underrun_count: audio.underrun_count,
            },
        )
    }

    #[inline(always)]
    fn stats_overlay_timing(
        &self,
    ) -> Option<crate::screens::components::shared::stats_overlay::TimingHealth> {
        if !self.state.shell.overlay_mode.shows_timing() {
            return None;
        }
        let display_clock = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|gs| gs.display_clock_health())
            .unwrap_or_default();
        let present = self.state.shell.last_present_stats;
        let interval_ns = if present.actual_interval_ns != 0 {
            present.actual_interval_ns
        } else {
            present.refresh_ns
        };
        Some(
            crate::screens::components::shared::stats_overlay::TimingHealth {
                interval_ns,
                display_error_ms: display_clock.error_seconds * 1000.0,
                display_catching_up: display_clock.catching_up,
                present_mode: present.mode,
                display_clock: present.display_clock,
                host_clock: present.host_clock,
                in_flight_images: present.in_flight_images,
                waited_for_image: present.waited_for_image,
                applied_back_pressure: present.applied_back_pressure,
                queue_idle_waited: present.queue_idle_waited,
                suboptimal: present.suboptimal,
                submitted_present_id: present.submitted_present_id,
                completed_present_id: present.completed_present_id,
                calibration_error_ns: present.calibration_error_ns,
                host_mapped: present.host_present_ns != 0
                    && present.display_clock != renderer::ClockDomainTrace::Unknown
                    && present.host_clock != renderer::ClockDomainTrace::Unknown,
                audio: self.stats_overlay_audio(),
            },
        )
    }

    #[inline(always)]
    fn request_redraw(&mut self, window: &Window, reason: &'static str) {
        self.state
            .shell
            .note_redraw_requested(Instant::now(), reason);
        window.request_redraw();
    }

    #[inline(always)]
    fn request_redraw_if_needed(&mut self, window: &Window, reason: &'static str) {
        if !self.state.shell.redraw_pending() {
            self.request_redraw(window, reason);
        }
    }

    #[inline(always)]
    fn chain_continuous_redraw(&mut self, window: &Window) {
        if self.redraw_interval_state(window).interval.is_none()
            && !self.state.shell.should_skip_compose_and_draw()
        {
            self.request_redraw_if_needed(window, "chain");
        }
    }

    fn log_frame_loop_mode(&mut self, mode: FrameLoopMode) {
        if !self.state.shell.note_frame_loop_mode(mode) {
            return;
        }
        let screen = self.state.screens.current_screen;
        let focused = self.state.shell.window_focused;
        let occluded = self.state.shell.window_occluded;
        let surface_active = self.state.shell.surface_active;
        let max_fps = self
            .state
            .shell
            .frame_interval
            .map(|interval| (1.0 / interval.as_secs_f64()).round() as u32)
            .unwrap_or(0);
        match mode {
            FrameLoopMode::Poll => debug!(
                "Frame pacing: poll screen={screen:?} focused={focused} occluded={occluded} surface_active={surface_active} vsync={} present={} max_fps={max_fps}",
                self.state.shell.vsync_enabled, self.state.shell.present_mode_policy,
            ),
            FrameLoopMode::WaitPending => debug!(
                "Frame pacing: wait_pending screen={screen:?} focused={focused} occluded={occluded} surface_active={surface_active} vsync={} present={} max_fps={max_fps}",
                self.state.shell.vsync_enabled, self.state.shell.present_mode_policy,
            ),
            FrameLoopMode::Scheduled(reason, interval) => debug!(
                "Frame pacing: scheduled reason={} interval_ms={:.3} screen={screen:?} focused={focused} occluded={occluded} surface_active={surface_active} vsync={} present={} max_fps={max_fps}",
                reason.as_str(),
                interval.as_secs_f64() * 1000.0,
                self.state.shell.vsync_enabled,
                self.state.shell.present_mode_policy,
            ),
        }
    }

    fn run_frame(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: Arc<Window>,
        redraw_started: Instant,
        request_to_redraw_us: u32,
        redraw_request_reason: &'static str,
    ) {
        let prev_frame_end = self.state.shell.last_frame_end_time;
        let pre_redraw_gap_us = elapsed_us_between(redraw_started, prev_frame_end);
        let delta_time = redraw_started
            .duration_since(self.state.shell.last_frame_time)
            .as_secs_f32();
        self.state.shell.last_frame_time = redraw_started;
        let total_elapsed = redraw_started
            .duration_since(self.state.shell.start_time)
            .as_secs_f32();

        // Tab acceleration scales non-gameplay screen dt. Gameplay, Practice,
        // and gameplay steps under evaluation transitions stay on wall-clock
        // `delta_time`. See issue #174.
        let tab_acceleration_allowed = !matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        );
        let logic_dt = config::apply_tab_acceleration(
            delta_time,
            tab_acceleration_allowed,
            self.state.shell.fast_forward_held,
            self.state.shell.slow_down_held,
            self.state.shell.tab_acceleration_enabled,
        );
        deadlib_present::runtime::tick(logic_dt);
        crate::screens::components::shared::visual_style_bg::tick_global(logic_dt);

        self.sync_gameplay_input_capture();
        self.sync_pad_config_fsr();
        self.reconcile_smx_assignment();
        self.maybe_autoprompt_smx_assign();
        self.drive_smx_options_lights(delta_time);
        self.drive_smx_player_options_lights(delta_time);
        self.apply_smx_managed_preset();
        self.drive_smx_light_brightness();
        self.state.shell.update_gamepad_overlay(redraw_started);

        let mut upload_us: u32 = 0;
        let mut draw_us: u32 = 0;
        let mut draw_stats = renderer::DrawStats::default();
        let input_started = Instant::now();
        if let Err(e) = self.flush_due_input_events(event_loop) {
            error!("Failed to handle debounced input: {e}");
            event_loop.exit();
            return;
        }
        let input_us: u32 = elapsed_us_since(input_started);

        let mut finished_fading_out_to: Option<CurrentScreen> = None;
        let mut finished_actor_fade_to: Option<CurrentScreen> = None;
        let update_started = Instant::now();
        match &mut self.state.shell.transition {
            TransitionState::FadingOut {
                elapsed,
                duration,
                target,
            } => {
                *elapsed += logic_dt;
                if *target == CurrentScreen::Evaluation
                    && self.state.screens.current_screen == CurrentScreen::Gameplay
                    && let Some(gs) = self.state.screens.gameplay_state.as_mut()
                {
                    // Keep gameplay stepping under the evaluation fade so late
                    // judgments and HUD animations can settle before we hand the
                    // state off, while input remains blocked by the transition.
                    let _ = gameplay::update(gs, delta_time);
                }
                if *elapsed >= *duration {
                    finished_fading_out_to = Some(*target);
                }
            }
            TransitionState::ActorsFadeOut {
                elapsed,
                duration,
                target,
            } => {
                *elapsed += logic_dt;
                if *elapsed >= *duration {
                    finished_actor_fade_to = Some(*target);
                }
            }
            TransitionState::FadingIn { elapsed, duration } => {
                *elapsed += logic_dt;
                let finished = *elapsed >= *duration;

                if self.state.screens.current_screen == CurrentScreen::Gameplay
                    && let Some(gs) = self.state.screens.gameplay_state.as_mut()
                {
                    let _ = gameplay::update(gs, delta_time);
                }

                if finished
                    && matches!(
                        self.state.shell.transition,
                        TransitionState::FadingIn { .. }
                    )
                {
                    self.state.shell.transition = TransitionState::Idle;
                }
            }
            TransitionState::ActorsFadeIn { elapsed } => {
                *elapsed += logic_dt;
                if *elapsed >= MENU_ACTORS_FADE_DURATION {
                    self.state.shell.transition = TransitionState::Idle;
                }
            }
            TransitionState::Idle => {
                let gameplay_prompt_active = self.state.screens.current_screen
                    == CurrentScreen::Gameplay
                    && self.state.gameplay_offset_save_prompt.is_some();
                if !gameplay_prompt_active {
                    let (action, _) = self.state.screens.step_idle(
                        logic_dt,
                        redraw_started,
                        &self.state.session,
                        &self.asset_manager,
                    );
                    if let Some(action) = action
                        && !matches!(action, ScreenAction::None)
                    {
                        let _ = self.handle_action(action, event_loop);
                    }
                }
                if self.state.screens.current_screen == CurrentScreen::Evaluation
                    && !self.state.screens.evaluation_state.auto_screenshot_taken
                    && evaluation::auto_screenshot_ready(&self.state.screens.evaluation_state)
                {
                    self.state.screens.evaluation_state.auto_screenshot_taken = true;
                    if should_auto_screenshot_eval(
                        &self.state.screens.evaluation_state,
                        config::get().auto_screenshot_eval,
                    ) {
                        self.state.shell.screenshot.request(None);
                    }
                }
            }
        }

        if let Some(target) = finished_actor_fade_to {
            self.finish_actor_fade_out(target, event_loop);
        }
        if let Some(target) = finished_fading_out_to {
            self.on_fade_complete(target, event_loop);
        }
        let update_us: u32 = elapsed_us_since(update_started);
        self.sync_lights(delta_time, total_elapsed);

        if self.window.as_ref().map(|w| w.id()) != Some(window.id()) {
            self.state.shell.last_frame_end_time = Instant::now();
            return;
        }
        if self.state.shell.should_skip_compose_and_draw() {
            self.state.shell.current_frame_vpf = 0;
            self.state.shell.last_frame_end_time = Instant::now();
            return;
        }

        self.sync_gameplay_background();
        self.sync_theme_background_video(total_elapsed);
        let actor_build_started = Instant::now();
        let (mut actors, clear_color) = self.get_current_actors();
        let actor_build_us = elapsed_us_since(actor_build_started);
        self.update_fps_stats(redraw_started);
        let screens = &self.state.screens;
        let current_screen = screens.current_screen;
        let (show_select_music_video_banners, show_select_music_banners) = {
            let cfg = config::get();
            (
                cfg.show_select_music_video_banners,
                cfg.show_select_music_banners,
            )
        };
        let post_select_banner_paths = if show_select_music_video_banners
            && matches!(
                current_screen,
                CurrentScreen::EvaluationSummary | CurrentScreen::Initials
            ) {
            self.post_select_display_stages()
                .iter()
                .filter_map(|stage| stage.song.banner_path.clone())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if let Some(backend) = &mut self.backend {
            let upload_started = Instant::now();
            let gameplay_time = match current_screen {
                CurrentScreen::Gameplay => screens.gameplay_state.as_ref().map(|state| {
                    deadsync_core::song_time::song_time_ns_to_seconds(state.current_music_time_ns())
                }),
                CurrentScreen::Practice => screens.practice_state.as_ref().map(|state| {
                    deadsync_core::song_time::song_time_ns_to_seconds(
                        state.gameplay.current_music_time_ns(),
                    )
                }),
                _ => None,
            };
            match current_screen {
                CurrentScreen::SelectMusic => {
                    let state = &screens.select_music_state;
                    let desired_path = if show_select_music_video_banners
                        && show_select_music_banners
                        && state.banner_high_quality_requested
                    {
                        match state.entries.get(state.selected_index) {
                            Some(select_music::MusicWheelEntry::Song(song)) => {
                                song.banner_path.as_deref()
                            }
                            Some(select_music::MusicWheelEntry::PackHeader {
                                banner_path, ..
                            }) => banner_path.as_deref(),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    self.dynamic_media.sync_active_banner_video(
                        &mut self.asset_manager,
                        backend,
                        desired_path,
                    );
                }
                CurrentScreen::SelectCourse => {
                    let state = &screens.select_course_state;
                    let desired_path = if show_select_music_video_banners
                        && show_select_music_banners
                        && state.banner_high_quality_requested
                    {
                        match state.entries.get(state.selected_index) {
                            Some(select_music::MusicWheelEntry::Song(song)) => {
                                song.banner_path.as_deref()
                            }
                            Some(select_music::MusicWheelEntry::PackHeader {
                                banner_path, ..
                            }) => banner_path.as_deref(),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    self.dynamic_media.sync_active_banner_video(
                        &mut self.asset_manager,
                        backend,
                        desired_path,
                    );
                }
                CurrentScreen::Evaluation => {
                    let desired_path = if show_select_music_video_banners {
                        screens
                            .evaluation_state
                            .score_info
                            .iter()
                            .flatten()
                            .find_map(|score| score.song.banner_path.as_deref())
                    } else {
                        None
                    };
                    self.dynamic_media.sync_active_banner_video(
                        &mut self.asset_manager,
                        backend,
                        desired_path,
                    );
                }
                CurrentScreen::EvaluationSummary | CurrentScreen::Initials => {
                    self.dynamic_media.sync_active_banner_videos(
                        &mut self.asset_manager,
                        backend,
                        &post_select_banner_paths,
                    );
                }
                _ => {
                    self.dynamic_media.sync_active_banner_video(
                        &mut self.asset_manager,
                        backend,
                        None,
                    );
                }
            }
            self.dynamic_media.queue_video_frames(
                &mut self.asset_manager,
                gameplay_time,
                total_elapsed,
            );
            self.asset_manager.queue_pending_generated_textures();
            self.asset_manager.drain_texture_uploads(
                backend,
                TextureUploadBudget {
                    max_uploads: LIVE_TEXTURE_UPLOAD_MAX_OPS,
                    max_bytes: LIVE_TEXTURE_UPLOAD_MAX_BYTES,
                },
            );
            upload_us = elapsed_us_since(upload_started);
        }
        let fonts = self.asset_manager.fonts();
        let build_screen_started = Instant::now();
        let (mut screen, text_layout) =
            if self.state.screens.current_screen == CurrentScreen::Gameplay {
                let text_layout_cache = &mut self.gameplay_text_layout_cache;
                let compose_scratch = &mut self.gameplay_compose_scratch;
                text_layout_cache.begin_frame_stats();
                let screen = compose::build_screen_cached_with_scratch_and_texture_context(
                    &actors,
                    clear_color,
                    &self.state.shell.metrics,
                    fonts,
                    total_elapsed,
                    text_layout_cache,
                    compose_scratch,
                    &PRESENT_TEXTURE_CONTEXT,
                );
                (screen, text_layout_cache.frame_stats())
            } else {
                let text_layout_cache = &mut self.ui_text_layout_cache;
                let compose_scratch = &mut self.ui_compose_scratch;
                text_layout_cache.begin_frame_stats();
                let screen = compose::build_screen_cached_with_scratch_and_texture_context(
                    &actors,
                    clear_color,
                    &self.state.shell.metrics,
                    fonts,
                    total_elapsed,
                    text_layout_cache,
                    compose_scratch,
                    &PRESENT_TEXTURE_CONTEXT,
                );
                (screen, text_layout_cache.frame_stats())
            };
        let build_screen_us = elapsed_us_since(build_screen_started);
        let resolve_textures_us = 0;
        let compose_us: u32 = actor_build_us
            .saturating_add(build_screen_us)
            .saturating_add(resolve_textures_us);
        let compose_breakdown: ComposeBreakdown = ComposeBreakdown {
            actor_build_us,
            build_screen_us,
            resolve_textures_us,
            render_objects: saturating_u32(screen.objects.len()),
            render_cameras: saturating_u32(screen.cameras.len()),
            text_layout,
        };

        let apply_present_back_pressure = self.apply_present_back_pressure();
        let mut capture_screenshot = false;
        if let Some(backend) = &mut self.backend {
            if self.state.shell.screenshot.pending() {
                backend.request_screenshot();
            }
            let draw_started = Instant::now();
            match backend.draw(
                &screen,
                self.asset_manager.textures(),
                apply_present_back_pressure,
            ) {
                Ok(stats) => {
                    draw_stats = stats;
                    self.state.shell.current_frame_vpf = stats.vertices;
                    self.state.shell.last_present_stats = stats.present_stats;
                    draw_us = elapsed_us_since(draw_started);
                    capture_screenshot = true;
                }
                Err(e) => {
                    error!("Failed to draw frame: {e}");
                    event_loop.exit();
                    return;
                }
            }
            if self.state.screens.current_screen == CurrentScreen::Gameplay {
                self.gameplay_compose_scratch
                    .recycle_render_list(&mut screen);
            } else {
                self.ui_compose_scratch.recycle_render_list(&mut screen);
            }
        }
        if capture_screenshot {
            self.capture_pending_screenshot(redraw_started);
        }
        let frame_finished = Instant::now();
        let frame_seconds = frame_finished.duration_since(prev_frame_end).as_secs_f32();
        self.state.shell.last_frame_end_time = frame_finished;
        self.chain_continuous_redraw(&window);
        let total_elapsed_end = frame_finished
            .duration_since(self.state.shell.start_time)
            .as_secs_f32();
        let frame_host_nanos = deadlib_platform::host_time::now_nanos();
        self.update_stutter_samples(frame_seconds, total_elapsed_end);
        self.record_frame_stats_sample(
            frame_host_nanos,
            frame_seconds,
            input_us,
            update_us,
            compose_us,
            upload_us,
            draw_us,
            draw_stats,
        );
        self.record_stutter_diag_frame(
            frame_host_nanos,
            self.state.screens.current_screen,
            frame_seconds,
            pre_redraw_gap_us,
            request_to_redraw_us,
            redraw_request_reason,
            input_us,
            update_us,
            compose_us,
            upload_us,
            draw_us,
            draw_stats,
        );
        self.trace_frame_stutter_if_needed(
            frame_seconds,
            total_elapsed_end,
            self.state.screens.current_screen,
            pre_redraw_gap_us,
            request_to_redraw_us,
            redraw_request_reason,
            input_us,
            update_us,
            compose_us,
            upload_us,
            draw_us,
            &actors,
            draw_stats,
            compose_breakdown,
        );
        self.trace_stutter_diag_dump_if_needed(
            frame_host_nanos,
            total_elapsed_end,
            self.state.screens.current_screen,
            frame_seconds,
        );
        self.trace_gameplay_frame_pacing_if_needed(
            frame_finished,
            self.state.screens.current_screen,
            frame_seconds,
            pre_redraw_gap_us,
            request_to_redraw_us,
            redraw_request_reason,
            draw_us,
            draw_stats,
        );
        actors.clear();
        self.actor_scratch = actors;
    }

    fn reset_options_state_for_entry(&mut self, from: CurrentScreen) {
        let current_color_index = self.state.screens.options_state.active_color_index;
        self.state.screens.options_state = options::init();
        self.state.screens.options_state.active_color_index = current_color_index;
        if matches!(
            from,
            CurrentScreen::Mappings | CurrentScreen::Input | CurrentScreen::ConfigurePads
        ) {
            options::open_input_submenu(&mut self.state.screens.options_state);
        } else if from == CurrentScreen::TestLights {
            options::open_lights_submenu(&mut self.state.screens.options_state);
        } else if from == CurrentScreen::SmxAssignPads {
            options::open_smx_config_submenu(&mut self.state.screens.options_state);
        }
    }

    fn new(
        backend_type: BackendType,
        overlay_mode: u8,
        color_index: i32,
        config: config::Config,
        profile_data: profile_data::Profile,
    ) -> Self {
        let software_renderer_threads = config.software_renderer_threads;
        let gfx_debug_enabled = config.gfx_debug;
        let state = AppState::new(config, profile_data, overlay_mode, color_index);
        Self {
            window: None,
            backend: None,
            backend_type,
            _idle_inhibitor: deadlib_platform::idle_inhibit::IdleInhibitor::acquire(),
            fsr_monitor: fsr_input::Monitor::new(),
            fsr_pads_active: false,
            pad_config_sync: pad_config_sync::PadConfigSync::default(),
            lights: lights::Manager::new(config.lights_driver, config.lights_com_port.as_str()),
            gameplay_lights: lights::gameplay::GameplayLightTracker::default(),
            smx_panels: smx_panel_fx::SmxPanelDriver::default(),
            smx_light_brightness: [100, 100],
            smx_gifs: None,
            smx_bg_synced: None,
            smx_blackout_synced: [false; 2],
            smx_scoped_bg_cache: std::collections::HashMap::new(),
            smx_scoped_bg_generation: 0,
            smx_difficulty_tint_cache: std::collections::HashMap::new(),
            asset_manager: AssetManager::new(),
            dynamic_media: DynamicMedia::new(),
            // Screen transitions clear the UI cache, so misses stop inserting
            // once the cache reaches its fixed footprint.
            ui_text_layout_cache: compose::TextLayoutCache::new(UI_TEXT_LAYOUT_CACHE_LIMIT),
            gameplay_text_layout_cache: compose::TextLayoutCache::new(
                GAMEPLAY_TEXT_LAYOUT_CACHE_LIMIT,
            ),
            ui_compose_scratch: compose::ComposeScratch::default(),
            gameplay_compose_scratch: compose::ComposeScratch::default(),
            actor_scratch: Vec::with_capacity(256),
            state,
            software_renderer_threads,
            gfx_debug_enabled,
        }
    }

    fn handle_action(
        &mut self,
        action: ScreenAction,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        let action = match action {
            // SL/zmod parity: a restart-triggered Cancel exit returns
            // `NavigateNoFade(SelectMusic)`. Redirect it to Gameplay so the
            // player skips the trip through SelectMusic.
            ScreenAction::NavigateNoFade(CurrentScreen::SelectMusic)
                if self.state.session.restart_pending
                    && self.state.screens.current_screen == CurrentScreen::Gameplay =>
            {
                self.state.session.restart_pending = false;
                ScreenAction::NavigateNoFade(CurrentScreen::Gameplay)
            }
            ScreenAction::Navigate(CurrentScreen::Evaluation)
                if self.should_chain_course_to_next_stage() =>
            {
                ScreenAction::Navigate(CurrentScreen::Gameplay)
            }
            ScreenAction::Navigate(CurrentScreen::SelectMusic)
                if self.state.screens.current_screen == CurrentScreen::Gameplay
                    && self.state.session.course_run.is_some() =>
            {
                ScreenAction::Navigate(CurrentScreen::SelectCourse)
            }
            ScreenAction::NavigateNoFade(CurrentScreen::SelectMusic)
                if self.state.screens.current_screen == CurrentScreen::Gameplay
                    && self.state.session.course_run.is_some() =>
            {
                ScreenAction::NavigateNoFade(CurrentScreen::SelectCourse)
            }
            other => other,
        };
        let commands = match action {
            ScreenAction::Navigate(screen) => {
                self.handle_navigation_action(screen);
                Vec::new()
            }
            ScreenAction::NavigateNoFade(screen) => {
                if self.maybe_begin_gameplay_offset_prompt(
                    self.state.screens.current_screen,
                    screen,
                    true,
                ) {
                    return Ok(());
                }
                // Skip the current screen's out-transition and immediately enter `screen`,
                // letting the target screen's in-transition handle the visual change.
                if matches!(self.state.shell.transition, TransitionState::Idle) {
                    self.on_fade_complete(screen, event_loop);
                }
                return Ok(());
            }
            ScreenAction::Exit => self.handle_exit_action(),
            ScreenAction::Shutdown => self.handle_shutdown_action(),
            ScreenAction::SelectProfiles { p1, p2 } => {
                let fast_profile_switch = profile::take_fast_profile_switch_from_select_music();
                let profile_data = profile::set_active_profiles(p1, p2);
                self.state.session.combo_carry = profile::combo_carry_for_profiles(&profile_data);
                if let Some(backend) = self.backend.as_mut() {
                    self.dynamic_media.set_profile_avatar_for_side(
                        &mut self.asset_manager,
                        backend,
                        profile_data::PlayerSide::P1,
                        profile_data[0].avatar_path.clone(),
                    );
                    self.dynamic_media.set_profile_avatar_for_side(
                        &mut self.asset_manager,
                        backend,
                        profile_data::PlayerSide::P2,
                        profile_data[1].avatar_path.clone(),
                    );
                }

                let [preferred_p1, preferred_p2] =
                    profile::preferred_difficulty_indices_for_profiles(
                        &profile_data,
                        profile::get_session_play_style(),
                    );
                let side = profile::get_session_player_side();
                let preferred_active = match side {
                    profile_data::PlayerSide::P1 => preferred_p1,
                    profile_data::PlayerSide::P2 => preferred_p2,
                };
                self.state.session.preferred_difficulty_index = preferred_active;

                let current_color_index =
                    self.state.screens.select_profile_state.active_color_index;
                if fast_profile_switch {
                    self.state.screens.select_music_state.active_color_index = current_color_index;
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index = preferred_active;
                    self.state.screens.select_music_state.selected_steps_index = preferred_active;
                    self.state
                        .screens
                        .select_music_state
                        .p2_preferred_difficulty_index = preferred_p2;
                    self.state
                        .screens
                        .select_music_state
                        .p2_selected_steps_index = preferred_p2;
                    select_music::trigger_immediate_refresh(
                        &mut self.state.screens.select_music_state,
                    );
                    if self.state.screens.current_screen != CurrentScreen::SelectMusic {
                        self.handle_navigation_action(CurrentScreen::SelectMusic);
                    }
                } else {
                    let cfg = crate::config::get();
                    let next = if crate::screens::options::qr_login::should_auto_show_groovestats(
                        cfg.groovestats_qr_login_when,
                    ) {
                        CurrentScreen::GrooveStatsLogin
                    } else if crate::screens::options::qr_login::should_auto_show(
                        cfg.arrowcloud_qr_login_when,
                    ) {
                        CurrentScreen::ArrowCloudLogin
                    } else {
                        CurrentScreen::SelectColor
                    };
                    // ProfileLoad asynchronously prepares SelectMusic/SelectCourse state;
                    // avoid redundant eager init here.
                    self.handle_navigation_action(next);
                }
                Vec::new()
            }
            ScreenAction::LinkArrowCloud {
                profile_id,
                display_name,
            } => {
                self.state.screens.arrowcloud_login_state.active_color_index =
                    self.state.screens.menu_state.active_color_index;
                self.state.screens.arrowcloud_login_state.target_profile =
                    Some(crate::screens::arrowcloud_login::ProfileTarget {
                        id: profile_id,
                        display_name,
                    });
                self.handle_navigation_action(CurrentScreen::ArrowCloudLogin);
                Vec::new()
            }
            ScreenAction::LinkGrooveStats {
                profile_id,
                display_name,
            } => {
                self.state
                    .screens
                    .groovestats_login_state
                    .active_color_index = self.state.screens.menu_state.active_color_index;
                self.state.screens.groovestats_login_state.target_profile =
                    Some(crate::screens::groovestats_login::ProfileTarget {
                        id: profile_id,
                        display_name,
                    });
                self.handle_navigation_action(CurrentScreen::GrooveStatsLogin);
                Vec::new()
            }
            ScreenAction::RequestScreenshot(side) => {
                self.state.shell.screenshot.request(side);
                Vec::new()
            }
            ScreenAction::RequestBanner(path_opt) => vec![Command::SetBanner(path_opt)],
            ScreenAction::RequestCdTitle(path_opt) => vec![Command::SetCdTitle(path_opt)],
            ScreenAction::RequestPackBanner(path_opt) => vec![Command::SetPackBanner(path_opt)],
            ScreenAction::RequestWheelItemBackgrounds(paths) => {
                vec![Command::SetWheelItemBackgrounds(paths)]
            }
            ScreenAction::RequestDensityGraph { slot, chart_opt } => {
                vec![Command::SetDensityGraph { slot, chart_opt }]
            }
            ScreenAction::ApplySongOffsetSync {
                simfile_path,
                delta_seconds,
            } => {
                if let Err(e) =
                    self.save_gameplay_song_offset(simfile_path.as_path(), delta_seconds)
                {
                    warn!("Failed to save song offset sync changes: {e}");
                }
                Vec::new()
            }
            ScreenAction::ApplySongOffsetSyncBatch { changes } => {
                if let Err(e) = self.save_song_offset_changes(&changes) {
                    warn!("Failed to save pack sync changes: {e}");
                }
                Vec::new()
            }
            ScreenAction::FetchOnlineGrade(hash) => vec![Command::FetchOnlineGrade(hash)],
            ScreenAction::WriteFsrDump => {
                let path = dirs::app_dirs().data_dir.join("fsrdump.txt");
                match self.fsr_monitor.write_debug_dump(&path) {
                    Ok(()) => {
                        info!("Wrote FSR debug dump to '{}'", path.display());
                        self.state.shell.gamepad_overlay_state =
                            Some((format!("Wrote {}", path.display()), Instant::now()));
                    }
                    Err(e) => {
                        warn!("Failed to write FSR debug dump: {e}");
                        self.state.shell.gamepad_overlay_state =
                            Some((format!("FSR dump failed: {e}"), Instant::now()));
                    }
                }
                Vec::new()
            }
            ScreenAction::ChangeGraphics {
                renderer,
                display_mode,
                resolution,
                monitor,
                vsync,
                present_mode_policy,
                max_fps,
                high_dpi,
            } => {
                // Ensure options menu reflects current hardware state before processing changes
                self.update_options_monitor_specs(event_loop);

                let mut present_config_changed = false;
                if let Some(vsync) = vsync {
                    self.state.shell.vsync_enabled = vsync;
                    debug!("Graphics setting changed: vsync={vsync}");
                    config::update_vsync(vsync);
                    options::sync_vsync(&mut self.state.screens.options_state, vsync);
                    present_config_changed = true;
                }
                if let Some(max_fps) = max_fps {
                    self.state.shell.set_max_fps(max_fps);
                    debug!("Graphics setting changed: max_fps={max_fps}");
                    config::update_max_fps(max_fps);
                    options::sync_max_fps(&mut self.state.screens.options_state, max_fps);
                }
                if let Some(policy) = present_mode_policy {
                    self.state.shell.set_present_mode_policy(policy);
                    debug!("Graphics setting changed: present_mode_policy={policy}");
                    config::update_present_mode_policy(policy);
                    options::sync_present_mode_policy(
                        &mut self.state.screens.options_state,
                        policy,
                    );
                    present_config_changed = true;
                }
                if let Some(enabled) = high_dpi {
                    debug!("Graphics setting changed: high_dpi={enabled}");
                    config::update_high_dpi(enabled);
                    options::sync_high_dpi(&mut self.state.screens.options_state, enabled);
                }

                let mut pending_resolution = None;
                if let Some((w, h)) = resolution {
                    self.state.shell.display_width = w;
                    self.state.shell.display_height = h;
                    config::update_display_resolution(w, h);
                    options::sync_display_resolution(&mut self.state.screens.options_state, w, h);
                    pending_resolution = Some((w, h));
                }
                let (_, monitor_count, chosen_monitor) = display::resolve_monitor(
                    event_loop,
                    monitor.unwrap_or(self.state.shell.display_monitor),
                );
                let target_renderer = renderer.unwrap_or(self.backend_type);
                let high_dpi_affects_renderer =
                    high_dpi.is_some() && target_renderer == BackendType::OpenGL;
                if high_dpi_affects_renderer && pending_resolution.is_none() {
                    pending_resolution = Some((
                        self.state.shell.display_width,
                        self.state.shell.display_height,
                    ));
                }
                let recreate_renderer = renderer.is_some() || high_dpi_affects_renderer;

                match (recreate_renderer, display_mode) {
                    (true, Some(mode)) => {
                        // When both change, avoid touching the old window; update state/config
                        // first so the new renderer is created directly in the target mode.
                        let prev_mode = self.state.shell.display_mode;
                        let fullscreen_type = match mode {
                            DisplayMode::Fullscreen(ft) => ft,
                            DisplayMode::Windowed => {
                                if let DisplayMode::Fullscreen(ft) = prev_mode {
                                    ft
                                } else {
                                    config::get().fullscreen_type
                                }
                            }
                        };
                        self.state.shell.display_mode = mode;
                        self.state.shell.display_monitor = chosen_monitor;
                        config::update_display_mode(mode);
                        config::update_display_monitor(chosen_monitor);
                        options::sync_display_mode(
                            &mut self.state.screens.options_state,
                            mode,
                            fullscreen_type,
                            chosen_monitor,
                            monitor_count,
                        );
                        self.switch_renderer(
                            target_renderer,
                            pending_resolution,
                            event_loop,
                            high_dpi_affects_renderer,
                        )?;
                    }
                    (false, Some(mode)) => {
                        self.apply_display_mode(mode, Some(chosen_monitor), event_loop)?;
                        if let Some((w, h)) = pending_resolution {
                            self.apply_resolution(w, h, event_loop)?;
                        }
                    }
                    (true, None) => {
                        if monitor.is_some() {
                            self.state.shell.display_monitor = chosen_monitor;
                            config::update_display_monitor(chosen_monitor);
                            let fullscreen_type = match self.state.shell.display_mode {
                                DisplayMode::Fullscreen(ft) => ft,
                                DisplayMode::Windowed => config::get().fullscreen_type,
                            };
                            options::sync_display_mode(
                                &mut self.state.screens.options_state,
                                self.state.shell.display_mode,
                                fullscreen_type,
                                chosen_monitor,
                                monitor_count,
                            );
                        }
                        self.switch_renderer(
                            target_renderer,
                            pending_resolution,
                            event_loop,
                            high_dpi_affects_renderer,
                        )?;
                    }
                    (false, None) => {
                        if monitor.is_some() {
                            // Move the existing window/fullscreen session to the chosen monitor.
                            self.apply_display_mode(
                                self.state.shell.display_mode,
                                Some(chosen_monitor),
                                event_loop,
                            )?;
                        }
                        if let Some((w, h)) = pending_resolution {
                            self.apply_resolution(w, h, event_loop)?;
                        }
                    }
                }
                if present_config_changed
                    && !recreate_renderer
                    && let Some(backend) = &mut self.backend
                {
                    backend.set_present_config(
                        self.state.shell.vsync_enabled,
                        self.state.shell.present_mode_policy,
                    );
                }
                Vec::new()
            }
            ScreenAction::UpdateShowOverlay(mode) => {
                self.state.shell.set_overlay_mode(mode);
                config::update_show_stats_mode(mode);
                options::sync_show_stats_mode(&mut self.state.screens.options_state, mode);
                Vec::new()
            }
            ScreenAction::UpdateMouseCursorHidden(hidden) => {
                if let Some(window) = &self.window {
                    window.set_cursor_visible(!hidden);
                }
                config::update_hide_mouse_cursor(hidden);
                options::sync_hide_mouse_cursor(&mut self.state.screens.options_state, hidden);
                Vec::new()
            }
            ScreenAction::TestLightsSetAuto => {
                test_lights::on_enter(&mut self.state.screens.test_lights_state);
                self.lights.set_test_auto_cycle();
                Vec::new()
            }
            ScreenAction::TestLightsStepCabinet(delta) => {
                self.lights.step_test_cabinet(delta);
                Vec::new()
            }
            ScreenAction::TestLightsStepButton(delta) => {
                self.lights.step_test_button(delta);
                Vec::new()
            }
            ScreenAction::ConsumeInput => Vec::new(),
            ScreenAction::None => Vec::new(),
        };
        self.run_commands(commands, event_loop)
    }

    #[inline(always)]
    fn gameplay_global_offset_changed(gs: &gameplay::State) -> bool {
        sync_offset::sync_offset_changed(
            gs.initial_global_offset_seconds(),
            gs.global_offset_seconds(),
        )
    }

    #[inline(always)]
    fn gameplay_song_offset_changed(gs: &gameplay::State) -> bool {
        sync_offset::sync_offset_changed(gs.initial_song_offset_seconds(), gs.song_offset_seconds())
    }

    #[inline(always)]
    fn gameplay_offset_changed(gs: &gameplay::State) -> bool {
        Self::gameplay_global_offset_changed(gs) || Self::gameplay_song_offset_changed(gs)
    }

    #[inline(always)]
    fn gameplay_saveable_offset_changed(gs: &gameplay::State) -> bool {
        sync_offset::gameplay_sync_offset_saveable_changed(
            gs.initial_global_offset_seconds(),
            gs.global_offset_seconds(),
            gs.initial_song_offset_seconds(),
            gs.song_offset_seconds(),
            config::song_path_is_writable(gs.song().simfile_path.as_path()),
        )
    }

    fn gameplay_sync_prompt_text(gs: &gameplay::State) -> String {
        let song = gs.song();
        let title = song.display_full_title(false);
        sync_offset::gameplay_sync_prompt_text(sync_offset::GameplaySyncPromptText {
            song_title: title.as_str(),
            song_writable: config::song_path_is_writable(song.simfile_path.as_path()),
            initial_global_offset_seconds: gs.initial_global_offset_seconds(),
            global_offset_seconds: gs.global_offset_seconds(),
            initial_song_offset_seconds: gs.initial_song_offset_seconds(),
            song_offset_seconds: gs.song_offset_seconds(),
        })
    }

    fn save_song_offset_changes(
        &mut self,
        changes: &[sync_offset::SongOffsetSyncChange],
    ) -> Result<(), String> {
        let summary = sync_offset::save_song_offset_changes(
            changes,
            config::song_path_is_writable,
            |simfile_path| {
                let updated_song = song_loading::reload_song_in_cache(simfile_path)?;
                if let Some(po_state) = self.state.screens.player_options_state.as_mut() {
                    let _ = deadsync_simfile::runtime_cache::replace_song_arc_if_same_simfile(
                        &mut po_state.song,
                        &updated_song,
                    );
                }
                Ok(())
            },
        )?;

        if summary.saved_files == 0 {
            if let Some(path) = summary.first_skipped_path {
                return Err(format!(
                    "Song offset sync changes target read-only AdditionalSongFoldersReadOnly path '{}'",
                    path.display()
                ));
            }
            return Ok(());
        }

        select_music::refresh_from_song_cache(&mut self.state.screens.select_music_state);
        if summary.skipped_read_only > 0
            && let Some(path) = summary.first_skipped_path.as_deref()
        {
            warn!(
                "Skipped {} song offset sync change(s) under read-only AdditionalSongFoldersReadOnly roots; first skipped '{}'.",
                summary.skipped_read_only,
                path.display()
            );
        }
        if summary.saved_files == 1 {
            if let Some(path) = summary.first_saved_path.as_deref() {
                info!(
                    "Saved song offset sync changes to '{}' (updated {} #OFFSET tags; refreshed song cache).",
                    path.display(),
                    summary.changed_tags_total
                );
            }
        } else {
            info!(
                "Saved pack sync changes to {} simfiles (updated {} #OFFSET tags; refreshed song cache).",
                summary.saved_files, summary.changed_tags_total
            );
        }
        Ok(())
    }

    fn save_gameplay_song_offset(&mut self, simfile_path: &Path, delta: f32) -> Result<(), String> {
        self.save_song_offset_changes(&[sync_offset::SongOffsetSyncChange {
            simfile_path: simfile_path.to_path_buf(),
            delta_seconds: delta,
        }])
    }

    fn maybe_begin_gameplay_offset_prompt(
        &mut self,
        from: CurrentScreen,
        target: CurrentScreen,
        navigate_no_fade: bool,
    ) -> bool {
        if self.state.gameplay_offset_save_prompt.is_some() {
            return true;
        }
        if from != CurrentScreen::Gameplay {
            return false;
        }
        // ITG parity: no save-sync prompt while playing a course.
        if self.state.session.course_run.is_some() {
            return false;
        }
        let Some(gs) = self.state.screens.gameplay_state.as_ref() else {
            return false;
        };
        if !Self::gameplay_offset_changed(gs) {
            return false;
        }
        if !Self::gameplay_saveable_offset_changed(gs) {
            return false;
        }
        self.state.gameplay_offset_save_prompt = Some(GameplayOffsetSavePrompt {
            target,
            navigate_no_fade,
            active_choice: 0,
        });
        true
    }

    fn finalize_gameplay_offset_prompt(
        &mut self,
        save_changes: bool,
        event_loop: &ActiveEventLoop,
    ) {
        let Some(prompt) = self.state.gameplay_offset_save_prompt.take() else {
            return;
        };
        if save_changes {
            let mut song_offset_change: Option<(PathBuf, f32)> = None;
            if let Some(gs) = self.state.screens.gameplay_state.as_ref() {
                if let Some(global_offset) = sync_offset::sync_offset_target_seconds(
                    gs.initial_global_offset_seconds(),
                    gs.global_offset_seconds(),
                ) {
                    config::update_global_offset(global_offset);
                }
                if let Some(delta) = sync_offset::sync_offset_delta_seconds(
                    gs.initial_song_offset_seconds(),
                    gs.song_offset_seconds(),
                ) && config::song_path_is_writable(gs.song().simfile_path.as_path())
                {
                    song_offset_change = Some((gs.song().simfile_path.clone(), delta));
                }
            }
            if let Some((simfile_path, delta)) = song_offset_change
                && let Err(e) = self.save_gameplay_song_offset(simfile_path.as_path(), delta)
            {
                warn!("Failed to save song offset sync changes: {e}");
            }
        }
        if prompt.navigate_no_fade {
            if matches!(self.state.shell.transition, TransitionState::Idle) {
                self.on_fade_complete(prompt.target, event_loop);
            }
            return;
        }
        self.handle_navigation_action_after_prompt(prompt.target);
    }

    fn route_gameplay_offset_prompt_input(
        &mut self,
        event_loop: &ActiveEventLoop,
        ev: &InputEvent,
    ) -> bool {
        if self.state.screens.current_screen != CurrentScreen::Gameplay
            || self.state.gameplay_offset_save_prompt.is_none()
        {
            return false;
        }
        if !ev.pressed {
            return true;
        }
        let decision = match gameplay_offset_prompt_choice_delta(
            ev.action,
            config::get().only_dedicated_menu_buttons,
        ) {
            Some(-1) => {
                let mut moved = false;
                if let Some(prompt) = self.state.gameplay_offset_save_prompt.as_mut()
                    && prompt.active_choice > 0
                {
                    prompt.active_choice -= 1;
                    moved = true;
                }
                if moved {
                    deadsync_audio_stream::play_sfx("assets/sounds/change.ogg");
                }
                None
            }
            Some(1) => {
                let mut moved = false;
                if let Some(prompt) = self.state.gameplay_offset_save_prompt.as_mut()
                    && prompt.active_choice < 1
                {
                    prompt.active_choice += 1;
                    moved = true;
                }
                if moved {
                    deadsync_audio_stream::play_sfx("assets/sounds/change.ogg");
                }
                None
            }
            _ => match ev.action {
                VirtualAction::p1_start
                | VirtualAction::p2_start
                | VirtualAction::p1_select
                | VirtualAction::p2_select => {
                    let save_changes = self
                        .state
                        .gameplay_offset_save_prompt
                        .as_ref()
                        .is_some_and(|prompt| prompt.active_choice == 0);
                    deadsync_audio_stream::play_sfx("assets/sounds/start.ogg");
                    Some(save_changes)
                }
                VirtualAction::p1_back | VirtualAction::p2_back => None,
                _ => None,
            },
        };
        if let Some(save_changes) = decision {
            self.finalize_gameplay_offset_prompt(save_changes, event_loop);
        }
        true
    }

    fn clear_course_runtime(&mut self) {
        self.state.session.course_run = None;
        self.state.session.course_eval_pages.clear();
        self.state.session.course_eval_page_index = 0;
    }

    fn update_combo_carry_from_gameplay(&mut self, gs: &gameplay::State) {
        if gs.autoplay_used() {
            return;
        }
        let play_style = profile::get_session_play_style();
        let player_side = profile::get_session_player_side();
        match play_style {
            profile_data::PlayStyle::Versus => {
                for idx in 0..gs.num_players().min(MAX_PLAYERS) {
                    let combo = gs.players()[idx].combo;
                    self.state.session.combo_carry[idx] = combo;
                    let side = if idx == 0 {
                        profile_data::PlayerSide::P1
                    } else {
                        profile_data::PlayerSide::P2
                    };
                    profile::update_current_combo_for_side(side, combo);
                }
            }
            profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
                if gs.num_players() == 0 {
                    return;
                }
                let combo = gs.players()[0].combo;
                self.state.session.combo_carry[profile_data::player_side_index(player_side)] =
                    combo;
                profile::update_current_combo_for_side(player_side, combo);
            }
        }
    }

    fn update_last_played_course(&self, course_path: &Path, difficulty_name: &str) {
        let play_style = profile::get_session_play_style();
        match play_style {
            profile_data::PlayStyle::Versus => {
                profile::update_last_played_course_for_side(
                    profile_data::PlayerSide::P1,
                    play_style,
                    course_path,
                    Some(difficulty_name),
                );
                profile::update_last_played_course_for_side(
                    profile_data::PlayerSide::P2,
                    play_style,
                    course_path,
                    Some(difficulty_name),
                );
            }
            profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
                profile::update_last_played_course_for_side(
                    profile::get_session_player_side(),
                    play_style,
                    course_path,
                    Some(difficulty_name),
                );
            }
        }
    }

    fn start_course_run_from_selected(&mut self) -> bool {
        let Some(selection) =
            select_course::selected_course_plan(&self.state.screens.select_course_state)
        else {
            warn!("Unable to start course run: selected course has no playable stages.");
            return false;
        };
        let course_path = selection.path.clone();
        let course_difficulty_name = selection.course_difficulty_name.clone();
        let Some(course_run) = build_course_run_from_selection(selection) else {
            warn!("Unable to start course run: failed to resolve course stages.");
            return false;
        };
        self.state.session.last_course_wheel_path = Some(course_path.clone());
        self.state.session.last_course_wheel_difficulty_name = Some(course_difficulty_name.clone());
        self.update_last_played_course(course_path.as_path(), course_difficulty_name.as_str());
        self.state.session.course_run = Some(course_run);
        self.state.session.course_eval_pages.clear();
        self.state.session.course_eval_page_index = 0;
        true
    }

    fn prepare_player_options_for_course_stage(&mut self, color_index: i32) -> bool {
        let Some(course_run) = self.state.session.course_run.as_ref() else {
            return false;
        };
        let Some(stage) = course_run.stages.get(course_run.next_stage_index) else {
            return false;
        };
        self.state.screens.player_options_state = Some(player_options::init(
            stage.song.clone(),
            stage.steps_index,
            stage.preferred_difficulty_index,
            color_index,
            CurrentScreen::SelectCourse,
            Some(player_options::FixedStepchart {
                label: course_run.course_stepchart_label.clone(),
            }),
        ));
        true
    }

    fn prepare_restart_player_options(
        &mut self,
        song: Arc<deadsync_chart::SongData>,
        chart_hashes: [&str; MAX_PLAYERS],
        music_rate: f32,
        scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
        active_color_index: i32,
        return_screen: CurrentScreen,
    ) -> bool {
        let play_style = profile::get_session_play_style();
        let player_side = profile::get_session_player_side();
        let target_chart_type = play_style.chart_type();
        let fallback_steps = self.state.session.preferred_difficulty_index;

        let p1_steps = song
            .steps_index_for_chart_hash(target_chart_type, chart_hashes[0])
            .unwrap_or(fallback_steps);
        let p2_steps = song
            .steps_index_for_chart_hash(target_chart_type, chart_hashes[1])
            .unwrap_or(fallback_steps);

        let chart_steps_index = match play_style {
            profile_data::PlayStyle::Versus => [p1_steps, p2_steps],
            profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
                let idx = profile_data::player_side_index(player_side);
                let selected = [p1_steps, p2_steps][idx];
                [selected; 2]
            }
        };

        let mut po_state = player_options::init(
            song,
            chart_steps_index,
            chart_steps_index,
            active_color_index,
            return_screen,
            None,
        );
        po_state.music_rate = music_rate;
        po_state.speed_mod =
            std::array::from_fn(|i| player_options::SpeedMod::from(scroll_speed[i]));
        player_options::sync_speed_mod_type_rows(&mut po_state);
        self.state.screens.player_options_state = Some(po_state);
        true
    }

    fn prepare_player_options_for_gameplay_restart(&mut self) -> bool {
        if let Some(gs) = self.state.screens.gameplay_state.as_ref() {
            let song = gs.song_arc();
            let chart_hashes = [
                gs.charts()[0].short_hash.clone(),
                gs.charts()[1].short_hash.clone(),
            ];
            let music_rate = gs.music_rate();
            let scroll_speed = [gs.scroll_speed_for_player(0), gs.scroll_speed_for_player(1)];
            let active_color_index = gs.active_color_index();
            return self.prepare_restart_player_options(
                song,
                [chart_hashes[0].as_str(), chart_hashes[1].as_str()],
                music_rate,
                scroll_speed,
                active_color_index,
                CurrentScreen::Gameplay,
            );
        }

        if self.state.screens.current_screen != CurrentScreen::Evaluation {
            return false;
        }

        let score_info = &self.state.screens.evaluation_state.score_info;
        let Some((song, chart_hashes, music_rate, scroll_speed)) =
            restart_payload_from_eval(score_info)
        else {
            return false;
        };
        let active_color_index = self.state.screens.evaluation_state.active_color_index;
        self.prepare_restart_player_options(
            song,
            [chart_hashes[0].as_str(), chart_hashes[1].as_str()],
            music_rate,
            scroll_speed,
            active_color_index,
            CurrentScreen::Gameplay,
        )
    }

    fn prepare_player_options_for_practice_from_eval(&mut self) -> bool {
        if self.state.screens.current_screen != CurrentScreen::Evaluation {
            return false;
        }

        let score_info = &self.state.screens.evaluation_state.score_info;
        let Some((song, chart_hashes, music_rate, scroll_speed)) =
            restart_payload_from_eval(score_info)
        else {
            return false;
        };
        let active_color_index = self.state.screens.evaluation_state.active_color_index;
        self.prepare_restart_player_options(
            song,
            [chart_hashes[0].as_str(), chart_hashes[1].as_str()],
            music_rate,
            scroll_speed,
            active_color_index,
            CurrentScreen::Practice,
        )
    }

    fn try_gameplay_restart(&mut self, event_loop: &ActiveEventLoop, label: &str) -> bool {
        if !self.prepare_player_options_for_gameplay_restart() {
            log::warn!("Ignored {label} restart: no restartable stage state.");
            return false;
        }
        let restart_count = self.state.session.gameplay_restart_count.saturating_add(1);

        // SL/zmod parity: if we're already in Gameplay, run the fast Cancel
        // exit (~0.5s) instead of the full ~1.5s gameplay out-transition.
        // The Cancel navigation is intercepted in `handle_action` and
        // redirected back to Gameplay, which uses a shortened in-transition.
        if self.state.screens.current_screen == CurrentScreen::Gameplay
            && let Some(gs) = self.state.screens.gameplay_state.as_mut()
        {
            let already_exiting = gs.exit_transition_active();
            gs.begin_restart_exit();
            crate::screens::gameplay::drain_audio_commands(gs);
            if !already_exiting && gs.exit_transition_active() {
                self.state.session.gameplay_restart_count = restart_count;
                self.state.session.restart_pending = true;
            }
            return true;
        }

        // Fallback (e.g. Ctrl+R from Evaluation): use the standard navigation.
        if let Err(e) =
            self.handle_action(ScreenAction::Navigate(CurrentScreen::Gameplay), event_loop)
        {
            log::error!("Failed to restart Gameplay with {label}: {e}");
        } else {
            self.state.session.gameplay_restart_count = restart_count;
        }
        true
    }

    fn try_gameplay_reload(&mut self, event_loop: &ActiveEventLoop, label: &str) -> bool {
        let simfile_path = if let Some(gs) = self.state.screens.gameplay_state.as_ref() {
            Some(gs.song().simfile_path.clone())
        } else if self.state.screens.current_screen == CurrentScreen::Evaluation {
            restart_payload_from_eval(&self.state.screens.evaluation_state.score_info)
                .map(|(song, ..)| song.simfile_path.clone())
        } else {
            None
        };
        let Some(simfile_path) = simfile_path else {
            log::warn!("Ignored {label} reload: no restartable stage state.");
            return false;
        };

        let updated_song = match song_loading::reload_song_in_cache(simfile_path.as_path()) {
            Ok(song) => song,
            Err(e) => {
                log::warn!(
                    "Ignored {label} reload for '{}': {e}",
                    simfile_path.display()
                );
                return false;
            }
        };
        select_music::refresh_from_song_cache(&mut self.state.screens.select_music_state);

        if !self.try_gameplay_restart(event_loop, label) {
            return false;
        }

        if let Some(po_state) = self.state.screens.player_options_state.as_mut() {
            let _ = deadsync_simfile::runtime_cache::replace_song_arc_if_same_simfile(
                &mut po_state.song,
                &updated_song,
            );
        }
        true
    }

    /// SL-zmod parity (`BGAnimations/ScreenEvaluation common/Shared/RestartHandler.lua`):
    /// Ctrl+P on the Evaluation screen re-enters the just-played chart in
    /// Practice mode. Mirrors `try_gameplay_restart`, but routes to
    /// `CurrentScreen::Practice` and does not touch
    /// `gameplay_restart_count` / `restart_pending` (those are gameplay-only).
    fn try_practice_from_eval(&mut self, event_loop: &ActiveEventLoop, label: &str) -> bool {
        if self.state.screens.current_screen != CurrentScreen::Evaluation {
            return false;
        }
        if !self.prepare_player_options_for_practice_from_eval() {
            log::warn!("Ignored {label} practice: no replayable evaluation payload.");
            return false;
        }
        if let Err(e) =
            self.handle_action(ScreenAction::Navigate(CurrentScreen::Practice), event_loop)
        {
            log::error!("Failed to enter Practice with {label}: {e}");
            return false;
        }
        true
    }

    fn try_practice_reload(&mut self, event_loop: &ActiveEventLoop, label: &str) -> bool {
        if self.state.screens.current_screen != CurrentScreen::Practice {
            return false;
        }
        let Some((
            simfile_path,
            old_song,
            music_rate,
            scroll_speed,
            active_color_index,
            old_hashes,
            old_difficulties,
        )) = self.state.screens.practice_state.as_ref().map(|ps| {
            let gs = &ps.gameplay;
            (
                gs.song().simfile_path.clone(),
                gs.song_arc(),
                gs.music_rate(),
                [gs.scroll_speed_for_player(0), gs.scroll_speed_for_player(1)],
                gs.active_color_index(),
                [
                    gs.charts()[0].short_hash.clone(),
                    gs.charts()[1].short_hash.clone(),
                ],
                [
                    gs.charts()[0].difficulty.clone(),
                    gs.charts()[1].difficulty.clone(),
                ],
            )
        })
        else {
            log::warn!("Ignored {label} reload: no practice stage state.");
            return false;
        };

        let updated_song = match song_loading::reload_song_in_cache(simfile_path.as_path()) {
            Ok(song) => song,
            Err(e) => {
                log::warn!(
                    "Ignored {label} reload for '{}': {e}",
                    simfile_path.display()
                );
                return false;
            }
        };
        select_music::refresh_from_song_cache(&mut self.state.screens.select_music_state);

        let target_chart_type = profile::get_session_play_style().chart_type();
        let new_hashes = deadsync_simfile::runtime_cache::reloaded_chart_hashes_for_restart(
            old_song.as_ref(),
            updated_song.as_ref(),
            target_chart_type,
            [old_hashes[0].as_str(), old_hashes[1].as_str()],
            [old_difficulties[0].as_str(), old_difficulties[1].as_str()],
        );

        if !self.prepare_restart_player_options(
            updated_song,
            [new_hashes[0].as_str(), new_hashes[1].as_str()],
            music_rate,
            scroll_speed,
            active_color_index,
            CurrentScreen::Practice,
        ) {
            log::warn!("Ignored {label} reload: could not rebuild practice options.");
            return false;
        }
        if let Err(e) =
            self.handle_action(ScreenAction::Navigate(CurrentScreen::Practice), event_loop)
        {
            log::error!("Failed to reload Practice with {label}: {e}");
            return false;
        }
        true
    }

    fn should_chain_course_to_next_stage(&self) -> bool {
        self.state.screens.current_screen == CurrentScreen::Gameplay
            && !self.current_gameplay_stage_failed()
            && self
                .state
                .session
                .course_run
                .as_ref()
                .is_some_and(|course| course.next_stage_index < course.stages.len())
    }

    fn current_gameplay_stage_failed(&self) -> bool {
        let Some(gs) = self.state.screens.gameplay_state.as_ref() else {
            return false;
        };
        (0..gs.num_players().min(MAX_PLAYERS)).any(|player_idx| {
            let p = &gs.players()[player_idx];
            p.is_failing || p.life <= 0.0 || p.fail_time.is_some()
        })
    }

    fn append_stage_results_from_eval(
        &mut self,
        eval_state: &evaluation::State,
    ) -> Option<stage_stats::StageSummary> {
        let in_course_run = self.state.session.course_run.is_some();
        let stage_summary = stage_summary_from_eval(eval_state);
        if let Some(stage) = stage_summary.as_ref() {
            for side in [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2] {
                if let Some(p) = stage
                    .players
                    .get(profile_data::player_side_index(side))
                    .and_then(|p| p.as_ref())
                {
                    profile::add_stage_calories_for_side(side, p.calories_burned);
                }
            }
            self.state.session.played_stages.push(stage.clone());
            if in_course_run {
                self.state
                    .session
                    .course_individual_stage_indices
                    .push(self.state.session.played_stages.len().saturating_sub(1));
            }
        }
        if let Some(course_run) = self.state.session.course_run.as_mut() {
            if let Some(stage) = stage_summary.as_ref() {
                course_run.stage_summaries.push(stage.clone());
            }
            let mut stage_page = eval_state.clone();
            stage_page.return_to_course = true;
            stage_page.auto_advance_seconds = None;
            course_run.stage_eval_pages.push(stage_page);
        }
        stage_summary
    }

    fn finalize_entered_evaluation(&mut self) {
        if let Some(backend) = self.backend.as_mut() {
            self.dynamic_media
                .clear_gameplay_backgrounds(&mut self.asset_manager, backend);
        }

        let color_idx = self.state.screens.evaluation_state.active_color_index;
        let eval_snapshot = self.state.screens.evaluation_state.clone();
        let _ = self.append_stage_results_from_eval(&eval_snapshot);
        self.state.screens.evaluation_state.return_to_course =
            self.state.session.course_run.is_some();
        self.state.screens.evaluation_state.auto_advance_seconds = None;

        // Pass / Fail SFX (zmod parity, issue #375). Based on the per-stage
        // result that was just captured into `eval_snapshot`; even when that
        // is immediately replaced by a course summary, this is the cue tied to
        // the player's actual exit from gameplay.
        let failed = crate::screens::evaluation::all_joined_players_failed(&eval_snapshot);
        if visual_styles::srpg10_active() {
            let sfx = if failed {
                visual_styles::SRPG10_EVAL_FAILED_SFX
            } else {
                visual_styles::SRPG10_EVAL_PASSED_SFX
            };
            deadsync_audio_stream::play_screen_sfx(sfx);
        } else {
            let folder = if failed {
                "assets/sounds/evaluation_fail"
            } else {
                "assets/sounds/evaluation_pass"
            };
            crate::assets::audio_folder::play_random_screen_sfx(folder);
        }

        if self
            .state
            .session
            .course_run
            .as_ref()
            .is_some_and(|course_run| {
                stage_stats::course_eval_is_final(
                    course_run.next_stage_index,
                    course_run.stages.len(),
                    failed,
                )
            })
        {
            if let Some(course_run) = self.state.session.course_run.as_ref() {
                let score_hash = course_run.score_hash.clone();
                let per_song_pages = course_run.stage_eval_pages.clone();
                let course_graph_stages = build_course_graph_stages(course_run);
                let course_summary = build_course_summary_stage(course_run);
                self.state.session.course_run = None;
                self.state.session.course_eval_pages.clear();
                self.state.session.course_eval_page_index = 0;

                if let Some(course_stage) = course_summary {
                    for side in [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2] {
                        if let Some(player) =
                            course_stage.players[profile_data::player_side_index(side)].as_ref()
                        {
                            scores::save_local_summary_score_for_side(
                                score_hash.as_str(),
                                side,
                                course_stage.music_rate,
                                player,
                            );
                        }
                    }
                    self.state.session.played_stages.push(course_stage.clone());

                    let gameplay_elapsed = stage_stats::total_stage_duration_seconds(
                        &self.state.session.played_stages,
                    );
                    let session_elapsed = self.state.screens.evaluation_state.session_elapsed;
                    let screen_elapsed = self.state.screens.evaluation_state.screen_elapsed;
                    let mut course_page = build_course_summary_eval_state(
                        &course_stage,
                        &course_graph_stages,
                        color_idx,
                        session_elapsed,
                        gameplay_elapsed,
                    );
                    apply_course_summary_column_judgments(&mut course_page, &per_song_pages);
                    course_page.screen_elapsed = screen_elapsed;
                    self.state.screens.evaluation_state = course_page.clone();

                    let mut pages = Vec::with_capacity(per_song_pages.len().saturating_add(1));
                    pages.push(course_page);
                    for mut page in per_song_pages {
                        page.return_to_course = true;
                        page.auto_advance_seconds = None;
                        page.screen_elapsed = screen_elapsed;
                        page.session_elapsed = session_elapsed;
                        page.gameplay_elapsed = gameplay_elapsed;
                        pages.push(page);
                    }
                    self.state.session.course_eval_pages = pages;
                    self.state.session.course_eval_page_index = 0;
                }
            }
        } else {
            self.state.session.course_eval_pages.clear();
            self.state.session.course_eval_page_index = 0;
        }

        self.state.screens.evaluation_state.gameplay_elapsed =
            stage_stats::total_stage_duration_seconds(&self.state.session.played_stages);
    }

    fn post_select_display_stages(&self) -> Cow<'_, [stage_stats::StageSummary]> {
        let stages = &self.state.session.played_stages;
        let hidden = &self.state.session.course_individual_stage_indices;
        if config::get().show_course_individual_scores || hidden.is_empty() || stages.is_empty() {
            return Cow::Borrowed(stages.as_slice());
        }
        let mut filtered = Vec::with_capacity(stages.len().saturating_sub(hidden.len()));
        let mut hidden_idx = 0usize;
        for (idx, stage) in stages.iter().enumerate() {
            while hidden_idx < hidden.len() && hidden[hidden_idx] < idx {
                hidden_idx = hidden_idx.saturating_add(1);
            }
            if hidden_idx < hidden.len() && hidden[hidden_idx] == idx {
                continue;
            }
            filtered.push(stage.clone());
        }
        Cow::Owned(filtered)
    }

    fn step_course_eval_page(&mut self, delta: i32) {
        let len = self.state.session.course_eval_pages.len();
        if len <= 1 || delta == 0 {
            return;
        }
        let mut idx = self.state.session.course_eval_page_index as i32 + delta;
        if idx < 0 {
            idx += len as i32;
        }
        let idx = (idx as usize) % len;
        self.state.session.course_eval_page_index = idx;

        let mut page = self.state.session.course_eval_pages[idx].clone();
        page.screen_elapsed = self.state.screens.evaluation_state.screen_elapsed;
        page.session_elapsed = self.state.screens.evaluation_state.session_elapsed;
        page.gameplay_elapsed = self.state.screens.evaluation_state.gameplay_elapsed;
        page.return_to_course = true;
        page.auto_advance_seconds = None;
        self.state.screens.evaluation_state = page;
        deadsync_audio_stream::play_sfx("assets/sounds/change.ogg");
    }

    fn apply_select_music_join(&mut self, join_side: profile_data::PlayerSide) {
        let play_style = profile::get_session_play_style();
        let p1_pref =
            profile::preferred_difficulty_for_side(profile_data::PlayerSide::P1, play_style);
        let p2_pref =
            profile::preferred_difficulty_for_side(profile_data::PlayerSide::P2, play_style);

        let side = profile::get_session_player_side();
        let sm = &mut self.state.screens.select_music_state;
        if side == profile_data::PlayerSide::P2 && join_side == profile_data::PlayerSide::P1 {
            sm.p2_selected_steps_index = sm.selected_steps_index;
            sm.p2_preferred_difficulty_index = sm.preferred_difficulty_index;
            sm.selected_steps_index = p1_pref;
            sm.preferred_difficulty_index = p1_pref;
        } else {
            sm.p2_selected_steps_index = p2_pref;
            sm.p2_preferred_difficulty_index = p2_pref;
        }

        if let Some(select_music::MusicWheelEntry::Song(song)) =
            sm.entries.get(sm.selected_index).cloned()
        {
            let best_playable = |preferred: usize| {
                let mut best = None;
                let mut min_diff = i32::MAX;
                for i in 0..STANDARD_DIFFICULTY_COUNT {
                    if select_music::is_difficulty_playable(&song, i) {
                        let diff = (i as i32 - preferred as i32).abs();
                        if diff < min_diff {
                            min_diff = diff;
                            best = Some(i);
                        }
                    }
                }
                best
            };

            if let Some(idx) = best_playable(sm.preferred_difficulty_index) {
                sm.selected_steps_index = idx;
            }
            if let Some(idx) = best_playable(sm.p2_preferred_difficulty_index) {
                sm.p2_selected_steps_index = idx;
            }
        }

        self.state.session.preferred_difficulty_index = sm.preferred_difficulty_index;
        select_music::trigger_immediate_refresh(sm);
        select_music::prime_displayed_chart_data(sm);
    }

    fn try_handle_late_join(&mut self, ev: &InputEvent) -> bool {
        if !ev.pressed {
            return false;
        }
        let join_side = match ev.action {
            VirtualAction::p1_start => profile_data::PlayerSide::P1,
            VirtualAction::p2_start => profile_data::PlayerSide::P2,
            _ => return false,
        };

        let screen = self.state.screens.current_screen;
        if !matches!(
            screen,
            CurrentScreen::SelectColor
                | CurrentScreen::SelectStyle
                | CurrentScreen::SelectPlayMode
                | CurrentScreen::SelectMusic
                | CurrentScreen::SelectCourse
        ) {
            return false;
        }
        if screen == CurrentScreen::SelectMusic
            && !crate::screens::select_music::allows_late_join(
                &self.state.screens.select_music_state,
            )
        {
            return false;
        }
        if screen == CurrentScreen::SelectCourse
            && !crate::screens::select_course::allows_late_join(
                &self.state.screens.select_course_state,
            )
        {
            return false;
        }

        if profile::get_session_play_style() == profile_data::PlayStyle::Double {
            return false;
        }

        let p1_joined = profile::is_session_side_joined(profile_data::PlayerSide::P1);
        let p2_joined = profile::is_session_side_joined(profile_data::PlayerSide::P2);
        if p1_joined && p2_joined {
            return false;
        }
        if (join_side == profile_data::PlayerSide::P1 && p1_joined)
            || (join_side == profile_data::PlayerSide::P2 && p2_joined)
        {
            return false;
        }
        if !(p1_joined || p2_joined) {
            return false;
        }

        profile::set_session_joined(true, true);
        profile::set_session_play_style(profile_data::PlayStyle::Versus);
        let show_select_profile = config::get().machine_show_select_profile;
        let join_profile = if show_select_profile {
            profile_data::ActiveProfile::Guest
        } else {
            profile::get_default_profile_for_side(join_side)
        };
        let joined_profile = profile::set_active_profile_for_side(join_side, join_profile);
        self.state.session.combo_carry[profile_data::player_side_index(join_side)] =
            joined_profile.current_combo;
        if let Some(backend) = self.backend.as_mut() {
            self.dynamic_media.set_profile_avatar_for_side(
                &mut self.asset_manager,
                backend,
                join_side,
                joined_profile.avatar_path.clone(),
            );
        }

        if screen == CurrentScreen::SelectStyle {
            self.state.screens.select_style_state.selected_index = 1;
        }
        if screen == CurrentScreen::SelectMusic {
            self.apply_select_music_join(join_side);
            // Per Simply-Love-SM5#741: when the Select Profile screen is on,
            // prompt the late-joining player with the profile-select widget
            // instead of silently leaving them as Guest.
            if show_select_profile {
                crate::screens::select_music::open_late_join_profile_overlay(
                    &mut self.state.screens.select_music_state,
                    join_side,
                );
            }
        }

        deadsync_audio_stream::play_sfx("assets/sounds/start.ogg");
        true
    }

    fn reset_operator_game_state(&mut self) {
        const RESET_STYLE: profile_data::PlayStyle = profile_data::PlayStyle::Single;

        profile::set_session_play_style(RESET_STYLE);
        profile::set_session_play_mode(profile_data::PlayMode::Regular);
        profile::set_session_player_side(profile_data::PlayerSide::P1);
        profile::set_session_joined(false, false);
        profile::set_session_music_rate(1.0);
        profile::set_session_timing_tick_mode(profile_data::TimingTickMode::Off);
        profile::set_fast_profile_switch_from_select_music(false);

        let preferred =
            profile::preferred_difficulty_for_side(profile_data::PlayerSide::P1, RESET_STYLE);
        self.state.session = SessionState::new(preferred, profile::combo_carry());
        self.state.gameplay_offset_save_prompt = None;
    }

    fn route_operator_menu_button(&mut self, ev: &InputEvent) -> bool {
        if !ev.pressed || !lights::operator_menu_action(ev.action) {
            return false;
        }
        if !lights::screen_allows_operator_menu_button(screen_light_context(
            self.state.screens.current_screen,
        )) {
            return true;
        }

        info!("{SERVICE_SWITCH_PRESSED}");
        self.state.shell.gamepad_overlay_state =
            Some((SERVICE_SWITCH_PRESSED.to_string(), Instant::now()));
        self.reset_operator_game_state();
        self.handle_navigation_action_after_prompt(CurrentScreen::Options);
        true
    }

    fn route_input_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        ev: InputEvent,
    ) -> Result<(), Box<dyn Error>> {
        self.sync_light_input(&ev);
        if self.route_operator_menu_button(&ev) {
            return Ok(());
        }
        if self.route_gameplay_offset_prompt_input(event_loop, &ev) {
            return Ok(());
        }
        if self.try_handle_late_join(&ev) {
            return Ok(());
        }
        if config::get().only_dedicated_menu_buttons && ev.action.is_gameplay_arrow() {
            let allow_gameplay_arrow = match self.state.screens.current_screen {
                CurrentScreen::Gameplay | CurrentScreen::Practice | CurrentScreen::Input => true,
                // SelectMusic keeps raw pad arrows as code-detector input
                // in OnlyDedicated mode, but gates wheel navigation itself.
                CurrentScreen::SelectMusic => true,
                CurrentScreen::Evaluation => crate::screens::evaluation::test_input_pane_active(
                    &self.state.screens.evaluation_state,
                ),
                _ => false,
            };
            if !allow_gameplay_arrow {
                return Ok(());
            }
        }
        if ev.pressed
            && matches!(
                self.state.screens.current_screen,
                CurrentScreen::Evaluation | CurrentScreen::EvaluationSummary
            )
            && matches!(
                ev.action,
                VirtualAction::p1_select | VirtualAction::p2_select
            )
        {
            let side = match ev.action {
                VirtualAction::p1_select => Some(profile_data::PlayerSide::P1),
                VirtualAction::p2_select => Some(profile_data::PlayerSide::P2),
                _ => None,
            };
            self.state.shell.screenshot.request(side);
            return Ok(());
        }
        if ev.pressed
            && matches!(
                self.state.screens.current_screen,
                CurrentScreen::Gameplay | CurrentScreen::Evaluation
            )
            && self.state.gameplay_offset_save_prompt.is_none()
            && self.state.session.course_run.is_none()
            && matches!(
                ev.action,
                VirtualAction::p1_restart | VirtualAction::p2_restart
            )
        {
            self.try_gameplay_restart(event_loop, "Restart button");
            return Ok(());
        }
        let action = match self.state.screens.current_screen {
            CurrentScreen::Menu => {
                crate::screens::menu::handle_input(&mut self.state.screens.menu_state, &ev)
            }
            CurrentScreen::SelectProfile => crate::screens::select_profile::handle_input(
                &mut self.state.screens.select_profile_state,
                &ev,
            ),
            CurrentScreen::SelectColor => crate::screens::select_color::handle_input(
                &mut self.state.screens.select_color_state,
                &ev,
            ),
            CurrentScreen::ArrowCloudLogin => crate::screens::arrowcloud_login::handle_input(
                &mut self.state.screens.arrowcloud_login_state,
                &ev,
            ),
            CurrentScreen::GrooveStatsLogin => crate::screens::groovestats_login::handle_input(
                &mut self.state.screens.groovestats_login_state,
                &ev,
            ),
            CurrentScreen::SelectStyle => crate::screens::select_style::handle_input(
                &mut self.state.screens.select_style_state,
                &ev,
            ),
            CurrentScreen::SelectPlayMode => crate::screens::select_mode::handle_input(
                &mut self.state.screens.select_play_mode_state,
                &ev,
            ),
            CurrentScreen::ProfileLoad => crate::screens::profile_load::handle_input(
                &mut self.state.screens.profile_load_state,
                &ev,
            ),
            CurrentScreen::Options => crate::screens::options::handle_input(
                &mut self.state.screens.options_state,
                &self.asset_manager,
                &ev,
            ),
            CurrentScreen::Credits => {
                crate::screens::credits::handle_input(&mut self.state.screens.credits_state, &ev)
            }
            CurrentScreen::ManageLocalProfiles => {
                crate::screens::manage_local_profiles::handle_input(
                    &mut self.state.screens.manage_local_profiles_state,
                    &ev,
                )
            }
            CurrentScreen::Mappings => {
                crate::screens::mappings::handle_input(&mut self.state.screens.mappings_state, &ev)
            }
            CurrentScreen::Input => {
                crate::screens::input::handle_input(&mut self.state.screens.input_state, &ev)
            }
            CurrentScreen::ConfigurePads => crate::screens::pad_config::handle_input(
                &mut self.state.screens.pad_config_state,
                &ev,
                self.state.shell.shift_held,
            ),
            CurrentScreen::TestLights => crate::screens::test_lights::handle_input(
                &mut self.state.screens.test_lights_state,
                &ev,
            ),
            CurrentScreen::OverscanAdjustment => crate::screens::overscan_adjustment::handle_input(
                &mut self.state.screens.overscan_adjustment_state,
                &ev,
            ),
            CurrentScreen::SmxAssignPads => crate::screens::smx_assign::handle_input(
                &mut self.state.screens.smx_assign_state,
                &ev,
            ),
            CurrentScreen::SelectMusic => crate::screens::select_music::handle_input(
                &mut self.state.screens.select_music_state,
                &ev,
                self.state.shell.shift_held,
            ),
            CurrentScreen::SelectCourse => crate::screens::select_course::handle_input(
                &mut self.state.screens.select_course_state,
                &ev,
            ),
            CurrentScreen::PlayerOptions => {
                if let Some(pos) = &mut self.state.screens.player_options_state {
                    crate::screens::player_options::handle_input(pos, &self.asset_manager, &ev)
                } else {
                    ScreenAction::None
                }
            }
            CurrentScreen::Evaluation => crate::screens::evaluation::handle_input(
                &mut self.state.screens.evaluation_state,
                &ev,
            ),
            CurrentScreen::EvaluationSummary => {
                let num_stages = self.post_select_display_stages().len();
                crate::screens::evaluation_summary::handle_input(
                    &mut self.state.screens.evaluation_summary_state,
                    num_stages,
                    &ev,
                )
            }
            CurrentScreen::Initials => {
                crate::screens::initials::handle_input(&mut self.state.screens.initials_state, &ev)
            }
            CurrentScreen::GameOver => {
                crate::screens::gameover::handle_input(&mut self.state.screens.gameover_state, &ev)
            }
            CurrentScreen::Sandbox => {
                crate::screens::sandbox::handle_input(&mut self.state.screens.sandbox_state, &ev)
            }
            CurrentScreen::Init => {
                crate::screens::init::handle_input(&mut self.state.screens.init_state, &ev)
            }
            CurrentScreen::Gameplay => {
                if let Some(gs) = &mut self.state.screens.gameplay_state {
                    crate::screens::gameplay::handle_input(gs, &ev)
                } else {
                    ScreenAction::None
                }
            }
            CurrentScreen::Practice => {
                if let Some(ps) = &mut self.state.screens.practice_state {
                    crate::screens::practice::handle_input(ps, &ev)
                } else {
                    ScreenAction::None
                }
            }
        };
        if matches!(action, ScreenAction::None) {
            return Ok(());
        }
        self.handle_action(action, event_loop)
    }

    fn refresh_gameplay_background_path(
        state: &mut gameplay::State,
        show_video_backgrounds: bool,
    ) -> Option<PathBuf> {
        let path = state
            .song()
            .gameplay_background_path_for_changes(
                &state.background_changes,
                state.next_background_change_ix,
                show_video_backgrounds,
            )
            .cloned();
        state.current_background_key = path.as_deref().map(crate::assets::media_path_key);
        state.current_background_path = path.clone();
        state.background_allow_video = show_video_backgrounds;
        state.background_path_dirty = false;
        path
    }

    fn active_gameplay_background_change(
        state: &gameplay::State,
    ) -> Option<&deadsync_chart::SongBackgroundChange> {
        state
            .next_background_change_ix
            .checked_sub(1)
            .and_then(|ix| state.background_changes.get(ix))
    }

    fn sync_gameplay_background(&mut self) {
        if !matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        ) {
            return;
        }
        let show_video_backgrounds = config::get().show_video_backgrounds;
        let (desired_path, desired_key, gameplay_time_sec, background_rate) = {
            let gs = match self.state.screens.current_screen {
                CurrentScreen::Gameplay => self.state.screens.gameplay_state.as_mut(),
                CurrentScreen::Practice => self
                    .state
                    .screens
                    .practice_state
                    .as_mut()
                    .map(|state| &mut state.gameplay),
                _ => None,
            };
            let Some(gs) = gs else {
                return;
            };
            let old_path_key = gs.current_background_key.clone();
            let old_texture_key = gs.background_texture_key.clone();
            let had_pending_background_change = gs.background_path_dirty;
            let mut background_changed = false;
            while let Some(change) = gs.background_changes.get(gs.next_background_change_ix) {
                if gs.current_beat() < change.start_beat {
                    break;
                }
                gs.next_background_change_ix += 1;
                background_changed = true;
            }
            if background_changed {
                gs.background_path_dirty = true;
            }
            if gs.background_path_dirty || gs.background_allow_video != show_video_backgrounds {
                Self::refresh_gameplay_background_path(gs, show_video_backgrounds);
            }
            if (background_changed || had_pending_background_change)
                && old_path_key != gs.current_background_key
            {
                let transition = Self::active_gameplay_background_change(gs)
                    .map(|change| change.transition.clone())
                    .unwrap_or_default();
                if transition.is_empty() || &*old_texture_key == "__black" {
                    gs.previous_background_texture_key = None;
                    gs.background_transition.clear();
                } else {
                    gs.previous_background_texture_key = Some(old_texture_key);
                    gs.background_transition = transition;
                    gs.background_transition_start_time =
                        deadsync_core::song_time::song_time_ns_to_seconds(
                            gs.current_music_time_ns(),
                        );
                }
            }
            (
                gs.current_background_path.clone(),
                gs.current_background_key.clone(),
                deadsync_core::song_time::song_time_ns_to_seconds(gs.current_music_time_ns()),
                Self::active_gameplay_background_change(gs)
                    .map(|change| change.rate)
                    .unwrap_or(1.0),
            )
        };

        let next_key = self.backend.as_mut().and_then(|backend| {
            self.dynamic_media.sync_gameplay_background(
                &mut self.asset_manager,
                backend,
                desired_path.as_deref(),
                desired_key.as_deref(),
                show_video_backgrounds,
                gameplay_time_sec,
                background_rate,
            )
        });
        if let Some(key) = next_key {
            let key = Arc::<str>::from(key);
            match self.state.screens.current_screen {
                CurrentScreen::Gameplay => {
                    if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
                        gs.background_texture_key = key;
                    }
                }
                CurrentScreen::Practice => {
                    if let Some(ps) = self.state.screens.practice_state.as_mut() {
                        ps.gameplay.background_texture_key = key;
                    }
                }
                _ => {}
            }
        }
        let gs = match self.state.screens.current_screen {
            CurrentScreen::Gameplay => self.state.screens.gameplay_state.as_ref(),
            CurrentScreen::Practice => self
                .state
                .screens
                .practice_state
                .as_ref()
                .map(|state| &state.gameplay),
            _ => None,
        };
        if let (Some(backend), Some(gs)) = (self.backend.as_mut(), gs) {
            let overlay_video_paths = gameplay_overlay_video_paths(gs);
            self.dynamic_media.sync_active_song_lua_videos(
                &mut self.asset_manager,
                backend,
                &overlay_video_paths,
            );
        }
    }

    fn sync_theme_background_video(&mut self, ui_time_sec: f32) {
        if matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        ) {
            crate::screens::components::shared::visual_style_bg::set_srpg_background_key(None);
            return;
        }

        let cfg = config::get();
        let path = (cfg.visual_style.is_srpg() && cfg.show_video_backgrounds)
            .then(visual_styles::shared_background_video_asset_path)
            .flatten()
            .map(|path| dirs::app_dirs().resolve_asset_path(path));

        let Some(backend) = self.backend.as_mut() else {
            crate::screens::components::shared::visual_style_bg::set_srpg_background_key(None);
            return;
        };

        let key =
            self.dynamic_media
                .set_background(&mut self.asset_manager, backend, path, ui_time_sec);
        let srpg_key = if key == "__black" { None } else { Some(key) };
        crate::screens::components::shared::visual_style_bg::set_srpg_background_key(srpg_key);
    }

    fn append_gameplay_offset_prompt_actors(&self, actors: &mut Vec<Actor>) {
        if self.state.screens.current_screen != CurrentScreen::Gameplay
            || self.state.gameplay_offset_save_prompt.is_none()
        {
            return;
        }
        let Some(gs) = self.state.screens.gameplay_state.as_ref() else {
            return;
        };
        if !Self::gameplay_saveable_offset_changed(gs) {
            return;
        }
        let active_choice = self
            .state
            .gameplay_offset_save_prompt
            .as_ref()
            .map_or(0, |prompt| prompt.active_choice)
            .min(1);
        let prompt_text = Self::gameplay_sync_prompt_text(gs);
        if prompt_text.is_empty() {
            return;
        }

        let w = space::screen_width();
        let h = space::screen_height();
        let cx = w * 0.5;
        let cy = h * 0.5;
        let answer_y = cy + 120.0;
        let choice_yes_x = cx - 100.0;
        let choice_no_x = cx + 100.0;
        let cursor_x = [choice_yes_x, choice_no_x][active_choice as usize];
        let cursor_color = color::simply_love_rgba(gs.active_color_index());

        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(w, h):
            diffuse(0.0, 0.0, 0.0, 0.9):
            z(GAMEPLAY_OFFSET_PROMPT_Z_BACKDROP)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(cursor_x, answer_y):
            setsize(145.0, 40.0):
            diffuse(cursor_color[0], cursor_color[1], cursor_color[2], 1.0):
            z(GAMEPLAY_OFFSET_PROMPT_Z_CURSOR)
        ));
        actors.push(act!(text:
            align(0.5, 0.5):
            xy(cx, cy - 60.0):
            font("miso"):
            zoom(0.95):
            maxwidth(w - 100.0):
            settext(prompt_text):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GAMEPLAY_OFFSET_PROMPT_Z_TEXT):
            horizalign(center)
        ));
        actors.push(act!(text:
            align(0.5, 0.5):
            xy(choice_yes_x, answer_y):
            font("wendy"):
            zoom(0.72):
            settext("YES"):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GAMEPLAY_OFFSET_PROMPT_Z_TEXT):
            horizalign(center)
        ));
        actors.push(act!(text:
            align(0.5, 0.5):
            xy(choice_no_x, answer_y):
            font("wendy"):
            zoom(0.72):
            settext("NO"):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GAMEPLAY_OFFSET_PROMPT_Z_TEXT):
            horizalign(center)
        ));
    }

    fn get_current_actors(&mut self) -> (Vec<Actor>, [f32; 4]) {
        const CLEAR: [f32; 4] = [0.03, 0.03, 0.03, 1.0];
        let mut screen_alpha_multiplier = 1.0;

        let is_actor_fade_screen = Self::is_actor_fade_screen(self.state.screens.current_screen);

        if is_actor_fade_screen {
            match self.state.shell.transition {
                TransitionState::ActorsFadeIn { elapsed } => {
                    screen_alpha_multiplier = (elapsed / MENU_ACTORS_FADE_DURATION).clamp(0.0, 1.0);
                }
                TransitionState::ActorsFadeOut {
                    elapsed, duration, ..
                } => {
                    screen_alpha_multiplier = 1.0 - (elapsed / duration).clamp(0.0, 1.0);
                }
                _ => {}
            }
        }

        let mut actors = std::mem::take(&mut self.actor_scratch);
        actors.clear();

        match self.state.screens.current_screen {
            CurrentScreen::Menu => {
                menu::push_actors(
                    &mut actors,
                    &self.state.screens.menu_state,
                    screen_alpha_multiplier,
                );
            }
            CurrentScreen::Gameplay => {
                if let Some(gs) = &mut self.state.screens.gameplay_state {
                    crate::screens::components::gameplay::gameplay_stats::refresh_density_graph_meshes(gs);
                    let smx_overlay_alpha = match self.state.shell.transition {
                        TransitionState::FadingIn { elapsed, duration } => {
                            if duration <= gameplay::TRANSITION_IN_RESTART_DURATION + 0.01 {
                                // Restart: the in-transition black fades over the whole short
                                // duration; mirror it so the overlays fade in with the black.
                                (elapsed / duration).clamp(0.0, 1.0)
                            } else {
                                // Normal entry: black holds solid until the last
                                // TRANSITION_IN_BLACK_FADE_DURATION seconds, then lifts.
                                let fade_start =
                                    duration - gameplay::TRANSITION_IN_BLACK_FADE_DURATION;
                                ((elapsed - fade_start)
                                    / gameplay::TRANSITION_IN_BLACK_FADE_DURATION)
                                    .clamp(0.0, 1.0)
                            }
                        }
                        TransitionState::FadingOut { elapsed, .. } => {
                            // Mirror the out-transition black quad: hold full opacity
                            // during TRANSITION_OUT_DELAY then fade down as the black fades up.
                            1.0 - ((elapsed - gameplay::TRANSITION_OUT_DELAY)
                                / gameplay::TRANSITION_OUT_FADE_DURATION)
                                .clamp(0.0, 1.0)
                        }
                        _ => 1.0,
                    };
                    gameplay::push_actors(
                        &mut actors,
                        gs,
                        &self.asset_manager,
                        gameplay::ActorViewOverride {
                            smx_overlay_alpha,
                            ..Default::default()
                        },
                    );
                }
            }
            CurrentScreen::Practice => {
                if let Some(ps) = &mut self.state.screens.practice_state {
                    crate::screens::components::gameplay::gameplay_stats::refresh_density_graph_meshes(
                        &mut ps.gameplay,
                    );
                    practice::push_actors(&mut actors, ps, &self.asset_manager);
                }
            }
            CurrentScreen::Options => options::push_actors(
                &mut actors,
                &self.state.screens.options_state,
                &self.asset_manager,
                screen_alpha_multiplier,
            ),
            CurrentScreen::Credits => {
                credits::push_actors(&mut actors, &self.state.screens.credits_state)
            }
            CurrentScreen::ManageLocalProfiles => manage_local_profiles::push_actors(
                &mut actors,
                &self.state.screens.manage_local_profiles_state,
                &self.asset_manager,
                screen_alpha_multiplier,
            ),
            CurrentScreen::Mappings => mappings::push_actors(
                &mut actors,
                &self.state.screens.mappings_state,
                &self.asset_manager,
                screen_alpha_multiplier,
            ),
            CurrentScreen::Input => {
                input_screen::push_actors(&mut actors, &self.state.screens.input_state)
            }
            CurrentScreen::ConfigurePads => {
                crate::screens::pad_config::push_actors(
                    &mut actors,
                    &self.state.screens.pad_config_state,
                );
            }
            CurrentScreen::TestLights => test_lights::push_actors(
                &mut actors,
                &self.state.screens.test_lights_state,
                self.lights.state_snapshot(),
                self.lights.mode(),
                screen_alpha_multiplier,
            ),
            CurrentScreen::OverscanAdjustment => overscan_adjustment::push_actors(
                &mut actors,
                &self.state.screens.overscan_adjustment_state,
                screen_alpha_multiplier,
            ),
            CurrentScreen::SmxAssignPads => crate::screens::smx_assign::push_actors(
                &mut actors,
                &self.state.screens.smx_assign_state,
                screen_alpha_multiplier,
            ),
            CurrentScreen::PlayerOptions => {
                if let Some(pos) = &self.state.screens.player_options_state {
                    player_options::push_actors(&mut actors, pos, &self.asset_manager);
                }
            }
            CurrentScreen::SelectProfile => select_profile::push_actors(
                &mut actors,
                &self.state.screens.select_profile_state,
                &self.asset_manager,
                screen_alpha_multiplier,
            ),
            CurrentScreen::SelectColor => select_color::push_actors(
                &mut actors,
                &self.state.screens.select_color_state,
                screen_alpha_multiplier,
            ),
            CurrentScreen::ArrowCloudLogin => crate::screens::arrowcloud_login::push_actors(
                &mut actors,
                &self.state.screens.arrowcloud_login_state,
                screen_alpha_multiplier,
            ),
            CurrentScreen::GrooveStatsLogin => crate::screens::groovestats_login::push_actors(
                &mut actors,
                &self.state.screens.groovestats_login_state,
                screen_alpha_multiplier,
            ),
            CurrentScreen::SelectStyle => {
                select_style::push_actors(&mut actors, &self.state.screens.select_style_state);
            }
            CurrentScreen::SelectPlayMode => select_mode::push_actors(
                &mut actors,
                &self.state.screens.select_play_mode_state,
                &self.asset_manager,
            ),
            CurrentScreen::ProfileLoad => {
                profile_load::push_actors(&mut actors, &self.state.screens.profile_load_state);
            }
            CurrentScreen::SelectMusic => {
                select_music::push_actors(
                    &mut actors,
                    &self.state.screens.select_music_state,
                    &self.asset_manager,
                    self.state.session.played_stages.len() + 1,
                );
            }
            CurrentScreen::SelectCourse => select_course::push_actors(
                &mut actors,
                &self.state.screens.select_course_state,
                &self.asset_manager,
            ),
            CurrentScreen::Sandbox => {
                sandbox::push_actors(&mut actors, &self.state.screens.sandbox_state)
            }
            CurrentScreen::Init => init::push_actors(&mut actors, &self.state.screens.init_state),
            CurrentScreen::Evaluation => {
                evaluation::push_actors(
                    &mut actors,
                    &self.state.screens.evaluation_state,
                    &self.asset_manager,
                );
            }
            CurrentScreen::EvaluationSummary => {
                let stages = self.post_select_display_stages();
                evaluation_summary::push_actors(
                    &mut actors,
                    &self.state.screens.evaluation_summary_state,
                    &stages,
                    &self.asset_manager,
                );
            }
            CurrentScreen::Initials => {
                let stages = self.post_select_display_stages();
                initials::push_actors(
                    &mut actors,
                    &self.state.screens.initials_state,
                    &stages,
                    &self.asset_manager,
                );
            }
            CurrentScreen::GameOver => gameover::push_actors(
                &mut actors,
                &self.state.screens.gameover_state,
                &self.state.session.played_stages,
                &self.asset_manager,
            ),
        };

        if self.state.shell.overlay_mode.shows_fps() {
            let overlay = crate::screens::components::shared::stats_overlay::build(
                self.backend_type,
                self.state.shell.last_fps,
                self.state.shell.last_vpf,
                self.stats_overlay_timing(),
            );
            actors.extend(overlay);
            if self.state.shell.overlay_mode.shows_stutter() {
                let now_seconds = Instant::now()
                    .duration_since(self.state.shell.start_time)
                    .as_secs_f32();
                let stutters = self.collect_visible_stutters(now_seconds);
                actors.extend(
                    crate::screens::components::shared::stats_overlay::build_stutter(&stutters),
                );
            }
        }

        self.push_frame_stats_overlay(&mut actors);

        // Bottom-corner build watermark so videos / screenshots always
        // carry the running version. Default on; user-toggleable via
        // Options, with a separate Left/Right side preference.
        let cfg = crate::config::get();
        if cfg.show_version_overlay {
            actors.extend(crate::screens::components::shared::version_overlay::build(
                cfg.version_overlay_side,
                cfg.log_level,
            ));
        }

        // Gamepad connection overlay (always on top of screen, but below transitions)
        if let Some((msg, _)) = &self.state.shell.gamepad_overlay_state {
            let params =
                crate::screens::components::shared::gamepad_overlay::Params { message: msg };
            actors.extend(crate::screens::components::shared::gamepad_overlay::build(
                params,
            ));
        }
        self.append_gameplay_offset_prompt_actors(&mut actors);

        match &self.state.shell.transition {
            TransitionState::FadingOut { .. } => {
                let (out_actors, _) =
                    self.get_out_transition_for_screen(self.state.screens.current_screen);
                actors.extend(out_actors);
            }
            TransitionState::ActorsFadeOut { target, .. } => {
                // Special case: Menu -> SelectColor / Menu -> Options should keep the
                // visual-style background bright and only fade UI, but still play the splash.
                if self.state.screens.current_screen == CurrentScreen::Menu
                    && (*target == CurrentScreen::SelectProfile
                        || *target == CurrentScreen::SelectColor
                        || *target == CurrentScreen::Options)
                {
                    let splash = crate::screens::components::menu::menu_splash::build(
                        self.state.screens.menu_state.active_color_index,
                    );
                    actors.extend(splash);
                }
            }
            TransitionState::FadingIn { .. } => {
                let (in_actors, _) =
                    self.get_in_transition_for_screen(self.state.screens.current_screen);
                actors.extend(in_actors);
            }
            _ => {}
        }

        self.append_screenshot_overlay_actors(&mut actors, Instant::now());

        (actors, CLEAR)
    }

    fn collect_visible_stutters(
        &self,
        now_seconds: f32,
    ) -> Vec<crate::screens::components::shared::stats_overlay::StutterEvent> {
        self.state
            .shell
            .stutter_samples
            .visible(now_seconds)
            .into_iter()
            .map(
                |sample| crate::screens::components::shared::stats_overlay::StutterEvent {
                    timestamp_seconds: sample.timestamp_seconds,
                    frame_ms: sample.frame_ms,
                    frame_multiple: sample.frame_multiple,
                    severity: sample.severity,
                    age_seconds: sample.age_seconds,
                },
            )
            .collect()
    }

    #[inline(always)]
    fn expected_frame_seconds_for_stutter(&self) -> f32 {
        let fps = self.state.shell.last_fps;
        if fps > 0.0 {
            return 1.0 / fps;
        }
        if let Some(interval) = self.effective_frame_interval() {
            return interval.as_secs_f32();
        }
        if self.state.shell.vsync_enabled {
            return 1.0 / 60.0;
        }
        0.0
    }

    #[inline(always)]
    fn update_stutter_samples(&mut self, frame_seconds: f32, total_elapsed: f32) {
        if !self.state.shell.overlay_mode.shows_stutter() {
            return;
        }
        let expected = self.expected_frame_seconds_for_stutter();
        let severity = stutter_severity(frame_seconds, expected);
        if severity == 0 {
            return;
        }
        self.state
            .shell
            .push_stutter_sample(total_elapsed, frame_seconds, expected, severity);
    }

    #[inline(always)]
    fn record_frame_stats_sample(
        &mut self,
        frame_host_nanos: u64,
        frame_seconds: f32,
        input_us: u32,
        update_us: u32,
        compose_us: u32,
        upload_us: u32,
        draw_us: u32,
        draw_stats: renderer::DrawStats,
    ) {
        if !self.state.shell.frame_stats_overlay_enabled {
            return;
        }
        let display_clock = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|gs| gs.display_clock_health())
            .unwrap_or_default();
        let display_error_us = (f64::from(display_clock.error_seconds) * 1_000_000.0)
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
        let sample = FrameStatsSample {
            host_nanos: frame_host_nanos.max(1),
            frame_us: seconds_to_us_u32(frame_seconds),
            input_us,
            update_us,
            compose_us,
            upload_us,
            draw_us,
            gpu_wait_us: draw_stats.gpu_wait_us,
            display_error_us,
            catching_up: display_clock.catching_up,
        };
        self.state.shell.frame_stats.push(sample);
        self.state.shell.frame_stats_long.push(&sample);
        update_frame_stats_spike_hold(
            &mut self.state.shell.frame_stats_spike_us,
            &mut self.state.shell.frame_stats_spike_ttl,
            sample.frame_us,
        );
    }

    fn push_frame_stats_overlay(&mut self, actors: &mut Vec<Actor>) {
        use crate::screens::components::shared::frame_stats_overlay;

        if !self.state.shell.frame_stats_overlay_enabled {
            return;
        }
        self.state
            .shell
            .frame_stats
            .snapshot(&mut self.state.shell.frame_stats_scratch);
        let samples = &self.state.shell.frame_stats_scratch;

        // Graph scale still tracks the live ring max; all displayed numbers come from the
        // long-window streaming stats (decaying-histogram p99 + EWMA mean/jitter) so they
        // stay steady instead of sawtoothing as outliers enter and leave a short window.
        let mut max_us: u32 = 0;
        for s in samples.iter() {
            if s.host_nanos == 0 {
                continue;
            }
            max_us = max_us.max(s.frame_us);
        }
        let long = &self.state.shell.frame_stats_long;
        let avg_frame_us = long.avg_frame_us();
        let p99_frame_us = long.p99_frame_us();
        let frame_jitter_us = long.frame_jitter_us();
        let display_error_jitter_us = long.error_jitter_us();
        let display_error_p99_ms = f64::from(long.p99_error_us()) as f32 / 1000.0;
        let cpu_work_us = long.avg_cpu_us();
        let gpu_wait_us = long.avg_gpu_us();
        let spike_hold_us = self.state.shell.frame_stats_spike_us.max(max_us);

        // Target frame time for the graph reference lines: the monitor refresh period if
        // known, else the configured max-FPS cap, else the smoothed average.
        let refresh_ns = self.state.shell.last_present_stats.refresh_ns;
        let target_frame_us = if refresh_ns != 0 {
            (refresh_ns / 1000) as u32
        } else if let Some(iv) = self.effective_frame_interval() {
            iv.as_micros().min(u128::from(u32::MAX)) as u32
        } else {
            avg_frame_us
        };

        // Stutter tally over the rolling ring window: frames past the 2× stutter threshold
        // (the orange reference line), and distinct display-clock catch-up events (rising
        // edges so a multi-frame resync counts once). Cheap single pass, gated to overlay-on.
        let over_budget_threshold = target_frame_us.saturating_mul(2).max(1);
        let mut over_budget_count: u32 = 0;
        let mut catch_up_count: u32 = 0;
        let mut prev_catch = false;
        for s in samples.iter() {
            if s.host_nanos == 0 {
                continue;
            }
            if s.frame_us >= over_budget_threshold {
                over_budget_count = over_budget_count.saturating_add(1);
            }
            if s.catching_up && !prev_catch {
                catch_up_count = catch_up_count.saturating_add(1);
            }
            prev_catch = s.catching_up;
        }

        let in_gameplay = self.state.screens.current_screen == CurrentScreen::Gameplay;
        let display_clock = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|gs| gs.display_clock_health())
            .unwrap_or_default();

        let audio = deadsync_audio_stream::get_output_timing_snapshot();
        let raw_callback_gap_ms =
            deadsync_audio_stream::timing_diag_last_callback_gap_ns() as f32 / 1_000_000.0;
        // Smooth the callback gap so the readout stops bouncing between e.g. 9.xx and 10.xx
        // every frame. Seed on the first sample, then EWMA at the same rate as the frame
        // average. Negative/zero raw values (no data yet) pass through unsmoothed.
        let callback_gap_ms = if raw_callback_gap_ms > 0.0 {
            let prev = self.state.shell.frame_stats_audio_gap_ms;
            let smoothed = if prev > 0.0 {
                prev + frame_stats_overlay::EWMA_ALPHA_MEAN * (raw_callback_gap_ms - prev)
            } else {
                raw_callback_gap_ms
            };
            self.state.shell.frame_stats_audio_gap_ms = smoothed;
            smoothed
        } else {
            raw_callback_gap_ms
        };

        let summary = frame_stats_overlay::FrameStatsSummary {
            avg_frame_us,
            p99_frame_us,
            max_frame_us: max_us,
            fps: self.state.shell.last_fps,
            display_error_ms: display_clock.error_seconds * 1000.0,
            display_error_p99_ms,
            display_catching_up: display_clock.catching_up,
            in_gameplay,
            audio_callback_gap_ms: callback_gap_ms,
            audio_underruns: audio.underrun_count,
            audio_output_delay_ms: audio.estimated_output_delay_ns as f32 / 1_000_000.0,
            audio_queued_frames: audio.queued_frames,
            frame_jitter_us,
            display_error_jitter_us,
            spike_hold_us,
            target_frame_us,
            cpu_work_us,
            gpu_wait_us,
            over_budget_count,
            catch_up_count,
        };
        let anchor = self.state.shell.frame_stats_overlay_anchor;
        let style = self.state.shell.frame_stats_overlay_style;
        let screen_w = deadlib_present::space::screen_width();
        let screen_h = deadlib_present::space::screen_height();
        // Always render the full overlay, including 2 players — the panel is narrow enough
        // (~half-screen) to sit in a corner or the bottom-center seam without covering either
        // notefield, so there's no need to drop to the stripped compact layout.
        actors.extend(frame_stats_overlay::build(
            samples, summary, anchor, false, style, screen_w, screen_h,
        ));
    }

    /// Current play context for overlay placement: `(in_gameplay, two_player, player_is_p2)`.
    /// Two-player covers Versus/Double or any 2+ active notefields (both sides occupied).
    fn frame_stats_play_context(&self) -> (bool, bool, bool) {
        let in_gameplay = self.state.screens.current_screen == CurrentScreen::Gameplay;
        let play_style = deadsync_profile::compat::get_session_play_style();
        let side = deadsync_profile::compat::get_session_player_side();
        let num_players = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|gs| gs.num_players())
            .unwrap_or(1);
        let two_player = matches!(
            play_style,
            profile_data::PlayStyle::Versus | profile_data::PlayStyle::Double
        ) || num_players >= 2;
        let player_is_p2 = matches!(side, profile_data::PlayerSide::P2);
        (in_gameplay, two_player, player_is_p2)
    }

    #[inline(always)]
    fn record_stutter_diag_frame(
        &mut self,
        frame_host_nanos: u64,
        screen: CurrentScreen,
        frame_seconds: f32,
        pre_redraw_gap_us: u32,
        request_to_redraw_us: u32,
        redraw_request_reason: &'static str,
        input_us: u32,
        update_us: u32,
        compose_us: u32,
        upload_us: u32,
        draw_us: u32,
        draw_stats: renderer::DrawStats,
    ) {
        if !stutter_diag_enabled() {
            return;
        }
        let display_clock = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|gs| gs.display_clock_health())
            .unwrap_or_default();
        let display_error_us_i64 =
            (f64::from(display_clock.error_seconds) * 1_000_000.0).round() as i64;
        let display_error_us =
            display_error_us_i64.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let present_stats = draw_stats.present_stats;
        self.state
            .shell
            .stutter_diag_frames
            .push(StutterDiagFrameSample {
                host_nanos: frame_host_nanos,
                screen,
                redraw_request_reason,
                frame_us: seconds_to_us_u32(frame_seconds),
                expected_us: seconds_to_us_u32(self.expected_frame_seconds_for_stutter()),
                pre_redraw_gap_us,
                request_to_redraw_us,
                input_us,
                update_us,
                compose_us,
                upload_us,
                draw_us,
                acquire_us: draw_stats.acquire_us,
                submit_us: draw_stats.submit_us,
                present_us: draw_stats.present_us,
                gpu_wait_us: draw_stats.gpu_wait_us,
                draw_setup_us: draw_stats.backend_setup_us,
                draw_prepare_us: draw_stats.backend_prepare_us,
                draw_record_us: draw_stats.backend_record_us,
                display_error_us,
                display_catching_up: display_clock.catching_up,
                present_mode: present_stats.mode,
                present_display_clock: present_stats.display_clock,
                present_host_clock: present_stats.host_clock,
                in_flight_images: present_stats.in_flight_images,
                waited_for_image: present_stats.waited_for_image,
                applied_back_pressure: present_stats.applied_back_pressure,
                queue_idle_waited: present_stats.queue_idle_waited,
                suboptimal: present_stats.suboptimal,
            });
    }

    fn dump_stutter_diag_window(
        &self,
        now_host_nanos: u64,
        total_elapsed: f32,
        screen: CurrentScreen,
        stutter_severity: u8,
        audio_triggered: bool,
        display_triggered: bool,
    ) {
        let mut frames = Vec::with_capacity(STUTTER_DIAG_FRAME_SAMPLE_COUNT);
        self.state.shell.stutter_diag_frames.collect_recent_by(
            now_host_nanos,
            STUTTER_DIAG_DUMP_WINDOW_NS,
            &mut frames,
            |sample| sample.host_nanos,
        );
        let mut audio_events = Vec::with_capacity(32);
        deadsync_audio_stream::collect_stutter_diag_events(
            now_host_nanos,
            STUTTER_DIAG_DUMP_WINDOW_NS,
            &mut audio_events,
        );
        let mut display_events = Vec::with_capacity(32);
        if let Some(gameplay_state) = self.state.screens.gameplay_state.as_ref() {
            gameplay_state.collect_display_clock_stutter_diag_events(
                now_host_nanos,
                STUTTER_DIAG_DUMP_WINDOW_NS,
                &mut display_events,
            );
        }
        trace!(
            "Stutter recorder dump t={:.3}s screen={:?} reason=[stutter:{} audio:{} display:{}] window_ms={:.1} frames={} audio_events={} display_events={}",
            total_elapsed,
            screen,
            stutter_severity,
            u8::from(audio_triggered),
            u8::from(display_triggered),
            STUTTER_DIAG_DUMP_WINDOW_NS as f64 / 1_000_000.0,
            frames.len(),
            audio_events.len(),
            display_events.len(),
        );
        for sample in frames {
            let age_ms = now_host_nanos.saturating_sub(sample.host_nanos) as f64 / 1_000_000.0;
            let multiple = if sample.expected_us > 0 {
                sample.frame_us as f64 / sample.expected_us as f64
            } else {
                0.0
            };
            trace!(
                "Stutter recorder frame age_ms={:.3} screen={:?} dt_ms={:.3} expected_ms={:.3} x{:.2} req={} phases_ms=[pre:{:.3} rq:{:.3} in:{:.3} up:{:.3} comp:{:.3} upload:{:.3} draw:{:.3}] draw_ms=[acq:{:.3} sub:{:.3} present:{:.3} gpu_wait:{:.3} setup:{:.3} prep:{:.3} record:{:.3}] display=[err_ms:{:+.3} catch:{}] present=[mode:{} display:{} host:{} inflight:{} wait:{} back:{} idle:{} subopt:{}]",
                age_ms,
                sample.screen,
                sample.frame_us as f64 / 1000.0,
                sample.expected_us as f64 / 1000.0,
                multiple,
                sample.redraw_request_reason,
                sample.pre_redraw_gap_us as f64 / 1000.0,
                sample.request_to_redraw_us as f64 / 1000.0,
                sample.input_us as f64 / 1000.0,
                sample.update_us as f64 / 1000.0,
                sample.compose_us as f64 / 1000.0,
                sample.upload_us as f64 / 1000.0,
                sample.draw_us as f64 / 1000.0,
                sample.acquire_us as f64 / 1000.0,
                sample.submit_us as f64 / 1000.0,
                sample.present_us as f64 / 1000.0,
                sample.gpu_wait_us as f64 / 1000.0,
                sample.draw_setup_us as f64 / 1000.0,
                sample.draw_prepare_us as f64 / 1000.0,
                sample.draw_record_us as f64 / 1000.0,
                sample.display_error_us as f64 / 1000.0,
                u8::from(sample.display_catching_up),
                sample.present_mode,
                sample.present_display_clock,
                sample.present_host_clock,
                sample.in_flight_images,
                u8::from(sample.waited_for_image),
                u8::from(sample.applied_back_pressure),
                u8::from(sample.queue_idle_waited),
                u8::from(sample.suboptimal),
            );
        }
        for event in display_events {
            let age_ms = now_host_nanos.saturating_sub(event.at_host_nanos) as f64 / 1_000_000.0;
            trace!(
                "Stutter recorder display age_ms={:.3} kind={} target_ms={:.3} prev_ms={:.3} curr_ms={:.3} err_ms={:+.3} step_ms={:+.3} limit_ms={:.3}",
                age_ms,
                event.kind,
                event.target_time_sec as f64 * 1000.0,
                event.previous_time_sec as f64 * 1000.0,
                event.current_time_sec as f64 * 1000.0,
                event.error_seconds as f64 * 1000.0,
                event.step_seconds as f64 * 1000.0,
                event.limit_seconds as f64 * 1000.0,
            );
        }
        for event in audio_events {
            let age_ms = now_host_nanos.saturating_sub(event.at_host_nanos) as f64 / 1_000_000.0;
            trace!(
                "Stutter recorder audio age_ms={:.3} kind={} value_ms={:.3} rate={} buf={} pad={} q={} period_ms={:.3} out_ms={:.3} qual={}",
                age_ms,
                event.kind,
                event.value_ns as f64 / 1_000_000.0,
                event.sample_rate_hz,
                event.buffer_frames,
                event.padding_frames,
                event.queued_frames,
                event.device_period_ns as f64 / 1_000_000.0,
                event.estimated_output_delay_ns as f64 / 1_000_000.0,
                event.timing_quality,
            );
        }
    }

    fn trace_stutter_diag_dump_if_needed(
        &mut self,
        now_host_nanos: u64,
        total_elapsed: f32,
        screen: CurrentScreen,
        frame_seconds: f32,
    ) {
        if !stutter_diag_enabled() {
            return;
        }
        if now_host_nanos == 0 {
            return;
        }
        let expected = self.expected_frame_seconds_for_stutter();
        let stutter_severity = stutter_severity(frame_seconds, expected);
        let audio_trigger_seq = deadsync_audio_stream::stutter_diag_trigger_seq();
        let display_trigger_seq = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|gs| gs.display_clock_stutter_diag_trigger_seq())
            .unwrap_or(0);
        let audio_triggered =
            audio_trigger_seq > self.state.shell.stutter_diag_last_audio_trigger_seq;
        let display_triggered =
            display_trigger_seq > self.state.shell.stutter_diag_last_display_trigger_seq;
        if stutter_severity == 0 && !audio_triggered && !display_triggered {
            return;
        }
        if now_host_nanos.saturating_sub(self.state.shell.stutter_diag_last_dump_host_nanos)
            < STUTTER_DIAG_MIN_DUMP_GAP_NS
        {
            return;
        }
        self.dump_stutter_diag_window(
            now_host_nanos,
            total_elapsed,
            screen,
            stutter_severity,
            audio_triggered,
            display_triggered,
        );
        self.state.shell.stutter_diag_last_audio_trigger_seq = audio_trigger_seq;
        self.state.shell.stutter_diag_last_display_trigger_seq = display_trigger_seq;
        self.state.shell.stutter_diag_last_dump_host_nanos = now_host_nanos;
    }

    #[inline(always)]
    fn trace_frame_stutter_if_needed(
        &self,
        frame_seconds: f32,
        total_elapsed: f32,
        screen: CurrentScreen,
        pre_redraw_gap_us: u32,
        request_to_redraw_us: u32,
        redraw_request_reason: &'static str,
        input_us: u32,
        update_us: u32,
        compose_us: u32,
        upload_us: u32,
        draw_us: u32,
        actors: &[deadlib_present::actors::Actor],
        draw_stats: renderer::DrawStats,
        compose_breakdown: ComposeBreakdown,
    ) {
        if !log::log_enabled!(log::Level::Trace) {
            return;
        }
        let expected = self.expected_frame_seconds_for_stutter();
        let severity = stutter_severity(frame_seconds, expected);
        if severity == 0 {
            return;
        }
        let frame_us_f = (frame_seconds * 1_000_000.0).max(0.0);
        let frame_us = if frame_us_f > u32::MAX as f32 {
            u32::MAX
        } else {
            frame_us_f as u32
        };
        let frame_work_us = input_us
            .saturating_add(update_us)
            .saturating_add(compose_us)
            .saturating_add(upload_us)
            .saturating_add(draw_us);
        let accounted_us = pre_redraw_gap_us.saturating_add(frame_work_us);
        let unaccounted_gap_us = frame_us.saturating_sub(accounted_us);
        let draw_split_us = draw_stats
            .acquire_us
            .saturating_add(draw_stats.submit_us)
            .saturating_add(draw_stats.present_us)
            .saturating_add(draw_stats.gpu_wait_us)
            .saturating_add(draw_stats.backend_setup_us)
            .saturating_add(draw_stats.backend_prepare_us)
            .saturating_add(draw_stats.backend_record_us);
        let draw_other_us = draw_us.saturating_sub(draw_split_us);
        let present_stats = draw_stats.present_stats;
        let redraw_late_us = pre_redraw_gap_us.saturating_sub(request_to_redraw_us);
        let display_clock = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|gs| gs.display_clock_health())
            .unwrap_or_default();
        let display_error_ms = display_clock.error_seconds * 1000.0;
        let mut dominant = ("redraw_delivery", request_to_redraw_us);
        let candidates = [
            ("input", input_us),
            ("update", update_us),
            ("compose", compose_us),
            ("upload", upload_us),
            ("present", draw_stats.present_us),
            ("gpu_wait", draw_stats.gpu_wait_us),
            ("draw_setup", draw_stats.backend_setup_us),
            ("draw_prepare", draw_stats.backend_prepare_us),
            ("draw_record", draw_stats.backend_record_us),
            ("draw_other", draw_other_us),
            ("unaccounted", unaccounted_gap_us),
            ("redrive_late", redraw_late_us),
        ];
        for (label, value) in candidates {
            if value > dominant.1 {
                dominant = (label, value);
            }
        }
        let multiple = if expected > 0.0 {
            frame_seconds / expected
        } else {
            0.0
        };
        let actor_stats = actor_tree_stats(actors);
        let audio_stats = deadsync_audio_stream::get_output_timing_snapshot();
        log::trace!(
            "Frame stutter t={:.3}s sev={} screen={:?} dt={:.3}ms expected={:.3}ms x{:.2} req={} dom={} dom_ms={:.3} phases_ms=[pre_redraw:{:.3} input:{:.3} update:{:.3} compose:{:.3} upload:{:.3} draw:{:.3} unaccounted:{:.3}] compose_dbg=[actors:{:.3} build:{:.3} resolve:{:.3} nodes:{} sprites:{} text:{} chars:{} frames:{} mesh:{} tmesh:{} cameras:{} shadows:{} objects:{} render_cameras:{} txt_hits:{} txt_shared:{} txt_miss:{} txt_lines:{} txt_glyphs:{} txt_entries:{} txt_aliases:{}] redraw_ms=[redrive_late:{:.3} request_to_redraw:{:.3}] draw_sub_ms=[acquire:{:.3} submit:{:.3} present:{:.3} gpu_wait:{:.3} other:{:.3}] draw_cpu_ms=[setup:{:.3} prep:{:.3} record:{:.3}] display_dbg=[active:{} err_ms:{:+.3} catch:{}] present_dbg=[mode:{} display:{} host:{} mapped:{} inflight:{} image_wait:{} back_pressure:{} queue_idle:{} subopt:{} submit_id:{} done_id:{} refresh_ms:{:.3} interval_ms:{:.3} margin_ms:{:.3} cal_ms:{:.3}] audio_dbg=[path:{} req:{} fallback:{} clock:{} qual:{} sf:{} cf:{} rate:{} buf:{} pad:{} q:{} tick_ms:{:.3} span_ms:{:.3} out_ms:{:.3} underruns:{}]",
            total_elapsed,
            severity,
            screen,
            frame_seconds * 1000.0,
            expected * 1000.0,
            multiple,
            redraw_request_reason,
            dominant.0,
            dominant.1 as f32 / 1000.0,
            pre_redraw_gap_us as f32 / 1000.0,
            input_us as f32 / 1000.0,
            update_us as f32 / 1000.0,
            compose_us as f32 / 1000.0,
            upload_us as f32 / 1000.0,
            draw_us as f32 / 1000.0,
            unaccounted_gap_us as f32 / 1000.0,
            compose_breakdown.actor_build_us as f32 / 1000.0,
            compose_breakdown.build_screen_us as f32 / 1000.0,
            compose_breakdown.resolve_textures_us as f32 / 1000.0,
            actor_stats.total,
            actor_stats.sprites,
            actor_stats.texts,
            actor_stats.text_chars,
            actor_stats.frames,
            actor_stats.meshes,
            actor_stats.textured_meshes,
            actor_stats.cameras,
            actor_stats.shadows,
            compose_breakdown.render_objects,
            compose_breakdown.render_cameras,
            compose_breakdown.text_layout.owned_hits,
            compose_breakdown.text_layout.shared_hits,
            compose_breakdown.text_layout.misses,
            compose_breakdown.text_layout.built_lines,
            compose_breakdown.text_layout.built_glyphs,
            compose_breakdown.text_layout.owned_entries,
            compose_breakdown.text_layout.shared_aliases,
            redraw_late_us as f32 / 1000.0,
            request_to_redraw_us as f32 / 1000.0,
            draw_stats.acquire_us as f32 / 1000.0,
            draw_stats.submit_us as f32 / 1000.0,
            draw_stats.present_us as f32 / 1000.0,
            draw_stats.gpu_wait_us as f32 / 1000.0,
            draw_other_us as f32 / 1000.0,
            draw_stats.backend_setup_us as f32 / 1000.0,
            draw_stats.backend_prepare_us as f32 / 1000.0,
            draw_stats.backend_record_us as f32 / 1000.0,
            u8::from(screen == CurrentScreen::Gameplay),
            display_error_ms,
            u8::from(display_clock.catching_up),
            present_stats.mode,
            present_stats.display_clock,
            present_stats.host_clock,
            present_stats.host_present_ns != 0,
            present_stats.in_flight_images,
            present_stats.waited_for_image,
            present_stats.applied_back_pressure,
            present_stats.queue_idle_waited,
            present_stats.suboptimal,
            present_stats.submitted_present_id,
            present_stats.completed_present_id,
            present_stats.refresh_ns as f32 / 1_000_000.0,
            present_stats.actual_interval_ns as f32 / 1_000_000.0,
            present_stats.present_margin_ns as f32 / 1_000_000.0,
            present_stats.calibration_error_ns as f32 / 1_000_000.0,
            audio_stats.backend,
            audio_stats.requested_output_mode.as_str(),
            audio_stats.fallback_from_native,
            audio_stats.timing_clock,
            audio_stats.timing_quality,
            audio_stats.timing_sanity_failure_count,
            audio_stats.clock_fallback_count,
            audio_stats.sample_rate_hz,
            audio_stats.buffer_frames,
            audio_stats.padding_frames,
            audio_stats.queued_frames,
            audio_stats.device_period_ns as f32 / 1_000_000.0,
            audio_stats.stream_latency_ns as f32 / 1_000_000.0,
            audio_stats.estimated_output_delay_ns as f32 / 1_000_000.0,
            audio_stats.underrun_count
        );
    }

    fn trace_gameplay_frame_pacing_if_needed(
        &mut self,
        now: Instant,
        screen: CurrentScreen,
        frame_seconds: f32,
        pre_redraw_gap_us: u32,
        request_to_redraw_us: u32,
        redraw_request_reason: &'static str,
        draw_us: u32,
        draw_stats: renderer::DrawStats,
    ) {
        let trace = &mut self.state.shell.gameplay_pacing_trace;
        if screen != CurrentScreen::Gameplay {
            trace.reset(now);
            return;
        }
        if trace.frames == 0 {
            trace.started_at = now;
        }
        let redraw_late_us = pre_redraw_gap_us.saturating_sub(request_to_redraw_us);
        let dt_us_f = (frame_seconds * 1_000_000.0).max(0.0);
        let dt_us = if dt_us_f > u32::MAX as f32 {
            u32::MAX
        } else {
            dt_us_f as u32
        };
        trace.frames = trace.frames.saturating_add(1);
        if redraw_request_reason == "chain" {
            trace.chain_frames = trace.chain_frames.saturating_add(1);
        } else {
            trace.other_frames = trace.other_frames.saturating_add(1);
        }
        trace.dt_sum_us = trace.dt_sum_us.saturating_add(u64::from(dt_us));
        trace.dt_max_us = trace.dt_max_us.max(dt_us);
        trace.redraw_late_sum_us = trace
            .redraw_late_sum_us
            .saturating_add(u64::from(redraw_late_us));
        trace.redraw_late_max_us = trace.redraw_late_max_us.max(redraw_late_us);
        trace.redraw_delivery_sum_us = trace
            .redraw_delivery_sum_us
            .saturating_add(u64::from(request_to_redraw_us));
        trace.redraw_delivery_max_us = trace.redraw_delivery_max_us.max(request_to_redraw_us);
        trace.redraw_delivery_over_1ms +=
            u32::from(request_to_redraw_us >= GAMEPLAY_REDRAW_DELIVERY_SLOW_US);
        trace.redraw_delivery_over_2ms +=
            u32::from(request_to_redraw_us >= GAMEPLAY_REDRAW_DELIVERY_BAD_US);
        trace.draw_sum_us = trace.draw_sum_us.saturating_add(u64::from(draw_us));
        trace.draw_max_us = trace.draw_max_us.max(draw_us);
        trace.present_sum_us = trace
            .present_sum_us
            .saturating_add(u64::from(draw_stats.present_us));
        trace.present_max_us = trace.present_max_us.max(draw_stats.present_us);
        trace.present_over_1ms += u32::from(draw_stats.present_us >= GAMEPLAY_PRESENT_SLOW_US);
        trace.present_over_3ms += u32::from(draw_stats.present_us >= GAMEPLAY_PRESENT_SPIKE_US);
        trace.draw_setup_sum_us = trace
            .draw_setup_sum_us
            .saturating_add(u64::from(draw_stats.backend_setup_us));
        trace.draw_prepare_sum_us = trace
            .draw_prepare_sum_us
            .saturating_add(u64::from(draw_stats.backend_prepare_us));
        trace.draw_record_sum_us = trace
            .draw_record_sum_us
            .saturating_add(u64::from(draw_stats.backend_record_us));
        let display_clock = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|gs| gs.display_clock_health())
            .unwrap_or_default();
        let display_error_us_i64 =
            (f64::from(display_clock.error_seconds) * 1_000_000.0).round() as i64;
        let display_error_last_us =
            display_error_us_i64.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let display_error_abs_us =
            display_error_us_i64.unsigned_abs().min(u64::from(u32::MAX)) as u32;
        trace.display_error_last_us = display_error_last_us;
        trace.display_error_abs_sum_us = trace
            .display_error_abs_sum_us
            .saturating_add(u64::from(display_error_abs_us));
        trace.display_error_abs_max_us = trace.display_error_abs_max_us.max(display_error_abs_us);
        trace.display_catching_up_frames += u32::from(display_clock.catching_up);
        trace.display_catching_up_last = display_clock.catching_up;
        let present_stats = draw_stats.present_stats;
        trace.present_last_mode = present_stats.mode;
        trace.present_display_clock_last = present_stats.display_clock;
        trace.present_host_clock_last = present_stats.host_clock;
        trace.present_inflight_sum = trace
            .present_inflight_sum
            .saturating_add(u64::from(present_stats.in_flight_images));
        trace.present_inflight_max = trace
            .present_inflight_max
            .max(present_stats.in_flight_images);
        trace.present_image_wait_frames += u32::from(present_stats.waited_for_image);
        trace.present_back_pressure_frames += u32::from(present_stats.applied_back_pressure);
        trace.present_queue_idle_frames += u32::from(present_stats.queue_idle_waited);
        trace.present_suboptimal_frames += u32::from(present_stats.suboptimal);
        trace.present_host_mapped_frames += u32::from(present_stats.host_present_ns != 0);
        trace.present_calibration_error_sum_ns = trace
            .present_calibration_error_sum_ns
            .saturating_add(present_stats.calibration_error_ns);
        trace.present_calibration_error_max_ns = trace
            .present_calibration_error_max_ns
            .max(present_stats.calibration_error_ns);
        if present_stats.actual_interval_ns > 0 {
            trace.present_interval_sum_ns = trace
                .present_interval_sum_ns
                .saturating_add(present_stats.actual_interval_ns);
            trace.present_interval_max_ns = trace
                .present_interval_max_ns
                .max(present_stats.actual_interval_ns);
            trace.present_interval_samples = trace.present_interval_samples.saturating_add(1);
        }
        if present_stats.completed_present_id != 0 {
            trace.present_margin_sum_ns = trace
                .present_margin_sum_ns
                .saturating_add(present_stats.present_margin_ns);
            trace.present_margin_max_ns = trace
                .present_margin_max_ns
                .max(present_stats.present_margin_ns);
            trace.present_margin_samples = trace.present_margin_samples.saturating_add(1);
        }
        if now.duration_since(trace.started_at) < GAMEPLAY_PACING_LOG_INTERVAL {
            return;
        }
        let frames = trace.frames.max(1);
        let ms = |sum_us: u64| sum_us as f64 / frames as f64 / 1000.0;
        let interval_samples = trace.present_interval_samples.max(1);
        let margin_samples = trace.present_margin_samples.max(1);
        let audio_stats = deadsync_audio_stream::get_output_timing_snapshot();
        log::trace!(
            "Gameplay frame pacing: frames={} req=[chain:{} other:{}] dt_ms=[avg:{:.3} max:{:.3}] redraw_ms=[late_avg:{:.3} late_max:{:.3} deliver_avg:{:.3} deliver_max:{:.3} >=1ms:{} >=2ms:{}] draw_ms=[avg:{:.3} max:{:.3}] present_ms=[avg:{:.3} max:{:.3} >=1ms:{} >=3ms:{}] draw_cpu_ms=[setup_avg:{:.3} prep_avg:{:.3} record_avg:{:.3}] display_dbg=[err_last_ms:{:+.3} abs_avg_ms:{:.3} abs_max_ms:{:.3} catch:{} catch_last:{}] present_dbg=[mode:{} display:{} host:{} mapped:{} inflight_avg:{:.2} inflight_max:{} image_wait:{} back_pressure:{} queue_idle:{} subopt:{} interval_ms_avg:{:.3} interval_ms_max:{:.3} margin_ms_avg:{:.3} margin_ms_max:{:.3} cal_ms_avg:{:.3} cal_ms_max:{:.3}] audio_dbg=[path:{} req:{} fallback:{} clock:{} qual:{} sf:{} cf:{} rate:{} buf:{} pad:{} q:{} tick_ms:{:.3} span_ms:{:.3} out_ms:{:.3} underruns:{}]",
            frames,
            trace.chain_frames,
            trace.other_frames,
            ms(trace.dt_sum_us),
            trace.dt_max_us as f64 / 1000.0,
            ms(trace.redraw_late_sum_us),
            trace.redraw_late_max_us as f64 / 1000.0,
            ms(trace.redraw_delivery_sum_us),
            trace.redraw_delivery_max_us as f64 / 1000.0,
            trace.redraw_delivery_over_1ms,
            trace.redraw_delivery_over_2ms,
            ms(trace.draw_sum_us),
            trace.draw_max_us as f64 / 1000.0,
            ms(trace.present_sum_us),
            trace.present_max_us as f64 / 1000.0,
            trace.present_over_1ms,
            trace.present_over_3ms,
            ms(trace.draw_setup_sum_us),
            ms(trace.draw_prepare_sum_us),
            ms(trace.draw_record_sum_us),
            trace.display_error_last_us as f64 / 1000.0,
            trace.display_error_abs_sum_us as f64 / frames as f64 / 1000.0,
            trace.display_error_abs_max_us as f64 / 1000.0,
            trace.display_catching_up_frames,
            u8::from(trace.display_catching_up_last),
            trace.present_last_mode,
            trace.present_display_clock_last,
            trace.present_host_clock_last,
            trace.present_host_mapped_frames,
            trace.present_inflight_sum as f64 / frames as f64,
            trace.present_inflight_max,
            trace.present_image_wait_frames,
            trace.present_back_pressure_frames,
            trace.present_queue_idle_frames,
            trace.present_suboptimal_frames,
            trace.present_interval_sum_ns as f64 / interval_samples as f64 / 1_000_000.0,
            trace.present_interval_max_ns as f64 / 1_000_000.0,
            trace.present_margin_sum_ns as f64 / margin_samples as f64 / 1_000_000.0,
            trace.present_margin_max_ns as f64 / 1_000_000.0,
            trace.present_calibration_error_sum_ns as f64 / frames as f64 / 1_000_000.0,
            trace.present_calibration_error_max_ns as f64 / 1_000_000.0,
            audio_stats.backend,
            audio_stats.requested_output_mode.as_str(),
            audio_stats.fallback_from_native,
            audio_stats.timing_clock,
            audio_stats.timing_quality,
            audio_stats.timing_sanity_failure_count,
            audio_stats.clock_fallback_count,
            audio_stats.sample_rate_hz,
            audio_stats.buffer_frames,
            audio_stats.padding_frames,
            audio_stats.queued_frames,
            audio_stats.device_period_ns as f64 / 1_000_000.0,
            audio_stats.stream_latency_ns as f64 / 1_000_000.0,
            audio_stats.estimated_output_delay_ns as f64 / 1_000_000.0,
            audio_stats.underrun_count
        );
        trace.reset(now);
    }

    #[inline(always)]
    fn update_fps_stats(&mut self, now: Instant) {
        self.state.shell.frame_count += 1;
        let elapsed = now.duration_since(self.state.shell.last_title_update);
        if elapsed.as_secs_f32() >= 1.0 {
            let fps = self.state.shell.frame_count as f32 / elapsed.as_secs_f32();
            self.state.shell.last_fps = fps;
            self.state.shell.last_vpf = self.state.shell.current_frame_vpf;
            self.state.shell.frame_count = 0;
            self.state.shell.last_title_update = now;
        }
    }

    /* -------------------- keyboard: map -> route -------------------- */

    #[inline(always)]
    fn handle_key_text(&mut self, event_loop: &ActiveEventLoop, text: &str) {
        let action = if self.state.screens.current_screen == CurrentScreen::ManageLocalProfiles {
            crate::screens::manage_local_profiles::handle_raw_key_event(
                &mut self.state.screens.manage_local_profiles_state,
                None,
                Some(text),
            )
        } else if self.state.screens.current_screen == CurrentScreen::SelectMusic {
            crate::screens::select_music::handle_raw_key_event(
                &mut self.state.screens.select_music_state,
                None,
                Some(text),
            )
        } else {
            ScreenAction::None
        };
        if matches!(action, ScreenAction::None) {
            return;
        }
        if let Err(e) = self.handle_action(action, event_loop) {
            log::error!("Failed to handle text input action: {e}");
        }
    }

    #[inline(always)]
    fn handle_raw_key_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        raw_key: RawKeyboardEvent,
    ) -> bool {
        use winit::keyboard::KeyCode;

        match raw_key.code {
            KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                self.state.shell.shift_held = raw_key.pressed;
            }
            KeyCode::ControlLeft | KeyCode::ControlRight => {
                self.state.shell.ctrl_held = raw_key.pressed;
            }
            KeyCode::AltLeft | KeyCode::AltRight => {
                self.state.shell.alt_held = raw_key.pressed;
            }
            _ => {}
        }

        if logical_input::with_keymap(|km| {
            km.raw_key_event_has_action(&raw_key, |action| {
                action == VirtualAction::system_fast_forward
            })
        }) {
            self.state.shell.fast_forward_held = raw_key.pressed;
        }
        if logical_input::with_keymap(|km| {
            km.raw_key_event_has_action(&raw_key, |action| {
                action == VirtualAction::system_slow_down
            })
        }) {
            self.state.shell.slow_down_held = raw_key.pressed;
        }

        if raw_key.pressed && raw_key.code == KeyCode::F4 && self.state.shell.alt_held {
            info!("Alt+F4 quit shortcut pressed. Shutting down.");
            event_loop.exit();
            return true;
        }

        if self.state.screens.current_screen == CurrentScreen::Sandbox {
            let action = crate::screens::sandbox::handle_raw_key_event(
                &mut self.state.screens.sandbox_state,
                &raw_key,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle Sandbox raw key action: {e}");
                }
                return true;
            }
        } else if self.state.screens.current_screen == CurrentScreen::Menu {
            let action = crate::screens::menu::handle_raw_key_event(
                &mut self.state.screens.menu_state,
                &raw_key,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle Menu raw key action: {e}");
                }
                return true;
            }
        } else if self.state.screens.current_screen == CurrentScreen::Mappings {
            let action = crate::screens::mappings::handle_raw_key_event(
                &mut self.state.screens.mappings_state,
                &raw_key,
            );
            if !matches!(action, ScreenAction::None)
                && let Err(e) = self.handle_action(action, event_loop)
            {
                log::error!("Failed to handle Mappings raw key action: {e}");
            }
            // On the Mappings screen, arrows/Enter/Escape are handled entirely
            // via raw keycodes; do not route through the virtual keymap.
            return true;
        } else if self.state.screens.current_screen == CurrentScreen::ManageLocalProfiles {
            let action = crate::screens::manage_local_profiles::handle_raw_key_event(
                &mut self.state.screens.manage_local_profiles_state,
                Some(&raw_key),
                None,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle ManageLocalProfiles raw key action: {e}");
                }
                return true;
            }
        } else if self.state.screens.current_screen == CurrentScreen::OverscanAdjustment {
            // The overscan screen owns the W/A/S/D/I/J/K/L adjustment keys so they
            // do not also fire as virtual P1 pad directions. Other keys (arrows,
            // Enter, Escape) fall through to the virtual keymap for menu/pad nav.
            if crate::screens::overscan_adjustment::handle_raw_key_event(
                &mut self.state.screens.overscan_adjustment_state,
                &raw_key,
            ) {
                return true;
            }
        } else if self.state.screens.current_screen == CurrentScreen::Input {
            let action = crate::screens::input::handle_raw_key_event(
                &mut self.state.screens.input_state,
                &raw_key,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle Input raw key action: {e}");
                }
                return true;
            }
        } else if self.state.screens.current_screen == CurrentScreen::SelectMusic {
            // Route screen-specific raw key handling (e.g., F7 fetch) to the screen
            let action = crate::screens::select_music::handle_raw_key_event_with_modifiers(
                &mut self.state.screens.select_music_state,
                Some(&raw_key),
                None,
                self.state.shell.ctrl_held,
                self.state.shell.shift_held,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle SelectMusic raw key action: {e}");
                }
                return true;
            }
        } else if self.state.screens.current_screen == CurrentScreen::Practice {
            if raw_key.pressed
                && !raw_key.repeat
                && raw_key.code == KeyCode::KeyR
                && self.state.shell.ctrl_held
                && self.state.shell.shift_held
                && config::get().keyboard_features
            {
                self.try_practice_reload(event_loop, "Ctrl+Shift+R");
                return true;
            }
            if let Some(ps) = self.state.screens.practice_state.as_mut() {
                let (consumed, action) =
                    crate::screens::practice::handle_raw_key_event(ps, &raw_key);
                if !matches!(action, ScreenAction::None) {
                    if let Err(e) = self.handle_action(action, event_loop) {
                        log::error!("Failed to handle Practice raw key action: {e}");
                    }
                    return true;
                }
                if consumed {
                    return true;
                }
            }
        } else if self.state.screens.current_screen == CurrentScreen::Evaluation {
            crate::screens::evaluation::handle_raw_key_event(
                &mut self.state.screens.evaluation_state,
                &raw_key,
            );
            if App::raw_keyboard_restart_screen(self.state.screens.current_screen)
                && raw_key.pressed
                && !raw_key.repeat
                && raw_key.code == KeyCode::KeyR
                && self.state.shell.ctrl_held
                && config::get().keyboard_features
                && self.state.session.course_run.is_none()
            {
                if self.state.shell.shift_held {
                    self.try_gameplay_reload(event_loop, "Ctrl+Shift+R");
                } else {
                    self.try_gameplay_restart(event_loop, "Ctrl+R");
                }
                return true;
            }
            if raw_key.pressed
                && !raw_key.repeat
                && raw_key.code == KeyCode::KeyP
                && self.state.shell.ctrl_held
                && config::get().keyboard_features
                && self.state.session.course_run.is_none()
                && self.try_practice_from_eval(event_loop, "Ctrl+P")
            {
                return true;
            }
            if raw_key.pressed
                && !raw_key.repeat
                && raw_key.code == KeyCode::F5
                && crate::screens::evaluation::retry_submissions(
                    &self.state.screens.evaluation_state,
                )
            {
                return true;
            }
            if raw_key.pressed && !self.state.session.course_eval_pages.is_empty() {
                match raw_key.code {
                    KeyCode::KeyN => {
                        self.step_course_eval_page(1);
                        return true;
                    }
                    KeyCode::KeyP => {
                        self.step_course_eval_page(-1);
                        return true;
                    }
                    _ => {}
                }
            }
        }
        let accepts_queued_input = input_routing::screen_accepts_queued_input(
            self.state.screens.current_screen,
            &self.state.shell.transition,
        );

        if raw_key.pressed && raw_key.code == KeyCode::F3 {
            if self.state.shell.ctrl_held && self.state.shell.shift_held {
                // Ctrl+Shift+F3: move the frame-stats overlay to the next corner (runtime only).
                if !raw_key.repeat && self.state.shell.frame_stats_overlay_enabled {
                    let (_, two_player, _) = self.frame_stats_play_context();
                    let anchor = self
                        .state
                        .shell
                        .cycle_frame_stats_overlay_anchor(two_player);
                    debug!("Frame stats overlay corner {anchor:?}");
                }
            } else if self.state.shell.ctrl_held && self.state.shell.alt_held {
                // Ctrl+Alt+F3: switch the overlay presentation (detailed ↔ minimal).
                if !raw_key.repeat && self.state.shell.frame_stats_overlay_enabled {
                    let style = self.state.shell.toggle_frame_stats_overlay_style();
                    debug!("Frame stats overlay style {}", style.label());
                }
            } else if self.state.shell.ctrl_held {
                if !raw_key.repeat {
                    let on = self.state.shell.toggle_frame_stats_overlay();
                    // Only auto-place when the user hasn't positioned it themselves; otherwise
                    // restore the remembered corner (persisted across toggles and restarts).
                    if on && !self.state.shell.frame_stats_overlay_anchor_user_set {
                        self.state.shell.frame_stats_overlay_anchor =
                            crate::screens::components::shared::frame_stats_overlay::default_anchor(
                            );
                    }
                    debug!("Frame stats overlay {}", if on { "ON" } else { "OFF" });
                }
            } else {
                let mode = self.state.shell.cycle_overlay_mode();
                debug!("Overlay {}", self.state.shell.overlay_mode.label());
                config::update_show_stats_mode(mode);
                options::sync_show_stats_mode(&mut self.state.screens.options_state, mode);
            }
        }
        if raw_key.pressed && !raw_key.repeat && raw_key.code == KeyCode::F9 {
            let new_value = !config::get().translated_titles;
            config::update_translated_titles(new_value);
            options::sync_translated_titles(&mut self.state.screens.options_state, new_value);
            deadsync_audio_stream::play_sfx("assets/sounds/change.ogg");
        }
        // Screen-specific Escape handling resides in per-screen raw handlers now

        if !accepts_queued_input {
            logical_input::clear_debounce_state();
            self.lights.clear_button_pressed();
            self.clear_gameplay_input_events();
            return true;
        }

        let gameplay_screen = self.state.screens.current_screen == CurrentScreen::Gameplay;
        if gameplay_screen {
            return false;
        }

        let mut input_err: Option<Box<dyn Error>> = None;
        logical_input::map_raw_key_event_with(&raw_key, |ev| {
            if input_err.is_none()
                && let Err(e) = self.route_input_event(event_loop, ev)
            {
                input_err = Some(e);
            }
        });
        if let Some(e) = input_err {
            log::error!("Failed to handle input: {e}");
            event_loop.exit();
            return true;
        }
        false
    }

    #[inline(always)]
    fn handle_live_key_event(&mut self, event_loop: &ActiveEventLoop, raw_key: RawKeyboardEvent) {
        let gameplay_screen = self.state.screens.current_screen == CurrentScreen::Gameplay;
        let handled_started = Instant::now();

        if !self.handle_raw_key_event(event_loop, raw_key) {
            if gameplay_screen {
                let start_screen = self.state.screens.current_screen;
                if let Some(gameplay_ev) = gameplay_raw_key_event(&raw_key)
                    && let Err(e) = self.route_gameplay_event(event_loop, gameplay_ev)
                {
                    log::error!("Failed to handle gameplay raw key: {e}");
                    event_loop.exit();
                    return;
                }
                if !self.gameplay_dispatch_continues(start_screen) {
                    return;
                }
            }

            let mut input_err: Option<Box<dyn Error>> = None;
            let start_screen = self.state.screens.current_screen;
            let mut discard_gameplay_batch = false;
            logical_input::map_raw_key_event_with(&raw_key, |ev| {
                if discard_gameplay_batch || input_err.is_some() {
                    return;
                }
                let result = if gameplay_screen {
                    self.route_gameplay_event(event_loop, GameplayQueuedEvent::Input(ev))
                } else {
                    self.route_input_event(event_loop, ev)
                };
                if let Err(e) = result {
                    input_err = Some(e);
                    return;
                }
                if gameplay_screen && !self.gameplay_dispatch_continues(start_screen) {
                    discard_gameplay_batch = true;
                }
            });
            if let Some(e) = input_err {
                log::error!("Failed to handle input: {e}");
                event_loop.exit();
                return;
            }
        }

        self.state.shell.note_gameplay_key_handler(
            gameplay_screen,
            raw_key.repeat,
            elapsed_us_since(handled_started),
        );
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    #[inline(always)]
    fn handle_unix_window_keyboard_fallback(
        &mut self,
        event_loop: &ActiveEventLoop,
        key_event: &winit::event::KeyEvent,
    ) {
        use winit::event::ElementState;
        use winit::keyboard::PhysicalKey;

        if deadsync_input_native::unix_raw_keyboard_backend_active() || !self.accepts_live_input() {
            return;
        }
        let PhysicalKey::Code(code) = key_event.physical_key else {
            return;
        };
        self.handle_live_key_event(
            event_loop,
            RawKeyboardEvent {
                code,
                pressed: key_event.state == ElementState::Pressed,
                repeat: key_event.repeat,
                timestamp: Instant::now(),
                host_nanos: host_time::now_nanos(),
            },
        );
    }

    /* -------------------- pad event routing -------------------- */

    #[inline(always)]
    fn handle_pad_event(&mut self, event_loop: &ActiveEventLoop, ev: PadEvent) {
        // Press-feedback gif: on any SMX panel press/release outside gameplay, play
        // the pack's `press` animation on that panel's low-priority layer. Gated on
        // smx_panel_lights; gated on non-gameplay so the judgement/sustain layers
        // (which are higher priority) own gameplay fully.
        let cfg = config::get();
        if cfg.smx_input && cfg.smx_panel_lights {
            if let PadEvent::RawButton {
                id, code, pressed, ..
            } = ev
            {
                let pad_slot = id.0 as usize;
                let is_gameplay = matches!(
                    self.state.screens.current_screen,
                    CurrentScreen::Gameplay | CurrentScreen::Practice
                );
                // During gameplay, only fire press feedback on the blacked-out (unused) pad.
                // Outside gameplay, fire on all pads.
                let is_blacked_out = self
                    .smx_blackout_synced
                    .get(pad_slot)
                    .copied()
                    .unwrap_or(false);
                if (!is_gameplay || is_blacked_out) && code.0 as usize != deadsync_smx::CENTER_PANEL
                {
                    self.smx_panels
                        .on_raw_panel(pad_slot, code.0 as usize, pressed);
                }
            }
        }

        if !input_routing::screen_accepts_queued_input(
            self.state.screens.current_screen,
            &self.state.shell.transition,
        ) {
            logical_input::clear_debounce_state();
            self.lights.clear_button_pressed();
            self.clear_gameplay_input_events();
            return;
        }
        let gameplay_screen = self.state.screens.current_screen == CurrentScreen::Gameplay;
        let mut input_err: Option<Box<dyn Error>> = None;
        let start_screen = self.state.screens.current_screen;
        let mut discard_gameplay_batch = false;
        logical_input::map_pad_event_with(&ev, |iev| {
            if discard_gameplay_batch || input_err.is_some() {
                return;
            }
            let result = if gameplay_screen {
                self.route_gameplay_event(event_loop, GameplayQueuedEvent::Input(iev))
            } else {
                self.route_input_event(event_loop, iev)
            };
            if let Err(e) = result {
                input_err = Some(e);
                return;
            }
            if gameplay_screen && !self.gameplay_dispatch_continues(start_screen) {
                discard_gameplay_batch = true;
            }
        });
        if let Some(e) = input_err {
            error!("Failed to handle pad input: {e}");
            event_loop.exit();
        }
    }

    // legacy virtual-action dispatcher removed; screens own their input

    #[cfg(any())]
    #[inline(always)]
    fn poll_gamepad_and_dispatch(&mut self, _event_loop: &ActiveEventLoop) {}

    fn handle_audio_and_profile_on_fade(
        &mut self,
        prev: CurrentScreen,
        target: CurrentScreen,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        let menu_music_enabled = config::get().menu_music;
        let target_menu_music = menu_music_enabled
            && matches!(
                target,
                CurrentScreen::SelectColor
                    | CurrentScreen::SelectStyle
                    | CurrentScreen::SelectPlayMode
            );
        let prev_menu_music = menu_music_enabled
            && matches!(
                prev,
                CurrentScreen::SelectColor
                    | CurrentScreen::SelectStyle
                    | CurrentScreen::SelectPlayMode
            );
        let target_course_music = target == CurrentScreen::SelectCourse;
        let prev_course_music = prev == CurrentScreen::SelectCourse;
        let target_credits_music = target == CurrentScreen::Credits;
        let prev_credits_music = prev == CurrentScreen::Credits;
        let target_srpg10_gameover_music =
            target == CurrentScreen::GameOver && visual_styles::srpg10_active();
        let prev_srpg10_gameover_music =
            prev == CurrentScreen::GameOver && visual_styles::srpg10_active();
        let keep_preview = (prev == CurrentScreen::SelectMusic
            && target == CurrentScreen::PlayerOptions)
            || (prev == CurrentScreen::PlayerOptions && target == CurrentScreen::SelectMusic);

        if prev == CurrentScreen::Evaluation && target != CurrentScreen::Evaluation {
            deadsync_audio_stream::stop_screen_sfx();
        }

        if target_menu_music {
            if !prev_menu_music {
                commands.push(Command::PlayMusic {
                    path: visual_styles::menu_music_resolved_path(),
                    looped: true,
                    volume: 1.0,
                });
            }
        } else if target_course_music {
            if !prev_course_music {
                commands.push(Command::PlayMusic {
                    path: dirs::app_dirs()
                        .resolve_asset_path("assets/music/select_course (loop).ogg"),
                    looped: true,
                    volume: 1.0,
                });
            }
        } else if target_credits_music {
            if !prev_credits_music {
                commands.push(Command::PlayMusic {
                    path: dirs::app_dirs().resolve_asset_path("assets/music/credits.ogg"),
                    looped: true,
                    volume: 1.0,
                });
            }
        } else if target_srpg10_gameover_music {
            if !prev_srpg10_gameover_music {
                commands.push(Command::PlayMusic {
                    path: visual_styles::srpg10_gameover_music_path(),
                    looped: false,
                    volume: 1.0,
                });
            }
        } else if (prev_menu_music || prev_course_music || prev_credits_music)
            && target != CurrentScreen::Gameplay
        {
            commands.push(Command::StopMusic);
        } else if target != CurrentScreen::Gameplay && !keep_preview {
            commands.push(Command::StopMusic);
        }

        if matches!(prev, CurrentScreen::Gameplay | CurrentScreen::Practice)
            && !matches!(target, CurrentScreen::Gameplay | CurrentScreen::Practice)
        {
            if !target_menu_music
                && !target_course_music
                && !target_credits_music
                && !target_srpg10_gameover_music
            {
                commands.push(Command::StopMusic);
            }
            if let Some(backend) = self.backend.as_mut() {
                self.dynamic_media
                    .set_background(&mut self.asset_manager, backend, None, 0.0);
            }
        }

        if prev == CurrentScreen::SelectMusic || prev == CurrentScreen::PlayerOptions {
            if prev == CurrentScreen::PlayerOptions
                && let Some(po_state) = &self.state.screens.player_options_state
            {
                let play_style = profile::get_session_play_style();
                let player_side = profile::get_session_player_side();
                let update_scroll_speed =
                    |commands: &mut Vec<Command>,
                     side: profile_data::PlayerSide,
                     speed_mod: &player_options::SpeedMod| {
                        let setting = match speed_mod.mod_type {
                            player_options::SpeedModType::C => {
                                ScrollSpeedSetting::CMod(speed_mod.value)
                            }
                            player_options::SpeedModType::X => {
                                ScrollSpeedSetting::XMod(speed_mod.value)
                            }
                            player_options::SpeedModType::M => {
                                ScrollSpeedSetting::MMod(speed_mod.value)
                            }
                        };

                        commands.push(Command::UpdateScrollSpeed { side, setting });
                        debug!("Saved scroll speed ({side:?}): {setting}");
                    };

                match play_style {
                    profile_data::PlayStyle::Versus => {
                        update_scroll_speed(
                            &mut commands,
                            profile_data::PlayerSide::P1,
                            &po_state.speed_mod[0],
                        );
                        update_scroll_speed(
                            &mut commands,
                            profile_data::PlayerSide::P2,
                            &po_state.speed_mod[1],
                        );
                    }
                    profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
                        let persisted_idx = profile_data::player_side_index(player_side);
                        update_scroll_speed(
                            &mut commands,
                            player_side,
                            &po_state.speed_mod[persisted_idx],
                        );
                    }
                }

                commands.push(Command::UpdateSessionMusicRate(po_state.music_rate));
                debug!("Session music rate set to {:.2}x", po_state.music_rate);

                let preferred_idx = match play_style {
                    profile_data::PlayStyle::Versus => po_state.chart_difficulty_index[0],
                    profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
                        let persisted_idx = profile_data::player_side_index(player_side);
                        po_state.chart_difficulty_index[persisted_idx]
                    }
                };
                self.state.session.preferred_difficulty_index = preferred_idx;
                commands.push(Command::UpdatePreferredDifficulty(preferred_idx));
                debug!(
                    "Updated preferred difficulty index to {} from PlayerOptions",
                    self.state.session.preferred_difficulty_index
                );
            }

            if !(target == CurrentScreen::SelectMusic
                || target == CurrentScreen::PlayerOptions
                || target == CurrentScreen::Gameplay
                || target == CurrentScreen::SelectCourse)
            {
                commands.push(Command::StopMusic);
            }
        }

        if prev == CurrentScreen::SelectMusic {
            self.state.session.preferred_difficulty_index = self
                .state
                .screens
                .select_music_state
                .preferred_difficulty_index;
        }
        commands
    }

    /// Begin a new play session: start the session timer, clear per-session state,
    /// and drop the SMX managed-config resolve signatures so each connected pad's
    /// default is reasserted for the new session. (A manual Apply from a prior
    /// session writes the pad + marker but not the resolve signature, so without
    /// the reset the override would persist into the next session — unlike a full
    /// app restart, which always resolves fresh.) No-op if a session is active.
    fn begin_play_session(&mut self) {
        if self.state.session.session_start_time.is_some() {
            return;
        }
        self.state.session.session_start_time = Some(Instant::now());
        self.state.session.played_stages.clear();
        self.state.session.course_individual_stage_indices.clear();
        self.pad_config_sync.reset_signatures();
        debug!("Session timer started.");
    }

    fn sync_profile_load_state(
        &mut self,
        profiles: &[profile_data::Profile; profile_data::PLAYER_SLOTS],
    ) {
        self.state.session.combo_carry = profile::combo_carry_for_profiles(profiles);
        let play_style = profile::get_session_play_style();
        let active_side = profile::get_session_player_side();
        let active_ix = profile_data::player_side_index(active_side);
        self.state.session.preferred_difficulty_index =
            profile::preferred_difficulty_for_profile(&profiles[active_ix], play_style);

        if let Some(backend) = self.backend.as_mut() {
            self.dynamic_media.set_profile_avatar_for_side(
                &mut self.asset_manager,
                backend,
                profile_data::PlayerSide::P1,
                profiles[0].avatar_path.clone(),
            );
            self.dynamic_media.set_profile_avatar_for_side(
                &mut self.asset_manager,
                backend,
                profile_data::PlayerSide::P2,
                profiles[1].avatar_path.clone(),
            );
        }
    }

    fn handle_screen_state_on_fade(&mut self, prev: CurrentScreen, target: CurrentScreen) {
        if prev == CurrentScreen::SelectColor {
            let idx = self.state.screens.select_color_state.active_color_index;
            self.sync_screen_color_index(idx);
        } else if prev == CurrentScreen::Options {
            let idx = self.state.screens.options_state.active_color_index;
            self.sync_screen_color_index(idx);
        }

        if target == CurrentScreen::Menu {
            self.state.session.session_start_time = None;
            self.state.session.played_stages.clear();
            self.state.session.course_individual_stage_indices.clear();
            self.state.session.combo_carry = profile::combo_carry();
            self.clear_course_runtime();
            self.state.session.last_course_wheel_path = None;
            self.state.session.last_course_wheel_difficulty_name = None;
            let current_color_index = self.state.screens.menu_state.active_color_index;
            self.state.screens.menu_state = menu::init();
            self.state.screens.menu_state.active_color_index = current_color_index;
        } else if target == CurrentScreen::Options {
            self.reset_options_state_for_entry(prev);
        } else if target == CurrentScreen::Credits {
            self.state.screens.credits_state = credits::init();
            self.state.screens.credits_state.active_color_index =
                self.state.screens.options_state.active_color_index;
        } else if target == CurrentScreen::ManageLocalProfiles {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.manage_local_profiles_state = manage_local_profiles::init();
            self.state
                .screens
                .manage_local_profiles_state
                .active_color_index = color_index;
        } else if target == CurrentScreen::Mappings {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.mappings_state = mappings::init();
            self.state.screens.mappings_state.active_color_index = color_index;
        } else if target == CurrentScreen::TestLights {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.test_lights_state = test_lights::init();
            self.state.screens.test_lights_state.active_color_index = color_index;
            test_lights::on_enter(&mut self.state.screens.test_lights_state);
            self.lights.set_test_auto_cycle();
        } else if target == CurrentScreen::OverscanAdjustment {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.overscan_adjustment_state = overscan_adjustment::init();
            self.state
                .screens
                .overscan_adjustment_state
                .active_color_index = color_index;
            overscan_adjustment::on_enter(&mut self.state.screens.overscan_adjustment_state);
        } else if target == CurrentScreen::SmxAssignPads {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.smx_assign_state = crate::screens::smx_assign::init();
            self.state.screens.smx_assign_state.active_color_index = color_index;
            crate::screens::smx_assign::on_enter(&mut self.state.screens.smx_assign_state);
        } else if target == CurrentScreen::SelectProfile {
            let current_color_index = self.state.screens.select_profile_state.active_color_index;
            self.state.screens.select_profile_state = select_profile::init();
            self.state.screens.select_profile_state.active_color_index = current_color_index;
            if prev == CurrentScreen::Menu {
                let p2 = self.state.screens.menu_state.started_by_p2;
                select_profile::set_joined(&mut self.state.screens.select_profile_state, !p2, p2);
                profile::set_fast_profile_switch_from_select_music(false);
            } else if prev == CurrentScreen::SelectMusic {
                let p1_joined = profile::is_session_side_joined(profile_data::PlayerSide::P1);
                let p2_joined = profile::is_session_side_joined(profile_data::PlayerSide::P2);
                select_profile::set_joined(
                    &mut self.state.screens.select_profile_state,
                    p1_joined,
                    p2_joined,
                );
            } else {
                profile::set_fast_profile_switch_from_select_music(false);
            }
        } else if target == CurrentScreen::SelectStyle {
            let current_color_index = self.state.screens.select_style_state.active_color_index;
            self.state.screens.select_style_state = select_style::init();
            self.state.screens.select_style_state.active_color_index = current_color_index;
            let p1_joined = profile::is_session_side_joined(profile_data::PlayerSide::P1);
            let p2_joined = profile::is_session_side_joined(profile_data::PlayerSide::P2);
            self.state.screens.select_style_state.selected_index = if p1_joined && p2_joined {
                1 // "2 Players"
            } else {
                0 // "1 Player"
            };
        } else if target == CurrentScreen::SelectPlayMode {
            let current_color_index = match prev {
                CurrentScreen::SelectStyle => {
                    self.state.screens.select_style_state.active_color_index
                }
                CurrentScreen::SelectColor => {
                    self.state.screens.select_color_state.active_color_index
                }
                CurrentScreen::SelectProfile => {
                    self.state.screens.select_profile_state.active_color_index
                }
                CurrentScreen::Menu => self.state.screens.menu_state.active_color_index,
                _ => self.state.screens.select_play_mode_state.active_color_index,
            };
            self.state.screens.select_play_mode_state = select_mode::init();
            self.state.screens.select_play_mode_state.active_color_index = current_color_index;
            select_mode::on_enter(&mut self.state.screens.select_play_mode_state);
        } else if target == CurrentScreen::ProfileLoad {
            let current_color_index = match prev {
                CurrentScreen::SelectPlayMode => {
                    self.state.screens.select_play_mode_state.active_color_index
                }
                CurrentScreen::SelectStyle => {
                    self.state.screens.select_style_state.active_color_index
                }
                CurrentScreen::SelectColor => {
                    self.state.screens.select_color_state.active_color_index
                }
                CurrentScreen::SelectProfile => {
                    self.state.screens.select_profile_state.active_color_index
                }
                CurrentScreen::Menu => self.state.screens.menu_state.active_color_index,
                _ => self.state.screens.profile_load_state.active_color_index,
            };
            self.state.screens.profile_load_state = profile_load::init();
            self.state.screens.profile_load_state.active_color_index = current_color_index;
            let profiles = profile::load_default_profiles_for_joined_sides();
            self.sync_profile_load_state(&profiles);
            profile_load::on_enter(&mut self.state.screens.profile_load_state);
        } else if target == CurrentScreen::PlayerOptions {
            if prev == CurrentScreen::SelectCourse {
                if !self.start_course_run_from_selected() {
                    self.state.screens.player_options_state = None;
                    return;
                }
                let color_index = self.state.screens.select_course_state.active_color_index;
                if !self.prepare_player_options_for_course_stage(color_index) {
                    self.state.screens.player_options_state = None;
                    warn!("Unable to prepare PlayerOptions for the selected course.");
                }
            } else {
                let (song_arc, chart_steps_index, preferred_difficulty_index) = {
                    let sm_state = &self.state.screens.select_music_state;
                    let entry = sm_state.entries.get(sm_state.selected_index).unwrap();
                    let song = match entry {
                        select_music::MusicWheelEntry::Song(s) => s,
                        _ => panic!("Cannot open player options on a pack header"),
                    };
                    let play_style = profile::get_session_play_style();
                    let (steps, pref) = match play_style {
                        profile_data::PlayStyle::Versus => (
                            [
                                sm_state.selected_steps_index,
                                sm_state.p2_selected_steps_index,
                            ],
                            [
                                sm_state.preferred_difficulty_index,
                                sm_state.p2_preferred_difficulty_index,
                            ],
                        ),
                        profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => (
                            [sm_state.selected_steps_index; 2],
                            [sm_state.preferred_difficulty_index; 2],
                        ),
                    };
                    (song.clone(), steps, pref)
                };

                let color_index = self.state.screens.select_music_state.active_color_index;
                let return_screen = if prev == CurrentScreen::Practice {
                    CurrentScreen::Practice
                } else {
                    CurrentScreen::SelectMusic
                };
                self.state.screens.player_options_state = Some(player_options::init(
                    song_arc,
                    chart_steps_index,
                    preferred_difficulty_index,
                    color_index,
                    return_screen,
                    None,
                ));
            }
        } else if target == CurrentScreen::Gameplay && prev == CurrentScreen::Gameplay {
            if self.state.session.course_run.is_some() {
                let color_index = self.state.screens.gameplay_state.as_ref().map_or(
                    self.state.screens.select_course_state.active_color_index,
                    |gs| gs.gameplay.active_color_index(),
                );
                if !self.prepare_player_options_for_course_stage(color_index) {
                    self.state.screens.player_options_state = None;
                    warn!("Unable to prepare gameplay for the next course stage.");
                }
            }
        } else if matches!(target, CurrentScreen::Gameplay | CurrentScreen::Practice)
            && (prev == CurrentScreen::SelectMusic
                || (target == CurrentScreen::Gameplay && prev == CurrentScreen::SelectCourse))
            && self.state.screens.player_options_state.is_none()
        {
            // Allow starting Gameplay directly from SelectMusic (Simply Love behavior) by
            // constructing a PlayerOptions state from persisted profile/session defaults.
            if prev == CurrentScreen::SelectCourse {
                if !self.start_course_run_from_selected() {
                    warn!("Unable to start gameplay: selected course has no playable stages.");
                    return;
                }
                let color_index = self.state.screens.select_course_state.active_color_index;
                if !self.prepare_player_options_for_course_stage(color_index) {
                    warn!("Unable to prepare gameplay for the selected course stage.");
                }
            } else {
                let (song_arc, chart_steps_index, preferred_difficulty_index) = {
                    let sm_state = &self.state.screens.select_music_state;
                    let entry = sm_state.entries.get(sm_state.selected_index).unwrap();
                    let song = match entry {
                        select_music::MusicWheelEntry::Song(s) => s,
                        _ => panic!("Cannot start gameplay or practice on a pack header"),
                    };
                    let play_style = profile::get_session_play_style();
                    let (steps, pref) = match play_style {
                        profile_data::PlayStyle::Versus => (
                            [
                                sm_state.selected_steps_index,
                                sm_state.p2_selected_steps_index,
                            ],
                            [
                                sm_state.preferred_difficulty_index,
                                sm_state.p2_preferred_difficulty_index,
                            ],
                        ),
                        profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => (
                            [sm_state.selected_steps_index; 2],
                            [sm_state.preferred_difficulty_index; 2],
                        ),
                    };
                    (song.clone(), steps, pref)
                };
                let color_index = self.state.screens.select_music_state.active_color_index;
                self.state.screens.player_options_state = Some(player_options::init(
                    song_arc,
                    chart_steps_index,
                    preferred_difficulty_index,
                    color_index,
                    CurrentScreen::SelectMusic,
                    None,
                ));
            }
        }
    }

    fn sync_screen_color_index(&mut self, idx: i32) {
        self.state.screens.menu_state.active_color_index = idx;
        self.state.screens.select_profile_state.active_color_index = idx;
        self.state.screens.select_style_state.active_color_index = idx;
        self.state.screens.select_play_mode_state.active_color_index = idx;
        self.state.screens.profile_load_state.active_color_index = idx;
        self.state.screens.select_music_state.active_color_index = idx;
        self.state.screens.select_course_state.active_color_index = idx;
        self.state.screens.options_state.active_color_index = idx;
        self.state.screens.credits_state.active_color_index = idx;
        self.state
            .screens
            .manage_local_profiles_state
            .active_color_index = idx;
        self.state.screens.input_state.active_color_index = idx;
        self.state.screens.pad_config_state.active_color_index = idx;
        self.state.screens.test_lights_state.active_color_index = idx;
        self.state
            .screens
            .evaluation_summary_state
            .active_color_index = idx;
        self.state.screens.initials_state.active_color_index = idx;
        self.state.screens.gameover_state.active_color_index = idx;
        if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
            gs.set_color_indices(idx, idx);
        }
    }

    fn handle_screen_entry_on_fade(
        &mut self,
        prev: CurrentScreen,
        target: CurrentScreen,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        if matches!(prev, CurrentScreen::Gameplay | CurrentScreen::Practice)
            && !matches!(target, CurrentScreen::Gameplay | CurrentScreen::Practice)
            && target != CurrentScreen::Evaluation
            && let Some(backend) = self.backend.as_mut()
        {
            self.dynamic_media
                .clear_gameplay_backgrounds(&mut self.asset_manager, backend);
        }
        if target == CurrentScreen::Practice {
            deadsync_audio_stream::stop_music();
            if let Some(mut po_state) = self.state.screens.player_options_state.take() {
                // Preserve the editor cursor/selection across a returning
                // PlayerOptions->Practice trip and across an in-place Practice
                // chart reload (Ctrl+Shift+R re-enters Practice from Practice).
                let edit_snapshot = ((prev == CurrentScreen::PlayerOptions
                    && po_state.return_screen == CurrentScreen::Practice)
                    || prev == CurrentScreen::Practice)
                    .then(|| {
                        self.state
                            .screens
                            .practice_state
                            .as_ref()
                            .map(practice::edit_snapshot)
                    })
                    .flatten();
                let song_arc = po_state.song.clone();
                let play_style = profile::get_session_play_style();
                let player_side = profile::get_session_player_side();
                let target_chart_type = play_style.chart_type();
                let mut resolved_steps_index = po_state.chart_steps_index;
                let mut resolve_chart = |slot: usize| {
                    let requested_idx = resolved_steps_index[slot];
                    if let Some(chart_ref) =
                        song_arc.chart_for_steps_index(target_chart_type, requested_idx)
                    {
                        return chart_ref;
                    }

                    let preferred_idx = po_state.chart_difficulty_index[slot];
                    if let Some(fallback_idx) =
                        song_arc.best_steps_index(target_chart_type, preferred_idx)
                        && let Some(chart_ref) =
                            song_arc.chart_for_steps_index(target_chart_type, fallback_idx)
                    {
                        warn!(
                            "Missing stepchart index {} for '{}'; using fallback index {}",
                            requested_idx, song_arc.title, fallback_idx
                        );
                        resolved_steps_index[slot] = fallback_idx;
                        return chart_ref;
                    }

                    let chart_ref = song_arc
                        .charts
                        .iter()
                        .find(|c| c.chart_type.eq_ignore_ascii_case(target_chart_type))
                        .or_else(|| song_arc.charts.first())
                        .expect("Selected song has no charts");
                    warn!(
                        "Missing indexed stepchart for '{}'; using raw chart fallback ({}/{})",
                        song_arc.title, chart_ref.chart_type, chart_ref.difficulty
                    );
                    chart_ref
                };
                let chart_ix_for_ref = |chart_ref: &deadsync_chart::ChartData| {
                    song_arc
                        .charts
                        .iter()
                        .position(|chart| std::ptr::eq(chart, chart_ref))
                        .expect("selected chart ref must come from selected song")
                };
                let (charts, chart_ixs, last_played_idx) = match play_style {
                    profile_data::PlayStyle::Versus => {
                        let chart_ref_p1 = resolve_chart(0);
                        let chart_ref_p2 = resolve_chart(1);
                        (
                            [
                                Arc::new(chart_ref_p1.clone()),
                                Arc::new(chart_ref_p2.clone()),
                            ],
                            [
                                chart_ix_for_ref(chart_ref_p1),
                                chart_ix_for_ref(chart_ref_p2),
                            ],
                            0usize,
                        )
                    }
                    profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
                        let idx = profile_data::player_side_index(player_side);
                        let chart_ref = resolve_chart(idx);
                        let chart = Arc::new(chart_ref.clone());
                        let chart_ix = chart_ix_for_ref(chart_ref);
                        ([chart.clone(), chart], [chart_ix, chart_ix], idx)
                    }
                };

                let cfg = config::get();
                let global_offset_seconds = cfg.global_offset_seconds;
                let pack_sync_offset_seconds =
                    deadsync_simfile::runtime_cache::pack_sync_offset_for_song_config(
                        song_arc.as_ref(),
                        &cfg,
                    );
                let cabinet_light_plan = cabinet_light_plan(song_arc.as_ref(), chart_ixs[0]);
                let mut requested_chart_ixs = chart_ixs.to_vec();
                let light_payload_start = requested_chart_ixs.len();
                if let Some(plan) = cabinet_light_plan.as_ref() {
                    requested_chart_ixs.extend(plan.request_chart_ixs());
                }

                let payload_started = Instant::now();
                let gameplay_song = match song_loading::load_gameplay_charts(
                    song_arc.as_ref(),
                    &requested_chart_ixs,
                    global_offset_seconds,
                ) {
                    Ok(gameplay_song) => gameplay_song,
                    Err(e) => {
                        error!(
                            "Failed to load practice payload for '{}': {}",
                            song_arc.title, e
                        );
                        self.commit_screen_change(CurrentScreen::PlayerOptions);
                        self.state.screens.player_options_state = Some(po_state);
                        return commands;
                    }
                };
                let gameplay_charts = [
                    Arc::new(gameplay_song[0].clone()),
                    Arc::new(gameplay_song[1].clone()),
                ];
                if let Some(plan) = cabinet_light_plan.as_ref() {
                    let (key, events) = cabinet_light_chart_from_loaded(
                        song_arc.as_ref(),
                        plan,
                        &gameplay_song[light_payload_start..],
                        global_offset_seconds,
                        pack_sync_offset_seconds,
                    );
                    self.gameplay_lights.set_cabinet_chart(key, events);
                }
                let payload_ms = payload_started.elapsed().as_secs_f64() * 1000.0;

                if play_style == profile_data::PlayStyle::Versus {
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index = po_state.chart_difficulty_index[0];
                    self.state.screens.select_music_state.selected_steps_index =
                        resolved_steps_index[0];
                    self.state
                        .screens
                        .select_music_state
                        .p2_preferred_difficulty_index = po_state.chart_difficulty_index[1];
                    self.state
                        .screens
                        .select_music_state
                        .p2_selected_steps_index = resolved_steps_index[1];
                } else {
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index =
                        po_state.chart_difficulty_index[last_played_idx];
                    self.state.screens.select_music_state.selected_steps_index =
                        resolved_steps_index[last_played_idx];
                }

                // Auto-switch CMod → the player's configured alternative for
                // no-cmod charts (this play only; the persisted profile is
                // untouched, so song select restores CMod).
                let scroll_speeds = player_options::apply_no_cmod_alternative(&mut po_state);

                let init_started = Instant::now();
                let mut gs = gameplay::init(
                    song_arc,
                    charts,
                    gameplay_charts,
                    gameplay_viewport(self.state.shell.metrics),
                    gameplay_session(),
                    gameplay_config_from_config(&config::get()),
                    po_state.active_color_index,
                    po_state.music_rate,
                    scroll_speeds,
                    po_state.player_profiles,
                    None,
                    None,
                    Some(Arc::from("Practice Mode")),
                    Arc::from("PRACTICE MODE"),
                    Some(LeadInTiming {
                        min_seconds_to_step: 0.0,
                        min_seconds_to_music: 0.0,
                    }),
                    None,
                    None,
                    None,
                    None,
                    None,
                    [0; MAX_PLAYERS],
                );
                gs.disable_score_for_practice();
                let init_ms = init_started.elapsed().as_secs_f64() * 1000.0;
                let overlay_video_paths = gameplay_overlay_video_paths(&gs);

                let sfx_prewarm_started = Instant::now();
                prewarm_gameplay_sfx(&gs);
                let sfx_prewarm_ms = sfx_prewarm_started.elapsed().as_secs_f64() * 1000.0;
                let asset_prewarm_started = Instant::now();
                if let Some(backend) = self.backend.as_mut() {
                    prewarm_gameplay_assets(&mut self.asset_manager, backend, &gs);
                    self.dynamic_media.set_gameplay_background_keys(
                        &mut self.asset_manager,
                        backend,
                        gameplay_media_keys(&gs),
                    );
                    self.dynamic_media.sync_active_song_lua_videos(
                        &mut self.asset_manager,
                        backend,
                        &overlay_video_paths,
                    );
                    if let Some(path) = gs.song().banner_path.as_ref() {
                        media_cache::ensure_banner_texture(&mut self.asset_manager, backend, path);
                    }
                }
                let asset_prewarm_ms = asset_prewarm_started.elapsed().as_secs_f64() * 1000.0;
                let text_prewarm_started = Instant::now();
                prewarm_gameplay_text_layout_cache(
                    &self.asset_manager,
                    &self.state.shell.metrics,
                    &mut self.gameplay_text_layout_cache,
                    &mut gs,
                );
                let text_prewarm_ms = text_prewarm_started.elapsed().as_secs_f64() * 1000.0;
                let song = gs.song();
                debug!(
                    "Practice transition timing: song='{}' payload_ms={payload_ms:.3} init_ms={init_ms:.3} sfx_prewarm_ms={sfx_prewarm_ms:.3} asset_prewarm_ms={asset_prewarm_ms:.3} text_prewarm_ms={text_prewarm_ms:.3}",
                    song.title
                );
                commands.push(Command::SetPackBanner(gs.pack_banner_path.clone()));
                let show_video_backgrounds = config::get().show_video_backgrounds;
                let background_path =
                    Self::refresh_gameplay_background_path(&mut gs, show_video_backgrounds);
                commands.push(Command::SetDynamicBackground(background_path));
                let mut practice_state = practice::init(gs);
                if let Some(snapshot) = edit_snapshot {
                    practice::restore_edit_snapshot(&mut practice_state, snapshot);
                }
                self.state.screens.practice_state = Some(practice_state);
                if let Some(ps) = self.state.screens.practice_state.as_mut() {
                    crate::screens::practice::on_enter(ps);
                }
            } else {
                panic!("Navigating to Practice without PlayerOptions state!");
            }
        }
        if target == CurrentScreen::Gameplay {
            deadsync_audio_stream::stop_music();
            if prev != CurrentScreen::Gameplay {
                self.state.session.gameplay_restart_count = 0;
                self.state.session.restart_pending = false;
            }
            let mut course_display_carry = None;
            let course_display_totals = self
                .state
                .session
                .course_run
                .as_ref()
                .map(|course| course.course_display_totals);
            let course_display_timing = self
                .state
                .session
                .course_run
                .as_ref()
                .map(course_display_timing_for_run);
            if prev == CurrentScreen::Gameplay && self.state.session.course_run.is_some() {
                if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
                    crate::screens::gameplay::on_exit(gs);
                }
            }
            if prev == CurrentScreen::Gameplay
                && self.state.session.course_run.is_some()
                && let Some(gameplay_results) = self.state.screens.gameplay_state.take()
            {
                self.update_combo_carry_from_gameplay(&gameplay_results);
                course_display_carry = Some(gameplay_results.course_display_carry());
                let color_idx = gameplay_results.active_color_index();
                let mut eval_state = evaluation::init(Some(gameplay_results));
                eval_state.active_color_index = color_idx;
                let _ = self.append_stage_results_from_eval(&eval_state);
            }

            let replay_pending =
                select_music::take_pending_replay(&mut self.state.screens.select_music_state);
            let replay_edges = replay_pending.as_ref().map(|payload| {
                payload
                    .replay
                    .iter()
                    .copied()
                    .map(|e| ReplayInputEdge {
                        lane_index: e.lane_index,
                        pressed: e.pressed,
                        source: e.source,
                        event_music_time_ns: e.event_music_time_ns,
                    })
                    .collect::<Vec<_>>()
            });
            let replay_offsets = replay_pending.as_ref().map(|payload| ReplayOffsetSnapshot {
                beat0_time_ns: payload.replay_beat0_time_ns,
            });
            let replay_status_text = replay_pending.as_ref().map(|payload| {
                Arc::<str>::from(format!(
                    "Autoplay - {} {:.2}%",
                    payload.name,
                    payload.score / 100.0
                ))
            });
            if let Some(mut po_state) = self.state.screens.player_options_state.take() {
                let song_arc = po_state.song.clone();
                let play_style = profile::get_session_play_style();
                let player_side = profile::get_session_player_side();
                let target_chart_type = play_style.chart_type();
                let mut resolved_steps_index = po_state.chart_steps_index;
                let mut resolve_chart = |slot: usize| {
                    let requested_idx = resolved_steps_index[slot];
                    if let Some(chart_ref) =
                        song_arc.chart_for_steps_index(target_chart_type, requested_idx)
                    {
                        return chart_ref;
                    }

                    let preferred_idx = po_state.chart_difficulty_index[slot];
                    if let Some(fallback_idx) =
                        song_arc.best_steps_index(target_chart_type, preferred_idx)
                        && let Some(chart_ref) =
                            song_arc.chart_for_steps_index(target_chart_type, fallback_idx)
                    {
                        warn!(
                            "Missing stepchart index {} for '{}'; using fallback index {}",
                            requested_idx, song_arc.title, fallback_idx
                        );
                        resolved_steps_index[slot] = fallback_idx;
                        return chart_ref;
                    }

                    let chart_ref = song_arc
                        .charts
                        .iter()
                        .find(|c| c.chart_type.eq_ignore_ascii_case(target_chart_type))
                        .or_else(|| song_arc.charts.first())
                        .expect("Selected song has no charts");
                    warn!(
                        "Missing indexed stepchart for '{}'; using raw chart fallback ({}/{})",
                        song_arc.title, chart_ref.chart_type, chart_ref.difficulty
                    );
                    chart_ref
                };
                let chart_ix_for_ref = |chart_ref: &deadsync_chart::ChartData| {
                    song_arc
                        .charts
                        .iter()
                        .position(|chart| std::ptr::eq(chart, chart_ref))
                        .expect("selected chart ref must come from selected song")
                };
                let (charts, chart_ixs, last_played_chart_ref, last_played_idx) = match play_style {
                    profile_data::PlayStyle::Versus => {
                        let chart_ref_p1 = resolve_chart(0);
                        let chart_ref_p2 = resolve_chart(1);
                        (
                            [
                                Arc::new(chart_ref_p1.clone()),
                                Arc::new(chart_ref_p2.clone()),
                            ],
                            [
                                chart_ix_for_ref(chart_ref_p1),
                                chart_ix_for_ref(chart_ref_p2),
                            ],
                            chart_ref_p1,
                            0usize,
                        )
                    }
                    profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
                        let idx = profile_data::player_side_index(player_side);
                        let chart_ref = resolve_chart(idx);
                        let chart = Arc::new(chart_ref.clone());
                        let chart_ix = chart_ix_for_ref(chart_ref);
                        ([chart.clone(), chart], [chart_ix, chart_ix], chart_ref, idx)
                    }
                };

                let gameplay_entry_started = Instant::now();
                let cfg = config::get();
                let global_offset_seconds = cfg.global_offset_seconds;
                let pack_sync_offset_seconds =
                    deadsync_simfile::runtime_cache::pack_sync_offset_for_song_config(
                        song_arc.as_ref(),
                        &cfg,
                    );
                let cabinet_light_plan = cabinet_light_plan(song_arc.as_ref(), chart_ixs[0]);
                let cabinet_light_key = cabinet_light_plan.as_ref().map(|plan| {
                    cabinet_light_key(
                        song_arc.as_ref(),
                        plan,
                        global_offset_seconds,
                        pack_sync_offset_seconds,
                    )
                });
                let reused_gameplay_charts =
                    if prev == CurrentScreen::Gameplay && self.state.session.course_run.is_none() {
                        self.state
                            .screens
                            .gameplay_state
                            .as_ref()
                            .filter(|current| {
                                deadsync_simfile::runtime_cache::can_reuse_quick_restart_payload(
                                    current.song(),
                                    [
                                        current.charts()[0].short_hash.as_str(),
                                        current.charts()[1].short_hash.as_str(),
                                    ],
                                    song_arc.as_ref(),
                                    [charts[0].short_hash.as_str(), charts[1].short_hash.as_str()],
                                )
                            })
                            .map(|current| current.gameplay_charts().clone())
                    } else {
                        None
                    };
                let reusing_gameplay_payload = reused_gameplay_charts.is_some();
                let payload_started = Instant::now();
                let gameplay_charts = if let Some(gameplay_charts) = reused_gameplay_charts {
                    debug!(
                        "Reusing gameplay payload for quick restart '{}'",
                        song_arc.title
                    );
                    match (cabinet_light_plan.as_ref(), cabinet_light_key.as_ref()) {
                        (Some(_), Some(key)) if self.gameplay_lights.cabinet_key_matches(key) => {
                            self.gameplay_lights.restart_cabinet_chart();
                        }
                        (Some(plan), _) => match load_cabinet_light_chart(
                            song_arc.as_ref(),
                            plan,
                            global_offset_seconds,
                            pack_sync_offset_seconds,
                        ) {
                            Ok((key, events)) => {
                                self.gameplay_lights.set_cabinet_chart(key, events)
                            }
                            Err(error) => {
                                warn!(
                                    "Failed to load cabinet light chart for '{}': {}",
                                    song_arc.title, error
                                );
                                self.gameplay_lights.clear();
                            }
                        },
                        _ => self.gameplay_lights.clear(),
                    }
                    gameplay_charts
                } else {
                    let mut requested_chart_ixs = chart_ixs.to_vec();
                    let light_payload_start = requested_chart_ixs.len();
                    if let Some(plan) = cabinet_light_plan.as_ref() {
                        requested_chart_ixs.extend(plan.request_chart_ixs());
                    }
                    let gameplay_song = match song_loading::load_gameplay_charts(
                        song_arc.as_ref(),
                        &requested_chart_ixs,
                        global_offset_seconds,
                    ) {
                        Ok(gameplay_song) => gameplay_song,
                        Err(e) => {
                            error!(
                                "Failed to load gameplay payload for '{}': {}",
                                song_arc.title, e
                            );
                            self.commit_screen_change(CurrentScreen::PlayerOptions);
                            self.state.screens.player_options_state = Some(po_state);
                            return commands;
                        }
                    };
                    let gameplay_charts = [
                        Arc::new(gameplay_song[0].clone()),
                        Arc::new(gameplay_song[1].clone()),
                    ];
                    if let Some(plan) = cabinet_light_plan.as_ref() {
                        let (key, events) = cabinet_light_chart_from_loaded(
                            song_arc.as_ref(),
                            plan,
                            &gameplay_song[light_payload_start..],
                            global_offset_seconds,
                            pack_sync_offset_seconds,
                        );
                        self.gameplay_lights.set_cabinet_chart(key, events);
                    } else {
                        self.gameplay_lights.clear();
                    }
                    gameplay_charts
                };
                let payload_ms = payload_started.elapsed().as_secs_f64() * 1000.0;

                // Keep SelectMusic's current stepchart in sync with what we're about to play.
                if play_style == profile_data::PlayStyle::Versus {
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index = po_state.chart_difficulty_index[0];
                    self.state.screens.select_music_state.selected_steps_index =
                        resolved_steps_index[0];
                    self.state
                        .screens
                        .select_music_state
                        .p2_preferred_difficulty_index = po_state.chart_difficulty_index[1];
                    self.state
                        .screens
                        .select_music_state
                        .p2_selected_steps_index = resolved_steps_index[1];
                } else {
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index =
                        po_state.chart_difficulty_index[last_played_idx];
                    self.state.screens.select_music_state.selected_steps_index =
                        resolved_steps_index[last_played_idx];
                }

                match play_style {
                    profile_data::PlayStyle::Versus => {
                        commands.push(Command::UpdateLastPlayed {
                            side: profile_data::PlayerSide::P1,
                            play_style,
                            music_path: song_arc.music_path.clone(),
                            chart_hash: Some(charts[0].short_hash.clone()),
                            difficulty_index: po_state.chart_difficulty_index[0],
                        });
                        commands.push(Command::UpdateLastPlayed {
                            side: profile_data::PlayerSide::P2,
                            play_style,
                            music_path: song_arc.music_path.clone(),
                            chart_hash: Some(charts[1].short_hash.clone()),
                            difficulty_index: po_state.chart_difficulty_index[1],
                        });
                    }
                    profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
                        commands.push(Command::UpdateLastPlayed {
                            side: player_side,
                            play_style,
                            music_path: song_arc.music_path.clone(),
                            chart_hash: Some(last_played_chart_ref.short_hash.clone()),
                            difficulty_index: po_state.chart_difficulty_index[last_played_idx],
                        });
                    }
                }

                // Auto-switch CMod → the player's configured alternative for
                // no-cmod charts (this play only; the persisted profile is
                // untouched, so song select restores CMod). Replays must
                // reproduce the recorded scroll speed, so skip the swap there.
                let scroll_speeds = if replay_pending.is_none() {
                    player_options::apply_no_cmod_alternative(&mut po_state)
                } else {
                    let to_scroll_speed = |m: &player_options::SpeedMod| match m.mod_type {
                        player_options::SpeedModType::X => ScrollSpeedSetting::XMod(m.value),
                        player_options::SpeedModType::C => ScrollSpeedSetting::CMod(m.value),
                        player_options::SpeedModType::M => ScrollSpeedSetting::MMod(m.value),
                    };
                    [
                        to_scroll_speed(&po_state.speed_mod[0]),
                        to_scroll_speed(&po_state.speed_mod[1]),
                    ]
                };

                let color_index = po_state.active_color_index;
                let lead_in_timing = self.state.session.course_run.as_ref().and_then(|course| {
                    (course.next_stage_index > 0).then_some(LeadInTiming {
                        min_seconds_to_step: COURSE_MIN_SECONDS_TO_STEP_NEXT_SONG,
                        min_seconds_to_music: COURSE_MIN_SECONDS_TO_MUSIC_NEXT_SONG,
                    })
                });
                let (course_display_info, course_banner_path) = self
                    .state
                    .session
                    .course_run
                    .as_ref()
                    .map_or((None, None), |course| {
                        (
                            Some(gameplay::CourseDisplayInfo {
                                name: Arc::from(course.name.as_str()),
                            }),
                            course.banner_path.clone(),
                        )
                    });
                let stage_intro_text: Arc<str> =
                    if let Some(course) = self.state.session.course_run.as_ref() {
                        let stage_num = course.next_stage_index.saturating_add(1);
                        let total = course.stages.len().max(1);
                        Arc::from(format!("STAGE {stage_num} / {total}"))
                    } else if config::get().keyboard_features
                        && self.state.session.gameplay_restart_count > 0
                    {
                        Arc::from(format!(
                            "RESTART {}",
                            self.state.session.gameplay_restart_count
                        ))
                    } else {
                        deadsync_simfile::event_intro::gameplay_event_intro_text(song_arc.as_ref())
                    };
                let combo_carry = self.state.session.combo_carry;
                let init_started = Instant::now();
                let mut gs = gameplay::init(
                    song_arc,
                    charts,
                    gameplay_charts,
                    gameplay_viewport(self.state.shell.metrics),
                    gameplay_session(),
                    gameplay_config_from_config(&config::get()),
                    color_index,
                    po_state.music_rate,
                    scroll_speeds,
                    po_state.player_profiles,
                    replay_edges,
                    replay_offsets,
                    replay_status_text,
                    stage_intro_text,
                    lead_in_timing,
                    course_display_carry,
                    course_display_totals,
                    course_display_timing,
                    course_display_info,
                    course_banner_path,
                    combo_carry,
                );
                let init_ms = init_started.elapsed().as_secs_f64() * 1000.0;
                let overlay_video_paths = gameplay_overlay_video_paths(&gs);

                let sfx_prewarm_started = Instant::now();
                prewarm_gameplay_sfx(&gs);
                let sfx_prewarm_ms = sfx_prewarm_started.elapsed().as_secs_f64() * 1000.0;
                let asset_prewarm_started = Instant::now();
                if let Some(backend) = self.backend.as_mut() {
                    prewarm_gameplay_assets(&mut self.asset_manager, backend, &gs);
                    self.dynamic_media.set_gameplay_background_keys(
                        &mut self.asset_manager,
                        backend,
                        gameplay_media_keys(&gs),
                    );
                    self.dynamic_media.sync_active_song_lua_videos(
                        &mut self.asset_manager,
                        backend,
                        &overlay_video_paths,
                    );
                    if let Some(path) = gs.song().banner_path.as_ref() {
                        media_cache::ensure_banner_texture(&mut self.asset_manager, backend, path);
                    }
                }
                let asset_prewarm_ms = asset_prewarm_started.elapsed().as_secs_f64() * 1000.0;
                let text_prewarm_started = Instant::now();
                prewarm_gameplay_text_layout_cache(
                    &self.asset_manager,
                    &self.state.shell.metrics,
                    &mut self.gameplay_text_layout_cache,
                    &mut gs,
                );
                let text_prewarm_ms = text_prewarm_started.elapsed().as_secs_f64() * 1000.0;
                let total_ms = gameplay_entry_started.elapsed().as_secs_f64() * 1000.0;
                let song = gs.song();
                if total_ms >= 50.0 {
                    info!(
                        "Gameplay transition timing: song='{}' restart={} payload_source={} payload_ms={payload_ms:.3} init_ms={init_ms:.3} sfx_prewarm_ms={sfx_prewarm_ms:.3} asset_prewarm_ms={asset_prewarm_ms:.3} text_prewarm_ms={text_prewarm_ms:.3} elapsed_ms={total_ms:.3}",
                        song.title,
                        prev == CurrentScreen::Gameplay,
                        if reusing_gameplay_payload {
                            "reuse"
                        } else {
                            "load"
                        },
                    );
                } else {
                    debug!(
                        "Gameplay transition timing: song='{}' restart={} payload_source={} payload_ms={payload_ms:.3} init_ms={init_ms:.3} sfx_prewarm_ms={sfx_prewarm_ms:.3} asset_prewarm_ms={asset_prewarm_ms:.3} text_prewarm_ms={text_prewarm_ms:.3} elapsed_ms={total_ms:.3}",
                        song.title,
                        prev == CurrentScreen::Gameplay,
                        if reusing_gameplay_payload {
                            "reuse"
                        } else {
                            "load"
                        },
                    );
                }
                commands.push(Command::SetPackBanner(gs.pack_banner_path.clone()));
                let show_video_backgrounds = config::get().show_video_backgrounds;
                let background_path =
                    Self::refresh_gameplay_background_path(&mut gs, show_video_backgrounds);
                commands.push(Command::SetDynamicBackground(background_path));
                self.state.screens.gameplay_state = Some(gs);
                if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
                    crate::screens::gameplay::on_enter(gs);
                }
                // Song Start / Restart SFX (zmod parity, issue #375). At this
                // point `gameplay_restart_count` has already been zeroed for
                // fresh entries (line above) and preserved for in-screen
                // restarts (`try_gameplay_restart` incremented it before we
                // arrived).
                let restart_count = self.state.session.gameplay_restart_count;
                if restart_count == 0 {
                    crate::assets::audio_folder::play_random_sfx("assets/sounds/song_start");
                } else {
                    crate::assets::audio_folder::play_indexed_sfx(
                        "assets/sounds/song_start/restart",
                        restart_count,
                        "restart.ogg",
                    );
                }
                if let Some(course) = self.state.session.course_run.as_mut() {
                    course.next_stage_index = course.next_stage_index.saturating_add(1);
                }
            } else {
                panic!("Navigating to Gameplay without PlayerOptions state!");
            }
        }

        if target == CurrentScreen::Evaluation {
            if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
                crate::screens::gameplay::on_exit(gs);
            }
            let gameplay_results = self.state.screens.gameplay_state.take();
            if let Some(gs) = gameplay_results.as_ref() {
                self.update_combo_carry_from_gameplay(gs);
            }
            if let (Some(backend), Some(gs)) = (self.backend.as_mut(), gameplay_results.as_ref())
                && let Some(path) = gs.song().banner_path.as_ref()
            {
                media_cache::ensure_banner_texture(&mut self.asset_manager, backend, path);
            }
            let color_idx = gameplay_results.as_ref().map_or(
                self.state.screens.evaluation_state.active_color_index,
                |gs| gs.gameplay.active_color_index(),
            );
            self.state.screens.evaluation_state = gameplay_results
                .map(|gs| evaluation::init(Some(gs)))
                .unwrap_or_else(|| evaluation::init(None));
            self.state.screens.evaluation_state.active_color_index = color_idx;
            self.state.screens.evaluation_state.return_to_course =
                self.state.session.course_run.is_some();
            self.state.screens.evaluation_state.auto_advance_seconds = None;
            if let Some(start) = self.state.session.session_start_time {
                self.state.screens.evaluation_state.session_elapsed =
                    Instant::now().duration_since(start).as_secs_f32();
            }
            self.state.screens.evaluation_state.gameplay_elapsed =
                stage_stats::total_stage_duration_seconds(&self.state.session.played_stages);
            self.finalize_entered_evaluation();
        }

        if target == CurrentScreen::EvaluationSummary {
            let color_idx = match prev {
                CurrentScreen::SelectMusic => {
                    self.state.screens.select_music_state.active_color_index
                }
                CurrentScreen::SelectCourse => {
                    self.state.screens.select_course_state.active_color_index
                }
                CurrentScreen::Evaluation => self.state.screens.evaluation_state.active_color_index,
                _ => {
                    self.state
                        .screens
                        .evaluation_summary_state
                        .active_color_index
                }
            };
            let return_to = evaluation_summary_return_to(
                prev,
                std::mem::take(&mut self.state.session.pending_post_select_summary_exit),
            );
            self.state.screens.evaluation_summary_state =
                evaluation_summary::init_for_return(return_to);
            self.state
                .screens
                .evaluation_summary_state
                .active_color_index = color_idx;

            let display_stages = self.post_select_display_stages().into_owned();
            if let Some(backend) = self.backend.as_mut() {
                for stage in display_stages.iter() {
                    if let Some(path) = stage.song.banner_path.as_ref() {
                        media_cache::ensure_banner_texture(&mut self.asset_manager, backend, path);
                    }
                }
            }
        }

        if target == CurrentScreen::Initials {
            let color_idx = match prev {
                CurrentScreen::EvaluationSummary => {
                    self.state
                        .screens
                        .evaluation_summary_state
                        .active_color_index
                }
                CurrentScreen::SelectMusic => {
                    self.state.screens.select_music_state.active_color_index
                }
                CurrentScreen::SelectCourse => {
                    self.state.screens.select_course_state.active_color_index
                }
                CurrentScreen::Evaluation => self.state.screens.evaluation_state.active_color_index,
                _ => self.state.screens.initials_state.active_color_index,
            };
            self.state.screens.initials_state = initials::init();
            self.state.screens.initials_state.active_color_index = color_idx;
            let display_stages = self.post_select_display_stages().into_owned();
            initials::set_highscore_lists(&mut self.state.screens.initials_state, &display_stages);

            if let Some(backend) = self.backend.as_mut() {
                for stage in display_stages.iter() {
                    if let Some(path) = stage.song.banner_path.as_ref() {
                        media_cache::ensure_banner_texture(&mut self.asset_manager, backend, path);
                    }
                }
            }
        }

        if target == CurrentScreen::GameOver {
            let color_idx = match prev {
                CurrentScreen::Initials => self.state.screens.initials_state.active_color_index,
                CurrentScreen::EvaluationSummary => {
                    self.state
                        .screens
                        .evaluation_summary_state
                        .active_color_index
                }
                CurrentScreen::SelectMusic => {
                    self.state.screens.select_music_state.active_color_index
                }
                CurrentScreen::SelectCourse => {
                    self.state.screens.select_course_state.active_color_index
                }
                CurrentScreen::Evaluation => self.state.screens.evaluation_state.active_color_index,
                _ => self.state.screens.gameover_state.active_color_index,
            };
            self.state.screens.gameover_state = gameover::init();
            self.state.screens.gameover_state.active_color_index = color_idx;
        }

        if target == CurrentScreen::SelectMusic {
            self.clear_course_runtime();
            self.begin_play_session();

            match prev {
                CurrentScreen::PlayerOptions => {
                    let preferred = self.state.session.preferred_difficulty_index;
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index = preferred;

                    if let Some(po) = self.state.screens.player_options_state.as_ref() {
                        let play_style = profile::get_session_play_style();
                        match play_style {
                            profile_data::PlayStyle::Versus => {
                                self.state.screens.select_music_state.selected_steps_index =
                                    po.chart_steps_index[0];
                                self.state
                                    .screens
                                    .select_music_state
                                    .p2_selected_steps_index = po.chart_steps_index[1];
                                self.state
                                    .screens
                                    .select_music_state
                                    .preferred_difficulty_index = po.chart_difficulty_index[0];
                                self.state
                                    .screens
                                    .select_music_state
                                    .p2_preferred_difficulty_index = po.chart_difficulty_index[1];
                            }
                            profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
                                let side = profile::get_session_player_side();
                                let idx = profile_data::player_side_index(side);
                                self.state.screens.select_music_state.selected_steps_index =
                                    po.chart_steps_index[idx];
                                self.state
                                    .screens
                                    .select_music_state
                                    .preferred_difficulty_index = po.chart_difficulty_index[idx];
                            }
                        }
                    }

                    let desired_steps_index =
                        self.state.screens.select_music_state.selected_steps_index;

                    if let Some(select_music::MusicWheelEntry::Song(song)) = self
                        .state
                        .screens
                        .select_music_state
                        .entries
                        .get(self.state.screens.select_music_state.selected_index)
                    {
                        let chart_type = profile::get_session_play_style().chart_type();
                        if song
                            .chart_for_steps_index(chart_type, desired_steps_index)
                            .is_none()
                        {
                            let mut best_match_index = None;
                            let mut min_diff = i32::MAX;
                            for i in 0..STANDARD_DIFFICULTY_COUNT {
                                if select_music::is_difficulty_playable(song, i) {
                                    let diff = (i as i32 - preferred as i32).abs();
                                    if diff < min_diff {
                                        min_diff = diff;
                                        best_match_index = Some(i);
                                    }
                                }
                            }
                            if let Some(idx) = best_match_index {
                                self.state.screens.select_music_state.selected_steps_index = idx;
                            }
                        }
                    }

                    select_music::trigger_immediate_refresh(
                        &mut self.state.screens.select_music_state,
                    );
                }
                CurrentScreen::Gameplay | CurrentScreen::Practice | CurrentScreen::Evaluation => {
                    select_music::reset_preview_after_gameplay(
                        &mut self.state.screens.select_music_state,
                    );
                }
                CurrentScreen::EvaluationSummary => {
                    select_music::trigger_immediate_refresh(
                        &mut self.state.screens.select_music_state,
                    );
                }
                CurrentScreen::ProfileLoad => {
                    select_music::trigger_immediate_refresh(
                        &mut self.state.screens.select_music_state,
                    );
                }
                _ => {
                    let current_color_index =
                        self.state.screens.select_music_state.active_color_index;
                    self.state.screens.select_music_state = select_music::init();
                    self.state.screens.select_music_state.active_color_index = current_color_index;
                    let preferred = self.state.session.preferred_difficulty_index;
                    self.state.screens.select_music_state.selected_steps_index = preferred;
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index = preferred;

                    let p2_pref = profile::preferred_difficulty_for_side(
                        profile_data::PlayerSide::P2,
                        profile::get_session_play_style(),
                    );
                    self.state
                        .screens
                        .select_music_state
                        .p2_selected_steps_index = p2_pref;
                    self.state
                        .screens
                        .select_music_state
                        .p2_preferred_difficulty_index = p2_pref;

                    // Treat the initial selection as already "settled" so preview/graphs can start
                    // immediately after the transition, matching ITG/Simply Love behavior.
                    select_music::trigger_immediate_refresh(
                        &mut self.state.screens.select_music_state,
                    );
                }
            }
            self.state.screens.select_music_state.gameplay_elapsed =
                stage_stats::total_stage_duration_seconds(&self.state.session.played_stages);

            // Prime the delayed panes (tech counts, breakdown, etc.) for the selected chart so they
            // render immediately on entry (no initial debounce delay).
            select_music::prime_displayed_chart_data(&mut self.state.screens.select_music_state);

            // Load the selected entry's banner during the fade-in so it appears immediately.
            let banner_path = match self
                .state
                .screens
                .select_music_state
                .entries
                .get(self.state.screens.select_music_state.selected_index)
            {
                Some(select_music::MusicWheelEntry::Song(song)) => song.banner_path.clone(),
                Some(select_music::MusicWheelEntry::PackHeader { banner_path, .. }) => {
                    banner_path.clone()
                }
                None => None,
            };
            commands.push(Command::SetBanner(banner_path));
            let cdtitle_path = match self
                .state
                .screens
                .select_music_state
                .entries
                .get(self.state.screens.select_music_state.selected_index)
            {
                Some(select_music::MusicWheelEntry::Song(song)) => song.cdtitle_path.clone(),
                _ => None,
            };
            commands.push(Command::SetCdTitle(cdtitle_path));

            // Pre-render the density graph during the fade-in so the panel isn't blank on entry.
            let chart_to_graph = match self
                .state
                .screens
                .select_music_state
                .entries
                .get(self.state.screens.select_music_state.selected_index)
            {
                Some(select_music::MusicWheelEntry::Song(song)) => {
                    let chart_type = profile::get_session_play_style().chart_type();
                    song.chart_for_steps_index(
                        chart_type,
                        self.state.screens.select_music_state.selected_steps_index,
                    )
                    .map(|c| DensityGraphSource {
                        max_nps: c.max_nps,
                        measure_nps_vec: c.measure_nps_vec.clone(),
                        measure_seconds_vec: c.measure_seconds_vec.clone(),
                        first_second: c.first_second,
                        last_second: song.precise_last_second(),
                    })
                }
                _ => None,
            };
            commands.push(Command::SetDensityGraph {
                slot: DensityGraphSlot::SelectMusicP1,
                chart_opt: chart_to_graph,
            });

            if profile::get_session_play_style() == profile_data::PlayStyle::Versus {
                let chart_to_graph_p2 = match self
                    .state
                    .screens
                    .select_music_state
                    .entries
                    .get(self.state.screens.select_music_state.selected_index)
                {
                    Some(select_music::MusicWheelEntry::Song(song)) => {
                        let chart_type = profile::get_session_play_style().chart_type();
                        song.chart_for_steps_index(
                            chart_type,
                            self.state
                                .screens
                                .select_music_state
                                .p2_selected_steps_index,
                        )
                        .map(|c| DensityGraphSource {
                            max_nps: c.max_nps,
                            measure_nps_vec: c.measure_nps_vec.clone(),
                            measure_seconds_vec: c.measure_seconds_vec.clone(),
                            first_second: c.first_second,
                            last_second: song.precise_last_second(),
                        })
                    }
                    _ => None,
                };
                commands.push(Command::SetDensityGraph {
                    slot: DensityGraphSlot::SelectMusicP2,
                    chart_opt: chart_to_graph_p2,
                });
            }

            // PlayerOptions state is tied to a specific song selection; once we're back on
            // SelectMusic, drop it so direct Gameplay starts cannot reuse stale song data.
            self.state.screens.player_options_state = None;
        }

        if target == CurrentScreen::SelectCourse {
            let restore_course_selection = self
                .state
                .session
                .course_run
                .as_ref()
                .map(|course| {
                    (
                        course.path.clone(),
                        Some(course.course_difficulty_name.clone()),
                    )
                })
                .or_else(|| {
                    self.state
                        .session
                        .last_course_wheel_path
                        .as_ref()
                        .map(|path| {
                            (
                                path.clone(),
                                self.state.session.last_course_wheel_difficulty_name.clone(),
                            )
                        })
                });
            self.clear_course_runtime();
            self.begin_play_session();

            match prev {
                CurrentScreen::ProfileLoad | CurrentScreen::EvaluationSummary => {
                    select_course::trigger_immediate_refresh(
                        &mut self.state.screens.select_course_state,
                    );
                }
                _ => {
                    let current_color_index =
                        self.state.screens.select_course_state.active_color_index;
                    self.state.screens.select_course_state = select_course::init();
                    self.state.screens.select_course_state.active_color_index = current_color_index;
                    select_course::trigger_immediate_refresh(
                        &mut self.state.screens.select_course_state,
                    );
                }
            }
            if let Some((course_path, course_diff_name)) = restore_course_selection.as_ref() {
                select_course::restore_selection_for_course(
                    &mut self.state.screens.select_course_state,
                    course_path.as_path(),
                    course_diff_name.as_deref(),
                );
            }

            let banner_path = match self
                .state
                .screens
                .select_course_state
                .entries
                .get(self.state.screens.select_course_state.selected_index)
            {
                Some(select_music::MusicWheelEntry::Song(song)) => song.banner_path.clone(),
                Some(select_music::MusicWheelEntry::PackHeader { banner_path, .. }) => {
                    banner_path.clone()
                }
                None => None,
            };
            commands.push(Command::SetBanner(banner_path));
            commands.push(Command::SetCdTitle(None));
        }
        commands
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, _cause: StartCause) {
        self.state.shell.note_new_events(Instant::now());
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::GamepadSystem(ev) => {
                if self.state.screens.current_screen == CurrentScreen::Sandbox {
                    crate::screens::sandbox::handle_gamepad_system_event(
                        &mut self.state.screens.sandbox_state,
                        &ev,
                    );
                }
                match &ev {
                    GpSystemEvent::StartupComplete => {}
                    GpSystemEvent::Connected {
                        name,
                        id,
                        backend,
                        initial,
                        ..
                    } => {
                        debug!(
                            "Gamepad connected: {} (ID: {}) via {:?}",
                            name,
                            usize::from(*id),
                            backend
                        );
                        if *backend == PadBackend::Smx {
                            config::send_smx_underglow_color();
                        }
                        if !*initial {
                            self.state.shell.gamepad_overlay_state = Some((
                                format!(
                                    "Connected: {} (ID: {}) via {:?}",
                                    name,
                                    usize::from(*id),
                                    backend
                                ),
                                Instant::now(),
                            ));
                        }
                    }
                    GpSystemEvent::Disconnected {
                        name,
                        id,
                        backend,
                        initial,
                        ..
                    } => {
                        debug!(
                            "Gamepad disconnected: {} (ID: {}) via {:?}",
                            name,
                            usize::from(*id),
                            backend
                        );
                        if *backend == PadBackend::Smx {
                            config::send_smx_underglow_color();
                        }
                        if !*initial {
                            self.state.shell.gamepad_overlay_state = Some((
                                format!(
                                    "Disconnected: {} (ID: {}) via {:?}",
                                    name,
                                    usize::from(*id),
                                    backend
                                ),
                                Instant::now(),
                            ));
                        }
                    }
                }
            }
            UserEvent::Pad(ev) => {
                if !self.accepts_live_input() {
                    return;
                }
                let gameplay_screen = self.state.screens.current_screen == CurrentScreen::Gameplay;
                let handled_started = Instant::now();
                let mut raw_pad_consumed = false;
                if self.state.screens.current_screen == CurrentScreen::Sandbox {
                    crate::screens::sandbox::handle_raw_pad_event(
                        &mut self.state.screens.sandbox_state,
                        &ev,
                    );
                } else if self.state.screens.current_screen == CurrentScreen::Mappings {
                    raw_pad_consumed = crate::screens::mappings::handle_raw_pad_event(
                        &mut self.state.screens.mappings_state,
                        &ev,
                    );
                } else if self.state.screens.current_screen == CurrentScreen::Input {
                    crate::screens::input::handle_raw_pad_event(
                        &mut self.state.screens.input_state,
                        &ev,
                    );
                } else if self.state.screens.current_screen == CurrentScreen::SelectMusic {
                    crate::screens::select_music::handle_raw_pad_event(
                        &mut self.state.screens.select_music_state,
                        &ev,
                    );
                } else if self.state.screens.current_screen == CurrentScreen::Evaluation {
                    crate::screens::evaluation::handle_raw_pad_event(
                        &mut self.state.screens.evaluation_state,
                        &ev,
                    );
                }
                if !raw_pad_consumed {
                    self.handle_pad_event(event_loop, ev);
                }
                self.state
                    .shell
                    .note_gameplay_pad_handler(gameplay_screen, elapsed_us_since(handled_started));
            }
            UserEvent::Key(ev) => {
                if !self.accepts_live_input() {
                    return;
                }
                self.handle_live_key_event(event_loop, ev);
            }
        }
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            if let Err(e) = self.init_graphics(event_loop) {
                error!("Failed to initialize graphics: {e}");
                event_loop.exit();
            }
            // After all initial loading is complete, start network checks.
            deadsync_online::runtime::init();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if window_id != window.id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                info!("Close requested. Shutting down.");
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                let now = Instant::now();
                let surface_changed = self
                    .state
                    .shell
                    .set_surface_active(new_size.width > 0 && new_size.height > 0, now);
                self.sync_gameplay_input_capture();
                if surface_changed {
                    debug!(
                        "Window surface state changed: active={} size={}x{} screen={:?}",
                        self.state.shell.surface_active,
                        new_size.width,
                        new_size.height,
                        self.state.screens.current_screen
                    );
                }
                self.sync_window_size(new_size);
                if surface_changed && self.state.shell.surface_active {
                    self.request_redraw(&window, "surface_active");
                }
            }
            WindowEvent::Focused(focused) => {
                #[cfg(target_os = "windows")]
                if matches!(
                    self.state.shell.display_mode,
                    DisplayMode::Fullscreen(config::FullscreenType::Exclusive)
                ) {
                    if !focused {
                        window.set_minimized(true);
                    } else if window.is_minimized().unwrap_or(false) {
                        window.set_minimized(false);
                    }
                }
                self.apply_window_focus_change(focused, Instant::now(), Some(&window));
            }
            WindowEvent::Occluded(occluded) => {
                if self
                    .state
                    .shell
                    .set_window_occluded(occluded, Instant::now())
                {
                    self.sync_gameplay_input_capture();
                    debug!(
                        "Window occlusion changed: occluded={} screen={:?}",
                        occluded, self.state.screens.current_screen
                    );
                    if !occluded && self.state.shell.surface_active {
                        self.request_redraw(&window, "occluded");
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if !self.accepts_live_input() {
                    return;
                }
                if key_event.state == winit::event::ElementState::Pressed
                    && let Some(text) = key_event.text.as_deref()
                {
                    self.handle_key_text(event_loop, text);
                }
                #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                self.handle_unix_window_keyboard_fallback(event_loop, &key_event);
            }
            WindowEvent::RedrawRequested => {
                let redraw_started = Instant::now();
                let (request_to_redraw_us, redraw_request_reason) =
                    self.state.shell.take_redraw_request_timing(redraw_started);
                self.run_frame(
                    event_loop,
                    window,
                    redraw_started,
                    request_to_redraw_us,
                    redraw_request_reason,
                );
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = self.window.clone() else {
            return;
        };
        self.state
            .shell
            .finish_gameplay_event_batch(Instant::now(), self.state.screens.current_screen);
        self.sync_gameplay_input_capture();
        match self.flush_due_input_events(event_loop) {
            Ok(true) => self.request_redraw(&window, "input_debounce"),
            Ok(false) => {}
            Err(e) => {
                error!("Failed to handle debounced input before wait: {e}");
                event_loop.exit();
                return;
            }
        }
        let interval_state = self.redraw_interval_state(&window);
        if let Some(interval) = interval_state.interval {
            self.log_frame_loop_mode(FrameLoopMode::Scheduled(interval_state.reason, interval));
            let now = Instant::now();
            if now >= self.state.shell.next_redraw_at {
                self.request_redraw_if_needed(&window, interval_state.reason.redraw_reason());
                self.state.shell.next_redraw_at =
                    config::advance_redraw_deadline(self.state.shell.next_redraw_at, now, interval);
            }
            let deadline = self.state.shell.next_redraw_at;
            let time_until_deadline = deadline.saturating_duration_since(now);
            if time_until_deadline <= SCHEDULED_REDRAW_POLL_GUARD {
                event_loop.set_control_flow(ControlFlow::Poll);
                return;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                deadline - SCHEDULED_REDRAW_POLL_GUARD,
            ));
            return;
        }
        if self.state.shell.redraw_pending() {
            self.log_frame_loop_mode(FrameLoopMode::WaitPending);
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        self.log_frame_loop_mode(FrameLoopMode::Poll);
        event_loop.set_control_flow(ControlFlow::Poll);
        self.request_redraw(&window, "poll");
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        config::flush_pending_saves();
        if let Some(backend) = &mut self.backend {
            self.dynamic_media
                .destroy_assets(&mut self.asset_manager, backend);
            let mut textures = self.asset_manager.take_textures();
            backend.dispose_textures(&mut textures);
            backend.cleanup();
        }
    }
}

#[inline(always)]
fn native_input_host() -> deadsync_input_native::BackendHost {
    deadsync_input_native::backend_host(config::pad_index_for_uuid, |vendor, product| {
        deadsync_smx::native_smx_owns_device(vendor, product, config::get().smx_input)
    })
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = config::get();
    let backend_type = config.video_renderer;
    let show_stats_mode = config.show_stats_mode.min(2);
    let color_index = config.simply_love_color;
    let profile_data = profile::get();
    let event_loop: EventLoop<UserEvent> = EventLoop::<UserEvent>::with_user_event().build()?;
    let mut app = App::new(
        backend_type,
        show_stats_mode,
        color_index,
        config,
        profile_data,
    );

    // Spawn background input backend threads; all input stays decoupled from frame rate.
    let proxy: EventLoopProxy<UserEvent> = event_loop.create_proxy();
    // Raw input backends default to "unfocused" until init_graphics seeds the
    // real focus state from the created window. This prevents global keyboard
    // input (e.g. Win32 RawInput RIDEV_INPUTSINK, evdev, IOHID) from being
    // routed into the game while it is launched into the background.
    app.sync_gameplay_input_capture();
    #[cfg(windows)]
    {
        let win_pad_backend = config.windows_gamepad_backend;
        let proxy_pad = proxy.clone();
        let proxy_sys = proxy.clone();
        let proxy_key = proxy.clone();
        let input_host = native_input_host();
        std::thread::spawn(move || {
            deadsync_input_native::run_windows_backend(
                win_pad_backend,
                move |pe| {
                    let _ = proxy_pad.send_event(UserEvent::Pad(pe));
                },
                move |se| {
                    let _ = proxy_sys.send_event(UserEvent::GamepadSystem(se));
                },
                move |ev| {
                    let _ = proxy_key.send_event(UserEvent::Key(ev));
                },
                input_host,
            );
        });
    }
    #[cfg(target_os = "linux")]
    {
        let proxy_pad = proxy.clone();
        let proxy_sys = proxy.clone();
        let proxy_key = proxy.clone();
        let input_host = native_input_host();
        std::thread::spawn(move || {
            deadsync_input_native::run_linux_backend(
                move |pe| {
                    let _ = proxy_pad.send_event(UserEvent::Pad(pe));
                },
                move |se| {
                    let _ = proxy_sys.send_event(UserEvent::GamepadSystem(se));
                },
                move |ke| {
                    let _ = proxy_key.send_event(UserEvent::Key(ke));
                },
                input_host,
            );
        });
    }
    #[cfg(target_os = "freebsd")]
    {
        let proxy_pad = proxy.clone();
        let proxy_sys = proxy.clone();
        let proxy_key = proxy.clone();
        let input_host = native_input_host();
        std::thread::spawn(move || {
            deadsync_input_native::run_freebsd_backend(
                move |pe| {
                    let _ = proxy_pad.send_event(UserEvent::Pad(pe));
                },
                move |se| {
                    let _ = proxy_sys.send_event(UserEvent::GamepadSystem(se));
                },
                move |ke| {
                    let _ = proxy_key.send_event(UserEvent::Key(ke));
                },
                input_host,
            );
        });
    }
    #[cfg(target_os = "macos")]
    {
        let proxy_pad = proxy.clone();
        let proxy_sys = proxy.clone();
        let proxy_key = proxy.clone();
        let input_host = native_input_host();
        std::thread::spawn(move || {
            deadsync_input_native::run_macos_backend(
                move |pe| {
                    let _ = proxy_pad.send_event(UserEvent::Pad(pe));
                },
                move |se| {
                    let _ = proxy_sys.send_event(UserEvent::GamepadSystem(se));
                },
                move |ke| {
                    let _ = proxy_key.send_event(UserEvent::Key(ke));
                },
                input_host,
            );
        });
    }
    // StepManiaX pad input (all platforms, user-selectable).
    if config.smx_input {
        let proxy_pad = proxy.clone();
        let proxy_sys = proxy.clone();
        let (p1_serial, p2_serial) = config::smx_pad_assignment();
        if deadsync_smx::init(deadsync_smx::InitConfig {
            p1_serial,
            p2_serial,
        }) {
            deadsync_smx::add_input_listener(Box::new(move |pe| {
                let _ = proxy_pad.send_event(UserEvent::Pad(pe));
            }));
            deadsync_smx::add_sys_listener(Box::new(move |se| {
                let _ = proxy_sys.send_event(UserEvent::GamepadSystem(se));
            }));
        }
    }
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, TechCounts};

    fn test_chart(hash: &str) -> ChartData {
        test_chart_with("dance-single", "Hard", hash)
    }

    fn test_chart_with(chart_type: &str, difficulty: &str, hash: &str) -> ChartData {
        ChartData {
            chart_type: chart_type.to_string(),
            difficulty: difficulty.to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 9,
            step_artist: String::new(),
            music_path: None,
            short_hash: hash.to_string(),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
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
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
        }
    }

    fn test_song(path: &str, offset: f32, hashes: [&str; 2]) -> SongData {
        SongData {
            simfile_path: PathBuf::from(path),
            title: "Song".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
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
            display_bpm: String::new(),
            offset,
            sample_start: None,
            sample_length: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
            normalized_bpms: "120.000".to_string(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: vec![test_chart(hashes[0]), test_chart(hashes[1])],
        }
    }

    fn test_score_info(
        song: Arc<SongData>,
        side: profile_data::PlayerSide,
        hash: &str,
        speed_mod: ScrollSpeedSetting,
        music_rate: f32,
    ) -> evaluation::ScoreInfo {
        evaluation::ScoreInfo {
            song: song.clone(),
            chart: Arc::new(test_chart(hash)),
            course_graph_stages: Vec::new(),
            side,
            profile_name: String::new(),
            score_valid: true,
            disqualified: false,
            expected_groovestats_submit: false,
            expected_arrowcloud_submit: false,
            groovestats: score_data::GrooveStatsEvalState::default(),
            itl: score_data::ItlEvalState::default(),
            judgment_counts: [0; judgment_rules::JUDGE_GRADE_COUNT],
            score_percent: 0.0,
            earned_grade_points: 0,
            possible_grade_points: 0,
            grade: score_data::Grade::Tier01,
            speed_mod,
            mods_text: {
                let profile = profile::get_for_side(side);
                profile_data::evaluation_mods_text(&profile, speed_mod)
            },
            hands_achieved: 0,
            hands_total: 0,
            holds_held: 0,
            holds_held_for_score: 0,
            holds_total: 0,
            rolls_held: 0,
            rolls_held_for_score: 0,
            rolls_total: 0,
            mines_hit_for_score: 0,
            mines_avoided: 0,
            mines_total: 0,
            timing: timing_rules::TimingStats::default(),
            arrow_timing: Default::default(),
            scatter: Vec::new(),
            scatter_worst_window_ms: 45.0,
            histogram: timing_rules::HistogramMs::default(),
            graph_first_second: 0.0,
            graph_last_second: song.precise_last_second(),
            music_rate,
            life_history: Vec::new(),
            fail_time: None,
            window_counts: timing_rules::WindowCounts::default(),
            window_counts_10ms: timing_rules::WindowCounts::default(),
            ex_score_percent: 0.0,
            hard_ex_score_percent: 0.0,
            calories_burned: 0.0,
            column_judgments: Vec::new(),
            noteskin: None,
            show_fa_plus_window: false,
            show_ex_score: false,
            show_hard_ex_score: false,
            show_fa_plus_pane: false,
            track_early_judgments: false,
            disabled_timing_windows: [false; 5],
            machine_records: Vec::new(),
            machine_record_highlight_rank: None,
            personal_records: Vec::new(),
            personal_record_highlight_rank: None,
            show_machine_personal_split: false,
        }
    }

    fn test_song_with_duration(path: &str, hash: &str, seconds: f32) -> Arc<SongData> {
        let mut song = test_song(path, 0.0, [hash, hash]);
        song.music_length_seconds = seconds;
        song.total_length_seconds = seconds.round() as i32;
        song.precise_last_second_seconds = seconds;
        Arc::new(song)
    }

    fn test_course_stage(song: Arc<SongData>) -> CourseStageRuntime {
        CourseStageRuntime {
            song,
            steps_index: [0; MAX_PLAYERS],
            preferred_difficulty_index: [0; MAX_PLAYERS],
        }
    }

    fn test_player_stage_summary(
        chart: Arc<ChartData>,
        grade: score_data::Grade,
        score_percent: f64,
        earned_grade_points: i32,
        possible_grade_points: i32,
    ) -> stage_stats::PlayerStageSummary {
        stage_stats::PlayerStageSummary {
            profile_name: "P1".to_string(),
            chart,
            score_valid: true,
            disqualified: false,
            groovestats: score_data::GrooveStatsEvalState::default(),
            itl: score_data::ItlEvalState::default(),
            grade,
            score_percent,
            earned_grade_points,
            possible_grade_points,
            ex_score_percent: 100.0,
            hard_ex_score_percent: 100.0,
            hands_achieved: 1,
            hands_total: 1,
            holds_held: 2,
            holds_held_for_score: 2,
            holds_total: 2,
            rolls_held: 1,
            rolls_held_for_score: 1,
            rolls_total: 1,
            mines_hit_for_score: 0,
            mines_avoided: 3,
            mines_total: 3,
            notes_hit: 20,
            calories_burned: 12.5,
            window_counts: timing_rules::WindowCounts {
                w0: 20,
                ..Default::default()
            },
            window_counts_10ms: timing_rules::WindowCounts {
                w0: 16,
                w1: 4,
                ..Default::default()
            },
            timing: timing_rules::TimingStats {
                mean_abs_ms: 10.0,
                mean_ms: 10.0,
                stddev_ms: 0.0,
                max_abs_ms: 10.0,
            },
            arrow_timing: Default::default(),
            scatter: vec![timing_rules::ScatterPoint {
                time_sec: 12.0,
                offset_ms: Some(10.0),
                direction_code: 1,
                is_stream: false,
                is_left_foot: true,
                miss_because_held: false,
            }],
            scatter_worst_window_ms: 45.0,
            histogram: timing_rules::HistogramMs {
                bins: vec![(10, 1)],
                smoothed: Vec::new(),
                max_count: 1,
                worst_observed_ms: 10.0,
                worst_window_ms: 45.0,
            },
            graph_first_second: 0.0,
            graph_last_second: 60.0,
            life_history: vec![(0.0, 1.0), (60.0, 0.0)],
            fail_time: Some(60.0),
            show_w0: true,
            show_ex_score: true,
            show_hard_ex_score: true,
            show_fa_plus_pane: true,
            track_early_judgments: true,
        }
    }

    #[test]
    fn raw_keyboard_restart_screen_matches_zmod_restart_flow() {
        assert!(App::raw_keyboard_restart_screen(CurrentScreen::Gameplay));
        assert!(App::raw_keyboard_restart_screen(CurrentScreen::Evaluation));
        assert!(!App::raw_keyboard_restart_screen(
            CurrentScreen::EvaluationSummary,
        ));
    }

    #[test]
    fn course_eval_final_on_completion_or_failure() {
        assert!(!stage_stats::course_eval_is_final(1, 3, false));
        assert!(stage_stats::course_eval_is_final(1, 3, true));
        assert!(stage_stats::course_eval_is_final(3, 3, false));
    }

    #[test]
    fn course_summary_uses_trail_totals_and_keeps_timing_graphs() {
        let song_a = test_song_with_duration("Songs/Test/a.ssc", "a", 60.0);
        let song_b = test_song_with_duration("Songs/Test/b.ssc", "b", 90.0);
        let mut chart = test_chart("stage-a");
        chart.step_artist = "Stage Artist".to_string();
        chart.description = "Stage Description".to_string();
        chart.chart_name = "Stage Chart Name".to_string();
        let chart = Arc::new(chart);
        let mut stage_players: [Option<stage_stats::PlayerStageSummary>; MAX_PLAYERS] =
            std::array::from_fn(|_| None);
        stage_players[0] = Some(test_player_stage_summary(
            chart,
            score_data::Grade::Failed,
            1.0,
            500,
            500,
        ));

        let mut course_display_totals = [CourseDisplayTotals::default(); MAX_PLAYERS];
        course_display_totals[0] = CourseDisplayTotals {
            possible_grade_points: 1000,
            total_steps: 40,
            holds_total: 4,
            rolls_total: 2,
            mines_total: 6,
        };
        let course = CourseRunState {
            path: PathBuf::from("Courses/Test.crs"),
            name: "Test Course".to_string(),
            banner_path: None,
            score_hash: "course-hash".to_string(),
            course_difficulty_name: "Hard".to_string(),
            course_meter: Some(12),
            course_stepchart_label: "Hard".to_string(),
            song_stub: song_a.clone(),
            stages: vec![
                test_course_stage(song_a.clone()),
                test_course_stage(song_b.clone()),
            ],
            course_display_totals,
            next_stage_index: 1,
            stage_summaries: vec![stage_stats::StageSummary {
                song: song_a.clone(),
                music_rate: 1.0,
                duration_seconds: 60.0,
                players: stage_players,
            }],
            stage_eval_pages: Vec::new(),
        };

        let summary = build_course_summary_stage(&course).expect("course summary");
        let player = summary.players[0].as_ref().expect("P1 summary");
        assert!((summary.duration_seconds - 150.0).abs() <= f32::EPSILON);
        assert!((player.score_percent - 0.5).abs() <= f64::EPSILON);
        assert_eq!(player.earned_grade_points, 500);
        assert_eq!(player.possible_grade_points, 1000);
        assert_eq!(player.holds_total, 4);
        assert_eq!(player.rolls_total, 2);
        assert_eq!(player.mines_total, 6);
        assert_eq!(player.grade, score_data::Grade::Failed);
        assert!(player.chart.step_artist.is_empty());
        assert!(player.chart.description.is_empty());
        assert!(player.chart.chart_name.is_empty());
        assert_eq!(player.scatter.len(), 1);
        assert!(!player.histogram.bins.is_empty());
        assert!((player.timing.mean_ms - 10.0).abs() <= f32::EPSILON);

        let course_page =
            score_info_from_stage(&summary, profile_data::PlayerSide::P1).expect("course page");
        let song_page =
            score_info_from_stage(&course.stage_summaries[0], profile_data::PlayerSide::P1)
                .expect("song page");
        assert!((course_page.score_percent - 0.5).abs() <= f64::EPSILON);
        assert!((song_page.score_percent - 1.0).abs() <= f64::EPSILON);
        assert!(!course_page.histogram.bins.is_empty());
        assert_eq!(course_page.scatter.len(), 1);
    }

    #[test]
    fn course_summary_merges_column_judgments_from_song_pages() {
        let song = test_song_with_duration("Songs/Test/course.ssc", "course", 120.0);
        let side = profile_data::PlayerSide::P2;
        let mut course_score = std::array::from_fn(|_| None);
        course_score[0] = Some(test_score_info(
            song.clone(),
            side,
            "course",
            ScrollSpeedSetting::default(),
            1.0,
        ));
        let mut course_page = evaluation::init_from_score_info(course_score, 120.0);

        let mut first = std::array::from_fn(|_| None);
        let mut first_p2 = test_score_info(
            song.clone(),
            side,
            "stage-a",
            ScrollSpeedSetting::default(),
            1.0,
        );
        first_p2.column_judgments = vec![
            evaluation::ColumnJudgments {
                w0: 1,
                w1: 2,
                early_w1: 1,
                early_total_w0: 1,
                held_miss: 1,
                ..Default::default()
            },
            evaluation::ColumnJudgments {
                w2: 3,
                miss: 1,
                early_w2: 2,
                early_total_w2: 2,
                ..Default::default()
            },
        ];
        first[0] = Some(first_p2);
        let mut ignored_p1 = test_score_info(
            song.clone(),
            profile_data::PlayerSide::P1,
            "ignored",
            ScrollSpeedSetting::default(),
            1.0,
        );
        ignored_p1.column_judgments = vec![evaluation::ColumnJudgments {
            w4: 1000,
            ..Default::default()
        }];
        first[1] = Some(ignored_p1);
        let first_page = evaluation::init_from_score_info(first, 60.0);

        let mut second = std::array::from_fn(|_| None);
        let mut second_p2 = test_score_info(
            song.clone(),
            side,
            "stage-b",
            ScrollSpeedSetting::default(),
            1.0,
        );
        second_p2.column_judgments = vec![
            evaluation::ColumnJudgments {
                w0: 4,
                w3: 5,
                early_w3: 1,
                early_total_w3: 1,
                held_miss: 2,
                ..Default::default()
            },
            evaluation::ColumnJudgments::default(),
            evaluation::ColumnJudgments {
                w5: 6,
                early_w5: 3,
                early_total_w5: 4,
                ..Default::default()
            },
        ];
        second[0] = Some(second_p2);
        let second_page = evaluation::init_from_score_info(second, 60.0);

        apply_course_summary_column_judgments(&mut course_page, &[first_page, second_page]);

        let columns = &course_page.score_info[0]
            .as_ref()
            .expect("course summary score")
            .column_judgments;
        assert_eq!(columns.len(), 3);
        assert_eq!(columns[0].w0, 5);
        assert_eq!(columns[0].w1, 2);
        assert_eq!(columns[0].w3, 5);
        assert_eq!(columns[0].w4, 0);
        assert_eq!(columns[0].early_w1, 1);
        assert_eq!(columns[0].early_w3, 1);
        assert_eq!(columns[0].early_total_w0, 1);
        assert_eq!(columns[0].early_total_w3, 1);
        assert_eq!(columns[0].held_miss, 3);
        assert_eq!(columns[1].w2, 3);
        assert_eq!(columns[1].miss, 1);
        assert_eq!(columns[1].early_w2, 2);
        assert_eq!(columns[1].early_total_w2, 2);
        assert_eq!(columns[2].w5, 6);
        assert_eq!(columns[2].early_w5, 3);
        assert_eq!(columns[2].early_total_w5, 4);
    }

    #[test]
    fn evaluation_restart_payload_uses_score_side_for_single_p2() {
        let song = Arc::new(test_song("Songs/Test/song.ssc", 0.0, ["p1", "p2"]));
        let mut score_info = std::array::from_fn(|_| None);
        score_info[0] = Some(test_score_info(
            song.clone(),
            profile_data::PlayerSide::P2,
            "p2hash",
            ScrollSpeedSetting::MMod(777.0),
            1.5,
        ));

        let (payload_song, chart_hashes, music_rate, scroll_speed) =
            restart_payload_from_eval(&score_info).expect("score info should restart");

        assert!(Arc::ptr_eq(&payload_song, &song));
        assert!(chart_hashes[0].is_empty());
        assert_eq!(chart_hashes[1], "p2hash");
        assert!((music_rate - 1.5).abs() < f32::EPSILON);
        assert_eq!(scroll_speed[0], ScrollSpeedSetting::default());
        assert_eq!(scroll_speed[1], ScrollSpeedSetting::MMod(777.0));
    }

    #[test]
    fn evaluation_summary_return_to_stays_in_select_music_for_set_summary() {
        assert_eq!(
            evaluation_summary_return_to(CurrentScreen::SelectMusic, false),
            CurrentScreen::SelectMusic,
        );
    }

    #[test]
    fn evaluation_summary_return_to_keeps_exit_flow_moving() {
        assert_eq!(
            evaluation_summary_return_to(CurrentScreen::SelectMusic, true),
            CurrentScreen::Initials,
        );
        assert_eq!(
            evaluation_summary_return_to(CurrentScreen::SelectCourse, true),
            CurrentScreen::Initials,
        );
    }
}
