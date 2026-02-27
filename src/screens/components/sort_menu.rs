use crate::act;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::network::{self, ConnectionStatus};
use crate::core::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::profile;
use crate::game::scores;
use crate::game::song::SongData;
use crate::screens::select_music::MusicWheelEntry;
use crate::ui::actors::Actor;
use crate::ui::color;
use std::sync::Arc;

const WIDTH: f32 = 210.0;
const HEIGHT: f32 = 160.0;
const HEADER_Y_OFFSET: f32 = -92.0;
const ITEM_SPACING: f32 = 36.0;
const ITEM_TOP_Y_OFFSET: f32 = -15.0;
const ITEM_BOTTOM_Y_OFFSET: f32 = 10.0;
const TOP_TEXT_BASE_ZOOM: f32 = 1.15;
const BOTTOM_TEXT_BASE_ZOOM: f32 = 0.85;
const UNFOCUSED_ROW_ZOOM: f32 = 0.5;
const FOCUSED_ROW_ZOOM: f32 = 0.6;
const DIM_ALPHA: f32 = 0.8;
const HINT_Y_OFFSET: f32 = 100.0;
const HINT_TEXT: &str = "PRESS &SELECT; TO CANCEL";
const WHEEL_SLOTS: usize = 7;
const WHEEL_FOCUS_SLOT: usize = WHEEL_SLOTS / 2;
const VISIBLE_ROWS: usize = WHEEL_SLOTS - 2;
const FONT_TOP: &str = "miso";
const FONT_BOTTOM: &str = "wendy";

pub const FOCUS_TWEEN_SECONDS: f32 = 0.15;
pub const SONG_SEARCH_FOCUS_TWEEN_SECONDS: f32 = 0.1;
pub const SONG_SEARCH_INPUT_LOCK_SECONDS: f32 = 0.25;
pub const REPLAY_FOCUS_TWEEN_SECONDS: f32 = 0.1;
pub const REPLAY_INPUT_LOCK_SECONDS: f32 = 0.15;

const SONG_SEARCH_PROMPT_TITLE: &str = "Song Search";
const SONG_SEARCH_PROMPT_HINT: &str = "'pack/song' format will search for songs in specific packs\n'[###]' format will search for BPMs/Difficulties";
const SONG_SEARCH_PROMPT_MAX_LEN: usize = 30;
const SONG_SEARCH_TEXT_ENTRY_W: f32 = 620.0;
const SONG_SEARCH_TEXT_ENTRY_H: f32 = 190.0;
const SONG_SEARCH_TEXT_ENTRY_CURSOR_PERIOD: f32 = 1.0;
const SONG_SEARCH_TEXT_ENTRY_FOOTER: &str = "START/ENTER: SEARCH    BACK/SELECT/ESC: CANCEL";
const SONG_SEARCH_PANE_W: f32 = 319.0;
const SONG_SEARCH_PANE_H: f32 = 319.0;
const SONG_SEARCH_PANE_BORDER: f32 = 2.0;
const SONG_SEARCH_TEXT_H: f32 = 15.0;
const SONG_SEARCH_ROW_SPACING: f32 = 30.0;
const SONG_SEARCH_WHEEL_SLOTS: usize = 12;
const SONG_SEARCH_WHEEL_FOCUS_SLOT: usize = SONG_SEARCH_WHEEL_SLOTS / 2 - 1;

// Simply Love ScreenSelectMusic overlay/Leaderboard.lua geometry.
const GS_LEADERBOARD_NUM_ENTRIES: usize = 13;
const GS_LEADERBOARD_ROW_HEIGHT: f32 = 24.0;
const GS_LEADERBOARD_PANE_HEIGHT: f32 = 360.0;
const GS_LEADERBOARD_PANE_WIDTH_SINGLE: f32 = 330.0;
const GS_LEADERBOARD_PANE_WIDTH_MULTI: f32 = 230.0;
const GS_LEADERBOARD_PANE_SIDE_OFFSET: f32 = 160.0;
const GS_LEADERBOARD_PANE_CENTER_Y: f32 = -15.0;
const GS_LEADERBOARD_DIM_ALPHA: f32 = 0.875;
const GS_LEADERBOARD_Z: i16 = 1480;
const GS_LEADERBOARD_ERROR_TIMEOUT: &str = "Timed Out";
const GS_LEADERBOARD_ERROR_FAILED: &str = "Failed to Load 😞";
const GS_LEADERBOARD_DISABLED_TEXT: &str = "Disabled";
const GS_LEADERBOARD_NO_SCORES_TEXT: &str = "No Scores";
const GS_LEADERBOARD_LOADING_TEXT: &str = "Loading ...";
const GS_LEADERBOARD_MACHINE_BEST: &str = "Machine's  Best";
const GS_LEADERBOARD_MORE_TEXT: &str = "More Leaderboards";
const GS_LEADERBOARD_CLOSE_HINT: &str = "Press &START; to dismiss.";
const GS_LEADERBOARD_RIVAL_COLOR: [f32; 4] = color::rgba_hex("#BD94FF");
const GS_LEADERBOARD_SELF_COLOR: [f32; 4] = color::rgba_hex("#A1FF94");
const SORTS_INACTIVE_COLOR: [f32; 4] = color::rgba_hex("#005D7F");
const SORTS_ACTIVE_COLOR: [f32; 4] = color::rgba_hex("#0030A8");
const REPLAY_MAX_ENTRIES: usize = 1024;

#[derive(Clone, Debug)]
pub struct SongSearchCandidate {
    pub pack_name: String,
    pub song: Arc<SongData>,
}

#[derive(Clone, Debug)]
pub struct SongSearchResultsState {
    pub search_text: String,
    pub candidates: Vec<SongSearchCandidate>,
    pub selected_index: usize,
    pub prev_selected_index: usize,
    pub focus_anim_elapsed: f32,
    pub input_lock: f32,
}

#[derive(Clone, Debug)]
pub struct SongSearchTextEntryState {
    pub query: String,
    pub blink_t: f32,
}

#[derive(Clone, Debug)]
pub enum SongSearchState {
    Hidden,
    TextEntry(SongSearchTextEntryState),
    Results(SongSearchResultsState),
}

#[derive(Default)]
struct SongSearchFilter {
    pack_term: Option<String>,
    song_term: Option<String>,
    difficulty: Option<u8>,
    bpm_tier: Option<i32>,
}

#[derive(Clone, Debug, Default)]
pub struct LeaderboardSideState {
    joined: bool,
    loading: bool,
    panes: Vec<scores::LeaderboardPane>,
    pane_index: usize,
    show_icons: bool,
    error_text: Option<String>,
    machine_pane: Option<scores::LeaderboardPane>,
    chart_hash: Option<String>,
}

#[derive(Debug)]
pub struct LeaderboardOverlayStateData {
    elapsed: f32,
    p1: LeaderboardSideState,
    p2: LeaderboardSideState,
}

#[derive(Debug)]
pub enum LeaderboardOverlayState {
    Hidden,
    Visible(LeaderboardOverlayStateData),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaderboardInputOutcome {
    None,
    ChangedPane,
    Closed,
}

#[derive(Clone, Debug)]
pub struct ReplayOverlayStateData {
    pub entries: Vec<scores::MachineReplayEntry>,
    pub selected_index: usize,
    pub prev_selected_index: usize,
    pub focus_anim_elapsed: f32,
    pub input_lock: f32,
}

#[derive(Clone, Debug)]
pub enum ReplayOverlayState {
    Hidden,
    Visible(ReplayOverlayStateData),
}

#[derive(Clone, Debug)]
pub enum ReplayInputOutcome {
    None,
    ChangedSelection,
    Closed,
    StartGameplay(ReplayStartPayload),
}

#[derive(Clone, Debug)]
pub struct ReplayStartPayload {
    pub replay: Vec<scores::ReplayEdge>,
    pub name: String,
    pub score: f64,
    pub replay_beat0_time_seconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    OpenSorts,
    BackToMain,
    SortByGroup,
    SortByTitle,
    SortByArtist,
    SortByBpm,
    SortByLength,
    SortByMeter,
    SortByPopularity,
    SortByRecent,
    SwitchToSingle,
    SwitchToDouble,
    TestInput,
    SongSearch,
    SwitchProfile,
    ReloadSongsCourses,
    PlayReplay,
    ShowLeaderboard,
}

#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub top_label: &'static str,
    pub bottom_label: &'static str,
    pub action: Action,
}

