use deadsync_config as config;
use deadsync_profile::compat as profile;
use deadsync_theme_simply_love::{
    SimplyLoveAdvancedConfigRequest, SimplyLoveCoinConfigRequest, SimplyLoveCourseConfigRequest,
    SimplyLoveGameplayConfigRequest, SimplyLoveGameplayPadLights, SimplyLoveGraphOrientation,
    SimplyLoveGraphOrigin, SimplyLoveLightsConfigRequest, SimplyLoveLightsDriver,
    SimplyLoveMachineConfigRequest, SimplyLoveNullOrDieConfigRequest, SimplyLoveNullOrDieGraph,
    SimplyLoveOnlineConfigRequest, SimplyLoveOptionsConfigRequest, SimplyLoveQrLoginPolicy,
    SimplyLoveSelectMusicConfigRequest, SimplyLoveSrpgShopFolder, SimplyLoveSyncKernel,
    SimplyLoveSyncKernelTarget, SimplyLoveTournamentConfigRequest,
};
use null_or_die::{BiasKernel, KernelTarget};

pub(super) fn execute_options(
    request: SimplyLoveOptionsConfigRequest,
    input: &mut deadsync_input::keymap::InputState,
) {
    use SimplyLoveOptionsConfigRequest as Request;

    match request {
        Request::Game(flag) => config::runtime_update::update_game_flag(flag),
        Request::ThemeSimplyLove => {
            config::runtime_update::update_theme_flag(config::theme::ThemeFlag::SimplyLove)
        }
        Request::Language(flag) => {
            config::runtime_update::update_language_flag(flag);
            let locale = deadsync_assets::language::resolve_locale(flag);
            deadsync_theme_simply_love::i18n::set_locale(deadsync_assets::language::load(&locale));
        }
        Request::LogLevel(level) => config::runtime_update::update_log_level(level),
        Request::LogToFile(enabled) => config::runtime_update::update_log_to_file(enabled),
        Request::GfxDebug(enabled) => config::runtime_update::update_gfx_debug(enabled),
        #[cfg(target_os = "windows")]
        Request::WindowsPadBackend(backend) => {
            config::runtime_update::update_windows_gamepad_backend(backend)
        }
        Request::UseFsrs(enabled) => config::runtime_update::update_use_fsrs(enabled),
        Request::ThreeKeyNavigation(enabled) => {
            config::runtime_update::update_three_key_navigation(enabled)
        }
        Request::ArcadeOptionsNavigation(enabled) => {
            config::runtime_update::update_arcade_options_navigation(enabled);
        }
        Request::OnlyDedicatedMenuButtons(enabled) => {
            config::runtime_update::update_only_dedicated_menu_buttons(enabled);
        }
        Request::SmxInput(enabled) => config::runtime_update::update_smx_input(enabled),
        Request::SmxPanelLights(enabled) => {
            config::runtime_update::update_smx_panel_lights(enabled)
        }
        Request::SmxManagesPadConfig(enabled) => {
            config::runtime_update::update_smx_manages_pad_config(enabled)
        }
        Request::SmxDefaultPadConfig(preset) => {
            config::runtime_update::update_smx_default_pad_config(preset)
        }
        Request::SmxDefaultLightBrightness(percent) => {
            profile::update_machine_default_light_brightness_from_config(percent);
        }
        Request::SmxPadGifsPack(pack) => config::runtime_update::update_smx_pad_gifs_pack(pack),
        Request::SmxJudgeGifsPack(pack) => config::runtime_update::update_smx_judge_gifs_pack(pack),
        Request::SmxIdleLightsBlack(enabled) => {
            config::runtime_update::update_smx_idle_lights_black(enabled)
        }
        Request::VisualDelayMillis(millis) => {
            config::runtime_update::update_visual_delay_seconds(millis as f32 / 1000.0);
        }
        Request::InputDebounceMillis(millis) => {
            let seconds = millis as f32 / 1000.0;
            config::runtime_update::update_input_debounce_seconds(seconds);
            input.set_debounce_seconds(seconds);
        }
    }
}

