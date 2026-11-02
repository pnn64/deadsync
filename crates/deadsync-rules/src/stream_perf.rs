use super::*;
use std::hint::black_box;
#[path = "../../../tests/perf/stream_alloc.rs"]
mod alloc;
#[path = "stream_original.rs"]
mod original;
#[path = "../../../tests/perf/stream_support.rs"]
mod support;

fn chart_data(lanes: usize, densities: &[usize], crlf: bool, decorated: bool) -> Vec<u8> {
    let mut data = Vec::new();
    for (measure, &density) in densities.iter().enumerate() {
        if decorated {
            data.extend_from_slice(b" // ignored 1111,;\n");
        }
        for row in 0..density.max(16) {
            if decorated {
                data.extend_from_slice(b" \t");
            }
            for column in 0..lanes {
                data.push(if row < density && column == (row + measure) % lanes {
                    b'1'
                } else {
                    b'0'
                });
            }
            if crlf {
                data.push(b'\r');
            }
            data.push(b'\n');
        }
        data.extend_from_slice(if measure + 1 == densities.len() {
            b";\n"
        } else {
            b",\n"
        });
    }
    data
}

#[test]
fn fixed_row_scan_preserves_irregular_lines_and_lane_fallbacks() {
    let fixtures: &[&[u8]] = &[
        b"",
        b"100\r\n",
        b"10\n1\n",
        b"\n1000\n",
        b"1000\n",
        b"1000",
        b"\n \t\r\n,\n\r\n 1000\n   \r\n;\n",
        b",\n;\n1000\n",
        b"//1111,;\n1000\n;ignored",
        b" \t1000\r\n\x0b\x0c2000\n;\n",
        b"1\0\xff0\n",
        b"10000\n",
        b"1\n00\n100\n1000\n10000\n,\n;\n",
        b"1,;0\n",
    ];
    let check = |data: &[u8], lanes| {
        assert_eq!(
            original::measure_densities(data, lanes),
            measure_densities(data, lanes),
            "{data:?}, lanes={lanes}"
        );
        assert_eq!(
            original::stream_measure_densities(data, lanes),
            stream_measure_densities(data, lanes),
            "{data:?}, lanes={lanes}"
        );
    };
    for lanes in [0, 3, 4, 5, 8, 10, 12] {
        for &data in fixtures {
            check(data, lanes);
        }
    }
    let alphabet = b"000000011234MLFK/;, \t\r\n\n\n\x0b\x0c\0\xff";
    let mut seed = 2718u64;
    for len in 0..384 {
        for _ in 0..5 {
            let data: Vec<_> = (0..len)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    alphabet[seed as usize % alphabet.len()]
                })
                .collect();
            for lanes in [0, 4, 5, 8, 10, 11] {
                check(&data, lanes);
            }
        }
    }
    for lanes in [4, 5, 8, 10] {
        for crlf in [false, true] {
            for decorated in [false, true] {
                let densities = [0, 1, 15, 16, 17, 31, 32, 192];
                let data = chart_data(lanes, &densities, crlf, decorated);
                assert_eq!(measure_densities(&data, lanes), densities);
                assert_eq!(
                    stream_measure_densities(&data, lanes),
                    [0, 1, 15, 16, 17, 31, 32, 32]
                );
                check(&data, lanes);
            }
        }
    }
}

#[test]
fn capped_progress_preserves_thresholds_run_bounds_and_early_end() {
    for lanes in [4, 5, 8, 10] {
        for crlf in [false, true] {
            for densities in [
                vec![],
                vec![0; 12],
                vec![192; 12],
                vec![0, 16, 17, 32, 15, 16, 193, 0, 1, 24, 32, 64],
            ] {
                let data = chart_data(lanes, &densities, crlf, false);
                for threshold in [0, 1, 16, 24, 32, 128, 255, usize::MAX] {
                    for current in 0..densities.len() + 3 {
                        assert_eq!(
                            original::stream_run_progress(&data, lanes, threshold, current),
                            stream_run_progress(&data, lanes, threshold, current),
                            "lanes={lanes}, threshold={threshold}, current={current}, densities={densities:?}"
                        );
                    }
                }
            }
        }
    }
    let data = chart_data(4, &[0, 16, 192, 24, 0, 16], false, false);
    assert_eq!(stream_run_progress(&data, 4, 16, 2), Some((2, 3)));
    assert_eq!(stream_run_progress(&data, 4, 16, 4), None);
    assert_eq!(stream_run_progress(b";\n1111\n", 4, 1, 0), None);
}

