use super::*;
use crate::fonts::machine_font_key;
use deadlib_present::actors::TextContent;
use deadsync_notefield::noteskin_model_actor_from_draw;
use deadsync_theme::FontRole;

pub(super) fn top_bar_actor(
    state: &State,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) -> Actor {
    let i18n_revision = crate::i18n::revision();
    let screen_size_bits = [screen_width().to_bits(), screen_height().to_bits()];
    {
        let cache = state.top_bar_cache.borrow();
        if let Some(cached) = cache.as_ref()
            && cached.i18n_revision == i18n_revision
            && cached.visual_policy == visual_policy
            && cached.screen_size_bits == screen_size_bits
        {
            return cached.actor.clone();
        }
    }
    let title = tr("ScreenTitles", "SelectModifiers");
    let actor = screen_bar::build(ScreenBarParams {
        visual_policy,
        title: &title,
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
    });
    *state.top_bar_cache.borrow_mut() = Some(PlayerOptionsTopBarCache {
        i18n_revision,
        visual_policy,
        screen_size_bits,
        actor: actor.clone(),
    });
    actor
}

pub(super) fn compile_row_titles(row_map: &RowMap, i18n_revision: u64) -> PlayerOptionsRowTitles {
    PlayerOptionsRowTitles {
        i18n_revision,
        titles: row_map
            .display_order()
            .iter()
            .map(|&id| row_title_content(row_map.row(id).name.get()))
            .collect(),
    }
}

pub(super) fn row_title_content(text: Arc<str>) -> TextContent {
    TextContent::inline_str(&text).unwrap_or(TextContent::Shared(text))
}

pub(super) fn prepare_row_titles(state: &mut State) {
    let pane_idx = state.current_pane.index();
    let i18n_revision = crate::i18n::revision();
    if state.row_titles[pane_idx].i18n_revision == i18n_revision {
        return;
    }
    state.row_titles[pane_idx] = compile_row_titles(&state.panes[pane_idx].row_map, i18n_revision);
}

pub(super) fn prepare_speed_values(state: &mut State, asset_manager: &AssetManager) {
    let mut dirty = std::mem::take(&mut state.speed_value_dirty);
    while dirty != 0 {
        let player_idx = dirty.trailing_zeros() as usize;
        dirty &= dirty - 1;
        let speed_mod = &state.speed_mod[player_idx];
        let text = speed_value_content(speed_mod);
        let (value_draw_width, value_draw_height) =
            measure_option_text(asset_manager, text.as_str(), INLINE_CHOICE_VALUE_ZOOM);
        state.speed_values[player_idx] = Some(SpeedValuePresentation {
            text,
            value_draw_width,
            value_draw_height,
        });
    }
}

pub(super) fn speed_value_content(speed_mod: &SpeedMod) -> TextContent {
    let inline = match speed_mod.mod_type {
        SpeedModType::X => TextContent::inline_format(format_args!("{:.2}x", speed_mod.value)),
        SpeedModType::C => TextContent::inline_format(format_args!("C{}", speed_mod.value as i32)),
        SpeedModType::M => TextContent::inline_format(format_args!("M{}", speed_mod.value as i32)),
    };
    inline.unwrap_or_else(|| TextContent::Shared(Arc::from(speed_mod.display())))
}

pub(super) fn mark_speed_value_dirty(state: &mut State, player_idx: usize) {
    let bit = 1 << player_idx.min(PLAYER_SLOTS - 1);
    state.speed_value_dirty |= bit;
    state.speed_header_dirty |= bit;
}

pub(super) const fn speed_value(state: &State, player_idx: usize) -> &SpeedValuePresentation {
    state.speed_values[player_idx]
        .as_ref()
        .expect("Player Options speed presentation must be prepared")
}

pub(super) fn prepare_speed_headers(state: &mut State, asset_manager: &AssetManager) {
    let mut dirty = std::mem::take(&mut state.speed_header_dirty);
    while dirty != 0 {
        let player_idx = dirty.trailing_zeros() as usize;
        dirty &= dirty - 1;
        let speed_mod = &state.speed_mod[player_idx];
        let chart = resolve_p1_chart(&state.song, &state.chart_steps_index, state.play_style);
        let main_bpm = speed_helper_bpm(&state.song, chart, speed_mod, state.music_rate);
        let scaled_bpm = scaled_speed_helper_bpm(
            &state.song,
            chart,
            speed_mod,
            state.music_rate,
            &state.player_options[player_idx],
        );
        let prefix = speed_mod.mod_type.prefix();
        let main = speed_header_content(prefix, main_bpm);
        let scaled = (scaled_bpm != main_bpm).then(|| speed_header_content(prefix, scaled_bpm));
        let main_draw_width =
            measure_header_text_width(asset_manager, main.as_str(), state.policy.machine_font);
        state.speed_headers[player_idx] = Some(SpeedHeaderPresentation {
            main,
            scaled,
            main_draw_width,
        });
    }
}

pub(super) fn speed_header_content(prefix: &'static str, bpm: Option<[i32; 2]>) -> TextContent {
    let Some([lo, hi]) = bpm else {
        return TextContent::Static(prefix);
    };
    if lo == hi {
        TextContent::inline_format(format_args!("{prefix}{lo}"))
            .unwrap_or_else(|| TextContent::Shared(Arc::from(format!("{prefix}{lo}"))))
    } else {
        TextContent::inline_format(format_args!("{prefix}{lo}-{hi}"))
            .unwrap_or_else(|| TextContent::Shared(Arc::from(format!("{prefix}{lo}-{hi}"))))
    }
}

pub(super) fn mark_speed_header_dirty(state: &mut State, player_idx: usize) {
    state.speed_header_dirty |= 1 << player_idx.min(PLAYER_SLOTS - 1);
}

pub(super) const fn mark_music_rate_dirty(state: &mut State) {
    state.speed_header_dirty = ALL_PLAYER_BITS;
    state.music_rate_text_dirty = true;
}

pub(super) const fn speed_header(state: &State, player_idx: usize) -> &SpeedHeaderPresentation {
    state.speed_headers[player_idx]
        .as_ref()
        .expect("Player Options speed header must be prepared")
}

pub(super) fn prepare_music_rate_text(state: &mut State) {
    if !std::mem::take(&mut state.music_rate_text_dirty) {
        return;
    }
    let display = music_rate_display_name(state);
    let mut lines = display.split('\n');
    let first_line = lines.next().unwrap_or_default();
    let second_line = lines.next();
    let third_line = lines.next();
    let (first, second) = if let (Some(second), None) = (second_line, third_line) {
        (
            music_rate_line_content(first_line),
            Some(music_rate_line_content(second)),
        )
    } else {
        (music_rate_line_content(&display), None)
    };
    state.music_rate_text = Some(MusicRatePresentation { first, second });
}

pub(super) fn music_rate_line_content(text: &str) -> TextContent {
    TextContent::inline_str(text).unwrap_or_else(|| TextContent::Shared(Arc::from(text)))
}

