use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct RowVisibility {
    pub(super) show_measure_counter_children: bool,
    pub(super) show_judgment_offsets: bool,
    pub(super) show_judgment_tilt_intensity: bool,
    pub(super) show_combo_offsets: bool,
    pub(super) show_error_bar_children: bool,
    pub(super) show_custom_fantastic_window_ms: bool,
    pub(super) show_density_graph_background: bool,
    pub(super) show_target_score: bool,
    pub(super) show_combo_rows: bool,
    pub(super) show_lifebar_rows: bool,
    pub(super) show_indicator_score_type: bool,
    pub(super) show_global_offset_shift: bool,
}

#[inline(always)]
pub(super) fn row_visible_with_flags(id: RowId, visibility: RowVisibility) -> bool {
    if id == RowId::MeasureCounterLookahead || id == RowId::MeasureCounterOptions {
        return visibility.show_measure_counter_children;
    }
    if id == RowId::JudgmentOffsetX || id == RowId::JudgmentOffsetY {
        return visibility.show_judgment_offsets;
    }
    if id == RowId::JudgmentTiltIntensity {
        return visibility.show_judgment_tilt_intensity;
    }
    if id == RowId::ComboOffsetX || id == RowId::ComboOffsetY {
        return visibility.show_combo_offsets;
    }
    if id == RowId::ErrorBarTrim
        || id == RowId::ErrorBarOptions
        || id == RowId::ErrorBarOffsetX
        || id == RowId::ErrorBarOffsetY
    {
        return visibility.show_error_bar_children;
    }
    if id == RowId::CustomBlueFantasticWindowMs {
        return visibility.show_custom_fantastic_window_ms;
    }
    if id == RowId::DensityGraphBackground {
        return visibility.show_density_graph_background;
    }
    if id == RowId::TargetScore {
        return visibility.show_target_score;
    }
    if id == RowId::ComboColors || id == RowId::ComboColorMode || id == RowId::CarryCombo {
        return visibility.show_combo_rows;
    }
    if id == RowId::LifeMeterType || id == RowId::LifeBarOptions {
        return visibility.show_lifebar_rows;
    }
    if id == RowId::IndicatorScoreType {
        return visibility.show_indicator_score_type;
    }
    if id == RowId::GlobalOffsetShift {
        return visibility.show_global_offset_shift;
    }
    true
}

#[inline(always)]
pub(super) fn conditional_row_parent(id: RowId) -> Option<RowId> {
    if id == RowId::MeasureCounterLookahead || id == RowId::MeasureCounterOptions {
        return Some(RowId::MeasureCounter);
    }
    if id == RowId::JudgmentOffsetX || id == RowId::JudgmentOffsetY {
        return Some(RowId::JudgmentFont);
    }
    if id == RowId::JudgmentTiltIntensity {
        return Some(RowId::JudgmentTilt);
    }
    if id == RowId::ComboOffsetX || id == RowId::ComboOffsetY {
        return Some(RowId::ComboFont);
    }
    if id == RowId::ErrorBarTrim
        || id == RowId::ErrorBarOptions
        || id == RowId::ErrorBarOffsetX
        || id == RowId::ErrorBarOffsetY
    {
        return Some(RowId::ErrorBar);
    }
    if id == RowId::CustomBlueFantasticWindowMs {
        return Some(RowId::CustomBlueFantasticWindow);
    }
    if id == RowId::DensityGraphBackground {
        return Some(RowId::DataVisualizations);
    }
    if id == RowId::ComboColors
        || id == RowId::ComboColorMode
        || id == RowId::CarryCombo
        || id == RowId::LifeMeterType
        || id == RowId::LifeBarOptions
    {
        return Some(RowId::Hide);
    }
    if id == RowId::IndicatorScoreType {
        return Some(RowId::MiniIndicator);
    }
    None
}

pub(super) fn measure_counter_children_visible(
    row_map: &RowMap,
    active: [bool; PLAYER_SLOTS],
) -> bool {
    let Some(row) = row_map.get(RowId::MeasureCounter) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        if choice_idx != 0 {
            return true;
        }
    }
    !any_active
}

pub(super) fn judgment_offsets_visible(row_map: &RowMap, active: [bool; PLAYER_SLOTS]) -> bool {
    let Some(row) = row_map.get(RowId::JudgmentFont) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        // "None" is always the last choice for font/texture rows.
        if choice_idx != max_choice {
            return true;
        }
    }
    !any_active
}

#[inline(always)]
pub(super) fn judgment_tilt_intensity_visible(
    row_map: &RowMap,
    active: [bool; PLAYER_SLOTS],
) -> bool {
    let Some(row) = row_map.get(RowId::JudgmentTilt) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        if choice_idx != 0 {
            return true;
        }
    }
    !any_active
}

