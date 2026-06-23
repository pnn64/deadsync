use crate::act;
use crate::assets;
use crate::game::parsing::noteskin::{
    ModelDrawState, ModelMeshCache, NUM_QUANTIZATIONS, NoteAnimPart, Noteskin, SpriteSlot,
};
use crate::game::scores;
use crate::game::{
    GameplayCoreState as State, scroll_effects_from_option, tap_explosion_options_from_profile,
};
use crate::screens::components::shared::noteskin_model::noteskin_model_actor_from_draw_cached;
use crate::screens::gameplay::GameplayNoteskinAssets;
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_present::cache::{TextCache, cached_text};
use deadlib_present::color;
use deadlib_present::compose::TextLayoutCache;
use deadlib_present::font;
use deadlib_present::space::*;
use deadlib_render::{BlendMode, TexturedMeshVertex};
use deadsync_core::input::{MAX_COLS, MAX_PLAYERS};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::SongTimeNs;
use deadsync_core::timing::{beat_to_note_row, note_row_to_beat};
use deadsync_gameplay::{
    AccelEffects, AppearanceEffects, COMBO_HUNDRED_MILESTONE_DURATION,
    COMBO_THOUSAND_MILESTONE_DURATION, ComboMilestoneKind, FantasticWindowOptions,
    GameplayErrorBarTrim, HELD_MISS_TOTAL_DURATION, HOLD_JUDGMENT_TOTAL_DURATION,
    PerspectiveEffects, PlayerRuntime, RECEPTOR_Y_OFFSET_FROM_CENTER,
    RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE, ScrollEffects, VisualEffects, active_column_cue,
    blue_fantastic_window_ms, column_flash_duration, gameplay_error_bar_trim_max_window_ix,
    hold_explosion_active, hold_explosion_enabled_for_options, hold_head_render_flags,
    let_go_head_beat, perspective_effects_from_profile, scroll_receptor_y,
    song_lua_column_y_offset, song_lua_note_hidden, spacing_multiplier_for_percent,
};
use deadsync_notefield::{
    AccelYParams, COLUMN_CUE_Y_OFFSET, DISPLAY_TURN_BLENDER, DISPLAY_TURN_LEFT,
    DISPLAY_TURN_LR_MIRROR, DISPLAY_TURN_MIRROR, DISPLAY_TURN_RANDOM, DISPLAY_TURN_RIGHT,
    DISPLAY_TURN_SHUFFLE, DISPLAY_TURN_UD_MIRROR, GameplayModsAttackMode, GameplayModsTextParams,
    HudLayoutOffsets, HudLayoutParams, HudLayoutYs, JudgmentTiltParams,
    LayoutMiniIndicatorPosition, MiniIndicatorColorStyle, MiniIndicatorMode, MiniIndicatorProgress,
    MiniIndicatorScoreType, MiniIndicatorSize, MiniIndicatorSubtractiveDisplay, NoteAlphaParams,
    NoteXParams, TapJudgmentRowsParams, TornadoBounds, VisualEffectParams, ZmodComboColorParams,
    ZmodComboColorStyle, ZmodLayoutParams, ZmodMeasureCounterText, ZmodMiniIndicatorParams,
    ZmodMiniIndicatorText, appearance_needs_rows, appearance_note_actor_alpha,
    appearance_note_glow, apply_accel_y as crate_apply_accel_y,
    apply_accel_y_with_peak as crate_apply_accel_y_with_peak, average_error_bar_mini_scale,
    beat_factor, beat_scroll_travel, bottom_cap_uv_window, clamp_rounded_i16,
    clipped_hold_body_bounds, column_cue_alpha, column_cue_height, column_cue_reverse_top_y,
    column_flash_alpha, column_flash_color, column_flash_height, column_flash_layout,
    column_flash_reverse_top_y, combo_actor_zoom, compute_invert_distances, compute_tornado_bounds,
    crossover_cue_height, edit_bar_candidate_step_rows, edit_bar_scroll_speed,
    edit_beat_bar_info_for_row, edit_beat_scroll_travel,
    effective_mini_value as crate_effective_mini_value, error_bar_boundaries_s,
    error_bar_color_for_window, error_bar_flash_alpha, error_bar_text_scalable_zoom,
    error_bar_tick_alpha, field_effect_height as field_effect_height_for_screen,
    fill_lane_col_offsets, find_first_displayed_beat, find_last_displayed_beat,
    for_each_visible_hold_index, for_each_visible_note_index,
    gameplay_mods_text as crate_gameplay_mods_text, held_miss_zoom, hold_body_bottom_for_tail_cap,
    hold_body_segment_budget, hold_draw_span, hold_glow_color,
    hold_indicator_column_x as crate_hold_indicator_column_x, hold_overlaps_visible_window,
    hold_segment_pose, hold_tail_cap_bounds, hud_layout_ys as crate_hud_layout_ys, hud_y,
    itg_actor_glow_alpha, judgment_actor_zoom,
    judgment_tilt_rotation_deg as crate_judgment_tilt_rotation_deg, maybe_flip_uv_vert,
    maybe_mirror_uv_horiz_for_reverse_flipped, mine_hides_after_resolution, mod_percent_key,
    move_col_extra, note_world_z_for_bumpy, note_x_offset as crate_note_x_offset,
    notefield_view_proj, offset_center, player_metric_y, quantize_centi_i32, quantize_centi_u32,
    receptor_row_center as crate_receptor_row_center, scale_cap_to_arrow, scale_effect_size,
    scale_sprite_to_arrow, scaled_edit_bar_alpha, smoothstep01, song_time_ns_delta_seconds,
    song_time_ns_to_seconds, stream_segment_index_exclusive_end,
    tap_judgment_rows as crate_tap_judgment_rows, timing_window_from_num, tipsy_y_extra,
    top_cap_rotation_deg, translated_uv_rect, visual_arrow_effect_zoom,
    visual_confusion_rotation_deg,
    visual_effect_params_for_col as crate_visual_effect_params_for_col,
    visual_hold_body_needs_z_buffer, visual_note_rotation_z, visual_pulse_zoom_for_y,
    visual_tiny_zoom, visual_use_legacy_hold_sprites, zmod_broken_run_counter_text,
    zmod_broken_run_end, zmod_broken_run_segment,
    zmod_combo_quint_active as crate_zmod_combo_quint_active,
    zmod_measure_counter_text as crate_zmod_measure_counter_text, zmod_mini_indicator_output,
    zmod_mini_indicator_zoom as crate_zmod_mini_indicator_zoom, zmod_percent_from_points,
    zmod_resolved_combo_color as crate_zmod_resolved_combo_color,
    zmod_resolved_mini_indicator_mode, zmod_run_timer_index,
    zmod_static_combo_color as crate_zmod_static_combo_color, zmod_stream_prog_completion_for_beat,
};
use deadsync_profile as profile_data;
use deadsync_rules::judgment::{self, HOLD_SCORE_HELD, JudgeGrade, Judgment};
use deadsync_rules::note::{HoldResult, Note};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::stream::StreamSegment;
use glam::Mat4 as Matrix4;
use std::array::from_fn;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;
use std::time::Instant;
use twox_hash::XxHash64;

#[inline(always)]
fn player_blue_window_ms(state: &State, player_idx: usize) -> f32 {
    let base = state.default_fa_plus_window_s();
    let Some(profile) = state.profiles().get(player_idx) else {
        return base * 1000.0;
    };
    blue_fantastic_window_ms(FantasticWindowOptions {
        base_fa_plus_s: base,
        custom_fantastic_window_s: profile.custom_fantastic_window.then(|| {
            f32::from(profile_data::clamp_custom_fantastic_window_ms(
                profile.custom_fantastic_window_ms,
            )) / 1000.0
        }),
        fa_plus_10ms_blue_window: profile.fa_plus_10ms_blue_window,
    })
}

// --- CONSTANTS ---

// Simply Love ScreenGameplay in/default.lua keeps intro cover actors alive for 2.0s.
const TRANSITION_IN_DURATION: f32 = 2.0;

// Gameplay Layout & Feel
const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0; // Dance lane width for hold bodies and square fallback visuals
const HOLD_JUDGMENT_Y_OFFSET_FROM_CENTER: f32 = -90.0; // Mirrors Simply Love metrics for hold judgments
const HOLD_JUDGMENT_Y_REVERSE_OFFSET_FROM_CENTER: f32 = 90.0;
const TAP_JUDGMENT_OFFSET_FROM_CENTER: f32 = 30.0; // From _fallback JudgmentTransformCommand
const COMBO_OFFSET_FROM_CENTER: f32 = 30.0; // From _fallback ComboTransformCommand (non-centered)
const COLUMN_CUE_TEXT_NORMAL_Y: f32 = 80.0;
const COLUMN_CUE_TEXT_REVERSE_Y: f32 = 260.0;
const COLUMN_CUE_BASE_ALPHA: f32 = 0.12;
const LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT: f32 = 140.0; // Each frame in Love 1x2 (doubleres).png is 140px tall
const HOLD_JUDGMENT_FINAL_HEIGHT: f32 = 32.0; // Matches Simply Love's final on-screen size
const HOLD_JUDGMENT_INITIAL_HEIGHT: f32 = HOLD_JUDGMENT_FINAL_HEIGHT * 0.8; // Mirrors 0.4->0.5 zoom ramp in metrics
const HOLD_JUDGMENT_FINAL_ZOOM: f32 =
    HOLD_JUDGMENT_FINAL_HEIGHT / LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT;
const HOLD_JUDGMENT_INITIAL_ZOOM: f32 =
    HOLD_JUDGMENT_INITIAL_HEIGHT / LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT;
const HELD_MISS_Y_OFFSET_FROM_CENTER: f32 = -50.0;
const HELD_MISS_Y_REVERSE_OFFSET_FROM_CENTER: f32 = 110.0;
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
const ERROR_BAR_MONO_BG_ALPHA: f32 = 0.5;
const ERROR_BAR_LINE_ALPHA: f32 = 0.3;
const ERROR_BAR_LINES_FADE_START_S: f32 = 2.5;
const ERROR_BAR_LINES_FADE_DUR_S: f32 = 0.5;
const ERROR_BAR_LABEL_FADE_DUR_S: f32 = 0.5;
const ERROR_BAR_LABEL_HOLD_S: f32 = 2.0;
const ERROR_BAR_CENTER_TICK_WIDTH: f32 = 1.0;
const OFFSET_INDICATOR_DUR_S: f32 = 0.5;
const DISPLAY_MODS_ZOOM: f32 = 0.8;
const DISPLAY_MODS_WRAP_WIDTH_PX: f32 = 125.0;
const DISPLAY_MODS_LINE_STEP: f32 = 15.0;
const DISPLAY_MODS_WARNING_W: f32 = 90.0;
const DISPLAY_MODS_WARNING_H: f32 = 30.0;
const DISPLAY_MODS_WARNING_ZOOM: f32 = 1.5;

const ERROR_BAR_COLORFUL_TICK_RGBA: [f32; 4] = color::rgba_hex("#b20000");
const ERROR_BAR_LONG_AVG_TICK_RGBA: [f32; 4] = color::rgba_hex("#0000ff");
const ERROR_BAR_LONG_AVG_TICK_EXTRA_H: f32 = 65.0;
const ERROR_BAR_LONG_AVG_TICK_WIDTH: f32 = 1.0; // SL Average.lua: LongAvgTick zoomto(1, ...)
const ERROR_BAR_TEXT_EARLY_RGBA: [f32; 4] = color::rgba_hex("#066af4");
const ERROR_BAR_TEXT_LATE_RGBA: [f32; 4] = color::rgba_hex("#ff5a4e");
const ERROR_BAR_TEXT_10MS_FAST_RGBA: [f32; 4] = color::rgba_hex("#0051db");
const ERROR_BAR_TEXT_10MS_SLOW_RGBA: [f32; 4] = color::rgba_hex("#ff1605");
const ERROR_BAR_CENTER_TICK_RGBA: [f32; 4] = [1.0, 1.0, 1.0, 0.3];
const ERROR_BAR_TEXT_ZOOM: f32 = 0.25;
const TEXT_CACHE_LIMIT: usize = 8192;
const COMBO_PREWARM_CAP: u32 = 2048;
const MEASURE_PREWARM_CAP: i32 = 64;
const RUN_TIMER_PREWARM_CAP_S: i32 = 600;

// Visual Feedback
const SHOW_COMBO_AT: u32 = 4; // From Simply Love metrics

#[inline(always)]
fn judgment_tilt_rotation_deg(profile: &profile_data::Profile, judgment: &Judgment) -> f32 {
    crate_judgment_tilt_rotation_deg(JudgmentTiltParams {
        enabled: profile.judgment_tilt,
        grade: judgment.grade,
        time_error_ms: judgment.time_error_ms,
        min_threshold_ms: profile.tilt_min_threshold_ms as f32,
        max_threshold_ms: profile.tilt_max_threshold_ms as f32,
        multiplier: profile.tilt_multiplier,
    })
}

// Z-order layers for key gameplay visuals (higher draws on top)
const Z_RECEPTOR: i32 = 100;
const Z_HOLD_BODY: i32 = 110;
const Z_HOLD_CAP: i32 = 110;
const Z_HOLD_GLOW: i32 = 111;
// ITG draws GhostArrowRow after columns; keep hold/roll ghost arrows above note lanes.
const Z_HOLD_EXPLOSION: i32 = 145;
// ITG's Explosion actor declares hold/roll children before tap judgments, so taps render on top.
const Z_TAP_EXPLOSION: i32 = 150;
// ITG NoteField draws ReceptorArrowRow before column renderers, so receptor
// press glow must stay under hold bodies instead of cutting through them.
const Z_RECEPTOR_GLOW: i32 = 105;
const Z_MINE_EXPLOSION: i32 = 101;
const Z_TAP_NOTE: i32 = 140;
const Z_COLUMN_CUE: i32 = 90;
const Z_COLUMN_FLASH: i32 = 91;
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
// Arrow Cloud/zmod load Average.lua from ScreenGameplay underlay, below the
// engine Player/NoteField. Keep it behind receptors even with front judgments.
const Z_ERROR_BAR_AVERAGE: i16 = Z_ERROR_BAR_LINE_BACK;

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

