//! Fuzzy "search for a setting" overlay for the Options screen.
//!
//! Ctrl+F opens a live typeahead over every reachable Options entry: the main
//! list and the rows of each submenu it opens, including nested pages such as
//! ArrowCloud Options. Tab accepts the ghost completion and Enter jumps the
//! cursor to the chosen row, opening its submenu through the normal fades so
//! settings applied on leaving a page (Graphics, Input) still apply.

use super::*;
use crate::screens::components::shared::fuzzy;
use crate::screens::components::shared::setting_search::{frame, ranking, text};
use deadlib_present::space::{screen_center_x, screen_center_y};
use std::cell::OnceCell;

/// Maximum number of ranked results shown in the overlay list.
pub(super) const SEARCH_MAX_RESULTS: usize = 8;
const CURSOR_BLINK_PERIOD: f32 = 1.0;

const Z_DIM: i16 = 1450;
const Z_PANEL_BORDER: i16 = 1451;
const Z_PANEL: i16 = 1452;
const Z_TEXT: i16 = 1453;

/// Where a search result lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SearchTarget {
    /// Index into the visible main Options list.
    Main(usize),
    /// Actual row index inside a submenu.
    Row { kind: SubmenuKind, row: usize },
}

/// A single ranked search result.
#[derive(Clone)]
pub(super) struct SettingMatch {
    pub(super) target: SearchTarget,
    pub(super) label: Arc<str>,
    score: i32,
    /// The page the entry sits on, shown right-aligned.
    location: Arc<str>,
    help_entries: &'static [HelpEntry],
    help: OnceCell<Option<Arc<str>>>,
    row_text: ranking::SearchRowText,
    retained_text: text::SearchResultText,
}

/// Live state of the search overlay.
#[derive(Clone)]
pub(super) struct SettingSearchOpen {
    pub(super) query: String,
    query_text: frame::SearchQueryText,
    pub(super) matches: Vec<SettingMatch>,
    pub(super) selected_index: usize,
    blink_t: f32,
}

#[derive(Clone, Default)]
pub(super) enum SettingSearchState {
    #[default]
    Hidden,
    Open(Box<SettingSearchOpen>),
}

impl SettingSearchState {
    #[inline(always)]
    pub(super) const fn is_open(&self) -> bool {
        matches!(self, Self::Open(_))
    }
}

/// The submenu a main Options entry opens, if any. Bookkeeping is left out:
/// its rows are read-only counters, not settings.
const fn item_submenu(id: ItemId) -> Option<SubmenuKind> {
    Some(match id {
        ItemId::ArcadeOptions => SubmenuKind::Coin,
        ItemId::SystemOptions => SubmenuKind::System,
        ItemId::GraphicsOptions => SubmenuKind::Graphics,
        ItemId::InputOptions => SubmenuKind::Input,
        ItemId::LightsOptions => SubmenuKind::Lights,
        ItemId::MachineOptions => SubmenuKind::Machine,
        ItemId::AdvancedOptions => SubmenuKind::Advanced,
        ItemId::CourseOptions => SubmenuKind::Course,
        ItemId::GameplayOptions => SubmenuKind::Gameplay,
        ItemId::TournamentModeOptions => SubmenuKind::Tournament,
        ItemId::SoundOptions => SubmenuKind::Sound,
        ItemId::SelectMusicOptions => SubmenuKind::SelectMusic,
        ItemId::OnlineScoreServices => SubmenuKind::OnlineScoring,
        ItemId::StreamingServices => SubmenuKind::Streaming,
        ItemId::NullOrDieOptions => SubmenuKind::NullOrDie,
        ItemId::FoldersOptions => SubmenuKind::Folders,
        ItemId::Downloads => SubmenuKind::Downloads,
        _ => return None,
    })
}

/// The nested submenu a launcher row opens, mirroring `activate_current_selection`.
const fn row_submenu(kind: SubmenuKind, id: SubRowId) -> Option<SubmenuKind> {
    Some(match (kind, id) {
        (SubmenuKind::Input, SubRowId::InputOptions) => SubmenuKind::InputBackend,
        (SubmenuKind::InputBackend, SubRowId::SmxConfig) => SubmenuKind::SmxConfig,
        (SubmenuKind::OnlineScoring, SubRowId::GsBsOptions) => SubmenuKind::GrooveStats,
        (SubmenuKind::OnlineScoring, SubRowId::ArrowCloudOptions) => SubmenuKind::ArrowCloud,
        (SubmenuKind::OnlineScoring, SubRowId::ScoreImport) => SubmenuKind::ScoreImport,
        (SubmenuKind::NullOrDie, SubRowId::NullOrDieOptions) => SubmenuKind::NullOrDieOptions,
        (SubmenuKind::NullOrDie, SubRowId::SyncPacks) => SubmenuKind::SyncPacks,
        _ => return None,
    })
}

