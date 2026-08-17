use super::*;

pub(super) fn inline_choice_geometry(
    row: &Row,
    left_x: f32,
    choice_idx: usize,
) -> Option<[f32; 2]> {
    debug_assert_eq!(row.choice_offsets.len(), row.choices.len());
    debug_assert_eq!(row.choice_widths.len(), row.choices.len());
    let width = *row.choice_widths.get(choice_idx)?;
    let offset = *row.choice_offsets.get(choice_idx)?;
    Some([width.mul_add(0.5, left_x + offset), width])
}

pub(super) fn prepare_choice_layouts(state: &mut State, asset_manager: &AssetManager) {
    if state.pane().choice_layout_ready {
        return;
    }
    arcade_next_row_size(state, asset_manager);
    let prepared = asset_manager.with_fonts(|all_fonts| {
        asset_manager
            .with_font("miso", |metrics_font| {
                let text_h = (metrics_font.height as f32).max(1.0) * INLINE_CHOICE_VALUE_ZOOM;
                for row in state.pane_mut().row_map.rows.iter_mut().flatten() {
                    if row.choice_widths.len() == row.choices.len() && row.choice_height > 0.0 {
                        continue;
                    }
                    let mut widths = Vec::with_capacity(row.choices.len());
                    for text in &row.choices {
                        let mut width = deadlib_present::font::measure_line_width_logical(
                            metrics_font,
                            text,
                            all_fonts,
                        ) as f32;
                        if !width.is_finite() || width <= 0.0 {
                            width = 1.0;
                        }
                        widths.push(width * INLINE_CHOICE_VALUE_ZOOM);
                    }
                    row.choice_offsets = if row_shows_all_choices_inline(row.id) {
                        let mut x = 0.0;
                        widths
                            .iter()
                            .map(|width| {
                                let offset = x;
                                x += *width + INLINE_CHOICE_SPACING;
                                offset
                            })
                            .collect::<Vec<_>>()
                            .into_boxed_slice()
                    } else {
                        Box::new([])
                    };
                    row.choice_widths = widths.into_boxed_slice();
                    row.choice_height = text_h;
                }
            })
            .is_some()
    });
    state.pane_mut().choice_layout_ready = prepared;
}

fn inline_row(state: &State, row_idx: usize) -> Option<&Row> {
    let row = state.pane().row_map.get_at(row_idx)?;
    row_supports_inline_nav(row).then_some(row)
}

