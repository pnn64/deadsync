use super::*;
use crate::{GsScoreEntryV1, cached_score_from_gs_entry, decode_gs_score_entry_ref, perf};
use std::{
    hint::black_box,
    sync::atomic::{AtomicU64, Ordering as AtomicOrdering},
};

mod baseline;
mod fixtures;

const HASH: &str = "0123456789abcdef";
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

struct Tree(PathBuf);
impl Tree {
    fn new(label: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        // A custom CARGO_TARGET_DIR may leave the workspace target absent.
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let path = root.join(format!(
            "score-storage-{label}-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, AtomicOrdering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        // This absolute child was created exclusively by this fixture.
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn gs(username: &str) -> GsScoreEntry {
    GsScoreEntry {
        score_percent: 0.975,
        grade_code: 3,
        lamp_index: Some(2),
        lamp_judge_count: Some(4),
        username: username.into(),
        fetched_at_ms: 1_700_000_000_000,
    }
}

fn wire(entry: &GsScoreEntry, legacy: bool) -> Vec<u8> {
    if legacy {
        bincode::encode_to_vec(
            GsScoreEntryV1 {
                score_percent: entry.score_percent,
                grade_code: entry.grade_code,
                lamp_index: entry.lamp_index,
                username: entry.username.clone(),
                fetched_at_ms: entry.fetched_at_ms,
            },
            bincode::config::standard(),
        )
        .unwrap()
    } else {
        encode_gs_score_entry(entry).unwrap()
    }
}

fn assert_entry(a: &GsScoreEntry, b: &GsScoreEntry) {
    assert_eq!(a.score_percent.to_bits(), b.score_percent.to_bits());
    assert_eq!(
        (
            a.grade_code,
            a.lamp_index,
            a.lamp_judge_count,
            &a.username,
            a.fetched_at_ms
        ),
        (
            b.grade_code,
            b.lamp_index,
            b.lamp_judge_count,
            &b.username,
            b.fetched_at_ms
        )
    );
}

fn check_decode(bytes: &[u8]) {
    let old = baseline::decode_bounded(bytes);
    let new = crate::decode_gs_score_entry(bytes);
    let borrowed = decode_gs_score_entry_ref(bytes);
    assert_eq!(old.is_some(), new.is_some(), "wire {bytes:?}");
    assert_eq!(old.is_some(), borrowed.is_some());
    if let Some(old) = old {
        assert_entry(&old, new.as_ref().unwrap());
        let borrowed = borrowed.unwrap();
        if !borrowed.username.is_empty() {
            let offset = borrowed.username.as_ptr() as usize - bytes.as_ptr() as usize;
            assert!(offset + borrowed.username.len() <= bytes.len());
        }
        assert_entry(&old, &borrowed.into_owned());
    }
    perf::assert_no_churn(|| {
        black_box(decode_gs_score_entry_ref(black_box(bytes)));
    });
}

#[test]
fn borrowed_wire_preserves_legacy_current_truncation_and_corruption() {
    for username in [
        "",
        "A",
        "Alice",
        "  alice  ",
        "Stra\u{df}e\u{65e5}",
        &"x".repeat(4096),
    ] {
        for legacy in [false, true] {
            for value in [0.0, -0.0, 0.975, -1.0, f64::NAN, f64::INFINITY] {
                let mut entry = gs(username);
                entry.score_percent = value;
                for lamp in [None, Some(0), Some(255)] {
                    entry.lamp_index = lamp;
                    entry.lamp_judge_count = lamp;
                    let bytes = wire(&entry, legacy);
                    check_decode(&bytes);
                    // Every short-record prefix; boundaries for long usernames.
                    for end in (0..bytes.len())
                        .filter(|end| bytes.len() < 128 || *end < 32 || *end > bytes.len() - 16)
                    {
                        check_decode(&bytes[..end]);
                    }
                    let mut trailing = bytes.clone();
                    trailing.extend_from_slice(&[0xff, 0, 1, 2]);
                    check_decode(&trailing);
                }
            }
        }
    }
    let bytes = wire(&gs("Alice"), false);
    for i in 0..bytes.len() {
        for value in [0, 1, 2, 127, 128, 251, 252, 253, 254, 255] {
            let mut mutated = bytes.clone();
            mutated[i] = value;
            check_decode(&mutated);
        }
    }
    let mut random = 13u64;
    for len in 0..128 {
        for _ in 0..16 {
            let bytes: Vec<_> = (0..len)
                .map(|_| {
                    random ^= random << 13;
                    random ^= random >> 7;
                    random ^= random << 17;
                    random as u8
                })
                .collect();
            check_decode(&bytes);
        }
    }
}

fn leaderboard(dir: &Path, hash: &str, max: usize) -> Vec<LocalLeaderboardCandidate<'static>> {
    let mut out = Vec::new();
    push_local_leaderboard_candidates_from_dir(
        dir,
        hash,
        "Player",
        Some("Tag"),
        max,
        &mut Vec::with_capacity(1024),
        &mut 7,
        &mut out,
    );
    out
}
fn replay(dir: &Path, hash: &str) -> Vec<LocalReplayCandidate<'static>> {
    let mut out = Vec::new();
    push_local_replay_candidates_from_dir(
        dir,
        hash,
        "AAA",
        &mut Vec::with_capacity(1024),
        &mut 7,
        &mut out,
    );
    out
}
fn cached(bytes: &[u8]) -> Option<CachedScore> {
    decode_gs_score_entry_ref(bytes).map(|entry| entry.cached_score())
}

fn check_local(dir: &Path, hash: &str) {
    for limit in [0, 1, 3, 10, usize::MAX] {
        let old = baseline::leaderboard(dir, hash, limit);
        let new = leaderboard(dir, hash, limit);
        assert_eq!(old.len(), new.len());
        for (a, b) in old.iter().zip(&new) {
            assert_eq!(
                (
                    a.name,
                    a.machine_tag,
                    a.score_percent.to_bits(),
                    a.played_at_ms,
                    a.is_fail,
                    a.ordinal
                ),
                (
                    b.name,
                    b.machine_tag,
                    b.score_percent.to_bits(),
                    b.played_at_ms,
                    b.is_fail,
                    b.ordinal
                )
            );
        }
    }
    let old = baseline::replay(dir, hash);
    let new = replay(dir, hash);
    assert_eq!(old.len(), new.len());
    for (a, b) in old.iter().zip(&new) {
        assert_eq!(
            (
                a.initials,
                &a.path,
                a.score_percent.to_bits(),
                a.played_at_ms,
                a.ordinal
            ),
            (
                b.initials,
                &b.path,
                b.score_percent.to_bits(),
                b.played_at_ms,
                b.ordinal
            )
        );
    }
    let (mut old, mut new) = (Vec::new(), Vec::new());
    baseline::push_local_leaderboard_plays_from_dir(dir, hash, "Player", Some("Tag"), &mut old);
    push_local_leaderboard_plays_from_dir(dir, hash, "Player", Some("Tag"), &mut new);
    assert_eq!(format!("{old:?}"), format!("{new:?}"));
    let (mut old, mut new) = (Vec::new(), Vec::new());
    baseline::push_local_replay_plays_from_dir(dir, hash, "AAA", &mut old);
    push_local_replay_plays_from_dir(dir, hash, "AAA", &mut new);
    assert_eq!(format!("{old:?}"), format!("{new:?}"));
}

fn local_tree(label: &str, count: usize, matching: usize) -> Tree {
    let tree = Tree::new(label);
    for i in 0..count {
        fixtures::write_score(
            &tree.0,
            if i < matching { HASH } else { "other-chart" },
            1_700_000_000_000 + i as i64,
            0.9 + (i % 7) as f64 / 100.,
            i.is_multiple_of(11),
            8,
        );
    }
    tree
}

#[test]
fn local_search_preserves_filters_order_failures_and_directory_handling() {
    let tree = local_tree("local", 48, 24);
    let valid = encode_local_score_entry(&fixtures::score_entry(999, 0.99, false, 3)).unwrap();
    for name in [
        format!("{HASH}-999.bin"),
        format!("{HASH}-not-a-time.bin"),
        format!("{HASH}-1.BIN"),
        "-1.bin".into(),
        "unrelated".into(),
        format!("{HASH}-9223372036854775808.bin"),
    ] {
        fs::write(tree.0.join(name), &valid).unwrap();
    }
    fs::create_dir(tree.0.join(format!("{HASH}-0.bin"))).unwrap();
    fs::write(tree.0.join(format!("{HASH}-1.bin")), [255, 0]).unwrap();
    for hash in [
        HASH,
        "other-chart",
        "missing",
        "",
        "0123456789abcdef-",
        "\u{65e5}",
    ] {
        check_local(&tree.0, hash);
    }
    check_local(&tree.0.join("missing"), HASH);
    check_local(&tree.0.join("unrelated"), HASH);
}

#[test]
fn local_search_follows_file_symlinks_and_skips_broken_or_directory_links() {
    let tree = Tree::new("links");
    let target = fixtures::write_score(&tree.0, "target", 1, 0.98, false, 8);
    let link = tree.0.join(format!("{HASH}-2.bin"));
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
    assert_eq!(leaderboard(&tree.0, HASH, 10).len(), 1);
    check_local(&tree.0, HASH);
    fs::remove_file(&target).unwrap();
    assert!(leaderboard(&tree.0, HASH, 10).is_empty());
    check_local(&tree.0, HASH);
    let target_dir = tree.0.join("directory");
    fs::create_dir(&target_dir).unwrap();
    let dir_link = tree.0.join(format!("{HASH}-3.bin"));
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&target_dir, dir_link).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_dir, dir_link).unwrap();
    check_local(&tree.0, HASH);
}

