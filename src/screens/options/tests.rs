use super::*;
use crate::assets::AssetManager;
use crate::engine::input::{InputEvent, InputSource, VirtualAction};
use std::time::Instant;

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
