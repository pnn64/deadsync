use super::*;
use std::hint::black_box;
#[path = "itl_unlocks_baseline.rs"]
mod baseline;

fn unlock_fixture(count: usize, value: bool) -> (ItlFileData, Vec<String>) {
    let folders: Vec<_> = (0..count)
        .map(|i| format!("/Songs/ITL Online 2026 Unlocks/日本-{i}"))
        .collect();
    let data = ItlFileData {
        unlock_folders: folders
            .iter()
            .map(|folder| (folder.clone(), value))
            .collect(),
        ..Default::default()
    };
    (data, folders)
}

#[test]
fn unlock_updates_preserve_flags_duplicates_trimming_and_reported_changes() {
    let (mut old, folders) = unlock_fixture(128, false);
    let mut new = old.clone();
    let mut names: Vec<_> = folders.iter().map(String::as_str).collect();
    names.extend([
        "",
        " \n\t",
        " new folder ",
        "new folder",
        "日本😀",
        "/Songs/ITL Online 2026 Unlocks/日本-0",
    ]);
    for _ in 0..2 {
        assert_eq!(
            baseline::itl_mark_unlock_folders(&mut old, names.iter().copied()),
            itl_mark_unlock_folders(&mut new, names.iter().copied())
        );
        assert_eq!(old.unlock_folders, new.unlock_folders);
    }
    assert!(new.unlock_folders.values().all(|value| *value));
    crate::perf::assert_no_churn(|| {
        assert!(!itl_mark_unlock_folders(&mut new, names.iter().copied()));
    });
    crate::perf::assert_reduced_churn(
        || {
            black_box(baseline::itl_mark_unlock_folders(
                &mut old,
                folders.iter().map(String::as_str),
            ));
        },
        || {
            black_box(itl_mark_unlock_folders(
                &mut new,
                folders.iter().map(String::as_str),
            ));
        },
    );
}

#[test]
#[ignore = "manual original/current unlock update benchmarks; run in release"]
fn unlock_updates_benchmark() {
    for count in [0, 64, 1024] {
        let (mut old, folders) = unlock_fixture(count, true);
        let mut new = old.clone();
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [true, false]
        } else {
            [false, true]
        };
        for current in order {
            crate::perf::measure_sampled(
                &format!(
                    "itl-unlocks/count={count}/existing/{}",
                    if current { "new" } else { "old" }
                ),
                128,
                1,
                || {
                    if current {
                        itl_mark_unlock_folders(
                            black_box(&mut new),
                            folders.iter().map(String::as_str),
                        )
                    } else {
                        baseline::itl_mark_unlock_folders(
                            black_box(&mut old),
                            folders.iter().map(String::as_str),
                        )
                    }
                },
            );
        }
    }
    for scenario in ["locked", "missing"] {
        let (data, folders) = unlock_fixture(1024, false);
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [true, false]
        } else {
            [false, true]
        };
        for current in order {
            crate::perf::measure_sampled_with_setup(
                &format!(
                    "itl-unlocks/count=1024/{scenario}/{}",
                    if current { "new" } else { "old" }
                ),
                256,
                1,
                || {
                    if scenario == "locked" {
                        data.clone()
                    } else {
                        ItlFileData::default()
                    }
                },
                |data| {
                    black_box(if current {
                        itl_mark_unlock_folders(black_box(data), folders.iter().map(String::as_str))
                    } else {
                        baseline::itl_mark_unlock_folders(
                            black_box(data),
                            folders.iter().map(String::as_str),
                        )
                    });
                },
            );
        }
    }
}
