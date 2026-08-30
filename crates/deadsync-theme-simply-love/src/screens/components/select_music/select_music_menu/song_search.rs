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
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::{AssetManager, FontRole, machine_font_key};
use crate::config::MachineFont;
use crate::screens::components::select_music::push_retained_overlay;
use crate::screens::components::shared::fuzzy;
use crate::screens::select_music::MusicWheelEntry;
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_chart::SongData;
use deadsync_simfile::song_search::{
    SongSearchCandidate, parse_song_search_live, song_passes_search_filters,
    song_search_difficulties_text,
};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::sync::Arc;

pub const SONG_SEARCH_MAX_LEN: usize = 80;
pub const SONG_SEARCH_MAX_RESULTS: usize = 9;
const CURSOR_BLINK_PERIOD: f32 = 1.0;

const Z_DIM: i16 = 1450;
const Z_PANEL_BORDER: i16 = 1451;
const Z_PANEL: i16 = 1452;
const Z_TEXT: i16 = 1453;

/// Which catalog the overlay is ranking against. Toggled with `Tab`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongSearchScope {
    Song,
    Pack,
}

impl SongSearchScope {
    #[inline(always)]
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Song => Self::Pack,
            Self::Pack => Self::Song,
        }
    }

    /// Localized scope badge text.
    fn label(self) -> Arc<str> {
        match self {
            Self::Song => tr("SelectMusic", "SongSearchScopeSongs"),
            Self::Pack => tr("SelectMusic", "SongSearchScopePacks"),
        }
    }
}

/// Difficulty words dropped when they stand alone in parentheses, e.g. `(Easy)`.
const DIFFICULTY_WORDS: [&str; 8] = [
    "beginner",
    "easy",
    "medium",
    "hard",
    "challenge",
    "edit",
    "expert",
    "basic",
];

/// Zero-width space / BOM, which some packs prepend to `#TITLE`.
#[inline]
const fn is_title_pad(c: char) -> bool {
    c.is_whitespace() || c == '\u{200b}' || c == '\u{feff}'
}

/// Non-numeric bracket tags ITL packs prepend to titles.
const STRIPPABLE_BRACKET_TAGS: [&str; 1] = ["mix"];

/// Whether a leading `[...]` group is an annotation: all digits, or a known tag.
#[inline]
fn is_strippable_bracket(inner: &str) -> bool {
    let inner = inner.trim();
    if inner.is_empty() {
        return false;
    }
    inner.bytes().all(|b| b.is_ascii_digit())
        || STRIPPABLE_BRACKET_TAGS
            .iter()
            .any(|tag| inner.eq_ignore_ascii_case(tag))
}

/// Strip search-irrelevant annotations from a raw `#TITLE`.
///
/// ITL packs prefix titles with a points bucket and level, so
/// `[6998] [12] automate` must search as `automate`. Stripping is deliberately
/// conservative: anything that is not a leading annotation is part of the real
/// title. Falls back to the original if cleaning would empty it.
pub fn clean_search_title(title: &str) -> String {
    let mut rest = title.trim_matches(is_title_pad);

    loop {
        let trimmed = rest.trim_start_matches(is_title_pad);
        let Some(inner_start) = trimmed.strip_prefix('[') else {
            rest = trimmed;
            break;
        };
        let Some(close) = inner_start.find(']') else {
            rest = trimmed;
            break;
        };
        if !is_strippable_bracket(&inner_start[..close]) {
            rest = trimmed;
            break;
        }
        rest = &inner_start[close + 1..];
    }

    let without_diff = strip_difficulty_parens(rest.trim());
    if without_diff.is_empty() {
        title.trim_matches(is_title_pad).to_string()
    } else {
        without_diff
    }
}

