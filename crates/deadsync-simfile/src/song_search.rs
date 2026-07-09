use std::sync::Arc;

use deadsync_chart::SongData;

#[derive(Clone, Debug)]
pub struct SongSearchCandidate {
    pub pack_name: String,
    pub song: Arc<SongData>,
}

#[derive(Clone)]
pub enum SongSearchCatalogEntry<'a> {
    PackHeader(&'a str),
    Song(&'a Arc<SongData>),
}

#[derive(Default)]
struct SongSearchFilter {
    pack_term: Option<String>,
    song_term: Option<String>,
    difficulty: Option<u8>,
    bpm_tier: Option<i32>,
}

#[inline(always)]
fn song_search_bpm_tier(bpm: f64) -> i32 {
    (((bpm + 0.5) / 10.0).floor() as i32) * 10
}

pub fn song_search_difficulties_text(song: &SongData, chart_type: &str) -> String {
    const ORDER: [&str; 5] = ["beginner", "easy", "medium", "hard", "challenge"];
    let mut out = String::new();
    for diff in ORDER {
        if let Some(chart) = song.charts.iter().find(|c| {
            c.chart_type.eq_ignore_ascii_case(chart_type) && c.difficulty.eq_ignore_ascii_case(diff)
        }) {
            if !out.is_empty() {
                out.push_str("   ");
            }
            out.push_str(&chart.meter.to_string());
        }
    }
    if out.is_empty() { "-".to_string() } else { out }
}

fn parse_song_search_filter(input: &str) -> SongSearchFilter {
    let lower = input.to_ascii_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut filter = SongSearchFilter::default();
    let mut stripped = String::with_capacity(lower.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '[' {
            let mut j = i + 1;
            let mut value: u32 = 0;
            let mut has_digit = false;
            while j < chars.len() {
                let Some(d) = chars[j].to_digit(10) else {
                    break;
                };
                has_digit = true;
                value = value.saturating_mul(10).saturating_add(d);
                j += 1;
            }
            if has_digit && j < chars.len() && chars[j] == ']' {
                if value <= 35 {
                    filter.difficulty = Some(value as u8);
                } else {
                    filter.bpm_tier = Some(song_search_bpm_tier(value as f64));
                }
                i = j + 1;
                continue;
            }
        }
        stripped.push(chars[i]);
        i += 1;
    }

    let stripped = stripped.trim();
    if let Some((left, right)) = stripped.split_once('/') {
        if !left.is_empty() {
            filter.pack_term = Some(left.to_string());
        }
        if !right.is_empty() {
            filter.song_term = Some(right.to_string());
        }
    } else if !stripped.is_empty() {
        filter.song_term = Some(stripped.to_string());
    }
    filter
}

pub fn build_song_search_candidates<'a>(
    entries: impl IntoIterator<Item = SongSearchCatalogEntry<'a>>,
    search_text: &str,
    chart_type: &str,
) -> Vec<SongSearchCandidate> {
    let filter = parse_song_search_filter(search_text);
    let mut out = Vec::new();
    let mut current_pack_name: Option<&str> = None;

    for entry in entries {
        match entry {
            SongSearchCatalogEntry::PackHeader(name) => {
                current_pack_name = Some(name);
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
                if let Some(pack_term) = &filter.pack_term
                    && !pack_name.to_ascii_lowercase().contains(pack_term)
                {
                    continue;
                }

                if let Some(song_term) = &filter.song_term {
                    let display = song.display_full_title(false).to_ascii_lowercase();
                    let translit = song.display_full_title(true).to_ascii_lowercase();
                    if !display.contains(song_term) && !translit.contains(song_term) {
                        continue;
                    }
                }

                if let Some(diff) = filter.difficulty
                    && !song.charts.iter().any(|c| {
                        c.chart_type.eq_ignore_ascii_case(chart_type)
                            && !c.difficulty.eq_ignore_ascii_case("edit")
                            && c.meter == diff as u32
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

                out.push(SongSearchCandidate {
                    pack_name: pack_name.to_string(),
                    song: Arc::clone(song),
                });
            }
        }
    }
    out.sort_by_cached_key(|c| c.song.display_full_title(false).to_ascii_lowercase());

    out
}

#[cfg(test)]
mod tests {
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
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
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
        assert_eq!(candidates[0].pack_name, "Warmups");
        assert_eq!(candidates[0].song.title, "Alpha");
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
    }
}
