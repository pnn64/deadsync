use super::*;
use crate::assets::AssetManager;
use crate::config::LightsDriverKind;
use crate::config::{MAX_FPS_MAX, MAX_FPS_MIN};
use deadsync_core::input::InputSource;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile as profile_data;
use deadsync_theme::views::{
    AppPathView, AppPathsView, AudioOutputDeviceView, GraphicsOptionsView, NoteskinCatalogView,
    SmxAssignmentPadView, SmxAssignmentView, SmxGifCatalogView,
};
use std::time::{Duration, Instant};

fn init() -> State {
    init_with_audio(AudioOptionsView::default())
}

fn test_app_paths() -> AppPathsView {
    let view = |path: &str| AppPathView {
        path: path.into(),
        display: path.to_owned(),
    };
    AppPathsView {
        data: view("/data"),
        cache: view("/cache"),
        songs: view("/data/songs"),
        courses: view("/data/courses"),
        profiles: view("/data/save/profiles"),
        screenshots: view("/data/save/screenshots"),
        log_file: view("/data/deadsync.log"),
        config_file: view("/data/deadsync.ini"),
    }
}

fn init_with_audio(audio_options: AudioOptionsView) -> State {
    super::init(
        SimplyLoveUpdaterCapabilities {
            app_update: true,
            ffmpeg_install: true,
        },
        test_app_paths(),
        audio_options,
        GraphicsOptionsView {
            software_thread_choices: vec![0, 1, 2],
            ..GraphicsOptionsView::default()
        },
        Vec::new(),
        NoteskinCatalogView {
            names: vec![profile_data::NoteSkin::DEFAULT_NAME.to_owned()],
        },
        deadsync_theme::views::SmxAssignmentView::default(),
        deadsync_theme::views::SmxGifCatalogView::default(),
    )
}

fn updater_view() -> SimplyLoveUpdaterView {
    SimplyLoveUpdaterView::default()
}

#[test]
fn smx_gif_choices_come_from_shell_catalog() {
    let state = super::init(
        SimplyLoveUpdaterCapabilities::default(),
        test_app_paths(),
        AudioOptionsView::default(),
        GraphicsOptionsView::default(),
        Vec::new(),
        NoteskinCatalogView::default(),
        SmxAssignmentView::default(),
        SmxGifCatalogView {
            background_packs: vec!["Background Pack".to_owned()],
            judgment_packs: vec!["Judgment Pack".to_owned()],
        },
    );

    assert_eq!(state.smx_bg_pack_choices, ["Background Pack"]);
    assert_eq!(state.smx_judge_pack_choices, ["Judgment Pack"]);
}

#[test]
fn smx_underglow_choice_emits_shell_hardware_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::SmxConfig);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::SmxConfig,
        SubRowId::SmxUnderglowTheme,
    );
    let before = state.sub[SubmenuKind::SmxConfig].cursor_indices[row];

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("underglow choice should emit shell work");
    let enabled = state.sub[SubmenuKind::SmxConfig].cursor_indices[row] == 1;

    assert_ne!(
        state.sub[SubmenuKind::SmxConfig].cursor_indices[row],
        before
    );
    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Hardware(
            crate::SimplyLoveHardwareRequest::SetSmxUnderglowTheme(value)
        )) if value == enabled
    ));
}

#[test]
fn select_music_choice_emits_shell_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::SelectMusic);
    let row = select_visible_row(&mut state, SubmenuKind::SelectMusic, SubRowId::ShowBanners);

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("Select Music choice should emit shell config work");
    let enabled = state.sub[SubmenuKind::SelectMusic].cursor_indices[row] == 1;

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::SelectMusic(
                crate::SimplyLoveSelectMusicConfigRequest::ShowBanners(value)
            )
        )) if value == enabled
    ));
}

#[test]
fn machine_choice_emits_shell_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Machine);
    let row = select_visible_row(&mut state, SubmenuKind::Machine, SubRowId::SelectProfile);

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("Machine choice should emit shell config work");
    let enabled = state.sub[SubmenuKind::Machine].cursor_indices[row] == 1;

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Machine(
                crate::SimplyLoveMachineConfigRequest::ShowSelectProfile(value)
            )
        )) if value == enabled
    ));
}

#[test]
fn advanced_choice_emits_shell_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Advanced);
    let row = select_visible_row(&mut state, SubmenuKind::Advanced, SubRowId::BannerCache);

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("Advanced choice should emit shell config work");
    let enabled = state.sub[SubmenuKind::Advanced].cursor_indices[row] == 1;

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Advanced(
                crate::SimplyLoveAdvancedConfigRequest::BannerCache(value)
            )
        )) if value == enabled
    ));
}

#[test]
fn course_choice_emits_shell_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Course);
    let row = select_visible_row(&mut state, SubmenuKind::Course, SubRowId::ShowRandomCourses);

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("Course choice should emit shell config work");
    let enabled = state.sub[SubmenuKind::Course].cursor_indices[row] == 1;

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Course(
                crate::SimplyLoveCourseConfigRequest::ShowRandom(value)
            )
        )) if value == enabled
    ));
}

#[test]
fn gameplay_choice_emits_shell_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Gameplay);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::Gameplay,
        SubRowId::CenteredP1Notefield,
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("Gameplay choice should emit shell config work");
    let enabled = state.sub[SubmenuKind::Gameplay].cursor_indices[row] == 1;

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Gameplay(
                crate::SimplyLoveGameplayConfigRequest::CenterPlayerOneNotefield(value)
            )
        )) if value == enabled
    ));
}

#[test]
fn lights_driver_choice_emits_shell_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Lights);
    let row = select_visible_row(&mut state, SubmenuKind::Lights, SubRowId::LightsDriver);

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("Lights driver choice should emit shell config work");
    let driver = lights_driver_from_index(state.sub[SubmenuKind::Lights].cursor_indices[row]);

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Lights(
                crate::SimplyLoveLightsConfigRequest::Driver(value)
            )
        )) if value == driver
    ));
}

#[test]
fn null_or_die_timing_choice_emits_shell_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::NullOrDieOptions);
    select_visible_row(
        &mut state,
        SubmenuKind::NullOrDieOptions,
        SubRowId::Fingerprint,
    );
    let delta = if state.null_or_die_fingerprint_tenths < NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS {
        1
    } else {
        -1
    };

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, delta, NavWrap::Clamp)
        .expect("Null-or-Die timing choice should emit shell config work");

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::NullOrDie(
                crate::SimplyLoveNullOrDieConfigRequest::FingerprintTenths(value)
            )
        )) if value == state.null_or_die_fingerprint_tenths
    ));
}

