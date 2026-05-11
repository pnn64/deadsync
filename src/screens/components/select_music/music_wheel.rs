use crate::act;
use crate::assets::{FontRole, current_machine_font_key};
use crate::config::{DefaultSyncOffset, SelectMusicItlRankMode, SelectMusicItlWheelMode};
use crate::engine::present::actors::Actor;
use crate::engine::present::cache::{SharedStrCache, cached_shared_str};
use crate::engine::present::color;
use crate::engine::space::widescale;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::chart::ChartData;
use crate::game::profile;
use crate::game::scores;
use crate::game::song::SongData;
use crate::screens::select_music::MusicWheelEntry;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
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
const NUM_WHEEL_SLOTS: usize = NUM_WHEEL_ITEMS_TO_DRAW + 2; // 17 -> 19 internal
const CENTER_WHEEL_SLOT_INDEX: usize = NUM_WHEEL_SLOTS / 2;
const WHEEL_DRAW_RADIUS: f32 = (NUM_WHEEL_ITEMS_TO_DRAW as f32) * 0.5; // 8.5
const SELECTION_HIGHLIGHT_BEAT_PERIOD: f32 = 2.0;
const LAMP_PULSE_PERIOD: f32 = 0.8;
const LAMP_PULSE_LERP_TO_WHITE: f32 = 0.70;
const NEW_BADGE_PULSE_PERIOD: f32 = 1.2;
const NEW_BADGE_COLOR: [f32; 4] = [0.3, 1.0, 0.3, 1.0];
const NEW_BADGE_COLOR_PEAK: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
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
const PACK_COUNT_TEXT_CACHE_LIMIT: usize = 1024;
const STR_REF_CACHE_LIMIT: usize = 4096;
// Simply Love and Arrow Cloud both use zoom(0.2) for the single-line ITL wheel value.
// Our stacked Points+Score mode is deadsync-only, so it needs a smaller zoom to
// keep both lines within that same visual footprint.
const ITL_SCORE_ZOOM: f32 = 0.2;
const ITL_POINTS_SCORE_ZOOM: f32 = 0.13;
const SONG_NULL_SYNC_RIGHT_EDGE: [f32; 4] = [80.0 / 255.0, 20.0 / 255.0, 27.0 / 255.0, 1.0];

thread_local! {
    static ITL_RANK_TEXT_CACHE: RefCell<HashMap<u32, Arc<str>>> =
        RefCell::new(HashMap::with_capacity(256));
    static ITL_EX_TEXT_CACHE: RefCell<HashMap<u32, Arc<str>>> =
        RefCell::new(HashMap::with_capacity(256));
    static ITL_POINTS_TEXT_CACHE: RefCell<HashMap<u32, Arc<str>>> =
        RefCell::new(HashMap::with_capacity(256));
    static PACK_COUNT_TEXT_CACHE: RefCell<HashMap<usize, Arc<str>>> =
        RefCell::new(HashMap::with_capacity(256));
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
    ITL_EX_TEXT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(text) = cache.get(&ex_hundredths) {
            return text.clone();
        }
        let text: Arc<str> = Arc::<str>::from(format!(
            "{}.{:02}",
            ex_hundredths / 100,
            ex_hundredths % 100
        ));
        if cache.len() < ITL_EX_TEXT_CACHE_LIMIT {
            cache.insert(ex_hundredths, text.clone());
        }
        text
    })
}

#[inline(always)]
fn cached_itl_rank_text(rank: u32) -> Arc<str> {
    ITL_RANK_TEXT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(text) = cache.get(&rank) {
            return text.clone();
        }
        let text: Arc<str> = Arc::<str>::from(rank.to_string());
        if cache.len() < ITL_RANK_TEXT_CACHE_LIMIT {
            cache.insert(rank, text.clone());
        }
        text
    })
}

#[inline(always)]
fn cached_itl_points_text(points: u32) -> Arc<str> {
    ITL_POINTS_TEXT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(text) = cache.get(&points) {
            return text.clone();
        }
        let text: Arc<str> = Arc::<str>::from(points.to_string());
        if cache.len() < ITL_POINTS_TEXT_CACHE_LIMIT {
            cache.insert(points, text.clone());
        }
        text
    })
}

