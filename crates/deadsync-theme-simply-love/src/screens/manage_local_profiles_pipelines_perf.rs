use super::*;
use crate::perf::measure;
use crate::pipelines_support::compare;
use std::hint::black_box;

fn summary(flags: usize, count: usize) -> crate::SimplyLoveItgImportSummary {
    crate::SimplyLoveItgImportSummary {
        display_name: "Player Å猫".into(),
        scores_total: count,
        scores_imported: if flags & 1 != 0 { count / 2 } else { count },
        scores_unmapped: if flags & 1 != 0 { count - count / 2 } else { 0 },
        favorites_total: count,
        favorites_imported: if flags & 2 != 0 { count / 2 } else { count },
        simply_love_options_imported: flags & 4 != 0,
        groovestats_imported: flags & 8 != 0,
        arrowcloud_imported: flags & 16 != 0,
        avatar_imported: flags & 32 != 0,
        itl_entries_imported: if flags & 64 != 0 { count } else { 0 },
        ..Default::default()
    }
}

fn assert_same(old: &ImportMessageState, new: &ImportMessageState) {
    assert_eq!(old.title, new.title);
    assert_eq!(old.lines.len(), new.lines.len());
    assert!(old.metrics.get().is_none() && new.metrics.get().is_none());
    for (before, after) in old.lines.iter().zip(&new.lines) {
        match (before, after) {
            (
                MessageLine::Center { text: a, rgba: ac },
                MessageLine::Center { text: b, rgba: bc },
            ) => {
                assert_eq!(a, b);
                assert_eq!(ac, bc);
            }
            (
                MessageLine::Row {
                    label: al,
                    status: av,
                    kind: ak,
                },
                MessageLine::Row {
                    label: bl,
                    status: bv,
                    kind: bk,
                },
            ) => {
                assert_eq!(al, bl);
                assert_eq!(av, bv);
                assert!(ak == bk);
            }
            _ => panic!("summary line kind changed"),
        }
    }
}

#[test]
fn import_summary_preserves_all_sections_statuses_and_counts() {
    for flags in 0..128 {
        for count in [0, 1, 999, 1_000, 12_345, usize::MAX] {
            let data = summary(flags, count);
            assert_same(
                &pipelines_original::import_summary_message(&data),
                &import_summary_message(&data),
            );
        }
    }
}

#[test]
fn import_summary_preserves_localized_and_opaque_profile_names() {
    for name in [
        "",
        "\0",
        "  Player  ",
        "Å猫\n{name}",
        "Player with a longer name beyond inline text storage",
    ] {
        let mut data = summary(127, 12_345);
        data.display_name = name.into();
        assert_same(
            &pipelines_original::import_summary_message(&data),
            &import_summary_message(&data),
        );
    }
}

#[test]
fn import_summary_keeps_translations_shared_without_string_round_trips() {
    for (flags, count) in [(0, 0), (127, 1_234), (3, 99_999)] {
        let data = summary(flags, count);
        black_box(import_summary_message(&data));
        let (old, before) = measure(|| pipelines_original::import_summary_message(&data));
        let (new, after) = measure(|| import_summary_message(&data));
        assert_same(&old, &new);
        assert_eq!(before.allocs - after.allocs, new.lines.len() * 2);
        assert!(after.allocated_bytes < before.allocated_bytes);
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_ui_pipelines_import_summary() {
    for (flags, count, label) in [(0, 0, "empty"), (127, 1_234, "all"), (3, 99_999, "partial")] {
        let data = summary(flags, count);
        compare(
            &format!("summary/{label}"),
            || {
                black_box(pipelines_original::import_summary_message(black_box(&data)));
            },
            || {
                black_box(import_summary_message(black_box(&data)));
            },
        );
    }
}
