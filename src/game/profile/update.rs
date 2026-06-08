use super::{
    PlayStyle, PlayerSide, ScrollSpeedSetting, lock_profiles, save_profile_ini_for_side,
    save_profile_stats_for_side, session_side_is_guest, side_ix,
};
use chrono::Local;
use deadsync_profile::{
    AccelEffectsMask, AppearanceEffectsMask, AttackMode, BackgroundFilter, ColumnFlashBrightness,
    ColumnFlashMask, ColumnFlashSize, ComboColors, ComboFont, ComboMode, ErrorBarMask,
    ErrorBarTrim, HeldMissGraphic, HideLightType, HoldJudgmentGraphic, HoldsMask, InsertMask,
    JudgmentGraphic, LifeMeterType, LiveTimingStatsMask, MeasureCounter, MeasureLines,
    MiniIndicator, MiniIndicatorColor, MiniIndicatorPosition, MiniIndicatorScoreType,
    MiniIndicatorSize, MiniIndicatorSubtractiveDisplay, NoCmodAlternative, NoteSkin, Perspective,
    Profile, RemoveMask, ScatterplotMaxWindow, ScoreDisplayMode, ScorePosition, ScrollOption,
    StepStatisticsMask, StepStatsExtra, TapExplosionMask, TargetScoreSetting, TimingWindowsOption,
    TurnOption, VisualEffectsMask,
};
use std::path::Path;

fn set_if_changed<T: PartialEq>(value: &mut T, new_value: T) -> bool {
    if *value == new_value {
        return false;
    }
    *value = new_value;
    true
}

fn update_profile_ini(side: PlayerSide, update: impl FnOnce(&mut Profile) -> bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if !update(profile) {
            return;
        }
    }
    save_profile_ini_for_side(side);
}

fn update_profile_stats(side: PlayerSide, update: impl FnOnce(&mut Profile) -> bool) {
    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if !update(profile) {
            return;
        }
    }
    save_profile_stats_for_side(side);
}

pub fn update_last_played_for_side(
    side: PlayerSide,
    style: PlayStyle,
    music_path: Option<&Path>,
    chart_hash: Option<&str>,
    difficulty_index: usize,
) {
    if session_side_is_guest(side) {
        return;
    }
    let new_path = music_path.map(|p| p.to_string_lossy().into_owned());
    let new_hash = chart_hash.map(str::to_string);
    update_profile_ini(side, |profile| {
        profile.set_last_played(style, new_path, new_hash, difficulty_index)
    });
}

pub fn update_last_played_course_for_side(
    side: PlayerSide,
    style: PlayStyle,
    course_path: &Path,
    difficulty_name: Option<&str>,
) {
    if session_side_is_guest(side) {
        return;
    }
    let new_path = Some(course_path.to_string_lossy().into_owned());
    let new_difficulty = difficulty_name.map(str::to_string);
    update_profile_ini(side, |profile| {
        profile.set_last_played_course(style, new_path, new_difficulty)
    });
}

pub fn add_stage_calories_for_side(side: PlayerSide, calories_burned: f32) {
    if session_side_is_guest(side) {
        return;
    }

    let today = Local::now().date_naive().to_string();
    update_profile_ini(side, |profile| {
        profile.add_stage_calories_for_day(&today, calories_burned);
        true
    });
}

pub fn update_player_initials_for_side(side: PlayerSide, initials: &str) {
    if session_side_is_guest(side) {
        return;
    }
    update_profile_ini(side, |profile| profile.set_player_initials(initials));
}

pub fn update_scroll_speed_for_side(side: PlayerSide, setting: ScrollSpeedSetting) {
    // Guest changes should persist for the active session; save_* no-ops for guests.
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.scroll_speed, setting)
    });
}

pub fn update_background_filter_for_side(side: PlayerSide, value: i32) {
    update_profile_ini(side, |profile| {
        set_if_changed(
            &mut profile.background_filter,
            BackgroundFilter::from_i32(value),
        )
    });
}

