use std::cmp::Ordering;
use std::fmt::Write as _;
use std::ops::Range;
use std::sync::Arc;

use deadsync_chart::SongData;

#[derive(Clone, Debug)]
pub struct SongSearchCandidate {
    pub pack_name: Arc<str>,
    pub title: Arc<str>,
    pub subtitle: Arc<str>,
    pub bpm: Arc<str>,
    pub difficulties: Arc<str>,
    pub song: Arc<SongData>,
}

#[derive(Clone)]
pub enum SongSearchCatalogEntry<'a> {
    PackHeader(&'a str),
    Song(&'a Arc<SongData>),
}

#[derive(Default)]
struct SongSearchFilter {
    terms: String,
    pack_term: Option<Range<usize>>,
    song_term: Option<Range<usize>>,
    difficulty: Option<u8>,
    bpm_tier: Option<i32>,
}

impl SongSearchFilter {
    fn pack_term(&self) -> Option<&str> {
        self.terms.get(self.pack_term.as_ref()?.clone())
    }

    fn song_term(&self) -> Option<&str> {
        self.terms.get(self.song_term.as_ref()?.clone())
    }
}

#[inline(always)]
fn song_search_bpm_tier(bpm: f64) -> i32 {
    (((bpm + 0.5) / 10.0).floor() * 10.0) as i32
}

#[inline(always)]
fn search_standard_difficulty_index(name: &str) -> Option<usize> {
    let index = match name.as_bytes().first()?.to_ascii_lowercase() {
        b'b' => 0,
        b'e' => 1,
        b'm' => 2,
        b'h' => 3,
        b'c' => 4,
        _ => return None,
    };
    const NAMES: [&str; 5] = ["Beginner", "Easy", "Medium", "Hard", "Challenge"];
    name.eq_ignore_ascii_case(NAMES[index]).then_some(index)
}

#[must_use]
pub fn song_search_difficulties_text(song: &SongData, chart_type: &str) -> String {
    let mut out = String::new();
    song_search_difficulties_text_into(song, chart_type, &mut out);
    out
}

/// Format the first standard-difficulty meters into reusable storage.
/// Replaces previous contents, including when no standard chart exists.
pub fn song_search_difficulties_text_into(song: &SongData, chart_type: &str, out: &mut String) {
    out.clear();
    let mut meters = [None; 5];
    let mut found = 0;
    for chart in &song.charts {
        if !chart.chart_type.eq_ignore_ascii_case(chart_type) {
            continue;
        }
        let Some(index) = search_standard_difficulty_index(&chart.difficulty) else {
            continue;
        };
        if meters[index].is_none() {
            meters[index] = Some(chart.meter);
            found += 1;
            if found == meters.len() {
                break;
            }
        }
    }
    if found == 0 {
        out.reserve_exact(1);
        out.push('-');
        return;
    }

    out.reserve(32);
    for meter in meters.into_iter().flatten() {
        if !out.is_empty() {
            out.push_str("   ");
        }
        write!(out, "{meter}").expect("writing to a String cannot fail");
    }
}

/// Inclusive display-BPM filter bounds, independent of chart type/difficulty.
#[must_use]
pub fn song_search_bpm_tiers(song: &SongData) -> Option<(i32, i32)> {
    let (bpm_lo, bpm_hi) = song.display_bpm_range()?;
    let mut lo = song_search_bpm_tier(bpm_lo);
    let mut hi = song_search_bpm_tier(bpm_hi);
    if lo > hi {
        std::mem::swap(&mut lo, &mut hi);
    }
    Some((lo, hi))
}

fn parse_song_search_filter(input: &str) -> SongSearchFilter {
    let mut filter = SongSearchFilter::default();
    let mut stripped = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        let ch = ch.to_ascii_lowercase();
        if ch == '[' {
            let mut tail = chars.clone();
            let mut value: u32 = 0;
            let mut has_digit = false;
            while let Some(ch) = tail.peek() {
                let Some(d) = ch.to_digit(10) else {
                    break;
                };
                has_digit = true;
                value = value.saturating_mul(10).saturating_add(d);
                tail.next();
            }
            if has_digit && tail.next_if_eq(&']').is_some() {
                if value <= 35 {
                    filter.difficulty = Some(value as u8);
                } else {
                    filter.bpm_tier = Some(song_search_bpm_tier(f64::from(value)));
                }
                chars = tail;
                continue;
            }
        }
        stripped.push(ch);
    }

    let term_start = stripped.len() - stripped.trim_start().len();
    let term_end = stripped.trim_end().len();
    if term_start < term_end {
        let terms = &stripped[term_start..term_end];
        if let Some(slash) = terms.find('/') {
            if slash > 0 {
                filter.pack_term = Some(term_start..term_start + slash);
            }
            let song_start = term_start + slash + 1;
            if song_start < term_end {
                filter.song_term = Some(song_start..term_end);
            }
        } else {
            filter.song_term = Some(term_start..term_end);
        }
    }
    filter.terms = stripped;
    filter
}

