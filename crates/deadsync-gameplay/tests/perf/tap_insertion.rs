use super::*;
use crate::perf::{assert_no_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/tap_insertion/baseline.rs"
    ));
}

type Transform = fn(&mut Vec<Note>, &TimingData, usize, usize);

fn transforms() -> [(&'static str, Transform, Transform); 3] {
    [
        ("wide", baseline::apply_wide_insert, apply_wide_insert),
        ("stomp", baseline::apply_stomp_insert, apply_stomp_insert),
        ("echo", baseline::apply_echo_insert, apply_echo_insert),
    ]
}

fn fixture(count: usize, stride: usize, cols: usize, offset: usize) -> Vec<Note> {
    (0..count)
        .map(|i| {
            let row = i * stride;
            let mut note = test_note_at(NoteType::Tap, None, false, row, row as f32 / 48.0);
            note.column = offset + i % cols;
            note
        })
        .collect()
}

fn assert_same(actual: &[Note], expected: &[Note]) {
    // Includes every Note/HoldData/judgment field. All generated fixture floats
    // are finite; textual comparison also distinguishes signed zero.
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
}

#[test]
fn batched_taps_keep_known_spacing_and_echo_chains() {
    let timing = test_timing(480);
    let source = fixture(3, 96, 4, 0);
    let mut wide = source.clone();
    apply_wide_insert(&mut wide, &timing, 0, 4);
    let cells = |notes: &[Note]| {
        notes
            .iter()
            .map(|n| (n.row_index, n.column))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        cells(&wide),
        [(0, 0), (0, 1), (96, 1), (96, 2), (192, 2), (192, 3)]
    );
    let mut stomp = source.clone();
    apply_stomp_insert(&mut stomp, &timing, 0, 4);
    assert_eq!(
        cells(&stomp),
        [(0, 0), (0, 3), (96, 1), (96, 2), (192, 1), (192, 2)]
    );
    let mut echo = source;
    apply_echo_insert(&mut echo, &timing, 0, 4);
    assert_eq!(
        cells(&echo),
        [
            (0, 0),
            (24, 0),
            (48, 0),
            (72, 0),
            (96, 1),
            (120, 1),
            (144, 1),
            (168, 1),
            (192, 2),
            (216, 2),
        ]
    );
    assert!(
        echo.iter()
            .all(|n| n.note_type == NoteType::Tap && n.hold.is_none())
    );
}

#[test]
fn batched_taps_match_parent_across_chart_shapes_and_metadata() {
    let timing = test_timing(4096);
    for cols in [1, 4, 5, 8, 10, MAX_COLS] {
        for offset in [0, 4] {
            for seed in 0..32_u64 {
                let mut rng = seed + 1;
                let mut notes = Vec::new();
                for i in 0..80 {
                    rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let row = i * [12, 24, 48][seed as usize % 3];
                    for item in 0..1 + ((rng >> 32) as usize % 3) {
                        let kind = [
                            NoteType::Tap,
                            NoteType::Lift,
                            NoteType::Hold,
                            NoteType::Roll,
                            NoteType::Mine,
                            NoteType::Fake,
                        ][(rng as usize + item) % 6];
                        let hold = (matches!(kind, NoteType::Hold | NoteType::Roll)
                            && rng & 16 != 0)
                            .then(|| {
                                let mut hold = test_hold();
                                hold.end_row_index = row + (rng >> 40) as usize % 180;
                                hold.end_beat = hold.end_row_index as f32 / 48.0;
                                hold.life = 0.75;
                                hold
                            });
                        let mut note =
                            test_note_at(kind, hold, rng & 8 != 0, row, row as f32 / 48.0);
                        note.column = ((rng >> 40) as usize + item) % (offset + cols + 2);
                        note.quantization_idx = item as u8;
                        note.can_be_judged = rng & 32 != 0;
                        note.result = Some(test_judgment(JudgeGrade::Excellent));
                        notes.push(note);
                    }
                }
                sort_player_notes(&mut notes);
                notes.dedup_by_key(|n| (n.row_index, n.column));
                for (_, old, new) in transforms() {
                    let mut actual = notes.clone();
                    let mut expected = notes.clone();
                    old(&mut expected, &timing, offset, cols);
                    new(&mut actual, &timing, offset, cols);
                    assert_same(&actual, &expected);
                }
            }
        }
    }
}

#[test]
fn batched_taps_preserve_replacements_hold_boundaries_and_foreign_cells() {
    let timing = test_timing(1024);
    for cols in [1, 4, 5] {
        for (_, old, new) in transforms() {
            for fake in [false, true] {
                let mut notes = fixture(8, 96, cols, 4);
                for i in 0..8 {
                    let row = i * 96;
                    let mut hold = test_hold();
                    hold.end_row_index = row + [0, 24, 96, 192][i % 4];
                    hold.end_beat = hold.end_row_index as f32 / 48.0;
                    let mut note =
                        test_note_at(NoteType::Hold, Some(hold), fake, row, row as f32 / 48.0);
                    note.column = 4 + (i + 1) % cols;
                    notes.push(note);
                    let mut foreign = test_note_at(NoteType::Tap, None, false, row + 24, 0.0);
                    foreign.column = 0;
                    notes.push(foreign);
                }
                sort_player_notes(&mut notes);
                notes.dedup_by_key(|n| (n.row_index, n.column));
                let mut expected = notes.clone();
                old(&mut expected, &timing, 4, cols);
                new(&mut notes, &timing, 4, cols);
                assert_same(&notes, &expected);
            }
        }
    }
}

#[test]
fn batched_taps_preserve_compatibility_and_missing_timing() {
    for timing in [test_timing(0), test_timing(48), test_timing(1024)] {
        for cols in [0, 1, 4, MAX_COLS + 1] {
            for shape in 0..4 {
                let mut source = fixture(10, 96, 4, 0);
                if shape == 1 {
                    source.extend(source.clone());
                }
                sort_player_notes(&mut source);
                if shape == 2 {
                    source.reverse();
                }
                if shape == 3 {
                    source.clear();
                }
                for (_, old, new) in transforms() {
                    let mut expected = source.clone();
                    let mut actual = source.clone();
                    old(&mut expected, &timing, 0, cols);
                    new(&mut actual, &timing, 0, cols);
                    assert_same(&actual, &expected);
                }
            }
        }
    }
}

#[test]
fn batched_taps_reuse_capacity_and_skip_unneeded_reservations() {
    let timing = test_timing(50000);
    for (_, _, new) in transforms() {
        let mut notes = fixture(512, 96, 4, 0);
        notes.reserve(notes.len() * 4);
        let pointer = notes.as_ptr();
        let capacity = notes.capacity();
        assert_no_churn(|| new(&mut notes, &timing, 0, 4));
        assert!(notes.len() > 512);
        assert_eq!(notes.as_ptr(), pointer);
        assert_eq!(notes.capacity(), capacity);

        let mut dense = fixture(512, 12, 4, 0);
        let original = dense.clone();
        assert_no_churn(|| new(&mut dense, &timing, 0, 4));
        assert_same(&dense, &original);
        let mut empty = Vec::new();
        assert_no_churn(|| new(&mut empty, &timing, 0, 4));
    }
}

#[test]
fn batched_taps_preserve_saturating_columns_and_timing_flags_in_combinations() {
    let segments = TimingSegments {
        fakes: vec![FakeSegment {
            beat: 2.0,
            length: 3.0,
        }],
        warps: vec![WarpSegment {
            beat: 6.0,
            length: 2.0,
        }],
        ..TimingSegments::default()
    };
    let timing = TimingData::from_segments(0.0, 0.0, &segments, &test_row_to_beat(2048));
    for offset in [0, 4, usize::MAX - 1, usize::MAX] {
        let mut actual = fixture(16, 96, 1, offset);
        let mut expected = actual.clone();
        // Multiple transforms consume each other's generated/replaced notes.
        for (_, old, new) in transforms().into_iter().cycle().take(6) {
            old(&mut expected, &timing, offset, 4);
            new(&mut actual, &timing, offset, 4);
            assert_same(&actual, &expected);
        }
    }
    for (_, _, new) in transforms().into_iter().take(2) {
        let mut notes = fixture(64, 96, 1, 0);
        // Wide/Stomp in a one-lane field only replace existing taps.
        assert_no_churn(|| new(&mut notes, &timing, 0, 1));
        assert_eq!(notes.len(), 64);
    }
}

#[test]
#[ignore = "manual paired release benchmark; run alone with --nocapture --test-threads=1"]
fn tap_insertion_bench() {
    let timing = test_timing(4096 * 96 + 192);
    let cases = [
        ("empty", 0, 96, 20000),
        ("tiny", 4, 96, 10000),
        ("sparse_512", 512, 96, 100),
        ("sparse_4096", 4096, 96, 10),
        ("dense_4096", 4096, 12, 100),
        ("mixed_512", 512, 96, 100),
        ("duplicates_512", 512, 96, 100),
        ("reverse_128", 128, 96, 50),
    ];
    for (transform, old, new) in transforms() {
        for (case, count, stride, iterations) in cases {
            let mut source = fixture(count, stride, 4, 0);
            if case == "mixed_512" {
                for (i, note) in source.iter_mut().enumerate() {
                    if i % 5 == 0 {
                        note.note_type = NoteType::Hold;
                        let mut hold = test_hold();
                        hold.end_row_index = note.row_index + 144;
                        hold.end_beat = hold.end_row_index as f32 / 48.0;
                        note.hold = Some(hold);
                    } else if i % 7 == 0 {
                        note.is_fake = true;
                    }
                }
            } else if case == "duplicates_512" {
                source.push(source[count / 2].clone());
                sort_player_notes(&mut source);
            } else if case == "reverse_128" {
                source.reverse();
            }
            let mut expected = source.clone();
            let mut actual = source.clone();
            old(&mut expected, &timing, 0, 4);
            new(&mut actual, &timing, 0, 4);
            assert_same(&actual, &expected);
            let mut variants = [("old", old), ("new", new)];
            if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
                variants.reverse();
            }
            for (version, function) in variants {
                measure_sampled(
                    &format!("{transform}_{case}_{version}"),
                    iterations,
                    source.len().max(1),
                    || {
                        // Both variants receive the same fresh owned input. Source
                        // cloning and output destruction are included in both.
                        let mut notes = black_box(&source).clone();
                        function(black_box(&mut notes), black_box(&timing), 0, 4);
                        black_box(notes)
                    },
                );
            }
        }
    }
}
