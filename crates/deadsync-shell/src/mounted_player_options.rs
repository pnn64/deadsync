//! Startup-selected Player Options presentation and its application handoff.

use crate::openitg::{Effect, host_effect, host_screen, screen_for_host};
use crate::{ActiveTheme, ThemeScreen as Screen};
use deadlib_assets::AssetManager;
use deadlib_platform::input::RawKeyboardEvent;
use deadlib_present::actors::Actor;
use deadsync_chart::SongData;
use deadsync_config::theme::ThemeFlag;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_input::InputEvent;
use deadsync_profile_gameplay::stage_options::{PlayerOptionsView, StageOptions};
use deadsync_theme::ThemeTransitions;
use deadsync_theme::player_options::{
    PlayerOptionsEntry, PlayerOptionsSelection, ThemePlayerOptions,
};
use deadsync_theme_openitg::{OpenItgScreen, OpenItgTheme};
use deadsync_theme_simply_love::{SimplyLoveEffect, views::SimplyLoveVisualPolicyView};
use std::sync::Arc;

pub(crate) type SimplyLovePlayerOptionsState =
    <ActiveTheme as ThemePlayerOptions>::PlayerOptionsState;

pub(crate) enum PlayerOptionsState {
    SimplyLove(SimplyLovePlayerOptionsState),
    OpenItg {
        state: <OpenItgTheme as ThemePlayerOptions>::PlayerOptionsState,
        effects: Vec<Effect>,
    },
}

impl PlayerOptionsState {
    pub(crate) fn new(
        theme: ThemeFlag,
        entry: PlayerOptionsEntry<Screen, PlayerOptionsView>,
    ) -> Self {
        match theme {
            ThemeFlag::SimplyLove => Self::SimplyLove(ActiveTheme::init_player_options(entry)),
            ThemeFlag::OpenItg => Self::OpenItg {
                state: OpenItgTheme::init_player_options(PlayerOptionsEntry {
                    song: entry.song,
                    steps: entry.steps,
                    preferred: entry.preferred,
                    color_index: entry.color_index,
                    return_screen: screen_for_host(entry.return_screen),
                    fixed_stepchart_label: entry.fixed_stepchart_label,
                    noteskin_catalog: entry.noteskin_catalog,
                    smx_gif_catalog: entry.smx_gif_catalog,
                    heart_rate_devices: entry.heart_rate_devices,
                    view: entry.view,
                    scroll_speeds: entry.scroll_speeds,
                }),
                effects: Vec::new(),
            },
        }
    }

    pub(crate) fn prepare(&mut self, assets: &AssetManager) {
        match self {
            Self::SimplyLove(state) => ActiveTheme::prepare_player_options(state, assets),
            Self::OpenItg { state, .. } => OpenItgTheme::prepare_player_options(state, assets),
        }
    }

    pub(crate) fn selection(&self) -> PlayerOptionsSelection<'_, Screen> {
        match self {
            Self::SimplyLove(state) => ActiveTheme::player_options_selection(state),
            Self::OpenItg { state, .. } => {
                let selection = OpenItgTheme::player_options_selection(state);
                PlayerOptionsSelection {
                    song: selection.song,
                    steps: selection.steps,
                    preferred: selection.preferred,
                    color_index: selection.color_index,
                    return_screen: host_screen(selection.return_screen),
                    fixed_stepchart_label: selection.fixed_stepchart_label,
                    scroll_speeds: selection.scroll_speeds,
                    music_rate: selection.music_rate,
                }
            }
        }
    }

    pub(crate) fn stage(&self) -> StageOptions {
        match self {
            Self::SimplyLove(state) => ActiveTheme::player_options_stage(state),
            Self::OpenItg { state, .. } => OpenItgTheme::player_options_stage(state),
        }
    }

    pub(crate) fn song_mut(&mut self) -> &mut Arc<SongData> {
        match self {
            Self::SimplyLove(state) => ActiveTheme::player_options_song_mut(state),
            Self::OpenItg { state, .. } => OpenItgTheme::player_options_song_mut(state),
        }
    }

    pub(crate) fn update(
        &mut self,
        dt: f32,
        assets: &AssetManager,
        host: &mut Vec<SimplyLoveEffect>,
    ) {
        match self {
            Self::SimplyLove(state) => ActiveTheme::update_player_options(state, dt, assets, host),
            Self::OpenItg { state, effects } => {
                OpenItgTheme::update_player_options(state, dt, assets, effects);
                append_effects(effects, host);
            }
        }
    }

    pub(crate) fn handle_input(
        &mut self,
        assets: &AssetManager,
        input: &InputEvent,
        host: &mut Vec<SimplyLoveEffect>,
    ) {
        match self {
            Self::SimplyLove(state) => {
                ActiveTheme::handle_player_options_input(state, assets, input, host)
            }
            Self::OpenItg { state, effects } => {
                OpenItgTheme::handle_player_options_input(state, assets, input, effects);
                append_effects(effects, host);
            }
        }
    }

    pub(crate) fn handle_raw_key(
        &mut self,
        key: Option<&RawKeyboardEvent>,
        text: Option<&str>,
        ctrl: bool,
        host: &mut Vec<SimplyLoveEffect>,
    ) -> bool {
        match self {
            Self::SimplyLove(state) => {
                ActiveTheme::handle_player_options_raw_key(state, key, text, ctrl, host)
            }
            Self::OpenItg { state, effects } => {
                let consumed =
                    OpenItgTheme::handle_player_options_raw_key(state, key, text, ctrl, effects);
                append_effects(effects, host);
                consumed
            }
        }
    }

    pub(crate) fn push_actors(
        &self,
        actors: &mut Vec<Actor>,
        assets: &AssetManager,
        policy: SimplyLoveVisualPolicyView,
    ) {
        match self {
            Self::SimplyLove(state) => {
                ActiveTheme::push_player_options_actors(actors, state, assets, policy)
            }
            Self::OpenItg { state, .. } => {
                OpenItgTheme::push_player_options_actors(actors, state, assets, ())
            }
        }
    }

    pub(crate) fn pad_light_preview(&self) -> [Option<u8>; MAX_PLAYERS] {
        match self {
            Self::SimplyLove(state) => ActiveTheme::player_options_pad_light_preview(state),
            Self::OpenItg { state, .. } => OpenItgTheme::player_options_pad_light_preview(state),
        }
    }

    pub(crate) fn simply_love_mut(&mut self) -> Option<&mut SimplyLovePlayerOptionsState> {
        match self {
            Self::SimplyLove(state) => Some(state),
            Self::OpenItg { .. } => None,
        }
    }

    pub(crate) fn entry_transition(&self) -> (Vec<Actor>, f32) {
        match self {
            Self::SimplyLove(_) => ActiveTheme::in_transition(Screen::PlayerOptions, None),
            Self::OpenItg { .. } => OpenItgTheme::in_transition(OpenItgScreen::PlayerOptions, ()),
        }
    }

    pub(crate) fn exit_transition(&self, target: Screen) -> (Vec<Actor>, f32) {
        match self {
            Self::SimplyLove(_) => ActiveTheme::out_transition(Screen::PlayerOptions, target),
            Self::OpenItg { .. } => {
                OpenItgTheme::out_transition(OpenItgScreen::PlayerOptions, screen_for_host(target))
            }
        }
    }
}

fn append_effects(effects: &mut Vec<Effect>, host: &mut Vec<SimplyLoveEffect>) {
    for effect in effects.drain(..) {
        host_effect(effect).append_to(host);
    }
}
