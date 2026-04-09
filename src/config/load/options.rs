use super::*;

pub(super) fn load(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    load_system_opts(conf, default, cfg);
    load_null_or_die_opts(conf, default, cfg);
    load_audio_opts(conf, default, cfg);
    load_select_music_opts(conf, default, cfg);
    load_runtime_opts(conf, default, cfg);
}

fn load_system_opts(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    cfg.vsync = conf
        .get("Options", "Vsync")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.vsync, |v| v != 0);
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
    cfg.windowed = conf
        .get("Options", "Windowed")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.windowed, |v| v != 0);
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
    cfg.auto_download_unlocks = conf
        .get("Options", "AutoDownloadUnlocks")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.auto_download_unlocks, |v| v != 0);
    cfg.auto_populate_gs_scores = conf
        .get("Options", "AutoPopulateGrooveStatsScores")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.auto_populate_gs_scores, |v| v != 0);
    cfg.enable_groovestats = conf
        .get("Options", "EnableGrooveStats")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.enable_groovestats, |v| v != 0);
    cfg.enable_arrowcloud = conf
        .get("Options", "EnableArrowCloud")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.enable_arrowcloud, |v| v != 0);
    cfg.enable_boogiestats = conf
        .get("Options", "EnableBoogieStats")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.enable_boogiestats, |v| v != 0);
    cfg.separate_unlocks_by_player = conf
        .get("Options", "SeparateUnlocksByPlayer")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.separate_unlocks_by_player, |v| v != 0);
    cfg.mine_hit_sound = conf
        .get("Options", "MineHitSound")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.mine_hit_sound, |v| v != 0);
    cfg.show_stats_mode = conf
        .get("Options", "ShowStatsMode")
        .and_then(|v| v.parse::<u8>().ok())
        .map(|v| v.min(3))
        .or_else(|| {
            conf.get("Options", "ShowStats")
                .and_then(|v| v.parse::<u8>().ok())
                .map(|v| if v != 0 { 1 } else { 0 })
        })
        .unwrap_or(default.show_stats_mode);
    cfg.translated_titles = conf
        .get("Options", "TranslatedTitles")
        .or_else(|| conf.get("Options", "translatedtitles"))
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.translated_titles);
    cfg.bg_brightness = conf
        .get("Options", "BGBrightness")
        .and_then(|v| v.parse::<f32>().ok())
        .map_or(default.bg_brightness, |v| v.clamp(0.0, 1.0));
    cfg.center_1player_notefield = conf
        .get("Options", "Center1Player")
        .or_else(|| conf.get("Options", "CenteredP1Notefield"))
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.center_1player_notefield);
    cfg.autosubmit_course_scores_individually = conf
        .get("Options", "CourseAutosubmitScoresIndividually")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.autosubmit_course_scores_individually, |v| v != 0);
    cfg.show_course_individual_scores = conf
        .get("Options", "CourseShowIndividualScores")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.show_course_individual_scores, |v| v != 0);
    cfg.show_most_played_courses = conf
        .get("Options", "CourseShowMostPlayed")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.show_most_played_courses, |v| v != 0);
    cfg.show_random_courses = conf
        .get("Options", "CourseShowRandom")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.show_random_courses, |v| v != 0);
    cfg.default_fail_type = conf
        .get("Options", "DefaultFailType")
        .and_then(|v| DefaultFailType::from_str(&v).ok())
        .unwrap_or(default.default_fail_type);
    cfg.banner_cache = conf
        .get("Options", "BannerCache")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.banner_cache, |v| v != 0);
    cfg.cdtitle_cache = conf
        .get("Options", "CDTitleCache")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.cdtitle_cache, |v| v != 0);
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
    cfg.windows_gamepad_backend = conf
        .get("Options", "GamepadBackend")
        .and_then(|s| WindowsPadBackend::from_str(&s).ok())
        .unwrap_or(default.windows_gamepad_backend);
    cfg.gfx_debug = conf
        .get("Options", "GfxDebug")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.gfx_debug, |v| v != 0);
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
        .map(|v| v.trim().to_string())
        .and_then(|v| {
            if v.eq_ignore_ascii_case("auto") || v.is_empty() {
                Some(0u8)
            } else {
                v.parse::<u8>().ok()
            }
        })
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
    cfg.null_or_die_full_spectrogram = conf
        .get("Options", "NullOrDieFullSpectrogram")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.null_or_die_full_spectrogram, |v| v != 0);
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
        .map_or(default.master_volume, |v: u8| v.clamp(0, 100));
    cfg.menu_music = conf
        .get("Options", "MenuMusic")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.menu_music, |v| v != 0);
    cfg.music_volume = conf
        .get("Options", "MusicVolume")
        .and_then(|v| v.parse().ok())
        .map_or(default.music_volume, |v: u8| v.clamp(0, 100));
    cfg.music_wheel_switch_speed = conf
        .get("Options", "MusicWheelSwitchSpeed")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.music_wheel_switch_speed, |v| v.max(1));
    cfg.sfx_volume = conf
        .get("Options", "SFXVolume")
        .and_then(|v| v.parse().ok())
        .map_or(default.sfx_volume, |v: u8| v.clamp(0, 100));
    cfg.assist_tick_volume = conf
        .get("Options", "AssistTickVolume")
        .and_then(|v| v.parse().ok())
        .map_or(default.assist_tick_volume, |v: u8| v.clamp(0, 100));
    cfg.audio_output_device_index = conf
        .get("Options", "AudioOutputDevice")
        .map(|v| v.trim().to_string())
        .and_then(|v| {
            if v.is_empty() || v.eq_ignore_ascii_case("auto") {
                None
            } else {
                v.parse::<u16>().ok()
            }
        })
        .or(default.audio_output_device_index);
    cfg.audio_output_mode = conf
        .get("Options", "AudioOutputMode")
        .and_then(|s| AudioOutputMode::from_str(&s).ok())
        .unwrap_or(default.audio_output_mode);
    cfg.audio_sample_rate_hz = conf
        .get("Options", "AudioSampleRateHz")
        .map(|v| v.trim().to_string())
        .and_then(|v| {
            if v.eq_ignore_ascii_case("auto") || v.is_empty() {
                None
            } else {
                v.parse::<u32>().ok()
            }
        })
        .or(default.audio_sample_rate_hz);
    cfg.rate_mod_preserves_pitch = conf
        .get("Options", "RateModPreservesPitch")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.rate_mod_preserves_pitch, |v| v != 0);
    cfg.write_current_screen = conf
        .get("Options", "WriteCurrentScreen")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default.write_current_screen);
}

