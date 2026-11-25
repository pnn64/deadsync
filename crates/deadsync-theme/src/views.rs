//! Plain data prepared for concrete theme screens.

use crate::{DisplayModeChoice, FullscreenChoice, PresentPolicyChoice, RendererChoice};
use deadsync_chart::course::CourseSessionPlan;
use deadsync_chart::{ChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
pub use deadsync_rules::combo::ComboRun;
use deadsync_rules::judgment::{self, JudgeGrade};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::{
    ArrowTimingStats, HistogramMs, ScatterPoint, TimingStats, WindowCounts,
};
pub use deadsync_score::Grade;
pub use deadsync_score::stage_stats::StageLifeTimeline;
use deadsync_score::{
    CachedPlayerLeaderboardData, ColumnJudgmentList, GrooveStatsEvalState, ItlEvalState,
    LeaderboardEntry,
};
use std::path::PathBuf;
use std::sync::Arc;

pub use deadsync_score::leaderboard::{
    CabinetCourseRecord as AttractCourseRecord, CabinetRecords as AttractRecords,
    CabinetSongRecord as AttractRecord,
};

/// Which entry into the shared play flow the selector presents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaySelectionKind {
    Style { pump: bool },
    Mode,
}

/// A visible choice supplied by the current style or mode controller.
pub struct PlaySelectionChoice<'a> {
    pub label: &'a Arc<str>,
    pub description: Option<&'a Arc<str>>,
    pub enabled: bool,
}

/// The visible choices and selection for the current play flow screen.
pub struct PlaySelectionView<'a> {
    pub session_style: crate::select_music::SessionStyleView,
    pub kind: PlaySelectionKind,
    pub title: &'a Arc<str>,
    pub choices: &'a [PlaySelectionChoice<'a>],
    pub selected: usize,
    pub player_names: [Option<&'a str>; MAX_PLAYERS],
    pub join_prompt: &'a Arc<str>,
    pub exit_progress: f32,
}

/// Shell-prepared audio clock values used by theme preview presentation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioPlaybackView {
    pub music_position_seconds: f64,
}

/// A normalized local score from the selected chart's score snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScoreboxLocalView {
    pub score_10000: f64,
    pub failed: bool,
}

impl ScoreboxLocalView {
    pub fn percent(self) -> f64 {
        deadsync_score::leaderboard::scorebox_score_percent(self.score_10000)
    }
}

/// The cabinet record and its holder for the selected chart.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScoreboxMachineView {
    pub name: String,
    pub score_10000: f64,
    pub failed: bool,
}

impl ScoreboxMachineView {
    pub fn percent(&self) -> f64 {
        deadsync_score::leaderboard::scorebox_score_percent(self.score_10000)
    }
}

/// Local ITG records ready to present for a matching chart.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScoreboxLocalRecords<'a> {
    pub machine: Option<&'a ScoreboxMachineView>,
    pub personal: Option<ScoreboxLocalView>,
}

/// Shell-prepared scores and leaderboards for one physical player side.
#[derive(Clone, Debug, Default)]
pub struct ScoreboxSideView {
    pub joined: bool,
    pub chart_hash: Option<String>,
    /// Any online leaderboard service can serve this side.
    pub online_active: bool,
    pub show_ex_score: bool,
    pub pane_filter: deadsync_score::SelectMusicScoreboxFilter,
    pub display_name: Arc<str>,
    pub groovestats_username: Arc<str>,
    pub player_initials: Arc<str>,
    pub local_itg: Option<ScoreboxLocalView>,
    pub local_ex: Option<ScoreboxLocalView>,
    pub local_hard_ex: Option<ScoreboxLocalView>,
    pub local_itl: Option<ScoreboxLocalView>,
    pub machine_itg: Option<ScoreboxMachineView>,
    pub leaderboards: Option<CachedPlayerLeaderboardData>,
}

