use super::*;
use crate::buffers_support::compare;
use crate::perf::measure;
use std::hint::black_box;

fn labels(count: usize, long: bool) -> Vec<String> {
    (0..count)
        .map(|i| {
            if long {
                format!("Palette Å猫 {i} with a localized name beyond inline storage")
            } else {
                format!("Palette {i}")
            }
        })
        .collect()
}

#[test]
fn borrowed_and_owned_choices_keep_inline_boundaries_and_unicode() {
    for length in 0..=160 {
        let choices = vec![
            String::new(),
            " \tÅ猫\0 ".into(),
            "x".repeat(length),
            "猫".repeat(length),
        ];
        let old = buffers_original::actor_texts(choices.clone());
        let borrowed = actor_texts(&choices);
        let owned = actor_texts(choices);
        assert_eq!(format!("{old:?}"), format!("{borrowed:?}"));
        assert_eq!(format!("{old:?}"), format!("{owned:?}"));
    }
}

#[test]
fn palette_replacement_invalidates_layout_and_preserves_each_player_selection() {
    use super::super::apply_judgment_palette_choices;
    let mut map = RowMap::new();
    // A missing palette row still does nothing.
    apply_judgment_palette_choices(&mut map, &[], &[], &[None, None]);
    let mut row = Row::exit();
    row.id = RowId::JudgmentColors;
    row.choice_widths = vec![5.0, 8.0].into_boxed_slice();
    row.choice_offsets = vec![0.0, 9.0].into_boxed_slice();
    row.choice_height = 20.0;
    map.insert(row);
    let choices = labels(3, false);
    let ids = vec![None, Some("chosen".to_owned()), Some("chosen".to_owned())];
    apply_judgment_palette_choices(
        &mut map,
        &choices,
        &ids,
        &[Some("chosen".into()), Some("missing".into())],
    );
    let row = map.row(RowId::JudgmentColors);
    assert_eq!(row.selected_choice_index, [1, 0]);
    assert!(row.choice_widths.is_empty() && row.choice_offsets.is_empty());
    assert_eq!(row.choice_height, 0.0);
    assert_eq!(
        row.choices
            .iter()
            .map(TextContent::as_str)
            .collect::<Vec<_>>(),
        choices.iter().map(String::as_str).collect::<Vec<_>>()
    );
    apply_judgment_palette_choices(&mut map, &[], &[], &[None, None]);
    assert!(map.row(RowId::JudgmentColors).choices.is_empty());
    assert_eq!(map.row(RowId::JudgmentColors).selected_choice_index, [0, 0]);
}

#[test]
fn borrowed_choices_remove_copied_strings_and_input_vector() {
    for long in [false, true] {
        let choices = labels(32, long);
        let (old, before) = measure(|| buffers_original::actor_texts(choices.clone()));
        let (new, after) = measure(|| actor_texts(&choices));
        assert_eq!(format!("{old:?}"), format!("{new:?}"));
        assert!(before.allocs - after.allocs >= choices.len());
        assert!(after.allocated_bytes < before.allocated_bytes);
        // Existing owned-input callers retain the same allocator costs.
        let first = choices.clone();
        let second = choices.clone();
        let (_, old_owned) = measure(|| buffers_original::actor_texts(first));
        let (_, new_owned) = measure(|| actor_texts(second));
        assert_eq!(old_owned, new_owned);
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_transient_buffers_palette_choices() {
    for (count, long) in [(1, false), (16, false), (128, false), (32, true)] {
        let choices = labels(count, long);
        compare(
            &format!("palette/{count}-{}", if long { "long" } else { "short" }),
            || {
                black_box(buffers_original::actor_texts(black_box(&choices).clone()));
            },
            || {
                black_box(actor_texts(black_box(&choices)));
            },
        );
    }
    let choices = labels(32, true);
    compare(
        "owned-control/32-long",
        || {
            black_box(buffers_original::actor_texts(black_box(&choices).clone()));
        },
        || {
            black_box(actor_texts(black_box(&choices).clone()));
        },
    );
}