pub fn update_hold_judgment_graphic_for_side(side: PlayerSide, setting: HoldJudgmentGraphic) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.hold_judgment_graphic, setting)
    });
}

pub fn update_held_miss_graphic_for_side(side: PlayerSide, setting: HeldMissGraphic) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.held_miss_graphic, setting)
    });
}

pub fn update_judgment_graphic_for_side(side: PlayerSide, setting: JudgmentGraphic) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.judgment_graphic, setting)
    });
}

pub fn update_combo_font_for_side(side: PlayerSide, setting: ComboFont) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.combo_font, setting)
    });
}

pub fn update_combo_colors_for_side(side: PlayerSide, setting: ComboColors) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.combo_colors, setting)
    });
}

pub fn update_combo_mode_for_side(side: PlayerSide, setting: ComboMode) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.combo_mode, setting)
    });
}

pub fn update_carry_combo_between_songs_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.carry_combo_between_songs, enabled)
    });
}

pub fn update_current_combo_for_side(side: PlayerSide, combo: u32) {
    update_profile_stats(side, |profile| {
        set_if_changed(&mut profile.current_combo, combo)
    });
}

pub fn update_scroll_option_for_side(side: PlayerSide, setting: ScrollOption) {
    update_profile_ini(side, |profile| profile.set_scroll_option(setting));
}

pub fn update_turn_option_for_side(side: PlayerSide, setting: TurnOption) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.turn_option, setting)
    });
}

pub fn update_insert_mask_for_side(side: PlayerSide, mask: InsertMask) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.insert_active_mask, mask)
    });
}

pub fn update_remove_mask_for_side(side: PlayerSide, mask: RemoveMask) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.remove_active_mask, mask)
    });
}

pub fn update_holds_mask_for_side(side: PlayerSide, mask: HoldsMask) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.holds_active_mask, mask)
    });
}

pub fn update_accel_effects_mask_for_side(side: PlayerSide, mask: AccelEffectsMask) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.accel_effects_active_mask, mask)
    });
}

pub fn update_visual_effects_mask_for_side(side: PlayerSide, mask: VisualEffectsMask) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.visual_effects_active_mask, mask)
    });
}

pub fn update_appearance_effects_mask_for_side(side: PlayerSide, mask: AppearanceEffectsMask) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.appearance_effects_active_mask, mask)
    });
}

pub fn update_attack_mode_for_side(side: PlayerSide, setting: AttackMode) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.attack_mode, setting)
    });
}

pub fn update_hide_light_type_for_side(side: PlayerSide, setting: HideLightType) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.hide_light_type, setting)
    });
}

pub fn update_rescore_early_hits_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.rescore_early_hits, enabled)
    });
}

pub fn update_early_dw_options_for_side(
    side: PlayerSide,
    hide_judgments: bool,
    hide_flash: bool,
    hide_column_flash: bool,
) {
    update_profile_ini(side, |profile| {
        profile.set_early_dw_options(hide_judgments, hide_flash, hide_column_flash)
    });
}

pub fn update_timing_windows_for_side(side: PlayerSide, setting: TimingWindowsOption) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.timing_windows, setting)
    });
}

pub fn update_hide_options_for_side(
    side: PlayerSide,
    hide_targets: bool,
    hide_song_bg: bool,
    hide_combo: bool,
    hide_lifebar: bool,
    hide_score: bool,
    hide_danger: bool,
    hide_combo_explosions: bool,
    hide_username: bool,
) {
    update_profile_ini(side, |profile| {
        profile.set_hide_options(
            hide_targets,
            hide_song_bg,
            hide_combo,
            hide_lifebar,
            hide_score,
            hide_danger,
            hide_combo_explosions,
            hide_username,
        )
    });
}

