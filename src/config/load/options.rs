use super::*;
use deadsync_config::audio::{
    clamp_audio_volume_percent, clamp_music_wheel_switch_speed, parse_auto_audio_output_device,
    parse_auto_audio_sample_rate_hz,
};
use deadsync_config::machine::{
    canonical_frame_stats_overlay_anchor, canonical_frame_stats_overlay_style,
    clamp_smx_light_brightness_percent,
};
use deadsync_config::numbers::parse_auto_threads_u8;
use deadsync_config::options::{
    parse_select_music_itl_rank_mode, parse_select_music_song_select_bg_mode, parse_show_stats_mode,
};
use deadsync_lights::{
    SerialPortName, parse_driver_or_default, parse_gameplay_pad_lights_or_default,
};

pub(super) fn load(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    load_system_opts(conf, default, cfg);
    load_null_or_die_opts(conf, default, cfg);
    load_audio_opts(conf, default, cfg);
    load_select_music_opts(conf, default, cfg);
    load_runtime_opts(conf, default, cfg);
}

fn load_system_opts(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    cfg.vsync = parse_u8_bool_or_default(conf.get("Options", "Vsync").as_deref(), default.vsync);
    cfg.max_fps = conf
        .get("Options", "MaxFps")
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(default.max_fps);
    cfg.present_mode_policy = conf
        .get("Options", "PresentModePolicy")
        .and_then(|s| PresentModePolicy::from_str(&s).ok())
        .or_else(|| {
            conf.get("Options", "UncappedMode").and_then(|s| {
                match s.trim().to_ascii_lowercase().as_str() {
                    "balanced" => Some(PresentModePolicy::Mailbox),
                    "unhinged" | "maxfps" | "max_fps" | "max-fps" => {
                        Some(PresentModePolicy::Immediate)
                    }
                    _ => None,
                }
            })
        })
        .unwrap_or(default.present_mode_policy);
    cfg.windowed =
        parse_u8_bool_or_default(conf.get("Options", "Windowed").as_deref(), default.windowed);
    cfg.fullscreen_type = conf
        .get("Options", "FullscreenType")
        .and_then(|v| FullscreenType::from_str(&v).ok())
        .unwrap_or(default.fullscreen_type);
    cfg.game_flag = conf
        .get("Options", "Game")
        .and_then(|v| GameFlag::from_str(&v).ok())
        .unwrap_or(default.game_flag);
    cfg.display_monitor = conf
        .get("Options", "DisplayMonitor")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default.display_monitor);
    cfg.auto_download_unlocks = parse_u8_bool_or_default(
        conf.get("Options", "AutoDownloadUnlocks").as_deref(),
        default.auto_download_unlocks,
    );
    cfg.auto_populate_gs_scores = parse_u8_bool_or_default(
        conf.get("Options", "AutoPopulateGrooveStatsScores")
            .as_deref(),
        default.auto_populate_gs_scores,
    );
    cfg.updater_install_enabled = parse_u8_bool_or_default(
        conf.get("Options", "UpdaterInstallEnabled").as_deref(),
        default.updater_install_enabled,
    );
    cfg.enable_groovestats = parse_u8_bool_or_default(
        conf.get("Options", "EnableGrooveStats").as_deref(),
        default.enable_groovestats,
    );
    cfg.enable_arrowcloud = parse_u8_bool_or_default(
        conf.get("Options", "EnableArrowCloud").as_deref(),
        default.enable_arrowcloud,
    );
    cfg.enable_boogiestats = parse_u8_bool_or_default(
        conf.get("Options", "EnableBoogieStats").as_deref(),
        default.enable_boogiestats,
    );
    cfg.submit_arrowcloud_fails = parse_u8_bool_or_default(
        conf.get("Options", "SubmitArrowCloudFails").as_deref(),
        default.submit_arrowcloud_fails,
    );
    cfg.arrowcloud_qr_login_when = conf
        .get("Options", "ArrowCloudQrLoginWhen")
        .and_then(|v| ArrowCloudQrLoginWhen::from_str(&v).ok())
        .unwrap_or(default.arrowcloud_qr_login_when);
    cfg.groovestats_qr_login_when = conf
        .get("Options", "GrooveStatsQrLoginWhen")
        .and_then(|v| GrooveStatsQrLoginWhen::from_str(&v).ok())
        .unwrap_or(default.groovestats_qr_login_when);
    cfg.separate_unlocks_by_player = parse_u8_bool_or_default(
        conf.get("Options", "SeparateUnlocksByPlayer").as_deref(),
        default.separate_unlocks_by_player,
    );
    cfg.mine_hit_sound = parse_u8_bool_or_default(
        conf.get("Options", "MineHitSound").as_deref(),
        default.mine_hit_sound,
    );
    let show_stats_mode = conf.get("Options", "ShowStatsMode");
    let show_stats_legacy = conf.get("Options", "ShowStats");
    cfg.show_stats_mode = parse_show_stats_mode(
        show_stats_mode.as_deref(),
        show_stats_legacy.as_deref(),
        default.show_stats_mode,
    );
    cfg.frame_stats_overlay_anchor = conf
        .get("Options", "FrameStatsOverlayAnchor")
        .map(|v| canonical_frame_stats_overlay_anchor(&v))
        .unwrap_or(default.frame_stats_overlay_anchor);
    cfg.frame_stats_overlay_style = conf
        .get("Options", "FrameStatsOverlayStyle")
        .map(|v| canonical_frame_stats_overlay_style(&v))
        .unwrap_or(default.frame_stats_overlay_style);
    cfg.translated_titles = conf
        .get("Options", "TranslatedTitles")
        .or_else(|| conf.get("Options", "translatedtitles"))
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.translated_titles);
    cfg.bg_brightness = conf
        .get("Options", "BGBrightness")
        .and_then(|v| v.parse::<f32>().ok())
        .map_or(default.bg_brightness, clamp_bg_brightness);
    cfg.gameplay_bg_color = conf
        .get("Options", "GameplayBgColor")
        .and_then(|v| Color::from_hex(&v))
        .unwrap_or(default.gameplay_bg_color);
    cfg.center_1player_notefield = conf
        .get("Options", "Center1Player")
        .or_else(|| conf.get("Options", "CenteredP1Notefield"))
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.center_1player_notefield);
    cfg.center_image_translate_x = conf
        .get("Options", "CenterImageTranslateX")
        .and_then(|v| v.trim().parse::<i32>().ok())
        .unwrap_or(default.center_image_translate_x);
    cfg.center_image_translate_y = conf
        .get("Options", "CenterImageTranslateY")
        .and_then(|v| v.trim().parse::<i32>().ok())
        .unwrap_or(default.center_image_translate_y);
    cfg.center_image_add_width = conf
        .get("Options", "CenterImageAddWidth")
        .and_then(|v| v.trim().parse::<i32>().ok())
        .unwrap_or(default.center_image_add_width);
    cfg.center_image_add_height = conf
        .get("Options", "CenterImageAddHeight")
        .and_then(|v| v.trim().parse::<i32>().ok())
        .unwrap_or(default.center_image_add_height);
    cfg.autosubmit_course_scores_individually = parse_u8_bool_or_default(
        conf.get("Options", "CourseAutosubmitScoresIndividually")
            .as_deref(),
        default.autosubmit_course_scores_individually,
    );
    cfg.show_course_individual_scores = parse_u8_bool_or_default(
        conf.get("Options", "CourseShowIndividualScores").as_deref(),
        default.show_course_individual_scores,
    );
    cfg.show_most_played_courses = parse_u8_bool_or_default(
        conf.get("Options", "CourseShowMostPlayed").as_deref(),
        default.show_most_played_courses,
    );
    cfg.show_random_courses = parse_u8_bool_or_default(
        conf.get("Options", "CourseShowRandom").as_deref(),
        default.show_random_courses,
    );
    cfg.default_fail_type = conf
        .get("Options", "DefaultFailType")
        .and_then(|v| DefaultFailType::from_str(&v).ok())
        .unwrap_or(default.default_fail_type);
    cfg.banner_cache = parse_u8_bool_or_default(
        conf.get("Options", "BannerCache").as_deref(),
        default.banner_cache,
    );
    cfg.cdtitle_cache = parse_u8_bool_or_default(
        conf.get("Options", "CDTitleCache").as_deref(),
        default.cdtitle_cache,
    );
    cfg.display_width = conf
        .get("Options", "DisplayWidth")
        .and_then(|v| v.parse().ok())
        .unwrap_or(default.display_width);
    cfg.display_height = conf
        .get("Options", "DisplayHeight")
        .and_then(|v| v.parse().ok())
        .unwrap_or(default.display_height);
    cfg.video_renderer = conf
        .get("Options", "VideoRenderer")
        .and_then(|s| BackendType::from_str(&s).ok())
        .unwrap_or(default.video_renderer);
    cfg.high_dpi = conf
        .get("Options", "HighDPI")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.high_dpi);
    cfg.hide_mouse_cursor = conf
        .get("Options", "HideMouseCursor")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.hide_mouse_cursor);
    cfg.windows_gamepad_backend = conf
        .get("Options", "GamepadBackend")
        .and_then(|s| WindowsPadBackend::from_str(&s).ok())
        .unwrap_or(default.windows_gamepad_backend);
    cfg.allow_shutdown_host = conf
        .get("Options", "AllowShutdown")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.allow_shutdown_host);
    cfg.smx_input = parse_u8_bool_or_default(
        conf.get("Options", "SmxInput").as_deref(),
        default.smx_input,
    );
    cfg.smx_manages_pad_config = parse_u8_bool_or_default(
        conf.get("Options", "SmxManagesPadConfig").as_deref(),
        default.smx_manages_pad_config,
    );
    cfg.smx_panel_lights = conf
        .get("Options", "SmxPanelLights")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.smx_panel_lights);
    cfg.smx_underglow_theme = conf
        .get("Options", "SmxUnderglowTheme")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.smx_underglow_theme);
    cfg.smx_default_pad_config = conf
        .get("Options", "SmxDefaultPadConfig")
        .and_then(|s| crate::config::SmxPadPreset::from_str(&s).ok())
        .unwrap_or(default.smx_default_pad_config);
    cfg.smx_default_light_brightness = conf
        .get("Options", "SmxDefaultLightBrightness")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(
            default.smx_default_light_brightness,
            clamp_smx_light_brightness_percent,
        );
    cfg.gfx_debug = parse_u8_bool_or_default(
        conf.get("Options", "GfxDebug").as_deref(),
        default.gfx_debug,
    );
    cfg.global_offset_seconds = conf
        .get("Options", "GlobalOffsetSeconds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(default.global_offset_seconds);
    cfg.language_flag = conf
        .get("Options", "Language")
        .and_then(|v| LanguageFlag::from_str(&v).ok())
        .unwrap_or(default.language_flag);
    cfg.log_level = conf
        .get("Options", "LogLevel")
        .and_then(|v| LogLevel::from_str(&v).ok())
        .unwrap_or(default.log_level);
    cfg.log_to_file = conf
        .get("Options", "LogToFile")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default.log_to_file);
    cfg.show_console = conf
        .get("Options", "ShowConsole")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default.show_console);
}

