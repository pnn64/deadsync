use deadsync_config::prelude as config;
use deadsync_theme_simply_love::{
    SimplyLoveAdvancedConfigRequest, SimplyLoveCourseConfigRequest,
    SimplyLoveGameplayConfigRequest, SimplyLoveGameplayPadLights, SimplyLoveGraphOrientation,
    SimplyLoveLightsConfigRequest, SimplyLoveLightsDriver, SimplyLoveMachineConfigRequest,
    SimplyLoveNullOrDieConfigRequest, SimplyLoveNullOrDieGraph, SimplyLoveOnlineConfigRequest,
    SimplyLoveOptionsConfigRequest, SimplyLoveQrLoginPolicy, SimplyLoveSelectMusicConfigRequest,
    SimplyLoveSrpgShopFolder, SimplyLoveSyncKernel, SimplyLoveSyncKernelTarget,
};
use null_or_die::{BiasKernel, KernelTarget};

pub(super) fn execute_options(request: SimplyLoveOptionsConfigRequest) {
    use SimplyLoveOptionsConfigRequest as Request;

    match request {
        Request::GameDance => config::update_game_flag(config::GameFlag::Dance),
        Request::ThemeSimplyLove => config::update_theme_flag(config::ThemeFlag::SimplyLove),
        Request::Language(flag) => {
            config::update_language_flag(flag);
            let locale = deadsync_assets::language::resolve_locale(flag);
            deadsync_theme_simply_love::i18n::set_locale(deadsync_assets::language::load(&locale));
        }
        Request::LogLevel(level) => config::update_log_level(level),
        Request::LogToFile(enabled) => config::update_log_to_file(enabled),
        Request::GfxDebug(enabled) => config::update_gfx_debug(enabled),
        #[cfg(target_os = "windows")]
        Request::WindowsPadBackend(backend) => config::update_windows_gamepad_backend(backend),
        Request::UseFsrs(enabled) => config::update_use_fsrs(enabled),
        Request::ThreeKeyNavigation(enabled) => config::update_three_key_navigation(enabled),
        Request::ArcadeOptionsNavigation(enabled) => {
            config::update_arcade_options_navigation(enabled)
        }
        Request::OnlyDedicatedMenuButtons(enabled) => {
            config::update_only_dedicated_menu_buttons(enabled)
        }
        Request::SmxInput(enabled) => config::update_smx_input(enabled),
        Request::SmxPanelLights(enabled) => config::update_smx_panel_lights(enabled),
        Request::SmxManagesPadConfig(enabled) => config::update_smx_manages_pad_config(enabled),
        Request::SmxDefaultPadConfig(preset) => config::update_smx_default_pad_config(preset),
        Request::SmxDefaultLightBrightness(percent) => {
            config::update_smx_default_light_brightness(percent)
        }
        Request::SmxPadGifsPack(pack) => config::update_smx_pad_gifs_pack(pack),
        Request::SmxJudgeGifsPack(pack) => config::update_smx_judge_gifs_pack(pack),
        Request::SmxIdleLightsBlack(enabled) => config::update_smx_idle_lights_black(enabled),
        Request::VisualDelayMillis(millis) => {
            config::update_visual_delay_seconds(millis as f32 / 1000.0)
        }
        Request::InputDebounceMillis(millis) => {
            config::update_input_debounce_seconds(millis as f32 / 1000.0)
        }
    }
}

pub(super) fn execute_lights(request: SimplyLoveLightsConfigRequest) {
    use SimplyLoveLightsConfigRequest as Request;

    match request {
        Request::Driver(driver) => config::update_lights_driver(match driver {
            SimplyLoveLightsDriver::Off => config::LightsDriverKind::Off,
            SimplyLoveLightsDriver::Snek => config::LightsDriverKind::Snek,
            SimplyLoveLightsDriver::Litboard => config::LightsDriverKind::Litboard,
            SimplyLoveLightsDriver::Win32Serial => config::LightsDriverKind::Win32Serial,
            SimplyLoveLightsDriver::Fusion => config::LightsDriverKind::Fusion,
            SimplyLoveLightsDriver::Gpb => config::LightsDriverKind::Gpb,
            SimplyLoveLightsDriver::PacDrive => config::LightsDriverKind::PacDrive,
            SimplyLoveLightsDriver::PiuioLeds => config::LightsDriverKind::PiuioLeds,
            SimplyLoveLightsDriver::Itgio => config::LightsDriverKind::Itgio,
            SimplyLoveLightsDriver::HidBlueDot => config::LightsDriverKind::HidBlueDot,
            SimplyLoveLightsDriver::Stac2 => config::LightsDriverKind::Stac2,
            SimplyLoveLightsDriver::MinimaidHid => config::LightsDriverKind::MinimaidHid,
        }),
        Request::GameplayPadLights(mode) => config::update_lights_gameplay_pad_lights(match mode {
            SimplyLoveGameplayPadLights::Input => config::GameplayPadLightMode::Input,
            SimplyLoveGameplayPadLights::Chart => config::GameplayPadLightMode::Chart,
        }),
        Request::SimplifyBass(enabled) => config::update_lights_simplify_bass(enabled),
    }
}

