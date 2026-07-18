use crate::runtime_cache;
use crate::scan::{RuntimeScanLogEntry, fmt_scan_time, push_unique_path};
use deadsync_chart::{ChartData, SongData, SongPack};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use twox_hash::XxHash64;

pub use rssp::course::{
    CourseEntry, CourseFile, CourseSong, Difficulty, SongSort, StepsSpec, difficulty_label,
    resolve_course_banner_path,
};

pub type LoadedCourse = (PathBuf, CourseFile);

pub const COURSE_RATING_ORDER: [Difficulty; 6] = [
    Difficulty::Beginner,
    Difficulty::Easy,
    Difficulty::Medium,
    Difficulty::Hard,
    Difficulty::Challenge,
    Difficulty::Edit,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CourseTotals {
    pub steps: u32,
    pub jumps: u32,
    pub holds: u32,
    pub mines: u32,
    pub hands: u32,
    pub rolls: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseRefError {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseLoadFailure {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct CourseLoadReport {
    pub courses: Vec<LoadedCourse>,
    pub failures: Vec<CourseLoadFailure>,
}

#[derive(Debug, Clone)]
pub struct CourseScanReport {
    pub courses: Vec<LoadedCourse>,
    pub failures: Vec<CourseLoadFailure>,
    pub autogen_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseScanRoots {
    pub roots: Vec<PathBuf>,
    pub primary_missing: bool,
}

#[derive(Debug, Clone)]
pub struct RuntimeCourseScanInput {
    pub courses_root: PathBuf,
    pub course_roots: Vec<PathBuf>,
    pub song_roots: Vec<PathBuf>,
    pub autogen_courses_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCourseScanEvent {
    Start {
        courses_root: PathBuf,
    },
    NoSongRoots,
    NoCourseRoots,
    Failure {
        message: String,
    },
    Finished {
        courses: usize,
        autogen_count: usize,
        failures: usize,
        elapsed: Duration,
    },
}

pub fn runtime_course_scan_log_entry(event: RuntimeCourseScanEvent) -> RuntimeScanLogEntry {
    match event {
        RuntimeCourseScanEvent::Start { courses_root } => RuntimeScanLogEntry::info(format!(
            "Starting course scan in '{}'...",
            courses_root.display()
        )),
        RuntimeCourseScanEvent::NoSongRoots => {
            RuntimeScanLogEntry::warn("No valid song roots found. No courses will be loaded.")
        }
        RuntimeCourseScanEvent::NoCourseRoots => {
            RuntimeScanLogEntry::warn("No valid course roots found. No courses will be loaded.")
        }
        RuntimeCourseScanEvent::Failure { message } => RuntimeScanLogEntry::warn(message),
        RuntimeCourseScanEvent::Finished {
            courses,
            autogen_count,
            failures,
            elapsed,
        } => RuntimeScanLogEntry::info(format!(
            "Finished course scan. Loaded {} courses ({} autogen, failed {}) in {}.",
            courses,
            autogen_count,
            failures,
            fmt_scan_time(elapsed)
        )),
    }
}

impl fmt::Display for CourseRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

pub fn parse_course_file(path: &Path) -> Result<CourseFile, String> {
    let data = fs::read(path)
        .map_err(|error| format!("Failed to read course '{}': {error}", path.display()))?;
    rssp::course::parse_crs(&data)
        .map_err(|error| format!("Failed to parse course '{}': {error}", path.display()))
}

pub fn collect_merged_course_paths(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        for path in collect_course_paths(root) {
            let mut key = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if cfg!(windows) {
                key.make_ascii_lowercase();
            }
            if seen.insert(key) {
                out.push(path);
            }
        }
    }
    out.sort_by_cached_key(|p| p.to_string_lossy().to_ascii_lowercase());
    out
}

pub fn collect_course_scan_roots(
    primary_root: &Path,
    extra_roots: impl IntoIterator<Item = PathBuf>,
) -> CourseScanRoots {
    let mut roots = Vec::with_capacity(2);
    let mut keys = Vec::with_capacity(2);
    let primary_missing = !primary_root.is_dir();
    if !primary_missing {
        push_unique_path(primary_root.to_path_buf(), &mut roots, &mut keys);
    }
    for extra in extra_roots {
        push_unique_path(extra, &mut roots, &mut keys);
    }
    CourseScanRoots {
        roots,
        primary_missing,
    }
}

pub fn course_progress_names<'a>(path: &'a Path, root: &'a Path) -> (&'a str, &'a str) {
    let fallback = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("courses");
    let group = path
        .parent()
        .and_then(|dir| dir.file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback);
    let course = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or_default();
    (group, course)
}

pub fn load_course_paths_with_progress<F>(
    course_paths: Vec<PathBuf>,
    progress_root: &Path,
    song_roots: &[PathBuf],
    total_song_count: usize,
    mut progress: Option<&mut F>,
) -> CourseLoadReport
where
    F: FnMut(usize, usize, &str, &str),
{
    let total_courses = course_paths.len();
    let mut courses = Vec::with_capacity(total_courses);
    let mut failures = Vec::new();
    let mut group_dirs = HashMap::new();
    let mut courses_done = 0usize;
    report_load_progress(&mut progress, 0, total_courses, "", "");

    for course_path in course_paths {
        let (group_display, course_display) = course_progress_names(&course_path, progress_root);
        let group_display = group_display.to_owned();
        let course_display = course_display.to_owned();
        let mut report_done = || {
            courses_done = courses_done.saturating_add(1);
            report_load_progress(
                &mut progress,
                courses_done,
                total_courses,
                &group_display,
                &course_display,
            );
        };

        let course = match parse_course_file(&course_path) {
            Ok(course) => course,
            Err(message) => {
                failures.push(CourseLoadFailure {
                    path: course_path,
                    message,
                });
                report_done();
                continue;
            }
        };

        match validate_course_refs(&course, song_roots, &mut group_dirs, total_song_count) {
            Ok(()) => courses.push((course_path, course)),
            Err(error) => failures.push(CourseLoadFailure {
                path: course_path,
                message: error.message,
            }),
        }
        report_done();
    }

    CourseLoadReport { courses, failures }
}

pub fn count_course_songs(packs: &[SongPack]) -> usize {
    packs.iter().map(|pack| pack.songs.len()).sum()
}

pub fn song_unique_key(song: &SongData) -> String {
    song.simfile_path
        .parent()
        .map(|p| p.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| song.simfile_path.to_string_lossy().to_ascii_lowercase())
}

pub fn nearest_filled_slot<T>(slots: &[Option<T>], preferred: usize) -> Option<usize> {
    if slots.is_empty() {
        return None;
    }
    let preferred = preferred.min(slots.len().saturating_sub(1));
    if slots[preferred].is_some() {
        return Some(preferred);
    }
    let mut best = None;
    let mut best_dist = usize::MAX;
    for (idx, slot) in slots.iter().enumerate() {
        if slot.is_none() {
            continue;
        }
        let dist = idx.abs_diff(preferred);
        if best.is_none() || dist < best_dist {
            best = Some(idx);
            best_dist = dist;
        }
    }
    best
}

pub fn shifted_course_difficulty(base: Difficulty, course: Difficulty) -> Difficulty {
    let base = base as i32;
    let delta = (course as i32) - (Difficulty::Medium as i32);
    let mut idx = base + delta;
    if idx < 0 {
        idx = 0;
    }
    if idx > Difficulty::Challenge as i32 {
        idx = Difficulty::Challenge as i32;
    }
    match idx {
        0 => Difficulty::Beginner,
        1 => Difficulty::Easy,
        2 => Difficulty::Medium,
        3 => Difficulty::Hard,
        _ => Difficulty::Challenge,
    }
}

pub const fn course_meter(course: &CourseFile, diff: Difficulty) -> Option<i32> {
    course.meters[diff as usize]
}

pub fn course_difficulty_from_meters(course: &CourseFile) -> Option<(&'static str, u32)> {
    const ORDER: [(Difficulty, &str); 6] = [
        (Difficulty::Challenge, "Challenge"),
        (Difficulty::Hard, "Hard"),
        (Difficulty::Medium, "Medium"),
        (Difficulty::Easy, "Easy"),
        (Difficulty::Beginner, "Beginner"),
        (Difficulty::Edit, "Edit"),
    ];
    for (diff, name) in ORDER {
        if let Some(meter) = course_meter(course, diff).filter(|v| *v >= 0) {
            return Some((name, meter as u32));
        }
    }
    None
}

pub fn resolve_course_chart<'a>(
    song: &'a SongData,
    entry: &CourseEntry,
    chart_type: &str,
    course_difficulty: Difficulty,
) -> Option<&'a ChartData> {
    let mut first_chart = None;
    let mut first_playable = None;
    let mut meter_match = None;
    let target_diff = match &entry.steps {
        StepsSpec::Difficulty(diff) => {
            let selected = if course_difficulty != Difficulty::Medium && !entry.no_difficult {
                shifted_course_difficulty(*diff, course_difficulty)
            } else {
                *diff
            };
            Some(difficulty_label(selected))
        }
        _ => None,
    };

    for chart in &song.charts {
        if !chart.chart_type.eq_ignore_ascii_case(chart_type) {
            continue;
        }
        if first_chart.is_none() {
            first_chart = Some(chart);
        }
        if !chart.has_note_data {
            continue;
        }
        if first_playable.is_none() {
            first_playable = Some(chart);
        }
        if let Some(target) = target_diff
            && chart.difficulty.eq_ignore_ascii_case(target)
        {
            return Some(chart);
        }
        if let StepsSpec::MeterRange { low, high } = &entry.steps {
            let meter = chart.meter as i32;
            if meter >= *low && meter <= *high && meter_match.is_none() {
                meter_match = Some(chart);
            }
        }
    }

    meter_match.or(first_playable).or(first_chart)
}

pub fn resolve_entry_song(
    course_path: &Path,
    entry_index: usize,
    random_seed: u64,
    entry: &CourseEntry,
    by_group_song: &HashMap<(String, String), Arc<SongData>>,
    by_song: &HashMap<String, Arc<SongData>>,
    all_songs: &[Arc<SongData>],
    songs_by_group: &HashMap<String, Vec<Arc<SongData>>>,
    song_play_counts: &HashMap<String, u32>,
    used_song_keys: &HashSet<String>,
    chart_type: &str,
    course_difficulty: Difficulty,
) -> Option<Arc<SongData>> {
    match &entry.song {
        CourseSong::Fixed { group, song } => {
            let song_key = song.trim().to_ascii_lowercase();
            if let Some(group) = group.as_deref().map(str::trim) {
                let group_key = group.to_ascii_lowercase();
                by_group_song.get(&(group_key, song_key)).cloned()
            } else {
                by_song.get(&song_key).cloned()
            }
        }
        CourseSong::SortPick { sort, index } => resolve_sort_pick_song(
            all_songs,
            song_play_counts,
            entry,
            chart_type,
            course_difficulty,
            *sort,
            *index,
        ),
        CourseSong::RandomAny | CourseSong::RandomWithinGroup { .. } => {
            let seeded = random_seed ^ ((course_difficulty as u64) << 32);
            resolve_random_song(
                course_path,
                entry_index,
                seeded,
                all_songs,
                songs_by_group,
                used_song_keys,
                entry,
                chart_type,
                course_difficulty,
            )
        }
        CourseSong::Unknown { .. } => None,
    }
}

pub fn push_song_bpm_range(min_bpm: &mut Option<f64>, max_bpm: &mut Option<f64>, song: &SongData) {
    let mut lo = song.min_bpm;
    let mut hi = song.max_bpm;
    if lo <= 0.0 && hi > 0.0 {
        lo = hi;
    }
    if hi <= 0.0 && lo > 0.0 {
        hi = lo;
    }
    if lo <= 0.0 || hi <= 0.0 {
        return;
    }
    *min_bpm = Some(min_bpm.map_or(lo, |curr| curr.min(lo)));
    *max_bpm = Some(max_bpm.map_or(hi, |curr| curr.max(hi)));
}

pub fn add_chart_totals(totals: &mut CourseTotals, chart: &ChartData) {
    totals.steps = totals.steps.saturating_add(chart.stats.total_steps);
    totals.jumps = totals.jumps.saturating_add(chart.stats.jumps);
    totals.holds = totals.holds.saturating_add(chart.stats.holds);
    totals.mines = totals.mines.saturating_add(chart.mines_nonfake);
    totals.hands = totals.hands.saturating_add(chart.stats.hands);
    totals.rolls = totals.rolls.saturating_add(chart.stats.rolls);
}

fn resolve_sort_pick_song(
    all_songs: &[Arc<SongData>],
    song_play_counts: &HashMap<String, u32>,
    entry: &CourseEntry,
    chart_type: &str,
    course_difficulty: Difficulty,
    sort: SongSort,
    index: i32,
) -> Option<Arc<SongData>> {
    if matches!(sort, SongSort::TopGrades | SongSort::LowestGrades) {
        return None;
    }

    let mut ranked = Vec::new();
    for (song_index, song) in all_songs.iter().enumerate() {
        if resolve_course_chart(song, entry, chart_type, course_difficulty).is_none() {
            continue;
        }
        let plays = song_play_counts
            .get(song_unique_key(song).as_str())
            .copied()
            .unwrap_or(0);
        ranked.push((plays, song_index));
    }

    let pick = index.max(0) as usize;
    select_song_by_play_rank(all_songs, ranked, sort, pick)
}

fn select_song_by_play_rank(
    all_songs: &[Arc<SongData>],
    mut ranked: Vec<(u32, usize)>,
    sort: SongSort,
    pick: usize,
) -> Option<Arc<SongData>> {
    if pick >= ranked.len() {
        return None;
    }
    match sort {
        SongSort::MostPlays => {
            ranked.select_nth_unstable_by(pick, |a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        }
        SongSort::FewestPlays => {
            ranked.select_nth_unstable_by(pick, |a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        }
        SongSort::TopGrades | SongSort::LowestGrades => {
            return None;
        }
    }
    all_songs.get(ranked[pick].1).cloned()
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn select_song_by_play_rank_for_bench(
    all_songs: &[Arc<SongData>],
    play_counts: &[u32],
    sort: SongSort,
    pick: usize,
) -> Option<Arc<SongData>> {
    let ranked = play_counts
        .iter()
        .copied()
        .take(all_songs.len())
        .enumerate()
        .map(|(song_index, plays)| (plays, song_index))
        .collect();
    select_song_by_play_rank(all_songs, ranked, sort, pick)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn select_song_by_play_rank_legacy_for_bench(
    all_songs: &[Arc<SongData>],
    play_counts: &[u32],
    sort: SongSort,
    pick: usize,
) -> Option<Arc<SongData>> {
    let mut ranked = all_songs
        .iter()
        .zip(play_counts)
        .map(|(song, &plays)| (plays, Arc::clone(song)))
        .collect::<Vec<_>>();
    match sort {
        SongSort::MostPlays => ranked.sort_by(|a, b| b.0.cmp(&a.0)),
        SongSort::FewestPlays => ranked.sort_by(|a, b| a.0.cmp(&b.0)),
        SongSort::TopGrades | SongSort::LowestGrades => return None,
    }
    ranked.get(pick).map(|(_, song)| Arc::clone(song))
}

fn random_pick_index(seed: u64, course_path: &Path, entry_index: usize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let mut hasher = XxHash64::with_seed(seed);
    hasher.write(course_path.to_string_lossy().as_bytes());
    hasher.write_u64(entry_index as u64);
    (hasher.finish() as usize) % len
}

fn resolve_random_song(
    course_path: &Path,
    entry_index: usize,
    random_seed: u64,
    all_songs: &[Arc<SongData>],
    songs_by_group: &HashMap<String, Vec<Arc<SongData>>>,
    used_song_keys: &HashSet<String>,
    entry: &CourseEntry,
    chart_type: &str,
    course_difficulty: Difficulty,
) -> Option<Arc<SongData>> {
    let pool: &[Arc<SongData>] = match &entry.song {
        CourseSong::RandomAny => all_songs,
        CourseSong::RandomWithinGroup { group } => songs_by_group
            .get(group.trim().to_ascii_lowercase().as_str())
            .map_or(&[], Vec::as_slice),
        _ => return None,
    };
    if pool.is_empty() {
        return None;
    }

    let mut all_candidates = Vec::new();
    let mut unused_candidates = Vec::new();
    for song in pool {
        if resolve_course_chart(song, entry, chart_type, course_difficulty).is_none() {
            continue;
        }
        all_candidates.push(song.clone());
        if !used_song_keys.contains(song_unique_key(song).as_str()) {
            unused_candidates.push(song.clone());
        }
    }

    let picked_pool = if unused_candidates.is_empty() {
        &all_candidates
    } else {
        &unused_candidates
    };
    if picked_pool.is_empty() {
        return None;
    }

    let idx = random_pick_index(random_seed, course_path, entry_index, picked_pool.len());
    picked_pool.get(idx).cloned()
}

pub fn load_course_scan_with_progress<F>(
    course_roots: &[PathBuf],
    progress_root: &Path,
    song_roots: &[PathBuf],
    autogen_courses_root: &Path,
    packs: &[SongPack],
    progress: Option<&mut F>,
) -> CourseScanReport
where
    F: FnMut(usize, usize, &str, &str),
{
    let course_paths = collect_merged_course_paths(course_roots);
    let mut report = load_course_paths_with_progress(
        course_paths,
        progress_root,
        song_roots,
        count_course_songs(packs),
        progress,
    );
    let autogen_courses = autogen_nonstop_group_courses(autogen_courses_root, packs);
    let autogen_count = autogen_courses.len();
    report.courses.extend(autogen_courses);
    CourseScanReport {
        courses: report.courses,
        failures: report.failures,
        autogen_count,
    }
}

pub fn scan_and_load_courses_runtime<F>(
    input: RuntimeCourseScanInput,
    progress: Option<&mut F>,
    mut event: impl FnMut(RuntimeCourseScanEvent),
) where
    F: FnMut(usize, usize, &str, &str),
{
    event(RuntimeCourseScanEvent::Start {
        courses_root: input.courses_root.clone(),
    });
    let started = Instant::now();

    if input.song_roots.is_empty() {
        event(RuntimeCourseScanEvent::NoSongRoots);
        runtime_cache::set_course_cache(Vec::new());
        return;
    }
    if input.course_roots.is_empty() {
        event(RuntimeCourseScanEvent::NoCourseRoots);
        runtime_cache::set_course_cache(Vec::new());
        return;
    }

    let report = {
        let song_cache = runtime_cache::get_song_cache();
        load_course_scan_with_progress(
            &input.course_roots,
            &input.courses_root,
            &input.song_roots,
            &input.autogen_courses_root,
            &song_cache,
            progress,
        )
    };
    for failure in &report.failures {
        event(RuntimeCourseScanEvent::Failure {
            message: failure.message.clone(),
        });
    }

    let courses = report.courses.len();
    let autogen_count = report.autogen_count;
    let failures = report.failures.len();
    runtime_cache::set_course_cache(report.courses);
    event(RuntimeCourseScanEvent::Finished {
        courses,
        autogen_count,
        failures,
        elapsed: started.elapsed(),
    });
}

pub fn autogen_nonstop_group_courses(
    courses_root: &Path,
    packs: &[SongPack],
) -> Vec<(PathBuf, CourseFile)> {
    let mut out = Vec::with_capacity(packs.len());

    for pack in packs {
        if pack.songs.is_empty() {
            continue;
        }

        let group_name = pack.group_name.trim();
        if group_name.is_empty() {
            continue;
        }
        let display_name = if pack.name.trim().is_empty() {
            group_name
        } else {
            pack.name.trim()
        };

        let entries = (0..4)
            .map(|_| CourseEntry {
                song: CourseSong::RandomWithinGroup {
                    group: group_name.to_string(),
                },
                steps: StepsSpec::Difficulty(Difficulty::Medium),
                modifiers: String::new(),
                secret: true,
                no_difficult: false,
                gain_lives: -1,
            })
            .collect();

        out.push((
            courses_root
                .join(group_name)
                .join("__deadsync_autogen_nonstop_random.crs"),
            CourseFile {
                name: format!("{display_name} Random"),
                name_translit: String::new(),
                scripter: "Autogen".to_string(),
                description: String::new(),
                banner: pack
                    .banner_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                background: String::new(),
                repeat: false,
                lives: -1,
                meters: [None; 6],
                entries,
            },
        ));
    }

    out
}

pub fn validate_course_refs(
    course: &CourseFile,
    song_roots: &[PathBuf],
    group_dirs: &mut HashMap<String, PathBuf>,
    total_song_count: usize,
) -> Result<(), CourseRefError> {
    for (idx, entry) in course.entries.iter().enumerate() {
        let entry_num = idx + 1;
        match &entry.song {
            rssp::course::CourseSong::Fixed { group, song } => {
                let Some(song_dir) =
                    resolve_song_dir(song_roots, group_dirs, group.as_deref(), song)
                else {
                    return Err(course_ref_error(format!(
                        "Course '{}' entry {entry_num} references missing song '{}{}'.",
                        course.name,
                        group
                            .as_deref()
                            .map(|g| format!("{g}/"))
                            .unwrap_or_default(),
                        song
                    )));
                };
                validate_song_dir(course, entry_num, &song_dir)?;
            }
            rssp::course::CourseSong::SortPick { sort, index } => {
                let supports_sort = matches!(
                    sort,
                    rssp::course::SongSort::MostPlays | rssp::course::SongSort::FewestPlays
                );
                if !supports_sort {
                    return Err(course_ref_error(format!(
                        "Course '{}' has unsupported sort selector in entry {entry_num} ({sort:?}).",
                        course.name
                    )));
                }

                let choose_index = (*index).max(0) as usize;
                if choose_index >= total_song_count {
                    return Err(course_ref_error(format!(
                        "Course '{}' entry {entry_num} references out-of-range sort pick '{}{}' with only {} songs installed.",
                        course.name,
                        sort_pick_label(*sort),
                        choose_index.saturating_add(1),
                        total_song_count
                    )));
                }
            }
            rssp::course::CourseSong::RandomAny => {}
            rssp::course::CourseSong::RandomWithinGroup { group } => {
                if resolve_course_group_dir(song_roots, group_dirs, group).is_none() {
                    return Err(course_ref_error(format!(
                        "Course '{}' entry {entry_num} references missing group '{}/*'.",
                        course.name, group
                    )));
                }
            }
            _ => {
                return Err(course_ref_error(format!(
                    "Course '{}' has unsupported song selector in entry {entry_num}.",
                    course.name
                )));
            }
        }
    }
    Ok(())
}

pub fn resolve_song_dir(
    song_roots: &[PathBuf],
    group_dirs: &mut HashMap<String, PathBuf>,
    group: Option<&str>,
    song: &str,
) -> Option<PathBuf> {
    let song = song.trim();
    if song.is_empty() {
        return None;
    }

    if let Some(group) = group.map(str::trim).filter(|g| !g.is_empty()) {
        let group_dir = resolve_course_group_dir(song_roots, group_dirs, group)?;
        return is_dir_ci(&group_dir, song);
    }

    for songs_root in song_roots.iter().rev() {
        let Ok(entries) = fs::read_dir(songs_root) else {
            continue;
        };
        for entry in entries.flatten() {
            let group_dir = entry.path();
            if !group_dir.is_dir() {
                continue;
            }
            if let Some(found) = is_dir_ci(&group_dir, song) {
                return Some(found);
            }
        }
    }
    None
}

pub fn resolve_course_group_dir(
    song_roots: &[PathBuf],
    group_dirs: &mut HashMap<String, PathBuf>,
    group: &str,
) -> Option<PathBuf> {
    let key = group.trim().to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }
    if let Some(path) = group_dirs.get(&key) {
        return Some(path.clone());
    }
    let mut path = None;
    for songs_root in song_roots.iter().rev() {
        path = is_dir_ci(songs_root, group);
        if path.is_some() {
            break;
        }
    }
    let path = path?;
    group_dirs.insert(key, path.clone());
    Some(path)
}

fn collect_course_paths(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("crs"))
            {
                out.push(path);
            }
        }
    }
    out.sort_by_cached_key(|p| p.to_string_lossy().to_ascii_lowercase());
    out
}

fn is_dir_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let want = name.trim();
    if want.is_empty() {
        return None;
    }
    let want_ci = want.to_ascii_lowercase();
    let Ok(entries) = fs::read_dir(dir) else {
        return None;
    };
    let mut ci_match = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let got = entry.file_name();
        let got = got.to_string_lossy();
        if got == want {
            return Some(path);
        }
        if ci_match.is_none() && got.to_ascii_lowercase() == want_ci {
            ci_match = Some(path);
        }
    }
    ci_match
}