fn load_select_music_opts(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    cfg.select_music_breakdown_style = conf
        .get("Options", "SelectMusicBreakdown")
        .and_then(|v| BreakdownStyle::from_str(&v).ok())
        .unwrap_or(default.select_music_breakdown_style);
    cfg.show_select_music_banners = conf
        .get("Options", "SelectMusicShowBanners")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.show_select_music_banners, |v| v != 0);
    cfg.show_select_music_video_banners = conf
        .get("Options", "SelectMusicShowVideoBanners")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default.show_select_music_video_banners);
    cfg.show_select_music_breakdown = conf
        .get("Options", "SelectMusicShowBreakdown")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.show_select_music_breakdown, |v| v != 0);
    cfg.show_select_music_cdtitles = conf
        .get("Options", "SelectMusicShowCDTitles")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.show_select_music_cdtitles, |v| v != 0);
    cfg.show_music_wheel_grades = conf
        .get("Options", "SelectMusicWheelGrades")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.show_music_wheel_grades, |v| v != 0);
    cfg.show_music_wheel_lamps = conf
        .get("Options", "SelectMusicWheelLamps")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.show_music_wheel_lamps, |v| v != 0);
    cfg.select_music_itl_wheel_mode = conf
        .get("Options", "SelectMusicWheelITL")
        .and_then(|v| SelectMusicItlWheelMode::from_str(&v).ok())
        .unwrap_or(default.select_music_itl_wheel_mode);
    cfg.select_music_wheel_style = conf
        .get("Options", "SelectMusicWheelStyle")
        .and_then(|v| SelectMusicWheelStyle::from_str(&v).ok())
        .unwrap_or(default.select_music_wheel_style);
    cfg.select_music_new_pack_mode = conf
        .get("Options", "SelectMusicNewPackMode")
        .and_then(|v| NewPackMode::from_str(&v).ok())
        .unwrap_or(default.select_music_new_pack_mode);
    cfg.show_select_music_previews = conf
        .get("Options", "SelectMusicPreviews")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.show_select_music_previews, |v| v != 0);
    cfg.show_select_music_preview_marker = conf
        .get("Options", "SelectMusicPreviewMarker")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.show_select_music_preview_marker, |v| v != 0);
    cfg.select_music_preview_loop = conf
        .get("Options", "SelectMusicPreviewLoop")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.select_music_preview_loop, |v| v != 0);
    cfg.select_music_pattern_info_mode = conf
        .get("Options", "SelectMusicPatternInfo")
        .and_then(|v| SelectMusicPatternInfoMode::from_str(&v).ok())
        .unwrap_or(default.select_music_pattern_info_mode);
    cfg.show_select_music_scorebox = conf
        .get("Options", "SelectMusicScorebox")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.show_select_music_scorebox, |v| v != 0);
    cfg.select_music_scorebox_placement = conf
        .get("Options", "SelectMusicScoreboxPlacement")
        .and_then(|v| SelectMusicScoreboxPlacement::from_str(&v).ok())
        .unwrap_or(default.select_music_scorebox_placement);
    cfg.select_music_scorebox_cycle_itg = conf
        .get("Options", "SelectMusicScoreboxCycleItg")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.select_music_scorebox_cycle_itg, |v| v != 0);
    cfg.select_music_scorebox_cycle_ex = conf
        .get("Options", "SelectMusicScoreboxCycleEx")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.select_music_scorebox_cycle_ex, |v| v != 0);
    cfg.select_music_scorebox_cycle_hard_ex = conf
        .get("Options", "SelectMusicScoreboxCycleHardEx")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.select_music_scorebox_cycle_hard_ex, |v| v != 0);
    cfg.select_music_scorebox_cycle_tournaments = conf
        .get("Options", "SelectMusicScoreboxCycleTournaments")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.select_music_scorebox_cycle_tournaments, |v| v != 0);
    cfg.select_music_chart_info_peak_nps = conf
        .get("Options", "SelectMusicChartInfoPeakNps")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.select_music_chart_info_peak_nps, |v| v != 0);
    cfg.select_music_chart_info_matrix_rating = conf
        .get("Options", "SelectMusicChartInfoMatrixRating")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.select_music_chart_info_matrix_rating, |v| v != 0);
    cfg.auto_screenshot_eval = conf
        .get("Options", "AutoScreenshotEval")
        .map(|v| auto_screenshot_mask_from_str(&v))
        .unwrap_or(default.auto_screenshot_eval);
}

