use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::{LookupKey, lookup_key, tr};
use crate::engine::gfx::{BlendMode, MeshMode};
use crate::engine::present::actors::{Actor, SizeSpec};
use crate::engine::present::cache::{TextCache, cached_text};
use crate::engine::present::color;
use crate::engine::present::compose::TextLayoutCache;
use crate::engine::present::density;
use crate::engine::present::font;
use crate::engine::space::*;
use crate::game::gameplay;
use crate::game::judgment::{self, JudgeGrade};
use crate::game::profile;
use crate::screens::components::shared::gs_scorebox;
use crate::screens::gameplay::State;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

const TEXT_CACHE_LIMIT: usize = 8192;
const COUNT_PREWARM_CAP: u32 = 2048;
const TIME_PREWARM_CAP_S: u32 = 600;

thread_local! {
    static PADDED_NUM_CACHE: RefCell<TextCache<(u32, u8)>> = RefCell::new(HashMap::with_capacity(2048));
    static PADDED_DIM_CACHE: RefCell<TextCache<(u32, u8)>> = RefCell::new(HashMap::with_capacity(2048));
    static PADDED_BRIGHT_CACHE: RefCell<TextCache<(u32, u8)>> = RefCell::new(HashMap::with_capacity(2048));
    static BLUE_WINDOW_LABEL_CACHE: RefCell<TextCache<i32>> = RefCell::new(HashMap::with_capacity(64));
    static PEAK_NPS_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(512));
    static GAME_TIME_CACHE: RefCell<TextCache<(u32, u8)>> = RefCell::new(HashMap::with_capacity(1024));
    static GAME_TIME_WIDTH_CACHE: RefCell<HashMap<(u32, u8), f32>> = RefCell::new(HashMap::with_capacity(1024));
    static STR_REF_CACHE: RefCell<TextCache<(usize, usize)>> = RefCell::new(HashMap::with_capacity(512));
}

static DIGIT_TEXT: LazyLock<[Arc<str>; 10]> =
    LazyLock::new(|| ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"].map(Arc::<str>::from));
static SLASH_TEXT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::<str>::from("/"));

#[derive(Clone, Copy)]
struct LabeledColor {
    label: LookupKey,
    color: [f32; 4],
}

const JUDGMENT_INFO: [LabeledColor; 6] = [
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentFantastic"),
        color: color::JUDGMENT_RGBA[0],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentExcellent"),
        color: color::JUDGMENT_RGBA[1],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentGreat"),
        color: color::JUDGMENT_RGBA[2],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentDecent"),
        color: color::JUDGMENT_RGBA[3],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentWayOff"),
        color: color::JUDGMENT_RGBA[4],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentMiss"),
        color: color::JUDGMENT_RGBA[5],
    },
];

const STEP_INFO_LABELS: [LookupKey; 4] = [
    lookup_key("Gameplay", "SongInfoSong"),
    lookup_key("Gameplay", "SongInfoArtist"),
    lookup_key("Gameplay", "SongInfoPack"),
    lookup_key("Gameplay", "SongInfoDesc"),
];

const HOLDS_MINES_ROLLS_LABELS: [LookupKey; 3] = [
    lookup_key("Gameplay", "HoldsLabel"),
    lookup_key("Gameplay", "MinesLabel"),
    lookup_key("Gameplay", "RollsLabel"),
];

fn step_info_label(index: usize) -> Arc<str> {
    STEP_INFO_LABELS
        .get(index)
        .map(LookupKey::get)
        .unwrap_or_else(|| Arc::from(""))
}

fn holds_mines_rolls_label(index: usize) -> Arc<str> {
    HOLDS_MINES_ROLLS_LABELS
        .get(index)
        .map(LookupKey::get)
        .unwrap_or_else(|| Arc::from(""))
}

fn judgment_label(index: usize) -> Arc<str> {
    JUDGMENT_INFO
        .get(index)
        .map(|info| info.label.get())
        .unwrap_or_else(|| Arc::from(""))
}

fn time_song_left_text() -> Arc<str> {
    tr("Gameplay", "TimeSong")
}

fn time_remaining_left_text() -> Arc<str> {
    tr("Gameplay", "TimeRemaining")
}

fn time_song_right_text() -> Arc<str> {
    tr("Gameplay", "TimeSong")
}

fn time_remaining_right_text() -> Arc<str> {
    tr("Gameplay", "TimeRemaining")
}

#[inline(always)]
fn cached_str_ref(text: &str) -> Arc<str> {
    let key = (text.as_ptr() as usize, text.len());
    cached_text(&STR_REF_CACHE, key, TEXT_CACHE_LIMIT, || text.to_owned())
}

#[inline(always)]
fn step_stats_player_idx(state: &State, player_side: profile::PlayerSide) -> usize {
    match (state.num_players, player_side) {
        (2, profile::PlayerSide::P2) => 1,
        _ => 0,
    }
}

fn clip_density_life_points(points: &mut Vec<[f32; 2]>, offset: f32) {
    let first_visible = points.partition_point(|p| p[0] < offset);
    if first_visible == 0 {
        return;
    }
    if first_visible >= points.len() {
        points.clear();
        return;
    }

    let a = points[first_visible - 1];
    let b = points[first_visible];
    let dx = (b[0] - a[0]).max(0.000_001_f32);
    let t = ((offset - a[0]) / dx).clamp(0.0_f32, 1.0_f32);
    points[first_visible - 1] = [offset, a[1] + (b[1] - a[1]) * t];
    points.drain(0..(first_visible - 1));
}

fn refresh_density_graph_meshes_for_player(state: &mut State, player_idx: usize) {
    let gameplay = &mut state.gameplay;
    let render = &mut state.density_graph;
    let graph_w = gameplay.density_graph_graph_w;
    let graph_h = gameplay.density_graph_graph_h;
    let scaled_width = gameplay.density_graph_scaled_width;
    if player_idx >= gameplay.num_players
        || graph_w <= 0.0_f32
        || graph_h <= 0.0_f32
        || scaled_width <= 0.0_f32
    {
        render.mesh[player_idx] = None;
        render.life_mesh[player_idx] = None;
        render.mesh_offset_px[player_idx] = 0;
        render.life_mesh_offset_px[player_idx] = 0;
        gameplay.density_graph_life_dirty[player_idx] = false;
        return;
    }

    let offset = (gameplay.density_graph_u0 * scaled_width).clamp(0.0_f32, scaled_width);
    let offset_px = offset.floor() as i32;
    let offset_px_f = offset_px as f32;

    if offset_px != render.mesh_offset_px[player_idx] {
        render.mesh_offset_px[player_idx] = offset_px;
        density::update_density_hist_mesh(
            &mut render.mesh[player_idx],
            render.cache[player_idx].as_ref(),
            offset_px_f,
            graph_w,
        );
    }

    let prev_offset_px = render.life_mesh_offset_px[player_idx];
    let offset_changed = offset_px != prev_offset_px;
    if !offset_changed && !gameplay.density_graph_life_dirty[player_idx] {
        return;
    }

    render.life_mesh_offset_px[player_idx] = offset_px;
    gameplay.density_graph_life_dirty[player_idx] = false;
    if offset_px > prev_offset_px {
        clip_density_life_points(
            &mut gameplay.density_graph_life_points[player_idx],
            offset_px_f,
        );
    }
    if gameplay.density_graph_life_points[player_idx].len() < 2 {
        render.life_mesh[player_idx] = None;
        return;
    }

    density::update_density_life_mesh(
        &mut render.life_mesh[player_idx],
        &gameplay.density_graph_life_points[player_idx],
        offset_px_f,
        graph_w,
        2.0_f32,
        [1.0_f32, 1.0_f32, 1.0_f32, 1.0_f32],
    );
}

pub fn refresh_density_graph_meshes(state: &mut State) {
    for player_idx in 0..state.num_players {
        refresh_density_graph_meshes_for_player(state, player_idx);
    }
}

#[inline(always)]
fn cached_padded_num(count: u32, digits: usize) -> Arc<str> {
    let digits = digits.clamp(1, u8::MAX as usize) as u8;
    cached_text(&PADDED_NUM_CACHE, (count, digits), TEXT_CACHE_LIMIT, || {
        format!("{:0width$}", count, width = digits as usize)
    })
}

#[inline(always)]
fn padded_dim_len(full: &str, count: u32, digits: usize) -> usize {
    if count == 0 {
        digits.saturating_sub(1).min(full.len())
    } else {
        full.find(|c: char| c != '0').unwrap_or(full.len())
    }
}

#[inline(always)]
fn cached_padded_runs(count: u32, digits: usize) -> (Arc<str>, Arc<str>) {
    let digits = digits.clamp(1, u8::MAX as usize) as u8;
    let dim = cached_text(&PADDED_DIM_CACHE, (count, digits), TEXT_CACHE_LIMIT, || {
        let full = cached_padded_num(count, digits as usize);
        let split = padded_dim_len(full.as_ref(), count, digits as usize);
        full[..split].to_string()
    });
    let bright = cached_text(
        &PADDED_BRIGHT_CACHE,
        (count, digits),
        TEXT_CACHE_LIMIT,
        || {
            let full = cached_padded_num(count, digits as usize);
            let split = padded_dim_len(full.as_ref(), count, digits as usize);
            full[split..].to_string()
        },
    );
    (dim, bright)
}

#[inline(always)]
fn cached_blue_window_label(ms: i32) -> Arc<str> {
    use crate::assets::i18n::tr_fmt;
    cached_text(&BLUE_WINDOW_LABEL_CACHE, ms, TEXT_CACHE_LIMIT, || {
        tr_fmt("Gameplay", "BlueWindowLabel", &[("ms", &ms.to_string())]).to_string()
    })
}

#[inline(always)]
fn cached_peak_nps_text(peak: f32) -> Arc<str> {
    use crate::assets::i18n::tr_fmt;
    cached_text(&PEAK_NPS_CACHE, peak.to_bits(), TEXT_CACHE_LIMIT, || {
        tr_fmt(
            "Gameplay",
            "PeakNps",
            &[("peak_nps", &format!("{:.2}", peak.max(0.0)))],
        )
        .to_string()
    })
}

#[inline(always)]
fn cached_game_time(seconds: u32, mode: u8) -> Arc<str> {
    cached_text(&GAME_TIME_CACHE, (seconds, mode), TEXT_CACHE_LIMIT, || {
        let seconds = seconds as u64;
        let minutes = seconds / 60;
        let secs = seconds % 60;
        match mode {
            0 => {
                let hours = seconds / 3600;
                let mins = (seconds % 3600) / 60;
                format!("{hours}:{mins:02}:{secs:02}")
            }
            1 => format!("{minutes:02}:{secs:02}"),
            _ => format!("{minutes}:{secs:02}"),
        }
    })
}

