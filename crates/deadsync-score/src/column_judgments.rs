use deadsync_core::input::MAX_COLS;
use deadsync_core::note::NoteType;
use deadsync_rules::judgment::{self, JudgeGrade, Judgment, TimingWindow};
use deadsync_rules::note::Note;
use smallvec::SmallVec;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColumnJudgments {
    pub w0: u32,
    pub w1: u32,
    pub w2: u32,
    pub w3: u32,
    pub w4: u32,
    pub w5: u32,
    pub miss: u32,
    pub early_w1: u32,
    pub early_w2: u32,
    pub early_w3: u32,
    pub early_w4: u32,
    pub early_w5: u32,
    pub early_total_w0: u32,
    pub early_total_w1: u32,
    pub early_total_w2: u32,
    pub early_total_w3: u32,
    pub early_total_w4: u32,
    pub early_total_w5: u32,
    pub held_miss: u32,
}

/// Per-lane evaluation totals. Gameplay styles remain inline through
/// [`MAX_COLS`]; wider imported/test domains retain a heap-backed fallback.
pub type ColumnJudgmentList = SmallVec<[ColumnJudgments; MAX_COLS]>;

#[inline(always)]
const fn add_early_total(slot: &mut ColumnJudgments, judgment: &Judgment, include_bad: bool) {
    if matches!(judgment.window, Some(TimingWindow::W0)) {
        slot.early_total_w0 = slot.early_total_w0.saturating_add(1);
        return;
    }
    match judgment.grade {
        JudgeGrade::Fantastic => slot.early_total_w1 = slot.early_total_w1.saturating_add(1),
        JudgeGrade::Excellent => slot.early_total_w2 = slot.early_total_w2.saturating_add(1),
        JudgeGrade::Great => slot.early_total_w3 = slot.early_total_w3.saturating_add(1),
        JudgeGrade::Decent if include_bad => {
            slot.early_total_w4 = slot.early_total_w4.saturating_add(1)
        }
        JudgeGrade::WayOff if include_bad => {
            slot.early_total_w5 = slot.early_total_w5.saturating_add(1)
        }
        _ => {}
    }
}

#[inline(always)]
const fn column_judgment_col(note: &Note, col_offset: usize, cols: usize) -> Option<usize> {
    if note.column < col_offset {
        return None;
    }
    let col = note.column - col_offset;
    if col < cols { Some(col) } else { None }
}

#[inline(always)]
const fn note_counts_for_column_judgments(note: &Note) -> bool {
    !note.is_fake && note.can_be_judged && !matches!(note.note_type, NoteType::Mine)
}

#[inline(always)]
fn add_column_judgment(slot: &mut ColumnJudgments, judgment: &Judgment, show_fa_plus_window: bool) {
    match judgment.grade {
        JudgeGrade::Fantastic => match judgment.window {
            Some(TimingWindow::W0) => slot.w0 = slot.w0.saturating_add(1),
            _ => {
                slot.w1 = slot.w1.saturating_add(1);
                if show_fa_plus_window && judgment.time_error_ms < 0.0 {
                    slot.early_w1 = slot.early_w1.saturating_add(1);
                }
            }
        },
        JudgeGrade::Excellent => {
            slot.w2 = slot.w2.saturating_add(1);
            if judgment.time_error_ms < 0.0 {
                slot.early_w2 = slot.early_w2.saturating_add(1);
            }
        }
        JudgeGrade::Great => {
            slot.w3 = slot.w3.saturating_add(1);
            if judgment.time_error_ms < 0.0 {
                slot.early_w3 = slot.early_w3.saturating_add(1);
            }
        }
        JudgeGrade::Decent => {
            slot.w4 = slot.w4.saturating_add(1);
            if judgment.time_error_ms < 0.0 {
                slot.early_w4 = slot.early_w4.saturating_add(1);
            }
        }
        JudgeGrade::WayOff => {
            slot.w5 = slot.w5.saturating_add(1);
            if judgment.time_error_ms < 0.0 {
                slot.early_w5 = slot.early_w5.saturating_add(1);
            }
        }
        JudgeGrade::Miss => {
            slot.miss = slot.miss.saturating_add(1);
        }
    }
}

