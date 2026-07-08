use crate::config::{self};
use deadlib_platform::dirs;
use deadsync_audio_decode as decode;
use deadsync_chart::{GameplayChartData, SongData};
use deadsync_simfile::cache::{
    GameplayChartLoadLogEntry, GameplayChartLoadLogLevel, GameplayChartLoadOptions,
    GameplayChartLoadReport, RuntimeSongLoadLogEntry, RuntimeSongLoadLogLevel,
    RuntimeSongLoadOptions, gameplay_chart_load_log_entries, load_gameplay_charts_with_options,
    load_song_with_cache_options, load_sync_analysis_chart_with_options,
};
use deadsync_simfile::media::{
    BG_ANIMATIONS_DIR, RANDOM_MOVIES_DIR, SONG_MOVIES_DIR, collect_media_roots,
    random_movie_paths_for_song,
};
use deadsync_simfile::runtime_cache::reload_song_in_cache_with;
use deadsync_simfile::song::ParseSongOptions;
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod scan;

pub(crate) use scan::collect_song_scan_roots;
pub use scan::scan_and_load_courses_with_progress_counts;
pub use scan::{reload_song_dirs_with_progress_counts, scan_and_load_songs_with_progress_counts};

/// Returns true when the pack (song-folder group) that owns this simfile is
/// listed in `NeverCacheList` and so must skip the on-disk cache entirely.
///
/// The group folder is the simfile's pack directory, i.e. the parent of the
/// song directory: `.../Songs/<Group>/<Song>/file.sm`.
fn song_group_is_never_cached(simfile_path: &Path) -> bool {
    simfile_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .is_some_and(config::group_is_never_cached)
}

/// Re-parse one simfile and replace its in-memory song-cache entry.
///
/// This is used after writing sync edits to disk so immediate replays use the
/// updated timing without a full songs rescan.
pub fn reload_song_in_cache(simfile_path: &Path) -> Result<Arc<SongData>, String> {
    let config = config::get();
    let global_offset_seconds = config.global_offset_seconds;
    let cachesongs = config.cachesongs && !song_group_is_never_cached(simfile_path);
    reload_song_in_cache_with(simfile_path, |path| {
        load_song_for_scan(path.to_path_buf(), false, cachesongs, global_offset_seconds)
            .map(|(song, _)| song)
    })
}

pub fn load_gameplay_charts(
    song: &SongData,
    requested_chart_ixs: &[usize],
    global_offset_seconds: f32,
) -> Result<Vec<GameplayChartData>, String> {
    let config = config::get();
    let never_cache = song_group_is_never_cached(&song.simfile_path);
    let cache_dir = dirs::app_dirs().song_cache_dir();
    let parse_options = parse_song_options();
    let options = GameplayChartLoadOptions {
        cache_dir: &cache_dir,
        parse_options: &parse_options,
        allow_cache_read: (config.fastload || config.cachesongs) && !never_cache,
        allow_cache_write: config.cachesongs && !never_cache,
        verify_cache_freshness: !config.fastload,
        global_offset_seconds,
    };
    let result = load_gameplay_charts_with_options(
        song,
        requested_chart_ixs,
        &options,
        compute_music_length_seconds,
    )?;
    log_gameplay_chart_load(song, &result.report);
    Ok(result.charts)
}

pub fn load_sync_analysis_chart(
    song: &SongData,
    chart_ix: usize,
) -> Result<GameplayChartData, String> {
    let config = config::get();
    let cache_dir = dirs::app_dirs().song_cache_dir();
    let parse_options = parse_song_options();
    let options = GameplayChartLoadOptions {
        cache_dir: &cache_dir,
        parse_options: &parse_options,
        allow_cache_read: (config.fastload || config.cachesongs)
            && !song_group_is_never_cached(&song.simfile_path),
        allow_cache_write: false,
        verify_cache_freshness: !config.fastload,
        global_offset_seconds: 0.0,
    };
    let mut result = load_sync_analysis_chart_with_options(
        song,
        chart_ix,
        &options,
        compute_music_length_seconds,
    )?;
    log_gameplay_chart_load(song, &result.report);
    result
        .charts
        .pop()
        .ok_or_else(|| format!("Chart index {chart_ix} out of range"))
}

fn log_gameplay_chart_load(song: &SongData, report: &GameplayChartLoadReport) {
    for entry in gameplay_chart_load_log_entries(song, report) {
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

pub(super) fn load_song_for_scan(
    simfile_path: PathBuf,
    fastload: bool,
    cachesongs: bool,
    global_offset_seconds: f32,
) -> Result<(SongData, bool), String> {
    let cache_dir = dirs::app_dirs().song_cache_dir();
    let parse_options = parse_song_options();
    let options = RuntimeSongLoadOptions {
        cache_dir: &cache_dir,
        parse_options: &parse_options,
        fastload,
        cachesongs,
        verify_cache_freshness: !fastload,
        global_offset_seconds,
    };
    let result =
        load_song_with_cache_options(&simfile_path, &options, compute_music_length_seconds)?;
    for entry in result.log_entries {
        emit_runtime_song_load_log(entry);
    }
    Ok((result.song, result.cache_hit))
}

fn emit_runtime_song_load_log(entry: RuntimeSongLoadLogEntry) {
    match entry.level {
        RuntimeSongLoadLogLevel::Debug => debug!("{}", entry.message),
        RuntimeSongLoadLogLevel::Warn => warn!("{}", entry.message),
    }
}

#[cfg(test)]
pub(crate) fn parse_song_for_test(
    path: &Path,
    global_offset_seconds: f32,
) -> Result<SongData, String> {
    deadsync_simfile::song::parse_song_meta_file(
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
