use super::*;
use deadsync_profile as profile_data;

// Small helpers to let the app dispatcher manage hold-to-scroll without exposing fields
pub fn on_nav_press(state: &mut State, dir: NavDirection) {
    state.nav_key_held_direction = Some(dir);
    screen_input::reset_hold_repeat(
        &mut state.nav_key_held_for,
        &mut state.nav_key_next_repeat_at,
        NAV_INITIAL_HOLD_DELAY,
    );
}

pub fn on_nav_release(state: &mut State, dir: NavDirection) {
    if state.nav_key_held_direction == Some(dir) {
        state.nav_key_held_direction = None;
        screen_input::reset_hold_repeat(
            &mut state.nav_key_held_for,
            &mut state.nav_key_next_repeat_at,
            NAV_INITIAL_HOLD_DELAY,
        );
    }
}

pub(super) fn on_lr_press(state: &mut State, delta: isize) {
    state.nav_lr_held_direction = Some(delta);
    screen_input::reset_hold_repeat(
        &mut state.nav_lr_held_for,
        &mut state.nav_lr_next_repeat_at,
        NAV_INITIAL_HOLD_DELAY,
    );
}

pub(super) fn on_lr_release(state: &mut State, delta: isize) {
    if state.nav_lr_held_direction == Some(delta) {
        state.nav_lr_held_direction = None;
        screen_input::reset_hold_repeat(
            &mut state.nav_lr_held_for,
            &mut state.nav_lr_next_repeat_at,
            NAV_INITIAL_HOLD_DELAY,
        );
    }
}

