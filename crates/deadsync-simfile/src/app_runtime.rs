use crate::cache::{
    GameplayChartLoadLogEntry, GameplayChartLoadLogLevel, GameplayChartLoadReport,
    RuntimeSongLoadLogEntry, RuntimeSongLoadLogLevel,
};
use crate::course::runtime_course_scan_log_entry;
use crate::media::{
    BG_ANIMATIONS_DIR, RANDOM_MOVIES_DIR, SONG_MOVIES_DIR, collect_media_roots,
    random_movie_paths_for_song,
};
use crate::runtime::{
    RuntimeSongConfig, gameplay_chart_load_log_entries_from_report, load_gameplay_charts_runtime,
    load_song_for_scan_runtime, load_sync_analysis_chart_runtime, reload_song_in_cache_runtime,
};
use crate::scan::{
    RuntimeCourseScanEnv, RuntimeScanAdapterEvent, RuntimeScanLogEntry, RuntimeScanLogLevel,
    RuntimeSongScanEnv, SongLoadOptions, SongScanRootEvent,
    reload_song_dirs_with_progress_counts_runtime, runtime_collect_song_scan_roots,
    runtime_song_scan_log_entry, scan_and_load_courses_with_progress_counts_runtime,
    scan_and_load_songs_with_progress_counts_runtime,
};
use crate::song::ParseSongOptions;
use deadlib_platform::dirs;
use deadsync_audio_decode as decode;
use deadsync_chart::{GameplayChartData, SongData};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn scan_and_load_songs_with_progress_counts<F>(root_path: &Path, progress: &mut F)
where
    F: FnMut(usize, usize, &str, &str),
{
    scan_and_load_songs_with_progress_counts_runtime(
        song_scan_env(root_path),
        progress,
        load_song_for_scan,
        deadsync_config::runtime::group_is_never_cached,
        emit_scan_adapter_log,
    );
}

pub fn reload_song_dirs_with_progress_counts<F>(
    root_path: &Path,
    dirs: &[PathBuf],
    progress: &mut F,
) where
    F: FnMut(usize, usize, &str, &str),
{
    reload_song_dirs_with_progress_counts_runtime(
        song_scan_env(root_path),
        dirs,
        progress,
        load_song_for_scan,
        deadsync_config::runtime::group_is_never_cached,
        emit_scan_adapter_log,
    );
}

pub fn scan_and_load_courses_with_progress_counts<F>(
    courses_root: &Path,
    songs_root: &Path,
    progress: &mut F,
) where
    F: FnMut(usize, usize, &str, &str),
{
    scan_and_load_courses_with_progress_counts_runtime(
        course_scan_env(courses_root, songs_root),
        progress,
        emit_scan_adapter_log,
    );
}

pub fn collect_song_scan_roots(root_path: &Path) -> Vec<PathBuf> {
    runtime_collect_song_scan_roots(&song_scan_env(root_path), emit_scan_adapter_log)
}

/// Re-parse one simfile and replace its in-memory song-cache entry.
///
/// This is used after writing sync edits to disk so immediate replays use the
/// updated timing without a full songs rescan.
pub fn reload_song_in_cache(simfile_path: &Path) -> Result<Arc<SongData>, String> {
    let cache_dir = dirs::app_dirs().song_cache_dir();
    let parse_options = parse_song_options();
    reload_song_in_cache_runtime(
        simfile_path,
        &cache_dir,
        &parse_options,
        runtime_song_config(),
        deadsync_config::runtime::group_is_never_cached,
        compute_music_length_seconds,
        emit_runtime_song_load_log,
    )
}

