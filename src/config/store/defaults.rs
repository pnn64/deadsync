use super::*;

pub(super) fn build_content() -> String {
    let default = Config::default();
    let mut content = String::with_capacity(4096);
    push_default_options(&mut content, &default);
    push_default_keymaps(&mut content);
    push_default_theme(&mut content, &default);
    content
}

fn push_default_options(content: &mut String, default: &Config) {
    push_section(content, "[Options]");
    push_line(content, "AudioOutputDevice", "Auto");
    push_line(content, "AudioOutputMode", "Auto");
    push_line(content, "AudioSampleRateHz", "Auto");
    push_line(content, "AdditionalSongFolders", "");
    push_bool(
        content,
        "AutoDownloadUnlocks",
        default.auto_download_unlocks,
    );
    push_bool(
        content,
        "AutoPopulateGrooveStatsScores",
        default.auto_populate_gs_scores,
    );
    push_line(content, "BGBrightness", default.bg_brightness);
    push_bool(content, "BannerCache", default.banner_cache);
    push_bool(content, "CacheSongs", default.cachesongs);
    push_bool(content, "CDTitleCache", default.cdtitle_cache);
    push_bool(content, "Center1Player", default.center_1player_notefield);
    push_bool(
        content,
        "CourseAutosubmitScoresIndividually",
        default.autosubmit_course_scores_individually,
    );
    push_bool(
        content,
        "CourseShowIndividualScores",
        default.show_course_individual_scores,
    );
    push_bool(
        content,
        "CourseShowMostPlayed",
        default.show_most_played_courses,
    );
    push_bool(content, "CourseShowRandom", default.show_random_courses);
    push_line(
        content,
        "DefaultFailType",
        default.default_fail_type.as_str(),
    );
    push_line(content, "DefaultNoteSkin", DEFAULT_MACHINE_NOTESKIN);
    push_line(content, "DisplayHeight", default.display_height);
    push_line(content, "DisplayWidth", default.display_width);
    push_line(content, "DisplayMonitor", default.display_monitor);
    push_bool(content, "EnableArrowCloud", default.enable_arrowcloud);
    push_bool(content, "EnableBoogieStats", default.enable_boogiestats);
    push_bool(content, "EnableGrooveStats", default.enable_groovestats);
    push_bool(
        content,
        "SubmitArrowCloudFails",
        default.submit_arrowcloud_fails,
    );
    push_bool(
        content,
        "SubmitGrooveStatsFails",
        default.submit_groovestats_fails,
    );
    push_bool(content, "FastLoad", default.fastload);
    push_line(content, "FullscreenType", default.fullscreen_type.as_str());
    push_line(content, "Game", default.game_flag.as_str());
    push_line(content, "GamepadBackend", default.windows_gamepad_backend);
    push_bool(content, "GfxDebug", default.gfx_debug);
    push_line(
        content,
        "GlobalOffsetSeconds",
        default.global_offset_seconds,
    );
    push_line(content, "Language", default.language_flag.as_str());
    push_line(content, "LogLevel", default.log_level.as_str());
    push_bool(content, "LogToFile", default.log_to_file);
    push_line(
        content,
        "LinuxAudioBackend",
        default.linux_audio_backend.as_str(),
    );
    push_line(content, "MaxFps", default.max_fps);
    push_line(content, "PresentModePolicy", default.present_mode_policy);
    push_line(content, "VisualDelaySeconds", default.visual_delay_seconds);
    push_line(content, "MasterVolume", default.master_volume);
    push_bool(content, "MenuMusic", default.menu_music);
    push_bool(content, "MineHitSound", default.mine_hit_sound);
    push_line(content, "MusicVolume", default.music_volume);
    push_line(
        content,
        "MusicWheelSwitchSpeed",
        default.music_wheel_switch_speed.max(1),
    );
    push_bool(
        content,
        "RateModPreservesPitch",
        default.rate_mod_preserves_pitch,
    );
    push_line(
        content,
        "SelectMusicBreakdown",
        default.select_music_breakdown_style.as_str(),
    );
    push_bool(
        content,
        "SelectMusicShowBanners",
        default.show_select_music_banners,
    );
    push_bool(
        content,
        "SelectMusicShowVideoBanners",
        default.show_select_music_video_banners,
    );
    push_bool(
        content,
        "SelectMusicShowBreakdown",
        default.show_select_music_breakdown,
    );
    push_bool(
        content,
        "SelectMusicShowCDTitles",
        default.show_select_music_cdtitles,
    );
    push_bool(
        content,
        "SelectMusicWheelGrades",
        default.show_music_wheel_grades,
    );
    push_bool(
        content,
        "SelectMusicWheelLamps",
        default.show_music_wheel_lamps,
    );
    push_line(
        content,
        "SelectMusicWheelStyle",
        default.select_music_wheel_style.as_str(),
    );
    push_line(
        content,
        "SelectMusicNewPackMode",
        default.select_music_new_pack_mode.as_str(),
    );
    push_bool(
        content,
        "SelectMusicPreviews",
        default.show_select_music_previews,
    );
    push_bool(
        content,
        "SelectMusicPreviewMarker",
        default.show_select_music_preview_marker,
    );
    push_bool(
        content,
        "SelectMusicPreviewLoop",
        default.select_music_preview_loop,
    );
    push_line(
        content,
        "SelectMusicPatternInfo",
        default.select_music_pattern_info_mode.as_str(),
    );
    push_bool(
        content,
        "SelectMusicScorebox",
        default.show_select_music_scorebox,
    );
    push_line(
        content,
        "SelectMusicScoreboxPlacement",
        default.select_music_scorebox_placement.as_str(),
    );
    push_bool(
        content,
        "SelectMusicScoreboxCycleItg",
        default.select_music_scorebox_cycle_itg,
    );
    push_bool(
        content,
        "SelectMusicScoreboxCycleEx",
        default.select_music_scorebox_cycle_ex,
    );
    push_bool(
        content,
        "SelectMusicScoreboxCycleHardEx",
        default.select_music_scorebox_cycle_hard_ex,
    );
    push_bool(
        content,
        "SelectMusicScoreboxCycleTournaments",
        default.select_music_scorebox_cycle_tournaments,
    );
    push_bool(
        content,
        "SelectMusicChartInfoPeakNps",
        default.select_music_chart_info_peak_nps,
    );
    push_bool(
        content,
        "SelectMusicChartInfoMatrixRating",
        default.select_music_chart_info_matrix_rating,
    );
    push_bool(
        content,
        "SeparateUnlocksByPlayer",
        default.separate_unlocks_by_player,
    );
    push_line(
        content,
        "AutoScreenshotEval",
        auto_screenshot_mask_to_str(default.auto_screenshot_eval),
    );
    push_bool(content, "ShowStats", default.show_stats_mode != 0);
    push_line(content, "ShowStatsMode", default.show_stats_mode.min(3));
    push_bool(content, "SmoothHistogram", default.smooth_histogram);
    push_line(
        content,
        "InputDebounceTime",
        format!("{:.3}", default.input_debounce_seconds),
    );
    push_bool(
        content,
        "ArcadeOptionsNavigation",
        default.arcade_options_navigation,
    );
    push_bool(content, "ThreeKeyNavigation", default.three_key_navigation);
    push_bool(
        content,
        "OnlyDedicatedMenuButtons",
        default.only_dedicated_menu_buttons,
    );
    push_line(content, "SongParsingThreads", default.song_parsing_threads);
    push_line(
        content,
        "SoftwareRendererThreads",
        default.software_renderer_threads,
    );
    push_line(content, "Theme", default.theme_flag.as_str());
    push_line(content, "AssistTickVolume", default.assist_tick_volume);
    push_line(content, "SFXVolume", default.sfx_volume);
    push_bool(content, "TranslatedTitles", default.translated_titles);
    push_line(content, "VideoRenderer", default.video_renderer);
    push_bool(content, "Vsync", default.vsync);
    push_bool(content, "Windowed", default.windowed);
    push_bool(content, "WriteCurrentScreen", default.write_current_screen);
    content.push('\n');
}