/// Measure Cues: a colored line marking a timing event (BPM change / Stop /
/// Delay / Scroll). Drawn at `Z_MEASURE_LINES`, so when emitted after the white
/// measure-line pass it sits on top of any coinciding white line. `alpha`
/// mirrors the white lines' per-subdivision opacity.
fn append_cue_bar(
    actors: &mut Vec<Actor>,
    x_center: f32,
    y: f32,
    width: f32,
    thickness: f32,
    color: (f32, f32, f32),
    alpha: f32,
) {
    let (r, g, b) = color;
    actors.push(act!(quad:
        align(0.5, 0.5): xy(x_center, y):
        zoomto(width, thickness):
        diffuse(r, g, b, alpha):
        z(Z_MEASURE_LINES)
    ));
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

type FastTextCache<K> = TextCache<K, BuildHasherDefault<XxHash64>>;

thread_local! {
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
    static ERROR_BAR_TEXT_LABEL_CACHE: RefCell<FastTextCache<(bool, i32)>> = RefCell::new(
        HashMap::with_capacity_and_hasher(256, BuildHasherDefault::default()),
    );
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
    error_bar_mask: u8,
    avg_error_bar_intensity_centi: i16,
    avg_error_bar_interval_ms: u16,
    accel: [i16; 5],
    visual: [i16; 9],
    appearance: [i16; 5],
    scroll: [i16; 5],
    perspective_tilt: i16,
    perspective_skew: i16,
    dark: i16,
    blind: i16,
    cover: i16,
    disabled_timing_windows: u8,
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

#[inline(always)]
fn cached_error_bar_text_label(early: bool, scaled: bool) -> Arc<str> {
    let rounded = if scaled { -2 } else { -1 };
    cached_text(
        &ERROR_BAR_TEXT_LABEL_CACHE,
        (early, rounded),
        TEXT_CACHE_LIMIT,
        || {
            if scaled {
                if early { "FAST" } else { "SLOW" }.to_string()
            } else {
                if early { "EARLY" } else { "LATE" }.to_string()
            }
        },
    )
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
fn disabled_timing_window_bits(setting: profile_data::TimingWindowsOption) -> u8 {
    setting
        .disabled_windows()
        .into_iter()
        .enumerate()
        .fold(0, |bits, (i, disabled)| {
            bits | if disabled { 1 << i } else { 0 }
        })
}

#[inline(always)]
const fn turn_option_bits(turn: profile_data::TurnOption) -> u16 {
    match turn {
        profile_data::TurnOption::None => 0,
        profile_data::TurnOption::Mirror => DISPLAY_TURN_MIRROR,
        profile_data::TurnOption::Left => DISPLAY_TURN_LEFT,
        profile_data::TurnOption::Right => DISPLAY_TURN_RIGHT,
        profile_data::TurnOption::LRMirror => DISPLAY_TURN_LR_MIRROR,
        profile_data::TurnOption::UDMirror => DISPLAY_TURN_UD_MIRROR,
        profile_data::TurnOption::Shuffle => DISPLAY_TURN_SHUFFLE,
        profile_data::TurnOption::Blender => DISPLAY_TURN_BLENDER,
        profile_data::TurnOption::Random => DISPLAY_TURN_RANDOM,
    }
}

#[inline(always)]
fn gameplay_mods_attack_mode(mode: profile_data::AttackMode) -> GameplayModsAttackMode {
    match mode {
        profile_data::AttackMode::Off => GameplayModsAttackMode::Off,
        profile_data::AttackMode::On => GameplayModsAttackMode::On,
        profile_data::AttackMode::Random => GameplayModsAttackMode::Random,
    }
}

#[inline(always)]
fn profile_error_bar_mask(profile: &profile_data::Profile) -> profile_data::ErrorBarMask {
    if profile.error_bar_active_mask.is_empty() {
        profile_data::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text)
    } else {
        profile.error_bar_active_mask
    }
}

#[inline(always)]
fn effective_accel_effects_for_player(state: &State, player_idx: usize) -> AccelEffects {
    if player_idx >= state.num_players() || player_idx >= MAX_PLAYERS {
        return AccelEffects::default();
    }
    state.effective_accel_effects_for_player_with_mask(
        player_idx,
        state.profiles()[player_idx]
            .accel_effects_active_mask
            .bits(),
    )
}

#[inline(always)]
fn effective_visual_effects_for_player(state: &State, player_idx: usize) -> VisualEffects {
    if player_idx >= state.num_players() || player_idx >= MAX_PLAYERS {
        return VisualEffects::default();
    }
    state.effective_visual_effects_for_player_with_mask(
        player_idx,
        state.profiles()[player_idx]
            .visual_effects_active_mask
            .bits(),
    )
}

#[inline(always)]
fn effective_scroll_effects_for_player(state: &State, player_idx: usize) -> ScrollEffects {
    if player_idx >= state.num_players() || player_idx >= MAX_PLAYERS {
        return ScrollEffects::default();
    }
    state.effective_scroll_effects_for_player_with_base(
        player_idx,
        scroll_effects_from_option(state.profiles()[player_idx].scroll_option),
    )
}

#[inline(always)]
fn effective_perspective_effects_for_player(
    state: &State,
    player_idx: usize,
) -> PerspectiveEffects {
    if player_idx >= state.num_players() || player_idx >= MAX_PLAYERS {
        return PerspectiveEffects::default();
    }
    state.effective_perspective_effects_for_player_with_base(
        player_idx,
        perspective_effects_from_profile(&state.profiles()[player_idx]),
    )
}

#[inline(always)]
fn effective_mini_percent_for_player(state: &State, player_idx: usize) -> f32 {
    if player_idx >= state.num_players() || player_idx >= MAX_PLAYERS {
        return 0.0;
    }
    state.effective_mini_percent_for_player_with_base(
        player_idx,
        state.profiles()[player_idx].mini_percent as f32,
    )
}

#[inline(always)]
fn effective_spacing_multiplier_for_player(state: &State, player_idx: usize) -> f32 {
    if player_idx >= state.num_players() {
        return 1.0;
    }
    spacing_multiplier_for_percent(state.profiles()[player_idx].spacing_percent)
}

#[inline(always)]
fn gameplay_mods_text_key(state: &State, player_idx: usize) -> GameplayModsTextKey {
    let profile = &state.profiles()[player_idx];
    let chart_attack = state.active_chart_attack_effects_for_player(player_idx);
    let scroll_speed = state.effective_scroll_speed_for_player(player_idx);
    let accel = effective_accel_effects_for_player(state, player_idx);
    let visual = effective_visual_effects_for_player(state, player_idx);
    let appearance = state.effective_appearance_effects_for_player(player_idx);
    let visibility = state.effective_visibility_effects_for_player(player_idx);
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
    let error_bar_mask = profile_error_bar_mask(profile);
    let average_error_bar_intensity =
        profile_data::clamp_average_error_bar_intensity(profile.average_error_bar_intensity);
    let average_error_bar_interval_ms =
        profile_data::clamp_average_error_bar_interval_ms(profile.average_error_bar_interval_ms);
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
        error_bar_mask: error_bar_mask.bits(),
        avg_error_bar_intensity_centi: clamp_rounded_i16(average_error_bar_intensity * 100.0),
        avg_error_bar_interval_ms: average_error_bar_interval_ms as u16,
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
        disabled_timing_windows: disabled_timing_window_bits(profile.timing_windows),
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
    pub hide_combo: bool,
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
fn hold_explosion_slot_for_col(
    explosion_ns: Option<&Noteskin>,
    col: usize,
    is_roll: bool,
) -> Option<&SpriteSlot> {
    let ns = explosion_ns?;
    let visuals = ns.hold_visuals_for_col(col, is_roll);
    visuals.explosion.as_ref().or_else(|| {
        if is_roll {
            ns.roll.explosion.as_ref()
        } else {
            ns.hold.explosion.as_ref()
        }
    })
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
fn field_effect_height(tilt: f32) -> f32 {
    field_effect_height_for_screen(screen_height(), tilt)
}

#[inline(always)]
fn accel_y_params(accel: AccelEffects) -> AccelYParams {
    AccelYParams {
        boost: accel.boost,
        brake: accel.brake,
        wave: accel.wave,
        boomerang: accel.boomerang,
        expand: accel.expand,
    }
}

#[inline(always)]
fn apply_accel_y_with_peak(
    raw_y: f32,
    elapsed: f32,
    _current_beat: f32,
    effect_height: f32,
    accel: AccelEffects,
) -> (f32, bool) {
    crate_apply_accel_y_with_peak(
        raw_y,
        elapsed,
        effect_height,
        screen_height(),
        accel_y_params(accel),
    )
}

#[inline(always)]
fn apply_accel_y(
    raw_y: f32,
    elapsed: f32,
    _current_beat: f32,
    effect_height: f32,
    accel: AccelEffects,
) -> f32 {
    crate_apply_accel_y(
        raw_y,
        elapsed,
        effect_height,
        screen_height(),
        accel_y_params(accel),
    )
}

#[inline(always)]
fn arrow_effect_game_time_seconds() -> f32 {
    deadlib_platform::host_time::instant_nanos(Instant::now()) as f32 / 1_000_000_000.0
}

#[inline(always)]
fn note_glow(y_no_reverse: f32, elapsed: f32, mini: f32, appearance: AppearanceEffects) -> f32 {
    appearance_note_glow(y_no_reverse, elapsed, mini, note_alpha_params(appearance))
}

#[inline(always)]
fn note_actor_alpha(
    y_no_reverse: f32,
    elapsed: f32,
    mini: f32,
    appearance: AppearanceEffects,
) -> f32 {
    appearance_note_actor_alpha(y_no_reverse, elapsed, mini, note_alpha_params(appearance))
}

#[inline(always)]
fn hold_alpha_needs_rows(appearance: AppearanceEffects) -> bool {
    appearance_needs_rows(note_alpha_params(appearance))
}

#[inline(always)]
fn note_alpha_params(appearance: AppearanceEffects) -> NoteAlphaParams {
    NoteAlphaParams {
        hidden: appearance.hidden,
        hidden_offset: appearance.hidden_offset,
        sudden: appearance.sudden,
        sudden_offset: appearance.sudden_offset,
        stealth: appearance.stealth,
        blink: appearance.blink,
        random_vanish: appearance.random_vanish,
    }
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
    crate_note_x_offset(
        local_col,
        y,
        elapsed,
        beat_factor,
        col_offsets,
        invert_distances,
        tornado_bounds,
        &visual.move_x_cols,
        NoteXParams {
            screen_height: screen_height(),
            tornado: visual.tornado,
            drunk: visual.drunk,
            flip: visual.flip,
            invert: visual.invert,
            beat: visual.beat,
        },
        visual.tiny,
    )
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
    crate_receptor_row_center(
        playfield_center_x,
        local_col,
        receptor_y_lane,
        elapsed,
        beat_factor,
        col_offsets,
        invert_distances,
        tornado_bounds,
        &visual.move_x_cols,
        &visual.move_y_cols,
        NoteXParams {
            screen_height: screen_height(),
            tornado: visual.tornado,
            drunk: visual.drunk,
            flip: visual.flip,
            invert: visual.invert,
            beat: visual.beat,
        },
        visual.tiny,
        visual.tipsy,
    )
}

#[inline(always)]
fn hold_indicator_column_x(
    playfield_center_x: f32,
    local_col: usize,
    elapsed: f32,
    beat_factor: f32,
    visual: VisualEffects,
    col_offsets: &[f32],
    invert_distances: &[f32],
    tornado_bounds: &[TornadoBounds],
) -> f32 {
    crate_hold_indicator_column_x(
        playfield_center_x,
        local_col,
        elapsed,
        beat_factor,
        col_offsets,
        invert_distances,
        tornado_bounds,
        &visual.move_x_cols,
        NoteXParams {
            screen_height: screen_height(),
            tornado: visual.tornado,
            drunk: visual.drunk,
            flip: visual.flip,
            invert: visual.invert,
            beat: visual.beat,
        },
        visual.tiny,
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
        geom_cache_key: deadlib_render::INVALID_TMESH_CACHE_KEY,
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
fn hold_strip_glow_actor(
    texture: Arc<str>,
    vertices: Arc<[TexturedMeshVertex]>,
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
        tint: [1.0, 1.0, 1.0, 0.0],
        glow: [1.0, 1.0, 1.0, 1.0],
        vertices,
        geom_cache_key: deadlib_render::INVALID_TMESH_CACHE_KEY,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        depth_test,
        visible: true,
        blend: BlendMode::Alpha,
        z,
    }
}

#[inline(always)]
fn visual_effect_params(visual: &VisualEffects, local_col: usize) -> VisualEffectParams {
    crate_visual_effect_params_for_col(
        VisualEffectParams {
            tiny: visual.tiny,
            pulse_inner: visual.pulse_inner,
            pulse_outer: visual.pulse_outer,
            pulse_offset: visual.pulse_offset,
            pulse_period: visual.pulse_period,
            confusion: visual.confusion,
            confusion_offset: visual.confusion_offset,
            dizzy: visual.dizzy,
            bumpy: visual.bumpy,
        },
        local_col,
        &visual.tiny_cols,
        &visual.confusion_offset_cols,
        &visual.bumpy_cols,
    )
}

#[inline(always)]
fn hold_body_needs_z_buffer(visual: &VisualEffects) -> bool {
    visual_hold_body_needs_z_buffer(VisualEffectParams {
        bumpy: visual.bumpy,
        ..VisualEffectParams::default()
    })
}

#[inline(always)]
fn tiny_zoom_for_col(visual: &VisualEffects, local_col: usize) -> f32 {
    visual_tiny_zoom(visual_effect_params(visual, local_col))
}

#[inline(always)]
fn pulse_zoom_for_y(y: f32, visual: &VisualEffects) -> f32 {
    visual_pulse_zoom_for_y(y, visual_effect_params(visual, 0))
}

#[inline(always)]
fn arrow_effect_zoom(visual: &VisualEffects, local_col: usize, y: f32) -> f32 {
    visual_arrow_effect_zoom(y, visual_effect_params(visual, local_col))
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
    let glow_alpha = itg_actor_glow_alpha(spec.alpha);
    if glow_alpha <= f32::EPSILON {
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
            *glow = [1.0, 1.0, 1.0, glow_alpha];
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
                glow(1.0, 1.0, 1.0, glow_alpha):
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
                glow(1.0, 1.0, 1.0, glow_alpha):
                blend(normal):
                z(spec.z as i32)
            ),
            spec.world_z,
        ));
    }
}

#[inline(always)]
fn confusion_rotation_deg(song_beat: f32, visual: VisualEffects, local_col: usize) -> f32 {
    visual_confusion_rotation_deg(song_beat, visual_effect_params(&visual, local_col))
}

#[inline(always)]
fn calc_note_rotation_z(
    visual: VisualEffects,
    note_beat: f32,
    song_beat: f32,
    is_hold_head: bool,
    local_col: usize,
) -> f32 {
    visual_note_rotation_z(
        note_beat,
        song_beat,
        is_hold_head,
        visual_effect_params(&visual, local_col),
    )
}

#[inline(always)]
fn song_lua_note_model_draw(mut draw: ModelDrawState, rotation_y_deg: f32) -> ModelDrawState {
    if rotation_y_deg.abs() > f32::EPSILON {
        draw.rot[1] += rotation_y_deg;
    }
    draw
}

#[inline(always)]
fn effective_mini_value(
    profile: &profile_data::Profile,
    visual: VisualEffects,
    mini_percent: f32,
) -> f32 {
    crate_effective_mini_value(mini_percent, profile.mini_percent as f32, visual.big)
}

#[inline(always)]
pub(crate) fn gameplay_mods_text(state: &State, player_idx: usize) -> Arc<str> {
    let key = gameplay_mods_text_key(state, player_idx);
    cached_text(&GAMEPLAY_MODS_CACHE, key, TEXT_CACHE_LIMIT, || {
        let profile = &state.profiles()[player_idx];
        crate_gameplay_mods_text(GameplayModsTextParams {
            speed: state.effective_scroll_speed_for_player(player_idx),
            noteskin: profile.noteskin.as_str(),
            insert_mask: key.insert_mask,
            remove_mask: key.remove_mask,
            holds_mask: key.holds_mask,
            turn_bits: key.turn_bits,
            attack_mode: gameplay_mods_attack_mode(profile.attack_mode),
            mini_percent: key.mini_percent,
            spacing_percent: key.spacing_percent,
            visual_delay_ms: key.visual_delay_ms,
            average_error_bar_active: key.error_bar_mask
                & profile_data::ErrorBarMask::AVERAGE.bits()
                != 0,
            avg_error_bar_intensity_centi: key.avg_error_bar_intensity_centi,
            avg_error_bar_interval_ms: key.avg_error_bar_interval_ms,
            accel: key.accel,
            visual: key.visual,
            appearance: key.appearance,
            scroll: key.scroll,
            perspective_tilt: key.perspective_tilt,
            perspective_skew: key.perspective_skew,
            dark: key.dark,
            blind: key.blind,
            cover: key.cover,
            disabled_timing_windows: key.disabled_timing_windows,
        })
    })
}

#[inline(always)]
fn column_flash_dimmed(brightness: profile_data::ColumnFlashBrightness) -> bool {
    matches!(brightness, profile_data::ColumnFlashBrightness::Dimmed)
}

#[inline(always)]
fn resolved_judgment_texture(
    profile: &profile_data::Profile,
) -> Option<&'static assets::TextureChoice> {
    assets::resolve_texture_choice_entry(
        profile.judgment_graphic.texture_key(),
        assets::judgment_texture_choices(),
    )
}

#[inline(always)]
fn resolved_hold_judgment_texture(
    profile: &profile_data::Profile,
) -> Option<&'static assets::TextureChoice> {
    assets::resolve_texture_choice_entry(
        profile.hold_judgment_graphic.texture_key(),
        assets::hold_judgment_texture_choices(),
    )
}

#[inline(always)]
fn resolved_held_miss_texture(
    profile: &profile_data::Profile,
) -> Option<&'static assets::TextureChoice> {
    assets::resolve_texture_choice_entry(
        profile.held_miss_graphic.texture_key(),
        assets::held_miss_texture_choices(),
    )
}

#[inline(always)]
fn judgment_frame_size(texture_key: &str) -> [f32; 2] {
    let Some(meta) = assets::texture_dims(texture_key) else {
        return [0.0, 76.0];
    };
    let (w, h) = assets::texture_source_frame_dims_from_real(texture_key, meta.w, meta.h);
    [w as f32, h as f32]
}

#[inline(always)]
fn tap_judgment_rows(
    profile: &profile_data::Profile,
    judgment: &Judgment,
    frame_rows: usize,
) -> (usize, Option<usize>) {
    crate_tap_judgment_rows(TapJudgmentRowsParams {
        grade: judgment.grade,
        window: judgment.window,
        time_error_ms: judgment.time_error_ms,
        frame_rows,
        show_fa_plus_window: profile.show_fa_plus_window,
        fa_plus_10ms_blue_window: profile.fa_plus_10ms_blue_window,
        split_15_10ms: profile.split_15_10ms,
        custom_fantastic_window: profile.custom_fantastic_window,
    })
}