impl ScoreboxSideView {
    /// Borrow the displayable local records if the snapshot belongs to the current chart.
    pub fn local_records(&self, chart_hash: Option<&str>) -> ScoreboxLocalRecords<'_> {
        if chart_hash.is_none() || self.chart_hash.as_deref() != chart_hash {
            return ScoreboxLocalRecords::default();
        }
        ScoreboxLocalRecords {
            machine: self
                .machine_itg
                .as_ref()
                .filter(|score| !score.failed || score.score_10000 > 0.0),
            personal: self
                .local_itg
                .filter(|score| !score.failed || score.score_10000 > 0.0),
        }
    }
}

/// One shell-resolved application path and its platform-friendly label.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppPathView {
    pub path: PathBuf,
    pub display: Arc<str>,
}

/// Stable identifiers for application paths exposed to theme UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppPathKind {
    Data,
    Cache,
    Songs,
    Courses,
    Profiles,
    Screenshots,
    LogFile,
    ConfigFile,
}

/// Shell-prepared application paths used by generic folder UIs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppPathsView {
    pub data: AppPathView,
    pub cache: AppPathView,
    pub songs: AppPathView,
    pub courses: AppPathView,
    pub profiles: AppPathView,
    pub screenshots: AppPathView,
    pub log_file: AppPathView,
    pub config_file: AppPathView,
}

impl AppPathsView {
    #[inline(always)]
    #[must_use]
    pub const fn get(&self, kind: AppPathKind) -> &AppPathView {
        match kind {
            AppPathKind::Data => &self.data,
            AppPathKind::Cache => &self.cache,
            AppPathKind::Songs => &self.songs,
            AppPathKind::Courses => &self.courses,
            AppPathKind::Profiles => &self.profiles,
            AppPathKind::Screenshots => &self.screenshots,
            AppPathKind::LogFile => &self.log_file,
            AppPathKind::ConfigFile => &self.config_file,
        }
    }
}

/// One shell-discovered audio output exposed without backend runtime types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioOutputDeviceView {
    pub name: String,
    pub is_default: bool,
    pub sample_rates_hz: Vec<u32>,
}

/// Startup audio choices prepared by the shell for a theme options screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioOptionsView {
    pub output_devices: Vec<AudioOutputDeviceView>,
    pub available_backend_names: Vec<String>,
    pub output_device: Option<u16>,
    pub output_mode: crate::AudioOutputModeChoice,
    pub selected_backend_name: String,
    pub sample_rate_hz: Option<u32>,
    pub preserve_pitch: bool,
    pub replay_gain: bool,
    pub master_volume: u8,
    pub music_volume: u8,
    pub sfx_volume: u8,
    pub assist_tick_volume: u8,
}

pub use deadsync_core::display::VideoMode as GraphicsVideoModeView;

/// Host monitor capabilities prepared by the shell for theme option UIs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphicsMonitorView {
    pub name: String,
    pub modes: Vec<GraphicsVideoModeView>,
}

/// Current graphics configuration and host-dependent choices exposed without
/// renderer, windowing, or platform implementation types.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphicsOptionsView {
    pub renderer: RendererChoice,
    pub display_mode: DisplayModeChoice,
    pub fullscreen: FullscreenChoice,
    pub monitor: usize,
    pub width: u32,
    pub height: u32,
    pub refresh_rate_millihertz: u32,
    pub aspect_ratio: f32,
    pub max_fps: u16,
    pub vsync: bool,
    pub present_policy: PresentPolicyChoice,
    pub high_dpi: bool,
    pub software_thread_choices: Vec<u8>,
    pub software_threads: u8,
}

/// Presentation hooks for the shared service-options controller.
pub struct SystemOptionsRenderer<'a, A> {
    pub phase: SystemOptionsPhase,
    pub background: &'a dyn Fn(&mut Vec<A>),
    pub page: &'a mut dyn FnMut(&mut Vec<A>, SystemOptionsPage<'_>),
}

/// Time to finish entering or leaving a settings page, in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SystemOptionsTiming {
    pub service_seconds: f32,
    pub settings_seconds: f32,
}

