use crate::act;
use crate::assets::{AssetManager, DensityGraphSlot, DensityGraphSource};
use crate::core::audio;
use crate::core::gfx::{BlendMode, MeshMode, MeshVertex};
use crate::core::input::{InputEvent, PadDir, VirtualAction};
use crate::core::network::{self, ConnectionStatus};
use crate::core::space::{
    is_wide, screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use crate::game::chart::ChartData;
use crate::game::parsing::simfile as song_loading;
use crate::game::profile;
use crate::game::scores;
use crate::game::song::{SongData, get_song_cache};
use crate::rgba_const;
use crate::screens::components::screen_bar::{
    self, AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::{heart_bg, music_wheel, pad_display};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use crate::ui::font;
use log::info;
use rssp::bpm::parse_bpm_map;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::KeyCode;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.5;
const TRANSITION_OUT_DURATION: f32 = 0.3;

// ITGmania metric: ScreenSelectMusic ShowOptionsMessageSeconds (fallback: 1.5).
const SHOW_OPTIONS_MESSAGE_SECONDS: f32 = 1.5;

// Simply Love BGAnimations/ScreenSelectMusic background.lua white flash overlay.
const SL_BG_FLASH_SLEEP_SECONDS: f32 = 0.6;
const SL_BG_FLASH_FADE_SECONDS: f32 = 0.5;

// Simply Love BGAnimations/ScreenSelectMusic overlay/MusicWheelAnimation.lua
const SL_WHEEL_CASCADE_NUM_VISIBLE_ITEMS: usize = 15;
const SL_WHEEL_CASCADE_DELAY_STEP_SECONDS: f32 = 0.05;
const SL_WHEEL_CASCADE_REVEAL_SECONDS: f32 = 0.1;
const SL_WHEEL_CASCADE_FINAL_ALPHA: f32 = 0.25;
const SL_WHEEL_CASCADE_ROW_Y_UPPER: f32 = 9.0;
const SL_WHEEL_CASCADE_ROW_Y_LOWER: f32 = 25.0;
const SL_WHEEL_CASCADE_Z: i16 = 63;

// Simply Love ScreenSelectMusic out.lua "Entering Options..." timings.
const ENTERING_OPTIONS_FADE_OUT_SECONDS: f32 = 0.125;
const ENTERING_OPTIONS_HIBERNATE_SECONDS: f32 = 0.1;
const ENTERING_OPTIONS_FADE_IN_SECONDS: f32 = 0.125;
const ENTERING_OPTIONS_HOLD_SECONDS: f32 = 1.0;
const ENTERING_OPTIONS_TOTAL_SECONDS: f32 = ENTERING_OPTIONS_FADE_OUT_SECONDS
    + ENTERING_OPTIONS_HIBERNATE_SECONDS
    + ENTERING_OPTIONS_FADE_IN_SECONDS
    + ENTERING_OPTIONS_HOLD_SECONDS;

const PRESS_START_FOR_OPTIONS_TEXT: &str = "Press &START; for options";
const ENTERING_OPTIONS_TEXT: &str = "Entering Options...";

// Simply Love BGAnimations/ScreenSelectMusic overlay/EscapeFromEventMode.lua prompt.
const SL_EXIT_PROMPT_BG_ALPHA: f32 = 0.925;
const SL_EXIT_PROMPT_TEXT: &str = "Do you want to exit this game?";
const SL_EXIT_PROMPT_NO_LABEL: &str = "No";
const SL_EXIT_PROMPT_YES_LABEL: &str = "Yes";
const SL_EXIT_PROMPT_NO_INFO: &str = "Keep playing.";
const SL_EXIT_PROMPT_YES_INFO: &str = "I'm finished.";
const SL_EXIT_PROMPT_CHOICE_Y: f32 = 250.0;
const SL_EXIT_PROMPT_CHOICE_X_OFFSET: f32 = 100.0;
const SL_EXIT_PROMPT_PROMPT_Y_OFFSET: f32 = -70.0;
const SL_EXIT_PROMPT_PROMPT_ZOOM: f32 = 1.3;
const SL_EXIT_PROMPT_LABEL_ZOOM: f32 = 1.1;
const SL_EXIT_PROMPT_INFO_ZOOM: f32 = 0.825;
const SL_EXIT_PROMPT_INFO_Y_OFFSET: f32 = 30.0;
const SL_EXIT_PROMPT_ACTIVE_ZOOM: f32 = 1.1;
const SL_EXIT_PROMPT_INACTIVE_ZOOM: f32 = 0.5;
const SL_EXIT_PROMPT_CHOICE_TWEEN_SECONDS: f32 = 0.1;
const SL_EXIT_PROMPT_CHOICES_DELAY_SECONDS: f32 = 0.0;
const SL_EXIT_PROMPT_CHOICES_FADE_SECONDS: f32 = 0.15;

// --- THEME LAYOUT CONSTANTS ---
const BANNER_NATIVE_WIDTH: f32 = 418.0;
const BANNER_NATIVE_HEIGHT: f32 = 164.0;
rgba_const!(UI_BOX_BG_COLOR, "#1E282F");

// --- Timing & Logic Constants ---
// ITGmania WheelBase::Move() uses `m_TimeBeforeMovingBegins = 1/4.0f` before auto-scrolling.
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(250);
const DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(300);
const PREVIEW_DELAY_SECONDS: f32 = 0.25;
const PREVIEW_FADE_OUT_SECONDS: f64 = 1.5;
const DEFAULT_PREVIEW_LENGTH: f64 = 12.0;

const MUSIC_WHEEL_SWITCH_SECONDS: f32 = 0.10;
const MUSIC_WHEEL_SETTLE_MIN_SPEED: f32 = 0.2;
// ITGmania PrefsManager default: MusicWheelSwitchSpeed=15.
const MUSIC_WHEEL_HOLD_SPIN_SPEED: f32 = 15.0;
// ITGmania WheelBase::MoveSpecific(): if |offset| < 0.25 then one more move for spin-down.
const MUSIC_WHEEL_STOP_SPINDOWN_THRESHOLD: f32 = 0.25;

const CHORD_UP: u8 = 1 << 0;
const CHORD_DOWN: u8 = 1 << 1;
const MENU_CHORD_LEFT: u8 = 1 << 0;
const MENU_CHORD_RIGHT: u8 = 1 << 1;

// Simply Love ScreenSelectMusic SortMenu geometry.
const SL_SORT_MENU_WIDTH: f32 = 210.0;
const SL_SORT_MENU_HEIGHT: f32 = 160.0;
const SL_SORT_MENU_HEADER_Y_OFFSET: f32 = -92.0;
const SL_SORT_MENU_ITEM_SPACING: f32 = 36.0;
const SL_SORT_MENU_ITEM_TOP_Y_OFFSET: f32 = -15.0;
const SL_SORT_MENU_ITEM_BOTTOM_Y_OFFSET: f32 = 10.0;
const SL_SORT_MENU_TOP_TEXT_BASE_ZOOM: f32 = 1.15;
const SL_SORT_MENU_BOTTOM_TEXT_BASE_ZOOM: f32 = 0.85;
const SL_SORT_MENU_UNFOCUSED_ROW_ZOOM: f32 = 0.5;
const SL_SORT_MENU_FOCUSED_ROW_ZOOM: f32 = 0.6;
const SL_SORT_MENU_FOCUS_TWEEN_SECONDS: f32 = 0.15;
const SL_SORT_MENU_DIM_ALPHA: f32 = 0.8;
const SL_SORT_MENU_HINT_Y_OFFSET: f32 = 100.0;
const SL_SORT_MENU_HINT_TEXT: &str = "PRESS &SELECT; TO CANCEL";
const SL_SORT_MENU_WHEEL_SLOTS: usize = 7;
const SL_SORT_MENU_WHEEL_FOCUS_SLOT: usize = SL_SORT_MENU_WHEEL_SLOTS / 2;
const SL_SORT_MENU_VISIBLE_ROWS: usize = SL_SORT_MENU_WHEEL_SLOTS - 2;
const SL_SONG_SEARCH_PROMPT_TITLE: &str = "Song Search";
const SL_SONG_SEARCH_PROMPT_HINT: &str = "'pack/song' format will search for songs in specific packs\n'[###]' format will search for BPMs/Difficulties";
const SL_SONG_SEARCH_PROMPT_MAX_LEN: usize = 30;
const SL_SONG_SEARCH_PANE_W: f32 = 319.0;
const SL_SONG_SEARCH_PANE_H: f32 = 319.0;
const SL_SONG_SEARCH_PANE_BORDER: f32 = 2.0;
const SL_SONG_SEARCH_TEXT_H: f32 = 15.0;
const SL_SONG_SEARCH_ROW_SPACING: f32 = 30.0;
const SL_SONG_SEARCH_WHEEL_SLOTS: usize = 12;
const SL_SONG_SEARCH_WHEEL_FOCUS_SLOT: usize = SL_SONG_SEARCH_WHEEL_SLOTS / 2 - 1;
const SL_SONG_SEARCH_FOCUS_TWEEN_SECONDS: f32 = 0.1;
const SL_SONG_SEARCH_INPUT_LOCK_SECONDS: f32 = 0.25;

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
rgba_const!(GS_LEADERBOARD_RIVAL_COLOR, "#BD94FF");
rgba_const!(GS_LEADERBOARD_SELF_COLOR, "#A1FF94");

// Simply Love [ScreenSelectMusic] [MusicWheel]: RecentSongsToShow=30.
const RECENT_SONGS_TO_SHOW: usize = 30;
const RECENT_SORT_HEADER: &str = "Recently Played";

#[inline(always)]
const fn chord_bit(dir: PadDir) -> u8 {
    match dir {
        PadDir::Up => CHORD_UP,
        PadDir::Down => CHORD_DOWN,
        _ => 0,
    }
}

// --- Preview helpers ---
fn sec_at_beat_from_bpms(normalized_bpms: &str, target_beat: f64) -> f64 {
    if !target_beat.is_finite() || target_beat <= 0.0 {
        return 0.0;
    }
    let mut bpm_map = parse_bpm_map(normalized_bpms);
    if bpm_map.is_empty() {
        bpm_map.push((0.0, 60.0));
    }
    if bpm_map.first().is_none_or(|(b, _)| *b != 0.0) {
        let first_bpm = bpm_map[0].1;
        bpm_map.insert(0, (0.0, first_bpm));
    }
    let mut time = 0.0;
    let mut last_beat = 0.0;
    let mut last_bpm = bpm_map[0].1;
    for &(beat, bpm) in &bpm_map {
        if target_beat <= beat {
            let delta_beats = (target_beat - last_beat).max(0.0);
            if last_bpm > 0.0 {
                time += (delta_beats * 60.0) / last_bpm;
            }
            return time.max(0.0);
        }
        if beat > last_beat && last_bpm > 0.0 {
            time += ((beat - last_beat) * 60.0) / last_bpm;
        }
        last_beat = beat;
        last_bpm = bpm;
    }
    if last_bpm > 0.0 {
        time += ((target_beat - last_beat).max(0.0) * 60.0) / last_bpm;
    }
    time.max(0.0)
}

fn beat_at_sec_from_bpms(normalized_bpms: &str, target_sec: f64) -> f64 {
    if !target_sec.is_finite() || target_sec <= 0.0 {
        return 0.0;
    }
    let mut bpm_map = parse_bpm_map(normalized_bpms);
    if bpm_map.is_empty() {
        bpm_map.push((0.0, 60.0));
    }
    if bpm_map.first().is_none_or(|(b, _)| *b != 0.0) {
        let first_bpm = bpm_map[0].1;
        bpm_map.insert(0, (0.0, first_bpm));
    }
    let mut elapsed = 0.0;
    let mut last_beat = 0.0;
    let mut last_bpm = bpm_map[0].1;
    for &(beat, bpm) in &bpm_map {
        let delta_beats = (beat - last_beat).max(0.0);
        let delta_sec = if last_bpm > 0.0 {
            (delta_beats * 60.0) / last_bpm
        } else {
            0.0
        };
        if elapsed + delta_sec >= target_sec {
            let remain = (target_sec - elapsed).max(0.0);
            let add_beats = if last_bpm > 0.0 {
                remain * last_bpm / 60.0
            } else {
                0.0
            };
            return (last_beat + add_beats).max(0.0);
        }
        elapsed += delta_sec;
        last_beat = beat;
        last_bpm = bpm;
    }
    let remain = (target_sec - elapsed).max(0.0);
    let add_beats = if last_bpm > 0.0 {
        remain * last_bpm / 60.0
    } else {
        0.0
    };
    (last_beat + add_beats).max(0.0)
}

fn sec_at_beat(song: &SongData, target_beat: f64) -> f64 {
    if !target_beat.is_finite() || target_beat <= 0.0 {
        return 0.0;
    }
    if let Some(chart) = song.charts.first() {
        return chart.timing.get_time_for_beat(target_beat as f32).max(0.0) as f64;
    }
    sec_at_beat_from_bpms(&song.normalized_bpms, target_beat)
}

fn beat_at_sec(song: &SongData, target_sec: f64) -> f64 {
    if !target_sec.is_finite() || target_sec <= 0.0 {
        return 0.0;
    }
    if let Some(chart) = song.charts.first() {
        return chart.timing.get_beat_for_time(target_sec as f32).max(0.0) as f64;
    }
    beat_at_sec_from_bpms(&song.normalized_bpms, target_sec)
}

#[inline(always)]
fn preview_song_sec(state: &State) -> Option<f64> {
    let start_sec = state.currently_playing_preview_start_sec?;
    let length_sec = state.currently_playing_preview_length_sec?;
    let stream_sec = audio::get_music_stream_position_seconds();
    if !stream_sec.is_finite() || stream_sec < 0.0 {
        return None;
    }
    let rate = profile::get_session_music_rate();
    let rate = if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    };
    let mut rel_song_sec = stream_sec * rate;
    if length_sec.is_finite() && length_sec > 0.0 {
        rel_song_sec = rel_song_sec.rem_euclid(length_sec);
    }
    Some((start_sec + rel_song_sec) as f64)
}

#[inline(always)]
fn sl_arrow_bounce01(entry_opt: Option<&MusicWheelEntry>, state: &State) -> f32 {
    let beat = match entry_opt {
        Some(MusicWheelEntry::Song(song)) => preview_song_sec(state).map_or(
            state.session_elapsed * song.max_bpm.max(1.0) as f32 / 60.0,
            |sec| beat_at_sec(song, sec) as f32,
        ),
        _ => state.session_elapsed * 2.5, // 150 BPM fallback
    };
    let effect_offset = -10.0 * crate::config::get().global_offset_seconds;
    let t = (beat + effect_offset).rem_euclid(1.0);
    (t * std::f32::consts::PI).sin().clamp(0.0, 1.0)
}

fn compute_preview_cut(song: &SongData) -> Option<(std::path::PathBuf, audio::Cut)> {
    let path = song.music_path.clone()?;
    let mut start = song.sample_start.unwrap_or(0.0) as f64;
    let mut length = song.sample_length.unwrap_or(0.0) as f64;
    let total_len = if song.music_length_seconds.is_finite() && song.music_length_seconds > 0.0 {
        song.music_length_seconds as f64
    } else {
        song.total_length_seconds.max(0) as f64
    };

    if !(length.is_sign_positive() && length.is_finite()) || length == 0.0 {
        let at_beat_100 = sec_at_beat(song, 100.0);
        start = if total_len > 0.0 && at_beat_100 + DEFAULT_PREVIEW_LENGTH > total_len {
            let last_beat = beat_at_sec(song, total_len);
            let mut i_beat = (last_beat / 2.0).round();
            if i_beat.is_finite() {
                i_beat -= i_beat % 4.0;
            } else {
                i_beat = 0.0;
            }
            sec_at_beat(song, i_beat)
        } else {
            at_beat_100
        };
        length = DEFAULT_PREVIEW_LENGTH;
    } else if total_len > 0.0 && (start + length) > total_len {
        let last_beat = beat_at_sec(song, total_len);
        let mut i_beat = (last_beat / 2.0).round();
        if i_beat.is_finite() {
            i_beat -= i_beat % 4.0;
        } else {
            i_beat = 0.0;
        }
        start = sec_at_beat(song, i_beat);
    }

    if !start.is_finite() || start < 0.0 {
        start = 0.0;
    }
    if !length.is_finite() || length <= 0.0 {
        length = DEFAULT_PREVIEW_LENGTH;
    }

    Some((
        path,
        audio::Cut {
            start_sec: start,
            length_sec: length,
            fade_out_sec: PREVIEW_FADE_OUT_SECONDS,
            ..Default::default()
        },
    ))
}

// Optimized formatter
fn fmt_music_rate(rate: f32) -> String {
    let scaled = (rate * 100.0).round() as i32;
    if scaled == 100 {
        return "1.0".to_string();
    }
    let int_part = scaled / 100;
    let frac2 = (scaled % 100).abs();
    if frac2 == 0 {
        format!("{}", int_part)
    } else if frac2 % 10 == 0 {
        format!("{}.{}", int_part, frac2 / 10)
    } else {
        format!("{}.{:02}", int_part, frac2)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum NavDirection {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum OutPromptState {
    None,
    PressStartForOptions { elapsed: f32 },
    EnteringOptions { elapsed: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ExitPromptState {
    None,
    Active {
        elapsed: f32,
        active_choice: u8,
        switch_from: Option<u8>,
        switch_elapsed: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortMenuAction {
    SortByGroup,
    SortByTitle,
    SortByRecent,
    SongSearch,
    ReloadSongsCourses,
    ShowLeaderboard,
}

#[derive(Clone, Copy, Debug)]
struct SortMenuItem {
    top_label: &'static str,
    bottom_label: &'static str,
    action: SortMenuAction,
}

const SORT_MENU_ITEMS: [SortMenuItem; 6] = [
    SortMenuItem {
        top_label: "Sort By",
        bottom_label: "Group",
        action: SortMenuAction::SortByGroup,
    },
    SortMenuItem {
        top_label: "Sort By",
        bottom_label: "Title",
        action: SortMenuAction::SortByTitle,
    },
    SortMenuItem {
        top_label: "Sort By",
        bottom_label: "Recently Played",
        action: SortMenuAction::SortByRecent,
    },
    SortMenuItem {
        top_label: "Wherefore Art Thou?",
        bottom_label: "Song Search",
        action: SortMenuAction::SongSearch,
    },
    SortMenuItem {
        top_label: "Take a Breather~",
        bottom_label: "Load New Songs",
        action: SortMenuAction::ReloadSongsCourses,
    },
    SortMenuItem {
        top_label: "GrooveStats",
        bottom_label: "Leaderboard",
        action: SortMenuAction::ShowLeaderboard,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortMenuState {
    Hidden,
    Visible { selected_index: usize },
}

#[derive(Clone, Debug)]
struct SongSearchCandidate {
    pack_name: String,
    song: Arc<SongData>,
}

#[derive(Clone, Debug)]
struct SongSearchResultsState {
    search_text: String,
    candidates: Vec<SongSearchCandidate>,
    selected_index: usize,
    prev_selected_index: usize,
    focus_anim_elapsed: f32,
    input_lock: f32,
}

#[derive(Clone, Debug)]
enum SongSearchState {
    Hidden,
    Prompt { query: String },
    Results(SongSearchResultsState),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReloadPhase {
    Songs,
    Courses,
}

enum ReloadMsg {
    Phase(ReloadPhase),
    Song { pack: String, song: String },
    Course { group: String, course: String },
    Done,
}

struct ReloadUiState {
    phase: ReloadPhase,
    line2: String,
    line3: String,
    done: bool,
    rx: mpsc::Receiver<ReloadMsg>,
}

impl ReloadUiState {
    fn new(rx: mpsc::Receiver<ReloadMsg>) -> Self {
        Self {
            phase: ReloadPhase::Songs,
            line2: String::new(),
            line3: String::new(),
            done: false,
            rx,
        }
    }
}

#[derive(Debug)]
struct LeaderboardFetchRequest {
    chart_hash: String,
    api_key: String,
    show_ex_score: bool,
}

#[derive(Debug)]
struct LeaderboardFetchResult {
    p1: Option<Result<scores::PlayerLeaderboardData, String>>,
    p2: Option<Result<scores::PlayerLeaderboardData, String>>,
}

#[derive(Clone, Debug, Default)]
struct LeaderboardSideState {
    joined: bool,
    loading: bool,
    panes: Vec<scores::LeaderboardPane>,
    pane_index: usize,
    show_icons: bool,
    error_text: Option<String>,
    machine_pane: Option<scores::LeaderboardPane>,
}

#[derive(Debug)]
struct LeaderboardOverlayStateData {
    elapsed: f32,
    p1: LeaderboardSideState,
    p2: LeaderboardSideState,
    rx: Option<mpsc::Receiver<LeaderboardFetchResult>>,
}

#[derive(Debug)]
enum LeaderboardOverlayState {
    Hidden,
    Visible(LeaderboardOverlayStateData),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WheelSortMode {
    Group,
    Title,
    Recent,
}

#[derive(Clone, Debug)]
pub enum MusicWheelEntry {
    PackHeader {
        name: String,
        original_index: usize,
        banner_path: Option<PathBuf>,
    },
    Song(Arc<SongData>),
}

#[derive(Clone, Debug)]
struct DisplayedChart {
    song: Arc<SongData>,
    chart_hash: String,
}

#[derive(Clone, Debug)]
struct EditSortCache {
    song: Arc<SongData>,
    chart_type: &'static str,
    indices: Vec<usize>,
}

pub struct State {
    pub entries: Vec<MusicWheelEntry>,
    pub selected_index: usize,
    pub selected_steps_index: usize,
    pub preferred_difficulty_index: usize,
    pub p2_selected_steps_index: usize,
    pub p2_preferred_difficulty_index: usize,
    pub active_color_index: i32,
    pub selection_animation_timer: f32,
    pub wheel_offset_from_selection: f32,
    pub current_banner_key: String,
    pub current_graph_key: String,
    pub current_graph_key_p2: String,
    pub current_graph_mesh: Option<Arc<[MeshVertex]>>,
    pub current_graph_mesh_p2: Option<Arc<[MeshVertex]>>,
    pub session_elapsed: f32,
    displayed_chart_p1: Option<DisplayedChart>,
    displayed_chart_p2: Option<DisplayedChart>,

    // Internal state
    out_prompt: OutPromptState,
    exit_prompt: ExitPromptState,
    reload_ui: Option<ReloadUiState>,
    song_search: SongSearchState,
    sort_menu: SortMenuState,
    leaderboard: LeaderboardOverlayState,
    sort_mode: WheelSortMode,
    all_entries: Vec<MusicWheelEntry>,
    group_entries: Vec<MusicWheelEntry>,
    title_entries: Vec<MusicWheelEntry>,
    recent_entries: Vec<MusicWheelEntry>,
    expanded_pack_name: Option<String>,
    bg: heart_bg::State,
    last_requested_banner_path: Option<PathBuf>,
    last_requested_chart_hash: Option<String>,
    last_requested_chart_hash_p2: Option<String>,
    chord_mask_p1: u8,
    chord_mask_p2: u8,
    menu_chord_mask: u8,
    last_steps_nav_dir_p1: Option<PadDir>,
    last_steps_nav_time_p1: Option<Instant>,
    last_steps_nav_dir_p2: Option<PadDir>,
    last_steps_nav_time_p2: Option<Instant>,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    sort_menu_prev_selected_index: usize,
    sort_menu_focus_anim_elapsed: f32,
    currently_playing_preview_path: Option<PathBuf>,
    currently_playing_preview_start_sec: Option<f32>,
    currently_playing_preview_length_sec: Option<f32>,
    prev_selected_index: usize,
    time_since_selection_change: f32,

    // Caches to avoid O(N) ops in hot paths
    cached_song: Option<Arc<SongData>>,
    cached_chart_type: &'static str,
    cached_steps_index_p1: usize,
    cached_steps_index_p2: usize,
    cached_chart_ix_p1: Option<usize>,
    cached_chart_ix_p2: Option<usize>,
    cached_edits: Option<EditSortCache>,
    pub pack_song_counts: HashMap<String, usize>,
    group_pack_song_counts: HashMap<String, usize>,
    title_pack_song_counts: HashMap<String, usize>,
    recent_pack_song_counts: HashMap<String, usize>,
}

pub(crate) fn is_difficulty_playable(song: &Arc<SongData>, difficulty_index: usize) -> bool {
    if difficulty_index >= color::FILE_DIFFICULTY_NAMES.len() {
        return false;
    }
    let target_difficulty_name = color::FILE_DIFFICULTY_NAMES[difficulty_index];
    let target_chart_type = profile::get_session_play_style().chart_type();
    song.charts.iter().any(|c| {
        c.chart_type.eq_ignore_ascii_case(target_chart_type)
            && c.difficulty.eq_ignore_ascii_case(target_difficulty_name)
            && !c.notes.is_empty()
    })
}

pub(crate) fn edit_charts_sorted<'a>(song: &'a SongData, chart_type: &str) -> Vec<&'a ChartData> {
    let mut edits: Vec<&ChartData> = song
        .charts
        .iter()
        .filter(|c| {
            c.chart_type.eq_ignore_ascii_case(chart_type)
                && c.difficulty.eq_ignore_ascii_case("edit")
                && !c.notes.is_empty()
        })
        .collect();
    edits.sort_by(|a, b| {
        a.description
            .to_lowercase()
            .cmp(&b.description.to_lowercase())
            .then(a.meter.cmp(&b.meter))
            .then(a.short_hash.cmp(&b.short_hash))
    });
    edits
}

pub(crate) fn chart_for_steps_index<'a>(
    song: &'a SongData,
    chart_type: &str,
    steps_index: usize,
) -> Option<&'a ChartData> {
    if steps_index < color::FILE_DIFFICULTY_NAMES.len() {
        let diff_name = color::FILE_DIFFICULTY_NAMES[steps_index];
        return song.charts.iter().find(|c| {
            c.chart_type.eq_ignore_ascii_case(chart_type)
                && c.difficulty.eq_ignore_ascii_case(diff_name)
                && !c.notes.is_empty()
        });
    }

    let edit_index = steps_index - color::FILE_DIFFICULTY_NAMES.len();
    edit_charts_sorted(song, chart_type)
        .get(edit_index)
        .copied()
}

fn edit_chart_indices_sorted(song: &SongData, chart_type: &str) -> Vec<usize> {
    let mut indices: Vec<usize> = song
        .charts
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            if c.chart_type.eq_ignore_ascii_case(chart_type)
                && c.difficulty.eq_ignore_ascii_case("edit")
                && !c.notes.is_empty()
            {
                Some(i)
            } else {
                None
            }
        })
        .collect();
    indices.sort_by(|&ai, &bi| {
        let a = &song.charts[ai];
        let b = &song.charts[bi];
        a.description
            .to_lowercase()
            .cmp(&b.description.to_lowercase())
            .then(a.meter.cmp(&b.meter))
            .then(a.short_hash.cmp(&b.short_hash))
    });
    indices
}

#[inline]
fn chart_ix_for_steps_index(
    song: &SongData,
    chart_type: &str,
    steps_index: usize,
    edits_sorted: &[usize],
) -> Option<usize> {
    if steps_index < color::FILE_DIFFICULTY_NAMES.len() {
        let diff_name = color::FILE_DIFFICULTY_NAMES[steps_index];
        return song
            .charts
            .iter()
            .enumerate()
            .find(|(_, c)| {
                c.chart_type.eq_ignore_ascii_case(chart_type)
                    && c.difficulty.eq_ignore_ascii_case(diff_name)
                    && !c.notes.is_empty()
            })
            .map(|(i, _)| i);
    }

    let edit_index = steps_index - color::FILE_DIFFICULTY_NAMES.len();
    edits_sorted.get(edit_index).copied()
}

fn ensure_chart_cache_for_song(
    state: &mut State,
    song: &Arc<SongData>,
    chart_type: &'static str,
    is_versus: bool,
) {
    let song_changed = state
        .cached_song
        .as_ref()
        .is_none_or(|s| !Arc::ptr_eq(s, song));
    let type_changed = state.cached_chart_type != chart_type;
    let p1_changed = state.cached_steps_index_p1 != state.selected_steps_index;
    let p2_changed = state.cached_steps_index_p2 != state.p2_selected_steps_index;

    if song_changed || type_changed {
        state.cached_edits = None;
    }

    let need_edits = state.selected_steps_index >= color::FILE_DIFFICULTY_NAMES.len()
        || (is_versus && state.p2_selected_steps_index >= color::FILE_DIFFICULTY_NAMES.len());
    if need_edits {
        let rebuild_edits = state
            .cached_edits
            .as_ref()
            .is_none_or(|c| !Arc::ptr_eq(&c.song, song) || c.chart_type != chart_type);
        if rebuild_edits {
            state.cached_edits = Some(EditSortCache {
                song: song.clone(),
                chart_type,
                indices: edit_chart_indices_sorted(song, chart_type),
            });
        }
    }

    let edits: &[usize] = state
        .cached_edits
        .as_ref()
        .map_or(&[], |c| c.indices.as_slice());

    if song_changed || type_changed || p1_changed {
        state.cached_chart_ix_p1 =
            chart_ix_for_steps_index(song, chart_type, state.selected_steps_index, edits);
    }
    if !is_versus {
        state.cached_chart_ix_p2 = None;
    } else if song_changed || type_changed || p2_changed {
        state.cached_chart_ix_p2 =
            chart_ix_for_steps_index(song, chart_type, state.p2_selected_steps_index, edits);
    }

    state.cached_song = Some(song.clone());
    state.cached_chart_type = chart_type;
    state.cached_steps_index_p1 = state.selected_steps_index;
    state.cached_steps_index_p2 = state.p2_selected_steps_index;
}

pub(crate) fn steps_index_for_chart_hash(
    song: &SongData,
    chart_type: &str,
    chart_hash: &str,
) -> Option<usize> {
    let chart = song.charts.iter().find(|c| {
        c.chart_type.eq_ignore_ascii_case(chart_type)
            && c.short_hash == chart_hash
            && !c.notes.is_empty()
    })?;

    if let Some(std_idx) = color::FILE_DIFFICULTY_NAMES
        .iter()
        .position(|&n| n.eq_ignore_ascii_case(&chart.difficulty))
    {
        return Some(std_idx);
    }
    if chart.difficulty.eq_ignore_ascii_case("edit") {
        let edits = edit_charts_sorted(song, chart_type);
        let pos = edits.iter().position(|c| c.short_hash == chart_hash)?;
        return Some(color::FILE_DIFFICULTY_NAMES.len() + pos);
    }
    None
}

pub(crate) fn steps_len(song: &SongData, chart_type: &str) -> usize {
    color::FILE_DIFFICULTY_NAMES.len() + edit_charts_sorted(song, chart_type).len()
}

fn rebuild_displayed_entries(state: &mut State) {
    let has_pack_headers = state
        .all_entries
        .iter()
        .any(|e| matches!(e, MusicWheelEntry::PackHeader { .. }));
    if !has_pack_headers {
        state.entries = state.all_entries.clone();
        if state.entries.is_empty() {
            state.wheel_offset_from_selection = 0.0;
        }
        return;
    }

    let mut new_entries = Vec::with_capacity(state.all_entries.len());
    let mut current_pack_name: Option<&str> = None;
    let expanded_pack_name = state.expanded_pack_name.as_deref();

    // Linear pass, avoid per-entry string clones.
    for entry in &state.all_entries {
        match entry {
            MusicWheelEntry::PackHeader { name, .. } => {
                current_pack_name = Some(name.as_str());
                new_entries.push(entry.clone());
            }
            MusicWheelEntry::Song(_) => {
                if expanded_pack_name == current_pack_name {
                    new_entries.push(entry.clone());
                }
            }
        }
    }
    state.entries = new_entries;
    if state.entries.is_empty() {
        state.wheel_offset_from_selection = 0.0;
    }
}

#[inline(always)]
fn selected_song_arc(state: &State) -> Option<Arc<SongData>> {
    match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(song)) => Some(song.clone()),
        _ => None,
    }
}

fn song_entry_index(entries: &[MusicWheelEntry], target_song: &Arc<SongData>) -> Option<usize> {
    entries
        .iter()
        .position(|e| matches!(e, MusicWheelEntry::Song(song) if Arc::ptr_eq(song, target_song)))
}

fn group_name_for_song(
    grouped_entries: &[MusicWheelEntry],
    target_song: &Arc<SongData>,
) -> Option<String> {
    let mut current_pack_name: Option<&str> = None;
    for entry in grouped_entries {
        match entry {
            MusicWheelEntry::PackHeader { name, .. } => {
                current_pack_name = Some(name.as_str());
            }
            MusicWheelEntry::Song(song) => {
                if Arc::ptr_eq(song, target_song) {
                    return current_pack_name.map(str::to_string);
                }
            }
        }
    }
    None
}

#[inline(always)]
fn song_title_sort_key(song: &SongData) -> (String, String, String) {
    let title = if song.translit_title.trim().is_empty() {
        song.title.as_str()
    } else {
        song.translit_title.as_str()
    };
    let subtitle = if song.translit_subtitle.trim().is_empty() {
        song.subtitle.as_str()
    } else {
        song.translit_subtitle.as_str()
    };
    (
        title.to_ascii_lowercase(),
        subtitle.to_ascii_lowercase(),
        song.simfile_path.to_string_lossy().to_ascii_lowercase(),
    )
}

#[inline(always)]
fn title_group_meta(song: &SongData) -> (u8, String) {
    let title = if song.translit_title.trim().is_empty() {
        song.title.as_str()
    } else {
        song.translit_title.as_str()
    };
    // Match expected title bucketing semantics:
    // classify by the first visible character only (after whitespace),
    // not by the first alphanumeric found later in the title.
    let first = title.trim_start().chars().next();
    match first {
        Some(ch) if ch.is_ascii_digit() => (1, "0-9".to_string()),
        Some(ch) if ch.is_ascii_alphabetic() => {
            let c = ch.to_ascii_uppercase();
            let rank = (c as u8).saturating_sub(b'A').saturating_add(2);
            (rank, c.to_string())
        }
        _ => (0, "Other".to_string()),
    }
}

#[inline(always)]
fn first_header_name(entries: &[MusicWheelEntry]) -> Option<String> {
    entries.iter().find_map(|e| {
        if let MusicWheelEntry::PackHeader { name, .. } = e {
            Some(name.clone())
        } else {
            None
        }
    })
}

fn build_title_grouped_entries(
    grouped_entries: &[MusicWheelEntry],
) -> (Vec<MusicWheelEntry>, HashMap<String, usize>) {
    let mut songs: Vec<Arc<SongData>> = grouped_entries
        .iter()
        .filter_map(|e| match e {
            MusicWheelEntry::Song(song) => Some(song.clone()),
            MusicWheelEntry::PackHeader { .. } => None,
        })
        .collect();

    songs.sort_by(|a, b| {
        let (a_bucket, _) = title_group_meta(a.as_ref());
        let (b_bucket, _) = title_group_meta(b.as_ref());
        a_bucket
            .cmp(&b_bucket)
            .then_with(|| song_title_sort_key(a.as_ref()).cmp(&song_title_sort_key(b.as_ref())))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.subtitle.cmp(&b.subtitle))
    });

    let mut entries: Vec<MusicWheelEntry> = Vec::with_capacity(songs.len().saturating_add(32));
    let mut counts: HashMap<String, usize> = HashMap::with_capacity(32);
    let mut current_group: Option<String> = None;
    let mut header_idx = 0usize;

    for song in songs {
        let (_, group_name) = title_group_meta(song.as_ref());
        if current_group.as_deref() != Some(group_name.as_str()) {
            entries.push(MusicWheelEntry::PackHeader {
                name: group_name.clone(),
                original_index: header_idx,
                banner_path: None,
            });
            current_group = Some(group_name.clone());
            header_idx += 1;
        }
        *counts.entry(group_name).or_insert(0) += 1;
        entries.push(MusicWheelEntry::Song(song));
    }

    (entries, counts)
}

fn build_recent_grouped_entries(
    grouped_entries: &[MusicWheelEntry],
) -> (Vec<MusicWheelEntry>, HashMap<String, usize>) {
    let mut hash_to_song: HashMap<String, Arc<SongData>> = HashMap::new();
    for entry in grouped_entries {
        let MusicWheelEntry::Song(song) = entry else {
            continue;
        };
        for chart in &song.charts {
            if chart.notes.is_empty() {
                continue;
            }
            hash_to_song
                .entry(chart.short_hash.clone())
                .or_insert_with(|| song.clone());
        }
    }

    let recent_chart_hashes = scores::recent_played_chart_hashes_for_machine();
    let mut recent_songs: Vec<Arc<SongData>> = Vec::with_capacity(RECENT_SONGS_TO_SHOW);
    let mut seen_song_ptrs: HashSet<usize> = HashSet::with_capacity(RECENT_SONGS_TO_SHOW);

    for chart_hash in recent_chart_hashes {
        let Some(song) = hash_to_song.get(chart_hash.as_str()) else {
            continue;
        };
        let song_ptr = Arc::as_ptr(song) as usize;
        if !seen_song_ptrs.insert(song_ptr) {
            continue;
        }
        recent_songs.push(song.clone());
        if recent_songs.len() >= RECENT_SONGS_TO_SHOW {
            break;
        }
    }

    let count = recent_songs.len();
    let mut entries: Vec<MusicWheelEntry> = Vec::with_capacity(count.saturating_add(1));
    entries.push(MusicWheelEntry::PackHeader {
        name: RECENT_SORT_HEADER.to_string(),
        original_index: 0,
        banner_path: None,
    });
    entries.extend(recent_songs.into_iter().map(MusicWheelEntry::Song));

    let mut counts: HashMap<String, usize> = HashMap::with_capacity(1);
    counts.insert(RECENT_SORT_HEADER.to_string(), count);
    (entries, counts)
}

fn refresh_recent_cache(state: &mut State) {
    let (recent_entries, recent_pack_song_counts) =
        build_recent_grouped_entries(&state.group_entries);
    state.recent_entries = recent_entries;
    state.recent_pack_song_counts = recent_pack_song_counts;
}

fn apply_wheel_sort(state: &mut State, sort_mode: WheelSortMode) {
    if state.sort_mode == sort_mode {
        return;
    }

    let selected_song = selected_song_arc(state);

    match sort_mode {
        WheelSortMode::Group => {
            state.all_entries = state.group_entries.clone();
            state.pack_song_counts = state.group_pack_song_counts.clone();
            state.expanded_pack_name = selected_song
                .as_ref()
                .and_then(|song| group_name_for_song(&state.group_entries, song))
                .or_else(|| first_header_name(&state.group_entries));
        }
        WheelSortMode::Title => {
            state.all_entries = state.title_entries.clone();
            state.pack_song_counts = state.title_pack_song_counts.clone();
            state.expanded_pack_name = selected_song
                .as_ref()
                .and_then(|song| group_name_for_song(&state.title_entries, song))
                .or_else(|| first_header_name(&state.title_entries));
        }
        WheelSortMode::Recent => {
            state.all_entries = state.recent_entries.clone();
            state.pack_song_counts = state.recent_pack_song_counts.clone();
            state.expanded_pack_name = selected_song
                .as_ref()
                .and_then(|song| group_name_for_song(&state.recent_entries, song))
                .or_else(|| first_header_name(&state.recent_entries));
        }
    }

    state.sort_mode = sort_mode;
    rebuild_displayed_entries(state);

    state.selected_index = if let Some(song) = selected_song.as_ref() {
        song_entry_index(&state.entries, song).unwrap_or_else(|| {
            state
                .selected_index
                .min(state.entries.len().saturating_sub(1))
        })
    } else {
        state
            .selected_index
            .min(state.entries.len().saturating_sub(1))
    };

    state.prev_selected_index = state.selected_index;
    state.time_since_selection_change = 0.0;
    state.wheel_offset_from_selection = 0.0;
    state.last_requested_banner_path = None;
    state.last_requested_chart_hash = None;
    state.last_requested_chart_hash_p2 = None;
    state.cached_song = None;
    state.cached_chart_ix_p1 = None;
    state.cached_chart_ix_p2 = None;
    state.cached_edits = None;
}

pub fn init() -> State {
    let started = Instant::now();
    info!("Initializing SelectMusic screen...");
    let lock_started = Instant::now();
    let song_cache = get_song_cache();
    let lock_wait = lock_started.elapsed();

    let target_chart_type = profile::get_session_play_style().chart_type();
    let total_packs = song_cache.len();
    let total_songs: usize = song_cache.iter().map(|p| p.songs.len()).sum();

    let mut all_entries = Vec::with_capacity(total_packs.saturating_add(total_songs));
    let mut pack_song_counts = HashMap::with_capacity(total_packs);

    let profile_data = profile::get();
    let max_diff_index = color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1);
    let initial_diff_index = if max_diff_index == 0 {
        0
    } else {
        profile_data.last_difficulty_index.min(max_diff_index)
    };

    let mut last_song_arc: Option<Arc<SongData>> = None;
    let mut last_pack_name: Option<String> = None;
    let last_path = profile_data.last_song_music_path.as_deref();

    let mut matched_packs = 0usize;
    let mut matched_songs = 0usize;

    // Filter and build entries in one pass
    for (i, pack) in song_cache.iter().enumerate() {
        let mut pack_name: Option<String> = None;
        let mut pack_song_count = 0usize;

        for song in &pack.songs {
            let ok = song
                .charts
                .iter()
                .any(|c| c.chart_type.eq_ignore_ascii_case(target_chart_type));
            if !ok {
                continue;
            }

            let pack_name = pack_name.get_or_insert_with(|| {
                matched_packs += 1;
                let name = pack.name.clone();
                all_entries.push(MusicWheelEntry::PackHeader {
                    name: name.clone(),
                    original_index: i,
                    banner_path: pack.banner_path.clone(),
                });
                name
            });

            pack_song_count += 1;
            matched_songs += 1;
            all_entries.push(MusicWheelEntry::Song(song.clone()));

            // Check for last played song
            if last_song_arc.is_none()
                && let Some(last_path) = last_path
                && song
                    .music_path
                    .as_ref()
                    .is_some_and(|p| p.to_string_lossy() == last_path)
            {
                last_song_arc = Some(song.clone());
                last_pack_name = Some(pack_name.clone());
            }
        }

        if let Some(name) = pack_name {
            // Compute cache for get_actors (HOT PATH OPTIMIZATION)
            pack_song_counts.insert(name, pack_song_count);
        }
    }

    let (title_entries, title_pack_song_counts) = build_title_grouped_entries(&all_entries);
    let (recent_entries, recent_pack_song_counts) = build_recent_grouped_entries(&all_entries);

    let mut state = State {
        all_entries: all_entries.clone(),
        group_entries: all_entries,
        title_entries,
        recent_entries,
        entries: Vec::new(),
        selected_index: 0,
        selected_steps_index: initial_diff_index,
        preferred_difficulty_index: initial_diff_index,
        p2_selected_steps_index: initial_diff_index,
        p2_preferred_difficulty_index: initial_diff_index,
        active_color_index: color::DEFAULT_COLOR_INDEX,
        selection_animation_timer: 0.0,
        wheel_offset_from_selection: 0.0,
        out_prompt: OutPromptState::None,
        exit_prompt: ExitPromptState::None,
        reload_ui: None,
        song_search: SongSearchState::Hidden,
        sort_menu: SortMenuState::Hidden,
        leaderboard: LeaderboardOverlayState::Hidden,
        sort_mode: WheelSortMode::Group,
        expanded_pack_name: last_pack_name,
        bg: heart_bg::State::new(),
        last_requested_banner_path: None,
        current_banner_key: "banner1.png".to_string(),
        last_requested_chart_hash: None,
        current_graph_key: "__white".to_string(),
        current_graph_key_p2: "__white".to_string(),
        current_graph_mesh: None,
        current_graph_mesh_p2: None,
        displayed_chart_p1: None,
        displayed_chart_p2: None,
        last_requested_chart_hash_p2: None,
        chord_mask_p1: 0,
        chord_mask_p2: 0,
        menu_chord_mask: 0,
        last_steps_nav_dir_p1: None,
        last_steps_nav_time_p1: None,
        last_steps_nav_dir_p2: None,
        last_steps_nav_time_p2: None,
        nav_key_held_direction: None,
        nav_key_held_since: None,
        sort_menu_prev_selected_index: 0,
        sort_menu_focus_anim_elapsed: SL_SORT_MENU_FOCUS_TWEEN_SECONDS,
        currently_playing_preview_path: None,
        currently_playing_preview_start_sec: None,
        currently_playing_preview_length_sec: None,
        session_elapsed: 0.0,
        prev_selected_index: 0,
        time_since_selection_change: 0.0,
        cached_song: None,
        cached_chart_type: "",
        cached_steps_index_p1: usize::MAX,
        cached_steps_index_p2: usize::MAX,
        cached_chart_ix_p1: None,
        cached_chart_ix_p2: None,
        cached_edits: None,
        pack_song_counts: pack_song_counts.clone(),
        group_pack_song_counts: pack_song_counts,
        title_pack_song_counts,
        recent_pack_song_counts,
    };

    let built_entries_len = state.all_entries.len();
    let rebuild_started = Instant::now();
    rebuild_displayed_entries(&mut state);
    let rebuild_dur = rebuild_started.elapsed();
    let displayed_entries_len = state.entries.len();

    // Restore selection
    if let Some(last_song) = last_song_arc {
        if let Some(idx) = state.entries.iter().position(|e| match e {
            MusicWheelEntry::Song(s) => Arc::ptr_eq(s, &last_song),
            _ => false,
        }) {
            state.selected_index = idx;
            if let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) {
                if let Some(hash) = profile_data.last_chart_hash.as_deref()
                    && let Some(idx2) = steps_index_for_chart_hash(song, target_chart_type, hash)
                {
                    state.selected_steps_index = idx2;
                    if idx2 < color::FILE_DIFFICULTY_NAMES.len() {
                        state.preferred_difficulty_index = idx2;
                    }
                    state.p2_selected_steps_index = state.selected_steps_index;
                    state.p2_preferred_difficulty_index = state.preferred_difficulty_index;
                    state.prev_selected_index = state.selected_index;
                    info!(
                        "SelectMusic init done: chart_type={target_chart_type} matched {matched_songs} songs in {matched_packs}/{total_packs} packs ({} total songs), entries {built_entries_len}→{displayed_entries_len}, lock {:?}, rebuild {:?}, total {:?}.",
                        total_songs,
                        lock_wait,
                        rebuild_dur,
                        started.elapsed()
                    );
                    return state;
                }

                let preferred = state.preferred_difficulty_index;
                let mut best_match_index = None;
                let mut min_diff = i32::MAX;
                for i in 0..color::FILE_DIFFICULTY_NAMES.len() {
                    if is_difficulty_playable(song, i) {
                        let diff = (i as i32 - preferred as i32).abs();
                        if diff < min_diff {
                            min_diff = diff;
                            best_match_index = Some(i);
                        }
                    }
                }
                if let Some(idx2) = best_match_index {
                    state.selected_steps_index = idx2;
                }
                state.p2_selected_steps_index = state.selected_steps_index;
                state.p2_preferred_difficulty_index = state.preferred_difficulty_index;
            }
        }
    }

    state.prev_selected_index = state.selected_index;
    info!(
        "SelectMusic init done: chart_type={target_chart_type} matched {matched_songs} songs in {matched_packs}/{total_packs} packs ({} total songs), entries {built_entries_len}→{displayed_entries_len}, lock {:?}, rebuild {:?}, total {:?}.",
        total_songs,
        lock_wait,
        rebuild_dur,
        started.elapsed()
    );
    state
}

#[inline(always)]
fn music_wheel_settle_offset(state: &mut State, dt: f32) {
    if dt <= 0.0 || state.wheel_offset_from_selection == 0.0 {
        return;
    }
    let off = state.wheel_offset_from_selection;
    let spin_speed = MUSIC_WHEEL_SETTLE_MIN_SPEED + off.abs() / MUSIC_WHEEL_SWITCH_SECONDS;
    if off > 0.0 {
        state.wheel_offset_from_selection = (off - spin_speed * dt).max(0.0);
    } else {
        state.wheel_offset_from_selection = (off + spin_speed * dt).min(0.0);
    }
}

#[inline(always)]
fn music_wheel_change(state: &mut State, dist: isize) {
    if dist == 0 {
        return;
    }
    let num_entries = state.entries.len();
    if num_entries == 0 {
        state.selected_index = 0;
        state.wheel_offset_from_selection = 0.0;
        state.time_since_selection_change = 0.0;
        return;
    }

    if dist > 0 {
        state.selected_index = (state.selected_index + 1) % num_entries;
        state.wheel_offset_from_selection += 1.0;
    } else if dist < 0 {
        state.selected_index = (state.selected_index + num_entries - 1) % num_entries;
        state.wheel_offset_from_selection -= 1.0;
    }
    state.time_since_selection_change = 0.0;
}

#[inline(always)]
fn music_wheel_update_hold_scroll(state: &mut State, dt: f32, dir: NavDirection) {
    if dt <= 0.0 {
        return;
    }

    let moving = match dir {
        NavDirection::Left => -1.0,
        NavDirection::Right => 1.0,
    };

    state.wheel_offset_from_selection -= MUSIC_WHEEL_HOLD_SPIN_SPEED * moving * dt;
    state.wheel_offset_from_selection = state.wheel_offset_from_selection.clamp(-1.0, 1.0);

    let off = state.wheel_offset_from_selection;
    let passed_selection = (moving < 0.0 && off >= 0.0) || (moving > 0.0 && off <= 0.0);
    if !passed_selection {
        return;
    }

    let dist = if moving < 0.0 { -1 } else { 1 };
    music_wheel_change(state, dist);
}

#[inline(always)]
fn clear_preview(state: &mut State) {
    state.currently_playing_preview_path = None;
    state.currently_playing_preview_start_sec = None;
    state.currently_playing_preview_length_sec = None;
    audio::stop_music();
}

#[inline(always)]
fn show_sort_menu(state: &mut State) {
    let selected_index = match state.sort_mode {
        WheelSortMode::Group => 0,
        WheelSortMode::Title => 1,
        WheelSortMode::Recent => 2,
    };
    state.sort_menu = SortMenuState::Visible { selected_index };
    state.sort_menu_prev_selected_index = selected_index;
    state.menu_chord_mask = 0;
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.sort_menu_focus_anim_elapsed = SL_SORT_MENU_FOCUS_TWEEN_SECONDS;
    clear_preview(state);
    audio::play_sfx("assets/sounds/start.ogg");
}

#[inline(always)]
fn hide_sort_menu(state: &mut State) {
    state.sort_menu = SortMenuState::Hidden;
    state.menu_chord_mask = 0;
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
}

#[inline(always)]
fn try_open_sort_menu(state: &mut State) -> bool {
    if state.menu_chord_mask & (MENU_CHORD_LEFT | MENU_CHORD_RIGHT)
        == (MENU_CHORD_LEFT | MENU_CHORD_RIGHT)
    {
        // Simply Love parity: Left+Right / MenuLeft+MenuRight code opens SortMenu
        // without leaving the current wheel selection. Our input path moves on the
        // first press, so cancel that first move before opening the menu.
        match state.nav_key_held_direction {
            Some(NavDirection::Left) => music_wheel_change(state, 1),
            Some(NavDirection::Right) => music_wheel_change(state, -1),
            None => {}
        }
        show_sort_menu(state);
        true
    } else {
        false
    }
}

#[inline(always)]
fn sort_menu_items(state: &State) -> &[SortMenuItem] {
    if matches!(
        state.entries.get(state.selected_index),
        Some(MusicWheelEntry::Song(_))
    ) {
        &SORT_MENU_ITEMS
    } else {
        &SORT_MENU_ITEMS[..5]
    }
}

#[inline(always)]
fn set_text_clip_rect(actor: &mut Actor, rect: [f32; 4]) {
    if let Actor::Text { clip, .. } = actor {
        *clip = Some(rect);
    }
}

#[inline(always)]
fn sort_menu_scroll_dir(len: usize, prev: usize, selected: usize) -> isize {
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

#[derive(Default)]
struct SongSearchFilter {
    pack_term: Option<String>,
    song_term: Option<String>,
    difficulty: Option<u8>,
    bpm_tier: Option<i32>,
}

#[inline(always)]
fn song_search_total_items(results: &SongSearchResultsState) -> usize {
    results.candidates.len() + 1
}

fn song_search_move(results: &mut SongSearchResultsState, delta: isize) -> bool {
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
fn song_search_focused_candidate(results: &SongSearchResultsState) -> Option<&SongSearchCandidate> {
    results.candidates.get(results.selected_index)
}

#[inline(always)]
fn song_search_bpm_tier(bpm: f64) -> i32 {
    (((bpm + 0.5) / 10.0).floor() as i32) * 10
}

fn song_search_difficulties_text(song: &SongData, chart_type: &str) -> String {
    const ORDER: [&str; 5] = ["beginner", "easy", "medium", "hard", "challenge"];
    let mut out = String::new();
    for diff in ORDER {
        if let Some(chart) = song.charts.iter().find(|c| {
            c.chart_type.eq_ignore_ascii_case(chart_type)
                && c.difficulty.eq_ignore_ascii_case(diff)
                && !c.notes.is_empty()
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
        let pack = left.trim();
        let song = right.trim();
        if !pack.is_empty() {
            filter.pack_term = Some(pack.to_string());
        }
        if !song.is_empty() {
            filter.song_term = Some(song.to_string());
        }
    } else if !stripped.is_empty() {
        filter.song_term = Some(stripped.to_string());
    }
    filter
}

fn build_song_search_candidates(state: &State, search_text: &str) -> Vec<SongSearchCandidate> {
    let filter = parse_song_search_filter(search_text);
    let chart_type = profile::get_session_play_style().chart_type();
    let mut out = Vec::new();
    let mut current_pack_name: Option<&str> = None;

    for entry in &state.group_entries {
        match entry {
            MusicWheelEntry::PackHeader { name, .. } => {
                current_pack_name = Some(name.as_str());
            }
            MusicWheelEntry::Song(song) => {
                if !song
                    .charts
                    .iter()
                    .any(|c| c.chart_type.eq_ignore_ascii_case(chart_type) && !c.notes.is_empty())
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
                            && !c.notes.is_empty()
                            && c.meter == diff as u32
                    })
                {
                    continue;
                }

                if let Some(want_tier) = filter.bpm_tier {
                    let mut lo = song_search_bpm_tier(song.min_bpm);
                    let mut hi = song_search_bpm_tier(song.max_bpm);
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

fn start_song_search_prompt(state: &mut State) {
    clear_preview(state);
    state.sort_menu = SortMenuState::Hidden;
    state.leaderboard = LeaderboardOverlayState::Hidden;
    state.menu_chord_mask = 0;
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.song_search = SongSearchState::Prompt {
        query: String::new(),
    };
}

#[inline(always)]
fn close_song_search(state: &mut State) {
    state.song_search = SongSearchState::Hidden;
}

fn start_song_search_results(state: &mut State, search_text: String) {
    let trimmed = search_text.trim().to_string();
    if trimmed.is_empty() {
        close_song_search(state);
        return;
    }
    let candidates = build_song_search_candidates(state, &trimmed);
    state.song_search = SongSearchState::Results(SongSearchResultsState {
        search_text: trimmed,
        candidates,
        selected_index: 0,
        prev_selected_index: 0,
        focus_anim_elapsed: SL_SONG_SEARCH_FOCUS_TWEEN_SECONDS,
        input_lock: SL_SONG_SEARCH_INPUT_LOCK_SECONDS,
    });
}

fn focus_song_from_search(state: &mut State, song: &Arc<SongData>) {
    if let Some(index) = song_entry_index(&state.entries, song) {
        state.selected_index = index;
        state.time_since_selection_change = 0.0;
        state.wheel_offset_from_selection = 0.0;
        state.last_requested_banner_path = None;
        state.last_requested_chart_hash = None;
        state.last_requested_chart_hash_p2 = None;
        return;
    }

    if let Some(group_name) = group_name_for_song(&state.all_entries, song) {
        state.expanded_pack_name = Some(group_name);
        rebuild_displayed_entries(state);
        if let Some(index) = song_entry_index(&state.entries, song) {
            state.selected_index = index;
            state.time_since_selection_change = 0.0;
            state.wheel_offset_from_selection = 0.0;
            state.last_requested_banner_path = None;
            state.last_requested_chart_hash = None;
            state.last_requested_chart_hash_p2 = None;
            return;
        }
    }

    if state.sort_mode != WheelSortMode::Group {
        apply_wheel_sort(state, WheelSortMode::Group);
    }
    if let Some(group_name) = group_name_for_song(&state.group_entries, song) {
        state.expanded_pack_name = Some(group_name);
        rebuild_displayed_entries(state);
    }
    if let Some(index) = song_entry_index(&state.entries, song) {
        state.selected_index = index;
    } else {
        state.selected_index = state
            .selected_index
            .min(state.entries.len().saturating_sub(1));
    }
    state.time_since_selection_change = 0.0;
    state.wheel_offset_from_selection = 0.0;
    state.last_requested_banner_path = None;
    state.last_requested_chart_hash = None;
    state.last_requested_chart_hash_p2 = None;
}

fn start_reload_songs_and_courses(state: &mut State) {
    if state.reload_ui.is_some() {
        return;
    }

    clear_preview(state);
    state.sort_menu = SortMenuState::Hidden;
    state.leaderboard = LeaderboardOverlayState::Hidden;
    state.menu_chord_mask = 0;
    state.chord_mask_p1 = 0;
    state.chord_mask_p2 = 0;
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.last_steps_nav_dir_p1 = None;
    state.last_steps_nav_time_p1 = None;
    state.last_steps_nav_dir_p2 = None;
    state.last_steps_nav_time_p2 = None;

    let (tx, rx) = mpsc::channel::<ReloadMsg>();
    state.reload_ui = Some(ReloadUiState::new(rx));

    std::thread::spawn(move || {
        let _ = tx.send(ReloadMsg::Phase(ReloadPhase::Songs));

        let interval = Duration::from_millis(50);
        let mut last_sent = Instant::now() - interval;
        let mut on_song = |pack: &str, song: &str| {
            let now = Instant::now();
            if now.duration_since(last_sent) < interval {
                return;
            }
            last_sent = now;
            let _ = tx.send(ReloadMsg::Song {
                pack: pack.to_owned(),
                song: song.to_owned(),
            });
        };
        song_loading::scan_and_load_songs_with_progress("songs", &mut on_song);

        let _ = tx.send(ReloadMsg::Phase(ReloadPhase::Courses));

        let mut last_sent = Instant::now() - interval;
        let mut on_course = |group: &str, course: &str| {
            let now = Instant::now();
            if now.duration_since(last_sent) < interval {
                return;
            }
            last_sent = now;
            let _ = tx.send(ReloadMsg::Course {
                group: group.to_owned(),
                course: course.to_owned(),
            });
        };
        song_loading::scan_and_load_courses_with_progress("courses", "songs", &mut on_course);

        let _ = tx.send(ReloadMsg::Done);
    });
}

fn poll_reload_ui(reload: &mut ReloadUiState) {
    while let Ok(msg) = reload.rx.try_recv() {
        match msg {
            ReloadMsg::Phase(phase) => {
                reload.phase = phase;
                reload.line2.clear();
                reload.line3.clear();
            }
            ReloadMsg::Song { pack, song } => {
                reload.phase = ReloadPhase::Songs;
                reload.line2 = pack;
                reload.line3 = song;
            }
            ReloadMsg::Course { group, course } => {
                reload.phase = ReloadPhase::Courses;
                reload.line2 = group;
                reload.line3 = course;
            }
            ReloadMsg::Done => {
                reload.done = true;
            }
        }
    }
}

fn refresh_after_reload(state: &mut State) {
    let selected_song = selected_song_arc(state);
    let selected_simfile_path = selected_song.as_ref().map(|song| song.simfile_path.clone());
    let selected_pack_name = if let Some(song) = selected_song.as_ref() {
        group_name_for_song(&state.entries, song)
    } else {
        match state.entries.get(state.selected_index) {
            Some(MusicWheelEntry::PackHeader { name, .. }) => Some(name.clone()),
            _ => None,
        }
    };
    let target_chart_type = profile::get_session_play_style().chart_type();
    let selected_hash_p1 = selected_song
        .as_ref()
        .and_then(|song| chart_for_steps_index(song, target_chart_type, state.selected_steps_index))
        .map(|chart| chart.short_hash.clone());
    let selected_hash_p2 = selected_song
        .as_ref()
        .and_then(|song| {
            chart_for_steps_index(song, target_chart_type, state.p2_selected_steps_index)
        })
        .map(|chart| chart.short_hash.clone());

    let sort_mode = state.sort_mode;
    let expanded_pack_name = state.expanded_pack_name.clone();
    let active_color_index = state.active_color_index;
    let old_steps_index_p1 = state.selected_steps_index;
    let old_steps_index_p2 = state.p2_selected_steps_index;
    let preferred_difficulty_index = state.preferred_difficulty_index;
    let p2_preferred_difficulty_index = state.p2_preferred_difficulty_index;

    let mut refreshed = init();
    refreshed.active_color_index = active_color_index;
    refreshed.preferred_difficulty_index = preferred_difficulty_index;
    refreshed.p2_preferred_difficulty_index = p2_preferred_difficulty_index;

    if sort_mode != WheelSortMode::Group {
        apply_wheel_sort(&mut refreshed, sort_mode);
    }

    if let Some(expanded) = expanded_pack_name
        && refreshed.all_entries.iter().any(
            |entry| matches!(entry, MusicWheelEntry::PackHeader { name, .. } if name == &expanded),
        )
    {
        refreshed.expanded_pack_name = Some(expanded);
        rebuild_displayed_entries(&mut refreshed);
    }

    let mut restored = false;
    if let Some(simfile_path) = selected_simfile_path {
        if let Some(index) = refreshed.entries.iter().position(|entry| {
            matches!(entry, MusicWheelEntry::Song(song) if song.simfile_path == simfile_path)
        }) {
            refreshed.selected_index = index;
            restored = true;
        } else if let Some(pack_name) = selected_pack_name.as_ref()
            && refreshed.expanded_pack_name.as_deref() != Some(pack_name.as_str())
            && refreshed
                .all_entries
                .iter()
                .any(|entry| matches!(entry, MusicWheelEntry::PackHeader { name, .. } if name == pack_name))
        {
            refreshed.expanded_pack_name = Some(pack_name.clone());
            rebuild_displayed_entries(&mut refreshed);
            if let Some(index) = refreshed.entries.iter().position(|entry| {
                matches!(entry, MusicWheelEntry::Song(song) if song.simfile_path == simfile_path)
            }) {
                refreshed.selected_index = index;
                restored = true;
            }
        }
    }

    if !restored
        && let Some(pack_name) = selected_pack_name
        && let Some(index) = refreshed.entries.iter().position(
            |entry| matches!(entry, MusicWheelEntry::PackHeader { name, .. } if name == &pack_name),
        )
    {
        refreshed.selected_index = index;
    }

    refreshed.selected_index = refreshed
        .selected_index
        .min(refreshed.entries.len().saturating_sub(1));
    refreshed.prev_selected_index = refreshed.selected_index;
    refreshed.time_since_selection_change = 0.0;
    refreshed.wheel_offset_from_selection = 0.0;

    if let Some(MusicWheelEntry::Song(song)) = refreshed.entries.get(refreshed.selected_index) {
        let mut restored_p1 = false;
        if let Some(hash) = selected_hash_p1.as_deref()
            && let Some(index) = steps_index_for_chart_hash(song, target_chart_type, hash)
        {
            refreshed.selected_steps_index = index;
            if index < color::FILE_DIFFICULTY_NAMES.len() {
                refreshed.preferred_difficulty_index = index;
            }
            restored_p1 = true;
        }
        if !restored_p1
            && chart_for_steps_index(song, target_chart_type, old_steps_index_p1).is_some()
        {
            refreshed.selected_steps_index = old_steps_index_p1;
        }

        let mut restored_p2 = false;
        if let Some(hash) = selected_hash_p2.as_deref()
            && let Some(index) = steps_index_for_chart_hash(song, target_chart_type, hash)
        {
            refreshed.p2_selected_steps_index = index;
            if index < color::FILE_DIFFICULTY_NAMES.len() {
                refreshed.p2_preferred_difficulty_index = index;
            }
            restored_p2 = true;
        }
        if !restored_p2
            && chart_for_steps_index(song, target_chart_type, old_steps_index_p2).is_some()
        {
            refreshed.p2_selected_steps_index = old_steps_index_p2;
        }
    }

    trigger_immediate_refresh(&mut refreshed);
    *state = refreshed;
}

fn sort_menu_move(state: &mut State, delta: isize) {
    let len = sort_menu_items(state).len();
    let SortMenuState::Visible { selected_index } = &mut state.sort_menu else {
        return;
    };
    if len == 0 {
        return;
    }
    let old = *selected_index;
    let next = ((*selected_index as isize + delta).rem_euclid(len as isize)) as usize;
    if next != old {
        state.sort_menu_prev_selected_index = old;
        *selected_index = next;
        state.sort_menu_focus_anim_elapsed = 0.0;
        audio::play_sfx("assets/sounds/change.ogg");
    }
}

#[inline(always)]
fn selected_chart_hash_for_side(
    state: &State,
    song: &SongData,
    side: profile::PlayerSide,
) -> Option<String> {
    let target_chart_type = profile::get_session_play_style().chart_type();
    let steps_index = match (profile::get_session_play_style(), side) {
        (profile::PlayStyle::Versus, profile::PlayerSide::P2) => state.p2_selected_steps_index,
        _ => state.selected_steps_index,
    };
    chart_for_steps_index(song, target_chart_type, steps_index).map(|c| c.short_hash.clone())
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

fn apply_leaderboard_side_fetch_result(
    side: &mut LeaderboardSideState,
    fetched: Result<scores::PlayerLeaderboardData, String>,
) {
    side.loading = false;
    match fetched {
        Ok(data) => {
            side.error_text = None;
            side.panes = data.panes;
            if let Some(machine) = side.machine_pane.clone() {
                side.panes.push(machine);
            }
            if side.panes.is_empty()
                && let Some(machine) = side.machine_pane.clone()
            {
                side.panes.push(machine);
            }
            side.pane_index = 0;
            side.show_icons = side.panes.len() > 1;
        }
        Err(error) => {
            side.error_text = Some(gs_error_text(&error));
            if side.panes.is_empty()
                && let Some(machine) = side.machine_pane.clone()
            {
                side.panes.push(machine);
            }
            side.pane_index = 0;
            side.show_icons = false;
        }
    }
}

fn show_leaderboard_overlay(state: &mut State) {
    let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) else {
        return;
    };

    let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
    let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
    if !p1_joined && !p2_joined {
        return;
    }

    let chart_hash_p1 = selected_chart_hash_for_side(state, song, profile::PlayerSide::P1);
    let chart_hash_p2 = selected_chart_hash_for_side(state, song, profile::PlayerSide::P2);

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

    let mut req_p1: Option<LeaderboardFetchRequest> = None;
    if p1_joined {
        let profile = profile::get_for_side(profile::PlayerSide::P1);
        if service && !profile.groovestats_api_key.is_empty() && chart_hash_p1.is_some() {
            req_p1 = Some(LeaderboardFetchRequest {
                chart_hash: chart_hash_p1.unwrap_or_default(),
                api_key: profile.groovestats_api_key,
                show_ex_score: profile.show_ex_score,
            });
            p1.loading = true;
        } else if let Some(machine) = p1.machine_pane.clone() {
            p1.panes.push(machine);
            if service_disabled {
                p1.panes.push(gs_disabled_pane());
            }
            p1.show_icons = false;
        }
    }

    let mut req_p2: Option<LeaderboardFetchRequest> = None;
    if p2_joined {
        let profile = profile::get_for_side(profile::PlayerSide::P2);
        if service && !profile.groovestats_api_key.is_empty() && chart_hash_p2.is_some() {
            req_p2 = Some(LeaderboardFetchRequest {
                chart_hash: chart_hash_p2.unwrap_or_default(),
                api_key: profile.groovestats_api_key,
                show_ex_score: profile.show_ex_score,
            });
            p2.loading = true;
        } else if let Some(machine) = p2.machine_pane.clone() {
            p2.panes.push(machine);
            if service_disabled {
                p2.panes.push(gs_disabled_pane());
            }
            p2.show_icons = false;
        }
    }

    let mut rx = None;
    if req_p1.is_some() || req_p2.is_some() {
        let (tx, thread_rx) = mpsc::channel::<LeaderboardFetchResult>();
        std::thread::spawn(move || {
            let p1_res = req_p1.map(|r| {
                scores::fetch_player_leaderboards(
                    &r.chart_hash,
                    &r.api_key,
                    r.show_ex_score,
                    GS_LEADERBOARD_NUM_ENTRIES,
                )
                .map_err(|e| e.to_string())
            });
            let p2_res = req_p2.map(|r| {
                scores::fetch_player_leaderboards(
                    &r.chart_hash,
                    &r.api_key,
                    r.show_ex_score,
                    GS_LEADERBOARD_NUM_ENTRIES,
                )
                .map_err(|e| e.to_string())
            });
            let _ = tx.send(LeaderboardFetchResult {
                p1: p1_res,
                p2: p2_res,
            });
        });
        rx = Some(thread_rx);
    }

    state.leaderboard = LeaderboardOverlayState::Visible(LeaderboardOverlayStateData {
        elapsed: 0.0,
        p1,
        p2,
        rx,
    });
    clear_preview(state);
}

#[inline(always)]
fn hide_leaderboard_overlay(state: &mut State) {
    state.leaderboard = LeaderboardOverlayState::Hidden;
}

fn poll_leaderboard_overlay(state: &mut State) {
    let LeaderboardOverlayState::Visible(overlay) = &mut state.leaderboard else {
        return;
    };
    let Some(rx) = &overlay.rx else {
        return;
    };
    let Ok(result) = rx.try_recv() else {
        return;
    };

    if let Some(p1_result) = result.p1 {
        apply_leaderboard_side_fetch_result(&mut overlay.p1, p1_result);
    }
    if let Some(p2_result) = result.p2 {
        apply_leaderboard_side_fetch_result(&mut overlay.p2, p2_result);
    }
    overlay.rx = None;
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

fn handle_leaderboard_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }
    let LeaderboardOverlayState::Visible(overlay) = &mut state.leaderboard else {
        return ScreenAction::None;
    };

    match ev.action {
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            if overlay.p1.joined && leaderboard_shift(&mut overlay.p1, -1) {
                audio::play_sfx("assets/sounds/change.ogg");
            }
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            if overlay.p1.joined && leaderboard_shift(&mut overlay.p1, 1) {
                audio::play_sfx("assets/sounds/change.ogg");
            }
        }
        VirtualAction::p2_left | VirtualAction::p2_menu_left => {
            if overlay.p2.joined && leaderboard_shift(&mut overlay.p2, -1) {
                audio::play_sfx("assets/sounds/change.ogg");
            }
        }
        VirtualAction::p2_right | VirtualAction::p2_menu_right => {
            if overlay.p2.joined && leaderboard_shift(&mut overlay.p2, 1) {
                audio::play_sfx("assets/sounds/change.ogg");
            }
        }
        VirtualAction::p1_start
        | VirtualAction::p2_start
        | VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            audio::play_sfx("assets/sounds/start.ogg");
            hide_leaderboard_overlay(state);
        }
        _ => {}
    }

    ScreenAction::None
}

fn sort_menu_activate(state: &mut State) {
    let SortMenuState::Visible { selected_index } = state.sort_menu else {
        return;
    };
    let items = sort_menu_items(state);
    if items.is_empty() {
        hide_sort_menu(state);
        return;
    }
    let selected_index = selected_index.min(items.len() - 1);
    audio::play_sfx("assets/sounds/start.ogg");
    match items[selected_index].action {
        SortMenuAction::SortByGroup => {
            apply_wheel_sort(state, WheelSortMode::Group);
            hide_sort_menu(state);
        }
        SortMenuAction::SortByTitle => {
            apply_wheel_sort(state, WheelSortMode::Title);
            hide_sort_menu(state);
        }
        SortMenuAction::SortByRecent => {
            apply_wheel_sort(state, WheelSortMode::Recent);
            hide_sort_menu(state);
        }
        SortMenuAction::SongSearch => {
            hide_sort_menu(state);
            start_song_search_prompt(state);
        }
        SortMenuAction::ReloadSongsCourses => {
            hide_sort_menu(state);
            start_reload_songs_and_courses(state);
        }
        SortMenuAction::ShowLeaderboard => {
            hide_sort_menu(state);
            show_leaderboard_overlay(state);
        }
    }
}

fn handle_sort_menu_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }
    match ev.action {
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => sort_menu_move(state, -1),
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => sort_menu_move(state, 1),
        VirtualAction::p1_start | VirtualAction::p2_start => sort_menu_activate(state),
        VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            audio::play_sfx("assets/sounds/start.ogg");
            hide_sort_menu(state);
        }
        _ => {}
    }
    ScreenAction::None
}

fn handle_song_search_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }

    let mut prompt_start: Option<String> = None;
    let mut prompt_close = false;
    match &mut state.song_search {
        SongSearchState::Prompt { query } => match ev.action {
            VirtualAction::p1_start | VirtualAction::p2_start => {
                audio::play_sfx("assets/sounds/start.ogg");
                prompt_start = Some(query.clone());
            }
            VirtualAction::p1_back
            | VirtualAction::p2_back
            | VirtualAction::p1_select
            | VirtualAction::p2_select => {
                audio::play_sfx("assets/sounds/start.ogg");
                prompt_close = true;
            }
            _ => {}
        },
        SongSearchState::Results(results) => {
            if results.input_lock > 0.0 {
                return ScreenAction::None;
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
                    if song_search_move(results, -1) {
                        audio::play_sfx("assets/sounds/change.ogg");
                    }
                }
                VirtualAction::p1_down
                | VirtualAction::p1_menu_down
                | VirtualAction::p1_right
                | VirtualAction::p1_menu_right
                | VirtualAction::p2_down
                | VirtualAction::p2_menu_down
                | VirtualAction::p2_right
                | VirtualAction::p2_menu_right => {
                    if song_search_move(results, 1) {
                        audio::play_sfx("assets/sounds/change.ogg");
                    }
                }
                VirtualAction::p1_start | VirtualAction::p2_start => {
                    let picked = song_search_focused_candidate(results).map(|c| c.song.clone());
                    audio::play_sfx("assets/sounds/start.ogg");
                    close_song_search(state);
                    if let Some(song) = picked {
                        focus_song_from_search(state, &song);
                    }
                }
                VirtualAction::p1_back
                | VirtualAction::p2_back
                | VirtualAction::p1_select
                | VirtualAction::p2_select => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    close_song_search(state);
                }
                _ => {}
            }
        }
        SongSearchState::Hidden => {}
    }

    if let Some(search_text) = prompt_start {
        start_song_search_results(state, search_text);
        return ScreenAction::None;
    }
    if prompt_close {
        close_song_search(state);
        return ScreenAction::None;
    }

    ScreenAction::None
}