/// Free text with `[###]` tokens stripped, plus the filters they produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SongSearchLiveQuery {
    pub text: String,
    pub difficulty: Option<u8>,
    pub bpm_tier: Option<i32>,
}

/// Parse a typeahead query, splitting `[###]` filters from the free text.
///
/// The `pack/song` split is deliberately not applied: packs have their own scope.
#[must_use]
pub fn parse_song_search_live(input: &str) -> SongSearchLiveQuery {
    let filter = parse_song_search_filter(input);
    let mut text = filter.terms;
    text.truncate(text.trim_end().len());
    let leading_whitespace = text.len() - text.trim_start().len();
    if leading_whitespace != 0 {
        text.drain(..leading_whitespace);
    }
    SongSearchLiveQuery {
        text,
        difficulty: filter.difficulty,
        bpm_tier: filter.bpm_tier,
    }
}

/// Whether `song` has the chart type and passes the `[###]` filters. Title
/// matching is the ranker's job, not this predicate's.
#[must_use]
#[inline]
pub fn song_passes_search_filters(
    song: &SongData,
    chart_type: &str,
    difficulty: Option<u8>,
    bpm_tier: Option<i32>,
) -> bool {
    let mut has_chart_type = false;
    let mut has_difficulty = difficulty.is_none();
    for chart in &song.charts {
        if !chart.chart_type.eq_ignore_ascii_case(chart_type) {
            continue;
        }
        has_chart_type = true;
        if difficulty.is_some_and(|diff| {
            !chart.difficulty.eq_ignore_ascii_case("edit") && chart.meter == u32::from(diff)
        }) {
            has_difficulty = true;
        }
        if has_difficulty {
            break;
        }
    }
    if !has_chart_type || !has_difficulty {
        return false;
    }

    if let Some(want_tier) = bpm_tier {
        let Some((lo, hi)) = song_search_bpm_tiers(song) else {
            return false;
        };
        if lo == hi {
            if want_tier != lo {
                return false;
            }
        } else if want_tier < lo || want_tier > hi {
            return false;
        }
    }

    true
}

#[inline]
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let needle = needle.as_bytes();
    needle.is_empty()
        || haystack
            .as_bytes()
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

#[inline]
fn joined_contains_ignore_ascii_case(left: &str, right: &str, needle: &str) -> bool {
    if contains_ignore_ascii_case(left, needle) {
        return true;
    }
    if right.trim().is_empty() {
        return false;
    }
    if contains_ignore_ascii_case(right, needle) {
        return true;
    }
    let joined_len = left.len().saturating_add(right.len().saturating_add(1));
    let Some(last_start) = joined_len.checked_sub(needle.len()) else {
        return false;
    };

    // Only matches crossing the inserted space remain. Compare the two slices
    // directly instead of selecting a virtual joined byte for every character.
    let left = left.as_bytes();
    let right = right.as_bytes();
    let needle = needle.as_bytes();
    let first_start = left.len().saturating_sub(needle.len() - 1);
    (first_start..=last_start.min(left.len())).any(|start| {
        let prefix_len = left.len() - start;
        needle[prefix_len] == b' '
            && left[start..].eq_ignore_ascii_case(&needle[..prefix_len])
            && right[..needle.len() - prefix_len - 1]
                .eq_ignore_ascii_case(&needle[prefix_len + 1..])
    })
}

#[inline]
fn song_title_contains(song: &SongData, translit: bool, needle: &str) -> bool {
    joined_contains_ignore_ascii_case(
        song.display_title(translit),
        song.display_subtitle(translit),
        needle,
    )
}

