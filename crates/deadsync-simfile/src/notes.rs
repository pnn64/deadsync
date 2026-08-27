use deadsync_chart::notes::ParsedNote;
use deadsync_core::note::NoteType;

#[must_use]
pub fn step_type_lanes(step_type: &str) -> usize {
    let step_type = step_type.trim();
    if step_type.eq_ignore_ascii_case("pump-double")
        || step_type.eq_ignore_ascii_case("pump_double")
    {
        10
    } else if step_type.eq_ignore_ascii_case("dance-double")
        || step_type.eq_ignore_ascii_case("dance_double")
    {
        8
    } else if step_type.eq_ignore_ascii_case("pump-single")
        || step_type.eq_ignore_ascii_case("pump_single")
    {
        5
    } else {
        4
    }
}

fn invalidate_hold(
    invalid_heads: &mut Vec<usize>,
    hold_heads: &mut [Option<usize>],
    col_index: usize,
) {
    if let Some(note_index) = hold_heads[col_index].take() {
        invalid_heads.push(note_index);
    }
}

/// Parses minimized chart note data into note events, tracking hold/roll tails.
#[must_use]
pub fn parse_chart_notes(minimized_note_data: &[u8], lanes: usize) -> Vec<ParsedNote> {
    parse_chart_notes_as(
        minimized_note_data,
        lanes,
        |row_index, column, note_type| ParsedNote {
            row_index,
            column,
            note_type,
            tail_row_index: None,
        },
        |note, tail_row_index| note.tail_row_index = Some(tail_row_index),
    )
}

pub(crate) fn parse_chart_notes_as<T>(
    minimized_note_data: &[u8],
    lanes: usize,
    new_note: impl FnMut(usize, usize, NoteType) -> T,
    set_tail: impl FnMut(&mut T, usize),
) -> Vec<T> {
    let note_capacity = minimized_note_data
        .iter()
        .filter(|&&ch| {
            matches!(
                ch,
                b'1' | b'F' | b'f' | b'2' | b'4' | b'M' | b'm' | b'L' | b'l'
            )
        })
        .count();
    parse_chart_notes_as_with_capacity(
        minimized_note_data,
        lanes,
        note_capacity,
        new_note,
        set_tail,
    )
}

pub(crate) fn parse_chart_notes_as_with_capacity<T>(
    minimized_note_data: &[u8],
    lanes: usize,
    note_capacity: usize,
    new_note: impl FnMut(usize, usize, NoteType) -> T,
    set_tail: impl FnMut(&mut T, usize),
) -> Vec<T> {
    let lanes = lanes.max(1);
    let mut stack_heads = [None; 10];
    let mut heap_heads = Vec::new();
    let hold_heads = if lanes <= stack_heads.len() {
        &mut stack_heads[..lanes]
    } else {
        heap_heads.resize(lanes, None);
        heap_heads.as_mut_slice()
    };
    parse_chart_notes_with_heads(
        minimized_note_data,
        hold_heads,
        note_capacity,
        new_note,
        set_tail,
    )
}

#[cfg(any(test, feature = "bench-support"))]
pub(crate) fn parse_chart_notes_legacy(
    minimized_note_data: &[u8],
    lanes: usize,
) -> Vec<ParsedNote> {
    let note_capacity = minimized_note_data
        .iter()
        .filter(|&&ch| {
            matches!(
                ch,
                b'1' | b'F' | b'f' | b'2' | b'4' | b'M' | b'm' | b'L' | b'l'
            )
        })
        .count();
    let mut hold_heads = vec![None; lanes.max(1)];
    parse_chart_notes_with_heads(
        minimized_note_data,
        &mut hold_heads,
        note_capacity,
        |row_index, column, note_type| ParsedNote {
            row_index,
            column,
            note_type,
            tail_row_index: None,
        },
        |note, tail_row_index| note.tail_row_index = Some(tail_row_index),
    )
}

fn parse_chart_notes_with_heads<T>(
    minimized_note_data: &[u8],
    hold_heads: &mut [Option<usize>],
    note_capacity: usize,
    mut new_note: impl FnMut(usize, usize, NoteType) -> T,
    mut set_tail: impl FnMut(&mut T, usize),
) -> Vec<T> {
    let mut notes = Vec::with_capacity(note_capacity);
    let mut row_index = 0usize;
    let lanes = hold_heads.len();
    let mut invalid_heads = Vec::new();

    for line in minimized_note_data.split(|&b| b == b'\n') {
        let trimmed_line = line.strip_suffix(b"\r").unwrap_or(line);
        if trimmed_line.is_empty() || trimmed_line == b"," {
            continue;
        }

        if trimmed_line.len() >= lanes {
            for (col_index, &ch) in trimmed_line.iter().take(lanes).enumerate() {
                match ch {
                    b'1' => {
                        invalidate_hold(&mut invalid_heads, hold_heads, col_index);
                        notes.push(new_note(row_index, col_index, NoteType::Tap));
                    }
                    b'F' | b'f' => {
                        invalidate_hold(&mut invalid_heads, hold_heads, col_index);
                        notes.push(new_note(row_index, col_index, NoteType::Fake));
                    }
                    b'2' | b'4' => {
                        invalidate_hold(&mut invalid_heads, hold_heads, col_index);
                        let note_type = if ch == b'2' {
                            NoteType::Hold
                        } else {
                            NoteType::Roll
                        };
                        let note_index = notes.len();
                        notes.push(new_note(row_index, col_index, note_type));
                        hold_heads[col_index] = Some(note_index);
                    }
                    b'M' | b'm' => {
                        invalidate_hold(&mut invalid_heads, hold_heads, col_index);
                        notes.push(new_note(row_index, col_index, NoteType::Mine));
                    }
                    b'L' | b'l' => {
                        invalidate_hold(&mut invalid_heads, hold_heads, col_index);
                        notes.push(new_note(row_index, col_index, NoteType::Lift));
                    }
                    b'3' => {
                        if let Some(head_idx) = hold_heads[col_index].take()
                            && let Some(note) = notes.get_mut(head_idx)
                        {
                            set_tail(note, row_index);
                        }
                    }
                    _ => {}
                }
            }
        }
        row_index += 1;
    }

    for col_index in 0..lanes {
        invalidate_hold(&mut invalid_heads, hold_heads, col_index);
    }
    if invalid_heads.is_empty() {
        return notes;
    }

    invalid_heads.sort_unstable();
    let mut invalid_iter = invalid_heads.into_iter().peekable();
    let mut note_index = 0usize;
    notes.retain(|_| {
        let keep = invalid_iter.peek().copied() != Some(note_index);
        if !keep {
            invalid_iter.next();
        }
        note_index += 1;
        keep
    });
    notes
}