pub(super) fn combo_offsets_visible(row_map: &RowMap, active: [bool; PLAYER_SLOTS]) -> bool {
    let Some(row) = row_map.get(RowId::ComboFont) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        // "None" is always the last choice for font/texture rows.
        if choice_idx != max_choice {
            return true;
        }
    }
    !any_active
}

pub(super) fn error_bar_children_visible(
    active: [bool; PLAYER_SLOTS],
    option_masks: [PlayerOptionMasks; PLAYER_SLOTS],
) -> bool {
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        if !option_masks[player_idx].error_bar.is_empty() {
            return true;
        }
    }
    !any_active
}

pub(super) fn custom_fantastic_window_ms_visible(
    row_map: &RowMap,
    active: [bool; PLAYER_SLOTS],
) -> bool {
    let Some(row) = row_map.get(RowId::CustomBlueFantasticWindow) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        if choice_idx != 0 {
            return true;
        }
    }
    !any_active
}

pub(super) fn density_graph_background_visible(
    row_map: &RowMap,
    active: [bool; PLAYER_SLOTS],
) -> bool {
    let Some(row) = row_map.get(RowId::DataVisualizations) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        if choice_idx == 2 {
            return true;
        }
    }
    !any_active
}

#[inline(always)]
fn selected_choice(row_map: &RowMap, id: RowId, player_idx: usize) -> Option<usize> {
    let row = row_map.get(id)?;
    let max_choice = row.choices.len().saturating_sub(1);
    Some(row.selected_choice_index[player_idx].min(max_choice))
}

pub(super) fn target_score_visible(row_map: &RowMap, active: [bool; PLAYER_SLOTS]) -> bool {
    let has_trigger_row = row_map.get(RowId::DataVisualizations).is_some()
        || row_map.get(RowId::MiniIndicator).is_some()
        || row_map.get(RowId::ActionOnMissedTarget).is_some();
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        if selected_choice(row_map, RowId::DataVisualizations, player_idx)
            .and_then(|idx| DATA_VISUALIZATIONS_VARIANTS.get(idx))
            .is_some_and(|&v| v == crate::game::profile::DataVisualizations::TargetScoreGraph)
        {
            return true;
        }
        if selected_choice(row_map, RowId::MiniIndicator, player_idx)
            .and_then(|idx| MINI_INDICATOR_VARIANTS.get(idx))
            .is_some_and(|&v| v == crate::game::profile::MiniIndicator::Pacemaker)
        {
            return true;
        }
        if selected_choice(row_map, RowId::ActionOnMissedTarget, player_idx)
            .is_some_and(|idx| idx != 0)
        {
            return true;
        }
    }
    !any_active || !has_trigger_row
}

pub(super) fn combo_rows_visible(
    active: [bool; PLAYER_SLOTS],
    option_masks: [PlayerOptionMasks; PLAYER_SLOTS],
) -> bool {
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let hide_combo = option_masks[player_idx].hide.contains(HideMask::COMBO);
        if !hide_combo {
            return true;
        }
    }
    !any_active
}

pub(super) fn lifebar_rows_visible(
    active: [bool; PLAYER_SLOTS],
    option_masks: [PlayerOptionMasks; PLAYER_SLOTS],
) -> bool {
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let hide_lifebar = option_masks[player_idx].hide.contains(HideMask::LIFE);
        if !hide_lifebar {
            return true;
        }
    }
    !any_active
}

pub(super) fn indicator_score_type_visible(row_map: &RowMap, active: [bool; PLAYER_SLOTS]) -> bool {
    let Some(row) = row_map.get(RowId::MiniIndicator) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        // Visible for Subtractive(1), Predictive(2), Pace(3)
        if (1..=3).contains(&choice_idx) {
            return true;
        }
    }
    !any_active
}

#[inline(always)]
pub(super) fn row_visibility(
    row_map: &RowMap,
    active: [bool; PLAYER_SLOTS],
    option_masks: [PlayerOptionMasks; PLAYER_SLOTS],
    allow_per_player_global_offsets: bool,
) -> RowVisibility {
    RowVisibility {
        show_measure_counter_children: measure_counter_children_visible(row_map, active),
        show_judgment_offsets: judgment_offsets_visible(row_map, active),
        show_judgment_tilt_intensity: judgment_tilt_intensity_visible(row_map, active),
        show_combo_offsets: combo_offsets_visible(row_map, active),
        show_error_bar_children: error_bar_children_visible(active, option_masks),
        show_custom_fantastic_window_ms: custom_fantastic_window_ms_visible(row_map, active),
        show_density_graph_background: density_graph_background_visible(row_map, active),
        show_target_score: target_score_visible(row_map, active),
        show_combo_rows: combo_rows_visible(active, option_masks),
        show_lifebar_rows: lifebar_rows_visible(active, option_masks),
        show_indicator_score_type: indicator_score_type_visible(row_map, active),
        show_global_offset_shift: allow_per_player_global_offsets,
    }
}