#[inline(always)]
fn gameplay_error_bar_trim(trim: profile_data::ErrorBarTrim) -> GameplayErrorBarTrim {
    match trim {
        profile_data::ErrorBarTrim::Off => GameplayErrorBarTrim::Off,
        profile_data::ErrorBarTrim::Fantastic => GameplayErrorBarTrim::Fantastic,
        profile_data::ErrorBarTrim::Excellent => GameplayErrorBarTrim::Excellent,
        profile_data::ErrorBarTrim::Great => GameplayErrorBarTrim::Great,
    }
}

#[inline(always)]
fn error_bar_trim_max_window_ix(trim: profile_data::ErrorBarTrim) -> usize {
    gameplay_error_bar_trim_max_window_ix(gameplay_error_bar_trim(trim))
}

#[inline(always)]
fn zmod_layout_params(profile: &profile_data::Profile) -> ZmodLayoutParams {
    // Zmod SL-Layout.lua: hasErrorBar checks multiple flags.
    let mut error_bar_mask = profile.error_bar_active_mask;
    if error_bar_mask.is_empty() {
        error_bar_mask =
            profile_data::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text);
    }
    let has_error_bar = !error_bar_mask.is_empty();
    let mini_indicator_position = match profile.mini_indicator_position {
        profile_data::MiniIndicatorPosition::Default => LayoutMiniIndicatorPosition::Default,
        profile_data::MiniIndicatorPosition::UnderUpArrow => {
            LayoutMiniIndicatorPosition::UnderUpArrow
        }
    };
    ZmodLayoutParams {
        judgment_height: ERROR_BAR_JUDGMENT_HEIGHT,
        has_error_bar,
        has_judgment_texture: resolved_judgment_texture(profile).is_some(),
        error_bar_up: profile.error_bar_up,
        has_measure_counter: profile.measure_counter != profile_data::MeasureCounter::None,
        measure_counter_up: profile.measure_counter_up,
        broken_run: profile.broken_run,
        mini_indicator_position,
    }
}

#[inline(always)]
fn hud_layout_ys(
    profile: &profile_data::Profile,
    judgment_y_base: f32,
    combo_y_base: f32,
    reverse: bool,
    judgment_extra_y: f32,
    combo_extra_y: f32,
    error_bar_extra_y: f32,
) -> HudLayoutYs {
    crate_hud_layout_ys(
        judgment_y_base,
        combo_y_base,
        reverse,
        HudLayoutOffsets {
            judgment_extra_y,
            combo_extra_y,
            error_bar_extra_y,
        },
        HudLayoutParams {
            zmod: zmod_layout_params(profile),
            has_judgment_texture: resolved_judgment_texture(profile).is_some(),
            error_bar_up: profile.error_bar_up,
            error_bar_offset: ERROR_BAR_OFFSET_FROM_JUDGMENT,
        },
    )
}

fn cached_zmod_measure_counter_text(text: ZmodMeasureCounterText) -> Arc<str> {
    match text {
        ZmodMeasureCounterText::Break(value) => cached_paren_i32(value),
        ZmodMeasureCounterText::Ratio { current, total } => cached_ratio_i32(current, total),
        ZmodMeasureCounterText::Total(value) => cached_int_i32(value),
    }
}

#[inline(always)]
fn zmod_run_timer_fmt(seconds: i32, minute_threshold: i32, trailing_space: bool) -> Arc<str> {
    cached_run_timer(seconds, minute_threshold, trailing_space)
}

#[inline(always)]
fn zmod_small_combo_font(combo_font: profile_data::ComboFont) -> &'static str {
    match combo_font {
        profile_data::ComboFont::Wendy | profile_data::ComboFont::WendyCursed => "wendy",
        profile_data::ComboFont::ArialRounded => "combo_arial_rounded",
        profile_data::ComboFont::Asap => "combo_asap",
        profile_data::ComboFont::BebasNeue => "combo_bebas_neue",
        profile_data::ComboFont::SourceCode => "combo_source_code",
        profile_data::ComboFont::Work => "combo_work",
        profile_data::ComboFont::Mega => "combo_mega",
        profile_data::ComboFont::None => "wendy",
    }
}

