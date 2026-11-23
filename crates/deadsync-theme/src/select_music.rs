//! Read-only data for presenting a music wheel and its current selection.

use deadsync_chart::ChartData;
use deadsync_core::input::MAX_PLAYERS;
use std::sync::Arc;

pub const SELECT_MUSIC_WHEEL_ROWS: usize = 15;
pub const SELECT_MUSIC_DIFFICULTY_ROWS: usize = 5;

#[derive(Clone, Copy)]
pub struct SelectMusicWheelRow<'a> {
    pub title: &'a str,
    /// Sections carry a song count. A song has no count.
    pub song_count: Option<usize>,
    pub expanded: bool,
}

#[derive(Clone, Copy)]
pub struct SelectMusicSongView<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub artist: &'a str,
    pub display_bpm: &'a str,
}

#[derive(Clone, Copy)]
pub struct SelectMusicPlayerView<'a> {
    pub joined: bool,
    pub name: &'a str,
    pub chart: Option<&'a ChartData>,
    pub step_index: usize,
}

#[derive(Clone, Copy)]
pub struct SelectMusicDifficultyRow<'a> {
    pub step_index: usize,
    pub chart: Option<&'a ChartData>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMusicPrompt {
    Hidden,
    OfferOptions,
    EnteringOptions,
}

pub struct SelectMusicView<'a> {
    /// The focused row is the center slot. The window wraps with the game wheel.
    pub rows: [Option<SelectMusicWheelRow<'a>>; SELECT_MUSIC_WHEEL_ROWS],
    pub wheel_offset: f32,
    pub selection_elapsed: f32,
    pub song: Option<SelectMusicSongView<'a>>,
    /// Prepared media for the current item. Each theme draws its own fallback.
    pub banner_texture: Option<&'a Arc<str>>,
    pub players: [SelectMusicPlayerView<'a>; MAX_PLAYERS],
    pub difficulty_rows: [SelectMusicDifficultyRow<'a>; SELECT_MUSIC_DIFFICULTY_ROWS],
    pub stage_number: Option<usize>,
    pub music_rate: f32,
    pub prompt: SelectMusicPrompt,
}
