use super::groovestats;
use crate::config;
use deadlib_platform::dirs;
use deadsync_online::downloads::{
    DownloadRuntimeEvent, DownloadSnapshot, UnlockCache, UnlockDownloadRuntimeHooks,
    read_unlock_cache_file, runtime_completion_counts, runtime_queue_event_unlock_download,
    runtime_snapshots, runtime_take_ready_song_reload_request, write_unlock_cache_file,
};
use deadsync_online::groovestats::ConnectionStatus;
use log::{debug, warn};
use std::path::PathBuf;

const RUNTIME_HOOKS: UnlockDownloadRuntimeHooks = UnlockDownloadRuntimeHooks::new(
    downloads_dir,
    unlock_destination_roots,
    load_unlock_cache,
    write_unlock_cache,
    log_download_event,
);

pub fn sort_menu_available() -> bool {
    config::get().auto_download_unlocks
        && matches!(groovestats::get_status(), ConnectionStatus::Connected(_))
}

pub fn snapshots() -> Vec<DownloadSnapshot> {
    runtime_snapshots()
}

pub fn completion_counts() -> (usize, usize) {
    runtime_completion_counts()
}

pub fn take_ready_song_reload_request() -> Vec<PathBuf> {
    runtime_take_ready_song_reload_request()
}

pub fn queue_event_unlock_download(url: &str, unlock_name: &str, pack_name: &str) {
    runtime_queue_event_unlock_download(RUNTIME_HOOKS, url, unlock_name, pack_name);
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

fn log_download_event(event: DownloadRuntimeEvent) {
    match event {
        DownloadRuntimeEvent::Cached { url, destination } => {
            debug!("Skipping unlock download for cached url='{url}' dest='{destination}'.");
        }
        DownloadRuntimeEvent::Duplicate { url, destination } => {
            debug!("Skipping duplicate unlock download url='{url}' dest='{destination}'.");
        }
        DownloadRuntimeEvent::NonZip { url, content_type } => {
            warn!("Attempted to download non-zip unlock from '{url}' ({content_type}).");
        }
    }
}
