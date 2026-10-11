use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/perf.rs"]
mod allocations;
#[path = "../../../tests/support/paired_bench.rs"]
mod bench;
#[path = "perf_original_cache.rs"]
mod original;

fn payload(count: usize, sparse: bool) -> ReplayGainCacheFile {
    ReplayGainCacheFile {
        entries: (0..count)
            .map(|i| ReplayGainCacheEntry {
                path_hash: if sparse {
                    i as u64 % 251
                } else {
                    (i as u64).wrapping_mul(0x9e3779b97f4a7c15)
                },
                mtime_unix_nanos: if sparse {
                    0
                } else {
                    1_800_000_000_000_000_000 + i as u64
                },
                content_hash: if sparse {
                    0
                } else {
                    !(i as u64).wrapping_mul(0xd6e8feb86659fd93)
                },
                lufs: -18.0 + (i % 37) as f32 / 8.0,
                true_peak_linear: (i % 31) as f32 / 17.0,
            })
            .collect(),
    }
}

#[test]
fn single_pass_cache_matches_original_wire_bytes() {
    for count in [0, 1, 2, 127, 250, 251, 256, 1000] {
        for sparse in [false, true] {
            let mut file = payload(count, sparse);
            for (entry, value) in file.entries.iter_mut().zip([
                0,
                250,
                251,
                65535,
                65536,
                u32::MAX as u64,
                u32::MAX as u64 + 1,
                u64::MAX,
            ]) {
                entry.path_hash = value;
                entry.lufs = f32::from_bits(value as u32);
            }
            let expected = original::encode_replaygain_cache(&file).unwrap();
            let (actual, churn) = allocations::measure(|| encode_replaygain_cache(&file).unwrap());
            assert_eq!(actual, expected, "count={count}, sparse={sparse}");
            assert_eq!(churn.allocs, 1);
            assert_eq!(churn.reallocs, 0);
        }
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_replaygain_encoding() {
    for (count, sparse, iterations) in [(8, false, 5000), (10_000, false, 32), (10_000, true, 32)] {
        let file = payload(count, sparse);
        let (_, old) = allocations::measure(|| original::encode_replaygain_cache(&file).unwrap());
        let (_, new) = allocations::measure(|| encode_replaygain_cache(&file).unwrap());
        println!("cache {count} sparse={sparse} allocations: original {old:?}, current {new:?}");
        bench::compare(
            &format!("cache {count} sparse={sparse}"),
            iterations,
            |current| {
                let data = if current {
                    encode_replaygain_cache(black_box(&file))
                } else {
                    original::encode_replaygain_cache(black_box(&file))
                };
                black_box(data.unwrap());
            },
        );
    }
}
