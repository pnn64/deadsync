//! Domain-agnostic fuzzy matcher: callers pass a label plus any synonym
//! aliases, so this is shared by the setting search and the song search.
//!
//! Subsequence scoring (fzf-style) first; an edit-distance fallback via
//! `strsim` runs only when that finds nothing, so typos like `prespective`
//! still resolve without costing anything on normal queries.
//!
//! Everything is sized for a catalog-per-keystroke budget: the query is folded
//! once, scoring allocates nothing, and the fallback skips candidates whose
//! length already puts them out of range.
//!
//! Scores are an ordering within a single query, not a stable scale.

use smallvec::{SmallVec, smallvec};
use std::borrow::Cow;
use unicode_normalization::char::decompose_canonical;

const CONTIGUOUS_BONUS: i32 = 15;
const BOUNDARY_BONUS: i32 = 10;
const PREFIX_BONUS: i32 = 20;
const GAP_PENALTY_MAX: i32 = 10;
/// Alias hits rank below a direct label hit of equal quality.
const ALIAS_PENALTY: i32 = 30;
const TYPO_BASE: i32 = 40;
/// Keeps the typo fallback on the stack for every query accepted by song
/// search, including the trailing DP cell.
const EDIT_DISTANCE_STACK_CAPACITY: usize = 96;

/// Full Unicode fold, not `to_ascii_lowercase`: labels are often non-Latin.
#[inline]
fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Combining diacritical marks, the block that decorates Latin base letters.
///
/// Deliberately narrow. Japanese dakuten (U+3099/U+309A) are combining marks
/// too, but they change the sound rather than decorate it, so folding them
/// would merge distinct titles.
const COMBINING_MARKS: std::ops::RangeInclusive<char> = '\u{300}'..='\u{36f}';

/// Reduce Latin letters to their unaccented base so an ASCII query can reach an
/// accented title: `Déjà Vu` searches as `Deja Vu`.
///
/// Also reconciles NFC and NFD, which otherwise behave as different titles —
/// macOS-authored simfiles commonly carry decomposed text. Case is preserved
/// because [`subsequence_score`] reads it for word-boundary bonuses.
///
/// Non-Latin scripts pass through untouched: a decomposition is only taken when
/// it yields an ASCII base, so Hangul syllables are not reduced to leading jamo
/// and voiced kana keep their mark.
///
/// Borrows for the common all-ASCII case, so callers can precompute this once at
/// index build without paying an allocation per title.
#[must_use]
pub fn fold_diacritics(text: &str) -> Cow<'_, str> {
    if text.is_ascii() {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    for ch in text.chars() {
        if COMBINING_MARKS.contains(&ch) {
            changed = true;
            continue;
        }
        let mut base = None;
        decompose_canonical(ch, |d| {
            if base.is_none() {
                base = Some(d);
            }
        });
        match base {
            Some(b) if b.is_ascii() && b != ch => {
                changed = true;
                out.push(b);
            }
            _ => out.push(ch),
        }
    }
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(text)
    }
}

/// A query folded once per keystroke, kept in both forms the two passes need.
#[derive(Clone, Debug, Default)]
pub struct Query {
    chars: Vec<char>,
    ascii: bool,
}

impl Query {
    #[inline(always)]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    /// Folded characters, for direct [`subsequence_score`] calls.
    #[inline(always)]
    #[must_use]
    pub fn chars(&self) -> &[char] {
        &self.chars
    }
}

/// Prepare a query once per keystroke for reuse across the whole catalog.
#[must_use]
pub fn prepare_query(query: &str) -> Query {
    let chars = query_chars(query);
    let ascii = chars.iter().all(char::is_ascii);
    Query { chars, ascii }
}

/// Folded query characters, computed once per keystroke and reused per candidate.
///
/// Diacritics are folded here so the query meets candidates on the same footing;
/// candidates are folded once at index build.
pub fn query_chars(query: &str) -> Vec<char> {
    fold_diacritics(query)
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(fold)
        .collect()
}

