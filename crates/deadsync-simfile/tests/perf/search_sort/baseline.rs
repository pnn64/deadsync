// Frozen from 8e6b38040 (0.5.1706).
use super::*;

#[inline]
fn lowercase_full_title_bytes(song: &SongData) -> impl Iterator<Item = u8> + '_ {
    let subtitle = song.display_subtitle(false);
    let has_subtitle = !subtitle.trim().is_empty();
    song.display_title(false)
        .bytes()
        .chain(has_subtitle.then_some(b' '))
        .chain(if has_subtitle { subtitle } else { "" }.bytes())
        .map(|byte| byte.to_ascii_lowercase())
}

#[inline]
pub(super) fn display_full_title_cmp(left: &SongData, right: &SongData) -> Ordering {
    lowercase_full_title_bytes(left).cmp(lowercase_full_title_bytes(right))
}

fn sort_song_search_candidates(candidates: &mut [SongSearchCandidate]) {
    candidates.sort_by(|left, right| display_full_title_cmp(&left.song, &right.song));
}

pub(super) fn build_song_search_candidates<'a>(
    entries: impl IntoIterator<Item = SongSearchCatalogEntry<'a>>,
    search_text: &str,
    chart_type: &str,
) -> Vec<SongSearchCandidate> {
    let filter = parse_song_search_filter(search_text);
    let entries = entries.into_iter();
    let (entry_count, upper) = entries.size_hint();
    let entry_count = upper.unwrap_or(entry_count);
    let mut out = Vec::with_capacity(entry_count);
    let mut current_pack_name: Option<&str> = None;
    let mut current_pack_shared: Option<Arc<str>> = None;

    for entry in entries {
        match entry {
            SongSearchCatalogEntry::PackHeader(name) => {
                current_pack_name = Some(name);
                current_pack_shared = None;
            }
            SongSearchCatalogEntry::Song(song) => {
                if !song
                    .charts
                    .iter()
                    .any(|c| c.chart_type.eq_ignore_ascii_case(chart_type))
                {
                    continue;
                }

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
                    && !song.charts.iter().any(|c| {
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
