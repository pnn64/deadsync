use crate::game::judgment::{JudgeGrade, Judgment};
use crate::game::note::NoteType;

use super::{PlayerRuntime, State, display_judge_ix};

#[inline(always)]
pub(super) fn add_provisional_early_score(p: &mut PlayerRuntime, grade: JudgeGrade) {
    let grade_ix = display_judge_ix(grade);
    p.provisional_scoring_counts[grade_ix] =
        p.provisional_scoring_counts[grade_ix].saturating_add(1);
}

#[inline(always)]
pub(super) fn remove_provisional_early_score(p: &mut PlayerRuntime, grade: JudgeGrade) {
    let grade_ix = display_judge_ix(grade);
    p.provisional_scoring_counts[grade_ix] =
        p.provisional_scoring_counts[grade_ix].saturating_sub(1);
}

#[inline(always)]
fn mark_row_provisional_early_result(state: &mut State, note_index: usize) {
    let Some(&row_entry_index) = state.note_row_entry_indices.get(note_index) else {
        return;
    };
    if row_entry_index == u32::MAX {
        return;
    }
    if let Some(row_entry) = state.row_entries.get_mut(row_entry_index as usize) {
        row_entry.had_provisional_early_hit = true;
    }
}

#[inline(always)]
fn mark_row_note_finalized(state: &mut State, note_index: usize) {
    let Some(&row_entry_index) = state.note_row_entry_indices.get(note_index) else {
        return;
    };
    if row_entry_index == u32::MAX {
        return;
    }
    let Some(row_entry) = state.row_entries.get_mut(row_entry_index as usize) else {
        return;
    };
    row_entry.unresolved_count = row_entry.unresolved_count.saturating_sub(1);
    if state.notes[note_index].note_type != NoteType::Lift {
        row_entry.unresolved_nonlift_count = row_entry.unresolved_nonlift_count.saturating_sub(1);
    }
}

#[inline(always)]
pub(super) fn register_provisional_early_result(
    state: &mut State,
    player: usize,
    note_index: usize,
    judgment: Judgment,
) {
    if state.notes[note_index].early_result.is_some() {
        return;
    }
    add_provisional_early_score(&mut state.players[player], judgment.grade);
    state.notes[note_index].early_result = Some(judgment);
    mark_row_provisional_early_result(state, note_index);
}

#[inline(always)]
pub(super) fn set_final_note_result(
    state: &mut State,
    player: usize,
    note_index: usize,
    judgment: Judgment,
) {
    let was_unjudged = state.notes[note_index].result.is_none();
    if was_unjudged && let Some(early) = state.notes[note_index].early_result.as_ref() {
        remove_provisional_early_score(&mut state.players[player], early.grade);
    }
    state.notes[note_index].result = Some(judgment);
    if was_unjudged {
        mark_row_note_finalized(state, note_index);
    }
}
