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
    let gameplay_bg_color = cfg.gameplay_bg_color.to_hex();
    let video_renderer = cfg.video_renderer.to_string();
    let practice = keycode_to_token(cfg.music_select_shortcut_practice);
    let song_search = keycode_to_token(cfg.music_select_shortcut_song_search);
    let load_songs = keycode_to_token(cfg.music_select_shortcut_load_songs);
    let test_input = keycode_to_token(cfg.music_select_shortcut_test_input);

    build_saved_config_file(
        SavedConfigFile {
            options: SavedOptionSection {
                audio: audio_options(cfg),
                audio_device: audio_device_options(cfg, cfg.audio_output_mode.as_str()),
                additional_song_folders,
                never_cache_list,
                system: system_options(cfg),
                input_hardware: system_input_hardware_options(cfg, true),
                display: display_options(
                    cfg,
                    cfg.present_mode_policy.as_str(),
                    video_renderer.as_str(),
                ),
                runtime_io: runtime_io_options(
                    cfg,
                    cfg.linux_audio_backend.as_str(),
                    cfg.lights_driver.as_str(),
                    cfg.lights_gameplay_pad_lights.as_str(),
                    cfg.lights_com_port.as_str(),
                ),
                runtime: runtime_options(cfg),
                stats_overlay: stats_overlay_options(
                    cfg,
                    Some(cfg.frame_stats_overlay_anchor),
                    Some(cfg.frame_stats_overlay_style),
                ),
                select_music: select_music_save_options(cfg),
                null_or_die: null_or_die_options(cfg),
                gameplay_bg_color: gameplay_bg_color.as_str(),
                default_noteskin: machine_default_noteskin,
                runtime_state_ids: runtime_state_ids(
                    smx_p1_serial,
                    smx_p2_serial,
                    default_profile_p1,
                    default_profile_p2,
                ),
                pad_order_lines: deadsync_input_native::pad_order_ini_lines(),
            },
            keymap,
            theme: ThemeSection {
                presentation: theme_presentation_options(cfg),
                machine: machine_flow_options(cfg),
                shortcuts: theme_shortcut_tokens(
                    practice.as_str(),
                    song_search.as_str(),
                    load_songs.as_str(),
                    test_input.as_str(),
                ),
                null_or_die: None,
            },
        },
        deadsync_input::write_keymap_ini_section,
    )
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