#[inline(always)]
fn cached_pack_count_text(count: usize) -> Arc<str> {
    PACK_COUNT_TEXT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(text) = cache.get(&count) {
            return text.clone();
        }
        let text: Arc<str> = Arc::<str>::from(count.to_string());
        if cache.len() < PACK_COUNT_TEXT_CACHE_LIMIT {
            cache.insert(count, text.clone());
        }
        text
    })
}

#[inline(always)]
fn cached_str_ref(text: &str) -> Arc<str> {
    cached_shared_str(&STR_REF_CACHE, text, STR_REF_CACHE_LIMIT)
}

fn song_pack_sync_style(
    song: &SongData,
    prefs: Option<&HashMap<String, rssp::pack::SyncPref>>,
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
        .unwrap_or(rssp::pack::SyncPref::Default);
    Some(crate::game::song::pack_sync_pref_default(pref, default))
}

#[inline(always)]
fn itl_score_line_y(side: profile::PlayerSide, joined_sides: usize) -> (f32, f32) {
    if joined_sides >= 2 {
        return if side == profile::PlayerSide::P1 {
            (-15.0, -6.0)
        } else {
            (0.0, 9.0)
        };
    }
    (-7.0, 3.0)
}

#[inline(always)]
fn itl_score_y(side: profile::PlayerSide, joined_sides: usize) -> f32 {
    if joined_sides >= 2 {
        if side == profile::PlayerSide::P1 {
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
    local_itl: Option<scores::CachedItlScore>,
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
const fn should_fetch_online_itl_score(is_selected_slot: bool, allow_online_fetch: bool) -> bool {
    is_selected_slot && allow_online_fetch
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
    let num_standard = color::FILE_DIFFICULTY_NAMES.len();
    if num_standard == 0 {
        return None;
    }

    let preferred = preferred_index.min(num_standard - 1);
    let preferred_name = color::FILE_DIFFICULTY_NAMES[preferred];
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
        let Some(diff_ix) = color::FILE_DIFFICULTY_NAMES
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

#[inline(always)]
const fn steps_slot_for_side(play_style: profile::PlayStyle, side: profile::PlayerSide) -> usize {
    match (play_style, side) {
        (profile::PlayStyle::Versus, profile::PlayerSide::P2) => 1,
        _ => 0,
    }
}

pub struct MusicWheelParams<'a> {
    pub entries: &'a [MusicWheelEntry],
    pub selected_index: usize,
    pub position_offset_from_selection: f32,
    pub selection_animation_timer: f32,
    pub selection_animation_beat: f32,
    pub color_pack_headers: bool,
    pub selected_charts: [Option<&'a ChartData>; profile::PLAYER_SLOTS],
    pub preferred_difficulty_index: [usize; profile::PLAYER_SLOTS],
    pub song_box_color: Option<[f32; 4]>,
    pub song_text_color: Option<[f32; 4]>,
    pub song_text_color_overrides: Option<&'a HashMap<usize, [f32; 4]>>,
    pub song_has_edit_ptrs: Option<&'a HashSet<usize>>,
    pub show_music_wheel_grades: bool,
    pub show_music_wheel_lamps: bool,
    pub itl_rank_mode: SelectMusicItlRankMode,
    pub itl_wheel_mode: SelectMusicItlWheelMode,
    pub allow_online_fetch: bool,
    pub new_pack_names: Option<&'a HashSet<String>>,
    pub pack_sync_prefs: Option<&'a HashMap<String, rssp::pack::SyncPref>>,
    pub default_sync_offset: DefaultSyncOffset,
}

pub fn build(p: MusicWheelParams) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(NUM_WHEEL_SLOTS * 8 + 1);
    let translated_titles = crate::config::get().translated_titles;
    let play_style = profile::get_session_play_style();
    let target_chart_type = play_style.chart_type();
    let song_box_color = p.song_box_color.unwrap_or_else(col_music_wheel_box);
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

    // Pack name: visually centered in the column
    let pack_center_x_local: f32 = half_highlight - sl_shift + widescale(9.0, 10.0);
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
    let itl_ex_x = screen_width() / widescale(2.15, 2.14) - 40.0;
    let itl_ex_color = color::JUDGMENT_RGBA[0];
    let itl_points_color = [1.0, 1.0, 1.0, 1.0];
    let joined_sides = usize::from(profile::is_session_side_joined(profile::PlayerSide::P1))
        + usize::from(profile::is_session_side_joined(profile::PlayerSide::P2));
    let itl_wheel_mode = itl_wheel_mode_for_sides(p.itl_wheel_mode, joined_sides);
    let is_double_style = target_chart_type.to_ascii_lowercase().contains("double");
    let selected_chart_hash_for_side = |side: profile::PlayerSide| {
        let Some(MusicWheelEntry::Song(_)) = p.entries.get(p.selected_index) else {
            return None;
        };
        let ix = steps_slot_for_side(play_style, side);
        p.selected_charts[ix].map(|chart| chart.short_hash.as_str())
    };
    if matches!(p.itl_rank_mode, SelectMusicItlRankMode::Overall) && p.allow_online_fetch {
        for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
            if !profile::is_session_side_joined(side) {
                continue;
            }
            if let Some(chart_hash) = selected_chart_hash_for_side(side) {
                let _ = scores::get_or_fetch_itl_self_score_for_side(chart_hash, side);
            }
        }
    }
    let overall_itl_ranks_p1 = if matches!(p.itl_rank_mode, SelectMusicItlRankMode::Overall)
        && profile::is_session_side_joined(profile::PlayerSide::P1)
    {
        Some(scores::get_cached_itl_tournament_overall_ranks_for_side(
            profile::PlayerSide::P1,
        ))
    } else {
        None
    };
    let overall_itl_ranks_p2 = if matches!(p.itl_rank_mode, SelectMusicItlRankMode::Overall)
        && profile::is_session_side_joined(profile::PlayerSide::P2)
    {
        Some(scores::get_cached_itl_tournament_overall_ranks_for_side(
            profile::PlayerSide::P2,
        ))
    } else {
        None
    };

    let num_entries = p.entries.len();

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

            match entry {
                MusicWheelEntry::PackHeader {
                    name,
                    original_index,
                    song_count,
                    ..
                } => {
                    let bg_col = col_pack_header_box();
                    let header_color = if p.color_pack_headers {
                        color::simply_love_rgba(*original_index as i32)
                    } else {
                        [1.0, 1.0, 1.0, 1.0]
                    };
                    let show_new_badge = p.color_pack_headers
                        && p.new_pack_names
                            .is_some_and(|new_packs| new_packs.contains(name.as_str()));
                    actors.push(act!(quad:
                        align(0.0, 0.5):
                        xy(highlight_left_world, y_center_item):
                        zoomto(highlight_w, item_h_full):
                        diffuse(0.0, 0.0, 0.0, 1.0):
                        z(51)
                    ));
                    actors.push(act!(quad:
                        align(0.0, 0.5):
                        xy(highlight_left_world, y_center_item):
                        zoomto(highlight_w, item_h_colored):
                        diffuse(bg_col[0], bg_col[1], bg_col[2], bg_col[3]):
                        z(52)
                    ));
                    actors.push(act!(text:
                        font("miso"):
                        settext(cached_str_ref(name.as_str())):
                        align(0.5, 0.5):
                        xy(highlight_left_world + pack_center_x_local, y_center_item):
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
                    let wheel_chart_for_side = |side: profile::PlayerSide| {
                        let ix = steps_slot_for_side(play_style, side);
                        if is_selected_slot {
                            p.selected_charts[ix]
                        } else {
                            chart_for_preferred_or_nearest_standard(
                                info,
                                target_chart_type,
                                p.preferred_difficulty_index[ix],
                            )
                        }
                    };
                    let has_lua = info.has_lua;
                    let lua_submit_allowed = has_lua
                        && if joined_sides == 0 {
                            wheel_chart_for_side(profile::PlayerSide::P1).is_some_and(|chart| {
                                scores::lua_chart_submit_allowed(chart.short_hash.as_str())
                            })
                        } else {
                            [profile::PlayerSide::P1, profile::PlayerSide::P2]
                                .iter()
                                .copied()
                                .any(|side| {
                                    profile::is_session_side_joined(side)
                                        && wheel_chart_for_side(side).is_some_and(|chart| {
                                            scores::lua_chart_submit_allowed(
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
                    if song_pack_sync_style(info, p.pack_sync_prefs, p.default_sync_offset)
                        == Some(DefaultSyncOffset::Null)
                    {
                        actors.push(act!(quad:
                            align(0.0, 0.5):
                            xy(highlight_left_world, y_center_item):
                            zoomto(highlight_w, item_h_colored):
                            diffuse(SONG_NULL_SYNC_RIGHT_EDGE[0], SONG_NULL_SYNC_RIGHT_EDGE[1], SONG_NULL_SYNC_RIGHT_EDGE[2], SONG_NULL_SYNC_RIGHT_EDGE[3]):
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
                            (profile::PlayerSide::P1, grade_x_p1),
                            (profile::PlayerSide::P2, grade_x_p2),
                        ] {
                            if !profile::is_session_side_joined(side) {
                                continue;
                            }
                            let Some(chart) = wheel_chart_for_side(side) else {
                                continue;
                            };
                            let Some(cached_score) =
                                scores::get_cached_score_for_side(&chart.short_hash, side)
                            else {
                                continue;
                            };
                            let has_score = cached_score.grade != scores::Grade::Failed
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
                                let lamp_dir = if side == profile::PlayerSide::P1 {
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
                                        None if cached_score.grade == scores::Grade::Failed => {
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
                                        font(current_machine_font_key(FontRole::ScreenEval)):
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

                    let should_fetch_online_itl =
                        should_fetch_online_itl_score(is_selected_slot, p.allow_online_fetch);

                    if !matches!(p.itl_rank_mode, SelectMusicItlRankMode::None) && joined_sides == 1
                    {
                        for (side, rank_x) in [
                            (profile::PlayerSide::P1, grade_x_p2),
                            (profile::PlayerSide::P2, grade_x_p1),
                        ] {
                            if !profile::is_session_side_joined(side) {
                                continue;
                            }
                            let Some(side_chart) = wheel_chart_for_side(side) else {
                                continue;
                            };
                            let rank = match p.itl_rank_mode {
                                SelectMusicItlRankMode::None => None,
                                SelectMusicItlRankMode::Chart => {
                                    let chart_hash = side_chart.short_hash.as_str();
                                    if should_fetch_online_itl {
                                        scores::get_or_fetch_itl_tournament_rank_for_side(
                                            chart_hash, side,
                                        )
                                    } else {
                                        scores::get_cached_itl_tournament_rank_for_side(
                                            chart_hash, side,
                                        )
                                    }
                                }
                                SelectMusicItlRankMode::Overall => match side {
                                    profile::PlayerSide::P2 => overall_itl_ranks_p2
                                        .as_ref()
                                        .and_then(|ranks| ranks.get(side_chart.short_hash.as_str()))
                                        .copied(),
                                    _ => overall_itl_ranks_p1
                                        .as_ref()
                                        .and_then(|ranks| ranks.get(side_chart.short_hash.as_str()))
                                        .copied(),
                                },
                            };
                            let Some(rank) = rank else {
                                continue;
                            };
                            let rank_color = itl_rank_color(rank, is_double_style);
                            actors.push(act!(text:
                                font(current_machine_font_key(FontRole::Header)):
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

                    for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
                        if matches!(itl_wheel_mode, SelectMusicItlWheelMode::Off) {
                            continue;
                        }
                        if !profile::is_session_side_joined(side) {
                            continue;
                        }
                        let side_chart = wheel_chart_for_side(side);
                        let side_chart_hash = side_chart.map(|chart| chart.short_hash.as_str());
                        let local_itl = scores::get_cached_itl_score_for_song(info, side);
                        let online_ex_hundredths = side_chart_hash.and_then(|chart_hash| {
                            if should_fetch_online_itl {
                                scores::get_or_fetch_itl_self_score_for_side(chart_hash, side)
                            } else {
                                scores::get_cached_itl_self_score_for_side(chart_hash, side)
                            }
                        });
                        let online_points = online_ex_hundredths.and_then(|online_ex| {
                            side_chart
                                .and_then(|chart| scores::itl_points_for_chart(chart, online_ex))
                        });
                        let Some((ex_hundredths, points)) =
                            choose_itl_wheel_score(local_itl, online_ex_hundredths, online_points)
                        else {
                            continue;
                        };
                        match itl_wheel_mode {
                            SelectMusicItlWheelMode::Off => {}
                            SelectMusicItlWheelMode::Score => {
                                actors.push(act!(text:
                                    font(current_machine_font_key(FontRole::Numbers)):
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
                                        font(current_machine_font_key(FontRole::Numbers)):
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
                                    font(current_machine_font_key(FontRole::Numbers)):
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
                                    font(current_machine_font_key(FontRole::Numbers)):
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
                        let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
                        let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
                        let p1_fav = p1_joined
                            && info.charts.iter().any(|c| {
                                profile::is_favorite(profile::PlayerSide::P1, &c.short_hash)
                            });
                        let p2_fav = p2_joined
                            && info.charts.iter().any(|c| {
                                profile::is_favorite(profile::PlayerSide::P2, &c.short_hash)
                            });
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
                        let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
                        let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
                        let both_joined = p1_joined && p2_joined;
                        if (p1_joined || p2_joined)
                            && let Some((pack_dir, song_dir)) =
                                crate::screens::select_music::song_pack_and_dir_name(info.as_ref())
                            && scores::is_itl_unlocks_pack(pack_dir)
                        {
                            let p1_locked = p1_joined
                                && !scores::is_itl_song_folder_unlocked_for_side(
                                    song_dir,
                                    profile::PlayerSide::P1,
                                );
                            let p2_locked = p2_joined
                                && !scores::is_itl_song_folder_unlocked_for_side(
                                    song_dir,
                                    profile::PlayerSide::P2,
                                );
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
            let bg_col = col_pack_header_box();

            // Add black background for 1px gap effect, just like real pack headers
            actors.push(act!(quad:
                align(0.0, 0.5):
                xy(highlight_left_world, y_center_item):
                zoomto(highlight_w, item_h_full):
                diffuse(0.0, 0.0, 0.0, 1.0):
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
                xy(highlight_left_world + pack_center_x_local, y_center_item):
                maxwidth(pack_name_max_w):
                zoom(1.0):
                diffuse(text_color[0], text_color[1], text_color[2], text_color[3]):
                z(53)
            ));
        }
    }

    // Selection highlight overlay (Simply Love: Graphics/MusicWheel highlight.lua + [MusicWheel] HighlightOnCommand)
    let selected_is_favorite =
        if let Some(MusicWheelEntry::Song(info)) = p.entries.get(p.selected_index) {
            let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
            let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
            info.charts.iter().any(|c| {
                (p1_joined && profile::is_favorite(profile::PlayerSide::P1, &c.short_hash))
                    || (p2_joined && profile::is_favorite(profile::PlayerSide::P2, &c.short_hash))
            })
        } else {
            false
        };
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

    actors
}

#[cfg(test)]
mod tests {
    use super::{
        choose_itl_wheel_score, itl_rank_color, itl_wheel_mode_for_sides,
        should_fetch_online_itl_score, steps_slot_for_side,
    };
    use crate::config::SelectMusicItlWheelMode;
    use crate::engine::present::color;
    use crate::game::profile;
    use crate::game::scores::CachedItlScore;

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
            steps_slot_for_side(profile::PlayStyle::Single, profile::PlayerSide::P2),
            0
        );
        assert_eq!(
            steps_slot_for_side(profile::PlayStyle::Double, profile::PlayerSide::P2),
            0
        );
        assert_eq!(
            steps_slot_for_side(profile::PlayStyle::Versus, profile::PlayerSide::P2),
            1
        );
    }

    #[test]
    fn online_itl_fetch_requires_selected_slot() {
        assert!(!should_fetch_online_itl_score(false, true));
    }

    #[test]
    fn online_itl_fetch_requires_settled_selection() {
        assert!(!should_fetch_online_itl_score(true, false));
        assert!(should_fetch_online_itl_score(true, true));
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
}
