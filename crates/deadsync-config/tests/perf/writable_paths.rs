use super::*;
use crate::perf::{assert_reduced_churn, measure_sampled};
use std::hint::black_box;
use std::path::PathBuf;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/writable_paths/baseline.rs"
    ));
}

fn fixture() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/perf-1706-paths");
    std::fs::create_dir_all(root.join("exists/Pack")).unwrap();
    std::fs::write(root.join("exists/Pack/song.ssc"), b"#TITLE:test;").unwrap();
    assert!(!root.join("missing").exists());
    root
}

fn folder(path: &Path, writable: bool) -> AdditionalSongFolder {
    AdditionalSongFolder {
        path: path.to_str().unwrap().to_owned(),
        writable,
    }
}

#[test]
fn writable_paths_preserve_longest_root_ties_and_filesystem_resolution() {
    let root = fixture();
    let exists = root.join("exists/Pack/song.ssc");
    let missing = root.join("missing/Pack/song.ssc");
    let paths = [
        exists.clone(),
        missing.clone(),
        root.join("exists/../exists/Pack/song.ssc"),
        root.join("missing2/Pack/song.ssc"),
        PathBuf::from(""),
        PathBuf::from("bad\0path"),
    ];
    for path in &paths {
        for flags in 0..16 {
            let roots = [
                root.join("exists"),
                root.join("exists/Pack"),
                root.join("missing"),
                root.join("missing/Pack"),
            ]
            .iter()
            .enumerate()
            .map(|(i, path)| folder(path, flags & (1 << i) != 0))
            .collect::<Vec<_>>();
            for length in 0..=roots.len() {
                assert_eq!(
                    baseline::song_path_is_writable_for_roots(path, &roots[..length]),
                    song_path_is_writable_for_roots(path, &roots[..length]),
                    "{path:?} flags={flags} length={length}"
                );
            }
        }
    }
    for path in [exists, missing] {
        let parent = path.parent().unwrap();
        for flags in 0..4 {
            let roots = [
                folder(parent, flags & 1 != 0),
                folder(parent, flags & 2 != 0),
            ];
            assert_eq!(
                song_path_is_writable_for_roots(&path, &roots),
                flags & 2 != 0
            );
            assert_eq!(
                baseline::song_path_is_writable_for_roots(&path, &roots),
                flags & 2 != 0
            );
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let path: PathBuf =
            std::ffi::OsString::from_wide(&[b'C' as u16, 58, 92, 0xd800, 92, 0x65e5]).into();
        for roots in [vec![], vec![folder(&root.join("missing"), false)]] {
            assert_eq!(
                baseline::song_path_is_writable_for_roots(&path, &roots),
                song_path_is_writable_for_roots(&path, &roots)
            );
        }
    }
}

#[test]
fn writable_paths_remove_unneeded_canonicalization_and_raw_path_copies() {
    let root = fixture();
    let path = root.join("missing/Pack/song.ssc");
    for count in [0, 1, 8, 64] {
        let roots = (0..count)
            .map(|i| folder(&root.join(format!("missing/Pack{i}")), false))
            .collect::<Vec<_>>();
        assert_reduced_churn(
            || {
                black_box(baseline::song_path_is_writable_for_roots(
                    black_box(&path),
                    black_box(&roots),
                ));
            },
            || {
                black_box(song_path_is_writable_for_roots(
                    black_box(&path),
                    black_box(&roots),
                ));
            },
        );
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn writable_paths_benchmark() {
    let root = fixture();
    let exists = root.join("exists/Pack/song.ssc");
    let missing = root.join("missing/Pack/song.ssc");
    let original = black_box(
        baseline::song_path_is_writable_for_roots as fn(&Path, &[AdditionalSongFolder]) -> bool,
    );
    let current =
        black_box(song_path_is_writable_for_roots as fn(&Path, &[AdditionalSongFolder]) -> bool);
    for (name, path, roots) in [
        ("empty-existing", exists.clone(), vec![]),
        ("empty-missing", missing.clone(), vec![]),
        (
            "existing-one",
            exists.clone(),
            vec![folder(&root.join("exists"), false)],
        ),
        (
            "existing-eight",
            exists,
            (0..8)
                .map(|_| folder(&root.join("exists"), false))
                .collect(),
        ),
        (
            "missing-one",
            missing.clone(),
            vec![folder(&root.join("missing"), false)],
        ),
        (
            "missing-eight",
            missing.clone(),
            (0..8)
                .map(|i| folder(&root.join(format!("missing/Pack{i}")), false))
                .collect(),
        ),
        (
            "missing-sixty-four",
            missing,
            (0..64)
                .map(|i| folder(&root.join(format!("missing/Pack{i}")), false))
                .collect(),
        ),
    ] {
        let run = |variant: &str, f: fn(&Path, &[AdditionalSongFolder]) -> bool| {
            measure_sampled(
                &format!("writable-paths/{name}/{variant}"),
                if roots.len() > 8 { 64 } else { 1024 },
                1,
                || f(black_box(&path), black_box(&roots)),
            );
        };
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            run("current", current);
            run("original", original);
        } else {
            run("original", original);
            run("current", current);
        }
    }
}
