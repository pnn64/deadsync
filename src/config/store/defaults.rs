use super::*;

pub(super) fn build_content() -> String {
    let default = Config::default();
    let gameplay_bg_color = default.gameplay_bg_color.to_hex();
    let video_renderer = default.video_renderer.to_string();
    let practice = keycode_to_token(default.music_select_shortcut_practice);
    let song_search = keycode_to_token(default.music_select_shortcut_song_search);
    let load_songs = keycode_to_token(default.music_select_shortcut_load_songs);
    let test_input = keycode_to_token(default.music_select_shortcut_test_input);

    build_default_config_file(
        DefaultConfigFile {
            options: DefaultOptionSection {
                audio: audio_options(&default),
                audio_device: audio_device_options(&default, "Auto"),
                additional_song_folders: &[],
                never_cache_list: &[],
                system: system_options(&default),
                input_hardware: system_input_hardware_options(&default, false),
                display: display_options(
                    &default,
                    default.present_mode_policy.as_str(),
                    video_renderer.as_str(),
                ),
                runtime_io: runtime_io_options(
                    &default,
                    default.linux_audio_backend.as_str(),
                    default.lights_driver.as_str(),
                    default.lights_gameplay_pad_lights.as_str(),
                    default.lights_com_port.as_str(),
                ),
                runtime: runtime_options(&default),
                stats_overlay: stats_overlay_options(&default, None, None),
                select_music: select_music_save_options(&default),
                gameplay_bg_color: gameplay_bg_color.as_str(),
                default_noteskin: DEFAULT_MACHINE_NOTESKIN,
                // No pad->player assignment by default (slots follow the hardware jumper).
                // No default local profiles until the operator or profile select assigns them.
                runtime_state_ids: runtime_state_ids("", "", "", ""),
                // Persisted pad ordering is empty until pads are seen; seeded at runtime.
                pad_order_lines: deadsync_input_native::DEFAULT_PAD_ORDER_INI_LINES,
            },
            keymap: (),
            theme: ThemeSection {
                presentation: theme_presentation_options(&default),
                machine: machine_flow_options(&default),
                shortcuts: theme_shortcut_tokens(
                    practice.as_str(),
                    song_search.as_str(),
                    load_songs.as_str(),
                    test_input.as_str(),
                ),
                null_or_die: Some(null_or_die_options(&default)),
            },
        },
        |content, ()| deadsync_input::write_default_keymap_ini_section(content),
    )
}
