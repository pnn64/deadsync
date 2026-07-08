use crate::config;
use deadlib_platform::dirs;
use deadsync_online::downloads::{
    DownloadSnapshot, UnlockCache, UnlockDownloadRuntimeHooks, read_unlock_cache_file,
    runtime_completion_counts, runtime_queue_event_unlock_download, runtime_snapshots,
    runtime_take_ready_song_reload_request, write_unlock_cache_file,
};
use deadsync_online::groovestats::ConnectionStatus;
use log::warn;
use std::path::PathBuf;

const DOWNLOAD_RUNTIME_HOOKS: UnlockDownloadRuntimeHooks = UnlockDownloadRuntimeHooks::new(
    downloads_dir,
    unlock_destination_roots,
    load_unlock_cache,
    write_unlock_cache,
    deadsync_online::downloads::log_runtime_event,
);

pub fn init() {
    init_groovestats();
    refresh_arrowcloud_status();
}

pub fn init_groovestats() {
    let cfg = crate::config::get();
    deadsync_online::groovestats::runtime_init_with_default_log(
        cfg.enable_groovestats,
        cfg.enable_boogiestats,
    );
}

pub fn refresh_arrowcloud_status() {
    deadsync_online::arrowcloud::runtime_init_with_default_log(
        crate::config::get().enable_arrowcloud,
    );
}

pub fn is_boogiestats_active() -> bool {
    let cfg = crate::config::get();
    cfg.enable_groovestats && cfg.enable_boogiestats
}

#[inline(always)]
pub fn active_groovestats_service() -> deadsync_online::groovestats::Service {
    deadsync_online::groovestats::Service::from_boogiestats_active(is_boogiestats_active())
}

pub fn unlock_downloads_available() -> bool {
    config::get().auto_download_unlocks
        && matches!(
            deadsync_online::groovestats::runtime_get_status(),
            ConnectionStatus::Connected(_)
        )
}

pub fn unlock_download_snapshots() -> Vec<DownloadSnapshot> {
    runtime_snapshots()
}

pub fn unlock_download_completion_counts() -> (usize, usize) {
    runtime_completion_counts()
}

pub fn take_ready_song_reload_request() -> Vec<PathBuf> {
    runtime_take_ready_song_reload_request()
}

pub fn queue_event_unlock_download(url: &str, unlock_name: &str, pack_name: &str) {
    runtime_queue_event_unlock_download(DOWNLOAD_RUNTIME_HOOKS, url, unlock_name, pack_name);
}

fn downloads_dir() -> PathBuf {
    dirs::app_dirs().downloads_dir()
}

fn unlock_destination_roots() -> Vec<PathBuf> {
    let folders = config::additional_song_folder_roots();
    let mut roots = Vec::with_capacity(1 + folders.len());
    roots.push(dirs::app_dirs().songs_dir());
    roots.extend(
        folders
            .into_iter()
            .filter(|folder| folder.writable)
            .map(|folder| PathBuf::from(folder.path)),
    );
    roots
}

fn load_unlock_cache() -> UnlockCache {
    let path = dirs::app_dirs().unlock_cache_path();
    match read_unlock_cache_file(&path) {
        Ok(cache) => cache,
        Err(error) => {
            if path.exists() {
                warn!("Failed to parse unlock cache {path:?}: {error}");
            }
            UnlockCache::new()
        }
    }
}

fn write_unlock_cache(cache: &UnlockCache) {
    let path = dirs::app_dirs().unlock_cache_path();
    if let Err(error) = write_unlock_cache_file(&path, cache) {
        warn!("Failed to write unlock cache file {path:?}: {error}");
    }
}
