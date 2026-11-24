//! Achievement ratios for the chart's Stream, Voltage, Air, Freeze, and Chaos axes.

use deadsync_core::note::NoteType;
use deadsync_rules::{
    combo::ComboRun,
    judgment::{JudgeGrade, select_row_final_judgment},
    note::{HoldResult, Note},
};

/// Uses the judged, transformed chart and the same recorded combos as evaluation.
/// Notes must be in row order, as established by gameplay setup.
#[must_use]
pub fn achievement(
    notes: &[Note],
    combos: &[ComboRun],
    played_seconds: f32,
    grade_points: [i32; 2],
) -> [f32; 5] {
    debug_assert!(
        notes
            .windows(2)
            .all(|pair| pair[0].row_index <= pair[1].row_index)
    );
    let mut taps = 0;
    let mut perfects = 0;
    let mut jumps = 0;
    let mut perfect_jumps = 0;
    let mut holds = 0;
    let mut held = 0;
    let mut index = 0;
    while index < notes.len() {
        let row = notes[index].row_index;
        let mut heads = 0;
        let mut missing = false;
        let mut last = None;
        while index < notes.len() && notes[index].row_index == row {
            let note = &notes[index];
            index += 1;
            if !note.can_be_judged || note.note_type == NoteType::Mine {
                continue;
            }
            taps += 1;
            perfects += u32::from(
                note.result
                    .is_some_and(|result| result.grade <= JudgeGrade::Excellent),
            );
            heads += u32::from(matches!(
                note.note_type,
                NoteType::Tap | NoteType::Hold | NoteType::Roll
            ));
            if let Some(result) = &note.result {
                select_row_final_judgment(&mut last, result);
            } else {
                missing = true;
            }
            holds += u32::from(note.note_type == NoteType::Hold);
            held += u32::from(
                matches!(note.note_type, NoteType::Hold | NoteType::Roll)
                    && note
                        .hold
                        .as_ref()
                        .is_some_and(|hold| hold.result == Some(HoldResult::Held)),
            );
        }
        if heads >= 2 {
            jumps += 1;
            perfect_jumps += u32::from(
                !missing && last.is_some_and(|result| result.grade <= JudgeGrade::Excellent),
            );
        }
    }
    // The highest-count run wins; ties keep the first run, regardless of duration.
    let best = combos
        .iter()
        .fold(None::<&ComboRun>, |best, run| match best {
            Some(current)
                if current.count.saturating_add(current.carried)
                    >= run.count.saturating_add(run.carried) =>
            {
                best
            }
            _ => Some(run),
        });
    let voltage = if played_seconds.is_finite() && played_seconds > 0.0 {
        best.map_or(0.0, |run| {
            ((run.end_second - run.start_second) / played_seconds).clamp(0.0, 1.0)
        })
    } else {
        0.0
    };
    let ratio = |actual: u32, possible: u32| {
        if possible == 0 {
            1.0
        } else {
            (actual as f32 / possible as f32).clamp(0.0, 1.0)
        }
    };
    [
        ratio(perfects, taps),
        voltage,
        ratio(perfect_jumps, jumps),
        ratio(held, holds),
        if grade_points[1] <= 0 {
            1.0
        } else {
            (grade_points[0] as f32 / grade_points[1] as f32).clamp(0.0, 1.0)
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_rules::{judgment::Judgment, note::HoldData};

    fn note(
        row: usize,
        kind: NoteType,
        grade: Option<JudgeGrade>,
        offset: i64,
        held: bool,
    ) -> Note {
        Note {
            participant: None,
            beat: row as f32,
            quantization_idx: 0,
            column: 0,
            note_type: kind,
            row_index: row,
            result: grade.map(|grade| Judgment {
                time_error_ms: offset as f32 / 1_000_000.0,
                time_error_music_ns: offset,
                grade,
                window: None,
                miss_because_held: false,
            }),
            early_result: None,
            hold: matches!(kind, NoteType::Hold | NoteType::Roll).then_some(HoldData {
                end_row_index: row + 1,
                end_beat: row as f32 + 1.0,
                result: Some(if held {
                    HoldResult::Held
                } else {
                    HoldResult::Missed
                }),
                life: 1.0,
                let_go_started_at: None,
                let_go_starting_life: 1.0,
                last_held_row_index: row,
                last_held_beat: row as f32,
            }),
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    #[test]
    fn performance_uses_completed_chords_and_the_highest_count_combo() {
        use JudgeGrade::{Excellent, Great, Miss};
        use NoteType::{Hold, Mine, Roll, Tap};
        let notes = [
            note(0, Tap, Some(Excellent), 10, false),
            note(0, Tap, Some(Great), 20, false),
            note(0, Mine, None, 0, false),
            note(1, Tap, Some(Great), -20, false),
            note(1, Tap, Some(Excellent), 10, false),
            note(2, Hold, Some(Excellent), 0, true),
            note(3, Roll, Some(Excellent), 0, true),
            note(4, Hold, Some(Miss), 0, false),
            note(5, Tap, Some(Excellent), 0, false),
            note(5, Tap, None, 0, false),
        ];
        let combos = [
            ComboRun {
                start_second: 0.0,
                end_second: 20.0,
                count: 6,
                carried: 0,
            },
            ComboRun {
                start_second: 20.0,
                end_second: 100.0,
                count: 5,
                carried: 0,
            },
            ComboRun {
                start_second: 0.0,
                end_second: 50.0,
                count: 6,
                carried: 0,
            },
        ];
        assert_eq!(
            achievement(&notes, &combos, 100.0, [50, 80]),
            [5.0 / 9.0, 0.2, 1.0 / 3.0, 1.0, 0.625]
        );
    }

    #[test]
    fn empty_categories_are_complete_and_negative_points_are_clamped() {
        assert_eq!(
            achievement(&[], &[], 0.0, [0, 0]),
            [1.0, 0.0, 1.0, 1.0, 1.0]
        );
        assert_eq!(achievement(&[], &[], 100.0, [-10, 80])[4], 0.0);
    }
}
