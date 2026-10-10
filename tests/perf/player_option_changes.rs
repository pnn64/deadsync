fn dataflow_select(state: &mut super::State, pane: OptionsPane, id: RowId, player: usize) {
    super::super::choice::apply_pane(state, pane);
    state.pane_mut().selected_row[player] = state
        .pane()
        .row_map
        .display_order()
        .iter()
        .position(|&row| row == id)
        .unwrap();
}

fn assert_dataflow_states_equal(old: &super::State, new: &super::State) {
    assert_eq!(old.player_options, new.player_options);
    assert_eq!(old.judgment_palette_ids, new.judgment_palette_ids);
    assert_eq!(old.heart_rate_device_ids, new.heart_rate_device_ids);
    assert_eq!(old.max_heart_rate, new.max_heart_rate);
    assert_eq!(old.option_masks, new.option_masks);
    assert_eq!(
        format!("{:?}", old.pending_effects),
        format!("{:?}", new.pending_effects)
    );
    for (a, b) in old.panes.iter().zip(&new.panes) {
        assert_eq!(a.selected_row, b.selected_row);
        assert_eq!(a.row_map.display_order(), b.row_map.display_order());
        for &id in a.row_map.display_order() {
            assert_eq!(
                a.row_map.row(id).selected_choice_index,
                b.row_map.row(id).selected_choice_index
            );
        }
    }
}

#[test]
fn option_dispatch_preserves_updates_noops_and_mirrored_choices() {
    use super::super::{NavWrap, choice};
    ensure_i18n();
    for (pane, id) in [
        (OptionsPane::Main, RowId::Perspective),
        (OptionsPane::Main, RowId::Mini),
        (OptionsPane::Advanced, RowId::Hide),
    ] {
        let (mut old, _) = setup_versus_state();
        let (mut new, _) = setup_versus_state();
        for p in [P1, P2] {
            old.heart_rate_device_ids[p] = Some(format!("device-{p}"));
            new.heart_rate_device_ids[p] = old.heart_rate_device_ids[p].clone();
            dataflow_select(&mut old, pane, id, p);
            dataflow_select(&mut new, pane, id, p);
            for (delta, wrap) in [
                (1, NavWrap::Wrap),
                (0, NavWrap::Clamp),
                (-1, NavWrap::Clamp),
                (500, NavWrap::Clamp),
            ] {
                choice::dataflow_originals::original_dispatch_behavior_delta(
                    &mut old, p, delta, wrap,
                );
                choice::dispatch_behavior_delta(&mut new, p, delta, wrap);
                assert_dataflow_states_equal(&old, &new);
                old.pending_effects.clear();
                new.pending_effects.clear();
            }
        }
    }
}

#[test]
fn bitmask_toggle_preserves_projection_visibility_and_effects() {
    use super::super::choice;
    ensure_i18n();
    let (mut old, _) = setup_versus_state();
    let (mut new, _) = setup_versus_state();
    for p in [P1, P2] {
        old.heart_rate_device_ids[p] = Some(format!("device-{p}"));
        new.heart_rate_device_ids[p] = old.heart_rate_device_ids[p].clone();
        dataflow_select(&mut old, OptionsPane::Advanced, RowId::Hide, p);
        dataflow_select(&mut new, OptionsPane::Advanced, RowId::Hide, p);
        for index in [0, 1, 0, usize::MAX] {
            old.pane_mut()
                .row_map
                .get_mut(RowId::Hide)
                .unwrap()
                .selected_choice_index[p] = index;
            new.pane_mut()
                .row_map
                .get_mut(RowId::Hide)
                .unwrap()
                .selected_choice_index[p] = index;
            let a = choice::dataflow_originals::original_toggle_bitmask_row_generic(
                &mut old,
                p,
                RowId::Hide,
            );
            let b = choice::toggle_bitmask_row_generic(&mut new, p, RowId::Hide);
            assert_eq!(a, b);
            assert_dataflow_states_equal(&old, &new);
            old.pending_effects.clear();
            new.pending_effects.clear();
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_profile_dataflow_option_input() {
    use super::super::{NavWrap, choice};
    use std::hint::black_box;
    ensure_i18n();
    for (label, pane, id, delta, toggle) in [
        ("input/numeric", OptionsPane::Main, RowId::Mini, 1, false),
        ("input/no_change", OptionsPane::Main, RowId::Mini, 0, false),
        (
            "input/perspective",
            OptionsPane::Main,
            RowId::Perspective,
            1,
            false,
        ),
        ("input/bitmask", OptionsPane::Advanced, RowId::Hide, 1, true),
    ] {
        let (mut old, _) = setup_versus_state();
        let (mut new, _) = setup_versus_state();
        dataflow_select(&mut old, pane, id, P1);
        dataflow_select(&mut new, pane, id, P1);
        old.heart_rate_device_ids[P1] = Some("device-one".into());
        new.heart_rate_device_ids[P1] = old.heart_rate_device_ids[P1].clone();
        crate::dataflow_perf::compare(
            label,
            || {
                if toggle {
                    black_box(
                        choice::dataflow_originals::original_toggle_bitmask_row_generic(
                            black_box(&mut old),
                            P1,
                            id,
                        ),
                    );
                } else {
                    choice::dataflow_originals::original_dispatch_behavior_delta(
                        black_box(&mut old),
                        P1,
                        black_box(delta),
                        NavWrap::Wrap,
                    );
                }
                black_box(&old.pending_effects);
                old.pending_effects.clear();
            },
            || {
                if toggle {
                    black_box(choice::toggle_bitmask_row_generic(
                        black_box(&mut new),
                        P1,
                        id,
                    ));
                } else {
                    choice::dispatch_behavior_delta(
                        black_box(&mut new),
                        P1,
                        black_box(delta),
                        NavWrap::Wrap,
                    );
                }
                black_box(&new.pending_effects);
                new.pending_effects.clear();
            },
        );
    }
}