fn push_default_keymaps(content: &mut String) {
    push_section(content, "[Keymaps]");
    for (key, value) in DEFAULT_KEYMAP_LINES {
        push_line(content, key, value);
    }
    content.push('\n');
}

fn push_default_theme(content: &mut String, default: &Config) {
    push_section(content, "[Theme]");
    push_bool(content, "KeyboardFeatures", default.keyboard_features);
    push_bool(content, "VideoBackgrounds", default.show_video_backgrounds);
    push_bool(
        content,
        "MachineShowEvalSummary",
        default.machine_show_eval_summary,
    );
    push_bool(
        content,
        "MachineShowGameOver",
        default.machine_show_gameover,
    );
    push_bool(
        content,
        "MachineShowNameEntry",
        default.machine_show_name_entry,
    );
    push_bool(
        content,
        "MachineShowSelectColor",
        default.machine_show_select_color,
    );
    push_bool(
        content,
        "MachineShowSelectPlayMode",
        default.machine_show_select_play_mode,
    );
    push_bool(
        content,
        "MachineShowSelectProfile",
        default.machine_show_select_profile,
    );
    push_bool(
        content,
        "MachineShowSelectStyle",
        default.machine_show_select_style,
    );
    push_bool(
        content,
        "MachineEnableReplays",
        default.machine_enable_replays,
    );
    push_bool(
        content,
        "MachineAllowPerPlayerGlobalOffsets",
        default.machine_allow_per_player_global_offsets,
    );
    push_line(
        content,
        "MachinePreferredStyle",
        default.machine_preferred_style.as_str(),
    );
    push_line(
        content,
        "MachinePreferredPlayMode",
        default.machine_preferred_play_mode.as_str(),
    );
    push_bool(
        content,
        "ShowSelectMusicGameplayTimer",
        default.show_select_music_gameplay_timer,
    );
    push_line(content, "SimplyLoveColor", default.simply_love_color);
    push_bool(content, "ZmodRatingBoxText", default.zmod_rating_box_text);
    push_bool(content, "ShowBpmDecimal", default.show_bpm_decimal);
    push_line(
        content,
        "NullOrDieSyncGraph",
        default.null_or_die_sync_graph.as_str(),
    );
    push_line(
        content,
        "NullOrDieConfidencePercent",
        clamp_null_or_die_confidence_percent(default.null_or_die_confidence_percent),
    );
    push_line(
        content,
        "PackSyncThreads",
        default.null_or_die_pack_sync_threads,
    );
    push_line(
        content,
        "NullOrDieFingerprintMs",
        format!(
            "{:.1}",
            clamp_null_or_die_positive_ms(default.null_or_die_fingerprint_ms)
        ),
    );
    push_line(
        content,
        "NullOrDieWindowMs",
        format!(
            "{:.1}",
            clamp_null_or_die_positive_ms(default.null_or_die_window_ms)
        ),
    );
    push_line(
        content,
        "NullOrDieStepMs",
        format!(
            "{:.1}",
            clamp_null_or_die_positive_ms(default.null_or_die_step_ms)
        ),
    );
    push_line(
        content,
        "NullOrDieMagicOffsetMs",
        format!(
            "{:.1}",
            clamp_null_or_die_magic_offset_ms(default.null_or_die_magic_offset_ms)
        ),
    );
    push_line(
        content,
        "NullOrDieKernelTarget",
        null_or_die_kernel_target_str(default.null_or_die_kernel_target),
    );
    push_line(
        content,
        "NullOrDieKernelType",
        null_or_die_kernel_type_str(default.null_or_die_kernel_type),
    );
    push_bool(
        content,
        "NullOrDieFullSpectrogram",
        default.null_or_die_full_spectrogram,
    );
    content.push('\n');
}