pub fn update_gameplay_extras_for_side(
    side: PlayerSide,
    column_flash_on_miss: bool,
    subtractive_scoring: bool,
    pacemaker: bool,
    nps_graph_at_top: bool,
) {
    update_profile_ini(side, |profile| {
        profile.set_gameplay_extras(
            column_flash_on_miss,
            subtractive_scoring,
            pacemaker,
            nps_graph_at_top,
        )
    });
}

pub fn update_column_flash_mask_for_side(side: PlayerSide, mask: ColumnFlashMask) {
    update_profile_ini(side, |profile| profile.set_column_flash_mask(mask));
}

pub fn update_column_flash_brightness_for_side(side: PlayerSide, setting: ColumnFlashBrightness) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.column_flash_brightness, setting)
    });
}

pub fn update_column_flash_size_for_side(side: PlayerSide, setting: ColumnFlashSize) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.column_flash_size, setting)
    });
}

pub fn update_transparent_density_graph_bg_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.transparent_density_graph_bg, enabled)
    });
}

pub fn update_mini_indicator_for_side(side: PlayerSide, setting: MiniIndicator) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.mini_indicator, setting)
    });
}

pub fn update_mini_indicator_score_type_for_side(
    side: PlayerSide,
    setting: MiniIndicatorScoreType,
) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.mini_indicator_score_type, setting)
    });
}

pub fn update_mini_indicator_subtractive_display_for_side(
    side: PlayerSide,
    setting: MiniIndicatorSubtractiveDisplay,
) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.mini_indicator_subtractive_display, setting)
    });
}

pub fn update_mini_indicator_size_for_side(side: PlayerSide, setting: MiniIndicatorSize) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.mini_indicator_size, setting)
    });
}

pub fn update_mini_indicator_color_for_side(side: PlayerSide, setting: MiniIndicatorColor) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.mini_indicator_color, setting)
    });
}

pub fn update_mini_indicator_position_for_side(side: PlayerSide, setting: MiniIndicatorPosition) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.mini_indicator_position, setting)
    });
}

pub fn update_noteskin_for_side(side: PlayerSide, setting: NoteSkin) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.noteskin, setting)
    });
}

pub fn update_mine_noteskin_for_side(side: PlayerSide, setting: Option<NoteSkin>) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.mine_noteskin, setting)
    });
}

pub fn update_receptor_noteskin_for_side(side: PlayerSide, setting: Option<NoteSkin>) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.receptor_noteskin, setting)
    });
}

pub fn update_tap_explosion_noteskin_for_side(side: PlayerSide, setting: Option<NoteSkin>) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.tap_explosion_noteskin, setting)
    });
}

pub fn update_tap_explosion_mask_for_side(side: PlayerSide, setting: TapExplosionMask) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.tap_explosion_active_mask, setting)
    });
}

pub fn update_notefield_offset_x_for_side(side: PlayerSide, offset: i32) {
    update_profile_ini(side, |profile| profile.set_note_field_offset_x(offset));
}

pub fn update_notefield_offset_y_for_side(side: PlayerSide, offset: i32) {
    update_profile_ini(side, |profile| profile.set_note_field_offset_y(offset));
}

pub fn update_judgment_offset_x_for_side(side: PlayerSide, offset: i32) {
    update_profile_ini(side, |profile| profile.set_judgment_offset_x(offset));
}

pub fn update_judgment_offset_y_for_side(side: PlayerSide, offset: i32) {
    update_profile_ini(side, |profile| profile.set_judgment_offset_y(offset));
}

pub fn update_combo_offset_x_for_side(side: PlayerSide, offset: i32) {
    update_profile_ini(side, |profile| profile.set_combo_offset_x(offset));
}

pub fn update_combo_offset_y_for_side(side: PlayerSide, offset: i32) {
    update_profile_ini(side, |profile| profile.set_combo_offset_y(offset));
}