#[inline(always)]
fn game_time_mode(total_seconds: f32) -> u8 {
    if total_seconds >= 3600.0 {
        0
    } else if total_seconds >= 600.0 {
        1
    } else {
        2
    }
}

#[inline(always)]
fn game_time_key(seconds: f32, total_seconds: f32) -> (u32, u8) {
    (seconds.max(0.0) as u32, game_time_mode(total_seconds))
}

#[inline(always)]
fn glyph_width_scaled(
    metrics_font: &font::Font,
    all_fonts: &HashMap<&'static str, font::Font>,
    ch: char,
    zoom: f32,
) -> f32 {
    font::find_glyph(metrics_font, ch, all_fonts).map_or(0, |glyph| glyph.advance_i32) as f32 * zoom
}

#[inline(always)]
fn push_versus_count_texts(
    actors: &mut Vec<Actor>,
    is_p1: bool,
    anchor_x: f32,
    y: f32,
    digit_w: f32,
    numbers_zoom_x: f32,
    numbers_zoom_y: f32,
    dim_text: Arc<str>,
    bright_text: Arc<str>,
    dim: [f32; 4],
    bright: [f32; 4],
    z: i16,
) {
    let dim_len = dim_text.len() as f32;
    let bright_len = bright_text.len() as f32;
    if is_p1 {
        if !dim_text.is_empty() {
            let mut a = act!(text:
                font("wendy_screenevaluation"): settext(dim_text):
                align(0.0, 0.5): xy(anchor_x, y):
                zoom(numbers_zoom_y):
                diffuse(dim[0], dim[1], dim[2], dim[3]):
                z(z):
                horizalign(left)
            );
            if let Actor::Text { scale, .. } = &mut a {
                scale[0] = numbers_zoom_x;
                scale[1] = numbers_zoom_y;
            }
            actors.push(a);
        }
        if !bright_text.is_empty() {
            let mut a = act!(text:
                font("wendy_screenevaluation"): settext(bright_text):
                align(0.0, 0.5): xy(anchor_x + dim_len * digit_w, y):
                zoom(numbers_zoom_y):
                diffuse(bright[0], bright[1], bright[2], bright[3]):
                z(z):
                horizalign(left)
            );
            if let Actor::Text { scale, .. } = &mut a {
                scale[0] = numbers_zoom_x;
                scale[1] = numbers_zoom_y;
            }
            actors.push(a);
        }
    } else {
        if !bright_text.is_empty() {
            let mut a = act!(text:
                font("wendy_screenevaluation"): settext(bright_text):
                align(1.0, 0.5): xy(anchor_x, y):
                zoom(numbers_zoom_y):
                diffuse(bright[0], bright[1], bright[2], bright[3]):
                z(z):
                horizalign(right)
            );
            if let Actor::Text { scale, .. } = &mut a {
                scale[0] = numbers_zoom_x;
                scale[1] = numbers_zoom_y;
            }
            actors.push(a);
        }
        if !dim_text.is_empty() {
            let mut a = act!(text:
                font("wendy_screenevaluation"): settext(dim_text):
                align(1.0, 0.5): xy(anchor_x - bright_len * digit_w, y):
                zoom(numbers_zoom_y):
                diffuse(dim[0], dim[1], dim[2], dim[3]):
                z(z):
                horizalign(right)
            );
            if let Actor::Text { scale, .. } = &mut a {
                scale[0] = numbers_zoom_x;
                scale[1] = numbers_zoom_y;
            }
            actors.push(a);
        }
    }
}

#[inline(always)]
fn cached_game_time_width_for_key(key: (u32, u8), asset_manager: &AssetManager) -> f32 {
    if let Some(w) = GAME_TIME_WIDTH_CACHE.with(|cache| cache.borrow().get(&key).copied()) {
        return w;
    }
    let text = cached_game_time(key.0, key.1);
    let width = asset_manager
        .with_fonts(|all_fonts| {
            asset_manager.with_font("miso", |f| {
                font::measure_line_width_logical(f, text.as_ref(), all_fonts) as f32
            })
        })
        .unwrap_or(0.0);
    GAME_TIME_WIDTH_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() < TEXT_CACHE_LIMIT {
            cache.insert(key, width);
        }
    });
    width
}

#[inline(always)]
fn digit_text(digit: u8) -> Arc<str> {
    if digit.is_ascii_digit() {
        DIGIT_TEXT[(digit - b'0') as usize].clone()
    } else {
        Arc::<str>::from("")
    }
}

#[inline(always)]
fn step_info_label_text(index: usize) -> Arc<str> {
    step_info_label(index)
}

#[inline(always)]
fn holds_mines_rolls_label_text(index: usize) -> Arc<str> {
    holds_mines_rolls_label(index)
}

pub fn prewarm_text_layout(
    cache: &mut TextLayoutCache,
    fonts: &HashMap<&'static str, font::Font>,
    asset_manager: &AssetManager,
    state: &State,
) {
    let mut max_count = 0u32;
    for player in 0..state.num_players {
        max_count = max_count
            .max(state.total_steps[player])
            .max(state.holds_total[player])
            .max(state.rolls_total[player])
            .max(state.mines_total[player]);
    }
    let digits = if max_count > 0 {
        (max_count.ilog10() as usize + 1).max(4)
    } else {
        4
    };
    for count in 0..=max_count.min(COUNT_PREWARM_CAP) {
        let (dim, bright) = cached_padded_runs(count, digits);
        cache.prewarm_text(fonts, "wendy_screenevaluation", dim.as_ref(), None);
        cache.prewarm_text(fonts, "wendy_screenevaluation", bright.as_ref(), None);
    }
    let (dim, bright) = cached_padded_runs(max_count, digits);
    cache.prewarm_text(fonts, "wendy_screenevaluation", dim.as_ref(), None);
    cache.prewarm_text(fonts, "wendy_screenevaluation", bright.as_ref(), None);
    for player in 0..state.num_players {
        for count in [
            state.total_steps[player],
            state.holds_total[player],
            state.rolls_total[player],
            state.mines_total[player],
        ] {
            let (dim, bright) = cached_padded_runs(count, digits);
            cache.prewarm_text(fonts, "wendy_screenevaluation", dim.as_ref(), None);
            cache.prewarm_text(fonts, "wendy_screenevaluation", bright.as_ref(), None);
        }
    }
    let end_seconds = crate::game::gameplay::song_time_ns_to_seconds(
        state.music_end_time_ns.max(state.notes_end_time_ns),
    )
    .ceil()
    .max(0.0) as u32;
    let mode = game_time_mode(end_seconds as f32);
    for second in 0..=end_seconds.min(TIME_PREWARM_CAP_S) {
        let key = (second, mode);
        let text = cached_game_time(second, mode);
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
        let _ = cached_game_time_width_for_key(key, asset_manager);
    }
    let key = (end_seconds, mode);
    let text = cached_game_time(end_seconds, mode);
    cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    let _ = cached_game_time_width_for_key(key, asset_manager);
    cache.prewarm_text(fonts, "miso", &time_song_left_text(), None);
    cache.prewarm_text(fonts, "miso", &time_remaining_left_text(), None);
    cache.prewarm_text(fonts, "miso", &time_song_right_text(), None);
    cache.prewarm_text(fonts, "miso", &time_remaining_right_text(), None);
    cache.prewarm_text(fonts, "miso", SLASH_TEXT.as_ref(), None);
    for label in (0..4).map(step_info_label).collect::<Vec<_>>().iter() {
        cache.prewarm_text(fonts, "miso", label.as_ref(), None);
    }
    for label in (0..3)
        .map(holds_mines_rolls_label)
        .collect::<Vec<_>>()
        .iter()
    {
        cache.prewarm_text(fonts, "miso", label.as_ref(), None);
    }
    for player in 0..state.num_players {
        let chart = &state.charts[player];
        cache.prewarm_text(fonts, "miso", state.song_full_title.as_ref(), None);
        cache.prewarm_text(fonts, "miso", state.song.artist.as_str(), None);
        cache.prewarm_text(fonts, "miso", state.pack_group.as_ref(), None);
        cache.prewarm_text(fonts, "miso", chart.description.as_str(), None);
        let peak = cached_peak_nps_text(chart.max_nps.max(0.0) as f32);
        cache.prewarm_text(fonts, "miso", peak.as_ref(), None);
    }
}

pub fn build(
    state: &State,
    asset_manager: &AssetManager,
    playfield_center_x: f32,
    player_side: profile::PlayerSide,
) -> Vec<Actor> {
    let wide = is_wide();
    let layout = step_stats_pane_layout(state, playfield_center_x, player_side);
    let mut actors = Vec::with_capacity(if wide { 48 } else { 1 });
    build_banner(&mut actors, state, layout, wide, player_side);
    build_pack_banner(&mut actors, state, layout, wide, player_side);
    build_steps_info(&mut actors, state, layout, wide, player_side);
    build_side_pane(&mut actors, state, asset_manager, layout, wide, player_side);
    build_holds_mines_rolls_pane(&mut actors, state, asset_manager, layout, wide, player_side);
    build_scorebox_pane(&mut actors, state, layout, wide, player_side);
    actors
}

#[derive(Clone, Copy, Debug)]
struct StepStatsPaneLayout {
    sidepane_center_x: f32,
    sidepane_center_y: f32,
    sidepane_width: f32,
    note_field_is_centered: bool,
    is_ultrawide: bool,
    banner_data_zoom: f32,
}

fn step_stats_pane_layout(
    state: &State,
    playfield_center_x: f32,
    player_side: profile::PlayerSide,
) -> StepStatsPaneLayout {
    let sw = screen_width();
    let sh = screen_height().max(1.0);
    let wide = is_wide();
    let is_ultrawide = sw / sh > (21.0 / 9.0);
    let note_field_is_centered = (playfield_center_x - screen_center_x()).abs() < 1.0;

    let mut sidepane_width = sw * 0.5;
    let mut sidepane_center_x = match player_side {
        profile::PlayerSide::P1 => sw * 0.75,
        profile::PlayerSide::P2 => sw * 0.25,
    };

    // zmod StepStatistics/default.lua:
    // when 1P notefield is centered on widescreen, clamp sidepane to the
    // region between notefield edge and screen edge.
    if !is_ultrawide && note_field_is_centered && wide {
        let nf_width = notefield_width(state).unwrap_or(256.0).max(1.0);
        sidepane_width = ((sw - nf_width) * 0.5).max(1.0);
        sidepane_center_x = match player_side {
            profile::PlayerSide::P1 => {
                screen_center_x() + nf_width + (sidepane_width - nf_width) * 0.5
            }
            profile::PlayerSide::P2 => {
                screen_center_x() - nf_width - (sidepane_width - nf_width) * 0.5
            }
        };
    }

    // zmod ultrawide versus override.
    if is_ultrawide && state.num_players > 1 {
        sidepane_width = sw * 0.2;
        sidepane_center_x = match player_side {
            profile::PlayerSide::P1 => sidepane_width * 0.5,
            profile::PlayerSide::P2 => sw - (sidepane_width * 0.5),
        };
    }

    let banner_data_zoom = if note_field_is_centered && wide && !is_ultrawide {
        let ar = sw / sh;
        let t = ((ar - (16.0 / 10.0)) / ((16.0 / 9.0) - (16.0 / 10.0))).clamp(0.0, 1.0);
        0.825 + (0.925 - 0.825) * t
    } else {
        1.0
    };

    StepStatsPaneLayout {
        sidepane_center_x,
        sidepane_center_y: screen_center_y() + 80.0,
        sidepane_width,
        note_field_is_centered,
        is_ultrawide,
        banner_data_zoom,
    }
}

