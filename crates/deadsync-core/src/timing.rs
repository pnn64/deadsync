pub const ROWS_PER_BEAT: i32 = 48;

#[inline(always)]
#[must_use]
pub fn note_row_to_beat(row: i32) -> f32 {
    row as f32 / ROWS_PER_BEAT as f32
}

#[inline(always)]
#[must_use]
pub fn beat_to_note_row(beat: f32) -> i32 {
    // ITGmania's NoteTypes.h uses std::lrint with nearest-even rounding.
    let row = beat * ROWS_PER_BEAT as f32;
    let magnitude = row.abs();
    // In [2^23, 2^24), f32 spacing is one: adding/subtracting 2^23
    // rounds smaller magnitudes to even. Larger f32 values are integral.
    let rounded = if magnitude < 8_388_608.0 {
        (magnitude + 8_388_608.0) - 8_388_608.0
    } else {
        magnitude
    };
    rounded.copysign(row) as i32
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

    #[test]
    fn note_row_ties_round_even() {
        for (row, expected) in [(0.5, 0), (1.5, 2), (48.5, 48), (49.5, 50), (-0.5, 0), (-1.5, -2)] {
            assert_eq!(beat_to_note_row(row / ROWS_PER_BEAT as f32), expected);
        }
    }
}
