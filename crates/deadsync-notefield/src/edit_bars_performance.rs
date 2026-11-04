use super::*;
use crate::transforms::numeric_performance::allocations;
use std::hint::black_box;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "edit_bars_original.rs"]
mod original;

fn random(state: &mut u64) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as u32
}

fn signatures() -> Vec<TimeSignatureSegment> {
    let values = [
        i32::MIN,
        -100,
        -1,
        0,
        1,
        2,
        3,
        4,
        5,
        7,
        16,
        64,
        1_000_001,
        i32::MAX,
    ];
    let mut out = Vec::new();
    for &numerator in &values {
        for &denominator in &values {
            out.push(TimeSignatureSegment {
                beat: 0.0,
                numerator,
                denominator,
            });
        }
    }
    let mut state = 0x9392_ffff_2281_3221;
    out.extend((0..10_000).map(|_| TimeSignatureSegment {
        beat: f32::from_bits(random(&mut state)),
        numerator: random(&mut state) as i32,
        denominator: random(&mut state) as i32,
    }));
    out
}

fn segments(len: usize, offset: bool) -> Vec<TimeSignatureSegment> {
    (0..len)
        .map(|index| TimeSignatureSegment {
            beat: index as f32 * 4.0
                + if offset && index == 3 {
                    1.0 / 48.0
                } else {
                    0.0
                },
            numerator: [3, 4, 5, 7][index % 4],
            denominator: [4, 8, 16, 2][index % 4],
        })
        .collect()
}

fn state(cursor: &EditBeatBarCursor<'_>) -> (usize, i64, i32, i32, i32, i32) {
    (
        cursor.index,
        cursor.measure_index_before,
        cursor.segment.start_row,
        cursor.segment.step_rows,
        cursor.segment.measure_frequency,
        cursor.next_start_row,
    )
}

fn compare_sequence(
    segments: &[TimeSignatureSegment],
    initial: i32,
    rows: &[i32],
    mut visit: impl FnMut(Option<Option<(u32, Option<i64>)>>, (usize, i64, i32, i32, i32, i32)),
) {
    let mut old = original::EditBeatBarCursor::new(initial, segments);
    let mut new = EditBeatBarCursor::new(initial, segments);
    assert_eq!(original::cursor_state(&old), state(&new));
    for &row in rows {
        let old_result = catch_unwind(AssertUnwindSafe(|| {
            old.info_for_row(row).map(|v| (v.frame, v.measure_index))
        }))
        .ok();
        let new_result = catch_unwind(AssertUnwindSafe(|| {
            new.info_for_row(row).map(|v| (v.frame, v.measure_index))
        }))
        .ok();
        assert_eq!(old_result, new_result, "row {row}");
        assert_eq!(original::cursor_state(&old), state(&new));
        visit(new_result, state(&new));
    }
}

#[test]
fn gcd_preserves_signed_limits_and_generated_pairs() {
    for a in [i32::MIN, -65536, -12, -1, 0, 1, 12, 65536, i32::MAX] {
        for b in [i32::MIN, -65536, -12, -1, 0, 1, 12, 65536, i32::MAX] {
            assert_eq!(gcd(a, b), original::gcd(a, b));
        }
    }
    let mut seed = 0x1275_2364_ffff_a345;
    for _ in 0..500_000 {
        let (a, b) = (random(&mut seed) as i32, random(&mut seed) as i32);
        assert_eq!(gcd(a, b), original::gcd(a, b));
    }
}

#[test]
fn segment_counts_preserve_limits_invalid_signatures_and_rounding() {
    let rows = [
        i32::MIN,
        -1000,
        -1,
        0,
        1,
        2,
        11,
        12,
        13,
        191,
        192,
        193,
        1000,
        i32::MAX,
    ];
    for sig in signatures() {
        for &start in &rows {
            for &end in &rows {
                assert_eq!(
                    bars_in_segment(start, end, sig),
                    original::bars_in_segment(start, end, sig),
                    "{start} {end} {sig:?}"
                );
            }
        }
    }
}