fn push_match(
    matches: &mut Vec<SettingMatch>,
    query: &fuzzy::Query,
    target: SearchTarget,
    raw_label: &Arc<str>,
    location: &Arc<str>,
    help_entries: &'static [HelpEntry],
) {
    let Some((label, score)) = ranking::matched_setting_label(query, raw_label, &[]) else {
        return;
    };
    matches.push(SettingMatch {
        target,
        label,
        score,
        location: Arc::clone(location),
        help_entries,
        help: OnceCell::new(),
        row_text: ranking::SearchRowText::default(),
        retained_text: text::SearchResultText::default(),
    });
}

/// Add the visible rows of `kind`, then those of any nested page it launches.
fn push_submenu_matches(
    state: &State,
    query: &fuzzy::Query,
    kind: SubmenuKind,
    location: &Arc<str>,
    matches: &mut Vec<SettingMatch>,
) {
    let rows = submenu_rows(kind);
    let items = submenu_items(kind);
    for row in submenu_visible_row_indices(state, kind, rows) {
        let Some(sub_row) = rows.get(row) else {
            continue;
        };
        let label = sub_row.label.get();
        let help = items.get(row).map_or(&[][..], |item| item.help);
        push_match(
            matches,
            query,
            SearchTarget::Row { kind, row },
            &label,
            location,
            help,
        );
        if let Some(child) = row_submenu(kind, sub_row.id) {
            push_submenu_matches(state, query, child, &label, matches);
        }
    }
}

