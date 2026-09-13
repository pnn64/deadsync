//! Full production spline preparation beside its frozen parent implementation.
use std::hint::black_box;

include!("spline_preparation_baseline.rs");

fn fixture(size: usize, seed: usize) -> SongLuaNoteHideWindows {
    let mut windows = Vec::new();
    for index in 0..32 {
        let start = ((index * 7919 + seed * 101) % (size + 8)) as f32 * 0.25 - 1.5;
        windows.push(SongLuaNoteHideWindowRuntime {
            column: index % 2,
            start_beat: start,
            end_beat: start + (index % 7) as f32 * 0.5,
        });
    }
    windows.extend([
        SongLuaNoteHideWindowRuntime {
            column: 0,
            start_beat: f32::NAN,
            end_beat: 4.0,
        },
        SongLuaNoteHideWindowRuntime {
            column: 0,
            start_beat: 0.0,
            end_beat: f32::INFINITY,
        },
        SongLuaNoteHideWindowRuntime {
            column: 0,
            start_beat: 4.0,
            end_beat: -4.0,
        },
    ]);
    SongLuaNoteHideWindows::new(windows)
}

#[test]
fn spline_coefficients_and_sampling_match_parent_bit_for_bit() {
    for size in [1, 2, 3, 4, 8, 31, 64, 257, 2048] {
        for seed in 0..8 {
            let source = fixture(size, seed);
            for beats_per_t in [0.125, 0.25, 1.0, 3.75, f32::MIN_POSITIVE] {
                let mut old = source.clone();
                let mut new = source.clone();
                for column in [0, 1, MAX_COLS - 1] {
                    old_set_zoom_spline(&mut old, column, beats_per_t, size);
                    new.set_zoom_spline(column, beats_per_t, size);
                    let old_points = &old.zoom_splines[column].coefficients;
                    let new_points = &new.zoom_splines[column].coefficients;
                    assert_eq!(old_points.len(), new_points.len());
                    for (index, (old, new)) in old_points.iter().zip(new_points).enumerate() {
                        assert_eq!(
                            old.map(f32::to_bits),
                            new.map(f32::to_bits),
                            "size {size}, seed {seed}, column {column}, spacing {beats_per_t}, coefficient {index}"
                        );
                    }
                    for step in -8..size as i32 * 4 + 8 {
                        let beat = step as f32 * 0.25 * beats_per_t;
                        assert_eq!(
                            old.zoom_offset(column, beat).to_bits(),
                            new.zoom_offset(column, beat).to_bits()
                        );
                    }
                    for beat in [f32::NEG_INFINITY, f32::INFINITY, f32::NAN, -0.0] {
                        assert_eq!(
                            old.zoom_offset(column, beat).to_bits(),
                            new.zoom_offset(column, beat).to_bits()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn invalid_spline_requests_preserve_the_existing_spline() {
    let mut state = SongLuaNoteHideWindows::default();
    state.set_zoom_spline(0, 0.25, 64);
    let previous = state.clone();
    for (column, spacing, size) in [
        (MAX_COLS, 1.0, 8),
        (usize::MAX, 1.0, 8),
        (0, 1.0, 0),
        (0, 1.0, 65_537),
        (0, 1.0, usize::MAX),
        (0, -1.0, 8),
        (0, 0.0, 8),
        (0, f32::INFINITY, 8),
        (0, f32::NAN, 8),
    ] {
        perf::assert_no_churn(|| state.set_zoom_spline(column, spacing, size));
        assert_eq!(state, previous);
    }
}

#[test]
fn spline_preparation_only_allocates_its_output_at_the_size_limit() {
    let mut state = fixture(65_536, 7);
    for size in [1, 2, 3, 64, 1024, 65_536] {
        state.set_zoom_spline(0, 0.25, size);
        perf::assert_churn_budget(1, size * std::mem::size_of::<[f32; 4]>(), || {
            state.set_zoom_spline(0, 0.25, size);
            black_box(&state.zoom_splines[0].coefficients);
        });
        let mut old = fixture(65_536, 7);
        old_set_zoom_spline(&mut old, 0, 0.25, size);
        for (old, new) in old.zoom_splines[0]
            .coefficients
            .iter()
            .zip(&state.zoom_splines[0].coefficients)
        {
            assert_eq!(old.map(f32::to_bits), new.map(f32::to_bits));
        }
    }
}

#[test]
#[ignore = "manual release benchmark; filter benchmark_load_preparation"]
fn benchmark_load_preparation_splines() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for size in [2, 64, 1024, 16_384, 65_536] {
        let source = fixture(size, 7);
        let iterations = (1_000_000 / size).clamp(32, 10_000);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let mut state = source.clone();
            let name = format!("spline_{size}/{}", if old { "old" } else { "new" });
            if old {
                perf::measure_sampled(&name, iterations, size, || {
                    old_set_zoom_spline(black_box(&mut state), 0, black_box(0.25), black_box(size));
                    black_box(&state.zoom_splines[0].coefficients);
                });
            } else {
                perf::measure_sampled(&name, iterations, size, || {
                    black_box(&mut state).set_zoom_spline(0, black_box(0.25), black_box(size));
                    black_box(&state.zoom_splines[0].coefficients);
                });
            }
        }
    }
}