pub fn build_versus_step_stats(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    if !is_wide() {
        return vec![];
    }
    // Simply Love shows centered step stats in 2P versus on widescreen, but not on ultrawide
    // (ultrawide already has native per-player side panes).
    let is_ultrawide = screen_width() / screen_height().max(1.0) > (21.0 / 9.0);
    if is_ultrawide {
        return vec![];
    }
    if state.num_players < 2 || state.players.len() < 2 {
        return vec![];
    }
    let show_for: [bool; 2] = [
        state.player_profiles[0].data_visualizations == profile::DataVisualizations::StepStatistics,
        state.player_profiles[1].data_visualizations == profile::DataVisualizations::StepStatistics,
    ];
    if !show_for[0] && !show_for[1] {
        return vec![];
    }

    let center_x = screen_center_x();

    let total_tapnotes = state.charts[0]
        .stats
        .total_steps
        .max(state.charts[1].stats.total_steps) as f32;
    let digits = if total_tapnotes > 0.0 {
        (total_tapnotes.log10().floor() as usize + 1).max(4)
    } else {
        4
    };

    let group_zoom_y = 0.8_f32;
    let group_zoom_x = if digits > 4 {
        (group_zoom_y - 0.12 * (digits.saturating_sub(4) as f32)).max(0.1)
    } else {
        group_zoom_y
    };
    let numbers_zoom_y = group_zoom_y * 0.5;
    let numbers_zoom_x = group_zoom_x * 0.5;
    let y_base = -280.0;

    // Keep the background bar below the top HUD (song title/BPM), but let the
    // digits sit above playfield elements if needed.
    let z_bg = 80i16;
    let z_fg = 110i16;

    let mut actors = Vec::with_capacity(128);
    // Center black column behind the counters (SL: VersusStepStatistics.lua).
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(screen_center_x(), screen_center_y()):
        zoomto(150.0, screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(z_bg)
    ));

    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("wendy_screenevaluation", |f| {
            let digit_w = glyph_width_scaled(f, all_fonts, '0', numbers_zoom_x);
            if digit_w <= 0.0 {
                return;
            }

            // Simply Love (VersusStepStatistics.lua) positions the two TapNoteJudgments actorframes at:
            // P1: x=-64, P2: x=+66 (relative to center). TapNoteJudgments internally uses
            // `PlayerNumber:Reverse()[player]` for halign, which is P1=0 (left), P2=1 (right),
            // so both number blocks extend inward and sit inside the 150px black column.
            let base_anchor_p1 = center_x - 64.0; // left edge for P1 block
            let base_anchor_p2 = center_x + 66.0; // right edge for P2 block
            let block_w = (digits as f32) * digit_w;
            let bar_left = center_x - 75.0;
            let bar_right = center_x + 75.0;
            let margin = 4.0;
            let anchor_p1 = base_anchor_p1.clamp(bar_left + margin, bar_right - margin - block_w);
            let anchor_p2 = base_anchor_p2.clamp(bar_left + margin + block_w, bar_right - margin);

            for (player_idx, show) in show_for.iter().copied().enumerate() {
                if !show {
                    continue;
                }
                let is_p1 = player_idx == 0;
                let group_y = 100.0;
                let anchor_x = if is_p1 { anchor_p1 } else { anchor_p2 };
                let group_origin_y = screen_center_y() + group_y;

                let player_profile = &state.player_profiles[player_idx];
                let show_fa_plus_window = player_profile.show_fa_plus_window;
                let show_fa_split = show_fa_plus_window || player_profile.custom_fantastic_window;
                let row_height = if show_fa_split { 29.0 } else { 35.0 };

                let (start, end) = state.note_ranges[player_idx];
                if show_fa_split && end > start {
                    let blue_window_ms = gameplay::player_blue_window_ms(state, player_idx);
                    let wc =
                        gameplay::display_window_counts(state, player_idx, Some(blue_window_ms));
                    let counts = [wc.w0, wc.w1, wc.w2, wc.w3, wc.w4, wc.w5, wc.miss];
                    let bright_colors = [
                        color::JUDGMENT_RGBA[0],
                        color::JUDGMENT_FA_PLUS_WHITE_RGBA,
                        color::JUDGMENT_RGBA[1],
                        color::JUDGMENT_RGBA[2],
                        color::JUDGMENT_RGBA[3],
                        color::JUDGMENT_RGBA[4],
                        color::JUDGMENT_RGBA[5],
                    ];
                    let dim_colors = [
                        color::JUDGMENT_DIM_RGBA[0],
                        color::JUDGMENT_FA_PLUS_WHITE_GAMEPLAY_DIM_RGBA,
                        color::JUDGMENT_DIM_RGBA[1],
                        color::JUDGMENT_DIM_RGBA[2],
                        color::JUDGMENT_DIM_RGBA[3],
                        color::JUDGMENT_DIM_RGBA[4],
                        color::JUDGMENT_DIM_RGBA[5],
                    ];
                    for (row_i, count) in counts.iter().copied().enumerate() {
                        let y =
                            group_origin_y + (y_base + row_i as f32 * row_height) * group_zoom_y;
                        let (dim_text, bright_text) = cached_padded_runs(count, digits);
                        push_versus_count_texts(
                            &mut actors,
                            is_p1,
                            anchor_x,
                            y,
                            digit_w,
                            numbers_zoom_x,
                            numbers_zoom_y,
                            dim_text,
                            bright_text,
                            dim_colors[row_i],
                            bright_colors[row_i],
                            z_fg,
                        );
                    }
                } else {
                    let counts = [
                        gameplay::display_judgment_count(state, player_idx, JudgeGrade::Fantastic),
                        gameplay::display_judgment_count(state, player_idx, JudgeGrade::Excellent),
                        gameplay::display_judgment_count(state, player_idx, JudgeGrade::Great),
                        gameplay::display_judgment_count(state, player_idx, JudgeGrade::Decent),
                        gameplay::display_judgment_count(state, player_idx, JudgeGrade::WayOff),
                        gameplay::display_judgment_count(state, player_idx, JudgeGrade::Miss),
                    ];
                    for (row_i, count) in counts.iter().copied().enumerate() {
                        let y =
                            group_origin_y + (y_base + row_i as f32 * row_height) * group_zoom_y;
                        let (dim_text, bright_text) = cached_padded_runs(count, digits);
                        push_versus_count_texts(
                            &mut actors,
                            is_p1,
                            anchor_x,
                            y,
                            digit_w,
                            numbers_zoom_x,
                            numbers_zoom_y,
                            dim_text,
                            bright_text,
                            color::JUDGMENT_DIM_RGBA[row_i],
                            color::JUDGMENT_RGBA[row_i],
                            z_fg,
                        );
                    }
                }
            }
        });
    });

    if let Some(banner_path) = &state.song.banner_path {
        let key = banner_path.to_string_lossy().into_owned();
        actors.push(act!(sprite(key):
            align(0.5, 0.5):
            xy(screen_center_x(), screen_center_y() + 70.0):
            setsize(418.0, 164.0):
            zoom(0.3):
            z(z_fg)
        ));
    }

    actors
}