pub(super) fn apply_submenu_choice_delta(
    state: &mut State,
    asset_manager: &AssetManager,
    delta: isize,
    wrap: NavWrap,
) -> Option<ThemeEffect> {
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
                        clear_render_cache(state);
                        return Some(volume_change_effect(
                            AudioVolumeTarget::Master,
                            state.master_volume_pct as u8,
                        ));
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
                        clear_render_cache(state);
                        return Some(volume_change_effect(
                            AudioVolumeTarget::Sfx,
                            state.sfx_volume_pct as u8,
                        ));
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
                        clear_render_cache(state);
                        return Some(volume_change_effect(
                            AudioVolumeTarget::AssistTick,
                            state.assist_tick_volume_pct as u8,
                        ));
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
                        clear_render_cache(state);
                        return Some(volume_change_effect(
                            AudioVolumeTarget::Music,
                            state.music_volume_pct as u8,
                        ));
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
                queue_sfx(state, "assets/sounds/change_value.ogg");
                clear_render_cache(state);
                return Some(audio_requests_effect(vec![
                    AudioRequest::SetGlobalOffsetMillis(state.global_offset_ms),
                ]));
            }
            return None;
        }
        if matches!(kind, SubmenuKind::SmxConfig) && row.id == SubRowId::SmxDefaultLightBrightness {
            // Numeric placeholder row: adjust the percent directly (like the volume
            // rows) instead of cycling a choice list.
            if adjust_ms_value(
                &mut state.smx_default_light_brightness_pct,
                delta,
                VOLUME_MIN_PERCENT,
                VOLUME_MAX_PERCENT,
            ) {
                config::update_smx_default_light_brightness(
                    state.smx_default_light_brightness_pct as u8,
                );
                queue_sfx(state, "assets/sounds/change_value.ogg");
                clear_render_cache(state);
            }
            return None;
        }
        if matches!(kind, SubmenuKind::Graphics) && row.id == SubRowId::MaxFpsValue {
            if adjust_max_fps_value_choice(state, delta, wrap) {
                queue_sfx(state, "assets/sounds/change_value.ogg");
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
                queue_sfx(state, "assets/sounds/change_value.ogg");
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
                queue_sfx(state, "assets/sounds/change_value.ogg");
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
                        queue_sfx(state, "assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                        return Some(null_or_die_config_effect(
                            crate::SimplyLoveNullOrDieConfigRequest::FingerprintTenths(
                                state.null_or_die_fingerprint_tenths,
                            ),
                        ));
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
                        queue_sfx(state, "assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                        return Some(null_or_die_config_effect(
                            crate::SimplyLoveNullOrDieConfigRequest::WindowTenths(
                                state.null_or_die_window_tenths,
                            ),
                        ));
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
                        queue_sfx(state, "assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                        return Some(null_or_die_config_effect(
                            crate::SimplyLoveNullOrDieConfigRequest::StepTenths(
                                state.null_or_die_step_tenths,
                            ),
                        ));
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
                        queue_sfx(state, "assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                        return Some(null_or_die_config_effect(
                            crate::SimplyLoveNullOrDieConfigRequest::MagicOffsetTenths(
                                state.null_or_die_magic_offset_tenths,
                            ),
                        ));
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
    let mut action: Option<ThemeEffect> = None;
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
    queue_sfx(state, "assets/sounds/change_value.ogg");

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
            SubRowId::LogLevel => config::update_log_level(log_level_from_choice(new_index)),
            SubRowId::LogFile => config::update_log_to_file(new_index == 1),
            SubRowId::DefaultNoteSkin => {
                if let Some(skin_name) = selected_choice.as_deref() {
                    profile::update_machine_default_noteskin(profile_data::NoteSkin::new(
                        skin_name,
                    ));
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
            action = Some(ThemeEffect::Runtime(
                crate::SimplyLoveRuntimeRequest::Config(
                    crate::SimplyLoveConfigRequest::ShowOverlay(mode),
                ),
            ));
        }
        if row.id == SubRowId::ValidationLayers {
            config::update_gfx_debug(yes_no_from_choice(new_index));
        }
        if row.id == SubRowId::HideMouseCursor {
            action = Some(ThemeEffect::Runtime(
                crate::SimplyLoveRuntimeRequest::Config(
                    crate::SimplyLoveConfigRequest::MouseCursorHidden(yes_no_from_choice(
                        new_index,
                    )),
                ),
            ));
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
    } else if matches!(kind, SubmenuKind::SmxConfig) {
        let row = &rows[row_index];
        if row.id == SubRowId::SmxInput {
            config::update_smx_input(yes_no_from_choice(new_index));
        }
        if row.id == SubRowId::SmxPanelLights {
            config::update_smx_panel_lights(yes_no_from_choice(new_index));
        }
        if row.id == SubRowId::SmxUnderglowTheme {
            action = Some(ThemeEffect::Runtime(
                crate::SimplyLoveRuntimeRequest::Hardware(
                    crate::SimplyLoveHardwareRequest::SetSmxUnderglowTheme(yes_no_from_choice(
                        new_index,
                    )),
                ),
            ));
        }
        if row.id == SubRowId::SmxUnderglowGrb {
            action = Some(ThemeEffect::Runtime(
                crate::SimplyLoveRuntimeRequest::Hardware(
                    crate::SimplyLoveHardwareRequest::SetSmxUnderglowGrb(new_index == 1),
                ),
            ));
        }
        if row.id == SubRowId::SmxManagesPadConfig {
            config::update_smx_manages_pad_config(yes_no_from_choice(new_index));
        }
        if row.id == SubRowId::SmxDefaultPadConfig {
            config::update_smx_default_pad_config(crate::config::SmxPadPreset::from_index(
                new_index,
            ));
        }
        if row.id == SubRowId::SmxBgPack {
            let pack = if new_index == 0 {
                crate::config::SmxPackName::default()
            } else {
                state
                    .smx_bg_pack_choices
                    .get(new_index - 1)
                    .map(|s| crate::config::SmxPackName::parse(s))
                    .unwrap_or_default()
            };
            config::update_smx_pad_gifs_pack(pack);
        }
        if row.id == SubRowId::SmxJudgePack {
            let pack = if new_index == 0 {
                crate::config::SmxPackName::default()
            } else {
                state
                    .smx_judge_pack_choices
                    .get(new_index - 1)
                    .map(|s| crate::config::SmxPackName::parse(s))
                    .unwrap_or_default()
            };
            config::update_smx_judge_gifs_pack(pack);
        }
        if row.id == SubRowId::SmxIdleLights {
            // Index 0 = Firmware (release idle pads to the pad's built-in
            // lighting), 1 = Black (keep the LEDs and hold idle pads dark).
            config::update_smx_idle_lights_black(new_index == 1);
        }
        if row.id == SubRowId::SmxSinglePadPlayer {
            // Pin the lone connected pad's serial to the chosen side (index 0 = P1,
            // 1 = P2). The SDK then relocates it to that slot. Row is only shown
            // with exactly one pad connected, so just take whichever slot has one.
            if let Some(request) = single_pad_assignment_request(&state.smx_assignment, new_index) {
                action = Some(ThemeEffect::Runtime(
                    crate::SimplyLoveRuntimeRequest::Hardware(request),
                ));
            }
        }
    } else if matches!(kind, SubmenuKind::Lights) {
        let row = &rows[row_index];
        let request = match row.id {
            SubRowId::LightsDriver => {
                crate::SimplyLoveLightsConfigRequest::Driver(lights_driver_from_index(new_index))
            }
            SubRowId::GameplayPadLights => crate::SimplyLoveLightsConfigRequest::GameplayPadLights(
                gameplay_pad_lights_from_index(new_index),
            ),
            SubRowId::LightsSimplifyBass => {
                crate::SimplyLoveLightsConfigRequest::SimplifyBass(yes_no_from_choice(new_index))
            }
            _ => return None,
        };
        action = Some(lights_config_effect(request));
    } else if matches!(kind, SubmenuKind::Machine) {
        let row = &rows[row_index];
        let enabled = new_index == 1;
        match row.id {
            SubRowId::PreferredColor => {
                state.active_color_index = new_index as i32;
                action = Some(ThemeEffect::Runtime(
                    crate::SimplyLoveRuntimeRequest::Config(
                        crate::SimplyLoveConfigRequest::PersistColor(state.active_color_index),
                    ),
                ));
            }
            id => {
                let request = match id {
                    SubRowId::SelectProfile => {
                        crate::SimplyLoveMachineConfigRequest::ShowSelectProfile(enabled)
                    }
                    SubRowId::SelectColor => {
                        crate::SimplyLoveMachineConfigRequest::ShowSelectColor(enabled)
                    }
                    SubRowId::SelectStyle => {
                        crate::SimplyLoveMachineConfigRequest::ShowSelectStyle(enabled)
                    }
                    SubRowId::PreferredStyle => {
                        crate::SimplyLoveMachineConfigRequest::PreferredPlayStyle(
                            machine_preferred_play_style_from_choice(new_index),
                        )
                    }
                    SubRowId::SelectPlayMode => {
                        crate::SimplyLoveMachineConfigRequest::ShowSelectPlayMode(enabled)
                    }
                    SubRowId::PreferredMode => {
                        crate::SimplyLoveMachineConfigRequest::PreferredPlayMode(
                            machine_preferred_play_mode_from_choice(new_index),
                        )
                    }
                    SubRowId::Font => crate::SimplyLoveMachineConfigRequest::Font(
                        machine_font_from_choice(new_index),
                    ),
                    SubRowId::BarColor => crate::SimplyLoveMachineConfigRequest::BarColor(
                        machine_bar_color_from_choice(new_index),
                    ),
                    SubRowId::EvaluationStyle => {
                        crate::SimplyLoveMachineConfigRequest::EvaluationStyle(
                            machine_evaluation_style_from_choice(new_index),
                        )
                    }
                    SubRowId::EvalSummary => {
                        crate::SimplyLoveMachineConfigRequest::ShowEvaluationSummary(enabled)
                    }
                    SubRowId::NiceSound => {
                        crate::SimplyLoveMachineConfigRequest::NiceSound(enabled)
                    }
                    SubRowId::NameEntry => {
                        crate::SimplyLoveMachineConfigRequest::ShowNameEntry(enabled)
                    }
                    SubRowId::GameoverScreen => {
                        crate::SimplyLoveMachineConfigRequest::ShowGameover(enabled)
                    }
                    SubRowId::MenuMusic => {
                        crate::SimplyLoveMachineConfigRequest::MenuMusic(enabled)
                    }
                    SubRowId::VisualStyle => crate::SimplyLoveMachineConfigRequest::VisualStyle(
                        visual_style_from_choice(new_index),
                    ),
                    SubRowId::ThemeVariant => crate::SimplyLoveMachineConfigRequest::SrpgVariant(
                        srpg_variant_from_choice(new_index),
                    ),
                    SubRowId::Replays => {
                        crate::SimplyLoveMachineConfigRequest::EnableReplays(enabled)
                    }
                    SubRowId::HeartRateMonitors => {
                        crate::SimplyLoveMachineConfigRequest::EnableHeartRateMonitors(enabled)
                    }
                    SubRowId::PerPlayerGlobalOffsets => {
                        crate::SimplyLoveMachineConfigRequest::AllowPerPlayerGlobalOffsets(enabled)
                    }
                    SubRowId::PackIniOffsets => {
                        crate::SimplyLoveMachineConfigRequest::PackIniOffsets(enabled)
                    }
                    SubRowId::DefaultSyncOffset => {
                        crate::SimplyLoveMachineConfigRequest::DefaultSyncOffset(
                            default_sync_offset_from_choice(new_index),
                        )
                    }
                    SubRowId::KeyboardFeatures => {
                        crate::SimplyLoveMachineConfigRequest::KeyboardFeatures(enabled)
                    }
                    SubRowId::VideoBgs => {
                        crate::SimplyLoveMachineConfigRequest::ShowVideoBackgrounds(enabled)
                    }
                    SubRowId::RandomBackgroundMode => {
                        crate::SimplyLoveMachineConfigRequest::RandomBackgroundMode(
                            random_background_mode_from_choice(new_index),
                        )
                    }
                    SubRowId::VersionOverlay => {
                        crate::SimplyLoveMachineConfigRequest::ShowVersionOverlay(enabled)
                    }
                    SubRowId::VersionOverlaySide => {
                        crate::SimplyLoveMachineConfigRequest::VersionOverlaySide(
                            version_overlay_side_from_choice(new_index),
                        )
                    }
                    SubRowId::WriteCurrentScreen => {
                        crate::SimplyLoveMachineConfigRequest::WriteCurrentScreen(enabled)
                    }
                    _ => return None,
                };
                action = Some(machine_config_effect(request));
            }
        }
    } else if matches!(kind, SubmenuKind::Advanced) {
        let row = &rows[row_index];
        let request = match row.id {
            SubRowId::DefaultFailType => crate::SimplyLoveAdvancedConfigRequest::DefaultFailType(
                default_fail_type_from_choice(new_index),
            ),
            SubRowId::BannerCache => {
                crate::SimplyLoveAdvancedConfigRequest::BannerCache(new_index == 1)
            }
            SubRowId::CdTitleCache => {
                crate::SimplyLoveAdvancedConfigRequest::CdTitleCache(new_index == 1)
            }
            SubRowId::SongParsingThreads => {
                crate::SimplyLoveAdvancedConfigRequest::SongParsingThreads(
                    thread_count_from_choice(&state.software_thread_choices, new_index),
                )
            }
            SubRowId::CacheSongs => {
                crate::SimplyLoveAdvancedConfigRequest::CacheSongs(new_index == 1)
            }
            SubRowId::FastLoad => crate::SimplyLoveAdvancedConfigRequest::FastLoad(new_index == 1),
            _ => return None,
        };
        action = Some(advanced_config_effect(request));
    } else if matches!(kind, SubmenuKind::NullOrDieOptions) {
        let row = &rows[row_index];
        let request = match row.id {
            SubRowId::SyncGraph => {
                crate::SimplyLoveNullOrDieConfigRequest::SyncGraph(match new_index {
                    0 => crate::SimplyLoveNullOrDieGraph::Frequency,
                    1 => crate::SimplyLoveNullOrDieGraph::BeatIndex,
                    _ => crate::SimplyLoveNullOrDieGraph::PostKernelFingerprint,
                })
            }
            SubRowId::GraphOrientation => {
                crate::SimplyLoveNullOrDieConfigRequest::GraphOrientation(if new_index == 1 {
                    crate::SimplyLoveGraphOrientation::Horizontal
                } else {
                    crate::SimplyLoveGraphOrientation::Vertical
                })
            }
            SubRowId::SyncConfidence => crate::SimplyLoveNullOrDieConfigRequest::ConfidencePercent(
                sync_confidence_from_choice(new_index),
            ),
            SubRowId::PackSyncThreads => crate::SimplyLoveNullOrDieConfigRequest::PackSyncThreads(
                thread_count_from_choice(&state.software_thread_choices, new_index),
            ),
            SubRowId::KernelTarget => {
                crate::SimplyLoveNullOrDieConfigRequest::KernelTarget(if new_index == 1 {
                    crate::SimplyLoveSyncKernelTarget::Accumulator
                } else {
                    crate::SimplyLoveSyncKernelTarget::Digest
                })
            }
            SubRowId::KernelType => {
                crate::SimplyLoveNullOrDieConfigRequest::Kernel(if new_index == 1 {
                    crate::SimplyLoveSyncKernel::Loudest
                } else {
                    crate::SimplyLoveSyncKernel::Rising
                })
            }
            SubRowId::FullSpectrogram => crate::SimplyLoveNullOrDieConfigRequest::FullSpectrogram(
                yes_no_from_choice(new_index),
            ),
            _ => return None,
        };
        action = Some(null_or_die_config_effect(request));
    } else if matches!(kind, SubmenuKind::Course) {
        let row = &rows[row_index];
        let enabled = yes_no_from_choice(new_index);
        let request = match row.id {
            SubRowId::ShowRandomCourses => {
                crate::SimplyLoveCourseConfigRequest::ShowRandom(enabled)
            }
            SubRowId::ShowMostPlayed => {
                crate::SimplyLoveCourseConfigRequest::ShowMostPlayed(enabled)
            }
            SubRowId::ShowIndividualScores => {
                crate::SimplyLoveCourseConfigRequest::ShowIndividualScores(enabled)
            }
            SubRowId::AutosubmitIndividual => {
                crate::SimplyLoveCourseConfigRequest::AutosubmitIndividual(enabled)
            }
            _ => return None,
        };
        action = Some(course_config_effect(request));
    } else if matches!(kind, SubmenuKind::Gameplay) {
        let row = &rows[row_index];
        let request = match row.id {
            SubRowId::BgBrightness => {
                crate::SimplyLoveGameplayConfigRequest::BackgroundBrightnessTenths(
                    new_index.min(10) as u8,
                )
            }
            SubRowId::CenteredP1Notefield => {
                crate::SimplyLoveGameplayConfigRequest::CenterPlayerOneNotefield(new_index == 1)
            }
            SubRowId::AnimatedBanners => {
                let mode = match new_index {
                    0 => config::GameplayBannerMode::Static,
                    1 => config::GameplayBannerMode::Once,
                    _ => config::GameplayBannerMode::Loop,
                };
                crate::SimplyLoveGameplayConfigRequest::BannerMode(mode)
            }
            SubRowId::ZmodRatingBox => {
                crate::SimplyLoveGameplayConfigRequest::ZmodRatingBoxText(new_index == 1)
            }
            SubRowId::BpmDecimal => {
                crate::SimplyLoveGameplayConfigRequest::ShowBpmDecimal(new_index == 1)
            }
            SubRowId::BpmPosition => {
                crate::SimplyLoveGameplayConfigRequest::BpmNearField(new_index == 1)
            }
            SubRowId::DelayedBack => {
                crate::SimplyLoveGameplayConfigRequest::DelayedBack(new_index == 1)
            }
            _ => return None,
        };
        action = Some(gameplay_config_effect(request));
    } else if matches!(kind, SubmenuKind::Sound) {
        let row = &rows[row_index];
        match row.id {
            SubRowId::SoundDevice => {
                let device = sound_device_from_choice(state, new_index);
                state.audio_options.output_device = device;
                let current_rate = state.audio_options.sample_rate_hz;
                let rate_choice = sample_rate_choice_index(state, current_rate);
                let mut requests = vec![AudioRequest::SetOutputDevice(device)];
                if current_rate.is_some() && rate_choice == 0 {
                    state.audio_options.sample_rate_hz = None;
                    requests.push(AudioRequest::SetSampleRate(None));
                }
                set_sound_choice_index(state, SubRowId::AudioSampleRate, rate_choice);
                action = Some(audio_requests_effect(requests));
            }
            SubRowId::AudioOutputMode => {
                let mode = AudioOutputModeChoice::from_choice(new_index);
                state.audio_options.output_mode = mode;
                action = Some(audio_requests_effect(vec![AudioRequest::SetOutputMode(
                    mode,
                )]));
                #[cfg(target_os = "linux")]
                set_sound_choice_index(state, SubRowId::AlsaExclusive, 0);
            }
            #[cfg(target_os = "linux")]
            SubRowId::LinuxAudioBackend => {
                let backend_name =
                    linux_audio_backend_name_from_choice(state, new_index).to_owned();
                state.audio_options.selected_backend_name = backend_name.clone();
                let mut requests = vec![AudioRequest::SetOutputBackend(backend_name.clone())];
                if backend_name.eq_ignore_ascii_case("ALSA") {
                    let exclusive_index = state.audio_options.output_mode.exclusive_choice_index();
                    set_sound_choice_index(state, SubRowId::AlsaExclusive, exclusive_index);
                } else {
                    if state.audio_options.output_mode == AudioOutputModeChoice::Exclusive {
                        let mode = selected_audio_output_mode(state);
                        state.audio_options.output_mode = mode;
                        requests.push(AudioRequest::SetOutputMode(mode));
                    }
                    set_sound_choice_index(state, SubRowId::AlsaExclusive, 0);
                }
                action = Some(audio_requests_effect(requests));
            }
            #[cfg(target_os = "linux")]
            SubRowId::AlsaExclusive => {
                let mode = selected_audio_output_mode(state).with_exclusive(new_index == 1);
                state.audio_options.output_mode = mode;
                action = Some(audio_requests_effect(vec![AudioRequest::SetOutputMode(
                    mode,
                )]));
            }
            SubRowId::AudioSampleRate => {
                let rate = sample_rate_from_choice(state, new_index);
                state.audio_options.sample_rate_hz = rate;
                action = Some(audio_requests_effect(vec![AudioRequest::SetSampleRate(
                    rate,
                )]));
            }
            SubRowId::MineSounds => {
                action = Some(audio_requests_effect(vec![AudioRequest::SetMineHitSound(
                    new_index == 1,
                )]));
            }
            SubRowId::RateModPreservesPitch => {
                let enabled = new_index == 1;
                state.audio_options.preserve_pitch = enabled;
                action = Some(audio_requests_effect(vec![AudioRequest::SetPreservePitch(
                    enabled,
                )]));
            }
            SubRowId::ReplayGain => {
                let enabled = new_index == 1;
                state.audio_options.replay_gain = enabled;
                action = Some(audio_requests_effect(vec![AudioRequest::SetReplayGain(
                    enabled,
                )]));
            }
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::SelectMusic) {
        let row = &rows[row_index];
        let request = match row.id {
            SubRowId::ShowBanners => crate::SimplyLoveSelectMusicConfigRequest::ShowBanners(
                yes_no_from_choice(new_index),
            ),
            SubRowId::ShowVideoBanners => {
                crate::SimplyLoveSelectMusicConfigRequest::ShowVideoBanners(yes_no_from_choice(
                    new_index,
                ))
            }
            SubRowId::ShowBreakdown => crate::SimplyLoveSelectMusicConfigRequest::ShowBreakdown(
                yes_no_from_choice(new_index),
            ),
            SubRowId::BreakdownStyle => crate::SimplyLoveSelectMusicConfigRequest::BreakdownStyle(
                breakdown_style_from_choice(new_index),
            ),
            SubRowId::ShowNativeLanguage => {
                crate::SimplyLoveSelectMusicConfigRequest::TranslatedTitles(
                    translated_titles_from_choice(new_index),
                )
            }
            SubRowId::MusicWheelSpeed => {
                crate::SimplyLoveSelectMusicConfigRequest::WheelSwitchSpeed(
                    music_wheel_scroll_speed_from_choice(new_index),
                )
            }
            SubRowId::MusicWheelStyle => crate::SimplyLoveSelectMusicConfigRequest::WheelStyle(
                select_music_wheel_style_from_choice(new_index),
            ),
            SubRowId::SeriesSort => crate::SimplyLoveSelectMusicConfigRequest::SortBySeries(
                yes_no_from_choice(new_index),
            ),
            SubRowId::SongSelectBg => {
                crate::SimplyLoveSelectMusicConfigRequest::SongSelectBackground(
                    select_music_song_select_bg_mode_from_choice(new_index),
                )
            }
            SubRowId::SwitchProfile => {
                crate::SimplyLoveSelectMusicConfigRequest::AllowProfileSwitch(yes_no_from_choice(
                    new_index,
                ))
            }
            SubRowId::ShowCdTitles => crate::SimplyLoveSelectMusicConfigRequest::ShowCdTitles(
                yes_no_from_choice(new_index),
            ),
            SubRowId::ShowWheelGrades => {
                crate::SimplyLoveSelectMusicConfigRequest::ShowWheelGrades(yes_no_from_choice(
                    new_index,
                ))
            }
            SubRowId::ShowWheelLamps => crate::SimplyLoveSelectMusicConfigRequest::ShowWheelLamps(
                yes_no_from_choice(new_index),
            ),
            SubRowId::ItlRank => crate::SimplyLoveSelectMusicConfigRequest::ItlRankMode(
                select_music_itl_rank_mode_from_choice(new_index),
            ),
            SubRowId::ItlWheelData => crate::SimplyLoveSelectMusicConfigRequest::ItlWheelMode(
                select_music_itl_wheel_mode_from_choice(new_index),
            ),
            SubRowId::NewPackBadge => crate::SimplyLoveSelectMusicConfigRequest::NewPackMode(
                select_music_new_pack_mode_from_choice(new_index),
            ),
            SubRowId::FolderStats => crate::SimplyLoveSelectMusicConfigRequest::ShowFolderStats(
                yes_no_from_choice(new_index),
            ),
            SubRowId::ShowPatternInfo => {
                crate::SimplyLoveSelectMusicConfigRequest::PatternInfoMode(
                    select_music_pattern_info_mode_from_choice(new_index),
                )
            }
            SubRowId::StepArtistBox => {
                crate::SimplyLoveSelectMusicConfigRequest::StepArtistBoxMode(
                    select_music_step_artist_box_mode_from_choice(new_index),
                )
            }
            SubRowId::MusicPreviews => crate::SimplyLoveSelectMusicConfigRequest::ShowPreviews(
                yes_no_from_choice(new_index),
            ),
            SubRowId::PreviewMarker => {
                crate::SimplyLoveSelectMusicConfigRequest::ShowPreviewMarker(yes_no_from_choice(
                    new_index,
                ))
            }
            SubRowId::LoopMusic => {
                crate::SimplyLoveSelectMusicConfigRequest::PreviewLoop(new_index == 1)
            }
            SubRowId::PreviewStartsImmediately => {
                crate::SimplyLoveSelectMusicConfigRequest::PreviewStartsImmediately(
                    yes_no_from_choice(new_index),
                )
            }
            SubRowId::ShowGameplayTimer => {
                crate::SimplyLoveSelectMusicConfigRequest::ShowGameplayTimer(yes_no_from_choice(
                    new_index,
                ))
            }
            SubRowId::ShowStageDisplay => {
                crate::SimplyLoveSelectMusicConfigRequest::ShowStageDisplay(yes_no_from_choice(
                    new_index,
                ))
            }
            SubRowId::ShowGsBox => crate::SimplyLoveSelectMusicConfigRequest::ShowScorebox(
                yes_no_from_choice(new_index),
            ),
            SubRowId::GsBoxPlacement => {
                crate::SimplyLoveSelectMusicConfigRequest::ScoreboxPlacement(
                    select_music_scorebox_placement_from_choice(new_index),
                )
            }
            _ => return None,
        };
        action = Some(select_music_config_effect(request));
    } else if matches!(kind, SubmenuKind::GrooveStats) {
        let row = &rows[row_index];
        let enabled = yes_no_from_choice(new_index);
        let request = match row.id {
            SubRowId::EnableGrooveStats => {
                crate::SimplyLoveOnlineConfigRequest::EnableGrooveStats(enabled)
            }
            SubRowId::ShowSrpgShop => crate::SimplyLoveOnlineConfigRequest::ShowSrpgShop(enabled),
            SubRowId::SrpgShopFolder => crate::SimplyLoveOnlineConfigRequest::SrpgShopFolder(
                srpg_shop_folder_from_index(new_index),
            ),
            SubRowId::EnableBoogieStats => {
                crate::SimplyLoveOnlineConfigRequest::EnableBoogieStats(enabled)
            }
            SubRowId::AutoPopulateScores => {
                crate::SimplyLoveOnlineConfigRequest::AutoPopulateScores(enabled)
            }
            SubRowId::AutoDownloadUnlocks => {
                crate::SimplyLoveOnlineConfigRequest::AutoDownloadUnlocks(enabled)
            }
            SubRowId::SeparateUnlocksByPlayer => {
                crate::SimplyLoveOnlineConfigRequest::SeparateUnlocksByPlayer(enabled)
            }
            SubRowId::GrooveStatsQrLogin => {
                crate::SimplyLoveOnlineConfigRequest::GrooveStatsQrLogin(
                    qr_login_policy_from_index(new_index),
                )
            }
            _ => return None,
        };
        let effect = online_config_effect(request);
        action = Some(
            if matches!(
                row.id,
                SubRowId::EnableGrooveStats | SubRowId::EnableBoogieStats
            ) {
                ThemeEffect::Batch(vec![effect, online_reinitialize_effect()])
            } else {
                effect
            },
        );
    } else if matches!(kind, SubmenuKind::ArrowCloud) {
        let row = &rows[row_index];
        let enabled = yes_no_from_choice(new_index);
        let request = match row.id {
            SubRowId::EnableArrowCloud => {
                crate::SimplyLoveOnlineConfigRequest::EnableArrowCloud(enabled)
            }
            SubRowId::ArrowCloudSubmitFails => {
                crate::SimplyLoveOnlineConfigRequest::SubmitArrowCloudFails(enabled)
            }
            SubRowId::ArrowCloudQrLogin => crate::SimplyLoveOnlineConfigRequest::ArrowCloudQrLogin(
                qr_login_policy_from_index(new_index),
            ),
            _ => return None,
        };
        let effect = online_config_effect(request);
        action = Some(if row.id == SubRowId::EnableArrowCloud {
            ThemeEffect::Batch(vec![effect, online_reinitialize_effect()])
        } else {
            effect
        });
    } else if matches!(kind, SubmenuKind::ScoreImport) {
        let row = &rows[row_index];
        if row.id == SubRowId::ScoreImportEndpoint {
            refresh_score_import_profile_options(state);
        }
    }
    clear_render_cache(state);
    action
}

fn online_reinitialize_effect() -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Online(
        crate::SimplyLoveOnlineRequest::Reinitialize,
    ))
}

