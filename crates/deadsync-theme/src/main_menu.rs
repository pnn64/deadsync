//! Presentation interface for the title menu.

use crate::{ThemeEffect, ThemeInputResult, ThemeTransitions, TransitionState};
use std::sync::Arc;

/// Host actions available from the title screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuItem {
    Play,
    Options,
    Content,
    Exit,
    Shutdown,
}

/// Arcade title screens expose only Play. Home menus expose the host actions.
pub const fn main_menu_items(home: bool, allow_shutdown: bool) -> &'static [MainMenuItem] {
    use MainMenuItem::*;
    if !home {
        &[Play]
    } else if allow_shutdown {
        &[Play, Options, Content, Exit, Shutdown]
    } else {
        &[Play, Options, Content, Exit]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuGrooveError {
    Disabled,
    MachineOffline,
    CannotConnect,
    TimedOut,
    InvalidResponse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuGrooveStatus {
    Pending { boogie: bool },
    Error { boogie: bool, kind: MainMenuGrooveError },
    Connected {
        boogie: bool,
        get_scores: bool,
        leaderboard: bool,
        auto_submit: bool,
    },
}

impl Default for MainMenuGrooveStatus {
    fn default() -> Self {
        Self::Pending { boogie: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuArrowCloudError {
    Disabled,
    TimedOut,
    HostBlocked,
    CannotConnect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MainMenuArrowCloudStatus {
    #[default]
    Pending,
    Connected,
    Error(MainMenuArrowCloudError),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MainMenuSmxConflictView {
    pub color_rgb: [f32; 3],
}

/// Game and service facts used to present the menu.
#[derive(Clone, Debug, PartialEq)]
pub struct MainMenuRuntimeView<Game, CoinMode> {
    pub game: Game,
    pub coin_mode: CoinMode,
    pub event_mode: bool,
    pub credits: u32,
    pub can_start: bool,
    pub allow_shutdown_host: bool,
    pub dedicated_three_key_nav: bool,
    pub song_count: usize,
    pub pack_count: usize,
    pub course_count: usize,
    pub local_ip: Option<Arc<str>>,
    pub groovestats: MainMenuGrooveStatus,
    pub arrowcloud: MainMenuArrowCloudStatus,
    pub smx_conflict: Option<MainMenuSmxConflictView>,
}

impl<Game: Default, CoinMode: Default> Default for MainMenuRuntimeView<Game, CoinMode> {
    fn default() -> Self {
        Self {
            game: Game::default(),
            coin_mode: CoinMode::default(),
            event_mode: true,
            credits: 0,
            can_start: true,
            allow_shutdown_host: false,
            dedicated_three_key_nav: false,
            song_count: 0,
            pack_count: 0,
            course_count: 0,
            local_ip: None,
            groovestats: MainMenuGrooveStatus::default(),
            arrowcloud: MainMenuArrowCloudStatus::default(),
            smx_conflict: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuPhase {
    Active,
    Entering,
}

/// Looping movie to use behind a title menu when video backgrounds are enabled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuBackgroundMovie {
    pub path: &'static str,
    pub rate: f32,
}

pub struct MainMenuFrame<'a, Screen, Policy> {
    pub update_banner_tag: Option<&'a str>,
    /// A decoded background frame, when the configured movie is visible.
    pub background_key: Option<&'a Arc<str>>,
    pub alpha_multiplier: f32,
    pub transition: &'a TransitionState<Screen>,
    pub visual_policy: Policy,
}

/// A theme's retained title menu and presentation hooks.
pub trait ThemeMainMenu: ThemeTransitions {
    type MenuState;
    type MenuRuntimeView;
    type MenuInput;
    type MenuRawKey;
    type MenuVisualPolicy;

    const MENU_BACKGROUND_MOVIE: Option<MenuBackgroundMovie> = None;

    fn init_menu(color_index: i32) -> Self::MenuState;
    fn menu_color_index(state: &Self::MenuState) -> i32;
    fn set_menu_color_index(state: &mut Self::MenuState, color_index: i32);

    fn reset_menu(state: &mut Self::MenuState) {
        *state = Self::init_menu(Self::menu_color_index(state));
    }

    fn sync_menu_runtime(state: &mut Self::MenuState, view: Self::MenuRuntimeView);
    fn update_menu(
        state: &mut Self::MenuState,
        delta_time: f32,
        phase: MainMenuPhase,
    ) -> ThemeEffect<Self::Screen, Self::RuntimeRequest>;
    fn handle_menu_input(
        state: &mut Self::MenuState,
        input: &Self::MenuInput,
    ) -> ThemeEffect<Self::Screen, Self::RuntimeRequest>;
    fn handle_menu_raw_key(
        _state: &mut Self::MenuState,
        _key: &Self::MenuRawKey,
    ) -> ThemeInputResult<Self::Screen, Self::RuntimeRequest> {
        ThemeInputResult::ignored()
    }
    fn push_menu_actors(
        actors: &mut Vec<Self::Actor>,
        state: &Self::MenuState,
        frame: MainMenuFrame<'_, Self::Screen, Self::MenuVisualPolicy>,
    );
    fn push_menu_exit_overlay(
        _actors: &mut Vec<Self::Actor>,
        _state: &Self::MenuState,
        _transition: &TransitionState<Self::Screen>,
        _visual_policy: Self::MenuVisualPolicy,
    ) {}
}