pub fn build_double_step_stats(
    state: &State,
    asset_manager: &AssetManager,
    playfield_center_x: f32,
) -> Vec<Actor> {
    if !is_wide() {
        return vec![];
    }
    let is_ultrawide = screen_width() / screen_height().max(1.0) > (21.0 / 9.0);
    if is_ultrawide {
        return vec![];
    }
    if state.cols_per_player <= 4 {
        return vec![];
    }

    let Some(notefield_width) = notefield_width(state) else {
        return vec![];
    };

    // Simply Love: StepStatistics/default.lua
    // - StepStatsPane centered: x=_screen.cx, y=_screen.cy+80
    // - BannerAndData is scaled when the notefield is centered (aspect 16:10..16:9)
    let header_h = 80.0;
    let pane_cx = screen_center_x();
    let pane_cy = screen_center_y() + header_h;

    let note_field_is_centered = (playfield_center_x - screen_center_x()).abs() < 1.0;
    let banner_data_zoom = if note_field_is_centered {
        let ar = screen_width() / screen_height();
        let t = ((ar - (16.0 / 10.0)) / ((16.0 / 9.0) - (16.0 / 10.0))).clamp(0.0, 1.0);
        0.825 + (0.925 - 0.825) * t
    } else {
        1.0
    };

    let mut actors = Vec::with_capacity(256);

    // DarkBackground.lua (double): two 200px-wide panels flanking the notefield.
    let nf_half_w = notefield_width * 0.5;
    let bg_y = screen_center_y();
    let z_bg = -80i16;
    actors.push(act!(quad:
        align(1.0, 0.5):
        xy(pane_cx - nf_half_w, bg_y):
        zoomto(200.0, screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.95):
        z(z_bg)
    ));
    actors.push(act!(quad:
        align(0.0, 0.5):
        xy(pane_cx + nf_half_w, bg_y):
        zoomto(200.0, screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.95):
        z(z_bg)
    ));

    // Banner.lua (double): xy(GetNotefieldWidth() - 140, -200)
    if let Some(banner_path) = &state.song.banner_path {
        let banner_key = banner_path.to_string_lossy().into_owned();
        let banner_x = pane_cx + ((notefield_width - 140.0) * banner_data_zoom);
        let banner_y = pane_cy + (-200.0 * banner_data_zoom);
        actors.push(act!(sprite(banner_key):
            align(0.5, 0.5): xy(banner_x, banner_y):
            setsize(418.0, 164.0):
            zoom(0.4 * banner_data_zoom):
            z(-50)
        ));
    }

    // Banner2.lua (zmod pack banner): static (no animation) at the final position.
    if let Some(pack_banner_path) = state.pack_banner_path.as_ref() {
        let pack_key = pack_banner_path.to_string_lossy().into_owned();
        let (final_offset, final_size) = if note_field_is_centered {
            (-115.0, 0.2)
        } else {
            (-160.0, 0.25)
        };
        let x = pane_cx + (final_offset * banner_data_zoom);
        let y = pane_cy + (20.0 * banner_data_zoom);
        actors.push(act!(sprite(pack_key):
            align(0.5, 0.5): xy(x, y):
            setsize(418.0, 164.0):
            zoom(final_size * banner_data_zoom):
            z(-49)
        ));
    }

    // TapNoteJudgments.lua (double): x(-GetNotefieldWidth() + 75), y(40), zoom(0.8)
    {
        let origin_x = pane_cx + ((-notefield_width + 75.0) * banner_data_zoom);
        let origin_y = pane_cy + (40.0 * banner_data_zoom);
        let base_zoom = 0.8 * banner_data_zoom;

        let total_tapnotes = state.charts[0].stats.total_steps as f32;
        let digits = if total_tapnotes > 0.0 {
            (total_tapnotes.log10().floor() as usize + 1).max(4)
        } else {
            4
        };
        let show_fa_plus_window = state.player_profiles[0].show_fa_plus_window;
        let player_profile = &state.player_profiles[0];
        let show_fa_split = show_fa_plus_window || player_profile.custom_fantastic_window;
        let show_blue_ms_label = player_profile.custom_fantastic_window
            || (show_fa_plus_window && player_profile.fa_plus_10ms_blue_window);
        let blue_window_ms = gameplay::player_blue_window_ms(state, 0);
        let blue_window_label = cached_blue_window_label(blue_window_ms.round() as i32);
        let row_height = if show_fa_split { 29.0 } else { 35.0 };
        let y_base = -280.0;

        asset_manager.with_fonts(|all_fonts| {
            asset_manager.with_font("wendy_screenevaluation", |f| {
                let numbers_zoom = base_zoom * 0.5;
                let digit_w = glyph_width_scaled(f, all_fonts, '0', numbers_zoom);
                if digit_w <= 0.0 {
                    return;
                }
                let block_w = digit_w * digits as f32;
                let numbers_left_x = origin_x + (1.4 * block_w);
                let label_x =
                    origin_x + ((80.0 + (digits.saturating_sub(4) as f32 * 16.0)) * base_zoom);
                let label_zoom = base_zoom * 0.833;
                let show_standard_judgments = !show_fa_split;

                if show_standard_judgments {
                    let counts = [
                        gameplay::display_judgment_count(state, 0, JudgeGrade::Fantastic),
                        gameplay::display_judgment_count(state, 0, JudgeGrade::Excellent),
                        gameplay::display_judgment_count(state, 0, JudgeGrade::Great),
                        gameplay::display_judgment_count(state, 0, JudgeGrade::Decent),
                        gameplay::display_judgment_count(state, 0, JudgeGrade::WayOff),
                        gameplay::display_judgment_count(state, 0, JudgeGrade::Miss),
                    ];
                    let labels: Vec<Arc<str>> = (0..6).map(judgment_label).collect();
                    for row_i in 0..labels.len() {
                        let local_y = y_base + (row_i as f32 * row_height);
                        let y_numbers = origin_y + (local_y * base_zoom);
                        let y_label = origin_y + ((local_y + 1.0) * base_zoom);
                        let bright = color::JUDGMENT_RGBA[row_i];
                        let dim = color::JUDGMENT_DIM_RGBA[row_i];
                        let count = counts[row_i];
                        let (dim_text, bright_text) = cached_padded_runs(count, digits);
                        let dim_len = dim_text.len() as f32;

                        if !dim_text.is_empty() {
                            actors.push(act!(text:
                                font("wendy_screenevaluation"): settext(dim_text):
                                align(0.0, 0.5): xy(numbers_left_x, y_numbers):
                                zoom(numbers_zoom):
                                diffuse(dim[0], dim[1], dim[2], dim[3]):
                                z(71):
                                horizalign(left)
                            ));
                        }
                        if !bright_text.is_empty() {
                            actors.push(act!(text:
                                font("wendy_screenevaluation"): settext(bright_text):
                                align(0.0, 0.5): xy(numbers_left_x + dim_len * digit_w, y_numbers):
                                zoom(numbers_zoom):
                                diffuse(bright[0], bright[1], bright[2], bright[3]):
                                z(71):
                                horizalign(left)
                            ));
                        }

                        actors.push(act!(text:
                            font("miso"): settext(labels[row_i].clone()):
                            align(1.0, 0.5): horizalign(right):
                            xy(label_x, y_label):
                            zoom(label_zoom):
                            maxwidth(72.0 * base_zoom):
                            diffuse(bright[0], bright[1], bright[2], bright[3]):
                            z(71)
                        ));

                        if show_blue_ms_label && row_i == 0 {
                            let y = y_label + (12.0 * base_zoom);
                            actors.push(act!(text:
                                font("miso"): settext(blue_window_label.clone()):
                                align(1.0, 0.5): horizalign(right):
                                xy(label_x, y):
                                zoom(0.6 * base_zoom):
                                maxwidth(72.0 * base_zoom):
                                diffuse(bright[0], bright[1], bright[2], bright[3]):
                                z(71)
                            ));
                        }
                    }
                } else {
                    let wc = gameplay::display_window_counts(state, 0, Some(blue_window_ms));
                    let counts = [wc.w0, wc.w1, wc.w2, wc.w3, wc.w4, wc.w5, wc.miss];
                    let bright_colors = [
                        color::JUDGMENT_RGBA[0],
                        color::JUDGMENT_FA_PLUS_WHITE_RGBA,
                        color::JUDGMENT_RGBA[1],
                        color::JUDGMENT_RGBA[2],
                        color::JUDGMENT_RGBA[3],
                        color::JUDGMENT_RGBA[4],
                        color::JUDGMENT_RGBA[5],
                    ];
                    let dim_colors = [
                        color::JUDGMENT_DIM_RGBA[0],
                        color::JUDGMENT_FA_PLUS_WHITE_GAMEPLAY_DIM_RGBA,
                        color::JUDGMENT_DIM_RGBA[1],
                        color::JUDGMENT_DIM_RGBA[2],
                        color::JUDGMENT_DIM_RGBA[3],
                        color::JUDGMENT_DIM_RGBA[4],
                        color::JUDGMENT_DIM_RGBA[5],
                    ];

                    let fa_label = judgment_label(0);
                    let labels = [
                        fa_label.clone(),
                        fa_label,
                        judgment_label(1),
                        judgment_label(2),
                        judgment_label(3),
                        judgment_label(4),
                        judgment_label(5),
                    ];
                    for row_i in 0..labels.len() {
                        let local_y = y_base + (row_i as f32 * row_height);
                        let y_numbers = origin_y + (local_y * base_zoom);
                        let y_label = origin_y + ((local_y + 1.0) * base_zoom);
                        let bright = bright_colors[row_i];
                        let dim = dim_colors[row_i];
                        let count = counts[row_i];
                        let (dim_text, bright_text) = cached_padded_runs(count, digits);
                        let dim_len = dim_text.len() as f32;

                        if !dim_text.is_empty() {
                            actors.push(act!(text:
                                font("wendy_screenevaluation"): settext(dim_text):
                                align(0.0, 0.5): xy(numbers_left_x, y_numbers):
                                zoom(numbers_zoom):
                                diffuse(dim[0], dim[1], dim[2], dim[3]):
                                z(71):
                                horizalign(left)
                            ));
                        }
                        if !bright_text.is_empty() {
                            actors.push(act!(text:
                                font("wendy_screenevaluation"): settext(bright_text):
                                align(0.0, 0.5): xy(numbers_left_x + dim_len * digit_w, y_numbers):
                                zoom(numbers_zoom):
                                diffuse(bright[0], bright[1], bright[2], bright[3]):
                                z(71):
                                horizalign(left)
                            ));
                        }

                        actors.push(act!(text:
                            font("miso"): settext(labels[row_i].clone()):
                            align(1.0, 0.5): horizalign(right):
                            xy(label_x, y_label):
                            zoom(label_zoom):
                            maxwidth(72.0 * base_zoom):
                            diffuse(bright[0], bright[1], bright[2], bright[3]):
                            z(71)
                        ));

                        if show_blue_ms_label && row_i == 0 {
                            let y = y_label + (12.0 * base_zoom);
                            actors.push(act!(text:
                                font("miso"): settext(blue_window_label.clone()):
                                align(1.0, 0.5): horizalign(right):
                                xy(label_x, y):
                                zoom(0.6 * base_zoom):
                                maxwidth(72.0 * base_zoom):
                                diffuse(bright[0], bright[1], bright[2], bright[3]):
                                z(71)
                            ));
                        }
                    }
                }
            });
        });
    }

    // HoldsMinesRolls.lua (double): x(-GetNotefieldWidth() + 212), y(-10), zoom(0.8)
    {
        let frame_cx = pane_cx + ((-notefield_width + 212.0) * banner_data_zoom);
        // Our holds/mines/rolls builder positions the frame origin at the *middle* row (Mines),
        // matching the non-double path where SL uses y=-140 and row2 is at y=28.
        // For double, SL uses y=-10 and zoom=0.8, so the middle row sits at:
        // -10 + (0.8 * 28) == 12.4
        let frame_cy = pane_cy + ((-10.0 + 0.8 * 28.0) * banner_data_zoom);
        let frame_zoom = 0.8 * banner_data_zoom;

        actors.extend(build_holds_mines_rolls_pane_at(
            state,
            asset_manager,
            frame_cx,
            frame_cy,
            frame_zoom,
        ));
    }

    // Scorebox.lua (double): x(GetNotefieldWidth() - 140), y(-115)
    {
        let frame_cx = pane_cx + ((notefield_width - 140.0) * banner_data_zoom);
        let frame_cy = pane_cy + (-115.0 * banner_data_zoom);
        let frame_zoom = banner_data_zoom;
        let side = profile::get_session_player_side();
        let snapshot = gameplay::scorebox_snapshot_for_side(state, side);
        actors.extend(gs_scorebox::gameplay_scorebox_actors_from_snapshot(
            side,
            snapshot,
            profile::get_for_side(side).display_scorebox,
            frame_cx,
            frame_cy,
            frame_zoom,
            state.current_music_time_display,
        ));
    }

    // Time.lua (double): x(-GetNotefieldWidth() + 150), y(75)
    {
        let base_x = pane_cx + ((-notefield_width + 150.0) * banner_data_zoom);
        let base_y = pane_cy + (75.0 * banner_data_zoom);

        let base_total = state.song.total_length_seconds.max(0) as f32;
        let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
            state.music_rate
        } else {
            1.0
        };
        let total_display_seconds = if rate == 0.0 {
            base_total
        } else {
            base_total / rate
        };
        let elapsed_display_seconds = if rate == 0.0 {
            state.current_music_time_display.max(0.0)
        } else {
            state.current_music_time_display.max(0.0) / rate
        };

        let total_time_key = game_time_key(total_display_seconds, total_display_seconds);
        let total_time_str = cached_game_time(total_time_key.0, total_time_key.1);
        let remaining_display_seconds = if let Some(fail_time) = state.players[0].fail_time {
            let fail_disp = if rate == 0.0 {
                fail_time.max(0.0)
            } else {
                fail_time.max(0.0) / rate
            };
            (total_display_seconds - fail_disp).max(0.0)
        } else {
            (total_display_seconds - elapsed_display_seconds).max(0.0)
        };
        let remaining_time_key = game_time_key(remaining_display_seconds, total_display_seconds);
        let remaining_time_str = cached_game_time(remaining_time_key.0, remaining_time_key.1);

        let number_zoom = banner_data_zoom;
        let label_zoom = 0.833 * number_zoom;
        let total_w = cached_game_time_width_for_key(total_time_key, asset_manager);

        // Simply Love (Time.lua):
        // label x = 32 + (total_width - 28) == total_width + 4
        let label_x = base_x + (total_w + 4.0) * number_zoom;

        // Remaining row (y=0)
        actors.push(act!(text:
            font("miso"):
            settext(remaining_time_str):
            align(-1.2, 0.5):
            xy(base_x, base_y):
            zoom(number_zoom):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(71)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(time_remaining_right_text()):
            align(1.0, 0.5):
            horizalign(right):
            xy(label_x, base_y + 1.0 * number_zoom):
            zoom(label_zoom):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(71)
        ));

        // Total row (y=20)
        actors.push(act!(text:
            font("miso"):
            settext(total_time_str):
            align(-1.2, 0.5):
            xy(base_x, base_y + (20.0 * number_zoom)):
            zoom(number_zoom):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(71)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(time_song_right_text()):
            align(1.0, 0.5):
            horizalign(right):
            xy(label_x, base_y + (20.0 * number_zoom) + 1.0 * number_zoom):
            zoom(label_zoom):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(71)
        ));
    }

    // Peak NPS text (DensityGraph.lua drives this in SL).
    {
        let scaled_peak = (state.charts[0].max_nps as f32 * state.music_rate).max(0.0);
        let peak_nps_text = cached_peak_nps_text(scaled_peak);
        // Simply Love computes this inside DensityGraph.lua with a funky halign() in double,
        // but the visual intent is that the Peak NPS label lives in the right dark pane.
        let x = pane_cx + nf_half_w + 96.0;
        let y = screen_center_y() + 126.0;
        actors.push(act!(text:
            font("miso"):
            settext(peak_nps_text):
            align(1.0, 0.5):
            xy(x, y):
            zoom(0.9):
            diffuse(1.0, 1.0, 1.0, 1.0):
            horizalign(right):
            z(200)
        ));
    }

    actors
}