#[test]
fn gs_index_preserves_legacy_records_invalid_files_and_best_score_selection() {
    let tree = Tree::new("index");
    let shard = tree.0.join("00");
    fs::create_dir(&shard).unwrap();
    for i in 0..96 {
        let mut entry = gs(if i % 2 == 0 { "Alice" } else { "\u{65e5}" });
        entry.score_percent = [0.0, 0.975, 0.95, 1.0, f64::NAN, f64::INFINITY][i % 6];
        entry.grade_code = [0, 3, 5, 255][i % 4];
        fs::write(
            shard.join(format!("chart{}-{i}.bin", i % 9)),
            wire(&entry, i % 3 == 0),
        )
        .unwrap();
    }
    for name in ["bad", "empty-1.bin", "-1.bin", "chart0-99.BIN"] {
        fs::write(shard.join(name), [255]).unwrap();
    }
    fs::create_dir(shard.join("chart0-100.bin")).unwrap();
    let old = baseline::best_gs_scores_from_dir(&tree.0);
    let new = best_gs_scores_from_dir(&tree.0);
    assert_eq!(old.len(), new.len());
    for (hash, a) in old {
        // Encoded fields distinguish signed zeros and NaN payloads.
        assert_eq!(
            bincode::encode_to_vec(a, bincode::config::standard()).unwrap(),
            bincode::encode_to_vec(new[&hash], bincode::config::standard()).unwrap()
        );
    }
}

