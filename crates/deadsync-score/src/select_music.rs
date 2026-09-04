use crate::{CachedScore, Grade};
use deadsync_chart::SongData;
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::cmp::Ordering;
use std::sync::Arc;
use std::vec::Drain;

pub const FOLDER_STATS_STAR_BUCKETS: usize = 5;

/// Immutable chart-to-song lookup shared by a batch of Select Music rankings.
///
/// The index borrows the screen's song list and is intentionally short-lived:
/// callers build it once before producing the machine and per-profile views,
/// then discard it after those immutable views have been assembled.
pub struct SongRankingIndex<'a> {
    songs: &'a [Arc<SongData>],
    chart_hash_to_song: FxHashMap<&'a str, usize>,
    song_order_rank: Vec<usize>,
}

/// Reusable main-thread storage for a batch of Select Music rankings.
///
/// All vectors retain their capacity between rankings. Recent-song membership
/// uses generation stamps so clearing it is O(1); a full clear occurs only
/// after the `u32` generation counter wraps.
#[derive(Default)]
pub struct SongRankingWorkspace {
    song_play_counts: Vec<u32>,
    popular: Vec<(usize, u32)>,
    recent_song_indices: Vec<usize>,
    seen_generation: Vec<u32>,
    generation: u32,
    scores: Vec<CachedScore>,
    top_grades: Vec<(usize, Option<Grade>)>,
}

impl<'a> SongRankingIndex<'a> {
    #[must_use]
    pub fn new(songs: &'a [Arc<SongData>]) -> Self {
        Self {
            songs,
            chart_hash_to_song: chart_hash_song_indices(songs),
            song_order_rank: Vec::new(),
        }
    }

    /// Precomputes a total song-order rank once for all subsequent ranking
    /// calls. Those calls must use the same comparator.
    pub fn prepare_song_order(&mut self, song_cmp: impl Fn(&SongData, &SongData) -> Ordering) {
        let mut song_order: Vec<usize> = (0..self.songs.len()).collect();
        song_order.sort_unstable_by(|left, right| {
            song_cmp(&self.songs[*left], &self.songs[*right]).then_with(|| left.cmp(right))
        });
        self.song_order_rank.resize(self.songs.len(), 0);
        for (rank, song_ix) in song_order.into_iter().enumerate() {
            self.song_order_rank[song_ix] = rank;
        }
    }

