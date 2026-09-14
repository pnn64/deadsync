use deadsync_profile::favorites_view::unicode_case_insensitive_cmp;
use deadsync_score::*;
use std::{
    cmp::Ordering,
    collections::HashMap,
    fs,
    hash::BuildHasher,
    hint::black_box,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering as AtomicOrdering},
};

#[path = "ui_preparation/baseline.rs"]
mod baseline;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn filter(bits: usize) -> SelectMusicScoreboxFilter {
    SelectMusicScoreboxFilter {
        itg: bits & 1 != 0,
        ex: bits & 2 != 0,
        hard_ex: bits & 4 != 0,
        tournaments: bits & 8 != 0,
    }
}

fn panes(count: usize, variant: &str) -> Vec<LeaderboardPane> {
    (0..count)
        .map(|i| LeaderboardPane {
            name: match variant {
                "tournaments" => format!("Tournament {i}"),
                "gs" => "GrooveStats".into(),
                _ => [
                    "GrooveStats",
                    "GrooveStats",
                    "ArrowCloud",
                    "SRPG",
                    "ITL",
                    "Other",
                    "srpg ex",
                    "\u{65e5}rPg",
                    "itl-RPG",
                ][i % 9]
                    .into(),
            },
            entries: Vec::new(),
            is_ex: i % 2 == 1 && variant != "tournaments",
            disabled: i % 3 == 1,
            personalized: i % 4 == 1,
            arrowcloud_kind: if variant == "mixed" && i % 7 == 2 {
                Some(
                    [
                        ArrowCloudPaneKind::Itg,
                        ArrowCloudPaneKind::Ex,
                        ArrowCloudPaneKind::HardEx,
                    ][i % 3],
                )
            } else {
                None
            },
        })
        .collect()
}

