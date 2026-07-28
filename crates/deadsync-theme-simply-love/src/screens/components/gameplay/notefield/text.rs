use crate::screens::gameplay::GameplayCoreState as State;
use deadlib_present::cache::{TextCache, cached_text};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    AccelEffects, AppearanceEffects, PerspectiveEffects, ScrollEffects, VisualEffects,
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

struct RecentTextCache<K> {
    entries: [Option<(K, Arc<str>)>; 2],
    next: usize,
}

impl<K> Default for RecentTextCache<K> {
    fn default() -> Self {
        Self {
            entries: [None, None],
            next: 0,
        }
    }
}

impl<K: Copy + Eq> RecentTextCache<K> {
    #[inline(always)]
    fn get_or_insert_with(&mut self, key: K, build: impl FnOnce() -> Arc<str>) -> Arc<str> {
        if let Some((_, text)) = self
            .entries
            .iter()
            .flatten()
            .find(|(cached_key, _)| *cached_key == key)
        {
            return Arc::clone(text);
        }
        let text = build();
        self.entries[self.next] = Some((key, Arc::clone(&text)));
        self.next = (self.next + 1) % self.entries.len();
        text
    }
}

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
    static RECENT_INT_CACHE_U32: RefCell<RecentTextCache<u32>> =
        RefCell::new(RecentTextCache::default());
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
fn shared_cached_int_u32(value: u32) -> Arc<str> {
    cached_text(&INT_CACHE_U32, value, TEXT_CACHE_LIMIT, || {
        value.to_string()
    })
}

#[inline(always)]
pub(super) fn cached_int_u32(value: u32) -> Arc<str> {
    RECENT_INT_CACHE_U32.with(|cache| {
        cache
            .borrow_mut()
            .get_or_insert_with(value, || shared_cached_int_u32(value))
    })
}

#[cfg(any(test, feature = "bench-support"))]
fn saturate_combo_text_cache() {
    INT_CACHE_U32.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clear();
        for value in 0..TEXT_CACHE_LIMIT as u32 {
            cache.insert(value, Arc::<str>::from(value.to_string()));
        }
    });
    RECENT_INT_CACHE_U32.with(|cache| *cache.borrow_mut() = RecentTextCache::default());
}

#[cfg(feature = "bench-support")]
pub(super) fn prepare_combo_text_benchmark() {
    saturate_combo_text_cache();
}

#[cfg(feature = "bench-support")]
pub(super) fn benchmark_combo_text_legacy(value: u32) -> Arc<str> {
    shared_cached_int_u32(value)
}

#[cfg(feature = "bench-support")]
pub(super) fn benchmark_combo_text(value: u32) -> Arc<str> {
    cached_int_u32(value)
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
fn preferred_mods_text_key(
    profile: &profile_data::Profile,
    scroll_speed: ScrollSpeedSetting,
) -> GameplayModsTextKey {
    let accel = AccelEffects::from_mask_bits(profile.accel_effects_active_mask.bits());
    let visual = VisualEffects::from_mask_bits(profile.visual_effects_active_mask.bits());
    let appearance =
        AppearanceEffects::from_mask_bits(profile.appearance_effects_active_mask.bits());
    let scroll = scroll_effects_from_flags(
        profile
            .scroll_option
            .contains(profile_data::ScrollOption::Reverse),
        profile
            .scroll_option
            .contains(profile_data::ScrollOption::Split),
        profile
            .scroll_option
            .contains(profile_data::ScrollOption::Alternate),
        profile
            .scroll_option
            .contains(profile_data::ScrollOption::Cross),
        profile
            .scroll_option
            .contains(profile_data::ScrollOption::Centered),
    );
    let (perspective_tilt, perspective_skew) = profile.perspective.tilt_skew();
    let display_mini = (profile.mini_percent as f32
        - if visual.big > f32::EPSILON {
            100.0 * visual.big
        } else {
            0.0
        })
    .clamp(-100.0, 150.0);
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
        insert_mask: profile.insert_active_mask.bits(),
        remove_mask: profile.remove_active_mask.bits(),
        holds_mask: profile.holds_active_mask.bits(),
        turn_bits: turn_option_bits(profile.turn_option),
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
        perspective_tilt: mod_percent_key(perspective_tilt),
        perspective_skew: mod_percent_key(perspective_skew),
        dark: mod_percent_key(if profile.hide_targets { 1.0 } else { 0.0 }),
        blind: 0,
        cover: mod_percent_key(if profile.hide_song_bg { 1.0 } else { 0.0 }),
        disabled_timing_windows: disabled_timing_window_bits(profile.timing_windows),
    }
}

#[inline(always)]
pub(crate) fn preferred_mods_text(state: &State, player_idx: usize) -> Arc<str> {
    // Simply Love's DisplayMods reads ModsLevel_Preferred. Runtime chart
    // attacks belong to the current/song levels and must not alter this text.
    let profile = &state.profiles()[player_idx];
    let key = preferred_mods_text_key(profile, state.scroll_speed_for_player(player_idx));
    cached_text(&GAMEPLAY_MODS_CACHE, key, TEXT_CACHE_LIMIT, || {
        crate_gameplay_mods_text(GameplayModsTextParams {
            speed: state.scroll_speed_for_player(player_idx),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_mods_key_uses_selected_profile_values() {
        let profile = profile_data::Profile {
            accel_effects_active_mask: profile_data::AccelEffectsMask::BOOST,
            visual_effects_active_mask: profile_data::VisualEffectsMask::DRUNK
                | profile_data::VisualEffectsMask::BIG,
            appearance_effects_active_mask: profile_data::AppearanceEffectsMask::HIDDEN,
            scroll_option: profile_data::ScrollOption::Reverse
                .union(profile_data::ScrollOption::Centered),
            mini_percent: 25,
            hide_targets: true,
            hide_song_bg: true,
            ..profile_data::Profile::default()
        };

        let key = preferred_mods_text_key(&profile, ScrollSpeedSetting::XMod(2.0));

        assert_eq!(key.accel[0], 100);
        assert_eq!(key.visual[0], 100);
        assert_eq!(key.appearance[0], 100);
        assert_eq!(key.scroll[0], 100);
        assert_eq!(key.scroll[4], 100);
        assert_eq!(key.mini_percent, -75);
        assert_eq!(key.dark, 100);
        assert_eq!(key.blind, 0);
        assert_eq!(key.cover, 100);
    }

    #[test]
    fn recent_combo_text_reuses_two_uncached_player_values() {
        saturate_combo_text_cache();
        let first_p1 = cached_int_u32(TEXT_CACHE_LIMIT as u32);
        let first_p2 = cached_int_u32(TEXT_CACHE_LIMIT as u32 + 1);
        let second_p1 = cached_int_u32(TEXT_CACHE_LIMIT as u32);
        let second_p2 = cached_int_u32(TEXT_CACHE_LIMIT as u32 + 1);

        assert_eq!(first_p1.as_ref(), TEXT_CACHE_LIMIT.to_string());
        assert_eq!(first_p2.as_ref(), (TEXT_CACHE_LIMIT + 1).to_string());
        assert!(Arc::ptr_eq(&first_p1, &second_p1));
        assert!(Arc::ptr_eq(&first_p2, &second_p2));
        INT_CACHE_U32.with(|cache| {
            let cache = cache.borrow();
            assert_eq!(cache.len(), TEXT_CACHE_LIMIT);
            assert!(!cache.contains_key(&(TEXT_CACHE_LIMIT as u32)));
            assert!(!cache.contains_key(&(TEXT_CACHE_LIMIT as u32 + 1)));
        });
    }
}
