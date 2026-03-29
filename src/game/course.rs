use crate::game::{
    parsing::simfile::{collect_song_scan_roots, fmt_scan_time},
    song::get_song_cache,
};
use crate::config::dirs;
use log::{info, warn};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

pub type CourseData = (PathBuf, rssp::course::CourseFile);

static COURSE_CACHE: std::sync::LazyLock<Mutex<Vec<CourseData>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

pub fn get_course_cache() -> std::sync::MutexGuard<'static, Vec<CourseData>> {
    COURSE_CACHE.lock().unwrap()
}

fn set_course_cache(courses: Vec<CourseData>) {
    *COURSE_CACHE.lock().unwrap() = courses;
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

#[inline(always)]
fn course_progress_names<'a>(path: &'a Path, root: &'a Path) -> (&'a str, &'a str) {
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

fn resolve_song_dir(
    song_roots: &[PathBuf],
    group_dirs: &mut HashMap<String, PathBuf>,
    group: Option<&str>,
    song: &str,
) -> Option<PathBuf> {
    fn resolve_group_dir(
        song_roots: &[PathBuf],
        group_dirs: &mut HashMap<String, PathBuf>,
        group: &str,
    ) -> Option<PathBuf> {
        let key = group.trim().to_ascii_lowercase();
        if key.is_empty() {
            return None;
        }
        if !group_dirs.contains_key(&key) {
            let mut path = None;
            for songs_root in song_roots.iter().rev() {
                path = is_dir_ci(songs_root, group);
                if path.is_some() {
                    break;
                }
            }
            let path = path?;
            group_dirs.insert(key.clone(), path);
        }
        group_dirs.get(&key).cloned()
    }

    let song = song.trim();
    if song.is_empty() {
        return None;
    }

    if let Some(group) = group.map(str::trim).filter(|g| !g.is_empty()) {
        let group_dir = resolve_group_dir(song_roots, group_dirs, group)?;
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

fn resolve_course_group_dir(
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

fn autogen_nonstop_group_courses() -> Vec<(PathBuf, rssp::course::CourseFile)> {
    let song_cache = get_song_cache();
    let mut out = Vec::with_capacity(song_cache.len());

    for pack in song_cache.iter() {
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

        let mut entries = Vec::with_capacity(4);
        for _ in 0..4 {
            entries.push(rssp::course::CourseEntry {
                song: rssp::course::CourseSong::RandomWithinGroup {
                    group: group_name.to_string(),
                },
                steps: rssp::course::StepsSpec::Difficulty(rssp::course::Difficulty::Medium),
                modifiers: String::new(),
                secret: true,
                no_difficult: false,
                gain_lives: -1,
            });
        }

        let mut path = dirs::app_dirs().courses_dir();
        path.push(group_name);
        path.push("__deadsync_autogen_nonstop_random.crs");

        out.push((
            path,
            rssp::course::CourseFile {
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

pub fn scan_and_load_courses(courses_root: &Path, songs_root: &Path) {
    scan_and_load_courses_impl::<fn(usize, usize, &str, &str)>(
        courses_root,
        songs_root,
        None,
    );
}

pub fn scan_and_load_courses_with_progress<F>(
    courses_root: &Path,
    songs_root: &Path,
    progress: &mut F,
) where
    F: FnMut(&str, &str),
{
    let mut with_counts = |_: usize, _: usize, group: &str, course: &str| progress(group, course);
    scan_and_load_courses_impl(courses_root, songs_root, Some(&mut with_counts));
}

pub fn scan_and_load_courses_with_progress_counts<F>(
    courses_root: &Path,
    songs_root: &Path,
    progress: &mut F,
) where
    F: FnMut(usize, usize, &str, &str),
{
    scan_and_load_courses_impl(courses_root, songs_root, Some(progress));
}

fn scan_and_load_courses_impl<F>(
    courses_root: &Path,
    songs_root: &Path,
    mut progress: Option<&mut F>,
) where
    F: FnMut(usize, usize, &str, &str),
{
    info!("Starting course scan in '{}'...", courses_root.display());
    let started = Instant::now();

    if !courses_root.is_dir() {
        warn!("Courses directory '{}' not found. No courses will be loaded.", courses_root.display());
        set_course_cache(Vec::new());
        return;
    }

    let song_roots = collect_song_scan_roots(songs_root);
    if song_roots.is_empty() {
        warn!("No valid song roots found. No courses will be loaded.");
        set_course_cache(Vec::new());
        return;
    }

    let mut loaded_courses = Vec::new();
    let mut courses_failed = 0usize;
    let mut group_dirs: HashMap<String, PathBuf> = HashMap::new();
    let total_song_count = {
        let song_cache = get_song_cache();
        song_cache
            .iter()
            .map(|pack| pack.songs.len())
            .sum::<usize>()
    };
    let course_paths = collect_course_paths(courses_root);
    let total_courses = course_paths.len();
    let mut courses_done = 0usize;
    report_load_progress(&mut progress, 0, total_courses, "", "");

    for course_path in course_paths {
        let (group_display, course_display) = course_progress_names(&course_path, courses_root);
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
        let data = match fs::read(&course_path) {
            Ok(d) => d,
            Err(e) => {
                courses_failed += 1;
                warn!("Failed to read course '{}': {}", course_path.display(), e);
                report_done();
                continue;
            }
        };

        let course = match rssp::course::parse_crs(&data) {
            Ok(c) => c,
            Err(e) => {
                courses_failed += 1;
                warn!("Failed to parse course '{}': {}", course_path.display(), e);
                report_done();
                continue;
            }
        };

        let mut ok = true;
        for (idx, entry) in course.entries.iter().enumerate() {
            match &entry.song {
                rssp::course::CourseSong::Fixed { group, song } => {
                    let Some(song_dir) =
                        resolve_song_dir(&song_roots, &mut group_dirs, group.as_deref(), song)
                    else {
                        warn!(
                            "Course '{}' entry {} references missing song '{}{}'.",
                            course.name,
                            idx + 1,
                            group
                                .as_deref()
                                .map(|g| format!("{g}/"))
                                .unwrap_or_default(),
                            song
                        );
                        ok = false;
                        break;
                    };

                    match rssp::pack::scan_song_dir(&song_dir, rssp::pack::ScanOpt::default()) {
                        Ok(Some(_)) => {}
                        Ok(None) => {
                            warn!(
                                "Course '{}' entry {} song dir has no simfile: {}",
                                course.name,
                                idx + 1,
                                song_dir.display()
                            );
                            ok = false;
                            break;
                        }
                        Err(e) => {
                            warn!(
                                "Course '{}' entry {} failed scanning song dir {}: {e:?}",
                                course.name,
                                idx + 1,
                                song_dir.display()
                            );
                            ok = false;
                            break;
                        }
                    }
                }
                rssp::course::CourseSong::SortPick { sort, index } => {
                    let supports_sort = matches!(
                        sort,
                        rssp::course::SongSort::MostPlays | rssp::course::SongSort::FewestPlays
                    );
                    if !supports_sort {
                        warn!(
                            "Course '{}' has unsupported sort selector in entry {} ({sort:?}).",
                            course.name,
                            idx + 1,
                        );
                        ok = false;
                        break;
                    }

                    let choose_index = (*index).max(0) as usize;
                    if choose_index >= total_song_count {
                        let label = match sort {
                            rssp::course::SongSort::MostPlays => "BEST",
                            rssp::course::SongSort::FewestPlays => "WORST",
                            rssp::course::SongSort::TopGrades => "GRADEBEST",
                            rssp::course::SongSort::LowestGrades => "GRADEWORST",
                        };
                        warn!(
                            "Course '{}' entry {} references out-of-range sort pick '{}{}' with only {} songs installed.",
                            course.name,
                            idx + 1,
                            label,
                            choose_index.saturating_add(1),
                            total_song_count
                        );
                        ok = false;
                        break;
                    }
                }
                rssp::course::CourseSong::RandomAny => {}
                rssp::course::CourseSong::RandomWithinGroup { group } => {
                    if resolve_course_group_dir(&song_roots, &mut group_dirs, group).is_none() {
                        warn!(
                            "Course '{}' entry {} references missing group '{}/*'.",
                            course.name,
                            idx + 1,
                            group
                        );
                        ok = false;
                        break;
                    }
                }
                _ => {
                    warn!(
                        "Course '{}' has unsupported song selector in entry {}.",
                        course.name,
                        idx + 1
                    );
                    ok = false;
                    break;
                }
            }
        }

        if ok {
            loaded_courses.push((course_path, course));
        } else {
            courses_failed += 1;
        }
        report_done();
    }

    let autogen_courses = autogen_nonstop_group_courses();
    let autogen_count = autogen_courses.len();
    loaded_courses.extend(autogen_courses);

    info!(
        "Finished course scan. Loaded {} courses ({} autogen, failed {}) in {}.",
        loaded_courses.len(),
        autogen_count,
        courses_failed,
        fmt_scan_time(started.elapsed())
    );
    set_course_cache(loaded_courses);
}

#[cfg(test)]
mod tests {
    use super::{resolve_course_group_dir, resolve_song_dir};
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;

    fn test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("deadsync-course-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
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
}