#[test]
fn online_enable_choice_persists_before_reinitializing_services() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::GrooveStats);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::GrooveStats,
        SubRowId::EnableGrooveStats,
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("online enable choice should emit shell work");
    let enabled = state.sub[SubmenuKind::GrooveStats].cursor_indices[row] == 1;
    let ThemeEffect::Batch(effects) = effect else {
        panic!("online enable choice should persist before reinitializing services");
    };

    assert_eq!(effects.len(), 2);
    assert!(matches!(
        &effects[0],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Online(
                crate::SimplyLoveOnlineConfigRequest::EnableGrooveStats(value)
            )
        )) if *value == enabled
    ));
    assert!(matches!(
        &effects[1],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Online(
            crate::SimplyLoveOnlineRequest::Reinitialize
        ))
    ));
}

fn input_event(action: VirtualAction, pressed: bool) -> InputEvent {
    let now = Instant::now();
    InputEvent {
        action,
        input_slot: 0,
        pressed,
        source: InputSource::Keyboard,
        timestamp: now,
        timestamp_host_nanos: 0,
        stored_at: now,
        emitted_at: now,
    }
}

fn smx_view(pads: [(bool, &str, &str); 2], can_swap: bool) -> SmxAssignmentView {
    SmxAssignmentView {
        pads: std::array::from_fn(|slot| SmxAssignmentPadView {
            connected: pads[slot].0,
            serial: pads[slot].1.to_owned(),
            label: pads[slot].2.to_owned(),
            input_state: 0,
            ..SmxAssignmentPadView::default()
        }),
        can_swap,
        conflict_warning: false,
        conflict_rgb: [1.0, 0.5, 0.0],
        player_rgb: [[0, 0, 255], [255, 0, 0]],
    }
}

#[test]
fn prepared_smx_view_drives_single_pad_state_and_request() {
    let mut state = init();
    let view = smx_view([(false, "", ""), (true, "PAD2", "SMX[P2]")], false);
    sync_smx_assignment(&mut state, &view);

    assert_eq!(state.smx_assignment, view);
    assert!(state.smx_assignment_status.contains("SMX[P2]"));
    let row_index = SMX_CONFIG_OPTIONS_ROWS
        .iter()
        .position(|row| row.id == SubRowId::SmxSinglePadPlayer)
        .unwrap();
    assert_eq!(
        state.sub[SubmenuKind::SmxConfig].choice_indices[row_index],
        1
    );
    assert!(matches!(
        single_pad_assignment_request(&state.smx_assignment, 0),
        Some(crate::SimplyLoveHardwareRequest::AssignSmxPads {
            p1_serial: Some(serial),
            p2_serial: None,
        }) if serial == "PAD2"
    ));
}

#[test]
fn prepared_smx_view_controls_assignment_row_visibility() {
    let mut state = init();
    let one = smx_view([(true, "PAD1", "SMX[P1]"), (false, "", "")], false);
    sync_smx_assignment(&mut state, &one);
    let visible =
        submenu_visible_row_indices(&state, SubmenuKind::SmxConfig, SMX_CONFIG_OPTIONS_ROWS);
    assert!(
        visible
            .iter()
            .any(|&index| { SMX_CONFIG_OPTIONS_ROWS[index].id == SubRowId::SmxSinglePadPlayer })
    );
    assert!(!visible.iter().any(|&index| {
        matches!(
            SMX_CONFIG_OPTIONS_ROWS[index].id,
            SubRowId::SmxAssignPads | SubRowId::SmxSwapPads
        )
    }));

    let two = smx_view([(true, "PAD1", "SMX[P1]"), (true, "PAD2", "SMX[P2]")], true);
    sync_smx_assignment(&mut state, &two);
    let visible =
        submenu_visible_row_indices(&state, SubmenuKind::SmxConfig, SMX_CONFIG_OPTIONS_ROWS);
    assert!(visible.iter().any(|&index| {
        matches!(
            SMX_CONFIG_OPTIONS_ROWS[index].id,
            SubRowId::SmxAssignPads | SubRowId::SmxSwapPads
        )
    }));
}

#[test]
fn srpg_shop_folder_is_hidden_when_shop_is_disabled() {
    let mut state = init();
    let show_index =
        row_position(GROOVESTATS_OPTIONS_ROWS, SubRowId::ShowSrpgShop).expect("show shop row");
    let folder_index =
        row_position(GROOVESTATS_OPTIONS_ROWS, SubRowId::SrpgShopFolder).expect("shop folder row");

    state.sub[SubmenuKind::GrooveStats].choice_indices[show_index] = yes_no_choice_index(false);
    let hidden =
        submenu_visible_row_indices(&state, SubmenuKind::GrooveStats, GROOVESTATS_OPTIONS_ROWS);
    assert!(!hidden.contains(&folder_index));

    state.sub[SubmenuKind::GrooveStats].choice_indices[show_index] = yes_no_choice_index(true);
    let visible =
        submenu_visible_row_indices(&state, SubmenuKind::GrooveStats, GROOVESTATS_OPTIONS_ROWS);
    assert!(visible.contains(&folder_index));
}

fn press(state: &mut State, asset_manager: &AssetManager, action: VirtualAction) -> ThemeEffect {
    handle_input(
        state,
        asset_manager,
        &updater_view(),
        &input_event(action, true),
    )
}

fn dedicated_press(
    state: &mut State,
    asset_manager: &AssetManager,
    action: VirtualAction,
) -> ThemeEffect {
    handle_dedicated_three_key_options_input(state, asset_manager, &input_event(action, true))
}

