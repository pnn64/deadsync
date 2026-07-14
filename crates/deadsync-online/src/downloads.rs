use crate::groovestats::ConnectionStatus;
use deadsync_net as network;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::thread;
use zip::ZipArchive;

const ITL_UNLOCK_PACK_YEAR: u32 = 2026;
const INVALID_PACK_CHARS: [char; 9] = ['/', '<', '>', ':', '"', '\\', '|', '?', '*'];
const WINDOWS_RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub type UnlockCache = HashMap<String, HashMap<String, bool>>;

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
pub struct DownloadState {
    cache_loaded: bool,
    unlock_cache: UnlockCache,
    entries: Vec<DownloadEntry>,
    ready_song_reload_dirs: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueDownloadResult {
    Queued(u64),
    Cached,
    Duplicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventUnlockDownload {
    pub id: u64,
    pub url: String,
    pub destination: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueEventUnlockDownloadResult {
    Queued(EventUnlockDownload),
    EmptyUrl,
    Cached { destination: String },
    Duplicate { destination: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnlockDownloadSuccess {
    pub url: String,
    pub destination: String,
    pub destination_pack: PathBuf,
    pub final_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnlockDownloadFailure {
    pub error_message: String,
    pub not_zip_content_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnlockDownloadWorkerResult {
    Success(UnlockDownloadSuccess),
    Failure(UnlockDownloadFailure),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadRuntimeEvent {
    Cached { url: String, destination: String },
    Duplicate { url: String, destination: String },
    NonZip { url: String, content_type: String },
}

pub type DownloadsDirFn = fn() -> PathBuf;
pub type UnlockDestinationRootsFn = fn() -> Vec<PathBuf>;
pub type LoadUnlockCacheFn = fn() -> UnlockCache;
pub type WriteUnlockCacheFn = fn(&UnlockCache);
pub type DownloadRuntimeEventFn = fn(DownloadRuntimeEvent);

#[derive(Clone, Copy)]
pub struct UnlockDownloadRuntimeHooks {
    downloads_dir: DownloadsDirFn,
    unlock_destination_roots: UnlockDestinationRootsFn,
    load_unlock_cache: LoadUnlockCacheFn,
    write_unlock_cache: WriteUnlockCacheFn,
    on_event: DownloadRuntimeEventFn,
}

impl UnlockDownloadRuntimeHooks {
    #[inline(always)]
    pub const fn new(
        downloads_dir: DownloadsDirFn,
        unlock_destination_roots: UnlockDestinationRootsFn,
        load_unlock_cache: LoadUnlockCacheFn,
        write_unlock_cache: WriteUnlockCacheFn,
        on_event: DownloadRuntimeEventFn,
    ) -> Self {
        Self {
            downloads_dir,
            unlock_destination_roots,
            load_unlock_cache,
            write_unlock_cache,
            on_event,
        }
    }
}

pub fn log_runtime_event(event: DownloadRuntimeEvent) {
    match event {
        DownloadRuntimeEvent::Cached { url, destination } => {
            log::debug!("Skipping unlock download for cached url='{url}' dest='{destination}'.");
        }
        DownloadRuntimeEvent::Duplicate { url, destination } => {
            log::debug!("Skipping duplicate unlock download url='{url}' dest='{destination}'.");
        }
        DownloadRuntimeEvent::NonZip { url, content_type } => {
            log::warn!("Attempted to download non-zip unlock from '{url}' ({content_type}).");
        }
    }
}

pub fn unlock_downloads_available(
    auto_download_unlocks: bool,
    groovestats_status: &ConnectionStatus,
) -> bool {
    auto_download_unlocks && matches!(groovestats_status, ConnectionStatus::Connected(_))
}

pub fn unlock_destination_roots(
    songs_dir: PathBuf,
    additional_roots: impl IntoIterator<Item = (PathBuf, bool)>,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    roots.push(songs_dir);
    roots.extend(
        additional_roots
            .into_iter()
            .filter_map(|(path, writable)| writable.then_some(path)),
    );
    roots
}

static RUNTIME_DOWNLOAD_STATE: LazyLock<Mutex<DownloadState>> =
    LazyLock::new(|| Mutex::new(DownloadState::default()));
static RUNTIME_NEXT_DOWNLOAD_ID: AtomicU64 = AtomicU64::new(1);

impl DownloadState {
    pub fn snapshots(&self) -> Vec<DownloadSnapshot> {
        self.entries
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

    pub fn completion_counts(&self) -> (usize, usize) {
        let total = self.entries.len();
        let finished = self.entries.iter().filter(|entry| entry.complete).count();
        (finished, total)
    }

    pub fn take_ready_song_reload_request(&mut self) -> Vec<PathBuf> {
        if self.ready_song_reload_dirs.is_empty()
            || self.entries.iter().any(|entry| !entry.complete)
        {
            return Vec::new();
        }
        std::mem::take(&mut self.ready_song_reload_dirs)
    }

    pub fn ensure_cache_loaded_with<F>(&mut self, load: F)
    where
        F: FnOnce() -> UnlockCache,
    {
        if self.cache_loaded {
            return;
        }
        self.unlock_cache = load();
        self.cache_loaded = true;
    }

    pub fn queue_download<F>(
        &mut self,
        url: &str,
        name: String,
        destination: String,
        next_id: F,
    ) -> QueueDownloadResult
    where
        F: FnOnce() -> u64,
    {
        if cache_has_destination(&self.unlock_cache, url, destination.as_str()) {
            return QueueDownloadResult::Cached;
        }
        if self
            .entries
            .iter()
            .any(|entry| entry.url == url && entry.destination == destination)
        {
            return QueueDownloadResult::Duplicate;
        }

        let id = next_id();
        self.entries.push(DownloadEntry {
            id,
            name,
            url: url.to_string(),
            destination,
            current_bytes: 0,
            total_bytes: 0,
            complete: false,
            error_message: None,
        });
        QueueDownloadResult::Queued(id)
    }

    pub fn mark_cache_success_with<F>(
        &mut self,
        url: &str,
        destination: &str,
        load: F,
    ) -> UnlockCache
    where
        F: FnOnce() -> UnlockCache,
    {
        self.ensure_cache_loaded_with(load);
        self.unlock_cache
            .entry(url.to_string())
            .or_default()
            .insert(destination.to_string(), true);
        self.unlock_cache.clone()
    }

    pub fn queue_ready_song_reload_dir(&mut self, path: PathBuf) {
        if self
            .ready_song_reload_dirs
            .iter()
            .any(|existing| existing == &path)
        {
            return;
        }
        self.ready_song_reload_dirs.push(path);
    }

    pub fn set_download_progress(&mut self, id: u64, current_bytes: u64, total_bytes: u64) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.current_bytes = current_bytes;
            entry.total_bytes = total_bytes;
        }
    }

    pub fn finish_download(&mut self, id: u64, error_message: Option<String>) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.complete = true;
            entry.error_message = error_message;
            if entry.total_bytes == 0 {
                entry.total_bytes = entry.current_bytes;
            }
        }
    }
}

pub fn runtime_snapshots() -> Vec<DownloadSnapshot> {
    RUNTIME_DOWNLOAD_STATE.lock().unwrap().snapshots()
}

pub fn runtime_completion_counts() -> (usize, usize) {
    RUNTIME_DOWNLOAD_STATE.lock().unwrap().completion_counts()
}

pub fn runtime_take_ready_song_reload_request() -> Vec<PathBuf> {
    RUNTIME_DOWNLOAD_STATE
        .lock()
        .unwrap()
        .take_ready_song_reload_request()
}

pub fn runtime_cache_snapshot(hooks: UnlockDownloadRuntimeHooks) -> UnlockCache {
    let mut state = RUNTIME_DOWNLOAD_STATE.lock().unwrap();
    state.ensure_cache_loaded_with(hooks.load_unlock_cache);
    state.unlock_cache.clone()
}

pub fn runtime_queue_event_unlock_download(
    hooks: UnlockDownloadRuntimeHooks,
    url: &str,
    unlock_name: &str,
    pack_name: &str,
) {
    let start = {
        let mut state = RUNTIME_DOWNLOAD_STATE.lock().unwrap();
        queue_event_unlock_download(
            &mut state,
            url,
            unlock_name,
            pack_name,
            hooks.load_unlock_cache,
            || RUNTIME_NEXT_DOWNLOAD_ID.fetch_add(1, Ordering::Relaxed),
        )
    };
    let download = match start {
        QueueEventUnlockDownloadResult::Queued(download) => download,
        QueueEventUnlockDownloadResult::EmptyUrl => return,
        QueueEventUnlockDownloadResult::Cached { destination } => {
            (hooks.on_event)(DownloadRuntimeEvent::Cached {
                url: url.to_string(),
                destination,
            });
            return;
        }
        QueueEventUnlockDownloadResult::Duplicate { destination } => {
            (hooks.on_event)(DownloadRuntimeEvent::Duplicate {
                url: url.to_string(),
                destination,
            });
            return;
        }
    };
    thread::spawn(move || {
        runtime_download_worker(hooks, download.id, download.url, download.destination);
    });
}

fn runtime_download_worker(
    hooks: UnlockDownloadRuntimeHooks,
    id: u64,
    url: String,
    destination: String,
) {
    let zip_path = (hooks.downloads_dir)().join(download_filename(id));
    let result = run_unlock_download_worker(
        url.clone(),
        destination,
        zip_path,
        &(hooks.unlock_destination_roots)(),
        |downloaded, total| runtime_set_download_progress(id, downloaded, total),
    );
    match result {
        UnlockDownloadWorkerResult::Success(success) => {
            runtime_mark_cache_success(hooks, success.url.as_str(), success.destination.as_str());
            runtime_queue_ready_song_reload_dir(success.destination_pack);
            runtime_set_download_progress(id, success.final_bytes, success.final_bytes);
            runtime_finish_download(id, None);
        }
        UnlockDownloadWorkerResult::Failure(failure) => {
            if let Some(content_type) = failure.not_zip_content_type {
                (hooks.on_event)(DownloadRuntimeEvent::NonZip { url, content_type });
            }
            runtime_finish_download(id, Some(failure.error_message));
        }
    }
}

fn runtime_mark_cache_success(hooks: UnlockDownloadRuntimeHooks, url: &str, destination: &str) {
    let cache_snapshot = {
        let mut state = RUNTIME_DOWNLOAD_STATE.lock().unwrap();
        state.mark_cache_success_with(url, destination, hooks.load_unlock_cache)
    };
    (hooks.write_unlock_cache)(&cache_snapshot);
}

fn runtime_queue_ready_song_reload_dir(path: PathBuf) {
    RUNTIME_DOWNLOAD_STATE
        .lock()
        .unwrap()
        .queue_ready_song_reload_dir(path);
}

fn runtime_set_download_progress(id: u64, current_bytes: u64, total_bytes: u64) {
    RUNTIME_DOWNLOAD_STATE
        .lock()
        .unwrap()
        .set_download_progress(id, current_bytes, total_bytes);
}

fn runtime_finish_download(id: u64, error_message: Option<String>) {
    RUNTIME_DOWNLOAD_STATE
        .lock()
        .unwrap()
        .finish_download(id, error_message);
}

pub fn queue_event_unlock_download<F, L>(
    state: &mut DownloadState,
    url: &str,
    unlock_name: &str,
    pack_name: &str,
    load_cache: L,
    next_id: F,
) -> QueueEventUnlockDownloadResult
where
    F: FnOnce() -> u64,
    L: FnOnce() -> UnlockCache,
{
    let url = url.trim();
    if url.is_empty() {
        return QueueEventUnlockDownloadResult::EmptyUrl;
    }
    let name = if unlock_name.trim().is_empty() {
        pack_name.trim().to_string()
    } else {
        unlock_name.trim().to_string()
    };
    let destination = sanitize_pack_name(pack_name);
    state.ensure_cache_loaded_with(load_cache);
    match state.queue_download(url, name, destination.clone(), next_id) {
        QueueDownloadResult::Queued(id) => {
            QueueEventUnlockDownloadResult::Queued(EventUnlockDownload {
                id,
                url: url.to_string(),
                destination,
            })
        }
        QueueDownloadResult::Cached => QueueEventUnlockDownloadResult::Cached { destination },
        QueueDownloadResult::Duplicate => QueueEventUnlockDownloadResult::Duplicate { destination },
    }
}

pub fn run_unlock_download_worker<F>(
    url: String,
    destination: String,
    zip_path: PathBuf,
    roots: &[PathBuf],
    report_progress: F,
) -> UnlockDownloadWorkerResult
where
    F: FnMut(u64, u64),
{
    if let Err(error) = download_zip_to_path(url.as_str(), &zip_path, report_progress) {
        return UnlockDownloadWorkerResult::Failure(download_failure(error));
    }

    let destination_pack = unlock_destination_pack(destination.as_str(), roots);
    if unzip_to_destination(&zip_path, &destination_pack).is_err() {
        return UnlockDownloadWorkerResult::Failure(UnlockDownloadFailure {
            error_message: "Failed to Unzip!".to_string(),
            not_zip_content_type: None,
        });
    }
    if let Err(error) = write_pack_ini_if_needed(&destination_pack, destination.as_str()) {
        return UnlockDownloadWorkerResult::Failure(UnlockDownloadFailure {
            error_message: error.to_string(),
            not_zip_content_type: None,
        });
    }
    UnlockDownloadWorkerResult::Success(UnlockDownloadSuccess {
        url,
        destination,
        destination_pack,
        final_bytes: file_len(&zip_path),
    })
}

pub fn unlock_destination_pack(destination: &str, roots: &[PathBuf]) -> PathBuf {
    let root_idx = choose_unlock_root(destination, roots).unwrap_or(0);
    roots[root_idx].join(destination)
}

fn download_failure(error: DownloadZipError) -> UnlockDownloadFailure {
    let not_zip_content_type = match &error {
        DownloadZipError::NotZip { content_type } => Some(content_type.clone()),
        _ => None,
    };
    UnlockDownloadFailure {
        error_message: error.to_string(),
        not_zip_content_type,
    }
}

#[derive(Serialize, Deserialize)]
pub struct UnlockCacheFile(pub UnlockCache);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadZipError {
    PrepareDir(String),
    Request(String),
    HttpStatus(u16),
    Io(String),
    NotZip { content_type: String },
}

impl Display for DownloadZipError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::PrepareDir(message) => write!(f, "Failed to prepare Downloads dir: {message}"),
            Self::Request(message) | Self::Io(message) => f.write_str(message),
            Self::HttpStatus(status) => write!(f, "Network Error {status}"),
            Self::NotZip { .. } => f.write_str("Download is not a Zip!"),
        }
    }
}

impl Error for DownloadZipError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadUnlockCacheError {
    Io(String),
    Decode(String),
}

impl Display for ReadUnlockCacheError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) | Self::Decode(message) => f.write_str(message),
        }
    }
}