pub(super) const fn music_rate_text(state: &State) -> &MusicRatePresentation {
    state
        .music_rate_text
        .as_ref()
        .expect("Player Options Music Rate title must be prepared")
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    actors.reserve(64);
    let active = state.active;
    let show_p2 = active[P1] && active[P2];
    let pane_alpha = state.pane_transition.alpha();
    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
            visual_policy,
        },
    );
    actors.push(top_bar_actor(state, visual_policy));

    // Discoverability hint: the top bar only renders its title, so add a
    // right-aligned note telling players they can open the fuzzy setting search.
    // Only shown when keyboard features are enabled (the search is keyboard-only)
    // and hidden while the search overlay itself is open (it has its own footer).
    if state.policy.keyboard_features && !state.search.is_open() {
        let search_hint = tr("PlayerOptions", "SettingSearchHint").to_string();
        actors.push(act!(text:
            font("miso"): settext(search_hint):
            align(1.0, 0.5): xy(deadlib_present::space::screen_width() - 10.0, 16.0):
            zoom(0.72): z(130):
            diffuse(1.0, 1.0, 1.0, 1.0): horizalign(right)
        ));
    }

    // zmod ScreenPlayerOptions overlay/default.lua speed helper parity.
    let speed_mod_y = 48.0;
    let speed_mod_zoom = 0.5_f32;
    let speed_mod_scaled_y = 52.0_f32;
    let speed_mod_scaled_zoom = 0.3_f32;
    let speed_mod_x_p1 = player_option_column_x(P1);
    let speed_mod_x_p2 = player_option_column_x(P2);
    let speed_mod_x = speed_mod_x_p1;
    // All previews (judgment, hold, noteskin, combo) share this center line.
    // Tweak these to dial in parity with Simply Love.
    const PREVIEW_CENTER_OFFSET_NORMAL: f32 = 80.75; // 4:3
    const PREVIEW_CENTER_OFFSET_WIDE: f32 = 98.75; // 16:9
    let preview_center_x =
        speed_mod_x_p1 + widescale(PREVIEW_CENTER_OFFSET_NORMAL, PREVIEW_CENTER_OFFSET_WIDE);

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
            let speed_color = color::simply_love_rgba(player_color_index(state, player_idx));
            let speed_text = speed_header(state, player_idx);
            // zmod uses GetWidth() from the main helper actor (unzoomed width), then +w*0.4.
            let main_draw_w = speed_text.main_draw_width;
            let speed_x = speed_x_for(player_idx);

            actors.push(
                act!(text: font(machine_font_key(state.policy.machine_font, FontRole::Header)): settext(speed_text.main.clone()):
                    align(0.5, 0.5): xy(speed_x, speed_mod_y): zoom(speed_mod_zoom):
                    diffuse(speed_color[0], speed_color[1], speed_color[2], pane_alpha):
                    z(Z_SPEED_MOD_TEXT)
                ),
            );

            if let Some(scaled_text) = speed_text.scaled.as_ref() {
                let scaled_x = main_draw_w.mul_add(0.4, speed_x);
                actors.push(act!(text: font(machine_font_key(state.policy.machine_font, FontRole::Header)): settext(scaled_text.clone()):
                    align(0.5, 0.5): xy(scaled_x, speed_mod_scaled_y): zoom(speed_mod_scaled_zoom):
                    diffuse(speed_color[0], speed_color[1], speed_color[2], 0.8 * pane_alpha):
                    z(Z_SPEED_MOD_TEXT)
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
    let row_titles = state.row_titles[state.current_pane.index()].titles.as_ref();
    debug_assert_eq!(row_titles.len(), total_rows);
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

    let fc = FrameCtx {
        state,
        asset_manager,
        active,
        show_p2,
        option_column_x: [speed_mod_x_p1, speed_mod_x_p2],
        row_left,
        row_width,
        preview_x: [preview_x_for(P1), preview_x_for(P2)],
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
        let active_bg = deadlib_present::color::rgba_hex("#333333");
        let inactive_bg_base = deadlib_present::color::rgba_hex("#071016");
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
            z(Z_ROW_BACKGROUND)
        ));
        if row.id != RowId::Exit {
            actors.push(act!(quad:
                align(0.0, 0.5): xy(row_left, current_row_y):
                zoomto(TITLE_BG_WIDTH, frame_h):
                diffuse(0.0, 0.0, 0.0, 0.25 * a):
                z(Z_ROW_FOREGROUND)
            ));
        }
        if row.id != RowId::Exit {
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
                let display = music_rate_text(state);
                if let Some(second) = display.second.as_ref() {
                    actors.push(act!(text: font("miso"): settext(display.first.clone()):
                        align(0.0, 0.5): xy(title_x, current_row_y - 7.0): zoom(title_zoom):
                        diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                        horizalign(left): maxwidth(title_max_w):
                        z(Z_ROW_FOREGROUND)
                    ));
                    actors.push(act!(text: font("miso"): settext(second.clone()):
                        align(0.0, 0.5): xy(title_x, current_row_y + 7.0): zoom(title_zoom):
                        diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                        horizalign(left): maxwidth(title_max_w):
                        z(Z_ROW_FOREGROUND)
                    ));
                } else {
                    actors.push(act!(text: font("miso"): settext(display.first.clone()):
                        align(0.0, 0.5): xy(title_x, current_row_y): zoom(title_zoom):
                        diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                        horizalign(left): maxwidth(title_max_w):
                        z(Z_ROW_FOREGROUND)
                    ));
                }
            } else {
                actors.push(
                    act!(text: font("miso"): settext(row_titles[item_idx].clone()):
                        align(0.0, 0.5): xy(title_x, current_row_y): zoom(title_zoom):
                        diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                        horizalign(left): maxwidth(title_max_w):
                        z(Z_ROW_FOREGROUND)
                    ),
                );
            }
        }
        // Inactive option text color should be #808080 (alpha 1.0)
        let mut sl_gray = deadlib_present::color::rgba_hex("#808080");
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
            // Align Exit horizontally with other single-value options. In versus
            // it remains a shared row, so the stacked cursors stay together.
            let choice_center_x = if active[P2] && !active[P1] {
                speed_mod_x_p2
            } else {
                speed_mod_x
            };
            actors.push(act!(text: font("miso"): settext(choice_text.clone()):
                align(0.5, 0.5): xy(choice_center_x, current_row_y): zoom(INLINE_CHOICE_VALUE_ZOOM):
                diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                z(Z_ROW_FOREGROUND)
            ));
            // Draw the selection cursor for the centered "Exit" text when active
            if is_active {
                draw_cursor_ring(actors, state, active, item_idx, a);
            }
        } else if show_all_choices_inline {
            let rc = RowCtx {
                fc: &fc,
                row,
                item_idx,
                current_row_y,
                a,
                is_active,
                sl_gray,
            };
            draw_inline_choices(actors, &rc, show_arcade_next_row, choice_inner_left);
        } else {
            let rc = RowCtx {
                fc: &fc,
                row,
                item_idx,
                current_row_y,
                a,
                is_active,
                sl_gray,
            };
            draw_single_value_with_preview(actors, &rc);
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
        let help_text_color = color::simply_love_rgba(player_color_index(state, player_idx));
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
                    help_line.char_count
                } else {
                    let line_fraction = (anim_time - start_time) / time_per_line;
                    ((help_line.char_count as f32 * line_fraction).round() as usize)
                        .min(help_line.char_count)
                };
                let visible_text =
                    revealed_text(&help_line.text, visible_chars, help_line.char_count);

                let line_y = (i as f32).mul_add(line_spacing, start_y);
                actors.push(act!(text:
                    font("miso"): settext(visible_text):
                    align(0.0, 0.5):
                    xy(help_x, line_y):
                    zoom(0.825):
                    diffuse(help_text_color[0], help_text_color[1], help_text_color[2], pane_alpha):
                    maxwidth(wrap_width): horizalign(left):
                    z(Z_ROW_FOREGROUND)
                ));
            }
        } else {
            let visible_text = row.help.first().map_or_else(TextContent::default, |line| {
                let fraction = (state.help_anim_time[player_idx] / REVEAL_DURATION).clamp(0.0, 1.0);
                let visible_chars =
                    ((line.char_count as f32 * fraction).round() as usize).min(line.char_count);
                revealed_text(&line.text, visible_chars, line.char_count)
            });

            actors.push(act!(text:
                font("miso"): settext(visible_text):
                align(0.0, 0.5):
                xy(help_x, help_box_bottom_y - (help_box_h * 0.5)):
                zoom(0.825):
                diffuse(help_text_color[0], help_text_color[1], help_text_color[2], pane_alpha):
                maxwidth(wrap_width): horizalign(left):
                z(Z_ROW_FOREGROUND)
            ));
        }
    }

    // BIOS-style setting search overlay draws above everything else.
    search::push_overlay(actors, state);
}

pub(super) fn revealed_text(
    text: &Arc<str>,
    visible_chars: usize,
    char_count: usize,
) -> TextContent {
    if visible_chars >= char_count {
        TextContent::Shared(text.clone())
    } else {
        TextContent::Owned(text.chars().take(visible_chars).collect())
    }
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(64);
    push_actors(
        &mut actors,
        state,
        asset_manager,
        crate::views::SimplyLoveVisualPolicyView::default(),
    );
    actors
}

