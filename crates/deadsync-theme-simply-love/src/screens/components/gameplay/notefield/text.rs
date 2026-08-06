use crate::screens::gameplay::GameplayCoreState as State;
use deadlib_present::actors::{InlineText, TextContent};
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

thread_local! {
    #[cfg(feature = "bench-support")]
    static BENCH_MINI_SIGNED_CACHE: RefCell<FastTextCache<(u32, bool)>> = RefCell::new(
        HashMap::with_capacity_and_hasher(TEXT_CACHE_LIMIT, BuildHasherDefault::default()),
    );
    static INT_CACHE_I32: RefCell<FastTextCache<i32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        512,
        BuildHasherDefault::default(),
    ));
    #[cfg(feature = "bench-support")]
    static INT_CACHE_U32: RefCell<FastTextCache<u32>> = RefCell::new(HashMap::with_capacity_and_hasher(
        512,
        BuildHasherDefault::default(),
    ));
    static RATIO_CACHE_I32: RefCell<FastTextCache<(i32, i32)>> = RefCell::new(
        HashMap::with_capacity_and_hasher(1024, BuildHasherDefault::default()),
    );
    static GAMEPLAY_MODS_CACHE: RefCell<FastTextCache<GameplayModsTextKey>> = RefCell::new(
        HashMap::with_capacity_and_hasher(256, BuildHasherDefault::default()),
    );
    #[cfg(feature = "bench-support")]
    static BENCH_OFFSET_MS_CACHE: RefCell<FastTextCache<i32>> = RefCell::new(
        HashMap::with_capacity_and_hasher(512, BuildHasherDefault::default()),
    );
    #[cfg(feature = "bench-support")]
    static BENCH_ERROR_BAR_LABEL_CACHE: RefCell<FastTextCache<(bool, bool)>> = RefCell::new(
        HashMap::with_capacity_and_hasher(4, BuildHasherDefault::default()),
    );
    #[cfg(feature = "bench-support")]
    static BENCH_PAREN_INT_CACHE: RefCell<FastTextCache<i32>> = RefCell::new(
        HashMap::with_capacity_and_hasher(512, BuildHasherDefault::default()),
    );
    #[cfg(feature = "bench-support")]
    static BENCH_RUN_TIMER_CACHE: RefCell<FastTextCache<(i32, i32, bool)>> = RefCell::new(
        HashMap::with_capacity_and_hasher(1024, BuildHasherDefault::default()),
    );
}

#[cfg(feature = "bench-support")]
pub(super) fn reset_mini_text_benchmark() {
    BENCH_MINI_SIGNED_CACHE.with(|cache| cache.borrow_mut().clear());
}

#[cfg(feature = "bench-support")]
pub(super) fn benchmark_pacemaker_text_legacy(value: f64, negative: bool) -> TextContent {
    let centi = quantize_centi_u32(value);
    TextContent::Shared(cached_text(
        &BENCH_MINI_SIGNED_CACHE,
        (centi, negative),
        TEXT_CACHE_LIMIT,
        || {
            if negative {
                format!("-{:.2}%", centi as f64 / 100.0)
            } else {
                format!("+{:.2}%", centi as f64 / 100.0)
            }
        },
    ))
}

#[cfg(feature = "bench-support")]
pub(super) fn benchmark_pacemaker_text(value: f64, negative: bool) -> TextContent {
    zmod_mini_indicator_text_content(ZmodMiniIndicatorText::SignedPercent { value, negative })
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
pub(super) fn cached_int_i32(value: i32) -> Arc<str> {
    cached_text(&INT_CACHE_I32, value, TEXT_CACHE_LIMIT, || {
        value.to_string()
    })
}

#[cfg(feature = "bench-support")]
#[inline(always)]
fn shared_cached_int_u32(value: u32) -> Arc<str> {
    cached_text(&INT_CACHE_U32, value, TEXT_CACHE_LIMIT, || {
        value.to_string()
    })
}

#[cfg(feature = "bench-support")]
fn saturate_combo_text_cache() {
    INT_CACHE_U32.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clear();
        for value in 0..TEXT_CACHE_LIMIT as u32 {
            cache.insert(value, Arc::<str>::from(value.to_string()));
        }
    });
}