pub(super) fn execute_coin(request: SimplyLoveCoinConfigRequest) {
    use SimplyLoveCoinConfigRequest as Request;

    match request {
        Request::Mode(mode) => config::runtime_update::update_coin_mode(mode),
        Request::CoinsPerCredit(coins) => config::runtime_update::update_coins_per_credit(coins),
        Request::SongsPerPlay(songs) => config::runtime_update::update_songs_per_play(songs),
        Request::EventMode(enabled) => config::runtime_update::update_event_mode(enabled),
        Request::PremiumFreeMinutes(minutes) => {
            config::runtime_update::update_premium_free_minutes(minutes)
        }
        Request::PremiumFreeGraceSeconds(seconds) => {
            config::runtime_update::update_premium_free_grace_seconds(seconds);
        }
        Request::ContinueOnGiveUp(enabled) => {
            config::runtime_update::update_continue_on_give_up(enabled)
        }
        Request::LongSongSeconds(seconds) => {
            config::runtime_update::update_long_song_seconds(seconds)
        }
        Request::MarathonSongSeconds(seconds) => {
            config::runtime_update::update_marathon_song_seconds(seconds)
        }
    }
}

pub(super) fn execute_lights(request: SimplyLoveLightsConfigRequest) {
    use SimplyLoveLightsConfigRequest as Request;

    match request {
        Request::Driver(driver) => config::runtime_update::update_lights_driver(match driver {
            SimplyLoveLightsDriver::Off => deadsync_lights::DriverKind::Off,
            SimplyLoveLightsDriver::Snek => deadsync_lights::DriverKind::Snek,
            SimplyLoveLightsDriver::Litboard => deadsync_lights::DriverKind::Litboard,
            SimplyLoveLightsDriver::Win32Serial => deadsync_lights::DriverKind::Win32Serial,
            SimplyLoveLightsDriver::Fusion => deadsync_lights::DriverKind::Fusion,
            SimplyLoveLightsDriver::Gpb => deadsync_lights::DriverKind::Gpb,
            SimplyLoveLightsDriver::PacDrive => deadsync_lights::DriverKind::PacDrive,
            SimplyLoveLightsDriver::PiuioLeds => deadsync_lights::DriverKind::PiuioLeds,
            SimplyLoveLightsDriver::Itgio => deadsync_lights::DriverKind::Itgio,
            SimplyLoveLightsDriver::HidBlueDot => deadsync_lights::DriverKind::HidBlueDot,
            SimplyLoveLightsDriver::Stac2 => deadsync_lights::DriverKind::Stac2,
            SimplyLoveLightsDriver::MinimaidHid => deadsync_lights::DriverKind::MinimaidHid,
        }),
        Request::GameplayPadLights(mode) => {
            config::runtime_update::update_lights_gameplay_pad_lights(match mode {
                SimplyLoveGameplayPadLights::Input => deadsync_lights::GameplayPadLightMode::Input,
                SimplyLoveGameplayPadLights::Chart => deadsync_lights::GameplayPadLightMode::Chart,
            })
        }
        Request::SimplifyBass(enabled) => {
            config::runtime_update::update_lights_simplify_bass(enabled)
        }
    }
}

pub(super) fn execute_advanced(request: SimplyLoveAdvancedConfigRequest) {
    use SimplyLoveAdvancedConfigRequest as Request;

    match request {
        Request::DefaultFailType(fail_type) => {
            config::runtime_update::update_default_fail_type(fail_type)
        }
        Request::BannerCache(enabled) => config::runtime_update::update_banner_cache(enabled),
        Request::CdTitleCache(enabled) => config::runtime_update::update_cdtitle_cache(enabled),
        Request::SongParsingThreads(threads) => {
            config::runtime_update::update_song_parsing_threads(threads)
        }
        Request::CacheSongs(enabled) => config::runtime_update::update_cache_songs(enabled),
        Request::FastLoad(enabled) => config::runtime_update::update_fastload(enabled),
        Request::AllowSongDeletion(enabled) => {
            config::runtime_update::update_allow_song_deletion(enabled)
        }
    }
}