fn write_result(result: Result<ScoreStoreWriteStatus, ScoreStoreWriteError>) -> String {
    match result {
        Ok(ScoreStoreWriteStatus::SkippedDuplicate) => "duplicate".into(),
        Ok(ScoreStoreWriteStatus::Written(path)) => format!(
            "written:{:?}:{:?}",
            path.file_name(),
            fs::read(&path).unwrap()
        ),
        Err(ScoreStoreWriteError::CreateDir { error, .. }) => format!("create:{:?}", error.kind()),
        Err(ScoreStoreWriteError::WriteFile { error, .. }) => format!("write:{:?}", error.kind()),
        Err(other) => panic!("unexpected write error: {other:?}"),
    }
}

#[test]
fn duplicate_stream_and_writer_preserve_username_epsilon_lamps_and_errors() {
    let old_tree = Tree::new("old-writer");
    let new_tree = Tree::new("new-writer");
    let entry = gs("Alice");
    for tree in [&old_tree, &new_tree] {
        for (name, bytes) in [
            (format!("{HASH}-1.bin"), wire(&entry, false)),
            (format!("{HASH}-legacy.bin"), wire(&gs("Legacy"), true)),
            (format!("{HASH}-broken.bin"), vec![255]),
            (format!("{HASH}suffix-1.bin"), wire(&gs("Other"), false)),
        ] {
            fs::write(tree.0.join(name), bytes).unwrap();
        }
        fs::write(
            tree.0.join(format!("{HASH}-long.bin")),
            wire(&gs(&"x".repeat(4096)), false),
        )
        .unwrap();
        fs::create_dir(tree.0.join(format!("{HASH}-directory.bin"))).unwrap();
        fs::write(tree.0.join("file"), []).unwrap();
    }
    for username in [
        "Alice",
        "ALICE",
        " alice ",
        "",
        " \u{2003}",
        "Legacy",
        "Other",
        "\u{65e5}",
    ] {
        for delta in [0.0, 0.5e-9, 1.0e-9, 2.0e-9, f64::NAN, f64::INFINITY] {
            for lamp in [None, Some(2), Some(255)] {
                let mut candidate = gs(username);
                candidate.score_percent += delta;
                candidate.lamp_judge_count = lamp;
                assert_eq!(
                    baseline::duplicate(&old_tree.0, HASH, &candidate),
                    gs_score_is_duplicate(&old_tree.0, HASH, &candidate)
                );
            }
        }
    }
    // Compare real writes, repeated imports, legacy deduplication, and failures.
    for (i, (username, hash, subdir, value, lamp, grade)) in [
        ("Alice", HASH, "", 0.975, Some(4), Grade::Tier03),
        ("ALICE", HASH, "", 0.975, Some(4), Grade::Tier03),
        ("Legacy", HASH, "", 0.975, None, Grade::Tier03),
        ("Other", HASH, "", 0.975, Some(4), Grade::Tier03),
        ("Alice", HASH, "", 0.975, Some(4), Grade::Failed),
        (" alice ", HASH, "", 0.975, Some(4), Grade::Tier03),
        ("", HASH, "absent", 0.975, Some(4), Grade::Tier03),
        (
            "Alice",
            HASH,
            "new-directory",
            0.975,
            Some(4),
            Grade::Tier03,
        ),
        ("Alice", HASH, "file", 0.975, Some(4), Grade::Tier03),
        ("Alice", "missing/child", "", 0.975, Some(4), Grade::Tier03),
    ]
    .into_iter()
    .enumerate()
    {
        let score = crate::cached_score(grade, value, Some(2), lamp);
        for _ in 0..2 {
            let old = baseline::write_gs_score_entry_file(
                &old_tree.0.join(subdir),
                hash,
                score,
                username,
                100 + i as i64,
            );
            let new = write_gs_score_entry_file(
                &new_tree.0.join(subdir),
                hash,
                score,
                username,
                100 + i as i64,
            );
            assert_eq!(write_result(old), write_result(new));
        }
    }
}