#[cfg(feature = "bench-support")]
pub(super) fn prepare_combo_text_benchmark() {
    saturate_combo_text_cache();
}

#[cfg(feature = "bench-support")]
pub(super) fn benchmark_combo_text_legacy(value: u32) -> TextContent {
    TextContent::Shared(shared_cached_int_u32(value))
}

#[cfg(feature = "bench-support")]
pub(super) fn benchmark_combo_text(value: u32) -> TextContent {
    TextContent::inline_u32(value)
}

#[inline(always)]
pub(super) fn cached_ratio_i32(curr: i32, total: i32) -> Arc<str> {
    cached_text(&RATIO_CACHE_I32, (curr, total), TEXT_CACHE_LIMIT, || {
        format!("{curr}/{total}")
    })
}

#[inline(always)]
pub(super) fn offset_ms_text(value: f32) -> TextContent {
    let key = quantize_centi_i32(f64::from(value));
    TextContent::inline_format(format_args!("{:.2}ms", key as f64 / 100.0))
        .expect("an i32 centisecond value and ms suffix fit inline")
}

#[inline(always)]
pub(super) const fn error_bar_text_label(early: bool, scaled: bool) -> TextContent {
    TextContent::Static(match (early, scaled) {
        (true, true) => "FAST",
        (true, false) => "EARLY",
        (false, true) => "SLOW",
        (false, false) => "LATE",
    })
}

#[cfg(feature = "bench-support")]
pub(super) fn benchmark_offset_ms_legacy(value: f32) -> Arc<str> {
    let key = quantize_centi_i32(f64::from(value));
    cached_text(&BENCH_OFFSET_MS_CACHE, key, TEXT_CACHE_LIMIT, || {
        format!("{:.2}ms", key as f64 / 100.0)
    })
}

#[cfg(feature = "bench-support")]
pub(super) fn benchmark_error_bar_label_legacy(early: bool, scaled: bool) -> Arc<str> {
    cached_text(
        &BENCH_ERROR_BAR_LABEL_CACHE,
        (early, scaled),
        TEXT_CACHE_LIMIT,
        || error_bar_text_label(early, scaled).as_str().to_string(),
    )
}

pub(super) fn zmod_run_timer_fmt(
    seconds: i32,
    minute_threshold: i32,
    trailing_space: bool,
) -> TextContent {
    let seconds = seconds.max(0);
    let mut text = InlineText::new();
    if seconds < 10 {
        assert!(text.push_ascii(b'0'));
        assert!(text.push_ascii(b'.'));
        assert!(text.push_ascii(b'0'));
        assert!(text.push_u32(seconds as u32));
    } else if seconds > minute_threshold {
        let secs = seconds % 60;
        assert!(text.push_u32((seconds / 60) as u32));
        assert!(text.push_ascii(b'.'));
        assert!(text.push_ascii(b'0' + (secs / 10) as u8));
        assert!(text.push_ascii(b'0' + (secs % 10) as u8));
    } else {
        assert!(text.push_ascii(b'0'));
        assert!(text.push_ascii(b'.'));
        assert!(text.push_u32(seconds as u32));
    }
    if trailing_space {
        assert!(text.push_ascii(b' '));
    }
    TextContent::Inline(text)
}

pub(super) fn zmod_measure_counter_text(text: ZmodMeasureCounterText) -> TextContent {
    let mut out = InlineText::new();
    match text {
        ZmodMeasureCounterText::Break(value) => {
            assert!(out.push_ascii(b'('));
            assert!(out.push_i32(value));
            assert!(out.push_ascii(b')'));
        }
        ZmodMeasureCounterText::Ratio { current, total } => {
            if !out.push_i32(current) || !out.push_ascii(b'/') || !out.push_i32(total) {
                return TextContent::Shared(cached_ratio_i32(current, total));
            }
        }
        ZmodMeasureCounterText::Total(value) => {
            assert!(out.push_i32(value));
        }
    }
    TextContent::Inline(out)
}

#[cfg(feature = "bench-support")]
pub(super) fn benchmark_measure_counter_text_legacy(text: ZmodMeasureCounterText) -> TextContent {
    TextContent::Shared(match text {
        ZmodMeasureCounterText::Break(value) => {
            cached_text(&BENCH_PAREN_INT_CACHE, value, TEXT_CACHE_LIMIT, || {
                format!("({value})")
            })
        }
        ZmodMeasureCounterText::Ratio { current, total } => cached_ratio_i32(current, total),
        ZmodMeasureCounterText::Total(value) => cached_int_i32(value),
    })
}