/// Frame-level context: things constant across all rows in a single
/// `get_actors` invocation. Held by reference inside `RowCtx`.
pub(super) struct FrameCtx<'a> {
    pub state: &'a State,
    pub asset_manager: &'a AssetManager,
    pub active: [bool; PLAYER_SLOTS],
    pub show_p2: bool,
    pub option_column_x: [f32; PLAYER_SLOTS],
    pub row_left: f32,
    pub row_width: f32,
    pub preview_x: [f32; PLAYER_SLOTS],
}

/// Per-row context: a frame ref plus everything tied to a specific row.
pub(super) struct RowCtx<'a> {
    pub fc: &'a FrameCtx<'a>,
    pub row: &'a Row,
    pub item_idx: usize,
    pub current_row_y: f32,
    pub a: f32,
    pub is_active: bool,
    pub sl_gray: [f32; 4],
}

/// Render z-order layers for the `player_options` screen. Higher values
/// draw on top of lower ones.
pub(super) const Z_ROW_BACKGROUND: i16 = 100;
/// Row text, underlines, cursor borders, choice values, help text.
pub(super) const Z_ROW_FOREGROUND: i16 = 101;
/// Previews drawn over a row (judgment / hold-judgment / noteskin sprites,
/// combo and font samples).
pub(super) const Z_ROW_PREVIEW: i16 = 102;
/// Receptor noteskin preview (drawn above the row preview layer).
pub(super) const Z_RECEPTOR_PREVIEW: i16 = 106;
/// Tap-explosion noteskin preview, the topmost preview layer.
/// Speed-mod overlay text, drawn on top of all preview layers.
pub(super) const Z_SPEED_MOD_TEXT: i16 = 121;

/// Visual zoom for choice values rendered inline next to the row title.
pub(super) const INLINE_CHOICE_VALUE_ZOOM: f32 = 0.835;
/// Horizontal pixel gap between consecutive inline choice values.
pub(super) const INLINE_CHOICE_SPACING: f32 = 15.75;

/// Zoom factor for judgment / hold-judgment / noteskin texture previews.
pub(super) const JUDGMENT_PREVIEW_ZOOM: f32 = 0.225;
/// Zoom factor for combo-font number previews.
pub(super) const COMBO_PREVIEW_ZOOM: f32 = 0.45;

/// Pixel size of one logical noteskin arrow used to compute preview scale.
pub(super) const NOTESKIN_PREVIEW_ARROW_PIXEL_SIZE: f32 = 64.0;
/// Scale applied to noteskin preview sprites relative to their natural size.
pub(super) const NOTESKIN_PREVIEW_SCALE: f32 = 0.45;

/// Underline thickness for multi-select row indicators (16:9 / 16:10 widescale).
pub(super) fn underline_thickness() -> f32 {
    widescale(2.0, 2.5).round().max(1.0)
}

/// Vertical pixel offset between a row's text baseline and its underline.
pub(super) fn underline_offset() -> f32 {
    widescale(3.0, 4.0)
}

/// Border width for the cursor / selection ring drawn around a row's
/// active choice.
pub(super) fn selection_border_width() -> f32 {
    widescale(2.0, 2.5)
}

/// Simply Love / Arrow Cloud offset stacked P1/P2 cursors by one pixel.
#[inline(always)]
pub(super) const fn cursor_stack_y(active: [bool; PLAYER_SLOTS], player_idx: usize) -> f32 {
    if !active[P1] || !active[P2] {
        return 0.0;
    }
    if player_idx == P2 { 1.0 } else { -1.0 }
}

/// Resolve the texture key for a player's currently-selected choice
/// from a fixed list of texture choices, returning `None` for "None".
pub(super) fn select_preview_texture<'a>(
    row: &Row,
    player_idx: usize,
    choices: &'a [deadlib_assets::TextureChoice],
) -> Option<&'a Arc<str>> {
    choices
        .get(row.selected_choice_index[player_idx])
        .and_then(|choice| {
            if choice.key.eq_ignore_ascii_case("None") {
                None
            } else {
                deadsync_assets::textures::resolve_texture_choice_entry(
                    Some(choice.key.as_ref()),
                    choices,
                )
                .map(|choice| &choice.key)
            }
        })
}

/// Project a bitmask row's stored bits into choice-index bits for underlining.
/// Returns `None` when the row does not use bitmask behavior.
pub(super) fn multi_select_mask(state: &State, row: &Row, player_idx: usize) -> Option<u16> {
    let RowBehavior::Bitmask(BitmaskBinding::Generic { init, writeback }) = row.behavior else {
        return None;
    };
    let active_bits = (init.get_active)(&state.option_masks[player_idx]);
    Some(
        writeback
            .bit_mapping
            .active_choice_bits(active_bits, row.choices.len()),
    )
}

/// Remove and return the lowest active choice index from a bounded UI mask.
#[inline(always)]
pub(super) const fn take_active_choice(mask: &mut u16) -> Option<usize> {
    if *mask == 0 {
        return None;
    }
    let choice_idx = mask.trailing_zeros() as usize;
    *mask &= *mask - 1;
    Some(choice_idx)
}

/// Draw multi-select underlines for one row: one underline beneath each
/// choice whose corresponding bit is set in the per-player active mask.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_multi_select_underlines(
    actors: &mut Vec<Actor>,
    state: &State,
    row: &Row,
    active: [bool; PLAYER_SLOTS],
    choice_left_x: f32,
    x_offsets: &[f32],
    widths: &[f32],
    current_row_y: f32,
    text_h: f32,
    a: f32,
) {
    let line_thickness = underline_thickness();
    let offset = underline_offset();
    let underline_base_y = text_h.mul_add(0.5, current_row_y) + offset;
    let underline_y = |player_idx: usize| {
        if active[P1] && active[P2] {
            (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
        } else {
            underline_base_y
        }
    };
    for player_idx in active_player_indices(active) {
        let Some(mut mask) = multi_select_mask(state, row, player_idx) else {
            continue;
        };
        let underline_y = underline_y(player_idx);
        let mut line_color = color::decorative_rgba(player_color_index(state, player_idx));
        line_color[3] *= a;
        while let Some(idx) = take_active_choice(&mut mask) {
            if let Some(sel_x) = x_offsets.get(idx).map(|offset| choice_left_x + offset) {
                let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                let underline_w = draw_w.ceil();
                actors.push(act!(quad:
                    align(0.0, 0.5):
                    xy(sel_x, underline_y):
                    zoomto(underline_w, line_thickness):
                    diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                    z(Z_ROW_FOREGROUND)
                ));
            }
        }
    }
}

/// Draw a single-select underline for one row: one underline under each
/// active player's chosen value.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_single_select_underline(
    actors: &mut Vec<Actor>,
    state: &State,
    row: &Row,
    active: [bool; PLAYER_SLOTS],
    choice_left_x: f32,
    x_offsets: &[f32],
    widths: &[f32],
    current_row_y: f32,
    text_h: f32,
    a: f32,
) {
    let line_thickness = underline_thickness();
    let offset = underline_offset();
    let underline_base_y = text_h.mul_add(0.5, current_row_y) + offset;
    let underline_y = |player_idx: usize| {
        if active[P1] && active[P2] {
            (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
        } else {
            underline_base_y
        }
    };
    for player_idx in active_player_indices(active) {
        let idx = row.selected_choice_index[player_idx].min(widths.len().saturating_sub(1));
        if let Some(sel_x) = x_offsets.get(idx).map(|offset| choice_left_x + offset) {
            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
            let underline_w = draw_w.ceil();
            let underline_y = underline_y(player_idx);
            let mut line_color = color::decorative_rgba(player_color_index(state, player_idx));
            line_color[3] *= a;
            actors.push(act!(quad:
                align(0.0, 0.5):
                xy(sel_x, underline_y):
                zoomto(underline_w, line_thickness):
                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                z(Z_ROW_FOREGROUND)
            ));
        }
    }
}

/// Color palette index for a player's underline / cursor.
pub(super) const fn player_color_index(state: &State, player_idx: usize) -> i32 {
    if player_idx == P2 {
        state.active_color_index - 2
    } else {
        state.active_color_index
    }
}

/// Current animated cursor rectangle for a player (center xy + size),
/// or `None` if the player is inactive or the cursor isn't initialised yet.
pub(super) fn cursor_for_player(state: &State, player_idx: usize) -> Option<(f32, f32, f32, f32)> {
    if player_idx >= PLAYER_SLOTS || !state.pane().cursor_initialized[player_idx] {
        return None;
    }
    let pane = state.pane();
    let t = pane.cursor_t[player_idx].clamp(0.0, 1.0);
    let r = CursorRect::lerp(pane.cursor_from[player_idx], pane.cursor_to[player_idx], t);
    Some((r.x, r.y, r.w, r.h))
}

/// Draw the 4-sided cursor ring around each active player's selected
/// option in row `item_idx`. No-ops for players whose cursor isn't on
/// this row or hasn't been initialised yet.
pub(super) fn draw_cursor_ring(
    actors: &mut Vec<Actor>,
    state: &State,
    active: [bool; PLAYER_SLOTS],
    item_idx: usize,
    a: f32,
) {
    let border_w = selection_border_width();
    for player_idx in active_player_indices(active) {
        if state.pane().selected_row[player_idx] != item_idx {
            continue;
        }
        let Some((center_x, center_y, ring_w, ring_h)) = cursor_for_player(state, player_idx)
        else {
            continue;
        };
        let center_y = center_y + cursor_stack_y(active, player_idx);

        let left = ring_w.mul_add(-0.5, center_x);
        let right = ring_w.mul_add(0.5, center_x);
        let top = ring_h.mul_add(-0.5, center_y);
        let bottom = ring_h.mul_add(0.5, center_y);
        let mut ring_color = color::decorative_rgba(player_color_index(state, player_idx));
        ring_color[3] *= a;

        actors.push(act!(quad:
            align(0.5, 0.5): xy(f32::midpoint(left, right), border_w.mul_add(0.5, top)):
            zoomto(ring_w, border_w):
            diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
            z(Z_ROW_FOREGROUND)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5): xy(f32::midpoint(left, right), border_w.mul_add(-0.5, bottom)):
            zoomto(ring_w, border_w):
            diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
            z(Z_ROW_FOREGROUND)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5): xy(border_w.mul_add(0.5, left), f32::midpoint(top, bottom)):
            zoomto(border_w, ring_h):
            diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
            z(Z_ROW_FOREGROUND)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5): xy(border_w.mul_add(-0.5, right), f32::midpoint(top, bottom)):
            zoomto(border_w, ring_h):
            diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
            z(Z_ROW_FOREGROUND)
        ));
    }
}

