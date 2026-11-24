//! Fuzzy "search for a song" overlay for Select Music.
//!
//! A live typeahead: each keystroke re-ranks the catalog, offers a ghost
//! completion, and Enter jumps to the pick. Tab accepts the completion and
//! Shift+Tab switches songs/packs. Song mode still honors the `[###]`
//! BPM/difficulty filter; the old `pack/song` split is gone.
//!
//! The opening shortcut is deliberately not named here: it is configurable, and
//! a second built-in chord opens it too. Both live with the key handling in
//! `select_music`, which is the one place that should spell them out.

use crate::act;
use crate::color;
use crate::fonts::machine_font_key;
use crate::i18n::{tr, tr_fmt};
use crate::screens::components::select_music::push_retained_overlay;
use crate::screens::components::shared::fuzzy;
#[cfg(test)]
use crate::screens::select_music::MusicWheelEntry;
use deadlib_assets::AssetManager;
use deadlib_present::actors::Actor;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_config::theme::MachineFont;
pub use deadsync_simfile::song_search_index::{
    SONG_SEARCH_MAX_RESULTS, SongSearchIndex, SongSearchMatch, SongSearchScope, build_pack_matches,
    build_song_matches, build_song_search_index, clean_search_title,
};
use deadsync_simfile::song_search::parse_song_search_live;
use deadsync_theme::FontRole;
use std::cell::RefCell;
use std::sync::Arc;

mod text;
pub use text::song_search_query_completed_with;
use text::song_search_fit;

pub const SONG_SEARCH_MAX_LEN: usize = 80;
const CURSOR_BLINK_PERIOD: f32 = 1.0;

const Z_DIM: i16 = 1450;
const Z_PANEL_BORDER: i16 = 1451;
const Z_PANEL: i16 = 1452;
const Z_TEXT: i16 = 1453;

fn scope_label(scope: SongSearchScope) -> Arc<str> {
    match scope {
        SongSearchScope::Song => tr("SelectMusic", "SongSearchScopeSongs"),
        SongSearchScope::Pack => tr("SelectMusic", "SongSearchScopePacks"),
    }
}

/// Live state of the open search overlay.
#[derive(Clone, Debug)]
pub struct SongSearchOpen {
    pub query: String,
    pub scope: SongSearchScope,
    pub matches: Vec<SongSearchMatch>,
    pub selected_index: usize,
    pub blink_t: f32,
    pub chart_type: &'static str,
    /// Generation of the latest ranking request; a result is applied only when
    /// its generation matches, so stale off-thread results are discarded.
    pub request_generation: u64,
    /// Generation the current `matches` were ranked for. While this trails
    /// `request_generation` the list belongs to an older query, so acting on it
    /// would pick a row the user never saw for what they typed.
    pub matches_generation: u64,
    /// Set once the first ranking result has been applied, so the overlay only
    /// shows "No matches" after a real result (not during the initial frame).
    pub has_result: bool,
    presentation: RefCell<Option<SongSearchPresentation>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SongSearchPresentationKey {
    scope: SongSearchScope,
    selected_index: usize,
    matches_generation: u64,
    has_result: bool,
    caret_on: bool,
    chart_type: &'static str,
    active_color_index: i32,
    machine_font: MachineFont,
    screen_width_bits: u32,
    screen_height_bits: u32,
}

#[derive(Clone, Debug)]
struct SongSearchPresentation {
    key: SongSearchPresentationKey,
    query: Box<str>,
    children: Arc<[Actor]>,
}

#[derive(Clone, Debug)]
pub enum SongSearchState {
    Hidden,
    Open(SongSearchOpen),
}

impl SongSearchState {
    #[inline(always)]
    #[must_use]
    pub const fn is_open(&self) -> bool {
        matches!(self, Self::Open(_))
    }

