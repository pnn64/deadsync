//! Ranking and catalog preparation for song and pack search.

use crate::fuzzy;
use crate::song_search::{
    SongSearchCandidate, parse_song_search_live, song_passes_search_filters, song_search_bpm_tiers,
    song_search_difficulties_text_into,
};
use deadsync_chart::SongData;
use deadsync_chart::music_wheel::MusicWheelEntry;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::sync::{Arc, OnceLock};

/// Maximum results returned by a live search.
pub const SONG_SEARCH_MAX_RESULTS: usize = 9;

/// The catalog entries eligible for a search.
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
}

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
    clean_search_title_cow(title).into_owned()
}

fn clean_search_title_cow(title: &str) -> Cow<'_, str> {
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
        Cow::Borrowed(title.trim_matches(is_title_pad))
    } else {
        without_diff
    }
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

// Keep whitespace pending across removed groups so normalization matches
// concatenating the surviving spans and then split_whitespace().join(" ").
fn append_normalized(out: &mut String, span: &str, pending_space: &mut bool) {
    for ch in span.chars() {
        if ch.is_whitespace() {
            *pending_space = true;
        } else {
            if *pending_space && !out.is_empty() {
                out.push(' ');
            }
            out.push(ch);
            *pending_space = false;
        }
    }
}

/// Remove exact difficulty groups and normalize whitespace when parentheses occur.
fn strip_difficulty_parens(input: &str) -> Cow<'_, str> {
    if !input.contains('(') {
        return Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    let mut pending_space = false;
    while let Some(open) = rest.find('(') {
        let Some(close_rel) = rest[open + 1..].find(')') else {
            break;
        };
        let close = open + 1 + close_rel;
        let inner = rest[open + 1..close].trim();
        let end = if DIFFICULTY_WORDS
            .iter()
            .any(|word| inner.eq_ignore_ascii_case(word))
        {
            open
        } else {
            close + 1
        };
        append_normalized(&mut out, &rest[..end], &mut pending_space);
        rest = &rest[close + 1..];
    }
    append_normalized(&mut out, rest, &mut pending_space);
    Cow::Owned(out)
}

/// Titles cleaned once from the wheel catalog, so keystrokes only score.
///
/// Titles keep their original case for word-boundary scoring, and pack names
/// live in a side table so a large library does not duplicate them per song.
#[derive(Clone, Debug, Default)]
pub struct SongSearchIndex {
    packs: Vec<PackIndexEntry>,
    songs: Vec<SongIndexEntry>,
    // The catalog is immutable after construction. Allocate only for BPM
    // searches, initialize only visited songs, and share populated slots on clone.
    bpm_tiers: OnceLock<Arc<[BpmTierSlot]>>,
}

type BpmTierSlot = OnceLock<Option<(i32, i32)>>;

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
    let (pack_count, song_count) = entries.iter().fold((0, 0), |(packs, songs), entry| {
        if matches!(entry, MusicWheelEntry::Song(_)) {
            (packs, songs + 1)
        } else if matches!(entry, MusicWheelEntry::PackHeader { .. }) && !entry.is_series_header() {
            (packs + 1, songs)
        } else {
            (packs, songs)
        }
    });
    let mut index = SongSearchIndex {
        packs: Vec::with_capacity(pack_count),
        songs: Vec::with_capacity(song_count),
        bpm_tiers: OnceLock::new(),
    };
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
                    search_name: folded_shared_key(name),
                    name: Arc::clone(name),
                    song_count: *song_count,
                });
            }
            MusicWheelEntry::Song(song) => {
                let native_raw = song.display_title(false);
                let title: Arc<str> = Arc::from(clean_search_title_cow(native_raw));
                let translit_raw = song.display_title(true);
                let search_translit =
                    (translit_raw != native_raw).then(|| folded_clean_key(translit_raw));
                index.songs.push(SongIndexEntry {
                    pack: current_pack,
                    search_title: folded_shared_key(&title),
                    search_translit,
                    title,
                    song: Arc::clone(song),
                });
            }
            MusicWheelEntry::Random(_) => {}
        }
    }

    index
}

