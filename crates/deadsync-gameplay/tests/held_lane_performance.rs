use deadsync_core::note::NoteType;
use deadsync_gameplay::count_held_tracks_at_row;
use deadsync_rules::note::{HoldData, Note};


// Starting-main implementation: each lane traverses the complete chart.

fn note(row: usize, column: usize, kind: NoteType, tail: Option<usize>) -> Note {
    Note {
        participant: None,
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