/// Elapsed seconds in the current page transition.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SystemOptionsPhase {
    #[default]
    Steady,
    Entering(f32),
    Leaving(f32),
}

/// Existing actions a service menu can place first. Listed input tools move
/// from the Input submenu to the service menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemOptionsServiceRow {
    Arcade,
    System,
    Graphics,
    SongPacks,
    Downloads,
    TestInput,
    ConfigureMappings,
    ReloadSongsCourses,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemOptionsPageKind {
    ServiceMenu,
    BasicGraphics,
    Settings,
}

pub struct SystemOptionsPage<'a> {
    /// Identity of the page within the current Options visit.
    pub id: usize,
    pub title: &'static str,
    pub kind: SystemOptionsPageKind,
    /// Selection in the visible controller order. `row_count` selects Done.
    pub selected: usize,
    pub row_count: usize,
    pub row: &'a dyn Fn(usize) -> Option<SystemOptionsRow>,
    pub description: Option<Arc<str>>,
    pub detail: Option<Arc<str>>,
    pub phase: SystemOptionsPhase,
}

pub struct SystemOptionsRow {
    pub label: Arc<str>,
    pub choices: Option<Arc<[Arc<str>]>>,
    pub selected_choice: usize,
    pub inline: bool,
    pub disabled: bool,
    /// Active bits of a multi-select row, in its choice order.
    pub enabled_choices: Option<u64>,
    pub choice_tint: Option<[f32; 4]>,
}

/// Audio timing telemetry translated by the shell for theme diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioTimingView {
    pub backend: &'static str,
    pub requested_output_mode: &'static str,
    pub fallback_from_native: bool,
    pub timing_clock: &'static str,
    pub timing_quality: &'static str,
    pub sample_rate_hz: u32,
    pub device_period_ns: u64,
    pub stream_latency_ns: u64,
    pub buffer_frames: u32,
    pub padding_frames: u32,
    pub queued_frames: u32,
    pub estimated_output_delay_ns: u64,
    pub clock_fallback_count: u64,
    pub timing_sanity_failure_count: u64,
    pub underrun_count: u64,
}

/// Native gamepad lifecycle data translated by the shell for theme display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GamepadSystemView {
    Connected {
        name: String,
        id: usize,
        vendor_id: Option<u16>,
        product_id: Option<u16>,
        backend: &'static str,
        initial: bool,
    },
    Disconnected {
        name: String,
        id: usize,
        backend: &'static str,
        initial: bool,
    },
    StartupComplete,
}

/// Cabinet and pad light state prepared by the shell for a test-light screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightsTestView {
    pub cabinet: [bool; 6],
    pub buttons: [[bool; 6]; 2],
    pub manual_cycle: bool,
}

/// Installed noteskin names discovered by the shell for theme option lists.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteskinCatalogView {
    pub names: Vec<String>,
}

/// Shell-observed SMX pad identity and input state for concrete theme flows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SmxAssignmentPadView {
    pub connected: bool,
    pub serial: String,
    pub label: String,
    pub input_state: u16,
    pub backend_id: String,
    pub pad_type: Option<String>,
}

/// Backend-neutral SMX assignment state prepared by the shell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SmxAssignmentView {
    pub pads: [SmxAssignmentPadView; 2],
    pub can_swap: bool,
    pub conflict_warning: bool,
    pub conflict_rgb: [f32; 3],
    pub player_rgb: [[u8; 3]; 2],
}

/// SMX GIF pack names discovered by the shell from runtime asset directories.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SmxGifCatalogView {
    pub background_packs: Vec<String>,
    pub judgment_packs: Vec<String>,
}

pub use deadsync_chart::course::CourseStagePlan as CourseStageView;
pub use deadsync_rules::course::CourseType as CourseTypeView;

/// Stage facts supplied before the presenter chooses the intro and labels.
pub struct GameplayStageView {
    pub intro: GameplayStageIntro,
    pub replay: Option<GameplayReplayView>,
    pub course: Option<Arc<SongData>>,
}

