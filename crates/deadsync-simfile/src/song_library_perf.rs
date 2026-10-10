use super::*;
use crate::metadata_perf::{compare, measure};
use std::hint::black_box;
mod original {
    include!("song_bounds_original.rs");
}

fn original_bounds(bpms: &[(f32, f32)]) -> (f64, f64) {
    (original::min_chart_bpm(bpms), original::max_chart_bpm(bpms))
}

fn assert_bounds(bpms: &[(f32, f32)]) {
    let expected = original_bounds(bpms);
    let actual = chart_bpm_bounds(bpms);
    assert_eq!(
        (actual.0.to_bits(), actual.1.to_bits()),
        (expected.0.to_bits(), expected.1.to_bits())
    );
}

#[test]
fn combined_chart_bounds_preserve_empty_invalid_and_extreme_bpm_values() {
    let values = [
        0.0,
        -0.0,
        -120.0,
        f32::MIN,
        f32::MAX,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fc01234),
        f32::from_bits(0x7f801234),
        60.0,
        180.0,
        120.0,
    ];
    assert_bounds(&[]);
    assert_eq!(chart_bpm_bounds(&[]), (f64::MAX, 0.0));
    for &a in &values {
        assert_bounds(&[(f32::NAN, a)]);
        for &b in &values {
            assert_bounds(&[(0.0, a), (f32::INFINITY, b)]);
        }
    }
    assert_eq!(
        chart_bpm_bounds(&[(0.0, -1.0), (1.0, f32::NAN)]),
        (f64::MAX, 0.0)
    );
}

#[test]
fn combined_chart_bounds_match_original_across_generated_float_bits() {
    let mut seed = 0x12345678u32;
    let mut values = Vec::new();
    for _ in 0..256 {
        values.clear();
        for i in 0..257 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            values.push((i as f32, f32::from_bits(seed)));
        }
        assert_bounds(&values);
        values.reverse();
        assert_bounds(&values);
    }
}

#[test]
#[ignore = "paired release benchmark; run alone with --nocapture"]
fn benchmark_library_chart_bounds() {
    for count in [0, 1, 8, 128, 4096] {
        for invalid in [false, true] {
            let input: Vec<_> = (0..count)
                .map(|i| {
                    (
                        i as f32,
                        if invalid && i % 3 == 0 {
                            f32::NAN
                        } else {
                            60.0 + (i % 241) as f32
                        },
                    )
                })
                .collect();
            let (_, a) = measure(|| original_bounds(&input));
            let (_, b) = measure(|| chart_bpm_bounds(&input));
            println!("ALLOC bounds/{count}/{invalid}: original {a:?}, current {b:?}");
            compare(
                &format!("bounds/{count}/{invalid}"),
                128,
                || {
                    black_box(original_bounds(black_box(&input)));
                },
                || {
                    black_box(chart_bpm_bounds(black_box(&input)));
                },
            );
        }
    }
}