// Keep the common title-prefix comparison small; only matching prefixes need
// to compare the remaining title bytes and optional subtitle separator.
#[inline(never)]
fn display_full_title_tail_cmp(left: &SongData, right: &SongData) -> Ordering {
    let left_title = left.display_title(false).as_bytes();
    let right_title = right.display_title(false).as_bytes();
    if left_title.len() == right_title.len() {
        let left_subtitle = left.display_subtitle(false);
        let right_subtitle = right.display_subtitle(false);
        return match (
            left_subtitle.trim().is_empty(),
            right_subtitle.trim().is_empty(),
        ) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => {
                crate::song_sort::cmp_ignore_ascii_case(left_subtitle, right_subtitle)
            }
        };
    }
    let shared = left_title.len().min(right_title.len());
    let (longer, shorter, reversed) = if left_title.len() > right_title.len() {
        (left, right, false)
    } else {
        (right, left, true)
    };
    let subtitle = shorter.display_subtitle(false);
    let order = if subtitle.trim().is_empty() {
        Ordering::Greater
    } else {
        match longer.display_title(false).as_bytes()[shared]
            .to_ascii_lowercase()
            .cmp(&b' ')
        {
            Ordering::Equal => {
                let longer_subtitle = longer.display_subtitle(false);
                let has_subtitle = !longer_subtitle.trim().is_empty();
                longer.display_title(false).as_bytes()[shared + 1..]
                    .iter()
                    .copied()
                    .chain(has_subtitle.then_some(b' '))
                    .chain(if has_subtitle { longer_subtitle } else { "" }.bytes())
                    .map(|byte| byte.to_ascii_lowercase())
                    .cmp(subtitle.bytes().map(|byte| byte.to_ascii_lowercase()))
            }
            order => order,
        }
    };
    if reversed { order.reverse() } else { order }
}

#[inline]
fn display_full_title_cmp(left: &SongData, right: &SongData) -> Ordering {
    let left_title = left.display_title(false).as_bytes();
    let right_title = right.display_title(false).as_bytes();
    for (&a, &b) in left_title.iter().zip(right_title) {
        if a != b {
            let order = a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase());
            if order != Ordering::Equal {
                return order;
            }
        }
    }
    display_full_title_tail_cmp(left, right)
}

fn sort_song_search_candidates(candidates: &mut [SongSearchCandidate]) {
    candidates.sort_by(|left, right| display_full_title_cmp(&left.song, &right.song));
}

pub fn build_song_search_candidates<'a>(
    entries: impl IntoIterator<Item = SongSearchCatalogEntry<'a>>,
    search_text: &str,
    chart_type: &str,
) -> Vec<SongSearchCandidate> {
    let filter = parse_song_search_filter(search_text);
    let entries = entries.into_iter();
    let (entry_count, upper) = entries.size_hint();
    let entry_count = upper.unwrap_or(entry_count);
    let mut out = Vec::new();
    let mut current_pack_name: Option<&str> = None;
    let mut current_pack_shared: Option<Arc<str>> = None;

    for entry in entries {
        match entry {
            SongSearchCatalogEntry::PackHeader(name) => {
                current_pack_name = Some(name);
                current_pack_shared = None;
            }
            SongSearchCatalogEntry::Song(song) => {
                // Retain the cheap presence check for short lists and queries
                // that will never rescan charts for a difficulty match.
                let first_chart = if song.charts.len() > 16 && filter.difficulty.is_some() {
                    song.charts
                        .iter()
                        .position(|c| c.chart_type.eq_ignore_ascii_case(chart_type))
                } else {
                    song.charts
                        .iter()
                        .any(|c| c.chart_type.eq_ignore_ascii_case(chart_type))
                        .then_some(0)
                };
                let Some(first_chart) = first_chart else {
                    continue;
                };

                let pack_name = current_pack_name.unwrap_or_default();
                if let Some(pack_term) = filter.pack_term()
                    && !contains_ignore_ascii_case(pack_name, pack_term)
                {
                    continue;
                }

                if let Some(song_term) = filter.song_term()
                    && !song_title_contains(song, false, song_term)
                    && !song_title_contains(song, true, song_term)
                {
                    continue;
                }

                if let Some(diff) = filter.difficulty
                    && !song.charts[first_chart..].iter().any(|c| {
                        c.chart_type.eq_ignore_ascii_case(chart_type)
                            && !c.difficulty.eq_ignore_ascii_case("edit")
                            && c.meter == u32::from(diff)
                    })
                {
                    continue;
                }

                if let Some(want_tier) = filter.bpm_tier {
                    let Some((bpm_lo, bpm_hi)) = song.display_bpm_range() else {
                        continue;
                    };
                    let mut lo = song_search_bpm_tier(bpm_lo);
                    let mut hi = song_search_bpm_tier(bpm_hi);
                    if lo > hi {
                        std::mem::swap(&mut lo, &mut hi);
                    }
                    if lo == hi {
                        if want_tier != lo {
                            continue;
                        }
                    } else if want_tier < lo || want_tier > hi {
                        continue;
                    }
                }

                if out.is_empty() {
                    out.reserve_exact(entry_count);
                }
                let pack_name =
                    Arc::clone(current_pack_shared.get_or_insert_with(|| Arc::from(pack_name)));
                out.push(SongSearchCandidate {
                    pack_name,
                    title: Arc::from(song.display_title(false)),
                    subtitle: Arc::from(song.display_subtitle(false)),
                    bpm: Arc::from(song.formatted_chart_display_bpm(None)),
                    difficulties: Arc::from(song_search_difficulties_text(song, chart_type)),
                    song: Arc::clone(song),
                });
            }
        }
    }
    sort_song_search_candidates(&mut out);

    out
}