    #[must_use]
    pub const fn songs(&self) -> &'a [Arc<SongData>] {
        self.songs
    }

    pub fn rank_popular<H: AsRef<str>>(
        &self,
        chart_play_counts: impl IntoIterator<Item = (H, u32)>,
        limit: usize,
        include_zero_play_songs: bool,
        workspace: &mut SongRankingWorkspace,
        song_cmp: impl Fn(&SongData, &SongData) -> Ordering,
    ) {
        workspace.song_play_counts.resize(self.songs.len(), 0);
        workspace.song_play_counts.fill(0);
        for (chart_hash, chart_plays) in chart_play_counts {
            let Some(&song_ix) = self.chart_hash_to_song.get(chart_hash.as_ref()) else {
                continue;
            };
            workspace.song_play_counts[song_ix] =
                workspace.song_play_counts[song_ix].saturating_add(chart_plays);
        }

        workspace.popular.clear();
        workspace.popular.reserve(self.songs.len());
        workspace.popular.extend(
            self.songs
                .iter()
                .enumerate()
                .filter(|(song_ix, _)| {
                    include_zero_play_songs || workspace.song_play_counts[*song_ix] > 0
                })
                .map(|(song_ix, _)| (song_ix, workspace.song_play_counts[song_ix])),
        );
        workspace
            .popular
            .sort_unstable_by(|(left_ix, left_count), (right_ix, right_count)| {
                right_count.cmp(left_count).then_with(|| {
                    if self.song_order_rank.len() == self.songs.len() {
                        self.song_order_rank[*left_ix].cmp(&self.song_order_rank[*right_ix])
                    } else {
                        song_cmp(&self.songs[*left_ix], &self.songs[*right_ix])
                            .then_with(|| left_ix.cmp(right_ix))
                    }
                })
            });
        workspace
            .popular
            .truncate(limit.min(workspace.popular.len()));
    }

    pub fn rank_recent<H: AsRef<str>>(
        &self,
        recent_chart_hashes: impl IntoIterator<Item = H>,
        limit: usize,
        workspace: &mut SongRankingWorkspace,
    ) {
        workspace.begin_recent_pass(self.songs.len(), limit);
        let generation = workspace.generation;
        for chart_hash in recent_chart_hashes {
            let Some(&song_ix) = self.chart_hash_to_song.get(chart_hash.as_ref()) else {
                continue;
            };
            if workspace.seen_generation[song_ix] == generation {
                continue;
            }
            workspace.seen_generation[song_ix] = generation;
            workspace.recent_song_indices.push(song_ix);
            if workspace.recent_song_indices.len() >= limit {
                break;
            }
        }
    }

    pub fn rank_top_grades(
        &self,
        chart_type: &str,
        mut chart_scores: impl FnMut(&str, &mut Vec<CachedScore>),
        workspace: &mut SongRankingWorkspace,
        song_cmp: impl Fn(&SongData, &SongData) -> Ordering,
    ) {
        workspace.scores.clear();
        if workspace.scores.capacity() < 2 {
            workspace.scores.reserve(2);
        }
        workspace.top_grades.clear();
        workspace.top_grades.reserve(self.songs.len());

        for (song_ix, song) in self.songs.iter().enumerate() {
            let mut best_grade = None;
            for chart in &song.charts {
                if !chart.chart_type.eq_ignore_ascii_case(chart_type) || !chart.has_note_data {
                    continue;
                }
                workspace.scores.clear();
                chart_scores(&chart.short_hash, &mut workspace.scores);
                for score in &workspace.scores {
                    if score.grade == Grade::Failed && score.score_percent <= 0.0 {
                        continue;
                    }
                    let grade = score.grade;
                    if best_grade
                        .is_none_or(|best| grade_sort_order(grade) < grade_sort_order(best))
                    {
                        best_grade = Some(grade);
                    }
                }
            }
            workspace.top_grades.push((song_ix, best_grade));
        }

        workspace
            .top_grades
            .sort_unstable_by(|(left_ix, left_grade), (right_ix, right_grade)| {
                left_grade
                    .map_or(u8::MAX, grade_sort_order)
                    .cmp(&right_grade.map_or(u8::MAX, grade_sort_order))
                    .then_with(|| {
                        if self.song_order_rank.len() == self.songs.len() {
                            self.song_order_rank[*left_ix].cmp(&self.song_order_rank[*right_ix])
                        } else {
                            song_cmp(&self.songs[*left_ix], &self.songs[*right_ix])
                                .then_with(|| left_ix.cmp(right_ix))
                        }
                    })
            });
    }
}

impl SongRankingWorkspace {
    fn begin_recent_pass(&mut self, song_count: usize, limit: usize) {
        self.seen_generation.resize(song_count, 0);
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen_generation.fill(0);
            self.generation = 1;
        }
        self.recent_song_indices.clear();
        self.recent_song_indices.reserve(limit.min(song_count));
    }

    #[must_use]
    pub fn popular(&self) -> &[(usize, u32)] {
        &self.popular
    }

    pub fn drain_popular(&mut self) -> Drain<'_, (usize, u32)> {
        self.popular.drain(..)
    }

    #[must_use]
    pub fn recent_song_indices(&self) -> &[usize] {
        &self.recent_song_indices
    }

    pub fn drain_recent_song_indices(&mut self) -> Drain<'_, usize> {
        self.recent_song_indices.drain(..)
    }

    #[must_use]
    pub fn top_grades(&self) -> &[(usize, Option<Grade>)] {
        &self.top_grades
    }

    pub fn drain_top_grades(&mut self) -> Drain<'_, (usize, Option<Grade>)> {
        self.top_grades.drain(..)
    }
}

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

