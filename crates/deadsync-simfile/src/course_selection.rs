//! Course selection policy and preparation shared by presentation layers.

use crate::course::{
    self, COURSE_RATING_ORDER, CourseFile, CourseSong, CourseTotals, Difficulty,
    ResolvedCourseStage, SongSort, add_chart_totals, course_difficulty_from_meters, course_meter,
    nearest_filled_slot, push_song_bpm_range, resolve_course_stage, song_unique_key,
};
use deadsync_chart::course::{CourseCyclePlan, CourseSessionPlan, CourseStagePlan};
use deadsync_chart::{ChartData, SongData, SongPack};
use deadsync_rules::course::CourseType;
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use std::hash::Hasher;
use std::path::Path;
use std::sync::Arc;
use twox_hash::XxHash64;

/// Course-wide difficulty and selection policy, independent of screen layout.
pub struct CoursePreparation {
    pub course_type: CourseType,
    pub has_random_entries: bool,
    pub has_most_played_entries: bool,
    preferred_default_idx: usize,
    available_course_diffs: Vec<Difficulty>,
}

impl CoursePreparation {
    pub fn new(course: &CourseFile) -> Self {
        let course_type = course::course_type(course);
        let mut has_random_entries = false;
        let mut has_most_played_entries = false;
        for entry in &course.entries {
            if matches!(
                &entry.song,
                CourseSong::RandomAny
                    | CourseSong::RandomWithinGroup { .. }
                    | CourseSong::Select(_)
            ) {
                has_random_entries = true;
            }
            if matches!(
                &entry.song,
                CourseSong::SortPick {
                    sort: SongSort::MostPlays,
                    ..
                }
            ) || matches!(
                &entry.song,
                CourseSong::Select(select) if select.sort == Some(SongSort::MostPlays)
            ) {
                has_most_played_entries = true;
            }
        }

        let preferred_default_idx = course_difficulty_from_meters(course)
            .and_then(|(difficulty_name, _)| {
                COURSE_RATING_ORDER.iter().position(|diff| {
                    course::difficulty_label(*diff).eq_ignore_ascii_case(difficulty_name)
                })
            })
            .unwrap_or(Difficulty::Medium as usize);
        let preferred_default_diff = COURSE_RATING_ORDER[preferred_default_idx];
        let mut available_course_diffs: Vec<Difficulty> = COURSE_RATING_ORDER
            .iter()
            .copied()
            .filter(|diff| course_meter(course, *diff).is_some_and(|meter| meter >= 0))
            .collect();
        if has_random_entries && available_course_diffs.len() <= 1 {
            available_course_diffs = COURSE_RATING_ORDER.to_vec();
        }
        if available_course_diffs.is_empty() {
            available_course_diffs.push(preferred_default_diff);
        }

        Self {
            course_type,
            has_random_entries,
            has_most_played_entries,
            preferred_default_idx,
            available_course_diffs,
        }
    }

    pub fn difficulties(&self) -> impl Iterator<Item = Difficulty> + '_ {
        self.available_course_diffs.iter().copied()
    }

    pub fn default_rating_index<T>(&self, ratings: &[Option<T>]) -> usize {
        nearest_filled_slot(ratings, self.preferred_default_idx)
            .unwrap_or(self.preferred_default_idx)
    }
}

/// Resolved gameplay stages and numeric facts for one retained course rating.
pub struct PreparedCourseRating {
    pub stages: Vec<CourseStagePlan>,
    pub totals: CourseTotals,
    pub rated_entry_count: usize,
    pub course_meter: Option<u32>,
    pub min_bpm: Option<f64>,
    pub max_bpm: Option<f64>,
    pub total_length_seconds: i32,
}

/// Immutable input for generating cycles after a course has been confirmed.
pub struct EndlessCourseSource {
    resolver: Arc<CourseResolver>,
    course: CourseFile,
    difficulty: Difficulty,
    original: CourseCyclePlan,
}

