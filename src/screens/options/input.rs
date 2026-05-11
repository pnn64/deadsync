use super::*;

// Small helpers to let the app dispatcher manage hold-to-scroll without exposing fields
pub fn on_nav_press(state: &mut State, dir: NavDirection) {
    state.nav_key_held_direction = Some(dir);
    state.nav_key_held_since = Some(Instant::now());
    state.nav_key_last_scrolled_at = Some(Instant::now());
}

pub fn on_nav_release(state: &mut State, dir: NavDirection) {
    if state.nav_key_held_direction == Some(dir) {
        state.nav_key_held_direction = None;
        state.nav_key_held_since = None;
        state.nav_key_last_scrolled_at = None;
    }
}

pub(super) fn on_lr_press(state: &mut State, delta: isize) {
    let now = Instant::now();
    state.nav_lr_held_direction = Some(delta);
    state.nav_lr_held_since = Some(now);
    state.nav_lr_last_adjusted_at = Some(now);
}

pub(super) fn on_lr_release(state: &mut State, delta: isize) {
    if state.nav_lr_held_direction == Some(delta) {
        state.nav_lr_held_direction = None;
        state.nav_lr_held_since = None;
        state.nav_lr_last_adjusted_at = None;
    }
}

pub(super) fn apply_submenu_choice_delta(
    state: &mut State,
    asset_manager: &AssetManager,
    delta: isize,
    wrap: NavWrap,
) -> Option<ScreenAction> {
    if !matches!(state.submenu_transition, SubmenuTransition::None) {
        return None;
    }
    let kind = match state.view {
        OptionsView::Submenu(k) => k,
        _ => return None,
    };
    let rows = submenu_rows(kind);
    if rows.is_empty() {
        return None;
    }
    let Some(row_index) = submenu_visible_row_to_actual(state, kind, state.sub_selected) else {
        // Exit row – no choices to change.
        return None;
    };

    if let Some(row) = rows.get(row_index) {
        // Block cycling disabled rows (e.g. dedicated menu buttons when unmapped).
        if is_submenu_row_disabled(kind, row.id) {
            return None;
        }
        if matches!(kind, SubmenuKind::Sound) {
            match row.id {
                SubRowId::MasterVolume => {
                    if adjust_ms_value(
                        &mut state.master_volume_pct,
                        delta,
                        VOLUME_MIN_PERCENT,
                        VOLUME_MAX_PERCENT,
                    ) {
                        config::update_master_volume(state.master_volume_pct as u8);
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                SubRowId::SfxVolume => {
                    if adjust_ms_value(
                        &mut state.sfx_volume_pct,
                        delta,
                        VOLUME_MIN_PERCENT,
                        VOLUME_MAX_PERCENT,
                    ) {
                        config::update_sfx_volume(state.sfx_volume_pct as u8);
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                SubRowId::AssistTickVolume => {
                    if adjust_ms_value(
                        &mut state.assist_tick_volume_pct,
                        delta,
                        VOLUME_MIN_PERCENT,
                        VOLUME_MAX_PERCENT,
                    ) {
                        config::update_assist_tick_volume(state.assist_tick_volume_pct as u8);
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                SubRowId::MusicVolume => {
                    if adjust_ms_value(
                        &mut state.music_volume_pct,
                        delta,
                        VOLUME_MIN_PERCENT,
                        VOLUME_MAX_PERCENT,
                    ) {
                        config::update_music_volume(state.music_volume_pct as u8);
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                _ => {}
            }
        }
        if matches!(kind, SubmenuKind::Sound) && row.id == SubRowId::GlobalOffset {
            if adjust_ms_value(
                &mut state.global_offset_ms,
                delta,
                GLOBAL_OFFSET_MIN_MS,
                GLOBAL_OFFSET_MAX_MS,
            ) {
                config::update_global_offset(state.global_offset_ms as f32 / 1000.0);
                audio::play_sfx("assets/sounds/change_value.ogg");
                clear_render_cache(state);
            }
            return None;
        }
        if matches!(kind, SubmenuKind::Graphics) && row.id == SubRowId::MaxFpsValue {
            if adjust_max_fps_value_choice(state, delta, wrap) {
                audio::play_sfx("assets/sounds/change_value.ogg");
                clear_render_cache(state);
            }
            return None;
        }
        if matches!(kind, SubmenuKind::Graphics) && row.id == SubRowId::VisualDelay {
            if adjust_ms_value(
                &mut state.visual_delay_ms,
                delta,
                VISUAL_DELAY_MIN_MS,
                VISUAL_DELAY_MAX_MS,
            ) {
                config::update_visual_delay_seconds(state.visual_delay_ms as f32 / 1000.0);
                audio::play_sfx("assets/sounds/change_value.ogg");
                clear_render_cache(state);
            }
            return None;
        }
        if matches!(kind, SubmenuKind::InputBackend) && row.id == SubRowId::Debounce {
            if adjust_ms_value(
                &mut state.input_debounce_ms,
                delta,
                INPUT_DEBOUNCE_MIN_MS,
                INPUT_DEBOUNCE_MAX_MS,
            ) {
                config::update_input_debounce_seconds(state.input_debounce_ms as f32 / 1000.0);
                audio::play_sfx("assets/sounds/change_value.ogg");
                clear_render_cache(state);
            }
            return None;
        }
        if matches!(kind, SubmenuKind::NullOrDieOptions) {
            match row.id {
                SubRowId::Fingerprint => {
                    if adjust_tenths_value(
                        &mut state.null_or_die_fingerprint_tenths,
                        delta,
                        NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS,
                        NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS,
                    ) {
                        config::update_null_or_die_fingerprint_ms(f64_from_tenths(
                            state.null_or_die_fingerprint_tenths,
                        ));
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                SubRowId::Window => {
                    if adjust_tenths_value(
                        &mut state.null_or_die_window_tenths,
                        delta,
                        NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS,
                        NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS,
                    ) {
                        config::update_null_or_die_window_ms(f64_from_tenths(
                            state.null_or_die_window_tenths,
                        ));
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                SubRowId::Step => {
                    if adjust_tenths_value(
                        &mut state.null_or_die_step_tenths,
                        delta,
                        NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS,
                        NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS,
                    ) {
                        config::update_null_or_die_step_ms(f64_from_tenths(
                            state.null_or_die_step_tenths,
                        ));
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                SubRowId::MagicOffset => {
                    if adjust_tenths_value(
                        &mut state.null_or_die_magic_offset_tenths,
                        delta,
                        NULL_OR_DIE_MAGIC_OFFSET_MIN_TENTHS,
                        NULL_OR_DIE_MAGIC_OFFSET_MAX_TENTHS,
                    ) {
                        config::update_null_or_die_magic_offset_ms(f64_from_tenths(
                            state.null_or_die_magic_offset_tenths,
                        ));
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                _ => {}
            }
        }
    }

    let choices = row_choices(state, kind, rows, row_index);
    let num_choices = choices.len();
    if num_choices == 0 {
        return None;
    }
    let mut action: Option<ScreenAction> = None;
    if row_index >= submenu_choice_indices(state, kind).len()
        || row_index >= submenu_cursor_indices(state, kind).len()
    {
        return None;
    }
    let choice_index =
        submenu_cursor_indices(state, kind)[row_index].min(num_choices.saturating_sub(1));
    let cur = choice_index as isize;
    let n = num_choices as isize;
    let raw = cur + delta;
    let mut new_index = match wrap {
        NavWrap::Wrap => raw.rem_euclid(n) as usize,
        NavWrap::Clamp => raw.clamp(0, n - 1) as usize,
    };
    if new_index >= num_choices {
        new_index = num_choices.saturating_sub(1);
    }
    if new_index == choice_index {
        return None;
    }
    let selected_choice = choices
        .get(new_index)
        .map(|choice| choice.as_ref().to_string());
    drop(choices);

    submenu_choice_indices_mut(state, kind)[row_index] = new_index;
    submenu_cursor_indices_mut(state, kind)[row_index] = new_index;
    if let Some(layout) = submenu_row_layout(state, asset_manager, kind, row_index)
        && layout.inline_row
        && let Some(&x) = layout.centers.get(new_index)
    {
        state.sub_inline_x = x;
    }
    audio::play_sfx("assets/sounds/change_value.ogg");

    if matches!(kind, SubmenuKind::System) {
        let row = &rows[row_index];
        match row.id {
            SubRowId::Game => config::update_game_flag(config::GameFlag::Dance),
            SubRowId::Theme => config::update_theme_flag(config::ThemeFlag::SimplyLove),
            SubRowId::Language => {
                let flag = language_flag_from_choice(new_index);
                config::update_language_flag(flag);
                assets::i18n::set_locale(&assets::i18n::resolve_locale(flag));
            }
            SubRowId::LogLevel => config::update_log_level(LogLevel::from_choice(new_index)),
            SubRowId::LogFile => config::update_log_to_file(new_index == 1),
            SubRowId::DefaultNoteSkin => {
                if let Some(skin_name) = selected_choice.as_deref() {
                    profile::update_machine_default_noteskin(profile::NoteSkin::new(skin_name));
                }
            }
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::Graphics) {
        let row = &rows[row_index];
        if row.id == SubRowId::DisplayAspectRatio {
            let (cur_w, cur_h) = selected_resolution(state);
            rebuild_resolution_choices(state, cur_w, cur_h);
        }
        if row.id == SubRowId::DisplayResolution {
            rebuild_refresh_rate_choices(state);
        }
        if row.id == SubRowId::DisplayMode {
            let (cur_w, cur_h) = selected_resolution(state);
            rebuild_resolution_choices(state, cur_w, cur_h);
        }
        if row.id == SubRowId::RefreshRate && state.max_fps_at_load == 0 && !max_fps_enabled(state)
        {
            seed_max_fps_value_choice(state, 0);
        }
        if row.id == SubRowId::MaxFps && yes_no_from_choice(new_index) && state.max_fps_at_load == 0
        {
            seed_max_fps_value_choice(state, 0);
        }
        if row.id == SubRowId::ShowStats {
            let mode = new_index.min(3) as u8;
            action = Some(ScreenAction::UpdateShowOverlay(mode));
        }
        if row.id == SubRowId::ValidationLayers {
            config::update_gfx_debug(yes_no_from_choice(new_index));
        }
        if row.id == SubRowId::SoftwareRendererThreads {
            let threads = software_thread_from_choice(&state.software_thread_choices, new_index);
            config::update_software_renderer_threads(threads);
        }
    } else if matches!(kind, SubmenuKind::InputBackend) {
        let row = &rows[row_index];
        if row.id == SubRowId::GamepadBackend {
            #[cfg(target_os = "windows")]
            {
                config::update_windows_gamepad_backend(windows_backend_from_choice(new_index));
            }
        }
        if row.id == SubRowId::UseFsrs {
            config::update_use_fsrs(yes_no_from_choice(new_index));
        }
        if row.id == SubRowId::MenuNavigation {
            config::update_three_key_navigation(new_index == 1);
        }
        if row.id == SubRowId::OptionsNavigation {
            config::update_arcade_options_navigation(new_index == 1);
        }
        if row.id == SubRowId::MenuButtons {
            state.pending_dedicated_menu_buttons = Some(new_index == 1);
        }
    } else if matches!(kind, SubmenuKind::Lights) {
        let row = &rows[row_index];
        if row.id == SubRowId::LightsDriver {
            config::update_lights_driver(lights_driver_from_choice(new_index));
        }
    } else if matches!(kind, SubmenuKind::Machine) {
        let row = &rows[row_index];
        let enabled = new_index == 1;
        match row.id {
            SubRowId::SelectProfile => config::update_machine_show_select_profile(enabled),
            SubRowId::SelectColor => config::update_machine_show_select_color(enabled),
            SubRowId::PreferredColor => {
                state.active_color_index = new_index as i32;
                config::update_simply_love_color(state.active_color_index);
            }
            SubRowId::SelectStyle => config::update_machine_show_select_style(enabled),
            SubRowId::PreferredStyle => config::update_machine_preferred_style(
                MachinePreferredPlayStyle::from_choice(new_index),
            ),
            SubRowId::SelectPlayMode => config::update_machine_show_select_play_mode(enabled),
            SubRowId::PreferredMode => config::update_machine_preferred_play_mode(
                MachinePreferredPlayMode::from_choice(new_index),
            ),
            SubRowId::Font => config::update_machine_font(MachineFont::from_choice(new_index)),
            SubRowId::EvalSummary => config::update_machine_show_eval_summary(enabled),
            SubRowId::NameEntry => config::update_machine_show_name_entry(enabled),
            SubRowId::GameoverScreen => config::update_machine_show_gameover(enabled),
            SubRowId::MenuMusic => config::update_menu_music(enabled),
            SubRowId::VisualStyle => {
                config::update_visual_style(VisualStyle::from_choice(new_index))
            }
            SubRowId::Replays => config::update_machine_enable_replays(enabled),
            SubRowId::PerPlayerGlobalOffsets => {
                config::update_machine_allow_per_player_global_offsets(enabled)
            }
            SubRowId::KeyboardFeatures => config::update_keyboard_features(enabled),
            SubRowId::VideoBgs => config::update_show_video_backgrounds(enabled),
            SubRowId::WriteCurrentScreen => config::update_write_current_screen(enabled),
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::Advanced) {
        let row = &rows[row_index];
        if row.id == SubRowId::DefaultFailType {
            config::update_default_fail_type(DefaultFailType::from_choice(new_index));
        } else if row.id == SubRowId::BannerCache {
            config::update_banner_cache(new_index == 1);
        } else if row.id == SubRowId::CdTitleCache {
            config::update_cdtitle_cache(new_index == 1);
        } else if row.id == SubRowId::SongParsingThreads {
            let threads = software_thread_from_choice(&state.software_thread_choices, new_index);
            config::update_song_parsing_threads(threads);
        } else if row.id == SubRowId::CacheSongs {
            config::update_cache_songs(new_index == 1);
        } else if row.id == SubRowId::FastLoad {
            config::update_fastload(new_index == 1);
        }
    } else if matches!(kind, SubmenuKind::NullOrDieOptions) {
        let row = &rows[row_index];
        if row.id == SubRowId::SyncGraph {
            config::update_null_or_die_sync_graph(SyncGraphMode::from_choice(new_index));
        } else if row.id == SubRowId::SyncConfidence {
            config::update_null_or_die_confidence_percent(sync_confidence_from_choice(new_index));
        } else if row.id == SubRowId::PackSyncThreads {
            let threads = software_thread_from_choice(&state.software_thread_choices, new_index);
            config::update_null_or_die_pack_sync_threads(threads);
        } else if row.id == SubRowId::KernelTarget {
            config::update_null_or_die_kernel_target(::null_or_die::KernelTarget::from_choice(
                new_index,
            ));
        } else if row.id == SubRowId::KernelType {
            config::update_null_or_die_kernel_type(::null_or_die::BiasKernel::from_choice(
                new_index,
            ));
        } else if row.id == SubRowId::FullSpectrogram {
            config::update_null_or_die_full_spectrogram(yes_no_from_choice(new_index));
        }
    } else if matches!(kind, SubmenuKind::Course) {
        let row = &rows[row_index];
        let enabled = yes_no_from_choice(new_index);
        match row.id {
            SubRowId::ShowRandomCourses => config::update_show_random_courses(enabled),
            SubRowId::ShowMostPlayed => config::update_show_most_played_courses(enabled),
            SubRowId::ShowIndividualScores => config::update_show_course_individual_scores(enabled),
            SubRowId::AutosubmitIndividual => {
                config::update_autosubmit_course_scores_individually(enabled)
            }
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::Gameplay) {
        let row = &rows[row_index];
        if row.id == SubRowId::BgBrightness {
            config::update_bg_brightness(bg_brightness_from_choice(new_index));
        } else if row.id == SubRowId::CenteredP1Notefield {
            config::update_center_1player_notefield(new_index == 1);
        } else if row.id == SubRowId::ZmodRatingBox {
            config::update_zmod_rating_box_text(new_index == 1);
        } else if row.id == SubRowId::BpmDecimal {
            config::update_show_bpm_decimal(new_index == 1);
        }
    } else if matches!(kind, SubmenuKind::Sound) {
        let row = &rows[row_index];
        match row.id {
            SubRowId::MasterVolume => {
                let vol = master_volume_from_choice(new_index);
                config::update_master_volume(vol);
            }
            SubRowId::SfxVolume => {
                let vol = master_volume_from_choice(new_index);
                config::update_sfx_volume(vol);
            }
            SubRowId::AssistTickVolume => {
                let vol = master_volume_from_choice(new_index);
                config::update_assist_tick_volume(vol);
            }
            SubRowId::MusicVolume => {
                let vol = master_volume_from_choice(new_index);
                config::update_music_volume(vol);
            }
            SubRowId::SoundDevice => {
                let device = sound_device_from_choice(state, new_index);
                config::update_audio_output_device(device);
                let current_rate = config::get().audio_sample_rate_hz;
                let rate_choice = sample_rate_choice_index(state, current_rate);
                if current_rate.is_some() && rate_choice == 0 {
                    config::update_audio_sample_rate(None);
                }
                set_sound_choice_index(state, SubRowId::AudioSampleRate, rate_choice);
            }
            SubRowId::AudioOutputMode => {
                config::update_audio_output_mode(audio_output_mode_from_choice(new_index));
                #[cfg(target_os = "linux")]
                set_sound_choice_index(state, SubRowId::AlsaExclusive, 0);
            }
            #[cfg(target_os = "linux")]
            SubRowId::LinuxAudioBackend => {
                let backend = linux_audio_backend_from_choice(state, new_index);
                config::update_linux_audio_backend(backend);
                if matches!(backend, config::LinuxAudioBackend::Alsa) {
                    set_sound_choice_index(
                        state,
                        SubRowId::AlsaExclusive,
                        alsa_exclusive_choice_index(config::get().audio_output_mode),
                    );
                } else {
                    if matches!(
                        config::get().audio_output_mode,
                        config::AudioOutputMode::Exclusive
                    ) {
                        config::update_audio_output_mode(selected_audio_output_mode(state));
                    }
                    set_sound_choice_index(state, SubRowId::AlsaExclusive, 0);
                }
            }
            #[cfg(target_os = "linux")]
            SubRowId::AlsaExclusive => {
                let mode = if new_index == 1 {
                    config::AudioOutputMode::Exclusive
                } else {
                    selected_audio_output_mode(state)
                };
                config::update_audio_output_mode(mode);
            }
            SubRowId::AudioSampleRate => {
                let rate = sample_rate_from_choice(state, new_index);
                config::update_audio_sample_rate(rate);
            }
            SubRowId::MineSounds => {
                config::update_mine_hit_sound(new_index == 1);
            }
            SubRowId::RateModPreservesPitch => {
                config::update_rate_mod_preserves_pitch(new_index == 1);
            }
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::SelectMusic) {
        let row = &rows[row_index];
        if row.id == SubRowId::ShowBanners {
            config::update_show_select_music_banners(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ShowVideoBanners {
            config::update_show_select_music_video_banners(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ShowBreakdown {
            config::update_show_select_music_breakdown(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::BreakdownStyle {
            config::update_select_music_breakdown_style(BreakdownStyle::from_choice(new_index));
        } else if row.id == SubRowId::ShowNativeLanguage {
            config::update_translated_titles(translated_titles_from_choice(new_index));
        } else if row.id == SubRowId::MusicWheelSpeed {
            config::update_music_wheel_switch_speed(music_wheel_scroll_speed_from_choice(
                new_index,
            ));
        } else if row.id == SubRowId::MusicWheelStyle {
            config::update_select_music_wheel_style(SelectMusicWheelStyle::from_choice(new_index));
        } else if row.id == SubRowId::SwitchProfile {
            config::update_allow_switch_profile_in_menu(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ShowCdTitles {
            config::update_show_select_music_cdtitles(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ShowWheelGrades {
            config::update_show_music_wheel_grades(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ShowWheelLamps {
            config::update_show_music_wheel_lamps(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ItlRank {
            config::update_select_music_itl_rank_mode(SelectMusicItlRankMode::from_choice(
                new_index,
            ));
        } else if row.id == SubRowId::ItlWheelData {
            config::update_select_music_itl_wheel_mode(SelectMusicItlWheelMode::from_choice(
                new_index,
            ));
        } else if row.id == SubRowId::NewPackBadge {
            config::update_select_music_new_pack_mode(NewPackMode::from_choice(new_index));
        } else if row.id == SubRowId::ShowPatternInfo {
            config::update_select_music_pattern_info_mode(SelectMusicPatternInfoMode::from_choice(
                new_index,
            ));
        } else if row.id == SubRowId::MusicPreviews {
            config::update_show_select_music_previews(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::PreviewMarker {
            config::update_show_select_music_preview_marker(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::LoopMusic {
            config::update_select_music_preview_loop(new_index == 1);
        } else if row.id == SubRowId::ShowGameplayTimer {
            config::update_show_select_music_gameplay_timer(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ShowStageDisplay {
            config::update_show_select_music_stage_display(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ShowGsBox {
            config::update_show_select_music_scorebox(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::GsBoxPlacement {
            config::update_select_music_scorebox_placement(
                SelectMusicScoreboxPlacement::from_choice(new_index),
            );
        }
    } else if matches!(kind, SubmenuKind::GrooveStats) {
        let row = &rows[row_index];
        if row.id == SubRowId::EnableGrooveStats {
            let enabled = yes_no_from_choice(new_index);
            config::update_enable_groovestats(enabled);
            // Re-run connectivity logic so toggling this option applies immediately.
            crate::game::online::init();
        } else if row.id == SubRowId::EnableBoogieStats {
            config::update_enable_boogiestats(yes_no_from_choice(new_index));
            crate::game::online::init();
        } else if row.id == SubRowId::AutoPopulateScores {
            config::update_auto_populate_gs_scores(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::AutoDownloadUnlocks {
            config::update_auto_download_unlocks(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::SeparateUnlocksByPlayer {
            config::update_separate_unlocks_by_player(yes_no_from_choice(new_index));
        }
    } else if matches!(kind, SubmenuKind::ArrowCloud) {
        let row = &rows[row_index];
        if row.id == SubRowId::EnableArrowCloud {
            config::update_enable_arrowcloud(yes_no_from_choice(new_index));
            crate::game::online::init();
        } else if row.id == SubRowId::ArrowCloudSubmitFails {
            config::update_submit_arrowcloud_fails(yes_no_from_choice(new_index));
        }
    } else if matches!(kind, SubmenuKind::ScoreImport) {
        let row = &rows[row_index];
        if row.id == SubRowId::ScoreImportEndpoint {
            refresh_score_import_profile_options(state);
        }
    }
    clear_render_cache(state);
    action
}

pub(super) fn cancel_current_view(state: &mut State) -> ScreenAction {
    match state.view {
        OptionsView::Main => ScreenAction::Navigate(Screen::Menu),
        OptionsView::Submenu(_) => {
            if let Some(parent_kind) = state.submenu_parent_kind {
                state.pending_submenu_kind = Some(parent_kind);
                state.pending_submenu_parent_kind = None;
                state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
            } else {
                state.submenu_transition = SubmenuTransition::FadeOutToMain;
            }
            state.submenu_fade_t = 0.0;
            ScreenAction::None
        }
    }
}

pub(super) fn undo_three_key_selection(state: &mut State, asset_manager: &AssetManager) {
    match state.menu_lr_undo {
        1 => match state.view {
            OptionsView::Main => {
                let total = ITEMS.len();
                if total > 0 {
                    state.selected = (state.selected + 1) % total;
                }
            }
            OptionsView::Submenu(kind) => {
                move_submenu_selection_vertical(
                    state,
                    asset_manager,
                    kind,
                    NavDirection::Down,
                    NavWrap::Wrap,
                );
            }
        },
        -1 => match state.view {
            OptionsView::Main => {
                let total = ITEMS.len();
                if total > 0 {
                    state.selected = if state.selected == 0 {
                        total - 1
                    } else {
                        state.selected - 1
                    };
                }
            }
            OptionsView::Submenu(kind) => {
                move_submenu_selection_vertical(
                    state,
                    asset_manager,
                    kind,
                    NavDirection::Up,
                    NavWrap::Wrap,
                );
            }
        },
        _ => {}
    }
}

pub(super) fn activate_current_selection(
    state: &mut State,
    asset_manager: &AssetManager,
) -> ScreenAction {
    match state.view {
        OptionsView::Main => {
            let total = ITEMS.len();
            if total == 0 {
                return ScreenAction::None;
            }
            let sel = state.selected.min(total - 1);
            let item = &ITEMS[sel];
            state.pending_submenu_parent_kind = None;

            match item.id {
                ItemId::SystemOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::System);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::GraphicsOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Graphics);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::InputOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Input);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::LightsOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Lights);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::MachineOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Machine);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::AdvancedOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Advanced);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::CourseOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Course);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::GameplayOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Gameplay);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::SoundOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Sound);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::SelectMusicOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::SelectMusic);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::OnlineScoreServices => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::OnlineScoring);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::NullOrDieOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    refresh_null_or_die_options(state);
                    state.pending_submenu_kind = Some(SubmenuKind::NullOrDie);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::ManageLocalProfiles => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    return ScreenAction::Navigate(Screen::ManageLocalProfiles);
                }
                ItemId::ReloadSongsCourses => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    start_reload_songs_and_courses(state);
                }
                ItemId::Credits => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    return ScreenAction::NavigateNoFade(Screen::Credits);
                }
                ItemId::Exit => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    return ScreenAction::Navigate(Screen::Menu);
                }
                _ => {}
            }
            ScreenAction::None
        }
        OptionsView::Submenu(kind) => {
            let total = submenu_total_rows(state, kind);
            if total == 0 {
                return ScreenAction::None;
            }
            let selected_row = state.sub_selected.min(total.saturating_sub(1));
            if matches!(kind, SubmenuKind::SelectMusic)
                && let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row)
            {
                let rows = submenu_rows(kind);
                let row_id = rows.get(row_idx).map(|row| row.id);
                if row_id == Some(SubRowId::GsBoxLeaderboards) {
                    let choice_idx = submenu_cursor_indices(state, kind)
                        .get(row_idx)
                        .copied()
                        .unwrap_or(0)
                        .min(SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES.saturating_sub(1));
                    toggle_select_music_scorebox_cycle_option(state, choice_idx);
                    return ScreenAction::None;
                } else if row_id == Some(SubRowId::ChartInfo) {
                    let choice_idx = submenu_cursor_indices(state, kind)
                        .get(row_idx)
                        .copied()
                        .unwrap_or(0)
                        .min(SELECT_MUSIC_CHART_INFO_NUM_CHOICES.saturating_sub(1));
                    toggle_select_music_chart_info_option(state, choice_idx);
                    return ScreenAction::None;
                }
            }
            if matches!(kind, SubmenuKind::Gameplay)
                && let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row)
            {
                let rows = submenu_rows(kind);
                if rows.get(row_idx).map(|row| row.id) == Some(SubRowId::AutoScreenshot) {
                    let choice_idx = submenu_cursor_indices(state, kind)
                        .get(row_idx)
                        .copied()
                        .unwrap_or(0)
                        .min(config::AUTO_SS_NUM_FLAGS.saturating_sub(1));
                    toggle_auto_screenshot_option(state, choice_idx);
                    return ScreenAction::None;
                }
            }
            if selected_row == total - 1 {
                audio::play_sfx("assets/sounds/start.ogg");
                if let Some(parent_kind) = state.submenu_parent_kind {
                    state.pending_submenu_kind = Some(parent_kind);
                    state.pending_submenu_parent_kind = None;
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                } else {
                    state.submenu_transition = SubmenuTransition::FadeOutToMain;
                }
                state.submenu_fade_t = 0.0;
                return ScreenAction::None;
            }
            if matches!(kind, SubmenuKind::Input) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ScreenAction::None;
                };
                if let Some(row) = rows.get(row_idx) {
                    match row.id {
                        SubRowId::ConfigureMappings => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            return ScreenAction::Navigate(Screen::Mappings);
                        }
                        SubRowId::TestInput => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            return ScreenAction::Navigate(Screen::Input);
                        }
                        SubRowId::InputOptions => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::InputBackend);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::Input);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ScreenAction::None;
                        }
                        _ => {}
                    }
                }
            } else if matches!(kind, SubmenuKind::InputBackend) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ScreenAction::None;
                };
                if let Some(row) = rows.get(row_idx)
                    && row.id == SubRowId::DebugFsrDump
                {
                    audio::play_sfx("assets/sounds/start.ogg");
                    return ScreenAction::WriteFsrDump;
                }
            } else if matches!(kind, SubmenuKind::OnlineScoring) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ScreenAction::None;
                };
                if let Some(row) = rows.get(row_idx) {
                    match row.id {
                        SubRowId::GsBsOptions => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::GrooveStats);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::OnlineScoring);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ScreenAction::None;
                        }
                        SubRowId::ArrowCloudOptions => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::ArrowCloud);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::OnlineScoring);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ScreenAction::None;
                        }
                        SubRowId::ScoreImport => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            refresh_score_import_options(state);
                            state.pending_submenu_kind = Some(SubmenuKind::ScoreImport);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::OnlineScoring);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ScreenAction::None;
                        }
                        _ => {}
                    }
                }
            } else if matches!(kind, SubmenuKind::NullOrDie) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ScreenAction::None;
                };
                if let Some(row) = rows.get(row_idx) {
                    match row.id {
                        SubRowId::NullOrDieOptions => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::NullOrDieOptions);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::NullOrDie);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ScreenAction::None;
                        }
                        SubRowId::SyncPacks => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            refresh_sync_pack_options(state);
                            state.pending_submenu_kind = Some(SubmenuKind::SyncPacks);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::NullOrDie);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ScreenAction::None;
                        }
                        _ => {}
                    }
                }
            } else if matches!(kind, SubmenuKind::ScoreImport) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ScreenAction::None;
                };
                if let Some(row) = rows.get(row_idx) {
                    match row.id {
                        SubRowId::ScoreImportPack => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            refresh_score_import_pack_options(state);
                            open_score_import_pack_picker(state);
                            return ScreenAction::None;
                        }
                        SubRowId::ScoreImportStart => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            if let Some(selection) = selected_score_import_selection(state) {
                                if selection.pack_groups.is_empty() {
                                    clear_navigation_holds(state);
                                    state.score_import_confirm = Some(ScoreImportConfirmState {
                                        selection,
                                        active_choice: 1,
                                    });
                                } else {
                                    begin_score_import(state, selection);
                                }
                            } else {
                                log::warn!(
                                    "Score import start requested, but no eligible profile is selected."
                                );
                            }
                            return ScreenAction::None;
                        }
                        _ => {}
                    }
                }
            } else if matches!(kind, SubmenuKind::SyncPacks) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ScreenAction::None;
                };
                if let Some(row) = rows.get(row_idx)
                    && row.id == SubRowId::SyncPackStart
                {
                    audio::play_sfx("assets/sounds/start.ogg");
                    let selection = selected_sync_pack_selection(state);
                    if selection.pack_group.is_none() {
                        clear_navigation_holds(state);
                        state.sync_pack_confirm = Some(SyncPackConfirmState {
                            selection,
                            active_choice: 1,
                        });
                    } else {
                        begin_pack_sync(state, selection);
                    }
                    return ScreenAction::None;
                }
            }
            if screen_input::dedicated_three_key_nav_enabled()
                && let Some(action) =
                    apply_submenu_choice_delta(state, asset_manager, 1, NavWrap::Wrap)
            {
                return action;
            }
            ScreenAction::None
        }
    }
}