// --- Statics for Judgment Counter Display ---

static JUDGMENT_ORDER: [JudgeGrade; 6] = [
    JudgeGrade::Fantastic,
    JudgeGrade::Excellent,
    JudgeGrade::Great,
    JudgeGrade::Decent,
    JudgeGrade::WayOff,
    JudgeGrade::Miss,
];

fn judgment_info(grade: JudgeGrade) -> &'static LabeledColor {
    &JUDGMENT_INFO[judgment::judge_grade_ix(grade)]
}

fn build_banner(
    actors: &mut Vec<Actor>,
    state: &State,
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: profile::PlayerSide,
) {
    if let Some(banner_path) = &state.song.banner_path {
        let banner_key = banner_path.to_string_lossy().into_owned();
        let mut local_banner_x = 70.0;
        if layout.note_field_is_centered && wide {
            local_banner_x = 72.0;
        }
        if player_side == profile::PlayerSide::P2 {
            local_banner_x *= -1.0;
        }
        if layout.is_ultrawide && state.num_players > 1 {
            local_banner_x *= -1.0;
        }
        let local_banner_y = -200.0;
        let banner_x = layout.sidepane_center_x + (local_banner_x * layout.banner_data_zoom);
        let banner_y = layout.sidepane_center_y + (local_banner_y * layout.banner_data_zoom);
        let final_zoom = 0.4 * layout.banner_data_zoom;
        actors.push(act!(sprite(banner_key):
            align(0.5, 0.5): xy(banner_x, banner_y):
            setsize(418.0, 164.0): zoom(final_zoom):
            z(-50)
        ));
    }
}

fn build_pack_banner(
    actors: &mut Vec<Actor>,
    state: &State,
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: profile::PlayerSide,
) {
    if !wide {
        return;
    }
    let Some(pack_banner_path) = state.pack_banner_path.as_ref() else {
        return;
    };
    let pack_key = pack_banner_path.to_string_lossy().into_owned();

    let x_sign = match player_side {
        profile::PlayerSide::P1 => 1.0,
        profile::PlayerSide::P2 => -1.0,
    };

    let (final_offset, final_size) = if layout.note_field_is_centered {
        (-115.0, 0.2)
    } else {
        (-160.0, 0.25)
    };
    let x = layout.sidepane_center_x + (final_offset * x_sign * layout.banner_data_zoom);
    let y = layout.sidepane_center_y + (20.0 * layout.banner_data_zoom);

    actors.push(act!(sprite(pack_key):
        align(0.5, 0.5):
        xy(x, y):
        setsize(418.0, 164.0):
        zoom(final_size * layout.banner_data_zoom):
        z(-49)
    ));
}

fn build_steps_info(
    actors: &mut Vec<Actor>,
    state: &State,
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: profile::PlayerSide,
) {
    if !wide {
        return;
    }
    actors.reserve(if layout.note_field_is_centered { 5 } else { 9 });

    // Dark background for the Step Statistics side pane (Simply Love: DarkBackground.lua).
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(layout.sidepane_center_x, screen_center_y()):
        zoomto(layout.sidepane_width, screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.95):
        z(-80)
    ));
    let note_field_is_centered = layout.note_field_is_centered;
    let banner_data_zoom = layout.banner_data_zoom;

    let player_idx = match (state.num_players, player_side) {
        (2, profile::PlayerSide::P2) => 1,
        _ => 0,
    };
    let chart = &state.charts[player_idx];
    let desc = chart.description.trim();
    let cred = chart.step_artist.trim();

    let mut cycle = [None::<&str>; 2];
    let mut cycle_len = 0usize;
    if !desc.is_empty() {
        cycle[cycle_len] = Some(desc);
        cycle_len += 1;
    }
    if !cred.is_empty() && cred != desc && cycle_len < cycle.len() {
        cycle[cycle_len] = Some(cred);
        cycle_len += 1;
    }
    let desc_text = if cycle_len == 0 {
        ""
    } else {
        let idx = ((state.total_elapsed_in_screen / 2.0).floor() as usize) % cycle_len;
        cycle[idx].unwrap_or("")
    };

    let ar = screen_width() / screen_height().max(1.0);
    let pnum = match player_side {
        profile::PlayerSide::P1 => 1,
        profile::PlayerSide::P2 => 2,
    };
    let pos_sign = if pnum == 1 { -1.0 } else { 1.0 };

    let mut x = -190.0;
    let xoffset = if pnum == 1 { 285.0 } else { 0.0 };
    let mut yoffset = 0.0;
    let mut zoom = 0.75;
    let mut xvalues = 45.0;
    let mut maxwidth = 320.0;
    if note_field_is_centered {
        xvalues = 0.0;
        yoffset = -5.0;
        if ar > 1.7 {
            x = if pnum == 1 { -220.0 } else { -150.0 };
            maxwidth = 240.0;
            zoom = 0.9;
        } else {
            x = if pnum == 1 { -240.0 } else { -150.0 };
            maxwidth = 210.0;
            zoom = 0.95;
        }
    }

    let origin_x = layout.sidepane_center_x + ((x + xoffset) * pos_sign * banner_data_zoom);
    let origin_y = layout.sidepane_center_y + ((-8.0 + yoffset) * banner_data_zoom);
    let group_zoom = zoom * banner_data_zoom;

    let row_h = 16.0;
    let z = 72i16;
    if !note_field_is_centered {
        for i in 0..4 {
            let y = origin_y + (row_h * (i as f32 + 1.0) * group_zoom);
            actors.push(act!(text:
                font("miso"): settext(step_info_label_text(i)):
                align(0.0, 0.5): xy(origin_x, y):
                zoom(group_zoom): z(z):
                horizalign(left)
            ));
        }
    }

    let values_x = origin_x + (xvalues * group_zoom);
    let y_song = origin_y + (row_h * 1.0 * group_zoom);
    actors.push(act!(text:
        font("miso"): settext(state.song_full_title.clone()):
        align(0.0, 0.5): xy(values_x, y_song):
        maxwidth(maxwidth):
        zoom(group_zoom): z(z):
        horizalign(left)
    ));
    let y_artist = origin_y + (row_h * 2.0 * group_zoom);
    actors.push(act!(text:
        font("miso"): settext(cached_str_ref(state.song.artist.as_str())):
        align(0.0, 0.5): xy(values_x, y_artist):
        maxwidth(maxwidth):
        zoom(group_zoom): z(z):
        horizalign(left)
    ));
    let y_pack = origin_y + (row_h * 3.0 * group_zoom);
    actors.push(act!(text:
        font("miso"): settext(state.pack_group.clone()):
        align(0.0, 0.5): xy(values_x, y_pack):
        maxwidth(maxwidth):
        zoom(group_zoom): z(z):
        horizalign(left)
    ));
    let y_desc = origin_y + (row_h * 4.0 * group_zoom);
    actors.push(act!(text:
        font("miso"): settext(cached_str_ref(desc_text)):
        align(0.0, 0.5): xy(values_x, y_desc):
        maxwidth(maxwidth):
        zoom(group_zoom): z(z):
        horizalign(left)
    ));
}

