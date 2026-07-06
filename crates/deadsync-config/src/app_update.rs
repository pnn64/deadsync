use crate::app_config::{Config, DisplayMode};
use crate::audio::{clamp_audio_volume_percent, clamp_music_wheel_switch_speed};
use crate::machine::clamp_smx_light_brightness_percent;
use crate::null_or_die::{
    clamp_null_or_die_confidence_percent, clamp_null_or_die_magic_offset_ms,
    clamp_null_or_die_positive_ms,
};
use crate::options::{clamp_bg_brightness, clamp_show_stats_mode, clamped_max_fps};
use crate::theme::{
    ArrowCloudQrLoginWhen, BreakdownStyle, DefaultFailType, DefaultSyncOffset, GameFlag,
    GameplayBpmPosition, GrooveStatsQrLoginWhen, LanguageFlag, LogLevel, MachineBarColor,
    MachineEvaluationStyle, MachineFont, MachinePreferredPlayMode, MachinePreferredPlayStyle,
    NewPackMode, RandomBackgroundMode, SelectMusicItlRankMode, SelectMusicItlWheelMode,
    SelectMusicPatternInfoMode, SelectMusicScoreboxPlacement, SelectMusicSongSelectBgMode,
    SelectMusicStepArtistBoxMode, SelectMusicWheelStyle, SrpgVariant, SyncGraphMode, ThemeFlag,
    VersionOverlaySide, VisualStyle,
};
use deadlib_render::{BackendType, PresentModePolicy};
use deadsync_audio::{AudioOutputMode, LinuxAudioBackend};
use deadsync_input::clamp_input_debounce_seconds;
use deadsync_input_native::WindowsPadBackend;
use deadsync_lights::{DriverKind as LightsDriverKind, GameplayPadLightMode};
use deadsync_smx::SmxPadPreset;
use null_or_die::{BiasKernel, KernelTarget};

pub fn set_display_mode(cfg: &mut Config, mode: DisplayMode) -> bool {
    match mode {
        DisplayMode::Windowed => set_if_changed(&mut cfg.windowed, true),
        DisplayMode::Fullscreen(fullscreen_type) => {
            let windowed = set_if_changed(&mut cfg.windowed, false);
            let fullscreen = set_if_changed(&mut cfg.fullscreen_type, fullscreen_type);
            windowed || fullscreen
        }
    }
}

pub fn set_display_resolution(cfg: &mut Config, width: u32, height: u32) -> bool {
    set_pair_if_changed(
        &mut cfg.display_width,
        width,
        &mut cfg.display_height,
        height,
    )
}

pub fn set_overscan(
    cfg: &mut Config,
    translate_x: i32,
    translate_y: i32,
    add_width: i32,
    add_height: i32,
) -> bool {
    set_quad_if_changed(
        &mut cfg.center_image_translate_x,
        translate_x,
        &mut cfg.center_image_translate_y,
        translate_y,
        &mut cfg.center_image_add_width,
        add_width,
        &mut cfg.center_image_add_height,
        add_height,
    )
}

pub fn set_visual_delay_seconds(cfg: &mut Config, delay: f32) -> bool {
    set_f32_if_changed(&mut cfg.visual_delay_seconds, delay.clamp(-1.0, 1.0))
}

pub fn set_bg_brightness(cfg: &mut Config, brightness: f32) -> bool {
    set_f32_if_changed(&mut cfg.bg_brightness, clamp_bg_brightness(brightness))
}

pub fn set_show_stats_mode(cfg: &mut Config, mode: u8) -> bool {
    set_if_changed(&mut cfg.show_stats_mode, clamp_show_stats_mode(mode))
}

pub fn set_master_volume(cfg: &mut Config, volume: u8) -> bool {
    set_if_changed(&mut cfg.master_volume, clamp_audio_volume_percent(volume))
}

pub fn set_music_volume(cfg: &mut Config, volume: u8) -> bool {
    set_if_changed(&mut cfg.music_volume, clamp_audio_volume_percent(volume))
}

