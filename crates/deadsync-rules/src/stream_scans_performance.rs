use super::*;
use crate::{perf_alloc as allocations, perf_bench as bench};
use std::hint::black_box;

#[path = "stream_scans_original.rs"]
mod original;

fn random(state: &mut u64) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as u32
}

fn check_segments(old: &[original::StreamSegment], new: &[StreamSegment]) {
    assert_eq!(old.len(), new.len());
    for (old, new) in old.iter().zip(new) {
        assert_eq!(
            (old.start(), old.end(), old.is_break()),
            (new.start(), new.end(), new.is_break())
        );
    }
}

fn check_outputs(
    measures: &[u8],
    threshold: Option<usize>,
    constant: bool,
    include: bool,
    mut visit: impl FnMut(u64),
) {
    let (old, before) = allocations::measure(|| {
        original::stream_outputs_full_measures(measures, threshold, constant, include)
    });
    let (new, after) = allocations::measure(|| {
        stream_outputs_full_measures(measures, threshold, constant, include)
    });
    check_segments(&old.counter_segments, &new.counter_segments);
    check_segments(&old.zmod_segments, &new.zmod_segments);
    assert_eq!(
        old.counter_segments.capacity(),
        new.counter_segments.capacity()
    );
    assert_eq!(old.zmod_segments.capacity(), new.zmod_segments.capacity());
    assert_eq!(old.total_stream.to_bits(), new.total_stream.to_bits());
    assert_eq!(old.total_break.to_bits(), new.total_break.to_bits());
    assert!(
        after.allocs <= before.allocs
            && after.reallocs <= before.reallocs
            && after.frees <= before.frees
            && after.allocated_bytes <= before.allocated_bytes
            && after.freed_bytes <= before.freed_bytes
            && after.peak_bytes <= before.peak_bytes
    );
    for segments in [&new.counter_segments, &new.zmod_segments] {
        visit(segments.len() as u64);
        visit(segments.capacity() as u64);
        for segment in segments {
            visit(u64::from(segment.start()));
            visit(u64::from(segment.end()));
            visit(u64::from(segment.is_break()));
        }
    }
    visit(u64::from(new.total_stream.to_bits()));
    visit(u64::from(new.total_break.to_bits()));
}

fn verify_rows<const L: usize>(mut visit: impl FnMut(u64)) {
    for value in 0..=255u8 {
        for column in 0..L {
            let mut row = [b'0'; L];
            row[column] = value;
            let new = density_row_has_step::<L>(&row);
            assert_eq!(new, original::density_row_has_step::<L>(&row));
            assert_eq!(new, row.iter().any(|byte| b"124".contains(byte)));
            visit(u64::from(new));
        }
    }
    for pair in 0..=65535u32 {
        for column in 0..L - 1 {
            let mut row = [b'0'; L];
            row[column] = pair as u8;
            row[column + 1] = (pair >> 8) as u8;
            let new = density_row_has_step::<L>(&row);
            assert_eq!(new, original::density_row_has_step::<L>(&row));
            visit(u64::from(new));
        }
    }
}

#[test]
fn row_classification_preserves_all_bytes_and_adjacent_pairs() {
    verify_rows::<4>(|_| {});
    verify_rows::<5>(|_| {});
    verify_rows::<8>(|_| {});
    verify_rows::<10>(|_| {});
}

fn densities(mode: &str, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| match mode {
            "none" => 0,
            "all" => 32,
            "alternating" => {
                if i % 2 == 0 {
                    32
                } else {
                    0
                }
            }
            _ => {
                if i % 64 < 48 {
                    32
                } else {
                    0
                }
            }
        })
        .collect()
}

fn verify_segments(mut visit: impl FnMut(u64)) {
    for len in 0..=12 {
        for bits in 0..1usize << len {
            let measures: Vec<_> = (0..len)
                .map(|i| if bits & (1 << i) == 0 { 0 } else { 32 })
                .collect();
            for threshold in [0, 1, 16, 32, 255, 256, usize::MAX] {
                let old = original::stream_sequences_threshold(&measures, threshold);
                let new = stream_sequences_threshold(&measures, threshold);
                check_segments(&old, &new);
                assert_eq!(old.capacity(), new.capacity());
                visit(new.len() as u64);
                visit(new.capacity() as u64);
                for segment in new {
                    visit(u64::from(segment.start()));
                    visit(u64::from(segment.end()));
                    visit(u64::from(segment.is_break()));
                }
            }
        }
    }
}

