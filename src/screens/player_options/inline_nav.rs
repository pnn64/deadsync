use super::*;

pub(super) fn inline_choice_centers(
    choices: &[String],
    asset_manager: &AssetManager,
    left_x: f32,
) -> Vec<f32> {
    if choices.is_empty() {
        return Vec::new();
    }
    let mut centers: Vec<f32> = Vec::with_capacity(choices.len());
    let mut x = left_x;
    let zoom = 0.835_f32;
    for text in choices {
        let (draw_w, _) = measure_option_text(asset_manager, text, zoom);
        centers.push(draw_w.mul_add(0.5, x));
        x += draw_w + INLINE_SPACING;
    }
    centers
}

pub(super) fn focused_inline_choice_index(
    state: &State,
    asset_manager: &AssetManager,
    player_idx: usize,
    row_idx: usize,
) -> Option<usize> {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row = state
        .pane()
        .row_map
        .display_order()
        .get(row_idx)
        .and_then(|&id| state.pane().row_map.get(id))?;
    if !row_supports_inline_nav(row) {
        return None;
    }
    let centers = inline_choice_centers(
        &row.choices,
        asset_manager,
        inline_choice_left_x_for_row(state, row_idx),
    );
    if centers.is_empty() {
        return None;
    }
    let mut focus_idx = row.selected_choice_index[idx].min(centers.len().saturating_sub(1));
    let anchor_x = state.pane().inline_choice_x[idx];
    if anchor_x.is_finite() {
        let mut best_dist = f32::INFINITY;
        for (i, &center_x) in centers.iter().enumerate() {
            let dist = (center_x - anchor_x).abs();
            if dist < best_dist {
                best_dist = dist;
                focus_idx = i;
            }
        }
    }
    Some(focus_idx)
}

pub(super) fn move_inline_focus(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    delta: isize,
) -> bool {
    if state.pane().row_map.is_empty() || delta == 0 {
        return false;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_idx = state.pane().selected_row[idx].min(state.pane().row_map.len().saturating_sub(1));
    let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_idx)
        .and_then(|&id| state.pane().row_map.get(id))
    else {
        return false;
    };
    if !row_supports_inline_nav(row) {
        return false;
    }
    let centers = inline_choice_centers(
        &row.choices,
        asset_manager,
        inline_choice_left_x_for_row(state, row_idx),
    );
    if centers.is_empty() {
        return false;
    }
    if row_allows_arcade_next_row(state, row_idx) {
        if state.pane().arcade_row_focus[idx] {
            if delta <= 0 {
                return false;
            }
            state.pane_mut().arcade_row_focus[idx] = false;
            state.pane_mut().inline_choice_x[idx] = centers[0];
            return true;
        }
        let Some(current_idx) = focused_inline_choice_index(state, asset_manager, idx, row_idx)
        else {
            return false;
        };
        if delta < 0 {
            if current_idx == 0 {
                state.pane_mut().arcade_row_focus[idx] = true;
                state.pane_mut().inline_choice_x[idx] = f32::NAN;
                return true;
            }
            state.pane_mut().inline_choice_x[idx] = centers[current_idx - 1];
            return true;
        }
        if current_idx + 1 >= centers.len() {
            return false;
        }
        state.pane_mut().inline_choice_x[idx] = centers[current_idx + 1];
        return true;
    }
    let Some(current_idx) = focused_inline_choice_index(state, asset_manager, idx, row_idx) else {
        return false;
    };
    let n = centers.len() as isize;
    let next_idx = ((current_idx as isize + delta).rem_euclid(n)) as usize;
    state.pane_mut().inline_choice_x[idx] = centers[next_idx];
    true
}

pub(super) fn commit_inline_focus_selection(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    row_idx: usize,
) -> bool {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_idx)
        .and_then(|&id| state.pane().row_map.get(id))
    else {
        return false;
    };
    if !row_supports_inline_nav(row) {
        return false;
    }
    let Some(focus_idx) = focused_inline_choice_index(state, asset_manager, idx, row_idx) else {
        return false;
    };
    let is_shared = row_is_shared(row.id);
    if let Some(&row_id) = state.pane().row_map.display_order().get(row_idx) {
        if let Some(row) = state.pane_mut().row_map.get_mut(row_id) {
            if is_shared {
                let changed = row.selected_choice_index.iter().any(|&v| v != focus_idx);
                row.selected_choice_index = [focus_idx; PLAYER_SLOTS];
                return changed;
            }
            let changed = row.selected_choice_index[idx] != focus_idx;
            row.selected_choice_index[idx] = focus_idx;
            return changed;
        }
    }
    false
}

pub(super) fn sync_inline_intent_from_row(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    row_idx: usize,
) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    if row_allows_arcade_next_row(state, row_idx) && state.pane().arcade_row_focus[idx] {
        state.pane_mut().inline_choice_x[idx] = f32::NAN;
        return;
    }
    let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_idx)
        .and_then(|&id| state.pane().row_map.get(id))
    else {
        return;
    };
    if !row_supports_inline_nav(row) {
        return;
    }
    let centers = inline_choice_centers(
        &row.choices,
        asset_manager,
        inline_choice_left_x_for_row(state, row_idx),
    );
    if centers.is_empty() {
        return;
    }
    let sel = row.selected_choice_index[idx].min(centers.len().saturating_sub(1));
    state.pane_mut().inline_choice_x[idx] = centers[sel];
}

