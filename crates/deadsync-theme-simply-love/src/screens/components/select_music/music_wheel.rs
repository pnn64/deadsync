use crate::act;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use crate::config::{
    DefaultSyncOffset, SelectMusicItlRankMode, SelectMusicItlWheelMode, SelectMusicSongSelectBgMode,
};
use crate::screens::components::shared::banner as shared_banner;
use crate::screens::select_music::MusicWheelEntry;
use crate::views::{MUSIC_WHEEL_SLOT_COUNT, MusicWheelRuntimeView, MusicWheelSlotRuntimeRequest};
use deadlib_present::actors::Actor;
use deadlib_present::cache::{
    SharedStrCache, TextCache, cached_shared_str, cached_text, text_cache_with_capacity,
};
use deadlib_present::color;
use deadlib_present::space::widescale;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_chart::song::resolve_sync_pref;
use deadsync_chart::{
    ChartData, STANDARD_DIFFICULTY_COUNT, STANDARD_DIFFICULTY_NAMES, SongData, SyncPref,
};
use deadsync_profile as profile_data;
use deadsync_score as score_data;
use deadsync_simfile::event_intro::is_srpg_event_song;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// --- Colors ---
fn col_music_wheel_box() -> [f32; 4] {
    const C: [f32; 4] = color::rgba_hex("#0a141b");
    C
}
fn col_pack_header_box() -> [f32; 4] {
    const C: [f32; 4] = color::rgba_hex("#4c565d");
    C
}

// --- Layout Constants ---
// Simply Love theme metric: [MusicWheel] NumWheelItems=17.
// StepMania/ITGmania WheelBase allocates `ceil(NumWheelItems+2)` internal items so that
// extra off-screen items can slide in during scroll and avoid exposing gaps.
const NUM_WHEEL_ITEMS_TO_DRAW: usize = 17;
const NUM_VISIBLE_WHEEL_ITEMS: usize = NUM_WHEEL_ITEMS_TO_DRAW - 2; // 17 -> 15 visible on-screen
const NUM_WHEEL_SLOTS: usize = MUSIC_WHEEL_SLOT_COUNT; // 17 -> 19 internal
const CENTER_WHEEL_SLOT_INDEX: usize = NUM_WHEEL_SLOTS / 2;
// Upper bound on actors emitted per wheel slot with every feature enabled and
// both player sides joined (box + art + title + BG art, plus per-side grades,
// lamps, event rank/rate/score and favorite heart). A single joined side
// measures ~6 actors/slot, so 16 leaves headroom for two sides and avoids any
// mid-build Vec reallocation regardless of config.
const MAX_ACTORS_PER_WHEEL_SLOT: usize = 16;
const WHEEL_ACTOR_CAPACITY: usize = NUM_WHEEL_SLOTS * MAX_ACTORS_PER_WHEEL_SLOT + 1;
const WHEEL_DRAW_RADIUS: f32 = (NUM_WHEEL_ITEMS_TO_DRAW as f32) * 0.5; // 8.5
const SELECTION_HIGHLIGHT_BEAT_PERIOD: f32 = 2.0;
const LAMP_PULSE_PERIOD: f32 = 0.8;
const LAMP_PULSE_LERP_TO_WHITE: f32 = 0.70;
const NEW_BADGE_PULSE_PERIOD: f32 = 1.2;
const NEW_BADGE_COLOR: [f32; 4] = [0.3, 1.0, 0.3, 1.0];
const NEW_BADGE_COLOR_PEAK: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const SERIES_FOLDER_COLLAPSED: [[f32; 4]; 3] = [
    color::rgba_hex("#5c6972"),
    color::rgba_hex("#74818b"),
    color::rgba_hex("#8495a1"),
];
const SERIES_FOLDER_EXPANDED: [[f32; 4]; 3] = [
    color::rgba_hex("#516777"),
    color::rgba_hex("#677f91"),
    color::rgba_hex("#7793a7"),
];
const FOLDER_NATIVE_WIDTH: f32 = 128.0;
const FOLDER_ZOOM: f32 = 0.175;
const HEART_PULSE_PERIOD: f32 = 0.8;
const HEART_COLOR_P1: [f32; 4] = [0.3, 0.5, 1.0, 1.0]; // blue
const HEART_COLOR_P2: [f32; 4] = [1.0, 0.47, 0.47, 1.0]; // pink (#ff7777)
const HEART_ZOOM_SINGLE: f32 = 0.039; // 512 * 0.039 ≈ 20px
const HEART_ZOOM_DUAL: f32 = 0.029; // 512 * 0.029 ≈ 15px
const LOCK_COLOR_P1: [f32; 4] = [1.0, 1.0, 0.0, 1.0]; // yellow
const LOCK_COLOR_P2: [f32; 4] = [1.0, 0.5, 0.0, 1.0]; // orange
const LOCK_ZOOM_SINGLE: f32 = 0.039; // 512 * 0.039 ≈ 20px
const LOCK_ZOOM_DUAL: f32 = 0.029; // 512 * 0.029 ≈ 15px
const WHEEL_BADGE_ZOOM: f32 = 0.1875;
const ITL_RANK_TEXT_CACHE_LIMIT: usize = 1024;
const ITL_EX_TEXT_CACHE_LIMIT: usize = 1024;
const ITL_POINTS_TEXT_CACHE_LIMIT: usize = 1024;
const SRPG_RATE_TEXT_CACHE_LIMIT: usize = 512;
const PACK_COUNT_TEXT_CACHE_LIMIT: usize = 1024;
const STR_REF_CACHE_LIMIT: usize = 4096;
// Simply Love and Arrow Cloud both use zoom(0.2) for the single-line ITL wheel value.
// Our stacked Points+Score mode is deadsync-only, so it needs a smaller zoom to
// keep both lines within that same visual footprint.
const ITL_SCORE_ZOOM: f32 = 0.2;
const ITL_POINTS_SCORE_ZOOM: f32 = 0.13;
const SONG_NULL_SYNC_RIGHT_EDGE: [f32; 4] = [80.0 / 255.0, 20.0 / 255.0, 27.0 / 255.0, 1.0];

#[inline(always)]
fn song_select_bg_path(song: &SongData, mode: SelectMusicSongSelectBgMode) -> Option<&PathBuf> {
    match mode {
        SelectMusicSongSelectBgMode::Off => None,
        SelectMusicSongSelectBgMode::Banner => {
            song.banner_path.as_ref().or(song.background_path.as_ref())
        }
        SelectMusicSongSelectBgMode::Bg => {
            song.background_path.as_ref().or(song.banner_path.as_ref())
        }
    }
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: &Path) {
    if !paths.iter().any(|existing| existing.as_path() == path) {
        paths.push(path.to_path_buf());
    }
}

fn visit_visible_song_select_bg_paths(
    entries: &[MusicWheelEntry],
    selected_index: usize,
    position_offset_from_selection: f32,
    mode: SelectMusicSongSelectBgMode,
    mut visit: impl FnMut(&Path) -> bool,
) -> bool {
    if entries.is_empty() || mode == SelectMusicSongSelectBgMode::Off {
        return true;
    }

    let num_entries = entries.len();
    for i_slot in 0..NUM_WHEEL_SLOTS {
        let offset_from_center = i_slot as isize - CENTER_WHEEL_SLOT_INDEX as isize;
        let offset_from_center_f = offset_from_center as f32 + position_offset_from_selection;
        if offset_from_center_f.abs() > WHEEL_DRAW_RADIUS {
            continue;
        }
        let list_index = ((selected_index as isize + offset_from_center + num_entries as isize)
            as usize)
            % num_entries;
        let path = match entries.get(list_index) {
            Some(MusicWheelEntry::PackHeader {
                banner_path: Some(path),
                ..
            }) => Some(path.as_path()),
            Some(MusicWheelEntry::Song(song)) => {
                song_select_bg_path(song, mode).map(PathBuf::as_path)
            }
            _ => None,
        };
        if path.is_some_and(|path| !visit(path)) {
            return false;
        }
    }
    true
}

pub fn visible_song_select_bg_paths(
    entries: &[MusicWheelEntry],
    selected_index: usize,
    position_offset_from_selection: f32,
    mode: SelectMusicSongSelectBgMode,
) -> Vec<PathBuf> {
    if entries.is_empty() || mode == SelectMusicSongSelectBgMode::Off {
        return Vec::new();
    }
    let mut paths = Vec::with_capacity(NUM_WHEEL_ITEMS_TO_DRAW);
    visit_visible_song_select_bg_paths(
        entries,
        selected_index,
        position_offset_from_selection,
        mode,
        |path| {
            push_unique_path(&mut paths, path);
            true
        },
    );
    paths
}

