use super::*;

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(64);
    let active = session_active_players();
    let show_p2 = active[P1] && active[P2];
    let pane_alpha = state.pane_transition.alpha();
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));
    let select_modifiers = tr("ScreenTitles", "SelectModifiers");
    actors.push(screen_bar::build(ScreenBarParams {
        title: &select_modifiers,
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
    }));

    let p1_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P1);
    let p2_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P2);
    let p1_avatar = p1_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });
    let p2_avatar = p2_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });

    let p1_joined =
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P1);
    let p2_joined =
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P2);
    let p1_guest =
        crate::game::profile::is_session_side_guest(crate::game::profile::PlayerSide::P1);
    let p2_guest =
        crate::game::profile::is_session_side_guest(crate::game::profile::PlayerSide::P2);

    let insert_card = tr("Common", "InsertCard");
    let press_start = tr("Common", "PressStart");

    let (footer_left, left_avatar) = if p1_joined {
        (
            Some(if p1_guest {
                insert_card.as_ref()
            } else {
                p1_profile.display_name.as_str()
            }),
            if p1_guest { None } else { p1_avatar },
        )
    } else {
        (Some(press_start.as_ref()), None)
    };
    let (footer_right, right_avatar) = if p2_joined {
        (
            Some(if p2_guest {
                insert_card.as_ref()
            } else {
                p2_profile.display_name.as_str()
            }),
            if p2_guest { None } else { p2_avatar },
        )
    } else {
        (Some(press_start.as_ref()), None)
    };
    let event_mode = tr("Common", "EventMode");
    actors.push(screen_bar::build(ScreenBarParams {
        title: &event_mode,
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar,
        right_avatar,
    }));
    // zmod ScreenPlayerOptions overlay/default.lua speed helper parity.
    let speed_mod_y = 48.0;
    let speed_mod_zoom = 0.5_f32;
    let speed_mod_scaled_y = 52.0_f32;
    let speed_mod_scaled_zoom = 0.3_f32;
    let speed_mod_x_p1 = screen_center_x() + widescale(-77.0, -100.0);
    let speed_mod_x_p2 = screen_center_x() + widescale(140.0, 154.0);
    let speed_mod_x = speed_mod_x_p1;
    // All previews (judgment, hold, noteskin, combo) share this center line.
    // Tweak these to dial in parity with Simply Love.
    const PREVIEW_CENTER_OFFSET_NORMAL: f32 = 80.75; // 4:3
    const PREVIEW_CENTER_OFFSET_WIDE: f32 = 98.75; // 16:9
    let preview_center_x =
        speed_mod_x_p1 + widescale(PREVIEW_CENTER_OFFSET_NORMAL, PREVIEW_CENTER_OFFSET_WIDE);

    let player_color_index = |player_idx: usize| {
        if player_idx == P2 {
            state.active_color_index - 2
        } else {
            state.active_color_index
        }
    };
    let speed_x_for = |player_idx: usize| {
        if player_idx == P2 {
            speed_mod_x_p2
        } else {
            speed_mod_x_p1
        }
    };
    let preview_dx = preview_center_x - speed_mod_x_p1;
    let preview_x_for = |player_idx: usize| speed_x_for(player_idx) + preview_dx;

    if state.current_pane == OptionsPane::Main {
        for player_idx in active_player_indices(active) {
            let speed_mod = &state.speed_mod[player_idx];
            let speed_color = color::simply_love_rgba(player_color_index(player_idx));
            let p_chart = resolve_p1_chart(&state.song, &state.chart_steps_index);
            let main_scroll =
                speed_mod_helper_scroll_text(&state.song, p_chart, speed_mod, state.music_rate);
            let speed_prefix = speed_mod.mod_type.prefix();
            let speed_text = format!("{speed_prefix}{main_scroll}");
            // zmod uses GetWidth() from the main helper actor (unzoomed width), then +w*0.4.
            let main_draw_w = measure_wendy_text_width(asset_manager, &speed_text);
            let speed_x = speed_x_for(player_idx);

            actors.push(act!(text: font("wendy"): settext(speed_text):
                align(0.5, 0.5): xy(speed_x, speed_mod_y): zoom(speed_mod_zoom):
                diffuse(speed_color[0], speed_color[1], speed_color[2], pane_alpha):
                z(121)
            ));

            let scaled_scroll = speed_mod_helper_scaled_text(
                &state.song,
                p_chart,
                speed_mod,
                state.music_rate,
                &state.player_profiles[player_idx],
            );
            if scaled_scroll != main_scroll {
                let scaled_text = format!("{speed_prefix}{scaled_scroll}");
                let scaled_x = speed_x + main_draw_w * 0.4;
                actors.push(act!(text: font("wendy"): settext(scaled_text):
                    align(0.5, 0.5): xy(scaled_x, speed_mod_scaled_y): zoom(speed_mod_scaled_zoom):
                    diffuse(speed_color[0], speed_color[1], speed_color[2], 0.8 * pane_alpha):
                    z(121)
                ));
            }
        }
    }
    /* ---------- SHARED GEOMETRY (rows aligned to help box) ---------- */
    // Help Text Box (from underlay.lua) — define this first so rows can match its width/left.
    let help_box_h = 40.0;
    let help_box_w = widescale(614.0, 792.0);
    let help_box_x = widescale(13.0, 30.666);
    let help_box_bottom_y = screen_height() - 36.0;
    let total_rows = state.pane().row_map.len();
    let frame_h = ROW_HEIGHT;
    let (fallback_y0, fallback_row_step) = row_layout_params();
    let row_alpha_cutoff: f32 = 0.001;
    // Make row frame LEFT and WIDTH exactly match the help box.
    let row_left = help_box_x;
    let row_width = help_box_w;
    //let row_center_x = row_left + (row_width * 0.5);
    let title_zoom = 0.88;
    // Title text x: slightly less padding so text sits further left.
    let title_left_pad = widescale(7.0, 13.0);
    let title_x = row_left + title_left_pad;
    // Keep header labels bounded to the title column so they never overlap option values.
    let title_max_w = (TITLE_BG_WIDTH - title_left_pad - 5.0).max(0.0);
    let cursor_now = |player_idx: usize| -> Option<(f32, f32, f32, f32)> {
        if player_idx >= PLAYER_SLOTS || !state.pane().cursor_initialized[player_idx] {
            return None;
        }
        let pane = state.pane();
        let t = pane.cursor_t[player_idx].clamp(0.0, 1.0);
        let r = CursorRect::lerp(pane.cursor_from[player_idx], pane.cursor_to[player_idx], t);
        Some((r.x, r.y, r.w, r.h))
    };

    for item_idx in 0..total_rows {
        let (current_row_y, row_alpha) = state
            .pane()
            .row_tweens
            .get(item_idx)
            .map(|tw| (tw.y(), tw.a()))
            .unwrap_or_else(|| {
                (
                    (item_idx as f32).mul_add(fallback_row_step, fallback_y0),
                    1.0,
                )
            });
        let row_alpha = (row_alpha * pane_alpha).clamp(0.0, 1.0);
        if row_alpha <= row_alpha_cutoff {
            continue;
        }
        let a = row_alpha;

        let is_active = (active[P1] && item_idx == state.pane().selected_row[P1])
            || (active[P2] && item_idx == state.pane().selected_row[P2]);
        let row = state
            .pane()
            .row_map
            .row(state.pane().row_map.id_at(item_idx));
        let active_bg = color::rgba_hex("#333333");
        let inactive_bg_base = color::rgba_hex("#071016");
        let bg_color = if is_active {
            active_bg
        } else {
            [
                inactive_bg_base[0],
                inactive_bg_base[1],
                inactive_bg_base[2],
                0.8,
            ]
        };
        // Row background — matches help box width & left
        actors.push(act!(quad:
            align(0.0, 0.5): xy(row_left, current_row_y):
            zoomto(row_width, frame_h):
            diffuse(bg_color[0], bg_color[1], bg_color[2], bg_color[3] * a):
            z(100)
        ));
        if row.id != RowId::Exit {
            actors.push(act!(quad:
                align(0.0, 0.5): xy(row_left, current_row_y):
                zoomto(TITLE_BG_WIDTH, frame_h):
                diffuse(0.0, 0.0, 0.0, 0.25 * a):
                z(101)
            ));
        }
        // Left column (row titles)
        let mut title_color = if is_active {
            let mut c = color::simply_love_rgba(state.active_color_index);
            c[3] = 1.0;
            c
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        title_color[3] *= a;
        // Handle multi-line row titles (e.g., "Music Rate\nbpm: 120")
        if row.id == RowId::MusicRate {
            let display = music_rate_display_name(state);
            let lines: Vec<&str> = display.split('\n').collect();
            if lines.len() == 2 {
                actors.push(act!(text: font("miso"): settext(lines[0].to_string()):
                    align(0.0, 0.5): xy(title_x, current_row_y - 7.0): zoom(title_zoom):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(title_max_w):
                    z(101)
                ));
                actors.push(act!(text: font("miso"): settext(lines[1].to_string()):
                    align(0.0, 0.5): xy(title_x, current_row_y + 7.0): zoom(title_zoom):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(title_max_w):
                    z(101)
                ));
            } else {
                actors.push(act!(text: font("miso"): settext(display):
                    align(0.0, 0.5): xy(title_x, current_row_y): zoom(title_zoom):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(title_max_w):
                    z(101)
                ));
            }
        } else {
            actors.push(
                act!(text: font("miso"): settext(row.name.get().to_string()):
                    align(0.0, 0.5): xy(title_x, current_row_y): zoom(title_zoom):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(title_max_w):
                    z(101)
                ),
            );
        }
        // Inactive option text color should be #808080 (alpha 1.0)
        let mut sl_gray = color::rgba_hex("#808080");
        sl_gray[3] *= a;
        // Some rows should display all choices inline
        let show_all_choices_inline = row_shows_all_choices_inline(row.id);
        let show_arcade_next_row = arcade_next_row_visible(state, item_idx);
        // Choice area: For single-choice rows (ShowOneInRow), use ItemsLongRowP1X positioning
        // For multi-choice rows (ShowAllInRow), use ItemsStartX positioning
        // ItemsLongRowP1X = WideScale(_screen.cx-100, _screen.cx-130) from Simply Love metrics
        // ItemsStartX = WideScale(146, 160) from Simply Love metrics
        let choice_inner_left = if show_all_choices_inline {
            inline_choice_left_x_for_row(state, item_idx)
        } else {
            screen_center_x() + widescale(-100.0, -130.0) // ItemsLongRowP1X for single-choice rows
        };
        if row.id == RowId::Exit {
            // Special case for the last "Exit" row
            let choice_text = &row.choices[row.selected_choice_index[P1]];
            let choice_color = if is_active {
                [1.0, 1.0, 1.0, a]
            } else {
                sl_gray
            };
            // Align Exit horizontally with other single-value options (Speed Mod line)
            let choice_center_x = speed_mod_x;
            actors.push(act!(text: font("miso"): settext(choice_text.clone()):
                align(0.5, 0.5): xy(choice_center_x, current_row_y): zoom(0.835):
                diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                z(101)
            ));
            // Draw the selection cursor for the centered "Exit" text when active
            if is_active {
                let border_w = widescale(2.0, 2.5);
                for player_idx in active_player_indices(active) {
                    if state.pane().selected_row[player_idx] != item_idx {
                        continue;
                    }
                    let Some((center_x, center_y, ring_w, ring_h)) = cursor_now(player_idx) else {
                        continue;
                    };

                    let left = center_x - ring_w * 0.5;
                    let right = center_x + ring_w * 0.5;
                    let top = center_y - ring_h * 0.5;
                    let bottom = center_y + ring_h * 0.5;
                    let mut ring_color = color::decorative_rgba(player_color_index(player_idx));
                    ring_color[3] *= a;

                    actors.push(act!(quad:
                        align(0.5, 0.5): xy((left + right) * 0.5, top + border_w * 0.5):
                        zoomto(ring_w, border_w):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy((left + right) * 0.5, bottom - border_w * 0.5):
                        zoomto(ring_w, border_w):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(left + border_w * 0.5, (top + bottom) * 0.5):
                        zoomto(border_w, ring_h):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(right - border_w * 0.5, (top + bottom) * 0.5):
                        zoomto(border_w, ring_h):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                }
            }
        } else if show_all_choices_inline {
            // Render every option horizontally; when active, all options should be white.
            // The active option gets an underline (quad) drawn just below the text.
            let value_zoom = 0.835;
            let spacing = 15.75;
            let next_row_item = show_arcade_next_row
                .then(|| arcade_next_row_layout(state, item_idx, asset_manager, value_zoom));
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
            let mut x_positions: Vec<f32> = Vec::with_capacity(widths.len());
            {
                let mut x = choice_inner_left;
                for w in &widths {
                    x_positions.push(x);
                    x += *w + spacing;
                }
            }
            // Draw underline under active options:
            // - For normal rows: underline the currently selected choice.
            // - For Scroll row: underline each enabled scroll mode (multi-select).
            // - For FA+ Options row: underline each enabled FA+ toggle (multi-select).
            if row.id == RowId::Scroll {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.scroll_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Hide {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.hide_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Insert {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.insert_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Remove {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.remove_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Holds {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.holds_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Accel {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.accel_effects_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Effect {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.visual_effects_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u16 << (idx as u16);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Appearance {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.appearance_effects_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::LifeBarOptions {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.life_bar_options_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::FAPlusOptions {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.fa_plus_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::GameplayExtras {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.gameplay_extras_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::GameplayExtrasMore {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.gameplay_extras_more_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::ResultsExtras {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.results_extras_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::MeasureCounterOptions {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.measure_counter_options_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::ErrorBar {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.error_bar_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::ErrorBarOptions {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.error_bar_options_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::EarlyDecentWayOffOptions {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.early_dw_active_mask[player_idx].bits();
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let idx =
                        row.selected_choice_index[player_idx].min(widths.len().saturating_sub(1));
                    if let Some(sel_x) = x_positions.get(idx).copied() {
                        let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                        let underline_w = draw_w.ceil();
                        let underline_y = underline_y_for(player_idx);
                        let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                        line_color[3] *= a;
                        actors.push(act!(quad:
                            align(0.0, 0.5):
                            xy(sel_x, underline_y):
                            zoomto(underline_w, line_thickness):
                            diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                            z(101)
                        ));
                    }
                }
            }
            // Draw the 4-sided cursor ring around the selected option when this row is active.
            if !widths.is_empty() {
                let border_w = widescale(2.0, 2.5);
                for player_idx in active_player_indices(active) {
                    if state.pane().selected_row[player_idx] != item_idx {
                        continue;
                    }
                    let Some((center_x, center_y, ring_w, ring_h)) = cursor_now(player_idx) else {
                        continue;
                    };

                    let left = center_x - ring_w * 0.5;
                    let right = center_x + ring_w * 0.5;
                    let top = center_y - ring_h * 0.5;
                    let bottom = center_y + ring_h * 0.5;
                    let mut ring_color = color::decorative_rgba(player_color_index(player_idx));
                    ring_color[3] *= a;
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy((left + right) * 0.5, top + border_w * 0.5):
                        zoomto(ring_w, border_w):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy((left + right) * 0.5, bottom - border_w * 0.5):
                        zoomto(ring_w, border_w):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(left + border_w * 0.5, (top + bottom) * 0.5):
                        zoomto(border_w, ring_h):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(right - border_w * 0.5, (top + bottom) * 0.5):
                        zoomto(border_w, ring_h):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                }
            }
            // Draw each option's text (active row: all white; inactive: #808080)
            if let Some((next_row_x, _, _)) = next_row_item {
                let next_row_color = if is_active {
                    [1.0, 1.0, 1.0, a]
                } else {
                    sl_gray
                };
                actors.push(act!(text: font("miso"): settext(ARCADE_NEXT_ROW_TEXT):
                    align(0.0, 0.5): xy(next_row_x, current_row_y): zoom(value_zoom):
                    diffuse(
                        next_row_color[0],
                        next_row_color[1],
                        next_row_color[2],
                        next_row_color[3]
                    ):
                    z(101)
                ));
            }
            for (idx, text) in row.choices.iter().enumerate() {
                let x = x_positions.get(idx).copied().unwrap_or(choice_inner_left);
                let color_rgba = if is_active {
                    [1.0, 1.0, 1.0, a]
                } else {
                    sl_gray
                };
                actors.push(act!(text: font("miso"): settext(text.clone()):
                    align(0.0, 0.5): xy(x, current_row_y): zoom(value_zoom):
                    diffuse(color_rgba[0], color_rgba[1], color_rgba[2], color_rgba[3]):
                    z(101)
                ));
            }
        } else {
            // Single value display (default behavior)
            // By default, align single-value choices to the same line as Speed Mod.
            // For Music Rate, center within the item column (to match SL parity).
            let primary_player_idx = if active[P1] { P1 } else { P2 };
            let mut choice_center_x = speed_mod_x;
            if row.id == RowId::MusicRate {
                let item_col_left = row_left + TITLE_BG_WIDTH;
                let item_col_w = row_width - TITLE_BG_WIDTH;
                choice_center_x = item_col_left + item_col_w * 0.5;
            } else if primary_player_idx == P2 {
                choice_center_x = screen_center_x().mul_add(2.0, -choice_center_x);
            }
            let choice_text_idx = row.selected_choice_index[primary_player_idx]
                .min(row.choices.len().saturating_sub(1));
            let choice_text = row
                .choices
                .get(choice_text_idx)
                .unwrap_or_else(|| row.choices.first().expect("OptionRow must have choices"));
            let choice_color = if is_active {
                [1.0, 1.0, 1.0, a]
            } else {
                sl_gray
            };
            asset_manager.with_fonts(|all_fonts| {
                asset_manager.with_font("miso", |metrics_font| {
                    let choice_display_text =
                        if arcade_row_focuses_next_row(state, primary_player_idx, item_idx) {
                            ARCADE_NEXT_ROW_TEXT.to_string()
                        } else if row.id == RowId::SpeedMod {
                            state.speed_mod[primary_player_idx].display()
                        } else {
                            choice_text.clone()
                        };
                    let mut text_w = crate::engine::present::font::measure_line_width_logical(
                        metrics_font,
                        &choice_display_text,
                        all_fonts,
                    ) as f32;
                    if !text_w.is_finite() || text_w <= 0.0 {
                        text_w = 1.0;
                    }
                    let text_h = (metrics_font.height as f32).max(1.0);
                    let value_zoom = 0.835;
                    let draw_w = text_w * value_zoom;
                    let draw_h = text_h * value_zoom;
                    actors.push(act!(text: font("miso"): settext(choice_display_text):
                        align(0.5, 0.5): xy(choice_center_x, current_row_y): zoom(value_zoom):
                        diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                        z(101)
                    ));
                    // Underline (always visible) — fixed pixel thickness for consistency
                    let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                    let underline_w = draw_w.ceil(); // pixel-align for crispness
                    let offset = widescale(3.0, 4.0); // place just under the baseline
                    let underline_y = current_row_y + draw_h * 0.5 + offset;
                    let underline_left_x = choice_center_x - draw_w * 0.5;
                    let mut line_color = color::decorative_rgba(player_color_index(primary_player_idx));
                    line_color[3] *= a;
                    actors.push(act!(quad:
                        align(0.0, 0.5): // start at text's left edge
                        xy(underline_left_x, underline_y):
                        zoomto(underline_w, line_thickness):
                        diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                        z(101)
                    ));
                    // Encircling cursor around the active option value (programmatic border)
                    if active[primary_player_idx] && state.pane().selected_row[primary_player_idx] == item_idx {
                        let border_w = widescale(2.0, 2.5);
                        if let Some((center_x, center_y, ring_w, ring_h)) =
                            cursor_now(primary_player_idx)
                        {
                            let left = center_x - ring_w * 0.5;
                            let right = center_x + ring_w * 0.5;
                            let top = center_y - ring_h * 0.5;
                            let bottom = center_y + ring_h * 0.5;
                            let mut ring_color =
                                color::decorative_rgba(player_color_index(primary_player_idx));
                            ring_color[3] *= a;
                            actors.push(act!(quad:
                                align(0.5, 0.5): xy(center_x, top + border_w * 0.5):
                                zoomto(ring_w, border_w):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                            actors.push(act!(quad:
                                align(0.5, 0.5): xy(center_x, bottom - border_w * 0.5):
                                zoomto(ring_w, border_w):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                            actors.push(act!(quad:
                                align(0.5, 0.5): xy(left + border_w * 0.5, center_y):
                                zoomto(border_w, ring_h):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                            actors.push(act!(quad:
                                align(0.5, 0.5): xy(right - border_w * 0.5, center_y):
                                zoomto(border_w, ring_h):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                        }
                    }
                    let p2_text = if show_p2 && row.id != RowId::MusicRate {
                        if arcade_row_focuses_next_row(state, P2, item_idx) {
                            ARCADE_NEXT_ROW_TEXT.to_string()
                        } else if row.id == RowId::SpeedMod {
                            state.speed_mod[P2].display()
                        } else if row.id == RowId::TypeOfSpeedMod {
                            let idx = state.speed_mod[P2].mod_type.choice_index();
                            row.choices.get(idx).cloned().unwrap_or_default()
                        } else {
                            let idx = row
                                .selected_choice_index[P2]
                                .min(row.choices.len().saturating_sub(1));
                            row.choices.get(idx).cloned().unwrap_or_default()
                        }
                    } else {
                        String::new()
                    };
                    if show_p2 && row.id != RowId::MusicRate {
                        let p2_choice_center_x = screen_center_x().mul_add(2.0, -choice_center_x);
                        let mut p2_w = crate::engine::present::font::measure_line_width_logical(
                            metrics_font,
                            &p2_text,
                            all_fonts,
                        ) as f32;
                        if !p2_w.is_finite() || p2_w <= 0.0 {
                            p2_w = 1.0;
                        }
                        let p2_draw_w = p2_w * value_zoom;
                        actors.push(act!(text: font("miso"): settext(p2_text.clone()):
                            align(0.5, 0.5): xy(p2_choice_center_x, current_row_y): zoom(value_zoom):
                            diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                            z(101)
                        ));
                        let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                        let underline_w = p2_draw_w.ceil();
                        let offset = widescale(3.0, 4.0);
                        let underline_y = current_row_y + draw_h * 0.5 + offset;
                        let underline_left_x = p2_choice_center_x - p2_draw_w * 0.5;
                        let mut line_color = color::decorative_rgba(player_color_index(P2));
                        line_color[3] *= a;
                        actors.push(act!(quad:
                            align(0.0, 0.5):
                            xy(underline_left_x, underline_y):
                            zoomto(underline_w, line_thickness):
                            diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                            z(101)
                        ));
                        if active[P2] && state.pane().selected_row[P2] == item_idx {
                            let border_w = widescale(2.0, 2.5);
                            if let Some((center_x, center_y, ring_w, ring_h)) = cursor_now(P2) {
                                let left = center_x - ring_w * 0.5;
                                let right = center_x + ring_w * 0.5;
                                let top = center_y - ring_h * 0.5;
                                let bottom = center_y + ring_h * 0.5;
                                let mut ring_color = color::decorative_rgba(player_color_index(P2));
                                ring_color[3] *= a;
                                actors.push(act!(quad:
                                    align(0.5, 0.5): xy(center_x, top + border_w * 0.5):
                                    zoomto(ring_w, border_w):
                                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                    z(101)
                                ));
                                actors.push(act!(quad:
                                    align(0.5, 0.5): xy(center_x, bottom - border_w * 0.5):
                                    zoomto(ring_w, border_w):
                                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                    z(101)
                                ));
                                actors.push(act!(quad:
                                    align(0.5, 0.5): xy(left + border_w * 0.5, center_y):
                                    zoomto(border_w, ring_h):
                                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                    z(101)
                                ));
                                actors.push(act!(quad:
                                    align(0.5, 0.5): xy(right - border_w * 0.5, center_y):
                                    zoomto(border_w, ring_h):
                                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                    z(101)
                                ));
                            }
                        }
                    }
                    // Add previews for the selected value on each side.
                    if row.id == RowId::JudgmentFont {
                        let texture_for = |player_idx: usize| -> Option<&str> {
                            assets::judgment_texture_choices()
                                .get(row.selected_choice_index[player_idx])
                                .and_then(|choice| {
                                    if choice.key.eq_ignore_ascii_case("None") {
                                        None
                                    } else {
                                        assets::resolve_texture_choice(
                                            Some(choice.key.as_str()),
                                            assets::judgment_texture_choices(),
                                        )
                                    }
                                })
                        };
                        if let Some(texture) = texture_for(primary_player_idx) {
                            actors.push(act!(sprite(texture):
                                align(0.5, 0.5):
                                xy(preview_x_for(primary_player_idx), current_row_y):
                                setstate(0):
                                zoom(0.225):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                        }
                        if show_p2
                            && primary_player_idx != P2
                            && let Some(texture) = texture_for(P2)
                        {
                            actors.push(act!(sprite(texture):
                                align(0.5, 0.5):
                                xy(preview_x_for(P2), current_row_y):
                                setstate(0):
                                zoom(0.225):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                        }
                    }
                    // Add hold judgment preview for "Hold Judgment" row showing both frames (Held and Let Go)
                    if row.id == RowId::HoldJudgment {
                        let texture_for = |player_idx: usize| -> Option<&str> {
                            assets::hold_judgment_texture_choices()
                                .get(row.selected_choice_index[player_idx])
                                .and_then(|choice| {
                                    if choice.key.eq_ignore_ascii_case("None") {
                                        None
                                    } else {
                                        assets::resolve_texture_choice(
                                            Some(choice.key.as_str()),
                                            assets::hold_judgment_texture_choices(),
                                        )
                                    }
                                })
                        };
                        let draw_hold_preview = |texture: &str, center_x: f32, actors: &mut Vec<Actor>| {
                            let zoom = 0.225;
                            let tex_w = crate::assets::texture_dims(texture)
                                .map_or(128.0, |meta| meta.w.max(1) as f32);
                            let center_offset = tex_w * zoom * 0.4;

                            actors.push(act!(sprite(texture):
                                align(0.5, 0.5):
                                xy(center_x - center_offset, current_row_y):
                                setstate(0):
                                zoom(zoom):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                            actors.push(act!(sprite(texture):
                                align(0.5, 0.5):
                                xy(center_x + center_offset, current_row_y):
                                setstate(1):
                                zoom(zoom):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                        };
                        if let Some(texture) = texture_for(primary_player_idx) {
                            draw_hold_preview(texture, preview_x_for(primary_player_idx), &mut actors);
                        }
                        if show_p2
                            && primary_player_idx != P2
                            && let Some(texture) = texture_for(P2)
                        {
                            draw_hold_preview(texture, preview_x_for(P2), &mut actors);
                        }
                    }
                    // Match ITGmania themes that show four directional noteskin preview arrows
                    // with explicit quant offsets: Left/Down/Up/Right and 0/1/3/2 quant indices.
                    if row.id == RowId::NoteSkin
                        || row.id == RowId::MineSkin
                        || row.id == RowId::ReceptorSkin
                        || row.id == RowId::TapExplosionSkin
                    {
                        const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0;
                        const PREVIEW_SCALE: f32 = 0.45;
                        const PREVIEW_ARROWS: [(usize, f32, f32); 4] = [
                            (0, 0.0, -1.5),
                            (1, 1.0, -0.5),
                            (2, 3.0, 0.5),
                            (3, 2.0, 1.5),
                        ];
                        let draw_noteskin_note =
                            |ns: &Noteskin,
                             note_idx: usize,
                             quant_idx: f32,
                             center_x: f32,
                             actors: &mut Vec<Actor>| {
                                let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                                let elapsed = state.preview_time;
                                let beat = state.preview_beat;
                                let note_uv_phase = ns.tap_note_uv_phase(elapsed, beat, 0.0);
                                let tap_spacing = ns.note_display_metrics.part_texture_translate
                                    [NoteAnimPart::Tap as usize]
                                    .note_color_spacing;
                                let uv_translate =
                                    [tap_spacing[0] * quant_idx, tap_spacing[1] * quant_idx];
                                if let Some(note_slots) = ns.note_layers.get(note_idx) {
                                    let primary_h = note_slots
                                        .first()
                                        .map(|slot| slot.logical_size()[1].max(1.0))
                                        .unwrap_or(1.0);
                                    let note_scale = if primary_h > f32::EPSILON {
                                        target_height / primary_h
                                    } else {
                                        PREVIEW_SCALE
                                    };
                                    for (layer_idx, note_slot) in note_slots.iter().enumerate() {
                                        let draw = note_slot.model_draw_at(elapsed, beat);
                                        if !draw.visible {
                                            continue;
                                        }
                                        let frame = note_slot.frame_index(elapsed, beat);
                                        let uv_elapsed = if note_slot.model.is_some() {
                                            note_uv_phase
                                        } else {
                                            elapsed
                                        };
                                        let uv = note_slot.uv_for_frame_at(frame, uv_elapsed);
                                        let uv = [
                                            uv[0] + uv_translate[0],
                                            uv[1] + uv_translate[1],
                                            uv[2] + uv_translate[0],
                                            uv[3] + uv_translate[1],
                                        ];
                                        let slot_size = note_slot.logical_size();
                                        let base_size = [slot_size[0] * note_scale, slot_size[1] * note_scale];
                                        let rot_rad = (-note_slot.def.rotation_deg as f32).to_radians();
                                        let (sin_r, cos_r) = rot_rad.sin_cos();
                                        let ox = draw.pos[0] * note_scale;
                                        let oy = draw.pos[1] * note_scale;
                                        let center = [
                                            center_x + ox * cos_r - oy * sin_r,
                                            current_row_y + ox * sin_r + oy * cos_r,
                                        ];
                                        let size = [
                                            base_size[0] * draw.zoom[0].max(0.0),
                                            base_size[1] * draw.zoom[1].max(0.0),
                                        ];
                                        if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
                                            continue;
                                        }
                                        let color = [draw.tint[0], draw.tint[1], draw.tint[2], draw.tint[3] * a];
                                        let blend = if draw.blend_add {
                                            BlendMode::Add
                                        } else {
                                            BlendMode::Alpha
                                        };
                                        let z = 102 + layer_idx as i32;
                                        if let Some(model_actor) = noteskin_model_actor(
                                            note_slot,
                                            center,
                                            size,
                                            uv,
                                            -note_slot.def.rotation_deg as f32,
                                            elapsed,
                                            beat,
                                            color,
                                            blend,
                                            z as i16,
                                        ) {
                                            actors.push(model_actor);
                                        } else if draw.blend_add {
                                            actors.push(act!(sprite(note_slot.texture_key_shared()):
                                                align(0.5, 0.5):
                                                xy(center[0], center[1]):
                                                setsize(size[0], size[1]):
                                                rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32):
                                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                                diffuse(color[0], color[1], color[2], color[3]):
                                                blend(add):
                                                z(z)
                                            ));
                                        } else {
                                            actors.push(act!(sprite(note_slot.texture_key_shared()):
                                                align(0.5, 0.5):
                                                xy(center[0], center[1]):
                                                setsize(size[0], size[1]):
                                                rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32):
                                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                                diffuse(color[0], color[1], color[2], color[3]):
                                                blend(normal):
                                                z(z)
                                            ));
                                        }
                                    }
                                    return;
                                }
                                let Some(note_slot) = ns.notes.get(note_idx) else {
                                    return;
                                };
                                let frame = note_slot.frame_index(elapsed, beat);
                                let uv_elapsed = if note_slot.model.is_some() {
                                    note_uv_phase
                                } else {
                                    elapsed
                                };
                                let uv = note_slot.uv_for_frame_at(frame, uv_elapsed);
                                let uv = [
                                    uv[0] + uv_translate[0],
                                    uv[1] + uv_translate[1],
                                    uv[2] + uv_translate[0],
                                    uv[3] + uv_translate[1],
                                ];
                                let size_raw = note_slot.logical_size();
                                let width = size_raw[0].max(1.0);
                                let height = size_raw[1].max(1.0);
                                let scale = if height > 0.0 {
                                    target_height / height
                                } else {
                                    PREVIEW_SCALE
                                };
                                let size = [width * scale, target_height];
                                let center = [center_x, current_row_y];
                                if let Some(model_actor) = noteskin_model_actor(
                                    note_slot,
                                    center,
                                    size,
                                    uv,
                                    -note_slot.def.rotation_deg as f32,
                                    elapsed,
                                    beat,
                                    [1.0, 1.0, 1.0, a],
                                    BlendMode::Alpha,
                                    102,
                                ) {
                                    actors.push(model_actor);
                                } else {
                                    actors.push(act!(sprite(note_slot.texture_key_shared()):
                                        align(0.5, 0.5):
                                        xy(center[0], center[1]):
                                        setsize(size[0], size[1]):
                                        rotationz(-note_slot.def.rotation_deg as f32):
                                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                        diffuse(1.0, 1.0, 1.0, a):
                                        z(102)
                                    ));
                                }
                            };
                        let draw_noteskin_preview =
                            |ns: &Noteskin, center_x: f32, actors: &mut Vec<Actor>| {
                                let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                                for (col, quant_idx, x_mult) in PREVIEW_ARROWS {
                                    let x = center_x + x_mult * target_height;
                                    let note_idx =
                                        col * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
                                    draw_noteskin_note(ns, note_idx, quant_idx, x, actors);
                                }
                            };
                        let draw_mine_preview =
                            |mine_ns: &Noteskin, center_x: f32, actors: &mut Vec<Actor>| {
                                let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                                let mine_col = if mine_ns.mines.len() > 1 || mine_ns.mine_frames.len() > 1 {
                                    1
                                } else {
                                    0
                                };
                                let fill_slot =
                                    mine_ns.mines.get(mine_col).and_then(|slot| slot.as_ref());
                                let frame_slot = mine_ns
                                    .mine_frames
                                    .get(mine_col)
                                    .and_then(|slot| slot.as_ref());
                                let Some(primary_slot) = frame_slot.or(fill_slot) else {
                                    return;
                                };
                                let mine_phase =
                                    mine_ns.tap_mine_uv_phase(state.preview_time, state.preview_beat, 0.0);
                                let mine_translation =
                                    mine_ns.part_uv_translation(NoteAnimPart::Mine, 0.0, false);
                                let mine_center = [center_x, current_row_y];
                                let scale_mine_slot = |slot: &SpriteSlot| {
                                    let size = slot
                                        .model
                                        .as_ref()
                                        .map(|model| model.size())
                                        .unwrap_or_else(|| {
                                            let logical = slot.logical_size();
                                            [logical[0], logical[1]]
                                        });
                                    let width = size[0].max(1.0);
                                    let height = size[1].max(1.0);
                                    let scale = target_height / height;
                                    [width * scale, target_height]
                                };
                                let draw_mine_slot =
                                    |slot: &SpriteSlot, alpha: f32, z: i32, actors: &mut Vec<Actor>| {
                                        let draw = slot.model_draw_at(state.preview_time, state.preview_beat);
                                        if !draw.visible {
                                            return;
                                        }
                                        let frame = slot.frame_index_from_phase(mine_phase);
                                        let uv_elapsed = if slot.model.is_some() {
                                            mine_phase
                                        } else {
                                            state.preview_time
                                        };
                                        let uv = slot.uv_for_frame_at(frame, uv_elapsed);
                                        let uv = [
                                            uv[0] + mine_translation[0],
                                            uv[1] + mine_translation[1],
                                            uv[2] + mine_translation[0],
                                            uv[3] + mine_translation[1],
                                        ];
                                        let size = scale_mine_slot(slot);
                                        if let Some(model_actor) = noteskin_model_actor(
                                            slot,
                                            mine_center,
                                            size,
                                            uv,
                                            -slot.def.rotation_deg as f32,
                                            state.preview_time,
                                            state.preview_beat,
                                            [1.0, 1.0, 1.0, alpha],
                                            BlendMode::Alpha,
                                            z as i16,
                                        ) {
                                            actors.push(model_actor);
                                        } else {
                                            actors.push(act!(sprite(slot.texture_key_shared()):
                                                align(0.5, 0.5):
                                                xy(mine_center[0], mine_center[1]):
                                                setsize(size[0], size[1]):
                                                rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                                diffuse(1.0, 1.0, 1.0, alpha):
                                                z(z)
                                            ));
                                        }
                                    };
                                if let Some(slot) = fill_slot {
                                    draw_mine_slot(slot, 0.85 * a, 106, actors);
                                }
                                if let Some(slot) = frame_slot {
                                    draw_mine_slot(slot, a, 107, actors);
                                } else if fill_slot.is_none() {
                                    draw_mine_slot(primary_slot, a, 107, actors);
                                }
                            };
                        let draw_receptor_preview =
                            |receptor_ns: &Noteskin, center_x: f32, actors: &mut Vec<Actor>| {
                                let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                                let receptor_color =
                                    receptor_ns.receptor_pulse.color_for_beat(state.preview_beat);
                                let color = [
                                    receptor_color[0],
                                    receptor_color[1],
                                    receptor_color[2],
                                    receptor_color[3] * a,
                                ];
                                for (col, _, x_mult) in PREVIEW_ARROWS {
                                    let Some(receptor_slot) = receptor_ns.receptor_off.get(col) else {
                                        continue;
                                    };
                                    let frame = receptor_slot
                                        .frame_index(state.preview_time, state.preview_beat);
                                    let uv = receptor_slot
                                        .uv_for_frame_at(frame, state.preview_time);
                                    let logical = receptor_slot.logical_size();
                                    let width = logical[0].max(1.0);
                                    let height = logical[1].max(1.0);
                                    let scale = if height > f32::EPSILON {
                                        target_height / height
                                    } else {
                                        PREVIEW_SCALE
                                    };
                                    let size = [width * scale, target_height];
                                    let center = [center_x + x_mult * target_height, current_row_y];
                                    if let Some(model_actor) = noteskin_model_actor(
                                        receptor_slot,
                                        center,
                                        size,
                                        uv,
                                        -receptor_slot.def.rotation_deg as f32,
                                        state.preview_time,
                                        state.preview_beat,
                                        color,
                                        BlendMode::Alpha,
                                        106,
                                    ) {
                                        actors.push(model_actor);
                                    } else {
                                        actors.push(act!(sprite(receptor_slot.texture_key_shared()):
                                            align(0.5, 0.5):
                                            xy(center[0], center[1]):
                                            setsize(size[0], size[1]):
                                            rotationz(-receptor_slot.def.rotation_deg as f32):
                                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                            diffuse(color[0], color[1], color[2], color[3]):
                                            z(106)
                                        ));
                                    }
                                }
                            };
                        let draw_tap_explosion_preview = |explosion_ns: &Noteskin,
                                                          receptor_ns: &Noteskin,
                                                          center_x: f32,
                                                          actors: &mut Vec<Actor>| {
                            let preview_time = state.preview_time * TAP_EXPLOSION_PREVIEW_SPEED;
                            let preview_beat = state.preview_beat * TAP_EXPLOSION_PREVIEW_SPEED;
                            let Some(explosion) = explosion_ns
                                .tap_explosions
                                .get("W1")
                                .or_else(|| explosion_ns.tap_explosions.values().next())
                            else {
                                return;
                            };
                            let duration = explosion.animation.duration();
                            let anim_time = if duration > f32::EPSILON {
                                preview_time.rem_euclid(duration)
                            } else {
                                0.0
                            };
                            let explosion_visual = explosion.animation.state_at(anim_time);
                            if !explosion_visual.visible {
                                return;
                            }
                            let slot = &explosion.slot;
                            let beat_for_anim = if slot.source.is_beat_based() {
                                anim_time.max(0.0)
                            } else {
                                preview_beat
                            };
                            let frame = slot.frame_index(anim_time, beat_for_anim);
                            let uv_elapsed = if slot.model.is_some() {
                                anim_time
                            } else {
                                preview_time
                            };
                            let uv = slot.uv_for_frame_at(frame, uv_elapsed);
                            let logical = slot.logical_size();
                            let width = logical[0].max(1.0);
                            let height = logical[1].max(1.0);
                            let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                            let scale = if height > f32::EPSILON {
                                target_height / height
                            } else {
                                PREVIEW_SCALE
                            };
                            let size = [width * scale, target_height];
                            let rotation_deg = receptor_ns
                                .receptor_off
                                .first()
                                .map(|slot| slot.def.rotation_deg as f32)
                                .unwrap_or(0.0);
                            let color = [
                                explosion_visual.diffuse[0],
                                explosion_visual.diffuse[1],
                                explosion_visual.diffuse[2],
                                explosion_visual.diffuse[3] * a,
                            ];
                            let blend = if explosion.animation.blend_add {
                                BlendMode::Add
                            } else {
                                BlendMode::Alpha
                            };
                            if let Some(model_actor) = noteskin_model_actor(
                                slot,
                                [center_x, current_row_y],
                                [
                                    size[0] * explosion_visual.zoom.max(0.0),
                                    size[1] * explosion_visual.zoom.max(0.0),
                                ],
                                uv,
                                -rotation_deg,
                                anim_time,
                                beat_for_anim,
                                color,
                                blend,
                                107,
                            ) {
                                actors.push(model_actor);
                            } else if matches!(blend, BlendMode::Add) {
                                actors.push(act!(sprite(slot.texture_key_shared()):
                                    align(0.5, 0.5):
                                    xy(center_x, current_row_y):
                                    setsize(size[0], size[1]):
                                    zoom(explosion_visual.zoom):
                                    rotationz(-rotation_deg):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(add):
                                    z(107)
                                ));
                            } else {
                                actors.push(act!(sprite(slot.texture_key_shared()):
                                    align(0.5, 0.5):
                                    xy(center_x, current_row_y):
                                    setsize(size[0], size[1]):
                                    zoom(explosion_visual.zoom):
                                    rotationz(-rotation_deg):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(normal):
                                    z(107)
                                ));
                            }
                        };
                        if row.id == RowId::NoteSkin {
                            if let Some(ns) = state.noteskin[primary_player_idx].as_ref() {
                                draw_noteskin_preview(
                                    ns,
                                    preview_x_for(primary_player_idx),
                                    &mut actors,
                                );
                            }
                            if show_p2 && primary_player_idx != P2
                                && let Some(ns) = state.noteskin[P2].as_ref()
                            {
                                draw_noteskin_preview(ns, preview_x_for(P2), &mut actors);
                            }
                        } else if row.id == RowId::MineSkin {
                            if let Some(mine_ns) = state.mine_noteskin[primary_player_idx]
                                .as_deref()
                                .or_else(|| state.noteskin[primary_player_idx].as_deref())
                            {
                                draw_mine_preview(
                                    mine_ns,
                                    preview_x_for(primary_player_idx),
                                    &mut actors,
                                );
                            }
                            if show_p2 && primary_player_idx != P2
                                && let Some(mine_ns) = state.mine_noteskin[P2]
                                    .as_deref()
                                    .or_else(|| state.noteskin[P2].as_deref())
                            {
                                draw_mine_preview(mine_ns, preview_x_for(P2), &mut actors);
                            }
                        } else if row.id == RowId::ReceptorSkin {
                            if let Some(receptor_ns) = state.receptor_noteskin[primary_player_idx]
                                .as_deref()
                                .or_else(|| state.noteskin[primary_player_idx].as_deref())
                            {
                                draw_receptor_preview(
                                    receptor_ns,
                                    preview_x_for(primary_player_idx),
                                    &mut actors,
                                );
                            }
                            if show_p2
                                && primary_player_idx != P2
                                && let Some(receptor_ns) = state.receptor_noteskin[P2]
                                    .as_deref()
                                    .or_else(|| state.noteskin[P2].as_deref())
                            {
                                draw_receptor_preview(receptor_ns, preview_x_for(P2), &mut actors);
                            }
                        } else if row.id == RowId::TapExplosionSkin {
                            if !state.player_profiles[primary_player_idx]
                                .tap_explosion_noteskin_hidden()
                                && let Some(explosion_ns) = state.tap_explosion_noteskin
                                    [primary_player_idx]
                                    .as_deref()
                                    .or_else(|| state.noteskin[primary_player_idx].as_deref())
                            {
                                let receptor_ns = state.receptor_noteskin[primary_player_idx]
                                    .as_deref()
                                    .or_else(|| state.noteskin[primary_player_idx].as_deref())
                                    .unwrap_or(explosion_ns);
                                draw_tap_explosion_preview(
                                    explosion_ns,
                                    receptor_ns,
                                    preview_x_for(primary_player_idx),
                                    &mut actors,
                                );
                            }
                            if show_p2
                                && primary_player_idx != P2
                                && !state.player_profiles[P2].tap_explosion_noteskin_hidden()
                                && let Some(explosion_ns) = state.tap_explosion_noteskin[P2]
                                    .as_deref()
                                    .or_else(|| state.noteskin[P2].as_deref())
                            {
                                let receptor_ns = state.receptor_noteskin[P2]
                                    .as_deref()
                                    .or_else(|| state.noteskin[P2].as_deref())
                                    .unwrap_or(explosion_ns);
                                draw_tap_explosion_preview(
                                    explosion_ns,
                                    receptor_ns,
                                    preview_x_for(P2),
                                    &mut actors,
                                );
                            }
                        }
                    }
                    // Add combo preview for "Combo Font" row showing ticking numbers
                    if row.id == RowId::ComboFont {
                        let combo_text = state.combo_preview_count.to_string();
                        let combo_zoom = 0.45;
                        // Choice indices are fixed by construction order:
                        // 0=Wendy, 1=ArialRounded, 2=Asap, 3=BebasNeue, 4=SourceCode,
                        // 5=Work, 6=WendyCursed, 7=None
                        let combo_font_for = |idx: usize| -> Option<&'static str> {
                            match idx {
                            0 => Some("wendy_combo"),
                            1 => Some("combo_arial_rounded"),
                            2 => Some("combo_asap"),
                            3 => Some("combo_bebas_neue"),
                            4 => Some("combo_source_code"),
                            5 => Some("combo_work"),
                            6 => Some("combo_wendy_cursed"),
                            _ => None,
                            }
                        };
                        let p1_choice_idx = row.selected_choice_index[primary_player_idx]
                            .min(row.choices.len().saturating_sub(1));
                        if let Some(font_name) = combo_font_for(p1_choice_idx) {
                            actors.push(act!(text:
                                font(font_name): settext(combo_text.clone()):
                                align(0.5, 0.5):
                                xy(preview_x_for(primary_player_idx), current_row_y):
                                zoom(combo_zoom): horizalign(center):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                        }
                        if show_p2 && primary_player_idx != P2 {
                            let p2_choice_idx = row.selected_choice_index[P2]
                                .min(row.choices.len().saturating_sub(1));
                            if let Some(font_name) = combo_font_for(p2_choice_idx) {
                            actors.push(act!(text:
                                font(font_name): settext(combo_text):
                                align(0.5, 0.5):
                                xy(preview_x_for(P2), current_row_y):
                                zoom(combo_zoom): horizalign(center):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                            }
                        }
                    }
                });
            });
        }
    }
    // ------------------- Description content (selected) -------------------
    actors.push(act!(quad:
        align(0.0, 1.0): xy(help_box_x, help_box_bottom_y):
        zoomto(help_box_w, help_box_h):
        diffuse(0.0, 0.0, 0.0, 0.8 * pane_alpha)
    ));
    const REVEAL_DURATION: f32 = 0.5;
    let split_help = active[P1] && active[P2];
    for player_idx in active_player_indices(active) {
        let row_idx =
            state.pane().selected_row[player_idx].min(state.pane().row_map.len().saturating_sub(1));
        let Some(row) = state
            .pane()
            .row_map
            .display_order()
            .get(row_idx)
            .and_then(|&id| state.pane().row_map.get(id))
        else {
            continue;
        };
        let help_text_color = color::simply_love_rgba(player_color_index(player_idx));
        let wrap_width = if split_help || player_idx == P2 {
            (help_box_w * 0.5) - 30.0
        } else {
            help_box_w - 30.0
        };
        let help_x = if split_help {
            (player_idx as f32).mul_add(help_box_w * 0.5, help_box_x + 12.0)
        } else if player_idx == P2 {
            help_box_x + help_box_w * 0.5 + 12.0
        } else {
            help_box_x + 12.0
        };

        let num_help_lines = row.help.len().max(1);
        let time_per_line = REVEAL_DURATION / num_help_lines as f32;

        if row.help.len() > 1 {
            let line_spacing = 12.0;
            let total_height = (row.help.len() as f32 - 1.0) * line_spacing;
            let start_y = help_box_bottom_y - (help_box_h * 0.5) - (total_height * 0.5);

            for (i, help_line) in row.help.iter().enumerate() {
                let start_time = i as f32 * time_per_line;
                let end_time = start_time + time_per_line;
                let anim_time = state.help_anim_time[player_idx];
                let visible_chars = if anim_time < start_time {
                    0
                } else if anim_time >= end_time {
                    help_line.chars().count()
                } else {
                    let line_fraction = (anim_time - start_time) / time_per_line;
                    let char_count = help_line.chars().count();
                    ((char_count as f32 * line_fraction).round() as usize).min(char_count)
                };
                let visible_text: String = help_line.chars().take(visible_chars).collect();

                let line_y = (i as f32).mul_add(line_spacing, start_y);
                actors.push(act!(text:
                    font("miso"): settext(visible_text):
                    align(0.0, 0.5):
                    xy(help_x, line_y):
                    zoom(0.825):
                    diffuse(help_text_color[0], help_text_color[1], help_text_color[2], pane_alpha):
                    maxwidth(wrap_width): horizalign(left):
                    z(101)
                ));
            }
        } else {
            let help_text = row.help.join(" | ");
            let char_count = help_text.chars().count();
            let fraction = (state.help_anim_time[player_idx] / REVEAL_DURATION).clamp(0.0, 1.0);
            let visible_chars = ((char_count as f32 * fraction).round() as usize).min(char_count);
            let visible_text: String = help_text.chars().take(visible_chars).collect();

            actors.push(act!(text:
                font("miso"): settext(visible_text):
                align(0.0, 0.5):
                xy(help_x, help_box_bottom_y - (help_box_h * 0.5)):
                zoom(0.825):
                diffuse(help_text_color[0], help_text_color[1], help_text_color[2], pane_alpha):
                maxwidth(wrap_width): horizalign(left):
                z(101)
            ));
        }
    }
    actors
}
