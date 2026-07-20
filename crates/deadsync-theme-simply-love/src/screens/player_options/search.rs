//! Fuzzy "search for a setting" overlay for Player Options.
//!
//! `/` opens a live typeahead: each keystroke re-ranks the visible settings
//! across all four panes, offers a ghost completion (Tab accepts), and Enter
//! jumps the cursor to the chosen row. Only visible rows are indexed, so hidden
//! sub-options never appear as dead-end results.

use super::fuzzy;
use super::*;
use deadlib_present::space::screen_width;

/// Maximum number of ranked results shown in the overlay list.
pub(super) const SEARCH_MAX_RESULTS: usize = 8;
const CURSOR_BLINK_PERIOD: f32 = 1.0;

const Z_DIM: i16 = 1450;
const Z_PANEL_BORDER: i16 = 1451;
const Z_PANEL: i16 = 1452;
const Z_TEXT: i16 = 1453;

/// A single ranked search result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SettingMatch {
    pub row_id: RowId,
    pub pane: OptionsPane,
    pub label: String,
    pub score: i32,
}

/// Live state of the search overlay.
#[derive(Clone, Debug)]
pub(super) struct SettingSearchOpen {
    pub query: String,
    pub matches: Vec<SettingMatch>,
    pub selected_index: usize,
    pub blink_t: f32,
    /// Player slot that opened the search; its cursor is the one that jumps.
    pub opener_player: usize,
}

#[derive(Clone, Debug)]
pub(super) enum SettingSearchState {
    Hidden,
    Open(SettingSearchOpen),
}

impl Default for SettingSearchState {
    fn default() -> Self {
        Self::Hidden
    }
}

impl SettingSearchState {
    #[inline(always)]
    pub(super) fn is_open(&self) -> bool {
        matches!(self, Self::Open(_))
    }
}

/// Search order; first pane wins for rows appearing in more than one.
const SEARCH_PANE_ORDER: [OptionsPane; OptionsPane::COUNT] = [
    OptionsPane::Main,
    OptionsPane::Display,
    OptionsPane::Advanced,
    OptionsPane::Uncommon,
];

/// Strip multi-line/templated i18n names down to a single clean label, e.g.
/// `Music Rate\nbpm: {bpm}` -> `Music Rate`.
fn clean_label(raw: &str) -> String {
    let mut end = raw.len();
    for pat in ["\\n", "\n", "{"] {
        if let Some(i) = raw.find(pat) {
            end = end.min(i);
        }
    }
    raw[..end].trim().to_string()
}

/// Build the ranked match list for `query` from the currently-visible rows.
pub(super) fn rebuild_matches(state: &State, query: &str) -> Vec<SettingMatch> {
    let q = fuzzy::query_chars(query);
    let active = state.active;
    let mut seen = [false; RowId::COUNT];
    let mut matches: Vec<SettingMatch> = Vec::new();

    for pane in SEARCH_PANE_ORDER {
        let row_map = &state.panes[pane.index()].row_map;
        let visibility = visibility::row_visibility(row_map, active, state.option_masks, state.policy);
        for (display_idx, &id) in row_map.display_order().iter().enumerate() {
            if id == RowId::Exit || seen[id.index()] {
                continue;
            }
            let Some(row) = row_map.get(id) else {
                continue;
            };
            if !visibility::is_row_visible(row_map, display_idx, visibility) {
                continue;
            }
            seen[id.index()] = true;

            let label = clean_label(&row.name.get());
            if q.is_empty() {
                matches.push(SettingMatch {
                    row_id: id,
                    pane,
                    label,
                    score: 0,
                });
            } else if let Some(score) = fuzzy::best_match_score(&q, &label, id) {
                matches.push(SettingMatch {
                    row_id: id,
                    pane,
                    label,
                    score,
                });
            }
        }
    }

    if !q.is_empty() {
        matches.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.label.len().cmp(&b.label.len()))
                .then_with(|| a.label.cmp(&b.label))
        });
    }

    matches
}

/// Open the overlay for `opener_player`, seeded with the full visible list.
pub(super) fn open(state: &mut State, opener_player: usize) {
    let matches = rebuild_matches(state, "");
    state.search = SettingSearchState::Open(SettingSearchOpen {
        query: String::new(),
        matches,
        selected_index: 0,
        blink_t: 0.0,
        opener_player,
    });
}

pub(super) fn close(state: &mut State) {
    state.search = SettingSearchState::Hidden;
}

/// Advance the caret-blink clock.
pub(super) fn update(search: &mut SettingSearchState, dt: f32) -> bool {
    match search {
        SettingSearchState::Hidden => false,
        SettingSearchState::Open(open) => {
            open.blink_t = (open.blink_t + dt.max(0.0)) % CURSOR_BLINK_PERIOD;
            true
        }
    }
}