pub fn visible_song_select_bg_paths_match(
    entries: &[MusicWheelEntry],
    selected_index: usize,
    position_offset_from_selection: f32,
    mode: SelectMusicSongSelectBgMode,
    expected: &[PathBuf],
) -> bool {
    let mut matched = 0;
    let completed = visit_visible_song_select_bg_paths(
        entries,
        selected_index,
        position_offset_from_selection,
        mode,
        |path| {
            if expected[..matched]
                .iter()
                .any(|known| known.as_path() == path)
            {
                return true;
            }
            if expected.get(matched).map(PathBuf::as_path) != Some(path) {
                return false;
            }
            matched += 1;
            true
        },
    );
    completed && matched == expected.len()
}

fn song_select_bg_texture_key(
    path: &Path,
    paths: &[PathBuf],
    texture_keys: &[Arc<str>],
    next_index: &mut usize,
) -> Arc<str> {
    if paths.get(*next_index).map(PathBuf::as_path) == Some(path)
        && let Some(key) = texture_keys.get(*next_index)
    {
        *next_index += 1;
        return Arc::clone(key);
    }
    if let Some(index) = paths.iter().position(|known| known.as_path() == path)
        && let Some(key) = texture_keys.get(index)
    {
        if index >= *next_index {
            *next_index = index + 1;
        }
        return Arc::clone(key);
    }
    Arc::from(path.to_string_lossy().as_ref())
}

fn song_select_bg_sprite(
    key: Arc<str>,
    center_x: f32,
    center_y: f32,
    width: f32,
    height: f32,
    alpha: f32,
    fade_left: f32,
) -> Actor {
    let mut actor = act!(sprite(&key):
        align(0.5, 0.5):
        xy(center_x, center_y):
        setsize(width, height):
        diffuse(1.0, 1.0, 1.0, alpha):
        fadeleft(fade_left):
        z(52)
    );
    if let Some(uv) = shared_banner::cover_uv(&key, width, height)
        && let Actor::Sprite { uv_rect, .. } = &mut actor
    {
        *uv_rect = Some(uv);
    }
    actor
}

thread_local! {
    static ITL_RANK_TEXT_CACHE: RefCell<TextCache<u32>> =
        RefCell::new(text_cache_with_capacity(256));
    static ITL_EX_TEXT_CACHE: RefCell<TextCache<u32>> =
        RefCell::new(text_cache_with_capacity(256));
    static ITL_POINTS_TEXT_CACHE: RefCell<TextCache<u32>> =
        RefCell::new(text_cache_with_capacity(256));
    static SRPG_RATE_TEXT_CACHE: RefCell<TextCache<u32>> =
        RefCell::new(text_cache_with_capacity(128));
    static PACK_COUNT_TEXT_CACHE: RefCell<TextCache<usize>> =
        RefCell::new(text_cache_with_capacity(256));
    static STR_REF_CACHE: RefCell<SharedStrCache> =
        RefCell::new(HashMap::with_capacity(1024));
}

const fn col_quint_lamp() -> [f32; 4] {
    // zmod quint color: color("1,0.2,0.406,1")
    [1.0, 0.2, 0.406, 1.0]
}
fn col_clear_lamp() -> [f32; 4] {
    // zmod clear lamp
    const C: [f32; 4] = color::rgba_hex("#0000CC");
    C
}
fn col_fail_lamp() -> [f32; 4] {
    // zmod fail lamp
    const C: [f32; 4] = color::rgba_hex("#990000");
    C
}

fn lamp_judge_count_color(lamp_index: u8) -> [f32; 4] {
    // zmod uses SL.JudgmentColors["FA+"][lamp+1] for the single-digit overlay.
    match lamp_index {
        1 => color::JUDGMENT_FA_PLUS_WHITE_RGBA,
        2 => color::JUDGMENT_RGBA[1],
        3 => color::JUDGMENT_RGBA[2],
        4 => color::JUDGMENT_RGBA[3],
        _ => [1.0; 4],
    }
}

#[inline(always)]
fn digit_text(digit: u8) -> &'static str {
    const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
    DIGITS[digit as usize]
}

#[inline(always)]
fn cached_itl_ex_text(ex_hundredths: u32) -> Arc<str> {
    cached_text(
        &ITL_EX_TEXT_CACHE,
        ex_hundredths,
        ITL_EX_TEXT_CACHE_LIMIT,
        || format!("{}.{:02}", ex_hundredths / 100, ex_hundredths % 100),
    )
}

#[inline(always)]
fn cached_itl_rank_text(rank: u32) -> Arc<str> {
    cached_text(
        &ITL_RANK_TEXT_CACHE,
        rank,
        ITL_RANK_TEXT_CACHE_LIMIT,
        || rank.to_string(),
    )
}

#[inline(always)]
fn cached_itl_points_text(points: u32) -> Arc<str> {
    cached_text(
        &ITL_POINTS_TEXT_CACHE,
        points,
        ITL_POINTS_TEXT_CACHE_LIMIT,
        || points.to_string(),
    )
}

#[inline(always)]
fn cached_srpg_rate_text(rate_hundredths: u32) -> Arc<str> {
    cached_text(
        &SRPG_RATE_TEXT_CACHE,
        rate_hundredths,
        SRPG_RATE_TEXT_CACHE_LIMIT,
        || format!("{}.{:02}", rate_hundredths / 100, rate_hundredths % 100),
    )
}

#[inline(always)]
fn cached_pack_count_text(count: usize) -> Arc<str> {
    cached_text(
        &PACK_COUNT_TEXT_CACHE,
        count,
        PACK_COUNT_TEXT_CACHE_LIMIT,
        || count.to_string(),
    )
}

#[inline(always)]
fn cached_str_ref(text: &str) -> Arc<str> {
    cached_shared_str(&STR_REF_CACHE, text, STR_REF_CACHE_LIMIT)
}

fn song_pack_sync_style(
    song: &SongData,
    prefs: Option<&HashMap<String, SyncPref>>,
    default: DefaultSyncOffset,
) -> Option<DefaultSyncOffset> {
    let prefs = prefs?;
    let pref = song
        .simfile_path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .and_then(|group| prefs.get(group).copied())
        .unwrap_or(SyncPref::Default);
    Some(DefaultSyncOffset::from_sync_pref(resolve_sync_pref(
        pref,
        default.sync_pref(),
    )))
}

#[inline(always)]
fn itl_score_line_y(side: profile_data::PlayerSide, joined_sides: usize) -> (f32, f32) {
    if joined_sides >= 2 {
        return if side == profile_data::PlayerSide::P1 {
            (-15.0, -6.0)
        } else {
            (0.0, 9.0)
        };
    }
    (-7.0, 3.0)
}

#[inline(always)]
fn itl_score_y(side: profile_data::PlayerSide, joined_sides: usize) -> f32 {
    if joined_sides >= 2 {
        if side == profile_data::PlayerSide::P1 {
            -11.0
        } else {
            4.0
        }
    } else {
        -4.0
    }
}

#[inline(always)]
fn choose_itl_wheel_score(
    local_itl: Option<score_data::CachedItlScore>,
    online_ex_hundredths: Option<u32>,
    online_points: Option<u32>,
) -> Option<(u32, Option<u32>)> {
    let ex_hundredths =
        online_ex_hundredths.or_else(|| local_itl.as_ref().map(|score| score.ex_hundredths))?;
    let points = if online_ex_hundredths.is_some() {
        online_points
    } else {
        local_itl.map(|score| score.points)
    };
    Some((ex_hundredths, points))
}

#[inline(always)]
const fn itl_wheel_mode_for_sides(
    mode: SelectMusicItlWheelMode,
    joined_sides: usize,
) -> SelectMusicItlWheelMode {
    match (mode, joined_sides >= 2) {
        (SelectMusicItlWheelMode::PointsAndScore, true) => SelectMusicItlWheelMode::Score,
        _ => mode,
    }
}

#[inline(always)]
pub(crate) const fn itl_fetch_flags(
    allow_online_fetch: bool,
    rank_mode: SelectMusicItlRankMode,
    wheel_mode: SelectMusicItlWheelMode,
    is_srpg_event: bool,
) -> (bool, bool, bool) {
    if !allow_online_fetch {
        return (false, false, false);
    }
    let fetch_rank = matches!(rank_mode, SelectMusicItlRankMode::Chart);
    let fetch_score = matches!(rank_mode, SelectMusicItlRankMode::Overall)
        || (!matches!(wheel_mode, SelectMusicItlWheelMode::Off) && !is_srpg_event);
    let fetch_srpg = !matches!(wheel_mode, SelectMusicItlWheelMode::Off) && is_srpg_event;
    (fetch_rank, fetch_score, fetch_srpg)
}

#[inline(always)]
fn itl_rank_color(rank: u32, is_double_style: bool) -> [f32; 4] {
    let [t1, t2, t3, t4, t5] = if is_double_style {
        [5, 20, 40, 50, 55]
    } else {
        [10, 25, 50, 75, 85]
    };
    if rank <= t1 {
        color::JUDGMENT_RGBA[0]
    } else if rank <= t2 {
        color::JUDGMENT_RGBA[1]
    } else if rank <= t3 {
        color::JUDGMENT_RGBA[2]
    } else if rank <= t4 {
        color::JUDGMENT_RGBA[3]
    } else if rank <= t5 {
        color::JUDGMENT_RGBA[4]
    } else {
        color::JUDGMENT_RGBA[5]
    }
}

