use super::{
    PlayStyle, PlayerSide, ScrollSpeedSetting, save_profile_ini_for_side,
    save_profile_stats_for_side,
};
use deadsync_profile::update::{self as profile_update, ProfileUpdatePersistence};
use deadsync_profile::{
    AccelEffectsMask, AppearanceEffectsMask, AttackMode, ColumnFlashBrightness, ColumnFlashMask,
    ColumnFlashSize, ComboColors, ComboFont, ComboMode, ErrorBarMask, ErrorBarTrim,
    HeldMissGraphic, HideLightType, HoldJudgmentGraphic, HoldsMask, InsertMask, JudgmentGraphic,
    LifeMeterType, LiveTimingStatsMask, MeasureCounter, MeasureLines, MiniIndicator,
    MiniIndicatorColor, MiniIndicatorPosition, MiniIndicatorScoreType, MiniIndicatorSize,
    MiniIndicatorSubtractiveDisplay, NoCmodAlternative, NoteSkin, Perspective, RemoveMask,
    ScatterplotMaxWindow, ScoreDisplayMode, ScorePosition, ScrollOption, StepStatisticsMask,
    StepStatsExtra, TapExplosionMask, TargetScoreSetting, TimingWindowsOption, TurnOption,
    VisualEffectsMask,
};
use std::path::Path;

fn persist_profile_update(side: PlayerSide, persistence: ProfileUpdatePersistence) {
    match persistence {
        ProfileUpdatePersistence::None => {}
        ProfileUpdatePersistence::Ini => save_profile_ini_for_side(side),
        ProfileUpdatePersistence::Stats => save_profile_stats_for_side(side),
    }
}

macro_rules! profile_update_wrappers {
    ($(pub fn $name:ident($side:ident: PlayerSide $(, $arg:ident: $ty:ty)*);)*) => {
        $(
            pub fn $name($side: PlayerSide $(, $arg: $ty)*) {
                persist_profile_update($side, profile_update::$name($side $(, $arg)*));
            }
        )*
    };
}

