//! Startup-selected Player Options layout and shared editor handoff.

use crate::openitg::screen_for_host;
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
use deadsync_theme_openitg::{OpenItgPlayerOptions, OpenItgScreen, OpenItgTheme};
use deadsync_theme_simply_love::{
    SimplyLoveEffect, screens::player_options, views::SimplyLoveVisualPolicyView,
};
use std::sync::Arc;

pub(crate) type SimplyLovePlayerOptionsState =
    <ActiveTheme as ThemePlayerOptions>::PlayerOptionsState;

pub(crate) struct PlayerOptionsState {
    editor: SimplyLovePlayerOptionsState,
    theme: ThemeFlag,
    openitg: Option<OpenItgPlayerOptions>,
}

impl PlayerOptionsState {
    pub(crate) fn new(
        theme: ThemeFlag,
        entry: PlayerOptionsEntry<Screen, PlayerOptionsView>,
    ) -> Self {
        let mut editor = ActiveTheme::init_player_options(entry);
        let openitg = (theme == ThemeFlag::OpenItg).then(|| {
            player_options::set_viewport(&mut editor, OpenItgPlayerOptions::VIEWPORT);
            OpenItgPlayerOptions::default()
        });
        Self {
            editor,
            theme,
            openitg,
        }
    }

    pub(crate) fn prepare(&mut self, assets: &AssetManager) {
        ActiveTheme::prepare_player_options(&mut self.editor, assets);
    }

    pub(crate) fn selection(&self) -> PlayerOptionsSelection<'_, Screen> {
        ActiveTheme::player_options_selection(&self.editor)
    }

    pub(crate) fn stage(&self) -> StageOptions {
        ActiveTheme::player_options_stage(&self.editor)
    }

    pub(crate) fn song_mut(&mut self) -> &mut Arc<SongData> {
        ActiveTheme::player_options_song_mut(&mut self.editor)
    }

    pub(crate) fn update(
        &mut self,
        dt: f32,
        assets: &AssetManager,
        host: &mut Vec<SimplyLoveEffect>,
    ) {
        ActiveTheme::update_player_options(&mut self.editor, dt, assets, host);
    }

    pub(crate) fn handle_input(
        &mut self,
        assets: &AssetManager,
        input: &InputEvent,
        host: &mut Vec<SimplyLoveEffect>,
    ) {
        ActiveTheme::handle_player_options_input(&mut self.editor, assets, input, host);
    }

    pub(crate) fn handle_raw_key(
        &mut self,
        key: Option<&RawKeyboardEvent>,
        text: Option<&str>,
        ctrl: bool,
        host: &mut Vec<SimplyLoveEffect>,
    ) -> bool {
        ActiveTheme::handle_player_options_raw_key(&mut self.editor, key, text, ctrl, host)
    }

    pub(crate) fn push_actors(
        &mut self,
        actors: &mut Vec<Actor>,
        assets: &AssetManager,
        policy: SimplyLoveVisualPolicyView,
        now: f64,
    ) {
        if let Some(presentation) = &mut self.openitg {
            player_options::push_actors_with_layout(actors, &self.editor, |actors, page| {
                presentation.push_actors(actors, page, assets.fonts(), now)
            });
        } else {
            ActiveTheme::push_player_options_actors(actors, &self.editor, assets, policy);
        }
    }

    pub(crate) fn pad_light_preview(&self) -> [Option<u8>; MAX_PLAYERS] {
        ActiveTheme::player_options_pad_light_preview(&self.editor)
    }

    pub(crate) fn editor_mut(&mut self) -> &mut SimplyLovePlayerOptionsState {
        &mut self.editor
    }

    pub(crate) fn entry_transition(&self) -> (Vec<Actor>, f32) {
        match self.theme {
            ThemeFlag::SimplyLove => ActiveTheme::in_transition(Screen::PlayerOptions, None),
            ThemeFlag::OpenItg => OpenItgTheme::in_transition(OpenItgScreen::PlayerOptions, ()),
        }
    }

    pub(crate) fn exit_transition(&self, target: Screen) -> (Vec<Actor>, f32) {
        match self.theme {
            ThemeFlag::SimplyLove => ActiveTheme::out_transition(Screen::PlayerOptions, target),
            ThemeFlag::OpenItg => {
                OpenItgTheme::out_transition(OpenItgScreen::PlayerOptions, screen_for_host(target))
            }
        }
    }
}