const ITEM_CATEGORY_SORTS: Item = Item {
    top_label: "",
    bottom_label: "SORTS...",
    action: Action::OpenSorts,
};
const ITEM_SORT_BY_GROUP: Item = Item {
    top_label: "Sort By",
    bottom_label: "Group",
    action: Action::SortByGroup,
};
const ITEM_SORT_BY_TITLE: Item = Item {
    top_label: "Sort By",
    bottom_label: "Title",
    action: Action::SortByTitle,
};
const ITEM_SORT_BY_ARTIST: Item = Item {
    top_label: "Sort By",
    bottom_label: "Artist",
    action: Action::SortByArtist,
};
const ITEM_SORT_BY_BPM: Item = Item {
    top_label: "Sort By",
    bottom_label: "BPM",
    action: Action::SortByBpm,
};
const ITEM_SORT_BY_LENGTH: Item = Item {
    top_label: "Sort By",
    bottom_label: "Length",
    action: Action::SortByLength,
};
const ITEM_SORT_BY_METER: Item = Item {
    top_label: "Sort By",
    bottom_label: "Level",
    action: Action::SortByMeter,
};
const ITEM_SORT_BY_POPULARITY: Item = Item {
    top_label: "Sort By",
    bottom_label: "Most Popular",
    action: Action::SortByPopularity,
};
const ITEM_SORT_BY_RECENT: Item = Item {
    top_label: "Sort By",
    bottom_label: "Recently Played",
    action: Action::SortByRecent,
};
const ITEM_SWITCH_TO_SINGLE: Item = Item {
    top_label: "Change Style To",
    bottom_label: "Single",
    action: Action::SwitchToSingle,
};
const ITEM_SWITCH_TO_DOUBLE: Item = Item {
    top_label: "Change Style To",
    bottom_label: "Double",
    action: Action::SwitchToDouble,
};
const ITEM_TEST_INPUT: Item = Item {
    top_label: "Feeling salty?",
    bottom_label: "Test Input",
    action: Action::TestInput,
};
const ITEM_SONG_SEARCH: Item = Item {
    top_label: "Wherefore Art Thou?",
    bottom_label: "Song Search",
    action: Action::SongSearch,
};
const ITEM_SWITCH_PROFILE: Item = Item {
    top_label: "Next Please",
    bottom_label: "Switch Profile",
    action: Action::SwitchProfile,
};
const ITEM_RELOAD_SONGS_COURSES: Item = Item {
    top_label: "Take a Breather~",
    bottom_label: "Load New Songs",
    action: Action::ReloadSongsCourses,
};
const ITEM_PLAY_REPLAY: Item = Item {
    top_label: "Machine Data",
    bottom_label: "Play Replay",
    action: Action::PlayReplay,
};
const ITEM_SHOW_LEADERBOARD: Item = Item {
    top_label: "GrooveStats",
    bottom_label: "Leaderboard",
    action: Action::ShowLeaderboard,
};
const ITEM_GO_BACK: Item = Item {
    top_label: "Options",
    bottom_label: "Go Back",
    action: Action::BackToMain,
};

pub const ITEMS_MAIN: [Item; 7] = [
    ITEM_CATEGORY_SORTS,
    ITEM_TEST_INPUT,
    ITEM_SONG_SEARCH,
    ITEM_SWITCH_PROFILE,
    ITEM_RELOAD_SONGS_COURSES,
    ITEM_PLAY_REPLAY,
    ITEM_SHOW_LEADERBOARD,
];

pub const ITEMS_MAIN_WITH_SWITCH_TO_SINGLE: [Item; 8] = [
    ITEM_CATEGORY_SORTS,
    ITEM_SWITCH_TO_SINGLE,
    ITEM_TEST_INPUT,
    ITEM_SONG_SEARCH,
    ITEM_SWITCH_PROFILE,
    ITEM_RELOAD_SONGS_COURSES,
    ITEM_PLAY_REPLAY,
    ITEM_SHOW_LEADERBOARD,
];

pub const ITEMS_MAIN_WITH_SWITCH_TO_DOUBLE: [Item; 8] = [
    ITEM_CATEGORY_SORTS,
    ITEM_SWITCH_TO_DOUBLE,
    ITEM_TEST_INPUT,
    ITEM_SONG_SEARCH,
    ITEM_SWITCH_PROFILE,
    ITEM_RELOAD_SONGS_COURSES,
    ITEM_PLAY_REPLAY,
    ITEM_SHOW_LEADERBOARD,
];

pub const ITEMS_SORTS: [Item; 9] = [
    ITEM_SORT_BY_GROUP,
    ITEM_SORT_BY_TITLE,
    ITEM_SORT_BY_ARTIST,
    ITEM_SORT_BY_BPM,
    ITEM_SORT_BY_LENGTH,
    ITEM_SORT_BY_METER,
    ITEM_SORT_BY_POPULARITY,
    ITEM_SORT_BY_RECENT,
    ITEM_GO_BACK,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Sorts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Hidden,
    Visible { page: Page, selected_index: usize },
}

#[inline(always)]
pub fn scroll_dir(len: usize, prev: usize, selected: usize) -> isize {
    if len <= 1 {
        return 0;
    }
    let prev = prev % len;
    let selected = selected % len;
    if selected == (prev + 1) % len {
        1
    } else if prev == (selected + 1) % len {
        -1
    } else {
        0
    }
}

#[inline(always)]
fn set_text_clip_rect(actor: &mut Actor, rect: [f32; 4]) {
    if let Actor::Text { clip, .. } = actor {
        *clip = Some(rect);
    }
}

#[inline(always)]
pub fn begin_song_search_prompt() -> SongSearchState {
    SongSearchState::TextEntry(SongSearchTextEntryState {
        query: String::new(),
        blink_t: 0.0,
    })
}

pub fn begin_song_search_results(
    group_entries: &[MusicWheelEntry],
    search_text: String,
) -> SongSearchState {
    let trimmed = search_text.trim().to_string();
    if trimmed.is_empty() {
        return SongSearchState::Hidden;
    }
    let candidates = build_song_search_candidates(group_entries, &trimmed);
    SongSearchState::Results(SongSearchResultsState {
        search_text: trimmed,
        candidates,
        selected_index: 0,
        prev_selected_index: 0,
        focus_anim_elapsed: SONG_SEARCH_FOCUS_TWEEN_SECONDS,
        input_lock: SONG_SEARCH_INPUT_LOCK_SECONDS,
    })
}

pub fn update_song_search(state: &mut SongSearchState, dt: f32) -> bool {
    let dt = dt.max(0.0);
    match state {
        SongSearchState::Hidden => false,
        SongSearchState::TextEntry(entry) => {
            entry.blink_t = (entry.blink_t + dt) % SONG_SEARCH_TEXT_ENTRY_CURSOR_PERIOD;
            true
        }
        SongSearchState::Results(results) => {
            results.input_lock = (results.input_lock - dt).max(0.0);
            if results.focus_anim_elapsed < SONG_SEARCH_FOCUS_TWEEN_SECONDS {
                results.focus_anim_elapsed =
                    (results.focus_anim_elapsed + dt).min(SONG_SEARCH_FOCUS_TWEEN_SECONDS);
            }
            true
        }
    }
}

pub fn song_search_add_text(entry: &mut SongSearchTextEntryState, text: &str) {
    let mut len = entry.query.chars().count();
    for ch in text.chars() {
        if ch.is_control() {
            continue;
        }
        if len >= SONG_SEARCH_PROMPT_MAX_LEN {
            break;
        }
        entry.query.push(ch);
        len += 1;
    }
}

#[inline(always)]
pub fn song_search_backspace(entry: &mut SongSearchTextEntryState) {
    let _ = entry.query.pop();
}

#[inline(always)]
pub fn song_search_total_items(results: &SongSearchResultsState) -> usize {
    results.candidates.len() + 1
}

