use super::*;
use crate::{paired_bench, perf};
use std::hint::black_box;

mod original {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/perf_1870_ranges.rs"
    ));
}

#[test]
fn range_merging_preserves_order_union_and_saturating_gaps() {
    let special = [
        vec![],
        vec![(0, 1)],
        vec![(9, 9), (9, 12), (4, 10)],
        vec![(30, 40), (0, 10), (20, 30), (10, 20), (0, 10)],
        vec![
            (0, 1),
            (u64::MAX - 1, u64::MAX),
            (u64::MAX - 3, u64::MAX - 2),
        ],
        vec![(3, 1), (1, 0), (0, 0)],
    ];
    for gap in [0, 1, 10, MERGE_GAP_BYTES, u64::MAX] {
        for ranges in &special {
            assert_eq!(
                merge_ranges(ranges.clone(), gap),
                original::merge_ranges(ranges.clone(), gap)
            );
        }
        let mut seed = 0x1827346_u64;
        for len in 0..128 {
            let ranges: Vec<_> = (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let start = seed % 4096;
                    (start, start + (seed >> 32) % 65)
                })
                .collect();
            let expected = original::merge_ranges(ranges.clone(), gap);
            let actual = merge_ranges(ranges, gap);
            assert_eq!(actual, expected, "len={len}, gap={gap}");
            assert!(
                actual
                    .windows(2)
                    .all(|p| p[0].1.saturating_add(gap) < p[1].0)
            );
        }
    }
}

#[test]
fn merging_reuses_the_consumed_allocation_without_churn() {
    for capacity in [0, 1, 64, 1024] {
        let mut ranges = Vec::with_capacity(capacity);
        ranges.extend((0..capacity).rev().map(|n| (n as u64, n as u64 + 2)));
        let pointer = ranges.as_ptr();
        let (_, before) = perf::measure(|| original::merge_ranges(ranges.clone(), 0));
        let (merged, after) = perf::measure(|| merge_ranges(ranges, 0));
        assert_eq!(merged.as_ptr(), pointer);
        assert_eq!(merged.capacity(), capacity);
        assert_eq!(after, perf::Churn::default());
        assert_eq!(before.allocs, if capacity == 0 { 0 } else { 2 });
        if capacity != 0 {
            assert_eq!(merged, [(0, capacity as u64 + 1)]);
        }
    }
}

fn ranges(len: usize, shape: &str) -> Vec<(u64, u64)> {
    let mut ranges: Vec<_> = (0..len)
        .map(|n| {
            let start = n as u64 * 80_000;
            (start, start + if shape == "overlap" { 100_000 } else { 10 })
        })
        .collect();
    if shape == "shuffled" && len != 0 {
        for i in (1..len).rev() {
            ranges.swap(i, (i * 7919 + 17) % (i + 1));
        }
    }
    ranges
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_archive_range_merging() {
    for (len, shape) in [
        (0, "sparse"),
        (1, "sparse"),
        (8, "sparse"),
        (8, "overlap"),
        (128, "sparse"),
        (128, "overlap"),
        (128, "shuffled"),
        (4096, "sparse"),
        (4096, "overlap"),
        (4096, "shuffled"),
    ] {
        let label = format!("ranges/{len}/{shape}");
        let input = ranges(len, shape);
        let old_input = input.clone();
        let new_input = input.clone();
        let (_, before) = perf::measure(|| original::merge_ranges(old_input, MERGE_GAP_BYTES));
        let (_, after) = perf::measure(|| merge_ranges(new_input, MERGE_GAP_BYTES));
        println!("{label} allocations: original {before:?}; current {after:?}");
        paired_bench::compare_prepared(
            &label,
            if len < 128 { 5000 } else { 256 },
            || input.clone(),
            |input, current| {
                drop(black_box(if current {
                    merge_ranges(black_box(input), black_box(MERGE_GAP_BYTES))
                } else {
                    original::merge_ranges(black_box(input), black_box(MERGE_GAP_BYTES))
                }));
            },
        );
    }
}
