use deadlib_render::{BackendType, PresentModePolicy};
use deadsync_assets::noteskin::Noteskin;
use deadsync_chart::{ChartData, SongData};
use deadsync_config::app_config::DisplayMode;
use deadsync_profile::{ActiveProfile, PlayerSide};
use deadsync_rules::judgment::{self, JudgeGrade};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing as timing_stats;
use deadsync_score as score_data;
use deadsync_simfile::sync_offset::SongOffsetSyncChange;
use std::path::PathBuf;
use std::sync::Arc;

pub mod input;

#[derive(Clone, Debug)]
pub struct CourseStagePlan {
    pub song: Arc<SongData>,
    pub chart_hash: String,
}

#[derive(Clone, Debug)]
pub struct SelectedCoursePlan {
    pub path: PathBuf,
    pub name: String,
    pub banner_path: Option<PathBuf>,
    pub score_hash: String,
    pub song_stub: Arc<SongData>,
    pub course_difficulty_name: String,
    pub course_meter: Option<u32>,
    pub course_stepchart_label: String,
    pub stages: Vec<CourseStagePlan>,
}

#[derive(Clone, Debug)]
pub struct CourseGraphStage {
    pub chart: Arc<ChartData>,
    pub song_last_second: f32,
}

/// Final score snapshot consumed by evaluation screens and course summaries.
#[derive(Clone)]
pub struct ScoreInfo {
    pub song: Arc<SongData>,
    pub chart: Arc<ChartData>,
    pub course_graph_stages: Vec<CourseGraphStage>,
    pub side: PlayerSide,
    pub profile_name: String,
    pub score_valid: bool,
    pub disqualified: bool,
    pub expected_groovestats_submit: bool,
    pub expected_arrowcloud_submit: bool,
    pub groovestats: score_data::GrooveStatsEvalState,
    pub itl: score_data::ItlEvalState,
    pub judgment_counts: judgment::JudgeCounts,
    pub score_percent: f64,
    pub earned_grade_points: i32,
    pub possible_grade_points: i32,
    pub grade: score_data::Grade,
    pub speed_mod: ScrollSpeedSetting,
    pub mods_text: Arc<str>,
    pub hands_achieved: u32,
    pub hands_total: u32,
    pub holds_held: u32,
    pub holds_held_for_score: u32,
    pub holds_total: u32,
    pub rolls_held: u32,
    pub rolls_held_for_score: u32,
    pub rolls_total: u32,
    pub mines_hit_for_score: u32,
    pub mines_avoided: u32,
    pub mines_total: u32,
    pub timing: timing_stats::TimingStats,
    pub arrow_timing: timing_stats::ArrowTimingStats,
    pub scatter: Vec<timing_stats::ScatterPoint>,
    pub scatter_worst_window_ms: f32,
    pub histogram: timing_stats::HistogramMs,
    pub graph_first_second: f32,
    pub graph_last_second: f32,
    pub music_rate: f32,
    pub life_history: Vec<(f32, f32)>,
    pub fail_time: Option<f32>,
    pub window_counts: timing_stats::WindowCounts,
    pub window_counts_10ms: timing_stats::WindowCounts,
    pub ex_score_percent: f64,
    pub hard_ex_score_percent: f64,
    pub calories_burned: f32,
    pub column_judgments: Vec<score_data::ColumnJudgments>,
    pub noteskin: Option<Arc<Noteskin>>,
    pub show_fa_plus_window: bool,
    pub show_ex_score: bool,
    pub show_hard_ex_score: bool,
    pub show_fa_plus_pane: bool,
    pub track_early_judgments: bool,
    pub disabled_timing_windows: [bool; 5],
    pub machine_records: Vec<score_data::LeaderboardEntry>,
    pub machine_record_highlight_rank: Option<u32>,
    pub personal_records: Vec<score_data::LeaderboardEntry>,
    pub personal_record_highlight_rank: Option<u32>,
    pub show_machine_personal_split: bool,
}

impl ScoreInfo {
    #[inline(always)]
    pub fn judgment_count(&self, grade: JudgeGrade) -> u32 {
        self.judgment_counts[judgment::judge_grade_ix(grade)]
    }