pub fn song_search_move(results: &mut SongSearchResultsState, delta: isize) -> bool {
    let len = song_search_total_items(results);
    if len == 0 || delta == 0 {
        return false;
    }
    let old = results.selected_index.min(len - 1);
    let next = ((old as isize + delta).rem_euclid(len as isize)) as usize;
    if next == old {
        return false;
    }
    results.prev_selected_index = old;
    results.selected_index = next;
    results.focus_anim_elapsed = 0.0;
    true
}

#[inline(always)]
pub fn song_search_focused_candidate(
    results: &SongSearchResultsState,
) -> Option<&SongSearchCandidate> {
    results.candidates.get(results.selected_index)
}

pub fn build_song_search_overlay(
    state: &SongSearchState,
    active_color_index: i32,
) -> Option<Vec<Actor>> {
    let mut actors = Vec::new();
    if matches!(state, SongSearchState::Hidden) {
        return None;
    }

    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.8):
        z(1450)
    ));

    match state {
        SongSearchState::Hidden => {}
        SongSearchState::TextEntry(entry) => {
            let cx = screen_center_x();
            let cy = screen_center_y();
            let panel_w = SONG_SEARCH_TEXT_ENTRY_W.min(screen_width() * 0.9);
            let panel_h = SONG_SEARCH_TEXT_ENTRY_H;
            let cursor = if entry.blink_t < SONG_SEARCH_TEXT_ENTRY_CURSOR_PERIOD * 0.5 {
                "▮"
            } else {
                " "
            };
            let mut value = entry.query.clone();
            if value.chars().count() < SONG_SEARCH_PROMPT_MAX_LEN {
                value.push_str(cursor);
            }
            let query_text = format!("> {value}");

            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(cx, cy):
                zoomto(panel_w + 2.0, panel_h + 2.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(1451)
            ));
            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(cx, cy):
                zoomto(panel_w, panel_h):
                diffuse(0.12, 0.12, 0.12, 1.0):
                z(1452)
            ));
            actors.push(act!(text:
                font("wendy"):
                settext(SONG_SEARCH_PROMPT_TITLE):
                align(0.5, 0.5):
                xy(cx, cy - panel_h * 0.5 + 22.0):
                zoom(0.42):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(1453):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(SONG_SEARCH_PROMPT_HINT):
                align(0.5, 0.5):
                xy(cx, cy - 28.0):
                zoom(0.78):
                maxwidth(panel_w - 40.0):
                diffuse(0.8, 0.8, 0.8, 1.0):
                z(1453):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(query_text):
                align(0.5, 0.5):
                xy(cx, cy + 30.0):
                zoom(1.05):
                maxwidth(panel_w - 36.0):
                diffuse(0.4, 1.0, 0.4, 1.0):
                z(1453):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(SONG_SEARCH_TEXT_ENTRY_FOOTER):
                align(0.5, 0.5):
                xy(cx, cy + panel_h * 0.5 - 16.0):
                zoom(0.78):
                diffuse(0.75, 0.75, 0.75, 1.0):
                z(1453):
                horizalign(center)
            ));
        }
        SongSearchState::Results(results) => {
            let pane_cx = screen_center_x();
            let pane_cy = screen_center_y() + 40.0;
            let list_base_y = pane_cy - SONG_SEARCH_PANE_H * 0.5 - SONG_SEARCH_TEXT_H * 2.5;
            let list_x = pane_cx - SONG_SEARCH_PANE_W * 0.25;
            let list_clip = [
                pane_cx - SONG_SEARCH_PANE_W * 0.5,
                pane_cy - SONG_SEARCH_PANE_H * 0.5,
                SONG_SEARCH_PANE_W * 0.5,
                SONG_SEARCH_PANE_H,
            ];
            let selected_color = color::simply_love_rgba(active_color_index);
            let total_items = song_search_total_items(results).max(1);
            let focus_t = (results.focus_anim_elapsed / SONG_SEARCH_FOCUS_TWEEN_SECONDS.max(1e-6))
                .clamp(0.0, 1.0);
            let scroll_dir = scroll_dir(
                total_items,
                results.prev_selected_index,
                results.selected_index,
            ) as f32;
            let scroll_shift = scroll_dir
                * [1.0 - focus_t, 0.0]
                    [(results.focus_anim_elapsed >= SONG_SEARCH_FOCUS_TWEEN_SECONDS) as usize];

            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(pane_cx, pane_cy):
                zoomto(SONG_SEARCH_PANE_W + SONG_SEARCH_PANE_BORDER, SONG_SEARCH_PANE_H + SONG_SEARCH_PANE_BORDER):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(1451)
            ));
            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(pane_cx, pane_cy):
                zoomto(SONG_SEARCH_PANE_W, SONG_SEARCH_PANE_H):
                diffuse(0.0, 0.0, 0.0, 1.0):
                z(1452)
            ));
            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(pane_cx, pane_cy):
                zoomto(SONG_SEARCH_PANE_BORDER, SONG_SEARCH_PANE_H - 10.0):
                diffuse(0.2, 0.2, 0.2, 1.0):
                z(1453)
            ));
            actors.push(act!(text:
                font("miso"):
                settext("Search Results For:"):
                align(0.5, 0.5):
                xy(pane_cx, pane_cy - SONG_SEARCH_PANE_H * 0.5 - SONG_SEARCH_TEXT_H * 5.0):
                zoom(0.8):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(1454):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(format!("\"{}\"", results.search_text)):
                align(0.5, 0.5):
                xy(pane_cx, pane_cy - SONG_SEARCH_PANE_H * 0.5 - SONG_SEARCH_TEXT_H * 3.0):
                zoom(0.8):
                maxwidth(SONG_SEARCH_PANE_W):
                diffuse(0.4, 1.0, 0.4, 1.0):
                z(1454):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(format!("{} Results Found", results.candidates.len())):
                align(0.5, 0.5):
                xy(pane_cx, pane_cy - SONG_SEARCH_PANE_H * 0.5 - SONG_SEARCH_TEXT_H):
                zoom(0.8):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(1454):
                horizalign(center)
            ));

            for slot_idx in 0..SONG_SEARCH_WHEEL_SLOTS {
                let offset = slot_idx as isize - SONG_SEARCH_WHEEL_FOCUS_SLOT as isize;
                let row_idx = ((results.selected_index as isize + offset)
                    .rem_euclid(total_items as isize)) as usize;
                let slot_pos = offset as f32 + scroll_shift;
                let y = (slot_pos + SONG_SEARCH_WHEEL_FOCUS_SLOT as f32 + 1.0)
                    .mul_add(SONG_SEARCH_ROW_SPACING, list_base_y);
                let focused = slot_pos.abs() < 0.5;
                let mut text = "Exit".to_string();
                let mut base_rgb = [1.0, 0.2, 0.2];
                if row_idx < results.candidates.len() {
                    let song = &results.candidates[row_idx].song;
                    text = song.display_title(false).to_string();
                    base_rgb = [1.0, 1.0, 1.0];
                }
                let focus_tint = if focused {
                    [selected_color[0], selected_color[1], selected_color[2]]
                } else {
                    [0.533, 0.533, 0.533]
                };
                let mut color_rgba = [
                    base_rgb[0] * focus_tint[0],
                    base_rgb[1] * focus_tint[1],
                    base_rgb[2] * focus_tint[2],
                    1.0,
                ];
                let alpha =
                    [0.0, 1.0][(slot_idx > 0 && slot_idx + 1 < SONG_SEARCH_WHEEL_SLOTS) as usize];
                color_rgba[3] *= alpha;
                let mut row = act!(text:
                    font("miso"):
                    settext(text):
                    align(0.5, 0.5):
                    xy(list_x, y):
                    maxwidth(155.0):
                    zoom(1.0):
                    diffuse(color_rgba[0], color_rgba[1], color_rgba[2], color_rgba[3]):
                    z(1454):
                    horizalign(center)
                );
                set_text_clip_rect(&mut row, list_clip);
                actors.push(row);
            }

            if let Some(candidate) = song_search_focused_candidate(results) {
                let chart_type = profile::get_session_play_style().chart_type();
                let details = [
                    ("Pack", candidate.pack_name.clone()),
                    ("Song", candidate.song.display_title(false).to_string()),
                    (
                        "Subtitle",
                        candidate.song.display_subtitle(false).to_string(),
                    ),
                    ("BPMs", candidate.song.formatted_display_bpm()),
                    (
                        "Difficulties",
                        song_search_difficulties_text(candidate.song.as_ref(), chart_type),
                    ),
                ];
                for (i, (label, value)) in details.iter().enumerate() {
                    let zoom = 0.8;
                    let row_i = i as f32;
                    let label_row = row_i * 2.0 + 1.0;
                    let value_row = row_i * 2.0 + 2.0;
                    let label_y = pane_cy - SONG_SEARCH_PANE_H * 0.5
                        + SONG_SEARCH_TEXT_H * zoom * label_row
                        + 8.0 * label_row;
                    let value_y = pane_cy - SONG_SEARCH_PANE_H * 0.5
                        + SONG_SEARCH_TEXT_H * zoom * value_row
                        + 8.0 * value_row;
                    actors.push(act!(text:
                        font("miso"):
                        settext(format!("{label}:")):
                        align(0.0, 0.5):
                        xy(pane_cx + 10.0, label_y):
                        zoom(zoom):
                        maxwidth(145.0 / zoom):
                        diffuse(0.67, 0.67, 1.0, 1.0):
                        z(1454):
                        horizalign(left)
                    ));
                    actors.push(act!(text:
                        font("miso"):
                        settext(value):
                        align(0.0, 0.5):
                        xy(pane_cx + 40.0, value_y):
                        zoom(zoom):
                        maxwidth(115.0 / zoom):
                        diffuse(1.0, 1.0, 1.0, 1.0):
                        z(1454):
                        horizalign(left)
                    ));
                }
            }
        }
    }

    Some(actors)
}