#[test]
fn candidate_step_preserves_empty_regular_and_irregular_arrays() {
    assert_eq!(
        edit_bar_candidate_step_rows(&[]),
        original::edit_bar_candidate_step_rows(&[])
    );
    for len in [1, 2, 4, 16, 64, 513] {
        for offset in [false, true] {
            let input = segments(len, offset);
            assert_eq!(
                edit_bar_candidate_step_rows(&input),
                original::edit_bar_candidate_step_rows(&input)
            );
        }
    }
    for sig in signatures() {
        assert_eq!(
            edit_bar_candidate_step_rows(&[sig]),
            original::edit_bar_candidate_step_rows(&[sig])
        );
    }
    let input = signatures();
    for chunk in input.chunks(37) {
        assert_eq!(
            edit_bar_candidate_step_rows(chunk),
            original::edit_bar_candidate_step_rows(chunk)
        );
    }
}

#[test]
fn cursor_sequences_preserve_info_and_all_cached_state() {
    let rows: Vec<_> = (0..256)
        .flat_map(|i| [i * 48, i * 48 + 1, i * 48 + 12])
        .chain((0..256).rev().map(|i| i * 48))
        .collect();
    for len in [0, 1, 2, 16, 64] {
        let mut input = segments(len, true);
        for index in (0..len).step_by(7) {
            input[index].numerator = 0;
        }
        for initial in [-100, 0, 192, 10_000] {
            compare_sequence(&input, initial, &rows, |_, _| {});
        }
    }
    assert_eq!(
        std::mem::size_of::<EditBeatBarCursor<'_>>(),
        std::mem::size_of::<original::EditBeatBarCursor<'_>>()
    );
}

#[test]
fn cursor_extremes_preserve_existing_panics_and_post_panic_state() {
    let rows = [-1, 0, 1, 100, i32::MAX];
    for beat in [-1.0, f32::NEG_INFINITY, f32::INFINITY, f32::NAN] {
        let input = [TimeSignatureSegment {
            beat,
            numerator: 4,
            denominator: 4,
        }];
        compare_sequence(&input, 0, &rows, |_, _| {});
    }
}