pub fn compute_column_judgments(
    notes: &[Note],
    eligible: &[bool],
    cols_per_player: usize,
    col_offset: usize,
    show_fa_plus_window: bool,
) -> ColumnJudgmentList {
    assert_eq!(
        notes.len(),
        eligible.len(),
        "column-judgment eligibility must align with notes"
    );
    let cols = cols_per_player;
    let mut out = ColumnJudgmentList::with_capacity(cols);
    out.resize(cols, ColumnJudgments::default());
    if cols == 0 {
        return out;
    }

    let mut row_start = 0;
    while row_start < notes.len() {
        let row_index = notes[row_start].row_index;
        let mut row_has_unjudged_note = false;
        let mut row_judgment = None;
        let mut row_early_judgment = None;
        let mut row_end = row_start;
        while row_end < notes.len() && notes[row_end].row_index == row_index {
            let note = &notes[row_end];
            if eligible[row_end]
                && note_counts_for_column_judgments(note)
                && column_judgment_col(note, col_offset, cols).is_some()
            {
                if let Some(candidate) = note.result.as_ref() {
                    judgment::select_row_final_judgment(&mut row_judgment, candidate);
                    if row_judgment.is_some_and(|chosen| std::ptr::eq(chosen, candidate)) {
                        row_early_judgment = note.early_result.as_ref();
                    }
                } else {
                    row_has_unjudged_note = true;
                }
            }
            row_end += 1;
        }
        if row_has_unjudged_note {
            row_start = row_end;
            continue;
        }
        let Some(row_judgment) = row_judgment else {
            row_start = row_end;
            continue;
        };

        for (note, &eligible) in notes[row_start..row_end]
            .iter()
            .zip(&eligible[row_start..row_end])
        {
            if !eligible || !note_counts_for_column_judgments(note) {
                continue;
            }
            let Some(col) = column_judgment_col(note, col_offset, cols) else {
                continue;
            };
            let slot = &mut out[col];
            add_column_judgment(slot, row_judgment, show_fa_plus_window);
            if row_judgment.grade == JudgeGrade::Miss
                && !matches!(note.note_type, NoteType::Lift)
                && note.result.as_ref().is_some_and(|j| j.miss_because_held)
            {
                slot.held_miss = slot.held_miss.saturating_add(1);
            }
            if let Some(early) = row_early_judgment {
                add_early_total(slot, row_judgment, false);
                add_early_total(slot, early, true);
            }
        }

        row_start = row_end;
    }

    out
}

