use crate::act;
use crate::assets;
use crate::engine::gfx::{BlendMode, TexturedMeshVertex};
use crate::engine::present::actors::{Actor, SizeSpec};
use crate::engine::present::cache::{TextCache, cached_text};
use crate::engine::present::color;
use crate::engine::present::compose::TextLayoutCache;
use crate::engine::present::font;
use crate::engine::space::*;
use crate::game::gameplay::{
    AccelEffects, AppearanceEffects, COMBO_HUNDRED_MILESTONE_DURATION,
    COMBO_THOUSAND_MILESTONE_DURATION, ComboMilestoneKind, HOLD_JUDGMENT_TOTAL_DURATION, MAX_COLS,
    NoteCountStat, RECEPTOR_Y_OFFSET_FROM_CENTER, RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE,
    TRANSITION_IN_DURATION, VisualEffects,
};
use crate::game::gameplay::{
    active_chart_attack_effects_for_player, active_hold_is_engaged,
    effective_accel_effects_for_player, effective_appearance_effects_for_player,
    effective_mini_percent_for_player, effective_perspective_effects_for_player,
    effective_scroll_effects_for_player, effective_scroll_speed_for_player,
    effective_spacing_multiplier_for_player, effective_visibility_effects_for_player,
    effective_visual_effects_for_player, receptor_glow_visual_for_col, row_hides_completed_note,
    scroll_receptor_y,
};
use crate::game::judgment::{HOLD_SCORE_HELD, JudgeGrade, Judgment, TimingWindow};
use crate::game::note::{HoldResult, MineResult, Note, NoteType};
use crate::game::parsing::noteskin::{
    ModelDrawState, ModelMeshCache, NUM_QUANTIZATIONS, NoteAnimPart, SpriteSlot,
};
use crate::game::parsing::song_lua::SongLuaNoteHideWindow;
use crate::game::{
    gameplay::{ActiveHold, PlayerRuntime, SongTimeNs, State},
    profile, scores,
    scroll::ScrollSpeedSetting,
    timing::{TimeSignatureSegment, beat_to_note_row, default_time_signature, note_row_to_beat},
};
use crate::screens::components::shared::noteskin_model::noteskin_model_actor_from_draw_cached;
use glam::{Mat4 as Matrix4, Vec3 as Vector3};
use rssp::streams::StreamSegment;
use std::array::from_fn;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;
use twox_hash::XxHash64;

// --- CONSTANTS ---

// Gameplay Layout & Feel
const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0; // Dance lane width for hold bodies and square fallback visuals
const HOLD_JUDGMENT_Y_OFFSET_FROM_CENTER: f32 = -90.0; // Mirrors Simply Love metrics for hold judgments
const HOLD_JUDGMENT_OFFSET_FROM_RECEPTOR: f32 =
    HOLD_JUDGMENT_Y_OFFSET_FROM_CENTER - RECEPTOR_Y_OFFSET_FROM_CENTER;
const TAP_JUDGMENT_OFFSET_FROM_CENTER: f32 = 30.0; // From _fallback JudgmentTransformCommand
const COMBO_OFFSET_FROM_CENTER: f32 = 30.0; // From _fallback ComboTransformCommand (non-centered)
const COLUMN_CUE_Y_OFFSET: f32 = 80.0;
const COLUMN_CUE_TEXT_NORMAL_Y: f32 = 80.0;
const COLUMN_CUE_TEXT_REVERSE_Y: f32 = 260.0;
const COLUMN_CUE_FADE_TIME: f32 = 0.15;
const COLUMN_CUE_BASE_ALPHA: f32 = 0.12;
const LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT: f32 = 140.0; // Each frame in Love 1x2 (doubleres).png is 140px tall
const HOLD_JUDGMENT_FINAL_HEIGHT: f32 = 32.0; // Matches Simply Love's final on-screen size
const HOLD_JUDGMENT_INITIAL_HEIGHT: f32 = HOLD_JUDGMENT_FINAL_HEIGHT * 0.8; // Mirrors 0.4->0.5 zoom ramp in metrics
const HOLD_JUDGMENT_FINAL_ZOOM: f32 =
    HOLD_JUDGMENT_FINAL_HEIGHT / LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT;
const HOLD_JUDGMENT_INITIAL_ZOOM: f32 =
    HOLD_JUDGMENT_INITIAL_HEIGHT / LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT;
const ERROR_BAR_JUDGMENT_HEIGHT: f32 = 40.0; // SL: judgmentHeight in SL-Layout.lua
const ERROR_BAR_OFFSET_FROM_JUDGMENT: f32 = ERROR_BAR_JUDGMENT_HEIGHT * 0.5 + 5.0; // SL: top/bottom +/-25px

const ERROR_BAR_WIDTH_COLORFUL: f32 = 160.0;
const ERROR_BAR_HEIGHT_COLORFUL: f32 = 10.0;
const ERROR_BAR_WIDTH_AVERAGE: f32 = 325.0;
const ERROR_BAR_HEIGHT_AVERAGE: f32 = 7.0;
const ERROR_BAR_WIDTH_MONOCHROME: f32 = 240.0;
const ERROR_BAR_TICK_WIDTH: f32 = 2.0;
const ERROR_BAR_TICK_DUR_COLORFUL: f32 = 0.5;
const ERROR_BAR_TICK_DUR_MONOCHROME: f32 = 0.75;
const ERROR_BAR_AVERAGE_TICK_EXTRA_H: f32 = 75.0;
const ERROR_BAR_SEG_ALPHA_BASE: f32 = 0.3;
const ERROR_BAR_MONO_BG_ALPHA: f32 = 0.5;
const ERROR_BAR_LINE_ALPHA: f32 = 0.3;
const ERROR_BAR_LINES_FADE_START_S: f32 = 2.5;
const ERROR_BAR_LINES_FADE_DUR_S: f32 = 0.5;
const ERROR_BAR_LABEL_FADE_DUR_S: f32 = 0.5;
const ERROR_BAR_LABEL_HOLD_S: f32 = 2.0;
const OFFSET_INDICATOR_DUR_S: f32 = 0.5;
const DISPLAY_MODS_ZOOM: f32 = 0.8;
const DISPLAY_MODS_WRAP_WIDTH_PX: f32 = 125.0;
const DISPLAY_MODS_LINE_STEP: f32 = 15.0;
const DISPLAY_MODS_WARNING_W: f32 = 90.0;
const DISPLAY_MODS_WARNING_H: f32 = 30.0;
const DISPLAY_MODS_WARNING_ZOOM: f32 = 1.5;

const ERROR_BAR_COLORFUL_TICK_RGBA: [f32; 4] = color::rgba_hex("#b20000");
const ERROR_BAR_TEXT_EARLY_RGBA: [f32; 4] = color::rgba_hex("#066af4");
const ERROR_BAR_TEXT_LATE_RGBA: [f32; 4] = color::rgba_hex("#ff5a4e");
const TEXT_CACHE_LIMIT: usize = 8192;
const COMBO_PREWARM_CAP: u32 = 2048;
const MEASURE_PREWARM_CAP: i32 = 64;
const RUN_TIMER_PREWARM_CAP_S: i32 = 600;
const MAX_NOTES_AFTER: usize = 64;

// Visual Feedback
const SHOW_COMBO_AT: u32 = 4; // From Simply Love metrics

#[inline(always)]
fn judgment_tilt_rotation_deg(profile: &profile::Profile, judgment: &Judgment) -> f32 {
    if !profile.judgment_tilt || judgment.grade == JudgeGrade::Miss {
        return 0.0;
    }
    let offset_ms = judgment.time_error_ms;
    if !offset_ms.is_finite() || !profile.tilt_multiplier.is_finite() {
        return 0.0;
    }
    let min_ms = profile.tilt_min_threshold_ms as f32;
    let max_ms = profile
        .tilt_max_threshold_ms
        .max(profile.tilt_min_threshold_ms) as f32;
    let active_ms = offset_ms.abs().min(max_ms) - min_ms;
    if active_ms <= 0.0 {
        return 0.0;
    }
    let dir = if offset_ms < 0.0 { 1.0 } else { -1.0 };
    dir * active_ms * 0.3 * profile.tilt_multiplier
}

// Z-order layers for key gameplay visuals (higher draws on top)
const Z_RECEPTOR: i32 = 100;
const Z_HOLD_BODY: i32 = 110;
const Z_HOLD_CAP: i32 = 110;
// ITG draws GhostArrowRow after columns; keep hold/roll ghost arrows above note lanes.
const Z_HOLD_EXPLOSION: i32 = 145;
// ITG's Explosion actor declares hold/roll children before tap judgments, so taps render on top.
const Z_TAP_EXPLOSION: i32 = 150;
// ITG NoteField draws ReceptorArrowRow before column renderers, so receptor
// press glow must stay under hold bodies instead of cutting through them.
const Z_HOLD_GLOW: i32 = 105;
const Z_MINE_EXPLOSION: i32 = 101;
const Z_TAP_NOTE: i32 = 140;
const Z_COLUMN_CUE: i32 = 90;
const MINE_CORE_SIZE_RATIO: f32 = 0.45;
const Z_MEASURE_LINES: i32 = 80;
const Z_JUDGMENT_FRONT: i16 = 200;
const Z_JUDGMENT_BACK: i16 = 95;
const Z_ERROR_BAR_BG_FRONT: i16 = 180;
const Z_ERROR_BAR_BG_BACK: i16 = 86;
const Z_ERROR_BAR_BAND_FRONT: i16 = 181;
const SPLIT_15_10MS_OVERLAY_ALPHA: f32 = 0.5;
const Z_ERROR_BAR_BAND_BACK: i16 = 87;
const Z_ERROR_BAR_LINE_FRONT: i16 = 182;
const Z_ERROR_BAR_LINE_BACK: i16 = 88;
const Z_ERROR_BAR_TICK_FRONT: i16 = 183;
const Z_ERROR_BAR_TICK_BACK: i16 = 89;
const Z_ERROR_BAR_TEXT_FRONT: i16 = 184;
const Z_ERROR_BAR_TEXT_BACK: i16 = 90;

const BLINK_MOD_FREQUENCY: f32 = 0.3333;
const BOOST_MOD_MIN_CLAMP: f32 = -400.0;
const BOOST_MOD_MAX_CLAMP: f32 = 400.0;
const BRAKE_MOD_MIN_CLAMP: f32 = -400.0;
const BRAKE_MOD_MAX_CLAMP: f32 = 400.0;
const WAVE_MOD_MAGNITUDE: f32 = 20.0;
const WAVE_MOD_HEIGHT: f32 = 38.0;
const EXPAND_MULTIPLIER_FREQUENCY: f32 = 3.0;
const EXPAND_MULTIPLIER_SCALE_FROM_LOW: f32 = -1.0;
const EXPAND_MULTIPLIER_SCALE_FROM_HIGH: f32 = 1.0;
const EXPAND_MULTIPLIER_SCALE_TO_LOW: f32 = 0.75;
const EXPAND_MULTIPLIER_SCALE_TO_HIGH: f32 = 1.75;
const EXPAND_SPEED_SCALE_FROM_LOW: f32 = 0.0;
const EXPAND_SPEED_SCALE_FROM_HIGH: f32 = 1.0;
const EXPAND_SPEED_SCALE_TO_LOW: f32 = 1.0;
const TIPSY_TIMER_FREQUENCY: f32 = 1.2;
const TIPSY_COLUMN_FREQUENCY: f32 = 1.8;
const TIPSY_ARROW_MAGNITUDE: f32 = 0.4;
const DRUNK_COLUMN_FREQUENCY: f32 = 0.2;
const DRUNK_OFFSET_FREQUENCY: f32 = 10.0;
const DRUNK_ARROW_MAGNITUDE: f32 = 0.5;
const BUMPY_Z_MAGNITUDE: f32 = 40.0;
const BUMPY_Z_ANGLE_DIVISOR: f32 = 16.0;
const TORNADO_X_OFFSET_FREQUENCY: f32 = 6.0;
const BEAT_OFFSET_HEIGHT: f32 = 15.0;
const BEAT_PI_HEIGHT: f32 = 2.0;
const CENTER_LINE_Y: f32 = 160.0;
const FADE_DIST_Y: f32 = 40.0;

#[derive(Clone, Copy)]
struct EditBeatBarInfo {
    frame: u32,
    measure_index: Option<i64>,
}

fn append_edit_measure_number(
    actors: &mut Vec<Actor>,
    edit_beat_bars: bool,
    measure_index: Option<i64>,
    x: f32,
    y: f32,
    field_zoom: f32,
) {
    let Some(measure) = measure_index else {
        return;
    };
    if !edit_beat_bars || measure < 0 {
        return;
    }
    actors.push(act!(text:
        font("miso"):
        settext(measure.to_string()):
        align(1.0, 0.5):
        horizalign(right):
        xy(x, y):
        zoom((field_zoom * 0.9).clamp(0.35, 0.75)):
        shadowlength(2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(Z_MEASURE_LINES + 1)
    ));
}

fn append_beat_bar(
    actors: &mut Vec<Actor>,
    edit_beat_bars: bool,
    edit_bar_frame: u32,
    x_center: f32,
    y: f32,
    width: f32,
    field_zoom: f32,
    thickness: f32,
    alpha: f32,
) {
    if edit_beat_bars {
        append_edit_beat_bar(
            actors,
            edit_bar_frame,
            x_center,
            y,
            width,
            field_zoom,
            thickness,
            alpha,
        );
    } else {
        actors.push(act!(quad:
            align(0.5, 0.5): xy(x_center, y):
            zoomto(width, thickness):
            diffuse(1.0, 1.0, 1.0, alpha):
            z(Z_MEASURE_LINES)
        ));
    }
}

fn append_edit_beat_bar(
    actors: &mut Vec<Actor>,
    frame: u32,
    x_center: f32,
    y: f32,
    width: f32,
    field_zoom: f32,
    thickness: f32,
    alpha: f32,
) {
    match frame {
        0 | 1 => append_edit_bar_segment(actors, x_center, y, width, thickness, alpha),
        2 => append_dashed_edit_bar(
            actors,
            x_center,
            y,
            width,
            thickness,
            12.0 * field_zoom,
            8.0 * field_zoom,
            alpha,
        ),
        _ => append_dashed_edit_bar(
            actors,
            x_center,
            y,
            width,
            thickness,
            4.0 * field_zoom,
            6.0 * field_zoom,
            alpha,
        ),
    }
}

fn append_edit_bar_segment(
    actors: &mut Vec<Actor>,
    x_center: f32,
    y: f32,
    width: f32,
    thickness: f32,
    alpha: f32,
) {
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(x_center, y):
        zoomto(width, thickness):
        diffuse(1.0, 1.0, 1.0, alpha):
        z(Z_MEASURE_LINES)
    ));
}

fn append_dashed_edit_bar(
    actors: &mut Vec<Actor>,
    x_center: f32,
    y: f32,
    width: f32,
    thickness: f32,
    dash: f32,
    gap: f32,
    alpha: f32,
) {
    let dash = dash.max(1.0);
    let step = (dash + gap).max(dash + 1.0);
    let left = x_center - width * 0.5;
    let right = x_center + width * 0.5;
    let mut x = left;
    while x < right {
        let seg_w = dash.min(right - x);
        actors.push(act!(quad:
            align(0.0, 0.5):
            xy(x, y):
            zoomto(seg_w, thickness):
            diffuse(1.0, 1.0, 1.0, alpha):
            z(Z_MEASURE_LINES)
        ));
        x += step;
    }
}

fn valid_edit_time_signature(sig: TimeSignatureSegment) -> TimeSignatureSegment {
    if sig.numerator > 0 && sig.denominator > 0 {
        sig
    } else {
        default_time_signature()
    }
}

fn edit_time_signature_at(segments: &[TimeSignatureSegment], index: usize) -> TimeSignatureSegment {
    if segments.is_empty() {
        default_time_signature()
    } else {
        valid_edit_time_signature(segments[index])
    }
}

fn edit_time_signature_count(segments: &[TimeSignatureSegment]) -> usize {
    segments.len().max(1)
}

fn edit_bar_step_rows(sig: TimeSignatureSegment) -> i32 {
    (beat_to_note_row(sig.denominator as f32 / 4.0) / 4).max(1)
}

fn edit_measure_frequency(sig: TimeSignatureSegment) -> i32 {
    sig.numerator.saturating_mul(4).max(1)
}

fn edit_measure_bars_in_segment(start_row: i32, end_row: i32, sig: TimeSignatureSegment) -> i64 {
    if end_row <= start_row {
        return 0;
    }
    let step = i64::from(edit_bar_step_rows(sig));
    let freq = i64::from(edit_measure_frequency(sig));
    let bars = (i64::from(end_row) - i64::from(start_row) - 1) / step + 1;
    (bars - 1) / freq + 1
}

fn edit_measure_index_before_segment(
    segments: &[TimeSignatureSegment],
    segment_index: usize,
) -> i64 {
    let mut measure_index = 0;
    for i in 0..segment_index {
        let sig = edit_time_signature_at(segments, i);
        let next_sig = edit_time_signature_at(segments, i + 1);
        measure_index += edit_measure_bars_in_segment(
            beat_to_note_row(sig.beat),
            beat_to_note_row(next_sig.beat),
            sig,
        );
    }
    measure_index
}

fn edit_time_signature_index_at_row(segments: &[TimeSignatureSegment], row: i32) -> usize {
    if segments.is_empty() {
        return 0;
    }

    let mut index = 0;
    for (i, sig) in segments.iter().enumerate() {
        if beat_to_note_row(sig.beat) <= row {
            index = i;
        } else {
            break;
        }
    }
    index
}

fn edit_beat_bar_info_for_row(
    row: i32,
    segments: &[TimeSignatureSegment],
) -> Option<EditBeatBarInfo> {
    if row < 0 {
        return None;
    }

    let segment_index = edit_time_signature_index_at_row(segments, row);
    let sig = edit_time_signature_at(segments, segment_index);
    let segment_start_row = beat_to_note_row(sig.beat);
    if row < segment_start_row {
        return None;
    }

    let step_rows = edit_bar_step_rows(sig);
    let local_rows = row - segment_start_row;
    if local_rows % step_rows != 0 {
        return None;
    }

    let bars_drawn = local_rows / step_rows;
    let measure_frequency = edit_measure_frequency(sig);
    let is_measure = bars_drawn % measure_frequency == 0;
    let frame = if is_measure {
        0
    } else if bars_drawn % 4 == 0 {
        1
    } else if bars_drawn % 2 == 0 {
        2
    } else {
        3
    };
    let measure_index = is_measure.then(|| {
        edit_measure_index_before_segment(segments, segment_index)
            + i64::from(bars_drawn / measure_frequency)
    });

    Some(EditBeatBarInfo {
        frame,
        measure_index,
    })
}

fn edit_bar_gcd(a: i32, b: i32) -> i32 {
    let mut a = i64::from(a).abs();
    let mut b = i64::from(b).abs();
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a.clamp(1, i64::from(i32::MAX)) as i32
}

fn edit_bar_candidate_step_rows(segments: &[TimeSignatureSegment]) -> i32 {
    let mut step = edit_bar_step_rows(edit_time_signature_at(segments, 0));
    for i in 0..edit_time_signature_count(segments) {
        let sig = edit_time_signature_at(segments, i);
        step = edit_bar_gcd(step, edit_bar_step_rows(sig));
        step = edit_bar_gcd(step, beat_to_note_row(sig.beat));
    }
    step.max(1)
}

fn edit_bar_scroll_speed(
    scroll_speed: ScrollSpeedSetting,
    reference_bpm: f32,
    music_rate: f32,
) -> f32 {
    match scroll_speed {
        ScrollSpeedSetting::XMod(multiplier) => multiplier,
        ScrollSpeedSetting::MMod(_) => scroll_speed.beat_multiplier(reference_bpm, music_rate),
        ScrollSpeedSetting::CMod(_) => 4.0,
    }
    .max(0.0)
}

fn scaled_edit_bar_alpha(scroll_speed: f32, visible_at: f32, full_at: f32) -> f32 {
    ((scroll_speed - visible_at) / (full_at - visible_at)).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, Default)]
struct TornadoBounds {
    min_x: f32,
    max_x: f32,
}
type FastTextCache<K> = TextCache<K, BuildHasherDefault<XxHash64>>;

thread_local! {
    static FMT2_CACHE_F32: RefCell<FastTextCache<i32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        512,
        BuildHasherDefault::default(),
    ));
    static PERCENT2_CACHE_F64: RefCell<FastTextCache<u32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        512,
        BuildHasherDefault::default(),
    ));
    static SIGNED_PERCENT2_CACHE_F64: RefCell<FastTextCache<(u32, bool)>> = RefCell::new(
        HashMap::with_capacity_and_hasher(512, BuildHasherDefault::default()),
    );
    static NEG_INT_CACHE_U32: RefCell<FastTextCache<u32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        256,
        BuildHasherDefault::default(),
    ));
    static PAREN_INT_CACHE_I32: RefCell<FastTextCache<i32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        512,
        BuildHasherDefault::default(),
    ));
    static INT_CACHE_I32: RefCell<FastTextCache<i32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        512,
        BuildHasherDefault::default(),
    ));
    static INT_CACHE_U32: RefCell<FastTextCache<u32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        512,
        BuildHasherDefault::default(),
    ));
    static RATIO_CACHE_I32: RefCell<FastTextCache<(i32, i32)>> = RefCell::new(
        HashMap::with_capacity_and_hasher(1024, BuildHasherDefault::default()),
    );
    static OFFSET_MS_CACHE_F32: RefCell<FastTextCache<i32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        512,
        BuildHasherDefault::default(),
    ));
    static RUN_TIMER_CACHE: RefCell<FastTextCache<(i32, i32, bool)>> = RefCell::new(
        HashMap::with_capacity_and_hasher(1024, BuildHasherDefault::default()),
    );
    static GAMEPLAY_MODS_CACHE: RefCell<FastTextCache<GameplayModsTextKey>> = RefCell::new(
        HashMap::with_capacity_and_hasher(256, BuildHasherDefault::default()),
    );
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GameplayModsTextKey {
    speed_tag: u8,
    speed_bits: u32,
    noteskin_hash: u64,
    insert_mask: u8,
    remove_mask: u8,
    holds_mask: u8,
    turn_bits: u16,
    attack_mode: u8,
    mini_percent: i16,
    spacing_percent: i16,
    visual_delay_ms: i16,
    accel: [i16; 5],
    visual: [i16; 9],
    appearance: [i16; 5],
    scroll: [i16; 5],
    perspective_tilt: i16,
    perspective_skew: i16,
    dark: i16,
    blind: i16,
    cover: i16,
}

#[inline(always)]
fn quantize_centi_i32(value: f64) -> i32 {
    (if value.is_finite() { value } else { 0.0 } * 100.0)
        .round()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

#[inline(always)]
fn quantize_centi_u32(value: f64) -> u32 {
    let value = if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    };
    ((value * 100.0).round()).clamp(0.0, u32::MAX as f64) as u32
}

#[inline(always)]
fn cached_fmt2_f32(value: f32) -> Arc<str> {
    let key = quantize_centi_i32(f64::from(value));
    cached_text(&FMT2_CACHE_F32, key, TEXT_CACHE_LIMIT, || {
        format!("{:.2}", key as f64 / 100.0)
    })
}

#[inline(always)]
fn cached_percent2_f64(value: f64) -> Arc<str> {
    let key = quantize_centi_u32(value);
    cached_text(&PERCENT2_CACHE_F64, key, TEXT_CACHE_LIMIT, || {
        format!("{:.2}%", key as f64 / 100.0)
    })
}

#[inline(always)]
fn cached_signed_percent2_f64(value: f64, neg: bool) -> Arc<str> {
    let key = quantize_centi_u32(value);
    cached_text(
        &SIGNED_PERCENT2_CACHE_F64,
        (key, neg),
        TEXT_CACHE_LIMIT,
        || {
            if neg {
                format!("-{:.2}%", key as f64 / 100.0)
            } else {
                format!("+{:.2}%", key as f64 / 100.0)
            }
        },
    )
}

#[inline(always)]
fn cached_neg_int_u32(value: u32) -> Arc<str> {
    cached_text(&NEG_INT_CACHE_U32, value, TEXT_CACHE_LIMIT, || {
        format!("-{value}")
    })
}

#[inline(always)]
fn cached_paren_i32(value: i32) -> Arc<str> {
    cached_text(&PAREN_INT_CACHE_I32, value, TEXT_CACHE_LIMIT, || {
        format!("({value})")
    })
}

#[inline(always)]
fn cached_int_i32(value: i32) -> Arc<str> {
    cached_text(&INT_CACHE_I32, value, TEXT_CACHE_LIMIT, || {
        value.to_string()
    })
}

#[inline(always)]
fn cached_int_u32(value: u32) -> Arc<str> {
    cached_text(&INT_CACHE_U32, value, TEXT_CACHE_LIMIT, || {
        value.to_string()
    })
}

#[inline(always)]
fn cached_ratio_i32(curr: i32, total: i32) -> Arc<str> {
    cached_text(&RATIO_CACHE_I32, (curr, total), TEXT_CACHE_LIMIT, || {
        format!("{curr}/{total}")
    })
}

#[inline(always)]
fn cached_offset_ms(value: f32) -> Arc<str> {
    let key = quantize_centi_i32(f64::from(value));
    cached_text(&OFFSET_MS_CACHE_F32, key, TEXT_CACHE_LIMIT, || {
        format!("{:.2}ms", key as f64 / 100.0)
    })
}

fn cached_run_timer(seconds: i32, minute_threshold: i32, trailing_space: bool) -> Arc<str> {
    let seconds = seconds.max(0);
    cached_text(
        &RUN_TIMER_CACHE,
        (seconds, minute_threshold, trailing_space),
        TEXT_CACHE_LIMIT,
        || {
            let mut s = if seconds < 10 {
                format!("0.0{seconds}")
            } else if seconds > minute_threshold {
                let minutes = seconds / 60;
                let secs = seconds % 60;
                format!("{minutes}.{secs:02}")
            } else {
                format!("0.{seconds}")
            };
            if trailing_space {
                s.push(' ');
            }
            s
        },
    )
}

#[inline(always)]
fn mod_percent_key(level: f32) -> i16 {
    let value = if level.is_finite() { level } else { 0.0 };
    (value * 100.0)
        .round()
        .clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

#[inline(always)]
fn clamp_rounded_i16(value: f32) -> i16 {
    let value = if value.is_finite() { value } else { 0.0 };
    value.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

#[inline(always)]
fn append_mod_part(parts: &mut Vec<String>, percent: i16, name: &str) {
    if percent == 0 {
        return;
    }
    if percent == 100 {
        parts.push(name.to_string());
    } else {
        parts.push(format!("{percent}% {name}"));
    }
}

#[inline(always)]
fn append_mini_part(parts: &mut Vec<String>, mini_percent: i16) {
    if mini_percent != 0 {
        parts.push(format!("{mini_percent}% Mini"));
    }
}

#[inline(always)]
fn append_spacing_part(parts: &mut Vec<String>, spacing_percent: i16) {
    if spacing_percent != 0 {
        parts.push(format!("{spacing_percent}% Spacing"));
    }
}

#[inline(always)]
fn append_perspective_parts(parts: &mut Vec<String>, tilt: i16, skew: i16) {
    if tilt == 0 && skew == 0 {
        parts.push("Overhead".to_string());
        return;
    }
    if skew == 0 {
        if tilt > 0 {
            append_mod_part(parts, tilt, "Distant");
        } else {
            append_mod_part(parts, -tilt, "Hallway");
        }
        return;
    }
    if skew == tilt {
        append_mod_part(parts, skew, "Space");
        return;
    }
    if skew == -tilt {
        append_mod_part(parts, skew, "Incoming");
        return;
    }
    append_mod_part(parts, skew, "Skew");
    append_mod_part(parts, tilt, "Tilt");
}

#[inline(always)]
fn turn_option_name(turn: profile::TurnOption) -> Option<&'static str> {
    match turn {
        profile::TurnOption::None => None,
        profile::TurnOption::Mirror => Some("Mirror"),
        profile::TurnOption::Left => Some("Left"),
        profile::TurnOption::Right => Some("Right"),
        profile::TurnOption::LRMirror => Some("LR-Mirror"),
        profile::TurnOption::UDMirror => Some("UD-Mirror"),
        profile::TurnOption::Shuffle => Some("Shuffle"),
        profile::TurnOption::Blender => Some("Blender"),
        profile::TurnOption::Random => Some("Random"),
    }
}

#[inline(always)]
const fn turn_option_bits(turn: profile::TurnOption) -> u16 {
    match turn {
        profile::TurnOption::None => 0,
        profile::TurnOption::Mirror => 1 << 0,
        profile::TurnOption::Left => 1 << 1,
        profile::TurnOption::Right => 1 << 2,
        profile::TurnOption::LRMirror => 1 << 3,
        profile::TurnOption::UDMirror => 1 << 4,
        profile::TurnOption::Shuffle => 1 << 5,
        profile::TurnOption::Blender => 1 << 6,
        profile::TurnOption::Random => 1 << 7,
    }
}

#[inline(always)]
fn append_turn_parts(parts: &mut Vec<String>, bits: u16) {
    for turn in [
        profile::TurnOption::Mirror,
        profile::TurnOption::Left,
        profile::TurnOption::Right,
        profile::TurnOption::LRMirror,
        profile::TurnOption::UDMirror,
        profile::TurnOption::Shuffle,
        profile::TurnOption::Blender,
        profile::TurnOption::Random,
    ] {
        if (bits & turn_option_bits(turn)) != 0
            && let Some(name) = turn_option_name(turn)
        {
            parts.push(name.to_string());
        }
    }
}

#[inline(always)]
fn attack_mode_name(mode: profile::AttackMode) -> Option<&'static str> {
    match mode {
        profile::AttackMode::Off => Some("NoAttacks"),
        profile::AttackMode::On => None,
        profile::AttackMode::Random => Some("RandomAttacks"),
    }
}

#[inline(always)]
fn push_transform_parts(parts: &mut Vec<String>, insert_mask: u8, remove_mask: u8, holds_mask: u8) {
    if (remove_mask & (1 << 2)) != 0 {
        parts.push("NoHolds".to_string());
    }
    if (holds_mask & (1 << 3)) != 0 {
        parts.push("NoRolls".to_string());
    }
    if (remove_mask & (1 << 1)) != 0 {
        parts.push("NoMines".to_string());
    }
    if (remove_mask & (1 << 0)) != 0 {
        parts.push("Little".to_string());
    }
    if (insert_mask & (1 << 0)) != 0 {
        parts.push("Wide".to_string());
    }
    if (insert_mask & (1 << 1)) != 0 {
        parts.push("Big".to_string());
    }
    if (insert_mask & (1 << 2)) != 0 {
        parts.push("Quick".to_string());
    }
    if (insert_mask & (1 << 3)) != 0 {
        parts.push("BMRize".to_string());
    }
    if (insert_mask & (1 << 4)) != 0 {
        parts.push("Skippy".to_string());
    }
    if (insert_mask & (1 << 7)) != 0 {
        parts.push("Mines".to_string());
    }
    if (insert_mask & (1 << 5)) != 0 {
        parts.push("Echo".to_string());
    }
    if (insert_mask & (1 << 6)) != 0 {
        parts.push("Stomp".to_string());
    }
    if (holds_mask & (1 << 0)) != 0 {
        parts.push("Planted".to_string());
    }
    if (holds_mask & (1 << 1)) != 0 {
        parts.push("Floored".to_string());
    }
    if (holds_mask & (1 << 2)) != 0 {
        parts.push("Twister".to_string());
    }
    if (holds_mask & (1 << 4)) != 0 {
        parts.push("HoldsToRolls".to_string());
    }
    if (remove_mask & (1 << 3)) != 0 {
        parts.push("NoJumps".to_string());
    }
    if (remove_mask & (1 << 4)) != 0 {
        parts.push("NoHands".to_string());
    }
    if (remove_mask & (1 << 6)) != 0 {
        parts.push("NoLifts".to_string());
    }
    if (remove_mask & (1 << 7)) != 0 {
        parts.push("NoFakes".to_string());
    }
    if (remove_mask & (1 << 5)) != 0 {
        parts.push("NoQuads".to_string());
    }
}

#[inline(always)]
fn gameplay_mods_text_key(state: &State, player_idx: usize) -> GameplayModsTextKey {
    let profile = &state.player_profiles[player_idx];
    let chart_attack = active_chart_attack_effects_for_player(state, player_idx);
    let scroll_speed = effective_scroll_speed_for_player(state, player_idx);
    let accel = effective_accel_effects_for_player(state, player_idx);
    let visual = effective_visual_effects_for_player(state, player_idx);
    let appearance = effective_appearance_effects_for_player(state, player_idx);
    let visibility = effective_visibility_effects_for_player(state, player_idx);
    let scroll = effective_scroll_effects_for_player(state, player_idx);
    let perspective = effective_perspective_effects_for_player(state, player_idx);
    let display_mini = (effective_mini_percent_for_player(state, player_idx)
        - if visual.big > f32::EPSILON {
            100.0 * visual.big
        } else {
            0.0
        })
    .clamp(-100.0, 150.0);
    let dark = if profile.hide_targets {
        1.0
    } else {
        visibility.dark
    };
    let cover = if profile.hide_song_bg {
        1.0
    } else {
        visibility.cover
    };
    let (speed_tag, speed_bits) = match scroll_speed {
        ScrollSpeedSetting::CMod(value) => (0, value.to_bits()),
        ScrollSpeedSetting::XMod(value) => (1, value.to_bits()),
        ScrollSpeedSetting::MMod(value) => (2, value.to_bits()),
    };
    let mut noteskin_hasher = XxHash64::default();
    noteskin_hasher.write(profile.noteskin.as_str().as_bytes());
    GameplayModsTextKey {
        speed_tag,
        speed_bits,
        noteskin_hash: noteskin_hasher.finish(),
        insert_mask: profile.insert_active_mask.bits() | chart_attack.insert_mask,
        remove_mask: profile.remove_active_mask.bits() | chart_attack.remove_mask,
        holds_mask: profile.holds_active_mask.bits() | chart_attack.holds_mask,
        turn_bits: turn_option_bits(profile.turn_option) | chart_attack.turn_bits,
        attack_mode: profile.attack_mode as u8,
        mini_percent: clamp_rounded_i16(display_mini),
        spacing_percent: profile
            .spacing_percent
            .clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        visual_delay_ms: profile
            .visual_delay_ms
            .clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        accel: [
            mod_percent_key(accel.boost),
            mod_percent_key(accel.brake),
            mod_percent_key(accel.wave),
            mod_percent_key(accel.expand),
            mod_percent_key(accel.boomerang),
        ],
        visual: [
            mod_percent_key(visual.drunk),
            mod_percent_key(visual.dizzy),
            mod_percent_key(visual.confusion),
            mod_percent_key(visual.flip),
            mod_percent_key(visual.invert),
            mod_percent_key(visual.tornado),
            mod_percent_key(visual.tipsy),
            mod_percent_key(visual.bumpy),
            mod_percent_key(visual.beat),
        ],
        appearance: [
            mod_percent_key(appearance.hidden),
            mod_percent_key(appearance.sudden),
            mod_percent_key(appearance.stealth),
            mod_percent_key(appearance.blink),
            mod_percent_key(appearance.random_vanish),
        ],
        scroll: [
            mod_percent_key(scroll.reverse),
            mod_percent_key(scroll.split),
            mod_percent_key(scroll.alternate),
            mod_percent_key(scroll.cross),
            mod_percent_key(scroll.centered),
        ],
        perspective_tilt: mod_percent_key(perspective.tilt),
        perspective_skew: mod_percent_key(perspective.skew),
        dark: mod_percent_key(dark),
        blind: mod_percent_key(visibility.blind),
        cover: mod_percent_key(cover),
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FieldPlacement {
    P1,
    P2,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ViewOverride {
    pub field_zoom: Option<f32>,
    pub scroll_speed: Option<ScrollSpeedSetting>,
    pub force_center_1player: bool,
    pub center_receptors_y: bool,
    pub receptor_y: Option<f32>,
    pub edit_beat_bars: bool,
    pub hide_display_mods: bool,
}

pub struct BuiltNotefield {
    pub layout_center_x: f32,
    pub field_actors: Vec<Arc<[Actor]>>,
    pub judgment_actors: Option<Vec<Arc<[Actor]>>>,
    pub combo_actors: Option<Vec<Arc<[Actor]>>>,
}

#[derive(Clone, Copy, Default)]
pub struct ProxyCaptureRequests {
    pub note_field: bool,
    pub judgment: bool,
    pub combo: bool,
}

impl BuiltNotefield {
    fn empty(layout_center_x: f32) -> Self {
        Self {
            layout_center_x,
            field_actors: Vec::new(),
            judgment_actors: None,
            combo_actors: None,
        }
    }
}

fn share_hud_range(hud_actors: &mut Vec<Actor>, start: usize) -> Option<Vec<Arc<[Actor]>>> {
    if start >= hud_actors.len() {
        return None;
    }
    let children = Arc::<[Actor]>::from(hud_actors.drain(start..).collect::<Vec<_>>());
    hud_actors.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: Arc::clone(&children),
        background: None,
        z: 0,
        tint: [1.0; 4],
        blend: None,
    });
    Some(vec![children])
}
#[inline(always)]
fn translated_uv_rect(mut uv: [f32; 4], translate: [f32; 2]) -> [f32; 4] {
    uv[0] += translate[0];
    uv[1] += translate[1];
    uv[2] += translate[0];
    uv[3] += translate[1];
    uv
}

#[inline(always)]
fn maybe_flip_uv_vert(mut uv: [f32; 4], flip: bool) -> [f32; 4] {
    if flip {
        (uv[1], uv[3]) = (uv[3], uv[1]);
    }
    uv
}

#[inline(always)]
const fn maybe_mirror_uv_horiz_for_reverse_flipped(
    uv: [f32; 4],
    lane_reverse: bool,
    body_flipped: bool,
) -> [f32; 4] {
    if lane_reverse && body_flipped {
        [uv[2], uv[1], uv[0], uv[3]]
    } else {
        uv
    }
}

#[inline(always)]
const fn top_cap_rotation_deg(lane_reverse: bool, body_flipped: bool) -> f32 {
    if lane_reverse && body_flipped {
        180.0
    } else {
        0.0
    }
}

#[inline(always)]
const fn tap_part_for_note_type(note_type: NoteType) -> NoteAnimPart {
    match note_type {
        NoteType::Fake => NoteAnimPart::Fake,
        NoteType::Lift => NoteAnimPart::Lift,
        _ => NoteAnimPart::Tap,
    }
}

#[inline(always)]
fn note_slot_base_size(slot: &SpriteSlot, scale: f32) -> [f32; 2] {
    if let Some(model) = slot.model.as_ref() {
        let model_size = model.size();
        if model_size[0] > f32::EPSILON && model_size[1] > f32::EPSILON {
            return [model_size[0] * scale, model_size[1] * scale];
        }
    }
    let logical = slot.logical_size();
    [logical[0] * scale, logical[1] * scale]
}

#[inline(always)]
fn slot_zoom_x(slot: &SpriteSlot, zoom: f32) -> f32 {
    if slot.def.mirror_h { -zoom } else { zoom }
}

#[inline(always)]
fn slot_zoom_y(slot: &SpriteSlot, zoom: f32) -> f32 {
    if slot.def.mirror_v { -zoom } else { zoom }
}

#[inline(always)]
fn scale_sprite_to_arrow(size: [i32; 2], target_arrow_px: f32) -> [f32; 2] {
    let width = size[0].max(0) as f32;
    let height = size[1].max(0) as f32;
    if height <= 0.0 || target_arrow_px <= 0.0 {
        [width, height]
    } else {
        let scale = target_arrow_px / height;
        [width * scale, target_arrow_px]
    }
}

#[inline(always)]
fn scale_cap_to_arrow(size: [i32; 2], target_arrow_px: f32) -> [f32; 2] {
    let width = size[0].max(0) as f32;
    let height = size[1].max(0) as f32;
    if width <= 0.0 || target_arrow_px <= 0.0 {
        [width, height]
    } else {
        let scale = target_arrow_px / width;
        [target_arrow_px, height * scale]
    }
}

#[inline(always)]
fn offset_center(
    center: [f32; 2],
    local_offset: [f32; 2],
    local_offset_rot_sin_cos: [f32; 2],
) -> [f32; 2] {
    let [sin_r, cos_r] = local_offset_rot_sin_cos;
    let offset = [
        local_offset[0] * cos_r - local_offset[1] * sin_r,
        local_offset[0] * sin_r + local_offset[1] * cos_r,
    ];
    [center[0] + offset[0], center[1] + offset[1]]
}

#[inline(always)]
fn hold_tail_cap_bounds(
    body_tail_y: f32,
    cap_height: f32,
    rendered_body_top: Option<f32>,
    rendered_body_bottom: Option<f32>,
) -> Option<(f32, f32)> {
    let default_bounds = (body_tail_y, body_tail_y + cap_height);
    let rb = match (rendered_body_top, rendered_body_bottom) {
        (Some(t), Some(b)) if b > t + 0.5 => b,
        _ => return Some(default_bounds),
    };

    let dist = body_tail_y - rb;
    if dist < -2.0 || dist > cap_height + 2.0 {
        return Some(default_bounds);
    }

    Some((rb, rb + cap_height))
}

#[inline(always)]
fn clipped_hold_body_bounds(
    body_top: f32,
    body_bottom: f32,
    natural_top: f32,
    natural_bottom: f32,
) -> Option<(f32, f32)> {
    let clipped_top = body_top.max(natural_top);
    let clipped_bottom = body_bottom.min(natural_bottom);
    (clipped_bottom > clipped_top).then_some((clipped_top, clipped_bottom))
}

#[inline(always)]
fn hold_draw_span(y_head: f32, y_tail: f32) -> Option<(f32, f32)> {
    let mut top = y_head.min(y_tail);
    let mut bottom = y_head.max(y_tail);
    if bottom < -200.0 || top > screen_height() + 200.0 {
        return None;
    }
    top = top.max(-400.0);
    bottom = bottom.min(screen_height() + 400.0);
    (bottom >= top).then_some((top, bottom))
}

const HOLD_BODY_LEGACY_SEGMENT_LIMIT: usize = 512;
const HOLD_BODY_SEGMENT_SAFETY_MAX: usize = 65_536;

#[inline(always)]
fn hold_body_segment_budget(visible_span: f32, segment_height: f32) -> (usize, bool) {
    let estimated = if visible_span <= f32::EPSILON || segment_height <= f32::EPSILON {
        1
    } else {
        (visible_span / segment_height).ceil() as usize
    };
    let max_segments = estimated
        .saturating_add(2)
        .clamp(2048, HOLD_BODY_SEGMENT_SAFETY_MAX);
    (max_segments, estimated <= HOLD_BODY_LEGACY_SEGMENT_LIMIT)
}

#[inline(always)]
fn bottom_cap_uv_window(
    v_base0: f32,
    v_base1: f32,
    draw_height: f32,
    cap_span: f32,
    anchor_to_top: bool,
) -> Option<(f32, f32)> {
    if cap_span <= f32::EPSILON || draw_height <= f32::EPSILON {
        return None;
    }
    // ITG DrawHoldPart computes add_to_tex_coord from the visible cap height.
    let tex_add = if anchor_to_top {
        0.0
    } else {
        (1.0 - draw_height / cap_span).clamp(0.0, 1.0)
    };
    let v_span = v_base1 - v_base0;
    let t0 = tex_add;
    let t1 = (draw_height / cap_span) + tex_add;
    Some((v_base0 + v_span * t0, v_base0 + v_span * t1))
}

#[inline(always)]
fn sm_scale(v: f32, in0: f32, in1: f32, out0: f32, out1: f32) -> f32 {
    let denom = in1 - in0;
    if denom.abs() < 1e-6 {
        return out1;
    }
    ((v - in0) / denom).mul_add(out1 - out0, out0)
}

#[inline(always)]
fn quantize_step(v: f32, step: f32) -> f32 {
    ((v + step * 0.5) / step).trunc() * step
}

#[inline(always)]
fn beat_factor(song_beat: f32) -> f32 {
    let accel_time = 0.2_f32;
    let total_time = 0.5_f32;
    let mut beat = song_beat + accel_time;
    let even_beat = (beat as i32 % 2) != 0;
    if beat < 0.0 {
        return 0.0;
    }
    beat -= beat.trunc();
    beat += 1.0;
    beat -= beat.trunc();
    if beat >= total_time {
        return 0.0;
    }
    let mut factor = if beat < accel_time {
        let t = sm_scale(beat, 0.0, accel_time, 0.0, 1.0);
        t * t
    } else {
        let t = sm_scale(beat, accel_time, total_time, 1.0, 0.0);
        1.0 - (1.0 - t) * (1.0 - t)
    };
    if even_beat {
        factor *= -1.0;
    }
    factor * 20.0
}

#[inline(always)]
fn field_effect_height(tilt: f32) -> f32 {
    screen_height() + tilt.abs() * 200.0
}

#[inline(always)]
fn apply_accel_y_with_peak(
    raw_y: f32,
    elapsed: f32,
    current_beat: f32,
    effect_height: f32,
    accel: AccelEffects,
) -> (f32, bool) {
    if raw_y < 0.0 {
        return (raw_y, true);
    }
    let mut y = raw_y;
    if accel.boost > f32::EPSILON {
        let new_y = y * 1.5 / ((y + effect_height / 1.2) / effect_height);
        let mut adjust = accel.boost * (new_y - y);
        adjust = adjust.clamp(BOOST_MOD_MIN_CLAMP, BOOST_MOD_MAX_CLAMP);
        y += adjust;
    }
    if accel.brake > f32::EPSILON {
        let scale = sm_scale(y, 0.0, effect_height, 0.0, 1.0);
        let new_y = y * scale;
        let mut adjust = accel.brake * (new_y - y);
        adjust = adjust.clamp(BRAKE_MOD_MIN_CLAMP, BRAKE_MOD_MAX_CLAMP);
        y += adjust;
    }
    if accel.wave > f32::EPSILON {
        y += accel.wave * WAVE_MOD_MAGNITUDE * (y / WAVE_MOD_HEIGHT.mul_add(1.0, 0.0)).sin();
    }
    let mut before_boomerang_peak = true;
    if accel.boomerang > f32::EPSILON {
        let peak_at_y = screen_height() * 0.75;
        before_boomerang_peak = y < peak_at_y;
        y = (-y * y / screen_height()) + 1.5 * y;
    }
    if accel.expand > f32::EPSILON {
        let seconds = elapsed.rem_euclid((std::f32::consts::PI * 2.0).max(f32::EPSILON));
        let multiplier = sm_scale(
            (seconds * EXPAND_MULTIPLIER_FREQUENCY).cos(),
            EXPAND_MULTIPLIER_SCALE_FROM_LOW,
            EXPAND_MULTIPLIER_SCALE_FROM_HIGH,
            EXPAND_MULTIPLIER_SCALE_TO_LOW,
            EXPAND_MULTIPLIER_SCALE_TO_HIGH,
        );
        y *= sm_scale(
            accel.expand,
            EXPAND_SPEED_SCALE_FROM_LOW,
            EXPAND_SPEED_SCALE_FROM_HIGH,
            EXPAND_SPEED_SCALE_TO_LOW,
            multiplier,
        );
    }
    let _ = current_beat;
    (y, before_boomerang_peak)
}

#[inline(always)]
fn apply_accel_y(
    raw_y: f32,
    elapsed: f32,
    current_beat: f32,
    effect_height: f32,
    accel: AccelEffects,
) -> f32 {
    apply_accel_y_with_peak(raw_y, elapsed, current_beat, effect_height, accel).0
}

#[inline(always)]
fn signed_effect_active(value: f32) -> bool {
    value.is_finite() && value.abs() > f32::EPSILON
}

#[inline(always)]
fn tipsy_y_extra(local_col: usize, elapsed: f32, visual: VisualEffects) -> f32 {
    if !signed_effect_active(visual.tipsy) {
        return 0.0;
    }
    let col = local_col as f32;
    let angle = elapsed * TIPSY_TIMER_FREQUENCY + col * TIPSY_COLUMN_FREQUENCY;
    visual.tipsy * angle.cos() * ScrollSpeedSetting::ARROW_SPACING * TIPSY_ARROW_MAGNITUDE
}

#[inline(always)]
fn beat_x_extra(y: f32, beat_factor: f32, visual: VisualEffects) -> f32 {
    if !signed_effect_active(visual.beat) {
        return 0.0;
    }
    let shift =
        beat_factor * (y / BEAT_OFFSET_HEIGHT + std::f32::consts::PI / BEAT_PI_HEIGHT).sin();
    visual.beat * shift
}

#[inline(always)]
fn drunk_x_extra(local_col: usize, y: f32, elapsed: f32, visual: VisualEffects) -> f32 {
    if !signed_effect_active(visual.drunk) {
        return 0.0;
    }
    let col = local_col as f32;
    let angle =
        elapsed + col * DRUNK_COLUMN_FREQUENCY + y * DRUNK_OFFSET_FREQUENCY / screen_height();
    visual.drunk * angle.cos() * ScrollSpeedSetting::ARROW_SPACING * DRUNK_ARROW_MAGNITUDE
}

#[inline(always)]
fn tornado_x_extra(
    local_col: usize,
    y: f32,
    base_x: f32,
    bounds: TornadoBounds,
    visual: VisualEffects,
) -> f32 {
    if !signed_effect_active(visual.tornado) {
        return 0.0;
    }
    let position_between = sm_scale(base_x, bounds.min_x, bounds.max_x, -1.0, 1.0).clamp(-1.0, 1.0);
    let radians = position_between.acos() + y * TORNADO_X_OFFSET_FREQUENCY / screen_height();
    let adjusted = sm_scale(radians.cos(), -1.0, 1.0, bounds.min_x, bounds.max_x);
    let _ = local_col;
    (adjusted - base_x) * visual.tornado
}

#[inline(always)]
fn note_alpha(y_no_reverse: f32, elapsed: f32, mini: f32, appearance: AppearanceEffects) -> f32 {
    if y_no_reverse < 0.0 {
        return 1.0;
    }
    let zoom = (1.0 - mini * 0.5).abs().max(0.01);
    let center_line = CENTER_LINE_Y / zoom;
    let hidden_sudden = appearance.hidden * appearance.sudden;
    let hidden_end = center_line
        + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, -1.0, -1.25)
        + center_line * appearance.hidden_offset;
    let hidden_start = center_line
        + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, 0.0, -0.25)
        + center_line * appearance.hidden_offset;
    let sudden_end = center_line
        + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, 0.0, 0.25)
        + center_line * appearance.sudden_offset;
    let sudden_start = center_line
        + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, 1.0, 1.25)
        + center_line * appearance.sudden_offset;
    let mut visible_adjust = 0.0;
    if appearance.hidden > f32::EPSILON {
        visible_adjust += appearance.hidden
            * sm_scale(y_no_reverse, hidden_start, hidden_end, 0.0, -1.0).clamp(-1.0, 0.0);
    }
    if appearance.sudden > f32::EPSILON {
        visible_adjust += appearance.sudden
            * sm_scale(y_no_reverse, sudden_start, sudden_end, -1.0, 0.0).clamp(-1.0, 0.0);
    }
    if appearance.stealth > f32::EPSILON {
        visible_adjust -= appearance.stealth;
    }
    if appearance.blink > f32::EPSILON {
        let blink = quantize_step((elapsed * 10.0).sin(), BLINK_MOD_FREQUENCY);
        visible_adjust += sm_scale(blink, 0.0, 1.0, -1.0, 0.0);
    }
    if appearance.random_vanish > f32::EPSILON {
        let dist = (y_no_reverse - center_line).abs();
        visible_adjust += sm_scale(dist, 80.0, 160.0, -1.0, 0.0) * appearance.random_vanish;
    }
    (1.0 + visible_adjust).clamp(0.0, 1.0)
}

#[inline(always)]
fn note_glow(y_no_reverse: f32, elapsed: f32, mini: f32, appearance: AppearanceEffects) -> f32 {
    let percent_visible = note_alpha(y_no_reverse, elapsed, mini, appearance);
    sm_scale((percent_visible - 0.5).abs(), 0.0, 0.5, 1.3, 0.0).max(0.0)
}

#[inline(always)]
fn compute_invert_distances(col_offsets: &[f32], out: &mut [f32]) {
    let num_cols = col_offsets.len();
    if num_cols == 0 {
        return;
    }
    let num_sides = if num_cols > 4 { 2 } else { 1 };
    let cols_per_side = (num_cols / num_sides).max(1);
    for i in 0..num_cols {
        let side = i / cols_per_side;
        let on_side = i % cols_per_side;
        let left_mid = (cols_per_side - 1) / 2;
        let right_mid = cols_per_side.div_ceil(2);
        let (first, last) = if on_side <= left_mid {
            (0, left_mid)
        } else if on_side >= right_mid {
            (right_mid, cols_per_side - 1)
        } else {
            (on_side / 2, on_side / 2)
        };
        let new_on_side = if first == last {
            0
        } else {
            sm_scale(
                on_side as f32,
                first as f32,
                last as f32,
                last as f32,
                first as f32,
            )
            .round() as usize
        };
        let new_col = side * cols_per_side + new_on_side.min(num_cols.saturating_sub(1));
        out[i] = col_offsets[new_col] - col_offsets[i];
    }
}

#[inline(always)]
fn compute_tornado_bounds(col_offsets: &[f32], out: &mut [TornadoBounds]) {
    let num_cols = col_offsets.len();
    let width = if num_cols > 4 { 2 } else { 3 };
    for (i, bounds) in out.iter_mut().take(num_cols).enumerate() {
        let start = i.saturating_sub(width);
        let end = (i + width).min(num_cols.saturating_sub(1));
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        for x in &col_offsets[start..=end] {
            min_x = min_x.min(*x);
            max_x = max_x.max(*x);
        }
        *bounds = TornadoBounds { min_x, max_x };
    }
}

#[inline(always)]
fn note_x_extra(
    local_col: usize,
    y: f32,
    elapsed: f32,
    beat_factor: f32,
    visual: VisualEffects,
    col_offsets: &[f32],
    invert_distances: &[f32],
    tornado_bounds: &[TornadoBounds],
) -> f32 {
    let mut r = 0.0;
    let base_x = col_offsets[local_col];
    if signed_effect_active(visual.tornado) {
        r += tornado_x_extra(local_col, y, base_x, tornado_bounds[local_col], visual);
    }
    if signed_effect_active(visual.drunk) {
        r += drunk_x_extra(local_col, y, elapsed, visual);
    }
    if signed_effect_active(visual.flip) {
        let mirrored = col_offsets[col_offsets.len().saturating_sub(1) - local_col];
        r += (mirrored - base_x) * visual.flip;
    }
    if signed_effect_active(visual.invert) {
        r += invert_distances[local_col] * visual.invert;
    }
    if signed_effect_active(visual.beat) {
        r += beat_x_extra(y, beat_factor, visual);
    }
    r
}

#[inline(always)]
fn tiny_spacing_scale(visual: VisualEffects) -> f32 {
    if visual.tiny.abs() <= f32::EPSILON || !visual.tiny.is_finite() {
        return 1.0;
    }
    0.5_f32.powf(visual.tiny).min(1.0)
}

#[inline(always)]
fn move_x_extra(visual: VisualEffects, local_col: usize) -> f32 {
    visual
        .move_x_cols
        .get(local_col)
        .copied()
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
        * ScrollSpeedSetting::ARROW_SPACING
}

#[inline(always)]
fn move_y_extra(visual: VisualEffects, local_col: usize) -> f32 {
    visual
        .move_y_cols
        .get(local_col)
        .copied()
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
        * ScrollSpeedSetting::ARROW_SPACING
}

#[inline(always)]
fn note_x_offset(
    local_col: usize,
    y: f32,
    elapsed: f32,
    beat_factor: f32,
    visual: VisualEffects,
    col_offsets: &[f32],
    invert_distances: &[f32],
    tornado_bounds: &[TornadoBounds],
) -> f32 {
    let base = col_offsets[local_col]
        + note_x_extra(
            local_col,
            y,
            elapsed,
            beat_factor,
            visual,
            col_offsets,
            invert_distances,
            tornado_bounds,
        );
    base * tiny_spacing_scale(visual) + move_x_extra(visual, local_col)
}

#[inline(always)]
fn receptor_row_center(
    playfield_center_x: f32,
    local_col: usize,
    receptor_y_lane: f32,
    elapsed: f32,
    beat_factor: f32,
    visual: VisualEffects,
    col_offsets: &[f32],
    invert_distances: &[f32],
    tornado_bounds: &[TornadoBounds],
) -> [f32; 2] {
    [
        playfield_center_x
            + note_x_offset(
                local_col,
                0.0,
                elapsed,
                beat_factor,
                visual,
                col_offsets,
                invert_distances,
                tornado_bounds,
            ),
        receptor_y_lane
            + tipsy_y_extra(local_col, elapsed, visual)
            + move_y_extra(visual, local_col),
    ]
}

#[inline(always)]
fn hold_segment_pose(top: [f32; 2], bottom: [f32; 2]) -> ([f32; 2], f32, f32) {
    let dx = bottom[0] - top[0];
    let dy = bottom[1] - top[1];
    let length = dx.hypot(dy);
    let rotation_deg = if length <= f32::EPSILON {
        0.0
    } else {
        dx.atan2(dy).to_degrees()
    };
    (
        [(top[0] + bottom[0]) * 0.5, (top[1] + bottom[1]) * 0.5],
        length,
        rotation_deg,
    )
}

#[inline(always)]
fn hold_strip_row_3d(
    center: [f32; 3],
    forward: [f32; 2],
    half_width: f32,
    u0: f32,
    u1: f32,
    v: f32,
    color: [f32; 4],
) -> [TexturedMeshVertex; 2] {
    let len = forward[0].hypot(forward[1]).max(f32::EPSILON);
    let nx = -forward[1] / len * half_width;
    let ny = forward[0] / len * half_width;
    [
        TexturedMeshVertex {
            pos: [center[0] + nx, center[1] + ny, center[2]],
            uv: [u0, v],
            tex_matrix_scale: [1.0, 1.0],
            color,
        },
        TexturedMeshVertex {
            pos: [center[0] - nx, center[1] - ny, center[2]],
            uv: [u1, v],
            tex_matrix_scale: [1.0, 1.0],
            color,
        },
    ]
}

#[inline(always)]
fn hold_strip_row_from_positions(
    left: [f32; 3],
    right: [f32; 3],
    u0: f32,
    u1: f32,
    v: f32,
    color: [f32; 4],
) -> [TexturedMeshVertex; 2] {
    [
        TexturedMeshVertex {
            pos: left,
            uv: [u0, v],
            tex_matrix_scale: [1.0, 1.0],
            color,
        },
        TexturedMeshVertex {
            pos: right,
            uv: [u1, v],
            tex_matrix_scale: [1.0, 1.0],
            color,
        },
    ]
}

#[inline(always)]
const fn hold_glow_color(alpha: f32) -> [f32; 4] {
    [1.0, 1.0, 1.0, alpha]
}

#[inline(always)]
fn hold_strip_quad(
    top: [TexturedMeshVertex; 2],
    bottom: [TexturedMeshVertex; 2],
) -> [TexturedMeshVertex; 6] {
    [top[0], top[1], bottom[1], top[0], bottom[1], bottom[0]]
}

#[inline(always)]
fn hold_strip_actor(
    texture: Arc<str>,
    vertices: Arc<[TexturedMeshVertex]>,
    blend: BlendMode,
    depth_test: bool,
    z: i16,
) -> Actor {
    Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        world_z: 0.0,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        local_transform: Matrix4::IDENTITY,
        texture,
        tint: [1.0; 4],
        glow: [1.0, 1.0, 1.0, 0.0],
        vertices,
        geom_cache_key: crate::engine::gfx::INVALID_TMESH_CACHE_KEY,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        depth_test,
        visible: true,
        blend,
        z,
    }
}

#[inline(always)]
fn mod_divisor(value: f32) -> f32 {
    if value.abs() > 0.001 {
        value
    } else if value.is_sign_negative() {
        -0.001
    } else {
        0.001
    }
}

#[inline(always)]
fn bumpy_angle(y: f32, offset: f32, period: f32) -> f32 {
    let offset = if offset.is_finite() { offset } else { 0.0 };
    let period = if period.is_finite() { period } else { 0.0 };
    let divisor = mod_divisor(period.mul_add(BUMPY_Z_ANGLE_DIVISOR, BUMPY_Z_ANGLE_DIVISOR));
    (y + 100.0 * offset) / divisor
}

#[inline(always)]
fn note_world_z_for_bumpy(y: f32, bumpy: f32, offset: f32, period: f32) -> f32 {
    if bumpy.abs() <= f32::EPSILON || !bumpy.is_finite() {
        return 0.0;
    }
    bumpy * BUMPY_Z_MAGNITUDE * bumpy_angle(y, offset, period).sin()
}

#[inline(always)]
fn bumpy_for_col(visual: &VisualEffects, local_col: usize) -> f32 {
    visual.bumpy + visual.bumpy_cols.get(local_col).copied().unwrap_or(0.0)
}

#[inline(always)]
fn tiny_zoom_for_col(visual: &VisualEffects, local_col: usize) -> f32 {
    let tiny = visual.tiny + visual.tiny_cols.get(local_col).copied().unwrap_or(0.0);
    if tiny.abs() <= f32::EPSILON || !tiny.is_finite() {
        return 1.0;
    }
    0.5_f32.powf(tiny)
}

#[inline(always)]
fn pulse_active(visual: &VisualEffects) -> bool {
    visual.pulse_inner.abs() > f32::EPSILON || visual.pulse_outer.abs() > f32::EPSILON
}

#[inline(always)]
fn pulse_inner_zoom(visual: &VisualEffects) -> f32 {
    if !pulse_active(visual) {
        return 1.0;
    }
    let inner = if visual.pulse_inner.is_finite() {
        visual.pulse_inner.mul_add(0.5, 1.0)
    } else {
        1.0
    };
    if inner.abs() <= f32::EPSILON {
        0.01
    } else {
        inner
    }
}

#[inline(always)]
fn pulse_zoom_for_y(y: f32, visual: &VisualEffects) -> f32 {
    if !pulse_active(visual) {
        return 1.0;
    }
    let outer = if visual.pulse_outer.is_finite() {
        visual.pulse_outer
    } else {
        0.0
    };
    let offset = if visual.pulse_offset.is_finite() {
        visual.pulse_offset
    } else {
        0.0
    };
    let period = if visual.pulse_period.is_finite() {
        visual.pulse_period
    } else {
        0.0
    };
    let divisor = mod_divisor(0.4 * TARGET_ARROW_PIXEL_SIZE * (1.0 + period));
    ((y + 100.0 * offset) / divisor)
        .sin()
        .mul_add(outer * 0.5, pulse_inner_zoom(visual))
}

#[inline(always)]
fn arrow_effect_zoom(visual: &VisualEffects, local_col: usize, y: f32) -> f32 {
    tiny_zoom_for_col(visual, local_col) * pulse_zoom_for_y(y, visual)
}

#[inline(always)]
fn actor_with_world_z(mut actor: Actor, world_z: f32) -> Actor {
    if world_z.abs() <= f32::EPSILON {
        return actor;
    }
    match &mut actor {
        Actor::Sprite { world_z: z, .. } | Actor::TexturedMesh { world_z: z, .. } => *z = world_z,
        _ => {}
    }
    actor
}

struct NoteGlowDraw<'a> {
    slot: &'a SpriteSlot,
    draw: ModelDrawState,
    model_center: [f32; 2],
    sprite_center: [f32; 2],
    size: [f32; 2],
    uv: [f32; 4],
    rotation_y: f32,
    model_rotation_z: f32,
    sprite_rotation_z: f32,
    alpha: f32,
    blend: BlendMode,
    z: i16,
    world_z: f32,
    prefer_sprite: bool,
}

fn push_note_glow_actor(
    actors: &mut Vec<Actor>,
    spec: NoteGlowDraw<'_>,
    model_cache: &mut ModelMeshCache,
) {
    if spec.alpha <= f32::EPSILON {
        return;
    }
    if !spec.prefer_sprite
        && let Some(glow_actor) = noteskin_model_actor_from_draw_cached(
            spec.slot,
            spec.draw,
            spec.model_center,
            spec.size,
            spec.uv,
            spec.model_rotation_z,
            [1.0, 1.0, 1.0, 0.0],
            spec.blend,
            spec.z,
            model_cache,
        )
    {
        let mut glow_actor = glow_actor;
        if let Actor::TexturedMesh { glow, .. } = &mut glow_actor {
            *glow = [1.0, 1.0, 1.0, spec.alpha];
        }
        actors.push(actor_with_world_z(glow_actor, spec.world_z));
        return;
    }
    // ITG Actor glow is a second white pass through TextureMode_Glow.
    if spec.draw.blend_add {
        actors.push(actor_with_world_z(
            act!(sprite(spec.slot.texture_key_handle()):
                align(0.5, 0.5):
                xy(spec.sprite_center[0], spec.sprite_center[1]):
                setsize(spec.size[0], spec.size[1]):
                rotationy(spec.rotation_y):
                rotationz(spec.sprite_rotation_z):
                customtexturerect(spec.uv[0], spec.uv[1], spec.uv[2], spec.uv[3]):
                diffuse(1.0, 1.0, 1.0, 0.0):
                glow(1.0, 1.0, 1.0, spec.alpha):
                blend(add):
                z(spec.z as i32)
            ),
            spec.world_z,
        ));
    } else {
        actors.push(actor_with_world_z(
            act!(sprite(spec.slot.texture_key_handle()):
                align(0.5, 0.5):
                xy(spec.sprite_center[0], spec.sprite_center[1]):
                setsize(spec.size[0], spec.size[1]):
                rotationy(spec.rotation_y):
                rotationz(spec.sprite_rotation_z):
                customtexturerect(spec.uv[0], spec.uv[1], spec.uv[2], spec.uv[3]):
                diffuse(1.0, 1.0, 1.0, 0.0):
                glow(1.0, 1.0, 1.0, spec.alpha):
                blend(normal):
                z(spec.z as i32)
            ),
            spec.world_z,
        ));
    }
}

#[inline(always)]
fn itg_actor_rotation_z(deg: f32) -> f32 {
    // ITGmania ArrowEffects returns Actor::rotationz degrees in screen space.
    // DeadSync applies sprite rotations in world space, where Y is inverted.
    -deg
}

#[inline(always)]
fn confusion_rotation_deg(song_beat: f32, visual: VisualEffects, local_col: usize) -> f32 {
    let mut itg_rotation = 0.0;
    let col_offset = visual
        .confusion_offset_cols
        .get(local_col)
        .copied()
        .filter(|value| value.is_finite())
        .unwrap_or(0.0);
    if col_offset.abs() > f32::EPSILON {
        itg_rotation += col_offset * (180.0 / std::f32::consts::PI);
    }
    if visual.confusion_offset.abs() > f32::EPSILON {
        itg_rotation += visual.confusion_offset * (180.0 / std::f32::consts::PI);
    }
    if visual.confusion.abs() > f32::EPSILON {
        let confusion = (song_beat * visual.confusion).rem_euclid(std::f32::consts::TAU);
        itg_rotation += confusion * (-180.0 / std::f32::consts::PI);
    }
    itg_actor_rotation_z(itg_rotation)
}

#[inline(always)]
fn dizzy_rotation_deg(note_beat: f32, song_beat: f32, visual: VisualEffects) -> f32 {
    if visual.dizzy <= f32::EPSILON {
        return 0.0;
    }
    let dizzy = ((note_beat - song_beat) * visual.dizzy).rem_euclid(std::f32::consts::TAU);
    dizzy * (180.0 / std::f32::consts::PI)
}

#[inline(always)]
fn calc_note_rotation_z(
    visual: VisualEffects,
    note_beat: f32,
    song_beat: f32,
    is_hold_head: bool,
    local_col: usize,
) -> f32 {
    let mut r = confusion_rotation_deg(song_beat, visual, local_col);
    if visual.dizzy > f32::EPSILON && !is_hold_head {
        r += itg_actor_rotation_z(dizzy_rotation_deg(note_beat, song_beat, visual));
    }
    r
}

#[inline(always)]
fn song_lua_note_model_draw(mut draw: ModelDrawState, rotation_y_deg: f32) -> ModelDrawState {
    if rotation_y_deg.abs() > f32::EPSILON {
        draw.rot[1] += rotation_y_deg;
    }
    draw
}

#[inline(always)]
fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline(always)]
fn effective_mini_value(
    profile: &profile::Profile,
    visual: VisualEffects,
    mini_percent: f32,
) -> f32 {
    let mut mini = if mini_percent.is_finite() {
        mini_percent
    } else {
        profile.mini_percent as f32
    };
    if visual.big > f32::EPSILON {
        // ITG _fallback/ArrowCloud map Effect Big to mod,-100% mini.
        mini -= 100.0;
    }
    mini.clamp(-100.0, 150.0) / 100.0
}

#[inline(always)]
fn judgment_actor_zoom(mini: f32, judgment_back: bool) -> f32 {
    if judgment_back {
        // Arrow Cloud's JudgmentBack actorframe applies its own linear
        // shrink on top of the Player ActorFrame inheritance.
        ((2.0 - mini) * 0.5).clamp(0.35, 1.0)
    } else {
        // ITGmania Player::Update applies the same min(pow(0.5, mini+tiny), 1.0)
        // factor to the front judgment actor that it applies to combo (see
        // Player.cpp fJudgmentZoom -> m_pActorWithJudgmentPosition->SetZoom).
        // Simply Love does not override this, so the front judgment shrinks
        // with Mini just like the combo does.
        combo_actor_zoom(mini)
    }
}

#[inline(always)]
fn combo_actor_zoom(mini: f32) -> f32 {
    // ITGmania Player::Update: min(pow(0.5, mini + tiny), 1.0). The Player
    // ActorFrame's mini scale is inherited by both the combo display and
    // the front judgment actor in Simply Love.
    0.5_f32.powf(mini).min(1.0)
}

#[inline(always)]
fn hallway_judgment_zoom(perspective_tilt: f32, perspective_skew: f32) -> f32 {
    // ITGmania's hallway draw path applies an extra 0.9x shrink to the notefield
    // during the perspective pass, but the judgment actor keeps its original zoom.
    // Mirror that apparent larger hallway judgment here for the HUD sprite path.
    if perspective_tilt >= -f32::EPSILON || perspective_skew.abs() > f32::EPSILON {
        return 1.0;
    }
    1.0 / (1.0 - 0.1 * (-perspective_tilt).clamp(0.0, 1.0)).max(0.000_001)
}

#[inline(always)]
fn format_speed_mod_for_display(speed: ScrollSpeedSetting) -> String {
    let fmt_float = |v: f32| -> String {
        let s = cached_fmt2_f32(v);
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    };

    match speed {
        ScrollSpeedSetting::XMod(mult) => {
            if (mult - 1.0).abs() <= 0.000_1 {
                "1x".to_string()
            } else {
                let mut out = fmt_float(mult);
                out.push('x');
                out
            }
        }
        ScrollSpeedSetting::CMod(bpm) => {
            if (bpm - bpm.round()).abs() <= 0.000_1 {
                let mut out = String::from("C");
                out.push_str(&(bpm.round() as i32).to_string());
                out
            } else {
                let mut out = String::from("C");
                out.push_str(&fmt_float(bpm));
                out
            }
        }
        ScrollSpeedSetting::MMod(bpm) => {
            if (bpm - bpm.round()).abs() <= 0.000_1 {
                let mut out = String::from("m");
                out.push_str(&(bpm.round() as i32).to_string());
                out
            } else {
                let mut out = String::from("m");
                out.push_str(&fmt_float(bpm));
                out
            }
        }
    }
}

#[inline(always)]
pub(crate) fn gameplay_mods_text(state: &State, player_idx: usize) -> Arc<str> {
    let key = gameplay_mods_text_key(state, player_idx);
    cached_text(&GAMEPLAY_MODS_CACHE, key, TEXT_CACHE_LIMIT, || {
        let mut parts = Vec::with_capacity(32);
        parts.push(format_speed_mod_for_display(
            effective_scroll_speed_for_player(state, player_idx),
        ));

        for (percent, name) in
            key.accel
                .into_iter()
                .zip(["Boost", "Brake", "Wave", "Expand", "Boomerang"])
        {
            append_mod_part(&mut parts, percent, name);
        }
        for (percent, name) in key.visual.into_iter().zip([
            "Drunk",
            "Dizzy",
            "Confusion",
            "Flip",
            "Invert",
            "Tornado",
            "Tipsy",
            "Bumpy",
            "Beat",
        ]) {
            append_mod_part(&mut parts, percent, name);
        }
        append_mini_part(&mut parts, key.mini_percent);
        append_spacing_part(&mut parts, key.spacing_percent);
        for (percent, name) in
            key.appearance
                .into_iter()
                .zip(["Hidden", "Sudden", "Stealth", "Blink", "RandomVanish"])
        {
            append_mod_part(&mut parts, percent, name);
        }
        for (percent, name) in
            key.scroll
                .into_iter()
                .zip(["Reverse", "Split", "Alternate", "Cross", "Centered"])
        {
            append_mod_part(&mut parts, percent, name);
        }
        append_mod_part(&mut parts, key.dark, "Dark");
        append_mod_part(&mut parts, key.blind, "Blind");
        append_mod_part(&mut parts, key.cover, "Hide BG");

        if let Some(name) = attack_mode_name(state.player_profiles[player_idx].attack_mode) {
            parts.push(name.to_string());
        }
        append_turn_parts(&mut parts, key.turn_bits);
        push_transform_parts(&mut parts, key.insert_mask, key.remove_mask, key.holds_mask);
        append_perspective_parts(&mut parts, key.perspective_tilt, key.perspective_skew);
        parts.push(state.player_profiles[player_idx].noteskin.to_string());
        if key.visual_delay_ms != 0 {
            parts.push(format!("{}ms VisualDelay", key.visual_delay_ms));
        }

        parts.join(", ")
    })
}

#[inline(always)]
fn active_column_cue(
    cues: &[crate::game::gameplay::ColumnCue],
    current_time: f32,
) -> Option<&crate::game::gameplay::ColumnCue> {
    if cues.is_empty() {
        return None;
    }
    let idx = cues.partition_point(|cue| cue.start_time <= current_time);
    idx.checked_sub(1).and_then(|i| cues.get(i))
}

#[inline(always)]
fn column_cue_alpha(elapsed_real: f32, duration_real: f32) -> f32 {
    if !elapsed_real.is_finite() || !duration_real.is_finite() {
        return 0.0;
    }
    if elapsed_real < 0.0 || elapsed_real > duration_real {
        return 0.0;
    }
    if duration_real <= COLUMN_CUE_FADE_TIME * 2.0 {
        return 0.0;
    }
    if elapsed_real < COLUMN_CUE_FADE_TIME {
        let t = (elapsed_real / COLUMN_CUE_FADE_TIME).clamp(0.0, 1.0);
        return 1.0 - (1.0 - t) * (1.0 - t);
    }
    if elapsed_real > duration_real - COLUMN_CUE_FADE_TIME {
        let t = ((elapsed_real - (duration_real - COLUMN_CUE_FADE_TIME)) / COLUMN_CUE_FADE_TIME)
            .clamp(0.0, 1.0);
        return 1.0 - t * t;
    }
    1.0
}

#[inline(always)]
fn column_cue_height() -> f32 {
    (screen_height() - COLUMN_CUE_Y_OFFSET).max(0.0)
}

#[inline(always)]
fn column_cue_reverse_bottom_y(lane_width: f32, notefield_offset_y: f32) -> f32 {
    // Simply Love rotates a top-aligned quad around the actor origin. DeadSync's
    // sprite fast path rotates around the rect center, so reverse cues are drawn
    // unrotated from their equivalent top edge instead.
    COLUMN_CUE_Y_OFFSET * 3.0
        + RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE
        + lane_width * 0.5
        + notefield_offset_y
}

#[inline(always)]
fn column_cue_reverse_top_y(lane_width: f32, cue_height: f32, notefield_offset_y: f32) -> f32 {
    column_cue_reverse_bottom_y(lane_width, notefield_offset_y) - cue_height
}

#[inline(always)]
const fn timing_window_from_num(n: usize) -> TimingWindow {
    match n {
        0 => TimingWindow::W0,
        1 => TimingWindow::W1,
        2 => TimingWindow::W2,
        3 => TimingWindow::W3,
        4 => TimingWindow::W4,
        _ => TimingWindow::W5,
    }
}

#[inline(always)]
fn error_bar_color_for_window(window: TimingWindow, show_fa_plus_window: bool) -> [f32; 4] {
    match window {
        TimingWindow::W0 => color::JUDGMENT_RGBA[0],
        TimingWindow::W1 => {
            if show_fa_plus_window {
                color::JUDGMENT_FA_PLUS_WHITE_RGBA
            } else {
                color::JUDGMENT_RGBA[0]
            }
        }
        TimingWindow::W2 => color::JUDGMENT_RGBA[1],
        TimingWindow::W3 => color::JUDGMENT_RGBA[2],
        TimingWindow::W4 => color::JUDGMENT_RGBA[3],
        TimingWindow::W5 => color::JUDGMENT_RGBA[4],
    }
}

#[inline(always)]
fn split_15_10ms_active(profile: &profile::Profile, judgment: &Judgment) -> bool {
    profile.show_fa_plus_window
        && profile.split_15_10ms
        && !profile.custom_fantastic_window
        && judgment.grade == JudgeGrade::Fantastic
        && judgment.time_error_ms.abs() > crate::game::timing::FA_PLUS_W010_MS
        && judgment.time_error_ms.abs() <= crate::game::timing::FA_PLUS_W0_MS
}

#[inline(always)]
fn resolved_judgment_texture(profile: &profile::Profile) -> Option<&'static assets::TextureChoice> {
    assets::resolve_texture_choice_entry(
        profile.judgment_graphic.texture_key(),
        assets::judgment_texture_choices(),
    )
}

#[inline(always)]
fn resolved_hold_judgment_texture(
    profile: &profile::Profile,
) -> Option<&'static assets::TextureChoice> {
    assets::resolve_texture_choice_entry(
        profile.hold_judgment_graphic.texture_key(),
        assets::hold_judgment_texture_choices(),
    )
}

#[inline(always)]
fn tap_judgment_rows(
    profile: &profile::Profile,
    judgment: &Judgment,
    frame_rows: usize,
) -> (usize, Option<usize>) {
    if frame_rows < 7 {
        return match judgment.grade {
            JudgeGrade::Fantastic => (0, None),
            JudgeGrade::Excellent => (1, None),
            JudgeGrade::Great => (2, None),
            JudgeGrade::Decent => (3, None),
            JudgeGrade::WayOff => (4, None),
            JudgeGrade::Miss => (5, None),
        };
    }

    match judgment.grade {
        JudgeGrade::Fantastic => {
            if split_15_10ms_active(profile, judgment) {
                // zmod SplitWhites keeps the 15ms blue base, then overlays the
                // white Fantastic art at half alpha for the 10ms-15ms slice.
                (0, Some(1))
            } else if profile.show_fa_plus_window {
                match judgment.window {
                    Some(TimingWindow::W0) => (0, None),
                    _ => (1, None),
                }
            } else {
                (0, None)
            }
        }
        JudgeGrade::Excellent => (2, None),
        JudgeGrade::Great => (3, None),
        JudgeGrade::Decent => (4, None),
        JudgeGrade::WayOff => (5, None),
        JudgeGrade::Miss => (6, None),
    }
}

#[inline(always)]
fn error_bar_tick_alpha(age: f32, dur: f32, multi_tick: bool) -> f32 {
    if !age.is_finite() || age < 0.0 {
        return 0.0;
    }
    if multi_tick {
        if age < 0.03 {
            1.0
        } else if age < dur {
            1.0 - (age - 0.03) / (dur - 0.03).max(0.000_001)
        } else {
            0.0
        }
    } else if age < dur {
        1.0
    } else {
        0.0
    }
}

#[inline(always)]
fn error_bar_flash_alpha(now: f32, started_at: Option<f32>, dur: f32) -> f32 {
    let Some(t0) = started_at else {
        return ERROR_BAR_SEG_ALPHA_BASE;
    };
    let age = now - t0;
    if !age.is_finite() || age < 0.0 || age >= dur {
        return ERROR_BAR_SEG_ALPHA_BASE;
    }
    let t = (age / dur).clamp(0.0, 1.0);
    1.0 - (1.0 - ERROR_BAR_SEG_ALPHA_BASE) * t
}

#[inline(always)]
fn error_bar_trim_max_window_ix(trim: profile::ErrorBarTrim) -> usize {
    match trim {
        profile::ErrorBarTrim::Off => 4,       // W5
        profile::ErrorBarTrim::Fantastic => 0, // W1
        profile::ErrorBarTrim::Excellent => 1, // W2
        profile::ErrorBarTrim::Great => 2,     // W3
    }
}

#[inline(always)]
fn error_bar_boundaries_s(
    windows_s: [f32; 5],
    w0_s: Option<f32>,
    show_fa_plus_window: bool,
    trim: profile::ErrorBarTrim,
) -> ([f32; 6], usize) {
    let mut out = [0.0_f32; 6];
    let mut len: usize = 0;
    let base_end = error_bar_trim_max_window_ix(trim) + 1; // 1..=5
    for wi in 1..=base_end {
        if show_fa_plus_window && wi == 1 {
            if let Some(w0) = w0_s
                && len < out.len()
            {
                out[len] = w0;
                len += 1;
            }
            if len < out.len() {
                out[len] = windows_s[0];
                len += 1;
            }
        } else if len < out.len() {
            out[len] = windows_s[wi - 1];
            len += 1;
        }
    }
    (out, len)
}

#[derive(Clone, Copy, Debug)]
struct ZmodLayoutYs {
    combo_y: f32,
    measure_counter_y: Option<f32>,
    subtractive_scoring_y: f32,
}

#[derive(Clone, Copy, Debug)]
struct HudLayoutYs {
    judgment_y: f32,
    error_bar_y: f32,
    error_bar_max_h: f32,
    zmod_layout: ZmodLayoutYs,
}

#[inline(always)]
fn hud_y(
    normal_y: f32,
    reverse_y: f32,
    centered_y: f32,
    reverse: bool,
    centered_percent: f32,
) -> f32 {
    let base_y = if reverse { reverse_y } else { normal_y };
    sm_scale(centered_percent, 0.0, 1.0, base_y, centered_y)
}

#[inline(always)]
fn zmod_layout_ys(
    profile: &crate::game::profile::Profile,
    judgment_y: f32,
    combo_y_base: f32,
    reverse: bool,
) -> ZmodLayoutYs {
    let mut top_y = judgment_y - ERROR_BAR_JUDGMENT_HEIGHT * 0.5;
    let mut bottom_y = judgment_y + ERROR_BAR_JUDGMENT_HEIGHT * 0.5;

    // Zmod SL-Layout.lua: hasErrorBar checks multiple flags.
    let mut error_bar_mask = profile.error_bar_active_mask;
    if error_bar_mask.is_empty() {
        error_bar_mask =
            profile::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text);
    }
    let has_error_bar = !error_bar_mask.is_empty();
    if has_error_bar {
        if resolved_judgment_texture(profile).is_none() {
            // Error bar replaces judgment; no top/bottom adjustment.
        } else if profile.error_bar_up {
            top_y -= 15.0;
        } else {
            bottom_y += 15.0;
        }
    }

    let mut measure_counter_y = None;
    let has_measure_counter = profile.measure_counter != crate::game::profile::MeasureCounter::None;
    if has_measure_counter {
        if profile.measure_counter_up {
            let mut y = top_y - 8.0;
            top_y -= 20.0;
            if profile.broken_run {
                y -= 16.0;
            }
            measure_counter_y = Some(y);
        } else {
            measure_counter_y = Some(bottom_y + 8.0);
            bottom_y += 21.0;
        }
    }

    // Zmod: HideLookahead is not implemented in deadsync, so we always take the normal branch.
    let subtractive_scoring_y = if has_measure_counter && profile.measure_counter_up {
        let y = bottom_y + 8.0;
        bottom_y += 16.0;
        y
    } else {
        let y = top_y - 8.0;
        top_y -= 16.0;
        y
    };

    let combo_y = if reverse {
        combo_y_base.min(top_y - 20.0)
    } else {
        combo_y_base.max(bottom_y + 20.0)
    };

    ZmodLayoutYs {
        combo_y,
        measure_counter_y,
        subtractive_scoring_y,
    }
}

#[inline(always)]
fn hud_layout_ys(
    profile: &crate::game::profile::Profile,
    judgment_y_base: f32,
    combo_y_base: f32,
    reverse: bool,
    judgment_extra_y: f32,
    combo_extra_y: f32,
    error_bar_extra_y: f32,
) -> HudLayoutYs {
    let mut zmod_layout = zmod_layout_ys(profile, judgment_y_base, combo_y_base, reverse);
    zmod_layout.combo_y += combo_extra_y;
    let judgment_y = judgment_y_base + judgment_extra_y;
    let (error_bar_y, error_bar_max_h) = if resolved_judgment_texture(profile).is_none() {
        (judgment_y_base + error_bar_extra_y, 30.0_f32)
    } else if profile.error_bar_up {
        (
            judgment_y_base - ERROR_BAR_OFFSET_FROM_JUDGMENT + error_bar_extra_y,
            10.0_f32,
        )
    } else {
        (
            judgment_y_base + ERROR_BAR_OFFSET_FROM_JUDGMENT + error_bar_extra_y,
            10.0_f32,
        )
    };
    HudLayoutYs {
        judgment_y,
        error_bar_y,
        error_bar_max_h,
        zmod_layout,
    }
}

#[inline(always)]
fn stream_segment_index_exclusive_end(segs: &[StreamSegment], curr_measure: f32) -> usize {
    if curr_measure.is_nan() {
        return segs.len();
    }
    segs.partition_point(|s| curr_measure >= s.end as f32)
}

#[inline(always)]
fn stream_segment_index_inclusive_end(segs: &[StreamSegment], curr_measure: f32) -> usize {
    if curr_measure.is_nan() {
        return segs.len();
    }
    segs.partition_point(|s| curr_measure > s.end as f32)
}

fn zmod_measure_counter_text(
    curr_beat_floor: f32,
    curr_measure: f32,
    segs: &[StreamSegment],
    stream_index_unshifted: usize,
    is_lookahead: bool,
    lookahead: u8,
    multiplier: f32,
) -> Option<Arc<str>> {
    if segs.is_empty() {
        return None;
    }

    let mut stream_index = stream_index_unshifted as isize;
    let beat_div4 = curr_beat_floor / 4.0;

    if curr_measure < 0.0 {
        if !is_lookahead {
            let first = segs[0];
            if !first.is_break {
                let v = ((-beat_div4) + (1.0 * multiplier)).floor() as i32;
                return Some(cached_paren_i32(v));
            }
            let len = (first.end - first.start) as i32;
            let v_unscaled = (-beat_div4).floor() as i32 + 1 + len;
            let v = ((v_unscaled as f32) * multiplier).floor() as i32;
            return Some(cached_paren_i32(v));
        }
        if !segs[0].is_break {
            stream_index -= 1;
        }
    }

    let seg = stream_index
        .try_into()
        .ok()
        .and_then(|i: usize| segs.get(i).copied())?;

    let segment_start = seg.start as f32;
    let segment_end = seg.end as f32;
    let seg_len = ((segment_end - segment_start) * multiplier).floor() as i32;
    let curr_count = (((beat_div4 - segment_start) * multiplier).floor() as i32) + 1;

    if seg.is_break {
        if lookahead == 0 {
            return None;
        }
        if is_lookahead {
            Some(cached_paren_i32(seg_len))
        } else {
            let remaining = seg_len - curr_count + 1;
            Some(cached_paren_i32(remaining))
        }
    } else if !is_lookahead && curr_count != 0 {
        Some(cached_ratio_i32(curr_count, seg_len))
    } else {
        Some(cached_int_i32(seg_len))
    }
}

fn zmod_broken_run_end(segs: &[StreamSegment], start_index: usize) -> (usize, bool) {
    let Some(first) = segs.get(start_index).copied() else {
        return (0, false);
    };
    if first.is_break {
        return (first.end, false);
    }

    let last_index = segs.len().saturating_sub(1);
    let mut end = first.end;
    let mut broken = false;

    for i in (start_index + 1)..segs.len() {
        let seg = segs[i];
        let len = seg.end - seg.start;
        if seg.is_break {
            if len < 4 && i != last_index {
                end += len;
                broken = true;
                continue;
            }
            break;
        }

        broken = true;
        end += len;
        if !segs[i - 1].is_break {
            end += 1;
        }
    }

    (end, broken)
}

fn zmod_broken_run_segment(
    segs: &[StreamSegment],
    curr_measure: f32,
) -> Option<(usize, usize, bool)> {
    for (i, seg) in segs.iter().copied().enumerate() {
        if seg.is_break {
            if curr_measure < seg.end as f32 {
                return Some((i, seg.end, false));
            }
            continue;
        }
        let (end, broken) = zmod_broken_run_end(segs, i);
        if curr_measure < end as f32 {
            return Some((i, end, broken));
        }
    }
    None
}

fn zmod_run_timer_index(segs: &[StreamSegment], curr_measure: f32) -> Option<usize> {
    let i = stream_segment_index_inclusive_end(segs, curr_measure);
    if i < segs.len() { Some(i) } else { None }
}

#[inline(always)]
fn zmod_run_timer_fmt(seconds: i32, minute_threshold: i32, trailing_space: bool) -> Arc<str> {
    cached_run_timer(seconds, minute_threshold, trailing_space)
}

#[inline(always)]
fn zmod_small_combo_font(combo_font: profile::ComboFont) -> &'static str {
    match combo_font {
        profile::ComboFont::Wendy | profile::ComboFont::WendyCursed => "wendy",
        profile::ComboFont::ArialRounded => "combo_arial_rounded",
        profile::ComboFont::Asap => "combo_asap",
        profile::ComboFont::BebasNeue => "combo_bebas_neue",
        profile::ComboFont::SourceCode => "combo_source_code",
        profile::ComboFont::Work => "combo_work",
        profile::ComboFont::Mega => "combo_mega",
        profile::ComboFont::None => "wendy",
    }
}

#[inline(always)]
fn zmod_combo_font_name(combo_font: profile::ComboFont) -> Option<&'static str> {
    match combo_font {
        profile::ComboFont::Wendy => Some("wendy_combo"),
        profile::ComboFont::ArialRounded => Some("combo_arial_rounded"),
        profile::ComboFont::Asap => Some("combo_asap"),
        profile::ComboFont::BebasNeue => Some("combo_bebas_neue"),
        profile::ComboFont::SourceCode => Some("combo_source_code"),
        profile::ComboFont::Work => Some("combo_work"),
        profile::ComboFont::WendyCursed => Some("combo_wendy_cursed"),
        profile::ComboFont::Mega => Some("combo_mega"),
        profile::ComboFont::None => None,
    }
}

pub fn prewarm_text_layout(
    cache: &mut TextLayoutCache,
    fonts: &HashMap<&'static str, font::Font>,
    state: &State,
) {
    let prewarm_u32 = |cache: &mut TextLayoutCache, font_name: &'static str, value: u32| {
        let text = cached_int_u32(value);
        cache.prewarm_text(fonts, font_name, text.as_ref(), None);
    };
    let prewarm_i32 = |cache: &mut TextLayoutCache, font_name: &'static str, value: i32| {
        let text = cached_int_i32(value);
        cache.prewarm_text(fonts, font_name, text.as_ref(), None);
    };
    let prewarm_ratio =
        |cache: &mut TextLayoutCache, font_name: &'static str, curr: i32, total: i32| {
            let text = cached_ratio_i32(curr, total);
            cache.prewarm_text(fonts, font_name, text.as_ref(), None);
        };
    let prewarm_timer = |cache: &mut TextLayoutCache,
                         font_name: &'static str,
                         second: i32,
                         threshold: i32,
                         trailing: bool| {
        let text = cached_run_timer(second, threshold, trailing);
        cache.prewarm_text(fonts, font_name, text.as_ref(), None);
    };
    let prewarm_percent = |cache: &mut TextLayoutCache, font_name: &'static str, value: f64| {
        let text = cached_percent2_f64(value.clamp(0.0, 100.0));
        cache.prewarm_text(fonts, font_name, text.as_ref(), None);
    };
    let prewarm_signed_percent =
        |cache: &mut TextLayoutCache, font_name: &'static str, value: f64, neg: bool| {
            let text = cached_signed_percent2_f64(value.clamp(0.0, 100.0), neg);
            cache.prewarm_text(fonts, font_name, text.as_ref(), None);
        };
    let prewarm_neg_u32 = |cache: &mut TextLayoutCache, font_name: &'static str, value: u32| {
        let text = cached_neg_int_u32(value);
        cache.prewarm_text(fonts, font_name, text.as_ref(), None);
    };
    let prewarm_offset = |cache: &mut TextLayoutCache, value: f32| {
        let text = cached_offset_ms(value);
        cache.prewarm_text(fonts, "wendy", text.as_ref(), None);
    };

    let mut max_combo = 0u32;
    let mut max_measure_len = 0i32;
    let music_end_seconds = crate::game::gameplay::song_time_ns_to_seconds(state.music_end_time_ns)
        .ceil()
        .max(0.0) as i32;

    for player in 0..state.num_players {
        let profile = &state.player_profiles[player];
        max_combo = max_combo.max(
            state.total_steps[player]
                .saturating_add(state.holds_total[player])
                .saturating_add(state.rolls_total[player]),
        );

        if let Some(font_name) = zmod_combo_font_name(profile.combo_font) {
            for value in 0..=max_combo.min(COMBO_PREWARM_CAP) {
                prewarm_u32(cache, font_name, value);
            }
            prewarm_u32(cache, font_name, max_combo);
        }

        let mods_text = gameplay_mods_text(state, player);
        cache.prewarm_text(
            fonts,
            "miso",
            mods_text.as_ref(),
            Some(DISPLAY_MODS_WRAP_WIDTH_PX as i32),
        );

        let mc_font_name = zmod_small_combo_font(profile.combo_font);
        let segs = &state.measure_counter_segments[player];
        let multiplier = profile.measure_counter.multiplier();
        for (seg_ix, seg) in segs.iter().copied().enumerate() {
            let scaled_len = (((seg.end - seg.start) as f32) * multiplier)
                .floor()
                .max(0.0) as i32;
            max_measure_len = max_measure_len.max(scaled_len);
            if !seg.is_break {
                let (broken_end, _) = zmod_broken_run_end(segs, seg_ix);
                max_measure_len = max_measure_len.max((broken_end - seg.start) as i32);
            }
        }
        let prewarm_measure_len = max_measure_len.min(MEASURE_PREWARM_CAP);
        for total in 1..=prewarm_measure_len {
            prewarm_i32(cache, mc_font_name, total);
            let break_text = cached_paren_i32(total);
            cache.prewarm_text(fonts, mc_font_name, break_text.as_ref(), None);
            for curr in 1..=total {
                prewarm_ratio(cache, mc_font_name, curr, total);
            }
        }
        if max_measure_len > prewarm_measure_len {
            prewarm_i32(cache, mc_font_name, max_measure_len);
            let break_text = cached_paren_i32(max_measure_len);
            cache.prewarm_text(fonts, mc_font_name, break_text.as_ref(), None);
            prewarm_ratio(cache, mc_font_name, 1, max_measure_len);
            prewarm_ratio(cache, mc_font_name, max_measure_len, max_measure_len);
        }
        for second in 0..=music_end_seconds.min(RUN_TIMER_PREWARM_CAP_S) {
            prewarm_timer(cache, mc_font_name, second, 60, false);
            prewarm_timer(cache, mc_font_name, second, 59, true);
        }
        prewarm_timer(cache, mc_font_name, music_end_seconds, 60, false);
        prewarm_timer(cache, mc_font_name, music_end_seconds, 59, true);
        if profile.measure_counter != crate::game::profile::MeasureCounter::None {
            let countdown_max = max_measure_len.clamp(16, MEASURE_PREWARM_CAP);
            for value in 0..=countdown_max {
                prewarm_i32(cache, mc_font_name, value);
            }
            prewarm_i32(cache, mc_font_name, max_measure_len.max(16));
        }
        if zmod_indicator_mode(profile) != profile::MiniIndicator::None {
            for &value in &[0.0, 50.0, 89.0, 95.0, 100.0] {
                prewarm_percent(cache, mc_font_name, value);
                prewarm_signed_percent(cache, mc_font_name, value, true);
                prewarm_signed_percent(cache, mc_font_name, value, false);
            }
            prewarm_percent(
                cache,
                mc_font_name,
                state.mini_indicator_target_score_percent[player],
            );
            prewarm_percent(
                cache,
                mc_font_name,
                state.mini_indicator_rival_score_percent[player],
            );
            prewarm_neg_u32(cache, mc_font_name, 0);
            prewarm_neg_u32(cache, mc_font_name, max_combo.min(COMBO_PREWARM_CAP));
            prewarm_neg_u32(cache, mc_font_name, max_combo);
        }
        if profile.error_ms_display {
            prewarm_offset(cache, 0.0);
        }
    }

    cache.prewarm_text(fonts, "game", "Early", None);
    cache.prewarm_text(fonts, "game", "Late", None);
    cache.prewarm_text(fonts, "wendy", "EARLY", None);
    cache.prewarm_text(fonts, "wendy", "LATE", None);
}

#[inline(always)]
fn zmod_combo_quint_active(state: &State, player_idx: usize, profile: &profile::Profile) -> bool {
    if !profile.show_fa_plus_window || player_idx >= state.num_players {
        return false;
    }
    let counts = state.live_window_counts[player_idx];
    counts.w0 > 0
        && counts.w1 == 0
        && counts.w2 == 0
        && counts.w3 == 0
        && counts.w4 == 0
        && counts.w5 == 0
        && counts.miss == 0
}

#[inline(always)]
fn zmod_combo_glow_pair(grade: JudgeGrade, quint: bool) -> ([f32; 4], [f32; 4]) {
    if quint && matches!(grade, JudgeGrade::Fantastic) {
        return (color::rgba_hex("#F7C0FE"), color::rgba_hex("#E928FF"));
    }
    match grade {
        JudgeGrade::Fantastic => (color::rgba_hex("#C8FFFF"), color::rgba_hex("#6BF0FF")),
        JudgeGrade::Excellent => (color::rgba_hex("#FDFFC9"), color::rgba_hex("#FDDB85")),
        JudgeGrade::Great => (color::rgba_hex("#C9FFC9"), color::rgba_hex("#94FEC1")),
        _ => ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]),
    }
}

#[inline(always)]
fn zmod_combo_solid_color(grade: JudgeGrade, quint: bool) -> [f32; 4] {
    if quint && matches!(grade, JudgeGrade::Fantastic) {
        return color::rgba_hex("#E928FF");
    }
    match grade {
        JudgeGrade::Fantastic => color::rgba_hex("#21CCE8"),
        JudgeGrade::Excellent => color::rgba_hex("#E29C18"),
        JudgeGrade::Great => color::rgba_hex("#66C955"),
        _ => [1.0, 1.0, 1.0, 1.0],
    }
}

#[inline(always)]
fn zmod_combo_glow_color(color1: [f32; 4], color2: [f32; 4], elapsed: f32) -> [f32; 4] {
    let effect_period = 0.8_f32;
    let through = (elapsed / effect_period).fract();
    let anim_t = ((through * 2.0 * std::f32::consts::PI).sin() + 1.0) * 0.5;
    [
        color1[0] + (color2[0] - color1[0]) * anim_t,
        color1[1] + (color2[1] - color1[1]) * anim_t,
        color1[2] + (color2[2] - color1[2]) * anim_t,
        1.0,
    ]
}

#[inline(always)]
fn zmod_combo_rainbow_color(elapsed: f32, scroll: bool, combo: u32) -> [f32; 4] {
    let speed = if scroll { 0.45 } else { 0.35 };
    let offset = if scroll { combo as f32 * 0.013 } else { 0.0 };
    let hue = (elapsed * speed + offset).fract();
    let h6 = hue * 6.0;
    let i = h6.floor() as i32;
    let f = h6 - i as f32;
    let q = 1.0 - f;
    match i.rem_euclid(6) {
        0 => [1.0, f, 0.0, 1.0],
        1 => [q, 1.0, 0.0, 1.0],
        2 => [0.0, 1.0, f, 1.0],
        3 => [0.0, q, 1.0, 1.0],
        4 => [f, 0.0, 1.0, 1.0],
        _ => [1.0, 0.0, q, 1.0],
    }
}

#[inline(always)]
fn scoring_count(
    scoring_counts: &crate::game::judgment::JudgeCounts,
    provisional_counts: &crate::game::judgment::JudgeCounts,
    grade: JudgeGrade,
) -> u32 {
    let grade_ix = crate::game::judgment::judge_grade_ix(grade);
    scoring_counts[grade_ix].saturating_add(provisional_counts[grade_ix])
}

#[inline(always)]
fn actual_grade_points_with_provisional(
    actual_dp: i32,
    provisional_counts: &crate::game::judgment::JudgeCounts,
) -> i32 {
    actual_dp
        .saturating_add(
            crate::game::judgment::calculate_itg_grade_points_from_counts(
                provisional_counts,
                0,
                0,
                0,
            ),
        )
        .max(0)
}

#[inline(always)]
fn add_provisional_early_bad_counts_to_ex_score(
    mut data: crate::game::judgment::ExScoreData,
    provisional_counts: &crate::game::judgment::JudgeCounts,
) -> crate::game::judgment::ExScoreData {
    let decent = provisional_counts[crate::game::judgment::judge_grade_ix(JudgeGrade::Decent)];
    let way_off = provisional_counts[crate::game::judgment::judge_grade_ix(JudgeGrade::WayOff)];
    let miss = provisional_counts[crate::game::judgment::judge_grade_ix(JudgeGrade::Miss)];
    data.counts.w4 = data.counts.w4.saturating_add(decent);
    data.counts.w5 = data.counts.w5.saturating_add(way_off);
    data.counts.miss = data.counts.miss.saturating_add(miss);
    data.counts_10ms.w4 = data.counts_10ms.w4.saturating_add(decent);
    data.counts_10ms.w5 = data.counts_10ms.w5.saturating_add(way_off);
    data.counts_10ms.miss = data.counts_10ms.miss.saturating_add(miss);
    data
}

/// Compute predictive kept/lost/pace percentages for ITG scoring.
fn predictive_itg_percents(
    current_possible_dp: i32,
    possible_dp: i32,
    actual_dp: i32,
) -> (f64, f64, f64) {
    let dp_lost = current_possible_dp.saturating_sub(actual_dp);
    let kept_dp = possible_dp.saturating_sub(dp_lost).max(0);
    let kept = ((f64::from(kept_dp) / f64::from(possible_dp)) * 10000.0).floor() / 100.0;
    let lost = (100.0 - kept).max(0.0);
    let pace = if current_possible_dp > 0 {
        ((f64::from(actual_dp) / f64::from(current_possible_dp)) * 10000.0).floor() / 100.0
    } else {
        0.0
    };
    (kept, lost, pace)
}

#[derive(Clone, Copy, Debug, Default)]
struct MiniIndicatorProgress {
    kept_percent: f64,
    lost_percent: f64,
    pace_percent: f64,
    current_possible_dp: i32,
    possible_dp: i32,
    actual_dp: i32,
    white_count: u32,
    w2: u32,
    w3: u32,
    w4: u32,
    w5: u32,
    miss: u32,
    let_go: u32,
    mines_hit: u32,
    judged_any: bool,
}

fn zmod_mini_indicator_progress(
    state: &State,
    p: &PlayerRuntime,
    player_idx: usize,
    score_type: profile::MiniIndicatorScoreType,
) -> MiniIndicatorProgress {
    let provisional = &p.provisional_scoring_counts;
    let w1 = scoring_count(&p.scoring_counts, provisional, JudgeGrade::Fantastic);
    let w2 = scoring_count(&p.scoring_counts, provisional, JudgeGrade::Excellent);
    let w3 = scoring_count(&p.scoring_counts, provisional, JudgeGrade::Great);
    let w4 = scoring_count(&p.scoring_counts, provisional, JudgeGrade::Decent);
    let w5 = scoring_count(&p.scoring_counts, provisional, JudgeGrade::WayOff);
    let miss = scoring_count(&p.scoring_counts, provisional, JudgeGrade::Miss);

    let let_go = p
        .holds_let_go_for_score
        .saturating_add(p.rolls_let_go_for_score);
    let mines_hit = p.mines_hit_for_score;
    let tap_rows = w1
        .saturating_add(w2)
        .saturating_add(w3)
        .saturating_add(w4)
        .saturating_add(w5)
        .saturating_add(miss);
    let resolved_holds = p
        .holds_held_for_score
        .saturating_add(p.holds_let_go_for_score);
    let resolved_rolls = p
        .rolls_held_for_score
        .saturating_add(p.rolls_let_go_for_score);
    let current_possible_dp = (tap_rows
        .saturating_add(resolved_holds)
        .saturating_add(resolved_rolls) as i32)
        .saturating_mul(HOLD_SCORE_HELD);

    let possible_dp = state.possible_grade_points[player_idx].max(1);
    let actual_dp = actual_grade_points_with_provisional(p.earned_grade_points, provisional);

    // Compute predictive percents for the active score type.
    let (kept_percent, lost_percent, pace_percent, white_count) = match score_type {
        profile::MiniIndicatorScoreType::Itg => {
            let (kept, lost, pace) =
                predictive_itg_percents(current_possible_dp, possible_dp, actual_dp);
            (kept, lost, pace, 0)
        }
        profile::MiniIndicatorScoreType::Ex | profile::MiniIndicatorScoreType::HardEx => {
            let score = add_provisional_early_bad_counts_to_ex_score(
                crate::game::gameplay::display_scored_ex_score_data(state, player_idx),
                provisional,
            );
            let white_count = score.counts.w1;
            if score_type == profile::MiniIndicatorScoreType::Ex {
                let (kept, lost, pace) =
                    crate::game::judgment::predictive_ex_score_percents(&score);
                (kept, lost, pace, white_count)
            } else {
                let (kept, lost, pace) =
                    crate::game::judgment::predictive_hard_ex_score_percents(&score);
                (kept, lost, pace, white_count)
            }
        }
    };

    let judged_any = tap_rows > 0 || let_go > 0 || mines_hit > 0 || p.is_failing || p.life <= 0.0;
    MiniIndicatorProgress {
        kept_percent,
        lost_percent,
        pace_percent,
        current_possible_dp,
        possible_dp,
        actual_dp,
        white_count,
        w2,
        w3,
        w4,
        w5,
        miss,
        let_go,
        mines_hit,
        judged_any,
    }
}

#[inline(always)]
fn zmod_subtractive_counter_state(
    progress: &MiniIndicatorProgress,
    score_type: profile::MiniIndicatorScoreType,
) -> (u32, bool) {
    let forced_percent = progress.w3 > 0
        || progress.w4 > 0
        || progress.w5 > 0
        || progress.miss > 0
        || progress.let_go > 0
        || progress.mines_hit > 0;
    match score_type {
        profile::MiniIndicatorScoreType::Itg => (progress.w2, forced_percent || progress.w2 > 10),
        profile::MiniIndicatorScoreType::Ex | profile::MiniIndicatorScoreType::HardEx => (
            progress.white_count,
            forced_percent || progress.w2 > 0 || progress.white_count > 10,
        ),
    }
}

#[inline(always)]
fn zmod_indicator_mode(profile: &profile::Profile) -> profile::MiniIndicator {
    if profile.mini_indicator != profile::MiniIndicator::None {
        return profile.mini_indicator;
    }
    if profile.subtractive_scoring {
        profile::MiniIndicator::SubtractiveScoring
    } else if profile.pacemaker {
        profile::MiniIndicator::Pacemaker
    } else {
        profile::MiniIndicator::None
    }
}

#[inline(always)]
fn zmod_indicator_default_color(score_percent: f64) -> [f32; 4] {
    if score_percent >= 96.0 {
        color::JUDGMENT_RGBA[0] // Fantastic
    } else if score_percent >= 89.0 {
        color::JUDGMENT_RGBA[1] // Excellent
    } else if score_percent >= 80.0 {
        color::JUDGMENT_RGBA[2] // Great
    } else if score_percent >= 68.0 {
        color::JUDGMENT_RGBA[3] // Decent
    } else {
        color::JUDGMENT_RGBA[5] // Miss
    }
}

#[inline(always)]
fn zmod_rival_color(pace: f64, rival_pace: f64) -> [f32; 4] {
    let r = (1.0 - (pace - rival_pace)).clamp(0.0, 1.0) as f32;
    let g = (0.5 - (rival_pace - pace)).clamp(0.0, 1.0) as f32;
    let b = (1.0 - (rival_pace - pace)).clamp(0.0, 1.0) as f32;
    [r, g, b, 1.0]
}

#[inline(always)]
fn zmod_pacemaker_color(pace: f64, rival_pace: f64) -> [f32; 4] {
    let r = (1.0 - (pace - rival_pace) / 100.0).clamp(0.0, 1.0) as f32;
    let g = (0.5 - (rival_pace - pace) / 100.0).clamp(0.0, 1.0) as f32;
    let b = (1.0 - (rival_pace - pace) / 100.0).clamp(0.0, 1.0) as f32;
    [r, g, b, 1.0]
}

fn zmod_stream_prog_completion(state: &State, player_idx: usize) -> Option<f64> {
    let total_stream = state.mini_indicator_total_stream_measures[player_idx] as f64;
    if total_stream <= 0.0 {
        return None;
    }
    let segs = &state.mini_indicator_stream_segments[player_idx];
    if segs.is_empty() {
        return None;
    }

    let beat_floor = state.current_beat_visible[player_idx].floor();
    if !beat_floor.is_finite() {
        return Some(0.0);
    }
    let upper_beat = (beat_floor as i32).saturating_add(1).max(0);
    if upper_beat <= 0 {
        return Some(0.0);
    }
    let mut completed_stream_beats: i64 = 0;
    for seg in segs {
        let start_beat = (seg.start as i32).saturating_mul(4);
        if start_beat >= upper_beat {
            break;
        }
        if seg.is_break {
            continue;
        }
        let end_beat = (seg.end as i32).saturating_mul(4);
        let lo = start_beat.max(0);
        let hi = upper_beat.min(end_beat);
        if hi > lo {
            completed_stream_beats += i64::from(hi - lo);
        }
    }
    let completed_stream_measures = (completed_stream_beats as f64) / 4.0;
    Some((completed_stream_measures / total_stream).clamp(0.0, 1.0))
}

fn zmod_mini_indicator_text(
    state: &State,
    p: &PlayerRuntime,
    profile: &profile::Profile,
    player_idx: usize,
) -> Option<(Arc<str>, [f32; 4])> {
    let mode = zmod_indicator_mode(profile);
    if mode == profile::MiniIndicator::None {
        return None;
    }

    let progress =
        zmod_mini_indicator_progress(state, p, player_idx, profile.mini_indicator_score_type);
    if !progress.judged_any {
        return None;
    }

    match mode {
        profile::MiniIndicator::SubtractiveScoring => {
            let (count, entered_percent_mode) =
                zmod_subtractive_counter_state(&progress, profile.mini_indicator_score_type);
            if !(entered_percent_mode || p.is_failing || p.life <= 0.0) && count > 0 {
                return Some((cached_neg_int_u32(count), color::rgba_hex("#ff55cc")));
            }

            let pcts = &progress;
            let score = pcts.kept_percent.clamp(0.0, 100.0);
            Some((
                cached_signed_percent2_f64(pcts.lost_percent.clamp(0.0, 100.0), true),
                zmod_indicator_default_color(score),
            ))
        }
        profile::MiniIndicator::PredictiveScoring => {
            let score = progress.kept_percent.clamp(0.0, 100.0);
            Some((
                cached_percent2_f64(score),
                zmod_indicator_default_color(score),
            ))
        }
        profile::MiniIndicator::PaceScoring => {
            let pace = progress.pace_percent.clamp(0.0, 100.0);
            Some((
                cached_percent2_f64(pace),
                zmod_indicator_default_color(pace),
            ))
        }
        profile::MiniIndicator::RivalScoring => {
            let possible = f64::from(progress.possible_dp.max(1));
            let current_possible = f64::from(progress.current_possible_dp.max(0));
            let actual = f64::from(progress.actual_dp.max(0));
            let pace = ((actual / possible) * 10000.0).floor() / 100.0;
            let rival_score =
                state.mini_indicator_rival_score_percent[player_idx].clamp(0.0, 100.0);
            let rival_pace =
                ((current_possible / possible) * 10000.0 * rival_score).floor() / 10000.0;
            let diff = (pace - rival_pace).abs();
            let text = cached_signed_percent2_f64(diff, pace < rival_pace);
            Some((text, zmod_rival_color(pace, rival_pace)))
        }
        profile::MiniIndicator::Pacemaker => {
            let possible = f64::from(progress.possible_dp.max(1));
            let current_possible = f64::from(progress.current_possible_dp.max(0));
            let actual = f64::from(progress.actual_dp.max(0));
            let pace = (actual / possible * 10000.0).floor();
            let target_ratio =
                (state.mini_indicator_target_score_percent[player_idx] / 100.0).clamp(0.0, 1.0);
            let rival_pace =
                ((current_possible / possible) * 1_000_000.0 * target_ratio).floor() / 100.0;

            let text = if pace < rival_pace {
                let diff = ((rival_pace - pace).floor() / 100.0).max(0.0);
                cached_signed_percent2_f64(diff, true)
            } else {
                let diff = ((pace - rival_pace).floor() / 100.0).max(0.0);
                cached_signed_percent2_f64(diff, false)
            };
            Some((text, zmod_pacemaker_color(pace, rival_pace)))
        }
        profile::MiniIndicator::StreamProg => {
            let completion = zmod_stream_prog_completion(state, player_idx)?;
            let rgba = if completion >= 0.9 {
                [
                    0.0,
                    1.0,
                    ((completion - 0.9) * 10.0).clamp(0.0, 1.0) as f32,
                    1.0,
                ]
            } else if completion >= 0.5 {
                [
                    ((0.9 - completion) * 10.0 / 4.0).clamp(0.0, 1.0) as f32,
                    1.0,
                    0.0,
                    1.0,
                ]
            } else {
                [
                    1.0,
                    ((completion - 0.2) * 10.0 / 3.0).clamp(0.0, 1.0) as f32,
                    0.0,
                    1.0,
                ]
            };
            Some((
                cached_percent2_f64((completion * 100.0).clamp(0.0, 100.0)),
                rgba,
            ))
        }
        profile::MiniIndicator::None => None,
    }
}

#[inline(always)]
fn rage_frustum(l: f32, r: f32, b: f32, t: f32, zn: f32, zf: f32) -> Matrix4 {
    let a = (r + l) / (r - l);
    let bb = (t + b) / (t - b);
    let c = -(zf + zn) / (zf - zn);
    let d = -(2.0 * zf * zn) / (zf - zn);
    // Match ITGmania's RageDisplay::GetFrustumMatrix (OpenGL-style frustum matrix).
    //
    // Note: `glam::Mat4::from_cols_array` takes elements in column-major order.
    Matrix4::from_cols_array(&[
        2.0 * zn / (r - l),
        0.0,
        0.0,
        0.0,
        0.0,
        2.0 * zn / (t - b),
        0.0,
        0.0,
        a,
        bb,
        c,
        -1.0,
        0.0,
        0.0,
        d,
        0.0,
    ])
}

fn notefield_view_proj(
    screen_w: f32,
    screen_h: f32,
    playfield_center_x: f32,
    center_y: f32,
    tilt: f32,
    skew: f32,
    reverse: bool,
) -> Option<Matrix4> {
    if !screen_w.is_finite() || !screen_h.is_finite() || screen_w <= 0.0 || screen_h <= 0.0 {
        return None;
    }

    let half_w = 0.5 * screen_w;
    let half_h = 0.5 * screen_h;

    // ITGmania: Player::PushPlayerMatrix -> LoadMenuPerspective(45, w, h, vanish_x, center_y)
    let fov_deg = 45.0_f32;
    let theta = (0.5 * fov_deg).to_radians();
    let tan_theta = theta.tan();
    if !tan_theta.is_finite() || tan_theta.abs() < 1e-6 {
        return None;
    }
    let dist = half_w / tan_theta;
    if !dist.is_finite() || dist <= 0.0 {
        return None;
    }

    let vanish_x = sm_scale(skew, 0.1, 1.0, playfield_center_x, half_w);
    let vanish_y = center_y;

    let near = 1.0_f32;
    let far = dist + 1000.0_f32;

    // Match RageDisplay::LoadMenuPerspective exactly (ITGmania).
    let mut vp_x = sm_scale(vanish_x, 0.0, screen_w, screen_w, 0.0);
    let mut vp_y = sm_scale(vanish_y, 0.0, screen_h, screen_h, 0.0);
    vp_x -= half_w;
    vp_y -= half_h;
    let l = (vp_x - half_w) / dist;
    let r = (vp_x + half_w) / dist;
    let b = (vp_y + half_h) / dist;
    let t = (vp_y - half_h) / dist;
    let proj = rage_frustum(l, r, b, t, near, far);

    let eye = Vector3::new(-vp_x + half_w, -vp_y + half_h, dist);
    let at = Vector3::new(-vp_x + half_w, -vp_y + half_h, 0.0);
    let view = Matrix4::look_at_rh(eye, at, Vector3::Y);

    // ITGmania: PlayerNoteFieldPositioner applies tilt/zoom/y_offset on the NoteField actor.
    let reverse_mult = if reverse { -1.0 } else { 1.0 };
    let tilt = tilt.clamp(-1.0, 1.0);
    let tilt_deg = (-30.0 * tilt) * reverse_mult;
    let tilt_abs = tilt.abs();
    let tilt_scale = 1.0 - 0.1 * tilt_abs;
    let y_offset_screen = if tilt > 0.0 {
        -45.0 * tilt
    } else {
        20.0 * tilt
    } * reverse_mult;
    // Screen y-down to world y-up.
    let y_offset_world = -y_offset_screen;

    let pivot_x = playfield_center_x - half_w;
    let pivot_y = half_h - center_y;
    // Convert our world coords (centered, y-up) back into the SM-style screen
    // coords (top-left, y-down) expected by the menu perspective camera.
    let world_to_screen = Matrix4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, //
        0.0, -1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        half_w, half_h, 0.0, 1.0,
    ]);
    let field = Matrix4::from_translation(Vector3::new(0.0, y_offset_world, 0.0))
        * Matrix4::from_translation(Vector3::new(pivot_x, pivot_y, 0.0))
        * Matrix4::from_rotation_x(tilt_deg.to_radians())
        * Matrix4::from_scale(Vector3::new(tilt_scale, tilt_scale, 1.0))
        * Matrix4::from_translation(Vector3::new(-pivot_x, -pivot_y, 0.0));

    Some((proj * view) * world_to_screen * field)
}

#[inline(always)]
fn hold_head_render_flags(
    active_state: Option<&ActiveHold>,
    current_beat: f32,
    note_beat: f32,
) -> (bool, bool) {
    let reached_receptor = current_beat >= note_beat;
    let engaged = reached_receptor && active_state.map(active_hold_is_engaged).unwrap_or(false);
    // ITG keeps rolls on their active art for the full initiated hold span,
    // even between taps; regular holds only stay active while the lane is held.
    let use_active = engaged
        && active_state
            .map(|h| matches!(h.note_type, NoteType::Roll) || h.is_pressed)
            .unwrap_or(false);
    (engaged, use_active)
}

#[inline(always)]
fn let_go_head_beat(note_beat: f32, end_beat: f32, last_held_beat: f32, visible_beat: f32) -> f32 {
    // ITG updates and renders from one song position. deadsync keeps separate
    // gameplay and display clocks, so a dropped hold head must never render
    // ahead of the visible beat or it can jump above the receptor.
    last_held_beat
        .clamp(note_beat, end_beat)
        .min(visible_beat.max(note_beat))
}

#[inline(always)]
fn song_time_ns_to_seconds(time_ns: SongTimeNs) -> f32 {
    (time_ns as f64 * 1.0e-9) as f32
}

#[inline(always)]
fn song_time_ns_delta_seconds(lhs: SongTimeNs, rhs: SongTimeNs) -> f32 {
    ((lhs as i128 - rhs as i128) as f64 * 1.0e-9) as f32
}

#[inline(always)]
fn lane_window_bounds_by_note_row(
    note_indices: &[usize],
    notes: &[Note],
    min_row: i32,
    max_row: i32,
) -> (usize, usize) {
    if max_row < 0 {
        return (0, 0);
    }
    let min_row = min_row.max(0) as usize;
    let max_row = max_row as usize;
    (
        note_indices.partition_point(|&note_index| notes[note_index].row_index < min_row),
        note_indices.partition_point(|&note_index| notes[note_index].row_index <= max_row),
    )
}

#[inline(always)]
fn lane_hold_window_bounds_by_note_row(
    hold_indices: &[usize],
    notes: &[Note],
    min_row: i32,
    max_row: i32,
) -> (usize, usize) {
    let (mut start, end) = lane_window_bounds_by_note_row(hold_indices, notes, min_row, max_row);
    let min_row = min_row.max(0) as usize;
    while start > 0 {
        let prev_note_index = hold_indices[start - 1];
        let prev_end_row = notes[prev_note_index]
            .hold
            .as_ref()
            .map_or(notes[prev_note_index].row_index, |hold| {
                beat_to_note_row(hold.end_beat).max(0) as usize
            });
        if prev_end_row < min_row {
            break;
        }
        start -= 1;
    }
    (start, end)
}

#[inline(always)]
fn find_first_displayed_beat(
    current_beat: f32,
    draw_distance_after_targets: f32,
    note_count_stats: &[NoteCountStat],
    mut y_offset_for_beat: impl FnMut(f32) -> f32,
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance_after_targets.is_finite() {
        return None;
    }
    let mut high = current_beat.max(0.0);
    let has_cache = !note_count_stats.is_empty();
    let mut low = if has_cache { 0.0 } else { high - 4.0 };
    let mut first = low;
    for _ in 0..24 {
        let mid = (low + high) * 0.5;
        if y_offset_for_beat(mid) < -draw_distance_after_targets
            || (has_cache
                && note_count_range(note_count_stats, mid, current_beat) > MAX_NOTES_AFTER)
        {
            first = mid;
            low = mid;
        } else {
            high = mid;
        }
    }
    Some(first)
}

#[inline(always)]
fn note_count_range(stats: &[NoteCountStat], low: f32, high: f32) -> usize {
    let low = note_count_at(stats, low);
    let high = note_count_at(stats, high);
    high.notes_upper.saturating_sub(low.notes_lower)
}

#[inline(always)]
fn note_count_at(stats: &[NoteCountStat], beat: f32) -> NoteCountStat {
    let ix = stats
        .partition_point(|stat| stat.beat <= beat)
        .saturating_sub(1);
    stats[ix]
}

#[inline(always)]
fn find_last_displayed_beat(
    current_beat: f32,
    draw_distance_before_targets: f32,
    displayed_speed_percent: f32,
    boomerang: bool,
    mut y_offset_for_beat: impl FnMut(f32) -> (f32, bool),
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance_before_targets.is_finite() {
        return None;
    }
    let mut search_distance = 10.0;
    let mut last = current_beat + search_distance;
    for _ in 0..20 {
        let (y_offset, before_peak) = y_offset_for_beat(last);
        if boomerang && !before_peak {
            last += search_distance;
        } else if y_offset > draw_distance_before_targets {
            last -= search_distance;
        } else {
            last += search_distance;
        }
        search_distance *= 0.5;
    }
    if displayed_speed_percent < 0.75 {
        last = last.min(current_beat + 16.0);
    }
    Some(last)
}

#[inline(always)]
fn for_each_visible_note_index(
    note_indices: &[usize],
    notes: &[Note],
    visible_row_range: Option<(i32, i32)>,
    mut visit: impl FnMut(usize),
) {
    if let Some((min_row, max_row)) = visible_row_range {
        let (start, end) = lane_window_bounds_by_note_row(note_indices, notes, min_row, max_row);
        for &note_index in &note_indices[start..end] {
            visit(note_index);
        }
        return;
    }
    for &note_index in note_indices {
        visit(note_index);
    }
}

#[inline(always)]
fn for_each_visible_hold_index(
    hold_indices: &[usize],
    notes: &[Note],
    visible_row_range: Option<(i32, i32)>,
    mut visit: impl FnMut(usize),
) {
    if let Some((min_row, max_row)) = visible_row_range {
        let (start, end) =
            lane_hold_window_bounds_by_note_row(hold_indices, notes, min_row, max_row);
        for &note_index in &hold_indices[start..end] {
            visit(note_index);
        }
        return;
    }
    for &note_index in hold_indices {
        visit(note_index);
    }
}

#[inline(always)]
fn hold_overlaps_visible_window(
    note_index: usize,
    notes: &[Note],
    visible_row_range: Option<(i32, i32)>,
) -> bool {
    if let Some((min_row, max_row)) = visible_row_range {
        let hold_end_row = notes[note_index]
            .hold
            .as_ref()
            .map_or(notes[note_index].row_index, |hold| {
                beat_to_note_row(hold.end_beat).max(0) as usize
            });
        return max_row >= 0
            && hold_end_row >= min_row.max(0) as usize
            && notes[note_index].row_index <= max_row as usize;
    }
    true
}

#[inline(always)]
fn song_lua_hides_note_window(
    windows: &[SongLuaNoteHideWindow],
    local_col: usize,
    beat: f32,
) -> bool {
    const EPS: f32 = 1.0e-4;
    windows.iter().any(|window| {
        window.column == local_col
            && beat + EPS >= window.start_beat
            && beat <= window.end_beat + EPS
    })
}

#[inline(always)]
fn song_lua_hides_note(state: &State, player: usize, local_col: usize, beat: f32) -> bool {
    song_lua_hides_note_window(&state.song_lua_note_hides[player], local_col, beat)
}

#[inline(always)]
const fn mine_hides_after_resolution(mine_result: Option<MineResult>) -> bool {
    // ITG hides mines once they have received any final mine judgment, not
    // only after a hit explosion.
    mine_result.is_some()
}

pub fn build_bundles(
    state: &State,
    profile: &profile::Profile,
    placement: FieldPlacement,
    play_style: profile::PlayStyle,
    center_1player_notefield: bool,
    capture_requests: ProxyCaptureRequests,
    view: ViewOverride,
    mut actors: &mut Vec<Actor>,
    mut hud_actors: &mut Vec<Actor>,
) -> BuiltNotefield {
    actors.clear();
    hud_actors.clear();
    let hold_judgment_texture = resolved_hold_judgment_texture(profile);

    // --- Playfield Positioning (1:1 with Simply Love) ---
    // In P2-only single-player, we still have a single player runtime (index 0),
    // but need to place the notefield on the P2 side of the screen.
    let player_idx = if state.num_players == 1 {
        0
    } else {
        match placement {
            FieldPlacement::P1 => 0,
            FieldPlacement::P2 => 1,
        }
    };
    if player_idx >= state.num_players {
        return BuiltNotefield::empty(screen_center_x());
    }
    // Use the cached field_zoom from gameplay state so visual layout and
    // scroll math share the exact same scaling as gameplay. Practice edit
    // mode overrides this to match ScreenEdit's half-scale edit field.
    let field_zoom = view.field_zoom.unwrap_or(state.field_zoom[player_idx]);
    let draw_distance_before_targets = state.draw_distance_before_targets[player_idx];
    let draw_distance_after_targets = state.draw_distance_after_targets[player_idx];
    let scroll_speed = view
        .scroll_speed
        .unwrap_or_else(|| effective_scroll_speed_for_player(state, player_idx));
    let col_start = player_idx * state.cols_per_player;
    let col_end = (col_start + state.cols_per_player)
        .min(state.num_cols)
        .min(MAX_COLS);
    let num_cols = col_end.saturating_sub(col_start);
    if num_cols == 0 {
        return BuiltNotefield::empty(screen_center_x());
    }
    let error_bar_mask = {
        let mut mask = profile.error_bar_active_mask;
        if mask.is_empty() {
            mask = profile::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text);
        }
        mask
    };
    let measure_line_extra = if view.edit_beat_bars {
        72
    } else {
        match profile.measure_lines {
            crate::game::profile::MeasureLines::Off => 0,
            crate::game::profile::MeasureLines::Measure => 18,
            crate::game::profile::MeasureLines::Quarter => 30,
            crate::game::profile::MeasureLines::Eighth => 42,
        }
    };
    let actor_cap = (num_cols * 10).max(28)
        + measure_line_extra
        + if profile.column_cues { num_cols + 4 } else { 0 }
        + if !error_bar_mask.is_empty() { 18 } else { 0 };
    let hud_cap = 8
        + if profile.column_cues { 1 } else { 0 }
        + if profile.hide_combo { 0 } else { 2 }
        + if error_bar_mask.contains(profile::ErrorBarMask::TEXT) {
            1
        } else {
            0
        };
    actors.reserve(actor_cap);
    hud_actors.reserve(hud_cap);
    let p = &state.players[player_idx];
    let mut model_cache = state.notefield_model_cache[player_idx].borrow_mut();

    // NoteFieldOffsetX is stored as a non-negative magnitude; for a single P1-style field,
    // apply the player-side sign flip used by Simply Love (P1=-, P2=+).
    let offset_sign = match placement {
        FieldPlacement::P1 => -1.0,
        FieldPlacement::P2 => 1.0,
    };
    let notefield_offset_x = offset_sign * (profile.note_field_offset_x.clamp(0, 50) as f32);
    let notefield_offset_y = profile.note_field_offset_y.clamp(-50, 50) as f32;
    let judgment_extra_x = profile
        .judgment_offset_x
        .clamp(profile::HUD_OFFSET_MIN, profile::HUD_OFFSET_MAX) as f32;
    let judgment_extra_y = profile
        .judgment_offset_y
        .clamp(profile::HUD_OFFSET_MIN, profile::HUD_OFFSET_MAX) as f32;
    let combo_extra_x = profile
        .combo_offset_x
        .clamp(profile::HUD_OFFSET_MIN, profile::HUD_OFFSET_MAX) as f32;
    let combo_extra_y = profile
        .combo_offset_y
        .clamp(profile::HUD_OFFSET_MIN, profile::HUD_OFFSET_MAX) as f32;
    let error_bar_extra_x = profile
        .error_bar_offset_x
        .clamp(profile::HUD_OFFSET_MIN, profile::HUD_OFFSET_MAX) as f32;
    let error_bar_extra_y = profile
        .error_bar_offset_y
        .clamp(profile::HUD_OFFSET_MIN, profile::HUD_OFFSET_MAX) as f32;
    let logical_screen_width = screen_width();
    let clamped_width = logical_screen_width.clamp(640.0, 854.0);
    let centered_one_side = state.num_players == 1
        && play_style == profile::PlayStyle::Single
        && center_1player_notefield;
    let centered_both_sides = state.num_players == 1 && play_style == profile::PlayStyle::Double;
    let base_playfield_center_x = if state.num_players == 2 {
        match placement {
            FieldPlacement::P1 => screen_center_x() - (clamped_width * 0.25),
            FieldPlacement::P2 => screen_center_x() + (clamped_width * 0.25),
        }
    } else if centered_both_sides || centered_one_side {
        screen_center_x()
    } else {
        match placement {
            FieldPlacement::P1 => screen_center_x() - (clamped_width * 0.25),
            FieldPlacement::P2 => screen_center_x() + (clamped_width * 0.25),
        }
    };
    let playfield_center_x = base_playfield_center_x + notefield_offset_x;
    // Simply Love's GetNotefieldX helper reports base center for centered one-player fields,
    // ignoring NoteFieldOffsetX for layout decisions.
    let layout_center_x = if state.num_players == 1 && (centered_both_sides || centered_one_side) {
        screen_center_x()
    } else {
        playfield_center_x
    };
    let receptor_y_override = view.receptor_y.map(|y| y + notefield_offset_y);
    let receptor_y_normal = if let Some(y) = receptor_y_override {
        y
    } else if view.center_receptors_y {
        screen_center_y() + notefield_offset_y
    } else {
        screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER + notefield_offset_y
    };
    let receptor_y_reverse = if let Some(y) = receptor_y_override {
        y
    } else if view.center_receptors_y {
        screen_center_y() + notefield_offset_y
    } else {
        screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE + notefield_offset_y
    };
    let scroll = effective_scroll_effects_for_player(state, player_idx);
    let perspective = effective_perspective_effects_for_player(state, player_idx);
    let centered_percent = if view.receptor_y.is_some() || view.center_receptors_y {
        1.0
    } else {
        scroll.centered
    };
    let receptor_y_centered = receptor_y_override.unwrap_or(screen_center_y() + notefield_offset_y);
    let column_reverse_percent: [f32; MAX_COLS] = from_fn(|i| {
        if i >= num_cols {
            return 0.0;
        }
        scroll.reverse_percent_for_column(i, num_cols)
    });
    let column_dirs: [f32; MAX_COLS] = from_fn(|i| {
        if i >= num_cols {
            return 1.0;
        }
        state.column_scroll_dirs[col_start + i]
    });
    let column_receptor_ys: [f32; MAX_COLS] = from_fn(|i| {
        if i >= num_cols {
            return receptor_y_normal;
        }
        scroll_receptor_y(
            column_reverse_percent[i],
            centered_percent,
            receptor_y_normal,
            receptor_y_reverse,
            receptor_y_centered,
        )
    });

    let elapsed_screen = state.total_elapsed_in_screen;
    let accel = effective_accel_effects_for_player(state, player_idx);
    let visual = effective_visual_effects_for_player(state, player_idx);
    let appearance = effective_appearance_effects_for_player(state, player_idx);
    let visibility = effective_visibility_effects_for_player(state, player_idx);
    let mini_percent = effective_mini_percent_for_player(state, player_idx);
    let mini = effective_mini_value(profile, visual, mini_percent);
    let spacing_mult = effective_spacing_multiplier_for_player(state, player_idx);
    let reverse_scroll = state.reverse_scroll[player_idx];
    let hud_reverse = column_reverse_percent[0] >= 0.999_9;
    let judgment_y_base = hud_y(
        screen_center_y() - TAP_JUDGMENT_OFFSET_FROM_CENTER + notefield_offset_y,
        screen_center_y() + TAP_JUDGMENT_OFFSET_FROM_CENTER + notefield_offset_y,
        receptor_y_centered + 95.0,
        hud_reverse,
        centered_percent,
    );
    let combo_y_base = hud_y(
        screen_center_y() + COMBO_OFFSET_FROM_CENTER + notefield_offset_y,
        screen_center_y() - COMBO_OFFSET_FROM_CENTER + notefield_offset_y,
        receptor_y_centered + 155.0,
        hud_reverse,
        centered_percent,
    );
    let hud_layout = hud_layout_ys(
        profile,
        judgment_y_base,
        combo_y_base,
        hud_reverse,
        judgment_extra_y,
        combo_extra_y,
        error_bar_extra_y,
    );
    let judgment_y = hud_layout.judgment_y;
    let zmod_layout = hud_layout.zmod_layout;
    let judgment_x = playfield_center_x + judgment_extra_x;
    let combo_x = playfield_center_x + combo_extra_x;
    let mc_font_name = zmod_small_combo_font(profile.combo_font);
    let judgment_zoom_mod = judgment_actor_zoom(mini, profile.judgment_back)
        * hallway_judgment_zoom(perspective.tilt, perspective.skew);
    let combo_zoom_mod = combo_actor_zoom(mini);
    let effect_height = field_effect_height(perspective.tilt);
    let receptor_alpha = (1.0 - visibility.dark).clamp(0.0, 1.0);
    let blind_active = visibility.blind > f32::EPSILON;

    if let Some(ns) = &state.noteskin[player_idx] {
        let mine_ns = state.mine_noteskin[player_idx].as_deref().unwrap_or(ns);
        let receptor_ns = state.receptor_noteskin[player_idx].as_deref().unwrap_or(ns);
        let tap_explosion_ns = if profile.tap_explosion_noteskin_hidden() {
            None
        } else {
            state.tap_explosion_noteskin[player_idx]
                .as_deref()
                .or_else(|| state.noteskin[player_idx].as_deref())
        };
        let timing = &state.timing_players[player_idx];
        let target_arrow_px = TARGET_ARROW_PIXEL_SIZE * field_zoom;
        let scale_sprite =
            |size: [i32; 2]| -> [f32; 2] { scale_sprite_to_arrow(size, target_arrow_px) };
        let scale_mine_slot = |slot: &SpriteSlot| -> [f32; 2] {
            // ITG NoteDisplay::DrawTap uses SetPRZForActor zoom for TapMine and does not
            // normalize Def.Model mine meshes to an arrow texture target size. Preserve
            // native model geometry scale here; keep sprite mines on texture-size scaling.
            if let Some(model) = slot.model.as_ref() {
                let model_size = model.size();
                if model_size[0] > f32::EPSILON && model_size[1] > f32::EPSILON {
                    return [model_size[0] * field_zoom, model_size[1] * field_zoom];
                }
            }
            scale_sprite(slot.size())
        };
        let logical_slot_size = |slot: &SpriteSlot| -> [f32; 2] { slot.logical_size() };
        let scale_explosion = |logical_size: [f32; 2]| -> [f32; 2] {
            [logical_size[0] * field_zoom, logical_size[1] * field_zoom]
        };
        let scale_hold_explosion = |slot: &SpriteSlot| -> [f32; 2] {
            // Match ITG ghost arrow behavior: hold/roll explosions use actor asset size
            // (including double-res handling) instead of being normalized to arrow size.
            let logical = logical_slot_size(slot);
            [logical[0] * field_zoom, logical[1] * field_zoom]
        };
        let current_time_ns = state.current_music_time_visible_ns[player_idx];
        let current_time = song_time_ns_to_seconds(current_time_ns);
        let current_beat = state.current_beat_visible[player_idx];
        // The column swap for Step's hold-turn section is handled at the player bundle
        // level. Keep the actual note/receptor/ghost visuals on the normal noteskin
        // path here; applying an extra local Y turn breaks model-backed arrows and hit
        // effects.
        let note_rotation_y = 0.0_f32;
        let prefer_sprite_note_path = false;
        let flat_tap_face_rotation_y = 0.0_f32;
        let beat_push = beat_factor(current_beat);
        let mut col_offsets = [0.0_f32; MAX_COLS];
        for (i, col_offset) in col_offsets.iter_mut().take(num_cols).enumerate() {
            *col_offset = ns.column_xs[i] as f32 * spacing_mult * field_zoom;
        }
        let mut invert_distances = [0.0_f32; MAX_COLS];
        compute_invert_distances(&col_offsets[..num_cols], &mut invert_distances[..num_cols]);
        let mut tornado_bounds = [TornadoBounds::default(); MAX_COLS];
        compute_tornado_bounds(&col_offsets[..num_cols], &mut tornado_bounds[..num_cols]);
        // ITG NoteField currently advances NoteDisplay resources twice per frame for
        // the master field (and once per additional field), so model/tween time in
        // NoteDisplay actors runs faster than wall-clock elapsed.
        let note_display_time_scale = state.num_players as f32 + 1.0;
        // Precompute per-frame values used for converting beat/time to Y positions
        let display_speed_percent =
            timing.get_speed_multiplier_ns(state.current_beat_visible[player_idx], current_time_ns);
        let (rate, cmod_pps_opt, curr_disp_beat, beatmod_multiplier) = match scroll_speed {
            ScrollSpeedSetting::CMod(c_bpm) => {
                let pps = (c_bpm / 60.0) * ScrollSpeedSetting::ARROW_SPACING * field_zoom;
                let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
                    state.music_rate
                } else {
                    1.0
                };
                (rate, Some(pps), 0.0, 0.0)
            }
            ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                let curr_disp = timing.get_displayed_beat(state.current_beat_visible[player_idx]);
                let player_multiplier =
                    scroll_speed.beat_multiplier(state.scroll_reference_bpm, state.music_rate);
                let final_multiplier = player_multiplier * display_speed_percent;
                (1.0, None, curr_disp, final_multiplier)
            }
        };
        let travel_offset_for_time_ns = |note_time_ns: SongTimeNs| -> f32 {
            let pps_chart = cmod_pps_opt.expect("cmod pps computed");
            let time_diff_real = song_time_ns_delta_seconds(note_time_ns, current_time_ns) / rate;
            time_diff_real * pps_chart
        };
        let raw_travel_offset_for_beat = |beat: f32| -> f32 {
            match scroll_speed {
                ScrollSpeedSetting::CMod(_) => {
                    travel_offset_for_time_ns(timing.get_time_for_beat_ns(beat))
                }
                ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                    let note_disp_beat = timing.get_displayed_beat(beat);
                    (note_disp_beat - curr_disp_beat)
                        * ScrollSpeedSetting::ARROW_SPACING
                        * field_zoom
                        * beatmod_multiplier
                }
            }
        };
        let note_endpoint_beat = |note: &Note, use_hold_end: bool| -> f32 {
            if use_hold_end {
                note.hold.as_ref().map_or(note.beat, |hold| hold.end_beat)
            } else {
                note.beat
            }
        };
        let travel_offset_for_note = |note: &Note, use_hold_end: bool| {
            raw_travel_offset_for_beat(note_endpoint_beat(note, use_hold_end))
        };
        // ITGmania derives the drawable row span by probing ArrowEffects::GetYOffset
        // every frame; keep that as the primary note candidate window even when
        // no accel mods are active.
        let visible_row_range = {
            let first_beat_to_draw = find_first_displayed_beat(
                current_beat,
                draw_distance_after_targets,
                &state.note_count_stats[player_idx],
                |beat| {
                    apply_accel_y(
                        raw_travel_offset_for_beat(beat),
                        elapsed_screen,
                        current_beat,
                        effect_height,
                        accel,
                    )
                },
            );
            let last_beat_to_draw = find_last_displayed_beat(
                current_beat,
                draw_distance_before_targets,
                display_speed_percent,
                accel.boomerang > f32::EPSILON,
                |beat| {
                    apply_accel_y_with_peak(
                        raw_travel_offset_for_beat(beat),
                        elapsed_screen,
                        current_beat,
                        effect_height,
                        accel,
                    )
                },
            );
            first_beat_to_draw
                .zip(last_beat_to_draw)
                .map(|(first, last)| {
                    let first_row = beat_to_note_row(first);
                    let last_row = beat_to_note_row(last.max(first)).max(first_row);
                    (first_row, last_row)
                })
        };
        let adjusted_travel_offset = |travel_offset: f32| -> f32 {
            apply_accel_y(
                travel_offset,
                elapsed_screen,
                current_beat,
                effect_height,
                accel,
            )
        };
        let (note_start, note_end) = state.note_ranges[player_idx];
        let tipsy_y_for_col = |local_col: usize| -> f32 {
            tipsy_y_extra(local_col, elapsed_screen, visual) + move_y_extra(visual, local_col)
        };
        let lane_y_from_travel =
            |local_col: usize, receptor_y_lane: f32, dir: f32, travel_offset: f32| -> f32 {
                receptor_y_lane
                    + dir * adjusted_travel_offset(travel_offset)
                    + tipsy_y_for_col(local_col)
            };
        let lane_center_x_from_travel = |local_col: usize, travel_offset: f32| -> f32 {
            playfield_center_x
                + note_x_offset(
                    local_col,
                    adjusted_travel_offset(travel_offset),
                    elapsed_screen,
                    beat_push,
                    visual,
                    &col_offsets[..num_cols],
                    &invert_distances[..num_cols],
                    &tornado_bounds[..num_cols],
                )
        };
        let lane_center_x_from_adjusted_travel = |local_col: usize, adjusted_travel: f32| -> f32 {
            playfield_center_x
                + note_x_offset(
                    local_col,
                    adjusted_travel,
                    elapsed_screen,
                    beat_push,
                    visual,
                    &col_offsets[..num_cols],
                    &invert_distances[..num_cols],
                    &tornado_bounds[..num_cols],
                )
        };
        let adjusted_travel_from_screen_y =
            |local_col: usize, receptor_y_lane: f32, dir: f32, y_pos: f32| -> f32 {
                let dir = if dir.abs() <= 0.000_1 {
                    if dir < 0.0 { -0.000_1 } else { 0.000_1 }
                } else {
                    dir
                };
                (y_pos - receptor_y_lane - tipsy_y_for_col(local_col)) / dir
            };
        let alpha_for_travel = |local_col: usize, travel_offset: f32| -> f32 {
            let adjusted = adjusted_travel_offset(travel_offset);
            note_alpha(
                adjusted + tipsy_y_for_col(local_col),
                elapsed_screen,
                mini,
                appearance,
            )
        };
        let glow_for_travel = |local_col: usize, travel_offset: f32| -> f32 {
            let adjusted = adjusted_travel_offset(travel_offset);
            note_glow(
                adjusted + tipsy_y_for_col(local_col),
                elapsed_screen,
                mini,
                appearance,
            )
        };
        let world_z_for_raw_travel = |local_col: usize, travel_offset: f32| -> f32 {
            note_world_z_for_bumpy(
                adjusted_travel_offset(travel_offset),
                bumpy_for_col(&visual, local_col),
                visual.bumpy_offset,
                visual.bumpy_period,
            )
        };
        let world_z_for_adjusted_travel = |local_col: usize, travel_offset: f32| -> f32 {
            note_world_z_for_bumpy(
                travel_offset,
                bumpy_for_col(&visual, local_col),
                visual.bumpy_offset,
                visual.bumpy_period,
            )
        };
        // For dynamic values (e.g., last_held_beat while letting go), fall back to timing for that beat.
        // Direction and receptor row are per-lane: upwards lanes anchor to the normal receptor row,
        // downwards lanes anchor to the reverse row.
        let compute_lane_y_dynamic =
            |local_col: usize, beat: f32, receptor_y_lane: f32, dir: f32| -> f32 {
                let travel_offset = match scroll_speed {
                    ScrollSpeedSetting::CMod(_) => {
                        travel_offset_for_time_ns(timing.get_time_for_beat_ns(beat))
                    }
                    ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                        let note_disp_beat = timing.get_displayed_beat(beat);
                        let beat_diff_disp = note_disp_beat - curr_disp_beat;
                        beat_diff_disp
                            * ScrollSpeedSetting::ARROW_SPACING
                            * field_zoom
                            * beatmod_multiplier
                    }
                };
                lane_y_from_travel(local_col, receptor_y_lane, dir, travel_offset)
            };
        // Measure Lines (Zmod parity: NoteField:SetBeatBarsAlpha).
        // ScreenEdit/Practice always draws editor beat bars at 16th-note spacing.
        let show_measure_lines = view.edit_beat_bars
            || !matches!(
                profile.measure_lines,
                crate::game::profile::MeasureLines::Off
            );
        if show_measure_lines {
            let edit_bar_speed =
                edit_bar_scroll_speed(scroll_speed, state.scroll_reference_bpm, state.music_rate);
            let time_signatures = state
                .gameplay_charts
                .get(player_idx)
                .map(|chart| chart.timing_segments.time_signatures.as_slice())
                .unwrap_or(&[]);
            let edit_candidate_step_rows = edit_bar_candidate_step_rows(time_signatures);
            let (alpha_measure, alpha_quarter, alpha_eighth, alpha_sixteenth, line_step) =
                if view.edit_beat_bars {
                    (
                        1.0,
                        1.0,
                        scaled_edit_bar_alpha(edit_bar_speed, 1.0, 2.0),
                        scaled_edit_bar_alpha(edit_bar_speed, 2.0, 4.0),
                        note_row_to_beat(edit_candidate_step_rows),
                    )
                } else {
                    match profile.measure_lines {
                        crate::game::profile::MeasureLines::Off => (0.0, 0.0, 0.0, 0.0, 0.5),
                        crate::game::profile::MeasureLines::Measure => (0.75, 0.0, 0.0, 0.0, 0.5),
                        crate::game::profile::MeasureLines::Quarter => (0.75, 0.5, 0.0, 0.0, 0.5),
                        crate::game::profile::MeasureLines::Eighth => (0.75, 0.5, 0.125, 0.0, 0.5),
                    }
                };

            let mut pos_min_x: f32 = f32::INFINITY;
            let mut pos_max_x: f32 = f32::NEG_INFINITY;
            let mut pos_receptor_y: f32 = 0.0;
            let mut pos_any = false;

            let mut neg_min_x: f32 = f32::INFINITY;
            let mut neg_max_x: f32 = f32::NEG_INFINITY;
            let mut neg_receptor_y: f32 = 0.0;
            let mut neg_any = false;

            for i in 0..num_cols {
                let x = ns.column_xs[i] as f32 * spacing_mult;
                if column_dirs[i] >= 0.0 {
                    if pos_any {
                        pos_min_x = pos_min_x.min(x);
                        pos_max_x = pos_max_x.max(x);
                    } else {
                        pos_any = true;
                        pos_receptor_y = column_receptor_ys[i];
                        pos_min_x = x;
                        pos_max_x = x;
                    }
                } else if !neg_any {
                    neg_any = true;
                    neg_receptor_y = column_receptor_ys[i];
                    neg_min_x = x;
                    neg_max_x = x;
                } else {
                    neg_min_x = neg_min_x.min(x);
                    neg_max_x = neg_max_x.max(x);
                }
            }

            let beat_units_start = (current_beat / line_step).floor() as i64;
            let thickness = (2.0 * field_zoom).max(1.0);
            let y_min = -400.0;
            let y_max = screen_height() + 400.0;
            let edit_row_for_unit = |u: i64| -> Option<i32> {
                u.checked_mul(i64::from(edit_candidate_step_rows))
                    .and_then(|row| i32::try_from(row).ok())
            };
            let edit_line_alpha = |frame: u32| -> f32 {
                match frame {
                    0 => alpha_measure,
                    1 => alpha_quarter,
                    2 => alpha_eighth,
                    _ => alpha_sixteenth,
                }
            };
            let line_alpha = |u: i64| -> f32 {
                match u.rem_euclid(8) {
                    0 => alpha_measure,
                    2 | 4 | 6 => alpha_quarter,
                    _ => alpha_eighth,
                }
            };
            let edit_line_thickness = |frame: u32| -> f32 {
                match frame {
                    0 => (3.0 * field_zoom).max(1.0),
                    1 => (2.0 * field_zoom).max(1.0),
                    _ => (1.0 * field_zoom).max(1.0),
                }
            };

            let mut draw_group = |min_x: f32, max_x: f32, receptor_y: f32, dir: f32| {
                let center_x_offset = 0.5 * (min_x + max_x) * field_zoom;
                let w = ((max_x - min_x) + ScrollSpeedSetting::ARROW_SPACING) * field_zoom;
                if !w.is_finite() || w <= 0.0 {
                    return;
                }

                let x_center = playfield_center_x + center_x_offset;

                // Walk backward from current beat.
                let mut u = if view.edit_beat_bars {
                    beat_units_start.max(0)
                } else {
                    beat_units_start
                };
                let mut iters = 0;
                while iters < 2000 {
                    if view.edit_beat_bars && u < 0 {
                        break;
                    }
                    let (beat, edit_info) = if view.edit_beat_bars {
                        let Some(row) = edit_row_for_unit(u) else {
                            break;
                        };
                        (
                            note_row_to_beat(row),
                            edit_beat_bar_info_for_row(row, time_signatures),
                        )
                    } else {
                        ((u as f32) * line_step, None)
                    };
                    let alpha = if view.edit_beat_bars {
                        edit_info.map_or(0.0, |info| edit_line_alpha(info.frame))
                    } else {
                        line_alpha(u)
                    };
                    let y = compute_lane_y_dynamic(0, beat, receptor_y, dir);
                    if !y.is_finite() {
                        break;
                    }
                    if (dir >= 0.0 && y < y_min) || (dir < 0.0 && y > y_max) {
                        break;
                    }
                    if alpha > 0.0 {
                        let edit_bar_frame = edit_info.map_or(0, |info| info.frame);
                        let line_thickness = if view.edit_beat_bars {
                            edit_line_thickness(edit_bar_frame)
                        } else {
                            thickness
                        };
                        append_beat_bar(
                            &mut actors,
                            view.edit_beat_bars,
                            edit_bar_frame,
                            x_center,
                            y,
                            w,
                            field_zoom,
                            line_thickness,
                            alpha,
                        );
                        append_edit_measure_number(
                            &mut actors,
                            view.edit_beat_bars,
                            edit_info.and_then(|info| info.measure_index),
                            x_center - w * 0.5,
                            y,
                            field_zoom,
                        );
                    }
                    u -= 1;
                    iters += 1;
                }

                // Walk forward from the next beat-bar candidate to avoid duplicating the start line.
                let mut u = if view.edit_beat_bars {
                    beat_units_start.max(0) + 1
                } else {
                    beat_units_start + 1
                };
                let mut iters = 0;
                while iters < 2000 {
                    let (beat, edit_info) = if view.edit_beat_bars {
                        let Some(row) = edit_row_for_unit(u) else {
                            break;
                        };
                        (
                            note_row_to_beat(row),
                            edit_beat_bar_info_for_row(row, time_signatures),
                        )
                    } else {
                        ((u as f32) * line_step, None)
                    };
                    let alpha = if view.edit_beat_bars {
                        edit_info.map_or(0.0, |info| edit_line_alpha(info.frame))
                    } else {
                        line_alpha(u)
                    };
                    let y = compute_lane_y_dynamic(0, beat, receptor_y, dir);
                    if !y.is_finite() {
                        break;
                    }
                    if (dir >= 0.0 && y > y_max) || (dir < 0.0 && y < y_min) {
                        break;
                    }
                    if alpha > 0.0 {
                        let edit_bar_frame = edit_info.map_or(0, |info| info.frame);
                        let line_thickness = if view.edit_beat_bars {
                            edit_line_thickness(edit_bar_frame)
                        } else {
                            thickness
                        };
                        append_beat_bar(
                            &mut actors,
                            view.edit_beat_bars,
                            edit_bar_frame,
                            x_center,
                            y,
                            w,
                            field_zoom,
                            line_thickness,
                            alpha,
                        );
                        append_edit_measure_number(
                            &mut actors,
                            view.edit_beat_bars,
                            edit_info.and_then(|info| info.measure_index),
                            x_center - w * 0.5,
                            y,
                            field_zoom,
                        );
                    }
                    u += 1;
                    iters += 1;
                }
            };

            if pos_any {
                draw_group(pos_min_x, pos_max_x, pos_receptor_y, 1.0);
            }
            if neg_any {
                draw_group(neg_min_x, neg_max_x, neg_receptor_y, -1.0);
            }
        }

        if profile.column_cues {
            let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
                state.music_rate
            } else {
                1.0
            };
            if let Some(cue) = active_column_cue(&state.column_cues[player_idx], current_time) {
                let duration_real = cue.duration / rate;
                let elapsed_real = (current_time - cue.start_time) / rate;
                let alpha_mul = column_cue_alpha(elapsed_real, duration_real);
                if alpha_mul > 0.0 {
                    let lane_width = ScrollSpeedSetting::ARROW_SPACING * field_zoom;
                    let cue_height = column_cue_height();
                    let mut countdown_text: Option<(f32, f32, i32)> = None;

                    if duration_real >= 5.0 {
                        let remaining = duration_real - elapsed_real;
                        if remaining > 0.5
                            && let Some(last_col) = cue.columns.last()
                        {
                            let local_col = last_col.column.saturating_sub(col_start);
                            if local_col < num_cols {
                                let x = playfield_center_x
                                    + ns.column_xs[local_col] as f32 * spacing_mult * field_zoom;
                                let y = if column_dirs[local_col] < 0.0 {
                                    COLUMN_CUE_TEXT_REVERSE_Y
                                        + COLUMN_CUE_Y_OFFSET
                                        + notefield_offset_y
                                } else {
                                    COLUMN_CUE_TEXT_NORMAL_Y
                                        + COLUMN_CUE_Y_OFFSET
                                        + notefield_offset_y
                                };
                                countdown_text = Some((x, y, remaining.round() as i32));
                            }
                        }
                    }

                    for col_cue in &cue.columns {
                        let local_col = col_cue.column.saturating_sub(col_start);
                        if local_col >= num_cols {
                            continue;
                        }
                        let x = playfield_center_x
                            + ns.column_xs[local_col] as f32 * spacing_mult * field_zoom;
                        let alpha = COLUMN_CUE_BASE_ALPHA * alpha_mul;
                        let color = if col_cue.is_mine {
                            [1.0, 0.0, 0.0, alpha]
                        } else {
                            [0.3, 1.0, 1.0, alpha]
                        };
                        if column_dirs[local_col] < 0.0 {
                            let reverse_y = column_cue_reverse_top_y(
                                lane_width,
                                cue_height,
                                notefield_offset_y,
                            );
                            actors.push(act!(quad:
                                align(0.5, 0.0):
                                xy(x, reverse_y):
                                zoomto(lane_width, cue_height):
                                fadetop(0.333):
                                diffuse(color[0], color[1], color[2], color[3]):
                                z(Z_COLUMN_CUE)
                            ));
                        } else {
                            actors.push(act!(quad:
                                align(0.5, 0.0):
                                xy(x, COLUMN_CUE_Y_OFFSET + notefield_offset_y):
                                zoomto(lane_width, cue_height):
                                fadebottom(0.333):
                                diffuse(color[0], color[1], color[2], color[3]):
                                z(Z_COLUMN_CUE)
                            ));
                        }
                    }

                    if let Some((x, y, value)) = countdown_text {
                        hud_actors.push(act!(text:
                            font(mc_font_name):
                            settext(cached_int_i32(value)):
                            align(0.5, 0.5):
                            xy(x, y):
                            zoom(0.5):
                            z(200):
                            diffuse(1.0, 1.0, 1.0, alpha_mul)
                        ));
                    }
                }
            }
        }

        // Receptors + glow
        for (i, &receptor_y_lane) in column_receptor_ys.iter().take(num_cols).enumerate() {
            let col = col_start + i;
            let receptor_hidden_by_song_lua =
                song_lua_hides_note(state, player_idx, i, current_beat);
            let confusion_receptor_rot = confusion_rotation_deg(current_beat, visual, i);
            let receptor_center = receptor_row_center(
                playfield_center_x,
                i,
                receptor_y_lane,
                elapsed_screen,
                beat_push,
                visual,
                &col_offsets[..num_cols],
                &invert_distances[..num_cols],
                &tornado_bounds[..num_cols],
            );
            let bop_timer = state.receptor_bop_timers[col];
            let bop_zoom = if bop_timer > 0.0 {
                receptor_ns
                    .receptor_step_behavior_for_col(i)
                    .sample_zoom(bop_timer)
            } else {
                1.0
            };
            if !receptor_hidden_by_song_lua
                && !profile.hide_targets
                && receptor_alpha > f32::EPSILON
            {
                let receptor_effect_zoom = arrow_effect_zoom(&visual, i, 0.0);
                let receptor_slot = &receptor_ns.receptor_off[i];
                let receptor_reverse = receptor_ns
                    .receptor_off_reverse
                    .get(i)
                    .copied()
                    .unwrap_or_default()
                    .state(column_reverse_percent[i] > 0.5);
                let receptor_rotation =
                    receptor_slot.def.rotation_deg as f32 + receptor_reverse.base_rotation_z();
                let receptor_frame =
                    receptor_slot.frame_index(state.total_elapsed_in_screen, current_beat);
                let receptor_uv =
                    receptor_slot.uv_for_frame_at(receptor_frame, state.total_elapsed_in_screen);
                let receptor_draw =
                    receptor_slot.model_draw_at(state.total_elapsed_in_screen, current_beat);
                // ITG Sprite::SetTexture uses source-frame dimensions for draw size,
                // so receptor and overlay keep their authored ratio (e.g. 64 vs 74 in
                // dance/default) instead of being normalized to arrow height.
                let base_receptor_size = scale_explosion(logical_slot_size(receptor_slot));
                let receptor_size = [
                    base_receptor_size[0] * receptor_effect_zoom * receptor_draw.zoom[0],
                    base_receptor_size[1] * receptor_effect_zoom * receptor_draw.zoom[1],
                ];
                let receptor_color = receptor_ns.receptor_pulse.color_for_beat(current_beat);
                let alpha = receptor_color[3] * receptor_draw.tint[3] * receptor_alpha;
                if receptor_draw.visible
                    && alpha > f32::EPSILON
                    && receptor_size[0] > f32::EPSILON
                    && receptor_size[1] > f32::EPSILON
                {
                    let [sin_r, cos_r] = receptor_slot.base_rot_sin_cos();
                    let offset_scale = field_zoom * receptor_effect_zoom;
                    let offset = [
                        receptor_draw.pos[0] * offset_scale * cos_r
                            - receptor_draw.pos[1] * offset_scale * sin_r,
                        receptor_draw.pos[0] * offset_scale * sin_r
                            + receptor_draw.pos[1] * offset_scale * cos_r,
                    ];
                    let center = [
                        receptor_center[0] + offset[0],
                        receptor_center[1] + offset[1],
                    ];
                    actors.push(act!(sprite(receptor_slot.texture_key_handle()):
                        align(0.5, receptor_reverse.vert_align()):
                        xy(center[0], center[1]):
                        setsize(receptor_size[0], receptor_size[1]):
                        zoomx(slot_zoom_x(receptor_slot, bop_zoom)):
                        zoomy(slot_zoom_y(receptor_slot, bop_zoom)):
                        diffuse(
                            receptor_color[0] * receptor_draw.tint[0],
                            receptor_color[1] * receptor_draw.tint[1],
                            receptor_color[2] * receptor_draw.tint[2],
                            alpha
                        ):
                        rotationy(note_rotation_y):
                        rotationz(receptor_draw.rot[2] - receptor_rotation + confusion_receptor_rot):
                        customtexturerect(
                            receptor_uv[0],
                            receptor_uv[1],
                            receptor_uv[2],
                            receptor_uv[3]
                        ):
                        z(Z_RECEPTOR)
                    ));
                }
            }
            let hold_slot = if receptor_hidden_by_song_lua {
                None
            } else if let Some(active) = state.active_holds[col]
                .as_ref()
                .filter(|active| active_hold_is_engaged(active))
            {
                let note_type = &state.notes[active.note_index].note_type;
                let visuals = ns.hold_visuals_for_col(i, matches!(note_type, NoteType::Roll));
                if let Some(slot) = visuals.explosion.as_ref() {
                    Some(slot)
                } else {
                    ns.hold.explosion.as_ref().map(|slot| slot)
                }
            } else {
                None
            };
            if let Some(hold_slot) = hold_slot {
                let draw = song_lua_note_model_draw(
                    hold_slot.model_draw_at(state.total_elapsed_in_screen, current_beat),
                    note_rotation_y,
                );
                let hold_frame = hold_slot.frame_index(state.total_elapsed_in_screen, current_beat);
                let hold_uv = hold_slot.uv_for_frame_at(hold_frame, state.total_elapsed_in_screen);
                let base_size = scale_hold_explosion(hold_slot);
                let hold_size = [
                    base_size[0] * draw.zoom[0].max(0.0),
                    base_size[1] * draw.zoom[1].max(0.0),
                ];
                if hold_size[0] <= f32::EPSILON || hold_size[1] <= f32::EPSILON {
                    continue;
                }
                let base_rotation = hold_slot.def.rotation_deg as f32;
                let final_rotation = base_rotation - draw.rot[2] - confusion_receptor_rot;
                let center = receptor_center;
                let color = draw.tint;
                let glow = hold_slot.model_glow_with_draw(
                    draw,
                    state.total_elapsed_in_screen,
                    current_beat,
                    color[3],
                );
                let blend = if draw.blend_add {
                    BlendMode::Add
                } else {
                    BlendMode::Alpha
                };
                if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                    hold_slot,
                    draw,
                    center,
                    hold_size,
                    hold_uv,
                    -final_rotation,
                    color,
                    blend,
                    Z_HOLD_EXPLOSION as i16,
                    &mut model_cache,
                ) {
                    actors.push(model_actor);
                    if let Some(glow_color) = glow
                        && let Some(glow_actor) = noteskin_model_actor_from_draw_cached(
                            hold_slot,
                            draw,
                            center,
                            hold_size,
                            hold_uv,
                            -final_rotation,
                            glow_color,
                            blend,
                            Z_HOLD_EXPLOSION as i16,
                            &mut model_cache,
                        )
                    {
                        actors.push(glow_actor);
                    }
                } else if draw.blend_add {
                    actors.push(act!(sprite(hold_slot.texture_key_handle()):
                        align(0.5, 0.5):
                        xy(center[0], center[1]):
                        setsize(hold_size[0], hold_size[1]):
                        rotationz(-final_rotation):
                        customtexturerect(hold_uv[0], hold_uv[1], hold_uv[2], hold_uv[3]):
                        diffuse(color[0], color[1], color[2], color[3]):
                        blend(add):
                        z(Z_HOLD_EXPLOSION)
                    ));
                    if let Some(glow_color) = glow {
                        actors.push(act!(sprite(hold_slot.texture_key_handle()):
                            align(0.5, 0.5):
                            xy(center[0], center[1]):
                            setsize(hold_size[0], hold_size[1]):
                            rotationz(-final_rotation):
                            customtexturerect(hold_uv[0], hold_uv[1], hold_uv[2], hold_uv[3]):
                            diffuse(glow_color[0], glow_color[1], glow_color[2], glow_color[3]):
                            blend(add):
                            z(Z_HOLD_EXPLOSION)
                        ));
                    }
                } else {
                    actors.push(act!(sprite(hold_slot.texture_key_handle()):
                        align(0.5, 0.5):
                        xy(center[0], center[1]):
                        setsize(hold_size[0], hold_size[1]):
                        rotationz(-final_rotation):
                        customtexturerect(hold_uv[0], hold_uv[1], hold_uv[2], hold_uv[3]):
                        diffuse(color[0], color[1], color[2], color[3]):
                        blend(normal):
                        z(Z_HOLD_EXPLOSION)
                    ));
                    if let Some(glow_color) = glow {
                        actors.push(act!(sprite(hold_slot.texture_key_handle()):
                            align(0.5, 0.5):
                            xy(center[0], center[1]):
                            setsize(hold_size[0], hold_size[1]):
                            rotationz(-final_rotation):
                            customtexturerect(hold_uv[0], hold_uv[1], hold_uv[2], hold_uv[3]):
                            diffuse(glow_color[0], glow_color[1], glow_color[2], glow_color[3]):
                            blend(normal):
                            z(Z_HOLD_EXPLOSION)
                        ));
                    }
                }
            }
            if !receptor_hidden_by_song_lua
                && !profile.hide_targets
                && receptor_alpha > f32::EPSILON
                && let Some((alpha, zoom)) = receptor_glow_visual_for_col(state, col)
                && let Some(glow_slot) = receptor_ns
                    .receptor_glow
                    .get(i)
                    .and_then(|slot| slot.as_ref())
            {
                let alpha = alpha * receptor_alpha;
                if alpha > f32::EPSILON {
                    let glow_frame =
                        glow_slot.frame_index(state.total_elapsed_in_screen, current_beat);
                    let glow_uv =
                        glow_slot.uv_for_frame_at(glow_frame, state.total_elapsed_in_screen);
                    let glow_draw =
                        glow_slot.model_draw_at(state.total_elapsed_in_screen, current_beat);
                    let base_glow_size = scale_explosion(logical_slot_size(glow_slot));
                    let behavior = receptor_ns.receptor_glow_behavior;
                    let glow_reverse = receptor_ns
                        .receptor_glow_reverse
                        .get(i)
                        .copied()
                        .unwrap_or_default()
                        .state(column_reverse_percent[i] > 0.5);
                    let glow_rotation =
                        glow_slot.def.rotation_deg as f32 + glow_reverse.base_rotation_z();
                    let width = base_glow_size[0] * zoom * glow_draw.zoom[0];
                    let height = base_glow_size[1] * zoom * glow_draw.zoom[1];
                    if glow_draw.visible && width > f32::EPSILON && height > f32::EPSILON {
                        let [sin_r, cos_r] = glow_slot.base_rot_sin_cos();
                        let offset = [
                            glow_draw.pos[0] * field_zoom * cos_r
                                - glow_draw.pos[1] * field_zoom * sin_r,
                            glow_draw.pos[0] * field_zoom * sin_r
                                + glow_draw.pos[1] * field_zoom * cos_r,
                        ];
                        let center = [
                            receptor_center[0] + offset[0],
                            receptor_center[1] + offset[1],
                        ];
                        let color = [
                            glow_draw.tint[0],
                            glow_draw.tint[1],
                            glow_draw.tint[2],
                            alpha * glow_draw.tint[3],
                        ];
                        if behavior.blend_add {
                            actors.push(act!(sprite(glow_slot.texture_key_handle()):
                                align(0.5, glow_reverse.vert_align()):
                                xy(center[0], center[1]):
                                setsize(width, height):
                                zoomx(slot_zoom_x(glow_slot, bop_zoom)):
                                zoomy(slot_zoom_y(glow_slot, bop_zoom)):
                                rotationy(note_rotation_y):
                                rotationz(glow_draw.rot[2] - glow_rotation + confusion_receptor_rot):
                                customtexturerect(glow_uv[0], glow_uv[1], glow_uv[2], glow_uv[3]):
                                diffuse(color[0], color[1], color[2], color[3]):
                                blend(add):
                                z(Z_HOLD_GLOW)
                            ));
                        } else {
                            actors.push(act!(sprite(glow_slot.texture_key_handle()):
                                align(0.5, glow_reverse.vert_align()):
                                xy(center[0], center[1]):
                                setsize(width, height):
                                zoomx(slot_zoom_x(glow_slot, bop_zoom)):
                                zoomy(slot_zoom_y(glow_slot, bop_zoom)):
                                rotationy(note_rotation_y):
                                rotationz(glow_draw.rot[2] - glow_rotation + confusion_receptor_rot):
                                customtexturerect(glow_uv[0], glow_uv[1], glow_uv[2], glow_uv[3]):
                                diffuse(color[0], color[1], color[2], color[3]):
                                blend(normal):
                                z(Z_HOLD_GLOW)
                            ));
                        }
                    }
                }
            }
        }
        // Tap explosions (receptor noteflash / GhostArrow) are independent of
        // the "Hide Combo Explosions" UI option, which only affects combo splodes.
        for (i, active_opt) in state.tap_explosions[col_start..col_start + num_cols]
            .iter()
            .enumerate()
        {
            if song_lua_hides_note(state, player_idx, i, current_beat) {
                continue;
            }
            if let Some(active) = active_opt.as_ref()
                && let Some(tap_explosion_ns) = tap_explosion_ns
                && let Some(explosion) = tap_explosion_ns.tap_explosion_for_col_with_bright(
                    i,
                    active.window,
                    active.bright,
                )
            {
                let receptor_y_lane = column_receptor_ys[i];
                let receptor_center = receptor_row_center(
                    playfield_center_x,
                    i,
                    receptor_y_lane,
                    elapsed_screen,
                    beat_push,
                    visual,
                    &col_offsets[..num_cols],
                    &invert_distances[..num_cols],
                    &tornado_bounds[..num_cols],
                );
                let confusion_receptor_rot = confusion_rotation_deg(current_beat, visual, i);
                for layer in explosion.layers.iter() {
                    let anim_time = active.elapsed;
                    let slot = &layer.slot;
                    let beat_for_anim = if slot.source.is_beat_based() {
                        (state.current_beat_display - active.start_beat).max(0.0)
                    } else {
                        state.current_beat_display
                    };
                    let frame = slot.frame_index(anim_time, beat_for_anim);
                    let uv = slot.uv_for_frame_at(frame, state.total_elapsed_in_screen);
                    let size = scale_explosion(logical_slot_size(slot));
                    let explosion_visual = layer.animation.state_at(active.elapsed);
                    if !explosion_visual.visible {
                        continue;
                    }
                    let glow = explosion_visual.glow;
                    let glow_strength =
                        glow[0].abs() + glow[1].abs() + glow[2].abs() + glow[3].abs();
                    if layer.animation.blend_add {
                        actors.push(act!(sprite(slot.texture_key_handle()):
                            align(0.5, 0.5):
                            xy(receptor_center[0], receptor_center[1]):
                            setsize(size[0], size[1]):
                            zoom(explosion_visual.zoom):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            diffuse(
                                explosion_visual.diffuse[0],
                                explosion_visual.diffuse[1],
                                explosion_visual.diffuse[2],
                                explosion_visual.diffuse[3]
                            ):
                            rotationy(flat_tap_face_rotation_y):
                            rotationz(explosion_visual.rotation_z - slot.def.rotation_deg as f32 + confusion_receptor_rot):
                            blend(add):
                            z(Z_TAP_EXPLOSION)
                        ));
                        if glow_strength > f32::EPSILON {
                            actors.push(act!(sprite(slot.texture_key_handle()):
                                align(0.5, 0.5):
                                xy(receptor_center[0], receptor_center[1]):
                                setsize(size[0], size[1]):
                                zoom(explosion_visual.zoom):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                diffuse(glow[0], glow[1], glow[2], glow[3]):
                                rotationy(flat_tap_face_rotation_y):
                                rotationz(explosion_visual.rotation_z - slot.def.rotation_deg as f32 + confusion_receptor_rot):
                                blend(add):
                                z(Z_TAP_EXPLOSION)
                            ));
                        }
                    } else {
                        actors.push(act!(sprite(slot.texture_key_handle()):
                            align(0.5, 0.5):
                            xy(receptor_center[0], receptor_center[1]):
                            setsize(size[0], size[1]):
                            zoom(explosion_visual.zoom):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            diffuse(
                                explosion_visual.diffuse[0],
                                explosion_visual.diffuse[1],
                                explosion_visual.diffuse[2],
                                explosion_visual.diffuse[3]
                            ):
                            rotationy(flat_tap_face_rotation_y):
                            rotationz(explosion_visual.rotation_z - slot.def.rotation_deg as f32 + confusion_receptor_rot):
                            blend(normal):
                            z(Z_TAP_EXPLOSION)
                        ));
                        if glow_strength > f32::EPSILON {
                            actors.push(act!(sprite(slot.texture_key_handle()):
                                align(0.5, 0.5):
                                xy(receptor_center[0], receptor_center[1]):
                                setsize(size[0], size[1]):
                                zoom(explosion_visual.zoom):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                diffuse(glow[0], glow[1], glow[2], glow[3]):
                                rotationy(flat_tap_face_rotation_y):
                                rotationz(explosion_visual.rotation_z - slot.def.rotation_deg as f32 + confusion_receptor_rot):
                                blend(normal):
                                z(Z_TAP_EXPLOSION)
                            ));
                        }
                    }
                }
            }
        }
        // Mine explosions
        for (i, active_opt) in state.mine_explosions[col_start..col_start + num_cols]
            .iter()
            .enumerate()
        {
            let Some(active) = active_opt.as_ref() else {
                continue;
            };
            let Some(explosion) = mine_ns.mine_hit_explosion.as_ref() else {
                continue;
            };
            let receptor_y_lane = column_receptor_ys[i];
            let receptor_center = receptor_row_center(
                playfield_center_x,
                i,
                receptor_y_lane,
                elapsed_screen,
                beat_push,
                visual,
                &col_offsets[..num_cols],
                &invert_distances[..num_cols],
                &tornado_bounds[..num_cols],
            );
            for layer in explosion.layers.iter() {
                let slot = &layer.slot;
                let explosion_visual = layer.animation.state_at(active.elapsed);
                if !explosion_visual.visible {
                    continue;
                }
                let frame = slot.frame_index(active.elapsed, current_beat);
                let uv = slot.uv_for_frame_at(frame, state.total_elapsed_in_screen);
                let size = scale_explosion(logical_slot_size(slot));
                let glow = explosion_visual.glow;
                let glow_strength = glow[0].abs() + glow[1].abs() + glow[2].abs() + glow[3].abs();
                if layer.animation.blend_add {
                    actors.push(act!(sprite(slot.texture_key_handle()):
                        align(0.5, 0.5):
                        xy(receptor_center[0], receptor_center[1]):
                        setsize(size[0], size[1]):
                        zoom(explosion_visual.zoom):
                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                        rotationz(-explosion_visual.rotation_z):
                        diffuse(
                            explosion_visual.diffuse[0],
                            explosion_visual.diffuse[1],
                            explosion_visual.diffuse[2],
                            explosion_visual.diffuse[3]
                        ):
                        blend(add):
                        z(Z_MINE_EXPLOSION)
                    ));
                    if glow_strength > f32::EPSILON {
                        actors.push(act!(sprite(slot.texture_key_handle()):
                            align(0.5, 0.5):
                            xy(receptor_center[0], receptor_center[1]):
                            setsize(size[0], size[1]):
                            zoom(explosion_visual.zoom):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            rotationz(-explosion_visual.rotation_z):
                            diffuse(glow[0], glow[1], glow[2], glow[3]):
                            blend(add):
                            z(Z_MINE_EXPLOSION)
                        ));
                    }
                } else {
                    actors.push(act!(sprite(slot.texture_key_handle()):
                        align(0.5, 0.5):
                        xy(receptor_center[0], receptor_center[1]):
                        setsize(size[0], size[1]):
                        zoom(explosion_visual.zoom):
                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                        rotationz(-explosion_visual.rotation_z):
                        diffuse(
                            explosion_visual.diffuse[0],
                            explosion_visual.diffuse[1],
                            explosion_visual.diffuse[2],
                            explosion_visual.diffuse[3]
                        ):
                        blend(normal):
                        z(Z_MINE_EXPLOSION)
                    ));
                    if glow_strength > f32::EPSILON {
                        actors.push(act!(sprite(slot.texture_key_handle()):
                            align(0.5, 0.5):
                            xy(receptor_center[0], receptor_center[1]):
                            setsize(size[0], size[1]):
                            zoom(explosion_visual.zoom):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            rotationz(-explosion_visual.rotation_z):
                            diffuse(glow[0], glow[1], glow[2], glow[3]):
                            blend(normal):
                            z(Z_MINE_EXPLOSION)
                        ));
                    }
                }
            }
        }
        let mut render_hold = |note_index: usize| {
            let note = &state.notes[note_index];
            if note.column < col_start || note.column >= col_end {
                return;
            }
            let local_col = note.column - col_start;
            if !matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
                return;
            }
            if song_lua_hides_note(state, player_idx, local_col, note.beat) {
                return;
            }
            let Some(hold) = &note.hold else {
                return;
            };
            if matches!(hold.result, Some(HoldResult::Held)) {
                return;
            }

            // Prepare static/dynamic Y positions for the hold body
            // Head Y: dynamic if actively held or let go, otherwise static cache
            let mut head_beat = note.beat;
            let is_head_dynamic = hold.let_go_started_at.is_some()
                || matches!(hold.result, Some(HoldResult::LetGo | HoldResult::Missed));

            if is_head_dynamic {
                head_beat =
                    let_go_head_beat(note.beat, hold.end_beat, hold.last_held_beat, current_beat);
            }

            let col_dir = column_dirs[local_col];
            let dir = col_dir;
            let lane_receptor_y = column_receptor_ys[local_col];
            let receptor_center = receptor_row_center(
                playfield_center_x,
                local_col,
                lane_receptor_y,
                elapsed_screen,
                beat_push,
                visual,
                &col_offsets[..num_cols],
                &invert_distances[..num_cols],
                &tornado_bounds[..num_cols],
            );
            let receptor_draw_y = receptor_center[1];
            let receptor_center_x = receptor_center[0];

            let head_travel_offset = if is_head_dynamic {
                raw_travel_offset_for_beat(head_beat)
            } else {
                travel_offset_for_note(note, false)
            };
            let tail_travel_offset = travel_offset_for_note(note, true);
            let head_y = lane_y_from_travel(local_col, lane_receptor_y, dir, head_travel_offset);
            let tail_y = lane_y_from_travel(local_col, lane_receptor_y, dir, tail_travel_offset);
            let note_display = ns.note_display_metrics;
            // ITG gates reverse noteskin metrics by lane reverse state, not by
            // temporary visual inversion from scroll gimmicks.
            let lane_reverse = col_dir < 0.0;
            let active_state = state.active_holds[note.column]
                .as_ref()
                .filter(|h| h.note_index == note_index);
            // ITG keeps early-hit hold heads scrolling as inactive until the head
            // reaches the receptor row; only then does hold-active rendering clamp.
            let (engaged, use_active) =
                hold_head_render_flags(active_state, current_beat, note.beat);
            // ITG swaps hold start/end for reverse before applying hold-body offsets.
            let mut hold_start_y = if lane_reverse { tail_y } else { head_y };
            let mut hold_end_y = if lane_reverse { head_y } else { tail_y };
            let mut hold_start_travel = if lane_reverse {
                tail_travel_offset
            } else {
                head_travel_offset
            };
            let mut hold_end_travel = if lane_reverse {
                head_travel_offset
            } else {
                tail_travel_offset
            };
            if engaged {
                if lane_reverse {
                    hold_end_y = receptor_draw_y;
                    hold_end_travel = 0.0;
                } else {
                    hold_start_y = receptor_draw_y;
                    hold_start_travel = 0.0;
                }
            }
            // ITG swaps hold start/end offsets for reverse before applying
            // noteskin hold-body offsets (NoteDisplay::DrawHold).
            let body_flipped = lane_reverse && note_display.flip_hold_body_when_reverse;
            let (y_head, y_tail) = if body_flipped {
                (
                    hold_start_y - note_display.stop_drawing_hold_body_offset_from_tail,
                    hold_end_y - note_display.start_drawing_hold_body_offset_from_head,
                )
            } else {
                (
                    hold_start_y + note_display.start_drawing_hold_body_offset_from_head,
                    hold_end_y + note_display.stop_drawing_hold_body_offset_from_tail,
                )
            };
            let (top, bottom, draw_body_or_cap) = hold_draw_span(y_head, y_tail)
                .map_or((0.0, 0.0, false), |(top, bottom)| (top, bottom, true));
            let let_go_gray = ns.hold_let_go_gray_percent.clamp(0.0, 1.0);
            let hold_life = hold.life.clamp(0.0, 1.0);
            let hold_color_scale = let_go_gray + (1.0 - let_go_gray) * hold_life;
            let hold_diffuse = [hold_color_scale, hold_color_scale, hold_color_scale, 1.0];
            // ITG places hold head actor using post-swap start/end offsets:
            // DrawActor(..., bFlipHeadAndTail ? fEndYOffset : fStartYOffset, ...).
            let flip_head_and_tail = lane_reverse && note_display.flip_head_and_tail_when_reverse;
            let head_anchor_y = if flip_head_and_tail {
                hold_end_y
            } else {
                hold_start_y
            };
            let head_anchor_travel = if flip_head_and_tail {
                hold_end_travel
            } else {
                hold_start_travel
            };
            let visuals =
                ns.hold_visuals_for_col(local_col, matches!(note.note_type, NoteType::Roll));
            let hold_head_part = if matches!(note.note_type, NoteType::Roll) {
                NoteAnimPart::RollHead
            } else {
                NoteAnimPart::HoldHead
            };
            let hold_body_part = if matches!(note.note_type, NoteType::Roll) {
                NoteAnimPart::RollBody
            } else {
                NoteAnimPart::HoldBody
            };
            let mut hold_topcap_part = if matches!(note.note_type, NoteType::Roll) {
                NoteAnimPart::RollTopCap
            } else {
                NoteAnimPart::HoldTopCap
            };
            let mut hold_bottomcap_part = if matches!(note.note_type, NoteType::Roll) {
                NoteAnimPart::RollBottomCap
            } else {
                NoteAnimPart::HoldBottomCap
            };
            let hold_part_phase = ns.part_uv_phase(
                hold_head_part,
                state.total_elapsed_in_screen,
                current_beat,
                note.beat,
            );
            let hold_body_phase = ns.part_uv_phase(
                hold_body_part,
                state.total_elapsed_in_screen,
                current_beat,
                note.beat,
            );
            let mut hold_topcap_phase = ns.part_uv_phase(
                hold_topcap_part,
                state.total_elapsed_in_screen,
                current_beat,
                note.beat,
            );
            let mut hold_bottomcap_phase = ns.part_uv_phase(
                hold_bottomcap_part,
                state.total_elapsed_in_screen,
                current_beat,
                note.beat,
            );
            let mut top_cap_slot = if use_active {
                visuals
                    .topcap_active
                    .as_ref()
                    .or(visuals.topcap_inactive.as_ref())
            } else {
                visuals
                    .topcap_inactive
                    .as_ref()
                    .or(visuals.topcap_active.as_ref())
            };
            let mut bottom_cap_slot = if use_active {
                visuals
                    .bottomcap_active
                    .as_ref()
                    .or(visuals.bottomcap_inactive.as_ref())
            } else {
                visuals
                    .bottomcap_inactive
                    .as_ref()
                    .or(visuals.bottomcap_active.as_ref())
            };
            if body_flipped {
                std::mem::swap(&mut top_cap_slot, &mut bottom_cap_slot);
                std::mem::swap(&mut hold_topcap_part, &mut hold_bottomcap_part);
                std::mem::swap(&mut hold_topcap_phase, &mut hold_bottomcap_phase);
            }
            // Prepare clipped body extents. ITG DrawHoldBodyInternal always
            // draws the bottom cap downward from y_tail, so we keep body clipping
            // anchored to that same tail-side join.
            let body_top = top;
            let mut body_bottom = bottom;
            let hold_tiny_zoom = tiny_zoom_for_col(&visual, local_col);
            let hold_base_target_arrow_px = target_arrow_px * hold_tiny_zoom;
            let hold_arrow_px_for_adjusted_travel = |travel_offset: f32| -> f32 {
                hold_base_target_arrow_px * pulse_zoom_for_y(travel_offset, &visual)
            };
            let hold_target_arrow_px = hold_arrow_px_for_adjusted_travel(0.0);
            let hold_head_zoom = hold_tiny_zoom
                * pulse_zoom_for_y(adjusted_travel_offset(head_anchor_travel), &visual);
            let hold_head_target_arrow_px = target_arrow_px * hold_head_zoom;
            let hold_note_scale = field_zoom * hold_head_zoom;
            if let Some(cap_slot) = bottom_cap_slot {
                let cap_size = scale_cap_to_arrow(cap_slot.size(), hold_target_arrow_px);
                let cap_height = cap_size[1];
                if cap_height > f32::EPSILON {
                    // ITGmania joins hold body to cap at the tail edge (with a tiny overlap),
                    // not at the cap midpoint. Keep the body clipped to that join line.
                    body_bottom = body_bottom.min(y_tail + 1.0);
                    if body_bottom >= y_tail - 1.0 {
                        body_bottom = y_tail + 1.0;
                    }
                }
            }
            // Track rendered body extents so the tail cap can attach cleanly when
            // body segments are visible.
            let mut rendered_body_top: Option<f32> = None;
            let mut rendered_body_bottom: Option<f32> = None;
            let mut body_head_row: Option<[[f32; 3]; 2]> = None;
            let mut body_tail_row: Option<[[f32; 3]; 2]> = None;
            let col_bumpy = bumpy_for_col(&visual, local_col);
            let hold_depth_test = col_bumpy.abs() > f32::EPSILON;
            let use_legacy_hold_sprites = col_bumpy.abs() <= f32::EPSILON
                && !signed_effect_active(visual.drunk)
                && !signed_effect_active(visual.tornado)
                && !signed_effect_active(visual.beat)
                && visual.pulse_outer.abs() <= f32::EPSILON;
            let hold_y_rotation_active = note_rotation_y.abs() > f32::EPSILON;
            // ITG draws hold bodies from y_head to y_tail (top-to-bottom in screen space).
            // If noteskin offsets invert the interval for ultra-short holds, skip body draw
            // and rely on tail-cap clipping.
            let body_direction_invalid = y_tail <= y_head;
            if draw_body_or_cap
                && !body_direction_invalid
                && body_bottom > body_top
                && let Some(body_slot) = if use_active {
                    visuals
                        .body_active
                        .as_ref()
                        .or(visuals.body_inactive.as_ref())
                } else {
                    visuals
                        .body_inactive
                        .as_ref()
                        .or(visuals.body_active.as_ref())
                }
            {
                let texture_size = body_slot.size();
                let texture_width = texture_size[0].max(1) as f32;
                let texture_height = texture_size[1].max(1) as f32;
                if texture_width > f32::EPSILON && texture_height > f32::EPSILON {
                    let body_frame = body_slot.frame_index_from_phase(hold_body_phase);
                    let body_width = hold_target_arrow_px;
                    let scale = body_width / texture_width;
                    let segment_height = (texture_height * scale).max(f32::EPSILON);
                    let body_uv_elapsed = if body_slot.model.is_some() {
                        hold_body_phase
                    } else {
                        state.total_elapsed_in_screen
                    };
                    let body_uv = maybe_flip_uv_vert(
                        translated_uv_rect(
                            body_slot.uv_for_frame_at(body_frame, body_uv_elapsed),
                            ns.part_uv_translation(hold_body_part, note.beat, false),
                        ),
                        body_flipped,
                    );
                    let u0 = body_uv[0];
                    let u1 = body_uv[2];
                    let v_top = body_uv[1];
                    let v_bottom = body_uv[3];
                    let v_range = v_bottom - v_top;
                    let natural_top = y_head;
                    let natural_bottom = y_tail;
                    let hold_length = natural_bottom - natural_top;
                    const SEGMENT_PHASE_EPS: f32 = 1e-4;
                    if hold_length > f32::EPSILON
                        && let Some((clipped_top, clipped_bottom)) = clipped_hold_body_bounds(
                            body_top,
                            body_bottom,
                            natural_top,
                            natural_bottom,
                        )
                    {
                        let visible_top_distance = clipped_top - natural_top;
                        let visible_bottom_distance = clipped_bottom - natural_top;
                        let visible_span = visible_bottom_distance - visible_top_distance;
                        let (max_segments, allow_legacy_sprites) =
                            hold_body_segment_budget(visible_span, segment_height);
                        let anchor_to_top =
                            lane_reverse && note_display.top_hold_anchor_when_reverse;
                        let phase_offset = if anchor_to_top {
                            0.0
                        } else {
                            let total_phase = hold_length / segment_height;
                            if total_phase >= 1.0 + SEGMENT_PHASE_EPS {
                                let fractional = total_phase.fract();
                                if fractional > SEGMENT_PHASE_EPS
                                    && (1.0 - fractional) > SEGMENT_PHASE_EPS
                                {
                                    1.0 - fractional
                                } else {
                                    0.0
                                }
                            } else {
                                0.0
                            }
                        };

                        let mut phase = visible_top_distance / segment_height + phase_offset;
                        let phase_end_adjusted =
                            visible_bottom_distance / segment_height + phase_offset;
                        let mut emitted = 0;

                        if use_legacy_hold_sprites && allow_legacy_sprites {
                            while phase + SEGMENT_PHASE_EPS < phase_end_adjusted
                                && emitted < max_segments
                            {
                                let mut next_phase = (phase.floor() + 1.0).min(phase_end_adjusted);
                                if next_phase - phase < SEGMENT_PHASE_EPS {
                                    next_phase = phase_end_adjusted;
                                }
                                if next_phase - phase < SEGMENT_PHASE_EPS {
                                    break;
                                }

                                let distance_start = (phase - phase_offset) * segment_height;
                                let distance_end = (next_phase - phase_offset) * segment_height;
                                let y_start = natural_top + distance_start;
                                let y_end = natural_top + distance_end;
                                let segment_top = y_start.max(body_top);
                                let segment_bottom = y_end.min(body_bottom);

                                if segment_bottom - segment_top <= f32::EPSILON {
                                    phase = next_phase;
                                    continue;
                                }

                                let base_floor = phase.floor();
                                let start_fraction = (phase - base_floor).clamp(0.0, 1.0);
                                let end_fraction = (next_phase - base_floor).clamp(0.0, 1.0);
                                let mut v0 = v_top + v_range * start_fraction;
                                let mut v1 = v_top + v_range * end_fraction;

                                let segment_size = segment_bottom - segment_top;
                                let portion = (segment_size / segment_height).clamp(0.0, 1.0);
                                let tail_gap = (natural_bottom - body_bottom).max(0.0);
                                let body_reaches_tail = tail_gap <= segment_height + 1.0;
                                let is_last_visible_segment = (body_bottom - segment_bottom).abs()
                                    <= 0.5
                                    || next_phase >= phase_end_adjusted - SEGMENT_PHASE_EPS;

                                if body_reaches_tail && is_last_visible_segment {
                                    if v_range >= 0.0 {
                                        v1 = v_bottom;
                                        v0 = v_bottom - v_range.abs() * portion;
                                    } else {
                                        v1 = v_bottom;
                                        v0 = v_bottom + v_range.abs() * portion;
                                    }
                                }

                                let segment_center_screen = (segment_top + segment_bottom) * 0.5;
                                let segment_center_travel = adjusted_travel_from_screen_y(
                                    local_col,
                                    lane_receptor_y,
                                    dir,
                                    segment_center_screen,
                                );
                                let segment_alpha = note_alpha(
                                    segment_center_travel + tipsy_y_for_col(local_col),
                                    elapsed_screen,
                                    mini,
                                    appearance,
                                );
                                let segment_glow = note_glow(
                                    segment_center_travel + tipsy_y_for_col(local_col),
                                    elapsed_screen,
                                    mini,
                                    appearance,
                                );
                                if segment_alpha > f32::EPSILON {
                                    let segment_center_x = lane_center_x_from_adjusted_travel(
                                        local_col,
                                        segment_center_travel,
                                    );
                                    rendered_body_top = Some(match rendered_body_top {
                                        None => segment_top,
                                        Some(v) => v.min(segment_top),
                                    });
                                    rendered_body_bottom = Some(match rendered_body_bottom {
                                        None => segment_bottom,
                                        Some(v) => v.max(segment_bottom),
                                    });
                                    actors.push(actor_with_world_z(
                                        act!(sprite(body_slot.texture_key_handle()):
                                            align(0.5, 0.5):
                                            xy(segment_center_x, segment_center_screen):
                                            setsize(body_width, segment_size):
                                            rotationy(note_rotation_y):
                                            rotationz(0.0):
                                            customtexturerect(u0, v0, u1, v1):
                                            diffuse(
                                                hold_diffuse[0],
                                                hold_diffuse[1],
                                                hold_diffuse[2],
                                                hold_diffuse[3] * segment_alpha
                                            ):
                                            z(Z_HOLD_BODY)
                                        ),
                                        world_z_for_adjusted_travel(
                                            local_col,
                                            segment_center_travel,
                                        ),
                                    ));
                                    if segment_glow > f32::EPSILON {
                                        actors.push(actor_with_world_z(
                                            act!(sprite(body_slot.texture_key_handle()):
                                                align(0.5, 0.5):
                                                xy(segment_center_x, segment_center_screen):
                                                setsize(body_width, segment_size):
                                                rotationy(note_rotation_y):
                                                rotationz(0.0):
                                                customtexturerect(u0, v0, u1, v1):
                                                diffuse(1.0, 1.0, 1.0, segment_glow):
                                                z(Z_HOLD_GLOW)
                                            ),
                                            world_z_for_adjusted_travel(
                                                local_col,
                                                segment_center_travel,
                                            ),
                                        ));
                                    }
                                }

                                phase = next_phase;
                                emitted += 1;
                            }
                        } else {
                            let body_slice_step = if hold_depth_test { 4.0 } else { 16.0 };
                            let use_body_mesh =
                                body_slot.model.is_none() && !hold_y_rotation_active;
                            let mut body_mesh_vertices: Option<Vec<TexturedMeshVertex>> = None;
                            let mut body_glow_vertices: Option<Vec<TexturedMeshVertex>> = None;
                            let mut prev_body_row: Option<[[f32; 3]; 2]> = None;

                            while phase + SEGMENT_PHASE_EPS < phase_end_adjusted
                                && emitted < max_segments
                            {
                                let mut next_phase = (phase.floor() + 1.0).min(phase_end_adjusted);
                                if next_phase - phase < SEGMENT_PHASE_EPS {
                                    next_phase = phase_end_adjusted;
                                }
                                if next_phase - phase < SEGMENT_PHASE_EPS {
                                    break;
                                }

                                let distance_start = (phase - phase_offset) * segment_height;
                                let distance_end = (next_phase - phase_offset) * segment_height;
                                let y_start = natural_top + distance_start;
                                let y_end = natural_top + distance_end;
                                let segment_top = y_start.max(body_top);
                                let segment_bottom = y_end.min(body_bottom);

                                if segment_bottom - segment_top <= f32::EPSILON {
                                    phase = next_phase;
                                    continue;
                                }

                                let base_floor = phase.floor();
                                let start_fraction = (phase - base_floor).clamp(0.0, 1.0);
                                let end_fraction = (next_phase - base_floor).clamp(0.0, 1.0);
                                let mut v0 = v_top + v_range * start_fraction;
                                let mut v1 = v_top + v_range * end_fraction;

                                let segment_size = segment_bottom - segment_top;
                                let portion = (segment_size / segment_height).clamp(0.0, 1.0);

                                let tail_gap = (natural_bottom - body_bottom).max(0.0);
                                let body_reaches_tail = tail_gap <= segment_height + 1.0;
                                let is_last_visible_segment = (body_bottom - segment_bottom).abs()
                                    <= 0.5
                                    || next_phase >= phase_end_adjusted - SEGMENT_PHASE_EPS;

                                if body_reaches_tail && is_last_visible_segment {
                                    if v_range >= 0.0 {
                                        v1 = v_bottom;
                                        v0 = v_bottom - v_range.abs() * portion;
                                    } else {
                                        v1 = v_bottom;
                                        v0 = v_bottom + v_range.abs() * portion;
                                    }
                                }
                                let mut slice_top = segment_top;
                                while slice_top + f32::EPSILON < segment_bottom {
                                    let slice_bottom =
                                        (slice_top + body_slice_step).min(segment_bottom);
                                    let slice_size = slice_bottom - slice_top;
                                    if slice_size <= f32::EPSILON {
                                        break;
                                    }
                                    let slice_t0 =
                                        ((slice_top - segment_top) / segment_size).clamp(0.0, 1.0);
                                    let slice_t1 = ((slice_bottom - segment_top) / segment_size)
                                        .clamp(0.0, 1.0);
                                    let slice_v0 = (v1 - v0).mul_add(slice_t0, v0);
                                    let slice_v1 = (v1 - v0).mul_add(slice_t1, v0);
                                    let slice_center_screen = (slice_top + slice_bottom) * 0.5;
                                    let slice_center_travel = adjusted_travel_from_screen_y(
                                        local_col,
                                        lane_receptor_y,
                                        dir,
                                        slice_center_screen,
                                    );
                                    let slice_alpha = note_alpha(
                                        slice_center_travel + tipsy_y_for_col(local_col),
                                        elapsed_screen,
                                        mini,
                                        appearance,
                                    );
                                    let slice_glow = note_glow(
                                        slice_center_travel + tipsy_y_for_col(local_col),
                                        elapsed_screen,
                                        mini,
                                        appearance,
                                    );
                                    if slice_alpha <= f32::EPSILON {
                                        prev_body_row = None;
                                        slice_top = slice_bottom;
                                        continue;
                                    }
                                    let slice_top_travel = adjusted_travel_from_screen_y(
                                        local_col,
                                        lane_receptor_y,
                                        dir,
                                        slice_top,
                                    );
                                    let slice_bottom_travel = adjusted_travel_from_screen_y(
                                        local_col,
                                        lane_receptor_y,
                                        dir,
                                        slice_bottom,
                                    );
                                    let slice_top_x = lane_center_x_from_adjusted_travel(
                                        local_col,
                                        slice_top_travel,
                                    );
                                    let slice_bottom_x = lane_center_x_from_adjusted_travel(
                                        local_col,
                                        slice_bottom_travel,
                                    );
                                    let (slice_center, slice_height, slice_rotation) =
                                        hold_segment_pose(
                                            [slice_top_x, slice_top],
                                            [slice_bottom_x, slice_bottom],
                                        );
                                    if slice_height <= f32::EPSILON {
                                        slice_top = slice_bottom;
                                        continue;
                                    }
                                    let slice_world_z =
                                        world_z_for_adjusted_travel(local_col, slice_center_travel);

                                    rendered_body_top = Some(match rendered_body_top {
                                        None => slice_top,
                                        Some(v) => v.min(slice_top),
                                    });
                                    rendered_body_bottom = Some(match rendered_body_bottom {
                                        None => slice_bottom,
                                        Some(v) => v.max(slice_bottom),
                                    });

                                    if use_body_mesh {
                                        let top_alpha = note_alpha(
                                            slice_top_travel + tipsy_y_for_col(local_col),
                                            elapsed_screen,
                                            mini,
                                            appearance,
                                        );
                                        let bottom_alpha = note_alpha(
                                            slice_bottom_travel + tipsy_y_for_col(local_col),
                                            elapsed_screen,
                                            mini,
                                            appearance,
                                        );
                                        let top_glow = note_glow(
                                            slice_top_travel + tipsy_y_for_col(local_col),
                                            elapsed_screen,
                                            mini,
                                            appearance,
                                        );
                                        let bottom_glow = note_glow(
                                            slice_bottom_travel + tipsy_y_for_col(local_col),
                                            elapsed_screen,
                                            mini,
                                            appearance,
                                        );
                                        let slice_forward = [
                                            slice_bottom_x - slice_top_x,
                                            slice_bottom - slice_top,
                                        ];
                                        let top_half_width =
                                            hold_arrow_px_for_adjusted_travel(slice_top_travel)
                                                * 0.5;
                                        let bottom_half_width =
                                            hold_arrow_px_for_adjusted_travel(slice_bottom_travel)
                                                * 0.5;
                                        let slice_top_z = world_z_for_adjusted_travel(
                                            local_col,
                                            slice_top_travel,
                                        );
                                        let slice_bottom_z = world_z_for_adjusted_travel(
                                            local_col,
                                            slice_bottom_travel,
                                        );
                                        let top_row = prev_body_row.unwrap_or_else(|| {
                                            let row = hold_strip_row_3d(
                                                [slice_top_x, slice_top, slice_top_z],
                                                slice_forward,
                                                top_half_width,
                                                u0,
                                                u1,
                                                slice_v0,
                                                [
                                                    hold_diffuse[0],
                                                    hold_diffuse[1],
                                                    hold_diffuse[2],
                                                    hold_diffuse[3] * top_alpha,
                                                ],
                                            );
                                            [row[0].pos, row[1].pos]
                                        });
                                        let top_row = hold_strip_row_from_positions(
                                            top_row[0],
                                            top_row[1],
                                            u0,
                                            u1,
                                            slice_v0,
                                            [
                                                hold_diffuse[0],
                                                hold_diffuse[1],
                                                hold_diffuse[2],
                                                hold_diffuse[3] * top_alpha,
                                            ],
                                        );
                                        if body_head_row.is_none() {
                                            body_head_row = Some([top_row[0].pos, top_row[1].pos]);
                                        }
                                        let bottom_row = hold_strip_row_3d(
                                            [slice_bottom_x, slice_bottom, slice_bottom_z],
                                            slice_forward,
                                            bottom_half_width,
                                            u0,
                                            u1,
                                            slice_v1,
                                            [
                                                hold_diffuse[0],
                                                hold_diffuse[1],
                                                hold_diffuse[2],
                                                hold_diffuse[3] * bottom_alpha,
                                            ],
                                        );
                                        let mesh_vertices = body_mesh_vertices
                                            .get_or_insert_with(|| Vec::with_capacity(96));
                                        mesh_vertices.extend_from_slice(&hold_strip_quad(
                                            top_row, bottom_row,
                                        ));
                                        if top_glow > f32::EPSILON || bottom_glow > f32::EPSILON {
                                            let top_glow_row = hold_strip_row_from_positions(
                                                top_row[0].pos,
                                                top_row[1].pos,
                                                u0,
                                                u1,
                                                slice_v0,
                                                hold_glow_color(top_glow),
                                            );
                                            let bottom_glow_row = hold_strip_row_from_positions(
                                                bottom_row[0].pos,
                                                bottom_row[1].pos,
                                                u0,
                                                u1,
                                                slice_v1,
                                                hold_glow_color(bottom_glow),
                                            );
                                            let glow_vertices = body_glow_vertices
                                                .get_or_insert_with(|| Vec::with_capacity(96));
                                            glow_vertices.extend_from_slice(&hold_strip_quad(
                                                top_glow_row,
                                                bottom_glow_row,
                                            ));
                                        }
                                        body_tail_row =
                                            Some([bottom_row[0].pos, bottom_row[1].pos]);
                                        prev_body_row =
                                            Some([bottom_row[0].pos, bottom_row[1].pos]);
                                    } else {
                                        actors.push(actor_with_world_z(
                                            act!(sprite(body_slot.texture_key_handle()):
                                                align(0.5, 0.5):
                                                xy(slice_center[0], slice_center[1]):
                                                setsize(body_width, slice_height):
                                                rotationy(note_rotation_y):
                                                rotationz(slice_rotation):
                                                customtexturerect(u0, slice_v0, u1, slice_v1):
                                                diffuse(
                                                    hold_diffuse[0],
                                                    hold_diffuse[1],
                                                    hold_diffuse[2],
                                                    hold_diffuse[3] * slice_alpha
                                                ):
                                                z(Z_HOLD_BODY)
                                            ),
                                            slice_world_z,
                                        ));
                                        if slice_glow > f32::EPSILON {
                                            actors.push(actor_with_world_z(
                                                act!(sprite(body_slot.texture_key_handle()):
                                                    align(0.5, 0.5):
                                                    xy(slice_center[0], slice_center[1]):
                                                    setsize(body_width, slice_height):
                                                    rotationy(note_rotation_y):
                                                    rotationz(slice_rotation):
                                                    customtexturerect(u0, slice_v0, u1, slice_v1):
                                                    diffuse(1.0, 1.0, 1.0, slice_glow):
                                                    z(Z_HOLD_GLOW)
                                                ),
                                                slice_world_z,
                                            ));
                                        }
                                    }
                                    slice_top = slice_bottom;
                                }

                                phase = next_phase;
                                emitted += 1;
                            }

                            if let Some(vertices) = body_mesh_vertices
                                && !vertices.is_empty()
                            {
                                actors.push(hold_strip_actor(
                                    body_slot.texture_key_shared(),
                                    Arc::from(vertices),
                                    BlendMode::Alpha,
                                    hold_depth_test,
                                    Z_HOLD_BODY as i16,
                                ));
                            }
                            if let Some(vertices) = body_glow_vertices
                                && !vertices.is_empty()
                            {
                                actors.push(hold_strip_actor(
                                    body_slot.texture_key_shared(),
                                    Arc::from(vertices),
                                    BlendMode::Alpha,
                                    hold_depth_test,
                                    Z_HOLD_GLOW as i16,
                                ));
                            }
                        }
                    }
                }
            }
            if draw_body_or_cap && let Some(cap_slot) = top_cap_slot {
                let head_position = y_head;
                if head_position > -400.0 && head_position < screen_height() + 400.0 {
                    let cap_frame = cap_slot.frame_index_from_phase(hold_topcap_phase);
                    let cap_uv_elapsed = if cap_slot.model.is_some() {
                        hold_topcap_phase
                    } else {
                        state.total_elapsed_in_screen
                    };
                    let cap_uv = maybe_flip_uv_vert(
                        translated_uv_rect(
                            cap_slot.uv_for_frame_at(cap_frame, cap_uv_elapsed),
                            ns.part_uv_translation(hold_topcap_part, note.beat, false),
                        ),
                        body_flipped,
                    );
                    let cap_uv = maybe_mirror_uv_horiz_for_reverse_flipped(
                        cap_uv,
                        lane_reverse,
                        body_flipped,
                    );
                    let cap_size = scale_cap_to_arrow(cap_slot.size(), hold_target_arrow_px);
                    let cap_width = cap_size[0];
                    let mut cap_height = cap_size[1];
                    let u0 = cap_uv[0];
                    let u1 = cap_uv[2];
                    let v0 = cap_uv[1];
                    let mut v1 = cap_uv[3];
                    let cap_top = y_head - cap_height;
                    let mut cap_bottom = y_head;
                    if cap_height > f32::EPSILON {
                        let v_span = v1 - v0;
                        if y_tail < cap_bottom {
                            let trimmed = (cap_bottom - y_tail).clamp(0.0, cap_height);
                            if trimmed >= cap_height - f32::EPSILON {
                                cap_height = 0.0;
                            } else if trimmed > f32::EPSILON {
                                let fraction = trimmed / cap_height;
                                v1 -= v_span * fraction;
                                cap_bottom -= trimmed;
                                cap_height = cap_bottom - cap_top;
                            }
                        }
                    }
                    if cap_height > f32::EPSILON {
                        let cap_center = (cap_top + cap_bottom) * 0.5;
                        let cap_center_travel = adjusted_travel_from_screen_y(
                            local_col,
                            lane_receptor_y,
                            dir,
                            cap_center,
                        );
                        let cap_alpha = note_alpha(
                            cap_center_travel + tipsy_y_for_col(local_col),
                            elapsed_screen,
                            mini,
                            appearance,
                        );
                        let cap_glow = note_glow(
                            cap_center_travel + tipsy_y_for_col(local_col),
                            elapsed_screen,
                            mini,
                            appearance,
                        );
                        if cap_alpha <= f32::EPSILON {
                            return;
                        }
                        let cap_top_travel =
                            adjusted_travel_from_screen_y(local_col, lane_receptor_y, dir, cap_top);
                        let cap_bottom_travel = adjusted_travel_from_screen_y(
                            local_col,
                            lane_receptor_y,
                            dir,
                            cap_bottom,
                        );
                        let cap_top_x =
                            lane_center_x_from_adjusted_travel(local_col, cap_top_travel);
                        let cap_bottom_x =
                            lane_center_x_from_adjusted_travel(local_col, cap_bottom_travel);
                        let (cap_center_xy, cap_draw_height, cap_path_rotation) =
                            hold_segment_pose([cap_top_x, cap_top], [cap_bottom_x, cap_bottom]);
                        if cap_draw_height <= f32::EPSILON {
                            return;
                        }
                        let use_top_cap_mesh = !use_legacy_hold_sprites
                            && cap_slot.model.is_none()
                            && !hold_y_rotation_active;
                        if use_top_cap_mesh {
                            let top_alpha = note_alpha(
                                cap_top_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let bottom_alpha = note_alpha(
                                cap_bottom_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let top_glow = note_glow(
                                cap_top_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let bottom_glow = note_glow(
                                cap_bottom_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let cap_forward = [cap_bottom_x - cap_top_x, cap_bottom - cap_top];
                            let top_half_width =
                                hold_arrow_px_for_adjusted_travel(cap_top_travel) * 0.5;
                            let bottom_half_width =
                                hold_arrow_px_for_adjusted_travel(cap_bottom_travel) * 0.5;
                            let cap_top_z = world_z_for_adjusted_travel(local_col, cap_top_travel);
                            let cap_bottom_z =
                                world_z_for_adjusted_travel(local_col, cap_bottom_travel);
                            let top_row = hold_strip_row_3d(
                                [cap_top_x, cap_top, cap_top_z],
                                cap_forward,
                                top_half_width,
                                u0,
                                u1,
                                v0,
                                [
                                    hold_diffuse[0],
                                    hold_diffuse[1],
                                    hold_diffuse[2],
                                    hold_diffuse[3] * top_alpha,
                                ],
                            );
                            let bottom_row = if let Some(body_head_row) = body_head_row
                                && rendered_body_top
                                    .is_some_and(|body_top| (body_top - cap_bottom).abs() <= 2.0)
                            {
                                hold_strip_row_from_positions(
                                    body_head_row[0],
                                    body_head_row[1],
                                    u0,
                                    u1,
                                    v1,
                                    [
                                        hold_diffuse[0],
                                        hold_diffuse[1],
                                        hold_diffuse[2],
                                        hold_diffuse[3] * bottom_alpha,
                                    ],
                                )
                            } else {
                                hold_strip_row_3d(
                                    [cap_bottom_x, cap_bottom, cap_bottom_z],
                                    cap_forward,
                                    bottom_half_width,
                                    u0,
                                    u1,
                                    v1,
                                    [
                                        hold_diffuse[0],
                                        hold_diffuse[1],
                                        hold_diffuse[2],
                                        hold_diffuse[3] * bottom_alpha,
                                    ],
                                )
                            };
                            actors.push(hold_strip_actor(
                                cap_slot.texture_key_shared(),
                                Arc::new(hold_strip_quad(top_row, bottom_row)),
                                BlendMode::Alpha,
                                hold_depth_test,
                                Z_HOLD_CAP as i16,
                            ));
                            if top_glow > f32::EPSILON || bottom_glow > f32::EPSILON {
                                let top_glow_row = hold_strip_row_from_positions(
                                    top_row[0].pos,
                                    top_row[1].pos,
                                    u0,
                                    u1,
                                    v0,
                                    hold_glow_color(top_glow),
                                );
                                let bottom_glow_row = hold_strip_row_from_positions(
                                    bottom_row[0].pos,
                                    bottom_row[1].pos,
                                    u0,
                                    u1,
                                    v1,
                                    hold_glow_color(bottom_glow),
                                );
                                actors.push(hold_strip_actor(
                                    cap_slot.texture_key_shared(),
                                    Arc::new(hold_strip_quad(top_glow_row, bottom_glow_row)),
                                    BlendMode::Alpha,
                                    hold_depth_test,
                                    Z_HOLD_GLOW as i16,
                                ));
                            }
                        } else {
                            let cap_world_z =
                                world_z_for_adjusted_travel(local_col, cap_center_travel);
                            let cap_rotation = cap_path_rotation
                                + top_cap_rotation_deg(lane_reverse, body_flipped);
                            actors.push(actor_with_world_z(
                                act!(sprite(cap_slot.texture_key_handle()):
                                    align(0.5, 0.5):
                                    xy(cap_center_xy[0], cap_center_xy[1]):
                                    setsize(cap_width, cap_draw_height):
                                    customtexturerect(u0, v0, u1, v1):
                                    diffuse(
                                        hold_diffuse[0],
                                        hold_diffuse[1],
                                        hold_diffuse[2],
                                        hold_diffuse[3] * cap_alpha
                                    ):
                                    rotationy(note_rotation_y):
                                    rotationz(cap_rotation):
                                    z(Z_HOLD_CAP)
                                ),
                                cap_world_z,
                            ));
                            if cap_glow > f32::EPSILON {
                                actors.push(actor_with_world_z(
                                    act!(sprite(cap_slot.texture_key_handle()):
                                        align(0.5, 0.5):
                                        xy(cap_center_xy[0], cap_center_xy[1]):
                                        setsize(cap_width, cap_draw_height):
                                        customtexturerect(u0, v0, u1, v1):
                                        diffuse(1.0, 1.0, 1.0, cap_glow):
                                        rotationy(note_rotation_y):
                                        rotationz(cap_rotation):
                                        z(Z_HOLD_GLOW)
                                    ),
                                    cap_world_z,
                                ));
                            }
                        }
                    }
                }
            }
            if draw_body_or_cap && let Some(cap_slot) = bottom_cap_slot {
                let tail_position = y_tail + 1.0;
                if tail_position > -400.0 && tail_position < screen_height() + 400.0 {
                    let cap_frame = cap_slot.frame_index_from_phase(hold_bottomcap_phase);
                    let cap_uv_elapsed = if cap_slot.model.is_some() {
                        hold_bottomcap_phase
                    } else {
                        state.total_elapsed_in_screen
                    };
                    let cap_uv = maybe_flip_uv_vert(
                        translated_uv_rect(
                            cap_slot.uv_for_frame_at(cap_frame, cap_uv_elapsed),
                            ns.part_uv_translation(hold_bottomcap_part, note.beat, false),
                        ),
                        body_flipped,
                    );
                    let cap_uv = maybe_mirror_uv_horiz_for_reverse_flipped(
                        cap_uv,
                        lane_reverse,
                        body_flipped,
                    );
                    let cap_size = scale_cap_to_arrow(cap_slot.size(), hold_target_arrow_px);
                    let cap_width = cap_size[0];
                    let cap_span = cap_size[1];
                    let u0 = cap_uv[0];
                    let u1 = cap_uv[2];
                    let v_base0 = cap_uv[1];
                    let v_base1 = cap_uv[3];
                    // Prefer attaching to rendered body edge when available; fall
                    // back to native tail anchoring for collapsed micro-holds.
                    let Some((raw_top, raw_bottom)) = hold_tail_cap_bounds(
                        y_tail + 1.0,
                        cap_span,
                        rendered_body_top,
                        rendered_body_bottom,
                    ) else {
                        return;
                    };
                    if cap_span <= f32::EPSILON {
                        return;
                    }

                    // ITG DrawHoldPart bottom-cap UV progression:
                    // add_to_tex_coord = (frame_h - visible_h / zoom) / frame_h, clamped at 0.
                    // In our renderer cap_span is already zoomed size, so this reduces to
                    // add_to_tex_coord = 1 - visible_h / cap_span.
                    let mut draw_top = raw_top;
                    let draw_bottom = raw_bottom;
                    if y_head > draw_top {
                        draw_top = y_head.min(draw_bottom);
                    }
                    let draw_height = draw_bottom - draw_top;
                    let anchor_to_top = lane_reverse && note_display.top_hold_anchor_when_reverse;
                    let Some((v0, v1)) = bottom_cap_uv_window(
                        v_base0,
                        v_base1,
                        draw_height,
                        cap_span,
                        anchor_to_top,
                    ) else {
                        return;
                    };
                    let cap_center = (draw_top + draw_bottom) * 0.5;
                    if draw_height > f32::EPSILON {
                        let cap_center_travel = adjusted_travel_from_screen_y(
                            local_col,
                            lane_receptor_y,
                            dir,
                            cap_center,
                        );
                        let cap_alpha = note_alpha(
                            cap_center_travel + tipsy_y_for_col(local_col),
                            elapsed_screen,
                            mini,
                            appearance,
                        );
                        let cap_glow = note_glow(
                            cap_center_travel + tipsy_y_for_col(local_col),
                            elapsed_screen,
                            mini,
                            appearance,
                        );
                        if cap_alpha <= f32::EPSILON {
                            return;
                        }
                        let cap_top_travel = adjusted_travel_from_screen_y(
                            local_col,
                            lane_receptor_y,
                            dir,
                            draw_top,
                        );
                        let cap_bottom_travel = adjusted_travel_from_screen_y(
                            local_col,
                            lane_receptor_y,
                            dir,
                            draw_bottom,
                        );
                        let cap_top_x =
                            lane_center_x_from_adjusted_travel(local_col, cap_top_travel);
                        let cap_bottom_x =
                            lane_center_x_from_adjusted_travel(local_col, cap_bottom_travel);
                        let (cap_center_xy, cap_draw_height, cap_rotation) =
                            hold_segment_pose([cap_top_x, draw_top], [cap_bottom_x, draw_bottom]);
                        if cap_draw_height <= f32::EPSILON {
                            return;
                        }
                        let use_bottom_cap_mesh = !use_legacy_hold_sprites
                            && cap_slot.model.is_none()
                            && !lane_reverse
                            && !hold_y_rotation_active;
                        if use_bottom_cap_mesh {
                            let top_alpha = note_alpha(
                                cap_top_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let bottom_alpha = note_alpha(
                                cap_bottom_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let top_glow = note_glow(
                                cap_top_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let bottom_glow = note_glow(
                                cap_bottom_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let cap_forward = [cap_bottom_x - cap_top_x, draw_bottom - draw_top];
                            let top_half_width =
                                hold_arrow_px_for_adjusted_travel(cap_top_travel) * 0.5;
                            let bottom_half_width =
                                hold_arrow_px_for_adjusted_travel(cap_bottom_travel) * 0.5;
                            let cap_top_z = world_z_for_adjusted_travel(local_col, cap_top_travel);
                            let cap_bottom_z =
                                world_z_for_adjusted_travel(local_col, cap_bottom_travel);
                            let top_row = if let Some(body_tail_row) = body_tail_row {
                                hold_strip_row_from_positions(
                                    body_tail_row[0],
                                    body_tail_row[1],
                                    u0,
                                    u1,
                                    v0,
                                    [
                                        hold_diffuse[0],
                                        hold_diffuse[1],
                                        hold_diffuse[2],
                                        hold_diffuse[3] * top_alpha,
                                    ],
                                )
                            } else {
                                hold_strip_row_3d(
                                    [cap_top_x, draw_top, cap_top_z],
                                    cap_forward,
                                    top_half_width,
                                    u0,
                                    u1,
                                    v0,
                                    [
                                        hold_diffuse[0],
                                        hold_diffuse[1],
                                        hold_diffuse[2],
                                        hold_diffuse[3] * top_alpha,
                                    ],
                                )
                            };
                            let bottom_row = hold_strip_row_3d(
                                [cap_bottom_x, draw_bottom, cap_bottom_z],
                                cap_forward,
                                bottom_half_width,
                                u0,
                                u1,
                                v1,
                                [
                                    hold_diffuse[0],
                                    hold_diffuse[1],
                                    hold_diffuse[2],
                                    hold_diffuse[3] * bottom_alpha,
                                ],
                            );
                            actors.push(hold_strip_actor(
                                cap_slot.texture_key_shared(),
                                Arc::new(hold_strip_quad(top_row, bottom_row)),
                                BlendMode::Alpha,
                                hold_depth_test,
                                Z_HOLD_CAP as i16,
                            ));
                            if top_glow > f32::EPSILON || bottom_glow > f32::EPSILON {
                                let top_glow_row = hold_strip_row_from_positions(
                                    top_row[0].pos,
                                    top_row[1].pos,
                                    u0,
                                    u1,
                                    v0,
                                    hold_glow_color(top_glow),
                                );
                                let bottom_glow_row = hold_strip_row_from_positions(
                                    bottom_row[0].pos,
                                    bottom_row[1].pos,
                                    u0,
                                    u1,
                                    v1,
                                    hold_glow_color(bottom_glow),
                                );
                                actors.push(hold_strip_actor(
                                    cap_slot.texture_key_shared(),
                                    Arc::new(hold_strip_quad(top_glow_row, bottom_glow_row)),
                                    BlendMode::Alpha,
                                    hold_depth_test,
                                    Z_HOLD_GLOW as i16,
                                ));
                            }
                        } else {
                            let cap_world_z =
                                world_z_for_adjusted_travel(local_col, cap_center_travel);
                            actors.push(actor_with_world_z(
                                act!(sprite(cap_slot.texture_key_handle()):
                                    align(0.5, 0.5):
                                    xy(cap_center_xy[0], cap_center_xy[1]):
                                    setsize(cap_width, cap_draw_height):
                                    customtexturerect(u0, v0, u1, v1):
                                    diffuse(
                                        hold_diffuse[0],
                                        hold_diffuse[1],
                                        hold_diffuse[2],
                                        hold_diffuse[3] * cap_alpha
                                    ):
                                    rotationy(note_rotation_y):
                                    rotationz(cap_rotation):
                                    z(Z_HOLD_CAP)
                                ),
                                cap_world_z,
                            ));
                            if cap_glow > f32::EPSILON {
                                actors.push(actor_with_world_z(
                                    act!(sprite(cap_slot.texture_key_handle()):
                                        align(0.5, 0.5):
                                        xy(cap_center_xy[0], cap_center_xy[1]):
                                        setsize(cap_width, cap_draw_height):
                                        customtexturerect(u0, v0, u1, v1):
                                        diffuse(1.0, 1.0, 1.0, cap_glow):
                                        rotationy(note_rotation_y):
                                        rotationz(cap_rotation):
                                        z(Z_HOLD_GLOW)
                                    ),
                                    cap_world_z,
                                ));
                            }
                        }
                    }
                }
            }
            let should_draw_hold_head = true;
            let head_draw_y = head_anchor_y;
            let head_draw_delta = (head_draw_y - receptor_draw_y) * dir;
            if should_draw_hold_head
                && head_draw_delta >= -draw_distance_after_targets
                && head_draw_delta <= draw_distance_before_targets
            {
                let head_alpha = alpha_for_travel(local_col, head_anchor_travel);
                let head_glow = glow_for_travel(local_col, head_anchor_travel);
                if head_alpha <= f32::EPSILON && head_glow <= f32::EPSILON {
                    return;
                }
                let hold_head_rot =
                    calc_note_rotation_z(visual, note.beat, current_beat, true, local_col);
                let note_idx = local_col * NUM_QUANTIZATIONS + note.quantization_idx as usize;
                let head_center_x = if (head_draw_y - receptor_draw_y).abs() <= 0.5 {
                    receptor_center_x
                } else {
                    lane_center_x_from_travel(local_col, head_anchor_travel)
                };
                let head_center = [head_center_x, head_draw_y];
                let head_world_z = world_z_for_raw_travel(local_col, head_anchor_travel);
                let elapsed = state.total_elapsed_in_screen;
                let head_layers = if use_active {
                    visuals
                        .head_active_layers
                        .as_deref()
                        .or(visuals.head_inactive_layers.as_deref())
                } else {
                    visuals
                        .head_inactive_layers
                        .as_deref()
                        .or(visuals.head_active_layers.as_deref())
                };
                let head_slot = if head_layers.is_none() && use_active {
                    visuals
                        .head_active
                        .as_ref()
                        .or(visuals.head_inactive.as_ref())
                } else if head_layers.is_none() {
                    visuals
                        .head_inactive
                        .as_ref()
                        .or(visuals.head_active.as_ref())
                } else {
                    None
                };
                let hold_head_translation =
                    ns.part_uv_translation(hold_head_part, note.beat, false);
                let head_slot = head_slot.and_then(|slot| {
                    let draw = song_lua_note_model_draw(
                        slot.model_draw_at(elapsed, current_beat),
                        note_rotation_y,
                    );
                    if !draw.visible {
                        return None;
                    }
                    let note_scale = hold_note_scale;
                    let base_size = note_slot_base_size(slot, note_scale);
                    (base_size[0] * draw.zoom[0].max(0.0) > f32::EPSILON
                        && base_size[1] * draw.zoom[1].max(0.0) > f32::EPSILON)
                        .then_some((slot, draw, note_scale, base_size))
                });
                if let Some((head_slot, draw, note_scale, base_size)) = head_slot {
                    let frame = head_slot.frame_index_from_phase(hold_part_phase);
                    let uv_elapsed = if head_slot.model.is_some() {
                        hold_part_phase
                    } else {
                        elapsed
                    };
                    let uv = translated_uv_rect(
                        head_slot.uv_for_frame_at(frame, uv_elapsed),
                        hold_head_translation,
                    );
                    let local_offset = [draw.pos[0] * note_scale, draw.pos[1] * note_scale];
                    let local_offset_rot_sin_cos = head_slot.base_rot_sin_cos();
                    let model_center = if head_slot.model.is_some() {
                        let [sin_r, cos_r] = local_offset_rot_sin_cos;
                        let offset = [
                            local_offset[0] * cos_r - local_offset[1] * sin_r,
                            local_offset[0] * sin_r + local_offset[1] * cos_r,
                        ];
                        [head_center[0] + offset[0], head_center[1] + offset[1]]
                    } else {
                        head_center
                    };
                    let size = [
                        base_size[0] * draw.zoom[0].max(0.0),
                        base_size[1] * draw.zoom[1].max(0.0),
                    ];
                    if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
                        return;
                    }
                    let color = [
                        draw.tint[0] * hold_diffuse[0],
                        draw.tint[1] * hold_diffuse[1],
                        draw.tint[2] * hold_diffuse[2],
                        draw.tint[3] * hold_diffuse[3] * head_alpha,
                    ];
                    let blend = if draw.blend_add {
                        BlendMode::Add
                    } else {
                        BlendMode::Alpha
                    };
                    if !prefer_sprite_note_path
                        && let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                            head_slot,
                            draw,
                            model_center,
                            size,
                            uv,
                            -head_slot.def.rotation_deg as f32 + hold_head_rot,
                            color,
                            blend,
                            Z_TAP_NOTE as i16,
                            &mut model_cache,
                        )
                    {
                        actors.push(actor_with_world_z(model_actor, head_world_z));
                    } else if draw.blend_add {
                        let sprite_center =
                            offset_center(head_center, local_offset, local_offset_rot_sin_cos);
                        actors.push(actor_with_world_z(
                            act!(sprite(head_slot.texture_key_handle()):
                                align(0.5, 0.5):
                                xy(sprite_center[0], sprite_center[1]):
                                setsize(size[0], size[1]):
                                rotationy(flat_tap_face_rotation_y):
                                rotationz(draw.rot[2] - head_slot.def.rotation_deg as f32 + hold_head_rot):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                diffuse(color[0], color[1], color[2], color[3]):
                                blend(add):
                                z(Z_TAP_NOTE)
                            ),
                            head_world_z,
                        ));
                    } else {
                        let sprite_center =
                            offset_center(head_center, local_offset, local_offset_rot_sin_cos);
                        actors.push(actor_with_world_z(
                            act!(sprite(head_slot.texture_key_handle()):
                                align(0.5, 0.5):
                                xy(sprite_center[0], sprite_center[1]):
                                setsize(size[0], size[1]):
                                rotationy(flat_tap_face_rotation_y):
                                rotationz(draw.rot[2] - head_slot.def.rotation_deg as f32 + hold_head_rot):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                diffuse(color[0], color[1], color[2], color[3]):
                                blend(normal):
                                z(Z_TAP_NOTE)
                            ),
                            head_world_z,
                        ));
                    }
                    push_note_glow_actor(
                        &mut actors,
                        NoteGlowDraw {
                            slot: head_slot,
                            draw,
                            model_center,
                            sprite_center: offset_center(
                                head_center,
                                local_offset,
                                local_offset_rot_sin_cos,
                            ),
                            size,
                            uv,
                            rotation_y: flat_tap_face_rotation_y,
                            model_rotation_z: -head_slot.def.rotation_deg as f32 + hold_head_rot,
                            sprite_rotation_z: draw.rot[2] - head_slot.def.rotation_deg as f32
                                + hold_head_rot,
                            alpha: head_glow,
                            blend,
                            z: Z_TAP_NOTE as i16,
                            world_z: head_world_z,
                            prefer_sprite: prefer_sprite_note_path,
                        },
                        &mut model_cache,
                    );
                } else if let Some(note_slots) = head_layers
                    .or_else(|| ns.note_layers.get(note_idx).map(|layers| layers.as_ref()))
                {
                    let note_scale = hold_note_scale;
                    for note_slot in note_slots.iter() {
                        let draw = song_lua_note_model_draw(
                            note_slot.model_draw_at(elapsed, current_beat),
                            note_rotation_y,
                        );
                        if !draw.visible {
                            continue;
                        }
                        let frame = note_slot.frame_index_from_phase(hold_part_phase);
                        let uv_elapsed = if note_slot.model.is_some() {
                            hold_part_phase
                        } else {
                            elapsed
                        };
                        let uv = translated_uv_rect(
                            note_slot.uv_for_frame_at(frame, uv_elapsed),
                            hold_head_translation,
                        );
                        let base_size = note_slot_base_size(note_slot, note_scale);
                        let offset_scale = note_scale;
                        let local_offset = [draw.pos[0] * offset_scale, draw.pos[1] * offset_scale];
                        let local_offset_rot_sin_cos = note_slot.base_rot_sin_cos();
                        let model_center = if note_slot.model.is_some() {
                            let [sin_r, cos_r] = local_offset_rot_sin_cos;
                            let offset = [
                                local_offset[0] * cos_r - local_offset[1] * sin_r,
                                local_offset[0] * sin_r + local_offset[1] * cos_r,
                            ];
                            [head_center[0] + offset[0], head_center[1] + offset[1]]
                        } else {
                            head_center
                        };
                        let size = [
                            base_size[0] * draw.zoom[0].max(0.0),
                            base_size[1] * draw.zoom[1].max(0.0),
                        ];
                        if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
                            continue;
                        }
                        let color = [
                            draw.tint[0] * hold_diffuse[0],
                            draw.tint[1] * hold_diffuse[1],
                            draw.tint[2] * hold_diffuse[2],
                            draw.tint[3] * hold_diffuse[3] * head_alpha,
                        ];
                        let layer_z = Z_TAP_NOTE;
                        let blend = if draw.blend_add {
                            BlendMode::Add
                        } else {
                            BlendMode::Alpha
                        };
                        if !prefer_sprite_note_path
                            && let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                                note_slot,
                                draw,
                                model_center,
                                size,
                                uv,
                                -note_slot.def.rotation_deg as f32 + hold_head_rot,
                                color,
                                blend,
                                layer_z as i16,
                                &mut model_cache,
                            )
                        {
                            actors.push(actor_with_world_z(model_actor, head_world_z));
                        } else if draw.blend_add {
                            let sprite_center =
                                offset_center(head_center, local_offset, local_offset_rot_sin_cos);
                            actors.push(actor_with_world_z(
                                act!(sprite(note_slot.texture_key_handle()):
                                    align(0.5, 0.5):
                                    xy(sprite_center[0], sprite_center[1]):
                                    setsize(size[0], size[1]):
                                    rotationy(flat_tap_face_rotation_y):
                                    rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32 + hold_head_rot):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(add):
                                    z(layer_z)
                                ),
                                head_world_z,
                            ));
                        } else {
                            let sprite_center =
                                offset_center(head_center, local_offset, local_offset_rot_sin_cos);
                            actors.push(actor_with_world_z(
                                act!(sprite(note_slot.texture_key_handle()):
                                    align(0.5, 0.5):
                                    xy(sprite_center[0], sprite_center[1]):
                                    setsize(size[0], size[1]):
                                    rotationy(flat_tap_face_rotation_y):
                                    rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32 + hold_head_rot):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(normal):
                                    z(layer_z)
                                ),
                                head_world_z,
                            ));
                        }
                        push_note_glow_actor(
                            &mut actors,
                            NoteGlowDraw {
                                slot: note_slot,
                                draw,
                                model_center,
                                sprite_center: offset_center(
                                    head_center,
                                    local_offset,
                                    local_offset_rot_sin_cos,
                                ),
                                size,
                                uv,
                                rotation_y: flat_tap_face_rotation_y,
                                model_rotation_z: -note_slot.def.rotation_deg as f32
                                    + hold_head_rot,
                                sprite_rotation_z: draw.rot[2] - note_slot.def.rotation_deg as f32
                                    + hold_head_rot,
                                alpha: head_glow,
                                blend,
                                z: layer_z as i16,
                                world_z: head_world_z,
                                prefer_sprite: prefer_sprite_note_path,
                            },
                            &mut model_cache,
                        );
                    }
                } else if let Some(note_slot) = ns.notes.get(note_idx) {
                    let frame = note_slot.frame_index_from_phase(hold_part_phase);
                    let uv_elapsed = if note_slot.model.is_some() {
                        hold_part_phase
                    } else {
                        elapsed
                    };
                    let uv = translated_uv_rect(
                        note_slot.uv_for_frame_at(frame, uv_elapsed),
                        hold_head_translation,
                    );
                    let size = scale_sprite_to_arrow(note_slot.size(), hold_head_target_arrow_px);
                    let draw = song_lua_note_model_draw(
                        note_slot.model_draw_at(elapsed, current_beat),
                        note_rotation_y,
                    );
                    if !prefer_sprite_note_path
                        && let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                            note_slot,
                            draw,
                            head_center,
                            size,
                            uv,
                            -note_slot.def.rotation_deg as f32 + hold_head_rot,
                            [
                                hold_diffuse[0],
                                hold_diffuse[1],
                                hold_diffuse[2],
                                hold_diffuse[3] * head_alpha,
                            ],
                            BlendMode::Alpha,
                            Z_TAP_NOTE as i16,
                            &mut model_cache,
                        )
                    {
                        actors.push(actor_with_world_z(model_actor, head_world_z));
                    } else {
                        actors.push(actor_with_world_z(
                            act!(sprite(note_slot.texture_key_handle()):
                                align(0.5, 0.5):
                                xy(head_center[0], head_center[1]):
                                setsize(size[0], size[1]):
                                rotationy(flat_tap_face_rotation_y):
                                rotationz(-note_slot.def.rotation_deg as f32 + hold_head_rot):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                diffuse(
                                    hold_diffuse[0],
                                    hold_diffuse[1],
                                    hold_diffuse[2],
                                    hold_diffuse[3] * head_alpha
                                ):
                                z(Z_TAP_NOTE)
                            ),
                            head_world_z,
                        ));
                    }
                    push_note_glow_actor(
                        &mut actors,
                        NoteGlowDraw {
                            slot: note_slot,
                            draw,
                            model_center: head_center,
                            sprite_center: head_center,
                            size,
                            uv,
                            rotation_y: flat_tap_face_rotation_y,
                            model_rotation_z: -note_slot.def.rotation_deg as f32 + hold_head_rot,
                            sprite_rotation_z: -note_slot.def.rotation_deg as f32 + hold_head_rot,
                            alpha: head_glow,
                            blend: BlendMode::Alpha,
                            z: Z_TAP_NOTE as i16,
                            world_z: head_world_z,
                            prefer_sprite: prefer_sprite_note_path,
                        },
                        &mut model_cache,
                    );
                }
            }
        };
        for local_col in 0..num_cols {
            let col = col_start + local_col;
            for_each_visible_hold_index(
                &state.lane_hold_indices[col],
                &state.notes,
                visible_row_range,
                |note_index| render_hold(note_index),
            );
        }
        let extra_hold_indices = state
            .active_holds
            .iter()
            .filter_map(|a| a.as_ref().map(|h| h.note_index))
            .chain(state.decaying_hold_indices.iter().copied())
            .filter(|&idx| {
                idx >= note_start
                    && idx < note_end
                    && !hold_overlaps_visible_window(idx, &state.notes, visible_row_range)
            });
        for note_index in extra_hold_indices {
            render_hold(note_index);
        }
        let elapsed = state.total_elapsed_in_screen;
        let note_display_time = elapsed * note_display_time_scale;
        let mine_fill_phase = current_beat.rem_euclid(1.0);
        let draw_hold_same_row = ns.note_display_metrics.draw_hold_head_for_taps_on_same_row;
        let draw_roll_same_row = ns.note_display_metrics.draw_roll_head_for_taps_on_same_row;
        let tap_same_row_means_hold = ns.note_display_metrics.tap_hold_roll_on_row_means_hold;
        // Visible tap and mine notes
        for col_idx in 0..num_cols {
            let col = col_start + col_idx;
            let column_note_indices = &state.lane_note_indices[col];
            let dir = column_dirs[col_idx];
            let receptor_y_lane = column_receptor_ys[col_idx];
            let fill_slot = mine_ns.mines.get(col_idx).and_then(|slot| slot.as_ref());
            let fill_gradient_slot = mine_ns
                .mine_fill_slots
                .get(col_idx)
                .and_then(|slot| slot.as_ref());
            let frame_slot = mine_ns
                .mine_frames
                .get(col_idx)
                .and_then(|slot| slot.as_ref());
            for_each_visible_note_index(
                column_note_indices,
                &state.notes,
                visible_row_range,
                |note_index| {
                    let note = &state.notes[note_index];
                    if matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
                        return;
                    }
                    if song_lua_hides_note(state, player_idx, col_idx, note.beat) {
                        return;
                    }
                    if !note.is_fake {
                        if matches!(note.note_type, NoteType::Mine) {
                            if mine_hides_after_resolution(note.mine_result) {
                                return;
                            }
                        } else if note.result.is_some()
                            && row_hides_completed_note(state, player_idx, note.row_index)
                        {
                            return;
                        }
                    }
                    let raw_travel_offset = travel_offset_for_note(note, false);
                    let travel_offset = adjusted_travel_offset(raw_travel_offset);
                    let y_pos =
                        lane_y_from_travel(col_idx, receptor_y_lane, dir, raw_travel_offset);
                    let delta = travel_offset;
                    if delta < -draw_distance_after_targets || delta > draw_distance_before_targets
                    {
                        return;
                    }
                    let note_alpha = alpha_for_travel(col_idx, raw_travel_offset);
                    let note_glow = if matches!(note.note_type, NoteType::Mine) {
                        0.0
                    } else {
                        glow_for_travel(col_idx, raw_travel_offset)
                    };
                    if note_alpha <= f32::EPSILON && note_glow <= f32::EPSILON {
                        return;
                    }
                    let column_center_x = lane_center_x_from_travel(col_idx, raw_travel_offset);
                    let note_world_z = world_z_for_adjusted_travel(col_idx, travel_offset);
                    let col_effect_zoom = arrow_effect_zoom(&visual, col_idx, travel_offset);
                    let col_note_scale = field_zoom * col_effect_zoom;
                    let col_target_arrow_px = target_arrow_px * col_effect_zoom;
                    let scale_mine_slot_for_note = |slot: &SpriteSlot| -> [f32; 2] {
                        let size = scale_mine_slot(slot);
                        [size[0] * col_effect_zoom, size[1] * col_effect_zoom]
                    };
                    let note_rot =
                        calc_note_rotation_z(visual, note.beat, current_beat, false, col_idx);
                    if matches!(note.note_type, NoteType::Mine) {
                        if fill_slot.is_none() && frame_slot.is_none() {
                            return;
                        }
                        let mine_note_beat = note.beat;
                        let mine_uv_phase =
                            mine_ns.tap_mine_uv_phase(elapsed, current_beat, mine_note_beat);
                        let mine_translation =
                            mine_ns.part_uv_translation(NoteAnimPart::Mine, mine_note_beat, false);
                        let circle_reference = frame_slot
                            .map(|slot| scale_mine_slot_for_note(slot))
                            .or_else(|| fill_slot.map(|slot| scale_mine_slot_for_note(slot)))
                            .unwrap_or([
                                TARGET_ARROW_PIXEL_SIZE * col_note_scale,
                                TARGET_ARROW_PIXEL_SIZE * col_note_scale,
                            ]);
                        if let Some(slot) = fill_slot {
                            if frame_slot.is_some()
                                && slot.model.is_none()
                                && slot.source.frame_count() <= 1
                                && let Some(gradient_slot) = fill_gradient_slot
                            {
                                let width = circle_reference[0] * MINE_CORE_SIZE_RATIO;
                                let height = circle_reference[1] * MINE_CORE_SIZE_RATIO;
                                if width > 0.0 && height > 0.0 {
                                    let frame =
                                        gradient_slot.frame_index_from_phase(mine_fill_phase);
                                    let uv = gradient_slot.uv_for_frame_at(frame, elapsed);
                                    actors.push(actor_with_world_z(
                                        act!(sprite(gradient_slot.texture_key_handle()):
                                            align(0.5, 0.5):
                                            xy(column_center_x, y_pos):
                                            setsize(width, height):
                                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                            diffuse(1.0, 1.0, 1.0, note_alpha):
                                            z(Z_TAP_NOTE - 2)
                                        ),
                                        note_world_z,
                                    ));
                                }
                            } else {
                                let draw = song_lua_note_model_draw(
                                    slot.model_draw_at(note_display_time, current_beat),
                                    note_rotation_y,
                                );
                                if draw.visible {
                                    let frame = slot.frame_index_from_phase(mine_uv_phase);
                                    let uv_elapsed = if slot.model.is_some() {
                                        mine_uv_phase
                                    } else {
                                        elapsed
                                    };
                                    let uv = translated_uv_rect(
                                        slot.uv_for_frame_at(frame, uv_elapsed),
                                        mine_translation,
                                    );
                                    let size = scale_mine_slot_for_note(slot);
                                    let width = size[0];
                                    let height = size[1];
                                    let base_rotation = -slot.def.rotation_deg as f32;
                                    // ITG only rotates mines when the actor/model declares it.
                                    let sprite_rotation = base_rotation + draw.rot[2] + note_rot;
                                    let center = [column_center_x, y_pos];
                                    if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                                        slot,
                                        draw,
                                        center,
                                        [width, height],
                                        uv,
                                        base_rotation + note_rot,
                                        [1.0, 1.0, 1.0, 0.9 * note_alpha],
                                        BlendMode::Alpha,
                                        (Z_TAP_NOTE - 1) as i16,
                                        &mut model_cache,
                                    ) {
                                        actors.push(actor_with_world_z(model_actor, note_world_z));
                                    } else {
                                        actors.push(actor_with_world_z(
                                            act!(sprite(slot.texture_key_handle()):
                                                align(0.5, 0.5):
                                                xy(center[0], center[1]):
                                                setsize(width, height):
                                                rotationy(note_rotation_y):
                                                rotationz(sprite_rotation):
                                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                                diffuse(1.0, 1.0, 1.0, 0.9 * note_alpha):
                                                z(Z_TAP_NOTE - 1)
                                            ),
                                            note_world_z,
                                        ));
                                    }
                                }
                            }
                        }
                        if let Some(slot) = frame_slot {
                            let draw = song_lua_note_model_draw(
                                slot.model_draw_at(note_display_time, current_beat),
                                note_rotation_y,
                            );
                            if !draw.visible {
                                return;
                            }
                            let frame = slot.frame_index_from_phase(mine_uv_phase);
                            let uv_elapsed = if slot.model.is_some() {
                                mine_uv_phase
                            } else {
                                elapsed
                            };
                            let uv = translated_uv_rect(
                                slot.uv_for_frame_at(frame, uv_elapsed),
                                mine_translation,
                            );
                            let size = scale_mine_slot_for_note(slot);
                            let base_rotation = -slot.def.rotation_deg as f32;
                            // ITG only rotates mines when the actor/model declares it.
                            let sprite_rotation = base_rotation + draw.rot[2] + note_rot;
                            let center = [column_center_x, y_pos];
                            if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                                slot,
                                draw,
                                center,
                                size,
                                uv,
                                base_rotation + note_rot,
                                [1.0, 1.0, 1.0, note_alpha],
                                BlendMode::Alpha,
                                Z_TAP_NOTE as i16,
                                &mut model_cache,
                            ) {
                                actors.push(actor_with_world_z(model_actor, note_world_z));
                            } else {
                                actors.push(actor_with_world_z(
                                    act!(sprite(slot.texture_key_handle()):
                                        align(0.5, 0.5):
                                        xy(center[0], center[1]):
                                        setsize(size[0], size[1]):
                                        rotationy(note_rotation_y):
                                        rotationz(sprite_rotation):
                                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                        diffuse(1.0, 1.0, 1.0, note_alpha):
                                        z(Z_TAP_NOTE)
                                    ),
                                    note_world_z,
                                ));
                            }
                        }
                        return;
                    }
                    let tap_note_part = tap_part_for_note_type(note.note_type);
                    let tap_row_flags = state.tap_row_hold_roll_flags[note_index];
                    let tap_replacement_roll =
                        if matches!(note.note_type, NoteType::Tap | NoteType::Lift) {
                            let same_row_has_hold = tap_row_flags & 0b01 != 0;
                            let same_row_has_roll = tap_row_flags & 0b10 != 0;
                            if same_row_has_hold && same_row_has_roll {
                                if draw_hold_same_row && draw_roll_same_row {
                                    Some(!tap_same_row_means_hold)
                                } else if draw_hold_same_row {
                                    Some(false)
                                } else if draw_roll_same_row {
                                    Some(true)
                                } else {
                                    None
                                }
                            } else if same_row_has_hold && draw_hold_same_row {
                                Some(false)
                            } else if same_row_has_roll && draw_roll_same_row {
                                Some(true)
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                    if let Some(use_roll_head) = tap_replacement_roll {
                        let visuals = ns.hold_visuals_for_col(col_idx, use_roll_head);
                        let part = if use_roll_head {
                            NoteAnimPart::RollHead
                        } else {
                            NoteAnimPart::HoldHead
                        };
                        if let Some(head_slots) = visuals
                            .head_inactive_layers
                            .as_deref()
                            .or(visuals.head_active_layers.as_deref())
                        {
                            let head_phase =
                                ns.part_uv_phase(part, elapsed, current_beat, note.beat);
                            let head_translation = ns.part_uv_translation(part, note.beat, false);
                            let note_scale = col_note_scale;
                            let center = [column_center_x, y_pos];
                            for head_slot in head_slots.iter() {
                                let draw = song_lua_note_model_draw(
                                    head_slot.model_draw_at(elapsed, current_beat),
                                    note_rotation_y,
                                );
                                if !draw.visible {
                                    continue;
                                }
                                let note_frame = head_slot.frame_index_from_phase(head_phase);
                                let uv_elapsed = if head_slot.model.is_some() {
                                    head_phase
                                } else {
                                    elapsed
                                };
                                let note_uv = translated_uv_rect(
                                    head_slot.uv_for_frame_at(note_frame, uv_elapsed),
                                    head_translation,
                                );
                                let base_size = note_slot_base_size(head_slot, note_scale);
                                let local_offset =
                                    [draw.pos[0] * note_scale, draw.pos[1] * note_scale];
                                let local_offset_rot_sin_cos = head_slot.base_rot_sin_cos();
                                let model_center = if head_slot.model.is_some() {
                                    let [sin_r, cos_r] = local_offset_rot_sin_cos;
                                    let offset = [
                                        local_offset[0] * cos_r - local_offset[1] * sin_r,
                                        local_offset[0] * sin_r + local_offset[1] * cos_r,
                                    ];
                                    [center[0] + offset[0], center[1] + offset[1]]
                                } else {
                                    center
                                };
                                let note_size = [
                                    base_size[0] * draw.zoom[0].max(0.0),
                                    base_size[1] * draw.zoom[1].max(0.0),
                                ];
                                if note_size[0] <= f32::EPSILON || note_size[1] <= f32::EPSILON {
                                    continue;
                                }
                                let blend = if draw.blend_add {
                                    BlendMode::Add
                                } else {
                                    BlendMode::Alpha
                                };
                                let color = [
                                    draw.tint[0],
                                    draw.tint[1],
                                    draw.tint[2],
                                    draw.tint[3] * note_alpha,
                                ];
                                if !prefer_sprite_note_path
                                    && let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                                        head_slot,
                                        draw,
                                        model_center,
                                        note_size,
                                        note_uv,
                                        -head_slot.def.rotation_deg as f32 + note_rot,
                                        color,
                                        blend,
                                        Z_TAP_NOTE as i16,
                                        &mut model_cache,
                                    )
                                {
                                    actors.push(actor_with_world_z(model_actor, note_world_z));
                                } else {
                                    let sprite_center = offset_center(
                                        center,
                                        local_offset,
                                        local_offset_rot_sin_cos,
                                    );
                                    if draw.blend_add {
                                        actors.push(actor_with_world_z(
                                            act!(sprite(head_slot.texture_key_handle()):
                                                align(0.5, 0.5):
                                                xy(sprite_center[0], sprite_center[1]):
                                                setsize(note_size[0], note_size[1]):
                                                rotationy(flat_tap_face_rotation_y):
                                                rotationz(draw.rot[2] - head_slot.def.rotation_deg as f32 + note_rot):
                                                customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                                                diffuse(color[0], color[1], color[2], color[3]):
                                                blend(add):
                                                z(Z_TAP_NOTE)
                                            ),
                                            note_world_z,
                                        ));
                                    } else {
                                        actors.push(actor_with_world_z(
                                            act!(sprite(head_slot.texture_key_handle()):
                                                align(0.5, 0.5):
                                                xy(sprite_center[0], sprite_center[1]):
                                                setsize(note_size[0], note_size[1]):
                                                rotationy(flat_tap_face_rotation_y):
                                                rotationz(draw.rot[2] - head_slot.def.rotation_deg as f32 + note_rot):
                                                customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                                                diffuse(color[0], color[1], color[2], color[3]):
                                                blend(normal):
                                                z(Z_TAP_NOTE)
                                            ),
                                            note_world_z,
                                        ));
                                    }
                                }
                                push_note_glow_actor(
                                    &mut actors,
                                    NoteGlowDraw {
                                        slot: head_slot,
                                        draw,
                                        model_center,
                                        sprite_center: offset_center(
                                            center,
                                            local_offset,
                                            local_offset_rot_sin_cos,
                                        ),
                                        size: note_size,
                                        uv: note_uv,
                                        rotation_y: flat_tap_face_rotation_y,
                                        model_rotation_z: -head_slot.def.rotation_deg as f32
                                            + note_rot,
                                        sprite_rotation_z: draw.rot[2]
                                            - head_slot.def.rotation_deg as f32
                                            + note_rot,
                                        alpha: note_glow,
                                        blend,
                                        z: Z_TAP_NOTE as i16,
                                        world_z: note_world_z,
                                        prefer_sprite: prefer_sprite_note_path,
                                    },
                                    &mut model_cache,
                                );
                            }
                            return;
                        }
                        if let Some(head_slot) = visuals
                            .head_inactive
                            .as_ref()
                            .or(visuals.head_active.as_ref())
                        {
                            let part = if use_roll_head {
                                NoteAnimPart::RollHead
                            } else {
                                NoteAnimPart::HoldHead
                            };
                            let head_phase =
                                ns.part_uv_phase(part, elapsed, current_beat, note.beat);
                            let head_translation = ns.part_uv_translation(part, note.beat, false);
                            let note_frame = head_slot.frame_index_from_phase(head_phase);
                            let uv_elapsed = if head_slot.model.is_some() {
                                head_phase
                            } else {
                                elapsed
                            };
                            let note_uv = translated_uv_rect(
                                head_slot.uv_for_frame_at(note_frame, uv_elapsed),
                                head_translation,
                            );
                            let note_scale = col_note_scale;
                            let note_size = note_slot_base_size(head_slot, note_scale);
                            let center = [column_center_x, y_pos];
                            let draw = song_lua_note_model_draw(
                                head_slot.model_draw_at(elapsed, current_beat),
                                note_rotation_y,
                            );
                            if !prefer_sprite_note_path
                                && let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                                    head_slot,
                                    draw,
                                    center,
                                    note_size,
                                    note_uv,
                                    -head_slot.def.rotation_deg as f32 + note_rot,
                                    [1.0, 1.0, 1.0, note_alpha],
                                    BlendMode::Alpha,
                                    Z_TAP_NOTE as i16,
                                    &mut model_cache,
                                )
                            {
                                actors.push(actor_with_world_z(model_actor, note_world_z));
                            } else {
                                actors.push(actor_with_world_z(
                                act!(sprite(head_slot.texture_key_handle()):
                                    align(0.5, 0.5):
                                    xy(center[0], center[1]):
                                    setsize(note_size[0], note_size[1]):
                                    rotationy(flat_tap_face_rotation_y):
                                    rotationz(-head_slot.def.rotation_deg as f32 + note_rot):
                                    customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                                    diffuse(1.0, 1.0, 1.0, note_alpha):
                                    z(Z_TAP_NOTE)
                                ),
                                note_world_z,
                            ));
                            }
                            push_note_glow_actor(
                                &mut actors,
                                NoteGlowDraw {
                                    slot: head_slot,
                                    draw,
                                    model_center: center,
                                    sprite_center: center,
                                    size: note_size,
                                    uv: note_uv,
                                    rotation_y: flat_tap_face_rotation_y,
                                    model_rotation_z: -head_slot.def.rotation_deg as f32 + note_rot,
                                    sprite_rotation_z: -head_slot.def.rotation_deg as f32
                                        + note_rot,
                                    alpha: note_glow,
                                    blend: BlendMode::Alpha,
                                    z: Z_TAP_NOTE as i16,
                                    world_z: note_world_z,
                                    prefer_sprite: prefer_sprite_note_path,
                                },
                                &mut model_cache,
                            );
                            return;
                        }
                    }
                    let note_idx = col_idx * NUM_QUANTIZATIONS + note.quantization_idx as usize;
                    let tap_note_translation =
                        ns.part_uv_translation(tap_note_part, note.beat, false);
                    let lift_layers = if note.note_type == NoteType::Lift {
                        ns.lift_note_layers.get(note_idx)
                    } else {
                        None
                    };
                    if let Some(note_slots) = lift_layers.or_else(|| ns.note_layers.get(note_idx)) {
                        let note_center = [column_center_x, y_pos];
                        let note_uv_phase =
                            ns.part_uv_phase(tap_note_part, elapsed, current_beat, note.beat);
                        let note_scale = col_note_scale;
                        for note_slot in note_slots.iter() {
                            let draw = song_lua_note_model_draw(
                                note_slot.model_draw_at(elapsed, current_beat),
                                note_rotation_y,
                            );
                            if !draw.visible {
                                continue;
                            }
                            let note_frame = note_slot.frame_index_from_phase(note_uv_phase);
                            let uv_elapsed = if note_slot.model.is_some() {
                                note_uv_phase
                            } else {
                                elapsed
                            };
                            let note_uv = translated_uv_rect(
                                note_slot.uv_for_frame_at(note_frame, uv_elapsed),
                                tap_note_translation,
                            );
                            let base_size = note_slot_base_size(note_slot, note_scale);
                            let offset_scale = note_scale;
                            let local_offset =
                                [draw.pos[0] * offset_scale, draw.pos[1] * offset_scale];
                            let local_offset_rot_sin_cos = note_slot.base_rot_sin_cos();
                            let model_center = if note_slot.model.is_some() {
                                let [sin_r, cos_r] = local_offset_rot_sin_cos;
                                let offset = [
                                    local_offset[0] * cos_r - local_offset[1] * sin_r,
                                    local_offset[0] * sin_r + local_offset[1] * cos_r,
                                ];
                                [note_center[0] + offset[0], note_center[1] + offset[1]]
                            } else {
                                note_center
                            };
                            let note_size = [
                                base_size[0] * draw.zoom[0].max(0.0),
                                base_size[1] * draw.zoom[1].max(0.0),
                            ];
                            if note_size[0] <= f32::EPSILON || note_size[1] <= f32::EPSILON {
                                continue;
                            }
                            let layer_z = Z_TAP_NOTE;
                            let blend = if draw.blend_add {
                                BlendMode::Add
                            } else {
                                BlendMode::Alpha
                            };
                            let color = [
                                draw.tint[0],
                                draw.tint[1],
                                draw.tint[2],
                                draw.tint[3] * note_alpha,
                            ];
                            if !prefer_sprite_note_path
                                && let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                                    note_slot,
                                    draw,
                                    model_center,
                                    note_size,
                                    note_uv,
                                    -note_slot.def.rotation_deg as f32 + note_rot,
                                    color,
                                    blend,
                                    layer_z as i16,
                                    &mut model_cache,
                                )
                            {
                                actors.push(actor_with_world_z(model_actor, note_world_z));
                            } else {
                                let sprite_center = offset_center(
                                    note_center,
                                    local_offset,
                                    local_offset_rot_sin_cos,
                                );
                                if draw.blend_add {
                                    actors.push(actor_with_world_z(
                                    act!(sprite(note_slot.texture_key_handle()):
                                        align(0.5, 0.5):
                                        xy(sprite_center[0], sprite_center[1]):
                                        setsize(note_size[0], note_size[1]):
                                        rotationy(flat_tap_face_rotation_y):
                                        rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32 + note_rot):
                                        customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                                        diffuse(color[0], color[1], color[2], color[3]):
                                        blend(add):
                                        z(layer_z)
                                    ),
                                    note_world_z,
                                ));
                                } else {
                                    actors.push(actor_with_world_z(
                                    act!(sprite(note_slot.texture_key_handle()):
                                        align(0.5, 0.5):
                                        xy(sprite_center[0], sprite_center[1]):
                                        setsize(note_size[0], note_size[1]):
                                        rotationy(flat_tap_face_rotation_y):
                                        rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32 + note_rot):
                                        customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                                        diffuse(color[0], color[1], color[2], color[3]):
                                        blend(normal):
                                        z(layer_z)
                                    ),
                                    note_world_z,
                                ));
                                }
                            }
                            push_note_glow_actor(
                                &mut actors,
                                NoteGlowDraw {
                                    slot: note_slot,
                                    draw,
                                    model_center,
                                    sprite_center: offset_center(
                                        note_center,
                                        local_offset,
                                        local_offset_rot_sin_cos,
                                    ),
                                    size: note_size,
                                    uv: note_uv,
                                    rotation_y: flat_tap_face_rotation_y,
                                    model_rotation_z: -note_slot.def.rotation_deg as f32 + note_rot,
                                    sprite_rotation_z: draw.rot[2]
                                        - note_slot.def.rotation_deg as f32
                                        + note_rot,
                                    alpha: note_glow,
                                    blend,
                                    z: layer_z as i16,
                                    world_z: note_world_z,
                                    prefer_sprite: prefer_sprite_note_path,
                                },
                                &mut model_cache,
                            );
                        }
                    } else if let Some(note_slot) = ns.notes.get(note_idx) {
                        let note_uv_phase =
                            ns.part_uv_phase(tap_note_part, elapsed, current_beat, note.beat);
                        let note_frame = note_slot.frame_index_from_phase(note_uv_phase);
                        let uv_elapsed = if note_slot.model.is_some() {
                            note_uv_phase
                        } else {
                            elapsed
                        };
                        let note_uv = translated_uv_rect(
                            note_slot.uv_for_frame_at(note_frame, uv_elapsed),
                            tap_note_translation,
                        );
                        let note_size =
                            scale_sprite_to_arrow(note_slot.size(), col_target_arrow_px);
                        let center = [column_center_x, y_pos];
                        let draw = song_lua_note_model_draw(
                            note_slot.model_draw_at(elapsed, current_beat),
                            note_rotation_y,
                        );
                        if !prefer_sprite_note_path
                            && let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                                note_slot,
                                draw,
                                center,
                                note_size,
                                note_uv,
                                -note_slot.def.rotation_deg as f32 + note_rot,
                                [1.0, 1.0, 1.0, note_alpha],
                                BlendMode::Alpha,
                                Z_TAP_NOTE as i16,
                                &mut model_cache,
                            )
                        {
                            actors.push(actor_with_world_z(model_actor, note_world_z));
                        } else {
                            actors.push(actor_with_world_z(
                            act!(sprite(note_slot.texture_key_handle()):
                                align(0.5, 0.5):
                                xy(center[0], center[1]):
                                setsize(note_size[0], note_size[1]):
                                rotationy(flat_tap_face_rotation_y):
                                rotationz(-note_slot.def.rotation_deg as f32 + note_rot):
                                customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                                diffuse(1.0, 1.0, 1.0, note_alpha):
                                z(Z_TAP_NOTE)
                            ),
                            note_world_z,
                        ));
                        }
                        push_note_glow_actor(
                            &mut actors,
                            NoteGlowDraw {
                                slot: note_slot,
                                draw,
                                model_center: center,
                                sprite_center: center,
                                size: note_size,
                                uv: note_uv,
                                rotation_y: flat_tap_face_rotation_y,
                                model_rotation_z: -note_slot.def.rotation_deg as f32 + note_rot,
                                sprite_rotation_z: -note_slot.def.rotation_deg as f32 + note_rot,
                                alpha: note_glow,
                                blend: BlendMode::Alpha,
                                z: Z_TAP_NOTE as i16,
                                world_z: note_world_z,
                                prefer_sprite: prefer_sprite_note_path,
                            },
                            &mut model_cache,
                        );
                    }
                },
            );
        }
    }
    // Simply Love: ScreenGameplay underlay/PerPlayer/NoteField/DisplayMods.lua
    // shows the current mod string for 5s, then decelerates out over 0.5s.
    // Arrow Cloud/zmod add a CMod warning below this block for ITL no-CMod charts.
    if !view.hide_display_mods {
        // Simply Love DisplayMods.lua uses sleep(5), but ScreenGameplay in/default.lua
        // keeps a full-screen intro cover up for 2.0s. Since deadsync's gameplay
        // in-transition cover is shorter, subtract the exact missing cover time so
        // the *visible* mods duration matches ITG/SL.
        const SL_DISPLAY_MODS_HOLD_S: f32 = 5.0;
        const SL_GAMEPLAY_IN_COVER_S: f32 = 2.0;
        const MODS_FADE_S: f32 = 0.5;
        let hold_adjust = (SL_GAMEPLAY_IN_COVER_S - TRANSITION_IN_DURATION).max(0.0);
        let mods_hold_s = (SL_DISPLAY_MODS_HOLD_S - hold_adjust).max(0.0);

        let alpha = if elapsed_screen <= mods_hold_s {
            1.0
        } else if elapsed_screen < mods_hold_s + MODS_FADE_S {
            let t = ((elapsed_screen - mods_hold_s) / MODS_FADE_S).clamp(0.0, 1.0);
            let decelerate = 1.0 - (1.0 - t) * (1.0 - t);
            1.0 - decelerate
        } else {
            0.0
        };

        if alpha > 0.0 {
            let mods_text = gameplay_mods_text(state, player_idx);
            let mods_line_y =
                screen_height() * 0.25 * 1.3 + DISPLAY_MODS_LINE_STEP + notefield_offset_y;
            let mods_line_count = mods_text
                .split(", ")
                .filter(|part| !part.is_empty())
                .count()
                .max(1) as f32;
            if !mods_text.is_empty() {
                hud_actors.push(act!(text:
                    font("miso"): settext(mods_text):
                    align(0.5, 0.5): xy(playfield_center_x, mods_line_y):
                    zoom(DISPLAY_MODS_ZOOM): wrapwidthpixels(DISPLAY_MODS_WRAP_WIDTH_PX): horizalign(center):
                    shadowcolor(0.0, 0.0, 0.0, 1.0):
                    shadowlength(1.0):
                    diffuse(1.0, 1.0, 1.0, alpha):
                    z(84)
                ));
            }
            if scores::should_warn_cmod_for_itl_chart(state, player_idx) {
                let warning_y = mods_line_y + DISPLAY_MODS_LINE_STEP * mods_line_count;
                hud_actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(playfield_center_x, warning_y):
                    setsize(DISPLAY_MODS_WARNING_W, DISPLAY_MODS_WARNING_H):
                    diffuse(0.0, 0.0, 0.0, 0.8 * alpha):
                    z(84)
                ));
                hud_actors.push(act!(text:
                    font("miso"): settext("CMod On"):
                    align(0.5, 0.5): xy(playfield_center_x, warning_y):
                    zoom(DISPLAY_MODS_WARNING_ZOOM):
                    diffuse(1.0, 0.0, 0.0, alpha):
                    z(85)
                ));
            }
        }
    }

    let combo_capture_start = hud_actors.len();
    // Combo Milestone Explosions (100 / 1000 combo)
    if !blind_active
        && !profile.hide_combo
        && !profile.hide_combo_explosions
        && !p.combo_milestones.is_empty()
    {
        let combo_center_x = playfield_center_x;
        let combo_center_y = zmod_layout.combo_y;
        let player_color = color::decorative_rgba(state.player_color_index);
        let ease_out_quad = |t: f32| -> f32 {
            let t = t.clamp(0.0, 1.0);
            1.0 - (1.0 - t).powi(2)
        };
        for milestone in &p.combo_milestones {
            match milestone.kind {
                ComboMilestoneKind::Hundred => {
                    let elapsed = milestone.elapsed;
                    let explosion_duration = 0.5_f32;
                    if elapsed <= explosion_duration {
                        let progress = (elapsed / explosion_duration).clamp(0.0, 1.0);
                        let zoom = (2.0 - progress) * combo_zoom_mod;
                        let alpha = (0.5 * (1.0 - progress)).max(0.0);
                        for &direction in &[1.0_f32, -1.0_f32] {
                            let rotation = 90.0 * direction * progress;
                            hud_actors.push(act!(sprite("combo_explosion.png"):
                                align(0.5, 0.5):
                                xy(combo_center_x, combo_center_y):
                                zoom(zoom):
                                rotationz(rotation):
                                diffuse(1.0, 1.0, 1.0, alpha):
                                blend(add):
                                z(89)
                            ));
                        }
                    }
                    if elapsed <= COMBO_HUNDRED_MILESTONE_DURATION {
                        let progress = (elapsed / COMBO_HUNDRED_MILESTONE_DURATION).clamp(0.0, 1.0);
                        let eased = ease_out_quad(progress);
                        let zoom = (0.25 + (2.0 - 0.25) * eased) * combo_zoom_mod;
                        let alpha = (0.6 * (1.0 - eased)).max(0.0);
                        let rotation = 10.0 + (0.0 - 10.0) * eased;
                        hud_actors.push(act!(sprite("combo_100milestone_splode.png"):
                            align(0.5, 0.5):
                            xy(combo_center_x, combo_center_y):
                            zoom(zoom):
                            rotationz(rotation):
                            diffuse(player_color[0], player_color[1], player_color[2], alpha):
                            blend(add):
                            z(89)
                        ));
                        let mini_duration = 0.4_f32;
                        if elapsed <= mini_duration {
                            let mini_progress = (elapsed / mini_duration).clamp(0.0, 1.0);
                            let mini_zoom = (0.25 + (1.8 - 0.25) * mini_progress) * combo_zoom_mod;
                            let mini_alpha = (1.0 - mini_progress).max(0.0);
                            let mini_rotation = 10.0 + (0.0 - 10.0) * mini_progress;
                            hud_actors.push(act!(sprite("combo_100milestone_minisplode.png"):
                                align(0.5, 0.5):
                                xy(combo_center_x, combo_center_y):
                                zoom(mini_zoom):
                                rotationz(mini_rotation):
                                diffuse(player_color[0], player_color[1], player_color[2], mini_alpha):
                                blend(add):
                                z(89)
                            ));
                        }
                    }
                }
                ComboMilestoneKind::Thousand => {
                    let elapsed = milestone.elapsed;
                    if elapsed <= COMBO_THOUSAND_MILESTONE_DURATION {
                        let progress =
                            (elapsed / COMBO_THOUSAND_MILESTONE_DURATION).clamp(0.0, 1.0);
                        let zoom = (0.25 + (3.0 - 0.25) * progress) * combo_zoom_mod;
                        let alpha = (0.7 * (1.0 - progress)).max(0.0);
                        let x_offset = 100.0 * progress * combo_zoom_mod;
                        for &direction in &[1.0_f32, -1.0_f32] {
                            let final_x = combo_center_x + x_offset * direction;
                            hud_actors.push(act!(sprite("combo_1000milestone_swoosh.png"):
                                align(0.5, 0.5):
                                xy(final_x, combo_center_y):
                                zoom(zoom):
                                zoomx(zoom * direction):
                                diffuse(player_color[0], player_color[1], player_color[2], alpha):
                                blend(add):
                                z(89)
                            ));
                        }
                    }
                }
            }
        }
    }
    // Combo
    if !blind_active && !profile.hide_combo {
        let combo_y = zmod_layout.combo_y;
        let combo_font_name = zmod_combo_font_name(profile.combo_font);
        if p.miss_combo >= SHOW_COMBO_AT {
            if let Some(font_name) = combo_font_name {
                hud_actors.push(act!(text:
                    font(font_name): settext(cached_int_u32(p.miss_combo)):
                    align(0.5, 0.5): xy(combo_x, combo_y):
                    zoom(0.75 * combo_zoom_mod): horizalign(center): shadowlength(1.0):
                    diffuse(1.0, 0.0, 0.0, 1.0):
                    z(90)
                ));
            }
        } else if p.combo >= SHOW_COMBO_AT {
            let quint_active = zmod_combo_quint_active(state, player_idx, profile);
            let final_color = match profile.combo_colors {
                profile::ComboColors::None => [1.0, 1.0, 1.0, 1.0],
                profile::ComboColors::Rainbow => {
                    if profile.combo_mode == profile::ComboMode::FullCombo {
                        if matches!(
                            p.full_combo_grade,
                            Some(JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great)
                        ) {
                            zmod_combo_rainbow_color(state.total_elapsed_in_screen, false, p.combo)
                        } else {
                            [1.0, 1.0, 1.0, 1.0]
                        }
                    } else {
                        zmod_combo_rainbow_color(state.total_elapsed_in_screen, false, p.combo)
                    }
                }
                profile::ComboColors::RainbowScroll => {
                    if profile.combo_mode == profile::ComboMode::FullCombo {
                        if matches!(
                            p.full_combo_grade,
                            Some(JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great)
                        ) {
                            zmod_combo_rainbow_color(state.total_elapsed_in_screen, true, p.combo)
                        } else {
                            [1.0, 1.0, 1.0, 1.0]
                        }
                    } else {
                        zmod_combo_rainbow_color(state.total_elapsed_in_screen, true, p.combo)
                    }
                }
                profile::ComboColors::Glow => {
                    let combo_grade = if profile.combo_mode == profile::ComboMode::FullCombo {
                        p.full_combo_grade
                    } else {
                        p.current_combo_grade
                    };
                    if let Some(grade) = combo_grade {
                        let (color1, color2) = zmod_combo_glow_pair(
                            grade,
                            quint_active && grade == JudgeGrade::Fantastic,
                        );
                        zmod_combo_glow_color(color1, color2, state.total_elapsed_in_screen)
                    } else {
                        [1.0, 1.0, 1.0, 1.0]
                    }
                }
                profile::ComboColors::Solid => {
                    let combo_grade = if profile.combo_mode == profile::ComboMode::FullCombo {
                        p.full_combo_grade
                    } else {
                        p.current_combo_grade
                    };
                    if let Some(grade) = combo_grade {
                        zmod_combo_solid_color(
                            grade,
                            quint_active && grade == JudgeGrade::Fantastic,
                        )
                    } else {
                        [1.0, 1.0, 1.0, 1.0]
                    }
                }
            };
            if let Some(font_name) = combo_font_name {
                hud_actors.push(act!(text:
                    font(font_name): settext(cached_int_u32(p.combo)):
                    align(0.5, 0.5): xy(combo_x, combo_y):
                    zoom(0.75 * combo_zoom_mod): horizalign(center): shadowlength(1.0):
                    diffuse(final_color[0], final_color[1], final_color[2], final_color[3]):
                    z(90)
                ));
            }
        }
    }
    let combo_actors = capture_requests
        .combo
        .then(|| share_hud_range(&mut hud_actors, combo_capture_start))
        .flatten();

    let show_error_bar_colorful = error_bar_mask.contains(profile::ErrorBarMask::COLORFUL);
    let show_error_bar_monochrome = error_bar_mask.contains(profile::ErrorBarMask::MONOCHROME);
    let show_error_bar_text = error_bar_mask.contains(profile::ErrorBarMask::TEXT);
    let show_error_bar_highlight = error_bar_mask.contains(profile::ErrorBarMask::HIGHLIGHT);
    let show_error_bar_average = error_bar_mask.contains(profile::ErrorBarMask::AVERAGE);
    let show_error_bar = !error_bar_mask.is_empty();
    let error_bar_y = hud_layout.error_bar_y;
    let error_bar_max_h = hud_layout.error_bar_max_h;
    let error_bar_x = playfield_center_x + error_bar_extra_x;
    let mut average_bar_y = 0.0_f32;
    for y in column_receptor_ys.iter().take(num_cols) {
        average_bar_y += *y;
    }
    if num_cols > 0 {
        average_bar_y /= num_cols as f32;
    }
    let judgment_z = if profile.judgment_back {
        Z_JUDGMENT_BACK
    } else {
        Z_JUDGMENT_FRONT
    };
    let error_bar_bg_z = if profile.judgment_back {
        Z_ERROR_BAR_BG_BACK
    } else {
        Z_ERROR_BAR_BG_FRONT
    };
    let error_bar_band_z = if profile.judgment_back {
        Z_ERROR_BAR_BAND_BACK
    } else {
        Z_ERROR_BAR_BAND_FRONT
    };
    let error_bar_line_z = if profile.judgment_back {
        Z_ERROR_BAR_LINE_BACK
    } else {
        Z_ERROR_BAR_LINE_FRONT
    };
    let error_bar_tick_z = if profile.judgment_back {
        Z_ERROR_BAR_TICK_BACK
    } else {
        Z_ERROR_BAR_TICK_FRONT
    };
    let error_bar_text_z = if profile.judgment_back {
        Z_ERROR_BAR_TEXT_BACK
    } else {
        Z_ERROR_BAR_TEXT_FRONT
    };

    // zmod ExtraAesthetics: offset indicator text (ErrorMSDisplay).
    if !blind_active
        && profile.error_ms_display
        && let Some(text) = p.offset_indicator_text
    {
        let age = elapsed_screen - text.started_at;
        if (0.0..OFFSET_INDICATOR_DUR_S).contains(&age) {
            let mut offset_y = screen_center_y() + notefield_offset_y;
            if show_error_bar {
                let min_sep = error_bar_max_h * 0.5 + 6.0;
                if (offset_y - error_bar_y).abs() < min_sep {
                    offset_y = error_bar_y + min_sep;
                }
            }
            let c = error_bar_color_for_window(text.window, profile.show_fa_plus_window);
            hud_actors.push(act!(text:
                font("wendy"): settext(cached_offset_ms(text.offset_ms)):
                align(0.5, 0.5): xy(playfield_center_x, offset_y):
                zoom(0.25): shadowlength(1.0):
                diffuse(c[0], c[1], c[2], 1.0):
                z(error_bar_text_z)
            ));
        }
    }

    // Error Bar (Simply Love parity)
    if !blind_active && show_error_bar {
        let mut styles = [profile::ErrorBarStyle::None; 4];
        let mut style_count = 0usize;
        if show_error_bar_colorful {
            styles[style_count] = profile::ErrorBarStyle::Colorful;
            style_count += 1;
        }
        if show_error_bar_monochrome {
            styles[style_count] = profile::ErrorBarStyle::Monochrome;
            style_count += 1;
        }
        if show_error_bar_highlight {
            styles[style_count] = profile::ErrorBarStyle::Highlight;
            style_count += 1;
        }
        if show_error_bar_average {
            styles[style_count] = profile::ErrorBarStyle::Average;
            style_count += 1;
        }
        let fa_plus_window_s = Some(crate::game::gameplay::player_fa_plus_window_s(
            state, player_idx,
        ));

        for style in styles.into_iter().take(style_count) {
            match style {
                crate::game::profile::ErrorBarStyle::Monochrome => {
                    let bar_h = error_bar_max_h;
                    let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                    let max_offset_s = state.timing_profile.windows_s[max_window_ix];
                    let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                        (ERROR_BAR_WIDTH_MONOCHROME * 0.5) / max_offset_s
                    } else {
                        0.0
                    };
                    let (bounds_s, bounds_len) = error_bar_boundaries_s(
                        state.timing_profile.windows_s,
                        fa_plus_window_s,
                        profile.show_fa_plus_window,
                        profile.error_bar_trim,
                    );

                    let bg_alpha = if profile.background_filter.is_off() {
                        ERROR_BAR_MONO_BG_ALPHA
                    } else {
                        0.0
                    };
                    if bg_alpha > 0.0 {
                        hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(error_bar_x, error_bar_y):
                            zoomto(ERROR_BAR_WIDTH_MONOCHROME + 2.0, bar_h + 2.0):
                            diffuse(0.0, 0.0, 0.0, bg_alpha):
                            z(error_bar_bg_z)
                        ));
                    }

                    hud_actors.push(act!(quad:
                        align(0.5, 0.5): xy(error_bar_x, error_bar_y):
                        zoomto(2.0, bar_h):
                        diffuse(0.5, 0.5, 0.5, 1.0):
                        z(error_bar_band_z)
                    ));

                    let line_alpha = if elapsed_screen < ERROR_BAR_LINES_FADE_START_S {
                        0.0
                    } else if elapsed_screen
                        < ERROR_BAR_LINES_FADE_START_S + ERROR_BAR_LINES_FADE_DUR_S
                    {
                        let t = (elapsed_screen - ERROR_BAR_LINES_FADE_START_S)
                            / ERROR_BAR_LINES_FADE_DUR_S;
                        ERROR_BAR_LINE_ALPHA * smoothstep01(t)
                    } else {
                        ERROR_BAR_LINE_ALPHA
                    };
                    if line_alpha > 0.0 && wscale.is_finite() && wscale > 0.0 {
                        for &bound in bounds_s.iter().take(bounds_len) {
                            let offset = bound * wscale;
                            if !offset.is_finite() {
                                continue;
                            }
                            for sx in [-1.0_f32, 1.0_f32] {
                                hud_actors.push(act!(quad:
                                    align(0.5, 0.5): xy(error_bar_x + sx * offset, error_bar_y):
                                    zoomto(1.0, bar_h):
                                    diffuse(1.0, 1.0, 1.0, line_alpha):
                                    z(error_bar_line_z)
                                ));
                            }
                        }
                    }

                    let label_fade_out_start_s =
                        ERROR_BAR_LABEL_FADE_DUR_S + ERROR_BAR_LABEL_HOLD_S;
                    let label_alpha = if elapsed_screen < ERROR_BAR_LABEL_FADE_DUR_S {
                        smoothstep01(elapsed_screen / ERROR_BAR_LABEL_FADE_DUR_S)
                    } else if elapsed_screen < label_fade_out_start_s {
                        1.0
                    } else if elapsed_screen < label_fade_out_start_s + ERROR_BAR_LABEL_FADE_DUR_S {
                        1.0 - smoothstep01(
                            (elapsed_screen - label_fade_out_start_s) / ERROR_BAR_LABEL_FADE_DUR_S,
                        )
                    } else {
                        0.0
                    };
                    if label_alpha > 0.0 {
                        let x_off = ERROR_BAR_WIDTH_MONOCHROME * 0.25;
                        hud_actors.push(act!(text:
                            font("game"): settext("Early"):
                            align(0.5, 0.5): xy(error_bar_x - x_off, error_bar_y):
                            zoom(0.7): diffuse(1.0, 1.0, 1.0, label_alpha):
                            z(error_bar_text_z)
                        ));
                        hud_actors.push(act!(text:
                            font("game"): settext("Late"):
                            align(0.5, 0.5): xy(error_bar_x + x_off, error_bar_y):
                            zoom(0.7): diffuse(1.0, 1.0, 1.0, label_alpha):
                            z(error_bar_text_z)
                        ));
                    }

                    if wscale.is_finite() && wscale > 0.0 {
                        let multi_tick = profile.error_bar_multi_tick;
                        for tick_opt in &p.error_bar_mono_ticks {
                            let Some(tick) = tick_opt else {
                                continue;
                            };
                            let alpha = error_bar_tick_alpha(
                                elapsed_screen - tick.started_at,
                                ERROR_BAR_TICK_DUR_MONOCHROME,
                                multi_tick,
                            );
                            if alpha <= 0.0 {
                                continue;
                            }
                            let x = tick.offset_s * wscale;
                            if !x.is_finite() {
                                continue;
                            }
                            let c = error_bar_color_for_window(
                                tick.window,
                                profile.show_fa_plus_window,
                            );
                            hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(error_bar_x + x, error_bar_y):
                                zoomto(ERROR_BAR_TICK_WIDTH, bar_h):
                                diffuse(c[0], c[1], c[2], alpha):
                                z(error_bar_tick_z)
                            ));
                        }
                    }
                }
                crate::game::profile::ErrorBarStyle::Colorful => {
                    let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                    let max_offset_s = state.timing_profile.windows_s[max_window_ix];
                    let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                        (ERROR_BAR_WIDTH_COLORFUL * 0.5) / max_offset_s
                    } else {
                        0.0
                    };
                    let (bounds_s, bounds_len) = error_bar_boundaries_s(
                        state.timing_profile.windows_s,
                        fa_plus_window_s,
                        profile.show_fa_plus_window,
                        profile.error_bar_trim,
                    );

                    let bar_visible = p
                        .error_bar_color_bar_started_at
                        .map(|t0| {
                            let age = elapsed_screen - t0;
                            (0.0..ERROR_BAR_TICK_DUR_COLORFUL).contains(&age)
                        })
                        .unwrap_or(false);

                    if bar_visible && wscale.is_finite() && wscale > 0.0 {
                        hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(error_bar_x, error_bar_y):
                            zoomto(ERROR_BAR_WIDTH_COLORFUL + 4.0, ERROR_BAR_HEIGHT_COLORFUL + 4.0):
                            diffuse(0.0, 0.0, 0.0, 1.0):
                            z(error_bar_bg_z)
                        ));

                        let base = if profile.show_fa_plus_window {
                            0usize
                        } else {
                            1usize
                        };
                        let mut lastx = 0.0_f32;
                        for (i, &bound) in bounds_s.iter().take(bounds_len).enumerate() {
                            let x = bound * wscale;
                            let width = x - lastx;
                            if !x.is_finite() || !width.is_finite() || width <= 0.0 {
                                lastx = x;
                                continue;
                            }
                            let window = timing_window_from_num(base + i);
                            let c = error_bar_color_for_window(window, profile.show_fa_plus_window);

                            let cx_early = -0.5 * (lastx + x);
                            let cx_late = 0.5 * (lastx + x);
                            hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(error_bar_x + cx_early, error_bar_y):
                                zoomto(width, ERROR_BAR_HEIGHT_COLORFUL):
                                diffuse(c[0], c[1], c[2], 1.0):
                                z(error_bar_band_z)
                            ));
                            hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(error_bar_x + cx_late, error_bar_y):
                                zoomto(width, ERROR_BAR_HEIGHT_COLORFUL):
                                diffuse(c[0], c[1], c[2], 1.0):
                                z(error_bar_band_z)
                            ));

                            lastx = x;
                        }
                    }

                    if wscale.is_finite() && wscale > 0.0 {
                        let multi_tick = profile.error_bar_multi_tick;
                        for tick_opt in &p.error_bar_color_ticks {
                            let Some(tick) = tick_opt else {
                                continue;
                            };
                            let alpha = error_bar_tick_alpha(
                                elapsed_screen - tick.started_at,
                                ERROR_BAR_TICK_DUR_COLORFUL,
                                multi_tick,
                            );
                            if alpha <= 0.0 {
                                continue;
                            }
                            let x = tick.offset_s * wscale;
                            if !x.is_finite() {
                                continue;
                            }
                            hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(error_bar_x + x, error_bar_y):
                            zoomto(ERROR_BAR_TICK_WIDTH, ERROR_BAR_HEIGHT_COLORFUL + 4.0):
                            diffuse(ERROR_BAR_COLORFUL_TICK_RGBA[0], ERROR_BAR_COLORFUL_TICK_RGBA[1], ERROR_BAR_COLORFUL_TICK_RGBA[2], alpha):
                            z(error_bar_line_z)
                        ));
                        }
                    }
                }
                crate::game::profile::ErrorBarStyle::Highlight => {
                    let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                    let max_offset_s = state.timing_profile.windows_s[max_window_ix];
                    let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                        (ERROR_BAR_WIDTH_COLORFUL * 0.5) / max_offset_s
                    } else {
                        0.0
                    };
                    let (bounds_s, bounds_len) = error_bar_boundaries_s(
                        state.timing_profile.windows_s,
                        fa_plus_window_s,
                        profile.show_fa_plus_window,
                        profile.error_bar_trim,
                    );

                    let bar_visible = p
                        .error_bar_color_bar_started_at
                        .map(|t0| {
                            let age = elapsed_screen - t0;
                            (0.0..ERROR_BAR_TICK_DUR_COLORFUL).contains(&age)
                        })
                        .unwrap_or(false);

                    if bar_visible && wscale.is_finite() && wscale > 0.0 {
                        hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(error_bar_x, error_bar_y):
                            zoomto(ERROR_BAR_WIDTH_COLORFUL + 4.0, ERROR_BAR_HEIGHT_COLORFUL + 4.0):
                            diffuse(0.0, 0.0, 0.0, 1.0):
                            z(error_bar_bg_z)
                        ));

                        let base = if profile.show_fa_plus_window {
                            0usize
                        } else {
                            1usize
                        };
                        let mut lastx = 0.0_f32;
                        for (i, &bound) in bounds_s.iter().take(bounds_len).enumerate() {
                            let x = bound * wscale;
                            let width = x - lastx;
                            if !x.is_finite() || !width.is_finite() || width <= 0.0 {
                                lastx = x;
                                continue;
                            }
                            let window_num = base + i;
                            let window = timing_window_from_num(window_num);
                            let wi = window_num.min(5);
                            let c = error_bar_color_for_window(window, profile.show_fa_plus_window);
                            let early_a = error_bar_flash_alpha(
                                elapsed_screen,
                                p.error_bar_color_flash_early[wi],
                                ERROR_BAR_TICK_DUR_COLORFUL,
                            );
                            let late_a = error_bar_flash_alpha(
                                elapsed_screen,
                                p.error_bar_color_flash_late[wi],
                                ERROR_BAR_TICK_DUR_COLORFUL,
                            );

                            let cx_early = -0.5 * (lastx + x);
                            let cx_late = 0.5 * (lastx + x);
                            hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(error_bar_x + cx_early, error_bar_y):
                                zoomto(width, ERROR_BAR_HEIGHT_COLORFUL):
                                diffuse(c[0], c[1], c[2], early_a):
                                z(error_bar_band_z)
                            ));
                            hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(error_bar_x + cx_late, error_bar_y):
                                zoomto(width, ERROR_BAR_HEIGHT_COLORFUL):
                                diffuse(c[0], c[1], c[2], late_a):
                                z(error_bar_band_z)
                            ));

                            lastx = x;
                        }
                    }

                    if wscale.is_finite() && wscale > 0.0 {
                        let multi_tick = profile.error_bar_multi_tick;
                        for tick_opt in &p.error_bar_color_ticks {
                            let Some(tick) = tick_opt else {
                                continue;
                            };
                            let alpha = error_bar_tick_alpha(
                                elapsed_screen - tick.started_at,
                                ERROR_BAR_TICK_DUR_COLORFUL,
                                multi_tick,
                            );
                            if alpha <= 0.0 {
                                continue;
                            }
                            let x = tick.offset_s * wscale;
                            if !x.is_finite() {
                                continue;
                            }
                            hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(error_bar_x + x, error_bar_y):
                            zoomto(ERROR_BAR_TICK_WIDTH, ERROR_BAR_HEIGHT_COLORFUL + 4.0):
                            diffuse(ERROR_BAR_COLORFUL_TICK_RGBA[0], ERROR_BAR_COLORFUL_TICK_RGBA[1], ERROR_BAR_COLORFUL_TICK_RGBA[2], alpha):
                            z(error_bar_line_z)
                        ));
                        }
                    }
                }
                crate::game::profile::ErrorBarStyle::Average => {
                    let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                    let max_offset_s = state.timing_profile.windows_s[max_window_ix];
                    let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                        (ERROR_BAR_WIDTH_AVERAGE * 0.5) / max_offset_s
                    } else {
                        0.0
                    };
                    let bar_visible = p
                        .error_bar_avg_bar_started_at
                        .map(|t0| {
                            let age = elapsed_screen - t0;
                            (0.0..ERROR_BAR_TICK_DUR_COLORFUL).contains(&age)
                        })
                        .unwrap_or(false);
                    if bar_visible && wscale.is_finite() && wscale > 0.0 {
                        let tick_h =
                            ERROR_BAR_HEIGHT_AVERAGE + 4.0 + ERROR_BAR_AVERAGE_TICK_EXTRA_H;
                        let multi_tick = profile.error_bar_multi_tick;
                        for tick_opt in &p.error_bar_avg_ticks {
                            let Some(tick) = tick_opt else {
                                continue;
                            };
                            let alpha = error_bar_tick_alpha(
                                elapsed_screen - tick.started_at,
                                ERROR_BAR_TICK_DUR_COLORFUL,
                                multi_tick,
                            );
                            if alpha <= 0.0 {
                                continue;
                            }
                            let x = tick.offset_s * wscale;
                            if !x.is_finite() {
                                continue;
                            }
                            hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(error_bar_x + x, average_bar_y):
                            zoomto(ERROR_BAR_TICK_WIDTH, tick_h):
                            diffuse(ERROR_BAR_COLORFUL_TICK_RGBA[0], ERROR_BAR_COLORFUL_TICK_RGBA[1], ERROR_BAR_COLORFUL_TICK_RGBA[2], alpha):
                            z(error_bar_line_z)
                        ));
                        }
                    }
                }
                crate::game::profile::ErrorBarStyle::Text => {}
                crate::game::profile::ErrorBarStyle::None => {}
            }
        }
        if show_error_bar_text && let Some(text) = p.error_bar_text {
            let age = elapsed_screen - text.started_at;
            if (0.0..ERROR_BAR_TICK_DUR_COLORFUL).contains(&age) {
                let x = if text.early { -40.0 } else { 40.0 };
                let s = if text.early { "EARLY" } else { "LATE" };
                let c = if text.early {
                    ERROR_BAR_TEXT_EARLY_RGBA
                } else {
                    ERROR_BAR_TEXT_LATE_RGBA
                };
                hud_actors.push(act!(text:
                    font("wendy"): settext(s):
                    align(0.5, 0.5): xy(error_bar_x + x, error_bar_y):
                    zoom(0.25): shadowlength(1.0):
                    diffuse(c[0], c[1], c[2], c[3]):
                    z(error_bar_text_z)
                ));
            }
        }
    }

    // Measure Counter / Measure Breakdown (Zmod parity)
    if profile.measure_counter != crate::game::profile::MeasureCounter::None {
        let segs: &[StreamSegment] = &state.measure_counter_segments[player_idx];
        if !segs.is_empty() {
            let lookahead: u8 = profile.measure_counter_lookahead.min(4);
            let multiplier = profile.measure_counter.multiplier();

            let beat_floor = state.current_beat_visible[player_idx].floor();
            let curr_measure = beat_floor / 4.0;
            let base_index = stream_segment_index_exclusive_end(segs, curr_measure);

            let mut column_width = ScrollSpeedSetting::ARROW_SPACING * field_zoom;
            if profile.measure_counter_left {
                column_width *= 4.0 / 3.0;
            }

            if let Some(measure_counter_y) = zmod_layout.measure_counter_y {
                for j in (0..=lookahead).rev() {
                    let seg_index_unshifted = base_index + j as usize;
                    if seg_index_unshifted >= segs.len() {
                        continue;
                    }

                    let is_lookahead = j != 0;
                    let text = zmod_measure_counter_text(
                        beat_floor,
                        curr_measure,
                        segs,
                        seg_index_unshifted,
                        is_lookahead,
                        lookahead,
                        multiplier,
                    );
                    let Some(text) = text else { continue };

                    let seg_unshifted = segs[seg_index_unshifted];
                    let rgba = if seg_unshifted.is_break {
                        if is_lookahead {
                            [0.4, 0.4, 0.4, 1.0]
                        } else {
                            [0.5, 0.5, 0.5, 1.0]
                        }
                    } else if is_lookahead {
                        [0.45, 0.45, 0.45, 1.0]
                    } else if text.contains('/') {
                        [1.0, 1.0, 1.0, 1.0]
                    } else {
                        [0.5, 0.5, 0.5, 1.0]
                    };

                    let zoom = 0.35 - 0.05 * (j as f32);
                    let mut x = playfield_center_x;
                    let mut y = measure_counter_y;

                    if profile.measure_counter_vert {
                        y += 20.0 * (j as f32);
                    } else {
                        let denom = if lookahead == 0 {
                            1.0
                        } else {
                            lookahead as f32
                        };
                        x += (column_width / denom) * 2.0 * (j as f32);
                    }
                    if profile.measure_counter_left {
                        x -= column_width;
                    }

                    hud_actors.push(act!(text:
                        font(mc_font_name): settext(text):
                        align(0.5, 0.5): xy(x, y):
                        zoom(zoom): horizalign(center): shadowlength(1.0):
                        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
                        z(85)
                    ));
                }

                // Broken Run Total (Zmod BrokenRunCounter.lua)
                if profile.broken_run
                    && let Some((broken_index, broken_end, is_broken)) =
                        zmod_broken_run_segment(segs, curr_measure)
                {
                    let seg0 = segs[broken_index];
                    if !seg0.is_break && is_broken {
                        let curr_count = (curr_measure - (seg0.start as f32)).floor() as i32 + 1;
                        let len = (broken_end - seg0.start) as i32;
                        let text = if curr_measure < 0.0 {
                            // BrokenRunCounter.lua special-cases negative time.
                            let first = segs[0];
                            if first.is_break {
                                let first_len = (first.end - first.start) as i32;
                                let v = (-curr_measure).floor() as i32 + 1 + first_len;
                                cached_paren_i32(v)
                            } else {
                                let v = (-curr_measure).floor() as i32 + 1;
                                cached_paren_i32(v)
                            }
                        } else if curr_count != 0 {
                            cached_ratio_i32(curr_count, len)
                        } else {
                            cached_int_i32(len)
                        };

                        if text.contains('/') {
                            let mut x = playfield_center_x;
                            let mut y = measure_counter_y + 15.0;
                            if profile.measure_counter_vert {
                                y -= 15.0;
                                x += column_width * (4.0 / 3.0);
                            }
                            if profile.measure_counter_left {
                                x -= column_width;
                            }

                            hud_actors.push(act!(text:
                                font(mc_font_name): settext(text):
                                align(0.5, 0.5): xy(x, y):
                                zoom(0.35): horizalign(center): shadowlength(1.0):
                                diffuse(1.0, 1.0, 1.0, 0.7):
                                z(85)
                            ));
                        }
                    }
                }
            }

            // Run Timer (Zmod RunTimer.lua: TimerMode=Time only)
            if profile.run_timer
                && let Some(stream_index) = zmod_run_timer_index(segs, curr_measure)
            {
                let seg = segs[stream_index];
                if !seg.is_break {
                    let cur_bps = state.timing.get_bpm_for_beat(state.current_beat_display) / 60.0;
                    let rate = state.music_rate;
                    if cur_bps.is_finite() && cur_bps > 0.0 && rate.is_finite() && rate > 0.0 {
                        let measure_seconds = 4.0 / (cur_bps * rate);
                        let curr_time = state.current_beat_display / (cur_bps * rate);

                        let seg_len_s =
                            (((seg.end - seg.start) as f32) * measure_seconds).ceil() as i32;
                        let total = zmod_run_timer_fmt(seg_len_s, 60, false);

                        let remaining_s =
                            (((seg.end as f32) * measure_seconds) - curr_time).ceil() as i32;
                        let remaining_s = remaining_s.max(0);

                        let text = if remaining_s > seg_len_s {
                            total
                        } else if remaining_s < 1 {
                            zmod_run_timer_fmt(0, 59, true)
                        } else {
                            zmod_run_timer_fmt(remaining_s, 59, true)
                        };

                        let active = text.contains(' ');
                        let rgba = if active {
                            [1.0, 1.0, 1.0, 1.0]
                        } else {
                            [0.5, 0.5, 0.5, 1.0]
                        };

                        let mut x = playfield_center_x;
                        if profile.measure_counter_left {
                            x -= column_width;
                        }
                        let y = zmod_layout.subtractive_scoring_y;

                        hud_actors.push(act!(text:
                            font(mc_font_name): settext(text):
                            align(0.5, 0.5): xy(x, y):
                            zoom(0.35): horizalign(center): shadowlength(1.0):
                            diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
                            z(85)
                        ));
                    }
                }
            }
        }
    }

    // Mini Indicator (zmod SubtractiveScoring.lua parity).
    if let Some((text, rgba)) = zmod_mini_indicator_text(state, p, profile, player_idx) {
        let column_width = ScrollSpeedSetting::ARROW_SPACING * field_zoom;
        let mut x = playfield_center_x + column_width;
        let mut h_align = 0.5;
        if !profile.measure_counter_left {
            h_align = 0.0;
            x -= 12.0;
        }

        hud_actors.push(act!(text:
            font(mc_font_name): settext(text):
            align(h_align, 0.5): xy(x, zmod_layout.subtractive_scoring_y):
            zoom(0.35): shadowlength(1.0):
            diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
            z(85)
        ));
    }

    let judgment_capture_start = hud_actors.len();
    // Judgment Sprite (tap judgments)
    if !blind_active && let Some(render_info) = &p.last_judgment {
        if let Some(judgment_texture) = resolved_judgment_texture(profile) {
            let (frame_cols, frame_rows) =
                assets::parse_sprite_sheet_dims(judgment_texture.key.as_ref());
            let judgment = &render_info.judgment;
            let elapsed = (elapsed_screen - render_info.started_at_screen_s).max(0.0);
            if elapsed < 0.9 {
                let zoom = if elapsed < 0.1 {
                    let t = elapsed / 0.1;
                    let ease_t = 1.0 - (1.0 - t).powi(2);
                    0.8 + (0.75 - 0.8) * ease_t
                } else if elapsed < 0.7 {
                    0.75
                } else {
                    let t = (elapsed - 0.7) / 0.2;
                    let ease_t = t.powi(2);
                    0.75 * (1.0 - ease_t)
                } * judgment_zoom_mod;
                let offset_sec = judgment.time_error_ms / 1000.0;
                let (frame_row, overlay_row) =
                    tap_judgment_rows(profile, judgment, frame_rows as usize);
                let frame_offset = if offset_sec < 0.0 { 0 } else { 1 };
                let columns = frame_cols.max(1) as usize;
                let col_index = if columns > 1 { frame_offset } else { 0 };
                let linear_index = (frame_row * columns + col_index) as u32;
                let rot_deg = judgment_tilt_rotation_deg(profile, judgment);
                hud_actors.push(act!(sprite(judgment_texture.texture_key_handle()):
                    align(0.5, 0.5): xy(judgment_x, judgment_y):
                    z(judgment_z): rotationz(rot_deg): setsize(0.0, 76.0): setstate(linear_index): zoom(zoom)
                ));
                if let Some(overlay_row) = overlay_row {
                    let overlay_index = (overlay_row * columns + col_index) as u32;
                    hud_actors.push(act!(sprite(judgment_texture.texture_key_handle()):
                        align(0.5, 0.5): xy(judgment_x, judgment_y):
                        z(judgment_z): rotationz(rot_deg): setsize(0.0, 76.0): setstate(overlay_index): zoom(zoom):
                        diffuse(1.0, 1.0, 1.0, SPLIT_15_10MS_OVERLAY_ALPHA)
                    ));
                }
            }
        }
    }
    for (i, hold_judgment) in state.hold_judgments[col_start..col_start + num_cols]
        .iter()
        .enumerate()
    {
        if blind_active {
            continue;
        }
        let Some(render_info) = hold_judgment.as_ref() else {
            continue;
        };
        let elapsed = (elapsed_screen - render_info.started_at_screen_s).max(0.0);
        if elapsed >= HOLD_JUDGMENT_TOTAL_DURATION {
            continue;
        }
        let zoom = if elapsed < 0.3 {
            let progress = (elapsed / 0.3).clamp(0.0, 1.0);
            (HOLD_JUDGMENT_INITIAL_ZOOM
                + progress * (HOLD_JUDGMENT_FINAL_ZOOM - HOLD_JUDGMENT_INITIAL_ZOOM))
                * judgment_zoom_mod
        } else {
            HOLD_JUDGMENT_FINAL_ZOOM * judgment_zoom_mod
        };
        let frame_index = match render_info.result {
            HoldResult::Held => 0,
            HoldResult::LetGo => 1,
            HoldResult::Missed => 1,
        } as u32;
        if let Some(texture) = hold_judgment_texture {
            let hold_judgment_y = sm_scale(
                column_reverse_percent[i],
                0.0,
                1.0,
                receptor_y_normal + HOLD_JUDGMENT_OFFSET_FROM_RECEPTOR,
                receptor_y_reverse - HOLD_JUDGMENT_OFFSET_FROM_RECEPTOR,
            );
            let column_offset = state.noteskin[player_idx]
                .as_ref()
                .and_then(|ns| ns.column_xs.get(i))
                .map(|&x| x as f32 * spacing_mult)
                .unwrap_or_else(|| ((i as f32) - 1.5) * TARGET_ARROW_PIXEL_SIZE * field_zoom);
            hud_actors.push(act!(sprite(texture.texture_key_handle()):
                align(0.5, 0.5):
                xy(judgment_x + column_offset, hold_judgment_y):
                z(195):
                setstate(frame_index):
                zoom(zoom):
                diffusealpha(1.0)
            ));
        }
    }
    let judgment_actors = capture_requests
        .judgment
        .then(|| share_hud_range(&mut hud_actors, judgment_capture_start))
        .flatten();

    let (tilt, skew) = (perspective.tilt, perspective.skew);
    if !actors.is_empty() {
        let center_y = 0.5 * (receptor_y_normal + receptor_y_reverse);
        if let Some(view_proj) = notefield_view_proj(
            screen_width(),
            screen_height(),
            playfield_center_x,
            center_y,
            tilt,
            skew,
            reverse_scroll,
        ) {
            actors.reserve(2);
            actors.insert(0, Actor::CameraPush { view_proj });
            actors.push(Actor::CameraPop);
        }
    }

    let field_actors = if capture_requests.note_field && !actors.is_empty() {
        let children = Arc::<[Actor]>::from(actors.drain(..).collect::<Vec<_>>());
        actors.push(Actor::SharedFrame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: Arc::clone(&children),
            background: None,
            z: 0,
            tint: [1.0; 4],
            blend: None,
        });
        vec![children]
    } else {
        Vec::new()
    };
    BuiltNotefield {
        layout_center_x,
        field_actors,
        judgment_actors,
        combo_actors,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MiniIndicatorProgress, TornadoBounds, Z_HOLD_BODY, Z_HOLD_GLOW, Z_RECEPTOR,
        actual_grade_points_with_provisional, add_provisional_early_bad_counts_to_ex_score,
        append_mini_part, append_perspective_parts, append_turn_parts, arrow_effect_zoom,
        bottom_cap_uv_window, calc_note_rotation_z, clipped_hold_body_bounds, combo_actor_zoom,
        confusion_rotation_deg, hallway_judgment_zoom, hold_body_segment_budget, hold_draw_span,
        hold_head_render_flags, hold_segment_pose, hold_strip_actor, hold_strip_row_3d,
        hold_tail_cap_bounds, hud_layout_ys, hud_y, judgment_actor_zoom,
        judgment_tilt_rotation_deg, let_go_head_beat, maybe_mirror_uv_horiz_for_reverse_flipped,
        move_x_extra, move_y_extra, note_alpha, note_glow, note_slot_base_size,
        note_world_z_for_bumpy, note_x_extra, offset_center, predictive_itg_percents,
        pulse_inner_zoom, pulse_zoom_for_y, push_transform_parts, receptor_row_center,
        scroll_receptor_y, song_lua_hides_note_window, tap_judgment_rows, tap_part_for_note_type,
        tiny_zoom_for_col, tipsy_y_extra, top_cap_rotation_deg, turn_option_bits, turn_option_name,
        zmod_subtractive_counter_state,
    };
    use crate::engine::gfx::BlendMode;
    use crate::engine::present::actors::Actor;
    use crate::game::gameplay::{
        AccelEffects, ActiveHold, AppearanceEffects, NoteCountStat, VisualEffects,
    };
    use crate::game::judgment::{
        ExScoreData, JUDGE_GRADE_COUNT, JudgeGrade, Judgment, TimingWindow, ex_score_percent,
        predictive_ex_score_percents,
    };
    use crate::game::note::{MineResult, Note, NoteType};
    use crate::game::parsing::noteskin::{
        NUM_QUANTIZATIONS, NoteAnimPart, Quantization, Style, load_itg_skin,
    };
    use crate::game::parsing::song_lua::SongLuaNoteHideWindow;
    use crate::game::profile;
    use crate::game::timing::{TimeSignatureSegment, beat_to_note_row};
    use std::sync::Arc;

    fn fantastic_judgment(window: TimingWindow, time_error_ms: f32) -> Judgment {
        Judgment {
            time_error_ms,
            time_error_music_ns: crate::game::judgment::judgment_time_error_music_ns_from_ms(
                time_error_ms,
                1.0,
            ),
            grade: JudgeGrade::Fantastic,
            window: Some(window),
            miss_because_held: false,
        }
    }

    fn test_note_at_beat(beat: f32) -> Note {
        Note {
            beat,
            quantization_idx: 0,
            column: 0,
            note_type: NoteType::Tap,
            row_index: beat_to_note_row(beat).max(0) as usize,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    #[test]
    fn edit_beat_bar_labels_default_measure_indices() {
        assert_eq!(
            super::edit_beat_bar_info_for_row(beat_to_note_row(0.0), &[])
                .and_then(|info| info.measure_index),
            Some(0)
        );
        assert_eq!(
            super::edit_beat_bar_info_for_row(beat_to_note_row(1.0), &[])
                .and_then(|info| info.measure_index),
            None
        );
        assert_eq!(
            super::edit_beat_bar_info_for_row(beat_to_note_row(4.0), &[])
                .and_then(|info| info.measure_index),
            Some(1)
        );
    }

    #[test]
    fn edit_beat_bar_labels_follow_time_signature_segments() {
        let segments = [
            TimeSignatureSegment {
                beat: 0.0,
                numerator: 3,
                denominator: 4,
            },
            TimeSignatureSegment {
                beat: 6.0,
                numerator: 4,
                denominator: 4,
            },
        ];

        assert_eq!(
            super::edit_beat_bar_info_for_row(beat_to_note_row(0.0), &segments)
                .and_then(|info| info.measure_index),
            Some(0)
        );
        assert_eq!(
            super::edit_beat_bar_info_for_row(beat_to_note_row(3.0), &segments)
                .and_then(|info| info.measure_index),
            Some(1)
        );
        assert_eq!(
            super::edit_beat_bar_info_for_row(beat_to_note_row(6.0), &segments)
                .and_then(|info| info.measure_index),
            Some(2)
        );
    }

    #[test]
    fn mine_hides_after_any_final_resolution() {
        assert!(!super::mine_hides_after_resolution(None));
        assert!(super::mine_hides_after_resolution(Some(MineResult::Hit)));
        assert!(super::mine_hides_after_resolution(Some(
            MineResult::Avoided
        )));
    }

    #[test]
    fn visible_beat_probe_keeps_brake_warped_notes() {
        let accel = AccelEffects {
            brake: 1.0,
            ..Default::default()
        };
        let last = super::find_last_displayed_beat(0.0, 120.0, 1.0, false, |beat| {
            let y = super::apply_accel_y(
                beat * 64.0,
                0.0,
                0.0,
                super::field_effect_height(0.0),
                accel,
            );
            (y, true)
        })
        .expect("finite beat range");
        assert!(
            last > 3.5,
            "last beat {last} should include the warped note"
        );

        let notes = vec![test_note_at_beat(0.5), test_note_at_beat(3.5)];
        let note_indices = vec![0usize, 1usize];
        let mut visited = Vec::new();
        super::for_each_visible_note_index(
            &note_indices,
            &notes,
            Some((beat_to_note_row(0.0), beat_to_note_row(last))),
            |note_index| visited.push(note_index),
        );
        assert_eq!(visited, vec![0, 1]);
    }

    #[test]
    fn visible_note_iterator_skips_rows_outside_window() {
        let notes = vec![
            test_note_at_beat(0.0),
            test_note_at_beat(4.0),
            test_note_at_beat(8.0),
        ];
        let note_indices = vec![0usize, 1, 2];
        let mut visited = Vec::new();

        super::for_each_visible_note_index(
            &note_indices,
            &notes,
            Some((beat_to_note_row(3.0), beat_to_note_row(5.0))),
            |note_index| visited.push(note_index),
        );

        assert_eq!(visited, vec![1]);
    }

    #[test]
    fn first_visible_beat_uses_note_count_cutoff() {
        let stats = (0..80)
            .map(|i| NoteCountStat {
                beat: i as f32 * 0.25,
                notes_lower: i,
                notes_upper: i + 1,
            })
            .collect::<Vec<_>>();

        let first = super::find_first_displayed_beat(20.0, 120.0, &stats, |_| 0.0)
            .expect("finite beat range");

        assert!((3.9..=4.1).contains(&first), "first beat was {first}");
    }

    #[test]
    fn hold_head_render_flags_keep_early_hit_inactive_before_receptor() {
        let active = ActiveHold {
            note_index: 42,
            start_time_ns: 100_000_000_000,
            end_time_ns: 12_000_000_000,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: true,
            life: 1.0,
            last_update_time_ns: 100_000_000_000,
        };
        let (engaged, use_active) = hold_head_render_flags(Some(&active), 99.99, 100.0);
        assert!(!engaged);
        assert!(!use_active);
    }

    #[test]
    fn hold_head_render_flags_switch_to_active_at_receptor() {
        let mut active = ActiveHold {
            note_index: 42,
            start_time_ns: 100_000_000_000,
            end_time_ns: 12_000_000_000,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: true,
            life: 1.0,
            last_update_time_ns: 100_000_000_000,
        };
        let (engaged, use_active) = hold_head_render_flags(Some(&active), 100.0, 100.0);
        assert!(engaged);
        assert!(use_active);

        active.is_pressed = false;
        let (engaged_released, use_active_released) =
            hold_head_render_flags(Some(&active), 100.0, 100.0);
        assert!(engaged_released);
        assert!(!use_active_released);
    }

    #[test]
    fn roll_head_render_flags_stay_active_between_taps() {
        let active = ActiveHold {
            note_index: 42,
            start_time_ns: 100_000_000_000,
            end_time_ns: 12_000_000_000,
            note_type: NoteType::Roll,
            let_go: false,
            is_pressed: false,
            life: 1.0,
            last_update_time_ns: 100_000_000_000,
        };
        let (engaged, use_active) = hold_head_render_flags(Some(&active), 100.0, 100.0);
        assert!(engaged);
        assert!(use_active);
    }

    #[test]
    fn hold_head_render_flags_require_engaged_life_state() {
        let exhausted = ActiveHold {
            note_index: 7,
            start_time_ns: 100_000_000_000,
            end_time_ns: 8_000_000_000,
            note_type: NoteType::Roll,
            let_go: false,
            is_pressed: true,
            life: 0.0,
            last_update_time_ns: 100_000_000_000,
        };
        let let_go = ActiveHold {
            note_index: 7,
            start_time_ns: 100_000_000_000,
            end_time_ns: 8_000_000_000,
            note_type: NoteType::Roll,
            let_go: true,
            is_pressed: true,
            life: 1.0,
            last_update_time_ns: 100_000_000_000,
        };
        assert_eq!(
            hold_head_render_flags(Some(&exhausted), 200.0, 100.0),
            (false, false)
        );
        assert_eq!(
            hold_head_render_flags(Some(&let_go), 200.0, 100.0),
            (false, false)
        );
    }

    #[test]
    fn let_go_head_beat_stays_at_receptor_until_visible_clock_catches_up() {
        let beat = let_go_head_beat(100.0, 108.0, 102.0, 101.25);
        assert!((beat - 101.25).abs() <= 1e-6);
    }

    #[test]
    fn let_go_head_beat_uses_last_held_once_visible_clock_has_caught_up() {
        let beat = let_go_head_beat(100.0, 108.0, 102.0, 103.0);
        assert!((beat - 102.0).abs() <= 1e-6);
    }

    #[test]
    fn receptor_glow_draws_under_hold_body() {
        assert!(Z_RECEPTOR < Z_HOLD_BODY);
        assert!(Z_HOLD_GLOW < Z_HOLD_BODY);
    }

    #[test]
    fn song_lua_zoom_hide_window_covers_receptor_beat() {
        let windows = [SongLuaNoteHideWindow {
            player: 0,
            column: 2,
            start_beat: 40.0,
            end_beat: 44.0,
        }];

        assert!(song_lua_hides_note_window(&windows, 2, 40.0));
        assert!(song_lua_hides_note_window(&windows, 2, 44.0));
        assert!(!song_lua_hides_note_window(&windows, 1, 42.0));
        assert!(!song_lua_hides_note_window(&windows, 2, 44.01));
    }

    #[test]
    fn reverse_column_cue_bounds_match_simply_love() {
        let lane_width = 64.0;
        let cue_height = super::column_cue_height();
        let top = super::column_cue_reverse_top_y(lane_width, cue_height, 0.0);
        let bottom = top + cue_height;

        assert!((cue_height - 400.0).abs() <= 1e-6);
        assert!((top - 17.0).abs() <= 1e-6);
        assert!((bottom - 417.0).abs() <= 1e-6);
    }

    #[test]
    fn hold_tail_cap_bounds_join_at_body_bottom_for_normal_scroll() {
        let body_tail_y = 100.0;
        let cap_height = 24.0;
        let (top, bottom) = hold_tail_cap_bounds(body_tail_y, cap_height, Some(20.0), Some(96.0))
            .expect("cap should connect when rendered body reaches tail side");
        assert!((top - 96.0).abs() <= 1e-6);
        assert!((bottom - 120.0).abs() <= 1e-6);
    }

    #[test]
    fn hold_tail_cap_bounds_falls_back_when_body_is_below_tail_anchor() {
        let body_tail_y = 100.0;
        let cap_height = 24.0;
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, Some(104.0), Some(160.0)),
            Some((100.0, 124.0))
        );
    }

    #[test]
    fn hold_tail_cap_bounds_skip_when_body_does_not_reach_tail() {
        let body_tail_y = 100.0;
        let cap_height = 24.0;
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, Some(20.0), Some(70.0)),
            Some((100.0, 124.0))
        );
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, Some(140.0), Some(200.0)),
            Some((100.0, 124.0))
        );
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, None, Some(95.0)),
            Some((100.0, 124.0))
        );
    }

    #[test]
    fn collapsed_hold_body_uses_tail_cap_fallback_bounds() {
        let body_top = 120.0;
        let body_bottom = 121.0;
        let natural_top = 100.0;
        let natural_bottom = 110.0;
        assert_eq!(
            clipped_hold_body_bounds(body_top, body_bottom, natural_top, natural_bottom),
            None
        );
        assert_eq!(
            hold_tail_cap_bounds(natural_bottom, 24.0, None, None),
            Some((110.0, 134.0))
        );
    }

    #[test]
    fn collapsed_hold_draw_span_still_draws_caps() {
        assert_eq!(hold_draw_span(120.0, 120.0), Some((120.0, 120.0)));
    }

    #[test]
    fn tiny_hold_body_repeat_uses_mesh_budget() {
        let (budget, allow_legacy) = hold_body_segment_budget(900.0, 0.25);
        assert!(budget > 2048);
        assert!(!allow_legacy);
    }

    #[test]
    fn normal_hold_body_repeat_keeps_legacy_budget() {
        let (budget, allow_legacy) = hold_body_segment_budget(900.0, 64.0);
        assert_eq!(budget, 2048);
        assert!(allow_legacy);
    }

    #[test]
    fn reverse_flipped_cap_uv_only_mirrors_when_both_flags_are_enabled() {
        let uv = [0.125, 0.25, 0.75, 0.875];
        assert_eq!(
            maybe_mirror_uv_horiz_for_reverse_flipped(uv, true, true),
            [0.75, 0.25, 0.125, 0.875]
        );
        assert_eq!(
            maybe_mirror_uv_horiz_for_reverse_flipped(uv, true, false),
            uv
        );
        assert_eq!(
            maybe_mirror_uv_horiz_for_reverse_flipped(uv, false, true),
            uv
        );
    }

    #[test]
    fn reverse_flipped_top_cap_rotation_matches_itg_parity_path() {
        assert!((top_cap_rotation_deg(true, true) - 180.0).abs() <= f32::EPSILON);
        assert!((top_cap_rotation_deg(true, false) - 0.0).abs() <= f32::EPSILON);
        assert!((top_cap_rotation_deg(false, true) - 0.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn lift_notes_use_lift_animation_part() {
        assert!(matches!(
            tap_part_for_note_type(NoteType::Lift),
            NoteAnimPart::Lift
        ));
    }

    #[test]
    fn bottom_cap_uv_window_matches_itg_add_to_tex_coord_progression() {
        let (v0, v1) = bottom_cap_uv_window(0.0, 1.0, 12.0, 24.0, false)
            .expect("non-zero cap span and draw height should produce UVs");
        assert!((v0 - 0.5).abs() <= 1e-6);
        assert!((v1 - 1.0).abs() <= 1e-6);

        let (full_v0, full_v1) = bottom_cap_uv_window(0.0, 1.0, 24.0, 24.0, false)
            .expect("full-height cap should preserve full UV range");
        assert!((full_v0 - 0.0).abs() <= 1e-6);
        assert!((full_v1 - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn bottom_cap_uv_window_honors_top_anchor_when_reverse() {
        let (v0, v1) = bottom_cap_uv_window(0.2, 0.8, 12.0, 24.0, true)
            .expect("top-anchored reverse path should produce UVs");
        assert!((v0 - 0.2).abs() <= 1e-6);
        assert!((v1 - 0.5).abs() <= 1e-6);
    }

    #[test]
    fn bottom_cap_uv_window_rejects_degenerate_inputs() {
        assert_eq!(bottom_cap_uv_window(0.0, 1.0, 0.0, 24.0, false), None);
        assert_eq!(bottom_cap_uv_window(0.0, 1.0, 24.0, 0.0, false), None);
    }

    #[test]
    fn subtractive_counter_uses_whites_for_ex_paths() {
        let itg = MiniIndicatorProgress {
            w2: 4,
            white_count: 7,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_counter_state(&itg, profile::MiniIndicatorScoreType::Itg),
            (4, false)
        );

        let ex = MiniIndicatorProgress {
            w2: 0,
            white_count: 7,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_counter_state(&ex, profile::MiniIndicatorScoreType::Ex),
            (7, false)
        );

        let hard_ex = MiniIndicatorProgress {
            w2: 1,
            white_count: 7,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_counter_state(&hard_ex, profile::MiniIndicatorScoreType::HardEx),
            (7, true)
        );
    }

    #[test]
    fn blink_alpha_matches_itg_boolean_behavior() {
        let partial = note_alpha(
            100.0,
            0.0,
            0.0,
            AppearanceEffects {
                blink: 0.3,
                ..AppearanceEffects::default()
            },
        );
        let full = note_alpha(
            100.0,
            0.0,
            0.0,
            AppearanceEffects {
                blink: 1.0,
                ..AppearanceEffects::default()
            },
        );
        assert!((partial - full).abs() <= 1e-6);
    }

    #[test]
    fn stealth_glow_matches_itg_visibility_curve() {
        let glow = note_glow(
            100.0,
            0.0,
            0.0,
            AppearanceEffects {
                stealth: 0.25,
                ..AppearanceEffects::default()
            },
        );
        assert!((glow - 0.65).abs() <= 1e-6);
    }

    #[test]
    fn sudden_offset_shifts_fade_band_like_itg() {
        let base = note_alpha(
            180.0,
            0.0,
            0.0,
            AppearanceEffects {
                sudden: 1.0,
                ..AppearanceEffects::default()
            },
        );
        let shifted = note_alpha(
            180.0,
            0.0,
            0.0,
            AppearanceEffects {
                sudden: 1.0,
                sudden_offset: 1.0,
                ..AppearanceEffects::default()
            },
        );
        assert!(shifted > base);
    }

    #[test]
    fn flip_note_x_extra_moves_to_mirrored_column() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let delta = note_x_extra(
            0,
            64.0,
            0.0,
            0.0,
            VisualEffects {
                flip: 1.0,
                ..VisualEffects::default()
            },
            &col_offsets,
            &invert,
            &tornado,
        );
        assert!((delta - 192.0).abs() <= 1e-6);
    }

    #[test]
    fn negative_position_mods_stay_active_like_itg() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let visual = VisualEffects {
            drunk: -1.0,
            tipsy: -1.0,
            flip: -0.5,
            ..VisualEffects::default()
        };
        let delta = note_x_extra(0, 0.0, 0.0, 0.0, visual, &col_offsets, &invert, &tornado);

        assert!((delta + 128.0).abs() <= 1e-6);
        assert!((tipsy_y_extra(0, 0.0, visual) + 25.6).abs() <= 1e-6);
    }

    #[test]
    fn bumpy_world_z_matches_itg_default_wave() {
        let z = note_world_z_for_bumpy(8.0 * std::f32::consts::PI, 1.0, 0.0, 0.0);
        assert!((z - 40.0).abs() <= 1e-4);
    }

    #[test]
    fn bumpy_period_changes_wave_length_like_itg() {
        let z = note_world_z_for_bumpy(-2.0 * std::f32::consts::PI, 1.0, 0.0, -1.25);
        assert!((z - 40.0).abs() <= 1e-4);
    }

    #[test]
    fn pulse_outer_zoom_matches_itg_formula() {
        let visual = VisualEffects {
            pulse_outer: 1.0,
            ..VisualEffects::default()
        };
        assert!((pulse_zoom_for_y(0.0, &visual) - 1.0).abs() <= 1e-6);
        assert!(
            (pulse_zoom_for_y(0.4 * 64.0 * std::f32::consts::FRAC_PI_2, &visual) - 1.5).abs()
                <= 1e-6
        );
    }

    #[test]
    fn pulse_inner_zero_clamps_like_itg() {
        let visual = VisualEffects {
            pulse_inner: -2.0,
            ..VisualEffects::default()
        };
        assert!((pulse_inner_zoom(&visual) - 0.01).abs() <= 1e-6);
    }

    #[test]
    fn hold_strip_row_3d_preserves_row_z() {
        let row = hold_strip_row_3d(
            [64.0, 128.0, 12.5],
            [0.0, 16.0],
            8.0,
            0.0,
            1.0,
            0.5,
            [1.0; 4],
        );
        assert!((row[0].pos[2] - 12.5).abs() <= 1e-6);
        assert!((row[1].pos[2] - 12.5).abs() <= 1e-6);
    }

    #[test]
    fn hold_strip_actor_carries_depth_test_flag() {
        let actor = hold_strip_actor(
            Arc::from("hold.png"),
            Arc::from([]),
            BlendMode::Alpha,
            true,
            Z_HOLD_BODY as i16,
        );
        assert!(matches!(
            actor,
            Actor::TexturedMesh {
                depth_test: true,
                ..
            }
        ));
    }

    #[test]
    fn tiny_column_zoom_matches_itg_power_formula() {
        let mut visual = VisualEffects::default();
        visual.tiny = -0.5;
        visual.tiny_cols[1] = 2.5;
        assert!((tiny_zoom_for_col(&visual, 1) - 0.5_f32.powf(2.0)).abs() <= 1e-6);
        assert!((tiny_zoom_for_col(&visual, 0) - 0.5_f32.powf(-0.5)).abs() <= 1e-6);
    }

    #[test]
    fn receptor_arrow_effect_zoom_matches_note_zoom_at_targets() {
        let visual = VisualEffects {
            tiny: 1.0,
            pulse_outer: 1.0,
            ..VisualEffects::default()
        };
        assert!((arrow_effect_zoom(&visual, 0, 0.0) - 0.5).abs() <= 1e-6);
    }

    #[test]
    fn move_and_confusion_column_mods_match_itg_scaling() {
        let mut visual = VisualEffects::default();
        visual.move_x_cols[1] = 0.5;
        visual.move_y_cols[1] = -0.25;
        visual.confusion_offset_cols[1] = std::f32::consts::FRAC_PI_2;

        assert_eq!(move_x_extra(visual, 1), 32.0);
        assert_eq!(move_y_extra(visual, 1), -16.0);
        assert!((confusion_rotation_deg(0.0, visual, 1) + 90.0).abs() <= 1e-6);
    }

    #[test]
    fn hold_segment_pose_keeps_vertical_segments_unrotated() {
        let (center, length, rotation) = hold_segment_pose([32.0, 100.0], [32.0, 180.0]);
        assert_eq!(center, [32.0, 140.0]);
        assert!((length - 80.0).abs() <= 1e-6);
        assert!(rotation.abs() <= 1e-6);
    }

    #[test]
    fn hold_segment_pose_uses_diagonal_length_and_rotation() {
        let (center, length, rotation) = hold_segment_pose([0.0, 0.0], [30.0, 40.0]);
        assert_eq!(center, [15.0, 20.0]);
        assert!((length - 50.0).abs() <= 1e-6);
        assert!((rotation - 36.869_896).abs() <= 1e-5);
    }

    #[test]
    fn receptor_center_uses_zero_travel_x_effects() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let center = receptor_row_center(
            320.0,
            1,
            240.0,
            1.0,
            0.0,
            VisualEffects {
                drunk: 1.0,
                ..VisualEffects::default()
            },
            &col_offsets,
            &invert,
            &tornado,
        );
        let expected_x = 320.0
            + col_offsets[1]
            + note_x_extra(
                1,
                0.0,
                1.0,
                0.0,
                VisualEffects {
                    drunk: 1.0,
                    ..VisualEffects::default()
                },
                &col_offsets,
                &invert,
                &tornado,
            );
        assert!((center[0] - expected_x).abs() <= 1e-6);
    }

    #[test]
    fn receptor_center_uses_tipsy_y_offset() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let visual = VisualEffects {
            tipsy: 1.0,
            ..VisualEffects::default()
        };
        let center = receptor_row_center(
            320.0,
            2,
            240.0,
            1.25,
            0.0,
            visual,
            &col_offsets,
            &invert,
            &tornado,
        );
        assert!((center[1] - (240.0 + tipsy_y_extra(2, 1.25, visual))).abs() <= 1e-6);
    }

    #[test]
    fn confusion_rotation_converts_itg_formula_to_actor_space() {
        let visual = VisualEffects {
            confusion: 1.5,
            ..VisualEffects::default()
        };
        let rotation = calc_note_rotation_z(visual, 12.0, 3.5, true, 0);
        let itg_expected = (3.5 * visual.confusion).rem_euclid(std::f32::consts::TAU)
            * (-180.0 / std::f32::consts::PI);
        assert!((rotation + itg_expected).abs() <= 1e-6);
    }

    #[test]
    fn confusion_offset_converts_static_rotation_to_actor_space() {
        let visual = VisualEffects {
            confusion_offset: std::f32::consts::FRAC_PI_2,
            ..VisualEffects::default()
        };
        let rotation = calc_note_rotation_z(visual, 12.0, 3.5, true, 0);
        assert!((rotation + 90.0).abs() <= 1e-6);
    }

    #[test]
    fn dizzy_rotation_converts_itg_formula_to_actor_space() {
        let visual = VisualEffects {
            dizzy: 2.0,
            ..VisualEffects::default()
        };
        let rotation = calc_note_rotation_z(visual, 6.75, 3.5, false, 0);
        let itg_expected = ((6.75 - 3.5) * visual.dizzy).rem_euclid(std::f32::consts::TAU)
            * (180.0 / std::f32::consts::PI);
        assert!((rotation + itg_expected).abs() <= 1e-6);
    }

    #[test]
    fn display_mods_mini_keeps_full_percent() {
        let mut parts = Vec::new();
        append_mini_part(&mut parts, 100);
        assert_eq!(parts, vec!["100% Mini".to_string()]);
    }

    #[test]
    fn display_mods_use_simply_love_turn_names() {
        assert_eq!(
            turn_option_name(profile::TurnOption::LRMirror),
            Some("LR-Mirror")
        );
        assert_eq!(
            turn_option_name(profile::TurnOption::UDMirror),
            Some("UD-Mirror")
        );
    }

    #[test]
    fn display_mods_append_all_active_turns_in_itg_order() {
        let mut parts = Vec::new();
        append_turn_parts(
            &mut parts,
            turn_option_bits(profile::TurnOption::Mirror)
                | turn_option_bits(profile::TurnOption::Random),
        );
        assert_eq!(parts, vec!["Mirror".to_string(), "Random".to_string()]);
    }

    #[test]
    fn display_mods_transform_order_matches_itg() {
        let mut parts = Vec::new();
        push_transform_parts(
            &mut parts,
            (1 << 0) | (1 << 1) | (1 << 7),
            (1 << 0) | (1 << 1),
            1 << 3,
        );
        assert_eq!(
            parts,
            vec![
                "NoRolls".to_string(),
                "NoMines".to_string(),
                "Little".to_string(),
                "Wide".to_string(),
                "Big".to_string(),
                "Mines".to_string(),
            ]
        );
    }

    #[test]
    fn display_mods_perspective_names_match_itg_rules() {
        let mut parts = Vec::new();
        append_perspective_parts(&mut parts, 0, 0);
        assert_eq!(parts, vec!["Overhead".to_string()]);

        let mut parts = Vec::new();
        append_perspective_parts(&mut parts, -100, 100);
        assert_eq!(parts, vec!["Incoming".to_string()]);
    }

    #[test]
    fn hud_y_only_uses_reverse_branch_for_full_reverse() {
        let normal_y = 100.0;
        let reverse_y = 200.0;
        let centered_y = 300.0;
        assert!((hud_y(normal_y, reverse_y, centered_y, false, 0.3) - 160.0).abs() <= 1e-6);
        assert!((hud_y(normal_y, reverse_y, centered_y, true, 0.3) - 230.0).abs() <= 1e-6);
    }

    #[test]
    fn centered_scroll_overshoots_like_itg() {
        assert!((hud_y(100.0, 500.0, 300.0, false, 2.0) - 500.0).abs() <= 1e-6);
        assert!((scroll_receptor_y(0.0, 2.0, 100.0, 500.0, 300.0) - 500.0).abs() <= 1e-6);
    }

    #[test]
    fn hud_layout_offsets_apply_independently() {
        let profile = profile::Profile {
            error_bar_active_mask: profile::ERROR_BAR_BIT_MONOCHROME,
            ..profile::Profile::default()
        };
        let base = hud_layout_ys(&profile, 100.0, 160.0, false, 0.0, 0.0, 0.0);
        let moved_judgment = hud_layout_ys(&profile, 100.0, 160.0, false, 25.0, 0.0, 0.0);
        assert_eq!(moved_judgment.judgment_y, 125.0);
        assert_eq!(moved_judgment.zmod_layout.combo_y, base.zmod_layout.combo_y);
        assert_eq!(moved_judgment.error_bar_y, base.error_bar_y);

        let moved_combo = hud_layout_ys(&profile, 100.0, 160.0, false, 0.0, -30.0, 0.0);
        assert_eq!(moved_combo.judgment_y, base.judgment_y);
        assert_eq!(
            moved_combo.zmod_layout.combo_y,
            base.zmod_layout.combo_y - 30.0
        );
        assert_eq!(moved_combo.error_bar_y, base.error_bar_y);

        let moved_error_bar = hud_layout_ys(&profile, 100.0, 160.0, false, 0.0, 0.0, 18.0);
        assert_eq!(moved_error_bar.judgment_y, base.judgment_y);
        assert_eq!(
            moved_error_bar.zmod_layout.combo_y,
            base.zmod_layout.combo_y
        );
        assert_eq!(moved_error_bar.error_bar_y, base.error_bar_y + 18.0);
    }

    #[test]
    fn hallway_judgment_zoom_only_boosts_hallway_tilt() {
        assert!((hallway_judgment_zoom(0.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((hallway_judgment_zoom(-1.0, 1.0) - 1.0).abs() <= 1e-6);
        assert!((hallway_judgment_zoom(1.0, 0.0) - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn judgment_actor_zoom_matches_itgmania_player_mini_formula_without_judgment_back() {
        // Without the Arrow Cloud JudgmentBack override, the front judgment
        // inherits the Player ActorFrame's mini scale, identical to combo:
        // min(pow(0.5, mini + tiny), 1.0).
        assert!((judgment_actor_zoom(0.0, false) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(1.0, false) - 0.5).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.5, false) - 0.5_f32.sqrt()).abs() <= 1e-6);
        // Negative mini is clamped to 1.0 by the min(_, 1.0) cap so the
        // judgment never grows past its base size.
        assert!((judgment_actor_zoom(-1.0, false) - 1.0).abs() <= 1e-6);
        // Parity with combo_actor_zoom is the whole point of this branch.
        for &mini in &[-1.0_f32, 0.0, 0.25, 0.5, 1.0, 1.5] {
            assert!((judgment_actor_zoom(mini, false) - combo_actor_zoom(mini)).abs() <= 1e-6);
        }
    }

    #[test]
    fn judgment_actor_zoom_matches_arrow_cloud_judgment_back_formula() {
        assert!((judgment_actor_zoom(0.35, true) - 0.825).abs() <= 1e-6);
        assert!((judgment_actor_zoom(1.5, true) - 0.35).abs() <= 1e-6);
        assert!((judgment_actor_zoom(-1.0, true) - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn judgment_tilt_thresholds_deadzone_and_cap() {
        let profile = profile::Profile {
            judgment_tilt: true,
            tilt_min_threshold_ms: 5,
            tilt_max_threshold_ms: 20,
            ..profile::Profile::default()
        };
        assert_eq!(
            judgment_tilt_rotation_deg(&profile, &fantastic_judgment(TimingWindow::W0, 5.0)),
            0.0
        );
        assert!(
            (judgment_tilt_rotation_deg(&profile, &fantastic_judgment(TimingWindow::W0, 10.0))
                + 1.5)
                .abs()
                <= 1e-6
        );
        assert!(
            (judgment_tilt_rotation_deg(&profile, &fantastic_judgment(TimingWindow::W0, 40.0))
                + 4.5)
                .abs()
                <= 1e-6
        );
    }

    #[test]
    fn judgment_tilt_keeps_early_late_direction() {
        let profile = profile::Profile {
            judgment_tilt: true,
            tilt_min_threshold_ms: 0,
            tilt_max_threshold_ms: 50,
            ..profile::Profile::default()
        };
        assert!(
            judgment_tilt_rotation_deg(&profile, &fantastic_judgment(TimingWindow::W0, -10.0))
                > 0.0
        );
        assert!(
            judgment_tilt_rotation_deg(&profile, &fantastic_judgment(TimingWindow::W0, 10.0)) < 0.0
        );
    }

    #[test]
    fn combo_actor_zoom_matches_itgmania_player_mini_formula() {
        // ITGmania Player::Update: min(pow(0.5, mini + tiny), 1.0).
        assert!((combo_actor_zoom(0.0) - 1.0).abs() <= 1e-6);
        assert!((combo_actor_zoom(1.0) - 0.5).abs() <= 1e-6);
        assert!((combo_actor_zoom(0.5) - 0.5_f32.sqrt()).abs() <= 1e-6);
        // Big (negative mini) is clamped to 1.0 by the min(_, 1.0) cap so
        // the combo never grows past its base size.
        assert!((combo_actor_zoom(-1.0) - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn hallway_judgment_zoom_matches_itgmania_hallway_quirk() {
        let zoom = hallway_judgment_zoom(-1.0, 0.0);
        assert!((zoom - (1.0 / 0.9)).abs() <= 1e-6);
    }

    #[test]
    fn tap_judgment_rows_overlay_white_for_split_15_10_hits() {
        let profile = profile::Profile {
            show_fa_plus_window: true,
            split_15_10ms: true,
            ..profile::Profile::default()
        };
        let judgment = fantastic_judgment(TimingWindow::W0, 12.0);
        assert_eq!(tap_judgment_rows(&profile, &judgment, 7), (0, Some(1)));
    }

    #[test]
    fn tap_judgment_rows_keep_plain_blue_when_split_is_off() {
        let profile = profile::Profile {
            show_fa_plus_window: true,
            ..profile::Profile::default()
        };
        let judgment = fantastic_judgment(TimingWindow::W0, 12.0);
        assert_eq!(tap_judgment_rows(&profile, &judgment, 7), (0, None));
    }

    #[test]
    fn tap_judgment_rows_ignore_split_without_fa_plus_window() {
        let profile = profile::Profile {
            split_15_10ms: true,
            ..profile::Profile::default()
        };
        let judgment = fantastic_judgment(TimingWindow::W0, 12.0);
        assert_eq!(tap_judgment_rows(&profile, &judgment, 7), (0, None));
    }

    #[test]
    fn tap_judgment_rows_defer_to_custom_window_over_fixed_split() {
        let profile = profile::Profile {
            show_fa_plus_window: true,
            split_15_10ms: true,
            custom_fantastic_window: true,
            custom_fantastic_window_ms: 12,
            ..profile::Profile::default()
        };
        let judgment = fantastic_judgment(TimingWindow::W1, 14.0);
        assert_eq!(tap_judgment_rows(&profile, &judgment, 7), (1, None));
    }

    #[test]
    fn tap_judgment_rows_keep_six_row_assets_unsplit() {
        let profile = profile::Profile {
            show_fa_plus_window: true,
            split_15_10ms: true,
            ..profile::Profile::default()
        };
        let fantastic = fantastic_judgment(TimingWindow::W0, 12.0);
        let excellent = Judgment {
            grade: JudgeGrade::Excellent,
            time_error_ms: 18.0,
            time_error_music_ns: crate::game::judgment::judgment_time_error_music_ns_from_ms(
                18.0, 1.0,
            ),
            window: Some(TimingWindow::W1),
            miss_because_held: false,
        };
        assert_eq!(tap_judgment_rows(&profile, &fantastic, 6), (0, None));
        assert_eq!(tap_judgment_rows(&profile, &excellent, 6), (1, None));
    }

    #[test]
    fn provisional_early_wayoff_counts_toward_predictive_ex_loss() {
        let score = ExScoreData {
            total_steps: 100,
            ..ExScoreData::default()
        };
        let base = predictive_ex_score_percents(&score);
        let mut provisional = [0u32; JUDGE_GRADE_COUNT];
        provisional[crate::game::judgment::judge_grade_ix(JudgeGrade::WayOff)] = 1;
        let adjusted = add_provisional_early_bad_counts_to_ex_score(score, &provisional);

        assert_eq!(base, (100.0, 0.0, 0.0));
        assert_eq!(predictive_ex_score_percents(&adjusted), (99.0, 1.0, 0.0));
        assert_eq!(ex_score_percent(&adjusted), 0.0);
    }

    #[test]
    fn provisional_early_wayoff_counts_toward_predictive_itg_loss() {
        let mut provisional = [0u32; JUDGE_GRADE_COUNT];
        provisional[crate::game::judgment::judge_grade_ix(JudgeGrade::WayOff)] = 1;
        let actual = actual_grade_points_with_provisional(0, &provisional);
        let (kept, lost, pace) = predictive_itg_percents(5, 100, actual);

        assert_eq!(actual, 0);
        assert_eq!((kept, lost, pace), (95.0, 5.0, 0.0));
    }

    #[test]
    fn cyber_model_tap_scale_uses_model_height_not_logical_height() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg_skin(&style, "cyber").expect("dance/cyber should load from assets/noteskins");
        let slot = ns
            .note_layers
            .first()
            .and_then(|layers| layers.iter().find(|slot| slot.model.is_some()))
            .expect("cyber should expose model-backed tap-note layer for 4th notes");

        let logical_h = slot.logical_size()[1].max(1.0);
        let model_h = slot
            .model
            .as_ref()
            .map(|model| model.size()[1])
            .expect("cyber tap slot should be model-backed");
        assert!(
            model_h > f32::EPSILON,
            "cyber model-backed tap slot should have positive model height"
        );
        assert!(
            logical_h / model_h > 1.5,
            "regression guard: cyber logical height must stay larger than model height so this test catches logical-height scaling; logical={logical_h}, model={model_h}"
        );
        let scale_h = note_slot_base_size(slot, 1.0)[1];
        assert!(
            (scale_h - model_h).abs() <= 1e-4,
            "model-backed tap notes must scale by model height; got scale_h={scale_h}, model_h={model_h}"
        );
    }

    #[test]
    fn default_tap_circles_stay_inside_arrow_in_gameplay_layout() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        const EPSILON: f32 = 1e-3;

        for col in 0..style.num_cols {
            let note_idx = col * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
            let layers = ns
                .note_layers
                .get(note_idx)
                .expect("default should expose Q4th tap layers for each column");

            let mut arrow_bounds: Option<(f32, f32, f32, f32)> = None;
            let mut circle_bounds = Vec::new();

            for slot in layers.iter() {
                let draw = slot.model_draw_at(0.0, 0.0);
                if !draw.visible {
                    continue;
                }
                let base_size = note_slot_base_size(slot, 1.0);
                let size = [
                    base_size[0] * draw.zoom[0].max(0.0),
                    base_size[1] * draw.zoom[1].max(0.0),
                ];
                if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
                    continue;
                }
                let local_offset = [draw.pos[0], draw.pos[1]];
                let center = offset_center([0.0, 0.0], local_offset, slot.base_rot_sin_cos());
                let half_w = size[0] * 0.5;
                let half_h = size[1] * 0.5;
                let bounds = (
                    center[0] - half_w,
                    center[0] + half_w,
                    center[1] - half_h,
                    center[1] + half_h,
                );
                let key = slot.texture_key().to_ascii_lowercase();
                if key.contains("_arrow") {
                    arrow_bounds = Some(bounds);
                } else if key.contains("_circle") {
                    circle_bounds.push(bounds);
                }
            }

            let (ax0, ax1, ay0, ay1) =
                arrow_bounds.expect("default tap layers should include arrow layer");
            assert_eq!(
                circle_bounds.len(),
                4,
                "default tap layers should include four circle layers"
            );
            for (idx, (cx0, cx1, cy0, cy1)) in circle_bounds.into_iter().enumerate() {
                assert!(
                    cx0 >= ax0 - EPSILON
                        && cx1 <= ax1 + EPSILON
                        && cy0 >= ay0 - EPSILON
                        && cy1 <= ay1 + EPSILON,
                    "column {col} circle {idx} escaped arrow bounds: circle=({cx0},{cx1},{cy0},{cy1}), arrow=({ax0},{ax1},{ay0},{ay1})"
                );
            }
        }
    }
}
