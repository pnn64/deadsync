pub mod evaluation;
pub mod gameplay;
pub mod init;
pub mod input;
pub mod mappings;
pub mod menu;
pub mod options;
pub mod player_options;
pub mod profile_load;
pub mod sandbox;
pub mod select_color;
pub mod select_music;
pub mod select_play_mode;
pub mod select_profile;
pub mod select_style;
use std::path::PathBuf;

use crate::assets::DensityGraphSlot;
use crate::config::DisplayMode;
use crate::core::gfx::BackendType;
use crate::game::chart::ChartData;
use crate::game::profile::ActiveProfile;
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
    RequestDensityGraph {
        slot: DensityGraphSlot,
        chart_opt: Option<ChartData>,
    },
    FetchOnlineGrade(String),
    ChangeGraphics {
        renderer: Option<BackendType>,
        display_mode: Option<DisplayMode>,
        monitor: Option<usize>,
        resolution: Option<(u32, u32)>,
    },
    UpdateShowOverlay(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Gameplay,
    Options,
    Init,
    Mappings,
    Input,
    SelectProfile,
    SelectColor,
    SelectStyle,
    SelectPlayMode,
    ProfileLoad,
    SelectMusic,
    Sandbox,
    Evaluation,
    PlayerOptions,
}