impl Error for ReadUnlockCacheError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteUnlockCacheError {
    CreateDir(String),
    Encode(String),
    WriteTemp(String),
    Commit(String),
}

impl Display for WriteUnlockCacheError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateDir(message)
            | Self::Encode(message)
            | Self::WriteTemp(message)
            | Self::Commit(message) => f.write_str(message),
        }
    }
}

impl Error for WriteUnlockCacheError {}

pub fn download_zip_to_path<F>(
    url: &str,
    zip_path: &Path,
    mut report_progress: F,
) -> Result<(), DownloadZipError>
where
    F: FnMut(u64, u64),
{
    if let Some(parent) = zip_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| DownloadZipError::PrepareDir(error.to_string()))?;
    }

    let response = network::get_agent()
        .get(url)
        .call()
        .map_err(|error| DownloadZipError::Request(error.to_string()))?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(DownloadZipError::HttpStatus(status));
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
    report_progress(0, total_bytes);

    let mut file =
        File::create(zip_path).map_err(|error| DownloadZipError::Io(error.to_string()))?;
    let mut body = response.into_body();
    let mut reader = body.as_reader();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded = 0u64;
    loop {
        let read = reader
            .read(&mut buf)
            .map_err(|error| DownloadZipError::Io(error.to_string()))?;
        if read == 0 {
            break;
        }
        file.write_all(&buf[..read])
            .map_err(|error| DownloadZipError::Io(error.to_string()))?;
        downloaded = downloaded.saturating_add(read as u64);
        report_progress(downloaded, total_bytes);
    }
    report_progress(downloaded, total_bytes.max(downloaded));

    if content_type.as_str() != "application/zip" {
        return Err(DownloadZipError::NotZip { content_type });
    }

    Ok(())
}