#[derive(Clone)]
pub enum GameplayStageIntro {
    Song {
        round: Option<usize>,
        event_name: Option<Arc<str>>,
    },
    Course { number: usize, total: Option<usize> },
    Restart(u32),
    Practice,
    Demonstration,
}

pub struct GameplayReplayView {
    pub name: String,
    pub score_10000: f64,
}

pub use deadsync_core::hud_style::{
    GameplayBackgroundWarningView, GameplayDifficultyView, GameplayHoldAtlas, GameplayHud,
    GameplayHudMotion, GameplayLifeView, GameplayProfileFooterView, GameplayScoreView,
    GameplaySongMeterView, GameplayStageBadgeView, GameplayTapAtlas, GameplayWarningScope,
};

/// Optional HUD updates from the live stage. None of these hooks control play.
pub trait GameplayLivePresentation<G> {
    fn update_gameplay_visuals(&mut self, _game: &G, _delta_time: f32) {}

    fn sync_leaderboards(&mut self, _updates: [Option<CachedPlayerLeaderboardData>; MAX_PLAYERS]) {}
    fn sync_itl_cmod_warning(&mut self, _warnings: [bool; MAX_PLAYERS]) {}

    fn toggle_lobby_hud(&mut self) {}

    fn sync_smx_sensors(&mut self, _views: [Option<SmxSensorPadView>; MAX_PLAYERS]) {}

    fn sync_heart_rate(&mut self, _generation: (u64, u64), _view: HeartRateView) {}
}

