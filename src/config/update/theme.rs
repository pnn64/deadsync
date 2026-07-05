use super::*;

pub fn update_machine_font(font: MachineFont) {
    update_config_value(font, |cfg| &mut cfg.machine_font);
}

pub fn update_machine_bar_color(color: MachineBarColor) {
    update_config_value(color, |cfg| &mut cfg.machine_bar_color);
}

pub fn update_machine_evaluation_style(style: MachineEvaluationStyle) {
    update_config_value(style, |cfg| &mut cfg.machine_evaluation_style);
}

pub fn update_select_music_breakdown_style(style: BreakdownStyle) {
    update_config_value(style, |cfg| &mut cfg.select_music_breakdown_style);
}

pub fn update_show_select_music_breakdown(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_select_music_breakdown);
}

pub fn update_show_select_music_banners(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_select_music_banners);
}

pub fn update_show_version_overlay(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_version_overlay);
}

pub fn update_version_overlay_side(side: VersionOverlaySide) {
    update_config_value(side, |cfg| &mut cfg.version_overlay_side);
}

pub fn update_show_select_music_video_banners(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_select_music_video_banners);
}

pub fn update_show_select_music_cdtitles(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_select_music_cdtitles);
}

pub fn update_show_music_wheel_grades(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_music_wheel_grades);
}

pub fn update_show_music_wheel_lamps(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_music_wheel_lamps);
}

pub fn update_select_music_itl_rank_mode(mode: SelectMusicItlRankMode) {
    update_config_value(mode, |cfg| &mut cfg.select_music_itl_rank_mode);
}

pub fn update_select_music_itl_wheel_mode(mode: SelectMusicItlWheelMode) {
    update_config_value(mode, |cfg| &mut cfg.select_music_itl_wheel_mode);
}

pub fn update_select_music_wheel_style(style: SelectMusicWheelStyle) {
    update_config_value(style, |cfg| &mut cfg.select_music_wheel_style);
}

pub fn update_select_music_song_select_bg_mode(mode: SelectMusicSongSelectBgMode) {
    update_config_value(mode, |cfg| &mut cfg.select_music_song_select_bg_mode);
}

pub fn update_select_music_new_pack_mode(mode: NewPackMode) {
    update_config_value(mode, |cfg| &mut cfg.select_music_new_pack_mode);
}

pub fn update_show_select_music_folder_stats(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_select_music_folder_stats);
}

pub fn update_show_select_music_previews(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_select_music_previews);
}

pub fn update_show_select_music_preview_marker(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_select_music_preview_marker);
}

pub fn update_select_music_preview_loop(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.select_music_preview_loop);
}

pub fn update_select_music_pattern_info_mode(mode: SelectMusicPatternInfoMode) {
    update_config_value(mode, |cfg| &mut cfg.select_music_pattern_info_mode);
}

pub fn update_select_music_step_artist_box_mode(mode: SelectMusicStepArtistBoxMode) {
    update_config_value(mode, |cfg| &mut cfg.select_music_step_artist_box_mode);
}

pub fn update_show_select_music_gameplay_timer(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_select_music_gameplay_timer);
}

pub fn update_show_select_music_stage_display(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_select_music_stage_display);
}

pub fn update_show_select_music_scorebox(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_select_music_scorebox);
}

pub fn update_select_music_scorebox_placement(mode: SelectMusicScoreboxPlacement) {
    update_config_value(mode, |cfg| &mut cfg.select_music_scorebox_placement);
}

pub fn update_select_music_scorebox_cycle_itg(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.select_music_scorebox_cycle_itg);
}

pub fn update_select_music_scorebox_cycle_ex(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.select_music_scorebox_cycle_ex);
}

pub fn update_select_music_scorebox_cycle_hard_ex(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.select_music_scorebox_cycle_hard_ex);
}

pub fn update_select_music_scorebox_cycle_tournaments(enabled: bool) {
    update_config_value(enabled, |cfg| {
        &mut cfg.select_music_scorebox_cycle_tournaments
    });
}

pub fn update_select_music_chart_info_peak_nps(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.select_music_chart_info_peak_nps);
}

pub fn update_select_music_chart_info_effective_bpm(enabled: bool) {
    update_config_value(enabled, |cfg| {
        &mut cfg.select_music_chart_info_effective_bpm
    });
}

pub fn update_select_music_chart_info_matrix_rating(enabled: bool) {
    update_config_value(enabled, |cfg| {
        &mut cfg.select_music_chart_info_matrix_rating
    });
}

pub fn update_auto_screenshot_eval(mask: u8) {
    update_config_value(mask, |cfg| &mut cfg.auto_screenshot_eval);
}

pub fn update_show_random_courses(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_random_courses);
}

pub fn update_show_most_played_courses(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_most_played_courses);
}

pub fn update_show_course_individual_scores(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_course_individual_scores);
}

pub fn update_autosubmit_course_scores_individually(enabled: bool) {
    update_config_value(enabled, |cfg| {
        &mut cfg.autosubmit_course_scores_individually
    });
}

pub fn update_zmod_rating_box_text(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.zmod_rating_box_text);
}

pub fn update_show_bpm_decimal(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.show_bpm_decimal);
}

pub fn update_gameplay_bpm_position(position: GameplayBpmPosition) {
    update_config_value(position, |cfg| &mut cfg.gameplay_bpm_position);
}

pub fn update_default_fail_type(fail_type: DefaultFailType) {
    update_config_value(fail_type, |cfg| &mut cfg.default_fail_type);
}