/// Render the inline-choices block for one row: every choice laid out
/// horizontally, with multi-select or single-select underline, the
/// optional cursor ring, the optional Arcade `next row` label, and
/// the choice texts themselves.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_inline_choices(
    actors: &mut Vec<Actor>,
    rc: &RowCtx,
    show_arcade_next_row: bool,
    choice_inner_left: f32,
) {
    let value_zoom = INLINE_CHOICE_VALUE_ZOOM;
    let next_row_item = show_arcade_next_row
        .then(|| arcade_next_row_layout(rc.fc.state, rc.item_idx, rc.fc.asset_manager));
    debug_assert!(
        rc.fc.state.pane().choice_layout_ready
            && rc.row.choice_widths.len() == rc.row.choices.len()
            && rc.row.choice_offsets.len() == rc.row.choices.len()
            && rc.row.choice_height > 0.0,
        "inline choice geometry must be prepared before actor construction"
    );
    let widths = rc.row.choice_widths.as_ref();
    let x_offsets = rc.row.choice_offsets.as_ref();
    let text_h = rc.row.choice_height;
    // Draw underline under rc.fc.active options:
    // - For normal rows: underline the currently selected choice.
    // - For Scroll rc.row: underline each enabled scroll mode (multi-select).
    // - For FA+ Options rc.row: underline each enabled FA+ toggle (multi-select).
    if matches!(rc.row.behavior, RowBehavior::Bitmask(_)) {
        draw_multi_select_underlines(
            actors,
            rc.fc.state,
            rc.row,
            rc.fc.active,
            choice_inner_left,
            x_offsets,
            widths,
            rc.current_row_y,
            text_h,
            rc.a,
        );
    } else {
        draw_single_select_underline(
            actors,
            rc.fc.state,
            rc.row,
            rc.fc.active,
            choice_inner_left,
            x_offsets,
            widths,
            rc.current_row_y,
            text_h,
            rc.a,
        );
    }
    // Draw the 4-sided cursor ring around the selected option when this rc.row is rc.fc.active.
    if !widths.is_empty() {
        draw_cursor_ring(actors, rc.fc.state, rc.fc.active, rc.item_idx, rc.a);
    }
    // Draw each option's text (rc.fc.active rc.row: all white; inactive: #808080)
    if let Some((next_row_x, _, _)) = next_row_item {
        let next_row_color = if rc.is_active {
            [1.0, 1.0, 1.0, rc.a]
        } else {
            rc.sl_gray
        };
        actors.push(act!(text: font("miso"): settext(ARCADE_NEXT_ROW_TEXT):
            align(0.0, 0.5): xy(next_row_x, rc.current_row_y): zoom(value_zoom):
            diffuse(
                next_row_color[0],
                next_row_color[1],
                next_row_color[2],
                next_row_color[3]
            ):
            z(Z_ROW_FOREGROUND)
        ));
    }
    for (idx, text) in rc.row.choices.iter().enumerate() {
        let x = x_offsets
            .get(idx)
            .map_or(choice_inner_left, |offset| choice_inner_left + offset);
        let color_rgba = if rc.is_active {
            [1.0, 1.0, 1.0, rc.a]
        } else {
            rc.sl_gray
        };
        actors.push(act!(text: font("miso"): settext(text.clone()):
            align(0.0, 0.5): xy(x, rc.current_row_y): zoom(value_zoom):
            diffuse(color_rgba[0], color_rgba[1], color_rgba[2], color_rgba[3]):
            z(Z_ROW_FOREGROUND)
        ));
    }
}

/// Render the single-value text + optional per-row preview block:
/// the chosen value text, its underline and cursor ring, the optional
/// P2 mirror, plus per-RowId preview sprites/text (judgment, hold,
/// noteskin, mineskin, receptor, explosion, combo).
pub(super) fn draw_single_value_with_preview(actors: &mut Vec<Actor>, rc: &RowCtx) {
    let primary_player_idx = if rc.fc.active[P1] { P1 } else { P2 };
    draw_cached_value_text(actors, rc, primary_player_idx);
    match rc.row.id {
        RowId::JudgmentFont => draw_judgment_preview(actors, rc, primary_player_idx),
        RowId::HoldJudgment => draw_hold_preview(actors, rc, primary_player_idx),
        RowId::HeldGraphic => draw_held_graphic_preview(actors, rc, primary_player_idx),
        RowId::NoteSkin
        | RowId::MineSkin
        | RowId::SkinMineSize
        | RowId::ReceptorSkin
        | RowId::TapExplosionSkin => {
            draw_noteskin_family_preview(actors, rc, primary_player_idx);
        }
        id if super::pack_options::slot_for_row(id).is_some() => {
            draw_noteskin_family_preview(actors, rc, primary_player_idx);
        }
        RowId::ComboFont => draw_combo_preview(actors, rc, primary_player_idx),
        RowId::HeartRateMonitor => draw_heart_rate_preview(actors, rc, primary_player_idx),
        _ => {}
    }
}

fn draw_heart_rate_preview(actors: &mut Vec<Actor>, rc: &RowCtx, primary_player_idx: usize) {
    draw_player_heart_rate_preview(actors, rc, primary_player_idx);
    if rc.fc.show_p2 && primary_player_idx != P2 {
        draw_player_heart_rate_preview(actors, rc, P2);
    }
}

