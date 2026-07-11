pub use deadlib_assets::{FontAssetSpec, TextureAssetSpec, texture_asset};

pub struct ThemeAssetManifest<T> {
    pub fonts: &'static [FontAssetSpec],
    pub textures: T,
    pub texture_needs_repeat_sampler: fn(&str) -> bool,
}

/// Concrete-theme metrics consumed by the canonical notefield layout plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NotefieldStyle {
    pub layout_width_min: f32,
    pub layout_width_max: f32,
    pub side_center_x_ratio: f32,
    pub receptor_normal_y: f32,
    pub receptor_reverse_y: f32,
    pub judgment_normal_y: f32,
    pub judgment_reverse_y: f32,
    pub judgment_centered_y: f32,
    pub combo_normal_y: f32,
    pub combo_reverse_y: f32,
    pub combo_centered_y: f32,
    pub judgment_height: f32,
    pub error_bar_offset_y: f32,
    pub measure_line_overscan_y: f32,
    pub measure_line_z: i16,
    pub measure_cue_scroll_color: [f32; 3],
    pub measure_cue_bpm_color: [f32; 3],
    pub measure_cue_delay_color: [f32; 3],
    pub measure_cue_stop_color: [f32; 3],
    pub measure_cue_alpha: f32,
    pub edit_measure_number_font: &'static str,
}

/// Semantic font roles a concrete theme maps to its bundled font resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontRole {
    /// Default body text.
    Normal,
    /// Emphasized labels.
    Bold,
    /// Screen and section titles.
    Header,
    /// Bottom action prompts.
    Footer,
    /// Numeric statistics.
    Numbers,
    /// Evaluation panel numerics.
    ScreenEval,
    /// Large headline numerals.
    Headline,
}

#[cfg(test)]
mod tests {
    use super::NotefieldStyle;

    #[test]
    fn notefield_style_is_a_plain_metric_contract() {
        let style = NotefieldStyle {
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

        assert_eq!(style.layout_width_min, 640.0);
        assert_eq!(style.error_bar_offset_y, 25.0);
    }
}
