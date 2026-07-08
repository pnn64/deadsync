use super::load_song_for_scan;
use deadlib_platform::dirs;
use deadsync_chart::SongData;
use deadsync_simfile::course::{
    RuntimeCourseScanEvent, RuntimeCourseScanInput,
    collect_course_scan_roots as simfile_collect_course_scan_roots, runtime_course_scan_log_entry,
    scan_and_load_courses_runtime,
};
use deadsync_simfile::scan::{
    RuntimeScanLogEntry, RuntimeScanLogLevel, RuntimeSongScanEvent, RuntimeSongScanInput,
    SongLoadOptions, SongScanRootEvent, collect_song_scan_roots as simfile_collect_song_scan_roots,
    reload_song_dirs_runtime, runtime_song_scan_log_entry, scan_and_load_songs_runtime,
};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};

pub fn scan_and_load_songs_with_progress_counts<F>(root_path: &Path, progress: &mut F)
where
    F: FnMut(usize, usize, &str, &str),
{
    scan_and_load_songs_impl(root_path, Some(progress));
}

pub fn reload_song_dirs_with_progress_counts<F>(
    root_path: &Path,
    dirs: &[PathBuf],
    progress: &mut F,
) where
    F: FnMut(usize, usize, &str, &str),
{
    reload_song_dirs_impl(root_path, dirs, Some(progress));
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

pub(crate) fn collect_song_scan_roots(root_path: &Path) -> Vec<PathBuf> {
    let additional_roots = crate::config::additional_song_folder_roots()
        .into_iter()
        .map(|folder| {
            let path = PathBuf::from(folder.path.as_str());
            (folder.path, path)
        });
    let report = simfile_collect_song_scan_roots(
        root_path,
        dirs::app_dirs().extra_song_roots(),
        additional_roots,
    );

    for event in report.events {
        match event {
            SongScanRootEvent::PrimaryMissing { root } => {
                warn!("Songs directory '{}' not found.", root.display());
            }
            SongScanRootEvent::AdditionalMissing { label, .. } => {
                warn!("AdditionalSongFolders entry '{label}' is not a directory; skipping.");
            }
        }
    }

    report.roots
}

fn collect_course_scan_roots(root_path: &Path) -> Vec<PathBuf> {
    let report =
        simfile_collect_course_scan_roots(root_path, dirs::app_dirs().extra_course_roots());
    if report.primary_missing {
        warn!("Courses directory '{}' not found.", root_path.display());
    }
    report.roots
}

fn process_song(
    simfile_path: PathBuf,
    fastload: bool,
    cachesongs: bool,
    global_offset_seconds: f32,
) -> Result<(SongData, bool), String> {
    load_song_for_scan(simfile_path, fastload, cachesongs, global_offset_seconds)
}

fn song_scan_input(root_path: &Path) -> RuntimeSongScanInput {
    let config = crate::config::get();
    RuntimeSongScanInput {
        base_root: root_path.to_path_buf(),
        song_roots: collect_song_scan_roots(root_path),
        cache_dir: dirs::app_dirs().song_cache_dir(),
        load_options: SongLoadOptions {
            fastload: config.fastload,
            cachesongs: config.cachesongs,
            global_offset_seconds: config.global_offset_seconds,
            song_parsing_threads: u32::from(config.song_parsing_threads),
        },
        requested_threads: config.song_parsing_threads,
    }
}

fn log_song_scan_event(event: RuntimeSongScanEvent) {
    emit_scan_log(runtime_song_scan_log_entry(event));
}

fn course_scan_input(courses_root: &Path, songs_root: &Path) -> RuntimeCourseScanInput {
    RuntimeCourseScanInput {
        courses_root: courses_root.to_path_buf(),
        course_roots: collect_course_scan_roots(courses_root),
        song_roots: collect_song_scan_roots(songs_root),
        autogen_courses_root: dirs::app_dirs().courses_dir(),
    }
}

fn log_course_scan_event(event: RuntimeCourseScanEvent) {
    emit_scan_log(runtime_course_scan_log_entry(event));
}

fn emit_scan_log(entry: RuntimeScanLogEntry) {
    match entry.level {
        RuntimeScanLogLevel::Debug => debug!("{}", entry.message),
        RuntimeScanLogLevel::Info => info!("{}", entry.message),
        RuntimeScanLogLevel::Warn => warn!("{}", entry.message),
    }
}

fn scan_and_load_songs_impl<F>(root_path: &Path, progress: Option<&mut F>)
where
    F: FnMut(usize, usize, &str, &str),
{
    scan_and_load_songs_runtime(
        song_scan_input(root_path),
        progress,
        process_song,
        crate::config::group_is_never_cached,
        log_song_scan_event,
    );
}

fn scan_and_load_courses_impl<F>(courses_root: &Path, songs_root: &Path, progress: Option<&mut F>)
where
    F: FnMut(usize, usize, &str, &str),
{
    scan_and_load_courses_runtime(
        course_scan_input(courses_root, songs_root),
        progress,
        log_course_scan_event,
    );
}

fn reload_song_dirs_impl<F>(root_path: &Path, pack_dirs: &[PathBuf], progress: Option<&mut F>)
where
    F: FnMut(usize, usize, &str, &str),
{
    reload_song_dirs_runtime(
        song_scan_input(root_path),
        pack_dirs,
        progress,
        process_song,
        crate::config::group_is_never_cached,
        log_song_scan_event,
    );
}
