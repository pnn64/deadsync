use super::*;

pub(super) fn normalize_machine_default_noteskin(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DEFAULT_MACHINE_NOTESKIN.to_string();
    }
    trimmed.to_ascii_lowercase()
}

fn normalize_additional_song_folders(raw: &str) -> String {
    let mut out = String::new();
    for path in raw
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(path);
    }
    out
}

fn parse_bool_str(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn load_additional_song_folders(conf: &SimpleIni) -> String {
    let read_only = conf
        .get("Options", "AdditionalSongFoldersReadOnly")
        .unwrap_or_default();
    let writable_raw = conf
        .get("Options", "AdditionalSongFoldersWritable")
        .unwrap_or_default();
    let deprecated = conf
        .get("Options", "AdditionalSongFolders")
        .unwrap_or_default();
    let writable = if writable_raw.trim().is_empty() {
        deprecated
    } else {
        writable_raw
    };

    if read_only.trim().is_empty() {
        return normalize_additional_song_folders(&writable);
    }
    if writable.trim().is_empty() {
        return normalize_additional_song_folders(&read_only);
    }

    let mut combined = String::with_capacity(read_only.len() + writable.len() + 1);
    combined.push_str(&read_only);
    combined.push(',');
    combined.push_str(&writable);
    normalize_additional_song_folders(&combined)
}

pub fn bootstrap_log_to_file() -> bool {
    let mut conf = SimpleIni::new();
    let default = Config::default().log_to_file;
    if conf.load(CONFIG_PATH).is_err() {
        return default;
    }
    conf.get("Options", "LogToFile")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default)
}