fn build_holds_mines_rolls_pane_at(
    state: &State,
    asset_manager: &AssetManager,
    frame_cx: f32,
    frame_cy: f32,
    frame_zoom: f32,
) -> Vec<Actor> {
    let p = &state.players[0];
    let mut actors = Vec::with_capacity(1);

    let categories = [
        (0usize, p.holds_held, state.holds_total[0]),
        (1usize, p.mines_avoided, state.mines_total[0]),
        (2usize, p.rolls_held, state.rolls_total[0]),
    ];

    let largest_count = categories
        .iter()
        .map(|(_, achieved, total)| (*achieved).max(*total))
        .max()
        .unwrap_or(0);
    let digits_needed = if largest_count == 0 {
        1
    } else {
        (largest_count as f32).log10().floor() as usize + 1
    };
    let digits_to_fmt = digits_needed.clamp(3, 4);
    let row_height = 28.0 * frame_zoom;
    let mut children = Vec::with_capacity(categories.len() * (digits_to_fmt * 2 + 2));

    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("wendy_screenevaluation", |metrics_font| {
            let value_zoom = 0.4 * frame_zoom;
            let label_zoom = 0.833 * frame_zoom;
            const GRAY: [f32; 4] = color::rgba_hex("#5A6166");
            let white = [1.0, 1.0, 1.0, 1.0];

            let digit_width = glyph_width_scaled(metrics_font, all_fonts, '0', value_zoom);
            if digit_width <= 0.0 {
                return;
            }
            let slash_width = glyph_width_scaled(metrics_font, all_fonts, '/', value_zoom);

            const LOGICAL_CHAR_WIDTH_FOR_LABEL: f32 = 36.0;
            let fixed_char_width_scaled_for_label = LOGICAL_CHAR_WIDTH_FOR_LABEL * value_zoom;

            for (i, (label_index, achieved, total)) in categories.iter().enumerate() {
                let item_y = (i as f32 - 1.0) * row_height;
                let right_anchor_x = 0.0;
                let mut cursor_x = right_anchor_x;

                let possible_str = cached_padded_num(*total, digits_to_fmt);
                let achieved_str = cached_padded_num(*achieved, digits_to_fmt);
                let possible_bytes = possible_str.as_bytes();
                let achieved_bytes = achieved_str.as_bytes();
                let possible_split = padded_dim_len(possible_str.as_ref(), *total, digits_to_fmt);
                let achieved_split =
                    padded_dim_len(achieved_str.as_ref(), *achieved, digits_to_fmt);

                for char_idx in 0..possible_bytes.len() {
                    let original_index = possible_bytes.len() - 1 - char_idx;
                    let color = if original_index < possible_split { GRAY } else { white };
                    let x_pos = cursor_x - (char_idx as f32 * digit_width);
                    children.push(act!(text:
                        font("wendy_screenevaluation"): settext(digit_text(possible_bytes[original_index])):
                        align(1.0, 0.5): xy(x_pos, item_y):
                        zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3])
                    ));
                }
                cursor_x -= possible_bytes.len() as f32 * digit_width;

                children.push(act!(text:
                    font("wendy_screenevaluation"): settext(SLASH_TEXT.clone()):
                    align(1.0, 0.5): xy(cursor_x, item_y):
                    zoom(value_zoom): diffuse(GRAY[0], GRAY[1], GRAY[2], GRAY[3])
                ));
                cursor_x -= slash_width;

                for char_idx in 0..achieved_bytes.len() {
                    let original_index = achieved_bytes.len() - 1 - char_idx;
                    let color = if original_index < achieved_split { GRAY } else { white };
                    let x_pos = cursor_x - (char_idx as f32 * digit_width);
                    children.push(act!(text:
                        font("wendy_screenevaluation"): settext(digit_text(achieved_bytes[original_index])):
                        align(1.0, 0.5): xy(x_pos, item_y):
                        zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3])
                    ));
                }

                let total_value_width_for_label = (achieved_str.len() + 1 + possible_str.len())
                    as f32
                    * fixed_char_width_scaled_for_label;
                let label_x = right_anchor_x - total_value_width_for_label - (10.0 * frame_zoom);

                children.push(act!(text:
                    font("miso"): settext(holds_mines_rolls_label_text(*label_index)):
                    align(1.0, 0.5): xy(label_x, item_y):
                    zoom(label_zoom):
                    horizalign(right):
                    diffuse(white[0], white[1], white[2], white[3])
                ));
            }
        });
    });

    actors.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [frame_cx, frame_cy],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z: 70,
    });
    actors
}

fn notefield_width(state: &State) -> Option<f32> {
    let ns = state.noteskin[0].as_ref()?;
    let field_zoom = state.field_zoom[0];
    let cols = state
        .cols_per_player
        .min(ns.column_xs.len())
        .min(ns.receptor_off.len());
    if cols == 0 {
        return None;
    }

    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    for x in ns.column_xs.iter().take(cols) {
        let xf = *x as f32;
        min_x = min_x.min(xf);
        max_x = max_x.max(xf);
    }

    let target_arrow_px = 64.0 * field_zoom.max(0.0);
    let size = ns.receptor_off[0].size();
    let w = size[0].max(0) as f32;
    let h = size[1].max(0) as f32;
    let arrow_w = if h > 0.0 && target_arrow_px > 0.0 {
        w * (target_arrow_px / h)
    } else {
        w * field_zoom.max(0.0)
    };

    Some(((max_x - min_x) * field_zoom.max(0.0)) + arrow_w)
}

fn build_holds_mines_rolls_pane(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: profile::PlayerSide,
) {
    if !wide {
        return;
    }
    let p = &state.players[0];
    let banner_data_zoom = layout.banner_data_zoom;
    let local_x = match player_side {
        profile::PlayerSide::P1 => 155.0,
        profile::PlayerSide::P2 => -85.0,
    };
    let local_y = -112.0;
    let frame_cx = layout.sidepane_center_x + (local_x * banner_data_zoom);
    let frame_cy = layout.sidepane_center_y + (local_y * banner_data_zoom);
    let frame_zoom = banner_data_zoom;

    let categories = [
        (0usize, p.holds_held, state.holds_total[0]),
        (1usize, p.mines_avoided, state.mines_total[0]),
        (2usize, p.rolls_held, state.rolls_total[0]),
    ];

    let largest_count = categories
        .iter()
        .map(|(_, achieved, total)| (*achieved).max(*total))
        .max()
        .unwrap_or(0);
    let digits_needed = if largest_count == 0 {
        1
    } else {
        (largest_count as f32).log10().floor() as usize + 1
    };
    let digits_to_fmt = digits_needed.clamp(3, 4);
    let row_height = 28.0 * frame_zoom;
    let mut children = Vec::with_capacity(categories.len() * (digits_to_fmt * 2 + 2));

    asset_manager.with_fonts(|all_fonts| asset_manager.with_font("wendy_screenevaluation", |metrics_font| {
        let value_zoom = 0.4 * frame_zoom;
        let label_zoom = 0.833 * frame_zoom;
        let gray = color::rgba_hex("#5A6166");
        let white = [1.0, 1.0, 1.0, 1.0];

        // --- HYBRID LAYOUT LOGIC ---
        // 1. Measure real character widths for number layout.
        let digit_width = glyph_width_scaled(metrics_font, all_fonts, '0', value_zoom);
        if digit_width <= 0.0 { return; }
        let slash_width = glyph_width_scaled(metrics_font, all_fonts, '/', value_zoom);

        // 2. Use a hardcoded width for calculating the label's position (for theme parity).
        const LOGICAL_CHAR_WIDTH_FOR_LABEL: f32 = 36.0;
        let fixed_char_width_scaled_for_label = LOGICAL_CHAR_WIDTH_FOR_LABEL * value_zoom;

        for (i, (label_index, achieved, total)) in categories.iter().enumerate() {
            let item_y = (i as f32 - 1.0) * row_height;
            let right_anchor_x = match player_side {
                profile::PlayerSide::P1 => 0.0,
                profile::PlayerSide::P2 => 100.0 * frame_zoom,
            };
            let mut cursor_x = right_anchor_x;

            let possible_str = cached_padded_num(*total, digits_to_fmt);
            let achieved_str = cached_padded_num(*achieved, digits_to_fmt);
            let possible_bytes = possible_str.as_bytes();
            let achieved_bytes = achieved_str.as_bytes();
            let possible_split = padded_dim_len(possible_str.as_ref(), *total, digits_to_fmt);
            let achieved_split = padded_dim_len(achieved_str.as_ref(), *achieved, digits_to_fmt);

            // --- Layout Numbers using MEASURED widths ---
            // 1. Draw "possible" number (right-most part)
            for char_idx in 0..possible_bytes.len() {
                let original_index = possible_bytes.len() - 1 - char_idx;
                let color = if original_index < possible_split {
                    gray
                } else {
                    white
                };
                let x_pos = cursor_x - (char_idx as f32 * digit_width);
                children.push(act!(text:
                    font("wendy_screenevaluation"): settext(digit_text(possible_bytes[original_index])):
                    align(1.0, 0.5): xy(x_pos, item_y):
                    zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3])
                ));
            }
            cursor_x -= possible_bytes.len() as f32 * digit_width;

            // 2. Draw slash
            children.push(act!(text: font("wendy_screenevaluation"): settext(SLASH_TEXT.clone()): align(1.0, 0.5): xy(cursor_x, item_y): zoom(value_zoom): diffuse(gray[0], gray[1], gray[2], gray[3])));
            cursor_x -= slash_width;

            // 3. Draw "achieved" number
            for char_idx in 0..achieved_bytes.len() {
                let original_index = achieved_bytes.len() - 1 - char_idx;
                let color = if original_index < achieved_split {
                    gray
                } else {
                    white
                };
                let x_pos = cursor_x - (char_idx as f32 * digit_width);
                children.push(act!(text:
                    font("wendy_screenevaluation"): settext(digit_text(achieved_bytes[original_index])):
                    align(1.0, 0.5): xy(x_pos, item_y):
                    zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3])
                ));
            }

            // --- Position Label using HARDCODED width assumption ---
            let total_value_width_for_label = (achieved_str.len() + 1 + possible_str.len()) as f32 * fixed_char_width_scaled_for_label;
            let label_x = right_anchor_x - total_value_width_for_label - (10.0 * frame_zoom);

            children.push(act!(text:
                font("miso"): settext(holds_mines_rolls_label_text(*label_index)): align(1.0, 0.5): xy(label_x, item_y):
                zoom(label_zoom): horizalign(right): diffuse(white[0], white[1], white[2], white[3])
            ));
        }
    }));

    actors.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [frame_cx, frame_cy],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z: 70,
    });
}

fn build_scorebox_pane(
    actors: &mut Vec<Actor>,
    state: &State,
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: profile::PlayerSide,
) {
    if !wide {
        return;
    }

    let x_sign = match player_side {
        profile::PlayerSide::P1 => 1.0,
        profile::PlayerSide::P2 => -1.0,
    };
    let mut local_x = 70.0 * x_sign;
    if layout.note_field_is_centered && wide {
        local_x += 2.0 * x_sign;
    }
    if layout.is_ultrawide && state.num_players > 1 {
        local_x = -local_x;
    }
    let frame_cx = layout.sidepane_center_x + (local_x * layout.banner_data_zoom);
    let frame_cy = layout.sidepane_center_y + (-115.0 * layout.banner_data_zoom);

    actors.extend(gs_scorebox::gameplay_scorebox_actors_from_snapshot(
        player_side,
        gameplay::scorebox_snapshot_for_side(state, player_side),
        profile::get_for_side(player_side).display_scorebox,
        frame_cx,
        frame_cy,
        layout.banner_data_zoom,
        state.current_music_time_display,
    ));
}