pub(super) fn execute_course(request: SimplyLoveCourseConfigRequest) {
    use SimplyLoveCourseConfigRequest as Request;

    match request {
        Request::ShowRandom(enabled) => config::runtime_update::update_show_random_courses(enabled),
        Request::ShowMostPlayed(enabled) => {
            config::runtime_update::update_show_most_played_courses(enabled)
        }
        Request::ShowIndividualScores(enabled) => {
            config::runtime_update::update_show_course_individual_scores(enabled);
        }
        Request::AutosubmitIndividual(enabled) => {
            config::runtime_update::update_autosubmit_course_scores_individually(enabled);
        }
        Request::AutosubmitPostFailPasses(enabled) => {
            config::runtime_update::update_autosubmit_course_post_fail_passes(enabled);
        }
    }
}

pub(super) fn execute_gameplay(request: SimplyLoveGameplayConfigRequest) {
    use SimplyLoveGameplayConfigRequest as Request;

    match request {
        Request::BackgroundBrightnessTenths(tenths) => {
            config::runtime_update::update_bg_brightness(f32::from(tenths.min(10)) / 10.0);
        }
        Request::CenterPlayerOneNotefield(enabled) => {
            config::runtime_update::update_center_1player_notefield(enabled);
        }
        Request::NoteScrollClock(clock) => config::runtime_update::update_note_scroll_clock(clock),
        Request::BannerMode(mode) => config::runtime_update::update_gameplay_banner_mode(mode),
        Request::ZmodRatingBoxText(enabled) => {
            config::runtime_update::update_zmod_rating_box_text(enabled)
        }
        Request::ShowBpmDecimal(enabled) => {
            config::runtime_update::update_show_bpm_decimal(enabled)
        }
        Request::BpmNearField(near_field) => {
            let position = if near_field {
                config::theme::GameplayBpmPosition::NearField
            } else {
                config::theme::GameplayBpmPosition::TopCenter
            };
            config::runtime_update::update_gameplay_bpm_position(position);
        }
        Request::DelayedBack(enabled) => config::runtime_update::update_delayed_back(enabled),
        Request::AutoScreenshotMask(mask) => {
            config::runtime_update::update_auto_screenshot_eval(mask)
        }
    }
}

pub(super) fn execute_tournament(request: SimplyLoveTournamentConfigRequest) {
    use SimplyLoveTournamentConfigRequest as Request;

    match request {
        Request::Enabled(enabled) => {
            config::runtime_update::update_tournament_mode_enabled(enabled)
        }
        Request::ScoringSystem(scoring_system) => {
            config::runtime_update::update_tournament_scoring_system(scoring_system);
        }
        Request::StepStats(show) => config::runtime_update::update_tournament_step_stats(show),
        Request::EnforceNoCmod(enabled) => {
            config::runtime_update::update_tournament_enforce_no_cmod(enabled)
        }
    }
}