pub fn sanitize_pack_name(raw: &str) -> String {
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

pub fn mime_token(value: &str) -> &str {
    value.split(';').next().unwrap_or("").trim()
}

pub fn cache_has_destination(cache: &UnlockCache, url: &str, destination: &str) -> bool {
    cache
        .get(url)
        .and_then(|packs| packs.get(destination))
        .copied()
        .unwrap_or(false)
}

pub fn cache_has_success(cache: &UnlockCache, url: &str) -> bool {
    cache
        .get(url)
        .is_some_and(|packs| packs.values().any(|success| *success))
}

pub fn choose_unlock_root(destination: &str, roots: &[impl AsRef<Path>]) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None;
    for (idx, root) in roots.iter().enumerate() {
        let Some(score) = unlock_root_score(root.as_ref(), destination) else {
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

pub fn unzip_to_destination(
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

pub fn write_pack_ini_if_needed(
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

pub fn read_unlock_cache_file(path: &Path) -> Result<UnlockCache, ReadUnlockCacheError> {
    let text =
        fs::read_to_string(path).map_err(|error| ReadUnlockCacheError::Io(error.to_string()))?;
    serde_json::from_str::<UnlockCacheFile>(&text)
        .map(|file| file.0)
        .map_err(|error| ReadUnlockCacheError::Decode(error.to_string()))
}

pub fn write_unlock_cache_file(
    path: &Path,
    cache: &UnlockCache,
) -> Result<(), WriteUnlockCacheError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| WriteUnlockCacheError::CreateDir(error.to_string()))?;
    }
    let text = serde_json::to_string(&UnlockCacheFile(cache.clone()))
        .map_err(|error| WriteUnlockCacheError::Encode(error.to_string()))?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).map_err(|error| WriteUnlockCacheError::WriteTemp(error.to_string()))?;
    fs::rename(&tmp, path).map_err(|error| {
        let _ = fs::remove_file(&tmp);
        WriteUnlockCacheError::Commit(error.to_string())
    })
}

pub fn download_filename(id: u64) -> String {
    format!("{id:016x}.zip")
}

pub fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

pub fn itl_unlock_pack_ini_content(pack_name: &str) -> Option<String> {
    let lower = pack_name.to_ascii_lowercase();
    if !lower.contains(&format!("itl online {ITL_UNLOCK_PACK_YEAR} unlocks")) {
        return None;
    }
    Some(format!(
        "[Group]\nVersion=1\nDisplayTitle={pack_name}\nTranslitTitle={pack_name}\nSortTitle={pack_name}\nSeries=ITL Online\nYear={ITL_UNLOCK_PACK_YEAR}\nBanner=\nSyncOffset=NULL\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn sanitize_pack_name_strips_invalid_chars() {
        assert_eq!(sanitize_pack_name("ITL/Unlocks:*?"), "ITLUnlocks");
    }

    #[test]
    fn sanitize_pack_name_avoids_windows_reserved_names() {
        assert_eq!(sanitize_pack_name("CON"), " CON ");
    }

    #[test]
    fn sanitize_pack_name_falls_back_when_empty() {
        assert_eq!(sanitize_pack_name("///"), "Unlocks");
    }

    #[test]
    fn mime_token_strips_parameters() {
        assert_eq!(
            mime_token("application/zip; charset=binary"),
            "application/zip"
        );
    }

    #[test]
    fn unlock_downloads_require_enabled_connected_groovestats() {
        let connected = ConnectionStatus::Connected(crate::groovestats::Services {
            get_scores: true,
            leaderboard: true,
            auto_submit: true,
        });
        assert!(unlock_downloads_available(true, &connected));
        assert!(!unlock_downloads_available(false, &connected));
        assert!(!unlock_downloads_available(
            true,
            &ConnectionStatus::Pending
        ));
    }

    #[test]
    fn unlock_destination_roots_keep_primary_and_writable_additional() {
        assert_eq!(
            unlock_destination_roots(
                PathBuf::from("Songs"),
                [
                    (PathBuf::from("Writable"), true),
                    (PathBuf::from("ReadOnly"), false),
                ],
            ),
            vec![PathBuf::from("Songs"), PathBuf::from("Writable")]
        );
    }

    #[test]
    fn cache_has_destination_reads_nested_success_flag() {
        let mut cache = UnlockCache::new();
        cache
            .entry("https://example.com/unlock.zip".to_string())
            .or_default()
            .insert("ITL Unlocks".to_string(), true);

        assert!(cache_has_destination(
            &cache,
            "https://example.com/unlock.zip",
            "ITL Unlocks"
        ));
        assert!(!cache_has_destination(
            &cache,
            "https://example.com/unlock.zip",
            "Other Pack"
        ));
        assert!(cache_has_success(&cache, "https://example.com/unlock.zip"));
        assert!(!cache_has_success(
            &cache,
            "https://example.com/missing.zip"
        ));
    }

    #[test]
    fn download_state_queues_snapshots_and_finishes_entries() {
        let mut state = DownloadState::default();
        state.ensure_cache_loaded_with(UnlockCache::new);

        assert_eq!(
            state.queue_download(
                "https://example.com/unlock.zip",
                "Unlock".to_string(),
                "ITL Unlocks".to_string(),
                || 7,
            ),
            QueueDownloadResult::Queued(7)
        );
        assert_eq!(state.completion_counts(), (0, 1));

        state.set_download_progress(7, 10, 30);
        let snapshots = state.snapshots();
        assert_eq!(snapshots.len(), 1);
        let snapshot = &snapshots[0];
        assert_eq!(snapshot.name, "Unlock");
        assert_eq!(snapshot.current_bytes, 10);
        assert_eq!(snapshot.total_bytes, 30);
        assert!(!snapshot.complete);
        assert_eq!(snapshot.error_message, None);

        state.finish_download(7, Some("failed".to_string()));
        let snapshots = state.snapshots();
        let snapshot = &snapshots[0];
        assert!(snapshot.complete);
        assert_eq!(snapshot.error_message.as_deref(), Some("failed"));
        assert_eq!(state.completion_counts(), (1, 1));
    }

    #[test]
    fn download_state_skips_cached_and_duplicate_entries() {
        let mut cache = UnlockCache::new();
        cache
            .entry("https://example.com/cached.zip".to_string())
            .or_default()
            .insert("ITL Unlocks".to_string(), true);
        let mut state = DownloadState::default();
        state.ensure_cache_loaded_with(|| cache);

        assert_eq!(
            state.queue_download(
                "https://example.com/cached.zip",
                "Cached".to_string(),
                "ITL Unlocks".to_string(),
                || 1,
            ),
            QueueDownloadResult::Cached
        );
        assert_eq!(
            state.queue_download(
                "https://example.com/unlock.zip",
                "Unlock".to_string(),
                "ITL Unlocks".to_string(),
                || 2,
            ),
            QueueDownloadResult::Queued(2)
        );
        assert_eq!(
            state.queue_download(
                "https://example.com/unlock.zip",
                "Unlock".to_string(),
                "ITL Unlocks".to_string(),
                || 3,
            ),
            QueueDownloadResult::Duplicate
        );
        assert_eq!(state.completion_counts(), (0, 1));
    }

    #[test]
    fn event_unlock_queue_trims_url_and_falls_back_to_pack_name() {
        let mut state = DownloadState::default();
        let result = queue_event_unlock_download(
            &mut state,
            " https://example.com/unlock.zip ",
            "",
            "ITL/Unlocks:*?",
            UnlockCache::new,
            || 9,
        );

        assert_eq!(
            result,
            QueueEventUnlockDownloadResult::Queued(EventUnlockDownload {
                id: 9,
                url: "https://example.com/unlock.zip".to_string(),
                destination: "ITLUnlocks".to_string(),
            })
        );
        let snapshots = state.snapshots();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].name, "ITL/Unlocks:*?");
    }

    #[test]
    fn event_unlock_queue_reports_empty_cached_and_duplicate() {
        let mut cache = UnlockCache::new();
        cache
            .entry("https://example.com/cached.zip".to_string())
            .or_default()
            .insert("ITL Unlocks".to_string(), true);
        let mut state = DownloadState::default();

        assert_eq!(
            queue_event_unlock_download(
                &mut state,
                " ",
                "Unlock",
                "ITL Unlocks",
                UnlockCache::new,
                || 1
            ),
            QueueEventUnlockDownloadResult::EmptyUrl
        );
        assert_eq!(
            queue_event_unlock_download(
                &mut state,
                "https://example.com/cached.zip",
                "Cached",
                "ITL Unlocks",
                || cache,
                || 2,
            ),
            QueueEventUnlockDownloadResult::Cached {
                destination: "ITL Unlocks".to_string(),
            }
        );
        assert_eq!(
            queue_event_unlock_download(
                &mut state,
                "https://example.com/unlock.zip",
                "Unlock",
                "ITL Unlocks",
                UnlockCache::new,
                || 3,
            ),
            QueueEventUnlockDownloadResult::Queued(EventUnlockDownload {
                id: 3,
                url: "https://example.com/unlock.zip".to_string(),
                destination: "ITL Unlocks".to_string(),
            })
        );
        assert_eq!(
            queue_event_unlock_download(
                &mut state,
                "https://example.com/unlock.zip",
                "Unlock",
                "ITL Unlocks",
                UnlockCache::new,
                || 4,
            ),
            QueueEventUnlockDownloadResult::Duplicate {
                destination: "ITL Unlocks".to_string(),
            }
        );
    }

    #[test]
    fn download_state_marks_cache_success_and_waits_for_downloads() {
        let mut state = DownloadState::default();
        state.ensure_cache_loaded_with(UnlockCache::new);
        assert_eq!(
            state.queue_download(
                "https://example.com/unlock.zip",
                "Unlock".to_string(),
                "ITL Unlocks".to_string(),
                || 1,
            ),
            QueueDownloadResult::Queued(1)
        );

        let reload_dir = PathBuf::from("Songs/ITL Unlocks");
        state.queue_ready_song_reload_dir(reload_dir.clone());
        state.queue_ready_song_reload_dir(reload_dir.clone());
        assert!(state.take_ready_song_reload_request().is_empty());

        let cache =
            state.mark_cache_success_with("https://example.com/unlock.zip", "ITL Unlocks", || {
                panic!("cache should already be loaded")
            });
        assert!(cache_has_destination(
            &cache,
            "https://example.com/unlock.zip",
            "ITL Unlocks"
        ));

        state.finish_download(1, None);
        assert_eq!(state.take_ready_song_reload_request(), vec![reload_dir]);
        assert!(state.take_ready_song_reload_request().is_empty());
    }

    #[test]
    fn itl_unlock_pack_ini_content_matches_pack_ini_shape() {
        let content =
            itl_unlock_pack_ini_content("ITL Online 2026 Unlocks").expect("pack ini content");
        assert!(content.contains("DisplayTitle=ITL Online 2026 Unlocks"));
        assert!(content.contains("Series=ITL Online"));
        assert!(content.contains("Year=2026"));
    }

    #[test]
    fn itl_unlock_pack_ini_content_skips_other_packs() {
        assert!(itl_unlock_pack_ini_content("Other Pack").is_none());
    }

    #[test]
    fn choose_unlock_root_prefers_last_writable_root_for_new_pack() {
        let roots = vec!["Songs", "ExtraSongsA", "ExtraSongsB"];

        assert_eq!(
            choose_unlock_root("Stamina RPG 10 Unlocks", &roots),
            Some(2)
        );
    }

    #[test]
    fn choose_unlock_root_keeps_existing_pack_location() {
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
    fn choose_unlock_root_uses_existing_additional_pack() {
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
    fn unlock_destination_pack_uses_existing_root_selection() {
        let root = temp_root("worker-destination");
        let primary = root.join("songs");
        let extra = root.join("extra");
        fs::create_dir_all(&primary).expect("create primary song root");
        fs::create_dir_all(extra.join("ITL Online 2026 Unlocks")).expect("create extra pack");
        let roots = vec![primary, extra.clone()];

        assert_eq!(
            unlock_destination_pack("ITL Online 2026 Unlocks", &roots),
            extra.join("ITL Online 2026 Unlocks")
        );
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn choose_unlock_root_skips_file_candidates() {
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

    #[test]
    fn download_zip_error_preserves_download_messages() {
        assert_eq!(
            DownloadZipError::PrepareDir("denied".to_string()).to_string(),
            "Failed to prepare Downloads dir: denied"
        );
        assert_eq!(
            DownloadZipError::HttpStatus(404).to_string(),
            "Network Error 404"
        );
        assert_eq!(
            DownloadZipError::NotZip {
                content_type: "text/html".to_string()
            }
            .to_string(),
            "Download is not a Zip!"
        );
    }
}
