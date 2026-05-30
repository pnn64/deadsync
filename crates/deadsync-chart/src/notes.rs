use deadsync_core::note::NoteType;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedNote {
    pub row_index: usize,
    pub column: usize,
    pub note_type: NoteType,
    pub tail_row_index: Option<usize>,
}
