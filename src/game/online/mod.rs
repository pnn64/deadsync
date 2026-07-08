use crate::config;
use deadlib_platform::dirs;
use deadsync_online::downloads::{
    UnlockCache, UnlockDownloadRuntimeHooks, read_unlock_cache_file,
    runtime_queue_event_unlock_download,
    unlock_destination_roots as online_unlock_destination_roots,
    unlock_downloads_available as online_unlock_downloads_available, write_unlock_cache_file,
};
pub use deadsync_online::downloads::{
    runtime_completion_counts as unlock_download_completion_counts,
    runtime_snapshots as unlock_download_snapshots,
    runtime_take_ready_song_reload_request as take_ready_song_reload_request,
};
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
    deadsync_online::groovestats::boogiestats_active(cfg.enable_groovestats, cfg.enable_boogiestats)
}

#[inline(always)]
pub fn active_groovestats_service() -> deadsync_online::groovestats::Service {
    let cfg = crate::config::get();
    deadsync_online::groovestats::active_service(cfg.enable_groovestats, cfg.enable_boogiestats)
}

pub fn unlock_downloads_available() -> bool {
    online_unlock_downloads_available(
        config::get().auto_download_unlocks,
        &deadsync_online::groovestats::runtime_get_status(),
    )
}

pub fn queue_event_unlock_download(url: &str, unlock_name: &str, pack_name: &str) {
    runtime_queue_event_unlock_download(DOWNLOAD_RUNTIME_HOOKS, url, unlock_name, pack_name);
}

fn downloads_dir() -> PathBuf {
    dirs::app_dirs().downloads_dir()
}

fn unlock_destination_roots() -> Vec<PathBuf> {
    online_unlock_destination_roots(
        dirs::app_dirs().songs_dir(),
        config::additional_song_folder_roots()
            .into_iter()
            .map(|folder| (PathBuf::from(folder.path), folder.writable)),
    )
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
