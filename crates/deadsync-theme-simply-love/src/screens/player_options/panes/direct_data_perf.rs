//! Compare real option rows and masks with the original three-pass initializer.
use super::*;
use deadsync_profile::PlayerOptionsData;
use std::hint::black_box;
#[path = "../../../../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod allocations;

mod support {
    pub use super::allocations::measure;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/perf/direct_data_support.rs"
    ));
}
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/perf/direct_data_original_rows.rs"
));

fn init_original(
    rows: &mut RowMap,
    profile: &PlayerOptionsData,
    masks: &mut PlayerOptionMasks,
    player: usize,
) {
    original_init_opted_in_bitmask_rows(rows, profile, masks, player);
    original_init_opted_in_cycle_rows(rows, profile, player);
    original_init_opted_in_numeric_rows(rows, profile, player);
}
fn init_current(
    rows: &mut RowMap,
    profile: &PlayerOptionsData,
    masks: &mut PlayerOptionMasks,
    player: usize,
) {
    init_opted_in_bitmask_rows(rows, profile, masks, player);
    init_opted_in_cycle_rows(rows, profile, player);
    init_opted_in_numeric_rows(rows, profile, player);
}
fn rows(pane: usize) -> RowMap {
    crate::tests::init_paths();
    crate::i18n::init_for_tests();
    match pane {
        0 => RowMap::new(),
        1 => build_uncommon_rows(Screen::SelectMusic),
        2 => build_advanced_rows(Screen::SelectMusic, true),
        _ => build_display_rows(&[], &[], &[], Screen::SelectMusic),
    }
}
fn profile(seed: i32) -> PlayerOptionsData {
    let mut profile = PlayerOptionsData::default();
    profile.display_scorebox = seed & 1 != 0;
    profile.judgment_tilt = seed & 2 != 0;
    profile.custom_fantastic_window = seed & 4 != 0;
    profile.transparent_density_graph_bg = seed & 8 != 0;
    profile.error_bar_offset_x = seed * 17 - 100;
    profile.error_bar_offset_y = 100 - seed * 17;
    profile.text_error_bar_threshold_ms = seed as u32 * 5;
    profile.turn_option = TURN_OPTION_VARIANTS[seed as usize % TURN_OPTION_VARIANTS.len()];
    profile
}
fn cursors(rows: &RowMap) -> Vec<Option<[usize; PLAYER_SLOTS]>> {
    rows.rows
        .iter()
        .map(|row| row.as_ref().map(|row| row.selected_choice_index))
        .collect()
}

#[test]
fn option_initializers_match_original_for_both_players_and_sparse_orders() {
    for pane in 0..4 {
        for seed in 0..32 {
            let mut original = rows(pane);
            let mut current = rows(pane);
            for rows in [&mut original, &mut current] {
                for row in rows.rows.iter_mut().flatten() {
                    row.selected_choice_index = [7, 11];
                }
                if seed % 2 == 1 {
                    rows.display_order.reverse();
                    if let Some(&id) = rows.display_order.first() {
                        rows.display_order.push(id); // Repeated callbacks must retain their order.
                    }
                }
                if seed % 3 == 0 {
                    for (index, row) in rows.rows.iter_mut().enumerate() {
                        if index % 3 == 0 {
                            *row = None;
                        } // IDs may refer to missing rows.
                    }
                }
            }
            let order = current.display_order.clone();
            let mut old_masks = PlayerOptionMasks::default();
            let mut new_masks = PlayerOptionMasks::default();
            for player in [1, 0, 1] {
                let profile = profile(seed);
                init_original(&mut original, &profile, &mut old_masks, player);
                init_current(&mut current, &profile, &mut new_masks, player);
                assert_eq!(cursors(&original), cursors(&current));
                assert_eq!(old_masks, new_masks);
                assert_eq!(current.display_order, order);
            }
        }
    }
}

