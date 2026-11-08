use super::*;
use crate::metadata_perf::{compare_owned, measure};
use std::hint::black_box;
mod original {
    use super::*;
    include!("course_original.rs");
}

fn fixture(label: &str, count: usize, kind: &str) -> (PathBuf, Vec<PathBuf>) {
    let root = test_dir(label);
    fs::create_dir_all(root.join("Group")).unwrap();
    let paths = (0..count)
        .map(|i| {
            let path = root.join("Group").join(format!("Course-{i:04}.crs"));
            let kind = if kind == "mixed" {
                ["success", "missing", "references"][i % 3]
            } else {
                kind
            };
            match kind {
                "missing" => {}
                "references" => {
                    fs::write(&path, b"#COURSE:Invalid;#SONG:Missing/Song:Hard:;").unwrap()
                }
                _ => fs::write(&path, format!("#COURSE:Course {i};")).unwrap(),
            }
            path
        })
        .collect();
    (root, paths)
}

#[test]
fn course_results_and_progress_preserve_mixed_failures_and_input_order() {
    for kind in ["success", "missing", "references", "mixed"] {
        let (root, mut paths) = fixture(&format!("ownership-{kind}"), 6, kind);
        paths.reverse();
        paths.push(paths[0].clone());
        let mut before = Vec::new();
        let mut after = Vec::new();
        let old = original::load_course_paths_with_progress(
            paths.clone(),
            &root,
            &[],
            0,
            Some(&mut |done, total, group: &str, item: &str| {
                before.push((done, total, group.to_owned(), item.to_owned()))
            }),
        );
        let new = load_course_paths_with_progress(
            paths.clone(),
            &root,
            &[],
            0,
            Some(&mut |done, total, group: &str, item: &str| {
                after.push((done, total, group.to_owned(), item.to_owned()))
            }),
        );
        assert_eq!(format!("{old:?}"), format!("{new:?}"));
        assert_eq!(before, after);
        assert_eq!(after.len(), paths.len() + 1);
        assert_eq!(after[0], (0, paths.len(), String::new(), String::new()));
        for (i, path) in paths.iter().enumerate() {
            assert_eq!(
                after[i + 1],
                (
                    i + 1,
                    paths.len(),
                    "Group".to_owned(),
                    path.file_name().unwrap().to_str().unwrap().to_owned()
                )
            );
        }
        if kind == "references" {
            assert_eq!(new.failures.len(), paths.len());
            assert!(
                new.failures
                    .iter()
                    .all(|f| f.message.contains("missing song"))
            );
        }
        let no_progress = load_course_paths_with_progress::<fn(usize, usize, &str, &str)>(
            paths,
            &root,
            &[],
            0,
            None,
        );
        assert_eq!(format!("{new:?}"), format!("{no_progress:?}"));
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn course_callbacks_run_between_file_loads_and_for_empty_input() {
    let (root, paths) = fixture("ownership-callback-order", 2, "success");
    let mut reports = Vec::new();
    for old in [true, false] {
        fs::write(&paths[1], b"#COURSE:Before callback;").unwrap();
        let mut calls = Vec::new();
        let mut progress = |done, total, group: &str, item: &str| {
            calls.push((done, total, group.to_owned(), item.to_owned()));
            if done == 1 {
                fs::write(&paths[1], b"#COURSE:After callback;").unwrap();
            }
        };
        let report = if old {
            original::load_course_paths_with_progress(
                paths.clone(),
                &root,
                &[],
                0,
                Some(&mut progress),
            )
        } else {
            load_course_paths_with_progress(paths.clone(), &root, &[], 0, Some(&mut progress))
        };
        assert_eq!(report.courses[1].1.name, "After callback");
        reports.push((format!("{report:?}"), calls));
    }
    assert_eq!(reports[0], reports[1]);
    let mut calls = Vec::new();
    let empty = load_course_paths_with_progress(
        Vec::new(),
        &root,
        &[],
        0,
        Some(&mut |done, total, group: &str, item: &str| {
            calls.push((done, total, group.to_owned(), item.to_owned()))
        }),
    );
    assert!(empty.courses.is_empty() && empty.failures.is_empty());
    assert_eq!(calls, [(0, 0, String::new(), String::new())]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn course_progress_removes_two_owned_labels_per_input() {
    let (root, paths) = fixture("ownership-allocations", 64, "mixed");
    let input = paths.clone();
    let (_, a) = measure(|| {
        original::load_course_paths_with_progress::<fn(usize, usize, &str, &str)>(
            input,
            &root,
            &[],
            0,
            None,
        )
    });
    let expected_bytes: usize = paths
        .iter()
        .map(|p| {
            let (g, c) = course_progress_names(p, &root);
            g.len() + c.len()
        })
        .sum();
    let (_, b) = measure(|| {
        load_course_paths_with_progress::<fn(usize, usize, &str, &str)>(paths, &root, &[], 0, None)
    });
    assert_eq!(a.allocs, b.allocs + 128);
    assert_eq!(a.allocated_bytes, b.allocated_bytes + expected_bytes);
    fs::remove_dir_all(root).unwrap();
}

fn no_op_progress(done: usize, total: usize, group: &str, item: &str) {
    black_box((done, total, group, item));
}

#[test]
#[ignore = "paired release benchmark; run alone with --nocapture"]
fn benchmark_metadata_courses() {
    for (count, kind) in [
        (0, "success"),
        (1, "success"),
        (64, "success"),
        (64, "missing"),
        (64, "references"),
        (64, "mixed"),
    ] {
        let (root, paths) = fixture(&format!("ownership-bench-{count}-{kind}"), count, kind);
        for callback in [false, true] {
            let label = format!("courses/{count}/{kind}/{callback}");
            let mut old_progress = no_op_progress as fn(usize, usize, &str, &str);
            let mut new_progress = old_progress;
            let input = paths.clone();
            let (_, a) = measure(|| {
                original::load_course_paths_with_progress(
                    input,
                    &root,
                    &[],
                    0,
                    callback.then_some(&mut old_progress),
                )
            });
            let input = paths.clone();
            let (_, b) = measure(|| {
                load_course_paths_with_progress(
                    input,
                    &root,
                    &[],
                    0,
                    callback.then_some(&mut new_progress),
                )
            });
            println!("ALLOC {label}: original {a:?}, current {b:?}");
            compare_owned(
                &label,
                if count == 0 {
                    65536
                } else if count == 1 {
                    128
                } else {
                    4
                },
                &paths,
                |input| {
                    black_box(original::load_course_paths_with_progress(
                        black_box(input),
                        black_box(&root),
                        &[],
                        0,
                        callback.then_some(&mut old_progress),
                    ));
                },
                |input| {
                    black_box(load_course_paths_with_progress(
                        black_box(input),
                        black_box(&root),
                        &[],
                        0,
                        callback.then_some(&mut new_progress),
                    ));
                },
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}
