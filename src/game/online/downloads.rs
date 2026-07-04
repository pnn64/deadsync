use super::groovestats;
use crate::config;
use deadlib_platform::dirs;
use deadsync_online::downloads::{
    DownloadSnapshot, DownloadZipError, UnlockCache, UnlockCacheFile, cache_has_destination,
    download_zip_to_path, itl_unlock_pack_ini_content, sanitize_pack_name,
};
use deadsync_online::groovestats::ConnectionStatus;
use log::{debug, warn};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use zip::ZipArchive;

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

fn choose_unlock_root(destination: &str, roots: &[PathBuf]) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None;
    for (idx, root) in roots.iter().enumerate() {
        let Some(score) = unlock_root_score(root, destination) else {
            continue;
        };
        if best.is_none_or(|(best_idx, best_score)| {
            score < best_score || score == best_score && idx > best_idx
        }) {
            best = Some((idx, score));
        }
    }
    best.map(|(idx, _)| idx)
}

fn unlock_root_score(root: &Path, destination: &str) -> Option<usize> {
    if root.exists() && !root.is_dir() {
        return None;
    }
    let pack = root.join(destination);
    if pack.exists() && !pack.is_dir() {
        return None;
    }
    Some(usize::from(!pack.is_dir()))
}

fn unzip_to_destination(
    zip_path: &Path,
    destination: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(destination)?;
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx)?;
        let Some(relative_path) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        let out_path = destination.join(relative_path);
        if entry.name().ends_with('/') {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out_file = File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out_file)?;
    }
    Ok(())
}

fn write_pack_ini_if_needed(
    destination_pack: &Path,
    pack_name: &str,
) -> Result<(), std::io::Error> {
    let Some(content) = itl_unlock_pack_ini_content(pack_name) else {
        return Ok(());
    };
    let pack_ini = destination_pack.join("Pack.ini");
    if pack_ini.exists() {
        return Ok(());
    }
    fs::write(pack_ini, content)
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
    let Ok(text) = fs::read_to_string(&path) else {
        return UnlockCache::new();
    };
    serde_json::from_str::<UnlockCacheFile>(&text)
        .map(|file| file.0)
        .unwrap_or_else(|error| {
            warn!("Failed to parse unlock cache {path:?}: {error}");
            UnlockCache::new()
        })
}

fn write_unlock_cache(cache: &UnlockCache) {
    let path = dirs::app_dirs().unlock_cache_path();
    if let Some(parent) = path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        warn!("Failed to create unlock cache dir {parent:?}: {error}");
        return;
    }
    let Ok(text) = serde_json::to_string(&UnlockCacheFile(cache.clone())) else {
        warn!("Failed to encode unlock cache.");
        return;
    };
    let tmp = path.with_extension("tmp");
    if let Err(error) = fs::write(&tmp, text) {
        warn!("Failed to write unlock cache temp file {tmp:?}: {error}");
        return;
    }
    if let Err(error) = fs::rename(&tmp, &path) {
        warn!("Failed to commit unlock cache file {path:?}: {error}");
        let _ = fs::remove_file(&tmp);
    }
}

fn download_filename(id: u64) -> String {
    format!("{id:016x}.zip")
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{
        DOWNLOAD_STATE, DownloadEntry, DownloadState, NEXT_DOWNLOAD_ID, choose_unlock_root,
        take_ready_song_reload_request,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn reset_download_state() {
        *DOWNLOAD_STATE.lock().unwrap() = DownloadState::default();
        NEXT_DOWNLOAD_ID.store(1, Ordering::Relaxed);
    }

    fn temp_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "deadsync-downloads-{label}-{}-{unique}",
            std::process::id()
        ))
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

    #[test]
    fn unlock_root_prefers_last_writable_root_for_new_pack() {
        let roots = vec![
            PathBuf::from("Songs"),
            PathBuf::from("ExtraSongsA"),
            PathBuf::from("ExtraSongsB"),
        ];

        assert_eq!(
            choose_unlock_root("Stamina RPG 10 Unlocks", &roots),
            Some(2)
        );
    }

    #[test]
    fn unlock_root_keeps_existing_pack_location() {
        let root = temp_root("existing-pack");
        let primary = root.join("songs");
        let extra = root.join("extra");
        fs::create_dir_all(primary.join("ITL Online 2026 Unlocks"))
            .expect("create primary unlock pack");
        fs::create_dir_all(&extra).expect("create extra song root");

        let roots = vec![primary, extra];

        assert_eq!(
            choose_unlock_root("ITL Online 2026 Unlocks", &roots),
            Some(0)
        );
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn unlock_root_uses_existing_additional_pack() {
        let root = temp_root("existing-extra-pack");
        let primary = root.join("songs");
        let extra = root.join("extra");
        fs::create_dir_all(&primary).expect("create primary song root");
        fs::create_dir_all(extra.join("Stamina RPG 10 Unlocks")).expect("create extra unlock pack");

        let roots = vec![primary, extra];

        assert_eq!(
            choose_unlock_root("Stamina RPG 10 Unlocks", &roots),
            Some(1)
        );
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn unlock_root_skips_file_candidates() {
        let root = temp_root("file-candidate");
        let primary = root.join("songs");
        let extra = root.join("extra");
        fs::create_dir_all(&primary).expect("create primary song root");
        fs::write(extra, "not a directory").expect("create extra file");

        let roots = vec![primary, root.join("extra")];

        assert_eq!(
            choose_unlock_root("ITL Online 2026 Unlocks", &roots),
            Some(0)
        );
        fs::remove_dir_all(root).expect("remove test root");
    }
}
