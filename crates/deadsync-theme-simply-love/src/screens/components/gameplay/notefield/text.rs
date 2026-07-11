use crate::GameplayCoreState as State;
use deadlib_present::cache::{TextCache, cached_text};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    AccelEffects, PerspectiveEffects, ScrollEffects, VisualEffects,
    perspective_effects_from_profile, scroll_effects_from_flags, spacing_multiplier_for_percent,
};
use deadsync_notefield::{
    DISPLAY_TURN_BLENDER, DISPLAY_TURN_LEFT, DISPLAY_TURN_LR_MIRROR, DISPLAY_TURN_MIRROR,
    DISPLAY_TURN_RANDOM, DISPLAY_TURN_RIGHT, DISPLAY_TURN_SHUFFLE, DISPLAY_TURN_UD_MIRROR,
    GameplayModsAttackMode, GameplayModsTextParams, ZmodMeasureCounterText, ZmodMiniIndicatorText,
    clamp_rounded_i16, gameplay_mods_text as crate_gameplay_mods_text, mod_percent_key,
    quantize_centi_i32, quantize_centi_u32,
};
use deadsync_profile as profile_data;
use deadsync_rules::scroll::ScrollSpeedSetting;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;
use twox_hash::XxHash64;

use super::TEXT_CACHE_LIMIT;

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
pub(super) fn cached_percent2_f64(value: f64) -> Arc<str> {
    let key = quantize_centi_u32(value);
    cached_text(&PERCENT2_CACHE_F64, key, TEXT_CACHE_LIMIT, || {
        format!("{:.2}%", key as f64 / 100.0)
    })
}

#[inline(always)]
pub(super) fn cached_signed_percent2_f64(value: f64, neg: bool) -> Arc<str> {
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
pub(super) fn cached_neg_int_u32(value: u32) -> Arc<str> {
    cached_text(&NEG_INT_CACHE_U32, value, TEXT_CACHE_LIMIT, || {
        format!("-{value}")
    })
}

#[inline(always)]
pub(super) fn cached_paren_i32(value: i32) -> Arc<str> {
    cached_text(&PAREN_INT_CACHE_I32, value, TEXT_CACHE_LIMIT, || {
        format!("({value})")
    })
}

#[inline(always)]
pub(super) fn cached_int_i32(value: i32) -> Arc<str> {
    cached_text(&INT_CACHE_I32, value, TEXT_CACHE_LIMIT, || {
        value.to_string()
    })
}

#[inline(always)]
pub(super) fn cached_int_u32(value: u32) -> Arc<str> {
    cached_text(&INT_CACHE_U32, value, TEXT_CACHE_LIMIT, || {
        value.to_string()
    })
}

#[inline(always)]
pub(super) fn cached_ratio_i32(curr: i32, total: i32) -> Arc<str> {
    cached_text(&RATIO_CACHE_I32, (curr, total), TEXT_CACHE_LIMIT, || {
        format!("{curr}/{total}")
    })
}

#[inline(always)]
pub(super) fn cached_offset_ms(value: f32) -> Arc<str> {
    let key = quantize_centi_i32(f64::from(value));
    cached_text(&OFFSET_MS_CACHE_F32, key, TEXT_CACHE_LIMIT, || {
        format!("{:.2}ms", key as f64 / 100.0)
    })
}

#[inline(always)]
pub(super) fn cached_error_bar_text_label(early: bool, scaled: bool) -> Arc<str> {
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

pub(super) fn cached_run_timer(
    seconds: i32,
    minute_threshold: i32,
    trailing_space: bool,
) -> Arc<str> {
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

pub(super) fn cached_zmod_measure_counter_text(text: ZmodMeasureCounterText) -> Arc<str> {
    match text {
        ZmodMeasureCounterText::Break(value) => cached_paren_i32(value),
        ZmodMeasureCounterText::Ratio { current, total } => cached_ratio_i32(current, total),
        ZmodMeasureCounterText::Total(value) => cached_int_i32(value),
    }
}

pub(super) fn zmod_run_timer_fmt(
    seconds: i32,
    minute_threshold: i32,
    trailing_space: bool,
) -> Arc<str> {
    cached_run_timer(seconds, minute_threshold, trailing_space)
}

pub(super) fn cached_zmod_mini_indicator_text(text: ZmodMiniIndicatorText) -> Arc<str> {
    match text {
        ZmodMiniIndicatorText::Percent(value) => cached_percent2_f64(value),
        ZmodMiniIndicatorText::SignedPercent { value, negative } => {
            cached_signed_percent2_f64(value, negative)
        }
        ZmodMiniIndicatorText::NegativeInt(value) => cached_neg_int_u32(value),
    }
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
pub(super) fn effective_accel_effects_for_player(state: &State, player_idx: usize) -> AccelEffects {
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
pub(super) fn effective_visual_effects_for_player(
    state: &State,
    player_idx: usize,
) -> VisualEffects {
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
pub(super) fn effective_scroll_effects_for_player(
    state: &State,
    player_idx: usize,
) -> ScrollEffects {
    if player_idx >= state.num_players() || player_idx >= MAX_PLAYERS {
        return ScrollEffects::default();
    }
    state.effective_scroll_effects_for_player_with_base(
        player_idx,
        scroll_effects_from_flags(
            state.profiles()[player_idx]
                .scroll_option
                .contains(profile_data::ScrollOption::Reverse),
            state.profiles()[player_idx]
                .scroll_option
                .contains(profile_data::ScrollOption::Split),
            state.profiles()[player_idx]
                .scroll_option
                .contains(profile_data::ScrollOption::Alternate),
            state.profiles()[player_idx]
                .scroll_option
                .contains(profile_data::ScrollOption::Cross),
            state.profiles()[player_idx]
                .scroll_option
                .contains(profile_data::ScrollOption::Centered),
        ),
    )
}

#[inline(always)]
pub(super) fn effective_perspective_effects_for_player(
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
pub(super) fn effective_mini_percent_for_player(state: &State, player_idx: usize) -> f32 {
    if player_idx >= state.num_players() || player_idx >= MAX_PLAYERS {
        return 0.0;
    }
    state.effective_mini_percent_for_player_with_base(
        player_idx,
        state.profiles()[player_idx].mini_percent as f32,
    )
}

#[inline(always)]
pub(super) fn effective_spacing_multiplier_for_player(state: &State, player_idx: usize) -> f32 {
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
