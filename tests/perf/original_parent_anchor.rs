// Frozen from main 4e59125883b7cf5331779a0a95627fea47930fd9 for paired regression checks and benchmarks.
pub(super) fn parent_anchor_visible_index(
    row_map: &RowMap,
    parent_id: RowId,
    visibility: RowVisibility,
) -> Option<i32> {
    row_map
        .display_order()
        .iter()
        .position(|&id| id == parent_id)
        .and_then(|idx| row_to_visible_index(row_map, idx, visibility))
        .map(|idx| idx as i32)
}

pub(super) fn hidden_row_anchor_visible_index(
    row_map: &RowMap,
    row_idx: usize,
    visibility: RowVisibility,
) -> Option<i32> {
    let row = row_map.get_at(row_idx)?;
    let parent_id = conditional_row_parent(row.id)?;
    parent_anchor_visible_index(row_map, parent_id, visibility)
}
