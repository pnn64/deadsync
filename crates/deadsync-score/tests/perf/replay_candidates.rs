use super::*;
use crate::leaderboard::replay_perf::{assert_replay_equal, baseline as old_entry, edges, pair};
use crate::perf;
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/replay_candidates/baseline.rs"
    ));
}

fn candidates(count: usize, pattern: &str) -> Vec<LocalReplayCandidate<'static>> {
    let mut seed = 43u64;
    let mut values = (0..count)
        .map(|ordinal| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            LocalReplayCandidate {
                initials: if ordinal % 3 == 0 { "雪" } else { "ABC" },
                path: PathBuf::from(format!("scores/local/{ordinal:016x}-1800000000000.bin")),
                score_percent: (seed >> 32) as f64 / u32::MAX as f64,
                played_at_ms: 1_800_000_000_000 + (ordinal % 7) as i64,
                ordinal,
            }
        })
        .collect::<Vec<_>>();
    match pattern {
        "sorted" => values.sort_unstable_by(compare_local_replay_candidates),
        "reverse" => values.sort_unstable_by(|a, b| compare_local_replay_candidates(b, a)),
        "ties" | "nan" => {
            for candidate in &mut values {
                candidate.score_percent = if pattern == "nan" { f64::NAN } else { 0.5 };
                candidate.played_at_ms = 1_800_000_000_000;
                candidate.initials = "same";
            }
            values.reverse();
        }
        _ => {}
    }
    values
}

fn copy_candidates<'a>(values: &[LocalReplayCandidate<'a>]) -> Vec<LocalReplayCandidate<'a>> {
    values
        .iter()
        .map(|value| LocalReplayCandidate {
            initials: value.initials,
            path: value.path.clone(),
            score_percent: value.score_percent,
            played_at_ms: value.played_at_ms,
            ordinal: value.ordinal,
        })
        .collect()
}

fn selected(
    candidates: impl Iterator<Item = LocalReplayCandidate<'static>>,
    limit: usize,
    reject_every: usize,
) -> Vec<usize> {
    candidates
        .filter(|value| reject_every == 0 || value.ordinal % reject_every != 0)
        .take(limit)
        .map(|value| value.ordinal)
        .collect()
}

fn selection_checksum(
    candidates: impl Iterator<Item = LocalReplayCandidate<'static>>,
    limit: usize,
    reject_every: usize,
) -> u64 {
    candidates
        .filter(|value| reject_every == 0 || value.ordinal % reject_every != 0)
        .take(limit)
        .fold(0u64, |sum, candidate| {
            sum.rotate_left(5) ^ candidate.ordinal as u64
        })
}

#[test]
fn incremental_candidate_order_preserves_ties_limits_and_skipped_files() {
    for count in [0, 1, 16, 64, 65, 128, 1024] {
        for pattern in ["random", "sorted", "reverse", "ties", "nan"] {
            let input = candidates(count, pattern);
            for limit in [0, 1, 5, 32, count / 2, count, usize::MAX] {
                for reject_every in [0, 1, 2, 7] {
                    assert_eq!(
                        selected(
                            baseline::ordered_replay_candidates(copy_candidates(&input), limit),
                            limit,
                            reject_every
                        ),
                        selected(
                            ordered_replay_candidates(copy_candidates(&input), limit),
                            limit,
                            reject_every
                        ),
                        "{count} {pattern} {limit} {reject_every}"
                    );
                }
            }
        }
    }
    // NaN comparisons retain the old sorting path. This mixed case has a
    // consistent date/name/ordinal order even though score comparison is partial.
    let mut input = candidates(128, "ties");
    input[50].score_percent = f64::NAN;
    assert!(matches!(
        ordered_replay_candidates(copy_candidates(&input), 5),
        OrderedReplayCandidates::Sorted(_)
    ));
    assert_eq!(
        selected(
            baseline::ordered_replay_candidates(copy_candidates(&input), 5),
            5,
            0
        ),
        selected(ordered_replay_candidates(input, 5), 5, 0)
    );
}

#[test]
fn candidate_heap_reuses_the_input_allocation() {
    let input = candidates(1024, "random");
    let pointer = input.as_ptr().cast::<u8>();
    let capacity = input.capacity();
    let mut ordered = None;
    perf::assert_no_churn(|| ordered = Some(ordered_replay_candidates(input, 5)));
    let OrderedReplayCandidates::Heap {
        candidates: heap, ..
    } = ordered.unwrap()
    else {
        panic!("large shuffled history should use a heap")
    };
    assert_eq!(heap.as_slice().as_ptr().cast::<u8>(), pointer);
    assert_eq!(heap.capacity(), capacity);

    let mut ordered = ordered_replay_candidates(candidates(1024, "random"), 5);
    for _ in 0..50 {
        let mut next = None;
        perf::assert_no_churn(|| next = ordered.next());
        assert!(next.is_some());
    }
    assert!(matches!(ordered, OrderedReplayCandidates::Sorted(_)));
}

