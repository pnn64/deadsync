
use super::*;
use crate::perf::assert_no_churn;


fn state_with_preferred_color() -> State {
    let mut state = init();
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::SelectColor,
        yes_no_choice_index(false),
    );
    clear_submenu_visible_rows_cache(&state);
    state
}

fn visible_row(state: &State, kind: SubmenuKind, id: SubRowId) -> usize {
    (0..submenu_total_rows(state, kind))
        .find(|&visible| {
            submenu_visible_row_to_actual(state, kind, visible)
                .is_some_and(|actual| submenu_rows(kind)[actual].id == id)
        })
        .expect("fixture row should be visible")
}


#[test]
fn borrowed_navigation_keeps_cached_geometry_and_has_no_allocator_churn() {
    let mut state = state_with_preferred_color();
    let assets = AssetManager::new();
    let kind = SubmenuKind::Machine;
    for id in [SubRowId::VisualStyle, SubRowId::PreferredColor] {
        let visible = visible_row(&state, kind, id);
        sync_submenu_inline_x_from_row(&mut state, &assets, kind, visible);
        let actual = submenu_visible_row_to_actual(&state, kind, visible).unwrap();
        let pointer = borrow_submenu_row_layout(&state, &assets, kind, actual)
            .unwrap()
            .centers
            .as_ptr();
        assert_no_churn(|| {
            sync_submenu_inline_x_from_row(&mut state, &assets, kind, visible);
            apply_submenu_inline_x_to_row(&mut state, &assets, kind, visible);
        });
        let layout = borrow_submenu_row_layout(&state, &assets, kind, actual).unwrap();
        assert_eq!(layout.centers.as_ptr(), pointer);
        assert_eq!(Arc::strong_count(&layout.centers), 1);
        drop(layout);
        clear_submenu_row_layout_cache(&state);
        sync_submenu_inline_x_from_row(&mut state, &assets, kind, visible);
    }
}
