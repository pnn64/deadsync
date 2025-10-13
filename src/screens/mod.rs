pub mod gameplay;
pub mod menu;
pub mod options;
pub mod init;
pub mod select_color;
pub mod select_music;
pub mod sandbox;
pub mod evaluation;
use std::path::PathBuf;

use crate::gameplay::chart::ChartData;
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
    SelectColor,
    SelectMusic,
    Sandbox,
    Evaluation,
}