fn load_null_or_die_opts(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    cfg.null_or_die_sync_graph = conf
        .get("Options", "NullOrDieSyncGraph")
        .and_then(|v| SyncGraphMode::from_str(&v).ok())
        .unwrap_or(default.null_or_die_sync_graph);
    cfg.null_or_die_confidence_percent = conf
        .get("Options", "NullOrDieConfidencePercent")
        .and_then(|v| v.parse::<u8>().ok())
        .map(clamp_null_or_die_confidence_percent)
        .unwrap_or(default.null_or_die_confidence_percent);
    cfg.null_or_die_pack_sync_threads = conf
        .get("Options", "PackSyncThreads")
        .and_then(|v| parse_auto_threads_u8(&v))
        .unwrap_or(default.null_or_die_pack_sync_threads);
    cfg.null_or_die_fingerprint_ms = conf
        .get("Options", "NullOrDieFingerprintMs")
        .and_then(|v| v.parse::<f64>().ok())
        .map(clamp_null_or_die_positive_ms)
        .unwrap_or(default.null_or_die_fingerprint_ms);
    cfg.null_or_die_window_ms = conf
        .get("Options", "NullOrDieWindowMs")
        .and_then(|v| v.parse::<f64>().ok())
        .map(clamp_null_or_die_positive_ms)
        .unwrap_or(default.null_or_die_window_ms);
    cfg.null_or_die_step_ms = conf
        .get("Options", "NullOrDieStepMs")
        .and_then(|v| v.parse::<f64>().ok())
        .map(clamp_null_or_die_positive_ms)
        .unwrap_or(default.null_or_die_step_ms);
    cfg.null_or_die_magic_offset_ms = conf
        .get("Options", "NullOrDieMagicOffsetMs")
        .and_then(|v| v.parse::<f64>().ok())
        .map(clamp_null_or_die_magic_offset_ms)
        .unwrap_or(default.null_or_die_magic_offset_ms);
    cfg.null_or_die_kernel_target = conf
        .get("Options", "NullOrDieKernelTarget")
        .and_then(|v| parse_null_or_die_kernel_target(&v))
        .unwrap_or(default.null_or_die_kernel_target);
    cfg.null_or_die_kernel_type = conf
        .get("Options", "NullOrDieKernelType")
        .and_then(|v| parse_null_or_die_kernel_type(&v))
        .unwrap_or(default.null_or_die_kernel_type);
    cfg.null_or_die_full_spectrogram = parse_u8_bool_or_default(
        conf.get("Options", "NullOrDieFullSpectrogram").as_deref(),
        default.null_or_die_full_spectrogram,
    );
}

