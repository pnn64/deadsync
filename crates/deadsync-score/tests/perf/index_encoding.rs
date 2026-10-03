use super::*;
use std::hint::black_box;

#[path = "index_encoding_baseline.rs"]
mod baseline;

fn fixture(count: usize, long_keys: bool) -> LocalScoreIndex {
    let mut index = LocalScoreIndex::default();
    for i in 0..count {
        let hash = format!(
            "chart-{i:016x}{}",
            if long_keys {
                "日本-".repeat(80)
            } else {
                String::new()
            }
        );
        index.best_itg.insert(
            hash.clone(),
            cached_score(Grade::Tier04, i as f64 / 100.0, Some(3), Some(2)),
        );
        index.best_ex.insert(
            hash.clone(),
            LocalScoreBestScalar {
                grade: Grade::Tier01,
                percent: i as f64,
            },
        );
        index.best_hard_ex.insert(
            hash.clone(),
            LocalScoreBestScalar {
                grade: Grade::Failed,
                percent: -(i as f64),
            },
        );
        index.best_pass_rate.insert(hash.clone(), i as u32);
        index.best_lamp.insert(
            hash,
            CachedLamp {
                index: 2,
                judge_count: Some(1),
            },
        );
    }
    index
}

#[test]
fn local_index_encoding_matches_original_bytes_without_cloning_keys() {
    for (count, long) in [
        (0, false),
        (1, false),
        (250, false),
        (251, false),
        (1024, false),
        (128, true),
    ] {
        let mut index = fixture(count, long);
        // Spare capacity and deleted buckets must not change serialized order.
        index.best_itg.reserve(100);
        index.best_itg.insert(
            "removed".to_string(),
            cached_score(Grade::Failed, 0.0, None, None),
        );
        index.best_itg.remove("removed");
        let before = index.clone();
        let original = baseline::encode_local_score_index(&index).unwrap();
        let current = encode_local_score_index(&index).unwrap();
        assert_eq!(current, original);
        assert_eq!(decode_local_score_index(&current), Some(before.clone()));
        assert_eq!(index, before);
        for length in 0..current.len().min(256) {
            assert!(decode_local_score_index(&current[..length]).is_none());
        }
        if count > 0 {
            crate::perf::assert_reduced_churn(
                || {
                    black_box(baseline::encode_local_score_index(&index));
                },
                || {
                    black_box(encode_local_score_index(&index));
                },
            );
        }
    }
    let mut index = fixture(1, false);
    let key = index.best_itg.keys().next().unwrap().clone();
    for value in [
        0.0,
        -0.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from_bits(0x7ff8_0123_4567_89ab),
    ] {
        index.best_itg.get_mut(&key).unwrap().score_percent = value;
        index.best_ex.get_mut(&key).unwrap().percent = value;
        assert_eq!(
            encode_local_score_index(&index),
            baseline::encode_local_score_index(&index)
        );
        let decoded = decode_local_score_index(&encode_local_score_index(&index).unwrap()).unwrap();
        assert_eq!(
            decoded.best_itg[&key].score_percent.to_bits(),
            value.to_bits()
        );
        assert_eq!(decoded.best_ex[&key].percent.to_bits(), value.to_bits());
    }
}

#[test]
#[ignore = "manual original/current local score index encoding benchmark; run in release"]
fn local_index_encoding_benchmark() {
    for (count, long) in [
        (0, false),
        (1, false),
        (128, false),
        (1024, false),
        (128, true),
    ] {
        let index = fixture(count, long);
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [true, false]
        } else {
            [false, true]
        };
        for new in order {
            let variant = if new { "new" } else { "old" };
            let work = if new {
                encode_local_score_index
            } else {
                baseline::encode_local_score_index
            };
            crate::perf::measure_sampled(
                &format!("local-index/entries={count}/long={long}/{variant}"),
                128,
                1,
                || work(black_box(&index)),
            );
        }
    }
}