#[cfg(test)]
mod tests {

    #[test]
    fn title_search_preserves_partial_and_whitespace_transliteration_fallbacks() {
        for (left, right, needle, matches) in [
            ("A", "B", "a b", true),
            ("A", "B", "a  b", false),
            ("", "B", " b", true),
            ("A", "", "a ", false),
            ("A", " ", "a ", false),
            ("A", "B", " b", true),
            ("A", "B", "a ", true),
            ("A", "B", "ab", false),
            ("AB", "CD", "b c", true),
            ("A", "B", "a b extra", false),
            ("α", "β", "α β", true),
        ] {
            assert_eq!(
                joined_contains_ignore_ascii_case(left, right, needle),
                matches
            );
        }
        for (title, subtitle, translit_title, translit_subtitle, query, matches) in [
            ("Original", "Mix", "", "", "original mix", true),
            (
                "Original",
                "Mix",
                "\u{2003}",
                "\u{a0}",
                "original mix",
                true,
            ),
            ("Original", "Mix", "\u{2003}", "\u{a0}", "missing", false),
            ("Original", "Mix", "Alternate", "", "alternate mix", true),
            ("Original", "Mix", "", "Edition", "original edition", true),
            ("Original", "Mix", "Original", "Mix", "missing", false),
            (
                "Original",
                "Mix",
                "Alternate",
                "Edition",
                "original mix",
                true,
            ),
            (
                "Original",
                "Mix",
                "Alternate",
                "Edition",
                "alternate edition",
                true,
            ),
            (
                "Original",
                "Mix",
                "Alternate",
                "Edition",
                "original edition",
                false,
            ),
            ("\u{c4}BC", "Mix", "", "", "\u{e4}bc", false),
        ] {
            let mut song = (*test_song_with_bpm(title, "128", 128.0, 128.0)).clone();
            song.subtitle = subtitle.into();
            song.translit_title = translit_title.into();
            song.translit_subtitle = translit_subtitle.into();
            let song = Arc::new(song);
            let candidates = build_song_search_candidates(
                [SongSearchCatalogEntry::Song(&song)],
                query,
                "dance-single",
            );
            assert_eq!(
                candidates.len(),
                usize::from(matches),
                "{query}, {translit_title:?}, {translit_subtitle:?}"
            );
            if matches {
                assert!(Arc::ptr_eq(&candidates[0].song, &song));
            }
        }
        let songs: Vec<_> = [
            (
                "Wide",
                "10000:20000",
                10000.0,
                20000.0,
                u32::MAX,
                "Challenge",
            ),
            ("Hidden", "???", 120.0, 180.0, 1, "Edit"),
            ("Low", "1", 1.0, 1.0, 2, "Beginner"),
        ]
        .into_iter()
        .map(|(title, bpm, lo, hi, meter, difficulty)| {
            let mut song = (*test_song_with_bpm(title, bpm, lo, hi)).clone();
            song.charts[0].meter = meter;
            song.charts[0].difficulty = difficulty.into();
            Arc::new(song)
        })
        .collect();
        let candidates = build_song_search_candidates(
            songs.iter().map(SongSearchCatalogEntry::Song),
            "",
            "dance-single",
        );
        assert_eq!(candidates.len(), songs.len());
        for candidate in candidates {
            assert_eq!(
                candidate.bpm.as_ref(),
                candidate.song.formatted_chart_display_bpm(None)
            );
            assert_eq!(
                candidate.difficulties.as_ref(),
                song_search_difficulties_text(&candidate.song, "dance-single")
            );
        }
    }