pub fn update_error_bar_offset_x_for_side(side: PlayerSide, offset: i32) {
    update_profile_ini(side, |profile| profile.set_error_bar_offset_x(offset));
}

pub fn update_error_bar_offset_y_for_side(side: PlayerSide, offset: i32) {
    update_profile_ini(side, |profile| profile.set_error_bar_offset_y(offset));
}

pub fn update_mini_percent_for_side(side: PlayerSide, percent: i32) {
    update_profile_ini(side, |profile| profile.set_mini_percent(percent));
}

pub fn update_spacing_percent_for_side(side: PlayerSide, percent: i32) {
    update_profile_ini(side, |profile| profile.set_spacing_percent(percent));
}

pub fn update_perspective_for_side(side: PlayerSide, perspective: Perspective) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.perspective, perspective)
    });
}

pub fn update_no_cmod_alternative_for_side(side: PlayerSide, setting: NoCmodAlternative) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.no_cmod_alternative, setting)
    });
}

pub fn update_visual_delay_ms_for_side(side: PlayerSide, ms: i32) {
    update_profile_ini(side, |profile| profile.set_visual_delay_ms(ms));
}

pub fn update_global_offset_shift_ms_for_side(side: PlayerSide, ms: i32) {
    update_profile_ini(side, |profile| profile.set_global_offset_shift_ms(ms));
}

pub fn update_show_fa_plus_window_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.show_fa_plus_window, enabled)
    });
}

pub fn update_show_ex_score_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.show_ex_score, enabled)
    });
}

pub fn update_show_hard_ex_score_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.show_hard_ex_score, enabled)
    });
}

pub fn update_show_fa_plus_pane_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.show_fa_plus_pane, enabled)
    });
}

pub fn update_fa_plus_10ms_blue_window_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.fa_plus_10ms_blue_window, enabled)
    });
}

pub fn update_track_early_judgments_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.track_early_judgments, enabled)
    });
}

pub fn update_scale_scatterplot_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.scale_scatterplot, enabled)
    });
}

pub fn update_split_15_10ms_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.split_15_10ms, enabled)
    });
}

pub fn update_custom_fantastic_window_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.custom_fantastic_window, enabled)
    });
}

pub fn update_custom_fantastic_window_ms_for_side(side: PlayerSide, ms: u8) {
    update_profile_ini(side, |profile| profile.set_custom_fantastic_window_ms(ms));
}

pub fn update_judgment_tilt_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.judgment_tilt, enabled)
    });
}

pub fn update_column_cues_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.column_cues, enabled)
    });
}

pub fn update_judgment_back_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.judgment_back, enabled)
    });
}

pub fn update_error_ms_display_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.error_ms_display, enabled)
    });
}

pub fn update_live_timing_stats_mask_for_side(side: PlayerSide, mask: LiveTimingStatsMask) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.live_timing_stats_mask, mask)
    });
}

pub fn update_live_timing_stats_enabled_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.live_timing_stats, enabled)
    });
}

pub fn update_rainbow_max_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.rainbow_max, enabled)
    });
}

pub fn update_responsive_colors_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.responsive_colors, enabled)
    });
}

pub fn update_show_life_percent_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.show_life_percent, enabled)
    });
}

pub fn update_tilt_multiplier_for_side(side: PlayerSide, multiplier: f32) {
    update_profile_ini(side, |profile| profile.set_tilt_multiplier(multiplier));
}

pub fn update_tilt_thresholds_for_side(side: PlayerSide, min_ms: u32, max_ms: u32) {
    update_profile_ini(side, |profile| profile.set_tilt_thresholds(min_ms, max_ms));
}

pub fn update_error_bar_mask_for_side(side: PlayerSide, mask: ErrorBarMask) {
    update_profile_ini(side, |profile| profile.set_error_bar_mask(mask));
}

pub fn update_error_bar_trim_for_side(side: PlayerSide, setting: ErrorBarTrim) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.error_bar_trim, setting)
    });
}

