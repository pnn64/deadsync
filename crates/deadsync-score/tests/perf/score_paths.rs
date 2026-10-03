use super::*;
use std::hint::black_box;

#[path = "score_paths_baseline.rs"]
mod baseline;

fn original_paths(paths: &baseline::OriginalScoreProfilePaths) -> [PathBuf; 8] {
    [
        paths.scores_dir(),
        paths.gs_dir(),
        paths.gs_chart_dir("abcdef"),
        paths.gs_index_path(),
        paths.ac_dir(),
        paths.ac_index_path(),
        paths.local_dir(),
        paths.local_index_path(),
    ]
}

fn current_paths(paths: &ScoreProfilePaths) -> [PathBuf; 8] {
    [
        paths.scores_dir(),
        paths.gs_dir(),
        paths.gs_chart_dir("abcdef"),
        paths.gs_index_path(),
        paths.ac_dir(),
        paths.ac_index_path(),
        paths.local_dir(),
        paths.local_index_path(),
    ]
}

#[test]
fn score_paths_preserve_exact_os_strings_and_rooted_shards() {
    let mut roots: Vec<PathBuf> = [
        "",
        ".",
        "profiles/player",
        "profiles/player/",
        "profiles\\player\\",
        "/",
        "C:",
        "C:\\",
        "C:/Profiles/日本 😀",
        "\\\\server\\share\\player",
        "\\\\?\\C:\\Profiles\\player",
        "\\\\?\\UNC\\server\\share\\player",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    roots.push(PathBuf::from("日本/long-player/".repeat(128)));
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        roots.push(
            std::ffi::OsString::from_wide(&[
                b'C' as u16,
                b':' as u16,
                b'\\' as u16,
                0xd800,
                0xdcff,
            ])
            .into(),
        );
        roots.push(std::ffi::OsString::from_wide(&[0xd800, b'/' as u16, b'P' as u16]).into());
    }
    for root in roots {
        let old = baseline::OriginalScoreProfilePaths::new(root.clone());
        let new = ScoreProfilePaths::new(root);
        assert_eq!(old.profile_dir().as_os_str(), new.profile_dir().as_os_str());
        for (old, new) in original_paths(&old).iter().zip(current_paths(&new)) {
            assert_eq!(old.as_os_str(), new.as_os_str());
        }
        for hash in ["", "a", "ab", "é", "日", "/a", "\\a", "..", "C:", "😀"] {
            assert_eq!(
                old.gs_chart_dir(hash).as_os_str(),
                new.gs_chart_dir(hash).as_os_str()
            );
        }
    }
    let old = baseline::OriginalScoreProfilePaths::new("Profiles/player");
    let new = ScoreProfilePaths::new("Profiles/player");
    crate::perf::assert_reduced_churn(
        || {
            black_box(original_paths(&old));
        },
        || {
            black_box(current_paths(&new));
        },
    );
}

#[test]
#[ignore = "manual original/current score path benchmarks; run in release"]
fn score_paths_benchmark() {
    for (name, root) in [
        ("empty", String::new()),
        ("profile", "C:\\DeadSync\\Profiles\\player-guid".to_string()),
        ("long", format!("C:\\{}", "日本-pack\\player\\".repeat(64))),
    ] {
        let old = baseline::OriginalScoreProfilePaths::new(&root);
        let new = ScoreProfilePaths::new(&root);
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [true, false]
        } else {
            [false, true]
        };
        for current in order {
            let variant = if current { "new" } else { "old" };
            crate::perf::measure_sampled(
                &format!("score-paths/root={name}/bundle/{variant}"),
                1024,
                8,
                || {
                    if current {
                        current_paths(black_box(&new))
                    } else {
                        original_paths(black_box(&old))
                    }
                },
            );
            crate::perf::measure_sampled(
                &format!("score-paths/root={name}/chart/{variant}"),
                4096,
                1,
                || {
                    if current {
                        new.gs_chart_dir(black_box("abcdef"))
                    } else {
                        old.gs_chart_dir(black_box("abcdef"))
                    }
                },
            );
        }
    }
}
