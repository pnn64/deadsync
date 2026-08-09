use crate::screens::gameplay::GameplayCoreState as State;
use deadlib_present::actors::{InlineText, TextAlign};
use deadlib_present::compose::{ComposeScratch, TextLayoutCache, prewarm_u32_text_slot};
use deadlib_present::font;
use deadsync_notefield::{MiniIndicatorMode, ZmodMeasureCounterText, zmod_broken_run_end};
use deadsync_profile as profile_data;

use super::super::display_mods::DISPLAY_MODS_WRAP_WIDTH_PX;
use super::super::{
    FRAME_TEXT_COMBO_BASE, FRAME_TEXT_MINI_BASE, FRAME_TEXT_OFFSET_BASE, FRAME_TEXT_VERTEX_BUFFERS,
};
use super::text::{
    cached_int_i32, preferred_mods_text, zmod_measure_counter_text, zmod_run_timer_fmt,
};
use super::{
    COLUMN_COUNTDOWN_PREWARM_CAP, MEASURE_PREWARM_CAP, RUN_TIMER_PREWARM_CAP_S,
    zmod_combo_font_name, zmod_indicator_mode, zmod_small_combo_font,
};

pub fn prewarm_text_layout(cache: &mut TextLayoutCache, fonts: &font::FontMap, state: &State) {
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
    let mut max_measure_len = 0i32;
    let music_end_seconds =
        deadsync_core::song_time::song_time_ns_to_seconds(state.music_end_time_ns())
            .ceil()
            .max(0.0) as i32;

    for player in 0..state.num_players() {
        let profile = &state.profiles()[player];
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
    let mini_glyphs = InlineText::copy_from("+-.%0123456789")
        .expect("the mini-indicator glyph domain fits inline");
    let offset_glyphs = InlineText::copy_from("-.ms0123456789")
        .expect("the offset-indicator glyph domain fits inline");
    for player in 0..state.num_players() {
        let profile = &state.profiles()[player];
        if let Some(font_name) = zmod_combo_font_name(profile.combo_font) {
            prewarm_u32_text_slot(
                cache,
                fonts,
                font_name,
                FRAME_TEXT_COMBO_BASE + player as u8,
                TextAlign::Center,
            );
        }
        if zmod_indicator_mode(profile) != MiniIndicatorMode::None {
            deadlib_present::compose::prewarm_frame_inline_text_slot(
                cache,
                scratch,
                fonts,
                zmod_small_combo_font(profile.combo_font),
                mini_glyphs,
                FRAME_TEXT_MINI_BASE + player as u8,
                FRAME_TEXT_VERTEX_BUFFERS,
            );
        }
        if profile.error_ms_display {
            deadlib_present::compose::prewarm_prepared_inline_text_slot(
                cache,
                scratch,
                fonts,
                "wendy",
                offset_glyphs,
                FRAME_TEXT_OFFSET_BASE + player as u8,
                TextAlign::Center,
                FRAME_TEXT_VERTEX_BUFFERS,
            );
        }
    }
}
