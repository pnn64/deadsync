use crate::cache::{
    GameplayChartLoadLogEntry, GameplayChartLoadLogLevel, GameplayChartLoadReport,
    RuntimeSongLoadLogEntry, RuntimeSongLoadLogLevel,
};
use crate::course::runtime_course_scan_log_entry;
use crate::media::random_movie_paths_for_song;
use crate::runtime::{
    RuntimeSongConfig, gameplay_chart_load_log_entries_from_report, load_gameplay_charts_runtime,
    load_song_for_scan_runtime_in, load_sync_analysis_chart_runtime, reload_song_in_cache_runtime,
};
use crate::scan::{
    RuntimeCourseScanEnv, RuntimeScanAdapterEvent, RuntimeScanLogEntry, RuntimeScanLogLevel,
    RuntimeSongScanEnv, SongLoadOptions, SongScanMode, SongScanRootEvent,
    reload_song_dirs_with_progress_counts_runtime, runtime_song_scan_log_entry,
    scan_and_load_courses_with_progress_counts_runtime,
    scan_and_load_songs_with_progress_counts_runtime,
};
use crate::song::{ParseSongOptions, SongAnalyzer, SongParseScratch};
use deadsync_chart::{
    GameplayChartData, SongBackgroundChange, SongData, background::expand_random_background_changes,
};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Session paths supplied by startup before scan workers are launched.
pub struct ScanPaths {
    pub song_cache: PathBuf,
    pub extra_songs: Vec<PathBuf>,
    pub extra_courses: Vec<PathBuf>,
    pub autogen_courses: PathBuf,
    pub song_movies: Vec<PathBuf>,
    pub random_movies: Vec<PathBuf>,
    pub bg_animations: Vec<PathBuf>,
}

struct ScanRuntime {
    paths: ScanPaths,
    probe_music_length: fn(&Path) -> Result<f32, String>,
}

static RUNTIME: std::sync::OnceLock<ScanRuntime> = std::sync::OnceLock::new();

/// Install scan paths and the host's music duration probe before loading charts.
pub fn init_paths(
    paths: ScanPaths,
    probe_music_length: fn(&Path) -> Result<f32, String>,
) -> Result<(), &'static str> {
    RUNTIME
        .set(ScanRuntime {
            paths,
            probe_music_length,
        })
        .map_err(|_| "scan paths already initialized")
}

fn runtime() -> &'static ScanRuntime {
    RUNTIME.get().expect("scan paths initialized at startup")
}

fn paths() -> &'static ScanPaths {
    &runtime().paths
}

pub fn scan_and_load_songs_with_progress_counts<F>(
    root_path: &Path,
    additional_song_roots: Vec<(String, PathBuf)>,
    load_options: SongLoadOptions,
    mode: SongScanMode,
    progress: &mut F,
    group_is_never_cached: impl Fn(&str) -> bool,
) where
    F: FnMut(usize, usize, &str, &str),
{
    let env = song_scan_env(root_path, additional_song_roots, load_options);
    let cache_dir = env.cache_dir.clone();
    let parse_options = parse_song_options();
    let analyzer = SongAnalyzer::new(&parse_options);
    let capture_debug_logs = log::log_enabled!(log::Level::Debug);
    scan_and_load_songs_with_progress_counts_runtime(
        env,
        mode,
        progress,
        SongParseScratch::default,
        move |scratch, path, fastload, cachesongs, offset| {
            let config = RuntimeSongConfig {
                fastload,
                cachesongs,
                global_offset_seconds: offset,
                capture_debug_logs,
            };
            load_song_for_scan(path, config, &cache_dir, &parse_options, &analyzer, scratch)
        },
        group_is_never_cached,
        emit_scan_adapter_log,
    );
}

pub fn reload_song_dirs_with_progress_counts<F>(
    root_path: &Path,
    additional_song_roots: Vec<(String, PathBuf)>,
    load_options: SongLoadOptions,
    dirs: &[PathBuf],
    progress: &mut F,
    group_is_never_cached: impl Fn(&str) -> bool,
) where
    F: FnMut(usize, usize, &str, &str),
{
    let env = song_scan_env(root_path, additional_song_roots, load_options);
    let cache_dir = env.cache_dir.clone();
    let parse_options = parse_song_options();
    let analyzer = SongAnalyzer::new(&parse_options);
    let capture_debug_logs = log::log_enabled!(log::Level::Debug);
    reload_song_dirs_with_progress_counts_runtime(
        env,
        dirs,
        progress,
        SongParseScratch::default,
        move |scratch, path, fastload, cachesongs, offset| {
            let config = RuntimeSongConfig {
                fastload,
                cachesongs,
                global_offset_seconds: offset,
                capture_debug_logs,
            };
            load_song_for_scan(path, config, &cache_dir, &parse_options, &analyzer, scratch)
        },
        group_is_never_cached,
        emit_scan_adapter_log,
    );
}

