use super::*;
use deadsync_input::Keymap;

pub(super) fn build_content(
    cfg: &Config,
    keymap: &Keymap,
    machine_default_noteskin: &str,
    additional_song_folders: &[AdditionalSongFolder],
    never_cache_list: &[String],
    smx_p1_serial: &str,
    smx_p2_serial: &str,
    default_profile_p1: &str,
    default_profile_p2: &str,
) -> String {
    let mut content = String::with_capacity(4096);
    push_saved_options(
        &mut content,
        cfg,
        machine_default_noteskin,
        additional_song_folders,
        never_cache_list,
        smx_p1_serial,
        smx_p2_serial,
        default_profile_p1,
        default_profile_p2,
    );
    deadsync_input::write_keymap_ini_section(&mut content, keymap);
    push_saved_theme(&mut content, cfg);
    content
}

fn push_saved_options(
    content: &mut String,
    cfg: &Config,
    machine_default_noteskin: &str,
    additional_song_folders: &[AdditionalSongFolder],
    never_cache_list: &[String],
    smx_p1_serial: &str,
    smx_p2_serial: &str,
    default_profile_p1: &str,
    default_profile_p2: &str,
) {
    push_section(content, "[Options]");
    push_config_audio_device_lines(content, cfg, cfg.audio_output_mode.as_str());
    push_config_additional_song_folder_lines(content, additional_song_folders);
    push_config_system_download_lines(content, cfg);
    push_config_system_bg_brightness_lines(content, cfg);
    push_config_gameplay_bg_color_line(content, cfg);
    push_config_system_banner_cache_lines(content, cfg);
    push_config_runtime_cache_lines(content, cfg);
    push_config_never_cache_list_line(content, never_cache_list);
    push_config_system_cdtitle_center_lines(content, cfg);
    push_config_system_course_lines(content, cfg);
    push_config_null_or_die_lines(content, cfg);
    push_config_default_noteskin_line(content, machine_default_noteskin);
    push_config_display_size_lines(content, cfg);
    push_config_system_online_lines(content, cfg);
    push_config_runtime_fastload_lines(content, cfg);
    push_config_display_fullscreen_lines(content, cfg);
    push_config_system_input_hardware_lines(content, cfg, true);
    push_config_runtime_state_id_lines(
        content,
        smx_p1_serial,
        smx_p2_serial,
        default_profile_p1,
        default_profile_p2,
    );
    push_config_pad_order_lines(content, deadsync_input_native::pad_order_ini_lines());
    push_config_system_diagnostics_lines(content, cfg);
    push_config_runtime_audio_backend_lines(content, cfg);
    push_config_display_frame_timing_lines(content, cfg);
    push_config_audio_playback_prefix_lines(content, cfg);
    push_config_system_mine_hit_sound_lines(content, cfg);
    push_config_audio_music_lines(content, cfg);
    push_config_select_music_lines(content, cfg);
    push_config_stats_overlay_lines(content, cfg, true);
    push_config_runtime_input_debounce_lines(content, cfg);
    push_config_runtime_navigation_lines(content, cfg);
    push_config_runtime_lights_driver_lines(content, cfg);
    push_config_runtime_lights_lines(content, cfg);
    push_config_runtime_lights_port_lines(content, cfg);
    push_config_runtime_menu_lines(content, cfg);
    push_config_display_monitor_lines(content, cfg);
    push_config_runtime_worker_theme_lines(content, cfg);
    push_config_audio_tail_lines(content, cfg);
    push_config_system_translation_lines(content, cfg);
    push_config_display_video_tail_lines(content, cfg);
    push_config_audio_write_current_screen_lines(content, cfg);
    content.push('\n');
}

fn push_saved_theme(content: &mut String, cfg: &Config) {
    push_section(content, "[Theme]");
    push_config_theme_lines(content, cfg);
    content.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full app-side round trip for the underglow options: the saved ini
    /// content must contain the keys, and parsing that same content back
    /// through the crate loader must return the saved values. Guards the app
    /// glue an option needs beyond the crate (struct field, save call, load
    /// copy), which a missed line silently breaks for one session-persisting
    /// setting (see the SmxUnderglowTheme persistence bug shipped in the
    /// original underglow PR).
    #[test]
    fn saved_content_round_trips_smx_underglow_options() {
        let mut cfg = Config::default();
        cfg.smx_underglow_theme = true;
        cfg.smx_underglow_grb = true;
        let content = build_content(&cfg, &Keymap::default(), "", &[], &[], "", "", "", "");
        assert!(content.contains("SmxUnderglowTheme=1"));
        assert!(content.contains("SmxUnderglowGrb=1"));

        let mut conf = SimpleIni::new();
        conf.load_str(&content);
        let defaults = super::super::system_options(&Config::default());
        assert!(!defaults.smx_underglow_theme && !defaults.smx_underglow_grb);
        let loaded = deadsync_config::options::load_system_options(&conf, defaults);
        assert!(loaded.smx_underglow_theme);
        assert!(loaded.smx_underglow_grb);
    }
}
