use super::*;

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
    if total_rows <= VISIBLE_ROWS {
        return RowWindow {
            first_start: 0,
            first_end: total_rows_i,
            second_start: total_rows_i,
            second_end: total_rows_i,
        };
    }

    let total = VISIBLE_ROWS as i32;
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

#[inline(always)]
pub(super) fn player_option_column_x(player_idx: usize) -> f32 {
    if player_idx == P2 {
        screen_center_x() + widescale(140.0, 154.0)
    } else {
        screen_center_x() + widescale(-77.0, -100.0)
    }
}

#[inline(always)]
pub(super) fn init_row_tweens(
    row_map: &RowMap,
    selected_row: [usize; PLAYER_SLOTS],
    active: [bool; PLAYER_SLOTS],
    option_masks: [PlayerOptionMasks; PLAYER_SLOTS],
    allow_per_player_global_offsets: bool,
) -> Vec<RowTween> {
    let total_rows = row_map.display_order().len();
    if total_rows == 0 {
        return Vec::new();
    }

    let (first_row_center_y, row_step) = row_layout_params();
    let visibility = row_visibility(
        row_map,
        active,
        option_masks,
        allow_per_player_global_offsets,
    );
    let visible_rows = count_visible_rows(row_map, visibility);
    if visible_rows == 0 {
        let y = first_row_center_y - row_step * 0.5;
        return (0..total_rows)
            .map(|_| RowTween {
                from_y: y,
                to_y: y,
                from_a: 0.0,
                to_a: 0.0,
                t: 1.0,
            })
            .collect();
    }

    let selected_visible = std::array::from_fn(|player_idx| {
        let idx = selected_row[player_idx].min(total_rows.saturating_sub(1));
        row_to_visible_index(row_map, idx, visibility).unwrap_or(0)
    });
    let w = compute_row_window(visible_rows, selected_visible, active);
    let mid_pos = (VISIBLE_ROWS as f32) * 0.5 - 0.5;
    let bottom_pos = (VISIBLE_ROWS as f32) - 0.5;
    let measure_counter_anchor_visible_idx =
        parent_anchor_visible_index(row_map, RowId::MeasureCounter, visibility);
    let judgment_font_anchor_visible_idx =
        parent_anchor_visible_index(row_map, RowId::JudgmentFont, visibility);
    let judgment_tilt_anchor_visible_idx =
        parent_anchor_visible_index(row_map, RowId::JudgmentTilt, visibility);
    let combo_font_anchor_visible_idx =
        parent_anchor_visible_index(row_map, RowId::ComboFont, visibility);
    let error_bar_anchor_visible_idx =
        parent_anchor_visible_index(row_map, RowId::ErrorBar, visibility);
    let hide_anchor_visible_idx = parent_anchor_visible_index(row_map, RowId::Hide, visibility);
    let gameplay_extras_anchor_visible_idx =
        parent_anchor_visible_index(row_map, RowId::GameplayExtras, visibility);
    let fa_plus_anchor_visible_idx =
        parent_anchor_visible_index(row_map, RowId::FAPlusOptions, visibility);

    let mut out: Vec<RowTween> = Vec::with_capacity(total_rows);
    let mut visible_idx = 0i32;
    for i in 0..total_rows {
        let visible = is_row_visible(row_map, i, visibility);
        let (f_pos, hidden) = if visible {
            let ii = visible_idx;
            visible_idx += 1;
            f_pos_for_visible_idx(ii, w, mid_pos, bottom_pos)
        } else {
            let anchor = row_map
                .display_order()
                .get(i)
                .and_then(|&id| row_map.get(id))
                .and_then(|row| match conditional_row_parent(row.id) {
                    Some(RowId::MeasureCounter) => measure_counter_anchor_visible_idx,
                    Some(RowId::JudgmentFont) => judgment_font_anchor_visible_idx,
                    Some(RowId::JudgmentTilt) => judgment_tilt_anchor_visible_idx,
                    Some(RowId::ComboFont) => combo_font_anchor_visible_idx,
                    Some(RowId::ErrorBar) => error_bar_anchor_visible_idx,
                    Some(RowId::Hide) => hide_anchor_visible_idx,
                    Some(RowId::GameplayExtras) => gameplay_extras_anchor_visible_idx,
                    Some(RowId::FAPlusOptions) => fa_plus_anchor_visible_idx,
                    _ => None,
                });
            if let Some(anchor_idx) = anchor {
                let (anchor_f_pos, _) = f_pos_for_visible_idx(anchor_idx, w, mid_pos, bottom_pos);
                (anchor_f_pos, true)
            } else {
                (-0.5, true)
            }
        };

        let y = (row_step * f_pos) + first_row_center_y;
        let a = if hidden { 0.0 } else { 1.0 };
        out.push(RowTween {
            from_y: y,
            to_y: y,
            from_a: a,
            to_a: a,
            t: 1.0,
        });
    }

    out
}

