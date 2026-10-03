use super::*;
use std::hint::black_box;

#[path = "stats_encoding_baseline.rs"]
mod baseline;

fn fixture(count: usize, long_names: bool) -> ProfileStats {
    ProfileStats {
        current_combo: u32::MAX,
        known_pack_names: (0..count)
            .map(|i| {
                format!(
                    "Pack {i:04} - {}",
                    if long_names {
                        "日本-".repeat(80)
                    } else {
                        "Example Collection".to_string()
                    }
                )
            })
            .collect(),
    }
}

#[test]
fn profile_stats_encoding_preserves_sorted_bytes_and_existing_decoders() {
    for count in [0, 1, 250, 251, 1024] {
        for combo in [0, 250, 251, u32::MAX] {
            let mut stats = fixture(count, count == 251);
            stats.current_combo = combo;
            stats
                .known_pack_names
                .extend(["", "Alpha", "alpha", "日本", "😀", "a\0b", "a\nb"].map(str::to_string));
            let before = stats.clone();
            let original = baseline::encode_profile_stats(&stats).unwrap();
            let current = encode_profile_stats(&stats).unwrap();
            assert_eq!(current, original);
            assert_eq!(decode_profile_stats(&current), Ok(before.clone()));
            assert_eq!(stats, before);
            let (wire, consumed) = bincode::decode_from_slice::<ProfileStatsV1, _>(
                &current,
                bincode::config::standard(),
            )
            .unwrap();
            assert_eq!(consumed, current.len());
            assert!(
                wire.known_pack_names
                    .windows(2)
                    .all(|pair| pair[0] < pair[1])
            );
            crate::perf::assert_reduced_churn(
                || {
                    black_box(baseline::encode_profile_stats(&stats));
                },
                || {
                    black_box(encode_profile_stats(&stats));
                },
            );
        }
    }
    let empty = ProfileStats::default();
    assert_eq!(
        encode_profile_stats(&empty),
        baseline::encode_profile_stats(&empty)
    );
    let bytes = encode_profile_stats(&empty).unwrap();
    assert_eq!(decode_profile_stats(&bytes), Ok(empty));
}

#[test]
#[ignore = "manual original/current profile stats encoding benchmark; run in release"]
fn profile_stats_encoding_benchmark() {
    for (count, long) in [
        (0, false),
        (1, false),
        (128, false),
        (1024, false),
        (128, true),
    ] {
        let stats = fixture(count, long);
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [true, false]
        } else {
            [false, true]
        };
        for new in order {
            let variant = if new { "new" } else { "old" };
            let work = if new {
                encode_profile_stats
            } else {
                baseline::encode_profile_stats
            };
            crate::perf::measure_sampled(
                &format!("profile-stats/packs={count}/long={long}/{variant}"),
                128,
                1,
                || work(black_box(&stats)),
            );
        }
    }
}