fn load_audio_opts(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    cfg.linux_audio_backend = conf
        .get("Options", "LinuxAudioBackend")
        .and_then(|v| LinuxAudioBackend::from_str(&v).ok())
        .unwrap_or(default.linux_audio_backend);
    cfg.visual_delay_seconds = conf
        .get("Options", "VisualDelaySeconds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(default.visual_delay_seconds);
    cfg.master_volume = conf
        .get("Options", "MasterVolume")
        .and_then(|v| v.parse().ok())
        .map_or(default.master_volume, clamp_audio_volume_percent);
    cfg.menu_music = parse_u8_bool_or_default(
        conf.get("Options", "MenuMusic").as_deref(),
        default.menu_music,
    );
    cfg.custom_sounds_enabled = parse_u8_bool_or_default(
        conf.get("Options", "CustomSoundsEnabled").as_deref(),
        default.custom_sounds_enabled,
    );
    cfg.music_volume = conf
        .get("Options", "MusicVolume")
        .and_then(|v| v.parse().ok())
        .map_or(default.music_volume, clamp_audio_volume_percent);
    cfg.music_wheel_switch_speed = conf
        .get("Options", "MusicWheelSwitchSpeed")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(
            default.music_wheel_switch_speed,
            clamp_music_wheel_switch_speed,
        );
    cfg.sfx_volume = conf
        .get("Options", "SFXVolume")
        .and_then(|v| v.parse().ok())
        .map_or(default.sfx_volume, clamp_audio_volume_percent);
    cfg.assist_tick_volume = conf
        .get("Options", "AssistTickVolume")
        .and_then(|v| v.parse().ok())
        .map_or(default.assist_tick_volume, clamp_audio_volume_percent);
    cfg.audio_output_device_index = conf
        .get("Options", "AudioOutputDevice")
        .and_then(|v| parse_auto_audio_output_device(&v))
        .unwrap_or(default.audio_output_device_index);
    cfg.audio_output_mode = conf
        .get("Options", "AudioOutputMode")
        .and_then(|s| AudioOutputMode::from_str(&s).ok())
        .unwrap_or(default.audio_output_mode);
    cfg.audio_sample_rate_hz = conf
        .get("Options", "AudioSampleRateHz")
        .and_then(|v| parse_auto_audio_sample_rate_hz(&v))
        .unwrap_or(default.audio_sample_rate_hz);
    cfg.rate_mod_preserves_pitch = parse_u8_bool_or_default(
        conf.get("Options", "RateModPreservesPitch").as_deref(),
        default.rate_mod_preserves_pitch,
    );
    cfg.enable_replaygain = parse_u8_bool_or_default(
        conf.get("Options", "ReplayGain").as_deref(),
        default.enable_replaygain,
    );
    cfg.write_current_screen = conf
        .get("Options", "WriteCurrentScreen")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default.write_current_screen);
    cfg.tab_acceleration = conf
        .get("Options", "TabAcceleration")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default.tab_acceleration);
}