impl EndlessCourseSource {
    pub fn for_selection(
        resolver: &Arc<CourseResolver>,
        course: &CourseFile,
        difficulty: Difficulty,
        selection: &CourseSessionPlan,
    ) -> Option<Self> {
        if selection.course_type != CourseType::Endless || selection.stages.is_empty() {
            return None;
        }
        Some(Self {
            resolver: Arc::clone(resolver),
            course: course.clone(),
            difficulty,
            original: CourseCyclePlan {
                path: selection.path.clone(),
                stages: selection.stages.clone(),
            },
        })
    }

    /// Reroll against the original catalog, falling back to the admitted order
    /// when the resolver produces no usable stages.
    pub fn next_cycle(&self) -> CourseCyclePlan {
        let Some(stages) = self.resolver.reroll_endless(
            &self.original.path,
            &self.course,
            self.difficulty,
            fresh_course_seed(),
        ) else {
            return self.original.clone();
        };
        CourseCyclePlan {
            path: self.original.path.clone(),
            stages,
        }
    }
}

/// Immutable catalog and score facts retained for selection and Endless cycles.
pub struct CourseResolver {
    by_group_song: HashMap<(String, String), Arc<SongData>>,
    by_song: HashMap<String, Arc<SongData>>,
    songs_by_group: HashMap<String, Vec<Arc<SongData>>>,
    all_songs: Vec<Arc<SongData>>,
    song_play_counts: HashMap<String, u32>,
    song_grade_counts: HashMap<String, course::CourseGradeCounts>,
    target_chart_type: String,
}

impl CourseResolver {
    pub fn new(
        song_packs: &[SongPack],
        played_chart_counts: &[(String, u32)],
        chart_grades: &[(String, u8)],
        target_chart_type: &str,
    ) -> Self {
        let (by_group_song, by_song, songs_by_group, all_songs, song_play_counts) =
            build_song_lookup(song_packs, played_chart_counts);
        let song_grade_counts =
            build_song_grade_counts(song_packs, chart_grades, target_chart_type);
        Self {
            by_group_song,
            by_song,
            songs_by_group,
            all_songs,
            song_play_counts,
            song_grade_counts,
            target_chart_type: target_chart_type.to_string(),
        }
    }

    /// Resolve one rating with the caller's course seed. The callback receives
    /// each exact selected chart inline, allowing presentation to fill its
    /// existing entry buffer without a second retained stage representation.
    pub fn prepare_rating(
        &self,
        path: &Path,
        course: &CourseFile,
        difficulty: Difficulty,
        seed: u64,
        mut on_stage: impl FnMut(&SongData, &ChartData, usize),
    ) -> Option<PreparedCourseRating> {
        let mut stages = Vec::with_capacity(course.entries.len());
        let mut totals = CourseTotals::default();
        let mut rated_entry_count = 0usize;
        let mut meter_sum = 0u32;
        let mut meter_count = 0usize;
        let mut total_length_seconds = 0i32;
        let mut min_bpm = None;
        let mut max_bpm = None;

        for stage in self.resolved_stages(path, course, difficulty, seed) {
            let song_data = &stage.song;
            // The shared resolver validates the chart before admitting a stage.
            let chart = &song_data.charts[stage.chart_index];
            let len = if song_data.music_length_seconds > 0.0 {
                song_data.music_length_seconds.round() as i32
            } else {
                song_data.total_length_seconds.max(0)
            };
            total_length_seconds = total_length_seconds.saturating_add(len.max(0));
            push_song_bpm_range(&mut min_bpm, &mut max_bpm, song_data);
            let entry_index = stages.len();
            stages.push(CourseStagePlan {
                song: song_data.clone(),
                chart_hash: chart.short_hash.clone(),
                modifiers: stage.modifiers,
                gain_seconds: stage.gain_seconds,
                gain_lives: stage.gain_lives,
            });
            add_chart_totals(&mut totals, chart);
            rated_entry_count = rated_entry_count.saturating_add(1);
            meter_sum = meter_sum.saturating_add(chart.meter);
            meter_count = meter_count.saturating_add(1);
            on_stage(song_data, chart, entry_index);
        }

        let explicit_meter = course_meter(course, difficulty)
            .filter(|v| *v >= 0)
            .map(|v| v as u32);
        if rated_entry_count == 0 && explicit_meter.is_none() && difficulty != Difficulty::Medium {
            return None;
        }
        let course_meter = explicit_meter.or_else(|| {
            if meter_count > 0 {
                Some((meter_sum as f32 / meter_count as f32).round() as u32)
            } else {
                None
            }
        });
        Some(PreparedCourseRating {
            stages,
            totals,
            rated_entry_count,
            course_meter,
            min_bpm,
            max_bpm,
            total_length_seconds: total_length_seconds.max(0),
        })
    }