pub fn handle_pad_dir(state: &mut State, dir: PadDir, pressed: bool) -> ScreenAction {
    if pressed {
        match dir {
            PadDir::Right => {
                // Simply Love [ScreenSelectMusic]: CodeSortList4 = "Left-Right".
                state.menu_chord_mask |= MENU_CHORD_RIGHT;
                if try_open_sort_menu(state) {
                    return ScreenAction::None;
                }
                if state.nav_key_held_direction == Some(NavDirection::Right) {
                    return ScreenAction::None;
                }
                music_wheel_change(state, 1);
                state.nav_key_held_direction = Some(NavDirection::Right);
                state.nav_key_held_since = Some(Instant::now());
            }
            PadDir::Left => {
                state.menu_chord_mask |= MENU_CHORD_LEFT;
                if try_open_sort_menu(state) {
                    return ScreenAction::None;
                }
                if state.nav_key_held_direction == Some(NavDirection::Left) {
                    return ScreenAction::None;
                }
                music_wheel_change(state, -1);
                state.nav_key_held_direction = Some(NavDirection::Left);
                state.nav_key_held_since = Some(Instant::now());
            }
            PadDir::Up | PadDir::Down => {
                if let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) {
                    let is_up = matches!(dir, PadDir::Up);
                    let now = Instant::now();

                    if state.last_steps_nav_dir_p1 == Some(dir)
                        && state
                            .last_steps_nav_time_p1
                            .is_some_and(|t| now.duration_since(t) < DOUBLE_TAP_WINDOW)
                    {
                        let target_chart_type = profile::get_session_play_style().chart_type();
                        let list_len = steps_len(song, target_chart_type);
                        let cur = state.selected_steps_index.min(list_len.saturating_sub(1));

                        let mut new_idx = None;
                        if is_up {
                            for i in (0..cur).rev() {
                                if chart_for_steps_index(song, target_chart_type, i).is_some() {
                                    new_idx = Some(i);
                                    break;
                                }
                            }
                        } else {
                            for i in (cur + 1)..list_len {
                                if chart_for_steps_index(song, target_chart_type, i).is_some() {
                                    new_idx = Some(i);
                                    break;
                                }
                            }
                        }

                        if let Some(new_idx) = new_idx {
                            state.selected_steps_index = new_idx;
                            if new_idx < color::FILE_DIFFICULTY_NAMES.len() {
                                state.preferred_difficulty_index = new_idx;
                            }
                            audio::play_sfx(if is_up {
                                "assets/sounds/easier.ogg"
                            } else {
                                "assets/sounds/harder.ogg"
                            });
                        }

                        state.last_steps_nav_dir_p1 = None;
                        state.last_steps_nav_time_p1 = None;
                    } else {
                        state.last_steps_nav_dir_p1 = Some(dir);
                        state.last_steps_nav_time_p1 = Some(now);
                    }

                    state.chord_mask_p1 |= chord_bit(dir);

                    // Combo check
                    if state.chord_mask_p1 & (CHORD_UP | CHORD_DOWN) == (CHORD_UP | CHORD_DOWN) {
                        if let Some(pack) = state.expanded_pack_name.take() {
                            info!("Up+Down combo: Collapsing pack '{}'.", pack);
                            rebuild_displayed_entries(state);
                            if let Some(new_sel) = state.entries.iter().position(|e| matches!(e, MusicWheelEntry::PackHeader { name, .. } if name == &pack)) {
                                state.selected_index = new_sel;
                                state.prev_selected_index = new_sel;
                                state.time_since_selection_change = 0.0;
                            }
                        }
                    }
                }
            }
        }
    } else {
        match dir {
            PadDir::Up => {
                state.chord_mask_p1 &= !CHORD_UP;
            }
            PadDir::Down => {
                state.chord_mask_p1 &= !CHORD_DOWN;
            }
            PadDir::Left => {
                state.menu_chord_mask &= !MENU_CHORD_LEFT;
                if state.nav_key_held_direction == Some(NavDirection::Left) {
                    let now = Instant::now();
                    let moving_started = state
                        .nav_key_held_since
                        .is_some_and(|t| now.duration_since(t) >= NAV_INITIAL_HOLD_DELAY);
                    if moving_started
                        && state.wheel_offset_from_selection.abs()
                            < MUSIC_WHEEL_STOP_SPINDOWN_THRESHOLD
                    {
                        music_wheel_change(state, -1);
                    }
                    state.nav_key_held_direction = None;
                    state.nav_key_held_since = None;
                }
            }
            PadDir::Right => {
                state.menu_chord_mask &= !MENU_CHORD_RIGHT;
                if state.nav_key_held_direction == Some(NavDirection::Right) {
                    let now = Instant::now();
                    let moving_started = state
                        .nav_key_held_since
                        .is_some_and(|t| now.duration_since(t) >= NAV_INITIAL_HOLD_DELAY);
                    if moving_started
                        && state.wheel_offset_from_selection.abs()
                            < MUSIC_WHEEL_STOP_SPINDOWN_THRESHOLD
                    {
                        music_wheel_change(state, 1);
                    }
                    state.nav_key_held_direction = None;
                    state.nav_key_held_since = None;
                }
            }
        }
    }
    ScreenAction::None
}

