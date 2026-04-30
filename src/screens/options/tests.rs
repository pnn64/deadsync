use super::*;
use crate::assets::AssetManager;
use crate::engine::input::{InputEvent, InputSource, VirtualAction};
use std::time::{Duration, Instant};

fn press(
    state: &mut State,
    asset_manager: &AssetManager,
    action: VirtualAction,
) -> ScreenAction {
    let now = Instant::now();
    handle_input(
        state,
        asset_manager,
        &InputEvent {
            action,
            input_slot: 0,
            pressed: true,
            source: InputSource::Keyboard,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        },
    )
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

    let before = state.sub_cursor_indices_system[3];
    press(&mut state, &asset_manager, VirtualAction::p2_right);
    assert_eq!(state.sub_cursor_indices_system[3], before + 1);
}
