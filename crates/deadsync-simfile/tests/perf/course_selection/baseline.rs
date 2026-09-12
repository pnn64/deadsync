// Frozen selection code from 12e4c25ff (0.5.1147). Only test visibility changed.
// Unchanged path hashing, song keys, and difficulty shifting helpers are shared.
#![allow(dead_code, clippy::all)]
use super::*;

pub fn resolve_course_stage(
    course_path: &Path,
    entry_index: usize,
    random_seed: u64,
    entry: &CourseEntry,
    by_group_song: &HashMap<(String, String), Arc<SongData>>,
    by_song: &HashMap<String, Arc<SongData>>,
    all_songs: &[Arc<SongData>],
    songs_by_group: &HashMap<String, Vec<Arc<SongData>>>,
    song_play_counts: &HashMap<String, u32>,
    song_grade_counts: &HashMap<String, CourseGradeCounts>,
    course_type: CourseType,
    selected_song_keys: &[String],
    chart_type: &str,
    course_difficulty: Difficulty,
) -> Option<ResolvedCourseStage> {
    let mut candidates = match &entry.song {
        CourseSong::Fixed { group, song } => {
            let song_key = song.trim().to_ascii_lowercase();
            let resolved = if let Some(group) = group.as_deref().map(str::trim) {
                let group_key = group.to_ascii_lowercase();
                by_group_song.get(&(group_key, song_key)).cloned()
            } else {
                by_song.get(&song_key).cloned()
            }?;
            course_candidates(std::slice::from_ref(&resolved), entry, chart_type)
        }
        CourseSong::SortPick { .. } | CourseSong::RandomAny => {
            course_candidates(all_songs, entry, chart_type)
        }
        CourseSong::RandomWithinGroup { group } => songs_by_group
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(group.trim()))
            .map(|(_, songs)| course_candidates(songs, entry, chart_type))
            .unwrap_or_default(),
        CourseSong::Select(select) => {
            let pool = select_song_pool(select, all_songs, songs_by_group);
            let mut candidates = course_candidates(&pool, entry, chart_type);
            candidates
                .items
                .retain(|candidate| song_select_matches(&candidate.song, select));
            candidates
        }
        CourseSong::Unknown { .. } => return None,
    };

    let (sort, pick) = match &entry.song {
        CourseSong::SortPick { sort, index } => (Some(*sort), (*index).max(0) as usize),
        CourseSong::Select(select) => (select.sort, select.index.max(0) as usize),
        _ => (None, 0),
    };
    avoid_course_repeats(
        &mut candidates.items,
        course_type,
        sort.is_none(),
        selected_song_keys,
    );
    let candidate = pick_course_candidate(
        &mut candidates.items,
        sort,
        pick,
        song_play_counts,
        song_grade_counts,
        random_seed ^ ((course_difficulty as u64) << 32),
        course_path,
        entry_index,
    )?;
    let chart_pick = random_pick_index(
        random_seed ^ song_key_hash(candidate.song_key()),
        course_path,
        entry_index ^ usize::MAX,
        candidate.chart_indices.len(),
    );
    let base_chart = *candidates
        .chart_indices
        .get(candidate.chart_indices.clone())?
        .get(chart_pick)?;
    let chart_index = shifted_chart_index(
        &candidate.song,
        base_chart,
        entry,
        chart_type,
        course_difficulty,
    );
    Some(ResolvedCourseStage {
        song: candidate.song,
        chart_index,
        modifiers: entry.modifiers.clone(),
        gain_seconds: entry.gain_seconds,
        gain_lives: entry.gain_lives,
    })
}

fn avoid_course_repeats(
    candidates: &mut Vec<CourseCandidate>,
    course_type: CourseType,
    random_sort: bool,
    selected_song_keys: &[String],
) {
    if candidates.len() <= 1 || selected_song_keys.is_empty() {
        return;
    }
    let last = selected_song_keys.last().map(String::as_str);
    if course_type != CourseType::Endless {
        candidates.retain(|candidate| Some(candidate.song_key()) != last);
        return;
    }
    if !random_sort {
        return;
    }

    let selected_lookup = (selected_song_keys.len() > 8).then(|| {
        selected_song_keys
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>()
    });
    let is_selected = |key: &str| {
        selected_lookup.as_ref().map_or_else(
            || selected_song_keys.iter().any(|selected| selected == key),
            |selected| selected.contains(key),
        )
    };
    let has_unplayed = candidates
        .iter()
        .any(|candidate| !is_selected(candidate.song_key()));
    if has_unplayed {
        candidates.retain(|candidate| !is_selected(candidate.song_key()));
        return;
    }

    if candidates
        .iter()
        .any(|candidate| Some(candidate.song_key()) != last)
    {
        candidates.retain(|candidate| Some(candidate.song_key()) != last);
    }
}