#[cfg(test)]
mod tests {
    use super::{
        ParsedNote, parse_chart_notes, parse_chart_notes_as_with_capacity,
        parse_chart_notes_legacy, step_type_lanes,
    };
    use deadsync_core::note::NoteType;

    #[test]
    fn step_type_lanes_supports_dance_and_pump() {
        assert_eq!(step_type_lanes("dance-double"), 8);
        assert_eq!(step_type_lanes("dance_double"), 8);
        assert_eq!(step_type_lanes(" DANCE_DOUBLE "), 8);
        assert_eq!(step_type_lanes("dance__double"), 4);
        assert_eq!(step_type_lanes(" dance-single "), 4);
        assert_eq!(step_type_lanes("pump-single"), 5);
        assert_eq!(step_type_lanes("PUMP_SINGLE"), 5);
        assert_eq!(step_type_lanes("pump-double"), 10);
        assert_eq!(step_type_lanes("PUMP_DOUBLE"), 10);
        assert_eq!(step_type_lanes("pump-halfdouble"), 4);
    }

    #[test]
    fn parse_chart_notes_reads_all_ten_pump_columns() {
        let notes = parse_chart_notes(b"1000100001\n", 10);
        assert_eq!(notes.len(), 3);
        assert_eq!(notes[0].column, 0);
        assert_eq!(notes[1].column, 4);
        assert_eq!(notes[2].column, 9);
    }

    #[test]
    fn parse_chart_notes_recognizes_lifts() {
        let notes = parse_chart_notes(b"0000\nL000\n0000\n0000\n", 4);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].row_index, 1);
        assert_eq!(notes[0].column, 0);
        assert_eq!(notes[0].note_type, NoteType::Lift);
        assert_eq!(notes[0].tail_row_index, None);
    }

    #[test]
    fn parse_chart_notes_keeps_closed_holds() {
        let notes = parse_chart_notes(b"2000\n3000\n", 4);
        assert_eq!(
            notes,
            vec![ParsedNote {
                row_index: 0,
                column: 0,
                note_type: NoteType::Hold,
                tail_row_index: Some(1),
            }]
        );
    }

    #[test]
    fn parse_chart_notes_drops_unmatched_hold_and_roll_heads() {
        let notes = parse_chart_notes(b"2400\n0000\n", 4);
        assert!(notes.is_empty());
    }

    #[test]
    fn parse_chart_notes_drops_holds_blocked_before_tail() {
        let notes = parse_chart_notes(b"2000\n1000\n3000\n", 4);
        assert_eq!(
            notes,
            vec![ParsedNote {
                row_index: 1,
                column: 0,
                note_type: NoteType::Tap,
                tail_row_index: None,
            }]
        );
    }

    #[test]
    fn parse_chart_notes_restarts_holds_after_new_head() {
        let notes = parse_chart_notes(b"2000\n2000\n3000\n", 4);
        assert_eq!(
            notes,
            vec![ParsedNote {
                row_index: 1,
                column: 0,
                note_type: NoteType::Hold,
                tail_row_index: Some(2),
            }]
        );
    }

    #[test]
    fn stack_hold_state_matches_legacy_heap_parsing() {
        let fixtures: &[(&[u8], usize)] = &[
            (b"2000\n0100\n3000\nM00L\nF000\n", 4),
            (b"2400000000\n0030000000\n0000300000\n1000000001\n", 10),
            (b"200000000000\n300000000000\n000000000001\n", 12),
        ];
        for &(notes, lanes) in fixtures {
            assert_eq!(
                parse_chart_notes(notes, lanes),
                parse_chart_notes_legacy(notes, lanes),
                "note parsing diverged for {lanes} lanes"
            );
        }
    }

    #[test]
    fn supplied_capacity_does_not_change_note_output() {
        let fixtures: &[(&[u8], usize)] = &[
            (b"2000\n0100\n3000\nM00L\nF000\n", 4),
            (b"2000\n1000\n3000\n", 4),
            (b"2400000000\n0030000000\n0000300000\n1000000001\n", 10),
            (b"200000000000\n300000000000\n000000000001\n", 12),
        ];
        for &(note_data, lanes) in fixtures {
            let expected = parse_chart_notes(note_data, lanes);
            let actual = parse_chart_notes_as_with_capacity(
                note_data,
                lanes,
                0,
                |row_index, column, note_type| ParsedNote {
                    row_index,
                    column,
                    note_type,
                    tail_row_index: None,
                },
                |note, tail_row_index| note.tail_row_index = Some(tail_row_index),
            );
            assert_eq!(actual, expected, "note parsing diverged for {lanes} lanes");
        }
    }
}
