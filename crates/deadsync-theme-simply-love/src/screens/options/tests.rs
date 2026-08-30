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
    init_with_config_and_audio(config::Config::default(), audio_options)
}

fn init_with_config(config: config::Config) -> State {
    init_with_config_and_audio(config, AudioOptionsView::default())
}

fn init_with_config_and_audio(config: config::Config, audio_options: AudioOptionsView) -> State {
    super::init(OptionsInitView {
        config,
        judgment_palettes: deadsync_config::judgment_palettes::JudgmentPaletteCatalog::default(),
        updater_capabilities: SimplyLoveUpdaterCapabilities {
            app_update: true,
            ffmpeg_install: true,
        },
        app_paths: test_app_paths(),
        audio: audio_options,
        graphics: GraphicsOptionsView {
            software_thread_choices: vec![0, 1, 2],
            ..GraphicsOptionsView::default()
        },
        song_packs: Vec::new(),
        pack_sync: OptionsPackSyncView::default(),
        noteskins: NoteskinCatalogView {
            names: vec![profile_data::NoteSkin::DEFAULT_NAME.to_owned()],
        },
        machine_player_options: profile_data::PlayerOptionsData::default(),
        smx_assignment: deadsync_theme::views::SmxAssignmentView::default(),
        smx_gifs: deadsync_theme::views::SmxGifCatalogView::default(),
        score_import_profiles: Vec::new(),
        bookkeeping: crate::views::BookkeepingView::default(),
    })
}

#[test]
fn system_game_choice_tracks_the_saved_game() {
    let config = config::Config {
        game_flag: config::GameFlag::Pump,
        ..config::Config::default()
    };
    let state = init_with_config(config);
    let game_row = SYSTEM_OPTIONS_ROWS
        .iter()
        .position(|row| row.id == SubRowId::Game)
        .expect("system options must contain the game row");

    assert_eq!(SYSTEM_OPTIONS_ROWS[game_row].choices.len(), 2);
    assert_eq!(state.sub[SubmenuKind::System].choice_indices[game_row], 1);
}

#[test]
fn main_visible_items_match_updater_capabilities_and_are_stable() {
    let state = init();
    let first = visible_items(&state);
    let second = visible_items(&state);
    assert!(std::ptr::eq(first, second));
    assert!(first.iter().any(|item| item.id == ItemId::CheckForUpdates));
    assert!(first.iter().any(|item| item.id == ItemId::RollBackVersion));
    assert!(
        first
            .iter()
            .any(|item| item.id == ItemId::DownloadVideoSupport)
    );

    let unavailable = build_visible_items(SimplyLoveUpdaterCapabilities::default());
    assert!(unavailable.iter().all(|item| !matches!(
        item.id,
        ItemId::CheckForUpdates | ItemId::RollBackVersion | ItemId::DownloadVideoSupport
    )));
}

#[test]
fn restoring_main_selection_preserves_child_rows_clamps_and_does_not_arm_change_sfx() {
    for item_id in [ItemId::ManageLocalProfiles, ItemId::Credits] {
        let mut state = init();
        let selected = visible_items(&state)
            .iter()
            .position(|item| item.id == item_id)
            .expect("standalone child row should be visible");

        state.restore_main_selection(selected);

        assert_eq!(visible_items(&state)[state.selected].id, item_id);
        assert_eq!(state.prev_selected, selected);
    }

    let mut state = init();
    let last = visible_items(&state).len() - 1;
    state.restore_main_selection(usize::MAX);

    assert_eq!(state.selected, last);
    assert_eq!(state.prev_selected, last);
}

#[test]
fn main_row_navigation_uses_directional_simply_love_sounds() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.restore_main_selection(1);

    assert!(
        press(&mut state, &asset_manager, VirtualAction::p1_up).is_empty(),
        "row sounds are emitted by the frame update"
    );
    let mut effects = Vec::new();
    update(
        &mut state,
        0.0,
        &asset_manager,
        &SmxAssignmentView::default(),
        &mut effects,
    );
    assert!(matches!(
        effects.as_slice(),
        [ThemeEffect::Runtime(
            crate::SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(PREV_ROW_SFX))
        )]
    ));

    handle_input(
        &mut state,
        &asset_manager,
        &updater_view(),
        &input_event(VirtualAction::p1_up, false),
        &mut Vec::new(),
    );
    assert!(
        press(&mut state, &asset_manager, VirtualAction::p1_down).is_empty(),
        "row sounds are emitted by the frame update"
    );
    effects.clear();
    update(
        &mut state,
        0.0,
        &asset_manager,
        &SmxAssignmentView::default(),
        &mut effects,
    );
    assert!(matches!(
        effects.as_slice(),
        [ThemeEffect::Runtime(
            crate::SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(NEXT_ROW_SFX))
        )]
    ));
}

#[test]
fn main_row_hold_repeat_matches_screen_options_service_metrics() {
    let asset_manager = AssetManager::new();
    let mut state = init();

    assert_eq!(MAIN_NAV_REPEAT_DELAY, Duration::from_millis(250));
    assert_eq!(MAIN_NAV_REPEAT_INTERVAL, Duration::from_nanos(83_333_333));

    press(&mut state, &asset_manager, VirtualAction::p1_down);
    let after_press = state.selected;
    update(
        &mut state,
        0.249,
        &asset_manager,
        &SmxAssignmentView::default(),
        &mut Vec::new(),
    );
    assert_eq!(state.selected, after_press);

    update(
        &mut state,
        0.002,
        &asset_manager,
        &SmxAssignmentView::default(),
        &mut Vec::new(),
    );
    assert_eq!(state.selected, after_press + 1);
    let after_first_repeat = state.selected;

    update(
        &mut state,
        0.081,
        &asset_manager,
        &SmxAssignmentView::default(),
        &mut Vec::new(),
    );
    assert_eq!(state.selected, after_first_repeat);
    update(
        &mut state,
        0.003,
        &asset_manager,
        &SmxAssignmentView::default(),
        &mut Vec::new(),
    );
    assert_eq!(state.selected, after_first_repeat + 1);
}

#[test]
fn options_select_color_actors_keep_static_texture_sources() {
    let state = init();
    let asset_manager = AssetManager::new();
    let texture = selected_visual_assets(&state).select_color;
    let actors = get_actors(&state, &asset_manager, 1.0);
    let sources: Vec<_> = actors
        .iter()
        .filter_map(|actor| match actor {
            actors::Actor::Sprite { source, .. } if source.texture_key() == Some(texture) => {
                Some(source)
            }
            _ => None,
        })
        .collect();

    assert!(!sources.is_empty());
    assert!(
        sources
            .iter()
            .all(|source| matches!(source, actors::SpriteSource::TextureStatic(_)))
    );
}

#[test]
fn retained_confirm_text_and_direct_staging_match_legacy_frames() {
    let benchmark = OptionsOverlayHotBenchmark::new();
    assert_eq!(
        benchmark.legacy_prompt_frame(),
        benchmark.retained_prompt_frame()
    );

    let mut legacy = Vec::with_capacity(5);
    let mut direct = Vec::with_capacity(5);
    assert_eq!(
        benchmark.legacy_confirm_frame(&mut legacy),
        benchmark.direct_confirm_frame(&mut direct)
    );
    assert_eq!(format!("{legacy:?}"), format!("{direct:?}"));
}

#[test]
fn direct_options_modal_append_matches_legacy_batches() {
    crate::assets::i18n::init_for_tests();
    let benchmark = OptionsModalAppendBenchmark::new();

    let mut legacy = Vec::with_capacity(7);
    let mut direct = Vec::with_capacity(7);
    assert_eq!(
        benchmark.legacy_reload_frame(&mut legacy),
        benchmark.direct_reload_frame(&mut direct)
    );
    assert_eq!(legacy.len(), benchmark.reload_actor_count());
    let [Actor::SharedFrame { children, .. }] = direct.as_slice() else {
        panic!("retained reload initialization should use one shared frame");
    };
    assert_eq!(format!("{legacy:#?}"), format!("{children:#?}"));

    legacy = Vec::with_capacity(80);
    direct = Vec::with_capacity(80);
    assert_eq!(
        benchmark.legacy_download_frame(&mut legacy),
        benchmark.direct_download_frame(&mut direct)
    );
    assert_eq!(legacy.len(), benchmark.download_actor_count());
    let [Actor::SharedFrame { children, .. }] = direct.as_slice() else {
        panic!("retained pack browser should use one shared frame");
    };
    assert_eq!(format!("{legacy:#?}"), format!("{:#?}", children.as_ref()));

    legacy = Vec::with_capacity(96);
    direct = Vec::with_capacity(96);
    assert_eq!(
        benchmark.legacy_palette_frame(&mut legacy),
        benchmark.direct_palette_frame(&mut direct)
    );
    assert_eq!(legacy.len(), benchmark.palette_actor_count());
    let [Actor::SharedFrame { children, .. }] = direct.as_slice() else {
        panic!("retained palette browser should use one shared frame");
    };
    assert_eq!(format!("{legacy:#?}"), format!("{:#?}", children.as_ref()));
}