pub const SMX_SENSOR_PANEL_COUNT: usize = 9;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SmxSensorPanelView {
    pub threshold: u16,
    pub value: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmxSensorPadView {
    pub fsr: bool,
    pub panels: [SmxSensorPanelView; SMX_SENSOR_PANEL_COUNT],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeartRatePlayerView {
    pub configured: bool,
    pub connected: bool,
    pub bpm: Option<u16>,
    pub max_heart_rate: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeartRateView {
    pub players: [HeartRatePlayerView; MAX_PLAYERS],
}

/// Theme-prepared course labels and summary metadata, separate from game startup.
#[derive(Clone, Debug)]
pub struct CoursePresentationView {
    pub name: String,
    pub banner_path: Option<PathBuf>,
    pub song_stub: Arc<SongData>,
    pub course_stepchart_label: String,
}

/// Compatibility view combining a game session plan with its presentation.
#[derive(Clone, Debug)]
pub struct SelectedCourseView {
    pub path: PathBuf,
    pub name: String,
    pub banner_path: Option<PathBuf>,
    pub score_hash: String,
    pub song_stub: Arc<SongData>,
    pub course_difficulty_name: String,
    pub course_meter: Option<u32>,
    pub course_stepchart_label: String,
    pub course_type: CourseTypeView,
    pub lives: i32,
    pub stages: Vec<CourseStageView>,
}

impl SelectedCourseView {
    /// Separate gameplay data and presentation by moving their existing storage.
    pub fn into_parts(self) -> (CourseSessionPlan, CoursePresentationView) {
        (
            CourseSessionPlan {
                path: self.path,
                score_hash: self.score_hash,
                course_difficulty_name: self.course_difficulty_name,
                course_meter: self.course_meter,
                course_type: self.course_type,
                lives: self.lives,
                stages: self.stages,
            },
            CoursePresentationView {
                name: self.name,
                banner_path: self.banner_path,
                song_stub: self.song_stub,
                course_stepchart_label: self.course_stepchart_label,
            },
        )
    }

    /// Rebuild the compatibility view without cloning either part.
    pub fn from_parts(plan: CourseSessionPlan, presentation: CoursePresentationView) -> Self {
        Self {
            path: plan.path,
            name: presentation.name,
            banner_path: presentation.banner_path,
            score_hash: plan.score_hash,
            song_stub: presentation.song_stub,
            course_difficulty_name: plan.course_difficulty_name,
            course_meter: plan.course_meter,
            course_stepchart_label: presentation.course_stepchart_label,
            course_type: plan.course_type,
            lives: plan.lives,
            stages: plan.stages,
        }
    }
}

/// Chart source and duration used by a course-summary density graph.
#[derive(Clone, Debug)]
pub struct CourseGraphStageView {
    pub chart: Arc<ChartData>,
    pub song_last_second: f32,
    pub step_seconds: f32,
}

/// Final score snapshot consumed by evaluation screens and course summaries.
///
/// The concrete noteskin handle and player-side identity remain generic so
/// this contract does not depend on an asset manager or profile runtime.
#[derive(Clone)]
pub struct EvaluationView<N, S> {
    pub song: Arc<SongData>,
    pub chart: Arc<ChartData>,
    pub course_graph_stages: Vec<CourseGraphStageView>,
    pub side: S,
    pub profile_name: String,
    pub score_valid: bool,
    pub disqualified: bool,
    pub expected_groovestats_submit: bool,
    pub expected_arrowcloud_submit: bool,
    pub groovestats: GrooveStatsEvalState,
    pub itl: ItlEvalState,
    pub judgment_counts: judgment::JudgeCounts,
    pub score_percent: f64,
    pub numeric_score: u32,
    pub radar_achievement: Option<[f32; 5]>,
    pub stages_passed: u32,
    pub earned_grade_points: i32,
    pub possible_grade_points: i32,
    pub grade: Grade,
    pub speed_mod: ScrollSpeedSetting,
    pub mods_text: Arc<str>,
    pub max_combo: u32,
    pub combo_runs: Arc<[ComboRun]>,
    pub jumps: [u32; 2],
    pub hands_achieved: u32,
    pub hands_total: u32,
    pub holds_held: u32,
    pub holds_held_for_score: u32,
    pub dropped_hold_or_roll: bool,
    pub holds_total: u32,
    pub rolls_held: u32,
    pub rolls_held_for_score: u32,
    pub rolls_total: u32,
    pub mines_hit_for_score: u32,
    pub mines_avoided: u32,
    pub mines_total: u32,
    pub timing: TimingStats,
    pub arrow_timing: ArrowTimingStats,
    pub scatter: Vec<ScatterPoint>,
    pub scatter_worst_window_ms: f32,
    pub histogram: HistogramMs,
    pub graph_first_second: f32,
    pub graph_last_second: f32,
    pub music_rate: f32,
    pub life_history: Vec<(f32, f32)>,
    pub steps_life: StageLifeTimeline,
    pub fail_time: Option<f32>,
    pub window_counts: WindowCounts,
    pub window_counts_10ms: WindowCounts,
    pub ex_score_percent: f64,
    pub hard_ex_score_percent: f64,
    pub calories_burned: f32,
    pub column_judgments: ColumnJudgmentList,
    pub noteskin: Option<N>,
    pub show_fa_plus_window: bool,
    pub show_ex_score: bool,
    pub show_hard_ex_score: bool,
    pub show_fa_plus_pane: bool,
    pub track_early_judgments: bool,
    pub dim_post_fail_scatter: bool,
    pub golf_score: Option<deadsync_rules::golf::GolfScore>,
    pub golf_par: Option<f64>,
    pub disabled_timing_windows: [bool; 5],
    pub machine_records: Vec<LeaderboardEntry>,
    pub machine_record_highlight_rank: Option<u32>,
    pub personal_records: Vec<LeaderboardEntry>,
    pub personal_record_highlight_rank: Option<u32>,
    pub show_machine_personal_split: bool,
}

impl<N, S> EvaluationView<N, S> {
    #[inline(always)]
    pub const fn judgment_count(&self, grade: JudgeGrade) -> u32 {
        self.judgment_counts[judgment::judge_grade_ix(grade)]
    }

    #[inline(always)]
    pub const fn is_course_summary(&self) -> bool {
        !self.course_graph_stages.is_empty()
    }
}

/// Completed-stage data supplied to the result screen at entry.
#[derive(Clone)]
pub struct EvaluationStageView<N, S> {
    pub score_info: EvaluationScores<N, S>,
    pub stage_number: Option<usize>,
    /// The shared Routine score displayed alongside individual results.
    pub routine_shared_score_percent: Option<f64>,
    pub stage_duration_seconds: f32,
    pub fail_stream_progress: [Option<(u32, u32)>; MAX_PLAYERS],
}

impl<N, S> Default for EvaluationStageView<N, S> {
    fn default() -> Self {
        Self {
            score_info: Arc::new([const { None }; MAX_PLAYERS]),
            stage_number: None,
            routine_shared_score_percent: None,
            stage_duration_seconds: 0.0,
            fail_stream_progress: [None; MAX_PLAYERS],
        }
    }
}

/// Shared, immutable score records for one result page.
pub type EvaluationScores<N, S> = Arc<[Option<EvaluationView<N, S>>; MAX_PLAYERS]>;

/// Completed results for the active evaluation page.
pub struct EvaluationOverview<'a> {
    pub title: &'a Arc<str>,
    pub artist: &'a Arc<str>,
    pub banner_key: Option<&'a Arc<str>>,
    pub course_summary: bool,
    pub stage_number: Option<usize>,
    pub session_style: crate::select_music::SessionStyleView,
    pub elapsed: f32,
    /// Time since the player elected to leave. The visit clock stops at that point.
    pub departure_elapsed: Option<f32>,
    pub players: [Option<EvaluationPlayerOverview<'a>>; MAX_PLAYERS],
}

pub struct EvaluationPlayerOverview<'a> {
    pub p2: bool,
    pub grade: Grade,
    pub percent: f64,
    pub numeric_score: u32,
    pub radar_possible: [f32; 5],
    pub radar_achievement: Option<[f32; 5]>,
    pub stages_passed: u32,
    pub individual_percent: Option<f64>,
    pub ex_percent: Option<f64>,
    pub hard_ex_percent: Option<f64>,
    pub difficulty: &'a str,
    pub meter: u32,
    pub mods: &'a Arc<str>,
    pub judgments: judgment::JudgeCounts,
    pub row_window_counts: WindowCounts,
    pub score_valid: bool,
    pub dropped_hold_or_roll: bool,
    pub jumps: [u32; 2],
    pub holds: [u32; 2],
    pub mines: [u32; 2],
    pub hands: [u32; 2],
    pub rolls: [u32; 2],
    pub max_combo: u32,
    pub combo_runs: &'a [ComboRun],
    pub graph_seconds: [f32; 2],
    pub course_graph_stages: &'a [CourseGraphStageView],
    pub steps_life: &'a StageLifeTimeline,
    pub machine_rank: Option<u32>,
    pub personal_rank: Option<u32>,
    pub disqualified: bool,
}

/// Density data requested by a theme for a chart preview.
#[derive(Clone, Debug)]
pub struct DensityGraphView {
    pub max_nps: f64,
    pub measure_nps_vec: Vec<f64>,
    pub measure_seconds_vec: Vec<f32>,
    pub first_second: f32,
    pub last_second: f32,
}

/// One profile row displayed by a theme's pad-configuration UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PadProfileEntry {
    pub name: String,
    pub is_default: bool,
    pub is_active: bool,
}

/// Presentation-facing timing health prepared by the process shell.
#[derive(Clone, Copy, Debug)]
pub struct TimingHealthView<P, C, A> {
    pub interval_ns: u64,
    pub display_error_ms: f32,
    pub display_catching_up: bool,
    pub present_mode: P,
    pub display_clock: C,
    pub host_clock: C,
    pub in_flight_images: u8,
    pub waited_for_image: bool,
    pub applied_back_pressure: bool,
    pub queue_idle_waited: bool,
    pub suboptimal: bool,
    pub submitted_present_id: u32,
    pub completed_present_id: u32,
    pub calibration_error_ns: u64,
    pub host_mapped: bool,
    pub audio: Option<A>,
}

/// Which screen corner or center seam a frame-statistics overlay uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopCenter,
    BottomCenter,
}