#[inline(always)]
fn song_search_bpm_tier(bpm: f64) -> i32 {
    (((bpm + 0.5) / 10.0).floor() as i32) * 10
}

fn song_search_display_bpm_range(song: &SongData) -> Option<(f64, f64)> {
    song.display_bpm_range()
}

fn song_search_difficulties_text(song: &SongData, chart_type: &str) -> String {
    const ORDER: [&str; 5] = ["beginner", "easy", "medium", "hard", "challenge"];
    let mut out = String::new();
    for diff in ORDER {
        if let Some(chart) = song.charts.iter().find(|c| {
            c.chart_type.eq_ignore_ascii_case(chart_type) && c.difficulty.eq_ignore_ascii_case(diff)
        }) {
            if !out.is_empty() {
                out.push_str("   ");
            }
            out.push_str(&chart.meter.to_string());
        }
    }
    if out.is_empty() { "-".to_string() } else { out }
}

fn parse_song_search_filter(input: &str) -> SongSearchFilter {
    let lower = input.to_ascii_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut filter = SongSearchFilter::default();
    let mut stripped = String::with_capacity(lower.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '[' {
            let mut j = i + 1;
            let mut value: u32 = 0;
            let mut has_digit = false;
            while j < chars.len() {
                let Some(d) = chars[j].to_digit(10) else {
                    break;
                };
                has_digit = true;
                value = value.saturating_mul(10).saturating_add(d);
                j += 1;
            }
            if has_digit && j < chars.len() && chars[j] == ']' {
                if value <= 35 {
                    filter.difficulty = Some(value as u8);
                } else {
                    filter.bpm_tier = Some(song_search_bpm_tier(value as f64));
                }
                i = j + 1;
                continue;
            }
        }
        stripped.push(chars[i]);
        i += 1;
    }

    let stripped = stripped.trim();
    if let Some((left, right)) = stripped.split_once('/') {
        if !left.is_empty() {
            filter.pack_term = Some(left.to_string());
        }
        if !right.is_empty() {
            filter.song_term = Some(right.to_string());
        }
    } else if !stripped.is_empty() {
        filter.song_term = Some(stripped.to_string());
    }
    filter
}

fn build_song_search_candidates(
    group_entries: &[MusicWheelEntry],
    search_text: &str,
) -> Vec<SongSearchCandidate> {
    let filter = parse_song_search_filter(search_text);
    let chart_type = profile::get_session_play_style().chart_type();
    let mut out = Vec::new();
    let mut current_pack_name: Option<&str> = None;

    for entry in group_entries {
        match entry {
            MusicWheelEntry::PackHeader { name, .. } => {
                current_pack_name = Some(name.as_str());
            }
            MusicWheelEntry::Song(song) => {
                if !song
                    .charts
                    .iter()
                    .any(|c| c.chart_type.eq_ignore_ascii_case(chart_type))
                {
                    continue;
                }

                let pack_name = current_pack_name.unwrap_or_default();
                if let Some(pack_term) = &filter.pack_term
                    && !pack_name.to_ascii_lowercase().contains(pack_term)
                {
                    continue;
                }

                if let Some(song_term) = &filter.song_term {
                    let display = song.display_full_title(false).to_ascii_lowercase();
                    let translit = song.display_full_title(true).to_ascii_lowercase();
                    if !display.contains(song_term) && !translit.contains(song_term) {
                        continue;
                    }
                }

                if let Some(diff) = filter.difficulty
                    && !song.charts.iter().any(|c| {
                        c.chart_type.eq_ignore_ascii_case(chart_type)
                            && !c.difficulty.eq_ignore_ascii_case("edit")
                            && c.meter == diff as u32
                    })
                {
                    continue;
                }

                if let Some(want_tier) = filter.bpm_tier {
                    let Some((bpm_lo, bpm_hi)) = song_search_display_bpm_range(song) else {
                        continue;
                    };
                    let mut lo = song_search_bpm_tier(bpm_lo);
                    let mut hi = song_search_bpm_tier(bpm_hi);
                    if lo > hi {
                        std::mem::swap(&mut lo, &mut hi);
                    }
                    if lo == hi {
                        if want_tier != lo {
                            continue;
                        }
                    } else if want_tier < lo || want_tier > hi {
                        continue;
                    }
                }

                out.push(SongSearchCandidate {
                    pack_name: pack_name.to_string(),
                    song: song.clone(),
                });
            }
        }
    }

    out
}

fn gs_machine_pane(chart_hash: Option<&str>) -> scores::LeaderboardPane {
    let entries = chart_hash
        .map(|h| scores::get_machine_leaderboard_local(h, GS_LEADERBOARD_NUM_ENTRIES))
        .unwrap_or_default();
    scores::LeaderboardPane {
        name: GS_LEADERBOARD_MACHINE_BEST.to_string(),
        entries,
        is_ex: false,
        disabled: false,
    }
}

fn gs_disabled_pane() -> scores::LeaderboardPane {
    scores::LeaderboardPane {
        name: "GrooveStats".to_string(),
        entries: Vec::new(),
        is_ex: false,
        disabled: true,
    }
}

fn gs_error_text(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        GS_LEADERBOARD_ERROR_TIMEOUT.to_string()
    } else {
        GS_LEADERBOARD_ERROR_FAILED.to_string()
    }
}

fn apply_leaderboard_side_snapshot(
    side: &mut LeaderboardSideState,
    snapshot: scores::CachedPlayerLeaderboardData,
) {
    let current_pane = side
        .panes
        .get(side.pane_index)
        .map(|pane| (pane.name.clone(), pane.is_ex, pane.disabled));

    if snapshot.loading {
        side.loading = true;
        side.error_text = None;
        side.show_icons = false;
        return;
    }

    side.loading = false;
    if let Some(error) = snapshot.error {
        side.error_text = Some(gs_error_text(&error));
        if side.panes.is_empty()
            && let Some(machine) = side.machine_pane.clone()
        {
            side.panes.push(machine);
        }
        side.pane_index = side.pane_index.min(side.panes.len().saturating_sub(1));
        side.show_icons = false;
        return;
    }

    let mut panes = snapshot.data.map_or_else(Vec::new, |data| data.panes);
    if let Some(machine) = side.machine_pane.clone() {
        panes.push(machine);
    }
    if panes.is_empty()
        && let Some(machine) = side.machine_pane.clone()
    {
        panes.push(machine);
    }

    side.error_text = None;
    if let Some((name, is_ex, disabled)) = current_pane {
        side.pane_index = panes
            .iter()
            .position(|pane| pane.name == name && pane.is_ex == is_ex && pane.disabled == disabled)
            .unwrap_or(side.pane_index.min(panes.len().saturating_sub(1)));
    } else {
        side.pane_index = 0;
    }
    side.show_icons = panes.len() > 1;
    side.panes = panes;
}

