//! Workshop manifest parity, including ordering, Unicode and error paths.
use super::*;
use std::hint::black_box;
#[path = "../../../tests/support/perf.rs"]
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
    "/../../tests/perf/direct_data_original_workshop.rs"
));

fn manifest(result: Result<Vec<Choice>, Error>) -> Result<String, String> {
    result
        .map(|choices| serde_json::to_string(&choices).unwrap())
        .map_err(|e| e.to_string())
}

fn fixture(groups: usize, files_per_group: usize) -> Vec<PathBuf> {
    (0..groups).flat_map(|group| (0..files_per_group).map(move |file| {
        let category = ["Arrows", "Receptors", "Holds", "Rolls", "Tap Explosions", "Hold Explosions", "Mines", "Mine Size", "Lifts"][group % 9];
        let label = if group % 7 == 0 { "RGB" } else { "DDR Vivid \u{66f2}" };
        let state = if file % 2 == 0 { "Inactive" } else { "Active" };
        Path::new("fixture").join(format!("Customizations/{category}/Cel - {label} {group:04}/Subfolder/tap {file} {state}.png"))
    })).collect()
}

#[test]
fn workshop_choices_match_original_including_errors_and_order() {
    let root = Path::new("fixture");
    let mut files = fixture(90, 8);
    files.extend(
        [
            "Customizations/Arrows/Metal - Exclusive/file.png",
            "Customizations/Arrows/ Cel - RGB /file.png",
            "Customizations/Holds/Shared/\u{130}NACTIVE.png",
            "Customizations/Rolls/Shared/INACTIVE.png",
        ]
        .map(|p| root.join(p)),
    );
    for family in ["Cel", "Metal", ""] {
        for input in [
            files.clone(),
            files.iter().rev().cloned().collect(),
            Vec::new(),
        ] {
            assert_eq!(
                manifest(choices_for(&input, root, family)),
                manifest(original_choices_for(&input, root, family))
            );
        }
    }
    for names in [
        vec![
            "Customizations/Arrows/Same!/a.png",
            "Customizations/Arrows/Same?/b.png",
        ],
        vec!["Customizations/Unknown/choice/a.png"],
        vec!["Customizations/Arrows/file.png"],
        vec!["Customizations"],
        vec!["../outside.png"],
    ] {
        let input: Vec<_> = names.iter().map(|p| root.join(p)).collect();
        assert_eq!(
            manifest(choices_for(&input, root, "Cel")),
            manifest(original_choices_for(&input, root, "Cel"))
        );
    }
}

#[test]
fn workshop_grouping_avoids_per_file_owned_slot_and_duplicate_id_keys() {
    let files = fixture(90, 8);
    let (before_result, before) =
        support::measure(|| original_choices_for(&files, Path::new("fixture"), "Cel"));
    let (after_result, after) =
        support::measure(|| choices_for(&files, Path::new("fixture"), "Cel"));
    assert_eq!(manifest(before_result), manifest(after_result));
    // At least the slot key per file and duplicate ID keys for existing groups.
    assert!(before.allocs - after.allocs >= files.len());
    assert!(after.allocated_bytes < before.allocated_bytes);
}

#[test]
#[ignore = "paired release benchmark; run alone"]
fn benchmark_direct_data_workshop() {
    for (label, groups, per_group) in [
        ("workshop/empty", 0, 0),
        ("workshop/single-file-groups", 90, 1),
        ("workshop/720-files", 90, 8),
        ("workshop/4096-files", 512, 8),
    ] {
        let files = fixture(groups, per_group);
        support::compare(
            label,
            || {
                black_box(
                    original_choices_for(
                        black_box(&files),
                        black_box(Path::new("fixture")),
                        black_box("Cel"),
                    )
                    .unwrap(),
                );
            },
            || {
                black_box(
                    choices_for(
                        black_box(&files),
                        black_box(Path::new("fixture")),
                        black_box("Cel"),
                    )
                    .unwrap(),
                );
            },
        );
    }
    for (label, foreign_percent) in [
        ("workshop/mixed-family-4096", 50),
        ("workshop/other-family-4096", 100),
    ] {
        let files: Vec<_> = fixture(512, 8)
            .into_iter()
            .enumerate()
            .map(|(index, path)| {
                if index % 100 < foreign_percent {
                    PathBuf::from(path.to_str().unwrap().replace("Cel", "Metal"))
                } else {
                    path
                }
            })
            .collect();
        support::compare(
            label,
            || {
                black_box(
                    original_choices_for(
                        black_box(&files),
                        black_box(Path::new("fixture")),
                        black_box("Cel"),
                    )
                    .unwrap(),
                );
            },
            || {
                black_box(
                    choices_for(
                        black_box(&files),
                        black_box(Path::new("fixture")),
                        black_box("Cel"),
                    )
                    .unwrap(),
                );
            },
        );
    }
}

#[test]
fn rejected_workshop_family_does_not_allocate_normalized_paths() {
    let files: Vec<_> = fixture(90, 8)
        .into_iter()
        .map(|p| PathBuf::from(p.to_str().unwrap().replace("Cel", "Metal")))
        .collect();
    let (old, before) =
        support::measure(|| original_choices_for(&files, Path::new("fixture"), "Cel"));
    let (new, after) = support::measure(|| choices_for(&files, Path::new("fixture"), "Cel"));
    assert_eq!(manifest(old), manifest(new));
    assert_eq!(before.allocs, files.len());
    assert_eq!(after.allocs, 0);
    assert_eq!(after.reallocs, 0);
}

#[test]
fn workshop_path_normalization_preserves_mixed_separators_and_errors() {
    let root = Path::new("fixture");
    for separator in ["/", "\\", "/\\", "//"] {
        for category in ["Arrows", "Holds", "Rolls", "Unknown", ""] {
            for folder in [
                "Cel - RGB",
                "Metal - Other",
                "\u{130} mixed \u{66f2}",
                "",
                "Cel/Sub",
            ] {
                let path =
                    root.join(["Customizations", category, folder, "INACTIVE.png"].join(separator));
                for family in ["Cel", "Metal"] {
                    assert_eq!(
                        manifest(choices_for(std::slice::from_ref(&path), root, family)),
                        manifest(original_choices_for(
                            std::slice::from_ref(&path),
                            root,
                            family
                        )),
                        "{path:?}"
                    );
                }
            }
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let path = root.join(std::ffi::OsString::from_wide(&[0xd800]));
        assert_eq!(
            manifest(choices_for(std::slice::from_ref(&path), root, "Cel")),
            manifest(original_choices_for(
                std::slice::from_ref(&path),
                root,
                "Cel"
            ))
        );
    }
}