const fn qr_login_policy_from_index(index: usize) -> crate::SimplyLoveQrLoginPolicy {
    match index {
        0 => crate::SimplyLoveQrLoginPolicy::Always,
        2 => crate::SimplyLoveQrLoginPolicy::Disabled,
        _ => crate::SimplyLoveQrLoginPolicy::Sometimes,
    }
}

const fn srpg_shop_folder_from_index(index: usize) -> crate::SimplyLoveSrpgShopFolder {
    match index {
        1 => crate::SimplyLoveSrpgShopFolder::Shops,
        2 => crate::SimplyLoveSrpgShopFolder::Faction,
        _ => crate::SimplyLoveSrpgShopFolder::Unlocks,
    }
}

pub(super) const fn lights_driver_from_index(index: usize) -> crate::SimplyLoveLightsDriver {
    match index {
        1 => crate::SimplyLoveLightsDriver::Snek,
        2 => crate::SimplyLoveLightsDriver::Litboard,
        3 => crate::SimplyLoveLightsDriver::Win32Serial,
        4 => crate::SimplyLoveLightsDriver::Fusion,
        5 => crate::SimplyLoveLightsDriver::Gpb,
        6 => crate::SimplyLoveLightsDriver::PacDrive,
        7 => crate::SimplyLoveLightsDriver::PiuioLeds,
        8 => crate::SimplyLoveLightsDriver::Itgio,
        9 => crate::SimplyLoveLightsDriver::HidBlueDot,
        10 => crate::SimplyLoveLightsDriver::Stac2,
        11 => crate::SimplyLoveLightsDriver::MinimaidHid,
        _ => crate::SimplyLoveLightsDriver::Off,
    }
}

