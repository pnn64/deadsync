use crate::act;
use crate::assets::{AssetManager, DensityGraphSlot, DensityGraphSource};
use crate::config::{self, DisplayMode};
use crate::core::display;
use crate::core::gfx::{self as renderer, BackendType, create_backend};
use crate::core::input::{self, InputEvent};
use crate::core::space::{self as space, Metrics};
use crate::game::parsing::{noteskin, simfile as song_loading};
use crate::game::{profile, scores, scroll::ScrollSpeedSetting, stage_stats};
use crate::screens::{
    Screen as CurrentScreen, ScreenAction, credits, evaluation, evaluation_summary, gameover,
    gameplay, init, initials, input as input_screen, manage_local_profiles, mappings, menu,
    options, player_options, profile_load, sandbox, select_color, select_course, select_mode,
    select_music, select_profile, select_style,
};
use crate::ui::color;
use chrono::Local;
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    monitor::MonitorHandle,
    window::Window,
};

use log::{error, info, warn};
use std::borrow::Cow;
use std::cmp;
use std::collections::{HashMap, HashSet};
use std::{
    error::Error,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::ui::actors::Actor;
/* -------------------- gamepad -------------------- */
use crate::core::input::{GpSystemEvent, PadEvent};

/* -------------------- user events -------------------- */
#[derive(Debug, Clone)]
pub enum UserEvent {
    Pad(PadEvent),
    GamepadSystem(GpSystemEvent),
}

/// Imperative effects to be executed by the shell.
enum Command {
    ExitNow,
    SetBanner(Option<PathBuf>),
    SetCdTitle(Option<PathBuf>),
    SetPackBanner(Option<PathBuf>),
    SetDensityGraph {
        slot: DensityGraphSlot,
        chart_opt: Option<DensityGraphSource>,
    },
    FetchOnlineGrade(String),
    PlayMusic {
        path: PathBuf,
        looped: bool,
        volume: f32,
    },
    StopMusic,
    SetDynamicBackground(Option<PathBuf>),
    UpdateScrollSpeed {
        side: profile::PlayerSide,
        setting: ScrollSpeedSetting,
    },
    UpdateSessionMusicRate(f32),
    UpdatePreferredDifficulty(usize),
    UpdateLastPlayed {
        side: profile::PlayerSide,
        music_path: Option<PathBuf>,
        chart_hash: Option<String>,
        difficulty_index: usize,
    },
}

/* -------------------- transition timing constants -------------------- */
const FADE_OUT_DURATION: f32 = 0.4;
const MENU_TO_SELECT_COLOR_OUT_DURATION: f32 = 1.0;
const MENU_ACTORS_FADE_DURATION: f32 = 0.65;
const COURSE_MIN_SECONDS_TO_STEP_NEXT_SONG: f32 = 4.0;
const COURSE_MIN_SECONDS_TO_MUSIC_NEXT_SONG: f32 = 0.0;
const SCREENSHOT_FLASH_ATTACK_SECONDS: f32 = 0.02;
const SCREENSHOT_FLASH_DECAY_SECONDS: f32 = 0.18;
const SCREENSHOT_FLASH_MAX_ALPHA: f32 = 0.7;
const SCREENSHOT_DIR: &str = "save/screenshots";
const SCREENSHOT_PREVIEW_TEXTURE_KEY: &str = "__screenshot_preview";
const SCREENSHOT_PREVIEW_SCALE: f32 = 0.2;
const SCREENSHOT_PREVIEW_HOLD_SECONDS: f32 = 0.4;
const SCREENSHOT_PREVIEW_MACHINE_EXTRA_HOLD_SECONDS: f32 = 0.25;
const SCREENSHOT_PREVIEW_TWEEN_SECONDS: f32 = 0.75;
const SCREENSHOT_PREVIEW_GLOW_PERIOD_SECONDS: f32 = 0.5;
const SCREENSHOT_PREVIEW_GLOW_ALPHA: f32 = 0.2;
const SCREENSHOT_PREVIEW_BORDER_PX: f32 = 4.0;
const SCREENSHOT_PREVIEW_Z: i16 = 32010;
const GAMEPLAY_OFFSET_PROMPT_Z_BACKDROP: i16 = 31990;
const GAMEPLAY_OFFSET_PROMPT_Z_CURSOR: i16 = 31991;
const GAMEPLAY_OFFSET_PROMPT_Z_TEXT: i16 = 31993;

#[derive(Clone, Copy)]
enum ScreenshotPreviewTarget {
    Player(profile::PlayerSide),
    Machine,
}

#[derive(Clone, Copy)]
struct ScreenshotPreviewState {
    started_at: Instant,
    target: ScreenshotPreviewTarget,
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

/* -------------------- transition state machine -------------------- */
#[derive(Debug)]
enum TransitionState {
    Idle,
    FadingOut {
        elapsed: f32,
        duration: f32,
        target: CurrentScreen,
    },
    FadingIn {
        elapsed: f32,
        duration: f32,
    },
    ActorsFadeOut {
        elapsed: f32,
        duration: f32,
        target: CurrentScreen,
    },
    ActorsFadeIn {
        elapsed: f32,
    },
}

const STUTTER_SAMPLE_COUNT: usize = 5;
const STUTTER_SAMPLE_LIFETIME: f32 = 3.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayMode {
    Off,
    Fps,
    FpsAndStutter,
}

impl OverlayMode {
    #[inline(always)]
    const fn from_code(mode: u8) -> Self {
        match mode {
            1 => Self::Fps,
            2 => Self::FpsAndStutter,
            _ => Self::Off,
        }
    }

    #[inline(always)]
    const fn next(self) -> Self {
        match self {
            Self::Off => Self::Fps,
            Self::Fps => Self::FpsAndStutter,
            Self::FpsAndStutter => Self::Off,
        }
    }

    #[inline(always)]
    const fn shows_fps(self) -> bool {
        !matches!(self, Self::Off)
    }

    #[inline(always)]
    const fn shows_stutter(self) -> bool {
        matches!(self, Self::FpsAndStutter)
    }

    #[inline(always)]
    const fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Fps => "FPS",
            Self::FpsAndStutter => "FPS+STUTTER",
        }
    }

    #[inline(always)]
    const fn code(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Fps => 1,
            Self::FpsAndStutter => 2,
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

/// Shell-level state: timing, window, renderer flags.
pub struct ShellState {
    frame_count: u32,
    last_title_update: Instant,
    last_frame_time: Instant,
    start_time: Instant,
    vsync_enabled: bool,
    frame_interval: Option<Duration>,
    next_redraw_at: Instant,
    display_mode: DisplayMode,
    display_monitor: usize,
    metrics: Metrics,
    last_fps: f32,
    last_vpf: u32,
    current_frame_vpf: u32,
    overlay_mode: OverlayMode,
    stutter_samples: [StutterSample; STUTTER_SAMPLE_COUNT],
    stutter_cursor: usize,
    transition: TransitionState,
    display_width: u32,
    display_height: u32,
    pending_window_position: Option<PhysicalPosition<i32>>,
    gamepad_overlay_state: Option<(String, Instant)>,
    pending_exit: bool,
    shift_held: bool,
    ctrl_held: bool,
    screenshot_pending: bool,
    screenshot_request_side: Option<profile::PlayerSide>,
    screenshot_flash_started_at: Option<Instant>,
    screenshot_preview: Option<ScreenshotPreviewState>,
}

#[inline(always)]
fn frame_interval_for_max_fps(max_fps: u16) -> Option<Duration> {
    if max_fps == 0 {
        None
    } else {
        Some(Duration::from_secs_f64(1.0 / f64::from(max_fps)))
    }
}

/// Active screen data bundle.
pub struct ScreensState {
    current_screen: CurrentScreen,
    menu_state: menu::State,
    gameplay_state: Option<gameplay::State>,
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
            start_time: now,
            vsync_enabled: cfg.vsync,
            frame_interval,
            next_redraw_at: now,
            display_mode: cfg.display_mode(),
            metrics,
            last_fps: 0.0,
            last_vpf: 0,
            current_frame_vpf: 0,
            overlay_mode: OverlayMode::from_code(overlay_mode),
            stutter_samples: [StutterSample::empty(); STUTTER_SAMPLE_COUNT],
            stutter_cursor: 0,
            transition: TransitionState::Idle,
            display_width: cfg.display_width,
            display_height: cfg.display_height,
            display_monitor: cfg.display_monitor,
            pending_window_position: None,
            gamepad_overlay_state: None,
            pending_exit: false,
            shift_held: false,
            ctrl_held: false,
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

    #[inline(always)]
    fn screenshot_flash_alpha(&self, now: Instant) -> f32 {
        let Some(started_at) = self.screenshot_flash_started_at else {
            return 0.0;
        };
        let elapsed = now.duration_since(started_at).as_secs_f32();
        let total = SCREENSHOT_FLASH_ATTACK_SECONDS + SCREENSHOT_FLASH_DECAY_SECONDS;
        if elapsed <= 0.0 || elapsed >= total {
            return 0.0;
        }
        if elapsed <= SCREENSHOT_FLASH_ATTACK_SECONDS {
            return (elapsed / SCREENSHOT_FLASH_ATTACK_SECONDS).clamp(0.0, 1.0)
                * SCREENSHOT_FLASH_MAX_ALPHA;
        }
        let fade =
            1.0 - ((elapsed - SCREENSHOT_FLASH_ATTACK_SECONDS) / SCREENSHOT_FLASH_DECAY_SECONDS);
        fade.clamp(0.0, 1.0) * SCREENSHOT_FLASH_MAX_ALPHA
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

#[inline(always)]
const fn machine_startup_screen_enabled(cfg: &config::Config, screen: CurrentScreen) -> bool {
    match screen {
        CurrentScreen::SelectProfile => cfg.machine_show_select_profile,
        CurrentScreen::SelectColor => cfg.machine_show_select_color,
        CurrentScreen::SelectStyle => cfg.machine_show_select_style,
        CurrentScreen::SelectPlayMode => cfg.machine_show_select_play_mode,
        _ => true,
    }
}

fn machine_resolve_startup_target(cfg: &config::Config, target: CurrentScreen) -> CurrentScreen {
    let order = [
        CurrentScreen::SelectProfile,
        CurrentScreen::SelectColor,
        CurrentScreen::SelectStyle,
        CurrentScreen::SelectPlayMode,
    ];
    let Some(start_idx) = order.iter().position(|screen| *screen == target) else {
        return target;
    };
    for screen in order.iter().skip(start_idx) {
        if machine_startup_screen_enabled(cfg, *screen) {
            return *screen;
        }
    }
    CurrentScreen::ProfileLoad
}

#[inline(always)]
fn machine_first_post_select_target(cfg: &config::Config) -> CurrentScreen {
    if cfg.machine_show_eval_summary {
        CurrentScreen::EvaluationSummary
    } else if cfg.machine_show_name_entry {
        CurrentScreen::Initials
    } else if cfg.machine_show_gameover {
        CurrentScreen::GameOver
    } else {
        CurrentScreen::Menu
    }
}

#[inline(always)]
fn machine_resolve_post_select_target(
    cfg: &config::Config,
    target: CurrentScreen,
) -> CurrentScreen {
    match target {
        CurrentScreen::EvaluationSummary => {
            if cfg.machine_show_eval_summary {
                CurrentScreen::EvaluationSummary
            } else if cfg.machine_show_name_entry {
                CurrentScreen::Initials
            } else if cfg.machine_show_gameover {
                CurrentScreen::GameOver
            } else {
                CurrentScreen::Menu
            }
        }
        CurrentScreen::Initials => {
            if cfg.machine_show_name_entry {
                CurrentScreen::Initials
            } else if cfg.machine_show_gameover {
                CurrentScreen::GameOver
            } else {
                CurrentScreen::Menu
            }
        }
        CurrentScreen::GameOver => {
            if cfg.machine_show_gameover {
                CurrentScreen::GameOver
            } else {
                CurrentScreen::Menu
            }
        }
        other => other,
    }
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
    let global_offset_seconds = crate::config::get().global_offset_seconds;
    let mut course_display_totals =
        [crate::game::gameplay::CourseDisplayTotals::default(); crate::game::gameplay::MAX_PLAYERS];
    for stage in &stages {
        for player_idx in 0..crate::game::gameplay::MAX_PLAYERS {
            let Some(chart) = select_music::chart_for_steps_index(
                stage.song.as_ref(),
                chart_type,
                stage.steps_index[player_idx],
            ) else {
                continue;
            };
            let add = crate::game::gameplay::course_display_totals_for_chart(
                chart,
                global_offset_seconds,
            );
            let total = &mut course_display_totals[player_idx];
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
    summary_song.simfile_path = course.path.clone();
    summary_song.title = course.name.clone();
    summary_song.translit_title = course.name.clone();
    summary_song.banner_path = course.banner_path.clone();
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
        let mut meter_sum = 0u32;
        let mut meter_count = 0u32;
        let mut any_failed = false;
        let mut show_w0 = false;
        let mut show_ex = false;
        let mut show_hard_ex = false;
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
            meter_sum = meter_sum.saturating_add(player.chart.meter);
            meter_count = meter_count.saturating_add(1);
            any_failed |= player.grade == scores::Grade::Failed;
            show_w0 |= player.show_w0;
            show_ex |= player.show_ex_score;
            show_hard_ex |= player.show_hard_ex_score;
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
        if grade != scores::Grade::Failed && show_w0 && ex_score_percent >= 100.0 {
            grade = scores::Grade::Quint;
        }
        let mut summary_chart = (*first_player.chart).clone();
        summary_chart.short_hash = course.score_hash.clone();
        summary_chart.difficulty = course.course_difficulty_name.clone();
        summary_chart.description = course.name.clone();
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
            grade,
            score_percent,
            ex_score_percent,
            hard_ex_score_percent,
            notes_hit,
            window_counts: counts,
            window_counts_10ms: counts_10ms,
            show_w0,
            show_ex_score: show_ex,
            show_hard_ex_score: show_hard_ex,
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

#[inline(always)]
fn highlight_rank_for_initials(
    entries: &[scores::LeaderboardEntry],
    initials: &str,
    score_percent: f64,
) -> Option<u32> {
    if initials.trim().is_empty() {
        return None;
    }
    let target = (score_percent * 10000.0).round();
    entries
        .iter()
        .find(|entry| entry.name == initials && (entry.score - target).abs() <= 0.5)
        .map(|entry| entry.rank.max(1))
}

fn score_info_from_stage(
    stage: &stage_stats::StageSummary,
    side: profile::PlayerSide,
) -> Option<evaluation::ScoreInfo> {
    let idx = side_ix(side);
    let player = stage.players[idx].as_ref()?;
    let mut judgment_counts = HashMap::new();
    judgment_counts.insert(
        crate::game::judgment::JudgeGrade::Fantastic,
        player
            .window_counts
            .w0
            .saturating_add(player.window_counts.w1),
    );
    judgment_counts.insert(
        crate::game::judgment::JudgeGrade::Excellent,
        player.window_counts.w2,
    );
    judgment_counts.insert(
        crate::game::judgment::JudgeGrade::Great,
        player.window_counts.w3,
    );
    judgment_counts.insert(
        crate::game::judgment::JudgeGrade::Decent,
        player.window_counts.w4,
    );
    judgment_counts.insert(
        crate::game::judgment::JudgeGrade::WayOff,
        player.window_counts.w5,
    );
    judgment_counts.insert(
        crate::game::judgment::JudgeGrade::Miss,
        player.window_counts.miss,
    );

    let chart_hash = player.chart.short_hash.as_str();
    let machine_records = scores::get_machine_leaderboard_local(chart_hash, usize::MAX);
    let personal_records =
        scores::get_personal_leaderboard_local_for_side(chart_hash, side, usize::MAX);
    let initials = profile::get_for_side(side).player_initials;
    let machine_record_highlight_rank = highlight_rank_for_initials(
        machine_records.as_slice(),
        initials.as_str(),
        player.score_percent,
    );
    let personal_record_highlight_rank = highlight_rank_for_initials(
        personal_records.as_slice(),
        initials.as_str(),
        player.score_percent,
    );
    let earned_machine_record = machine_record_highlight_rank.is_some_and(|rank| rank <= 10);
    let earned_top2_personal = personal_record_highlight_rank.is_some_and(|rank| rank <= 2);

    Some(evaluation::ScoreInfo {
        song: stage.song.clone(),
        chart: player.chart.clone(),
        profile_name: player.profile_name.clone(),
        judgment_counts,
        score_percent: player.score_percent,
        grade: player.grade,
        speed_mod: profile::get_for_side(side).scroll_speed,
        hands_achieved: 0,
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
        scroll_option: profile::get_for_side(side).scroll_option,
        life_history: Vec::new(),
        fail_time: (player.grade == scores::Grade::Failed).then_some(stage.duration_seconds),
        window_counts: player.window_counts,
        window_counts_10ms: player.window_counts_10ms,
        ex_score_percent: player.ex_score_percent,
        hard_ex_score_percent: player.hard_ex_score_percent,
        column_judgments: Vec::new(),
        noteskin: None,
        show_fa_plus_window: player.show_w0,
        show_ex_score: player.show_ex_score,
        show_hard_ex_score: player.show_hard_ex_score,
        show_fa_plus_pane: player.show_w0,
        machine_records,
        machine_record_highlight_rank,
        personal_records,
        personal_record_highlight_rank,
        show_machine_personal_split: !earned_machine_record && earned_top2_personal,
    })
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

fn save_screenshot_image(image: &image::RgbaImage) -> Result<PathBuf, Box<dyn Error>> {
    let dir = PathBuf::from(SCREENSHOT_DIR);
    std::fs::create_dir_all(&dir)?;

    let stamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let mut path = dir.join(format!("deadsync-{stamp}.png"));
    let mut suffix = 1_u32;
    while path.exists() {
        path = dir.join(format!("deadsync-{stamp}-{suffix:02}.png"));
        suffix = suffix.saturating_add(1);
        if suffix > 9_999 {
            return Err(
                std::io::Error::other("Failed to allocate unique screenshot filename").into(),
            );
        }
    }

    image.save_with_format(&path, image::ImageFormat::Png)?;
    Ok(path)
}

#[inline(always)]
fn set_opaque_alpha(image: &mut image::RgbaImage) {
    for pixel in image.pixels_mut() {
        pixel.0[3] = 255;
    }
}

fn prewarm_gameplay_assets(
    assets: &mut AssetManager,
    backend: &mut renderer::Backend,
    state: &gameplay::State,
) {
    let mut seen = HashSet::<String>::with_capacity(256);
    for noteskin in state.noteskin.iter().flatten() {
        noteskin.for_each_texture_key(|key| {
            if seen.insert(key.to_owned()) {
                assets.ensure_texture_for_key(backend, key);
            }
        });
    }
    crate::core::audio::preload_sfx("assets/sounds/boom.ogg");
    crate::core::audio::preload_sfx("assets/sounds/assist_tick.ogg");
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
        grade: si.grade,
        score_percent: si.score_percent,
        ex_score_percent: si.ex_score_percent,
        hard_ex_score_percent: si.hard_ex_score_percent,
        notes_hit: notes_hit(si),
        window_counts: si.window_counts,
        window_counts_10ms: si.window_counts_10ms,
        show_w0: (si.show_fa_plus_window && si.show_fa_plus_pane) || si.show_ex_score,
        show_ex_score: si.show_ex_score,
        show_hard_ex_score: si.show_hard_ex_score,
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
            let Some(si) = eval.score_info.first().and_then(|s| s.as_ref()) else {
                return None;
            };
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
    ) -> Option<ScreenAction> {
        match self.current_screen {
            CurrentScreen::Gameplay => self
                .gameplay_state
                .as_mut()
                .map(|gs| gameplay::update(gs, delta_time)),
            CurrentScreen::Init => Some(init::update(&mut self.init_state, delta_time)),
            CurrentScreen::Options => {
                options::update(&mut self.options_state, delta_time, asset_manager)
            }
            CurrentScreen::Credits => {
                credits::update(&mut self.credits_state, delta_time);
                None
            }
            CurrentScreen::ManageLocalProfiles => {
                manage_local_profiles::update(&mut self.manage_local_profiles_state, delta_time)
            }
            CurrentScreen::Mappings => {
                mappings::update(&mut self.mappings_state, delta_time);
                None
            }
            CurrentScreen::Input => input_screen::update(&mut self.input_state, delta_time),
            CurrentScreen::PlayerOptions => {
                if let Some(pos) = &mut self.player_options_state {
                    player_options::update(pos, delta_time, asset_manager);
                }
                None
            }
            CurrentScreen::Sandbox => {
                sandbox::update(&mut self.sandbox_state, delta_time);
                None
            }
            CurrentScreen::SelectProfile => {
                select_profile::update(&mut self.select_profile_state, delta_time);
                None
            }
            CurrentScreen::SelectColor => {
                select_color::update(&mut self.select_color_state, delta_time);
                None
            }
            CurrentScreen::SelectStyle => {
                select_style::update(&mut self.select_style_state, delta_time)
            }
            CurrentScreen::SelectPlayMode => {
                select_mode::update(&mut self.select_play_mode_state, delta_time)
            }
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

                    let max_diff_index = color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1);
                    let p2_pref = profile::get_for_side(profile::PlayerSide::P2)
                        .last_difficulty_index
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
                action
            }
            CurrentScreen::Evaluation => {
                if let Some(start) = session.session_start_time {
                    self.evaluation_state.session_elapsed = now.duration_since(start).as_secs_f32();
                }
                self.evaluation_state.gameplay_elapsed =
                    total_gameplay_elapsed(&session.played_stages);
                evaluation::update(&mut self.evaluation_state, delta_time);
                if let Some(delay) = self.evaluation_state.auto_advance_seconds
                    && self.evaluation_state.screen_elapsed >= delay
                    && self.player_options_state.is_some()
                {
                    Some(ScreenAction::Navigate(CurrentScreen::Gameplay))
                } else {
                    None
                }
            }
            CurrentScreen::EvaluationSummary => {
                evaluation_summary::update(&mut self.evaluation_summary_state, delta_time);
                None
            }
            CurrentScreen::Initials => initials::update(&mut self.initials_state, delta_time),
            CurrentScreen::GameOver => gameover::update(&mut self.gameover_state, delta_time),
            CurrentScreen::SelectMusic => {
                if let Some(start) = session.session_start_time {
                    self.select_music_state.session_elapsed =
                        now.duration_since(start).as_secs_f32();
                }
                self.select_music_state.gameplay_elapsed =
                    total_gameplay_elapsed(&session.played_stages);
                Some(select_music::update(
                    &mut self.select_music_state,
                    delta_time,
                ))
            }
            CurrentScreen::SelectCourse => {
                if let Some(start) = session.session_start_time {
                    self.select_course_state.session_elapsed =
                        now.duration_since(start).as_secs_f32();
                }
                Some(select_course::update(
                    &mut self.select_course_state,
                    delta_time,
                ))
            }
            CurrentScreen::Menu => None,
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
        let max_diff_index = crate::ui::color::FILE_DIFFICULTY_NAMES
            .len()
            .saturating_sub(1);
        let preferred = if max_diff_index == 0 {
            0
        } else {
            cmp::min(profile_data.last_difficulty_index, max_diff_index)
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
    asset_manager: AssetManager,
    state: AppState,
    software_renderer_threads: u8,
    gfx_debug_enabled: bool,
}

impl App {
    #[inline(always)]
    const fn is_actor_fade_screen(screen: CurrentScreen) -> bool {
        matches!(
            screen,
            CurrentScreen::Menu
                | CurrentScreen::Options
                | CurrentScreen::ManageLocalProfiles
                | CurrentScreen::Mappings
                | CurrentScreen::Input
        )
    }

    fn update_options_monitor_specs(&mut self, event_loop: &ActiveEventLoop) {
        let monitors: Vec<MonitorHandle> = event_loop.available_monitors().collect();
        let specs = display::monitor_specs(&monitors);
        options::update_monitor_specs(&mut self.state.screens.options_state, specs);
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
            asset_manager: AssetManager::new(),
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
                    self.asset_manager.set_profile_avatar_for_side(
                        backend,
                        profile::PlayerSide::P1,
                        profile_data[0].avatar_path.clone(),
                    );
                    self.asset_manager.set_profile_avatar_for_side(
                        backend,
                        profile::PlayerSide::P2,
                        profile_data[1].avatar_path.clone(),
                    );
                }

                let max_diff_index = crate::ui::color::FILE_DIFFICULTY_NAMES
                    .len()
                    .saturating_sub(1);
                let preferred_p1 = if max_diff_index == 0 {
                    0
                } else {
                    cmp::min(profile_data[0].last_difficulty_index, max_diff_index)
                };
                let preferred_p2 = if max_diff_index == 0 {
                    0
                } else {
                    cmp::min(profile_data[1].last_difficulty_index, max_diff_index)
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
            ScreenAction::RequestBanner(path_opt) => vec![Command::SetBanner(path_opt)],
            ScreenAction::RequestCdTitle(path_opt) => vec![Command::SetCdTitle(path_opt)],
            ScreenAction::RequestDensityGraph { slot, chart_opt } => {
                vec![Command::SetDensityGraph { slot, chart_opt }]
            }
            ScreenAction::FetchOnlineGrade(hash) => vec![Command::FetchOnlineGrade(hash)],
            ScreenAction::ChangeGraphics {
                renderer,
                display_mode,
                resolution,
                monitor,
                max_fps,
            } => {
                // Ensure options menu reflects current hardware state before processing changes
                self.update_options_monitor_specs(event_loop);

                if let Some(max_fps) = max_fps {
                    self.state.shell.set_max_fps(max_fps);
                    config::update_max_fps(max_fps);
                    options::sync_max_fps(&mut self.state.screens.options_state, max_fps);
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

                match (renderer, display_mode) {
                    (Some(new_backend), Some(mode)) => {
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
                        self.switch_renderer(new_backend, pending_resolution, event_loop)?;
                    }
                    (None, Some(mode)) => {
                        self.apply_display_mode(mode, Some(chosen_monitor), event_loop)?;
                        if let Some((w, h)) = pending_resolution {
                            self.apply_resolution(w, h, event_loop)?;
                        }
                    }
                    (Some(new_backend), None) => {
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
                        self.switch_renderer(new_backend, pending_resolution, event_loop)?;
                    }
                    (None, None) => {
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
                Vec::new()
            }
            ScreenAction::UpdateShowOverlay(mode) => {
                self.state.shell.set_overlay_mode(mode);
                config::update_show_stats_mode(mode);
                options::sync_show_stats_mode(&mut self.state.screens.options_state, mode);
                Vec::new()
            }
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

    fn save_gameplay_song_offset(&mut self, simfile_path: &Path, delta: f32) -> Result<(), String> {
        if delta.abs() < 0.000_001_f32 {
            return Ok(());
        }
        let changed_tags = save_song_offset_delta_to_simfile(simfile_path, delta)?;
        let _ = song_loading::reload_song_in_cache(simfile_path)?;
        select_music::refresh_from_song_cache(&mut self.state.screens.select_music_state);
        info!(
            "Saved song offset sync changes to '{}' (updated {} #OFFSET tags; refreshed song cache).",
            simfile_path.display(),
            changed_tags
        );
        Ok(())
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
        if from != CurrentScreen::Gameplay || target == CurrentScreen::Gameplay {
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
                    crate::core::audio::play_sfx("assets/sounds/change.ogg");
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
                    crate::core::audio::play_sfx("assets/sounds/change.ogg");
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
                crate::core::audio::play_sfx("assets/sounds/start.ogg");
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

    fn handle_navigation_action(&mut self, target: CurrentScreen) {
        self.handle_navigation_action_inner(target, true);
    }

    fn handle_navigation_action_after_prompt(&mut self, target: CurrentScreen) {
        self.handle_navigation_action_inner(target, false);
    }

    fn handle_navigation_action_inner(&mut self, target: CurrentScreen, allow_offset_prompt: bool) {
        let from = self.state.screens.current_screen;
        let mut target = target;
        let cfg = config::get();

        // After at least one stage, leaving song/course select routes through the
        // machine-configured post-session flow before returning to title.
        if (from == CurrentScreen::SelectMusic || from == CurrentScreen::SelectCourse)
            && target == CurrentScreen::Menu
            && !self.state.session.played_stages.is_empty()
        {
            target = machine_first_post_select_target(&cfg);
        }

        let startup_flow = matches!(
            from,
            CurrentScreen::Menu
                | CurrentScreen::SelectProfile
                | CurrentScreen::SelectColor
                | CurrentScreen::SelectStyle
                | CurrentScreen::SelectPlayMode
        ) && matches!(
            target,
            CurrentScreen::SelectProfile
                | CurrentScreen::SelectColor
                | CurrentScreen::SelectStyle
                | CurrentScreen::SelectPlayMode
                | CurrentScreen::ProfileLoad
        );
        if startup_flow {
            target = machine_resolve_startup_target(&cfg, target);
        }
        target = machine_resolve_post_select_target(&cfg, target);

        // If Select Profile is disabled and gameplay was started from Menu,
        // initialize joined/session-side defaults from the Start button used on Menu.
        if startup_flow
            && from == CurrentScreen::Menu
            && target != CurrentScreen::SelectProfile
            && !cfg.machine_show_select_profile
            && matches!(
                target,
                CurrentScreen::SelectColor
                    | CurrentScreen::SelectStyle
                    | CurrentScreen::SelectPlayMode
                    | CurrentScreen::ProfileLoad
            )
        {
            let p2_started = self.state.screens.menu_state.started_by_p2;
            profile::set_session_player_side(if p2_started {
                profile::PlayerSide::P2
            } else {
                profile::PlayerSide::P1
            });
            profile::set_session_joined(!p2_started, p2_started);
            profile::set_fast_profile_switch_from_select_music(false);
        }

        if allow_offset_prompt && self.maybe_begin_gameplay_offset_prompt(from, target, false) {
            return;
        }

        if from == CurrentScreen::Init && target == CurrentScreen::Menu {
            info!("Instant navigation Init→Menu (out-transition handled by Init screen)");
            self.state.screens.current_screen = target;
            self.state.shell.transition = TransitionState::ActorsFadeIn { elapsed: 0.0 };
            crate::ui::runtime::clear_all();
            return;
        }

        if !matches!(self.state.shell.transition, TransitionState::Idle) {
            return;
        }

        self.state.shell.pending_exit = false;
        if self.is_actor_only_fade(from, target) {
            self.start_actor_fade(from, target);
        } else {
            self.start_global_fade(target);
        }
    }

    fn clear_course_runtime(&mut self) {
        self.state.session.course_run = None;
        self.state.session.course_eval_pages.clear();
        self.state.session.course_eval_page_index = 0;
    }

    fn update_combo_carry_from_gameplay(&mut self, gs: &gameplay::State) {
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

    fn prepare_player_options_for_gameplay_restart(&mut self) -> bool {
        let Some(gs) = self.state.screens.gameplay_state.as_ref() else {
            return false;
        };

        let play_style = profile::get_session_play_style();
        let player_side = profile::get_session_player_side();
        let target_chart_type = play_style.chart_type();
        let fallback_steps = self.state.session.preferred_difficulty_index;

        let p1_steps = select_music::steps_index_for_chart_hash(
            &gs.song,
            target_chart_type,
            gs.charts[0].short_hash.as_str(),
        )
        .unwrap_or(fallback_steps);
        let p2_steps = select_music::steps_index_for_chart_hash(
            &gs.song,
            target_chart_type,
            gs.charts[1].short_hash.as_str(),
        )
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
            gs.song.clone(),
            chart_steps_index,
            chart_steps_index,
            gs.active_color_index,
            CurrentScreen::Gameplay,
            None,
        );
        po_state.music_rate = gs.music_rate;
        po_state.player_profiles = gs.player_profiles.clone();
        po_state.speed_mod = std::array::from_fn(|i| match gs.scroll_speed[i] {
            ScrollSpeedSetting::XMod(v) => player_options::SpeedMod {
                mod_type: "X".to_string(),
                value: v,
            },
            ScrollSpeedSetting::CMod(v) => player_options::SpeedMod {
                mod_type: "C".to_string(),
                value: v,
            },
            ScrollSpeedSetting::MMod(v) => player_options::SpeedMod {
                mod_type: "M".to_string(),
                value: v,
            },
        });
        self.state.screens.player_options_state = Some(po_state);
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
                    profile::add_stage_calories_for_side(side, p.notes_hit);
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
        crate::core::audio::play_sfx("assets/sounds/change.ogg");
    }

    fn is_actor_only_fade(&self, from: CurrentScreen, to: CurrentScreen) -> bool {
        (from == CurrentScreen::Menu
            && (to == CurrentScreen::Options
                || to == CurrentScreen::SelectProfile
                || to == CurrentScreen::SelectColor))
            || ((from == CurrentScreen::Options
                || from == CurrentScreen::SelectProfile
                || from == CurrentScreen::SelectColor)
                && to == CurrentScreen::Menu)
            || (from == CurrentScreen::SelectProfile && to == CurrentScreen::SelectColor)
            || (from == CurrentScreen::SelectProfile && to == CurrentScreen::SelectStyle)
            || (from == CurrentScreen::SelectStyle && to == CurrentScreen::SelectProfile)
            || (from == CurrentScreen::SelectColor && to == CurrentScreen::SelectStyle)
            || (from == CurrentScreen::SelectStyle && to == CurrentScreen::SelectColor)
            || (from == CurrentScreen::Options && to == CurrentScreen::Mappings)
            || (from == CurrentScreen::Mappings && to == CurrentScreen::Options)
            || (from == CurrentScreen::Options && to == CurrentScreen::ManageLocalProfiles)
            || (from == CurrentScreen::ManageLocalProfiles && to == CurrentScreen::Options)
    }

    fn start_actor_fade(&mut self, from: CurrentScreen, target: CurrentScreen) {
        info!("Starting actor-only fade out to screen: {target:?}");
        let duration = if from == CurrentScreen::Menu
            && (target == CurrentScreen::SelectProfile
                || target == CurrentScreen::SelectColor
                || target == CurrentScreen::Options)
        {
            MENU_TO_SELECT_COLOR_OUT_DURATION
        } else if from == CurrentScreen::SelectColor {
            select_color::exit_anim_duration()
        } else if from == CurrentScreen::SelectProfile {
            select_profile::exit_anim_duration()
        } else {
            FADE_OUT_DURATION
        };
        self.state.shell.transition = TransitionState::ActorsFadeOut {
            elapsed: 0.0,
            duration,
            target,
        };
    }

    fn start_global_fade(&mut self, target: CurrentScreen) {
        info!("Starting global fade out to screen: {target:?}");
        let (_, out_duration) =
            self.get_out_transition_for_screen(self.state.screens.current_screen);
        self.state.shell.transition = TransitionState::FadingOut {
            elapsed: 0.0,
            duration: out_duration,
            target,
        };
    }

    fn handle_exit_action(&mut self) -> Vec<Command> {
        if self.state.screens.current_screen == CurrentScreen::Menu
            && matches!(self.state.shell.transition, TransitionState::Idle)
        {
            info!("Exit requested from Menu; playing menu out-transition before shutdown.");
            let (_, out_duration) =
                self.get_out_transition_for_screen(self.state.screens.current_screen);
            self.state.shell.transition = TransitionState::FadingOut {
                elapsed: 0.0,
                duration: out_duration,
                target: self.state.screens.current_screen,
            };
            self.state.shell.pending_exit = true;
            Vec::new()
        } else {
            info!("Exit action received. Shutting down.");
            vec![Command::ExitNow]
        }
    }

    fn apply_select_music_join(&mut self, join_side: profile::PlayerSide) {
        let max_diff_index = color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1);
        let p1_pref = profile::get_for_side(profile::PlayerSide::P1)
            .last_difficulty_index
            .min(max_diff_index);
        let p2_pref = profile::get_for_side(profile::PlayerSide::P2)
            .last_difficulty_index
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

        crate::core::audio::play_sfx("assets/sounds/start.ogg");
        true
    }

    fn route_input_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        ev: InputEvent,
    ) -> Result<(), Box<dyn Error>> {
        if self.route_gameplay_offset_prompt_input(event_loop, &ev) {
            return Ok(());
        }
        if self.try_handle_late_join(&ev) {
            return Ok(());
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
                    crate::game::gameplay::handle_input(gs, &ev)
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

    fn run_commands(
        &mut self,
        commands: Vec<Command>,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        for command in commands {
            self.execute_command(command, event_loop)?;
        }
        Ok(())
    }

    #[inline(always)]
    const fn should_log_command_timing(command: &Command) -> bool {
        matches!(
            command,
            Command::SetBanner(_)
                | Command::SetCdTitle(_)
                | Command::SetPackBanner(_)
                | Command::SetDensityGraph { .. }
                | Command::SetDynamicBackground(_)
                | Command::PlayMusic { .. }
        )
    }

    #[inline(always)]
    const fn command_label(command: &Command) -> &'static str {
        match command {
            Command::ExitNow => "ExitNow",
            Command::SetBanner(_) => "SetBanner",
            Command::SetCdTitle(_) => "SetCdTitle",
            Command::SetPackBanner(_) => "SetPackBanner",
            Command::SetDensityGraph { .. } => "SetDensityGraph",
            Command::FetchOnlineGrade(_) => "FetchOnlineGrade",
            Command::PlayMusic { .. } => "PlayMusic",
            Command::StopMusic => "StopMusic",
            Command::SetDynamicBackground(_) => "SetDynamicBackground",
            Command::UpdateScrollSpeed { .. } => "UpdateScrollSpeed",
            Command::UpdateSessionMusicRate(_) => "UpdateSessionMusicRate",
            Command::UpdatePreferredDifficulty(_) => "UpdatePreferredDifficulty",
            Command::UpdateLastPlayed { .. } => "UpdateLastPlayed",
        }
    }

    fn execute_command(
        &mut self,
        command: Command,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        let label = Self::command_label(&command);
        let always_log_timing = Self::should_log_command_timing(&command);
        let started = Instant::now();
        match command {
            Command::ExitNow => {
                event_loop.exit();
            }
            Command::SetBanner(path_opt) => self.apply_banner(path_opt),
            Command::SetCdTitle(path_opt) => self.apply_cdtitle(path_opt),
            Command::SetPackBanner(path_opt) => self.apply_pack_banner(path_opt),
            Command::SetDensityGraph { slot, chart_opt } => {
                self.apply_density_graph(slot, chart_opt)
            }
            Command::FetchOnlineGrade(hash) => self.spawn_grade_fetch(hash),
            Command::PlayMusic {
                path,
                looped,
                volume,
            } => self.play_music_command(path, looped, volume),
            Command::StopMusic => self.stop_music_command(),
            Command::SetDynamicBackground(path_opt) => self.apply_dynamic_background(path_opt),
            Command::UpdateScrollSpeed { side, setting } => {
                profile::update_scroll_speed_for_side(side, setting);
            }
            Command::UpdateSessionMusicRate(rate) => {
                crate::game::profile::set_session_music_rate(rate);
            }
            Command::UpdatePreferredDifficulty(idx) => {
                self.state.session.preferred_difficulty_index = idx;
            }
            Command::UpdateLastPlayed {
                side,
                music_path,
                chart_hash,
                difficulty_index,
            } => {
                profile::update_last_played_for_side(
                    side,
                    music_path.as_deref(),
                    chart_hash.as_deref(),
                    difficulty_index,
                );
            }
        }
        let elapsed = started.elapsed();
        let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
        if elapsed_ms >= 100.0 {
            warn!(
                "Slow command: {} took {:.2}ms on screen {:?}",
                label, elapsed_ms, self.state.screens.current_screen
            );
        } else if elapsed_ms >= 16.7 {
            info!(
                "Frame-cost command: {} took {:.2}ms on screen {:?}",
                label, elapsed_ms, self.state.screens.current_screen
            );
        } else if always_log_timing {
            info!(
                "Command timing: {} took {:.2}ms on screen {:?}",
                label, elapsed_ms, self.state.screens.current_screen
            );
        }
        Ok(())
    }

    fn apply_banner(&mut self, path_opt: Option<PathBuf>) {
        if let Some(backend) = self.backend.as_mut() {
            if let Some(path) = path_opt {
                let key = self.asset_manager.set_dynamic_banner(backend, Some(path));
                match self.state.screens.current_screen {
                    CurrentScreen::SelectCourse => {
                        self.state.screens.select_course_state.current_banner_key = key;
                    }
                    _ => {
                        self.state.screens.select_music_state.current_banner_key = key;
                    }
                }
            } else {
                self.asset_manager.destroy_dynamic_banner(backend);
                let color_index = match self.state.screens.current_screen {
                    CurrentScreen::SelectCourse => {
                        self.state.screens.select_course_state.active_color_index
                    }
                    _ => self.state.screens.select_music_state.active_color_index,
                };
                let banner_num = color_index.rem_euclid(12) + 1;
                let key = format!("banner{banner_num}.png");
                match self.state.screens.current_screen {
                    CurrentScreen::SelectCourse => {
                        self.state.screens.select_course_state.current_banner_key = key;
                    }
                    _ => {
                        self.state.screens.select_music_state.current_banner_key = key;
                    }
                }
            }
        }
    }

    fn apply_cdtitle(&mut self, path_opt: Option<PathBuf>) {
        if let Some(backend) = self.backend.as_mut() {
            self.state.screens.select_music_state.current_cdtitle_key =
                self.asset_manager.set_dynamic_cdtitle(backend, path_opt);
        }
    }

    fn apply_pack_banner(&mut self, path_opt: Option<PathBuf>) {
        if let Some(backend) = self.backend.as_mut() {
            self.asset_manager
                .set_dynamic_pack_banner(backend, path_opt);
        }
    }

    fn apply_density_graph(
        &mut self,
        slot: DensityGraphSlot,
        chart_opt: Option<DensityGraphSource>,
    ) {
        let (graph_w, graph_h) = if space::is_wide() {
            (286.0_f32, 64.0_f32)
        } else {
            (276.0_f32, 64.0_f32)
        };
        let mesh = chart_opt.and_then(|chart| {
            let verts = crate::screens::components::density_graph::build_density_histogram_mesh(
                &chart.measure_nps_vec,
                chart.max_nps,
                &chart.timing,
                chart.first_second,
                chart.last_second,
                graph_w,
                graph_h,
                0.0,
                graph_w,
                None,
                1.0,
            );
            if verts.is_empty() {
                None
            } else {
                Some(Arc::from(verts.into_boxed_slice()))
            }
        });

        match slot {
            DensityGraphSlot::SelectMusicP1 => {
                self.state.screens.select_music_state.current_graph_mesh = mesh;
                self.state.screens.select_music_state.current_graph_key = "__white".to_string();
            }
            DensityGraphSlot::SelectMusicP2 => {
                self.state.screens.select_music_state.current_graph_mesh_p2 = mesh;
                self.state.screens.select_music_state.current_graph_key_p2 = "__white".to_string();
            }
        }
    }

    fn spawn_grade_fetch(&self, hash: String) {
        info!("Fetching online grade for chart hash: {hash}");
        let mut spawned = 0;
        for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
            if !profile::is_session_side_joined(side) {
                continue;
            }
            let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
                continue;
            };
            let profile = profile::get_for_side(side);
            if profile.groovestats_api_key.is_empty() || profile.groovestats_username.is_empty() {
                continue;
            }

            spawned += 1;
            let hash = hash.clone();
            std::thread::spawn(move || {
                if let Err(e) = scores::fetch_and_store_grade(profile_id, profile, hash) {
                    warn!("Failed to fetch online grade: {e}");
                }
            });
        }
        if spawned == 0 {
            warn!(
                "Skipping GrooveStats grade fetch: no joined local profile with GrooveStats configured"
            );
        }
    }

    fn play_music_command(&self, path: PathBuf, looped: bool, volume: f32) {
        crate::core::audio::play_music(path, crate::core::audio::Cut::default(), looped, volume);
    }

    fn stop_music_command(&self) {
        crate::core::audio::stop_music();
    }

    fn apply_dynamic_background(&mut self, path_opt: Option<PathBuf>) {
        if let Some(backend) = self.backend.as_mut() {
            let key = self.asset_manager.set_dynamic_background(backend, path_opt);
            if let Some(gs) = &mut self.state.screens.gameplay_state {
                gs.background_texture_key = key;
            }
        }
    }

    fn capture_pending_screenshot(&mut self, now: Instant) {
        if !self.state.shell.screenshot_pending {
            return;
        }
        self.state.shell.screenshot_pending = false;
        let request_side = self.state.shell.screenshot_request_side.take();
        let capture_result = {
            let Some(backend) = self.backend.as_mut() else {
                return;
            };
            backend.capture_frame()
        };

        match capture_result {
            Ok(mut image) => {
                // Screen captures should be opaque to avoid viewer-side alpha compositing.
                set_opaque_alpha(&mut image);
                match save_screenshot_image(&image) {
                    Ok(path) => {
                        self.state.shell.screenshot_flash_started_at = Some(now);

                        if self.state.screens.current_screen == CurrentScreen::Evaluation {
                            if let Err(e) = self.replace_screenshot_preview_texture(&image) {
                                warn!("Failed to create screenshot preview texture: {e}");
                                self.state.shell.screenshot_preview = None;
                            } else {
                                self.state.shell.screenshot_preview =
                                    Some(ScreenshotPreviewState {
                                        started_at: now,
                                        target: Self::screenshot_preview_target(request_side),
                                    });
                            }
                        }

                        crate::core::audio::play_sfx("assets/sounds/screenshot.ogg");
                        info!("Saved screenshot to {}", path.display());
                    }
                    Err(e) => warn!("Failed to save screenshot: {e}"),
                }
            }
            Err(e) => warn!(
                "Screenshot capture unavailable for renderer {}: {e}",
                self.backend_type
            ),
        }
    }

    #[inline(always)]
    fn screenshot_preview_target(side: Option<profile::PlayerSide>) -> ScreenshotPreviewTarget {
        if let Some(side) = side
            && profile::is_session_side_joined(side)
            && !profile::is_session_side_guest(side)
        {
            return ScreenshotPreviewTarget::Player(side);
        }
        ScreenshotPreviewTarget::Machine
    }

    fn replace_screenshot_preview_texture(
        &mut self,
        image: &image::RgbaImage,
    ) -> Result<(), Box<dyn Error>> {
        let Some(backend) = self.backend.as_mut() else {
            return Ok(());
        };

        if let Some(old) = self
            .asset_manager
            .textures
            .remove(SCREENSHOT_PREVIEW_TEXTURE_KEY)
        {
            let mut old_map = HashMap::with_capacity(1);
            old_map.insert(SCREENSHOT_PREVIEW_TEXTURE_KEY.to_string(), old);
            backend.dispose_textures(&mut old_map);
        }

        let texture = backend.create_texture(image, crate::core::gfx::SamplerDesc::default())?;
        self.asset_manager
            .textures
            .insert(SCREENSHOT_PREVIEW_TEXTURE_KEY.to_string(), texture);
        crate::assets::register_texture_dims(
            SCREENSHOT_PREVIEW_TEXTURE_KEY,
            image.width(),
            image.height(),
        );
        Ok(())
    }

    #[inline(always)]
    fn screenshot_preview_pose(&self, now: Instant) -> Option<(f32, f32, f32, f32)> {
        if self.state.screens.current_screen != CurrentScreen::Evaluation {
            return None;
        }
        let preview = self.state.shell.screenshot_preview?;
        let elapsed = now.duration_since(preview.started_at).as_secs_f32();
        if !elapsed.is_finite() || elapsed < 0.0 {
            return None;
        }

        let hold_seconds = SCREENSHOT_PREVIEW_HOLD_SECONDS
            + match preview.target {
                ScreenshotPreviewTarget::Machine => SCREENSHOT_PREVIEW_MACHINE_EXTRA_HOLD_SECONDS,
                ScreenshotPreviewTarget::Player(_) => 0.0,
            };
        let total_seconds = hold_seconds + SCREENSHOT_PREVIEW_TWEEN_SECONDS;
        if elapsed >= total_seconds {
            return None;
        }

        let screen_w = space::screen_width();
        let screen_h = space::screen_height();
        let start_x = screen_w * 0.5;
        let start_y = screen_h * 0.5;

        let (target_x, target_y) = match preview.target {
            ScreenshotPreviewTarget::Player(profile::PlayerSide::P1) => (20.0, screen_h + 10.0),
            ScreenshotPreviewTarget::Player(profile::PlayerSide::P2) => {
                (screen_w - 20.0, screen_h + 10.0)
            }
            ScreenshotPreviewTarget::Machine => (screen_w * 0.5, screen_h + 10.0),
        };

        let (x, y, scale) = if elapsed <= hold_seconds {
            (start_x, start_y, SCREENSHOT_PREVIEW_SCALE)
        } else {
            let t = ((elapsed - hold_seconds) / SCREENSHOT_PREVIEW_TWEEN_SECONDS).clamp(0.0, 1.0);
            let smooth = t * t * (3.0 - 2.0 * t);
            (
                start_x + (target_x - start_x) * smooth,
                start_y + (target_y - start_y) * smooth,
                SCREENSHOT_PREVIEW_SCALE * (1.0 - smooth),
            )
        };

        let blink_phase =
            elapsed * (std::f32::consts::TAU / SCREENSHOT_PREVIEW_GLOW_PERIOD_SECONDS);
        let glow_alpha = blink_phase.sin().mul_add(0.5, 0.5) * SCREENSHOT_PREVIEW_GLOW_ALPHA;
        Some((x, y, scale.max(0.0), glow_alpha.clamp(0.0, 1.0)))
    }

    fn append_screenshot_overlay_actors(&self, actors: &mut Vec<Actor>, now: Instant) {
        let flash_alpha = self.state.shell.screenshot_flash_alpha(now);
        if flash_alpha > 0.0 {
            actors.push(act!(quad:
                align(0.0, 0.0):
                xy(0.0, 0.0):
                zoomto(space::screen_width(), space::screen_height()):
                diffuse(1.0, 1.0, 1.0, flash_alpha):
                z(32000)
            ));
        }

        let Some((x, y, scale, glow_alpha)) = self.screenshot_preview_pose(now) else {
            return;
        };
        if scale <= 0.0 {
            return;
        }

        let screen_w = space::screen_width();
        let screen_h = space::screen_height();
        let shot_w = screen_w * scale;
        let shot_h = screen_h * scale;
        if shot_w <= 0.0 || shot_h <= 0.0 {
            return;
        }

        let border = SCREENSHOT_PREVIEW_BORDER_PX;
        let outer_w = shot_w + border * 2.0;
        let outer_h = shot_h + border * 2.0;
        let edge_alpha = (0.7 + glow_alpha).clamp(0.0, 1.0);
        let z = SCREENSHOT_PREVIEW_Z;

        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(x, y):
            setsize(outer_w, outer_h):
            diffuse(1.0, 1.0, 1.0, glow_alpha * 0.4):
            z(z)
        ));
        actors.push(act!(sprite(SCREENSHOT_PREVIEW_TEXTURE_KEY.to_string()):
            align(0.5, 0.5):
            xy(x, y):
            setsize(screen_w, screen_h):
            zoom(scale):
            z(z + 1)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(x, y - shot_h * 0.5 - border * 0.5):
            setsize(outer_w, border):
            diffuse(1.0, 1.0, 1.0, edge_alpha):
            z(z + 2)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(x, y + shot_h * 0.5 + border * 0.5):
            setsize(outer_w, border):
            diffuse(1.0, 1.0, 1.0, edge_alpha):
            z(z + 2)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(x - shot_w * 0.5 - border * 0.5, y):
            setsize(border, outer_h):
            diffuse(1.0, 1.0, 1.0, edge_alpha):
            z(z + 2)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(x + shot_w * 0.5 + border * 0.5, y):
            setsize(border, outer_h):
            diffuse(1.0, 1.0, 1.0, edge_alpha):
            z(z + 2)
        ));
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

    fn get_current_actors(&self) -> (Vec<Actor>, [f32; 4]) {
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
                if let Some(gs) = &self.state.screens.gameplay_state {
                    gameplay::get_actors(gs, &self.asset_manager)
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
            let overlay = crate::screens::components::stats_overlay::build(
                self.backend_type,
                self.state.shell.last_fps,
                self.state.shell.last_vpf,
            );
            actors.extend(overlay);
            if self.state.shell.overlay_mode.shows_stutter() {
                let now_seconds = Instant::now()
                    .duration_since(self.state.shell.start_time)
                    .as_secs_f32();
                let stutters = self.collect_visible_stutters(now_seconds);
                actors.extend(crate::screens::components::stats_overlay::build_stutter(
                    &stutters,
                ));
            }
        }

        // Gamepad connection overlay (always on top of screen, but below transitions)
        if let Some((msg, _)) = &self.state.shell.gamepad_overlay_state {
            let params = crate::screens::components::gamepad_overlay::Params { message: msg };
            actors.extend(crate::screens::components::gamepad_overlay::build(params));
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
                    let splash = crate::screens::components::menu_splash::build(
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

    fn get_out_transition_for_screen(&self, screen: CurrentScreen) -> (Vec<Actor>, f32) {
        match screen {
            CurrentScreen::Menu => {
                menu::out_transition(self.state.screens.menu_state.active_color_index)
            }
            CurrentScreen::Gameplay => gameplay::out_transition(),
            CurrentScreen::Options => options::out_transition(),
            CurrentScreen::Credits => credits::out_transition(),
            CurrentScreen::ManageLocalProfiles => manage_local_profiles::out_transition(),
            CurrentScreen::Mappings => mappings::out_transition(),
            CurrentScreen::PlayerOptions => player_options::out_transition(),
            CurrentScreen::SelectProfile => select_profile::out_transition(),
            CurrentScreen::SelectColor => select_color::out_transition(),
            CurrentScreen::SelectStyle => select_style::out_transition(),
            CurrentScreen::SelectPlayMode => select_mode::out_transition(),
            CurrentScreen::ProfileLoad => profile_load::out_transition(),
            CurrentScreen::SelectMusic => select_music::out_transition(),
            CurrentScreen::SelectCourse => select_course::out_transition(),
            CurrentScreen::Sandbox => sandbox::out_transition(),
            CurrentScreen::Init => init::out_transition(),
            CurrentScreen::Evaluation => evaluation::out_transition(),
            CurrentScreen::EvaluationSummary => evaluation_summary::out_transition(),
            CurrentScreen::Initials => initials::out_transition(),
            CurrentScreen::GameOver => gameover::out_transition(),
            CurrentScreen::Input => input_screen::out_transition(),
        }
    }

    fn get_in_transition_for_screen(&self, screen: CurrentScreen) -> (Vec<Actor>, f32) {
        match screen {
            CurrentScreen::Menu => menu::in_transition(),
            CurrentScreen::Gameplay => {
                gameplay::in_transition(self.state.screens.gameplay_state.as_ref())
            }
            CurrentScreen::Options => options::in_transition(),
            CurrentScreen::Credits => credits::in_transition(),
            CurrentScreen::ManageLocalProfiles => manage_local_profiles::in_transition(),
            CurrentScreen::Mappings => mappings::in_transition(),
            CurrentScreen::PlayerOptions => player_options::in_transition(),
            CurrentScreen::SelectProfile => select_profile::in_transition(),
            CurrentScreen::SelectColor => select_color::in_transition(),
            CurrentScreen::SelectStyle => select_style::in_transition(),
            CurrentScreen::SelectPlayMode => select_mode::in_transition(),
            CurrentScreen::ProfileLoad => profile_load::in_transition(),
            CurrentScreen::SelectMusic => select_music::in_transition(),
            CurrentScreen::SelectCourse => select_course::in_transition(),
            CurrentScreen::Sandbox => sandbox::in_transition(),
            CurrentScreen::Evaluation => evaluation::in_transition(),
            CurrentScreen::EvaluationSummary => evaluation_summary::in_transition(),
            CurrentScreen::Initials => initials::in_transition(),
            CurrentScreen::GameOver => gameover::in_transition(),
            CurrentScreen::Input => input_screen::in_transition(),
            CurrentScreen::Init => (vec![], 0.0),
        }
    }

    fn collect_visible_stutters(
        &self,
        now_seconds: f32,
    ) -> Vec<crate::screens::components::stats_overlay::StutterEvent> {
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
            out.push(crate::screens::components::stats_overlay::StutterEvent {
                timestamp_seconds: sample.at_seconds,
                frame_ms: sample.frame_seconds * 1000.0,
                frame_multiple,
                severity: sample.severity,
                age_seconds,
            });
        }
        out
    }

    #[inline(always)]
    fn update_stutter_samples(&mut self, frame_seconds: f32, total_elapsed: f32) {
        if !self.state.shell.overlay_mode.shows_stutter() {
            return;
        }
        let fps = self.state.shell.last_fps;
        if fps <= 0.0 {
            return;
        }
        let expected = 1.0 / fps;
        let thresholds = [expected * 2.0, expected * 4.0, 0.1];
        let mut severity: usize = 0;
        while severity < thresholds.len() && frame_seconds > thresholds[severity] {
            severity += 1;
        }
        if severity == 0 {
            return;
        }
        self.state.shell.push_stutter_sample(
            total_elapsed,
            frame_seconds,
            expected,
            severity as u8,
        );
    }

    #[inline(always)]
    fn update_fps_title(&mut self, window: &Window, now: Instant) {
        self.state.shell.frame_count += 1;
        let elapsed = now.duration_since(self.state.shell.last_title_update);
        if elapsed.as_secs_f32() >= 1.0 {
            let fps = self.state.shell.frame_count as f32 / elapsed.as_secs_f32();
            self.state.shell.last_fps = fps;
            self.state.shell.last_vpf = self.state.shell.current_frame_vpf;
            let screen_name = format!("{:?}", self.state.screens.current_screen);
            window.set_title(&format!(
                "DeadSync - {:?} | {} | {:.2} FPS",
                self.backend_type, screen_name, fps
            ));
            self.state.shell.frame_count = 0;
            self.state.shell.last_title_update = now;
        }
    }

    fn init_graphics(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        // Collect monitors and update options immediately so the initial menu state is correct.
        self.update_options_monitor_specs(event_loop);

        let mut window_attributes = Window::default_attributes()
            .with_title(format!("DeadSync - {:?}", self.backend_type))
            .with_resizable(true)
            .with_transparent(false)
            // Keep the window hidden until startup assets are ready so the first
            // visible frame starts Init animations at t=0.
            .with_visible(false);

        let window_width = self.state.shell.display_width;
        let window_height = self.state.shell.display_height;
        let (monitor_handle, monitor_count, monitor_idx) =
            display::resolve_monitor(event_loop, self.state.shell.display_monitor);
        self.state.shell.display_monitor = monitor_idx;
        let fullscreen_type = match self.state.shell.display_mode {
            DisplayMode::Fullscreen(ft) => ft,
            DisplayMode::Windowed => config::get().fullscreen_type,
        };
        options::sync_display_mode(
            &mut self.state.screens.options_state,
            self.state.shell.display_mode,
            fullscreen_type,
            self.state.shell.display_monitor,
            monitor_count,
        );

        match self.state.shell.display_mode {
            DisplayMode::Fullscreen(fullscreen_type) => {
                let fullscreen = display::fullscreen_mode(
                    fullscreen_type,
                    window_width,
                    window_height,
                    monitor_handle,
                    event_loop,
                );
                window_attributes = window_attributes.with_fullscreen(fullscreen);
            }
            DisplayMode::Windowed => {
                window_attributes = window_attributes
                    .with_inner_size(PhysicalSize::new(window_width, window_height));
                if let Some(pos) = self.state.shell.pending_window_position.take() {
                    window_attributes = window_attributes.with_position(pos);
                } else if let Some(pos) =
                    display::default_window_position(window_width, window_height, monitor_handle)
                {
                    window_attributes = window_attributes.with_position(pos);
                }
            }
        }

        let window = Arc::new(event_loop.create_window(window_attributes)?);
        // Re-assert the opaque hint so compositors do not apply alpha-based blending.
        window.set_transparent(false);
        let sz = window.inner_size();
        self.state.shell.metrics = crate::core::space::metrics_for_window(sz.width, sz.height);
        crate::core::space::set_current_metrics(self.state.shell.metrics);
        let mut backend = create_backend(
            self.backend_type,
            window.clone(),
            self.state.shell.vsync_enabled,
            self.gfx_debug_enabled,
        )?;

        if self.backend_type == BackendType::Software {
            let threads = match self.software_renderer_threads {
                0 => None,
                n => Some(n as usize),
            };
            backend.configure_software_threads(threads);
        }

        self.asset_manager.load_initial_assets(&mut backend)?;

        let now = Instant::now();
        self.state.shell.start_time = now;
        self.state.shell.last_frame_time = now;
        self.state.shell.last_title_update = now;
        self.state.shell.next_redraw_at = now;
        self.state.shell.frame_count = 0;
        self.state.shell.current_frame_vpf = 0;

        window.set_visible(true);
        window.request_redraw();

        self.window = Some(window);
        self.backend = Some(backend);
        info!("Starting event loop...");
        Ok(())
    }

    fn switch_renderer(
        &mut self,
        target: BackendType,
        desired_size: Option<(u32, u32)>,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        if target == self.backend_type {
            return Ok(());
        }

        let previous_backend = self.backend_type;
        let mut old_window_pos: Option<PhysicalPosition<i32>> = None;
        if let Some((w, h)) = desired_size {
            self.state.shell.display_width = w;
            self.state.shell.display_height = h;
        }
        if let Some(window) = &self.window {
            if desired_size.is_none() {
                let sz = window.inner_size();
                self.state.shell.display_width = sz.width;
                self.state.shell.display_height = sz.height;
            }
            if matches!(self.state.shell.display_mode, DisplayMode::Fullscreen(_)) {
                window.set_fullscreen(None);
            }
            if matches!(self.state.shell.display_mode, DisplayMode::Windowed)
                && let Ok(pos) = window.outer_position()
            {
                old_window_pos = Some(pos);
            }
            window.set_visible(false);
        }

        if let Some(mut backend) = self.backend.take() {
            self.asset_manager.destroy_dynamic_assets(&mut backend);
            backend.dispose_textures(&mut self.asset_manager.textures);
            backend.cleanup();
        }
        self.backend = None;
        self.window = None;
        self.state.shell.pending_window_position = old_window_pos;

        self.backend_type = target;
        self.state.shell.frame_count = 0;
        let now = Instant::now();
        self.state.shell.last_title_update = now;
        self.state.shell.last_frame_time = now;
        self.state.shell.next_redraw_at = now;

        match self.init_graphics(event_loop) {
            Ok(()) => {
                config::update_video_renderer(target);
                options::sync_video_renderer(&mut self.state.screens.options_state, target);
                crate::ui::runtime::clear_all();
                self.reset_dynamic_assets_after_renderer_switch();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                info!("Switched renderer to {target:?}");
                Ok(())
            }
            Err(e) => {
                error!("Failed to switch renderer to {target:?}: {e}");
                self.backend_type = previous_backend;
                if let Err(restoration_err) = self.init_graphics(event_loop) {
                    error!(
                        "Failed to restore previous renderer {previous_backend:?}: {restoration_err}"
                    );
                }
                options::sync_video_renderer(
                    &mut self.state.screens.options_state,
                    previous_backend,
                );
                let (_, monitor_count, monitor_idx) =
                    display::resolve_monitor(event_loop, self.state.shell.display_monitor);
                self.state.shell.display_monitor = monitor_idx;
                let fullscreen_type = match self.state.shell.display_mode {
                    DisplayMode::Fullscreen(ft) => ft,
                    DisplayMode::Windowed => config::get().fullscreen_type,
                };
                options::sync_display_mode(
                    &mut self.state.screens.options_state,
                    self.state.shell.display_mode,
                    fullscreen_type,
                    monitor_idx,
                    monitor_count,
                );
                self.state.shell.pending_window_position = None;
                config::update_video_renderer(previous_backend);
                Err(e)
            }
        }
    }

    fn apply_display_mode(
        &mut self,
        mode: DisplayMode,
        monitor_override: Option<usize>,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        let (monitor_handle, monitor_count, resolved_monitor) = display::resolve_monitor(
            event_loop,
            monitor_override.unwrap_or(self.state.shell.display_monitor),
        );
        self.state.shell.display_monitor = resolved_monitor;
        let previous_mode = self.state.shell.display_mode;

        if let Some(window) = &self.window {
            if matches!(previous_mode, DisplayMode::Windowed) {
                let sz = window.inner_size();
                self.state.shell.display_width = sz.width;
                self.state.shell.display_height = sz.height;
                if let Ok(pos) = window.outer_position() {
                    self.state.shell.pending_window_position = Some(pos);
                }
            }

            match mode {
                DisplayMode::Windowed => {
                    window.set_fullscreen(None);
                    let size = PhysicalSize::new(
                        self.state.shell.display_width,
                        self.state.shell.display_height,
                    );
                    let _ = window.request_inner_size(size);
                    if let Some(pos) = self.state.shell.pending_window_position.take() {
                        window.set_outer_position(pos);
                    } else if let Some(pos) = display::default_window_position(
                        self.state.shell.display_width,
                        self.state.shell.display_height,
                        monitor_handle,
                    ) {
                        window.set_outer_position(pos);
                    }
                }
                DisplayMode::Fullscreen(fullscreen_type) => {
                    let fullscreen = display::fullscreen_mode(
                        fullscreen_type,
                        self.state.shell.display_width,
                        self.state.shell.display_height,
                        monitor_handle,
                        event_loop,
                    );
                    window.set_fullscreen(fullscreen);
                }
            }

            let sz = window.inner_size();
            self.state.shell.metrics = space::metrics_for_window(sz.width, sz.height);
            space::set_current_metrics(self.state.shell.metrics);
            if let Some(backend) = &mut self.backend {
                backend.resize(sz.width, sz.height);
            }
        }

        self.state.shell.display_mode = mode;

        let fullscreen_type = match mode {
            DisplayMode::Fullscreen(ft) => ft,
            DisplayMode::Windowed => match previous_mode {
                DisplayMode::Fullscreen(ft) => ft,
                DisplayMode::Windowed => config::get().fullscreen_type,
            },
        };
        config::update_display_mode(mode);
        config::update_display_monitor(self.state.shell.display_monitor);
        options::sync_display_mode(
            &mut self.state.screens.options_state,
            mode,
            fullscreen_type,
            self.state.shell.display_monitor,
            monitor_count,
        );
        Ok(())
    }

    fn apply_resolution(
        &mut self,
        width: u32,
        height: u32,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        self.state.shell.display_width = width;
        self.state.shell.display_height = height;
        let (monitor_handle, _, resolved_monitor) =
            display::resolve_monitor(event_loop, self.state.shell.display_monitor);
        self.state.shell.display_monitor = resolved_monitor;

        if let Some(window) = &self.window {
            match self.state.shell.display_mode {
                DisplayMode::Windowed => {
                    let size = PhysicalSize::new(width, height);
                    let _ = window.request_inner_size(size);
                }
                DisplayMode::Fullscreen(fullscreen_type) => {
                    let fullscreen = display::fullscreen_mode(
                        fullscreen_type,
                        width,
                        height,
                        monitor_handle,
                        event_loop,
                    );
                    window.set_fullscreen(fullscreen);
                }
            }

            let sz = window.inner_size();
            self.state.shell.metrics = space::metrics_for_window(sz.width, sz.height);
            space::set_current_metrics(self.state.shell.metrics);
            if let Some(backend) = &mut self.backend {
                backend.resize(sz.width, sz.height);
            }
        }

        Ok(())
    }

    fn reset_dynamic_assets_after_renderer_switch(&mut self) {
        self.apply_banner(None);
        self.apply_cdtitle(None);
        self.apply_density_graph(DensityGraphSlot::SelectMusicP1, None);
        self.apply_density_graph(DensityGraphSlot::SelectMusicP2, None);
        self.apply_dynamic_background(None);

        select_music::trigger_immediate_refresh(&mut self.state.screens.select_music_state);
        self.state.screens.select_music_state.current_graph_key = "__white".to_string();
        self.state.screens.select_music_state.current_graph_key_p2 = "__white".to_string();
    }

    /* -------------------- keyboard: map -> route -------------------- */

    #[inline(always)]
    fn handle_key_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        key_event: winit::event::KeyEvent,
    ) {
        // Track modifier key state for gameplay raw sync combos (F11/F12).
        if let winit::keyboard::PhysicalKey::Code(code) = key_event.physical_key {
            use winit::event::ElementState;
            use winit::keyboard::KeyCode;
            match code {
                KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                    self.state.shell.shift_held = key_event.state == ElementState::Pressed;
                }
                KeyCode::ControlLeft | KeyCode::ControlRight => {
                    self.state.shell.ctrl_held = key_event.state == ElementState::Pressed;
                }
                _ => {}
            }
        }

        if self.state.screens.current_screen == CurrentScreen::Sandbox {
            let action = crate::screens::sandbox::handle_raw_key_event(
                &mut self.state.screens.sandbox_state,
                &key_event,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle Sandbox raw key action: {e}");
                }
                return;
            }
        } else if self.state.screens.current_screen == CurrentScreen::Menu {
            let action = crate::screens::menu::handle_raw_key_event(
                &mut self.state.screens.menu_state,
                &key_event,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle Menu raw key action: {e}");
                }
                return;
            }
        } else if self.state.screens.current_screen == CurrentScreen::Mappings {
            let action = crate::screens::mappings::handle_raw_key_event(
                &mut self.state.screens.mappings_state,
                &key_event,
            );
            if !matches!(action, ScreenAction::None)
                && let Err(e) = self.handle_action(action, event_loop)
            {
                log::error!("Failed to handle Mappings raw key action: {e}");
            }
            // On the Mappings screen, arrows/Enter/Escape are handled entirely
            // via raw keycodes; do not route through the virtual keymap.
            return;
        } else if self.state.screens.current_screen == CurrentScreen::ManageLocalProfiles {
            let action = crate::screens::manage_local_profiles::handle_raw_key_event(
                &mut self.state.screens.manage_local_profiles_state,
                &key_event,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle ManageLocalProfiles raw key action: {e}");
                }
                return;
            }
        } else if self.state.screens.current_screen == CurrentScreen::Input {
            let action = crate::screens::input::handle_raw_key_event(
                &mut self.state.screens.input_state,
                &key_event,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle Input raw key action: {e}");
                }
                return;
            }
        } else if self.state.screens.current_screen == CurrentScreen::SelectMusic {
            // Route screen-specific raw key handling (e.g., F7 fetch) to the screen
            let action = crate::screens::select_music::handle_raw_key_event(
                &mut self.state.screens.select_music_state,
                &key_event,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle SelectMusic raw key action: {e}");
                }
                return;
            }
        } else if self.state.screens.current_screen == CurrentScreen::Evaluation {
            if key_event.state == winit::event::ElementState::Pressed
                && !self.state.session.course_eval_pages.is_empty()
                && let winit::keyboard::PhysicalKey::Code(code) = key_event.physical_key
            {
                match code {
                    winit::keyboard::KeyCode::KeyN => {
                        self.step_course_eval_page(1);
                        return;
                    }
                    winit::keyboard::KeyCode::KeyP => {
                        self.step_course_eval_page(-1);
                        return;
                    }
                    _ => {}
                }
            }
        } else if self.state.screens.current_screen == CurrentScreen::Gameplay {
            if self.state.gameplay_offset_save_prompt.is_none() {
                if key_event.state == winit::event::ElementState::Pressed
                    && !key_event.repeat
                    && self.state.shell.ctrl_held
                    && key_event.physical_key
                        == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyR)
                    && config::get().keyboard_features
                    && self.state.session.course_run.is_none()
                {
                    if self.prepare_player_options_for_gameplay_restart() {
                        let restart_count =
                            self.state.session.gameplay_restart_count.saturating_add(1);
                        if let Err(e) = self.handle_action(
                            ScreenAction::Navigate(CurrentScreen::Gameplay),
                            event_loop,
                        ) {
                            log::error!("Failed to restart Gameplay with Ctrl+R: {e}");
                        } else {
                            self.state.session.gameplay_restart_count = restart_count;
                        }
                    } else {
                        log::warn!("Ignored Ctrl+R restart: no active gameplay state.");
                    }
                    return;
                }
                if let Some(gs) = &mut self.state.screens.gameplay_state {
                    let action = crate::game::gameplay::handle_raw_key_event(
                        gs,
                        &key_event,
                        self.state.shell.shift_held,
                    );
                    if !matches!(action, ScreenAction::None) {
                        if let Err(e) = self.handle_action(action, event_loop) {
                            log::error!("Failed to handle Gameplay raw key action: {e}");
                        }
                        return;
                    }
                }
            }
        }
        let is_transitioning = !matches!(self.state.shell.transition, TransitionState::Idle);
        let _event_timestamp = Instant::now();

        if key_event.state == winit::event::ElementState::Pressed
            && key_event.physical_key
                == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::F3)
        {
            let mode = self.state.shell.cycle_overlay_mode();
            log::info!("Overlay {}", self.state.shell.overlay_mode.label());
            config::update_show_stats_mode(mode);
            options::sync_show_stats_mode(&mut self.state.screens.options_state, mode);
        }
        // Screen-specific Escape handling resides in per-screen raw handlers now

        if is_transitioning {
            return;
        }

        for ev in input::map_key_event(&key_event) {
            if let Err(e) = self.route_input_event(event_loop, ev) {
                log::error!("Failed to handle input: {e}");
                event_loop.exit();
                return;
            }
        }
    }

    /* -------------------- pad event routing -------------------- */

    #[inline(always)]
    fn handle_pad_event(&mut self, event_loop: &ActiveEventLoop, ev: PadEvent) {
        let is_transitioning = !matches!(self.state.shell.transition, TransitionState::Idle);
        if is_transitioning || self.state.screens.current_screen == CurrentScreen::Init {
            return;
        }
        for iev in input::map_pad_event(&ev) {
            if let Err(e) = self.route_input_event(event_loop, iev) {
                error!("Failed to handle pad input: {e}");
                event_loop.exit();
                return;
            }
        }
    }

    // legacy virtual-action dispatcher removed; screens own their input

    #[cfg(any())]
    #[inline(always)]
    fn poll_gamepad_and_dispatch(&mut self, _event_loop: &ActiveEventLoop) {}

    fn on_fade_complete(&mut self, target: CurrentScreen, event_loop: &ActiveEventLoop) {
        if self.state.shell.pending_exit {
            info!("Fade-out complete; exiting application.");
            event_loop.exit();
            return;
        }

        let prev = self.state.screens.current_screen;
        self.state.screens.current_screen = target;
        if target != CurrentScreen::Gameplay {
            self.state.gameplay_offset_save_prompt = None;
        }
        if target == CurrentScreen::SelectColor {
            select_color::on_enter(&mut self.state.screens.select_color_state);
        }

        let mut commands: Vec<Command> = Vec::new();

        commands.extend(self.handle_audio_and_profile_on_fade(prev, target));
        self.handle_screen_state_on_fade(prev, target);
        commands.extend(self.handle_screen_entry_on_fade(prev, target));

        // Ensure monitor specs are fresh when entering the options screen
        if target == CurrentScreen::Options {
            self.update_options_monitor_specs(event_loop);
        }

        let instant_options_credits_swap = matches!(
            (prev, target),
            (CurrentScreen::Options, CurrentScreen::Credits)
                | (CurrentScreen::Credits, CurrentScreen::Options)
        );
        if instant_options_credits_swap {
            self.state.shell.transition = TransitionState::Idle;
        } else {
            let (_, in_duration) = self.get_in_transition_for_screen(target);
            self.state.shell.transition = TransitionState::FadingIn {
                elapsed: 0.0,
                duration: in_duration,
            };
        }
        crate::ui::runtime::clear_all();
        let _ = self.run_commands(commands, event_loop);
    }

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
                    path: PathBuf::from("assets/music/in_two (loop).ogg"),
                    looped: true,
                    volume: 1.0,
                });
            }
        } else if target_course_music {
            if !prev_course_music {
                commands.push(Command::PlayMusic {
                    path: PathBuf::from("assets/music/select_course (loop).ogg"),
                    looped: true,
                    volume: 1.0,
                });
            }
        } else if target_credits_music {
            if !prev_credits_music {
                commands.push(Command::PlayMusic {
                    path: PathBuf::from("assets/music/credits.ogg"),
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

        if prev == CurrentScreen::Gameplay && target != CurrentScreen::Gameplay {
            if !target_menu_music && !target_course_music && !target_credits_music {
                commands.push(Command::StopMusic);
            }
            if let Some(backend) = self.backend.as_mut() {
                self.asset_manager.set_dynamic_background(backend, None);
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
                        let setting = match speed_mod.mod_type.as_str() {
                            "C" => Some(ScrollSpeedSetting::CMod(speed_mod.value)),
                            "X" => Some(ScrollSpeedSetting::XMod(speed_mod.value)),
                            "M" => Some(ScrollSpeedSetting::MMod(speed_mod.value)),
                            _ => None,
                        };

                        if let Some(setting) = setting {
                            commands.push(Command::UpdateScrollSpeed { side, setting });
                            info!("Saved scroll speed ({side:?}): {setting}");
                        } else {
                            warn!(
                                "Unsupported speed mod '{}' not saved to profile.",
                                speed_mod.mod_type
                            );
                        }
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
                info!("Session music rate set to {:.2}x", po_state.music_rate);

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
                info!(
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
                gs.player_color = color::simply_love_rgba(idx);
            }
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
            let current_color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.options_state = options::init();
            self.state.screens.options_state.active_color_index = current_color_index;
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
            let current_color_index = self.state.screens.select_play_mode_state.active_color_index;
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
                self.state.screens.player_options_state = Some(player_options::init(
                    song_arc,
                    chart_steps_index,
                    preferred_difficulty_index,
                    color_index,
                    CurrentScreen::SelectMusic,
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
        } else if target == CurrentScreen::Gameplay
            && (prev == CurrentScreen::SelectMusic || prev == CurrentScreen::SelectCourse)
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
                        _ => panic!("Cannot start gameplay on a pack header"),
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

    fn handle_screen_entry_on_fade(
        &mut self,
        prev: CurrentScreen,
        target: CurrentScreen,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        if target == CurrentScreen::Gameplay {
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
            if prev == CurrentScreen::Gameplay && self.state.session.course_run.is_some() {
                if let Some(gameplay_results) = self.state.screens.gameplay_state.take() {
                    self.update_combo_carry_from_gameplay(&gameplay_results);
                    course_display_carry = Some(
                        crate::game::gameplay::course_display_carry_from_state(&gameplay_results),
                    );
                    let color_idx = gameplay_results.active_color_index;
                    let mut eval_state = evaluation::init(Some(gameplay_results));
                    eval_state.active_color_index = color_idx;
                    let _ = self.append_stage_results_from_eval(&eval_state);
                }
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
                        event_music_time: e.event_music_time,
                    })
                    .collect::<Vec<_>>()
            });
            let replay_offsets = replay_pending.as_ref().map(|payload| {
                crate::game::gameplay::ReplayOffsetSnapshot {
                    beat0_time_seconds: payload.replay_beat0_time_seconds,
                }
            });
            let replay_status_text = replay_pending.as_ref().map(|payload| {
                format!("Autoplay - {} {:.2}%", payload.name, payload.score / 100.0)
            });
            if let Some(po_state) = self.state.screens.player_options_state.take() {
                let song_arc = po_state.song;
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

                let (charts, last_played_chart_ref, last_played_idx) = match play_style {
                    profile::PlayStyle::Versus => {
                        let chart_ref_p1 = resolve_chart(0);
                        let chart_ref_p2 = resolve_chart(1);
                        (
                            [
                                Arc::new(chart_ref_p1.clone()),
                                Arc::new(chart_ref_p2.clone()),
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
                        ([chart.clone(), chart], chart_ref, idx)
                    }
                };

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
                            music_path: song_arc.music_path.clone(),
                            chart_hash: Some(charts[0].short_hash.clone()),
                            difficulty_index: po_state.chart_difficulty_index[0],
                        });
                        commands.push(Command::UpdateLastPlayed {
                            side: profile::PlayerSide::P2,
                            music_path: song_arc.music_path.clone(),
                            chart_hash: Some(charts[1].short_hash.clone()),
                            difficulty_index: po_state.chart_difficulty_index[1],
                        });
                    }
                    profile::PlayStyle::Single | profile::PlayStyle::Double => {
                        commands.push(Command::UpdateLastPlayed {
                            side: player_side,
                            music_path: song_arc.music_path.clone(),
                            chart_hash: Some(last_played_chart_ref.short_hash.clone()),
                            difficulty_index: po_state.chart_difficulty_index[last_played_idx],
                        });
                    }
                }

                let to_scroll_speed = |m: &player_options::SpeedMod| match m.mod_type.as_str() {
                    "X" => crate::game::scroll::ScrollSpeedSetting::XMod(m.value),
                    "C" => crate::game::scroll::ScrollSpeedSetting::CMod(m.value),
                    "M" => crate::game::scroll::ScrollSpeedSetting::MMod(m.value),
                    _ => crate::game::scroll::ScrollSpeedSetting::default(),
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
                let gs = gameplay::init(
                    song_arc,
                    charts,
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

                if let Some(backend) = self.backend.as_mut() {
                    prewarm_gameplay_assets(&mut self.asset_manager, backend, &gs);
                    if let Some(path) = gs.song.banner_path.as_ref() {
                        self.asset_manager.ensure_texture_from_path(backend, path);
                    }
                }
                commands.push(Command::SetPackBanner(gs.pack_banner_path.clone()));
                commands.push(Command::SetDynamicBackground(
                    gs.song.background_path.clone(),
                ));
                self.state.screens.gameplay_state = Some(gs);
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
                self.asset_manager.ensure_texture_from_path(backend, path);
            }
            let color_idx = gameplay_results.as_ref().map_or(
                self.state.screens.evaluation_state.active_color_index,
                |gs| gs.active_color_index,
            );
            self.state.screens.evaluation_state = evaluation::init(gameplay_results);
            self.state.screens.evaluation_state.active_color_index = color_idx;
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
            self.state.screens.evaluation_summary_state = evaluation_summary::init();
            self.state
                .screens
                .evaluation_summary_state
                .active_color_index = color_idx;

            let display_stages = self.post_select_display_stages().into_owned();
            if let Some(backend) = self.backend.as_mut() {
                for stage in display_stages.iter() {
                    if let Some(path) = stage.song.banner_path.as_ref() {
                        self.asset_manager.ensure_texture_from_path(backend, path);
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
                        self.asset_manager.ensure_texture_from_path(backend, path);
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
                info!("Session timer started.");
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
                CurrentScreen::Gameplay | CurrentScreen::Evaluation => {
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

                    let max_diff_index = color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1);
                    let p2_pref = profile::get_for_side(profile::PlayerSide::P2)
                        .last_difficulty_index
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
                        timing: c.timing.clone(),
                        first_second: 0.0_f32.min(c.timing.get_time_for_beat(0.0)),
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
                            timing: c.timing.clone(),
                            first_second: 0.0_f32.min(c.timing.get_time_for_beat(0.0)),
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
                info!("Session timer started.");
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
                        info!(
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
                        info!(
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
                }
                self.handle_pad_event(event_loop, ev);
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
            crate::core::network::init();
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
                if new_size.width > 0 && new_size.height > 0 {
                    self.state.shell.metrics =
                        space::metrics_for_window(new_size.width, new_size.height);
                    space::set_current_metrics(self.state.shell.metrics);
                    if let Some(backend) = &mut self.backend {
                        backend.resize(new_size.width, new_size.height);
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                self.handle_key_event(event_loop, key_event);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let delta_time = now
                    .duration_since(self.state.shell.last_frame_time)
                    .as_secs_f32();
                self.state.shell.last_frame_time = now;
                let total_elapsed = now
                    .duration_since(self.state.shell.start_time)
                    .as_secs_f32();
                crate::ui::runtime::tick(delta_time);

                // --- Manage gamepad overlay lifetime ---
                self.state.shell.update_gamepad_overlay(now);
                self.update_stutter_samples(delta_time, total_elapsed);

                let mut finished_fading_out_to: Option<CurrentScreen> = None;
                match &mut self.state.shell.transition {
                    TransitionState::FadingOut {
                        elapsed,
                        duration,
                        target,
                    } => {
                        *elapsed += delta_time;
                        if *elapsed >= *duration {
                            finished_fading_out_to = Some(*target);
                        }
                    }
                    TransitionState::ActorsFadeOut {
                        elapsed,
                        duration,
                        target,
                    } => {
                        *elapsed += delta_time;
                        if *elapsed >= *duration {
                            let target_screen = *target;
                            let prev = self.state.screens.current_screen;
                            self.state.screens.current_screen = target_screen;
                            if target_screen == CurrentScreen::SelectColor {
                                select_color::on_enter(&mut self.state.screens.select_color_state);
                            }

                            // SelectProfile/SelectColor/SelectStyle share the looping menu BGM.
                            // Keep SelectMusic preview playing when moving to/from PlayerOptions.
                            let menu_music_enabled = config::get().menu_music;
                            let target_menu_music = menu_music_enabled
                                && matches!(
                                    target_screen,
                                    CurrentScreen::SelectColor | CurrentScreen::SelectStyle
                                );
                            let prev_menu_music = menu_music_enabled
                                && matches!(
                                    prev,
                                    CurrentScreen::SelectColor | CurrentScreen::SelectStyle
                                );
                            let keep_preview = (prev == CurrentScreen::SelectMusic
                                && target_screen == CurrentScreen::PlayerOptions)
                                || (prev == CurrentScreen::PlayerOptions
                                    && target_screen == CurrentScreen::SelectMusic);

                            if target_menu_music {
                                if !prev_menu_music {
                                    crate::core::audio::play_music(
                                        std::path::PathBuf::from("assets/music/in_two (loop).ogg"),
                                        crate::core::audio::Cut::default(),
                                        true,
                                        1.0,
                                    );
                                }
                            } else if prev_menu_music {
                                crate::core::audio::stop_music();
                            } else if !keep_preview {
                                crate::core::audio::stop_music();
                            }

                            if target_screen == CurrentScreen::Menu {
                                let current_color_index =
                                    self.state.screens.menu_state.active_color_index;
                                self.state.screens.menu_state = menu::init();
                                self.state.screens.menu_state.active_color_index =
                                    current_color_index;
                            } else if target_screen == CurrentScreen::Options {
                                let current_color_index =
                                    self.state.screens.options_state.active_color_index;
                                self.state.screens.options_state = options::init();
                                self.state.screens.options_state.active_color_index =
                                    current_color_index;
                            } else if target_screen == CurrentScreen::ManageLocalProfiles {
                                let color_index =
                                    self.state.screens.options_state.active_color_index;
                                self.state.screens.manage_local_profiles_state =
                                    manage_local_profiles::init();
                                self.state
                                    .screens
                                    .manage_local_profiles_state
                                    .active_color_index = color_index;
                            } else if target_screen == CurrentScreen::SelectProfile {
                                let current_color_index =
                                    self.state.screens.select_profile_state.active_color_index;
                                self.state.screens.select_profile_state = select_profile::init();
                                self.state.screens.select_profile_state.active_color_index =
                                    current_color_index;
                                if prev == CurrentScreen::Menu {
                                    let p2 = self.state.screens.menu_state.started_by_p2;
                                    select_profile::set_joined(
                                        &mut self.state.screens.select_profile_state,
                                        !p2,
                                        p2,
                                    );
                                }
                            } else if target_screen == CurrentScreen::SelectStyle {
                                let current_color_index =
                                    self.state.screens.select_style_state.active_color_index;
                                self.state.screens.select_style_state = select_style::init();
                                self.state.screens.select_style_state.active_color_index =
                                    current_color_index;
                                let p1_joined =
                                    profile::is_session_side_joined(profile::PlayerSide::P1);
                                let p2_joined =
                                    profile::is_session_side_joined(profile::PlayerSide::P2);
                                self.state.screens.select_style_state.selected_index =
                                    if p1_joined && p2_joined {
                                        1 // "2 Players"
                                    } else {
                                        0 // "1 Player"
                                    };
                            } else if target_screen == CurrentScreen::Mappings {
                                let color_index =
                                    self.state.screens.options_state.active_color_index;
                                self.state.screens.mappings_state = mappings::init();
                                self.state.screens.mappings_state.active_color_index = color_index;
                            }

                            if prev == CurrentScreen::SelectColor {
                                let idx = self.state.screens.select_color_state.active_color_index;
                                self.state.screens.menu_state.active_color_index = idx;
                                self.state.screens.select_profile_state.active_color_index = idx;
                                self.state.screens.select_style_state.active_color_index = idx;
                                self.state.screens.profile_load_state.active_color_index = idx;
                                self.state.screens.select_music_state.active_color_index = idx;
                                self.state.screens.select_course_state.active_color_index = idx;
                                self.state.screens.credits_state.active_color_index = idx;
                                if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
                                    gs.active_color_index = idx;
                                    gs.player_color = color::simply_love_rgba(idx);
                                }
                                self.state.screens.options_state.active_color_index = idx;
                                self.state
                                    .screens
                                    .manage_local_profiles_state
                                    .active_color_index = idx;
                            }

                            if target_screen == CurrentScreen::Options {
                                self.update_options_monitor_specs(event_loop);
                            }

                            self.state.shell.transition =
                                if Self::is_actor_fade_screen(target_screen) {
                                    TransitionState::ActorsFadeIn { elapsed: 0.0 }
                                } else {
                                    TransitionState::Idle
                                };
                            crate::ui::runtime::clear_all();
                        }
                    }
                    TransitionState::FadingIn { elapsed, duration } => {
                        *elapsed += delta_time;
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
                        *elapsed += delta_time;
                        if *elapsed >= MENU_ACTORS_FADE_DURATION {
                            self.state.shell.transition = TransitionState::Idle;
                        }
                    }
                    TransitionState::Idle => {
                        let gameplay_prompt_active = self.state.screens.current_screen
                            == CurrentScreen::Gameplay
                            && self.state.gameplay_offset_save_prompt.is_some();
                        if !gameplay_prompt_active {
                            if let Some(action) = self.state.screens.step_idle(
                                delta_time,
                                now,
                                &self.state.session,
                                &self.asset_manager,
                            ) && !matches!(action, ScreenAction::None)
                            {
                                let _ = self.handle_action(action, event_loop);
                            }
                        }
                    }
                }

                if let Some(target) = finished_fading_out_to {
                    self.on_fade_complete(target, event_loop);
                }

                if self.window.as_ref().map(|w| w.id()) != Some(window_id) {
                    return;
                }

                let (actors, clear_color) = self.get_current_actors();
                self.update_fps_title(&window, now);
                let fonts = self.asset_manager.fonts();
                let screen = crate::ui::compose::build_screen(
                    &actors,
                    clear_color,
                    &self.state.shell.metrics,
                    fonts,
                    total_elapsed,
                );

                if let Some(backend) = &mut self.backend {
                    if self.state.shell.screenshot_pending {
                        backend.request_screenshot();
                    }
                    match backend.draw(&screen, &self.asset_manager.textures) {
                        Ok(vpf) => {
                            self.state.shell.current_frame_vpf = vpf;
                            self.capture_pending_screenshot(now);
                        }
                        Err(e) => {
                            error!("Failed to draw frame: {e}");
                            event_loop.exit();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = &self.window else {
            return;
        };
        if let Some(interval) = self.state.shell.frame_interval {
            let now = Instant::now();
            if now >= self.state.shell.next_redraw_at {
                window.request_redraw();
                self.state.shell.next_redraw_at = now.checked_add(interval).unwrap_or(now);
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.state.shell.next_redraw_at));
            return;
        }
        event_loop.set_control_flow(ControlFlow::Poll);
        window.request_redraw();
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        config::flush_pending_saves();
        if let Some(backend) = &mut self.backend {
            self.asset_manager.destroy_dynamic_assets(backend);
            backend.dispose_textures(&mut self.asset_manager.textures);
            backend.cleanup();
        }
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = config::get();
    let backend_type = config.video_renderer;
    let win_pad_backend = config.windows_gamepad_backend;
    let show_stats_mode = config.show_stats_mode.min(2);
    let color_index = config.simply_love_color;
    let profile_data = profile::get();

    song_loading::scan_and_load_songs("songs");
    song_loading::scan_and_load_courses("courses", "songs");
    crate::assets::prewarm_banner_cache(&collect_banner_cache_paths());
    std::thread::spawn(|| {
        if std::panic::catch_unwind(noteskin::prewarm_itg_preview_cache).is_err() {
            warn!("noteskin prewarm thread panicked; first-use preview hitches may occur");
        }
    });
    let event_loop: EventLoop<UserEvent> = EventLoop::<UserEvent>::with_user_event().build()?;

    // Spawn background thread to pump pad input and emit user events; decoupled from frame rate.
    let proxy: EventLoopProxy<UserEvent> = event_loop.create_proxy();
    std::thread::spawn(move || {
        let proxy_pad = proxy.clone();
        input::run_pad_backend(
            win_pad_backend,
            move |pe| {
                let _ = proxy_pad.send_event(UserEvent::Pad(pe));
            },
            move |se| {
                let _ = proxy.send_event(UserEvent::GamepadSystem(se));
            },
        );
    });

    let mut app = App::new(
        backend_type,
        show_stats_mode,
        color_index,
        config,
        profile_data,
    );
    event_loop.run_app(&mut app)?;
    Ok(())
}

fn collect_banner_cache_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    {
        let song_cache = crate::game::song::get_song_cache();
        for pack in song_cache.iter() {
            if let Some(path) = pack.banner_path.as_ref() {
                out.push(path.clone());
            }
            for song in &pack.songs {
                if let Some(path) = song.banner_path.as_ref() {
                    out.push(path.clone());
                }
            }
        }
    }
    {
        let course_cache = crate::game::course::get_course_cache();
        for (course_path, course) in course_cache.iter() {
            if let Some(path) =
                rssp::course::resolve_course_banner_path(course_path, &course.banner)
            {
                out.push(path);
            }
        }
    }
    out
}