/// Remove `(...)` groups that are exactly a difficulty word.
fn strip_difficulty_parens(input: &str) -> String {
    if !input.contains('(') {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(open) = rest.find('(') {
        let Some(close_rel) = rest[open + 1..].find(')') else {
            break;
        };
        let close = open + 1 + close_rel;
        let inner = rest[open + 1..close].trim();
        if DIFFICULTY_WORDS
            .iter()
            .any(|word| inner.eq_ignore_ascii_case(word))
        {
            out.push_str(&rest[..open]);
        } else {
            out.push_str(&rest[..=close]);
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A single ranked result.
#[derive(Clone, Debug)]
pub enum SongSearchMatch {
    Song {
        candidate: SongSearchCandidate,
        score: i32,
    },
    Pack {
        name: Arc<str>,
        song_count: usize,
        score: i32,
    },
}

impl SongSearchMatch {
    /// The completion / list label for this match.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Song { candidate, .. } => clean_search_title(candidate.song.display_title(false)),
            Self::Pack { name, .. } => name.to_string(),
        }
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
    let mut chars: Vec<char> = open.query.chars().collect();
    let before = chars.len();
    while chars.last().is_some_and(|c| c.is_whitespace()) {
        chars.pop();
    }
    while chars.last().is_some_and(|c| !c.is_whitespace()) {
        chars.pop();
    }
    if chars.len() == before {
        return false;
    }
    open.query = chars.into_iter().collect();
    true
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
    let remainder: String = accepted.chars().skip(consumed).collect();
    if remainder.is_empty() {
        return None;
    }
    Some(SongSearchCompletion {
        display: format!("{}{remainder}", open.query),
        typed: open.query.clone(),
        accepted,
    })
}

/// Rebuild `query` so its free text becomes `label`, keeping the `[###]` filter
/// tokens verbatim (a BPM token cannot be rebuilt from the parsed tier).
#[must_use]
pub fn song_search_query_completed_with(query: &str, label: &str) -> String {
    let mut out = String::new();
    let mut chars = query.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '[' {
            continue;
        }
        let mut tail = chars.clone();
        let mut token = String::from('[');
        let mut has_digit = false;
        while let Some(&d) = tail.peek() {
            if !d.is_ascii_digit() {
                break;
            }
            has_digit = true;
            token.push(d);
            tail.next();
        }
        if has_digit && tail.peek() == Some(&']') {
            tail.next();
            token.push(']');
            out.push_str(&token);
            out.push(' ');
            chars = tail;
        }
    }
    out.push_str(label);
    out.chars().take(SONG_SEARCH_MAX_LEN).collect()
}

/// Longest prefix of `text` that fits `max_w`, so overlaid ghost actors can be
/// drawn without `maxwidth` and therefore never scale away from each other.
fn song_search_fit(asset_manager: &AssetManager, text: &str, max_w: f32, zoom: f32) -> String {
    let width = |s: &str| -> f32 {
        let mut out = 0.0_f32;
        asset_manager.with_fonts(|all_fonts| {
            asset_manager.with_font("miso", |font| {
                out = deadlib_present::font::measure_line_width_logical(font, s, all_fonts) as f32
                    * zoom;
            });
        });
        out
    };
    if !width(text).is_finite() || width(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect();
        if width(&candidate) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo].iter().collect()
}

/// Titles cleaned once from the wheel catalog, so keystrokes only score.
///
/// Titles keep their original case for word-boundary scoring, and pack names
/// live in a side table so a large library does not duplicate them per song.
#[derive(Clone, Debug, Default)]
pub struct SongSearchIndex {
    packs: Vec<PackIndexEntry>,
    songs: Vec<SongIndexEntry>,
}

#[derive(Clone, Debug)]
struct PackIndexEntry {
    name: Arc<str>,
    /// Diacritic-folded `name`, for scoring. Shares the same allocation when the
    /// name is already unaccented.
    search_name: Arc<str>,
    song_count: usize,
}

#[derive(Clone, Debug)]
struct SongIndexEntry {
    /// Index into [`SongSearchIndex::packs`], or `usize::MAX` when the song has
    /// no owning pack header yet.
    pack: usize,
    /// Cleaned native title (original case, for word-boundary scoring).
    title: Arc<str>,
    /// Diacritic-folded `title`, and the folded transliterated title when it
    /// differs. Folding per keystroke across the whole catalog would be far too
    /// expensive. Shares the title's allocation when it is already unaccented.
    search_title: Arc<str>,
    search_translit: Option<Arc<str>>,
    song: Arc<SongData>,
}

impl SongSearchIndex {
    #[inline(always)]
    #[must_use]
    pub const fn song_count(&self) -> usize {
        self.songs.len()
    }

    #[inline(always)]
    #[must_use]
    pub const fn pack_count(&self) -> usize {
        self.packs.len()
    }

    fn pack_name(&self, idx: usize) -> Arc<str> {
        self.packs
            .get(idx)
            .map_or_else(|| Arc::from(""), |p| Arc::clone(&p.name))
    }
}

/// Build the search index from the grouped wheel catalog. Runs once at catalog
/// load (and on reload), not per keystroke.
#[must_use]
pub fn build_song_search_index(entries: &[MusicWheelEntry]) -> SongSearchIndex {
    let mut index = SongSearchIndex::default();
    let mut current_pack = usize::MAX;

    for entry in entries {
        match entry {
            MusicWheelEntry::PackHeader {
                name, song_count, ..
            } => {
                if entry.is_series_header() {
                    continue;
                }
                current_pack = index.packs.len();
                index.packs.push(PackIndexEntry {
                    search_name: folded_key(name),
                    name: Arc::clone(name),
                    song_count: *song_count,
                });
            }
            MusicWheelEntry::Song(song) => {
                let native_raw = song.display_title(false);
                let title: Arc<str> = Arc::from(clean_search_title(native_raw));
                let translit_raw = song.display_title(true);
                let search_translit = (translit_raw != native_raw)
                    .then(|| folded_key(&clean_search_title(translit_raw)));
                index.songs.push(SongIndexEntry {
                    pack: current_pack,
                    search_title: folded_key(&title),
                    search_translit,
                    title,
                    song: Arc::clone(song),
                });
            }
        }
    }

    index
}

/// The diacritic-folded form used for scoring, reusing the original allocation
/// when folding changes nothing (the common all-ASCII case).
fn folded_key(text: &str) -> Arc<str> {
    match fuzzy::fold_diacritics(text) {
        std::borrow::Cow::Borrowed(_) => Arc::from(text),
        std::borrow::Cow::Owned(folded) => Arc::from(folded),
    }
}

/// Filter by `[###]`, then fuzzy-rank the cleaned titles.
///
/// Only the rows that can be shown are materialized, so a large catalog does
/// not build candidates it will discard.
#[must_use]
pub fn build_song_matches(
    index: &SongSearchIndex,
    query: &str,
    chart_type: &str,
) -> Vec<SongSearchMatch> {
    let parsed = parse_song_search_live(query);
    let q = fuzzy::prepare_query(&parsed.text);
    let empty_query = q.is_empty();
    // Only nine rows can be shown. Keep those nine ordered on the stack rather
    // than allocating and sorting every fuzzy match in a large library.
    let mut ranked = TopResults::<SONG_SEARCH_MAX_RESULTS>::new();

    for (i, entry) in index.songs.iter().enumerate() {
        if !song_passes_search_filters(&entry.song, chart_type, parsed.difficulty, parsed.bpm_tier)
        {
            continue;
        }

        if empty_query {
            // Nothing typed: only the first window of rows is ever visible.
            ranked.push_back((0, i));
            if ranked.is_full() {
                break;
            }
            continue;
        }

        let mut score = fuzzy::best_match_score(&q, &entry.search_title, &[]);
        if let Some(translit) = &entry.search_translit
            && let Some(t) = fuzzy::best_match_score(&q, translit, &[])
        {
            score = Some(score.map_or(t, |s| s.max(t)));
        }
        let Some(score) = score else {
            continue;
        };
        ranked.insert_by((score, i), |a, b| song_rank_cmp(index, a, b));
    }

    ranked
        .take()
        .map(|(score, i)| {
            let entry = &index.songs[i];
            let song = &entry.song;
            // Only shown rows reach here, so building detail strings is cheap.
            SongSearchMatch::Song {
                candidate: SongSearchCandidate {
                    pack_name: index.pack_name(entry.pack),
                    title: Arc::clone(&entry.title),
                    subtitle: Arc::from(song.display_subtitle(false)),
                    bpm: Arc::from(song.formatted_chart_display_bpm(None)),
                    difficulties: Arc::from(song_search_difficulties_text(song, chart_type)),
                    song: Arc::clone(song),
                },
                score,
            }
        })
        .collect()
}

/// Rank packs for `query` by fuzzy-matching their display names.
#[must_use]
pub fn build_pack_matches(index: &SongSearchIndex, query: &str) -> Vec<SongSearchMatch> {
    let q = fuzzy::prepare_query(query);
    let mut ranked = TopResults::<SONG_SEARCH_MAX_RESULTS>::new();

    for (i, pack) in index.packs.iter().enumerate() {
        let score = if q.is_empty() {
            Some(0)
        } else {
            fuzzy::best_match_score(&q, &pack.search_name, &[])
        };
        let Some(score) = score else {
            continue;
        };
        ranked.insert_by((score, i), |a, b| pack_rank_cmp(index, a, b));
    }

    ranked
        .take()
        .map(|(score, i)| SongSearchMatch::Pack {
            name: Arc::clone(&index.packs[i].name),
            song_count: index.packs[i].song_count,
            score,
        })
        .collect()
}

#[derive(Clone, Copy)]
struct TopResults<const N: usize> {
    entries: [(i32, usize); N],
    len: usize,
}

impl<const N: usize> TopResults<N> {
    const fn new() -> Self {
        Self {
            entries: [(0, 0); N],
            len: 0,
        }
    }

    #[inline]
    fn push_back(&mut self, value: (i32, usize)) {
        debug_assert!(self.len < N);
        self.entries[self.len] = value;
        self.len += 1;
    }

    #[inline]
    const fn is_full(&self) -> bool {
        self.len == N
    }

    #[inline]
    fn insert_by(
        &mut self,
        value: (i32, usize),
        compare: impl Fn(&(i32, usize), &(i32, usize)) -> Ordering,
    ) {
        if self.is_full() && compare(&self.entries[N - 1], &value) != Ordering::Greater {
            return;
        }
        let position = self.entries[..self.len]
            .partition_point(|entry| compare(entry, &value) != Ordering::Greater);

        self.len = (self.len + 1).min(N);
        self.entries
            .copy_within(position..self.len - 1, position + 1);
        self.entries[position] = value;
    }

    fn take(self) -> impl Iterator<Item = (i32, usize)> {
        self.entries.into_iter().take(self.len)
    }
}

#[inline]
fn song_rank_cmp(index: &SongSearchIndex, a: &(i32, usize), b: &(i32, usize)) -> Ordering {
    b.0.cmp(&a.0)
        .then_with(|| cmp_ascii_ci(&index.songs[a.1].title, &index.songs[b.1].title))
}

#[inline]
fn pack_rank_cmp(index: &SongSearchIndex, a: &(i32, usize), b: &(i32, usize)) -> Ordering {
    b.0.cmp(&a.0)
        .then_with(|| cmp_ascii_ci(&index.packs[a.1].name, &index.packs[b.1].name))
}

/// Pre-optimization full-sort path retained only for exact benchmark comparisons.
#[cfg(any(test, feature = "bench-support"))]
#[must_use]
pub fn build_song_matches_reference(
    index: &SongSearchIndex,
    query: &str,
    chart_type: &str,
) -> Vec<SongSearchMatch> {
    let parsed = parse_song_search_live(query);
    let q = fuzzy::prepare_query(&parsed.text);
    let empty_query = q.is_empty();
    let mut ranked: Vec<(i32, usize)> = Vec::new();

    for (i, entry) in index.songs.iter().enumerate() {
        if !song_passes_search_filters(&entry.song, chart_type, parsed.difficulty, parsed.bpm_tier)
        {
            continue;
        }
        if empty_query {
            ranked.push((0, i));
            if ranked.len() >= SONG_SEARCH_MAX_RESULTS {
                break;
            }
            continue;
        }

        let mut score = fuzzy::best_match_score(&q, &entry.search_title, &[]);
        if let Some(translit) = &entry.search_translit
            && let Some(t) = fuzzy::best_match_score(&q, translit, &[])
        {
            score = Some(score.map_or(t, |s| s.max(t)));
        }
        let Some(score) = score else {
            continue;
        };
        ranked.push((score, i));
    }

    if !empty_query {
        ranked.sort_by(|a, b| song_rank_cmp(index, a, b));
        ranked.truncate(SONG_SEARCH_MAX_RESULTS);
    }
    ranked
        .into_iter()
        .map(|(score, i)| {
            let entry = &index.songs[i];
            let song = &entry.song;
            SongSearchMatch::Song {
                candidate: SongSearchCandidate {
                    pack_name: index.pack_name(entry.pack),
                    title: Arc::clone(&entry.title),
                    subtitle: Arc::from(song.display_subtitle(false)),
                    bpm: Arc::from(song.formatted_chart_display_bpm(None)),
                    difficulties: Arc::from(song_search_difficulties_text(song, chart_type)),
                    song: Arc::clone(song),
                },
                score,
            }
        })
        .collect()
}

/// Pre-optimization full-sort path retained only for exact benchmark comparisons.
#[cfg(any(test, feature = "bench-support"))]
#[must_use]
pub fn build_pack_matches_reference(index: &SongSearchIndex, query: &str) -> Vec<SongSearchMatch> {
    let q = fuzzy::prepare_query(query);
    let mut ranked: Vec<(i32, usize)> = Vec::new();
    for (i, pack) in index.packs.iter().enumerate() {
        let score = if q.is_empty() {
            Some(0)
        } else {
            fuzzy::best_match_score(&q, &pack.search_name, &[])
        };
        let Some(score) = score else {
            continue;
        };
        ranked.push((score, i));
    }
    ranked.sort_by(|a, b| pack_rank_cmp(index, a, b));
    ranked.truncate(SONG_SEARCH_MAX_RESULTS);
    ranked
        .into_iter()
        .map(|(score, i)| SongSearchMatch::Pack {
            name: Arc::clone(&index.packs[i].name),
            song_count: index.packs[i].song_count,
            score,
        })
        .collect()
}

/// Allocation-free case-insensitive ordering, for tie-breaks.
fn cmp_ascii_ci(a: &str, b: &str) -> std::cmp::Ordering {
    a.bytes()
        .map(|c| c.to_ascii_lowercase())
        .cmp(b.bytes().map(|c| c.to_ascii_lowercase()))
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
    const PANEL_BG: [f32; 4] = color::rgba_hex("#071016");
    const FOCUS_BG: [f32; 4] = color::rgba_hex("#333333");
    const GRAY: [f32; 4] = color::rgba_hex("#808080");
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
        font("miso"): settext(format!("[ {} ]", open.scope.label())):
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
                let display = song_search_fit(asset_manager, &completion.display, budget, 0.9);
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

/// Stable old/new fixture for the song-search actor batch.
#[cfg(any(test, feature = "bench-support"))]
pub struct SongSearchOverlayAppendBenchmark {
    state: SongSearchState,
    assets: AssetManager,
}

#[cfg(any(test, feature = "bench-support"))]
impl SongSearchOverlayAppendBenchmark {
    #[must_use]
    pub fn new() -> Self {
        let matches = (0..SONG_SEARCH_MAX_RESULTS)
            .map(|index| SongSearchMatch::Pack {
                name: Arc::from(format!("Benchmark Pack {index:02}")),
                song_count: 20 + index,
                score: 1_000 - index as i32,
            })
            .collect();
        Self {
            state: SongSearchState::Open(SongSearchOpen {
                query: String::new(),
                scope: SongSearchScope::Pack,
                matches,
                selected_index: 3,
                blink_t: 0.25,
                chart_type: "dance-single",
                request_generation: 1,
                matches_generation: 1,
                has_result: true,
                presentation: RefCell::new(None),
            }),
            assets: AssetManager::new(),
        }
    }

    #[must_use]
    pub fn actor_count(&self) -> usize {
        let SongSearchState::Open(open) = &self.state else {
            unreachable!("song-search fixture is open");
        };
        let mut actors = Vec::with_capacity(48);
        push_song_search_overlay_unreserved(&mut actors, open, 2, MachineFont::Mega, &self.assets);
        actors.len()
    }

    #[must_use]
    pub fn legacy_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        let SongSearchState::Open(open) = &self.state else {
            unreachable!("song-search fixture is open");
        };
        push_song_search_overlay_unreserved(out, open, 2, MachineFont::Mega, &self.assets);
        std::hint::black_box(&*out);
        super::overlay_actor_checksum(out)
    }

    #[must_use]
    pub fn direct_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        let visible =
            push_song_search_overlay(out, &self.state, 2, MachineFont::Mega, &self.assets);
        debug_assert!(visible);
        std::hint::black_box(&*out);
        super::overlay_actor_checksum(out)
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for SongSearchOverlayAppendBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::{ChartData, SongData};
    use std::path::PathBuf;

    #[test]
    fn retained_search_tree_matches_immediate_reuses_selection_and_caret() {
        crate::assets::i18n::init_for_tests();
        let mut fixture = SongSearchOverlayAppendBenchmark::new();
        let mut immediate = Vec::with_capacity(48);
        let _ = fixture.legacy_frame(&mut immediate);

        let mut retained = Vec::with_capacity(1);
        let _ = fixture.direct_frame(&mut retained);
        let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
            panic!("retained song-search overlay should use one shared frame");
        };
        assert_eq!(
            format!("{immediate:#?}"),
            format!("{:#?}", children.as_ref())
        );
        let first = Arc::clone(children);

        retained.clear();
        let _ = fixture.direct_frame(&mut retained);
        let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
            panic!("stable song-search overlay should remain shared");
        };
        assert!(Arc::ptr_eq(&first, children));

        let SongSearchState::Open(open) = &mut fixture.state else {
            unreachable!("song-search fixture is open");
        };
        open.selected_index += 1;
        retained.clear();
        let _ = fixture.direct_frame(&mut retained);
        let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
            panic!("changed song-search overlay should rebuild a shared frame");
        };
        assert!(!Arc::ptr_eq(&first, children));

        immediate.clear();
        let _ = fixture.legacy_frame(&mut immediate);
        assert_eq!(
            format!("{immediate:#?}"),
            format!("{:#?}", children.as_ref())
        );

        let SongSearchState::Open(open) = &mut fixture.state else {
            unreachable!("song-search fixture is open");
        };
        open.query = "no matching completion".to_owned();
        open.blink_t = 0.0;
        retained.clear();
        let _ = fixture.direct_frame(&mut retained);
        let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
            panic!("visible search caret should remain retained");
        };
        let caret_on = Arc::clone(children);

        assert!(update_song_search(&mut fixture.state, 0.6));
        retained.clear();
        let _ = fixture.direct_frame(&mut retained);
        let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
            panic!("changed search caret should rebuild a shared frame");
        };
        assert!(!Arc::ptr_eq(&caret_on, children));

        immediate.clear();
        let _ = fixture.legacy_frame(&mut immediate);
        assert_eq!(
            format!("{immediate:#?}"),
            format!("{:#?}", children.as_ref())
        );
    }

    fn test_chart(meter: u32) -> ChartData {
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: "hard".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter,
            step_artist: String::new(),
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
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
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

    fn match_signature(matches: &[SongSearchMatch]) -> Vec<(u8, String, usize, i32)> {
        matches
            .iter()
            .map(|item| match item {
                SongSearchMatch::Song { candidate, score } => (
                    0,
                    candidate.title.to_string(),
                    candidate.song.charts.len(),
                    *score,
                ),
                SongSearchMatch::Pack {
                    name,
                    song_count,
                    score,
                } => (1, name.to_string(), *song_count, *score),
            })
            .collect()
    }

    #[test]
    fn fuzzy_ranks_prefix_first() {
        let songs = vec![
            ("Pack A", test_song("Butterfly", 128.0)),
            ("Pack A", test_song("Boaty McBoatface", 140.0)),
        ];
        let index = index_from(&songs);
        let matches = build_song_matches(&index, "butter", "dance-single");
        assert!(!matches.is_empty());
        assert_eq!(matches[0].label(), "Butterfly");
    }

    #[test]
    fn bpm_filter_narrows_songs() {
        let songs = vec![
            ("Pack A", test_song("Slow One", 90.0)),
            ("Pack A", test_song("Fast One", 200.0)),
        ];
        let index = index_from(&songs);
        let matches = build_song_matches(&index, "[200]", "dance-single");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].label(), "Fast One");
    }

    #[test]
    fn pack_matches_rank_by_name() {
        let songs = vec![
            ("Tsunamix", test_song("Song A", 120.0)),
            ("Tsunamix", test_song("Song B", 120.0)),
            ("Otaku's Dream", test_song("Song C", 120.0)),
        ];
        let index = index_from(&songs);
        let matches = build_pack_matches(&index, "tsun");
        assert_eq!(matches.len(), 1);
        match &matches[0] {
            SongSearchMatch::Pack {
                name, song_count, ..
            } => {
                assert_eq!(name.as_ref(), "Tsunamix");
                assert_eq!(*song_count, 2);
            }
            _ => panic!("expected a pack match"),
        }
    }

    #[test]
    fn empty_query_lists_everything() {
        let songs = vec![
            ("Pack A", test_song("Alpha", 120.0)),
            ("Pack A", test_song("Beta", 120.0)),
        ];
        let index = index_from(&songs);
        let matches = build_song_matches(&index, "", "dance-single");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn bounded_song_ranking_matches_full_sort() {
        let songs = (0..96)
            .map(|i| {
                let pack = if i % 2 == 0 { "Pack A" } else { "Pack B" };
                let title = format!("Catalog Song {:02} Remix {}", i % 23, 95 - i);
                (pack, test_song(&title, 120.0 + f64::from(i)))
            })
            .collect::<Vec<_>>();
        let index = index_from(&songs);

        for query in [
            "song",
            "ctlg",
            "remix 4",
            "catalog song 07",
            "zzzz",
            "",
            "[10]",
        ] {
            assert_eq!(
                match_signature(&build_song_matches(&index, query, "dance-single")),
                match_signature(&build_song_matches_reference(&index, query, "dance-single",)),
                "query={query:?}",
            );
        }
    }

    #[test]
    fn bounded_pack_ranking_matches_full_sort() {
        let mut wheel = Vec::new();
        for i in 0..64 {
            let name = format!("Collection Pack {:02}", i % 19);
            wheel.push(MusicWheelEntry::PackHeader {
                name: Arc::from(name),
                original_index: i,
                banner_path: None,
                song_count: i + 1,
                pack_key: None,
                parent_series: None,
            });
            wheel.push(MusicWheelEntry::Song(test_song(
                &format!("Song {i}"),
                120.0,
            )));
        }
        let index = build_song_search_index(&wheel);

        for query in ["pack", "cllctn", "pack 04", "missing", ""] {
            assert_eq!(
                match_signature(&build_pack_matches(&index, query)),
                match_signature(&build_pack_matches_reference(&index, query)),
                "query={query:?}",
            );
        }
    }

    #[test]
    fn clean_title_strips_itl_point_level_and_mix_prefixes() {
        assert_eq!(clean_search_title("[6998] [12] automate"), "automate");
        assert_eq!(
            clean_search_title("[4599] [10] Somebody Set Up Us the Bob-omb"),
            "Somebody Set Up Us the Bob-omb"
        );
        // Leading zero-width space + points/level + [Mix] annotation all stripped.
        assert_eq!(
            clean_search_title("\u{200b}[10000] [15] [Mix] TECHCORE ESSENCE 2685"),
            "TECHCORE ESSENCE 2685"
        );
    }

    #[test]
    fn clean_title_keeps_non_annotation_brackets_and_real_parens() {
        // A non-numeric, non-Mix bracket is part of the real title and kept.
        assert_eq!(
            clean_search_title("[Boss] Final Battle"),
            "[Boss] Final Battle"
        );
        assert_eq!(
            clean_search_title("Slow Down (short cut)"),
            "Slow Down (short cut)"
        );
        assert_eq!(
            clean_search_title("Sippin Yak' (DENNETT Remix)"),
            "Sippin Yak' (DENNETT Remix)"
        );
    }

    #[test]
    fn clean_title_drops_difficulty_parenthetical() {
        assert_eq!(clean_search_title("[12] Karma (Easy)"), "Karma");
        assert_eq!(clean_search_title("Some Song (Challenge)"), "Some Song");
    }

    #[test]
    fn clean_title_falls_back_when_only_annotations() {
        // Nothing but a numeric bracket -> keep the original so the row isn't blank.
        assert_eq!(clean_search_title("[123]"), "[123]");
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
    fn accented_titles_are_reachable_from_an_ascii_query() {
        let songs = vec![
            ("Pack", test_song("Déjà Vu", 175.0)),
            ("Pack", test_song("Señorita", 150.0)),
            ("Pack", test_song("Über Alles", 160.0)),
        ];
        let index = index_from(&songs);

        // Every prefix has to match, or a live typeahead shows an empty list
        // partway through a word the user is spelling correctly.
        for query in ["d", "de", "dej", "deja", "dejav", "deja v", "deja vu"] {
            let matches = build_song_matches(&index, query, "dance-single");
            assert!(
                matches.iter().any(|m| m.label() == "Déjà Vu"),
                "{query:?} did not reach the accented title"
            );
        }
        for (query, want) in [("senor", "Señorita"), ("uber", "Über Alles")] {
            let matches = build_song_matches(&index, query, "dance-single");
            assert!(
                matches.iter().any(|m| m.label() == want),
                "{query:?} did not reach {want:?}"
            );
        }

        // Typing the accent works too, and the shown label keeps it.
        let matches = build_song_matches(&index, "déjà", "dance-single");
        assert_eq!(matches[0].label(), "Déjà Vu");
    }

    #[test]
    fn decomposed_and_precomposed_titles_search_alike() {
        // macOS-authored simfiles commonly carry NFD text; without folding, the
        // two spellings behave as different songs.
        let precomposed = "caf\u{e9}";
        let decomposed = "cafe\u{301}";
        assert_ne!(precomposed, decomposed);

        for title in [precomposed, decomposed] {
            let songs = vec![("Pack", test_song(title, 150.0))];
            let index = index_from(&songs);
            for query in ["cafe", "caf\u{e9}", "cafe\u{301}"] {
                assert_eq!(
                    build_song_matches(&index, query, "dance-single").len(),
                    1,
                    "query {query:?} against title {title:?}"
                );
            }
        }
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