const fn gameplay_pad_lights_from_index(index: usize) -> crate::SimplyLoveGameplayPadLights {
    if index == 1 {
        crate::SimplyLoveGameplayPadLights::Chart
    } else {
        crate::SimplyLoveGameplayPadLights::Input
    }
}

pub(super) fn single_pad_assignment_request(
    view: &SmxAssignmentView,
    player_index: usize,
) -> Option<crate::SimplyLoveHardwareRequest> {
    let serial = view
        .pads
        .iter()
        .find(|pad| pad.connected && !pad.serial.is_empty())?
        .serial
        .clone();
    let (p1_serial, p2_serial) = if player_index == 1 {
        (None, Some(serial))
    } else {
        (Some(serial), None)
    };
    Some(crate::SimplyLoveHardwareRequest::AssignSmxPads {
        p1_serial,
        p2_serial,
    })
}

fn move_main_selection(state: &mut State, dir: NavDirection) {
    let total = visible_items(state).len();
    if total == 0 {
        return;
    }
    state.selected = match dir {
        NavDirection::Up => {
            if state.selected == 0 {
                total - 1
            } else {
                state.selected - 1
            }
        }
        NavDirection::Down => (state.selected + 1) % total,
    };
}

fn move_options_selection_vertical(
    state: &mut State,
    asset_manager: &AssetManager,
    dir: NavDirection,
) {
    match state.view {
        OptionsView::Main => move_main_selection(state, dir),
        OptionsView::Submenu(kind) => {
            move_submenu_selection_vertical(state, asset_manager, kind, dir, NavWrap::Wrap);
        }
    }
}