fn load_runtime_opts(conf: &SimpleIni, default: Config, cfg: &mut Config) {
    cfg.fastload = conf
        .get("Options", "FastLoad")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.fastload, |v| v != 0);
    cfg.cachesongs = conf
        .get("Options", "CacheSongs")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.cachesongs, |v| v != 0);
    cfg.song_parsing_threads = conf
        .get("Options", "SongParsingThreads")
        .map(|v| v.trim().to_string())
        .and_then(|v| {
            if v.eq_ignore_ascii_case("auto") || v.is_empty() {
                Some(0u8)
            } else {
                v.parse::<u8>().ok()
            }
        })
        .unwrap_or(default.song_parsing_threads);
    cfg.smooth_histogram = conf
        .get("Options", "SmoothHistogram")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.smooth_histogram, |v| v != 0);
    cfg.input_debounce_seconds = conf
        .get("Options", "InputDebounceTime")
        .map(|v| v.trim().to_string())
        .and_then(|v| {
            if v.is_empty() {
                return None;
            }
            let lower = v.to_ascii_lowercase();
            if let Some(ms) = lower.strip_suffix("ms") {
                return ms
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .map(|n| (n / 1000.0).clamp(0.0, 0.2));
            }
            v.parse::<f32>().ok().map(|n| {
                let secs = if n > 1.0 { n / 1000.0 } else { n };
                secs.clamp(0.0, 0.2)
            })
        })
        .unwrap_or(default.input_debounce_seconds);
    cfg.arcade_options_navigation = conf
        .get("Options", "ArcadeOptionsNavigation")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.arcade_options_navigation);
    cfg.three_key_navigation = conf
        .get("Options", "ThreeKeyNavigation")
        .and_then(|v| parse_loose_bool_str(&v))
        .unwrap_or(default.three_key_navigation);
    cfg.only_dedicated_menu_buttons = conf
        .get("Options", "OnlyDedicatedMenuButtons")
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default.only_dedicated_menu_buttons, |v| v != 0);
    cfg.theme_flag = conf
        .get("Options", "Theme")
        .and_then(|v| ThemeFlag::from_str(&v).ok())
        .unwrap_or(default.theme_flag);
    cfg.software_renderer_threads = conf
        .get("Options", "SoftwareRendererThreads")
        .map(|v| v.trim().to_string())
        .and_then(|v| {
            if v.eq_ignore_ascii_case("auto") || v.is_empty() {
                Some(0u8)
            } else {
                v.parse::<u8>().ok()
            }
        })
        .unwrap_or(default.software_renderer_threads);
}