#[test]
fn duplicate_stream_preserves_prefix_matching_and_overwrites() {
    let old = Tree::new("old-prefix");
    let new = Tree::new("new-prefix");
    let score = crate::cached_score(Grade::Tier03, 0.975, Some(2), Some(4));
    for hash in ["", "chart", "chart-", "chart-long", "\u{65e5}"] {
        for tree in [&old, &new] {
            fs::write(
                tree.0.join(format!("{hash}-not-a-timestamp.bin")),
                wire(&gs("Alice"), false),
            )
            .unwrap();
        }
        for username in ["Alice", "alice", "Missing"] {
            let entry = gs(username);
            assert_eq!(
                baseline::duplicate(&old.0, hash, &entry),
                gs_score_is_duplicate(&old.0, hash, &entry)
            );
        }
        for username in ["First", "Replacement"] {
            assert_eq!(
                write_result(baseline::write_gs_score_entry_file(
                    &old.0, hash, score, username, 123
                )),
                write_result(write_gs_score_entry_file(
                    &new.0, hash, score, username, 123
                ))
            );
        }
    }
}

fn gs_tree(label: &str, count: usize) -> Tree {
    let tree = Tree::new(label);
    for i in 0..count {
        fs::write(
            tree.0.join(format!("{HASH}-{i:04}.bin")),
            wire(&gs(&format!("Player {i:04}")), i % 4 == 0),
        )
        .unwrap();
    }
    tree
}

