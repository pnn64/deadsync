use super::*;

#[inline(always)]
pub(super) fn measure_text_box(asset_manager: &AssetManager, text: &str, zoom: f32) -> (f32, f32) {
    let mut out_w = 1.0_f32;
    let mut out_h = 16.0_f32;
    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("miso", |metrics_font| {
            out_h = (metrics_font.height as f32).max(1.0) * zoom;
            let mut w = font::measure_line_width_logical(metrics_font, text, all_fonts) as f32;
            if !w.is_finite() || w <= 0.0 {
                w = 1.0;
            }
            out_w = w * zoom;
        });
    });
    (out_w, out_h)
}

#[inline(always)]
pub(super) fn ring_size_for_text(draw_w: f32, text_h: f32) -> (f32, f32) {
    let pad_y = widescale(6.0, 8.0);
    let min_pad_x = widescale(2.0, 3.0);
    let max_pad_x = widescale(22.0, 28.0);
    let width_ref = widescale(180.0, 220.0);
    let border_w = widescale(2.0, 2.5);
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
    (draw_w + pad_x * 2.0, text_h + pad_y * 2.0)
}

#[inline(always)]
pub(super) fn row_mid_y_for_cursor(
    state: &State,
    row_idx: usize,
    total_rows: usize,
    selected: usize,
    s: f32,
    list_y: f32,
) -> f32 {
    state
        .row_tweens
        .get(row_idx)
        .map(|tw| tw.to_y)
        .unwrap_or_else(|| row_dest_for_index(total_rows, selected, row_idx, s, list_y).0)
}

#[inline(always)]
pub(super) fn wrap_miso_text(
    asset_manager: &AssetManager,
    raw_text: &str,
    max_width_px: f32,
    zoom: f32,
) -> String {
    asset_manager
        .with_fonts(|all_fonts| {
            asset_manager.with_font("miso", |miso_font| {
                let mut out = String::new();
                let mut is_first_output_line = true;

                for segment in raw_text.split('\n') {
                    let trimmed = segment.trim_end();
                    if trimmed.is_empty() {
                        if !is_first_output_line {
                            out.push('\n');
                        }
                        continue;
                    }

                    let mut current_line = String::new();
                    for word in trimmed.split_whitespace() {
                        let candidate = if current_line.is_empty() {
                            word.to_owned()
                        } else {
                            let mut tmp = current_line.clone();
                            tmp.push(' ');
                            tmp.push_str(word);
                            tmp
                        };

                        let logical_w =
                            font::measure_line_width_logical(miso_font, &candidate, all_fonts)
                                as f32;
                        if !current_line.is_empty() && logical_w * zoom > max_width_px {
                            if !is_first_output_line {
                                out.push('\n');
                            }
                            out.push_str(&current_line);
                            is_first_output_line = false;
                            current_line.clear();
                            current_line.push_str(word);
                        } else {
                            current_line = candidate;
                        }
                    }

                    if !current_line.is_empty() {
                        if !is_first_output_line {
                            out.push('\n');
                        }
                        out.push_str(&current_line);
                        is_first_output_line = false;
                    }
                }

                if out.is_empty() {
                    raw_text.to_string()
                } else {
                    out
                }
            })
        })
        .unwrap_or_else(|| raw_text.to_string())
}

pub(super) fn build_description_layout(
    asset_manager: &AssetManager,
    key: DescriptionCacheKey,
    item: &Item,
    s: f32,
) -> DescriptionLayout {
    let title_side_pad = DESC_TITLE_SIDE_PAD_PX * s;
    let wrap_extra_pad = desc_wrap_extra_pad_unscaled() * s;
    let title_max_width_px =
        desc_w_unscaled().mul_add(s, -((2.0 * title_side_pad) + wrap_extra_pad));
    let bullet_side_pad = DESC_BULLET_SIDE_PAD_PX * s;
    let bullet_max_width_px = desc_w_unscaled().mul_add(
        s,
        -((2.0 * bullet_side_pad) + (DESC_BULLET_INDENT_PX * s) + wrap_extra_pad),
    );

    let mut blocks = Vec::new();

    if item.help.is_empty() {
        // No help entries — show the item name as a paragraph fallback.
        let wrapped = wrap_miso_text(
            asset_manager,
            &item.name.get(),
            title_max_width_px,
            DESC_TITLE_ZOOM * s,
        );
        blocks.push(RenderedHelpBlock::Paragraph {
            line_count: wrapped.lines().count().max(1),
            text: Arc::from(wrapped),
        });
    } else {
        for entry in item.help {
            match entry {
                HelpEntry::Paragraph(lkey) => {
                    let raw = lkey.get();
                    let wrapped = wrap_miso_text(
                        asset_manager,
                        &raw,
                        title_max_width_px,
                        DESC_TITLE_ZOOM * s,
                    );
                    blocks.push(RenderedHelpBlock::Paragraph {
                        line_count: wrapped.lines().count().max(1),
                        text: Arc::from(wrapped),
                    });
                }
                HelpEntry::Bullet(lkey) => {
                    let resolved = lkey.get();
                    let trimmed = resolved.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let mut entry_str = String::with_capacity(trimmed.len() + 2);
                    entry_str.push('\u{2022}');
                    entry_str.push(' ');
                    entry_str.push_str(trimmed);
                    let wrapped = wrap_miso_text(
                        asset_manager,
                        &entry_str,
                        bullet_max_width_px,
                        DESC_BODY_ZOOM * s,
                    );
                    blocks.push(RenderedHelpBlock::Bullet {
                        line_count: wrapped.lines().count().max(1),
                        text: Arc::from(wrapped),
                    });
                }
            }
        }
    }

    DescriptionLayout { key, blocks }
}

