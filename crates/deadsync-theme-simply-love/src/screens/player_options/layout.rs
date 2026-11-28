use deadlib_assets::AssetView;
use super::*;
use deadsync_theme::player_options::PlayerOptionsViewport;

pub(super) const DEFAULT_VIEWPORT: PlayerOptionsViewport = PlayerOptionsViewport {
    slots: VISIBLE_ROWS as u8,
    separate_exit: false,
    row_tween_seconds: ROW_TWEEN_SECONDS,
};

/// Configure the shared editor for the mounted Player Options layout.
pub fn set_viewport(state: &mut State, viewport: PlayerOptionsViewport) {
    state.viewport = PlayerOptionsViewport {
        slots: viewport.slots.max(4),
        ..viewport
    };
    reset_viewport_rows(state);
}

pub(super) const fn row_layout_slots(id: RowId) -> usize {
    if matches!(id, RowId::GameplayExtras) {
        2
    } else {
        1
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RowWindow {
    pub(super) first_start: i32,
    pub(super) first_end: i32,
    pub(super) second_start: i32,
    pub(super) second_end: i32,
}

#[inline(always)]
pub(super) fn compute_row_window(
    total_rows: usize,
    selected_row: [usize; PLAYER_SLOTS],
    active: [bool; PLAYER_SLOTS],
    visible_slots: usize,
) -> RowWindow {
    if total_rows == 0 {
        return RowWindow {
            first_start: 0,
            first_end: 0,
            second_start: 0,
            second_end: 0,
        };
    }

    let total_rows_i = total_rows as i32;
    if total_rows <= visible_slots {
        return RowWindow {
            first_start: 0,
            first_end: total_rows_i,
            second_start: total_rows_i,
            second_end: total_rows_i,
        };
    }

    let total = visible_slots as i32;
    let halfsize = total / 2;

    // Mirror ITGmania ScreenOptions::PositionRows() semantics (signed math matters).
    let p1_choice = if active[P1] {
        selected_row[P1] as i32
    } else {
        selected_row[P2] as i32
    };
    let p2_choice = if active[P2] {
        selected_row[P2] as i32
    } else {
        selected_row[P1] as i32
    };
    let p1_choice = p1_choice.clamp(0, total_rows_i - 1);
    let p2_choice = p2_choice.clamp(0, total_rows_i - 1);

    let (mut first_start, mut first_end, mut second_start, mut second_end) =
        if active[P1] && active[P2] {
            let earliest = p1_choice.min(p2_choice);
            let first_start = (earliest - halfsize / 2).max(0);
            let first_end = first_start + halfsize;

            let latest = p1_choice.max(p2_choice);
            let second_start = (latest - halfsize / 2).max(0).max(first_end);
            let second_end = second_start + halfsize;
            (first_start, first_end, second_start, second_end)
        } else {
            let first_start = (p1_choice - halfsize).max(0);
            let first_end = first_start + total;
            (first_start, first_end, first_end, first_end)
        };

    first_end = first_end.min(total_rows_i);
    second_end = second_end.min(total_rows_i);

    loop {
        let sum = (first_end - first_start) + (second_end - second_start);
        if sum >= total_rows_i || sum >= total {
            break;
        }
        if second_start > first_end {
            second_start -= 1;
        } else if first_start > 0 {
            first_start -= 1;
        } else if second_end < total_rows_i {
            second_end += 1;
        } else {
            break;
        }
    }

    RowWindow {
        first_start,
        first_end,
        second_start,
        second_end,
    }
}

#[inline(always)]
pub(super) fn row_layout_params() -> (f32, f32) {
    // Must match the geometry in get_actors(): rows align to the help box.
    let frame_h = ROW_HEIGHT;
    let first_row_center_y = screen_center_y() + ROW_START_OFFSET;
    let help_box_h = 40.0_f32;
    let help_box_bottom_y = screen_height() - 36.0;
    let help_top_y = help_box_bottom_y - help_box_h;
    let n_rows_f = VISIBLE_ROWS as f32;
    let mut row_gap = if n_rows_f > 0.0 {
        (n_rows_f - 0.5).mul_add(-frame_h, help_top_y - first_row_center_y) / n_rows_f
    } else {
        0.0
    };
    if !row_gap.is_finite() || row_gap < 0.0 {
        row_gap = 0.0;
    }
    (first_row_center_y, frame_h + row_gap)
}

/// Row frames share the help box's left edge and width (underlay.lua).
#[inline(always)]
pub(super) fn row_frame_left() -> f32 {
    widescale(13.0, 30.666)
}

#[inline(always)]
pub(super) fn row_frame_width() -> f32 {
    widescale(614.0, 792.0)
}

#[inline(always)]
pub(super) fn player_option_column_x(player_idx: usize) -> f32 {
    if player_idx == P2 {
        screen_center_x() + widescale(140.0, 154.0)
    } else {
        screen_center_x() + widescale(-77.0, -100.0)
    }
}

pub(super) fn init_row_tweens(
    row_map: &RowMap,
    selected_row: [usize; PLAYER_SLOTS],
    active: [bool; PLAYER_SLOTS],
    option_masks: [PlayerOptionMasks; PLAYER_SLOTS],
    policy: PlayerOptionsPolicyView,
    viewport: PlayerOptionsViewport,
) -> Vec<RowTween> {
    let visibility = row_visibility(row_map, active, option_masks, policy);
    row_targets(row_map, selected_row, active, visibility, viewport)
        .map(|(y, alpha)| RowTween {
            from_y: y,
            to_y: y,
            from_a: alpha,
            to_a: alpha,
            from_scroll: 0.0,
            to_scroll: 0.0,
            t: 1.0,
        })
        .collect()
}

pub(super) fn reset_viewport_rows(state: &mut State) {
    let (active, masks, policy, viewport) = (
        state.active,
        state.option_masks,
        state.policy,
        state.viewport,
    );
    let pane = state.pane_mut();
    pane.row_tweens = init_row_tweens(
        &pane.row_map,
        pane.selected_row,
        active,
        masks,
        policy,
        viewport,
    );
    pane.layout_key = None;
    if viewport.separate_exit {
        let visibility = row_visibility(&pane.row_map, active, masks, policy);
        pane.scroll_position = row_scroll_position(&pane.row_map, pane.selected_row, visibility);
    }
}

pub(super) fn retarget_row_tweens(
    row_map: &RowMap,
    row_tweens: &mut [RowTween],
    selected_row: [usize; PLAYER_SLOTS],
    active: [bool; PLAYER_SLOTS],
    visibility: RowVisibility,
    viewport: PlayerOptionsViewport,
) {
    for (tween, (y, alpha)) in row_tweens.iter_mut().zip(row_targets(
        row_map,
        selected_row,
        active,
        visibility,
        viewport,
    )) {
        retarget_row_tween(tween, y, alpha);
    }
}

fn row_targets(
    row_map: &RowMap,
    selected_row: [usize; PLAYER_SLOTS],
    active: [bool; PLAYER_SLOTS],
    visibility: RowVisibility,
    viewport: PlayerOptionsViewport,
) -> impl Iterator<Item = (f32, f32)> + '_ {
    let (first_y, row_step) = row_layout_params();
    let rows = row_map.len();
    let exit = viewport.separate_exit && row_map.display_order().last() == Some(&RowId::Exit);
    let body_slots = count_visible_row_slots(row_map, visibility).saturating_sub(usize::from(exit));
    let selected_visible = std::array::from_fn(|player| {
        let index = selected_row[player].min(rows.saturating_sub(1));
        row_to_visible_index(row_map, index, visibility)
            .unwrap_or(0)
            .min(body_slots.saturating_sub(1))
    });
    let window = compute_row_window(
        body_slots,
        selected_visible,
        active,
        usize::from(viewport.slots),
    );
    let slots = f32::from(viewport.slots);
    let mid = slots.mul_add(0.5, -0.5);
    let bottom = slots - 0.5;
    let mut visible_index = 0;
    (0..rows).map(move |index| {
        let (slot, hidden) = if exit && index + 1 == rows {
            (slots, window.second_end as usize != body_slots)
        } else if body_slots == 0 {
            (-0.5, true)
        } else {
            row_f_pos_for_index(
                row_map,
                index,
                visibility,
                &mut visible_index,
                window,
                mid,
                bottom,
            )
        };
        (
            row_step.mul_add(slot, first_y),
            if hidden { 0.0 } else { 1.0 },
        )
    })
}

pub(super) fn row_scroll_position(
    row_map: &RowMap,
    selected_row: [usize; PLAYER_SLOTS],
    visibility: RowVisibility,
) -> [f32; PLAYER_SLOTS] {
    let mut positions = [0; PLAYER_SLOTS];
    let mut count = 0usize;
    for index in 0..row_map.len() {
        if !is_row_visible(row_map, index, visibility) {
            continue;
        }
        for player in 0..PLAYER_SLOTS {
            if selected_row[player] == index {
                positions[player] = count;
            }
        }
        count += 1;
    }
    let last = count.saturating_sub(1).max(1) as f32;
    positions.map(|position| position as f32 / last)
}

#[inline(always)]
fn retarget_row_tween(tween: &mut RowTween, y: f32, alpha: f32) {
    if (y - tween.to_y).abs() <= 0.01 && alpha == tween.to_a {
        return;
    }
    tween.restart_from_current();
    tween.to_y = y;
    tween.to_a = alpha;
}

#[inline(always)]
pub(super) const fn f_pos_for_visible_idx(
    visible_idx: i32,
    window: RowWindow,
    mid_pos: f32,
    bottom_pos: f32,
) -> (f32, bool) {
    let hidden_above = visible_idx < window.first_start;
    let hidden_mid = visible_idx >= window.first_end && visible_idx < window.second_start;
    let hidden_below = visible_idx >= window.second_end;
    if hidden_above {
        return (-0.5, true);
    }
    if hidden_mid {
        return (mid_pos, true);
    }
    if hidden_below {
        return (bottom_pos, true);
    }

    let shown_pos = if visible_idx < window.first_end {
        visible_idx - window.first_start
    } else {
        (window.first_end - window.first_start) + (visible_idx - window.second_start)
    };
    (shown_pos as f32, false)
}

pub(super) fn row_f_pos_for_index(
    row_map: &RowMap,
    row_idx: usize,
    visibility: RowVisibility,
    visible_idx: &mut i32,
    window: RowWindow,
    mid_pos: f32,
    bottom_pos: f32,
) -> (f32, bool) {
    if is_row_visible(row_map, row_idx, visibility) {
        let idx = *visible_idx;
        let slots = row_layout_slots(row_map.id_at(row_idx)) as i32;
        *visible_idx += slots;
        let (first, first_hidden) = f_pos_for_visible_idx(idx, window, mid_pos, bottom_pos);
        let (last, last_hidden) =
            f_pos_for_visible_idx(idx + slots - 1, window, mid_pos, bottom_pos);
        // A wrapped row occupies complete slots; never show half at a window edge.
        return ((first + last) * 0.5, first_hidden || last_hidden);
    }

    if let Some(slot) = hidden_row_anchor_visible_index(row_map, row_idx, visibility) {
        let parent_id = conditional_row_parent(row_map.id_at(row_idx)).unwrap();
        let (first, _) = f_pos_for_visible_idx(slot, window, mid_pos, bottom_pos);
        let (last, _) = f_pos_for_visible_idx(
            slot + row_layout_slots(parent_id) as i32 - 1,
            window,
            mid_pos,
            bottom_pos,
        );
        return ((first + last) * 0.5, true);
    }

    (-0.5, true)
}

pub(super) fn cursor_dest_for_player(
    state: &State,
    asset_manager: &dyn AssetView,
    player_idx: usize,
) -> Option<(f32, f32, f32, f32)> {
    let visibility = row_visibility(
        &state.pane().row_map,
        state.active,
        state.option_masks,
        state.policy,
    );
    cursor_dest_for_player_with_visibility(state, asset_manager, player_idx, visibility)
}

pub(super) fn cursor_dest_for_player_with_visibility(
    state: &State,
    asset_manager: &dyn AssetView,
    player_idx: usize,
    visibility: RowVisibility,
) -> Option<(f32, f32, f32, f32)> {
    if state.pane().row_map.is_empty() {
        return None;
    }
    let player_idx = player_idx.min(PLAYER_SLOTS - 1);
    let active = state.active;
    let mut row_idx =
        state.pane().selected_row[player_idx].min(state.pane().row_map.len().saturating_sub(1));
    if !is_row_visible(&state.pane().row_map, row_idx, visibility) {
        row_idx = fallback_visible_row(&state.pane().row_map, row_idx, visibility)?;
    }
    let row = state
        .pane()
        .row_map
        .display_order()
        .get(row_idx)
        .and_then(|&id| state.pane().row_map.get(id))?;
    debug_assert!(state.pane().choice_layout_ready);
    debug_assert_eq!(row.choice_widths.len(), row.choices.len());
    debug_assert!(row.choice_height > 0.0);

    let y = state
        .pane()
        .row_tweens
        .get(row_idx)
        .map(|tw| tw.to_y)
        .unwrap_or_else(|| {
            // Fallback (no windowing) if row tweens aren't initialized yet.
            let (y0, step) = row_layout_params();
            (row_idx as f32).mul_add(step, y0)
        });

    let border_w = widescale(2.0, 2.5);
    let pad_y = widescale(6.0, 8.0);
    let min_pad_x = widescale(2.0, 3.0);
    let max_pad_x = widescale(22.0, 28.0);
    let width_ref = widescale(180.0, 220.0);

    // Shared geometry for Music Rate centering (must match get_actors()).
    let row_left = row_frame_left();
    let row_width = row_frame_width();
    let item_col_left = row_left + TITLE_BG_WIDTH;
    let item_col_w = row_width - TITLE_BG_WIDTH;
    let music_rate_center_x = item_col_left + item_col_w * 0.5;

    if row.id == RowId::Exit {
        // Exit row is shared (OptionRowExit); its cursor is centered on Speed Mod helper X.
        let choice_idx = row.selected_choice_index[P1].min(row.choices.len().saturating_sub(1));
        let draw_w = *row.choice_widths.get(choice_idx)?;
        let draw_h = row.choice_height;
        let mut size_t = draw_w / width_ref;
        if !size_t.is_finite() {
            size_t = 0.0;
        }
        size_t = size_t.clamp(0.0, 1.0);
        let mut pad_x = (max_pad_x - min_pad_x).mul_add(size_t, min_pad_x);
        let max_pad_by_spacing = (INLINE_SPACING - border_w).max(min_pad_x);
        if pad_x > max_pad_by_spacing {
            pad_x = max_pad_by_spacing;
        }
        let ring_w = draw_w + pad_x * 2.0;
        let ring_h = draw_h + pad_y * 2.0;
        let center_x = if active[P2] && !active[P1] {
            player_option_column_x(P2)
        } else {
            player_option_column_x(P1)
        };
        return Some((center_x, y, ring_w, ring_h));
    }

    if row_shows_all_choices_inline(row.id) {
        if row.choices.is_empty() {
            return None;
        }
        let spacing = INLINE_SPACING;
        let choice_inner_left = inline_choice_left_x_for_row(state, row_idx);
        let choice_scale = inline_choice_scale(row);
        let text_h = row.choice_height * choice_scale;
        if arcade_row_focuses_next_row(state, player_idx, row_idx) {
            let (left_x, draw_w, draw_h) = arcade_next_row_layout(state, row_idx, asset_manager);
            let mut size_t = draw_w / width_ref;
            if !size_t.is_finite() {
                size_t = 0.0;
            }
            size_t = size_t.clamp(0.0, 1.0);
            let mut pad_x = (max_pad_x - min_pad_x).mul_add(size_t, min_pad_x);
            let max_pad_by_spacing = (spacing - border_w).max(min_pad_x);
            if pad_x > max_pad_by_spacing {
                pad_x = max_pad_by_spacing;
            }
            let ring_w = draw_w + pad_x * 2.0;
            let ring_h = draw_h + pad_y * 2.0;
            return Some((draw_w.mul_add(0.5, left_x), y, ring_w, ring_h));
        }

        let focus_idx = focused_inline_choice_index(state, player_idx, row_idx)
            .unwrap_or_else(|| row.selected_choice_index[player_idx])
            .min(row.choices.len().saturating_sub(1));
        let draw_w = *row.choice_widths.get(focus_idx)? * choice_scale;
        let [offset_x, offset_y] = inline_choice_position(row, focus_idx);
        let center_x = choice_inner_left - inline_row_scroll_target(state, row_idx)
            + offset_x
            + draw_w * 0.5;

        let mut size_t = draw_w / width_ref;
        if !size_t.is_finite() {
            size_t = 0.0;
        }
        size_t = size_t.clamp(0.0, 1.0);
        let mut pad_x = (max_pad_x - min_pad_x).mul_add(size_t, min_pad_x);
        let max_pad_by_spacing = (spacing * choice_scale - border_w).max(min_pad_x);
        if pad_x > max_pad_by_spacing {
            pad_x = max_pad_by_spacing;
        }
        let ring_w = draw_w + pad_x * 2.0;
        let ring_h = text_h + pad_y * 2.0;
        return Some((center_x, y + offset_y, ring_w, ring_h));
    }

    // Single value rows (ShowOneInRow).
    let mut center_x = player_option_column_x(player_idx);
    if row.id == RowId::MusicRate {
        center_x = music_rate_center_x;
    }

    let selected_idx =
        row.selected_choice_index[player_idx].min(row.choices.len().saturating_sub(1));
    let (draw_w, draw_h) = if arcade_row_focuses_next_row(state, player_idx, row_idx) {
        let [width, height] = arcade_next_row_size(state, asset_manager);
        (width, height)
    } else if row.id == RowId::SpeedMod {
        let value = speed_value(state, player_idx);
        (value.value_draw_width, value.value_draw_height)
    } else {
        let idx = if row.id == RowId::TypeOfSpeedMod {
            state.speed_mod[player_idx].mod_type.choice_index()
        } else {
            selected_idx
        }
        .min(row.choices.len().saturating_sub(1));
        (*row.choice_widths.get(idx)?, row.choice_height)
    };
    let mut size_t = draw_w / width_ref;
    if !size_t.is_finite() {
        size_t = 0.0;
    }
    size_t = size_t.clamp(0.0, 1.0);
    let mut pad_x = (max_pad_x - min_pad_x).mul_add(size_t, min_pad_x);
    let max_pad_by_spacing = (INLINE_SPACING - border_w).max(min_pad_x);
    if pad_x > max_pad_by_spacing {
        pad_x = max_pad_by_spacing;
    }
    let ring_w = draw_w + pad_x * 2.0;
    let ring_h = draw_h + pad_y * 2.0;
    Some((center_x, y, ring_w, ring_h))
}
