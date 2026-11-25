use deadsync_chart::{GameplayChartData, SongData};
use deadsync_config::{self as config, app_config::Config};
use deadsync_simfile::{app_runtime, runtime::RuntimeSongConfig, scan::SongLoadOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) fn scan_options() -> SongLoadOptions {
    let cfg = config::runtime::get();
    SongLoadOptions {
        fastload: cfg.fastload,
        cachesongs: cfg.cachesongs,
        global_offset_seconds: cfg.global_offset_seconds,
        song_parsing_threads: u32::from(cfg.song_parsing_threads),
    }
}

pub(crate) fn additional_song_roots() -> Vec<(String, PathBuf)> {
    config::runtime::additional_song_folder_roots()
        .into_iter()
        .map(|folder| {
            let path = PathBuf::from(folder.path.as_str());
            (folder.path, path)
        })
        .collect()
}

pub(crate) fn collect_song_scan_roots(root: &Path) -> Vec<PathBuf> {
    app_runtime::collect_song_scan_roots(root, additional_song_roots())
}

pub(crate) fn load_gameplay_charts(
    song: &SongData,
    requested_chart_ixs: &[usize],
    global_offset_seconds: f32,
) -> Result<Vec<GameplayChartData>, String> {
    let cfg = config::runtime::get();
    app_runtime::load_gameplay_charts(
        song,
        requested_chart_ixs,
        RuntimeSongConfig::new(cfg.fastload, cfg.cachesongs, global_offset_seconds),
        config::runtime::group_is_never_cached,
    )
}

pub(crate) fn reload_song_in_cache(path: &Path) -> Result<Arc<SongData>, String> {
    let cfg = config::runtime::get();
    app_runtime::reload_song_in_cache(
        path,
        RuntimeSongConfig::new(cfg.fastload, cfg.cachesongs, cfg.global_offset_seconds),
        config::runtime::group_is_never_cached,
    )
}

pub(crate) fn pack_sync_offset_for_song(song: &SongData, cfg: &Config) -> f32 {
    deadsync_simfile::runtime_cache::pack_sync_offset_for_song(
        song,
        cfg.machine_pack_ini_offsets,
        cfg.machine_default_sync_offset.sync_pref(),
    )
}