pub(super) fn execute_advanced(request: SimplyLoveAdvancedConfigRequest) {
    use SimplyLoveAdvancedConfigRequest as Request;

    match request {
        Request::DefaultFailType(fail_type) => config::update_default_fail_type(fail_type),
        Request::BannerCache(enabled) => config::update_banner_cache(enabled),
        Request::CdTitleCache(enabled) => config::update_cdtitle_cache(enabled),
        Request::SongParsingThreads(threads) => config::update_song_parsing_threads(threads),
        Request::CacheSongs(enabled) => config::update_cache_songs(enabled),
        Request::FastLoad(enabled) => config::update_fastload(enabled),
    }
}

pub(super) fn execute_course(request: SimplyLoveCourseConfigRequest) {
    use SimplyLoveCourseConfigRequest as Request;

    match request {
        Request::ShowRandom(enabled) => config::update_show_random_courses(enabled),
        Request::ShowMostPlayed(enabled) => config::update_show_most_played_courses(enabled),
        Request::ShowIndividualScores(enabled) => {
            config::update_show_course_individual_scores(enabled)
        }
        Request::AutosubmitIndividual(enabled) => {
            config::update_autosubmit_course_scores_individually(enabled)
        }
    }
}

pub(super) fn execute_gameplay(request: SimplyLoveGameplayConfigRequest) {
    use SimplyLoveGameplayConfigRequest as Request;

    match request {
        Request::BackgroundBrightnessTenths(tenths) => {
            config::update_bg_brightness(tenths.min(10) as f32 / 10.0)
        }
        Request::CenterPlayerOneNotefield(enabled) => {
            config::update_center_1player_notefield(enabled)
        }
        Request::BannerMode(mode) => config::update_gameplay_banner_mode(mode),
        Request::ZmodRatingBoxText(enabled) => config::update_zmod_rating_box_text(enabled),
        Request::ShowBpmDecimal(enabled) => config::update_show_bpm_decimal(enabled),
        Request::BpmNearField(near_field) => {
            let position = if near_field {
                config::GameplayBpmPosition::NearField
            } else {
                config::GameplayBpmPosition::TopCenter
            };
            config::update_gameplay_bpm_position(position);
        }
        Request::DelayedBack(enabled) => config::update_delayed_back(enabled),
        Request::AutoScreenshotMask(mask) => config::update_auto_screenshot_eval(mask),
    }
}

pub(super) fn execute_machine(request: SimplyLoveMachineConfigRequest) {
    use SimplyLoveMachineConfigRequest as Request;

    match request {
        Request::ShowSelectProfile(enabled) => config::update_machine_show_select_profile(enabled),
        Request::ShowSelectColor(enabled) => config::update_machine_show_select_color(enabled),
        Request::ShowSelectStyle(enabled) => config::update_machine_show_select_style(enabled),
        Request::PreferredPlayStyle(style) => config::update_machine_preferred_style(style),
        Request::ShowSelectPlayMode(enabled) => {
            config::update_machine_show_select_play_mode(enabled)
        }
        Request::PreferredPlayMode(mode) => config::update_machine_preferred_play_mode(mode),
        Request::Font(font) => config::update_machine_font(font),
        Request::BarColor(color) => config::update_machine_bar_color(color),
        Request::EvaluationStyle(style) => config::update_machine_evaluation_style(style),
        Request::ShowEvaluationSummary(enabled) => {
            config::update_machine_show_eval_summary(enabled)
        }
        Request::NiceSound(enabled) => config::update_machine_nice_sound(enabled),
        Request::ShowNameEntry(enabled) => config::update_machine_show_name_entry(enabled),
        Request::ShowGameover(enabled) => config::update_machine_show_gameover(enabled),
        Request::MenuMusic(enabled) => config::update_menu_music(enabled),
        Request::VisualStyle(style) => config::update_visual_style(style),
        Request::SrpgVariant(variant) => config::update_srpg_variant(variant),
        Request::EnableReplays(enabled) => config::update_machine_enable_replays(enabled),
        Request::EnableHeartRateMonitors(enabled) => {
            config::update_machine_enable_heart_rate_monitors(enabled)
        }
        Request::AllowPerPlayerGlobalOffsets(enabled) => {
            config::update_machine_allow_per_player_global_offsets(enabled)
        }
        Request::PackIniOffsets(enabled) => config::update_machine_pack_ini_offsets(enabled),
        Request::DefaultSyncOffset(offset) => config::update_machine_default_sync_offset(offset),
        Request::KeyboardFeatures(enabled) => config::update_keyboard_features(enabled),
        Request::ShowVideoBackgrounds(enabled) => config::update_show_video_backgrounds(enabled),
        Request::RandomBackgroundMode(mode) => config::update_random_background_mode(mode),
        Request::ShowVersionOverlay(enabled) => config::update_show_version_overlay(enabled),
        Request::VersionOverlaySide(side) => config::update_version_overlay_side(side),
        Request::WriteCurrentScreen(enabled) => config::update_write_current_screen(enabled),
    }
}

