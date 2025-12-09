pub mod gameplay;
pub mod menu;
pub mod options;
pub mod init;
pub mod select_color;
pub mod mappings;
pub mod input;
pub mod select_music;
pub mod sandbox;
pub mod evaluation;
pub mod player_options;
use std::path::PathBuf;

use crate::core::gfx::BackendType;
use crate::config::DisplayMode;
use crate::game::chart::ChartData;
#[derive(Debug, Clone, PartialEq)]
pub enum ScreenAction {
    None,
    Navigate(Screen),
    Exit,
    RequestBanner(Option<PathBuf>),
    RequestDensityGraph(Option<ChartData>),
    FetchOnlineGrade(String),
    ChangeRenderer(BackendType),
    ChangeGraphics {
        renderer: Option<BackendType>,
        display_mode: Option<DisplayMode>,
        resolution: Option<(u32, u32)>,
    },
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
