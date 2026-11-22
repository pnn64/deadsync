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
use std::path::{Path, PathBuf};
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
    chart_indices: Vec<usize>,
    pub totals: CourseTotals,
    pub rated_entry_count: usize,
    pub course_meter: Option<u32>,
    pub min_bpm: Option<f64>,
    pub max_bpm: Option<f64>,
    pub total_length_seconds: i32,
}

impl PreparedCourseRating {
    /// The exact charts admitted by the resolver, in stage order.
    pub fn charts(&self) -> impl Iterator<Item = (&SongData, &ChartData)> {
        self.stages
            .iter()
            .zip(&self.chart_indices)
            .map(|(stage, &index)| (stage.song.as_ref(), &stage.song.charts[index]))
    }
}

#[derive(Clone, Copy)]
pub struct CourseChoice<'a> {
    pub path: &'a Path,
    pub difficulty: Difficulty,
}

/// A course rating retained from the same catalog that supplied the wheel.
#[derive(Clone)]
pub struct ConfirmedCourse {
    catalog: Arc<PreparedCourseCatalog>,
    course_index: usize,
    difficulty: Difficulty,
}

impl ConfirmedCourse {
    fn choice(&self) -> CourseChoice<'_> {
        CourseChoice {
            path: self.catalog.courses[self.course_index].path(),
            difficulty: self.difficulty,
        }
    }

    pub fn can_start(&self) -> bool {
        self.catalog.can_start(self.choice())
    }

    pub fn start(&self) -> Option<PreparedCourseStart> {
        self.catalog.start(self.choice())
    }
}

impl std::fmt::Debug for ConfirmedCourse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let choice = self.choice();
        f.debug_struct("ConfirmedCourse")
            .field("path", &choice.path)
            .field("difficulty", &choice.difficulty)
            .finish()
    }
}

pub struct PreparedCourseStart {
    pub plan: CourseSessionPlan,
    pub song: Arc<SongData>,
    pub endless_source: Option<EndlessCourseSource>,
}

/// One course and its resolved ratings, shared with the selection screen.
pub struct PreparedCourse {
    source: CourseFile,
    path: PathBuf,
    score_hash: String,
    name: String,
    banner_path: Option<PathBuf>,
    ratings: Vec<Option<PreparedCourseRating>>,
    default_rating_index: usize,
    preparation: CoursePreparation,
}

impl PreparedCourse {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn score_hash(&self) -> &str {
        &self.score_hash
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn scripter(&self) -> &str {
        &self.source.scripter
    }
    pub fn description(&self) -> &str {
        &self.source.description
    }
    pub fn banner_path(&self) -> Option<&Path> {
        self.banner_path.as_deref()
    }
    pub fn ratings(&self) -> &[Option<PreparedCourseRating>] {
        &self.ratings
    }
    pub const fn default_rating_index(&self) -> usize {
        self.default_rating_index
    }
    pub const fn has_random_entries(&self) -> bool {
        self.preparation.has_random_entries
    }
    pub const fn has_most_played_entries(&self) -> bool {
        self.preparation.has_most_played_entries
    }

    fn plan(
        &self,
        difficulty: Difficulty,
        meter: Option<u32>,
        stages: Vec<CourseStagePlan>,
    ) -> CourseSessionPlan {
        CourseSessionPlan {
            path: self.path.clone(),
            score_hash: self.score_hash.clone(),
            course_difficulty_name: course::difficulty_label(difficulty).to_owned(),
            course_meter: meter,
            course_type: self.preparation.course_type,
            lives: self.source.lives,
            stages,
        }
    }

    fn summary_song(&self) -> SongData {
        let rating = self.ratings[self.default_rating_index]
            .as_ref()
            .expect("course default rating");
        course_song_stub(
            &self.path,
            &self.name,
            &self.source.scripter,
            self.banner_path(),
            rating.min_bpm,
            rating.max_bpm,
            rating.total_length_seconds,
        )
    }
}

/// A catalog resolves each rating once. Confirmation and display use that same resolution.
pub struct PreparedCourseCatalog {
    resolver: Arc<CourseResolver>,
    courses: Vec<PreparedCourse>,
    by_path: FxHashMap<PathBuf, usize>,
}

impl PreparedCourseCatalog {
    pub fn new(
        courses: Vec<(PathBuf, CourseFile)>,
        song_packs: &[SongPack],
        played_chart_counts: &[(String, u32)],
        chart_grades: &[(String, u8)],
        chart_type: &str,
    ) -> Self {
        let resolver = Arc::new(CourseResolver::new(
            song_packs,
            played_chart_counts,
            chart_grades,
            chart_type,
        ));
        let mut catalog = Self {
            resolver,
            courses: Vec::with_capacity(courses.len()),
            by_path: FxHashMap::default(),
        };
        for (path, course) in courses {
            let preparation = CoursePreparation::new(&course);
            let seed = fresh_course_seed();
            let mut ratings = Vec::from([const { None }; COURSE_RATING_ORDER.len()]);
            for difficulty in preparation.difficulties() {
                ratings[difficulty as usize] =
                    catalog
                        .resolver
                        .prepare_rating(&path, &course, difficulty, seed, |_, _, _| {});
            }
            let default_rating_index = preparation.default_rating_index(&ratings);
            let name = if course.name.trim().is_empty() {
                path.file_stem()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_default()
                    .to_owned()
            } else {
                course.name.clone()
            };
            catalog.by_path.insert(path.clone(), catalog.courses.len());
            catalog.courses.push(PreparedCourse {
                score_hash: course_score_hash(&path),
                name,
                banner_path: course::resolve_course_banner_path(&path, &course.banner),
                path,
                source: course,
                ratings,
                default_rating_index,
                preparation,
            });
        }
        catalog
    }