/// Re-rank after the query changed and clamp the selection.
fn refresh(state: &mut State) {
    let SettingSearchState::Open(open) = &state.search else {
        return;
    };
    let query = open.query.clone();
    let matches = rebuild_matches(state, &query);
    if let SettingSearchState::Open(open) = &mut state.search {
        open.matches = matches;
        open.selected_index = open
            .selected_index
            .min(open.matches.len().saturating_sub(1));
    }
}

pub(super) fn add_text(state: &mut State, text: &str) {
    if let SettingSearchState::Open(open) = &mut state.search {
        for ch in text.chars() {
            if ch.is_control() {
                continue;
            }
            open.query.push(ch);
        }
        open.selected_index = 0;
    }
    refresh(state);
}

/// Delete the last query char (no-op on an empty query).
pub(super) fn backspace(state: &mut State) {
    let changed = match &mut state.search {
        SettingSearchState::Open(open) => {
            if open.query.is_empty() {
                false
            } else {
                open.query.pop();
                open.selected_index = 0;
                true
            }
        }
        SettingSearchState::Hidden => false,
    };
    if changed {
        refresh(state);
    }
}

pub(super) fn move_selection(state: &mut State, delta: isize) {
    if let SettingSearchState::Open(open) = &mut state.search {
        let shown = open.matches.len().min(SEARCH_MAX_RESULTS);
        if shown == 0 {
            return;
        }
        let cur = open.selected_index.min(shown - 1) as isize;
        open.selected_index = (cur + delta).rem_euclid(shown as isize) as usize;
    }
}

/// Complete the typed query to the focused match's label (Tab / →).
pub(super) fn accept_ghost(state: &mut State) {
    let label = match &state.search {
        SettingSearchState::Open(open) => open
            .matches
            .get(open.selected_index)
            .map(|m| m.label.clone()),
        SettingSearchState::Hidden => None,
    };
    if let (Some(label), SettingSearchState::Open(open)) = (label, &mut state.search) {
        open.query = label;
        open.selected_index = 0;
    }
    refresh(state);
}

#[inline(always)]
pub(super) fn focused_match(open: &SettingSearchOpen) -> Option<&SettingMatch> {
    open.matches.get(open.selected_index)
}

/// The row's currently selected choice, for the detail line.
fn current_value(state: &State, m: &SettingMatch, player_idx: usize) -> Option<String> {
    let row = state.panes[m.pane.index()].row_map.get(m.row_id)?;
    let idx = row.selected_choice_index[player_idx].min(row.choices.len().saturating_sub(1));
    row.choices.get(idx).map(|c| c.to_string())
}

fn pane_label(pane: OptionsPane) -> &'static str {
    match pane {
        OptionsPane::Main => "Main",
        OptionsPane::Display => "Display",
        OptionsPane::Advanced => "Advanced",
        OptionsPane::Uncommon => "Uncommon",
    }
}

