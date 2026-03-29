use crate::act;
use crate::assets::{self, AssetManager};
use crate::config::{
    self, BreakdownStyle, NewPackMode, SelectMusicPatternInfoMode, SelectMusicScoreboxPlacement,
    SyncGraphMode,
};
use crate::engine::audio;
use crate::engine::gfx::{BlendMode, MeshMode, MeshVertex, SamplerDesc, SamplerFilter};
use crate::engine::input::{InputEvent, PadDir, RawKeyboardEvent, VirtualAction};
use crate::engine::present::actors::{Actor, SizeSpec, SpriteSource};
use crate::engine::present::color;
use crate::engine::present::font;
use crate::engine::space::{
    current_window_px, is_wide, screen_center_x, screen_center_y, screen_height, screen_width,
    widescale,
};
use crate::game::chart::{ChartData, ChartDisplayBpm};
use crate::game::course;
use crate::game::parsing::simfile as song_loading;
use crate::game::profile;
use crate::game::scores;
use crate::game::song::{SongData, get_song_cache};
use crate::rgba_const;
use crate::screens::components::{
    select_music::{music_wheel, screen_bars, select_pane, sort_menu, step_artist_bar},
    shared::{
        banner as shared_banner, gs_scorebox, heart_bg, mode_pads, profile_boxes, test_input,
        timers,
    },
};
use crate::screens::{DensityGraphSlot, DensityGraphSource, Screen, ScreenAction};
use image::{Rgba, RgbaImage};
use log::debug;
use null_or_die::{BiasKernel, BiasStreamCfg, BiasStreamEvent, GraphOrientation, KernelTarget};
use rssp::bpm::parse_bpm_map;
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::thread::LocalKey;
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

#[path = "select_music/pack_sync.rs"]
mod pack_sync;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.5;
const TRANSITION_OUT_DURATION: f32 = 0.3;
const RELOAD_BAR_H: f32 = 30.0;
const SYNC_OVERLAY_Z: i16 = 1495;
const SYNC_HEAT_TEXTURE_KEY: &str = "__generated/sync-overlay-heat";
const SYNC_HEAT_ALPHA: f32 = 1.0;
const SYNC_READY_TEXT_ZOOM: f32 = 0.95;
const SYNC_READY_LINE_STEP: f32 = 24.0 * SYNC_READY_TEXT_ZOOM;
const SYNC_OVERLAY_MAX_PENDING_MSGS: usize = 32;
const SYNC_OVERLAY_MAX_MSGS_PER_FRAME: usize = 32;
const SYNC_OVERLAY_POLL_BUDGET: Duration = Duration::from_millis(2);

// Simply Love BGAnimations/ScreenSelectMusic overlay/PerPlayer/StepArtist.lua
// Cycles through AuthorCredit, Description, ChartName every 2 seconds.
const STEP_ARTIST_CYCLE_SECONDS: f32 = 2.0;

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
const CDTITLE_SPIN_SECONDS: f32 = 0.5;
const CDTITLE_FRAME_DELAY_SECONDS: f32 = 0.1;
const CDTITLE_ZOOM_BASE: f32 = 22.0;
const CDTITLE_RATIO_MIN: f32 = 2.5;
const CDTITLE_OFFSET_X: f32 = (BANNER_NATIVE_WIDTH - 30.0) * 0.5;
const CDTITLE_OFFSET_Y: f32 = (BANNER_NATIVE_HEIGHT - 30.0) * 0.5;
rgba_const!(UI_BOX_BG_COLOR, "#1E282F");

// --- Timing & Logic Constants ---
// ITGmania WheelBase::Move() uses `m_TimeBeforeMovingBegins = 1/4.0f` before auto-scrolling.
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(250);
// ScreenSelectMusic inherits Screen's default menu repeat timing via InputFilter:
// 0.375s before repeating, then 8 repeats/sec.
const OVERLAY_NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(375);
const OVERLAY_NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_millis(125);
const DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(300);
// ITGmania InputQueue: g_fSimultaneousThreshold = 0.05f.
const CHORD_SIMULTANEOUS_WINDOW: Duration = Duration::from_millis(50);
const PREVIEW_DELAY_SECONDS: f32 = 0.25;
const PREVIEW_FADE_OUT_SECONDS: f64 = 1.5;
const DEFAULT_PREVIEW_LENGTH: f64 = 12.0;
const SELECT_MUSIC_LEADERBOARD_NUM_ENTRIES: usize = 5;

const MUSIC_WHEEL_SWITCH_SECONDS: f32 = 0.10;
const MUSIC_WHEEL_SETTLE_MIN_SPEED: f32 = 0.2;
// ITGmania PrefsManager default: MusicWheelSwitchSpeed=15.
const MUSIC_WHEEL_HOLD_SPIN_SPEED_DEFAULT: f32 = 15.0;
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
const NUM_STANDARD_DIFFICULTIES: usize = color::FILE_DIFFICULTY_NAMES.len();
const TEXT_CACHE_LIMIT: usize = 8192;

type TextCache<K> = HashMap<K, Arc<str>>;

thread_local! {
    static SESSION_TIME_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(2048));
    static CHART_LENGTH_CACHE: RefCell<TextCache<i32>> = RefCell::new(HashMap::with_capacity(2048));
    static UINT_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(4096));
    static MUSIC_RATE_FMT_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(256));
    static MUSIC_RATE_BANNER_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(128));
    static CHART_INFO_CACHE: RefCell<TextCache<(u8, u32, u64, u64)>> = RefCell::new(HashMap::with_capacity(512));
    static STAMINA_MONO_CACHE: RefCell<TextCache<u64>> = RefCell::new(HashMap::with_capacity(512));
    static STAMINA_CANDLES_CACHE: RefCell<TextCache<u64>> = RefCell::new(HashMap::with_capacity(512));
    static STREAM_TOTAL_CACHE: RefCell<TextCache<(u32, u32)>> = RefCell::new(HashMap::with_capacity(512));
    static TECH_STREAM_CACHE: RefCell<TextCache<(u32, usize, u32)>> = RefCell::new(HashMap::with_capacity(512));
    static TOTAL_LABEL_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(512));
    static STR_REF_CACHE: RefCell<TextCache<(usize, usize)>> = RefCell::new(HashMap::with_capacity(4096));
    static SCORE_PERCENT_CACHE: RefCell<TextCache<u64>> = RefCell::new(HashMap::with_capacity(2048));
}

#[inline(always)]
fn music_wheel_hold_spin_speed() -> f32 {
    let configured = crate::config::get().music_wheel_switch_speed;
    if configured == 0 {
        MUSIC_WHEEL_HOLD_SPIN_SPEED_DEFAULT
    } else {
        configured.max(1) as f32
    }
}

#[inline(always)]
fn cached_text<K, F>(cache: &'static LocalKey<RefCell<TextCache<K>>>, key: K, build: F) -> Arc<str>
where
    K: Copy + Eq + std::hash::Hash,
    F: FnOnce() -> String,
{
    cache.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(text) = cache.get(&key) {
            return text.clone();
        }
        let text: Arc<str> = Arc::<str>::from(build());
        if cache.len() < TEXT_CACHE_LIMIT {
            cache.insert(key, text.clone());
        }
        text
    })
}

#[inline(always)]
fn cached_u32_text(value: u32) -> Arc<str> {
    cached_text(&UINT_TEXT_CACHE, value, || value.to_string())
}

#[inline(always)]
fn cached_total_label_text(total: u32) -> Arc<str> {
    cached_text(&TOTAL_LABEL_CACHE, total, || {
        let mut s = total.to_string();
        s.push_str(" Total");
        s
    })
}

#[inline(always)]
fn cached_str_ref(text: &str) -> Arc<str> {
    let key = (text.as_ptr() as usize, text.len());
    cached_text(&STR_REF_CACHE, key, || text.to_owned())
}

#[inline(always)]
fn placeholder_score_percent() -> Arc<str> {
    static PLACEHOLDER: OnceLock<Arc<str>> = OnceLock::new();
    PLACEHOLDER
        .get_or_init(|| Arc::<str>::from("??.??%"))
        .clone()
}

#[inline(always)]
fn cached_score_percent_text(score_percent: f64) -> Arc<str> {
    let score = if score_percent.is_finite() {
        score_percent.clamp(0.0, 1.0) * 100.0
    } else {
        0.0
    };
    cached_text(&SCORE_PERCENT_CACHE, score.to_bits(), || {
        format!("{score:.2}%")
    })
}

#[inline(always)]
fn cached_chart_info_text(
    show_peak_nps: bool,
    show_matrix_rating: bool,
    meter: u32,
    peak_nps: f64,
    matrix_rating: f64,
) -> Arc<str> {
    let peak_nps = if peak_nps.is_finite() {
        peak_nps.max(0.0)
    } else {
        0.0
    };
    let matrix_rating = if matrix_rating.is_finite() {
        matrix_rating.max(0.0)
    } else {
        0.0
    };
    let mut mask = (show_peak_nps as u8) | ((show_matrix_rating as u8) << 1);
    if mask == 0 {
        mask = 1;
    }
    let matrix_rating_rounded = (matrix_rating * 100.0).round() / 100.0;
    let matrix_rating_text = if meter >= 11 && matrix_rating_rounded > 0.0 {
        format!("MR: {matrix_rating_rounded:.2}")
    } else {
        "MR: N/A".to_string()
    };
    cached_text(
        &CHART_INFO_CACHE,
        (mask, meter, peak_nps.to_bits(), matrix_rating.to_bits()),
        || match mask {
            0b10 => matrix_rating_text,
            0b11 => format!("PNPS: {peak_nps:.1} | {matrix_rating_text}"),
            _ => format!("Peak NPS: {peak_nps:.1}"),
        },
    )
}

#[inline(always)]
fn cached_stamina_mono_text(percent: f64) -> Arc<str> {
    let percent = if percent.is_finite() { percent } else { 0.0 };
    cached_text(&STAMINA_MONO_CACHE, percent.to_bits(), || {
        format!("{percent:.1}% Mono")
    })
}

#[inline(always)]
fn cached_stamina_candles_text(percent: f64) -> Arc<str> {
    let percent = if percent.is_finite() { percent } else { 0.0 };
    cached_text(&STAMINA_CANDLES_CACHE, percent.to_bits(), || {
        format!("{percent:.1}% Candles")
    })
}

#[inline(always)]
fn cached_stream_total_text(total_streams: u32, stream_percent: f32) -> Arc<str> {
    let stream_percent = if stream_percent.is_finite() {
        stream_percent
    } else {
        0.0
    };
    cached_text(
        &STREAM_TOTAL_CACHE,
        (total_streams, stream_percent.to_bits()),
        || format!("{total_streams} ({stream_percent:.1}%)"),
    )
}

#[inline(always)]
fn cached_tech_stream_text(
    total_streams: u32,
    total_measures: usize,
    stream_percent: f32,
) -> Arc<str> {
    let stream_percent = if stream_percent.is_finite() {
        stream_percent
    } else {
        0.0
    };
    cached_text(
        &TECH_STREAM_CACHE,
        (total_streams, total_measures, stream_percent.to_bits()),
        || format!("{total_streams}/{total_measures} ({stream_percent:.1}%)"),
    )
}

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
    sec_at_beat_from_bpms(&song.normalized_bpms, target_beat)
}

fn beat_at_sec(song: &SongData, target_sec: f64) -> f64 {
    if !target_sec.is_finite() || target_sec <= 0.0 {
        return 0.0;
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
fn preview_marker(
    displayed: Option<&DisplayedChart>,
    preview_sec: Option<f64>,
    graph_left: f32,
    graph_w: f32,
) -> Option<PreviewMarker> {
    let displayed = displayed?;
    let preview_sec = preview_sec?;
    let chart = displayed.song.charts.get(displayed.chart_ix)?;
    if graph_w <= 0.0 || !preview_sec.is_finite() {
        return None;
    }
    let first_second = chart.first_second;
    let last_second = displayed
        .song
        .precise_last_second()
        .max(first_second + 0.001);
    let (window_w_px, _) = current_window_px();
    let px_per_unit = window_w_px as f32 / screen_width().max(1.0);
    let unit_per_px = if px_per_unit.is_finite() && px_per_unit > 0.0 {
        1.0 / px_per_unit
    } else {
        1.0
    };
    let width_px = 2.0_f32;
    let width_units = width_px * unit_per_px;
    let max_x = (graph_w - width_units).max(0.0);
    let x = (((preview_sec as f32 - first_second) / (last_second - first_second)).clamp(0.0, 1.0)
        * max_x)
        .clamp(0.0, max_x);
    let left_px = (graph_left + x) * px_per_unit;
    let right_px = left_px + width_px;
    let start_px = left_px.floor() as i32;
    let end_px = right_px.ceil() as i32;
    let mut marker = PreviewMarker::default();
    for px in start_px..end_px {
        if marker.len == marker.cols.len() {
            break;
        }
        let overlap = (right_px.min(px as f32 + 1.0) - left_px.max(px as f32)).clamp(0.0, 1.0);
        if overlap <= 0.0 {
            continue;
        }
        let col_x = (px as f32 * unit_per_px - graph_left).clamp(0.0, graph_w - unit_per_px);
        marker.cols[marker.len] = PreviewMarkerCol {
            x: col_x,
            a: overlap,
        };
        marker.len += 1;
    }
    (marker.len > 0).then_some(marker)
}

#[derive(Clone, Copy, Debug, Default)]
struct PreviewMarkerCol {
    x: f32,
    a: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct PreviewMarker {
    cols: [PreviewMarkerCol; 4],
    len: usize,
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

#[inline(always)]
fn fallback_banner_key(active_color_index: i32) -> String {
    let banner_num = active_color_index.rem_euclid(12) + 1;
    format!("banner{banner_num}.png")
}

// Optimized formatter
fn fmt_music_rate(rate: f32) -> Arc<str> {
    let rate = if rate.is_finite() { rate } else { 1.0 };
    cached_text(&MUSIC_RATE_FMT_CACHE, rate.to_bits(), || {
        let scaled = (rate * 100.0).round() as i32;
        if scaled == 100 {
            return "1.0".to_string();
        }
        let int_part = scaled / 100;
        let frac2 = (scaled % 100).abs();
        if frac2 == 0 {
            int_part.to_string()
        } else if frac2 % 10 == 0 {
            format!("{int_part}.{}", frac2 / 10)
        } else {
            format!("{int_part}.{frac2:02}")
        }
    })
}

#[inline(always)]
fn cached_music_rate_banner_text(rate: f32) -> Arc<str> {
    let rate = if rate.is_finite() { rate } else { 1.0 };
    cached_text(&MUSIC_RATE_BANNER_CACHE, rate.to_bits(), || {
        let rate_text = fmt_music_rate(rate);
        let mut text = String::with_capacity(rate_text.len() + 12);
        text.push_str(rate_text.as_ref());
        text.push_str("x Music Rate");
        text
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    Song {
        done: usize,
        total: usize,
        pack: String,
        song: String,
    },
    Course {
        done: usize,
        total: usize,
        group: String,
        course: String,
    },
    Done,
}

struct ReloadUiState {
    phase: ReloadPhase,
    line2: String,
    line3: String,
    songs_done: usize,
    songs_total: usize,
    courses_done: usize,
    courses_total: usize,
    done: bool,
    started_at: Instant,
    rx: mpsc::Receiver<ReloadMsg>,
}

impl ReloadUiState {
    fn new(rx: mpsc::Receiver<ReloadMsg>) -> Self {
        Self {
            phase: ReloadPhase::Songs,
            line2: String::new(),
            line3: String::new(),
            songs_done: 0,
            songs_total: 0,
            courses_done: 0,
            courses_total: 0,
            done: false,
            started_at: Instant::now(),
            rx,
        }
    }
}

enum SyncWorkerMsg {
    Event(BiasStreamEvent),
    Finished(Result<null_or_die::api::SyncChartResult, String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyncOverlayPhase {
    Running,
    Ready,
    Failed,
}

struct SyncOverlayStateData {
    simfile_path: PathBuf,
    song_title: String,
    chart_label: String,
    song_offset_seconds: f32,
    kernel_target: KernelTarget,
    kernel_type: BiasKernel,
    graph_mode: SyncGraphMode,
    cols: usize,
    freq_rows: usize,
    total_beats: usize,
    digest_rows: usize,
    times_ms: Vec<f64>,
    freq_domain: Vec<f64>,
    beat_digest: Vec<f64>,
    digest_col_sums: Vec<f64>,
    post_rows: usize,
    post_kernel: Vec<f64>,
    convolution: Vec<f64>,
    curve_mesh: Option<Arc<[MeshVertex]>>,
    edge_discard: usize,
    beats_processed: usize,
    preview_bias_ms: Option<f64>,
    final_bias_ms: Option<f64>,
    final_confidence: Option<f64>,
    phase: SyncOverlayPhase,
    yes_selected: bool,
    error_text: Option<String>,
    rx: mpsc::Receiver<SyncWorkerMsg>,
}

enum SyncOverlayState {
    Hidden,
    Visible(SyncOverlayStateData),
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
    pub current_cdtitle_key: Option<String>,
    pub current_graph_key: String,
    pub current_graph_key_p2: String,
    pub current_graph_mesh: Option<Arc<[MeshVertex]>>,
    pub current_graph_mesh_p2: Option<Arc<[MeshVertex]>>,
    pub session_elapsed: f32,
    pub gameplay_elapsed: f32,
    displayed_chart_p1: Option<DisplayedChart>,
    displayed_chart_p2: Option<DisplayedChart>,
    step_artist_cycle_base: f32,

    // Internal state
    out_prompt: OutPromptState,
    exit_prompt: ExitPromptState,
    reload_ui: Option<ReloadUiState>,
    song_search: sort_menu::SongSearchState,
    song_search_ignore_next_back_select: bool,
    replay_overlay: sort_menu::ReplayOverlayState,
    sync_overlay: SyncOverlayState,
    pack_sync_overlay: crate::screens::pack_sync::OverlayState,
    pub test_input_overlay_visible: bool,
    test_input_overlay: test_input::State,
    profile_switch_overlay: Option<profile_boxes::State>,
    pending_replay: Option<sort_menu::ReplayStartPayload>,
    sort_menu: sort_menu::State,
    leaderboard: sort_menu::LeaderboardOverlayState,
    downloads_overlay: sort_menu::DownloadsOverlayState,
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
    last_requested_cdtitle_path: Option<PathBuf>,
    pub(crate) banner_high_quality_requested: bool,
    cdtitle_spin_elapsed: f32,
    cdtitle_anim_elapsed: f32,
    last_requested_chart_hash: Option<String>,
    last_requested_chart_hash_p2: Option<String>,
    last_refreshed_leaderboard_hash: Option<String>,
    last_refreshed_leaderboard_hash_p2: Option<String>,
    chord_mask_p1: u8,
    chord_mask_p2: u8,
    menu_chord_mask: u8,
    p1_chord_up_pressed_at: Option<Instant>,
    p1_chord_down_pressed_at: Option<Instant>,
    p2_chord_up_pressed_at: Option<Instant>,
    p2_chord_down_pressed_at: Option<Instant>,
    p1_select_held: bool,
    p2_select_held: bool,
    menu_chord_left_pressed_at: Option<Instant>,
    menu_chord_right_pressed_at: Option<Instant>,
    last_steps_nav_dir_p1: Option<PadDir>,
    last_steps_nav_time_p1: Option<Instant>,
    last_steps_nav_dir_p2: Option<PadDir>,
    last_steps_nav_time_p2: Option<Instant>,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    overlay_nav_held_direction: Option<NavDirection>,
    overlay_nav_held_since: Option<Instant>,
    overlay_nav_last_scrolled_at: Option<Instant>,
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
    cached_standard_chart_ixs: [Option<usize>; NUM_STANDARD_DIFFICULTIES],
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
    new_pack_names: HashSet<String>,
}

#[inline(always)]
fn cached_score_exists(score: scores::CachedScore) -> bool {
    score.grade != scores::Grade::Failed || score.score_percent > 0.0
}

fn song_has_cached_score(song: &SongData) -> bool {
    for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
        if !profile::is_session_side_joined(side) {
            continue;
        }
        for chart in &song.charts {
            if scores::get_cached_score_for_side(&chart.short_hash, side)
                .is_some_and(cached_score_exists)
            {
                return true;
            }
        }
    }
    false
}

fn joined_local_profile_ids() -> Vec<String> {
    let mut profile_ids = Vec::with_capacity(2);
    for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
        if !profile::is_session_side_joined(side) {
            continue;
        }
        let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
            continue;
        };
        if !profile_ids.iter().any(|id| id == &profile_id) {
            profile_ids.push(profile_id);
        }
    }
    profile_ids
}

fn sync_new_pack_names(
    profile_ids: &[String],
    scanned_pack_names: Vec<String>,
    scored_pack_names: &HashSet<String>,
    mode: NewPackMode,
) -> HashSet<String> {
    match mode {
        NewPackMode::Disabled => {
            profile::mark_packs_known(profile_ids, scanned_pack_names.iter().map(String::as_str));
            HashSet::new()
        }
        NewPackMode::OpenPack => profile::sync_known_packs(profile_ids, &scanned_pack_names),
        NewPackMode::HasScore => scanned_pack_names
            .into_iter()
            .filter(|name| !scored_pack_names.contains(name.as_str()))
            .collect(),
    }
}

fn maybe_clear_selected_pack_on_score(state: &mut State, mode: NewPackMode) {
    if mode != NewPackMode::HasScore
        || state.sort_mode != WheelSortMode::Group
        || state.new_pack_names.is_empty()
    {
        return;
    }
    let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) else {
        return;
    };
    let song = song.clone();
    if !song_has_cached_score(&song) {
        return;
    }
    let Some(pack_name) = group_name_for_song(&state.entries, &song) else {
        return;
    };
    state.new_pack_names.remove(&pack_name);
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
    })
}

pub(crate) fn edit_charts_sorted<'a>(song: &'a SongData, chart_type: &str) -> Vec<&'a ChartData> {
    let mut edits: Vec<&ChartData> = song
        .charts
        .iter()
        .filter(|c| {
            c.chart_type.eq_ignore_ascii_case(chart_type)
                && c.difficulty.eq_ignore_ascii_case("edit")
        })
        .collect();
    edits.sort_by_cached_key(|c| (c.description.to_lowercase(), c.meter, c.short_hash.as_str()));
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
            {
                Some(i)
            } else {
                None
            }
        })
        .collect();
    indices.sort_by_cached_key(|&idx| {
        let c = &song.charts[idx];
        (c.description.to_lowercase(), c.meter, c.short_hash.as_str())
    });
    indices
}

