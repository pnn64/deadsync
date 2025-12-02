pub mod gameplay;
pub mod menu;
pub mod options;
pub mod init;
pub mod select_color;
pub mod mappings;
pub mod select_music;
pub mod sandbox;
pub mod evaluation;
pub mod player_options;
use std::path::PathBuf;

use crate::game::chart::ChartData;
#[derive(Debug, Clone, PartialEq)]
pub enum ScreenAction {
    None,
    Navigate(Screen),
    Exit,
    RequestBanner(Option<PathBuf>),
    RequestDensityGraph(Option<ChartData>),
    FetchOnlineGrade(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Gameplay,
    Options,
    Init,
    Mappings,
    SelectColor,
    SelectMusic,
    Sandbox,
    Evaluation,
    PlayerOptions,
}