fn start_side(action: VirtualAction) -> Option<profile_data::PlayerSide> {
    match action {
        VirtualAction::p1_start => Some(profile_data::PlayerSide::P1),
        VirtualAction::p2_start => Some(profile_data::PlayerSide::P2),
        _ => None,
    }
}

fn on_start_press(state: &mut State, side: profile_data::PlayerSide) {
    let idx = profile_data::player_side_index(side);
    state.start_input[idx].held = true;
    let start_input = &mut state.start_input[idx];
    screen_input::reset_hold_repeat(
        &mut start_input.held_for,
        &mut start_input.next_repeat_at,
        NAV_INITIAL_HOLD_DELAY,
    );
}

fn clear_start_hold(state: &mut State, side: profile_data::PlayerSide) {
    let idx = profile_data::player_side_index(side);
    state.start_input[idx] = OptionsStartInput::default();
}

fn dedicated_three_key_options_event(action: VirtualAction) -> bool {
    matches!(
        action,
        VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left
            | VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right
            | VirtualAction::p1_start
            | VirtualAction::p2_start
    )
}

fn dedicated_three_key_menu_nav(view: OptionsView) -> bool {
    matches!(
        view,
        OptionsView::Main | OptionsView::Submenu(SubmenuKind::Input)
    )
}

fn selected_submenu_row_has_choices(state: &State, kind: SubmenuKind) -> Option<bool> {
    let rows = submenu_rows(kind);
    let row_idx = submenu_visible_row_to_actual(state, kind, state.sub_selected)?;
    Some(!row_choices(state, kind, rows, row_idx).is_empty())
}

/// Whether Left/Right on the focused submenu row should fall through to
/// vertical navigation (like the main menu) instead of cycling a value: true
/// for rows with nothing to cycle, i.e. pure link/action rows (a single
/// localized "Open" choice), rows with no choices, and the Exit row. Value
/// rows keep Left/Right, including ones whose single choice is a placeholder
/// for a numeric Left/Right adjustment (e.g. the volume sliders).
fn selected_row_lr_navigates(state: &State) -> bool {
    let OptionsView::Submenu(kind) = state.view else {
        return false;
    };
    let rows = submenu_rows(kind);
    let Some(row_idx) = submenu_visible_row_to_actual(state, kind, state.sub_selected) else {
        return true; // Exit row: nothing to cycle.
    };
    let Some(row) = rows.get(row_idx) else {
        return false;
    };
    let open_link = matches!(
        row.choices,
        [Choice::Localized(key)] if key.section == "Common" && key.key == "Open"
    );
    open_link || row_choices(state, kind, rows, row_idx).is_empty()
}

fn handle_dedicated_three_key_start_nav(
    state: &mut State,
    asset_manager: &AssetManager,
    kind: SubmenuKind,
    side: profile_data::PlayerSide,
    repeated: bool,
) -> ThemeEffect {
    if screen_input::menu_lr_both_held(&state.menu_lr_chord, side) {
        move_submenu_selection_vertical(
            state,
            asset_manager,
            kind,
            NavDirection::Up,
            NavWrap::Clamp,
        );
        return ThemeEffect::None;
    }
    if submenu_visible_row_to_actual(state, kind, state.sub_selected).is_none() {
        if repeated {
            return ThemeEffect::None;
        }
        clear_navigation_holds(state);
        return activate_current_selection(state, asset_manager);
    }
    if selected_submenu_row_has_choices(state, kind) == Some(false) {
        if repeated {
            return ThemeEffect::None;
        }
        clear_navigation_holds(state);
        return activate_current_selection(state, asset_manager);
    }
    move_submenu_selection_vertical(
        state,
        asset_manager,
        kind,
        NavDirection::Down,
        NavWrap::Clamp,
    );
    ThemeEffect::None
}

pub(super) fn repeat_held_dedicated_three_key_start(
    state: &mut State,
    asset_manager: &AssetManager,
    side: profile_data::PlayerSide,
    dt: f32,
) -> Option<ThemeEffect> {
    let OptionsView::Submenu(kind) = state.view else {
        clear_start_hold(state, side);
        return None;
    };
    if dedicated_three_key_menu_nav(state.view) {
        clear_start_hold(state, side);
        return None;
    }
    let idx = profile_data::player_side_index(side);
    if !state.start_input[idx].held {
        return None;
    };
    let start_input = &mut state.start_input[idx];
    if !screen_input::advance_hold_repeat(
        &mut start_input.held_for,
        &mut start_input.next_repeat_at,
        NAV_REPEAT_SCROLL_INTERVAL,
        dt,
    ) {
        return None;
    }
    let action = handle_dedicated_three_key_start_nav(state, asset_manager, kind, side, true);
    (!matches!(action, ThemeEffect::None)).then_some(action)
}

