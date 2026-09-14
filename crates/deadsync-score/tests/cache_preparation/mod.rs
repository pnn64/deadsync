use super::*;
use crate::*;
use std::{
    hint::black_box,
    sync::atomic::{AtomicU64, Ordering as AtomicOrdering},
};
mod baseline;
#[path = "../score_storage/fixtures.rs"]
mod fixtures;

const HASH: &str = "0123456789abcdef";
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let path = root.join(format!(
            "cache-preparation-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, AtomicOrdering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        // Absolute workspace child created exclusively by this fixture.
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}
fn input(chart_type: &str) -> GrooveStatsEvalInput<'_> {
    GrooveStatsEvalInput {
        chart_type,
        music_rate: 1.0,
        remove_mask: 0,
        insert_mask: 0,
        holds_mask: 0,
        fail_type_ok: true,
        autoplay_used: false,
        is_course_mode: false,
        course_submit_allowed: false,
    }
}
fn assert_eval(input: GrooveStatsEvalInput<'_>) {
    let old = baseline::groovestats_eval_state_from_parts(input);
    let new = groovestats_eval_state_from_parts(input);
    assert_eq!(
        (old.valid, old.reason_lines, old.manual_qr_url),
        (new.valid, new.reason_lines, new.manual_qr_url)
    );
}
#[test]
fn eligibility_preserves_every_reason_combination_and_mixed_chart_types() {
    for bits in 0..(1 << GROOVESTATS_REASON_COUNT) {
        let checks = std::array::from_fn(|i| bits & (1 << i) != 0);
        assert_eq!(
            baseline::groovestats_reason_lines(&checks),
            groovestats_reason_lines(&checks)
        );
    }
    let mut state = 371;
    for chart_type in [
        "",
        "dance-single",
        " PUMP-Double ",
        "DANCE-SOLO",
        "dancesolo",
        "solodance",
        "pump-sOlO",
        "\u{2003}DaNcE-\u{65e5}\u{2003}",
        "\u{65e5}dance",
        "danc\u{e9}",
        "Dance-So\u{130}lo",
        "\0dance",
        "dance\0solo",
    ] {
        for _ in 0..128 {
            let r = random(&mut state);
            let mut v = input(chart_type);
            v.remove_mask = r as u8;
            v.insert_mask = (r >> 8) as u8;
            v.holds_mask = (r >> 16) as u8;
            v.fail_type_ok = r & (1 << 24) != 0;
            v.autoplay_used = r & (1 << 25) != 0;
            v.is_course_mode = r & (1 << 26) != 0;
            v.course_submit_allowed = r & (1 << 27) != 0;
            for rate in [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                -1.0,
                -0.0,
                0.99,
                1.0,
                3.0,
                3.01,
            ] {
                v.music_rate = rate;
                assert_eval(v);
            }
        }
    }
    for _ in 0..4096 {
        let text: String = (0..(random(&mut state) % 64))
            .map(|_| {
                [
                    'd', 'a', 'n', 'c', 'e', 'p', 'u', 'm', 's', 'o', 'l', 'D', 'A', 'N', 'C', 'E',
                    'P', 'U', 'M', 'S', 'O', 'L', '-', ' ', '\u{65e5}', '\u{130}', '\0',
                ][random(&mut state) as usize % 27]
            })
            .collect();
        assert_eval(input(&text));
    }
}
#[test]
fn valid_eligibility_has_no_heap_churn() {
    for text in ["dance-single", " PUMP-DOUBLE ", "dance-\u{65e5}"] {
        perf::assert_no_churn(|| {
            black_box(groovestats_eval_state_from_parts(black_box(input(text))));
        });
    }
    let checks = [false; GROOVESTATS_REASON_COUNT];
    perf::assert_churn_budget(13, 1200, || {
        black_box(groovestats_reason_lines(black_box(&checks)));
    });
}
fn key(i: usize, matching: bool) -> PlayerLeaderboardCacheKey {
    PlayerLeaderboardCacheKey {
        chart_hash: if matching {
            if i % 2 == 0 {
                HASH.into()
            } else {
                HASH.to_ascii_uppercase()
            }
        } else {
            format!("{i:016x}")
        },
        api_key: "gs-api".into(),
        arrowcloud_api_key: format!("ac-user-{i}"),
        include_arrowcloud: i % 2 == 0,
        show_ex_score: i % 3 == 0,
    }
}
fn cache(count: usize, matching: usize, mask: usize, now: Instant) -> PlayerLeaderboardCacheState {
    let mut out = PlayerLeaderboardCacheState::default();
    // Reserve tombstones to make reuse measurable without table growth noise.
    out.invalidated_after.reserve(count);
    for i in 0..count {
        let key = key(i, i < matching);
        if mask & 1 != 0 {
            out.by_key.insert(
                key.clone(),
                PlayerLeaderboardCacheEntry {
                    value: if i % 2 == 0 {
                        PlayerLeaderboardCacheValue::Error(Arc::from("cached error"))
                    } else {
                        PlayerLeaderboardCacheValue::Ready(Arc::new(PlayerLeaderboardData {
                            panes: Vec::new(),
                            srpg_self_score: Some(9700),
                            itl_self_score: None,
                            itl_self_rank: None,
                        }))
                    },
                    max_entries: i,
                    refreshed_at: now,
                    retry_after: Some(now),
                },
            );
        }
        if mask & 2 != 0 {
            out.in_flight.insert(key.clone(), i);
        }
        if mask & 4 != 0 {
            out.pending_refresh.insert(key.clone(), i + 5);
        }
        if mask & 8 != 0 {
            out.invalidated_after.insert(key, now);
        }
    }
    out
}
fn clone_cache(state: &PlayerLeaderboardCacheState) -> PlayerLeaderboardCacheState {
    PlayerLeaderboardCacheState {
        by_key: state.by_key.clone(),
        in_flight: state.in_flight.clone(),
        pending_refresh: state.pending_refresh.clone(),
        invalidated_after: state.invalidated_after.clone(),
    }
}
fn assert_cache(a: &PlayerLeaderboardCacheState, b: &PlayerLeaderboardCacheState) {
    assert_eq!(a.in_flight, b.in_flight);
    assert_eq!(a.pending_refresh, b.pending_refresh);
    assert_eq!(a.invalidated_after, b.invalidated_after);
    assert_eq!(a.by_key.len(), b.by_key.len());
    for (key, value) in &a.by_key {
        let other = b.by_key.get(key).unwrap();
        assert_eq!(
            (value.max_entries, value.refreshed_at, value.retry_after),
            (other.max_entries, other.refreshed_at, other.retry_after)
        );
        match (&value.value, &other.value) {
            (PlayerLeaderboardCacheValue::Ready(a), PlayerLeaderboardCacheValue::Ready(b)) => {
                assert!(Arc::ptr_eq(a, b))
            }
            (PlayerLeaderboardCacheValue::Error(a), PlayerLeaderboardCacheValue::Error(b)) => {
                assert_eq!(a, b)
            }
            _ => panic!("cache value changed"),
        }
    }
}
#[test]
fn invalidation_preserves_overlapping_keys_unrelated_entries_and_stale_completion() {
    let now = Instant::now();
    let later = now + Duration::from_secs(1);
    for mask in 0..16 {
        for count in [0, 1, 64] {
            for matching in [0, 1, count] {
                let original = cache(count, matching, mask, now);
                for api in ["gs-api", "GS-API", "missing", ""] {
                    let mut old = clone_cache(&original);
                    let mut new = clone_cache(&original);
                    for stamp in [later, now, later] {
                        baseline::invalidate_chart_for_api(&mut old, api, HASH, stamp);
                        new.invalidate_chart_for_api(api, HASH, stamp);
                        assert_cache(&old, &new);
                    }
                    let key = key(0, true);
                    for result in [&mut old, &mut new] {
                        let completion = result.complete_fetch::<()>(
                            &key,
                            10,
                            now,
                            later,
                            Duration::from_secs(5),
                            Err("network".into()),
                            false,
                            false,
                        );
                        if count > 0 && matching > 0 && mask != 0 && api == "gs-api" {
                            assert!(completion.queued_fetch.is_none());
                            assert!(!result.by_key.contains_key(&key));
                        }
                    }
                    assert_cache(&old, &new);
                }
            }
        }
    }
}
#[test]
fn repeated_invalidation_reuses_tombstones_without_churn() {
    let now = Instant::now();
    let mut state = cache(128, 64, 8, now);
    perf::assert_no_churn(|| {
        state.invalidate_chart_for_api(black_box("gs-api"), black_box(HASH), now)
    });
}
#[test]
fn invalidated_success_cannot_restore_scores_but_later_fetch_can() {
    let now = Instant::now();
    let later = now + Duration::from_secs(1);
    let original = cache(1, 1, 7, now);
    let key = key(0, true);
    for new in [false, true] {
        let mut state = clone_cache(&original);
        if new {
            state.invalidate_chart_for_api("gs-api", HASH, later);
        } else {
            baseline::invalidate_chart_for_api(&mut state, "gs-api", HASH, later);
        }
        for started in [now, later, later + Duration::from_nanos(1)] {
            let completion = state.complete_fetch(
                &key,
                10,
                started,
                later + Duration::from_secs(1),
                Duration::from_secs(5),
                Ok(PlayerLeaderboardFetchSuccess {
                    data: PlayerLeaderboardData {
                        panes: Vec::new(),
                        srpg_self_score: Some(9700),
                        itl_self_score: Some(9600),
                        itl_self_rank: Some(3),
                    },
                    imported_score: Some(42),
                    itl_self_found: true,
                }),
                true,
                true,
            );
            assert!(completion.queued_fetch.is_none());
            if started <= later {
                assert!(completion.fetched_itl_self.is_none());
                assert!(completion.fetched_srpg_self_score.is_none());
                assert!(completion.fetched_imported_score.is_none());
                assert!(!state.by_key.contains_key(&key));
            } else {
                assert_eq!(completion.fetched_imported_score, Some(42));
                assert_eq!(completion.fetched_srpg_self_score, Some(9700));
                assert_eq!(completion.fetched_itl_self, Some((Some(9600), Some(3))));
                assert!(state.by_key.contains_key(&key));
                assert!(!state.invalidated_after.contains_key(&key));
            }
        }
    }
}
fn gs_bytes(i: usize, legacy: bool) -> Vec<u8> {
    let entry = GsScoreEntry {
        score_percent: 0.8 + (i % 15) as f64 * 0.01,
        grade_code: 3,
        lamp_index: Some(2),
        lamp_judge_count: Some(4),
        username: format!("User-{i}"),
        fetched_at_ms: 1_700_000_000_000 + i as i64,
    };
    if legacy {
        bincode::encode_to_vec(
            crate::GsScoreEntryV1 {
                score_percent: entry.score_percent,
                grade_code: entry.grade_code,
                lamp_index: entry.lamp_index,
                username: entry.username,
                fetched_at_ms: entry.fetched_at_ms,
            },
            bincode::config::standard(),
        )
        .unwrap()
    } else {
        encode_gs_score_entry(&entry).unwrap()
    }
}
fn gs_tree(count: usize, shards: usize, junk: usize) -> Tree {
    let tree = Tree::new();
    for shard in 0..shards {
        fs::create_dir(tree.0.join(format!("{shard:02}"))).unwrap();
    }
    for i in 0..count {
        let dir = tree.0.join(format!("{:02}", i % shards));
        fs::write(
            dir.join(format!("{:016x}-{}.bin", i % 32, i)),
            gs_bytes(i, i % 3 == 0),
        )
        .unwrap();
    }
    for i in 0..junk {
        fs::write(tree.0.join("00").join(format!("junk-{i}.txt")), [255]).unwrap();
    }
    tree
}
fn local_tree(count: usize, junk: usize) -> Tree {
    let tree = Tree::new();
    for i in 0..count {
        fixtures::write_score(
            &tree.0,
            &format!("{:016x}", i % 32),
            i as i64,
            0.8 + (i % 15) as f64 * 0.01,
            i % 9 == 0,
            i % 8,
        );
    }
    for i in 0..junk {
        fs::write(tree.0.join(format!("junk-{i}.txt")), [255]).unwrap();
    }
    tree
}
fn local_scan(path: &Path) -> LocalScoreIndex {
    let mut index = LocalScoreIndex::default();
    scan_local_scores_dir(path, &mut index);
    index
}
fn old_local_scan(path: &Path) -> LocalScoreIndex {
    let mut index = LocalScoreIndex::default();
    baseline::scan_local_scores_dir(path, &mut index);
    index
}
fn assert_index(a: &LocalScoreIndex, b: &LocalScoreIndex) {
    assert_eq!(a.best_itg, b.best_itg);
    assert_eq!(a.best_ex, b.best_ex);
    assert_eq!(a.best_hard_ex, b.best_hard_ex);
    assert_eq!(a.best_pass_rate, b.best_pass_rate);
}
#[test]
fn index_scans_preserve_versions_selection_depth_and_corrupt_files() {
    for (count, shards, junk) in [(0, 1, 0), (1, 1, 0), (128, 4, 64)] {
        let gs = gs_tree(count, shards, junk);
        // The GS root and deeper directories were never part of the index.
        fs::write(gs.0.join("root-1.bin"), gs_bytes(1, false)).unwrap();
        let child = gs.0.join("00");
        fs::create_dir(child.join("nested")).unwrap();
        fs::write(child.join("nested/deeper-1.bin"), gs_bytes(1, false)).unwrap();
        for name in [
            "bad.bin",
            "-1.bin",
            "bad-1.BIN",
            "corrupt-1.bin",
            "empty-1.bin",
        ] {
            fs::write(child.join(name), []).unwrap();
        }
        fs::write(
            child.join("hash-with-dashes-nonnumeric.bin"),
            gs_bytes(12, false),
        )
        .unwrap();
        let mut large = gs_bytes(14, false);
        large.extend(vec![0; 8192]);
        fs::write(child.join("large-.bin"), large).unwrap();
        fs::create_dir(child.join("directory-1.bin")).unwrap();
        assert_eq!(
            baseline::best_gs_scores_from_dir(&gs.0),
            best_gs_scores_from_dir(&gs.0)
        );
        assert!(best_gs_scores_from_dir(&gs.0).contains_key("hash-with-dashes"));
        let local = local_tree(count, junk);
        fs::write(local.0.join(format!("{HASH}-777.bin")), [255]).unwrap();
        fs::create_dir(local.0.join(format!("{HASH}-778.bin"))).unwrap();
        let shard = local.0.join("00");
        fixtures::write_score(&shard, HASH, 900, 0.99, false, 256);
        fixtures::write_score(&shard.join("nested"), HASH, 901, 1.0, false, 0);
        assert_index(&old_local_scan(&local.0), &local_scan(&local.0));
        let old = baseline::load_local_score_index_from_root(&local.0);
        fs::remove_file(local.0.join("index.bin")).unwrap();
        let new = load_local_score_index_from_root(&local.0);
        assert_index(&old, &new);
        assert_index(&new, &load_local_score_index_from_root(&local.0));
        assert_eq!(new.best_itg.get(HASH).unwrap().score_percent, 0.99);
        assert!(best_gs_scores_from_dir(&local.0.join("missing")).is_empty());
    }
}
#[test]
fn index_scans_follow_file_and_directory_links() {
    let tree = gs_tree(1, 1, 0);
    let local = local_tree(1, 0);
    let gs_source = fs::read_dir(tree.0.join("00"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let local_source = fs::read_dir(&local.0)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    #[cfg(unix)]
    use std::os::unix::fs::{symlink as symlink_file, symlink as symlink_dir};
    #[cfg(windows)]
    use std::os::windows::fs::{symlink_dir, symlink_file};
    if let Err(error) = symlink_file(&gs_source, tree.0.join("00/link-1.bin")) {
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || (cfg!(windows) && error.raw_os_error() == Some(1314))
        {
            eprintln!("symlink regression not exercised: {error}");
            return;
        }
        panic!("{error}");
    }
    symlink_file(tree.0.join("missing"), tree.0.join("00/broken-1.bin")).unwrap();
    symlink_dir(tree.0.join("00"), tree.0.join("link-dir")).unwrap();
    symlink_file(&local_source, local.0.join(format!("{HASH}-300.bin"))).unwrap();
    symlink_dir(tree.0.join("00"), local.0.join("link-dir")).unwrap();
    assert_eq!(
        baseline::best_gs_scores_from_dir(&tree.0),
        best_gs_scores_from_dir(&tree.0)
    );
    assert_index(&old_local_scan(&local.0), &local_scan(&local.0));
}
#[test]
fn gs_index_reuses_read_storage_with_less_churn() {
    let tree = gs_tree(128, 4, 0);
    perf::assert_reduced_churn(
        || {
            black_box(baseline::best_gs_scores_from_dir(&tree.0));
        },
        || {
            black_box(best_gs_scores_from_dir(&tree.0));
        },
    );
}
#[cfg(windows)]
#[test]
fn index_scans_skip_locked_records_without_reusing_previous_contents() {
    use std::os::windows::fs::OpenOptionsExt;
    let gs = gs_tree(1, 1, 0);
    let local = local_tree(1, 0);
    let gs_path = gs.0.join("00/locked-1.bin");
    fs::write(&gs_path, gs_bytes(1, false)).unwrap();
    let local_path = fixtures::write_score(&local.0, HASH, 1234, 0.99, false, 0);
    let _gs_lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(gs_path)
        .unwrap();
    let _local_lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(local_path)
        .unwrap();
    let scores = best_gs_scores_from_dir(&gs.0);
    assert_eq!(baseline::best_gs_scores_from_dir(&gs.0), scores);
    assert!(!scores.contains_key("locked"));
    let scores = local_scan(&local.0);
    assert_index(&old_local_scan(&local.0), &scores);
    assert!(!scores.best_itg.contains_key(HASH));
}
fn variants() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    }
}
#[test]
#[ignore = "manual release comparison against 0.5.1222"]
fn benchmark_cache_preparation() {
    for (label, count, junk) in [
        ("empty", 0, 0),
        ("one", 1, 0),
        ("128", 128, 0),
        ("mixed", 128, 384),
    ] {
        let tree = local_tree(count, junk);
        for new in variants() {
            let f = black_box(
                if new { local_scan } else { old_local_scan } as fn(&Path) -> LocalScoreIndex
            );
            perf::measure_sampled(
                &format!("local/{label}/{}", if new { "new" } else { "old" }),
                8,
                count.max(1),
                || f(black_box(&tree.0)),
            );
        }
    }
    for (label, count, shards, junk) in [
        ("empty", 0, 1, 0),
        ("one", 1, 1, 0),
        ("128", 128, 1, 0),
        ("mixed", 128, 4, 384),
    ] {
        let tree = gs_tree(count, shards, junk);
        for new in variants() {
            let f = black_box(if new {
                best_gs_scores_from_dir
            } else {
                baseline::best_gs_scores_from_dir
            } as fn(&Path) -> HashMap<String, CachedScore>);
            perf::measure_sampled(
                &format!("gs/{label}/{}", if new { "new" } else { "old" }),
                8,
                count.max(1),
                || f(black_box(&tree.0)),
            );
        }
    }
    let now = Instant::now();
    for (label, count, matching, mask) in [
        ("one", 1, 1, 15),
        ("all", 128, 128, 15),
        ("sparse", 1024, 8, 15),
        ("miss", 1024, 0, 15),
        ("moving", 128, 128, 7),
        ("growing", 128, 128, 7),
        ("tombstones", 128, 128, 8),
    ] {
        let mut original = cache(count, matching, mask, now);
        if label == "growing" {
            original.invalidated_after = HashMap::new();
        }
        for new in variants() {
            let f = black_box(if new {
                PlayerLeaderboardCacheState::invalidate_chart_for_api
            } else {
                baseline::invalidate_chart_for_api
            }
                as fn(&mut PlayerLeaderboardCacheState, &str, &str, Instant));
            perf::measure_sampled_with_setup(
                &format!("invalidate/{label}/{}", if new { "new" } else { "old" }),
                100,
                count,
                || clone_cache(&original),
                |state| f(black_box(state), black_box("gs-api"), black_box(HASH), now),
            );
        }
    }
    for (label, mut v) in [
        ("valid", input("dance-single")),
        ("pump", input(" PUMP-DOUBLE ")),
        ("solo", input("dance-SOLO")),
        ("invalid", input("invalid")),
        ("unicode", input("dance-\u{65e5}")),
    ] {
        if label == "invalid" {
            v.remove_mask = 255;
            v.insert_mask = 255;
            v.fail_type_ok = false;
            v.autoplay_used = true;
            v.music_rate = 0.9;
        }
        for new in variants() {
            let f = black_box(if new {
                groovestats_eval_state_from_parts
            } else {
                baseline::groovestats_eval_state_from_parts
            }
                as fn(GrooveStatsEvalInput<'_>) -> GrooveStatsEvalState);
            perf::measure_sampled(
                &format!("eligibility/{label}/{}", if new { "new" } else { "old" }),
                20_000,
                1,
                || f(black_box(v)),
            );
        }
    }
}