fn handle_pad_dir_p2(state: &mut State, dir: PadDir, pressed: bool) -> ScreenAction {
    if !(matches!(dir, PadDir::Up | PadDir::Down)) {
        return ScreenAction::None;
    }
    if pressed {
        if let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) {
            let is_up = matches!(dir, PadDir::Up);
            let now = Instant::now();

            if state.last_steps_nav_dir_p2 == Some(dir)
                && state
                    .last_steps_nav_time_p2
                    .is_some_and(|t| now.duration_since(t) < DOUBLE_TAP_WINDOW)
            {
                let target_chart_type = profile::get_session_play_style().chart_type();
                let list_len = steps_len(song, target_chart_type);
                let cur = state
                    .p2_selected_steps_index
                    .min(list_len.saturating_sub(1));

                let mut new_idx = None;
                if is_up {
                    for i in (0..cur).rev() {
                        if chart_for_steps_index(song, target_chart_type, i).is_some() {
                            new_idx = Some(i);
                            break;
                        }
                    }
                } else {
                    for i in (cur + 1)..list_len {
                        if chart_for_steps_index(song, target_chart_type, i).is_some() {
                            new_idx = Some(i);
                            break;
                        }
                    }
                }

                if let Some(new_idx) = new_idx {
                    state.p2_selected_steps_index = new_idx;
                    if new_idx < color::FILE_DIFFICULTY_NAMES.len() {
                        state.p2_preferred_difficulty_index = new_idx;
                    }
                    audio::play_sfx(if is_up {
                        "assets/sounds/easier.ogg"
                    } else {
                        "assets/sounds/harder.ogg"
                    });
                }

                state.last_steps_nav_dir_p2 = None;
                state.last_steps_nav_time_p2 = None;
            } else {
                state.last_steps_nav_dir_p2 = Some(dir);
                state.last_steps_nav_time_p2 = Some(now);
            }

            state.chord_mask_p2 |= chord_bit(dir);

            // Combo check
            if state.chord_mask_p2 & (CHORD_UP | CHORD_DOWN) == (CHORD_UP | CHORD_DOWN) {
                if let Some(pack) = state.expanded_pack_name.take() {
                    info!("Up+Down combo: Collapsing pack '{}'.", pack);
                    rebuild_displayed_entries(state);
                    if let Some(new_sel) = state.entries.iter().position(
                        |e| matches!(e, MusicWheelEntry::PackHeader { name, .. } if name == &pack),
                    ) {
                        state.selected_index = new_sel;
                        state.prev_selected_index = new_sel;
                        state.time_since_selection_change = 0.0;
                    }
                }
            }
        }
    } else {
        match dir {
            PadDir::Up => state.chord_mask_p2 &= !CHORD_UP,
            PadDir::Down => state.chord_mask_p2 &= !CHORD_DOWN,
            _ => {}
        }
    }
    ScreenAction::None
}