#[test]
fn segment_scans_preserve_exhaustive_short_runs_breaks_and_capacities() {
    verify_segments(|_| {});
}

fn verify_outputs(mut visit: impl FnMut(u64)) {
    for len in 0..=9 {
        for bits in 0..1usize << len {
            let measures: Vec<_> = (0..len)
                .map(|i| if bits & (1 << i) == 0 { 0 } else { 32 })
                .collect();
            for threshold in [
                None,
                Some(0),
                Some(16),
                Some(20),
                Some(24),
                Some(32),
                Some(256),
                Some(usize::MAX),
            ] {
                for constant in [false, true] {
                    for include in [false, true] {
                        check_outputs(&measures, threshold, constant, include, &mut visit);
                    }
                }
            }
        }
    }
    let mut seed = 0x9876_3333_4444_9873;
    for index in 0..1000 {
        let len = index % 257;
        let measures: Vec<_> = (0..len).map(|_| random(&mut seed) as u8).collect();
        for threshold in [Some(16), Some(20), Some(24), Some(32), Some(255)] {
            for constant in [false, true] {
                check_outputs(&measures, threshold, constant, true, &mut visit);
            }
        }
    }
    for mode in ["none", "all", "alternating", "runs"] {
        for len in [16, 64, 257, 2048] {
            let measures = densities(mode, len);
            for constant in [false, true] {
                check_outputs(&measures, Some(16), constant, true, &mut visit);
            }
        }
    }
    for mode in ["none", "all", "alternating", "runs"] {
        let measures = densities(mode, 64);
        for counter_capacity in [0, 1, 4, 7, 64] {
            for zmod_capacity in [0, 1, 4, 7, 64] {
                let old = original::build_stream_outputs(
                    &measures,
                    16,
                    counter_capacity,
                    (16, 1.25, zmod_capacity),
                );
                let new = build_stream_outputs(
                    &measures,
                    16,
                    counter_capacity,
                    (16, 1.25, zmod_capacity),
                );
                check_segments(&old.counter_segments, &new.counter_segments);
                check_segments(&old.zmod_segments, &new.zmod_segments);
                assert_eq!(
                    old.counter_segments.capacity(),
                    new.counter_segments.capacity()
                );
                assert_eq!(old.zmod_segments.capacity(), new.zmod_segments.capacity());
                assert_eq!(old.total_stream.to_bits(), new.total_stream.to_bits());
                assert_eq!(old.total_break.to_bits(), new.total_break.to_bits());
                visit(new.counter_segments.capacity() as u64);
                visit(new.zmod_segments.capacity() as u64);
            }
        }
    }
}

#[test]
fn combined_outputs_preserve_float_bits_vector_capacities_and_allocation_bounds() {
    verify_outputs(|_| {});
}

fn chart(lanes: usize, mode: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for measure in 0..64 {
        for row in 0..64 {
            for column in 0..lanes {
                out.push(if mode != "empty" && column == (row + measure) % lanes {
                    b"124"[row % 3]
                } else {
                    b'0'
                });
            }
            out.push(b'\n');
        }
        out.extend_from_slice(b",\n");
    }
    out
}

fn verify_parser(mut visit: impl FnMut(u64)) {
    let mut seed = 0x8393_abc3_fff2_3321;
    for index in 0..2000 {
        let data: Vec<_> = (0..index % 512)
            .map(|_| {
                let alphabet = b"00124MFLK\n\n\n,;/ \t\r\x0b\x0c\0\xff";
                alphabet[random(&mut seed) as usize % alphabet.len()]
            })
            .collect();
        for lanes in [0, 4, 5, 8, 10, 11] {
            let old = original::measure_densities(&data, lanes);
            let new = measure_densities(&data, lanes);
            assert_eq!(old, new);
            assert_eq!(old.capacity(), new.capacity());
            visit(new.len() as u64);
            for count in new {
                visit(count as u64);
            }
            let old = original::stream_measure_densities(&data, lanes);
            let new = stream_measure_densities(&data, lanes);
            assert_eq!(old, new);
            assert_eq!(old.capacity(), new.capacity());
            for threshold in [0, 16, 32, usize::MAX] {
                assert_eq!(
                    original::stream_run_progress(&data, lanes, threshold, 2),
                    stream_run_progress(&data, lanes, threshold, 2)
                );
            }
        }
    }
    for lanes in [4, 5, 8, 10] {
        for mode in ["empty", "steps"] {
            let data = chart(lanes, mode);
            assert_eq!(
                original::measure_densities(&data, lanes),
                measure_densities(&data, lanes)
            );
            assert_eq!(
                original::stream_measure_densities(&data, lanes),
                stream_measure_densities(&data, lanes)
            );
        }
    }
}

