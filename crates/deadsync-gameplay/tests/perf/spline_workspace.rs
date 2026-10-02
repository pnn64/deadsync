use super::{SongLuaSplineSolver, perf, solve_song_lua_spline};
use std::hint::black_box;

include!("spline_workspace_baseline.rs");

fn fixture(size: usize, axes: usize) -> Vec<[f32; 3]> {
    (0..size)
        .map(|i| {
            std::array::from_fn(|axis| {
                if axis < axes {
                    ((i * (17 + axis * 6)) % 131) as f32 - 64.0
                } else {
                    -0.0
                }
            })
        })
        .collect()
}

fn assert_coefficients(a: &[[[f32; 4]; 3]], b: &[[[f32; 4]; 3]]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a
        .iter()
        .flatten()
        .flatten()
        .zip(b.iter().flatten().flatten())
    {
        assert!(
            a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
            "{a:?} != {b:?}"
        );
    }
}

#[test]
fn reusable_solver_matches_current_parent_across_growth_shrink_and_float_edges() {
    let mut solver = SongLuaSplineSolver::default();
    for size in [0, 1, 2, 3, 32, 256, 65_536, 16, 0, 1, 256] {
        for axes in 0..=3 {
            let points = fixture(size, axes);
            let old = old_solve_song_lua_spline(&points);
            assert_coefficients(&old, solver.solve(&points));
            assert_coefficients(&old, &solve_song_lua_spline(&points));
        }
    }
    let values = [
        0.0,
        -0.0,
        f32::from_bits(1),
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fc0_0042),
    ];
    for shift in 0..values.len() {
        let points: Vec<_> = (0..31)
            .map(|i| std::array::from_fn(|axis| values[(i + axis + shift) % values.len()]))
            .collect();
        assert_coefficients(&old_solve_song_lua_spline(&points), solver.solve(&points));
    }
}

#[test]
fn warmed_solver_has_no_allocator_churn_even_when_points_change() {
    let mut solver = SongLuaSplineSolver::default();
    let mut points = fixture(1024, 3);
    solver.solve(&points);
    perf::assert_no_churn(|| {
        for i in 0..32 {
            points[i][0] += 0.25;
            black_box(solver.solve(&points));
        }
        black_box(solver.solve(&points[..2]));
        black_box(solver.solve(&[]));
        black_box(solver.solve(&points));
    });
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_spline_workspace() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (size, axes) in [
        (2, 3),
        (32, 0),
        (32, 1),
        (32, 3),
        (256, 3),
        (4096, 3),
        (65_536, 3),
    ] {
        let points = fixture(size, axes);
        let mut solver = SongLuaSplineSolver::default();
        assert_coefficients(&old_solve_song_lua_spline(&points), solver.solve(&points));
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!(
                "workspace_{size}_{axes}axes_{}",
                if old { "old" } else { "new" }
            );
            perf::measure_sampled(&label, if size < 4096 { 1024 } else { 32 }, size, || {
                if old {
                    black_box(old_solve_song_lua_spline(black_box(&points)));
                } else {
                    black_box(solver.solve(black_box(&points)));
                }
            });
        }
    }
}