pub(super) fn description_layout(
    state: &State,
    asset_manager: &AssetManager,
    key: DescriptionCacheKey,
    item: &Item,
    s: f32,
) -> DescriptionLayout {
    if let Some(layout) = state.description_layout_cache.borrow().as_ref()
        && layout.key == key
    {
        return layout.clone();
    }
    let layout = build_description_layout(asset_manager, key, item, s);
    *state.description_layout_cache.borrow_mut() = Some(layout.clone());
    layout
}

pub fn clear_description_layout_cache(state: &State) {
    *state.description_layout_cache.borrow_mut() = None;
}

pub fn clear_render_cache(state: &State) {
    clear_submenu_row_layout_cache(state);
    clear_description_layout_cache(state);
}

pub(super) fn submenu_cursor_dest(
    state: &State,
    asset_manager: &AssetManager,
    kind: SubmenuKind,
    s: f32,
    list_x: f32,
    list_y: f32,
    list_w: f32,
) -> Option<(f32, f32, f32, f32)> {
    if is_launcher_submenu(kind) {
        return None;
    }
    let rows = submenu_rows(kind);
    let total_rows = submenu_total_rows(state, kind);
    if total_rows == 0 {
        return None;
    }
    let selected_row = state.sub_selected.min(total_rows - 1);
    let row_mid_y = row_mid_y_for_cursor(state, selected_row, total_rows, selected_row, s, list_y);
    let value_zoom = 0.835_f32;
    let label_bg_w = SUB_LABEL_COL_W * s;
    let item_col_left = list_x + label_bg_w;
    let item_col_w = list_w - label_bg_w;
    let single_center_x =
        item_col_w.mul_add(0.5, item_col_left) + SUB_SINGLE_VALUE_CENTER_OFFSET * s;

    if selected_row == total_rows - 1 {
        let (draw_w, text_h) = measure_text_box(asset_manager, "Exit", value_zoom);
        let (ring_w, ring_h) = ring_size_for_text(draw_w, text_h);
        return Some((single_center_x, row_mid_y, ring_w, ring_h));
    }
    let row_idx = submenu_visible_row_to_actual(state, kind, selected_row)?;
    let row = &rows[row_idx];
    let layout = submenu_row_layout(state, asset_manager, kind, row_idx)?;
    if layout.texts.is_empty() {
        return None;
    }
    let selected_choice = submenu_cursor_indices(state, kind)
        .get(row_idx)
        .copied()
        .unwrap_or(0)
        .min(layout.texts.len().saturating_sub(1));

    let draw_w = layout.widths[selected_choice];
    let center_x = if row.inline && layout.inline_row {
        let choice_inner_left = SUB_INLINE_ITEMS_LEFT_PAD.mul_add(s, list_x + label_bg_w);
        choice_inner_left + layout.centers[selected_choice]
    } else {
        single_center_x
    };
    let (ring_w, ring_h) = ring_size_for_text(draw_w, layout.text_h);
    Some((center_x, row_mid_y, ring_w, ring_h))
}

pub(super) fn build_yes_no_confirm_overlay(
    prompt_text: String,
    active_choice: u8,
    active_color_index: i32,
) -> Vec<Actor> {
    let w = screen_width();
    let h = screen_height();
    let cx = w * 0.5;
    let cy = h * 0.5;
    let answer_y = cy + 118.0;
    let yes_x = cx - 100.0;
    let no_x = cx + 100.0;
    let cursor_x = [yes_x, no_x][active_choice.min(1) as usize];
    let cursor_color = color::simply_love_rgba(active_color_index);

    vec![
        act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(w, h):
            diffuse(0.0, 0.0, 0.0, 0.9):
            z(700)
        ),
        act!(quad:
            align(0.5, 0.5):
            xy(cursor_x, answer_y):
            setsize(145.0, 40.0):
            diffuse(cursor_color[0], cursor_color[1], cursor_color[2], 1.0):
            z(701)
        ),
        act!(text:
            align(0.5, 0.5):
            xy(cx, cy - 65.0):
            font("miso"):
            zoom(0.95):
            maxwidth(w - 90.0):
            settext(prompt_text):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(702):
            horizalign(center)
        ),
        act!(text:
            align(0.5, 0.5):
            xy(yes_x, answer_y):
            font(current_machine_font_key(FontRole::Header)):
            zoom(0.72):
            settext(tr("Common", "Yes")):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(702):
            horizalign(center)
        ),
        act!(text:
            align(0.5, 0.5):
            xy(no_x, answer_y):
            font(current_machine_font_key(FontRole::Header)):
            zoom(0.72):
            settext(tr("Common", "No")):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(702):
            horizalign(center)
        ),
    ]
}