    /// Resolve an Endless cycle using fresh per-cycle repeat history. Existing
    /// rating metadata remains unchanged; this path prepares only stage payloads.
    pub fn reroll_endless(
        &self,
        path: &Path,
        course: &CourseFile,
        difficulty: Difficulty,
        seed: u64,
    ) -> Option<Vec<CourseStagePlan>> {
        if course::course_type(course) != CourseType::Endless {
            return None;
        }
        let resolved = self.resolved_stages(path, course, difficulty, seed);
        let mut stages = Vec::with_capacity(course.entries.len());
        for stage in resolved {
            let chart = &stage.song.charts[stage.chart_index];
            stages.push(CourseStagePlan {
                song: stage.song.clone(),
                chart_hash: chart.short_hash.clone(),
                modifiers: stage.modifiers,
                gain_seconds: stage.gain_seconds,
                gain_lives: stage.gain_lives,
            });
        }
        (!stages.is_empty()).then_some(stages)
    }

    fn resolved_stages<'a>(
        &'a self,
        path: &'a Path,
        course: &'a CourseFile,
        difficulty: Difficulty,
        seed: u64,
    ) -> impl Iterator<Item = ResolvedCourseStage> + 'a {
        let mut selected_song_keys = Vec::with_capacity(course.entries.len());
        let course_type = course::course_type(course);
        course
            .entries
            .iter()
            .enumerate()
            .filter_map(move |(entry_index, entry)| {
                let stage = resolve_course_stage(
                    path,
                    entry_index,
                    seed,
                    entry,
                    &self.by_group_song,
                    &self.by_song,
                    &self.all_songs,
                    &self.songs_by_group,
                    &self.song_play_counts,
                    &self.song_grade_counts,
                    course_type,
                    &selected_song_keys,
                    &self.target_chart_type,
                    difficulty,
                )?;
                stage.song.charts.get(stage.chart_index)?;
                selected_song_keys.push(song_unique_key(&stage.song));
                Some(stage)
            })
    }
}

fn song_dir_key(song: &SongData) -> Option<String> {
    song.simfile_path
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .map(|s| s.trim().to_ascii_lowercase())
}

