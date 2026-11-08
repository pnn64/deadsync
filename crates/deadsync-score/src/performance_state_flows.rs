// Frozen from starting main c28a030a8 for differential tests and paired benchmarks.
use super::*;
use std::hint::black_box;

fn score(percent: f64, failed: bool, lamp: Option<u8>) -> CachedScore {
    CachedScore {
        grade: if failed { Grade::Failed } else { Grade::Tier01 },
        score_percent: percent,
        lamp_index: lamp,
        lamp_judge_count: Some(2),
    }
}

fn snapshot_sources(
    count: usize,
    overlap: bool,
) -> (
    HashMap<String, CachedScore>,
    HashMap<String, CachedScore>,
    HashMap<String, ArrowCloudScores>,
) {
    let mut local = HashMap::with_capacity(count);
    let mut gs = HashMap::with_capacity(count);
    let mut ac = HashMap::with_capacity(count);
    for index in 0..count {
        local.insert(
            format!("{index:016x}"),
            score(0.98, index % 7 == 0, Some(3)),
        );
        gs.insert(
            format!("{:016x}", index + usize::from(!overlap) * count),
            score(0.99, index % 5 == 0, Some(2)),
        );
        ac.insert(
            format!("{:016x}", index + usize::from(!overlap) * count * 2),
            ArrowCloudScores {
                itg: (index % 3 != 0).then_some(ArrowCloudScore {
                    score_percent: 0.995,
                    is_fail: index % 11 == 0,
                    server_grade: None,
                    played_at: None,
                    play_id: None,
                }),
                ..Default::default()
            },
        );
    }
    (local, gs, ac)
}

#[test]
fn borrowed_snapshot_keys_preserve_merge_policy_and_owned_sorted_output() {
    for overlap in [false, true] {
        let (mut local, mut gs, mut ac) = snapshot_sources(17, overlap);
        // Equal scores, failed overrides, richer lamps, nonfinite values,
        // case-sensitive keys and non-ASCII hashes must keep the old policy.
        for (hash, value) in [
            ("\u{e9}lan", 0.0),
            ("TIE", 0.98),
            ("tie", f64::NAN),
            ("empty-ac", f64::INFINITY),
        ] {
            local.insert(hash.into(), score(value, false, Some(4)));
            gs.insert(hash.into(), score(value, true, None));
            ac.insert(hash.into(), ArrowCloudScores::default());
        }
        for sources in 0..8 {
            let local = (sources & 1 != 0).then_some(&local);
            let gs = (sources & 2 != 0).then_some(&gs);
            let ac = (sources & 4 != 0).then_some(&ac);
            let expected = original_merged_profile_scores(local, gs, ac);
            let actual = merged_profile_scores_from_sources(local, gs, ac);
            assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
            assert!(actual.windows(2).all(|pair| pair[0].0 < pair[1].0));
        }
        let actual = merged_profile_scores_from_sources(Some(&local), Some(&gs), Some(&ac));
        let saved = format!("{actual:?}");
        local.clear();
        gs.clear();
        ac.clear();
        assert_eq!(
            format!("{actual:?}"),
            saved,
            "snapshot must own its hashes and scores"
        );
    }
    let empty = HashMap::new();
    assert!(merged_profile_scores_from_sources(Some(&empty), None, None).is_empty());
}

#[test]
fn borrowed_snapshot_keys_reduce_temporary_table_storage() {
    for overlap in [false, true] {
        let (local, gs, ac) = snapshot_sources(4_096, overlap);
        let (expected, original) = crate::perf::measure(|| {
            original_merged_profile_scores(Some(&local), Some(&gs), Some(&ac))
        });
        let (actual, current) = crate::perf::measure(|| {
            merged_profile_scores_from_sources(Some(&local), Some(&gs), Some(&ac))
        });
        assert_eq!(actual, expected);
        // Both return the same owned keys. Only the temporary table's key
        // handles shrink from String (24 bytes) to &str (16 bytes) on this host.
        assert_eq!(current.allocs, original.allocs);
        assert_eq!(current.reallocs, original.reallocs);
        assert!(current.allocated_bytes < original.allocated_bytes);
    }
}