#[cfg(any(test, feature = "bench-support"))]
pub fn compute_column_judgments_reference(
    notes: &[Note],
    eligible: &[bool],
    cols_per_player: usize,
    col_offset: usize,
    show_fa_plus_window: bool,
) -> Vec<ColumnJudgments> {
    assert_eq!(
        notes.len(),
        eligible.len(),
        "column-judgment eligibility must align with notes"
    );
    let cols = cols_per_player;
    let mut out = vec![ColumnJudgments::default(); cols];
    if cols == 0 {
        return out;
    }

    let mut row_start = 0;
    while row_start < notes.len() {
        let row_index = notes[row_start].row_index;
        let mut row_end = row_start + 1;
        while row_end < notes.len() && notes[row_end].row_index == row_index {
            row_end += 1;
        }

        let row_notes = &notes[row_start..row_end];
        let row_eligible = &eligible[row_start..row_end];
        let mut row_has_unjudged_note = false;
        let row_judgment =
            judgment::aggregate_row_final_judgment(row_notes.iter().zip(row_eligible).filter_map(
                |(note, &eligible)| {
                    if !eligible {
                        return None;
                    }
                    if !note_counts_for_column_judgments(note)
                        || column_judgment_col(note, col_offset, cols).is_none()
                    {
                        return None;
                    }
                    let Some(judgment) = note.result.as_ref() else {
                        row_has_unjudged_note = true;
                        return None;
                    };
                    Some(judgment)
                },
            ));
        if row_has_unjudged_note {
            row_start = row_end;
            continue;
        }
        let Some(row_judgment) = row_judgment else {
            row_start = row_end;
            continue;
        };
        let row_early_judgment = row_notes
            .iter()
            .find(|note| {
                note.result
                    .as_ref()
                    .is_some_and(|j| std::ptr::eq(j, row_judgment))
            })
            .and_then(|note| note.early_result.as_ref());

        for (note, &eligible) in row_notes.iter().zip(row_eligible) {
            if !eligible || !note_counts_for_column_judgments(note) {
                continue;
            }
            let Some(col) = column_judgment_col(note, col_offset, cols) else {
                continue;
            };

            let slot = &mut out[col];
            add_column_judgment(slot, row_judgment, show_fa_plus_window);
            if row_judgment.grade == JudgeGrade::Miss
                && !matches!(note.note_type, NoteType::Lift)
                && note.result.as_ref().is_some_and(|j| j.miss_because_held)
            {
                slot.held_miss = slot.held_miss.saturating_add(1);
            }

            if let Some(early) = row_early_judgment {
                add_early_total(slot, row_judgment, false);
                add_early_total(slot, early, true);
            }
        }

        row_start = row_end;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap_note(column: usize, result: Judgment, early_result: Option<Judgment>) -> Note {
        Note {
            beat: 0.0,
            quantization_idx: 0,
            column,
            note_type: NoteType::Tap,
            row_index: 0,
            result: Some(result),
            early_result,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    fn judgment(grade: JudgeGrade, window: Option<TimingWindow>, time_error_ms: f32) -> Judgment {
        Judgment {
            time_error_ms,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(time_error_ms, 1.0),
            grade,
            window,
            miss_because_held: false,
        }
    }

    #[test]
    fn tracks_split_white_early_fantastics() {
        let notes = [tap_note(
            0,
            judgment(JudgeGrade::Fantastic, Some(TimingWindow::W1), -8.0),
            None,
        )];

        let with_fa = compute_column_judgments(&notes, &[true], 1, 0, true);
        let without_fa = compute_column_judgments(&notes, &[true], 1, 0, false);

        assert_eq!(with_fa[0].w1, 1);
        assert_eq!(with_fa[0].early_w1, 1);
        assert_eq!(without_fa[0].early_w1, 0);
    }

    #[test]
    fn tracks_rescored_early_totals() {
        let notes = [tap_note(
            0,
            judgment(JudgeGrade::Excellent, Some(TimingWindow::W2), -18.0),
            Some(judgment(JudgeGrade::WayOff, Some(TimingWindow::W5), -18.0)),
        )];

        let out = compute_column_judgments(&notes, &[true], 1, 0, false);

        assert_eq!(out[0].w2, 1);
        assert_eq!(out[0].early_w2, 1);
        assert_eq!(out[0].early_total_w2, 1);
        assert_eq!(out[0].early_total_w5, 1);
    }

    #[test]
    fn tracks_w0_rescore_target() {
        let notes = [tap_note(
            0,
            judgment(JudgeGrade::Fantastic, Some(TimingWindow::W0), -4.0),
            Some(judgment(JudgeGrade::Decent, Some(TimingWindow::W4), -16.0)),
        )];

        let out = compute_column_judgments(&notes, &[true], 1, 0, true);

        assert_eq!(out[0].w0, 1);
        assert_eq!(out[0].early_total_w0, 1);
        assert_eq!(out[0].early_total_w4, 1);
    }

    #[test]
    fn uses_row_judgment_for_jump_columns() {
        let notes = [
            tap_note(
                0,
                judgment(JudgeGrade::WayOff, Some(TimingWindow::W5), -140.0),
                None,
            ),
            tap_note(
                1,
                judgment(JudgeGrade::Fantastic, Some(TimingWindow::W1), 8.0),
                None,
            ),
        ];

        let out = compute_column_judgments(&notes, &[true; 2], 4, 0, false);

        assert_eq!(out[0].w1, 1);
        assert_eq!(out[1].w1, 1);
        assert_eq!(out[0].w5, 0);
        assert_eq!(out[1].w5, 0);
    }

    #[test]
    fn keeps_miss_priority_for_jump_columns() {
        let notes = [
            tap_note(0, judgment(JudgeGrade::Miss, None, 0.0), None),
            tap_note(
                1,
                judgment(JudgeGrade::Fantastic, Some(TimingWindow::W1), 8.0),
                None,
            ),
        ];

        let out = compute_column_judgments(&notes, &[true; 2], 4, 0, false);

        assert_eq!(out[0].miss, 1);
        assert_eq!(out[1].miss, 1);
        assert_eq!(out[0].w1, 0);
        assert_eq!(out[1].w1, 0);
    }

    #[test]
    fn keeps_held_miss_on_raw_column() {
        let mut held_miss = judgment(JudgeGrade::Miss, None, 0.0);
        held_miss.miss_because_held = true;
        let notes = [
            tap_note(0, held_miss, None),
            tap_note(
                1,
                judgment(JudgeGrade::Fantastic, Some(TimingWindow::W1), 8.0),
                None,
            ),
        ];

        let out = compute_column_judgments(&notes, &[true; 2], 4, 0, false);

        assert_eq!(out[0].miss, 1);
        assert_eq!(out[1].miss, 1);
        assert_eq!(out[0].held_miss, 1);
        assert_eq!(out[1].held_miss, 0);
    }

    #[test]
    fn counts_fatal_row_and_excludes_rows_after_death() {
        let mut before = tap_note(
            0,
            judgment(JudgeGrade::Fantastic, Some(TimingWindow::W1), 4.0),
            None,
        );
        before.row_index = 0;
        let mut fatal = tap_note(0, judgment(JudgeGrade::Miss, None, 0.0), None);
        fatal.row_index = 48;
        let mut after = tap_note(
            0,
            judgment(JudgeGrade::Excellent, Some(TimingWindow::W2), 18.0),
            None,
        );
        after.row_index = 96;
        let notes = [before, fatal, after];

        let out = compute_column_judgments(&notes, &[true, true, false], 1, 0, false);

        assert_eq!(out[0].w1, 1);
        assert_eq!(out[0].miss, 1);
        assert_eq!(out[0].w2, 0);
    }

    #[test]
    fn fused_scan_matches_reference_across_mixed_rows_and_players() {
        let mut notes = Vec::new();
        let mut eligible = Vec::new();
        for row in 0usize..64 {
            let width = 1 + row % 8;
            for col in 0..width {
                let grade = match (row + col) % 7 {
                    0 => JudgeGrade::Fantastic,
                    1 => JudgeGrade::Excellent,
                    2 => JudgeGrade::Great,
                    3 => JudgeGrade::Decent,
                    4 => JudgeGrade::WayOff,
                    5 => JudgeGrade::Miss,
                    _ => JudgeGrade::Fantastic,
                };
                let window = match grade {
                    JudgeGrade::Fantastic if (row + col).is_multiple_of(2) => {
                        Some(TimingWindow::W0)
                    }
                    JudgeGrade::Fantastic => Some(TimingWindow::W1),
                    JudgeGrade::Excellent => Some(TimingWindow::W2),
                    JudgeGrade::Great => Some(TimingWindow::W3),
                    JudgeGrade::Decent => Some(TimingWindow::W4),
                    JudgeGrade::WayOff => Some(TimingWindow::W5),
                    JudgeGrade::Miss => None,
                };
                let offset = (row * 13 + col * 29) as f32 % 281.0 - 140.0;
                let mut result = judgment(grade, window, offset);
                result.miss_because_held = grade == JudgeGrade::Miss && col.is_multiple_of(2);
                let mut note = tap_note(
                    col,
                    result,
                    (row + col)
                        .is_multiple_of(3)
                        .then(|| judgment(JudgeGrade::WayOff, Some(TimingWindow::W5), -96.0)),
                );
                note.row_index = row * 48;
                note.note_type = if (row + col).is_multiple_of(11) {
                    NoteType::Lift
                } else if (row + col).is_multiple_of(13) {
                    NoteType::Mine
                } else {
                    NoteType::Tap
                };
                note.is_fake = (row + col).is_multiple_of(17);
                note.can_be_judged = !(row + col).is_multiple_of(19);
                if row == 37 && col == 0 {
                    note.result = None;
                }
                notes.push(note);
                eligible.push(!(row + col).is_multiple_of(23));
            }
        }

        for (cols, offset) in [(4, 0), (4, 4), (8, 0)] {
            for show_fa_plus_window in [false, true] {
                let expected = compute_column_judgments_reference(
                    &notes,
                    &eligible,
                    cols,
                    offset,
                    show_fa_plus_window,
                );
                let actual =
                    compute_column_judgments(&notes, &eligible, cols, offset, show_fa_plus_window);
                assert_eq!(actual.as_slice(), expected.as_slice());
                assert!(!actual.spilled());
            }
        }
    }

    #[test]
    fn wide_output_preserves_reference_behavior_with_heap_fallback() {
        let notes = (0..16)
            .map(|column| {
                tap_note(
                    column,
                    judgment(JudgeGrade::Fantastic, Some(TimingWindow::W1), column as f32),
                    None,
                )
            })
            .collect::<Vec<_>>();
        let expected = compute_column_judgments_reference(&notes, &[true; 16], 16, 0, true);
        let actual = compute_column_judgments(&notes, &[true; 16], 16, 0, true);

        assert_eq!(actual.as_slice(), expected.as_slice());
        assert!(actual.spilled());
    }

    #[test]
    fn selected_tie_keeps_later_notes_early_rescore() {
        let notes = [
            tap_note(
                0,
                judgment(JudgeGrade::Excellent, Some(TimingWindow::W2), 12.0),
                Some(judgment(JudgeGrade::Decent, Some(TimingWindow::W4), -80.0)),
            ),
            tap_note(
                1,
                judgment(JudgeGrade::Great, Some(TimingWindow::W3), 12.0),
                Some(judgment(JudgeGrade::WayOff, Some(TimingWindow::W5), -96.0)),
            ),
        ];

        let actual = compute_column_judgments(&notes, &[true; 2], 2, 0, true);

        assert_eq!(actual[0].w3, 1);
        assert_eq!(actual[1].w3, 1);
        assert_eq!(actual[0].early_total_w3, 1);
        assert_eq!(actual[0].early_total_w5, 1);
        assert_eq!(actual[0].early_total_w4, 0);
    }
}
