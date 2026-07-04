use super::groovestats;
use crate::config;
use deadlib_platform::dirs;
use deadsync_online::downloads::{
    DownloadSnapshot, DownloadZipError, UnlockCache, cache_has_destination, choose_unlock_root,
    download_filename, download_zip_to_path, file_len, read_unlock_cache_file, sanitize_pack_name,
    unzip_to_destination, write_pack_ini_if_needed, write_unlock_cache_file,
};
use deadsync_online::groovestats::ConnectionStatus;
use log::{debug, warn};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

#[derive(Clone, Debug)]
struct DownloadEntry {
    id: u64,
    name: String,
    url: String,
    destination: String,
    current_bytes: u64,
    total_bytes: u64,
    complete: bool,
    error_message: Option<String>,
}

#[derive(Default)]
struct DownloadState {
    cache_loaded: bool,
    unlock_cache: UnlockCache,
    entries: Vec<DownloadEntry>,
    ready_song_reload_dirs: Vec<PathBuf>,
}

static DOWNLOAD_STATE: LazyLock<Mutex<DownloadState>> =
    LazyLock::new(|| Mutex::new(DownloadState::default()));
static NEXT_DOWNLOAD_ID: AtomicU64 = AtomicU64::new(1);

pub fn sort_menu_available() -> bool {
    config::get().auto_download_unlocks
        && matches!(groovestats::get_status(), ConnectionStatus::Connected(_))
}

pub fn snapshots() -> Vec<DownloadSnapshot> {
    DOWNLOAD_STATE
        .lock()
        .unwrap()
        .entries
        .iter()
        .map(|entry| DownloadSnapshot {
            name: entry.name.clone(),
            current_bytes: entry.current_bytes,
            total_bytes: entry.total_bytes,
            complete: entry.complete,
            error_message: entry.error_message.clone(),
        })
        .collect()
}

pub fn completion_counts() -> (usize, usize) {
    let state = DOWNLOAD_STATE.lock().unwrap();
    let total = state.entries.len();
    let finished = state.entries.iter().filter(|entry| entry.complete).count();
    (finished, total)
}

pub fn take_ready_song_reload_request() -> Vec<PathBuf> {
    let mut state = DOWNLOAD_STATE.lock().unwrap();
    if state.ready_song_reload_dirs.is_empty() || state.entries.iter().any(|entry| !entry.complete)
    {
        return Vec::new();
    }
    std::mem::take(&mut state.ready_song_reload_dirs)
}

pub fn queue_event_unlock_download(url: &str, unlock_name: &str, pack_name: &str) {
    let url = url.trim();
    if url.is_empty() {
        return;
    }
    let name = if unlock_name.trim().is_empty() {
        pack_name.trim().to_string()
    } else {
        unlock_name.trim().to_string()
    };
    let destination = sanitize_pack_name(pack_name);
    let id = match begin_download(url, name, destination.clone()) {
        Some(id) => id,
        None => return,
    };
    let url = url.to_string();
    std::thread::spawn(move || download_worker(id, url, destination));
}

fn begin_download(url: &str, name: String, destination: String) -> Option<u64> {
    let mut state = DOWNLOAD_STATE.lock().unwrap();
    ensure_cache_loaded(&mut state);
    if cache_has_destination(&state.unlock_cache, url, destination.as_str()) {
        debug!("Skipping unlock download for cached url='{url}' dest='{destination}'.");
        return None;
    }
    if state
        .entries
        .iter()
        .any(|entry| entry.url == url && entry.destination == destination)
    {
        debug!("Skipping duplicate unlock download url='{url}' dest='{destination}'.");
        return None;
    }

    let id = NEXT_DOWNLOAD_ID.fetch_add(1, Ordering::Relaxed);
    state.entries.push(DownloadEntry {
        id,
        name,
        url: url.to_string(),
        destination,
        current_bytes: 0,
        total_bytes: 0,
        complete: false,
        error_message: None,
    });
    Some(id)
}

fn download_worker(id: u64, url: String, destination: String) {
    let zip_path = dirs::app_dirs().downloads_dir().join(download_filename(id));
    let result = download_one(id, url.as_str(), &zip_path)
        .and_then(|_| extract_zip(id, &zip_path, destination.as_str(), url.as_str()));
    finish_download(id, result.err());
}