pub fn update_center_tick_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.center_tick, enabled)
    });
}

pub fn update_text_error_bar_scalable_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.text_error_bar_scalable, enabled)
    });
}

pub fn update_text_error_bar_threshold_ms_for_side(side: PlayerSide, ms: u32) {
    update_profile_ini(side, |profile| profile.set_text_error_bar_threshold_ms(ms));
}

pub fn update_average_error_bar_intensity_for_side(side: PlayerSide, intensity: f32) {
    update_profile_ini(side, |profile| {
        profile.set_average_error_bar_intensity(intensity)
    });
}

pub fn update_short_average_error_bar_enabled_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.short_average_error_bar_enabled, enabled)
    });
}

pub fn update_average_error_bar_interval_ms_for_side(side: PlayerSide, ms: u32) {
    update_profile_ini(side, |profile| {
        profile.set_average_error_bar_interval_ms(ms)
    });
}

pub fn update_long_error_bar_enabled_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.long_error_bar_enabled, enabled)
    });
}

pub fn update_long_error_bar_intensity_for_side(side: PlayerSide, intensity: f32) {
    update_profile_ini(side, |profile| {
        profile.set_long_error_bar_intensity(intensity)
    });
}

pub fn update_long_error_bar_threshold_ms_for_side(side: PlayerSide, ms: u32) {
    update_profile_ini(side, |profile| profile.set_long_error_bar_threshold_ms(ms));
}

pub fn update_long_error_bar_min_samples_for_side(side: PlayerSide, n: u32) {
    update_profile_ini(side, |profile| profile.set_long_error_bar_min_samples(n));
}

pub fn update_step_statistics_for_side(side: PlayerSide, mask: StepStatisticsMask) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.step_statistics, mask)
    });
}

pub fn update_step_stats_extra_for_side(side: PlayerSide, setting: StepStatsExtra) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.step_stats_extra, setting)
    });
}

pub fn update_display_scorebox_for_side(side: PlayerSide, enabled: bool) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.display_scorebox, enabled)
    });
}

pub fn update_scatterplot_max_window_for_side(side: PlayerSide, setting: ScatterplotMaxWindow) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.scatterplot_max_window, setting)
    });
}

pub fn update_score_position_for_side(side: PlayerSide, setting: ScorePosition) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.score_position, setting)
    });
}

pub fn update_score_display_mode_for_side(side: PlayerSide, setting: ScoreDisplayMode) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.score_display_mode, setting)
    });
}

pub fn update_target_score_for_side(side: PlayerSide, setting: TargetScoreSetting) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.target_score, setting)
    });
}

pub fn update_lifemeter_type_for_side(side: PlayerSide, setting: LifeMeterType) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.lifemeter_type, setting)
    });
}

pub fn update_error_bar_options_for_side(side: PlayerSide, up: bool, multi_tick: bool) {
    update_profile_ini(side, |profile| {
        profile.set_error_bar_options(up, multi_tick)
    });
}

pub fn update_measure_counter_for_side(side: PlayerSide, setting: MeasureCounter) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.measure_counter, setting)
    });
}

pub fn update_measure_counter_lookahead_for_side(side: PlayerSide, lookahead: u8) {
    update_profile_ini(side, |profile| {
        profile.set_measure_counter_lookahead(lookahead)
    });
}

pub fn update_measure_counter_options_for_side(
    side: PlayerSide,
    left: bool,
    up: bool,
    vert: bool,
    broken_run: bool,
    run_timer: bool,
) {
    update_profile_ini(side, |profile| {
        profile.set_measure_counter_options(left, up, vert, broken_run, run_timer)
    });
}

pub fn update_measure_lines_for_side(side: PlayerSide, setting: MeasureLines) {
    update_profile_ini(side, |profile| {
        set_if_changed(&mut profile.measure_lines, setting)
    });
}
