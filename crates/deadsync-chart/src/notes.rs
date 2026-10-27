use deadsync_core::note::NoteType;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedNote {
    /// Authored Routine participant (0 = P1, 1 = P2).
    pub player: u8,
    pub row_index: usize,
    pub column: usize,
    pub note_type: NoteType,
    pub tail_row_index: Option<usize>,
}

#[must_use]
pub fn step_type_lanes(step_type: &str) -> usize {
    let bytes = step_type.trim().as_bytes();
    let pump = |style: &[u8]| {
        bytes.len() == 5 + style.len()
            && matches!(bytes[4], b'-' | b'_')
            && bytes[..4].eq_ignore_ascii_case(b"pump")
            && bytes[5..].eq_ignore_ascii_case(style)
    };
    let dance = |style: &[u8]| {
        bytes.len() == 6 + style.len()
            && matches!(bytes[5], b'-' | b'_')
            && bytes[..5].eq_ignore_ascii_case(b"dance")
            && bytes[6..].eq_ignore_ascii_case(style)
    };
    // Partner charts span both pads, like doubles.
    if pump(b"double") || pump(b"couple") || pump(b"routine") {
        10
    } else if dance(b"double") || dance(b"couple") || dance(b"routine") {
        8
    } else if pump(b"single") {
        5
    } else {
        4
    }
}

/// Parses minimized chart note data into note events, tracking hold/roll tails.
#[must_use]
pub fn parse_chart_notes(minimized_note_data: &[u8], lanes: usize) -> Vec<ParsedNote> {
    parse_chart_notes_as(
        minimized_note_data,
        lanes,
        |row_index, column, note_type| ParsedNote {
            player: 0,
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
    let (words, tail) = minimized_note_data.as_chunks::<8>();
    let mut note_capacity = 0;
    for &bytes in words {
        let word = u64::from_ne_bytes(bytes);
        // Case folding also maps control bytes 0x11/12/14 to digits. Keep
        // their original bit 5 as a high-bit mask for the digit comparisons.
        let digit_mask = (word & 0x2020_2020_2020_2020) << 2;
        let word = word | 0x2020_2020_2020_2020;
        let equal = |byte: u8| {
            let xor = word ^ (u64::from(byte) * 0x0101_0101_0101_0101);
            // Set each zero byte's high bit. Masking before the addition
            // prevents carries between bytes, so every match is exact.
            !(((xor & 0x7f7f_7f7f_7f7f_7f7f) + 0x7f7f_7f7f_7f7f_7f7f) | xor | 0x7f7f_7f7f_7f7f_7f7f)
        };
        let matches = ((equal(b'1') | equal(b'2') | equal(b'4')) & digit_mask)
            | equal(b'f')
            | equal(b'm')
            | equal(b'l');
        // Sum eight independent 0/1 lanes without requiring a POPCNT target.
        note_capacity += ((matches >> 7).wrapping_mul(0x0101_0101_0101_0101) >> 56) as usize;
    }
    note_capacity += tail
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
    // No Vec element can have index usize::MAX; keep each head in one word.
    let mut stack_heads = [usize::MAX; 10];
    let mut heap_heads = Vec::new();
    let hold_heads = if lanes <= stack_heads.len() {
        &mut stack_heads[..lanes]
    } else {
        heap_heads.resize(lanes, usize::MAX);
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

fn parse_chart_notes_with_heads<T>(
    minimized_note_data: &[u8],
    hold_heads: &mut [usize],
    note_capacity: usize,
    mut new_note: impl FnMut(usize, usize, NoteType) -> T,
    mut set_tail: impl FnMut(&mut T, usize),
) -> Vec<T> {
    let mut notes = Vec::with_capacity(note_capacity);
    let mut row_index = 0usize;
    let lanes = hold_heads.len();
    let mut invalid_heads = Vec::new();

    let mut line_start = 0;
    for line_end in memchr::memchr_iter(b'\n', minimized_note_data)
        .chain(std::iter::once(minimized_note_data.len()))
    {
        let line = &minimized_note_data[line_start..line_end];
        line_start = line_end + 1;
        let trimmed_line = line.strip_suffix(b"\r").unwrap_or(line);
        if trimmed_line.is_empty() || trimmed_line == b"," {
            continue;
        }

        if trimmed_line.len() >= lanes {
            for (col_index, (&ch, head)) in trimmed_line[..lanes]
                .iter()
                .zip(hold_heads.iter_mut())
                .enumerate()
            {
                let note_type = match ch {
                    b'1' => NoteType::Tap,
                    b'F' | b'f' => NoteType::Fake,
                    b'2' => NoteType::Hold,
                    b'4' => NoteType::Roll,
                    b'M' | b'm' => NoteType::Mine,
                    b'L' | b'l' => NoteType::Lift,
                    b'3' => {
                        let head_idx = std::mem::replace(head, usize::MAX);
                        if head_idx != usize::MAX
                            && let Some(note) = notes.get_mut(head_idx)
                        {
                            set_tail(note, row_index);
                        }
                        continue;
                    }
                    _ => continue,
                };
                let note_index = std::mem::replace(head, usize::MAX);
                if note_index != usize::MAX {
                    invalid_heads.push(note_index);
                }
                let note_index = notes.len();
                notes.push(new_note(row_index, col_index, note_type));
                if matches!(note_type, NoteType::Hold | NoteType::Roll) {
                    *head = note_index;
                }
            }
        }
        row_index += 1;
    }

    invalid_heads.extend(
        hold_heads
            .iter()
            .copied()
            .filter(|&index| index != usize::MAX),
    );
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
        ParsedNote, parse_chart_notes, parse_chart_notes_as_with_capacity, step_type_lanes,
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
        assert_eq!(step_type_lanes("dance-couple"), 8);
        assert_eq!(step_type_lanes("DANCE_ROUTINE"), 8);
        assert_eq!(step_type_lanes("pump-couple"), 10);
        assert_eq!(step_type_lanes(" pump_routine "), 10);
        assert_eq!(step_type_lanes("pump-routinex"), 4);
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
                player: 0,
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
                player: 0,
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
                player: 0,
                row_index: 1,
                column: 0,
                note_type: NoteType::Hold,
                tail_row_index: Some(2),
            }]
        );
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
                    player: 0,
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

    #[test]
    fn note_reservation_matches_original_for_every_byte_and_lane_count() {
        for lanes in [0, 1, 4, 5, 8, 10, 12] {
            for (byte, length) in (0..=255)
                .flat_map(|byte| (0..=17).chain([127, 128]).map(move |length| (byte, length)))
            {
                let mut data = vec![byte; length];
                data.extend_from_slice(b"\r\n,\n200000000000\n100000000000\n300000000000\n");
                let capacity = data
                    .iter()
                    .filter(|&&ch| {
                        matches!(
                            ch,
                            b'1' | b'F' | b'f' | b'2' | b'4' | b'M' | b'm' | b'L' | b'l'
                        )
                    })
                    .count();
                let expected = parse_chart_notes_as_with_capacity(
                    &data,
                    lanes,
                    capacity,
                    |row_index, column, note_type| ParsedNote {
                        player: 0,
                        row_index,
                        column,
                        note_type,
                        tail_row_index: None,
                    },
                    |note, tail_row_index| note.tail_row_index = Some(tail_row_index),
                );
                let actual = parse_chart_notes(&data, lanes);
                assert_eq!(actual, expected, "byte={byte}, lanes={lanes}");
                assert_eq!(
                    actual.capacity(),
                    expected.capacity(),
                    "byte={byte}, lanes={lanes}"
                );
            }
        }
    }
}