fn refresh_leaderboard_side_from_cache(
    side: &mut LeaderboardSideState,
    player: profile::PlayerSide,
) {
    let Some(chart_hash) = side.chart_hash.as_deref() else {
        return;
    };
    let Some(snapshot) = scores::get_or_fetch_player_leaderboards_for_side(
        chart_hash,
        player,
        GS_LEADERBOARD_NUM_ENTRIES,
    ) else {
        side.loading = false;
        side.error_text = None;
        if side.panes.is_empty()
            && let Some(machine) = side.machine_pane.clone()
        {
            side.panes.push(machine);
        }
        side.show_icons = false;
        return;
    };
    apply_leaderboard_side_snapshot(side, snapshot);
}

pub fn show_leaderboard_overlay(
    chart_hash_p1: Option<String>,
    chart_hash_p2: Option<String>,
) -> Option<LeaderboardOverlayState> {
    let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
    let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
    if !p1_joined && !p2_joined {
        return None;
    }

    let mut p1 = LeaderboardSideState {
        joined: p1_joined,
        machine_pane: Some(gs_machine_pane(chart_hash_p1.as_deref())),
        ..Default::default()
    };
    let mut p2 = LeaderboardSideState {
        joined: p2_joined,
        machine_pane: Some(gs_machine_pane(chart_hash_p2.as_deref())),
        ..Default::default()
    };

    let status = network::get_status();
    let service = matches!(
        &status,
        ConnectionStatus::Connected(services) if services.leaderboard
    );
    let service_disabled = matches!(
        &status,
        ConnectionStatus::Connected(services) if !services.leaderboard
    );

    if p1_joined {
        let profile = profile::get_for_side(profile::PlayerSide::P1);
        if service && !profile.groovestats_api_key.is_empty() && chart_hash_p1.is_some() {
            p1.chart_hash = chart_hash_p1;
            refresh_leaderboard_side_from_cache(&mut p1, profile::PlayerSide::P1);
        } else if let Some(machine) = p1.machine_pane.clone() {
            p1.panes.push(machine);
            if service_disabled {
                p1.panes.push(gs_disabled_pane());
            }
            p1.show_icons = false;
        }
    }

    if p2_joined {
        let profile = profile::get_for_side(profile::PlayerSide::P2);
        if service && !profile.groovestats_api_key.is_empty() && chart_hash_p2.is_some() {
            p2.chart_hash = chart_hash_p2;
            refresh_leaderboard_side_from_cache(&mut p2, profile::PlayerSide::P2);
        } else if let Some(machine) = p2.machine_pane.clone() {
            p2.panes.push(machine);
            if service_disabled {
                p2.panes.push(gs_disabled_pane());
            }
            p2.show_icons = false;
        }
    }

    Some(LeaderboardOverlayState::Visible(
        LeaderboardOverlayStateData {
            elapsed: 0.0,
            p1,
            p2,
        },
    ))
}

#[inline(always)]
pub fn hide_leaderboard_overlay(state: &mut LeaderboardOverlayState) {
    *state = LeaderboardOverlayState::Hidden;
}

pub fn update_leaderboard_overlay(state: &mut LeaderboardOverlayState, dt: f32) {
    let LeaderboardOverlayState::Visible(overlay) = state else {
        return;
    };
    overlay.elapsed += dt.max(0.0);
    if overlay.p1.joined && overlay.p1.chart_hash.is_some() {
        refresh_leaderboard_side_from_cache(&mut overlay.p1, profile::PlayerSide::P1);
    }
    if overlay.p2.joined && overlay.p2.chart_hash.is_some() {
        refresh_leaderboard_side_from_cache(&mut overlay.p2, profile::PlayerSide::P2);
    }
}

#[inline(always)]
fn leaderboard_shift(side: &mut LeaderboardSideState, delta: isize) -> bool {
    if side.loading || side.error_text.is_some() || side.panes.len() <= 1 {
        return false;
    }
    let prev = side.pane_index;
    let len = side.panes.len() as isize;
    side.pane_index = ((side.pane_index as isize + delta).rem_euclid(len)) as usize;
    side.pane_index != prev
}

pub fn handle_leaderboard_input(
    state: &mut LeaderboardOverlayState,
    ev: &InputEvent,
) -> LeaderboardInputOutcome {
    if !ev.pressed {
        return LeaderboardInputOutcome::None;
    }
    let LeaderboardOverlayState::Visible(overlay) = state else {
        return LeaderboardInputOutcome::None;
    };

    match ev.action {
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            if overlay.p1.joined && leaderboard_shift(&mut overlay.p1, -1) {
                return LeaderboardInputOutcome::ChangedPane;
            }
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            if overlay.p1.joined && leaderboard_shift(&mut overlay.p1, 1) {
                return LeaderboardInputOutcome::ChangedPane;
            }
        }
        VirtualAction::p2_left | VirtualAction::p2_menu_left => {
            if overlay.p2.joined && leaderboard_shift(&mut overlay.p2, -1) {
                return LeaderboardInputOutcome::ChangedPane;
            }
        }
        VirtualAction::p2_right | VirtualAction::p2_menu_right => {
            if overlay.p2.joined && leaderboard_shift(&mut overlay.p2, 1) {
                return LeaderboardInputOutcome::ChangedPane;
            }
        }
        VirtualAction::p1_start
        | VirtualAction::p2_start
        | VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            hide_leaderboard_overlay(state);
            return LeaderboardInputOutcome::Closed;
        }
        _ => {}
    }

    LeaderboardInputOutcome::None
}