    #[inline(always)]
    pub fn is_course_summary(&self) -> bool {
        !self.course_graph_stages.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DensityGraphSlot {
    SelectMusicP1,
    SelectMusicP2,
}

#[derive(Debug, Clone)]
pub struct DensityGraphSource {
    pub max_nps: f64,
    pub measure_nps_vec: Vec<f64>,
    pub measure_seconds_vec: Vec<f32>,
    pub first_second: f32,
    pub last_second: f32,
}

#[derive(Debug, Clone)]
pub enum ScreenAction {
    None,
    /// Consume the current input edge without scheduling app-level work.
    ConsumeInput,
    Navigate(Screen),
    /// Navigate immediately without running the current screen's out-transition.
    /// This is used for cases where the current screen already rendered its own
    /// full-screen transition-out animation and we only want the target's in-transition.
    NavigateNoFade(Screen),
    Exit,
    /// Power off the host machine after the menu out-transition. Only
    /// dispatched when the operator has enabled `AllowShutdown` in
    /// `deadsync.ini` and the user picks the Shutdown menu entry.
    Shutdown,
    SelectProfiles {
        p1: ActiveProfile,
        p2: ActiveProfile,
    },
    /// Open the ArrowCloud QR-login screen scoped to a specific profile
    /// (rather than P1/P2 session sides).  Dispatched from
    /// Manage Local Profiles → per-profile menu → Link ArrowCloud.
    LinkArrowCloud {
        profile_id: String,
        display_name: String,
    },
    /// GrooveStats counterpart of `LinkArrowCloud`.
    LinkGrooveStats {
        profile_id: String,
        display_name: String,
    },
    RequestScreenshot(Option<PlayerSide>),
    RequestBanner(Option<PathBuf>),
    RequestCdTitle(Option<PathBuf>),
    RequestPackBanner(Option<PathBuf>),
    RequestWheelItemBackgrounds(Vec<PathBuf>),
    RequestDensityGraph {
        slot: DensityGraphSlot,
        chart_opt: Option<DensityGraphSource>,
    },
    ApplySongOffsetSync {
        simfile_path: PathBuf,
        delta_seconds: f32,
    },
    ApplySongOffsetSyncBatch {
        changes: Vec<SongOffsetSyncChange>,
    },
    FetchOnlineGrade(String),
    WriteFsrDump,
    ChangeGraphics {
        renderer: Option<BackendType>,
        display_mode: Option<DisplayMode>,
        monitor: Option<usize>,
        resolution: Option<(u32, u32)>,
        vsync: Option<bool>,
        present_mode_policy: Option<PresentModePolicy>,
        max_fps: Option<u16>,
        high_dpi: Option<bool>,
    },
    UpdateShowOverlay(u8),
    UpdateMouseCursorHidden(bool),
    TestLightsSetAuto,
    TestLightsStepCabinet(i8),
    TestLightsStepButton(i8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Gameplay,
    Practice,
    Options,
    Credits,
    ManageLocalProfiles,
    Init,
    Initials,
    GameOver,
    Mappings,
    Input,
    SelectProfile,
    GrooveStatsLogin,
    ArrowCloudLogin,
    SelectColor,
    SelectStyle,
    SelectPlayMode,
    ProfileLoad,
    SelectMusic,
    SelectCourse,
    Sandbox,
    Evaluation,
    EvaluationSummary,
    PlayerOptions,
    TestLights,
    OverscanAdjustment,
    ConfigurePads,
    SmxAssignPads,
}

impl Screen {
    /// Stable external screen name written to `save/current_screen.txt`.
    pub const fn current_screen_file_name(self) -> &'static str {
        match self {
            Self::Menu => "ScreenTitleMenu",
            Self::Gameplay => "ScreenGameplay",
            Self::Practice => "ScreenPractice",
            Self::Options => "ScreenOptionsService",
            Self::Credits => "ScreenCredits",
            Self::ManageLocalProfiles => "ScreenOptionsManageProfiles",
            Self::Init => "ScreenInit",
            Self::Initials => "ScreenNameEntryTraditional",
            Self::GameOver => "ScreenGameOver",
            Self::Mappings => "ScreenMapControllers",
            Self::Input => "ScreenTestInput",
            Self::SelectProfile => "ScreenSelectProfile",
            Self::GrooveStatsLogin => "ScreenGrooveStatsLogin",
            Self::ArrowCloudLogin => "ScreenArrowCloudLogin",
            Self::SelectColor => "ScreenSelectColor",
            Self::SelectStyle => "ScreenSelectStyle",
            Self::SelectPlayMode => "ScreenSelectPlayMode",
            Self::ProfileLoad => "ScreenProfileLoad",
            Self::SelectMusic => "ScreenSelectMusic",
            Self::SelectCourse => "ScreenSelectCourse",
            Self::Sandbox => "ScreenSandbox",
            Self::Evaluation => "ScreenEvaluationStage",
            Self::EvaluationSummary => "ScreenEvaluationSummary",
            Self::PlayerOptions => "ScreenPlayerOptions",
            Self::TestLights => "ScreenTestLights",
            Self::OverscanAdjustment => "ScreenOverscanConfig",
            Self::ConfigurePads => "ScreenConfigurePads",
            Self::SmxAssignPads => "ScreenSmxAssignPads",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Screen;

    #[test]
    fn current_screen_file_names_match_theme_names() {
        assert_eq!(Screen::Menu.current_screen_file_name(), "ScreenTitleMenu");
        assert_eq!(
            Screen::Options.current_screen_file_name(),
            "ScreenOptionsService"
        );
        assert_eq!(
            Screen::Practice.current_screen_file_name(),
            "ScreenPractice"
        );
        assert_eq!(
            Screen::ManageLocalProfiles.current_screen_file_name(),
            "ScreenOptionsManageProfiles"
        );
        assert_eq!(
            Screen::Mappings.current_screen_file_name(),
            "ScreenMapControllers"
        );
        assert_eq!(Screen::Input.current_screen_file_name(), "ScreenTestInput");
        assert_eq!(
            Screen::Evaluation.current_screen_file_name(),
            "ScreenEvaluationStage"
        );
        assert_eq!(
            Screen::PlayerOptions.current_screen_file_name(),
            "ScreenPlayerOptions"
        );
        assert_eq!(
            Screen::TestLights.current_screen_file_name(),
            "ScreenTestLights"
        );
    }
}
