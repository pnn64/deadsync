use super::*;
use crate::assets::AssetManager;
use crate::engine::input::{InputEvent, InputSource, VirtualAction};
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

fn age_start_hold(state: &mut State, side: profile::PlayerSide) {
    let idx = screen_input::player_side_ix(side);
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
    let idx = inferred_aspect_choice(1024, 768);
    assert_eq!(
        DISPLAY_ASPECT_RATIO_CHOICES[idx].as_str_static(),
        Some("4:3")
    );
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
        assert_eq!(
            LIGHTS_OPTIONS_ROWS[0].choices[idx].as_str_static(),
            Some(driver.as_str())
        );
    }
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

    age_start_hold(&mut state, profile::PlayerSide::P1);
    assert!(
        repeat_held_dedicated_three_key_start(
            &mut state,
            &asset_manager,
            profile::PlayerSide::P1,
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

    age_start_hold(&mut state, profile::PlayerSide::P1);
    assert!(
        repeat_held_dedicated_three_key_start(
            &mut state,
            &asset_manager,
            profile::PlayerSide::P1,
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

    age_start_hold(&mut state, profile::PlayerSide::P1);
    assert!(
        repeat_held_dedicated_three_key_start(
            &mut state,
            &asset_manager,
            profile::PlayerSide::P1,
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

    age_start_hold(&mut state, profile::PlayerSide::P1);
    assert!(
        repeat_held_dedicated_three_key_start(
            &mut state,
            &asset_manager,
            profile::PlayerSide::P1,
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
fn random_movies_only_shows_when_video_bgs_are_on() {
    let mut state = init();

    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::VideoBgs,
        yes_no_choice_index(false),
    );
    let visible = submenu_visible_row_indices(&state, SubmenuKind::Machine, MACHINE_OPTIONS_ROWS);
    assert!(
        !visible
            .iter()
            .any(|&idx| MACHINE_OPTIONS_ROWS[idx].id == SubRowId::RandomBackgroundMode)
    );

    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::VideoBgs,
        yes_no_choice_index(true),
    );
    let visible = submenu_visible_row_indices(&state, SubmenuKind::Machine, MACHINE_OPTIONS_ROWS);
    assert!(
        visible
            .iter()
            .any(|&idx| MACHINE_OPTIONS_ROWS[idx].id == SubRowId::RandomBackgroundMode)
    );
}