fn draw_player_heart_rate_preview(actors: &mut Vec<Actor>, rc: &RowCtx, player_idx: usize) {
    if rc.fc.state.heart_rate_device_ids[player_idx].is_none() {
        return;
    }
    let reading = rc.fc.state.heart_rate_readings[player_idx];
    let mut rgba = color::decorative_rgba(player_color_index(rc.fc.state, player_idx));
    rgba[3] = if reading.connected { rc.a } else { rc.a * 0.45 };
    let bpm = reading.bpm.unwrap_or(0);
    let pulse =
        crate::screens::components::shared::heart_rate::pulse_scale(rc.fc.state.preview_time, bpm);
    let center_x = rc.fc.preview_x[player_idx];
    let heart_width = 22.0 * pulse;
    let heart_height = 18.7 * pulse;
    actors.push(act!(sprite("heart.png"):
        align(0.5, 0.5): xy(center_x - 10.0, rc.current_row_y):
        zoomto(heart_width, heart_height):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
        z(Z_ROW_FOREGROUND + 1)
    ));
    let text = crate::screens::components::shared::heart_rate::text(reading.bpm);
    actors.push(act!(text:
        font("miso"): settext(text): align(0.0, 0.5): horizalign(left):
        xy(center_x + 9.0, rc.current_row_y): zoom(1.0):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
        z(Z_ROW_FOREGROUND + 1)
    ));
}

fn draw_cached_value_text(actors: &mut Vec<Actor>, rc: &RowCtx, primary_player_idx: usize) {
    debug_assert!(rc.fc.state.pane().choice_layout_ready);
    debug_assert_eq!(rc.row.choice_widths.len(), rc.row.choices.len());
    debug_assert!(rc.row.choice_height > 0.0);
    let mut choice_center_x = rc.fc.option_column_x[primary_player_idx];
    if rc.row.id == RowId::MusicRate {
        let item_col_left = rc.fc.row_left + TITLE_BG_WIDTH;
        choice_center_x = (rc.fc.row_width - TITLE_BG_WIDTH).mul_add(0.5, item_col_left);
    }
    let choice_idx = rc.row.selected_choice_index[primary_player_idx]
        .min(rc.row.choices.len().saturating_sub(1));
    let choice_color = if rc.is_active {
        [1.0, 1.0, 1.0, rc.a]
    } else {
        rc.sl_gray
    };
    let value_zoom = INLINE_CHOICE_VALUE_ZOOM;
    let display = |player_idx: usize, selected_idx: usize| {
        if arcade_row_focuses_next_row(rc.fc.state, player_idx, rc.item_idx) {
            let [width, _] = arcade_next_row_size(rc.fc.state, rc.fc.asset_manager);
            return (TextContent::Static(ARCADE_NEXT_ROW_TEXT), width);
        }
        if rc.row.id == RowId::SpeedMod {
            let value = speed_value(rc.fc.state, player_idx);
            return (value.text.clone(), value.value_draw_width);
        }
        let idx = if rc.row.id == RowId::TypeOfSpeedMod {
            rc.fc.state.speed_mod[player_idx].mod_type.choice_index()
        } else {
            selected_idx
        }
        .min(rc.row.choices.len().saturating_sub(1));
        let text = rc
            .row
            .choices
            .get(idx)
            .expect("prepared option row must have selected text")
            .clone();
        let width = *rc
            .row
            .choice_widths
            .get(idx)
            .expect("prepared option row must have selected width");
        (text, width)
    };

    let (content, draw_w) = display(primary_player_idx, choice_idx);
    let draw_h = rc.row.choice_height;
    actors.push(act!(text: font("miso"): settext(content):
        align(0.5, 0.5): xy(choice_center_x, rc.current_row_y): zoom(value_zoom):
        diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
        z(Z_ROW_FOREGROUND)
    ));
    let line_thickness = underline_thickness();
    let underline_y = draw_h.mul_add(0.5, rc.current_row_y) + underline_offset();
    let mut line_color =
        color::decorative_rgba(player_color_index(rc.fc.state, primary_player_idx));
    line_color[3] *= rc.a;
    actors.push(act!(quad:
        align(0.0, 0.5):
        xy(draw_w.mul_add(-0.5, choice_center_x), underline_y):
        zoomto(draw_w.ceil(), line_thickness):
        diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
        z(Z_ROW_FOREGROUND)
    ));

    if rc.fc.show_p2 && rc.row.id != RowId::MusicRate {
        let p2_idx = rc.row.selected_choice_index[P2].min(rc.row.choices.len().saturating_sub(1));
        let (content, draw_w) = display(P2, p2_idx);
        let center_x = rc.fc.option_column_x[P2];
        actors.push(act!(text: font("miso"): settext(content):
            align(0.5, 0.5): xy(center_x, rc.current_row_y): zoom(value_zoom):
            diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
            z(Z_ROW_FOREGROUND)
        ));
        let mut line_color = color::decorative_rgba(player_color_index(rc.fc.state, P2));
        line_color[3] *= rc.a;
        actors.push(act!(quad:
            align(0.0, 0.5):
            xy(draw_w.mul_add(-0.5, center_x), underline_y):
            zoomto(draw_w.ceil(), line_thickness):
            diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
            z(Z_ROW_FOREGROUND)
        ));
    }
    draw_cursor_ring(actors, rc.fc.state, rc.fc.active, rc.item_idx, rc.a);
}

fn draw_judgment_preview(actors: &mut Vec<Actor>, rc: &RowCtx, primary_player_idx: usize) {
    if rc.row.id == RowId::JudgmentFont {
        let texture_for = |player_idx: usize| -> Option<&Arc<str>> {
            select_preview_texture(
                rc.row,
                player_idx,
                deadsync_assets::textures::judgment_texture_choices(),
            )
        };
        if let Some(texture) = texture_for(primary_player_idx) {
            actors.push(act!(sprite(texture):
                align(0.5, 0.5):
                xy(rc.fc.preview_x[primary_player_idx], rc.current_row_y):
                setstate(0):
                zoom(JUDGMENT_PREVIEW_ZOOM):
                diffuse(1.0, 1.0, 1.0, rc.a):
                z(Z_ROW_PREVIEW)
            ));
        }
        if rc.fc.show_p2
            && primary_player_idx != P2
            && let Some(texture) = texture_for(P2)
        {
            actors.push(act!(sprite(texture):
                align(0.5, 0.5):
                xy(rc.fc.preview_x[P2], rc.current_row_y):
                setstate(0):
                zoom(JUDGMENT_PREVIEW_ZOOM):
                diffuse(1.0, 1.0, 1.0, rc.a):
                z(Z_ROW_PREVIEW)
            ));
        }
    }
}

fn draw_hold_preview(actors: &mut Vec<Actor>, rc: &RowCtx, primary_player_idx: usize) {
    if rc.row.id == RowId::HoldJudgment {
        let texture_for = |player_idx: usize| -> Option<&Arc<str>> {
            select_preview_texture(
                rc.row,
                player_idx,
                deadsync_assets::textures::hold_judgment_texture_choices(),
            )
        };
        let draw_hold_preview = |texture: &Arc<str>, center_x: f32, actors: &mut Vec<Actor>| {
            let zoom = JUDGMENT_PREVIEW_ZOOM;
            let tex_w = deadlib_assets::texture_dims(texture.as_ref())
                .map_or(128.0, |meta| meta.w.max(1) as f32);
            let center_offset = tex_w * zoom * 0.4;

            actors.push(act!(sprite(texture):
                align(0.5, 0.5):
                xy(center_x - center_offset, rc.current_row_y):
                setstate(0):
                zoom(zoom):
                diffuse(1.0, 1.0, 1.0, rc.a):
                z(Z_ROW_PREVIEW)
            ));
            actors.push(act!(sprite(texture):
                align(0.5, 0.5):
                xy(center_x + center_offset, rc.current_row_y):
                setstate(1):
                zoom(zoom):
                diffuse(1.0, 1.0, 1.0, rc.a):
                z(Z_ROW_PREVIEW)
            ));
        };
        if let Some(texture) = texture_for(primary_player_idx) {
            draw_hold_preview(texture, rc.fc.preview_x[primary_player_idx], &mut *actors);
        }
        if rc.fc.show_p2
            && primary_player_idx != P2
            && let Some(texture) = texture_for(P2)
        {
            draw_hold_preview(texture, rc.fc.preview_x[P2], &mut *actors);
        }
    }
}

