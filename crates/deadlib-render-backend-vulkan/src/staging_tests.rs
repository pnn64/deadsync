use super::*;
use std::hint::black_box;

fn staging(capacity: vk::DeviceSize) -> TextureStagingBuffer {
    TextureStagingBuffer {
        resource: BufferResource {
            buffer: vk::Buffer::null(),
            memory: vk::DeviceMemory::null(),
        },
        capacity,
    }
}

// Frozen from 22ce642b5, including first-entry tie breaking.
fn baseline(pool: &[TextureStagingBuffer], needed: vk::DeviceSize) -> Option<usize> {
    pool.iter()
        .enumerate()
        .filter(|(_, staging)| staging.capacity >= needed)
        .min_by_key(|(_, staging)| staging.capacity)
        .map(|(index, _)| index)
}

#[test]
fn staging_selection_preserves_fits_ties_and_extreme_sizes() {
    let capacities = [0, 1, 2, 4096, u64::MAX];
    let mut pool = Vec::with_capacity(6);
    for len in 0..=6 {
        for mut pattern in 0..5usize.pow(len) {
            pool.clear();
            for _ in 0..len {
                pool.push(staging(capacities[pattern % capacities.len()]));
                pattern /= capacities.len();
            }
            for needed in [0, 1, 2, 3, 4095, 4096, 4097, u64::MAX] {
                assert_eq!(
                    best_fit_staging_index(&pool, needed),
                    baseline(&pool, needed)
                );
            }
        }
    }
}

#[test]
fn staging_reuse_preserves_pool_order_and_byte_accounting() {
    let mut actual: Vec<_> = [4096, 1024, 4096, 2048, 8192].map(staging).into();
    let mut expected: Vec<_> = actual.iter().map(|s| staging(s.capacity)).collect();
    let mut actual_bytes = actual.iter().map(|s| s.capacity as usize).sum();
    let mut expected_bytes = actual_bytes;
    for index in 0..1024 {
        let needed = [4096, 8193, 512, 2048, 4095, 8192][index % 6];
        let old = baseline(&expected, needed).map(|slot| {
            let selected = expected.swap_remove(slot);
            expected_bytes -= selected.capacity as usize;
            selected
        });
        let mut new = None;
        perf::assert_no_churn(|| {
            new = take_pooled_texture_staging(&mut actual, &mut actual_bytes, needed);
        });
        assert_eq!(
            old.as_ref().map(|s| s.capacity),
            new.as_ref().map(|s| s.capacity)
        );
        assert_eq!(expected_bytes, actual_bytes);
        assert_eq!(expected.len(), actual.len());
        assert!(
            expected
                .iter()
                .zip(&actual)
                .all(|(a, b)| a.capacity == b.capacity)
        );
        if let (Some(old), Some(new)) = (old, new) {
            expected_bytes += old.capacity as usize;
            actual_bytes += new.capacity as usize;
            expected.push(old);
            actual.push(new);
        }
    }
}

#[test]
#[ignore = "manual release benchmark; --ignored --nocapture --test-threads=1"]
fn staging_search_benchmark() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for count in [0, 1, 3, 8, 32] {
        for case in ["equal", "first", "middle", "last", "fit", "miss"] {
            let mut pool: Vec<_> = (0..count)
                .map(|i| {
                    staging(if case == "equal" {
                        4096
                    } else {
                        (i as u64 + 2) * 4096
                    })
                })
                .collect();
            if count != 0 {
                let slot = match case {
                    "first" => 0,
                    "middle" => count / 2,
                    _ => count - 1,
                };
                if matches!(case, "first" | "middle" | "last") {
                    pool[slot].capacity = 4096;
                }
            }
            let needed = if case == "miss" { u64::MAX } else { 4096 };
            for old in if reverse {
                [false, true]
            } else {
                [true, false]
            } {
                let name = format!("{case}/{count}/{}", if old { "before" } else { "after" });
                // Long enough batches to measure even the empty/single-slot cases.
                if old {
                    perf::measure_sampled(&name, 262144, 1, || {
                        baseline(black_box(&pool), black_box(needed))
                    });
                } else {
                    perf::measure_sampled(&name, 262144, 1, || {
                        best_fit_staging_index(black_box(&pool), black_box(needed))
                    });
                }
            }
        }
    }
}

fn baseline_take(
    pool: &mut Vec<TextureStagingBuffer>,
    bytes: &mut usize,
    needed: vk::DeviceSize,
) -> Option<TextureStagingBuffer> {
    let staging = pool.swap_remove(baseline(pool, needed)?);
    *bytes = bytes.saturating_sub(staging.capacity as usize);
    Some(staging)
}

fn measure_reuse(
    name: &str,
    capacities: &[u64],
    requests: &[u64],
    mut take: impl FnMut(
        &mut Vec<TextureStagingBuffer>,
        &mut usize,
        u64,
    ) -> Option<TextureStagingBuffer>,
) {
    let mut pool: Vec<_> = capacities.iter().copied().map(staging).collect();
    let mut bytes = capacities.iter().map(|&capacity| capacity as usize).sum();
    let mut request = 0;
    perf::measure_sampled(name, 262144, 1, || {
        let needed = requests[request];
        request = (request + 1) % requests.len();
        if let Some(staging) = take(
            black_box(&mut pool),
            black_box(&mut bytes),
            black_box(needed),
        ) {
            // Model retirement returning the selected buffer to the pool.
            bytes += staging.capacity as usize;
            pool.push(staging);
        }
        black_box((&pool, bytes));
    });
}

#[test]
#[ignore = "manual release benchmark of pool selection, removal, and reuse"]
fn staging_reuse_benchmark() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (case, capacities, requests) in [
        ("video", vec![3_110_400; 3], vec![3_110_400]),
        (
            "mixed",
            vec![4096, 8192, 16384, 4096, 8192, 16384],
            vec![4096, 8192, 16384],
        ),
        ("oversized", (2..34).map(|n| n * 4096).collect(), vec![4096]),
        ("missing", vec![4096; 8], vec![8192]),
    ] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let name = format!("reuse-{case}/{}", if old { "before" } else { "after" });
            if old {
                measure_reuse(&name, &capacities, &requests, baseline_take);
            } else {
                measure_reuse(&name, &capacities, &requests, take_pooled_texture_staging);
            }
        }
    }
}
