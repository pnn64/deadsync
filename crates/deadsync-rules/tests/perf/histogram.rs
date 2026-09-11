use super::*;
use std::hint::black_box;

type Smoother = fn(&HistCounts, i32) -> Vec<(i32, f32)>;
type HistogramBuilder = fn(&[Note]) -> HistogramMs;

#[test]
fn smoothing_allocates_only_its_output() {
    for dense in [false, true] {
        let counts = counts_fixture(180, dense);
        crate::perf::assert_churn_budget(1, 361 * size_of::<(i32, f32)>(), || {
            assert_eq!(smooth_hist_counts(&counts, 180).len(), 361);
        });
    }
}

fn counts_fixture(radius: i32, dense: bool) -> HistCounts {
    let mut counts = HistCounts {
        min_bin: -radius,
        ..HistCounts::default()
    };
    for bin in -radius..=radius {
        let count = if bin % 7 == 0 {
            (bin.unsigned_abs() % 13) + 1
        } else {
            0
        };
        if dense {
            counts.dense.push(count);
        }
        if count != 0 {
            counts.bins.push((bin, count));
        }
        counts.max_count = counts.max_count.max(count);
    }
    counts
}

fn assert_smoothing_equal(actual: &[(i32, f32)], expected: &[(i32, f32)]) {
    assert_eq!(actual.len(), expected.len());
    for ((a_bin, a), (b_bin, b)) in actual.iter().zip(expected) {
        assert_eq!(a_bin, b_bin);
        assert_eq!(a.to_bits(), b.to_bits(), "bin {a_bin}");
    }
}

#[test]
fn smoothing_is_bit_exact_for_empty_dense_sparse_and_clamped_edges() {
    for radius in [0, 1, 2, 3, 7, 42, 180, 513] {
        for dense in [false, true] {
            let counts = counts_fixture(radius, dense);
            for window in [-1, 0, 1, 2, 3, 4, 21, 180, 900] {
                assert_smoothing_equal(
                    &smooth_hist_counts(&counts, window),
                    &legacy_smooth_hist_counts(&counts, window),
                );
            }
        }
    }
    for window in [-1, 0, 1, 180] {
        assert_smoothing_equal(
            &smooth_hist_counts(&HistCounts::default(), window),
            &legacy_smooth_hist_counts(&HistCounts::default(), window),
        );
    }
}

#[test]
fn histogram_builder_preserves_judgment_metadata_and_smoothed_bits() {
    let mut notes = Vec::new();
    for row in 0..2048 {
        let grade = if row % 17 == 0 {
            JudgeGrade::Miss
        } else {
            JudgeGrade::Excellent
        };
        for column in 0..(1 + row % 4) {
            notes.push(test_note(
                row,
                column,
                grade,
                ((row % 121) as f32 - 60.0) * 0.25,
            ));
        }
    }
    let actual = build_histogram_ms(&notes);
    let expected = legacy_build_histogram_ms(&notes);
    assert_eq!(actual.bins, expected.bins);
    assert_eq!(actual.max_count, expected.max_count);
    assert_eq!(
        actual.worst_observed_ms.to_bits(),
        expected.worst_observed_ms.to_bits()
    );
    assert_eq!(
        actual.worst_window_ms.to_bits(),
        expected.worst_window_ms.to_bits()
    );
    assert_smoothing_equal(&actual.smoothed, &expected.smoothed);
}

#[test]
#[ignore = "manual release comparison; run serially with --nocapture"]
fn histogram_bench() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (radius, dense) in [(42, true), (180, true), (900, false)] {
        let counts = counts_fixture(radius, dense);
        let mut routines: [(&str, Smoother); 2] = [
            ("old", legacy_smooth_hist_counts),
            ("new", smooth_hist_counts),
        ];
        if reverse {
            routines.reverse();
        }
        for (label, routine) in routines {
            crate::perf::measure_sampled(
                &format!("smooth_{radius}_{dense}_{label}"),
                512,
                (radius * 2 + 1) as usize,
                || routine(black_box(&counts), black_box(radius)),
            );
        }
    }
    for size in [128, 8192] {
        let notes = (0..size)
            .map(|row| {
                test_note(
                    row,
                    0,
                    JudgeGrade::Excellent,
                    ((row % 121) as f32 - 60.0) * 0.25,
                )
            })
            .collect::<Vec<_>>();
        let mut routines: [(&str, HistogramBuilder); 2] = [
            ("old", legacy_build_histogram_ms),
            ("new", build_histogram_ms),
        ];
        if reverse {
            routines.reverse();
        }
        for (label, routine) in routines {
            crate::perf::measure_sampled(&format!("histogram_{size}_{label}"), 256, size, || {
                routine(black_box(&notes))
            });
        }
    }
}

// Frozen 0.5.1133 smoothing and full builder; counting routines stay shared.
fn legacy_smooth_hist_counts(counts: &HistCounts, worst_window_bin: i32) -> Vec<(i32, f32)> {
    let mut smoothed = Vec::with_capacity((worst_window_bin * 2 + 1).max(1) as usize);
    for bin in -worst_window_bin..=worst_window_bin {
        let mut y = 0.0_f32;
        for (offset, weight) in (-3..=3).zip(GAUSS7) {
            let sample = (bin + offset).clamp(-worst_window_bin, worst_window_bin);
            y = (hist_count_at(counts, sample) as f32).mul_add(weight, y);
        }
        smoothed.push((bin, y));
    }
    smoothed
}

fn legacy_build_histogram_ms(notes: &[Note]) -> HistogramMs {
    let mut fast_counts = [0u32; FAST_HIST_BINS];
    let scan = scan_hist_bins(notes, &mut fast_counts);
    let counts = count_hist_bins(notes, scan, &fast_counts);
    let worst_window_ms = effective_windows_ms()[scan.meta.worst_window_ix];
    let worst_window_bin = (worst_window_ms / HIST_BIN_MS).round() as i32;
    let smoothed = legacy_smooth_hist_counts(&counts, worst_window_bin);

    HistogramMs {
        bins: counts.bins,
        smoothed,
        max_count: counts.max_count,
        worst_observed_ms: (scan.meta.worst_observed_bin_abs as f32) * HIST_BIN_MS,
        worst_window_ms: worst_window_ms.max(scan.meta.max_abs),
    }
}
