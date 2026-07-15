mod effect;
mod runtime;
mod screen;
pub mod views;

pub use deadlib_assets::{FontAssetSpec, TextureAssetSpec, texture_asset};
pub use effect::{ThemeEffect, ThemeFlowEvent};
pub use runtime::{
    AudioCut, AudioOutputModeChoice, AudioRequest, AudioVolumeTarget, DisplayModeChoice,
    FullscreenChoice, GraphicsRequest, PlatformRequest, PresentPolicyChoice, RendererChoice,
    RevealPathKind, thread_choice_index, thread_count_from_choice,
};
pub use screen::{Theme, ThemeScreenId};

pub struct ThemeAssetManifest<T> {
    pub fonts: &'static [FontAssetSpec],
    pub textures: T,
    pub texture_needs_repeat_sampler: fn(&str) -> bool,
}

/// Theme-provided metrics and colors for canonical column cue composition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnCueStyle {
    pub top_y: f32,
    pub reverse_anchor_y: f32,
    pub crossover_height_trim: f32,
    pub body_fade: f32,
    pub base_alpha: f32,
    pub normal_color: [f32; 3],
    pub mine_color: [f32; 3],
    pub countdown_normal_y: f32,
    pub countdown_reverse_y: f32,
    pub countdown_color: [f32; 3],
    pub countdown_zoom: f32,
    pub body_z: i16,
    pub countdown_z: i16,
}

/// Theme-provided geometry for one column-flash size mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnFlashLayoutStyle {
    pub top_y: f32,
    pub height_trim: f32,
    pub reverse_trim: f32,
    pub fade: f32,
}

/// Theme-provided metrics and judgment colors for column miss flashes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnFlashStyle {
    pub default_layout: ColumnFlashLayoutStyle,
    pub compact_layout: ColumnFlashLayoutStyle,
    pub reverse_anchor_y: f32,
    pub normal_alpha: f32,
    pub dimmed_alpha: f32,
    pub miss_color: [f32; 3],
    pub decent_color: [f32; 3],
    pub way_off_color: [f32; 3],
    pub great_color: [f32; 3],
    pub excellent_color: [f32; 3],
    pub fantastic_color: [f32; 3],
    pub fantastic_blue_color: [f32; 3],
    pub z: i16,
}

/// Theme-provided presentation values for the canonical measure counter and
/// run timer actors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CounterHudStyle {
    pub text_z: i16,
    pub shadow_len: f32,
    pub base_zoom: f32,
    pub lookahead_zoom_step: f32,
    pub vertical_step_y: f32,
    pub left_column_scale: f32,
    pub horizontal_span: f32,
    pub break_lookahead_color: [f32; 4],
    pub break_current_color: [f32; 4],
    pub stream_lookahead_color: [f32; 4],
    pub ratio_color: [f32; 4],
    pub total_color: [f32; 4],
    pub broken_y_offset: f32,
    pub broken_vertical_y_offset: f32,
    pub broken_vertical_x_scale: f32,
    pub broken_color: [f32; 4],
    pub run_active_color: [f32; 4],
    pub run_inactive_color: [f32; 4],
}

/// Theme-provided placement and failure color for the canonical mini score
/// indicator actor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MiniIndicatorStyle {
    pub column_offset: f32,
    pub under_up_x_offset: f32,
    pub unanchored_x_offset: f32,
    pub failed_color: [f32; 3],
    pub shadow_len: f32,
    pub text_z: i16,
}

/// Theme-provided placement and layering for canonical tap and hold judgment
/// feedback actors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JudgmentFeedbackStyle {
    pub tap_front_z: i16,
    pub tap_back_z: i16,
    pub split_overlay_alpha: f32,
    pub held_miss_normal_y: f32,
    pub held_miss_reverse_y: f32,
    pub held_miss_z: i16,
    pub hold_normal_y: f32,
    pub hold_reverse_y: f32,
    pub hold_z: i16,
    pub hold_initial_zoom: f32,
    pub hold_final_zoom: f32,
}