#[test]
fn audio_options_view_builds_and_rebuilds_localized_device_labels() {
    let audio_options = AudioOptionsView {
        output_devices: vec![
            AudioOutputDeviceView {
                name: "Primary Device".to_owned(),
                is_default: true,
                sample_rates_hz: vec![44_100, 48_000],
            },
            AudioOutputDeviceView {
                name: "Secondary Device".to_owned(),
                is_default: false,
                sample_rates_hz: vec![48_000],
            },
        ],
        available_backend_names: vec!["Auto".to_owned(), "ALSA".to_owned()],
        output_device: Some(1),
        output_mode: AudioOutputModeChoice::Shared,
        selected_backend_name: "ALSA".to_owned(),
        sample_rate_hz: Some(48_000),
        preserve_pitch: true,
        replay_gain: true,
        master_volume: 91,
        music_volume: 81,
        sfx_volume: 71,
        assist_tick_volume: 61,
    };
    let mut state = init_with_audio(audio_options.clone());

    assert_eq!(state.audio_options, audio_options);
    assert_eq!(state.audio_options.output_device, Some(1));
    assert_eq!(
        state.audio_options.output_mode,
        AudioOutputModeChoice::Shared
    );
    assert_eq!(state.audio_options.sample_rate_hz, Some(48_000));
    assert!(state.audio_options.preserve_pitch);
    assert!(state.audio_options.replay_gain);
    assert_eq!(state.master_volume_pct, 91);
    assert_eq!(state.music_volume_pct, 81);
    assert_eq!(state.sound_device_options.len(), 3);
    assert_eq!(state.sound_device_options[0].config_index, None);
    assert_eq!(
        state.sound_device_options[0].sample_rates_hz,
        [44_100, 48_000]
    );
    assert!(
        state.sound_device_options[1]
            .label
            .starts_with("Primary Device")
    );
    assert_eq!(state.sound_device_options[1].config_index, Some(0));
    assert_eq!(state.sound_device_options[2].label, "Secondary Device");
    assert_eq!(state.sound_device_options[2].config_index, Some(1));
    assert_eq!(
        get_choice_by_id(
            &state.sub[SubmenuKind::Sound].choice_indices,
            SOUND_OPTIONS_ROWS,
            SubRowId::SoundDevice,
        ),
        Some(2)
    );

    state.sound_device_options.clear();
    state.i18n_revision = u64::MAX;
    sync_i18n_cache(&mut state);
    assert_eq!(state.sound_device_options.len(), 3);
    assert!(
        state.sound_device_options[1]
            .label
            .starts_with("Primary Device")
    );
    #[cfg(target_os = "linux")]
    assert_eq!(
        state.linux_backend_choices,
        [tr("Common", "Auto").to_string(), "ALSA".to_owned()]
    );
}

#[test]
fn sound_device_change_emits_output_and_invalid_rate_requests() {
    let asset_manager = AssetManager::new();
    let mut state = init_with_audio(AudioOptionsView {
        output_devices: vec![AudioOutputDeviceView {
            name: "48 kHz only".to_owned(),
            is_default: false,
            sample_rates_hz: vec![48_000],
        }],
        output_device: None,
        sample_rate_hz: Some(44_100),
        ..AudioOptionsView::default()
    });
    state.view = OptionsView::Submenu(SubmenuKind::Sound);
    select_visible_row(&mut state, SubmenuKind::Sound, SubRowId::SoundDevice);

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Clamp)
        .expect("device change should emit requests");

    assert_eq!(state.audio_options.output_device, Some(0));
    assert_eq!(state.audio_options.sample_rate_hz, None);
    assert_eq!(sample_rate_choice_index(&state, None), 0);
    let ThemeEffect::Batch(effects) = effect else {
        panic!("expected batched device/rate requests");
    };
    assert_eq!(effects.len(), 2);
    assert!(matches!(
        &effects[0],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            AudioRequest::SetOutputDevice(Some(0))
        ))
    ));
    assert!(matches!(
        &effects[1],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            AudioRequest::SetSampleRate(None)
        ))
    ));
}

#[test]
fn sound_runtime_toggles_emit_neutral_audio_requests() {
    let asset_manager = AssetManager::new();
    let mut state = init_with_audio(AudioOptionsView {
        preserve_pitch: false,
        replay_gain: false,
        ..AudioOptionsView::default()
    });
    state.view = OptionsView::Submenu(SubmenuKind::Sound);

    select_visible_row(
        &mut state,
        SubmenuKind::Sound,
        SubRowId::RateModPreservesPitch,
    );
    let pitch = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Clamp);
    assert!(matches!(
        pitch,
        Some(ThemeEffect::Runtime(
            crate::SimplyLoveRuntimeRequest::Audio(AudioRequest::SetPreservePitch(true))
        ))
    ));
    assert!(state.audio_options.preserve_pitch);

    select_visible_row(&mut state, SubmenuKind::Sound, SubRowId::ReplayGain);
    let replay_gain = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Clamp);
    assert!(matches!(
        replay_gain,
        Some(ThemeEffect::Runtime(
            crate::SimplyLoveRuntimeRequest::Audio(AudioRequest::SetReplayGain(true))
        ))
    ));
    assert!(state.audio_options.replay_gain);

    set_sound_choice_index(&mut state, SubRowId::MineSounds, 0);
    select_visible_row(&mut state, SubmenuKind::Sound, SubRowId::MineSounds);
    let mine_sound = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Clamp);
    assert!(matches!(
        mine_sound,
        Some(ThemeEffect::Runtime(
            crate::SimplyLoveRuntimeRequest::Audio(AudioRequest::SetMineHitSound(true))
        ))
    ));

    state.global_offset_ms = 0;
    select_visible_row(&mut state, SubmenuKind::Sound, SubRowId::GlobalOffset);
    let global_offset = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Clamp);
    assert!(matches!(
        global_offset,
        Some(ThemeEffect::Runtime(
            crate::SimplyLoveRuntimeRequest::Audio(AudioRequest::SetGlobalOffsetMillis(1))
        ))
    ));
    assert_eq!(state.global_offset_ms, 1);
}

fn age_start_hold(state: &mut State, side: profile_data::PlayerSide) {
    let idx = profile_data::player_side_index(side);
    state.start_input[idx].held = true;
    state.start_input[idx].held_for = NAV_INITIAL_HOLD_DELAY;
    state.start_input[idx].next_repeat_at = NAV_INITIAL_HOLD_DELAY;
}

fn repeat_tick_dt() -> f32 {
    Duration::from_millis(1).as_secs_f32()
}

fn select_visible_row(state: &mut State, kind: SubmenuKind, row_id: SubRowId) -> usize {
    let rows = submenu_rows(kind);
    let actual = row_position(rows, row_id).expect("row should exist");
    let visible = submenu_visible_row_indices(state, kind, rows);
    state.sub_selected = visible
        .iter()
        .position(|&idx| idx == actual)
        .expect("row should be visible");
    actual
}

#[test]
fn inferred_aspect_choice_maps_1024x768_to_4_3() {
    let idx = display_aspect_choice_index(1024, 768);
    assert!(matches!(
        DISPLAY_ASPECT_RATIO_CHOICES[idx],
        Choice::Literal("4:3")
    ));
}

#[test]
fn sync_display_resolution_selects_loaded_4_3_mode() {
    let mut state = init();
    sync_display_resolution(&mut state, 1024, 768);

    assert_eq!(selected_aspect_label(&state), "4:3");
    assert_eq!(selected_resolution(&state), (1024, 768));
    assert!(state.resolution_choices.contains(&(1024, 768)));
}

#[test]
fn max_fps_choices_are_single_fps_steps() {
    let choices = build_max_fps_choices();

    assert_eq!(choices.first().copied(), Some(MAX_FPS_MIN));
    assert_eq!(choices.get(1).copied(), Some(MAX_FPS_MIN + 1));
    assert!(choices.contains(&60));
    assert!(choices.contains(&600));
    assert_eq!(choices.last().copied(), Some(MAX_FPS_MAX));
}