#[test]
fn retained_reload_initialization_reuses_and_tracks_events() {
    crate::assets::i18n::init_for_tests();
    let mut benchmark = OptionsModalAppendBenchmark::new();
    let mut retained = Vec::with_capacity(1);
    let old_checksum = benchmark.direct_reload_frame(&mut retained);
    let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
        panic!("retained reload initialization should use one shared frame");
    };
    let first = Arc::clone(children);

    let _ = benchmark.direct_reload_frame(&mut retained);
    let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
        panic!("stable reload initialization should remain shared");
    };
    assert!(Arc::ptr_eq(&first, children));

    benchmark.advance_reload_fixture();
    let new_checksum = benchmark.direct_reload_frame(&mut retained);
    let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
        panic!("changed reload initialization should rebuild a shared frame");
    };
    assert!(!Arc::ptr_eq(&first, children));
    assert_ne!(old_checksum, new_checksum);
}

#[test]
fn retained_pack_browser_reuses_and_tracks_caret_phase() {
    crate::assets::i18n::init_for_tests();
    let mut benchmark = OptionsModalAppendBenchmark::new();
    let mut retained = Vec::with_capacity(1);
    let _ = benchmark.direct_download_frame(&mut retained);
    let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
        panic!("retained pack browser should use one shared frame");
    };
    let first = Arc::clone(children);

    retained.clear();
    let _ = benchmark.direct_download_frame(&mut retained);
    let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
        panic!("stable pack browser should remain shared");
    };
    assert!(Arc::ptr_eq(&first, children));

    benchmark.advance_download_caret(0.5);
    retained.clear();
    let _ = benchmark.direct_download_frame(&mut retained);
    let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
        panic!("changed pack-browser caret should rebuild a shared frame");
    };
    assert!(!Arc::ptr_eq(&first, children));

    let mut immediate = Vec::with_capacity(80);
    let _ = benchmark.legacy_download_frame(&mut immediate);
    assert_eq!(
        format!("{immediate:#?}"),
        format!("{:#?}", children.as_ref())
    );
}

fn updater_view() -> SimplyLoveUpdaterView {
    SimplyLoveUpdaterView::default()
}

#[test]
fn updater_panels_replace_only_for_changed_slots() {
    let mut state = init();
    let mut updater = SimplyLoveUpdaterView {
        update: crate::views::SimplyLoveUpdatePhase::Checking,
        ffmpeg: crate::views::SimplyLoveFfmpegPhase::Checking,
    };
    sync_updater_panels(&mut state, &updater, true, true);
    assert!(state.update_panel.is_some());
    assert!(state.ffmpeg_panel.is_some());

    updater.update = crate::views::SimplyLoveUpdatePhase::Idle;
    updater.ffmpeg = crate::views::SimplyLoveFfmpegPhase::Idle;
    sync_updater_panels(&mut state, &updater, false, false);
    assert!(state.update_panel.is_some());
    assert!(state.ffmpeg_panel.is_some());

    sync_updater_panels(&mut state, &updater, true, false);
    assert!(state.update_panel.is_none());
    assert!(state.ffmpeg_panel.is_some());
    sync_updater_panels(&mut state, &updater, false, true);
    assert!(state.ffmpeg_panel.is_none());
}

#[test]
fn updater_panels_rebuild_for_a_new_locale_revision() {
    let mut state = init();
    let mut updater = SimplyLoveUpdaterView {
        update: crate::views::SimplyLoveUpdatePhase::Checking,
        ffmpeg: crate::views::SimplyLoveFfmpegPhase::Checking,
    };
    sync_updater_panels(&mut state, &updater, true, true);
    updater.update = crate::views::SimplyLoveUpdatePhase::Idle;
    updater.ffmpeg = crate::views::SimplyLoveFfmpegPhase::Idle;
    state.updater_i18n_revision = u64::MAX;
    sync_updater_panels(&mut state, &updater, false, false);
    assert!(state.update_panel.is_none());
    assert!(state.ffmpeg_panel.is_none());
}

#[test]
fn visual_assets_follow_local_machine_choices() {
    let mut state = init();
    let rows = submenu_rows(SubmenuKind::Machine);
    let style_row = rows
        .iter()
        .position(|row| row.id == SubRowId::VisualStyle)
        .expect("machine options should contain visual style");
    let variant_row = rows
        .iter()
        .position(|row| row.id == SubRowId::ThemeVariant)
        .expect("machine options should contain theme variant");
    state.sub[SubmenuKind::Machine].choice_indices[style_row] =
        visual_style_choice_index(config::VisualStyle::Srpg9);
    state.sub[SubmenuKind::Machine].choice_indices[variant_row] =
        srpg_variant_choice_index(config::SrpgVariant::Srpg10);

    let selected = selected_visual_assets(&state);
    let expected = visual_styles::for_style_and_variant(
        config::VisualStyle::Srpg9,
        config::SrpgVariant::Srpg10,
    );
    assert_eq!(
        selected.select_color,
        expected.select_color,
        "style_choice={} variant_choice={}",
        state.sub[SubmenuKind::Machine].choice_indices[style_row],
        state.sub[SubmenuKind::Machine].choice_indices[variant_row]
    );
}

#[test]
fn pack_sync_policy_comes_from_prepared_profile_and_current_options() {
    let mut state = init();
    state.pack_sync = OptionsPackSyncView {
        target_chart_type: "dance-double".to_owned(),
        preferred_difficulty_index: 4,
    };
    set_choice_by_id(
        &mut state.sub[SubmenuKind::InputBackend].choice_indices,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::MenuNavigation,
        1,
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::InputBackend].choice_indices,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::MenuButtons,
        1,
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::NullOrDieOptions].choice_indices,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::SyncConfidence,
        sync_confidence_choice_index(75),
    );

    let navigation = navigation_policy(&state);
    assert!(navigation.only_dedicated_menu_buttons);
    assert!(navigation.three_key_navigation);
    assert_eq!(confidence_percent(&state), 75);
    assert_eq!(state.pack_sync.target_chart_type, "dance-double");
    assert_eq!(state.pack_sync.preferred_difficulty_index, 4);
}

#[test]
fn smx_gif_choices_come_from_shell_catalog() {
    let state = super::init(OptionsInitView {
        config: config::Config::default(),
        judgment_palettes: deadsync_config::judgment_palettes::JudgmentPaletteCatalog::default(),
        updater_capabilities: SimplyLoveUpdaterCapabilities::default(),
        app_paths: test_app_paths(),
        audio: AudioOptionsView::default(),
        graphics: GraphicsOptionsView::default(),
        song_packs: Vec::new(),
        pack_sync: OptionsPackSyncView::default(),
        noteskins: NoteskinCatalogView::default(),
        machine_player_options: profile_data::PlayerOptionsData::default(),
        smx_assignment: SmxAssignmentView::default(),
        smx_gifs: SmxGifCatalogView {
            background_packs: vec!["Background Pack".to_owned()],
            judgment_packs: vec!["Judgment Pack".to_owned()],
        },
        score_import_profiles: Vec::new(),
        bookkeeping: crate::views::BookkeepingView::default(),
    });

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
fn system_choice_emits_shell_options_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::System);
    let row = select_visible_row(&mut state, SubmenuKind::System, SubRowId::LogFile);

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("system choice should emit shell config work");
    let enabled = state.sub[SubmenuKind::System].cursor_indices[row] == 1;

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Options(
                crate::SimplyLoveOptionsConfigRequest::LogToFile(value)
            )
        )) if value == enabled
    ));
}

#[test]
fn show_local_ip_choice_emits_machine_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Machine);
    let row = select_visible_row(&mut state, SubmenuKind::Machine, SubRowId::ShowLocalIp);

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("local IP choice should emit shell config work");
    let enabled = state.sub[SubmenuKind::Machine].cursor_indices[row] == 1;

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Machine(
                crate::SimplyLoveMachineConfigRequest::ShowLocalIp(value)
            )
        )) if value == enabled
    ));
}

