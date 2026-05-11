mod commands;
mod dynamic_media;
mod graphics;
mod input_routing;
pub(crate) mod media_cache;
mod screen_nav;
mod screenshot;

use self::commands::Command;
use self::dynamic_media::DynamicMedia;
use self::input_routing::{GameplayQueuedEvent, gameplay_raw_key_event};
use self::screen_nav::TransitionState;
use self::screenshot::{ScreenshotPreviewState, should_auto_screenshot_eval};
use crate::act;
use crate::assets::{AssetManager, TextureUploadBudget, visual_styles};
use crate::config::{self, DisplayMode, dirs};
use crate::engine::display;
use crate::engine::gfx::{
    self as renderer, BackendType, PresentModePolicy, SamplerDesc, SamplerFilter, SamplerWrap,
};
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use crate::engine::host_time;
use crate::engine::input::{self, InputEvent};
use crate::engine::lights::{
    self, ButtonLight, CabinetLight, HideFlags, Mode as LightMode, Player as LightPlayer,
};
use crate::engine::present::color;
use crate::engine::space::{self as space, Metrics};
use crate::game::note::NoteType;
use crate::game::parsing::simfile as song_loading;
use crate::game::{profile, scores, scroll::ScrollSpeedSetting, stage_stats};
use crate::screens::{
    DensityGraphSlot, DensityGraphSource, Screen as CurrentScreen, ScreenAction,
    SongOffsetSyncChange, credits, evaluation, evaluation_summary, gameover, gameplay, init,
    initials, input as input_screen, manage_local_profiles, mappings, menu, options,
    player_options, practice, profile_load, sandbox, select_color, select_course, select_mode,
    select_music, select_profile, select_style,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::Window,
};