#[inline(always)]
pub(super) fn is_row_visible(row_map: &RowMap, row_idx: usize, visibility: RowVisibility) -> bool {
    row_map
        .display_order()
        .get(row_idx)
        .and_then(|&id| row_map.get(id))
        .is_some_and(|row| row_visible_with_flags(row.id, visibility))
}

pub(super) fn count_visible_rows(row_map: &RowMap, visibility: RowVisibility) -> usize {
    row_map
        .display_order()
        .iter()
        .filter_map(|&id| row_map.get(id))
        .filter(|row| row_visible_with_flags(row.id, visibility))
        .count()
}

pub(super) fn row_to_visible_index(
    row_map: &RowMap,
    row_idx: usize,
    visibility: RowVisibility,
) -> Option<usize> {
    if row_idx >= row_map.display_order().len() {
        return None;
    }
    if !is_row_visible(row_map, row_idx, visibility) {
        return None;
    }
    let mut pos = 0usize;
    for i in 0..row_idx {
        if is_row_visible(row_map, i, visibility) {
            pos += 1;
        }
    }
    Some(pos)
}

pub(super) fn fallback_visible_row(
    row_map: &RowMap,
    row_idx: usize,
    visibility: RowVisibility,
) -> Option<usize> {
    if row_map.display_order().is_empty() {
        return None;
    }
    let start = row_idx.min(row_map.display_order().len().saturating_sub(1));
    for i in start..row_map.display_order().len() {
        if is_row_visible(row_map, i, visibility) {
            return Some(i);
        }
    }
    (0..start)
        .rev()
        .find(|&i| is_row_visible(row_map, i, visibility))
}

pub(super) fn next_visible_row(
    row_map: &RowMap,
    current_row: usize,
    dir: NavDirection,
    visibility: RowVisibility,
    wrap: NavWrap,
) -> Option<usize> {
    if row_map.display_order().is_empty() {
        return None;
    }
    let len = row_map.display_order().len();
    let mut idx = current_row.min(len.saturating_sub(1));
    if !is_row_visible(row_map, idx, visibility) {
        idx = fallback_visible_row(row_map, idx, visibility)?;
    }
    for _ in 0..len {
        idx = match dir {
            NavDirection::Up => {
                if idx == 0 {
                    match wrap {
                        NavWrap::Wrap => len - 1,
                        NavWrap::Clamp => return None,
                    }
                } else {
                    idx - 1
                }
            }
            NavDirection::Down => {
                if idx + 1 >= len {
                    match wrap {
                        NavWrap::Wrap => 0,
                        NavWrap::Clamp => return None,
                    }
                } else {
                    idx + 1
                }
            }
            NavDirection::Left | NavDirection::Right => return Some(idx),
        };
        if is_row_visible(row_map, idx, visibility) {
            return Some(idx);
        }
    }
    None
}

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

pub(super) fn sync_selected_rows_with_visibility(state: &mut State, active: [bool; PLAYER_SLOTS]) {
    if state.pane().row_map.is_empty() {
        state.pane_mut().selected_row = [0; PLAYER_SLOTS];
        state.pane_mut().prev_selected_row = [0; PLAYER_SLOTS];
        return;
    }
    let visibility = row_visibility(
        &state.pane().row_map,
        active,
        state.option_masks,
        state.allow_per_player_global_offsets,
    );
    for player_idx in [P1, P2] {
        let idx =
            state.pane().selected_row[player_idx].min(state.pane().row_map.len().saturating_sub(1));
        if is_row_visible(&state.pane().row_map, idx, visibility) {
            state.pane_mut().selected_row[player_idx] = idx;
            continue;
        }
        if let Some(fallback) = fallback_visible_row(&state.pane().row_map, idx, visibility) {
            state.pane_mut().selected_row[player_idx] = fallback;
            if active[player_idx] {
                state.pane_mut().prev_selected_row[player_idx] = fallback;
            }
        }
    }
}

#[inline(always)]
pub(super) fn row_allows_arcade_next_row(state: &State, row_idx: usize) -> bool {
    arcade_options_navigation_active()
        && pane_uses_arcade_next_row(state.current_pane)
        && state
            .pane()
            .row_map
            .get_at(row_idx)
            .is_some_and(|row| row.id != RowId::Exit && row_supports_inline_nav(row))
}

#[inline(always)]
pub(super) fn arcade_row_uses_choice_focus(state: &State, player_idx: usize) -> bool {
    if !arcade_options_navigation_active() || !pane_uses_arcade_next_row(state.current_pane) {
        return false;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_idx = state.pane().selected_row[idx].min(state.pane().row_map.len().saturating_sub(1));
    state
        .pane()
        .row_map
        .display_order()
        .get(row_idx)
        .and_then(|&id| state.pane().row_map.get(id))
        .is_some_and(row_supports_inline_nav)
}
