use crate::game::note::NoteType;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedNote {
    pub row_index: usize,
    pub column: usize,
    pub note_type: NoteType,
    pub tail_row_index: Option<usize>,
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
pub fn parse_chart_notes(minimized_note_data: &[u8], lanes: usize) -> Vec<ParsedNote> {
    let mut notes = Vec::new();
    let mut row_index = 0usize;
    let lanes = lanes.max(1);
    let mut hold_heads: Vec<Option<usize>> = vec![None; lanes];
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
                        invalidate_hold(&mut invalid_heads, &mut hold_heads, col_index);
                        notes.push(ParsedNote {
                            row_index,
                            column: col_index,
                            note_type: NoteType::Tap,
                            tail_row_index: None,
                        });
                    }
                    b'F' | b'f' => {
                        invalidate_hold(&mut invalid_heads, &mut hold_heads, col_index);
                        notes.push(ParsedNote {
                            row_index,
                            column: col_index,
                            note_type: NoteType::Fake,
                            tail_row_index: None,
                        });
                    }
                    b'2' | b'4' => {
                        invalidate_hold(&mut invalid_heads, &mut hold_heads, col_index);
                        let note_type = if ch == b'2' {
                            NoteType::Hold
                        } else {
                            NoteType::Roll
                        };
                        let note_index = notes.len();
                        notes.push(ParsedNote {
                            row_index,
                            column: col_index,
                            note_type,
                            tail_row_index: None,
                        });
                        hold_heads[col_index] = Some(note_index);
                    }
                    b'M' | b'm' => {
                        invalidate_hold(&mut invalid_heads, &mut hold_heads, col_index);
                        notes.push(ParsedNote {
                            row_index,
                            column: col_index,
                            note_type: NoteType::Mine,
                            tail_row_index: None,
                        });
                    }
                    b'L' | b'l' => {
                        invalidate_hold(&mut invalid_heads, &mut hold_heads, col_index);
                        notes.push(ParsedNote {
                            row_index,
                            column: col_index,
                            note_type: NoteType::Lift,
                            tail_row_index: None,
                        });
                    }
                    b'3' => {
                        if let Some(head_idx) = hold_heads[col_index].take()
                            && let Some(note) = notes.get_mut(head_idx)
                        {
                            note.tail_row_index = Some(row_index);
                        }
                    }
                    _ => {}
                }
            }
        }
        row_index += 1;
    }

    for col_index in 0..lanes {
        invalidate_hold(&mut invalid_heads, &mut hold_heads, col_index);
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
    use super::{ParsedNote, parse_chart_notes};
    use crate::game::note::NoteType;

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
}