fn format_groovestats_date(date: &str) -> String {
    let trimmed = date.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let ymd = trimmed.split_once(' ').map_or(trimmed, |(value, _)| value);
    let ymd = ymd.split_once('T').map_or(ymd, |(value, _)| value);
    let mut parts = ymd.split('-');
    let (Some(year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next()) else {
        return trimmed.to_string();
    };
    let month_txt = match month {
        "01" => "Jan",
        "02" => "Feb",
        "03" => "Mar",
        "04" => "Apr",
        "05" => "May",
        "06" => "Jun",
        "07" => "Jul",
        "08" => "Aug",
        "09" => "Sep",
        "10" => "Oct",
        "11" => "Nov",
        "12" => "Dec",
        _ => return trimmed.to_string(),
    };
    let day_num = day.parse::<u32>().unwrap_or(0);
    if day_num == 0 {
        return trimmed.to_string();
    }
    format!("{month_txt} {day_num}, {year}")
}

#[inline(always)]
fn leaderboard_icon_bounce_offset(elapsed: f32, dir: f32) -> f32 {
    let t = elapsed.rem_euclid(1.0);
    let phase = if t < 0.5 {
        let u = t / 0.5;
        1.0 - (1.0 - u) * (1.0 - u)
    } else {
        let u = (t - 0.5) / 0.5;
        1.0 - u * u
    };
    dir * 10.0 * phase
}

pub fn build_leaderboard_overlay(state: &LeaderboardOverlayState) -> Option<Vec<Actor>> {
    let LeaderboardOverlayState::Visible(overlay) = state else {
        return None;
    };

    let mut actors = Vec::new();
    let overlay_elapsed = overlay.elapsed;
    let joined_count = overlay.p1.joined as usize + overlay.p2.joined as usize;
    let pane_width = if joined_count <= 1 {
        GS_LEADERBOARD_PANE_WIDTH_SINGLE
    } else {
        GS_LEADERBOARD_PANE_WIDTH_MULTI
    };
    let show_date = joined_count <= 1;
    let pane_cy = screen_center_y() + GS_LEADERBOARD_PANE_CENTER_Y;
    let row_center = (GS_LEADERBOARD_NUM_ENTRIES as f32 + 1.0) * 0.5;

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, GS_LEADERBOARD_DIM_ALPHA):
        z(GS_LEADERBOARD_Z)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(GS_LEADERBOARD_CLOSE_HINT):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_height() - 50.0):
        zoom(1.1):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 1):
        horizalign(center)
    ));

    let mut draw_panel = |side: &LeaderboardSideState, center_x: f32| {
        let pane = side
            .panes
            .get(side.pane_index.min(side.panes.len().saturating_sub(1)));
        let header_text = if side.loading {
            "GrooveStats".to_string()
        } else if let Some(p) = pane {
            p.name.replace("ITL Online", "ITL")
        } else {
            "GrooveStats".to_string()
        };
        let show_ex = !side.loading
            && side.error_text.is_none()
            && pane.is_some_and(|p| p.is_ex && !p.disabled);
        let show_hard_ex = !side.loading
            && side.error_text.is_none()
            && pane.is_some_and(|p| p.is_hard_ex() && !p.disabled);
        let is_disabled = !side.loading && pane.is_some_and(|p| p.disabled);

        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(center_x, pane_cy):
            zoomto(pane_width + 2.0, GS_LEADERBOARD_PANE_HEIGHT + 2.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GS_LEADERBOARD_Z + 2)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(center_x, pane_cy):
            zoomto(pane_width, GS_LEADERBOARD_PANE_HEIGHT):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(GS_LEADERBOARD_Z + 3)
        ));

        let header_y = pane_cy - GS_LEADERBOARD_PANE_HEIGHT * 0.5 + GS_LEADERBOARD_ROW_HEIGHT * 0.5;
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(center_x, header_y):
            zoomto(pane_width + 2.0, GS_LEADERBOARD_ROW_HEIGHT + 2.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GS_LEADERBOARD_Z + 4)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(center_x, header_y):
            zoomto(pane_width, GS_LEADERBOARD_ROW_HEIGHT):
            diffuse(0.0, 0.0, 1.0, 1.0):
            z(GS_LEADERBOARD_Z + 5)
        ));
        actors.push(act!(text:
            font("wendy"):
            settext(header_text):
            align(0.5, 0.5):
            xy(center_x, header_y):
            zoom(0.5):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GS_LEADERBOARD_Z + 6):
            horizalign(center)
        ));
        if show_ex {
            actors.push(act!(text:
                font("wendy"):
                settext("EX"):
                align(1.0, 0.5):
                xy(center_x + pane_width * 0.5 - 16.0, header_y):
                zoom(0.5):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(GS_LEADERBOARD_Z + 6):
                horizalign(right)
            ));
        } else if show_hard_ex {
            let hex = color::HARD_EX_SCORE_RGBA;
            actors.push(act!(text:
                font("wendy"):
                settext("H.EX"):
                align(1.0, 0.5):
                xy(center_x + pane_width * 0.5 - 16.0, header_y):
                zoom(0.5):
                diffuse(hex[0], hex[1], hex[2], hex[3]):
                z(GS_LEADERBOARD_Z + 6):
                horizalign(right)
            ));
        }

        let rank_x = center_x - pane_width * 0.5 + 32.0;
        let name_x = center_x - pane_width * 0.5 + 100.0;
        let score_x = if show_date {
            center_x + 63.0
        } else {
            center_x + pane_width * 0.5 - 2.0
        };
        let date_x = center_x + pane_width * 0.5 - 2.0;

        for i in 0..GS_LEADERBOARD_NUM_ENTRIES {
            let y = pane_cy + GS_LEADERBOARD_ROW_HEIGHT * ((i + 1) as f32 - row_center);
            let mut rank = String::new();
            let mut name = String::new();
            let mut score = String::new();
            let mut date = String::new();
            let mut has_highlight = false;
            let mut highlight_rgb = [0.0, 0.0, 0.0];
            let mut rank_col = [1.0, 1.0, 1.0, 1.0];
            let mut name_col = [1.0, 1.0, 1.0, 1.0];
            let mut score_col = if show_hard_ex {
                color::HARD_EX_SCORE_RGBA
            } else {
                [1.0, 1.0, 1.0, 1.0]
            };
            let mut date_col = [1.0, 1.0, 1.0, 1.0];

            if side.loading {
                if i == 0 {
                    name = GS_LEADERBOARD_LOADING_TEXT.to_string();
                }
            } else if let Some(err) = &side.error_text {
                if i == 0 {
                    name = err.clone();
                }
            } else if is_disabled {
                if i == 0 {
                    name = GS_LEADERBOARD_DISABLED_TEXT.to_string();
                }
            } else if let Some(current) = pane {
                if let Some(entry) = current.entries.get(i) {
                    rank = format!("{}.", entry.rank);
                    name = entry.name.clone();
                    score = format!("{:.2}%", entry.score / 100.0);
                    date = format_groovestats_date(&entry.date);

                    if entry.is_rival || entry.is_self {
                        has_highlight = true;
                        if entry.is_rival {
                            highlight_rgb = [
                                GS_LEADERBOARD_RIVAL_COLOR[0],
                                GS_LEADERBOARD_RIVAL_COLOR[1],
                                GS_LEADERBOARD_RIVAL_COLOR[2],
                            ];
                        } else {
                            highlight_rgb = [
                                GS_LEADERBOARD_SELF_COLOR[0],
                                GS_LEADERBOARD_SELF_COLOR[1],
                                GS_LEADERBOARD_SELF_COLOR[2],
                            ];
                        }
                        rank_col = [0.0, 0.0, 0.0, 1.0];
                        name_col = [0.0, 0.0, 0.0, 1.0];
                        score_col = [0.0, 0.0, 0.0, 1.0];
                        date_col = [0.0, 0.0, 0.0, 1.0];
                    }
                    if entry.is_fail {
                        score_col = [1.0, 0.0, 0.0, 1.0];
                    }
                } else if i == 0 && current.entries.is_empty() {
                    name = GS_LEADERBOARD_NO_SCORES_TEXT.to_string();
                }
            }

            if has_highlight {
                actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(center_x, y):
                    zoomto(pane_width, GS_LEADERBOARD_ROW_HEIGHT):
                    diffuse(highlight_rgb[0], highlight_rgb[1], highlight_rgb[2], 1.0):
                    z(GS_LEADERBOARD_Z + 5)
                ));
            }

            actors.push(act!(text:
                font("miso"):
                settext(rank):
                align(1.0, 0.5):
                xy(rank_x, y):
                zoom(0.8):
                maxwidth(30.0):
                diffuse(rank_col[0], rank_col[1], rank_col[2], rank_col[3]):
                z(GS_LEADERBOARD_Z + 7):
                horizalign(right)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(name):
                align(0.5, 0.5):
                xy(name_x, y):
                zoom(0.8):
                maxwidth(130.0):
                diffuse(name_col[0], name_col[1], name_col[2], name_col[3]):
                z(GS_LEADERBOARD_Z + 7):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(score):
                align(1.0, 0.5):
                xy(score_x, y):
                zoom(0.8):
                diffuse(score_col[0], score_col[1], score_col[2], score_col[3]):
                z(GS_LEADERBOARD_Z + 7):
                horizalign(right)
            ));
            if show_date {
                actors.push(act!(text:
                    font("miso"):
                    settext(date):
                    align(1.0, 0.5):
                    xy(date_x, y):
                    zoom(0.8):
                    diffuse(date_col[0], date_col[1], date_col[2], date_col[3]):
                    z(GS_LEADERBOARD_Z + 7):
                    horizalign(right)
                ));
            }
        }

        if !side.loading && side.error_text.is_none() && side.show_icons {
            let icon_y =
                pane_cy + GS_LEADERBOARD_PANE_HEIGHT * 0.5 - GS_LEADERBOARD_ROW_HEIGHT * 0.5;
            let left_dx = leaderboard_icon_bounce_offset(overlay_elapsed, 1.0);
            let right_dx = leaderboard_icon_bounce_offset(overlay_elapsed, -1.0);
            actors.push(act!(text:
                font("miso"):
                settext("&MENULEFT;"):
                align(0.5, 0.5):
                xy(center_x - pane_width * 0.5 + 10.0 + left_dx, icon_y):
                zoom(1.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(GS_LEADERBOARD_Z + 8):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(GS_LEADERBOARD_MORE_TEXT):
                align(0.5, 0.5):
                xy(center_x, icon_y):
                zoom(1.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(GS_LEADERBOARD_Z + 8):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext("&MENURiGHT;"):
                align(0.5, 0.5):
                xy(center_x + pane_width * 0.5 - 10.0 + right_dx, icon_y):
                zoom(1.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(GS_LEADERBOARD_Z + 8):
                horizalign(center)
            ));
        }
    };

    if joined_count <= 1 {
        if overlay.p1.joined {
            draw_panel(&overlay.p1, screen_center_x());
        } else if overlay.p2.joined {
            draw_panel(&overlay.p2, screen_center_x());
        }
    } else {
        draw_panel(
            &overlay.p1,
            screen_center_x() - GS_LEADERBOARD_PANE_SIDE_OFFSET,
        );
        draw_panel(
            &overlay.p2,
            screen_center_x() + GS_LEADERBOARD_PANE_SIDE_OFFSET,
        );
    }

    Some(actors)
}

#[inline(always)]
fn replay_total_items(state: &ReplayOverlayStateData) -> usize {
    state.entries.len() + 1
}

pub fn begin_replay_overlay(chart_hash: &str) -> ReplayOverlayState {
    if chart_hash.trim().is_empty() {
        return ReplayOverlayState::Hidden;
    }
    let entries = scores::get_machine_replays_local(chart_hash, REPLAY_MAX_ENTRIES);
    ReplayOverlayState::Visible(ReplayOverlayStateData {
        entries,
        selected_index: 0,
        prev_selected_index: 0,
        focus_anim_elapsed: REPLAY_FOCUS_TWEEN_SECONDS,
        input_lock: REPLAY_INPUT_LOCK_SECONDS,
    })
}

pub fn update_replay_overlay(state: &mut ReplayOverlayState, dt: f32) -> bool {
    let ReplayOverlayState::Visible(overlay) = state else {
        return false;
    };
    let dt = dt.max(0.0);
    overlay.input_lock = (overlay.input_lock - dt).max(0.0);
    if overlay.focus_anim_elapsed < REPLAY_FOCUS_TWEEN_SECONDS {
        overlay.focus_anim_elapsed =
            (overlay.focus_anim_elapsed + dt).min(REPLAY_FOCUS_TWEEN_SECONDS);
    }
    true
}

pub fn handle_replay_input(state: &mut ReplayOverlayState, ev: &InputEvent) -> ReplayInputOutcome {
    if !ev.pressed {
        return ReplayInputOutcome::None;
    }
    let ReplayOverlayState::Visible(overlay) = state else {
        return ReplayInputOutcome::None;
    };

    if overlay.input_lock > 0.0 {
        return ReplayInputOutcome::None;
    }

    match ev.action {
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => {
            let len = replay_total_items(overlay);
            if len <= 1 {
                return ReplayInputOutcome::None;
            }
            let old = overlay.selected_index.min(len - 1);
            let next = ((old as isize - 1).rem_euclid(len as isize)) as usize;
            if next == old {
                return ReplayInputOutcome::None;
            }
            overlay.prev_selected_index = old;
            overlay.selected_index = next;
            overlay.focus_anim_elapsed = 0.0;
            ReplayInputOutcome::ChangedSelection
        }
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => {
            let len = replay_total_items(overlay);
            if len <= 1 {
                return ReplayInputOutcome::None;
            }
            let old = overlay.selected_index.min(len - 1);
            let next = ((old as isize + 1).rem_euclid(len as isize)) as usize;
            if next == old {
                return ReplayInputOutcome::None;
            }
            overlay.prev_selected_index = old;
            overlay.selected_index = next;
            overlay.focus_anim_elapsed = 0.0;
            ReplayInputOutcome::ChangedSelection
        }
        VirtualAction::p1_start | VirtualAction::p2_start => {
            let selected = overlay
                .selected_index
                .min(replay_total_items(overlay).saturating_sub(1));
            if selected >= overlay.entries.len() {
                *state = ReplayOverlayState::Hidden;
                ReplayInputOutcome::Closed
            } else {
                let (replay, name, score, replay_beat0_time_seconds) = {
                    let entry = &overlay.entries[selected];
                    (
                        entry.replay.clone(),
                        entry.name.clone(),
                        entry.score,
                        entry.replay_beat0_time_seconds,
                    )
                };
                *state = ReplayOverlayState::Hidden;
                ReplayInputOutcome::StartGameplay(ReplayStartPayload {
                    replay,
                    name,
                    score,
                    replay_beat0_time_seconds,
                })
            }
        }
        VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            *state = ReplayOverlayState::Hidden;
            ReplayInputOutcome::Closed
        }
        _ => ReplayInputOutcome::None,
    }
}

