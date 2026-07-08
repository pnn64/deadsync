use crate::config::{self};
use crate::game::song::get_song_cache;
use deadlib_platform::dirs;
use deadsync_audio_decode as decode;
use deadsync_chart::{GameplayChartData, SongData};
use deadsync_simfile::cache::{
    GameplayChartLoadOptions, GameplayChartLoadReport, GameplayChartLoadSource,
    GameplayChartLoadWarning, SerializableSongData, build_song_meta,
    load_gameplay_charts_with_options, load_song_cache_file, load_sync_analysis_chart_with_options,
    song_cache_path, write_song_cache_file,
};
use deadsync_simfile::media::{
    BG_ANIMATIONS_DIR, RANDOM_MOVIES_DIR, SONG_MOVIES_DIR, collect_media_roots,
};
use deadsync_simfile::song::{ParseSongOptions, parse_song_data_file};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod scan;

pub(crate) use scan::collect_song_scan_roots;
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

pub(super) fn compute_song_cache_path(path: &Path) -> Option<PathBuf> {
    let cache_dir = dirs::app_dirs().song_cache_dir();
    match song_cache_path(&cache_dir, path) {
        Ok(path) => Some(path),
        Err(error) => {
            warn!(
                "Could not generate cache path for {path:?}: {error}. Caching disabled for this file."
            );
            None
        }
    }
}

pub(super) fn load_song_from_cache(
    path: &Path,
    cache_path: &Path,
    verify_freshness: bool,
) -> Option<SongData> {
    let song = load_song_cache_file(path, cache_path, verify_freshness)?;
    debug!("Cache hit for: {:?}", path.file_name().unwrap_or_default());
    Some(song)
}

fn write_song_cache(cache_path: &Path, data: &SerializableSongData, global_offset_seconds: f32) {
    if let Err(error) = write_song_cache_file(cache_path, data, global_offset_seconds) {
        warn!(
            "Could not write song cache for {:?}: {error}",
            Path::new(&data.simfile_path)
                .file_name()
                .unwrap_or_default()
        );
    }
}

/// Re-parse one simfile and replace its in-memory song-cache entry.
///
/// This is used after writing sync edits to disk so immediate replays use the
/// updated timing without a full songs rescan.
pub fn reload_song_in_cache(simfile_path: &Path) -> Result<Arc<SongData>, String> {
    let config = config::get();
    let global_offset_seconds = config.global_offset_seconds;
    let cachesongs = config.cachesongs && !song_group_is_never_cached(simfile_path);
    let cache_path = cachesongs
        .then(|| compute_song_cache_path(simfile_path))
        .flatten();
    let song_data = parse_song_and_maybe_write_cache(
        simfile_path,
        false,
        cachesongs,
        cache_path.as_deref(),
        global_offset_seconds,
    )?;
    let updated = Arc::new(song_data);

    let mut song_cache = get_song_cache();
    let mut replaced = false;
    for pack in song_cache.iter_mut() {
        for song in &mut pack.songs {
            if song.simfile_path == simfile_path {
                *song = updated.clone();
                replaced = true;
            }
        }
    }
    if !replaced {
        return Err(format!(
            "Song '{}' not found in song cache",
            simfile_path.display()
        ));
    }
    Ok(updated)
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
    log_gameplay_cache_warnings(&report.warnings);
    if let GameplayChartLoadSource::Cache { verify_freshness } = report.source {
        if verify_freshness {
            debug!(
                "Gameplay cache hit for: {:?}",
                song.simfile_path.file_name().unwrap_or_default()
            );
        } else {
            debug!(
                "Gameplay cache hit (no freshness check) for: {:?}",
                song.simfile_path.file_name().unwrap_or_default()
            );
        }
    }
    if let Some(song_data_load) = &report.song_data_load {
        if song_data_load.elapsed_ms >= 25.0 {
            info!(
                "Gameplay song data load: source=parse file={:?} parse_ms={:.3} write_ms={:.3} elapsed_ms={:.3}",
                song.simfile_path.file_name().unwrap_or_default(),
                song_data_load.parse_ms,
                song_data_load.write_ms,
                song_data_load.elapsed_ms
            );
        } else {
            debug!(
                "Gameplay song data load: source=parse file={:?} parse_ms={:.3} write_ms={:.3} elapsed_ms={:.3}",
                song.simfile_path.file_name().unwrap_or_default(),
                song_data_load.parse_ms,
                song_data_load.write_ms,
                song_data_load.elapsed_ms
            );
        }
    }
    if report.elapsed_ms >= 25.0 {
        info!(
            "Gameplay chart payload load: song='{}' requested={} load_ms={:.3} materialize_ms={:.3} elapsed_ms={:.3}",
            song.title,
            report.requested_count,
            report.load_ms,
            report.materialize_ms,
            report.elapsed_ms
        );
    } else {
        debug!(
            "Gameplay chart payload load: song='{}' requested={} load_ms={:.3} materialize_ms={:.3} elapsed_ms={:.3}",
            song.title,
            report.requested_count,
            report.load_ms,
            report.materialize_ms,
            report.elapsed_ms
        );
    }
}

fn log_gameplay_cache_warnings(warnings: &[GameplayChartLoadWarning]) {
    for warning in warnings {
        match warning {
            GameplayChartLoadWarning::CachePath { path, error } => warn!(
                "Could not generate cache path for {path:?}: {error}. Caching disabled for this file."
            ),
            GameplayChartLoadWarning::CacheWrite {
                simfile_name,
                error,
            } => warn!("Could not write song cache for {simfile_name:?}: {error}"),
        }
    }
}

pub(super) fn parse_song_and_maybe_write_cache(
    path: &Path,
    fastload: bool,
    cachesongs: bool,
    cache_path: Option<&Path>,
    global_offset_seconds: f32,
) -> Result<SongData, String> {
    if fastload {
        debug!("Cache miss for: {:?}", path.file_name().unwrap_or_default());
    } else {
        debug!(
            "Parsing (fastload disabled): {:?}",
            path.file_name().unwrap_or_default()
        );
    }
    let song_data = parse_song_cache_data(path, global_offset_seconds)?;
    if cachesongs && let Some(cp) = cache_path {
        write_song_cache(cp, &song_data, global_offset_seconds);
    }
    Ok(build_song_meta(song_data, global_offset_seconds))
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

fn parse_song_options() -> ParseSongOptions {
    ParseSongOptions::new(
        bgchange_asset_roots(SONG_MOVIES_DIR),
        bgchange_asset_roots(RANDOM_MOVIES_DIR),
        bgchange_asset_roots(BG_ANIMATIONS_DIR),
    )
}

/// Parse and normalize a simfile on a cache miss.
fn parse_song_cache_data(
    path: &Path,
    global_offset_seconds: f32,
) -> Result<SerializableSongData, String> {
    parse_song_data_file(
        path,
        &parse_song_options(),
        global_offset_seconds,
        compute_music_length_seconds,
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
