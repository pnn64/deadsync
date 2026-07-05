use super::*;

pub(super) fn build_content() -> String {
    let default = Config::default();
    let mut content = String::with_capacity(4096);
    push_default_options(&mut content, &default);
    deadsync_input::write_default_keymap_ini_section(&mut content);
    push_default_theme(&mut content, &default);
    content
}

fn push_default_options(content: &mut String, default: &Config) {
    push_section(content, "[Options]");
    push_config_audio_device_lines(content, default, "Auto");
    push_config_additional_song_folder_lines(content, &[]);
    push_config_system_download_lines(content, default);
    push_config_system_bg_brightness_lines(content, default);
    push_config_gameplay_bg_color_line(content, default);
    push_config_system_banner_cache_lines(content, default);
    push_config_runtime_cache_lines(content, default);
    push_config_never_cache_list_line(content, &[]);
    push_config_system_cdtitle_center_lines(content, default);
    push_config_system_course_lines(content, default);
    push_config_default_noteskin_line(content, DEFAULT_MACHINE_NOTESKIN);
    push_config_display_size_lines(content, default);
    push_config_display_monitor_lines(content, default);
    push_config_system_online_lines(content, default);
    push_config_runtime_fastload_lines(content, default);
    push_config_display_fullscreen_lines(content, default);
    push_config_system_input_hardware_lines(content, default, false);
    // No pad→player assignment by default (slots follow the hardware jumper).
    // No default local profiles until the operator or profile select assigns them.
    push_config_runtime_state_id_lines(content, "", "", "", "");
    // Persisted pad ordering is empty until pads are seen; seeded at runtime.
    push_config_pad_order_lines(content, deadsync_input_native::DEFAULT_PAD_ORDER_INI_LINES);
    push_config_system_diagnostics_lines(content, default);
    push_config_runtime_audio_backend_lines(content, default);
    push_config_display_frame_timing_lines(content, default);
    push_config_audio_playback_prefix_lines(content, default);
    push_config_system_mine_hit_sound_lines(content, default);
    push_config_audio_music_lines(content, default);
    push_config_select_music_lines(content, default);
    push_config_stats_overlay_lines(content, default, false);
    push_config_runtime_input_debounce_lines(content, default);
    push_config_runtime_navigation_lines(content, default);
    push_config_runtime_lights_driver_lines(content, default);
    push_config_runtime_lights_lines(content, default);
    push_config_runtime_lights_port_lines(content, default);
    push_config_runtime_menu_lines(content, default);
    push_config_runtime_worker_theme_lines(content, default);
    push_config_audio_tail_lines(content, default);
    push_config_system_translation_lines(content, default);
    push_config_display_video_tail_lines(content, default);
    push_config_audio_write_current_screen_lines(content, default);
    content.push('\n');
}

fn push_default_theme(content: &mut String, default: &Config) {
    push_section(content, "[Theme]");
    push_config_theme_lines(content, default);
    push_config_null_or_die_lines(content, default);
    content.push('\n');
}
