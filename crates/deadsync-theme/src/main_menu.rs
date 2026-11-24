//! Presentation interface for the title menu.

use crate::{Screen, ThemeEffect, ThemeInputResult, ThemeTransitions, TransitionState, UiCue};
use deadsync_input::{InputEvent, MenuLrChord, VirtualAction};
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

/// Focus and logical menu controls used by titles with a single list of host actions.
#[derive(Default)]
pub struct MainMenuSelection {
    selected: usize,
    dedicated_navigation: bool,
    chords: [MenuLrChord; 2],
}

impl MainMenuSelection {
    pub const fn index(&self) -> usize {
        self.selected
    }

    pub fn sync(&mut self, items: &[MainMenuItem], dedicated_navigation: bool) {
        if self.dedicated_navigation != dedicated_navigation {
            self.chords = Default::default();
            self.dedicated_navigation = dedicated_navigation;
        }
        self.selected = self.selected.min(items.len().saturating_sub(1));
    }

    /// The shell admits Start and records the physical side before forwarding the event.
    pub fn input<R>(
        &mut self,
        event: &InputEvent,
        items: &[MainMenuItem],
    ) -> ThemeEffect<Screen, R> {
        use ThemeEffect as Effect;
        use VirtualAction::*;
        if self.dedicated_navigation {
            let side = match event.action {
                p1_menu_left | p1_menu_right => Some(0),
                p2_menu_left | p2_menu_right => Some(1),
                _ => None,
            };
            if side.is_some_and(|side| self.chords[side].update(event)) {
                return Effect::Navigate(Screen::Init);
            }
        }
        if !event.pressed || items.is_empty() {
            return Effect::None;
        }
        let delta = match event.action {
            p1_start | p2_start => {
                let destination = match items[self.selected] {
                    MainMenuItem::Play => Effect::Navigate(Screen::SelectProfile),
                    MainMenuItem::Options => Effect::Navigate(Screen::Options),
                    MainMenuItem::Content => Effect::Navigate(Screen::ContentBrowser),
                    MainMenuItem::Exit => Effect::Exit,
                    MainMenuItem::Shutdown => Effect::Shutdown,
                };
                return Effect::sequence(Effect::UiCue(UiCue::Confirm), destination);
            }
            p1_back | p2_back => return Effect::Navigate(Screen::Init),
            p1_menu_left | p1_menu_up | p2_menu_left | p2_menu_up => -1,
            p1_menu_right | p1_menu_down | p2_menu_right | p2_menu_down => 1,
            p1_left | p1_up | p2_left | p2_up if !self.dedicated_navigation => -1,
            p1_right | p1_down | p2_right | p2_down if !self.dedicated_navigation => 1,
            _ => return Effect::None,
        };
        let next = (self.selected as isize + delta).rem_euclid(items.len() as isize) as usize;
        if self.selected == next {
            return Effect::None;
        }
        self.selected = next;
        Effect::UiCue(UiCue::Change)
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
    /// Coins already inserted toward the next whole credit.
    pub credit_remainder: u8,
    /// Cabinet price of a whole credit, always at least one.
    pub coins_per_credit: u8,
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
            credit_remainder: 0,
            coins_per_credit: 1,
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
    /// Title effects use shared navigation and cues with the host's request type.
    fn update_menu<R>(
        state: &mut Self::MenuState,
        delta_time: f32,
        phase: MainMenuPhase,
    ) -> ThemeEffect<Self::Screen, R>;
    fn handle_menu_input<R>(
        state: &mut Self::MenuState,
        input: &Self::MenuInput,
    ) -> ThemeEffect<Self::Screen, R>;
    fn handle_menu_raw_key<R>(
        _state: &mut Self::MenuState,
        _key: &Self::MenuRawKey,
    ) -> ThemeInputResult<Self::Screen, R> {
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
