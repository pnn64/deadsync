use super::*;
use crate::{paired_bench, perf};
use std::hint::black_box;

#[path = "beginner_verdict_original.rs"]
mod original;

fn verdicts(count: usize) -> HashMap<u64, bool> {
    (0..count)
        .map(|i| (u64::MAX - i as u64 * 7919, i % 3 == 0))
        .collect()
}

#[test]
fn verdict_json_preserves_all_u64_keys_and_boolean_values() {
    for count in [0, 1, 7, 60, 1200] {
        let mut values = verdicts(count);
        values.extend([(0, false), (u32::MAX as u64, true), (u64::MAX, false)]);
        let old: serde_json::Value =
            serde_json::from_str(&original::encode(&values).unwrap()).unwrap();
        let encoded = serde_json::to_string(&values).unwrap();
        let current: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(current, old);
        assert_eq!(
            serde_json::from_str::<HashMap<u64, bool>>(&encoded).unwrap(),
            values
        );
        assert_eq!(current["0"], false);
        assert_eq!(current["18446744073709551615"], false);
    }
    assert_eq!(
        serde_json::to_string(&HashMap::<u64, bool>::new()).unwrap(),
        "{}"
    );
}

#[test]
fn verdict_file_replacement_and_failures_match_the_original() {
    let root = std::env::temp_dir().join(format!("deadsync-verdict-perf-{}", std::process::id()));
    assert!(!root.exists(), "test requires its own fresh directory");
    std::fs::create_dir(&root).unwrap();
    let old = root.join("original/nested/verdicts.json");
    let current = root.join("current/nested/verdicts.json");
    for values in [verdicts(60), HashMap::new(), verdicts(1)] {
        original::write_verdicts(&old, &values).unwrap();
        write_verdicts(&current, &values).unwrap();
        assert_eq!(read_verdicts(&old), values);
        assert_eq!(read_verdicts(&current), values);
        assert!(!old.with_extension("tmp").exists());
        assert!(!current.with_extension("tmp").exists());
    }
    // A directory at the destination makes the rename fail, after the temp write.
    for (label, write) in [
        (
            "old-blocked",
            original::write_verdicts as fn(&Path, &HashMap<u64, bool>) -> Result<(), String>,
        ),
        ("new-blocked", write_verdicts),
    ] {
        let blocked = root.join(label);
        std::fs::create_dir(&blocked).unwrap();
        assert!(write(&blocked, &verdicts(3)).is_err());
        assert!(blocked.is_dir());
        assert!(!blocked.with_extension("tmp").exists());
        std::fs::remove_dir(blocked).unwrap();
    }
    // A file used as the parent fails before creating a temporary file.
    let blocked = root.join("file-parent");
    std::fs::write(&blocked, b"preserve me").unwrap();
    assert!(original::write_verdicts(&blocked.join("x.json"), &verdicts(1)).is_err());
    assert!(write_verdicts(&blocked.join("x.json"), &verdicts(1)).is_err());
    assert_eq!(std::fs::read(&blocked).unwrap(), b"preserve me");
    std::fs::remove_file(blocked).unwrap();
    for path in [old, current] {
        std::fs::remove_file(&path).unwrap();
        let parent = path.parent().unwrap();
        std::fs::remove_dir(parent).unwrap();
        std::fs::remove_dir(parent.parent().unwrap()).unwrap();
    }
    std::fs::remove_dir(root).unwrap();
}

#[test]
#[ignore = "paired release benchmark; serialization excludes filesystem latency"]
fn benchmark_verdict_serialization() {
    for count in [0, 1, 7, 60, 1200, 9000] {
        let values = verdicts(count);
        let (_, before) = perf::measure(|| original::encode(&values).unwrap());
        let (_, after) = perf::measure(|| serde_json::to_string(&values).unwrap());
        println!("verdict/{count} churn: {before:?} -> {after:?}");
        assert_eq!(after.allocs, 1);
        if count > 0 {
            assert!(after.allocs < before.allocs);
        }
        paired_bench::compare(
            &format!("verdict/{count}"),
            (100_000 / count.max(1)).max(20),
            |current| {
                let values = black_box(&values);
                black_box(if current {
                    serde_json::to_string(values).unwrap()
                } else {
                    original::encode(values).unwrap()
                });
            },
        );
    }
}