#[test]
fn counter_segments_preserve_breaks_and_avoid_empty_allocation() {
    let mut seed = 17u64;
    for len in [0, 1, 2, 3, 16, 64, 257, 2048] {
        for pattern in 0..8 {
            let measures: Vec<_> = (0..len)
                .map(|i| match pattern {
                    0 => 0,
                    1 => 15,
                    2 => 16,
                    3 => 32,
                    4 => {
                        if i % 2 == 0 {
                            16
                        } else {
                            0
                        }
                    }
                    5 => {
                        if i % 8 < 4 {
                            16
                        } else {
                            0
                        }
                    }
                    6 => {
                        if i + 1 == len {
                            16
                        } else {
                            0
                        }
                    }
                    _ => {
                        seed ^= seed << 13;
                        seed ^= seed >> 7;
                        seed ^= seed << 17;
                        (seed % 256) as u8
                    }
                })
                .collect();
            for threshold in [0, 1, 16, 24, 32, 256, usize::MAX] {
                let (old, old_churn) =
                    alloc::measure(|| original::stream_sequences_threshold(&measures, threshold));
                let (new, new_churn) =
                    alloc::measure(|| stream_sequences_threshold(&measures, threshold));
                assert_eq!(old, new);
                assert!(new_churn.allocs <= old_churn.allocs);
                assert!(new_churn.reallocs <= old_churn.reallocs);
                assert!(new_churn.allocated_bytes <= old_churn.allocated_bytes);
                if new.is_empty() {
                    assert_eq!(new_churn.allocs, 0);
                }
                assert_eq!(
                    stream_outputs_full_measures(&measures, Some(threshold), false, false)
                        .counter_segments,
                    old
                );
            }
        }
    }
    let segs = stream_sequences_threshold(&[0, 0, 16, 16, 0, 16, 0, 0], 16);
    assert_eq!(
        segs.iter()
            .map(|s| (s.start(), s.end(), s.is_break()))
            .collect::<Vec<_>>(),
        [(0, 2, true), (2, 4, false), (5, 6, false), (6, 8, true)]
    );
}

fn uncapped_progress_impl<const LANES: usize>(
    data: &[u8],
    threshold: usize,
    current: usize,
) -> Option<(usize, usize)> {
    let mut progress = StreamProgress::new(threshold, current);
    for_each_measure_density::<LANES>(data, None, |density| progress.record(density));
    progress.finish()
}
fn uncapped_progress(
    data: &[u8],
    lanes: usize,
    threshold: usize,
    current: usize,
) -> Option<(usize, usize)> {
    match lanes {
        5 => uncapped_progress_impl::<5>(data, threshold, current),
        8 => uncapped_progress_impl::<8>(data, threshold, current),
        10 => uncapped_progress_impl::<10>(data, threshold, current),
        _ => uncapped_progress_impl::<4>(data, threshold, current),
    }
}