pub fn handle_input(
    state: &mut State,
    asset_manager: &AssetManager,
    ev: &InputEvent,
) -> ScreenAction {
    if state.reload_ui.is_some() {
        return ScreenAction::None;
    }
    let three_key_action = screen_input::three_key_menu_action(&mut state.menu_lr_chord, ev);
    if screen_input::dedicated_three_key_nav_enabled() {
        match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left
                if !ev.pressed =>
            {
                state.menu_lr_undo = 0;
                on_nav_release(state, NavDirection::Up);
                return ScreenAction::None;
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right
                if !ev.pressed =>
            {
                state.menu_lr_undo = 0;
                on_nav_release(state, NavDirection::Down);
                return ScreenAction::None;
            }
            _ => {}
        }
    }
    if state.score_import_pack_picker.is_some() {
        if let Some((_, nav)) = three_key_action {
            match nav {
                screen_input::ThreeKeyMenuAction::Prev => {
                    pack_picker_step(state, -1);
                    audio::play_sfx("assets/sounds/change.ogg");
                }
                screen_input::ThreeKeyMenuAction::Next => {
                    pack_picker_step(state, 1);
                    audio::play_sfx("assets/sounds/change.ogg");
                }
                screen_input::ThreeKeyMenuAction::Confirm => {
                    if pack_picker_toggle_current(state) {
                        audio::play_sfx("assets/sounds/start.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Cancel => {
                    close_score_import_pack_picker(state);
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            return ScreenAction::None;
        }
        if !ev.pressed {
            // Track release to disable hold-repeat.
            match ev.action {
                VirtualAction::p1_up
                | VirtualAction::p1_menu_up
                | VirtualAction::p2_up
                | VirtualAction::p2_menu_up => {
                    on_nav_release(state, NavDirection::Up);
                }
                VirtualAction::p1_down
                | VirtualAction::p1_menu_down
                | VirtualAction::p2_down
                | VirtualAction::p2_menu_down => {
                    on_nav_release(state, NavDirection::Down);
                }
                _ => {}
            }
            return ScreenAction::None;
        }
        match ev.action {
            VirtualAction::p1_up
            | VirtualAction::p1_menu_up
            | VirtualAction::p2_up
            | VirtualAction::p2_menu_up => {
                pack_picker_step(state, -1);
                on_nav_press(state, NavDirection::Up);
                audio::play_sfx("assets/sounds/change.ogg");
            }
            VirtualAction::p1_down
            | VirtualAction::p1_menu_down
            | VirtualAction::p2_down
            | VirtualAction::p2_menu_down => {
                pack_picker_step(state, 1);
                on_nav_press(state, NavDirection::Down);
                audio::play_sfx("assets/sounds/change.ogg");
            }
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left => {
                pack_picker_page(state, -1);
                audio::play_sfx("assets/sounds/change.ogg");
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right => {
                pack_picker_page(state, 1);
                audio::play_sfx("assets/sounds/change.ogg");
            }
            VirtualAction::p1_start | VirtualAction::p2_start => {
                if pack_picker_toggle_current(state) {
                    audio::play_sfx("assets/sounds/start.ogg");
                }
            }
            VirtualAction::p1_select | VirtualAction::p2_select => {
                toggle_all_score_import_packs(state);
                audio::play_sfx("assets/sounds/start.ogg");
            }
            VirtualAction::p1_back | VirtualAction::p2_back => {
                close_score_import_pack_picker(state);
                audio::play_sfx("assets/sounds/change.ogg");
            }
            _ => {}
        }
        return ScreenAction::None;
    }
    if let Some(score_import) = state.score_import_ui.as_ref() {
        // After completion, any Confirm or Cancel input dismisses the overlay
        // — the worker has already finished, so no cancel signal is needed.
        if score_import.done {
            let dismiss = matches!(
                three_key_action,
                Some((
                    _,
                    screen_input::ThreeKeyMenuAction::Cancel
                        | screen_input::ThreeKeyMenuAction::Confirm
                ))
            ) || (ev.pressed
                && matches!(
                    ev.action,
                    VirtualAction::p1_back
                        | VirtualAction::p2_back
                        | VirtualAction::p1_start
                        | VirtualAction::p2_start
                ));
            if dismiss {
                clear_navigation_holds(state);
                state.score_import_ui = None;
                audio::play_sfx("assets/sounds/start.ogg");
            }
            return ScreenAction::None;
        }
        let cancel_requested = matches!(
            three_key_action,
            Some((_, screen_input::ThreeKeyMenuAction::Cancel))
        ) || (ev.pressed
            && matches!(ev.action, VirtualAction::p1_back | VirtualAction::p2_back));
        if cancel_requested {
            score_import.cancel_requested.store(true, Ordering::Relaxed);
            clear_navigation_holds(state);
            state.score_import_ui = None;
            audio::play_sfx("assets/sounds/change.ogg");
            log::warn!("Score import cancel requested by user.");
        }
        return ScreenAction::None;
    }
    if !matches!(
        state.pack_sync_overlay,
        shared_pack_sync::OverlayState::Hidden
    ) {
        return shared_pack_sync::handle_input(&mut state.pack_sync_overlay, ev);
    }
    if let Some(confirm) = state.score_import_confirm.as_mut() {
        if let Some((_, nav)) = three_key_action {
            match nav {
                screen_input::ThreeKeyMenuAction::Prev => {
                    if confirm.active_choice > 0 {
                        confirm.active_choice -= 1;
                        audio::play_sfx("assets/sounds/change.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Next => {
                    if confirm.active_choice < 1 {
                        confirm.active_choice += 1;
                        audio::play_sfx("assets/sounds/change.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Confirm => {
                    let should_start = confirm.active_choice == 0;
                    audio::play_sfx("assets/sounds/start.ogg");
                    if should_start {
                        clear_navigation_holds(state);
                        begin_score_import_from_confirm(state);
                    } else {
                        clear_navigation_holds(state);
                        state.score_import_confirm = None;
                    }
                }
                screen_input::ThreeKeyMenuAction::Cancel => {
                    clear_navigation_holds(state);
                    state.score_import_confirm = None;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            return ScreenAction::None;
        }
        if !ev.pressed {
            return ScreenAction::None;
        }
        match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left => {
                if confirm.active_choice > 0 {
                    confirm.active_choice -= 1;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right => {
                if confirm.active_choice < 1 {
                    confirm.active_choice += 1;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_start
            | VirtualAction::p1_select
            | VirtualAction::p2_start
            | VirtualAction::p2_select => {
                let should_start = confirm.active_choice == 0;
                audio::play_sfx("assets/sounds/start.ogg");
                if should_start {
                    clear_navigation_holds(state);
                    begin_score_import_from_confirm(state);
                } else {
                    clear_navigation_holds(state);
                    state.score_import_confirm = None;
                }
            }
            VirtualAction::p1_back | VirtualAction::p2_back => {
                clear_navigation_holds(state);
                state.score_import_confirm = None;
                audio::play_sfx("assets/sounds/change.ogg");
            }
            _ => {}
        }
        return ScreenAction::None;
    }
    if let Some(confirm) = state.sync_pack_confirm.as_mut() {
        if let Some((_, nav)) = three_key_action {
            match nav {
                screen_input::ThreeKeyMenuAction::Prev => {
                    if confirm.active_choice > 0 {
                        confirm.active_choice -= 1;
                        audio::play_sfx("assets/sounds/change.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Next => {
                    if confirm.active_choice < 1 {
                        confirm.active_choice += 1;
                        audio::play_sfx("assets/sounds/change.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Confirm => {
                    let should_start = confirm.active_choice == 0;
                    audio::play_sfx("assets/sounds/start.ogg");
                    clear_navigation_holds(state);
                    if should_start {
                        begin_pack_sync_from_confirm(state);
                    } else {
                        state.sync_pack_confirm = None;
                    }
                }
                screen_input::ThreeKeyMenuAction::Cancel => {
                    clear_navigation_holds(state);
                    state.sync_pack_confirm = None;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            return ScreenAction::None;
        }
        if !ev.pressed {
            return ScreenAction::None;
        }
        match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left => {
                if confirm.active_choice > 0 {
                    confirm.active_choice -= 1;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right => {
                if confirm.active_choice < 1 {
                    confirm.active_choice += 1;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_start
            | VirtualAction::p1_select
            | VirtualAction::p2_start
            | VirtualAction::p2_select => {
                let should_start = confirm.active_choice == 0;
                audio::play_sfx("assets/sounds/start.ogg");
                clear_navigation_holds(state);
                if should_start {
                    begin_pack_sync_from_confirm(state);
                } else {
                    state.sync_pack_confirm = None;
                }
            }
            VirtualAction::p1_back | VirtualAction::p2_back => {
                clear_navigation_holds(state);
                state.sync_pack_confirm = None;
                audio::play_sfx("assets/sounds/change.ogg");
            }
            _ => {}
        }
        return ScreenAction::None;
    }
    // Ignore new navigation while a local submenu fade is in progress.
    if !matches!(state.submenu_transition, SubmenuTransition::None) {
        return ScreenAction::None;
    }
    if let Some((_, nav)) = three_key_action {
        return match nav {
            screen_input::ThreeKeyMenuAction::Prev => {
                match state.view {
                    OptionsView::Main => {
                        let total = ITEMS.len();
                        if total > 0 {
                            state.selected = if state.selected == 0 {
                                total - 1
                            } else {
                                state.selected - 1
                            };
                        }
                    }
                    OptionsView::Submenu(kind) => {
                        move_submenu_selection_vertical(
                            state,
                            asset_manager,
                            kind,
                            NavDirection::Up,
                            NavWrap::Wrap,
                        );
                    }
                }
                on_nav_press(state, NavDirection::Up);
                state.menu_lr_undo = 1;
                ScreenAction::None
            }
            screen_input::ThreeKeyMenuAction::Next => {
                match state.view {
                    OptionsView::Main => {
                        let total = ITEMS.len();
                        if total > 0 {
                            state.selected = (state.selected + 1) % total;
                        }
                    }
                    OptionsView::Submenu(kind) => {
                        move_submenu_selection_vertical(
                            state,
                            asset_manager,
                            kind,
                            NavDirection::Down,
                            NavWrap::Wrap,
                        );
                    }
                }
                on_nav_press(state, NavDirection::Down);
                state.menu_lr_undo = -1;
                ScreenAction::None
            }
            screen_input::ThreeKeyMenuAction::Confirm => {
                state.menu_lr_undo = 0;
                clear_navigation_holds(state);
                activate_current_selection(state, asset_manager)
            }
            screen_input::ThreeKeyMenuAction::Cancel => {
                undo_three_key_selection(state, asset_manager);
                state.menu_lr_undo = 0;
                clear_navigation_holds(state);
                cancel_current_view(state)
            }
        };
    }

    match ev.action {
        VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
            return cancel_current_view(state);
        }
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => {
            if ev.pressed {
                match state.view {
                    OptionsView::Main => {
                        let total = ITEMS.len();
                        if total > 0 {
                            state.selected = if state.selected == 0 {
                                total - 1
                            } else {
                                state.selected - 1
                            };
                        }
                    }
                    OptionsView::Submenu(kind) => {
                        move_submenu_selection_vertical(
                            state,
                            asset_manager,
                            kind,
                            NavDirection::Up,
                            NavWrap::Wrap,
                        );
                    }
                }
                on_nav_press(state, NavDirection::Up);
            } else {
                on_nav_release(state, NavDirection::Up);
            }
        }
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => {
            if ev.pressed {
                match state.view {
                    OptionsView::Main => {
                        let total = ITEMS.len();
                        if total > 0 {
                            state.selected = (state.selected + 1) % total;
                        }
                    }
                    OptionsView::Submenu(kind) => {
                        move_submenu_selection_vertical(
                            state,
                            asset_manager,
                            kind,
                            NavDirection::Down,
                            NavWrap::Wrap,
                        );
                    }
                }
                on_nav_press(state, NavDirection::Down);
            } else {
                on_nav_release(state, NavDirection::Down);
            }
        }
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => {
            if ev.pressed {
                if let Some(action) =
                    apply_submenu_choice_delta(state, asset_manager, -1, NavWrap::Wrap)
                {
                    on_lr_press(state, -1);
                    return action;
                }
                on_lr_press(state, -1);
            } else {
                on_lr_release(state, -1);
            }
        }
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => {
            if ev.pressed {
                if let Some(action) =
                    apply_submenu_choice_delta(state, asset_manager, 1, NavWrap::Wrap)
                {
                    on_lr_press(state, 1);
                    return action;
                }
                on_lr_press(state, 1);
            } else {
                on_lr_release(state, 1);
            }
        }
        VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
            return activate_current_selection(state, asset_manager);
        }
        _ => {}
    }
    ScreenAction::None
}