profile_update_wrappers! {
    pub fn update_last_played_for_side(side: PlayerSide, style: PlayStyle, music_path: Option<&Path>, chart_hash: Option<&str>, difficulty_index: usize);
    pub fn update_last_played_course_for_side(side: PlayerSide, style: PlayStyle, course_path: &Path, difficulty_name: Option<&str>);
    pub fn add_stage_calories_for_side(side: PlayerSide, calories_burned: f32);
    pub fn update_player_initials_for_side(side: PlayerSide, initials: &str);
    pub fn update_scroll_speed_for_side(side: PlayerSide, setting: ScrollSpeedSetting);
    pub fn update_background_filter_for_side(side: PlayerSide, value: i32);
    pub fn update_hold_judgment_graphic_for_side(side: PlayerSide, setting: HoldJudgmentGraphic);
    pub fn update_held_miss_graphic_for_side(side: PlayerSide, setting: HeldMissGraphic);
    pub fn update_judgment_graphic_for_side(side: PlayerSide, setting: JudgmentGraphic);
    pub fn update_combo_font_for_side(side: PlayerSide, setting: ComboFont);
    pub fn update_combo_colors_for_side(side: PlayerSide, setting: ComboColors);
    pub fn update_combo_mode_for_side(side: PlayerSide, setting: ComboMode);
    pub fn update_carry_combo_between_songs_for_side(side: PlayerSide, enabled: bool);
    pub fn update_current_combo_for_side(side: PlayerSide, combo: u32);
    pub fn update_scroll_option_for_side(side: PlayerSide, setting: ScrollOption);
    pub fn update_turn_option_for_side(side: PlayerSide, setting: TurnOption);
    pub fn update_insert_mask_for_side(side: PlayerSide, mask: InsertMask);
    pub fn update_remove_mask_for_side(side: PlayerSide, mask: RemoveMask);
    pub fn update_holds_mask_for_side(side: PlayerSide, mask: HoldsMask);
    pub fn update_accel_effects_mask_for_side(side: PlayerSide, mask: AccelEffectsMask);
    pub fn update_visual_effects_mask_for_side(side: PlayerSide, mask: VisualEffectsMask);
    pub fn update_appearance_effects_mask_for_side(side: PlayerSide, mask: AppearanceEffectsMask);
    pub fn update_attack_mode_for_side(side: PlayerSide, setting: AttackMode);
    pub fn update_hide_light_type_for_side(side: PlayerSide, setting: HideLightType);
    pub fn update_rescore_early_hits_for_side(side: PlayerSide, enabled: bool);
    pub fn update_early_dw_options_for_side(side: PlayerSide, hide_judgments: bool, hide_flash: bool, hide_column_flash: bool);
    pub fn update_timing_windows_for_side(side: PlayerSide, setting: TimingWindowsOption);
    pub fn update_hide_options_for_side(side: PlayerSide, hide_targets: bool, hide_song_bg: bool, hide_combo: bool, hide_lifebar: bool, hide_score: bool, hide_danger: bool, hide_combo_explosions: bool, hide_username: bool);
    pub fn update_gameplay_extras_for_side(side: PlayerSide, column_flash_on_miss: bool, subtractive_scoring: bool, pacemaker: bool, nps_graph_at_top: bool);
    pub fn update_column_flash_mask_for_side(side: PlayerSide, mask: ColumnFlashMask);
    pub fn update_column_flash_brightness_for_side(side: PlayerSide, setting: ColumnFlashBrightness);
    pub fn update_column_flash_size_for_side(side: PlayerSide, setting: ColumnFlashSize);
    pub fn update_transparent_density_graph_bg_for_side(side: PlayerSide, enabled: bool);
    pub fn update_smx_fsr_display_for_side(side: PlayerSide, enabled: bool);
    pub fn update_smx_pad_input_display_for_side(side: PlayerSide, enabled: bool);
    pub fn update_smx_bg_pack_for_side(side: PlayerSide, pack: &str);
    pub fn update_smx_judge_pack_for_side(side: PlayerSide, pack: &str);
    pub fn update_mini_indicator_for_side(side: PlayerSide, setting: MiniIndicator);
    pub fn update_mini_indicator_score_type_for_side(side: PlayerSide, setting: MiniIndicatorScoreType);
    pub fn update_mini_indicator_subtractive_display_for_side(side: PlayerSide, setting: MiniIndicatorSubtractiveDisplay);
    pub fn update_mini_indicator_size_for_side(side: PlayerSide, setting: MiniIndicatorSize);
    pub fn update_mini_indicator_color_for_side(side: PlayerSide, setting: MiniIndicatorColor);
    pub fn update_mini_indicator_position_for_side(side: PlayerSide, setting: MiniIndicatorPosition);
    pub fn update_noteskin_for_side(side: PlayerSide, setting: NoteSkin);
    pub fn update_mine_noteskin_for_side(side: PlayerSide, setting: Option<NoteSkin>);
    pub fn update_receptor_noteskin_for_side(side: PlayerSide, setting: Option<NoteSkin>);
    pub fn update_tap_explosion_noteskin_for_side(side: PlayerSide, setting: Option<NoteSkin>);
    pub fn update_tap_explosion_mask_for_side(side: PlayerSide, setting: TapExplosionMask);
    pub fn update_notefield_offset_x_for_side(side: PlayerSide, offset: i32);
    pub fn update_notefield_offset_y_for_side(side: PlayerSide, offset: i32);
    pub fn update_judgment_offset_x_for_side(side: PlayerSide, offset: i32);
    pub fn update_judgment_offset_y_for_side(side: PlayerSide, offset: i32);
    pub fn update_combo_offset_x_for_side(side: PlayerSide, offset: i32);
    pub fn update_combo_offset_y_for_side(side: PlayerSide, offset: i32);
    pub fn update_error_bar_offset_x_for_side(side: PlayerSide, offset: i32);
    pub fn update_error_bar_offset_y_for_side(side: PlayerSide, offset: i32);
    pub fn update_mini_percent_for_side(side: PlayerSide, percent: i32);
    pub fn update_spacing_percent_for_side(side: PlayerSide, percent: i32);
    pub fn update_perspective_for_side(side: PlayerSide, perspective: Perspective);
    pub fn update_no_cmod_alternative_for_side(side: PlayerSide, setting: NoCmodAlternative);
    pub fn update_visual_delay_ms_for_side(side: PlayerSide, ms: i32);
    pub fn update_global_offset_shift_ms_for_side(side: PlayerSide, ms: i32);
    pub fn update_show_fa_plus_window_for_side(side: PlayerSide, enabled: bool);
    pub fn update_show_ex_score_for_side(side: PlayerSide, enabled: bool);
    pub fn update_show_hard_ex_score_for_side(side: PlayerSide, enabled: bool);
    pub fn update_show_fa_plus_pane_for_side(side: PlayerSide, enabled: bool);
    pub fn update_fa_plus_10ms_blue_window_for_side(side: PlayerSide, enabled: bool);
    pub fn update_track_early_judgments_for_side(side: PlayerSide, enabled: bool);
    pub fn update_scale_scatterplot_for_side(side: PlayerSide, enabled: bool);
    pub fn update_split_15_10ms_for_side(side: PlayerSide, enabled: bool);
    pub fn update_custom_fantastic_window_for_side(side: PlayerSide, enabled: bool);
    pub fn update_custom_fantastic_window_ms_for_side(side: PlayerSide, ms: u8);
    pub fn update_pad_light_brightness_for_side(side: PlayerSide, value: i32);
    pub fn update_judgment_tilt_for_side(side: PlayerSide, enabled: bool);
    pub fn update_column_cues_for_side(side: PlayerSide, enabled: bool);
    pub fn update_measure_cues_for_side(side: PlayerSide, enabled: bool);
    pub fn update_crossover_cues_for_side(side: PlayerSide, enabled: bool);
    pub fn update_crossover_cue_brackets_for_side(side: PlayerSide, enabled: bool);
    pub fn update_crossover_cue_duration_ms_for_side(side: PlayerSide, ms: u16);
    pub fn update_crossover_cue_quantization_for_side(side: PlayerSide, quantization: u8);
    pub fn update_column_countdown_for_side(side: PlayerSide, enabled: bool);
    pub fn update_judgment_back_for_side(side: PlayerSide, enabled: bool);
    pub fn update_error_ms_display_for_side(side: PlayerSide, enabled: bool);
    pub fn update_live_timing_stats_mask_for_side(side: PlayerSide, mask: LiveTimingStatsMask);
    pub fn update_live_timing_stats_enabled_for_side(side: PlayerSide, enabled: bool);
    pub fn update_rainbow_max_for_side(side: PlayerSide, enabled: bool);
    pub fn update_responsive_colors_for_side(side: PlayerSide, enabled: bool);
    pub fn update_show_life_percent_for_side(side: PlayerSide, enabled: bool);
    pub fn update_tilt_multiplier_for_side(side: PlayerSide, multiplier: f32);
    pub fn update_tilt_thresholds_for_side(side: PlayerSide, min_ms: u32, max_ms: u32);
    pub fn update_error_bar_mask_for_side(side: PlayerSide, mask: ErrorBarMask);
    pub fn update_error_bar_trim_for_side(side: PlayerSide, setting: ErrorBarTrim);
    pub fn update_center_tick_for_side(side: PlayerSide, enabled: bool);
    pub fn update_text_error_bar_scalable_for_side(side: PlayerSide, enabled: bool);
    pub fn update_text_error_bar_threshold_ms_for_side(side: PlayerSide, ms: u32);
    pub fn update_average_error_bar_intensity_for_side(side: PlayerSide, intensity: f32);
    pub fn update_short_average_error_bar_enabled_for_side(side: PlayerSide, enabled: bool);
    pub fn update_average_error_bar_interval_ms_for_side(side: PlayerSide, ms: u32);
    pub fn update_long_error_bar_enabled_for_side(side: PlayerSide, enabled: bool);
    pub fn update_long_error_bar_intensity_for_side(side: PlayerSide, intensity: f32);
    pub fn update_long_error_bar_threshold_ms_for_side(side: PlayerSide, ms: u32);
    pub fn update_long_error_bar_min_samples_for_side(side: PlayerSide, n: u32);
    pub fn update_step_statistics_for_side(side: PlayerSide, mask: StepStatisticsMask);
    pub fn update_step_stats_extra_for_side(side: PlayerSide, setting: StepStatsExtra);
    pub fn update_display_scorebox_for_side(side: PlayerSide, enabled: bool);
    pub fn update_scatterplot_max_window_for_side(side: PlayerSide, setting: ScatterplotMaxWindow);
    pub fn update_score_position_for_side(side: PlayerSide, setting: ScorePosition);
    pub fn update_score_display_mode_for_side(side: PlayerSide, setting: ScoreDisplayMode);
    pub fn update_target_score_for_side(side: PlayerSide, setting: TargetScoreSetting);
    pub fn update_lifemeter_type_for_side(side: PlayerSide, setting: LifeMeterType);
    pub fn update_error_bar_options_for_side(side: PlayerSide, up: bool, multi_tick: bool);
    pub fn update_measure_counter_for_side(side: PlayerSide, setting: MeasureCounter);
    pub fn update_measure_counter_lookahead_for_side(side: PlayerSide, lookahead: u8);
    pub fn update_measure_counter_options_for_side(side: PlayerSide, left: bool, up: bool, vert: bool, broken_run: bool, run_timer: bool);
    pub fn update_measure_lines_for_side(side: PlayerSide, setting: MeasureLines);
}
