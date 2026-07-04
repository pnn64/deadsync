use super::*;
use deadsync_config::machine::clamp_smx_light_brightness_percent;

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
    push_line(content, "AdditionalSongFolders", "");
    push_line(content, "AdditionalSongFoldersWritable", "");
    push_line(content, "AdditionalSongFoldersReadOnly", "");
    push_config_system_download_lines(content, default);
    push_line(content, "BGBrightness", default.bg_brightness);
    push_line(
        content,
        "GameplayBgColor",
        default.gameplay_bg_color.to_hex(),
    );
    push_bool(content, "BannerCache", default.banner_cache);
    push_config_runtime_cache_lines(content, default);
    push_line(content, "NeverCacheList", "");
    push_bool(content, "CDTitleCache", default.cdtitle_cache);
    push_bool(content, "Center1Player", default.center_1player_notefield);
    push_line(
        content,
        "CenterImageTranslateX",
        default.center_image_translate_x,
    );
    push_line(
        content,
        "CenterImageTranslateY",
        default.center_image_translate_y,
    );
    push_line(
        content,
        "CenterImageAddWidth",
        default.center_image_add_width,
    );
    push_line(
        content,
        "CenterImageAddHeight",
        default.center_image_add_height,
    );
    push_config_system_course_lines(content, default);
    push_line(content, "DefaultNoteSkin", DEFAULT_MACHINE_NOTESKIN);
    push_line(content, "DisplayHeight", default.display_height);
    push_line(content, "DisplayWidth", default.display_width);
    push_line(content, "DisplayMonitor", default.display_monitor);
    push_config_system_online_lines(content, default);
    push_config_runtime_fastload_lines(content, default);
    push_line(content, "FullscreenType", default.fullscreen_type.as_str());
    push_line(content, "Game", default.game_flag.as_str());
    push_line(content, "GamepadBackend", default.windows_gamepad_backend);
    push_bool(content, "AllowShutdown", default.allow_shutdown_host);
    push_bool(content, "SmxInput", default.smx_input);
    push_bool(
        content,
        "SmxManagesPadConfig",
        default.smx_manages_pad_config,
    );
    push_bool(content, "SmxPanelLights", default.smx_panel_lights);
    push_line(
        content,
        "SmxDefaultPadConfig",
        default.smx_default_pad_config.as_str(),
    );
    push_line(
        content,
        "SmxDefaultLightBrightness",
        clamp_smx_light_brightness_percent(default.smx_default_light_brightness),
    );
    // No pad→player assignment by default (slots follow the hardware jumper).
    push_line(content, "SmxP1Serial", "");
    push_line(content, "SmxP2Serial", "");
    // No default local profiles until the operator or profile select assigns them.
    push_line(content, "DefaultLocalProfileIDP1", "");
    push_line(content, "DefaultLocalProfileIDP2", "");
    // Persisted pad ordering is empty until pads are seen; seeded at runtime.
    for (key, value) in deadsync_input_native::DEFAULT_PAD_ORDER_INI_LINES {
        push_line(content, key, value);
    }
    push_config_system_diagnostics_lines(content, default);
    push_line(
        content,
        "LinuxAudioBackend",
        default.linux_audio_backend.as_str(),
    );
    push_line(content, "MaxFps", default.max_fps);
    push_line(content, "PresentModePolicy", default.present_mode_policy);
    push_config_audio_playback_prefix_lines(content, default);
    push_bool(content, "MineHitSound", default.mine_hit_sound);
    push_config_audio_music_lines(content, default);
    push_config_select_music_lines(content, default);
    push_config_stats_overlay_lines(content, default, false);
    push_line(
        content,
        "InputDebounceTime",
        format!("{:.3}", default.input_debounce_seconds),
    );
    push_config_runtime_navigation_lines(content, default);
    push_line(content, "LightsDriver", default.lights_driver.as_str());
    push_line(
        content,
        "GameplayPadLights",
        default.lights_gameplay_pad_lights.as_str(),
    );
    push_config_runtime_lights_lines(content, default);
    push_line(content, "LightsComPort", default.lights_com_port.as_str());
    push_config_runtime_menu_lines(content, default);
    push_config_runtime_worker_theme_lines(content, default);
    push_line(content, "AssistTickVolume", default.assist_tick_volume);
    push_line(content, "SFXVolume", default.sfx_volume);
    push_bool(content, "TabAcceleration", default.tab_acceleration);
    push_bool(content, "TranslatedTitles", default.translated_titles);
    push_line(content, "VideoRenderer", default.video_renderer);
    push_bool(content, "Vsync", default.vsync);
    push_bool(content, "Windowed", default.windowed);
    push_bool(content, "WriteCurrentScreen", default.write_current_screen);
    content.push('\n');
}

fn push_default_theme(content: &mut String, default: &Config) {
    push_section(content, "[Theme]");
    push_config_theme_lines(content, default);
    push_config_null_or_die_lines(content, default);
    content.push('\n');
}
