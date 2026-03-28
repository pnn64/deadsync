use crate::act;
use crate::engine::gfx::{BlendMode, MeshMode, TexturedMeshVertex};
use crate::engine::present::actors::{Actor, SizeSpec};
use crate::engine::present::cache::{TextCache, cached_text};
use crate::engine::present::color;
use crate::engine::present::compose::TextLayoutCache;
use crate::engine::present::font;
use crate::engine::space::*;
use crate::game::gameplay::{
    AccelEffects, AppearanceEffects, COMBO_HUNDRED_MILESTONE_DURATION,
    COMBO_THOUSAND_MILESTONE_DURATION, ComboMilestoneKind, HOLD_JUDGMENT_TOTAL_DURATION, MAX_COLS,
    RECEPTOR_Y_OFFSET_FROM_CENTER, RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE, TRANSITION_IN_DURATION,
    VisualEffects,
};
use crate::game::gameplay::{
    active_chart_attack_effects_for_player, active_hold_is_engaged,
    effective_accel_effects_for_player, effective_appearance_effects_for_player,
    effective_mini_percent_for_player, effective_perspective_effects_for_player,
    effective_scroll_effects_for_player, effective_scroll_speed_for_player,
    effective_visibility_effects_for_player, effective_visual_effects_for_player,
    receptor_glow_visual_for_col, scroll_receptor_y,
};
use crate::game::judgment::{HOLD_SCORE_HELD, JudgeGrade, Judgment, TimingWindow};
use crate::game::note::{HoldResult, NoteType};
use crate::game::parsing::noteskin::{
    ModelEffectMode, NUM_QUANTIZATIONS, NoteAnimPart, SpriteSlot,
};
use crate::game::{
    gameplay::{ActiveHold, PlayerRuntime, State},
    profile, scores,
    scroll::ScrollSpeedSetting,
};
use crate::screens::components::shared::noteskin_model::noteskin_model_actor_from_draw_cached;
use cgmath::{Deg, Matrix4, Point3, Vector3};
use rssp::streams::StreamSegment;
use std::array::from_fn;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
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
const TEXT_CACHE_LIMIT: usize = 8192;
const COMBO_PREWARM_CAP: u32 = 2048;
const MEASURE_PREWARM_CAP: i32 = 64;
const RUN_TIMER_PREWARM_CAP_S: i32 = 600;

