//! Fuzzy matcher for the setting search.
//!
//! Subsequence scoring (fzf-style) first; an edit-distance fallback via
//! `strsim` runs only when that finds nothing, so typos like `prespective`
//! still resolve without costing anything on normal queries.
//!
//! Scores are an ordering within a single query, not a stable scale.

use super::row::RowId;

const CONTIGUOUS_BONUS: i32 = 15;
const BOUNDARY_BONUS: i32 = 10;
const PREFIX_BONUS: i32 = 20;
const GAP_PENALTY_MAX: i32 = 10;
/// Alias hits rank below a direct label hit of equal quality.
const ALIAS_PENALTY: i32 = 30;
const TYPO_BASE: i32 = 40;

/// Folded query characters, computed once per keystroke and reused per candidate.
pub(super) fn query_chars(query: &str) -> Vec<char> {
    query
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Best score for a candidate label plus its aliases, or `None` when nothing
/// matches (neither by subsequence nor within the typo threshold).
pub(super) fn best_match_score(query: &[char], label: &str, id: RowId) -> Option<i32> {
    let mut best = subsequence_score(query, label);

    for alias in aliases(id) {
        if let Some(score) = subsequence_score(query, alias) {
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
pub(super) fn subsequence_score(query: &[char], candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let cand: Vec<char> = candidate.chars().collect();
    let mut score = 0i32;
    let mut ci = 0usize;
    let mut prev_match: Option<usize> = None;
    let mut first_match: Option<usize> = None;

    for &qc in query {
        let mut matched = None;
        while ci < cand.len() {
            if cand[ci].to_ascii_lowercase() == qc {
                matched = Some(ci);
                break;
            }
            ci += 1;
        }
        let mi = matched?;

        if first_match.is_none() {
            first_match = Some(mi);
        }
        if let Some(prev) = prev_match {
            if mi == prev + 1 {
                score += CONTIGUOUS_BONUS;
            } else {
                score -= ((mi - prev - 1) as i32).min(GAP_PENALTY_MAX);
            }
        }
        if is_word_boundary(&cand, mi) {
            score += BOUNDARY_BONUS;
        }

        prev_match = Some(mi);
        ci = mi + 1;
    }

    let first = first_match.unwrap_or(0);
    if first == 0 {
        score += PREFIX_BONUS;
    }
    // Earlier and shorter matches win ties.
    score -= first as i32;
    score -= (cand.len() as i32) / 8;

    Some(score)
}

/// Edit-distance fallback, only consulted when subsequence matching fails.
fn typo_score(query: &[char], candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return None;
    }
    let query_str: String = query.iter().collect();
    let threshold = (query.len() / 3).max(1);
    let cand_lower = candidate.to_ascii_lowercase();

    let mut best_distance: Option<usize> = None;
    for word in std::iter::once(cand_lower.as_str()).chain(cand_lower.split_whitespace()) {
        // Only compare against words of comparable length; skip tiny words that
        // would match almost anything.
        if word.len() + threshold < query.len() {
            continue;
        }
        let distance = strsim::levenshtein(&query_str, word);
        best_distance = Some(best_distance.map_or(distance, |b| b.min(distance)));
    }

    match best_distance {
        Some(distance) if distance <= threshold => Some(TYPO_BASE - distance as i32),
        _ => None,
    }
}

#[inline]
fn is_word_boundary(cand: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = cand[i - 1];
    let cur = cand[i];
    !prev.is_alphanumeric() || (prev.is_lowercase() && cur.is_uppercase())
}

/// Synonym keywords per row, so "cmod" or "arrows" resolve to the right setting.
pub(super) fn aliases(id: RowId) -> &'static [&'static str] {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn score(query: &str, label: &str) -> Option<i32> {
        subsequence_score(&query_chars(query), label)
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(score("", "Speed Mod"), Some(0));
    }

    #[test]
    fn subsequence_matches_non_contiguous() {
        assert!(score("spdm", "Speed Mod").is_some());
        assert!(score("errbar", "Error Bar").is_some());
    }

    #[test]
    fn non_subsequence_returns_none() {
        assert!(score("zzz", "Speed Mod").is_none());
    }

    #[test]
    fn prefix_and_contiguous_outrank_scattered() {
        let prefix = score("speed", "Speed Mod").unwrap();
        let scattered = score("sd", "Speed Mod").unwrap();
        assert!(prefix > scattered, "prefix {prefix} vs scattered {scattered}");
    }

    #[test]
    fn better_label_ranks_higher_across_candidates() {
        let q = query_chars("speed");
        let direct = best_match_score(&q, "Speed Mod", RowId::SpeedMod).unwrap();
        let unrelated = best_match_score(&q, "Perspective", RowId::Perspective);
        assert!(unrelated.is_none() || direct > unrelated.unwrap());
    }

    #[test]
    fn alias_matches_when_label_differs() {
        let q = query_chars("arrows");
        assert!(subsequence_score(&q, "NoteSkin").is_none());
        assert!(best_match_score(&q, "NoteSkin", RowId::NoteSkin).is_some());
    }

    #[test]
    fn typo_tolerance_resolves_misspellings() {
        let q = query_chars("prespective");
        assert!(subsequence_score(&q, "Perspective").is_none());
        assert!(best_match_score(&q, "Perspective", RowId::Perspective).is_some());
    }

    #[test]
    fn typo_tolerance_rejects_unrelated() {
        let q = query_chars("xylophone");
        assert!(best_match_score(&q, "Speed Mod", RowId::SpeedMod).is_none());
    }
}