pub fn scan_and_load_courses_with_progress_counts<F>(
    courses_root: &Path,
    songs_root: &Path,
    additional_song_roots: Vec<(String, PathBuf)>,
    progress: &mut F,
) where
    F: FnMut(usize, usize, &str, &str),
{
    scan_and_load_courses_with_progress_counts_runtime(
        course_scan_env(courses_root, songs_root, additional_song_roots),
        progress,
        emit_scan_adapter_log,
    );
}

pub fn collect_song_scan_roots(
    root_path: &Path,
    additional_song_roots: Vec<(String, PathBuf)>,
) -> Vec<PathBuf> {
    let report = crate::scan::collect_song_scan_roots(
        root_path,
        existing_roots(&paths().extra_songs),
        additional_song_roots,
    );
    for event in report.events {
        emit_song_root_log(event);
    }
    report.roots
}

/// Re-parse one simfile and replace its in-memory song-cache entry.
///
/// This is used after writing sync edits to disk so immediate replays use the
/// updated timing without a full songs rescan.
pub fn reload_song_in_cache(
    simfile_path: &Path,
    config: RuntimeSongConfig,
    group_is_never_cached: impl FnMut(&str) -> bool,
) -> Result<Arc<SongData>, String> {
    let cache_dir = &paths().song_cache;
    let parse_options = parse_song_options();
    reload_song_in_cache_runtime(
        simfile_path,
        cache_dir,
        &parse_options,
        config,
        group_is_never_cached,
        compute_music_length_seconds,
        emit_runtime_song_load_log,
    )
}

fn existing_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots.iter().filter(|path| path.is_dir()).cloned().collect()
}

fn song_scan_env(
    root_path: &Path,
    additional_song_roots: Vec<(String, PathBuf)>,
    load_options: SongLoadOptions,
) -> RuntimeSongScanEnv {
    RuntimeSongScanEnv {
        base_root: root_path.to_path_buf(),
        extra_song_roots: existing_roots(&paths().extra_songs),
        additional_song_roots,
        cache_dir: paths().song_cache.clone(),
        load_options,
        requested_threads: load_options.song_parsing_threads,
    }
}

fn course_scan_env(
    courses_root: &Path,
    songs_root: &Path,
    additional_song_roots: Vec<(String, PathBuf)>,
) -> RuntimeCourseScanEnv {
    RuntimeCourseScanEnv {
        courses_root: courses_root.to_path_buf(),
        songs_root: songs_root.to_path_buf(),
        extra_course_roots: existing_roots(&paths().extra_courses),
        extra_song_roots: existing_roots(&paths().extra_songs),
        additional_song_roots,
        autogen_courses_root: paths().autogen_courses.clone(),
    }
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
    config: RuntimeSongConfig,
    group_is_never_cached: impl FnMut(&str) -> bool,
) -> Result<Vec<GameplayChartData>, String> {
    let cache_dir = &paths().song_cache;
    let parse_options = parse_song_options();
    let result = load_gameplay_charts_runtime(
        song,
        requested_chart_ixs,
        cache_dir,
        &parse_options,
        config,
        group_is_never_cached,
        compute_music_length_seconds,
    )?;
    log_gameplay_chart_load(song, &result.report);
    Ok(result.charts)
}

pub fn load_sync_analysis_chart(
    song: &SongData,
    chart_ix: usize,
) -> Result<GameplayChartData, String> {
    let cache_dir = &paths().song_cache;
    let parse_options = parse_song_options();
    let mut result = load_sync_analysis_chart_runtime(
        song,
        chart_ix,
        cache_dir,
        &parse_options,
        compute_music_length_seconds,
    )?;
    log_gameplay_chart_load(song, &result.report);
    result
        .charts
        .pop()
        .ok_or_else(|| format!("Chart index {chart_ix} out of range"))
}

/// Called during song preparation, before the gameplay Lua VM is compiled.
pub fn load_chart_timings(
    song: &SongData,
    config: RuntimeSongConfig,
    group_is_never_cached: impl FnMut(&str) -> bool,
) -> Result<Vec<deadsync_rules::timing::TimingData>, String> {
    Ok(
        load_chart_timing_and_notes(song, config, group_is_never_cached)?
            .into_iter()
            .map(|(timing, _, _)| timing)
            .collect(),
    )
}

