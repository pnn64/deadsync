use crate::{CachedScore, Grade};
use deadsync_chart::SongData;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::Arc;

pub const FOLDER_STATS_STAR_BUCKETS: usize = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FolderStatsSummary {
    pub count_charts: u32,
    pub passes: u32,
    pub star_counts: [u32; FOLDER_STATS_STAR_BUCKETS],
    pub best_grade: u8,
}

pub fn folder_stats_summary<'a>(
    songs: impl IntoIterator<Item = &'a SongData>,
    target_chart_type: &str,
    difficulty: &str,
    mut cached_score: impl FnMut(&str) -> Option<CachedScore>,
) -> FolderStatsSummary {
    let mut summary = FolderStatsSummary::default();
    for song in songs {
        for chart in &song.charts {
            if !chart.chart_type.eq_ignore_ascii_case(target_chart_type)
                || !chart.difficulty.eq_ignore_ascii_case(difficulty)
            {
                continue;
            }
            summary.count_charts = summary.count_charts.saturating_add(1);
            let Some(score) = cached_score(&chart.short_hash) else {
                continue;
            };
            if score.grade == Grade::Failed {
                continue;
            }
            summary.passes = summary.passes.saturating_add(1);
            if let Some(bucket) = folder_stats_grade_bucket(score.grade) {
                summary.star_counts[bucket] = summary.star_counts[bucket].saturating_add(1);
            }
        }
    }
    summary.best_grade = folder_stats_best_grade(&summary.star_counts);
    summary
}

pub const fn folder_stats_grade_bucket(grade: Grade) -> Option<usize> {
    match grade {
        Grade::Quint => Some(0),
        Grade::Tier01 => Some(1),
        Grade::Tier02 => Some(2),
        Grade::Tier03 => Some(3),
        Grade::Tier04 => Some(4),
        _ => None,
    }
}

pub fn folder_stats_best_grade(star_counts: &[u32; FOLDER_STATS_STAR_BUCKETS]) -> u8 {
    star_counts
        .iter()
        .position(|count| *count > 0)
        .map_or(0, |idx| (FOLDER_STATS_STAR_BUCKETS - idx) as u8)
}

pub fn folder_stats_difficulty_label(difficulty: &str) -> &str {
    if difficulty.eq_ignore_ascii_case("Challenge") {
        "Expert"
    } else if difficulty.eq_ignore_ascii_case("Beginner") {
        "Beginner"
    } else if difficulty.eq_ignore_ascii_case("Easy") {
        "Easy"
    } else if difficulty.eq_ignore_ascii_case("Medium") {
        "Medium"
    } else if difficulty.eq_ignore_ascii_case("Hard") {
        "Hard"
    } else if difficulty.eq_ignore_ascii_case("Edit") {
        "Edit"
    } else {
        difficulty
    }
}

pub fn grade_sort_order(grade: Grade) -> u8 {
    grade.to_sprite_state() as u8
}

pub fn grade_group_name(grade: Grade) -> &'static str {
    match grade {
        Grade::Quint => "\u{2605}\u{2605}\u{2605}\u{2605}\u{2605}",
        Grade::Tier01 => "\u{2605}\u{2605}\u{2605}\u{2605}",
        Grade::Tier02 => "\u{2605}\u{2605}\u{2605}",
        Grade::Tier03 => "\u{2605}\u{2605}",
        Grade::Tier04 => "\u{2605}",
        Grade::Tier05 => "S+",
        Grade::Tier06 => "S",
        Grade::Tier07 => "S-",
        Grade::Tier08 => "A+",
        Grade::Tier09 => "A",
        Grade::Tier10 => "A-",
        Grade::Tier11 => "B+",
        Grade::Tier12 => "B",
        Grade::Tier13 => "B-",
        Grade::Tier14 => "C+",
        Grade::Tier15 => "C",
        Grade::Tier16 => "C-",
        Grade::Tier17 => "D",
        Grade::Failed => "Failed",
    }
}