pub(super) fn handle_dedicated_three_key_options_input(
    state: &mut State,
    asset_manager: &AssetManager,
    ev: &InputEvent,
) -> ThemeEffect {
    if !ev.pressed {
        match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left => {
                state.menu_lr_undo = 0;
                if dedicated_three_key_menu_nav(state.view) {
                    on_nav_release(state, NavDirection::Up);
                } else {
                    on_lr_release(state, -1);
                }
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right => {
                state.menu_lr_undo = 0;
                if dedicated_three_key_menu_nav(state.view) {
                    on_nav_release(state, NavDirection::Down);
                } else {
                    on_lr_release(state, 1);
                }
            }
            VirtualAction::p1_start => clear_start_hold(state, profile_data::PlayerSide::P1),
            VirtualAction::p2_start => clear_start_hold(state, profile_data::PlayerSide::P2),
            _ => {}
        }
        return ThemeEffect::None;
    }

    if dedicated_three_key_menu_nav(state.view) {
        return match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left => {
                move_options_selection_vertical(state, asset_manager, NavDirection::Up);
                on_nav_press(state, NavDirection::Up);
                ThemeEffect::None
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right => {
                move_options_selection_vertical(state, asset_manager, NavDirection::Down);
                on_nav_press(state, NavDirection::Down);
                ThemeEffect::None
            }
            VirtualAction::p1_start | VirtualAction::p2_start => {
                clear_navigation_holds(state);
                activate_current_selection(state, asset_manager)
            }
            _ => ThemeEffect::None,
        };
    }

    match state.view {
        OptionsView::Submenu(kind) => match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left => {
                if let Some(action) =
                    apply_submenu_choice_delta(state, asset_manager, -1, NavWrap::Wrap)
                {
                    on_lr_press(state, -1);
                    return action;
                }
                on_lr_press(state, -1);
                ThemeEffect::None
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right => {
                if let Some(action) =
                    apply_submenu_choice_delta(state, asset_manager, 1, NavWrap::Wrap)
                {
                    on_lr_press(state, 1);
                    return action;
                }
                on_lr_press(state, 1);
                ThemeEffect::None
            }
            VirtualAction::p1_start | VirtualAction::p2_start => {
                let Some(side) = start_side(ev.action) else {
                    return ThemeEffect::None;
                };
                on_start_press(state, side);
                handle_dedicated_three_key_start_nav(state, asset_manager, kind, side, false)
            }
            _ => ThemeEffect::None,
        },
        OptionsView::Main => ThemeEffect::None,
    }
}

/// The submenu a given submenu sits under (`None` = it opens straight off the
/// main options list). The single source of truth for back navigation: when we
/// return to a parent submenu we must also restore *its* parent link, otherwise
/// a third-level page (e.g. SMX Config) would strand its parent (Input Options)
/// with no way back to the Input launcher.
pub(super) fn submenu_parent_kind_of(kind: SubmenuKind) -> Option<SubmenuKind> {
    match kind {
        SubmenuKind::InputBackend => Some(SubmenuKind::Input),
        SubmenuKind::SmxConfig => Some(SubmenuKind::InputBackend),
        SubmenuKind::GrooveStats | SubmenuKind::ArrowCloud | SubmenuKind::ScoreImport => {
            Some(SubmenuKind::OnlineScoring)
        }
        SubmenuKind::NullOrDieOptions | SubmenuKind::SyncPacks => Some(SubmenuKind::NullOrDie),
        _ => None,
    }
}

pub(super) fn cancel_current_view(state: &mut State) -> ThemeEffect {
    match state.view {
        OptionsView::Main => ThemeEffect::Navigate(Screen::Menu),
        OptionsView::Submenu(_) => {
            if let Some(parent_kind) = state.submenu_parent_kind {
                state.pending_submenu_kind = Some(parent_kind);
                // Restore the parent's own parent so a further Back keeps climbing.
                state.pending_submenu_parent_kind = submenu_parent_kind_of(parent_kind);
                state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
            } else {
                state.submenu_transition = SubmenuTransition::FadeOutToMain;
            }
            state.submenu_fade_t = 0.0;
            ThemeEffect::None
        }
    }
}

