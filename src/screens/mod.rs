pub mod evaluation;
pub mod gameplay;
pub mod init;
pub mod input;
pub mod mappings;
pub mod menu;
pub mod options;
pub mod player_options;
pub mod sandbox;
pub mod select_color;
pub mod select_music;
use std::path::PathBuf;

use crate::config::DisplayMode;
use crate::core::gfx::BackendType;
use crate::game::chart::ChartData;
#[derive(Debug, Clone)]
pub enum ScreenAction {
    None,
    Navigate(Screen),
    Exit,
    RequestBanner(Option<PathBuf>),
    RequestDensityGraph(Option<ChartData>),
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
    SelectColor,
    SelectMusic,
    Sandbox,
    Evaluation,
    PlayerOptions,
}