fn draw_held_graphic_preview(actors: &mut Vec<Actor>, rc: &RowCtx, primary_player_idx: usize) {
    if rc.row.id == RowId::HeldGraphic {
        let texture_for = |player_idx: usize| -> Option<&Arc<str>> {
            select_preview_texture(
                rc.row,
                player_idx,
                deadsync_assets::textures::held_miss_texture_choices(),
            )
        };
        if let Some(texture) = texture_for(primary_player_idx) {
            actors.push(act!(sprite(texture):
                align(0.5, 0.5):
                xy(rc.fc.preview_x[primary_player_idx], rc.current_row_y):
                setstate(0):
                zoom(JUDGMENT_PREVIEW_ZOOM):
                diffuse(1.0, 1.0, 1.0, rc.a):
                z(Z_ROW_PREVIEW)
            ));
        }
        if rc.fc.show_p2
            && primary_player_idx != P2
            && let Some(texture) = texture_for(P2)
        {
            actors.push(act!(sprite(texture):
                align(0.5, 0.5):
                xy(rc.fc.preview_x[P2], rc.current_row_y):
                setstate(0):
                zoom(JUDGMENT_PREVIEW_ZOOM):
                diffuse(1.0, 1.0, 1.0, rc.a):
                z(Z_ROW_PREVIEW)
            ));
        }
    }
}

pub(super) fn draw_thumb(
    actors: &mut Vec<Actor>,
    state: &State,
    thumb: &super::pack_options::Thumb,
    center: [f32; 2],
    size: f32,
    alpha: f32,
    z: i16,
) {
    draw_live_preview(
        actors,
        state,
        &thumb.name,
        thumb.part,
        center,
        size,
        alpha,
        z,
    );
}

fn draw_noteskin_family_preview(actors: &mut Vec<Actor>, rc: &RowCtx, primary_player_idx: usize) {
    for player in [primary_player_idx, P2] {
        if player == P2 && primary_player_idx != P2 && !rc.fc.show_p2 {
            continue;
        }
        let first = actors.len();
        let state = rc.fc.state;
        let options = &state.player_options[player];
        let center = rc.fc.preview_x[player];
        match rc.row.id {
            RowId::NoteSkin => {
                if let Some(skin) = state.noteskin.cache.get(options.noteskin.as_str()) {
                    draw_noteskin_preview(actors, rc, skin, center);
                }
            }
            RowId::ReceptorSkin => {
                let name = options
                    .receptor_noteskin
                    .as_ref()
                    .unwrap_or(&options.noteskin);
                if let Some(skin) = state.noteskin.cache.get(name.as_str()) {
                    draw_receptor_preview(actors, rc, skin, center);
                }
            }
            RowId::MineSkin | RowId::SkinMineSize => {
                let name = options.mine_noteskin.as_ref().unwrap_or(&options.noteskin);
                let size = NOTESKIN_PREVIEW_ARROW_PIXEL_SIZE * NOTESKIN_PREVIEW_SCALE;
                let size = if rc.row.id == RowId::SkinMineSize {
                    size * options.mine_size_percent.clamp(10, 200) as f32 / 200.0
                } else {
                    size
                };
                draw_live_preview(
                    actors,
                    state,
                    name.as_str(),
                    8,
                    [center, rc.current_row_y],
                    size,
                    rc.a,
                    Z_ROW_PREVIEW,
                );
            }
            _ => {
                if let Some(thumb) = state.pack_menu.preview(player, rc.row.id) {
                    draw_thumb(
                        actors,
                        state,
                        thumb,
                        [center, rc.current_row_y],
                        32.0,
                        rc.a,
                        Z_ROW_PREVIEW,
                    );
                }
            }
        }
        let width = if matches!(rc.row.id, RowId::NoteSkin | RowId::ReceptorSkin) {
            96.0
        } else {
            40.0
        };
        fit_preview(
            &mut actors[first..],
            [center, rc.current_row_y],
            [width, 32.0],
        );
        if primary_player_idx == P2 {
            break;
        }
    }
}

fn draw_combo_preview(actors: &mut Vec<Actor>, rc: &RowCtx, primary_player_idx: usize) {
    if rc.row.id == RowId::ComboFont {
        let combo_text = TextContent::inline_u32(rc.fc.state.combo_preview_count);
        let combo_zoom = COMBO_PREVIEW_ZOOM;
        // Choice indices are fixed by construction order:
        // 0=Wendy, 1=ArialRounded, 2=Asap, 3=BebasNeue, 4=SourceCode,
        // 5=Work, 6=WendyCursed, 7=Mega, 8=None
        let combo_font_for = |idx: usize| -> Option<&'static str> {
            match idx {
                0 => Some("wendy_combo"),
                1 => Some("combo_arial_rounded"),
                2 => Some("combo_asap"),
                3 => Some("combo_bebas_neue"),
                4 => Some("combo_source_code"),
                5 => Some("combo_work"),
                6 => Some("combo_wendy_cursed"),
                7 => Some("combo_mega"),
                _ => None,
            }
        };
        let p1_choice_idx = rc.row.selected_choice_index[primary_player_idx]
            .min(rc.row.choices.len().saturating_sub(1));
        if let Some(font_name) = combo_font_for(p1_choice_idx) {
            actors.push(act!(text:
                font(font_name): settext(combo_text.clone()):
                align(0.5, 0.5):
                xy(rc.fc.preview_x[primary_player_idx], rc.current_row_y):
                zoom(combo_zoom): horizalign(center):
                diffuse(1.0, 1.0, 1.0, rc.a):
                z(Z_ROW_PREVIEW)
            ));
        }
        if rc.fc.show_p2 && primary_player_idx != P2 {
            let p2_choice_idx =
                rc.row.selected_choice_index[P2].min(rc.row.choices.len().saturating_sub(1));
            if let Some(font_name) = combo_font_for(p2_choice_idx) {
                actors.push(act!(text:
                    font(font_name): settext(combo_text):
                    align(0.5, 0.5):
                    xy(rc.fc.preview_x[P2], rc.current_row_y):
                    zoom(combo_zoom): horizalign(center):
                    diffuse(1.0, 1.0, 1.0, rc.a):
                    z(Z_ROW_PREVIEW)
                ));
            }
        }
    }
}

const DANCE_PREVIEW_ARROWS: [(usize, f32, f32); 4] =
    [(0, 0.0, -1.5), (1, 1.0, -0.5), (2, 3.0, 0.5), (3, 2.0, 1.5)];
const PUMP_PREVIEW_ARROWS: [(usize, f32, f32); 5] = [
    (0, 0.0, -2.0),
    (1, 1.0, -1.0),
    (2, 2.0, 0.0),
    (3, 3.0, 1.0),
    (4, 4.0, 2.0),
];

const fn preview_arrows(num_cols: usize) -> &'static [(usize, f32, f32)] {
    if matches!(num_cols, 5 | 10) {
        &PUMP_PREVIEW_ARROWS
    } else {
        &DANCE_PREVIEW_ARROWS
    }
}

// Keep layer selection, animation, and model/sprite composition together so the
// noteskin row, component rows, and picker use the same note presentation.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn draw_noteskin_note(
    actors: &mut Vec<Actor>,
    state: &State,
    ns: &Noteskin,
    part: NoteAnimPart,
    note_idx: usize,
    quant_idx: f32,
    center: [f32; 2],
    target_height: f32,
    alpha: f32,
    z: i16,
) {
    let elapsed = state.preview_time;
    let beat = state.preview_beat;
    let phase = ns.part_uv_phase(part, elapsed, beat, 0.0);
    let spacing = ns.note_display_metrics.part_texture_translate[part as usize].note_color_spacing;
    let translation = [spacing[0] * quant_idx, spacing[1] * quant_idx];
    let layers = if part == NoteAnimPart::Lift {
        ns.lift_note_layers.get(note_idx)
    } else {
        None
    };
    let slots = layers
        .or_else(|| ns.note_layers.get(note_idx))
        .map(AsRef::as_ref)
        .or_else(|| ns.notes.get(note_idx).map(std::slice::from_ref))
        .unwrap_or_default();
    let Some(primary) = slots.first() else { return };
    let note_scale = target_height / primary.logical_size()[1].max(1.0);
    for (layer_idx, slot) in slots.iter().enumerate() {
        let frame = slot.frame_index_from_phase(phase);
        let uv_elapsed = if slot.model.is_some() { phase } else { elapsed };
        let uv = slot.uv_for_frame_at(frame, uv_elapsed);
        let uv = [
            uv[0] + translation[0],
            uv[1] + translation[1],
            uv[2] + translation[0],
            uv[3] + translation[1],
        ];
        let logical = slot.logical_size();
        draw_preview_slot(
            actors,
            slot,
            preview_slot_draw(slot, elapsed, beat),
            center,
            [logical[0] * note_scale, logical[1] * note_scale],
            uv,
            -slot.def.rotation_deg as f32,
            [1.0, 1.0, 1.0, alpha],
            BlendMode::Alpha,
            z + layer_idx as i16,
        );
    }
}