/// Theme-provided metrics and colors for canonical combo numbers and combo
/// milestone feedback.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComboFeedbackStyle {
    pub threshold: u32,
    pub milestone_z: i16,
    pub number_z: i16,
    pub number_zoom: f32,
    pub shadow_len: f32,
    pub miss_color: [f32; 4],
    pub burst_duration: f32,
    pub burst_start_zoom: f32,
    pub burst_end_zoom: f32,
    pub burst_start_alpha: f32,
    pub burst_rotation_deg: f32,
    pub hundred_start_zoom: f32,
    pub hundred_end_zoom: f32,
    pub hundred_start_alpha: f32,
    pub hundred_start_rotation_deg: f32,
    pub mini_duration: f32,
    pub mini_start_zoom: f32,
    pub mini_end_zoom: f32,
    pub mini_start_alpha: f32,
    pub mini_start_rotation_deg: f32,
    pub thousand_start_zoom: f32,
    pub thousand_end_zoom: f32,
    pub thousand_start_alpha: f32,
    pub thousand_x_travel: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ErrorBarPalette {
    pub fantastic_blue: [f32; 4],
    pub fa_plus_white: [f32; 4],
    pub excellent: [f32; 4],
    pub great: [f32; 4],
    pub decent: [f32; 4],
    pub way_off: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorBarLayers {
    pub background: i16,
    pub band: i16,
    pub line: i16,
    pub tick: i16,
    pub text: i16,
}

/// Theme-provided dimensions, colors, timings, labels, and layers for the
/// canonical error-bar actors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ErrorBarStyle {
    pub colorful_width: f32,
    pub colorful_height: f32,
    pub colorful_border_size: f32,
    pub average_width: f32,
    pub average_height: f32,
    pub average_tick_padding: f32,
    pub monochrome_width: f32,
    pub monochrome_border_size: f32,
    pub monochrome_center_width: f32,
    pub monochrome_line_width: f32,
    pub tick_width: f32,
    pub colorful_tick_duration: f32,
    pub monochrome_tick_duration: f32,
    pub average_tick_extra_height: f32,
    pub monochrome_background_alpha: f32,
    pub line_alpha: f32,
    pub lines_fade_start: f32,
    pub lines_fade_duration: f32,
    pub label_fade_duration: f32,
    pub label_hold: f32,
    pub label_x_ratio: f32,
    pub label_zoom: f32,
    pub center_tick_width: f32,
    pub highlight_inactive_alpha: f32,
    pub offset_indicator_duration: f32,
    pub offset_indicator_gap: f32,
    pub offset_indicator_zoom: f32,
    pub offset_indicator_shadow_len: f32,
    pub long_average_tick_duration: f32,
    pub long_average_tick_extra_height: f32,
    pub long_average_tick_width: f32,
    pub text_duration: f32,
    pub text_x_offset: f32,
    pub text_zoom: f32,
    pub text_shadow_len: f32,
    pub background_color: [f32; 4],
    pub monochrome_center_color: [f32; 4],
    pub monochrome_line_color: [f32; 4],
    pub label_color: [f32; 4],
    pub colorful_tick_color: [f32; 4],
    pub average_center_tick_color: [f32; 4],
    pub long_average_tick_color: [f32; 4],
    pub text_early_color: [f32; 4],
    pub text_late_color: [f32; 4],
    pub text_scaled_early_color: [f32; 4],
    pub text_scaled_late_color: [f32; 4],
    pub palette: ErrorBarPalette,
    pub label_font: &'static str,
    pub offset_indicator_font: &'static str,
    pub text_font: &'static str,
    pub early_label: &'static str,
    pub late_label: &'static str,
    pub front_layers: ErrorBarLayers,
    pub back_layers: ErrorBarLayers,
    pub average_z: i16,
}

/// Theme-selected draw layers consumed by canonical receptor composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceptorStyle {
    pub target_z: i16,
    pub press_glow_z: i16,
    pub hold_explosion_z: i16,
}