#[cfg(feature = "bench-support")]
pub(super) fn benchmark_run_timer_legacy(
    seconds: i32,
    minute_threshold: i32,
    trailing_space: bool,
) -> TextContent {
    let seconds = seconds.max(0);
    TextContent::Shared(cached_text(
        &BENCH_RUN_TIMER_CACHE,
        (seconds, minute_threshold, trailing_space),
        TEXT_CACHE_LIMIT,
        || {
            let mut text = if seconds < 10 {
                format!("0.0{seconds}")
            } else if seconds > minute_threshold {
                format!("{}.{:02}", seconds / 60, seconds % 60)
            } else {
                format!("0.{seconds}")
            };
            if trailing_space {
                text.push(' ');
            }
            text
        },
    ))
}

pub(super) fn zmod_mini_indicator_text_content(text: ZmodMiniIndicatorText) -> TextContent {
    let mut out = InlineText::new();
    match text {
        ZmodMiniIndicatorText::Percent(value) => push_percent(&mut out, value, None),
        ZmodMiniIndicatorText::SignedPercent { value, negative } => {
            push_percent(&mut out, value, Some(negative));
        }
        ZmodMiniIndicatorText::NegativeInt(value) => {
            assert!(out.push_ascii(b'-'));
            assert!(out.push_u32(value));
        }
    }
    TextContent::frame_inline(out)
}