#[test]
fn streaming_duplicate_reduces_churn_even_without_a_match() {
    let tree = gs_tree("churn", 32);
    let entry = gs("Missing");
    perf::assert_reduced_churn(
        || {
            assert!(!baseline::duplicate(&tree.0, HASH, &entry));
        },
        || {
            assert!(!gs_score_is_duplicate(&tree.0, HASH, &entry));
        },
    );
    let bytes = wire(&gs("Alice"), false);
    perf::assert_reduced_churn(
        || {
            black_box(baseline::cached(&bytes));
        },
        || {
            black_box(cached(&bytes));
        },
    );
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
fn benchmark_score_storage() {
    for (label, count, matching) in [
        ("selective", 512, 8),
        ("missing", 512, 0),
        ("all", 128, 128),
    ] {
        let tree = local_tree(label, count, matching);
        for new in variants() {
            let function = black_box(if new {
                leaderboard
            } else {
                baseline::leaderboard
            }
                as fn(&Path, &str, usize) -> Vec<LocalLeaderboardCandidate<'static>>);
            perf::measure_sampled(
                &format!("local/{label}/{}", if new { "new" } else { "old" }),
                8,
                count,
                || function(black_box(&tree.0), black_box(HASH), black_box(10)),
            );
        }
        if label == "selective" {
            for new in variants() {
                let function = black_box(if new { replay } else { baseline::replay }
                    as fn(&Path, &str) -> Vec<LocalReplayCandidate<'static>>);
                perf::measure_sampled(
                    &format!("local/replay/{}", if new { "new" } else { "old" }),
                    8,
                    count,
                    || function(black_box(&tree.0), black_box(HASH)),
                );
            }
        }
    }
    for (label, bytes) in [
        ("v2", wire(&gs("Alice"), false)),
        ("v1", wire(&gs("Alice"), true)),
        ("long", wire(&gs(&"x".repeat(4096)), false)),
        ("invalid", vec![255]),
    ] {
        for new in variants() {
            let function = black_box(
                if new { cached } else { baseline::cached } as fn(&[u8]) -> Option<CachedScore>
            );
            perf::measure_sampled(
                &format!("decode/{label}/{}", if new { "new" } else { "old" }),
                20_000,
                1,
                || function(black_box(&bytes)),
            );
        }
    }
    for (label, bytes) in [
        ("v2", wire(&gs("Alice"), false)),
        ("v1", wire(&gs("Alice"), true)),
    ] {
        for new in variants() {
            let function = black_box(if new {
                crate::decode_gs_score_entry
            } else {
                baseline::decode_gs_score_entry
            } as fn(&[u8]) -> Option<GsScoreEntry>);
            perf::measure_sampled(
                &format!("owned/{label}/{}", if new { "new" } else { "old" }),
                20_000,
                1,
                || function(black_box(&bytes)),
            );
        }
    }
    let index = Tree::new("bench-index");
    let shard = gs_tree("index-shard", 128);
    fs::rename(&shard.0, index.0.join("00")).unwrap();
    // Move the owned fixture back after the index benchmark so both drops are valid.
    for new in variants() {
        let function = black_box(if new {
            best_gs_scores_from_dir
        } else {
            baseline::best_gs_scores_from_dir
        } as fn(&Path) -> HashMap<String, CachedScore>);
        perf::measure_sampled(
            &format!("index/128/{}", if new { "new" } else { "old" }),
            8,
            128,
            || function(black_box(&index.0)),
        );
    }
    fs::rename(index.0.join("00"), &shard.0).unwrap();
    for (label, count, position) in [
        ("early", 128, Some(0)),
        ("late", 128, Some(127)),
        ("miss", 128, None),
        ("one-hit", 1, Some(0)),
        ("one-miss", 1, None),
        ("empty", 0, None),
    ] {
        let tree = gs_tree(label, count);
        let entry = gs("Alice");
        if let Some(position) = position {
            // Use actual enumeration order, independent of filesystem sorting.
            let path = fs::read_dir(&tree.0)
                .unwrap()
                .nth(position)
                .unwrap()
                .unwrap()
                .path();
            fs::write(path, wire(&entry, false)).unwrap();
        }
        assert_eq!(
            baseline::duplicate(&tree.0, HASH, &entry),
            position.is_some()
        );
        assert_eq!(
            gs_score_is_duplicate(&tree.0, HASH, &entry),
            position.is_some()
        );
        for new in variants() {
            let function = black_box(if new {
                gs_score_is_duplicate
            } else {
                baseline::duplicate
            } as fn(&Path, &str, &GsScoreEntry) -> bool);
            perf::measure_sampled(
                &format!("duplicate/{label}/{}", if new { "new" } else { "old" }),
                8,
                1,
                || function(black_box(&tree.0), black_box(HASH), black_box(&entry)),
            );
        }
    }
}