fn song_scan_env(root_path: &Path) -> RuntimeSongScanEnv {
    let config = deadsync_config::runtime::get();
    RuntimeSongScanEnv {
        base_root: root_path.to_path_buf(),
        extra_song_roots: dirs::app_dirs().extra_song_roots(),
        additional_song_roots: additional_song_roots(),
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

fn course_scan_env(courses_root: &Path, songs_root: &Path) -> RuntimeCourseScanEnv {
    RuntimeCourseScanEnv {
        courses_root: courses_root.to_path_buf(),
        songs_root: songs_root.to_path_buf(),
        extra_course_roots: dirs::app_dirs().extra_course_roots(),
        extra_song_roots: dirs::app_dirs().extra_song_roots(),
        additional_song_roots: additional_song_roots(),
        autogen_courses_root: dirs::app_dirs().courses_dir(),
    }
}

fn additional_song_roots() -> Vec<(String, PathBuf)> {
    deadsync_config::runtime::additional_song_folder_roots()
        .into_iter()
        .map(|folder| {
            let path = PathBuf::from(folder.path.as_str());
            (folder.path, path)
        })
        .collect()
}

fn emit_scan_adapter_log(event: RuntimeScanAdapterEvent) {
    match event {
        RuntimeScanAdapterEvent::SongRoot(root_event) => emit_song_root_log(root_event),
        RuntimeScanAdapterEvent::CourseRootMissing { root } => {
            warn!("Courses directory '{}' not found.", root.display());
        }
        RuntimeScanAdapterEvent::Song(song_event) => {
            emit_scan_log(runtime_song_scan_log_entry(song_event));
        }
        RuntimeScanAdapterEvent::Course(course_event) => {
            emit_scan_log(runtime_course_scan_log_entry(course_event));
        }
    }
}

fn emit_song_root_log(event: SongScanRootEvent) {
    match event {
        SongScanRootEvent::PrimaryMissing { root } => {
            warn!("Songs directory '{}' not found.", root.display());
        }
        SongScanRootEvent::AdditionalMissing { label, .. } => {
            warn!("AdditionalSongFolders entry '{label}' is not a directory; skipping.");
        }
    }
}

fn emit_scan_log(entry: RuntimeScanLogEntry) {
    match entry.level {
        RuntimeScanLogLevel::Debug => debug!("{}", entry.message),
        RuntimeScanLogLevel::Info => info!("{}", entry.message),
        RuntimeScanLogLevel::Warn => warn!("{}", entry.message),
    }
}

pub fn load_gameplay_charts(
    song: &SongData,
    requested_chart_ixs: &[usize],
    global_offset_seconds: f32,
) -> Result<Vec<GameplayChartData>, String> {
    let cache_dir = dirs::app_dirs().song_cache_dir();
    let parse_options = parse_song_options();
    let result = load_gameplay_charts_runtime(
        song,
        requested_chart_ixs,
        &cache_dir,
        &parse_options,
        RuntimeSongConfig {
            global_offset_seconds,
            ..runtime_song_config()
        },
        deadsync_config::runtime::group_is_never_cached,
        compute_music_length_seconds,
    )?;
    log_gameplay_chart_load(song, &result.report);
    Ok(result.charts)
}

pub fn load_sync_analysis_chart(
    song: &SongData,
    chart_ix: usize,
) -> Result<GameplayChartData, String> {
    let cache_dir = dirs::app_dirs().song_cache_dir();
    let parse_options = parse_song_options();
    let mut result = load_sync_analysis_chart_runtime(
        song,
        chart_ix,
        &cache_dir,
        &parse_options,
        runtime_song_config(),
        deadsync_config::runtime::group_is_never_cached,
        compute_music_length_seconds,
    )?;
    log_gameplay_chart_load(song, &result.report);
    result
        .charts
        .pop()
        .ok_or_else(|| format!("Chart index {chart_ix} out of range"))
}

fn log_gameplay_chart_load(song: &SongData, report: &GameplayChartLoadReport) {
    for entry in gameplay_chart_load_log_entries_from_report(song, report) {
        emit_gameplay_chart_load_log(entry);
    }
}

fn emit_gameplay_chart_load_log(entry: GameplayChartLoadLogEntry) {
    match entry.level {
        GameplayChartLoadLogLevel::Debug => debug!("{}", entry.message),
        GameplayChartLoadLogLevel::Info => info!("{}", entry.message),
        GameplayChartLoadLogLevel::Warn => warn!("{}", entry.message),
    }
}

fn load_song_for_scan(
    simfile_path: PathBuf,
    fastload: bool,
    cachesongs: bool,
    global_offset_seconds: f32,
) -> Result<(SongData, bool), String> {
    let cache_dir = dirs::app_dirs().song_cache_dir();
    let parse_options = parse_song_options();
    let config = RuntimeSongConfig {
        fastload,
        cachesongs,
        global_offset_seconds,
    };
    let (song, cache_hit, log_entries) = load_song_for_scan_runtime(
        simfile_path,
        &cache_dir,
        &parse_options,
        config,
        compute_music_length_seconds,
    )?;
    for entry in log_entries {
        emit_runtime_song_load_log(entry);
    }
    Ok((song, cache_hit))
}

fn emit_runtime_song_load_log(entry: RuntimeSongLoadLogEntry) {
    match entry.level {
        RuntimeSongLoadLogLevel::Debug => debug!("{}", entry.message),
        RuntimeSongLoadLogLevel::Warn => warn!("{}", entry.message),
    }
}

pub fn parse_song_for_test(path: &Path, global_offset_seconds: f32) -> Result<SongData, String> {
    crate::song::parse_song_meta_file(
        path,
        &parse_song_options(),
        global_offset_seconds,
        compute_music_length_seconds,
    )
}

fn bgchange_asset_roots(dirname: &str) -> Vec<PathBuf> {
    let dirs = dirs::app_dirs();
    let cwd = std::env::current_dir().ok();
    collect_media_roots(dirname, &dirs.data_dir, &dirs.exe_dir, cwd.as_deref())
}

pub fn random_movie_paths(song: &SongData, random_movies: bool) -> Vec<PathBuf> {
    if !random_movies {
        return Vec::new();
    }
    random_movie_paths_for_song(song, &bgchange_asset_roots(RANDOM_MOVIES_DIR))
}

fn parse_song_options() -> ParseSongOptions {
    ParseSongOptions::new(
        bgchange_asset_roots(SONG_MOVIES_DIR),
        bgchange_asset_roots(RANDOM_MOVIES_DIR),
        bgchange_asset_roots(BG_ANIMATIONS_DIR),
    )
}

fn runtime_song_config() -> RuntimeSongConfig {
    let config = deadsync_config::runtime::get();
    RuntimeSongConfig {
        fastload: config.fastload,
        cachesongs: config.cachesongs,
        global_offset_seconds: config.global_offset_seconds,
    }
}

/// Computes the length of the music file in seconds when the decode layer supports it.
/// Returns 0.0 on failure or if no music path is provided.
fn compute_music_length_seconds(music_path: Option<&Path>) -> f32 {
    let Some(path) = music_path else {
        return 0.0;
    };
    match decode::file_length_seconds(path) {
        Ok(sec) => sec,
        Err(e) => {
            warn!("Failed to compute audio length for {path:?}: {e}");
            0.0
        }
    }
}
