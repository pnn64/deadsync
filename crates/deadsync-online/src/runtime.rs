use crate::downloads::{
    UnlockCache, UnlockDownloadRuntimeHooks, read_unlock_cache_file,
    runtime_forget_cached_destination, runtime_queue_event_unlock_download,
    runtime_retry_failed_downloads, unlock_destination_roots as download_destination_roots,
    unlock_downloads_available as downloads_available, write_unlock_cache_file,
};
pub use crate::downloads::{
    runtime_completion_counts as unlock_download_completion_counts,
    runtime_snapshots as unlock_download_snapshots,
    runtime_take_ready_song_reload_request as take_ready_song_reload_request,
};
use deadlib_platform::dirs;
use log::warn;
use std::path::PathBuf;

const DOWNLOAD_RUNTIME_HOOKS: UnlockDownloadRuntimeHooks = UnlockDownloadRuntimeHooks::new(
    downloads_dir,
    unlock_destination_roots,
    load_unlock_cache,
    write_unlock_cache,
    crate::downloads::log_runtime_event,
);

pub fn init() {
    init_groovestats();
    refresh_arrowcloud_status();
}

pub fn init_groovestats() {
    let cfg = deadsync_config::runtime::get();
    crate::groovestats::runtime_init_with_default_log(
        cfg.enable_groovestats,
        cfg.enable_boogiestats,
    );
}

pub fn refresh_arrowcloud_status() {
    crate::arrowcloud::runtime_init_with_default_log(
        deadsync_config::runtime::get().enable_arrowcloud,
    );
}

pub fn is_boogiestats_active() -> bool {
    let cfg = deadsync_config::runtime::get();
    crate::groovestats::boogiestats_active(cfg.enable_groovestats, cfg.enable_boogiestats)
}

#[inline(always)]
pub fn active_groovestats_service() -> crate::groovestats::Service {
    let cfg = deadsync_config::runtime::get();
    crate::groovestats::active_service(cfg.enable_groovestats, cfg.enable_boogiestats)
}

pub fn unlock_downloads_available() -> bool {
    downloads_available(
        deadsync_config::runtime::get().auto_download_unlocks,
        &crate::groovestats::runtime_get_status(),
    )
}

pub fn queue_event_unlock_download(url: &str, unlock_name: &str, pack_name: &str) {
    runtime_queue_event_unlock_download(DOWNLOAD_RUNTIME_HOOKS, url, unlock_name, pack_name);
}

pub fn retry_failed_unlock_downloads() -> usize {
    runtime_retry_failed_downloads(DOWNLOAD_RUNTIME_HOOKS)
}

pub fn forget_cached_unlock(url: &str, pack_name: &str) {
    let destination = crate::downloads::sanitize_pack_name(pack_name);
    runtime_forget_cached_destination(DOWNLOAD_RUNTIME_HOOKS, url, &destination);
}

fn downloads_dir() -> PathBuf {
    dirs::app_dirs().downloads_dir()
}

fn unlock_destination_roots() -> Vec<PathBuf> {
    download_destination_roots(
        dirs::app_dirs().songs_dir(),
        deadsync_config::runtime::additional_song_folder_roots()
            .into_iter()
            .map(|folder| (PathBuf::from(folder.path), folder.writable)),
    )
}

pub(crate) fn installed_pack_paths(pack_name: &str) -> Vec<PathBuf> {
    let mut roots = vec![dirs::app_dirs().songs_dir()];
    for folder in deadsync_config::runtime::additional_song_folder_roots() {
        let root = PathBuf::from(folder.path);
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots.into_iter().map(|root| root.join(pack_name)).collect()
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
