use crate::act;
use crate::assets::AssetManager;
use crate::core::audio;
use crate::core::input::{InputEvent, PadDir, VirtualAction};
use crate::core::space::{
    is_wide, screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use crate::game::chart::ChartData;
use crate::game::course::get_course_cache;
use crate::game::profile;
use crate::game::song::{SongData, get_song_cache};
use crate::rgba_const;
use crate::screens::components::{
    gs_scorebox, heart_bg, music_wheel, pad_display, select_pane, select_shared, step_artist_bar,
};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::select_music::MusicWheelEntry;

const TRANSITION_IN_DURATION: f32 = 0.5;
const TRANSITION_OUT_DURATION: f32 = 0.3;
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(250);
const MUSIC_WHEEL_SWITCH_SECONDS: f32 = 0.10;
const MUSIC_WHEEL_SETTLE_MIN_SPEED: f32 = 0.2;
const MUSIC_WHEEL_HOLD_SPIN_SPEED: f32 = 15.0;
const MUSIC_WHEEL_STOP_SPINDOWN_THRESHOLD: f32 = 0.25;
const BANNER_NATIVE_WIDTH: f32 = 418.0;
const BANNER_NATIVE_HEIGHT: f32 = 164.0;
const BANNER_UPDATE_DELAY_SECONDS: f32 = 0.01;

rgba_const!(UI_BOX_BG_COLOR, "#1E282F");
rgba_const!(COURSE_WHEEL_SONG_TEXT_COLOR, "#D77272");

#[derive(Clone, Debug)]
struct CourseSongEntry {
    title: String,
    difficulty: String,
    meter: Option<u32>,
    step_artist: String,
}

#[derive(Clone, Copy, Debug, Default)]
struct CourseTotals {
    steps: u32,
    jumps: u32,
    holds: u32,
    mines: u32,
    hands: u32,
    rolls: u32,
}

#[derive(Clone, Debug)]
struct CourseMeta {
    path: PathBuf,
    name: String,
    scripter: String,
    description: String,
    banner_path: Option<PathBuf>,
    entries: Vec<CourseSongEntry>,
    totals: CourseTotals,
    rated_entry_count: usize,
    meter_sum: u32,
    meter_count: usize,
    min_bpm: Option<f64>,
    max_bpm: Option<f64>,
    total_length_seconds: i32,
}

struct InitData {
    all_entries: Vec<MusicWheelEntry>,
    pack_course_counts: HashMap<String, usize>,
    course_meta_by_path: HashMap<PathBuf, Arc<CourseMeta>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum NavDirection {
    Left,
    Right,
}

pub struct State {
    pub entries: Vec<MusicWheelEntry>,
    pub selected_index: usize,
    pub active_color_index: i32,
    pub selection_animation_timer: f32,
    pub wheel_offset_from_selection: f32,
    pub current_banner_key: String,
    pub session_elapsed: f32,

    all_entries: Vec<MusicWheelEntry>,
    pack_course_counts: HashMap<String, usize>,
    course_meta_by_path: HashMap<PathBuf, Arc<CourseMeta>>,
    bg: heart_bg::State,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    last_requested_banner_path: Option<PathBuf>,
    prev_selected_index: usize,
    time_since_selection_change: f32,
}

#[inline(always)]
fn song_dir_key(song: &SongData) -> Option<String> {
    song.simfile_path
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .map(|s| s.trim().to_ascii_lowercase())
}

fn build_song_lookup() -> (
    HashMap<(String, String), Arc<SongData>>,
    HashMap<String, Arc<SongData>>,
) {
    let song_cache = get_song_cache();
    let mut by_group_song: HashMap<(String, String), Arc<SongData>> = HashMap::new();
    let mut by_song: HashMap<String, Arc<SongData>> = HashMap::new();

    for pack in song_cache.iter() {
        let group_key = pack.group_name.trim().to_ascii_lowercase();
        for song in &pack.songs {
            let Some(song_key) = song_dir_key(song) else {
                continue;
            };
            by_group_song.insert((group_key.clone(), song_key.clone()), song.clone());
            by_song.entry(song_key).or_insert_with(|| song.clone());
        }
    }

    (by_group_song, by_song)
}

#[inline(always)]
fn course_group_name(path: &Path) -> String {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Courses".to_string())
}

#[inline(always)]
fn course_name(path: &Path, course: &rssp::course::CourseFile) -> String {
    if !course.name.trim().is_empty() {
        course.name.clone()
    } else {
        path.file_stem()
            .and_then(|n| n.to_str())
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| "Untitled Course".to_string())
    }
}

fn resolve_course_banner(
    course_path: &Path,
    course: &rssp::course::CourseFile,
    fallback: Option<PathBuf>,
) -> Option<PathBuf> {
    let banner_tag = course.banner.trim();
    if banner_tag.is_empty() {
        return fallback;
    }
    let rel = Path::new(banner_tag);
    if rel.is_absolute() && rel.is_file() {
        return Some(rel.to_path_buf());
    }
    let parent = course_path.parent().unwrap_or_else(|| Path::new(""));
    let joined = parent.join(rel);
    if joined.is_file() {
        return Some(joined);
    }
    fallback
}

#[inline(always)]
fn course_steps_label(steps: &rssp::course::StepsSpec) -> String {
    match steps {
        rssp::course::StepsSpec::Difficulty(diff) => rssp::course::difficulty_label(*diff)
            .to_ascii_lowercase()
            .to_string(),
        rssp::course::StepsSpec::MeterRange { low, high } => format!("{low}-{high}"),
        rssp::course::StepsSpec::Unknown { raw } => {
            if raw.trim().is_empty() {
                "?".to_string()
            } else {
                raw.trim().to_string()
            }
        }
    }
}

#[inline(always)]
fn course_entry_song_label(entry: &rssp::course::CourseEntry) -> String {
    match &entry.song {
        rssp::course::CourseSong::Fixed { song, .. } => song.clone(),
        rssp::course::CourseSong::RandomAny => "RANDOM".to_string(),
        rssp::course::CourseSong::RandomWithinGroup { group } => format!("{group}/*"),
        rssp::course::CourseSong::SortPick { .. } => "SORT PICK".to_string(),
        rssp::course::CourseSong::Unknown { raw } => raw.clone(),
    }
}

#[inline(always)]
fn chart_step_artist(chart: &ChartData) -> String {
    if chart.difficulty.eq_ignore_ascii_case("edit") && !chart.description.trim().is_empty() {
        chart.description.clone()
    } else if !chart.step_artist.trim().is_empty() {
        chart.step_artist.clone()
    } else {
        "Unknown".to_string()
    }
}

fn resolve_course_chart<'a>(
    song: &'a SongData,
    entry: &rssp::course::CourseEntry,
    chart_type: &str,
) -> Option<&'a ChartData> {
    let mut first_chart = None;
    let mut first_playable = None;
    let mut meter_match = None;
    let target_diff = match &entry.steps {
        rssp::course::StepsSpec::Difficulty(diff) => Some(rssp::course::difficulty_label(*diff)),
        _ => None,
    };

    for chart in &song.charts {
        if !chart.chart_type.eq_ignore_ascii_case(chart_type) {
            continue;
        }
        if first_chart.is_none() {
            first_chart = Some(chart);
        }
        if chart.notes.is_empty() {
            continue;
        }
        if first_playable.is_none() {
            first_playable = Some(chart);
        }
        if let Some(target) = target_diff
            && chart.difficulty.eq_ignore_ascii_case(target)
        {
            return Some(chart);
        }
        if let rssp::course::StepsSpec::MeterRange { low, high } = &entry.steps {
            let meter = chart.meter as i32;
            if meter >= *low && meter <= *high && meter_match.is_none() {
                meter_match = Some(chart);
            }
        }
    }

    meter_match.or(first_playable).or(first_chart)
}