#[inline]
fn push_percent(out: &mut InlineText, value: f64, negative: Option<bool>) {
    let centi = quantize_centi_u32(value);
    if let Some(negative) = negative {
        assert!(out.push_ascii(if negative { b'-' } else { b'+' }));
    }
    assert!(out.push_u32(centi / 100));
    assert!(out.push_ascii(b'.'));
    assert!(out.push_ascii(b'0' + ((centi / 10) % 10) as u8));
    assert!(out.push_ascii(b'0' + (centi % 10) as u8));
    assert!(out.push_ascii(b'%'));
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
fn preferred_mods_text_from(
    profile: &profile_data::Profile,
    scroll_speed: ScrollSpeedSetting,
) -> Arc<str> {
    // Simply Love's DisplayMods reads ModsLevel_Preferred. Runtime chart
    // attacks belong to the current/song levels and must not alter this text.
    let key = preferred_mods_text_key(profile, scroll_speed);
    cached_text(&GAMEPLAY_MODS_CACHE, key, TEXT_CACHE_LIMIT, || {
        crate_gameplay_mods_text(GameplayModsTextParams {
            speed: scroll_speed,
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
pub(crate) fn preferred_mods_text(state: &State, player_idx: usize) -> Arc<str> {
    preferred_mods_text_from(
        &state.profiles()[player_idx],
        state.scroll_speed_for_player(player_idx),
    )
}

#[cfg(feature = "bench-support")]
pub struct DisplayModsTextBench {
    profile: profile_data::Profile,
    scroll_speed: ScrollSpeedSetting,
    cached: Arc<str>,
}

#[cfg(feature = "bench-support")]
impl Default for DisplayModsTextBench {
    fn default() -> Self {
        let profile = profile_data::Profile {
            accel_effects_active_mask: profile_data::AccelEffectsMask::BOOST,
            visual_effects_active_mask: profile_data::VisualEffectsMask::DRUNK,
            appearance_effects_active_mask: profile_data::AppearanceEffectsMask::HIDDEN,
            scroll_option: profile_data::ScrollOption::Reverse,
            mini_percent: 25,
            spacing_percent: 125,
            hide_targets: true,
            ..profile_data::Profile::default()
        };
        let scroll_speed = ScrollSpeedSetting::XMod(2.0);
        let cached = preferred_mods_text_from(&profile, scroll_speed);
        Self {
            profile,
            scroll_speed,
            cached,
        }
    }
}

#[cfg(feature = "bench-support")]
impl DisplayModsTextBench {
    const SAMPLES: usize = 256;

    pub fn old_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let text = preferred_mods_text_from(
                std::hint::black_box(&self.profile),
                std::hint::black_box(self.scroll_speed),
            );
            checksum.rotate_left(7) ^ text.len() ^ sample
        })
    }

    pub fn new_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let text = Arc::clone(std::hint::black_box(&self.cached));
            checksum.rotate_left(7) ^ text.len() ^ sample
        })
    }
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
    fn inline_combo_text_preserves_all_decimal_boundaries() {
        for value in [0, 9, 10, 99, 100, u16::MAX as u32, u32::MAX] {
            assert_eq!(TextContent::inline_u32(value).as_str(), value.to_string());
        }
    }

    #[test]
    fn inline_offset_text_preserves_quantized_display() {
        for (value, expected) in [
            (f32::NEG_INFINITY, "0.00ms"),
            (f32::MIN, "-21474836.48ms"),
            (-180.005, "-180.01ms"),
            (-12.345, "-12.35ms"),
            (0.0, "0.00ms"),
            (12.345, "12.35ms"),
            (180.005, "180.01ms"),
            (f32::MAX, "21474836.47ms"),
            (f32::INFINITY, "0.00ms"),
        ] {
            let text = offset_ms_text(value);
            assert!(matches!(text, TextContent::Inline(_)));
            assert_eq!(text.as_str(), expected);
        }
    }

    #[test]
    fn error_bar_labels_match_fast_slow_modes() {
        for (early, scaled, expected) in [
            (true, true, "FAST"),
            (true, false, "EARLY"),
            (false, true, "SLOW"),
            (false, false, "LATE"),
        ] {
            let text = error_bar_text_label(early, scaled);
            assert!(matches!(text, TextContent::Static(_)));
            assert_eq!(text.as_str(), expected);
        }
    }

    #[test]
    fn inline_measure_counter_text_preserves_every_variant() {
        for (value, expected) in [
            (ZmodMeasureCounterText::Break(-12), "(-12)"),
            (
                ZmodMeasureCounterText::Ratio {
                    current: 37,
                    total: 64,
                },
                "37/64",
            ),
            (ZmodMeasureCounterText::Total(i32::MAX), "2147483647"),
        ] {
            let text = zmod_measure_counter_text(value);
            assert!(matches!(text, TextContent::Inline(_)));
            assert_eq!(text.as_str(), expected);
        }

        let overflow = zmod_measure_counter_text(ZmodMeasureCounterText::Ratio {
            current: i32::MIN,
            total: i32::MIN,
        });
        assert!(matches!(overflow, TextContent::Shared(_)));
        assert_eq!(overflow.as_str(), "-2147483648/-2147483648");
    }

    #[test]
    fn inline_run_timer_preserves_threshold_and_spacing() {
        for (seconds, threshold, trailing, expected) in [
            (-1, 59, false, "0.00"),
            (7, 59, true, "0.07 "),
            (59, 59, false, "0.59"),
            (60, 59, true, "1.00 "),
            (3_661, 59, false, "61.01"),
            (i32::MAX, 59, true, "35791394.07 "),
        ] {
            let text = zmod_run_timer_fmt(seconds, threshold, trailing);
            assert!(matches!(text, TextContent::Inline(_)));
            assert_eq!(text.as_str(), expected);
        }
    }

    #[test]
    fn inline_mini_indicator_text_preserves_quantized_display() {
        for (value, expected) in [
            (f64::NEG_INFINITY, "0.00%"),
            (-1.0, "0.00%"),
            (0.0, "0.00%"),
            (1.234, "1.23%"),
            (1.235, "1.24%"),
            (100.0, "100.00%"),
            (f64::INFINITY, "0.00%"),
            (f64::NAN, "0.00%"),
        ] {
            let text = zmod_mini_indicator_text_content(ZmodMiniIndicatorText::Percent(value));
            assert!(matches!(text, TextContent::FrameInline(_)));
            assert_eq!(text.as_str(), expected);
        }

        for (negative, expected) in [(true, "-12.35%"), (false, "+12.35%")] {
            let text = zmod_mini_indicator_text_content(ZmodMiniIndicatorText::SignedPercent {
                value: 12.345,
                negative,
            });
            assert_eq!(text.as_str(), expected);
        }

        let text = zmod_mini_indicator_text_content(ZmodMiniIndicatorText::NegativeInt(u32::MAX));
        assert_eq!(text.as_str(), "-4294967295");
    }
}