pub fn handle_confirm(state: &mut State) -> ScreenAction {
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    if state.out_prompt != OutPromptState::None {
        return ScreenAction::None;
    }
    if state.entries.is_empty() {
        audio::play_sfx("assets/sounds/expand.ogg");
        return ScreenAction::None;
    }
    match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(_)) => {
            audio::play_sfx("assets/sounds/start.ogg");
            state.out_prompt = OutPromptState::PressStartForOptions { elapsed: 0.0 };
            ScreenAction::None
        }
        Some(MusicWheelEntry::PackHeader { name, .. }) => {
            audio::play_sfx("assets/sounds/expand.ogg");
            let target = name.clone();
            if state.expanded_pack_name.as_ref() == Some(&target) {
                state.expanded_pack_name = None;
            } else {
                state.expanded_pack_name = Some(target.clone());
            }
            rebuild_displayed_entries(state);
            if let Some(new_sel) = state.entries.iter().position(
                |e| matches!(e, MusicWheelEntry::PackHeader { name, .. } if name == &target),
            ) {
                state.selected_index = new_sel;
            } else {
                state.selected_index = 0;
            }
            state.prev_selected_index = state.selected_index;
            state.time_since_selection_change = 0.0;
            ScreenAction::None
        }
        None => ScreenAction::None,
    }
}