pub(super) fn undo_three_key_selection(state: &mut State, asset_manager: &AssetManager) {
    match state.menu_lr_undo {
        1 => match state.view {
            OptionsView::Main => {
                let total = visible_items(state).len();
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
                let total = visible_items(state).len();
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
) -> ThemeEffect {
    match state.view {
        OptionsView::Main => {
            let visible = visible_items(state);
            let total = visible.len();
            if total == 0 {
                return ThemeEffect::None;
            }
            let sel = state.selected.min(total - 1);
            let item = visible[sel];
            state.pending_submenu_parent_kind = None;

            match item.id {
                ItemId::SystemOptions => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::System);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::GraphicsOptions => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Graphics);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::InputOptions => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Input);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::LightsOptions => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Lights);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::MachineOptions => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Machine);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::AdvancedOptions => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Advanced);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::CourseOptions => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Course);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::GameplayOptions => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Gameplay);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::SoundOptions => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Sound);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::SelectMusicOptions => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::SelectMusic);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::OnlineScoreServices => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::OnlineScoring);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::NullOrDieOptions => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    refresh_null_or_die_options(state);
                    state.pending_submenu_kind = Some(SubmenuKind::NullOrDie);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::FoldersOptions => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Folders);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::ManageLocalProfiles => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    return ThemeEffect::Navigate(Screen::ManageLocalProfiles);
                }
                ItemId::ReloadSongsCourses => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    start_reload_songs_and_courses(state);
                }
                ItemId::CheckForUpdates => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    return ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Updater(
                        crate::SimplyLoveUpdaterRequest::CheckForUpdates,
                    ));
                }
                ItemId::RollBackVersion => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    return ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Updater(
                        crate::SimplyLoveUpdaterRequest::CheckForRollback,
                    ));
                }
                ItemId::DownloadVideoSupport => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    // Probe ffmpeg/ffprobe on a worker thread — the lookup
                    // spawns subprocesses and would stutter the UI thread.
                    return ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Updater(
                        crate::SimplyLoveUpdaterRequest::CheckFfmpegAvailability,
                    ));
                }
                ItemId::Credits => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    return ThemeEffect::NavigateNoFade(Screen::Credits);
                }
                ItemId::Exit => {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    return ThemeEffect::Navigate(Screen::Menu);
                }
                _ => {}
            }
            ThemeEffect::None
        }
        OptionsView::Submenu(kind) => {
            let total = submenu_total_rows(state, kind);
            if total == 0 {
                return ThemeEffect::None;
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
                    return toggle_select_music_scorebox_cycle_option(state, choice_idx);
                } else if row_id == Some(SubRowId::ChartInfo) {
                    let choice_idx = submenu_cursor_indices(state, kind)
                        .get(row_idx)
                        .copied()
                        .unwrap_or(0)
                        .min(SELECT_MUSIC_CHART_INFO_NUM_CHOICES.saturating_sub(1));
                    return toggle_select_music_chart_info_option(state, choice_idx);
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
                    return toggle_auto_screenshot_option(state, choice_idx);
                }
            }
            if selected_row == total - 1 {
                queue_sfx(state, "assets/sounds/start.ogg");
                if let Some(parent_kind) = state.submenu_parent_kind {
                    state.pending_submenu_kind = Some(parent_kind);
                    // Restore the parent's own parent so a further Back keeps climbing.
                    state.pending_submenu_parent_kind = submenu_parent_kind_of(parent_kind);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                } else {
                    state.submenu_transition = SubmenuTransition::FadeOutToMain;
                }
                state.submenu_fade_t = 0.0;
                return ThemeEffect::None;
            }
            if matches!(kind, SubmenuKind::Input) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ThemeEffect::None;
                };
                if let Some(row) = rows.get(row_idx) {
                    match row.id {
                        SubRowId::ConfigureMappings => {
                            queue_sfx(state, "assets/sounds/start.ogg");
                            return ThemeEffect::Navigate(Screen::Mappings);
                        }
                        SubRowId::TestInput => {
                            queue_sfx(state, "assets/sounds/start.ogg");
                            return ThemeEffect::Navigate(Screen::Input);
                        }
                        SubRowId::ConfigurePads => {
                            queue_sfx(state, "assets/sounds/start.ogg");
                            return ThemeEffect::Navigate(Screen::ConfigurePads);
                        }
                        SubRowId::InputOptions => {
                            queue_sfx(state, "assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::InputBackend);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::Input);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ThemeEffect::None;
                        }
                        _ => {}
                    }
                }
            } else if matches!(kind, SubmenuKind::InputBackend) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ThemeEffect::None;
                };
                if let Some(row) = rows.get(row_idx) {
                    match row.id {
                        SubRowId::DebugFsrDump => {
                            queue_sfx(state, "assets/sounds/start.ogg");
                            return ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Debug(
                                crate::SimplyLoveDebugRequest::WriteFsrDump,
                            ));
                        }
                        SubRowId::SmxConfig => {
                            queue_sfx(state, "assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::SmxConfig);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::InputBackend);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ThemeEffect::None;
                        }
                        _ => {}
                    }
                }
            } else if matches!(kind, SubmenuKind::SmxConfig) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ThemeEffect::None;
                };
                if let Some(row) = rows.get(row_idx) {
                    match row.id {
                        SubRowId::SmxAssignPads => {
                            queue_sfx(state, "assets/sounds/start.ogg");
                            crate::screens::smx_assign::set_pending_return(Screen::Options);
                            return ThemeEffect::Navigate(Screen::SmxAssignPads);
                        }
                        SubRowId::SmxSwapPads => {
                            // Immediate action: swap P1/P2 and stay on the page so
                            // the user can watch the pad colours swap. Needs both
                            // pads; otherwise it is a no-op, so signal that.
                            if state.smx_assignment.can_swap {
                                queue_sfx(state, "assets/sounds/start.ogg");
                                return ThemeEffect::Runtime(
                                    crate::SimplyLoveRuntimeRequest::Hardware(
                                        crate::SimplyLoveHardwareRequest::SwapSmxPads,
                                    ),
                                );
                            } else {
                                queue_sfx(state, "assets/sounds/common_invalid.ogg");
                            }
                            return ThemeEffect::None;
                        }
                        _ => {}
                    }
                }
            } else if matches!(kind, SubmenuKind::Lights) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ThemeEffect::None;
                };
                if let Some(row) = rows.get(row_idx)
                    && row.id == SubRowId::TestLights
                {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    return ThemeEffect::Navigate(Screen::TestLights);
                }
            } else if matches!(kind, SubmenuKind::Graphics) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ThemeEffect::None;
                };
                if let Some(row) = rows.get(row_idx)
                    && row.id == SubRowId::OverscanAdjustment
                {
                    queue_sfx(state, "assets/sounds/start.ogg");
                    return ThemeEffect::Navigate(Screen::OverscanAdjustment);
                }
            } else if matches!(kind, SubmenuKind::Folders) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ThemeEffect::None;
                };
                if let Some(request) = rows
                    .get(row_idx)
                    .and_then(|row| folder_reveal_request(&state.app_paths, row.id))
                {
                    return crate::effects::sfx_then(
                        "assets/sounds/start.ogg",
                        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Platform(request)),
                    );
                }
            } else if matches!(kind, SubmenuKind::OnlineScoring) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ThemeEffect::None;
                };
                if let Some(row) = rows.get(row_idx) {
                    match row.id {
                        SubRowId::GsBsOptions => {
                            queue_sfx(state, "assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::GrooveStats);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::OnlineScoring);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ThemeEffect::None;
                        }
                        SubRowId::ArrowCloudOptions => {
                            queue_sfx(state, "assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::ArrowCloud);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::OnlineScoring);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ThemeEffect::None;
                        }
                        SubRowId::ScoreImport => {
                            queue_sfx(state, "assets/sounds/start.ogg");
                            refresh_score_import_options(state);
                            state.pending_submenu_kind = Some(SubmenuKind::ScoreImport);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::OnlineScoring);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ThemeEffect::None;
                        }
                        _ => {}
                    }
                }
            } else if matches!(kind, SubmenuKind::NullOrDie) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ThemeEffect::None;
                };
                if let Some(row) = rows.get(row_idx) {
                    match row.id {
                        SubRowId::NullOrDieOptions => {
                            queue_sfx(state, "assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::NullOrDieOptions);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::NullOrDie);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ThemeEffect::None;
                        }
                        SubRowId::SyncPacks => {
                            queue_sfx(state, "assets/sounds/start.ogg");
                            refresh_sync_pack_options(state);
                            state.pending_submenu_kind = Some(SubmenuKind::SyncPacks);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::NullOrDie);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ThemeEffect::None;
                        }
                        _ => {}
                    }
                }
            } else if matches!(kind, SubmenuKind::ScoreImport) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ThemeEffect::None;
                };
                if let Some(row) = rows.get(row_idx) {
                    match row.id {
                        SubRowId::ScoreImportPack => {
                            queue_sfx(state, "assets/sounds/start.ogg");
                            refresh_score_import_pack_options(state);
                            open_score_import_pack_picker(state);
                            return ThemeEffect::None;
                        }
                        SubRowId::ScoreImportStart => {
                            queue_sfx(state, "assets/sounds/start.ogg");
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
                            return ThemeEffect::None;
                        }
                        _ => {}
                    }
                }
            } else if matches!(kind, SubmenuKind::SyncPacks) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ThemeEffect::None;
                };
                if let Some(row) = rows.get(row_idx)
                    && row.id == SubRowId::SyncPackStart
                {
                    queue_sfx(state, "assets/sounds/start.ogg");
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
                    return ThemeEffect::None;
                }
            }
            if screen_input::dedicated_three_key_nav_enabled()
                && let Some(action) =
                    apply_submenu_choice_delta(state, asset_manager, 1, NavWrap::Wrap)
            {
                return action;
            }
            ThemeEffect::None
        }
    }
}

pub fn handle_input(
    state: &mut State,
    asset_manager: &AssetManager,
    updater: &SimplyLoveUpdaterView,
    ev: &InputEvent,
) -> ThemeEffect {
    let effect = handle_input_impl(state, asset_manager, updater, ev);
    prepend_pending_sfx(state, effect)
}

