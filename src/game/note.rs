use crate::game::judgment::Judgment;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum NoteType {
    Tap,
    Hold,
    Roll,
    Mine,
    Fake,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HoldResult {
    Held,
    LetGo,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MineResult {
    Hit,
    Avoided,
}

#[derive(Clone, Debug)]
pub struct HoldData {
    pub end_row_index: usize,
    pub end_beat: f32,
    pub result: Option<HoldResult>,
    pub life: f32,
    pub let_go_started_at: Option<f32>,
    pub let_go_starting_life: f32,
    pub last_held_row_index: usize,
    pub last_held_beat: f32,
}

#[derive(Clone, Debug)]
pub struct Note {
    pub beat: f32,
    pub quantization_idx: u8,
    pub column: usize,
    pub note_type: NoteType,
    pub row_index: usize,
    pub result: Option<Judgment>,
    pub hold: Option<HoldData>,
    pub mine_result: Option<MineResult>,
    pub is_fake: bool,
    // Optimization: cached result of !is_fake && !warp && !fake_segment
    pub can_be_judged: bool,
}
