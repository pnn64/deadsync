use crate::config;
use crate::core::network::{self, ConnectionStatus};
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use zip::ZipArchive;

const ITL_UNLOCK_PACK_YEAR: u32 = 2026;
const INVALID_PACK_CHARS: [char; 9] = ['/', '<', '>', ':', '"', '\\', '|', '?', '*'];
const WINDOWS_RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Clone, Debug)]
pub struct DownloadSnapshot {
    pub name: String,
    pub current_bytes: u64,
    pub total_bytes: u64,
    pub complete: bool,
    pub error_message: Option<String>,
}

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
}

type UnlockCache = HashMap<String, HashMap<String, bool>>;

static DOWNLOAD_STATE: LazyLock<Mutex<DownloadState>> =
    LazyLock::new(|| Mutex::new(DownloadState::default()));
static NEXT_DOWNLOAD_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize, Deserialize)]
struct UnlockCacheFile(HashMap<String, HashMap<String, bool>>);

pub fn sort_menu_available() -> bool {
    config::get().auto_download_unlocks
        && matches!(network::get_status(), ConnectionStatus::Connected(_))
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
    let zip_path = downloads_dir().join(download_filename(id));
    let result = download_one(id, url.as_str(), destination.as_str(), &zip_path)
        .and_then(|_| extract_zip(id, &zip_path, destination.as_str(), url.as_str()));
    finish_download(id, result.err());
}

fn download_one(id: u64, url: &str, _destination: &str, zip_path: &Path) -> Result<(), String> {
    if let Some(parent) = zip_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        return Err(format!("Failed to prepare Downloads dir: {error}"));
    }

    let agent = network::get_agent();
    let response = agent.get(url).call().map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(format!("Network Error {status}"));
    }

    let content_type = response
        .headers()
        .get("Content-Type")
        .and_then(|value| value.to_str().ok())
        .map(|value| mime_token(value).to_string())
        .unwrap_or_default();
    let total_bytes = response
        .headers()
        .get("Content-Length")
        .and_then(|value| value.to_str().ok())
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(0);
    set_download_total(id, total_bytes);

    let mut file = File::create(zip_path).map_err(|error| error.to_string())?;
    let mut body = response.into_body();
    let mut reader = body.as_reader();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded = 0u64;
    loop {
        let read = reader.read(&mut buf).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        file.write_all(&buf[..read])
            .map_err(|error| error.to_string())?;
        downloaded = downloaded.saturating_add(read as u64);
        set_download_progress(id, downloaded, total_bytes);
    }
    set_download_progress(id, downloaded, total_bytes.max(downloaded));

    if content_type.as_str() != "application/zip" {
        warn!("Attempted to download non-zip unlock from '{url}' ({content_type}).");
        return Err("Download is not a Zip!".to_string());
    }

    Ok(())
}

fn extract_zip(id: u64, zip_path: &Path, destination: &str, url: &str) -> Result<(), String> {
    let destination_pack = songs_dir().join(destination);
    unzip_to_destination(zip_path, &destination_pack)
        .map_err(|_| "Failed to Unzip!".to_string())?;
    write_pack_ini_if_needed(&destination_pack, destination).map_err(|error| error.to_string())?;
    mark_cache_success(url, destination);
    let total = file_len(zip_path);
    set_download_progress(id, total, total);
    Ok(())
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
    let lower = pack_name.to_ascii_lowercase();
    if !lower.contains(&format!("itl online {ITL_UNLOCK_PACK_YEAR} unlocks")) {
        return Ok(());
    }
    let pack_ini = destination_pack.join("Pack.ini");
    if pack_ini.exists() {
        return Ok(());
    }
    let content = format!(
        "[Group]\nVersion=1\nDisplayTitle={pack_name}\nTranslitTitle={pack_name}\nSortTitle={pack_name}\nSeries=ITL Online\nYear={ITL_UNLOCK_PACK_YEAR}\nBanner=\nSyncOffset=NULL\n"
    );
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

fn set_download_total(id: u64, total_bytes: u64) {
    let mut state = DOWNLOAD_STATE.lock().unwrap();
    if let Some(entry) = state.entries.iter_mut().find(|entry| entry.id == id) {
        entry.total_bytes = total_bytes;
    }
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
    let path = unlock_cache_path();
    let Ok(text) = fs::read_to_string(&path) else {
        return HashMap::new();
    };
    serde_json::from_str::<UnlockCacheFile>(&text)
        .map(|file| file.0)
        .unwrap_or_else(|error| {
            warn!("Failed to parse unlock cache {path:?}: {error}");
            HashMap::new()
        })
}

fn write_unlock_cache(cache: &UnlockCache) {
    let path = unlock_cache_path();
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

fn cache_has_destination(cache: &UnlockCache, url: &str, destination: &str) -> bool {
    cache
        .get(url)
        .and_then(|packs| packs.get(destination))
        .copied()
        .unwrap_or(false)
}

fn unlock_cache_path() -> PathBuf {
    songs_dir().join("unlocks-cache.json")
}

fn songs_dir() -> PathBuf {
    PathBuf::from("songs")
}

fn downloads_dir() -> PathBuf {
    PathBuf::from("cache/downloads")
}

fn download_filename(id: u64) -> String {
    format!("{id:016x}.zip")
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn sanitize_pack_name(raw: &str) -> String {
    let mut sanitized = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if INVALID_PACK_CHARS.contains(&ch) {
            continue;
        }
        sanitized.push(ch);
    }
    if sanitized.trim().is_empty() {
        sanitized = "Unlocks".to_string();
    }
    if WINDOWS_RESERVED_NAMES
        .iter()
        .any(|name| name.eq_ignore_ascii_case(sanitized.trim()))
    {
        return format!(" {} ", sanitized.trim());
    }
    sanitized
}

fn mime_token(value: &str) -> &str {
    value.split(';').next().unwrap_or("").trim()
}

#[cfg(test)]
mod tests {
    use super::{mime_token, sanitize_pack_name};

    #[test]
    fn sanitize_pack_name_strips_invalid_chars() {
        assert_eq!(sanitize_pack_name("ITL/Unlocks:*?"), "ITLUnlocks");
    }

    #[test]
    fn sanitize_pack_name_avoids_windows_reserved_names() {
        assert_eq!(sanitize_pack_name("CON"), " CON ");
    }

    #[test]
    fn mime_token_strips_parameters() {
        assert_eq!(
            mime_token("application/zip; charset=binary"),
            "application/zip"
        );
    }
}
