use super::*;
use crate::{data_paths_perf_support::compare, perf::measure};
use std::hint::black_box;

include!("data_paths_original.rs");

fn state(count: usize) -> State {
    let mut state = init();
    state.song_packs = (0..count)
        .map(|i| OptionsSongPackView {
            group_name: format!("Pack {i:05} 日本語"),
            display_name: format!("Display {:05}", count - i),
            songs: Vec::new(),
        })
        .collect();
    state.score_import_pack_selected = (0..count)
        .step_by(2)
        .map(|i| format!("pack {i:05} 日本語"))
        .collect();
    state
}

fn equivalent(old: &State, new: &State) {
    assert_eq!(
        format!("{:?}", old.score_import_pack_options),
        format!("{:?}", new.score_import_pack_options)
    );
    assert_eq!(
        old.score_import_pack_selected,
        new.score_import_pack_selected
    );
    assert_eq!(
        format!("{:?}", old.score_import_pack_picker),
        format!("{:?}", new.score_import_pack_picker)
    );
    assert_eq!(
        old.sub[SubmenuKind::ScoreImport].choice_indices,
        new.sub[SubmenuKind::ScoreImport].choice_indices
    );
    assert_eq!(
        old.sub[SubmenuKind::ScoreImport].cursor_indices,
        new.sub[SubmenuKind::ScoreImport].cursor_indices
    );
    assert_eq!(old.display_mode_choices, new.display_mode_choices);
    assert_eq!(old.software_thread_labels, new.software_thread_labels);
    assert_eq!(
        format!("{:?}", old.sound_device_options),
        format!("{:?}", new.sound_device_options)
    );
    assert_eq!(old.sync_pack_choices, new.sync_pack_choices);
    assert_eq!(old.sync_pack_filters, new.sync_pack_filters);
    assert_eq!(old.i18n_revision, new.i18n_revision);
}

#[test]
fn data_paths_pack_refresh_preserves_selection_and_controls() {
    for count in [0, 1, 2, 32] {
        for selection in 0..3 {
            let mut old = state(count);
            let mut new = state(count);
            for state in [&mut old, &mut new] {
                state.song_packs.extend([
                    OptionsSongPackView {
                        group_name: " PACK 00000 日本語 ".into(),
                        display_name: "duplicate".into(),
                        songs: Vec::new(),
                    },
                    OptionsSongPackView {
                        group_name: "  ".into(),
                        display_name: "empty".into(),
                        songs: Vec::new(),
                    },
                    OptionsSongPackView {
                        group_name: " École ".into(),
                        display_name: "  ".into(),
                        songs: Vec::new(),
                    },
                    OptionsSongPackView {
                        group_name: " école ".into(),
                        display_name: "lowercase accent".into(),
                        songs: Vec::new(),
                    },
                ]);
                if selection == 0 {
                    state.score_import_pack_selected.clear();
                }
                if selection == 2 {
                    state
                        .score_import_pack_selected
                        .extend((0..count).map(|i| format!("pack {i:05} 日本語")));
                    state
                        .score_import_pack_selected
                        .extend(["École".into(), "école".into()]);
                }
                state
                    .score_import_pack_selected
                    .insert("removed pack".into());
                if selection == 1 {
                    open_score_import_pack_picker(state);
                    state.score_import_pack_picker.as_mut().unwrap().cursor = 999;
                }

                state.sub[SubmenuKind::ScoreImport].choice_indices[SCORE_IMPORT_ROW_PACK_INDEX] =
                    999;
                state.sub[SubmenuKind::ScoreImport].cursor_indices[SCORE_IMPORT_ROW_PACK_INDEX] =
                    999;
            }
            original_refresh_score_import_pack_options(&mut old);
            refresh_score_import_pack_options(&mut new);
            equivalent(&old, &new);
            assert!(!new.score_import_pack_selected.contains("removed pack"));
            for state in [&mut old, &mut new] {
                state.i18n_revision = u64::MAX;
                state.song_packs.truncate(count / 2);
            }
            original_sync_i18n_cache(&mut old);
            sync_i18n_cache(&mut new);
            equivalent(&old, &new);
        }
    }
}

#[test]
fn data_paths_pack_refresh_eliminates_per_key_copies() {
    for count in [1, 32, 256] {
        for locale in [false, true] {
            let mut old = state(count);
            let mut new = state(count);
            old.i18n_revision = u64::MAX;
            new.i18n_revision = u64::MAX;
            // Warm translations and both paths before measuring allocations.
            if locale {
                original_sync_i18n_cache(&mut old);
                sync_i18n_cache(&mut new);
            } else {
                original_refresh_score_import_pack_options(&mut old);
                refresh_score_import_pack_options(&mut new);
            }
            old.i18n_revision = u64::MAX;
            new.i18n_revision = u64::MAX;
            let (_, before) = measure(|| {
                if locale {
                    original_sync_i18n_cache(&mut old)
                } else {
                    original_refresh_score_import_pack_options(&mut old)
                }
            });
            let (_, after) = measure(|| {
                if locale {
                    sync_i18n_cache(&mut new)
                } else {
                    refresh_score_import_pack_options(&mut new)
                }
            });
            equivalent(&old, &new);
            assert_eq!(
                before.allocs - after.allocs,
                count + usize::from(old.score_import_pack_selected.is_empty()),
                "count={count}, locale={locale}"
            );
            assert!(after.allocated_bytes < before.allocated_bytes);
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_data_paths_pack_refresh() {
    for count in [0, 32, 256, 4096] {
        for (locale, selected) in [(false, false), (false, true), (true, false), (true, true)] {
            let mut old = state(count);
            let mut new = state(count);
            if !selected {
                old.score_import_pack_selected.clear();
                new.score_import_pack_selected.clear();
            }
            compare(
                &format!(
                    "packs/{}/{}/{count}",
                    if locale { "locale" } else { "refresh" },
                    if selected { "selected" } else { "all" }
                ),
                || {
                    if locale {
                        old.i18n_revision = u64::MAX;
                        original_sync_i18n_cache(black_box(&mut old));
                    } else {
                        original_refresh_score_import_pack_options(black_box(&mut old));
                    }
                    black_box(&old.score_import_pack_selected);
                },
                || {
                    if locale {
                        new.i18n_revision = u64::MAX;
                        sync_i18n_cache(black_box(&mut new));
                    } else {
                        refresh_score_import_pack_options(black_box(&mut new));
                    }
                    black_box(&new.score_import_pack_selected);
                },
            );
            equivalent(&old, &new);
        }
    }
}