#[test]
fn show_local_ip_sits_beside_version_overlay_settings() {
    let side_row = row_position(MACHINE_OPTIONS_ROWS, SubRowId::VersionOverlaySide).unwrap();
    let ip_row = row_position(MACHINE_OPTIONS_ROWS, SubRowId::ShowLocalIp).unwrap();
    assert_eq!(ip_row, side_row + 1);

    let side_item = MACHINE_OPTIONS_ITEMS
        .iter()
        .position(|item| item.id == ItemId::MchVersionOverlaySide)
        .unwrap();
    assert_eq!(
        MACHINE_OPTIONS_ITEMS[side_item + 1].id,
        ItemId::MchShowLocalIp
    );
}

#[test]
fn smx_numeric_choice_emits_shell_options_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::SmxConfig);
    select_visible_row(
        &mut state,
        SubmenuKind::SmxConfig,
        SubRowId::SmxDefaultLightBrightness,
    );
    let delta = if state.smx_default_light_brightness_pct < VOLUME_MAX_PERCENT {
        1
    } else {
        -1
    };

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, delta, NavWrap::Clamp)
        .expect("SMX numeric choice should emit shell config work");

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Options(
                crate::SimplyLoveOptionsConfigRequest::SmxDefaultLightBrightness(value)
            )
        )) if value == state.smx_default_light_brightness_pct as u8
    ));
}

#[test]
fn machine_noteskin_choice_emits_shell_profile_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.system_noteskin_choices.push("delta".to_owned());
    state.view = OptionsView::Submenu(SubmenuKind::System);
    select_visible_row(&mut state, SubmenuKind::System, SubRowId::DefaultNoteSkin);

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("machine noteskin should emit shell profile work");

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
            crate::SimplyLoveProfileRequest::SetMachineDefaultNoteskin(noteskin)
        )) if noteskin.as_str() == "delta"
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
fn difficulty_colors_choice_emits_zmod_scheme_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::SelectMusic);
    select_visible_row(
        &mut state,
        SubmenuKind::SelectMusic,
        SubRowId::DifficultyColors,
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("difficulty colors should emit shell config work");

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::SelectMusic(
                crate::SimplyLoveSelectMusicConfigRequest::DifficultyColors(
                    config::DifficultyColorScheme::Itg
                )
            )
        ))
    ));
}

#[test]
fn hide_inactive_series_is_visible_for_both_wheel_styles() {
    let mut state = init();
    let rows = submenu_rows(SubmenuKind::SelectMusic);
    let hide_row = rows
        .iter()
        .position(|row| row.id == SubRowId::HideInactiveSeries)
        .expect("Select Music options should contain Hide Inactive Series");

    for wheel_style in [0, 1] {
        set_choice_by_id(
            &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
            rows,
            SubRowId::MusicWheelStyle,
            wheel_style,
        );
        assert!(
            submenu_visible_row_indices(&state, SubmenuKind::SelectMusic, rows).contains(&hide_row)
        );
    }
}

#[test]
fn hide_inactive_series_choice_emits_shell_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::SelectMusic);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::SelectMusic,
        SubRowId::HideInactiveSeries,
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("Hide Inactive Series should emit shell config work");
    let enabled = state.sub[SubmenuKind::SelectMusic].cursor_indices[row] == 1;

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::SelectMusic(
                crate::SimplyLoveSelectMusicConfigRequest::HideInactiveSeries(value)
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
fn coin_options_initialize_and_follow_mode_locks() {
    let mut state = init_with_config(config::Config {
        coin: config::CoinOptions {
            mode: config::CoinMode::Pay,
            coins_per_credit: 3,
            songs_per_play: 5,
            event_mode: true,
            premium_free_minutes: 12,
            ..config::CoinOptions::default()
        },
        ..config::Config::default()
    });
    state.view = OptionsView::Submenu(SubmenuKind::Coin);

    assert_eq!(
        get_choice_by_id(
            &state.sub[SubmenuKind::Coin].choice_indices,
            COIN_OPTIONS_ROWS,
            SubRowId::CoinMode,
        ),
        Some(1)
    );
    assert_eq!(
        get_choice_by_id(
            &state.sub[SubmenuKind::Coin].choice_indices,
            COIN_OPTIONS_ROWS,
            SubRowId::CoinsPerCredit,
        ),
        Some(2)
    );
    assert!(is_submenu_row_disabled(
        &state,
        SubmenuKind::Coin,
        SubRowId::EventMode
    ));
    assert!(!is_submenu_row_disabled(
        &state,
        SubmenuKind::Coin,
        SubRowId::PremiumFree
    ));
}

#[test]
fn coin_choice_emits_typed_shell_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Coin);
    select_visible_row(&mut state, SubmenuKind::Coin, SubRowId::SongsPerPlay);

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("coin choice should emit shell config work");

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Coin(crate::SimplyLoveCoinConfigRequest::SongsPerPlay(
                4
            ))
        ))
    ));
}

#[test]
fn bookkeeping_rows_show_shell_counters_and_are_read_only() {
    let mut state = init();
    state.bookkeeping = crate::views::BookkeepingView {
        coins_inserted: 12,
        credits_spent: 8,
        plays_started: 5,
        stages_played: 17,
    };
    let row = row_position(BOOKKEEPING_ROWS, SubRowId::StagesPlayed)
        .expect("bookkeeping must contain the stages-played row");

    assert_eq!(
        row_choices(&state, SubmenuKind::Bookkeeping, BOOKKEEPING_ROWS, row)[0],
        "17"
    );
    assert!(is_submenu_row_disabled(
        &state,
        SubmenuKind::Bookkeeping,
        SubRowId::StagesPlayed
    ));
}

#[test]
fn advanced_choice_emits_shell_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Advanced);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::Advanced,
        SubRowId::AllowSongDeletion,
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("Advanced choice should emit shell config work");
    let enabled = state.sub[SubmenuKind::Advanced].cursor_indices[row] == 1;

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Advanced(
                crate::SimplyLoveAdvancedConfigRequest::AllowSongDeletion(value)
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
fn post_fail_pass_choice_follows_individual_autosubmit_visibility() {
    let mut state = init();
    let parent = row_position(COURSE_OPTIONS_ROWS, SubRowId::AutosubmitIndividual)
        .expect("course options should contain individual autosubmit");
    let child = row_position(COURSE_OPTIONS_ROWS, SubRowId::AutosubmitPostFailPasses)
        .expect("course options should contain post-fail passes");

    state.sub[SubmenuKind::Course].choice_indices[parent] = yes_no_choice_index(false);
    let hidden = submenu_visible_row_indices(&state, SubmenuKind::Course, COURSE_OPTIONS_ROWS);
    assert!(!hidden.contains(&child));

    state.sub[SubmenuKind::Course].choice_indices[parent] = yes_no_choice_index(true);
    let shown = submenu_visible_row_indices(&state, SubmenuKind::Course, COURSE_OPTIONS_ROWS);
    assert!(shown.contains(&child));
}

#[test]
fn post_fail_pass_choice_emits_course_config_request() {
    let asset_manager = AssetManager::new();
    let mut config = config::Config {
        autosubmit_course_scores_individually: true,
        ..config::Config::default()
    };
    config.autosubmit_course_post_fail_passes = false;
    let mut state = init_with_config(config);
    state.view = OptionsView::Submenu(SubmenuKind::Course);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::Course,
        SubRowId::AutosubmitPostFailPasses,
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("post-fail pass choice should emit shell config work");
    let enabled = state.sub[SubmenuKind::Course].cursor_indices[row] == 1;

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Course(
                crate::SimplyLoveCourseConfigRequest::AutosubmitPostFailPasses(value)
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
fn tournament_options_initialize_and_emit_typed_requests() {
    let asset_manager = AssetManager::new();
    let mut config = config::Config::default();
    config.tournament = config::TournamentModeOptions {
        enabled: true,
        scoring_system: config::TournamentScoringSystem::Itg,
        show_step_stats: false,
        enforce_no_cmod: true,
    };
    let mut state = init_with_config(config);
    state.view = OptionsView::Submenu(SubmenuKind::Tournament);

    for (id, expected) in [
        (SubRowId::TournamentMode, 1),
        (SubRowId::TournamentScoring, 1),
        (SubRowId::TournamentStepStats, 0),
        (SubRowId::TournamentEnforceNoCmod, 1),
    ] {
        let row = row_position(TOURNAMENT_OPTIONS_ROWS, id).expect("tournament row");
        assert_eq!(
            state.sub[SubmenuKind::Tournament].choice_indices[row],
            expected
        );
    }

    select_visible_row(
        &mut state,
        SubmenuKind::Tournament,
        SubRowId::TournamentScoring,
    );
    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("tournament scoring should emit shell config work");

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Tournament(
                crate::SimplyLoveTournamentConfigRequest::ScoringSystem(
                    config::TournamentScoringSystem::Ex
                )
            )
        ))
    ));
}