fn load_select_music_opts(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    cfg.select_music_breakdown_style = conf
        .get("Options", "SelectMusicBreakdown")
        .and_then(|v| BreakdownStyle::from_str(&v).ok())
        .unwrap_or(default.select_music_breakdown_style);
    cfg.show_select_music_banners = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicShowBanners").as_deref(),
        default.show_select_music_banners,
    );
    cfg.show_version_overlay = parse_u8_bool_or_default(
        conf.get("Options", "ShowVersionOverlay").as_deref(),
        default.show_version_overlay,
    );
    cfg.version_overlay_side = conf
        .get("Options", "VersionOverlaySide")
        .and_then(|v| VersionOverlaySide::from_str(&v).ok())
        .unwrap_or(default.version_overlay_side);
    cfg.show_select_music_video_banners = conf
        .get("Options", "SelectMusicShowVideoBanners")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default.show_select_music_video_banners);
    cfg.show_select_music_breakdown = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicShowBreakdown").as_deref(),
        default.show_select_music_breakdown,
    );
    cfg.show_select_music_stage_display = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicShowStageDisplay")
            .as_deref(),
        default.show_select_music_stage_display,
    );
    cfg.show_select_music_cdtitles = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicShowCDTitles").as_deref(),
        default.show_select_music_cdtitles,
    );
    cfg.show_music_wheel_grades = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicWheelGrades").as_deref(),
        default.show_music_wheel_grades,
    );
    cfg.show_music_wheel_lamps = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicWheelLamps").as_deref(),
        default.show_music_wheel_lamps,
    );
    let itl_rank_mode = conf.get("Options", "SelectMusicWheelITLRank");
    let legacy_itl_chart_rank = conf.get("Options", "SelectMusicShowITLChartRank");
    cfg.select_music_itl_rank_mode = parse_select_music_itl_rank_mode(
        itl_rank_mode.as_deref(),
        legacy_itl_chart_rank.as_deref(),
        default.select_music_itl_rank_mode,
    );
    cfg.select_music_itl_wheel_mode = conf
        .get("Options", "SelectMusicWheelITL")
        .and_then(|v| SelectMusicItlWheelMode::from_str(&v).ok())
        .unwrap_or(default.select_music_itl_wheel_mode);
    cfg.select_music_wheel_style = conf
        .get("Options", "SelectMusicWheelStyle")
        .and_then(|v| SelectMusicWheelStyle::from_str(&v).ok())
        .unwrap_or(default.select_music_wheel_style);
    let song_select_bg = conf.get("Options", "SongSelectBG");
    let legacy_song_select_bg = conf.get("Options", "SelectMusicSongSelectBG");
    cfg.select_music_song_select_bg_mode = parse_select_music_song_select_bg_mode(
        song_select_bg.as_deref(),
        legacy_song_select_bg.as_deref(),
        default.select_music_song_select_bg_mode,
    );
    cfg.select_music_new_pack_mode = conf
        .get("Options", "SelectMusicNewPackMode")
        .and_then(|v| NewPackMode::from_str(&v).ok())
        .unwrap_or(default.select_music_new_pack_mode);
    cfg.show_select_music_folder_stats = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicFolderStats").as_deref(),
        default.show_select_music_folder_stats,
    );
    cfg.show_select_music_previews = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicPreviews").as_deref(),
        default.show_select_music_previews,
    );
    cfg.show_select_music_preview_marker = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicPreviewMarker").as_deref(),
        default.show_select_music_preview_marker,
    );
    cfg.select_music_preview_loop = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicPreviewLoop").as_deref(),
        default.select_music_preview_loop,
    );
    cfg.select_music_pattern_info_mode = conf
        .get("Options", "SelectMusicPatternInfo")
        .and_then(|v| SelectMusicPatternInfoMode::from_str(&v).ok())
        .unwrap_or(default.select_music_pattern_info_mode);
    cfg.select_music_step_artist_box_mode = conf
        .get("Options", "SelectMusicStepArtistBox")
        .and_then(|v| SelectMusicStepArtistBoxMode::from_str(&v).ok())
        .unwrap_or(default.select_music_step_artist_box_mode);
    cfg.show_select_music_scorebox = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicScorebox").as_deref(),
        default.show_select_music_scorebox,
    );
    cfg.select_music_scorebox_placement = conf
        .get("Options", "SelectMusicScoreboxPlacement")
        .and_then(|v| SelectMusicScoreboxPlacement::from_str(&v).ok())
        .unwrap_or(default.select_music_scorebox_placement);
    cfg.select_music_scorebox_cycle_itg = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicScoreboxCycleItg")
            .as_deref(),
        default.select_music_scorebox_cycle_itg,
    );
    cfg.select_music_scorebox_cycle_ex = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicScoreboxCycleEx").as_deref(),
        default.select_music_scorebox_cycle_ex,
    );
    cfg.select_music_scorebox_cycle_hard_ex = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicScoreboxCycleHardEx")
            .as_deref(),
        default.select_music_scorebox_cycle_hard_ex,
    );
    cfg.select_music_scorebox_cycle_tournaments = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicScoreboxCycleTournaments")
            .as_deref(),
        default.select_music_scorebox_cycle_tournaments,
    );
    cfg.select_music_chart_info_peak_nps = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicChartInfoPeakNps")
            .as_deref(),
        default.select_music_chart_info_peak_nps,
    );
    cfg.select_music_chart_info_effective_bpm = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicChartInfoEffectiveBpm")
            .as_deref(),
        default.select_music_chart_info_effective_bpm,
    );
    cfg.select_music_chart_info_matrix_rating = parse_u8_bool_or_default(
        conf.get("Options", "SelectMusicChartInfoMatrixRating")
            .as_deref(),
        default.select_music_chart_info_matrix_rating,
    );
    cfg.auto_screenshot_eval = conf
        .get("Options", "AutoScreenshotEval")
        .map(|v| auto_screenshot_mask_from_str(&v))
        .unwrap_or(default.auto_screenshot_eval);
}