pub(super) fn execute_machine(request: SimplyLoveMachineConfigRequest) {
    use SimplyLoveMachineConfigRequest as Request;

    match request {
        Request::ShowSelectProfile(enabled) => {
            config::runtime_update::update_machine_show_select_profile(enabled)
        }
        Request::ShowSelectColor(enabled) => {
            config::runtime_update::update_machine_show_select_color(enabled)
        }
        Request::ShowSelectStyle(enabled) => {
            config::runtime_update::update_machine_show_select_style(enabled)
        }
        Request::PreferredPlayStyle(style) => {
            config::runtime_update::update_machine_preferred_style(style)
        }
        Request::ShowSelectPlayMode(enabled) => {
            config::runtime_update::update_machine_show_select_play_mode(enabled);
        }
        Request::PreferredPlayMode(mode) => {
            config::runtime_update::update_machine_preferred_play_mode(mode)
        }
        Request::Font(font) => config::runtime_update::update_machine_font(font),
        Request::BarColor(color) => config::runtime_update::update_machine_bar_color(color),
        Request::EvaluationStyle(style) => {
            config::runtime_update::update_machine_evaluation_style(style)
        }
        Request::ShowEvaluationSummary(enabled) => {
            config::runtime_update::update_machine_show_eval_summary(enabled);
        }
        Request::EasterEggs(enabled) => config::runtime_update::update_machine_easter_eggs(enabled),
        Request::NiceSound(enabled) => config::runtime_update::update_machine_nice_sound(enabled),
        Request::ShowNameEntry(enabled) => {
            config::runtime_update::update_machine_show_name_entry(enabled)
        }
        Request::ShowGameover(enabled) => {
            config::runtime_update::update_machine_show_gameover(enabled)
        }
        Request::MenuMusic(enabled) => config::runtime_update::update_menu_music(enabled),
        Request::VisualStyle(style) => config::runtime_update::update_visual_style(style),
        Request::SrpgVariant(variant) => config::runtime_update::update_srpg_variant(variant),
        Request::EnableReplays(enabled) => {
            config::runtime_update::update_machine_enable_replays(enabled)
        }
        Request::EnableHeartRateMonitors(enabled) => {
            config::runtime_update::update_machine_enable_heart_rate_monitors(enabled);
        }
        Request::AllowPerPlayerGlobalOffsets(enabled) => {
            config::runtime_update::update_machine_allow_per_player_global_offsets(enabled);
        }
        Request::PackIniOffsets(enabled) => {
            config::runtime_update::update_machine_pack_ini_offsets(enabled)
        }
        Request::DefaultSyncOffset(offset) => {
            config::runtime_update::update_machine_default_sync_offset(offset)
        }
        Request::KeyboardFeatures(enabled) => {
            config::runtime_update::update_keyboard_features(enabled)
        }
        Request::ShowVideoBackgrounds(enabled) => {
            config::runtime_update::update_show_video_backgrounds(enabled)
        }
        Request::RandomBackgroundMode(mode) => {
            config::runtime_update::update_random_background_mode(mode)
        }
        Request::ShowVersionOverlay(enabled) => {
            config::runtime_update::update_show_version_overlay(enabled)
        }
        Request::VersionOverlaySide(side) => {
            config::runtime_update::update_version_overlay_side(side)
        }
        Request::ShowLocalIp(enabled) => config::runtime_update::update_show_local_ip(enabled),
        Request::WriteCurrentScreen(enabled) => {
            config::runtime_update::update_write_current_screen(enabled)
        }
    }
}

pub(super) fn execute_null_or_die(request: SimplyLoveNullOrDieConfigRequest) {
    use SimplyLoveNullOrDieConfigRequest as Request;

    match request {
        Request::SyncGraph(graph) => {
            config::runtime_update::update_null_or_die_sync_graph(match graph {
                SimplyLoveNullOrDieGraph::Frequency => config::theme::SyncGraphMode::Frequency,
                SimplyLoveNullOrDieGraph::BeatIndex => config::theme::SyncGraphMode::BeatIndex,
                SimplyLoveNullOrDieGraph::PostKernelFingerprint => {
                    config::theme::SyncGraphMode::PostKernelFingerprint
                }
            })
        }
        Request::GraphOrientation(orientation) => {
            config::runtime_update::update_null_or_die_graph_orientation(match orientation {
                SimplyLoveGraphOrientation::Vertical => null_or_die::GraphOrientation::Vertical,
                SimplyLoveGraphOrientation::Horizontal => null_or_die::GraphOrientation::Horizontal,
            });
        }
        Request::GraphOrigin(origin) => {
            config::runtime_update::update_null_or_die_graph_origin(match origin {
                SimplyLoveGraphOrigin::Bottom => config::null_or_die::GraphOrigin::Bottom,
                SimplyLoveGraphOrigin::Top => config::null_or_die::GraphOrigin::Top,
            })
        }
        Request::ConfidencePercent(percent) => {
            config::runtime_update::update_null_or_die_confidence_percent(percent);
        }
        Request::CacheResults(enabled) => {
            config::runtime_update::update_null_or_die_cache_results(enabled)
        }
        Request::PackSyncThreads(threads) => {
            config::runtime_update::update_null_or_die_pack_sync_threads(threads)
        }
        Request::FingerprintTenths(tenths) => {
            config::runtime_update::update_null_or_die_fingerprint_ms(f64::from(tenths) / 10.0);
        }
        Request::WindowTenths(tenths) => {
            config::runtime_update::update_null_or_die_window_ms(f64::from(tenths) / 10.0);
        }
        Request::StepTenths(tenths) => {
            config::runtime_update::update_null_or_die_step_ms(f64::from(tenths) / 10.0)
        }
        Request::MagicOffsetTenths(tenths) => {
            config::runtime_update::update_null_or_die_magic_offset_ms(f64::from(tenths) / 10.0);
        }
        Request::KernelTarget(target) => {
            config::runtime_update::update_null_or_die_kernel_target(match target {
                SimplyLoveSyncKernelTarget::Digest => KernelTarget::Digest,
                SimplyLoveSyncKernelTarget::Accumulator => KernelTarget::Accumulator,
            })
        }
        Request::Kernel(kernel) => {
            config::runtime_update::update_null_or_die_kernel_type(match kernel {
                SimplyLoveSyncKernel::Rising => BiasKernel::Rising,
                SimplyLoveSyncKernel::Loudest => BiasKernel::Loudest,
            })
        }
        Request::FullSpectrogram(enabled) => {
            config::runtime_update::update_null_or_die_full_spectrogram(enabled)
        }
    }
}