#[test]
fn note_scroll_clock_initializes_from_config_and_emits_typed_request() {
    let asset_manager = AssetManager::new();
    let mut view = OptionsInitView {
        config: config::Config {
            note_scroll_clock: config::NoteScrollClock::FrameStable,
            ..config::Config::default()
        },
        judgment_palettes: deadsync_config::judgment_palettes::JudgmentPaletteCatalog::default(),
        updater_capabilities: SimplyLoveUpdaterCapabilities::default(),
        app_paths: test_app_paths(),
        audio: AudioOptionsView::default(),
        graphics: GraphicsOptionsView::default(),
        song_packs: Vec::new(),
        pack_sync: OptionsPackSyncView::default(),
        noteskins: NoteskinCatalogView::default(),
        machine_player_options: profile_data::PlayerOptionsData::default(),
        smx_assignment: SmxAssignmentView::default(),
        smx_gifs: SmxGifCatalogView::default(),
        score_import_profiles: Vec::new(),
        bookkeeping: crate::views::BookkeepingView::default(),
    };
    let mut state = super::init(view.clone());
    state.view = OptionsView::Submenu(SubmenuKind::Gameplay);
    let row = select_visible_row(&mut state, SubmenuKind::Gameplay, SubRowId::NoteScrollClock);

    assert_eq!(
        state.sub[SubmenuKind::Gameplay].cursor_indices[row],
        config::NoteScrollClock::FrameStable.choice_index()
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("note scroll clock should emit shell config work");

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Gameplay(
                crate::SimplyLoveGameplayConfigRequest::NoteScrollClock(
                    config::NoteScrollClock::RawAudio
                )
            )
        ))
    ));

    view.config.note_scroll_clock = config::NoteScrollClock::RawAudio;
    let raw_state = super::init(view);
    let raw_row = row_position(GAMEPLAY_OPTIONS_ROWS, SubRowId::NoteScrollClock)
        .expect("note scroll clock row");
    assert_eq!(
        raw_state.sub[SubmenuKind::Gameplay].cursor_indices[raw_row],
        config::NoteScrollClock::RawAudio.choice_index()
    );
}

#[test]
fn judgment_palette_default_row_uses_catalog_and_emits_full_catalog_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    let custom_id = state
        .judgment_palettes
        .create_palette(
            "Warm",
            deadsync_config::judgment_palettes::SIMPLY_LOVE_PALETTE_ID,
        )
        .unwrap();
    state.view = OptionsView::Submenu(SubmenuKind::Gameplay);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::Gameplay,
        SubRowId::DefaultJudgmentPalette,
    );

    assert_eq!(
        row_choices(&state, SubmenuKind::Gameplay, GAMEPLAY_OPTIONS_ROWS, row)
            .iter()
            .map(std::convert::AsRef::as_ref)
            .collect::<Vec<_>>(),
        ["Simply Love", "Warm"]
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("palette choice should emit persistence work");
    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::JudgmentPalettes(catalog))
            if catalog.default_palette_id == custom_id
    ));
}

#[test]
fn judgment_palette_presentation_reuses_stable_browser_and_rebuilds_on_selection() {
    let mut state = init();
    show_judgment_palette_overlay(&mut state);

    let mut first = Vec::with_capacity(1);
    assert!(push_judgment_palette_overlay(
        &mut first,
        &state,
        2,
        crate::config::MachineFont::Mega,
    ));
    let Actor::SharedFrame {
        children: first_children,
        ..
    } = &first[0]
    else {
        panic!("palette browser should render one retained tree");
    };
    let first_children = Arc::clone(first_children);

    assert!(update_judgment_palette_overlay(&mut state, 0.2).is_some());
    let mut blink = Vec::with_capacity(1);
    assert!(push_judgment_palette_overlay(
        &mut blink,
        &state,
        2,
        crate::config::MachineFont::Mega,
    ));
    let Actor::SharedFrame {
        children: blink_children,
        ..
    } = &blink[0]
    else {
        panic!("palette browser should remain retained");
    };
    assert!(Arc::ptr_eq(&first_children, blink_children));

    assert!(
        handle_judgment_palette_input(&mut state, &input_event(VirtualAction::p1_down, true),)
            .is_some()
    );
    let mut changed = Vec::with_capacity(1);
    assert!(push_judgment_palette_overlay(
        &mut changed,
        &state,
        2,
        crate::config::MachineFont::Mega,
    ));
    let Actor::SharedFrame {
        children: changed_children,
        ..
    } = &changed[0]
    else {
        panic!("palette browser should render one retained tree");
    };
    assert!(!Arc::ptr_eq(&first_children, changed_children));

    let mut immediate = Vec::with_capacity(96);
    push_judgment_palette_overlay_unreserved(
        &mut immediate,
        &state,
        2,
        crate::config::MachineFont::Mega,
    );
    assert_eq!(format!("{changed_children:#?}"), format!("{immediate:#?}"));
}

#[test]
fn judgment_palette_overlay_copies_builtin_and_edits_rgb_from_pad_input() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Gameplay);
    select_visible_row(
        &mut state,
        SubmenuKind::Gameplay,
        SubRowId::ManageJudgmentPalettes,
    );

    press(&mut state, &asset_manager, VirtualAction::p1_start);
    assert!(judgment_palette_overlay_visible(
        &state.judgment_palette_overlay
    ));

    let create_effects = press(&mut state, &asset_manager, VirtualAction::p1_start);
    assert_eq!(state.judgment_palettes.palettes.len(), 2);
    assert!(matches!(
        state.judgment_palette_overlay,
        JudgmentPaletteOverlayState::Editor { .. }
    ));
    assert!(create_effects.iter().any(|effect| matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::JudgmentPalettes(_))
    )));

    press(&mut state, &asset_manager, VirtualAction::p1_down);
    press(&mut state, &asset_manager, VirtualAction::p1_start);
    let before = state.judgment_palettes.palettes[1]
        .palette
        .color(deadlib_present::color::JudgmentColorRole::FantasticBlue);
    let edit_effects = press(&mut state, &asset_manager, VirtualAction::p1_right);
    let after = state.judgment_palettes.palettes[1]
        .palette
        .color(deadlib_present::color::JudgmentColorRole::FantasticBlue);

    let before_red = (before[0] * 255.0).round() as u8;
    let after_red = (after[0] * 255.0).round() as u8;
    assert_eq!(after_red, before_red + 1);
    assert!(edit_effects.iter().any(|effect| matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::JudgmentPalettes(_))
    )));

    let mut repeat_effects = Vec::new();
    update(
        &mut state,
        (NAV_INITIAL_HOLD_DELAY + Duration::from_millis(1)).as_secs_f32(),
        &asset_manager,
        &SmxAssignmentView::default(),
        &mut repeat_effects,
    );
    let repeated = state.judgment_palettes.palettes[1]
        .palette
        .color(deadlib_present::color::JudgmentColorRole::FantasticBlue);
    assert_eq!((repeated[0] * 255.0).round() as u8, after_red + 1);
    assert!(repeat_effects.iter().any(|effect| matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::JudgmentPalettes(_))
    )));

    handle_input(
        &mut state,
        &asset_manager,
        &updater_view(),
        &input_event(VirtualAction::p1_right, false),
        &mut Vec::new(),
    );
    update(
        &mut state,
        NAV_REPEAT_SCROLL_INTERVAL.as_secs_f32(),
        &asset_manager,
        &SmxAssignmentView::default(),
        &mut Vec::new(),
    );
    let released = state.judgment_palettes.palettes[1]
        .palette
        .color(deadlib_present::color::JudgmentColorRole::FantasticBlue);
    assert_eq!((released[0] * 255.0).round() as u8, after_red + 1);
}