pub fn ranked_popular_songs<K: Ord>(
    songs: Vec<Arc<SongData>>,
    chart_play_counts: impl IntoIterator<Item = (String, u32)>,
    limit: usize,
    include_zero_play_songs: bool,
    sort_key: impl Fn(&SongData) -> K,
) -> Vec<(Arc<SongData>, u32)> {
    let hash_to_song_ix = chart_hash_song_indices(&songs);
    let mut song_play_counts = vec![0u32; songs.len()];
    for (chart_hash, chart_plays) in chart_play_counts {
        let Some(&song_ix) = hash_to_song_ix.get(chart_hash.as_str()) else {
            continue;
        };
        song_play_counts[song_ix] = song_play_counts[song_ix].saturating_add(chart_plays);
    }

    let mut ranked: Vec<(Arc<SongData>, u32)> = songs
        .into_iter()
        .enumerate()
        .filter(|(song_ix, _)| include_zero_play_songs || song_play_counts[*song_ix] > 0)
        .map(|(song_ix, song)| (song, song_play_counts[song_ix]))
        .collect();
    ranked.sort_by_cached_key(|(song, play_count)| (Reverse(*play_count), sort_key(song)));
    ranked.truncate(limit.min(ranked.len()));
    ranked
}

pub fn ranked_recent_songs(
    songs: Vec<Arc<SongData>>,
    recent_chart_hashes: impl IntoIterator<Item = String>,
    limit: usize,
) -> Vec<Arc<SongData>> {
    let hash_to_song_ix = chart_hash_song_indices(&songs);
    let mut recent_song_ixs: Vec<usize> = Vec::with_capacity(limit);
    let mut seen_song_ix = vec![false; songs.len()];

    for chart_hash in recent_chart_hashes {
        let Some(&song_ix) = hash_to_song_ix.get(chart_hash.as_str()) else {
            continue;
        };
        if seen_song_ix[song_ix] {
            continue;
        }
        seen_song_ix[song_ix] = true;
        recent_song_ixs.push(song_ix);
        if recent_song_ixs.len() >= limit {
            break;
        }
    }

    recent_song_ixs
        .into_iter()
        .map(|song_ix| songs[song_ix].clone())
        .collect()
}

pub fn ranked_top_grade_songs<K: Ord>(
    songs: Vec<Arc<SongData>>,
    chart_type: &str,
    mut chart_scores: impl FnMut(&str, &mut Vec<CachedScore>),
    sort_key: impl Fn(&SongData) -> K,
) -> Vec<(Arc<SongData>, Option<Grade>)> {
    let mut scores = Vec::with_capacity(2);
    let mut graded_songs: Vec<(Arc<SongData>, Option<Grade>)> = Vec::with_capacity(songs.len());
    for song in songs {
        let mut best_grade = None;
        for chart in &song.charts {
            if !chart.chart_type.eq_ignore_ascii_case(chart_type) || !chart.has_note_data {
                continue;
            }
            scores.clear();
            chart_scores(&chart.short_hash, &mut scores);
            for score in &scores {
                if score.grade == Grade::Failed && score.score_percent <= 0.0 {
                    continue;
                }
                let grade = score.grade;
                if best_grade.is_none()
                    || grade_sort_order(grade) < grade_sort_order(best_grade.unwrap())
                {
                    best_grade = Some(grade);
                }
            }
        }
        graded_songs.push((song, best_grade));
    }

    graded_songs.sort_by_cached_key(|(song, best)| {
        let grade_key = best.map_or(u8::MAX, grade_sort_order);
        (grade_key, sort_key(song))
    });
    graded_songs
}