/// Theme-selected actor layers and ratios consumed by canonical note-field
/// composition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NotefieldActorStyle {
    pub hold_body_z: i16,
    pub hold_cap_z: i16,
    pub hold_glow_z: i16,
    pub tap_explosion_z: i16,
    pub mine_explosion_z: i16,
    pub note_z: i16,
    pub mine_core_size_ratio: f32,
}

/// Concrete-theme metrics consumed by the canonical notefield layout plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NotefieldStyle {
    pub layout_width_min: f32,
    pub layout_width_max: f32,
    pub side_center_x_ratio: f32,
    pub receptor_normal_y: f32,
    pub receptor_reverse_y: f32,
    pub receptor: ReceptorStyle,
    pub actors: NotefieldActorStyle,
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
    pub column_cue: ColumnCueStyle,
    pub column_flash: ColumnFlashStyle,
    pub counter_hud: CounterHudStyle,
    pub mini_indicator: MiniIndicatorStyle,
    pub judgment_feedback: JudgmentFeedbackStyle,
    pub combo_feedback: ComboFeedbackStyle,
    pub error_bar: ErrorBarStyle,
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
    use super::{
        ColumnCueStyle, ColumnFlashLayoutStyle, ColumnFlashStyle, ComboFeedbackStyle,
        CounterHudStyle, ErrorBarLayers, ErrorBarPalette, ErrorBarStyle, JudgmentFeedbackStyle,
        MiniIndicatorStyle, NotefieldActorStyle, NotefieldStyle, ReceptorStyle,
    };

    #[test]
    fn notefield_style_is_a_plain_metric_contract() {
        let style = NotefieldStyle {
            layout_width_min: 640.0,
            layout_width_max: 854.0,
            side_center_x_ratio: 0.25,
            receptor_normal_y: -125.0,
            receptor_reverse_y: 145.0,
            receptor: ReceptorStyle {
                target_z: 100,
                press_glow_z: 105,
                hold_explosion_z: 145,
            },
            actors: NotefieldActorStyle {
                hold_body_z: 110,
                hold_cap_z: 110,
                hold_glow_z: 111,
                tap_explosion_z: 150,
                mine_explosion_z: 101,
                note_z: 140,
                mine_core_size_ratio: 0.45,
            },
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
                way_off_color: [0.788, 0.522, 0.369],
                great_color: [0.4, 0.788, 0.333],
                excellent_color: [0.886, 0.612, 0.094],
                fantastic_color: [1.0, 1.0, 1.0],
                fantastic_blue_color: [0.129, 0.8, 0.91],
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
                colorful_tick_color: [0.698, 0.0, 0.0, 1.0],
                average_center_tick_color: [1.0, 1.0, 1.0, 0.3],
                long_average_tick_color: [0.0, 0.0, 1.0, 1.0],
                text_early_color: [0.024, 0.416, 0.957, 1.0],
                text_late_color: [1.0, 0.353, 0.306, 1.0],
                text_scaled_early_color: [0.0, 0.318, 0.859, 1.0],
                text_scaled_late_color: [1.0, 0.086, 0.02, 1.0],
                palette: ErrorBarPalette {
                    fantastic_blue: [0.129, 0.8, 0.91, 1.0],
                    fa_plus_white: [1.0, 1.0, 1.0, 1.0],
                    excellent: [0.886, 0.612, 0.094, 1.0],
                    great: [0.4, 0.788, 0.333, 1.0],
                    decent: [0.706, 0.361, 1.0, 1.0],
                    way_off: [0.788, 0.522, 0.369, 1.0],
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

        assert_eq!(style.layout_width_min, 640.0);
        assert_eq!(style.error_bar_offset_y, 25.0);
        assert_eq!(style.actors.tap_explosion_z, 150);
        assert_eq!(style.actors.mine_core_size_ratio, 0.45);
    }
}