#[test]
fn gameplay_banner_choice_emits_playback_mode_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Gameplay);
    let row = select_visible_row(&mut state, SubmenuKind::Gameplay, SubRowId::AnimatedBanners);

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("Gameplay banner choice should emit shell config work");
    let mode = match state.sub[SubmenuKind::Gameplay].cursor_indices[row] {
        0 => config::GameplayBannerMode::Static,
        1 => config::GameplayBannerMode::Once,
        _ => config::GameplayBannerMode::Loop,
    };

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Gameplay(
                crate::SimplyLoveGameplayConfigRequest::BannerMode(value)
            )
        )) if value == mode
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
fn null_or_die_orientation_emits_shell_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::NullOrDieOptions);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::NullOrDieOptions,
        SubRowId::GraphOrientation,
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("Null-or-Die graph orientation should emit shell config work");
    let expected = if state.sub[SubmenuKind::NullOrDieOptions].cursor_indices[row] == 1 {
        crate::SimplyLoveGraphOrientation::Horizontal
    } else {
        crate::SimplyLoveGraphOrientation::Vertical
    };

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::NullOrDie(
                crate::SimplyLoveNullOrDieConfigRequest::GraphOrientation(value)
            )
        )) if value == expected
    ));
}

#[test]
fn machine_scroll_speed_choice_emits_shell_profile_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::System);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::System,
        SubRowId::DefaultScrollSpeed,
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("machine scroll speed should emit shell profile work");
    let expected =
        state.system_scroll_speed_values[state.sub[SubmenuKind::System].cursor_indices[row]];

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
            crate::SimplyLoveProfileRequest::SetMachineDefaultScrollSpeed(setting)
        )) if setting == expected
    ));
}

#[test]
fn machine_scroll_direction_choice_emits_shell_profile_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::System);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::System,
        SubRowId::DefaultScrollDirection,
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("machine scroll direction should emit shell profile work");
    let expected =
        state.system_scroll_direction_values[state.sub[SubmenuKind::System].cursor_indices[row]];

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
            crate::SimplyLoveProfileRequest::SetMachineDefaultScroll(setting)
        )) if setting == expected
    ));
}

#[test]
fn machine_background_filter_choice_emits_shell_profile_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::System);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::System,
        SubRowId::DefaultBackgroundFilter,
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("machine background filter should emit shell profile work");
    let expected =
        state.system_background_filter_values[state.sub[SubmenuKind::System].cursor_indices[row]];

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
            crate::SimplyLoveProfileRequest::SetMachineDefaultBackgroundFilter(setting)
        )) if setting == expected
    ));
}

#[test]
fn null_or_die_origin_is_visible_only_for_vertical_graphs() {
    let mut state = init();
    let origin_row = row_position(NULL_OR_DIE_OPTIONS_ROWS, SubRowId::GraphOrigin)
        .expect("Null-or-Die options should contain graph origin");
    let visible = |state: &State| {
        submenu_visible_row_indices(
            state,
            SubmenuKind::NullOrDieOptions,
            NULL_OR_DIE_OPTIONS_ROWS,
        )
    };

    assert!(visible(&state).contains(&origin_row));

    set_choice_by_id(
        &mut state.sub[SubmenuKind::NullOrDieOptions].choice_indices,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::GraphOrientation,
        1,
    );
    assert!(!visible(&state).contains(&origin_row));

    set_choice_by_id(
        &mut state.sub[SubmenuKind::NullOrDieOptions].choice_indices,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::GraphOrientation,
        0,
    );
    assert!(visible(&state).contains(&origin_row));
}

#[test]
fn null_or_die_origin_defaults_to_bottom_and_emits_shell_config_request() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::NullOrDieOptions);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::NullOrDieOptions,
        SubRowId::GraphOrigin,
    );
    assert_eq!(
        state.sub[SubmenuKind::NullOrDieOptions].cursor_indices[row],
        0
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("Null-or-Die graph origin should emit shell config work");

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::NullOrDie(
                crate::SimplyLoveNullOrDieConfigRequest::GraphOrigin(
                    crate::SimplyLoveGraphOrigin::Top
                )
            )
        ))
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
    let mut effects = Vec::with_capacity(8);
    append_pending_effects(&mut state, effect, &mut effects);

    assert_eq!(effects.capacity(), 8);
    assert_eq!(effects.len(), 3);
    assert!(is_change_value_sfx(&effects[0]));
    assert!(matches!(
        &effects[1],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Online(
                crate::SimplyLoveOnlineConfigRequest::EnableGrooveStats(value)
            )
        )) if *value == enabled
    ));
    assert!(matches!(
        &effects[2],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Online(
            crate::SimplyLoveOnlineRequest::Reinitialize
        ))
    ));
    assert!(!state.online_reinit_pending);
}

#[test]
fn arrowcloud_enable_persists_before_reinitializing_services() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::ArrowCloud);
    let row = select_visible_row(
        &mut state,
        SubmenuKind::ArrowCloud,
        SubRowId::EnableArrowCloud,
    );

    let effect = apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap)
        .expect("ArrowCloud enable choice should emit shell work");
    let enabled = state.sub[SubmenuKind::ArrowCloud].cursor_indices[row] == 1;
    let mut effects = Vec::with_capacity(8);
    append_pending_effects(&mut state, effect, &mut effects);

    assert_eq!(effects.capacity(), 8);
    assert_eq!(effects.len(), 3);
    assert!(is_change_value_sfx(&effects[0]));
    assert!(matches!(
        &effects[1],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::Online(
                crate::SimplyLoveOnlineConfigRequest::EnableArrowCloud(value)
            )
        )) if *value == enabled
    ));
    assert!(matches!(
        &effects[2],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Online(
            crate::SimplyLoveOnlineRequest::Reinitialize
        ))
    ));
    assert!(!state.online_reinit_pending);
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

fn press(
    state: &mut State,
    asset_manager: &AssetManager,
    action: VirtualAction,
) -> Vec<ThemeEffect> {
    let mut effects = Vec::new();
    handle_input(
        state,
        asset_manager,
        &updater_view(),
        &input_event(action, true),
        &mut effects,
    );
    effects
}

fn apply_choice_effects(
    state: &mut State,
    asset_manager: &AssetManager,
    delta: isize,
    wrap: NavWrap,
) -> Vec<ThemeEffect> {
    let effect = apply_submenu_choice_delta(state, asset_manager, delta, wrap)
        .expect("choice should change");
    let mut effects = Vec::new();
    append_pending_effects(state, effect, &mut effects);
    effects
}

fn is_change_value_sfx(effect: &ThemeEffect) -> bool {
    matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(path)))
            if *path == "assets/sounds/change_value.ogg"
    )
}

fn dedicated_press(
    state: &mut State,
    asset_manager: &AssetManager,
    action: VirtualAction,
) -> ThemeEffect {
    handle_dedicated_three_key_options_input(state, asset_manager, &input_event(action, true))
}

fn dedicated_three_key_arcade_state() -> State {
    init_with_config(config::Config {
        three_key_navigation: true,
        only_dedicated_menu_buttons: true,
        arcade_options_navigation: true,
        ..config::Config::default()
    })
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

    let effects = apply_choice_effects(&mut state, &asset_manager, 1, NavWrap::Clamp);

    assert_eq!(state.audio_options.output_device, Some(0));
    assert_eq!(state.audio_options.sample_rate_hz, None);
    assert_eq!(sample_rate_choice_index(&state, None), 0);
    assert_eq!(effects.len(), 3);
    assert!(is_change_value_sfx(&effects[0]));
    assert!(matches!(
        &effects[1],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            AudioRequest::SetOutputDevice(Some(0))
        ))
    ));
    assert!(matches!(
        &effects[2],
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
    let pitch = apply_choice_effects(&mut state, &asset_manager, 1, NavWrap::Clamp);
    assert_eq!(pitch.len(), 2);
    assert!(is_change_value_sfx(&pitch[0]));
    assert!(matches!(
        &pitch[1],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            AudioRequest::SetPreservePitch(true)
        ))
    ));
    assert!(state.audio_options.preserve_pitch);

    select_visible_row(&mut state, SubmenuKind::Sound, SubRowId::ReplayGain);
    let replay_gain = apply_choice_effects(&mut state, &asset_manager, 1, NavWrap::Clamp);
    assert_eq!(replay_gain.len(), 2);
    assert!(is_change_value_sfx(&replay_gain[0]));
    assert!(matches!(
        &replay_gain[1],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            AudioRequest::SetReplayGain(true)
        ))
    ));
    assert!(state.audio_options.replay_gain);

    set_sound_choice_index(&mut state, SubRowId::MineSounds, 0);
    select_visible_row(&mut state, SubmenuKind::Sound, SubRowId::MineSounds);
    let mine_sound = apply_choice_effects(&mut state, &asset_manager, 1, NavWrap::Clamp);
    assert_eq!(mine_sound.len(), 2);
    assert!(is_change_value_sfx(&mine_sound[0]));
    assert!(matches!(
        &mine_sound[1],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            AudioRequest::SetMineHitSound(true)
        ))
    ));

    state.global_offset_ms = 0;
    select_visible_row(&mut state, SubmenuKind::Sound, SubRowId::GlobalOffset);
    let global_offset = apply_choice_effects(&mut state, &asset_manager, 1, NavWrap::Clamp);
    assert!(matches!(
        global_offset.as_slice(),
        [
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                AudioRequest::PlaySfx(path)
            )),
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                AudioRequest::SetGlobalOffsetMillis(1)
            ))
        ] if *path == "assets/sounds/change_value.ogg"
    ));
    assert_eq!(state.global_offset_ms, 1);
}