pub(super) fn execute_null_or_die(request: SimplyLoveNullOrDieConfigRequest) {
    use SimplyLoveNullOrDieConfigRequest as Request;

    match request {
        Request::SyncGraph(graph) => config::update_null_or_die_sync_graph(match graph {
            SimplyLoveNullOrDieGraph::Frequency => config::SyncGraphMode::Frequency,
            SimplyLoveNullOrDieGraph::BeatIndex => config::SyncGraphMode::BeatIndex,
            SimplyLoveNullOrDieGraph::PostKernelFingerprint => {
                config::SyncGraphMode::PostKernelFingerprint
            }
        }),
        Request::GraphOrientation(orientation) => {
            config::update_null_or_die_graph_orientation(match orientation {
                SimplyLoveGraphOrientation::Vertical => config::GraphOrientation::Vertical,
                SimplyLoveGraphOrientation::Horizontal => config::GraphOrientation::Horizontal,
            })
        }
        Request::ConfidencePercent(percent) => {
            config::update_null_or_die_confidence_percent(percent)
        }
        Request::PackSyncThreads(threads) => config::update_null_or_die_pack_sync_threads(threads),
        Request::FingerprintTenths(tenths) => {
            config::update_null_or_die_fingerprint_ms(tenths as f64 / 10.0)
        }
        Request::WindowTenths(tenths) => config::update_null_or_die_window_ms(tenths as f64 / 10.0),
        Request::StepTenths(tenths) => config::update_null_or_die_step_ms(tenths as f64 / 10.0),
        Request::MagicOffsetTenths(tenths) => {
            config::update_null_or_die_magic_offset_ms(tenths as f64 / 10.0)
        }
        Request::KernelTarget(target) => config::update_null_or_die_kernel_target(match target {
            SimplyLoveSyncKernelTarget::Digest => KernelTarget::Digest,
            SimplyLoveSyncKernelTarget::Accumulator => KernelTarget::Accumulator,
        }),
        Request::Kernel(kernel) => config::update_null_or_die_kernel_type(match kernel {
            SimplyLoveSyncKernel::Rising => BiasKernel::Rising,
            SimplyLoveSyncKernel::Loudest => BiasKernel::Loudest,
        }),
        Request::FullSpectrogram(enabled) => config::update_null_or_die_full_spectrogram(enabled),
    }
}

pub(super) fn execute_online(request: SimplyLoveOnlineConfigRequest) {
    use SimplyLoveOnlineConfigRequest as Request;

    match request {
        Request::EnableGrooveStats(enabled) => config::update_enable_groovestats(enabled),
        Request::ShowSrpgShop(enabled) => config::update_show_srpg_shop(enabled),
        Request::SrpgShopFolder(folder) => config::update_srpg_shop_folder(match folder {
            SimplyLoveSrpgShopFolder::Unlocks => config::SrpgShopFolder::Unlocks,
            SimplyLoveSrpgShopFolder::Shops => config::SrpgShopFolder::Shops,
            SimplyLoveSrpgShopFolder::Faction => config::SrpgShopFolder::Faction,
        }),
        Request::EnableBoogieStats(enabled) => config::update_enable_boogiestats(enabled),
        Request::AutoPopulateScores(enabled) => config::update_auto_populate_gs_scores(enabled),
        Request::AutoDownloadUnlocks(enabled) => config::update_auto_download_unlocks(enabled),
        Request::SeparateUnlocksByPlayer(enabled) => {
            config::update_separate_unlocks_by_player(enabled)
        }
        Request::GrooveStatsQrLogin(policy) => {
            config::update_groovestats_qr_login_when(match policy {
                SimplyLoveQrLoginPolicy::Always => config::GrooveStatsQrLoginWhen::Always,
                SimplyLoveQrLoginPolicy::Sometimes => config::GrooveStatsQrLoginWhen::Sometimes,
                SimplyLoveQrLoginPolicy::Disabled => config::GrooveStatsQrLoginWhen::Disabled,
            })
        }
        Request::EnableArrowCloud(enabled) => config::update_enable_arrowcloud(enabled),
        Request::SubmitArrowCloudFails(enabled) => config::update_submit_arrowcloud_fails(enabled),
        Request::ArrowCloudQrLogin(policy) => {
            config::update_arrowcloud_qr_login_when(match policy {
                SimplyLoveQrLoginPolicy::Always => config::ArrowCloudQrLoginWhen::Always,
                SimplyLoveQrLoginPolicy::Sometimes => config::ArrowCloudQrLoginWhen::Sometimes,
                SimplyLoveQrLoginPolicy::Disabled => config::ArrowCloudQrLoginWhen::Disabled,
            })
        }
    }
}