fn score_entry() -> LocalScoreEntry {
    LocalScoreEntry {
        version: 1,
        played_at_ms: 1_800_000_000_000,
        music_rate: 1.0,
        score_percent: 0.95,
        grade_code: 0,
        lamp_index: None,
        lamp_judge_count: None,
        ex_score_percent: 0.9,
        hard_ex_score_percent: 0.8,
        judgment_counts: [100, 1, 0, 0, 0, 0],
        holds_held: 3,
        holds_total: 4,
        rolls_held: 1,
        rolls_total: 1,
        mines_avoided: 2,
        mines_total: 2,
        hands_achieved: 1,
        fail_time: Some(99.0),
        beat0_time_ns: -123456,
        replay: edges(31, 3),
    }
}

#[test]
fn replay_loader_skips_missing_and_truncated_files_without_changing_rank() {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-replay-perf-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    let valid = dir.join("valid.bin");
    let corrupt = dir.join("corrupt.bin");
    let absent = dir.join("absent.bin");
    let encoded = encode_local_score_entry(&score_entry()).unwrap();
    fs::write(&valid, &encoded).unwrap();
    fs::write(&corrupt, &encoded[..encoded.len() / 2]).unwrap();
    for all_missing in [false, true] {
        let mut input = candidates(128, "random");
        input.sort_unstable_by(compare_local_replay_candidates);
        for (index, candidate) in input.iter_mut().enumerate() {
            candidate.path = if !all_missing && index % 3 == 2 {
                valid.clone()
            } else if index % 2 == 0 {
                absent.clone()
            } else {
                corrupt.clone()
            };
        }
        input.rotate_left(17);
        for limit in [1, 5, 128] {
            let old = baseline::local_replay_entries(copy_candidates(&input), limit);
            let new = local_replay_entries(copy_candidates(&input), limit);
            assert_eq!(old.len(), new.len());
            for (old, new) in old.iter().zip(&new) {
                assert_replay_equal(old, new);
            }
            if !all_missing {
                assert!(!new.is_empty());
            }
        }
    }
    fs::remove_file(valid).unwrap();
    fs::remove_file(corrupt).unwrap();
    fs::remove_dir(dir).unwrap();
}

#[test]
#[ignore = "manual release benchmark; seven batches and separate allocation accounting"]
fn replay_preparation_bench_selection() {
    for (label, count, limit, pattern, reject_every) in [
        ("empty", 0, 5, "random", 0),
        ("small", 16, 5, "random", 0),
        ("medium", 256, 5, "random", 0),
        ("large", 4096, 5, "random", 0),
        ("one", 4096, 1, "random", 0),
        ("thirty_two", 4096, 32, "random", 0),
        ("all", 4096, 4096, "random", 0),
        ("sorted", 4096, 5, "sorted", 0),
        ("reverse", 4096, 5, "reverse", 0),
        ("ties", 4096, 5, "ties", 0),
        ("nan", 4096, 5, "nan", 0),
        ("half_missing", 4096, 5, "random", 2),
        ("all_missing", 4096, 5, "random", 1),
    ] {
        let input = candidates(count, pattern);
        pair(
            &format!("select_{label}"),
            if count > 256 { 32 } else { 1024 },
            count,
            || {
                selection_checksum(
                    baseline::ordered_replay_candidates(copy_candidates(black_box(&input)), limit),
                    limit,
                    reject_every,
                )
            },
            || {
                selection_checksum(
                    ordered_replay_candidates(copy_candidates(black_box(&input)), limit),
                    limit,
                    reject_every,
                )
            },
        );
    }
    for (count, edge_count) in [(16, 64), (256, 4096), (4096, 4096)] {
        let input = candidates(count, "random");
        let replay = edges(edge_count, 7);
        let old = || {
            baseline::ordered_replay_candidates(copy_candidates(black_box(&input)), 5)
                .take(5)
                .enumerate()
                .map(|(rank, candidate)| {
                    old_entry::machine_replay_entry(
                        rank as u32 + 1,
                        MachineReplayPlay {
                            initials: candidate.initials.into(),
                            score_percent: candidate.score_percent,
                            played_at_ms: candidate.played_at_ms,
                            is_fail: false,
                            replay_beat0_time_ns: -123456789,
                            replay: black_box(&replay).clone(),
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        let new = || {
            ordered_replay_candidates(copy_candidates(black_box(&input)), 5)
                .take(5)
                .enumerate()
                .map(|(rank, candidate)| {
                    machine_replay_entry(
                        rank as u32 + 1,
                        MachineReplayPlay {
                            initials: candidate.initials.into(),
                            score_percent: candidate.score_percent,
                            played_at_ms: candidate.played_at_ms,
                            is_fail: false,
                            replay_beat0_time_ns: -123456789,
                            replay: black_box(&replay).clone(),
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        pair(
            &format!("prepare_{count}"),
            if count > 256 { 32 } else { 256 },
            5,
            old,
            new,
        );
    }
}
