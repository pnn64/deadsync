// Frozen from fc570795b (0.5.1206); only module visibility adapted.
use super::*;

pub(in crate::screens::player_options) fn current_value(
    state: &State,
    m: &SettingMatch,
    player_idx: usize,
) -> Option<String> {
    let row = state.panes[m.pane.index()].row_map.get(m.row_id)?;
    let idx = row.selected_choice_index[player_idx].min(row.choices.len().saturating_sub(1));
    row.choices.get(idx).map(std::string::ToString::to_string)
}

pub(in crate::screens::player_options) fn push_overlay(actors: &mut Vec<Actor>, state: &State) {
    let SettingSearchState::Open(open) = &state.search else {
        return;
    };

    let cx = screen_center_x();
    let cy = screen_center_y();
    let panel_w = 360.0_f32.min(screen_width() * 0.92);
    let panel_h = 360.0_f32;
    let top = panel_h.mul_add(-0.5, cy);

    // Theme-native palette, matching the options screen and option rows.
    let theme = color::simply_love_rgba(state.active_color_index);
    const PANEL_BG: [f32; 4] = deadlib_present::color::rgba_hex("#071016");
    const FOCUS_BG: [f32; 4] = deadlib_present::color::rgba_hex("#333333");
    const GRAY: [f32; 4] = deadlib_present::color::rgba_hex("#808080");
    const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

    actors.reserve(32);

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.8): z(Z_DIM)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy):
        zoomto(panel_w + 2.0, panel_h + 2.0):
        diffuse(WHITE[0], WHITE[1], WHITE[2], 1.0): z(Z_PANEL_BORDER)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy):
        zoomto(panel_w, panel_h):
        diffuse(PANEL_BG[0], PANEL_BG[1], PANEL_BG[2], 1.0): z(Z_PANEL)
    ));

    let title = open.component.map_or_else(
        || tr("PlayerOptions", "SettingSearchTitle"),
        |row| state.pane().row_map.row(row).name.get(),
    );
    actors.push(act!(text:
        font("wendy"): settext(title):
        align(0.5, 0.5): xy(cx, top + 20.0): zoom(0.4):
        maxwidth(panel_w - 24.0):
        diffuse(WHITE[0], WHITE[1], WHITE[2], 1.0): z(Z_TEXT): horizalign(center)
    ));

    // Query line: prompt + typed text with an inline ghost. The ghost is drawn
    // by laying the full label underneath (gray) and the typed prefix on top
    // (theme color), so the remainder shows through — no font measurement.
    let caret_on = open.blink_t < CURSOR_BLINK_PERIOD * 0.5;
    let query_y = top + 46.0;
    let query_x = cx - panel_w * 0.5 + 14.0;
    let text_x = query_x + 14.0;
    actors.push(act!(text:
        font("miso"): settext("> "):
        align(0.0, 0.5): xy(query_x, query_y): zoom(0.9):
        diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(left)
    ));
    if open.query.is_empty() {
        let placeholder = tr(
            "PlayerOptions",
            if open.component.is_some() {
                "SkinSearchPlaceholder"
            } else {
                "SettingSearchPlaceholder"
            },
        );
        actors.push(act!(text:
            font("miso"): settext(placeholder):
            align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
            maxwidth(panel_w - 40.0):
            diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(left)
        ));
    } else {
        // Shared with accept_ghost so Tab does exactly what the ghost shows.
        let ghost = completion(open);
        match ghost {
            Some((full_label, prefix)) => {
                actors.push(act!(text:
                    font("miso"): settext(full_label):
                    align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
                    maxwidth(panel_w - 40.0):
                    diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(left)
                ));
                actors.push(act!(text:
                    font("miso"): settext(prefix):
                    align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
                    maxwidth(panel_w - 40.0):
                    diffuse(theme[0], theme[1], theme[2], 1.0): z(Z_TEXT + 1): horizalign(left)
                ));
            }
            None => {
                let caret = if caret_on { "▮" } else { "" };
                actors.push(act!(text:
                    font("miso"): settext(format!("{}{caret}", open.query)):
                    align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
                    maxwidth(panel_w - 40.0):
                    diffuse(theme[0], theme[1], theme[2], 1.0): z(Z_TEXT): horizalign(left)
                ));
            }
        }
    }

    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, top + 66.0):
        zoomto(panel_w - 20.0, 1.0):
        diffuse(GRAY[0], GRAY[1], GRAY[2], 0.5): z(Z_TEXT)
    ));

    let list_top = top + 84.0;
    let row_step = 21.0;
    let list_x = cx - panel_w * 0.5 + 16.0;
    let pane_x = cx + panel_w * 0.5 - 16.0;
    if open.matches.is_empty() {
        let no_matches = tr("PlayerOptions", "SettingSearchNoMatches");
        actors.push(act!(text:
            font("miso"): settext(no_matches):
            align(0.0, 0.5): xy(list_x, list_top): zoom(0.8):
            maxwidth(panel_w - 32.0):
            diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(left)
        ));
    }
    let range = visible_range(open);
    let shown = range.len();
    let first = range.start;
    for i in 0..shown {
        let m = &open.matches[first + i];
        let y = (i as f32).mul_add(row_step, list_top);
        let focused = first + i == open.selected_index;
        if focused {
            actors.push(act!(quad:
                align(0.0, 0.5): xy(cx - panel_w * 0.5 + 8.0, y):
                zoomto(panel_w - 16.0, row_step - 2.0):
                diffuse(FOCUS_BG[0], FOCUS_BG[1], FOCUS_BG[2], 1.0): z(Z_TEXT)
            ));
        }
        let (text_rgb, pane_rgb) = if focused {
            (
                [theme[0], theme[1], theme[2]],
                [theme[0], theme[1], theme[2]],
            )
        } else {
            ([GRAY[0], GRAY[1], GRAY[2]], [GRAY[0], GRAY[1], GRAY[2]])
        };
        if let Some(thumb) = &m.thumb {
            if focused {
                request_preview_priority(
                    state,
                    &thumb.name,
                    thumb.part,
                    NoteskinPreviewPriority::Focused,
                );
            }
            super::render::draw_thumb(
                actors,
                state,
                thumb,
                [list_x + 10.0, y],
                18.0,
                1.0,
                Z_TEXT + 1,
            );
        }
        let choice_x = if open.component.is_some() {
            list_x + 24.0
        } else {
            list_x
        };
        let choice_width = if open.component.is_some() {
            panel_w - 64.0
        } else {
            panel_w * 0.62
        };
        actors.push(act!(text:
            font("miso"): settext(m.row_text.get(&m.label, focused)):
            align(0.0, 0.5): xy(choice_x, y): zoom(0.85):
            maxwidth(choice_width):
            diffuse(text_rgb[0], text_rgb[1], text_rgb[2], 1.0): z(Z_TEXT + 1): horizalign(left)
        ));
        actors.push(act!(text:
            font("miso"): settext(Arc::clone(&m.pane_text)):
            align(1.0, 0.5): xy(pane_x, y): zoom(0.7):
            maxwidth(panel_w * 0.34):
            diffuse(pane_rgb[0], pane_rgb[1], pane_rgb[2], 1.0): z(Z_TEXT + 1): horizalign(right)
        ));
    }

    // Focused match detail: current value, then wrapped help text.
    if let Some(m) = focused_match(open) {
        let value_y = panel_h.mul_add(0.5, cy) - 74.0;
        if let Some(value) = current_value(state, m, open.opener_player) {
            let current = tr_fmt(
                "PlayerOptions",
                "SettingSearchCurrent",
                &[("value", &value)],
            );
            actors.push(act!(text:
                font("miso"): settext(current):
                align(0.0, 0.5): xy(list_x, value_y): zoom(0.75):
                maxwidth(panel_w - 32.0):
                diffuse(WHITE[0], WHITE[1], WHITE[2], 1.0): z(Z_TEXT): horizalign(left)
            ));
        }
        if open.component.is_none()
            && let Some(help) = help_text(state, m)
        {
            actors.push(act!(text:
                font("miso"): settext(help):
                align(0.0, 0.0): xy(list_x, value_y + 14.0): zoom(0.72):
                wrapwidthpixels((panel_w - 32.0) / 0.72):
                diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(left)
            ));
        }
    }

    let footer = tr(
        "PlayerOptions",
        if open.component.is_some() {
            "SkinSearchFooter"
        } else {
            "SettingSearchFooter"
        },
    );
    actors.push(act!(text:
        font("miso"): settext(footer):
        align(0.5, 0.5): xy(cx, panel_h.mul_add(0.5, cy) - 14.0): zoom(0.7):
        maxwidth(panel_w - 24.0):
        diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(center)
    ));
}