#[derive(Clone)]
pub(super) struct CourseCandidate {
    pub(super) song: Arc<SongData>,
    pub(super) song_key: OnceCell<String>,
    pub(super) chart_indices: std::ops::Range<usize>,
    pub(super) source_index: usize,
}

impl CourseCandidate {
    pub(super) fn song_key(&self) -> &str {
        self.song_key
            .get_or_init(|| song_unique_key(&self.song))
            .as_str()
    }
}

#[derive(Clone, Default)]
pub(super) struct CourseCandidates {
    pub(super) items: Vec<CourseCandidate>,
    pub(super) chart_indices: Vec<usize>,
}

pub(super) fn course_candidates(
    songs: &[Arc<SongData>],
    entry: &CourseEntry,
    chart_type: &str,
) -> CourseCandidates {
    let chart_capacity = songs.iter().map(|song| song.charts.len()).sum();
    let mut candidates = CourseCandidates {
        items: Vec::with_capacity(songs.len()),
        chart_indices: Vec::with_capacity(chart_capacity),
    };
    for (source_index, song) in songs.iter().enumerate() {
        let chart_indices =
            append_matching_chart_indices(song, entry, chart_type, &mut candidates.chart_indices);
        if !chart_indices.is_empty() {
            candidates.items.push(CourseCandidate {
                song: song.clone(),
                song_key: OnceCell::new(),
                chart_indices,
                source_index,
            });
        }
    }
    candidates
}

fn append_matching_chart_indices(
    song: &SongData,
    entry: &CourseEntry,
    chart_type: &str,
    indices: &mut Vec<usize>,
) -> std::ops::Range<usize> {
    let start = indices.len();
    for (index, chart) in song.charts.iter().enumerate() {
        if chart.has_note_data
            && chart.chart_type.eq_ignore_ascii_case(chart_type)
            && chart_matches_entry(chart, entry)
        {
            indices.push(index);
        }
    }
    start..indices.len()
}

fn chart_matches_entry(chart: &ChartData, entry: &CourseEntry) -> bool {
    if let CourseSong::Select(select) = &entry.song {
        let difficulty_matches = select.difficulties.is_empty()
            || select.difficulties.iter().any(|diff| {
                chart
                    .difficulty
                    .eq_ignore_ascii_case(difficulty_label(*diff))
            });
        let meter_matches = select.meter_range.is_none_or(|(low, high)| {
            let meter = chart.meter as i32;
            meter >= low && meter <= high
        });
        return difficulty_matches && meter_matches;
    }
    match &entry.steps {
        StepsSpec::Difficulty(diff) => chart
            .difficulty
            .eq_ignore_ascii_case(difficulty_label(*diff)),
        StepsSpec::MeterRange { low, high } => {
            let meter = chart.meter as i32;
            meter >= *low && meter <= *high
        }
        StepsSpec::Unknown { .. } => false,
    }
}

fn shifted_chart_index(
    song: &SongData,
    base_index: usize,
    entry: &CourseEntry,
    chart_type: &str,
    course_difficulty: Difficulty,
) -> usize {
    if course_difficulty == Difficulty::Medium || entry.no_difficult {
        return base_index;
    }
    let Some(base) = song.charts.get(base_index) else {
        return base_index;
    };
    let Some(base_diff) = chart_difficulty(base) else {
        return base_index;
    };
    let shifted = shifted_course_difficulty(base_diff, course_difficulty);
    song.charts
        .iter()
        .position(|chart| {
            chart.has_note_data
                && chart.chart_type.eq_ignore_ascii_case(chart_type)
                && chart
                    .difficulty
                    .eq_ignore_ascii_case(difficulty_label(shifted))
        })
        .unwrap_or(base_index)
}

fn chart_difficulty(chart: &ChartData) -> Option<Difficulty> {
    COURSE_RATING_ORDER.into_iter().find(|diff| {
        chart
            .difficulty
            .eq_ignore_ascii_case(difficulty_label(*diff))
    })
}

pub(super) fn select_song_pool(
    select: &SongSelect,
    all_songs: &[Arc<SongData>],
    songs_by_group: &HashMap<String, Vec<Arc<SongData>>>,
) -> Vec<Arc<SongData>> {
    if select.groups.is_empty() {
        return all_songs.to_vec();
    }
    let mut songs = Vec::new();
    for group in &select.groups {
        if let Some(group_songs) = songs_by_group.get(group) {
            songs.extend(group_songs.iter().cloned());
        }
    }
    songs
}