fn draw_noteskin_preview(actors: &mut Vec<Actor>, rc: &RowCtx, ns: &Noteskin, center_x: f32) {
    let target_height = NOTESKIN_PREVIEW_ARROW_PIXEL_SIZE * NOTESKIN_PREVIEW_SCALE;
    for &(col, quant_idx, x_mult) in preview_arrows(ns.column_xs.len()) {
        let x = x_mult.mul_add(target_height, center_x);
        let note_idx = col * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
        draw_noteskin_note(
            actors,
            rc.fc.state,
            ns,
            NoteAnimPart::Tap,
            note_idx,
            quant_idx,
            [x, rc.current_row_y],
            target_height,
            rc.a,
            Z_ROW_PREVIEW,
        );
    }
}

pub(super) fn draw_live_preview(
    actors: &mut Vec<Actor>,
    state: &State,
    name: &str,
    part: usize,
    center: [f32; 2],
    size: f32,
    alpha: f32,
    z: i16,
) -> bool {
    let Some(skin) = state.noteskin.cache.get(name) else {
        return false;
    };
    let first = actors.len();
    // Leave room for tap-explosion zoom commands inside the fixed icon bounds.
    let target = if part == 6 { size * 0.9 } else { size };
    draw_skin_part(actors, state, skin, part, center, target, alpha, z);
    fit_preview(&mut actors[first..], center, [size, size]);
    true
}

/// Fit the complete animated geometry, including rotated sprites and model transforms.
fn fit_preview(actors: &mut [Actor], center: [f32; 2], limit: [f32; 2]) {
    use deadlib_present::actors::SizeSpec;
    let mut min = glam::Vec2::splat(f32::INFINITY);
    let mut max = glam::Vec2::splat(f32::NEG_INFINITY);
    for actor in actors.iter() {
        match actor {
            Actor::Sprite {
                offset,
                size: [SizeSpec::Px(w), SizeSpec::Px(h)],
                scale,
                rot_z_deg,
                ..
            } => {
                let (sin, cos) = rot_z_deg.to_radians().sin_cos();
                let w = (w * scale[0]).abs();
                let h = (h * scale[1]).abs();
                let half =
                    glam::Vec2::new(cos.abs() * w + sin.abs() * h, sin.abs() * w + cos.abs() * h)
                        * 0.5;
                min = min.min(glam::Vec2::from(*offset) - half);
                max = max.max(glam::Vec2::from(*offset) + half);
            }
            Actor::TexturedMesh {
                offset,
                vertices,
                local_transform,
                ..
            } => {
                for vertex in vertices.iter() {
                    let p = local_transform
                        .transform_point3(glam::Vec3::from(vertex.pos))
                        .truncate()
                        + glam::Vec2::from(*offset);
                    min = min.min(p);
                    max = max.max(p);
                }
            }
            _ => {}
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return;
    }
    let extent = (max - min).max(glam::Vec2::splat(1.0));
    let scale = (limit[0] / extent.x).min(limit[1] / extent.y).min(1.0);
    let origin = (min + max) * 0.5;
    for actor in actors {
        match actor {
            Actor::Sprite {
                offset,
                scale: zoom,
                ..
            } => {
                *offset = (glam::Vec2::from(center) + (glam::Vec2::from(*offset) - origin) * scale)
                    .to_array();
                zoom[0] *= scale;
                zoom[1] *= scale;
            }
            Actor::TexturedMesh {
                offset,
                local_transform,
                ..
            } => {
                *offset = (glam::Vec2::from(center) + (glam::Vec2::from(*offset) - origin) * scale)
                    .to_array();
                *local_transform =
                    glam::Mat4::from_scale(glam::Vec3::new(scale, scale, 1.0)) * *local_transform;
            }
            _ => {}
        }
    }
}

fn draw_mine_preview(
    actors: &mut Vec<Actor>,
    state: &State,
    mine_ns: &Noteskin,
    mine_center: [f32; 2],
    target_height: f32,
    alpha: f32,
    z: i16,
) {
    let mine_col = if mine_ns.mines.len() > 1 || mine_ns.mine_frames.len() > 1 {
        1
    } else {
        0
    };
    let fill_slot = mine_ns.mines.get(mine_col).and_then(|slot| slot.as_ref());
    let frame_slot = mine_ns
        .mine_frames
        .get(mine_col)
        .and_then(|slot| slot.as_ref());
    let Some(primary_slot) = frame_slot.or(fill_slot) else {
        return;
    };
    let mine_phase = mine_ns.tap_mine_uv_phase(state.preview_time, state.preview_beat, 0.0);
    let mine_translation = mine_ns.part_uv_translation(NoteAnimPart::Mine, 0.0, false);

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
    let draw_mine_slot = |slot: &SpriteSlot, alpha: f32, z: i32, actors: &mut Vec<Actor>| {
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
        draw_mine_slot(slot, 0.85 * alpha, i32::from(z), actors);
    }
    if let Some(slot) = frame_slot {
        draw_mine_slot(slot, alpha, i32::from(z) + 1, actors);
    } else if fill_slot.is_none() {
        draw_mine_slot(primary_slot, alpha, i32::from(z) + 1, actors);
    }
}

#[inline(always)]
fn slot_preview_zoom_x(slot: &SpriteSlot, zoom: f32) -> f32 {
    if slot.def.mirror_h { -zoom } else { zoom }
}

#[inline(always)]
fn slot_preview_zoom_y(slot: &SpriteSlot, zoom: f32) -> f32 {
    if slot.def.mirror_v { -zoom } else { zoom }
}

#[allow(clippy::too_many_arguments)]
fn draw_skin_part(
    actors: &mut Vec<Actor>,
    state: &State,
    skin: &Noteskin,
    part: usize,
    center: [f32; 2],
    size: f32,
    alpha: f32,
    z: i16,
) {
    match part {
        0 | 10 => draw_noteskin_note(
            actors,
            state,
            skin,
            if part == 10 {
                NoteAnimPart::Lift
            } else {
                NoteAnimPart::Tap
            },
            Quantization::Q4th as usize,
            0.0,
            center,
            size,
            alpha,
            z,
        ),
        1 => draw_receptor_note(actors, state, skin, 0, center, size, alpha, z),
        6 => draw_tap_explosion_preview(actors, state, skin, center, size, alpha, z),
        8 => draw_mine_preview(actors, state, skin, center, size, alpha, z),
        _ => {
            let (slot, anim) = match part {
                2 => (skin.hold.body_active.as_ref(), Some(NoteAnimPart::HoldBody)),
                3 => (
                    skin.hold.body_inactive.as_ref(),
                    Some(NoteAnimPart::HoldBody),
                ),
                4 => (skin.roll.body_active.as_ref(), Some(NoteAnimPart::RollBody)),
                5 => (
                    skin.roll.body_inactive.as_ref(),
                    Some(NoteAnimPart::RollBody),
                ),
                7 => (skin.hold.explosion.as_ref(), None),
                _ => return,
            };
            let Some(slot) = slot else { return };
            let elapsed = state.preview_time;
            let beat = state.preview_beat;
            let phase = anim.map(|part| skin.part_uv_phase(part, elapsed, beat, 0.0));
            let frame = phase.map_or_else(
                || slot.frame_index(elapsed, beat),
                |phase| slot.frame_index_from_phase(phase),
            );
            let uv_time = if slot.model.is_some() {
                phase.unwrap_or(elapsed)
            } else {
                elapsed
            };
            let uv = slot.uv_for_frame_at(frame, uv_time);
            let logical = slot.logical_size();
            let scale = size / logical[1].max(1.0);
            draw_preview_slot(
                actors,
                slot,
                preview_slot_draw(slot, elapsed, beat),
                center,
                [logical[0] * scale, logical[1] * scale],
                uv,
                -slot.def.rotation_deg as f32,
                [1.0, 1.0, 1.0, alpha],
                BlendMode::Alpha,
                z,
            );
        }
    }
}

fn preview_slot_draw(
    slot: &SpriteSlot,
    elapsed: f32,
    beat: f32,
) -> deadsync_noteskin::ModelDrawState {
    let mut draw = slot.model_draw_at(elapsed, beat);
    if let Some(glow) = slot.model_glow_with_draw(draw, elapsed, beat, 1.0) {
        draw.glow = glow;
    }
    draw
}

// Model and sprite transforms share one path, including diffuse/glow effects.
// Explosion commands supply their sampled draw state instead of the idle state.
#[allow(clippy::too_many_arguments)]
fn draw_preview_slot(
    actors: &mut Vec<Actor>,
    slot: &SpriteSlot,
    draw: deadsync_noteskin::ModelDrawState,
    center: [f32; 2],
    size: [f32; 2],
    uv: [f32; 4],
    rotation: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
) {
    if !draw.visible {
        return;
    }
    let blend = if draw.blend_add {
        BlendMode::Add
    } else {
        blend
    };
    let mut actor = if let Some(actor) =
        noteskin_model_actor_from_draw(slot, draw, center, size, uv, rotation, color, blend, z)
    {
        actor
    } else {
        let logical = slot.logical_size();
        let ox = draw.pos[0] * size[0] / logical[0].max(1.0);
        let oy = draw.pos[1] * size[1] / logical[1].max(1.0);
        let (sin, cos) = rotation.to_radians().sin_cos();
        let pos = [
            center[0] + ox * cos - oy * sin,
            center[1] + ox * sin + oy * cos,
        ];
        let size = [size[0] * draw.zoom[0].abs(), size[1] * draw.zoom[1].abs()];
        if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
            return;
        }
        let tint = std::array::from_fn::<_, 4, _>(|i| color[i] * draw.tint[i]);
        let mut actor = act!(sprite(slot.texture_key_shared()):
            align(0.5, 0.5): xy(pos[0], pos[1]): setsize(size[0], size[1]):
            zoomx(slot_preview_zoom_x(slot, draw.zoom[0].signum())):
            zoomy(slot_preview_zoom_y(slot, draw.zoom[1].signum())):
            rotationz(draw.rot[2] + rotation):
            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
            diffuse(tint[0], tint[1], tint[2], tint[3]): z(z)
        );
        if let Actor::Sprite {
            blend: actor_blend, ..
        } = &mut actor
        {
            *actor_blend = blend;
        }
        actor
    };
    match &mut actor {
        Actor::Sprite { glow, .. } | Actor::TexturedMesh { glow, .. } => {
            *glow = [
                draw.glow[0],
                draw.glow[1],
                draw.glow[2],
                draw.glow[3] * color[3],
            ];
        }
        _ => {}
    }
    actors.push(actor);
}