pub(super) fn execute_online(request: SimplyLoveOnlineConfigRequest) {
    use SimplyLoveOnlineConfigRequest as Request;

    match request {
        Request::EnableGrooveStats(enabled) => {
            config::runtime_update::update_enable_groovestats(enabled)
        }
        Request::ShowSrpgShop(enabled) => config::runtime_update::update_show_srpg_shop(enabled),
        Request::SrpgShopFolder(folder) => {
            config::runtime_update::update_srpg_shop_folder(match folder {
                SimplyLoveSrpgShopFolder::Unlocks => config::theme::SrpgShopFolder::Unlocks,
                SimplyLoveSrpgShopFolder::Shops => config::theme::SrpgShopFolder::Shops,
                SimplyLoveSrpgShopFolder::Faction => config::theme::SrpgShopFolder::Faction,
            })
        }
        Request::EnableBoogieStats(enabled) => {
            config::runtime_update::update_enable_boogiestats(enabled)
        }
        Request::AutoPopulateScores(enabled) => {
            config::runtime_update::update_auto_populate_gs_scores(enabled)
        }
        Request::AutoDownloadUnlocks(enabled) => {
            config::runtime_update::update_auto_download_unlocks(enabled)
        }
        Request::SeparateUnlocksByPlayer(enabled) => {
            config::runtime_update::update_separate_unlocks_by_player(enabled);
        }
        Request::GrooveStatsQrLogin(policy) => {
            config::runtime_update::update_groovestats_qr_login_when(match policy {
                SimplyLoveQrLoginPolicy::Always => config::theme::GrooveStatsQrLoginWhen::Always,
                SimplyLoveQrLoginPolicy::Sometimes => {
                    config::theme::GrooveStatsQrLoginWhen::Sometimes
                }
                SimplyLoveQrLoginPolicy::Disabled => {
                    config::theme::GrooveStatsQrLoginWhen::Disabled
                }
            });
        }
        Request::EnableArrowCloud(enabled) => {
            config::runtime_update::update_enable_arrowcloud(enabled)
        }
        Request::SubmitArrowCloudFails(enabled) => {
            config::runtime_update::update_submit_arrowcloud_fails(enabled)
        }
        Request::ShowArrowCloudResultDialogs(enabled) => {
            config::runtime_update::update_show_arrowcloud_result_dialogs(enabled);
        }
        Request::ArrowCloudQrLogin(policy) => {
            config::runtime_update::update_arrowcloud_qr_login_when(match policy {
                SimplyLoveQrLoginPolicy::Always => config::theme::ArrowCloudQrLoginWhen::Always,
                SimplyLoveQrLoginPolicy::Sometimes => {
                    config::theme::ArrowCloudQrLoginWhen::Sometimes
                }
                SimplyLoveQrLoginPolicy::Disabled => config::theme::ArrowCloudQrLoginWhen::Disabled,
            });
        }
    }
}