fn build_side_pane(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: profile::PlayerSide,
) {
    if !wide {
        return;
    }

    let x_sign = match player_side {
        profile::PlayerSide::P1 => 1.0,
        profile::PlayerSide::P2 => -1.0,
    };
    let player_idx = step_stats_player_idx(state, player_side);
    let judgments_local_x = if layout.is_ultrawide && state.num_players > 1 {
        154.0 * x_sign
    } else if layout.note_field_is_centered && wide {
        -156.0 * x_sign
    } else {
        -widescale(152.0, 204.0) * x_sign
    };
    let final_judgments_center_x =
        layout.sidepane_center_x + (judgments_local_x * layout.banner_data_zoom);
    let final_judgments_center_y = layout.sidepane_center_y;
    let parent_local_zoom = 0.8;
    let final_text_base_zoom = layout.banner_data_zoom * parent_local_zoom;

    let total_tapnotes = state.charts[player_idx].stats.total_steps as f32;
    let digits = if total_tapnotes > 0.0 {
        (total_tapnotes.log10().floor() as usize + 1).max(4)
    } else {
        4
    };
    let extra_digits = digits.saturating_sub(4) as f32;
    let base_label_local_x_offset = 80.0;
    const LABEL_DIGIT_STEP: f32 = 16.0;
    const NUMBER_TO_LABEL_GAP: f32 = 8.0;
    let base_numbers_local_x_offset = base_label_local_x_offset - NUMBER_TO_LABEL_GAP;
    let show_fa_plus_window = state.player_profiles[player_idx].show_fa_plus_window;
    let player_profile = &state.player_profiles[player_idx];
    let show_fa_split = show_fa_plus_window || player_profile.custom_fantastic_window;
    let show_blue_ms_label = player_profile.custom_fantastic_window
        || (show_fa_plus_window && player_profile.fa_plus_10ms_blue_window);
    let blue_window_ms = gameplay::player_blue_window_ms(state, player_idx);
    let blue_window_label = cached_blue_window_label(blue_window_ms.round() as i32);
    actors.reserve(if show_fa_split {
        22 + usize::from(show_blue_ms_label)
    } else {
        16
    });
    let row_height = if show_fa_split { 29.0 } else { 35.0 };
    let y_base = -280.0;

    asset_manager.with_fonts(|all_fonts| asset_manager.with_font("wendy_screenevaluation", |f| {
        let numbers_zoom = final_text_base_zoom * 0.5;
        let max_digit_w = glyph_width_scaled(f, all_fonts, '0', numbers_zoom);
        if max_digit_w <= 0.0 { return; }

        let digit_local_width = max_digit_w / final_text_base_zoom;
        let label_local_x_offset = base_label_local_x_offset + (extra_digits * LABEL_DIGIT_STEP);
        let label_world_x =
            final_judgments_center_x + (x_sign * label_local_x_offset * final_text_base_zoom);
        let numbers_local_x_offset = base_numbers_local_x_offset + (extra_digits * digit_local_width);
        let numbers_cx =
            final_judgments_center_x + (x_sign * numbers_local_x_offset * final_text_base_zoom);
        let show_standard_judgments = !show_fa_split;

        if show_standard_judgments {
            // Standard ITG-style rows: Fantastic..Miss using aggregate grade counts.
            for (index, grade) in JUDGMENT_ORDER.iter().enumerate() {
                let info = judgment_info(*grade);
                let count = gameplay::display_judgment_count(state, 0, *grade);

                let local_y = y_base + (index as f32 * row_height);
                let world_y = final_judgments_center_y + (local_y * final_text_base_zoom);

                let bright = info.color;
                let dim = color::JUDGMENT_DIM_RGBA[index];
                let (dim_text, bright_text) = cached_padded_runs(count, digits);
                let dim_len = dim_text.len() as f32;
                let bright_len = bright_text.len() as f32;

                if player_side == profile::PlayerSide::P1 {
                    if !bright_text.is_empty() {
                        actors.push(act!(text:
                            font("wendy_screenevaluation"): settext(bright_text):
                            align(1.0, 0.5): xy(numbers_cx, world_y): zoom(numbers_zoom):
                            diffuse(bright[0], bright[1], bright[2], bright[3]): z(71)
                        ));
                    }
                    if !dim_text.is_empty() {
                        actors.push(act!(text:
                            font("wendy_screenevaluation"): settext(dim_text):
                            align(1.0, 0.5): xy(numbers_cx - bright_len * max_digit_w, world_y):
                            zoom(numbers_zoom):
                            diffuse(dim[0], dim[1], dim[2], dim[3]): z(71)
                        ));
                    }
                } else {
                    if !dim_text.is_empty() {
                        actors.push(act!(text:
                            font("wendy_screenevaluation"): settext(dim_text):
                            align(0.0, 0.5): xy(numbers_cx, world_y): zoom(numbers_zoom):
                            diffuse(dim[0], dim[1], dim[2], dim[3]): z(71):
                            horizalign(left)
                        ));
                    }
                    if !bright_text.is_empty() {
                        actors.push(act!(text:
                            font("wendy_screenevaluation"): settext(bright_text):
                            align(0.0, 0.5): xy(numbers_cx + dim_len * max_digit_w, world_y):
                            zoom(numbers_zoom):
                            diffuse(bright[0], bright[1], bright[2], bright[3]): z(71):
                            horizalign(left)
                        ));
                    }
                }

                let label_world_y = world_y + (1.0 * final_text_base_zoom);
                let label_zoom = final_text_base_zoom * 0.833;
                let label = info.label.get();

                if player_side == profile::PlayerSide::P1 {
                    actors.push(act!(text:
                        font("miso"): settext(label): align(0.0, 0.5):
                        xy(label_world_x, label_world_y): zoom(label_zoom):
                        maxwidth(72.0 * final_text_base_zoom): horizalign(left):
                        diffuse(bright[0], bright[1], bright[2], bright[3]):
                        z(71)
                    ));
                } else {
                    actors.push(act!(text:
                        font("miso"): settext(label): align(1.0, 0.5):
                        xy(label_world_x, label_world_y): zoom(label_zoom):
                        maxwidth(72.0 * final_text_base_zoom): horizalign(right):
                        diffuse(bright[0], bright[1], bright[2], bright[3]):
                        z(71)
                    ));
                }
            }
        } else {
            // FA+ mode: split Fantastic into W0 (blue) and W1 (white) using per-note windows,
            // matching Simply Love's FA+ Step Statistics semantics.
            let wc = gameplay::display_window_counts(state, player_idx, Some(blue_window_ms));
            let fantastic_color = judgment_info(JudgeGrade::Fantastic).color;
            let excellent_color = judgment_info(JudgeGrade::Excellent).color;
            let great_color = judgment_info(JudgeGrade::Great).color;
            let decent_color = judgment_info(JudgeGrade::Decent).color;
            let wayoff_color = judgment_info(JudgeGrade::WayOff).color;
            let miss_color = judgment_info(JudgeGrade::Miss).color;

            // Dim palette for FA+ side pane: reuse gameplay dim colors for Fantastic..Miss,
            // and a dedicated dim color for the white FA+ row.
            let dim_fantastic = color::JUDGMENT_DIM_RGBA[0];
            let dim_excellent = color::JUDGMENT_DIM_RGBA[1];
            let dim_great = color::JUDGMENT_DIM_RGBA[2];
            let dim_decent = color::JUDGMENT_DIM_RGBA[3];
            let dim_wayoff = color::JUDGMENT_DIM_RGBA[4];
            let dim_miss = color::JUDGMENT_DIM_RGBA[5];
            let dim_white_fa = color::JUDGMENT_FA_PLUS_WHITE_GAMEPLAY_DIM_RGBA;

            let white_fa_color = color::JUDGMENT_FA_PLUS_WHITE_RGBA;

            let rows: [(usize, [f32; 4], [f32; 4], u32); 7] = [
                (0, fantastic_color, dim_fantastic, wc.w0),
                (0, white_fa_color, dim_white_fa, wc.w1),
                (1, excellent_color, dim_excellent, wc.w2),
                (2, great_color, dim_great, wc.w3),
                (3, decent_color, dim_decent, wc.w4),
                (4, wayoff_color, dim_wayoff, wc.w5),
                (5, miss_color, dim_miss, wc.miss),
            ];

            for (index, (label_index, bright, dim, count)) in rows.iter().enumerate() {
                let local_y = y_base + (index as f32 * row_height);
                let world_y = final_judgments_center_y + (local_y * final_text_base_zoom);

                let (dim_text, bright_text) = cached_padded_runs(*count, digits);
                let dim_len = dim_text.len() as f32;
                let bright_len = bright_text.len() as f32;

                if player_side == profile::PlayerSide::P1 {
                    if !bright_text.is_empty() {
                        actors.push(act!(text:
                            font("wendy_screenevaluation"): settext(bright_text):
                            align(1.0, 0.5): xy(numbers_cx, world_y): zoom(numbers_zoom):
                            diffuse(bright[0], bright[1], bright[2], bright[3]): z(71)
                        ));
                    }
                    if !dim_text.is_empty() {
                        actors.push(act!(text:
                            font("wendy_screenevaluation"): settext(dim_text):
                            align(1.0, 0.5): xy(numbers_cx - bright_len * max_digit_w, world_y):
                            zoom(numbers_zoom):
                            diffuse(dim[0], dim[1], dim[2], dim[3]): z(71)
                        ));
                    }
                } else {
                    if !dim_text.is_empty() {
                        actors.push(act!(text:
                            font("wendy_screenevaluation"): settext(dim_text):
                            align(0.0, 0.5): xy(numbers_cx, world_y): zoom(numbers_zoom):
                            diffuse(dim[0], dim[1], dim[2], dim[3]): z(71):
                            horizalign(left)
                        ));
                    }
                    if !bright_text.is_empty() {
                        actors.push(act!(text:
                            font("wendy_screenevaluation"): settext(bright_text):
                            align(0.0, 0.5): xy(numbers_cx + dim_len * max_digit_w, world_y):
                            zoom(numbers_zoom):
                            diffuse(bright[0], bright[1], bright[2], bright[3]): z(71):
                            horizalign(left)
                        ));
                    }
                }

                let label_world_y = world_y + (1.0 * final_text_base_zoom);
                let label_zoom = final_text_base_zoom * 0.833;
                let sublabel_y = label_world_y + (12.0 * final_text_base_zoom);
                let sublabel_zoom = final_text_base_zoom * 0.6;
                let label = judgment_label(*label_index);

                if player_side == profile::PlayerSide::P1 {
                    actors.push(act!(text:
                        font("miso"): settext(label): align(0.0, 0.5):
                        xy(label_world_x, label_world_y): zoom(label_zoom):
                        maxwidth(72.0 * final_text_base_zoom): horizalign(left):
                        diffuse(bright[0], bright[1], bright[2], bright[3]):
                        z(71)
                    ));
                    if show_blue_ms_label && index == 0 {
                        actors.push(act!(text:
                            font("miso"): settext(blue_window_label.clone()): align(0.0, 0.5):
                            xy(label_world_x, sublabel_y): zoom(sublabel_zoom):
                            maxwidth(72.0 * final_text_base_zoom): horizalign(left):
                            diffuse(bright[0], bright[1], bright[2], bright[3]):
                            z(71)
                        ));
                    }
                } else {
                    actors.push(act!(text:
                        font("miso"): settext(label): align(1.0, 0.5):
                        xy(label_world_x, label_world_y): zoom(label_zoom):
                        maxwidth(72.0 * final_text_base_zoom): horizalign(right):
                        diffuse(bright[0], bright[1], bright[2], bright[3]):
                        z(71)
                    ));
                    if show_blue_ms_label && index == 0 {
                        actors.push(act!(text:
                            font("miso"): settext(blue_window_label.clone()): align(1.0, 0.5):
                            xy(label_world_x, sublabel_y): zoom(sublabel_zoom):
                            maxwidth(72.0 * final_text_base_zoom): horizalign(right):
                            diffuse(bright[0], bright[1], bright[2], bright[3]):
                            z(71)
                        ));
                    }
                }
            }
        }

        // --- Time Display (Remaining / Total) ---
        {
            let local_y = -40.0 * layout.banner_data_zoom;

            // Base chart length in seconds (GetLastSecond semantics).
            let base_total = state.song.total_length_seconds.max(0) as f32;
            // Displayed duration should respect music rate (SongLength / MusicRate),
            // while the on-screen timer still advances in real seconds.
            let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
                state.music_rate
            } else {
                1.0
            };
            let total_display_seconds = if rate == 0.0 {
                base_total
            } else {
                base_total / rate
            };
            let elapsed_display_seconds = if rate == 0.0 {
                state.current_music_time_display.max(0.0)
            } else {
                state.current_music_time_display.max(0.0) / rate
            };

            let total_time_key = game_time_key(total_display_seconds, total_display_seconds);
            let total_time_str = cached_game_time(total_time_key.0, total_time_key.1);

            let remaining_display_seconds = if let Some(fail_time) = state.players[0].fail_time {
                let fail_disp = if rate == 0.0 {
                    fail_time.max(0.0)
                } else {
                    fail_time.max(0.0) / rate
                };
                (total_display_seconds - fail_disp).max(0.0)
            } else {
                (total_display_seconds - elapsed_display_seconds).max(0.0)
            };
            let remaining_time_key = game_time_key(remaining_display_seconds, total_display_seconds);
            let remaining_time_str =
                cached_game_time(remaining_time_key.0, remaining_time_key.1);

            let font_name = "miso";
            let text_zoom = layout.banner_data_zoom * 0.833;
            // Time values currently render without explicit zoom, so treat as 1.0
            let time_value_zoom = 1.0_f32;

            let numbers_block_width = (digits as f32) * max_digit_w;
            let numbers_left_x = numbers_cx - numbers_block_width + 2.0;

            let total_width_px =
                cached_game_time_width_for_key(total_time_key, asset_manager) * time_value_zoom;
            let remaining_width_px =
                cached_game_time_width_for_key(remaining_time_key, asset_manager) * time_value_zoom;
            // Use "9:59" as the baseline look the layout was tuned for.
            let baseline_width_px =
                cached_game_time_width_for_key((9 * 60 + 59, 2), asset_manager) * time_value_zoom;

            let red_color = color::rgba_hex("#ff3030");
            let white_color = [1.0, 1.0, 1.0, 1.0];
            let remaining_color = if state.players[0].is_failing { red_color } else { white_color };

            // --- Total Time Row ---
            let y_pos_total = layout.sidepane_center_y + local_y + 13.0;
            let label_offset: f32 = 29.0;
            // Keep original spacing for <= 9:59, otherwise push label after the time width
            let desired_gap_px = (label_offset - baseline_width_px).max(4.0_f32);
            let label_offset_total = if total_width_px > baseline_width_px {
                total_width_px + desired_gap_px
            } else {
                label_offset
            };

            let (time_x, label_dir) = if player_side == profile::PlayerSide::P1 {
                (numbers_left_x, 1.0_f32)
            } else {
                let numbers_right_x = numbers_cx + numbers_block_width - 2.0;
                (numbers_right_x, -1.0_f32)
            };

            if player_side == profile::PlayerSide::P1 {
                actors.push(act!(text: font(font_name): settext(total_time_str):
                    align(0.0, 0.5): horizalign(left):
                    xy(time_x, y_pos_total):
                    z(71):
                    diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
                ));
                actors.push(act!(text: font(font_name): settext(time_song_left_text()):
                    align(0.0, 0.5): horizalign(left):
                    xy(time_x + label_dir * label_offset_total, y_pos_total + 1.0):
                    zoom(text_zoom): z(71):
                    diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
                ));
            } else {
                actors.push(act!(text: font(font_name): settext(total_time_str):
                    align(1.0, 0.5): horizalign(right):
                    xy(time_x, y_pos_total):
                    z(71):
                    diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
                ));
                actors.push(act!(text: font(font_name): settext(time_song_left_text()):
                    align(1.0, 0.5): horizalign(right):
                    xy(time_x + label_dir * label_offset_total, y_pos_total + 1.0):
                    zoom(text_zoom): z(71):
                    diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
                ));
            }

            // --- Remaining Time Row ---
            let y_pos_remaining = layout.sidepane_center_y + local_y - 7.0;

            // Keep original spacing for <= 9:59, otherwise push label after the time width
            let label_offset_remaining = if remaining_width_px > baseline_width_px {
                remaining_width_px + desired_gap_px
            } else {
                label_offset
            };

            if player_side == profile::PlayerSide::P1 {
                actors.push(act!(text: font(font_name): settext(remaining_time_str):
                    align(0.0, 0.5): horizalign(left):
                    xy(time_x, y_pos_remaining):
                    z(71):
                    diffuse(remaining_color[0], remaining_color[1], remaining_color[2], remaining_color[3])
                ));
                actors.push(act!(text: font(font_name): settext(time_remaining_left_text()):
                    align(0.0, 0.5): horizalign(left):
                    xy(time_x + label_dir * label_offset_remaining, y_pos_remaining + 1.0):
                    zoom(text_zoom): z(71):
                    diffuse(remaining_color[0], remaining_color[1], remaining_color[2], remaining_color[3])
                ));
            } else {
                actors.push(act!(text: font(font_name): settext(remaining_time_str):
                    align(1.0, 0.5): horizalign(right):
                    xy(time_x, y_pos_remaining):
                    z(71):
                    diffuse(remaining_color[0], remaining_color[1], remaining_color[2], remaining_color[3])
                ));
                actors.push(act!(text: font(font_name): settext(time_remaining_left_text()):
                    align(1.0, 0.5): horizalign(right):
                    xy(time_x + label_dir * label_offset_remaining, y_pos_remaining + 1.0):
                    zoom(text_zoom): z(71):
                    diffuse(remaining_color[0], remaining_color[1], remaining_color[2], remaining_color[3])
                ));
            }
        }
    }));

    // Density graph (Simply Love StepStatistics/DensityGraph.lua).
    if wide {
        const BG_RGB: [f32; 3] = [
            30.0 / 255.0, // 0x1E
            40.0 / 255.0, // 0x28
            47.0 / 255.0, // 0x2F
        ];

        let graph_h = state.density_graph_graph_h;
        let graph_w = state.density_graph_graph_w;
        if graph_w > 0.0_f32 && graph_h > 0.0_f32 {
            let x0 = layout.sidepane_center_x - graph_w * 0.5;
            let y0 = layout.sidepane_center_y + 55.0;
            let bg_alpha = if state.player_profiles[player_idx].transparent_density_graph_bg {
                0.5
            } else {
                1.0
            };

            actors.push(act!(quad:
                align(0.0, 0.0): xy(x0, y0):
                zoomto(graph_w, graph_h):
                diffuse(BG_RGB[0], BG_RGB[1], BG_RGB[2], bg_alpha):
                z(59)
            ));

            if let Some(mesh) = &state.density_graph.mesh[player_idx]
                && !mesh.is_empty()
            {
                actors.push(Actor::Mesh {
                    align: [0.0, 0.0],
                    offset: [x0, y0],
                    size: [SizeSpec::Px(graph_w), SizeSpec::Px(graph_h)],
                    vertices: mesh.clone(),
                    mode: MeshMode::Triangles,
                    visible: true,
                    blend: BlendMode::Alpha,
                    z: 60,
                });
            }

            if let Some(mesh) = &state.density_graph.life_mesh[player_idx]
                && !mesh.is_empty()
            {
                actors.push(Actor::Mesh {
                    align: [0.0, 0.0],
                    offset: [x0, y0],
                    size: [SizeSpec::Px(graph_w), SizeSpec::Px(graph_h)],
                    vertices: mesh.clone(),
                    mode: MeshMode::Triangles,
                    visible: true,
                    blend: BlendMode::Alpha,
                    z: 61,
                });
            }
        }
    }

    // --- Peak NPS Display (as seen in Simply Love's Step Statistics) ---
    if wide {
        let scaled_peak = (state.charts[0].max_nps as f32 * state.music_rate).max(0.0);
        let peak_nps_text = cached_peak_nps_text(scaled_peak);

        // Positioned based on visual parity with Simply Love's Step Statistics pane
        // for Player 1, which is on the right side of the screen.
        let peak_nps_x = match player_side {
            profile::PlayerSide::P1 => screen_width() - 59.0,
            profile::PlayerSide::P2 => widescale(6.0, 130.0),
        };
        let peak_nps_y = screen_center_y() + 126.0;

        actors.push(act!(text:
            font("miso"):
            settext(peak_nps_text):
            // Pivot point is the text's right-center
            align(1.0, 0.5):
            xy(peak_nps_x, peak_nps_y):
            zoom(0.9):
            diffuse(1.0, 1.0, 1.0, 1.0):
            // Align the text content itself to the right
            horizalign(right):
            z(200)
        ));
    }
}
