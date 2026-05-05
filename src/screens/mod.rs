pub mod components;
pub mod credits;
pub mod edit_profile;
pub mod evaluation;
pub mod evaluation_summary;
pub(crate) mod favorite_code;
pub mod gameover;
pub mod gameplay;
pub mod init;
pub mod initials;
pub mod input;
pub mod manage_local_profiles;
pub mod mappings;
pub mod menu;
pub mod options;
pub(crate) mod pack_sync;
pub mod player_options;
pub mod practice;
pub mod profile_load;
pub mod sandbox;
pub mod select_color;
pub mod select_course;
pub mod select_mode;
pub mod select_music;
pub mod select_profile;
pub mod select_style;
use std::path::PathBuf;

use crate::config::DisplayMode;
use crate::engine::gfx::{BackendType, PresentModePolicy};
use crate::game::profile::{ActiveProfile, PlayerSide};

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
pub struct SongOffsetSyncChange {
    pub simfile_path: PathBuf,
    pub delta_seconds: f32,
}

#[derive(Debug, Clone)]
pub enum ScreenAction {
    None,
    Navigate(Screen),
    /// Navigate immediately without running the current screen's out-transition.
    /// This is used for cases where the current screen already rendered its own
    /// full-screen transition-out animation and we only want the target's in-transition.
    NavigateNoFade(Screen),
    Exit,
    SelectProfiles {
        p1: ActiveProfile,
        p2: ActiveProfile,
    },
    RequestScreenshot(Option<PlayerSide>),
    RequestBanner(Option<PathBuf>),
    RequestCdTitle(Option<PathBuf>),
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Gameplay,
    Practice,
    Options,
    Credits,
    ManageLocalProfiles,
    EditProfile,
    Init,
    Initials,
    GameOver,
    Mappings,
    Input,
    SelectProfile,
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
            Self::EditProfile => "ScreenEditProfile",
            Self::Init => "ScreenInit",
            Self::Initials => "ScreenNameEntryTraditional",
            Self::GameOver => "ScreenGameOver",
            Self::Mappings => "ScreenMapControllers",
            Self::Input => "ScreenTestInput",
            Self::SelectProfile => "ScreenSelectProfile",
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
    }
}

#[inline(always)]
pub(crate) fn progress_percent_tenths(done: usize, total: usize) -> u32 {
    if total == 0 {
        return 0;
    }
    (((done.min(total) as u128) * 1000) / total as u128) as u32
}

#[inline(always)]
pub(crate) fn progress_count_text(done: usize, total: usize) -> String {
    let pct = progress_percent_tenths(done, total);
    format!("{done}/{total} ({}.{:01}%)", pct / 10, pct % 10)
}
