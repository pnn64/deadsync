use super::*;
use std::hint::black_box;

#[allow(dead_code)]
#[path = "histogram_storage/baseline.rs"]
mod baseline;

fn note(row: usize, column: usize, grade: JudgeGrade, offset: f32) -> Note {
    Note {
        beat: row as f32 / 48.0,
        quantization_idx: 0,
        column,
        note_type: NoteType::Tap,
        row_index: row,
        result: Some(Judgment {
            time_error_ms: offset,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(offset, 1.0),
            grade,
            window: None,
            miss_because_held: false,
        }),
        early_result: None,
        hold: None,
        mine_result: None,
        is_fake: false,
        can_be_judged: true,
    }
}

fn assert_points(actual: &[(i32, f32)], expected: &[(i32, f32)]) {
    assert_eq!(actual.len(), expected.len());
    for (&(a_bin, a), &(b_bin, b)) in actual.iter().zip(expected) {
        assert_eq!(a_bin, b_bin);
        assert_eq!(a.to_bits(), b.to_bits(), "bin {a_bin}");
    }
}

fn assert_histogram(actual: &HistogramMs, expected: &HistogramMs) {
    assert_eq!(actual.bins, expected.bins);
    assert_eq!(actual.max_count, expected.max_count);
    assert_eq!(
        actual.worst_window_ms.to_bits(),
        expected.worst_window_ms.to_bits()
    );
    assert_eq!(
        actual.worst_observed_ms.to_bits(),
        expected.worst_observed_ms.to_bits()
    );
    assert_points(&actual.smoothed, &expected.smoothed);
}

fn histogram(bins: Vec<(i32, u32)>, window: f32) -> HistogramMs {
    HistogramMs {
        max_count: bins.iter().map(|&(_, count)| count).max().unwrap_or(0),
        bins,
        smoothed: Vec::new(),
        worst_observed_ms: window,
        worst_window_ms: window,
    }
}

#[test]
fn counting_preserves_rows_flags_bins_and_metadata_at_storage_boundaries() {
    let offsets = [
        -6000.5, -512.01, -512.0, -511.99, -1.01, -0.0, 0.0, 0.99, 511.99, 512.0, 600.0, 6000.5,
    ];
    let grades = [
        JudgeGrade::Fantastic,
        JudgeGrade::Excellent,
        JudgeGrade::Great,
        JudgeGrade::Decent,
        JudgeGrade::WayOff,
        JudgeGrade::Miss,
    ];
    for length in [0, 1, 7, 128, 8192] {
        for mode in 0..3 {
            let mut notes = Vec::new();
            for row in 0..length {
                for col in 0..=row % 3 {
                    let offset = match mode {
                        0 => (row % 121) as f32 - 60.5,
                        1 => 600.0 + (row % 121) as f32,
                        _ => offsets[(row + col) % offsets.len()],
                    };
                    let mut n = note(row, col, grades[(row + col) % grades.len()], offset);
                    n.is_fake = row % 23 == 5;
                    n.can_be_judged = row % 29 != 7;
                    n.note_type = [
                        NoteType::Tap,
                        NoteType::Mine,
                        NoteType::Lift,
                        NoteType::Hold,
                        NoteType::Roll,
                    ][(row + col) % 5];
                    if row % 31 == 8 {
                        n.result = None;
                    }
                    notes.push(n);
                }
            }
            assert_histogram(
                &build_histogram_ms(&notes),
                &baseline::build_histogram_ms(&notes),
            );
        }
    }
}

#[test]
fn merges_preserve_duplicates_unsorted_bins_zero_counts_and_span_boundaries() {
    assert_histogram(
        &merge_histograms_ms(&[]),
        &baseline::merge_histograms_ms(&[]),
    );
    for span in [1, 2, 1023, 1024, 1025, 4095, 4096, 4097, 20001] {
        for start in [-20000, -512, 0, 512, 20000] {
            let end = start + span - 1;
            let fixtures = [
                histogram(vec![(end, 3), (start, 0), (start, 2), (end, 1)], 180.0),
                histogram(Vec::new(), 320.0),
                histogram(
                    vec![(start, 16_777_219), (end, 7), (start + span / 2, 11)],
                    42.0,
                ),
            ];
            assert_histogram(
                &merge_histograms_ms(&fixtures),
                &baseline::merge_histograms_ms(&fixtures),
            );
            for selected in 0..3 {
                let iter = fixtures
                    .iter()
                    .enumerate()
                    .filter_map(|(index, hist)| (index != selected).then_some(hist));
                assert_histogram(
                    &merge_histograms_ms_iter(iter.clone()),
                    &baseline::merge_histograms_ms_iter(iter),
                );
            }
        }
    }
    let extremes = [histogram(vec![(i32::MAX, 2), (i32::MIN, 3)], f32::NAN)];
    assert_histogram(
        &merge_histograms_ms(&extremes),
        &baseline::merge_histograms_ms(&extremes),
    );
    let zeros = [histogram(vec![(-9000, 0), (9000, 0), (9000, 0)], 180.0)];
    assert_histogram(
        &merge_histograms_ms(&zeros),
        &baseline::merge_histograms_ms(&zeros),
    );
}

