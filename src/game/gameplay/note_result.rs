use deadsync_rules::judgment::Judgment;

use super::{
    HeldMissRenderInfo, MAX_COLS, State, apply_final_note_result, mark_row_entry_note_finalized,
    mark_row_entry_provisional_early_result, register_provisional_early_note_result,
    trigger_column_flash_for_grade,
};

#[inline(always)]
pub(super) fn register_provisional_early_result(
    state: &mut State,
    note_index: usize,
    judgment: Judgment,
) {
    if !register_provisional_early_note_result(&mut state.notes[note_index], judgment) {
        return;
    }
    mark_row_entry_provisional_early_result(
        &mut state.row_entries,
        &state.note_row_entry_indices,
        note_index,
    );
}

#[inline(always)]
pub(super) fn set_final_note_result(state: &mut State, note_index: usize, judgment: Judgment) {
    let effects = apply_final_note_result(&mut state.notes[note_index], judgment, MAX_COLS);
    if effects.mark_row_finalized {
        let note_type = state.notes[note_index].note_type;
        mark_row_entry_note_finalized(
            &mut state.row_entries,
            &state.note_row_entry_indices,
            note_index,
            note_type,
        );
    }
    if let Some(column) = effects.trigger_miss_flash_column {
        trigger_column_flash_for_grade(state, column, judgment.grade);
    }
    if let Some(column) = effects.held_miss_column {
        state.held_miss_judgments[column] = Some(HeldMissRenderInfo {
            started_at_screen_s: state.total_elapsed_in_screen,
        });
    }
}