/// Build the overlay actors, or `None` when the search is hidden.
pub(super) fn build_overlay(state: &State) -> Option<Vec<Actor>> {
    let SettingSearchState::Open(open) = &state.search else {
        return None;
    };

    let cx = screen_center_x();
    let cy = screen_center_y();
    let panel_w = 360.0_f32.min(screen_width() * 0.92);
    let panel_h = 300.0_f32;
    let top = cy - panel_h * 0.5;
    let theme = color::simply_love_rgba(state.active_color_index);

    let mut actors = Vec::with_capacity(32);

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.8): z(Z_DIM)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy):
        zoomto(panel_w + 2.0, panel_h + 2.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_PANEL_BORDER)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy):
        zoomto(panel_w, panel_h):
        diffuse(0.1, 0.1, 0.1, 1.0): z(Z_PANEL)
    ));

    actors.push(act!(text:
        font("wendy"): settext("Setting Search"):
        align(0.5, 0.5): xy(cx, top + 20.0): zoom(0.4):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_TEXT): horizalign(center)
    ));

    // Query line: prompt + typed text with an inline ghost completion. The
    // ghost is drawn by laying the full match label underneath (grey) and the
    // real-cased typed prefix on top (theme green), so the untyped remainder
    // shows through as a suggestion — no font measurement required.
    let caret_on = open.blink_t < CURSOR_BLINK_PERIOD * 0.5;
    let query_y = top + 46.0;
    let query_x = cx - panel_w * 0.5 + 14.0;
    let text_x = query_x + 14.0;
    actors.push(act!(text:
        font("miso"): settext("> "):
        align(0.0, 0.5): xy(query_x, query_y): zoom(0.9):
        diffuse(0.7, 0.7, 0.7, 1.0): z(Z_TEXT): horizalign(left)
    ));
    if open.query.is_empty() {
        actors.push(act!(text:
            font("miso"): settext("type to search"):
            align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
            maxwidth(panel_w - 40.0):
            diffuse(0.5, 0.5, 0.5, 1.0): z(Z_TEXT): horizalign(left)
        ));
    } else {
        let q_chars = open.query.chars().count();
        let ghost = focused_match(open).and_then(|m| {
            let starts = m
                .label
                .to_ascii_lowercase()
                .starts_with(&open.query.to_ascii_lowercase());
            if starts && m.label.chars().count() > q_chars {
                let prefix: String = m.label.chars().take(q_chars).collect();
                Some((m.label.clone(), prefix))
            } else {
                None
            }
        });
        match ghost {
            Some((full_label, prefix)) => {
                // Grey underlay: the full suggested label.
                actors.push(act!(text:
                    font("miso"): settext(full_label):
                    align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
                    maxwidth(panel_w - 40.0):
                    diffuse(0.5, 0.5, 0.5, 1.0): z(Z_TEXT): horizalign(left)
                ));
                // Green overlay: the typed portion, in the label's own casing.
                actors.push(act!(text:
                    font("miso"): settext(prefix):
                    align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
                    maxwidth(panel_w - 40.0):
                    diffuse(0.4, 1.0, 0.4, 1.0): z(Z_TEXT + 1): horizalign(left)
                ));
            }
            None => {
                let caret = if caret_on { "▮" } else { "" };
                actors.push(act!(text:
                    font("miso"): settext(format!("{}{caret}", open.query)):
                    align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
                    maxwidth(panel_w - 40.0):
                    diffuse(0.4, 1.0, 0.4, 1.0): z(Z_TEXT): horizalign(left)
                ));
            }
        }
    }

    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, top + 66.0):
        zoomto(panel_w - 20.0, 1.0):
        diffuse(0.35, 0.35, 0.35, 1.0): z(Z_TEXT)
    ));

    let list_top = top + 84.0;
    let row_step = 21.0;
    let list_x = cx - panel_w * 0.5 + 16.0;
    let pane_x = cx + panel_w * 0.5 - 16.0;
    if open.matches.is_empty() {
        actors.push(act!(text:
            font("miso"): settext("No matches"):
            align(0.0, 0.5): xy(list_x, list_top): zoom(0.8):
            diffuse(0.6, 0.6, 0.6, 1.0): z(Z_TEXT): horizalign(left)
        ));
    }
    let shown = open.matches.len().min(SEARCH_MAX_RESULTS);
    for i in 0..shown {
        let m = &open.matches[i];
        let y = list_top + i as f32 * row_step;
        let focused = i == open.selected_index;
        let (r, g, b) = if focused {
            (theme[0], theme[1], theme[2])
        } else {
            (0.85, 0.85, 0.85)
        };
        let prefix = if focused { "▸ " } else { "  " };
        actors.push(act!(text:
            font("miso"): settext(format!("{prefix}{}", m.label)):
            align(0.0, 0.5): xy(list_x, y): zoom(0.85):
            maxwidth(panel_w * 0.62):
            diffuse(r, g, b, 1.0): z(Z_TEXT): horizalign(left)
        ));
        actors.push(act!(text:
            font("miso"): settext(pane_label(m.pane)):
            align(1.0, 0.5): xy(pane_x, y): zoom(0.7):
            diffuse(0.6, 0.6, 0.75, 1.0): z(Z_TEXT): horizalign(right)
        ));
    }

    // Detail line for the focused match (current value).
    if let Some(m) = focused_match(open) {
        if let Some(value) = current_value(state, m, open.opener_player) {
            actors.push(act!(text:
                font("miso"): settext(format!("Current: {value}")):
                align(0.0, 0.5): xy(list_x, cy + panel_h * 0.5 - 34.0): zoom(0.75):
                maxwidth(panel_w - 32.0):
                diffuse(1.0, 1.0, 1.0, 1.0): z(Z_TEXT): horizalign(left)
            ));
        }
    }

    actors.push(act!(text:
        font("miso"): settext("↑↓ move   ⇥ complete   ⏎ go   esc cancel"):
        align(0.5, 0.5): xy(cx, cy + panel_h * 0.5 - 14.0): zoom(0.7):
        diffuse(0.7, 0.7, 0.7, 1.0): z(Z_TEXT): horizalign(center)
    ));

    Some(actors)
}