pub fn build_replay_overlay(
    state: &ReplayOverlayState,
    active_color_index: i32,
) -> Option<Vec<Actor>> {
    let ReplayOverlayState::Visible(overlay) = state else {
        return None;
    };

    let mut actors = Vec::new();
    let pane_width = GS_LEADERBOARD_PANE_WIDTH_SINGLE;
    let pane_cx = screen_center_x();
    let pane_cy = screen_center_y() + GS_LEADERBOARD_PANE_CENTER_Y;
    let row_center = (GS_LEADERBOARD_NUM_ENTRIES as f32 + 1.0) * 0.5;
    let selected_color = color::simply_love_rgba(active_color_index);
    let total_items = replay_total_items(overlay).max(1);
    let visible_rows = GS_LEADERBOARD_NUM_ENTRIES;
    let window_start = if total_items <= visible_rows {
        0
    } else {
        overlay
            .selected_index
            .saturating_sub(visible_rows / 2)
            .min(total_items - visible_rows)
    };

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, GS_LEADERBOARD_DIM_ALPHA):
        z(GS_LEADERBOARD_Z)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, pane_cy):
        zoomto(pane_width + 2.0, GS_LEADERBOARD_PANE_HEIGHT + 2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 2)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, pane_cy):
        zoomto(pane_width, GS_LEADERBOARD_PANE_HEIGHT):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(GS_LEADERBOARD_Z + 3)
    ));

    let header_y = pane_cy - GS_LEADERBOARD_PANE_HEIGHT * 0.5 + GS_LEADERBOARD_ROW_HEIGHT * 0.5;
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, header_y):
        zoomto(pane_width + 2.0, GS_LEADERBOARD_ROW_HEIGHT + 2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 4)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, header_y):
        zoomto(pane_width, GS_LEADERBOARD_ROW_HEIGHT):
        diffuse(0.0, 0.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 5)
    ));
    actors.push(act!(text:
        font("wendy"):
        settext("Play Replay"):
        align(0.5, 0.5):
        xy(pane_cx, header_y):
        zoom(0.5):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 6):
        horizalign(center)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(format!("{} Local Scores", overlay.entries.len())):
        align(0.5, 0.5):
        xy(pane_cx, pane_cy - GS_LEADERBOARD_PANE_HEIGHT * 0.5 - 24.0):
        zoom(0.8):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 6):
        horizalign(center)
    ));

    let rank_x = pane_cx - pane_width * 0.5 + 32.0;
    let name_x = pane_cx - pane_width * 0.5 + 100.0;
    let score_x = pane_cx + 63.0;
    let date_x = pane_cx + pane_width * 0.5 - 2.0;

    for row_slot in 0..visible_rows {
        let row_idx = window_start + row_slot;
        if row_idx >= total_items {
            break;
        }
        let y = pane_cy + GS_LEADERBOARD_ROW_HEIGHT * ((row_slot + 1) as f32 - row_center);
        let selected = row_idx == overlay.selected_index;
        if selected {
            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(pane_cx, y):
                zoomto(pane_width, GS_LEADERBOARD_ROW_HEIGHT):
                diffuse(selected_color[0], selected_color[1], selected_color[2], 1.0):
                z(GS_LEADERBOARD_Z + 5)
            ));
        }

        let (rank, name, score, date, is_fail, is_exit) = if row_idx < overlay.entries.len() {
            let entry = &overlay.entries[row_idx];
            (
                format!("{}.", entry.rank),
                entry.name.clone(),
                format!("{:.2}%", entry.score / 100.0),
                format_groovestats_date(&entry.date),
                entry.is_fail,
                false,
            )
        } else {
            (
                String::new(),
                "Exit".to_string(),
                String::new(),
                String::new(),
                false,
                true,
            )
        };

        let base = if selected {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        let name_col = if is_exit {
            if selected {
                [0.2, 0.0, 0.0, 1.0]
            } else {
                [1.0, 0.25, 0.25, 1.0]
            }
        } else {
            base
        };
        let score_col = if is_fail && !selected {
            [1.0, 0.0, 0.0, 1.0]
        } else {
            base
        };

        actors.push(act!(text:
            font("miso"):
            settext(rank):
            align(1.0, 0.5):
            xy(rank_x, y):
            zoom(0.8):
            maxwidth(30.0):
            diffuse(base[0], base[1], base[2], base[3]):
            z(GS_LEADERBOARD_Z + 7):
            horizalign(right)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(name):
            align(0.5, 0.5):
            xy(name_x, y):
            zoom(0.8):
            maxwidth(130.0):
            diffuse(name_col[0], name_col[1], name_col[2], name_col[3]):
            z(GS_LEADERBOARD_Z + 7):
            horizalign(center)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(score):
            align(1.0, 0.5):
            xy(score_x, y):
            zoom(0.8):
            diffuse(score_col[0], score_col[1], score_col[2], score_col[3]):
            z(GS_LEADERBOARD_Z + 7):
            horizalign(right)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(date):
            align(1.0, 0.5):
            xy(date_x, y):
            zoom(0.8):
            diffuse(base[0], base[1], base[2], base[3]):
            z(GS_LEADERBOARD_Z + 7):
            horizalign(right)
        ));
    }

    actors.push(act!(text:
        font("miso"):
        settext("START: PLAY REPLAY    BACK/SELECT: CANCEL"):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_height() - 50.0):
        zoom(1.1):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 8):
        horizalign(center)
    ));

    Some(actors)
}