pub fn set_sfx_volume(cfg: &mut Config, volume: u8) -> bool {
    set_if_changed(&mut cfg.sfx_volume, clamp_audio_volume_percent(volume))
}

pub fn set_assist_tick_volume(cfg: &mut Config, volume: u8) -> bool {
    set_if_changed(
        &mut cfg.assist_tick_volume,
        clamp_audio_volume_percent(volume),
    )
}

pub fn set_music_wheel_switch_speed(cfg: &mut Config, speed: u8) -> bool {
    set_if_changed(
        &mut cfg.music_wheel_switch_speed,
        clamp_music_wheel_switch_speed(speed),
    )
}

pub fn set_smx_default_light_brightness(cfg: &mut Config, percent: u8) -> bool {
    set_if_changed(
        &mut cfg.smx_default_light_brightness,
        clamp_smx_light_brightness_percent(percent),
    )
}

pub fn set_input_debounce_seconds(cfg: &mut Config, seconds: f32) -> bool {
    set_f32_if_changed(
        &mut cfg.input_debounce_seconds,
        clamp_input_debounce_seconds(seconds),
    )
}

pub fn set_arcade_options_navigation(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.arcade_options_navigation, enabled)
}

pub fn set_delayed_back(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.delayed_back, enabled)
}

pub fn set_use_fsrs(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.use_fsrs, enabled)
}

pub fn set_smx_input(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.smx_input, enabled)
}

pub fn set_smx_manages_pad_config(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.smx_manages_pad_config, enabled)
}

pub fn set_smx_panel_lights(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.smx_panel_lights, enabled)
}

pub fn set_smx_pad_gifs_pack(cfg: &mut Config, pack: crate::options::SmxPackName) -> bool {
    set_if_changed(&mut cfg.smx_pad_gifs_pack, pack)
}

pub fn set_smx_judge_gifs_pack(cfg: &mut Config, pack: crate::options::SmxPackName) -> bool {
    set_if_changed(&mut cfg.smx_judge_gifs_pack, pack)
}

pub fn set_smx_underglow_theme(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.smx_underglow_theme, enabled)
}

pub fn set_smx_underglow_grb(cfg: &mut Config, grb: bool) -> bool {
    set_if_changed(&mut cfg.smx_underglow_grb, grb)
}

pub fn set_keyboard_features(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.keyboard_features, enabled)
}

pub fn set_machine_font(cfg: &mut Config, font: MachineFont) -> bool {
    set_if_changed(&mut cfg.machine_font, font)
}

pub fn set_machine_bar_color(cfg: &mut Config, color: MachineBarColor) -> bool {
    set_if_changed(&mut cfg.machine_bar_color, color)
}

pub fn set_machine_evaluation_style(cfg: &mut Config, style: MachineEvaluationStyle) -> bool {
    set_if_changed(&mut cfg.machine_evaluation_style, style)
}

pub fn set_select_music_breakdown_style(cfg: &mut Config, style: BreakdownStyle) -> bool {
    set_if_changed(&mut cfg.select_music_breakdown_style, style)
}

pub fn set_version_overlay_side(cfg: &mut Config, side: VersionOverlaySide) -> bool {
    set_if_changed(&mut cfg.version_overlay_side, side)
}

pub fn set_select_music_itl_rank_mode(cfg: &mut Config, mode: SelectMusicItlRankMode) -> bool {
    set_if_changed(&mut cfg.select_music_itl_rank_mode, mode)
}

pub fn set_select_music_itl_wheel_mode(cfg: &mut Config, mode: SelectMusicItlWheelMode) -> bool {
    set_if_changed(&mut cfg.select_music_itl_wheel_mode, mode)
}

pub fn set_select_music_wheel_style(cfg: &mut Config, style: SelectMusicWheelStyle) -> bool {
    set_if_changed(&mut cfg.select_music_wheel_style, style)
}

pub fn set_select_music_song_select_bg_mode(
    cfg: &mut Config,
    mode: SelectMusicSongSelectBgMode,
) -> bool {
    set_if_changed(&mut cfg.select_music_song_select_bg_mode, mode)
}