#[test]
fn parser_preserves_malformed_data_lane_fallbacks_and_progress() {
    verify_parser(|_| {});
}

#[test]
#[ignore = "write exact source-comparison corpus"]
fn dump_stream_scans_trace() {
    use std::io::Write;
    let file = std::fs::File::create(
        std::env::var_os("NOTEFIELD_STREAM_SCANS_TRACE_OUT").expect("trace path"),
    )
    .unwrap();
    let mut out = std::io::BufWriter::with_capacity(1024 * 1024, file);
    let mut visit = |value: u64| out.write_all(&value.to_le_bytes()).unwrap();
    verify_rows::<4>(&mut visit);
    verify_rows::<5>(&mut visit);
    verify_rows::<8>(&mut visit);
    verify_rows::<10>(&mut visit);
    verify_segments(&mut visit);
    verify_outputs(&mut visit);
    verify_parser(&mut visit);
    out.flush().unwrap();
}

fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    bench::compare(label, iterations, &mut work);
    for (variant, current) in [("original", false), ("current", true)] {
        let (_, churn) = allocations::measure(|| work(current));
        println!("{label}/{variant}: allocation {churn:?}");
    }
}

fn row_bench<const L: usize>(mode: &str) {
    let rows: Vec<_> = (0..64)
        .map(|i| {
            let mut row = [b'0'; L];
            match mode {
                "first" => row[0] = b"124"[i % 3],
                "last" => row[L - 1] = b"124"[i % 3],
                _ => {}
            }
            row
        })
        .collect();
    compare(&format!("row/{L}/{mode}/64"), 10_000, |current| {
        let call = black_box(if current {
            density_row_has_step::<L>
        } else {
            original::density_row_has_step::<L>
        } as fn(&[u8]) -> bool);
        for row in &rows {
            black_box(call(black_box(row)));
        }
    });
}

#[test]
#[ignore = "paired CPU cycles, throughput and allocation measurements"]
fn benchmark_stream_scans() {
    for lanes in [4, 5, 8, 10] {
        let data = chart(lanes, "steps");
        compare(&format!("density/{lanes}/4096"), 1000, |current| {
            let call = black_box(if current {
                measure_densities
            } else {
                original::measure_densities
            } as fn(&[u8], usize) -> Vec<usize>);
            black_box(call(black_box(&data), black_box(lanes)));
        });
    }
    for mode in ["empty", "first", "last"] {
        row_bench::<4>(mode);
        row_bench::<5>(mode);
        row_bench::<8>(mode);
        row_bench::<10>(mode);
    }
    for mode in ["none", "all", "alternating", "runs"] {
        let measures = densities(mode, 2048);
        compare(&format!("segments/{mode}/2048"), 10_000, |current| {
            if current {
                black_box(stream_sequences_threshold(
                    black_box(&measures),
                    black_box(16),
                ));
            } else {
                black_box(original::stream_sequences_threshold(
                    black_box(&measures),
                    black_box(16),
                ));
            }
        });
    }
    for (name, mode, constant, threshold) in [
        ("all_variable", "all", false, 16),
        ("alternating_variable", "alternating", false, 16),
        ("runs_variable", "runs", false, 16),
        ("all_constant", "all", true, 32),
        ("runs_constant", "runs", true, 32),
        ("different_threshold", "runs", true, 16),
        ("higher_counter_all", "all", false, 32),
        ("higher_counter_runs", "runs", false, 32),
    ] {
        let measures = densities(mode, 2048);
        compare(&format!("outputs/{name}/2048"), 10_000, |current| {
            if current {
                black_box(stream_outputs_full_measures(
                    black_box(&measures),
                    black_box(Some(threshold)),
                    black_box(constant),
                    black_box(true),
                ));
            } else {
                black_box(original::stream_outputs_full_measures(
                    black_box(&measures),
                    black_box(Some(threshold)),
                    black_box(constant),
                    black_box(true),
                ));
            }
        });
    }
}
