use deadsync_theme::{
    ColumnCueStyle, ColumnFlashLayoutStyle, ColumnFlashStyle, ComboFeedbackStyle, CounterHudStyle,
    ErrorBarLayers, ErrorBarPalette, ErrorBarStyle, JudgmentFeedbackStyle, MiniIndicatorStyle,
    NotefieldStyle,
};

const fn rgb8(r: u8, g: u8, b: u8) -> [f32; 3] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

const fn rgba8(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

pub const SIMPLY_LOVE_NOTEFIELD_STYLE: NotefieldStyle = NotefieldStyle {
    layout_width_min: 640.0,
    layout_width_max: 854.0,
    side_center_x_ratio: 0.25,
    receptor_normal_y: -125.0,
    receptor_reverse_y: 145.0,
    judgment_normal_y: -30.0,
    judgment_reverse_y: 30.0,
    judgment_centered_y: 95.0,
    combo_normal_y: 30.0,
    combo_reverse_y: -30.0,
    combo_centered_y: 155.0,
    judgment_height: 40.0,
    error_bar_offset_y: 25.0,
    measure_line_overscan_y: 400.0,
    measure_line_z: 80,
    measure_cue_scroll_color: [0.824, 0.706, 0.549],
    measure_cue_bpm_color: [1.0, 1.0, 0.0],
    measure_cue_delay_color: [1.0, 0.45, 0.75],
    measure_cue_stop_color: [1.0, 0.0, 0.0],
    measure_cue_alpha: 0.7,
    edit_measure_number_font: "miso",
    column_cue: ColumnCueStyle {
        top_y: 80.0,
        reverse_anchor_y: 304.0,
        crossover_height_trim: 270.0,
        body_fade: 0.333,
        base_alpha: 0.12,
        normal_color: [0.3, 1.0, 1.0],
        mine_color: [1.0, 0.0, 0.0],
        countdown_normal_y: 160.0,
        countdown_reverse_y: 340.0,
        countdown_color: [1.0, 1.0, 1.0],
        countdown_zoom: 0.5,
        body_z: 90,
        countdown_z: 200,
    },
    column_flash: ColumnFlashStyle {
        default_layout: ColumnFlashLayoutStyle {
            top_y: 80.0,
            height_trim: 0.0,
            reverse_trim: 0.0,
            fade: 0.333,
        },
        compact_layout: ColumnFlashLayoutStyle {
            top_y: 70.0,
            height_trim: 270.0,
            reverse_trim: 30.0,
            fade: 0.2,
        },
        reverse_anchor_y: 304.0,
        normal_alpha: 0.66,
        dimmed_alpha: 0.3,
        miss_color: [1.0, 0.0, 0.0],
        decent_color: [0.70, 0.36, 1.0],
        way_off_color: rgb8(0xc9, 0x85, 0x5e),
        great_color: rgb8(0x66, 0xc9, 0x55),
        excellent_color: rgb8(0xe2, 0x9c, 0x18),
        fantastic_color: [1.0, 1.0, 1.0],
        fantastic_blue_color: rgb8(0x21, 0xcc, 0xe8),
        z: 91,
    },
    counter_hud: CounterHudStyle {
        text_z: 85,
        shadow_len: 1.0,
        base_zoom: 0.35,
        lookahead_zoom_step: 0.05,
        vertical_step_y: 20.0,
        left_column_scale: 4.0 / 3.0,
        horizontal_span: 2.0,
        break_lookahead_color: [0.4, 0.4, 0.4, 1.0],
        break_current_color: [0.5, 0.5, 0.5, 1.0],
        stream_lookahead_color: [0.45, 0.45, 0.45, 1.0],
        ratio_color: [1.0, 1.0, 1.0, 1.0],
        total_color: [0.5, 0.5, 0.5, 1.0],
        broken_y_offset: 15.0,
        broken_vertical_y_offset: -15.0,
        broken_vertical_x_scale: 4.0 / 3.0,
        broken_color: [1.0, 1.0, 1.0, 0.7],
        run_active_color: [1.0, 1.0, 1.0, 1.0],
        run_inactive_color: [0.5, 0.5, 0.5, 1.0],
    },
    mini_indicator: MiniIndicatorStyle {
        column_offset: 1.0,
        under_up_x_offset: -45.0,
        unanchored_x_offset: -12.0,
        failed_color: [0.5, 0.5, 0.5],
        shadow_len: 1.0,
        text_z: 85,
    },
    judgment_feedback: JudgmentFeedbackStyle {
        tap_front_z: 200,
        tap_back_z: 95,
        split_overlay_alpha: 0.5,
        held_miss_normal_y: -50.0,
        held_miss_reverse_y: 110.0,
        held_miss_z: 196,
        hold_normal_y: -90.0,
        hold_reverse_y: 90.0,
        hold_z: 195,
        hold_initial_zoom: 25.6 / 140.0,
        hold_final_zoom: 32.0 / 140.0,
    },
    combo_feedback: ComboFeedbackStyle {
        threshold: 4,
        milestone_z: 89,
        number_z: 90,
        number_zoom: 0.75,
        shadow_len: 1.0,
        miss_color: [1.0, 0.0, 0.0, 1.0],
        burst_duration: 0.5,
        burst_start_zoom: 2.0,
        burst_end_zoom: 1.0,
        burst_start_alpha: 0.5,
        burst_rotation_deg: 90.0,
        hundred_start_zoom: 0.25,
        hundred_end_zoom: 2.0,
        hundred_start_alpha: 0.6,
        hundred_start_rotation_deg: 10.0,
        mini_duration: 0.4,
        mini_start_zoom: 0.25,
        mini_end_zoom: 1.8,
        mini_start_alpha: 1.0,
        mini_start_rotation_deg: 10.0,
        thousand_start_zoom: 0.25,
        thousand_end_zoom: 3.0,
        thousand_start_alpha: 0.7,
        thousand_x_travel: 100.0,
    },
    error_bar: ErrorBarStyle {
        colorful_width: 160.0,
        colorful_height: 10.0,
        colorful_border_size: 4.0,
        average_width: 325.0,
        average_height: 7.0,
        average_tick_padding: 4.0,
        monochrome_width: 240.0,
        monochrome_border_size: 2.0,
        monochrome_center_width: 2.0,
        monochrome_line_width: 1.0,
        tick_width: 2.0,
        colorful_tick_duration: 0.5,
        monochrome_tick_duration: 0.75,
        average_tick_extra_height: 75.0,
        monochrome_background_alpha: 0.5,
        line_alpha: 0.3,
        lines_fade_start: 2.5,
        lines_fade_duration: 0.5,
        label_fade_duration: 0.5,
        label_hold: 2.0,
        label_x_ratio: 0.25,
        label_zoom: 0.7,
        center_tick_width: 1.0,
        highlight_inactive_alpha: 0.3,
        offset_indicator_duration: 0.5,
        offset_indicator_gap: 6.0,
        offset_indicator_zoom: 0.25,
        offset_indicator_shadow_len: 1.0,
        long_average_tick_duration: 0.5,
        long_average_tick_extra_height: 65.0,
        long_average_tick_width: 1.0,
        text_duration: 0.5,
        text_x_offset: 40.0,
        text_zoom: 0.25,
        text_shadow_len: 1.0,
        background_color: [0.0, 0.0, 0.0, 1.0],
        monochrome_center_color: [0.5, 0.5, 0.5, 1.0],
        monochrome_line_color: [1.0, 1.0, 1.0, 1.0],
        label_color: [1.0, 1.0, 1.0, 1.0],
        colorful_tick_color: rgba8(0xb2, 0x00, 0x00),
        average_center_tick_color: [1.0, 1.0, 1.0, 0.3],
        long_average_tick_color: rgba8(0x00, 0x00, 0xff),
        text_early_color: rgba8(0x06, 0x6a, 0xf4),
        text_late_color: rgba8(0xff, 0x5a, 0x4e),
        text_scaled_early_color: rgba8(0x00, 0x51, 0xdb),
        text_scaled_late_color: rgba8(0xff, 0x16, 0x05),
        palette: ErrorBarPalette {
            fantastic_blue: rgba8(0x21, 0xcc, 0xe8),
            fa_plus_white: [1.0, 1.0, 1.0, 1.0],
            excellent: rgba8(0xe2, 0x9c, 0x18),
            great: rgba8(0x66, 0xc9, 0x55),
            decent: rgba8(0xb4, 0x5c, 0xff),
            way_off: rgba8(0xc9, 0x85, 0x5e),
        },
        label_font: "game",
        offset_indicator_font: "wendy",
        text_font: "wendy",
        early_label: "Early",
        late_label: "Late",
        front_layers: ErrorBarLayers {
            background: 180,
            band: 181,
            line: 182,
            tick: 183,
            text: 184,
        },
        back_layers: ErrorBarLayers {
            background: 86,
            band: 87,
            line: 88,
            tick: 89,
            text: 90,
        },
        average_z: 88,
    },
};

pub const fn notefield_style() -> NotefieldStyle {
    SIMPLY_LOVE_NOTEFIELD_STYLE
}

#[cfg(test)]
mod tests {
    use super::notefield_style;

    #[test]
    fn factory_keeps_simply_love_gameplay_metrics() {
        let style = notefield_style();

        assert_eq!(style.receptor_normal_y, -125.0);
        assert_eq!(style.receptor_reverse_y, 145.0);
        assert_eq!(style.judgment_centered_y, 95.0);
        assert_eq!(style.combo_centered_y, 155.0);
        assert_eq!(style.measure_line_z, 80);
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
            super::rgba8(0x00, 0x00, 0xff)
        );
        assert_eq!(style.error_bar.text_x_offset, 40.0);
        assert_eq!(style.error_bar.text_font, "wendy");
        assert_eq!(
            style.error_bar.palette.decent,
            super::rgba8(0xb4, 0x5c, 0xff)
        );
        assert_eq!(style.error_bar.front_layers.text, 184);
        assert_eq!(style.error_bar.back_layers.text, 90);
        assert_eq!(style.error_bar.average_z, 88);
    }
}
