//! Startup-selected Player Options layout and shared editor handoff.

use crate::{ActiveTheme, ThemeScreen as Screen};
use deadlib_assets::AssetManager;
use deadlib_platform::input::RawKeyboardEvent;
use deadlib_present::actors::{Actor, TextContent};
use deadsync_assets::noteskin::Noteskin;
use deadsync_chart::SongData;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_input::InputEvent;
use deadsync_profile_gameplay::stage_options::{PlayerOptionsView, StageOptions};
use deadsync_theme::ThemeTransitions;
use deadsync_theme::player_options::{
    HeartRateDevicesView, NoteskinPreviewRequest, PlayerOptionsEntry, PlayerOptionsLayout,
    PlayerOptionsSelection, ThemePlayerOptions,
};
use deadsync_theme_simply_love::{SimplyLoveEffect, views::SimplyLoveVisualPolicyView};
use std::sync::Arc;

type EditorState = <ActiveTheme as ThemePlayerOptions>::PlayerOptionsState;
type Layout = Box<dyn PlayerOptionsLayout<Actor, TextContent>>;

pub(crate) type PlayerOptionsLayoutFactory = fn() -> Layout;

pub(crate) struct PlayerOptionsState {
    editor: EditorState,
    layout: Option<Layout>,
}

impl PlayerOptionsState {
    pub(crate) fn new(
        entry: PlayerOptionsEntry<Screen, PlayerOptionsView>,
        layout: Option<PlayerOptionsLayoutFactory>,
    ) -> Self {
        let mut editor = ActiveTheme::init_player_options(entry);
        let layout = layout.map(|mount| mount());
        if let Some(layout) = &layout {
            ActiveTheme::set_player_options_viewport(&mut editor, layout.viewport());
        }
        Self { editor, layout }
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
        if let Some(layout) = &mut self.layout {
            ActiveTheme::with_player_options_page(actors, &self.editor, |actors, page| {
                layout.push_page(actors, page, assets, now);
            });
        } else {
            ActiveTheme::push_player_options_actors(actors, &self.editor, assets, policy);
        }
    }

    pub(crate) fn pad_light_preview(&self) -> [Option<u8>; MAX_PLAYERS] {
        ActiveTheme::player_options_pad_light_preview(&self.editor)
    }

    pub(crate) fn sync_heart_rate(&mut self, devices: &HeartRateDevicesView) {
        ActiveTheme::sync_player_options_heart_rate(&mut self.editor, devices);
    }

    pub(crate) fn take_noteskin_requests(
        &mut self,
        requests: &mut Vec<NoteskinPreviewRequest>,
    ) -> Option<usize> {
        ActiveTheme::take_player_options_noteskin_requests(&mut self.editor, requests)
    }

    pub(crate) fn retain_noteskin_previews(&mut self, requests: &[NoteskinPreviewRequest]) {
        ActiveTheme::retain_player_options_noteskin_previews(&mut self.editor, requests);
    }

    pub(crate) fn set_noteskin_preview(&mut self, name: &str, parts: u16, skin: Option<Arc<Noteskin>>) {
        ActiveTheme::set_player_options_noteskin_preview(&mut self.editor, name, parts, skin);
    }

    pub(crate) fn noteskin_preview_textures(skin: &Noteskin, parts: u16) -> Vec<(Arc<str>, bool)> {
        ActiveTheme::player_options_noteskin_textures(skin, parts)
    }

    #[cfg(test)]
    pub(crate) fn editor_mut(&mut self) -> &mut EditorState {
        &mut self.editor
    }

    pub(crate) fn entry_transition(&self) -> (Vec<Actor>, f32) {
        self.layout.as_ref().map_or_else(
            || ActiveTheme::in_transition(Screen::PlayerOptions, None),
            |layout| layout.entry_transition(),
        )
    }

    pub(crate) fn exit_transition(&self, target: Screen) -> (Vec<Actor>, f32) {
        self.layout.as_ref().map_or_else(
            || ActiveTheme::out_transition(Screen::PlayerOptions, target),
            |layout| layout.exit_transition(target),
        )
    }
}