#[inline]
fn standard_chart_indices(
    song: &SongData,
    chart_type: &str,
) -> [Option<usize>; NUM_STANDARD_DIFFICULTIES] {
    let mut out = [None; NUM_STANDARD_DIFFICULTIES];
    for (chart_ix, chart) in song.charts.iter().enumerate() {
        if !chart.chart_type.eq_ignore_ascii_case(chart_type) {
            continue;
        }
        for (diff_ix, &diff_name) in color::FILE_DIFFICULTY_NAMES.iter().enumerate() {
            if out[diff_ix].is_none() && chart.difficulty.eq_ignore_ascii_case(diff_name) {
                out[diff_ix] = Some(chart_ix);
                break;
            }
        }
    }
    out
}

#[inline]
fn chart_ix_for_steps_index(
    standard_charts: &[Option<usize>; NUM_STANDARD_DIFFICULTIES],
    steps_index: usize,
    edits_sorted: &[usize],
) -> Option<usize> {
    if steps_index < color::FILE_DIFFICULTY_NAMES.len() {
        return standard_charts[steps_index];
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
        state.cached_standard_chart_ixs = standard_chart_indices(song, chart_type);
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
        state.cached_chart_ix_p1 = chart_ix_for_steps_index(
            &state.cached_standard_chart_ixs,
            state.selected_steps_index,
            edits,
        );
    }
    if !is_versus {
        state.cached_chart_ix_p2 = None;
    } else if song_changed || type_changed || p2_changed || state.cached_chart_ix_p2.is_none() {
        // Recover from stale/missing P2 cache without requiring wheel movement.
        state.cached_chart_ix_p2 = chart_ix_for_steps_index(
            &state.cached_standard_chart_ixs,
            state.p2_selected_steps_index,
            edits,
        );
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
    let chart = song
        .charts
        .iter()
        .find(|c| c.chart_type.eq_ignore_ascii_case(chart_type) && c.short_hash == chart_hash)?;

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

pub(crate) fn best_steps_index(
    song: &SongData,
    chart_type: &str,
    preferred_difficulty_index: usize,
) -> Option<usize> {
    let standard_len = color::FILE_DIFFICULTY_NAMES.len();
    if standard_len == 0 {
        return None;
    }

    let preferred = preferred_difficulty_index.min(standard_len - 1);
    let mut best_standard = None;
    let mut best_distance = usize::MAX;
    for idx in 0..standard_len {
        if chart_for_steps_index(song, chart_type, idx).is_none() {
            continue;
        }
        let distance = idx.abs_diff(preferred);
        if distance < best_distance {
            best_distance = distance;
            best_standard = Some(idx);
        }
    }
    if best_standard.is_some() {
        return best_standard;
    }

    if edit_charts_sorted(song, chart_type).is_empty() {
        None
    } else {
        Some(standard_len)
    }
}

fn rebuild_displayed_entries(state: &mut State) {
    state.entries = build_displayed_entries(
        &state.all_entries,
        state.expanded_pack_name.as_deref(),
        config::get().select_music_wheel_style,
    );
    if state.entries.is_empty() {
        state.wheel_offset_from_selection = 0.0;
    }
}

fn build_displayed_entries(
    all_entries: &[MusicWheelEntry],
    expanded_pack_name: Option<&str>,
    wheel_style: crate::config::SelectMusicWheelStyle,
) -> Vec<MusicWheelEntry> {
    let has_pack_headers = all_entries
        .iter()
        .any(|entry| matches!(entry, MusicWheelEntry::PackHeader { .. }));
    if !has_pack_headers {
        return all_entries.to_vec();
    }

    // Simply Love parity:
    // `OnlyShowActiveSection=true` hides every other section when a pack is open,
    // but `HideActiveSectionTitle=false` keeps the active header visible.
    let hide_non_active_packs = expanded_pack_name.is_some()
        && matches!(wheel_style, crate::config::SelectMusicWheelStyle::Iidx);

    let mut new_entries = Vec::with_capacity(all_entries.len());
    let mut current_pack_name: Option<&str> = None;
    for entry in all_entries {
        match entry {
            MusicWheelEntry::PackHeader { name, .. } => {
                current_pack_name = Some(name.as_str());
                if !hide_non_active_packs || expanded_pack_name == Some(name.as_str()) {
                    new_entries.push(entry.clone());
                }
            }
            MusicWheelEntry::Song(_) => {
                if expanded_pack_name == current_pack_name {
                    new_entries.push(entry.clone());
                }
            }
        }
    }
    new_entries
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

const RANDOM_BPM_CYCLE_SPEED: f32 = 0.2;

fn random_bpm_cycle_text(elapsed: f32) -> String {
    let cycle = (elapsed / RANDOM_BPM_CYCLE_SPEED) as u32;
    // Deterministic per-cycle "random" via integer hash (Knuth multiplicative)
    let hash = cycle.wrapping_mul(2654435761);
    if hash % 10 == 0 {
        "???".to_string()
    } else {
        (hash % 1000).to_string()
    }
}

/// Formats a BPM range with music rate applied, matching Simply Love's
/// `StringifyDisplayBPMs` semantics: integers at 1.0x, one decimal otherwise.
fn format_bpm_with_rate(range: Option<(f64, f64)>, music_rate: f32) -> String {
    let Some((lo, hi)) = range else {
        return String::new();
    };
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate as f64
    } else {
        1.0
    };
    let lo = lo * rate;
    let hi = hi * rate;
    let use_decimals = (music_rate - 1.0).abs() > 0.001;
    let fmt_one = |v: f64| {
        if use_decimals {
            let s = format!("{v:.1}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            format!("{v:.0}")
        }
    };
    if (lo - hi).abs() < 1.0e-6 {
        fmt_one(lo)
    } else {
        format!("{} - {}", fmt_one(lo.min(hi)), fmt_one(lo.max(hi)))
    }
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
        (
            song_bpm_for_sort(song.as_ref()),
            song_title_sort_key(song.as_ref()),
        )
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
        if !chart.chart_type.eq_ignore_ascii_case(chart_type) || !chart.has_note_data {
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
            if !chart.has_note_data {
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
            if !chart.has_note_data {
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
    state.last_requested_cdtitle_path = None;
    state.cdtitle_spin_elapsed = 0.0;
    state.cdtitle_anim_elapsed = 0.0;
    state.last_requested_chart_hash = None;
    state.last_requested_chart_hash_p2 = None;
    state.cached_song = None;
    state.cached_chart_ix_p1 = None;
    state.cached_chart_ix_p2 = None;
    state.cached_edits = None;
    state.cached_standard_chart_ixs = [None; NUM_STANDARD_DIFFICULTIES];
}

pub fn init() -> State {
    let started = Instant::now();
    debug!("Preparing SelectMusic state...");
    let lock_started = Instant::now();
    let song_cache = get_song_cache();
    let lock_wait = lock_started.elapsed();

    let target_chart_type = profile::get_session_play_style().chart_type();
    let total_packs = song_cache.len();
    let total_songs: usize = song_cache.iter().map(|p| p.songs.len()).sum();
    let new_pack_mode = config::get().select_music_new_pack_mode;
    let clear_new_packs_on_score = new_pack_mode == NewPackMode::HasScore;
    let joined_profile_ids = joined_local_profile_ids();

    let mut all_entries = Vec::with_capacity(total_packs.saturating_add(total_songs));
    let mut pack_song_counts = HashMap::with_capacity(total_packs);
    let mut pack_total_seconds_by_index = vec![0.0_f64; total_packs];
    let mut song_has_edit_ptrs = HashSet::with_capacity(total_songs);
    let mut scored_pack_names = HashSet::new();

    let profile_data = profile::get();
    let last_played = profile_data.last_played(profile::get_session_play_style());
    let max_diff_index = color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1);
    let initial_diff_index = if max_diff_index == 0 {
        0
    } else {
        last_played.difficulty_index.min(max_diff_index)
    };

    let mut last_song_arc: Option<Arc<SongData>> = None;
    let mut last_pack_name: Option<String> = None;
    let last_path = last_played.song_music_path.as_deref();

    let mut matched_packs = 0usize;
    let mut matched_songs = 0usize;

    // Filter and build entries in one pass
    for (i, pack) in song_cache.iter().enumerate() {
        let mut pack_name: Option<String> = None;
        let mut pack_song_count = 0usize;
        let mut pack_total_seconds = 0.0_f64;
        let mut pack_has_cached_score = false;

        for song in &pack.songs {
            let mut has_target_chart_type = false;
            let mut has_edit = false;
            for chart in &song.charts {
                if !chart.chart_type.eq_ignore_ascii_case(target_chart_type) {
                    continue;
                }
                has_target_chart_type = true;
                if chart.difficulty.eq_ignore_ascii_case("edit") {
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
            if clear_new_packs_on_score && !pack_has_cached_score && song_has_cached_score(song) {
                pack_has_cached_score = true;
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
            pack_total_seconds +=
                if song.music_length_seconds.is_finite() && song.music_length_seconds > 0.0 {
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
            if pack_has_cached_score {
                scored_pack_names.insert(name.clone());
            }
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
    let new_pack_names = sync_new_pack_names(
        &joined_profile_ids,
        pack_song_counts.keys().cloned().collect(),
        &scored_pack_names,
        new_pack_mode,
    );

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
        sync_overlay: SyncOverlayState::Hidden,
        pack_sync_overlay: crate::screens::pack_sync::OverlayState::Hidden,
        test_input_overlay_visible: false,
        test_input_overlay: test_input::State::default(),
        profile_switch_overlay: None,
        pending_replay: None,
        sort_menu: sort_menu::State::Hidden,
        leaderboard: sort_menu::LeaderboardOverlayState::Hidden,
        downloads_overlay: sort_menu::DownloadsOverlayState::Hidden,
        sort_mode: WheelSortMode::Group,
        expanded_pack_name: last_pack_name,
        bg: heart_bg::State::new(),
        last_requested_banner_path: None,
        last_requested_cdtitle_path: None,
        banner_high_quality_requested: false,
        cdtitle_spin_elapsed: 0.0,
        cdtitle_anim_elapsed: 0.0,
        current_banner_key: "banner1.png".to_string(),
        current_cdtitle_key: None,
        last_requested_chart_hash: None,
        current_graph_key: "__white".to_string(),
        current_graph_key_p2: "__white".to_string(),
        current_graph_mesh: None,
        current_graph_mesh_p2: None,
        displayed_chart_p1: None,
        displayed_chart_p2: None,
        last_requested_chart_hash_p2: None,
        last_refreshed_leaderboard_hash: None,
        last_refreshed_leaderboard_hash_p2: None,
        chord_mask_p1: 0,
        chord_mask_p2: 0,
        menu_chord_mask: 0,
        p1_chord_up_pressed_at: None,
        p1_chord_down_pressed_at: None,
        p2_chord_up_pressed_at: None,
        p2_chord_down_pressed_at: None,
        p1_select_held: false,
        p2_select_held: false,
        menu_chord_left_pressed_at: None,
        menu_chord_right_pressed_at: None,
        last_steps_nav_dir_p1: None,
        last_steps_nav_time_p1: None,
        last_steps_nav_dir_p2: None,
        last_steps_nav_time_p2: None,
        nav_key_held_direction: None,
        nav_key_held_since: None,
        overlay_nav_held_direction: None,
        overlay_nav_held_since: None,
        overlay_nav_last_scrolled_at: None,
        sort_menu_prev_selected_index: 0,
        sort_menu_focus_anim_elapsed: sort_menu::FOCUS_TWEEN_SECONDS,
        currently_playing_preview_path: None,
        currently_playing_preview_start_sec: None,
        currently_playing_preview_length_sec: None,
        session_elapsed: 0.0,
        gameplay_elapsed: 0.0,
        prev_selected_index: 0,
        time_since_selection_change: 0.0,
        step_artist_cycle_base: 0.0,
        cached_song: None,
        cached_chart_type: "",
        cached_steps_index_p1: usize::MAX,
        cached_steps_index_p2: usize::MAX,
        cached_chart_ix_p1: None,
        cached_chart_ix_p2: None,
        cached_edits: None,
        cached_standard_chart_ixs: [None; NUM_STANDARD_DIFFICULTIES],
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
        new_pack_names,
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
                if let Some(hash) = last_played.chart_hash.as_deref()
                    && let Some(idx2) = steps_index_for_chart_hash(song, target_chart_type, hash)
                {
                    state.selected_steps_index = idx2;
                    if idx2 < color::FILE_DIFFICULTY_NAMES.len() {
                        state.preferred_difficulty_index = idx2;
                    }
                    state.p2_selected_steps_index = state.selected_steps_index;
                    state.p2_preferred_difficulty_index = state.preferred_difficulty_index;
                    state.prev_selected_index = state.selected_index;
                    debug!(
                        "SelectMusic state ready: chart_type={target_chart_type} matched {matched_songs} songs in {matched_packs}/{total_packs} packs ({} total songs), entries {built_entries_len}→{displayed_entries_len}, lock {:?}, rebuild {:?}, total {:?}.",
                        total_songs,
                        lock_wait,
                        rebuild_dur,
                        started.elapsed()
                    );
                    return state;
                }

                if let Some(idx2) =
                    best_steps_index(song, target_chart_type, state.preferred_difficulty_index)
                {
                    state.selected_steps_index = idx2;
                }
                if let Some(idx2) =
                    best_steps_index(song, target_chart_type, state.p2_preferred_difficulty_index)
                {
                    state.p2_selected_steps_index = idx2;
                } else {
                    state.p2_selected_steps_index = state.selected_steps_index;
                }
                if state.selected_steps_index < color::FILE_DIFFICULTY_NAMES.len() {
                    state.preferred_difficulty_index = state.selected_steps_index;
                }
                if state.p2_selected_steps_index < color::FILE_DIFFICULTY_NAMES.len() {
                    state.p2_preferred_difficulty_index = state.p2_selected_steps_index;
                }
            }
        }
    }

    state.prev_selected_index = state.selected_index;
    debug!(
        "SelectMusic state ready: chart_type={target_chart_type} matched {matched_songs} songs in {matched_packs}/{total_packs} packs ({} total songs), entries {built_entries_len}→{displayed_entries_len}, lock {:?}, rebuild {:?}, total {:?}.",
        total_songs,
        lock_wait,
        rebuild_dur,
        started.elapsed()
    );
    state
}

pub fn init_placeholder() -> State {
    let profile_data = profile::get();
    let last_played = profile_data.last_played(profile::get_session_play_style());
    let max_diff_index = color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1);
    let initial_diff_index = if max_diff_index == 0 {
        0
    } else {
        last_played.difficulty_index.min(max_diff_index)
    };

    State {
        all_entries: Vec::new(),
        group_entries: Vec::new(),
        title_entries: Vec::new(),
        artist_entries: Vec::new(),
        bpm_entries: Vec::new(),
        length_entries: Vec::new(),
        meter_entries: Vec::new(),
        popularity_entries: Vec::new(),
        recent_entries: Vec::new(),
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
        sync_overlay: SyncOverlayState::Hidden,
        pack_sync_overlay: crate::screens::pack_sync::OverlayState::Hidden,
        test_input_overlay_visible: false,
        test_input_overlay: test_input::State::default(),
        profile_switch_overlay: None,
        pending_replay: None,
        sort_menu: sort_menu::State::Hidden,
        leaderboard: sort_menu::LeaderboardOverlayState::Hidden,
        downloads_overlay: sort_menu::DownloadsOverlayState::Hidden,
        sort_mode: WheelSortMode::Group,
        expanded_pack_name: None,
        bg: heart_bg::State::new(),
        last_requested_banner_path: None,
        last_requested_cdtitle_path: None,
        banner_high_quality_requested: false,
        cdtitle_spin_elapsed: 0.0,
        cdtitle_anim_elapsed: 0.0,
        current_banner_key: "banner1.png".to_string(),
        current_cdtitle_key: None,
        last_requested_chart_hash: None,
        current_graph_key: "__white".to_string(),
        current_graph_key_p2: "__white".to_string(),
        current_graph_mesh: None,
        current_graph_mesh_p2: None,
        displayed_chart_p1: None,
        displayed_chart_p2: None,
        last_requested_chart_hash_p2: None,
        last_refreshed_leaderboard_hash: None,
        last_refreshed_leaderboard_hash_p2: None,
        chord_mask_p1: 0,
        chord_mask_p2: 0,
        menu_chord_mask: 0,
        p1_chord_up_pressed_at: None,
        p1_chord_down_pressed_at: None,
        p2_chord_up_pressed_at: None,
        p2_chord_down_pressed_at: None,
        p1_select_held: false,
        p2_select_held: false,
        menu_chord_left_pressed_at: None,
        menu_chord_right_pressed_at: None,
        last_steps_nav_dir_p1: None,
        last_steps_nav_time_p1: None,
        last_steps_nav_dir_p2: None,
        last_steps_nav_time_p2: None,
        nav_key_held_direction: None,
        nav_key_held_since: None,
        overlay_nav_held_direction: None,
        overlay_nav_held_since: None,
        overlay_nav_last_scrolled_at: None,
        sort_menu_prev_selected_index: 0,
        sort_menu_focus_anim_elapsed: sort_menu::FOCUS_TWEEN_SECONDS,
        currently_playing_preview_path: None,
        currently_playing_preview_start_sec: None,
        currently_playing_preview_length_sec: None,
        session_elapsed: 0.0,
        gameplay_elapsed: 0.0,
        prev_selected_index: 0,
        time_since_selection_change: 0.0,
        step_artist_cycle_base: 0.0,
        cached_song: None,
        cached_chart_type: "",
        cached_steps_index_p1: usize::MAX,
        cached_steps_index_p2: usize::MAX,
        cached_chart_ix_p1: None,
        cached_chart_ix_p2: None,
        cached_edits: None,
        cached_standard_chart_ixs: [None; NUM_STANDARD_DIFFICULTIES],
        pack_total_seconds_by_index: Vec::new(),
        song_has_edit_ptrs: HashSet::new(),
        pack_song_counts: HashMap::new(),
        group_pack_song_counts: HashMap::new(),
        title_pack_song_counts: HashMap::new(),
        artist_pack_song_counts: HashMap::new(),
        bpm_pack_song_counts: HashMap::new(),
        length_pack_song_counts: HashMap::new(),
        meter_pack_song_counts: HashMap::new(),
        popularity_pack_song_counts: HashMap::new(),
        recent_pack_song_counts: HashMap::new(),
        new_pack_names: HashSet::new(),
    }
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

    let hold_spin_speed = music_wheel_hold_spin_speed();
    state.wheel_offset_from_selection -= hold_spin_speed * moving * dt;
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
fn sync_preview_song(state: &mut State, selected_song: Option<&Arc<SongData>>, loop_preview: bool) {
    let music_path = selected_song.and_then(|s| s.music_path.clone());
    if state.currently_playing_preview_path == music_path {
        return;
    }

    state.currently_playing_preview_path = music_path;
    if let Some(song) = selected_song {
        if let Some((path, cut)) = compute_preview_cut(song) {
            state.currently_playing_preview_start_sec = Some(cut.start_sec as f32);
            state.currently_playing_preview_length_sec = Some(cut.length_sec as f32);
            audio::play_music(
                path,
                cut,
                loop_preview,
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
fn clear_overlay_nav_hold(state: &mut State) {
    state.overlay_nav_held_direction = None;
    state.overlay_nav_held_since = None;
    state.overlay_nav_last_scrolled_at = None;
}

#[inline(always)]
fn start_overlay_nav_hold(state: &mut State, dir: NavDirection) {
    let now = Instant::now();
    state.overlay_nav_held_direction = Some(dir);
    state.overlay_nav_held_since = Some(now);
    state.overlay_nav_last_scrolled_at = Some(now);
}

#[inline(always)]
fn release_overlay_nav_hold(state: &mut State, dir: NavDirection) {
    if state.overlay_nav_held_direction == Some(dir) {
        clear_overlay_nav_hold(state);
    }
}

#[inline(always)]
const fn overlay_nav_delta(dir: NavDirection) -> isize {
    match dir {
        NavDirection::Left => -1,
        NavDirection::Right => 1,
    }
}

#[inline(always)]
const fn overlay_nav_dir(action: VirtualAction) -> Option<NavDirection> {
    match action {
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => Some(NavDirection::Left),
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => Some(NavDirection::Right),
        _ => None,
    }
}

#[inline(always)]
fn show_sort_menu(state: &mut State) {
    state.sort_menu = sort_menu::State::Visible {
        page: sort_menu::Page::Main,
        selected_index: 0,
    };
    state.sort_menu_prev_selected_index = 0;
    clear_menu_chord(state);
    clear_overlay_nav_hold(state);
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
    clear_overlay_nav_hold(state);
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
fn try_open_sort_menu_with_select_start(
    state: &mut State,
    select_held: bool,
    pressed: bool,
) -> bool {
    if !pressed || !select_held {
        return false;
    }
    // Simply Love parity: holding Select and pressing Start opens SortMenu.
    show_sort_menu(state);
    true
}

#[inline(always)]
fn update_select_hold_state(state: &mut State, ev: &InputEvent) {
    match ev.action {
        VirtualAction::p1_select => state.p1_select_held = ev.pressed,
        VirtualAction::p2_select => state.p2_select_held = ev.pressed,
        _ => {}
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
fn sort_menu_items(state: &State, page: sort_menu::Page) -> Vec<sort_menu::Item> {
    if page == sort_menu::Page::Sorts {
        return sort_menu::ITEMS_SORTS.to_vec();
    }
    let replays_enabled = config::get().machine_enable_replays;
    let downloads_enabled = crate::game::online::downloads::sort_menu_available();
    let has_song_selected = matches!(
        state.entries.get(state.selected_index),
        Some(MusicWheelEntry::Song(_))
    );
    let has_pack_selected = matches!(
        state.entries.get(state.selected_index),
        Some(MusicWheelEntry::PackHeader { .. })
    );
    let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
    let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
    let single_player_joined = p1_joined ^ p2_joined;
    let mut items = Vec::with_capacity(11);
    items.push(sort_menu::ITEM_CATEGORY_SORTS);
    match (profile::get_session_play_style(), single_player_joined) {
        (profile::PlayStyle::Single, true) => items.push(sort_menu::ITEM_SWITCH_TO_DOUBLE),
        (profile::PlayStyle::Double, true) => items.push(sort_menu::ITEM_SWITCH_TO_SINGLE),
        _ => {}
    }
    items.push(sort_menu::ITEM_TEST_INPUT);
    items.push(sort_menu::ITEM_SONG_SEARCH);
    items.push(sort_menu::ITEM_SWITCH_PROFILE);
    items.push(sort_menu::ITEM_RELOAD_SONGS_COURSES);
    if downloads_enabled {
        items.push(sort_menu::ITEM_VIEW_DOWNLOADS);
    }
    if has_pack_selected {
        items.push(sort_menu::ITEM_SYNC_PACK);
    }
    if has_song_selected {
        items.push(sort_menu::ITEM_SYNC_SONG);
        if replays_enabled {
            items.push(sort_menu::ITEM_PLAY_REPLAY);
        }
        items.push(sort_menu::ITEM_SHOW_LEADERBOARD);
    }
    items
}

#[inline(always)]
fn show_test_input_overlay(state: &mut State) {
    clear_preview(state);
    state.song_search = sort_menu::SongSearchState::Hidden;
    state.leaderboard = sort_menu::LeaderboardOverlayState::Hidden;
    state.downloads_overlay = sort_menu::DownloadsOverlayState::Hidden;
    state.replay_overlay = sort_menu::ReplayOverlayState::Hidden;
    state.sync_overlay = SyncOverlayState::Hidden;
    pack_sync::hide_overlay(state);
    state.profile_switch_overlay = None;
    clear_menu_chord(state);
    clear_overlay_nav_hold(state);
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
    state.downloads_overlay = sort_menu::DownloadsOverlayState::Hidden;
    state.replay_overlay = sort_menu::ReplayOverlayState::Hidden;
    state.sync_overlay = SyncOverlayState::Hidden;
    pack_sync::hide_overlay(state);
    state.profile_switch_overlay = None;
    hide_test_input_overlay(state);
    clear_menu_chord(state);
    clear_overlay_nav_hold(state);
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
    state.downloads_overlay = sort_menu::DownloadsOverlayState::Hidden;
    state.replay_overlay = sort_menu::ReplayOverlayState::Hidden;
    state.sync_overlay = SyncOverlayState::Hidden;
    pack_sync::hide_overlay(state);
    hide_test_input_overlay(state);
    clear_menu_chord(state);
    clear_p1_ud_chord(state);
    clear_p2_ud_chord(state);
    clear_overlay_nav_hold(state);
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
    clear_overlay_nav_hold(state);
}

#[inline(always)]
fn close_song_search(state: &mut State) {
    state.song_search = sort_menu::SongSearchState::Hidden;
    clear_overlay_nav_hold(state);
}

#[inline(always)]
fn cancel_song_search(state: &mut State) {
    state.song_search = sort_menu::SongSearchState::Hidden;
    clear_overlay_nav_hold(state);
    state.song_search_ignore_next_back_select = true;
}

fn start_song_search_results(state: &mut State, search_text: String) {
    clear_overlay_nav_hold(state);
    state.song_search = sort_menu::begin_song_search_results(&state.group_entries, search_text);
}

fn focus_song_from_search(state: &mut State, song: &Arc<SongData>) {
    if let Some(index) = song_entry_index(&state.entries, song) {
        state.selected_index = index;
        state.time_since_selection_change = 0.0;
        state.wheel_offset_from_selection = 0.0;
        state.last_requested_banner_path = None;
        state.last_requested_cdtitle_path = None;
        state.cdtitle_spin_elapsed = 0.0;
        state.cdtitle_anim_elapsed = 0.0;
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
            state.last_requested_cdtitle_path = None;
            state.cdtitle_spin_elapsed = 0.0;
            state.cdtitle_anim_elapsed = 0.0;
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
    state.last_requested_cdtitle_path = None;
    state.cdtitle_spin_elapsed = 0.0;
    state.cdtitle_anim_elapsed = 0.0;
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
    state.sync_overlay = SyncOverlayState::Hidden;
    pack_sync::hide_overlay(state);
    state.profile_switch_overlay = None;
    hide_test_input_overlay(state);
    clear_menu_chord(state);
    clear_p1_ud_chord(state);
    clear_p2_ud_chord(state);
    clear_overlay_nav_hold(state);
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

        let mut on_song = |done: usize, total: usize, pack: &str, song: &str| {
            let _ = tx.send(ReloadMsg::Song {
                done,
                total,
                pack: pack.to_owned(),
                song: song.to_owned(),
            });
        };
        song_loading::scan_and_load_songs_with_progress_counts("songs", &mut on_song);

        let _ = tx.send(ReloadMsg::Phase(ReloadPhase::Courses));

        let mut on_course = |done: usize, total: usize, group: &str, course: &str| {
            let _ = tx.send(ReloadMsg::Course {
                done,
                total,
                group: group.to_owned(),
                course: course.to_owned(),
            });
        };
        course::scan_and_load_courses_with_progress_counts("courses", "songs", &mut on_course);

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
            ReloadMsg::Song {
                done,
                total,
                pack,
                song,
            } => {
                reload.phase = ReloadPhase::Songs;
                reload.songs_done = done;
                reload.songs_total = total;
                reload.line2 = pack;
                reload.line3 = song;
            }
            ReloadMsg::Course {
                done,
                total,
                group,
                course,
            } => {
                reload.phase = ReloadPhase::Courses;
                reload.courses_done = done;
                reload.courses_total = total;
                reload.line2 = group;
                reload.line3 = course;
            }
            ReloadMsg::Done => {
                reload.done = true;
            }
        }
    }
}

#[inline(always)]
fn reload_progress(reload: &ReloadUiState) -> (usize, usize, f32) {
    let done = reload.songs_done.saturating_add(reload.courses_done);
    let mut total = reload.songs_total.saturating_add(reload.courses_total);
    if total < done {
        total = done;
    }
    let mut progress = if total > 0 {
        (done as f32 / total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if !reload.done && total > 0 && progress >= 1.0 {
        progress = 0.999;
    }
    (done, total, progress)
}

#[inline(always)]
const fn reload_phase_label(phase: ReloadPhase) -> &'static str {
    match phase {
        ReloadPhase::Songs => "Loading songs...",
        ReloadPhase::Courses => "Loading courses...",
    }
}

fn reload_detail_lines(reload: &ReloadUiState) -> (String, String) {
    (reload.line2.clone(), reload.line3.clone())
}

fn push_reload_overlay(actors: &mut Vec<Actor>, reload: &ReloadUiState, active_color_index: i32) {
    let (done, total, progress) = reload_progress(reload);
    let elapsed = reload.started_at.elapsed().as_secs_f32().max(0.0);
    let count_text = if total == 0 {
        String::new()
    } else {
        crate::screens::progress_count_text(done, total)
    };
    let show_speed_row = total > 0;
    let speed_text = if elapsed > 0.0 && show_speed_row {
        format!("Current speed: {:.1} items/s", done as f32 / elapsed)
    } else if show_speed_row {
        "Current speed: 0.0 items/s".to_string()
    } else {
        String::new()
    };
    let (line2, line3) = reload_detail_lines(reload);
    let fill = color::decorative_rgba(active_color_index);

    let bar_w = widescale(360.0, 520.0);
    let bar_h = RELOAD_BAR_H;
    let bar_cx = screen_center_x();
    let bar_cy = screen_center_y() + 34.0;
    let fill_w = (bar_w - 4.0) * progress.clamp(0.0, 1.0);

    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.8):
        z(1450)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(if total == 0 { "Initializing..." } else { reload_phase_label(reload.phase) }):
        align(0.5, 0.5):
        xy(screen_center_x(), bar_cy - 98.0):
        zoom(1.05):
        horizalign(center):
        z(1451)
    ));
    if !line2.is_empty() {
        actors.push(act!(text:
            font("miso"):
            settext(line2):
            align(0.5, 0.5):
            xy(screen_center_x(), bar_cy - 74.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(1451)
        ));
    }
    if !line3.is_empty() {
        actors.push(act!(text:
            font("miso"):
            settext(line3):
            align(0.5, 0.5):
            xy(screen_center_x(), bar_cy - 50.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(1451)
        ));
    }

    let mut bar_children = Vec::with_capacity(4);
    bar_children.push(act!(quad:
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoomto(bar_w, bar_h):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(0)
    ));
    bar_children.push(act!(quad:
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoomto(bar_w - 4.0, bar_h - 4.0):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1)
    ));
    if fill_w > 0.0 {
        bar_children.push(act!(quad:
            align(0.0, 0.5):
            xy(2.0, bar_h / 2.0):
            zoomto(fill_w, bar_h - 4.0):
            diffuse(fill[0], fill[1], fill[2], 1.0):
            z(2)
        ));
    }
    bar_children.push(act!(text:
        font("miso"):
        settext(count_text):
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoom(0.9):
        horizalign(center):
        z(3)
    ));
    actors.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [bar_cx, bar_cy],
        size: [SizeSpec::Px(bar_w), SizeSpec::Px(bar_h)],
        background: None,
        z: 1451,
        children: bar_children,
    });

    if show_speed_row {
        actors.push(act!(text:
            font("miso"):
            settext(speed_text):
            align(0.5, 0.5):
            xy(screen_center_x(), bar_cy + 36.0):
            zoom(0.9):
            horizalign(center):
            z(1451)
        ));
    }
}

#[inline(always)]
fn sync_bias_to_graph_x(bias_ms: f64, times_ms: &[f64], graph_w: f32) -> f32 {
    if times_ms.len() < 2 || graph_w <= 0.0 {
        return graph_w * 0.5;
    }
    let start = times_ms[0];
    let end = *times_ms.last().unwrap_or(&start);
    let span = end - start;
    if !span.is_finite() || span.abs() < f64::EPSILON {
        return graph_w * 0.5;
    }
    let t = ((bias_ms - start) / span).clamp(0.0, 1.0) as f32;
    t * (graph_w - 1.0).max(0.0)
}

fn push_line_segment(
    out: &mut Vec<MeshVertex>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    color: [f32; 4],
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx.mul_add(dx, dy * dy)).sqrt();
    if len <= 0.000_1 {
        return;
    }
    let half = thickness * 0.5;
    let nx = -dy / len * half;
    let ny = dx / len * half;

    let a = [x0 + nx, y0 + ny];
    let b = [x0 - nx, y0 - ny];
    let c = [x1 + nx, y1 + ny];
    let d = [x1 - nx, y1 - ny];

    out.push(MeshVertex { pos: a, color });
    out.push(MeshVertex { pos: b, color });
    out.push(MeshVertex { pos: c, color });
    out.push(MeshVertex { pos: c, color });
    out.push(MeshVertex { pos: b, color });
    out.push(MeshVertex { pos: d, color });
}

fn build_sync_curve_mesh(
    values: &[f64],
    edge_discard: usize,
    graph_w: f32,
    graph_h: f32,
    color: [f32; 4],
) -> Option<Arc<[MeshVertex]>> {
    if values.len() < 2 || graph_w <= 0.0 || graph_h <= 0.0 {
        return None;
    }
    let edge = edge_discard.min(values.len() / 2);
    let core = &values[edge..values.len().saturating_sub(edge)];
    if core.is_empty() {
        return None;
    }
    let mut min_value = f64::INFINITY;
    let mut max_value = f64::NEG_INFINITY;
    for &value in core {
        min_value = min_value.min(value);
        max_value = max_value.max(value);
    }
    let y_top = graph_h * 0.1;
    let y_bottom = graph_h * 0.9;
    let mut out: Vec<MeshVertex> = Vec::with_capacity(values.len().saturating_sub(1) * 6);
    for i in 0..values.len().saturating_sub(1) {
        let denom = values.len().saturating_sub(1) as f32;
        let x0 = (i as f32 / denom) * (graph_w - 1.0).max(0.0);
        let x1 = ((i + 1) as f32 / denom) * (graph_w - 1.0).max(0.0);
        let t0 = sync_heat_norm01(values[i], min_value, max_value) as f32;
        let t1 = sync_heat_norm01(values[i + 1], min_value, max_value) as f32;
        let y0 = y_bottom + (y_top - y_bottom) * t0;
        let y1 = y_bottom + (y_top - y_bottom) * t1;
        push_line_segment(&mut out, x0, y0, x1, y1, 1.5, color);
    }
    if out.is_empty() {
        None
    } else {
        Some(Arc::from(out.into_boxed_slice()))
    }
}

#[inline(always)]
fn sync_heat_norm01(v: f64, lo: f64, hi: f64) -> f64 {
    let span = hi - lo;
    if !span.is_finite() || span.abs() < f64::EPSILON {
        0.5
    } else {
        ((v - lo) / span).clamp(0.0, 1.0)
    }
}

#[inline(always)]
fn sync_lerp(a: f64, b: f64, t: f64) -> f64 {
    a * (1.0 - t) + b * t
}

fn sync_percentile(values: &[f64], pct: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = (pct / 100.0) * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        sync_lerp(sorted[lo], sorted[hi], rank - lo as f64)
    }
}

#[inline(always)]
fn sync_viridis(t: f64) -> [f32; 4] {
    const STOPS: [[u8; 3]; 5] = [
        [68, 1, 84],
        [59, 82, 139],
        [33, 145, 140],
        [94, 201, 98],
        [253, 231, 37],
    ];
    let x = t.clamp(0.0, 1.0) * 4.0;
    let i = x.floor() as usize;
    let (a, b, frac) = if i >= 4 {
        (STOPS[4], STOPS[4], 0.0)
    } else {
        (STOPS[i], STOPS[i + 1], x - i as f64)
    };
    let mix = |aa: u8, bb: u8| ((aa as f64) * (1.0 - frac) + (bb as f64) * frac) as f32 / 255.0;
    [
        mix(a[0], b[0]),
        mix(a[1], b[1]),
        mix(a[2], b[2]),
        SYNC_HEAT_ALPHA,
    ]
}

fn sync_heat_value_range(values: &[f64], clim_pct: Option<(f64, f64)>) -> Option<(f64, f64)> {
    if values.is_empty() {
        return None;
    }
    if let Some((lo_pct, hi_pct)) = clim_pct {
        let lo = sync_percentile(values, lo_pct);
        let hi = sync_percentile(values, hi_pct);
        if hi > lo {
            return Some((lo, hi));
        }
    }
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !lo.is_finite() || !hi.is_finite() {
        None
    } else if hi > lo {
        Some((lo, hi))
    } else {
        Some((lo - 1.0, hi + 1.0))
    }
}

fn build_sync_heat_image(
    matrix: &[f64],
    total_rows: usize,
    data_rows: usize,
    cols: usize,
    graph_w: f32,
    graph_h: f32,
    clim_pct: Option<(f64, f64)>,
) -> Option<RgbaImage> {
    if data_rows == 0 || cols == 0 || graph_w <= 0.0 || graph_h <= 0.0 {
        return None;
    }
    let image_h = (graph_h.round() as u32).max(1);
    let image_w = (graph_w.round() as u32).max(1);
    let used = data_rows.saturating_mul(cols).min(matrix.len());
    let (lo, hi) = sync_heat_value_range(&matrix[..used], clim_pct)?;
    let mut image = RgbaImage::new(image_w, image_h);
    for py in 0..image_h as usize {
        let row = (((image_h as usize - 1 - py) * total_rows) / image_h as usize)
            .min(total_rows.saturating_sub(1));
        for px in 0..image_w as usize {
            let rgba = if row < data_rows {
                let col = (px * cols / image_w as usize).min(cols.saturating_sub(1));
                let value = matrix[row * cols + col];
                let color = sync_viridis(sync_heat_norm01(value, lo, hi));
                Rgba([
                    (color[0] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (color[1] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (color[2] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (color[3] * 255.0).round().clamp(0.0, 255.0) as u8,
                ])
            } else {
                Rgba([0, 0, 0, 0])
            };
            image.put_pixel(px as u32, py as u32, rgba);
        }
    }
    Some(image)
}

fn sync_heat_source(overlay: &SyncOverlayStateData) -> Option<(&[f64], usize, usize)> {
    match overlay.graph_mode {
        SyncGraphMode::Frequency
            if overlay.freq_rows > 0
                && overlay.freq_domain.len() == overlay.freq_rows.saturating_mul(overlay.cols) =>
        {
            Some((
                overlay.freq_domain.as_slice(),
                overlay.freq_rows,
                overlay.freq_rows,
            ))
        }
        SyncGraphMode::BeatIndex
            if overlay.digest_rows > 0
                && overlay.beat_digest.len()
                    == overlay.digest_rows.saturating_mul(overlay.cols) =>
        {
            Some((
                overlay.beat_digest.as_slice(),
                overlay.total_beats.max(overlay.digest_rows),
                overlay.digest_rows,
            ))
        }
        SyncGraphMode::PostKernelFingerprint
            if overlay.post_rows > 0
                && overlay.post_kernel.len() == overlay.post_rows.saturating_mul(overlay.cols) =>
        {
            Some((
                overlay.post_kernel.as_slice(),
                overlay.post_rows,
                overlay.post_rows,
            ))
        }
        SyncGraphMode::PostKernelFingerprint
            if overlay.phase == SyncOverlayPhase::Running
                && overlay.digest_rows > 0
                && overlay.beat_digest.len()
                    == overlay.digest_rows.saturating_mul(overlay.cols) =>
        {
            Some((
                overlay.beat_digest.as_slice(),
                overlay.total_beats.max(overlay.digest_rows),
                overlay.digest_rows,
            ))
        }
        _ => None,
    }
}

#[inline(always)]
fn sync_heat_clim_pct(overlay: &SyncOverlayStateData) -> Option<(f64, f64)> {
    match overlay.graph_mode {
        SyncGraphMode::Frequency => None,
        SyncGraphMode::BeatIndex if overlay.phase == SyncOverlayPhase::Ready => Some((10.0, 90.0)),
        SyncGraphMode::PostKernelFingerprint => Some((3.0, 97.0)),
        _ => None,
    }
}

#[inline(always)]
fn sync_overlay_graph_size() -> (f32, f32) {
    (widescale(520.0, 640.0) - 80.0, 132.0)
}

fn refresh_sync_overlay_heat_texture(overlay: &mut SyncOverlayStateData) {
    let (graph_w, graph_h) = sync_overlay_graph_size();
    let Some((matrix, total_rows, data_rows)) = sync_heat_source(overlay) else {
        return;
    };
    let clim_pct = sync_heat_clim_pct(overlay);
    let Some(image) = build_sync_heat_image(
        matrix,
        total_rows,
        data_rows,
        overlay.cols,
        graph_w,
        graph_h,
        clim_pct,
    ) else {
        return;
    };
    assets::register_generated_texture(
        SYNC_HEAT_TEXTURE_KEY,
        image,
        SamplerDesc {
            filter: SamplerFilter::Nearest,
            ..SamplerDesc::default()
        },
    );
}

fn refresh_sync_overlay_curve_mesh(overlay: &mut SyncOverlayStateData) {
    let (graph_w, graph_h) = sync_overlay_graph_size();
    overlay.curve_mesh = build_sync_curve_mesh(
        &overlay.convolution,
        overlay.edge_discard,
        graph_w,
        graph_h,
        [1.0, 1.0, 1.0, 1.0],
    );
}

#[derive(Default)]
struct SyncOverlayRefresh {
    heat: bool,
    curve: bool,
}

impl SyncOverlayRefresh {
    #[inline(always)]
    fn heat(&mut self) {
        self.heat = true;
    }

    #[inline(always)]
    fn meshes(&mut self) {
        self.heat = true;
        self.curve = true;
    }

    fn flush(self, overlay: &mut SyncOverlayStateData) {
        if self.heat {
            refresh_sync_overlay_heat_texture(overlay);
        }
        if self.curve {
            refresh_sync_overlay_curve_mesh(overlay);
        }
    }
}

#[inline(always)]
fn sync_overlay_poll_exhausted(started: Instant, handled: usize) -> bool {
    handled >= SYNC_OVERLAY_MAX_MSGS_PER_FRAME || started.elapsed() >= SYNC_OVERLAY_POLL_BUDGET
}

fn sync_overlay_apply_beat(
    overlay: &mut SyncOverlayStateData,
    beat_seq: usize,
    row: Vec<f64>,
    freq_delta: Option<Vec<f64>>,
    refresh: &mut SyncOverlayRefresh,
) {
    if let Some(freq_delta) = freq_delta
        && overlay.phase == SyncOverlayPhase::Running
        && overlay.cols > 0
        && overlay.freq_rows > 0
        && freq_delta.len() == overlay.freq_rows.saturating_mul(overlay.cols)
    {
        if overlay.freq_domain.len() != freq_delta.len() {
            overlay.freq_domain.resize(freq_delta.len(), 0.0);
        }
        for (sum, value) in overlay.freq_domain.iter_mut().zip(freq_delta) {
            *sum += value;
        }
        refresh.heat();
    }

    if overlay.phase != SyncOverlayPhase::Running
        || overlay.kernel_target != KernelTarget::Digest
        || overlay.cols == 0
        || row.len() != overlay.cols
    {
        return;
    }

    overlay.beats_processed = overlay.beats_processed.max(beat_seq + 1);
    overlay.digest_rows = overlay.beats_processed;
    overlay.beat_digest.extend_from_slice(row.as_slice());
    for (sum, value) in overlay.digest_col_sums.iter_mut().zip(row.iter().copied()) {
        *sum += value;
    }
    overlay.convolution =
        sync_convolution_from_digest_sums(&overlay.digest_col_sums, overlay.kernel_type);
    overlay.preview_bias_ms = sync_peak_bias_ms(
        &overlay.convolution,
        &overlay.times_ms,
        overlay.edge_discard,
    );
    refresh.meshes();
}

fn sync_overlay_apply_event(
    overlay: &mut SyncOverlayStateData,
    event: BiasStreamEvent,
    refresh: &mut SyncOverlayRefresh,
) {
    match event {
        BiasStreamEvent::Init(init) => {
            overlay.cols = init.cols;
            overlay.freq_rows = init.freq_rows;
            overlay.total_beats = init.planned_beats;
            overlay.digest_rows = 0;
            overlay.times_ms = init.times_ms;
            overlay.freq_domain.clear();
            overlay.beat_digest.clear();
            overlay.kernel_target = init.kernel_target;
            overlay.digest_col_sums = vec![0.0; init.cols];
            overlay.post_rows = 0;
            overlay.post_kernel.clear();
            overlay.convolution.clear();
            overlay.curve_mesh = None;
            overlay.beats_processed = 0;
            overlay.preview_bias_ms = None;
        }
        BiasStreamEvent::Beat(beat) => sync_overlay_apply_beat(
            overlay,
            beat.beat_seq,
            beat.digest_row,
            beat.freq_delta,
            refresh,
        ),
        BiasStreamEvent::Convolution(conv) => {
            overlay.post_rows = conv.rows;
            overlay.post_kernel = conv.post_kernel;
            overlay.convolution = conv.convolution;
            overlay.edge_discard = conv.edge_discard;
            overlay.preview_bias_ms = sync_peak_bias_ms(
                &overlay.convolution,
                &overlay.times_ms,
                overlay.edge_discard,
            );
            refresh.meshes();
        }
        BiasStreamEvent::Done(estimate) => {
            overlay.final_bias_ms = Some(estimate.bias_ms);
            overlay.final_confidence = Some(estimate.confidence);
        }
    }
}

fn sync_overlay_apply_result(
    overlay: &mut SyncOverlayStateData,
    result: Result<null_or_die::api::SyncChartResult, String>,
    refresh: &mut SyncOverlayRefresh,
) {
    match result {
        Ok(result) => {
            if overlay.times_ms.is_empty() {
                overlay.times_ms = result.plot.times_ms.clone();
                overlay.cols = result.plot.cols;
            }
            overlay.freq_rows = result.plot.freq_rows;
            overlay.freq_domain = result.plot.freq_domain.clone();
            overlay.total_beats = overlay.total_beats.max(result.plot.digest_rows);
            overlay.beats_processed = overlay.beats_processed.max(result.plot.digest_rows);
            if overlay.beat_digest.len() != result.plot.beat_digest.len() {
                overlay.beat_digest = result.plot.beat_digest.clone();
            }
            overlay.digest_rows = result.plot.digest_rows;
            overlay.post_rows = result.plot.post_rows;
            overlay.post_kernel = result.plot.post_kernel.clone();
            if overlay.convolution.is_empty() {
                overlay.convolution = result.plot.convolution.clone();
                overlay.edge_discard = result.plot.edge_discard;
            }
            overlay.final_bias_ms = Some(result.estimate.bias_ms);
            overlay.final_confidence = Some(result.estimate.confidence);
            if overlay.preview_bias_ms.is_none() {
                overlay.preview_bias_ms = sync_peak_bias_ms(
                    &overlay.convolution,
                    &overlay.times_ms,
                    overlay.edge_discard,
                );
            }
            overlay.phase = SyncOverlayPhase::Ready;
            overlay.yes_selected = true;
            refresh.meshes();
        }
        Err(err) => {
            overlay.phase = SyncOverlayPhase::Failed;
            overlay.error_text = Some(err);
        }
    }
}

fn sync_graph_label(overlay: &SyncOverlayStateData) -> &'static str {
    if overlay.graph_mode == SyncGraphMode::PostKernelFingerprint
        && (overlay.post_rows == 0
            || overlay.post_kernel.len() != overlay.post_rows.saturating_mul(overlay.cols))
    {
        "Post-kernel fingerprint (building)"
    } else {
        overlay.graph_mode.label()
    }
}

fn build_sync_overlay(state: &SyncOverlayState, active_color_index: i32) -> Option<Vec<Actor>> {
    let SyncOverlayState::Visible(overlay) = state else {
        return None;
    };

    let mut actors = Vec::with_capacity(26);
    let accent = color::simply_love_rgba(active_color_index);
    let pane_w = widescale(520.0, 640.0);
    let pane_h = 430.0;
    let pane_cx = screen_center_x();
    let pane_cy = screen_center_y() - 10.0;
    let pane_left = pane_cx - pane_w * 0.5;
    let pane_top = pane_cy - pane_h * 0.5;
    let (graph_w, graph_h) = sync_overlay_graph_size();
    let graph_x = pane_left + 40.0;
    let graph_y = pane_top + 116.0;
    let graph_center_y = graph_y + graph_h * 0.5;

    let title = match overlay.phase {
        SyncOverlayPhase::Running => "Syncing song...",
        SyncOverlayPhase::Ready => "Sync complete",
        SyncOverlayPhase::Failed => "Sync failed",
    };
    let subtitle = format!("{}  [{}]", overlay.song_title, overlay.chart_label);
    let ready_prompt_y = pane_top + pane_h - 116.0;
    let ready_offset_line = if overlay.phase == SyncOverlayPhase::Ready {
        let delta_seconds = sync_apply_delta_seconds(overlay).unwrap_or(0.0);
        let new_offset = overlay.song_offset_seconds + delta_seconds;
        sync_prompt_offset_line(overlay.song_offset_seconds, new_offset)
    } else {
        None
    };

    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.85):
        z(SYNC_OVERLAY_Z)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, pane_cy):
        zoomto(pane_w + 2.0, pane_h + 2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(SYNC_OVERLAY_Z + 1)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, pane_cy):
        zoomto(pane_w, pane_h):
        diffuse(0.02, 0.02, 0.02, 1.0):
        z(SYNC_OVERLAY_Z + 2)
    ));
    actors.push(act!(text:
        font("wendy"):
        settext(title):
        align(0.5, 0.5):
        xy(pane_cx, pane_top + 34.0):
        zoom(0.62):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(SYNC_OVERLAY_Z + 3):
        horizalign(center)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(subtitle):
        align(0.5, 0.5):
        xy(pane_cx, pane_top + 62.0):
        zoom(0.9):
        maxwidth(pane_w - 30.0):
        diffuse(0.82, 0.82, 0.82, 1.0):
        z(SYNC_OVERLAY_Z + 3):
        horizalign(center)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, graph_center_y):
        zoomto(graph_w + 2.0, graph_h + 2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(SYNC_OVERLAY_Z + 3)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, graph_center_y):
        zoomto(graph_w, graph_h):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(SYNC_OVERLAY_Z + 4)
    ));
    if sync_heat_source(overlay).is_some() {
        actors.push(Actor::Sprite {
            align: [0.0, 0.0],
            offset: [graph_x, graph_y],
            world_z: 0.0,
            size: [SizeSpec::Px(graph_w), SizeSpec::Px(graph_h)],
            source: SpriteSource::Texture(Arc::<str>::from(SYNC_HEAT_TEXTURE_KEY)),
            tint: [1.0, 1.0, 1.0, 1.0],
            glow: [0.0, 0.0, 0.0, 0.0],
            z: SYNC_OVERLAY_Z + 4,
            cell: None,
            grid: None,
            uv_rect: None,
            visible: true,
            flip_x: false,
            flip_y: false,
            cropleft: 0.0,
            cropright: 0.0,
            croptop: 0.0,
            cropbottom: 0.0,
            fadeleft: 0.0,
            faderight: 0.0,
            fadetop: 0.0,
            fadebottom: 0.0,
            blend: BlendMode::Alpha,
            mask_source: false,
            mask_dest: false,
            rot_x_deg: 0.0,
            rot_y_deg: 0.0,
            rot_z_deg: 0.0,
            local_offset: [0.0, 0.0],
            local_offset_rot_sin_cos: [0.0, 1.0],
            texcoordvelocity: None,
            animate: false,
            state_delay: 0.0,
            scale: [1.0, 1.0],
            effect: Default::default(),
        });
    }
    actors.push(act!(text:
        font("miso"):
        settext(sync_graph_label(overlay)):
        align(0.5, 0.5):
        xy(pane_cx, graph_y - 14.0):
        zoom(0.8):
        diffuse(0.75, 0.75, 0.75, 1.0):
        z(SYNC_OVERLAY_Z + 5):
        horizalign(center)
    ));
    actors.push(act!(quad:
        align(0.0, 0.5):
        xy(graph_x, graph_center_y):
        zoomto(graph_w, 1.0):
        diffuse(0.25, 0.25, 0.25, 1.0):
        z(SYNC_OVERLAY_Z + 5)
    ));

    if let Some(mesh) = overlay.curve_mesh.clone() {
        actors.push(Actor::Mesh {
            align: [0.0, 0.0],
            offset: [graph_x, graph_y],
            size: [SizeSpec::Px(graph_w), SizeSpec::Px(graph_h)],
            vertices: mesh,
            mode: MeshMode::Triangles,
            visible: true,
            blend: BlendMode::Alpha,
            z: SYNC_OVERLAY_Z + 6,
        });
    } else {
        actors.push(act!(text:
            font("miso"):
            settext("Waiting for streamed analysis data..."):
            align(0.5, 0.5):
            xy(pane_cx, graph_center_y):
            zoom(0.9):
            diffuse(0.6, 0.6, 0.6, 1.0):
            z(SYNC_OVERLAY_Z + 6):
            horizalign(center)
        ));
    }

    if let Some(bias_ms) = overlay.final_bias_ms.or(overlay.preview_bias_ms) {
        let marker_x = graph_x + sync_bias_to_graph_x(bias_ms, &overlay.times_ms, graph_w);
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(marker_x, graph_center_y):
            zoomto(2.0, graph_h):
            diffuse(0.9, 0.1, 0.1, 1.0):
            z(SYNC_OVERLAY_Z + 7)
        ));
    }

    let status_text = match overlay.phase {
        SyncOverlayPhase::Running => match overlay.total_beats.max(overlay.beats_processed) {
            0 => "Beat 0".to_string(),
            total => format!(
                "Beat {} out of {}",
                overlay.beats_processed.min(total),
                total
            ),
        },
        SyncOverlayPhase::Ready => {
            let bias = overlay
                .final_bias_ms
                .or(overlay.preview_bias_ms)
                .unwrap_or(0.0);
            let confidence = overlay.final_confidence.unwrap_or(0.0) * 100.0;
            format!("Suggested sync: {bias:+.2} ms   (confidence {confidence:.0}%)")
        }
        SyncOverlayPhase::Failed => overlay
            .error_text
            .as_deref()
            .unwrap_or("Unknown sync analysis error")
            .to_string(),
    };
    let status_y =
        if matches!(overlay.phase, SyncOverlayPhase::Ready) && ready_offset_line.is_some() {
            ready_prompt_y - SYNC_READY_LINE_STEP * 1.5
        } else {
            graph_y + graph_h + 18.0
        };
    actors.push(act!(text:
        font("miso"):
        settext(status_text):
        align(0.5, 0.5):
        xy(pane_cx, status_y):
        zoom(SYNC_READY_TEXT_ZOOM):
        maxwidth(pane_w - 26.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(SYNC_OVERLAY_Z + 4):
        horizalign(center)
    ));

    match overlay.phase {
        SyncOverlayPhase::Ready => {
            let answer_y = pane_top + pane_h - 48.0;
            let choice_yes_x = pane_cx - 100.0;
            let choice_no_x = pane_cx + 100.0;
            let cursor_x = if overlay.yes_selected {
                choice_yes_x
            } else {
                choice_no_x
            };
            let prompt = build_sync_save_prompt_text(overlay);
            let prompt_y = if ready_offset_line.is_some() {
                ready_prompt_y + SYNC_READY_LINE_STEP
            } else {
                ready_prompt_y
            };

            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(cursor_x, answer_y):
                zoomto(145.0, 40.0):
                diffuse(accent[0], accent[1], accent[2], 1.0):
                z(SYNC_OVERLAY_Z + 4)
            ));
            if let Some(line) = ready_offset_line.as_deref() {
                actors.push(act!(text:
                    font("miso"):
                    settext(line):
                    align(0.5, 0.5):
                    xy(pane_cx, ready_prompt_y - SYNC_READY_LINE_STEP * 0.5):
                    zoom(SYNC_READY_TEXT_ZOOM):
                    maxwidth(pane_w - 90.0):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(SYNC_OVERLAY_Z + 4):
                    horizalign(center)
                ));
            }
            actors.push(act!(text:
                font("miso"):
                settext(prompt):
                align(0.5, 0.5):
                xy(pane_cx, prompt_y):
                zoom(SYNC_READY_TEXT_ZOOM):
                maxwidth(pane_w - 90.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(SYNC_OVERLAY_Z + 4):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("wendy"):
                settext("YES"):
                align(0.5, 0.5):
                xy(choice_yes_x, answer_y):
                zoom(0.72):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(SYNC_OVERLAY_Z + 4):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("wendy"):
                settext("NO"):
                align(0.5, 0.5):
                xy(choice_no_x, answer_y):
                zoom(0.72):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(SYNC_OVERLAY_Z + 4):
                horizalign(center)
            ));
        }
        SyncOverlayPhase::Running => {
            actors.push(act!(text:
                font("miso"):
                settext("START/BACK/SELECT: CANCEL"):
                align(0.5, 0.5):
                xy(pane_cx, pane_top + pane_h - 16.0):
                zoom(0.82):
                diffuse(0.85, 0.85, 0.85, 1.0):
                z(SYNC_OVERLAY_Z + 4):
                horizalign(center)
            ));
        }
        SyncOverlayPhase::Failed => {
            actors.push(act!(text:
                font("miso"):
                settext("START/BACK/SELECT: CLOSE"):
                align(0.5, 0.5):
                xy(pane_cx, pane_top + pane_h - 16.0):
                zoom(0.82):
                diffuse(0.85, 0.85, 0.85, 1.0):
                z(SYNC_OVERLAY_Z + 4):
                horizalign(center)
            ));
        }
    }

    Some(actors)
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
        } else if !restored_p1
            && let Some(index) = best_steps_index(
                song,
                target_chart_type,
                refreshed.preferred_difficulty_index,
            )
        {
            refreshed.selected_steps_index = index;
            if index < color::FILE_DIFFICULTY_NAMES.len() {
                refreshed.preferred_difficulty_index = index;
            }
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
        } else if !restored_p2
            && let Some(index) = best_steps_index(
                song,
                target_chart_type,
                refreshed.p2_preferred_difficulty_index,
            )
        {
            refreshed.p2_selected_steps_index = index;
            if index < color::FILE_DIFFICULTY_NAMES.len() {
                refreshed.p2_preferred_difficulty_index = index;
            }
        }
    }

    trigger_immediate_refresh(&mut refreshed);
    *state = refreshed;
}

fn sort_menu_move(state: &mut State, delta: isize) -> bool {
    let (page, selected_index) = match state.sort_menu {
        sort_menu::State::Visible {
            page,
            selected_index,
        } => (page, selected_index),
        sort_menu::State::Hidden => return false,
    };
    let len = sort_menu_items(state, page).len();
    if len == 0 {
        return false;
    }
    let old = selected_index.min(len - 1);
    let next = ((old as isize + delta).rem_euclid(len as isize)) as usize;
    if next == old {
        return false;
    }
    state.sort_menu_prev_selected_index = old;
    if let sort_menu::State::Visible { selected_index, .. } = &mut state.sort_menu {
        *selected_index = next;
    }
    state.sort_menu_focus_anim_elapsed = 0.0;
    audio::play_sfx("assets/sounds/change.ogg");
    true
}

fn update_overlay_nav_hold(state: &mut State) {
    let Some(dir) = state.overlay_nav_held_direction else {
        return;
    };
    let Some(held_since) = state.overlay_nav_held_since else {
        clear_overlay_nav_hold(state);
        return;
    };
    let Some(last_at) = state.overlay_nav_last_scrolled_at else {
        clear_overlay_nav_hold(state);
        return;
    };

    let overlay_active = state.sort_menu != sort_menu::State::Hidden
        || matches!(state.song_search, sort_menu::SongSearchState::Results(_));
    if !overlay_active {
        clear_overlay_nav_hold(state);
        return;
    }

    let now = Instant::now();
    if now.duration_since(held_since) < OVERLAY_NAV_INITIAL_HOLD_DELAY
        || now.duration_since(last_at) < OVERLAY_NAV_REPEAT_SCROLL_INTERVAL
    {
        return;
    }

    let moved = if let sort_menu::SongSearchState::Results(results) = &mut state.song_search {
        if results.input_lock > 0.0 {
            false
        } else {
            sort_menu::song_search_move(results, overlay_nav_delta(dir))
        }
    } else {
        sort_menu_move(state, overlay_nav_delta(dir))
    };
    if moved {
        state.overlay_nav_last_scrolled_at = Some(now);
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

fn show_leaderboard_overlay(state: &mut State) {
    let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) else {
        return;
    };

    let chart_hash_p1 = selected_chart_hash_for_side(state, song, profile::PlayerSide::P1);
    let chart_hash_p2 = selected_chart_hash_for_side(state, song, profile::PlayerSide::P2);
    if let Some(overlay) = sort_menu::show_leaderboard_overlay(chart_hash_p1, chart_hash_p2) {
        state.replay_overlay = sort_menu::ReplayOverlayState::Hidden;
        state.downloads_overlay = sort_menu::DownloadsOverlayState::Hidden;
        state.sync_overlay = SyncOverlayState::Hidden;
        pack_sync::hide_overlay(state);
        state.profile_switch_overlay = None;
        hide_test_input_overlay(state);
        state.leaderboard = overlay;
        clear_preview(state);
    }
}

fn show_downloads_overlay(state: &mut State) {
    state.leaderboard = sort_menu::LeaderboardOverlayState::Hidden;
    state.replay_overlay = sort_menu::ReplayOverlayState::Hidden;
    state.sync_overlay = SyncOverlayState::Hidden;
    pack_sync::hide_overlay(state);
    state.profile_switch_overlay = None;
    hide_test_input_overlay(state);
    state.downloads_overlay = sort_menu::show_downloads_overlay();
    clear_preview(state);
}

fn show_replay_overlay(state: &mut State) {
    if !config::get().machine_enable_replays {
        return;
    }
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
    state.downloads_overlay = sort_menu::DownloadsOverlayState::Hidden;
    state.sync_overlay = SyncOverlayState::Hidden;
    pack_sync::hide_overlay(state);
    state.profile_switch_overlay = None;
    hide_test_input_overlay(state);
    state.replay_overlay = overlay;
    clear_preview(state);
}

#[inline(always)]
fn hide_sync_overlay(state: &mut State) {
    state.sync_overlay = SyncOverlayState::Hidden;
}

#[inline(always)]
fn selected_steps_index_for_sync(state: &State) -> usize {
    match (
        profile::get_session_play_style(),
        profile::get_session_player_side(),
    ) {
        (profile::PlayStyle::Versus, profile::PlayerSide::P2) => state.p2_selected_steps_index,
        _ => state.selected_steps_index,
    }
}

fn sync_chart_label(chart: &ChartData) -> String {
    if chart.difficulty.eq_ignore_ascii_case("edit") && !chart.description.trim().is_empty() {
        format!("{} ({})", chart.difficulty, chart.description)
    } else {
        chart.difficulty.clone()
    }
}

pub(crate) fn selected_chart_ix_for_sync(
    song: &SongData,
    chart_type: &str,
    steps_index: usize,
) -> Option<usize> {
    let standard = standard_chart_indices(song, chart_type);
    let edits = edit_chart_indices_sorted(song, chart_type);
    chart_ix_for_steps_index(&standard, steps_index, edits.as_slice())
}

#[inline(always)]
fn sync_kernel_row(kind: BiasKernel) -> [f64; 5] {
    if kind == BiasKernel::Loudest {
        [1.0, 3.0, 10.0, 3.0, 1.0]
    } else {
        [1.0, 1.0, 0.0, -1.0, -1.0]
    }
}

fn sync_convolution_from_digest_sums(col_sums: &[f64], kind: BiasKernel) -> Vec<f64> {
    let cols = col_sums.len();
    if cols == 0 {
        return Vec::new();
    }
    let kernel = sync_kernel_row(kind);
    let mut out = vec![0.0; cols];
    for (c, out_val) in out.iter_mut().enumerate() {
        let mut sum = 0.0;
        for (k, &weight) in kernel.iter().enumerate() {
            let cc = (c as isize - k as isize + 2).rem_euclid(cols as isize) as usize;
            sum += col_sums[cc] * weight;
        }
        *out_val = sum * 5.0;
    }
    out
}

fn sync_peak_bias_ms(convolution: &[f64], times_ms: &[f64], edge_discard: usize) -> Option<f64> {
    if convolution.is_empty() {
        return None;
    }
    let edge = edge_discard.min(convolution.len().saturating_sub(1) / 2);
    if convolution.len() <= edge.saturating_mul(2) {
        return None;
    }
    let mut peak_ix = edge;
    let mut peak_val = f64::NEG_INFINITY;
    for (i, &value) in convolution
        .iter()
        .enumerate()
        .skip(edge)
        .take(convolution.len().saturating_sub(edge * 2))
    {
        if value > peak_val {
            peak_val = value;
            peak_ix = i;
        }
    }
    if times_ms.len() == convolution.len() {
        times_ms.get(peak_ix).copied()
    } else {
        let half = (convolution.len() / 2) as isize;
        Some((peak_ix as isize - half) as f64)
    }
}

#[inline(always)]
fn sync_apply_delta_seconds(overlay: &SyncOverlayStateData) -> Option<f32> {
    overlay
        .final_bias_ms
        .map(|bias_ms| -(bias_ms as f32) * 0.001)
        .filter(|v| v.is_finite())
}

#[inline(always)]
fn sync_quantized_offset(v: f32) -> f32 {
    (v / 0.001).round() * 0.001
}

#[inline(always)]
fn sync_prompt_offset_line(old_offset: f32, new_offset: f32) -> Option<String> {
    let old_q = sync_quantized_offset(old_offset);
    let new_q = sync_quantized_offset(new_offset);
    let delta = new_q - old_q;
    if delta.abs() < 0.000_1 {
        return None;
    }
    let direction = if delta > 0.0 { "earlier" } else { "later" };
    Some(format!(
        "Song offset from {old_q:+.3} to {new_q:+.3} (notes {direction})"
    ))
}

#[inline(always)]
fn sync_confidence_threshold_percent() -> u8 {
    config::get().null_or_die_confidence_percent.min(100)
}

#[inline(always)]
fn sync_confidence_threshold() -> f64 {
    f64::from(sync_confidence_threshold_percent()) / 100.0
}

#[inline(always)]
fn sync_confidence_percent(confidence: Option<f64>) -> u32 {
    (confidence.unwrap_or(0.0).clamp(0.0, 1.0) * 100.0).round() as u32
}

fn sync_low_confidence_warning(confidence: Option<f64>, threshold: f64) -> Option<String> {
    let confidence = confidence?;
    if confidence >= threshold {
        return None;
    }
    let confidence_pct = sync_confidence_percent(Some(confidence));
    let threshold_pct = (threshold.clamp(0.0, 1.0) * 100.0).round() as u32;
    Some(format!(
        "Warning: confidence {confidence_pct}% is below the {threshold_pct}% pack-sync threshold.\nSingle-song sync can still save it."
    ))
}

fn build_sync_save_prompt_text(overlay: &SyncOverlayStateData) -> String {
    let mut prompt = String::new();
    if let Some(warning) =
        sync_low_confidence_warning(overlay.final_confidence, sync_confidence_threshold())
    {
        prompt.push_str(&warning);
        prompt.push_str("\n\n");
    }
    prompt.push_str("Would you like to save these changes?\n");
    prompt.push_str("Choosing NO will discard your changes.");
    prompt
}

fn show_sync_overlay(state: &mut State) {
    let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) else {
        return;
    };
    let song = song.clone();
    let target_chart_type = profile::get_session_play_style().chart_type();
    let steps_index = selected_steps_index_for_sync(state);
    let Some(chart_ix) = selected_chart_ix_for_sync(song.as_ref(), target_chart_type, steps_index)
    else {
        return;
    };
    let Some(chart) = song.charts.get(chart_ix) else {
        return;
    };
    let chart_label = sync_chart_label(chart);

    clear_preview(state);
    state.song_search = sort_menu::SongSearchState::Hidden;
    state.leaderboard = sort_menu::LeaderboardOverlayState::Hidden;
    state.downloads_overlay = sort_menu::DownloadsOverlayState::Hidden;
    state.replay_overlay = sort_menu::ReplayOverlayState::Hidden;
    pack_sync::hide_overlay(state);
    state.profile_switch_overlay = None;
    hide_test_input_overlay(state);
    clear_menu_chord(state);
    clear_p1_ud_chord(state);
    clear_p2_ud_chord(state);
    clear_overlay_nav_hold(state);
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.last_steps_nav_dir_p1 = None;
    state.last_steps_nav_time_p1 = None;
    state.last_steps_nav_dir_p2 = None;
    state.last_steps_nav_time_p2 = None;

    let cfg = config::null_or_die_bias_cfg();
    let kernel_target = cfg.kernel_target;
    let kernel_type = cfg.kernel_type;
    let graph_mode = config::get().null_or_die_sync_graph;
    let stream_cfg = BiasStreamCfg {
        emit_freq_delta: matches!(graph_mode, SyncGraphMode::Frequency),
        orientation: GraphOrientation::Horizontal,
    };

    let simfile_path = song.simfile_path.clone();
    let simfile_path_thread = simfile_path.clone();
    let (tx, rx) = mpsc::sync_channel::<SyncWorkerMsg>(SYNC_OVERLAY_MAX_PENDING_MSGS);
    std::thread::spawn(move || {
        let tx_done = tx.clone();
        let result = null_or_die::api::analyze_chart_stream(
            simfile_path_thread.as_path(),
            chart_ix,
            &cfg,
            stream_cfg,
            |event| {
                let _ = tx.send(SyncWorkerMsg::Event(event));
            },
        );
        let _ = tx_done.send(SyncWorkerMsg::Finished(result));
    });

    state.sync_overlay = SyncOverlayState::Visible(SyncOverlayStateData {
        simfile_path,
        song_title: song.display_full_title(false),
        chart_label,
        song_offset_seconds: song.offset,
        kernel_target,
        kernel_type,
        graph_mode,
        cols: 0,
        freq_rows: 0,
        total_beats: 0,
        digest_rows: 0,
        times_ms: Vec::new(),
        freq_domain: Vec::new(),
        beat_digest: Vec::new(),
        digest_col_sums: Vec::new(),
        post_rows: 0,
        post_kernel: Vec::new(),
        convolution: Vec::new(),
        curve_mesh: None,
        edge_discard: 2,
        beats_processed: 0,
        preview_bias_ms: None,
        final_bias_ms: None,
        final_confidence: None,
        phase: SyncOverlayPhase::Running,
        yes_selected: true,
        error_text: None,
        rx,
    });
}

fn poll_sync_overlay(overlay: &mut SyncOverlayStateData) {
    let started = Instant::now();
    let mut handled = 0usize;
    let mut refresh = SyncOverlayRefresh::default();

    loop {
        if sync_overlay_poll_exhausted(started, handled) {
            break;
        }
        match overlay.rx.try_recv() {
            Ok(SyncWorkerMsg::Event(event)) => {
                sync_overlay_apply_event(overlay, event, &mut refresh);
                handled += 1;
            }
            Ok(SyncWorkerMsg::Finished(result)) => {
                sync_overlay_apply_result(overlay, result, &mut refresh);
                handled += 1;
            }
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                if overlay.phase == SyncOverlayPhase::Running {
                    overlay.phase = SyncOverlayPhase::Failed;
                    overlay.error_text = Some("sync worker disconnected".to_string());
                }
                break;
            }
        }
    }

    refresh.flush(overlay);
}

fn handle_sync_overlay_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }

    let mut close_overlay = false;
    let mut apply_sync: Option<(PathBuf, f32)> = None;
    let mut play_change = false;
    let mut play_start = false;

    {
        let SyncOverlayState::Visible(overlay) = &mut state.sync_overlay else {
            return ScreenAction::None;
        };
        match overlay.phase {
            SyncOverlayPhase::Running | SyncOverlayPhase::Failed => match ev.action {
                VirtualAction::p1_start
                | VirtualAction::p2_start
                | VirtualAction::p1_back
                | VirtualAction::p2_back
                | VirtualAction::p1_select
                | VirtualAction::p2_select => {
                    close_overlay = true;
                    play_start = true;
                }
                _ => {}
            },
            SyncOverlayPhase::Ready => match ev.action {
                VirtualAction::p1_left
                | VirtualAction::p1_menu_left
                | VirtualAction::p1_up
                | VirtualAction::p1_menu_up
                | VirtualAction::p2_left
                | VirtualAction::p2_menu_left
                | VirtualAction::p2_up
                | VirtualAction::p2_menu_up => {
                    if !overlay.yes_selected {
                        overlay.yes_selected = true;
                        play_change = true;
                    }
                }
                VirtualAction::p1_right
                | VirtualAction::p1_menu_right
                | VirtualAction::p1_down
                | VirtualAction::p1_menu_down
                | VirtualAction::p2_right
                | VirtualAction::p2_menu_right
                | VirtualAction::p2_down
                | VirtualAction::p2_menu_down => {
                    if overlay.yes_selected {
                        overlay.yes_selected = false;
                        play_change = true;
                    }
                }
                VirtualAction::p1_start | VirtualAction::p2_start => {
                    if overlay.yes_selected
                        && let Some(delta_seconds) = sync_apply_delta_seconds(overlay)
                        && delta_seconds.abs() >= 0.000_001
                    {
                        apply_sync = Some((overlay.simfile_path.clone(), delta_seconds));
                    }
                    close_overlay = true;
                    play_start = true;
                }
                VirtualAction::p1_back
                | VirtualAction::p2_back
                | VirtualAction::p1_select
                | VirtualAction::p2_select => {
                    close_overlay = true;
                    play_start = true;
                }
                _ => {}
            },
        }
    }

    if play_change {
        audio::play_sfx("assets/sounds/change.ogg");
    }
    if play_start {
        audio::play_sfx("assets/sounds/start.ogg");
    }
    if close_overlay {
        hide_sync_overlay(state);
    }
    if let Some((simfile_path, delta_seconds)) = apply_sync {
        return ScreenAction::ApplySongOffsetSync {
            simfile_path,
            delta_seconds,
        };
    }
    ScreenAction::None
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
    crate::engine::present::runtime::clear_all();
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