fn handle_input_impl(
    state: &mut State,
    asset_manager: &AssetManager,
    updater: &SimplyLoveUpdaterView,
    ev: &InputEvent,
) -> ThemeEffect {
    use crate::screens::components::shared::{ffmpeg_overlay, update_overlay};

    match update_overlay::handle_input(&updater.update, ev) {
        update_overlay::InputOutcome::Passthrough => {}
        update_overlay::InputOutcome::Consumed => return ThemeEffect::None,
        update_overlay::InputOutcome::Request(request) => {
            return ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Updater(request));
        }
    }
    match ffmpeg_overlay::handle_input(&updater.ffmpeg, ev) {
        update_overlay::InputOutcome::Passthrough => {}
        update_overlay::InputOutcome::Consumed => return ThemeEffect::None,
        update_overlay::InputOutcome::Request(request) => {
            return ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Updater(request));
        }
    }
    if state.reload_ui.is_some() {
        return ThemeEffect::None;
    }
    let three_key_action = screen_input::three_key_menu_action(&mut state.menu_lr_chord, ev);
    if state.score_import_pack_picker.is_some() {
        if let Some((_, nav)) = three_key_action {
            match nav {
                screen_input::ThreeKeyMenuAction::Prev => {
                    pack_picker_step(state, -1);
                    queue_sfx(state, "assets/sounds/change.ogg");
                }
                screen_input::ThreeKeyMenuAction::Next => {
                    pack_picker_step(state, 1);
                    queue_sfx(state, "assets/sounds/change.ogg");
                }
                screen_input::ThreeKeyMenuAction::Confirm => {
                    if pack_picker_toggle_current(state) {
                        queue_sfx(state, "assets/sounds/start.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Cancel => {
                    close_score_import_pack_picker(state);
                    queue_sfx(state, "assets/sounds/change.ogg");
                }
            }
            return ThemeEffect::None;
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
            return ThemeEffect::None;
        }
        match ev.action {
            VirtualAction::p1_up
            | VirtualAction::p1_menu_up
            | VirtualAction::p2_up
            | VirtualAction::p2_menu_up => {
                pack_picker_step(state, -1);
                on_nav_press(state, NavDirection::Up);
                queue_sfx(state, "assets/sounds/change.ogg");
            }
            VirtualAction::p1_down
            | VirtualAction::p1_menu_down
            | VirtualAction::p2_down
            | VirtualAction::p2_menu_down => {
                pack_picker_step(state, 1);
                on_nav_press(state, NavDirection::Down);
                queue_sfx(state, "assets/sounds/change.ogg");
            }
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left => {
                pack_picker_page(state, -1);
                queue_sfx(state, "assets/sounds/change.ogg");
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right => {
                pack_picker_page(state, 1);
                queue_sfx(state, "assets/sounds/change.ogg");
            }
            VirtualAction::p1_start | VirtualAction::p2_start => {
                if pack_picker_toggle_current(state) {
                    queue_sfx(state, "assets/sounds/start.ogg");
                }
            }
            VirtualAction::p1_select | VirtualAction::p2_select => {
                toggle_all_score_import_packs(state);
                queue_sfx(state, "assets/sounds/start.ogg");
            }
            VirtualAction::p1_back | VirtualAction::p2_back => {
                close_score_import_pack_picker(state);
                queue_sfx(state, "assets/sounds/change.ogg");
            }
            _ => {}
        }
        return ThemeEffect::None;
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
                queue_sfx(state, "assets/sounds/start.ogg");
            }
            return ThemeEffect::None;
        }
        let cancel_requested = matches!(
            three_key_action,
            Some((_, screen_input::ThreeKeyMenuAction::Cancel))
        ) || (ev.pressed
            && matches!(ev.action, VirtualAction::p1_back | VirtualAction::p2_back));
        if cancel_requested {
            clear_navigation_holds(state);
            state.score_import_ui = None;
            queue_online(state, crate::SimplyLoveOnlineRequest::CancelScoreImport);
            queue_sfx(state, "assets/sounds/change.ogg");
        }
        return ThemeEffect::None;
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
                        queue_sfx(state, "assets/sounds/change.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Next => {
                    if confirm.active_choice < 1 {
                        confirm.active_choice += 1;
                        queue_sfx(state, "assets/sounds/change.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Confirm => {
                    let should_start = confirm.active_choice == 0;
                    queue_sfx(state, "assets/sounds/start.ogg");
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
                    queue_sfx(state, "assets/sounds/change.ogg");
                }
            }
            return ThemeEffect::None;
        }
        if !ev.pressed {
            return ThemeEffect::None;
        }
        match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left => {
                if confirm.active_choice > 0 {
                    confirm.active_choice -= 1;
                    queue_sfx(state, "assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right => {
                if confirm.active_choice < 1 {
                    confirm.active_choice += 1;
                    queue_sfx(state, "assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_start
            | VirtualAction::p1_select
            | VirtualAction::p2_start
            | VirtualAction::p2_select => {
                let should_start = confirm.active_choice == 0;
                queue_sfx(state, "assets/sounds/start.ogg");
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
                queue_sfx(state, "assets/sounds/change.ogg");
            }
            _ => {}
        }
        return ThemeEffect::None;
    }
    if let Some(confirm) = state.sync_pack_confirm.as_mut() {
        if let Some((_, nav)) = three_key_action {
            match nav {
                screen_input::ThreeKeyMenuAction::Prev => {
                    if confirm.active_choice > 0 {
                        confirm.active_choice -= 1;
                        queue_sfx(state, "assets/sounds/change.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Next => {
                    if confirm.active_choice < 1 {
                        confirm.active_choice += 1;
                        queue_sfx(state, "assets/sounds/change.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Confirm => {
                    let should_start = confirm.active_choice == 0;
                    queue_sfx(state, "assets/sounds/start.ogg");
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
                    queue_sfx(state, "assets/sounds/change.ogg");
                }
            }
            return ThemeEffect::None;
        }
        if !ev.pressed {
            return ThemeEffect::None;
        }
        match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left => {
                if confirm.active_choice > 0 {
                    confirm.active_choice -= 1;
                    queue_sfx(state, "assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right => {
                if confirm.active_choice < 1 {
                    confirm.active_choice += 1;
                    queue_sfx(state, "assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_start
            | VirtualAction::p1_select
            | VirtualAction::p2_start
            | VirtualAction::p2_select => {
                let should_start = confirm.active_choice == 0;
                queue_sfx(state, "assets/sounds/start.ogg");
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
                queue_sfx(state, "assets/sounds/change.ogg");
            }
            _ => {}
        }
        return ThemeEffect::None;
    }
    // Ignore new navigation while a local submenu fade is in progress.
    if !matches!(state.submenu_transition, SubmenuTransition::None) {
        return ThemeEffect::None;
    }
    if screen_input::dedicated_three_key_nav_enabled()
        && matches!(state.view, OptionsView::Main | OptionsView::Submenu(_))
        && dedicated_three_key_options_event(ev.action)
    {
        return handle_dedicated_three_key_options_input(state, asset_manager, ev);
    }
    if let Some((_, nav)) = three_key_action {
        return match nav {
            screen_input::ThreeKeyMenuAction::Prev => {
                move_options_selection_vertical(state, asset_manager, NavDirection::Up);
                on_nav_press(state, NavDirection::Up);
                state.menu_lr_undo = 1;
                ThemeEffect::None
            }
            screen_input::ThreeKeyMenuAction::Next => {
                move_options_selection_vertical(state, asset_manager, NavDirection::Down);
                on_nav_press(state, NavDirection::Down);
                state.menu_lr_undo = -1;
                ThemeEffect::None
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
                move_options_selection_vertical(state, asset_manager, NavDirection::Up);
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
                move_options_selection_vertical(state, asset_manager, NavDirection::Down);
                on_nav_press(state, NavDirection::Down);
            } else {
                on_nav_release(state, NavDirection::Down);
            }
        }
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => {
            if !ev.pressed {
                // The press may have armed either hold (nav on a link row,
                // value-repeat on a choice row) and the cursor may have moved
                // since; releasing both is a no-op for the one not armed.
                on_nav_release(state, NavDirection::Up);
                on_lr_release(state, -1);
            } else if matches!(state.view, OptionsView::Main) || selected_row_lr_navigates(state) {
                move_options_selection_vertical(state, asset_manager, NavDirection::Up);
                on_nav_press(state, NavDirection::Up);
            } else {
                if let Some(action) =
                    apply_submenu_choice_delta(state, asset_manager, -1, NavWrap::Wrap)
                {
                    on_lr_press(state, -1);
                    return action;
                }
                on_lr_press(state, -1);
            }
        }
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => {
            if !ev.pressed {
                on_nav_release(state, NavDirection::Down);
                on_lr_release(state, 1);
            } else if matches!(state.view, OptionsView::Main) || selected_row_lr_navigates(state) {
                move_options_selection_vertical(state, asset_manager, NavDirection::Down);
                on_nav_press(state, NavDirection::Down);
            } else {
                if let Some(action) =
                    apply_submenu_choice_delta(state, asset_manager, 1, NavWrap::Wrap)
                {
                    on_lr_press(state, 1);
                    return action;
                }
                on_lr_press(state, 1);
            }
        }
        VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
            return activate_current_selection(state, asset_manager);
        }
        _ => {}
    }
    ThemeEffect::None
}