#[test]
fn apply_replaygain_item_matches_row() {
    let row_idx =
        row_position(SOUND_OPTIONS_ROWS, SubRowId::ApplyReplayGain).expect("apply row exists");
    assert_eq!(SOUND_OPTIONS_ITEMS[row_idx].id, ItemId::SndApplyReplayGain);
    // The action row directly follows the ReplayGain toggle.
    let toggle_idx =
        row_position(SOUND_OPTIONS_ROWS, SubRowId::ReplayGain).expect("toggle row exists");
    assert_eq!(row_idx, toggle_idx + 1);
}

#[test]
fn apply_replaygain_row_visible_only_when_replaygain_on() {
    let mut state = init_with_audio(AudioOptionsView {
        replay_gain: false,
        ..AudioOptionsView::default()
    });
    state.view = OptionsView::Submenu(SubmenuKind::Sound);

    let actual =
        row_position(SOUND_OPTIONS_ROWS, SubRowId::ApplyReplayGain).expect("apply row exists");
    let hidden = submenu_visible_row_indices(&state, SubmenuKind::Sound, SOUND_OPTIONS_ROWS);
    assert!(
        !hidden.contains(&actual),
        "apply row must be hidden while ReplayGain is Off"
    );

    set_sound_choice_index(&mut state, SubRowId::ReplayGain, 1);
    let shown = submenu_visible_row_indices(&state, SubmenuKind::Sound, SOUND_OPTIONS_ROWS);
    assert!(
        shown.contains(&actual),
        "apply row must be visible while ReplayGain is On"
    );
}

#[test]
fn apply_replaygain_start_opens_overlay_and_requests_analysis() {
    let asset_manager = AssetManager::new();
    let mut state = init_with_audio(AudioOptionsView {
        replay_gain: true,
        ..AudioOptionsView::default()
    });
    state.view = OptionsView::Submenu(SubmenuKind::Sound);
    select_visible_row(&mut state, SubmenuKind::Sound, SubRowId::ApplyReplayGain);

    let effect = press(&mut state, &asset_manager, VirtualAction::p1_start);

    assert!(
        state.apply_replaygain_ui.is_some(),
        "starting the action should open the progress overlay"
    );
    assert!(
        effect.iter().any(|effect| effect_contains_content(
            effect,
            crate::SimplyLoveContentRequest::ApplyReplayGain
        )),
        "start should request bulk ReplayGain analysis"
    );
}

#[test]
fn apply_replaygain_cancel_requests_worker_stop() {
    let asset_manager = AssetManager::new();
    let mut state = init_with_audio(AudioOptionsView {
        replay_gain: true,
        ..AudioOptionsView::default()
    });
    state.view = OptionsView::Submenu(SubmenuKind::Sound);
    state.apply_replaygain_ui = Some(ApplyReplayGainUiState::new());
    apply_apply_replaygain_event(
        &mut state,
        crate::views::SimplyLoveApplyReplayGainEvent::Started { total: 10 },
    );

    let effect = press(&mut state, &asset_manager, VirtualAction::p1_back);

    assert!(
        effect.iter().any(|effect| effect_contains_content(
            effect,
            crate::SimplyLoveContentRequest::SkipReplayGain
        )),
        "back should request cooperative skip of the analysis pass"
    );
    assert!(
        state
            .apply_replaygain_ui
            .as_ref()
            .is_some_and(|ui| ui.cancel_requested),
        "cancel should be marked pending, keeping the overlay up"
    );
}

#[test]
fn apply_replaygain_finish_then_dismiss_closes_overlay() {
    let asset_manager = AssetManager::new();
    let mut state = init_with_audio(AudioOptionsView {
        replay_gain: true,
        ..AudioOptionsView::default()
    });
    state.view = OptionsView::Submenu(SubmenuKind::Sound);
    state.apply_replaygain_ui = Some(ApplyReplayGainUiState::new());
    apply_apply_replaygain_event(
        &mut state,
        crate::views::SimplyLoveApplyReplayGainEvent::Finished {
            done: 10,
            total: 10,
            cancelled: false,
        },
    );

    // While finished, pressing Start dismisses the overlay.
    let _ = press(&mut state, &asset_manager, VirtualAction::p1_start);
    assert!(
        state.apply_replaygain_ui.is_none(),
        "pressing Start after completion should dismiss the overlay"
    );
}

/// Returns true when `effect` is, or contains within a batch, a runtime content
/// request equal to `want`.
fn effect_contains_content(effect: &ThemeEffect, want: crate::SimplyLoveContentRequest) -> bool {
    fn is_match(effect: &ThemeEffect, want: &crate::SimplyLoveContentRequest) -> bool {
        match effect {
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Content(req)) => {
                std::mem::discriminant(req) == std::mem::discriminant(want)
            }
            ThemeEffect::Batch(effects) => effects.iter().any(|e| is_match(e, want)),
            _ => false,
        }
    }
    is_match(effect, &want)
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
    let idx = display_aspect_choice_index(4.0 / 3.0);
    assert!(matches!(
        DISPLAY_ASPECT_RATIO_CHOICES[idx],
        Choice::Literal("4:3")
    ));
}

#[test]
fn sync_display_resolution_selects_loaded_4_3_mode() {
    let mut state = init();
    sync_display_aspect_ratio(&mut state, 4.0 / 3.0);
    sync_display_resolution(&mut state, 1024, 768);

    assert_eq!(selected_aspect_label(&state), "4:3");
    assert_eq!(selected_resolution(&state), (1024, 768));
    assert!(state.resolution_choices.contains(&(1024, 768)));
}