#[test]
#[ignore = "archive exact edit-bar inputs, outputs and cursor state"]
fn dump_edit_bars_trace() {
    let mut out = Vec::new();
    let mut seed = 0x1275_2364_ffff_a345;
    for _ in 0..100_000 {
        let (a, b) = (random(&mut seed) as i32, random(&mut seed) as i32);
        for value in [a, b, gcd(a, b)] {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    for sig in signatures() {
        for (start, end) in [(i32::MIN, i32::MAX), (-1000, 1000), (0, 192), (192, 0)] {
            for value in [start, end, sig.numerator, sig.denominator] {
                out.extend_from_slice(&value.to_le_bytes());
            }
            out.extend_from_slice(&bars_in_segment(start, end, sig).to_le_bytes());
        }
        out.extend_from_slice(&edit_bar_candidate_step_rows(&[sig]).to_le_bytes());
    }
    let rows: Vec<_> = (0..256)
        .flat_map(|i| [i * 48, i * 48 + 1, i * 48 + 12])
        .chain((0..256).rev().map(|i| i * 48))
        .collect();
    for len in [0, 1, 2, 16, 64] {
        let mut input = segments(len, true);
        for index in (0..len).step_by(7) {
            input[index].numerator = 0;
        }
        for initial in [-100, 0, 192, 10_000] {
            compare_sequence(&input, initial, &rows, |result, state| {
                match result {
                    None => out.push(0),
                    Some(None) => out.push(1),
                    Some(Some((frame, index))) => {
                        out.push(2);
                        out.extend_from_slice(&frame.to_le_bytes());
                        out.extend_from_slice(&index.unwrap_or(i64::MIN).to_le_bytes());
                    }
                }
                for value in [
                    state.0 as i64,
                    state.1,
                    i64::from(state.2),
                    i64::from(state.3),
                    i64::from(state.4),
                    i64::from(state.5),
                ] {
                    out.extend_from_slice(&value.to_le_bytes());
                }
            });
        }
    }
    std::fs::write(
        std::env::var_os("NOTEFIELD_EDIT_BARS_TRACE_OUT").expect("trace path"),
        out,
    )
    .unwrap();
}

fn compare(label: &str, mut work: impl FnMut(bool)) {
    let iterations = if label == "candidate/empty" {
        10_000
    } else {
        1_000
    };
    bench::compare(label, iterations, &mut work);
    for (variant, current) in [("original", false), ("current", true)] {
        let (_, churn) = allocations::measure(|| work(current));
        assert_eq!(churn, allocations::Churn::default());
        println!("{label}/{variant}: allocation {churn:?}");
    }
}

#[test]
#[ignore = "paired CPU cycles and allocation measurements"]
fn benchmark_edit_bars() {
    for mode in ["regular", "zeros", "mixed", "limits"] {
        let mut seed = 0x6384_aaaa_9854_3572;
        let pairs: Vec<_> = (0..64)
            .map(|i| match mode {
                "regular" => (12, i * 48),
                "zeros" => (0, i * 48),
                "limits" => (i32::MIN, i32::MAX - i),
                _ => (random(&mut seed) as i32, random(&mut seed) as i32),
            })
            .collect();
        compare(&format!("gcd/{mode}/64"), |current| {
            let call = black_box(if current {
                gcd as fn(i32, i32) -> i32
            } else {
                original::gcd as fn(i32, i32) -> i32
            });
            for &(a, b) in &pairs {
                black_box(call(black_box(a), black_box(b)));
            }
        });
    }
    for mode in ["regular", "irregular", "limits", "empty"] {
        let inputs: Vec<_> = (0..64)
            .map(|i| {
                let sig = TimeSignatureSegment {
                    beat: 0.0,
                    numerator: if mode == "limits" {
                        i32::MAX
                    } else {
                        [3, 4, 5, 7][i as usize % 4]
                    },
                    denominator: if mode == "limits" {
                        i32::MAX
                    } else {
                        [4, 8, 3, 7][i as usize % 4]
                    },
                };
                let (start, end) = match mode {
                    "limits" => (i32::MIN, i32::MAX),
                    "empty" => (100, i),
                    "irregular" => (-i * 37, 13_891 + i * 53),
                    _ => (i * 192, i * 192 + 384),
                };
                (start, end, sig)
            })
            .collect();
        compare(&format!("bars/{mode}/64"), |current| {
            let call = black_box(if current {
                bars_in_segment as fn(i32, i32, TimeSignatureSegment) -> i64
            } else {
                original::bars_in_segment as fn(i32, i32, TimeSignatureSegment) -> i64
            });
            for &(start, end, sig) in &inputs {
                black_box(call(black_box(start), black_box(end), black_box(sig)));
            }
        });
    }
    for (label, input) in [
        ("candidate/regular64", segments(64, false)),
        ("candidate/offset64", segments(64, true)),
        ("candidate/long512", segments(512, true)),
        ("candidate/empty", vec![]),
    ] {
        compare(label, |current| {
            let call = black_box(if current {
                edit_bar_candidate_step_rows as fn(&[TimeSignatureSegment]) -> i32
            } else {
                original::edit_bar_candidate_step_rows as fn(&[TimeSignatureSegment]) -> i32
            });
            black_box(call(black_box(&input)));
        });
    }
    let input = segments(64, false);
    for mode in ["forward", "reverse", "seek", "steady"] {
        let rows: Vec<_> = (0..64)
            .map(|i| match mode {
                "reverse" => (63 - i) * 192,
                "seek" => {
                    if i % 2 == 0 {
                        i * 192
                    } else {
                        (63 - i) * 192
                    }
                }
                "steady" => 96,
                _ => i * 192,
            })
            .collect();
        let initial = if mode == "reverse" { 63 * 192 } else { 0 };
        compare(&format!("cursor/{mode}/64"), |current| {
            if current {
                let mut cursor = EditBeatBarCursor::new(black_box(initial), black_box(&input));
                for &row in &rows {
                    black_box(cursor.info_for_row(black_box(row)));
                }
                black_box(state(&cursor));
            } else {
                let mut cursor =
                    original::EditBeatBarCursor::new(black_box(initial), black_box(&input));
                for &row in &rows {
                    black_box(cursor.info_for_row(black_box(row)));
                }
                black_box(original::cursor_state(&cursor));
            }
        });
    }
}
