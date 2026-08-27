//! Fuzzy "search for a setting" overlay for Player Options.
//!
//! Ctrl+F opens a live typeahead: each keystroke re-ranks the visible settings
//! across all four panes, offers a ghost completion (Tab accepts), and Enter
//! jumps the cursor to the chosen row. Only visible rows are indexed, so hidden
//! sub-options never appear as dead-end results.

use super::*;
use crate::screens::components::shared::fuzzy;
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
    pub label: Arc<str>,
    pub score: i32,
    /// Actor-ready labels prepared at the query-change boundary. Index zero is
    /// the ordinary row and index one is the focused row.
    row_text: [Arc<str>; 2],
    pane_text: Arc<str>,
}

impl SettingMatch {
    fn new(row_id: RowId, pane: OptionsPane, label: String, score: i32) -> Self {
        let label: Arc<str> = label.into();
        Self {
            row_id,
            pane,
            score,
            row_text: [
                Arc::from(format!("  {label}")),
                Arc::from(format!("▸ {label}")),
            ],
            pane_text: pane_label(pane),
            label,
        }
    }
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
    pub(super) const fn is_open(&self) -> bool {
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

/// English synonym keywords matched alongside the localized label, so queries
/// like "cmod" or "arrows" resolve. Lives here because `fuzzy` is domain-agnostic.
const fn row_aliases(id: RowId) -> &'static [&'static str] {
    match id {
        RowId::SpeedMod => &["speed", "cmod", "mmod", "xmod", "bpm", "rate"],
        RowId::TypeOfSpeedMod => &["speed type", "cmod", "mmod", "xmod"],
        RowId::NoteSkin => &["arrows", "skin", "notes"],
        RowId::MineSkin => &["mines", "bombs"],
        RowId::ReceptorSkin => &["receptors", "targets"],
        RowId::BackgroundFilter => &["bg", "background", "darken", "brightness"],
        RowId::Perspective => &["tilt", "hallway", "incoming", "overhead"],
        RowId::Mini => &["small", "size", "zoom"],
        RowId::MusicRate => &["rate", "speed", "tempo", "haste"],
        RowId::VisualDelay => &["offset", "delay", "sync"],
        RowId::GlobalOffsetShift => &["offset", "sync", "global"],
        RowId::Hide => &["hide", "hidden", "targets", "danger"],
        RowId::Scroll => &["reverse", "split", "cross", "centered"],
        RowId::Turn => &["mirror", "left", "right", "shuffle"],
        RowId::ErrorBar => &["error bar", "timing", "offset"],
        RowId::MeasureCounter => &["measure", "counter", "stream"],
        RowId::LifeMeterType => &["life", "health", "bar"],
        RowId::JudgmentFont => &["judgment", "judgement", "font"],
        RowId::ComboFont => &["combo", "font"],
        RowId::HeartRateMonitor => &["heart rate", "hr", "bpm"],
        _ => &[],
    }
}

/// Build the ranked match list for `query` from the currently-visible rows.
pub(super) fn rebuild_matches(state: &State, query: &str) -> Vec<SettingMatch> {
    let q = fuzzy::prepare_query(query);
    let active = state.active;
    let mut seen = [false; RowId::COUNT];
    let mut matches: Vec<SettingMatch> = Vec::new();

    for pane in SEARCH_PANE_ORDER {
        let row_map = &state.panes[pane.index()].row_map;
        let visibility =
            visibility::row_visibility(row_map, active, state.option_masks, state.policy);
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
                matches.push(SettingMatch::new(id, pane, label, 0));
            } else if let Some(score) =
                fuzzy::best_match_score(&q, &fuzzy::fold_diacritics(&label), row_aliases(id))
            {
                matches.push(SettingMatch::new(id, pane, label, score));
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
///
/// Clears hold state like `switch_to_pane` does: the overlay swallows input, so
/// a direction held as it opens would never see its release and stay stuck.
pub(super) fn open(state: &mut State, opener_player: usize) {
    let matches = rebuild_matches(state, "");
    state.nav_input = [PlayerNavInput::default(); PLAYER_SLOTS];
    state.start_input = [PlayerStartInput::default(); PLAYER_SLOTS];
    state.search = SettingSearchState::Open(SettingSearchOpen {
        query: String::new(),
        matches,
        selected_index: 0,
        blink_t: 0.0,
        opener_player,
    });
}

/// Close the overlay, clearing hold state so nothing leaks into the screen below.
pub(super) fn close(state: &mut State) {
    state.nav_input = [PlayerNavInput::default(); PLAYER_SLOTS];
    state.start_input = [PlayerStartInput::default(); PLAYER_SLOTS];
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

/// Ghost completion for the focused match: `(full_label, typed_prefix)`.
///
/// Single source of truth shared by the renderer and `accept_ghost`, so Tab can
/// only complete to something visibly offered. Prefix extensions only — alias
/// matches (e.g. "arrows" focusing "NoteSkin") deliberately offer none.
pub(super) fn completion(open: &SettingSearchOpen) -> Option<(String, String)> {
    if open.query.is_empty() {
        return None;
    }
    let m = focused_match(open)?;
    let consumed = fuzzy::folded_prefix_len(&open.query, &m.label)?;
    (m.label.chars().count() > consumed).then(|| {
        let prefix: String = m.label.chars().take(consumed).collect();
        (m.label.to_string(), prefix)
    })
}

/// Accept the ghost completion (Tab / →). No-op when none is offered.
pub(super) fn accept_ghost(state: &mut State) {
    let label = match &state.search {
        SettingSearchState::Open(open) => completion(open).map(|(label, _)| label),
        SettingSearchState::Hidden => None,
    };
    let Some(label) = label else {
        return;
    };
    if let SettingSearchState::Open(open) = &mut state.search {
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

/// Row help text joined to one line; `None` when the row has none.
pub(super) fn help_text(state: &State, m: &SettingMatch) -> Option<String> {
    let row = state.panes[m.pane.index()].row_map.get(m.row_id)?;
    let text = row
        .help
        .iter()
        .map(|line| line.text.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() { None } else { Some(text) }
}

fn pane_label(pane: OptionsPane) -> Arc<str> {
    // Reuse existing translated pane names instead of English-only new keys.
    let key = match pane {
        OptionsPane::Main => "WhatComesNextMainModifiers",
        OptionsPane::Display => "WhatComesNextDisplayModifiers",
        OptionsPane::Advanced => "WhatComesNextAdvancedModifiers",
        OptionsPane::Uncommon => "WhatComesNextUncommonModifiers",
    };
    tr("PlayerOptions", key)
}

/// Append overlay actors when the search is visible.
pub(super) fn push_overlay(actors: &mut Vec<Actor>, state: &State) {
    let SettingSearchState::Open(open) = &state.search else {
        return;
    };

    let cx = screen_center_x();
    let cy = screen_center_y();
    let panel_w = 360.0_f32.min(screen_width() * 0.92);
    let panel_h = 360.0_f32;
    let top = cy - panel_h * 0.5;

    // Theme-native palette, matching the options screen and option rows.
    let theme = color::simply_love_rgba(state.active_color_index);
    const PANEL_BG: [f32; 4] = color::rgba_hex("#071016");
    const FOCUS_BG: [f32; 4] = color::rgba_hex("#333333");
    const GRAY: [f32; 4] = color::rgba_hex("#808080");
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

    let title = tr("PlayerOptions", "SettingSearchTitle");
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
        let placeholder = tr("PlayerOptions", "SettingSearchPlaceholder");
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
    let shown = open.matches.len().min(SEARCH_MAX_RESULTS);
    for i in 0..shown {
        let m = &open.matches[i];
        let y = list_top + i as f32 * row_step;
        let focused = i == open.selected_index;
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
        actors.push(act!(text:
            font("miso"): settext(Arc::clone(&m.row_text[usize::from(focused)])):
            align(0.0, 0.5): xy(list_x, y): zoom(0.85):
            maxwidth(panel_w * 0.62):
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
        let value_y = cy + panel_h * 0.5 - 74.0;
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
        if let Some(help) = help_text(state, m) {
            actors.push(act!(text:
                font("miso"): settext(help):
                align(0.0, 0.0): xy(list_x, value_y + 14.0): zoom(0.72):
                wrapwidthpixels((panel_w - 32.0) / 0.72):
                diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(left)
            ));
        }
    }

    let footer = tr("PlayerOptions", "SettingSearchFooter");
    actors.push(act!(text:
        font("miso"): settext(footer):
        align(0.5, 0.5): xy(cx, cy + panel_h * 0.5 - 14.0): zoom(0.7):
        maxwidth(panel_w - 24.0):
        diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(center)
    ));
}

#[cfg(any(test, feature = "bench-support"))]
fn search_text_checksum(text: &str) -> u64 {
    text.bytes().fold(text.len() as u64, |checksum, byte| {
        checksum.rotate_left(5) ^ u64::from(byte)
    })
}

/// Stable-frame workload for the eight visible search rows.
#[cfg(any(test, feature = "bench-support"))]
pub struct PlayerOptionsSearchBenchmark {
    matches: Vec<SettingMatch>,
    focused: usize,
}

#[cfg(any(test, feature = "bench-support"))]
impl PlayerOptionsSearchBenchmark {
    pub fn new() -> Self {
        const LABELS: [&str; SEARCH_MAX_RESULTS] = [
            "Speed Mod",
            "NoteSkin",
            "Background Filter",
            "Perspective",
            "Music Rate",
            "Visual Delay",
            "Error Bar",
            "Judgment Font",
        ];
        let matches = LABELS
            .into_iter()
            .enumerate()
            .map(|(index, label)| {
                SettingMatch::new(
                    RowId::SpeedMod,
                    SEARCH_PANE_ORDER[index % SEARCH_PANE_ORDER.len()],
                    label.to_owned(),
                    index as i32,
                )
            })
            .collect();
        Self {
            matches,
            focused: 3,
        }
    }

    pub fn legacy_frame(&self) -> u64 {
        self.matches
            .iter()
            .enumerate()
            .fold(0, |checksum, (index, item)| {
                let prefix = if index == self.focused { "▸ " } else { "  " };
                let row = format!("{prefix}{}", item.label);
                let pane = pane_label(item.pane).to_string();
                checksum.rotate_left(11)
                    ^ search_text_checksum(&row)
                    ^ search_text_checksum(&pane).rotate_left(23)
            })
    }

    pub fn current_frame(&self) -> u64 {
        self.matches
            .iter()
            .enumerate()
            .fold(0, |checksum, (index, item)| {
                let row = Arc::clone(&item.row_text[usize::from(index == self.focused)]);
                let pane = Arc::clone(&item.pane_text);
                checksum.rotate_left(11)
                    ^ search_text_checksum(&row)
                    ^ search_text_checksum(&pane).rotate_left(23)
            })
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for PlayerOptionsSearchBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
pub(super) fn build_overlay_legacy(state: &State) -> Option<Vec<Actor>> {
    if !state.search.is_open() {
        return None;
    }
    let mut actors = Vec::with_capacity(32);
    push_overlay(&mut actors, state);
    Some(actors)
}