#[test]
fn non_square_pixel_mode_survives_aspect_change() {
    let mut state = init();
    sync_display_resolution(&mut state, 3840, 780);
    sync_display_aspect_ratio(&mut state, 4.0 / 3.0);
    rebuild_resolution_choices(&mut state, 3840, 780);

    assert_eq!(selected_aspect_ratio(&state), 4.0 / 3.0);
    assert_eq!(selected_resolution(&state), (3840, 780));
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
fn max_fps_seed_uses_triple_monitor_refresh() {
    let mut state = init();
    state.refresh_rate_choices = vec![60_000, 144_000];

    set_choice_by_id(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::RefreshRate,
        0,
    );
    assert_eq!(max_fps_seed_value(&state, 0), 180);

    set_choice_by_id(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::RefreshRate,
        1,
    );
    assert_eq!(max_fps_seed_value(&state, 0), 432);
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
fn system_items_match_rows() {
    let expected = [
        (SubRowId::Game, ItemId::SysGame),
        (SubRowId::Theme, ItemId::SysTheme),
        (SubRowId::Language, ItemId::SysLanguage),
        (SubRowId::LogLevel, ItemId::SysLogLevel),
        (SubRowId::LogFile, ItemId::SysLogFile),
        (SubRowId::DefaultScrollSpeed, ItemId::SysDefaultScrollSpeed),
        (
            SubRowId::DefaultScrollDirection,
            ItemId::SysDefaultScrollDirection,
        ),
        (
            SubRowId::DefaultBackgroundFilter,
            ItemId::SysDefaultBackgroundFilter,
        ),
        (SubRowId::DefaultNoteSkin, ItemId::SysDefaultNoteSkin),
    ];

    assert_eq!(SYSTEM_OPTIONS_ROWS.len() + 1, SYSTEM_OPTIONS_ITEMS.len());
    for (idx, (row_id, item_id)) in expected.into_iter().enumerate() {
        assert_eq!(SYSTEM_OPTIONS_ROWS[idx].id, row_id);
        assert_eq!(SYSTEM_OPTIONS_ITEMS[idx].id, item_id);
    }
    assert_eq!(SYSTEM_OPTIONS_ITEMS.last().unwrap().id, ItemId::Exit);
}

#[test]
fn system_default_choices_keep_advanced_ini_values_visible() {
    let speed = deadsync_rules::scroll::ScrollSpeedSetting::MMod(433.0);
    let direction = profile_data::ScrollOption::Reverse.union(profile_data::ScrollOption::Centered);
    let filter = profile_data::BackgroundFilter::from_percent(93);

    assert!(system_scroll_speed_values(speed).contains(&speed));
    assert!(system_scroll_direction_values(direction).contains(&direction));
    assert!(system_background_filter_values(filter).contains(&filter));
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
        (SubRowId::SmxIdleLights, ItemId::InpSmxIdleLights),
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
fn graphics_presentation_modes_share_one_row() {
    let state = init();
    let row_idx = row_position(GRAPHICS_OPTIONS_ROWS, SubRowId::PresentMode)
        .expect("presentation mode row should exist");
    let row = &GRAPHICS_OPTIONS_ROWS[row_idx];

    assert_eq!(row.choices.len(), 3);
    assert_eq!(GRAPHICS_OPTIONS_ITEMS[row_idx].id, ItemId::GfxPresentMode);
    assert_eq!(
        selected_present_config(&state),
        (false, PresentPolicyChoice::Mailbox)
    );
    assert_eq!(
        present_mode_choice_index(true, PresentPolicyChoice::Immediate),
        0
    );
    assert_eq!(
        present_config_from_choice(0, PresentPolicyChoice::Immediate),
        (true, PresentPolicyChoice::Immediate)
    );
    assert_eq!(
        present_config_from_choice(1, PresentPolicyChoice::Immediate),
        (false, PresentPolicyChoice::Mailbox)
    );
    assert_eq!(
        present_config_from_choice(2, PresentPolicyChoice::Mailbox),
        (false, PresentPolicyChoice::Immediate)
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
        &mut Vec::new(),
    );
    update(
        &mut state,
        1.0,
        &asset_manager,
        &SmxAssignmentView::default(),
        &mut Vec::new(),
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

    let effects = apply_choice_effects(&mut state, &asset_manager, 1, NavWrap::Wrap);
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
        )) if *path == "assets/sounds/change_value.ogg"
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
        &mut Vec::new(),
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
        &mut Vec::new(),
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
        &mut Vec::new(),
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
fn service_child_three_key_select_moves_up_one_row() {
    let asset_manager = AssetManager::new();
    let mut state = dedicated_three_key_arcade_state();
    state.view = OptionsView::Submenu(SubmenuKind::Graphics);
    state.sub_selected = 2;

    press(&mut state, &asset_manager, VirtualAction::p1_select);

    assert_eq!(state.sub_selected, 1);
    assert_eq!(state.nav_key_held_direction, Some(NavDirection::Up));
    handle_input(
        &mut state,
        &asset_manager,
        &updater_view(),
        &input_event(VirtualAction::p1_select, false),
        &mut Vec::new(),
    );
    assert_eq!(state.nav_key_held_direction, None);
}

#[test]
fn input_launcher_three_key_start_opens_test_input() {
    let asset_manager = AssetManager::new();
    let mut state = dedicated_three_key_arcade_state();
    state.view = OptionsView::Submenu(SubmenuKind::Input);
    select_visible_row(&mut state, SubmenuKind::Input, SubRowId::TestInput);

    let effects = press(&mut state, &asset_manager, VirtualAction::p1_start);

    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ThemeEffect::Navigate(Screen::Input)))
    );
}

#[test]
fn service_child_three_key_start_opens_test_lights() {
    let asset_manager = AssetManager::new();
    let mut state = dedicated_three_key_arcade_state();
    state.view = OptionsView::Submenu(SubmenuKind::Lights);
    select_visible_row(&mut state, SubmenuKind::Lights, SubRowId::TestLights);

    let effects = press(&mut state, &asset_manager, VirtualAction::p1_start);

    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ThemeEffect::Navigate(Screen::TestLights)))
    );
}

#[test]
fn service_child_three_key_start_opens_overscan_adjustment() {
    let asset_manager = AssetManager::new();
    let mut state = dedicated_three_key_arcade_state();
    state.view = OptionsView::Submenu(SubmenuKind::Graphics);
    select_visible_row(
        &mut state,
        SubmenuKind::Graphics,
        SubRowId::OverscanAdjustment,
    );

    let effects = press(&mut state, &asset_manager, VirtualAction::p1_start);

    assert!(matches!(
        effects.as_slice(),
        [
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                AudioRequest::PlaySfx("assets/sounds/start.ogg")
            )),
            ThemeEffect::Navigate(Screen::OverscanAdjustment)
        ]
    ));
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
fn online_scoring_three_key_menu_lr_moves_rows() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::OnlineScoring);
    set_choice_by_id(
        &mut state.sub[SubmenuKind::InputBackend].choice_indices,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::MenuNavigation,
        1,
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::InputBackend].choice_indices,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::MenuButtons,
        1,
    );

    assert_eq!(state.sub_selected, 0);
    press(&mut state, &asset_manager, VirtualAction::p1_menu_right);
    assert_eq!(state.sub_selected, 1);
    assert_eq!(state.nav_key_held_direction, Some(NavDirection::Down));
    handle_input(
        &mut state,
        &asset_manager,
        &updater_view(),
        &input_event(VirtualAction::p1_menu_right, false),
        &mut Vec::new(),
    );
    assert_eq!(state.nav_key_held_direction, None);

    press(&mut state, &asset_manager, VirtualAction::p1_menu_left);
    assert_eq!(state.sub_selected, 0);
    press(&mut state, &asset_manager, VirtualAction::p2_menu_left);
    assert_eq!(state.sub_selected, ONLINE_SCORING_OPTIONS_ROWS.len());
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
fn machine_visibility_cache_refreshes_after_choice_change() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Machine);
    let row_idx = select_visible_row(&mut state, SubmenuKind::Machine, SubRowId::SelectColor);
    state.sub[SubmenuKind::Machine].choice_indices[row_idx] = yes_no_choice_index(true);
    state.sub[SubmenuKind::Machine].cursor_indices[row_idx] = yes_no_choice_index(true);
    clear_render_cache(&state);

    let before = submenu_total_rows(&state, SubmenuKind::Machine);
    apply_submenu_choice_delta(&mut state, &asset_manager, 1, NavWrap::Wrap);
    let after = submenu_total_rows(&state, SubmenuKind::Machine);

    assert_eq!(after, before + 1);
    let preferred_color = row_position(MACHINE_OPTIONS_ROWS, SubRowId::PreferredColor)
        .expect("machine options should contain preferred color");
    assert!(cached_submenu_visible_rows(&state, SubmenuKind::Machine).contains(&preferred_color));
}

#[test]
fn row_tween_destinations_retarget_only_when_layout_changes() {
    let mut row_tweens = Vec::new();
    let mut key = None;
    update_row_tweens(&mut row_tweens, &mut key, 12, 0, 1.0, 20.0, 0.016);
    let initial_targets: Vec<_> = row_tweens
        .iter()
        .map(|tween| (tween.to_y, tween.to_a))
        .collect();

    update_row_tweens(&mut row_tweens, &mut key, 12, 0, 1.0, 20.0, 0.016);
    assert_eq!(
        row_tweens
            .iter()
            .map(|tween| (tween.to_y, tween.to_a))
            .collect::<Vec<_>>(),
        initial_targets
    );

    update_row_tweens(&mut row_tweens, &mut key, 12, 8, 1.0, 20.0, 0.016);
    assert_ne!(
        row_tweens
            .iter()
            .map(|tween| (tween.to_y, tween.to_a))
            .collect::<Vec<_>>(),
        initial_targets
    );
    assert!(row_tweens.iter().any(|tween| tween.t < 1.0));
}

#[test]
fn row_window_and_edge_tweens_match_screen_options_position_rows() {
    assert_eq!(ROW_TWEEN_SECONDS, 0.2);
    assert_eq!(scroll_offset(5, 12), 0);
    assert_eq!(scroll_offset(6, 12), 1);
    assert_eq!(scroll_offset(11, 12), 2);

    let mut row_tweens = Vec::new();
    let mut key = None;
    update_row_tweens(&mut row_tweens, &mut key, 12, 5, 1.0, 20.0, 0.0);
    let first_visible_y = row_tweens[0].y();
    let bottom_hidden_y = row_tweens[10].y();
    assert_eq!((row_tweens[0].a(), row_tweens[10].a()), (1.0, 0.0));

    update_row_tweens(&mut row_tweens, &mut key, 12, 6, 1.0, 20.0, 0.0);
    let row_step = ROW_H + ROW_GAP;
    assert_eq!(
        row_tweens[0].to_y,
        0.5f32.mul_add(-row_step, first_visible_y)
    );
    assert_eq!(
        row_tweens[10].to_y,
        0.5f32.mul_add(-row_step, bottom_hidden_y)
    );
    assert_eq!((row_tweens[0].from_a, row_tweens[0].to_a), (1.0, 0.0));
    assert_eq!((row_tweens[10].from_a, row_tweens[10].to_a), (0.0, 1.0));

    update_row_tweens(&mut row_tweens, &mut key, 12, 6, 1.0, 20.0, 0.1);
    assert_eq!((row_tweens[0].t, row_tweens[0].a()), (0.5, 0.5));
    assert_eq!((row_tweens[10].t, row_tweens[10].a()), (0.5, 0.5));
}