pub fn set_select_music_new_pack_mode(cfg: &mut Config, mode: NewPackMode) -> bool {
    set_if_changed(&mut cfg.select_music_new_pack_mode, mode)
}

pub fn set_select_music_pattern_info_mode(
    cfg: &mut Config,
    mode: SelectMusicPatternInfoMode,
) -> bool {
    set_if_changed(&mut cfg.select_music_pattern_info_mode, mode)
}

pub fn set_select_music_step_artist_box_mode(
    cfg: &mut Config,
    mode: SelectMusicStepArtistBoxMode,
) -> bool {
    set_if_changed(&mut cfg.select_music_step_artist_box_mode, mode)
}

pub fn set_select_music_scorebox_placement(
    cfg: &mut Config,
    mode: SelectMusicScoreboxPlacement,
) -> bool {
    set_if_changed(&mut cfg.select_music_scorebox_placement, mode)
}

pub fn set_gameplay_bpm_position(cfg: &mut Config, position: GameplayBpmPosition) -> bool {
    set_if_changed(&mut cfg.gameplay_bpm_position, position)
}

pub fn set_default_fail_type(cfg: &mut Config, fail_type: DefaultFailType) -> bool {
    set_if_changed(&mut cfg.default_fail_type, fail_type)
}

pub fn set_visual_style(cfg: &mut Config, style: VisualStyle) -> bool {
    set_if_changed(&mut cfg.visual_style, style)
}

pub fn set_srpg_variant(cfg: &mut Config, variant: SrpgVariant) -> bool {
    set_if_changed(&mut cfg.srpg_variant, variant)
}

pub fn set_random_background_mode(cfg: &mut Config, mode: RandomBackgroundMode) -> bool {
    set_if_changed(&mut cfg.random_background_mode, mode)
}

pub fn set_machine_preferred_style(cfg: &mut Config, style: MachinePreferredPlayStyle) -> bool {
    set_if_changed(&mut cfg.machine_preferred_style, style)
}

pub fn set_machine_preferred_play_mode(cfg: &mut Config, mode: MachinePreferredPlayMode) -> bool {
    set_if_changed(&mut cfg.machine_preferred_play_mode, mode)
}

pub fn set_machine_default_sync_offset(cfg: &mut Config, offset: DefaultSyncOffset) -> bool {
    set_if_changed(&mut cfg.machine_default_sync_offset, offset)
}

pub fn set_arrowcloud_qr_login_when(cfg: &mut Config, when: ArrowCloudQrLoginWhen) -> bool {
    set_if_changed(&mut cfg.arrowcloud_qr_login_when, when)
}

pub fn set_groovestats_qr_login_when(cfg: &mut Config, when: GrooveStatsQrLoginWhen) -> bool {
    set_if_changed(&mut cfg.groovestats_qr_login_when, when)
}

pub fn set_game_flag(cfg: &mut Config, flag: GameFlag) -> bool {
    set_if_changed(&mut cfg.game_flag, flag)
}

pub fn set_theme_flag(cfg: &mut Config, flag: ThemeFlag) -> bool {
    set_if_changed(&mut cfg.theme_flag, flag)
}

pub fn set_language_flag(cfg: &mut Config, flag: LanguageFlag) -> bool {
    set_if_changed(&mut cfg.language_flag, flag)
}

pub fn set_machine_show_select_profile(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.machine_show_select_profile, enabled)
}

pub fn set_allow_switch_profile_in_menu(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.allow_switch_profile_in_menu, enabled)
}

pub fn set_show_video_backgrounds(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_video_backgrounds, enabled)
}

pub fn set_write_current_screen(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.write_current_screen, enabled)
}

pub fn set_machine_show_select_color(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.machine_show_select_color, enabled)
}

pub fn set_machine_show_select_style(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.machine_show_select_style, enabled)
}

pub fn set_machine_show_select_play_mode(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.machine_show_select_play_mode, enabled)
}

pub fn set_machine_show_eval_summary(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.machine_show_eval_summary, enabled)
}