use log::{debug, error, info, trace, warn};
use std::borrow::Cow;
use std::cmp;
use std::collections::HashSet;
use std::{
    error::Error,
    ffi::OsString,
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

use crate::engine::present::actors::Actor;
/* -------------------- gamepad -------------------- */
use crate::engine::input::{GpSystemEvent, PadEvent};

/* -------------------- user events -------------------- */
#[derive(Debug, Clone)]
pub enum UserEvent {
    Pad(PadEvent),
    Key(input::RawKeyboardEvent),
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
const BACKGROUND_REDRAW_INTERVAL: Duration = Duration::from_millis(67);
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
const LIGHTS_AHEAD_NS: crate::game::gameplay::SongTimeNs = 50_000_000;
const LIGHTS_MAX_CATCHUP_NS: crate::game::gameplay::SongTimeNs = 500_000_000;

#[derive(Clone, Copy, Debug, Default)]
struct GameplayLightTracker {
    notes_ptr: usize,
    notes_len: usize,
    cursor: usize,
    last_time_ns: crate::game::gameplay::SongTimeNs,
}

impl GameplayLightTracker {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn queue_blinks(&mut self, lights: &mut lights::Manager, state: &crate::game::gameplay::State) {
        let now_ns = state.current_music_time_ns.saturating_add(LIGHTS_AHEAD_NS);
        let notes_ptr = state.notes.as_ptr() as usize;
        let reset = self.notes_ptr != notes_ptr
            || self.notes_len != state.notes.len()
            || now_ns < self.last_time_ns
            || now_ns.saturating_sub(self.last_time_ns) > LIGHTS_MAX_CATCHUP_NS;
        if reset {
            self.notes_ptr = notes_ptr;
            self.notes_len = state.notes.len();
            self.cursor = state.note_time_cache_ns.partition_point(|&t| t <= now_ns);
            self.last_time_ns = now_ns;
            return;
        }

        while self.cursor < state.notes.len() && state.note_time_cache_ns[self.cursor] <= now_ns {
            let note = &state.notes[self.cursor];
            if gameplay_note_lights(note) {
                blink_note_lights(lights, state, note.column);
            }
            self.cursor += 1;
        }
        self.last_time_ns = now_ns;
    }
}

fn gameplay_note_lights(note: &crate::game::note::Note) -> bool {
    note.can_be_judged
        && !note.is_fake
        && matches!(
            note.note_type,
            NoteType::Tap | NoteType::Hold | NoteType::Roll
        )
}

fn blink_note_lights(
    lights: &mut lights::Manager,
    state: &crate::game::gameplay::State,
    column: usize,
) {
    let Some(local_col) = physical_light_col(state, column) else {
        return;
    };
    if let Some(cabinet) = cabinet_light_for_col(local_col) {
        lights.blink_cabinet(cabinet);
    }
}

fn physical_light_col(state: &crate::game::gameplay::State, column: usize) -> Option<usize> {
    if state.cols_per_player == 0 {
        return None;
    }
    let local = column % state.cols_per_player;
    if state.cols_per_player >= 8 && state.num_players == 1 {
        return Some(local % 4);
    }
    (local < 4).then_some(local)
}

const fn cabinet_light_for_col(local_col: usize) -> Option<CabinetLight> {
    match local_col {
        0 => Some(CabinetLight::MarqueeUpperLeft),
        1 => Some(CabinetLight::MarqueeUpperRight),
        2 => Some(CabinetLight::MarqueeLowerLeft),
        3 => Some(CabinetLight::MarqueeLowerRight),
        _ => None,
    }
}

const fn light_mode_for_screen(screen: CurrentScreen) -> LightMode {
    match screen {
        CurrentScreen::Init => LightMode::Attract,
        CurrentScreen::Gameplay | CurrentScreen::Practice => LightMode::Gameplay,
        CurrentScreen::Evaluation | CurrentScreen::EvaluationSummary | CurrentScreen::Initials => {
            LightMode::Cleared
        }
        CurrentScreen::GameOver => LightMode::Stage,
        CurrentScreen::Menu
        | CurrentScreen::Options
        | CurrentScreen::Credits
        | CurrentScreen::ManageLocalProfiles
        | CurrentScreen::Mappings
        | CurrentScreen::Input
        | CurrentScreen::SelectProfile
        | CurrentScreen::SelectColor
        | CurrentScreen::SelectStyle
        | CurrentScreen::SelectPlayMode
        | CurrentScreen::ProfileLoad
        | CurrentScreen::SelectMusic
        | CurrentScreen::SelectCourse
        | CurrentScreen::Sandbox
        | CurrentScreen::PlayerOptions => LightMode::MenuStartAndDirections,
    }
}

const fn light_button_from_action(
    action: input::VirtualAction,
) -> Option<(LightPlayer, ButtonLight)> {
    match action {
        input::VirtualAction::p1_left | input::VirtualAction::p1_menu_left => {
            Some((LightPlayer::P1, ButtonLight::Left))
        }
        input::VirtualAction::p1_down | input::VirtualAction::p1_menu_down => {
            Some((LightPlayer::P1, ButtonLight::Down))
        }
        input::VirtualAction::p1_up | input::VirtualAction::p1_menu_up => {
            Some((LightPlayer::P1, ButtonLight::Up))
        }
        input::VirtualAction::p1_right | input::VirtualAction::p1_menu_right => {
            Some((LightPlayer::P1, ButtonLight::Right))
        }
        input::VirtualAction::p1_start => Some((LightPlayer::P1, ButtonLight::Start)),
        input::VirtualAction::p2_left | input::VirtualAction::p2_menu_left => {
            Some((LightPlayer::P2, ButtonLight::Left))
        }
        input::VirtualAction::p2_down | input::VirtualAction::p2_menu_down => {
            Some((LightPlayer::P2, ButtonLight::Down))
        }
        input::VirtualAction::p2_up | input::VirtualAction::p2_menu_up => {
            Some((LightPlayer::P2, ButtonLight::Up))
        }
        input::VirtualAction::p2_right | input::VirtualAction::p2_menu_right => {
            Some((LightPlayer::P2, ButtonLight::Right))
        }
        input::VirtualAction::p2_start => Some((LightPlayer::P2, ButtonLight::Start)),
        _ => None,
    }
}

fn hide_flags_for_gameplay(state: &crate::game::gameplay::State) -> [HideFlags; 2] {
    std::array::from_fn(|player| {
        hide_flags_from_profile(state.player_profiles[player].hide_light_type)
    })
}

const fn hide_flags_from_profile(hide: profile::HideLightType) -> HideFlags {
    match hide {
        profile::HideLightType::NoHideLights => HideFlags {
            all: false,
            marquee: false,
            bass: false,
        },
        profile::HideLightType::HideAllLights => HideFlags {
            all: true,
            marquee: true,
            bass: true,
        },
        profile::HideLightType::HideMarqueeLights => HideFlags {
            all: false,
            marquee: true,
            bass: false,
        },
        profile::HideLightType::HideBassLights => HideFlags {
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
    song: Arc<crate::game::song::SongData>,
    steps_index: [usize; crate::game::gameplay::MAX_PLAYERS],
    preferred_difficulty_index: [usize; crate::game::gameplay::MAX_PLAYERS],
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
    song_stub: Arc<crate::game::song::SongData>,
    stages: Vec<CourseStageRuntime>,
    course_display_totals:
        [crate::game::gameplay::CourseDisplayTotals; crate::game::gameplay::MAX_PLAYERS],
    next_stage_index: usize,
    stage_summaries: Vec<stage_stats::StageSummary>,
    stage_eval_pages: Vec<evaluation::State>,
}

#[derive(Clone, Copy)]
struct GameplayEventBatchTrace {
    started_at: Instant,
    gameplay_seen: bool,
    key_events: u32,
    key_repeat_events: u32,
    pad_events: u32,
    queued_events: u32,
    app_handler_sum_us: u64,
    app_handler_max_us: u32,
}

impl GameplayEventBatchTrace {
    #[inline(always)]
    fn new(now: Instant) -> Self {
        Self {
            started_at: now,
            gameplay_seen: false,
            key_events: 0,
            key_repeat_events: 0,
            pad_events: 0,
            queued_events: 0,
            app_handler_sum_us: 0,
            app_handler_max_us: 0,
        }
    }

    #[inline(always)]
    fn reset(&mut self, now: Instant) {
        *self = Self::new(now);
    }
}

#[derive(Clone, Copy)]
struct GameplayEventTrace {
    started_at: Instant,
    batches: u32,
    key_events: u32,
    key_repeat_events: u32,
    pad_events: u32,
    queued_events: u32,
    batch_sum_us: u64,
    batch_max_us: u32,
    app_handler_sum_us: u64,
    app_handler_max_us: u32,
    dispatch_overhead_sum_us: u64,
    dispatch_overhead_max_us: u32,
    slow_batches: u32,
}

impl GameplayEventTrace {
    #[inline(always)]
    fn new(now: Instant) -> Self {
        Self {
            started_at: now,
            batches: 0,
            key_events: 0,
            key_repeat_events: 0,
            pad_events: 0,
            queued_events: 0,
            batch_sum_us: 0,
            batch_max_us: 0,
            app_handler_sum_us: 0,
            app_handler_max_us: 0,
            dispatch_overhead_sum_us: 0,
            dispatch_overhead_max_us: 0,
            slow_batches: 0,
        }
    }

    #[inline(always)]
    fn reset(&mut self, now: Instant) {
        *self = Self::new(now);
    }
}

const STUTTER_SAMPLE_COUNT: usize = 5;
const STUTTER_SAMPLE_LIFETIME: f32 = 3.4;

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
fn seconds_to_us_u32(seconds: f32) -> u32 {
    let micros = (seconds * 1_000_000.0).max(0.0);
    if micros > u32::MAX as f32 {
        u32::MAX
    } else {
        micros as u32
    }
}

#[inline(always)]
fn stutter_severity(frame_seconds: f32, expected_seconds: f32) -> u8 {
    if expected_seconds <= 0.0 {
        return 0;
    }
    let thresholds = [expected_seconds * 2.0, expected_seconds * 4.0, 0.1];
    let mut severity: u8 = 0;
    while usize::from(severity) < thresholds.len()
        && frame_seconds > thresholds[usize::from(severity)]
    {
        severity = severity.saturating_add(1);
    }
    severity
}

#[inline(always)]
fn stutter_diag_enabled() -> bool {
    log::log_enabled!(log::Level::Trace)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayMode {
    Off,
    Fps,
    FpsAndStutter,
    FpsStutterTiming,
}

impl OverlayMode {
    #[inline(always)]
    const fn from_code(mode: u8) -> Self {
        match mode {
            1 => Self::Fps,
            2 => Self::FpsAndStutter,
            3 => Self::FpsStutterTiming,
            _ => Self::Off,
        }
    }

    #[inline(always)]
    const fn next(self) -> Self {
        match self {
            Self::Off => Self::Fps,
            Self::Fps => Self::FpsAndStutter,
            Self::FpsAndStutter => Self::FpsStutterTiming,
            Self::FpsStutterTiming => Self::Off,
        }
    }

    #[inline(always)]
    const fn shows_fps(self) -> bool {
        !matches!(self, Self::Off)
    }

    #[inline(always)]
    const fn shows_stutter(self) -> bool {
        matches!(self, Self::FpsAndStutter | Self::FpsStutterTiming)
    }

    #[inline(always)]
    const fn shows_timing(self) -> bool {
        matches!(self, Self::FpsStutterTiming)
    }

    #[inline(always)]
    const fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Fps => "FPS",
            Self::FpsAndStutter => "FPS+STUTTER",
            Self::FpsStutterTiming => "FPS+STUTTER+TIMING",
        }
    }

    #[inline(always)]
    const fn code(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Fps => 1,
            Self::FpsAndStutter => 2,
            Self::FpsStutterTiming => 3,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct StutterSample {
    at_seconds: f32,
    frame_seconds: f32,
    expected_seconds: f32,
    severity: u8,
}

impl StutterSample {
    #[inline(always)]
    const fn empty() -> Self {
        Self {
            at_seconds: -1.0,
            frame_seconds: 0.0,
            expected_seconds: 0.0,
            severity: 0,
        }
    }
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

#[derive(Clone, Copy)]
struct StutterDiagRecorder {
    frames: [StutterDiagFrameSample; STUTTER_DIAG_FRAME_SAMPLE_COUNT],
    frame_cursor: usize,
    frame_len: usize,
    last_audio_trigger_seq: u64,
    last_display_trigger_seq: u64,
    last_dump_host_nanos: u64,
}

impl StutterDiagRecorder {
    #[inline(always)]
    const fn new() -> Self {
        Self {
            frames: [StutterDiagFrameSample::empty(); STUTTER_DIAG_FRAME_SAMPLE_COUNT],
            frame_cursor: 0,
            frame_len: 0,
            last_audio_trigger_seq: 0,
            last_display_trigger_seq: 0,
            last_dump_host_nanos: 0,
        }
    }

    #[inline(always)]
    fn reset(&mut self) {
        self.frames = [StutterDiagFrameSample::empty(); STUTTER_DIAG_FRAME_SAMPLE_COUNT];
        self.frame_cursor = 0;
        self.frame_len = 0;
        self.last_dump_host_nanos = 0;
    }

    #[inline(always)]
    fn push_frame(&mut self, sample: StutterDiagFrameSample) {
        self.frames[self.frame_cursor] = sample;
        self.frame_cursor = (self.frame_cursor + 1) % STUTTER_DIAG_FRAME_SAMPLE_COUNT;
        self.frame_len = self
            .frame_len
            .saturating_add(1)
            .min(STUTTER_DIAG_FRAME_SAMPLE_COUNT);
    }

    fn collect_recent_frames(
        &self,
        now_host_nanos: u64,
        window_ns: u64,
        out: &mut Vec<StutterDiagFrameSample>,
    ) {
        let start = self
            .frame_cursor
            .saturating_add(STUTTER_DIAG_FRAME_SAMPLE_COUNT)
            .saturating_sub(self.frame_len)
            % STUTTER_DIAG_FRAME_SAMPLE_COUNT;
        for i in 0..self.frame_len {
            let sample = self.frames[(start + i) % STUTTER_DIAG_FRAME_SAMPLE_COUNT];
            if sample.host_nanos == 0 {
                continue;
            }
            if now_host_nanos.saturating_sub(sample.host_nanos) <= window_ns {
                out.push(sample);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ActorTreeStats {
    total: u32,
    sprites: u32,
    texts: u32,
    meshes: u32,
    textured_meshes: u32,
    frames: u32,
    cameras: u32,
    shadows: u32,
    text_chars: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct ComposeBreakdown {
    actor_build_us: u32,
    build_screen_us: u32,
    resolve_textures_us: u32,
    render_objects: u32,
    render_cameras: u32,
    text_layout: crate::engine::present::compose::TextLayoutFrameStats,
}

#[inline(always)]
const fn saturating_u32(value: usize) -> u32 {
    if value > u32::MAX as usize {
        u32::MAX
    } else {
        value as u32
    }
}

fn actor_tree_stats(actors: &[crate::engine::present::actors::Actor]) -> ActorTreeStats {
    fn visit(stats: &mut ActorTreeStats, actor: &crate::engine::present::actors::Actor) {
        stats.total = stats.total.saturating_add(1);
        match actor {
            crate::engine::present::actors::Actor::Sprite { .. } => {
                stats.sprites = stats.sprites.saturating_add(1);
            }
            crate::engine::present::actors::Actor::Text { content, .. } => {
                stats.texts = stats.texts.saturating_add(1);
                stats.text_chars = stats
                    .text_chars
                    .saturating_add(saturating_u32(content.len()));
            }
            crate::engine::present::actors::Actor::Mesh { .. } => {
                stats.meshes = stats.meshes.saturating_add(1);
            }
            crate::engine::present::actors::Actor::TexturedMesh { .. } => {
                stats.textured_meshes = stats.textured_meshes.saturating_add(1);
            }
            crate::engine::present::actors::Actor::Frame { children, .. } => {
                stats.frames = stats.frames.saturating_add(1);
                for child in children {
                    visit(stats, child);
                }
            }
            crate::engine::present::actors::Actor::SharedFrame { children, .. } => {
                stats.frames = stats.frames.saturating_add(1);
                for child in children.iter() {
                    visit(stats, child);
                }
            }
            crate::engine::present::actors::Actor::Camera { children, .. } => {
                stats.cameras = stats.cameras.saturating_add(1);
                for child in children {
                    visit(stats, child);
                }
            }
            crate::engine::present::actors::Actor::CameraPush { .. } => {
                stats.cameras = stats.cameras.saturating_add(1);
            }
            crate::engine::present::actors::Actor::CameraPop => {}
            crate::engine::present::actors::Actor::Shadow { child, .. } => {
                stats.shadows = stats.shadows.saturating_add(1);
                visit(stats, child);
            }
        }
    }

    let mut stats = ActorTreeStats::default();
    for actor in actors {
        visit(&mut stats, actor);
    }
    stats
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameIntervalReason {
    None,
    MaxFps,
    Background,
    MaxFpsBackground,
}

impl FrameIntervalReason {
    #[inline(always)]
    const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::MaxFps => "max_fps",
            Self::Background => "background",
            Self::MaxFpsBackground => "max_fps+background",
        }
    }

    #[inline(always)]
    const fn redraw_reason(self) -> &'static str {
        match self {
            Self::None => "scheduled",
            Self::MaxFps => "scheduled_maxfps",
            Self::Background => "scheduled_background",
            Self::MaxFpsBackground => "scheduled_maxfps_background",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrameIntervalState {
    interval: Option<Duration>,
    reason: FrameIntervalReason,
}

#[inline(always)]
const fn should_background_throttle_unfocused(screen: CurrentScreen) -> bool {
    !matches!(screen, CurrentScreen::Gameplay | CurrentScreen::Practice)
}

#[inline(always)]
const fn foreground_input_active(window_focused: bool, surface_active: bool) -> bool {
    window_focused && surface_active
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameLoopMode {
    Poll,
    WaitPending,
    Scheduled(FrameIntervalReason, Duration),
}

#[derive(Clone, Copy)]
struct GameplayPacingTrace {
    started_at: Instant,
    frames: u32,
    chain_frames: u32,
    other_frames: u32,
    dt_sum_us: u64,
    dt_max_us: u32,
    redraw_late_sum_us: u64,
    redraw_late_max_us: u32,
    redraw_delivery_sum_us: u64,
    redraw_delivery_max_us: u32,
    redraw_delivery_over_1ms: u32,
    redraw_delivery_over_2ms: u32,
    draw_sum_us: u64,
    draw_max_us: u32,
    present_sum_us: u64,
    present_max_us: u32,
    present_over_1ms: u32,
    present_over_3ms: u32,
    draw_setup_sum_us: u64,
    draw_prepare_sum_us: u64,
    draw_record_sum_us: u64,
    display_error_abs_sum_us: u64,
    display_error_abs_max_us: u32,
    display_error_last_us: i32,
    display_catching_up_frames: u32,
    display_catching_up_last: bool,
    present_last_mode: renderer::PresentModeTrace,
    present_display_clock_last: renderer::ClockDomainTrace,
    present_host_clock_last: renderer::ClockDomainTrace,
    present_inflight_sum: u64,
    present_inflight_max: u8,
    present_image_wait_frames: u32,
    present_back_pressure_frames: u32,
    present_queue_idle_frames: u32,
    present_suboptimal_frames: u32,
    present_host_mapped_frames: u32,
    present_calibration_error_sum_ns: u64,
    present_calibration_error_max_ns: u64,
    present_interval_sum_ns: u64,
    present_interval_max_ns: u64,
    present_interval_samples: u32,
    present_margin_sum_ns: u64,
    present_margin_max_ns: u64,
    present_margin_samples: u32,
}

impl GameplayPacingTrace {
    #[inline(always)]
    fn new(now: Instant) -> Self {
        Self {
            started_at: now,
            frames: 0,
            chain_frames: 0,
            other_frames: 0,
            dt_sum_us: 0,
            dt_max_us: 0,
            redraw_late_sum_us: 0,
            redraw_late_max_us: 0,
            redraw_delivery_sum_us: 0,
            redraw_delivery_max_us: 0,
            redraw_delivery_over_1ms: 0,
            redraw_delivery_over_2ms: 0,
            draw_sum_us: 0,
            draw_max_us: 0,
            present_sum_us: 0,
            present_max_us: 0,
            present_over_1ms: 0,
            present_over_3ms: 0,
            draw_setup_sum_us: 0,
            draw_prepare_sum_us: 0,
            draw_record_sum_us: 0,
            display_error_abs_sum_us: 0,
            display_error_abs_max_us: 0,
            display_error_last_us: 0,
            display_catching_up_frames: 0,
            display_catching_up_last: false,
            present_last_mode: renderer::PresentModeTrace::Unknown,
            present_display_clock_last: renderer::ClockDomainTrace::Unknown,
            present_host_clock_last: renderer::ClockDomainTrace::Unknown,
            present_inflight_sum: 0,
            present_inflight_max: 0,
            present_image_wait_frames: 0,
            present_back_pressure_frames: 0,
            present_queue_idle_frames: 0,
            present_suboptimal_frames: 0,
            present_host_mapped_frames: 0,
            present_calibration_error_sum_ns: 0,
            present_calibration_error_max_ns: 0,
            present_interval_sum_ns: 0,
            present_interval_max_ns: 0,
            present_interval_samples: 0,
            present_margin_sum_ns: 0,
            present_margin_max_ns: 0,
            present_margin_samples: 0,
        }
    }

    #[inline(always)]
    fn reset(&mut self, now: Instant) {
        *self = Self::new(now);
    }
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
    pending_redraw_request_at: Option<Instant>,
    pending_redraw_request_reason: &'static str,
    last_logged_frame_loop_mode: Option<FrameLoopMode>,
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
    stutter_samples: [StutterSample; STUTTER_SAMPLE_COUNT],
    stutter_cursor: usize,
    stutter_diag: StutterDiagRecorder,
    transition: TransitionState,
    display_width: u32,
    display_height: u32,
    pending_window_position: Option<PhysicalPosition<i32>>,
    gamepad_overlay_state: Option<(String, Instant)>,
    pending_exit: bool,
    shift_held: bool,
    ctrl_held: bool,
    tab_held: bool,
    backquote_held: bool,
    tab_acceleration_enabled: bool,
    window_focused: bool,
    window_occluded: bool,
    surface_active: bool,
    screenshot_pending: bool,
    screenshot_request_side: Option<profile::PlayerSide>,
    screenshot_flash_started_at: Option<Instant>,
    screenshot_preview: Option<ScreenshotPreviewState>,
}

/// Hold-Tab fast-forward / hold-` slow-down multipliers, ITGmania parity.
///
/// `Tab` alone → `TAB_FAST_MULTIPLIER`× engine update rate.
/// `` ` `` alone → `1.0 / TAB_SLOW_DIVISOR`× rate.
/// Both held → `0.0` (halt).
///
/// Always returns `1.0` on `CurrentScreen::Gameplay` (timing-sensitive,
/// per issue #174) or when `enabled` is false.
const TAB_FAST_MULTIPLIER: f32 = 4.0;
const TAB_SLOW_DIVISOR: f32 = 4.0;
/// Upper bound on the post-acceleration logic dt fed to screens.
/// Prevents catastrophic spikes (e.g. the app stalled for several seconds
/// and the user happens to be holding Tab) from injecting absurd dt values
/// into per-screen `update(dt)` accumulators.
const MAX_LOGIC_DT_PER_FRAME: f32 = 0.25;

#[inline]
fn apply_tab_acceleration(
    wall_dt: f32,
    screen: CurrentScreen,
    fast: bool,
    slow: bool,
    enabled: bool,
) -> f32 {
    if !enabled || matches!(screen, CurrentScreen::Gameplay | CurrentScreen::Practice) {
        return wall_dt;
    }
    let scaled = match (fast, slow) {
        (true, true) => 0.0,
        (true, false) => wall_dt * TAB_FAST_MULTIPLIER,
        (false, true) => wall_dt / TAB_SLOW_DIVISOR,
        (false, false) => wall_dt,
    };
    scaled.clamp(0.0, MAX_LOGIC_DT_PER_FRAME)
}

#[inline(always)]
fn frame_interval_for_max_fps(max_fps: u16) -> Option<Duration> {
    if max_fps == 0 {
        None
    } else {
        Some(Duration::from_secs_f64(1.0 / f64::from(max_fps)))
    }
}

#[inline(always)]
fn advance_redraw_deadline(deadline: Instant, now: Instant, interval: Duration) -> Instant {
    if deadline > now {
        return deadline;
    }
    let step_ns = interval.as_nanos();
    if step_ns == 0 {
        return now;
    }
    let overdue_ns = now.duration_since(deadline).as_nanos();
    let steps = overdue_ns / step_ns + 1;
    if steps <= u128::from(u32::MAX)
        && let Some(delta) = interval.checked_mul(steps as u32)
        && let Some(next) = deadline.checked_add(delta)
    {
        return next;
    }
    now.checked_add(interval).unwrap_or(now)
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
    player_options_state: Option<player_options::State>,
    init_state: init::State,
    select_profile_state: select_profile::State,
    select_color_state: select_color::State,
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
    combo_carry: [u32; crate::game::gameplay::MAX_PLAYERS],
    gameplay_restart_count: u32,
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
        let frame_interval = frame_interval_for_max_fps(cfg.max_fps);
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
            pending_redraw_request_at: None,
            pending_redraw_request_reason: "none",
            last_logged_frame_loop_mode: None,
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
            stutter_samples: [StutterSample::empty(); STUTTER_SAMPLE_COUNT],
            stutter_cursor: 0,
            stutter_diag: StutterDiagRecorder::new(),
            transition: TransitionState::Idle,
            display_width: cfg.display_width,
            display_height: cfg.display_height,
            display_monitor: cfg.display_monitor,
            pending_window_position: None,
            gamepad_overlay_state: None,
            pending_exit: false,
            shift_held: false,
            ctrl_held: false,
            tab_held: false,
            backquote_held: false,
            tab_acceleration_enabled: cfg.tab_acceleration,
            // Default to unfocused so background input backends (Win32 RawInput,
            // evdev, IOHID) drop globally-observed key events until the window
            // is created and proven focused.
            window_focused: false,
            window_occluded: false,
            surface_active: cfg.display_width > 0 && cfg.display_height > 0,
            screenshot_pending: false,
            screenshot_request_side: None,
            screenshot_flash_started_at: None,
            screenshot_preview: None,
        }
    }

    #[inline(always)]
    fn set_max_fps(&mut self, max_fps: u16) {
        self.frame_interval = frame_interval_for_max_fps(max_fps);
        self.next_redraw_at = Instant::now();
        self.last_logged_frame_loop_mode = None;
    }

    #[inline(always)]
    fn set_present_mode_policy(&mut self, policy: PresentModePolicy) {
        self.present_mode_policy = policy;
        self.next_redraw_at = Instant::now();
        self.last_logged_frame_loop_mode = None;
    }

    #[inline(always)]
    fn reset_frame_clock(&mut self, now: Instant) {
        self.last_frame_time = now;
        self.last_frame_end_time = now;
        self.next_redraw_at = now;
        self.pending_redraw_request_at = None;
        self.pending_redraw_request_reason = "none";
        self.last_logged_frame_loop_mode = None;
        self.gameplay_pacing_trace.reset(now);
        self.gameplay_event_batch_trace.reset(now);
        self.gameplay_event_trace.reset(now);
        self.stutter_diag.reset();
    }

    #[inline(always)]
    fn note_redraw_requested(&mut self, now: Instant, reason: &'static str) {
        if self.pending_redraw_request_at.is_none() {
            self.pending_redraw_request_at = Some(now);
            self.pending_redraw_request_reason = reason;
        }
    }

    #[inline(always)]
    fn take_redraw_request_timing(&mut self, now: Instant) -> (u32, &'static str) {
        let requested_at = self.pending_redraw_request_at.take();
        let reason = if requested_at.is_some() {
            self.pending_redraw_request_reason
        } else {
            "external"
        };
        self.pending_redraw_request_reason = "none";
        (
            requested_at
                .map(|at| elapsed_us_between(now, at))
                .unwrap_or_default(),
            reason,
        )
    }

    #[inline(always)]
    fn redraw_pending(&self) -> bool {
        self.pending_redraw_request_at.is_some()
    }

    #[inline(always)]
    fn frame_interval_state(&self, screen: CurrentScreen) -> FrameIntervalState {
        let base = (!self.vsync_enabled)
            .then_some(self.frame_interval)
            .flatten();
        let background = (self.window_occluded
            || !self.surface_active
            || (!self.window_focused && should_background_throttle_unfocused(screen)))
        .then_some(BACKGROUND_REDRAW_INTERVAL);
        match (base, background) {
            (Some(base), Some(background)) => FrameIntervalState {
                interval: Some(cmp::max(base, background)),
                reason: FrameIntervalReason::MaxFpsBackground,
            },
            (Some(interval), None) => FrameIntervalState {
                interval: Some(interval),
                reason: FrameIntervalReason::MaxFps,
            },
            (None, Some(interval)) => FrameIntervalState {
                interval: Some(interval),
                reason: FrameIntervalReason::Background,
            },
            (None, None) => FrameIntervalState {
                interval: None,
                reason: FrameIntervalReason::None,
            },
        }
    }

    #[inline(always)]
    fn note_frame_loop_mode(&mut self, mode: FrameLoopMode) -> bool {
        if self.last_logged_frame_loop_mode == Some(mode) {
            return false;
        }
        self.last_logged_frame_loop_mode = Some(mode);
        true
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
        self.window_occluded || !self.surface_active
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
    fn push_stutter_sample(
        &mut self,
        at_seconds: f32,
        frame_seconds: f32,
        expected_seconds: f32,
        severity: u8,
    ) {
        self.stutter_samples[self.stutter_cursor] = StutterSample {
            at_seconds,
            frame_seconds,
            expected_seconds,
            severity,
        };
        self.stutter_cursor = (self.stutter_cursor + 1) % STUTTER_SAMPLE_COUNT;
    }

    #[inline(always)]
    fn clear_stutter_samples(&mut self) {
        self.stutter_samples = [StutterSample::empty(); STUTTER_SAMPLE_COUNT];
        self.stutter_cursor = 0;
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
    fn new(
        preferred_difficulty_index: usize,
        combo_carry: [u32; crate::game::gameplay::MAX_PLAYERS],
    ) -> Self {
        Self {
            preferred_difficulty_index,
            session_start_time: None,
            played_stages: Vec::new(),
            pending_post_select_summary_exit: false,
            course_individual_stage_indices: Vec::new(),
            combo_carry,
            gameplay_restart_count: 0,
            course_run: None,
            course_eval_pages: Vec::new(),
            course_eval_page_index: 0,
            last_course_wheel_path: None,
            last_course_wheel_difficulty_name: None,
        }
    }
}

#[inline(always)]
const fn side_ix(side: profile::PlayerSide) -> usize {
    match side {
        profile::PlayerSide::P1 => 0,
        profile::PlayerSide::P2 => 1,
    }
}

#[inline(always)]
fn combo_carry_from_profiles() -> [u32; crate::game::gameplay::MAX_PLAYERS] {
    [
        profile::get_for_side(profile::PlayerSide::P1).current_combo,
        profile::get_for_side(profile::PlayerSide::P2).current_combo,
    ]
}

fn course_stage_runtime_from_plan(
    plan: &select_course::CourseStagePlan,
    chart_type: &str,
) -> Option<CourseStageRuntime> {
    let steps_idx = select_music::steps_index_for_chart_hash(
        plan.song.as_ref(),
        chart_type,
        plan.chart_hash.as_str(),
    )?;
    Some(CourseStageRuntime {
        song: plan.song.clone(),
        steps_index: [steps_idx; crate::game::gameplay::MAX_PLAYERS],
        preferred_difficulty_index: [steps_idx; crate::game::gameplay::MAX_PLAYERS],
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
    let mut course_display_totals =
        [crate::game::gameplay::CourseDisplayTotals::default(); crate::game::gameplay::MAX_PLAYERS];
    for stage in &stages {
        for (player_idx, total) in course_display_totals.iter_mut().enumerate() {
            let Some(chart) = select_music::chart_for_steps_index(
                stage.song.as_ref(),
                chart_type,
                stage.steps_index[player_idx],
            ) else {
                continue;
            };
            let add = crate::game::gameplay::course_display_totals_for_chart(chart);
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

#[inline(always)]
fn merge_window_counts(
    mut total: crate::game::timing::WindowCounts,
    add: crate::game::timing::WindowCounts,
) -> crate::game::timing::WindowCounts {
    total.w0 = total.w0.saturating_add(add.w0);
    total.w1 = total.w1.saturating_add(add.w1);
    total.w2 = total.w2.saturating_add(add.w2);
    total.w3 = total.w3.saturating_add(add.w3);
    total.w4 = total.w4.saturating_add(add.w4);
    total.w5 = total.w5.saturating_add(add.w5);
    total.miss = total.miss.saturating_add(add.miss);
    total
}

fn build_course_summary_stage(course: &CourseRunState) -> Option<stage_stats::StageSummary> {
    if course.stage_summaries.is_empty() {
        return None;
    }
    let mut summary_song = (*course.song_stub).clone();
    summary_song.simfile_path.clone_from(&course.path);
    summary_song.title.clone_from(&course.name);
    summary_song.translit_title.clone_from(&course.name);
    summary_song.banner_path.clone_from(&course.banner_path);
    let duration_seconds: f32 = course
        .stage_summaries
        .iter()
        .map(|stage| stage.duration_seconds.max(0.0))
        .sum();
    summary_song.music_length_seconds = duration_seconds;
    summary_song.total_length_seconds = duration_seconds.round() as i32;
    let summary_song = Arc::new(summary_song);

    let mut players: [Option<stage_stats::PlayerStageSummary>; crate::game::gameplay::MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
        let idx = side_ix(side);
        let mut weighted_score = 0.0_f64;
        let mut weighted_ex = 0.0_f64;
        let mut weighted_hard_ex = 0.0_f64;
        let mut weight_sum = 0.0_f64;
        let mut notes_hit: u32 = 0;
        let mut calories_burned = 0.0_f32;
        let mut meter_sum = 0u32;
        let mut meter_count = 0u32;
        let mut any_failed = false;
        let mut score_valid = true;
        let mut disqualified = false;
        let mut show_w0 = false;
        let mut show_fa_plus_pane = false;
        let mut show_ex = false;
        let mut show_hard_ex = false;
        let mut track_early_judgments = false;
        let mut counts = crate::game::timing::WindowCounts::default();
        let mut counts_10ms = crate::game::timing::WindowCounts::default();
        let mut first_player: Option<&stage_stats::PlayerStageSummary> = None;
        for stage in &course.stage_summaries {
            let Some(player) = stage.players[idx].as_ref() else {
                continue;
            };
            if first_player.is_none() {
                first_player = Some(player);
            }
            let weight = player.notes_hit.max(1) as f64;
            weighted_score += player.score_percent * weight;
            weighted_ex += player.ex_score_percent * weight;
            weighted_hard_ex += player.hard_ex_score_percent * weight;
            weight_sum += weight;
            notes_hit = notes_hit.saturating_add(player.notes_hit);
            calories_burned += player.calories_burned.max(0.0);
            meter_sum = meter_sum.saturating_add(player.chart.meter);
            meter_count = meter_count.saturating_add(1);
            any_failed |= player.grade == scores::Grade::Failed;
            score_valid &= player.score_valid;
            disqualified |= player.disqualified;
            show_w0 |= player.show_w0;
            show_fa_plus_pane |= player.show_fa_plus_pane;
            show_ex |= player.show_ex_score;
            show_hard_ex |= player.show_hard_ex_score;
            track_early_judgments |= player.track_early_judgments;
            counts = merge_window_counts(counts, player.window_counts);
            counts_10ms = merge_window_counts(counts_10ms, player.window_counts_10ms);
        }
        let Some(first_player) = first_player else {
            continue;
        };
        let score_percent = if weight_sum > 0.0 {
            (weighted_score / weight_sum).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let ex_score_percent = if weight_sum > 0.0 {
            (weighted_ex / weight_sum).clamp(0.0, 100.0)
        } else {
            0.0
        };
        let hard_ex_score_percent = if weight_sum > 0.0 {
            (weighted_hard_ex / weight_sum).clamp(0.0, 100.0)
        } else {
            0.0
        };
        let mut grade = if any_failed {
            scores::Grade::Failed
        } else {
            scores::score_to_grade(score_percent * 10000.0)
        };
        grade = scores::promote_quint_grade(grade, ex_score_percent);
        let mut summary_chart = (*first_player.chart).clone();
        summary_chart.short_hash.clone_from(&course.score_hash);
        summary_chart
            .difficulty
            .clone_from(&course.course_difficulty_name);
        summary_chart.description.clone_from(&course.name);
        summary_chart.meter = course.course_meter.unwrap_or_else(|| {
            if meter_count > 0 {
                (meter_sum as f32 / meter_count as f32).round() as u32
            } else {
                summary_chart.meter
            }
        });
        players[idx] = Some(stage_stats::PlayerStageSummary {
            profile_name: first_player.profile_name.clone(),
            chart: Arc::new(summary_chart),
            score_valid,
            disqualified,
            groovestats: scores::GrooveStatsEvalState {
                valid: false,
                reason_lines: vec!["GrooveStats QR is unavailable in course mode.".to_string()],
                manual_qr_url: None,
            },
            itl: scores::ItlEvalState::default(),
            grade,
            score_percent,
            ex_score_percent,
            hard_ex_score_percent,
            notes_hit,
            calories_burned,
            window_counts: counts,
            window_counts_10ms: counts_10ms,
            show_w0,
            show_fa_plus_pane,
            show_ex_score: show_ex,
            show_hard_ex_score: show_hard_ex,
            track_early_judgments,
        });
    }

    let music_rate = course
        .stage_summaries
        .last()
        .map(|s| s.music_rate)
        .unwrap_or(1.0);
    Some(stage_stats::StageSummary {
        song: summary_song,
        music_rate,
        duration_seconds,
        players,
    })
}

fn score_info_from_stage(
    stage: &stage_stats::StageSummary,
    side: profile::PlayerSide,
) -> Option<evaluation::ScoreInfo> {
    let idx = side_ix(side);
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
        scores::leaderboard_rank_for_score(machine_records.as_slice(), player.score_percent);
    let personal_record_highlight_rank =
        scores::leaderboard_rank_for_score(personal_records.as_slice(), player.score_percent);
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
        grade: player.grade,
        speed_mod: profile::get_for_side(side).scroll_speed,
        mods_text: fallback_eval_mods_text(side, profile::get_for_side(side).scroll_speed),
        hands_achieved: 0,
        hands_total: 0,
        holds_held: 0,
        holds_total: 0,
        rolls_held: 0,
        rolls_total: 0,
        mines_avoided: 0,
        mines_total: 0,
        timing: crate::game::timing::TimingStats::default(),
        scatter: Vec::new(),
        scatter_worst_window_ms: 45.0,
        histogram: crate::game::timing::HistogramMs::default(),
        graph_first_second: 0.0,
        graph_last_second: stage.song.precise_last_second(),
        music_rate: if stage.music_rate.is_finite() && stage.music_rate > 0.0 {
            stage.music_rate
        } else {
            1.0
        },
        life_history: Vec::new(),
        fail_time: (player.grade == scores::Grade::Failed).then_some(stage.duration_seconds),
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
        machine_records,
        machine_record_highlight_rank,
        personal_records,
        personal_record_highlight_rank,
        show_machine_personal_split: !earned_machine_record && earned_top2_personal,
    })
}

fn fallback_eval_mods_text(side: profile::PlayerSide, speed_mod: ScrollSpeedSetting) -> Arc<str> {
    let profile = profile::get_for_side(side);
    let mut parts = vec![speed_mod.to_string()];
    if profile.mini_percent != 0 {
        parts.push(format!("{}% Mini", profile.mini_percent));
    }
    if profile.spacing_percent != 0 {
        parts.push(format!("{}% Spacing", profile.spacing_percent));
    }
    let scroll = profile.scroll_option;
    if scroll.contains(profile::ScrollOption::Reverse) {
        parts.push("Reverse".to_string());
    }
    if scroll.contains(profile::ScrollOption::Split) {
        parts.push("Split".to_string());
    }
    if scroll.contains(profile::ScrollOption::Alternate) {
        parts.push("Alternate".to_string());
    }
    if scroll.contains(profile::ScrollOption::Cross) {
        parts.push("Cross".to_string());
    }
    if scroll.contains(profile::ScrollOption::Centered) {
        parts.push("Centered".to_string());
    }
    parts.push(profile.perspective.to_string());
    parts.push(profile.noteskin.to_string());
    Arc::<str>::from(parts.join(", "))
}

fn build_course_summary_eval_state(
    stage: &stage_stats::StageSummary,
    active_color_index: i32,
    session_elapsed: f32,
    gameplay_elapsed: f32,
) -> evaluation::State {
    let mut score_info: [Option<evaluation::ScoreInfo>; crate::game::gameplay::MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
        score_info[side_ix(side)] = score_info_from_stage(stage, side);
    }
    let mut state = evaluation::init_from_score_info(score_info, stage.duration_seconds);
    state.active_color_index = active_color_index;
    state.session_elapsed = session_elapsed;
    state.gameplay_elapsed = gameplay_elapsed;
    state.return_to_course = true;
    state.allow_online_panes = false;
    state
}

fn push_song_lua_video_paths<'a>(
    overlays: &'a [crate::game::parsing::song_lua::SongLuaOverlayActor],
    seen: &mut HashSet<&'a str>,
    paths: &mut Vec<PathBuf>,
) {
    for overlay in overlays {
        let crate::game::parsing::song_lua::SongLuaOverlayKind::Sprite {
            texture_path,
            texture_key,
        } = &overlay.kind
        else {
            continue;
        };
        if !crate::assets::dynamic::is_dynamic_video_path(texture_path) {
            continue;
        }
        if !overlay.initial_state.decode_movie {
            continue;
        }
        if seen.insert(texture_key.as_ref()) {
            paths.push(texture_path.clone());
        }
    }
}

#[cfg(test)]
fn song_lua_video_paths(
    overlays: &[crate::game::parsing::song_lua::SongLuaOverlayActor],
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    push_song_lua_video_paths(overlays, &mut seen, &mut paths);
    paths
}

fn gameplay_song_lua_video_paths(state: &gameplay::State) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    push_song_lua_video_paths(&state.song_lua_overlays, &mut seen, &mut paths);
    for layer in &state.song_lua_background_visual_layers {
        push_song_lua_video_paths(&layer.overlays, &mut seen, &mut paths);
    }
    for layer in &state.song_lua_foreground_visual_layers {
        push_song_lua_video_paths(&layer.overlays, &mut seen, &mut paths);
    }
    paths
}

fn gameplay_overlay_video_paths(state: &gameplay::State) -> Vec<PathBuf> {
    let mut paths = gameplay_song_lua_video_paths(state);
    if let Some(path) = state.song.active_foreground_path(state.current_beat)
        && crate::assets::dynamic::is_dynamic_video_path(path)
        && !paths.iter().any(|existing| existing == path)
    {
        paths.push(path.clone());
    }
    paths
}

fn model_texture_sampler(key: &str) -> SamplerDesc {
    SamplerDesc {
        wrap: SamplerWrap::Repeat,
        ..crate::assets::parse_texture_hints(key).sampler_desc()
    }
}

fn prewarm_gameplay_assets(
    assets: &mut AssetManager,
    backend: &mut renderer::Backend,
    state: &gameplay::State,
) {
    fn song_lua_overlay_sampler(
        overlay: &crate::game::parsing::song_lua::SongLuaOverlayActor,
    ) -> SamplerDesc {
        let uses_repeat_state = |state: &crate::game::parsing::song_lua::SongLuaOverlayState| {
            state.texture_wrapping
                || state
                    .texcoord_offset
                    .is_some_and(|[u, v]| u.abs() > f32::EPSILON || v.abs() > f32::EPSILON)
                || state
                    .custom_texture_rect
                    .is_some_and(|[u0, v0, u1, v1]| u0 < 0.0 || v0 < 0.0 || u1 > 1.0 || v1 > 1.0)
                || state.texcoord_velocity.is_some()
        };
        let uses_repeat_delta =
            |delta: &crate::game::parsing::song_lua::SongLuaOverlayStateDelta| {
                delta.texture_wrapping == Some(true)
                    || delta
                        .texcoord_offset
                        .is_some_and(|[u, v]| u.abs() > f32::EPSILON || v.abs() > f32::EPSILON)
                    || delta.custom_texture_rect.is_some_and(|[u0, v0, u1, v1]| {
                        u0 < 0.0 || v0 < 0.0 || u1 > 1.0 || v1 > 1.0
                    })
                    || delta.texcoord_velocity.is_some()
            };
        let uses_nearest_state =
            |state: &crate::game::parsing::song_lua::SongLuaOverlayState| !state.texture_filtering;
        let uses_nearest_delta =
            |delta: &crate::game::parsing::song_lua::SongLuaOverlayStateDelta| {
                delta.texture_filtering == Some(false)
            };
        let uses_repeat = uses_repeat_state(&overlay.initial_state)
            || overlay
                .message_commands
                .iter()
                .flat_map(|command| command.blocks.iter())
                .any(|block| uses_repeat_delta(&block.delta));
        let uses_nearest = uses_nearest_state(&overlay.initial_state)
            || overlay
                .message_commands
                .iter()
                .flat_map(|command| command.blocks.iter())
                .any(|block| uses_nearest_delta(&block.delta));
        SamplerDesc {
            filter: if uses_nearest {
                SamplerFilter::Nearest
            } else {
                SamplerFilter::Linear
            },
            wrap: if uses_repeat {
                SamplerWrap::Repeat
            } else {
                SamplerWrap::Clamp
            },
            ..SamplerDesc::default()
        }
    }

    fn gameplay_media_paths(state: &gameplay::State) -> Vec<&PathBuf> {
        let mut paths = Vec::with_capacity(
            1usize
                .saturating_add(state.song.background_changes.len())
                .saturating_add(state.song.foreground_changes.len()),
        );
        if let Some(path) = state.song.background_path.as_ref() {
            paths.push(path);
        }
        for change in &state.song.background_changes {
            let crate::game::song::SongBackgroundChangeTarget::File(path) = &change.target else {
                continue;
            };
            paths.push(path);
        }
        for change in &state.song.foreground_changes {
            paths.push(&change.path);
        }
        paths
    }

    fn prewarm_model_texture_key(
        assets: &mut AssetManager,
        backend: &mut renderer::Backend,
        seen: &mut HashSet<String>,
        seen_model_textures: &mut HashSet<String>,
        key: &str,
    ) {
        let key = crate::assets::canonical_texture_key(key);
        if !seen_model_textures.insert(key.clone()) {
            return;
        }
        assets.ensure_texture_for_key_with_sampler(backend, &key, model_texture_sampler(&key));
        seen.insert(key);
    }

    fn prewarm_noteskin_textures(
        assets: &mut AssetManager,
        backend: &mut renderer::Backend,
        seen: &mut HashSet<String>,
        seen_model_textures: &mut HashSet<String>,
        noteskin: &crate::game::parsing::noteskin::Noteskin,
    ) {
        noteskin.for_each_texture_key(|key| {
            if seen.insert(key.to_owned()) {
                assets.ensure_texture_for_key(backend, key);
            }
        });
        noteskin.for_each_model_slot(|slot| {
            prewarm_model_texture_key(
                assets,
                backend,
                seen,
                seen_model_textures,
                slot.texture_key(),
            )
        });
    }

    let mut seen = HashSet::<String>::with_capacity(256);
    let mut seen_model_textures = HashSet::<String>::with_capacity(64);
    let mut seen_song_lua_fonts = HashSet::<&'static str>::with_capacity(8);
    for noteskin in state.noteskin.iter().flatten() {
        prewarm_noteskin_textures(
            assets,
            backend,
            &mut seen,
            &mut seen_model_textures,
            noteskin,
        );
    }
    for noteskin in state.mine_noteskin.iter().flatten() {
        prewarm_noteskin_textures(
            assets,
            backend,
            &mut seen,
            &mut seen_model_textures,
            noteskin,
        );
    }
    for noteskin in state.receptor_noteskin.iter().flatten() {
        prewarm_noteskin_textures(
            assets,
            backend,
            &mut seen,
            &mut seen_model_textures,
            noteskin,
        );
    }
    for noteskin in state.tap_explosion_noteskin.iter().flatten() {
        prewarm_noteskin_textures(
            assets,
            backend,
            &mut seen,
            &mut seen_model_textures,
            noteskin,
        );
    }
    for path in gameplay_media_paths(state) {
        let key = path.to_string_lossy().into_owned();
        if seen.insert(key) {
            media_cache::ensure_banner_texture(assets, backend, path);
        }
    }
    let mut prewarm_song_lua_overlays =
        |overlays: &[crate::game::parsing::song_lua::SongLuaOverlayActor]| {
            for overlay in overlays {
                match &overlay.kind {
                    crate::game::parsing::song_lua::SongLuaOverlayKind::BitmapText {
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
                    crate::game::parsing::song_lua::SongLuaOverlayKind::Sprite {
                        texture_path,
                        texture_key,
                    } => {
                        let key = texture_key.as_ref();
                        let first_seen = seen.insert(key.to_owned());
                        let sampler = song_lua_overlay_sampler(overlay);
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
                    crate::game::parsing::song_lua::SongLuaOverlayKind::ActorMultiVertex {
                        texture_path: Some(texture_path),
                        texture_key: Some(texture_key),
                        ..
                    } => {
                        let key = texture_key.as_ref();
                        let first_seen = seen.insert(key.to_owned());
                        let sampler = song_lua_overlay_sampler(overlay);
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
                    crate::game::parsing::song_lua::SongLuaOverlayKind::Model { layers } => {
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
                    crate::game::parsing::song_lua::SongLuaOverlayKind::NoteskinActor { slots } => {
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
    prewarm_song_lua_overlays(&state.song_lua_overlays);
    for layer in &state.song_lua_background_visual_layers {
        prewarm_song_lua_overlays(&layer.overlays);
    }
    for layer in &state.song_lua_foreground_visual_layers {
        prewarm_song_lua_overlays(&layer.overlays);
    }
}

fn prewarm_gameplay_sfx(state: &gameplay::State) {
    crate::engine::audio::preload_sfx("assets/sounds/boom.ogg");
    crate::engine::audio::preload_sfx("assets/sounds/assist_tick.ogg");

    let mut seen = HashSet::<String>::with_capacity(state.song_lua_sound_paths.len());
    let mut prewarm_sound_overlays =
        |overlays: &[crate::game::parsing::song_lua::SongLuaOverlayActor]| {
            for overlay in overlays {
                let crate::game::parsing::song_lua::SongLuaOverlayKind::Sound { sound_path } =
                    &overlay.kind
                else {
                    continue;
                };
                let key = sound_path.to_string_lossy().into_owned();
                if seen.insert(key.clone()) {
                    crate::engine::audio::preload_sfx(&key);
                }
            }
        };

    prewarm_sound_overlays(&state.song_lua_overlays);
    for layer in &state.song_lua_background_visual_layers {
        prewarm_sound_overlays(&layer.overlays);
    }
    for layer in &state.song_lua_foreground_visual_layers {
        prewarm_sound_overlays(&layer.overlays);
    }
    for sound_path in &state.song_lua_sound_paths {
        let key = sound_path.to_string_lossy().into_owned();
        if seen.insert(key.clone()) {
            crate::engine::audio::preload_sfx(&key);
        }
    }
}

fn prewarm_gameplay_text_layout_cache(
    assets: &AssetManager,
    metrics: &Metrics,
    cache: &mut crate::engine::present::compose::TextLayoutCache,
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
    let _ = crate::engine::present::compose::build_screen_cached(
        &actors,
        [0.0, 0.0, 0.0, 1.0],
        metrics,
        fonts,
        0.0,
        cache,
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
    let mut keys = Vec::with_capacity(
        1usize
            .saturating_add(state.song.background_changes.len())
            .saturating_add(state.song.foreground_changes.len()),
    );
    if let Some(path) = state.song.background_path.as_ref() {
        keys.push(path.to_string_lossy().into_owned());
    }
    for change in &state.song.background_changes {
        let crate::game::song::SongBackgroundChangeTarget::File(path) = &change.target else {
            continue;
        };
        keys.push(path.to_string_lossy().into_owned());
    }
    for change in &state.song.foreground_changes {
        keys.push(change.path.to_string_lossy().into_owned());
    }
    keys
}

fn total_gameplay_elapsed(stages: &[stage_stats::StageSummary]) -> f32 {
    let mut total = 0.0;
    for stage in stages {
        let sec = if stage.duration_seconds.is_finite() {
            stage.duration_seconds.max(0.0)
        } else {
            0.0
        };
        total += sec;
    }
    total
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

    let mut song_opt: Option<Arc<crate::game::song::SongData>> = None;
    let mut music_rate: f32 = 1.0;
    let mut players: [Option<stage_stats::PlayerStageSummary>; crate::game::gameplay::MAX_PLAYERS] =
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
        ex_score_percent: si.ex_score_percent,
        hard_ex_score_percent: si.hard_ex_score_percent,
        notes_hit: notes_hit(si),
        calories_burned: si.calories_burned,
        window_counts: si.window_counts,
        window_counts_10ms: si.window_counts_10ms,
        show_w0: (si.show_fa_plus_window && si.show_fa_plus_pane) || si.show_ex_score,
        show_fa_plus_pane: si.show_fa_plus_pane,
        show_ex_score: si.show_ex_score,
        show_hard_ex_score: si.show_hard_ex_score,
        track_early_judgments: si.track_early_judgments,
    };

    match play_style {
        profile::PlayStyle::Versus => {
            for (idx, side) in [(0, profile::PlayerSide::P1), (1, profile::PlayerSide::P2)] {
                let Some(si) = eval.score_info.get(idx).and_then(|s| s.as_ref()) else {
                    continue;
                };
                song_opt = Some(si.song.clone());
                music_rate = si.music_rate;
                players[side_ix(side)] = Some(to_player(si));
            }
        }
        profile::PlayStyle::Single | profile::PlayStyle::Double => {
            let si = eval.score_info.first().and_then(|s| s.as_ref())?;
            song_opt = Some(si.song.clone());
            music_rate = si.music_rate;
            players[side_ix(player_side)] = Some(to_player(si));
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

#[inline(always)]
fn quantize_sync_offset_seconds(v: f32) -> f32 {
    (v / 0.001_f32).round() * 0.001_f32
}

#[inline(always)]
fn sync_change_line(label: &str, start: f32, new: f32) -> Option<String> {
    let start_q = quantize_sync_offset_seconds(start);
    let new_q = quantize_sync_offset_seconds(new);
    let delta_q = new_q - start_q;
    if delta_q.abs() < 0.000_1_f32 {
        return None;
    }
    let direction = if delta_q > 0.0 { "earlier" } else { "later" };
    Some(format!(
        "{label} from {start_q:+.3} to {new_q:+.3} (notes {direction})"
    ))
}

#[inline(always)]
fn format_offset_tag_value(value: f32) -> String {
    let mut v = quantize_sync_offset_seconds(value);
    if v.abs() < 0.000_5_f32 {
        v = 0.0;
    }
    format!("{v:.3}")
}

#[inline(always)]
fn replace_song_arc_if_same_simfile(
    current_song: &mut Arc<crate::game::song::SongData>,
    updated_song: &Arc<crate::game::song::SongData>,
) -> bool {
    if current_song.simfile_path != updated_song.simfile_path {
        return false;
    }
    *current_song = updated_song.clone();
    true
}

#[inline(always)]
fn can_reuse_quick_restart_payload(
    current_song: &crate::game::song::SongData,
    current_chart_hashes: [&str; 2],
    next_song: &crate::game::song::SongData,
    next_chart_hashes: [&str; 2],
) -> bool {
    current_song.simfile_path == next_song.simfile_path
        && (current_song.offset - next_song.offset).abs() < 0.000_001_f32
        && current_chart_hashes == next_chart_hashes
}

fn restart_payload_from_eval(
    score_info: &[Option<evaluation::ScoreInfo>; crate::game::gameplay::MAX_PLAYERS],
) -> Option<(
    Arc<crate::game::song::SongData>,
    [String; crate::game::gameplay::MAX_PLAYERS],
    f32,
    [ScrollSpeedSetting; crate::game::gameplay::MAX_PLAYERS],
)> {
    let mut song = None;
    let mut chart_hashes = std::array::from_fn(|_| String::new());
    let mut scroll_speed = [ScrollSpeedSetting::default(); crate::game::gameplay::MAX_PLAYERS];
    let mut music_rate = None;

    for entry in score_info.iter().flatten() {
        song.get_or_insert_with(|| entry.song.clone());
        let side = side_ix(entry.side);
        chart_hashes[side] = entry.chart.short_hash.clone();
        scroll_speed[side] = entry.speed_mod;
        if music_rate.is_none() && entry.music_rate.is_finite() && entry.music_rate > 0.0 {
            music_rate = Some(entry.music_rate);
        }
    }

    song.map(|song| (song, chart_hashes, music_rate.unwrap_or(1.0), scroll_speed))
}

fn rewrite_simfile_offset_tags(
    simfile_bytes: &[u8],
    delta: f32,
) -> Result<(Vec<u8>, usize), String> {
    const TAG: &[u8] = b"#OFFSET:";
    let len = simfile_bytes.len();
    let mut out: Vec<u8> = Vec::with_capacity(len.saturating_add(64));
    let mut changed = 0usize;
    let mut cursor = 0usize;
    let mut i = 0usize;

    while i + TAG.len() <= len {
        if simfile_bytes[i..i + TAG.len()].eq_ignore_ascii_case(TAG) {
            out.extend_from_slice(&simfile_bytes[cursor..i + TAG.len()]);
            let mut value_start = i + TAG.len();
            while value_start < len
                && simfile_bytes[value_start].is_ascii_whitespace()
                && simfile_bytes[value_start] != b';'
            {
                value_start += 1;
            }
            out.extend_from_slice(&simfile_bytes[i + TAG.len()..value_start]);

            let mut value_end = value_start;
            while value_end < len && simfile_bytes[value_end] != b';' {
                value_end += 1;
            }
            if value_end >= len {
                return Err("Malformed #OFFSET tag: missing ';' terminator".to_string());
            }

            let raw = &simfile_bytes[value_start..value_end];
            let Some(trim_start) = raw.iter().position(|b| !b.is_ascii_whitespace()) else {
                return Err("Malformed #OFFSET tag: empty value".to_string());
            };
            let Some(trim_end_inclusive) = raw.iter().rposition(|b| !b.is_ascii_whitespace())
            else {
                return Err("Malformed #OFFSET tag: empty value".to_string());
            };
            let trim_end = trim_end_inclusive + 1;
            let value_bytes = &raw[trim_start..trim_end];
            let value_str = std::str::from_utf8(value_bytes)
                .map_err(|_| "Malformed #OFFSET tag: value is not valid UTF-8".to_string())?;
            let parsed_value = value_str
                .parse::<f32>()
                .map_err(|_| format!("Malformed #OFFSET tag value: '{value_str}'"))?;
            let new_value = parsed_value + delta;

            out.extend_from_slice(&raw[..trim_start]);
            out.extend_from_slice(format_offset_tag_value(new_value).as_bytes());
            out.extend_from_slice(&raw[trim_end..]);
            out.push(b';');

            changed = changed.saturating_add(1);
            i = value_end.saturating_add(1);
            cursor = i;
            continue;
        }
        i += 1;
    }

    out.extend_from_slice(&simfile_bytes[cursor..]);
    Ok((out, changed))
}

#[inline(always)]
fn simfile_backup_path(simfile_path: &Path) -> PathBuf {
    let mut backup = OsString::from(simfile_path.as_os_str());
    backup.push(".old");
    PathBuf::from(backup)
}

fn save_song_offset_delta_to_simfile(simfile_path: &Path, delta: f32) -> Result<usize, String> {
    let simfile_bytes = std::fs::read(simfile_path)
        .map_err(|e| format!("Failed to read simfile '{}': {e}", simfile_path.display()))?;
    let (rewritten, changed_tags) = rewrite_simfile_offset_tags(&simfile_bytes, delta)?;
    if changed_tags == 0 {
        return Err(format!(
            "No #OFFSET tags found in simfile '{}'",
            simfile_path.display()
        ));
    }

    let backup_path = simfile_backup_path(simfile_path);
    std::fs::copy(simfile_path, &backup_path).map_err(|e| {
        format!(
            "Failed to create backup '{}': {e}",
            backup_path.to_string_lossy()
        )
    })?;
    std::fs::write(simfile_path, rewritten)
        .map_err(|e| format!("Failed to write simfile '{}': {e}", simfile_path.display()))?;
    Ok(changed_tags)
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
            player_options_state: None,
            init_state,
            select_profile_state,
            select_color_state,
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

                    let play_style = profile::get_session_play_style();
                    let max_diff_index = color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1);
                    let p2_profile = profile::get_for_side(profile::PlayerSide::P2);
                    let p2_pref = p2_profile
                        .last_played(play_style)
                        .difficulty_index
                        .min(max_diff_index);
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
                    total_gameplay_elapsed(&session.played_stages);
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
                    total_gameplay_elapsed(&session.played_stages);
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
        profile_data: profile::Profile,
        overlay_mode: u8,
        color_index: i32,
    ) -> Self {
        let play_style = profile::get_session_play_style();
        let max_diff_index = crate::engine::present::color::FILE_DIFFICULTY_NAMES
            .len()
            .saturating_sub(1);
        let preferred = if max_diff_index == 0 {
            0
        } else {
            cmp::min(
                profile_data.last_played(play_style).difficulty_index,
                max_diff_index,
            )
        };

        let shell = ShellState::new(&cfg, overlay_mode);
        let session = SessionState::new(preferred, combo_carry_from_profiles());
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
    backend: Option<renderer::Backend>,
    backend_type: BackendType,
    fsr_monitor: input::fsr::Monitor,
    lights: lights::Manager,
    gameplay_lights: GameplayLightTracker,
    asset_manager: AssetManager,
    dynamic_media: DynamicMedia,
    ui_text_layout_cache: crate::engine::present::compose::TextLayoutCache,
    gameplay_text_layout_cache: crate::engine::present::compose::TextLayoutCache,
    ui_compose_scratch: crate::engine::present::compose::ComposeScratch,
    gameplay_compose_scratch: crate::engine::present::compose::ComposeScratch,
    gameplay_actor_scratch: Vec<Actor>,
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
        foreground_input_active(
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
        input::set_raw_keyboard_window_focused(focused);
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
            self.state.shell.tab_held = false;
            self.state.shell.backquote_held = false;
            input::clear_debounce_state();
            self.lights.clear_button_pressed();
            self.clear_gameplay_input_events();
        } else if let Some(w) = window {
            self.request_redraw(w, "focus");
        }
    }

    #[inline(always)]
    fn sync_input_fsr_view(&mut self) {
        let pending = input_screen::take_fsr_command(&mut self.state.screens.input_state)
            .or_else(|| select_music::take_fsr_command(&mut self.state.screens.select_music_state));
        let on_input = self.state.screens.current_screen == CurrentScreen::Input;
        let on_select_music = self.state.screens.current_screen == CurrentScreen::SelectMusic
            && self
                .state
                .screens
                .select_music_state
                .test_input_overlay_visible;
        if config::get().use_fsrs
            && let Some(cmd) = pending
        {
            let _ = self
                .fsr_monitor
                .update_threshold(cmd.sensor_index, cmd.threshold);
        }
        let view = if config::get().use_fsrs && (on_input || on_select_music) {
            self.fsr_monitor.poll_view()
        } else {
            None
        };
        input_screen::set_fsr_view(
            &mut self.state.screens.input_state,
            on_input.then_some(view.clone()).flatten(),
        );
        select_music::set_fsr_view(
            &mut self.state.screens.select_music_state,
            on_select_music.then_some(view).flatten(),
        );
    }

    fn sync_lights(&mut self, delta_time: f32, elapsed_seconds: f32) {
        let config = config::get();
        self.lights
            .set_driver(config.lights_driver, config.lights_com_port.as_str());
        let screen = self.state.screens.current_screen;
        self.lights.set_mode(light_mode_for_screen(screen));
        self.lights.set_joined([
            profile::is_session_side_joined(profile::PlayerSide::P1),
            profile::is_session_side_joined(profile::PlayerSide::P2),
        ]);
        self.lights.set_hide_flags(self.current_light_hide_flags());
        self.sync_gameplay_light_blinks();
        self.lights.tick(delta_time, elapsed_seconds);
    }

    fn sync_light_input(&mut self, ev: &InputEvent) {
        let Some((player, button)) = light_button_from_action(ev.action) else {
            return;
        };
        self.lights.set_button_pressed(player, button, ev.pressed);
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

    fn sync_gameplay_light_blinks(&mut self) {
        match self.state.screens.current_screen {
            CurrentScreen::Gameplay => {
                if let Some(gs) = self.state.screens.gameplay_state.as_ref() {
                    self.gameplay_lights.queue_blinks(&mut self.lights, gs);
                    return;
                }
            }
            CurrentScreen::Practice => {
                if let Some(ps) = self.state.screens.practice_state.as_ref() {
                    self.gameplay_lights
                        .queue_blinks(&mut self.lights, &ps.gameplay);
                    return;
                }
            }
            _ => {}
        }
        self.gameplay_lights.clear();
    }

    #[inline(always)]
    fn stats_overlay_audio(
        &self,
    ) -> Option<crate::screens::components::shared::stats_overlay::AudioHealth> {
        let audio = crate::engine::audio::get_output_timing_snapshot();
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
            .map(|gs| crate::game::gameplay::display_clock_health(gs))
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
        crate::engine::present::runtime::tick(delta_time);

        // Tab/`-acceleration: scale the dt fed to non-gameplay screens and
        // their fade transitions. Wall-clock dt (`delta_time`) is preserved
        // for the present runtime tick above and for the gameplay step that
        // continues running under the FadingOut→Evaluation and FadingIn
        // transitions below, so timing-sensitive paths stay on real time.
        // See `apply_tab_acceleration` and issue #174.
        let logic_dt = apply_tab_acceleration(
            delta_time,
            self.state.screens.current_screen,
            self.state.shell.tab_held,
            self.state.shell.backquote_held,
            self.state.shell.tab_acceleration_enabled,
        );
        crate::screens::components::shared::visual_style_bg::tick_global(logic_dt);

        self.sync_gameplay_input_capture();
        self.sync_input_fsr_view();
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
                        self.state.shell.screenshot_pending = true;
                        self.state.shell.screenshot_request_side = None;
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
        self.sync_theme_background_video();
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
                    crate::game::gameplay::song_time_ns_to_seconds(state.current_music_time_ns)
                }),
                CurrentScreen::Practice => screens.practice_state.as_ref().map(|state| {
                    crate::game::gameplay::song_time_ns_to_seconds(
                        state.gameplay.current_music_time_ns,
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
                let screen = crate::engine::present::compose::build_screen_cached_with_scratch(
                    &actors,
                    clear_color,
                    &self.state.shell.metrics,
                    fonts,
                    total_elapsed,
                    text_layout_cache,
                    compose_scratch,
                );
                (screen, text_layout_cache.frame_stats())
            } else {
                let text_layout_cache = &mut self.ui_text_layout_cache;
                let compose_scratch = &mut self.ui_compose_scratch;
                text_layout_cache.begin_frame_stats();
                let screen = crate::engine::present::compose::build_screen_cached_with_scratch(
                    &actors,
                    clear_color,
                    &self.state.shell.metrics,
                    fonts,
                    total_elapsed,
                    text_layout_cache,
                    compose_scratch,
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
            if self.state.shell.screenshot_pending {
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
        self.update_stutter_samples(frame_seconds, total_elapsed_end);
        self.record_stutter_diag_frame(
            frame_finished,
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
            frame_finished,
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
        if self.state.screens.current_screen == CurrentScreen::Gameplay {
            actors.clear();
            self.gameplay_actor_scratch = actors;
        }
    }

    fn reset_options_state_for_entry(&mut self, from: CurrentScreen) {
        let current_color_index = self.state.screens.options_state.active_color_index;
        self.state.screens.options_state = options::init();
        self.state.screens.options_state.active_color_index = current_color_index;
        if matches!(from, CurrentScreen::Mappings | CurrentScreen::Input) {
            options::open_input_submenu(&mut self.state.screens.options_state);
        }
    }

    fn new(
        backend_type: BackendType,
        overlay_mode: u8,
        color_index: i32,
        config: config::Config,
        profile_data: profile::Profile,
    ) -> Self {
        let software_renderer_threads = config.software_renderer_threads;
        let gfx_debug_enabled = config.gfx_debug;
        let state = AppState::new(config, profile_data, overlay_mode, color_index);
        Self {
            window: None,
            backend: None,
            backend_type,
            fsr_monitor: input::fsr::Monitor::new(),
            lights: lights::Manager::new(config.lights_driver, config.lights_com_port.as_str()),
            gameplay_lights: GameplayLightTracker::default(),
            asset_manager: AssetManager::new(),
            dynamic_media: DynamicMedia::new(),
            // Screen transitions clear the UI cache, so misses stop inserting
            // once the cache reaches its fixed footprint.
            ui_text_layout_cache: crate::engine::present::compose::TextLayoutCache::new(
                UI_TEXT_LAYOUT_CACHE_LIMIT,
            ),
            gameplay_text_layout_cache: crate::engine::present::compose::TextLayoutCache::new(
                GAMEPLAY_TEXT_LAYOUT_CACHE_LIMIT,
            ),
            ui_compose_scratch: crate::engine::present::compose::ComposeScratch::default(),
            gameplay_compose_scratch: crate::engine::present::compose::ComposeScratch::default(),
            gameplay_actor_scratch: Vec::with_capacity(256),
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
            ScreenAction::SelectProfiles { p1, p2 } => {
                let fast_profile_switch = profile::take_fast_profile_switch_from_select_music();
                let profile_data = profile::set_active_profiles(p1, p2);
                self.state.session.combo_carry =
                    [profile_data[0].current_combo, profile_data[1].current_combo];
                if let Some(backend) = self.backend.as_mut() {
                    self.dynamic_media.set_profile_avatar_for_side(
                        &mut self.asset_manager,
                        backend,
                        profile::PlayerSide::P1,
                        profile_data[0].avatar_path.clone(),
                    );
                    self.dynamic_media.set_profile_avatar_for_side(
                        &mut self.asset_manager,
                        backend,
                        profile::PlayerSide::P2,
                        profile_data[1].avatar_path.clone(),
                    );
                }

                let max_diff_index = crate::engine::present::color::FILE_DIFFICULTY_NAMES
                    .len()
                    .saturating_sub(1);
                let play_style = profile::get_session_play_style();
                let preferred_p1 = if max_diff_index == 0 {
                    0
                } else {
                    cmp::min(
                        profile_data[0].last_played(play_style).difficulty_index,
                        max_diff_index,
                    )
                };
                let preferred_p2 = if max_diff_index == 0 {
                    0
                } else {
                    cmp::min(
                        profile_data[1].last_played(play_style).difficulty_index,
                        max_diff_index,
                    )
                };
                let side = profile::get_session_player_side();
                let preferred_active = match side {
                    profile::PlayerSide::P1 => preferred_p1,
                    profile::PlayerSide::P2 => preferred_p2,
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
                    // ProfileLoad asynchronously prepares SelectMusic/SelectCourse state;
                    // avoid redundant eager init here.
                    self.handle_navigation_action(CurrentScreen::SelectColor);
                }
                Vec::new()
            }
            ScreenAction::RequestScreenshot(side) => {
                self.state.shell.screenshot_pending = true;
                self.state.shell.screenshot_request_side = side;
                Vec::new()
            }
            ScreenAction::RequestBanner(path_opt) => vec![Command::SetBanner(path_opt)],
            ScreenAction::RequestCdTitle(path_opt) => vec![Command::SetCdTitle(path_opt)],
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
            ScreenAction::ConsumeInput => Vec::new(),
            ScreenAction::None => Vec::new(),
        };
        self.run_commands(commands, event_loop)
    }

    #[inline(always)]
    fn gameplay_global_offset_changed(gs: &gameplay::State) -> bool {
        (gs.global_offset_seconds - gs.initial_global_offset_seconds).abs() > 0.000_001_f32
    }

    #[inline(always)]
    fn gameplay_song_offset_changed(gs: &gameplay::State) -> bool {
        (gs.song_offset_seconds - gs.initial_song_offset_seconds).abs() > 0.000_001_f32
    }

    #[inline(always)]
    fn gameplay_offset_changed(gs: &gameplay::State) -> bool {
        Self::gameplay_global_offset_changed(gs) || Self::gameplay_song_offset_changed(gs)
    }

    fn gameplay_sync_prompt_text(gs: &gameplay::State) -> String {
        let mut text = String::with_capacity(320);

        if let Some(line) = sync_change_line(
            "Global Offset",
            gs.initial_global_offset_seconds,
            gs.global_offset_seconds,
        ) {
            text.push_str(&line);
            text.push_str("\n\n");
        }

        if let Some(line) = sync_change_line(
            "Song offset",
            gs.initial_song_offset_seconds,
            gs.song_offset_seconds,
        ) {
            text.push_str("You have changed the timing of\n");
            text.push_str(&gs.song.display_full_title(false));
            text.push_str(":\n\n");
            text.push_str(&line);
            text.push_str("\n\n");
        }

        text.push_str("Would you like to save these changes?\n");
        text.push_str("Choosing NO will discard your changes.");
        text
    }

    fn save_song_offset_changes(&mut self, changes: &[SongOffsetSyncChange]) -> Result<(), String> {
        let mut saved_files = 0usize;
        let mut changed_tags_total = 0usize;
        let mut first_saved_path: Option<&Path> = None;

        for change in changes {
            if change.delta_seconds.abs() < 0.000_001_f32 {
                continue;
            }
            changed_tags_total += save_song_offset_delta_to_simfile(
                change.simfile_path.as_path(),
                change.delta_seconds,
            )?;
            let updated_song = song_loading::reload_song_in_cache(change.simfile_path.as_path())?;
            if let Some(po_state) = self.state.screens.player_options_state.as_mut() {
                let _ = replace_song_arc_if_same_simfile(&mut po_state.song, &updated_song);
            }
            saved_files += 1;
            if first_saved_path.is_none() {
                first_saved_path = Some(change.simfile_path.as_path());
            }
        }

        if saved_files == 0 {
            return Ok(());
        }

        select_music::refresh_from_song_cache(&mut self.state.screens.select_music_state);
        if saved_files == 1 {
            if let Some(path) = first_saved_path {
                info!(
                    "Saved song offset sync changes to '{}' (updated {} #OFFSET tags; refreshed song cache).",
                    path.display(),
                    changed_tags_total
                );
            }
        } else {
            info!(
                "Saved pack sync changes to {} simfiles (updated {} #OFFSET tags; refreshed song cache).",
                saved_files, changed_tags_total
            );
        }
        Ok(())
    }

    fn save_gameplay_song_offset(&mut self, simfile_path: &Path, delta: f32) -> Result<(), String> {
        self.save_song_offset_changes(&[SongOffsetSyncChange {
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
                if Self::gameplay_global_offset_changed(gs) {
                    config::update_global_offset(gs.global_offset_seconds);
                }
                if Self::gameplay_song_offset_changed(gs) {
                    song_offset_change = Some((
                        gs.song.simfile_path.clone(),
                        gs.song_offset_seconds - gs.initial_song_offset_seconds,
                    ));
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
        let decision = match ev.action {
            input::VirtualAction::p1_left
            | input::VirtualAction::p1_menu_left
            | input::VirtualAction::p2_left
            | input::VirtualAction::p2_menu_left => {
                let mut moved = false;
                if let Some(prompt) = self.state.gameplay_offset_save_prompt.as_mut()
                    && prompt.active_choice > 0
                {
                    prompt.active_choice -= 1;
                    moved = true;
                }
                if moved {
                    crate::engine::audio::play_sfx("assets/sounds/change.ogg");
                }
                None
            }
            input::VirtualAction::p1_right
            | input::VirtualAction::p1_menu_right
            | input::VirtualAction::p2_right
            | input::VirtualAction::p2_menu_right => {
                let mut moved = false;
                if let Some(prompt) = self.state.gameplay_offset_save_prompt.as_mut()
                    && prompt.active_choice < 1
                {
                    prompt.active_choice += 1;
                    moved = true;
                }
                if moved {
                    crate::engine::audio::play_sfx("assets/sounds/change.ogg");
                }
                None
            }
            input::VirtualAction::p1_start
            | input::VirtualAction::p2_start
            | input::VirtualAction::p1_select
            | input::VirtualAction::p2_select => {
                let save_changes = self
                    .state
                    .gameplay_offset_save_prompt
                    .as_ref()
                    .is_some_and(|prompt| prompt.active_choice == 0);
                crate::engine::audio::play_sfx("assets/sounds/start.ogg");
                Some(save_changes)
            }
            input::VirtualAction::p1_back | input::VirtualAction::p2_back => None,
            _ => None,
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
        if gs.autoplay_used {
            return;
        }
        let play_style = profile::get_session_play_style();
        let player_side = profile::get_session_player_side();
        match play_style {
            profile::PlayStyle::Versus => {
                for idx in 0..gs.num_players.min(crate::game::gameplay::MAX_PLAYERS) {
                    let combo = gs.players[idx].combo;
                    self.state.session.combo_carry[idx] = combo;
                    let side = if idx == 0 {
                        profile::PlayerSide::P1
                    } else {
                        profile::PlayerSide::P2
                    };
                    profile::update_current_combo_for_side(side, combo);
                }
            }
            profile::PlayStyle::Single | profile::PlayStyle::Double => {
                if gs.num_players == 0 {
                    return;
                }
                let combo = gs.players[0].combo;
                self.state.session.combo_carry[side_ix(player_side)] = combo;
                profile::update_current_combo_for_side(player_side, combo);
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
        self.state.session.last_course_wheel_path = Some(selection.path.clone());
        self.state.session.last_course_wheel_difficulty_name =
            Some(selection.course_difficulty_name.clone());
        let Some(course_run) = build_course_run_from_selection(selection) else {
            warn!("Unable to start course run: failed to resolve course stages.");
            return false;
        };
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
        song: Arc<crate::game::song::SongData>,
        chart_hashes: [&str; crate::game::gameplay::MAX_PLAYERS],
        music_rate: f32,
        scroll_speed: [ScrollSpeedSetting; crate::game::gameplay::MAX_PLAYERS],
        active_color_index: i32,
    ) -> bool {
        let play_style = profile::get_session_play_style();
        let player_side = profile::get_session_player_side();
        let target_chart_type = play_style.chart_type();
        let fallback_steps = self.state.session.preferred_difficulty_index;

        let p1_steps =
            select_music::steps_index_for_chart_hash(&song, target_chart_type, chart_hashes[0])
                .unwrap_or(fallback_steps);
        let p2_steps =
            select_music::steps_index_for_chart_hash(&song, target_chart_type, chart_hashes[1])
                .unwrap_or(fallback_steps);

        let chart_steps_index = match play_style {
            profile::PlayStyle::Versus => [p1_steps, p2_steps],
            profile::PlayStyle::Single | profile::PlayStyle::Double => {
                let idx = side_ix(player_side);
                let selected = [p1_steps, p2_steps][idx];
                [selected; 2]
            }
        };

        let mut po_state = player_options::init(
            song,
            chart_steps_index,
            chart_steps_index,
            active_color_index,
            CurrentScreen::Gameplay,
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
            let song = gs.song.clone();
            let chart_hashes = [
                gs.charts[0].short_hash.clone(),
                gs.charts[1].short_hash.clone(),
            ];
            let music_rate = gs.music_rate;
            let scroll_speed = gs.scroll_speed;
            let active_color_index = gs.active_color_index;
            return self.prepare_restart_player_options(
                song,
                [chart_hashes[0].as_str(), chart_hashes[1].as_str()],
                music_rate,
                scroll_speed,
                active_color_index,
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
        )
    }

    fn try_gameplay_restart(&mut self, event_loop: &ActiveEventLoop, label: &str) -> bool {
        if self.prepare_player_options_for_gameplay_restart() {
            let restart_count = self.state.session.gameplay_restart_count.saturating_add(1);
            if let Err(e) =
                self.handle_action(ScreenAction::Navigate(CurrentScreen::Gameplay), event_loop)
            {
                log::error!("Failed to restart Gameplay with {label}: {e}");
            } else {
                self.state.session.gameplay_restart_count = restart_count;
            }
            true
        } else {
            log::warn!("Ignored {label} restart: no restartable stage state.");
            false
        }
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
        (0..gs.num_players.min(crate::game::gameplay::MAX_PLAYERS)).any(|player_idx| {
            let p = &gs.players[player_idx];
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
            for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
                if let Some(p) = stage.players.get(side_ix(side)).and_then(|p| p.as_ref()) {
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
            if config::get().show_course_individual_scores {
                let mut stage_page = eval_state.clone();
                stage_page.return_to_course = true;
                stage_page.auto_advance_seconds = None;
                course_run.stage_eval_pages.push(stage_page);
            }
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

        if let Some(course_run) = self.state.session.course_run.as_mut() {
            if course_run.next_stage_index >= course_run.stages.len() {
                let score_hash = course_run.score_hash.clone();
                let per_song_pages = course_run.stage_eval_pages.clone();
                let course_summary = build_course_summary_stage(course_run);
                self.state.session.course_run = None;
                self.state.session.course_eval_pages.clear();
                self.state.session.course_eval_page_index = 0;

                if let Some(course_stage) = course_summary {
                    for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
                        if let Some(player) = course_stage.players[side_ix(side)].as_ref() {
                            scores::save_local_summary_score_for_side(
                                score_hash.as_str(),
                                side,
                                course_stage.music_rate,
                                player,
                            );
                        }
                    }
                    self.state.session.played_stages.push(course_stage.clone());

                    let gameplay_elapsed =
                        total_gameplay_elapsed(&self.state.session.played_stages);
                    let session_elapsed = self.state.screens.evaluation_state.session_elapsed;
                    let screen_elapsed = self.state.screens.evaluation_state.screen_elapsed;
                    let mut course_page = build_course_summary_eval_state(
                        &course_stage,
                        color_idx,
                        session_elapsed,
                        gameplay_elapsed,
                    );
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
            total_gameplay_elapsed(&self.state.session.played_stages);
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
        crate::engine::audio::play_sfx("assets/sounds/change.ogg");
    }

    fn apply_select_music_join(&mut self, join_side: profile::PlayerSide) {
        let play_style = profile::get_session_play_style();
        let max_diff_index = color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1);
        let p1_profile = profile::get_for_side(profile::PlayerSide::P1);
        let p2_profile = profile::get_for_side(profile::PlayerSide::P2);
        let p1_pref = p1_profile
            .last_played(play_style)
            .difficulty_index
            .min(max_diff_index);
        let p2_pref = p2_profile
            .last_played(play_style)
            .difficulty_index
            .min(max_diff_index);

        let side = profile::get_session_player_side();
        let sm = &mut self.state.screens.select_music_state;
        if side == profile::PlayerSide::P2 && join_side == profile::PlayerSide::P1 {
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
                for i in 0..color::FILE_DIFFICULTY_NAMES.len() {
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
            input::VirtualAction::p1_start => profile::PlayerSide::P1,
            input::VirtualAction::p2_start => profile::PlayerSide::P2,
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

        if profile::get_session_play_style() == profile::PlayStyle::Double {
            return false;
        }

        let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
        let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
        if p1_joined && p2_joined {
            return false;
        }
        if (join_side == profile::PlayerSide::P1 && p1_joined)
            || (join_side == profile::PlayerSide::P2 && p2_joined)
        {
            return false;
        }
        if !(p1_joined || p2_joined) {
            return false;
        }

        profile::set_session_joined(true, true);
        profile::set_session_play_style(profile::PlayStyle::Versus);
        let guest_profile =
            profile::set_active_profile_for_side(join_side, profile::ActiveProfile::Guest);
        self.state.session.combo_carry[side_ix(join_side)] = guest_profile.current_combo;

        if screen == CurrentScreen::SelectStyle {
            self.state.screens.select_style_state.selected_index = 1;
        }
        if screen == CurrentScreen::SelectMusic {
            self.apply_select_music_join(join_side);
        }

        crate::engine::audio::play_sfx("assets/sounds/start.ogg");
        true
    }

    fn route_input_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        ev: InputEvent,
    ) -> Result<(), Box<dyn Error>> {
        self.sync_light_input(&ev);
        if self.route_gameplay_offset_prompt_input(event_loop, &ev) {
            return Ok(());
        }
        if self.try_handle_late_join(&ev) {
            return Ok(());
        }
        if config::get().only_dedicated_menu_buttons && ev.action.is_gameplay_arrow() {
            let allow_gameplay_arrow = match self.state.screens.current_screen {
                CurrentScreen::Gameplay | CurrentScreen::Practice | CurrentScreen::Input => true,
                CurrentScreen::SelectMusic => {
                    self.state
                        .screens
                        .select_music_state
                        .test_input_overlay_visible
                }
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
                input::VirtualAction::p1_select | input::VirtualAction::p2_select
            )
        {
            self.state.shell.screenshot_pending = true;
            self.state.shell.screenshot_request_side = match ev.action {
                input::VirtualAction::p1_select => Some(profile::PlayerSide::P1),
                input::VirtualAction::p2_select => Some(profile::PlayerSide::P2),
                _ => None,
            };
            return Ok(());
        }
        if ev.pressed
            && self.state.screens.current_screen == CurrentScreen::Gameplay
            && self.state.gameplay_offset_save_prompt.is_none()
            && self.state.session.course_run.is_none()
            && matches!(
                ev.action,
                input::VirtualAction::p1_restart | input::VirtualAction::p2_restart
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
            CurrentScreen::SelectMusic => crate::screens::select_music::handle_input(
                &mut self.state.screens.select_music_state,
                &ev,
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
        state: &mut crate::game::gameplay::State,
        show_video_backgrounds: bool,
    ) -> Option<PathBuf> {
        let path = state
            .song
            .gameplay_background_path_for_change_ix(
                state.next_background_change_ix,
                show_video_backgrounds,
            )
            .cloned();
        state.current_background_path = path.clone();
        state.background_allow_video = show_video_backgrounds;
        state.background_path_dirty = false;
        path
    }

    fn sync_gameplay_background(&mut self) {
        if !matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        ) {
            return;
        }
        let show_video_backgrounds = config::get().show_video_backgrounds;
        let desired_path = {
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
            let mut background_changed = false;
            while let Some(change) = gs.song.background_changes.get(gs.next_background_change_ix) {
                if gs.current_beat < change.start_beat {
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
            gs.current_background_path.clone()
        };

        let next_key = self.backend.as_mut().and_then(|backend| {
            self.dynamic_media.sync_gameplay_background(
                &mut self.asset_manager,
                backend,
                desired_path.as_deref(),
                show_video_backgrounds,
            )
        });
        if let Some(key) = next_key {
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

    fn sync_theme_background_video(&mut self) {
        if matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        ) {
            crate::screens::components::shared::visual_style_bg::set_srpg9_background_key(None);
            return;
        }

        let cfg = config::get();
        let path = (cfg.visual_style == config::VisualStyle::Srpg9 && cfg.show_video_backgrounds)
            .then(visual_styles::shared_background_video_asset_path)
            .flatten()
            .map(|path| dirs::app_dirs().resolve_asset_path(path));

        let Some(backend) = self.backend.as_mut() else {
            crate::screens::components::shared::visual_style_bg::set_srpg9_background_key(None);
            return;
        };

        let key = self
            .dynamic_media
            .set_background(&mut self.asset_manager, backend, path);
        let srpg9_key = if key == "__black" { None } else { Some(key) };
        crate::screens::components::shared::visual_style_bg::set_srpg9_background_key(srpg9_key);
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
        if !Self::gameplay_offset_changed(gs) {
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
        let cursor_color = color::simply_love_rgba(gs.active_color_index);

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

        let mut actors = match self.state.screens.current_screen {
            CurrentScreen::Menu => {
                menu::get_actors(&self.state.screens.menu_state, screen_alpha_multiplier)
            }
            CurrentScreen::Gameplay => {
                let mut actors = std::mem::take(&mut self.gameplay_actor_scratch);
                actors.clear();
                if let Some(gs) = &mut self.state.screens.gameplay_state {
                    crate::screens::components::gameplay::gameplay_stats::refresh_density_graph_meshes(gs);
                    gameplay::push_actors(
                        &mut actors,
                        gs,
                        &self.asset_manager,
                        gameplay::ActorViewOverride::default(),
                    );
                }
                actors
            }
            CurrentScreen::Practice => {
                if let Some(ps) = &mut self.state.screens.practice_state {
                    crate::screens::components::gameplay::gameplay_stats::refresh_density_graph_meshes(
                        &mut ps.gameplay,
                    );
                    practice::get_actors(ps, &self.asset_manager)
                } else {
                    vec![]
                }
            }
            CurrentScreen::Options => options::get_actors(
                &self.state.screens.options_state,
                &self.asset_manager,
                screen_alpha_multiplier,
            ),
            CurrentScreen::Credits => credits::get_actors(&self.state.screens.credits_state),
            CurrentScreen::ManageLocalProfiles => manage_local_profiles::get_actors(
                &self.state.screens.manage_local_profiles_state,
                &self.asset_manager,
                screen_alpha_multiplier,
            ),
            CurrentScreen::Mappings => mappings::get_actors(
                &self.state.screens.mappings_state,
                &self.asset_manager,
                screen_alpha_multiplier,
            ),
            CurrentScreen::Input => input_screen::get_actors(&self.state.screens.input_state),
            CurrentScreen::PlayerOptions => {
                if let Some(pos) = &self.state.screens.player_options_state {
                    player_options::get_actors(pos, &self.asset_manager)
                } else {
                    vec![]
                }
            }
            CurrentScreen::SelectProfile => select_profile::get_actors(
                &self.state.screens.select_profile_state,
                &self.asset_manager,
                screen_alpha_multiplier,
            ),
            CurrentScreen::SelectColor => select_color::get_actors(
                &self.state.screens.select_color_state,
                screen_alpha_multiplier,
            ),
            CurrentScreen::SelectStyle => {
                select_style::get_actors(&self.state.screens.select_style_state)
            }
            CurrentScreen::SelectPlayMode => select_mode::get_actors(
                &self.state.screens.select_play_mode_state,
                &self.asset_manager,
            ),
            CurrentScreen::ProfileLoad => {
                profile_load::get_actors(&self.state.screens.profile_load_state)
            }
            CurrentScreen::SelectMusic => select_music::get_actors(
                &self.state.screens.select_music_state,
                &self.asset_manager,
                self.state.session.played_stages.len() + 1,
            ),
            CurrentScreen::SelectCourse => select_course::get_actors(
                &self.state.screens.select_course_state,
                &self.asset_manager,
            ),
            CurrentScreen::Sandbox => sandbox::get_actors(&self.state.screens.sandbox_state),
            CurrentScreen::Init => init::get_actors(&self.state.screens.init_state),
            CurrentScreen::Evaluation => {
                evaluation::get_actors(&self.state.screens.evaluation_state, &self.asset_manager)
            }
            CurrentScreen::EvaluationSummary => {
                let stages = self.post_select_display_stages();
                evaluation_summary::get_actors(
                    &self.state.screens.evaluation_summary_state,
                    &stages,
                    &self.asset_manager,
                )
            }
            CurrentScreen::Initials => {
                let stages = self.post_select_display_stages();
                initials::get_actors(
                    &self.state.screens.initials_state,
                    &stages,
                    &self.asset_manager,
                )
            }
            CurrentScreen::GameOver => gameover::get_actors(
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
                // Special case: Menu → SelectColor / Menu → Options should keep the heart
                // background bright and only fade UI, but still play the hearts splash.
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
        let mut out = Vec::with_capacity(STUTTER_SAMPLE_COUNT);
        let start = self.state.shell.stutter_cursor;
        for i in 0..STUTTER_SAMPLE_COUNT {
            let sample = self.state.shell.stutter_samples[(start + i) % STUTTER_SAMPLE_COUNT];
            if sample.severity == 0 {
                continue;
            }
            let age_seconds = now_seconds - sample.at_seconds;
            if !(0.0..=STUTTER_SAMPLE_LIFETIME).contains(&age_seconds) {
                continue;
            }
            let frame_multiple = if sample.expected_seconds > 0.0 {
                sample.frame_seconds / sample.expected_seconds
            } else {
                0.0
            };
            out.push(
                crate::screens::components::shared::stats_overlay::StutterEvent {
                    timestamp_seconds: sample.at_seconds,
                    frame_ms: sample.frame_seconds * 1000.0,
                    frame_multiple,
                    severity: sample.severity,
                    age_seconds,
                },
            );
        }
        out
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
    fn record_stutter_diag_frame(
        &mut self,
        frame_finished: Instant,
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
            .map(|gs| crate::game::gameplay::display_clock_health(gs))
            .unwrap_or_default();
        let display_error_us_i64 =
            (f64::from(display_clock.error_seconds) * 1_000_000.0).round() as i64;
        let display_error_us =
            display_error_us_i64.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let present_stats = draw_stats.present_stats;
        self.state
            .shell
            .stutter_diag
            .push_frame(StutterDiagFrameSample {
                host_nanos: crate::engine::host_time::instant_nanos(frame_finished),
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
        self.state.shell.stutter_diag.collect_recent_frames(
            now_host_nanos,
            STUTTER_DIAG_DUMP_WINDOW_NS,
            &mut frames,
        );
        let mut audio_events = Vec::with_capacity(32);
        crate::engine::audio::collect_stutter_diag_events(
            now_host_nanos,
            STUTTER_DIAG_DUMP_WINDOW_NS,
            &mut audio_events,
        );
        let mut display_events = Vec::with_capacity(32);
        if let Some(gameplay_state) = self.state.screens.gameplay_state.as_ref() {
            crate::game::gameplay::collect_display_clock_stutter_diag_events(
                gameplay_state,
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
        frame_finished: Instant,
        total_elapsed: f32,
        screen: CurrentScreen,
        frame_seconds: f32,
    ) {
        if !stutter_diag_enabled() {
            return;
        }
        let now_host_nanos = crate::engine::host_time::instant_nanos(frame_finished);
        if now_host_nanos == 0 {
            return;
        }
        let expected = self.expected_frame_seconds_for_stutter();
        let stutter_severity = stutter_severity(frame_seconds, expected);
        let audio_trigger_seq = crate::engine::audio::stutter_diag_trigger_seq();
        let display_trigger_seq = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|gs| crate::game::gameplay::display_clock_stutter_diag_trigger_seq(gs))
            .unwrap_or(0);
        let audio_triggered =
            audio_trigger_seq > self.state.shell.stutter_diag.last_audio_trigger_seq;
        let display_triggered =
            display_trigger_seq > self.state.shell.stutter_diag.last_display_trigger_seq;
        if stutter_severity == 0 && !audio_triggered && !display_triggered {
            return;
        }
        if now_host_nanos.saturating_sub(self.state.shell.stutter_diag.last_dump_host_nanos)
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
        self.state.shell.stutter_diag.last_audio_trigger_seq = audio_trigger_seq;
        self.state.shell.stutter_diag.last_display_trigger_seq = display_trigger_seq;
        self.state.shell.stutter_diag.last_dump_host_nanos = now_host_nanos;
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
        actors: &[crate::engine::present::actors::Actor],
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
            .map(|gs| crate::game::gameplay::display_clock_health(gs))
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
        let audio_stats = crate::engine::audio::get_output_timing_snapshot();
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
            .map(|gs| crate::game::gameplay::display_clock_health(gs))
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
        let audio_stats = crate::engine::audio::get_output_timing_snapshot();
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

    fn capture_compose_case_now(&mut self) {
        let total_elapsed = Instant::now()
            .duration_since(self.state.shell.start_time)
            .as_secs_f32();
        let screen_name = format!("{:?}", self.state.screens.current_screen);
        let (actors, clear_color) = self.get_current_actors();
        let Ok((case, output)) = crate::test_support::compose_case::capture_case(
            &screen_name,
            &actors,
            clear_color,
            &self.state.shell.metrics,
            self.asset_manager.fonts(),
            total_elapsed,
        ) else {
            warn!("Failed to capture compose case for {screen_name}");
            return;
        };

        let (case_path, output_path) =
            crate::test_support::compose_case::default_capture_paths(&screen_name);
        if let Err(e) = crate::test_support::compose_case::write_case(&case_path, &case) {
            warn!(
                "Failed to write compose case '{}': {e}",
                case_path.display()
            );
            return;
        }
        if let Err(e) =
            crate::test_support::compose_case::write_render_snapshot(&output_path, &output)
        {
            warn!(
                "Failed to write compose output snapshot '{}': {e}",
                output_path.display()
            );
            return;
        }

        info!(
            "Saved compose capture for {}: case='{}' output='{}' hash={} objects={} cameras={}",
            screen_name,
            case_path.display(),
            output_path.display(),
            case.expected.output_hash,
            case.expected.objects,
            case.expected.cameras
        );
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
        raw_key: input::RawKeyboardEvent,
    ) -> bool {
        use winit::keyboard::KeyCode;

        match raw_key.code {
            KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                self.state.shell.shift_held = raw_key.pressed;
            }
            KeyCode::ControlLeft | KeyCode::ControlRight => {
                self.state.shell.ctrl_held = raw_key.pressed;
            }
            KeyCode::Tab => {
                self.state.shell.tab_held = raw_key.pressed;
            }
            KeyCode::Backquote => {
                self.state.shell.backquote_held = raw_key.pressed;
            }
            _ => {}
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
            let action = crate::screens::select_music::handle_raw_key_event(
                &mut self.state.screens.select_music_state,
                Some(&raw_key),
                None,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle SelectMusic raw key action: {e}");
                }
                return true;
            }
        } else if self.state.screens.current_screen == CurrentScreen::Practice {
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
            if App::raw_keyboard_restart_screen(self.state.screens.current_screen)
                && raw_key.pressed
                && !raw_key.repeat
                && raw_key.code == KeyCode::KeyR
                && self.state.shell.ctrl_held
                && config::get().keyboard_features
                && self.state.session.course_run.is_none()
            {
                self.try_gameplay_restart(event_loop, "Ctrl+R");
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
        let is_transitioning = !matches!(self.state.shell.transition, TransitionState::Idle);

        if raw_key.pressed && raw_key.code == KeyCode::F3 {
            let mode = self.state.shell.cycle_overlay_mode();
            debug!("Overlay {}", self.state.shell.overlay_mode.label());
            config::update_show_stats_mode(mode);
            options::sync_show_stats_mode(&mut self.state.screens.options_state, mode);
        }
        if raw_key.pressed && !raw_key.repeat && raw_key.code == KeyCode::F9 {
            let new_value = !config::get().translated_titles;
            config::update_translated_titles(new_value);
            options::sync_translated_titles(&mut self.state.screens.options_state, new_value);
            crate::engine::audio::play_sfx("assets/sounds/change.ogg");
        }
        if raw_key.pressed
            && !raw_key.repeat
            && self.state.shell.ctrl_held
            && self.state.shell.shift_held
            && raw_key.code == KeyCode::F10
        {
            self.capture_compose_case_now();
            return true;
        }
        // Screen-specific Escape handling resides in per-screen raw handlers now

        if is_transitioning {
            input::clear_debounce_state();
            self.clear_gameplay_input_events();
            return true;
        }

        let gameplay_screen = self.state.screens.current_screen == CurrentScreen::Gameplay;
        if gameplay_screen {
            return false;
        }

        let mut input_err: Option<Box<dyn Error>> = None;
        input::map_raw_key_event_with(&raw_key, |ev| {
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
    fn handle_live_key_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        raw_key: input::RawKeyboardEvent,
    ) {
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
            input::map_raw_key_event_with(&raw_key, |ev| {
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

        if input::unix_raw_keyboard_backend_active() || !self.accepts_live_input() {
            return;
        }
        let PhysicalKey::Code(code) = key_event.physical_key else {
            return;
        };
        self.handle_live_key_event(
            event_loop,
            input::RawKeyboardEvent {
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
        let is_transitioning = !matches!(self.state.shell.transition, TransitionState::Idle);
        if is_transitioning {
            input::clear_debounce_state();
            self.clear_gameplay_input_events();
            return;
        }
        let gameplay_screen = self.state.screens.current_screen == CurrentScreen::Gameplay;
        let mut input_err: Option<Box<dyn Error>> = None;
        let start_screen = self.state.screens.current_screen;
        let mut discard_gameplay_batch = false;
        input::map_pad_event_with(&ev, |iev| {
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
        let keep_preview = (prev == CurrentScreen::SelectMusic
            && target == CurrentScreen::PlayerOptions)
            || (prev == CurrentScreen::PlayerOptions && target == CurrentScreen::SelectMusic);

        if target_menu_music {
            if !prev_menu_music {
                commands.push(Command::PlayMusic {
                    path: dirs::app_dirs()
                        .resolve_asset_path(visual_styles::menu_music_asset_path()),
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
            if !target_menu_music && !target_course_music && !target_credits_music {
                commands.push(Command::StopMusic);
            }
            if let Some(backend) = self.backend.as_mut() {
                self.dynamic_media
                    .set_background(&mut self.asset_manager, backend, None);
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
                     side: profile::PlayerSide,
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
                    profile::PlayStyle::Versus => {
                        update_scroll_speed(
                            &mut commands,
                            profile::PlayerSide::P1,
                            &po_state.speed_mod[0],
                        );
                        update_scroll_speed(
                            &mut commands,
                            profile::PlayerSide::P2,
                            &po_state.speed_mod[1],
                        );
                    }
                    profile::PlayStyle::Single | profile::PlayStyle::Double => {
                        let persisted_idx = match player_side {
                            profile::PlayerSide::P1 => 0,
                            profile::PlayerSide::P2 => 1,
                        };
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
                    profile::PlayStyle::Versus => po_state.chart_difficulty_index[0],
                    profile::PlayStyle::Single | profile::PlayStyle::Double => {
                        let persisted_idx = match player_side {
                            profile::PlayerSide::P1 => 0,
                            profile::PlayerSide::P2 => 1,
                        };
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
            self.state.session.combo_carry = combo_carry_from_profiles();
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
        } else if target == CurrentScreen::SelectProfile {
            let current_color_index = self.state.screens.select_profile_state.active_color_index;
            self.state.screens.select_profile_state = select_profile::init();
            self.state.screens.select_profile_state.active_color_index = current_color_index;
            if prev == CurrentScreen::Menu {
                let p2 = self.state.screens.menu_state.started_by_p2;
                select_profile::set_joined(&mut self.state.screens.select_profile_state, !p2, p2);
                profile::set_fast_profile_switch_from_select_music(false);
            } else if prev == CurrentScreen::SelectMusic {
                let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
                let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
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
            let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
            let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
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
                        profile::PlayStyle::Versus => (
                            [
                                sm_state.selected_steps_index,
                                sm_state.p2_selected_steps_index,
                            ],
                            [
                                sm_state.preferred_difficulty_index,
                                sm_state.p2_preferred_difficulty_index,
                            ],
                        ),
                        profile::PlayStyle::Single | profile::PlayStyle::Double => (
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
                    |gs| gs.active_color_index,
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
                        profile::PlayStyle::Versus => (
                            [
                                sm_state.selected_steps_index,
                                sm_state.p2_selected_steps_index,
                            ],
                            [
                                sm_state.preferred_difficulty_index,
                                sm_state.p2_preferred_difficulty_index,
                            ],
                        ),
                        profile::PlayStyle::Single | profile::PlayStyle::Double => (
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
        self.state
            .screens
            .evaluation_summary_state
            .active_color_index = idx;
        self.state.screens.initials_state.active_color_index = idx;
        self.state.screens.gameover_state.active_color_index = idx;
        if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
            gs.active_color_index = idx;
            gs.player_color_index = idx;
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
            crate::engine::audio::stop_music();
            if let Some(po_state) = self.state.screens.player_options_state.take() {
                let song_arc = po_state.song.clone();
                let play_style = profile::get_session_play_style();
                let player_side = profile::get_session_player_side();
                let target_chart_type = play_style.chart_type();
                let mut resolved_steps_index = po_state.chart_steps_index;
                let mut resolve_chart = |slot: usize| {
                    let requested_idx = resolved_steps_index[slot];
                    if let Some(chart_ref) = select_music::chart_for_steps_index(
                        &song_arc,
                        target_chart_type,
                        requested_idx,
                    ) {
                        return chart_ref;
                    }

                    let preferred_idx = po_state.chart_difficulty_index[slot];
                    if let Some(fallback_idx) =
                        select_music::best_steps_index(&song_arc, target_chart_type, preferred_idx)
                        && let Some(chart_ref) = select_music::chart_for_steps_index(
                            &song_arc,
                            target_chart_type,
                            fallback_idx,
                        )
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
                let chart_ix_for_ref = |chart_ref: &crate::game::chart::ChartData| {
                    song_arc
                        .charts
                        .iter()
                        .position(|chart| std::ptr::eq(chart, chart_ref))
                        .expect("selected chart ref must come from selected song")
                };
                let (charts, chart_ixs, last_played_idx) = match play_style {
                    profile::PlayStyle::Versus => {
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
                    profile::PlayStyle::Single | profile::PlayStyle::Double => {
                        let idx = match player_side {
                            profile::PlayerSide::P1 => 0,
                            profile::PlayerSide::P2 => 1,
                        };
                        let chart_ref = resolve_chart(idx);
                        let chart = Arc::new(chart_ref.clone());
                        let chart_ix = chart_ix_for_ref(chart_ref);
                        ([chart.clone(), chart], [chart_ix, chart_ix], idx)
                    }
                };

                let payload_started = Instant::now();
                let gameplay_song = match song_loading::load_gameplay_charts(
                    song_arc.as_ref(),
                    &chart_ixs,
                    config::get().global_offset_seconds,
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
                let payload_ms = payload_started.elapsed().as_secs_f64() * 1000.0;

                if play_style == profile::PlayStyle::Versus {
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

                let to_scroll_speed = |m: &player_options::SpeedMod| match m.mod_type {
                    player_options::SpeedModType::X => ScrollSpeedSetting::XMod(m.value),
                    player_options::SpeedModType::C => ScrollSpeedSetting::CMod(m.value),
                    player_options::SpeedModType::M => ScrollSpeedSetting::MMod(m.value),
                };
                let scroll_speeds = [
                    to_scroll_speed(&po_state.speed_mod[0]),
                    to_scroll_speed(&po_state.speed_mod[1]),
                ];

                let init_started = Instant::now();
                let mut gs = gameplay::init(
                    song_arc,
                    charts,
                    gameplay_charts,
                    po_state.active_color_index,
                    po_state.music_rate,
                    scroll_speeds,
                    po_state.player_profiles,
                    None,
                    None,
                    Some(Arc::from("Practice Mode")),
                    Arc::from("PRACTICE MODE"),
                    Some(crate::game::gameplay::LeadInTiming {
                        min_seconds_to_step: 0.0,
                        min_seconds_to_music: 0.0,
                    }),
                    None,
                    None,
                    [0; crate::game::gameplay::MAX_PLAYERS],
                );
                crate::game::gameplay::disable_score_for_practice(&mut gs);
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
                    if let Some(path) = gs.song.banner_path.as_ref() {
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
                debug!(
                    "Practice transition timing: song='{}' payload_ms={payload_ms:.3} init_ms={init_ms:.3} sfx_prewarm_ms={sfx_prewarm_ms:.3} asset_prewarm_ms={asset_prewarm_ms:.3} text_prewarm_ms={text_prewarm_ms:.3}",
                    gs.song.title
                );
                commands.push(Command::SetPackBanner(gs.pack_banner_path.clone()));
                let show_video_backgrounds = config::get().show_video_backgrounds;
                let background_path =
                    Self::refresh_gameplay_background_path(&mut gs, show_video_backgrounds);
                commands.push(Command::SetDynamicBackground(background_path));
                self.state.screens.practice_state = Some(practice::init(gs));
                if let Some(ps) = self.state.screens.practice_state.as_mut() {
                    crate::screens::practice::on_enter(ps);
                }
            } else {
                panic!("Navigating to Practice without PlayerOptions state!");
            }
        }
        if target == CurrentScreen::Gameplay {
            crate::engine::audio::stop_music();
            if prev != CurrentScreen::Gameplay {
                self.state.session.gameplay_restart_count = 0;
            }
            let mut course_display_carry = None;
            let course_display_totals = self
                .state
                .session
                .course_run
                .as_ref()
                .map(|course| course.course_display_totals);
            if prev == CurrentScreen::Gameplay
                && self.state.session.course_run.is_some()
                && let Some(gameplay_results) = self.state.screens.gameplay_state.take()
            {
                self.update_combo_carry_from_gameplay(&gameplay_results);
                course_display_carry = Some(
                    crate::game::gameplay::course_display_carry_from_state(&gameplay_results),
                );
                let color_idx = gameplay_results.active_color_index;
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
                    .map(|e| crate::game::gameplay::ReplayInputEdge {
                        lane_index: e.lane_index,
                        pressed: e.pressed,
                        source: e.source,
                        event_music_time_ns: e.event_music_time_ns,
                    })
                    .collect::<Vec<_>>()
            });
            let replay_offsets = replay_pending.as_ref().map(|payload| {
                crate::game::gameplay::ReplayOffsetSnapshot {
                    beat0_time_ns: payload.replay_beat0_time_ns,
                }
            });
            let replay_status_text = replay_pending.as_ref().map(|payload| {
                Arc::<str>::from(format!(
                    "Autoplay - {} {:.2}%",
                    payload.name,
                    payload.score / 100.0
                ))
            });
            if let Some(po_state) = self.state.screens.player_options_state.take() {
                let song_arc = po_state.song.clone();
                let play_style = profile::get_session_play_style();
                let player_side = profile::get_session_player_side();
                let target_chart_type = play_style.chart_type();
                let mut resolved_steps_index = po_state.chart_steps_index;
                let mut resolve_chart = |slot: usize| {
                    let requested_idx = resolved_steps_index[slot];
                    if let Some(chart_ref) = select_music::chart_for_steps_index(
                        &song_arc,
                        target_chart_type,
                        requested_idx,
                    ) {
                        return chart_ref;
                    }

                    let preferred_idx = po_state.chart_difficulty_index[slot];
                    if let Some(fallback_idx) =
                        select_music::best_steps_index(&song_arc, target_chart_type, preferred_idx)
                        && let Some(chart_ref) = select_music::chart_for_steps_index(
                            &song_arc,
                            target_chart_type,
                            fallback_idx,
                        )
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
                let chart_ix_for_ref = |chart_ref: &crate::game::chart::ChartData| {
                    song_arc
                        .charts
                        .iter()
                        .position(|chart| std::ptr::eq(chart, chart_ref))
                        .expect("selected chart ref must come from selected song")
                };
                let (charts, chart_ixs, last_played_chart_ref, last_played_idx) = match play_style {
                    profile::PlayStyle::Versus => {
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
                    profile::PlayStyle::Single | profile::PlayStyle::Double => {
                        let idx = match player_side {
                            profile::PlayerSide::P1 => 0,
                            profile::PlayerSide::P2 => 1,
                        };
                        let chart_ref = resolve_chart(idx);
                        let chart = Arc::new(chart_ref.clone());
                        let chart_ix = chart_ix_for_ref(chart_ref);
                        ([chart.clone(), chart], [chart_ix, chart_ix], chart_ref, idx)
                    }
                };

                let gameplay_entry_started = Instant::now();
                let reused_gameplay_charts =
                    if prev == CurrentScreen::Gameplay && self.state.session.course_run.is_none() {
                        self.state
                            .screens
                            .gameplay_state
                            .as_ref()
                            .filter(|current| {
                                can_reuse_quick_restart_payload(
                                    current.song.as_ref(),
                                    [
                                        current.charts[0].short_hash.as_str(),
                                        current.charts[1].short_hash.as_str(),
                                    ],
                                    song_arc.as_ref(),
                                    [charts[0].short_hash.as_str(), charts[1].short_hash.as_str()],
                                )
                            })
                            .map(|current| current.gameplay_charts.clone())
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
                    gameplay_charts
                } else {
                    let gameplay_song = match song_loading::load_gameplay_charts(
                        song_arc.as_ref(),
                        &chart_ixs,
                        config::get().global_offset_seconds,
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
                    [
                        Arc::new(gameplay_song[0].clone()),
                        Arc::new(gameplay_song[1].clone()),
                    ]
                };
                let payload_ms = payload_started.elapsed().as_secs_f64() * 1000.0;

                // Keep SelectMusic's current stepchart in sync with what we're about to play.
                if play_style == profile::PlayStyle::Versus {
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
                    profile::PlayStyle::Versus => {
                        commands.push(Command::UpdateLastPlayed {
                            side: profile::PlayerSide::P1,
                            play_style,
                            music_path: song_arc.music_path.clone(),
                            chart_hash: Some(charts[0].short_hash.clone()),
                            difficulty_index: po_state.chart_difficulty_index[0],
                        });
                        commands.push(Command::UpdateLastPlayed {
                            side: profile::PlayerSide::P2,
                            play_style,
                            music_path: song_arc.music_path.clone(),
                            chart_hash: Some(charts[1].short_hash.clone()),
                            difficulty_index: po_state.chart_difficulty_index[1],
                        });
                    }
                    profile::PlayStyle::Single | profile::PlayStyle::Double => {
                        commands.push(Command::UpdateLastPlayed {
                            side: player_side,
                            play_style,
                            music_path: song_arc.music_path.clone(),
                            chart_hash: Some(last_played_chart_ref.short_hash.clone()),
                            difficulty_index: po_state.chart_difficulty_index[last_played_idx],
                        });
                    }
                }

                let to_scroll_speed = |m: &player_options::SpeedMod| match m.mod_type {
                    player_options::SpeedModType::X => {
                        crate::game::scroll::ScrollSpeedSetting::XMod(m.value)
                    }
                    player_options::SpeedModType::C => {
                        crate::game::scroll::ScrollSpeedSetting::CMod(m.value)
                    }
                    player_options::SpeedModType::M => {
                        crate::game::scroll::ScrollSpeedSetting::MMod(m.value)
                    }
                };
                let scroll_speeds = [
                    to_scroll_speed(&po_state.speed_mod[0]),
                    to_scroll_speed(&po_state.speed_mod[1]),
                ];

                let color_index = po_state.active_color_index;
                let lead_in_timing = self.state.session.course_run.as_ref().and_then(|course| {
                    (course.next_stage_index > 0).then_some(crate::game::gameplay::LeadInTiming {
                        min_seconds_to_step: COURSE_MIN_SECONDS_TO_STEP_NEXT_SONG,
                        min_seconds_to_music: COURSE_MIN_SECONDS_TO_MUSIC_NEXT_SONG,
                    })
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
                        Arc::from("EVENT")
                    };
                let combo_carry = self.state.session.combo_carry;
                let init_started = Instant::now();
                let mut gs = gameplay::init(
                    song_arc,
                    charts,
                    gameplay_charts,
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
                    if let Some(path) = gs.song.banner_path.as_ref() {
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
                if total_ms >= 50.0 {
                    info!(
                        "Gameplay transition timing: song='{}' restart={} payload_source={} payload_ms={payload_ms:.3} init_ms={init_ms:.3} sfx_prewarm_ms={sfx_prewarm_ms:.3} asset_prewarm_ms={asset_prewarm_ms:.3} text_prewarm_ms={text_prewarm_ms:.3} elapsed_ms={total_ms:.3}",
                        gs.song.title,
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
                        gs.song.title,
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
                if let Some(course) = self.state.session.course_run.as_mut() {
                    course.next_stage_index = course.next_stage_index.saturating_add(1);
                }
            } else {
                panic!("Navigating to Gameplay without PlayerOptions state!");
            }
        }

        if target == CurrentScreen::Evaluation {
            let gameplay_results = self.state.screens.gameplay_state.take();
            if let Some(gs) = gameplay_results.as_ref() {
                self.update_combo_carry_from_gameplay(gs);
            }
            if let (Some(backend), Some(gs)) = (self.backend.as_mut(), gameplay_results.as_ref())
                && let Some(path) = gs.song.banner_path.as_ref()
            {
                media_cache::ensure_banner_texture(&mut self.asset_manager, backend, path);
            }
            let color_idx = gameplay_results.as_ref().map_or(
                self.state.screens.evaluation_state.active_color_index,
                |gs| gs.active_color_index,
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
                total_gameplay_elapsed(&self.state.session.played_stages);
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
            if self.state.session.session_start_time.is_none() {
                self.state.session.session_start_time = Some(Instant::now());
                self.state.session.played_stages.clear();
                self.state.session.course_individual_stage_indices.clear();
                debug!("Session timer started.");
            }

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
                            profile::PlayStyle::Versus => {
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
                            profile::PlayStyle::Single | profile::PlayStyle::Double => {
                                let side = profile::get_session_player_side();
                                let idx = match side {
                                    profile::PlayerSide::P1 => 0,
                                    profile::PlayerSide::P2 => 1,
                                };
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
                        if select_music::chart_for_steps_index(
                            song,
                            chart_type,
                            desired_steps_index,
                        )
                        .is_none()
                        {
                            let mut best_match_index = None;
                            let mut min_diff = i32::MAX;
                            for i in 0..color::FILE_DIFFICULTY_NAMES.len() {
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
                CurrentScreen::ProfileLoad => {
                    // SelectMusic state is prepared asynchronously while ProfileLoad is displayed.
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

                    let play_style = profile::get_session_play_style();
                    let max_diff_index = color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1);
                    let p2_profile = profile::get_for_side(profile::PlayerSide::P2);
                    let p2_pref = p2_profile
                        .last_played(play_style)
                        .difficulty_index
                        .min(max_diff_index);
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
                total_gameplay_elapsed(&self.state.session.played_stages);

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
                    select_music::chart_for_steps_index(
                        song,
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

            if profile::get_session_play_style() == profile::PlayStyle::Versus {
                let chart_to_graph_p2 = match self
                    .state
                    .screens
                    .select_music_state
                    .entries
                    .get(self.state.screens.select_music_state.selected_index)
                {
                    Some(select_music::MusicWheelEntry::Song(song)) => {
                        let chart_type = profile::get_session_play_style().chart_type();
                        select_music::chart_for_steps_index(
                            song,
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
            if self.state.session.session_start_time.is_none() {
                self.state.session.session_start_time = Some(Instant::now());
                self.state.session.played_stages.clear();
                self.state.session.course_individual_stage_indices.clear();
                debug!("Session timer started.");
            }

            match prev {
                CurrentScreen::ProfileLoad => {
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
                if self.state.screens.current_screen == CurrentScreen::Sandbox {
                    crate::screens::sandbox::handle_raw_pad_event(
                        &mut self.state.screens.sandbox_state,
                        &ev,
                    );
                } else if self.state.screens.current_screen == CurrentScreen::Mappings {
                    crate::screens::mappings::handle_raw_pad_event(
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
                }
                self.handle_pad_event(event_loop, ev);
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
            crate::game::online::init();
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
                    advance_redraw_deadline(self.state.shell.next_redraw_at, now, interval);
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
        std::thread::spawn(move || {
            input::run_windows_backend(
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
            );
        });
    }
    #[cfg(target_os = "linux")]
    {
        let proxy_pad = proxy.clone();
        let proxy_sys = proxy.clone();
        let proxy_key = proxy.clone();
        std::thread::spawn(move || {
            input::run_linux_backend(
                move |pe| {
                    let _ = proxy_pad.send_event(UserEvent::Pad(pe));
                },
                move |se| {
                    let _ = proxy_sys.send_event(UserEvent::GamepadSystem(se));
                },
                move |ke| {
                    let _ = proxy_key.send_event(UserEvent::Key(ke));
                },
            );
        });
    }
    #[cfg(target_os = "freebsd")]
    {
        let proxy_pad = proxy.clone();
        let proxy_sys = proxy.clone();
        let proxy_key = proxy.clone();
        std::thread::spawn(move || {
            input::run_freebsd_backend(
                move |pe| {
                    let _ = proxy_pad.send_event(UserEvent::Pad(pe));
                },
                move |se| {
                    let _ = proxy_sys.send_event(UserEvent::GamepadSystem(se));
                },
                move |ke| {
                    let _ = proxy_key.send_event(UserEvent::Key(ke));
                },
            );
        });
    }
    #[cfg(target_os = "macos")]
    {
        let proxy_pad = proxy.clone();
        let proxy_sys = proxy.clone();
        let proxy_key = proxy.clone();
        std::thread::spawn(move || {
            input::run_macos_backend(
                move |pe| {
                    let _ = proxy_pad.send_event(UserEvent::Pad(pe));
                },
                move |se| {
                    let _ = proxy_sys.send_event(UserEvent::GamepadSystem(se));
                },
                move |ke| {
                    let _ = proxy_key.send_event(UserEvent::Key(ke));
                },
            );
        });
    }
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{
        chart::{ChartData, StaminaCounts},
        parsing::song_lua::{SongLuaOverlayActor, SongLuaOverlayKind, SongLuaOverlayState},
        song::SongData,
    };

    fn test_chart(hash: &str) -> ChartData {
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: "Hard".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 9,
            step_artist: String::new(),
            music_path: None,
            short_hash: hash.to_string(),
            stats: rssp::stats::ArrowStats::default(),
            tech_counts: rssp::TechCounts::default(),
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
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: vec![test_chart(hashes[0]), test_chart(hashes[1])],
        }
    }

    #[test]
    fn song_lua_video_paths_filter_and_dedupe_video_sprites() {
        let movie = PathBuf::from("badapple.AVI");
        let overlays = vec![
            SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Sprite {
                    texture_path: movie.clone(),
                    texture_key: Arc::from(movie.to_string_lossy().into_owned()),
                },
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState {
                    decode_movie: true,
                    ..Default::default()
                },
                message_commands: Vec::new(),
            },
            SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Sprite {
                    texture_path: movie.clone(),
                    texture_key: Arc::from(movie.to_string_lossy().into_owned()),
                },
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState {
                    decode_movie: true,
                    ..Default::default()
                },
                message_commands: Vec::new(),
            },
            SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Sprite {
                    texture_path: PathBuf::from("panel.png"),
                    texture_key: Arc::from("panel.png"),
                },
                name: None,
                parent_index: None,
                initial_state: Default::default(),
                message_commands: Vec::new(),
            },
            SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Quad,
                name: None,
                parent_index: None,
                initial_state: Default::default(),
                message_commands: Vec::new(),
            },
        ];

        assert_eq!(song_lua_video_paths(&overlays), vec![movie]);
    }

    #[test]
    fn song_lua_video_paths_skip_disabled_video_decode() {
        let movie = PathBuf::from("badapple.AVI");
        let overlays = vec![SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Sprite {
                texture_path: movie.clone(),
                texture_key: Arc::from(movie.to_string_lossy().into_owned()),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState {
                decode_movie: false,
                ..Default::default()
            },
            message_commands: Vec::new(),
        }];

        assert!(song_lua_video_paths(&overlays).is_empty());
    }

    #[test]
    fn model_texture_sampler_forces_repeat_for_plain_textures() {
        let key = "noteskins/dance/custom/textures/Tap Note parts.png";
        let sampler = model_texture_sampler(key);

        assert_eq!(sampler.wrap, SamplerWrap::Repeat);
        assert_eq!(sampler.filter, SamplerFilter::Linear);
    }

    #[test]
    fn model_texture_sampler_preserves_texture_hints() {
        let key = "noteskins/dance/custom/textures/Tap Note parts (nearest mipmaps).png";
        let sampler = model_texture_sampler(key);

        assert_eq!(sampler.wrap, SamplerWrap::Repeat);
        assert_eq!(sampler.filter, SamplerFilter::Nearest);
        assert!(sampler.mipmaps);
    }

    fn test_score_info(
        song: Arc<SongData>,
        side: profile::PlayerSide,
        hash: &str,
        speed_mod: ScrollSpeedSetting,
        music_rate: f32,
    ) -> evaluation::ScoreInfo {
        evaluation::ScoreInfo {
            song: song.clone(),
            chart: Arc::new(test_chart(hash)),
            side,
            profile_name: String::new(),
            score_valid: true,
            disqualified: false,
            expected_groovestats_submit: false,
            expected_arrowcloud_submit: false,
            groovestats: crate::game::scores::GrooveStatsEvalState::default(),
            itl: crate::game::scores::ItlEvalState::default(),
            judgment_counts: [0; crate::game::judgment::JUDGE_GRADE_COUNT],
            score_percent: 0.0,
            grade: crate::game::scores::Grade::Tier01,
            speed_mod,
            mods_text: fallback_eval_mods_text(side, speed_mod),
            hands_achieved: 0,
            hands_total: 0,
            holds_held: 0,
            holds_total: 0,
            rolls_held: 0,
            rolls_total: 0,
            mines_avoided: 0,
            mines_total: 0,
            timing: crate::game::timing::TimingStats::default(),
            scatter: Vec::new(),
            scatter_worst_window_ms: 45.0,
            histogram: crate::game::timing::HistogramMs::default(),
            graph_first_second: 0.0,
            graph_last_second: song.precise_last_second(),
            music_rate,
            life_history: Vec::new(),
            fail_time: None,
            window_counts: crate::game::timing::WindowCounts::default(),
            window_counts_10ms: crate::game::timing::WindowCounts::default(),
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
            machine_records: Vec::new(),
            machine_record_highlight_rank: None,
            personal_records: Vec::new(),
            personal_record_highlight_rank: None,
            show_machine_personal_split: false,
        }
    }

    #[test]
    fn foreground_input_requires_focus_and_surface() {
        assert!(foreground_input_active(true, true));
        assert!(!foreground_input_active(false, true));
        assert!(!foreground_input_active(true, false));
        assert!(!foreground_input_active(false, false));
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
    fn replace_song_arc_if_same_simfile_swaps_in_reloaded_song() {
        let mut current = Arc::new(test_song("Songs/Test/song.ssc", 0.0, ["a", "b"]));
        let updated = Arc::new(test_song("Songs/Test/song.ssc", -0.003, ["a", "b"]));

        assert!(replace_song_arc_if_same_simfile(&mut current, &updated));
        assert!(Arc::ptr_eq(&current, &updated));
        assert!((current.offset + 0.003).abs() < 0.000_001_f32);
    }

    #[test]
    fn replace_song_arc_if_same_simfile_ignores_other_song() {
        let original = Arc::new(test_song("Songs/Test/a.ssc", 0.0, ["a", "b"]));
        let mut current = original.clone();
        let updated = Arc::new(test_song("Songs/Test/b.ssc", -0.003, ["a", "b"]));

        assert!(!replace_song_arc_if_same_simfile(&mut current, &updated));
        assert!(Arc::ptr_eq(&current, &original));
    }

    #[test]
    fn quick_restart_payload_reuse_rejects_song_offset_mismatch() {
        let current_song = test_song("Songs/Test/song.ssc", 0.0, ["a", "b"]);
        let updated_song = test_song("Songs/Test/song.ssc", -0.003, ["a", "b"]);

        assert!(!can_reuse_quick_restart_payload(
            &current_song,
            ["a", "b"],
            &updated_song,
            ["a", "b"],
        ));
    }

    #[test]
    fn quick_restart_payload_reuse_accepts_identical_song_and_charts() {
        let current_song = test_song("Songs/Test/song.ssc", -0.003, ["a", "b"]);
        let next_song = test_song("Songs/Test/song.ssc", -0.003, ["a", "b"]);

        assert!(can_reuse_quick_restart_payload(
            &current_song,
            ["a", "b"],
            &next_song,
            ["a", "b"],
        ));
    }

    #[test]
    fn evaluation_restart_payload_uses_score_side_for_single_p2() {
        let song = Arc::new(test_song("Songs/Test/song.ssc", 0.0, ["p1", "p2"]));
        let mut score_info = std::array::from_fn(|_| None);
        score_info[0] = Some(test_score_info(
            song.clone(),
            profile::PlayerSide::P2,
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

    // ---- Tab acceleration helper (issue #174) ----

    const EPS: f32 = 1e-6;

    #[test]
    fn tab_accel_no_modifier_is_passthrough() {
        let dt = 0.016_f32;
        assert!(
            (apply_tab_acceleration(dt, CurrentScreen::Menu, false, false, true) - dt).abs() < EPS
        );
    }

    #[test]
    fn tab_accel_fast_multiplies_by_four() {
        let dt = 0.016_f32;
        let out = apply_tab_acceleration(dt, CurrentScreen::Menu, true, false, true);
        assert!((out - dt * 4.0).abs() < EPS, "got {out}");
    }

    #[test]
    fn tab_accel_slow_divides_by_four() {
        let dt = 0.016_f32;
        let out = apply_tab_acceleration(dt, CurrentScreen::Menu, false, true, true);
        assert!((out - dt / 4.0).abs() < EPS, "got {out}");
    }

    #[test]
    fn tab_accel_both_held_halts() {
        let dt = 0.016_f32;
        let out = apply_tab_acceleration(dt, CurrentScreen::Menu, true, true, true);
        assert_eq!(out, 0.0);
    }

    #[test]
    fn tab_accel_gameplay_screen_never_scales() {
        let dt = 0.016_f32;
        for (fast, slow) in [(false, false), (true, false), (false, true), (true, true)] {
            let out = apply_tab_acceleration(dt, CurrentScreen::Gameplay, fast, slow, true);
            assert!(
                (out - dt).abs() < EPS,
                "Gameplay must passthrough; fast={fast} slow={slow} got={out}"
            );
        }
    }

    #[test]
    fn tab_accel_disabled_never_scales() {
        let dt = 0.016_f32;
        for screen in [
            CurrentScreen::Menu,
            CurrentScreen::SelectMusic,
            CurrentScreen::Evaluation,
        ] {
            for (fast, slow) in [(false, false), (true, false), (false, true), (true, true)] {
                let out = apply_tab_acceleration(dt, screen, fast, slow, false);
                assert!(
                    (out - dt).abs() < EPS,
                    "disabled must passthrough; screen={screen:?} fast={fast} slow={slow} got={out}"
                );
            }
        }
    }

    #[test]
    fn tab_accel_clamps_to_max_logic_dt() {
        // Big stall: 1s wall_dt with 4x fast-forward would yield 4s; must clamp to MAX.
        let out = apply_tab_acceleration(1.0, CurrentScreen::Menu, true, false, true);
        assert_eq!(out, MAX_LOGIC_DT_PER_FRAME);
    }

    #[test]
    fn tab_accel_clamp_does_not_affect_normal_frames() {
        let dt = 0.016_f32;
        let out = apply_tab_acceleration(dt, CurrentScreen::Menu, true, false, true);
        assert!(out < MAX_LOGIC_DT_PER_FRAME);
        assert!((out - 4.0 * dt).abs() < EPS);
    }
}