/// Best score across `label` and `aliases`, or `None` if nothing matches.
/// Pass `&[]` when a domain has no synonyms.
///
/// Candidates are expected to be pre-folded with [`fold_diacritics`] so the hot
/// loop pays only the per-char case fold; queries are folded by
/// [`prepare_query`].
#[must_use]
pub fn best_match_score(query: &Query, label: &str, aliases: &[&str]) -> Option<i32> {
    let mut best = subsequence_score(&query.chars, label);

    for alias in aliases {
        if let Some(score) = subsequence_score(&query.chars, alias) {
            let adjusted = score - ALIAS_PENALTY;
            best = Some(best.map_or(adjusted, |b| b.max(adjusted)));
        }
    }

    if best.is_none() {
        best = typo_score(query, label);
    }

    best
}

/// Subsequence match + score; `None` unless every query char appears in order.
/// Single-pass and allocation-free.
#[must_use]
pub fn subsequence_score(query: &[char], candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let mut score = 0i32;
    let mut qi = 0usize;
    let mut prev_match: Option<usize> = None;
    let mut first_match: Option<usize> = None;
    let mut prev_char: Option<char> = None;
    let mut cand_len = 0usize;

    for (pos, ch) in candidate.chars().enumerate() {
        cand_len = pos + 1;
        if qi < query.len() && fold(ch) == query[qi] {
            if first_match.is_none() {
                first_match = Some(pos);
            }
            if let Some(prev) = prev_match {
                if pos == prev + 1 {
                    score += CONTIGUOUS_BONUS;
                } else {
                    score -= ((pos - prev - 1) as i32).min(GAP_PENALTY_MAX);
                }
            }
            let boundary = match prev_char {
                None => true,
                Some(pc) => !pc.is_alphanumeric() || (pc.is_lowercase() && ch.is_uppercase()),
            };
            if boundary {
                score += BOUNDARY_BONUS;
            }
            prev_match = Some(pos);
            qi += 1;
        }
        prev_char = Some(ch);
    }

    if qi != query.len() {
        return None;
    }

    let first = first_match.unwrap_or(0);
    if first == 0 {
        score += PREFIX_BONUS;
    }
    // Earlier and shorter matches win ties.
    score -= first as i32;
    score -= (cand_len as i32) / 8;

    Some(score)
}

/// Per-char form of [`fold_diacritics`] composed with the case fold.
///
/// `None` for a combining mark, which folds away entirely.
#[inline]
fn fold_search(c: char) -> Option<char> {
    if COMBINING_MARKS.contains(&c) {
        return None;
    }
    let mut base = None;
    decompose_canonical(c, |d| {
        if base.is_none() {
            base = Some(d);
        }
    });
    Some(fold(match base {
        Some(b) if b.is_ascii() => b,
        _ => c,
    }))
}

/// Case- and diacritic-insensitive prefix test returning candidate chars
/// consumed, so callers can split the original candidate. Compares char-by-char
/// because folding can change byte *and* char length, so a folded byte-prefix
/// isn't a char prefix. The count is over *original* candidate chars, including
/// combining marks that fold away, so it stays a valid split point.
pub fn folded_prefix_len(query: &str, candidate: &str) -> Option<usize> {
    let mut q = query.chars().filter_map(fold_search);
    let mut consumed = 0usize;
    for cc in candidate.chars() {
        let Some(cf) = fold_search(cc) else {
            // A combining mark decorates the char before it, so it belongs on
            // the consumed side of the split.
            consumed += 1;
            continue;
        };
        match q.next() {
            None => return Some(consumed),
            Some(qc) => {
                if cf != qc {
                    return None;
                }
                consumed += 1;
            }
        }
    }
    q.next().is_none().then_some(consumed)
}