pub(super) fn execute_select_music(request: SimplyLoveSelectMusicConfigRequest) {
    use SimplyLoveSelectMusicConfigRequest as Request;

    match request {
        Request::ShowBanners(enabled) => {
            config::runtime_update::update_show_select_music_banners(enabled)
        }
        Request::ShowVideoBanners(enabled) => {
            config::runtime_update::update_show_select_music_video_banners(enabled);
        }
        Request::ShowBreakdown(enabled) => {
            config::runtime_update::update_show_select_music_breakdown(enabled)
        }
        Request::BreakdownStyle(style) => {
            config::runtime_update::update_select_music_breakdown_style(style)
        }
        Request::TranslatedTitles(enabled) => {
            config::runtime_update::update_translated_titles(enabled)
        }
        Request::WheelSwitchSpeed(speed) => {
            config::runtime_update::update_music_wheel_switch_speed(speed)
        }
        Request::WheelStyle(style) => {
            config::runtime_update::update_select_music_wheel_style(style)
        }
        Request::DifficultyColors(scheme) => {
            config::runtime_update::update_difficulty_color_scheme(scheme)
        }
        Request::HideInactiveSeries(enabled) => {
            config::runtime_update::update_hide_inactive_series(enabled)
        }
        Request::DefaultSort(sort) => {
            config::runtime_update::update_select_music_default_sort(sort)
        }
        Request::LastSort(sort) => config::runtime_update::update_select_music_last_sort(sort),
        Request::SeriesSource(source) => {
            config::runtime_update::update_select_music_series_source(source)
        }
        Request::SongSelectBackground(mode) => {
            config::runtime_update::update_select_music_song_select_bg_mode(mode);
        }
        Request::AllowProfileSwitch(enabled) => {
            config::runtime_update::update_allow_switch_profile_in_menu(enabled);
        }
        Request::ShowCdTitles(enabled) => {
            config::runtime_update::update_show_select_music_cdtitles(enabled)
        }
        Request::ShowWheelGrades(enabled) => {
            config::runtime_update::update_show_music_wheel_grades(enabled)
        }
        Request::ShowWheelLamps(enabled) => {
            config::runtime_update::update_show_music_wheel_lamps(enabled)
        }
        Request::ItlRankMode(mode) => {
            config::runtime_update::update_select_music_itl_rank_mode(mode)
        }
        Request::ItlWheelMode(mode) => {
            config::runtime_update::update_select_music_itl_wheel_mode(mode)
        }
        Request::NewPackMode(mode) => {
            config::runtime_update::update_select_music_new_pack_mode(mode)
        }
        Request::ShowFolderStats(enabled) => {
            config::runtime_update::update_show_select_music_folder_stats(enabled)
        }
        Request::PatternInfoMode(mode) => {
            config::runtime_update::update_select_music_pattern_info_mode(mode)
        }
        Request::StepArtistBoxMode(mode) => {
            config::runtime_update::update_select_music_step_artist_box_mode(mode)
        }
        Request::ShowPreviews(enabled) => {
            config::runtime_update::update_show_select_music_previews(enabled)
        }
        Request::ShowPreviewMarker(enabled) => {
            config::runtime_update::update_show_select_music_preview_marker(enabled);
        }
        Request::PreviewLoop(enabled) => {
            config::runtime_update::update_select_music_preview_loop(enabled)
        }
        Request::PreviewStartsImmediately(enabled) => {
            config::runtime_update::update_select_music_preview_starts_immediately(enabled);
        }
        Request::ShowGameplayTimer(enabled) => {
            config::runtime_update::update_show_select_music_gameplay_timer(enabled);
        }
        Request::ShowStageDisplay(enabled) => {
            config::runtime_update::update_show_select_music_stage_display(enabled);
        }
        Request::ShowScorebox(enabled) => {
            config::runtime_update::update_show_select_music_scorebox(enabled)
        }
        Request::ScoreboxPlacement(mode) => {
            config::runtime_update::update_select_music_scorebox_placement(mode)
        }
        Request::ScoreboxCycleMask(mask) => {
            config::runtime_update::update_select_music_scorebox_cycle_itg(mask & (1 << 0) != 0);
            config::runtime_update::update_select_music_scorebox_cycle_ex(mask & (1 << 1) != 0);
            config::runtime_update::update_select_music_scorebox_cycle_hard_ex(
                mask & (1 << 2) != 0,
            );
            config::runtime_update::update_select_music_scorebox_cycle_tournaments(
                mask & (1 << 3) != 0,
            );
        }
        Request::ChartInfoMask(mask) => {
            config::runtime_update::update_select_music_chart_info_peak_nps(mask & (1 << 0) != 0);
            config::runtime_update::update_select_music_chart_info_effective_bpm(
                mask & (1 << 1) != 0,
            );
            config::runtime_update::update_select_music_chart_info_matrix_rating(
                mask & (1 << 2) != 0,
            );
        }
    }
}
