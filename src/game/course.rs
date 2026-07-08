use crate::game::{parsing::simfile::collect_song_scan_roots, song::get_song_cache};
use deadlib_platform::dirs;
use deadsync_simfile::course::{
    collect_course_scan_roots as simfile_collect_course_scan_roots, load_course_scan_with_progress,
};
use deadsync_simfile::runtime_cache;
use deadsync_simfile::scan::fmt_scan_time;
use log::{info, warn};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub type CourseData = runtime_cache::CourseData;

pub fn get_course_cache() -> std::sync::MutexGuard<'static, Vec<CourseData>> {
    runtime_cache::get_course_cache()
}

fn set_course_cache(courses: Vec<CourseData>) {
    runtime_cache::set_course_cache(courses);
}

fn collect_course_scan_roots(root_path: &Path) -> Vec<PathBuf> {
    let report =
        simfile_collect_course_scan_roots(root_path, dirs::app_dirs().extra_course_roots());
    if report.primary_missing {
        warn!("Courses directory '{}' not found.", root_path.display());
    }
    report.roots
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

fn scan_and_load_courses_impl<F>(courses_root: &Path, songs_root: &Path, progress: Option<&mut F>)
where
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

    let report = {
        let courses_dir = dirs::app_dirs().courses_dir();
        let song_cache = get_song_cache();
        load_course_scan_with_progress(
            &course_roots,
            courses_root,
            &song_roots,
            &courses_dir,
            &song_cache,
            progress,
        )
    };
    for failure in &report.failures {
        warn!("{}", failure.message);
    }

    info!(
        "Finished course scan. Loaded {} courses ({} autogen, failed {}) in {}.",
        report.courses.len(),
        report.autogen_count,
        report.failures.len(),
        fmt_scan_time(started.elapsed())
    );
    set_course_cache(report.courses);
}