fn compare<T: std::fmt::Debug + PartialEq>(
    name: &str,
    iterations: usize,
    mut old: impl FnMut() -> T,
    mut new: impl FnMut() -> T,
) {
    let (a, old_churn) = alloc::measure(&mut old);
    let (b, new_churn) = alloc::measure(&mut new);
    assert_eq!(a, b, "{name}");
    assert!(new_churn.allocs <= old_churn.allocs, "{name}");
    assert!(new_churn.reallocs <= old_churn.reallocs, "{name}");
    assert!(
        new_churn.allocated_bytes <= old_churn.allocated_bytes,
        "{name}"
    );
    drop((a, b));
    println!("ALLOC {name}: original {old_churn:?}, current {new_churn:?}");
    support::compare(
        name,
        iterations,
        || {
            black_box(old());
        },
        || {
            black_box(new());
        },
    );
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_stream_paths() {
    let old_density = black_box(original::stream_measure_densities as fn(&[u8], usize) -> Vec<u8>);
    let new_density = black_box(stream_measure_densities as fn(&[u8], usize) -> Vec<u8>);
    let old_progress = black_box(
        original::stream_run_progress as fn(&[u8], usize, usize, usize) -> Option<(usize, usize)>,
    );
    let new_progress =
        black_box(stream_run_progress as fn(&[u8], usize, usize, usize) -> Option<(usize, usize)>);
    let no_cap =
        black_box(uncapped_progress as fn(&[u8], usize, usize, usize) -> Option<(usize, usize)>);
    for lanes in [4, 5, 8, 10] {
        for (label, density, crlf) in [
            ("sparse", 0, false),
            ("stream", 16, false),
            ("dense", 192, false),
            ("crlf", 16, true),
        ] {
            let data = chart_data(lanes, &vec![density; 256], crlf, false);
            compare(
                &format!("density-{lanes}-{label}"),
                1,
                || old_density(black_box(&data), black_box(lanes)),
                || new_density(black_box(&data), black_box(lanes)),
            );
            if !crlf {
                compare(
                    &format!("progress-{lanes}-{label}"),
                    1,
                    || {
                        old_progress(
                            black_box(&data),
                            black_box(lanes),
                            black_box(16),
                            black_box(128),
                        )
                    },
                    || {
                        new_progress(
                            black_box(&data),
                            black_box(lanes),
                            black_box(16),
                            black_box(128),
                        )
                    },
                );
            }
            if label == "stream" || label == "dense" {
                assert_eq!(
                    no_cap(&data, lanes, 16, 128),
                    new_progress(&data, lanes, 16, 128)
                );
                support::compare(
                    &format!("isolate-cap-{lanes}-{label}"),
                    1,
                    || {
                        black_box(no_cap(
                            black_box(&data),
                            black_box(lanes),
                            black_box(16),
                            black_box(128),
                        ));
                    },
                    || {
                        black_box(new_progress(
                            black_box(&data),
                            black_box(lanes),
                            black_box(16),
                            black_box(128),
                        ));
                    },
                );
            }
        }
    }
    for (label, data) in [
        ("empty", vec![]),
        ("ragged", b"10\n1\n100\r\n00\n1000\n,\n;\n".repeat(64)),
        ("decorated", chart_data(4, &vec![16; 256], true, true)),
    ] {
        compare(
            &format!("density-control-{label}"),
            1,
            || old_density(black_box(&data), black_box(4)),
            || new_density(black_box(&data), black_box(4)),
        );
    }
    for (label, data, current) in [
        ("empty", vec![], 0),
        ("outside", chart_data(4, &vec![16; 256], false, false), 300),
        ("crlf", chart_data(4, &vec![192; 256], true, false), 128),
    ] {
        compare(
            &format!("progress-control-{label}"),
            1,
            || {
                old_progress(
                    black_box(&data),
                    black_box(4),
                    black_box(16),
                    black_box(current),
                )
            },
            || {
                new_progress(
                    black_box(&data),
                    black_box(4),
                    black_box(16),
                    black_box(current),
                )
            },
        );
    }
    let old_segments =
        black_box(original::stream_sequences_threshold as fn(&[u8], usize) -> Vec<StreamSegment>);
    let new_segments =
        black_box(stream_sequences_threshold as fn(&[u8], usize) -> Vec<StreamSegment>);
    for (label, data) in [
        ("empty", vec![]),
        ("none", vec![0; 256]),
        ("below", vec![15; 256]),
        ("all", vec![16; 256]),
        (
            "alternating",
            (0..256).map(|i| if i % 2 == 0 { 16 } else { 0 }).collect(),
        ),
        (
            "mixed",
            (0..256).map(|i| if i % 8 < 4 { 16 } else { 0 }).collect(),
        ),
        (
            "delayed",
            (0..256)
                .map(|i| if (200..250).contains(&i) { 16 } else { 0 })
                .collect(),
        ),
        ("short", vec![0, 16, 0, 0]),
    ] {
        compare(
            &format!("segments-{label}"),
            100,
            || old_segments(black_box(&data), black_box(16)),
            || new_segments(black_box(&data), black_box(16)),
        );
    }
}