/// Edit-distance fallback, only consulted when subsequence matching fails.
///
/// Levenshtein distance is at least the length difference, so words too far off
/// in length are skipped before paying for the comparison. Lengths count chars,
/// not bytes, or multi-byte labels look several times longer than they are.
fn typo_score(query: &Query, candidate: &str) -> Option<i32> {
    let qlen = query.chars.len();
    if qlen == 0 {
        return None;
    }
    let threshold = (qlen / 3).max(1);

    let mut best_distance: Option<usize> = None;
    for word in std::iter::once(candidate).chain(candidate.split_whitespace()) {
        let distance = if query.ascii && word.is_ascii() {
            let bytes = word.as_bytes();
            if bytes.len().abs_diff(qlen) > threshold {
                continue;
            }
            bounded_levenshtein_by(qlen, bytes.len(), threshold, |qi, wi| {
                query.chars[qi] == char::from(bytes[wi].to_ascii_lowercase())
            })
        } else {
            // Candidate titles are already diacritic-folded. Case-fold into an
            // inline buffer so Unicode typo matching has the same semantics as
            // the old temporary `String` without touching the heap for normal
            // search lengths.
            let word_folded: SmallVec<[char; EDIT_DISTANCE_STACK_CAPACITY]> =
                word.chars().map(fold).collect();
            if word_folded.len().abs_diff(qlen) > threshold {
                continue;
            }
            bounded_levenshtein_by(qlen, word_folded.len(), threshold, |qi, wi| {
                query.chars[qi] == word_folded[wi]
            })
        };
        let Some(distance) = distance else {
            continue;
        };
        best_distance = Some(best_distance.map_or(distance, |b| b.min(distance)));
        if best_distance == Some(0) {
            break;
        }
    }
    match best_distance {
        Some(distance) if distance <= threshold => Some(TYPO_BASE - distance as i32),
        _ => None,
    }
}

/// Levenshtein distance limited to the only values typo scoring can accept.
///
/// The diagonal band avoids work that cannot lead to a distance at or below
/// `limit`, while the inline row removes the allocation performed by
/// `strsim::levenshtein` for each candidate word.
fn bounded_levenshtein_by(
    a_len: usize,
    b_len: usize,
    limit: usize,
    mut equal: impl FnMut(usize, usize) -> bool,
) -> Option<usize> {
    if a_len.abs_diff(b_len) > limit {
        return None;
    }
    if a_len == 0 {
        return (b_len <= limit).then_some(b_len);
    }
    if b_len == 0 {
        return (a_len <= limit).then_some(a_len);
    }

    let sentinel = limit + 1;
    let mut row: SmallVec<[usize; EDIT_DISTANCE_STACK_CAPACITY]> = smallvec![sentinel; b_len + 1];
    for (j, cell) in row.iter_mut().enumerate().take(b_len.min(limit) + 1) {
        *cell = j;
    }

    for i in 1..=a_len {
        let start = i.saturating_sub(limit).max(1);
        let end = (i + limit).min(b_len);
        if start > end {
            return None;
        }

        let mut diagonal = row[start - 1];
        if start == 1 {
            row[0] = i.min(sentinel);
        } else {
            row[start - 1] = sentinel;
        }
        for j in start..=end {
            let above = row[j];
            let value = (above + 1)
                .min(row[j - 1] + 1)
                .min(diagonal + usize::from(!equal(i - 1, j - 1)))
                .min(sentinel);
            diagonal = above;
            row[j] = value;
        }
        if end < b_len {
            row[end + 1] = sentinel;
        }
    }

    (row[b_len] <= limit).then_some(row[b_len])
}

/// The committed pre-optimization query representation, retained so the
/// guarded benchmark can compare the new implementation with its exact old
/// allocation and CPU behavior.
#[cfg(any(test, feature = "bench-support"))]
pub struct ReferenceQuery {
    chars: Vec<char>,
    text: String,
}

#[cfg(any(test, feature = "bench-support"))]
impl ReferenceQuery {
    #[inline(always)]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }
}

#[cfg(any(test, feature = "bench-support"))]
#[must_use]
pub fn prepare_query_reference(query: &str) -> ReferenceQuery {
    let chars = query_chars(query);
    let text = chars.iter().collect();
    ReferenceQuery { chars, text }
}

#[cfg(any(test, feature = "bench-support"))]
#[must_use]
pub fn best_match_score_reference(
    query: &ReferenceQuery,
    label: &str,
    aliases: &[&str],
) -> Option<i32> {
    let mut best = subsequence_score(&query.chars, label);

    for alias in aliases {
        if let Some(score) = subsequence_score(&query.chars, alias) {
            let adjusted = score - ALIAS_PENALTY;
            best = Some(best.map_or(adjusted, |b| b.max(adjusted)));
        }
    }

    if best.is_none() {
        best = typo_score_reference(query, label);
    }

    best
}