fn build_song_lookup(
    song_packs: &[SongPack],
    played_chart_counts: &[(String, u32)],
) -> (
    HashMap<(String, String), Arc<SongData>>,
    HashMap<String, Arc<SongData>>,
    HashMap<String, Vec<Arc<SongData>>>,
    Vec<Arc<SongData>>,
    HashMap<String, u32>,
) {
    let total_songs = song_packs.iter().map(|pack| pack.songs.len()).sum();
    let mut by_group_song = HashMap::with_capacity(total_songs);
    let mut by_song = HashMap::with_capacity(total_songs);
    let mut songs_by_group: HashMap<String, Vec<Arc<SongData>>> =
        HashMap::with_capacity(song_packs.len());
    let mut all_songs = Vec::with_capacity(total_songs);

    for pack in song_packs {
        if pack.songs.is_empty() {
            continue;
        }
        let group_name = pack.group_name.trim().to_string();
        let group_key = group_name.to_ascii_lowercase();
        let group_songs = songs_by_group.entry(group_name).or_default();
        group_songs.reserve(pack.songs.len());
        for song in &pack.songs {
            all_songs.push(song.clone());
            group_songs.push(song.clone());
            let Some(song_key) = song_dir_key(song) else {
                continue;
            };
            by_group_song.insert((group_key.clone(), song_key.clone()), song.clone());
            by_song.entry(song_key).or_insert_with(|| song.clone());
        }
    }

    let song_play_counts = build_song_play_counts(song_packs, played_chart_counts, total_songs);

    (
        by_group_song,
        by_song,
        songs_by_group,
        all_songs,
        song_play_counts,
    )
}

fn build_song_play_counts(
    song_packs: &[SongPack],
    played_chart_counts: &[(String, u32)],
    total_songs: usize,
) -> HashMap<String, u32> {
    if played_chart_counts.is_empty() {
        return HashMap::new();
    }

    let mut plays_by_chart: FxHashMap<&str, u32> = FxHashMap::with_capacity_and_hasher(
        played_chart_counts.len(),
        rustc_hash::FxBuildHasher::default(),
    );
    for (chart_hash, plays) in played_chart_counts {
        plays_by_chart
            .entry(chart_hash.as_str())
            .and_modify(|count| *count = count.saturating_add(*plays))
            .or_insert(*plays);
    }

    let mut song_play_counts: HashMap<String, u32> =
        HashMap::with_capacity(plays_by_chart.len().min(total_songs));
    for pack in song_packs {
        for song in &pack.songs {
            let mut matched = false;
            let mut song_plays = 0_u32;
            for chart in &song.charts {
                if let Some(plays) = plays_by_chart.remove(chart.short_hash.as_str()) {
                    matched = true;
                    song_plays = song_plays.saturating_add(plays);
                }
            }
            if matched {
                song_play_counts
                    .entry(song_unique_key(song))
                    .and_modify(|count| *count = count.saturating_add(song_plays))
                    .or_insert(song_plays);
            }
            if plays_by_chart.is_empty() {
                return song_play_counts;
            }
        }
    }

    song_play_counts
}

fn build_song_grade_counts(
    song_packs: &[SongPack],
    chart_grades: &[(String, u8)],
    chart_type: &str,
) -> HashMap<String, course::CourseGradeCounts> {
    if chart_grades.is_empty() {
        return HashMap::new();
    }
    let grades_by_chart: FxHashMap<&str, u8> = chart_grades
        .iter()
        .map(|(chart_hash, grade)| (chart_hash.as_str(), *grade))
        .collect();
    let mut song_grades = HashMap::new();
    for pack in song_packs {
        for song in &pack.songs {
            let mut counts = course::CourseGradeCounts::default();
            let mut has_grade = false;
            for chart in &song.charts {
                if !chart.chart_type.eq_ignore_ascii_case(chart_type) {
                    continue;
                }
                let Some(grade) = grades_by_chart.get(chart.short_hash.as_str()).copied() else {
                    continue;
                };
                let Some(count) = counts.get_mut(grade as usize) else {
                    continue;
                };
                *count = count.saturating_add(1);
                has_grade = true;
            }
            if has_grade {
                song_grades.insert(song_unique_key(song), counts);
            }
        }
    }
    song_grades
}

#[inline(always)]
#[must_use]
pub fn course_score_hash(course_path: &Path) -> String {
    let mut hasher = XxHash64::with_seed(0xC0_01_53_42_0A);
    hasher.write(course_path.to_string_lossy().as_bytes());
    format!("course-{:016x}", hasher.finish())
}

pub fn fresh_course_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0_u64, |duration| duration.as_nanos() as u64)
}

#[cfg(test)]
mod tests;
