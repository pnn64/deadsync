// Reference routine frozen from 5aab5d460 / 0.5.1137.
use super::*;
use std::hint::black_box;

fn legacy_build_crossover_rows<const LANES: usize>(
    notes: &[Note],
    note_range: (usize, usize),
    col_start: usize,
) -> (Vec<[u8; LANES]>, Vec<f32>, Vec<usize>) {
    let (start, end) = note_range;
    let end = end.min(notes.len());
    let start = start.min(end);
    let mut cells = Vec::with_capacity((end - start).saturating_mul(2));
    let mut sequence = 0usize;

    for note in &notes[start..end] {
        if note.column < col_start || note.column - col_start >= LANES {
            continue;
        }
        let Some(ch) = crossover_note_char(note) else {
            continue;
        };
        let lane = note.column - col_start;
        cells.push((note.row_index, sequence, note.beat, lane, ch));
        sequence += 1;
        if let Some(hold) = note.hold.as_ref() {
            cells.push((hold.end_row_index, sequence, hold.end_beat, lane, b'3'));
            sequence += 1;
        }
    }
    cells.sort_unstable_by_key(|&(row_index, sequence, _, _, _)| (row_index, sequence));

    let mut row_arrays = Vec::with_capacity(cells.len());
    let mut row_to_beat = Vec::with_capacity(cells.len());
    let mut row_indices = Vec::with_capacity(cells.len());
    for (row_index, _, beat, lane, ch) in cells {
        if row_indices.last().copied() != Some(row_index) {
            row_arrays.push([b'0'; LANES]);
            row_to_beat.push(beat);
            row_indices.push(row_index);
        }
        apply_crossover_cell(
            row_arrays
                .last_mut()
                .expect("a row is inserted before its cells"),
            lane,
            ch,
        );
    }
    (row_arrays, row_to_beat, row_indices)
}

fn row_fixture(count: usize, hold_every: usize, chord: usize) -> Vec<Note> {
    (0..count)
        .map(|i| {
            let row = (i / chord) * 12;
            let hold = (hold_every != 0 && i % hold_every == 0).then(|| {
                let mut hold = test_hold();
                hold.end_row_index = row + 12 * (1 + i % 19);
                hold.end_beat = hold.end_row_index as f32 / 48.0;
                hold
            });
            let kind = if hold.is_some() {
                NoteType::Hold
            } else {
                NoteType::Tap
            };
            let mut note = test_note_at(kind, hold, false, row, row as f32 / 48.0);
            note.column = i % 4;
            note
        })
        .collect()
}

fn assert_same_rows<const LANES: usize>(notes: &[Note], range: (usize, usize), col_start: usize) {
    let actual = build_crossover_rows::<LANES>(notes, range, col_start);
    let expected = legacy_build_crossover_rows::<LANES>(notes, range, col_start);
    assert_eq!(actual.0, expected.0);
    assert_eq!(actual.2, expected.2);
    assert_eq!(
        actual.1.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        expected.1.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
    );
}

#[test]
fn merged_rows_preserve_cells_beats_and_ranges() {
    for count in [0, 1, 2, 32, 63, 64, 65, 257, 4096] {
        for holds in [0, 1, 7] {
            for chord in [1, 2, 4] {
                let mut notes = row_fixture(count, holds, chord);
                for ordered in [true, false] {
                    if !ordered {
                        notes.reverse();
                    }
                    for range in [
                        (0, count),
                        (count / 3, count / 2),
                        (count, 0),
                        (0, usize::MAX),
                        (usize::MAX, usize::MAX),
                    ] {
                        assert_same_rows::<4>(&notes, range, 0);
                        assert_same_rows::<8>(&notes, range, 2);
                        assert_same_rows::<10>(&notes, range, 0);
                        assert_same_rows::<0>(&notes, range, 0);
                    }
                }
            }
        }
    }
}

#[test]
fn merged_rows_preserve_overlaps_fakes_and_first_beat_bits() {
    let kinds = [
        NoteType::Tap,
        NoteType::Lift,
        NoteType::Hold,
        NoteType::Roll,
        NoteType::Mine,
        NoteType::Fake,
    ];
    let mut state = 95643321_u64;
    for seed in 0..64 {
        let mut notes = Vec::new();
        for i in 0..128 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let value = (state >> 32) as usize;
            let row = value % 16;
            let hold = (value.is_multiple_of(3)).then(|| {
                let mut hold = test_hold();
                // Includes zero-length tails, backward tails and crossing holds.
                hold.end_row_index = (value / 17) % 20;
                hold.end_beat = f32::from_bits(0x7fc0_0000 + i as u32);
                hold
            });
            let mut note = test_note_at(
                kinds[value % kinds.len()],
                hold,
                value.is_multiple_of(7),
                row,
                if i % 5 == 0 { -0.0 } else { i as f32 },
            );
            note.column = value % 12;
            note.can_be_judged = value.is_multiple_of(2);
            notes.push(note);
        }
        if seed % 2 == 0 {
            notes.sort_by_key(|note| note.row_index);
        }
        assert_same_rows::<4>(&notes, (0, notes.len()), seed % 5);
        assert_same_rows::<8>(&notes, (3, 120), 4);
        assert_same_rows::<16>(&notes, (0, notes.len()), 0);
        assert_same_rows::<4>(&notes, (0, notes.len()), usize::MAX);
    }
}

#[test]
fn ordered_rows_allocate_only_exact_outputs_and_tails() {
    let taps = row_fixture(4096, 0, 4);
    crate::perf::assert_churn_budget(3, 1024 * 16, || {
        black_box(build_crossover_rows::<4>(&taps, (0, taps.len()), 0));
    });
    let holds = row_fixture(4096, 7, 4);
    let result = build_crossover_rows::<4>(&holds, (0, holds.len()), 0);
    assert_eq!(result.0.len(), result.0.capacity());
    assert_eq!(result.1.len(), result.1.capacity());
    assert_eq!(result.2.len(), result.2.capacity());
    crate::perf::assert_churn_budget(4, 50_000, || {
        black_box(build_crossover_rows::<4>(&holds, (0, holds.len()), 0));
    });
    crate::perf::assert_no_churn(|| {
        black_box(build_crossover_rows::<4>(&taps, (0, 0), 0));
        black_box(build_crossover_rows::<4>(&taps, (0, taps.len()), 4));
    });
}

#[test]
#[ignore = "manual old/new release benchmark"]
fn crossover_rows_bench() {
    for (label, count, holds, chord, ordered) in [
        ("small", 32, 7, 1, true),
        ("taps", 4096, 0, 1, true),
        ("chords", 4096, 0, 4, true),
        ("mixed", 4096, 7, 2, true),
        ("holds", 4096, 1, 2, true),
        ("large", 16384, 7, 2, true),
        ("unordered", 4096, 7, 2, false),
    ] {
        let mut notes = row_fixture(count, holds, chord);
        if !ordered {
            notes.reverse();
        }
        let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
        for old in [!reverse, reverse] {
            let name = format!("crossover_{label}_{}", if old { "old" } else { "new" });
            crate::perf::measure_sampled(
                &name,
                if count < 100 { 4000 } else { 128 },
                count,
                || {
                    let notes = black_box(notes.as_slice());
                    if old {
                        legacy_build_crossover_rows::<4>(notes, (0, notes.len()), 0)
                    } else {
                        build_crossover_rows::<4>(notes, (0, notes.len()), 0)
                    }
                },
            );
        }
    }
}