fn download_one(id: u64, url: &str, zip_path: &Path) -> Result<(), String> {
    download_zip_to_path(url, zip_path, |downloaded, total| {
        set_download_progress(id, downloaded, total);
    })
    .map_err(|error| {
        if let DownloadZipError::NotZip { content_type } = &error {
            warn!("Attempted to download non-zip unlock from '{url}' ({content_type}).");
        }
        error.to_string()
    })
}

fn extract_zip(id: u64, zip_path: &Path, destination: &str, url: &str) -> Result<(), String> {
    let destination_pack = unlock_destination_pack(destination);
    unzip_to_destination(zip_path, &destination_pack)
        .map_err(|_| "Failed to Unzip!".to_string())?;
    write_pack_ini_if_needed(&destination_pack, destination).map_err(|error| error.to_string())?;
    mark_cache_success(url, destination);
    queue_ready_song_reload_dir(destination_pack);
    let total = file_len(zip_path);
    set_download_progress(id, total, total);
    Ok(())
}

fn unlock_destination_pack(destination: &str) -> PathBuf {
    let roots = unlock_destination_roots();
    let root_idx = choose_unlock_root(destination, &roots).unwrap_or(0);
    roots[root_idx].join(destination)
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

fn mark_cache_success(url: &str, destination: &str) {
    let cache_snapshot = {
        let mut state = DOWNLOAD_STATE.lock().unwrap();
        ensure_cache_loaded(&mut state);
        state
            .unlock_cache
            .entry(url.to_string())
            .or_default()
            .insert(destination.to_string(), true);
        state.unlock_cache.clone()
    };
    write_unlock_cache(&cache_snapshot);
}

fn queue_ready_song_reload_dir(path: PathBuf) {
    let mut state = DOWNLOAD_STATE.lock().unwrap();
    if state
        .ready_song_reload_dirs
        .iter()
        .any(|existing| existing == &path)
    {
        return;
    }
    state.ready_song_reload_dirs.push(path);
}

fn set_download_progress(id: u64, current_bytes: u64, total_bytes: u64) {
    let mut state = DOWNLOAD_STATE.lock().unwrap();
    if let Some(entry) = state.entries.iter_mut().find(|entry| entry.id == id) {
        entry.current_bytes = current_bytes;
        entry.total_bytes = total_bytes;
    }
}

fn finish_download(id: u64, error_message: Option<String>) {
    let mut state = DOWNLOAD_STATE.lock().unwrap();
    if let Some(entry) = state.entries.iter_mut().find(|entry| entry.id == id) {
        entry.complete = true;
        entry.error_message = error_message;
        if entry.total_bytes == 0 {
            entry.total_bytes = entry.current_bytes;
        }
    }
}

fn ensure_cache_loaded(state: &mut DownloadState) {
    if state.cache_loaded {
        return;
    }
    state.unlock_cache = load_unlock_cache();
    state.cache_loaded = true;
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

#[cfg(test)]
mod tests {
    use super::{
        DOWNLOAD_STATE, DownloadEntry, DownloadState, NEXT_DOWNLOAD_ID,
        take_ready_song_reload_request,
    };
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;

    fn reset_download_state() {
        *DOWNLOAD_STATE.lock().unwrap() = DownloadState::default();
        NEXT_DOWNLOAD_ID.store(1, Ordering::Relaxed);
    }

    #[test]
    fn take_ready_song_reload_request_waits_for_downloads() {
        reset_download_state();
        {
            let mut state = DOWNLOAD_STATE.lock().unwrap();
            state.entries.push(DownloadEntry {
                id: 1,
                name: "Unlock".to_string(),
                url: "https://example.com/unlock.zip".to_string(),
                destination: "ITL Unlocks".to_string(),
                current_bytes: 10,
                total_bytes: 10,
                complete: false,
                error_message: None,
            });
            state
                .ready_song_reload_dirs
                .push(PathBuf::from("Songs/ITL Unlocks"));
        }

        assert!(take_ready_song_reload_request().is_empty());

        DOWNLOAD_STATE.lock().unwrap().entries[0].complete = true;

        assert_eq!(
            take_ready_song_reload_request(),
            vec![PathBuf::from("Songs/ITL Unlocks")]
        );
        assert!(take_ready_song_reload_request().is_empty());
    }
}