#[inline(always)]
fn report_load_progress<F>(
    progress: &mut Option<&mut F>,
    done: usize,
    total: usize,
    group: &str,
    item: &str,
) where
    F: FnMut(usize, usize, &str, &str),
{
    if let Some(cb) = progress.as_mut() {
        cb(done, total, group, item);
    }
}

fn validate_song_dir(
    course: &CourseFile,
    entry_num: usize,
    song_dir: &Path,
) -> Result<(), CourseRefError> {
    match rssp::pack::scan_song_dir(song_dir, rssp::pack::ScanOpt::default()) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(course_ref_error(format!(
            "Course '{}' entry {entry_num} song dir has no simfile: {}",
            course.name,
            song_dir.display()
        ))),
        Err(error) => Err(course_ref_error(format!(
            "Course '{}' entry {entry_num} failed scanning song dir {}: {error:?}",
            course.name,
            song_dir.display()
        ))),
    }
}

fn sort_pick_label(sort: rssp::course::SongSort) -> &'static str {
    match sort {
        rssp::course::SongSort::MostPlays => "BEST",
        rssp::course::SongSort::FewestPlays => "WORST",
        rssp::course::SongSort::TopGrades => "GRADEBEST",
        rssp::course::SongSort::LowestGrades => "GRADEWORST",
    }
}

