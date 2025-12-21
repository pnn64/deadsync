use crate::game::note::NoteType;
use rssp::notes::{NoteKind, ParsedNote as RsspParsedNote};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedNote {
    pub row_index: usize,
    pub column: usize,
    pub note_type: NoteType,
    pub tail_row_index: Option<usize>,
}

impl From<RsspParsedNote> for ParsedNote {
    fn from(note: RsspParsedNote) -> Self {
        let note_type = match note.note_kind {
            NoteKind::Tap => NoteType::Tap,
            NoteKind::Hold => NoteType::Hold,
            NoteKind::Roll => NoteType::Roll,
            NoteKind::Mine => NoteType::Mine,
            NoteKind::Fake => NoteType::Fake,
        };
        Self {
            row_index: note.row_index,
            column: note.column,
            note_type,
            tail_row_index: note.tail_row_index,
        }
    }
}

/// Parses the raw, minimized `#NOTES:` data block via rssp.
pub fn parse_chart_notes(raw_note_bytes: &[u8]) -> Vec<ParsedNote> {
    rssp::notes::parse_chart_notes(raw_note_bytes, 4)
        .into_iter()
        .map(ParsedNote::from)
        .collect()
}
