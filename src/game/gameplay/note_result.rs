use deadsync_rules::judgment::Judgment;

use super::{
    MAX_COLS, State, apply_final_note_result_to_rows, apply_provisional_early_note_result,
    held_miss_render_info, trigger_column_flash_for_grade,
};

#[inline(always)]
pub(super) fn register_provisional_early_result(
    state: &mut State,
    note_index: usize,
    judgment: Judgment,
) {
    apply_provisional_early_note_result(
        &mut state.notes,
        &mut state.row_entries,
        &state.note_row_entry_indices,
        note_index,
        judgment,
    );
}

#[inline(always)]
pub(super) fn set_final_note_result(state: &mut State, note_index: usize, judgment: Judgment) {
    let update = apply_final_note_result_to_rows(
        &mut state.notes,
        &mut state.row_entries,
        &state.note_row_entry_indices,
        note_index,
        judgment,
        MAX_COLS,
    );
    let effects = update.effects;
    if let Some(column) = effects.trigger_miss_flash_column {
        trigger_column_flash_for_grade(state, column, judgment.grade);
    }
    if let Some(column) = effects.held_miss_column {
        state.held_miss_judgments[column] =
            Some(held_miss_render_info(state.total_elapsed_in_screen));
    }
}