fn handle_downloads_overlay_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    match sort_menu::handle_downloads_input(&mut state.downloads_overlay, ev) {
        sort_menu::DownloadsInputOutcome::ChangedSelection => {
            audio::play_sfx("assets/sounds/change.ogg");
        }
        sort_menu::DownloadsInputOutcome::Closed => {
            audio::play_sfx("assets/sounds/start.ogg");
        }
        sort_menu::DownloadsInputOutcome::None => {}
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
    clear_overlay_nav_hold(state);
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
        sort_menu::Action::ViewDownloads => {
            hide_sort_menu(state);
            show_downloads_overlay(state);
            ScreenAction::None
        }
        sort_menu::Action::SyncSong => {
            hide_sort_menu(state);
            show_sync_overlay(state);
            ScreenAction::None
        }
        sort_menu::Action::SyncPack => {
            hide_sort_menu(state);
            pack_sync::show_from_selected(state);
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
    let Some(dir) = overlay_nav_dir(ev.action) else {
        if !ev.pressed {
            return ScreenAction::None;
        }
        clear_overlay_nav_hold(state);
        return match ev.action {
            VirtualAction::p1_start | VirtualAction::p2_start => sort_menu_activate(state),
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
                ScreenAction::None
            }
            _ => ScreenAction::None,
        };
    };

    if !ev.pressed {
        release_overlay_nav_hold(state, dir);
        return ScreenAction::None;
    }

    let _ = sort_menu_move(state, overlay_nav_delta(dir));
    start_overlay_nav_hold(state, dir);
    ScreenAction::None
}

