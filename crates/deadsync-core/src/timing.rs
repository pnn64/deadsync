pub const ROWS_PER_BEAT: i32 = 48;

#[inline(always)]
pub fn note_row_to_beat(row: i32) -> f32 {
    row as f32 / ROWS_PER_BEAT as f32
}

#[inline(always)]
pub fn beat_to_note_row(beat: f32) -> i32 {
    (beat * ROWS_PER_BEAT as f32).round() as i32
}

#[cfg(test)]
mod tests {
    use super::{ROWS_PER_BEAT, beat_to_note_row, note_row_to_beat};

    #[test]
    fn note_rows_round_trip_whole_beats() {
        assert_eq!(ROWS_PER_BEAT, 48);
        assert_eq!(beat_to_note_row(4.0), 192);
        assert_eq!(note_row_to_beat(192), 4.0);
    }

    #[test]
    fn beat_to_note_row_rounds_to_nearest_row() {
        assert_eq!(beat_to_note_row(0.5), 24);
        assert_eq!(beat_to_note_row(0.51), 24);
        assert_eq!(beat_to_note_row(0.52), 25);
    }
}