fn course_ref_error(message: String) -> CourseRefError {
    CourseRefError { message }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, SyncPref, TechCounts};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-simfile-course-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixed_course(group: Option<&str>, song: &str) -> CourseFile {
        CourseFile {
            name: "Course".to_string(),
            name_translit: String::new(),
            scripter: String::new(),
            description: String::new(),
            banner: String::new(),
            background: String::new(),
            repeat: false,
            lives: -1,
            meters: [None; 6],
            entries: vec![rssp::course::CourseEntry {
                song: rssp::course::CourseSong::Fixed {
                    group: group.map(str::to_string),
                    song: song.to_string(),
                },
                steps: rssp::course::StepsSpec::Difficulty(rssp::course::Difficulty::Medium),
                modifiers: String::new(),
                secret: false,
                no_difficult: false,
                gain_lives: -1,
            }],
        }
    }

    fn test_song() -> SongData {
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
            charts: Vec::new(),
        }
    }

    fn test_chart(difficulty: &str, meter: u32, has_note_data: bool, hash: &str) -> ChartData {
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: difficulty.to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter,
            step_artist: String::new(),
            music_path: None,
            short_hash: hash.to_string(),
            stats: ArrowStats {
                total_steps: meter,
                jumps: 1,
                hands: 2,
                holds: 3,
                rolls: 4,
                ..ArrowStats::default()
            },
            tech_counts: TechCounts::default(),
            mines_nonfake: 5,
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
            has_note_data,
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

    fn song_with_charts(path: &str, charts: Vec<ChartData>) -> Arc<SongData> {
        let mut song = test_song();
        song.simfile_path = PathBuf::from(path);
        song.min_bpm = 0.0;
        song.max_bpm = 180.0;
        song.charts = charts;
        Arc::new(song)
    }

    fn entry_with_steps(steps: StepsSpec) -> CourseEntry {
        CourseEntry {
            song: CourseSong::RandomAny,
            steps,
            modifiers: String::new(),
            secret: false,
            no_difficult: false,
            gain_lives: -1,
        }
    }

    fn song_pack(group_name: &str, name: &str, songs: usize) -> SongPack {
        SongPack {
            group_name: group_name.to_string(),
            name: name.to_string(),
            sort_title: String::new(),
            translit_title: String::new(),
            series: String::new(),
            year: 0,
            sync_pref: SyncPref::Default,
            directory: PathBuf::new(),
            banner_path: Some(PathBuf::from("banner.png")),
            songs: (0..songs).map(|_| Arc::new(test_song())).collect(),
        }
    }

    #[test]
    fn autogen_nonstop_group_courses_builds_random_medium_course() {
        let courses_root = PathBuf::from("courses");
        let courses =
            autogen_nonstop_group_courses(&courses_root, &[song_pack("Pack", "Display", 2)]);

        assert_eq!(courses.len(), 1);
        assert_eq!(
            courses[0].0,
            courses_root
                .join("Pack")
                .join("__deadsync_autogen_nonstop_random.crs")
        );
        let course = &courses[0].1;
        assert_eq!(course.name, "Display Random");
        assert_eq!(course.scripter, "Autogen");
        assert_eq!(course.banner, "banner.png");
        assert_eq!(course.entries.len(), 4);
        for entry in &course.entries {
            assert!(matches!(
                &entry.song,
                CourseSong::RandomWithinGroup { group } if group == "Pack"
            ));
            assert!(matches!(
                entry.steps,
                StepsSpec::Difficulty(Difficulty::Medium)
            ));
            assert!(entry.secret);
            assert_eq!(entry.gain_lives, -1);
        }
    }

    #[test]
    fn autogen_nonstop_group_courses_skips_empty_or_unnamed_packs() {
        let courses = autogen_nonstop_group_courses(
            Path::new("courses"),
            &[
                song_pack("Empty", "Empty", 0),
                song_pack("   ", "Unnamed", 1),
                song_pack("Valid", "", 1),
            ],
        );

        assert_eq!(courses.len(), 1);
        assert_eq!(courses[0].1.name, "Valid Random");
    }

    #[test]
    fn resolve_song_dir_prefers_later_root() {
        let root = test_dir("resolve-song-dir");
        let base = root.join("base");
        let extra = root.join("extra");
        let base_song = base.join("Pack").join("Song");
        let extra_song = extra.join("Pack").join("Song");
        fs::create_dir_all(&base_song).unwrap();
        fs::create_dir_all(&extra_song).unwrap();

        let found = resolve_song_dir(
            &[base.clone(), extra.clone()],
            &mut HashMap::new(),
            Some("pack"),
            "song",
        );
        assert_eq!(found, Some(extra_song.clone()));

        let group =
            resolve_course_group_dir(&[base.clone(), extra.clone()], &mut HashMap::new(), "pack");
        assert_eq!(group, Some(extra.join("Pack")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn collect_merged_course_paths_dedupes_relative_paths() {
        let root = test_dir("merged-paths");
        let base = root.join("base");
        let extra = root.join("extra");
        fs::create_dir_all(base.join("Pack")).unwrap();
        fs::create_dir_all(extra.join("Pack")).unwrap();
        fs::write(base.join("Pack").join("course.crs"), b"#COURSE:Base;").unwrap();
        fs::write(extra.join("Pack").join("course.crs"), b"#COURSE:Extra;").unwrap();
        fs::write(extra.join("Pack").join("other.crs"), b"#COURSE:Other;").unwrap();

        let paths = collect_merged_course_paths(&[base.clone(), extra.clone()]);

        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&base.join("Pack").join("course.crs")));
        assert!(paths.contains(&extra.join("Pack").join("other.crs")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn collect_course_scan_roots_reports_missing_primary_and_dedupes_extras() {
        let root = test_dir("course-roots");
        let primary = root.join("courses");
        let extra = root.join("extra");
        fs::create_dir_all(&extra).unwrap();

        let missing = collect_course_scan_roots(&primary, [extra.clone(), extra.clone()]);
        assert!(missing.primary_missing);
        assert_eq!(missing.roots, vec![extra.clone()]);

        fs::create_dir_all(&primary).unwrap();
        let present = collect_course_scan_roots(&primary, [extra.clone()]);
        assert!(!present.primary_missing);
        assert_eq!(present.roots, vec![primary, extra]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn course_progress_names_use_group_and_file_fallbacks() {
        let root = test_dir("course-progress");
        let course = root.join("Group").join("course.crs");

        assert_eq!(
            course_progress_names(&course, &root),
            ("Group", "course.crs")
        );
        assert_eq!(
            course_progress_names(Path::new("course.crs"), &root),
            (root.file_name().unwrap().to_str().unwrap(), "course.crs")
        );
        assert_eq!(
            course_progress_names(Path::new("course.crs"), Path::new("")),
            ("courses", "course.crs")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_course_paths_reports_progress_and_successes() {
        let root = test_dir("load-course-success");
        let course = root.join("Group").join("course.crs");
        fs::create_dir_all(course.parent().unwrap()).unwrap();
        fs::write(&course, b"#COURSE:Base;").unwrap();
        let mut progress = Vec::new();

        let report = load_course_paths_with_progress(
            vec![course.clone()],
            &root,
            &[],
            0,
            Some(&mut |done, total, group, item| {
                progress.push((done, total, group.to_string(), item.to_string()));
            }),
        );

        assert_eq!(report.failures, Vec::new());
        assert_eq!(report.courses.len(), 1);
        assert_eq!(report.courses[0].0, course);
        assert_eq!(report.courses[0].1.name, "Base");
        assert_eq!(
            progress,
            vec![
                (0, 1, String::new(), String::new()),
                (1, 1, "Group".to_string(), "course.crs".to_string()),
            ]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_course_paths_reports_parse_failures() {
        let root = test_dir("load-course-failure");
        let course = root.join("missing.crs");
        let mut progress = Vec::new();

        let report = load_course_paths_with_progress(
            vec![course.clone()],
            &root,
            &[],
            0,
            Some(&mut |done, total, group, item| {
                progress.push((done, total, group.to_string(), item.to_string()));
            }),
        );

        assert!(report.courses.is_empty());
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].path, course);
        assert!(report.failures[0].message.contains("Failed to read course"));
        assert_eq!(
            progress,
            vec![
                (0, 1, String::new(), String::new()),
                (
                    1,
                    1,
                    root.file_name().unwrap().to_string_lossy().to_string(),
                    "missing.crs".to_string()
                ),
            ]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_course_scan_merges_loaded_and_autogen_courses() {
        let root = test_dir("load-course-scan");
        let courses_root = root.join("courses");
        let course = courses_root.join("Group").join("course.crs");
        fs::create_dir_all(course.parent().unwrap()).unwrap();
        fs::write(&course, b"#COURSE:Base;").unwrap();
        let packs = vec![song_pack("Pack", "Display", 2)];
        let mut progress = Vec::new();

        let report = load_course_scan_with_progress(
            std::slice::from_ref(&courses_root),
            &courses_root,
            &[],
            &courses_root,
            &packs,
            Some(&mut |done, total, group, item| {
                progress.push((done, total, group.to_string(), item.to_string()));
            }),
        );

        assert_eq!(report.failures, Vec::new());
        assert_eq!(report.autogen_count, 1);
        assert_eq!(report.courses.len(), 2);
        assert_eq!(report.courses[0].0, course);
        assert_eq!(report.courses[0].1.name, "Base");
        assert_eq!(report.courses[1].1.name, "Display Random");
        assert_eq!(count_course_songs(&packs), 2);
        assert_eq!(
            progress,
            vec![
                (0, 1, String::new(), String::new()),
                (1, 1, "Group".to_string(), "course.crs".to_string()),
            ]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_course_chart_prefers_shifted_difficulty_then_playable_fallback() {
        let song = song_with_charts(
            "Pack/Song/song.ssc",
            vec![
                test_chart("Easy", 3, false, "easy"),
                test_chart("Hard", 9, true, "hard"),
                test_chart("Challenge", 12, true, "challenge"),
            ],
        );
        let entry = entry_with_steps(StepsSpec::Difficulty(Difficulty::Hard));

        let shifted = resolve_course_chart(&song, &entry, "dance-single", Difficulty::Challenge)
            .expect("shifted chart");
        assert_eq!(shifted.short_hash, "challenge");

        let fallback = resolve_course_chart(&song, &entry, "dance-double", Difficulty::Medium);
        assert!(fallback.is_none());

        let missing = entry_with_steps(StepsSpec::Difficulty(Difficulty::Medium));
        let fallback = resolve_course_chart(&song, &missing, "dance-single", Difficulty::Medium)
            .expect("playable fallback");
        assert_eq!(fallback.short_hash, "hard");
    }

    #[test]
    fn resolve_entry_song_uses_sort_picks_and_random_unused_candidates() {
        let course_path = PathBuf::from("Courses/course.crs");
        let slow = song_with_charts("Pack/Slow/song.ssc", vec![test_chart("Hard", 7, true, "s")]);
        let fast = song_with_charts("Pack/Fast/song.ssc", vec![test_chart("Hard", 9, true, "f")]);
        let all_songs = vec![slow.clone(), fast.clone()];
        let by_group_song = HashMap::new();
        let by_song = HashMap::new();
        let mut songs_by_group = HashMap::new();
        songs_by_group.insert("pack".to_string(), all_songs.clone());
        let song_play_counts =
            HashMap::from([(song_unique_key(&slow), 4), (song_unique_key(&fast), 12)]);

        let sort_entry = CourseEntry {
            song: CourseSong::SortPick {
                sort: SongSort::MostPlays,
                index: 0,
            },
            steps: StepsSpec::Difficulty(Difficulty::Hard),
            modifiers: String::new(),
            secret: false,
            no_difficult: false,
            gain_lives: -1,
        };
        let picked = resolve_entry_song(
            &course_path,
            0,
            0,
            &sort_entry,
            &by_group_song,
            &by_song,
            &all_songs,
            &songs_by_group,
            &song_play_counts,
            &HashSet::new(),
            "dance-single",
            Difficulty::Medium,
        )
        .expect("sort pick");
        assert_eq!(picked.simfile_path, fast.simfile_path);

        let random_entry = CourseEntry {
            song: CourseSong::RandomWithinGroup {
                group: "Pack".to_string(),
            },
            steps: StepsSpec::Difficulty(Difficulty::Hard),
            modifiers: String::new(),
            secret: false,
            no_difficult: false,
            gain_lives: -1,
        };
        let used = HashSet::from([song_unique_key(&fast)]);
        let random = resolve_entry_song(
            &course_path,
            0,
            0,
            &random_entry,
            &by_group_song,
            &by_song,
            &all_songs,
            &songs_by_group,
            &song_play_counts,
            &used,
            "dance-single",
            Difficulty::Medium,
        )
        .expect("random pick");
        assert_eq!(random.simfile_path, slow.simfile_path);
    }

    #[test]
    fn play_rank_selection_preserves_stable_ties_and_pick_direction() {
        let first = song_with_charts("Pack/First/song.ssc", Vec::new());
        let second = song_with_charts("Pack/Second/song.ssc", Vec::new());
        let third = song_with_charts("Pack/Third/song.ssc", Vec::new());
        let songs = vec![first, second.clone(), third.clone()];
        let ranked = vec![(9, 0), (9, 1), (3, 2)];

        let tied = select_song_by_play_rank(&songs, ranked.clone(), SongSort::MostPlays, 1)
            .expect("second stable tie");
        let fewest = select_song_by_play_rank(&songs, ranked, SongSort::FewestPlays, 0)
            .expect("fewest plays");

        assert!(Arc::ptr_eq(&tied, &second));
        assert!(Arc::ptr_eq(&fewest, &third));
        assert!(select_song_by_play_rank(&songs, vec![(9, 0)], SongSort::MostPlays, 1).is_none());
    }

    #[test]
    fn add_chart_totals_and_bpm_range_normalize_course_metadata() {
        let chart = test_chart("Hard", 9, true, "hard");
        let mut totals = CourseTotals::default();
        add_chart_totals(&mut totals, &chart);
        assert_eq!(
            totals,
            CourseTotals {
                steps: 9,
                jumps: 1,
                hands: 2,
                holds: 3,
                rolls: 4,
                mines: 5,
            }
        );

        let mut song = test_song();
        song.min_bpm = 0.0;
        song.max_bpm = 175.0;
        let mut min_bpm = None;
        let mut max_bpm = None;
        push_song_bpm_range(&mut min_bpm, &mut max_bpm, &song);
        assert_eq!((min_bpm, max_bpm), (Some(175.0), Some(175.0)));
    }

    #[test]
    fn validate_course_refs_rejects_missing_fixed_song() {
        let root = test_dir("missing-fixed-song");
        let songs = root.join("songs");
        fs::create_dir_all(songs.join("Pack")).unwrap();
        let course = fixed_course(Some("Pack"), "Missing");

        let error = validate_course_refs(&course, &[songs], &mut HashMap::new(), 1).unwrap_err();

        assert!(
            error
                .message
                .contains("entry 1 references missing song 'Pack/Missing'")
        );

        let _ = fs::remove_dir_all(root);
    }
}
