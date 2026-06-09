use crate::config::dirs;
use crate::game::{parsing::simfile::collect_song_scan_roots, song::get_song_cache};
use deadsync_simfile::course::{
    CourseFile, autogen_nonstop_group_courses, collect_merged_course_paths, parse_course_file,
    validate_course_refs,
};
use deadsync_simfile::scan::fmt_scan_time;
use log::{info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

pub type CourseData = (PathBuf, CourseFile);

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

fn collect_course_scan_roots(root_path: &Path) -> Vec<PathBuf> {
    fn push_unique_root(path: PathBuf, roots: &mut Vec<PathBuf>, keys: &mut Vec<String>) {
        let mut key = path.to_string_lossy().into_owned();
        if cfg!(windows) {
            key.make_ascii_lowercase();
        }
        if keys.iter().any(|existing| existing == &key) {
            return;
        }
        keys.push(key);
        roots.push(path);
    }

    let mut roots = Vec::with_capacity(2);
    let mut keys = Vec::with_capacity(2);
    if root_path.is_dir() {
        push_unique_root(root_path.to_path_buf(), &mut roots, &mut keys);
    } else {
        warn!("Courses directory '{}' not found.", root_path.display());
    }

    for extra in dirs::app_dirs().extra_course_roots() {
        push_unique_root(extra, &mut roots, &mut keys);
    }

    roots
}

pub fn scan_and_load_courses(courses_root: &Path, songs_root: &Path) {
    scan_and_load_courses_impl::<fn(usize, usize, &str, &str)>(courses_root, songs_root, None);
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

    let song_roots = collect_song_scan_roots(songs_root);
    if song_roots.is_empty() {
        warn!("No valid song roots found. No courses will be loaded.");
        set_course_cache(Vec::new());
        return;
    }
    let course_roots = collect_course_scan_roots(courses_root);
    if course_roots.is_empty() {
        warn!("No valid course roots found. No courses will be loaded.");
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
    let course_paths = collect_merged_course_paths(&course_roots);
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
        let course = match parse_course_file(&course_path) {
            Ok(c) => c,
            Err(error) => {
                courses_failed += 1;
                warn!("{error}");
                report_done();
                continue;
            }
        };

        if let Err(error) =
            validate_course_refs(&course, &song_roots, &mut group_dirs, total_song_count)
        {
            warn!("{error}");
            courses_failed += 1;
        } else {
            loaded_courses.push((course_path, course));
        }
        report_done();
    }

    let autogen_courses = {
        let courses_dir = dirs::app_dirs().courses_dir();
        let song_cache = get_song_cache();
        autogen_nonstop_group_courses(&courses_dir, &song_cache)
    };
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