fn load_runtime_opts(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    cfg.fastload =
        parse_u8_bool_or_default(conf.get("Options", "FastLoad").as_deref(), default.fastload);
    cfg.cachesongs = parse_u8_bool_or_default(
        conf.get("Options", "CacheSongs").as_deref(),
        default.cachesongs,
    );
    cfg.song_parsing_threads = conf
        .get("Options", "SongParsingThreads")
        .and_then(|v| parse_auto_threads_u8(&v))
        .unwrap_or(default.song_parsing_threads);
    cfg.smooth_histogram = parse_u8_bool_or_default(
        conf.get("Options", "SmoothHistogram").as_deref(),
        default.smooth_histogram,
    );
    cfg.shade_scatterplot_judgments = parse_u8_bool_or_default(
        conf.get("Options", "ShadeScatterplotJudgments").as_deref(),
        default.shade_scatterplot_judgments,
    );
    cfg.input_debounce_seconds = conf
        .get("Options", "InputDebounceTime")
        .and_then(|v| deadsync_input::parse_input_debounce_seconds(&v))
        .unwrap_or(default.input_debounce_seconds);
    cfg.arcade_options_navigation = conf
        .get("Options", "ArcadeOptionsNavigation")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.arcade_options_navigation);
    cfg.delayed_back = conf
        .get("Options", "DelayedBack")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.delayed_back);
    cfg.three_key_navigation = conf
        .get("Options", "ThreeKeyNavigation")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.three_key_navigation);
    cfg.use_fsrs = conf
        .get("Options", "UseFSRs")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.use_fsrs);
    cfg.lights_driver = conf
        .get("Options", "LightsDriver")
        .map(|v| parse_driver_or_default(&v, default.lights_driver))
        .unwrap_or(default.lights_driver);
    cfg.lights_gameplay_pad_lights = conf
        .get("Options", "GameplayPadLights")
        .map(|v| parse_gameplay_pad_lights_or_default(&v, default.lights_gameplay_pad_lights))
        .unwrap_or(default.lights_gameplay_pad_lights);
    cfg.lights_simplify_bass = conf
        .get("Options", "LightsSimplifyBass")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.lights_simplify_bass);
    cfg.lights_com_port = conf
        .get("Options", "LightsComPort")
        .map(|v| SerialPortName::parse(&v, default.lights_com_port))
        .unwrap_or(default.lights_com_port);
    cfg.only_dedicated_menu_buttons = parse_u8_bool_or_default(
        conf.get("Options", "OnlyDedicatedMenuButtons").as_deref(),
        default.only_dedicated_menu_buttons,
    );
    cfg.theme_flag = conf
        .get("Options", "Theme")
        .and_then(|v| ThemeFlag::from_str(&v).ok())
        .unwrap_or(default.theme_flag);
    cfg.software_renderer_threads = conf
        .get("Options", "SoftwareRendererThreads")
        .and_then(|v| parse_auto_threads_u8(&v))
        .unwrap_or(default.software_renderer_threads);
}

