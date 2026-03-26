use super::*;

pub(super) fn normalize_machine_default_noteskin(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DEFAULT_MACHINE_NOTESKIN.to_string();
    }
    trimmed.to_ascii_lowercase()
}

#[inline(always)]
pub(super) fn create_default_config_file() -> Result<(), std::io::Error> {
    info!("'{CONFIG_PATH}' not found, creating with default values.");
    let default = Config::default();

    let mut content = String::new();

    // [Options] section - keys in alphabetical order
    content.push_str("[Options]\n");
    content.push_str("AudioOutputDevice=Auto\n");
    content.push_str("AudioOutputMode=Auto\n");
    content.push_str("AudioSampleRateHz=Auto\n");
    content.push_str("AdditionalSongFolders=\n");
    content.push_str(&format!(
        "AutoDownloadUnlocks={}\n",
        if default.auto_download_unlocks {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "AutoPopulateGrooveStatsScores={}\n",
        if default.auto_populate_gs_scores {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!("BGBrightness={}\n", default.bg_brightness));
    content.push_str(&format!(
        "BannerCache={}\n",
        if default.banner_cache { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "CacheSongs={}\n",
        if default.cachesongs { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "CDTitleCache={}\n",
        if default.cdtitle_cache { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "Center1Player={}\n",
        if default.center_1player_notefield {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseAutosubmitScoresIndividually={}\n",
        if default.autosubmit_course_scores_individually {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseShowIndividualScores={}\n",
        if default.show_course_individual_scores {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseShowMostPlayed={}\n",
        if default.show_most_played_courses {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseShowRandom={}\n",
        if default.show_random_courses {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "DefaultFailType={}\n",
        default.default_fail_type.as_str()
    ));
    content.push_str(&format!("DefaultNoteSkin={DEFAULT_MACHINE_NOTESKIN}\n"));
    content.push_str(&format!("DisplayHeight={}\n", default.display_height));
    content.push_str(&format!("DisplayWidth={}\n", default.display_width));
    content.push_str(&format!("DisplayMonitor={}\n", default.display_monitor));
    content.push_str(&format!(
        "EnableArrowCloud={}\n",
        if default.enable_arrowcloud { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "EnableBoogieStats={}\n",
        if default.enable_boogiestats { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "EnableGrooveStats={}\n",
        if default.enable_groovestats { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "FastLoad={}\n",
        if default.fastload { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "FullscreenType={}\n",
        default.fullscreen_type.as_str()
    ));
    content.push_str(&format!("Game={}\n", default.game_flag.as_str()));
    content.push_str(&format!(
        "GamepadBackend={}\n",
        default.windows_gamepad_backend
    ));
    content.push_str(&format!(
        "GfxDebug={}\n",
        if default.gfx_debug { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "GlobalOffsetSeconds={}\n",
        default.global_offset_seconds
    ));
    content.push_str(&format!("Language={}\n", default.language_flag.as_str()));
    content.push_str(&format!("LogLevel={}\n", default.log_level.as_str()));
    content.push_str(&format!(
        "LogToFile={}\n",
        if default.log_to_file { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "LinuxAudioBackend={}\n",
        default.linux_audio_backend.as_str()
    ));
    content.push_str(&format!("MaxFps={}\n", default.max_fps));
    content.push_str(&format!(
        "PresentModePolicy={}\n",
        default.present_mode_policy
    ));
    content.push_str(&format!(
        "VisualDelaySeconds={}\n",
        default.visual_delay_seconds
    ));
    content.push_str(&format!("MasterVolume={}\n", default.master_volume));
    content.push_str(&format!(
        "MenuMusic={}\n",
        if default.menu_music { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "MineHitSound={}\n",
        if default.mine_hit_sound { "1" } else { "0" }
    ));
    content.push_str(&format!("MusicVolume={}\n", default.music_volume));
    content.push_str(&format!(
        "MusicWheelSwitchSpeed={}\n",
        default.music_wheel_switch_speed.max(1)
    ));
    content.push_str(&format!(
        "RateModPreservesPitch={}\n",
        if default.rate_mod_preserves_pitch {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicBreakdown={}\n",
        default.select_music_breakdown_style.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicShowBanners={}\n",
        if default.show_select_music_banners {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicShowVideoBanners={}\n",
        if default.show_select_music_video_banners {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicShowBreakdown={}\n",
        if default.show_select_music_breakdown {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicShowCDTitles={}\n",
        if default.show_select_music_cdtitles {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicWheelGrades={}\n",
        if default.show_music_wheel_grades {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicWheelLamps={}\n",
        if default.show_music_wheel_lamps {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicNewPackMode={}\n",
        default.select_music_new_pack_mode.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicPreviews={}\n",
        if default.show_select_music_previews {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicPreviewMarker={}\n",
        if default.show_select_music_preview_marker {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicPreviewLoop={}\n",
        if default.select_music_preview_loop {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicPatternInfo={}\n",
        default.select_music_pattern_info_mode.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicScorebox={}\n",
        if default.show_select_music_scorebox {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxPlacement={}\n",
        default.select_music_scorebox_placement.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleItg={}\n",
        if default.select_music_scorebox_cycle_itg {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleEx={}\n",
        if default.select_music_scorebox_cycle_ex {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleHardEx={}\n",
        if default.select_music_scorebox_cycle_hard_ex {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleTournaments={}\n",
        if default.select_music_scorebox_cycle_tournaments {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicChartInfoPeakNps={}\n",
        if default.select_music_chart_info_peak_nps {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicChartInfoMatrixRating={}\n",
        if default.select_music_chart_info_matrix_rating {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SeparateUnlocksByPlayer={}\n",
        if default.separate_unlocks_by_player {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "AutoScreenshotEval={}\n",
        auto_screenshot_mask_to_str(default.auto_screenshot_eval)
    ));
    content.push_str(&format!(
        "ShowStats={}\n",
        if default.show_stats_mode != 0 {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "ShowStatsMode={}\n",
        default.show_stats_mode.min(3)
    ));
    content.push_str(&format!(
        "SmoothHistogram={}\n",
        if default.smooth_histogram { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "InputDebounceTime={:.3}\n",
        default.input_debounce_seconds
    ));
    content.push_str(&format!(
        "OnlyDedicatedMenuButtons={}\n",
        if default.only_dedicated_menu_buttons {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SongParsingThreads={}\n",
        default.song_parsing_threads
    ));
    content.push_str(&format!(
        "SoftwareRendererThreads={}\n",
        default.software_renderer_threads
    ));
    content.push_str(&format!("Theme={}\n", default.theme_flag.as_str()));
    content.push_str(&format!(
        "AssistTickVolume={}\n",
        default.assist_tick_volume
    ));
    content.push_str(&format!("SFXVolume={}\n", default.sfx_volume));
    content.push_str(&format!(
        "TranslatedTitles={}\n",
        if default.translated_titles { "1" } else { "0" }
    ));
    content.push_str(&format!("VideoRenderer={}\n", default.video_renderer));
    content.push_str(&format!(
        "Vsync={}\n",
        if default.vsync { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "Windowed={}\n",
        if default.windowed { "1" } else { "0" }
    ));
    content.push('\n');

    // [Keymaps] section with sane defaults (comma-separated)
    content.push_str("[Keymaps]\n");
    content.push_str("P1_Back=KeyCode::Escape\n");
    content.push_str("P1_Down=KeyCode::ArrowDown,KeyCode::KeyS\n");
    content.push_str("P1_Left=KeyCode::ArrowLeft,KeyCode::KeyA\n");
    content.push_str("P1_MenuDown=\n");
    content.push_str("P1_MenuLeft=\n");
    content.push_str("P1_MenuRight=\n");
    content.push_str("P1_MenuUp=\n");
    content.push_str("P1_Operator=\n");
    content.push_str("P1_Restart=\n");
    content.push_str("P1_Right=KeyCode::ArrowRight,KeyCode::KeyD\n");
    content.push_str("P1_Select=KeyCode::Slash\n");
    content.push_str("P1_Start=KeyCode::Enter\n");
    content.push_str("P1_Up=KeyCode::ArrowUp,KeyCode::KeyW\n");
    // Player 2 keyboard defaults: numpad directions + Start on NumpadEnter + Back on Numpad0.
    content.push_str("P2_Back=KeyCode::Numpad0\n");
    content.push_str("P2_Down=KeyCode::Numpad2\n");
    content.push_str("P2_Left=KeyCode::Numpad4\n");
    content.push_str("P2_MenuDown=\n");
    content.push_str("P2_MenuLeft=\n");
    content.push_str("P2_MenuRight=\n");
    content.push_str("P2_MenuUp=\n");
    content.push_str("P2_Operator=\n");
    content.push_str("P2_Restart=\n");
    content.push_str("P2_Right=KeyCode::Numpad6\n");
    content.push_str("P2_Select=KeyCode::NumpadDecimal\n");
    content.push_str("P2_Start=KeyCode::NumpadEnter\n");
    content.push_str("P2_Up=KeyCode::Numpad8\n");
    content.push('\n');

    // [Theme] section should be last
    content.push_str("[Theme]\n");
    content.push_str(&format!(
        "KeyboardFeatures={}\n",
        if default.keyboard_features { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "VideoBackgrounds={}\n",
        if default.show_video_backgrounds {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowEvalSummary={}\n",
        if default.machine_show_eval_summary {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowGameOver={}\n",
        if default.machine_show_gameover {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowNameEntry={}\n",
        if default.machine_show_name_entry {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectColor={}\n",
        if default.machine_show_select_color {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectPlayMode={}\n",
        if default.machine_show_select_play_mode {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectProfile={}\n",
        if default.machine_show_select_profile {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectStyle={}\n",
        if default.machine_show_select_style {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineEnableReplays={}\n",
        if default.machine_enable_replays {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachinePreferredStyle={}\n",
        default.machine_preferred_style.as_str()
    ));
    content.push_str(&format!(
        "MachinePreferredPlayMode={}\n",
        default.machine_preferred_play_mode.as_str()
    ));
    content.push_str(&format!(
        "ShowSelectMusicGameplayTimer={}\n",
        if default.show_select_music_gameplay_timer {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!("SimplyLoveColor={}\n", default.simply_love_color));
    content.push_str(&format!(
        "ZmodRatingBoxText={}\n",
        if default.zmod_rating_box_text {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "ShowBpmDecimal={}\n",
        if default.show_bpm_decimal { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "NullOrDieSyncGraph={}\n",
        default.null_or_die_sync_graph.as_str()
    ));
    content.push_str(&format!(
        "NullOrDieConfidencePercent={}\n",
        clamp_null_or_die_confidence_percent(default.null_or_die_confidence_percent)
    ));
    content.push_str(&format!(
        "NullOrDieFingerprintMs={:.1}\n",
        clamp_null_or_die_positive_ms(default.null_or_die_fingerprint_ms)
    ));
    content.push_str(&format!(
        "NullOrDieWindowMs={:.1}\n",
        clamp_null_or_die_positive_ms(default.null_or_die_window_ms)
    ));
    content.push_str(&format!(
        "NullOrDieStepMs={:.1}\n",
        clamp_null_or_die_positive_ms(default.null_or_die_step_ms)
    ));
    content.push_str(&format!(
        "NullOrDieMagicOffsetMs={:.1}\n",
        clamp_null_or_die_magic_offset_ms(default.null_or_die_magic_offset_ms)
    ));
    content.push_str(&format!(
        "NullOrDieKernelTarget={}\n",
        null_or_die_kernel_target_str(default.null_or_die_kernel_target)
    ));
    content.push_str(&format!(
        "NullOrDieKernelType={}\n",
        null_or_die_kernel_type_str(default.null_or_die_kernel_type)
    ));
    content.push_str(&format!(
        "NullOrDieFullSpectrogram={}\n",
        if default.null_or_die_full_spectrogram {
            "1"
        } else {
            "0"
        }
    ));
    content.push('\n');

    std::fs::write(CONFIG_PATH, content)
}

pub(super) fn save_without_keymaps() {
    // Manual writer that keeps [Options]/[Theme] sorted and emits a stable,
    // CamelCase [Keymaps] section derived from the current in-memory keymap.
    let cfg = *lock_config();
    let keymap = crate::core::input::get_keymap();
    let machine_default_noteskin = MACHINE_DEFAULT_NOTESKIN.lock().unwrap().clone();
    let additional_song_folders = ADDITIONAL_SONG_FOLDERS.lock().unwrap().clone();

    let mut content = String::new();

    // [Options] (alphabetical order)
    content.push_str("[Options]\n");
    let audio_output_device = cfg
        .audio_output_device_index
        .map_or_else(|| "Auto".to_string(), |idx| idx.to_string());
    content.push_str(&format!("AudioOutputDevice={audio_output_device}\n"));
    content.push_str(&format!(
        "AudioOutputMode={}\n",
        cfg.audio_output_mode.as_str()
    ));
    let audio_rate_str = match cfg.audio_sample_rate_hz {
        None => "Auto".to_string(),
        Some(hz) => hz.to_string(),
    };
    content.push_str(&format!("AudioSampleRateHz={audio_rate_str}\n"));
    content.push_str(&format!(
        "AdditionalSongFolders={additional_song_folders}\n"
    ));
    content.push_str(&format!(
        "AutoDownloadUnlocks={}\n",
        if cfg.auto_download_unlocks { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "AutoPopulateGrooveStatsScores={}\n",
        if cfg.auto_populate_gs_scores {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "BGBrightness={}\n",
        cfg.bg_brightness.clamp(0.0, 1.0)
    ));
    content.push_str(&format!(
        "BannerCache={}\n",
        if cfg.banner_cache { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "CacheSongs={}\n",
        if cfg.cachesongs { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "CDTitleCache={}\n",
        if cfg.cdtitle_cache { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "Center1Player={}\n",
        if cfg.center_1player_notefield {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseAutosubmitScoresIndividually={}\n",
        if cfg.autosubmit_course_scores_individually {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseShowIndividualScores={}\n",
        if cfg.show_course_individual_scores {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseShowMostPlayed={}\n",
        if cfg.show_most_played_courses {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "CourseShowRandom={}\n",
        if cfg.show_random_courses { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "DefaultFailType={}\n",
        cfg.default_fail_type.as_str()
    ));
    content.push_str(&format!(
        "NullOrDieSyncGraph={}\n",
        cfg.null_or_die_sync_graph.as_str()
    ));
    content.push_str(&format!(
        "NullOrDieConfidencePercent={}\n",
        clamp_null_or_die_confidence_percent(cfg.null_or_die_confidence_percent)
    ));
    content.push_str(&format!(
        "NullOrDieFingerprintMs={:.1}\n",
        clamp_null_or_die_positive_ms(cfg.null_or_die_fingerprint_ms)
    ));
    content.push_str(&format!(
        "NullOrDieWindowMs={:.1}\n",
        clamp_null_or_die_positive_ms(cfg.null_or_die_window_ms)
    ));
    content.push_str(&format!(
        "NullOrDieStepMs={:.1}\n",
        clamp_null_or_die_positive_ms(cfg.null_or_die_step_ms)
    ));
    content.push_str(&format!(
        "NullOrDieMagicOffsetMs={:.1}\n",
        clamp_null_or_die_magic_offset_ms(cfg.null_or_die_magic_offset_ms)
    ));
    content.push_str(&format!(
        "NullOrDieKernelTarget={}\n",
        null_or_die_kernel_target_str(cfg.null_or_die_kernel_target)
    ));
    content.push_str(&format!(
        "NullOrDieKernelType={}\n",
        null_or_die_kernel_type_str(cfg.null_or_die_kernel_type)
    ));
    content.push_str(&format!(
        "NullOrDieFullSpectrogram={}\n",
        if cfg.null_or_die_full_spectrogram {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!("DefaultNoteSkin={machine_default_noteskin}\n"));
    content.push_str(&format!("DisplayHeight={}\n", cfg.display_height));
    content.push_str(&format!("DisplayWidth={}\n", cfg.display_width));
    content.push_str(&format!(
        "EnableArrowCloud={}\n",
        if cfg.enable_arrowcloud { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "EnableBoogieStats={}\n",
        if cfg.enable_boogiestats { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "EnableGrooveStats={}\n",
        if cfg.enable_groovestats { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "FastLoad={}\n",
        if cfg.fastload { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "FullscreenType={}\n",
        cfg.fullscreen_type.as_str()
    ));
    content.push_str(&format!("Game={}\n", cfg.game_flag.as_str()));
    content.push_str(&format!("GamepadBackend={}\n", cfg.windows_gamepad_backend));
    content.push_str(&format!(
        "GfxDebug={}\n",
        if cfg.gfx_debug { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "GlobalOffsetSeconds={}\n",
        cfg.global_offset_seconds
    ));
    content.push_str(&format!("Language={}\n", cfg.language_flag.as_str()));
    content.push_str(&format!("LogLevel={}\n", cfg.log_level.as_str()));
    content.push_str(&format!(
        "LogToFile={}\n",
        if cfg.log_to_file { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "LinuxAudioBackend={}\n",
        cfg.linux_audio_backend.as_str()
    ));
    content.push_str(&format!("MaxFps={}\n", cfg.max_fps));
    content.push_str(&format!("PresentModePolicy={}\n", cfg.present_mode_policy));
    content.push_str(&format!(
        "VisualDelaySeconds={}\n",
        cfg.visual_delay_seconds
    ));
    content.push_str(&format!("MasterVolume={}\n", cfg.master_volume));
    content.push_str(&format!(
        "MenuMusic={}\n",
        if cfg.menu_music { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "MineHitSound={}\n",
        if cfg.mine_hit_sound { "1" } else { "0" }
    ));
    content.push_str(&format!("MusicVolume={}\n", cfg.music_volume));
    content.push_str(&format!(
        "MusicWheelSwitchSpeed={}\n",
        cfg.music_wheel_switch_speed.max(1)
    ));
    content.push_str(&format!(
        "RateModPreservesPitch={}\n",
        if cfg.rate_mod_preserves_pitch {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicBreakdown={}\n",
        cfg.select_music_breakdown_style.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicShowBanners={}\n",
        if cfg.show_select_music_banners {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicShowVideoBanners={}\n",
        if cfg.show_select_music_video_banners {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicShowBreakdown={}\n",
        if cfg.show_select_music_breakdown {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicShowCDTitles={}\n",
        if cfg.show_select_music_cdtitles {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicWheelGrades={}\n",
        if cfg.show_music_wheel_grades {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicWheelLamps={}\n",
        if cfg.show_music_wheel_lamps { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "SelectMusicWheelITL={}\n",
        cfg.select_music_itl_wheel_mode.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicNewPackMode={}\n",
        cfg.select_music_new_pack_mode.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicPreviews={}\n",
        if cfg.show_select_music_previews {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicPreviewMarker={}\n",
        if cfg.show_select_music_preview_marker {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicPreviewLoop={}\n",
        if cfg.select_music_preview_loop {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicPatternInfo={}\n",
        cfg.select_music_pattern_info_mode.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicScorebox={}\n",
        if cfg.show_select_music_scorebox {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxPlacement={}\n",
        cfg.select_music_scorebox_placement.as_str()
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleItg={}\n",
        if cfg.select_music_scorebox_cycle_itg {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleEx={}\n",
        if cfg.select_music_scorebox_cycle_ex {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleHardEx={}\n",
        if cfg.select_music_scorebox_cycle_hard_ex {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicScoreboxCycleTournaments={}\n",
        if cfg.select_music_scorebox_cycle_tournaments {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicChartInfoPeakNps={}\n",
        if cfg.select_music_chart_info_peak_nps {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SelectMusicChartInfoMatrixRating={}\n",
        if cfg.select_music_chart_info_matrix_rating {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "SeparateUnlocksByPlayer={}\n",
        if cfg.separate_unlocks_by_player {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "AutoScreenshotEval={}\n",
        auto_screenshot_mask_to_str(cfg.auto_screenshot_eval)
    ));
    content.push_str(&format!(
        "ShowStats={}\n",
        if cfg.show_stats_mode != 0 { "1" } else { "0" }
    ));
    content.push_str(&format!("ShowStatsMode={}\n", cfg.show_stats_mode.min(3)));
    content.push_str(&format!(
        "SmoothHistogram={}\n",
        if cfg.smooth_histogram { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "InputDebounceTime={:.3}\n",
        cfg.input_debounce_seconds
    ));
    content.push_str(&format!(
        "OnlyDedicatedMenuButtons={}\n",
        if cfg.only_dedicated_menu_buttons {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!("DisplayMonitor={}\n", cfg.display_monitor));
    content.push_str(&format!(
        "SongParsingThreads={}\n",
        cfg.song_parsing_threads
    ));
    content.push_str(&format!(
        "SoftwareRendererThreads={}\n",
        cfg.software_renderer_threads
    ));
    content.push_str(&format!("Theme={}\n", cfg.theme_flag.as_str()));
    content.push_str(&format!("AssistTickVolume={}\n", cfg.assist_tick_volume));
    content.push_str(&format!("SFXVolume={}\n", cfg.sfx_volume));
    content.push_str(&format!(
        "TranslatedTitles={}\n",
        if cfg.translated_titles { "1" } else { "0" }
    ));
    content.push_str(&format!("VideoRenderer={}\n", cfg.video_renderer));
    content.push_str(&format!("Vsync={}\n", if cfg.vsync { "1" } else { "0" }));
    content.push_str(&format!(
        "Windowed={}\n",
        if cfg.windowed { "1" } else { "0" }
    ));
    content.push('\n');

    // [Keymaps] – stable order with CamelCase keys.
    content.push_str("[Keymaps]\n");
    for act in ALL_VIRTUAL_ACTIONS {
        let key_name = action_to_ini_key(act);
        let mut tokens: Vec<String> = Vec::new();
        let mut i = 0;
        while let Some(binding) = keymap.binding_at(act, i) {
            tokens.push(binding_to_token(binding));
            i += 1;
        }
        let value = tokens.join(",");
        content.push_str(key_name);
        content.push('=');
        content.push_str(&value);
        content.push('\n');
    }

    // [Theme] – last section
    content.push('\n');
    content.push_str("[Theme]\n");
    content.push_str(&format!(
        "KeyboardFeatures={}\n",
        if cfg.keyboard_features { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "VideoBackgrounds={}\n",
        if cfg.show_video_backgrounds { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "MachineShowEvalSummary={}\n",
        if cfg.machine_show_eval_summary {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowGameOver={}\n",
        if cfg.machine_show_gameover { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "MachineShowNameEntry={}\n",
        if cfg.machine_show_name_entry {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectColor={}\n",
        if cfg.machine_show_select_color {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectPlayMode={}\n",
        if cfg.machine_show_select_play_mode {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectProfile={}\n",
        if cfg.machine_show_select_profile {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineShowSelectStyle={}\n",
        if cfg.machine_show_select_style {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!(
        "MachineEnableReplays={}\n",
        if cfg.machine_enable_replays { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "MachinePreferredStyle={}\n",
        cfg.machine_preferred_style.as_str()
    ));
    content.push_str(&format!(
        "MachinePreferredPlayMode={}\n",
        cfg.machine_preferred_play_mode.as_str()
    ));
    content.push_str(&format!(
        "ShowSelectMusicGameplayTimer={}\n",
        if cfg.show_select_music_gameplay_timer {
            "1"
        } else {
            "0"
        }
    ));
    content.push_str(&format!("SimplyLoveColor={}\n", cfg.simply_love_color));
    content.push_str(&format!(
        "ZmodRatingBoxText={}\n",
        if cfg.zmod_rating_box_text { "1" } else { "0" }
    ));
    content.push_str(&format!(
        "ShowBpmDecimal={}\n",
        if cfg.show_bpm_decimal { "1" } else { "0" }
    ));
    content.push('\n');

    queue_save_write(content);
}