#[inline(always)]
fn zmod_combo_font_name(combo_font: profile_data::ComboFont) -> Option<&'static str> {
    match combo_font {
        profile_data::ComboFont::Wendy => Some("wendy_combo"),
        profile_data::ComboFont::ArialRounded => Some("combo_arial_rounded"),
        profile_data::ComboFont::Asap => Some("combo_asap"),
        profile_data::ComboFont::BebasNeue => Some("combo_bebas_neue"),
        profile_data::ComboFont::SourceCode => Some("combo_source_code"),
        profile_data::ComboFont::Work => Some("combo_work"),
        profile_data::ComboFont::WendyCursed => Some("combo_wendy_cursed"),
        profile_data::ComboFont::Mega => Some("combo_mega"),
        profile_data::ComboFont::None => None,
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
    let music_end_seconds =
        deadsync_core::song_time::song_time_ns_to_seconds(state.music_end_time_ns())
            .ceil()
            .max(0.0) as i32;

    for player in 0..state.num_players() {
        let profile = &state.profiles()[player];
        let totals = state.display_totals_for_player(player);
        max_combo = max_combo.max(
            totals
                .total_steps
                .saturating_add(totals.holds_total)
                .saturating_add(totals.rolls_total),
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
        let segs = state.measure_counter_segments(player);
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
        if profile.measure_counter != profile_data::MeasureCounter::None {
            let countdown_max = max_measure_len.clamp(16, MEASURE_PREWARM_CAP);
            for value in 0..=countdown_max {
                prewarm_i32(cache, mc_font_name, value);
            }
            prewarm_i32(cache, mc_font_name, max_measure_len.max(16));
        }
        if zmod_indicator_mode(profile) != MiniIndicatorMode::None {
            for &value in &[0.0, 50.0, 89.0, 95.0, 100.0] {
                prewarm_percent(cache, mc_font_name, value);
                prewarm_signed_percent(cache, mc_font_name, value, true);
                prewarm_signed_percent(cache, mc_font_name, value, false);
            }
            prewarm_percent(
                cache,
                mc_font_name,
                state.mini_indicator_target_score_percent(player),
            );
            prewarm_percent(
                cache,
                mc_font_name,
                state.mini_indicator_rival_score_percent(player),
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
fn zmod_combo_quint_active(
    state: &State,
    player_idx: usize,
    profile: &profile_data::Profile,
) -> bool {
    if player_idx >= state.num_players() {
        return false;
    }
    let counts = if profile.combo_mode == profile_data::ComboMode::FullCombo {
        let blue_window_ms = player_blue_window_ms(state, player_idx);
        state.display_window_counts(player_idx, None, blue_window_ms)
    } else {
        state.players()[player_idx].current_combo_window_counts
    };
    crate_zmod_combo_quint_active(profile.show_fa_plus_window, counts)
}

#[inline(always)]
fn zmod_combo_color_style(colors: profile_data::ComboColors) -> ZmodComboColorStyle {
    match colors {
        profile_data::ComboColors::None => ZmodComboColorStyle::None,
        profile_data::ComboColors::Rainbow => ZmodComboColorStyle::Rainbow,
        profile_data::ComboColors::RainbowScroll => ZmodComboColorStyle::RainbowScroll,
        profile_data::ComboColors::Glow => ZmodComboColorStyle::Glow,
        profile_data::ComboColors::Solid => ZmodComboColorStyle::Solid,
    }
}

fn zmod_combo_color_params(
    state: &State,
    p: &PlayerRuntime,
    profile: &profile_data::Profile,
    player_idx: usize,
) -> ZmodComboColorParams {
    ZmodComboColorParams {
        style: zmod_combo_color_style(profile.combo_colors),
        full_combo_mode: profile.combo_mode == profile_data::ComboMode::FullCombo,
        combo: p.combo,
        full_combo_grade: p.full_combo_grade,
        current_combo_grade: p.current_combo_grade,
        quint_active: zmod_combo_quint_active(state, player_idx, profile),
        elapsed_s: state.total_elapsed_in_screen(),
    }
}

fn zmod_resolved_combo_color(
    state: &State,
    p: &PlayerRuntime,
    profile: &profile_data::Profile,
    player_idx: usize,
) -> [f32; 4] {
    crate_zmod_resolved_combo_color(zmod_combo_color_params(state, p, profile, player_idx))
}

fn zmod_static_combo_color(
    state: &State,
    p: &PlayerRuntime,
    profile: &profile_data::Profile,
    player_idx: usize,
) -> [f32; 4] {
    crate_zmod_static_combo_color(zmod_combo_color_params(state, p, profile, player_idx))
}

fn zmod_mini_indicator_progress(
    state: &State,
    p: &PlayerRuntime,
    player_idx: usize,
    score_type: profile_data::MiniIndicatorScoreType,
) -> MiniIndicatorProgress {
    let w1 = p.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Fantastic)];
    let w2 = p.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Excellent)];
    let w3 = p.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Great)];
    let w4 = p.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Decent)];
    let w5 = p.scoring_counts[judgment::judge_grade_ix(JudgeGrade::WayOff)];
    let miss = p.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Miss)];

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

    let possible_dp = state
        .display_totals_for_player(player_idx)
        .possible_grade_points
        .max(1);
    let actual_dp = p.earned_grade_points;

    // Compute predictive percents for the active score type.
    let (
        kept_percent,
        lost_percent,
        pace_percent,
        current_score_percent,
        current_possible_ratio,
        white_count,
        white_10ms_count,
    ) = match score_type {
        profile_data::MiniIndicatorScoreType::Itg => {
            let (kept, lost, pace) = judgment::predictive_itg_score_percents(
                current_possible_dp,
                possible_dp,
                actual_dp,
            );
            let current_score = zmod_percent_from_points(actual_dp, possible_dp);
            let current_possible_ratio =
                (f64::from(current_possible_dp.max(0)) / f64::from(possible_dp)).clamp(0.0, 1.0);
            (
                kept,
                lost,
                pace,
                current_score,
                current_possible_ratio,
                0,
                0,
            )
        }
        profile_data::MiniIndicatorScoreType::Ex | profile_data::MiniIndicatorScoreType::HardEx => {
            let blue_window_ms = player_blue_window_ms(state, player_idx);
            let score = state.display_scored_ex_score_data(player_idx, blue_window_ms);
            let white_count = score.counts.w1;
            let fantastic_total = score.counts.w0.saturating_add(score.counts.w1);
            let white_10ms_count = fantastic_total.saturating_sub(score.counts_10ms.w0);
            let current_possible_ratio = judgment::ex_current_possible_ratio(&score);
            if score_type == profile_data::MiniIndicatorScoreType::Ex {
                let (kept, lost, pace) = judgment::predictive_ex_score_percents(&score);
                (
                    kept,
                    lost,
                    pace,
                    judgment::ex_score_percent(&score),
                    current_possible_ratio,
                    white_count,
                    white_10ms_count,
                )
            } else {
                let (kept, lost, pace) = judgment::predictive_hard_ex_score_percents(&score);
                (
                    kept,
                    lost,
                    pace,
                    judgment::hard_ex_score_percent(&score),
                    current_possible_ratio,
                    white_count,
                    white_10ms_count,
                )
            }
        }
    };

    let judged_any = tap_rows > 0 || let_go > 0 || mines_hit > 0 || p.is_failing || p.life <= 0.0;
    MiniIndicatorProgress {
        kept_percent,
        lost_percent,
        pace_percent,
        current_score_percent,
        current_possible_ratio,
        current_possible_dp,
        actual_dp,
        white_count,
        white_10ms_count,
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
fn mini_indicator_score_type(
    score_type: profile_data::MiniIndicatorScoreType,
) -> MiniIndicatorScoreType {
    match score_type {
        profile_data::MiniIndicatorScoreType::Itg => MiniIndicatorScoreType::Itg,
        profile_data::MiniIndicatorScoreType::Ex => MiniIndicatorScoreType::Ex,
        profile_data::MiniIndicatorScoreType::HardEx => MiniIndicatorScoreType::HardEx,
    }
}

#[inline(always)]
fn mini_indicator_mode(mode: profile_data::MiniIndicator) -> MiniIndicatorMode {
    match mode {
        profile_data::MiniIndicator::None => MiniIndicatorMode::None,
        profile_data::MiniIndicator::SubtractiveScoring => MiniIndicatorMode::SubtractiveScoring,
        profile_data::MiniIndicator::PredictiveScoring => MiniIndicatorMode::PredictiveScoring,
        profile_data::MiniIndicator::PaceScoring => MiniIndicatorMode::PaceScoring,
        profile_data::MiniIndicator::RivalScoring => MiniIndicatorMode::RivalScoring,
        profile_data::MiniIndicator::Pacemaker => MiniIndicatorMode::Pacemaker,
        profile_data::MiniIndicator::StreamProg => MiniIndicatorMode::StreamProg,
    }
}

#[inline(always)]
fn zmod_indicator_mode(profile: &profile_data::Profile) -> MiniIndicatorMode {
    zmod_resolved_mini_indicator_mode(
        mini_indicator_mode(profile.mini_indicator),
        profile.subtractive_scoring,
        profile.pacemaker,
    )
}

#[inline(always)]
fn mini_indicator_color_style(style: profile_data::MiniIndicatorColor) -> MiniIndicatorColorStyle {
    match style {
        profile_data::MiniIndicatorColor::Default => MiniIndicatorColorStyle::Default,
        profile_data::MiniIndicatorColor::Detailed => MiniIndicatorColorStyle::Detailed,
        profile_data::MiniIndicatorColor::Combo => MiniIndicatorColorStyle::Combo,
    }
}

#[inline(always)]
fn mini_indicator_subtractive_display(
    display: profile_data::MiniIndicatorSubtractiveDisplay,
) -> MiniIndicatorSubtractiveDisplay {
    match display {
        profile_data::MiniIndicatorSubtractiveDisplay::Percent => {
            MiniIndicatorSubtractiveDisplay::CountThenPercent
        }
        profile_data::MiniIndicatorSubtractiveDisplay::Points => {
            MiniIndicatorSubtractiveDisplay::Points
        }
    }
}

#[inline(always)]
fn zmod_mini_indicator_zoom(size: profile_data::MiniIndicatorSize) -> f32 {
    let size = match size {
        profile_data::MiniIndicatorSize::Default => MiniIndicatorSize::Default,
        profile_data::MiniIndicatorSize::Large => MiniIndicatorSize::Large,
    };
    crate_zmod_mini_indicator_zoom(size)
}

fn zmod_stream_prog_completion(state: &State, player_idx: usize) -> Option<f64> {
    let total_stream = state.mini_indicator_total_stream_measures(player_idx) as f64;
    let segs = state.mini_indicator_stream_segments(player_idx);
    let beat_floor = state.visible_beat(player_idx).floor();
    zmod_stream_prog_completion_for_beat(total_stream, segs, beat_floor)
}

fn cached_zmod_mini_indicator_text(text: ZmodMiniIndicatorText) -> Arc<str> {
    match text {
        ZmodMiniIndicatorText::Percent(value) => cached_percent2_f64(value),
        ZmodMiniIndicatorText::SignedPercent { value, negative } => {
            cached_signed_percent2_f64(value, negative)
        }
        ZmodMiniIndicatorText::NegativeInt(value) => cached_neg_int_u32(value),
    }
}

fn zmod_mini_indicator_text(
    state: &State,
    p: &PlayerRuntime,
    profile: &profile_data::Profile,
    player_idx: usize,
) -> Option<(Arc<str>, [f32; 4])> {
    let mode = zmod_indicator_mode(profile);
    let progress =
        zmod_mini_indicator_progress(state, p, player_idx, profile.mini_indicator_score_type);
    let output = zmod_mini_indicator_output(
        &progress,
        ZmodMiniIndicatorParams {
            mode,
            color_style: mini_indicator_color_style(profile.mini_indicator_color),
            subtractive_display: mini_indicator_subtractive_display(
                profile.mini_indicator_subtractive_display,
            ),
            score_type: mini_indicator_score_type(profile.mini_indicator_score_type),
            combo_color: zmod_static_combo_color(state, p, profile, player_idx),
            is_failing: p.is_failing,
            life: p.life,
            rival_score_percent: state.mini_indicator_rival_score_percent(player_idx),
            target_score_percent: state.mini_indicator_target_score_percent(player_idx),
            stream_completion: zmod_stream_prog_completion(state, player_idx),
        },
    )?;
    Some((cached_zmod_mini_indicator_text(output.text), output.color))
}

#[inline(always)]
fn hold_explosion_enabled(profile: &profile_data::Profile) -> bool {
    hold_explosion_enabled_for_options(tap_explosion_options_from_profile(profile))
}

#[inline(always)]
fn song_lua_hides_note(state: &State, player: usize, local_col: usize, beat: f32) -> bool {
    song_lua_note_hidden(
        &state.song_lua_visuals().note_hides[player],
        local_col,
        beat,
    )
}

pub(crate) fn build_bundles(
    state: &State,
    noteskin_assets: &GameplayNoteskinAssets,
    model_caches: &[RefCell<ModelMeshCache>; MAX_PLAYERS],
    profile: &profile_data::Profile,
    placement: FieldPlacement,
    play_style: profile_data::PlayStyle,
    center_1player_notefield: bool,
    capture_requests: ProxyCaptureRequests,
    view: ViewOverride,
    mut actors: &mut Vec<Actor>,
    mut hud_actors: &mut Vec<Actor>,
) -> BuiltNotefield {
    actors.clear();
    hud_actors.clear();
    let hold_judgment_texture = resolved_hold_judgment_texture(profile);
    let held_miss_texture = resolved_held_miss_texture(profile);

    // --- Playfield Positioning (1:1 with Simply Love) ---
    // In P2-only single-player, we still have a single player runtime (index 0),
    // but need to place the notefield on the P2 side of the screen.
    let player_idx = if state.num_players() == 1 {
        0
    } else {
        match placement {
            FieldPlacement::P1 => 0,
            FieldPlacement::P2 => 1,
        }
    };
    if player_idx >= state.num_players() {
        return BuiltNotefield::empty(screen_center_x());
    }
    // Use the cached field_zoom from gameplay state so visual layout and
    // scroll math share the exact same scaling as gameplay. Practice edit
    // mode overrides this to match ScreenEdit's half-scale edit field.
    let field_zoom = view
        .field_zoom
        .unwrap_or_else(|| state.field_zoom_for_player(player_idx));
    let draw_distance_before_targets = state.notefield_draw_distance_before_targets(player_idx);
    let draw_distance_after_targets = state.notefield_draw_distance_after_targets(player_idx);
    let scroll_speed = view
        .scroll_speed
        .unwrap_or_else(|| state.effective_scroll_speed_for_player(player_idx));
    let col_start = player_idx * state.cols_per_player();
    let col_end = (col_start + state.cols_per_player())
        .min(state.num_cols())
        .min(MAX_COLS);
    let num_cols = col_end.saturating_sub(col_start);
    if num_cols == 0 {
        return BuiltNotefield::empty(screen_center_x());
    }
    let error_bar_mask = {
        let mut mask = profile.error_bar_active_mask;
        if mask.is_empty() {
            mask =
                profile_data::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text);
        }
        mask
    };
    let measure_line_extra = if view.edit_beat_bars {
        72
    } else {
        match profile.measure_lines {
            profile_data::MeasureLines::Off => 0,
            profile_data::MeasureLines::Measure => 18,
            profile_data::MeasureLines::Quarter => 30,
            profile_data::MeasureLines::Eighth => 42,
        }
    };
    let actor_cap = (num_cols * 10).max(28)
        + measure_line_extra
        + if profile.measure_cues { 32 } else { 0 }
        + if profile.column_cues { num_cols + 4 } else { 0 }
        + if profile.column_flash_on_miss {
            num_cols
        } else {
            0
        }
        + if !error_bar_mask.is_empty() { 18 } else { 0 };
    let hud_cap = 8
        + if profile.column_cues { 1 } else { 0 }
        + if held_miss_texture.is_some() {
            num_cols
        } else {
            0
        }
        + if profile.hide_combo || view.hide_combo {
            0
        } else {
            2
        }
        + if error_bar_mask.contains(profile_data::ErrorBarMask::TEXT) {
            1
        } else {
            0
        };
    actors.reserve(actor_cap);
    hud_actors.reserve(hud_cap);
    let p = &state.players()[player_idx];
    let mut model_cache = model_caches[player_idx].borrow_mut();

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
        .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
        as f32;
    let judgment_extra_y = profile
        .judgment_offset_y
        .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
        as f32;
    let combo_extra_x = profile
        .combo_offset_x
        .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
        as f32;
    let combo_extra_y = profile
        .combo_offset_y
        .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
        as f32;
    let error_bar_extra_x = profile
        .error_bar_offset_x
        .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
        as f32;
    let error_bar_extra_y = profile
        .error_bar_offset_y
        .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
        as f32;
    let logical_screen_width = screen_width();
    let clamped_width = logical_screen_width.clamp(640.0, 854.0);
    let centered_one_side = state.num_players() == 1
        && play_style == profile_data::PlayStyle::Single
        && center_1player_notefield;
    let centered_both_sides =
        state.num_players() == 1 && play_style == profile_data::PlayStyle::Double;
    let base_playfield_center_x = if state.num_players() == 2 {
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
    let layout_center_x = if state.num_players() == 1 && (centered_both_sides || centered_one_side)
    {
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
        state.notefield_column_scroll_dir(col_start + i)
    });
    let base_column_receptor_ys: [f32; MAX_COLS] = from_fn(|i| {
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
    let current_time_ns = state.visible_music_time_ns(player_idx);
    let current_time = song_time_ns_to_seconds(current_time_ns);
    let current_beat = state.visible_beat(player_idx);
    let column_receptor_ys: [f32; MAX_COLS] = from_fn(|i| {
        if i >= num_cols {
            return base_column_receptor_ys[i];
        }
        base_column_receptor_ys[i]
            + song_lua_column_y_offset(
                &state.song_lua_visuals().column_offsets[player_idx],
                i,
                current_time,
            ) * field_zoom
    });

    let elapsed_screen = state.total_elapsed_in_screen();
    // ITG's default ArrowEffects timer is RageTimer::GetTimeSinceStart, not
    // music time or time since entering gameplay.
    let arrow_effect_time = arrow_effect_game_time_seconds();
    let accel = effective_accel_effects_for_player(state, player_idx);
    let visual = effective_visual_effects_for_player(state, player_idx);
    let appearance = state.effective_appearance_effects_for_player(player_idx);
    let visibility = state.effective_visibility_effects_for_player(player_idx);
    let mini_percent = effective_mini_percent_for_player(state, player_idx);
    let mini = effective_mini_value(profile, visual, mini_percent);
    let spacing_mult = effective_spacing_multiplier_for_player(state, player_idx);
    let reverse_scroll = state.notefield_reverse_scroll(player_idx);
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
    let judgment_zoom_mod = judgment_actor_zoom(
        mini,
        profile.judgment_back,
        perspective.tilt,
        perspective.skew,
    );
    let combo_zoom_mod = combo_actor_zoom(mini);
    let effect_height = field_effect_height(perspective.tilt);
    let receptor_alpha = (1.0 - visibility.dark).clamp(0.0, 1.0);
    let blind_active = visibility.blind > f32::EPSILON;

    if let Some(ns) = &noteskin_assets.noteskin[player_idx] {
        let mine_ns = noteskin_assets.mine_noteskin[player_idx]
            .as_deref()
            .unwrap_or(ns);
        let receptor_ns = noteskin_assets.receptor_noteskin[player_idx]
            .as_deref()
            .unwrap_or(ns);
        let tap_explosion_ns = if profile.tap_explosion_noteskin_hidden() {
            None
        } else {
            noteskin_assets.tap_explosion_noteskin[player_idx]
                .as_deref()
                .or_else(|| noteskin_assets.noteskin[player_idx].as_deref())
        };
        let Some(timing) = state.timing_for_player(player_idx) else {
            return BuiltNotefield::empty(screen_center_x());
        };
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
        let scale_explosion = |logical_size: [f32; 2], effect_zoom: f32| -> [f32; 2] {
            scale_effect_size(logical_size, field_zoom, effect_zoom)
        };
        let scale_hold_explosion = |slot: &SpriteSlot, effect_zoom: f32| -> [f32; 2] {
            // Match ITG ghost arrow behavior: hold/roll explosions use actor asset size
            // (including double-res handling) instead of being normalized to arrow size.
            let logical = logical_slot_size(slot);
            scale_effect_size(logical, field_zoom, effect_zoom)
        };
        // ITG's FindFirst/FindLastDisplayedBeat search from m_fSongBeat, while
        // ArrowEffects::GetYOffset uses m_fSongBeatVisible internally.
        let current_search_beat = timing.get_beat_for_time_ns(state.current_music_time_ns());
        // The column swap for Step's hold-turn section is handled at the player bundle
        // level. Keep the actual note/receptor/ghost visuals on the normal noteskin
        // path here; applying an extra local Y turn breaks model-backed arrows and hit
        // effects.
        let note_rotation_y = 0.0_f32;
        let prefer_sprite_note_path = false;
        let flat_tap_face_rotation_y = 0.0_f32;
        let beat_push = beat_factor(current_beat);
        let mut col_offsets = [0.0_f32; MAX_COLS];
        fill_lane_col_offsets(
            &mut col_offsets,
            Some(ns.column_xs.as_slice()),
            num_cols,
            spacing_mult,
            field_zoom,
        );
        let mut invert_distances = [0.0_f32; MAX_COLS];
        compute_invert_distances(&col_offsets[..num_cols], &mut invert_distances[..num_cols]);
        let mut tornado_bounds = [TornadoBounds::default(); MAX_COLS];
        compute_tornado_bounds(&col_offsets[..num_cols], &mut tornado_bounds[..num_cols]);
        // ITG NoteField currently advances NoteDisplay resources twice per frame for
        // the master field (and once per additional field), so model/tween time in
        // NoteDisplay actors runs faster than wall-clock elapsed.
        let note_display_time_scale = state.num_players() as f32 + 1.0;
        // Precompute per-frame values used for converting beat/time to Y positions
        let display_speed_percent = timing.get_speed_multiplier_ns(current_beat, current_time_ns);
        // PARITY[ITGmania ArrowEffects::GetYOffset]: ScreenEdit's editing
        // state uses raw beat spacing instead of displayed beat/speed timing.
        let edit_beat_spacing = view.edit_beat_bars;
        let (rate, cmod_bps_opt, curr_disp_beat, beatmod_multiplier, post_accel_scale) =
            match scroll_speed {
                _ if edit_beat_spacing => {
                    let player_multiplier = scroll_speed
                        .beat_multiplier(state.scroll_reference_bpm(), state.music_rate());
                    (1.0, None, current_beat, 1.0, field_zoom * player_multiplier)
                }
                ScrollSpeedSetting::CMod(c_bpm) => {
                    let gameplay_music_rate = state.music_rate();
                    let rate = if gameplay_music_rate.is_finite() && gameplay_music_rate > 0.0 {
                        gameplay_music_rate
                    } else {
                        1.0
                    };
                    (rate, Some(c_bpm / 60.0), 0.0, 0.0, field_zoom)
                }
                ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                    let curr_disp = timing.get_displayed_beat(current_beat);
                    let player_multiplier = scroll_speed
                        .beat_multiplier(state.scroll_reference_bpm(), state.music_rate());
                    (
                        1.0,
                        None,
                        curr_disp,
                        display_speed_percent,
                        field_zoom * player_multiplier,
                    )
                }
            };
        let travel_offset_for_time_ns = |note_time_ns: SongTimeNs| -> f32 {
            let bps_chart = cmod_bps_opt.expect("cmod bps computed");
            let time_diff_real = song_time_ns_delta_seconds(note_time_ns, current_time_ns) / rate;
            time_diff_real * bps_chart * ScrollSpeedSetting::ARROW_SPACING
        };
        let raw_travel_offset_for_beat = |beat: f32| -> f32 {
            if edit_beat_spacing {
                edit_beat_scroll_travel(beat, curr_disp_beat)
            } else {
                match scroll_speed {
                    ScrollSpeedSetting::CMod(_) => {
                        travel_offset_for_time_ns(timing.get_time_for_beat_ns(beat))
                    }
                    ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                        let note_disp_beat = timing.get_displayed_beat(beat);
                        beat_scroll_travel(note_disp_beat, curr_disp_beat, beatmod_multiplier)
                    }
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
                current_search_beat,
                draw_distance_after_targets,
                state.note_count_stats(player_idx),
                |beat| {
                    apply_accel_y(
                        raw_travel_offset_for_beat(beat),
                        elapsed_screen,
                        current_beat,
                        effect_height,
                        accel,
                    ) * post_accel_scale
                },
            );
            let last_beat_to_draw = find_last_displayed_beat(
                current_search_beat,
                draw_distance_before_targets,
                display_speed_percent,
                accel.boomerang > f32::EPSILON,
                |beat| {
                    let (y, before_peak) = apply_accel_y_with_peak(
                        raw_travel_offset_for_beat(beat),
                        elapsed_screen,
                        current_beat,
                        effect_height,
                        accel,
                    );
                    (y * post_accel_scale, before_peak)
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
            ) * post_accel_scale
        };
        let (note_start, note_end) = state.note_range_for_player(player_idx);
        let tipsy_y_for_col = |local_col: usize| -> f32 {
            tipsy_y_extra(local_col, arrow_effect_time, visual.tipsy)
                + move_col_extra(&visual.move_y_cols, local_col)
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
                    arrow_effect_time,
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
                    arrow_effect_time,
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
        let actor_alpha_for_travel = |local_col: usize, travel_offset: f32| -> f32 {
            let adjusted = adjusted_travel_offset(travel_offset);
            note_actor_alpha(
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
                visual_effect_params(&visual, local_col).bumpy,
                visual.bumpy_offset,
                visual.bumpy_period,
            )
        };
        let world_z_for_adjusted_travel = |local_col: usize, travel_offset: f32| -> f32 {
            note_world_z_for_bumpy(
                travel_offset,
                visual_effect_params(&visual, local_col).bumpy,
                visual.bumpy_offset,
                visual.bumpy_period,
            )
        };
        // For dynamic values (e.g., last_held_beat while letting go), fall back to timing for that beat.
        // Direction and receptor row are per-lane: upwards lanes anchor to the normal receptor row,
        // downwards lanes anchor to the reverse row.
        let compute_lane_y_dynamic =
            |local_col: usize, beat: f32, receptor_y_lane: f32, dir: f32| -> f32 {
                let travel_offset = raw_travel_offset_for_beat(beat);
                lane_y_from_travel(local_col, receptor_y_lane, dir, travel_offset)
            };
        // Measure Lines (Zmod parity: NoteField:SetBeatBarsAlpha).
        // ScreenEdit/Practice always draws editor beat bars at 16th-note spacing.
        let show_measure_lines = view.edit_beat_bars
            || !matches!(profile.measure_lines, profile_data::MeasureLines::Off);
        // Measure Cues reuse the same playfield geometry as the white measure
        // lines, so enter this block when either feature is active.
        if show_measure_lines || profile.measure_cues {
            let edit_bar_speed = edit_bar_scroll_speed(
                scroll_speed,
                state.scroll_reference_bpm(),
                state.music_rate(),
            );
            let time_signatures = state
                .gameplay_chart(player_idx)
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
                        profile_data::MeasureLines::Off => (0.0, 0.0, 0.0, 0.0, 0.5),
                        profile_data::MeasureLines::Measure => (0.75, 0.0, 0.0, 0.0, 0.5),
                        profile_data::MeasureLines::Quarter => (0.75, 0.5, 0.0, 0.0, 0.5),
                        profile_data::MeasureLines::Eighth => (0.75, 0.5, 0.125, 0.0, 0.5),
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

            if show_measure_lines {
                if pos_any {
                    draw_group(pos_min_x, pos_max_x, pos_receptor_y, 1.0);
                }
                if neg_any {
                    draw_group(neg_min_x, neg_max_x, neg_receptor_y, -1.0);
                }
            }

            // Measure Cues: colored lines marking Scrolls (tan), BPM changes
            // (yellow), Delays (pink), and Stops (red). Drawn after the white
            // pass so the colored line takes priority, and in tan -> yellow ->
            // pink -> red order so that when events coincide on a beat the
            // higher-priority color ends up on top (Red > Pink > Yellow > Tan).
            // Iterates only the sparse timing lists, so it adds no per-beat
            // overhead beyond the existing measure-line walk.
            if profile.measure_cues {
                const CUE_SCROLL: (f32, f32, f32) = (0.824, 0.706, 0.549);
                const CUE_BPM: (f32, f32, f32) = (1.0, 1.0, 0.0);
                const CUE_DELAY: (f32, f32, f32) = (1.0, 0.45, 0.75);
                const CUE_STOP: (f32, f32, f32) = (1.0, 0.0, 0.0);

                let (bpms, stops, delays, scrolls) = state
                    .gameplay_chart(player_idx)
                    .map(|chart| {
                        (
                            chart.timing_segments.bpms.as_slice(),
                            chart.timing_segments.stops.as_slice(),
                            chart.timing_segments.delays.as_slice(),
                            chart.timing_segments.scrolls.as_slice(),
                        )
                    })
                    .unwrap_or((&[], &[], &[], &[]));

                // Thickness keys off the cue beat's position on the same 0.5-beat
                // grid the gameplay measure lines use, mirroring the editor's
                // measure-line gradation (measure thickest, quarter a step down,
                // eighth/off-grid thinnest). Alpha is held constant and readable:
                // cues mark discrete timing events, so unlike grid lines they
                // should stay clearly visible wherever they land rather than
                // fading out on finer subdivisions.
                const CUE_ALPHA: f32 = 0.7;
                let cue_style_for_beat = |beat: f32| -> (f32, f32) {
                    let units = beat / 0.5;
                    let rounded = units.round();
                    let scale = if (units - rounded).abs() <= 1e-3 {
                        match (rounded as i64).rem_euclid(8) {
                            0 => 3.0,         // measure
                            2 | 4 | 6 => 2.0, // quarter
                            _ => 1.0,         // eighth
                        }
                    } else {
                        1.0 // off the eighth grid -> finest
                    };
                    ((scale * field_zoom).max(1.0), CUE_ALPHA)
                };

                let groups = [
                    (pos_any, pos_min_x, pos_max_x, pos_receptor_y, 1.0f32),
                    (neg_any, neg_min_x, neg_max_x, neg_receptor_y, -1.0f32),
                ];
                for (active, min_x, max_x, receptor_y, dir) in groups {
                    if !active {
                        continue;
                    }
                    let center_x_offset = 0.5 * (min_x + max_x) * field_zoom;
                    let w = ((max_x - min_x) + ScrollSpeedSetting::ARROW_SPACING) * field_zoom;
                    if !w.is_finite() || w <= 0.0 {
                        continue;
                    }
                    let x_center = playfield_center_x + center_x_offset;

                    let mut push_cue = |beat: f32, color: (f32, f32, f32)| {
                        let y = compute_lane_y_dynamic(0, beat, receptor_y, dir);
                        if y.is_finite() && y >= y_min && y <= y_max {
                            let (line_thickness, alpha) = cue_style_for_beat(beat);
                            append_cue_bar(
                                &mut actors,
                                x_center,
                                y,
                                w,
                                line_thickness,
                                color,
                                alpha,
                            );
                        }
                    };

                    // Tan (lowest priority, drawn first): only beats where the
                    // scroll ratio actually changes (the initial scroll is
                    // skipped).
                    for win in scrolls.windows(2) {
                        if win[1].ratio != win[0].ratio {
                            push_cue(win[1].beat, CUE_SCROLL);
                        }
                    }
                    // Yellow: only beats where the BPM actually changes from the
                    // previous segment (the initial BPM at beat 0 is skipped).
                    for win in bpms.windows(2) {
                        if win[1].1 != win[0].1 {
                            push_cue(win[1].0, CUE_BPM);
                        }
                    }
                    for delay in delays {
                        push_cue(delay.beat, CUE_DELAY);
                    }
                    for stop in stops {
                        push_cue(stop.beat, CUE_STOP);
                    }
                }
            }
        }

        if profile.column_cues {
            let gameplay_music_rate = state.music_rate();
            let rate = if gameplay_music_rate.is_finite() && gameplay_music_rate > 0.0 {
                gameplay_music_rate
            } else {
                1.0
            };
            if let Some(cue) = active_column_cue(state.column_cues(player_idx), current_time) {
                let duration_real = cue.duration / rate;
                let elapsed_real = (current_time - cue.start_time) / rate;
                let alpha_mul = column_cue_alpha(elapsed_real, duration_real);
                if alpha_mul > 0.0 {
                    let lane_width = ScrollSpeedSetting::ARROW_SPACING * field_zoom;
                    let cue_height = column_cue_height(screen_height());
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
                                RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE,
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

        if profile.crossover_cues {
            let gameplay_music_rate = state.music_rate();
            let rate = if gameplay_music_rate.is_finite() && gameplay_music_rate > 0.0 {
                gameplay_music_rate
            } else {
                1.0
            };
            if let Some(cue) = active_column_cue(state.crossover_cues(player_idx), current_time) {
                let duration_real = cue.duration / rate;
                let elapsed_real = (current_time - cue.start_time) / rate;
                let alpha_mul = column_cue_alpha(elapsed_real, duration_real);
                if alpha_mul > 0.0 {
                    let lane_width = ScrollSpeedSetting::ARROW_SPACING * field_zoom;
                    let cue_height = crossover_cue_height(screen_height());
                    let mut countdown_text: Option<(f32, f32, i32)> = None;

                    if profile.column_countdown && duration_real >= 5.0 {
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
                                RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE,
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

        if profile.column_flash_on_miss {
            let lane_width = ScrollSpeedSetting::ARROW_SPACING * field_zoom;
            let flash_layout = column_flash_layout(
                profile.column_flash_size == profile_data::ColumnFlashSize::Compact,
            );
            let flash_height = column_flash_height(screen_height(), flash_layout);
            for (i, flash_opt) in state
                .column_flashes_for_columns(col_start, num_cols)
                .iter()
                .enumerate()
            {
                let Some(flash) = flash_opt else {
                    continue;
                };
                let alpha = column_flash_alpha(
                    flash.started_at_screen_s,
                    elapsed_screen,
                    column_flash_duration(flash.grade),
                    column_flash_dimmed(profile.column_flash_brightness),
                );
                if alpha <= 0.0 {
                    continue;
                }
                let x = playfield_center_x + ns.column_xs[i] as f32 * spacing_mult * field_zoom;
                let color = column_flash_color(flash.grade, flash.blue_fantastic, alpha);
                if column_dirs[i] < 0.0 {
                    let reverse_y = column_flash_reverse_top_y(
                        flash_layout,
                        lane_width,
                        flash_height,
                        notefield_offset_y,
                        RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE,
                    );
                    actors.push(act!(quad:
                        align(0.5, 0.0):
                        xy(x, reverse_y):
                        zoomto(lane_width, flash_height):
                        fadetop(flash_layout.fade):
                        diffuse(color[0], color[1], color[2], color[3]):
                        z(Z_COLUMN_FLASH)
                    ));
                } else {
                    actors.push(act!(quad:
                        align(0.5, 0.0):
                        xy(x, flash_layout.y_offset + notefield_offset_y):
                        zoomto(lane_width, flash_height):
                        fadebottom(flash_layout.fade):
                        diffuse(color[0], color[1], color[2], color[3]):
                        z(Z_COLUMN_FLASH)
                    ));
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
                arrow_effect_time,
                beat_push,
                visual,
                &col_offsets[..num_cols],
                &invert_distances[..num_cols],
                &tornado_bounds[..num_cols],
            );
            let bop_zoom = state.receptor_bop_zoom(col);
            let receptor_effect_zoom = arrow_effect_zoom(&visual, i, 0.0);
            if !receptor_hidden_by_song_lua
                && !profile.hide_targets
                && receptor_alpha > f32::EPSILON
            {
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
                    receptor_slot.frame_index(state.total_elapsed_in_screen(), current_beat);
                let receptor_uv =
                    receptor_slot.uv_for_frame_at(receptor_frame, state.total_elapsed_in_screen());
                let receptor_draw =
                    receptor_slot.model_draw_at(state.total_elapsed_in_screen(), current_beat);
                // ITG Sprite::SetTexture uses source-frame dimensions for draw size,
                // so receptor and overlay keep their authored ratio (e.g. 64 vs 74 in
                // dance/default) instead of being normalized to arrow height.
                let base_receptor_size =
                    scale_explosion(logical_slot_size(receptor_slot), receptor_effect_zoom);
                let receptor_size = [
                    base_receptor_size[0] * receptor_draw.zoom[0],
                    base_receptor_size[1] * receptor_draw.zoom[1],
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
            let hold_slot = if receptor_hidden_by_song_lua || !hold_explosion_enabled(profile) {
                None
            } else {
                state.active_hold(col).and_then(|active| {
                    let note = state.notes().get(active.note_index)?;
                    if !hold_explosion_active(Some(active), current_beat, note.beat) {
                        return None;
                    }
                    hold_explosion_slot_for_col(
                        tap_explosion_ns,
                        i,
                        matches!(note.note_type, NoteType::Roll),
                    )
                })
            };
            if let Some(hold_slot) = hold_slot {
                let draw = song_lua_note_model_draw(
                    hold_slot.model_draw_at(state.total_elapsed_in_screen(), current_beat),
                    note_rotation_y,
                );
                let hold_frame =
                    hold_slot.frame_index(state.total_elapsed_in_screen(), current_beat);
                let hold_uv =
                    hold_slot.uv_for_frame_at(hold_frame, state.total_elapsed_in_screen());
                let base_size = scale_hold_explosion(hold_slot, receptor_effect_zoom);
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
                    state.total_elapsed_in_screen(),
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
                && let Some((alpha, zoom)) = state.receptor_glow_visual_for_col(col)
                && let Some(glow_slot) = receptor_ns
                    .receptor_glow
                    .get(i)
                    .and_then(|slot| slot.as_ref())
            {
                let alpha = alpha * receptor_alpha;
                if alpha > f32::EPSILON {
                    let glow_frame =
                        glow_slot.frame_index(state.total_elapsed_in_screen(), current_beat);
                    let glow_uv =
                        glow_slot.uv_for_frame_at(glow_frame, state.total_elapsed_in_screen());
                    let glow_draw =
                        glow_slot.model_draw_at(state.total_elapsed_in_screen(), current_beat);
                    let base_glow_size =
                        scale_explosion(logical_slot_size(glow_slot), receptor_effect_zoom);
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
                        let offset_scale = field_zoom * receptor_effect_zoom;
                        let offset = [
                            glow_draw.pos[0] * offset_scale * cos_r
                                - glow_draw.pos[1] * offset_scale * sin_r,
                            glow_draw.pos[0] * offset_scale * sin_r
                                + glow_draw.pos[1] * offset_scale * cos_r,
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
                                z(Z_RECEPTOR_GLOW)
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
                                z(Z_RECEPTOR_GLOW)
                            ));
                        }
                    }
                }
            }
        }
        // Tap explosions (receptor noteflash / GhostArrow) are independent of
        // the "Hide Combo Explosions" UI option, which only affects combo splodes.
        for (i, active_opt) in state
            .tap_explosions_for_columns(col_start, num_cols)
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
                    arrow_effect_time,
                    beat_push,
                    visual,
                    &col_offsets[..num_cols],
                    &invert_distances[..num_cols],
                    &tornado_bounds[..num_cols],
                );
                let confusion_receptor_rot = confusion_rotation_deg(current_beat, visual, i);
                let explosion_effect_zoom = arrow_effect_zoom(&visual, i, 0.0);
                for layer in explosion.layers.iter() {
                    let anim_time = active.elapsed;
                    let slot = &layer.slot;
                    let beat_for_anim = if slot.source.is_beat_based() {
                        (state.current_beat_display() - active.start_beat).max(0.0)
                    } else {
                        state.current_beat_display()
                    };
                    let frame = slot.frame_index(anim_time, beat_for_anim);
                    let uv = slot.uv_for_frame_at(frame, state.total_elapsed_in_screen());
                    let size = scale_explosion(logical_slot_size(slot), explosion_effect_zoom);
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
        for (i, active_opt) in state
            .mine_explosions_for_columns(col_start, num_cols)
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
                arrow_effect_time,
                beat_push,
                visual,
                &col_offsets[..num_cols],
                &invert_distances[..num_cols],
                &tornado_bounds[..num_cols],
            );
            let explosion_effect_zoom = arrow_effect_zoom(&visual, i, 0.0);
            for layer in explosion.layers.iter() {
                let slot = &layer.slot;
                let explosion_visual = layer.animation.state_at(active.elapsed);
                if !explosion_visual.visible {
                    continue;
                }
                let frame = slot.frame_index(active.elapsed, current_beat);
                let uv = slot.uv_for_frame_at(frame, state.total_elapsed_in_screen());
                let size = scale_explosion(logical_slot_size(slot), explosion_effect_zoom);
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
            let note = &state.notes()[note_index];
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
                arrow_effect_time,
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
            let active_state = state
                .active_hold(note.column)
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
            let (top, bottom, draw_body_or_cap) = hold_draw_span(y_head, y_tail, screen_height())
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
                state.total_elapsed_in_screen(),
                current_beat,
                note.beat,
            );
            let hold_body_phase = ns.part_uv_phase(
                hold_body_part,
                state.total_elapsed_in_screen(),
                current_beat,
                note.beat,
            );
            let mut hold_topcap_phase = ns.part_uv_phase(
                hold_topcap_part,
                state.total_elapsed_in_screen(),
                current_beat,
                note.beat,
            );
            let mut hold_bottomcap_phase = ns.part_uv_phase(
                hold_bottomcap_part,
                state.total_elapsed_in_screen(),
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
                // ITGmania joins hold body to cap at the tail edge (with a tiny overlap),
                // not at the cap midpoint. Keep the body clipped to that join line.
                body_bottom = hold_body_bottom_for_tail_cap(body_bottom, y_tail, cap_size[1]);
            }
            // Track rendered body extents so the tail cap can attach cleanly when
            // body segments are visible.
            let mut rendered_body_top: Option<f32> = None;
            let mut rendered_body_bottom: Option<f32> = None;
            let mut body_head_row: Option<[[f32; 3]; 2]> = None;
            let mut body_tail_row: Option<[[f32; 3]; 2]> = None;
            let col_bumpy = visual_effect_params(&visual, local_col).bumpy;
            let hold_depth_test = hold_body_needs_z_buffer(&visual);
            let use_legacy_hold_sprites = visual_use_legacy_hold_sprites(
                col_bumpy,
                visual.drunk,
                visual.tornado,
                visual.beat,
                visual.pulse_outer,
            );
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
                        state.total_elapsed_in_screen()
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

                        let hold_alpha_rows = hold_alpha_needs_rows(appearance);
                        if use_legacy_hold_sprites && allow_legacy_sprites && !hold_alpha_rows {
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
                                let segment_alpha = note_actor_alpha(
                                    segment_center_travel + tipsy_y_for_col(local_col),
                                    elapsed_screen,
                                    mini,
                                    appearance,
                                );
                                let segment_glow = itg_actor_glow_alpha(note_glow(
                                    segment_center_travel + tipsy_y_for_col(local_col),
                                    elapsed_screen,
                                    mini,
                                    appearance,
                                ));
                                if segment_alpha > f32::EPSILON || segment_glow > f32::EPSILON {
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
                                    if segment_alpha > f32::EPSILON {
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
                                    }
                                    if segment_glow > f32::EPSILON {
                                        actors.push(actor_with_world_z(
                                            act!(sprite(body_slot.texture_key_handle()):
                                                align(0.5, 0.5):
                                                xy(segment_center_x, segment_center_screen):
                                                setsize(body_width, segment_size):
                                                rotationy(note_rotation_y):
                                                rotationz(0.0):
                                                customtexturerect(u0, v0, u1, v1):
                                                diffuse(1.0, 1.0, 1.0, 0.0):
                                                glow(1.0, 1.0, 1.0, segment_glow):
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
                                    let slice_alpha = note_actor_alpha(
                                        slice_center_travel + tipsy_y_for_col(local_col),
                                        elapsed_screen,
                                        mini,
                                        appearance,
                                    );
                                    let slice_glow = itg_actor_glow_alpha(note_glow(
                                        slice_center_travel + tipsy_y_for_col(local_col),
                                        elapsed_screen,
                                        mini,
                                        appearance,
                                    ));
                                    if slice_alpha <= f32::EPSILON && slice_glow <= f32::EPSILON {
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
                                        let top_alpha = note_actor_alpha(
                                            slice_top_travel + tipsy_y_for_col(local_col),
                                            elapsed_screen,
                                            mini,
                                            appearance,
                                        );
                                        let bottom_alpha = note_actor_alpha(
                                            slice_bottom_travel + tipsy_y_for_col(local_col),
                                            elapsed_screen,
                                            mini,
                                            appearance,
                                        );
                                        let top_glow = itg_actor_glow_alpha(note_glow(
                                            slice_top_travel + tipsy_y_for_col(local_col),
                                            elapsed_screen,
                                            mini,
                                            appearance,
                                        ));
                                        let bottom_glow = itg_actor_glow_alpha(note_glow(
                                            slice_bottom_travel + tipsy_y_for_col(local_col),
                                            elapsed_screen,
                                            mini,
                                            appearance,
                                        ));
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
                                        if top_alpha > f32::EPSILON || bottom_alpha > f32::EPSILON {
                                            let mesh_vertices = body_mesh_vertices
                                                .get_or_insert_with(|| Vec::with_capacity(96));
                                            mesh_vertices.extend_from_slice(&hold_strip_quad(
                                                top_row, bottom_row,
                                            ));
                                        }
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
                                        if slice_alpha > f32::EPSILON {
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
                                        }
                                        if slice_glow > f32::EPSILON {
                                            actors.push(actor_with_world_z(
                                                act!(sprite(body_slot.texture_key_handle()):
                                                    align(0.5, 0.5):
                                                    xy(slice_center[0], slice_center[1]):
                                                    setsize(body_width, slice_height):
                                                    rotationy(note_rotation_y):
                                                    rotationz(slice_rotation):
                                                    customtexturerect(u0, slice_v0, u1, slice_v1):
                                                    diffuse(1.0, 1.0, 1.0, 0.0):
                                                    glow(1.0, 1.0, 1.0, slice_glow):
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
                                actors.push(hold_strip_glow_actor(
                                    body_slot.texture_key_shared(),
                                    Arc::from(vertices),
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
                        state.total_elapsed_in_screen()
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
                        let cap_alpha = note_actor_alpha(
                            cap_center_travel + tipsy_y_for_col(local_col),
                            elapsed_screen,
                            mini,
                            appearance,
                        );
                        let cap_glow = itg_actor_glow_alpha(note_glow(
                            cap_center_travel + tipsy_y_for_col(local_col),
                            elapsed_screen,
                            mini,
                            appearance,
                        ));
                        if cap_alpha <= f32::EPSILON && cap_glow <= f32::EPSILON {
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
                            let top_alpha = note_actor_alpha(
                                cap_top_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let bottom_alpha = note_actor_alpha(
                                cap_bottom_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let top_glow = itg_actor_glow_alpha(note_glow(
                                cap_top_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            ));
                            let bottom_glow = itg_actor_glow_alpha(note_glow(
                                cap_bottom_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            ));
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
                            if top_alpha > f32::EPSILON || bottom_alpha > f32::EPSILON {
                                actors.push(hold_strip_actor(
                                    cap_slot.texture_key_shared(),
                                    Arc::new(hold_strip_quad(top_row, bottom_row)),
                                    BlendMode::Alpha,
                                    hold_depth_test,
                                    Z_HOLD_CAP as i16,
                                ));
                            }
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
                                actors.push(hold_strip_glow_actor(
                                    cap_slot.texture_key_shared(),
                                    Arc::new(hold_strip_quad(top_glow_row, bottom_glow_row)),
                                    hold_depth_test,
                                    Z_HOLD_GLOW as i16,
                                ));
                            }
                        } else {
                            let cap_world_z =
                                world_z_for_adjusted_travel(local_col, cap_center_travel);
                            let cap_rotation = cap_path_rotation
                                + top_cap_rotation_deg(lane_reverse, body_flipped);
                            if cap_alpha > f32::EPSILON {
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
                            }
                            if cap_glow > f32::EPSILON {
                                actors.push(actor_with_world_z(
                                    act!(sprite(cap_slot.texture_key_handle()):
                                        align(0.5, 0.5):
                                        xy(cap_center_xy[0], cap_center_xy[1]):
                                        setsize(cap_width, cap_draw_height):
                                        customtexturerect(u0, v0, u1, v1):
                                        diffuse(1.0, 1.0, 1.0, 0.0):
                                        glow(1.0, 1.0, 1.0, cap_glow):
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
                        state.total_elapsed_in_screen()
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
                        let cap_alpha = note_actor_alpha(
                            cap_center_travel + tipsy_y_for_col(local_col),
                            elapsed_screen,
                            mini,
                            appearance,
                        );
                        let cap_glow = itg_actor_glow_alpha(note_glow(
                            cap_center_travel + tipsy_y_for_col(local_col),
                            elapsed_screen,
                            mini,
                            appearance,
                        ));
                        if cap_alpha <= f32::EPSILON && cap_glow <= f32::EPSILON {
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
                            let top_alpha = note_actor_alpha(
                                cap_top_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let bottom_alpha = note_actor_alpha(
                                cap_bottom_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            );
                            let top_glow = itg_actor_glow_alpha(note_glow(
                                cap_top_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            ));
                            let bottom_glow = itg_actor_glow_alpha(note_glow(
                                cap_bottom_travel + tipsy_y_for_col(local_col),
                                elapsed_screen,
                                mini,
                                appearance,
                            ));
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
                            if top_alpha > f32::EPSILON || bottom_alpha > f32::EPSILON {
                                actors.push(hold_strip_actor(
                                    cap_slot.texture_key_shared(),
                                    Arc::new(hold_strip_quad(top_row, bottom_row)),
                                    BlendMode::Alpha,
                                    hold_depth_test,
                                    Z_HOLD_CAP as i16,
                                ));
                            }
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
                                actors.push(hold_strip_glow_actor(
                                    cap_slot.texture_key_shared(),
                                    Arc::new(hold_strip_quad(top_glow_row, bottom_glow_row)),
                                    hold_depth_test,
                                    Z_HOLD_GLOW as i16,
                                ));
                            }
                        } else {
                            let cap_world_z =
                                world_z_for_adjusted_travel(local_col, cap_center_travel);
                            if cap_alpha > f32::EPSILON {
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
                            }
                            if cap_glow > f32::EPSILON {
                                actors.push(actor_with_world_z(
                                    act!(sprite(cap_slot.texture_key_handle()):
                                        align(0.5, 0.5):
                                        xy(cap_center_xy[0], cap_center_xy[1]):
                                        setsize(cap_width, cap_draw_height):
                                        customtexturerect(u0, v0, u1, v1):
                                        diffuse(1.0, 1.0, 1.0, 0.0):
                                        glow(1.0, 1.0, 1.0, cap_glow):
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
                let head_alpha = actor_alpha_for_travel(local_col, head_anchor_travel);
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
                let elapsed = state.total_elapsed_in_screen();
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
                state.lane_hold_indices(col),
                state.notes(),
                visible_row_range,
                |note_index| render_hold(note_index),
            );
        }
        let extra_hold_indices = state
            .active_hold_note_indices()
            .chain(state.decaying_hold_indices().iter().copied())
            .filter(|&idx| {
                idx >= note_start
                    && idx < note_end
                    && !hold_overlaps_visible_window(idx, state.notes(), visible_row_range)
            });
        for note_index in extra_hold_indices {
            render_hold(note_index);
        }
        let elapsed = state.total_elapsed_in_screen();
        let note_display_time = elapsed * note_display_time_scale;
        let mine_fill_phase = current_beat.rem_euclid(1.0);
        let draw_hold_same_row = ns.note_display_metrics.draw_hold_head_for_taps_on_same_row;
        let draw_roll_same_row = ns.note_display_metrics.draw_roll_head_for_taps_on_same_row;
        let tap_same_row_means_hold = ns.note_display_metrics.tap_hold_roll_on_row_means_hold;
        // Visible tap and mine notes
        for col_idx in 0..num_cols {
            let col = col_start + col_idx;
            let column_note_indices = state.lane_note_row_indices(col);
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
                state.notes(),
                // ITGmania gets tap candidates from a row-keyed TrackMap via
                // GetTapNoteRangeInclusive, then NoteDisplay::IsOnScreen
                // performs the exact ArrowEffects visibility check below.
                visible_row_range,
                |note_index| {
                    let note = &state.notes()[note_index];
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
                            && state.row_hides_completed_note(player_idx, note.row_index)
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
                    let note_alpha = actor_alpha_for_travel(col_idx, raw_travel_offset);
                    let note_glow = glow_for_travel(col_idx, raw_travel_offset);
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
                                    let glow_alpha = itg_actor_glow_alpha(note_glow);
                                    if glow_alpha > f32::EPSILON {
                                        actors.push(actor_with_world_z(
                                            act!(sprite(gradient_slot.texture_key_handle()):
                                                align(0.5, 0.5):
                                                xy(column_center_x, y_pos):
                                                setsize(width, height):
                                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                                diffuse(1.0, 1.0, 1.0, 0.0):
                                                glow(1.0, 1.0, 1.0, glow_alpha):
                                                z(Z_TAP_NOTE - 2)
                                            ),
                                            note_world_z,
                                        ));
                                    }
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
                                    push_note_glow_actor(
                                        &mut actors,
                                        NoteGlowDraw {
                                            slot,
                                            draw,
                                            model_center: center,
                                            sprite_center: center,
                                            size: [width, height],
                                            uv,
                                            rotation_y: note_rotation_y,
                                            model_rotation_z: base_rotation + note_rot,
                                            sprite_rotation_z: sprite_rotation,
                                            alpha: note_glow,
                                            blend: BlendMode::Alpha,
                                            z: (Z_TAP_NOTE - 1) as i16,
                                            world_z: note_world_z,
                                            prefer_sprite: prefer_sprite_note_path,
                                        },
                                        &mut model_cache,
                                    );
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
                            push_note_glow_actor(
                                &mut actors,
                                NoteGlowDraw {
                                    slot,
                                    draw,
                                    model_center: center,
                                    sprite_center: center,
                                    size,
                                    uv,
                                    rotation_y: note_rotation_y,
                                    model_rotation_z: base_rotation + note_rot,
                                    sprite_rotation_z: sprite_rotation,
                                    alpha: note_glow,
                                    blend: BlendMode::Alpha,
                                    z: Z_TAP_NOTE as i16,
                                    world_z: note_world_z,
                                    prefer_sprite: prefer_sprite_note_path,
                                },
                                &mut model_cache,
                            );
                        }
                        return;
                    }
                    let tap_note_part = tap_part_for_note_type(note.note_type);
                    let tap_row_flags = state.tap_row_hold_roll_flags(note_index);
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
            let mods_line_y = screen_height() * 0.25 * 1.3 + notefield_offset_y;
            let mods_line_count = mods_text
                .split(", ")
                .filter(|part| !part.is_empty())
                .count()
                .max(1) as f32;
            if !mods_text.is_empty() {
                hud_actors.push(act!(text:
                    font("miso"): settext(mods_text):
                    align(0.5, 0.0): xy(playfield_center_x, mods_line_y):
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
    let show_combo = !view.hide_combo && !blind_active && !profile.hide_combo;
    if show_combo && !profile.hide_combo_explosions && !p.combo_milestones.is_empty() {
        let combo_splode_tex = assets::visual_styles::combo_100milestone_splode_texture_key();
        let combo_minisplode_tex =
            assets::visual_styles::combo_100milestone_minisplode_texture_key();
        let combo_swoosh_tex = assets::visual_styles::combo_1000milestone_swoosh_texture_key();
        let combo_splode_zoom_scale = assets::visual_styles::effect_zoom_scale(combo_splode_tex);
        let combo_minisplode_zoom_scale =
            assets::visual_styles::effect_zoom_scale(combo_minisplode_tex);
        let combo_swoosh_zoom_scale = assets::visual_styles::effect_zoom_scale(combo_swoosh_tex);
        let combo_center_x = playfield_center_x;
        let combo_center_y = zmod_layout.combo_y;
        let player_color = color::decorative_rgba(state.player_color_index());
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
                        let alpha = (0.5_f32 * (1.0_f32 - progress)).max(0.0_f32);
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
                        let zoom = (0.25 + (2.0 - 0.25) * eased)
                            * combo_zoom_mod
                            * combo_splode_zoom_scale;
                        let alpha = (0.6 * (1.0 - eased)).max(0.0);
                        let rotation = 10.0 + (0.0 - 10.0) * eased;
                        hud_actors.push(act!(sprite(combo_splode_tex):
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
                            let mini_zoom = (0.25 + (1.8 - 0.25) * mini_progress)
                                * combo_zoom_mod
                                * combo_minisplode_zoom_scale;
                            let mini_alpha = (1.0_f32 - mini_progress).max(0.0_f32);
                            let mini_rotation = 10.0 + (0.0 - 10.0) * mini_progress;
                            hud_actors.push(act!(sprite(combo_minisplode_tex):
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
                        let zoom = (0.25 + (3.0 - 0.25) * progress)
                            * combo_zoom_mod
                            * combo_swoosh_zoom_scale;
                        let alpha = (0.7_f32 * (1.0_f32 - progress)).max(0.0_f32);
                        let x_offset = 100.0 * progress * combo_zoom_mod;
                        for &direction in &[1.0_f32, -1.0_f32] {
                            let final_x = combo_center_x + x_offset * direction;
                            hud_actors.push(act!(sprite(combo_swoosh_tex):
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
    if show_combo {
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
            let final_color = zmod_resolved_combo_color(state, p, profile, player_idx);
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

    let show_error_bar_colorful = error_bar_mask.contains(profile_data::ErrorBarMask::COLORFUL);
    let show_error_bar_monochrome = error_bar_mask.contains(profile_data::ErrorBarMask::MONOCHROME);
    let show_error_bar_text = error_bar_mask.contains(profile_data::ErrorBarMask::TEXT);
    let show_error_bar_highlight = error_bar_mask.contains(profile_data::ErrorBarMask::HIGHLIGHT);
    let show_error_bar_average = error_bar_mask.contains(profile_data::ErrorBarMask::AVERAGE);
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
    let avg_error_bar_mini_scale = average_error_bar_mini_scale(mini);
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
        let mut styles = [profile_data::ErrorBarStyle::None; 4];
        let mut style_count = 0usize;
        if show_error_bar_colorful {
            styles[style_count] = profile_data::ErrorBarStyle::Colorful;
            style_count += 1;
        }
        if show_error_bar_monochrome {
            styles[style_count] = profile_data::ErrorBarStyle::Monochrome;
            style_count += 1;
        }
        if show_error_bar_highlight {
            styles[style_count] = profile_data::ErrorBarStyle::Highlight;
            style_count += 1;
        }
        if show_error_bar_average {
            styles[style_count] = profile_data::ErrorBarStyle::Average;
            style_count += 1;
        }
        let blue_fantastic_window_s = Some(player_blue_window_ms(state, player_idx) / 1000.0);

        for style in styles.into_iter().take(style_count) {
            match style {
                profile_data::ErrorBarStyle::Monochrome => {
                    let bar_h = error_bar_max_h;
                    let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                    let max_offset_s = state.timing_profile_windows_s()[max_window_ix];
                    let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                        (ERROR_BAR_WIDTH_MONOCHROME * 0.5) / max_offset_s
                    } else {
                        0.0
                    };
                    let (bounds_s, bounds_len) = error_bar_boundaries_s(
                        state.timing_profile_windows_s(),
                        blue_fantastic_window_s,
                        profile.show_fa_plus_window,
                        max_window_ix,
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
                profile_data::ErrorBarStyle::Colorful => {
                    let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                    let max_offset_s = state.timing_profile_windows_s()[max_window_ix];
                    let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                        (ERROR_BAR_WIDTH_COLORFUL * 0.5) / max_offset_s
                    } else {
                        0.0
                    };
                    let (bounds_s, bounds_len) = error_bar_boundaries_s(
                        state.timing_profile_windows_s(),
                        blue_fantastic_window_s,
                        profile.show_fa_plus_window,
                        max_window_ix,
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
                profile_data::ErrorBarStyle::Highlight => {
                    let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                    let max_offset_s = state.timing_profile_windows_s()[max_window_ix];
                    let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                        (ERROR_BAR_WIDTH_COLORFUL * 0.5) / max_offset_s
                    } else {
                        0.0
                    };
                    let (bounds_s, bounds_len) = error_bar_boundaries_s(
                        state.timing_profile_windows_s(),
                        blue_fantastic_window_s,
                        profile.show_fa_plus_window,
                        max_window_ix,
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
                profile_data::ErrorBarStyle::Average => {
                    let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
                    let max_offset_s = state.timing_profile_windows_s()[max_window_ix];
                    let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                        (ERROR_BAR_WIDTH_AVERAGE * 0.5 * avg_error_bar_mini_scale) / max_offset_s
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
                    if profile.short_average_error_bar_enabled
                        && bar_visible
                        && wscale.is_finite()
                        && wscale > 0.0
                    {
                        let tick_h =
                            (ERROR_BAR_HEIGHT_AVERAGE + 4.0 + ERROR_BAR_AVERAGE_TICK_EXTRA_H)
                                * avg_error_bar_mini_scale;

                        if profile.center_tick {
                            hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(error_bar_x, average_bar_y):
                            zoomto(ERROR_BAR_CENTER_TICK_WIDTH, tick_h):
                            diffuse(ERROR_BAR_CENTER_TICK_RGBA[0], ERROR_BAR_CENTER_TICK_RGBA[1], ERROR_BAR_CENTER_TICK_RGBA[2], ERROR_BAR_CENTER_TICK_RGBA[3]):
                            z(Z_ERROR_BAR_AVERAGE)
                            ));
                        }

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
                            // Intensity scaling, clamping and the single-sample
                            // 0.75 correction are baked into tick.offset_s when
                            // the tick is registered (see error_bar_register_tap).
                            let x = tick.offset_s * wscale;
                            if !x.is_finite() {
                                continue;
                            }
                            hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(error_bar_x + x, average_bar_y):
                            zoomto(ERROR_BAR_TICK_WIDTH * avg_error_bar_mini_scale, tick_h):
                            diffuse(ERROR_BAR_COLORFUL_TICK_RGBA[0], ERROR_BAR_COLORFUL_TICK_RGBA[1], ERROR_BAR_COLORFUL_TICK_RGBA[2], alpha):
                            z(Z_ERROR_BAR_AVERAGE)
                        ));
                        }
                    }
                }
                profile_data::ErrorBarStyle::Text => {}
                profile_data::ErrorBarStyle::None => {}
            }
        }

        if profile.long_error_bar_enabled
            && p.error_bar_long_avg_visible
            && let Some(long_tick) = p.error_bar_long_avg_tick
        {
            let max_window_ix = error_bar_trim_max_window_ix(profile.error_bar_trim);
            let max_offset_s = state.timing_profile_windows_s()[max_window_ix];
            let bar_width = if show_error_bar_average {
                ERROR_BAR_WIDTH_AVERAGE
            } else if show_error_bar_colorful {
                ERROR_BAR_WIDTH_COLORFUL
            } else {
                ERROR_BAR_WIDTH_MONOCHROME
            };
            let long_mini_scale = if show_error_bar_average {
                avg_error_bar_mini_scale
            } else {
                1.0
            };
            let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                (bar_width * 0.5 * long_mini_scale) / max_offset_s
            } else {
                0.0
            };
            let alpha = error_bar_tick_alpha(
                elapsed_screen - long_tick.started_at,
                ERROR_BAR_TICK_DUR_COLORFUL,
                profile.error_bar_multi_tick,
            );
            if alpha > 0.0 && wscale.is_finite() && wscale > 0.0 {
                let intensity =
                    profile_data::clamp_long_error_bar_intensity(profile.long_error_bar_intensity);
                let scaled_offset = if max_offset_s.is_finite() && max_offset_s > 0.0 {
                    (long_tick.offset_s * intensity).clamp(-max_offset_s, max_offset_s)
                } else {
                    long_tick.offset_s * intensity
                };
                let x = scaled_offset * wscale;
                if x.is_finite() {
                    let long_tick_y = if show_error_bar_average {
                        average_bar_y
                    } else {
                        error_bar_y
                    };
                    let long_tick_z = if show_error_bar_average {
                        Z_ERROR_BAR_AVERAGE
                    } else {
                        error_bar_line_z
                    };
                    let long_tick_h =
                        (ERROR_BAR_HEIGHT_AVERAGE + 4.0 + ERROR_BAR_LONG_AVG_TICK_EXTRA_H)
                            * long_mini_scale;
                    hud_actors.push(act!(quad:
                        align(0.5, 0.5): xy(error_bar_x + x, long_tick_y):
                        zoomto(ERROR_BAR_LONG_AVG_TICK_WIDTH, long_tick_h):
                        diffuse(ERROR_BAR_LONG_AVG_TICK_RGBA[0], ERROR_BAR_LONG_AVG_TICK_RGBA[1], ERROR_BAR_LONG_AVG_TICK_RGBA[2], alpha):
                        z(long_tick_z)
                    ));
                }
            }
        }
        if show_error_bar_text && let Some(text) = p.error_bar_text {
            let age = elapsed_screen - text.started_at;
            if (0.0..ERROR_BAR_TICK_DUR_COLORFUL).contains(&age) {
                let x = if text.early { -40.0 } else { 40.0 };
                let label = cached_error_bar_text_label(text.early, text.scaled);
                let zoom = if text.scaled {
                    error_bar_text_scalable_zoom(
                        text.offset_ms.abs(),
                        text.scale_start_ms,
                        state.timing_profile_windows_s()[0] * 1000.0,
                    )
                } else {
                    ERROR_BAR_TEXT_ZOOM
                };
                let c = if text.early {
                    if text.scaled {
                        ERROR_BAR_TEXT_10MS_FAST_RGBA
                    } else {
                        ERROR_BAR_TEXT_EARLY_RGBA
                    }
                } else {
                    if text.scaled {
                        ERROR_BAR_TEXT_10MS_SLOW_RGBA
                    } else {
                        ERROR_BAR_TEXT_LATE_RGBA
                    }
                };
                hud_actors.push(act!(text:
                    font("wendy"): settext(label):
                    align(0.5, 0.5): xy(error_bar_x + x, error_bar_y):
                    zoom(zoom): shadowlength(1.0):
                    diffuse(c[0], c[1], c[2], c[3]):
                    z(error_bar_text_z)
                ));
            }
        }
    }

    // Measure Counter / Measure Breakdown (Zmod parity)
    if profile.measure_counter != profile_data::MeasureCounter::None {
        let segs: &[StreamSegment] = state.measure_counter_segments(player_idx);
        if !segs.is_empty() {
            let lookahead: u8 = profile.measure_counter_lookahead.min(4);
            let multiplier = profile.measure_counter.multiplier();

            let beat_floor = current_beat.floor();
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
                    let text = crate_zmod_measure_counter_text(
                        beat_floor,
                        curr_measure,
                        segs,
                        seg_index_unshifted,
                        is_lookahead,
                        lookahead,
                        multiplier,
                    );
                    let Some(text_kind) = text else { continue };
                    let is_ratio = matches!(text_kind, ZmodMeasureCounterText::Ratio { .. });
                    let text = cached_zmod_measure_counter_text(text_kind);

                    let seg_unshifted = segs[seg_index_unshifted];
                    let rgba = if seg_unshifted.is_break {
                        if is_lookahead {
                            [0.4, 0.4, 0.4, 1.0]
                        } else {
                            [0.5, 0.5, 0.5, 1.0]
                        }
                    } else if is_lookahead {
                        [0.45, 0.45, 0.45, 1.0]
                    } else if is_ratio {
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
                        let text_kind = zmod_broken_run_counter_text(
                            curr_measure,
                            segs,
                            broken_index,
                            broken_end,
                        );
                        if let Some(text_kind @ ZmodMeasureCounterText::Ratio { .. }) = text_kind {
                            let text = cached_zmod_measure_counter_text(text_kind);
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
                    let cur_bps = state
                        .timing()
                        .get_bpm_for_beat(state.current_beat_display())
                        / 60.0;
                    let rate = state.music_rate();
                    if cur_bps.is_finite() && cur_bps > 0.0 && rate.is_finite() && rate > 0.0 {
                        let measure_seconds = 4.0 / (cur_bps * rate);
                        let curr_time = state.current_beat_display() / (cur_bps * rate);

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
        // Grey out the mini indicator once the player has failed the song.
        let rgba = if p.is_failing || p.life <= 0.0 {
            [0.5, 0.5, 0.5, rgba[3]]
        } else {
            rgba
        };
        let column_width = ScrollSpeedSetting::ARROW_SPACING * field_zoom;
        let mut x = match profile.mini_indicator_position {
            profile_data::MiniIndicatorPosition::Default => playfield_center_x + column_width,
            profile_data::MiniIndicatorPosition::UnderUpArrow => {
                playfield_center_x + column_width - 45.0 + zmod_layout.subtractive_scoring_addx
            }
        };
        let mut h_align = 0.5;
        let mini_indicator_zoom = zmod_mini_indicator_zoom(profile.mini_indicator_size);
        if !profile.measure_counter_left {
            h_align = 0.0;
            x -= 12.0;
        }

        hud_actors.push(act!(text:
            font(mc_font_name): settext(text):
            align(h_align, 0.5): xy(x, zmod_layout.subtractive_scoring_y):
            zoom(mini_indicator_zoom): shadowlength(1.0):
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
                    let t: f32 = elapsed / 0.1;
                    let ease_t = 1.0 - (1.0 - t).powi(2);
                    0.8 + (0.75 - 0.8) * ease_t
                } else if elapsed < 0.7 {
                    0.75
                } else {
                    let t: f32 = (elapsed - 0.7) / 0.2;
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
                let [judgment_w, judgment_h] = judgment_frame_size(judgment_texture.key.as_ref());
                hud_actors.push(act!(sprite(judgment_texture.texture_key_handle()):
                    align(0.5, 0.5): xy(judgment_x, judgment_y):
                    z(judgment_z): rotationz(rot_deg): setsize(judgment_w, judgment_h): setstate(linear_index): zoom(zoom)
                ));
                if let Some(overlay_row) = overlay_row {
                    let overlay_index = (overlay_row * columns + col_index) as u32;
                    hud_actors.push(act!(sprite(judgment_texture.texture_key_handle()):
                        align(0.5, 0.5): xy(judgment_x, judgment_y):
                        z(judgment_z): rotationz(rot_deg): setsize(judgment_w, judgment_h): setstate(overlay_index): zoom(zoom):
                        diffuse(1.0, 1.0, 1.0, SPLIT_15_10MS_OVERLAY_ALPHA)
                    ));
                }
            }
        }
    }
    let indicator_beat_push = beat_factor(current_beat);
    let mut indicator_col_offsets = [0.0_f32; MAX_COLS];
    fill_lane_col_offsets(
        &mut indicator_col_offsets,
        noteskin_assets.noteskin[player_idx]
            .as_ref()
            .map(|ns| ns.column_xs.as_slice()),
        num_cols,
        spacing_mult,
        field_zoom,
    );
    let mut indicator_invert_distances = [0.0_f32; MAX_COLS];
    compute_invert_distances(
        &indicator_col_offsets[..num_cols],
        &mut indicator_invert_distances[..num_cols],
    );
    let mut indicator_tornado_bounds = [TornadoBounds::default(); MAX_COLS];
    compute_tornado_bounds(
        &indicator_col_offsets[..num_cols],
        &mut indicator_tornado_bounds[..num_cols],
    );
    if !blind_active && let Some(texture) = held_miss_texture {
        let texture_scale = if assets::parse_texture_hints(texture.key.as_ref()).doubleres {
            0.5
        } else {
            1.0
        };
        for (i, held_miss) in state
            .held_miss_judgments_for_columns(col_start, num_cols)
            .iter()
            .enumerate()
        {
            let Some(render_info) = held_miss.as_ref() else {
                continue;
            };
            let elapsed = (elapsed_screen - render_info.started_at_screen_s).max(0.0);
            if elapsed >= HELD_MISS_TOTAL_DURATION {
                continue;
            }
            let (zoom_x, zoom_y) = held_miss_zoom(elapsed, mini);
            let zoom_x = zoom_x * texture_scale;
            let zoom_y = zoom_y * texture_scale;
            if zoom_x <= f32::EPSILON || zoom_y <= f32::EPSILON {
                continue;
            }
            let y = player_metric_y(
                screen_center_y(),
                notefield_offset_y,
                column_reverse_percent[i],
                HELD_MISS_Y_OFFSET_FROM_CENTER,
                HELD_MISS_Y_REVERSE_OFFSET_FROM_CENTER,
            );
            let x = hold_indicator_column_x(
                playfield_center_x,
                i,
                arrow_effect_time,
                indicator_beat_push,
                visual,
                &indicator_col_offsets[..num_cols],
                &indicator_invert_distances[..num_cols],
                &indicator_tornado_bounds[..num_cols],
            );
            hud_actors.push(act!(sprite(texture.texture_key_handle()):
                align(0.5, 0.5):
                xy(x, y):
                z(196):
                setstate(0):
                zoomx(zoom_x):
                zoomy(zoom_y):
                diffusealpha(1.0)
            ));
        }
    }
    for (i, hold_judgment) in state
        .hold_judgments_for_columns(col_start, num_cols)
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
            let hold_judgment_y = player_metric_y(
                screen_center_y(),
                notefield_offset_y,
                column_reverse_percent[i],
                HOLD_JUDGMENT_Y_OFFSET_FROM_CENTER,
                HOLD_JUDGMENT_Y_REVERSE_OFFSET_FROM_CENTER,
            );
            let x = hold_indicator_column_x(
                playfield_center_x,
                i,
                arrow_effect_time,
                indicator_beat_push,
                visual,
                &indicator_col_offsets[..num_cols],
                &indicator_invert_distances[..num_cols],
                &indicator_tornado_bounds[..num_cols],
            );
            hud_actors.push(act!(sprite(texture.texture_key_handle()):
                align(0.5, 0.5):
                xy(x, hold_judgment_y):
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
        TornadoBounds, Z_ERROR_BAR_AVERAGE, Z_HOLD_BODY, Z_HOLD_GLOW, Z_RECEPTOR, Z_RECEPTOR_GLOW,
        Z_TAP_NOTE, bottom_cap_uv_window, clipped_hold_body_bounds, combo_actor_zoom,
        confusion_rotation_deg, error_bar_boundaries_s, error_bar_text_scalable_zoom,
        error_bar_trim_max_window_ix, hold_body_segment_budget, hold_draw_span,
        hold_explosion_active, hold_explosion_enabled, hold_explosion_slot_for_col,
        hold_head_render_flags, hold_indicator_column_x, hold_segment_pose, hold_strip_actor,
        hold_strip_glow_actor, hold_strip_row_3d, hold_tail_cap_bounds, hud_layout_ys, hud_y,
        itg_actor_glow_alpha, judgment_actor_zoom, judgment_frame_size, let_go_head_beat,
        maybe_mirror_uv_horiz_for_reverse_flipped, move_col_extra, note_slot_base_size,
        note_world_z_for_bumpy, note_x_offset, offset_center, player_metric_y, receptor_row_center,
        scroll_receptor_y, tap_part_for_note_type, tipsy_y_extra, top_cap_rotation_deg,
    };
    use crate::assets;
    use crate::game::parsing::noteskin::{
        NUM_QUANTIZATIONS, NoteAnimPart, Quantization, Style, load_itg_skin,
    };
    use deadlib_present::actors::Actor;
    use deadlib_render::BlendMode;
    use deadsync_core::note::NoteType;
    use deadsync_core::timing::beat_to_note_row;
    use deadsync_gameplay::{
        AccelEffects, ActiveHold, SongLuaNoteHideWindowRuntime, VisualEffects, song_lua_note_hidden,
    };
    use deadsync_profile as profile_data;
    use deadsync_rules::note::{MineResult, Note};
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_rules::timing::{self, TimeSignatureSegment};
    use std::sync::Arc;

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

    fn test_note_at_dense_row(beat: f32, row_index: usize) -> Note {
        let mut note = test_note_at_beat(beat);
        note.row_index = row_index;
        note
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
    fn zero_scroll_start_stagnates_negative_lead_in_window() {
        let timing = timing::TimingData::from_segments(
            0.0,
            0.0,
            &timing::TimingSegments {
                bpms: vec![(0.0, 120.0)],
                speeds: vec![timing::SpeedSegment {
                    beat: 0.0,
                    ratio: 0.1,
                    delay: 0.0,
                    unit: timing::SpeedUnit::Beats,
                }],
                scrolls: vec![
                    timing::ScrollSegment {
                        beat: 0.0,
                        ratio: 0.0,
                    },
                    timing::ScrollSegment {
                        beat: 4.0,
                        ratio: 1.0,
                    },
                ],
                ..timing::TimingSegments::default()
            },
            &[],
        );
        let current_beat = -12.0;
        let current_time_ns = timing.get_time_for_beat_ns(current_beat);
        let speed = timing.get_speed_multiplier_ns(current_beat, current_time_ns);
        let curr_disp_beat = timing.get_displayed_beat(current_beat);
        let later_disp_beat = timing.get_displayed_beat(-6.0);

        let last = super::find_last_displayed_beat(current_beat, 120.0, speed, false, |beat| {
            let note_disp_beat = timing.get_displayed_beat(beat);
            (
                super::beat_scroll_travel(note_disp_beat, curr_disp_beat, speed),
                true,
            )
        })
        .expect("finite lead-in visible range");
        let beat_zero_travel =
            super::beat_scroll_travel(timing.get_displayed_beat(0.0), curr_disp_beat, speed);

        assert!((speed - 0.1).abs() <= 0.0001);
        assert!((curr_disp_beat - 0.0).abs() <= 0.0001);
        assert!((later_disp_beat - 0.0).abs() <= 0.0001);
        assert!(
            beat_zero_travel.abs() <= 0.001,
            "beat zero should be stagnant at the receptor during lead-in, travel={beat_zero_travel}"
        );
        assert!(
            last >= 3.99,
            "slow lead-in should include the first four beats, last={last}"
        );
    }

    #[test]
    fn brake_applies_before_scroll_multiplier_like_itg() {
        let accel = AccelEffects {
            brake: 1.0,
            ..Default::default()
        };
        let effect_height = super::field_effect_height(0.0);
        let raw_y = ScrollSpeedSetting::ARROW_SPACING;
        let scroll_speed = 2.0;
        let itg_order = super::apply_accel_y(raw_y, 0.0, 0.0, effect_height, accel) * scroll_speed;
        let pre_scaled_order =
            super::apply_accel_y(raw_y * scroll_speed, 0.0, 0.0, effect_height, accel);
        let expected_itg_order = raw_y * (raw_y / effect_height) * scroll_speed;

        assert!(itg_order < pre_scaled_order);
        assert!((itg_order - expected_itg_order).abs() <= 0.001);
    }

    #[test]
    fn beat_measure_travel_applies_mini_once_like_notes() {
        let raw = super::beat_scroll_travel(12.0, 8.0, 1.25);
        let field_zoom = 0.75;
        let player_speed = 5.0;
        let expected = (12.0 - 8.0) * ScrollSpeedSetting::ARROW_SPACING * 1.25;

        assert!((raw - expected).abs() <= 0.001);

        let note_y = raw * field_zoom * player_speed;
        let old_measure_y = raw * field_zoom * field_zoom * player_speed;

        assert!((note_y - expected * field_zoom * player_speed).abs() <= 0.001);
        assert!(
            (note_y - old_measure_y).abs() > 100.0,
            "double-applying field zoom would drift measure lines away from notes"
        );
    }

    #[test]
    fn edit_beat_travel_uses_step_editor_spacing() {
        let edit_raw = super::edit_beat_scroll_travel(44.0, 40.0);
        let displayed_raw = super::beat_scroll_travel(42.0, 40.0, 0.5);

        assert!((edit_raw - 4.0 * ScrollSpeedSetting::ARROW_SPACING).abs() <= 0.001);
        assert!((displayed_raw - ScrollSpeedSetting::ARROW_SPACING).abs() <= 0.001);
        assert!(
            (edit_raw - displayed_raw).abs() > 100.0,
            "ITG's step editor ignores displayed beat and speed segments"
        );
    }

    #[test]
    fn visible_note_window_uses_itg_rows_not_dense_rows() {
        let notes = vec![
            test_note_at_dense_row(0.0, 0),
            test_note_at_dense_row(4.0, 1),
        ];
        let note_indices = vec![0usize, 1usize];
        let mut visited = Vec::new();

        super::for_each_visible_note_index(
            &note_indices,
            &notes,
            Some((beat_to_note_row(3.5), beat_to_note_row(4.5))),
            |note_index| visited.push(note_index),
        );

        assert_eq!(visited, vec![1]);
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
    fn hold_explosion_waits_for_receptor_on_early_hit() {
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

        assert!(!hold_explosion_active(Some(&active), 99.99, 100.0));
        assert!(hold_explosion_active(Some(&active), 100.0, 100.0));
    }

    #[test]
    fn hold_explosion_requires_live_hold_state() {
        let exhausted = ActiveHold {
            note_index: 7,
            start_time_ns: 100_000_000_000,
            end_time_ns: 8_000_000_000,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: true,
            life: 0.0,
            last_update_time_ns: 100_000_000_000,
        };
        let let_go = ActiveHold {
            note_index: 7,
            start_time_ns: 100_000_000_000,
            end_time_ns: 8_000_000_000,
            note_type: NoteType::Hold,
            let_go: true,
            is_pressed: true,
            life: 1.0,
            last_update_time_ns: 100_000_000_000,
        };

        assert!(!hold_explosion_active(Some(&exhausted), 100.0, 100.0));
        assert!(!hold_explosion_active(Some(&let_go), 100.0, 100.0));
        assert!(!hold_explosion_active(None, 100.0, 100.0));
    }

    #[test]
    fn hold_explosion_option_uses_holding_mask() {
        let enabled = profile_data::Profile::default();
        assert!(hold_explosion_enabled(&enabled));

        let mut disabled = profile_data::Profile::default();
        disabled
            .tap_explosion_active_mask
            .remove(profile_data::TapExplosionMask::HOLDING);

        assert!(!hold_explosion_enabled(&disabled));

        disabled
            .tap_explosion_active_mask
            .insert(profile_data::TapExplosionMask::HOLDING);
        disabled
            .tap_explosion_active_mask
            .remove(profile_data::TapExplosionMask::HELD);

        assert!(hold_explosion_enabled(&disabled));
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
        assert!(Z_RECEPTOR_GLOW < Z_HOLD_BODY);
    }

    #[test]
    fn hold_glow_draws_over_hold_body_like_itg_second_pass() {
        assert!(Z_HOLD_BODY < Z_HOLD_GLOW);
        assert!(Z_HOLD_GLOW < Z_TAP_NOTE);
    }

    #[test]
    fn average_error_bar_draws_under_receptors() {
        assert!(i32::from(Z_ERROR_BAR_AVERAGE) < Z_RECEPTOR);
        assert!(i32::from(Z_ERROR_BAR_AVERAGE) < Z_TAP_NOTE);
    }

    #[test]
    fn text_error_bar_scalable_zoom_matches_sl_fork_curve_at_default_threshold() {
        fn assert_close(actual: f32, expected: f32) {
            assert!(
                (actual - expected).abs() <= 0.0001,
                "{actual} != {expected}"
            );
        }

        let w2_ms = timing::TimingProfile::default_itg_with_fa_plus().windows_s[0] * 1000.0;
        let smaller_white_ms = timing::FA_PLUS_W010_MS;
        let w1_ms = timing::FA_PLUS_W0_MS;
        let inner_mid_ms = (smaller_white_ms + w1_ms) * 0.5;
        let fantastic_mid_ms = (w1_ms + w2_ms) * 0.5;

        assert_close(
            error_bar_text_scalable_zoom(inner_mid_ms, 10.0, w2_ms),
            0.35,
        );
        assert_close(
            error_bar_text_scalable_zoom(fantastic_mid_ms, 10.0, w2_ms),
            0.4,
        );
        assert_close(error_bar_text_scalable_zoom(w2_ms + 1.0, 10.0, w2_ms), 0.45);
    }

    #[test]
    fn song_lua_zoom_hide_window_covers_receptor_beat() {
        let windows = [SongLuaNoteHideWindowRuntime {
            column: 2,
            start_beat: 40.0,
            end_beat: 44.0,
        }];

        assert!(song_lua_note_hidden(&windows, 2, 40.0));
        assert!(song_lua_note_hidden(&windows, 2, 44.0));
        assert!(!song_lua_note_hidden(&windows, 1, 42.0));
        assert!(!song_lua_note_hidden(&windows, 2, 44.01));
    }

    #[test]
    fn reverse_column_cue_bounds_match_simply_love() {
        let lane_width = 64.0;
        let cue_height = super::column_cue_height(super::screen_height());
        let top = super::column_cue_reverse_top_y(
            lane_width,
            cue_height,
            0.0,
            super::RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE,
        );
        let bottom = top + cue_height;

        assert!((cue_height - 400.0).abs() <= 1e-6);
        assert!((top - 17.0).abs() <= 1e-6);
        assert!((bottom - 417.0).abs() <= 1e-6);
    }

    #[test]
    fn column_flash_default_layout_matches_original_simply_love() {
        let lane_width = 64.0;
        let layout = super::column_flash_layout(false);
        let height = super::column_flash_height(super::screen_height(), layout);
        let top = super::column_flash_reverse_top_y(
            layout,
            lane_width,
            height,
            0.0,
            super::RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE,
        );
        let bottom = top + height;

        assert!((layout.y_offset - 80.0).abs() <= 1e-6);
        assert!((layout.fade - 0.333).abs() <= 1e-6);
        assert!((height - 400.0).abs() <= 1e-6);
        assert!((top - 17.0).abs() <= 1e-6);
        assert!((bottom - 417.0).abs() <= 1e-6);
    }

    #[test]
    fn column_flash_compact_layout_matches_chris_reference() {
        let lane_width = 64.0;
        let layout = super::column_flash_layout(true);
        let height = super::column_flash_height(super::screen_height(), layout);
        let top = super::column_flash_reverse_top_y(
            layout,
            lane_width,
            height,
            0.0,
            super::RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE,
        );
        let bottom = top + height;

        assert!((layout.y_offset - 70.0).abs() <= 1e-6);
        assert!((layout.fade - 0.2).abs() <= 1e-6);
        assert!((height - 140.0).abs() <= 1e-6);
        assert!((top - 247.0).abs() <= 1e-6);
        assert!((bottom - 387.0).abs() <= 1e-6);
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
        assert_eq!(hold_draw_span(120.0, 120.0, 480.0), Some((120.0, 120.0)));
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
    fn note_actor_glow_clamps_like_itg_vertex_color() {
        assert_eq!(itg_actor_glow_alpha(1.3), 1.0);
        assert_eq!(itg_actor_glow_alpha(0.65), 0.65);
        assert_eq!(itg_actor_glow_alpha(f32::NAN), 0.0);
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
    fn hold_strip_glow_actor_uses_texture_mask_pass() {
        let actor = hold_strip_glow_actor(
            Arc::from("hold.png"),
            Arc::from([]),
            true,
            Z_HOLD_GLOW as i16,
        );
        assert!(matches!(
            actor,
            Actor::TexturedMesh {
                tint: [1.0, 1.0, 1.0, 0.0],
                glow: [1.0, 1.0, 1.0, 1.0],
                depth_test: true,
                ..
            }
        ));
    }

    #[test]
    fn move_and_confusion_column_mods_match_itg_scaling() {
        let mut visual = VisualEffects::default();
        visual.move_x_cols[1] = 0.5;
        visual.move_y_cols[1] = -0.25;
        visual.confusion_offset_cols[1] = std::f32::consts::FRAC_PI_2;

        assert_eq!(move_col_extra(&visual.move_x_cols, 1), 32.0);
        assert_eq!(move_col_extra(&visual.move_y_cols, 1), -16.0);
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
            + note_x_offset(
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
        assert!((center[1] - (240.0 + tipsy_y_extra(2, 1.25, visual.tipsy))).abs() <= 1e-6);
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
    fn hold_indicator_columns_use_receptor_lane_x() {
        let playfield_center_x = 123.0;
        let columns = [-96.0, -32.0, 32.0, 96.0];
        let field_zoom_80_mini = 1.0 - 0.8 * 0.5;
        let col_offsets = columns.map(|x| x * field_zoom_80_mini);
        let invert_distances = [0.0; 4];
        let tornado_bounds = [TornadoBounds::default(); 4];
        const EPS: f32 = 1e-5;

        assert!(
            (hold_indicator_column_x(
                playfield_center_x,
                0,
                0.0,
                0.0,
                VisualEffects::default(),
                &col_offsets,
                &invert_distances,
                &tornado_bounds,
            ) - (playfield_center_x - 57.6))
                .abs()
                <= EPS
        );
        assert!(
            (hold_indicator_column_x(
                playfield_center_x,
                3,
                0.0,
                0.0,
                VisualEffects::default(),
                &col_offsets,
                &invert_distances,
                &tornado_bounds,
            ) - (playfield_center_x + 57.6))
                .abs()
                <= EPS
        );

        let flipped = VisualEffects {
            flip: 1.0,
            ..VisualEffects::default()
        };
        assert!(
            (hold_indicator_column_x(
                playfield_center_x,
                0,
                0.0,
                0.0,
                flipped,
                &col_offsets,
                &invert_distances,
                &tornado_bounds,
            ) - (playfield_center_x + 57.6))
                .abs()
                <= EPS
        );
    }

    #[test]
    fn player_metric_y_applies_notefield_offset_after_reverse_mix() {
        const EPS: f32 = 1e-5;
        let center_y = 240.0;
        let offset_y = -22.0;

        let standard = player_metric_y(center_y, offset_y, 0.0, -90.0, 90.0);
        let reverse = player_metric_y(center_y, offset_y, 1.0, -90.0, 90.0);
        let split = player_metric_y(center_y, offset_y, 0.5, -90.0, 90.0);

        assert!((standard - 128.0).abs() <= EPS);
        assert!((reverse - 308.0).abs() <= EPS);
        assert!((split - 218.0).abs() <= EPS);
    }

    #[test]
    fn hud_layout_offsets_apply_independently() {
        let profile = profile_data::Profile {
            error_bar_active_mask: profile_data::ErrorBarMask::MONOCHROME,
            ..profile_data::Profile::default()
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
    fn judgment_actor_zoom_matches_itgmania_player_mini_formula_without_judgment_back() {
        // Without the Arrow Cloud JudgmentBack override, the front judgment
        // inherits the Player ActorFrame's mini scale, identical to combo:
        // min(pow(0.5, mini + tiny), 1.0).
        assert!((judgment_actor_zoom(0.0, false, 0.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(1.0, false, 0.0, 0.0) - 0.5).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.5, false, 0.0, 0.0) - 0.5_f32.sqrt()).abs() <= 1e-6);
        // Negative mini is clamped to 1.0 by the min(_, 1.0) cap so the
        // judgment never grows past its base size.
        assert!((judgment_actor_zoom(-1.0, false, 0.0, 0.0) - 1.0).abs() <= 1e-6);
        // ITGmania draws tap judgments outside PlayerNoteFieldPositioner, so
        // Hallway/Distant/Incoming/Space do not affect this actor's zoom.
        assert!((judgment_actor_zoom(0.0, false, -1.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.0, false, 1.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.0, false, -1.0, 1.0) - 1.0).abs() <= 1e-6);
        // Parity with combo_actor_zoom is the whole point of this branch.
        for &mini in &[-1.0_f32, 0.0, 0.25, 0.5, 1.0, 1.5] {
            assert!(
                (judgment_actor_zoom(mini, false, -1.0, 0.0) - combo_actor_zoom(mini)).abs()
                    <= 1e-6
            );
        }
    }

    #[test]
    fn judgment_actor_zoom_matches_arrow_cloud_judgment_back_formula() {
        assert!((judgment_actor_zoom(0.35, true, 0.0, 0.0) - 0.825).abs() <= 1e-6);
        assert!((judgment_actor_zoom(1.5, true, 0.0, 0.0) - 0.35).abs() <= 1e-6);
        assert!((judgment_actor_zoom(-1.0, true, 0.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.0, true, -1.0, 0.0) - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn judgment_frame_size_uses_logical_atlas_frame_dims() {
        let censored = "judgements/Test Censored 1x7 (doubleres).png";
        let tight_censored = "judgements/Test Censored Tight 1x7 (doubleres).png";
        let love = "judgements/Test Love 2x7 (doubleres).png";
        assets::register_texture_dims(censored, 600, 1400);
        assets::register_texture_dims(tight_censored, 600, 1050);
        assets::register_texture_dims(love, 880, 1036);

        assert_eq!(judgment_frame_size(censored), [300.0, 100.0]);
        assert_eq!(judgment_frame_size(tight_censored), [300.0, 75.0]);
        assert_eq!(judgment_frame_size(love), [220.0, 74.0]);

        let visible_art_h = 68.0 / 2.0;
        let original_drawn_art_h = visible_art_h * judgment_frame_size(censored)[1] / 100.0;
        let tight_drawn_art_h = visible_art_h * judgment_frame_size(tight_censored)[1] / 75.0;
        assert_eq!(original_drawn_art_h, tight_drawn_art_h);
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
    fn error_bar_boundaries_use_10ms_blue_fantastic_window() {
        let windows = timing::TimingProfile::default_itg_with_fa_plus().windows_s;
        let (bounds, len) = error_bar_boundaries_s(
            windows,
            Some(timing::FA_PLUS_W010_MS / 1000.0),
            true,
            error_bar_trim_max_window_ix(profile_data::ErrorBarTrim::Fantastic),
        );

        assert_eq!(len, 2);
        assert!((bounds[0] * 1000.0 - timing::FA_PLUS_W010_MS).abs() <= 0.001);
        assert!((bounds[1] - windows[0]).abs() <= 0.000001);
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
    fn hold_explosion_slot_respects_explosion_noteskin_choice() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let cel_ns =
            load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let default_ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let ddr_vivid_ns = load_itg_skin(&style, "ddr-vivid")
            .expect("dance/ddr-vivid should load from assets/noteskins");

        let base_slot = hold_explosion_slot_for_col(Some(&cel_ns), 0, false)
            .expect("cel should define a hold explosion");
        let selected_slot = hold_explosion_slot_for_col(Some(&ddr_vivid_ns), 0, false)
            .expect("ddr-vivid should define a hold explosion");

        assert_ne!(
            selected_slot.texture_key(),
            base_slot.texture_key(),
            "hold explosions should come from the selected explosion noteskin"
        );
        assert!(
            selected_slot.texture_key().contains("ddr-vivid"),
            "selected hold explosion should resolve from ddr-vivid, got '{}'",
            selected_slot.texture_key()
        );
        assert!(
            hold_explosion_slot_for_col(Some(&default_ns), 0, false).is_none(),
            "a selected noteskin with blank hold explosions must not fall back to the base noteskin"
        );
        assert!(
            hold_explosion_slot_for_col(None, 0, false).is_none(),
            "the no-explosion choice should also hide hold explosions"
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