pub fn handle_raw_key_event(state: &mut State, key: &KeyEvent) -> ScreenAction {
    if state.reload_ui.is_some() {
        return ScreenAction::None;
    }

    if key.state == ElementState::Pressed {
        if matches!(state.song_search, SongSearchState::Results(_))
            && let winit::keyboard::PhysicalKey::Code(KeyCode::Escape) = key.physical_key
        {
            audio::play_sfx("assets/sounds/start.ogg");
            close_song_search(state);
            return ScreenAction::None;
        }
        let mut prompt_start: Option<String> = None;
        let mut prompt_close = false;
        if let SongSearchState::Prompt { query } = &mut state.song_search {
            if let winit::keyboard::PhysicalKey::Code(code) = key.physical_key {
                match code {
                    KeyCode::Backspace => {
                        let _ = query.pop();
                        return ScreenAction::None;
                    }
                    KeyCode::Escape => {
                        audio::play_sfx("assets/sounds/start.ogg");
                        prompt_close = true;
                    }
                    KeyCode::Enter | KeyCode::NumpadEnter => {
                        audio::play_sfx("assets/sounds/start.ogg");
                        prompt_start = Some(query.clone());
                    }
                    _ => {}
                }
            }

            if !prompt_close
                && prompt_start.is_none()
                && let Some(text) = key.text.as_ref()
            {
                let mut len = query.chars().count();
                for ch in text.chars() {
                    if ch.is_control() {
                        continue;
                    }
                    if len >= SL_SONG_SEARCH_PROMPT_MAX_LEN {
                        break;
                    }
                    query.push(ch);
                    len += 1;
                }
            }

            if let Some(search_text) = prompt_start {
                start_song_search_results(state, search_text);
                return ScreenAction::None;
            }
            if prompt_close {
                close_song_search(state);
                return ScreenAction::None;
            }
            return ScreenAction::None;
        }
    }

    if key.state != ElementState::Pressed {
        return ScreenAction::None;
    }
    if let winit::keyboard::PhysicalKey::Code(KeyCode::F7) = key.physical_key {
        let target_chart_type = profile::get_session_play_style().chart_type();
        if let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) {
            if let Some(chart) =
                chart_for_steps_index(song, target_chart_type, state.selected_steps_index)
            {
                return ScreenAction::FetchOnlineGrade(chart.short_hash.clone());
            }
        }
    }
    ScreenAction::None
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if state.reload_ui.is_some() {
        return ScreenAction::None;
    }

    if state.out_prompt != OutPromptState::None {
        if ev.pressed
            && matches!(ev.action, VirtualAction::p1_start | VirtualAction::p2_start)
            && matches!(
                state.out_prompt,
                OutPromptState::PressStartForOptions { .. }
            )
        {
            audio::play_sfx("assets/sounds/start.ogg");
            state.out_prompt = OutPromptState::EnteringOptions { elapsed: 0.0 };
        }
        return ScreenAction::None;
    }

    if !matches!(state.song_search, SongSearchState::Hidden) {
        return handle_song_search_input(state, ev);
    }

    if state.exit_prompt != ExitPromptState::None {
        return handle_exit_prompt_input(state, ev);
    }

    if !matches!(state.leaderboard, LeaderboardOverlayState::Hidden) {
        return handle_leaderboard_input(state, ev);
    }

    if state.sort_menu != SortMenuState::Hidden {
        return handle_sort_menu_input(state, ev);
    }

    let play_style = crate::game::profile::get_session_play_style();
    if play_style == crate::game::profile::PlayStyle::Versus {
        return match ev.action {
            VirtualAction::p1_left | VirtualAction::p1_menu_left => {
                handle_pad_dir(state, PadDir::Left, ev.pressed)
            }
            VirtualAction::p1_right | VirtualAction::p1_menu_right => {
                handle_pad_dir(state, PadDir::Right, ev.pressed)
            }
            VirtualAction::p1_up | VirtualAction::p1_menu_up => {
                handle_pad_dir(state, PadDir::Up, ev.pressed)
            }
            VirtualAction::p1_down | VirtualAction::p1_menu_down => {
                handle_pad_dir(state, PadDir::Down, ev.pressed)
            }
            VirtualAction::p1_start if ev.pressed => handle_confirm(state),
            VirtualAction::p1_back if ev.pressed => {
                begin_exit_prompt(state);
                ScreenAction::None
            }

            VirtualAction::p2_left | VirtualAction::p2_menu_left => {
                handle_pad_dir(state, PadDir::Left, ev.pressed)
            }
            VirtualAction::p2_right | VirtualAction::p2_menu_right => {
                handle_pad_dir(state, PadDir::Right, ev.pressed)
            }
            VirtualAction::p2_up | VirtualAction::p2_menu_up => {
                handle_pad_dir_p2(state, PadDir::Up, ev.pressed)
            }
            VirtualAction::p2_down | VirtualAction::p2_menu_down => {
                handle_pad_dir_p2(state, PadDir::Down, ev.pressed)
            }
            VirtualAction::p2_start if ev.pressed => handle_confirm(state),
            VirtualAction::p2_back if ev.pressed => {
                begin_exit_prompt(state);
                ScreenAction::None
            }
            _ => ScreenAction::None,
        };
    }

    match crate::game::profile::get_session_player_side() {
        crate::game::profile::PlayerSide::P2 => match ev.action {
            VirtualAction::p2_left | VirtualAction::p2_menu_left => {
                handle_pad_dir(state, PadDir::Left, ev.pressed)
            }
            VirtualAction::p2_right | VirtualAction::p2_menu_right => {
                handle_pad_dir(state, PadDir::Right, ev.pressed)
            }
            VirtualAction::p2_up | VirtualAction::p2_menu_up => {
                handle_pad_dir(state, PadDir::Up, ev.pressed)
            }
            VirtualAction::p2_down | VirtualAction::p2_menu_down => {
                handle_pad_dir(state, PadDir::Down, ev.pressed)
            }
            VirtualAction::p2_start if ev.pressed => handle_confirm(state),
            VirtualAction::p2_back if ev.pressed => {
                begin_exit_prompt(state);
                ScreenAction::None
            }
            _ => ScreenAction::None,
        },
        crate::game::profile::PlayerSide::P1 => match ev.action {
            VirtualAction::p1_left | VirtualAction::p1_menu_left => {
                handle_pad_dir(state, PadDir::Left, ev.pressed)
            }
            VirtualAction::p1_right | VirtualAction::p1_menu_right => {
                handle_pad_dir(state, PadDir::Right, ev.pressed)
            }
            VirtualAction::p1_up | VirtualAction::p1_menu_up => {
                handle_pad_dir(state, PadDir::Up, ev.pressed)
            }
            VirtualAction::p1_down | VirtualAction::p1_menu_down => {
                handle_pad_dir(state, PadDir::Down, ev.pressed)
            }
            VirtualAction::p1_start if ev.pressed => handle_confirm(state),
            VirtualAction::p1_back if ev.pressed => {
                begin_exit_prompt(state);
                ScreenAction::None
            }
            _ => ScreenAction::None,
        },
    }
}

pub fn update(state: &mut State, dt: f32) -> ScreenAction {
    if state.reload_ui.is_some() {
        let done = {
            let reload = state.reload_ui.as_mut().unwrap();
            poll_reload_ui(reload);
            reload.done
        };
        if done {
            state.reload_ui = None;
            refresh_after_reload(state);
        }
        return ScreenAction::None;
    }

    if let SongSearchState::Results(results) = &mut state.song_search {
        let dt = dt.max(0.0);
        results.input_lock = (results.input_lock - dt).max(0.0);
        if results.focus_anim_elapsed < SL_SONG_SEARCH_FOCUS_TWEEN_SECONDS {
            results.focus_anim_elapsed =
                (results.focus_anim_elapsed + dt).min(SL_SONG_SEARCH_FOCUS_TWEEN_SECONDS);
        }
        return ScreenAction::None;
    }
    if matches!(state.song_search, SongSearchState::Prompt { .. }) {
        return ScreenAction::None;
    }

    match state.out_prompt {
        OutPromptState::PressStartForOptions { elapsed } => {
            let elapsed = elapsed + dt.max(0.0);
            if elapsed >= SHOW_OPTIONS_MESSAGE_SECONDS {
                state.out_prompt = OutPromptState::None;
                return ScreenAction::NavigateNoFade(Screen::Gameplay);
            }
            state.out_prompt = OutPromptState::PressStartForOptions { elapsed };
            return ScreenAction::None;
        }
        OutPromptState::EnteringOptions { elapsed } => {
            let elapsed = elapsed + dt.max(0.0);
            if elapsed >= ENTERING_OPTIONS_TOTAL_SECONDS {
                state.out_prompt = OutPromptState::None;
                return ScreenAction::NavigateNoFade(Screen::PlayerOptions);
            }
            state.out_prompt = OutPromptState::EnteringOptions { elapsed };
            return ScreenAction::None;
        }
        OutPromptState::None => {}
    }

    if let ExitPromptState::Active {
        elapsed,
        switch_from,
        switch_elapsed,
        ..
    } = &mut state.exit_prompt
    {
        let dt = dt.max(0.0);
        *elapsed += dt;
        if switch_from.is_some() {
            *switch_elapsed += dt;
            if *switch_elapsed >= SL_EXIT_PROMPT_CHOICE_TWEEN_SECONDS {
                *switch_from = None;
                *switch_elapsed = 0.0;
            }
        }
    }

    if let LeaderboardOverlayState::Visible(overlay) = &mut state.leaderboard {
        overlay.elapsed += dt.max(0.0);
    }
    poll_leaderboard_overlay(state);

    state.time_since_selection_change += dt;
    if dt > 0.0 {
        state.selection_animation_timer += dt;
        if state.sort_menu != SortMenuState::Hidden
            && state.sort_menu_focus_anim_elapsed < SL_SORT_MENU_FOCUS_TWEEN_SECONDS
        {
            state.sort_menu_focus_anim_elapsed =
                (state.sort_menu_focus_anim_elapsed + dt).min(SL_SORT_MENU_FOCUS_TWEEN_SECONDS);
        }
    }

    let now = Instant::now();
    let wheel_moving = state
        .nav_key_held_since
        .is_some_and(|t| now.duration_since(t) >= NAV_INITIAL_HOLD_DELAY);
    if wheel_moving {
        match state.nav_key_held_direction.clone() {
            Some(dir) => music_wheel_update_hold_scroll(state, dt, dir),
            None => music_wheel_settle_offset(state, dt),
        };
    } else {
        music_wheel_settle_offset(state, dt);
    }

    if state.selected_index != state.prev_selected_index {
        audio::play_sfx("assets/sounds/change.ogg");
        state.prev_selected_index = state.selected_index;
        state.time_since_selection_change = 0.0;

        if matches!(
            state.entries.get(state.selected_index),
            Some(MusicWheelEntry::PackHeader { .. })
        ) {
            state.displayed_chart_p1 = None;
            state.displayed_chart_p2 = None;
        }

        if let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) {
            let pref = state.preferred_difficulty_index;
            let mut best = None;
            let mut min = i32::MAX;
            for i in 0..color::FILE_DIFFICULTY_NAMES.len() {
                if is_difficulty_playable(song, i) {
                    let diff = (i as i32 - pref as i32).abs();
                    if diff < min {
                        min = diff;
                        best = Some(i);
                    }
                }
            }
            if let Some(b) = best {
                state.selected_steps_index = b;
            }

            let pref2 = state.p2_preferred_difficulty_index;
            let mut best2 = None;
            let mut min2 = i32::MAX;
            for i in 0..color::FILE_DIFFICULTY_NAMES.len() {
                if is_difficulty_playable(song, i) {
                    let diff = (i as i32 - pref2 as i32).abs();
                    if diff < min2 {
                        min2 = diff;
                        best2 = Some(i);
                    }
                }
            }
            if let Some(b) = best2 {
                state.p2_selected_steps_index = b;
            }
        }
    }

    if state.sort_menu != SortMenuState::Hidden
        || !matches!(state.leaderboard, LeaderboardOverlayState::Hidden)
    {
        if state.currently_playing_preview_path.is_some() {
            clear_preview(state);
        }
        return ScreenAction::None;
    }

    // --- Immediate Updates ---
    let (selected_song, selected_pack) = match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(s)) => (Some(s.clone()), None),
        Some(MusicWheelEntry::PackHeader {
            name, banner_path, ..
        }) => (None, Some((name, banner_path))),
        None => (None, None),
    };

    let new_banner = selected_song
        .as_ref()
        .and_then(|s| s.banner_path.clone())
        .or_else(|| {
            selected_pack
                .as_ref()
                .and_then(|(_, p)| p.as_ref().cloned())
        });

    if state.last_requested_banner_path != new_banner {
        state.last_requested_banner_path = new_banner.clone();
        return ScreenAction::RequestBanner(new_banner);
    }

    // --- Delayed Updates ---
    if state.time_since_selection_change >= PREVIEW_DELAY_SECONDS {
        let music_path = selected_song.as_ref().and_then(|s| s.music_path.clone());
        if state.currently_playing_preview_path != music_path {
            state.currently_playing_preview_path = music_path;
            if let Some(song) = &selected_song {
                if let Some((path, cut)) = compute_preview_cut(song) {
                    state.currently_playing_preview_start_sec = Some(cut.start_sec as f32);
                    state.currently_playing_preview_length_sec = Some(cut.length_sec as f32);
                    audio::play_music(
                        path,
                        cut,
                        true,
                        crate::game::profile::get_session_music_rate(),
                    );
                } else {
                    state.currently_playing_preview_start_sec = None;
                    state.currently_playing_preview_length_sec = None;
                    audio::stop_music();
                }
            } else {
                state.currently_playing_preview_start_sec = None;
                state.currently_playing_preview_length_sec = None;
                audio::stop_music();
            }
        }

        let play_style = profile::get_session_play_style();
        let target_chart_type = play_style.chart_type();

        if let Some(song) = selected_song.as_ref() {
            let is_versus = play_style == crate::game::profile::PlayStyle::Versus;
            ensure_chart_cache_for_song(state, song, target_chart_type, is_versus);

            let desired_hash_p1 = state
                .cached_chart_ix_p1
                .map(|ix| song.charts[ix].short_hash.as_str());

            if state
                .displayed_chart_p1
                .as_ref()
                .map(|d| d.chart_hash.as_str())
                != desired_hash_p1
            {
                state.displayed_chart_p1 = desired_hash_p1.map(|h| DisplayedChart {
                    song: song.clone(),
                    chart_hash: h.to_string(),
                });
            }

            if state.last_requested_chart_hash.as_deref() != desired_hash_p1 {
                state.last_requested_chart_hash = desired_hash_p1.map(str::to_string);
                return ScreenAction::RequestDensityGraph {
                    slot: DensityGraphSlot::SelectMusicP1,
                    chart_opt: state.cached_chart_ix_p1.map(|ix| {
                        let c = &song.charts[ix];
                        DensityGraphSource {
                            max_nps: c.max_nps,
                            measure_nps_vec: c.measure_nps_vec.clone(),
                            timing: c.timing.clone(),
                            first_second: 0.0_f32.min(c.timing.get_time_for_beat(0.0)),
                            last_second: song.total_length_seconds.max(0) as f32,
                        }
                    }),
                };
            }

            if is_versus {
                let desired_hash_p2 = state
                    .cached_chart_ix_p2
                    .map(|ix| song.charts[ix].short_hash.as_str());

                if state
                    .displayed_chart_p2
                    .as_ref()
                    .map(|d| d.chart_hash.as_str())
                    != desired_hash_p2
                {
                    state.displayed_chart_p2 = desired_hash_p2.map(|h| DisplayedChart {
                        song: song.clone(),
                        chart_hash: h.to_string(),
                    });
                }

                if state.last_requested_chart_hash_p2.as_deref() != desired_hash_p2 {
                    state.last_requested_chart_hash_p2 = desired_hash_p2.map(str::to_string);
                    return ScreenAction::RequestDensityGraph {
                        slot: DensityGraphSlot::SelectMusicP2,
                        chart_opt: state.cached_chart_ix_p2.map(|ix| {
                            let c = &song.charts[ix];
                            DensityGraphSource {
                                max_nps: c.max_nps,
                                measure_nps_vec: c.measure_nps_vec.clone(),
                                timing: c.timing.clone(),
                                first_second: 0.0_f32.min(c.timing.get_time_for_beat(0.0)),
                                last_second: song.total_length_seconds.max(0) as f32,
                            }
                        }),
                    };
                }
            } else {
                state.displayed_chart_p2 = None;
            }
        } else {
            state.displayed_chart_p1 = None;
            state.displayed_chart_p2 = None;
            state.cached_song = None;
            state.cached_chart_ix_p1 = None;
            state.cached_chart_ix_p2 = None;
            state.cached_edits = None;
        }
    } else if state.currently_playing_preview_path.is_some() {
        state.currently_playing_preview_path = None;
        state.currently_playing_preview_start_sec = None;
        state.currently_playing_preview_length_sec = None;
        audio::stop_music();
    }

    ScreenAction::None
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    (
        vec![
            act!(quad: align(0.0, 0.0): xy(0.0, 0.0): zoomto(screen_width(), screen_height()): diffuse(0.0, 0.0, 0.0, 1.0): z(1100): linear(TRANSITION_IN_DURATION): alpha(0.0): linear(0.0): visible(false)),
        ],
        TRANSITION_IN_DURATION,
    )
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    (
        vec![
            act!(quad: align(0.0, 0.0): xy(0.0, 0.0): zoomto(screen_width(), screen_height()): diffuse(0.0, 0.0, 0.0, 0.0): z(1200): linear(TRANSITION_OUT_DURATION): alpha(1.0)),
        ],
        TRANSITION_OUT_DURATION,
    )
}

