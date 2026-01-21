use crate::act;
use crate::assets::AssetManager;
use crate::core::audio;
use crate::core::input::{InputEvent, PadDir, VirtualAction};
use crate::core::space::{
    is_wide, screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use crate::game::chart::ChartData;
use crate::game::profile;
use crate::game::scores;
use crate::game::song::{SongData, get_song_cache};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use crate::ui::components::screen_bar::{
    self, AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::ui::components::{heart_bg, music_wheel, pad_display};
use crate::ui::font;
use log::info;
use rssp::bpm::parse_bpm_map;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::KeyCode;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.5;
const TRANSITION_OUT_DURATION: f32 = 0.3;

// --- THEME LAYOUT CONSTANTS ---
const BANNER_NATIVE_WIDTH: f32 = 418.0;
const BANNER_NATIVE_HEIGHT: f32 = 164.0;
static UI_BOX_BG_COLOR: LazyLock<[f32; 4]> = LazyLock::new(|| color::rgba_hex("#1E282F"));

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

#[derive(Clone, Debug)]
pub enum MusicWheelEntry {
    PackHeader {
        name: String,
        original_index: usize,
        banner_path: Option<PathBuf>,
    },
    Song(Arc<SongData>),
}

pub struct State {
    pub entries: Vec<MusicWheelEntry>,
    pub selected_index: usize,
    pub selected_steps_index: usize,
    pub preferred_difficulty_index: usize,
    pub active_color_index: i32,
    pub selection_animation_timer: f32,
    pub wheel_offset_from_selection: f32,
    pub current_banner_key: String,
    pub current_graph_key: String,
    pub session_elapsed: f32,
    pub displayed_chart_data: Option<Arc<ChartData>>,

    // Internal state
    all_entries: Vec<MusicWheelEntry>,
    expanded_pack_name: Option<String>,
    bg: heart_bg::State,
    last_requested_banner_path: Option<PathBuf>,
    last_requested_chart_hash: Option<String>,
    active_chord_keys: HashSet<KeyCode>,
    last_steps_nav_key: Option<KeyCode>,
    last_steps_nav_time: Option<Instant>,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    currently_playing_preview_path: Option<PathBuf>,
    prev_selected_index: usize,
    time_since_selection_change: f32,

    // Caches to avoid O(N) ops in hot paths
    pub pack_song_counts: HashMap<String, usize>,
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
    edit_charts_sorted(song, chart_type).get(edit_index).copied()
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
    let mut new_entries = Vec::with_capacity(state.all_entries.len());
    let mut current_pack_name: Option<&String> = None;

    // Linear pass, minimized cloning
    for entry in &state.all_entries {
        match entry {
            MusicWheelEntry::PackHeader { name, .. } => {
                current_pack_name = Some(name);
                new_entries.push(entry.clone());
            }
            MusicWheelEntry::Song(_) => {
                if state.expanded_pack_name.as_ref() == current_pack_name.cloned().as_ref() {
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

pub fn init() -> State {
    info!("Initializing SelectMusic screen...");
    let song_cache = get_song_cache();
    let mut all_entries = Vec::with_capacity(song_cache.len() * 10); // Heuristic alloc
    let mut pack_song_counts = HashMap::with_capacity(song_cache.len());
    let target_chart_type = profile::get_session_play_style().chart_type();

    let profile_data = profile::get();
    let max_diff_index = color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1);
    let initial_diff_index = if max_diff_index == 0 {
        0
    } else {
        profile_data.last_difficulty_index.min(max_diff_index)
    };

    let mut last_song_arc: Option<Arc<SongData>> = None;
    let mut last_pack_name: Option<String> = None;

    // Filter and build entries in one pass
    for (i, pack) in song_cache.iter().enumerate() {
        let style_songs: Vec<Arc<SongData>> = pack
            .songs
            .iter()
            .filter(|song| {
                song.charts
                    .iter()
                    .any(|c| c.chart_type.eq_ignore_ascii_case(target_chart_type))
            })
            .cloned()
            .collect();

        if !style_songs.is_empty() {
            // Compute cache for get_actors (HOT PATH OPTIMIZATION)
            pack_song_counts.insert(pack.name.clone(), style_songs.len());

            all_entries.push(MusicWheelEntry::PackHeader {
                name: pack.name.clone(),
                original_index: i,
                banner_path: pack.banner_path.clone(),
            });

            // Check for last played song
            if let Some(last_path) = profile_data.last_song_music_path.as_deref() {
                if last_song_arc.is_none() {
                    for song in &style_songs {
                        if song
                            .music_path
                            .as_ref()
                            .is_some_and(|p| p.to_string_lossy() == last_path)
                        {
                            last_song_arc = Some(song.clone());
                            last_pack_name = Some(pack.name.clone());
                        }
                    }
                }
            }

            for song in style_songs {
                all_entries.push(MusicWheelEntry::Song(song));
            }
        }
    }

    let mut state = State {
        all_entries,
        entries: Vec::new(),
        selected_index: 0,
        selected_steps_index: initial_diff_index,
        preferred_difficulty_index: initial_diff_index,
        active_color_index: color::DEFAULT_COLOR_INDEX,
        selection_animation_timer: 0.0,
        wheel_offset_from_selection: 0.0,
        expanded_pack_name: last_pack_name,
        bg: heart_bg::State::new(),
        last_requested_banner_path: None,
        current_banner_key: "banner1.png".to_string(),
        last_requested_chart_hash: None,
        current_graph_key: "__white".to_string(),
        active_chord_keys: HashSet::new(),
        last_steps_nav_key: None,
        last_steps_nav_time: None,
        nav_key_held_direction: None,
        nav_key_held_since: None,
        currently_playing_preview_path: None,
        session_elapsed: 0.0,
        prev_selected_index: 0,
        time_since_selection_change: 0.0,
        displayed_chart_data: None,
        pack_song_counts,
    };

    rebuild_displayed_entries(&mut state);

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
                    state.prev_selected_index = state.selected_index;
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
            }
        }
    }

    state.prev_selected_index = state.selected_index;
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

pub fn handle_pad_dir(state: &mut State, dir: PadDir, pressed: bool) -> ScreenAction {
    if pressed {
        match dir {
            PadDir::Right => {
                if state.nav_key_held_direction == Some(NavDirection::Right) {
                    return ScreenAction::None;
                }
                music_wheel_change(state, 1);
                state.nav_key_held_direction = Some(NavDirection::Right);
                state.nav_key_held_since = Some(Instant::now());
            }
            PadDir::Left => {
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
                    let kc = if is_up {
                        KeyCode::ArrowUp
                    } else {
                        KeyCode::ArrowDown
                    };
                    let now = Instant::now();

                    if state.last_steps_nav_key == Some(kc)
                        && state
                            .last_steps_nav_time
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

                        state.last_steps_nav_key = None;
                        state.last_steps_nav_time = None;
                    } else {
                        state.last_steps_nav_key = Some(kc);
                        state.last_steps_nav_time = Some(now);
                    }

                    // Combo check
                    let other_key = if is_up {
                        KeyCode::ArrowDown
                    } else {
                        KeyCode::ArrowUp
                    };
                    if state.active_chord_keys.contains(&other_key) {
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
                    state.active_chord_keys.insert(kc);
                }
            }
        }
    } else {
        match dir {
            PadDir::Up => {
                state.active_chord_keys.remove(&KeyCode::ArrowUp);
            }
            PadDir::Down => {
                state.active_chord_keys.remove(&KeyCode::ArrowDown);
            }
            PadDir::Left => {
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

pub fn handle_confirm(state: &mut State) -> ScreenAction {
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    if state.entries.is_empty() {
        audio::play_sfx("assets/sounds/expand.ogg");
        return ScreenAction::None;
    }
    match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(_)) => ScreenAction::Navigate(Screen::PlayerOptions),
        Some(MusicWheelEntry::PackHeader { name, .. }) => {
            audio::play_sfx("assets/sounds/expand.ogg");
            let target = name.clone();
            if state.expanded_pack_name.as_ref() == Some(&target) {
                state.expanded_pack_name = None;
            } else {
                state.expanded_pack_name = Some(target.clone());
            }
            rebuild_displayed_entries(state);
            if let Some(new_sel) = state.entries.iter().position(|e| {
                matches!(e, MusicWheelEntry::PackHeader { name, .. } if name == &target)
            }) {
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
    if key.state != ElementState::Pressed {
        return ScreenAction::None;
    }
    if let winit::keyboard::PhysicalKey::Code(KeyCode::F7) = key.physical_key {
        let target_chart_type = profile::get_session_play_style().chart_type();
        if let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) {
            if let Some(chart) = chart_for_steps_index(song, target_chart_type, state.selected_steps_index) {
                return ScreenAction::FetchOnlineGrade(chart.short_hash.clone());
            }
        }
    }
    ScreenAction::None
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    match ev.action {
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
        VirtualAction::p1_back if ev.pressed => ScreenAction::Navigate(Screen::Menu),
        _ => ScreenAction::None,
    }
}

pub fn update(state: &mut State, dt: f32) -> ScreenAction {
    state.time_since_selection_change += dt;
    if dt > 0.0 {
        state.selection_animation_timer += dt;
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
            state.displayed_chart_data = None;
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
        }
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
                    audio::play_music(
                        path,
                        cut,
                        true,
                        crate::game::profile::get_session_music_rate(),
                    );
                } else {
                    audio::stop_music();
                }
            } else {
                audio::stop_music();
            }
        }

        let chart_disp = selected_song.as_ref().and_then(|song| {
            let target_chart_type = profile::get_session_play_style().chart_type();
            chart_for_steps_index(song, target_chart_type, state.selected_steps_index).cloned()
        });
        state.displayed_chart_data = chart_disp.clone().map(Arc::new);

        let new_hash = chart_disp.as_ref().map(|c| c.short_hash.clone());
        if state.last_requested_chart_hash != new_hash {
            state.last_requested_chart_hash = new_hash;
            return ScreenAction::RequestDensityGraph(chart_disp);
        }
    } else if state.currently_playing_preview_path.is_some() {
        state.currently_playing_preview_path = None;
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
    state.last_requested_banner_path = None;
}

pub fn reset_preview_after_gameplay(state: &mut State) {
    state.currently_playing_preview_path = None;
    trigger_immediate_refresh(state);
}

pub fn prime_displayed_chart_data(state: &mut State) {
    if let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) {
        let target_chart_type = profile::get_session_play_style().chart_type();
        state.displayed_chart_data =
            chart_for_steps_index(song, target_chart_type, state.selected_steps_index)
                .cloned()
                .map(Arc::new);
        return;
    }
    state.displayed_chart_data = None;
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

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(256);
    let profile = profile::get();
    let play_style = crate::game::profile::get_session_play_style();
    let is_p2_single = play_style == crate::game::profile::PlayStyle::Single
        && crate::game::profile::get_session_player_side() == crate::game::profile::PlayerSide::P2;
    let target_chart_type = play_style.chart_type();

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));
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
    }));

    let footer_avatar = profile
        .avatar_texture_key
        .as_deref()
        .map(|k| AvatarParams { texture_key: k });
    actors.push(screen_bar::build(ScreenBarParams {
        title: "EVENT MODE",
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: Some(&profile.display_name),
        center_text: None,
        right_text: Some("PRESS START"),
        left_avatar: footer_avatar,
    }));

    let preferred_idx = state
        .preferred_difficulty_index
        .min(color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1));
    let mut sel_col = color::difficulty_rgba(color::FILE_DIFFICULTY_NAMES[preferred_idx], state.active_color_index);
    if let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) {
        if let Some(chart) = chart_for_steps_index(song, target_chart_type, state.selected_steps_index) {
            sel_col = color::difficulty_rgba(&chart.difficulty, state.active_color_index);
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
    let immediate_chart = match entry_opt {
        Some(MusicWheelEntry::Song(s)) => {
            chart_for_steps_index(s, target_chart_type, state.selected_steps_index)
        }
        _ => None,
    };

    let disp_chart = state.displayed_chart_data.as_deref();

    let (step_artist, steps, jumps, holds, mines, hands, rolls, meter) =
        if let Some(c) = immediate_chart {
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

    // Step Artist & Steps
    let comp_h = screen_height() / 28.0;
    let y_cen =
        (screen_center_y() - 9.0) - 0.5 * comp_h + if is_p2_single { 88.0 } else { 0.0 };
    let step_artist_x0 = if is_p2_single {
        screen_center_x() - 244.0
    } else if is_wide() {
        screen_center_x() - 356.0
    } else {
        screen_center_x() - 346.0
    };
    let q_cx = step_artist_x0 + 113.0;
    let s_x = step_artist_x0 + 30.0;
    let a_x = step_artist_x0 + 75.0;

    actors.push(act!(quad: align(0.5, 0.5): xy(q_cx, y_cen): setsize(175.0, comp_h): z(120): diffuse(sel_col[0], sel_col[1], sel_col[2], 1.0)));
    actors.push(act!(text: font("miso"): settext("STEPS"): align(0.0, 0.5): xy(s_x, y_cen): zoom(0.8): maxwidth(40.0): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
    actors.push(act!(text: font("miso"): settext(step_artist): align(0.0, 0.5): xy(a_x, y_cen): zoom(0.8): maxwidth(124.0): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));

    // Density Graph
    let panel_w = if is_wide() { 286.0 } else { 276.0 };
    let mut graph_kids = vec![
        act!(quad: align(0.0, 0.0): xy(0.0, 0.0): setsize(panel_w, 64.0): diffuse(UI_BOX_BG_COLOR[0], UI_BOX_BG_COLOR[1], UI_BOX_BG_COLOR[2], UI_BOX_BG_COLOR[3])),
    ];

    if let Some(c) = disp_chart {
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

        let peak_x = panel_w * 0.5 + if is_p2_single { -136.0 } else { 60.0 };
        graph_kids.push(act!(sprite(state.current_graph_key.clone()): align(0.0, 0.0): xy(0.0, 0.0): setsize(panel_w, 64.0)));
        graph_kids.push(act!(text: font("miso"): settext(peak): align(0.0, 0.5): xy(peak_x, -9.0): zoom(0.8): diffuse(1.0, 1.0, 1.0, 1.0)));
        graph_kids.push(act!(quad: align(0.0, 0.0): xy(0.0, 47.0): setsize(panel_w, 17.0): diffuse(0.0, 0.0, 0.0, 0.5)));
        graph_kids.push(act!(text: font("miso"): settext(bd_text): align(0.5, 0.5): xy(panel_w * 0.5, 55.5): zoom(0.8): maxwidth(panel_w)));
    }

    let chart_info_cx = screen_center_x() - 182.0 - if is_wide() { 5.0 } else { 0.0 };
    let graph_cy = screen_center_y() + if is_p2_single { 111.0 } else { 23.0 };
    actors.push(Actor::Frame {
        align: [0.0, 0.0],
        offset: [
            chart_info_cx - 0.5 * panel_w,
            graph_cy - 32.0,
        ],
        size: [SizeSpec::Px(panel_w), SizeSpec::Px(64.0)],
        background: None,
        z: 51,
        children: graph_kids,
    });

    // Pane Display
    let pane_cx = if is_p2_single {
        screen_width() * 0.75 + 5.0
    } else {
        screen_width() * 0.25 - 5.0
    };
    let pane_top = screen_height() - 92.0;
    actors.push(act!(quad: align(0.5, 0.0): xy(pane_cx, pane_top): setsize(screen_width() / 2.0 - 10.0, 60.0): z(120): diffuse(sel_col[0], sel_col[1], sel_col[2], 1.0)));

    let tz = widescale(0.8, 0.9);
    let cols = [
        widescale(-104.0, -133.0),
        widescale(-36.0, -38.0),
        widescale(54.0, 76.0),
        widescale(150.0, 190.0),
    ];
    let rows = [13.0, 31.0, 49.0];

    // Stats Grid
    let stats = [
        ("Steps", &steps),
        ("Mines", &mines),
        ("Jumps", &jumps),
        ("Hands", &hands),
        ("Holds", &holds),
        ("Rolls", &rolls),
    ];
    for (i, (lbl, val)) in stats.iter().enumerate() {
        let (c, r) = (i % 2, i / 2);
        actors.push(act!(text: font("miso"): settext(*val): align(1.0, 0.5): horizalign(right): xy(pane_cx + cols[c], pane_top + rows[r]): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
        actors.push(act!(text: font("miso"): settext(*lbl): align(0.0, 0.5): xy(pane_cx + cols[c] + 3.0, pane_top + rows[r]): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
    }

    // Scores
    let (s_name, s_pct) = if let Some(c) = immediate_chart {
        if let Some(sc) = scores::get_cached_score(&c.short_hash) {
            if sc.grade != scores::Grade::Failed {
                (
                    profile.player_initials.clone(),
                    format!("{:.2}%", sc.score_percent * 100.0),
                )
            } else {
                ("----".to_string(), "??.??%".to_string())
            }
        } else {
            ("----".to_string(), "??.??%".to_string())
        }
    } else {
        ("----".to_string(), "??.??%".to_string())
    };

    for i in 0..2 {
        actors.push(act!(text: font("miso"): settext(&s_name): align(0.5, 0.5): xy(pane_cx + cols[2] - 50.0 * tz, pane_top + rows[i]): maxwidth(30.0): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
        actors.push(act!(text: font("miso"): settext(&s_pct): align(1.0, 0.5): xy(pane_cx + cols[2] + 25.0 * tz, pane_top + rows[i]): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
    }

    // Difficulty Meter
    let mut m_actor = act!(text: font("wendy"): settext(meter): align(1.0, 0.5): horizalign(right): xy(pane_cx + cols[3], pane_top + rows[1]): z(121): diffuse(0.0, 0.0, 0.0, 1.0));
    if !is_wide() {
        if let Actor::Text { max_width, .. } = &mut m_actor {
            *max_width = Some(66.0);
        }
    }
    actors.push(m_actor);

    // Pattern Info
    let (cross, foot, side, jack, brack, stream) = if let Some(c) = disp_chart {
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

    // Steps Display List
    let lst_cx = screen_center_x() - 26.0;
    let lst_cy = screen_center_y() + 67.0;
    actors.push(act!(quad: align(0.5, 0.5): xy(lst_cx, lst_cy): setsize(32.0, 152.0): z(120): diffuse(UI_BOX_BG_COLOR[0], UI_BOX_BG_COLOR[1], UI_BOX_BG_COLOR[2], UI_BOX_BG_COLOR[3])));

    const VISIBLE_STEPS_SLOTS: usize = 5;
    let (steps_charts, selected_steps_index) = match entry_opt {
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
            (v, state.selected_steps_index)
        }
        _ => (vec![None; color::FILE_DIFFICULTY_NAMES.len()], state.preferred_difficulty_index),
    };
    let list_len = steps_charts.len();
    let selected_steps_index = selected_steps_index.min(list_len.saturating_sub(1));
    let top_index = if list_len > VISIBLE_STEPS_SLOTS {
        // Simply Love: keep Edit charts off-screen until you scroll past Expert.
        // Once you're in Edit charts, keep the selected chart in the bottom slot and
        // shift the other difficulties upward as you move deeper.
        selected_steps_index
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
        preferred_difficulty_index: state.preferred_difficulty_index,
        selected_steps_index: state.selected_steps_index,
    }));

    // Bouncing Arrow
    let arrow_slot = (selected_steps_index.saturating_sub(top_index)).min(VISIBLE_STEPS_SLOTS - 1);
    let arrow_y = lst_cy + (arrow_slot as i32 - 2) as f32 * 30.0 + 1.0;
    let bpm_val = if let Some(MusicWheelEntry::Song(s)) = entry_opt {
        s.max_bpm.max(1.0)
    } else {
        150.0
    };
    let phase = (state.session_elapsed / (60.0 / bpm_val as f32)) * 6.28318;
    let (arrow_x0, arrow_dx, arrow_align_x, arrow_rot) = if is_p2_single {
        let x0 = lst_cx + 14.0 + 1.0;
        (x0, 1.5 - 1.5 * phase.cos(), 0.0, 180.0)
    } else {
        (
            screen_center_x() - 53.0,
            -1.5 + 1.5 * phase.cos(),
            0.0,
            0.0,
        )
    };
    actors.push(act!(sprite("meter_arrow.png"):
        align(arrow_align_x, 0.5):
        xy(arrow_x0 + arrow_dx, arrow_y):
        rotationz(arrow_rot):
        zoom(0.575):
        z(122)
    ));

    actors
}