fn song_select_matches(song: &SongData, select: &SongSelect) -> bool {
    let song_dir = song
        .simfile_path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !select.titles.is_empty()
        && !select
            .titles
            .iter()
            .any(|title| title == song_dir || title == &song.title || title == &song.translit_title)
    {
        return false;
    }
    if !select.artists.is_empty()
        && !select
            .artists
            .iter()
            .any(|artist| artist == &song.artist || artist == &song.translit_artist)
    {
        return false;
    }
    if !select.genres.is_empty() && !select.genres.iter().any(|genre| genre == &song.genre) {
        return false;
    }
    if let Some((low, high)) = select.bpm_range {
        let Some((song_low, song_high)) = song.display_bpm_range() else {
            return false;
        };
        if song_low < low || song_high > high {
            return false;
        }
    }
    if let Some((low, high)) = select.duration_range {
        if song.music_length_seconds < low || song.music_length_seconds > high {
            return false;
        }
    }
    true
}

fn pick_course_candidate(
    candidates: &mut Vec<CourseCandidate>,
    sort: Option<SongSort>,
    pick: usize,
    song_play_counts: &HashMap<String, u32>,
    song_grade_counts: &HashMap<String, CourseGradeCounts>,
    random_seed: u64,
    course_path: &Path,
    entry_index: usize,
) -> Option<CourseCandidate> {
    if candidates.is_empty() {
        return None;
    }
    let index = if let Some(sort) = sort {
        selected_course_candidate_index(
            candidates,
            sort,
            pick,
            song_play_counts,
            song_grade_counts,
        )?
    } else {
        random_pick_index(random_seed, course_path, entry_index, candidates.len())
    };
    Some(candidates.swap_remove(index))
}

enum RankedCourseCandidates<'a> {
    Plays(Vec<(u32, usize)>),
    Grades(Vec<(&'a CourseGradeCounts, usize)>),
}

fn ranked_course_candidates<'a>(
    candidates: &[CourseCandidate],
    sort: SongSort,
    song_play_counts: &HashMap<String, u32>,
    song_grade_counts: &'a HashMap<String, CourseGradeCounts>,
) -> RankedCourseCandidates<'a> {
    static EMPTY_GRADE_COUNTS: CourseGradeCounts = [0; 19];
    match sort {
        SongSort::MostPlays | SongSort::FewestPlays => RankedCourseCandidates::Plays(
            candidates
                .iter()
                .enumerate()
                .map(|(index, candidate)| {
                    (
                        song_play_counts
                            .get(candidate.song_key())
                            .copied()
                            .unwrap_or(0),
                        index,
                    )
                })
                .collect(),
        ),
        SongSort::TopGrades | SongSort::LowestGrades => RankedCourseCandidates::Grades(
            candidates
                .iter()
                .enumerate()
                .map(|(index, candidate)| {
                    (
                        song_grade_counts
                            .get(candidate.song_key())
                            .unwrap_or(&EMPTY_GRADE_COUNTS),
                        index,
                    )
                })
                .collect(),
        ),
    }
}

fn compare_ranked_course_candidates<T: Ord>(
    left: &(T, usize),
    right: &(T, usize),
    candidates: &[CourseCandidate],
    descending: bool,
) -> std::cmp::Ordering {
    let order = if descending {
        right.0.cmp(&left.0)
    } else {
        left.0.cmp(&right.0)
    };
    order.then(
        candidates[left.1]
            .source_index
            .cmp(&candidates[right.1].source_index),
    )
}

fn select_ranked_course_candidate(
    ranked: &mut RankedCourseCandidates<'_>,
    candidates: &[CourseCandidate],
    sort: SongSort,
    pick: usize,
) -> usize {
    let descending = matches!(sort, SongSort::MostPlays | SongSort::TopGrades);
    match ranked {
        RankedCourseCandidates::Plays(ranked) => {
            ranked.select_nth_unstable_by(pick, |left, right| {
                compare_ranked_course_candidates(left, right, candidates, descending)
            });
            ranked[pick].1
        }
        RankedCourseCandidates::Grades(ranked) => {
            ranked.select_nth_unstable_by(pick, |left, right| {
                compare_ranked_course_candidates(left, right, candidates, descending)
            });
            ranked[pick].1
        }
    }
}

pub(super) fn selected_course_candidate_index(
    candidates: &[CourseCandidate],
    sort: SongSort,
    pick: usize,
    song_play_counts: &HashMap<String, u32>,
    song_grade_counts: &HashMap<String, CourseGradeCounts>,
) -> Option<usize> {
    if pick >= candidates.len() {
        return None;
    }
    let mut ranked =
        ranked_course_candidates(candidates, sort, song_play_counts, song_grade_counts);
    Some(select_ranked_course_candidate(
        &mut ranked,
        candidates,
        sort,
        pick,
    ))
}
