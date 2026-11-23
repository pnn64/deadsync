//! Presentation interface for editing a stage's player options.

use crate::views::{NoteskinCatalogView, SmxGifCatalogView};
use crate::{ThemeEffect, ThemeTransitions};
use deadlib_assets::AssetManager;
use deadsync_chart::SongData;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_rules::scroll::ScrollSpeedSetting;
use std::sync::Arc;

pub struct PlayerOptionsEntry<Screen, View> {
    pub song: Arc<SongData>,
    pub steps: [usize; MAX_PLAYERS],
    pub preferred: [usize; MAX_PLAYERS],
    pub color_index: i32,
    pub return_screen: Screen,
    pub fixed_stepchart_label: Option<String>,
    pub noteskin_catalog: NoteskinCatalogView,
    pub smx_gif_catalog: SmxGifCatalogView,
    pub heart_rate_devices: HeartRateDevicesView,
    pub view: View,
    /// Restore the exact effective speed when reopening a submitted stage.
    pub scroll_speeds: Option<[ScrollSpeedSetting; MAX_PLAYERS]>,
}

/// Read-only selection facts for navigation and stage submission.
pub struct PlayerOptionsSelection<'a, Screen> {
    pub song: &'a Arc<SongData>,
    pub steps: [usize; MAX_PLAYERS],
    pub preferred: [usize; MAX_PLAYERS],
    pub color_index: i32,
    pub return_screen: Screen,
    pub fixed_stepchart_label: Option<&'a str>,
    pub scroll_speeds: [ScrollSpeedSetting; MAX_PLAYERS],
    pub music_rate: f32,
}

/// Prepared rows and player focus borrowed for one Player Options frame.
pub struct PlayerOptionsPage<'a, T, A> {
    pub title: &'static str,
    pub active: [bool; MAX_PLAYERS],
    pub selected_rows: [usize; MAX_PLAYERS],
    /// Each seat's position in the selectable row order, including Exit.
    pub scroll_position: [f32; MAX_PLAYERS],
    pub row_count: usize,
    pub visible_slots: usize,
    pub alpha: f32,
    pub row: &'a dyn Fn(usize) -> Option<PlayerOptionsRow<'a, T>>,
    pub help: &'a dyn Fn(usize, usize) -> Option<&'a Arc<str>>,
    /// Draw the row's live samples at the given screen-space player centers and Y.
    pub draw_preview: &'a dyn Fn(&mut Vec<A>, usize, [f32; MAX_PLAYERS], f32),
}

/// The mounted layout's scrolling window and row movement.
#[derive(Clone, Copy)]
pub struct PlayerOptionsViewport {
    pub slots: u8,
    pub separate_exit: bool,
    pub row_tween_seconds: f32,
}

pub struct PlayerOptionsRow<'a, T> {
    pub label: &'a T,
    pub detail: Option<&'a T>,
    pub slot: f32,
    pub slots: usize,
    pub alpha: f32,
    pub exit: bool,
    pub values: [Option<&'a T>; MAX_PLAYERS],
    pub next_row: [bool; MAX_PLAYERS],
    pub inline: Option<PlayerOptionsInline<'a, T>>,
    pub has_preview: bool,
}

pub struct PlayerOptionsInline<'a, T> {
    pub choices: &'a [T],
    pub focused: [Option<usize>; MAX_PLAYERS],
    pub selected: [usize; MAX_PLAYERS],
    /// Choice-index masks for rows that allow multiple simultaneous selections.
    pub enabled: [Option<u16>; MAX_PLAYERS],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeartRateDeviceView {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeartRateReadingView {
    pub configured: bool,
    pub connected: bool,
    pub bpm: Option<u16>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeartRateDevicesView {
    pub supported: bool,
    pub scanning: bool,
    pub devices: Vec<HeartRateDeviceView>,
    pub error: Option<String>,
    pub readings: [HeartRateReadingView; MAX_PLAYERS],
}

/// Prepare the focused selection before other visible icons and nearby choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NoteskinPreviewPriority {
    Focused,
    Visible,
    Nearby,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteskinPreviewRequest {
    pub name: Arc<str>,
    pub parts: u16,
    pub priority: NoteskinPreviewPriority,
}

pub trait ThemePlayerOptions: ThemeTransitions {
    type PlayerOptionsState;
    type PlayerOptionsView;
    type PlayerOptionsStage;
    type PlayerOptionsInput;
    type PlayerOptionsRawKey;
    type PlayerOptionsVisualPolicy;
    type PlayerOptionsNoteskin;

    fn init_player_options(
        entry: PlayerOptionsEntry<Self::Screen, Self::PlayerOptionsView>,
    ) -> Self::PlayerOptionsState;
    fn player_options_selection(
        state: &Self::PlayerOptionsState,
    ) -> PlayerOptionsSelection<'_, Self::Screen>;
    fn player_options_stage(state: &Self::PlayerOptionsState) -> Self::PlayerOptionsStage;
    fn player_options_song_mut(state: &mut Self::PlayerOptionsState) -> &mut Arc<SongData>;
    fn prepare_player_options(state: &mut Self::PlayerOptionsState, assets: &AssetManager);
    fn update_player_options(
        state: &mut Self::PlayerOptionsState,
        dt: f32,
        assets: &AssetManager,
        effects: &mut Vec<ThemeEffect<Self::Screen, Self::RuntimeRequest>>,
    );
    fn handle_player_options_input(
        state: &mut Self::PlayerOptionsState,
        assets: &AssetManager,
        input: &Self::PlayerOptionsInput,
        effects: &mut Vec<ThemeEffect<Self::Screen, Self::RuntimeRequest>>,
    );
    fn handle_player_options_raw_key(
        state: &mut Self::PlayerOptionsState,
        key: Option<&Self::PlayerOptionsRawKey>,
        text: Option<&str>,
        ctrl_held: bool,
        effects: &mut Vec<ThemeEffect<Self::Screen, Self::RuntimeRequest>>,
    ) -> bool;
    fn push_player_options_actors(
        actors: &mut Vec<Self::Actor>,
        state: &Self::PlayerOptionsState,
        assets: &AssetManager,
        visual_policy: Self::PlayerOptionsVisualPolicy,
    );

    fn sync_player_options_heart_rate(
        _state: &mut Self::PlayerOptionsState,
        _devices: &HeartRateDevicesView,
    ) {
    }
    fn player_options_pad_light_preview(
        _state: &Self::PlayerOptionsState,
    ) -> [Option<u8>; MAX_PLAYERS] {
        [None; MAX_PLAYERS]
    }

    fn take_player_options_noteskin_requests(
        _state: &mut Self::PlayerOptionsState,
        requests: &mut Vec<NoteskinPreviewRequest>,
    ) -> Option<usize> {
        requests.clear();
        None
    }
    fn retain_player_options_noteskin_previews(
        _state: &mut Self::PlayerOptionsState,
        _requests: &[NoteskinPreviewRequest],
    ) {
    }
    fn set_player_options_noteskin_preview(
        _state: &mut Self::PlayerOptionsState,
        _name: &str,
        _parts: u16,
        _skin: Option<Arc<Self::PlayerOptionsNoteskin>>,
    ) {
    }
    fn player_options_noteskin_textures(
        _skin: &Self::PlayerOptionsNoteskin,
        _parts: u16,
    ) -> Vec<(Arc<str>, bool)> {
        Vec::new()
    }
}