    pub fn courses(&self) -> &[PreparedCourse] {
        &self.courses
    }

    fn rating(&self, choice: CourseChoice<'_>) -> Option<(&PreparedCourse, &PreparedCourseRating)> {
        let course = &self.courses[*self.by_path.get(choice.path)?];
        let rating = course.ratings[choice.difficulty as usize].as_ref()?;
        Some((course, rating))
    }

    pub fn can_start(&self, choice: CourseChoice<'_>) -> bool {
        self.rating(choice)
            .is_some_and(|(_, rating)| !rating.stages.is_empty())
    }

    pub fn confirm(self: &Arc<Self>, choice: CourseChoice<'_>) -> Option<ConfirmedCourse> {
        let course_index = *self.by_path.get(choice.path)?;
        Some(ConfirmedCourse {
            catalog: Arc::clone(self),
            course_index,
            difficulty: choice.difficulty,
        })
    }

    pub fn selected_cycle(&self, choice: CourseChoice<'_>) -> Option<CourseCyclePlan> {
        let (course, rating) = self.rating(choice)?;
        if rating.stages.is_empty() {
            return None;
        }
        Some(CourseCyclePlan {
            path: course.path.clone(),
            stages: rating.stages.clone(),
        })
    }

    pub fn selected_plan(&self, choice: CourseChoice<'_>) -> Option<CourseSessionPlan> {
        let (course, rating) = self.rating(choice)?;
        if rating.stages.is_empty() {
            return None;
        }
        Some(course.plan(
            choice.difficulty,
            rating.course_meter,
            rating.stages.clone(),
        ))
    }

    pub fn start(&self, choice: CourseChoice<'_>) -> Option<PreparedCourseStart> {
        let (course, rating) = self.rating(choice)?;
        if rating.stages.is_empty() {
            return None;
        }
        let plan = course.plan(
            choice.difficulty,
            rating.course_meter,
            rating.stages.clone(),
        );
        let endless_source = EndlessCourseSource::for_selection(
            &self.resolver,
            &course.source,
            choice.difficulty,
            &plan,
        );
        Some(PreparedCourseStart {
            plan,
            song: Arc::new(course.summary_song()),
            endless_source,
        })
    }

    pub fn reroll_cycle(&self, choice: CourseChoice<'_>) -> Option<CourseCyclePlan> {
        let (course, _) = self.rating(choice)?;
        let stages = self.resolver.reroll_endless(
            &course.path,
            &course.source,
            choice.difficulty,
            fresh_course_seed(),
        )?;
        Some(CourseCyclePlan {
            path: course.path.clone(),
            stages,
        })
    }

    pub fn reroll_plan(&self, choice: CourseChoice<'_>) -> Option<CourseSessionPlan> {
        let (course, rating) = self.rating(choice)?;
        let cycle = self.reroll_cycle(choice)?;
        Some(course.plan(choice.difficulty, rating.course_meter, cycle.stages))
    }
}

/// Build the neutral song record used for course wheel entries and completed summaries.
pub fn course_song_stub(
    path: &Path,
    name: &str,
    scripter: &str,
    banner_path: Option<&Path>,
    min_bpm: Option<f64>,
    max_bpm: Option<f64>,
    total_length_seconds: i32,
) -> SongData {
    let length = total_length_seconds.max(0);
    SongData {
        simfile_path: path.to_path_buf(),
        title: name.to_owned(),
        subtitle: String::new(),
        translit_title: name.to_owned(),
        translit_subtitle: String::new(),
        artist: scripter.to_owned(),
        translit_artist: String::new(),
        genre: String::new(),
        banner_path: banner_path.map(Path::to_path_buf),
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
        min_bpm: min_bpm.unwrap_or(0.0),
        max_bpm: max_bpm.unwrap_or_else(|| min_bpm.unwrap_or(0.0)),
        normalized_bpms: String::new(),
        song_timing: None,
        music_length_seconds: length as f32,
        first_second: 0.0,
        total_length_seconds: length,
        precise_last_second_seconds: length as f32,
        precise_note_first_second_seconds: None,
        precise_note_last_second_seconds: Some(length as f32),
        last_second_hint: 0.0,
        charts: Vec::new(),
    }
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
        let mut chart_indices = Vec::with_capacity(course.entries.len());
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
            chart_indices.push(stage.chart_index);
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
            chart_indices,
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