#[test]
fn max_fps_hold_delta_accelerates() {
    assert_eq!(max_fps_hold_delta(1, Duration::from_millis(300)), 5);
    assert_eq!(max_fps_hold_delta(1, Duration::from_millis(700)), 10);
    assert_eq!(max_fps_hold_delta(1, Duration::from_millis(1200)), 25);
    assert_eq!(max_fps_hold_delta(-1, Duration::from_millis(1800)), -50);
}

#[test]
fn input_backend_items_match_rows() {
    let expected = [
        (SubRowId::GamepadBackend, ItemId::InpGamepadBackend),
        (SubRowId::UseFsrs, ItemId::InpUseFsrs),
        (SubRowId::SmxConfig, ItemId::InpSmxConfig),
        (SubRowId::DebugFsrDump, ItemId::InpDebugFsrDump),
        (SubRowId::MenuNavigation, ItemId::InpMenuNavigation),
        (SubRowId::OptionsNavigation, ItemId::InpOptionsNavigation),
        (SubRowId::MenuButtons, ItemId::InpMenuButtons),
        (SubRowId::Debounce, ItemId::InpDebounce),
    ];

    assert_eq!(
        INPUT_BACKEND_OPTIONS_ROWS.len() + 1,
        INPUT_BACKEND_OPTIONS_ITEMS.len()
    );
    for (idx, (row_id, item_id)) in expected.into_iter().enumerate() {
        assert_eq!(INPUT_BACKEND_OPTIONS_ROWS[idx].id, row_id);
        assert_eq!(INPUT_BACKEND_OPTIONS_ITEMS[idx].id, item_id);
    }
    assert_eq!(INPUT_BACKEND_OPTIONS_ITEMS.last().unwrap().id, ItemId::Exit);
}

#[test]
fn smx_config_items_match_rows() {
    let expected = [
        (SubRowId::SmxInput, ItemId::InpSmxInput),
        (SubRowId::SmxPanelLights, ItemId::InpSmxPanelLights),
        (SubRowId::SmxUnderglowTheme, ItemId::InpSmxUnderglowTheme),
        (SubRowId::SmxUnderglowGrb, ItemId::InpSmxUnderglowGrb),
        (
            SubRowId::SmxManagesPadConfig,
            ItemId::InpSmxManagesPadConfig,
        ),
        (
            SubRowId::SmxDefaultPadConfig,
            ItemId::InpSmxDefaultPadConfig,
        ),
        (SubRowId::SmxSinglePadPlayer, ItemId::InpSmxSinglePadPlayer),
        (
            SubRowId::SmxDefaultLightBrightness,
            ItemId::InpSmxDefaultLightBrightness,
        ),
        (SubRowId::SmxBgPack, ItemId::InpSmxBgPack),
        (SubRowId::SmxJudgePack, ItemId::InpSmxJudgePack),
        (SubRowId::SmxAssignPads, ItemId::InpSmxAssignPads),
        (SubRowId::SmxSwapPads, ItemId::InpSmxSwapPads),
    ];

    assert_eq!(
        SMX_CONFIG_OPTIONS_ROWS.len() + 1,
        SMX_CONFIG_OPTIONS_ITEMS.len()
    );
    for (idx, (row_id, item_id)) in expected.into_iter().enumerate() {
        assert_eq!(SMX_CONFIG_OPTIONS_ROWS[idx].id, row_id);
        assert_eq!(SMX_CONFIG_OPTIONS_ITEMS[idx].id, item_id);
    }
    assert_eq!(SMX_CONFIG_OPTIONS_ITEMS.last().unwrap().id, ItemId::Exit);
}

#[test]
fn lights_driver_choices_roundtrip() {
    let cases = [
        (LightsDriverKind::Off, crate::SimplyLoveLightsDriver::Off),
        (LightsDriverKind::Snek, crate::SimplyLoveLightsDriver::Snek),
        (
            LightsDriverKind::Litboard,
            crate::SimplyLoveLightsDriver::Litboard,
        ),
        (
            LightsDriverKind::Win32Serial,
            crate::SimplyLoveLightsDriver::Win32Serial,
        ),
        (
            LightsDriverKind::Fusion,
            crate::SimplyLoveLightsDriver::Fusion,
        ),
        (LightsDriverKind::Gpb, crate::SimplyLoveLightsDriver::Gpb),
        (
            LightsDriverKind::PacDrive,
            crate::SimplyLoveLightsDriver::PacDrive,
        ),
        (
            LightsDriverKind::PiuioLeds,
            crate::SimplyLoveLightsDriver::PiuioLeds,
        ),
        (
            LightsDriverKind::Itgio,
            crate::SimplyLoveLightsDriver::Itgio,
        ),
        (
            LightsDriverKind::HidBlueDot,
            crate::SimplyLoveLightsDriver::HidBlueDot,
        ),
        (
            LightsDriverKind::Stac2,
            crate::SimplyLoveLightsDriver::Stac2,
        ),
        (
            LightsDriverKind::MinimaidHid,
            crate::SimplyLoveLightsDriver::MinimaidHid,
        ),
    ];

    assert_eq!(LIGHTS_OPTIONS_ROWS[0].choices.len(), cases.len());
    assert!(
        !LIGHTS_OPTIONS_ROWS[0].inline,
        "the driver list is too long to render every choice in one row"
    );
    for (driver, request_driver) in cases {
        let idx = lights_driver_choice_index(driver);
        assert_eq!(lights_driver_from_index(idx), request_driver);
        assert!(matches!(
            LIGHTS_OPTIONS_ROWS[0].choices[idx],
            Choice::Literal(label) if label == driver.as_str()
        ));
    }
}

#[test]
fn lights_items_match_rows() {
    let expected = [
        (SubRowId::LightsDriver, ItemId::LightsDriver),
        (SubRowId::GameplayPadLights, ItemId::LightsGameplayPadLights),
        (SubRowId::LightsSimplifyBass, ItemId::LightsSimplifyBass),
        (SubRowId::TestLights, ItemId::LightsTest),
    ];

    assert_eq!(LIGHTS_OPTIONS_ROWS.len() + 1, LIGHTS_OPTIONS_ITEMS.len());
    for (idx, (row_id, item_id)) in expected.into_iter().enumerate() {
        assert_eq!(LIGHTS_OPTIONS_ROWS[idx].id, row_id);
        assert_eq!(LIGHTS_OPTIONS_ITEMS[idx].id, item_id);
    }
    assert_eq!(LIGHTS_OPTIONS_ITEMS.last().unwrap().id, ItemId::Exit);
}