pub(super) fn apply_inline_intent_to_row(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    row_idx: usize,
) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    if row_allows_arcade_next_row(state, row_idx) && state.pane().arcade_row_focus[idx] {
        state.pane_mut().inline_choice_x[idx] = f32::NAN;
        return;
    }
    let Some(row) = state
        .pane()
        .row_map
        .display_order()
        .get(row_idx)
        .and_then(|&id| state.pane().row_map.get(id))
    else {
        return;
    };
    if !row_supports_inline_nav(row) {
        return;
    }
    let centers = inline_choice_centers(
        &row.choices,
        asset_manager,
        inline_choice_left_x_for_row(state, row_idx),
    );
    if centers.is_empty() {
        return;
    }
    let sel = row.selected_choice_index[idx].min(centers.len().saturating_sub(1));
    if state.current_pane == OptionsPane::Main {
        state.pane_mut().inline_choice_x[idx] = centers[sel];
        return;
    }
    if !state.pane().inline_choice_x[idx].is_finite() {
        state.pane_mut().inline_choice_x[idx] = centers[sel];
    }
}

pub(super) fn move_selection_vertical(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
    dir: NavDirection,
) {
    if !matches!(dir, NavDirection::Up | NavDirection::Down) || state.pane().row_map.is_empty() {
        return;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    sync_selected_rows_with_visibility(state, active);
    let visibility = row_visibility(
        &state.pane().row_map,
        active,
        state.hide_active_mask,
        state.error_bar_active_mask,
        state.allow_per_player_global_offsets,
    );
    let current_row =
        state.pane().selected_row[idx].min(state.pane().row_map.len().saturating_sub(1));
    if !state.pane().inline_choice_x[idx].is_finite() {
        if let Some((anchor_x, _, _, _)) = cursor_dest_for_player(state, asset_manager, idx) {
            state.pane_mut().inline_choice_x[idx] = anchor_x;
        } else {
            sync_inline_intent_from_row(state, asset_manager, idx, current_row);
        }
    }
    if let Some(next_row) = next_visible_row(&state.pane().row_map, current_row, dir, visibility) {
        state.pane_mut().selected_row[idx] = next_row;
        state.pane_mut().arcade_row_focus[idx] = row_allows_arcade_next_row(state, next_row);
        apply_inline_intent_to_row(state, asset_manager, idx, next_row);
    }
}

#[inline(always)]
pub(super) fn measure_option_text(
    asset_manager: &AssetManager,
    text: &str,
    zoom: f32,
) -> (f32, f32) {
    let mut out_w = 40.0_f32;
    let mut out_h = 16.0_f32;
    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("miso", |metrics_font| {
            out_h = (metrics_font.height as f32).max(1.0) * zoom;
            let mut w = crate::engine::present::font::measure_line_width_logical(
                metrics_font,
                text,
                all_fonts,
            ) as f32;
            if !w.is_finite() || w <= 0.0 {
                w = 1.0;
            }
            out_w = w * zoom;
        });
    });
    (out_w, out_h)
}

#[inline(always)]
pub(super) fn inline_choice_left_x() -> f32 {
    widescale(162.0, 176.0)
}

#[inline(always)]
pub(super) fn arcade_inline_choice_shift_x() -> f32 {
    widescale(6.0, 8.0)
}

#[inline(always)]
pub(super) fn arcade_next_row_gap_x() -> f32 {
    widescale(5.0, 6.0)
}

#[inline(always)]
pub(super) fn inline_choice_left_x_for_row(state: &State, row_idx: usize) -> f32 {
    inline_choice_left_x()
        + if row_allows_arcade_next_row(state, row_idx) {
            arcade_inline_choice_shift_x()
        } else {
            0.0
        }
}

#[inline(always)]
pub(super) fn arcade_next_row_visible(state: &State, row_idx: usize) -> bool {
    row_allows_arcade_next_row(state, row_idx)
}

#[inline(always)]
pub(super) fn arcade_row_focuses_next_row(
    state: &State,
    player_idx: usize,
    row_idx: usize,
) -> bool {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    row_allows_arcade_next_row(state, row_idx)
        && state.pane().arcade_row_focus[idx]
        && state.pane().selected_row[idx] == row_idx
}

pub(super) fn arcade_next_row_layout(
    state: &State,
    row_idx: usize,
    asset_manager: &AssetManager,
    zoom: f32,
) -> (f32, f32, f32) {
    let (draw_w, draw_h) = measure_option_text(asset_manager, ARCADE_NEXT_ROW_TEXT, zoom);
    let left_x = inline_choice_left_x_for_row(state, row_idx) - draw_w - arcade_next_row_gap_x();
    (left_x, draw_w, draw_h)
}