pub(super) fn focused_inline_choice_index(
    state: &State,
    player_idx: usize,
    row_idx: usize,
) -> Option<usize> {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row = inline_row(state, row_idx)?;
    let mut focus_idx = row.selected_choice_index[idx].min(row.choices.len().saturating_sub(1));
    let anchor_x = state.pane().inline_choice_x[idx];
    if anchor_x.is_finite() {
        debug_assert!(state.pane().choice_layout_ready);
        debug_assert_eq!(row.choice_offsets.len(), row.choices.len());
        debug_assert_eq!(row.choice_widths.len(), row.choices.len());
        let mut best_dist = f32::INFINITY;
        let left_x = inline_choice_left_x_for_row(state, row_idx);
        for (i, (&offset, &width)) in row
            .choice_offsets
            .iter()
            .zip(row.choice_widths.iter())
            .enumerate()
        {
            let dist = (width.mul_add(0.5, left_x + offset) - anchor_x).abs();
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
    player_idx: usize,
    delta: isize,
    wrap: NavWrap,
) -> bool {
    if state.pane().row_map.is_empty() || delta == 0 {
        return false;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_idx = state.pane().selected_row[idx].min(state.pane().row_map.len().saturating_sub(1));
    let Some(row) = inline_row(state, row_idx) else {
        return false;
    };
    let choice_count = row.choices.len();
    if choice_count == 0 {
        return false;
    }
    let left_x = inline_choice_left_x_for_row(state, row_idx);
    if row_allows_arcade_next_row(state, row_idx) {
        if state.pane().arcade_row_focus[idx] {
            if delta <= 0 {
                return false;
            }
            let Some([target, _]) = inline_choice_geometry(row, left_x, 0) else {
                return false;
            };
            state.pane_mut().arcade_row_focus[idx] = false;
            state.pane_mut().inline_choice_x[idx] = target;
            return true;
        }
        let Some(current_idx) = focused_inline_choice_index(state, idx, row_idx) else {
            return false;
        };
        if delta < 0 {
            if current_idx == 0 {
                state.pane_mut().arcade_row_focus[idx] = true;
                state.pane_mut().inline_choice_x[idx] = f32::NAN;
                return true;
            }
            let Some([target, _]) = inline_choice_geometry(row, left_x, current_idx - 1) else {
                return false;
            };
            state.pane_mut().inline_choice_x[idx] = target;
            return true;
        }
        if current_idx + 1 >= choice_count {
            return false;
        }
        let Some([target, _]) = inline_choice_geometry(row, left_x, current_idx + 1) else {
            return false;
        };
        state.pane_mut().inline_choice_x[idx] = target;
        return true;
    }
    let Some(current_idx) = focused_inline_choice_index(state, idx, row_idx) else {
        return false;
    };
    let n = choice_count as isize;
    let raw = current_idx as isize + delta;
    let next_idx = match wrap {
        NavWrap::Wrap => raw.rem_euclid(n) as usize,
        NavWrap::Clamp => raw.clamp(0, n - 1) as usize,
    };
    if next_idx == current_idx {
        return false;
    }
    let Some([target, _]) = inline_choice_geometry(row, left_x, next_idx) else {
        return false;
    };
    state.pane_mut().inline_choice_x[idx] = target;
    true
}

pub(super) fn commit_inline_focus_selection(
    state: &mut State,
    player_idx: usize,
    row_idx: usize,
) -> bool {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let Some(row) = inline_row(state, row_idx) else {
        return false;
    };
    if row.choices.is_empty() {
        return false;
    }
    let Some(focus_idx) = focused_inline_choice_index(state, idx, row_idx) else {
        return false;
    };
    let is_shared = row.mirror_across_players;
    if let Some(&row_id) = state.pane().row_map.display_order().get(row_idx)
        && let Some(row) = state.pane_mut().row_map.get_mut(row_id)
    {
        if is_shared {
            let changed = row.selected_choice_index.iter().any(|&v| v != focus_idx);
            row.selected_choice_index = [focus_idx; PLAYER_SLOTS];
            return changed;
        }
        let changed = row.selected_choice_index[idx] != focus_idx;
        row.selected_choice_index[idx] = focus_idx;
        return changed;
    }
    false
}

/// Anchor `inline_choice_x` to the current row's selected choice.
///
/// `force` distinguishes the two flavors:
/// - `force = true` (`sync_inline_intent_from_row`): always overwrite the
///   anchor — used when the caller knows it owns the intent (e.g. after a
///   profile-driven selection change).
/// - `force = false` (`apply_inline_intent_to_row`): preserve any existing
///   finite anchor on non-Main panes (so horizontal intent carries between
///   rows), but always reset on the Main pane.
fn write_inline_intent(state: &mut State, player_idx: usize, row_idx: usize, force: bool) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    if row_allows_arcade_next_row(state, row_idx) && state.pane().arcade_row_focus[idx] {
        state.pane_mut().inline_choice_x[idx] = f32::NAN;
        return;
    }
    let Some(row) = inline_row(state, row_idx) else {
        return;
    };
    if row.choices.is_empty() {
        return;
    }
    let sel = row.selected_choice_index[idx].min(row.choices.len() - 1);
    let Some([target, _]) =
        inline_choice_geometry(row, inline_choice_left_x_for_row(state, row_idx), sel)
    else {
        return;
    };
    if force
        || state.current_pane == OptionsPane::Main
        || !state.pane().inline_choice_x[idx].is_finite()
    {
        state.pane_mut().inline_choice_x[idx] = target;
    }
}

pub(super) fn sync_inline_intent_from_row(state: &mut State, player_idx: usize, row_idx: usize) {
    write_inline_intent(state, player_idx, row_idx, true);
}

pub(super) fn apply_inline_intent_to_row(state: &mut State, player_idx: usize, row_idx: usize) {
    write_inline_intent(state, player_idx, row_idx, false);
}

pub(super) fn move_selection_vertical(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
    dir: NavDirection,
    wrap: NavWrap,
) {
    if !matches!(dir, NavDirection::Up | NavDirection::Down) || state.pane().row_map.is_empty() {
        return;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    sync_selected_rows_with_visibility(state, active);
    let visibility = row_visibility(
        &state.pane().row_map,
        active,
        state.option_masks,
        state.policy,
    );
    let current_row =
        state.pane().selected_row[idx].min(state.pane().row_map.len().saturating_sub(1));
    if !state.pane().inline_choice_x[idx].is_finite() {
        if let Some((anchor_x, _, _, _)) = cursor_dest_for_player(state, asset_manager, idx) {
            state.pane_mut().inline_choice_x[idx] = anchor_x;
        } else {
            sync_inline_intent_from_row(state, idx, current_row);
        }
    }
    if let Some(next_row) =
        next_visible_row(&state.pane().row_map, current_row, dir, visibility, wrap)
    {
        state.pane_mut().selected_row[idx] = next_row;
        state.pane_mut().arcade_row_focus[idx] = row_allows_arcade_next_row(state, next_row);
        apply_inline_intent_to_row(state, idx, next_row);
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
            let mut w =
                deadlib_present::font::measure_line_width_logical(metrics_font, text, all_fonts)
                    as f32;
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

pub(super) fn arcade_next_row_size(state: &State, asset_manager: &AssetManager) -> [f32; 2] {
    if let Some(size) = state.arcade_next_row_size.get() {
        return size;
    }
    let (width, height) = measure_option_text(
        asset_manager,
        ARCADE_NEXT_ROW_TEXT,
        INLINE_CHOICE_VALUE_ZOOM,
    );
    let size = [width, height];
    state.arcade_next_row_size.set(Some(size));
    size
}

pub(super) fn arcade_next_row_layout(
    state: &State,
    row_idx: usize,
    asset_manager: &AssetManager,
) -> (f32, f32, f32) {
    let [draw_w, draw_h] = arcade_next_row_size(state, asset_manager);
    let left_x = inline_choice_left_x_for_row(state, row_idx) - draw_w - arcade_next_row_gap_x();
    (left_x, draw_w, draw_h)
}