pub fn load_chart_timing_and_notes(
    song: &SongData,
    config: RuntimeSongConfig,
    group_is_never_cached: impl FnMut(&str) -> bool,
) -> Result<Vec<(deadsync_rules::timing::TimingData, Vec<u8>, Vec<f32>)>, String> {
    let parse_options = parse_song_options();
    let options = crate::runtime::gameplay_chart_load_options(
        song,
        &paths().song_cache,
        &parse_options,
        config,
        group_is_never_cached,
    );
    let started = std::time::Instant::now();
    let timings =
        crate::cache::load_chart_timing_and_notes(song, &options, |_| song.music_length_seconds)?;
    debug!(
        "Song Lua chart timing: song='{}' charts={} elapsed_ms={:.3}",
        song.title,
        timings.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    Ok(timings)
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
    config: RuntimeSongConfig,
    cache_dir: &Path,
    parse_options: &ParseSongOptions,
    analyzer: &SongAnalyzer,
    scratch: &mut SongParseScratch,
) -> Result<(SongData, bool), String> {
    let (song, cache_hit, log_entries) = load_song_for_scan_runtime_in(
        simfile_path,
        cache_dir,
        parse_options,
        analyzer,
        scratch,
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

#[must_use]
pub fn random_movie_paths(song: &SongData, random_movies: bool) -> Vec<PathBuf> {
    if !random_movies {
        return Vec::new();
    }
    random_movie_paths_for_song(song, &paths().random_movies)
}

#[must_use]
pub fn gameplay_background_changes(
    song: &SongData,
    gameplay_chart: &GameplayChartData,
    random_movie_paths: Vec<PathBuf>,
) -> Vec<SongBackgroundChange> {
    if random_movie_paths.is_empty() {
        return song.background_changes.clone();
    }
    let seed_text = song
        .simfile_path
        .parent()
        .map(|path| path.to_string_lossy())
        .unwrap_or_else(|| song.simfile_path.to_string_lossy());
    expand_random_background_changes(
        song,
        &gameplay_chart.timing,
        &gameplay_chart.timing_segments,
        random_movie_paths,
        seed_text.as_ref(),
    )
}

fn parse_song_options() -> ParseSongOptions {
    ParseSongOptions::new(
        paths().song_movies.clone(),
        paths().random_movies.clone(),
        paths().bg_animations.clone(),
    )
}

/// Probes the music file length in seconds through the host.
/// Returns 0.0 on failure or if no music path is provided.
fn compute_music_length_seconds(music_path: Option<&Path>) -> f32 {
    let Some(path) = music_path else {
        return 0.0;
    };
    match (runtime().probe_music_length)(path) {
        Ok(sec) => sec,
        Err(e) => {
            warn!("Failed to compute audio length for {path:?}: {e}");
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn chart_load_uses_the_supplied_cache_policy_and_timing_offset() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("deadsync-load-{}-{nanos}", std::process::id()));
        let simfile = root.join("Songs/Test Pack/Test Song/chart.sm");
        fs::create_dir_all(simfile.parent().unwrap()).unwrap();
        let source = "#TITLE:Chart;#MUSIC:track.ogg;#OFFSET:0;#BPMS:0=120;#NOTES:dance-single::Hard:9:0,0,0,0,0:\n1000\n0100\n0010\n0001\n;";
        fs::write(&simfile, source).unwrap();
        fs::write(simfile.with_file_name("track.ogg"), []).unwrap();
        let cache_dir = root.join("cache");
        init_paths(
            ScanPaths {
                song_cache: cache_dir.clone(),
                extra_songs: Vec::new(),
                extra_courses: Vec::new(),
                autogen_courses: root.join("Courses"),
                song_movies: Vec::new(),
                random_movies: Vec::new(),
                bg_animations: Vec::new(),
            },
            |path| {
                if path.ends_with("track.ogg") {
                    Ok(12.5)
                } else {
                    Err("unknown test track".into())
                }
            },
        )
        .unwrap();
        let song = parse_song_for_test(&simfile, 0.0).unwrap();
        assert_eq!(song.music_length_seconds, 12.5);
        assert_eq!(compute_music_length_seconds(Some(&simfile)), 0.0);
        assert_eq!(compute_music_length_seconds(None), 0.0);
        let cache_path = crate::cache::song_cache_path(&cache_dir, &simfile).unwrap();
        let config = RuntimeSongConfig::new(true, true, 0.25);

        let uncached =
            load_gameplay_charts(&song, &[0], config, |pack| pack == "Test Pack").unwrap();
        assert!(!cache_path.exists());
        assert_eq!(uncached[0].timing.get_time_for_beat(0.0), -0.25);

        load_gameplay_charts(&song, &[0], config, |_| false).unwrap();
        assert!(cache_path.is_file());
        fs::write(&simfile, source.replace("0=120", "0=240")).unwrap();

        let timings =
            load_chart_timings(&song, RuntimeSongConfig::new(true, true, 0.5), |_| false).unwrap();
        assert_eq!(timings[0].get_time_for_beat(0.0), -0.5);
        assert_eq!(timings[0].get_time_for_beat(1.0), 0.0);

        let analysis = load_sync_analysis_chart(&song, 0).unwrap();
        assert_eq!(analysis.timing.get_time_for_beat(0.0), 0.0);
        assert_eq!(analysis.timing.get_time_for_beat(1.0), 0.25);
        fs::remove_dir_all(root).unwrap();
    }
}
