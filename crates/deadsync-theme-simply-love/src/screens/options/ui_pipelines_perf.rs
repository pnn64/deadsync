use super::super::layout::pipelines_original as original;
use super::*;
use crate::perf::assert_no_churn;
use crate::pipelines_support::compare;
use std::hint::black_box;

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
        .expect("benchmark row should be visible")
}

fn assert_navigation_same(old: &State, new: &State, kind: SubmenuKind) {
    assert_eq!(old.sub_inline_x.to_bits(), new.sub_inline_x.to_bits());
    assert_eq!(
        submenu_choice_indices(old, kind),
        submenu_choice_indices(new, kind)
    );
    assert_eq!(
        submenu_cursor_indices(old, kind),
        submenu_cursor_indices(new, kind)
    );
}

#[test]
fn borrowed_navigation_preserves_every_submenu_and_index_boundary() {
    let mut old = init();
    let mut new = init();
    let assets = AssetManager::new();
    for kind in SubmenuKind::ALL {
        let total = submenu_total_rows(&old, kind);
        assert_eq!(total, submenu_total_rows(&new, kind));
        for visible in (0..total).chain([usize::MAX]) {
            for selected in [0, 1, usize::MAX] {
                if let Some(actual) = submenu_visible_row_to_actual(&old, kind, visible) {
                    if let Some(slot) = submenu_choice_indices_mut(&mut old, kind).get_mut(actual) {
                        *slot = selected;
                    }
                    if let Some(slot) = submenu_choice_indices_mut(&mut new, kind).get_mut(actual) {
                        *slot = selected;
                    }
                }
                old.sub_inline_x = f32::NAN;
                new.sub_inline_x = f32::NAN;
                original::sync_submenu_inline_x_from_row(&mut old, &assets, kind, visible);
                sync_submenu_inline_x_from_row(&mut new, &assets, kind, visible);
                assert_navigation_same(&old, &new, kind);
                original::apply_submenu_inline_x_to_row(&mut old, &assets, kind, visible);
                apply_submenu_inline_x_to_row(&mut new, &assets, kind, visible);
                assert_navigation_same(&old, &new, kind);
            }
        }
        // No RefCell borrow survives either operation or prevents invalidation.
        clear_submenu_row_layout_cache(&old);
        clear_submenu_row_layout_cache(&new);
    }
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

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_ui_pipelines_option_navigation() {
    let assets = AssetManager::new();
    for (kind, id, label) in [
        (SubmenuKind::Machine, SubRowId::VisualStyle, "visual"),
        (SubmenuKind::Machine, SubRowId::PreferredColor, "color"),
        (SubmenuKind::Sound, SubRowId::MasterVolume, "volume"),
    ] {
        for apply in [false, true] {
            let mut old = state_with_preferred_color();
            let mut new = state_with_preferred_color();
            let visible = visible_row(&old, kind, id);
            let original_fn = if apply {
                original::apply_submenu_inline_x_to_row
            } else {
                original::sync_submenu_inline_x_from_row
            };
            let current_fn = if apply {
                apply_submenu_inline_x_to_row
            } else {
                sync_submenu_inline_x_from_row
            };
            original_fn(&mut old, &assets, kind, visible);
            current_fn(&mut new, &assets, kind, visible);
            compare(
                &format!("layout/{}-{label}", if apply { "apply" } else { "sync" }),
                || {
                    original_fn(black_box(&mut old), black_box(&assets), kind, visible);
                    black_box(old.sub_inline_x);
                },
                || {
                    current_fn(black_box(&mut new), black_box(&assets), kind, visible);
                    black_box(new.sub_inline_x);
                },
            );
        }
    }
}