#[test]
fn option_initialization_eliminates_three_order_allocations() {
    let profile = profile(17);
    for pane in 1..4 {
        let mut original = rows(pane);
        let mut current = rows(pane);
        let mut old_masks = PlayerOptionMasks::default();
        let mut new_masks = PlayerOptionMasks::default();
        let (_, before) =
            support::measure(|| init_original(&mut original, &profile, &mut old_masks, 0));
        let (_, after) =
            support::measure(|| init_current(&mut current, &profile, &mut new_masks, 0));
        assert_eq!(before.allocs - after.allocs, 3);
        assert_eq!(
            before.allocated_bytes - after.allocated_bytes,
            3 * current.len() * std::mem::size_of::<RowId>()
        );
        assert_eq!(cursors(&original), cursors(&current));
        assert_eq!(old_masks, new_masks);
    }
}

#[test]
#[ignore = "paired release benchmark; run alone"]
fn benchmark_direct_data_rows() {
    for (pane, label) in [
        (0, "rows/empty"),
        (1, "rows/uncommon"),
        (2, "rows/advanced"),
        (3, "rows/display"),
    ] {
        let profile = profile(17);
        let mut original = rows(pane);
        let mut current = rows(pane);
        let mut old_masks = PlayerOptionMasks::default();
        let mut new_masks = PlayerOptionMasks::default();
        support::compare(
            label,
            || {
                init_original(
                    black_box(&mut original),
                    black_box(&profile),
                    &mut old_masks,
                    black_box(0),
                );
                black_box((&original, &old_masks));
            },
            || {
                init_current(
                    black_box(&mut current),
                    black_box(&profile),
                    &mut new_masks,
                    black_box(0),
                );
                black_box((&current, &new_masks));
            },
        );
    }
}

thread_local! {
    static CALLBACKS: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
}
fn record_callback(name: &'static str) {
    CALLBACKS.with_borrow_mut(|calls| calls.push(name));
}

#[test]
fn option_initialization_preserves_callback_order_and_count() {
    let mut rows = rows(2);
    for row in rows.rows.iter_mut().flatten() {
        use crate::screens::player_options::row::{BitmaskBinding, CycleBinding};
        match &mut row.behavior {
            RowBehavior::Bitmask(BitmaskBinding::Generic { init, .. }) => {
                init.from_profile = |_| {
                    record_callback("mask-profile");
                    0
                };
                init.set_active = |_, _| record_callback("mask-write");
                init.get_active = |_| {
                    record_callback("mask-read");
                    0
                };
            }
            RowBehavior::Cycle(CycleBinding::Index(binding)) => {
                if let Some(init) = &mut binding.init {
                    init.from_profile = |_| {
                        record_callback("index-profile");
                        0
                    };
                }
            }
            RowBehavior::Cycle(CycleBinding::Bool(binding)) => {
                if let Some(init) = &mut binding.init {
                    init.from_profile = |_| {
                        record_callback("bool-profile");
                        0
                    };
                }
            }
            RowBehavior::Numeric(binding) => {
                if let Some(init) = &mut binding.init {
                    init.from_profile = |_| {
                        record_callback("numeric-profile");
                        0
                    };
                    init.format = |_| {
                        record_callback("numeric-format");
                        "0".into()
                    };
                }
            }
            _ => {}
        }
    }
    rows.display_order.reverse();
    rows.display_order.extend_from_within(..);
    let profile = profile(17);
    let mut masks = PlayerOptionMasks::default();
    CALLBACKS.with_borrow_mut(Vec::clear);
    init_original(&mut rows, &profile, &mut masks, 0);
    let before = CALLBACKS.with_borrow_mut(std::mem::take);
    init_current(&mut rows, &profile, &mut masks, 0);
    let after = CALLBACKS.with_borrow_mut(std::mem::take);
    assert!(before.contains(&"mask-write"));
    assert!(before.contains(&"numeric-format"));
    assert!(before.contains(&"index-profile"));
    assert_eq!(before, after);
}