#[test]
fn fixed_width_snapshot_sort_matches_string_order_for_all_byte_ranges() {
    let mut hashes: Vec<_> = (1..257_u64)
        .map(|index| {
            format!(
                "{:016x}",
                index.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(23)
            )
        })
        .collect();
    hashes.extend([
        "0000000000000000".into(),
        "FFFFFFFFFFFFFFFF".into(),
        "ffffffffffffffff".into(),
        "\0".repeat(16),
        "\u{e9}".repeat(8),
        "\u{6771}\u{4eac}0123456789".into(),
    ]);
    for variable_width in [false, true] {
        let mut actual: Vec<_> = hashes
            .iter()
            .enumerate()
            .map(|(index, hash)| (hash.clone(), score(index as f64, false, None)))
            .collect();
        if variable_width {
            actual.push(("short".into(), score(1.0, true, None)));
        }
        let mut expected = actual.clone();
        expected.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
        sort_profile_scores(&mut actual);
        assert_eq!(actual, expected);
        crate::perf::assert_no_churn(|| {
            sort_profile_scores(&mut actual);
        });
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_score_snapshots() {
    for (count, overlap) in [(64, true), (4_096, true), (4_096, false)] {
        let (local, gs, ac) = snapshot_sources(count, overlap);
        let label = format!("merged snapshot count={count} overlap={overlap}");
        let (_, original) = crate::perf::measure(|| {
            original_merged_profile_scores(Some(&local), Some(&gs), Some(&ac))
        });
        let (_, current) = crate::perf::measure(|| {
            merged_profile_scores_from_sources(Some(&local), Some(&gs), Some(&ac))
        });
        println!("{label}: original {original:?}, current {current:?}");
        crate::paired_bench::compare(&label, if count < 100 { 2_000 } else { 100 }, |current| {
            black_box(if current {
                merged_profile_scores_from_sources(
                    black_box(Some(&local)),
                    black_box(Some(&gs)),
                    black_box(Some(&ac)),
                )
            } else {
                original_merged_profile_scores(
                    black_box(Some(&local)),
                    black_box(Some(&gs)),
                    black_box(Some(&ac)),
                )
            });
        });
    }
    let gs = snapshot_sources(4_096, true).1;
    crate::paired_bench::compare("merged snapshot single-source control", 100, |current| {
        black_box(if current {
            merged_profile_scores_from_sources(None, black_box(Some(&gs)), None)
        } else {
            original_merged_profile_scores(None, black_box(Some(&gs)), None)
        });
    });
    let gs: HashMap<_, _> = gs
        .into_iter()
        .map(|(hash, value)| (format!("{hash}-variable"), value))
        .collect();
    crate::paired_bench::compare("merged snapshot variable-width control", 100, |current| {
        black_box(if current {
            merged_profile_scores_from_sources(None, black_box(Some(&gs)), None)
        } else {
            original_merged_profile_scores(None, black_box(Some(&gs)), None)
        });
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RetryItem<'a> {
    hash: &'a str,
    value: u32,
}

#[test]
fn borrowed_retry_key_preserves_order_caps_and_key_calls() {
    for cap in [0, 1, 3, 32] {
        let mut original = SubmitRetryState::default();
        let mut current = SubmitRetryState::default();
        let mut calls = [0, 0];
        for (value, hash) in [
            "",
            " \t",
            "one",
            "ONE",
            " two ",
            "two",
            "Three",
            "\u{e9}lan",
            "\u{c9}lan",
            "three",
        ]
        .into_iter()
        .enumerate()
        {
            for side in [0, 1, 7] {
                let item = RetryItem {
                    hash,
                    value: value as u32,
                };
                let observed = std::cell::Cell::new(0);
                original_upsert(
                    &mut original,
                    side,
                    item,
                    |entry| {
                        observed.set(observed.get() + 1);
                        entry.hash
                    },
                    cap,
                );
                calls[0] = observed.get();
                observed.set(0);
                current.upsert_by_key(
                    side,
                    item,
                    |entry| {
                        observed.set(observed.get() + 1);
                        entry.hash
                    },
                    cap,
                );
                calls[1] = observed.get();
                assert_eq!(calls[0], calls[1]);
                for side in [0, 1] {
                    assert_eq!(current.entries(side), original.entries(side));
                }
            }
        }
    }
}

#[test]
fn borrowed_retry_key_has_no_churn_on_update_or_reserved_insert() {
    let mut state = SubmitRetryState::default();
    state.entries_mut(0).reserve(4);
    for (value, hash) in ["0123456789abcdef", "0123456789ABCDEF", "another-chart"]
        .into_iter()
        .enumerate()
    {
        crate::perf::assert_no_churn(|| {
            state.upsert_by_key(
                0,
                RetryItem {
                    hash,
                    value: value as u32,
                },
                |entry| entry.hash,
                4,
            );
        });
    }
    assert_eq!(state.entries(0).len(), 2);
    assert_eq!(state.entries(0)[0].value, 1);
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_retry_keys() {
    for count in [1, 32, 128] {
        let hashes: Vec<_> = (0..count).map(|index| format!("{index:016x}")).collect();
        let mut state = SubmitRetryState::default();
        for hash in &hashes {
            state.entries_mut(0).push(RetryItem { hash, value: 0 });
        }
        let item = RetryItem {
            hash: &hashes[count - 1],
            value: 1,
        };
        let (_, original) =
            crate::perf::measure(|| original_upsert(&mut state, 0, item, |entry| entry.hash, 128));
        let (_, current) =
            crate::perf::measure(|| state.upsert_by_key(0, item, |entry| entry.hash, 128));
        let label = format!("retry key count={count}");
        println!("{label}: original {original:?}, current {current:?}");
        crate::paired_bench::compare(&label, 100_000, |current| {
            if current {
                state.upsert_by_key(black_box(0), black_box(item), |entry| entry.hash, 128);
            } else {
                original_upsert(
                    &mut state,
                    black_box(0),
                    black_box(item),
                    |entry| entry.hash,
                    128,
                );
            }
            black_box(state.entries(0));
        });
    }
}

fn original_upsert<T, K>(
    state: &mut SubmitRetryState<T>,
    side_index: usize,
    entry: T,
    key: K,
    cap: usize,
) where
    K: Fn(&T) -> &str,
{
    let hash = key(&entry).trim().to_string();
    if hash.is_empty() {
        return;
    }
    let entries = state.entries_mut(side_index);
    if let Some(stored) = entries
        .iter_mut()
        .find(|stored| key(stored).eq_ignore_ascii_case(hash.as_str()))
    {
        *stored = entry;
        return;
    }
    entries.push(entry);
    if entries.len() > cap {
        entries.drain(0..entries.len() - cap);
    }
}

fn original_merged_profile_scores(
    local: Option<&HashMap<String, CachedScore>>,
    gs: Option<&HashMap<String, CachedScore>>,
    ac: Option<&HashMap<String, ArrowCloudScores>>,
) -> Vec<(String, CachedScore)> {
    let local = local.filter(|scores| !scores.is_empty());
    let gs = gs.filter(|scores| !scores.is_empty());
    let ac = ac.filter(|scores| !scores.is_empty());
    match (local, gs, ac) {
        (None, None, None) => return Vec::new(),
        (Some(scores), None, None) | (None, Some(scores), None) => {
            return original_collect_sorted_profile_scores(
                scores
                    .iter()
                    .map(|(chart_hash, score)| (chart_hash.clone(), *score)),
            );
        }
        (None, None, Some(scores)) => {
            let itg_count = scores
                .values()
                .filter(|scores| scores.itg.is_some())
                .count();
            let mut merged = Vec::with_capacity(itg_count);
            for (chart_hash, scores) in scores {
                if let Some(score) = scores.itg {
                    merged.push((chart_hash.clone(), score.to_cached_score()));
                }
            }
            merged.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
            return merged;
        }
        _ => {}
    }

    let capacity =
        local.map_or(0, HashMap::len) + gs.map_or(0, HashMap::len) + ac.map_or(0, HashMap::len);
    let mut merged = HashMap::with_capacity_and_hasher(capacity, FxBuildHasher);
    let mut insert = |chart_hash: &str, score: CachedScore| match merged.get_mut(chart_hash) {
        Some(best) => {
            *best = best_cached_itg_score([Some(*best), Some(score)])
                .expect("two scores always produce a best score");
        }
        None => {
            merged.insert(chart_hash.to_owned(), score);
        }
    };
    if let Some(scores) = local {
        for (chart_hash, score) in scores {
            insert(chart_hash, *score);
        }
    }
    if let Some(scores) = gs {
        for (chart_hash, score) in scores {
            insert(chart_hash, *score);
        }
    }
    if let Some(scores) = ac {
        for (chart_hash, score) in scores {
            if let Some(score) = score.itg {
                insert(chart_hash, score.to_cached_score());
            }
        }
    }
    original_collect_sorted_profile_scores(merged.into_iter())
}

fn original_collect_sorted_profile_scores(
    entries: impl Iterator<Item = (String, CachedScore)>,
) -> Vec<(String, CachedScore)> {
    let mut scores: Vec<_> = entries.collect();
    scores.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
    scores
}