#[inline(always)]
pub(super) fn f_pos_for_visible_idx(
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

pub(super) fn cursor_dest_for_player(
    state: &State,
    asset_manager: &AssetManager,
    player_idx: usize,
) -> Option<(f32, f32, f32, f32)> {
    if state.pane().row_map.is_empty() {
        return None;
    }
    let player_idx = player_idx.min(PLAYER_SLOTS - 1);
    let active = session_active_players();
    let visibility = row_visibility(
        &state.pane().row_map,
        active,
        state.option_masks,
        state.allow_per_player_global_offsets,
    );
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

    let value_zoom = 0.835_f32;
    let border_w = widescale(2.0, 2.5);
    let pad_y = widescale(6.0, 8.0);
    let min_pad_x = widescale(2.0, 3.0);
    let max_pad_x = widescale(22.0, 28.0);
    let width_ref = widescale(180.0, 220.0);

    // Shared geometry for Music Rate centering (must match get_actors()).
    let help_box_w = widescale(614.0, 792.0);
    let help_box_x = widescale(13.0, 30.666);
    let row_left = help_box_x;
    let row_width = help_box_w;
    let item_col_left = row_left + TITLE_BG_WIDTH;
    let item_col_w = row_width - TITLE_BG_WIDTH;
    let music_rate_center_x = item_col_left + item_col_w * 0.5;

    if row.id == RowId::Exit {
        // Exit row is shared (OptionRowExit); its cursor is centered on Speed Mod helper X.
        let choice_text = row
            .choices
            .get(row.selected_choice_index[P1])
            .or_else(|| row.choices.first())?;
        let (draw_w, draw_h) = measure_option_text(asset_manager, choice_text, value_zoom);
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
        let mut widths: Vec<f32> = Vec::with_capacity(row.choices.len());
        let mut text_h: f32 = 16.0;
        asset_manager.with_fonts(|all_fonts| {
            asset_manager.with_font("miso", |metrics_font| {
                text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                for text in &row.choices {
                    let mut w = crate::engine::present::font::measure_line_width_logical(
                        metrics_font,
                        text,
                        all_fonts,
                    ) as f32;
                    if !w.is_finite() || w <= 0.0 {
                        w = 1.0;
                    }
                    widths.push(w * value_zoom);
                }
            });
        });
        if widths.is_empty() {
            return None;
        }
        if arcade_row_focuses_next_row(state, player_idx, row_idx) {
            let (left_x, draw_w, draw_h) =
                arcade_next_row_layout(state, row_idx, asset_manager, value_zoom);
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

        let focus_idx = focused_inline_choice_index(state, asset_manager, player_idx, row_idx)
            .unwrap_or_else(|| row.selected_choice_index[player_idx])
            .min(widths.len().saturating_sub(1));
        let mut left_x = choice_inner_left;
        for w in widths.iter().take(focus_idx) {
            left_x += *w + spacing;
        }
        let draw_w = widths[focus_idx];
        let center_x = draw_w.mul_add(0.5, left_x);

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
        let ring_h = text_h + pad_y * 2.0;
        return Some((center_x, y, ring_w, ring_h));
    }

    // Single value rows (ShowOneInRow).
    let mut center_x = player_option_column_x(player_idx);
    if row.id == RowId::MusicRate {
        center_x = music_rate_center_x;
    }

    let display_text = if arcade_row_focuses_next_row(state, player_idx, row_idx) {
        ARCADE_NEXT_ROW_TEXT.to_string()
    } else if row.id == RowId::SpeedMod {
        state.speed_mod[player_idx].display()
    } else if row.id == RowId::TypeOfSpeedMod {
        let idx = state.speed_mod[player_idx].mod_type.choice_index();
        row.choices.get(idx).cloned().unwrap_or_default()
    } else {
        let idx = row.selected_choice_index[player_idx].min(row.choices.len().saturating_sub(1));
        row.choices.get(idx).cloned().unwrap_or_default()
    };

    let (draw_w, draw_h) = measure_option_text(asset_manager, &display_text, value_zoom);
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