pub fn get_actors(
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(320);
    let is_fading_submenu = !matches!(state.submenu_transition, SubmenuTransition::None);

    /* -------------------------- HEART BACKGROUND -------------------------- */
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index, // <-- CHANGED
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        // Keep hearts always visible for actor-only fades (Options/Menu/Mappings);
        // local submenu fades are handled via content_alpha on UI actors only.
        alpha_mul: 1.0,
    }));

    if alpha_multiplier <= 0.0 {
        return actors;
    }

    if let Some(reload) = &state.reload_ui {
        let mut ui_actors = build_reload_overlay_actors(reload, state.active_color_index);
        for actor in &mut ui_actors {
            actor.mul_alpha(alpha_multiplier);
        }
        actors.extend(ui_actors);
        return actors;
    }
    if let Some(score_import) = &state.score_import_ui {
        let header = if score_import.done {
            "Score import complete"
        } else {
            "Importing scores..."
        };
        let total = score_import.total_charts.max(score_import.processed_charts);
        let progress_line = format!(
            "Endpoint: {}   Profile: {}\nPack: {}\nProgress: {}/{} (found={}, missing={}, failed={})",
            score_import.endpoint.display_name(),
            score_import.profile_name,
            score_import.pack_label,
            score_import.processed_charts,
            total,
            score_import.imported_scores,
            score_import.missing_scores,
            score_import.failed_requests
        );
        let detail_line = if score_import.done {
            score_import.done_message.as_str()
        } else {
            score_import.detail_line.as_str()
        };
        let text = format!("{header}\n{progress_line}\n{detail_line}");

        let mut ui_actors: Vec<Actor> = Vec::with_capacity(2);
        ui_actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 0.7):
            z(300)
        ));
        ui_actors.push(act!(text:
            align(0.5, 0.5):
            xy(screen_width() * 0.5, screen_height() * 0.5):
            zoom(0.95):
            diffuse(1.0, 1.0, 1.0, 1.0):
            font("miso"):
            settext(text):
            horizalign(center):
            z(301)
        ));
        for actor in &mut ui_actors {
            actor.mul_alpha(alpha_multiplier);
        }
        actors.extend(ui_actors);
        return actors;
    }
    if let Some(mut ui_actors) =
        shared_pack_sync::build_overlay(&state.pack_sync_overlay, state.active_color_index)
    {
        for actor in &mut ui_actors {
            actor.mul_alpha(alpha_multiplier);
        }
        actors.extend(ui_actors);
        return actors;
    }

    let mut ui_actors = Vec::new();

    /* ------------------------------ TOP BAR ------------------------------- */
    const FG: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    let title_text = match state.view {
        OptionsView::Main => "OPTIONS",
        OptionsView::Submenu(kind) => submenu_title(kind),
    };
    ui_actors.push(screen_bar::build(screen_bar::ScreenBarParams {
        title: title_text,
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
        fg_color: FG,
    }));

    /* --------------------------- MAIN CONTENT UI -------------------------- */

    // --- global colors ---
    let col_active_bg = color::rgba_hex("#333333"); // active bg for normal rows

    // inactive bg = #071016 @ 0.8 alpha
    let base_inactive = color::rgba_hex("#071016");
    let col_inactive_bg: [f32; 4] = [base_inactive[0], base_inactive[1], base_inactive[2], 0.8];

    let col_white = [1.0, 1.0, 1.0, 1.0];
    let col_black = [0.0, 0.0, 0.0, 1.0];

    // Simply Love brand color (now uses the active theme color).
    let col_brand_bg = color::simply_love_rgba(state.active_color_index); // <-- CHANGED

    // --- scale & origin honoring fixed screen-space margins ---
    let (s, list_x, list_y) = scaled_block_origin_with_margins();

    // Geometry (scaled)
    let list_w = list_w_unscaled() * s;
    let sep_w = SEP_W * s;
    let desc_w = desc_w_unscaled() * s;
    let desc_h = DESC_H * s;

    // Separator immediately to the RIGHT of the rows, aligned to the FIRST row top
    ui_actors.push(act!(quad:
        align(0.0, 0.0):
        xy(list_x + list_w, list_y):
        zoomto(sep_w, desc_h):
        diffuse(col_active_bg[0], col_active_bg[1], col_active_bg[2], col_active_bg[3]) // #333333
    ));

    // Description box (RIGHT of separator), aligned to the first row top
    let desc_x = list_x + list_w + sep_w;
    ui_actors.push(act!(quad:
        align(0.0, 0.0):
        xy(desc_x, list_y):
        zoomto(desc_w, desc_h):
        diffuse(col_active_bg[0], col_active_bg[1], col_active_bg[2], col_active_bg[3]) // #333333
    ));

    // -------------------------- Rows + Description -------------------------
    let selected_item: Option<(DescriptionCacheKey, &Item)>;
    let cursor_now = || -> Option<(f32, f32, f32, f32)> {
        if !state.cursor_initialized {
            return None;
        }
        let t = state.cursor_t.clamp(0.0, 1.0);
        let x = (state.cursor_to_x - state.cursor_from_x).mul_add(t, state.cursor_from_x);
        let y = (state.cursor_to_y - state.cursor_from_y).mul_add(t, state.cursor_from_y);
        let w = (state.cursor_to_w - state.cursor_from_w).mul_add(t, state.cursor_from_w);
        let h = (state.cursor_to_h - state.cursor_from_h).mul_add(t, state.cursor_from_h);
        Some((x, y, w, h))
    };

    match state.view {
        OptionsView::Main => {
            // Active text color (for normal rows) – Simply Love uses row index + global color index.
            let col_active_text =
                color::simply_love_rgba(state.active_color_index + state.selected as i32);

            let total_items = ITEMS.len();
            let row_h = ROW_H * s;
            for (item_idx, _) in ITEMS.iter().enumerate() {
                let (row_mid_y, row_alpha) = state
                    .row_tweens
                    .get(item_idx)
                    .map(|tw| (tw.y(), tw.a()))
                    .unwrap_or_else(|| {
                        row_dest_for_index(total_items, state.selected, item_idx, s, list_y)
                    });
                let row_alpha = row_alpha.clamp(0.0, 1.0);
                if row_alpha <= 0.001 {
                    continue;
                }
                let row_y = row_mid_y - 0.5 * row_h;
                let is_active = item_idx == state.selected;
                let is_exit = item_idx == total_items - 1;
                let row_w = if is_exit || !is_active {
                    list_w - sep_w
                } else {
                    list_w
                };
                let bg = if is_active {
                    if is_exit { col_brand_bg } else { col_active_bg }
                } else {
                    col_inactive_bg
                };

                ui_actors.push(act!(quad:
                    align(0.0, 0.0):
                    xy(list_x, row_y):
                    zoomto(row_w, row_h):
                    diffuse(bg[0], bg[1], bg[2], bg[3] * row_alpha)
                ));

                let heart_x = HEART_LEFT_PAD.mul_add(s, list_x);
                let text_x_base = TEXT_LEFT_PAD.mul_add(s, list_x);
                if !is_exit {
                    let mut heart_tint = if is_active {
                        col_active_text
                    } else {
                        col_white
                    };
                    heart_tint[3] *= row_alpha;
                    ui_actors.push(act!(sprite("heart.png"):
                        align(0.0, 0.5):
                        xy(heart_x, row_mid_y):
                        zoom(HEART_ZOOM):
                        diffuse(heart_tint[0], heart_tint[1], heart_tint[2], heart_tint[3])
                    ));
                }

                let text_x = if is_exit { heart_x } else { text_x_base };
                let label = ITEMS[item_idx].name.get();
                let mut color_t = if is_exit {
                    if is_active { col_black } else { col_white }
                } else if is_active {
                    col_active_text
                } else {
                    col_white
                };
                color_t[3] *= row_alpha;
                ui_actors.push(act!(text:
                    align(0.0, 0.5):
                    xy(text_x, row_mid_y):
                    zoom(ITEM_TEXT_ZOOM):
                    diffuse(color_t[0], color_t[1], color_t[2], color_t[3]):
                    font("miso"):
                    settext(&label):
                    horizalign(left)
                ));
            }

            let sel = state.selected.min(ITEMS.len() - 1);
            selected_item = Some((DescriptionCacheKey::Main(sel), &ITEMS[sel]));
        }
        OptionsView::Submenu(kind) => {
            let rows = submenu_rows(kind);
            let choice_indices = submenu_choice_indices(state, kind);
            let items = submenu_items(kind);
            let visible_rows = submenu_visible_row_indices(state, kind, rows);
            if is_launcher_submenu(kind) {
                let col_active_text =
                    color::simply_love_rgba(state.active_color_index + state.sub_selected as i32);
                let total_rows = rows.len() + 1;
                let row_h = ROW_H * s;
                for row_idx in 0..total_rows {
                    let (row_mid_y, row_alpha) = state
                        .row_tweens
                        .get(row_idx)
                        .map(|tw| (tw.y(), tw.a()))
                        .unwrap_or_else(|| {
                            row_dest_for_index(total_rows, state.sub_selected, row_idx, s, list_y)
                        });
                    let row_alpha = row_alpha.clamp(0.0, 1.0);
                    if row_alpha <= 0.001 {
                        continue;
                    }
                    let row_y = row_mid_y - 0.5 * row_h;
                    let is_active = row_idx == state.sub_selected;
                    let is_exit = row_idx == total_rows - 1;
                    let row_w = if is_exit || !is_active {
                        list_w - sep_w
                    } else {
                        list_w
                    };
                    let bg = if is_active {
                        if is_exit { col_brand_bg } else { col_active_bg }
                    } else {
                        col_inactive_bg
                    };

                    ui_actors.push(act!(quad:
                        align(0.0, 0.0):
                        xy(list_x, row_y):
                        zoomto(row_w, row_h):
                        diffuse(bg[0], bg[1], bg[2], bg[3] * row_alpha)
                    ));

                    let heart_x = HEART_LEFT_PAD.mul_add(s, list_x);
                    let text_x_base = TEXT_LEFT_PAD.mul_add(s, list_x);
                    if !is_exit {
                        let mut heart_tint = if is_active {
                            col_active_text
                        } else {
                            col_white
                        };
                        heart_tint[3] *= row_alpha;
                        ui_actors.push(act!(sprite("heart.png"):
                            align(0.0, 0.5):
                            xy(heart_x, row_mid_y):
                            zoom(HEART_ZOOM):
                            diffuse(heart_tint[0], heart_tint[1], heart_tint[2], heart_tint[3])
                        ));
                    }

                    let text_x = if is_exit { heart_x } else { text_x_base };
                    let label = if row_idx < rows.len() {
                        rows[row_idx].label.get()
                    } else {
                        Arc::from("Exit")
                    };
                    let mut text_color = if is_exit {
                        if is_active { col_black } else { col_white }
                    } else if is_active {
                        col_active_text
                    } else {
                        col_white
                    };
                    text_color[3] *= row_alpha;
                    ui_actors.push(act!(text:
                        align(0.0, 0.5):
                        xy(text_x, row_mid_y):
                        zoom(ITEM_TEXT_ZOOM):
                        diffuse(text_color[0], text_color[1], text_color[2], text_color[3]):
                        font("miso"):
                        settext(&label):
                        horizalign(left)
                    ));

                    if row_idx < rows.len() {
                        let row = &rows[row_idx];
                        if row.inline {
                            let choices = row_choices(state, kind, rows, row_idx);
                            if !choices.is_empty() {
                                let choice_idx = choice_indices
                                    .get(row_idx)
                                    .copied()
                                    .unwrap_or(0)
                                    .min(choices.len().saturating_sub(1));
                                let mut value_color = if is_active {
                                    col_active_text
                                } else {
                                    col_white
                                };
                                value_color[3] *= row_alpha;
                                let value_x = list_w.mul_add(1.0, list_x - TEXT_LEFT_PAD * s);
                                ui_actors.push(act!(text:
                                    align(1.0, 0.5):
                                    xy(value_x, row_mid_y):
                                    zoom(ITEM_TEXT_ZOOM):
                                    diffuse(value_color[0], value_color[1], value_color[2], value_color[3]):
                                    font("miso"):
                                    settext(choices[choice_idx].clone().into_owned()):
                                    horizalign(right)
                                ));
                            }
                        }
                    }
                }

                let sel = state.sub_selected.min(total_rows.saturating_sub(1));
                let (item_idx, item) = if sel < rows.len() {
                    (sel, &items[sel])
                } else {
                    let idx = items.len().saturating_sub(1);
                    (idx, &items[idx])
                };
                selected_item = Some((DescriptionCacheKey::Submenu(kind, item_idx), item));
            } else {
                // Active text color for submenu rows.
                let col_active_text = color::simply_love_rgba(state.active_color_index);
                // Inactive option text color should be #808080 (alpha 1.0), match player options.
                let sl_gray = color::rgba_hex("#808080");

                let total_rows = visible_rows.len() + 1; // + Exit row

                let label_bg_w = SUB_LABEL_COL_W * s;
                let label_text_x = SUB_LABEL_TEXT_LEFT_PAD.mul_add(s, list_x);
                // Keep submenu header labels bounded to the left label column.
                let label_text_max_w = (label_bg_w - SUB_LABEL_TEXT_LEFT_PAD * s - 5.0).max(0.0);

                // Helper to compute the cursor center X for a given submenu row index.
                let calc_row_center_x = |row_idx: usize| -> f32 {
                    if row_idx >= total_rows {
                        return list_w.mul_add(0.5, list_x);
                    }
                    if row_idx == total_rows - 1 {
                        // Exit row: center within the items column (row width minus label column),
                        // matching how single-value rows like Music Rate are centered in player_options.rs.
                        let item_col_left = list_x + label_bg_w;
                        let item_col_w = list_w - label_bg_w;
                        return item_col_w.mul_add(0.5, item_col_left)
                            + SUB_SINGLE_VALUE_CENTER_OFFSET * s;
                    }
                    let Some(actual_row_idx) = visible_rows.get(row_idx).copied() else {
                        return list_w.mul_add(0.5, list_x);
                    };
                    let row = &rows[actual_row_idx];
                    let item_col_left = list_x + label_bg_w;
                    let item_col_w = list_w - label_bg_w;
                    let single_center_x =
                        item_col_w.mul_add(0.5, item_col_left) + SUB_SINGLE_VALUE_CENTER_OFFSET * s;
                    // Non-inline rows behave as single-value rows: keep the cursor centered
                    // on the center of the available items column (row width minus label column).
                    if !row.inline {
                        return single_center_x;
                    }
                    let Some(layout) =
                        submenu_row_layout(state, asset_manager, kind, actual_row_idx)
                    else {
                        return list_w.mul_add(0.5, list_x);
                    };
                    if !layout.inline_row || layout.centers.is_empty() {
                        return single_center_x;
                    }
                    let sel_idx = choice_indices
                        .get(actual_row_idx)
                        .copied()
                        .unwrap_or(0)
                        .min(layout.centers.len().saturating_sub(1));
                    SUB_INLINE_ITEMS_LEFT_PAD.mul_add(s, list_x + label_bg_w)
                        + layout.centers[sel_idx]
                };

                let row_h = ROW_H * s;
                for row_idx in 0..total_rows {
                    let (row_mid_y, row_alpha) = state
                        .row_tweens
                        .get(row_idx)
                        .map(|tw| (tw.y(), tw.a()))
                        .unwrap_or_else(|| {
                            row_dest_for_index(total_rows, state.sub_selected, row_idx, s, list_y)
                        });
                    let row_alpha = row_alpha.clamp(0.0, 1.0);
                    if row_alpha <= 0.001 {
                        continue;
                    }
                    let row_y = row_mid_y - 0.5 * row_h;

                    let is_active = row_idx == state.sub_selected;
                    let is_exit = row_idx == total_rows - 1;

                    let row_w = if is_exit {
                        list_w - sep_w
                    } else if is_active {
                        list_w
                    } else {
                        list_w - sep_w
                    };

                    let bg = if is_active {
                        col_active_bg
                    } else {
                        col_inactive_bg
                    };

                    ui_actors.push(act!(quad:
                        align(0.0, 0.0):
                        xy(list_x, row_y):
                        zoomto(row_w, row_h):
                        diffuse(bg[0], bg[1], bg[2], bg[3] * row_alpha)
                    ));
                    let show_option_row = !is_exit;

                    if show_option_row {
                        let Some(actual_row_idx) = visible_rows.get(row_idx).copied() else {
                            continue;
                        };
                        // Left label background column (matches player options style).
                        ui_actors.push(act!(quad:
                            align(0.0, 0.0):
                            xy(list_x, row_y):
                            zoomto(label_bg_w, row_h):
                            diffuse(0.0, 0.0, 0.0, 0.25 * row_alpha)
                        ));

                        let row = &rows[actual_row_idx];
                        let label = row.label.get();
                        let is_disabled = is_submenu_row_disabled(kind, row.id);
                        #[cfg(target_os = "linux")]
                        let child_label_indent = if matches!(kind, SubmenuKind::Sound)
                            && sound_parent_row(actual_row_idx).is_some()
                        {
                            12.0 * s
                        } else {
                            0.0
                        };
                        #[cfg(not(target_os = "linux"))]
                        let child_label_indent = 0.0;
                        let label_text_x = label_text_x + child_label_indent;
                        let label_text_max_w = (label_text_max_w - child_label_indent).max(0.0);
                        let title_color = if is_active {
                            let mut c = col_active_text;
                            c[3] = 1.0;
                            c
                        } else {
                            col_white
                        };
                        let mut title_color = title_color;
                        title_color[3] *= row_alpha;

                        ui_actors.push(act!(text:
                            align(0.0, 0.5):
                            xy(label_text_x, row_mid_y):
                            zoom(ITEM_TEXT_ZOOM):
                            diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                            font("miso"):
                            settext(&label):
                            maxwidth(label_text_max_w):
                            horizalign(left)
                        ));

                        // Inline Off/On options in the items column (or a single centered value if inline == false).
                        if let Some(layout) =
                            submenu_row_layout(state, asset_manager, kind, actual_row_idx)
                            && !layout.texts.is_empty()
                        {
                            let value_zoom = 0.835_f32;
                            let selected_choice = choice_indices
                                .get(actual_row_idx)
                                .copied()
                                .unwrap_or(0)
                                .min(layout.texts.len().saturating_sub(1));
                            let is_chart_info_row = matches!(kind, SubmenuKind::SelectMusic)
                                && row.id == SubRowId::ChartInfo;
                            let is_scorebox_cycle_row = matches!(kind, SubmenuKind::SelectMusic)
                                && row.id == SubRowId::GsBoxLeaderboards;
                            let is_auto_screenshot_row = matches!(kind, SubmenuKind::Gameplay)
                                && row.id == SubRowId::AutoScreenshot;
                            let is_multi_toggle_row = is_chart_info_row
                                || is_scorebox_cycle_row
                                || is_auto_screenshot_row;
                            let chart_info_enabled_mask = if is_chart_info_row {
                                select_music_chart_info_enabled_mask()
                            } else {
                                0
                            };
                            let scorebox_enabled_mask = if is_scorebox_cycle_row {
                                select_music_scorebox_cycle_enabled_mask()
                            } else {
                                0
                            };
                            let auto_screenshot_mask = if is_auto_screenshot_row {
                                auto_screenshot_enabled_mask()
                            } else {
                                0
                            };
                            let mut selected_left_x: Option<f32> = None;
                            let choice_inner_left =
                                SUB_INLINE_ITEMS_LEFT_PAD.mul_add(s, list_x + label_bg_w);

                            if layout.inline_row {
                                for (idx, choice) in layout.texts.iter().enumerate() {
                                    let x = choice_inner_left
                                        + layout.x_positions.get(idx).copied().unwrap_or_default();
                                    let is_choice_selected = idx == selected_choice;
                                    if is_choice_selected {
                                        selected_left_x = Some(x);
                                    }
                                    let is_choice_enabled = if is_chart_info_row {
                                        (chart_info_enabled_mask
                                            & select_music_chart_info_bit_from_choice(idx))
                                            != 0
                                    } else if is_scorebox_cycle_row {
                                        (scorebox_enabled_mask
                                            & scorebox_cycle_bit_from_choice(idx))
                                            != 0
                                    } else if is_auto_screenshot_row {
                                        (auto_screenshot_mask
                                            & auto_screenshot_bit_from_choice(idx))
                                            != 0
                                    } else {
                                        false
                                    };
                                    let mut choice_color = if is_disabled && !is_choice_selected {
                                        sl_gray
                                    } else if is_multi_toggle_row {
                                        if is_choice_enabled {
                                            col_white
                                        } else {
                                            sl_gray
                                        }
                                    } else if is_active {
                                        col_white
                                    } else {
                                        sl_gray
                                    };
                                    choice_color[3] *= row_alpha;
                                    ui_actors.push(act!(text:
                                        align(0.0, 0.5):
                                        xy(x, row_mid_y):
                                        zoom(value_zoom):
                                        diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                                        font("miso"):
                                        settext(choice):
                                        horizalign(left)
                                    ));
                                }
                            } else {
                                let mut choice_color = if is_active { col_white } else { sl_gray };
                                choice_color[3] *= row_alpha;
                                let choice_center_x = calc_row_center_x(row_idx);
                                let draw_w =
                                    layout.widths.get(selected_choice).copied().unwrap_or(40.0);
                                selected_left_x = Some(choice_center_x - draw_w * 0.5);
                                let choice_text = layout
                                    .texts
                                    .get(selected_choice)
                                    .cloned()
                                    .unwrap_or_else(|| Arc::<str>::from("??"));
                                ui_actors.push(act!(text:
                                    align(0.5, 0.5):
                                    xy(choice_center_x, row_mid_y):
                                    zoom(value_zoom):
                                    diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                                    font("miso"):
                                    settext(choice_text):
                                    horizalign(center)
                                ));
                            }

                            // For normal rows, underline the selected option.
                            // For multi-toggle rows, underline each enabled option.
                            if layout.inline_row && is_multi_toggle_row {
                                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                                let offset = widescale(3.0, 4.0);
                                let underline_y = row_mid_y + layout.text_h * 0.5 + offset;
                                let mut line_color =
                                    color::decorative_rgba(state.active_color_index);
                                line_color[3] *= row_alpha;
                                for idx in 0..layout.texts.len() {
                                    let enabled = if is_chart_info_row {
                                        let bit = select_music_chart_info_bit_from_choice(idx);
                                        bit != 0 && (chart_info_enabled_mask & bit) != 0
                                    } else if is_scorebox_cycle_row {
                                        let bit = scorebox_cycle_bit_from_choice(idx);
                                        bit != 0 && (scorebox_enabled_mask & bit) != 0
                                    } else {
                                        let bit = auto_screenshot_bit_from_choice(idx);
                                        bit != 0 && (auto_screenshot_mask & bit) != 0
                                    };
                                    if !enabled {
                                        continue;
                                    }
                                    let underline_left_x = choice_inner_left
                                        + layout.x_positions.get(idx).copied().unwrap_or_default();
                                    let underline_w =
                                        layout.widths.get(idx).copied().unwrap_or(40.0).ceil();
                                    ui_actors.push(act!(quad:
                                        align(0.0, 0.5):
                                        xy(underline_left_x, underline_y):
                                        zoomto(underline_w, line_thickness):
                                        diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                        z(101)
                                    ));
                                }
                            } else if let Some(sel_left_x) = selected_left_x {
                                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                                let underline_w = layout
                                    .widths
                                    .get(selected_choice)
                                    .copied()
                                    .unwrap_or(40.0)
                                    .ceil();
                                let offset = widescale(3.0, 4.0);
                                let underline_y = row_mid_y + layout.text_h * 0.5 + offset;
                                let mut line_color =
                                    color::decorative_rgba(state.active_color_index);
                                line_color[3] *= row_alpha;
                                ui_actors.push(act!(quad:
                                    align(0.0, 0.5):
                                    xy(sel_left_x, underline_y):
                                    zoomto(underline_w, line_thickness):
                                    diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                    z(101)
                                ));
                            }

                            // Encircling cursor ring around the active option when this row is active.
                            // During submenu fades, hide the ring to avoid exposing its construction.
                            if is_active
                                && !is_fading_submenu
                                && let Some((center_x, center_y, ring_w, ring_h)) = cursor_now()
                            {
                                let border_w = widescale(2.0, 2.5);
                                let left = center_x - ring_w * 0.5;
                                let right = center_x + ring_w * 0.5;
                                let top = center_y - ring_h * 0.5;
                                let bottom = center_y + ring_h * 0.5;
                                let mut ring_color =
                                    color::decorative_rgba(state.active_color_index);
                                ring_color[3] *= row_alpha;
                                ui_actors.push(act!(quad:
                                align(0.5, 0.5):
                                xy(center_x, top + border_w * 0.5):
                                zoomto(ring_w, border_w):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                                ui_actors.push(act!(quad:
                                align(0.5, 0.5):
                                xy(center_x, bottom - border_w * 0.5):
                                zoomto(ring_w, border_w):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                                ui_actors.push(act!(quad:
                                align(0.5, 0.5):
                                xy(left + border_w * 0.5, center_y):
                                zoomto(border_w, ring_h):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                                ui_actors.push(act!(quad:
                                align(0.5, 0.5):
                                xy(right - border_w * 0.5, center_y):
                                zoomto(border_w, ring_h):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                            }
                        }
                    } else {
                        // Exit row: centered "Exit" text in the items column.
                        let exit_label = tr("Common", "Exit");
                        let label = exit_label.clone();
                        let value_zoom = 0.835_f32;
                        let mut choice_color = if is_active { col_white } else { sl_gray };
                        choice_color[3] *= row_alpha;
                        let center_x = calc_row_center_x(row_idx);
                        let center_y = row_mid_y;

                        ui_actors.push(act!(text:
                        align(0.5, 0.5):
                        xy(center_x, center_y):
                        zoom(value_zoom):
                        diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                        font("miso"):
                        settext(label):
                        horizalign(center)
                    ));

                        // Draw the selection cursor ring for the Exit row when active.
                        // During submenu fades, hide the ring to avoid exposing its construction.
                        if is_active
                            && !is_fading_submenu
                            && let Some((ring_x, ring_y, ring_w, ring_h)) = cursor_now()
                        {
                            let border_w = widescale(2.0, 2.5);
                            let left = ring_x - ring_w * 0.5;
                            let right = ring_x + ring_w * 0.5;
                            let top = ring_y - ring_h * 0.5;
                            let bottom = ring_y + ring_h * 0.5;
                            let mut ring_color = color::decorative_rgba(state.active_color_index);
                            ring_color[3] *= row_alpha;

                            ui_actors.push(act!(quad:
                                align(0.5, 0.5):
                                xy((left + right) * 0.5, top + border_w * 0.5):
                                zoomto(ring_w, border_w):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                            ui_actors.push(act!(quad:
                                align(0.5, 0.5):
                                xy((left + right) * 0.5, bottom - border_w * 0.5):
                                zoomto(ring_w, border_w):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                            ui_actors.push(act!(quad:
                                align(0.5, 0.5):
                                xy(left + border_w * 0.5, (top + bottom) * 0.5):
                                zoomto(border_w, ring_h):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                            ui_actors.push(act!(quad:
                                align(0.5, 0.5):
                                xy(right - border_w * 0.5, (top + bottom) * 0.5):
                                zoomto(border_w, ring_h):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                        }
                    }
                }

                // Description items for the submenu
                let total_rows = visible_rows.len() + 1;
                let sel = state.sub_selected.min(total_rows.saturating_sub(1));
                let (item_idx, item) = if sel < visible_rows.len() {
                    let actual_row_idx = visible_rows[sel];
                    (actual_row_idx, &items[actual_row_idx])
                } else {
                    let idx = items.len().saturating_sub(1);
                    (idx, &items[idx])
                };
                selected_item = Some((DescriptionCacheKey::Submenu(kind, item_idx), item));
            }
        }
    }

    // ------------------- Description content (selected) -------------------
    if let Some((desc_key, item)) = selected_item {
        // Match Simply Love's description box feel:
        // - explicit top/side padding for title and bullets so they can be tuned
        // - text zoom similar to other help text (player options, etc.)
        let mut cursor_y = DESC_TITLE_TOP_PAD_PX.mul_add(s, list_y);
        let desc_layout = description_layout(state, asset_manager, desc_key, item, s);
        let title_side_pad = DESC_TITLE_SIDE_PAD_PX * s;
        let title_step_px = 20.0 * s;
        let body_step_px = 18.0 * s;
        let bullet_side_pad = DESC_BULLET_SIDE_PAD_PX * s;

        for block in &desc_layout.blocks {
            match block {
                RenderedHelpBlock::Paragraph { text, line_count } => {
                    ui_actors.push(act!(text:
                        align(0.0, 0.0):
                        xy(desc_x + title_side_pad, cursor_y):
                        zoom(DESC_TITLE_ZOOM):
                        diffuse(1.0, 1.0, 1.0, 1.0):
                        font("miso"): settext(text):
                        horizalign(left)
                    ));
                    cursor_y += title_step_px * *line_count as f32 + DESC_BULLET_TOP_PAD_PX * s;
                }
                RenderedHelpBlock::Bullet { text, line_count } => {
                    let bullet_x = DESC_BULLET_INDENT_PX.mul_add(s, desc_x + bullet_side_pad);
                    ui_actors.push(act!(text:
                        align(0.0, 0.0):
                        xy(bullet_x, cursor_y):
                        zoom(DESC_BODY_ZOOM):
                        diffuse(1.0, 1.0, 1.0, 1.0):
                        font("miso"): settext(text):
                        horizalign(left)
                    ));
                    cursor_y += body_step_px * *line_count as f32;
                }
            }
        }
    }
    if let Some(confirm) = &state.score_import_confirm {
        let prompt_text = format!(
            "Import ALL packs for {} / {}?\nOnly missing GS scores: {}.\nRate limit is hard-capped at 3 requests per second.\nFor many charts this can take more than one hour.\nSpamming APIs can be problematic.\n\nStart now?",
            confirm.selection.endpoint.display_name(),
            if confirm.selection.profile.display_name.is_empty() {
                confirm.selection.profile.id.as_str()
            } else {
                confirm.selection.profile.display_name.as_str()
            },
            if confirm.selection.only_missing_gs_scores {
                "Yes"
            } else {
                "No"
            }
        );
        ui_actors.extend(build_yes_no_confirm_overlay(
            prompt_text,
            confirm.active_choice,
            state.active_color_index,
        ));
    }
    if let Some(confirm) = &state.sync_pack_confirm {
        let prompt_text = format!(
            "Sync {}?\nThis will analyze every matching simfile here in Options.\nYou can review offsets and confidence before saving.\n\nStart now?",
            if confirm.selection.pack_group.is_none() {
                "ALL files"
            } else {
                confirm.selection.pack_label.as_str()
            }
        );
        ui_actors.extend(build_yes_no_confirm_overlay(
            prompt_text,
            confirm.active_choice,
            state.active_color_index,
        ));
    }

    let combined_alpha = alpha_multiplier * state.content_alpha;
    for actor in &mut ui_actors {
        actor.mul_alpha(combined_alpha);
    }
    actors.extend(ui_actors);

    actors
}
