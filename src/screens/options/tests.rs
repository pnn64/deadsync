use super::*;
use crate::assets::AssetManager;
use crate::config::{MAX_FPS_MAX, MAX_FPS_MIN};
use deadsync_core::input::InputSource;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_lights::DriverKind as LightsDriverKind;
use deadsync_profile as profile_data;
use std::time::{Duration, Instant};

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

fn press(state: &mut State, asset_manager: &AssetManager, action: VirtualAction) -> ScreenAction {
    handle_input(state, asset_manager, &input_event(action, true))
}

fn dedicated_press(
    state: &mut State,
    asset_manager: &AssetManager,
    action: VirtualAction,
) -> ScreenAction {
    handle_dedicated_three_key_options_input(state, asset_manager, &input_event(action, true))
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
    let idx = display::display_aspect_choice_index(1024, 768);
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
        LightsDriverKind::Off,
        LightsDriverKind::Snek,
        LightsDriverKind::Litboard,
        LightsDriverKind::Win32Serial,
        LightsDriverKind::Fusion,
        LightsDriverKind::Gpb,
        LightsDriverKind::PacDrive,
        LightsDriverKind::PiuioLeds,
        LightsDriverKind::Itgio,
        LightsDriverKind::HidBlueDot,
        LightsDriverKind::Stac2,
        LightsDriverKind::MinimaidHid,
    ];

    assert_eq!(LIGHTS_OPTIONS_ROWS[0].choices.len(), cases.len());
    assert!(
        !LIGHTS_OPTIONS_ROWS[0].inline,
        "the driver list is too long to render every choice in one row"
    );
    for driver in cases {
        let idx = lights_driver_choice_index(driver);
        assert_eq!(lights_driver_from_choice(idx), driver);
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
    update(&mut state, 1.0, &asset_manager);
    update(&mut state, 1.0, &asset_manager);
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
    assert_eq!(state.selected, visible_items().len() - 1);
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
    let mut state = init();
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

    update(&mut state, 0.0, &asset_manager);
    assert_eq!(
        state.sub[SubmenuKind::Graphics].cursor_indices[row],
        after_press
    );

    update(
        &mut state,
        (NAV_INITIAL_HOLD_DELAY + Duration::from_millis(1)).as_secs_f32(),
        &asset_manager,
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
    let item_pos = visible_items()
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
fn folder_path_for_row_resolves_each_folder_row() {
    use deadlib_platform::dirs::app_dirs;
    let dirs = app_dirs();
    let expectations: &[(SubRowId, std::path::PathBuf)] = &[
        (SubRowId::FoldersDataDir, dirs.data_dir.clone()),
        (SubRowId::FoldersCacheDir, dirs.cache_dir.clone()),
        (SubRowId::FoldersSongs, dirs.songs_dir()),
        (SubRowId::FoldersCourses, dirs.courses_dir()),
        (SubRowId::FoldersProfiles, dirs.profiles_root()),
        (SubRowId::FoldersScreenshots, dirs.screenshots_dir()),
        (SubRowId::FoldersLogFile, dirs.log_path()),
        (SubRowId::FoldersConfigFile, dirs.config_path()),
    ];
    for (id, expected) in expectations {
        assert_eq!(
            folder_path_for_row(*id).as_ref(),
            Some(expected),
            "row {:?} should resolve to {}",
            id,
            expected.display()
        );
        assert!(is_folder_row(*id));
    }

    assert!(folder_path_for_row(SubRowId::Game).is_none());
    assert!(!is_folder_row(SubRowId::Game));
}

/// Run pending submenu fades to completion (cap iterations so a stuck transition
/// fails the test rather than hanging).
fn settle_submenu(state: &mut State, asset_manager: &AssetManager) {
    for _ in 0..16 {
        if matches!(state.submenu_transition, SubmenuTransition::None) {
            return;
        }
        update(state, SUBMENU_FADE_DURATION + 0.001, asset_manager);
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
