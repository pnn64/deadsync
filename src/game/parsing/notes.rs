use crate::game::note::NoteType;
use log::info;

#[derive(Clone, Debug)]
pub struct ParsedNote {
    pub row_index: usize,
    pub column: usize,
    pub note_type: NoteType,
    pub tail_row_index: Option<usize>,
}

/// Parses the raw, minimized `#NOTES:` data block from `rssp`.
///
/// This function converts the byte representation of notes into a vector of
/// `(row_index, column, NoteType)`, leaving the conversion from row to beat
/// up to the `TimingData` module.
pub fn parse_chart_notes(raw_note_bytes: &[u8]) -> Vec<ParsedNote> {
    let mut notes = Vec::new();
    let mut row_index = 0;
    let mut hold_heads: [Option<usize>; 4] = [None; 4];

    // Split by lines, also handling potential commas on their own lines
    for line in raw_note_bytes.split(|&b| b == b'\n') {
        let trimmed_line = line.strip_suffix(b"\r").unwrap_or(line);
        if trimmed_line.is_empty() || trimmed_line == b"," {
            continue;
        }

        if trimmed_line.len() >= 4 {
            for (col_index, &ch) in trimmed_line.iter().take(4).enumerate() {
                match ch {
                    b'1' => {
                        notes.push(ParsedNote {
                            row_index,
                            column: col_index,
                            note_type: NoteType::Tap,
                            tail_row_index: None,
                        });
                    }
                    b'F' | b'f' => {
                        // Fake tap: render only; never judge or score
                        notes.push(ParsedNote {
                            row_index,
                            column: col_index,
                            note_type: NoteType::Fake,
                            tail_row_index: None,
                        });
                    }
                    b'2' | b'4' => {
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
                        notes.push(ParsedNote {
                            row_index,
                            column: col_index,
                            note_type: NoteType::Mine,
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

    info!("Pre-parsed {} notes from raw chart data.", notes.len());
    notes
}