#[test]
fn graphics_hide_cursor_item_matches_row() {
    let row_idx = row_position(GRAPHICS_OPTIONS_ROWS, SubRowId::HideMouseCursor)
        .expect("hide cursor row should exist");

    assert_eq!(
        GRAPHICS_OPTIONS_ITEMS.len(),
        GRAPHICS_OPTIONS_ROWS.len() + 1
    );
    assert_eq!(
        GRAPHICS_OPTIONS_ITEMS[row_idx].id,
        ItemId::GfxHideMouseCursor
    );
}

#[test]
fn graphics_overscan_item_matches_row() {
    let row_idx = row_position(GRAPHICS_OPTIONS_ROWS, SubRowId::OverscanAdjustment)
        .expect("overscan row should exist");

    assert_eq!(
        GRAPHICS_OPTIONS_ITEMS[row_idx].id,
        ItemId::GfxOverscanAdjustment
    );
}

#[test]
fn p2_can_navigate_and_change_system_options() {
    let asset_manager = AssetManager::new();
    let mut state = init();

    assert_eq!(state.selected, 0);
    press(&mut state, &asset_manager, VirtualAction::p2_start);
    update(
        &mut state,
        1.0,
        &asset_manager,
        &SmxAssignmentView::default(),
    );
    update(
        &mut state,
        1.0,
        &asset_manager,
        &SmxAssignmentView::default(),
    );
    assert!(matches!(
        state.view,
        OptionsView::Submenu(SubmenuKind::System)
    ));

    press(&mut state, &asset_manager, VirtualAction::p2_down);
    press(&mut state, &asset_manager, VirtualAction::p2_down);
    press(&mut state, &asset_manager, VirtualAction::p2_down);
    assert_eq!(state.sub_selected, 3);

    let before = state.sub[SubmenuKind::System].cursor_indices[3];
    press(&mut state, &asset_manager, VirtualAction::p2_right);
    assert_eq!(state.sub[SubmenuKind::System].cursor_indices[3], before + 1);
}

#[test]
fn main_options_left_right_move_rows_like_up_down() {
    let asset_manager = AssetManager::new();
    let mut state = init();

    assert_eq!(state.selected, 0);
    press(&mut state, &asset_manager, VirtualAction::p1_right);
    assert_eq!(state.selected, 1);
    press(&mut state, &asset_manager, VirtualAction::p1_left);
    assert_eq!(state.selected, 0);
    press(&mut state, &asset_manager, VirtualAction::p2_left);
    assert_eq!(state.selected, visible_items(&state).len() - 1);
    press(&mut state, &asset_manager, VirtualAction::p2_right);
    assert_eq!(state.selected, 0);
}

#[test]
fn link_row_pages_lr_moves_rows_in_standard_mode() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Input);

    // Every row on the Input launcher page is an "Open" link, so Left/Right
    // navigates up/down exactly like the main options menu.
    assert_eq!(state.sub_selected, 0);
    press(&mut state, &asset_manager, VirtualAction::p1_right);
    assert_eq!(state.sub_selected, 1);
    press(&mut state, &asset_manager, VirtualAction::p1_left);
    assert_eq!(state.sub_selected, 0);
}

#[test]
fn value_rows_keep_left_right_for_adjustment() {
    let asset_manager = AssetManager::new();
    let mut state = init_with_audio(AudioOptionsView {
        master_volume: 50,
        ..AudioOptionsView::default()
    });
    state.view = OptionsView::Submenu(SubmenuKind::Sound);
    select_visible_row(&mut state, SubmenuKind::Sound, SubRowId::MasterVolume);

    // Master Volume's single choice is a numeric placeholder, not a link:
    // Left adjusts the value and must not move the cursor.
    let row_before = state.sub_selected;
    let volume_before = state.master_volume_pct;
    press(&mut state, &asset_manager, VirtualAction::p1_left);
    assert_eq!(state.sub_selected, row_before);
    assert!(state.master_volume_pct < volume_before);
}

#[test]
fn volume_rows_emit_shell_request_before_feedback_sound() {
    let asset_manager = AssetManager::new();
    let mut state = init_with_audio(AudioOptionsView {
        master_volume: 50,
        ..AudioOptionsView::default()
    });
    state.view = OptionsView::Submenu(SubmenuKind::Sound);
    select_visible_row(&mut state, SubmenuKind::Sound, SubRowId::MasterVolume);

    let Some(ThemeEffect::Batch(effects)) =
        apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
    else {
        panic!("volume adjustment should emit an ordered request batch");
    };
    assert!(matches!(
        effects[0],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            deadsync_theme::AudioRequest::SetVolume {
                target: deadsync_theme::AudioVolumeTarget::Master,
                percent: 51,
            }
        ))
    ));
    assert!(matches!(
        &effects[1],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            deadsync_theme::AudioRequest::PlaySfx(path)
        )) if path == "assets/sounds/change_value.ogg"
    ));
}

#[test]
fn link_row_lr_release_clears_the_nav_hold() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Input);

    // Press on a link row arms hold-to-scroll; the release must clear it even
    // though the cursor moved to a different row in between.
    press(&mut state, &asset_manager, VirtualAction::p1_right);
    assert_eq!(state.nav_key_held_direction, Some(NavDirection::Down));
    handle_input(
        &mut state,
        &asset_manager,
        &updater_view(),
        &input_event(VirtualAction::p1_right, false),
    );
    assert_eq!(state.nav_key_held_direction, None);
}

#[test]
fn input_launcher_three_key_lr_moves_rows_like_service_menu() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Input);

    assert_eq!(state.sub_selected, 0);
    dedicated_press(&mut state, &asset_manager, VirtualAction::p1_right);
    assert_eq!(state.sub_selected, 1);
    dedicated_press(&mut state, &asset_manager, VirtualAction::p1_left);
    assert_eq!(state.sub_selected, 0);
    dedicated_press(&mut state, &asset_manager, VirtualAction::p2_left);
    assert_eq!(state.sub_selected, INPUT_OPTIONS_ROWS.len());
}

#[test]
fn input_launcher_three_key_start_opens_real_input_options() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Input);
    select_visible_row(&mut state, SubmenuKind::Input, SubRowId::InputOptions);

    dedicated_press(&mut state, &asset_manager, VirtualAction::p1_start);

    assert_eq!(state.pending_submenu_kind, Some(SubmenuKind::InputBackend));
    assert_eq!(state.pending_submenu_parent_kind, Some(SubmenuKind::Input));
    assert_eq!(
        state.submenu_transition,
        SubmenuTransition::FadeOutToSubmenu
    );
}