fn draw_receptor_preview(actors: &mut Vec<Actor>, rc: &RowCtx, skin: &Noteskin, center_x: f32) {
    let size = NOTESKIN_PREVIEW_ARROW_PIXEL_SIZE * NOTESKIN_PREVIEW_SCALE;
    for &(col, _, x_mult) in preview_arrows(skin.column_xs.len()) {
        draw_receptor_note(
            actors,
            rc.fc.state,
            skin,
            col,
            [x_mult.mul_add(size, center_x), rc.current_row_y],
            size,
            rc.a,
            Z_RECEPTOR_PREVIEW,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_receptor_note(
    actors: &mut Vec<Actor>,
    state: &State,
    skin: &Noteskin,
    col: usize,
    center: [f32; 2],
    size: f32,
    alpha: f32,
    z: i16,
) {
    let elapsed = state.preview_time;
    let beat = state.preview_beat;
    let pulse = skin.receptor_pulse.color_for_beat(beat);
    let idle = skin.receptor_idle_glow.alpha(beat, false);
    for (index, slot, color) in [
        (
            0,
            skin.receptor_off.get(col),
            [pulse[0], pulse[1], pulse[2], pulse[3] * alpha],
        ),
        (
            1,
            skin.receptor_idle_glow_layers
                .get(col)
                .and_then(Option::as_ref)
                .or_else(|| skin.receptor_glow.get(col).and_then(Option::as_ref)),
            [1.0, 1.0, 1.0, idle * alpha],
        ),
    ] {
        let Some(slot) = slot else { continue };
        if color[3] <= f32::EPSILON {
            continue;
        }
        let frame = slot.frame_index(elapsed, beat);
        let uv = slot.uv_for_frame_at(frame, elapsed);
        let logical = slot.logical_size();
        let scale = size / logical[1].max(1.0);
        draw_preview_slot(
            actors,
            slot,
            preview_slot_draw(slot, elapsed, beat),
            center,
            [logical[0] * scale, logical[1] * scale],
            uv,
            -slot.def.rotation_deg as f32,
            color,
            if index == 0 {
                BlendMode::Alpha
            } else {
                BlendMode::Add
            },
            z + index,
        );
    }
}

fn draw_tap_explosion_preview(
    actors: &mut Vec<Actor>,
    state: &State,
    skin: &Noteskin,
    center: [f32; 2],
    size: f32,
    alpha: f32,
    z: i16,
) {
    let Some(explosion) = skin
        .tap_explosions
        .get("W1")
        .or_else(|| skin.tap_explosions.values().next())
    else {
        return;
    };
    let time = state.preview_time * TAP_EXPLOSION_PREVIEW_SPEED;
    let beat = state.preview_beat * TAP_EXPLOSION_PREVIEW_SPEED;
    let duration = explosion.duration();
    let elapsed = if duration > f32::EPSILON {
        time.rem_euclid(duration)
    } else {
        0.0
    };
    let scale = size / explosion.slot.logical_size()[1].max(1.0);
    for (index, layer) in explosion.layers.iter().enumerate() {
        let visual = layer.animation.state_at(elapsed);
        let slot = &layer.slot;
        let frame_beat = if slot.source.is_beat_based() {
            elapsed
        } else {
            beat
        };
        let frame = slot.frame_index(elapsed, frame_beat);
        let uv = slot.uv_for_frame_at(frame, if slot.model.is_some() { elapsed } else { time });
        let logical = slot.logical_size();
        let draw = deadsync_noteskin::ModelDrawState {
            zoom: [visual.zoom, visual.zoom, 1.0],
            rot: [0.0, 0.0, visual.rotation_z],
            tint: visual.diffuse,
            glow: visual.glow,
            visible: visual.visible,
            blend_add: layer.animation.blend_add,
            ..Default::default()
        };
        draw_preview_slot(
            actors,
            slot,
            draw,
            center,
            [logical[0] * scale, logical[1] * scale],
            uv,
            -slot.def.rotation_deg as f32,
            [1.0, 1.0, 1.0, alpha],
            BlendMode::Alpha,
            z + index as i16,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{fit_preview, preview_arrows};
    use crate::act;
    use deadlib_present::actors::{Actor, SizeSpec};

    #[test]
    fn wide_rotated_previews_fit_without_changing_aspect_ratio() {
        let mut actors = [act!(sprite("test/large"):
            align(0.5, 0.5): xy(180.0, 220.0):
            setsize(1000.0, 50.0): zoom(4.0): rotationz(90.0)
        )];
        fit_preview(&mut actors, [100.0, 100.0], [32.0, 32.0]);
        let Actor::Sprite {
            offset,
            size: [SizeSpec::Px(width), SizeSpec::Px(_)],
            scale,
            ..
        } = &actors[0]
        else {
            panic!("sprite")
        };
        assert_eq!(*offset, [100.0, 100.0]);
        assert!((width * scale[0] - 32.0).abs() < 0.001);
        assert_eq!(scale[0], scale[1], "fit keeps the original aspect ratio");
    }

    #[test]
    fn preview_arrows_match_active_game_columns() {
        assert_eq!(
            preview_arrows(4)
                .iter()
                .map(|(col, _, _)| *col)
                .collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        assert_eq!(
            preview_arrows(5)
                .iter()
                .map(|(col, _, _)| *col)
                .collect::<Vec<_>>(),
            [0, 1, 2, 3, 4]
        );
        assert_eq!(preview_arrows(10), preview_arrows(5));
    }
}