pub fn trigger_immediate_refresh(state: &mut State) {
    state.time_since_selection_change = PREVIEW_DELAY_SECONDS;
    state.last_requested_chart_hash = None;
    state.last_requested_chart_hash_p2 = None;
    state.last_requested_banner_path = None;
}

pub fn reset_preview_after_gameplay(state: &mut State) {
    let was_recent_sort = state.sort_mode == WheelSortMode::Recent;
    refresh_recent_cache(state);
    if was_recent_sort {
        state.sort_mode = WheelSortMode::Group;
        apply_wheel_sort(state, WheelSortMode::Recent);
    }
    state.currently_playing_preview_path = None;
    state.currently_playing_preview_start_sec = None;
    state.currently_playing_preview_length_sec = None;
    trigger_immediate_refresh(state);
}

pub fn prime_displayed_chart_data(state: &mut State) {
    if let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) {
        let target_chart_type = profile::get_session_play_style().chart_type();
        state.displayed_chart_p1 =
            chart_for_steps_index(song, target_chart_type, state.selected_steps_index).map(|c| {
                DisplayedChart {
                    song: song.clone(),
                    chart_hash: c.short_hash.clone(),
                }
            });
        state.displayed_chart_p2 =
            chart_for_steps_index(song, target_chart_type, state.p2_selected_steps_index).map(
                |c| DisplayedChart {
                    song: song.clone(),
                    chart_hash: c.short_hash.clone(),
                },
            );
        return;
    }
    state.displayed_chart_p1 = None;
    state.displayed_chart_p2 = None;
}

// Fast non-allocating formatters where possible
fn format_session_time(seconds: f32) -> String {
    if seconds < 0.0 {
        return "00:00".to_string();
    }
    let s = seconds as u64;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

fn format_chart_length(seconds: i32) -> String {
    let s = seconds.max(0) as u64;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{}:{:02}", m, s)
    }
}

fn format_groovestats_date(date: &str) -> String {
    if date.trim().is_empty() {
        return String::new();
    }
    let Some((ymd, _time)) = date.split_once(' ') else {
        return date.to_string();
    };
    let mut parts = ymd.split('-');
    let (Some(year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next()) else {
        return date.to_string();
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
        _ => return date.to_string(),
    };
    let day_num = day.parse::<u32>().unwrap_or(0);
    if day_num == 0 {
        return date.to_string();
    }
    format!("{month_txt} {day_num}, {year}")
}

#[inline(always)]
fn leaderboard_icon_bounce_offset(elapsed: f32, dir: f32) -> f32 {
    let t = elapsed.rem_euclid(1.0);
    let phase = if t < 0.5 {
        let u = t / 0.5;
        1.0 - (1.0 - u) * (1.0 - u) // decelerate
    } else {
        let u = (t - 0.5) / 0.5;
        1.0 - u * u // accelerate back
    };
    dir * 10.0 * phase
}

fn sl_select_music_bg_flash() -> Actor {
    act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(-98):
        sleep(SL_BG_FLASH_SLEEP_SECONDS):
        linear(SL_BG_FLASH_FADE_SECONDS): alpha(0.0):
        linear(0.0): visible(false)
    )
}

