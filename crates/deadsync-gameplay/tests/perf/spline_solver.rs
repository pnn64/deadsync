use super::{perf, solve_song_lua_spline};
use std::hint::black_box;

include!("spline_solver_baseline.rs");

fn fixture(size: usize, axes: usize) -> Vec<[f32; 3]> {
    (0..size)
        .map(|i| {
            std::array::from_fn(|axis| {
                if axis < axes {
                    ((i * (17 + axis * 6)) % 131) as f32 - 64.0
                } else {
                    1.0
                }
            })
        })
        .collect()
}

fn compare(points: &[[f32; 3]]) {
    let old = old_solve_song_lua_spline(points);
    let new = solve_song_lua_spline(points);
    assert_eq!(old.len(), new.len());
    for (i, (old, new)) in old.iter().zip(&new).enumerate() {
        for axis in 0..3 {
            for coefficient in 0..4 {
                let old = old[axis][coefficient];
                let new = new[axis][coefficient];
                // Optimized NaN arithmetic can choose a different operand's
                // payload. Preserve classification; all other bits must match.
                assert!(
                    old.to_bits() == new.to_bits() || (old.is_nan() && new.is_nan()),
                    "size {}, point {i}, axis {axis}, coefficient {coefficient}: {old:?} != {new:?}",
                    points.len()
                );
            }
        }
    }
}

#[test]
fn solver_matches_parent_bits_for_all_axes_and_size_limit() {
    for size in [0, 1, 2, 3, 4, 8, 31, 64, 257, 2048, 65_536] {
        for axes in 0..=3 {
            compare(&fixture(size, axes));
        }
        for axis in 1..3 {
            let mut points = fixture(size, 1);
            for point in &mut points {
                point.rotate_left(axis);
            }
            compare(&points);
        }
    }
}

#[test]
fn solver_preserves_signed_zero_subnormals_overflow_and_nan_classification() {
    let values = [
        0.0,
        -0.0,
        f32::from_bits(1),
        -f32::from_bits(1),
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fc0_0042),
    ];
    for size in [1, 2, 3, 4, 31] {
        for shift in 0..values.len() {
            compare(&vec![[values[shift]; 3]; size]);
            compare(
                &(0..size)
                    .map(|i| std::array::from_fn(|axis| values[(i + axis + shift) % values.len()]))
                    .collect::<Vec<_>>(),
            );
        }
    }
    let mut seed = 0x8497_361bu32;
    for size in [3, 16, 257] {
        let points = (0..size)
            .map(|_| {
                std::array::from_fn(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    f32::from_bits(seed)
                })
            })
            .collect::<Vec<_>>();
        compare(&points);
    }
}

#[test]
fn solver_only_allocates_output_and_one_shared_diagonal_buffer() {
    for (size, axes) in [
        (0, 0),
        (1, 3),
        (2, 3),
        (256, 0),
        (256, 1),
        (256, 3),
        (65_536, 3),
    ] {
        let points = fixture(size, axes);
        let scratch = usize::from(size >= 3 && axes != 0);
        perf::assert_churn_budget(
            usize::from(size != 0) + scratch,
            size * (48 + scratch * 4),
            || {
                black_box(solve_song_lua_spline(&points));
            },
        );
    }
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_spline_solver() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (size, axes) in [
        (2, 3),
        (32, 0),
        (32, 1),
        (32, 3),
        (256, 1),
        (256, 2),
        (256, 3),
        (4096, 3),
        (65_536, 1),
        (65_536, 3),
    ] {
        let points = fixture(size, axes);
        compare(&points);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!(
                "solver_{size}_{axes}axes_{}",
                if old { "old" } else { "new" }
            );
            perf::measure_sampled(&label, if size < 4096 { 1024 } else { 32 }, size, || {
                black_box(if old {
                    old_solve_song_lua_spline(black_box(&points))
                } else {
                    solve_song_lua_spline(black_box(&points))
                });
            });
        }
    }
}