#[must_use]
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

#[must_use]
pub fn folder_stats_best_grade(star_counts: &[u32; FOLDER_STATS_STAR_BUCKETS]) -> u8 {
    star_counts
        .iter()
        .position(|count| *count > 0)
        .map_or(0, |idx| (FOLDER_STATS_STAR_BUCKETS - idx) as u8)
}

#[must_use]
pub const fn folder_stats_difficulty_label(difficulty: &str) -> &str {
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

#[must_use]
pub const fn grade_sort_order(grade: Grade) -> u8 {
    grade.to_sprite_state() as u8
}

#[must_use]
pub const fn grade_group_name(grade: Grade) -> &'static str {
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

pub fn ranked_popular_songs<H: AsRef<str>>(
    songs: Vec<Arc<SongData>>,
    chart_play_counts: impl IntoIterator<Item = (H, u32)>,
    limit: usize,
    include_zero_play_songs: bool,
    song_cmp: impl Fn(&SongData, &SongData) -> Ordering,
) -> Vec<(Arc<SongData>, u32)> {
    let index = SongRankingIndex::new(&songs);
    let mut workspace = SongRankingWorkspace::default();
    index.rank_popular(
        chart_play_counts,
        limit,
        include_zero_play_songs,
        &mut workspace,
        song_cmp,
    );
    workspace
        .drain_popular()
        .map(|(song_ix, count)| (Arc::clone(&songs[song_ix]), count))
        .collect()
}

pub fn ranked_recent_songs<H: AsRef<str>>(
    songs: Vec<Arc<SongData>>,
    recent_chart_hashes: impl IntoIterator<Item = H>,
    limit: usize,
) -> Vec<Arc<SongData>> {
    let index = SongRankingIndex::new(&songs);
    let mut workspace = SongRankingWorkspace::default();
    index.rank_recent(recent_chart_hashes, limit, &mut workspace);
    workspace
        .drain_recent_song_indices()
        .into_iter()
        .map(|song_ix| songs[song_ix].clone())
        .collect()
}

/// # Panics
///
/// Panics if an internal state invariant is violated.
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

fn chart_hash_song_indices(songs: &[Arc<SongData>]) -> FxHashMap<&str, usize> {
    let chart_count = songs
        .iter()
        .map(|song| {
            song.charts
                .iter()
                .filter(|chart| chart.has_note_data)
                .count()
        })
        .sum();
    let mut hash_to_song_ix =
        FxHashMap::with_capacity_and_hasher(chart_count, FxBuildHasher::default());
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
        let mut played = song(vec![chart("Hard", "a"), chart("Challenge", "b")]);
        played.simfile_path = PathBuf::from("c.ssc");
        let mut zero_b = song(vec![chart("Hard", "c")]);
        zero_b.simfile_path = PathBuf::from("b.ssc");
        let mut zero_a = song(vec![chart("Hard", "d")]);
        zero_a.simfile_path = PathBuf::from("a.ssc");
        let songs = vec![Arc::new(played), Arc::new(zero_b), Arc::new(zero_a)];
        let counts = [("a".to_string(), 2), ("b".to_string(), 3)];

        let ranked = ranked_popular_songs(
            songs.clone(),
            counts.iter().map(|(hash, count)| (hash.as_str(), *count)),
            3,
            true,
            |left, right| left.simfile_path.cmp(&right.simfile_path),
        );

        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].1, 5);
        assert_eq!(ranked[1].1, 0);
        assert_eq!(ranked[1].0.simfile_path, PathBuf::from("a.ssc"));
        assert_eq!(ranked[2].0.simfile_path, PathBuf::from("b.ssc"));

        let ranked = ranked_popular_songs(
            songs,
            [("a".to_string(), 2), ("b".to_string(), 3)],
            3,
            false,
            |left, right| left.simfile_path.cmp(&right.simfile_path),
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

        let hashes = [
            "missing".to_string(),
            "b".to_string(),
            "a".to_string(),
            "c".to_string(),
        ];
        let ranked = ranked_recent_songs(songs.clone(), hashes.iter().map(String::as_str), 2);

        assert_eq!(ranked.len(), 2);
        assert!(Arc::ptr_eq(&ranked[0], &songs[0]));
        assert!(Arc::ptr_eq(&ranked[1], &songs[1]));
    }

    #[test]
    fn ranking_workspace_reuses_index_without_leaking_previous_results() {
        let mut first = song(vec![chart("Hard", "a"), chart("Challenge", "b")]);
        first.simfile_path = PathBuf::from("b.ssc");
        let mut second = song(vec![chart("Hard", "c")]);
        second.simfile_path = PathBuf::from("a.ssc");
        let songs = vec![Arc::new(first), Arc::new(second)];
        let mut index = SongRankingIndex::new(&songs);
        index.prepare_song_order(|left, right| left.simfile_path.cmp(&right.simfile_path));
        let mut workspace = SongRankingWorkspace::default();

        index.rank_popular(
            [("a", 2), ("b", 3), ("c", 1)],
            2,
            true,
            &mut workspace,
            |left, right| left.simfile_path.cmp(&right.simfile_path),
        );
        assert_eq!(workspace.popular()[0].1, 5);
        assert_eq!(workspace.popular()[0].0, 0);

        index.rank_popular([("c", 7)], 2, false, &mut workspace, |left, right| {
            left.simfile_path.cmp(&right.simfile_path)
        });
        assert_eq!(workspace.popular().len(), 1);
        assert_eq!(workspace.popular()[0].1, 7);
        assert_eq!(workspace.popular()[0].0, 1);

        index.rank_recent(["b", "a", "c"], 2, &mut workspace);
        assert_eq!(workspace.recent_song_indices(), &[0, 1]);
        index.rank_recent(["c", "a"], 2, &mut workspace);
        assert_eq!(workspace.recent_song_indices(), &[1, 0]);
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

    #[test]
    fn indexed_top_grades_match_cached_key_ranking() {
        let mut songs = vec![
            Arc::new(song(vec![chart("Hard", "a")])),
            Arc::new(song(vec![chart("Hard", "b")])),
            Arc::new(song(vec![chart("Challenge", "c")])),
        ];
        Arc::get_mut(&mut songs[0]).unwrap().simfile_path = PathBuf::from("c.ssc");
        Arc::get_mut(&mut songs[1]).unwrap().simfile_path = PathBuf::from("a.ssc");
        Arc::get_mut(&mut songs[2]).unwrap().simfile_path = PathBuf::from("b.ssc");
        let fill_scores = |hash: &str, out: &mut Vec<CachedScore>| match hash {
            "a" => out.push(cached_score(Grade::Tier03, 0.90, None, None)),
            "b" => out.push(cached_score(Grade::Tier03, 0.91, None, None)),
            "c" => out.push(cached_score(Grade::Failed, 0.0, None, None)),
            _ => {}
        };
        let expected = ranked_top_grade_songs(songs.clone(), "dance-single", fill_scores, |song| {
            song.simfile_path.clone()
        });

        let mut index = SongRankingIndex::new(&songs);
        index.prepare_song_order(|left, right| left.simfile_path.cmp(&right.simfile_path));
        let mut workspace = SongRankingWorkspace::default();
        index.rank_top_grades(
            "dance-single",
            fill_scores,
            &mut workspace,
            |left, right| left.simfile_path.cmp(&right.simfile_path),
        );

        assert_eq!(workspace.top_grades().len(), expected.len());
        for ((actual_ix, actual_grade), (expected_song, expected_grade)) in
            workspace.top_grades().iter().zip(&expected)
        {
            assert!(Arc::ptr_eq(&songs[*actual_ix], expected_song));
            assert_eq!(actual_grade, expected_grade);
        }
    }
}
