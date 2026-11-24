use deadsync_theme::NotefieldHudStyle;
pub use deadsync_assets::gameplay_presentation::field_style::DEFAULT_GAMEPLAY_HUD_STYLE as SIMPLY_LOVE_HUD_STYLE;

#[must_use]
pub const fn notefield_style() -> NotefieldHudStyle {
    SIMPLY_LOVE_HUD_STYLE
}

#[must_use]
pub fn notefield_style_with_palette(palette: deadsync_theme::color::JudgmentPalette) -> NotefieldHudStyle {
    if palette == crate::color::SIMPLY_LOVE_JUDGMENT_PALETTE {
        return SIMPLY_LOVE_HUD_STYLE;
    }
    deadsync_assets::gameplay_presentation::field_style::notefield_style_with_palette(palette)
}

#[cfg(test)]
mod tests {
    use super::{notefield_style, notefield_style_with_palette};
    use deadlib_present::color::Color;
    use deadsync_theme::color::JudgmentColorRole;

    #[test]
    fn simply_love_hud_metrics() {
        let style = notefield_style();

        assert_eq!(style.combo_centered_y, 155.0);
        assert_eq!(style.edit_measure_number_font, "miso");
        assert_eq!(style.column_cue.top_y, 80.0);
        assert_eq!(style.column_cue.reverse_anchor_y, 304.0);
        assert_eq!(style.column_cue.crossover_height_trim, 270.0);
        assert_eq!(style.column_cue.normal_color, [0.3, 1.0, 1.0]);
        assert_eq!(style.column_cue.countdown_reverse_y, 340.0);
        assert_eq!(style.column_cue.body_z, 90);
        assert_eq!(style.column_flash.default_layout.top_y, 80.0);
        assert_eq!(style.column_flash.compact_layout.height_trim, 270.0);
        assert_eq!(style.column_flash.compact_layout.reverse_trim, 30.0);
        assert_eq!(style.column_flash.normal_alpha, 0.66);
        assert_eq!(style.column_flash.dimmed_alpha, 0.3);
        assert_eq!(style.column_flash.z, 91);
        assert_eq!(style.counter_hud.text_z, 85);
        assert_eq!(style.counter_hud.left_column_scale, 4.0 / 3.0);
        assert_eq!(style.counter_hud.broken_color[3], 0.7);
        assert_eq!(style.mini_indicator.under_up_x_offset, -45.0);
        assert_eq!(style.mini_indicator.unanchored_x_offset, -12.0);
        assert_eq!(style.judgment_feedback.tap_front_z, 200);
        assert_eq!(style.judgment_feedback.tap_back_z, 95);
        assert_eq!(style.judgment_feedback.held_miss_reverse_y, 110.0);
        assert_eq!(style.judgment_feedback.hold_final_zoom, 32.0 / 140.0);
        assert_eq!(style.combo_feedback.threshold, 4);
        assert_eq!(style.combo_feedback.milestone_z, 89);
        assert_eq!(style.combo_feedback.number_z, 90);
        assert_eq!(style.combo_feedback.thousand_x_travel, 100.0);
        assert_eq!(style.error_bar.colorful_width, 160.0);
        assert_eq!(style.error_bar.colorful_border_size, 4.0);
        assert_eq!(style.error_bar.monochrome_width, 240.0);
        assert_eq!(style.error_bar.monochrome_border_size, 2.0);
        assert_eq!(style.error_bar.monochrome_center_width, 2.0);
        assert_eq!(style.error_bar.monochrome_line_width, 1.0);
        assert_eq!(style.error_bar.average_width, 325.0);
        assert_eq!(style.error_bar.average_tick_padding, 4.0);
        assert_eq!(style.error_bar.offset_indicator_gap, 6.0);
        assert_eq!(style.error_bar.long_average_tick_extra_height, 65.0);
        assert_eq!(
            style.error_bar.long_average_tick_color,
            deadlib_present::color::rgba_hex("#0000ff")
        );
        assert_eq!(style.error_bar.text_x_offset, 40.0);
        assert_eq!(style.error_bar.text_font, "wendy");
        assert_eq!(
            style.error_bar.palette.decent,
            deadlib_present::color::rgba_hex("#b45cff")
        );
        assert_eq!(style.error_bar.front_layers.text, 184);
        assert_eq!(style.error_bar.back_layers.text, 90);
        assert_eq!(style.error_bar.average_z, 88);
    }

    #[test]
    fn custom_palette_recolors_judgment_feedback_without_changing_geometry() {
        let baseline = notefield_style();
        let custom = crate::color::SIMPLY_LOVE_JUDGMENT_PALETTE.with_color(
            JudgmentColorRole::Miss,
            Color::rgb(0.2, 0.4, 0.6).to_rgba(),
            crate::color::JUDGMENT_PRESET.dim_peaks,
        );
        let styled = notefield_style_with_palette(custom);

        assert_eq!(styled.judgment_normal_y, baseline.judgment_normal_y);
        assert_eq!(styled.column_flash.miss_color, [0.2, 0.4, 0.6]);
    }
}