/// The diacritic-folded form used for scoring, sharing the input allocation when
/// folding changes nothing (the common all-ASCII case).
fn folded_shared_key(text: &Arc<str>) -> Arc<str> {
    match fuzzy::fold_diacritics(text) {
        Cow::Borrowed(_) => Arc::clone(text),
        Cow::Owned(folded) => Arc::from(folded),
    }
}

/// Clean and fold a secondary title without first materializing the common
/// unchanged-cleaning case as a temporary `String`.
fn folded_clean_key(text: &str) -> Arc<str> {
    let cleaned = clean_search_title_cow(text);
    match fuzzy::fold_diacritics(&cleaned) {
        Cow::Borrowed(_) => Arc::from(cleaned),
        Cow::Owned(folded) => Arc::from(folded),
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
    // An empty query stops after nine hits. Keep that first short request
    // allocation-free with respect to the cache; reuse slots if already built.
    let mut bpm_tiers = index.bpm_tiers.get();
    let cache_bpm = parsed.bpm_tier.is_some() && !empty_query;

    for (i, entry) in index.songs.iter().enumerate() {
        if !song_passes_search_filters(&entry.song, chart_type, parsed.difficulty, None) {
            continue;
        }
        if let Some(want) = parsed.bpm_tier {
            let tiers = if cache_bpm || bpm_tiers.is_some() {
                let slots = bpm_tiers.get_or_insert_with(|| {
                    index.bpm_tiers.get_or_init(|| {
                        (0..index.songs.len())
                            .map(|_| BpmTierSlot::new())
                            .collect::<Arc<[_]>>()
                    })
                });
                *slots[i].get_or_init(|| song_search_bpm_tiers(&entry.song))
            } else {
                song_search_bpm_tiers(&entry.song)
            };
            if !tiers.is_some_and(|(lo, hi)| (lo..=hi).contains(&want)) {
                continue;
            }
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

    let mut detail_scratch = String::new();
    ranked
        .take()
        .map(|(score, i)| {
            let entry = &index.songs[i];
            let song = &entry.song;
            // Copy each formatted label into its required owning Arc before
            // reusing the buffer for the next label/row. No buffer for misses.
            if detail_scratch.capacity() == 0 {
                detail_scratch.reserve(32);
            }
            deadsync_chart::song::format_display_bpm_range_into(
                song.chart_display_bpm_range(None),
                1.0,
                &mut detail_scratch,
            );
            let bpm = Arc::from(detail_scratch.as_str());
            song_search_difficulties_text_into(song, chart_type, &mut detail_scratch);
            let difficulties = Arc::from(detail_scratch.as_str());
            SongSearchMatch::Song {
                candidate: SongSearchCandidate {
                    pack_name: index.pack_name(entry.pack),
                    title: Arc::clone(&entry.title),
                    subtitle: Arc::from(song.display_subtitle(false)),
                    bpm,
                    difficulties,
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

/// Allocation-free case-insensitive ordering, for tie-breaks.
fn cmp_ascii_ci(a: &str, b: &str) -> std::cmp::Ordering {
    crate::song_sort::cmp_ignore_ascii_case(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::ChartData;
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
    fn index_shares_unchanged_ascii_search_keys() {
        let songs = vec![("Pack ASCII", test_song("Plain ASCII", 120.0))];
        let index = index_from(&songs);

        assert!(Arc::ptr_eq(
            &index.packs[0].name,
            &index.packs[0].search_name
        ));
        assert!(Arc::ptr_eq(
            &index.songs[0].title,
            &index.songs[0].search_title
        ));
    }

    #[test]
    fn title_cleaning_borrows_slices_until_rewriting_is_required() {
        assert!(matches!(
            clean_search_title_cow("Plain ASCII"),
            Cow::Borrowed("Plain ASCII")
        ));
        assert!(matches!(
            clean_search_title_cow("[6998] [12] Anthem"),
            Cow::Borrowed("Anthem")
        ));
        assert!(matches!(
            clean_search_title_cow("Anthem (Challenge)"),
            Cow::Owned(ref value) if value == "Anthem"
        ));
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
}
