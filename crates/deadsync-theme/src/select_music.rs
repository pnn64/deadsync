//! Read-only data for presenting a music wheel and its current selection.

use crate::views::ScoreboxLocalRecords;
use deadsync_chart::ChartData;
use deadsync_core::input::MAX_PLAYERS;
use std::sync::Arc;

/// The centered window includes one retained row beyond each visible edge.
pub const SELECT_MUSIC_WHEEL_ROWS: usize = 17;
pub const SELECT_MUSIC_DIFFICULTY_ROWS: usize = 5;
pub const SELECT_COURSE_TRACK_ROWS: usize = 6;
pub const WHEEL_OUTGOING_BANNERS: usize = 4;

#[derive(Clone, Copy)]
pub struct WheelBannerFade<'a> {
    pub texture: &'a Arc<str>,
    pub elapsed_seconds: f32,
}

#[derive(Clone, Copy)]
pub struct SelectMusicWheelRow<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub artist: &'a str,
    /// Sections carry a song count. A song has no count.
    pub song_count: Option<usize>,
    pub expanded: bool,
    pub action: Option<SelectMusicWheelAction>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMusicWheelAction {
    RevealSong,
    PreviewedSong,
}

#[derive(Clone, Copy)]
pub struct SelectMusicSongView<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub artist: &'a str,
    pub display_bpm: &'a str,
    pub music_length_seconds: f32,
}

#[derive(Clone, Copy)]
pub struct SelectMusicPlayerView<'a> {
    pub joined: bool,
    pub name: &'a str,
    pub chart: Option<&'a ChartData>,
    pub step_index: usize,
    pub local_records: ScoreboxLocalRecords<'a>,
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

/// Active catalog ordering, including temporary player and collection filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMusicSortView {
    Series,
    Group,
    Title,
    Artist,
    Genre,
    Bpm,
    Length,
    Meter,
    Popularity { side: Option<usize> },
    Recent { side: Option<usize> },
    TopGrades { side: Option<usize> },
    Favorites { side: usize },
    Playlist,
}

pub struct SelectMusicView<'a> {
    /// The focused row is the center slot. The window wraps with the game wheel.
    pub rows: [Option<SelectMusicWheelRow<'a>>; SELECT_MUSIC_WHEEL_ROWS],
    pub wheel_offset: f32,
    pub wheel_locked: bool,
    pub sort: SelectMusicSortView,
    /// Preview beat or the browsing clock when no preview is active.
    pub selection_beat: f32,
    pub song: Option<SelectMusicSongView<'a>>,
    /// Prepared media for the current item. Each theme draws its own fallback.
    pub banner_texture: Option<&'a Arc<str>>,
    pub players: [SelectMusicPlayerView<'a>; MAX_PLAYERS],
    pub difficulty_rows: [SelectMusicDifficultyRow<'a>; SELECT_MUSIC_DIFFICULTY_ROWS],
    pub stage_number: Option<usize>,
    pub music_rate: f32,
    pub prompt: SelectMusicPrompt,
}

/// Track and meter data prepared for the current course rating.
#[derive(Clone, Copy)]
pub struct SelectCourseTrack<'a> {
    pub number: usize,
    /// Combined title retained for compact single-line layouts.
    pub title: &'a Arc<str>,
    pub banner_title: &'a Arc<str>,
    pub subtitle: &'a Arc<str>,
    pub artist: &'a Arc<str>,
    pub meter: &'a Arc<str>,
    pub difficulty: &'a str,
}

/// Information about the currently highlighted course and rating.
pub struct SelectCourseItem<'a> {
    pub title: &'a str,
    pub author: &'a str,
    pub description: &'a Arc<str>,
    pub songs: &'a str,
    pub bpm: &'a str,
    pub length: &'a str,
    pub rating: Option<&'a str>,
    pub can_change_rating: bool,
    pub rating_focused: bool,
    pub tracks: [Option<SelectCourseTrack<'a>>; SELECT_COURSE_TRACK_ROWS],
    pub machine_record: Option<[&'a str; 2]>,
    pub player_record: Option<[&'a str; 2]>,
}

/// The course browser uses the same circular wheel window as song selection.
pub struct SelectCourseView<'a> {
    pub rows: [Option<SelectMusicWheelRow<'a>>; SELECT_MUSIC_WHEEL_ROWS],
    pub wheel_offset: f32,
    pub selection_beat: f32,
    pub item: Option<SelectCourseItem<'a>>,
    pub banner_texture: Option<&'a Arc<str>>,
    pub player_names: [Option<&'a str>; MAX_PLAYERS],
    pub music_rate: f32,
    pub prompt: SelectMusicPrompt,
}

/// The player graph to refresh on the music selection screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DensityGraphSlot {
    SelectMusicP1,
    SelectMusicP2,
}
