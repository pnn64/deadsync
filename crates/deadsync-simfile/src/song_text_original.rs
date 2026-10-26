// Frozen runtime code from b3ad9b1ea, with test-only visibility and an observation wrapper.
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
pub(super) fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
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
fn song_title_contains(song: &SongData, needle: &str) -> bool {
    let title = song.display_title(false);
    let subtitle = song.display_subtitle(false);
    if joined_contains_ignore_ascii_case(title, subtitle, needle) {
        return true;
    }
    let translit_title = song.display_title(true);
    let translit_subtitle = song.display_subtitle(true);
    (title != translit_title || subtitle != translit_subtitle)
        && joined_contains_ignore_ascii_case(translit_title, translit_subtitle, needle)
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
    let mut difficulties = String::new();

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
                    && !song_title_contains(song, song_term)
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
                    difficulties.reserve(32);
                }
                let pack_name =
                    Arc::clone(current_pack_shared.get_or_insert_with(|| Arc::from(pack_name)));
                out.push(SongSearchCandidate {
                    pack_name,
                    title: Arc::from(song.display_title(false)),
                    subtitle: Arc::from(song.display_subtitle(false)),
                    bpm: Arc::from(song.formatted_chart_display_bpm(None)),
                    difficulties: {
                        song_search_difficulties_text_into(song, chart_type, &mut difficulties);
                        Arc::from(difficulties.as_str())
                    },
                    song: Arc::clone(song),
                });
            }
        }
    }
    sort_song_search_candidates(&mut out);

    out
}

pub(super) fn title_subtitle_contains_ignore_ascii_case(
    title: &str,
    subtitle: &str,
    needle: &str,
) -> bool {
    let subtitle = (!subtitle.trim().is_empty()).then_some(subtitle);
    let joined_len = title
        .len()
        .saturating_add(subtitle.map_or(0, |value| value.len().saturating_add(1)));
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return true;
    }
    if needle.len() > joined_len {
        return false;
    }

    let title = title.as_bytes();
    let subtitle = subtitle.map(str::as_bytes).unwrap_or_default();
    (0..=joined_len - needle.len()).any(|start| {
        needle.iter().enumerate().all(|(offset, expected)| {
            let index = start + offset;
            let actual = if index < title.len() {
                title[index]
            } else if index == title.len() {
                b' '
            } else {
                subtitle[index - title.len() - 1]
            };
            actual.eq_ignore_ascii_case(expected)
        })
    })
}

pub(super) fn filter_debug(input: &str) -> String {
    let filter = parse_song_search_filter(input);
    format!(
        "{:?}",
        (
            &filter.terms,
            &filter.pack_term,
            &filter.song_term,
            filter.difficulty,
            filter.bpm_tier
        )
    )
}