pub struct RenderParams<'a> {
    pub items: &'a [Item],
    pub selected_index: usize,
    pub prev_selected_index: usize,
    pub focus_anim_elapsed: f32,
    pub selected_color: [f32; 4],
}

pub fn build_overlay(p: RenderParams<'_>) -> Vec<Actor> {
    let mut actors = Vec::new();
    let cx = screen_center_x();
    let cy = screen_center_y();
    let clip_rect = [cx - WIDTH * 0.5, cy - HEIGHT * 0.5, WIDTH, HEIGHT];
    let selected_index = p.selected_index.min(p.items.len().saturating_sub(1));

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, DIM_ALPHA):
        z(1450)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy + HEADER_Y_OFFSET):
        zoomto(WIDTH + 2.0, 22.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1451)
    ));
    actors.push(act!(text:
        font(FONT_BOTTOM):
        settext("OPTIONS"):
        align(0.5, 0.5):
        xy(cx, cy + HEADER_Y_OFFSET):
        zoom(0.4):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1452):
        horizalign(center)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy):
        zoomto(WIDTH + 2.0, HEIGHT + 2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1451)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy):
        zoomto(WIDTH, HEIGHT):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1452)
    ));

    if !p.items.is_empty() {
        let focus_t = (p.focus_anim_elapsed / FOCUS_TWEEN_SECONDS.max(1e-6)).clamp(0.0, 1.0);
        let scroll_dir = scroll_dir(
            p.items.len(),
            p.prev_selected_index.min(p.items.len() - 1),
            selected_index,
        ) as f32;
        let scroll_shift = scroll_dir
            * [1.0 - focus_t, 0.0][(p.focus_anim_elapsed >= FOCUS_TWEEN_SECONDS) as usize];
        let selected_rgba = [
            p.selected_color[0],
            p.selected_color[1],
            p.selected_color[2],
            1.0,
        ];
        let mut draw_row = |item_idx: usize, slot_pos: f32| {
            let focus_lerp = (1.0 - slot_pos.abs()).clamp(0.0, 1.0);
            let row_zoom =
                (FOCUSED_ROW_ZOOM - UNFOCUSED_ROW_ZOOM).mul_add(focus_lerp, UNFOCUSED_ROW_ZOOM);
            let row_alpha = (3.0 - slot_pos.abs()).clamp(0.0, 1.0);
            let row_tint = [
                (selected_rgba[0] - 0.533).mul_add(focus_lerp, 0.533),
                (selected_rgba[1] - 0.533).mul_add(focus_lerp, 0.533),
                (selected_rgba[2] - 0.533).mul_add(focus_lerp, 0.533),
            ];
            let top_color = [row_tint[0], row_tint[1], row_tint[2], row_alpha];
            let y = slot_pos.mul_add(ITEM_SPACING, cy);
            let item = &p.items[item_idx];
            let bottom_color = match item.action {
                Action::OpenSorts => [
                    (SORTS_ACTIVE_COLOR[0] - SORTS_INACTIVE_COLOR[0])
                        .mul_add(focus_lerp, SORTS_INACTIVE_COLOR[0]),
                    (SORTS_ACTIVE_COLOR[1] - SORTS_INACTIVE_COLOR[1])
                        .mul_add(focus_lerp, SORTS_INACTIVE_COLOR[1]),
                    (SORTS_ACTIVE_COLOR[2] - SORTS_INACTIVE_COLOR[2])
                        .mul_add(focus_lerp, SORTS_INACTIVE_COLOR[2]),
                    row_alpha,
                ],
                Action::BackToMain => [row_tint[0], 0.0, 0.0, row_alpha],
                _ => [row_tint[0], row_tint[1], row_tint[2], row_alpha],
            };

            let mut top = act!(text:
                font(FONT_TOP):
                settext(item.top_label):
                align(0.5, 0.5):
                xy(cx, y + ITEM_TOP_Y_OFFSET * row_zoom):
                zoom(TOP_TEXT_BASE_ZOOM * row_zoom):
                diffuse(top_color[0], top_color[1], top_color[2], top_color[3]):
                z(1454):
                horizalign(center)
            );
            set_text_clip_rect(&mut top, clip_rect);
            actors.push(top);

            let mut bottom = act!(text:
                font(FONT_BOTTOM):
                settext(item.bottom_label):
                align(0.5, 0.5):
                xy(cx, y + ITEM_BOTTOM_Y_OFFSET * row_zoom):
                maxwidth(405.0):
                zoom(BOTTOM_TEXT_BASE_ZOOM * row_zoom):
                diffuse(
                    bottom_color[0],
                    bottom_color[1],
                    bottom_color[2],
                    bottom_color[3]
                ):
                z(1454):
                horizalign(center)
            );
            set_text_clip_rect(&mut bottom, clip_rect);
            actors.push(bottom);
        };

        if p.items.len() <= VISIBLE_ROWS {
            let span = p.items.len();
            let first_offset = -((span as isize).saturating_sub(1) / 2);
            for i in 0..span {
                let offset = first_offset + i as isize;
                let item_idx = ((selected_index as isize + offset)
                    .rem_euclid(p.items.len() as isize)) as usize;
                let slot_pos = offset as f32 + scroll_shift;
                draw_row(item_idx, slot_pos);
            }
        } else {
            for slot_idx in 0..WHEEL_SLOTS {
                let offset = slot_idx as isize - WHEEL_FOCUS_SLOT as isize;
                let item_idx = ((selected_index as isize + offset)
                    .rem_euclid(p.items.len() as isize)) as usize;
                let slot_pos = offset as f32 + scroll_shift;
                draw_row(item_idx, slot_pos);
            }
        }
    }

    actors.push(act!(text:
        font(FONT_BOTTOM):
        settext(HINT_TEXT):
        align(0.5, 0.5):
        xy(cx, cy + HINT_Y_OFFSET):
        zoom(0.26):
        diffuse(0.7, 0.7, 0.7, 1.0):
        z(1454):
        horizalign(center)
    ));
    actors
}
