use deadsync_core::note::NoteType;
use deadsync_rules::judgment::{JudgeGrade, Judgment};

use super::{HeldMissRenderInfo, MAX_COLS, State};

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
    note_index: usize,
    judgment: Judgment,
) {
    if state.notes[note_index].early_result.is_some() {
        return;
    }
    state.notes[note_index].early_result = Some(judgment);
    mark_row_provisional_early_result(state, note_index);
}

#[inline(always)]
pub(super) fn set_final_note_result(state: &mut State, note_index: usize, judgment: Judgment) {
    let was_unjudged = state.notes[note_index].result.is_none();
    state.notes[note_index].result = Some(judgment);
    if was_unjudged {
        mark_row_note_finalized(state, note_index);
        if judgment.grade == JudgeGrade::Miss && judgment.miss_because_held {
            let column = state.notes[note_index].column;
            if column < MAX_COLS {
                state.held_miss_judgments[column] = Some(HeldMissRenderInfo {
                    started_at_screen_s: state.total_elapsed_in_screen,
                });
            }
        }
    }
}
