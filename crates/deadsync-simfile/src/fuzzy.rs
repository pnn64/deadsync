//! Domain-agnostic fuzzy matcher: callers pass a label plus any synonym
//! aliases, so this is shared by the setting search and the song search.
//!
//! Subsequence scoring (fzf-style) first; an edit-distance fallback via
//! bounded dynamic programming runs only when that finds nothing, so typos like `prespective`
//! still resolve without costing anything on normal queries.
//!
//! Everything is sized for a catalog-per-keystroke budget: the query is folded
//! once, subsequence scoring allocates nothing, and the fallback stays inline
//! for ordinary interactive queries and skips words that cannot improve its score.
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
/// Most interactive queries stay inline; longer input spills without changing
/// the public behavior of the matcher.
const QUERY_STACK_CAPACITY: usize = 32;
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
    let mut chars = text.char_indices();
    let Some((offset, replacement)) = chars.find_map(|(offset, ch)| {
        let folded = fold_diacritic(ch);
        (folded != Some(ch)).then_some((offset, folded))
    }) else {
        return Cow::Borrowed(text);
    };
    // Delay ownership until the first changed scalar. The preceding UTF-8
    // bytes can be copied unchanged, and folding never increases byte length.
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..offset]);
    if let Some(ch) = replacement {
        out.push(ch);
    }
    for ch in chars.as_str().chars() {
        if let Some(ch) = fold_diacritic(ch) {
            out.push(ch);
        }
    }
    Cow::Owned(out)
}

/// One scalar of the case-preserving fold, shared by title and query paths.
#[inline]
fn fold_diacritic(ch: char) -> Option<char> {
    if ch.is_ascii() {
        return Some(ch);
    }
    if COMBINING_MARKS.contains(&ch) {
        return None;
    }
    let mut base = None;
    decompose_canonical(ch, |decomposed| {
        if base.is_none() {
            base = Some(decomposed);
        }
    });
    Some(match base {
        Some(base) if base.is_ascii() => base,
        _ => ch,
    })
}

/// A query folded once per keystroke, with short character sequences inline.
#[derive(Clone, Debug)]
pub struct Query {
    chars: SmallVec<[char; QUERY_STACK_CAPACITY]>,
    ascii: bool,
    empty: bool,
}

impl Default for Query {
    fn default() -> Self {
        Self {
            chars: SmallVec::new(),
            ascii: true,
            empty: true,
        }
    }
}

impl Query {
    #[inline(always)]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.empty
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
    let chars: SmallVec<[char; QUERY_STACK_CAPACITY]> = query
        .chars()
        .filter_map(fold_diacritic)
        .filter(|c| !c.is_whitespace())
        .map(fold)
        .collect();
    let ascii = chars.iter().all(char::is_ascii);
    let empty = chars.is_empty();
    Query {
        chars,
        ascii,
        empty,
    }
}

/// Folded query characters, computed once per keystroke and reused per candidate.
///
/// Diacritics are folded here so the query meets candidates on the same footing;
/// candidates are folded once at index build.
pub fn query_chars(query: &str) -> Vec<char> {
    query
        .chars()
        .filter_map(fold_diacritic)
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
    let mut best = subsequence_score_prepared(query, label);

    for alias in aliases {
        if let Some(score) = subsequence_score_prepared(query, alias) {
            let adjusted = score - ALIAS_PENALTY;
            best = Some(best.map_or(adjusted, |b| b.max(adjusted)));
        }
    }

    if best.is_none() {
        best = typo_score(query, label);
    }

    best
}

#[inline]
fn subsequence_score_prepared(query: &Query, candidate: &str) -> Option<i32> {
    if query.ascii && candidate.is_ascii() {
        // Dispatch here so short labels do not pay for an extra scorer call.
        if candidate.len() <= 32 {
            subsequence_score_ascii_scalar(&query.chars, candidate.as_bytes())
        } else {
            subsequence_score_ascii(&query.chars, candidate.as_bytes())
        }
    } else {
        subsequence_score(&query.chars, candidate)
    }
}

/// Byte-specialized scorer for the overwhelmingly common ASCII catalog path.
/// Candidate byte offsets and character offsets are identical here, so it
/// preserves the generic scorer's ordering without UTF-8 decoding or Unicode
/// classification in the inner loop.
fn subsequence_score_ascii(query: &[char], candidate: &[u8]) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    // Prefix matches need only contiguous and boundary bonuses. Keep their
    // common loop separate from the position tracking needed for gaps.
    let mut score = PREFIX_BONUS;
    let mut matched = 0;
    let mut previous = b' ';
    for (&ch, &wanted) in candidate.iter().zip(query) {
        if char::from(ch.to_ascii_lowercase()) != wanted {
            break;
        }
        if matched != 0 {
            score += CONTIGUOUS_BONUS;
        }
        if !previous.is_ascii_alphanumeric()
            || (previous.is_ascii_lowercase() && ch.is_ascii_uppercase())
        {
            score += BOUNDARY_BONUS;
        }
        previous = ch;
        matched += 1;
    }
    if matched == query.len() {
        return Some(score - (candidate.len() as i32) / 8);
    }
    subsequence_score_ascii_tail(query, candidate, matched, score)
}