    use std::{path::PathBuf, sync::Arc};

    use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, TechCounts};

    use super::*;

    fn test_song(title: &str, subtitle: &str) -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: PathBuf::from("test.sm"),
            title: title.to_string(),
            subtitle: subtitle.to_string(),
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
            display_bpm: "128".to_string(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 128.0,
            max_bpm: 128.0,
            normalized_bpms: "128".to_string(),
            song_timing: None,
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            last_second_hint: 0.0,
            charts: Vec::new(),
        })
    }

    fn test_chart(chart_type: &str) -> ChartData {
        ChartData {
            chart_type: chart_type.to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 12,
            step_artist: String::new(),
            music_path: None,
            short_hash: format!("{chart_type}-hash"),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
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
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 128.0,
            max_bpm: 128.0,
        }
    }

    fn test_song_with_bpm(
        title: &str,
        display_bpm: &str,
        min_bpm: f64,
        max_bpm: f64,
    ) -> Arc<SongData> {
        let mut song = (*test_song(title, "")).clone();
        song.display_bpm = display_bpm.to_string();
        song.min_bpm = min_bpm;
        song.max_bpm = max_bpm;
        song.charts = vec![test_chart("dance-single"), test_chart("dance-double")];
        Arc::new(song)
    }

    #[test]
    fn bpm_filter_uses_display_bpm_range() {
        let slow = test_song_with_bpm("Slow", "128", 128.0, 128.0);
        let range = test_song_with_bpm("Range", "120:180", 120.0, 180.0);
        let entries = [
            SongSearchCatalogEntry::PackHeader("Pack"),
            SongSearchCatalogEntry::Song(&slow),
            SongSearchCatalogEntry::Song(&range),
        ];

        let candidates = build_song_search_candidates(entries, "[180]", "dance-single");

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].song.title, "Range");
    }

    #[test]
    fn search_filter_extracts_tokens_without_changing_term_text() {
        let filter = parse_song_search_filter("  FINALs/[12]SoNG [180] Mix  ");

        assert_eq!(filter.pack_term(), Some("finals"));
        assert_eq!(filter.song_term(), Some("song  mix"));
        assert_eq!(filter.difficulty, Some(12));
        assert_eq!(filter.bpm_tier, Some(180));

        let malformed = parse_song_search_filter("Pack/[x]ÄBC");
        assert_eq!(malformed.pack_term(), Some("pack"));
        assert_eq!(malformed.song_term(), Some("[x]Äbc"));
        assert_eq!(malformed.difficulty, None);
        assert_eq!(malformed.bpm_tier, None);

        let saturated = parse_song_search_filter("[999999999999999999999999]Overflow");
        assert_eq!(saturated.bpm_tier, Some(i32::MAX));
    }

    #[test]
    fn pack_and_song_terms_filter_candidates() {
        let alpha = test_song_with_bpm("Alpha", "128", 128.0, 128.0);
        let beta = test_song_with_bpm("Beta", "128", 128.0, 128.0);
        let entries = [
            SongSearchCatalogEntry::PackHeader("Warmups"),
            SongSearchCatalogEntry::Song(&alpha),
            SongSearchCatalogEntry::PackHeader("Finals"),
            SongSearchCatalogEntry::Song(&beta),
        ];

        let candidates = build_song_search_candidates(entries, "warm/alpha", "dance-single");

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].pack_name.as_ref(), "Warmups");
        assert_eq!(candidates[0].song.title, "Alpha");
    }

    #[test]
    fn difficulty_search_retains_first_match_and_skips_other_types_and_edits() {
        let mut song = (*test_song("Alpha", "Mix")).clone();
        let mut other_type = test_chart("dance-double");
        other_type.meter = 12;
        let mut edit = test_chart("dance-single");
        edit.difficulty = "eDiT".into();
        edit.meter = 12;
        let mut first = test_chart("DANCE-SINGLE");
        first.meter = 11;
        let mut last = test_chart("dance-single");
        last.meter = 13;
        song.charts = vec![other_type, edit, first, last];
        let song = Arc::new(song);
        let entries = [
            SongSearchCatalogEntry::PackHeader("Pack"),
            SongSearchCatalogEntry::Song(&song),
        ];
        for (query, count) in [
            ("[11]", 1),
            ("[12]", 0),
            ("[13]", 1),
            ("pack/alpha [11]", 1),
            ("other/alpha [11]", 0),
            ("pack/missing [13]", 0),
            ("alpha mix [13][128]", 1),
        ] {
            assert_eq!(
                build_song_search_candidates(entries.clone(), query, "dance-single").len(),
                count,
                "{query}"
            );
        }
        let mut first_match = (*song).clone();
        first_match.charts[1].difficulty = "Hard".into();
        let first_match = Arc::new(first_match);
        assert_eq!(
            build_song_search_candidates(
                [SongSearchCatalogEntry::Song(&first_match)],
                "[12]",
                "dance-single"
            )
            .len(),
            1
        );
        let mut large = (*song).clone();
        large
            .charts
            .splice(0..0, std::iter::repeat_n(test_chart("pump-single"), 32));
        let large = Arc::new(large);
        for (query, count) in [("[11]", 1), ("[12]", 0), ("[13]", 1)] {
            assert_eq!(
                build_song_search_candidates(
                    [SongSearchCatalogEntry::Song(&large)],
                    query,
                    "dance-single"
                )
                .len(),
                count
            );
        }
    }

    #[test]
    fn candidates_prepare_display_text_and_share_pack_storage() {
        let alpha = test_song_with_bpm("Alpha", "128", 128.0, 128.0);
        let beta = test_song_with_bpm("Beta", "128", 128.0, 128.0);
        let entries = [
            SongSearchCatalogEntry::PackHeader("Warmups"),
            SongSearchCatalogEntry::Song(&alpha),
            SongSearchCatalogEntry::Song(&beta),
        ];

        let candidates = build_song_search_candidates(entries, "", "dance-single");

        assert_eq!(candidates.len(), 2);
        assert!(Arc::ptr_eq(
            &candidates[0].pack_name,
            &candidates[1].pack_name
        ));
        for candidate in &candidates {
            assert_eq!(
                candidate.title.as_ref(),
                candidate.song.display_title(false)
            );
            assert_eq!(
                candidate.subtitle.as_ref(),
                candidate.song.display_subtitle(false)
            );
            assert_eq!(
                candidate.bpm.as_ref(),
                candidate.song.formatted_chart_display_bpm(None)
            );
            assert_eq!(candidate.difficulties.as_ref(), "12");
        }
    }

    #[test]
    fn difficulty_filter_ignores_edits() {
        let mut chart = test_chart("dance-single");
        chart.difficulty = "Edit".to_string();
        chart.meter = 12;
        let mut song = (*test_song("Edit Only", "")).clone();
        song.charts = vec![chart];
        let song = Arc::new(song);
        let entries = [
            SongSearchCatalogEntry::PackHeader("Pack"),
            SongSearchCatalogEntry::Song(&song),
        ];

        let candidates = build_song_search_candidates(entries, "[12]", "dance-single");

        assert!(candidates.is_empty());
    }

    #[test]
    fn difficulties_text_uses_standard_order() {
        let mut song = (*test_song("Song", "")).clone();
        let mut hard = test_chart("dance-single");
        hard.difficulty = "Hard".to_string();
        hard.meter = 11;
        let mut easy = test_chart("dance-single");
        easy.difficulty = "Easy".to_string();
        easy.meter = 4;
        song.charts = vec![hard, easy];

        assert_eq!(
            song_search_difficulties_text(&song, "dance-single"),
            "4   11"
        );

        song.charts.clear();
        assert_eq!(song_search_difficulties_text(&song, "dance-single"), "-");
    }

    #[test]
    fn difficulties_preserve_first_chart_and_type_filtering() {
        let mut song = (*test_song("Song", "")).clone();
        let mut charts = Vec::new();
        for (chart_type, difficulty, meter) in [
            ("dance-single", "Edit", 20),
            ("pump-single", "Beginner", 99),
            ("dance-single", "Challenge", 14),
            ("dance-single", "Easy", 5),
            ("dance-single", "Easy", 7),
            ("dance-single", "Medium", 9),
            ("dance-single", "Hard", 12),
            ("dance-single", "Beginner", 2),
        ] {
            let mut chart = test_chart(chart_type);
            chart.difficulty = difficulty.to_string();
            chart.meter = meter;
            charts.push(chart);
        }
        song.charts = charts;

        assert_eq!(
            song_search_difficulties_text(&song, "dance-single"),
            "2   5   9   12   14"
        );
        assert_eq!(song_search_difficulties_text(&song, "pump-single"), "99");
    }

    #[test]
    fn song_term_matches_ascii_case_and_title_subtitle_boundary() {
        let song = test_song_with_bpm("Alpha", "128", 128.0, 128.0);
        let mut with_subtitle = (*song).clone();
        with_subtitle.subtitle = "Mix".to_string();
        let with_subtitle = Arc::new(with_subtitle);
        let entries = [
            SongSearchCatalogEntry::PackHeader("WarmUps"),
            SongSearchCatalogEntry::Song(&with_subtitle),
        ];

        let candidates = build_song_search_candidates(entries, "warmups/HA mI", "dance-single");

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].song.title, "Alpha");
    }

    #[test]
    fn full_title_comparison_preserves_subtitles_whitespace_and_utf8() {
        let songs: Vec<_> = [
            ("Alpha", ""),
            ("ALPHA", " \t"),
            ("alpha", "Mix"),
            ("Alpha", "mIX"),
            ("Alpha", " Mix"),
            ("Alpha", "Zoo"),
            ("ALPHA MIX", ""),
            ("Alph", "a Mix"),
            ("", "Mix"),
            ("", ""),
            ("\u{c4}lpha", "Mix"),
            ("\u{e4}lpha", "mix"),
            ("Alpha", "\u{a0}"),
            ("Alpha", "\u{e9}Mix"),
        ]
        .into_iter()
        .map(|(title, subtitle)| test_song(title, subtitle))
        .collect();
        for left in &songs {
            for right in &songs {
                assert_eq!(
                    display_full_title_cmp(left, right),
                    left.display_full_title(false)
                        .to_ascii_lowercase()
                        .cmp(&right.display_full_title(false).to_ascii_lowercase())
                );
            }
        }
    }

    #[test]
    fn candidates_sort_by_lowercase_full_title_and_keep_ties_stable() {
        let beta = test_song_with_bpm("beta", "128", 128.0, 128.0);

        let mut alpha_z = (*test_song_with_bpm("Alpha", "128", 128.0, 128.0)).clone();
        alpha_z.subtitle = "Zoo".to_string();
        let alpha_z = Arc::new(alpha_z);

        let alpha_mix_title = test_song_with_bpm("ALPHA MIX", "128", 128.0, 128.0);
        let mut alpha_mix_parts = (*test_song_with_bpm("Alpha", "128", 128.0, 128.0)).clone();
        alpha_mix_parts.subtitle = "Mix".to_string();
        let alpha_mix_parts = Arc::new(alpha_mix_parts);

        let entries = [
            SongSearchCatalogEntry::PackHeader("Pack"),
            SongSearchCatalogEntry::Song(&beta),
            SongSearchCatalogEntry::Song(&alpha_mix_title),
            SongSearchCatalogEntry::Song(&alpha_z),
            SongSearchCatalogEntry::Song(&alpha_mix_parts),
        ];

        let candidates = build_song_search_candidates(entries, "", "dance-single");
        let titles = candidates
            .iter()
            .map(|candidate| {
                (
                    candidate.song.title.as_str(),
                    candidate.song.subtitle.as_str(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            titles,
            [
                ("ALPHA MIX", ""),
                ("Alpha", "Mix"),
                ("Alpha", "Zoo"),
                ("beta", ""),
            ]
        );
    }
}
