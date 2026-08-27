use crate::screens::gameplay::GameplayCoreState as State;
use deadlib_present::actors::{InlineText, TextAlign};
use deadlib_present::compose::{ComposeScratch, TextLayoutCache, prewarm_u32_text_slot};
use deadlib_present::font;
use deadsync_notefield::{
    COUNTER_TEXT_SLOTS_PER_PLAYER, MEASURE_COUNTER_LOOKAHEAD_MAX, MiniIndicatorMode,
    zmod_broken_run_end,
};
use deadsync_profile as profile_data;

use super::super::display_mods::DISPLAY_MODS_WRAP_WIDTH_PX;
use super::super::{
    FRAME_TEXT_COMBO_BASE, FRAME_TEXT_COUNTDOWN_BASE, FRAME_TEXT_COUNTER_BASE,
    FRAME_TEXT_EDIT_MEASURE_BASE, FRAME_TEXT_ERROR_BASE, FRAME_TEXT_MINI_BASE,
    FRAME_TEXT_VERTEX_BUFFERS,
};
use super::text::{cached_int_i32, preferred_mods_text};
use super::{
    MEASURE_PREWARM_CAP, zmod_combo_font_name, zmod_indicator_mode, zmod_small_combo_font,
};

pub fn prewarm_text_layout(cache: &mut TextLayoutCache, fonts: &font::FontMap, state: &State) {
    let prewarm_i32 = |cache: &mut TextLayoutCache, font_name: &'static str, value: i32| {
        let text = cached_int_i32(value);
        cache.prewarm_text(fonts, font_name, text.as_ref(), None);
    };
    let mut max_measure_len = 0i32;

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
            let scaled_len = (((seg.end() - seg.start()) as f32) * multiplier)
                .floor()
                .max(0.0) as i32;
            max_measure_len = max_measure_len.max(scaled_len);
            if !seg.is_break() {
                let (broken_end, _) = zmod_broken_run_end(segs, seg_ix);
                max_measure_len = max_measure_len.max(broken_end - seg.start() as i32);
            }
        }
        if profile.measure_counter != profile_data::MeasureCounter::None {
            let countdown_max = max_measure_len.clamp(16, MEASURE_PREWARM_CAP);
            for value in 0..=countdown_max {
                prewarm_i32(cache, mc_font_name, value);
            }
            prewarm_i32(cache, mc_font_name, max_measure_len.max(16));
        }
    }
}