#[test]
fn borrowed_row_layout_does_not_clone_shared_geometry() {
    let state = init();
    let asset_manager = AssetManager::new();
    let row_idx = row_position(MACHINE_OPTIONS_ROWS, SubRowId::VisualStyle)
        .expect("machine options should contain visual style");
    let owned = submenu_row_layout(&state, &asset_manager, SubmenuKind::Machine, row_idx)
        .expect("visual style should have a row layout");
    let strong_count = Arc::strong_count(&owned.texts);

    let borrowed = borrow_submenu_row_layout(&state, &asset_manager, SubmenuKind::Machine, row_idx)
        .expect("cached visual style layout should be borrowable");

    assert_eq!(Arc::strong_count(&borrowed.texts), strong_count);
    assert_eq!(borrowed.texts.as_ref(), owned.texts.as_ref());
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
        )) if *path == "assets/sounds/start.ogg"
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

    let pending_capacity = state.pending_sfx.capacity();
    let mut effects = Vec::with_capacity(2);
    append_pending_effects(&mut state, effect, &mut effects);
    assert!(matches!(
        &effects[0],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            deadsync_theme::AudioRequest::PlaySfx(path)
        )) if *path == "assets/sounds/change_value.ogg"
    ));
    assert!(matches!(
        effects[1],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
            crate::SimplyLoveConfigRequest::ShowOverlay(2)
        ))
    ));
    assert!(state.pending_sfx.is_empty());
    assert_eq!(state.pending_sfx.capacity(), pending_capacity);
}

#[test]
fn queued_audio_follows_returned_work_without_spilling() {
    let mut state = init();
    queue_audio(&mut state, AudioRequest::SetSampleRate(Some(48_000)));
    queue_audio(
        &mut state,
        AudioRequest::SetOutputMode(AudioOutputModeChoice::Shared),
    );
    assert!(!state.pending_audio.spilled());
    let effect = ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
        crate::SimplyLoveConfigRequest::ShowOverlay(2),
    ));

    let mut effects = Vec::with_capacity(3);
    append_pending_effects(&mut state, effect, &mut effects);

    assert!(matches!(
        effects.as_slice(),
        [
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
                crate::SimplyLoveConfigRequest::ShowOverlay(2)
            )),
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                AudioRequest::SetSampleRate(Some(48_000))
            )),
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                AudioRequest::SetOutputMode(AudioOutputModeChoice::Shared)
            ))
        ]
    ));
    assert!(state.pending_audio.is_empty());
    assert!(!state.pending_audio.spilled());
}

#[test]
fn queued_sfx_precede_score_import_runtime_work() {
    let mut state = init();
    queue_sfx(&mut state, "assets/sounds/start.ogg");
    queue_online(
        &mut state,
        crate::SimplyLoveOnlineRequest::CancelScoreImport,
    );

    let mut effects = Vec::with_capacity(2);
    append_pending_effects(&mut state, ThemeEffect::None, &mut effects);
    assert!(matches!(
        &effects[0],
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            deadsync_theme::AudioRequest::PlaySfx(path)
        )) if *path == "assets/sounds/start.ogg"
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
fn retained_score_pack_rows_match_legacy_text_and_geometry() {
    let benchmark = ScoreImportPickerBenchmark::new();
    let mut legacy = Vec::with_capacity(20);
    let mut current = Vec::with_capacity(20);

    assert_eq!(
        benchmark.legacy_frame(&mut legacy, 7),
        benchmark.current_frame(&mut current, 7)
    );
    assert_eq!(legacy.len(), current.len());
    for (legacy, current) in legacy.iter().zip(&current) {
        let (
            Actor::Text {
                content: legacy_text,
                align: legacy_align,
                offset: legacy_offset,
                z: legacy_z,
                ..
            },
            Actor::Text {
                content: current_text,
                align: current_align,
                offset: current_offset,
                z: current_z,
                ..
            },
        ) = (legacy, current)
        else {
            panic!("picker benchmark emitted a non-text actor");
        };
        assert_eq!(legacy_text.as_str(), current_text.as_str());
        assert_eq!(legacy_align, current_align);
        assert_eq!(legacy_offset, current_offset);
        assert_eq!(legacy_z, current_z);
    }
}

#[test]
fn update_drain_emits_a_queued_sound_without_follow_up_work() {
    let mut state = init();
    queue_sfx(&mut state, "assets/sounds/change.ogg");

    let mut effects = Vec::new();
    append_pending_effects(&mut state, ThemeEffect::None, &mut effects);
    assert!(matches!(
        effects.as_slice(),
        [ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            deadsync_theme::AudioRequest::PlaySfx(path)
        ))] if *path == "assets/sounds/change.ogg"
    ));
    assert!(state.pending_sfx.is_empty());
}

#[test]
fn content_reload_requests_use_prepared_paths() {
    let mut state = init();

    let effect = start_reload_songs_and_courses(&mut state);

    assert!(matches!(
        effect,
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Content(
            crate::SimplyLoveContentRequest::ReloadLibrary {
                songs_root,
                courses_root,
            }
        )) if songs_root == std::path::Path::new("/data/songs")
            && courses_root == std::path::Path::new("/data/courses")
    ));
    assert!(state.reload_ui.is_some());
}

#[test]
fn shell_content_events_drive_reload_progress_and_completion() {
    let mut state = init();
    let _ = start_reload_songs_and_courses(&mut state);

    sync_reload_events(
        &mut state,
        vec![
            crate::views::SimplyLoveContentReloadEvent::Song {
                done: 3,
                total: 8,
                pack: "Pack".to_owned(),
                song: "Song".to_owned(),
            },
            crate::views::SimplyLoveContentReloadEvent::Finished {
                song_packs: Vec::new(),
            },
        ],
    );

    let reload = state
        .reload_ui
        .as_ref()
        .expect("reload chrome should remain");
    assert_eq!((reload.songs_done, reload.songs_total), (3, 8));
    assert_eq!(
        (reload.line2.as_str(), reload.line3.as_str()),
        ("Pack", "Song")
    );
    assert!(reload.done);

    let mut effects = Vec::new();
    update(
        &mut state,
        0.0,
        &AssetManager::new(),
        &SmxAssignmentView::default(),
        &mut effects,
    );
    assert!(effects.is_empty());
    assert!(state.reload_ui.is_none());
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
            &mut Vec::new(),
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
    clear_render_cache(&state);

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

    let mut effects = Vec::new();
    update(
        &mut state,
        SUBMENU_FADE_DURATION + 0.001,
        &asset_manager,
        &SmxAssignmentView::default(),
        &mut effects,
    );

    assert!(
        matches!(
            effects.as_slice(),
            [ThemeEffect::Runtime(
                crate::SimplyLoveRuntimeRequest::Graphics(deadsync_theme::GraphicsRequest {
                    software_threads: Some(2),
                    ..
                })
            )]
        ),
        "unexpected effects: {effects:?}"
    );
}

#[test]
fn graphics_aspect_change_is_independent_of_resolution() {
    let asset_manager = AssetManager::new();
    let mut state = init();
    state.view = OptionsView::Submenu(SubmenuKind::Graphics);
    let (width, height) = selected_resolution(&state);
    state.display_width_at_load = width;
    state.display_height_at_load = height;
    *get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::DisplayAspectRatio,
    )
    .expect("display aspect row") = 2;
    state.submenu_transition = SubmenuTransition::FadeOutToMain;

    let mut effects = Vec::new();
    update(
        &mut state,
        SUBMENU_FADE_DURATION + 0.001,
        &asset_manager,
        &SmxAssignmentView::default(),
        &mut effects,
    );

    assert!(matches!(
        effects.as_slice(),
        [ThemeEffect::Runtime(
            crate::SimplyLoveRuntimeRequest::Graphics(deadsync_theme::GraphicsRequest {
                aspect_ratio: Some(ratio),
                resolution: None,
                ..
            })
        )] if (*ratio - 4.0 / 3.0).abs() <= f32::EPSILON
    ));
}