pub fn set_machine_nice_sound(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.machine_nice_sound, enabled)
}

pub fn set_machine_show_name_entry(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.machine_show_name_entry, enabled)
}

pub fn set_machine_show_gameover(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.machine_show_gameover, enabled)
}

pub fn set_machine_enable_replays(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.machine_enable_replays, enabled)
}

pub fn set_machine_allow_per_player_global_offsets(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.machine_allow_per_player_global_offsets, enabled)
}

pub fn set_machine_pack_ini_offsets(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.machine_pack_ini_offsets, enabled)
}

pub fn set_enable_groovestats(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.enable_groovestats, enabled)
}

pub fn set_enable_boogiestats(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.enable_boogiestats, enabled)
}

pub fn set_enable_arrowcloud(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.enable_arrowcloud, enabled)
}

pub fn set_submit_arrowcloud_fails(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.submit_arrowcloud_fails, enabled)
}

pub fn set_auto_download_unlocks(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.auto_download_unlocks, enabled)
}

pub fn set_auto_populate_gs_scores(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.auto_populate_gs_scores, enabled)
}

pub fn set_separate_unlocks_by_player(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.separate_unlocks_by_player, enabled)
}

pub fn set_display_monitor(cfg: &mut Config, monitor: usize) -> bool {
    set_if_changed(&mut cfg.display_monitor, monitor)
}

pub fn set_video_renderer(cfg: &mut Config, renderer: BackendType) -> bool {
    set_if_changed(&mut cfg.video_renderer, renderer)
}

pub fn set_present_mode_policy(cfg: &mut Config, mode: PresentModePolicy) -> bool {
    set_if_changed(&mut cfg.present_mode_policy, mode)
}

pub fn set_windows_gamepad_backend(cfg: &mut Config, backend: WindowsPadBackend) -> bool {
    set_if_changed(&mut cfg.windows_gamepad_backend, backend)
}

pub fn set_smx_default_pad_config(cfg: &mut Config, preset: SmxPadPreset) -> bool {
    set_if_changed(&mut cfg.smx_default_pad_config, preset)
}

pub fn set_audio_output_mode(cfg: &mut Config, mode: AudioOutputMode) -> bool {
    set_if_changed(&mut cfg.audio_output_mode, mode)
}

pub fn set_linux_audio_backend(cfg: &mut Config, backend: LinuxAudioBackend) -> bool {
    set_if_changed(&mut cfg.linux_audio_backend, backend)
}

pub fn set_menu_music(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.menu_music, enabled)
}

pub fn set_software_renderer_threads(cfg: &mut Config, threads: u8) -> bool {
    set_if_changed(&mut cfg.software_renderer_threads, threads)
}

pub fn set_audio_sample_rate(cfg: &mut Config, rate: Option<u32>) -> bool {
    set_if_changed(&mut cfg.audio_sample_rate_hz, rate)
}

pub fn set_audio_output_device(cfg: &mut Config, index: Option<u16>) -> bool {
    set_if_changed(&mut cfg.audio_output_device_index, index)
}

pub fn set_mine_hit_sound(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.mine_hit_sound, enabled)
}

pub fn set_translated_titles(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.translated_titles, enabled)
}

pub fn set_rate_mod_preserves_pitch(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.rate_mod_preserves_pitch, enabled)
}

pub fn set_enable_replaygain(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.enable_replaygain, enabled)
}

pub fn set_lights_driver(cfg: &mut Config, driver: LightsDriverKind) -> bool {
    set_if_changed(&mut cfg.lights_driver, driver)
}

pub fn set_lights_gameplay_pad_lights(cfg: &mut Config, mode: GameplayPadLightMode) -> bool {
    set_if_changed(&mut cfg.lights_gameplay_pad_lights, mode)
}

pub fn set_lights_simplify_bass(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.lights_simplify_bass, enabled)
}

pub fn set_gfx_debug(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.gfx_debug, enabled)
}

pub fn set_high_dpi(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.high_dpi, enabled)
}

pub fn set_hide_mouse_cursor(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.hide_mouse_cursor, enabled)
}

