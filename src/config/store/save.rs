use super::*;
use crate::engine::input::Keymap;

pub(super) fn build_content(
    cfg: &Config,
    keymap: &Keymap,
    machine_default_noteskin: &str,
    additional_song_folders: &str,
) -> String {
    let mut content = String::with_capacity(4096);
    push_saved_options(
        &mut content,
        cfg,
        machine_default_noteskin,
        additional_song_folders,
    );
    push_saved_keymaps(&mut content, keymap);
    push_saved_theme(&mut content, cfg);
    content
}

fn push_saved_options(
    content: &mut String,
    cfg: &Config,
    machine_default_noteskin: &str,
    additional_song_folders: &str,
) {
    let audio_output_device = cfg
        .audio_output_device_index
        .map_or_else(|| "Auto".to_string(), |idx| idx.to_string());
    let audio_rate_str = cfg
        .audio_sample_rate_hz
        .map_or_else(|| "Auto".to_string(), |hz| hz.to_string());

    push_section(content, "[Options]");
    push_line(content, "AudioOutputDevice", audio_output_device);
    push_line(content, "AudioOutputMode", cfg.audio_output_mode.as_str());
    push_line(content, "AudioSampleRateHz", audio_rate_str);
    push_line(content, "AdditionalSongFolders", additional_song_folders);
    push_bool(content, "AutoDownloadUnlocks", cfg.auto_download_unlocks);
    push_bool(
        content,
        "AutoPopulateGrooveStatsScores",
        cfg.auto_populate_gs_scores,
    );
    push_line(content, "BGBrightness", cfg.bg_brightness.clamp(0.0, 1.0));
    push_bool(content, "BannerCache", cfg.banner_cache);
    push_bool(content, "CacheSongs", cfg.cachesongs);
    push_bool(content, "CDTitleCache", cfg.cdtitle_cache);
    push_bool(content, "Center1Player", cfg.center_1player_notefield);
    push_bool(
        content,
        "CourseAutosubmitScoresIndividually",
        cfg.autosubmit_course_scores_individually,
    );
    push_bool(
        content,
        "CourseShowIndividualScores",
        cfg.show_course_individual_scores,
    );
    push_bool(
        content,
        "CourseShowMostPlayed",
        cfg.show_most_played_courses,
    );
    push_bool(content, "CourseShowRandom", cfg.show_random_courses);
    push_line(content, "DefaultFailType", cfg.default_fail_type.as_str());
    push_line(
        content,
        "NullOrDieSyncGraph",
        cfg.null_or_die_sync_graph.as_str(),
    );
    push_line(
        content,
        "NullOrDieConfidencePercent",
        clamp_null_or_die_confidence_percent(cfg.null_or_die_confidence_percent),
    );
    push_line(
        content,
        "PackSyncThreads",
        cfg.null_or_die_pack_sync_threads,
    );
    push_line(
        content,
        "NullOrDieFingerprintMs",
        format!(
            "{:.1}",
            clamp_null_or_die_positive_ms(cfg.null_or_die_fingerprint_ms)
        ),
    );
    push_line(
        content,
        "NullOrDieWindowMs",
        format!(
            "{:.1}",
            clamp_null_or_die_positive_ms(cfg.null_or_die_window_ms)
        ),
    );
    push_line(
        content,
        "NullOrDieStepMs",
        format!(
            "{:.1}",
            clamp_null_or_die_positive_ms(cfg.null_or_die_step_ms)
        ),
    );
    push_line(
        content,
        "NullOrDieMagicOffsetMs",
        format!(
            "{:.1}",
            clamp_null_or_die_magic_offset_ms(cfg.null_or_die_magic_offset_ms)
        ),
    );
    push_line(
        content,
        "NullOrDieKernelTarget",
        null_or_die_kernel_target_str(cfg.null_or_die_kernel_target),
    );
    push_line(
        content,
        "NullOrDieKernelType",
        null_or_die_kernel_type_str(cfg.null_or_die_kernel_type),
    );
    push_bool(
        content,
        "NullOrDieFullSpectrogram",
        cfg.null_or_die_full_spectrogram,
    );
    push_line(content, "DefaultNoteSkin", machine_default_noteskin);
    push_line(content, "DisplayHeight", cfg.display_height);
    push_line(content, "DisplayWidth", cfg.display_width);
    push_bool(content, "EnableArrowCloud", cfg.enable_arrowcloud);
    push_bool(content, "EnableBoogieStats", cfg.enable_boogiestats);
    push_bool(content, "EnableGrooveStats", cfg.enable_groovestats);
    push_bool(
        content,
        "SubmitArrowCloudFails",
        cfg.submit_arrowcloud_fails,
    );
    push_bool(
        content,
        "SubmitGrooveStatsFails",
        cfg.submit_groovestats_fails,
    );
    push_bool(content, "FastLoad", cfg.fastload);
    push_line(content, "FullscreenType", cfg.fullscreen_type.as_str());
    push_line(content, "Game", cfg.game_flag.as_str());
    push_line(content, "GamepadBackend", cfg.windows_gamepad_backend);
    push_bool(content, "GfxDebug", cfg.gfx_debug);
    push_line(content, "GlobalOffsetSeconds", cfg.global_offset_seconds);
    push_line(content, "Language", cfg.language_flag.as_str());
    push_line(content, "LogLevel", cfg.log_level.as_str());
    push_bool(content, "LogToFile", cfg.log_to_file);
    push_line(
        content,
        "LinuxAudioBackend",
        cfg.linux_audio_backend.as_str(),
    );
    push_line(content, "MaxFps", cfg.max_fps);
    push_line(content, "PresentModePolicy", cfg.present_mode_policy);
    push_line(content, "VisualDelaySeconds", cfg.visual_delay_seconds);
    push_line(content, "MasterVolume", cfg.master_volume);
    push_bool(content, "MenuMusic", cfg.menu_music);
    push_bool(content, "MineHitSound", cfg.mine_hit_sound);
    push_line(content, "MusicVolume", cfg.music_volume);
    push_line(
        content,
        "MusicWheelSwitchSpeed",
        cfg.music_wheel_switch_speed.max(1),
    );
    push_bool(
        content,
        "RateModPreservesPitch",
        cfg.rate_mod_preserves_pitch,
    );
    push_line(
        content,
        "SelectMusicBreakdown",
        cfg.select_music_breakdown_style.as_str(),
    );
    push_bool(
        content,
        "SelectMusicShowBanners",
        cfg.show_select_music_banners,
    );
    push_bool(
        content,
        "SelectMusicShowVideoBanners",
        cfg.show_select_music_video_banners,
    );
    push_bool(
        content,
        "SelectMusicShowBreakdown",
        cfg.show_select_music_breakdown,
    );
    push_bool(
        content,
        "SelectMusicShowCDTitles",
        cfg.show_select_music_cdtitles,
    );
    push_bool(
        content,
        "SelectMusicWheelGrades",
        cfg.show_music_wheel_grades,
    );
    push_bool(content, "SelectMusicWheelLamps", cfg.show_music_wheel_lamps);
    push_line(
        content,
        "SelectMusicWheelITLRank",
        cfg.select_music_itl_rank_mode.as_str(),
    );
    push_line(
        content,
        "SelectMusicWheelITL",
        cfg.select_music_itl_wheel_mode.as_str(),
    );
    push_line(
        content,
        "SelectMusicWheelStyle",
        cfg.select_music_wheel_style.as_str(),
    );
    push_line(
        content,
        "SelectMusicNewPackMode",
        cfg.select_music_new_pack_mode.as_str(),
    );
    push_bool(
        content,
        "SelectMusicPreviews",
        cfg.show_select_music_previews,
    );
    push_bool(
        content,
        "SelectMusicPreviewMarker",
        cfg.show_select_music_preview_marker,
    );
    push_bool(
        content,
        "SelectMusicPreviewLoop",
        cfg.select_music_preview_loop,
    );
    push_line(
        content,
        "SelectMusicPatternInfo",
        cfg.select_music_pattern_info_mode.as_str(),
    );
    push_bool(
        content,
        "SelectMusicScorebox",
        cfg.show_select_music_scorebox,
    );
    push_line(
        content,
        "SelectMusicScoreboxPlacement",
        cfg.select_music_scorebox_placement.as_str(),
    );
    push_bool(
        content,
        "SelectMusicScoreboxCycleItg",
        cfg.select_music_scorebox_cycle_itg,
    );
    push_bool(
        content,
        "SelectMusicScoreboxCycleEx",
        cfg.select_music_scorebox_cycle_ex,
    );
    push_bool(
        content,
        "SelectMusicScoreboxCycleHardEx",
        cfg.select_music_scorebox_cycle_hard_ex,
    );
    push_bool(
        content,
        "SelectMusicScoreboxCycleTournaments",
        cfg.select_music_scorebox_cycle_tournaments,
    );
    push_bool(
        content,
        "SelectMusicChartInfoPeakNps",
        cfg.select_music_chart_info_peak_nps,
    );
    push_bool(
        content,
        "SelectMusicChartInfoMatrixRating",
        cfg.select_music_chart_info_matrix_rating,
    );
    push_bool(
        content,
        "SeparateUnlocksByPlayer",
        cfg.separate_unlocks_by_player,
    );
    push_line(
        content,
        "AutoScreenshotEval",
        auto_screenshot_mask_to_str(cfg.auto_screenshot_eval),
    );
    push_bool(content, "ShowStats", cfg.show_stats_mode != 0);
    push_line(content, "ShowStatsMode", cfg.show_stats_mode.min(3));
    push_bool(content, "SmoothHistogram", cfg.smooth_histogram);
    push_line(
        content,
        "InputDebounceTime",
        format!("{:.3}", cfg.input_debounce_seconds),
    );
    push_bool(
        content,
        "ArcadeOptionsNavigation",
        cfg.arcade_options_navigation,
    );
    push_bool(content, "ThreeKeyNavigation", cfg.three_key_navigation);
    push_bool(content, "UseFSRs", cfg.use_fsrs);
    push_bool(
        content,
        "OnlyDedicatedMenuButtons",
        cfg.only_dedicated_menu_buttons,
    );
    push_line(content, "DisplayMonitor", cfg.display_monitor);
    push_line(content, "SongParsingThreads", cfg.song_parsing_threads);
    push_line(
        content,
        "SoftwareRendererThreads",
        cfg.software_renderer_threads,
    );
    push_line(content, "Theme", cfg.theme_flag.as_str());
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

fn push_saved_keymaps(content: &mut String, keymap: &Keymap) {
    push_section(content, "[Keymaps]");
    for act in ALL_VIRTUAL_ACTIONS {
        let key_name = action_to_ini_key(act);
        let mut tokens: Vec<String> = Vec::new();
        let mut i = 0;
        while let Some(binding) = keymap.binding_at(act, i) {
            tokens.push(binding_to_token(binding));
            i += 1;
        }
        push_line(content, key_name, tokens.join(","));
    }
    content.push('\n');
}

fn push_saved_theme(content: &mut String, cfg: &Config) {
    push_section(content, "[Theme]");
    push_bool(content, "KeyboardFeatures", cfg.keyboard_features);
    push_line(
        content,
        "MenuBackgroundStyle",
        cfg.menu_background_style.as_str(),
    );
    push_bool(content, "VideoBackgrounds", cfg.show_video_backgrounds);
    push_bool(
        content,
        "MachineShowEvalSummary",
        cfg.machine_show_eval_summary,
    );
    push_bool(content, "MachineShowGameOver", cfg.machine_show_gameover);
    push_bool(content, "MachineShowNameEntry", cfg.machine_show_name_entry);
    push_bool(
        content,
        "MachineShowSelectColor",
        cfg.machine_show_select_color,
    );
    push_bool(
        content,
        "MachineShowSelectPlayMode",
        cfg.machine_show_select_play_mode,
    );
    push_bool(
        content,
        "MachineShowSelectProfile",
        cfg.machine_show_select_profile,
    );
    push_bool(
        content,
        "AllowSwitchProfileInMenu",
        cfg.allow_switch_profile_in_menu,
    );
    push_bool(
        content,
        "MachineShowSelectStyle",
        cfg.machine_show_select_style,
    );
    push_bool(content, "MachineEnableReplays", cfg.machine_enable_replays);
    push_bool(
        content,
        "MachineAllowPerPlayerGlobalOffsets",
        cfg.machine_allow_per_player_global_offsets,
    );
    push_line(
        content,
        "MachinePreferredStyle",
        cfg.machine_preferred_style.as_str(),
    );
    push_line(
        content,
        "MachinePreferredPlayMode",
        cfg.machine_preferred_play_mode.as_str(),
    );
    push_bool(
        content,
        "ShowSelectMusicGameplayTimer",
        cfg.show_select_music_gameplay_timer,
    );
    push_line(content, "SimplyLoveColor", cfg.simply_love_color);
    push_bool(content, "ZmodRatingBoxText", cfg.zmod_rating_box_text);
    push_bool(content, "ShowBpmDecimal", cfg.show_bpm_decimal);
    content.push('\n');
}