#[test]
fn service_child_three_key_lr_changes_value_not_row() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Graphics);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::Graphics,
        SubRowId::DisplayAspectRatio,
    );
    let before_row = state.sub_selected;
    let before_choice = state.sub[SubmenuKind::Graphics].cursor_indices[row];
    let choices = row_choices(&state, SubmenuKind::Graphics, GRAPHICS_OPTIONS_ROWS, row);
    assert!(choices.len() > 1);

    dedicated_press(&mut state, &asset_manager, VirtualAction::p1_right);

    assert_eq!(state.sub_selected, before_row);
    assert_ne!(
        state.sub[SubmenuKind::Graphics].cursor_indices[row],
        before_choice
    );
}

#[test]
fn service_child_three_key_lr_repeat_uses_update_dt() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Graphics);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::Graphics,
        SubRowId::DisplayAspectRatio,
    );

    dedicated_press(&mut state, &asset_manager, VirtualAction::p1_right);
    let after_press = state.sub[SubmenuKind::Graphics].cursor_indices[row];

    update(
        &mut state,
        0.0,
        &asset_manager,
        &SmxAssignmentView::default(),
    );
    assert_eq!(
        state.sub[SubmenuKind::Graphics].cursor_indices[row],
        after_press
    );

    update(
        &mut state,
        (NAV_INITIAL_HOLD_DELAY + Duration::from_millis(1)).as_secs_f32(),
        &asset_manager,
        &SmxAssignmentView::default(),
    );
    assert_ne!(
        state.sub[SubmenuKind::Graphics].cursor_indices[row],
        after_press
    );
}

#[test]
fn service_child_three_key_start_moves_down_one_row() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Graphics);
    state.sub_selected = 0;

    dedicated_press(&mut state, &asset_manager, VirtualAction::p1_start);

    assert_eq!(state.sub_selected, 1);
}

#[test]
fn online_scoring_three_key_start_opens_gs_options() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::OnlineScoring);
    select_visible_row(
        &mut state,
        SubmenuKind::OnlineScoring,
        SubRowId::GsBsOptions,
    );

    dedicated_press(&mut state, &asset_manager, VirtualAction::p1_start);

    assert_eq!(state.pending_submenu_kind, Some(SubmenuKind::GrooveStats));
    assert_eq!(
        state.pending_submenu_parent_kind,
        Some(SubmenuKind::OnlineScoring)
    );
    assert_eq!(
        state.submenu_transition,
        SubmenuTransition::FadeOutToSubmenu
    );
}

#[test]
fn online_scoring_three_key_start_opens_arrowcloud_options() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::OnlineScoring);
    select_visible_row(
        &mut state,
        SubmenuKind::OnlineScoring,
        SubRowId::ArrowCloudOptions,
    );

    dedicated_press(&mut state, &asset_manager, VirtualAction::p1_start);

    assert_eq!(state.pending_submenu_kind, Some(SubmenuKind::ArrowCloud));
    assert_eq!(
        state.pending_submenu_parent_kind,
        Some(SubmenuKind::OnlineScoring)
    );
    assert_eq!(
        state.submenu_transition,
        SubmenuTransition::FadeOutToSubmenu
    );
}

#[test]
fn service_child_three_key_left_right_start_moves_up_one_row() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Graphics);
    state.sub_selected = 1;
    screen_input::track_menu_lr_chord(
        &mut state.menu_lr_chord,
        &input_event(VirtualAction::p1_left, true),
    );
    screen_input::track_menu_lr_chord(
        &mut state.menu_lr_chord,
        &input_event(VirtualAction::p1_right, true),
    );

    dedicated_press(&mut state, &asset_manager, VirtualAction::p1_start);

    assert_eq!(state.sub_selected, 0);
}

#[test]
fn service_child_three_key_exit_left_right_start_moves_up() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Graphics);
    let exit_row = submenu_total_rows(&state, SubmenuKind::Graphics).saturating_sub(1);
    state.sub_selected = exit_row;
    screen_input::track_menu_lr_chord(
        &mut state.menu_lr_chord,
        &input_event(VirtualAction::p1_left, true),
    );
    screen_input::track_menu_lr_chord(
        &mut state.menu_lr_chord,
        &input_event(VirtualAction::p1_right, true),
    );

    dedicated_press(&mut state, &asset_manager, VirtualAction::p1_start);

    assert_eq!(state.sub_selected, exit_row - 1);
    assert_eq!(state.submenu_transition, SubmenuTransition::None);
}

#[test]
fn service_child_three_key_held_start_repeats_down() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Graphics);
    state.sub_selected = 0;

    dedicated_press(&mut state, &asset_manager, VirtualAction::p1_start);
    assert_eq!(state.sub_selected, 1);

    age_start_hold(&mut state, profile_data::PlayerSide::P1);
    assert!(
        repeat_held_dedicated_three_key_start(
            &mut state,
            &asset_manager,
            profile_data::PlayerSide::P1,
            repeat_tick_dt(),
        )
        .is_none()
    );

    assert_eq!(state.sub_selected, 2);
}

#[test]
fn service_child_three_key_held_left_right_start_repeats_up() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Graphics);
    state.sub_selected = 2;
    screen_input::track_menu_lr_chord(
        &mut state.menu_lr_chord,
        &input_event(VirtualAction::p1_left, true),
    );
    screen_input::track_menu_lr_chord(
        &mut state.menu_lr_chord,
        &input_event(VirtualAction::p1_right, true),
    );

    age_start_hold(&mut state, profile_data::PlayerSide::P1);
    assert!(
        repeat_held_dedicated_three_key_start(
            &mut state,
            &asset_manager,
            profile_data::PlayerSide::P1,
            repeat_tick_dt(),
        )
        .is_none()
    );

    assert_eq!(state.sub_selected, 1);
}

#[test]
fn service_child_three_key_held_start_stops_at_exit() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Graphics);
    let exit_row = submenu_total_rows(&state, SubmenuKind::Graphics).saturating_sub(1);
    state.sub_selected = exit_row;

    age_start_hold(&mut state, profile_data::PlayerSide::P1);
    assert!(
        repeat_held_dedicated_three_key_start(
            &mut state,
            &asset_manager,
            profile_data::PlayerSide::P1,
            repeat_tick_dt(),
        )
        .is_none()
    );

    assert_eq!(state.sub_selected, exit_row);
    assert_eq!(state.submenu_transition, SubmenuTransition::None);
}

#[test]
fn input_launcher_three_key_held_start_does_not_repeat_rows() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Input);
    state.sub_selected = 0;

    age_start_hold(&mut state, profile_data::PlayerSide::P1);
    assert!(
        repeat_held_dedicated_three_key_start(
            &mut state,
            &asset_manager,
            profile_data::PlayerSide::P1,
            repeat_tick_dt(),
        )
        .is_none()
    );

    assert_eq!(state.sub_selected, 0);
}