fn handle_song_search_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if matches!(state.song_search, sort_menu::SongSearchState::Hidden) {
        return ScreenAction::None;
    }

    if matches!(state.song_search, sort_menu::SongSearchState::TextEntry(_)) {
        if !ev.pressed {
            return ScreenAction::None;
        }

        let mut prompt_start = None;
        let mut prompt_close = false;
        if let sort_menu::SongSearchState::TextEntry(entry) = &mut state.song_search {
            match ev.action {
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
            }
        }

        if let Some(search_text) = prompt_start {
            start_song_search_results(state, search_text);
        } else if prompt_close {
            cancel_song_search(state);
        }
        return ScreenAction::None;
    }

    if let Some(dir) = overlay_nav_dir(ev.action) {
        if !ev.pressed {
            release_overlay_nav_hold(state, dir);
            return ScreenAction::None;
        }

        if let sort_menu::SongSearchState::Results(results) = &state.song_search
            && results.input_lock > 0.0
        {
            return ScreenAction::None;
        }

        start_overlay_nav_hold(state, dir);
        if let sort_menu::SongSearchState::Results(results) = &mut state.song_search
            && results.input_lock <= 0.0
        {
            let _ = sort_menu::song_search_move(results, overlay_nav_delta(dir));
        }
        return ScreenAction::None;
    }

    if !ev.pressed {
        return ScreenAction::None;
    }

    if let sort_menu::SongSearchState::Results(results) = &state.song_search
        && results.input_lock > 0.0
    {
        return ScreenAction::None;
    }

    clear_overlay_nav_hold(state);
    match ev.action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            let picked = if let sort_menu::SongSearchState::Results(results) = &state.song_search {
                sort_menu::song_search_focused_candidate(results).map(|c| c.song.clone())
            } else {
                None
            };
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
                            state.step_artist_cycle_base = state.session_elapsed;
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
                            debug!("Up+Down combo: Collapsing pack '{}'.", pack);
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
                    state.step_artist_cycle_base = state.session_elapsed;
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
                    debug!("Up+Down combo: Collapsing pack '{}'.", pack);
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
        Some(MusicWheelEntry::Song(song)) => {
            let song = song.clone();
            audio::play_sfx("assets/sounds/start.ogg");
            // ITGmania parity: force sample preview to start on selection finalize.
            let cfg = config::get();
            if cfg.show_select_music_previews {
                sync_preview_song(state, Some(&song), cfg.select_music_preview_loop);
            }
            state.out_prompt = OutPromptState::PressStartForOptions { elapsed: 0.0 };
            ScreenAction::None
        }
        Some(MusicWheelEntry::PackHeader { name, .. }) => {
            audio::play_sfx("assets/sounds/expand.ogg");
            let target = name.clone();
            if config::get().select_music_new_pack_mode == NewPackMode::OpenPack
                && state.new_pack_names.remove(&target)
            {
                let profile_ids = joined_local_profile_ids();
                profile::mark_pack_known(&profile_ids, &target);
            }
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

pub fn handle_raw_key_event(
    state: &mut State,
    key: Option<&RawKeyboardEvent>,
    text: Option<&str>,
) -> ScreenAction {
    if state.reload_ui.is_some() {
        return ScreenAction::None;
    }

    if !matches!(
        state.pack_sync_overlay,
        crate::screens::pack_sync::OverlayState::Hidden
    ) {
        if key.is_some_and(|key| key.pressed && key.code == KeyCode::Escape) {
            pack_sync::hide_overlay(state);
            state.song_search_ignore_next_back_select = true;
        }
        return ScreenAction::None;
    }

    if !matches!(state.sync_overlay, SyncOverlayState::Hidden) {
        if key.is_some_and(|key| key.pressed && key.code == KeyCode::Escape) {
            hide_sync_overlay(state);
            state.song_search_ignore_next_back_select = true;
        }
        return ScreenAction::None;
    }

    if !matches!(state.replay_overlay, sort_menu::ReplayOverlayState::Hidden) {
        if key.is_some_and(|key| key.pressed && key.code == KeyCode::Escape) {
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

    if key.is_some_and(|key| key.pressed) {
        if matches!(state.song_search, sort_menu::SongSearchState::Results(_))
            && key.is_some_and(|key| key.code == KeyCode::Escape)
        {
            cancel_song_search(state);
            return ScreenAction::None;
        }
        let mut prompt_start: Option<String> = None;
        let mut prompt_close = false;
        if let sort_menu::SongSearchState::TextEntry(entry) = &mut state.song_search {
            if let Some(key) = key {
                let code = key.code;
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
                && let Some(text) = text
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
    } else if key.is_none() && text.is_some() {
        if let sort_menu::SongSearchState::TextEntry(entry) = &mut state.song_search {
            if let Some(text) = text {
                sort_menu::song_search_add_text(entry, text);
            }
            return ScreenAction::None;
        }
    }

    if !key.is_some_and(|key| key.pressed) {
        return ScreenAction::None;
    }
    if key.is_some_and(|key| key.code == KeyCode::F7) {
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
    update_select_hold_state(state, ev);

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

    if !matches!(
        state.pack_sync_overlay,
        crate::screens::pack_sync::OverlayState::Hidden
    ) {
        return pack_sync::handle_input(state, ev);
    }

    if !matches!(state.sync_overlay, SyncOverlayState::Hidden) {
        return handle_sync_overlay_input(state, ev);
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

    if !matches!(
        state.downloads_overlay,
        sort_menu::DownloadsOverlayState::Hidden
    ) {
        return handle_downloads_overlay_input(state, ev);
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
            VirtualAction::p1_start if ev.pressed => {
                if try_open_sort_menu_with_select_start(state, state.p1_select_held, ev.pressed) {
                    ScreenAction::None
                } else {
                    handle_confirm(state)
                }
            }
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
            VirtualAction::p2_start if ev.pressed => {
                if try_open_sort_menu_with_select_start(state, state.p2_select_held, ev.pressed) {
                    ScreenAction::None
                } else {
                    handle_confirm(state)
                }
            }
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
            VirtualAction::p2_start if ev.pressed => {
                if try_open_sort_menu_with_select_start(state, state.p2_select_held, ev.pressed) {
                    ScreenAction::None
                } else {
                    handle_confirm(state)
                }
            }
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
            VirtualAction::p1_start if ev.pressed => {
                if try_open_sort_menu_with_select_start(state, state.p1_select_held, ev.pressed) {
                    ScreenAction::None
                } else {
                    handle_confirm(state)
                }
            }
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
        update_overlay_nav_hold(state);
        return ScreenAction::None;
    }
    if pack_sync::poll(state) {
        return ScreenAction::None;
    }
    if let SyncOverlayState::Visible(overlay) = &mut state.sync_overlay {
        poll_sync_overlay(overlay);
        return ScreenAction::None;
    }
    if sort_menu::update_replay_overlay(&mut state.replay_overlay, dt) {
        return ScreenAction::None;
    }
    if let Some(overlay) = state.profile_switch_overlay.as_mut() {
        profile_boxes::update(overlay, dt);
        return ScreenAction::None;
    }
    if crate::game::online::downloads::take_ready_song_reload_request() {
        start_reload_songs_and_courses(state);
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
    sort_menu::update_downloads_overlay(&mut state.downloads_overlay, dt);

    state.time_since_selection_change += dt;
    if dt > 0.0 {
        state.selection_animation_timer += dt;
        if state.cdtitle_spin_elapsed < CDTITLE_SPIN_SECONDS {
            state.cdtitle_spin_elapsed =
                (state.cdtitle_spin_elapsed + dt).min(CDTITLE_SPIN_SECONDS);
        }
        state.cdtitle_anim_elapsed += dt;
        if state.sort_menu != sort_menu::State::Hidden
            && state.sort_menu_focus_anim_elapsed < sort_menu::FOCUS_TWEEN_SECONDS
        {
            state.sort_menu_focus_anim_elapsed =
                (state.sort_menu_focus_anim_elapsed + dt).min(sort_menu::FOCUS_TWEEN_SECONDS);
        }
    }
    if state.sort_menu != sort_menu::State::Hidden {
        update_overlay_nav_hold(state);
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
        state.step_artist_cycle_base = state.session_elapsed;
        state.cdtitle_spin_elapsed = 0.0;
        state.cdtitle_anim_elapsed = 0.0;

        if matches!(
            state.entries.get(state.selected_index),
            Some(MusicWheelEntry::PackHeader { .. })
        ) {
            state.displayed_chart_p1 = None;
            state.displayed_chart_p2 = None;
        }

        if let Some(MusicWheelEntry::Song(song)) = state.entries.get(state.selected_index) {
            let target_chart_type = profile::get_session_play_style().chart_type();
            if let Some(idx) =
                best_steps_index(song, target_chart_type, state.preferred_difficulty_index)
            {
                state.selected_steps_index = idx;
                if idx < color::FILE_DIFFICULTY_NAMES.len() {
                    state.preferred_difficulty_index = idx;
                }
            }
            if let Some(idx) =
                best_steps_index(song, target_chart_type, state.p2_preferred_difficulty_index)
            {
                state.p2_selected_steps_index = idx;
                if idx < color::FILE_DIFFICULTY_NAMES.len() {
                    state.p2_preferred_difficulty_index = idx;
                }
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

    let overlays_block_delayed_updates = state.sort_menu != sort_menu::State::Hidden
        || !matches!(
            state.leaderboard,
            sort_menu::LeaderboardOverlayState::Hidden
        )
        || !matches!(
            state.pack_sync_overlay,
            crate::screens::pack_sync::OverlayState::Hidden
        )
        || !matches!(state.sync_overlay, SyncOverlayState::Hidden)
        || !matches!(state.replay_overlay, sort_menu::ReplayOverlayState::Hidden)
        || state.profile_switch_overlay.is_some()
        || state.test_input_overlay_visible;
    if overlays_block_delayed_updates && state.currently_playing_preview_path.is_some() {
        clear_preview(state);
    }

    let cfg = config::get();
    maybe_clear_selected_pack_on_score(state, cfg.select_music_new_pack_mode);

    // Keep banner/CDTitle aligned to the restored wheel selection even while
    // overlays are visible; only preview/GS fetches are paused under overlays.
    let (selected_song, selected_pack) = match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(s)) => (Some(s.clone()), None),
        Some(MusicWheelEntry::PackHeader {
            name, banner_path, ..
        }) => (None, Some((name, banner_path))),
        None => (None, None),
    };

    let new_banner = if cfg.show_select_music_banners {
        selected_song
            .as_ref()
            .and_then(|s| s.banner_path.clone())
            .or_else(|| {
                selected_pack
                    .as_ref()
                    .and_then(|(_, p)| p.as_ref().cloned())
            })
    } else {
        None
    };
    let new_cdtitle = if cfg.show_select_music_cdtitles {
        selected_song.as_ref().and_then(|s| s.cdtitle_path.clone())
    } else {
        None
    };

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
    if state.last_requested_cdtitle_path != new_cdtitle {
        if new_cdtitle.is_some() {
            state.cdtitle_spin_elapsed = 0.0;
            state.cdtitle_anim_elapsed = 0.0;
        }
        state.last_requested_cdtitle_path = new_cdtitle.clone();
        return ScreenAction::RequestCdTitle(new_cdtitle);
    }

    if overlays_block_delayed_updates {
        return ScreenAction::None;
    }

    // --- Delayed Updates ---
    if cfg.show_select_music_previews && allow_gs_fetch_for_selection(state) {
        sync_preview_song(state, selected_song.as_ref(), cfg.select_music_preview_loop);
    } else if state.currently_playing_preview_path.is_some() {
        clear_preview(state);
    }

    if allow_gs_fetch_for_selection(state) {
        let play_style = profile::get_session_play_style();
        let target_chart_type = play_style.chart_type();
        let show_select_music_leaderboards = cfg.show_select_music_scorebox
            && (cfg.select_music_scorebox_cycle_itg
                || cfg.select_music_scorebox_cycle_ex
                || cfg.select_music_scorebox_cycle_hard_ex
                || cfg.select_music_scorebox_cycle_tournaments);

        if let Some(song) = selected_song.as_ref() {
            let is_versus = play_style == crate::game::profile::PlayStyle::Versus;
            ensure_chart_cache_for_song(state, song, target_chart_type, is_versus);

            if !displayed_chart_matches(
                state.displayed_chart_p1.as_ref(),
                song,
                state.cached_chart_ix_p1,
            ) {
                state.displayed_chart_p1 =
                    state.cached_chart_ix_p1.map(|chart_ix| DisplayedChart {
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
                            measure_seconds_vec: c.measure_seconds_vec.clone(),
                            first_second: c.first_second,
                            last_second: song.precise_last_second(),
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
                    state.displayed_chart_p2 =
                        state.cached_chart_ix_p2.map(|chart_ix| DisplayedChart {
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
                                measure_seconds_vec: c.measure_seconds_vec.clone(),
                                first_second: c.first_second,
                                last_second: song.precise_last_second(),
                            }
                        }),
                    };
                }
                if show_select_music_leaderboards {
                    maybe_refresh_select_music_leaderboard(
                        &mut state.last_refreshed_leaderboard_hash_p2,
                        profile::PlayerSide::P2,
                        desired_hash_p2,
                    );
                }
            } else {
                state.displayed_chart_p2 = None;
            }
            if show_select_music_leaderboards {
                let primary_side = if is_versus {
                    profile::PlayerSide::P1
                } else {
                    profile::get_session_player_side()
                };
                maybe_refresh_select_music_leaderboard(
                    &mut state.last_refreshed_leaderboard_hash,
                    primary_side,
                    desired_hash_p1,
                );
            }
        } else {
            state.displayed_chart_p1 = None;
            state.displayed_chart_p2 = None;
            state.cached_song = None;
            state.cached_chart_ix_p1 = None;
            state.cached_chart_ix_p2 = None;
            state.cached_edits = None;
            state.cached_standard_chart_ixs = [None; NUM_STANDARD_DIFFICULTIES];
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
    state.last_requested_cdtitle_path = None;
    state.banner_high_quality_requested = false;
    state.cdtitle_spin_elapsed = 0.0;
    state.cdtitle_anim_elapsed = 0.0;
}

pub fn refresh_from_song_cache(state: &mut State) {
    refresh_after_reload(state);
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
    // Treat evaluation -> SelectMusic like a fresh chart visit so the existing
    // scorebox snapshot stays visible while the current chart is refreshed.
    state.last_refreshed_leaderboard_hash = None;
    state.last_refreshed_leaderboard_hash_p2 = None;
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
    state.displayed_chart_p2 = state
        .cached_chart_ix_p2
        .map(|chart_ix| DisplayedChart { song, chart_ix });
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
fn format_session_time(seconds: f32) -> Arc<str> {
    let s = if !seconds.is_finite() || seconds < 0.0 {
        0_u64
    } else {
        seconds as u64
    };
    let key = s.min(u32::MAX as u64) as u32;
    cached_text(&SESSION_TIME_CACHE, key, || {
        let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
        if s < 3600 {
            format!("{m:02}:{sec:02}")
        } else if s < 36000 {
            format!("{h}:{m:02}:{sec:02}")
        } else {
            format!("{h:02}:{m:02}:{sec:02}")
        }
    })
}

fn format_chart_length(seconds: i32) -> Arc<str> {
    let key = seconds.max(0);
    cached_text(&CHART_LENGTH_CACHE, key, || {
        let s = key as u64;
        let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
        if h > 0 {
            format!("{h}:{m:02}:{s:02}")
        } else {
            format!("{m}:{s:02}")
        }
    })
}

#[inline(always)]
fn allow_gs_fetch_for_selection(state: &State) -> bool {
    state.nav_key_held_direction.is_none()
        && state.wheel_offset_from_selection.abs() < 0.0001
        && state.time_since_selection_change >= PREVIEW_DELAY_SECONDS
}

#[inline(always)]
fn maybe_refresh_select_music_leaderboard(
    last_refreshed_hash: &mut Option<String>,
    side: profile::PlayerSide,
    chart_hash: Option<&str>,
) {
    let Some(chart_hash) = chart_hash else {
        return;
    };
    if last_refreshed_hash.as_deref() == Some(chart_hash) || !scores::is_gs_active_for_side(side) {
        return;
    }
    let _ = scores::refresh_player_leaderboards_for_side(
        chart_hash,
        side,
        SELECT_MUSIC_LEADERBOARD_NUM_ENTRIES,
    );
    *last_refreshed_hash = Some(chart_hash.to_string());
}

/// Selects the step artist display text for a chart, cycling through non-empty
/// values of [step_artist, description, chart_name] every 2 seconds, matching
/// Simply Love / ITGMania behavior.
fn step_artist_cycle_text<'a>(chart: &'a ChartData, cycle_elapsed: f32) -> &'a str {
    let candidates: [&str; 3] = [
        chart.step_artist.as_str(),
        chart.description.as_str(),
        chart.chart_name.as_str(),
    ];
    let mut non_empty: Vec<&str> = Vec::with_capacity(3);
    for &s in &candidates {
        if !s.trim().is_empty() && !non_empty.iter().any(|&prev| prev == s) {
            non_empty.push(s);
        }
    }
    match non_empty.len() {
        0 => "",
        1 => non_empty[0],
        n => {
            let idx = (cycle_elapsed / STEP_ARTIST_CYCLE_SECONDS).floor().max(0.0) as usize % n;
            non_empty[idx]
        }
    }
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
    let immediate_chart_p1 = selected_song.and_then(|song| {
        chart_for_steps_index(song, target_chart_type, state.selected_steps_index)
    });
    let immediate_chart_p2 = if is_versus {
        selected_song.and_then(|song| {
            chart_for_steps_index(song, target_chart_type, state.p2_selected_steps_index)
        })
    } else {
        None
    };
    let allow_gs_fetch = allow_gs_fetch_for_selection(state);
    let cfg = config::get();

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));
    actors.push(sl_select_music_bg_flash());
    actors.extend(screen_bars::build("SELECT MUSIC"));

    let p1_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P1);
    let p2_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P2);

    let mode_side = if is_p2_single {
        profile::PlayerSide::P2
    } else {
        profile::PlayerSide::P1
    };
    let scorebox_cycle_enabled = cfg.select_music_scorebox_cycle_itg
        || cfg.select_music_scorebox_cycle_ex
        || cfg.select_music_scorebox_cycle_hard_ex
        || cfg.select_music_scorebox_cycle_tournaments;
    let mode_chart_hash =
        if allow_gs_fetch && cfg.show_select_music_scorebox && scorebox_cycle_enabled {
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
    actors.push(timers::build_session(format_session_time(
        state.session_elapsed,
    )));
    if cfg.show_select_music_gameplay_timer {
        actors.push(timers::build_gameplay(format_session_time(
            state.gameplay_elapsed,
        )));
    }

    // Pads
    {
        actors.push(mode_pads::build_label(score_mode_text.as_str()));
        actors.extend(mode_pads::build());
    }

    // Banner
    let (banner_zoom, banner_cx, banner_cy) = if is_wide() {
        (0.7655, screen_center_x() - 170.0, 96.0)
    } else {
        (0.75, screen_center_x() - 166.0, 96.0)
    };
    let banner_key = if cfg.show_select_music_banners {
        state.current_banner_key.clone()
    } else {
        fallback_banner_key(state.active_color_index)
    };
    actors.push(shared_banner::sprite(
        banner_key,
        banner_cx,
        banner_cy,
        BANNER_NATIVE_WIDTH,
        BANNER_NATIVE_HEIGHT,
        banner_zoom,
        51,
    ));
    if cfg.show_select_music_cdtitles
        && let Some(cdtitle_key) = state.current_cdtitle_key.as_ref()
        && asset_manager.has_texture_key(cdtitle_key)
        && let Some(tex) = crate::assets::texture_dims(cdtitle_key)
    {
        let (cols, rows) = crate::assets::sprite_sheet_dims(cdtitle_key);
        let cols = cols.max(1);
        let rows = rows.max(1);
        let frame_w = (tex.w.max(1) as f32) / cols as f32;
        let frame_h = (tex.h.max(1) as f32) / rows as f32;
        let dim1 = frame_w.max(frame_h);
        let dim2 = frame_w.min(frame_h).max(1.0);
        let ratio = (dim1 / dim2).max(CDTITLE_RATIO_MIN);
        let to_scale = dim1.max(1.0);
        let cdtitle_x = banner_cx + CDTITLE_OFFSET_X * banner_zoom;
        let cdtitle_y = banner_cy + CDTITLE_OFFSET_Y * banner_zoom;
        let cdtitle_zoom = (CDTITLE_ZOOM_BASE / to_scale) * ratio * banner_zoom;
        let cdtitle_rot =
            360.0 * (state.cdtitle_spin_elapsed / CDTITLE_SPIN_SECONDS).clamp(0.0, 1.0);
        let total_frames = cols.saturating_mul(rows).max(1);
        let cdtitle_frame = if total_frames > 1 {
            ((state.cdtitle_anim_elapsed / CDTITLE_FRAME_DELAY_SECONDS)
                .floor()
                .max(0.0) as u32)
                % total_frames
        } else {
            0
        };
        actors.push(act!(sprite(cdtitle_key.clone()): align(0.5, 0.5): xy(cdtitle_x, cdtitle_y): zoom(cdtitle_zoom): rotationy(cdtitle_rot): setstate(cdtitle_frame): z(101)));
    }

    let music_rate = crate::game::profile::get_session_music_rate();
    if (music_rate - 1.0).abs() > 0.001 {
        let text = cached_music_rate_banner_text(music_rate);
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
    let (artist, bpm, len_text): (String, String, Arc<str>) = match entry_opt {
        Some(MusicWheelEntry::Song(s)) => {
            let bpm = match immediate_chart_p1.and_then(|c| c.display_bpm.as_ref()) {
                Some(ChartDisplayBpm::Random) => random_bpm_cycle_text(state.session_elapsed),
                _ => format_bpm_with_rate(
                    s.chart_display_bpm_range(immediate_chart_p1),
                    music_rate,
                ),
            };
            (
                s.artist.clone(),
                bpm,
                format_chart_length(((s.total_length_seconds.max(0) as f32) / music_rate) as i32),
            )
        }
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
        None => ("".to_string(), "".to_string(), Arc::<str>::from("")),
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

    let cycle_elapsed = state.session_elapsed - state.step_artist_cycle_base;

    let (step_artist, steps, jumps, holds, mines, hands, rolls, meter) =
        if let Some(c) = immediate_chart_p1 {
            (
                step_artist_cycle_text(c, cycle_elapsed),
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
        step_artist_cycle_text(c, cycle_elapsed)
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
    let graph_h = 64.0_f32;
    let graph_body_h = 47.0_f32;
    let chart_info_cx = screen_center_x() - 182.0 - if is_wide() { 5.0 } else { 0.0 };
    let graph_left = chart_info_cx - 0.5 * panel_w;
    let (window_w_px, _) = current_window_px();
    let marker_col_w = if window_w_px > 0 {
        screen_width() / window_w_px as f32
    } else {
        1.0
    };
    let breakdown_style = cfg.select_music_breakdown_style;
    let pattern_info_mode = cfg.select_music_pattern_info_mode;
    let preview_sec = if cfg.show_select_music_preview_marker {
        preview_song_sec(state)
    } else {
        None
    };
    let preview_marker_p1 = preview_marker(
        state.displayed_chart_p1.as_ref(),
        preview_sec,
        graph_left,
        panel_w,
    );
    let preview_marker_p2 = preview_marker(
        state.displayed_chart_p2.as_ref(),
        preview_sec,
        graph_left,
        panel_w,
    );
    let build_breakdown_panel = |graph_cy: f32,
                                 is_p2_layout: bool,
                                 graph_key: &String,
                                 graph_mesh: Option<Arc<[MeshVertex]>>,
                                 preview_marker: Option<PreviewMarker>,
                                 chart: Option<&ChartData>| {
        let mut graph_kids = vec![
            act!(quad: align(0.0, 0.0): xy(0.0, 0.0): setsize(panel_w, graph_h): diffuse(UI_BOX_BG_COLOR[0], UI_BOX_BG_COLOR[1], UI_BOX_BG_COLOR[2], UI_BOX_BG_COLOR[3])),
        ];

        if let Some(c) = chart {
            let scaled_peak_nps = if music_rate.is_finite() {
                c.max_nps * music_rate as f64
            } else {
                c.max_nps
            };
            let peak = cached_chart_info_text(
                cfg.select_music_chart_info_peak_nps,
                cfg.select_music_chart_info_matrix_rating,
                c.meter,
                scaled_peak_nps,
                c.matrix_rating,
            );
            // Match Simply Love's minimization loop (0 -> 3) based on rendered width.
            let bd_text = asset_manager
                .with_fonts(|all_fonts| {
                    asset_manager.with_font("miso", |miso_font| -> Option<Arc<str>> {
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
                            Some(cached_str_ref(detailed_breakdown))
                        } else if fits(partial_breakdown) {
                            Some(cached_str_ref(partial_breakdown))
                        } else if fits(simple_breakdown) {
                            Some(cached_str_ref(simple_breakdown))
                        } else {
                            Some(cached_total_label_text(c.total_streams))
                        }
                    })
                })
                .flatten()
                .unwrap_or_else(|| match breakdown_style {
                    BreakdownStyle::Sl => cached_str_ref(&c.simple_breakdown),
                    BreakdownStyle::Sn => cached_str_ref(&c.sn_simple_breakdown),
                });

            let peak_x = panel_w * 0.5 + if is_p2_layout { -136.0 } else { 60.0 };
            if let Some(mesh) = graph_mesh
                && !mesh.is_empty()
            {
                graph_kids.push(Actor::Mesh {
                    align: [0.0, 0.0],
                    offset: [0.0, 0.0],
                    size: [SizeSpec::Px(panel_w), SizeSpec::Px(graph_h)],
                    vertices: mesh,
                    mode: MeshMode::Triangles,
                    visible: true,
                    blend: BlendMode::Alpha,
                    z: 0,
                });
            } else if graph_key != "__white" {
                graph_kids.push(act!(sprite(graph_key.clone()):
                    align(0.0, 0.0): xy(0.0, 0.0): setsize(panel_w, graph_h)
                ));
            }
            if let Some(marker) = preview_marker {
                for col in marker.cols.iter().take(marker.len) {
                    graph_kids.push(act!(quad:
                        align(0.0, 0.0):
                        xy(col.x, 0.0):
                        setsize(marker_col_w, graph_h):
                        diffuse(1.0, 1.0, 1.0, col.a):
                        z(1)
                    ));
                }
            }
            graph_kids.push(act!(text: font("miso"): settext(peak): align(0.0, 0.5): xy(peak_x, -9.0): zoom(0.8): diffuse(1.0, 1.0, 1.0, 1.0): z(2)));
            graph_kids.push(act!(quad: align(0.0, 0.0): xy(0.0, graph_body_h): setsize(panel_w, graph_h - graph_body_h): diffuse(0.0, 0.0, 0.0, 0.5): z(2)));
            graph_kids.push(act!(text: font("miso"): settext(bd_text): align(0.5, 0.5): xy(panel_w * 0.5, 55.5): zoom(0.8): maxwidth(panel_w): z(2)));
        }

        Actor::Frame {
            align: [0.0, 0.0],
            offset: [graph_left, graph_cy - 32.0],
            size: [SizeSpec::Px(panel_w), SizeSpec::Px(graph_h)],
            background: None,
            z: 51,
            children: graph_kids,
        }
    };

    if cfg.show_select_music_breakdown {
        if is_versus {
            actors.push(build_breakdown_panel(
                screen_center_y() + 23.0,
                false,
                &state.current_graph_key,
                state.current_graph_mesh.clone(),
                preview_marker_p1,
                disp_chart_p1,
            ));
            actors.push(build_breakdown_panel(
                screen_center_y() + 111.0,
                true,
                &state.current_graph_key_p2,
                state.current_graph_mesh_p2.clone(),
                preview_marker_p2,
                disp_chart_p2,
            ));
        } else {
            let graph_cy = screen_center_y() + if is_p2_single { 111.0 } else { 23.0 };
            actors.push(build_breakdown_panel(
                graph_cy,
                is_p2_single,
                &state.current_graph_key,
                state.current_graph_mesh.clone(),
                preview_marker_p1,
                disp_chart_p1,
            ));
        }
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
        let gs_active = scores::is_gs_active_for_side(side);
        let show_rivals = gs_active && cfg.show_select_music_scorebox && scorebox_cycle_enabled;
        let show_ex_score = profile::get_for_side(side).show_ex_score;

        let chart_hash = if allow_gs_fetch && show_rivals {
            chart.map(|c| c.short_hash.as_str())
        } else {
            None
        };
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
            meter: (!show_rivals).then_some(meter),
        });

        if show_rivals {
            let placeholder = (
                "----".to_string(),
                gs_scorebox::unknown_score_percent_text(),
            );
            let gs_view = gs_scorebox::select_music_scorebox_view(
                side,
                chart_hash,
                placeholder.clone(),
                placeholder,
            );

            // Simply Love PaneDisplay order: Machine/World first, then Player.
            let lines = [
                (
                    gs_view.machine_name.as_str(),
                    gs_view.machine_score.as_ref(),
                ),
                (gs_view.player_name.as_str(), gs_view.player_score.as_ref()),
            ];
            for i in 0..2 {
                let (name, pct) = lines[i];
                out.push(act!(text: font("miso"): settext(name): align(0.5, 0.5): xy(pane_cx + cols[2] - 50.0 * tz, pane_top + rows[i]): maxwidth(30.0): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
                out.push(act!(text: font("miso"): settext(pct): align(1.0, 0.5): xy(pane_cx + cols[2] + 25.0 * tz, pane_top + rows[i]): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
            }
            let score_mode_label_storage = format!("{} Score", gs_view.mode_text);
            let score_mode_label = gs_view
                .loading_text
                .as_deref()
                .unwrap_or(score_mode_label_storage.as_str());
            out.push(act!(text: font("miso"): settext(score_mode_label): align(0.5, 0.5): xy(pane_cx + cols[2] - 15.0, pane_top + rows[2]): maxwidth(90.0): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0): horizalign(center)));
            if gs_view.show_rivals {
                for i in 0..3 {
                    let (name, pct) = (&gs_view.rivals[i].0, &gs_view.rivals[i].1);
                    let pct = pct.as_ref();
                    out.push(act!(text: font("miso"): settext(name): align(0.5, 0.5): xy(pane_cx + cols[2] + 50.0 * tz, pane_top + rows[i]): maxwidth(30.0): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
                    out.push(act!(text: font("miso"): settext(pct): align(1.0, 0.5): xy(pane_cx + cols[2] + 125.0 * tz, pane_top + rows[i]): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
                }
            }
        } else {
            let mut player_name = "----";
            let mut player_score = placeholder_score_percent();
            if let Some(c) = chart
                && let Some(sc) = scores::get_cached_local_score_for_side(&c.short_hash, side)
                && (sc.grade != scores::Grade::Failed || sc.score_percent > 0.0)
            {
                player_name = player_initials;
                player_score = cached_score_percent_text(sc.score_percent);
            }

            let mut machine_name_storage: Option<String> = None;
            let mut machine_score = placeholder_score_percent();
            if let Some(c) = chart
                && let Some((initials, sc)) = scores::get_machine_record_local(&c.short_hash)
                && (sc.grade != scores::Grade::Failed || sc.score_percent > 0.0)
            {
                machine_name_storage = Some(initials);
                machine_score = cached_score_percent_text(sc.score_percent);
            }
            let machine_name = machine_name_storage.as_deref().unwrap_or("----");

            let names = [machine_name, player_name];
            let scores = [machine_score, player_score];
            for i in 0..2 {
                out.push(act!(text: font("miso"): settext(names[i]): align(0.5, 0.5): xy(pane_cx + cols[2] - 50.0 * tz, pane_top + rows[i]): maxwidth(30.0): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
                out.push(act!(text: font("miso"): settext(scores[i].clone()): align(1.0, 0.5): xy(pane_cx + cols[2] + 25.0 * tz, pane_top + rows[i]): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
            }
            out.push(act!(text: font("miso"): settext(if show_ex_score { "EX Score" } else { "ITG Score" }): align(0.5, 0.5): xy(pane_cx + cols[2] - 15.0, pane_top + rows[2]): maxwidth(90.0): zoom(tz): z(121): diffuse(0.0, 0.0, 0.0, 1.0): horizalign(center)));
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
            ): (
                Arc<str>,
                Arc<str>,
                Arc<str>,
                Arc<str>,
                Arc<str>,
                Arc<str>,
                Arc<str>,
                Arc<str>,
                Arc<str>,
                Arc<str>,
                Arc<str>,
                Arc<str>,
                Arc<str>,
            ) = if let Some(c) = disp_chart_p1 {
                (
                    cached_u32_text(c.stamina_counts.boxes),
                    cached_u32_text(c.stamina_counts.anchors),
                    cached_u32_text(c.stamina_counts.staircases),
                    cached_u32_text(c.stamina_counts.sweeps),
                    cached_u32_text(c.stamina_counts.towers),
                    cached_u32_text(c.stamina_counts.triangles),
                    cached_u32_text(c.stamina_counts.doritos),
                    cached_u32_text(c.stamina_counts.hip_breakers),
                    cached_u32_text(c.stamina_counts.copters),
                    cached_u32_text(c.stamina_counts.spirals),
                    cached_stamina_mono_text(c.stamina_counts.mono_percent),
                    cached_stamina_candles_text(c.stamina_counts.candle_percent),
                    cached_stream_total_text(c.total_streams, chart_stream_percent(c)),
                )
            } else {
                (
                    cached_u32_text(0),
                    cached_u32_text(0),
                    cached_u32_text(0),
                    cached_u32_text(0),
                    cached_u32_text(0),
                    cached_u32_text(0),
                    cached_u32_text(0),
                    cached_u32_text(0),
                    cached_u32_text(0),
                    cached_u32_text(0),
                    cached_stamina_mono_text(0.0),
                    cached_stamina_candles_text(0.0),
                    cached_stream_total_text(0, 0.0),
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

            let push_pattern_line = |actors: &mut Vec<Actor>,
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
                boxes.as_ref(),
                "Boxes",
            );
            push_pattern_line(
                &mut actors,
                col1_left,
                col_w1,
                col1_num_x,
                1,
                anchors.as_ref(),
                "Anchors",
            );
            push_pattern_line(
                &mut actors,
                col1_left,
                col_w1,
                col1_num_x,
                2,
                staircases.as_ref(),
                "Staircases",
            );
            push_pattern_line(
                &mut actors,
                col1_left,
                col_w1,
                col1_num_x,
                3,
                sweeps.as_ref(),
                "Sweeps",
            );

            push_pattern_line(
                &mut actors,
                col2_left,
                col_w2,
                col2_num_x,
                0,
                triangles.as_ref(),
                "Triangles",
            );
            push_pattern_line(
                &mut actors,
                col2_left,
                col_w2,
                col2_num_x,
                1,
                hip_breakers.as_ref(),
                "Hip Breakers",
            );
            push_pattern_line(
                &mut actors,
                col2_left,
                col_w2,
                col2_num_x,
                2,
                doritos.as_ref(),
                "Doritos",
            );
            push_pattern_line(
                &mut actors,
                col2_left,
                col_w2,
                col2_num_x,
                3,
                towers.as_ref(),
                "Towers",
            );

            push_pattern_line(
                &mut actors,
                col3_left,
                col_w3,
                col3_num_x,
                0,
                spirals.as_ref(),
                "Spirals",
            );
            push_pattern_line(
                &mut actors,
                col3_left,
                col_w3,
                col3_num_x,
                1,
                copters.as_ref(),
                "Copters",
            );

            let col3_label_x = col3_num_x + 3.0;
            let col3_num_w = (col3_num_x - col3_left).max(8.0);
            let col3_label_w = (col3_left + col_w3 - col3_label_x - 2.0).max(8.0);
            let relaxed_num_w = col3_num_w * 1.65;

            let mono_y = stamina_base_y + 2.0 * stamina_row_step;
            actors.push(act!(text: font("miso"): settext(mono_value): align(1.0, 0.5): horizalign(right): xy(col3_num_x, mono_y): maxwidth(relaxed_num_w): zoom(stamina_zoom): z(121): diffuse(1.0, 1.0, 1.0, 1.0)));
            actors.push(act!(text: font("miso"): settext(candles_value): align(0.0, 0.5): horizalign(left): xy(col3_label_x, mono_y): maxwidth(col3_label_w): zoom(stamina_zoom): z(121): diffuse(1.0, 1.0, 1.0, 1.0)));

            let stream_y = stamina_base_y + 3.0 * stamina_row_step;
            actors.push(act!(text: font("miso"): settext(total_stream): align(1.0, 0.5): horizalign(right): xy(col3_num_x, stream_y): maxwidth(relaxed_num_w): zoom(stamina_zoom): z(121): diffuse(1.0, 1.0, 1.0, 1.0)));
            actors.push(act!(text: font("miso"): settext("Total Stream"): align(0.0, 0.5): horizalign(left): xy(col3_label_x, stream_y): maxwidth(col3_label_w): zoom(stamina_zoom): z(121): diffuse(1.0, 1.0, 1.0, 1.0)));
        } else {
            let (cross, foot, side, jack, brack, stream): (
                Arc<str>,
                Arc<str>,
                Arc<str>,
                Arc<str>,
                Arc<str>,
                Arc<str>,
            ) = if let Some(c) = disp_chart_p1 {
                (
                    cached_u32_text(c.tech_counts.crossovers),
                    cached_u32_text(c.tech_counts.footswitches),
                    cached_u32_text(c.tech_counts.sideswitches),
                    cached_u32_text(c.tech_counts.jacks),
                    cached_u32_text(c.tech_counts.brackets),
                    if c.total_measures > 0 {
                        cached_tech_stream_text(
                            c.total_streams,
                            c.total_measures,
                            chart_stream_percent(c),
                        )
                    } else {
                        Arc::<str>::from("None (0.0%)")
                    },
                )
            } else {
                (
                    cached_u32_text(0),
                    cached_u32_text(0),
                    cached_u32_text(0),
                    cached_u32_text(0),
                    cached_u32_text(0),
                    Arc::<str>::from("None (0.0%)"),
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
            let cached_standard_indices = state.cached_song.as_ref().and_then(|cached_song| {
                if Arc::ptr_eq(cached_song, song) && state.cached_chart_type == target_chart_type {
                    Some(&state.cached_standard_chart_ixs)
                } else {
                    None
                }
            });
            let mut v: Vec<Option<&ChartData>> = Vec::with_capacity(NUM_STANDARD_DIFFICULTIES);
            for diff_ix in 0..NUM_STANDARD_DIFFICULTIES {
                let chart = if let Some(indices) = cached_standard_indices {
                    indices[diff_ix].and_then(|ix| song.charts.get(ix))
                } else {
                    let diff = color::FILE_DIFFICULTY_NAMES[diff_ix];
                    song.charts.iter().find(|c| {
                        c.chart_type.eq_ignore_ascii_case(target_chart_type)
                            && c.difficulty.eq_ignore_ascii_case(diff)
                    })
                };
                v.push(chart);
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
            vec![None; NUM_STANDARD_DIFFICULTIES],
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
        song_text_color_overrides: None,
        song_has_edit_ptrs: Some(&state.song_has_edit_ptrs),
        show_music_wheel_grades: cfg.show_music_wheel_grades,
        show_music_wheel_lamps: cfg.show_music_wheel_lamps,
        itl_wheel_mode: cfg.select_music_itl_wheel_mode,
        allow_online_fetch: allow_gs_fetch,
        new_pack_names: (state.sort_mode == WheelSortMode::Group).then_some(&state.new_pack_names),
    }));
    actors.extend(sl_select_music_wheel_cascade_mask());

    // GrooveStats scorebox placement.
    // Auto keeps the current layout, including pane placement for both-GS versus.
    // StepPane forces the scorebox into the pane area whenever it is shown.
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
        let p1_gs = scores::is_gs_active_for_side(profile::PlayerSide::P1);
        let p2_gs = scores::is_gs_active_for_side(profile::PlayerSide::P2);
        let both_gs_versus = is_versus && p1_gs && p2_gs;
        let force_step_pane =
            cfg.select_music_scorebox_placement == SelectMusicScoreboxPlacement::StepPane;
        let mut push_scorebox = |side: profile::PlayerSide,
                                 steps_idx: usize,
                                 center_x: f32,
                                 center_y: f32,
                                 zoom: f32,
                                 z_boost: i16| {
            let chart_hash =
                if allow_gs_fetch && cfg.show_select_music_scorebox && scorebox_cycle_enabled {
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
            let scorebox = gs_scorebox::select_music_scorebox_actors(
                side,
                chart_hash,
                cfg.show_select_music_scorebox && scorebox_cycle_enabled,
                center_x,
                center_y,
                zoom,
                state.selection_animation_timer,
            );
            if z_boost == 0 || scorebox.is_empty() {
                actors.extend(scorebox);
            } else {
                actors.push(Actor::Frame {
                    align: [0.0, 0.0],
                    offset: [0.0, 0.0],
                    size: [SizeSpec::Fill, SizeSpec::Fill],
                    background: None,
                    z: z_boost,
                    children: scorebox,
                });
            }
        };
        let pane_scorebox_zoom = widescale(0.60, 0.64);
        let pane_scorebox_width = 162.0 * pane_scorebox_zoom;
        let pane_scorebox_center_y = pane_layout.pane_top + pane_layout.pane_height * 0.5;
        let pane_right_inset = 4.0;
        let pane_box_center_x = |pane_cx: f32| {
            pane_cx + pane_layout.pane_width * 0.5 - pane_scorebox_width * 0.5 - pane_right_inset
        };

        if both_gs_versus || force_step_pane {
            if is_versus {
                push_scorebox(
                    profile::PlayerSide::P1,
                    state.selected_steps_index,
                    pane_box_center_x(screen_width() * 0.25 - 5.0),
                    pane_scorebox_center_y,
                    pane_scorebox_zoom,
                    60,
                );
                push_scorebox(
                    profile::PlayerSide::P2,
                    state.p2_selected_steps_index,
                    pane_box_center_x(screen_width() * 0.75 + 5.0),
                    pane_scorebox_center_y,
                    pane_scorebox_zoom,
                    60,
                );
            } else if is_p2_single {
                push_scorebox(
                    profile::PlayerSide::P2,
                    state.p2_selected_steps_index,
                    pane_box_center_x(screen_width() * 0.75 + 5.0),
                    pane_scorebox_center_y,
                    pane_scorebox_zoom,
                    60,
                );
            } else {
                push_scorebox(
                    profile::PlayerSide::P1,
                    state.selected_steps_index,
                    pane_box_center_x(screen_width() * 0.25 - 5.0),
                    pane_scorebox_center_y,
                    pane_scorebox_zoom,
                    60,
                );
            }
        } else if is_versus {
            let incumbent = profile::get_session_player_side();
            if incumbent == profile::PlayerSide::P2 {
                push_scorebox(
                    profile::PlayerSide::P2,
                    state.p2_selected_steps_index,
                    scorebox_center_p1,
                    scorebox_center_y_above_pane,
                    scorebox_zoom,
                    0,
                );
                push_scorebox(
                    profile::PlayerSide::P1,
                    state.selected_steps_index,
                    scorebox_center_p2,
                    scorebox_center_y_above_pane,
                    scorebox_zoom,
                    0,
                );
            } else {
                push_scorebox(
                    profile::PlayerSide::P1,
                    state.selected_steps_index,
                    scorebox_center_p1,
                    scorebox_center_y_above_pane,
                    scorebox_zoom,
                    0,
                );
                push_scorebox(
                    profile::PlayerSide::P2,
                    state.p2_selected_steps_index,
                    scorebox_center_p2,
                    scorebox_center_y_above_pane,
                    scorebox_zoom,
                    0,
                );
            }
        } else if is_p2_single {
            push_scorebox(
                profile::PlayerSide::P2,
                state.p2_selected_steps_index,
                scorebox_center_p1,
                scorebox_center_y_above_pane,
                scorebox_zoom,
                0,
            );
        } else {
            push_scorebox(
                profile::PlayerSide::P1,
                state.selected_steps_index,
                scorebox_center_p1,
                scorebox_center_y_p1_single,
                scorebox_zoom,
                0,
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
        push_reload_overlay(&mut actors, reload, state.active_color_index);
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
    if let Some(pack_sync_overlay) =
        pack_sync::build_overlay(&state.pack_sync_overlay, state.active_color_index)
    {
        actors.extend(pack_sync_overlay);
        return actors;
    }
    if let Some(sync_overlay) = build_sync_overlay(&state.sync_overlay, state.active_color_index) {
        actors.extend(sync_overlay);
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
        let items = sort_menu_items(state, page);
        actors.extend(sort_menu::build_overlay(sort_menu::RenderParams {
            items: items.as_slice(),
            selected_index,
            prev_selected_index: state.sort_menu_prev_selected_index,
            focus_anim_elapsed: state.sort_menu_focus_anim_elapsed,
            selected_color: color::simply_love_rgba(state.active_color_index),
        }));
    }

    if let Some(leaderboard_overlay) = sort_menu::build_leaderboard_overlay(&state.leaderboard) {
        actors.extend(leaderboard_overlay);
    }
    if let Some(downloads_overlay) =
        sort_menu::build_downloads_overlay(&state.downloads_overlay, state.active_color_index)
    {
        actors.extend(downloads_overlay);
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

#[cfg(test)]
mod tests {
    use super::{
        PREVIEW_DELAY_SECONDS, WheelSortMode, build_displayed_entries, init_placeholder,
        reset_preview_after_gameplay, sync_low_confidence_warning,
    };
    use crate::config::SelectMusicWheelStyle;
    use crate::game::song::SongData;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn test_song(title: &str) -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: PathBuf::from(format!("{title}.ssc")),
            title: title.to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
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
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: Vec::new(),
        })
    }

    fn test_entries() -> Vec<super::MusicWheelEntry> {
        vec![
            super::MusicWheelEntry::PackHeader {
                name: "Pack A".to_string(),
                original_index: 0,
                banner_path: None,
            },
            super::MusicWheelEntry::Song(test_song("Song A1")),
            super::MusicWheelEntry::Song(test_song("Song A2")),
            super::MusicWheelEntry::PackHeader {
                name: "Pack B".to_string(),
                original_index: 1,
                banner_path: None,
            },
            super::MusicWheelEntry::Song(test_song("Song B1")),
        ]
    }

    #[test]
    fn reset_preview_after_gameplay_rearms_leaderboard_refresh() {
        let mut state = init_placeholder();
        state.last_refreshed_leaderboard_hash = Some("abc123".to_string());
        state.last_refreshed_leaderboard_hash_p2 = Some("def456".to_string());

        reset_preview_after_gameplay(&mut state);

        assert_eq!(state.last_refreshed_leaderboard_hash, None);
        assert_eq!(state.last_refreshed_leaderboard_hash_p2, None);
        assert_eq!(state.time_since_selection_change, PREVIEW_DELAY_SECONDS);
    }

    #[test]
    fn reset_preview_after_gameplay_preserves_non_group_sort_modes() {
        let mut state = init_placeholder();
        state.sort_mode = WheelSortMode::Group;

        reset_preview_after_gameplay(&mut state);

        assert_eq!(state.sort_mode, WheelSortMode::Group);
    }

    #[test]
    fn sync_low_confidence_warning_mentions_confidence_and_threshold() {
        let warning = sync_low_confidence_warning(Some(0.73), 0.80).unwrap();
        assert!(warning.contains("73%"));
        assert!(warning.contains("80%"));
    }

    #[test]
    fn itg_wheel_style_keeps_other_pack_headers_visible() {
        let entries =
            build_displayed_entries(&test_entries(), Some("Pack A"), SelectMusicWheelStyle::Itg);

        assert_eq!(entries.len(), 4);
        assert!(matches!(
            entries[0],
            super::MusicWheelEntry::PackHeader { ref name, .. } if name == "Pack A"
        ));
        assert!(matches!(
            entries[3],
            super::MusicWheelEntry::PackHeader { ref name, .. } if name == "Pack B"
        ));
    }

    #[test]
    fn iidx_wheel_style_only_shows_active_pack_and_header() {
        let entries =
            build_displayed_entries(&test_entries(), Some("Pack A"), SelectMusicWheelStyle::Iidx);

        assert_eq!(entries.len(), 3);
        assert!(matches!(
            entries[0],
            super::MusicWheelEntry::PackHeader { ref name, .. } if name == "Pack A"
        ));
        assert!(entries.iter().all(|entry| {
            !matches!(
                entry,
                super::MusicWheelEntry::PackHeader { name, .. } if name == "Pack B"
            )
        }));
    }
}
