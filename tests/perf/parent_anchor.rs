include!("row_fixtures.rs");

mod original {
    use super::*;
    include!("original_parent_anchor.rs");
}

fn rows(order: &[RowId]) -> RowMap {
    let mut map = RowMap::new();
    for &id in ROW_IDS {
        map.insert(Row {
            id,
            behavior: RowBehavior::Exit,
            name: crate::i18n::lookup_key("PlayerOptions", "JudgmentFont"),
            choices: Box::new([]),
            selected_choice_index: [0; 2],
            help: Box::new([]),
            choice_difficulty_indices: None,
            mirror_across_players: false,
            choice_widths: Box::new([]),
            choice_offsets: Box::new([]),
            choice_height: 0.0,
        });
    }
    map.display_order = order.to_vec();
    map
}

fn masks() -> Vec<u64> {
    let mut masks = vec![0, u64::MAX, 0xaaaa_aaaa_aaaa_aaaa, 0x5555_5555_5555_5555];
    let mut seed = 0x28a9_f153u64;
    for _ in 0..32 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        masks.push(seed);
    }
    masks
}

#[test]
fn parent_anchors_match_original_across_flags_orders_and_missing_rows() {
    assert_eq!(ROW_IDS.len(), RowId::COUNT);
    let mut order = ROW_IDS.to_vec();
    for layout in 0..4 {
        let mut map = rows(&order);
        if layout == 2 {
            for (i, row) in map.rows.iter_mut().enumerate() {
                if i % 7 == 0 {
                    *row = None;
                }
            }
        }
        for mask in masks() {
            for &id in ROW_IDS {
                let flags = visibility(mask);
                assert_eq!(
                    parent_anchor_visible_index(&map, id, flags),
                    original::parent_anchor_visible_index(&map, id, flags),
                    "{layout} {mask:x} {id:?}"
                );
            }
        }
        match layout {
            0 => order.reverse(),
            1 => order.extend_from_within(..9),
            2 => order.truncate(ROW_IDS.len() / 2),
            _ => {}
        }
    }
    let empty = rows(&[]);
    for &id in ROW_IDS {
        assert_eq!(
            parent_anchor_visible_index(&empty, id, visibility(u64::MAX)),
            None
        );
    }
}

#[test]
fn hidden_child_anchors_preserve_parent_visibility_and_position() {
    let mut map = rows(ROW_IDS);
    map.display_order.reverse();
    for mask in masks() {
        for index in 0..=map.len() {
            let flags = visibility(mask);
            assert_eq!(
                hidden_row_anchor_visible_index(&map, index, flags),
                original::hidden_row_anchor_visible_index(&map, index, flags),
                "{mask:x} {index}"
            );
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --nocapture --test-threads=1"]
fn benchmark_direct_scans() {
    for (label, count, hidden) in [
        ("anchor/visible_8", 8, false),
        ("anchor/visible_64", 64, false),
        ("anchor/visible_118", ROW_IDS.len(), false),
        ("anchor/hidden_64", 64, true),
    ] {
        let parent = if hidden {
            RowId::DataVisualizations
        } else {
            RowId::GameplayExtrasMore
        };
        let mut order = ROW_IDS.to_vec();
        order.retain(|&id| id != parent);
        order.truncate(count - 1);
        order.push(parent);
        assert_eq!(order.len(), count);
        let map = rows(&order);
        let flags = visibility(if hidden { 0 } else { u64::MAX });
        assert_eq!(
            parent_anchor_visible_index(&map, parent, flags),
            original::parent_anchor_visible_index(&map, parent, flags)
        );
        crate::scan_perf::compare(
            label,
            || (&map, parent, flags),
            |(map, parent, flags)| original::parent_anchor_visible_index(map, parent, flags),
            |(map, parent, flags)| parent_anchor_visible_index(map, parent, flags),
        );
    }
}
