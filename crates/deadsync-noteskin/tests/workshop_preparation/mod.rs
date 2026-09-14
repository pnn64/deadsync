use super::*;
use crate::perf;
use std::hint::black_box;

mod baseline;

fn signature(result: Result<Vec<Choice>, Error>) -> Result<Vec<u8>, String> {
    result
        .map(|choices| serde_json::to_vec(&choices).unwrap())
        .map_err(|error| format!("{error:?}: {error}"))
}

fn paths(root: &Path, count: usize, depth: usize) -> Vec<PathBuf> {
    (0..count)
        .flat_map(|index| {
            let category = ["Arrows", "Holds", "Mines", "Receptors"][index % 4];
            let folders = "Cel - Nested /".repeat(depth);
            ["active", "inactive"].map(move |state| {
                root.join(format!(
                    "Customizations/{category}/{folders}Cel - Style {index}/note {state}.png"
                ))
            })
        })
        .collect()
}

#[test]
fn streamed_slugs_preserve_unicode_lowercase_expansions_and_separators() {
    for label in [
        "",
        "--",
        "A / B---",
        "İ RGB",
        "K-ΣΟΣ",
        "AΣA",
        "a\0b",
        " Metal Cel ",
    ] {
        let actual = slug(label);
        assert_eq!(actual, baseline::slug(label));
        assert_eq!(actual.capacity(), actual.len());
    }
    // Every Unicode scalar, inside ASCII context, catches lowercase expansions
    // and context-sensitive casing that could affect the portable identifier.
    let mut label = String::new();
    for scalar in 0..=0x10ffff {
        let Some(ch) = char::from_u32(scalar) else {
            continue;
        };
        label.clear();
        label.push('A');
        label.push(ch);
        label.push('Z');
        let actual = slug(&label);
        assert_eq!(actual, baseline::slug(&label), "U+{scalar:04X}");
        assert_eq!(actual.capacity(), actual.len());
    }
}

#[test]
fn streamed_choices_preserve_complete_manifests_and_errors() {
    let root = Path::new("workshop-fixture");
    for count in [0, 1, 8, 64, 512] {
        for depth in [0, 1, 4] {
            let files = paths(root, count, depth);
            for family in ["Cel", "Metal"] {
                assert_eq!(
                    signature(choices_for(&files, root, family)),
                    signature(baseline::choices_for(&files, root, family))
                );
            }
        }
    }
    for names in [
        vec!["Customizations/Arrows/Cel - RGB/file.png"],
        vec!["Customizations/Arrows/Cel - DDR VIVID/file.png"],
        vec!["Customizations/Holds/Cel/---/ İ K Σ /file INACTIVE.png"],
        vec!["Customizations/Rolls/Metal and Cel/file.png"],
        vec![
            "Customizations/Arrows/a/file.png",
            "Customizations/Arrows/A/file.png",
        ],
        vec![
            "Customizations/Arrows/a/file.png",
            "Customizations/Arrows/a/second.png",
        ],
        vec![
            "Customizations/Arrows/A-b/file.png",
            "Customizations/Arrows/A b/file.png",
        ],
        vec!["Customizations/Unknown/Metal/file.png"],
        vec!["Customizations/Unknown/Cel/file.png"],
        vec!["Customizations/Arrows/file.png"],
        vec!["Customizations/Arrows//file.png"],
        vec!["Customizations/Arrows/ CelCel-- /file.png"],
    ] {
        let files: Vec<_> = names.into_iter().map(|name| root.join(name)).collect();
        for family in ["Cel", "Metal", ""] {
            assert_eq!(
                signature(choices_for(&files, root, family)),
                signature(baseline::choices_for(&files, root, family))
            );
        }
    }
    let outside = [PathBuf::from("elsewhere/file.png")];
    assert_eq!(
        signature(choices_for(&outside, root, "Cel")),
        signature(baseline::choices_for(&outside, root, "Cel"))
    );
    let files = paths(root, 512, 4);
    perf::assert_reduced_churn(
        || {
            black_box(baseline::choices_for(&files, root, "Cel").unwrap());
        },
        || {
            black_box(choices_for(&files, root, "Cel").unwrap());
        },
    );
}

fn compare<T>(name: &str, units: usize, old: impl FnMut() -> T, new: impl FnMut() -> T) {
    let iterations = if units == 1 { 4096 } else { 16 };
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        perf::measure_sampled(&format!("{name}/new"), iterations, units, new);
        perf::measure_sampled(&format!("{name}/old"), iterations, units, old);
    } else {
        perf::measure_sampled(&format!("{name}/old"), iterations, units, old);
        perf::measure_sampled(&format!("{name}/new"), iterations, units, new);
    }
}

#[test]
#[ignore = "manual release before/after CPU and allocation benchmark"]
fn benchmark_workshop_preparation() {
    let root = Path::new("workshop-fixture");
    let old = black_box(
        baseline::choices_for as fn(&[PathBuf], &Path, &str) -> Result<Vec<Choice>, Error>,
    );
    let new = black_box(choices_for as fn(&[PathBuf], &Path, &str) -> Result<Vec<Choice>, Error>);
    for (count, depth) in [(0, 0), (1, 0), (64, 0), (512, 0), (512, 4)] {
        let files = paths(root, count, depth);
        assert_eq!(
            signature(old(&files, root, "Cel")),
            signature(new(&files, root, "Cel"))
        );
        compare(
            &format!("workshop_choices/{count}x{depth}"),
            files.len().max(1),
            || old(black_box(&files), root, "Cel").unwrap(),
            || new(black_box(&files), root, "Cel").unwrap(),
        );
    }
}