fn counts(radius: i32, dense: bool) -> (HistCounts<'static>, baseline::HistCounts) {
    let bins: Vec<_> = (-radius..=radius)
        .filter(|bin| bin % 7 == 0)
        .map(|bin| (bin, bin.unsigned_abs() % 23 + 1))
        .collect();
    let values = if dense {
        (-radius..=radius)
            .map(|bin| {
                if bin % 7 == 0 {
                    bin.unsigned_abs() % 23 + 1
                } else {
                    0
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    (
        HistCounts {
            bins: bins.clone(),
            dense: Cow::Owned(values.clone()),
            min_bin: -radius,
            max_count: 23,
        },
        baseline::HistCounts {
            bins,
            dense: values,
            min_bin: -radius,
            max_count: 23,
        },
    )
}

#[test]
fn monotonic_sparse_smoothing_keeps_fma_bits_and_repeated_clamped_samples() {
    for radius in [0, 1, 3, 42, 180, 512, 2048, 10000] {
        for dense in [false, true] {
            let (new, old) = counts(radius, dense);
            for window in [-1, 0, 1, 2, 3, 4, 42, 180, 513, 900] {
                assert_points(
                    &smooth_hist_counts(&new, window),
                    &baseline::smooth_hist_counts(&old, window),
                );
            }
        }
    }
    let new = HistCounts {
        bins: vec![
            (-20000, 2),
            (-3, 16_777_219),
            (0, u32::MAX),
            (3, 11),
            (20000, 1),
        ],
        ..HistCounts::default()
    };
    let old = baseline::HistCounts {
        bins: new.bins.clone(),
        ..baseline::HistCounts::default()
    };
    for window in [0, 1, 3, 4, 42] {
        assert_points(
            &smooth_hist_counts(&new, window),
            &baseline::smooth_hist_counts(&old, window),
        );
    }
}

#[test]
fn ordinary_builds_and_dense_or_sparse_merges_allocate_only_final_outputs() {
    let notes: Vec<_> = (0..128)
        .map(|row| note(row, 0, JudgeGrade::Excellent, (row % 121) as f32 - 60.0))
        .collect();
    crate::perf::assert_churn_budget(2, 16384, || {
        black_box(build_histogram_ms(&notes));
    });
    for span in [121, 1024, 4097, 20001] {
        let histograms = [
            histogram(vec![(0, 1), (span - 1, 2)], 180.0),
            histogram(vec![(0, 3), (span - 1, 4)], 180.0),
        ];
        crate::perf::assert_churn_budget(2, 32768, || {
            black_box(merge_histograms_ms(&histograms));
        });
    }
    crate::perf::assert_no_churn(|| {
        black_box(merge_histograms_ms(&[]));
    });
}

#[test]
fn returned_outputs_survive_later_calls_and_input_changes() {
    let mut input = [histogram(vec![(-60, 1), (60, 2)], 180.0)];
    let first = merge_histograms_ms(&input);
    let expected = baseline::merge_histograms_ms(&input);
    for n in 0..100 {
        input[0].bins = vec![(-n, n as u32), (n, 7)];
        assert_histogram(
            &merge_histograms_ms(&input),
            &baseline::merge_histograms_ms(&input),
        );
        assert_histogram(&first, &expected);
    }
}

fn pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    let mut old =
        || crate::perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut old);
    let mut new =
        || crate::perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut new);
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        new();
        old();
    } else {
        old();
        new();
    }
}

#[test]
#[ignore = "manual release comparison; --ignored --test-threads=1 --nocapture"]
fn benchmark_histogram_storage() {
    for size in [0, 1, 128, 8192] {
        for mode in ["fast", "shifted", "sparse"] {
            let notes: Vec<_> = (0..size)
                .map(|row| {
                    let offset = match mode {
                        "fast" => (row % 121) as f32 - 60.0,
                        "shifted" => (row % 121) as f32 + 600.0,
                        _ => ((row % 13) as f32 - 6.0) * 1000.0,
                    };
                    note(row, 0, JudgeGrade::Excellent, offset)
                })
                .collect();
            assert_histogram(
                &build_histogram_ms(&notes),
                &baseline::build_histogram_ms(&notes),
            );
            pair(
                &format!("build_{mode}_{size}"),
                if size > 128 { 64 } else { 512 },
                size.max(1),
                || baseline::build_histogram_ms(black_box(&notes)),
                || build_histogram_ms(black_box(&notes)),
            );
        }
    }
    for stages in [0, 1, 8, 64] {
        for (kind, radius, stride) in [
            ("dense", 180i32, 1),
            ("wide_dense", 1024, 7),
            ("sparse", 9000, 1000),
        ] {
            let histograms: Vec<_> = (0..stages)
                .map(|stage| {
                    histogram(
                        (-radius..=radius)
                            .step_by(stride)
                            .map(|bin| (bin, (bin.unsigned_abs() as usize + stage) as u32 % 23 + 1))
                            .collect(),
                        radius as f32,
                    )
                })
                .collect();
            assert_histogram(
                &merge_histograms_ms(&histograms),
                &baseline::merge_histograms_ms(&histograms),
            );
            pair(
                &format!("merge_{kind}_{stages}"),
                if radius > 1000 { 32 } else { 256 },
                stages.max(1),
                || baseline::merge_histograms_ms(black_box(&histograms)),
                || merge_histograms_ms(black_box(&histograms)),
            );
        }
    }
    for (radius, dense) in [
        (42, true),
        (180, true),
        (900, true),
        (42, false),
        (180, false),
        (900, false),
        (9000, false),
    ] {
        let (new, old) = counts(radius, dense);
        let kind = if dense { "dense" } else { "sparse" };
        pair(
            &format!("smooth_{kind}_{radius}"),
            if radius > 1000 { 32 } else { 256 },
            (radius * 2 + 1) as usize,
            || baseline::smooth_hist_counts(black_box(&old), black_box(radius)),
            || smooth_hist_counts(black_box(&new), black_box(radius)),
        );
    }
}