#[inline(always)]
fn srpg_rate_color(rate_hundredths: u32, side: profile_data::PlayerSide) -> [f32; 4] {
    let shade = (1.0 - ((rate_hundredths as f32 - 100.0) / 50.0)).clamp(0.0, 1.0);
    if side == profile_data::PlayerSide::P2 {
        [shade, shade, 1.0, 1.0]
    } else {
        [1.0, shade, shade, 1.0]
    }
}

// Helper from select_music.rs
fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        (b[0] - a[0]).mul_add(t, a[0]),
        (b[1] - a[1]).mul_add(t, a[1]),
        (b[2] - a[2]).mul_add(t, a[2]),
        (b[3] - a[3]).mul_add(t, a[3]),
    ]
}

fn chart_for_preferred_or_nearest_standard<'a>(
    song: &'a SongData,
    chart_type: &str,
    preferred_index: usize,
) -> Option<&'a ChartData> {
    let num_standard = STANDARD_DIFFICULTY_COUNT;
    if num_standard == 0 {
        return None;
    }

    let preferred = preferred_index.min(num_standard - 1);
    let preferred_name = STANDARD_DIFFICULTY_NAMES[preferred];
    if let Some(chart) = song.charts.iter().find(|chart| {
        chart.chart_type.eq_ignore_ascii_case(chart_type)
            && chart.difficulty.eq_ignore_ascii_case(preferred_name)
    }) {
        return Some(chart);
    }

    let mut best_chart = None;
    let mut best_distance = usize::MAX;
    for chart in &song.charts {
        if !chart.has_note_data || !chart.chart_type.eq_ignore_ascii_case(chart_type) {
            continue;
        }
        let Some(diff_ix) = STANDARD_DIFFICULTY_NAMES
            .iter()
            .position(|diff| chart.difficulty.eq_ignore_ascii_case(diff))
        else {
            continue;
        };
        let distance = diff_ix.abs_diff(preferred);
        if distance < best_distance {
            best_distance = distance;
            best_chart = Some(chart);
        }
    }
    best_chart
}

/// Build the fixed borrowed slot request shared by Select Music and Select
/// Course. Slot-to-entry and side-to-chart mapping intentionally mirrors the
/// composer so shell-prepared data stays aligned while the wheel animates.
pub(crate) fn runtime_slot_requests<'a>(
    entries: &'a [MusicWheelEntry],
    selected_index: usize,
    selected_charts: [Option<&'a ChartData>; profile_data::PLAYER_SLOTS],
    preferred_difficulty_index: [usize; profile_data::PLAYER_SLOTS],
    play_style: profile_data::PlayStyle,
) -> [MusicWheelSlotRuntimeRequest<'a>; MUSIC_WHEEL_SLOT_COUNT] {
    if entries.is_empty() {
        return [MusicWheelSlotRuntimeRequest::Empty; MUSIC_WHEEL_SLOT_COUNT];
    }
    let target_chart_type = play_style.chart_type();
    std::array::from_fn(|slot| {
        let offset = slot as isize - CENTER_WHEEL_SLOT_INDEX as isize;
        let list_index =
            ((selected_index as isize + offset + entries.len() as isize) as usize) % entries.len();
        match &entries[list_index] {
            MusicWheelEntry::PackHeader { pack_key, .. } => MusicWheelSlotRuntimeRequest::Pack {
                key: pack_key.as_deref(),
            },
            MusicWheelEntry::Song(song) => {
                let charts = if slot == CENTER_WHEEL_SLOT_INDEX {
                    selected_charts
                } else if preferred_difficulty_index[0] == preferred_difficulty_index[1] {
                    let chart = chart_for_preferred_or_nearest_standard(
                        song,
                        target_chart_type,
                        preferred_difficulty_index[0],
                    );
                    [chart, chart]
                } else {
                    [
                        chart_for_preferred_or_nearest_standard(
                            song,
                            target_chart_type,
                            preferred_difficulty_index[0],
                        ),
                        chart_for_preferred_or_nearest_standard(
                            song,
                            target_chart_type,
                            preferred_difficulty_index[1],
                        ),
                    ]
                };
                let chart_hashes = std::array::from_fn(|side_index| {
                    let side = if side_index == 0 {
                        profile_data::PlayerSide::P1
                    } else {
                        profile_data::PlayerSide::P2
                    };
                    charts[profile_data::runtime_player_index(play_style, side)]
                        .map(|chart| chart.short_hash.as_str())
                });
                MusicWheelSlotRuntimeRequest::Song {
                    song,
                    chart_hashes,
                    is_srpg_event: is_srpg_event_song(song),
                }
            }
        }
    })
}

pub struct MusicWheelParams<'a> {
    pub machine_font: MachineFont,
    pub entries: &'a [MusicWheelEntry],
    pub selected_index: usize,
    pub position_offset_from_selection: f32,
    pub selection_animation_timer: f32,
    pub selection_animation_beat: f32,
    pub color_pack_headers: bool,
    pub selected_charts: [Option<&'a ChartData>; profile_data::PLAYER_SLOTS],
    pub preferred_difficulty_index: [usize; profile_data::PLAYER_SLOTS],
    pub song_box_color: Option<[f32; 4]>,
    pub song_text_color: Option<[f32; 4]>,
    pub song_text_color_overrides: Option<&'a HashMap<usize, [f32; 4]>>,
    pub song_has_edit_ptrs: Option<&'a HashSet<usize>>,
    pub show_music_wheel_grades: bool,
    pub show_music_wheel_lamps: bool,
    pub itl_rank_mode: SelectMusicItlRankMode,
    pub itl_wheel_mode: SelectMusicItlWheelMode,
    pub song_select_bg_mode: SelectMusicSongSelectBgMode,
    pub song_select_bg_paths: &'a [PathBuf],
    pub song_select_bg_texture_keys: &'a [Arc<str>],
    pub expanded_series_name: Option<&'a str>,
    pub expanded_pack_name: Option<&'a str>,
    pub new_pack_names: Option<&'a HashSet<String>>,
    pub pack_sync_prefs: Option<&'a HashMap<String, SyncPref>>,
    pub default_sync_offset: DefaultSyncOffset,
    pub runtime: &'a MusicWheelRuntimeView,
}