fn chart_hash_song_indices(songs: &[Arc<SongData>]) -> HashMap<&str, usize> {
    let mut hash_to_song_ix = HashMap::with_capacity(songs.len().saturating_mul(8));
    for (song_ix, song) in songs.iter().enumerate() {
        for chart in &song.charts {
            if chart.has_note_data {
                hash_to_song_ix
                    .entry(chart.short_hash.as_str())
                    .or_insert(song_ix);
            }
        }
    }
    hash_to_song_ix
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cached_score;
    use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, TechCounts};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn chart(difficulty: &str, hash: &str) -> ChartData {
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: difficulty.to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 9,
            step_artist: String::new(),
            music_path: None,
            short_hash: hash.to_string(),
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
            min_bpm: 120.0,
            max_bpm: 120.0,
        }
    }

    fn song(charts: Vec<ChartData>) -> SongData {
        SongData {
            simfile_path: PathBuf::from("song.ssc"),
            title: String::new(),
            subtitle: String::new(),
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
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
            normalized_bpms: String::new(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts,
        }
    }

    #[test]
    fn folder_stats_buckets_match_arrow_cloud_top_grades() {
        assert_eq!(folder_stats_grade_bucket(Grade::Quint), Some(0));
        assert_eq!(folder_stats_grade_bucket(Grade::Tier01), Some(1));
        assert_eq!(folder_stats_grade_bucket(Grade::Tier04), Some(4));
        assert_eq!(folder_stats_grade_bucket(Grade::Tier05), None);
        assert_eq!(folder_stats_grade_bucket(Grade::Failed), None);
    }

    #[test]
    fn folder_stats_best_grade_matches_arrow_cloud_rank() {
        assert_eq!(folder_stats_best_grade(&[0, 0, 0, 0, 0]), 0);
        assert_eq!(folder_stats_best_grade(&[0, 0, 0, 0, 2]), 1);
        assert_eq!(folder_stats_best_grade(&[0, 0, 3, 0, 2]), 3);
        assert_eq!(folder_stats_best_grade(&[1, 0, 3, 0, 2]), 5);
    }

    #[test]
    fn folder_stats_challenge_displays_as_expert() {
        assert_eq!(folder_stats_difficulty_label("Challenge"), "Expert");
        assert_eq!(folder_stats_difficulty_label("Hard"), "Hard");
    }

    #[test]
    fn folder_stats_summary_counts_passes_and_star_buckets() {
        let songs = vec![
            song(vec![chart("Hard", "a"), chart("Hard", "b")]),
            song(vec![chart("Challenge", "c"), chart("Hard", "d")]),
        ];
        let scores = HashMap::from([
            ("a", cached_score(Grade::Quint, 0.99, None, None)),
            ("b", cached_score(Grade::Tier02, 0.95, None, None)),
            ("d", cached_score(Grade::Failed, 0.20, None, None)),
        ]);

        let summary = folder_stats_summary(&songs, "dance-single", "Hard", |hash| {
            scores.get(hash).copied()
        });

        assert_eq!(summary.count_charts, 3);
        assert_eq!(summary.passes, 2);
        assert_eq!(summary.star_counts, [1, 0, 1, 0, 0]);
        assert_eq!(summary.best_grade, 5);
    }

    #[test]
    fn grade_display_policy_matches_evaluation_order() {
        assert_eq!(grade_sort_order(Grade::Quint), 0);
        assert_eq!(grade_sort_order(Grade::Failed), 18);
        assert_eq!(grade_group_name(Grade::Tier05), "S+");
        assert_eq!(grade_group_name(Grade::Tier04), "\u{2605}");
    }

    #[test]
    fn ranked_popular_songs_sums_chart_counts_and_keeps_requested_zeroes() {
        let songs = vec![
            Arc::new(song(vec![chart("Hard", "a"), chart("Challenge", "b")])),
            Arc::new(song(vec![chart("Hard", "c")])),
            Arc::new(song(vec![chart("Hard", "d")])),
        ];

        let ranked = ranked_popular_songs(
            songs.clone(),
            [("a".to_string(), 2), ("b".to_string(), 3)],
            3,
            true,
            |song| song.simfile_path.clone(),
        );

        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].1, 5);
        assert_eq!(ranked[1].1, 0);

        let ranked = ranked_popular_songs(
            songs,
            [("a".to_string(), 2), ("b".to_string(), 3)],
            3,
            false,
            |song| song.simfile_path.clone(),
        );
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].1, 5);
    }

    #[test]
    fn ranked_recent_songs_dedupes_by_song_and_ignores_unknown_hashes() {
        let songs = vec![
            Arc::new(song(vec![chart("Hard", "a"), chart("Challenge", "b")])),
            Arc::new(song(vec![chart("Hard", "c")])),
        ];

        let ranked = ranked_recent_songs(
            songs.clone(),
            [
                "missing".to_string(),
                "b".to_string(),
                "a".to_string(),
                "c".to_string(),
            ],
            2,
        );

        assert_eq!(ranked.len(), 2);
        assert!(Arc::ptr_eq(&ranked[0], &songs[0]));
        assert!(Arc::ptr_eq(&ranked[1], &songs[1]));
    }

    #[test]
    fn ranked_top_grade_songs_sorts_best_grade_then_title_key() {
        let songs = vec![
            Arc::new(song(vec![chart("Hard", "a")])),
            Arc::new(song(vec![chart("Hard", "b")])),
            Arc::new(song(vec![chart("Challenge", "c")])),
        ];

        let ranked = ranked_top_grade_songs(
            songs,
            "dance-single",
            |hash, out| match hash {
                "a" => out.push(cached_score(Grade::Tier03, 0.90, None, None)),
                "b" => out.push(cached_score(Grade::Quint, 0.99, None, None)),
                "c" => out.push(cached_score(Grade::Failed, 0.0, None, None)),
                _ => {}
            },
            |song| song.simfile_path.clone(),
        );

        assert_eq!(ranked[0].1, Some(Grade::Quint));
        assert_eq!(ranked[1].1, Some(Grade::Tier03));
        assert_eq!(ranked[2].1, None);
    }
}
