use deadsync_theme::NotefieldStyle;

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
    }
}