#[inline(always)]
fn create_default_config_file() -> Result<(), std::io::Error> {
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

pub fn load() {
    // --- Load main deadsync.ini ---
    if !std::path::Path::new(CONFIG_PATH).exists()
        && let Err(e) = create_default_config_file()
    {
        warn!("Failed to create default config file: {e}");
    }

    let mut conf = SimpleIni::new();
    match conf.load(CONFIG_PATH) {
        Ok(()) => {
            {
                let noteskin = conf
                    .get("Options", "DefaultNoteSkin")
                    .map(|v| normalize_machine_default_noteskin(&v))
                    .unwrap_or_else(|| DEFAULT_MACHINE_NOTESKIN.to_string());
                *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = noteskin;
                *ADDITIONAL_SONG_FOLDERS.lock().unwrap() = load_additional_song_folders(&conf);
            }

            // This block populates the global CONFIG struct from the file,
            // using default values for any missing keys.
            {
                let mut cfg = lock_config();
                let default = Config::default();

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
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.translated_titles);
                cfg.bg_brightness = conf
                    .get("Options", "BGBrightness")
                    .and_then(|v| v.parse::<f32>().ok())
                    .map_or(default.bg_brightness, |v| v.clamp(0.0, 1.0));
                cfg.center_1player_notefield = conf
                    .get("Options", "Center1Player")
                    .or_else(|| conf.get("Options", "CenteredP1Notefield"))
                    .map(|v| v.trim().to_ascii_lowercase())
                    .and_then(|v| match v.as_str() {
                        "1" | "true" | "yes" | "on" => Some(true),
                        "0" | "false" | "no" | "off" => Some(false),
                        _ => None,
                    })
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
                cfg.null_or_die_sync_graph = conf
                    .get("Options", "NullOrDieSyncGraph")
                    .and_then(|v| SyncGraphMode::from_str(&v).ok())
                    .unwrap_or(default.null_or_die_sync_graph);
                cfg.null_or_die_confidence_percent = conf
                    .get("Options", "NullOrDieConfidencePercent")
                    .and_then(|v| v.parse::<u8>().ok())
                    .map(clamp_null_or_die_confidence_percent)
                    .unwrap_or(default.null_or_die_confidence_percent);
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
                cfg.simply_love_color = conf
                    .get("Theme", "SimplyLoveColor")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(default.simply_love_color);
                cfg.show_select_music_gameplay_timer = conf
                    .get("Theme", "ShowSelectMusicGameplayTimer")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.show_select_music_gameplay_timer);
                cfg.keyboard_features = conf
                    .get("Theme", "KeyboardFeatures")
                    .and_then(|v| parse_bool_str(&v))
                    .unwrap_or(default.keyboard_features);
                cfg.show_video_backgrounds = conf
                    .get("Theme", "VideoBackgrounds")
                    .and_then(|v| parse_bool_str(&v))
                    .unwrap_or(default.show_video_backgrounds);
                cfg.machine_show_eval_summary = conf
                    .get("Theme", "MachineShowEvalSummary")
                    .and_then(|v| parse_bool_str(&v))
                    .unwrap_or(default.machine_show_eval_summary);
                cfg.machine_show_name_entry = conf
                    .get("Theme", "MachineShowNameEntry")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_show_name_entry);
                cfg.machine_show_gameover = conf
                    .get("Theme", "MachineShowGameOver")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_show_gameover);
                cfg.machine_show_select_profile = conf
                    .get("Theme", "MachineShowSelectProfile")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_show_select_profile);
                cfg.machine_show_select_color = conf
                    .get("Theme", "MachineShowSelectColor")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_show_select_color);
                cfg.machine_show_select_style = conf
                    .get("Theme", "MachineShowSelectStyle")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_show_select_style);
                cfg.machine_show_select_play_mode = conf
                    .get("Theme", "MachineShowSelectPlayMode")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_show_select_play_mode);
                cfg.machine_enable_replays = conf
                    .get("Theme", "MachineEnableReplays")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.machine_enable_replays);
                cfg.machine_preferred_style = conf
                    .get("Theme", "MachinePreferredStyle")
                    .and_then(|v| MachinePreferredPlayStyle::from_str(&v).ok())
                    .unwrap_or(default.machine_preferred_style);
                cfg.machine_preferred_play_mode = conf
                    .get("Theme", "MachinePreferredPlayMode")
                    .and_then(|v| MachinePreferredPlayMode::from_str(&v).ok())
                    .unwrap_or(default.machine_preferred_play_mode);
                cfg.zmod_rating_box_text = conf
                    .get("Theme", "ZmodRatingBoxText")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.zmod_rating_box_text);
                cfg.show_bpm_decimal = conf
                    .get("Theme", "ShowBpmDecimal")
                    .map(|v| v.trim().to_string())
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else if v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                            || v.eq_ignore_ascii_case("on")
                        {
                            Some(true)
                        } else if v.eq_ignore_ascii_case("false")
                            || v.eq_ignore_ascii_case("no")
                            || v.eq_ignore_ascii_case("off")
                        {
                            Some(false)
                        } else {
                            v.parse::<u8>().ok().map(|n| n != 0)
                        }
                    })
                    .unwrap_or(default.show_bpm_decimal);

                sync_audio_mix_levels_from_config(&cfg);
                logging::set_file_logging_enabled(cfg.log_to_file);
                info!("Configuration loaded from '{CONFIG_PATH}'.");
            } // Lock on CONFIG is released here.

            // Load keymaps from the same INI and publish globally.
            let km = load_keymap_from_ini_local(&conf);
            crate::core::input::set_keymap(km);

            // Only write [Options]/[Theme] if any of those keys are missing.
            let missing_opts = {
                let has = |sec: &str, key: &str| conf.get(sec, key).is_some();
                let mut miss = false;
                let options_keys = [
                    "AudioOutputDevice",
                    "AudioOutputMode",
                    "AudioSampleRateHz",
                    "AdditionalSongFolders",
                    "AutoDownloadUnlocks",
                    "AutoPopulateGrooveStatsScores",
                    "BGBrightness",
                    "BannerCache",
                    "CacheSongs",
                    "CDTitleCache",
                    "Center1Player",
                    "CourseAutosubmitScoresIndividually",
                    "CourseShowIndividualScores",
                    "CourseShowMostPlayed",
                    "CourseShowRandom",
                    "DefaultFailType",
                    "DefaultNoteSkin",
                    "DisplayHeight",
                    "DisplayWidth",
                    "FastLoad",
                    "EnableArrowCloud",
                    "EnableBoogieStats",
                    "EnableGrooveStats",
                    "FullscreenType",
                    "Game",
                    "GamepadBackend",
                    "GfxDebug",
                    "GlobalOffsetSeconds",
                    "Language",
                    "LogLevel",
                    "LogToFile",
                    "LinuxAudioBackend",
                    "MaxFps",
                    "MasterVolume",
                    "MenuMusic",
                    "MineHitSound",
                    "MusicVolume",
                    "MusicWheelSwitchSpeed",
                    "SongParsingThreads",
                    "RateModPreservesPitch",
                    "SelectMusicBreakdown",
                    "SelectMusicShowBanners",
                    "SelectMusicShowVideoBanners",
                    "SelectMusicShowBreakdown",
                    "SelectMusicShowCDTitles",
                    "SelectMusicWheelGrades",
                    "SelectMusicWheelLamps",
                    "SelectMusicPreviews",
                    "SelectMusicPreviewLoop",
                    "SelectMusicPatternInfo",
                    "SelectMusicScorebox",
                    "SelectMusicScoreboxCycleItg",
                    "SelectMusicScoreboxCycleEx",
                    "SelectMusicScoreboxCycleHardEx",
                    "SelectMusicScoreboxCycleTournaments",
                    "SeparateUnlocksByPlayer",
                    "ShowStats",
                    "ShowStatsMode",
                    "SmoothHistogram",
                    "InputDebounceTime",
                    "OnlyDedicatedMenuButtons",
                    "AssistTickVolume",
                    "SFXVolume",
                    "SoftwareRendererThreads",
                    "Theme",
                    "TranslatedTitles",
                    "VideoRenderer",
                    "VisualDelaySeconds",
                    "Vsync",
                    "Windowed",
                ];
                for k in options_keys {
                    if !has("Options", k) {
                        miss = true;
                        break;
                    }
                }
                if !miss && !has("Theme", "SimplyLoveColor") {
                    miss = true;
                }
                if !miss && !has("Theme", "ShowSelectMusicGameplayTimer") {
                    miss = true;
                }
                if !miss && !has("Theme", "KeyboardFeatures") {
                    miss = true;
                }
                if !miss && !has("Theme", "VideoBackgrounds") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowEvalSummary") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowGameOver") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowNameEntry") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowSelectColor") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowSelectPlayMode") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowSelectProfile") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineShowSelectStyle") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachineEnableReplays") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachinePreferredStyle") {
                    miss = true;
                }
                if !miss && !has("Theme", "MachinePreferredPlayMode") {
                    miss = true;
                }
                if !miss && !has("Theme", "ZmodRatingBoxText") {
                    miss = true;
                }
                if !miss && !has("Theme", "ShowBpmDecimal") {
                    miss = true;
                }
                miss
            };
            if missing_opts {
                save_without_keymaps();
                info!("'{CONFIG_PATH}' updated with default values for any missing fields.");
            } else {
                info!("Configuration OK; no write needed.");
            }
        }
        Err(e) => {
            warn!("Failed to load '{CONFIG_PATH}': {e}. Using default values.");
            *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = DEFAULT_MACHINE_NOTESKIN.to_string();
            *ADDITIONAL_SONG_FOLDERS.lock().unwrap() = String::new();
        }
    }
    let mut dedicated = get().only_dedicated_menu_buttons;
    if dedicated && !crate::core::input::any_player_has_dedicated_menu_buttons() {
        warn!(
            "only_dedicated_menu_buttons is enabled but no player has dedicated menu buttons mapped — disabling."
        );
        dedicated = false;
        lock_config().only_dedicated_menu_buttons = false;
    }
    crate::core::input::set_only_dedicated_menu_buttons(dedicated);
    crate::core::input::set_input_debounce_seconds(get().input_debounce_seconds);
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
