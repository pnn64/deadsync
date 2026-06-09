use crate::config::dirs;
use deadsync_chart::{GameplayChartData, SongData};
use deadsync_simfile::cache::{
    SerializableSongData, load_gameplay_charts_cache_file, load_song_cache_file, song_cache_path,
    write_song_cache_file,
};
use log::{debug, warn};
use std::path::{Path, PathBuf};

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

pub(super) fn write_song_cache(
    cache_path: &Path,
    data: &SerializableSongData,
    global_offset_seconds: f32,
) {
    if let Err(error) = write_song_cache_file(cache_path, data, global_offset_seconds) {
        warn!(
            "Could not write song cache for {:?}: {error}",
            Path::new(&data.simfile_path)
                .file_name()
                .unwrap_or_default()
        );
    }
}

pub(super) fn load_gameplay_charts_from_cache(
    song: &SongData,
    requested_chart_ixs: &[usize],
    global_offset_seconds: f32,
    verify_freshness: bool,
) -> Option<Vec<GameplayChartData>> {
    let cache_path = compute_song_cache_path(&song.simfile_path)?;
    let charts = load_gameplay_charts_cache_file(
        song,
        &cache_path,
        requested_chart_ixs,
        global_offset_seconds,
        verify_freshness,
    )?;
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
    Some(charts)
}