#[test]
fn pane_selection_preserves_filter_order_and_primary_identity() {
    let mut state = 71;
    for size in [0, 1, 2, 5, 8, 9, 16, 64] {
        for variant in ["mixed", "gs", "tournaments"] {
            let mut panes = panes(size, variant);
            for _ in 0..8 {
                for i in (1..size).rev() {
                    panes.swap(i, random(&mut state) as usize % (i + 1));
                }
                for bits in 0..16 {
                    let old = baseline::select_music_scorebox_filtered_panes(&panes, filter(bits));
                    let new = select_music_scorebox_pane_refs(&panes, filter(bits));
                    assert_eq!(old.len(), new.len());
                    for (a, b) in old.iter().zip(&new) {
                        assert!(std::ptr::eq(*a, *b));
                    }
                    for show_ex in [false, true] {
                        assert_eq!(
                            baseline::preferred_primary_scorebox_pane(&old, show_ex)
                                .map(|p| p as *const _),
                            preferred_primary_scorebox_pane(&new, show_ex).map(|p| p as *const _)
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn small_pane_lists_have_no_heap_churn_and_spill_once() {
    for size in 0..=8 {
        let panes = panes(size, "mixed");
        for bits in 0..16 {
            perf::assert_no_churn(|| {
                let refs = select_music_scorebox_pane_refs(black_box(&panes), filter(bits));
                black_box(preferred_primary_scorebox_pane(
                    black_box(&refs),
                    black_box(true),
                ));
            });
        }
    }
    for size in [9, 64] {
        let panes = panes(size, "mixed");
        perf::assert_churn_budget(1, size * std::mem::size_of::<&LeaderboardPane>(), || {
            black_box(select_music_scorebox_pane_refs(
                black_box(&panes),
                filter(15),
            ));
        });
    }
}

fn check_cmp(left: &str, right: &str) {
    assert_eq!(
        unicode_case_insensitive_cmp(left, right),
        baseline::unicode_case_insensitive_cmp(left, right),
        "{left:?}, {right:?}"
    );
}

#[test]
fn unicode_comparison_preserves_expansions_boundaries_and_sort_ties() {
    // Preserve the existing per-character mapping, including sigma's behavior;
    // str::to_lowercase has contextual rules and is not this API's baseline.
    let values = [
        "",
        "A",
        "a",
        "Alpha",
        "ALPHA",
        "Alpha0",
        "aLPhA1",
        "\0",
        "\u{7f}",
        "\u{80}",
        "\u{212a}",
        "k",
        "\u{130}",
        "i\u{307}",
        "\u{3a3}",
        "\u{3c2}",
        "\u{3c3}",
        "\u{1e9e}",
        "\u{df}",
        "\u{10400}",
        "\u{10428}",
        "\u{65e5}",
    ];
    for left in values {
        for right in values {
            check_cmp(left, right);
            check_cmp(
                &format!("Common ASCII Prefix {left}"),
                &format!("COMMON ascii prefix {right}"),
            );
        }
    }
    for byte in 0..=127u8 {
        for other in 0..=127u8 {
            check_cmp(
                std::str::from_utf8(&[byte]).unwrap(),
                std::str::from_utf8(&[other]).unwrap(),
            );
        }
    }
    let mut state = 391;
    let mut names = Vec::new();
    for _ in 0..10_000 {
        let mut make = || {
            (0..random(&mut state) % 32)
                .map(|_| char::from_u32((random(&mut state) % 0x110000) as u32).unwrap_or('A'))
                .collect::<String>()
        };
        let (a, b) = (make(), make());
        check_cmp(&a, &b);
        if names.len() < 1024 {
            names.extend([a, b]);
        }
    }
    for value in values {
        names.extend([value.into(), value.into()]);
    }
    assert_eq!(
        sorted_names(&names, baseline::unicode_case_insensitive_cmp),
        sorted_names(&names, unicode_case_insensitive_cmp)
    );
    perf::assert_no_churn(|| {
        for pair in names.windows(2) {
            black_box(unicode_case_insensitive_cmp(
                black_box(&pair[0]),
                black_box(&pair[1]),
            ));
        }
    });
}

fn sorted_names(names: &[String], compare: fn(&str, &str) -> Ordering) -> Vec<usize> {
    let mut order: Vec<_> = (0..names.len()).collect();
    order.sort_by(|&a, &b| compare(&names[a], &names[b]));
    order
}

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
struct Tree(PathBuf);
impl Tree {
    fn new(label: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let path = root.join(format!(
            "ui-preparation-{label}-{:010}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, AtomicOrdering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn history_tree(label: &str, shards: usize, per_shard: usize) -> Tree {
    let tree = Tree::new(label);
    for shard in 0..shards.max(1) {
        let dir = if shards == 0 {
            tree.0.clone()
        } else {
            tree.0.join(format!("{shard:02}"))
        };
        fs::create_dir_all(&dir).unwrap();
        for i in 0..per_shard {
            fs::write(
                dir.join(format!("chart{:03}-{}.bin", i % 32, shard * per_shard + i)),
                [],
            )
            .unwrap();
        }
    }
    tree
}

fn old_recent(dir: &Path) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    baseline::collect_recent_local_plays_in_root(dir, &mut out);
    out
}
fn new_recent(dir: &Path) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    collect_recent_local_plays_in_root(dir, &mut out);
    out
}
fn old_counts(dir: &Path) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    baseline::collect_local_play_counts_in_root(dir, &mut out);
    out
}
fn new_counts(dir: &Path) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    collect_local_play_counts_in_root(dir, &mut out);
    out
}

fn check_history(root: &Path) {
    assert_eq!(
        baseline::total_local_score_bins_in_root(root),
        total_local_score_bins_in_root(root)
    );
    let recent = old_recent(root);
    let counts = old_counts(root);
    assert_eq!(recent, new_recent(root));
    assert_eq!(counts, new_counts(root));
    let mut expected_recent: Vec<_> = recent.into_iter().collect();
    expected_recent.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let expected_recent: Vec<_> = expected_recent.into_iter().map(|(hash, _)| hash).collect();
    let mut expected_counts: Vec<_> = counts.into_iter().collect();
    expected_counts.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    assert_eq!(recent_played_chart_hashes_in_root(root), expected_recent);
    assert_eq!(played_chart_counts_in_root(root), expected_counts);
    assert_eq!(
        played_chart_history_in_root(root),
        PlayedChartHistory {
            recent_chart_hashes: expected_recent,
            played_chart_counts: expected_counts
        }
    );
}

#[test]
fn history_walk_preserves_depth_filename_rules_counts_and_existing_state() {
    let tree = history_tree("behavior", 3, 40);
    for name in [
        "root-42.bin",
        "root-99.bin",
        "root--1.bin",
        "root-0.BIN",
        "root-+1.bin",
        "root-9223372036854775808.bin",
        ".bin",
        "-1.bin",
        "file.",
        "plain",
        "\u{65e5}-12.bin",
    ] {
        fs::write(tree.0.join(name), []).unwrap();
    }
    let deep = tree.0.join("00/deeper");
    fs::create_dir(&deep).unwrap();
    fs::write(deep.join("must-not-count-7.bin"), []).unwrap();
    fs::create_dir(tree.0.join("directory-15.bin")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        fs::write(
            tree.0
                .join(std::ffi::OsStr::from_bytes(b"invalid\xff-15.bin")),
            [],
        )
        .unwrap();
    }
    check_history(&tree.0);
    check_history(&tree.0.join("plain"));
    check_history(&tree.0.join("absent"));
    let mut old = HashMap::from([("root".into(), u32::MAX - 1), ("unrelated".into(), 27)]);
    let mut new = old.clone();
    baseline::collect_local_play_counts_in_root(&tree.0, &mut old);
    collect_local_play_counts_in_root(&tree.0, &mut new);
    assert_eq!(old, new);
    let mut old = HashMap::from([("root".into(), i64::MAX), ("unrelated".into(), -7)]);
    let mut new = old.clone();
    baseline::collect_recent_local_plays_in_root(&tree.0, &mut old);
    collect_recent_local_plays_in_root(&tree.0, &mut new);
    assert_eq!(old, new);
}

#[test]
fn history_walk_follows_file_and_directory_links_one_level() {
    let tree = history_tree("links", 0, 2);
    let link = tree.0.join("link-88.bin");
    let target = tree.0.join("chart000-0.bin");
    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_file(&target, &link);
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(&target, &link);
    if let Err(error) = result {
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || (cfg!(windows) && error.raw_os_error() == Some(1314))
        {
            eprintln!("symlink fixture unavailable: {error}");
            return;
        }
        panic!("symlink fixture: {error}");
    }
    check_history(&tree.0);
    fs::remove_file(&target).unwrap();
    check_history(&tree.0);
    let shard = tree.0.join("shard");
    fs::create_dir(&shard).unwrap();
    fs::write(shard.join("linked-chart-3.bin"), []).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&shard, tree.0.join("alias")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&shard, tree.0.join("alias")).unwrap();
    check_history(&tree.0);
    assert_eq!(new_counts(&tree.0)["linked-chart"], 2);
}

#[test]
fn history_scan_reduces_allocation_churn() {
    let tree = history_tree("churn", 2, 64);
    perf::assert_reduced_churn(
        || {
            black_box(old_recent(&tree.0));
        },
        || {
            black_box(new_recent(&tree.0));
        },
    );
}

fn old_primary(panes: &[LeaderboardPane], filter: SelectMusicScoreboxFilter, ex: bool) -> usize {
    let refs = baseline::select_music_scorebox_filtered_panes(panes, filter);
    black_box(baseline::preferred_primary_scorebox_pane(
        black_box(&refs),
        ex,
    ))
    .map_or(0, |pane| pane as *const _ as usize)
}
fn new_primary(panes: &[LeaderboardPane], filter: SelectMusicScoreboxFilter, ex: bool) -> usize {
    let refs = select_music_scorebox_pane_refs(panes, filter);
    black_box(preferred_primary_scorebox_pane(black_box(&refs), ex))
        .map_or(0, |pane| pane as *const _ as usize)
}
fn variants() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    }
}

#[test]
#[ignore = "manual release benchmark; run alone on a pinned CPU"]
fn benchmark_ui_preparation() {
    for (count, variant, bits) in [
        (0, "empty", 15),
        (1, "gs", 15),
        (5, "mixed", 15),
        (8, "tournaments", 15),
        (8, "mixed", 0),
        (9, "mixed", 15),
        (64, "tournaments", 15),
    ] {
        let panes = panes(count, variant);
        for new in variants() {
            let function = black_box(if new { new_primary } else { old_primary }
                as fn(&[LeaderboardPane], SelectMusicScoreboxFilter, bool) -> usize);
            perf::measure_sampled(
                &format!(
                    "panes/{count}-{variant}-{bits}/{}",
                    if new { "new" } else { "old" }
                ),
                10_000,
                1,
                || function(black_box(&panes), black_box(filter(bits)), black_box(true)),
            );
        }
    }
    for (label, a, b) in [
        ("ascii-early", "Alpha", "Beta"),
        (
            "ascii-prefix",
            "Controller Connection 1234",
            "CONTROLLER connection 1235",
        ),
        ("equal", "Profile ABC", "PROFILE abc"),
        ("unicode-first", "\u{130}stanbul", "istanbul"),
        ("unicode-late", "Profile \u{212a}ey", "PROFILE key"),
        ("empty", "", ""),
    ] {
        for new in variants() {
            let function = black_box(if new {
                unicode_case_insensitive_cmp
            } else {
                baseline::unicode_case_insensitive_cmp
            } as fn(&str, &str) -> Ordering);
            perf::measure_sampled(
                &format!("compare/{label}/{}", if new { "new" } else { "old" }),
                50_000,
                1,
                || function(black_box(a), black_box(b)),
            );
        }
    }
    for label in ["ascii", "mixed", "unicode"] {
        let names: Vec<_> = (0..1024)
            .map(|i| match label {
                "mixed" if i % 3 == 0 => format!("Profile \u{130} {i:04}"),
                "unicode" => format!("\u{65e5}\u{130}\u{3a3} {:04}", (i * 337) % 1024),
                _ => format!("Profile {:04}", (i * 337) % 1024),
            })
            .collect();
        for new in variants() {
            let compare = black_box(if new {
                unicode_case_insensitive_cmp
            } else {
                baseline::unicode_case_insensitive_cmp
            } as fn(&str, &str) -> Ordering);
            perf::measure_sampled(
                &format!("sort/{label}/{}", if new { "new" } else { "old" }),
                32,
                names.len(),
                || sorted_names(black_box(&names), compare),
            );
        }
    }
    for (label, shards, count) in [("flat", 0, 512), ("sharded", 4, 128), ("empty", 0, 0)] {
        let tree = history_tree(label, shards, count);
        for new in variants() {
            let function = black_box(
                if new { new_recent } else { old_recent } as fn(&Path) -> HashMap<String, i64>
            );
            perf::measure_sampled(
                &format!("history/{label}/{}", if new { "new" } else { "old" }),
                4,
                (shards.max(1) * count).max(1),
                || function(black_box(&tree.0)),
            );
        }
        for new in variants() {
            let function = black_box(if new {
                total_local_score_bins_in_root
            } else {
                baseline::total_local_score_bins_in_root
            } as fn(&Path) -> u32);
            perf::measure_sampled(
                &format!("count/{label}/{}", if new { "new" } else { "old" }),
                4,
                (shards.max(1) * count).max(1),
                || function(black_box(&tree.0)),
            );
        }
    }
}