pub fn set_simply_love_color(cfg: &mut Config, index: i32) -> bool {
    set_if_changed(&mut cfg.simply_love_color, index)
}

pub fn set_global_offset_seconds(cfg: &mut Config, offset: f32) -> bool {
    set_f32_if_changed(&mut cfg.global_offset_seconds, offset)
}

pub fn set_vsync(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.vsync, enabled)
}

pub fn set_max_fps(cfg: &mut Config, max_fps: u16) -> bool {
    set_if_changed(&mut cfg.max_fps, clamped_max_fps(max_fps))
}

pub fn set_frame_stats_overlay_anchor(cfg: &mut Config, key: &'static str) -> bool {
    set_if_changed(&mut cfg.frame_stats_overlay_anchor, key)
}

pub fn set_frame_stats_overlay_style(cfg: &mut Config, key: &'static str) -> bool {
    set_if_changed(&mut cfg.frame_stats_overlay_style, key)
}

pub fn set_log_level(cfg: &mut Config, level: LogLevel) -> bool {
    set_if_changed(&mut cfg.log_level, level)
}

pub fn set_log_to_file(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.log_to_file, enabled)
}

pub fn set_center_1player_notefield(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.center_1player_notefield, enabled)
}

pub fn set_banner_cache(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.banner_cache, enabled)
}

pub fn set_cdtitle_cache(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.cdtitle_cache, enabled)
}

pub fn set_song_parsing_threads(cfg: &mut Config, threads: u8) -> bool {
    set_if_changed(&mut cfg.song_parsing_threads, threads)
}

pub fn set_cache_songs(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.cachesongs, enabled)
}

pub fn set_fastload(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.fastload, enabled)
}

pub fn set_show_select_music_breakdown(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_select_music_breakdown, enabled)
}

pub fn set_show_select_music_banners(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_select_music_banners, enabled)
}

pub fn set_show_version_overlay(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_version_overlay, enabled)
}

pub fn set_show_select_music_video_banners(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_select_music_video_banners, enabled)
}

pub fn set_show_select_music_cdtitles(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_select_music_cdtitles, enabled)
}

pub fn set_show_music_wheel_grades(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_music_wheel_grades, enabled)
}

pub fn set_show_music_wheel_lamps(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_music_wheel_lamps, enabled)
}

pub fn set_show_select_music_folder_stats(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_select_music_folder_stats, enabled)
}

pub fn set_show_select_music_previews(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_select_music_previews, enabled)
}

pub fn set_show_select_music_preview_marker(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_select_music_preview_marker, enabled)
}

pub fn set_select_music_preview_loop(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.select_music_preview_loop, enabled)
}

pub fn set_show_select_music_gameplay_timer(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_select_music_gameplay_timer, enabled)
}

pub fn set_show_select_music_stage_display(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_select_music_stage_display, enabled)
}

pub fn set_show_select_music_scorebox(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_select_music_scorebox, enabled)
}

pub fn set_select_music_scorebox_cycle_itg(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.select_music_scorebox_cycle_itg, enabled)
}

pub fn set_select_music_scorebox_cycle_ex(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.select_music_scorebox_cycle_ex, enabled)
}

pub fn set_select_music_scorebox_cycle_hard_ex(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.select_music_scorebox_cycle_hard_ex, enabled)
}

pub fn set_select_music_scorebox_cycle_tournaments(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.select_music_scorebox_cycle_tournaments, enabled)
}

pub fn set_select_music_chart_info_peak_nps(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.select_music_chart_info_peak_nps, enabled)
}

pub fn set_select_music_chart_info_effective_bpm(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.select_music_chart_info_effective_bpm, enabled)
}

pub fn set_select_music_chart_info_matrix_rating(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.select_music_chart_info_matrix_rating, enabled)
}

pub fn set_auto_screenshot_eval(cfg: &mut Config, mask: u8) -> bool {
    set_if_changed(&mut cfg.auto_screenshot_eval, mask)
}

pub fn set_show_random_courses(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_random_courses, enabled)
}