fn subsequence_score_ascii_tail(
    query: &[char],
    candidate: &[u8],
    matched: usize,
    prefix_score: i32,
) -> Option<i32> {
    let mut score = if matched == 0 { 0 } else { prefix_score };
    let mut next = matched;
    for (index, &wanted) in query.iter().enumerate().skip(matched) {
        // The prepared-query ASCII guard makes this narrowing exact.
        let lower = wanted as u8;
        let pos = if candidate.get(next)?.to_ascii_lowercase() == lower {
            next
        } else {
            let rest = &candidate[next + 1..];
            let offset = if rest.len() < 32 {
                rest.iter()
                    .position(|ch| ch.to_ascii_lowercase() == lower)?
            } else {
                memchr::memchr2(lower, lower.to_ascii_uppercase(), rest)?
            };
            next + 1 + offset
        };
        if index == 0 {
            // A failed prefix cannot start at zero. Its offset still affects
            // the score even when byte search skips the preceding text.
            score -= pos as i32;
        } else if pos == next {
            score += CONTIGUOUS_BONUS;
        } else {
            score -= ((pos - next) as i32).min(GAP_PENALTY_MAX);
        }
        let ch = candidate[pos];
        if pos == 0
            || !candidate[pos - 1].is_ascii_alphanumeric()
            || (candidate[pos - 1].is_ascii_lowercase() && ch.is_ascii_uppercase())
        {
            score += BOUNDARY_BONUS;
        }
        next = pos + 1;
    }
    Some(score - (candidate.len() as i32) / 8)
}