#[cfg(any(test, feature = "bench-support"))]
fn typo_score_reference(query: &ReferenceQuery, candidate: &str) -> Option<i32> {
    let qlen = query.chars.len();
    if qlen == 0 {
        return None;
    }
    let threshold = (qlen / 3).max(1);

    let mut best_distance: Option<usize> = None;
    for word in std::iter::once(candidate).chain(candidate.split_whitespace()) {
        let wlen = word.chars().count();
        if wlen + threshold < qlen || wlen > qlen + threshold {
            continue;
        }
        let word_folded: String = word.chars().map(fold).collect();
        let distance = strsim::levenshtein(&query.text, &word_folded);
        best_distance = Some(best_distance.map_or(distance, |b| b.min(distance)));
        if best_distance == Some(0) {
            break;
        }
    }
    match best_distance {
        Some(distance) if distance <= threshold => Some(TYPO_BASE - distance as i32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEED_ALIASES: &[&str] = &["speed", "cmod", "mmod", "xmod"];
    const NOTESKIN_ALIASES: &[&str] = &["arrows", "skin", "notes"];

    fn score(query: &str, label: &str) -> Option<i32> {
        subsequence_score(&query_chars(query), label)
    }

    #[test]
    fn folds_non_ascii_case_for_localized_labels() {
        // Only matches with full Unicode folding; to_ascii_lowercase is a no-op here.
        assert!(score("скор", "Скорость").is_some());
        assert!(score("ταχ", "Ταχύτητα").is_some());
    }

    #[test]
    fn fold_diacritics_reduces_latin_accents_only() {
        assert_eq!(fold_diacritics("Déjà Vu"), "Deja Vu");
        assert_eq!(fold_diacritics("Señorita"), "Senorita");
        assert_eq!(fold_diacritics("Über Ålesund"), "Uber Alesund");
        // NFC and NFD converge, so the two spellings search alike.
        assert_eq!(fold_diacritics("caf\u{e9}"), "cafe");
        assert_eq!(fold_diacritics("cafe\u{301}"), "cafe");
        // Case survives: subsequence_score reads it for word-boundary bonuses.
        assert_eq!(fold_diacritics("ÉCLAT"), "ECLAT");
        // All-ASCII borrows rather than allocating.
        assert!(matches!(fold_diacritics("Speed Mod"), Cow::Borrowed(_)));
    }

    #[test]
    fn fold_diacritics_leaves_non_latin_scripts_alone() {
        // Hangul syllables decompose into jamo; taking a base would shred them.
        assert_eq!(fold_diacritics("한국어"), "한국어");
        // Dakuten changes the sound rather than decorating a Latin letter, in
        // both precomposed and decomposed spellings.
        assert_eq!(fold_diacritics("ガガ"), "ガガ");
        assert_eq!(fold_diacritics("\u{30ab}\u{3099}"), "\u{30ab}\u{3099}");
        assert_eq!(fold_diacritics("Скорость"), "Скорость");
        assert_eq!(fold_diacritics("Ταχύτητα"), "Ταχύτητα");
    }

    #[test]
    fn folded_prefix_len_counts_original_chars_across_folding() {
        // The count splits the *original* candidate, so a precomposed accent is
        // one char and a decomposed one is two.
        assert_eq!(folded_prefix_len("deja", "D\u{e9}j\u{e0} Vu"), Some(4));
        assert_eq!(folded_prefix_len("deja", "De\u{301}ja\u{300} Vu"), Some(6));
        assert_eq!(folded_prefix_len("déjà", "Deja Vu"), Some(4));
    }

    #[test]
    fn folded_prefix_len_handles_non_ascii_and_counts_chars() {
        assert_eq!(folded_prefix_len("spe", "Speed Mod"), Some(3));
        assert_eq!(folded_prefix_len("скор", "Скорость"), Some(4));
        assert_eq!(folded_prefix_len("Speed Mod", "Speed Mod"), Some(9));
        assert_eq!(folded_prefix_len("Speed Mod Extra", "Speed Mod"), None);
        assert_eq!(folded_prefix_len("xyz", "Speed Mod"), None);
        assert_eq!(folded_prefix_len("", "Speed Mod"), Some(0));
    }

    #[test]
    fn typo_threshold_uses_char_counts_not_bytes() {
        // "Скор" is 4 chars but 8 bytes; a byte guard would skip the comparison.
        let q = prepare_query("скол");
        assert!(best_match_score(&q, "Скор", &[]).is_some());
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(score("", "Speed Mod"), Some(0));
    }

    #[test]
    fn subsequence_matches_non_contiguous() {
        assert!(score("spdm", "Speed Mod").is_some());
        assert!(score("errbar", "Error Bar").is_some());
        assert!(score("bfly", "Butterfly").is_some());
    }

    #[test]
    fn non_subsequence_returns_none() {
        assert!(score("zzz", "Speed Mod").is_none());
    }

    #[test]
    fn prefix_and_contiguous_outrank_scattered() {
        let prefix = score("speed", "Speed Mod").unwrap();
        let scattered = score("sd", "Speed Mod").unwrap();
        assert!(
            prefix > scattered,
            "prefix {prefix} vs scattered {scattered}"
        );
    }

    #[test]
    fn better_label_ranks_higher_across_candidates() {
        let q = prepare_query("speed");
        let direct = best_match_score(&q, "Speed Mod", SPEED_ALIASES).unwrap();
        let unrelated = best_match_score(&q, "Perspective", &[]);
        assert!(unrelated.is_none() || direct > unrelated.unwrap());
    }

    #[test]
    fn alias_matches_when_label_differs() {
        let q = prepare_query("arrows");
        assert!(subsequence_score(q.chars(), "NoteSkin").is_none());
        assert!(best_match_score(&q, "NoteSkin", NOTESKIN_ALIASES).is_some());
    }

    #[test]
    fn typo_tolerance_resolves_misspellings() {
        let q = prepare_query("prespective");
        assert!(subsequence_score(q.chars(), "Perspective").is_none());
        assert!(best_match_score(&q, "Perspective", &[]).is_some());

        // Transposition in a song-style title.
        let q = prepare_query("atcion");
        assert!(subsequence_score(q.chars(), "Action").is_none());
        assert!(best_match_score(&q, "Action", &[]).is_some());
    }

    #[test]
    fn typo_tolerance_rejects_unrelated() {
        let q = prepare_query("xylophone");
        assert!(best_match_score(&q, "Speed Mod", SPEED_ALIASES).is_none());
        assert!(best_match_score(&q, "Butterfly", &[]).is_none());
    }

    #[test]
    fn bounded_distance_matches_full_levenshtein_within_threshold() {
        let words = ["", "a", "b", "ab", "ba", "abc", "cab", "скор"];
        for a in words {
            for b in words {
                let a_chars: Vec<char> = a.chars().collect();
                let b_chars: Vec<char> = b.chars().collect();
                let full = strsim::levenshtein(a, b);
                for limit in 0..=4 {
                    let bounded =
                        bounded_levenshtein_by(a_chars.len(), b_chars.len(), limit, |ai, bi| {
                            a_chars[ai] == b_chars[bi]
                        });
                    assert_eq!(
                        bounded,
                        (full <= limit).then_some(full),
                        "a={a:?}, b={b:?}, limit={limit}",
                    );
                }
            }
        }
    }

    #[test]
    fn optimized_matcher_matches_committed_reference() {
        const QUERIES: [&str; 9] = [
            "",
            "sonf",
            "prespective",
            "atcion",
            "deja",
            "déjà",
            "скол",
            "速度",
            "xylophone",
        ];
        const LABELS: [&str; 9] = [
            "Catalog Song 0042 Remix 11",
            "Perspective",
            "Action",
            "Déjà Vu",
            "Скорость",
            "速度",
            "Butterfly",
            "Speed Mod",
            "A Very Long Unrelated Candidate",
        ];

        for query in QUERIES {
            let optimized = prepare_query(query);
            let reference = prepare_query_reference(query);
            assert_eq!(optimized.is_empty(), reference.is_empty());
            for label in LABELS {
                let folded = fold_diacritics(label);
                assert_eq!(
                    best_match_score(&optimized, &folded, SPEED_ALIASES),
                    best_match_score_reference(&reference, &folded, SPEED_ALIASES),
                    "query={query:?}, label={label:?}",
                );
            }
        }
    }
}
