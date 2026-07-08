use crate::scan::push_unique_path;
use deadsync_chart::SongPack;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub use rssp::course::{
    CourseEntry, CourseFile, CourseSong, Difficulty, SongSort, StepsSpec, difficulty_label,
    resolve_course_banner_path,
};

pub type LoadedCourse = (PathBuf, CourseFile);

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
    use deadsync_chart::{SongData, SyncPref};
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
