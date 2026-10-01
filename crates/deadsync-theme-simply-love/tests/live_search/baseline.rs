// Frozen from 4391f6452 (0.5.1650). Only the BPM formatting call in
// build_song_matches is redirected to the frozen formatter; unchanged index,
// top-nine storage, parsing and fuzzy scoring are shared with production.
use super::*;
use std::fmt::Write as _;

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
                    bpm: Arc::from(format_display_bpm_range(
                        song.chart_display_bpm_range(None),
                        1.0,
                    )),
                    difficulties: Arc::from(song_search_difficulties_text(song, chart_type)),
                    song: Arc::clone(song),
                },
                score,
            }
        })
        .collect()
}

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

#[inline]
pub(super) fn song_rank_cmp(
    index: &SongSearchIndex,
    a: &(i32, usize),
    b: &(i32, usize),
) -> Ordering {
    b.0.cmp(&a.0)
        .then_with(|| cmp_ascii_ci(&index.songs[a.1].title, &index.songs[b.1].title))
}

#[inline]
pub(super) fn pack_rank_cmp(
    index: &SongSearchIndex,
    a: &(i32, usize),
    b: &(i32, usize),
) -> Ordering {
    b.0.cmp(&a.0)
        .then_with(|| cmp_ascii_ci(&index.packs[a.1].name, &index.packs[b.1].name))
}

pub(super) fn cmp_ascii_ci(a: &str, b: &str) -> std::cmp::Ordering {
    a.bytes()
        .map(|c| c.to_ascii_lowercase())
        .cmp(b.bytes().map(|c| c.to_ascii_lowercase()))
}

#[must_use]
pub fn song_search_difficulties_text(song: &SongData, chart_type: &str) -> String {
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
        return "-".to_string();
    }

    let mut out = String::with_capacity(32);
    for meter in meters.into_iter().flatten() {
        if !out.is_empty() {
            out.push_str("   ");
        }
        write!(out, "{meter}").expect("writing to a String cannot fail");
    }
    out
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
        let Some((bpm_lo, bpm_hi)) = song.display_bpm_range() else {
            return false;
        };
        let mut lo = song_search_bpm_tier(bpm_lo);
        let mut hi = song_search_bpm_tier(bpm_hi);
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }
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

#[inline(always)]
fn song_search_bpm_tier(bpm: f64) -> i32 {
    (((bpm + 0.5) / 10.0).floor() * 10.0) as i32
}

#[must_use]
pub fn format_display_bpm_range(range: Option<(f64, f64)>, music_rate: f32) -> String {
    let Some((lo, hi)) = range else {
        return String::new();
    };
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    let lo = lo * f64::from(rate);
    let hi = hi * f64::from(rate);
    let use_decimals = (rate - 1.0).abs() > 0.001;
    let equal = (lo - hi).abs() < 1.0e-6;
    let mut out = String::with_capacity(if equal { 8 } else { 19 });
    let write_one = |out: &mut String, value: f64| {
        if use_decimals {
            write!(out, "{value:.1}").expect("writing to a String cannot fail");
            if out.ends_with('0') {
                out.pop();
                if out.ends_with('.') {
                    out.pop();
                }
            }
        } else {
            write!(out, "{value:.0}").expect("writing to a String cannot fail");
        }
    };
    if equal {
        write_one(&mut out, lo);
    } else {
        write_one(&mut out, lo.min(hi));
        out.push_str(" - ");
        write_one(&mut out, lo.max(hi));
    }
    out
}
