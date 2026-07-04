use super::*;
use deadsync_config::cache::never_cache_list_value;
use deadsync_config::folders::additional_song_folder_paths;
use deadsync_config::machine::clamp_smx_light_brightness_percent;
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
    push_line(content, "AdditionalSongFolders", "");
    push_line(
        content,
        "AdditionalSongFoldersWritable",
        additional_song_folder_paths(additional_song_folders, true),
    );
    push_line(
        content,
        "AdditionalSongFoldersReadOnly",
        additional_song_folder_paths(additional_song_folders, false),
    );
    push_config_system_download_lines(content, cfg);
    push_line(
        content,
        "BGBrightness",
        clamp_bg_brightness(cfg.bg_brightness),
    );
    push_line(content, "GameplayBgColor", cfg.gameplay_bg_color.to_hex());
    push_bool(content, "BannerCache", cfg.banner_cache);
    push_config_runtime_cache_lines(content, cfg);
    push_line(
        content,
        "NeverCacheList",
        never_cache_list_value(never_cache_list),
    );
    push_bool(content, "CDTitleCache", cfg.cdtitle_cache);
    push_bool(content, "Center1Player", cfg.center_1player_notefield);
    push_line(
        content,
        "CenterImageTranslateX",
        cfg.center_image_translate_x,
    );
    push_line(
        content,
        "CenterImageTranslateY",
        cfg.center_image_translate_y,
    );
    push_line(content, "CenterImageAddWidth", cfg.center_image_add_width);
    push_line(content, "CenterImageAddHeight", cfg.center_image_add_height);
    push_config_system_course_lines(content, cfg);
    push_config_null_or_die_lines(content, cfg);
    push_line(content, "DefaultNoteSkin", machine_default_noteskin);
    push_line(content, "DisplayHeight", cfg.display_height);
    push_line(content, "DisplayWidth", cfg.display_width);
    push_config_system_online_lines(content, cfg);
    push_config_runtime_fastload_lines(content, cfg);
    push_line(content, "FullscreenType", cfg.fullscreen_type.as_str());
    push_line(content, "Game", cfg.game_flag.as_str());
    push_line(content, "GamepadBackend", cfg.windows_gamepad_backend);
    push_bool(content, "AllowShutdown", cfg.allow_shutdown_host);
    push_bool(content, "SmxInput", cfg.smx_input);
    push_bool(content, "SmxManagesPadConfig", cfg.smx_manages_pad_config);
    push_bool(content, "SmxPanelLights", cfg.smx_panel_lights);
    push_bool(content, "SmxUnderglowTheme", cfg.smx_underglow_theme);
    push_line(
        content,
        "SmxDefaultPadConfig",
        cfg.smx_default_pad_config.as_str(),
    );
    push_line(
        content,
        "SmxDefaultLightBrightness",
        clamp_smx_light_brightness_percent(cfg.smx_default_light_brightness),
    );
    push_line(content, "SmxP1Serial", smx_p1_serial);
    push_line(content, "SmxP2Serial", smx_p2_serial);
    push_line(content, "DefaultLocalProfileIDP1", default_profile_p1);
    push_line(content, "DefaultLocalProfileIDP2", default_profile_p2);
    for (key, value) in deadsync_input_native::pad_order_ini_lines() {
        push_line(content, key, value);
    }
    push_config_system_diagnostics_lines(content, cfg);
    push_line(
        content,
        "LinuxAudioBackend",
        cfg.linux_audio_backend.as_str(),
    );
    push_line(content, "MaxFps", cfg.max_fps);
    push_line(content, "PresentModePolicy", cfg.present_mode_policy);
    push_config_audio_playback_prefix_lines(content, cfg);
    push_bool(content, "MineHitSound", cfg.mine_hit_sound);
    push_config_audio_music_lines(content, cfg);
    push_config_select_music_lines(content, cfg);
    push_config_stats_overlay_lines(content, cfg, true);
    push_line(
        content,
        "InputDebounceTime",
        format!("{:.3}", cfg.input_debounce_seconds),
    );
    push_config_runtime_navigation_lines(content, cfg);
    push_line(content, "LightsDriver", cfg.lights_driver.as_str());
    push_line(
        content,
        "GameplayPadLights",
        cfg.lights_gameplay_pad_lights.as_str(),
    );
    push_config_runtime_lights_lines(content, cfg);
    push_line(content, "LightsComPort", cfg.lights_com_port.as_str());
    push_config_runtime_menu_lines(content, cfg);
    push_line(content, "DisplayMonitor", cfg.display_monitor);
    push_config_runtime_worker_theme_lines(content, cfg);
    push_line(content, "AssistTickVolume", cfg.assist_tick_volume);
    push_line(content, "SFXVolume", cfg.sfx_volume);
    push_bool(content, "TabAcceleration", cfg.tab_acceleration);
    push_bool(content, "TranslatedTitles", cfg.translated_titles);
    push_line(content, "VideoRenderer", cfg.video_renderer);
    push_bool(content, "Vsync", cfg.vsync);
    push_bool(content, "Windowed", cfg.windowed);
    push_bool(content, "WriteCurrentScreen", cfg.write_current_screen);
    content.push('\n');
}

fn push_saved_theme(content: &mut String, cfg: &Config) {
    push_section(content, "[Theme]");
    push_config_theme_lines(content, cfg);
    content.push('\n');
}