#[cfg(test)]
mod tests {
    use super::Color;

    #[test]
    fn from_hex_accepts_hash_and_bare_forms() {
        assert_eq!(Color::from_hex("#000000"), Some(Color::BLACK));
        assert_eq!(Color::from_hex("FFFFFF"), Some(Color::rgb(1.0, 1.0, 1.0)));
        let gray = Color::from_hex("#0C0C0C").unwrap();
        let expected = 12.0 / 255.0;
        for ch in [gray.r, gray.g, gray.b] {
            assert!((ch - expected).abs() < f32::EPSILON);
        }
        assert_eq!(gray.a, 1.0);
    }

    #[test]
    fn from_hex_parses_argb() {
        let c = Color::from_hex("#8001FE7F").unwrap();
        assert!((c.a - 128.0 / 255.0).abs() < f32::EPSILON);
        assert!((c.r - 1.0 / 255.0).abs() < f32::EPSILON);
        assert!((c.g - 254.0 / 255.0).abs() < f32::EPSILON);
        assert!((c.b - 127.0 / 255.0).abs() < f32::EPSILON);
    }

    #[test]
    fn from_hex_is_case_insensitive_and_trims() {
        assert_eq!(Color::from_hex("  #0c0c0c  "), Color::from_hex("#0C0C0C"));
        assert_eq!(
            Color::from_hex("  80ffffff  "),
            Color::from_hex("#80FFFFFF")
        );
    }

    #[test]
    fn from_hex_rejects_malformed() {
        assert_eq!(Color::from_hex(""), None);
        assert_eq!(Color::from_hex("#FFF"), None);
        assert_eq!(Color::from_hex("#GGGGGG"), None);
        assert_eq!(Color::from_hex("#1234567"), None);
        assert_eq!(Color::from_hex("#123456789"), None);
    }

    #[test]
    fn to_hex_round_trips() {
        assert_eq!(Color::from_hex("#0C0C0C").unwrap().to_hex(), "#0C0C0C");
        assert_eq!(Color::from_hex("#8001FE7F").unwrap().to_hex(), "#8001FE7F");
    }
}