#[inline(always)]
fn push_song_bpm_range(min_bpm: &mut Option<f64>, max_bpm: &mut Option<f64>, song: &SongData) {
    let mut lo = song.min_bpm;
    let mut hi = song.max_bpm;
    if lo <= 0.0 && hi > 0.0 {
        lo = hi;
    }
    if hi <= 0.0 && lo > 0.0 {
        hi = lo;
    }
    if lo <= 0.0 || hi <= 0.0 {
        return;
    }
    *min_bpm = Some(min_bpm.map_or(lo, |curr| curr.min(lo)));
    *max_bpm = Some(max_bpm.map_or(hi, |curr| curr.max(hi)));
}

#[inline(always)]
fn add_chart_totals(totals: &mut CourseTotals, chart: &ChartData) {
    totals.steps = totals.steps.saturating_add(chart.stats.total_steps);
    totals.jumps = totals.jumps.saturating_add(chart.stats.jumps);
    totals.holds = totals.holds.saturating_add(chart.stats.holds);
    totals.mines = totals.mines.saturating_add(chart.mines_nonfake);
    totals.hands = totals.hands.saturating_add(chart.stats.hands);
    totals.rolls = totals.rolls.saturating_add(chart.stats.rolls);
}

fn make_course_song(meta: &CourseMeta) -> SongData {
    SongData {
        simfile_path: meta.path.clone(),
        title: meta.name.clone(),
        subtitle: String::new(),
        translit_title: meta.name.clone(),
        translit_subtitle: String::new(),
        artist: if meta.scripter.trim().is_empty() {
            "Course".to_string()
        } else {
            meta.scripter.clone()
        },
        banner_path: meta.banner_path.clone(),
        background_path: None,
        music_path: None,
        display_bpm: String::new(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: meta.min_bpm.unwrap_or(0.0),
        max_bpm: meta.max_bpm.unwrap_or(meta.min_bpm.unwrap_or(0.0)),
        normalized_bpms: String::new(),
        normalized_stops: String::new(),
        normalized_delays: String::new(),
        normalized_warps: String::new(),
        normalized_speeds: String::new(),
        normalized_scrolls: String::new(),
        normalized_fakes: String::new(),
        music_length_seconds: meta.total_length_seconds.max(0) as f32,
        total_length_seconds: meta.total_length_seconds.max(0),
        charts: Vec::new(),
    }
}

fn build_init_data() -> InitData {
    let translated_titles = crate::config::get().translated_titles;
    let target_chart_type = profile::get_session_play_style().chart_type();
    let (by_group_song, by_song) = build_song_lookup();
    let course_cache = get_course_cache();

    let mut grouped: HashMap<String, Vec<Arc<CourseMeta>>> = HashMap::new();
    let mut course_meta_by_path: HashMap<PathBuf, Arc<CourseMeta>> = HashMap::new();

    for (path, course) in course_cache.iter() {
        let mut entries = Vec::with_capacity(course.entries.len());
        let mut total_seconds = 0i32;
        let mut first_banner: Option<PathBuf> = None;
        let mut totals = CourseTotals::default();
        let mut rated_entry_count = 0usize;
        let mut meter_sum = 0u32;
        let mut meter_count = 0usize;
        let mut min_bpm = None;
        let mut max_bpm = None;

        for entry in &course.entries {
            let mut title = course_entry_song_label(entry);
            let mut difficulty = course_steps_label(&entry.steps);
            let mut meter = None;
            let mut step_artist = if course.scripter.trim().is_empty() {
                "Unknown".to_string()
            } else {
                course.scripter.clone()
            };

            let resolved = match &entry.song {
                rssp::course::CourseSong::Fixed { group, song } => {
                    let song_key = song.trim().to_ascii_lowercase();
                    if let Some(group) = group.as_deref().map(str::trim) {
                        let group_key = group.to_ascii_lowercase();
                        by_group_song.get(&(group_key, song_key)).cloned()
                    } else {
                        by_song.get(&song_key).cloned()
                    }
                }
                _ => None,
            };

            if let Some(song_data) = resolved.as_ref() {
                title = song_data.display_full_title(translated_titles);
                if first_banner.is_none() {
                    first_banner = song_data.banner_path.clone();
                }
                let len = if song_data.music_length_seconds > 0.0 {
                    song_data.music_length_seconds.round() as i32
                } else {
                    song_data.total_length_seconds.max(0)
                };
                total_seconds = total_seconds.saturating_add(len.max(0));
                push_song_bpm_range(&mut min_bpm, &mut max_bpm, song_data);

                if let Some(chart) = resolve_course_chart(song_data, entry, target_chart_type) {
                    difficulty = chart.difficulty.to_ascii_lowercase();
                    meter = Some(chart.meter);
                    step_artist = chart_step_artist(chart);
                    add_chart_totals(&mut totals, chart);
                    rated_entry_count = rated_entry_count.saturating_add(1);
                    meter_sum = meter_sum.saturating_add(chart.meter);
                    meter_count = meter_count.saturating_add(1);
                }
            }
            entries.push(CourseSongEntry {
                title,
                difficulty,
                meter,
                step_artist,
            });
        }

        let group_name = course_group_name(path);
        let meta = Arc::new(CourseMeta {
            path: path.clone(),
            name: course_name(path, course),
            scripter: course.scripter.clone(),
            description: course.description.clone(),
            banner_path: resolve_course_banner(path, course, first_banner),
            entries,
            totals,
            rated_entry_count,
            meter_sum,
            meter_count,
            min_bpm,
            max_bpm,
            total_length_seconds: total_seconds.max(0),
        });

        grouped.entry(group_name).or_default().push(meta.clone());
        course_meta_by_path.insert(meta.path.clone(), meta);
    }

    let mut all_courses: Vec<Arc<CourseMeta>> = grouped.into_values().flatten().collect();
    all_courses.sort_by_cached_key(|c| c.name.to_ascii_lowercase());

    let mut all_entries = Vec::with_capacity(all_courses.len());
    for meta in all_courses {
        let song_stub = Arc::new(make_course_song(&meta));
        all_entries.push(MusicWheelEntry::Song(song_stub));
    }

    InitData {
        all_entries,
        pack_course_counts: HashMap::new(),
        course_meta_by_path,
    }
}

fn rebuild_displayed_entries(state: &mut State) {
    state.entries = state.all_entries.clone();
    if state.entries.is_empty() {
        state.wheel_offset_from_selection = 0.0;
    }
}

fn selected_course_meta(state: &State) -> Option<Arc<CourseMeta>> {
    let MusicWheelEntry::Song(song) = state.entries.get(state.selected_index)? else {
        return None;
    };
    state.course_meta_by_path.get(&song.simfile_path).cloned()
}

#[inline(always)]
fn selected_banner_path(state: &State) -> Option<PathBuf> {
    match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(song)) => song.banner_path.clone(),
        Some(MusicWheelEntry::PackHeader { banner_path, .. }) => banner_path.clone(),
        None => None,
    }
}