    #[inline(always)]
    #[must_use]
    pub const fn is_hidden(&self) -> bool {
        matches!(self, Self::Hidden)
    }
}

/// Open a fresh overlay; the caller populates `matches`.
#[must_use]
pub const fn begin_song_search() -> SongSearchState {
    SongSearchState::Open(SongSearchOpen {
        query: String::new(),
        scope: SongSearchScope::Song,
        matches: Vec::new(),
        selected_index: 0,
        blink_t: 0.0,
        chart_type: "dance-single",
        request_generation: 0,
        matches_generation: 0,
        has_result: false,
        presentation: RefCell::new(None),
    })
}

/// Advance the caret-blink clock; returns whether a redraw is warranted.
pub fn update_song_search(state: &mut SongSearchState, dt: f32) -> bool {
    match state {
        SongSearchState::Hidden => false,
        SongSearchState::Open(open) => {
            open.blink_t = (open.blink_t + dt.max(0.0)) % CURSOR_BLINK_PERIOD;
            true
        }
    }
}

/// Append typed text, filtering control chars and capping the length. Returns
/// whether the query changed: named keys arrive here with a textual form too
/// (Enter is `\r`), and re-ranking on those would reset the highlight.
pub fn song_search_add_text(open: &mut SongSearchOpen, text: &str) -> bool {
    let mut len = open.query.chars().count();
    let mut changed = false;
    for ch in text.chars() {
        if ch.is_control() {
            continue;
        }
        if len >= SONG_SEARCH_MAX_LEN {
            break;
        }
        open.query.push(ch);
        len += 1;
        changed = true;
    }
    changed
}

/// Delete the last query char. Returns whether anything changed.
#[inline(always)]
pub fn song_search_backspace(open: &mut SongSearchOpen) -> bool {
    open.query.pop().is_some()
}

/// Delete trailing whitespace then the word before it. Returns whether it changed.
pub fn song_search_delete_word(open: &mut SongSearchOpen) -> bool {
    let before = open.query.len();
    let end = open
        .query
        .trim_end()
        .trim_end_matches(|c: char| !c.is_whitespace())
        .len();
    open.query.truncate(end);
    end != before
}

#[inline(always)]
#[must_use]
pub fn song_search_shown(open: &SongSearchOpen) -> usize {
    open.matches.len().min(SONG_SEARCH_MAX_RESULTS)
}

/// Move the highlight within the visible window, wrapping around.
pub fn song_search_move(open: &mut SongSearchOpen, delta: isize) -> bool {
    let shown = song_search_shown(open);
    if shown == 0 || delta == 0 {
        return false;
    }
    let cur = open.selected_index.min(shown - 1) as isize;
    let next = (cur + delta).rem_euclid(shown as isize) as usize;
    if next == cur as usize {
        return false;
    }
    open.selected_index = next;
    true
}

#[inline(always)]
#[must_use]
pub fn song_search_focused_match(open: &SongSearchOpen) -> Option<&SongSearchMatch> {
    open.matches.get(open.selected_index)
}

/// A ghost completion offered for the focused match.
pub struct SongSearchCompletion {
    /// Gray text drawn under the query; always starts with `typed`.
    pub display: String,
    /// The typed text, drawn over `display` in theme color.
    pub typed: String,
    /// The query to install when the completion is accepted.
    pub accepted: String,
}

/// The ghost completion, or `None` when none is offered.
///
/// Single source of truth for the renderer and Tab, so Tab can only complete to
/// something visibly offered. The ghost is only offered when the query Tab would
/// install literally extends what the user typed, so completing a title never
/// silently widens the search and never renders text Tab would not produce.
#[must_use]
pub fn song_search_completion(open: &SongSearchOpen) -> Option<SongSearchCompletion> {
    if open.query.is_empty() {
        return None;
    }
    let text = parse_song_search_live(&open.query).text;
    if text.is_empty() {
        return None;
    }
    let label = song_search_focused_match(open)?.label();
    // The ghost is drawn as gray text *under* the typed text, so it can only be
    // offered when it literally extends what is on screen. Testing the raw query
    // against the query Tab would install keeps the two honest: a trailing space
    // or a trailing `[###]` token means the accepted query reorders what the user
    // typed, and no ghost can truthfully be drawn over that.
    let accepted = song_search_query_completed_with(&open.query, &label);
    let consumed = fuzzy::folded_prefix_len(&open.query, &accepted)?;
    let (split, _) = accepted.char_indices().nth(consumed)?;
    let remainder = &accepted[split..];
    let mut display = String::with_capacity(open.query.len() + remainder.len());
    display.push_str(&open.query);
    display.push_str(remainder);
    Some(SongSearchCompletion {
        display,
        typed: open.query.clone(),
        accepted,
    })
}

/// Append the overlay actors, returning whether the search is visible.
pub fn push_song_search_overlay(
    actors: &mut Vec<Actor>,
    state: &SongSearchState,
    active_color_index: i32,
    machine_font: MachineFont,
    asset_manager: &AssetManager,
) -> bool {
    let SongSearchState::Open(open) = state else {
        return false;
    };

    let key = SongSearchPresentationKey {
        scope: open.scope,
        selected_index: open.selected_index,
        matches_generation: open.matches_generation,
        has_result: open.has_result,
        caret_on: open.blink_t < CURSOR_BLINK_PERIOD * 0.5,
        chart_type: open.chart_type,
        active_color_index,
        machine_font,
        screen_width_bits: screen_width().to_bits(),
        screen_height_bits: screen_height().to_bits(),
    };
    let cached = open
        .presentation
        .borrow()
        .as_ref()
        .filter(|presentation| presentation.key == key && presentation.query.as_ref() == open.query)
        .map(|presentation| Arc::clone(&presentation.children));
    let children = cached.unwrap_or_else(|| {
        let mut children = Vec::with_capacity(48);
        push_song_search_overlay_unreserved(
            &mut children,
            open,
            active_color_index,
            machine_font,
            asset_manager,
        );
        let children = Arc::<[Actor]>::from(children);
        *open.presentation.borrow_mut() = Some(SongSearchPresentation {
            key,
            query: open.query.clone().into_boxed_str(),
            children: Arc::clone(&children),
        });
        children
    });
    push_retained_overlay(actors, children);
    true
}

fn push_song_search_overlay_unreserved(
    actors: &mut Vec<Actor>,
    open: &SongSearchOpen,
    active_color_index: i32,
    machine_font: MachineFont,
    asset_manager: &AssetManager,
) {
    let cx = screen_center_x();
    let cy = screen_center_y();
    let panel_w = 380.0_f32.min(screen_width() * 0.92);
    let panel_h = 404.0_f32;
    let top = panel_h.mul_add(-0.5, cy);

    let theme = color::simply_love_rgba(active_color_index);
    const PANEL_BG: [f32; 4] = deadlib_present::color::rgba_hex("#071016");
    const FOCUS_BG: [f32; 4] = deadlib_present::color::rgba_hex("#333333");
    const GRAY: [f32; 4] = deadlib_present::color::rgba_hex("#808080");
    const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

    // Dim behind the modal.
    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.8): z(Z_DIM)
    ));
    // Panel border + fill.
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

    // Title + scope badge.
    actors.push(act!(text:
        font(machine_font_key(machine_font, FontRole::Header)):
        settext(tr("SelectMusic", "SongSearchTitle")):
        align(0.0, 0.5): xy(cx - panel_w * 0.5 + 14.0, top + 20.0): zoom(0.42):
        diffuse(WHITE[0], WHITE[1], WHITE[2], 1.0): z(Z_TEXT): horizalign(left)
    ));
    actors.push(act!(text:
        font("miso"): settext(format!("[ {} ]", scope_label(open.scope))):
        align(1.0, 0.5): xy(cx + panel_w * 0.5 - 14.0, top + 20.0): zoom(0.85):
        diffuse(theme[0], theme[1], theme[2], 1.0): z(Z_TEXT): horizalign(right)
    ));

    // Query line: prompt + typed text with an inline ghost completion. The ghost
    // is drawn by laying the completed title underneath (gray) and the typed
    // text on top (theme color), so the untyped remainder shows through.
    let caret_on = open.blink_t < CURSOR_BLINK_PERIOD * 0.5;
    let query_y = top + 48.0;
    let query_x = cx - panel_w * 0.5 + 14.0;
    let text_x = query_x + 14.0;
    actors.push(act!(text:
        font("miso"): settext("> "):
        align(0.0, 0.5): xy(query_x, query_y): zoom(0.9):
        diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(left)
    ));
    if open.query.is_empty() {
        let placeholder = match open.scope {
            SongSearchScope::Song => tr("SelectMusic", "SongSearchPlaceholderSongs"),
            SongSearchScope::Pack => tr("SelectMusic", "SongSearchPlaceholderPacks"),
        };
        actors.push(act!(text:
            font("miso"): settext(placeholder):
            align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
            maxwidth(panel_w - 44.0):
            diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(left)
        ));
    } else {
        match song_search_completion(open) {
            Some(completion) => {
                // `maxwidth` scales each actor by its own measured width, so a
                // ghost wider than the box would shrink out from under the
                // typed text. Trim both to what fits instead.
                let budget = panel_w - 44.0;
                let display =
                    song_search_fit(asset_manager, &completion.display, budget, 0.9).to_owned();
                let typed: String = completion
                    .typed
                    .chars()
                    .take(display.chars().count())
                    .collect();
                actors.push(act!(text:
                    font("miso"): settext(display):
                    align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
                    diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(left)
                ));
                actors.push(act!(text:
                    font("miso"): settext(typed):
                    align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
                    diffuse(theme[0], theme[1], theme[2], 1.0): z(Z_TEXT + 1): horizalign(left)
                ));
            }
            None => {
                let caret = if caret_on { "▮" } else { "" };
                actors.push(act!(text:
                    font("miso"): settext(format!("{}{caret}", open.query)):
                    align(0.0, 0.5): xy(text_x, query_y): zoom(0.9):
                    maxwidth(panel_w - 44.0):
                    diffuse(theme[0], theme[1], theme[2], 1.0): z(Z_TEXT): horizalign(left)
                ));
            }
        }
    }

    // Divider.
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, top + 68.0):
        zoomto(panel_w - 20.0, 1.0):
        diffuse(GRAY[0], GRAY[1], GRAY[2], 0.5): z(Z_TEXT)
    ));

    // Results list.
    let list_top = top + 86.0;
    let row_step = 20.0;
    let list_x = cx - panel_w * 0.5 + 16.0;
    let count_x = cx + panel_w * 0.5 - 16.0;
    if open.matches.is_empty() && open.has_result {
        actors.push(act!(text:
            font("miso"): settext(tr("SelectMusic", "SongSearchNoMatches")):
            align(0.0, 0.5): xy(list_x, list_top): zoom(0.8):
            diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(left)
        ));
    }
    let shown = song_search_shown(open);
    for i in 0..shown {
        let m = &open.matches[i];
        let y = (i as f32).mul_add(row_step, list_top);
        let focused = i == open.selected_index;
        if focused {
            actors.push(act!(quad:
                align(0.0, 0.5): xy(cx - panel_w * 0.5 + 8.0, y):
                zoomto(panel_w - 16.0, row_step - 2.0):
                diffuse(FOCUS_BG[0], FOCUS_BG[1], FOCUS_BG[2], 1.0): z(Z_TEXT)
            ));
        }
        let rgb = if focused {
            [theme[0], theme[1], theme[2]]
        } else {
            [GRAY[0], GRAY[1], GRAY[2]]
        };
        let prefix = if focused { "▸ " } else { "  " };
        actors.push(act!(text:
            font("miso"): settext(format!("{prefix}{}", m.label())):
            align(0.0, 0.5): xy(list_x, y): zoom(0.85):
            maxwidth(panel_w * 0.66):
            diffuse(rgb[0], rgb[1], rgb[2], 1.0): z(Z_TEXT + 1): horizalign(left)
        ));
        if let SongSearchMatch::Pack { song_count, .. } = m {
            actors.push(act!(text:
                font("miso"): settext(format!("{song_count}")):
                align(1.0, 0.5): xy(count_x, y): zoom(0.75):
                diffuse(rgb[0], rgb[1], rgb[2], 1.0): z(Z_TEXT + 1): horizalign(right)
            ));
        }
    }

    // Detail block for the focused match; values are precomputed when ranked.
    if let Some(m) = song_search_focused_match(open) {
        let detail_top = panel_h.mul_add(0.5, cy) - 96.0;
        let details: Vec<(Arc<str>, Arc<str>)> = match m {
            SongSearchMatch::Song { candidate, .. } => {
                let mut rows = vec![(
                    tr("SelectMusic", "SongSearchLabelPack"),
                    Arc::clone(&candidate.pack_name),
                )];
                if !candidate.subtitle.trim().is_empty() {
                    rows.push((
                        tr("SelectMusic", "SongSearchLabelSubtitle"),
                        Arc::clone(&candidate.subtitle),
                    ));
                }
                rows.push((
                    tr("SelectMusic", "SongSearchLabelBpms"),
                    Arc::clone(&candidate.bpm),
                ));
                rows.push((
                    tr("SelectMusic", "SongSearchLabelDifficulties"),
                    Arc::clone(&candidate.difficulties),
                ));
                rows
            }
            SongSearchMatch::Pack {
                name, song_count, ..
            } => vec![
                (tr("SelectMusic", "SongSearchLabelPack"), Arc::clone(name)),
                (
                    tr("SelectMusic", "SongSearchLabelSongs"),
                    Arc::from(song_count.to_string()),
                ),
            ],
        };
        for (i, (label, value)) in details.iter().enumerate() {
            let y = (i as f32).mul_add(16.0, detail_top);
            actors.push(act!(text:
                font("miso"): settext(format!("{label}:")):
                align(0.0, 0.5): xy(list_x, y): zoom(0.75):
                maxwidth(70.0 / 0.75):
                diffuse(0.67, 0.67, 1.0, 1.0): z(Z_TEXT): horizalign(left)
            ));
            actors.push(act!(text:
                font("miso"): settext(value.clone()):
                align(0.0, 0.5): xy(list_x + 74.0, y): zoom(0.75):
                maxwidth((panel_w - 108.0) / 0.75):
                diffuse(WHITE[0], WHITE[1], WHITE[2], 1.0): z(Z_TEXT): horizalign(left)
            ));
        }
    }

    // Footer hints.
    let scope_hint = match open.scope {
        SongSearchScope::Song => tr("SelectMusic", "SongSearchToggleToPacks"),
        SongSearchScope::Pack => tr("SelectMusic", "SongSearchToggleToSongs"),
    };
    actors.push(act!(text:
        font("miso"):
        settext(tr_fmt("SelectMusic", "SongSearchFooter", &[("scope", &scope_hint)])):
        align(0.5, 0.5): xy(cx, panel_h.mul_add(0.5, cy) - 14.0): zoom(0.62):
        maxwidth(panel_w - 20.0):
        diffuse(GRAY[0], GRAY[1], GRAY[2], 1.0): z(Z_TEXT): horizalign(center)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::{ChartData, SongData};
    use std::path::PathBuf;

    fn test_chart(meter: u32) -> ChartData {
        ChartData {
            radar_numerators: [0.0; 5],
            chart_type: "dance-single".to_string(),
            difficulty: "hard".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter,
            step_artist: String::new(),
            music_file: String::new(),
            music_path: None,
            short_hash: String::new(),
            stats: deadsync_chart::ArrowStats::default(),
            tech_counts: deadsync_chart::TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: deadsync_chart::StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            matrix_profile: Box::default(),
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: false,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
        }
    }

    fn test_song(title: &str, bpm: f64) -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: PathBuf::from("test.sm"),
            title: title.to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            translit_artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: format!("{bpm}"),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: bpm,
            max_bpm: bpm,
            normalized_bpms: format!("{bpm}"),
            song_timing: None,
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            precise_note_first_second_seconds: None,
            precise_note_last_second_seconds: None,
            last_second_hint: 0.0,
            charts: vec![test_chart(10)],
        })
    }

    fn index_from<'a>(songs: &'a [(&'a str, Arc<SongData>)]) -> SongSearchIndex {
        let mut wheel: Vec<MusicWheelEntry> = Vec::new();
        let mut current: Option<&str> = None;
        for (pack, song) in songs {
            if current != Some(pack) {
                wheel.push(MusicWheelEntry::PackHeader {
                    name: Arc::from(*pack),
                    original_index: 0,
                    banner_path: None,
                    song_count: songs.iter().filter(|(p, _)| p == pack).count(),
                    pack_key: Some(Arc::from(*pack)),
                    parent_series: None,
                });
                current = Some(pack);
            }
            wheel.push(MusicWheelEntry::Song(song.clone()));
        }
        build_song_search_index(&wheel)
    }

    #[test]
    fn annotated_title_autocompletes_to_clean_name() {
        let songs = vec![("ITL Online 2026", test_song("[6998] [12] automate", 175.0))];
        let index = index_from(&songs);
        let matches = build_song_matches(&index, "auto", "dance-single");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].label(), "automate");
    }

    #[test]
    fn delete_word_removes_trailing_word_and_whitespace() {
        let mut open = match begin_song_search() {
            SongSearchState::Open(open) => open,
            _ => unreachable!(),
        };
        open.query = "hello world".to_string();
        assert!(song_search_delete_word(&mut open));
        assert_eq!(open.query, "hello ");
        // Second delete clears the remaining word.
        assert!(song_search_delete_word(&mut open));
        assert_eq!(open.query, "");
        assert!(!song_search_delete_word(&mut open));
    }

    #[test]
    fn completion_folds_accents_and_splits_the_original_title() {
        let songs = vec![("Pack", test_song("Déjà Vu", 175.0))];
        let index = index_from(&songs);
        let mut open = match begin_song_search() {
            SongSearchState::Open(open) => open,
            _ => unreachable!(),
        };
        open.query = "deja".to_string();
        open.matches = build_song_matches(&index, "deja", "dance-single");
        open.selected_index = 0;

        let completion = song_search_completion(&open).expect("ghost for an accented title");
        // The split must land on the original title's chars, not the folded
        // ones, or the ghost duplicates or drops a letter.
        assert_eq!(completion.display, "deja Vu");
        assert_eq!(completion.accepted, "Déjà Vu");
        assert!(completion.display.starts_with(&completion.typed));
    }

    /// The ghost is drawn under the typed text, so `display` must literally begin
    /// with what the user typed and must agree with what Tab installs. A trailing
    /// space or a trailing `[###]` token makes the accepted query reorder the
    /// input, and the ghost has to decline rather than render a phantom title.
    #[test]
    fn completion_display_always_extends_the_typed_query() {
        let songs = vec![("ITL Online 2026", test_song("automate", 175.0))];
        let index = index_from(&songs);
        let mut open = match begin_song_search() {
            SongSearchState::Open(open) => open,
            _ => unreachable!(),
        };

        for (query, expected) in [
            ("auto", Some(("automate", "automate"))),
            ("[10] auto", Some(("[10] automate", "[10] automate"))),
            // Reordered by acceptance, so no honest ghost exists.
            ("auto ", None),
            ("auto [10]", None),
        ] {
            open.query = query.to_string();
            open.matches = build_song_matches(&index, query, "dance-single");
            open.selected_index = 0;
            match (song_search_completion(&open), expected) {
                (Some(completion), Some((display, accepted))) => {
                    assert_eq!(completion.display, display, "display for {query:?}");
                    assert_eq!(completion.accepted, accepted, "accepted for {query:?}");
                    assert_eq!(completion.typed, query, "typed for {query:?}");
                    assert!(
                        completion.display.starts_with(&completion.typed),
                        "ghost for {query:?} does not extend the typed text"
                    );
                }
                (None, None) => {}
                (Some(completion), None) => {
                    panic!(
                        "expected no ghost for {query:?}, got {:?}",
                        completion.display
                    )
                }
                (None, Some((display, _))) => {
                    panic!("expected ghost {display:?} for {query:?}, got none")
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "song_search/data_paths_perf.rs"]
mod data_paths_perf;

#[cfg(test)]
#[path = "song_search/menu_buffers_perf.rs"]
mod menu_buffers_perf;