pub(super) fn execute_select_music(request: SimplyLoveSelectMusicConfigRequest) {
    use SimplyLoveSelectMusicConfigRequest as Request;

    match request {
        Request::ShowBanners(enabled) => config::update_show_select_music_banners(enabled),
        Request::ShowVideoBanners(enabled) => {
            config::update_show_select_music_video_banners(enabled)
        }
        Request::ShowBreakdown(enabled) => config::update_show_select_music_breakdown(enabled),
        Request::BreakdownStyle(style) => config::update_select_music_breakdown_style(style),
        Request::TranslatedTitles(enabled) => config::update_translated_titles(enabled),
        Request::WheelSwitchSpeed(speed) => config::update_music_wheel_switch_speed(speed),
        Request::WheelStyle(style) => config::update_select_music_wheel_style(style),
        Request::SortBySeries(enabled) => config::update_sort_music_wheel_by_series(enabled),
        Request::SongSelectBackground(mode) => {
            config::update_select_music_song_select_bg_mode(mode)
        }
        Request::AllowProfileSwitch(enabled) => {
            config::update_allow_switch_profile_in_menu(enabled)
        }
        Request::ShowCdTitles(enabled) => config::update_show_select_music_cdtitles(enabled),
        Request::ShowWheelGrades(enabled) => config::update_show_music_wheel_grades(enabled),
        Request::ShowWheelLamps(enabled) => config::update_show_music_wheel_lamps(enabled),
        Request::ItlRankMode(mode) => config::update_select_music_itl_rank_mode(mode),
        Request::ItlWheelMode(mode) => config::update_select_music_itl_wheel_mode(mode),
        Request::NewPackMode(mode) => config::update_select_music_new_pack_mode(mode),
        Request::ShowFolderStats(enabled) => config::update_show_select_music_folder_stats(enabled),
        Request::PatternInfoMode(mode) => config::update_select_music_pattern_info_mode(mode),
        Request::StepArtistBoxMode(mode) => config::update_select_music_step_artist_box_mode(mode),
        Request::ShowPreviews(enabled) => config::update_show_select_music_previews(enabled),
        Request::ShowPreviewMarker(enabled) => {
            config::update_show_select_music_preview_marker(enabled)
        }
        Request::PreviewLoop(enabled) => config::update_select_music_preview_loop(enabled),
        Request::PreviewStartsImmediately(enabled) => {
            config::update_select_music_preview_starts_immediately(enabled)
        }
        Request::ShowGameplayTimer(enabled) => {
            config::update_show_select_music_gameplay_timer(enabled)
        }
        Request::ShowStageDisplay(enabled) => {
            config::update_show_select_music_stage_display(enabled)
        }
        Request::ShowScorebox(enabled) => config::update_show_select_music_scorebox(enabled),
        Request::ScoreboxPlacement(mode) => config::update_select_music_scorebox_placement(mode),
        Request::ScoreboxCycleMask(mask) => {
            config::update_select_music_scorebox_cycle_itg(mask & (1 << 0) != 0);
            config::update_select_music_scorebox_cycle_ex(mask & (1 << 1) != 0);
            config::update_select_music_scorebox_cycle_hard_ex(mask & (1 << 2) != 0);
            config::update_select_music_scorebox_cycle_tournaments(mask & (1 << 3) != 0);
        }
        Request::ChartInfoMask(mask) => {
            config::update_select_music_chart_info_peak_nps(mask & (1 << 0) != 0);
            config::update_select_music_chart_info_effective_bpm(mask & (1 << 1) != 0);
            config::update_select_music_chart_info_matrix_rating(mask & (1 << 2) != 0);
        }
    }
}