pub fn init() -> State {
    let init = build_init_data();
    let mut state = State {
        entries: Vec::new(),
        selected_index: 0,
        active_color_index: color::DEFAULT_COLOR_INDEX,
        selection_animation_timer: 0.0,
        wheel_offset_from_selection: 0.0,
        current_banner_key: "banner1.png".to_string(),
        session_elapsed: 0.0,
        all_entries: init.all_entries,
        pack_course_counts: init.pack_course_counts,
        course_meta_by_path: init.course_meta_by_path,
        bg: heart_bg::State::new(),
        nav_key_held_direction: None,
        nav_key_held_since: None,
        last_requested_banner_path: None,
        prev_selected_index: 0,
        time_since_selection_change: 0.0,
    };
    rebuild_displayed_entries(&mut state);
    state
}

#[inline(always)]
fn music_wheel_settle_offset(state: &mut State, dt: f32) {
    if dt <= 0.0 || state.wheel_offset_from_selection == 0.0 {
        return;
    }
    let off = state.wheel_offset_from_selection;
    let speed = MUSIC_WHEEL_SETTLE_MIN_SPEED + off.abs() / MUSIC_WHEEL_SWITCH_SECONDS;
    if off > 0.0 {
        state.wheel_offset_from_selection = (off - speed * dt).max(0.0);
    } else {
        state.wheel_offset_from_selection = (off + speed * dt).min(0.0);
    }
}