pub fn push(actors: &mut Vec<Actor>, p: MusicWheelParams) {
    actors.reserve(WHEEL_ACTOR_CAPACITY);
    let translated_titles = p.runtime.translated_titles;
    let song_bg_alpha = if p.runtime.song_bg_dimmed { 0.5 } else { 1.0 };
    let section_bg_alpha = if p.runtime.section_bg_dimmed {
        0.5
    } else {
        1.0
    };
    let play_style = p.runtime.play_style;
    let target_chart_type = play_style.chart_type();
    let [p1_joined, p2_joined] = p.runtime.joined;
    let side_joined = |side: profile_data::PlayerSide| match side {
        profile_data::PlayerSide::P1 => p1_joined,
        profile_data::PlayerSide::P2 => p2_joined,
    };
    let mut song_box_color = p.song_box_color.unwrap_or_else(col_music_wheel_box);
    song_box_color[3] *= song_bg_alpha;
    let default_song_text_color = p.song_text_color.unwrap_or([1.0, 1.0, 1.0, 1.0]);

    const WHEEL_WIDTH_DIVISOR: f32 = 2.125;
    let num_visible_items = NUM_VISIBLE_WHEEL_ITEMS;

    // SL metrics-derived values
    let sl_shift = widescale(28.0, 33.0); // InitCommand shift in SL
    let highlight_w: f32 = screen_width() / WHEEL_WIDTH_DIVISOR; // _screen.w/2.125
    let highlight_left_world: f32 = screen_center_x() + sl_shift; // left edge of the column
    let half_highlight: f32 = 0.5 * highlight_w;

    // Local Xs (container is LEFT-anchored at highlight_left_world)
    // In SL, titles are WideScale(75,111) from wheel center (no +sl_shift); cancel the container shift here.
    let title_x_local: f32 = widescale(75.0, 111.0) - sl_shift;
    let title_max_w_local: f32 = widescale(245.0, 350.0);

    // Simply Love [MusicWheelItem] section metrics. Child pack sections are
    // indented 10px from their parent Series sections.
    let section_x_local: f32 = widescale(35.0, 74.0) - sl_shift;
    let child_section_x_local: f32 = widescale(45.0, 84.0) - sl_shift;
    let empty_center_x_local: f32 = half_highlight - sl_shift + widescale(9.0, 10.0);
    let pack_name_max_w: f32 = widescale(240.0, 310.0);

    // Pack count
    let pack_count_x_local: f32 = screen_width() / 2.0 - widescale(9.0, 10.0) - sl_shift;

    // "Has Edit" icon (Simply Love: Graphics/MusicWheelItem Song NormalPart/default.lua)
    let badge_right_x_local: f32 = screen_width() / widescale(2.15, 2.14) - 8.0;
    let badge_gap_x: f32 = widescale(18.0, 24.0);

    // --- VERTICAL GEOMETRY (1:1 with Simply Love Lua) ---
    let slot_spacing: f32 = screen_height() / (num_visible_items as f32);
    let item_h_full: f32 = slot_spacing;
    let item_h_colored: f32 = slot_spacing - 1.0;
    let center_y: f32 = screen_center_y();
    let line_gap_units: f32 = 6.0;

    // Selection pulse (Simply Love [MusicWheel] HighlightOnCommand):
    // diffuseshift + effectclock("beatnooffset") + effectperiod(2)
    let highlight_phase =
        (p.selection_animation_beat / SELECTION_HIGHLIGHT_BEAT_PERIOD) * std::f32::consts::PI * 2.0;
    let anim_t = f32::midpoint(highlight_phase.cos(), 1.0);

    let lamp_pulse_t_unscaled =
        (p.selection_animation_timer / LAMP_PULSE_PERIOD) * std::f32::consts::PI * 2.0;
    let lamp_pulse_t = f32::midpoint(lamp_pulse_t_unscaled.sin(), 1.0);
    let grade_zoom = widescale(0.18, 0.3);
    let grade_x_p1 = widescale(10.0, 17.0);
    let grade_x_p2 = widescale(26.0, 47.0);
    let itl_rank_zoom = widescale(0.2, 0.3);
    let srpg_rate_zoom = widescale(0.15, 0.2);
    let itl_ex_x = screen_width() / widescale(2.15, 2.14) - 40.0;
    let itl_ex_color = color::JUDGMENT_RGBA[0];
    let srpg_score_color = [1.0, 1.0, 1.0, 1.0];
    let itl_points_color = [1.0, 1.0, 1.0, 1.0];
    let joined_sides = usize::from(p1_joined) + usize::from(p2_joined);
    let itl_wheel_mode = itl_wheel_mode_for_sides(p.itl_wheel_mode, joined_sides);
    let is_double_style = matches!(play_style, profile_data::PlayStyle::Double);

    let header_font = machine_font_key(p.machine_font, FontRole::Header);
    let numbers_font = machine_font_key(p.machine_font, FontRole::Numbers);
    let screen_eval_font = machine_font_key(p.machine_font, FontRole::ScreenEval);

    let num_entries = p.entries.len();
    let mut song_select_bg_key_index = 0;

    if num_entries > 0 {
        for i_slot in 0..NUM_WHEEL_SLOTS {
            let offset_from_center = i_slot as isize - CENTER_WHEEL_SLOT_INDEX as isize;
            let offset_from_center_f = offset_from_center as f32 + p.position_offset_from_selection;
            if offset_from_center_f.abs() > WHEEL_DRAW_RADIUS {
                continue;
            }
            let y_center_item = offset_from_center_f.mul_add(slot_spacing, center_y);
            let is_selected_slot = i_slot == CENTER_WHEEL_SLOT_INDEX;

            // The selected_index from the state now freely increments/decrements. We use it as a base
            // and apply the modulo here for safe list access.
            let list_index =
                ((p.selected_index as isize + offset_from_center + num_entries as isize) as usize)
                    % num_entries;

            let Some(entry) = p.entries.get(list_index) else {
                continue;
            };
            let runtime_slot = &p.runtime.slots[i_slot];
            let runtime_for_side = |side: profile_data::PlayerSide| {
                &runtime_slot.sides[profile_data::player_side_index(side)]
            };

            match entry {
                MusicWheelEntry::PackHeader {
                    name,
                    original_index,
                    banner_path,
                    song_count,
                    pack_key,
                    parent_series,
                } => {
                    let is_series_header = pack_key.is_none() && parent_series.is_some();
                    let is_child_pack = pack_key.is_some() && parent_series.is_some();
                    let mut bg_col = col_pack_header_box();
                    bg_col[3] *= section_bg_alpha;
                    let header_color = if p.color_pack_headers {
                        color::simply_love_rgba(*original_index as i32)
                    } else {
                        [1.0, 1.0, 1.0, 1.0]
                    };
                    let show_new_badge = pack_key.is_some()
                        && p.color_pack_headers
                        && p.new_pack_names
                            .is_some_and(|new_packs| new_packs.contains(name.as_str()));
                    actors.push(act!(quad:
                        align(0.0, 0.5):
                        xy(highlight_left_world, y_center_item):
                        zoomto(highlight_w, item_h_full):
                        diffuse(0.0, 0.0, 0.0, section_bg_alpha):
                        z(51)
                    ));
                    actors.push(act!(quad:
                        align(0.0, 0.5):
                        xy(highlight_left_world, y_center_item):
                        zoomto(highlight_w, item_h_colored):
                        diffuse(bg_col[0], bg_col[1], bg_col[2], bg_col[3]):
                        z(52)
                    ));
                    if p.song_select_bg_mode != SelectMusicSongSelectBgMode::Off
                        && let Some(path) = banner_path.as_ref()
                    {
                        let active = if is_series_header {
                            p.expanded_series_name
                                .is_some_and(|expanded| expanded == name.as_str())
                        } else {
                            p.expanded_pack_name
                                .is_some_and(|expanded| expanded == name.as_str())
                        };
                        let alpha = if active { 0.5 } else { 0.1 };
                        actors.push(song_select_bg_sprite(
                            song_select_bg_texture_key(
                                path,
                                p.song_select_bg_paths,
                                p.song_select_bg_texture_keys,
                                &mut song_select_bg_key_index,
                            ),
                            highlight_left_world + half_highlight,
                            y_center_item,
                            highlight_w,
                            item_h_full,
                            alpha,
                            0.1,
                        ));
                    }
                    let folder_frame_x = if is_child_pack { 8.0 } else { -3.0 };
                    let folder_mid_x =
                        highlight_left_world + folder_frame_x + FOLDER_NATIVE_WIDTH * FOLDER_ZOOM
                            - 8.0;
                    if is_series_header {
                        let expanded = p
                            .expanded_series_name
                            .is_some_and(|expanded| expanded == name.as_str());
                        let [back, mid, front] = if expanded {
                            SERIES_FOLDER_EXPANDED
                        } else {
                            SERIES_FOLDER_COLLAPSED
                        };
                        for (x, y, tint) in [
                            (folder_mid_x - 4.0, 0.0, back),
                            (folder_mid_x, 1.0, mid),
                            (folder_mid_x + 4.0, 2.0, front),
                        ] {
                            actors.push(act!(sprite("folder-solid.png"):
                                align(0.0, 0.5):
                                xy(x, y_center_item + y):
                                zoom(FOLDER_ZOOM):
                                diffuse(tint[0], tint[1], tint[2], tint[3]):
                                z(53)
                            ));
                        }
                    } else {
                        actors.push(act!(sprite("folder-solid.png"):
                            align(0.0, 0.5):
                            xy(folder_mid_x, y_center_item + 1.0):
                            zoom(FOLDER_ZOOM):
                            diffuse(header_color[0], header_color[1], header_color[2], header_color[3]):
                            z(53)
                        ));
                    }
                    actors.push(act!(text:
                        font("miso"):
                        settext(cached_str_ref(name.as_str())):
                        align(0.0, 0.5):
                        xy(
                            highlight_left_world
                                + if is_child_pack { child_section_x_local } else { section_x_local },
                            y_center_item
                        ):
                        maxwidth(pack_name_max_w):
                        zoom(1.0):
                        diffuse(header_color[0], header_color[1], header_color[2], 1.0):
                        z(53)
                    ));
                    if show_new_badge {
                        let phase = (p.selection_animation_timer / NEW_BADGE_PULSE_PERIOD)
                            * std::f32::consts::PI
                            * 2.0;
                        let pulse_t = f32::midpoint(phase.sin(), 1.0);
                        let color = lerp_color(NEW_BADGE_COLOR, NEW_BADGE_COLOR_PEAK, pulse_t);
                        actors.push(act!(text:
                            font("miso"):
                            settext("NEW"):
                            align(1.0, 0.5):
                            xy(highlight_left_world + pack_count_x_local - widescale(30.0, 40.0), y_center_item):
                            zoom(0.6):
                            diffuse(color[0], color[1], color[2], color[3]):
                            z(53)
                        ));
                    }
                    if *song_count > 0 {
                        actors.push(act!(text:
                            font("miso"):
                            settext(cached_pack_count_text(*song_count)):
                            align(1.0, 0.5):
                            xy(highlight_left_world + pack_count_x_local, y_center_item):
                            zoom(0.75):
                            horizalign(right):
                            diffuse(1.0, 1.0, 1.0, 1.0):
                            z(53)
                        ));
                    }

                    // Favorite heart icon on favorited pack headers — mirrors
                    // the song-row heart so the player can spot favorited packs
                    // while scrolling Group sort.
                    if pack_key.is_some() {
                        let p1_fav =
                            p1_joined && runtime_for_side(profile_data::PlayerSide::P1).favorite;
                        let p2_fav =
                            p2_joined && runtime_for_side(profile_data::PlayerSide::P2).favorite;
                        let both_joined = p1_joined && p2_joined;
                        let heart_x = -23.0_f32;
                        let heart_pulse_t = {
                            let t = (p.selection_animation_timer / HEART_PULSE_PERIOD).fract();
                            (t * std::f32::consts::TAU).sin() * 0.5 + 0.5
                        };
                        if p1_fav {
                            let heart_y = if both_joined { -6.0 } else { 0.0 };
                            let col =
                                lerp_color(HEART_COLOR_P1, [1.0, 1.0, 1.0, 1.0], heart_pulse_t);
                            let zm = if both_joined {
                                HEART_ZOOM_DUAL
                            } else {
                                HEART_ZOOM_SINGLE
                            };
                            actors.push(act!(sprite("fave-icon.png"):
                                align(0.5, 0.5):
                                xy(highlight_left_world + heart_x, y_center_item + heart_y):
                                zoom(zm):
                                diffuse(col[0], col[1], col[2], col[3]):
                                z(54)
                            ));
                        }
                        if p2_fav {
                            let heart_y = if both_joined { 6.0 } else { 0.0 };
                            let col =
                                lerp_color(HEART_COLOR_P2, [1.0, 1.0, 1.0, 1.0], heart_pulse_t);
                            let zm = if both_joined {
                                HEART_ZOOM_DUAL
                            } else {
                                HEART_ZOOM_SINGLE
                            };
                            actors.push(act!(sprite("fave-icon.png"):
                                align(0.5, 0.5):
                                xy(highlight_left_world + heart_x, y_center_item + heart_y):
                                zoom(zm):
                                diffuse(col[0], col[1], col[2], col[3]):
                                z(54)
                            ));
                        }
                    }

                    continue;
                }
                MusicWheelEntry::Song(info) => {
                    let song_ptr = std::sync::Arc::as_ptr(info) as usize;
                    let txt_col = p
                        .song_text_color_overrides
                        .and_then(|m| m.get(&song_ptr).copied())
                        .unwrap_or(default_song_text_color);
                    let title = info.display_title(translated_titles);
                    let subtitle = info.display_subtitle(translated_titles);
                    let has_subtitle = !subtitle.trim().is_empty();
                    let has_edit = if let Some(cached) = p.song_has_edit_ptrs {
                        cached.contains(&song_ptr)
                    } else {
                        info.charts.iter().any(|c| {
                            c.chart_type.eq_ignore_ascii_case(target_chart_type)
                                && c.difficulty.eq_ignore_ascii_case("edit")
                        })
                    };
                    let wheel_charts: [Option<&ChartData>; profile_data::PLAYER_SLOTS] =
                        if is_selected_slot {
                            p.selected_charts
                        } else if p.preferred_difficulty_index[0] == p.preferred_difficulty_index[1]
                        {
                            // Both sides request the same preferred difficulty,
                            // so the per-side chart scan is identical. Resolve
                            // once and reuse instead of scanning the chart list
                            // twice. (&ChartData is Copy.)
                            let chart = chart_for_preferred_or_nearest_standard(
                                info,
                                target_chart_type,
                                p.preferred_difficulty_index[0],
                            );
                            [chart, chart]
                        } else {
                            [
                                chart_for_preferred_or_nearest_standard(
                                    info,
                                    target_chart_type,
                                    p.preferred_difficulty_index[0],
                                ),
                                chart_for_preferred_or_nearest_standard(
                                    info,
                                    target_chart_type,
                                    p.preferred_difficulty_index[1],
                                ),
                            ]
                        };
                    let wheel_chart_for_side = |side: profile_data::PlayerSide| {
                        wheel_charts[profile_data::runtime_player_index(play_style, side)]
                    };
                    let has_lua = info.has_lua;
                    let lua_submit_allowed = has_lua
                        && if joined_sides == 0 {
                            wheel_chart_for_side(profile_data::PlayerSide::P1).is_some_and(
                                |chart| {
                                    score_data::lua_chart_submit_allowed(chart.short_hash.as_str())
                                },
                            )
                        } else {
                            [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2]
                                .iter()
                                .copied()
                                .any(|side| {
                                    side_joined(side)
                                        && wheel_chart_for_side(side).is_some_and(|chart| {
                                            score_data::lua_chart_submit_allowed(
                                                chart.short_hash.as_str(),
                                            )
                                        })
                                })
                        };
                    actors.push(act!(quad:
                        align(0.0, 0.5):
                        xy(highlight_left_world, y_center_item):
                        zoomto(highlight_w, item_h_full):
                        diffuse(0.0, 10.0 / 255.0, 17.0 / 255.0, 0.5):
                        z(51)
                    ));
                    actors.push(act!(quad:
                        align(0.0, 0.5):
                        xy(highlight_left_world, y_center_item):
                        zoomto(highlight_w, item_h_colored):
                        diffuse(song_box_color[0], song_box_color[1], song_box_color[2], song_box_color[3]):
                        z(52)
                    ));
                    if let Some(path) = song_select_bg_path(info, p.song_select_bg_mode) {
                        let art_w = (highlight_w - 50.0).max(1.0);
                        actors.push(song_select_bg_sprite(
                            song_select_bg_texture_key(
                                path,
                                p.song_select_bg_paths,
                                p.song_select_bg_texture_keys,
                                &mut song_select_bg_key_index,
                            ),
                            highlight_left_world + highlight_w - art_w * 0.5,
                            y_center_item,
                            art_w,
                            (item_h_full - 2.0).max(1.0),
                            0.25,
                            1.0,
                        ));
                    }
                    if song_pack_sync_style(info, p.pack_sync_prefs, p.default_sync_offset)
                        == Some(DefaultSyncOffset::Null)
                    {
                        actors.push(act!(quad:
                            align(0.0, 0.5):
                            xy(highlight_left_world, y_center_item):
                            zoomto(highlight_w, item_h_colored):
                            diffuse(SONG_NULL_SYNC_RIGHT_EDGE[0], SONG_NULL_SYNC_RIGHT_EDGE[1], SONG_NULL_SYNC_RIGHT_EDGE[2], SONG_NULL_SYNC_RIGHT_EDGE[3] * song_bg_alpha):
                            fadeleft(1.0):
                            z(52)
                        ));
                    }

                    let subtitle_y_offset = if has_subtitle { -line_gap_units } else { 0.0 };
                    actors.push(act!(text:
                        font("miso"):
                        settext(cached_str_ref(title)):
                        align(0.0, 0.5):
                        xy(highlight_left_world + title_x_local, y_center_item + subtitle_y_offset):
                        maxwidth(title_max_w_local):
                        zoom(0.85):
                        diffuse(txt_col[0], txt_col[1], txt_col[2], txt_col[3]):
                        z(53)
                    ));
                    if has_subtitle {
                        actors.push(act!(text:
                            font("miso"):
                            settext(cached_str_ref(subtitle)):
                            align(0.0, 0.5):
                            xy(highlight_left_world + title_x_local, y_center_item + line_gap_units):
                            maxwidth(title_max_w_local):
                            zoom(0.7):
                            diffuse(txt_col[0], txt_col[1], txt_col[2], txt_col[3]):
                            z(53)
                        ));
                    }
                    if has_lua {
                        let lua_x = if has_edit {
                            badge_right_x_local - badge_gap_x
                        } else {
                            badge_right_x_local
                        };
                        if lua_submit_allowed {
                            actors.push(act!(sprite("GrooveStats.png"):
                                align(1.0, 0.5):
                                xy(highlight_left_world + lua_x, y_center_item):
                                zoom(WHEEL_BADGE_ZOOM):
                                z(53)
                            ));
                        }
                        actors.push(act!(sprite("has_lua.png"):
                            align(1.0, 0.5):
                            xy(highlight_left_world + lua_x, y_center_item):
                            zoom(WHEEL_BADGE_ZOOM):
                            z(54)
                        ));
                    }
                    if has_edit {
                        actors.push(act!(sprite("has_edit.png"):
                            align(1.0, 0.5):
                            xy(highlight_left_world + badge_right_x_local, y_center_item):
                            zoom(WHEEL_BADGE_ZOOM):
                            z(53)
                        ));
                    }
                    if p.show_music_wheel_grades || p.show_music_wheel_lamps {
                        for (side, grade_x) in [
                            (profile_data::PlayerSide::P1, grade_x_p1),
                            (profile_data::PlayerSide::P2, grade_x_p2),
                        ] {
                            if !side_joined(side) {
                                continue;
                            }
                            let Some(cached_score) = runtime_for_side(side).score else {
                                continue;
                            };
                            let has_score = cached_score.grade != score_data::Grade::Failed
                                || cached_score.score_percent > 0.0;
                            if !has_score {
                                continue;
                            }

                            if p.show_music_wheel_grades {
                                let mut grade_actor = act!(sprite("grades/grades 1x19.png"):
                                    align(0.5, 0.5):
                                    xy(highlight_left_world + grade_x, y_center_item):
                                    zoom(grade_zoom):
                                    z(53):
                                    visible(true)
                                );
                                if let Actor::Sprite { cell, .. } = &mut grade_actor {
                                    *cell = Some((cached_score.grade.to_sprite_state(), u32::MAX));
                                }
                                actors.push(grade_actor);
                            }

                            if p.show_music_wheel_lamps {
                                let lamp_dir = if side == profile_data::PlayerSide::P1 {
                                    -1.0
                                } else {
                                    1.0
                                };
                                let lamp_x = grade_x + lamp_dir * widescale(13.0, 20.0);
                                let lamp_w = widescale(5.0, 6.0);
                                let lamp_h = 31.0;
                                let (lamp_color, lamp_pulsing, lamp_index) =
                                    match cached_score.lamp_index {
                                        Some(0) => (col_quint_lamp(), true, Some(0u8)),
                                        Some(idx @ 1..=4) => {
                                            let color_index = (idx - 1) as usize;
                                            let base = color::JUDGMENT_RGBA[color_index.min(5)];
                                            (base, true, Some(idx))
                                        }
                                        Some(_) => (col_clear_lamp(), false, None),
                                        None if cached_score.grade == score_data::Grade::Failed => {
                                            (col_fail_lamp(), false, None)
                                        }
                                        None => (col_clear_lamp(), false, None),
                                    };
                                let lamp_color_final = if lamp_pulsing {
                                    let lamp_color2 =
                                        lerp_color([1.0; 4], lamp_color, LAMP_PULSE_LERP_TO_WHITE);
                                    lerp_color(lamp_color, lamp_color2, lamp_pulse_t)
                                } else {
                                    lamp_color
                                };
                                actors.push(act!(quad:
                                    align(0.5, 0.5):
                                    xy(highlight_left_world + lamp_x, y_center_item):
                                    zoomto(lamp_w, lamp_h):
                                    diffuse(lamp_color_final[0], lamp_color_final[1], lamp_color_final[2], lamp_color_final[3]):
                                    z(53)
                                ));
                                if let Some(lamp_index) = lamp_index
                                    && let Some(count) = cached_score.lamp_judge_count
                                    && count < 10
                                {
                                    let judge_x = grade_x + lamp_dir * widescale(7.0, 13.0);
                                    let judge_col = lamp_judge_count_color(lamp_index);
                                    actors.push(act!(text:
                                        font(screen_eval_font):
                                        settext(digit_text(count)):
                                        align(0.5, 0.5):
                                        horizalign(center):
                                        xy(highlight_left_world + judge_x, y_center_item + 10.0):
                                        zoom(0.15):
                                        diffuse(judge_col[0], judge_col[1], judge_col[2], judge_col[3]):
                                        z(61)
                                    ));
                                }
                            }
                        }
                    }

                    if !matches!(p.itl_rank_mode, SelectMusicItlRankMode::None) && joined_sides == 1
                    {
                        for (side, rank_x) in [
                            (profile_data::PlayerSide::P1, grade_x_p2),
                            (profile_data::PlayerSide::P2, grade_x_p1),
                        ] {
                            let side_joined = match side {
                                profile_data::PlayerSide::P1 => p1_joined,
                                profile_data::PlayerSide::P2 => p2_joined,
                            };
                            if !side_joined {
                                continue;
                            }
                            let Some(rank) = runtime_for_side(side).itl_rank else {
                                continue;
                            };
                            let rank_color = itl_rank_color(rank, is_double_style);
                            actors.push(act!(text:
                                font(header_font):
                                settext(cached_itl_rank_text(rank)):
                                align(0.5, 0.5):
                                horizalign(center):
                                xy(highlight_left_world + rank_x, y_center_item):
                                zoom(itl_rank_zoom):
                                diffuse(rank_color[0], rank_color[1], rank_color[2], rank_color[3]):
                                z(53)
                            ));
                        }
                    }

                    let is_srpg_event = is_srpg_event_song(info);
                    if is_srpg_event && joined_sides == 1 {
                        for (side, rate_x) in [
                            (profile_data::PlayerSide::P1, grade_x_p2),
                            (profile_data::PlayerSide::P2, grade_x_p1),
                        ] {
                            let side_joined = match side {
                                profile_data::PlayerSide::P1 => p1_joined,
                                profile_data::PlayerSide::P2 => p2_joined,
                            };
                            if !side_joined {
                                continue;
                            }
                            let Some(rate_hundredths) =
                                runtime_for_side(side).srpg_pass_rate_hundredths
                            else {
                                continue;
                            };
                            let rate_color = srpg_rate_color(rate_hundredths, side);
                            actors.push(act!(text:
                                font(numbers_font):
                                settext(cached_srpg_rate_text(rate_hundredths)):
                                align(0.5, 0.5):
                                horizalign(center):
                                xy(highlight_left_world + rate_x, y_center_item):
                                zoom(srpg_rate_zoom):
                                diffuse(rate_color[0], rate_color[1], rate_color[2], rate_color[3]):
                                z(53)
                            ));
                        }
                    }

                    for side in [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2] {
                        if matches!(itl_wheel_mode, SelectMusicItlWheelMode::Off) {
                            continue;
                        }
                        if is_srpg_event {
                            let Some(score_hundredths) =
                                runtime_for_side(side).srpg_itl_ex_hundredths
                            else {
                                continue;
                            };
                            actors.push(act!(text:
                                font(numbers_font):
                                settext(cached_itl_ex_text(score_hundredths)):
                                align(1.0, 0.5):
                                horizalign(right):
                                xy(highlight_left_world + itl_ex_x, y_center_item + itl_score_y(side, joined_sides)):
                                zoom(ITL_SCORE_ZOOM):
                                diffuse(srpg_score_color[0], srpg_score_color[1], srpg_score_color[2], srpg_score_color[3]):
                                z(53)
                            ));
                            continue;
                        }
                        let runtime = runtime_for_side(side);
                        let Some((ex_hundredths, points)) = choose_itl_wheel_score(
                            runtime.local_itl,
                            runtime.online_itl_ex_hundredths,
                            runtime.online_itl_points,
                        ) else {
                            continue;
                        };
                        match itl_wheel_mode {
                            SelectMusicItlWheelMode::Off => {}
                            SelectMusicItlWheelMode::Score => {
                                actors.push(act!(text:
                                    font(numbers_font):
                                    settext(cached_itl_ex_text(ex_hundredths)):
                                    align(1.0, 0.5):
                                    horizalign(right):
                                    xy(highlight_left_world + itl_ex_x, y_center_item + itl_score_y(side, joined_sides)):
                                    zoom(ITL_SCORE_ZOOM):
                                    diffuse(itl_ex_color[0], itl_ex_color[1], itl_ex_color[2], itl_ex_color[3]):
                                    z(53)
                                ));
                            }
                            SelectMusicItlWheelMode::PointsAndScore => {
                                let Some(points) = points else {
                                    actors.push(act!(text:
                                        font(numbers_font):
                                        settext(cached_itl_ex_text(ex_hundredths)):
                                        align(1.0, 0.5):
                                        horizalign(right):
                                        xy(highlight_left_world + itl_ex_x, y_center_item + itl_score_y(side, joined_sides)):
                                        zoom(ITL_SCORE_ZOOM):
                                        diffuse(itl_ex_color[0], itl_ex_color[1], itl_ex_color[2], itl_ex_color[3]):
                                        z(53)
                                    ));
                                    continue;
                                };
                                let (points_y, ex_y) = itl_score_line_y(side, joined_sides);
                                actors.push(act!(text:
                                    font(numbers_font):
                                    settext(cached_itl_points_text(points)):
                                    align(1.0, 0.5):
                                    horizalign(right):
                                    xy(highlight_left_world + itl_ex_x, y_center_item + points_y):
                                    zoom(ITL_POINTS_SCORE_ZOOM):
                                    diffuse(
                                        itl_points_color[0],
                                        itl_points_color[1],
                                        itl_points_color[2],
                                        itl_points_color[3]
                                    ):
                                    z(53)
                                ));
                                actors.push(act!(text:
                                    font(numbers_font):
                                    settext(cached_itl_ex_text(ex_hundredths)):
                                    align(1.0, 0.5):
                                    horizalign(right):
                                    xy(highlight_left_world + itl_ex_x, y_center_item + ex_y):
                                    zoom(ITL_POINTS_SCORE_ZOOM):
                                    diffuse(itl_ex_color[0], itl_ex_color[1], itl_ex_color[2], itl_ex_color[3]):
                                    z(53)
                                ));
                            }
                        }
                    }

                    // Favorite heart icon
                    {
                        let p1_fav =
                            p1_joined && runtime_for_side(profile_data::PlayerSide::P1).favorite;
                        let p2_fav =
                            p2_joined && runtime_for_side(profile_data::PlayerSide::P2).favorite;
                        let both_joined = p1_joined && p2_joined;
                        let heart_x = -23.0_f32;
                        let heart_pulse_t = {
                            let t = (p.selection_animation_timer / HEART_PULSE_PERIOD).fract();
                            (t * std::f32::consts::TAU).sin() * 0.5 + 0.5
                        };
                        if p1_fav {
                            let heart_y = if both_joined { -6.0 } else { 0.0 };
                            let col =
                                lerp_color(HEART_COLOR_P1, [1.0, 1.0, 1.0, 1.0], heart_pulse_t);
                            let zm = if both_joined {
                                HEART_ZOOM_DUAL
                            } else {
                                HEART_ZOOM_SINGLE
                            };
                            actors.push(act!(sprite("fave-icon.png"):
                                align(0.5, 0.5):
                                xy(highlight_left_world + heart_x, y_center_item + heart_y):
                                zoom(zm):
                                diffuse(col[0], col[1], col[2], col[3]):
                                z(54)
                            ));
                        }
                        if p2_fav {
                            let heart_y = if both_joined { 6.0 } else { 0.0 };
                            let col =
                                lerp_color(HEART_COLOR_P2, [1.0, 1.0, 1.0, 1.0], heart_pulse_t);
                            let zm = if both_joined {
                                HEART_ZOOM_DUAL
                            } else {
                                HEART_ZOOM_SINGLE
                            };
                            actors.push(act!(sprite("fave-icon.png"):
                                align(0.5, 0.5):
                                xy(highlight_left_world + heart_x, y_center_item + heart_y):
                                zoom(zm):
                                diffuse(col[0], col[1], col[2], col[3]):
                                z(54)
                            ));
                        }
                    }

                    // ITL unlocks lock icon (per-player)
                    {
                        let both_joined = p1_joined && p2_joined;
                        if p1_joined || p2_joined {
                            let p1_locked =
                                p1_joined && runtime_for_side(profile_data::PlayerSide::P1).locked;
                            let p2_locked =
                                p2_joined && runtime_for_side(profile_data::PlayerSide::P2).locked;
                            let lock_x = -12.0_f32;
                            if p1_locked {
                                let lock_y = if both_joined { -8.0 } else { 0.0 };
                                let zm = if both_joined {
                                    LOCK_ZOOM_DUAL
                                } else {
                                    LOCK_ZOOM_SINGLE
                                };
                                let c = LOCK_COLOR_P1;
                                actors.push(act!(sprite("lock.png"):
                                    align(0.5, 0.5):
                                    xy(highlight_left_world + lock_x, y_center_item + lock_y):
                                    zoom(zm):
                                    diffuse(c[0], c[1], c[2], c[3]):
                                    z(54)
                                ));
                            }
                            if p2_locked {
                                let lock_y = if both_joined { 8.0 } else { 0.0 };
                                let zm = if both_joined {
                                    LOCK_ZOOM_DUAL
                                } else {
                                    LOCK_ZOOM_SINGLE
                                };
                                let c = LOCK_COLOR_P2;
                                actors.push(act!(sprite("lock.png"):
                                    align(0.5, 0.5):
                                    xy(highlight_left_world + lock_x, y_center_item + lock_y):
                                    zoom(zm):
                                    diffuse(c[0], c[1], c[2], c[3]):
                                    z(54)
                                ));
                            }
                        }
                    }
                    continue;
                }
            }
        }
    } else {
        // Handle the case where there are no songs or packs loaded.
        let empty_text = "- EMPTY -";
        let text_color = color::decorative_rgba(0); // Red

        for i_slot in 0..NUM_WHEEL_SLOTS {
            let offset_from_center = i_slot as isize - CENTER_WHEEL_SLOT_INDEX as isize;
            let offset_from_center_f = offset_from_center as f32 + p.position_offset_from_selection;
            if offset_from_center_f.abs() > WHEEL_DRAW_RADIUS {
                continue;
            }
            let y_center_item = offset_from_center_f.mul_add(slot_spacing, center_y);

            // Use pack header colors for the empty state
            let mut bg_col = col_pack_header_box();
            bg_col[3] *= section_bg_alpha;

            // Add black background for 1px gap effect, just like real pack headers
            actors.push(act!(quad:
                align(0.0, 0.5):
                xy(highlight_left_world, y_center_item):
                zoomto(highlight_w, item_h_full):
                diffuse(0.0, 0.0, 0.0, section_bg_alpha):
                z(51)
            ));

            // Colored (gray) quad background for the slot
            actors.push(act!(quad:
                align(0.0, 0.5):
                xy(highlight_left_world, y_center_item):
                zoomto(highlight_w, item_h_colored):
                diffuse(bg_col[0], bg_col[1], bg_col[2], bg_col[3]):
                z(52)
            ));

            // "- EMPTY -" text, centered like a pack header
            actors.push(act!(text:
                font("miso"):
                settext(empty_text):
                align(0.5, 0.5):
                xy(highlight_left_world + empty_center_x_local, y_center_item):
                maxwidth(pack_name_max_w):
                zoom(1.0):
                diffuse(text_color[0], text_color[1], text_color[2], text_color[3]):
                z(53)
            ));
        }
    }

    // Selection highlight overlay (Simply Love: Graphics/MusicWheel highlight.lua + [MusicWheel] HighlightOnCommand)
    let selected_runtime = &p.runtime.slots[CENTER_WHEEL_SLOT_INDEX];
    let selected_is_favorite = (p1_joined && selected_runtime.sides[0].favorite)
        || (p2_joined && selected_runtime.sides[1].favorite);
    let highlight_c1: [f32; 4] = if selected_is_favorite {
        [1.0, 0.75, 0.80, 0.20] // pink tint
    } else {
        [0.8, 0.8, 0.8, 0.15]
    };
    let highlight_c2: [f32; 4] = if selected_is_favorite {
        [1.0, 0.75, 0.80, 0.08]
    } else {
        [0.8, 0.8, 0.8, 0.05]
    };
    let highlight_col = lerp_color(highlight_c1, highlight_c2, anim_t);
    actors.push(act!(quad:
        align(0.0, 0.5):
        xy(highlight_left_world, center_y):
        zoomto(highlight_w, item_h_colored):
        diffuse(highlight_col[0], highlight_col[1], highlight_col[2], highlight_col[3]):
        z(62)
    ));
}

