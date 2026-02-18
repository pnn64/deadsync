use crate::act;
use crate::assets::{AssetManager, DensityGraphSlot, DensityGraphSource};
use crate::config::{self, BreakdownStyle, SelectMusicPatternInfoMode};
use crate::core::audio;
use crate::core::gfx::{BlendMode, MeshMode, MeshVertex};
use crate::core::input::{InputEvent, PadDir, VirtualAction};
use crate::core::space::{
    is_wide, screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use crate::game::chart::ChartData;
use crate::game::parsing::simfile as song_loading;
use crate::game::profile;
use crate::game::scores;
use crate::game::song::{SongData, get_song_cache};
use crate::rgba_const;
use crate::screens::components::{
    gs_scorebox, heart_bg, music_wheel, pad_display, profile_boxes, select_pane, select_shared,
    sort_menu, step_artist_bar, test_input,
};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use crate::ui::font;
use log::info;
use rssp::bpm::parse_bpm_map;
use std::cmp::Reverse;
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
// ITGmania InputQueue: g_fSimultaneousThreshold = 0.05f.
const CHORD_SIMULTANEOUS_WINDOW: Duration = Duration::from_millis(50);
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

// Simply Love [ScreenSelectMusic] [MusicWheel]: RecentSongsToShow=30.
const RECENT_SONGS_TO_SHOW: usize = 30;
const POPULAR_SONGS_TO_SHOW: usize = 50;
const RECENT_SORT_HEADER: &str = "Recently Played";
const POPULAR_SORT_HEADER: &str = "Most Popular";
const AUTO_STAMINA_MIN_METER: u32 = 11;
const AUTO_STAMINA_MIN_STREAM_PERCENT: f32 = 10.0;
const AUTO_STAMINA_MAX_CROSSOVERS: u32 = 9;
const AUTO_STAMINA_MAX_SIDESWITCHES: u32 = 9;

#[inline(always)]
fn chart_stream_percent(chart: &ChartData) -> f32 {
    if chart.total_measures == 0 {
        return 0.0;
    }
    (chart.total_streams as f32 / chart.total_measures as f32) * 100.0
}

#[inline(always)]
fn chart_is_stamina_like(chart: &ChartData) -> bool {
    chart.meter >= AUTO_STAMINA_MIN_METER
        && chart_stream_percent(chart) >= AUTO_STAMINA_MIN_STREAM_PERCENT
        && chart.tech_counts.crossovers <= AUTO_STAMINA_MAX_CROSSOVERS
        && chart.tech_counts.sideswitches <= AUTO_STAMINA_MAX_SIDESWITCHES
}

#[inline(always)]
fn show_stamina_panel(mode: SelectMusicPatternInfoMode, chart: Option<&ChartData>) -> bool {
    match mode {
        SelectMusicPatternInfoMode::Tech => false,
        SelectMusicPatternInfoMode::Stamina => true,
        SelectMusicPatternInfoMode::Auto => chart.is_some_and(chart_is_stamina_like),
    }
}

#[inline(always)]
const fn chord_bit(dir: PadDir) -> u8 {
    match dir {
        PadDir::Up => CHORD_UP,
        PadDir::Down => CHORD_DOWN,
        _ => 0,
    }
}

#[inline(always)]
fn chord_times_are_simultaneous(a: Option<Instant>, b: Option<Instant>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            if a >= b {
                a.duration_since(b) <= CHORD_SIMULTANEOUS_WINDOW
            } else {
                b.duration_since(a) <= CHORD_SIMULTANEOUS_WINDOW
            }
        }
        _ => false,
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
fn sl_selection_anim_beat(entry_opt: Option<&MusicWheelEntry>, state: &State) -> f32 {
    match entry_opt {
        Some(MusicWheelEntry::Song(song)) => preview_song_sec(state).map_or(
            state.session_elapsed * song.max_bpm.max(1.0) as f32 / 60.0,
            |sec| beat_at_sec(song, sec) as f32,
        ),
        _ => state.session_elapsed * 2.5, // 150 BPM fallback
    }
}

#[inline(always)]
fn sl_arrow_bounce01(entry_opt: Option<&MusicWheelEntry>, state: &State) -> f32 {
    let beat = sl_selection_anim_beat(entry_opt, state);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WheelSortMode {
    Group,
    Title,
    Artist,
    Bpm,
    Length,
    Meter,
    Popularity,
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
    chart_ix: usize,
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
    pub gameplay_elapsed: f32,
    displayed_chart_p1: Option<DisplayedChart>,
    displayed_chart_p2: Option<DisplayedChart>,

    // Internal state
    out_prompt: OutPromptState,
    exit_prompt: ExitPromptState,
    reload_ui: Option<ReloadUiState>,
    song_search: sort_menu::SongSearchState,
    song_search_ignore_next_back_select: bool,
    replay_overlay: sort_menu::ReplayOverlayState,
    test_input_overlay_visible: bool,
    test_input_overlay: test_input::State,
    profile_switch_overlay: Option<profile_boxes::State>,
    pending_replay: Option<sort_menu::ReplayStartPayload>,
    sort_menu: sort_menu::State,
    leaderboard: sort_menu::LeaderboardOverlayState,
    sort_mode: WheelSortMode,
    all_entries: Vec<MusicWheelEntry>,
    group_entries: Vec<MusicWheelEntry>,
    title_entries: Vec<MusicWheelEntry>,
    artist_entries: Vec<MusicWheelEntry>,
    bpm_entries: Vec<MusicWheelEntry>,
    length_entries: Vec<MusicWheelEntry>,
    meter_entries: Vec<MusicWheelEntry>,
    popularity_entries: Vec<MusicWheelEntry>,
    recent_entries: Vec<MusicWheelEntry>,
    expanded_pack_name: Option<String>,
    bg: heart_bg::State,
    last_requested_banner_path: Option<PathBuf>,
    banner_high_quality_requested: bool,
    last_requested_chart_hash: Option<String>,
    last_requested_chart_hash_p2: Option<String>,
    chord_mask_p1: u8,
    chord_mask_p2: u8,
    menu_chord_mask: u8,
    p1_chord_up_pressed_at: Option<Instant>,
    p1_chord_down_pressed_at: Option<Instant>,
    p2_chord_up_pressed_at: Option<Instant>,
    p2_chord_down_pressed_at: Option<Instant>,
    menu_chord_left_pressed_at: Option<Instant>,
    menu_chord_right_pressed_at: Option<Instant>,
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
    pack_total_seconds_by_index: Vec<f64>,
    song_has_edit_ptrs: HashSet<usize>,
    pub pack_song_counts: HashMap<String, usize>,
    group_pack_song_counts: HashMap<String, usize>,
    title_pack_song_counts: HashMap<String, usize>,
    artist_pack_song_counts: HashMap<String, usize>,
    bpm_pack_song_counts: HashMap<String, usize>,
    length_pack_song_counts: HashMap<String, usize>,
    meter_pack_song_counts: HashMap<String, usize>,
    popularity_pack_song_counts: HashMap<String, usize>,
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
    edits.sort_by_cached_key(|c| {
        (
            c.description.to_lowercase(),
            c.meter,
            c.short_hash.as_str(),
        )
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
    indices.sort_by_cached_key(|&idx| {
        let c = &song.charts[idx];
        (
            c.description.to_lowercase(),
            c.meter,
            c.short_hash.as_str(),
        )
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

#[inline(always)]
fn displayed_chart_matches(
    displayed: Option<&DisplayedChart>,
    song: &Arc<SongData>,
    desired_ix: Option<usize>,
) -> bool {
    match (displayed, desired_ix) {
        (Some(d), Some(ix)) => Arc::ptr_eq(&d.song, song) && d.chart_ix == ix,
        (None, None) => true,
        _ => false,
    }
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
fn alpha_group_bucket_from_text(text: &str) -> u8 {
    let first = text.trim_start().chars().next();
    match first {
        Some(ch) if ch.is_ascii_digit() => 1,
        Some(ch) if ch.is_ascii_alphabetic() => {
            let c = ch.to_ascii_uppercase();
            (c as u8).saturating_sub(b'A').saturating_add(2)
        }
        _ => 0,
    }
}

#[inline(always)]
fn alpha_group_meta_from_text(text: &str) -> (u8, String) {
    let bucket = alpha_group_bucket_from_text(text);
    let label = match bucket {
        0 => "Other".to_string(),
        1 => "0-9".to_string(),
        b => ((b'A' + b.saturating_sub(2)) as char).to_string(),
    };
    (bucket, label)
}

#[inline(always)]
fn title_group_bucket(song: &SongData) -> u8 {
    let title = if song.translit_title.trim().is_empty() {
        song.title.as_str()
    } else {
        song.translit_title.as_str()
    };
    alpha_group_bucket_from_text(title)
}

#[inline(always)]
fn title_group_label(song: &SongData) -> String {
    let bucket = title_group_bucket(song);
    match bucket {
        0 => "Other".to_string(),
        1 => "0-9".to_string(),
        b => ((b'A' + b.saturating_sub(2)) as char).to_string(),
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

    songs.sort_by_cached_key(|song| {
        (
            title_group_bucket(song.as_ref()),
            song_title_sort_key(song.as_ref()),
            song.title.clone(),
            song.subtitle.clone(),
        )
    });

    let mut entries: Vec<MusicWheelEntry> = Vec::with_capacity(songs.len().saturating_add(32));
    let mut counts: HashMap<String, usize> = HashMap::with_capacity(32);
    let mut current_group: Option<String> = None;
    let mut header_idx = 0usize;

    for song in songs {
        let group_name = title_group_label(song.as_ref());
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

#[inline(always)]
fn song_artist_sort_key(song: &SongData) -> (String, String) {
    (
        song.artist.to_ascii_lowercase(),
        song.simfile_path.to_string_lossy().to_ascii_lowercase(),
    )
}

fn build_artist_grouped_entries(
    grouped_entries: &[MusicWheelEntry],
) -> (Vec<MusicWheelEntry>, HashMap<String, usize>) {
    let mut songs: Vec<Arc<SongData>> = grouped_entries
        .iter()
        .filter_map(|e| match e {
            MusicWheelEntry::Song(song) => Some(song.clone()),
            MusicWheelEntry::PackHeader { .. } => None,
        })
        .collect();

    songs.sort_by_cached_key(|song| {
        (
            alpha_group_bucket_from_text(&song.artist),
            song_artist_sort_key(song.as_ref()),
            song_title_sort_key(song.as_ref()),
        )
    });

    let mut entries: Vec<MusicWheelEntry> = Vec::with_capacity(songs.len().saturating_add(32));
    let mut counts: HashMap<String, usize> = HashMap::with_capacity(32);
    let mut current_group: Option<String> = None;
    let mut header_idx = 0usize;

    for song in songs {
        let (_, group_name) = alpha_group_meta_from_text(&song.artist);
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

#[inline(always)]
fn song_bpm_for_sort(song: &SongData) -> i32 {
    song_display_bpm_range(song).map_or(0, |(_lo, hi)| hi.max(0.0) as i32)
}

fn song_display_bpm_range(song: &SongData) -> Option<(f64, f64)> {
    song.display_bpm_range()
}

#[inline(always)]
fn bpm_bucket_name(max_bpm: i32) -> String {
    const SORT_BPM_DIVISION: i32 = 10;
    let mut hi = max_bpm.max(0);
    let rem = hi.rem_euclid(SORT_BPM_DIVISION);
    hi += SORT_BPM_DIVISION - rem - 1;
    let lo = hi - (SORT_BPM_DIVISION - 1);
    format!("{lo:03}-{hi:03}")
}

fn build_bpm_grouped_entries(
    grouped_entries: &[MusicWheelEntry],
) -> (Vec<MusicWheelEntry>, HashMap<String, usize>) {
    let mut songs: Vec<Arc<SongData>> = grouped_entries
        .iter()
        .filter_map(|e| match e {
            MusicWheelEntry::Song(song) => Some(song.clone()),
            MusicWheelEntry::PackHeader { .. } => None,
        })
        .collect();

    songs.sort_by_cached_key(|song| {
        (song_bpm_for_sort(song.as_ref()), song_title_sort_key(song.as_ref()))
    });

    let mut entries: Vec<MusicWheelEntry> = Vec::with_capacity(songs.len().saturating_add(32));
    let mut counts: HashMap<String, usize> = HashMap::with_capacity(32);
    let mut current_group: Option<String> = None;
    let mut header_idx = 0usize;

    for song in songs {
        let group_name = bpm_bucket_name(song_bpm_for_sort(song.as_ref()));
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

#[inline(always)]
fn song_length_for_sort(song: &SongData) -> i32 {
    if song.music_length_seconds.is_finite() && song.music_length_seconds > 0.0 {
        song.music_length_seconds.max(0.0) as i32
    } else {
        song.total_length_seconds.max(0)
    }
}

#[inline(always)]
fn length_bucket_name(length_seconds: i32) -> String {
    const SORT_LENGTH_DIVISION: i32 = 60;
    let mut hi = length_seconds.max(0);
    let rem = hi.rem_euclid(SORT_LENGTH_DIVISION);
    hi += SORT_LENGTH_DIVISION - rem - 1;
    let lo = hi - (SORT_LENGTH_DIVISION - 1);
    format!("{}-{}", format_chart_length(lo), format_chart_length(hi))
}

fn build_length_grouped_entries(
    grouped_entries: &[MusicWheelEntry],
) -> (Vec<MusicWheelEntry>, HashMap<String, usize>) {
    let mut songs: Vec<Arc<SongData>> = grouped_entries
        .iter()
        .filter_map(|e| match e {
            MusicWheelEntry::Song(song) => Some(song.clone()),
            MusicWheelEntry::PackHeader { .. } => None,
        })
        .collect();

    songs.sort_by_cached_key(|song| {
        (
            song_length_for_sort(song.as_ref()),
            song_title_sort_key(song.as_ref()),
        )
    });

    let mut entries: Vec<MusicWheelEntry> = Vec::with_capacity(songs.len().saturating_add(32));
    let mut counts: HashMap<String, usize> = HashMap::with_capacity(32);
    let mut current_group: Option<String> = None;
    let mut header_idx = 0usize;

    for song in songs {
        let group_name = length_bucket_name(song_length_for_sort(song.as_ref()));
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

fn song_meter_for_sort(song: &SongData, chart_type: &str) -> Option<u32> {
    let mut best_non_edit: Option<u32> = None;
    let mut best_any: Option<u32> = None;
    for chart in &song.charts {
        if !chart.chart_type.eq_ignore_ascii_case(chart_type) || chart.notes.is_empty() {
            continue;
        }
        best_any = Some(best_any.map_or(chart.meter, |m| m.max(chart.meter)));
        if !chart.difficulty.eq_ignore_ascii_case("edit") {
            best_non_edit = Some(best_non_edit.map_or(chart.meter, |m| m.max(chart.meter)));
        }
    }
    best_non_edit.or(best_any)
}

#[inline(always)]
fn meter_bucket_name(meter: Option<u32>) -> String {
    meter.map_or_else(|| "N/A".to_string(), |m| format!("{:02}", m.min(99)))
}

fn build_meter_grouped_entries(
    grouped_entries: &[MusicWheelEntry],
    chart_type: &str,
) -> (Vec<MusicWheelEntry>, HashMap<String, usize>) {
    let mut songs: Vec<Arc<SongData>> = grouped_entries
        .iter()
        .filter_map(|e| match e {
            MusicWheelEntry::Song(song) => Some(song.clone()),
            MusicWheelEntry::PackHeader { .. } => None,
        })
        .collect();

    songs.sort_by_cached_key(|song| {
        (
            song_meter_for_sort(song.as_ref(), chart_type).unwrap_or(u32::MAX),
            song_title_sort_key(song.as_ref()),
        )
    });

    let mut entries: Vec<MusicWheelEntry> = Vec::with_capacity(songs.len().saturating_add(32));
    let mut counts: HashMap<String, usize> = HashMap::with_capacity(32);
    let mut current_group: Option<String> = None;
    let mut header_idx = 0usize;

    for song in songs {
        let group_name = meter_bucket_name(song_meter_for_sort(song.as_ref(), chart_type));
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

fn build_popularity_grouped_entries(
    grouped_entries: &[MusicWheelEntry],
) -> (Vec<MusicWheelEntry>, HashMap<String, usize>) {
    let songs: Vec<Arc<SongData>> = grouped_entries
        .iter()
        .filter_map(|e| match e {
            MusicWheelEntry::Song(song) => Some(song.clone()),
            MusicWheelEntry::PackHeader { .. } => None,
        })
        .collect();
    let mut hash_to_song_ix: HashMap<&str, usize> =
        HashMap::with_capacity(songs.len().saturating_mul(8));
    for (song_ix, song) in songs.iter().enumerate() {
        for chart in &song.charts {
            if chart.notes.is_empty() {
                continue;
            }
            hash_to_song_ix
                .entry(chart.short_hash.as_str())
                .or_insert(song_ix);
        }
    }
    let mut song_play_counts = vec![0u32; songs.len()];
    for (chart_hash, chart_plays) in scores::played_chart_counts_for_machine() {
        let Some(&song_ix) = hash_to_song_ix.get(chart_hash.as_str()) else {
            continue;
        };
        song_play_counts[song_ix] = song_play_counts[song_ix].saturating_add(chart_plays);
    }
    let mut ranked: Vec<(Arc<SongData>, u32)> = songs
        .into_iter()
        .enumerate()
        .map(|(song_ix, song)| (song, song_play_counts[song_ix]))
        .collect();

    ranked.sort_by_cached_key(|(song, play_count)| {
        (Reverse(*play_count), song_title_sort_key(song.as_ref()))
    });
    ranked.truncate(POPULAR_SONGS_TO_SHOW.min(ranked.len()));

    let count = ranked.len();
    let mut entries: Vec<MusicWheelEntry> = Vec::with_capacity(count.saturating_add(1));
    entries.push(MusicWheelEntry::PackHeader {
        name: POPULAR_SORT_HEADER.to_string(),
        original_index: 0,
        banner_path: None,
    });
    entries.extend(
        ranked
            .into_iter()
            .map(|(song, _)| MusicWheelEntry::Song(song)),
    );

    let mut counts: HashMap<String, usize> = HashMap::with_capacity(1);
    counts.insert(POPULAR_SORT_HEADER.to_string(), count);
    (entries, counts)
}

fn build_recent_grouped_entries(
    grouped_entries: &[MusicWheelEntry],
) -> (Vec<MusicWheelEntry>, HashMap<String, usize>) {
    let songs: Vec<Arc<SongData>> = grouped_entries
        .iter()
        .filter_map(|e| match e {
            MusicWheelEntry::Song(song) => Some(song.clone()),
            MusicWheelEntry::PackHeader { .. } => None,
        })
        .collect();

    let mut hash_to_song_ix: HashMap<&str, usize> =
        HashMap::with_capacity(songs.len().saturating_mul(8));
    for (song_ix, song) in songs.iter().enumerate() {
        for chart in &song.charts {
            if chart.notes.is_empty() {
                continue;
            }
            hash_to_song_ix
                .entry(chart.short_hash.as_str())
                .or_insert(song_ix);
        }
    }

    let mut recent_song_ixs: Vec<usize> = Vec::with_capacity(RECENT_SONGS_TO_SHOW);
    let mut seen_song_ix = vec![false; songs.len()];

    for chart_hash in scores::recent_played_chart_hashes_for_machine() {
        let Some(&song_ix) = hash_to_song_ix.get(chart_hash.as_str()) else {
            continue;
        };
        if seen_song_ix[song_ix] {
            continue;
        }
        seen_song_ix[song_ix] = true;
        recent_song_ixs.push(song_ix);
        if recent_song_ixs.len() >= RECENT_SONGS_TO_SHOW {
            break;
        }
    }

    let count = recent_song_ixs.len();
    let mut entries: Vec<MusicWheelEntry> = Vec::with_capacity(count.saturating_add(1));
    entries.push(MusicWheelEntry::PackHeader {
        name: RECENT_SORT_HEADER.to_string(),
        original_index: 0,
        banner_path: None,
    });
    entries.extend(
        recent_song_ixs
            .into_iter()
            .map(|song_ix| MusicWheelEntry::Song(songs[song_ix].clone())),
    );

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

fn refresh_popularity_cache(state: &mut State) {
    let (popularity_entries, popularity_pack_song_counts) =
        build_popularity_grouped_entries(&state.group_entries);
    state.popularity_entries = popularity_entries;
    state.popularity_pack_song_counts = popularity_pack_song_counts;
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
        WheelSortMode::Artist => {
            state.all_entries = state.artist_entries.clone();
            state.pack_song_counts = state.artist_pack_song_counts.clone();
            state.expanded_pack_name = selected_song
                .as_ref()
                .and_then(|song| group_name_for_song(&state.artist_entries, song))
                .or_else(|| first_header_name(&state.artist_entries));
        }
        WheelSortMode::Bpm => {
            state.all_entries = state.bpm_entries.clone();
            state.pack_song_counts = state.bpm_pack_song_counts.clone();
            state.expanded_pack_name = selected_song
                .as_ref()
                .and_then(|song| group_name_for_song(&state.bpm_entries, song))
                .or_else(|| first_header_name(&state.bpm_entries));
        }
        WheelSortMode::Length => {
            state.all_entries = state.length_entries.clone();
            state.pack_song_counts = state.length_pack_song_counts.clone();
            state.expanded_pack_name = selected_song
                .as_ref()
                .and_then(|song| group_name_for_song(&state.length_entries, song))
                .or_else(|| first_header_name(&state.length_entries));
        }
        WheelSortMode::Meter => {
            state.all_entries = state.meter_entries.clone();
            state.pack_song_counts = state.meter_pack_song_counts.clone();
            state.expanded_pack_name = selected_song
                .as_ref()
                .and_then(|song| group_name_for_song(&state.meter_entries, song))
                .or_else(|| first_header_name(&state.meter_entries));
        }
        WheelSortMode::Popularity => {
            state.all_entries = state.popularity_entries.clone();
            state.pack_song_counts = state.popularity_pack_song_counts.clone();
            state.expanded_pack_name = selected_song
                .as_ref()
                .and_then(|song| group_name_for_song(&state.popularity_entries, song))
                .or_else(|| first_header_name(&state.popularity_entries));
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
    let mut pack_total_seconds_by_index = vec![0.0_f64; total_packs];
    let mut song_has_edit_ptrs = HashSet::with_capacity(total_songs);

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
        let mut pack_total_seconds = 0.0_f64;

        for song in &pack.songs {
            let mut has_target_chart_type = false;
            let mut has_edit = false;
            for chart in &song.charts {
                if !chart.chart_type.eq_ignore_ascii_case(target_chart_type) {
                    continue;
                }
                has_target_chart_type = true;
                if chart.difficulty.eq_ignore_ascii_case("edit") && !chart.notes.is_empty() {
                    has_edit = true;
                    break;
                }
            }
            if !has_target_chart_type {
                continue;
            }
            if has_edit {
                song_has_edit_ptrs.insert(Arc::as_ptr(song) as usize);
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
            pack_total_seconds += if song.music_length_seconds.is_finite()
                && song.music_length_seconds > 0.0
            {
                song.music_length_seconds as f64
            } else {
                song.total_length_seconds.max(0) as f64
            };
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
            pack_total_seconds_by_index[i] = pack_total_seconds;
        }
    }

    let (title_entries, title_pack_song_counts) = build_title_grouped_entries(&all_entries);
    let (artist_entries, artist_pack_song_counts) = build_artist_grouped_entries(&all_entries);
    let (bpm_entries, bpm_pack_song_counts) = build_bpm_grouped_entries(&all_entries);
    let (length_entries, length_pack_song_counts) = build_length_grouped_entries(&all_entries);
    let (meter_entries, meter_pack_song_counts) =
        build_meter_grouped_entries(&all_entries, target_chart_type);
    let (popularity_entries, popularity_pack_song_counts) =
        build_popularity_grouped_entries(&all_entries);
    let (recent_entries, recent_pack_song_counts) = build_recent_grouped_entries(&all_entries);

    let mut state = State {
        all_entries: all_entries.clone(),
        group_entries: all_entries,
        title_entries,
        artist_entries,
        bpm_entries,
        length_entries,
        meter_entries,
        popularity_entries,
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
        song_search: sort_menu::SongSearchState::Hidden,
        song_search_ignore_next_back_select: false,
        replay_overlay: sort_menu::ReplayOverlayState::Hidden,
        test_input_overlay_visible: false,
        test_input_overlay: test_input::State::default(),
        profile_switch_overlay: None,
        pending_replay: None,
        sort_menu: sort_menu::State::Hidden,
        leaderboard: sort_menu::LeaderboardOverlayState::Hidden,
        sort_mode: WheelSortMode::Group,
        expanded_pack_name: last_pack_name,
        bg: heart_bg::State::new(),
        last_requested_banner_path: None,
        banner_high_quality_requested: false,
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
        p1_chord_up_pressed_at: None,
        p1_chord_down_pressed_at: None,
        p2_chord_up_pressed_at: None,
        p2_chord_down_pressed_at: None,
        menu_chord_left_pressed_at: None,
        menu_chord_right_pressed_at: None,
        last_steps_nav_dir_p1: None,
        last_steps_nav_time_p1: None,
        last_steps_nav_dir_p2: None,
        last_steps_nav_time_p2: None,
        nav_key_held_direction: None,
        nav_key_held_since: None,
        sort_menu_prev_selected_index: 0,
        sort_menu_focus_anim_elapsed: sort_menu::FOCUS_TWEEN_SECONDS,
        currently_playing_preview_path: None,
        currently_playing_preview_start_sec: None,
        currently_playing_preview_length_sec: None,
        session_elapsed: 0.0,
        gameplay_elapsed: 0.0,
        prev_selected_index: 0,
        time_since_selection_change: 0.0,
        cached_song: None,
        cached_chart_type: "",
        cached_steps_index_p1: usize::MAX,
        cached_steps_index_p2: usize::MAX,
        cached_chart_ix_p1: None,
        cached_chart_ix_p2: None,
        cached_edits: None,
        pack_total_seconds_by_index,
        song_has_edit_ptrs,
        pack_song_counts: pack_song_counts.clone(),
        group_pack_song_counts: pack_song_counts,
        title_pack_song_counts,
        artist_pack_song_counts,
        bpm_pack_song_counts,
        length_pack_song_counts,
        meter_pack_song_counts,
        popularity_pack_song_counts,
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
fn clear_menu_chord(state: &mut State) {
    state.menu_chord_mask = 0;
    state.menu_chord_left_pressed_at = None;
    state.menu_chord_right_pressed_at = None;
}

#[inline(always)]
fn clear_p1_ud_chord(state: &mut State) {
    state.chord_mask_p1 = 0;
    state.p1_chord_up_pressed_at = None;
    state.p1_chord_down_pressed_at = None;
}

#[inline(always)]
fn clear_p2_ud_chord(state: &mut State) {
    state.chord_mask_p2 = 0;
    state.p2_chord_up_pressed_at = None;
    state.p2_chord_down_pressed_at = None;
}

#[inline(always)]
fn show_sort_menu(state: &mut State) {
    state.sort_menu = sort_menu::State::Visible {
        page: sort_menu::Page::Main,
        selected_index: 0,
    };
    state.sort_menu_prev_selected_index = 0;
    clear_menu_chord(state);
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.sort_menu_focus_anim_elapsed = sort_menu::FOCUS_TWEEN_SECONDS;
    clear_preview(state);
    audio::play_sfx("assets/sounds/start.ogg");
}

#[inline(always)]
fn hide_sort_menu(state: &mut State) {
    state.sort_menu = sort_menu::State::Hidden;
    clear_menu_chord(state);
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
}

#[inline(always)]
fn try_open_sort_menu(state: &mut State) -> bool {
    if state.menu_chord_mask & (MENU_CHORD_LEFT | MENU_CHORD_RIGHT)
        == (MENU_CHORD_LEFT | MENU_CHORD_RIGHT)
        && chord_times_are_simultaneous(
            state.menu_chord_left_pressed_at,
            state.menu_chord_right_pressed_at,
        )
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
fn sort_submenu_index_for_mode(sort_mode: WheelSortMode) -> usize {
    match sort_mode {
        WheelSortMode::Group => 0,
        WheelSortMode::Title => 1,
        WheelSortMode::Artist => 2,
        WheelSortMode::Bpm => 3,
        WheelSortMode::Length => 4,
        WheelSortMode::Meter => 5,
        WheelSortMode::Popularity => 6,
        WheelSortMode::Recent => 7,
    }
}

#[inline(always)]
fn show_sorts_submenu(state: &mut State) {
    let selected_index = sort_submenu_index_for_mode(state.sort_mode);
    state.sort_menu = sort_menu::State::Visible {
        page: sort_menu::Page::Sorts,
        selected_index,
    };
    state.sort_menu_prev_selected_index = selected_index;
    state.sort_menu_focus_anim_elapsed = 0.0;
}

#[inline(always)]
fn sort_menu_items(state: &State, page: sort_menu::Page) -> &[sort_menu::Item] {
    if page == sort_menu::Page::Sorts {
        return &sort_menu::ITEMS_SORTS;
    }
    let has_song_selected = matches!(
        state.entries.get(state.selected_index),
        Some(MusicWheelEntry::Song(_))
    );
    let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
    let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
    let single_player_joined = p1_joined ^ p2_joined;
    match (
        profile::get_session_play_style(),
        single_player_joined,
        has_song_selected,
    ) {
        (profile::PlayStyle::Single, true, true) => &sort_menu::ITEMS_MAIN_WITH_SWITCH_TO_DOUBLE,
        (profile::PlayStyle::Single, true, false) => {
            &sort_menu::ITEMS_MAIN_WITH_SWITCH_TO_DOUBLE[..6]
        }
        (profile::PlayStyle::Double, true, true) => &sort_menu::ITEMS_MAIN_WITH_SWITCH_TO_SINGLE,
        (profile::PlayStyle::Double, true, false) => {
            &sort_menu::ITEMS_MAIN_WITH_SWITCH_TO_SINGLE[..6]
        }
        (_, _, true) => &sort_menu::ITEMS_MAIN,
        (_, _, false) => &sort_menu::ITEMS_MAIN[..5],
    }
}

#[inline(always)]
fn show_test_input_overlay(state: &mut State) {
    clear_preview(state);
    state.song_search = sort_menu::SongSearchState::Hidden;
    state.leaderboard = sort_menu::LeaderboardOverlayState::Hidden;
    state.replay_overlay = sort_menu::ReplayOverlayState::Hidden;
    state.profile_switch_overlay = None;
    clear_menu_chord(state);
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.test_input_overlay_visible = true;
    test_input::clear(&mut state.test_input_overlay);
}

#[inline(always)]
fn hide_test_input_overlay(state: &mut State) {
    state.test_input_overlay_visible = false;
}

fn start_song_search_prompt(state: &mut State) {
    clear_preview(state);
    state.sort_menu = sort_menu::State::Hidden;
    state.leaderboard = sort_menu::LeaderboardOverlayState::Hidden;
    state.replay_overlay = sort_menu::ReplayOverlayState::Hidden;
    state.profile_switch_overlay = None;
    hide_test_input_overlay(state);
    clear_menu_chord(state);
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.song_search = sort_menu::begin_song_search_prompt();
}

fn show_profile_switch_overlay(state: &mut State) {
    profile::set_fast_profile_switch_from_select_music(false);
    clear_preview(state);
    state.sort_menu = sort_menu::State::Hidden;
    state.song_search = sort_menu::SongSearchState::Hidden;
    state.leaderboard = sort_menu::LeaderboardOverlayState::Hidden;
    state.replay_overlay = sort_menu::ReplayOverlayState::Hidden;
    hide_test_input_overlay(state);
    clear_menu_chord(state);
    clear_p1_ud_chord(state);
    clear_p2_ud_chord(state);
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.last_steps_nav_dir_p1 = None;
    state.last_steps_nav_time_p1 = None;
    state.last_steps_nav_dir_p2 = None;
    state.last_steps_nav_time_p2 = None;

    let mut overlay = profile_boxes::init();
    overlay.active_color_index = state.active_color_index;
    profile_boxes::set_joined(
        &mut overlay,
        profile::is_session_side_joined(profile::PlayerSide::P1),
        profile::is_session_side_joined(profile::PlayerSide::P2),
    );
    state.profile_switch_overlay = Some(overlay);
}

#[inline(always)]
fn restore_sort_menu_after_profile_overlay(state: &mut State) {
    let selected_index = sort_menu_items(state, sort_menu::Page::Main)
        .iter()
        .position(|item| matches!(item.action, sort_menu::Action::SwitchProfile))
        .unwrap_or(0);
    state.sort_menu = sort_menu::State::Visible {
        page: sort_menu::Page::Main,
        selected_index,
    };
    state.sort_menu_prev_selected_index = selected_index;
    state.sort_menu_focus_anim_elapsed = sort_menu::FOCUS_TWEEN_SECONDS;
}

#[inline(always)]
fn close_song_search(state: &mut State) {
    state.song_search = sort_menu::SongSearchState::Hidden;
}

#[inline(always)]
fn cancel_song_search(state: &mut State) {
    state.song_search = sort_menu::SongSearchState::Hidden;
    state.song_search_ignore_next_back_select = true;
}

fn start_song_search_results(state: &mut State, search_text: String) {
    state.song_search = sort_menu::begin_song_search_results(&state.group_entries, search_text);
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
    state.sort_menu = sort_menu::State::Hidden;
    state.leaderboard = sort_menu::LeaderboardOverlayState::Hidden;
    state.replay_overlay = sort_menu::ReplayOverlayState::Hidden;
    state.profile_switch_overlay = None;
    hide_test_input_overlay(state);
    clear_menu_chord(state);
    clear_p1_ud_chord(state);
    clear_p2_ud_chord(state);
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
    let (page, selected_index) = match state.sort_menu {
        sort_menu::State::Visible {
            page,
            selected_index,
        } => (page, selected_index),
        sort_menu::State::Hidden => return,
    };
    let len = sort_menu_items(state, page).len();
    if len == 0 {
        return;
    }
    let old = selected_index.min(len - 1);
    let next = ((old as isize + delta).rem_euclid(len as isize)) as usize;
    if next == old {
        return;
    }
    state.sort_menu_prev_selected_index = old;
    if let sort_menu::State::Visible { selected_index, .. } = &mut state.sort_menu {
        *selected_index = next;
    }
    state.sort_menu_focus_anim_elapsed = 0.0;
    audio::play_sfx("assets/sounds/change.ogg");
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

fn show_leaderboard_overlay(state: &mut State) {
    let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) else {
        return;
    };

    let chart_hash_p1 = selected_chart_hash_for_side(state, song, profile::PlayerSide::P1);
    let chart_hash_p2 = selected_chart_hash_for_side(state, song, profile::PlayerSide::P2);
    if let Some(overlay) = sort_menu::show_leaderboard_overlay(chart_hash_p1, chart_hash_p2) {
        state.replay_overlay = sort_menu::ReplayOverlayState::Hidden;
        state.profile_switch_overlay = None;
        hide_test_input_overlay(state);
        state.leaderboard = overlay;
        clear_preview(state);
    }
}

fn show_replay_overlay(state: &mut State) {
    let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) else {
        return;
    };
    let side = profile::get_session_player_side();
    let Some(chart_hash) = selected_chart_hash_for_side(state, song, side) else {
        return;
    };
    let overlay = sort_menu::begin_replay_overlay(&chart_hash);
    if matches!(overlay, sort_menu::ReplayOverlayState::Hidden) {
        return;
    }
    state.leaderboard = sort_menu::LeaderboardOverlayState::Hidden;
    state.profile_switch_overlay = None;
    hide_test_input_overlay(state);
    state.replay_overlay = overlay;
    clear_preview(state);
}

fn switch_single_player_style(state: &mut State, new_style: profile::PlayStyle) {
    hide_sort_menu(state);

    let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
    let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
    let side = match (p1_joined, p2_joined) {
        (true, false) => profile::PlayerSide::P1,
        (false, true) => profile::PlayerSide::P2,
        _ => profile::get_session_player_side(),
    };
    match side {
        profile::PlayerSide::P1 => profile::set_session_joined(true, false),
        profile::PlayerSide::P2 => profile::set_session_joined(false, true),
    }
    profile::set_session_player_side(side);
    profile::set_session_play_style(new_style);
    refresh_after_reload(state);
    state.selection_animation_timer = 0.0;
    crate::ui::runtime::clear_all();
}

fn handle_leaderboard_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    match sort_menu::handle_leaderboard_input(&mut state.leaderboard, ev) {
        sort_menu::LeaderboardInputOutcome::ChangedPane => {
            audio::play_sfx("assets/sounds/change.ogg");
        }
        sort_menu::LeaderboardInputOutcome::Closed => {
            audio::play_sfx("assets/sounds/start.ogg");
        }
        sort_menu::LeaderboardInputOutcome::None => {}
    }

    ScreenAction::None
}

fn handle_replay_overlay_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    match sort_menu::handle_replay_input(&mut state.replay_overlay, ev) {
        sort_menu::ReplayInputOutcome::ChangedSelection => {
            audio::play_sfx("assets/sounds/change.ogg");
            ScreenAction::None
        }
        sort_menu::ReplayInputOutcome::Closed => {
            audio::play_sfx("assets/sounds/start.ogg");
            ScreenAction::None
        }
        sort_menu::ReplayInputOutcome::StartGameplay(payload) => {
            state.pending_replay = Some(payload);
            state.out_prompt = OutPromptState::None;
            audio::play_sfx("assets/sounds/start.ogg");
            ScreenAction::Navigate(Screen::Gameplay)
        }
        sort_menu::ReplayInputOutcome::None => ScreenAction::None,
    }
}

fn handle_profile_switch_overlay_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    let Some(overlay) = &mut state.profile_switch_overlay else {
        return ScreenAction::None;
    };
    match profile_boxes::handle_input(overlay, ev) {
        ScreenAction::SelectProfiles { p1, p2 } => {
            state.profile_switch_overlay = None;
            profile::set_fast_profile_switch_from_select_music(true);
            ScreenAction::SelectProfiles { p1, p2 }
        }
        ScreenAction::Navigate(_) => {
            state.profile_switch_overlay = None;
            restore_sort_menu_after_profile_overlay(state);
            ScreenAction::None
        }
        _ => ScreenAction::None,
    }
}

fn handle_test_input_overlay_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    test_input::apply_virtual_input(&mut state.test_input_overlay, ev);
    let close_side = match ev.action {
        VirtualAction::p1_start | VirtualAction::p1_back => Some(profile::PlayerSide::P1),
        VirtualAction::p2_start | VirtualAction::p2_back => Some(profile::PlayerSide::P2),
        _ => None,
    };
    if ev.pressed && close_side.is_some_and(profile::is_session_side_joined) {
        hide_test_input_overlay(state);
        audio::play_sfx("assets/sounds/start.ogg");
    }
    ScreenAction::None
}

fn sort_menu_activate(state: &mut State) -> ScreenAction {
    let (page, selected_index) = match state.sort_menu {
        sort_menu::State::Visible {
            page,
            selected_index,
        } => (page, selected_index),
        sort_menu::State::Hidden => return ScreenAction::None,
    };
    let items = sort_menu_items(state, page);
    if items.is_empty() {
        hide_sort_menu(state);
        return ScreenAction::None;
    }
    let selected_index = selected_index.min(items.len() - 1);
    audio::play_sfx("assets/sounds/start.ogg");
    match items[selected_index].action {
        sort_menu::Action::OpenSorts => {
            show_sorts_submenu(state);
            ScreenAction::None
        }
        sort_menu::Action::BackToMain => {
            state.sort_menu = sort_menu::State::Visible {
                page: sort_menu::Page::Main,
                selected_index: 0,
            };
            state.sort_menu_prev_selected_index = 0;
            state.sort_menu_focus_anim_elapsed = 0.0;
            ScreenAction::None
        }
        sort_menu::Action::SortByGroup => {
            apply_wheel_sort(state, WheelSortMode::Group);
            hide_sort_menu(state);
            ScreenAction::None
        }
        sort_menu::Action::SortByTitle => {
            apply_wheel_sort(state, WheelSortMode::Title);
            hide_sort_menu(state);
            ScreenAction::None
        }
        sort_menu::Action::SortByArtist => {
            apply_wheel_sort(state, WheelSortMode::Artist);
            hide_sort_menu(state);
            ScreenAction::None
        }
        sort_menu::Action::SortByBpm => {
            apply_wheel_sort(state, WheelSortMode::Bpm);
            hide_sort_menu(state);
            ScreenAction::None
        }
        sort_menu::Action::SortByLength => {
            apply_wheel_sort(state, WheelSortMode::Length);
            hide_sort_menu(state);
            ScreenAction::None
        }
        sort_menu::Action::SortByMeter => {
            apply_wheel_sort(state, WheelSortMode::Meter);
            hide_sort_menu(state);
            ScreenAction::None
        }
        sort_menu::Action::SortByPopularity => {
            apply_wheel_sort(state, WheelSortMode::Popularity);
            hide_sort_menu(state);
            ScreenAction::None
        }
        sort_menu::Action::SortByRecent => {
            apply_wheel_sort(state, WheelSortMode::Recent);
            hide_sort_menu(state);
            ScreenAction::None
        }
        sort_menu::Action::SwitchToSingle => {
            switch_single_player_style(state, profile::PlayStyle::Single);
            ScreenAction::None
        }
        sort_menu::Action::SwitchToDouble => {
            switch_single_player_style(state, profile::PlayStyle::Double);
            ScreenAction::None
        }
        sort_menu::Action::TestInput => {
            hide_sort_menu(state);
            show_test_input_overlay(state);
            ScreenAction::None
        }
        sort_menu::Action::SongSearch => {
            hide_sort_menu(state);
            start_song_search_prompt(state);
            ScreenAction::None
        }
        sort_menu::Action::SwitchProfile => {
            show_profile_switch_overlay(state);
            ScreenAction::None
        }
        sort_menu::Action::ReloadSongsCourses => {
            hide_sort_menu(state);
            start_reload_songs_and_courses(state);
            ScreenAction::None
        }
        sort_menu::Action::PlayReplay => {
            hide_sort_menu(state);
            show_replay_overlay(state);
            ScreenAction::None
        }
        sort_menu::Action::ShowLeaderboard => {
            hide_sort_menu(state);
            show_leaderboard_overlay(state);
            ScreenAction::None
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
        VirtualAction::p1_start | VirtualAction::p2_start => return sort_menu_activate(state),
        VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            audio::play_sfx("assets/sounds/start.ogg");
            match state.sort_menu {
                sort_menu::State::Visible {
                    page: sort_menu::Page::Sorts,
                    ..
                } => {
                    state.sort_menu = sort_menu::State::Visible {
                        page: sort_menu::Page::Main,
                        selected_index: 0,
                    };
                    state.sort_menu_prev_selected_index = 0;
                    state.sort_menu_focus_anim_elapsed = 0.0;
                }
                _ => hide_sort_menu(state),
            }
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
        sort_menu::SongSearchState::TextEntry(entry) => match ev.action {
            VirtualAction::p1_start | VirtualAction::p2_start => {
                prompt_start = Some(entry.query.clone());
            }
            VirtualAction::p1_back
            | VirtualAction::p2_back
            | VirtualAction::p1_select
            | VirtualAction::p2_select => {
                prompt_close = true;
            }
            _ => {}
        },
        sort_menu::SongSearchState::Results(results) => {
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
                    let _ = sort_menu::song_search_move(results, -1);
                }
                VirtualAction::p1_down
                | VirtualAction::p1_menu_down
                | VirtualAction::p1_right
                | VirtualAction::p1_menu_right
                | VirtualAction::p2_down
                | VirtualAction::p2_menu_down
                | VirtualAction::p2_right
                | VirtualAction::p2_menu_right => {
                    let _ = sort_menu::song_search_move(results, 1);
                }
                VirtualAction::p1_start | VirtualAction::p2_start => {
                    let picked =
                        sort_menu::song_search_focused_candidate(results).map(|c| c.song.clone());
                    close_song_search(state);
                    if let Some(song) = picked {
                        focus_song_from_search(state, &song);
                        refresh_after_reload(state);
                    }
                }
                VirtualAction::p1_back
                | VirtualAction::p2_back
                | VirtualAction::p1_select
                | VirtualAction::p2_select => {
                    cancel_song_search(state);
                }
                _ => {}
            }
        }
        sort_menu::SongSearchState::Hidden => {}
    }

    if let Some(search_text) = prompt_start {
        start_song_search_results(state, search_text);
        return ScreenAction::None;
    }
    if prompt_close {
        cancel_song_search(state);
        return ScreenAction::None;
    }

    ScreenAction::None
}

pub fn handle_pad_dir(
    state: &mut State,
    dir: PadDir,
    pressed: bool,
    timestamp: Instant,
) -> ScreenAction {
    if pressed {
        match dir {
            PadDir::Right => {
                // Simply Love [ScreenSelectMusic]: CodeSortList4 = "Left-Right".
                state.menu_chord_mask |= MENU_CHORD_RIGHT;
                state.menu_chord_right_pressed_at = Some(timestamp);
                if try_open_sort_menu(state) {
                    return ScreenAction::None;
                }
                if state.menu_chord_mask & (MENU_CHORD_LEFT | MENU_CHORD_RIGHT)
                    == (MENU_CHORD_LEFT | MENU_CHORD_RIGHT)
                {
                    // ITGmania parity: if both directions are held, neutralize wheel movement.
                    state.nav_key_held_direction = None;
                    state.nav_key_held_since = None;
                    return ScreenAction::None;
                }
                if state.nav_key_held_direction == Some(NavDirection::Right) {
                    return ScreenAction::None;
                }
                music_wheel_change(state, 1);
                state.nav_key_held_direction = Some(NavDirection::Right);
                state.nav_key_held_since = Some(timestamp);
            }
            PadDir::Left => {
                state.menu_chord_mask |= MENU_CHORD_LEFT;
                state.menu_chord_left_pressed_at = Some(timestamp);
                if try_open_sort_menu(state) {
                    return ScreenAction::None;
                }
                if state.menu_chord_mask & (MENU_CHORD_LEFT | MENU_CHORD_RIGHT)
                    == (MENU_CHORD_LEFT | MENU_CHORD_RIGHT)
                {
                    // ITGmania parity: if both directions are held, neutralize wheel movement.
                    state.nav_key_held_direction = None;
                    state.nav_key_held_since = None;
                    return ScreenAction::None;
                }
                if state.nav_key_held_direction == Some(NavDirection::Left) {
                    return ScreenAction::None;
                }
                music_wheel_change(state, -1);
                state.nav_key_held_direction = Some(NavDirection::Left);
                state.nav_key_held_since = Some(timestamp);
            }
            PadDir::Up | PadDir::Down => {
                if let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) {
                    let is_up = matches!(dir, PadDir::Up);
                    let now = timestamp;

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
                    if is_up {
                        state.p1_chord_up_pressed_at = Some(timestamp);
                    } else {
                        state.p1_chord_down_pressed_at = Some(timestamp);
                    }

                    // Combo check
                    if state.chord_mask_p1 & (CHORD_UP | CHORD_DOWN) == (CHORD_UP | CHORD_DOWN)
                        && chord_times_are_simultaneous(
                            state.p1_chord_up_pressed_at,
                            state.p1_chord_down_pressed_at,
                        )
                    {
                        if let Some(pack) = state.expanded_pack_name.take() {
                            info!("Up+Down combo: Collapsing pack '{}'.", pack);
                            rebuild_displayed_entries(state);
                            if let Some(new_sel) = state.entries.iter().position(|e| matches!(e, MusicWheelEntry::PackHeader { name, .. } if name == &pack)) {
                                state.selected_index = new_sel;
                                state.prev_selected_index = new_sel;
                                state.time_since_selection_change = 0.0;
                                // Clear delayed chart-driven panels immediately on folder close.
                                state.displayed_chart_p1 = None;
                                state.displayed_chart_p2 = None;
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
                state.p1_chord_up_pressed_at = None;
            }
            PadDir::Down => {
                state.chord_mask_p1 &= !CHORD_DOWN;
                state.p1_chord_down_pressed_at = None;
            }
            PadDir::Left => {
                state.menu_chord_mask &= !MENU_CHORD_LEFT;
                state.menu_chord_left_pressed_at = None;
                if state.nav_key_held_direction == Some(NavDirection::Left) {
                    let now = timestamp;
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
                } else if state.menu_chord_mask & MENU_CHORD_RIGHT != 0 {
                    // After releasing one side of a held-opposite pair, resume remaining hold.
                    state.nav_key_held_direction = Some(NavDirection::Right);
                    state.nav_key_held_since = Some(timestamp);
                }
            }
            PadDir::Right => {
                state.menu_chord_mask &= !MENU_CHORD_RIGHT;
                state.menu_chord_right_pressed_at = None;
                if state.nav_key_held_direction == Some(NavDirection::Right) {
                    let now = timestamp;
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
                } else if state.menu_chord_mask & MENU_CHORD_LEFT != 0 {
                    // After releasing one side of a held-opposite pair, resume remaining hold.
                    state.nav_key_held_direction = Some(NavDirection::Left);
                    state.nav_key_held_since = Some(timestamp);
                }
            }
        }
    }
    ScreenAction::None
}

fn handle_pad_dir_p2(
    state: &mut State,
    dir: PadDir,
    pressed: bool,
    timestamp: Instant,
) -> ScreenAction {
    if !(matches!(dir, PadDir::Up | PadDir::Down)) {
        return ScreenAction::None;
    }
    if pressed {
        if let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) {
            let is_up = matches!(dir, PadDir::Up);
            let now = timestamp;

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
            if is_up {
                state.p2_chord_up_pressed_at = Some(timestamp);
            } else {
                state.p2_chord_down_pressed_at = Some(timestamp);
            }

            // Combo check
            if state.chord_mask_p2 & (CHORD_UP | CHORD_DOWN) == (CHORD_UP | CHORD_DOWN)
                && chord_times_are_simultaneous(
                    state.p2_chord_up_pressed_at,
                    state.p2_chord_down_pressed_at,
                )
            {
                if let Some(pack) = state.expanded_pack_name.take() {
                    info!("Up+Down combo: Collapsing pack '{}'.", pack);
                    rebuild_displayed_entries(state);
                    if let Some(new_sel) = state.entries.iter().position(
                        |e| matches!(e, MusicWheelEntry::PackHeader { name, .. } if name == &pack),
                    ) {
                        state.selected_index = new_sel;
                        state.prev_selected_index = new_sel;
                        state.time_since_selection_change = 0.0;
                        // Clear delayed chart-driven panels immediately on folder close.
                        state.displayed_chart_p1 = None;
                        state.displayed_chart_p2 = None;
                    }
                }
            }
        }
    } else {
        match dir {
            PadDir::Up => {
                state.chord_mask_p2 &= !CHORD_UP;
                state.p2_chord_up_pressed_at = None;
            }
            PadDir::Down => {
                state.chord_mask_p2 &= !CHORD_DOWN;
                state.p2_chord_down_pressed_at = None;
            }
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

    if !matches!(state.replay_overlay, sort_menu::ReplayOverlayState::Hidden) {
        if key.state == ElementState::Pressed
            && matches!(
                key.physical_key,
                winit::keyboard::PhysicalKey::Code(KeyCode::Escape)
            )
        {
            state.replay_overlay = sort_menu::ReplayOverlayState::Hidden;
            state.song_search_ignore_next_back_select = true;
            return ScreenAction::None;
        }
        return ScreenAction::None;
    }
    if state.test_input_overlay_visible {
        return ScreenAction::None;
    }
    if state.profile_switch_overlay.is_some() {
        return ScreenAction::None;
    }

    if key.state == ElementState::Pressed {
        if matches!(state.song_search, sort_menu::SongSearchState::Results(_))
            && let winit::keyboard::PhysicalKey::Code(KeyCode::Escape) = key.physical_key
        {
            cancel_song_search(state);
            return ScreenAction::None;
        }
        let mut prompt_start: Option<String> = None;
        let mut prompt_close = false;
        if let sort_menu::SongSearchState::TextEntry(entry) = &mut state.song_search {
            if let winit::keyboard::PhysicalKey::Code(code) = key.physical_key {
                match code {
                    KeyCode::Backspace => {
                        sort_menu::song_search_backspace(entry);
                        return ScreenAction::None;
                    }
                    KeyCode::Escape => {
                        prompt_close = true;
                    }
                    KeyCode::Enter | KeyCode::NumpadEnter => {
                        prompt_start = Some(entry.query.clone());
                    }
                    _ => {}
                }
            }

            if !prompt_close
                && prompt_start.is_none()
                && let Some(text) = key.text.as_ref()
            {
                sort_menu::song_search_add_text(entry, text);
            }

            if let Some(search_text) = prompt_start {
                start_song_search_results(state, search_text);
                return ScreenAction::None;
            }
            if prompt_close {
                cancel_song_search(state);
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

    if matches!(state.song_search, sort_menu::SongSearchState::Hidden)
        && state.song_search_ignore_next_back_select
    {
        if matches!(
            ev.action,
            VirtualAction::p1_back
                | VirtualAction::p2_back
                | VirtualAction::p1_select
                | VirtualAction::p2_select
        ) {
            state.song_search_ignore_next_back_select = false;
            if ev.pressed {
                return ScreenAction::None;
            }
        } else if ev.pressed {
            state.song_search_ignore_next_back_select = false;
        }
    }

    if !matches!(state.song_search, sort_menu::SongSearchState::Hidden) {
        return handle_song_search_input(state, ev);
    }

    if !matches!(state.replay_overlay, sort_menu::ReplayOverlayState::Hidden) {
        return handle_replay_overlay_input(state, ev);
    }
    if state.test_input_overlay_visible {
        return handle_test_input_overlay_input(state, ev);
    }
    if state.profile_switch_overlay.is_some() {
        return handle_profile_switch_overlay_input(state, ev);
    }

    if state.exit_prompt != ExitPromptState::None {
        return handle_exit_prompt_input(state, ev);
    }

    if !matches!(
        state.leaderboard,
        sort_menu::LeaderboardOverlayState::Hidden
    ) {
        return handle_leaderboard_input(state, ev);
    }

    if state.sort_menu != sort_menu::State::Hidden {
        return handle_sort_menu_input(state, ev);
    }

    let play_style = crate::game::profile::get_session_play_style();
    if play_style == crate::game::profile::PlayStyle::Versus {
        return match ev.action {
            VirtualAction::p1_left | VirtualAction::p1_menu_left => {
                handle_pad_dir(state, PadDir::Left, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_right | VirtualAction::p1_menu_right => {
                handle_pad_dir(state, PadDir::Right, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_up | VirtualAction::p1_menu_up => {
                handle_pad_dir(state, PadDir::Up, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_down | VirtualAction::p1_menu_down => {
                handle_pad_dir(state, PadDir::Down, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_start if ev.pressed => handle_confirm(state),
            VirtualAction::p1_back if ev.pressed => {
                begin_exit_prompt(state);
                ScreenAction::None
            }

            VirtualAction::p2_left | VirtualAction::p2_menu_left => {
                handle_pad_dir(state, PadDir::Left, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_right | VirtualAction::p2_menu_right => {
                handle_pad_dir(state, PadDir::Right, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_up | VirtualAction::p2_menu_up => {
                handle_pad_dir_p2(state, PadDir::Up, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_down | VirtualAction::p2_menu_down => {
                handle_pad_dir_p2(state, PadDir::Down, ev.pressed, ev.timestamp)
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
                handle_pad_dir(state, PadDir::Left, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_right | VirtualAction::p2_menu_right => {
                handle_pad_dir(state, PadDir::Right, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_up | VirtualAction::p2_menu_up => {
                handle_pad_dir(state, PadDir::Up, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_down | VirtualAction::p2_menu_down => {
                handle_pad_dir(state, PadDir::Down, ev.pressed, ev.timestamp)
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
                handle_pad_dir(state, PadDir::Left, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_right | VirtualAction::p1_menu_right => {
                handle_pad_dir(state, PadDir::Right, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_up | VirtualAction::p1_menu_up => {
                handle_pad_dir(state, PadDir::Up, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_down | VirtualAction::p1_menu_down => {
                handle_pad_dir(state, PadDir::Down, ev.pressed, ev.timestamp)
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

    if sort_menu::update_song_search(&mut state.song_search, dt) {
        return ScreenAction::None;
    }
    if sort_menu::update_replay_overlay(&mut state.replay_overlay, dt) {
        return ScreenAction::None;
    }
    if let Some(overlay) = state.profile_switch_overlay.as_mut() {
        profile_boxes::update(overlay, dt);
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

    sort_menu::update_leaderboard_overlay(&mut state.leaderboard, dt);

    state.time_since_selection_change += dt;
    if dt > 0.0 {
        state.selection_animation_timer += dt;
        if state.sort_menu != sort_menu::State::Hidden
            && state.sort_menu_focus_anim_elapsed < sort_menu::FOCUS_TWEEN_SECONDS
        {
            state.sort_menu_focus_anim_elapsed =
                (state.sort_menu_focus_anim_elapsed + dt).min(sort_menu::FOCUS_TWEEN_SECONDS);
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

    let selected_song_for_cache = match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(song)) => Some(song.clone()),
        _ => None,
    };
    if let Some(song) = selected_song_for_cache {
        let play_style = profile::get_session_play_style();
        ensure_chart_cache_for_song(
            state,
            &song,
            play_style.chart_type(),
            play_style == profile::PlayStyle::Versus,
        );
    }

    if state.sort_menu != sort_menu::State::Hidden
        || !matches!(
            state.leaderboard,
            sort_menu::LeaderboardOverlayState::Hidden
        )
        || !matches!(state.replay_overlay, sort_menu::ReplayOverlayState::Hidden)
        || state.profile_switch_overlay.is_some()
        || state.test_input_overlay_visible
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
        state.banner_high_quality_requested = false;
        return ScreenAction::RequestBanner(new_banner);
    }
    if new_banner.is_some()
        && !state.banner_high_quality_requested
        && state.nav_key_held_direction.is_none()
        && state.wheel_offset_from_selection.abs() < 0.0001
    {
        state.banner_high_quality_requested = true;
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
    } else if state.currently_playing_preview_path.is_some() {
        state.currently_playing_preview_path = None;
        state.currently_playing_preview_start_sec = None;
        state.currently_playing_preview_length_sec = None;
        audio::stop_music();
    }

    if allow_gs_fetch_for_selection(state) {
        let play_style = profile::get_session_play_style();
        let target_chart_type = play_style.chart_type();

        if let Some(song) = selected_song.as_ref() {
            let is_versus = play_style == crate::game::profile::PlayStyle::Versus;
            ensure_chart_cache_for_song(state, song, target_chart_type, is_versus);

            if !displayed_chart_matches(state.displayed_chart_p1.as_ref(), song, state.cached_chart_ix_p1)
            {
                state.displayed_chart_p1 = state.cached_chart_ix_p1.map(|chart_ix| DisplayedChart {
                    song: song.clone(),
                    chart_ix,
                });
            }
            let desired_hash_p1 = state
                .cached_chart_ix_p1
                .map(|ix| song.charts[ix].short_hash.as_str());

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
                if !displayed_chart_matches(
                    state.displayed_chart_p2.as_ref(),
                    song,
                    state.cached_chart_ix_p2,
                ) {
                    state.displayed_chart_p2 = state.cached_chart_ix_p2.map(|chart_ix| DisplayedChart {
                        song: song.clone(),
                        chart_ix,
                    });
                }
                let desired_hash_p2 = state
                    .cached_chart_ix_p2
                    .map(|ix| song.charts[ix].short_hash.as_str());

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
    state.banner_high_quality_requested = false;
}

pub fn reset_preview_after_gameplay(state: &mut State) {
    let was_recent_sort = state.sort_mode == WheelSortMode::Recent;
    let was_popularity_sort = state.sort_mode == WheelSortMode::Popularity;
    refresh_recent_cache(state);
    refresh_popularity_cache(state);
    if was_recent_sort {
        state.sort_mode = WheelSortMode::Group;
        apply_wheel_sort(state, WheelSortMode::Recent);
    } else if was_popularity_sort {
        state.sort_mode = WheelSortMode::Group;
        apply_wheel_sort(state, WheelSortMode::Popularity);
    }
    state.currently_playing_preview_path = None;
    state.currently_playing_preview_start_sec = None;
    state.currently_playing_preview_length_sec = None;
    trigger_immediate_refresh(state);
}

pub fn prime_displayed_chart_data(state: &mut State) {
    let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) else {
        state.displayed_chart_p1 = None;
        state.displayed_chart_p2 = None;
        return;
    };
    let song = song.clone();
    let play_style = profile::get_session_play_style();
    let target_chart_type = play_style.chart_type();
    let is_versus = play_style == crate::game::profile::PlayStyle::Versus;
    ensure_chart_cache_for_song(state, &song, target_chart_type, is_versus);

    state.displayed_chart_p1 = state.cached_chart_ix_p1.map(|chart_ix| DisplayedChart {
        song: song.clone(),
        chart_ix,
    });
    state.displayed_chart_p2 = state.cached_chart_ix_p2.map(|chart_ix| DisplayedChart {
        song,
        chart_ix,
    });
}

pub fn take_pending_replay(state: &mut State) -> Option<sort_menu::ReplayStartPayload> {
    state.pending_replay.take()
}

#[inline(always)]
pub fn allows_late_join(state: &State) -> bool {
    state.reload_ui.is_none()
        && state.out_prompt == OutPromptState::None
        && state.exit_prompt == ExitPromptState::None
        && state.sort_menu == sort_menu::State::Hidden
        && matches!(state.song_search, sort_menu::SongSearchState::Hidden)
        && matches!(state.replay_overlay, sort_menu::ReplayOverlayState::Hidden)
        && matches!(
            state.leaderboard,
            sort_menu::LeaderboardOverlayState::Hidden
        )
        && state.profile_switch_overlay.is_none()
        && !state.test_input_overlay_visible
}

// Fast non-allocating formatters where possible
fn format_session_time(seconds: f32) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "00:00".to_string();
    }
    let s = seconds as u64;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if s < 3600 {
        format!("{m:02}:{sec:02}")
    } else if s < 36000 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{h:02}:{m:02}:{sec:02}")
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

#[inline(always)]
fn allow_gs_fetch_for_selection(state: &State) -> bool {
    state.nav_key_held_direction.is_none()
        && state.wheel_offset_from_selection.abs() < 0.0001
        && state.time_since_selection_change >= PREVIEW_DELAY_SECONDS
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
    let selected_entry = state.entries.get(state.selected_index);
    let selected_song = match selected_entry {
        Some(MusicWheelEntry::Song(song)) => Some(song.as_ref()),
        _ => None,
    };
    let immediate_chart_p1 = selected_song
        .and_then(|song| chart_for_steps_index(song, target_chart_type, state.selected_steps_index));
    let immediate_chart_p2 = if is_versus {
        selected_song.and_then(|song| {
            chart_for_steps_index(song, target_chart_type, state.p2_selected_steps_index)
        })
    } else {
        None
    };
    let allow_gs_fetch = allow_gs_fetch_for_selection(state);

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));
    actors.push(sl_select_music_bg_flash());
    actors.extend(select_shared::build_screen_bars("SELECT MUSIC"));

    let p1_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P1);
    let p2_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P2);

    let mode_side = if is_p2_single {
        profile::PlayerSide::P2
    } else {
        profile::PlayerSide::P1
    };
    let mode_chart_hash = if allow_gs_fetch {
        let mode_chart = if mode_side == profile::PlayerSide::P2 && is_versus {
            immediate_chart_p2
        } else {
            immediate_chart_p1
        };
        mode_chart.map(|c| c.short_hash.as_str())
    } else {
        None
    };
    let score_mode_text = gs_scorebox::select_music_mode_text(mode_side, mode_chart_hash);

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
    if let Some(chart) = immediate_chart_p1 {
        sel_col_p1 = color::difficulty_rgba(&chart.difficulty, state.active_color_index);
    }
    if let Some(chart) = immediate_chart_p2 {
        sel_col_p2 = color::difficulty_rgba(&chart.difficulty, state.active_color_index);
    }

    // Timer (zmod parity: optional gameplay timer to the right of session timer).
    actors.push(select_shared::build_session_timer(format_session_time(
        state.session_elapsed,
    )));
    if crate::config::get().show_select_music_gameplay_timer {
        actors.push(select_shared::build_gameplay_timer(format_session_time(
            state.gameplay_elapsed,
        )));
    }

    // Pads
    {
        actors.push(select_shared::build_mode_pad_text(score_mode_text.as_str()));
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
    let entry_opt = selected_entry;
    let (artist, bpm, len_text) = match entry_opt {
        Some(MusicWheelEntry::Song(s)) => (
            s.artist.clone(),
            s.formatted_display_bpm(),
            format_chart_length(((s.total_length_seconds.max(0) as f32) / music_rate) as i32),
        ),
        Some(MusicWheelEntry::PackHeader { original_index, .. }) => {
            let total_sec = state
                .pack_total_seconds_by_index
                .get(*original_index)
                .copied()
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

    let disp_chart_p1 = state
        .displayed_chart_p1
        .as_ref()
        .and_then(|d| d.song.charts.get(d.chart_ix));
    let disp_chart_p2 = state
        .displayed_chart_p2
        .as_ref()
        .and_then(|d| d.song.charts.get(d.chart_ix));

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
    let base_y = (screen_center_y() - 9.0) - 0.5 * (screen_height() / 28.0);
    let mut push_step_artist = |y_cen: f32, x0: f32, sel_col: [f32; 4], step_artist: &str| {
        actors.extend(step_artist_bar::build(
            step_artist_bar::StepArtistBarParams {
                x0,
                center_y: y_cen,
                accent_color: sel_col,
                z_base: 120,
                label_text: "STEPS",
                label_max_width: 40.0,
                artist_text: step_artist,
                artist_x_offset: 75.0,
                artist_max_width: 124.0,
                artist_color: [0.0, 0.0, 0.0, 1.0],
            },
        ));
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
    let cfg = config::get();
    let breakdown_style = cfg.select_music_breakdown_style;
    let pattern_info_mode = cfg.select_music_pattern_info_mode;
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
                        let (detailed_breakdown, partial_breakdown, simple_breakdown) =
                            match breakdown_style {
                                BreakdownStyle::Sl => (
                                    &c.detailed_breakdown,
                                    &c.partial_breakdown,
                                    &c.simple_breakdown,
                                ),
                                BreakdownStyle::Sn => (
                                    &c.sn_detailed_breakdown,
                                    &c.sn_partial_breakdown,
                                    &c.sn_simple_breakdown,
                                ),
                            };
                        let fits = |text: &str| {
                            (font::measure_line_width_logical(miso_font, text, all_fonts) as f32)
                                <= max_allowed_logical_width
                        };

                        if fits(detailed_breakdown) {
                            Some(detailed_breakdown.clone())
                        } else if fits(partial_breakdown) {
                            Some(partial_breakdown.clone())
                        } else if fits(simple_breakdown) {
                            Some(simple_breakdown.clone())
                        } else {
                            Some(format!("{} Total", c.total_streams))
                        }
                    })
                })
                .flatten()
                .unwrap_or_else(|| match breakdown_style {
                    BreakdownStyle::Sl => c.simple_breakdown.clone(),
                    BreakdownStyle::Sn => c.sn_simple_breakdown.clone(),
                });

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
    let pane_layout = select_pane::layout();
    let pane_top = pane_layout.pane_top;
    let tz = pane_layout.text_zoom;
    let cols = pane_layout.cols;
    let rows = pane_layout.rows;

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
        // Scores
        let placeholder = ("----".to_string(), "??.??%".to_string());
        let fallback_player = if let Some(c) = chart
            && let Some(sc) = scores::get_cached_local_score_for_side(&c.short_hash, side)
            && (sc.grade != scores::Grade::Failed || sc.score_percent > 0.0)
        {
            (
                player_initials.to_string(),
                format!("{:.2}%", sc.score_percent * 100.0),
            )
        } else {
            placeholder.clone()
        };

        let fallback_machine = if let Some(c) = chart
            && let Some((initials, sc)) = scores::get_machine_record_local(&c.short_hash)
            && (sc.grade != scores::Grade::Failed || sc.score_percent > 0.0)
        {
            (initials, format!("{:.2}%", sc.score_percent * 100.0))
        } else {
            placeholder
        };

        let chart_hash = if allow_gs_fetch {
            chart.map(|c| c.short_hash.as_str())
        } else {
            None
        };
        let gs_view = gs_scorebox::select_music_scorebox_view(
            side,
            chart_hash,
            fallback_machine,
            fallback_player,
        );
        let mut out = select_pane::build_base(select_pane::StatsPaneParams {
            pane_cx,
            accent_color: sel_col,
            values: select_pane::StatsValues {
                steps,
                mines,
                jumps,
                hands,
                holds,
                rolls,
            },
            meter: (!gs_view.show_rivals).then_some(meter),
        });

        // Simply Love PaneDisplay order: Machine/World first, then Player.
        let lines = [
            (
                gs_view.machine_name.as_str(),
                gs_view.machine_score.as_str(),
            ),
            (gs_view.player_name.as_str(), gs_view.player_score.as_str()),
        ];
        for i in 0..2 {
            let (name, pct) = lines[i];
            out.push(act!(text: font("miso"): settext(name): align(0.5, 0.5): xy(pane_cx + cols[2] - 50.0 * tz, pane_top + rows[i]): maxwidth(30.0): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
            out.push(act!(text: font("miso"): settext(pct): align(1.0, 0.5): xy(pane_cx + cols[2] + 25.0 * tz, pane_top + rows[i]): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
        }
        if let Some(status) = gs_view.loading_text {
            out.push(act!(text: font("miso"): settext(status): align(0.5, 0.5): xy(pane_cx + cols[2] - 15.0, pane_top + rows[2]): maxwidth(90.0): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0): horizalign(center)));
        }
        if gs_view.show_rivals {
            for i in 0..3 {
                let (name, pct) = (&gs_view.rivals[i].0, &gs_view.rivals[i].1);
                out.push(act!(text: font("miso"): settext(name): align(0.5, 0.5): xy(pane_cx + cols[2] + 50.0 * tz, pane_top + rows[i]): maxwidth(30.0): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
                out.push(act!(text: font("miso"): settext(pct): align(1.0, 0.5): xy(pane_cx + cols[2] + 125.0 * tz, pane_top + rows[i]): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
            }
        }

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
        let pat_cx = chart_info_cx;
        let pat_cy = screen_center_y() + if is_p2_single { 23.0 } else { 111.0 };
        actors.push(act!(quad: align(0.5, 0.5): xy(pat_cx, pat_cy): setsize(panel_w, 64.0): z(120): diffuse(UI_BOX_BG_COLOR[0], UI_BOX_BG_COLOR[1], UI_BOX_BG_COLOR[2], UI_BOX_BG_COLOR[3])));
        if show_stamina_panel(pattern_info_mode, disp_chart_p1) {
            let pct = |value: f64| {
                if value.is_finite() {
                    value
                } else {
                    0.0
                }
            };
            let (
                boxes,
                anchors,
                staircases,
                sweeps,
                towers,
                triangles,
                doritos,
                hip_breakers,
                copters,
                spirals,
                mono_value,
                candles_value,
                total_stream,
            ) =
                if let Some(c) = disp_chart_p1 {
                    (
                        c.stamina_counts.boxes.to_string(),
                        c.stamina_counts.anchors.to_string(),
                        c.stamina_counts.staircases.to_string(),
                        c.stamina_counts.sweeps.to_string(),
                        c.stamina_counts.towers.to_string(),
                        c.stamina_counts.triangles.to_string(),
                        c.stamina_counts.doritos.to_string(),
                        c.stamina_counts.hip_breakers.to_string(),
                        c.stamina_counts.copters.to_string(),
                        c.stamina_counts.spirals.to_string(),
                        format!("{:.1}% Mono", pct(c.stamina_counts.mono_percent)),
                        format!("{:.1}% Candles", pct(c.stamina_counts.candle_percent)),
                        format!("{} ({:.1}%)", c.total_streams, chart_stream_percent(c)),
                    )
                } else {
                    (
                        "0".to_string(),
                        "0".to_string(),
                        "0".to_string(),
                        "0".to_string(),
                        "0".to_string(),
                        "0".to_string(),
                        "0".to_string(),
                        "0".to_string(),
                        "0".to_string(),
                        "0".to_string(),
                        "0.0% Mono".to_string(),
                        "0.0% Candles".to_string(),
                        "0 (0.0%)".to_string(),
                    )
                };

            let panel_left = pat_cx - panel_w * 0.5;
            let col_w1 = panel_w / 3.0;
            let col_w2 = panel_w / 3.0;
            let col_w3 = panel_w / 3.0;
            let col1_left = panel_left + 4.0;
            let col2_left = col1_left + col_w1;
            let col3_left = col2_left + col_w2;

            let stamina_row_step = 14.5;
            let stamina_zoom = 0.85;
            let stamina_base_y = pat_cy - 21.75;

            let push_pattern_line =
                |actors: &mut Vec<Actor>,
                 col_left: f32,
                 col_w: f32,
                 num_right_x: f32,
                 row: usize,
                 num: &str,
                 label: &str| {
                    let y = stamina_base_y + row as f32 * stamina_row_step;
                    let label_x = num_right_x + 3.0;
                    let num_w = (num_right_x - col_left).max(8.0);
                    let label_w = (col_left + col_w - label_x - 2.0).max(8.0);
                    actors.push(act!(text: font("miso"): settext(num): align(1.0, 0.5): horizalign(right): xy(num_right_x, y): maxwidth(num_w): zoom(stamina_zoom): z(121): diffuse(1.0, 1.0, 1.0, 1.0)));
                    actors.push(act!(text: font("miso"): settext(label): align(0.0, 0.5): horizalign(left): xy(label_x, y): maxwidth(label_w): zoom(stamina_zoom): z(121): diffuse(1.0, 1.0, 1.0, 1.0)));
                };

            let num_anchor_frac = 0.31;
            let col1_num_x = col1_left + col_w1 * num_anchor_frac;
            let col2_num_x = col2_left + col_w2 * num_anchor_frac;
            let col3_num_x = col3_left + col_w3 * num_anchor_frac;

            push_pattern_line(
                &mut actors,
                col1_left,
                col_w1,
                col1_num_x,
                0,
                boxes.as_str(),
                "Boxes",
            );
            push_pattern_line(
                &mut actors,
                col1_left,
                col_w1,
                col1_num_x,
                1,
                anchors.as_str(),
                "Anchors",
            );
            push_pattern_line(
                &mut actors,
                col1_left,
                col_w1,
                col1_num_x,
                2,
                staircases.as_str(),
                "Staircases",
            );
            push_pattern_line(
                &mut actors,
                col1_left,
                col_w1,
                col1_num_x,
                3,
                sweeps.as_str(),
                "Sweeps",
            );

            push_pattern_line(
                &mut actors,
                col2_left,
                col_w2,
                col2_num_x,
                0,
                triangles.as_str(),
                "Triangles",
            );
            push_pattern_line(
                &mut actors,
                col2_left,
                col_w2,
                col2_num_x,
                1,
                hip_breakers.as_str(),
                "Hip Breakers",
            );
            push_pattern_line(
                &mut actors,
                col2_left,
                col_w2,
                col2_num_x,
                2,
                doritos.as_str(),
                "Doritos",
            );
            push_pattern_line(
                &mut actors,
                col2_left,
                col_w2,
                col2_num_x,
                3,
                towers.as_str(),
                "Towers",
            );

            push_pattern_line(
                &mut actors,
                col3_left,
                col_w3,
                col3_num_x,
                0,
                spirals.as_str(),
                "Spirals",
            );
            push_pattern_line(
                &mut actors,
                col3_left,
                col_w3,
                col3_num_x,
                1,
                copters.as_str(),
                "Copters",
            );

            let col3_label_x = col3_num_x + 3.0;
            let col3_num_w = (col3_num_x - col3_left).max(8.0);
            let col3_label_w = (col3_left + col_w3 - col3_label_x - 2.0).max(8.0);
            let relaxed_num_w = col3_num_w * 1.65;

            let mono_y = stamina_base_y + 2.0 * stamina_row_step;
            actors.push(act!(text: font("miso"): settext(mono_value.as_str()): align(1.0, 0.5): horizalign(right): xy(col3_num_x, mono_y): maxwidth(relaxed_num_w): zoom(stamina_zoom): z(121): diffuse(1.0, 1.0, 1.0, 1.0)));
            actors.push(act!(text: font("miso"): settext(candles_value.as_str()): align(0.0, 0.5): horizalign(left): xy(col3_label_x, mono_y): maxwidth(col3_label_w): zoom(stamina_zoom): z(121): diffuse(1.0, 1.0, 1.0, 1.0)));

            let stream_y = stamina_base_y + 3.0 * stamina_row_step;
            actors.push(act!(text: font("miso"): settext(total_stream.as_str()): align(1.0, 0.5): horizalign(right): xy(col3_num_x, stream_y): maxwidth(relaxed_num_w): zoom(stamina_zoom): z(121): diffuse(1.0, 1.0, 1.0, 1.0)));
            actors.push(act!(text: font("miso"): settext("Total Stream"): align(0.0, 0.5): horizalign(left): xy(col3_label_x, stream_y): maxwidth(col3_label_w): zoom(stamina_zoom): z(121): diffuse(1.0, 1.0, 1.0, 1.0)));
        } else {
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
                            chart_stream_percent(c)
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

            let p_v_x = pat_cx - panel_w * 0.5 + 39.0;
            let p_l_x = pat_cx - panel_w * 0.5 + 48.0;
            let p_base_y = pat_cy - 18.0;
            let items = [
                (cross, "Crossovers", 0_u8, 0_u8, None),
                (foot, "Footswitches", 1_u8, 0_u8, None),
                (side, "Sideswitches", 0_u8, 1_u8, None),
                (jack, "Jacks", 1_u8, 1_u8, None),
                (brack, "Brackets", 0_u8, 2_u8, None),
                (stream, "Total Stream", 1_u8, 2_u8, Some(100.0)),
            ];

            for (val, lbl, c, r, mw) in items {
                let y = p_base_y + r as f32 * 19.0;
                let vx = p_v_x + c as f32 * 148.0;
                let lx = p_l_x + c as f32 * 148.0;
                match mw {
                    Some(w) => actors.push(act!(text: font("miso"): settext(val): align(1.0, 0.5): horizalign(right): xy(vx, y): maxwidth(w): zoom(0.78): z(121): diffuse(1.0, 1.0, 1.0, 1.0))),
                    None => actors.push(act!(text: font("miso"): settext(val): align(1.0, 0.5): horizalign(right): xy(vx, y): zoom(0.78): z(121): diffuse(1.0, 1.0, 1.0, 1.0))),
                }
                actors.push(act!(text: font("miso"): settext(lbl): align(0.0, 0.5): horizalign(left): xy(lx, y): zoom(0.78): z(121): diffuse(1.0, 1.0, 1.0, 1.0)));
            }
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
            let cached_edit_indices = state.cached_edits.as_ref().and_then(|c| {
                if Arc::ptr_eq(&c.song, song) && c.chart_type == target_chart_type {
                    Some(c.indices.as_slice())
                } else {
                    None
                }
            });
            if let Some(indices) = cached_edit_indices {
                v.reserve(indices.len());
                for &chart_ix in indices {
                    v.push(song.charts.get(chart_ix));
                }
            } else {
                v.extend(
                    edit_charts_sorted(song, target_chart_type)
                        .into_iter()
                        .map(Some),
                );
            }
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
    let selection_animation_beat = sl_selection_anim_beat(entry_opt, state);
    actors.extend(music_wheel::build(music_wheel::MusicWheelParams {
        entries: &state.entries,
        selected_index: state.selected_index,
        position_offset_from_selection: state.wheel_offset_from_selection,
        selection_animation_timer: state.selection_animation_timer,
        selection_animation_beat,
        pack_song_counts: &state.pack_song_counts, // O(1) Lookup
        color_pack_headers: state.sort_mode == WheelSortMode::Group,
        preferred_difficulty_index: state.preferred_difficulty_index,
        selected_steps_index: state.selected_steps_index,
        song_box_color: None,
        song_text_color: None,
        song_has_edit_ptrs: Some(&state.song_has_edit_ptrs),
    }));
    actors.extend(sl_select_music_wheel_cascade_mask());

    // GrooveStats scorebox placement.
    // Keep P1 single where it already is, but move P2 single/versus up so they sit above PaneDisplay.
    if is_wide() {
        let scorebox_zoom = widescale(0.95, 1.0);
        let scorebox_side_inset = 320.0;
        let scorebox_center_p1 = screen_width() * 0.25 - 5.0 + scorebox_side_inset;
        let scorebox_center_p2 = screen_width() * 0.75 + 5.0 - scorebox_side_inset;
        let footer_top = screen_height() - 32.0;
        let scorebox_center_y_p1_single = footer_top - 44.0;
        let tech_box_bottom_y = screen_center_y() + 111.0 + 32.0;
        let pane_to_tech_gap = pane_layout.pane_top - tech_box_bottom_y;
        let scorebox_center_y_above_pane =
            pane_layout.pane_top - (40.0 * scorebox_zoom) - pane_to_tech_gap;
        let mut push_scorebox =
            |side: profile::PlayerSide, steps_idx: usize, center_x: f32, center_y: f32| {
                let chart_hash = if allow_gs_fetch {
                    match selected_entry {
                        Some(MusicWheelEntry::Song(song)) => {
                            chart_for_steps_index(song, target_chart_type, steps_idx)
                                .map(|c| c.short_hash.as_str())
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                actors.extend(gs_scorebox::gameplay_scorebox_actors(
                    side,
                    chart_hash,
                    center_x,
                    center_y,
                    scorebox_zoom,
                    state.selection_animation_timer,
                ));
            };

        if is_versus {
            push_scorebox(
                profile::PlayerSide::P1,
                state.selected_steps_index,
                scorebox_center_p1,
                scorebox_center_y_above_pane,
            );
            push_scorebox(
                profile::PlayerSide::P2,
                state.p2_selected_steps_index,
                scorebox_center_p2,
                scorebox_center_y_above_pane,
            );
        } else if is_p2_single {
            push_scorebox(
                profile::PlayerSide::P2,
                state.p2_selected_steps_index,
                scorebox_center_p1,
                scorebox_center_y_above_pane,
            );
        } else {
            push_scorebox(
                profile::PlayerSide::P1,
                state.selected_steps_index,
                scorebox_center_p1,
                scorebox_center_y_p1_single,
            );
        }
    }

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

    if let Some(song_search_overlay) =
        sort_menu::build_song_search_overlay(&state.song_search, state.active_color_index)
    {
        actors.extend(song_search_overlay);
        return actors;
    }
    if let Some(overlay) = state.profile_switch_overlay.as_ref() {
        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 0.8):
            z(1450)
        ));
        actors.extend(profile_boxes::get_box_actors_with_z(
            overlay,
            asset_manager,
            1.0,
            1451,
        ));
        return actors;
    }
    if let Some(replay_overlay) =
        sort_menu::build_replay_overlay(&state.replay_overlay, state.active_color_index)
    {
        actors.extend(replay_overlay);
        return actors;
    }
    if state.test_input_overlay_visible {
        let play_style = profile::get_session_play_style();
        let (mut show_p1, mut show_p2, pad_spacing) = match play_style {
            profile::PlayStyle::Double => (true, true, 105.0),
            profile::PlayStyle::Single | profile::PlayStyle::Versus => (
                profile::is_session_side_joined(profile::PlayerSide::P1),
                profile::is_session_side_joined(profile::PlayerSide::P2),
                125.0,
            ),
        };
        if !show_p1 && !show_p2 {
            match profile::get_session_player_side() {
                profile::PlayerSide::P1 => show_p1 = true,
                profile::PlayerSide::P2 => show_p2 = true,
            }
        }
        actors.extend(test_input::build_select_music_overlay(
            &state.test_input_overlay,
            show_p1,
            show_p2,
            pad_spacing,
        ));
        return actors;
    }

    if let sort_menu::State::Visible {
        page,
        selected_index,
    } = state.sort_menu
    {
        actors.extend(sort_menu::build_overlay(sort_menu::RenderParams {
            items: sort_menu_items(state, page),
            selected_index,
            prev_selected_index: state.sort_menu_prev_selected_index,
            focus_anim_elapsed: state.sort_menu_focus_anim_elapsed,
            selected_color: color::simply_love_rgba(state.active_color_index),
        }));
    }

    if let Some(leaderboard_overlay) = sort_menu::build_leaderboard_overlay(&state.leaderboard) {
        actors.extend(leaderboard_overlay);
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
