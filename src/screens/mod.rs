pub mod components;
pub mod credits;
pub mod evaluation;
pub mod evaluation_summary;
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
pub mod profile_load;
pub mod sandbox;
pub mod select_color;
pub mod select_course;
pub mod select_mode;
pub mod select_music;
pub mod select_profile;
pub mod select_style;
use std::path::PathBuf;

use crate::assets::{DensityGraphSlot, DensityGraphSource};
use crate::config::DisplayMode;
use crate::core::gfx::{BackendType, PresentModePolicy};
use crate::game::profile::ActiveProfile;

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
    ChangeGraphics {
        renderer: Option<BackendType>,
        display_mode: Option<DisplayMode>,
        monitor: Option<usize>,
        resolution: Option<(u32, u32)>,
        vsync: Option<bool>,
        present_mode_policy: Option<PresentModePolicy>,
        max_fps: Option<u16>,
    },
    UpdateShowOverlay(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Gameplay,
    Options,
    Credits,
    ManageLocalProfiles,
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