#[test]
fn preferred_color_only_shows_when_select_color_is_off() {
    let mut state = init();

    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::SelectColor,
        yes_no_choice_index(true),
    );
    let visible = submenu_visible_row_indices(&state, SubmenuKind::Machine, MACHINE_OPTIONS_ROWS);
    assert!(
        !visible
            .iter()
            .any(|&idx| MACHINE_OPTIONS_ROWS[idx].id == SubRowId::PreferredColor)
    );

    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::SelectColor,
        yes_no_choice_index(false),
    );
    let visible = submenu_visible_row_indices(&state, SubmenuKind::Machine, MACHINE_OPTIONS_ROWS);
    assert!(
        visible
            .iter()
            .any(|&idx| MACHINE_OPTIONS_ROWS[idx].id == SubRowId::PreferredColor)
    );
}

#[test]
fn default_sync_offset_only_shows_when_pack_offsets_are_on() {
    let mut state = init();

    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::PackIniOffsets,
        yes_no_choice_index(false),
    );
    let visible = submenu_visible_row_indices(&state, SubmenuKind::Machine, MACHINE_OPTIONS_ROWS);
    assert!(
        !visible
            .iter()
            .any(|&idx| MACHINE_OPTIONS_ROWS[idx].id == SubRowId::DefaultSyncOffset)
    );

    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::PackIniOffsets,
        yes_no_choice_index(true),
    );
    let visible = submenu_visible_row_indices(&state, SubmenuKind::Machine, MACHINE_OPTIONS_ROWS);
    assert!(
        visible
            .iter()
            .any(|&idx| MACHINE_OPTIONS_ROWS[idx].id == SubRowId::DefaultSyncOffset)
    );
}

#[test]
fn folders_submenu_is_registered() {
    assert!(SubmenuKind::ALL.contains(&SubmenuKind::Folders));
    assert_eq!(submenu_rows(SubmenuKind::Folders).len(), 8);
    // FOLDERS_OPTIONS_ITEMS has 8 folder entries plus the Exit row.
    assert_eq!(submenu_items(SubmenuKind::Folders).len(), 9);
    assert_eq!(submenu_title(SubmenuKind::Folders), "FOLDERS");
}

#[test]
fn folders_top_level_item_opens_folders_submenu() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    let item_pos = visible_items(&state)
        .iter()
        .position(|item| item.id == ItemId::FoldersOptions)
        .expect("FoldersOptions should be visible on the main Options screen");
    state.selected = item_pos;

    press(&mut state, &asset_manager, VirtualAction::p1_start);

    assert_eq!(state.pending_submenu_kind, Some(SubmenuKind::Folders));
    assert_eq!(
        state.submenu_transition,
        SubmenuTransition::FadeOutToSubmenu
    );
}

#[test]
fn folder_rows_build_typed_reveal_requests() {
    let paths = test_app_paths();
    let expectations = [
        (
            SubRowId::FoldersDataDir,
            deadsync_theme::views::AppPathKind::Data,
            deadsync_theme::RevealPathKind::Directory,
        ),
        (
            SubRowId::FoldersCacheDir,
            deadsync_theme::views::AppPathKind::Cache,
            deadsync_theme::RevealPathKind::Directory,
        ),
        (
            SubRowId::FoldersSongs,
            deadsync_theme::views::AppPathKind::Songs,
            deadsync_theme::RevealPathKind::Directory,
        ),
        (
            SubRowId::FoldersCourses,
            deadsync_theme::views::AppPathKind::Courses,
            deadsync_theme::RevealPathKind::Directory,
        ),
        (
            SubRowId::FoldersProfiles,
            deadsync_theme::views::AppPathKind::Profiles,
            deadsync_theme::RevealPathKind::Directory,
        ),
        (
            SubRowId::FoldersScreenshots,
            deadsync_theme::views::AppPathKind::Screenshots,
            deadsync_theme::RevealPathKind::Directory,
        ),
        (
            SubRowId::FoldersLogFile,
            deadsync_theme::views::AppPathKind::LogFile,
            deadsync_theme::RevealPathKind::File,
        ),
        (
            SubRowId::FoldersConfigFile,
            deadsync_theme::views::AppPathKind::ConfigFile,
            deadsync_theme::RevealPathKind::File,
        ),
    ];
    for (id, path_kind, kind) in expectations {
        let expected = &paths.get(path_kind).path;
        assert_eq!(
            folder_path_for_row(&paths, id),
            Some(expected.as_path()),
            "row {:?} should resolve to {}",
            id,
            expected.display()
        );
        assert_eq!(
            folder_reveal_request(&paths, id),
            Some(deadsync_theme::PlatformRequest::RevealPath {
                path: expected.clone(),
                kind,
            })
        );
    }

    assert!(folder_path_for_row(&paths, SubRowId::Game).is_none());
    assert!(folder_reveal_request(&paths, SubRowId::Game).is_none());
}

#[test]
fn folder_activation_requests_audio_before_platform_reveal() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    let expected_path = state.app_paths.data.path.clone();
    state.view = OptionsView::Submenu(SubmenuKind::Folders);
    select_visible_row(&mut state, SubmenuKind::Folders, SubRowId::FoldersDataDir);

    let effect = activate_current_selection(&mut state, &asset_manager);
    let ThemeEffect::Batch(effects) = effect else {
        panic!("expected batched folder effect");
    };
    assert_eq!(effects.len(), 2);
    assert!(matches!(
        &effects[0],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            deadsync_theme::AudioRequest::PlaySfx(path)
        )) if path == "assets/sounds/start.ogg"
    ));
    assert!(matches!(
        &effects[1],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Platform(
            deadsync_theme::PlatformRequest::RevealPath { path, kind }
        )) if path == &expected_path
            && *kind == deadsync_theme::RevealPathKind::Directory
    ));
}

#[test]
fn queued_sfx_precede_follow_up_runtime_work() {
    let mut state = init();
    queue_sfx(&mut state, "assets/sounds/change_value.ogg");
    let effect = ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
        crate::SimplyLoveConfigRequest::ShowOverlay(2),
    ));

    let ThemeEffect::Batch(effects) = prepend_pending_sfx(&mut state, effect) else {
        panic!("queued sound and config work should be batched");
    };
    assert!(matches!(
        &effects[0],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            deadsync_theme::AudioRequest::PlaySfx(path)
        )) if path == "assets/sounds/change_value.ogg"
    ));
    assert!(matches!(
        effects[1],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::ShowOverlay(2)
        ))
    ));
    assert!(state.pending_sfx.is_empty());
}

