use deadsync_core::note::NoteType;
use deadsync_gameplay::{count_held_tracks_at_row, is_hold_body_at_row};
use deadsync_rules::note::{HoldData, Note};
use std::hint::black_box;

#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

// Starting-main implementation: each lane traverses the complete chart.
fn original_count(notes: &[Note], row: usize, offset: usize, cols: usize) -> usize {
    (0..cols)
        .filter(|local| is_hold_body_at_row(notes, row, offset + *local))
        .count()
}

fn note(row: usize, column: usize, kind: NoteType, tail: Option<usize>) -> Note {
    Note {
        beat: row as f32 / 48.0,
        quantization_idx: 0,
        column,
        note_type: kind,
        row_index: row,
        result: None,
        early_result: None,
        hold: tail.map(|end| HoldData {
            end_row_index: end,
            end_beat: end as f32 / 48.0,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 0.0,
            last_held_row_index: row,
            last_held_beat: row as f32 / 48.0,
        }),
        mine_result: None,
        is_fake: false,
        can_be_judged: true,
    }
}

#[test]
fn held_lanes_keep_tail_boundaries_blocking_cells_and_source_order_ties() {
    let mut notes = vec![
        note(0, 0, NoteType::Hold, Some(48)),
        note(0, 1, NoteType::Roll, Some(48)),
        note(24, 0, NoteType::Tap, None),
        note(24, 1, NoteType::Hold, None),
    ];
    assert_eq!(count_held_tracks_at_row(&notes, 0, 0, 4), 0);
    assert_eq!(count_held_tracks_at_row(&notes, 23, 0, 4), 2);
    assert_eq!(count_held_tracks_at_row(&notes, 24, 0, 4), 0);
    notes.push(note(24, 1, NoteType::Roll, Some(48)));
    assert_eq!(count_held_tracks_at_row(&notes, 25, 0, 4), 1);
    assert_eq!(count_held_tracks_at_row(&notes, 48, 0, 4), 1);
    assert_eq!(count_held_tracks_at_row(&notes, 49, 0, 4), 0);
    notes.swap(3, 4);
    assert_eq!(count_held_tracks_at_row(&notes, 25, 0, 4), 0);
}

#[test]
fn held_lanes_match_original_on_unsorted_duplicate_and_extended_lane_inputs() {
    let kinds = [
        NoteType::Tap,
        NoteType::Hold,
        NoteType::Roll,
        NoteType::Mine,
        NoteType::Lift,
        NoteType::Fake,
    ];
    let mut notes: Vec<_> = (0..512)
        .map(|i| {
            let row = (i * 37) % 128;
            note(
                row,
                (i * 19) % 14,
                kinds[i % kinds.len()],
                (i % 3 != 0).then_some(row + i % 49),
            )
        })
        .collect();
    for _ in 0..3 {
        for row in 0..=176 {
            for (offset, cols) in [
                (0, 0),
                (0, 4),
                (4, 4),
                (0, 10),
                (1, 12),
                (usize::MAX - 3, 2),
            ] {
                assert_eq!(
                    count_held_tracks_at_row(&notes, row, offset, cols),
                    original_count(&notes, row, offset, cols),
                    "row {row}, offset {offset}, cols {cols}"
                );
            }
        }
        notes.reverse();
        notes.rotate_left(17);
    }
}

#[test]
#[ignore = "paired release throughput benchmark"]
fn benchmark_held_lanes() {
    let notes: Vec<_> = (0..8192)
        .map(|i| {
            note(
                i / 10 * 12,
                i % 10,
                if i % 7 == 0 {
                    NoteType::Hold
                } else {
                    NoteType::Tap
                },
                Some(i / 10 * 12 + 96),
            )
        })
        .collect();
    for cols in [4, 8, 10] {
        paired_bench::compare(
            &format!("held lanes ({cols} columns, 8192 notes)"),
            500,
            |current| {
                let (notes, row, cols) = black_box((notes.as_slice(), 4900, cols));
                black_box(if current {
                    count_held_tracks_at_row(notes, row, 0, cols)
                } else {
                    original_count(notes, row, 0, cols)
                });
            },
        );
    }
}