/// Build the ranked match list for `query` from every reachable entry.
pub(super) fn rebuild_matches(state: &State, query: &str) -> Vec<SettingMatch> {
    let q = fuzzy::prepare_query(query);
    // Main-list entries are on the page the search opens from; leave their
    // location blank rather than adding an untranslated label.
    let main_location: Arc<str> = Arc::from("");
    let mut matches = Vec::new();
    for (index, item) in visible_items(state).iter().enumerate() {
        if item.id == ItemId::Exit {
            continue;
        }
        let label = item.name.get();
        push_match(
            &mut matches,
            &q,
            SearchTarget::Main(index),
            &label,
            &main_location,
            item.help,
        );
        if let Some(kind) = item_submenu(item.id) {
            push_submenu_matches(state, &q, kind, &label, &mut matches);
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

/// Ctrl+F is a keyboard feature; keyboard-less cabinets never open it.
fn keyboard_features_enabled(state: &State) -> bool {
    get_choice_by_id(
        &state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::KeyboardFeatures,
    )
    .is_some_and(yes_no_from_choice)
}

/// Whether the search can open now: keyboard features on, no page fade in
/// progress and no other overlay or text entry owning input.
pub(super) fn can_open(state: &State) -> bool {
    keyboard_features_enabled(state)
        && !state.search.is_open()
        && matches!(state.submenu_transition, SubmenuTransition::None)
        && state.reload_ui.is_none()
        && state.twitch_channel_entry.is_none()
        && matches!(state.judgment_palette_overlay, JudgmentPaletteOverlayState::Hidden)
        && matches!(state.download_packs_overlay, DownloadPacksOverlayState::Hidden)
        && matches!(state.pack_sync_overlay, shared_pack_sync::OverlayState::Hidden)
        && state.score_import_ui.is_none()
        && state.apply_replaygain_ui.is_none()
        && state.score_import_pack_picker.is_none()
        && state.score_import_confirm.is_none()
        && state.sync_pack_confirm.is_none()
}

/// Open the overlay, seeded with the full list.
pub(super) fn open(state: &mut State) {
    let matches = rebuild_matches(state, "");
    clear_navigation_holds(state);
    state.search = SettingSearchState::Open(Box::new(SettingSearchOpen {
        query: String::new(),
        query_text: frame::SearchQueryText::default(),
        matches,
        selected_index: 0,
        blink_t: 0.0,
    }));
}

/// Close the overlay, clearing holds so nothing leaks into the screen below.
pub(super) fn close(state: &mut State) {
    clear_navigation_holds(state);
    state.search = SettingSearchState::Hidden;
}

/// Advance the caret-blink clock.
pub(super) fn update(search: &mut SettingSearchState, dt: f32) {
    if let SettingSearchState::Open(open) = search {
        open.blink_t = (open.blink_t + dt.max(0.0)) % CURSOR_BLINK_PERIOD;
    }
}

/// Re-rank after the query changed and clamp the selection.
fn refresh(state: &mut State) {
    let SettingSearchState::Open(open) = &state.search else {
        return;
    };
    let matches = rebuild_matches(state, &open.query);
    if let SettingSearchState::Open(open) = &mut state.search {
        open.query_text.invalidate();
        open.matches = matches;
        open.selected_index = open
            .selected_index
            .min(open.matches.len().saturating_sub(1));
    }
}

pub(super) fn add_text(state: &mut State, typed: &str) {
    let SettingSearchState::Open(open) = &mut state.search else {
        return;
    };
    let before = open.query.len();
    open.query.extend(typed.chars().filter(|ch| !ch.is_control()));
    if open.query.len() == before {
        return;
    }
    open.selected_index = 0;
    refresh(state);
}

/// Delete the last query char (no-op on an empty query).
pub(super) fn backspace(state: &mut State) {
    let SettingSearchState::Open(open) = &mut state.search else {
        return;
    };
    if open.query.pop().is_none() {
        return;
    }
    open.selected_index = 0;
    refresh(state);
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
/// Shared by the renderer and `accept_ghost`, so Tab only completes to what is
/// visibly offered.
fn completion(open: &SettingSearchOpen) -> Option<(Arc<str>, Arc<str>)> {
    let m = open.matches.get(open.selected_index)?;
    m.retained_text.completion(&open.query, &m.label)
}

/// Accept the ghost completion (Tab / Right). No-op when none is offered.
pub(super) fn accept_ghost(state: &mut State) {
    let SettingSearchState::Open(open) = &mut state.search else {
        return;
    };
    let Some((label, _)) = completion(open) else {
        return;
    };
    open.query.clear();
    open.query.push_str(&label);
    open.selected_index = 0;
    refresh(state);
}

/// Help text joined to one line; `None` when the entry has none.
fn help_text(m: &SettingMatch) -> Option<Arc<str>> {
    m.help
        .get_or_init(|| {
            let mut joined = String::new();
            for entry in m.help_entries {
                let (HelpEntry::Paragraph(key) | HelpEntry::Bullet(key)) = entry else {
                    continue;
                };
                let line = key.get().replace("\\n", "\n");
                for part in line.lines().map(str::trim).filter(|s| !s.is_empty()) {
                    if !joined.is_empty() {
                        joined.push(' ');
                    }
                    joined.push_str(part);
                }
            }
            (!joined.is_empty()).then(|| Arc::from(joined))
        })
        .clone()
}

/// Jump to the focused result and close the overlay.
pub(super) fn commit(state: &mut State) {
    let target = match &state.search {
        SettingSearchState::Open(open) => open.matches.get(open.selected_index).map(|m| m.target),
        SettingSearchState::Hidden => None,
    };
    close(state);
    if let Some(target) = target {
        queue_sfx(state, "assets/sounds/start.ogg");
        jump(state, target);
    }
}

/// Visible index of `row` within submenu `kind`.
fn visible_row_index(state: &State, kind: SubmenuKind, row: usize) -> Option<usize> {
    submenu_visible_row_indices(state, kind, submenu_rows(kind))
        .iter()
        .position(|&idx| idx == row)
}

/// Run what entering `kind` from its launcher would have run.
fn prepare_submenu(state: &mut State, kind: SubmenuKind) {
    match kind {
        SubmenuKind::NullOrDie | SubmenuKind::NullOrDieOptions | SubmenuKind::SyncPacks => {
            refresh_null_or_die_options(state);
        }
        SubmenuKind::ScoreImport => refresh_score_import_options(state),
        _ => {}
    }
}

fn start_fade_to_submenu(state: &mut State, kind: SubmenuKind) {
    state.pending_submenu_kind = Some(kind);
    state.pending_submenu_parent_kind = submenu_parent_kind_of(kind);
    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
}

pub(super) fn jump(state: &mut State, target: SearchTarget) {
    match (state.view, target) {
        (OptionsView::Main, SearchTarget::Main(index)) => {
            state.selected = index.min(visible_items(state).len().saturating_sub(1));
        }
        (OptionsView::Submenu(current), SearchTarget::Row { kind, row }) if current == kind => {
            if let Some(index) = visible_row_index(state, kind, row) {
                state.sub_selected = index;
                state.sub_inline_x = f32::NAN;
            }
        }
        (OptionsView::Main, SearchTarget::Row { kind, .. }) => {
            prepare_submenu(state, kind);
            state.search_jump = Some(target);
            start_fade_to_submenu(state, kind);
            state.submenu_fade_t = 0.0;
        }
        (OptionsView::Submenu(_), _) => {
            // Leave through the main list: it is where pages such as Graphics
            // apply their deferred changes. `continue_jump_on_main` picks up
            // from there.
            state.search_jump = Some(target);
            state.submenu_transition = SubmenuTransition::FadeOutToMain;
            state.submenu_fade_t = 0.0;
        }
    }
}

/// Called once the fade back to the main list has finished: select the main
/// entry, or carry straight on into the target submenu.
pub(super) fn continue_jump_on_main(state: &mut State) {
    match state.search_jump {
        Some(SearchTarget::Main(index)) => {
            state.search_jump = None;
            state.selected = index.min(visible_items(state).len().saturating_sub(1));
        }
        Some(SearchTarget::Row { kind, .. }) => {
            prepare_submenu(state, kind);
            start_fade_to_submenu(state, kind);
            // The main list is already faded out; switch on the next update.
            state.submenu_fade_t = 1.0;
            state.content_alpha = 0.0;
        }
        None => {}
    }
}

/// Called when a fade into `kind` switches views: land on the searched row.
pub(super) fn take_jump_row(state: &mut State, kind: SubmenuKind) -> Option<usize> {
    let Some(SearchTarget::Row { kind: target, row }) = state.search_jump else {
        return None;
    };
    state.search_jump = None;
    if target == kind {
        visible_row_index(state, kind, row)
    } else {
        None
    }
}

/// Raw keyboard entry: Ctrl+F opens, then typing filters, Tab completes,
/// arrows move, Enter jumps and Escape closes.
pub(super) fn handle_raw_key_event(
    state: &mut State,
    key: Option<&RawKeyboardEvent>,
    typed: Option<&str>,
    ctrl_held: bool,
) -> bool {
    if !state.search.is_open() {
        if ctrl_held
            && key.is_some_and(|k| k.pressed && k.code == KeyCode::KeyF)
            && can_open(state)
        {
            open(state);
            return true;
        }
        return false;
    }
    if let Some(key) = key {
        if key.pressed {
            match key.code {
                KeyCode::Escape => close(state),
                KeyCode::Backspace => backspace(state),
                KeyCode::Tab | KeyCode::ArrowRight => accept_ghost(state),
                KeyCode::ArrowUp => move_selection(state, -1),
                KeyCode::ArrowDown => move_selection(state, 1),
                KeyCode::PageUp => move_selection(state, -(SEARCH_MAX_RESULTS as isize)),
                KeyCode::PageDown => move_selection(state, SEARCH_MAX_RESULTS as isize),
                KeyCode::Enter | KeyCode::NumpadEnter => commit(state),
                _ => {}
            }
        }
        return true;
    }
    // Never type Ctrl-modified text, e.g. a redelivered Ctrl+F.
    if let Some(typed) = typed
        && !ctrl_held
    {
        add_text(state, typed);
    }
    true
}

/// Pad input while the overlay is open: Up/Down move, Start jumps, Back closes.
pub(super) fn handle_input(state: &mut State, ev: &InputEvent) {
    if !ev.pressed {
        return;
    }
    match ev.action {
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => move_selection(state, -1),
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => move_selection(state, 1),
        VirtualAction::p1_start | VirtualAction::p2_start => commit(state),
        VirtualAction::p1_back | VirtualAction::p2_back => close(state),
        _ => {}
    }
}

/// Top-bar note telling keyboard players the search exists.
pub(super) fn push_hint(actors: &mut Vec<Actor>, state: &State) {
    if !keyboard_features_enabled(state) || state.search.is_open() {
        return;
    }
    actors.push(act!(text:
        font("miso"): settext(tr("PlayerOptions", "SettingSearchHint")):
        align(1.0, 0.5): xy(screen_width() - 10.0, 16.0):
        zoom(0.72): z(130):
        diffuse(1.0, 1.0, 1.0, 1.0): horizalign(right)
    ));
}

/// Append overlay actors when the search is open.
pub(super) fn push_overlay(actors: &mut Vec<Actor>, state: &State) {
    let SettingSearchState::Open(open) = &state.search else {
        return;
    };

    let cx = screen_center_x();
    let cy = screen_center_y();
    let panel_w = 420.0_f32.min(screen_width() * 0.92);
    let panel_h = 360.0_f32;
    let top = panel_h.mul_add(-0.5, cy);

    let theme = color::simply_love_rgba(state.active_color_index);
    const PANEL_BG: [f32; 4] = deadlib_present::color::rgba_hex("#071016");
    const FOCUS_BG: [f32; 4] = deadlib_present::color::rgba_hex("#333333");
    const GRAY: [f32; 4] = deadlib_present::color::rgba_hex("#808080");

    actors.reserve(32);
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
        diffuse(PANEL_BG[0], PANEL_BG[1], PANEL_BG[2], 1.0): z(Z_PANEL)
    ));
    actors.push(act!(text:
        font("wendy"): settext(tr("PlayerOptions", "SettingSearchTitle")):
        align(0.5, 0.5): xy(cx, top + 20.0): zoom(0.4):
        maxwidth(panel_w - 24.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_TEXT): horizalign(center)
    ));

    // Query line: the ghost is drawn by laying the full label underneath
    // (gray) and the typed prefix on top (theme color).
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
        actors.push(act!(text:
            font("miso"): settext(tr("PlayerOptions", "SettingSearchPlaceholder")):
            align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
            maxwidth(panel_w - 40.0):
            diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(left)
        ));
    } else if let Some((full_label, prefix)) = completion(open) {
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
    } else {
        actors.push(act!(text:
            font("miso"): settext(open.query_text.get(&open.query, caret_on)):
            align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
            maxwidth(panel_w - 40.0):
            diffuse(theme[0], theme[1], theme[2], 1.0): z(Z_TEXT): horizalign(left)
        ));
    }

    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, top + 66.0):
        zoomto(panel_w - 20.0, 1.0):
        diffuse(GRAY[0], GRAY[1], GRAY[2], 0.5): z(Z_TEXT)
    ));

    let list_top = top + 84.0;
    let row_step = 21.0;
    let list_x = cx - panel_w * 0.5 + 16.0;
    let location_x = cx + panel_w * 0.5 - 16.0;
    if open.matches.is_empty() {
        actors.push(act!(text:
            font("miso"): settext(tr("PlayerOptions", "SettingSearchNoMatches")):
            align(0.0, 0.5): xy(list_x, list_top): zoom(0.8):
            maxwidth(panel_w - 32.0):
            diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(left)
        ));
    }
    for (i, m) in open.matches.iter().take(SEARCH_MAX_RESULTS).enumerate() {
        let y = (i as f32).mul_add(row_step, list_top);
        let focused = i == open.selected_index;
        if focused {
            actors.push(act!(quad:
                align(0.0, 0.5): xy(cx - panel_w * 0.5 + 8.0, y):
                zoomto(panel_w - 16.0, row_step - 2.0):
                diffuse(FOCUS_BG[0], FOCUS_BG[1], FOCUS_BG[2], 1.0): z(Z_TEXT)
            ));
        }
        let rgb = if focused { theme } else { GRAY };
        actors.push(act!(text:
            font("miso"): settext(m.row_text.get(&m.label, focused)):
            align(0.0, 0.5): xy(list_x, y): zoom(0.85):
            maxwidth(panel_w * 0.56):
            diffuse(rgb[0], rgb[1], rgb[2], 1.0): z(Z_TEXT + 1): horizalign(left)
        ));
        actors.push(act!(text:
            font("miso"): settext(Arc::clone(&m.location)):
            align(1.0, 0.5): xy(location_x, y): zoom(0.7):
            maxwidth(panel_w * 0.38):
            diffuse(rgb[0], rgb[1], rgb[2], 1.0): z(Z_TEXT + 1): horizalign(right)
        ));
    }

    // Focused match help, wrapped under the list.
    if let Some(help) = open.matches.get(open.selected_index).and_then(help_text) {
        let help_y = panel_h.mul_add(0.5, cy) - 84.0;
        actors.push(act!(text:
            font("miso"): settext(help):
            align(0.0, 0.0): xy(list_x, help_y): zoom(0.72):
            wrapwidthpixels((panel_w - 32.0) / 0.72):
            diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(left)
        ));
    }

    actors.push(act!(text:
        font("miso"): settext(tr("PlayerOptions", "SettingSearchFooter")):
        align(0.5, 0.5): xy(cx, panel_h.mul_add(0.5, cy) - 14.0): zoom(0.7):
        maxwidth(panel_w - 24.0):
        diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(center)
    ));
}