#[test]
fn queued_sfx_precede_score_import_runtime_work() {
    let mut state = init();
    queue_sfx(&mut state, "assets/sounds/start.ogg");
    queue_online(
        &mut state,
        crate::SimplyLoveOnlineRequest::CancelScoreImport,
    );

    let ThemeEffect::Batch(effects) = prepend_pending_sfx(&mut state, ThemeEffect::None) else {
        panic!("queued sound and score-import work should be batched");
    };
    assert!(matches!(
        &effects[0],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            deadsync_theme::AudioRequest::PlaySfx(path)
        )) if path == "assets/sounds/start.ogg"
    ));
    assert!(matches!(
        effects[1],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Online(
            crate::SimplyLoveOnlineRequest::CancelScoreImport
        ))
    ));
    assert!(state.pending_sfx.is_empty());
    assert!(state.pending_online.is_empty());
}

#[test]
fn score_import_events_update_the_theme_overlay() {
    let mut state = init();
    state.score_import_ui = Some(ScoreImportUiState::new(
        score_data::ScoreImportEndpoint::GrooveStats,
        "Player".to_owned(),
        "All Packs".to_owned(),
    ));
    apply_score_import_events(
        &mut state,
        vec![
            crate::SimplyLoveScoreImportEvent::Progress(crate::SimplyLoveScoreImportProgress {
                processed_charts: 4,
                total_charts: 10,
                imported_scores: 3,
                missing_scores: 1,
                failed_requests: 0,
                detail: "Checking chart".to_owned(),
            }),
            crate::SimplyLoveScoreImportEvent::Finished(Ok(crate::SimplyLoveScoreImportSummary {
                requested_charts: 10,
                imported_scores: 7,
                missing_scores: 2,
                failed_requests: 1,
                rate_limit_per_second: 3,
                elapsed_seconds: 5.0,
                canceled: false,
            })),
        ],
    );

    let overlay = state.score_import_ui.expect("score-import overlay");
    assert_eq!(overlay.processed_charts, 4);
    assert_eq!(overlay.detail_line, "Checking chart");
    assert!(overlay.done);
    assert!(overlay.done_message.contains("imported=7"));
}

#[test]
fn score_import_profile_debug_redacts_api_keys() {
    let profile = crate::SimplyLoveScoreImportProfile {
        id: "profile".to_owned(),
        display_name: "Player".to_owned(),
        groovestats_api_key: "gs-secret".to_owned(),
        groovestats_username: "username".to_owned(),
        arrowcloud_api_key: "ac-secret".to_owned(),
    };
    let debug = format!("{profile:?}");
    assert!(!debug.contains("gs-secret"));
    assert!(!debug.contains("ac-secret"));
    assert!(debug.contains("<redacted>"));
}

#[test]
fn update_drain_emits_a_queued_sound_without_follow_up_work() {
    let mut state = init();
    queue_sfx(&mut state, "assets/sounds/change.ogg");

    let effect = prepend_pending_sfx_opt(&mut state, None)
        .expect("queued update sound should become an effect");
    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            deadsync_theme::AudioRequest::PlaySfx(path)
        )) if path == "assets/sounds/change.ogg"
    ));
    assert!(state.pending_sfx.is_empty());
}

/// Run pending submenu fades to completion (cap iterations so a stuck transition
/// fails the test rather than hanging).
fn settle_submenu(state: &mut State, asset_manager: &AssetManager) {
    for _ in 0..16 {
        if matches!(state.submenu_transition, SubmenuTransition::None) {
            return;
        }
        update(
            state,
            SUBMENU_FADE_DURATION + 0.001,
            asset_manager,
            &SmxAssignmentView::default(),
        );
    }
    panic!("submenu transition did not settle");
}

#[test]
fn input_backend_back_returns_to_input_not_root() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    // In the Input submenu, whose parent is the main list (no parent kind).
    state.view = OptionsView::Submenu(SubmenuKind::Input);
    state.submenu_parent_kind = None;

    // Open the inner Input Options (InputBackend) page.
    select_visible_row(&mut state, SubmenuKind::Input, SubRowId::InputOptions);
    activate_current_selection(&mut state, &asset_manager);
    settle_submenu(&mut state, &asset_manager);
    assert_eq!(state.view, OptionsView::Submenu(SubmenuKind::InputBackend));
    assert_eq!(state.submenu_parent_kind, Some(SubmenuKind::Input));

    // Back from the inner page must land on the parent Input submenu, not root.
    cancel_current_view(&mut state);
    settle_submenu(&mut state, &asset_manager);
    assert_eq!(state.view, OptionsView::Submenu(SubmenuKind::Input));
}

#[test]
fn input_backend_back_returns_to_input_after_visiting_smx_config() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Input);
    state.submenu_parent_kind = None;

    // Input -> InputBackend.
    select_visible_row(&mut state, SubmenuKind::Input, SubRowId::InputOptions);
    activate_current_selection(&mut state, &asset_manager);
    settle_submenu(&mut state, &asset_manager);

    // The SMX Config row only shows when FSRs are enabled.
    set_choice_by_id(
        &mut state.sub[SubmenuKind::InputBackend].choice_indices,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::UseFsrs,
        yes_no_choice_index(true),
    );

    // InputBackend -> SmxConfig, then back to InputBackend.
    select_visible_row(&mut state, SubmenuKind::InputBackend, SubRowId::SmxConfig);
    activate_current_selection(&mut state, &asset_manager);
    settle_submenu(&mut state, &asset_manager);
    assert_eq!(state.view, OptionsView::Submenu(SubmenuKind::SmxConfig));
    cancel_current_view(&mut state);
    settle_submenu(&mut state, &asset_manager);
    assert_eq!(state.view, OptionsView::Submenu(SubmenuKind::InputBackend));

    // The parent link back to Input must survive the round trip.
    cancel_current_view(&mut state);
    settle_submenu(&mut state, &asset_manager);
    assert_eq!(state.view, OptionsView::Submenu(SubmenuKind::Input));
}

#[test]
fn graphics_threads_emit_neutral_request_on_exit() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Graphics);
    *get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::SoftwareRendererThreads,
    )
    .expect("software thread row") = 2;
    state.submenu_transition = SubmenuTransition::FadeOutToMain;

    let effect = update(
        &mut state,
        SUBMENU_FADE_DURATION + 0.001,
        &asset_manager,
        &SmxAssignmentView::default(),
    );

    assert!(
        matches!(
            &effect,
            Some(ThemeEffect::Runtime(
                crate::SimplyLoveRuntimeRequest::Graphics(deadsync_theme::GraphicsRequest {
                    software_threads: Some(2),
                    ..
                })
            ))
        ),
        "unexpected effect: {effect:?}"
    );
}
