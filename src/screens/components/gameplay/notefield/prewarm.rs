use crate::game::GameplayCoreState as State;
use deadlib_present::compose::TextLayoutCache;
use deadlib_present::font;
use deadsync_notefield::MiniIndicatorMode;
use deadsync_profile as profile_data;
use std::collections::HashMap;

use super::text::{
    cached_int_i32, cached_int_u32, cached_neg_int_u32, cached_offset_ms, cached_paren_i32,
    cached_percent2_f64, cached_ratio_i32, cached_signed_percent2_f64, gameplay_mods_text,
    zmod_run_timer_fmt,
};
use super::{
    COMBO_PREWARM_CAP, DISPLAY_MODS_WRAP_WIDTH_PX, MEASURE_PREWARM_CAP, RUN_TIMER_PREWARM_CAP_S,
    zmod_broken_run_end, zmod_combo_font_name, zmod_indicator_mode, zmod_small_combo_font,
};

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
        let text = zmod_run_timer_fmt(second, threshold, trailing);
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