// Visual Feedback
const SHOW_COMBO_AT: u32 = 4; // From Simply Love metrics

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
    insert_mask: u8,
    remove_mask: u8,
    holds_mask: u8,
    turn_bits: u16,
    attack_mode: u8,
    mini_percent: i16,
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
    GameplayModsTextKey {
        speed_tag,
        speed_bits,
        insert_mask: profile::normalize_insert_mask(profile.insert_active_mask)
            | chart_attack.insert_mask,
        remove_mask: profile::normalize_remove_mask(profile.remove_active_mask)
            | chart_attack.remove_mask,
        holds_mask: profile::normalize_holds_mask(profile.holds_active_mask)
            | chart_attack.holds_mask,
        turn_bits: turn_option_bits(profile.turn_option) | chart_attack.turn_bits,
        attack_mode: profile.attack_mode as u8,
        mini_percent: clamp_rounded_i16(display_mini),
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
fn apply_accel_y(
    raw_y: f32,
    elapsed: f32,
    current_beat: f32,
    effect_height: f32,
    accel: AccelEffects,
) -> f32 {
    if raw_y < 0.0 {
        return raw_y;
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
    if accel.boomerang > f32::EPSILON {
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
    y
}

#[inline(always)]
fn tipsy_y_extra(local_col: usize, elapsed: f32, visual: VisualEffects) -> f32 {
    if visual.tipsy <= f32::EPSILON {
        return 0.0;
    }
    let col = local_col as f32;
    let angle = elapsed * TIPSY_TIMER_FREQUENCY + col * TIPSY_COLUMN_FREQUENCY;
    visual.tipsy * angle.cos() * ScrollSpeedSetting::ARROW_SPACING * TIPSY_ARROW_MAGNITUDE
}

#[inline(always)]
fn beat_x_extra(y: f32, beat_factor: f32, visual: VisualEffects) -> f32 {
    if visual.beat <= f32::EPSILON {
        return 0.0;
    }
    let shift =
        beat_factor * (y / BEAT_OFFSET_HEIGHT + std::f32::consts::PI / BEAT_PI_HEIGHT).sin();
    visual.beat * shift
}

#[inline(always)]
fn drunk_x_extra(local_col: usize, y: f32, elapsed: f32, visual: VisualEffects) -> f32 {
    if visual.drunk <= f32::EPSILON {
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
    if visual.tornado <= f32::EPSILON {
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
    let hidden_end = center_line + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, -1.0, -1.25);
    let hidden_start = center_line + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, 0.0, -0.25);
    let sudden_end = center_line + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, 0.0, 0.25);
    let sudden_start = center_line + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, 1.0, 1.25);
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
        let right_mid = (cols_per_side + 1) / 2;
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
    for i in 0..num_cols {
        let start = i.saturating_sub(width);
        let end = (i + width).min(num_cols.saturating_sub(1));
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        for x in &col_offsets[start..=end] {
            min_x = min_x.min(*x);
            max_x = max_x.max(*x);
        }
        out[i] = TornadoBounds { min_x, max_x };
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
    if visual.tornado > f32::EPSILON {
        r += tornado_x_extra(local_col, y, base_x, tornado_bounds[local_col], visual);
    }
    if visual.drunk > f32::EPSILON {
        r += drunk_x_extra(local_col, y, elapsed, visual);
    }
    if visual.flip > f32::EPSILON {
        let mirrored = col_offsets[col_offsets.len().saturating_sub(1) - local_col];
        r += (mirrored - base_x) * visual.flip;
    }
    if visual.invert > f32::EPSILON {
        r += invert_distances[local_col] * visual.invert;
    }
    if visual.beat > f32::EPSILON {
        r += beat_x_extra(y, beat_factor, visual);
    }
    r
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
            + col_offsets[local_col]
            + note_x_extra(
                local_col,
                0.0,
                elapsed,
                beat_factor,
                visual,
                col_offsets,
                invert_distances,
                tornado_bounds,
            ),
        receptor_y_lane + tipsy_y_extra(local_col, elapsed, visual),
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
fn hold_strip_row(
    center: [f32; 2],
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
            pos: [center[0] + nx, center[1] + ny],
            uv: [u0, v],
            tex_matrix_scale: [1.0, 1.0],
            color,
        },
        TexturedMeshVertex {
            pos: [center[0] - nx, center[1] - ny],
            uv: [u1, v],
            tex_matrix_scale: [1.0, 1.0],
            color,
        },
    ]
}

#[inline(always)]
fn hold_strip_row_from_positions(
    left: [f32; 2],
    right: [f32; 2],
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
fn push_hold_strip_quad(
    out: &mut Vec<TexturedMeshVertex>,
    top: [TexturedMeshVertex; 2],
    bottom: [TexturedMeshVertex; 2],
) {
    out.extend_from_slice(&[top[0], top[1], bottom[1], top[0], bottom[1], bottom[0]]);
}

#[inline(always)]
fn hold_strip_actor(texture: Arc<str>, vertices: Arc<[TexturedMeshVertex]>, z: i16) -> Actor {
    Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        world_z: 0.0,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        texture,
        vertices,
        mode: MeshMode::Triangles,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        visible: true,
        blend: BlendMode::Alpha,
        z,
    }
}

#[inline(always)]
fn note_world_z(y: f32, visual: VisualEffects) -> f32 {
    if visual.bumpy <= f32::EPSILON {
        return 0.0;
    }
    visual.bumpy * BUMPY_Z_MAGNITUDE * (y / BUMPY_Z_ANGLE_DIVISOR).sin()
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

#[inline(always)]
fn calc_note_rotation_z(
    visual: VisualEffects,
    note_beat: f32,
    song_beat: f32,
    is_hold_head: bool,
) -> f32 {
    let mut r = 0.0;
    if visual.confusion > f32::EPSILON {
        let conf = song_beat.rem_euclid(2.0 * std::f32::consts::PI);
        r += conf * (-180.0 / std::f32::consts::PI) * visual.confusion;
    }
    if visual.dizzy > f32::EPSILON && !is_hold_head {
        let dizzy = (note_beat - song_beat).rem_euclid(2.0 * std::f32::consts::PI);
        r += dizzy * (180.0 / std::f32::consts::PI) * visual.dizzy;
    }
    r
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
fn mini_judgment_zoom(mini: f32) -> f32 {
    0.5_f32.powf(mini).min(1.0)
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
fn gameplay_mods_text(state: &State, player_idx: usize) -> Arc<str> {
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
        append_mod_part(&mut parts, key.cover, "Cover");

        if let Some(name) = attack_mode_name(state.player_profiles[player_idx].attack_mode) {
            parts.push(name.to_string());
        }
        append_turn_parts(&mut parts, key.turn_bits);
        push_transform_parts(&mut parts, key.insert_mask, key.remove_mask, key.holds_mask);
        append_perspective_parts(&mut parts, key.perspective_tilt, key.perspective_skew);
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
fn tap_judgment_rows(profile: &profile::Profile, judgment: &Judgment) -> (usize, Option<usize>) {
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

#[inline(always)]
fn hud_y(
    normal_y: f32,
    reverse_y: f32,
    centered_y: f32,
    reverse: bool,
    centered_percent: f32,
) -> f32 {
    let base_y = if reverse { reverse_y } else { normal_y };
    sm_scale(
        centered_percent.clamp(0.0, 1.0),
        0.0,
        1.0,
        base_y,
        centered_y,
    )
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
    let mut error_bar_mask = profile::normalize_error_bar_mask(profile.error_bar_active_mask);
    if error_bar_mask == 0 {
        error_bar_mask =
            profile::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text);
    }
    let has_error_bar = error_bar_mask != 0;
    if has_error_bar {
        if matches!(
            profile.judgment_graphic,
            crate::game::profile::JudgmentGraphic::None
        ) {
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
                let v = ((beat_div4 * -1.0) + (1.0 * multiplier)).floor() as i32;
                return Some(cached_paren_i32(v));
            }
            let len = (first.end - first.start) as i32;
            let v_unscaled = (beat_div4 * -1.0).floor() as i32 + 1 + len;
            let v = ((v_unscaled as f32) * multiplier).floor() as i32;
            return Some(cached_paren_i32(v));
        }
        if !segs[0].is_break {
            stream_index -= 1;
        }
    }

    let Some(seg) = stream_index
        .try_into()
        .ok()
        .and_then(|i: usize| segs.get(i).copied())
    else {
        return None;
    };

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
    let music_end_seconds = state.music_end_time.ceil().max(0.0) as i32;

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
            let countdown_max = max_measure_len.max(16).min(MEASURE_PREWARM_CAP);
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
fn scoring_count(p: &PlayerRuntime, grade: JudgeGrade) -> u32 {
    p.scoring_counts[crate::game::judgment::judge_grade_ix(grade)]
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
    let w1 = scoring_count(p, JudgeGrade::Fantastic);
    let w2 = scoring_count(p, JudgeGrade::Excellent);
    let w3 = scoring_count(p, JudgeGrade::Great);
    let w4 = scoring_count(p, JudgeGrade::Decent);
    let w5 = scoring_count(p, JudgeGrade::WayOff);
    let miss = scoring_count(p, JudgeGrade::Miss);

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
    let actual_dp = p.earned_grade_points.max(0);

    // Compute predictive percents for the active score type.
    let (kept_percent, lost_percent, pace_percent, white_count) = match score_type {
        profile::MiniIndicatorScoreType::Itg => {
            let (kept, lost, pace) =
                predictive_itg_percents(current_possible_dp, possible_dp, actual_dp);
            (kept, lost, pace, 0)
        }
        profile::MiniIndicatorScoreType::Ex | profile::MiniIndicatorScoreType::HardEx => {
            let score = crate::game::gameplay::display_ex_score_data(state, player_idx);
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
fn rage_frustum(l: f32, r: f32, b: f32, t: f32, zn: f32, zf: f32) -> Matrix4<f32> {
    let a = (r + l) / (r - l);
    let bb = (t + b) / (t - b);
    let c = -(zf + zn) / (zf - zn);
    let d = -(2.0 * zf * zn) / (zf - zn);
    // Match ITGmania's RageDisplay::GetFrustumMatrix (OpenGL-style frustum matrix).
    //
    // Note: cgmath::Matrix4::new takes elements in column-major order.
    Matrix4::new(
        // column 0
        2.0 * zn / (r - l),
        0.0,
        0.0,
        0.0,
        // column 1
        0.0,
        2.0 * zn / (t - b),
        0.0,
        0.0,
        // column 2
        a,
        bb,
        c,
        -1.0,
        // column 3
        0.0,
        0.0,
        d,
        0.0,
    )
}

fn notefield_view_proj(
    screen_w: f32,
    screen_h: f32,
    playfield_center_x: f32,
    center_y: f32,
    tilt: f32,
    skew: f32,
    reverse: bool,
) -> Option<Matrix4<f32>> {
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

    let eye = Point3::new(-vp_x + half_w, -vp_y + half_h, dist);
    let at = Point3::new(-vp_x + half_w, -vp_y + half_h, 0.0);
    let view = Matrix4::look_at_rh(eye, at, Vector3::unit_y());

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
    let world_to_screen = Matrix4::new(
        1.0, 0.0, 0.0, 0.0, //
        0.0, -1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        half_w, half_h, 0.0, 1.0,
    );
    let field = Matrix4::from_translation(Vector3::new(0.0, y_offset_world, 0.0))
        * Matrix4::from_translation(Vector3::new(pivot_x, pivot_y, 0.0))
        * Matrix4::from_angle_x(Deg(tilt_deg))
        * Matrix4::from_nonuniform_scale(tilt_scale, tilt_scale, 1.0)
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
    let use_active = engaged && active_state.map(|h| h.is_pressed).unwrap_or(false);
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

pub fn build(
    state: &State,
    profile: &profile::Profile,
    placement: FieldPlacement,
    play_style: profile::PlayStyle,
    center_1player_notefield: bool,
) -> (Vec<Actor>, f32) {
    let hold_judgment_texture: Option<&str> = match profile.hold_judgment_graphic {
        profile::HoldJudgmentGraphic::Love => Some("hold_judgements/Love 1x2 (doubleres).png"),
        profile::HoldJudgmentGraphic::Mute => Some("hold_judgements/mute 1x2 (doubleres).png"),
        profile::HoldJudgmentGraphic::ITG2 => Some("hold_judgements/ITG2 1x2 (doubleres).png"),
        profile::HoldJudgmentGraphic::None => None,
    };

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
        return (Vec::new(), screen_center_x());
    }
    // Use the cached field_zoom from gameplay state so visual layout and
    // scroll math share the exact same scaling as gameplay.
    let field_zoom = state.field_zoom[player_idx];
    let draw_distance_before_targets = state.draw_distance_before_targets[player_idx];
    let draw_distance_after_targets = state.draw_distance_after_targets[player_idx];
    let scroll_speed = effective_scroll_speed_for_player(state, player_idx);
    let col_start = player_idx * state.cols_per_player;
    let col_end = (col_start + state.cols_per_player)
        .min(state.num_cols)
        .min(MAX_COLS);
    let num_cols = col_end.saturating_sub(col_start);
    if num_cols == 0 {
        return (Vec::new(), screen_center_x());
    }
    let error_bar_mask = {
        let mut mask = profile::normalize_error_bar_mask(profile.error_bar_active_mask);
        if mask == 0 {
            mask = profile::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text);
        }
        mask
    };
    let measure_line_extra = match profile.measure_lines {
        crate::game::profile::MeasureLines::Off => 0,
        crate::game::profile::MeasureLines::Measure => 18,
        crate::game::profile::MeasureLines::Quarter => 30,
        crate::game::profile::MeasureLines::Eighth => 42,
    };
    let actor_cap = (num_cols * 10).max(28)
        + measure_line_extra
        + if profile.column_cues { num_cols + 4 } else { 0 }
        + if error_bar_mask != 0 { 18 } else { 0 };
    let hud_cap = 8
        + if profile.column_cues { 1 } else { 0 }
        + if !profile.hide_combo { 2 } else { 0 }
        + if (error_bar_mask & profile::ERROR_BAR_BIT_TEXT) != 0 {
            1
        } else {
            0
        };
    let mut actors = Vec::with_capacity(actor_cap);
    let mut hud_actors: Vec<Actor> = Vec::with_capacity(hud_cap);
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
    let receptor_y_normal = screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER + notefield_offset_y;
    let receptor_y_reverse =
        screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE + notefield_offset_y;
    let scroll = effective_scroll_effects_for_player(state, player_idx);
    let perspective = effective_perspective_effects_for_player(state, player_idx);
    let centered_percent = scroll.centered.clamp(0.0, 1.0);
    let receptor_y_centered = screen_center_y() + notefield_offset_y;
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
            scroll.centered,
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
    let reverse_scroll = state.reverse_scroll[player_idx];
    let hud_reverse = column_reverse_percent[0] >= 0.999_9;
    let judgment_y = hud_y(
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
    let zmod_layout = zmod_layout_ys(profile, judgment_y, combo_y_base, hud_reverse);
    let mc_font_name = zmod_small_combo_font(profile.combo_font);
    // ITGmania Player::Update: min(pow(0.5, mini + tiny), 1.0); deadsync currently supports Mini.
    let judgment_zoom_mod = mini_judgment_zoom(mini);
    let effect_height = field_effect_height(perspective.tilt);
    let receptor_alpha = (1.0 - visibility.dark).clamp(0.0, 1.0);
    let blind_active = visibility.blind > f32::EPSILON;

    if let Some(ns) = &state.noteskin[player_idx] {
        let timing = &state.timing_players[player_idx];
        let target_arrow_px = TARGET_ARROW_PIXEL_SIZE * field_zoom;
        let scale_sprite = |size: [i32; 2]| -> [f32; 2] {
            let width = size[0].max(0) as f32;
            let height = size[1].max(0) as f32;
            if height <= 0.0 || target_arrow_px <= 0.0 {
                [width, height]
            } else {
                let scale = target_arrow_px / height;
                [width * scale, target_arrow_px]
            }
        };
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
        let scale_cap = |size: [i32; 2]| -> [f32; 2] {
            let width = size[0].max(0) as f32;
            let height = size[1].max(0) as f32;
            if width <= 0.0 || target_arrow_px <= 0.0 {
                [width, height]
            } else {
                let scale = target_arrow_px / width;
                [target_arrow_px, height * scale]
            }
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
        let current_time = state.current_music_time_visible[player_idx];
        let current_beat = state.current_beat_visible[player_idx];
        let confusion_receptor_rot = if visual.confusion > f32::EPSILON {
            let beat = current_beat.rem_euclid(2.0 * std::f32::consts::PI);
            beat * (-180.0 / std::f32::consts::PI) * visual.confusion
        } else {
            0.0
        };
        let beat_push = beat_factor(current_beat);
        let mut col_offsets = [0.0_f32; MAX_COLS];
        for i in 0..num_cols {
            col_offsets[i] = ns.column_xs[i] as f32 * field_zoom;
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
                let speed_multiplier = timing
                    .get_speed_multiplier(state.current_beat_visible[player_idx], current_time);
                let player_multiplier =
                    scroll_speed.beat_multiplier(state.scroll_reference_bpm, state.music_rate);
                let final_multiplier = player_multiplier * speed_multiplier;
                (1.0, None, curr_disp, final_multiplier)
            }
        };
        let travel_offset_for_cached_note = |note_index: usize, use_hold_end: bool| -> f32 {
            match scroll_speed {
                ScrollSpeedSetting::CMod(_) => {
                    let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                    let note_time = if use_hold_end {
                        state.hold_end_time_cache[note_index]
                            .unwrap_or(state.note_time_cache[note_index])
                    } else {
                        state.note_time_cache[note_index]
                    };
                    let time_diff_real = (note_time - current_time) / rate;
                    time_diff_real * pps_chart
                }
                ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                    let note_disp = if use_hold_end {
                        state.hold_end_display_beat_cache[note_index]
                            .unwrap_or(state.note_display_beat_cache[note_index])
                    } else {
                        state.note_display_beat_cache[note_index]
                    };
                    let beat_diff_disp = note_disp - curr_disp_beat;
                    beat_diff_disp
                        * ScrollSpeedSetting::ARROW_SPACING
                        * field_zoom
                        * beatmod_multiplier
                }
            }
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
        let tipsy_y_for_col =
            |local_col: usize| -> f32 { tipsy_y_extra(local_col, elapsed_screen, visual) };
        let lane_y_from_travel =
            |local_col: usize, receptor_y_lane: f32, dir: f32, travel_offset: f32| -> f32 {
                receptor_y_lane
                    + dir * adjusted_travel_offset(travel_offset)
                    + tipsy_y_for_col(local_col)
            };
        let lane_center_x_from_travel = |local_col: usize, travel_offset: f32| -> f32 {
            playfield_center_x
                + col_offsets[local_col]
                + note_x_extra(
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
                + col_offsets[local_col]
                + note_x_extra(
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
        let world_z_for_raw_travel = |travel_offset: f32| -> f32 {
            note_world_z(adjusted_travel_offset(travel_offset), visual)
        };
        let world_z_for_adjusted_travel =
            |travel_offset: f32| -> f32 { note_world_z(travel_offset, visual) };
        // For dynamic values (e.g., last_held_beat while letting go), fall back to timing for that beat.
        // Direction and receptor row are per-lane: upwards lanes anchor to the normal receptor row,
        // downwards lanes anchor to the reverse row.
        let compute_lane_y_dynamic =
            |local_col: usize, beat: f32, receptor_y_lane: f32, dir: f32| -> f32 {
                let travel_offset = match scroll_speed {
                    ScrollSpeedSetting::CMod(_) => {
                        let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                        let note_time_chart = timing.get_time_for_beat(beat);
                        let time_diff_real = (note_time_chart - current_time) / rate;
                        time_diff_real * pps_chart
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
        // Measure Lines (Zmod parity: NoteField:SetBeatBarsAlpha)
        if !matches!(
            profile.measure_lines,
            crate::game::profile::MeasureLines::Off
        ) {
            let (alpha_measure, alpha_quarter, alpha_eighth) = match profile.measure_lines {
                crate::game::profile::MeasureLines::Off => (0.0, 0.0, 0.0),
                crate::game::profile::MeasureLines::Measure => (0.75, 0.0, 0.0),
                crate::game::profile::MeasureLines::Quarter => (0.75, 0.5, 0.0),
                crate::game::profile::MeasureLines::Eighth => (0.75, 0.5, 0.125),
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
                let x = ns.column_xs[i] as f32;
                if column_dirs[i] >= 0.0 {
                    if !pos_any {
                        pos_any = true;
                        pos_receptor_y = column_receptor_ys[i];
                        pos_min_x = x;
                        pos_max_x = x;
                    } else {
                        pos_min_x = pos_min_x.min(x);
                        pos_max_x = pos_max_x.max(x);
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

            let beat_units_start = (current_beat * 2.0).floor() as i64;
            let thickness = (2.0 * field_zoom).max(1.0);
            let y_min = -400.0;
            let y_max = screen_height() + 400.0;
            let alpha_lut = [
                alpha_measure,
                alpha_eighth,
                alpha_quarter,
                alpha_eighth,
                alpha_quarter,
                alpha_eighth,
                alpha_quarter,
                alpha_eighth,
            ];

            let mut draw_group = |min_x: f32, max_x: f32, receptor_y: f32, dir: f32| {
                let center_x_offset = 0.5 * (min_x + max_x) * field_zoom;
                let w = ((max_x - min_x) + ScrollSpeedSetting::ARROW_SPACING) * field_zoom;
                if !w.is_finite() || w <= 0.0 {
                    return;
                }

                let x_center = playfield_center_x + center_x_offset;

                // Walk backward from current beat.
                let mut u = beat_units_start;
                let mut iters = 0;
                while iters < 2000 {
                    let alpha = alpha_lut[u.rem_euclid(8) as usize];

                    let beat = (u as f32) * 0.5;
                    let y = compute_lane_y_dynamic(0, beat, receptor_y, dir);
                    if !y.is_finite() {
                        break;
                    }
                    if (dir >= 0.0 && y < y_min) || (dir < 0.0 && y > y_max) {
                        break;
                    }
                    if alpha > 0.0 {
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(x_center, y):
                            zoomto(w, thickness):
                            diffuse(1.0, 1.0, 1.0, alpha):
                            z(Z_MEASURE_LINES)
                        ));
                    }
                    u -= 1;
                    iters += 1;
                }

                // Walk forward from next half-beat to avoid duplicating the start line.
                let mut u = beat_units_start + 1;
                let mut iters = 0;
                while iters < 2000 {
                    let alpha = alpha_lut[u.rem_euclid(8) as usize];

                    let beat = (u as f32) * 0.5;
                    let y = compute_lane_y_dynamic(0, beat, receptor_y, dir);
                    if !y.is_finite() {
                        break;
                    }
                    if (dir >= 0.0 && y > y_max) || (dir < 0.0 && y < y_min) {
                        break;
                    }
                    if alpha > 0.0 {
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(x_center, y):
                            zoomto(w, thickness):
                            diffuse(1.0, 1.0, 1.0, alpha):
                            z(Z_MEASURE_LINES)
                        ));
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
            let current_time = state.current_music_time_visible[player_idx];
            if let Some(cue) = active_column_cue(&state.column_cues[player_idx], current_time) {
                let duration_real = cue.duration / rate;
                let elapsed_real = (current_time - cue.start_time) / rate;
                let alpha_mul = column_cue_alpha(elapsed_real, duration_real);
                if alpha_mul > 0.0 {
                    let lane_width = ScrollSpeedSetting::ARROW_SPACING * field_zoom;
                    let cue_height = (screen_height() - COLUMN_CUE_Y_OFFSET).max(0.0);
                    let mut countdown_text: Option<(f32, f32, i32)> = None;

                    if duration_real >= 5.0 {
                        let remaining = duration_real - elapsed_real;
                        if remaining > 0.5
                            && let Some(last_col) = cue.columns.last()
                        {
                            let local_col = last_col.column.saturating_sub(col_start);
                            if local_col < num_cols {
                                let x = playfield_center_x
                                    + ns.column_xs[local_col] as f32 * field_zoom;
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
                        let x = playfield_center_x + ns.column_xs[local_col] as f32 * field_zoom;
                        let alpha = COLUMN_CUE_BASE_ALPHA * alpha_mul;
                        let color = if col_cue.is_mine {
                            [1.0, 0.0, 0.0, alpha]
                        } else {
                            [0.3, 1.0, 1.0, alpha]
                        };
                        if column_dirs[local_col] < 0.0 {
                            let reverse_y = COLUMN_CUE_Y_OFFSET
                                + COLUMN_CUE_Y_OFFSET * 2.0
                                + RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE
                                + lane_width * 0.5
                                + notefield_offset_y;
                            actors.push(act!(quad:
                                align(0.5, 0.0):
                                xy(x, reverse_y):
                                zoomto(lane_width, cue_height):
                                fadebottom(0.333):
                                rotationz(180):
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
        for i in 0..num_cols {
            let col = col_start + i;
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
            if !profile.hide_targets && receptor_alpha > f32::EPSILON {
                let bop_timer = state.receptor_bop_timers[col];
                let bop_zoom = if bop_timer > 0.0 {
                    let t = (0.11 - bop_timer) / 0.11;
                    0.75 + (1.0 - 0.75) * t
                } else {
                    1.0
                };
                let receptor_slot = &ns.receptor_off[i];
                let receptor_frame =
                    receptor_slot.frame_index(state.total_elapsed_in_screen, current_beat);
                let receptor_uv =
                    receptor_slot.uv_for_frame_at(receptor_frame, state.total_elapsed_in_screen);
                // ITG Sprite::SetTexture uses source-frame dimensions for draw size,
                // so receptor and overlay keep their authored ratio (e.g. 64 vs 74 in
                // dance/default) instead of being normalized to arrow height.
                let receptor_size = scale_explosion(logical_slot_size(receptor_slot));
                let receptor_color = ns.receptor_pulse.color_for_beat(current_beat);
                let alpha = receptor_color[3] * receptor_alpha;
                if alpha > f32::EPSILON {
                    actors.push(act!(sprite(receptor_slot.texture_key_shared()):
                        align(0.5, 0.5):
                        xy(receptor_center[0], receptor_center[1]):
                        setsize(receptor_size[0], receptor_size[1]):
                        zoom(bop_zoom):
                        diffuse(
                            receptor_color[0],
                            receptor_color[1],
                            receptor_color[2],
                            alpha
                        ):
                        rotationz(-receptor_slot.def.rotation_deg as f32 + confusion_receptor_rot):
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
            let hold_slot = if let Some(active) = state.active_holds[col]
                .as_ref()
                .filter(|active| active_hold_is_engaged(active))
            {
                let note_type = &state.notes[active.note_index].note_type;
                let visuals = ns.hold_visuals_for_col(i, matches!(note_type, NoteType::Roll));
                if let Some(slot) = visuals.explosion.as_ref() {
                    Some(slot)
                } else if let Some(slot) = ns.hold.explosion.as_ref() {
                    Some(slot)
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(hold_slot) = hold_slot {
                let draw = hold_slot.model_draw_at(state.total_elapsed_in_screen, current_beat);
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
                let receptor_rotation = ns
                    .receptor_off
                    .get(i)
                    .map(|slot| slot.def.rotation_deg as f32)
                    .unwrap_or(0.0);
                let base_rotation = hold_slot.def.rotation_deg as f32;
                let final_rotation =
                    base_rotation + receptor_rotation - draw.rot[2] - confusion_receptor_rot;
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
                    actors.push(act!(sprite(hold_slot.texture_key_shared()):
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
                        actors.push(act!(sprite(hold_slot.texture_key_shared()):
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
                    actors.push(act!(sprite(hold_slot.texture_key_shared()):
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
                        actors.push(act!(sprite(hold_slot.texture_key_shared()):
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
            if !profile.hide_targets && receptor_alpha > f32::EPSILON {
                if let Some((alpha, zoom)) = receptor_glow_visual_for_col(state, col)
                    && let Some(glow_slot) = ns.receptor_glow.get(i).and_then(|slot| slot.as_ref())
                {
                    let alpha = alpha * receptor_alpha;
                    if alpha > f32::EPSILON {
                        let glow_frame =
                            glow_slot.frame_index(state.total_elapsed_in_screen, current_beat);
                        let glow_uv =
                            glow_slot.uv_for_frame_at(glow_frame, state.total_elapsed_in_screen);
                        let glow_size = scale_explosion(logical_slot_size(glow_slot));
                        let behavior = ns.receptor_glow_behavior;
                        let width = glow_size[0] * zoom;
                        let height = glow_size[1] * zoom;
                        if behavior.blend_add {
                            actors.push(act!(sprite(glow_slot.texture_key_shared()):
                                align(0.5, 0.5):
                                xy(receptor_center[0], receptor_center[1]):
                                setsize(width, height):
                                rotationz(-glow_slot.def.rotation_deg as f32 + confusion_receptor_rot):
                                customtexturerect(glow_uv[0], glow_uv[1], glow_uv[2], glow_uv[3]):
                                diffuse(1.0, 1.0, 1.0, alpha):
                                blend(add):
                                z(Z_HOLD_GLOW)
                            ));
                        } else {
                            actors.push(act!(sprite(glow_slot.texture_key_shared()):
                                align(0.5, 0.5):
                                xy(receptor_center[0], receptor_center[1]):
                                setsize(width, height):
                                rotationz(-glow_slot.def.rotation_deg as f32 + confusion_receptor_rot):
                                customtexturerect(glow_uv[0], glow_uv[1], glow_uv[2], glow_uv[3]):
                                diffuse(1.0, 1.0, 1.0, alpha):
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
        for i in 0..num_cols {
            let col = col_start + i;
            if let Some(active) = state.tap_explosions[col].as_ref()
                && let Some(explosion) = ns.tap_explosions.get(&active.window)
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
                let anim_time = active.elapsed;
                let slot = &explosion.slot;
                let beat_for_anim = if slot.source.is_beat_based() {
                    (state.current_beat_display - active.start_beat).max(0.0)
                } else {
                    state.current_beat_display
                };
                let frame = slot.frame_index(anim_time, beat_for_anim);
                let uv = slot.uv_for_frame_at(frame, state.total_elapsed_in_screen);
                let size = scale_explosion(logical_slot_size(slot));
                let explosion_visual = explosion.animation.state_at(active.elapsed);
                if !explosion_visual.visible {
                    continue;
                }
                let rotation_deg = ns
                    .receptor_off
                    .get(i)
                    .map(|slot| slot.def.rotation_deg)
                    .unwrap_or(0);
                let glow = explosion_visual.glow;
                let glow_strength = glow[0].abs() + glow[1].abs() + glow[2].abs() + glow[3].abs();
                if explosion.animation.blend_add {
                    actors.push(act!(sprite(slot.texture_key_shared()):
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
                        rotationz(-(rotation_deg as f32) + confusion_receptor_rot):
                        blend(add):
                        z(Z_TAP_EXPLOSION)
                    ));
                    if glow_strength > f32::EPSILON {
                        actors.push(act!(sprite(slot.texture_key_shared()):
                            align(0.5, 0.5):
                            xy(receptor_center[0], receptor_center[1]):
                            setsize(size[0], size[1]):
                            zoom(explosion_visual.zoom):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            diffuse(glow[0], glow[1], glow[2], glow[3]):
                            rotationz(-(rotation_deg as f32) + confusion_receptor_rot):
                            blend(add):
                            z(Z_TAP_EXPLOSION)
                        ));
                    }
                } else {
                    actors.push(act!(sprite(slot.texture_key_shared()):
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
                        rotationz(-(rotation_deg as f32) + confusion_receptor_rot):
                        blend(normal):
                        z(Z_TAP_EXPLOSION)
                    ));
                    if glow_strength > f32::EPSILON {
                        actors.push(act!(sprite(slot.texture_key_shared()):
                            align(0.5, 0.5):
                            xy(receptor_center[0], receptor_center[1]):
                            setsize(size[0], size[1]):
                            zoom(explosion_visual.zoom):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            diffuse(glow[0], glow[1], glow[2], glow[3]):
                            rotationz(-(rotation_deg as f32) + confusion_receptor_rot):
                            blend(normal):
                            z(Z_TAP_EXPLOSION)
                        ));
                    }
                }
            }
        }
        // Mine explosions
        for i in 0..num_cols {
            let col = col_start + i;
            let Some(active) = state.mine_explosions[col].as_ref() else {
                continue;
            };
            let Some(explosion) = ns.mine_hit_explosion.as_ref() else {
                continue;
            };
            let slot = &explosion.slot;
            let explosion_visual = explosion.animation.state_at(active.elapsed);
            if !explosion_visual.visible {
                continue;
            }
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
            let frame = slot.frame_index(active.elapsed, current_beat);
            let uv = slot.uv_for_frame_at(frame, state.total_elapsed_in_screen);
            let size = scale_explosion(logical_slot_size(slot));
            let glow = explosion_visual.glow;
            let glow_strength = glow[0].abs() + glow[1].abs() + glow[2].abs() + glow[3].abs();
            if explosion.animation.blend_add {
                actors.push(act!(sprite(slot.texture_key_shared()):
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
                    actors.push(act!(sprite(slot.texture_key_shared()):
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
                actors.push(act!(sprite(slot.texture_key_shared()):
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
                    actors.push(act!(sprite(slot.texture_key_shared()):
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
        // Only consider notes that are currently in or near the lookahead window.
        let notes_len = state.notes.len();
        let (note_start, note_end) = state.note_ranges[player_idx];
        let min_visible_index = state.arrows[col_start..col_end]
            .iter()
            .filter_map(|v| v.first())
            .map(|a| a.note_index)
            .min()
            .unwrap_or(note_start);
        let max_visible_index = state.note_spawn_cursor[player_idx]
            .clamp(note_start, note_end)
            .min(notes_len);
        let extra_hold_indices = state
            .active_holds
            .iter()
            .filter_map(|a| a.as_ref().map(|h| h.note_index))
            .chain(state.decaying_hold_indices.iter().copied())
            .filter(|&idx| {
                idx >= note_start
                    && idx < note_end
                    && (idx < min_visible_index || idx >= max_visible_index)
            });

        // Render holds in the visible window, plus any active/decaying holds outside it.
        // This avoids per-frame allocations and hashing for deduping.
        for note_index in (min_visible_index..max_visible_index).chain(extra_hold_indices) {
            let note = &state.notes[note_index];
            if note.column < col_start || note.column >= col_end {
                continue;
            }
            let local_col = note.column - col_start;
            if !matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
                continue;
            }
            let Some(hold) = &note.hold else {
                continue;
            };
            if matches!(hold.result, Some(HoldResult::Held)) {
                continue;
            }

            // Prepare static/dynamic Y positions for the hold body
            // Head Y: dynamic if actively held or let go, otherwise static cache
            let mut head_beat = note.beat;
            let is_head_dynamic =
                hold.let_go_started_at.is_some() || hold.result == Some(HoldResult::LetGo);

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
                match scroll_speed {
                    ScrollSpeedSetting::CMod(_) => {
                        let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                        let note_time_chart = timing.get_time_for_beat(head_beat);
                        (note_time_chart - current_time) / rate * pps_chart
                    }
                    ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                        let note_disp_beat = timing.get_displayed_beat(head_beat);
                        (note_disp_beat - curr_disp_beat)
                            * ScrollSpeedSetting::ARROW_SPACING
                            * field_zoom
                            * beatmod_multiplier
                    }
                }
            } else {
                travel_offset_for_cached_note(note_index, false)
            };
            let tail_travel_offset = travel_offset_for_cached_note(note_index, true);
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
            let mut top = y_head.min(y_tail);
            let mut bottom = y_head.max(y_tail);
            let mut draw_body_or_cap = !(bottom < -200.0 || top > screen_height() + 200.0);
            top = top.max(-400.0);
            bottom = bottom.min(screen_height() + 400.0);
            draw_body_or_cap &= bottom > top;
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
            if let Some(cap_slot) = bottom_cap_slot {
                let cap_size = scale_cap(cap_slot.size());
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
            let mut body_head_row: Option<[[f32; 2]; 2]> = None;
            let mut body_tail_row: Option<[[f32; 2]; 2]> = None;
            let use_legacy_hold_sprites = visual.bumpy <= f32::EPSILON
                && visual.drunk <= f32::EPSILON
                && visual.tornado <= f32::EPSILON
                && visual.beat <= f32::EPSILON;
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
                    let body_width = TARGET_ARROW_PIXEL_SIZE * field_zoom;
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
                    let max_segments = 2048;
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
                        let anchor_to_top =
                            lane_reverse && note_display.top_hold_anchor_when_reverse;
                        let phase_offset = if !anchor_to_top {
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
                        } else {
                            0.0
                        };

                        let mut phase = visible_top_distance / segment_height + phase_offset;
                        let phase_end_adjusted =
                            visible_bottom_distance / segment_height + phase_offset;
                        let mut emitted = 0;

                        if use_legacy_hold_sprites {
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
                                        act!(sprite(body_slot.texture_key_shared()):
                                            align(0.5, 0.5):
                                            xy(segment_center_x, segment_center_screen):
                                            setsize(body_width, segment_size):
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
                                        world_z_for_adjusted_travel(segment_center_travel),
                                    ));
                                }

                                phase = next_phase;
                                emitted += 1;
                            }
                        } else {
                            let body_slice_step = if visual.bumpy > f32::EPSILON {
                                4.0
                            } else {
                                16.0
                            };
                            let use_body_mesh =
                                body_slot.model.is_none() && visual.bumpy <= f32::EPSILON;
                            let mut body_mesh_vertices =
                                use_body_mesh.then(|| Vec::with_capacity(96));
                            let mut prev_body_row: Option<[[f32; 2]; 2]> = None;

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
                                        world_z_for_adjusted_travel(slice_center_travel);

                                    rendered_body_top = Some(match rendered_body_top {
                                        None => slice_top,
                                        Some(v) => v.min(slice_top),
                                    });
                                    rendered_body_bottom = Some(match rendered_body_bottom {
                                        None => slice_bottom,
                                        Some(v) => v.max(slice_bottom),
                                    });

                                    if let Some(mesh_vertices) = body_mesh_vertices.as_mut() {
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
                                        let slice_forward = [
                                            slice_bottom_x - slice_top_x,
                                            slice_bottom - slice_top,
                                        ];
                                        let half_width = body_width * 0.5;
                                        let top_row = prev_body_row.unwrap_or_else(|| {
                                            let row = hold_strip_row(
                                                [slice_top_x, slice_top],
                                                slice_forward,
                                                half_width,
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
                                        let bottom_row = hold_strip_row(
                                            [slice_bottom_x, slice_bottom],
                                            slice_forward,
                                            half_width,
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
                                        push_hold_strip_quad(mesh_vertices, top_row, bottom_row);
                                        body_tail_row =
                                            Some([bottom_row[0].pos, bottom_row[1].pos]);
                                        prev_body_row =
                                            Some([bottom_row[0].pos, bottom_row[1].pos]);
                                    } else {
                                        actors.push(actor_with_world_z(
                                            act!(sprite(body_slot.texture_key_shared()):
                                                align(0.5, 0.5):
                                                xy(slice_center[0], slice_center[1]):
                                                setsize(body_width, slice_height):
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
                                    Z_HOLD_BODY as i16,
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
                    let cap_size = scale_cap(cap_slot.size());
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
                        if cap_alpha <= f32::EPSILON {
                            continue;
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
                            continue;
                        }
                        let use_top_cap_mesh = !use_legacy_hold_sprites
                            && cap_slot.model.is_none()
                            && visual.bumpy <= f32::EPSILON;
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
                            let cap_forward = [cap_bottom_x - cap_top_x, cap_bottom - cap_top];
                            let half_width = cap_width * 0.5;
                            let top_row = hold_strip_row(
                                [cap_top_x, cap_top],
                                cap_forward,
                                half_width,
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
                                hold_strip_row(
                                    [cap_bottom_x, cap_bottom],
                                    cap_forward,
                                    half_width,
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
                            let mut cap_vertices = Vec::with_capacity(6);
                            push_hold_strip_quad(&mut cap_vertices, top_row, bottom_row);
                            actors.push(hold_strip_actor(
                                cap_slot.texture_key_shared(),
                                Arc::from(cap_vertices),
                                Z_HOLD_CAP as i16,
                            ));
                        } else {
                            let cap_world_z = world_z_for_adjusted_travel(cap_center_travel);
                            let cap_rotation = cap_path_rotation
                                + top_cap_rotation_deg(lane_reverse, body_flipped);
                            actors.push(actor_with_world_z(
                                act!(sprite(cap_slot.texture_key_shared()):
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
                                    rotationz(cap_rotation):
                                    z(Z_HOLD_CAP)
                                ),
                                cap_world_z,
                            ));
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
                    let cap_size = scale_cap(cap_slot.size());
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
                        continue;
                    };
                    if cap_span <= f32::EPSILON {
                        continue;
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
                        continue;
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
                        if cap_alpha <= f32::EPSILON {
                            continue;
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
                            continue;
                        }
                        let use_bottom_cap_mesh = !use_legacy_hold_sprites
                            && cap_slot.model.is_none()
                            && visual.bumpy <= f32::EPSILON
                            && !lane_reverse;
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
                            let cap_forward = [cap_bottom_x - cap_top_x, draw_bottom - draw_top];
                            let half_width = cap_width * 0.5;
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
                                hold_strip_row(
                                    [cap_top_x, draw_top],
                                    cap_forward,
                                    half_width,
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
                            let bottom_row = hold_strip_row(
                                [cap_bottom_x, draw_bottom],
                                cap_forward,
                                half_width,
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
                            let mut cap_vertices = Vec::with_capacity(6);
                            push_hold_strip_quad(&mut cap_vertices, top_row, bottom_row);
                            actors.push(hold_strip_actor(
                                cap_slot.texture_key_shared(),
                                Arc::from(cap_vertices),
                                Z_HOLD_CAP as i16,
                            ));
                        } else {
                            let cap_world_z = world_z_for_adjusted_travel(cap_center_travel);
                            actors.push(actor_with_world_z(
                                act!(sprite(cap_slot.texture_key_shared()):
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
                                    rotationz(cap_rotation):
                                    z(Z_HOLD_CAP)
                                ),
                                cap_world_z,
                            ));
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
                if head_alpha <= f32::EPSILON {
                    continue;
                }
                let hold_head_rot = calc_note_rotation_z(visual, note.beat, current_beat, true);
                let note_idx = local_col * NUM_QUANTIZATIONS + note.quantization_idx as usize;
                let head_center_x = if (head_draw_y - receptor_draw_y).abs() <= 0.5 {
                    receptor_center_x
                } else {
                    lane_center_x_from_travel(local_col, head_anchor_travel)
                };
                let head_center = [head_center_x, head_draw_y];
                let head_world_z = world_z_for_raw_travel(head_anchor_travel);
                let elapsed = state.total_elapsed_in_screen;
                let head_slot = if use_active {
                    visuals
                        .head_active
                        .as_ref()
                        .or(visuals.head_inactive.as_ref())
                } else {
                    visuals
                        .head_inactive
                        .as_ref()
                        .or(visuals.head_active.as_ref())
                };
                let hold_head_translation =
                    ns.part_uv_translation(hold_head_part, note.beat, false);
                let head_slot = head_slot.and_then(|slot| {
                    let draw = slot.model_draw_at(elapsed, current_beat);
                    if !draw.visible {
                        return None;
                    }
                    let note_scale = field_zoom;
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
                        continue;
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
                    if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
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
                    ) {
                        actors.push(actor_with_world_z(model_actor, head_world_z));
                    } else if draw.blend_add {
                        let sprite_center =
                            offset_center(head_center, local_offset, local_offset_rot_sin_cos);
                        actors.push(actor_with_world_z(
                            act!(sprite(head_slot.texture_key_shared()):
                                align(0.5, 0.5):
                                xy(sprite_center[0], sprite_center[1]):
                                setsize(size[0], size[1]):
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
                            act!(sprite(head_slot.texture_key_shared()):
                                align(0.5, 0.5):
                                xy(sprite_center[0], sprite_center[1]):
                                setsize(size[0], size[1]):
                                rotationz(draw.rot[2] - head_slot.def.rotation_deg as f32 + hold_head_rot):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                diffuse(color[0], color[1], color[2], color[3]):
                                blend(normal):
                                z(Z_TAP_NOTE)
                            ),
                            head_world_z,
                        ));
                    }
                } else if let Some(note_slots) = ns.note_layers.get(note_idx) {
                    let note_scale = field_zoom;
                    for note_slot in note_slots.iter() {
                        let draw = note_slot.model_draw_at(elapsed, current_beat);
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
                        if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
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
                        ) {
                            actors.push(actor_with_world_z(model_actor, head_world_z));
                        } else if draw.blend_add {
                            let sprite_center =
                                offset_center(head_center, local_offset, local_offset_rot_sin_cos);
                            actors.push(actor_with_world_z(
                                act!(sprite(note_slot.texture_key_shared()):
                                    align(0.5, 0.5):
                                    xy(sprite_center[0], sprite_center[1]):
                                    setsize(size[0], size[1]):
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
                                act!(sprite(note_slot.texture_key_shared()):
                                    align(0.5, 0.5):
                                    xy(sprite_center[0], sprite_center[1]):
                                    setsize(size[0], size[1]):
                                    rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32 + hold_head_rot):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(normal):
                                    z(layer_z)
                                ),
                                head_world_z,
                            ));
                        }
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
                    let size = scale_sprite(note_slot.size());
                    let draw = note_slot.model_draw_at(elapsed, current_beat);
                    if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
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
                    ) {
                        actors.push(actor_with_world_z(model_actor, head_world_z));
                    } else {
                        actors.push(actor_with_world_z(
                            act!(sprite(note_slot.texture_key_shared()):
                                align(0.5, 0.5):
                                xy(head_center[0], head_center[1]):
                                setsize(size[0], size[1]):
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
                }
            }
        }
        let elapsed = state.total_elapsed_in_screen;
        let note_display_time = elapsed * note_display_time_scale;
        let mine_fill_phase = current_beat.rem_euclid(1.0);
        let draw_hold_same_row = ns.note_display_metrics.draw_hold_head_for_taps_on_same_row;
        let draw_roll_same_row = ns.note_display_metrics.draw_roll_head_for_taps_on_same_row;
        let tap_same_row_means_hold = ns.note_display_metrics.tap_hold_roll_on_row_means_hold;
        // Active arrows
        for col_idx in 0..num_cols {
            let col = col_start + col_idx;
            let column_arrows = &state.arrows[col];
            let dir = column_dirs[col_idx];
            let receptor_y_lane = column_receptor_ys[col_idx];
            let fill_slot = ns.mines.get(col_idx).and_then(|slot| slot.as_ref());
            let fill_gradient_slot = ns
                .mine_fill_slots
                .get(col_idx)
                .and_then(|slot| slot.as_ref());
            let frame_slot = ns.mine_frames.get(col_idx).and_then(|slot| slot.as_ref());
            for arrow in column_arrows {
                let raw_travel_offset = match scroll_speed {
                    ScrollSpeedSetting::CMod(_) => {
                        let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                        // SAFETY: `state.arrows` only stores note indices sourced
                        // from `state.note_time_cache`, so `arrow.note_index` is
                        // valid for this cache lookup.
                        let note_time_chart =
                            unsafe { *state.note_time_cache.get_unchecked(arrow.note_index) };
                        (note_time_chart - current_time) / rate * pps_chart
                    }
                    ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                        // SAFETY: `state.arrows` only stores note indices sourced
                        // from `state.note_display_beat_cache`, so `arrow.note_index`
                        // is valid for this cache lookup.
                        let note_disp_beat = unsafe {
                            *state
                                .note_display_beat_cache
                                .get_unchecked(arrow.note_index)
                        };
                        (note_disp_beat - curr_disp_beat)
                            * ScrollSpeedSetting::ARROW_SPACING
                            * field_zoom
                            * beatmod_multiplier
                    }
                };
                let travel_offset = adjusted_travel_offset(raw_travel_offset);
                let y_pos = lane_y_from_travel(col_idx, receptor_y_lane, dir, raw_travel_offset);
                let delta = travel_offset;
                if delta < -draw_distance_after_targets || delta > draw_distance_before_targets {
                    continue;
                }
                if matches!(arrow.note_type, NoteType::Hold | NoteType::Roll) {
                    continue;
                }
                // SAFETY: `state.arrows` stores indices produced from `state.notes`,
                // so `arrow.note_index` is in-bounds here.
                let note = unsafe { state.notes.get_unchecked(arrow.note_index) };
                let note_alpha = alpha_for_travel(col_idx, raw_travel_offset);
                if note_alpha <= f32::EPSILON {
                    continue;
                }
                let column_center_x = lane_center_x_from_travel(col_idx, raw_travel_offset);
                let note_world_z = world_z_for_adjusted_travel(travel_offset);
                let note_rot = calc_note_rotation_z(visual, note.beat, current_beat, false);
                if matches!(arrow.note_type, NoteType::Mine) {
                    if fill_slot.is_none() && frame_slot.is_none() {
                        continue;
                    }
                    let mine_note_beat = note.beat;
                    let mine_uv_phase = ns.tap_mine_uv_phase(elapsed, current_beat, mine_note_beat);
                    let mine_translation =
                        ns.part_uv_translation(NoteAnimPart::Mine, mine_note_beat, false);
                    let circle_reference = frame_slot
                        .map(scale_mine_slot)
                        .or_else(|| fill_slot.map(scale_mine_slot))
                        .unwrap_or([
                            TARGET_ARROW_PIXEL_SIZE * field_zoom,
                            TARGET_ARROW_PIXEL_SIZE * field_zoom,
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
                                let frame = gradient_slot.frame_index_from_phase(mine_fill_phase);
                                let uv = gradient_slot.uv_for_frame_at(frame, elapsed);
                                actors.push(actor_with_world_z(
                                    act!(sprite(gradient_slot.texture_key_shared()):
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
                            let draw = slot.model_draw_at(note_display_time, current_beat);
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
                                let size = scale_mine_slot(slot);
                                let width = size[0];
                                let height = size[1];
                                let base_rotation = -slot.def.rotation_deg as f32;
                                let has_scripted_rot =
                                    matches!(slot.model_effect.mode, ModelEffectMode::Spin)
                                        || slot.model_auto_rot_total_frames > f32::EPSILON
                                        || draw.rot[2].abs() > f32::EPSILON;
                                let legacy_rot = if has_scripted_rot {
                                    0.0
                                } else {
                                    -note_display_time * 45.0
                                };
                                let sprite_rotation =
                                    base_rotation + legacy_rot + draw.rot[2] + note_rot;
                                let center = [column_center_x, y_pos];
                                if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                                    slot,
                                    draw,
                                    center,
                                    [width, height],
                                    uv,
                                    base_rotation + legacy_rot + note_rot,
                                    [1.0, 1.0, 1.0, 0.9 * note_alpha],
                                    BlendMode::Alpha,
                                    (Z_TAP_NOTE - 1) as i16,
                                    &mut model_cache,
                                ) {
                                    actors.push(actor_with_world_z(model_actor, note_world_z));
                                } else {
                                    actors.push(actor_with_world_z(
                                        act!(sprite(slot.texture_key_shared()):
                                            align(0.5, 0.5):
                                            xy(center[0], center[1]):
                                            setsize(width, height):
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
                        let draw = slot.model_draw_at(note_display_time, current_beat);
                        if !draw.visible {
                            continue;
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
                        let size = scale_mine_slot(slot);
                        let base_rotation = -slot.def.rotation_deg as f32;
                        let has_scripted_rot =
                            matches!(slot.model_effect.mode, ModelEffectMode::Spin)
                                || slot.model_auto_rot_total_frames > f32::EPSILON
                                || draw.rot[2].abs() > f32::EPSILON;
                        let legacy_rot = if has_scripted_rot {
                            0.0
                        } else {
                            note_display_time * 120.0
                        };
                        let sprite_rotation = base_rotation + legacy_rot + draw.rot[2] + note_rot;
                        let center = [column_center_x, y_pos];
                        if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
                            slot,
                            draw,
                            center,
                            size,
                            uv,
                            base_rotation + legacy_rot + note_rot,
                            [1.0, 1.0, 1.0, note_alpha],
                            BlendMode::Alpha,
                            Z_TAP_NOTE as i16,
                            &mut model_cache,
                        ) {
                            actors.push(actor_with_world_z(model_actor, note_world_z));
                        } else {
                            actors.push(actor_with_world_z(
                                act!(sprite(slot.texture_key_shared()):
                                    align(0.5, 0.5):
                                    xy(center[0], center[1]):
                                    setsize(size[0], size[1]):
                                    rotationz(sprite_rotation):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(1.0, 1.0, 1.0, note_alpha):
                                    z(Z_TAP_NOTE)
                                ),
                                note_world_z,
                            ));
                        }
                    }
                    continue;
                }
                let tap_note_part = tap_part_for_note_type(note.note_type);
                // SAFETY: `state.arrows` only stores note indices sourced from
                // `tap_row_hold_roll_flags`, so this cache lookup is in-bounds.
                let tap_row_flags = unsafe {
                    *state
                        .tap_row_hold_roll_flags
                        .get_unchecked(arrow.note_index)
                };
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
                        let head_phase = ns.part_uv_phase(part, elapsed, current_beat, note.beat);
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
                        let note_scale = field_zoom;
                        let note_size = note_slot_base_size(head_slot, note_scale);
                        let center = [column_center_x, y_pos];
                        let draw = head_slot.model_draw_at(elapsed, current_beat);
                        if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
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
                        ) {
                            actors.push(actor_with_world_z(model_actor, note_world_z));
                        } else {
                            actors.push(actor_with_world_z(
                                act!(sprite(head_slot.texture_key_shared()):
                                    align(0.5, 0.5):
                                    xy(center[0], center[1]):
                                    setsize(note_size[0], note_size[1]):
                                    rotationz(-head_slot.def.rotation_deg as f32 + note_rot):
                                    customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                                    diffuse(1.0, 1.0, 1.0, note_alpha):
                                    z(Z_TAP_NOTE)
                                ),
                                note_world_z,
                            ));
                        }
                        continue;
                    }
                }
                let note_idx = col_idx * NUM_QUANTIZATIONS + note.quantization_idx as usize;
                let tap_note_translation = ns.part_uv_translation(tap_note_part, note.beat, false);
                let lift_layers = if note.note_type == NoteType::Lift {
                    ns.lift_note_layers.get(note_idx)
                } else {
                    None
                };
                if let Some(note_slots) = lift_layers.or_else(|| ns.note_layers.get(note_idx)) {
                    let note_center = [column_center_x, y_pos];
                    let note_uv_phase =
                        ns.part_uv_phase(tap_note_part, elapsed, current_beat, note.beat);
                    let note_scale = field_zoom;
                    for note_slot in note_slots.iter() {
                        let draw = note_slot.model_draw_at(elapsed, current_beat);
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
                        let local_offset = [draw.pos[0] * offset_scale, draw.pos[1] * offset_scale];
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
                        if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
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
                        ) {
                            actors.push(actor_with_world_z(model_actor, note_world_z));
                        } else {
                            let sprite_center =
                                offset_center(note_center, local_offset, local_offset_rot_sin_cos);
                            if draw.blend_add {
                                actors.push(actor_with_world_z(
                                    act!(sprite(note_slot.texture_key_shared()):
                                        align(0.5, 0.5):
                                        xy(sprite_center[0], sprite_center[1]):
                                        setsize(note_size[0], note_size[1]):
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
                                    act!(sprite(note_slot.texture_key_shared()):
                                        align(0.5, 0.5):
                                        xy(sprite_center[0], sprite_center[1]):
                                        setsize(note_size[0], note_size[1]):
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
                    let note_size = scale_sprite(note_slot.size());
                    let center = [column_center_x, y_pos];
                    let draw = note_slot.model_draw_at(elapsed, current_beat);
                    if let Some(model_actor) = noteskin_model_actor_from_draw_cached(
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
                    ) {
                        actors.push(actor_with_world_z(model_actor, note_world_z));
                    } else {
                        actors.push(actor_with_world_z(
                            act!(sprite(note_slot.texture_key_shared()):
                                align(0.5, 0.5):
                                xy(center[0], center[1]):
                                setsize(note_size[0], note_size[1]):
                                rotationz(-note_slot.def.rotation_deg as f32 + note_rot):
                                customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                                diffuse(1.0, 1.0, 1.0, note_alpha):
                                z(Z_TAP_NOTE)
                            ),
                            note_world_z,
                        ));
                    }
                }
            }
        }
    }
    // Simply Love: ScreenGameplay underlay/PerPlayer/NoteField/DisplayMods.lua
    // shows the current mod string for 5s, then decelerates out over 0.5s.
    // Arrow Cloud/zmod add a CMod warning below this block for ITL no-CMod charts.
    {
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

    // Combo Milestone Explosions (100 / 1000 combo)
    if !blind_active
        && !profile.hide_combo
        && !profile.hide_combo_explosions
        && !p.combo_milestones.is_empty()
    {
        let combo_center_x = playfield_center_x;
        let combo_center_y = zmod_layout.combo_y;
        let player_color = state.player_color;
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
                        let zoom = (2.0 - progress) * judgment_zoom_mod;
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
                        let zoom = (0.25 + (2.0 - 0.25) * eased) * judgment_zoom_mod;
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
                            let mini_zoom =
                                (0.25 + (1.8 - 0.25) * mini_progress) * judgment_zoom_mod;
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
                        let zoom = (0.25 + (3.0 - 0.25) * progress) * judgment_zoom_mod;
                        let alpha = (0.7 * (1.0 - progress)).max(0.0);
                        let x_offset = 100.0 * progress * judgment_zoom_mod;
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
                    align(0.5, 0.5): xy(playfield_center_x, combo_y):
                    zoom(0.75 * judgment_zoom_mod): horizalign(center): shadowlength(1.0):
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
                    align(0.5, 0.5): xy(playfield_center_x, combo_y):
                    zoom(0.75 * judgment_zoom_mod): horizalign(center): shadowlength(1.0):
                    diffuse(final_color[0], final_color[1], final_color[2], final_color[3]):
                    z(90)
                ));
            }
        }
    }

    let show_error_bar_colorful = (error_bar_mask & profile::ERROR_BAR_BIT_COLORFUL) != 0;
    let show_error_bar_monochrome = (error_bar_mask & profile::ERROR_BAR_BIT_MONOCHROME) != 0;
    let show_error_bar_text = (error_bar_mask & profile::ERROR_BAR_BIT_TEXT) != 0;
    let show_error_bar_highlight = (error_bar_mask & profile::ERROR_BAR_BIT_HIGHLIGHT) != 0;
    let show_error_bar_average = (error_bar_mask & profile::ERROR_BAR_BIT_AVERAGE) != 0;
    let show_error_bar = error_bar_mask != 0;
    let (error_bar_y, error_bar_max_h) = if matches!(
        profile.judgment_graphic,
        crate::game::profile::JudgmentGraphic::None
    ) {
        (judgment_y, 30.0_f32)
    } else if profile.error_bar_up {
        (judgment_y - ERROR_BAR_OFFSET_FROM_JUDGMENT, 10.0_f32)
    } else {
        (judgment_y + ERROR_BAR_OFFSET_FROM_JUDGMENT, 10.0_f32)
    };
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
        if age >= 0.0 && age < OFFSET_INDICATOR_DUR_S {
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

                    let bg_alpha = if matches!(
                        profile.background_filter,
                        crate::game::profile::BackgroundFilter::Off
                    ) {
                        ERROR_BAR_MONO_BG_ALPHA
                    } else {
                        0.0
                    };
                    if bg_alpha > 0.0 {
                        hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(playfield_center_x, error_bar_y):
                            zoomto(ERROR_BAR_WIDTH_MONOCHROME + 2.0, bar_h + 2.0):
                            diffuse(0.0, 0.0, 0.0, bg_alpha):
                            z(error_bar_bg_z)
                        ));
                    }

                    hud_actors.push(act!(quad:
                        align(0.5, 0.5): xy(playfield_center_x, error_bar_y):
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
                        for i in 0..bounds_len {
                            let offset = bounds_s[i] * wscale;
                            if !offset.is_finite() {
                                continue;
                            }
                            for sx in [-1.0_f32, 1.0_f32] {
                                hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(playfield_center_x + sx * offset, error_bar_y):
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
                            align(0.5, 0.5): xy(playfield_center_x - x_off, error_bar_y):
                            zoom(0.7): diffuse(1.0, 1.0, 1.0, label_alpha):
                            z(error_bar_text_z)
                        ));
                        hud_actors.push(act!(text:
                            font("game"): settext("Late"):
                            align(0.5, 0.5): xy(playfield_center_x + x_off, error_bar_y):
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
                                align(0.5, 0.5): xy(playfield_center_x + x, error_bar_y):
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
                            age >= 0.0 && age < ERROR_BAR_TICK_DUR_COLORFUL
                        })
                        .unwrap_or(false);

                    if bar_visible && wscale.is_finite() && wscale > 0.0 {
                        hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(playfield_center_x, error_bar_y):
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
                        for i in 0..bounds_len {
                            let x = bounds_s[i] * wscale;
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
                                align(0.5, 0.5): xy(playfield_center_x + cx_early, error_bar_y):
                                zoomto(width, ERROR_BAR_HEIGHT_COLORFUL):
                                diffuse(c[0], c[1], c[2], 1.0):
                                z(error_bar_band_z)
                            ));
                            hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(playfield_center_x + cx_late, error_bar_y):
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
                            align(0.5, 0.5): xy(playfield_center_x + x, error_bar_y):
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
                            age >= 0.0 && age < ERROR_BAR_TICK_DUR_COLORFUL
                        })
                        .unwrap_or(false);

                    if bar_visible && wscale.is_finite() && wscale > 0.0 {
                        hud_actors.push(act!(quad:
                            align(0.5, 0.5): xy(playfield_center_x, error_bar_y):
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
                        for i in 0..bounds_len {
                            let x = bounds_s[i] * wscale;
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
                                align(0.5, 0.5): xy(playfield_center_x + cx_early, error_bar_y):
                                zoomto(width, ERROR_BAR_HEIGHT_COLORFUL):
                                diffuse(c[0], c[1], c[2], early_a):
                                z(error_bar_band_z)
                            ));
                            hud_actors.push(act!(quad:
                                align(0.5, 0.5): xy(playfield_center_x + cx_late, error_bar_y):
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
                            align(0.5, 0.5): xy(playfield_center_x + x, error_bar_y):
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
                            age >= 0.0 && age < ERROR_BAR_TICK_DUR_COLORFUL
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
                            align(0.5, 0.5): xy(playfield_center_x + x, average_bar_y):
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
            if age >= 0.0 && age < ERROR_BAR_TICK_DUR_COLORFUL {
                let x = if text.early { -40.0 } else { 40.0 };
                let s = if text.early { "EARLY" } else { "LATE" };
                hud_actors.push(act!(text:
                    font("wendy"): settext(s):
                    align(0.5, 0.5): xy(playfield_center_x + x, error_bar_y):
                    zoom(0.25): shadowlength(1.0):
                    diffuse(1.0, 1.0, 1.0, 1.0):
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
                            if !first.is_break {
                                let v = (curr_measure * -1.0).floor() as i32 + 1;
                                cached_paren_i32(v)
                            } else {
                                let first_len = (first.end - first.start) as i32;
                                let v = (curr_measure * -1.0).floor() as i32 + 1 + first_len;
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

    // Judgment Sprite (tap judgments)
    if !blind_active && let Some(render_info) = &p.last_judgment {
        if matches!(profile.judgment_graphic, profile::JudgmentGraphic::None) {
            // Player chose to hide tap judgment graphics.
            // Still keep life/score effects; only suppress the visual sprite.
        } else {
            let judgment = &render_info.judgment;
            let elapsed = render_info.judged_at.elapsed().as_secs_f32();
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
                let (frame_row, overlay_row) = tap_judgment_rows(profile, judgment);
                let frame_offset = if offset_sec < 0.0 { 0 } else { 1 };
                let columns = match profile.judgment_graphic {
                    profile::JudgmentGraphic::Censored => 1,
                    _ => 2,
                };
                let col_index = if columns > 1 { frame_offset } else { 0 };
                let linear_index = (frame_row * columns + col_index) as u32;
                let judgment_texture = match profile.judgment_graphic {
                    profile::JudgmentGraphic::Bebas => "judgements/Bebas 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Censored => "judgements/Censored 1x7 (doubleres).png",
                    profile::JudgmentGraphic::Chromatic => {
                        "judgements/Chromatic 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Code => "judgements/Code 2x7 (doubleres).png",
                    profile::JudgmentGraphic::ComicSans => {
                        "judgements/Comic Sans 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Emoticon => "judgements/Emoticon 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Focus => "judgements/Focus 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Grammar => "judgements/Grammar 2x7 (doubleres).png",
                    profile::JudgmentGraphic::GrooveNights => {
                        "judgements/GrooveNights 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::ITG2 => "judgements/ITG2 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Love => "judgements/Love 2x7 (doubleres).png",
                    profile::JudgmentGraphic::LoveChroma => {
                        "judgements/Love Chroma 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Miso => "judgements/Miso 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Papyrus => "judgements/Papyrus 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Rainbowmatic => {
                        "judgements/Rainbowmatic 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Roboto => "judgements/Roboto 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Shift => "judgements/Shift 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Tactics => "judgements/Tactics 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Wendy => "judgements/Wendy 2x7 (doubleres).png",
                    profile::JudgmentGraphic::WendyChroma => {
                        "judgements/Wendy Chroma 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::None => {
                        unreachable!("JudgmentGraphic::None is filtered above")
                    }
                };
                let rot_deg = if profile.judgment_tilt && judgment.grade != JudgeGrade::Miss {
                    let abs_sec = offset_sec.abs().min(0.050);
                    let dir = if offset_sec < 0.0 { 1.0 } else { -1.0 };
                    dir * abs_sec * 300.0 * profile.tilt_multiplier
                } else {
                    0.0
                };
                hud_actors.push(act!(sprite(judgment_texture):
                    align(0.5, 0.5): xy(playfield_center_x, judgment_y):
                    z(judgment_z): rotationz(rot_deg): setsize(0.0, 76.0): setstate(linear_index): zoom(zoom)
                ));
                if let Some(overlay_row) = overlay_row {
                    let overlay_index = (overlay_row * columns + col_index) as u32;
                    hud_actors.push(act!(sprite(judgment_texture):
                        align(0.5, 0.5): xy(playfield_center_x, judgment_y):
                        z(judgment_z): rotationz(rot_deg): setsize(0.0, 76.0): setstate(overlay_index): zoom(zoom):
                        diffuse(1.0, 1.0, 1.0, SPLIT_15_10MS_OVERLAY_ALPHA)
                    ));
                }
            }
        }
    }
    for i in 0..num_cols {
        let col = col_start + i;
        if blind_active {
            continue;
        }
        let Some(render_info) = state.hold_judgments[col].as_ref() else {
            continue;
        };
        let elapsed = render_info.triggered_at.elapsed().as_secs_f32();
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
                .map(|&x| x as f32)
                .unwrap_or_else(|| ((i as f32) - 1.5) * TARGET_ARROW_PIXEL_SIZE * field_zoom);
            hud_actors.push(act!(sprite(texture):
                align(0.5, 0.5):
                xy(playfield_center_x + column_offset, hold_judgment_y):
                z(195):
                setstate(frame_index):
                zoom(zoom):
                diffusealpha(1.0)
            ));
        }
    }

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
            actors = vec![Actor::Camera {
                view_proj,
                children: actors,
            }];
        }
    }

    if hud_actors.is_empty() {
        return (actors, layout_center_x);
    }
    let mut out: Vec<Actor> = Vec::with_capacity(hud_actors.len() + actors.len());
    out.extend(hud_actors);
    out.extend(actors);
    (out, layout_center_x)
}

#[cfg(test)]
mod tests {
    use super::{
        MiniIndicatorProgress, TornadoBounds, Z_HOLD_BODY, Z_HOLD_GLOW, Z_RECEPTOR,
        append_mini_part, append_perspective_parts, append_turn_parts, bottom_cap_uv_window,
        clipped_hold_body_bounds, hold_head_render_flags, hold_segment_pose, hold_tail_cap_bounds,
        hud_y, let_go_head_beat, maybe_mirror_uv_horiz_for_reverse_flipped, note_alpha,
        note_slot_base_size, note_world_z, note_x_extra, offset_center, push_transform_parts,
        receptor_row_center, tap_judgment_rows, tap_part_for_note_type, tipsy_y_extra,
        top_cap_rotation_deg, turn_option_bits, turn_option_name, zmod_subtractive_counter_state,
    };
    use crate::game::gameplay::{ActiveHold, AppearanceEffects, VisualEffects};
    use crate::game::judgment::{JudgeGrade, Judgment, TimingWindow};
    use crate::game::note::NoteType;
    use crate::game::parsing::noteskin::{
        NUM_QUANTIZATIONS, NoteAnimPart, Quantization, Style, load_itg_skin,
    };
    use crate::game::profile;

    fn fantastic_judgment(window: TimingWindow, time_error_ms: f32) -> Judgment {
        Judgment {
            time_error_ms,
            grade: JudgeGrade::Fantastic,
            window: Some(window),
            miss_because_held: false,
        }
    }

    #[test]
    fn hold_head_render_flags_keep_early_hit_inactive_before_receptor() {
        let active = ActiveHold {
            note_index: 42,
            end_time: 12.0,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: true,
            life: 1.0,
        };
        let (engaged, use_active) = hold_head_render_flags(Some(&active), 99.99, 100.0);
        assert!(!engaged);
        assert!(!use_active);
    }

    #[test]
    fn hold_head_render_flags_switch_to_active_at_receptor() {
        let mut active = ActiveHold {
            note_index: 42,
            end_time: 12.0,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: true,
            life: 1.0,
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
    fn hold_head_render_flags_require_engaged_life_state() {
        let exhausted = ActiveHold {
            note_index: 7,
            end_time: 8.0,
            note_type: NoteType::Roll,
            let_go: false,
            is_pressed: true,
            life: 0.0,
        };
        let let_go = ActiveHold {
            note_index: 7,
            end_time: 8.0,
            note_type: NoteType::Roll,
            let_go: true,
            is_pressed: true,
            life: 1.0,
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
    fn bumpy_world_z_matches_itg_default_wave() {
        let z = note_world_z(
            8.0 * std::f32::consts::PI,
            VisualEffects {
                bumpy: 1.0,
                ..VisualEffects::default()
            },
        );
        assert!((z - 40.0).abs() <= 1e-4);
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
    fn tap_judgment_rows_overlay_white_for_split_15_10_hits() {
        let profile = profile::Profile {
            show_fa_plus_window: true,
            split_15_10ms: true,
            ..profile::Profile::default()
        };
        let judgment = fantastic_judgment(TimingWindow::W0, 12.0);
        assert_eq!(tap_judgment_rows(&profile, &judgment), (0, Some(1)));
    }

    #[test]
    fn tap_judgment_rows_keep_plain_blue_when_split_is_off() {
        let profile = profile::Profile {
            show_fa_plus_window: true,
            ..profile::Profile::default()
        };
        let judgment = fantastic_judgment(TimingWindow::W0, 12.0);
        assert_eq!(tap_judgment_rows(&profile, &judgment), (0, None));
    }

    #[test]
    fn tap_judgment_rows_ignore_split_without_fa_plus_window() {
        let profile = profile::Profile {
            split_15_10ms: true,
            ..profile::Profile::default()
        };
        let judgment = fantastic_judgment(TimingWindow::W0, 12.0);
        assert_eq!(tap_judgment_rows(&profile, &judgment), (0, None));
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
        assert_eq!(tap_judgment_rows(&profile, &judgment), (1, None));
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