pub fn build(p: MusicWheelParams) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(WHEEL_ACTOR_CAPACITY);
    push(&mut actors, p);
    actors
}

#[cfg(test)]
mod tests {
    use super::{
        choose_itl_wheel_score, itl_fetch_flags, itl_rank_color, itl_wheel_mode_for_sides,
        runtime_slot_requests, song_select_bg_path, srpg_rate_color, visible_song_select_bg_paths,
        visible_song_select_bg_paths_match,
    };
    use crate::config::{
        SelectMusicItlRankMode, SelectMusicItlWheelMode, SelectMusicSongSelectBgMode,
    };
    use crate::screens::select_music::MusicWheelEntry;
    use crate::views::{MUSIC_WHEEL_SLOT_COUNT, MusicWheelSlotRuntimeRequest};
    use deadlib_present::color;
    use deadsync_chart::SongData;
    use deadsync_profile as profile_data;
    use deadsync_score::CachedItlScore;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn song_with_art(banner_path: Option<&str>, background_path: Option<&str>) -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: PathBuf::from("songs/Test/song.ssc"),
            title: "Song".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: banner_path.map(PathBuf::from),
            background_path: background_path.map(PathBuf::from),
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
            normalized_bpms: String::new(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: Vec::new(),
        })
    }

    #[test]
    fn choose_itl_wheel_score_prefers_online_tournament_score() {
        let local = Some(CachedItlScore {
            ex_hundredths: 9732,
            clear_type: 4,
            points: 12_345,
        });

        assert_eq!(
            choose_itl_wheel_score(local, Some(9912), Some(19_912)),
            Some((9912, Some(19_912)))
        );
    }

    #[test]
    fn choose_itl_wheel_score_falls_back_to_local_when_no_online_score() {
        let local = Some(CachedItlScore {
            ex_hundredths: 9732,
            clear_type: 4,
            points: 12_345,
        });

        assert_eq!(
            choose_itl_wheel_score(local, None, None),
            Some((9732, Some(12_345)))
        );
    }

    #[test]
    fn choose_itl_wheel_score_keeps_online_score_without_points() {
        let local = Some(CachedItlScore {
            ex_hundredths: 9732,
            clear_type: 4,
            points: 12_345,
        });

        assert_eq!(
            choose_itl_wheel_score(local, Some(9912), None),
            Some((9912, None))
        );
    }

    #[test]
    fn points_score_wheel_falls_back_to_score_for_versus() {
        assert_eq!(
            itl_wheel_mode_for_sides(SelectMusicItlWheelMode::PointsAndScore, 2),
            SelectMusicItlWheelMode::Score
        );
        assert_eq!(
            itl_wheel_mode_for_sides(SelectMusicItlWheelMode::PointsAndScore, 1),
            SelectMusicItlWheelMode::PointsAndScore
        );
        assert_eq!(
            itl_wheel_mode_for_sides(SelectMusicItlWheelMode::Off, 2),
            SelectMusicItlWheelMode::Off
        );
    }

    #[test]
    fn single_p2_uses_primary_steps_slot() {
        assert_eq!(
            profile_data::runtime_player_index(
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P2
            ),
            0
        );
        assert_eq!(
            profile_data::runtime_player_index(
                profile_data::PlayStyle::Double,
                profile_data::PlayerSide::P2
            ),
            0
        );
        assert_eq!(
            profile_data::runtime_player_index(
                profile_data::PlayStyle::Versus,
                profile_data::PlayerSide::P2
            ),
            1
        );
    }

    #[test]
    fn runtime_slot_requests_keep_fixed_wheel_alignment() {
        let song = song_with_art(None, None);
        let entries = vec![
            MusicWheelEntry::PackHeader {
                name: "Before".to_string(),
                original_index: 0,
                banner_path: None,
                song_count: 1,
                pack_key: Some("Before".to_string()),
                parent_series: None,
            },
            MusicWheelEntry::Song(song.clone()),
            MusicWheelEntry::PackHeader {
                name: "After".to_string(),
                original_index: 1,
                banner_path: None,
                song_count: 1,
                pack_key: Some("After".to_string()),
                parent_series: None,
            },
        ];
        let slots = runtime_slot_requests(
            &entries,
            1,
            [None, None],
            [0, 0],
            profile_data::PlayStyle::Single,
        );

        assert_eq!(slots.len(), MUSIC_WHEEL_SLOT_COUNT);
        let center = MUSIC_WHEEL_SLOT_COUNT / 2;
        assert!(matches!(
            slots[center],
            MusicWheelSlotRuntimeRequest::Song {
                song: prepared,
                chart_hashes: [None, None],
                ..
            } if std::ptr::eq(prepared, song.as_ref())
        ));
        assert!(matches!(
            slots[center - 1],
            MusicWheelSlotRuntimeRequest::Pack {
                key: Some("Before")
            }
        ));
        assert!(matches!(
            slots[center + 1],
            MusicWheelSlotRuntimeRequest::Pack { key: Some("After") }
        ));
    }

    #[test]
    fn online_itl_fetch_flags_follow_mode_and_settle_state() {
        assert_eq!(
            itl_fetch_flags(
                false,
                SelectMusicItlRankMode::Chart,
                SelectMusicItlWheelMode::Score,
                false,
            ),
            (false, false, false)
        );
        assert_eq!(
            itl_fetch_flags(
                true,
                SelectMusicItlRankMode::Chart,
                SelectMusicItlWheelMode::Off,
                false,
            ),
            (true, false, false)
        );
        assert_eq!(
            itl_fetch_flags(
                true,
                SelectMusicItlRankMode::Overall,
                SelectMusicItlWheelMode::Score,
                false,
            ),
            (false, true, false)
        );
        assert_eq!(
            itl_fetch_flags(
                true,
                SelectMusicItlRankMode::None,
                SelectMusicItlWheelMode::Score,
                true,
            ),
            (false, false, true)
        );
    }

    #[test]
    fn srpg_rate_color_matches_zmod_ramp() {
        assert_eq!(
            srpg_rate_color(100, profile_data::PlayerSide::P1),
            [1.0, 1.0, 1.0, 1.0]
        );
        assert_eq!(
            srpg_rate_color(150, profile_data::PlayerSide::P1),
            [1.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(
            srpg_rate_color(150, profile_data::PlayerSide::P2),
            [0.0, 0.0, 1.0, 1.0]
        );
    }

    #[test]
    fn itl_rank_color_matches_arrow_cloud_single_thresholds() {
        assert_eq!(itl_rank_color(10, false), color::JUDGMENT_RGBA[0]);
        assert_eq!(itl_rank_color(11, false), color::JUDGMENT_RGBA[1]);
        assert_eq!(itl_rank_color(25, false), color::JUDGMENT_RGBA[1]);
        assert_eq!(itl_rank_color(26, false), color::JUDGMENT_RGBA[2]);
        assert_eq!(itl_rank_color(50, false), color::JUDGMENT_RGBA[2]);
        assert_eq!(itl_rank_color(51, false), color::JUDGMENT_RGBA[3]);
        assert_eq!(itl_rank_color(75, false), color::JUDGMENT_RGBA[3]);
        assert_eq!(itl_rank_color(76, false), color::JUDGMENT_RGBA[4]);
        assert_eq!(itl_rank_color(85, false), color::JUDGMENT_RGBA[4]);
        assert_eq!(itl_rank_color(86, false), color::JUDGMENT_RGBA[5]);
    }

    #[test]
    fn itl_rank_color_matches_arrow_cloud_double_thresholds() {
        assert_eq!(itl_rank_color(5, true), color::JUDGMENT_RGBA[0]);
        assert_eq!(itl_rank_color(6, true), color::JUDGMENT_RGBA[1]);
        assert_eq!(itl_rank_color(20, true), color::JUDGMENT_RGBA[1]);
        assert_eq!(itl_rank_color(21, true), color::JUDGMENT_RGBA[2]);
        assert_eq!(itl_rank_color(40, true), color::JUDGMENT_RGBA[2]);
        assert_eq!(itl_rank_color(41, true), color::JUDGMENT_RGBA[3]);
        assert_eq!(itl_rank_color(50, true), color::JUDGMENT_RGBA[3]);
        assert_eq!(itl_rank_color(51, true), color::JUDGMENT_RGBA[4]);
        assert_eq!(itl_rank_color(55, true), color::JUDGMENT_RGBA[4]);
        assert_eq!(itl_rank_color(56, true), color::JUDGMENT_RGBA[5]);
    }

    #[test]
    fn song_select_bg_banner_mode_prefers_banner() {
        let song = song_with_art(Some("banner.png"), Some("background.png"));
        let path = song_select_bg_path(&song, SelectMusicSongSelectBgMode::Banner).unwrap();
        assert_eq!(path.as_path(), PathBuf::from("banner.png").as_path());
    }

    #[test]
    fn song_select_bg_bg_mode_prefers_background() {
        let song = song_with_art(Some("banner.png"), Some("background.png"));
        let path = song_select_bg_path(&song, SelectMusicSongSelectBgMode::Bg).unwrap();
        assert_eq!(path.as_path(), PathBuf::from("background.png").as_path());
    }

    #[test]
    fn song_select_bg_modes_fall_back_to_available_art() {
        let song = song_with_art(Some("banner.png"), None);
        let path = song_select_bg_path(&song, SelectMusicSongSelectBgMode::Bg).unwrap();
        assert_eq!(path.as_path(), PathBuf::from("banner.png").as_path());

        let song = song_with_art(None, Some("background.png"));
        let path = song_select_bg_path(&song, SelectMusicSongSelectBgMode::Banner).unwrap();
        assert_eq!(path.as_path(), PathBuf::from("background.png").as_path());
    }

    #[test]
    fn visible_song_select_bg_paths_includes_pack_and_song_art_once() {
        let entries = vec![
            MusicWheelEntry::PackHeader {
                name: "Pack".to_string(),
                original_index: 0,
                banner_path: Some(PathBuf::from("pack.png")),
                song_count: 1,
                pack_key: Some("Pack".to_string()),
                parent_series: None,
            },
            MusicWheelEntry::Song(song_with_art(Some("song.png"), Some("background.png"))),
        ];

        let paths =
            visible_song_select_bg_paths(&entries, 1, 0.0, SelectMusicSongSelectBgMode::Banner);

        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&PathBuf::from("pack.png")));
        assert!(paths.contains(&PathBuf::from("song.png")));
        assert!(visible_song_select_bg_paths_match(
            &entries,
            1,
            0.0,
            SelectMusicSongSelectBgMode::Banner,
            &paths,
        ));

        let mut changed = paths.clone();
        changed[0] = PathBuf::from("different.png");
        assert!(!visible_song_select_bg_paths_match(
            &entries,
            1,
            0.0,
            SelectMusicSongSelectBgMode::Banner,
            &changed,
        ));
        assert!(visible_song_select_bg_paths_match(
            &entries,
            1,
            0.0,
            SelectMusicSongSelectBgMode::Off,
            &[],
        ));
    }
}