impl OverlayAnchor {
    #[inline(always)]
    #[must_use]
    pub const fn to_key(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::TopRight => "top-right",
            Self::BottomLeft => "bottom-left",
            Self::BottomRight => "bottom-right",
            Self::TopCenter => "top-center",
            Self::BottomCenter => "bottom-center",
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "top-left" => Some(Self::TopLeft),
            "top-right" => Some(Self::TopRight),
            "bottom-left" => Some(Self::BottomLeft),
            "bottom-right" => Some(Self::BottomRight),
            "top-center" => Some(Self::TopCenter),
            "bottom-center" => Some(Self::BottomCenter),
            _ => None,
        }
    }
}

/// Presentation detail shown by a frame-statistics overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayStyle {
    Detailed,
    Minimal,
}

impl OverlayStyle {
    #[inline(always)]
    #[must_use]
    pub const fn toggle(self) -> Self {
        match self {
            Self::Detailed => Self::Minimal,
            Self::Minimal => Self::Detailed,
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Detailed => "detailed",
            Self::Minimal => "minimal",
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn from_key(key: &str) -> Self {
        match key.trim().to_ascii_lowercase().as_str() {
            "minimal" => Self::Minimal,
            _ => Self::Detailed,
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn show_p99(self) -> bool {
        matches!(self, Self::Detailed)
    }

    #[inline(always)]
    #[must_use]
    pub const fn show_histogram(self) -> bool {
        matches!(self, Self::Detailed)
    }
}

/// One captured frame's per-phase timing plus sync state.
#[derive(Clone, Copy, Debug)]
pub struct FrameStatsSample {
    pub host_nanos: u64,
    pub frame_us: u32,
    pub maintenance_us: u32,
    pub input_us: u32,
    pub update_us: u32,
    pub compose_us: u32,
    pub upload_us: u32,
    pub draw_us: u32,
    pub gpu_wait_us: u32,
    pub display_error_us: i32,
    pub catching_up: bool,
}

impl FrameStatsSample {
    #[inline(always)]
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            host_nanos: 0,
            frame_us: 0,
            maintenance_us: 0,
            input_us: 0,
            update_us: 0,
            compose_us: 0,
            upload_us: 0,
            draw_us: 0,
            gpu_wait_us: 0,
            display_error_us: 0,
            catching_up: false,
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.host_nanos == 0
    }

    #[inline(always)]
    #[must_use]
    pub const fn measured_us(&self) -> u32 {
        self.maintenance_us
            .saturating_add(self.input_us)
            .saturating_add(self.update_us)
            .saturating_add(self.compose_us)
            .saturating_add(self.upload_us)
            .saturating_add(self.draw_us)
            .saturating_add(self.gpu_wait_us)
    }

    #[inline(always)]
    #[must_use]
    pub const fn idle_us(&self) -> u32 {
        self.frame_us.saturating_sub(self.measured_us())
    }

    #[inline(always)]
    #[must_use]
    pub const fn cpu_work_us(&self) -> u32 {
        self.maintenance_us
            .saturating_add(self.input_us)
            .saturating_add(self.update_us)
            .saturating_add(self.compose_us)
            .saturating_add(self.upload_us)
            .saturating_add(self.draw_us)
    }
}

/// Precomputed sync-health readouts supplied to an overlay renderer.
#[derive(Clone, Copy, Debug)]
pub struct FrameStatsSummary {
    pub avg_frame_us: u32,
    pub p99_frame_us: u32,
    pub max_frame_us: u32,
    pub fps: f32,
    pub display_error_ms: f32,
    pub display_error_p99_ms: f32,
    pub display_catching_up: bool,
    pub in_gameplay: bool,
    pub audio_callback_gap_ms: f32,
    pub audio_underruns: u64,
    pub audio_output_delay_ms: f32,
    pub audio_queued_frames: u32,
    pub frame_jitter_us: u32,
    pub display_error_jitter_us: u32,
    pub spike_hold_us: u32,
    pub target_frame_us: u32,
    pub cpu_work_us: u32,
    pub gpu_wait_us: u32,
    pub over_budget_count: u32,
    pub catch_up_count: u32,
}

#[cfg(test)]
mod tests {
    use super::{
        AudioOptionsView, AudioOutputDeviceView, FrameStatsSample, OverlayAnchor, OverlayStyle,
    };

    #[test]
    fn audio_options_view_preserves_plain_device_data() {
        let view = AudioOptionsView {
            output_devices: vec![AudioOutputDeviceView {
                name: "Primary".to_owned(),
                is_default: true,
                sample_rates_hz: vec![44_100, 48_000],
            }],
            available_backend_names: vec!["Auto".to_owned(), "ALSA".to_owned()],
            output_device: Some(0),
            output_mode: crate::AudioOutputModeChoice::Shared,
            selected_backend_name: "ALSA".to_owned(),
            sample_rate_hz: Some(48_000),
            preserve_pitch: true,
            replay_gain: false,
            master_volume: 90,
            music_volume: 80,
            sfx_volume: 70,
            assist_tick_volume: 60,
        };

        assert_eq!(view.output_devices[0].name, "Primary");
        assert_eq!(view.output_devices[0].sample_rates_hz, [44_100, 48_000]);
        assert_eq!(view.available_backend_names, ["Auto", "ALSA"]);
        assert_eq!(view.output_device, Some(0));
        assert_eq!(view.output_mode, crate::AudioOutputModeChoice::Shared);
        assert_eq!(view.selected_backend_name, "ALSA");
        assert_eq!(view.sample_rate_hz, Some(48_000));
        assert!(view.preserve_pitch);
        assert_eq!(view.master_volume, 90);
    }

    #[test]
    fn frame_sample_preserves_phase_and_idle_math() {
        let sample = FrameStatsSample {
            host_nanos: 1,
            frame_us: 10_000,
            maintenance_us: 50,
            input_us: 100,
            update_us: 200,
            compose_us: 300,
            upload_us: 400,
            draw_us: 500,
            gpu_wait_us: 600,
            display_error_us: 0,
            catching_up: false,
        };
        assert_eq!(sample.cpu_work_us(), 1_550);
        assert_eq!(sample.measured_us(), 2_150);
        assert_eq!(sample.idle_us(), 7_850);
    }

    #[test]
    fn overlay_keys_round_trip() {
        for anchor in [
            OverlayAnchor::TopLeft,
            OverlayAnchor::TopRight,
            OverlayAnchor::BottomLeft,
            OverlayAnchor::BottomRight,
            OverlayAnchor::TopCenter,
            OverlayAnchor::BottomCenter,
        ] {
            assert_eq!(OverlayAnchor::from_key(anchor.to_key()), Some(anchor));
        }
        assert_eq!(OverlayAnchor::from_key("auto"), None);
        assert_eq!(OverlayStyle::from_key("minimal"), OverlayStyle::Minimal);
        assert_eq!(OverlayStyle::from_key("unknown"), OverlayStyle::Detailed);
    }
}

pub use deadsync_core::hud_style::GameplayChatLayout;

/// Borrowed chat snapshot and current screen facts for overlay composition.
pub struct TwitchChatView<'a, Message, Options> {
    pub screen: crate::Screen,
    pub gameplay: Option<GameplayChatLayout>,
    pub tournament: bool,
    pub sides: [bool; MAX_PLAYERS],
    pub entries: &'a [Arc<Message>],
    pub revision: u64,
    pub options: Options,
}

pub trait TwitchChatPresentation<A, Message, Options> {
    fn push_chat(
        &mut self,
        actors: &mut Vec<A>,
        assets: &dyn deadlib_assets::AssetView,
        view: TwitchChatView<'_, Message, Options>,
    );
}