fn sl_select_music_wheel_cascade_mask() -> Vec<Actor> {
    let n = SL_WHEEL_CASCADE_NUM_VISIBLE_ITEMS;
    let count = n.saturating_sub(2);
    let mut actors = Vec::with_capacity(count * 2);

    let slot_spacing = screen_height() / n as f32;
    let item_half_h = slot_spacing * 0.5;
    let x = screen_center_x() + screen_width() * 0.25;
    let w = screen_width() * 0.5;

    for i in 1..=count {
        let t_sleep = i as f32 * SL_WHEEL_CASCADE_DELAY_STEP_SECONDS;
        let y_base = slot_spacing * i as f32;

        // upper half mask
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(x, SL_WHEEL_CASCADE_ROW_Y_UPPER + y_base):
            zoomto(w, item_half_h):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(SL_WHEEL_CASCADE_Z):
            cropbottom(0.0):
            sleep(t_sleep):
            linear(SL_WHEEL_CASCADE_REVEAL_SECONDS): cropbottom(1.0): alpha(SL_WHEEL_CASCADE_FINAL_ALPHA):
            linear(0.0): visible(false)
        ));

        // lower half mask
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(x, SL_WHEEL_CASCADE_ROW_Y_LOWER + y_base):
            zoomto(w, item_half_h):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(SL_WHEEL_CASCADE_Z):
            croptop(0.0):
            sleep(t_sleep):
            linear(SL_WHEEL_CASCADE_REVEAL_SECONDS): croptop(1.0): alpha(SL_WHEEL_CASCADE_FINAL_ALPHA):
            linear(0.0): visible(false)
        ));
    }

    actors
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(256);
    let side = crate::game::profile::get_session_player_side();
    let play_style = crate::game::profile::get_session_play_style();
    let is_p2_single = play_style == crate::game::profile::PlayStyle::Single
        && side == crate::game::profile::PlayerSide::P2;
    let is_versus = play_style == crate::game::profile::PlayStyle::Versus;
    let target_chart_type = play_style.chart_type();

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));
    actors.push(sl_select_music_bg_flash());
    actors.push(screen_bar::build(ScreenBarParams {
        title: "SELECT MUSIC",
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
    }));

    let p1_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P1);
    let p2_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P2);
    let p1_avatar = p1_profile
        .avatar_texture_key
        .as_deref()
        .map(|k| AvatarParams { texture_key: k });
    let p2_avatar = p2_profile
        .avatar_texture_key
        .as_deref()
        .map(|k| AvatarParams { texture_key: k });

    let p1_joined =
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P1);
    let p2_joined =
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P2);
    let p1_guest =
        crate::game::profile::is_session_side_guest(crate::game::profile::PlayerSide::P1);
    let p2_guest =
        crate::game::profile::is_session_side_guest(crate::game::profile::PlayerSide::P2);

    let (footer_left, left_avatar) = if p1_joined {
        (
            Some(if p1_guest {
                "INSERT CARD"
            } else {
                p1_profile.display_name.as_str()
            }),
            if p1_guest { None } else { p1_avatar },
        )
    } else {
        (Some("PRESS START"), None)
    };
    let (footer_right, right_avatar) = if p2_joined {
        (
            Some(if p2_guest {
                "INSERT CARD"
            } else {
                p2_profile.display_name.as_str()
            }),
            if p2_guest { None } else { p2_avatar },
        )
    } else {
        (Some("PRESS START"), None)
    };
    actors.push(screen_bar::build(ScreenBarParams {
        title: "EVENT MODE",
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar,
        right_avatar,
    }));

    let preferred_idx_p1 = state
        .preferred_difficulty_index
        .min(color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1));
    let mut sel_col_p1 = color::difficulty_rgba(
        color::FILE_DIFFICULTY_NAMES[preferred_idx_p1],
        state.active_color_index,
    );

    let preferred_idx_p2 = state
        .p2_preferred_difficulty_index
        .min(color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1));
    let mut sel_col_p2 = color::difficulty_rgba(
        color::FILE_DIFFICULTY_NAMES[preferred_idx_p2],
        state.active_color_index,
    );
    if let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) {
        if let Some(chart) =
            chart_for_steps_index(song, target_chart_type, state.selected_steps_index)
        {
            sel_col_p1 = color::difficulty_rgba(&chart.difficulty, state.active_color_index);
        }
        if let Some(chart) =
            chart_for_steps_index(song, target_chart_type, state.p2_selected_steps_index)
        {
            sel_col_p2 = color::difficulty_rgba(&chart.difficulty, state.active_color_index);
        }
    }

    // Timer
    actors.push(act!(text: font("wendy_monospace_numbers"): settext(format_session_time(state.session_elapsed)): align(0.5, 0.5): xy(screen_center_x(), 10.0): zoom(widescale(0.3, 0.36)): z(121): diffuse(1.0, 1.0, 1.0, 1.0): horizalign(center)));

    // Pads
    {
        actors.push(act!(text: font("wendy"): settext("ITG"): align(1.0, 0.5): xy(screen_width() - widescale(55.0, 62.0), 15.0): zoom(widescale(0.5, 0.6)): z(121): diffuse(1.0, 1.0, 1.0, 1.0)));
        let pad_zoom = 0.24 * widescale(0.435, 0.525);
        actors.push(pad_display::build(pad_display::PadDisplayParams {
            center_x: screen_width() - widescale(35.0, 41.0),
            center_y: widescale(22.0, 23.5),
            zoom: pad_zoom,
            z: 121,
            is_active: true,
        }));
        actors.push(pad_display::build(pad_display::PadDisplayParams {
            center_x: screen_width() - widescale(15.0, 17.0),
            center_y: widescale(22.0, 23.5),
            zoom: pad_zoom,
            z: 121,
            is_active: false,
        }));
    }

    // Banner
    let (banner_zoom, banner_cx, banner_cy) = if is_wide() {
        (0.7655, screen_center_x() - 170.0, 96.0)
    } else {
        (0.75, screen_center_x() - 166.0, 96.0)
    };
    actors.push(act!(sprite(state.current_banner_key.clone()): align(0.5, 0.5): xy(banner_cx, banner_cy): setsize(BANNER_NATIVE_WIDTH, BANNER_NATIVE_HEIGHT): zoom(banner_zoom): z(51)));

    let music_rate = crate::game::profile::get_session_music_rate();
    if (music_rate - 1.0).abs() > 0.001 {
        let text = format!("{}x Music Rate", fmt_music_rate(music_rate));
        actors.push(act!(quad: align(0.5, 0.5): xy(banner_cx, banner_cy + 75.0 * banner_zoom): setsize(BANNER_NATIVE_WIDTH * banner_zoom, 14.0 * banner_zoom): z(52): diffuse(0.117, 0.156, 0.184, 0.8)));
        actors.push(act!(text: font("miso"): settext(text): align(0.5, 0.5): xy(banner_cx, banner_cy + 75.0 * banner_zoom): zoom(0.85 * banner_zoom): shadowlength(1.0): z(53): diffuse(1.0, 1.0, 1.0, 1.0)));
    }

    // Info Box
    let (box_w, frame_x, frame_y) = if is_wide() {
        (320.0, screen_center_x() - 170.0, screen_center_y() - 55.0)
    } else {
        (310.0, screen_center_x() - 165.0, screen_center_y() - 55.0)
    };
    let entry_opt = state.entries.get(state.selected_index);
    let (artist, bpm, len_text) = match entry_opt {
        Some(MusicWheelEntry::Song(s)) => (
            s.artist.clone(),
            s.formatted_display_bpm(),
            format_chart_length(
                ((if s.music_length_seconds > 0.0 {
                    s.music_length_seconds
                } else {
                    s.total_length_seconds.max(0) as f32
                }) / music_rate)
                    .round() as i32,
            ),
        ),
        Some(MusicWheelEntry::PackHeader { original_index, .. }) => {
            let total_sec: f64 = get_song_cache()
                .get(*original_index)
                .map(|p| {
                    p.songs
                        .iter()
                        .map(|s| {
                            (if s.music_length_seconds > 0.0 {
                                s.music_length_seconds
                            } else {
                                s.total_length_seconds.max(0) as f32
                            }) as f64
                        })
                        .sum()
                })
                .unwrap_or(0.0);
            (
                "".to_string(),
                "".to_string(),
                format_session_time((total_sec / music_rate as f64) as f32),
            )
        }
        None => ("".to_string(), "".to_string(), "".to_string()),
    };

    actors.push(Actor::Frame {
        align: [0.0, 0.0], offset: [frame_x, frame_y], size: [SizeSpec::Px(box_w), SizeSpec::Px(50.0)], background: None, z: 51,
        children: vec![
            act!(quad: setsize(box_w, 50.0): diffuse(UI_BOX_BG_COLOR[0], UI_BOX_BG_COLOR[1], UI_BOX_BG_COLOR[2], UI_BOX_BG_COLOR[3])),
            Actor::Frame {
                align: [0.0, 0.0], offset: [-110.0, -6.0], size: [SizeSpec::Fill, SizeSpec::Fill], background: None, z: 0,
                children: vec![
                    act!(text: font("miso"): settext("ARTIST"): align(1.0, 0.0): y(-11.0): maxwidth(44.0): diffuse(0.5, 0.5, 0.5, 1.0): z(52)),
                    act!(text: font("miso"): settext(artist): align(0.0, 0.0): xy(5.0, -11.0): maxwidth(box_w - 60.0): zoomtoheight(15.0): diffuse(1.0, 1.0, 1.0, 1.0): z(52)),
                    act!(text: font("miso"): settext("BPM"): align(1.0, 0.0): y(10.0): diffuse(0.5, 0.5, 0.5, 1.0): z(52)),
                    act!(text: font("miso"): settext(bpm): align(0.0, 0.0): xy(5.0, 10.0): zoomtoheight(15.0): diffuse(1.0, 1.0, 1.0, 1.0): z(52)),
                    act!(text: font("miso"): settext("LENGTH"): align(1.0, 0.0): xy(box_w - 130.0, 10.0): diffuse(0.5, 0.5, 0.5, 1.0): z(52)),
                    act!(text: font("miso"): settext(len_text): align(0.0, 0.0): xy(box_w - 125.0, 10.0): zoomtoheight(15.0): diffuse(1.0, 1.0, 1.0, 1.0): z(52)),
                ],
            },
        ],
    });

    // Chart Stats & Graph
    let immediate_chart_p1 = match entry_opt {
        Some(MusicWheelEntry::Song(s)) => {
            chart_for_steps_index(s, target_chart_type, state.selected_steps_index)
        }
        _ => None,
    };

    let immediate_chart_p2 = match entry_opt {
        Some(MusicWheelEntry::Song(s)) => {
            chart_for_steps_index(s, target_chart_type, state.p2_selected_steps_index)
        }
        _ => None,
    };

    let disp_chart_p1 = state
        .displayed_chart_p1
        .as_ref()
        .and_then(|d| d.song.charts.iter().find(|c| c.short_hash == d.chart_hash));
    let disp_chart_p2 = state
        .displayed_chart_p2
        .as_ref()
        .and_then(|d| d.song.charts.iter().find(|c| c.short_hash == d.chart_hash));

    let (step_artist, steps, jumps, holds, mines, hands, rolls, meter) =
        if let Some(c) = immediate_chart_p1 {
            (
                if c.difficulty.eq_ignore_ascii_case("edit") && !c.description.trim().is_empty() {
                    c.description.as_str()
                } else {
                    c.step_artist.as_str()
                },
                c.stats.total_steps.to_string(),
                c.stats.jumps.to_string(),
                c.stats.holds.to_string(),
                c.mines_nonfake.to_string(),
                c.stats.hands.to_string(),
                c.stats.rolls.to_string(),
                c.meter.to_string(),
            )
        } else {
            (
                "",
                "?".to_string(),
                "?".to_string(),
                "?".to_string(),
                "?".to_string(),
                "?".to_string(),
                "?".to_string(),
                if matches!(entry_opt, Some(MusicWheelEntry::Song(_))) {
                    "?".to_string()
                } else {
                    "".to_string()
                },
            )
        };

    let step_artist_p2 = if let Some(c) = immediate_chart_p2 {
        if c.difficulty.eq_ignore_ascii_case("edit") && !c.description.trim().is_empty() {
            c.description.as_str()
        } else {
            c.step_artist.as_str()
        }
    } else {
        ""
    };

    let (steps_p2, jumps_p2, holds_p2, mines_p2, hands_p2, rolls_p2, meter_p2) =
        if let Some(c) = immediate_chart_p2 {
            (
                c.stats.total_steps.to_string(),
                c.stats.jumps.to_string(),
                c.stats.holds.to_string(),
                c.mines_nonfake.to_string(),
                c.stats.hands.to_string(),
                c.stats.rolls.to_string(),
                c.meter.to_string(),
            )
        } else {
            (
                "?".to_string(),
                "?".to_string(),
                "?".to_string(),
                "?".to_string(),
                "?".to_string(),
                "?".to_string(),
                if matches!(entry_opt, Some(MusicWheelEntry::Song(_))) {
                    "?".to_string()
                } else {
                    "".to_string()
                },
            )
        };

    // Step Artist & Steps
    let comp_h = screen_height() / 28.0;
    let base_y = (screen_center_y() - 9.0) - 0.5 * comp_h;
    let mut push_step_artist = |y_cen: f32, x0: f32, sel_col: [f32; 4], step_artist: &str| {
        let q_cx = x0 + 113.0;
        let s_x = x0 + 30.0;
        let a_x = x0 + 75.0;

        actors.push(act!(quad: align(0.5, 0.5): xy(q_cx, y_cen): setsize(175.0, comp_h): z(120): diffuse(sel_col[0], sel_col[1], sel_col[2], 1.0)));
        actors.push(act!(text: font("miso"): settext("STEPS"): align(0.0, 0.5): xy(s_x, y_cen): zoom(0.8): maxwidth(40.0): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
        actors.push(act!(text: font("miso"): settext(step_artist): align(0.0, 0.5): xy(a_x, y_cen): zoom(0.8): maxwidth(124.0): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
    };

    if is_versus {
        let x0_p1 = if is_wide() {
            screen_center_x() - 355.5
        } else {
            screen_center_x() - 345.5
        };
        push_step_artist(base_y, x0_p1, sel_col_p1, step_artist);
        push_step_artist(
            base_y + 88.0,
            screen_center_x() - 244.0,
            sel_col_p2,
            step_artist_p2,
        );
    } else {
        let y_cen = base_y + if is_p2_single { 88.0 } else { 0.0 };
        let step_artist_x0 = if is_p2_single {
            screen_center_x() - 244.0
        } else if is_wide() {
            screen_center_x() - 355.5
        } else {
            screen_center_x() - 345.5
        };
        push_step_artist(y_cen, step_artist_x0, sel_col_p1, step_artist);
    }

    // Density Graph
    let panel_w = if is_wide() { 286.0 } else { 276.0 };
    let chart_info_cx = screen_center_x() - 182.0 - if is_wide() { 5.0 } else { 0.0 };
    let build_breakdown_panel = |graph_cy: f32,
                                 is_p2_layout: bool,
                                 graph_key: &String,
                                 graph_mesh: Option<Arc<[MeshVertex]>>,
                                 chart: Option<&ChartData>| {
        let mut graph_kids = vec![
            act!(quad: align(0.0, 0.0): xy(0.0, 0.0): setsize(panel_w, 64.0): diffuse(UI_BOX_BG_COLOR[0], UI_BOX_BG_COLOR[1], UI_BOX_BG_COLOR[2], UI_BOX_BG_COLOR[3])),
        ];

        if let Some(c) = chart {
            let peak = format!(
                "Peak NPS: {:.1}",
                if music_rate.is_finite() {
                    c.max_nps * music_rate as f64
                } else {
                    c.max_nps
                }
            );
            // Match Simply Love's minimization loop (0 -> 3) based on rendered width.
            let bd_text = asset_manager
                .with_fonts(|all_fonts| {
                    asset_manager.with_font("miso", |miso_font| -> Option<String> {
                        let text_zoom = 0.8;
                        let max_allowed_logical_width = panel_w / text_zoom;
                        let fits = |text: &str| {
                            (font::measure_line_width_logical(miso_font, text, all_fonts) as f32)
                                <= max_allowed_logical_width
                        };

                        if fits(&c.detailed_breakdown) {
                            Some(c.detailed_breakdown.clone())
                        } else if fits(&c.partial_breakdown) {
                            Some(c.partial_breakdown.clone())
                        } else if fits(&c.simple_breakdown) {
                            Some(c.simple_breakdown.clone())
                        } else {
                            Some(format!("{} Total", c.total_streams))
                        }
                    })
                })
                .flatten()
                .unwrap_or_else(|| c.simple_breakdown.clone());

            let peak_x = panel_w * 0.5 + if is_p2_layout { -136.0 } else { 60.0 };
            if let Some(mesh) = graph_mesh
                && !mesh.is_empty()
            {
                graph_kids.push(Actor::Mesh {
                    align: [0.0, 0.0],
                    offset: [0.0, 0.0],
                    size: [SizeSpec::Px(panel_w), SizeSpec::Px(64.0)],
                    vertices: mesh,
                    mode: MeshMode::Triangles,
                    visible: true,
                    blend: BlendMode::Alpha,
                    z: 0,
                });
            } else if graph_key != "__white" {
                graph_kids.push(act!(sprite(graph_key.clone()):
                    align(0.0, 0.0): xy(0.0, 0.0): setsize(panel_w, 64.0)
                ));
            }
            graph_kids.push(act!(text: font("miso"): settext(peak): align(0.0, 0.5): xy(peak_x, -9.0): zoom(0.8): diffuse(1.0, 1.0, 1.0, 1.0)));
            graph_kids.push(act!(quad: align(0.0, 0.0): xy(0.0, 47.0): setsize(panel_w, 17.0): diffuse(0.0, 0.0, 0.0, 0.5)));
            graph_kids.push(act!(text: font("miso"): settext(bd_text): align(0.5, 0.5): xy(panel_w * 0.5, 55.5): zoom(0.8): maxwidth(panel_w)));
        }

        Actor::Frame {
            align: [0.0, 0.0],
            offset: [chart_info_cx - 0.5 * panel_w, graph_cy - 32.0],
            size: [SizeSpec::Px(panel_w), SizeSpec::Px(64.0)],
            background: None,
            z: 51,
            children: graph_kids,
        }
    };

    if is_versus {
        actors.push(build_breakdown_panel(
            screen_center_y() + 23.0,
            false,
            &state.current_graph_key,
            state.current_graph_mesh.clone(),
            disp_chart_p1,
        ));
        actors.push(build_breakdown_panel(
            screen_center_y() + 111.0,
            true,
            &state.current_graph_key_p2,
            state.current_graph_mesh_p2.clone(),
            disp_chart_p2,
        ));
    } else {
        let graph_cy = screen_center_y() + if is_p2_single { 111.0 } else { 23.0 };
        actors.push(build_breakdown_panel(
            graph_cy,
            is_p2_single,
            &state.current_graph_key,
            state.current_graph_mesh.clone(),
            disp_chart_p1,
        ));
    }

    // Pane Display
    let pane_top = screen_height() - 92.0;
    let tz = widescale(0.8, 0.9);
    let cols = [
        widescale(-104.0, -133.0),
        widescale(-36.0, -38.0),
        widescale(54.0, 76.0),
        widescale(150.0, 190.0),
    ];
    let rows = [13.0, 31.0, 49.0];

    let build_pane = |pane_cx: f32,
                      sel_col: [f32; 4],
                      side: profile::PlayerSide,
                      player_initials: &str,
                      steps: &str,
                      mines: &str,
                      jumps: &str,
                      hands: &str,
                      holds: &str,
                      rolls: &str,
                      meter: &str,
                      chart: Option<&ChartData>| {
        let mut out = Vec::with_capacity(32);

        out.push(act!(quad: align(0.5, 0.0): xy(pane_cx, pane_top): setsize(screen_width() / 2.0 - 10.0, 60.0): z(120): diffuse(sel_col[0], sel_col[1], sel_col[2], 1.0)));

        // Stats Grid
        let stats = [
            ("Steps", steps),
            ("Mines", mines),
            ("Jumps", jumps),
            ("Hands", hands),
            ("Holds", holds),
            ("Rolls", rolls),
        ];
        for (i, (lbl, val)) in stats.iter().enumerate() {
            let (c, r) = (i % 2, i / 2);
            out.push(act!(text: font("miso"): settext(*val): align(1.0, 0.5): horizalign(right): xy(pane_cx + cols[c], pane_top + rows[r]): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
            out.push(act!(text: font("miso"): settext(*lbl): align(0.0, 0.5): xy(pane_cx + cols[c] + 3.0, pane_top + rows[r]): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
        }

        // Scores
        let placeholder = ("----".to_string(), "??.??%".to_string());
        let (p_name, p_pct) = if let Some(c) = chart
            && let Some(sc) = scores::get_cached_score_for_side(&c.short_hash, side)
            && (sc.grade != scores::Grade::Failed || sc.score_percent > 0.0)
        {
            (
                player_initials.to_string(),
                format!("{:.2}%", sc.score_percent * 100.0),
            )
        } else {
            placeholder.clone()
        };

        let (m_name, m_pct) = if let Some(c) = chart
            && let Some((initials, sc)) = scores::get_machine_record_local(&c.short_hash)
            && (sc.grade != scores::Grade::Failed || sc.score_percent > 0.0)
        {
            (initials, format!("{:.2}%", sc.score_percent * 100.0))
        } else {
            placeholder
        };

        // Simply Love PaneDisplay order: Machine/World first, then Player.
        let lines = [(m_name, m_pct), (p_name, p_pct)];
        for i in 0..2 {
            let (name, pct) = &lines[i];
            out.push(act!(text: font("miso"): settext(name): align(0.5, 0.5): xy(pane_cx + cols[2] - 50.0 * tz, pane_top + rows[i]): maxwidth(30.0): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
            out.push(act!(text: font("miso"): settext(pct): align(1.0, 0.5): xy(pane_cx + cols[2] + 25.0 * tz, pane_top + rows[i]): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
        }

        // Difficulty Meter
        let mut m_actor = act!(text: font("wendy"): settext(meter): align(1.0, 0.5): horizalign(right): xy(pane_cx + cols[3], pane_top + rows[1]): z(121): diffuse(0.0, 0.0, 0.0, 1.0));
        if !is_wide() {
            if let Actor::Text { max_width, .. } = &mut m_actor {
                *max_width = Some(66.0);
            }
        }
        out.push(m_actor);
        out
    };

    if is_versus {
        actors.extend(build_pane(
            screen_width() * 0.25 - 5.0,
            sel_col_p1,
            profile::PlayerSide::P1,
            p1_profile.player_initials.as_str(),
            &steps,
            &mines,
            &jumps,
            &hands,
            &holds,
            &rolls,
            &meter,
            immediate_chart_p1,
        ));
        actors.extend(build_pane(
            screen_width() * 0.75 + 5.0,
            sel_col_p2,
            profile::PlayerSide::P2,
            p2_profile.player_initials.as_str(),
            &steps_p2,
            &mines_p2,
            &jumps_p2,
            &hands_p2,
            &holds_p2,
            &rolls_p2,
            &meter_p2,
            immediate_chart_p2,
        ));
    } else {
        let pane_cx = if is_p2_single {
            screen_width() * 0.75 + 5.0
        } else {
            screen_width() * 0.25 - 5.0
        };
        actors.extend(build_pane(
            pane_cx,
            sel_col_p1,
            if is_p2_single {
                profile::PlayerSide::P2
            } else {
                profile::PlayerSide::P1
            },
            if is_p2_single {
                p2_profile.player_initials.as_str()
            } else {
                p1_profile.player_initials.as_str()
            },
            &steps,
            &mines,
            &jumps,
            &hands,
            &holds,
            &rolls,
            &meter,
            immediate_chart_p1,
        ));
    }

    if !is_versus {
        // Pattern Info
        let (cross, foot, side, jack, brack, stream) = if let Some(c) = disp_chart_p1 {
            (
                c.tech_counts.crossovers.to_string(),
                c.tech_counts.footswitches.to_string(),
                c.tech_counts.sideswitches.to_string(),
                c.tech_counts.jacks.to_string(),
                c.tech_counts.brackets.to_string(),
                if c.total_measures > 0 {
                    format!(
                        "{}/{} ({:.1}%)",
                        c.total_streams,
                        c.total_measures,
                        (c.total_streams as f32 / c.total_measures as f32) * 100.0
                    )
                } else {
                    "None (0.0%)".to_string()
                },
            )
        } else {
            (
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "None (0.0%)".to_string(),
            )
        };

        let pat_cx = chart_info_cx;
        let pat_cy = screen_center_y() + if is_p2_single { 23.0 } else { 111.0 };
        actors.push(act!(quad: align(0.5, 0.5): xy(pat_cx, pat_cy): setsize(panel_w, 64.0): z(120): diffuse(UI_BOX_BG_COLOR[0], UI_BOX_BG_COLOR[1], UI_BOX_BG_COLOR[2], UI_BOX_BG_COLOR[3])));

        let p_v_x = pat_cx - panel_w * 0.5 + 40.0;
        let p_l_x = pat_cx - panel_w * 0.5 + 50.0;
        let p_base_y = pat_cy - 19.0;
        let items = [
            (&cross, "Crossovers", 0, 0, None),
            (&foot, "Footswitches", 1, 0, None),
            (&side, "Sideswitches", 0, 1, None),
            (&jack, "Jacks", 1, 1, None),
            (&brack, "Brackets", 0, 2, None),
            (&stream, "Total Stream", 1, 2, Some(100.0)),
        ];

        for (val, lbl, c, r, mw) in items {
            let y = p_base_y + r as f32 * 20.0;
            let vx = p_v_x + c as f32 * 150.0;
            let lx = p_l_x + c as f32 * 150.0;
            match mw {
                Some(w) => actors.push(act!(text: font("miso"): settext(val): align(1.0, 0.5): horizalign(right): xy(vx, y): maxwidth(w): zoom(0.8): z(121): diffuse(1.0, 1.0, 1.0, 1.0))),
                None => actors.push(act!(text: font("miso"): settext(val): align(1.0, 0.5): horizalign(right): xy(vx, y): zoom(0.8): z(121): diffuse(1.0, 1.0, 1.0, 1.0))),
            }
            actors.push(act!(text: font("miso"): settext(lbl): align(0.0, 0.5): horizalign(left): xy(lx, y): zoom(0.8): z(121): diffuse(1.0, 1.0, 1.0, 1.0)));
        }
    }

    // Steps Display List
    let lst_cx = screen_center_x() - 26.0;
    let lst_cy = screen_center_y() + 67.0;
    actors.push(act!(quad: align(0.5, 0.5): xy(lst_cx, lst_cy): setsize(32.0, 152.0): z(120): diffuse(UI_BOX_BG_COLOR[0], UI_BOX_BG_COLOR[1], UI_BOX_BG_COLOR[2], UI_BOX_BG_COLOR[3])));

    const VISIBLE_STEPS_SLOTS: usize = 5;
    let (steps_charts, sel_p1, sel_p2) = match entry_opt {
        Some(MusicWheelEntry::Song(song)) => {
            let mut v: Vec<Option<&ChartData>> =
                Vec::with_capacity(color::FILE_DIFFICULTY_NAMES.len());
            for &diff in &color::FILE_DIFFICULTY_NAMES {
                v.push(song.charts.iter().find(|c| {
                    c.chart_type.eq_ignore_ascii_case(target_chart_type)
                        && c.difficulty.eq_ignore_ascii_case(diff)
                        && !c.notes.is_empty()
                }));
            }
            v.extend(
                edit_charts_sorted(song, target_chart_type)
                    .into_iter()
                    .map(Some),
            );
            (v, state.selected_steps_index, state.p2_selected_steps_index)
        }
        _ => (
            vec![None; color::FILE_DIFFICULTY_NAMES.len()],
            state.preferred_difficulty_index,
            state.p2_preferred_difficulty_index,
        ),
    };
    let list_len = steps_charts.len();
    let sel_p1 = sel_p1.min(list_len.saturating_sub(1));
    let sel_p2 = sel_p2.min(list_len.saturating_sub(1));
    let focus_sel = if is_versus {
        sel_p1.max(sel_p2)
    } else {
        sel_p1
    };
    let top_index = if list_len > VISIBLE_STEPS_SLOTS {
        // Simply Love: keep Edit charts off-screen until you scroll past Expert.
        // Once you're in Edit charts, keep the selected chart in the bottom slot and
        // shift the other difficulties upward as you move deeper.
        focus_sel
            .saturating_sub(VISIBLE_STEPS_SLOTS - 1)
            .min(list_len - VISIBLE_STEPS_SLOTS)
    } else {
        0
    };

    for slot in 0..VISIBLE_STEPS_SLOTS {
        let y = (slot as i32 - 2) as f32 * 30.0;
        actors.push(act!(quad: align(0.5, 0.5): xy(lst_cx, lst_cy + y): setsize(28.0, 28.0): z(121): diffuse(0.059, 0.059, 0.059, 1.0)));
        let idx = top_index + slot;
        if idx >= list_len {
            continue;
        }
        if let Some(chart) = steps_charts[idx] {
            let c = color::difficulty_rgba(&chart.difficulty, state.active_color_index);
            actors.push(act!(text: font("wendy"): settext(chart.meter.to_string()): align(0.5, 0.5): xy(lst_cx, lst_cy + y): zoom(0.45): z(122): diffuse(c[0], c[1], c[2], 1.0)));
        }
    }

    // Music Wheel
    actors.extend(music_wheel::build(music_wheel::MusicWheelParams {
        entries: &state.entries,
        selected_index: state.selected_index,
        position_offset_from_selection: state.wheel_offset_from_selection,
        selection_animation_timer: state.selection_animation_timer,
        pack_song_counts: &state.pack_song_counts, // O(1) Lookup
        color_pack_headers: state.sort_mode == WheelSortMode::Group,
        preferred_difficulty_index: state.preferred_difficulty_index,
        selected_steps_index: state.selected_steps_index,
    }));
    actors.extend(sl_select_music_wheel_cascade_mask());

    // Bouncing Arrow (SL parity: bounce + effectperiod(1) + effectoffset(-10*GlobalOffsetSeconds))
    let bounce = sl_arrow_bounce01(entry_opt, state);
    let dx_p1 = -3.0 * bounce;
    let dx_p2 = 3.0 * bounce;
    if is_versus {
        let slot_p1 = (sel_p1.saturating_sub(top_index)).min(VISIBLE_STEPS_SLOTS - 1);
        let y_p1 = lst_cy + (slot_p1 as i32 - 2) as f32 * 30.0 + 1.0;
        actors.push(act!(sprite("meter_arrow.png"):
            align(0.0, 0.5):
            xy(screen_center_x() - 53.0 + dx_p1, y_p1):
            rotationz(0.0):
            zoom(0.575):
            z(122)
        ));

        let slot_p2 = (sel_p2.saturating_sub(top_index)).min(VISIBLE_STEPS_SLOTS - 1);
        let y_p2 = lst_cy + (slot_p2 as i32 - 2) as f32 * 30.0 + 1.0;
        actors.push(act!(sprite("meter_arrow.png"):
            align(0.0, 0.5):
            xy(lst_cx + 8.0 + dx_p2, y_p2):
            rotationz(180.0):
            zoom(0.575):
            z(122)
        ));
    } else {
        let arrow_slot = (sel_p1.saturating_sub(top_index)).min(VISIBLE_STEPS_SLOTS - 1);
        let arrow_y = lst_cy + (arrow_slot as i32 - 2) as f32 * 30.0 + 1.0;
        let (arrow_x0, arrow_dx, arrow_rot) = if is_p2_single {
            let x0 = lst_cx + 8.0;
            (x0, dx_p2, 180.0)
        } else {
            (screen_center_x() - 53.0, dx_p1, 0.0)
        };
        actors.push(act!(sprite("meter_arrow.png"):
            align(0.0, 0.5):
            xy(arrow_x0 + arrow_dx, arrow_y):
            rotationz(arrow_rot):
            zoom(0.575):
            z(122)
        ));
    }

    if let Some(reload) = &state.reload_ui {
        let header = match reload.phase {
            ReloadPhase::Songs => "Loading songs...",
            ReloadPhase::Courses => "Loading courses...",
        };
        let text = if reload.line2.is_empty() && reload.line3.is_empty() {
            header.to_string()
        } else if reload.line2.is_empty() {
            format!("{header}\n{}", reload.line3)
        } else if reload.line3.is_empty() {
            format!("{header}\n{}", reload.line2)
        } else {
            format!("{header}\n{}\n{}", reload.line2, reload.line3)
        };

        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 0.8):
            z(1450)
        ));
        actors.push(act!(text:
            align(0.5, 0.5):
            xy(screen_center_x(), screen_center_y()):
            zoom(1.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            font("miso"):
            settext(text):
            horizalign(center):
            z(1451)
        ));
        return actors;
    }

    if !matches!(state.song_search, SongSearchState::Hidden) {
        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 0.8):
            z(1450)
        ));
        match &state.song_search {
            SongSearchState::Prompt { query } => {
                let cx = screen_center_x();
                let cy = screen_center_y();
                let panel_w = 720.0;
                let panel_h = 220.0;
                let query_text = format!("> {query}");

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
                    diffuse(0.0, 0.0, 0.0, 1.0):
                    z(1452)
                ));
                actors.push(act!(text:
                    font("wendy"):
                    settext(SL_SONG_SEARCH_PROMPT_TITLE):
                    align(0.5, 0.5):
                    xy(cx, cy - 78.0):
                    zoom(0.52):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(1453):
                    horizalign(center)
                ));
                actors.push(act!(text:
                    font("miso"):
                    settext(SL_SONG_SEARCH_PROMPT_HINT):
                    align(0.5, 0.5):
                    xy(cx, cy - 20.0):
                    zoom(0.8):
                    maxwidth(670.0):
                    diffuse(0.8, 0.8, 0.8, 1.0):
                    z(1453):
                    horizalign(center)
                ));
                actors.push(act!(text:
                    font("miso"):
                    settext(query_text):
                    align(0.5, 0.5):
                    xy(cx, cy + 48.0):
                    zoom(0.95):
                    maxwidth(650.0):
                    diffuse(0.4, 1.0, 0.4, 1.0):
                    z(1453):
                    horizalign(center)
                ));
                actors.push(act!(text:
                    font("wendy"):
                    settext("Press ENTER/START to search, BACK/SELECT to cancel"):
                    align(0.5, 0.5):
                    xy(cx, cy + 88.0):
                    zoom(0.24):
                    diffuse(0.75, 0.75, 0.75, 1.0):
                    z(1453):
                    horizalign(center)
                ));
            }
            SongSearchState::Results(results) => {
                let pane_cx = screen_center_x();
                let pane_cy = screen_center_y() + 40.0;
                let list_base_y =
                    pane_cy - SL_SONG_SEARCH_PANE_H * 0.5 - SL_SONG_SEARCH_TEXT_H * 2.5;
                let list_x = pane_cx - SL_SONG_SEARCH_PANE_W * 0.25;
                let list_clip = [
                    pane_cx - SL_SONG_SEARCH_PANE_W * 0.5,
                    pane_cy - SL_SONG_SEARCH_PANE_H * 0.5,
                    SL_SONG_SEARCH_PANE_W * 0.5,
                    SL_SONG_SEARCH_PANE_H,
                ];
                let selected_color = color::simply_love_rgba(state.active_color_index);
                let total_items = song_search_total_items(results).max(1);
                let focus_t = (results.focus_anim_elapsed
                    / SL_SONG_SEARCH_FOCUS_TWEEN_SECONDS.max(1e-6))
                .clamp(0.0, 1.0);
                let scroll_dir = sort_menu_scroll_dir(
                    total_items,
                    results.prev_selected_index,
                    results.selected_index,
                ) as f32;
                let scroll_shift = scroll_dir
                    * [1.0 - focus_t, 0.0][(results.focus_anim_elapsed
                        >= SL_SONG_SEARCH_FOCUS_TWEEN_SECONDS)
                        as usize];

                actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(pane_cx, pane_cy):
                    zoomto(
                        SL_SONG_SEARCH_PANE_W + SL_SONG_SEARCH_PANE_BORDER,
                        SL_SONG_SEARCH_PANE_H + SL_SONG_SEARCH_PANE_BORDER
                    ):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(1451)
                ));
                actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(pane_cx, pane_cy):
                    zoomto(SL_SONG_SEARCH_PANE_W, SL_SONG_SEARCH_PANE_H):
                    diffuse(0.0, 0.0, 0.0, 1.0):
                    z(1452)
                ));
                actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(pane_cx, pane_cy):
                    zoomto(SL_SONG_SEARCH_PANE_BORDER, SL_SONG_SEARCH_PANE_H - 10.0):
                    diffuse(0.2, 0.2, 0.2, 1.0):
                    z(1453)
                ));
                actors.push(act!(text:
                    font("miso"):
                    settext("Search Results For:"):
                    align(0.5, 0.5):
                    xy(pane_cx, pane_cy - SL_SONG_SEARCH_PANE_H * 0.5 - SL_SONG_SEARCH_TEXT_H * 5.0):
                    zoom(0.8):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(1454):
                    horizalign(center)
                ));
                actors.push(act!(text:
                    font("miso"):
                    settext(format!("\"{}\"", results.search_text)):
                    align(0.5, 0.5):
                    xy(pane_cx, pane_cy - SL_SONG_SEARCH_PANE_H * 0.5 - SL_SONG_SEARCH_TEXT_H * 3.0):
                    zoom(0.8):
                    maxwidth(SL_SONG_SEARCH_PANE_W):
                    diffuse(0.4, 1.0, 0.4, 1.0):
                    z(1454):
                    horizalign(center)
                ));
                actors.push(act!(text:
                    font("miso"):
                    settext(format!("{} Results Found", results.candidates.len())):
                    align(0.5, 0.5):
                    xy(pane_cx, pane_cy - SL_SONG_SEARCH_PANE_H * 0.5 - SL_SONG_SEARCH_TEXT_H):
                    zoom(0.8):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(1454):
                    horizalign(center)
                ));

                for slot_idx in 0..SL_SONG_SEARCH_WHEEL_SLOTS {
                    let offset = slot_idx as isize - SL_SONG_SEARCH_WHEEL_FOCUS_SLOT as isize;
                    let row_idx = ((results.selected_index as isize + offset)
                        .rem_euclid(total_items as isize))
                        as usize;
                    let slot_pos = offset as f32 + scroll_shift;
                    let y = (slot_pos + SL_SONG_SEARCH_WHEEL_FOCUS_SLOT as f32 + 1.0)
                        .mul_add(SL_SONG_SEARCH_ROW_SPACING, list_base_y);
                    let focused = slot_pos.abs() < 0.5;
                    let mut text = "Exit".to_string();
                    let mut color_rgba = [1.0, 0.2, 0.2, 1.0];
                    if row_idx < results.candidates.len() {
                        let song = &results.candidates[row_idx].song;
                        text = song.display_title(false).to_string();
                        color_rgba = [1.0, 1.0, 1.0, 1.0];
                    }
                    if focused {
                        color_rgba[0] = selected_color[0];
                        color_rgba[1] = selected_color[1];
                        color_rgba[2] = selected_color[2];
                    } else {
                        color_rgba[0] *= 0.533;
                        color_rgba[1] *= 0.533;
                        color_rgba[2] *= 0.533;
                    }
                    let alpha = [0.0, 1.0]
                        [(slot_idx > 0 && slot_idx + 1 < SL_SONG_SEARCH_WHEEL_SLOTS) as usize];
                    color_rgba[3] *= alpha;
                    let mut row = act!(text:
                        font("miso"):
                        settext(text):
                        align(0.5, 0.5):
                        xy(list_x, y):
                        maxwidth(310.0):
                        zoom([0.9, 1.0][focused as usize]):
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
                        let y = pane_cy - SL_SONG_SEARCH_PANE_H * 0.5
                            + (SL_SONG_SEARCH_TEXT_H * 0.8 + 8.0) * (i as f32 * 2.0 + 1.0);
                        actors.push(act!(text:
                            font("miso"):
                            settext(format!("{label}:")):
                            align(0.0, 0.5):
                            xy(pane_cx + 10.0, y):
                            zoom(0.64):
                            maxwidth(180.0):
                            diffuse(0.67, 0.67, 1.0, 1.0):
                            z(1454):
                            horizalign(left)
                        ));
                        actors.push(act!(text:
                            font("miso"):
                            settext(value):
                            align(0.0, 0.5):
                            xy(pane_cx + 40.0, y + 13.0):
                            zoom(0.64):
                            maxwidth(210.0):
                            diffuse(1.0, 1.0, 1.0, 1.0):
                            z(1454):
                            horizalign(left)
                        ));
                    }
                }
            }
            SongSearchState::Hidden => {}
        }
        return actors;
    }

    if let SortMenuState::Visible { selected_index } = state.sort_menu {
        let sort_items = sort_menu_items(state);
        let selected_index = selected_index.min(sort_items.len().saturating_sub(1));
        let cx = screen_center_x();
        let cy = screen_center_y();
        let clip_rect = [
            cx - SL_SORT_MENU_WIDTH * 0.5,
            cy - SL_SORT_MENU_HEIGHT * 0.5,
            SL_SORT_MENU_WIDTH,
            SL_SORT_MENU_HEIGHT,
        ];
        let selected_color = color::simply_love_rgba(state.active_color_index);
        actors.push(act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, SL_SORT_MENU_DIM_ALPHA):
            z(1450)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5): xy(cx, cy + SL_SORT_MENU_HEADER_Y_OFFSET):
            zoomto(SL_SORT_MENU_WIDTH + 2.0, 22.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(1451)
        ));
        actors.push(act!(text:
            font("wendy"):
            settext("OPTIONS"):
            align(0.5, 0.5):
            xy(cx, cy + SL_SORT_MENU_HEADER_Y_OFFSET):
            zoom(0.4):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(1452):
            horizalign(center)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5): xy(cx, cy):
            zoomto(SL_SORT_MENU_WIDTH + 2.0, SL_SORT_MENU_HEIGHT + 2.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(1451)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5): xy(cx, cy):
            zoomto(SL_SORT_MENU_WIDTH, SL_SORT_MENU_HEIGHT):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(1452)
        ));
        if !sort_items.is_empty() {
            let focus_t = (state.sort_menu_focus_anim_elapsed
                / SL_SORT_MENU_FOCUS_TWEEN_SECONDS.max(1e-6))
            .clamp(0.0, 1.0);
            let scroll_dir = sort_menu_scroll_dir(
                sort_items.len(),
                state
                    .sort_menu_prev_selected_index
                    .min(sort_items.len().saturating_sub(1)),
                selected_index,
            ) as f32;
            let scroll_shift = scroll_dir
                * [1.0 - focus_t, 0.0][(state.sort_menu_focus_anim_elapsed
                    >= SL_SORT_MENU_FOCUS_TWEEN_SECONDS)
                    as usize];
            let selected_rgba = [selected_color[0], selected_color[1], selected_color[2], 1.0];
            let mut draw_row = |item_idx: usize, slot_pos: f32| {
                let focus_lerp = (1.0 - slot_pos.abs()).clamp(0.0, 1.0);
                let row_zoom = (SL_SORT_MENU_FOCUSED_ROW_ZOOM - SL_SORT_MENU_UNFOCUSED_ROW_ZOOM)
                    .mul_add(focus_lerp, SL_SORT_MENU_UNFOCUSED_ROW_ZOOM);
                let row_alpha = (3.0 - slot_pos.abs()).clamp(0.0, 1.0);
                let text_color = [
                    (selected_rgba[0] - 0.533).mul_add(focus_lerp, 0.533),
                    (selected_rgba[1] - 0.533).mul_add(focus_lerp, 0.533),
                    (selected_rgba[2] - 0.533).mul_add(focus_lerp, 0.533),
                    row_alpha,
                ];
                let y = slot_pos.mul_add(SL_SORT_MENU_ITEM_SPACING, cy);
                let item = &sort_items[item_idx];

                let mut top = act!(text:
                    font("miso"):
                    settext(item.top_label):
                    align(0.5, 0.5):
                    xy(cx, y + SL_SORT_MENU_ITEM_TOP_Y_OFFSET * row_zoom):
                    zoom(SL_SORT_MENU_TOP_TEXT_BASE_ZOOM * row_zoom):
                    diffuse(text_color[0], text_color[1], text_color[2], text_color[3]):
                    z(1454):
                    horizalign(center)
                );
                set_text_clip_rect(&mut top, clip_rect);
                actors.push(top);

                let mut bottom = act!(text:
                    font("wendy"):
                    settext(item.bottom_label):
                    align(0.5, 0.5):
                    xy(cx, y + SL_SORT_MENU_ITEM_BOTTOM_Y_OFFSET * row_zoom):
                    maxwidth(405.0):
                    zoom(SL_SORT_MENU_BOTTOM_TEXT_BASE_ZOOM * row_zoom):
                    diffuse(text_color[0], text_color[1], text_color[2], text_color[3]):
                    z(1454):
                    horizalign(center)
                );
                set_text_clip_rect(&mut bottom, clip_rect);
                actors.push(bottom);
            };

            if sort_items.len() <= SL_SORT_MENU_VISIBLE_ROWS {
                let span = sort_items.len();
                let first_offset = -((span as isize).saturating_sub(1) / 2);
                for i in 0..span {
                    let offset = first_offset + i as isize;
                    let item_idx = ((selected_index as isize + offset)
                        .rem_euclid(sort_items.len() as isize))
                        as usize;
                    let slot_pos = offset as f32 + scroll_shift;
                    draw_row(item_idx, slot_pos);
                }
            } else {
                for slot_idx in 0..SL_SORT_MENU_WHEEL_SLOTS {
                    let offset = slot_idx as isize - SL_SORT_MENU_WHEEL_FOCUS_SLOT as isize;
                    let item_idx = ((selected_index as isize + offset)
                        .rem_euclid(sort_items.len() as isize))
                        as usize;
                    let slot_pos = offset as f32 + scroll_shift;
                    draw_row(item_idx, slot_pos);
                }
            }
        }
        actors.push(act!(text:
            font("wendy"):
            settext(SL_SORT_MENU_HINT_TEXT):
            align(0.5, 0.5):
            xy(cx, cy + SL_SORT_MENU_HINT_Y_OFFSET):
            zoom(0.26):
            diffuse(0.7, 0.7, 0.7, 1.0):
            z(1454):
            horizalign(center)
        ));
    }

    if let LeaderboardOverlayState::Visible(overlay) = &state.leaderboard {
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

            let header_y =
                pane_cy - GS_LEADERBOARD_PANE_HEIGHT * 0.5 + GS_LEADERBOARD_ROW_HEIGHT * 0.5;
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
                let mut score_col = [1.0, 1.0, 1.0, 1.0];
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
    }

    // Simply Love ScreenSelectMusic out transition: "Press &START; for options"
    if state.out_prompt != OutPromptState::None {
        actors.push(act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 0.0):
            cropbottom(1.0):
            fadebottom(0.5):
            z(1400):
            linear(TRANSITION_OUT_DURATION): cropbottom(-0.5): alpha(1.0)
        ));

        match state.out_prompt {
            OutPromptState::PressStartForOptions { .. } => {
                actors.push(act!(text:
                    font("wendy"):
                    settext(PRESS_START_FOR_OPTIONS_TEXT):
                    align(0.5, 0.5):
                    xy(screen_center_x(), screen_center_y()):
                    zoom(0.75):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(1401)
                ));
            }
            OutPromptState::EnteringOptions { .. } => {
                // Fade out "Press Start for options"
                actors.push(act!(text:
                    font("wendy"):
                    settext(PRESS_START_FOR_OPTIONS_TEXT):
                    align(0.5, 0.5):
                    xy(screen_center_x(), screen_center_y()):
                    zoom(0.75):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(1401):
                    linear(ENTERING_OPTIONS_FADE_OUT_SECONDS): alpha(0.0)
                ));

                // Fade in "Entering Options..." after 0.1s hibernate
                actors.push(act!(text:
                    font("wendy"):
                    settext(ENTERING_OPTIONS_TEXT):
                    align(0.5, 0.5):
                    xy(screen_center_x(), screen_center_y()):
                    zoom(0.75):
                    diffuse(1.0, 1.0, 1.0, 0.0):
                    z(1401):
                    sleep(ENTERING_OPTIONS_FADE_OUT_SECONDS + ENTERING_OPTIONS_HIBERNATE_SECONDS):
                    linear(ENTERING_OPTIONS_FADE_IN_SECONDS): alpha(1.0):
                    sleep(ENTERING_OPTIONS_HOLD_SECONDS)
                ));
            }
            OutPromptState::None => {}
        }
    }

    // Simply Love "Exit from Event Mode" prompt overlay.
    if let ExitPromptState::Active {
        elapsed,
        active_choice,
        switch_from,
        switch_elapsed,
    } = state.exit_prompt
    {
        let choices_alpha = if elapsed <= SL_EXIT_PROMPT_CHOICES_DELAY_SECONDS {
            0.0
        } else {
            ((elapsed - SL_EXIT_PROMPT_CHOICES_DELAY_SECONDS) / SL_EXIT_PROMPT_CHOICES_FADE_SECONDS)
                .clamp(0.0, 1.0)
        };
        let p2_color = color::simply_love_rgba(state.active_color_index - 2);

        actors.push(act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, SL_EXIT_PROMPT_BG_ALPHA):
            z(1500)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(SL_EXIT_PROMPT_TEXT):
            align(0.5, 0.0):
            xy(screen_center_x(), screen_center_y() + SL_EXIT_PROMPT_PROMPT_Y_OFFSET):
            zoom(SL_EXIT_PROMPT_PROMPT_ZOOM):
            maxwidth(420.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(1501):
            horizalign(center)
        ));

        let zoom_no = exit_prompt_choice_zoom(0, active_choice, switch_from, switch_elapsed);
        let zoom_yes = exit_prompt_choice_zoom(1, active_choice, switch_from, switch_elapsed);
        let cx = screen_center_x();
        push_exit_prompt_choice(
            &mut actors,
            cx - SL_EXIT_PROMPT_CHOICE_X_OFFSET,
            SL_EXIT_PROMPT_CHOICE_Y,
            SL_EXIT_PROMPT_NO_LABEL,
            SL_EXIT_PROMPT_NO_INFO,
            active_choice == 0,
            zoom_no,
            p2_color,
            choices_alpha,
            1502,
        );
        push_exit_prompt_choice(
            &mut actors,
            cx + SL_EXIT_PROMPT_CHOICE_X_OFFSET,
            SL_EXIT_PROMPT_CHOICE_Y,
            SL_EXIT_PROMPT_YES_LABEL,
            SL_EXIT_PROMPT_YES_INFO,
            active_choice == 1,
            zoom_yes,
            p2_color,
            choices_alpha,
            1502,
        );
    }

    actors
}