#[inline(always)]
fn music_wheel_change(state: &mut State, dist: isize) {
    if dist == 0 {
        return;
    }
    let len = state.entries.len();
    if len == 0 {
        state.selected_index = 0;
        state.wheel_offset_from_selection = 0.0;
        state.time_since_selection_change = 0.0;
        return;
    }
    if dist > 0 {
        state.selected_index = (state.selected_index + 1) % len;
        state.wheel_offset_from_selection += 1.0;
    } else {
        state.selected_index = (state.selected_index + len - 1) % len;
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
    let passed = (moving < 0.0 && off >= 0.0) || (moving > 0.0 && off <= 0.0);
    if passed {
        music_wheel_change(state, if moving < 0.0 { -1 } else { 1 });
    }
}

fn handle_wheel_dir(state: &mut State, dir: PadDir, pressed: bool, ts: Instant) -> ScreenAction {
    match (dir, pressed) {
        (PadDir::Left, true) => {
            if state.nav_key_held_direction == Some(NavDirection::Left) {
                return ScreenAction::None;
            }
            music_wheel_change(state, -1);
            state.nav_key_held_direction = Some(NavDirection::Left);
            state.nav_key_held_since = Some(ts);
        }
        (PadDir::Right, true) => {
            if state.nav_key_held_direction == Some(NavDirection::Right) {
                return ScreenAction::None;
            }
            music_wheel_change(state, 1);
            state.nav_key_held_direction = Some(NavDirection::Right);
            state.nav_key_held_since = Some(ts);
        }
        (PadDir::Left, false) => {
            if state.nav_key_held_direction == Some(NavDirection::Left) {
                let moving_started = state
                    .nav_key_held_since
                    .is_some_and(|t| ts.duration_since(t) >= NAV_INITIAL_HOLD_DELAY);
                if moving_started
                    && state.wheel_offset_from_selection.abs() < MUSIC_WHEEL_STOP_SPINDOWN_THRESHOLD
                {
                    music_wheel_change(state, -1);
                }
                state.nav_key_held_direction = None;
                state.nav_key_held_since = None;
            }
        }
        (PadDir::Right, false) => {
            if state.nav_key_held_direction == Some(NavDirection::Right) {
                let moving_started = state
                    .nav_key_held_since
                    .is_some_and(|t| ts.duration_since(t) >= NAV_INITIAL_HOLD_DELAY);
                if moving_started
                    && state.wheel_offset_from_selection.abs() < MUSIC_WHEEL_STOP_SPINDOWN_THRESHOLD
                {
                    music_wheel_change(state, 1);
                }
                state.nav_key_held_direction = None;
                state.nav_key_held_since = None;
            }
        }
        _ => {}
    }
    ScreenAction::None
}

pub fn handle_confirm(state: &mut State) -> ScreenAction {
    if state.entries.is_empty() {
        audio::play_sfx("assets/sounds/start.ogg");
        return ScreenAction::None;
    }
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;

    match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(_)) => {
            // Course gameplay handoff is not wired yet; keep the action local for now.
            audio::play_sfx("assets/sounds/start.ogg");
            ScreenAction::None
        }
        _ => ScreenAction::None,
    }
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    let play_style = profile::get_session_play_style();
    if play_style == profile::PlayStyle::Versus {
        return match ev.action {
            VirtualAction::p1_left | VirtualAction::p1_menu_left => {
                handle_wheel_dir(state, PadDir::Left, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_right | VirtualAction::p1_menu_right => {
                handle_wheel_dir(state, PadDir::Right, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_left | VirtualAction::p2_menu_left => {
                handle_wheel_dir(state, PadDir::Left, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_right | VirtualAction::p2_menu_right => {
                handle_wheel_dir(state, PadDir::Right, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
                handle_confirm(state)
            }
            VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
                audio::play_sfx("assets/sounds/start.ogg");
                ScreenAction::Navigate(Screen::Menu)
            }
            _ => ScreenAction::None,
        };
    }

    match profile::get_session_player_side() {
        profile::PlayerSide::P1 => match ev.action {
            VirtualAction::p1_left | VirtualAction::p1_menu_left => {
                handle_wheel_dir(state, PadDir::Left, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_right | VirtualAction::p1_menu_right => {
                handle_wheel_dir(state, PadDir::Right, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_start if ev.pressed => handle_confirm(state),
            VirtualAction::p1_back if ev.pressed => {
                audio::play_sfx("assets/sounds/start.ogg");
                ScreenAction::Navigate(Screen::Menu)
            }
            _ => ScreenAction::None,
        },
        profile::PlayerSide::P2 => match ev.action {
            VirtualAction::p2_left | VirtualAction::p2_menu_left => {
                handle_wheel_dir(state, PadDir::Left, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_right | VirtualAction::p2_menu_right => {
                handle_wheel_dir(state, PadDir::Right, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_start if ev.pressed => handle_confirm(state),
            VirtualAction::p2_back if ev.pressed => {
                audio::play_sfx("assets/sounds/start.ogg");
                ScreenAction::Navigate(Screen::Menu)
            }
            _ => ScreenAction::None,
        },
    }
}

pub fn update(state: &mut State, dt: f32) -> ScreenAction {
    let dt = dt.max(0.0);
    state.selection_animation_timer += dt;
    state.time_since_selection_change += dt;

    let now = Instant::now();
    let moving = state
        .nav_key_held_since
        .is_some_and(|t| now.duration_since(t) >= NAV_INITIAL_HOLD_DELAY);
    if moving {
        match state.nav_key_held_direction.clone() {
            Some(dir) => music_wheel_update_hold_scroll(state, dt, dir),
            None => music_wheel_settle_offset(state, dt),
        }
    } else {
        music_wheel_settle_offset(state, dt);
    }

    if state.selected_index != state.prev_selected_index {
        state.prev_selected_index = state.selected_index;
        state.time_since_selection_change = 0.0;
        audio::play_sfx("assets/sounds/change.ogg");
    }

    if state.time_since_selection_change >= BANNER_UPDATE_DELAY_SECONDS {
        let banner = selected_banner_path(state);
        if banner != state.last_requested_banner_path {
            state.last_requested_banner_path = banner.clone();
            return ScreenAction::RequestBanner(banner);
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

#[inline(always)]
pub fn trigger_immediate_refresh(state: &mut State) {
    state.time_since_selection_change = BANNER_UPDATE_DELAY_SECONDS;
    state.last_requested_banner_path = None;
}

#[inline(always)]
pub fn allows_late_join(_state: &State) -> bool {
    true
}

fn format_session_time(seconds: f32) -> String {
    if seconds < 0.0 {
        return "00:00".to_string();
    }
    let s = seconds as u64;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn format_len(seconds: i32) -> String {
    let s = seconds.max(0) as u64;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[inline(always)]
fn format_bpm_value(bpm: f64) -> String {
    if !bpm.is_finite() || bpm <= 0.0 {
        return "?".to_string();
    }
    let rounded = bpm.round();
    if (bpm - rounded).abs() < 0.05 {
        format!("{}", rounded as i32)
    } else {
        format!("{bpm:.1}")
    }
}

fn format_bpm_range(min_bpm: Option<f64>, max_bpm: Option<f64>) -> String {
    let (Some(min_bpm), Some(max_bpm)) = (min_bpm, max_bpm) else {
        return "?".to_string();
    };
    let lo = min_bpm.min(max_bpm);
    let hi = min_bpm.max(max_bpm);
    let lo_txt = format_bpm_value(lo);
    let hi_txt = format_bpm_value(hi);
    if (hi - lo).abs() < 0.05 {
        lo_txt
    } else {
        format!("{lo_txt}-{hi_txt}")
    }
}

fn sl_select_music_bg_flash() -> Actor {
    act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(-98):
        sleep(0.6):
        linear(0.5): alpha(0.0):
        linear(0.0): visible(false)
    )
}

pub fn get_actors(state: &State, _asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(256);
    let side = profile::get_session_player_side();
    let play_style = profile::get_session_play_style();
    let is_p2_single = play_style == profile::PlayStyle::Single && side == profile::PlayerSide::P2;
    let selected_entry = state.entries.get(state.selected_index);
    let selected_meta = selected_course_meta(state);

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));
    actors.push(sl_select_music_bg_flash());
    actors.extend(select_shared::build_screen_bars("SELECT COURSE"));
    actors.push(select_shared::build_session_timer(format_session_time(
        state.session_elapsed,
    )));

    let mode_text = gs_scorebox::select_music_mode_text(profile::PlayerSide::P1, None);
    actors.push(select_shared::build_mode_pad_text(mode_text.as_str()));
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

    let (banner_zoom, banner_cx, banner_cy) = if is_wide() {
        (0.7655, screen_center_x() - 170.0, 96.0)
    } else {
        (0.75, screen_center_x() - 166.0, 96.0)
    };
    actors.push(act!(sprite(state.current_banner_key.clone()):
        align(0.5, 0.5):
        xy(banner_cx, banner_cy):
        setsize(BANNER_NATIVE_WIDTH, BANNER_NATIVE_HEIGHT):
        zoom(banner_zoom):
        z(51)
    ));

    let music_rate = profile::get_session_music_rate();
    let (songs_label, songs_value, bpm_text, len_text, desc_text) =
        match (selected_entry, selected_meta.as_ref()) {
            (Some(MusicWheelEntry::Song(_)), Some(meta)) => (
                "SONGS".to_string(),
                meta.entries.len().to_string(),
                format_bpm_range(meta.min_bpm, meta.max_bpm),
                format_len(((meta.total_length_seconds as f32) / music_rate).round() as i32),
                meta.description.clone(),
            ),
            _ => (
                "SONGS".to_string(),
                "0".to_string(),
                "?".to_string(),
                "0:00".to_string(),
                String::new(),
            ),
        };

    let (steps_text, jumps_text, holds_text, mines_text, hands_text, rolls_text, meter_text) =
        match selected_meta.as_ref() {
            Some(meta) => {
                let meter = if meta.meter_count > 0 {
                    format!("{}", (meta.meter_sum as f32 / meta.meter_count as f32).round() as i32)
                } else {
                    "?".to_string()
                };
                if meta.rated_entry_count > 0 {
                    (
                        meta.totals.steps.to_string(),
                        meta.totals.jumps.to_string(),
                        meta.totals.holds.to_string(),
                        meta.totals.mines.to_string(),
                        meta.totals.hands.to_string(),
                        meta.totals.rolls.to_string(),
                        meter,
                    )
                } else {
                    (
                        "?".to_string(),
                        "?".to_string(),
                        "?".to_string(),
                        "?".to_string(),
                        "?".to_string(),
                        "?".to_string(),
                        meter,
                    )
                }
            }
            None => (
                "?".to_string(),
                "?".to_string(),
                "?".to_string(),
                "?".to_string(),
                "?".to_string(),
                "?".to_string(),
                "?".to_string(),
            ),
        };

    let pane_sel_col = selected_meta
        .as_ref()
        .and_then(|meta| meta.entries.first())
        .map(|entry| color::difficulty_rgba(&entry.difficulty, state.active_color_index))
        .unwrap_or_else(|| color::simply_love_rgba(state.active_color_index));
    let pane_cx = if is_p2_single {
        screen_width() * 0.75 + 5.0
    } else {
        screen_width() * 0.25 - 5.0
    };
    actors.extend(select_pane::build_base(select_pane::StatsPaneParams {
        pane_cx,
        accent_color: pane_sel_col,
        values: select_pane::StatsValues {
            steps: steps_text.as_str(),
            mines: mines_text.as_str(),
            jumps: jumps_text.as_str(),
            hands: hands_text.as_str(),
            holds: holds_text.as_str(),
            rolls: rolls_text.as_str(),
        },
        meter: Some(meter_text.as_str()),
    }));

    let (box_w, frame_x, frame_y) = if is_wide() {
        (320.0, screen_center_x() - 170.0, screen_center_y() - 55.0)
    } else {
        (310.0, screen_center_x() - 165.0, screen_center_y() - 55.0)
    };
    actors.push(Actor::Frame {
        align: [0.0, 0.0],
        offset: [frame_x, frame_y],
        size: [SizeSpec::Px(box_w), SizeSpec::Px(50.0)],
        background: None,
        z: 51,
        children: vec![
            act!(quad:
                setsize(box_w, 50.0):
                diffuse(UI_BOX_BG_COLOR[0], UI_BOX_BG_COLOR[1], UI_BOX_BG_COLOR[2], UI_BOX_BG_COLOR[3])
            ),
            Actor::Frame {
                align: [0.0, 0.0],
                offset: [-110.0, -6.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                background: None,
                z: 0,
                children: vec![
                    act!(text: font("miso"): settext(songs_label): align(1.0, 0.0): y(-11.0): maxwidth(56.0): diffuse(0.5, 0.5, 0.5, 1.0): z(52)),
                    act!(text: font("miso"): settext(songs_value): align(0.0, 0.0): xy(5.0, -11.0): maxwidth(box_w - 60.0): zoomtoheight(15.0): diffuse(1.0, 1.0, 1.0, 1.0): z(52)),
                    act!(text: font("miso"): settext("BPM"): align(1.0, 0.0): y(10.0): diffuse(0.5, 0.5, 0.5, 1.0): z(52)),
                    act!(text: font("miso"): settext(bpm_text): align(0.0, 0.0): xy(5.0, 10.0): zoomtoheight(15.0): diffuse(1.0, 1.0, 1.0, 1.0): z(52)),
                    act!(text: font("miso"): settext("LENGTH"): align(1.0, 0.0): xy(box_w - 130.0, 10.0): diffuse(0.5, 0.5, 0.5, 1.0): z(52)),
                    act!(text: font("miso"): settext(len_text): align(0.0, 0.0): xy(box_w - 125.0, 10.0): zoomtoheight(15.0): diffuse(1.0, 1.0, 1.0, 1.0): z(52)),
                ],
            },
        ],
    });

    let panel_w = if is_wide() { 286.0 } else { 276.0 };
    let panel_h = 152.0;
    let panel_cx = screen_center_x() - 182.0 - if is_wide() { 5.0 } else { 0.0 };
    let panel_cy = screen_center_y() + 67.0;
    let panel_top = panel_cy - panel_h * 0.5;
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(panel_cx, panel_cy):
        setsize(panel_w, panel_h):
        z(120):
        diffuse(UI_BOX_BG_COLOR[0], UI_BOX_BG_COLOR[1], UI_BOX_BG_COLOR[2], UI_BOX_BG_COLOR[3])
    ));

    let (step_idx_text, step_artist_text, step_artist_col) = match selected_meta.as_ref() {
        Some(meta) if !meta.entries.is_empty() => {
            let idx = ((state.session_elapsed / 2.0).floor() as usize) % meta.entries.len();
            let entry = &meta.entries[idx];
            (
                format!("#{}", idx + 1),
                entry.step_artist.clone(),
                color::difficulty_rgba(&entry.difficulty, state.active_color_index),
            )
        }
        _ => (
            "#-".to_string(),
            "Step Artist".to_string(),
            [0.5, 0.5, 0.5, 1.0],
        ),
    };
    let step_artist_x0 = if is_wide() {
        screen_center_x() - 355.5
    } else {
        screen_center_x() - 345.5
    };
    let step_artist_y = (screen_center_y() - 9.0) - 0.5 * (screen_height() / 28.0);
    actors.extend(step_artist_bar::build(step_artist_bar::StepArtistBarParams {
        x0: step_artist_x0,
        center_y: step_artist_y,
        accent_color: step_artist_col,
        label_text: step_idx_text.as_str(),
        label_max_width: 22.0,
        artist_text: step_artist_text.as_str(),
        artist_x_offset: 60.0,
        artist_max_width: 138.0,
        artist_color: [
            UI_BOX_BG_COLOR[0],
            UI_BOX_BG_COLOR[1],
            UI_BOX_BG_COLOR[2],
            1.0,
        ],
    }));

    let list_left_x = panel_cx - panel_w * 0.5 + 8.0;
    let list_start_y = panel_top + 8.0;
    let list_bottom_y = panel_cy + panel_h * 0.5 - 18.0;
    let avail_h = (list_bottom_y - list_start_y).max(12.0);
    if let Some(meta) = selected_meta.as_ref()
        && !meta.entries.is_empty()
    {
        let line_h = (avail_h / (meta.entries.len() as f32)).min(14.0);
        let text_zoom = (line_h / 16.0).clamp(0.28, 0.8);
        for (i, entry) in meta.entries.iter().enumerate() {
            let y = list_start_y + i as f32 * line_h;
            let diff_text = entry
                .meter
                .map(|meter| meter.to_string())
                .unwrap_or_else(|| "?".to_string());
            let diff_color = color::difficulty_rgba(&entry.difficulty, state.active_color_index);
            actors.push(act!(text:
                font("miso"):
                settext(diff_text):
                align(0.0, 0.0):
                xy(list_left_x, y):
                zoom(text_zoom):
                maxwidth(74.0):
                z(121):
                diffuse(diff_color[0], diff_color[1], diff_color[2], 1.0)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(format!("{}. {}", i + 1, entry.title)):
                align(0.0, 0.0):
                xy(list_left_x + 78.0, y):
                zoom(text_zoom):
                maxwidth(panel_w - 92.0):
                z(121):
                diffuse(1.0, 1.0, 1.0, 1.0)
            ));
        }
    } else {
        actors.push(act!(text:
            font("miso"):
            settext("Select a course to view songs."):
            align(0.0, 0.0):
            xy(list_left_x, list_start_y):
            zoom(0.72):
            maxwidth(panel_w - 16.0):
            z(121):
            diffuse(1.0, 1.0, 1.0, 1.0)
        ));
    }

    if !desc_text.trim().is_empty() {
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(panel_cx, panel_cy + panel_h * 0.5 - 9.0):
            setsize(panel_w, 16.0):
            z(121):
            diffuse(0.0, 0.0, 0.0, 0.5)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(desc_text):
            align(0.5, 0.5):
            xy(panel_cx, panel_cy + panel_h * 0.5 - 9.0):
            zoom(0.72):
            maxwidth(panel_w - 8.0):
            z(122):
            diffuse(1.0, 1.0, 1.0, 1.0)
        ));
    }

    actors.extend(music_wheel::build(music_wheel::MusicWheelParams {
        entries: &state.entries,
        selected_index: state.selected_index,
        position_offset_from_selection: state.wheel_offset_from_selection,
        selection_animation_timer: state.selection_animation_timer,
        pack_song_counts: &state.pack_course_counts,
        color_pack_headers: true,
        preferred_difficulty_index: 0,
        selected_steps_index: 0,
        song_box_color: None,
        song_text_color: Some(COURSE_WHEEL_SONG_TEXT_COLOR),
    }));

    if !matches!(selected_entry, Some(MusicWheelEntry::Song(_))) {
        actors.push(act!(text:
            font("miso"):
            settext("Pick a course"):
            align(0.5, 0.5):
            xy(screen_center_x() - 26.0, screen_center_y() + 67.0):
            zoom(0.8):
            z(122):
            diffuse(1.0, 1.0, 1.0, 0.8)
        ));
    }

    actors
}