/// # Panics
///
/// Panics if an internal state invariant is violated.
pub fn prewarm_frame_text_scratch(
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    fonts: &font::FontMap,
    state: &State,
    edit_measure_text: bool,
) {
    let mini_glyphs = InlineText::copy_from("+-.%0123456789")
        .expect("the mini-indicator glyph domain fits inline");
    let offset_glyphs = InlineText::copy_from("-.ms0123456789")
        .expect("the offset-indicator glyph domain fits inline");
    let error_label_glyphs =
        InlineText::copy_from("EarlyLate").expect("the error-bar label glyph domain fits inline");
    let error_feedback_glyphs = InlineText::copy_from("EARLYTFSOW")
        .expect("the error-bar feedback glyph domain fits inline");
    let counter_glyphs = InlineText::copy_from("-/()0123456789")
        .expect("the measure-counter glyph domain fits inline");
    let timer_glyphs =
        InlineText::copy_from(" .0123456789").expect("the run-timer glyph domain fits inline");
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
        let countdown_slot = FRAME_TEXT_COUNTDOWN_BASE
            + player as u8 * deadsync_notefield::COLUMN_COUNTDOWN_SLOTS_PER_PLAYER;
        if profile.column_cues {
            prewarm_u32_text_slot(
                cache,
                fonts,
                zmod_small_combo_font(profile.combo_font),
                countdown_slot,
                TextAlign::Center,
            );
        }
        if profile.crossover_cues && profile.column_countdown {
            for offset in 1..deadsync_notefield::COLUMN_COUNTDOWN_SLOTS_PER_PLAYER {
                prewarm_u32_text_slot(
                    cache,
                    fonts,
                    zmod_small_combo_font(profile.combo_font),
                    countdown_slot + offset,
                    TextAlign::Center,
                );
            }
        }
        if edit_measure_text {
            let edit_measure_slot = FRAME_TEXT_EDIT_MEASURE_BASE
                + player as u8 * deadsync_notefield::EDIT_MEASURE_TEXT_SLOTS_PER_PLAYER;
            for offset in 0..deadsync_notefield::EDIT_MEASURE_TEXT_SLOTS_PER_PLAYER {
                prewarm_u32_text_slot(
                    cache,
                    fonts,
                    "miso",
                    edit_measure_slot + offset,
                    TextAlign::Right,
                );
            }
        }
        if zmod_indicator_mode(profile) != MiniIndicatorMode::None {
            deadlib_present::compose::prewarm_prepared_inline_text_slot(
                cache,
                scratch,
                fonts,
                zmod_small_combo_font(profile.combo_font),
                mini_glyphs,
                FRAME_TEXT_MINI_BASE + player as u8,
                TextAlign::Left,
                FRAME_TEXT_VERTEX_BUFFERS,
            );
        }
        let mut error_mask = profile.error_bar_active_mask;
        if error_mask.is_empty() {
            error_mask =
                profile_data::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text);
        }
        let error_slot = FRAME_TEXT_ERROR_BASE
            + player as u8 * deadsync_notefield::ERROR_BAR_TEXT_SLOTS_PER_PLAYER;
        if profile.error_ms_display {
            deadlib_present::compose::prewarm_prepared_inline_text_slot(
                cache,
                scratch,
                fonts,
                "wendy",
                offset_glyphs,
                error_slot,
                TextAlign::Center,
                FRAME_TEXT_VERTEX_BUFFERS,
            );
        }
        if error_mask.contains(profile_data::ErrorBarMask::MONOCHROME) {
            for slot in [error_slot + 1, error_slot + 2] {
                deadlib_present::compose::prewarm_prepared_inline_text_slot(
                    cache,
                    scratch,
                    fonts,
                    "game",
                    error_label_glyphs,
                    slot,
                    TextAlign::Center,
                    FRAME_TEXT_VERTEX_BUFFERS,
                );
            }
        }
        if error_mask.contains(profile_data::ErrorBarMask::TEXT) {
            deadlib_present::compose::prewarm_prepared_inline_text_slot(
                cache,
                scratch,
                fonts,
                "wendy",
                error_feedback_glyphs,
                error_slot + 3,
                TextAlign::Center,
                FRAME_TEXT_VERTEX_BUFFERS,
            );
        }
        if profile.measure_counter != profile_data::MeasureCounter::None {
            let font_name = zmod_small_combo_font(profile.combo_font);
            let slot_base = FRAME_TEXT_COUNTER_BASE + player as u8 * COUNTER_TEXT_SLOTS_PER_PLAYER;
            let lookahead = profile
                .measure_counter_lookahead
                .min(MEASURE_COUNTER_LOOKAHEAD_MAX);
            for slot in 0..=lookahead {
                deadlib_present::compose::prewarm_cached_prepared_inline_text_slot(
                    cache,
                    scratch,
                    fonts,
                    font_name,
                    counter_glyphs,
                    slot_base + slot,
                    TextAlign::Center,
                    FRAME_TEXT_VERTEX_BUFFERS,
                );
            }
            if profile.broken_run {
                deadlib_present::compose::prewarm_cached_prepared_inline_text_slot(
                    cache,
                    scratch,
                    fonts,
                    font_name,
                    counter_glyphs,
                    slot_base + MEASURE_COUNTER_LOOKAHEAD_MAX + 1,
                    TextAlign::Center,
                    FRAME_TEXT_VERTEX_BUFFERS,
                );
            }
            if profile.run_timer {
                deadlib_present::compose::prewarm_prepared_inline_text_slot(
                    cache,
                    scratch,
                    fonts,
                    font_name,
                    timer_glyphs,
                    slot_base + MEASURE_COUNTER_LOOKAHEAD_MAX + 2,
                    TextAlign::Center,
                    FRAME_TEXT_VERTEX_BUFFERS,
                );
            }
        }
    }
}