#[inline(always)]
fn begin_exit_prompt(state: &mut State) {
    state.exit_prompt = ExitPromptState::Active {
        elapsed: 0.0,
        active_choice: 0,
        switch_from: None,
        switch_elapsed: 0.0,
    };
    // Match SL's `MusicWheel:Move(0)` intent: stop any ongoing hold-scroll.
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
}

#[inline(always)]
fn exit_prompt_choice_zoom(
    choice: u8,
    active_choice: u8,
    switch_from: Option<u8>,
    switch_elapsed: f32,
) -> f32 {
    #[inline(always)]
    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        (b - a).mul_add(t, a)
    }

    if let Some(from) = switch_from {
        let t = (switch_elapsed / SL_EXIT_PROMPT_CHOICE_TWEEN_SECONDS).clamp(0.0, 1.0);
        if choice == from {
            return lerp(SL_EXIT_PROMPT_ACTIVE_ZOOM, SL_EXIT_PROMPT_INACTIVE_ZOOM, t);
        }
        if choice == active_choice {
            return lerp(SL_EXIT_PROMPT_INACTIVE_ZOOM, SL_EXIT_PROMPT_ACTIVE_ZOOM, t);
        }
    }

    [SL_EXIT_PROMPT_INACTIVE_ZOOM, SL_EXIT_PROMPT_ACTIVE_ZOOM][(choice == active_choice) as usize]
}

#[allow(clippy::too_many_arguments)]
fn push_exit_prompt_choice(
    out: &mut Vec<Actor>,
    cx: f32,
    cy: f32,
    label: &str,
    info: &str,
    active: bool,
    choice_zoom: f32,
    active_rgba: [f32; 4],
    alpha: f32,
    z: i16,
) {
    let mut rgba = [1.0; 4];
    if active {
        rgba = active_rgba;
    }
    rgba[3] *= alpha;

    out.push(act!(text:
        align(0.5, 0.5):
        xy(cx, cy):
        font("wendy"):
        zoom(SL_EXIT_PROMPT_LABEL_ZOOM * choice_zoom):
        settext(label):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
        z(z):
        horizalign(center)
    ));
    out.push(act!(text:
        align(0.5, 0.5):
        xy(cx, cy + SL_EXIT_PROMPT_INFO_Y_OFFSET * choice_zoom):
        font("miso"):
        zoom(SL_EXIT_PROMPT_INFO_ZOOM * choice_zoom):
        settext(info):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
        z(z):
        horizalign(center)
    ));
}

fn handle_exit_prompt_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }
    let ExitPromptState::Active { active_choice, .. } = state.exit_prompt else {
        return ScreenAction::None;
    };

    match ev.action {
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => {
            let ExitPromptState::Active {
                active_choice,
                switch_from,
                switch_elapsed,
                ..
            } = &mut state.exit_prompt
            else {
                return ScreenAction::None;
            };
            let prev = *active_choice;
            *active_choice = 1 - prev;
            *switch_from = Some(prev);
            *switch_elapsed = 0.0;
            audio::play_sfx("assets/sounds/change.ogg");
            ScreenAction::None
        }

        VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            audio::play_sfx("assets/sounds/start.ogg");
            state.exit_prompt = ExitPromptState::None;
            ScreenAction::None
        }

        VirtualAction::p1_start | VirtualAction::p2_start => {
            audio::play_sfx("assets/sounds/start.ogg");
            state.exit_prompt = ExitPromptState::None;
            if active_choice == 1 {
                ScreenAction::Navigate(Screen::Menu)
            } else {
                ScreenAction::None
            }
        }

        _ => ScreenAction::None,
    }
}