fn subsequence_score_ascii_scalar(query: &[char], candidate: &[u8]) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let mut score = 0i32;
    let mut qi = 0usize;
    let mut prev_match: Option<usize> = None;
    let mut first_match: Option<usize> = None;
    let mut prev_char: Option<u8> = None;

    for (pos, &ch) in candidate.iter().enumerate() {
        if char::from(ch.to_ascii_lowercase()) == query[qi] {
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
                Some(pc) => {
                    !pc.is_ascii_alphanumeric()
                        || (pc.is_ascii_lowercase() && ch.is_ascii_uppercase())
                }
            };
            if boundary {
                score += BOUNDARY_BONUS;
            }
            prev_match = Some(pos);
            qi += 1;
            // The full byte length is already known, so the unmatched suffix
            // cannot affect scoring once the query has been consumed.
            if qi == query.len() {
                break;
            }
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
    score -= first as i32;
    score -= (candidate.len() as i32) / 8;
    Some(score)
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

    let mut chars = candidate.chars();
    while let Some(ch) = chars.next() {
        let pos = cand_len;
        cand_len += 1;
        if fold(ch) == query[qi] {
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
            if qi == query.len() {
                // Only the total scalar count affects the unmatched suffix.
                // Chars::count counts UTF-8 boundaries without decoding each
                // remaining scalar through the scoring loop.
                cand_len += chars.count();
                break;
            }
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
    fold_diacritic(c).map(fold)
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
    typo_score_parts(&query.chars, query.ascii, candidate)
}

fn typo_score_parts(query: &[char], query_ascii: bool, candidate: &str) -> Option<i32> {
    let qlen = query.len();
    if qlen == 0 {
        return None;
    }
    let threshold = (qlen / 3).max(1);

    let mut best_distance: Option<usize> = None;
    // A one-word label is already the whole-candidate comparison. Repeating
    // that exact edit-distance problem cannot improve the best distance.
    let words = candidate
        .split_whitespace()
        .filter(|word| word.len() != candidate.len());
    for word in std::iter::once(candidate).chain(words) {
        // Equal or worse distances cannot change the score. Once a close word
        // exists, spend only the edit budget required to improve it.
        let limit = best_distance.map_or(threshold, |best| best - 1);
        let distance = if query_ascii && word.is_ascii() {
            let bytes = word.as_bytes();
            if bytes.len().abs_diff(qlen) > limit {
                continue;
            }
            bounded_levenshtein_by(qlen, bytes.len(), limit, |qi, wi| {
                query[qi] == char::from(bytes[wi].to_ascii_lowercase())
            })
        } else {
            // Candidate titles are already diacritic-folded. Case-fold into an
            // inline buffer so Unicode typo matching has the same semantics as
            // the old temporary `String` without touching the heap for normal
            // search lengths.
            // Case folding keeps one scalar per input here. Count before
            // materializing: long unrelated Unicode titles should never spill
            // the inline buffer just to fail the length guard.
            if word.chars().count().abs_diff(qlen) > limit {
                continue;
            }
            let word_folded: SmallVec<[char; EDIT_DISTANCE_STACK_CAPACITY]> =
                word.chars().map(fold).collect();
            bounded_levenshtein_by(qlen, word_folded.len(), limit, |qi, wi| {
                query[qi] == word_folded[wi]
            })
        };
        let Some(distance) = distance else {
            continue;
        };
        best_distance = Some(distance);
        // This fallback is called only after the whole-label subsequence
        // failed. An exact word would have been a subsequence too, so one edit
        // is already the best possible fallback result.
        if distance <= 1 {
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
    mut a_len: usize,
    mut b_len: usize,
    limit: usize,
    mut equal: impl FnMut(usize, usize) -> bool,
) -> Option<usize> {
    if a_len.abs_diff(b_len) > limit {
        return None;
    }
    // Equal ends do not contribute to Levenshtein distance. Trimming them
    // also keeps a long, nearly matching title's working row inline.
    let mut offset = 0;
    while offset < a_len.min(b_len) && equal(offset, offset) {
        offset += 1;
    }
    a_len -= offset;
    b_len -= offset;
    while a_len > 0 && b_len > 0 && equal(offset + a_len - 1, offset + b_len - 1) {
        a_len -= 1;
        b_len -= 1;
    }
    if a_len == 0 {
        return (b_len <= limit).then_some(b_len);
    }
    if b_len == 0 {
        return (a_len <= limit).then_some(a_len);
    }

    // Trimming left a mismatch at both ends. With one edit available, equal
    // lengths can only be a single substitution; unequal lengths must match
    // after skipping the first scalar on the longer side. No DP row is needed.
    if limit <= 1 {
        return (limit == 1 && matches_one_edit(a_len, b_len, offset, equal)).then_some(1);
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
        let mut row_min = sentinel;
        for j in start..=end {
            let above = row[j];
            let value = (above + 1)
                .min(row[j - 1] + 1)
                .min(diagonal + usize::from(!equal(offset + i - 1, offset + j - 1)))
                .min(sentinel);
            diagonal = above;
            row[j] = value;
            row_min = row_min.min(value);
        }
        // Every path to the final cell crosses this band. Future edits cannot
        // rescue a row whose cheapest path already exceeds the allowed cost.
        if row_min > limit {
            return None;
        }
        if end < b_len {
            row[end + 1] = sentinel;
        }
    }

    (row[b_len] <= limit).then_some(row[b_len])
}

// Keep these loops out of the general matcher's inline code budget.
#[inline(never)]
fn matches_one_edit(
    a_len: usize,
    b_len: usize,
    offset: usize,
    mut equal: impl FnMut(usize, usize) -> bool,
) -> bool {
    match a_len.cmp(&b_len) {
        std::cmp::Ordering::Equal => a_len == 1,
        std::cmp::Ordering::Greater => (0..b_len).all(|i| equal(offset + i + 1, offset + i)),
        std::cmp::Ordering::Less => (0..a_len).all(|i| equal(offset + i, offset + i + 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEED_ALIASES: &[&str] = &["speed", "cmod", "mmod", "xmod"];
    const NOTESKIN_ALIASES: &[&str] = &["arrows", "skin", "notes"];

    #[test]
    fn bounded_distance_matches_full_distance_at_every_short_word_threshold() {
        let mut words = vec![String::new()];
        for len in 1..=5 {
            for bits in 0..1usize << len {
                words.push(
                    (0..len)
                        .map(|i| if bits & (1 << i) == 0 { 'a' } else { 'b' })
                        .collect(),
                );
            }
        }
        for a in &words {
            for b in &words {
                let distance = strsim::levenshtein(a, b);
                for limit in 0..=6 {
                    assert_eq!(
                        bounded_levenshtein_by(a.len(), b.len(), limit, |i, j| {
                            a.as_bytes()[i] == b.as_bytes()[j]
                        }),
                        (distance <= limit).then_some(distance),
                        "{a:?} / {b:?}, limit {limit}",
                    );
                }
            }
        }
    }

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
    fn ascii_fast_path_matches_unicode_scorer() {
        const QUERIES: [&str; 8] = ["", "song", "csr", "ab", "ABC", "spdm", "zzz", "123"];
        const LABELS: [&str; 10] = [
            "",
            "Catalog Song 0042 Remix 11",
            "CamelCaseRun",
            "a---b trailing suffix",
            "ABC",
            "Speed Mod",
            "under_score_123",
            "no match",
            "song with a deliberately long suffix",
            "123 Start",
        ];

        for query in QUERIES {
            let prepared = prepare_query(query);
            for label in LABELS {
                assert_eq!(
                    subsequence_score_prepared(&prepared, label),
                    subsequence_score(prepared.chars(), label),
                    "query={query:?}, label={label:?}",
                );
            }
        }
    }
}