pub fn set_show_most_played_courses(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_most_played_courses, enabled)
}

pub fn set_show_course_individual_scores(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_course_individual_scores, enabled)
}

pub fn set_autosubmit_course_scores_individually(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.autosubmit_course_scores_individually, enabled)
}

pub fn set_zmod_rating_box_text(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.zmod_rating_box_text, enabled)
}

pub fn set_show_bpm_decimal(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.show_bpm_decimal, enabled)
}

pub fn set_null_or_die_sync_graph(cfg: &mut Config, mode: SyncGraphMode) -> bool {
    set_if_changed(&mut cfg.null_or_die_sync_graph, mode)
}

pub fn set_null_or_die_confidence_percent(cfg: &mut Config, value: u8) -> bool {
    set_if_changed(
        &mut cfg.null_or_die_confidence_percent,
        clamp_null_or_die_confidence_percent(value),
    )
}

pub fn set_null_or_die_pack_sync_threads(cfg: &mut Config, threads: u8) -> bool {
    set_if_changed(&mut cfg.null_or_die_pack_sync_threads, threads)
}

pub fn set_null_or_die_fingerprint_ms(cfg: &mut Config, value: f64) -> bool {
    set_f64_if_changed(
        &mut cfg.null_or_die_fingerprint_ms,
        clamp_null_or_die_positive_ms(value),
    )
}

pub fn set_null_or_die_window_ms(cfg: &mut Config, value: f64) -> bool {
    set_f64_if_changed(
        &mut cfg.null_or_die_window_ms,
        clamp_null_or_die_positive_ms(value),
    )
}

pub fn set_null_or_die_step_ms(cfg: &mut Config, value: f64) -> bool {
    set_f64_if_changed(
        &mut cfg.null_or_die_step_ms,
        clamp_null_or_die_positive_ms(value),
    )
}

pub fn set_null_or_die_magic_offset_ms(cfg: &mut Config, value: f64) -> bool {
    set_f64_if_changed(
        &mut cfg.null_or_die_magic_offset_ms,
        clamp_null_or_die_magic_offset_ms(value),
    )
}

pub fn set_null_or_die_kernel_target(cfg: &mut Config, value: KernelTarget) -> bool {
    set_if_changed(&mut cfg.null_or_die_kernel_target, value)
}

pub fn set_null_or_die_kernel_type(cfg: &mut Config, value: BiasKernel) -> bool {
    set_if_changed(&mut cfg.null_or_die_kernel_type, value)
}

pub fn set_null_or_die_full_spectrogram(cfg: &mut Config, enabled: bool) -> bool {
    set_if_changed(&mut cfg.null_or_die_full_spectrogram, enabled)
}

pub fn set_if_changed<T>(slot: &mut T, value: T) -> bool
where
    T: PartialEq,
{
    if *slot == value {
        false
    } else {
        *slot = value;
        true
    }
}

pub fn set_pair_if_changed<T>(a: &mut T, a_value: T, b: &mut T, b_value: T) -> bool
where
    T: PartialEq,
{
    let mut changed = set_if_changed(a, a_value);
    changed |= set_if_changed(b, b_value);
    changed
}

pub fn set_quad_if_changed<T>(
    a: &mut T,
    a_value: T,
    b: &mut T,
    b_value: T,
    c: &mut T,
    c_value: T,
    d: &mut T,
    d_value: T,
) -> bool
where
    T: PartialEq,
{
    let mut changed = set_if_changed(a, a_value);
    changed |= set_if_changed(b, b_value);
    changed |= set_if_changed(c, c_value);
    changed |= set_if_changed(d, d_value);
    changed
}

pub fn set_f32_if_changed(slot: &mut f32, value: f32) -> bool {
    if (*slot - value).abs() <= f32::EPSILON {
        false
    } else {
        *slot = value;
        true
    }
}

pub fn set_f64_if_changed(slot: &mut f64, value: f64) -> bool {
    if (*slot - value).abs() <= f64::EPSILON {
        false
    } else {
        *slot = value;
        true
    }
}
