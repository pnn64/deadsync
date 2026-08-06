use crate::screens::gameplay::GameplayCoreState as State;
use deadlib_present::actors::{InlineText, TextContent};
use deadlib_present::compose::{ComposeScratch, TextLayoutCache};
use deadlib_present::font;
use deadsync_notefield::{MiniIndicatorMode, ZmodMeasureCounterText, zmod_broken_run_end};
use deadsync_profile as profile_data;

use super::super::display_mods::DISPLAY_MODS_WRAP_WIDTH_PX;
use super::text::{
    cached_int_i32, preferred_mods_text, zmod_measure_counter_text, zmod_run_timer_fmt,
};
use super::{
    COLUMN_COUNTDOWN_PREWARM_CAP, COMBO_PREWARM_CAP, MEASURE_PREWARM_CAP, RUN_TIMER_PREWARM_CAP_S,
    zmod_combo_font_name, zmod_indicator_mode, zmod_small_combo_font,
};

pub fn prewarm_text_layout(cache: &mut TextLayoutCache, fonts: &font::FontMap, state: &State) {
    let prewarm_u32 = |cache: &mut TextLayoutCache, font_name: &'static str, value: u32| {
        let text = TextContent::inline_u32(value);
        cache.prewarm_text(fonts, font_name, text.as_str(), None);
    };
    let prewarm_i32 = |cache: &mut TextLayoutCache, font_name: &'static str, value: i32| {
        let text = cached_int_i32(value);
        cache.prewarm_text(fonts, font_name, text.as_ref(), None);
    };
    let prewarm_ratio =
        |cache: &mut TextLayoutCache, font_name: &'static str, curr: i32, total: i32| {
            let text = zmod_measure_counter_text(ZmodMeasureCounterText::Ratio {
                current: curr,
                total,
            });
            cache.prewarm_text(fonts, font_name, text.as_str(), None);
        };
    let prewarm_timer = |cache: &mut TextLayoutCache,
                         font_name: &'static str,
                         second: i32,
                         threshold: i32,
                         trailing: bool| {
        let text = zmod_run_timer_fmt(second, threshold, trailing);
        cache.prewarm_text(fonts, font_name, text.as_str(), None);
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

        let mods_text = preferred_mods_text(state, player);
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
                max_measure_len = max_measure_len.max(broken_end - seg.start as i32);
            }
        }
        let prewarm_measure_len = max_measure_len.min(MEASURE_PREWARM_CAP);
        for total in 1..=prewarm_measure_len {
            let total_text = zmod_measure_counter_text(ZmodMeasureCounterText::Total(total));
            cache.prewarm_text(fonts, mc_font_name, total_text.as_str(), None);
            let break_text = zmod_measure_counter_text(ZmodMeasureCounterText::Break(total));
            cache.prewarm_text(fonts, mc_font_name, break_text.as_str(), None);
            for curr in 1..=total {
                prewarm_ratio(cache, mc_font_name, curr, total);
            }
        }
        if max_measure_len > prewarm_measure_len {
            let total_text =
                zmod_measure_counter_text(ZmodMeasureCounterText::Total(max_measure_len));
            cache.prewarm_text(fonts, mc_font_name, total_text.as_str(), None);
            let break_text =
                zmod_measure_counter_text(ZmodMeasureCounterText::Break(max_measure_len));
            cache.prewarm_text(fonts, mc_font_name, break_text.as_str(), None);
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
        if profile.column_cues || (profile.crossover_cues && profile.column_countdown) {
            let music_rate = state.music_rate();
            let rate = if music_rate.is_finite() && music_rate > 0.0 {
                music_rate
            } else {
                1.0
            };
            let mut countdown_max = 0;
            if profile.column_cues {
                for cue in state.column_cues(player) {
                    countdown_max = countdown_max.max((cue.duration / rate).ceil() as i32);
                }
            }
            if profile.crossover_cues && profile.column_countdown {
                for cue in state.crossover_cues(player) {
                    countdown_max = countdown_max.max((cue.duration / rate).ceil() as i32);
                }
            }
            let capped = countdown_max.clamp(0, COLUMN_COUNTDOWN_PREWARM_CAP);
            for value in 0..=capped {
                prewarm_i32(cache, mc_font_name, value);
            }
            if countdown_max > capped {
                prewarm_i32(cache, mc_font_name, countdown_max);
            }
        }
        if profile.error_ms_display {
            cache.prewarm_text(fonts, "wendy", "0.00ms", None);
        }
    }

    cache.prewarm_text(fonts, "game", "Early", None);
    cache.prewarm_text(fonts, "game", "Late", None);
    cache.prewarm_text(fonts, "wendy", "EARLY", None);
    cache.prewarm_text(fonts, "wendy", "LATE", None);
}

pub fn prewarm_frame_text_scratch(
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    fonts: &font::FontMap,
    state: &State,
) {
    let mut longest = InlineText::new();
    assert!(longest.push_ascii(b'-'));
    assert!(longest.push_u32(u32::MAX));
    let enabled_players = (0..state.num_players())
        .filter(|&player| zmod_indicator_mode(&state.profiles()[player]) != MiniIndicatorMode::None)
        .count();
    let vertex_buffers = enabled_players.saturating_mul(4);
    for player in 0..state.num_players() {
        let profile = &state.profiles()[player];
        if zmod_indicator_mode(profile) == MiniIndicatorMode::None {
            continue;
        }
        deadlib_present::compose::prewarm_frame_inline_text(
            cache,
            scratch,
            fonts,
            zmod_small_combo_font(profile.combo_font),
            longest,
            vertex_buffers,
        );
    }
}
