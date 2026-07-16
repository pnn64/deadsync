use deadsync_net as network;
use std::collections::HashSet;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::thread;
use zip::ZipArchive;

pub const WEBSITE_URL: &str = "https://stepmaniaonline.net/";
const CATALOG_URL: &str = "https://stepmaniaonline.net/api/packs";
const DOWNLOAD_URL_PREFIX: &str = "https://stepmaniaonline.net/download/pack";
const USER_AGENT: &str = "DeadSync StepManiaOnline Pack Browser/1.0";

const MAX_CATALOG_BYTES: usize = 8 * 1024 * 1024;
const MAX_CATALOG_ROWS: usize = 20_000;
const MAX_CATALOG_LINE_BYTES: usize = 2 * 1024;
const MAX_INSTALLS: usize = 64;
const DOWNLOAD_QUEUE_CAPACITY: usize = 8;
const PROGRESS_STEP_BYTES: u64 = 512 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 200_000;
const MAX_ARCHIVE_PATH_BYTES: usize = 768;
const MAX_UNCOMPRESSED_BYTES: u64 = 128 * 1024 * 1024 * 1024;
const UNCOMPRESSED_HEADROOM_BYTES: u64 = 256 * 1024 * 1024;
const DESTINATION_MAX_CHARS: usize = 160;
const WINDOWS_RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CatalogPhase {
    #[default]
    Idle,
    Loading,
    Ready,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallPhase {
    Queued,
    Downloading,
    Extracting,
    Installed,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackInfo {
    pub id: u64,
    pub name: String,
    pub song_count: u32,
    pub size_bytes: u64,
    pub sync: Option<String>,
    pub pack_type: Option<String>,
    pub substyle: Option<String>,
    pub min_version: Option<String>,
    normalized_name: String,
    search_text: String,
}

impl PackInfo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u64,
        name: String,
        song_count: u32,
        size_bytes: u64,
        sync: Option<String>,
        pack_type: Option<String>,
        substyle: Option<String>,
        min_version: Option<String>,
    ) -> Self {
        let normalized_name = name.to_lowercase();
        let mut search_text = String::with_capacity(normalized_name.len() + 80);
        search_text.push_str(normalized_name.as_str());
        for value in [&sync, &pack_type, &substyle, &min_version]
            .into_iter()
            .flatten()
        {
            search_text.push(' ');
            search_text.push_str(value.to_lowercase().as_str());
        }
        search_text.push(' ');
        search_text.push_str(id.to_string().as_str());
        Self {
            id,
            name,
            song_count,
            size_bytes,
            sync,
            pack_type,
            substyle,
            min_version,
            normalized_name,
            search_text,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallSnapshot {
    pub pack_id: u64,
    pub phase: InstallPhase,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub phase: CatalogPhase,
    pub catalog: Arc<[PackInfo]>,
    pub revision: u64,
    pub message: Option<String>,
    pub installs: Vec<InstallSnapshot>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            phase: CatalogPhase::Idle,
            catalog: Arc::from(Vec::<PackInfo>::new()),
            revision: 0,
            message: None,
            installs: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepManiaOnlineError {
    Network(network::NetworkError),
    Catalog(String),
    Io {
        action: &'static str,
        message: String,
    },
    Archive(String),
    AlreadyInstalled(String),
}

impl Display for StepManiaOnlineError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(error) => write!(f, "StepManiaOnline request failed: {error}"),
            Self::Catalog(message) => write!(f, "Invalid StepManiaOnline catalog: {message}"),
            Self::Io { action, message } => write!(f, "Failed to {action}: {message}"),
            Self::Archive(message) => write!(f, "Unsafe or invalid pack archive: {message}"),
            Self::AlreadyInstalled(name) => write!(f, "'{name}' is already installed"),
        }
    }
}

impl std::error::Error for StepManiaOnlineError {}

impl From<network::NetworkError> for StepManiaOnlineError {
    fn from(error: network::NetworkError) -> Self {
        Self::Network(error)
    }
}

/// Returns matching catalog indices in relevance order. A blank query keeps
/// the server's natural catalog order. Search work is bounded by the catalog
/// cap and uses strings normalized once when the catalog is parsed.
pub fn search_catalog(catalog: &[PackInfo], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return (0..catalog.len()).collect();
    }
    let tokens: Vec<&str> = query.split_whitespace().collect();
    let mut buckets: [Vec<usize>; 5] = std::array::from_fn(|_| Vec::new());
    for (idx, pack) in catalog.iter().enumerate() {
        let rank = if pack.normalized_name == query {
            Some(0)
        } else if pack.normalized_name.starts_with(query.as_str()) {
            Some(1)
        } else if pack.normalized_name.contains(query.as_str()) {
            Some(2)
        } else if tokens
            .iter()
            .all(|token| pack.normalized_name.contains(token))
        {
            Some(3)
        } else if tokens.iter().all(|token| pack.search_text.contains(token)) {
            Some(4)
        } else {
            None
        };
        if let Some(rank) = rank {
            buckets[rank].push(idx);
        }
    }
    buckets.into_iter().flatten().collect()
}

pub fn parse_catalog(text: &str) -> Result<Vec<PackInfo>, StepManiaOnlineError> {
    if text.len() > MAX_CATALOG_BYTES {
        return Err(StepManiaOnlineError::Catalog(format!(
            "response exceeds {MAX_CATALOG_BYTES} bytes"
        )));
    }
    let mut lines = text.lines();
    let header = lines
        .next()
        .map(|line| line.trim_start_matches('\u{feff}').trim_end_matches('\r'))
        .ok_or_else(|| StepManiaOnlineError::Catalog("response is empty".to_string()))?;
    if header != "ID, Pack Name, Song Count, Size, Sync, PackType, Substyle, Min Version" {
        return Err(StepManiaOnlineError::Catalog(
            "unexpected header".to_string(),
        ));
    }

    let mut packs = Vec::with_capacity(4096);
    let mut ids = HashSet::with_capacity(4096);
    for (idx, raw_line) in lines.enumerate() {
        let line_number = idx + 2;
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        if packs.len() == MAX_CATALOG_ROWS {
            return Err(StepManiaOnlineError::Catalog(format!(
                "catalog exceeds {MAX_CATALOG_ROWS} packs"
            )));
        }
        if line.len() > MAX_CATALOG_LINE_BYTES {
            return Err(catalog_line_error(line_number, "row is too long"));
        }
        let pack = parse_catalog_line(line, line_number)?;
        if !ids.insert(pack.id) {
            return Err(catalog_line_error(line_number, "duplicate pack ID"));
        }
        packs.push(pack);
    }
    if packs.is_empty() {
        return Err(StepManiaOnlineError::Catalog(
            "catalog contains no packs".to_string(),
        ));
    }
    Ok(packs)
}

fn parse_catalog_line(line: &str, line_number: usize) -> Result<PackInfo, StepManiaOnlineError> {
    // The upstream endpoint wraps names in quotes but does not CSV-escape
    // embedded quotes. Its six trailing columns have fixed, comma-free value
    // domains, so parsing from the right preserves names containing commas.
    let fields: Vec<&str> = line.rsplitn(7, ", ").collect();
    if fields.len() != 7 {
        return Err(catalog_line_error(line_number, "expected eight columns"));
    }
    let (id_text, quoted_name) = fields[6]
        .split_once(", ")
        .ok_or_else(|| catalog_line_error(line_number, "missing pack name"))?;
    let name = quoted_name
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| catalog_line_error(line_number, "pack name is not quoted"))?;
    if name.trim().is_empty() {
        return Err(catalog_line_error(line_number, "pack name is empty"));
    }
    let id = parse_catalog_number(id_text, line_number, "ID")?;
    let song_count = parse_catalog_number(fields[5], line_number, "song count")?;
    let size_bytes = parse_catalog_number(fields[4], line_number, "size")?;
    Ok(PackInfo::new(
        id,
        name.to_string(),
        song_count,
        size_bytes,
        optional_catalog_value(fields[3]),
        optional_catalog_value(fields[2]),
        optional_catalog_value(fields[1]),
        optional_catalog_value(fields[0]),
    ))
}

fn parse_catalog_number<T>(
    text: &str,
    line_number: usize,
    field: &str,
) -> Result<T, StepManiaOnlineError>
where
    T: std::str::FromStr,
{
    text.parse()
        .map_err(|_| catalog_line_error(line_number, format!("invalid {field}").as_str()))
}

fn optional_catalog_value(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("none") || text.eq_ignore_ascii_case("null") {
        None
    } else {
        Some(text.to_string())
    }
}

fn catalog_line_error(line_number: usize, message: &str) -> StepManiaOnlineError {
    StepManiaOnlineError::Catalog(format!("line {line_number}: {message}"))
}

/// Runtime/cache contract:
///
/// - Owner: StepManiaOnline catalog and install workers; UI only snapshots it.
/// - Thread safety: one mutex guards immutable `Arc` snapshots and ready paths.
/// - Lifetime: process session; no disk catalog cache is maintained.
/// - Capacity: 20,000 catalog rows, 64 install records, eight queued downloads,
///   and at most 64 targeted reload paths before they coalesce to `songs/`.
/// - Warmup: asynchronously when the Options pack browser first opens.
/// - Miss behavior: an idle catalog starts one background HTTP request; gameplay
///   never calls this runtime and never performs network or filesystem work.
/// - Eviction: only the oldest terminal install record is evicted at the cap;
///   active records are never evicted and catalog rows live for the session.
/// - Destruction: `Arc` data is dropped by whichever non-audio thread releases
///   the last snapshot; temporary archives/staging dirs are worker-cleaned.
/// - Instrumentation: catalog loads, installs, failures, and history evictions
///   are logged; UI snapshots expose phases, byte progress, and messages.
/// - Worst frame cost: one mutex acquisition plus an `Arc` clone. Install list
///   cloning is worker-only and bounded to 64 records.
struct RuntimeState {
    generation: u64,
    snapshot: Arc<Snapshot>,
    ready_song_dirs: Vec<PathBuf>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            generation: 0,
            snapshot: Arc::new(Snapshot::default()),
            ready_song_dirs: Vec::new(),
        }
    }
}

#[derive(Clone)]
struct DownloadJob {
    pack: PackInfo,
    songs_root: PathBuf,
}

static RUNTIME: LazyLock<Mutex<RuntimeState>> =
    LazyLock::new(|| Mutex::new(RuntimeState::default()));
static DOWNLOAD_QUEUE: LazyLock<Result<SyncSender<DownloadJob>, String>> =
    LazyLock::new(start_download_worker);

fn lock_runtime() -> MutexGuard<'static, RuntimeState> {
    RUNTIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn runtime_snapshot() -> Arc<Snapshot> {
    Arc::clone(&lock_runtime().snapshot)
}

pub fn runtime_ensure_catalog() {
    start_catalog_request(false);
}

pub fn runtime_refresh_catalog() {
    start_catalog_request(true);
}

pub fn runtime_queue_download(pack_id: u64, songs_root: PathBuf) -> Result<(), String> {
    if songs_root.as_os_str().is_empty() {
        return Err("Songs directory is empty.".to_string());
    }
    let sender = DOWNLOAD_QUEUE
        .as_ref()
        .map_err(|error| (*error).clone())?
        .clone();
    let pack = {
        let mut runtime = lock_runtime();
        let pack = runtime
            .snapshot
            .catalog
            .iter()
            .find(|pack| pack.id == pack_id)
            .cloned()
            .ok_or_else(|| format!("Pack {pack_id} is not in the current catalog."))?;
        queue_install_snapshot(&mut runtime, &pack)?;
        pack
    };
    let job = DownloadJob { pack, songs_root };
    match sender.try_send(job) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(job)) => {
            let message = format!(
                "The download queue is full (maximum {DOWNLOAD_QUEUE_CAPACITY} waiting packs)."
            );
            set_install_error(job.pack.id, message.clone());
            Err(message)
        }
        Err(TrySendError::Disconnected(job)) => {
            let message = "The pack download worker stopped unexpectedly.".to_string();
            set_install_error(job.pack.id, message.clone());
            Err(message)
        }
    }
}

pub fn runtime_take_ready_song_dirs() -> Vec<PathBuf> {
    std::mem::take(&mut lock_runtime().ready_song_dirs)
}

fn start_catalog_request(force: bool) {
    let generation = {
        let mut runtime = lock_runtime();
        if runtime.snapshot.phase == CatalogPhase::Loading
            || !force && runtime.snapshot.phase == CatalogPhase::Ready
        {
            return;
        }
        runtime.generation = runtime.generation.wrapping_add(1);
        let generation = runtime.generation;
        let mut snapshot = (*runtime.snapshot).clone();
        snapshot.phase = CatalogPhase::Loading;
        snapshot.message = Some(if snapshot.catalog.is_empty() {
            "Loading the StepManiaOnline pack catalog...".to_string()
        } else {
            "Refreshing the StepManiaOnline pack catalog...".to_string()
        });
        runtime.snapshot = Arc::new(snapshot);
        generation
    };

    let spawn = thread::Builder::new()
        .name("smo-catalog".to_string())
        .spawn(move || finish_catalog_request(generation, fetch_catalog()));
    if let Err(error) = spawn {
        finish_catalog_request(
            generation,
            Err(StepManiaOnlineError::Io {
                action: "start the catalog worker",
                message: error.to_string(),
            }),
        );
    }
}

fn fetch_catalog() -> Result<Vec<PackInfo>, StepManiaOnlineError> {
    let response = network::get_agent()
        .get(CATALOG_URL)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(network::error_from_ureq)?;
    let text = network::read_text_body_bounded(response, MAX_CATALOG_BYTES)?;
    parse_catalog(text.as_str())
}

fn finish_catalog_request(generation: u64, result: Result<Vec<PackInfo>, StepManiaOnlineError>) {
    let mut runtime = lock_runtime();
    if runtime.generation != generation {
        log::debug!("Discarding stale StepManiaOnline catalog generation {generation}.");
        return;
    }
    let mut snapshot = (*runtime.snapshot).clone();
    match result {
        Ok(packs) => {
            let count = packs.len();
            snapshot.phase = CatalogPhase::Ready;
            snapshot.catalog = Arc::from(packs);
            snapshot.revision = snapshot.revision.wrapping_add(1);
            snapshot.message = None;
            log::info!("Loaded {count} StepManiaOnline packs.");
        }
        Err(error) => {
            log::warn!("StepManiaOnline catalog request failed: {error}");
            snapshot.phase = CatalogPhase::Error;
            snapshot.message = Some(error.to_string());
        }
    }
    runtime.snapshot = Arc::new(snapshot);
}

fn start_download_worker() -> Result<SyncSender<DownloadJob>, String> {
    let (sender, receiver) = sync_channel::<DownloadJob>(DOWNLOAD_QUEUE_CAPACITY);
    thread::Builder::new()
        .name("smo-downloads".to_string())
        .spawn(move || {
            while let Ok(job) = receiver.recv() {
                run_download_job(job);
            }
        })
        .map_err(|error| format!("Failed to start the pack download worker: {error}"))?;
    Ok(sender)
}

fn queue_install_snapshot(runtime: &mut RuntimeState, pack: &PackInfo) -> Result<(), String> {
    let mut snapshot = (*runtime.snapshot).clone();
    if let Some(install) = snapshot
        .installs
        .iter_mut()
        .find(|install| install.pack_id == pack.id)
    {
        match install.phase {
            InstallPhase::Queued | InstallPhase::Downloading | InstallPhase::Extracting => {
                return Err(format!("'{}' is already queued.", pack.name));
            }
            InstallPhase::Installed => {
                return Err(format!(
                    "'{}' was already installed this session.",
                    pack.name
                ));
            }
            InstallPhase::Error => {
                *install = queued_install(pack);
                runtime.snapshot = Arc::new(snapshot);
                return Ok(());
            }
        }
    }
    if snapshot.installs.len() == MAX_INSTALLS {
        let terminal = snapshot.installs.iter().position(|install| {
            matches!(install.phase, InstallPhase::Installed | InstallPhase::Error)
        });
        let Some(index) = terminal else {
            return Err("Too many pack installs are active.".to_string());
        };
        let evicted = snapshot.installs.remove(index);
        log::debug!(
            "Evicted terminal StepManiaOnline install history for pack {}.",
            evicted.pack_id
        );
    }
    snapshot.installs.push(queued_install(pack));
    runtime.snapshot = Arc::new(snapshot);
    Ok(())
}

fn queued_install(pack: &PackInfo) -> InstallSnapshot {
    InstallSnapshot {
        pack_id: pack.id,
        phase: InstallPhase::Queued,
        downloaded_bytes: 0,
        total_bytes: pack.size_bytes,
        message: Some("Waiting for the download slot...".to_string()),
    }
}

fn run_download_job(job: DownloadJob) {
    set_install_phase(
        job.pack.id,
        InstallPhase::Downloading,
        Some("Connecting to StepManiaOnline...".to_string()),
    );
    match install_pack(&job) {
        Ok(destination) => {
            let mut runtime = lock_runtime();
            queue_ready_dir(&mut runtime, destination, &job.songs_root);
            update_install(&mut runtime, job.pack.id, |install| {
                install.phase = InstallPhase::Installed;
                install.downloaded_bytes = install.total_bytes.max(install.downloaded_bytes);
                install.message = Some("Installed. Song indexing is ready.".to_string());
            });
            log::info!("Installed StepManiaOnline pack '{}'.", job.pack.name);
        }
        Err(error) => {
            log::warn!(
                "Failed to install StepManiaOnline pack '{}': {error}",
                job.pack.name
            );
            set_install_error(job.pack.id, error.to_string());
        }
    }
}

fn queue_ready_dir(runtime: &mut RuntimeState, destination: PathBuf, songs_root: &Path) {
    if runtime
        .ready_song_dirs
        .iter()
        .any(|queued| queued == &destination || queued == songs_root)
    {
        return;
    }
    if runtime.ready_song_dirs.len() == MAX_INSTALLS {
        runtime.ready_song_dirs.clear();
        runtime.ready_song_dirs.push(songs_root.to_path_buf());
    } else {
        runtime.ready_song_dirs.push(destination);
    }
}

fn set_install_phase(pack_id: u64, phase: InstallPhase, message: Option<String>) {
    let mut runtime = lock_runtime();
    update_install(&mut runtime, pack_id, |install| {
        install.phase = phase;
        install.message = message;
    });
}

fn set_install_progress(pack_id: u64, downloaded_bytes: u64, total_bytes: u64) {
    let mut runtime = lock_runtime();
    update_install(&mut runtime, pack_id, |install| {
        install.phase = InstallPhase::Downloading;
        install.downloaded_bytes = downloaded_bytes;
        install.total_bytes = total_bytes;
        install.message = Some("Downloading pack archive...".to_string());
    });
}

fn set_install_error(pack_id: u64, message: String) {
    let mut runtime = lock_runtime();
    update_install(&mut runtime, pack_id, |install| {
        install.phase = InstallPhase::Error;
        install.message = Some(message);
    });
}

fn update_install(
    runtime: &mut RuntimeState,
    pack_id: u64,
    update: impl FnOnce(&mut InstallSnapshot),
) {
    let mut snapshot = (*runtime.snapshot).clone();
    if let Some(install) = snapshot
        .installs
        .iter_mut()
        .find(|install| install.pack_id == pack_id)
    {
        update(install);
        runtime.snapshot = Arc::new(snapshot);
    }
}

fn install_pack(job: &DownloadJob) -> Result<PathBuf, StepManiaOnlineError> {
    fs::create_dir_all(&job.songs_root)
        .map_err(|error| io_error("create the Songs directory", error))?;
    let destination = choose_destination(&job.songs_root, &job.pack)?;
    let downloads_dir = deadlib_platform::dirs::app_dirs().downloads_dir();
    fs::create_dir_all(&downloads_dir)
        .map_err(|error| io_error("create the Downloads directory", error))?;
    let archive_path = downloads_dir.join(format!(".deadsync-smo-{}.part.zip", job.pack.id));
    let staging = job
        .songs_root
        .join(format!(".deadsync-smo-{}.part", job.pack.id));
    remove_temp_path(&archive_path)?;
    remove_temp_path(&staging)?;

    let result = (|| {
        download_archive(&job.pack, &archive_path)?;
        set_install_phase(
            job.pack.id,
            InstallPhase::Extracting,
            Some("Verifying and extracting the pack...".to_string()),
        );
        extract_archive(&archive_path, &staging, job.pack.size_bytes)?;
        if child_exists_case_insensitive(
            &job.songs_root,
            destination.file_name().unwrap_or_default(),
        )? {
            return Err(StepManiaOnlineError::AlreadyInstalled(
                destination
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ));
        }
        fs::rename(&staging, &destination)
            .map_err(|error| io_error("commit the installed pack", error))?;
        Ok(destination)
    })();

    if let Err(error) = remove_temp_path(&archive_path) {
        log::warn!("Could not remove StepManiaOnline temporary archive: {error}");
    }
    if result.is_err()
        && let Err(error) = remove_temp_path(&staging)
    {
        log::warn!("Could not remove StepManiaOnline staging directory: {error}");
    }
    result
}

fn download_archive(pack: &PackInfo, archive_path: &Path) -> Result<(), StepManiaOnlineError> {
    if pack.size_bytes == 0 || pack.size_bytes > MAX_ARCHIVE_BYTES {
        return Err(StepManiaOnlineError::Archive(format!(
            "advertised size {} is outside the supported range",
            pack.size_bytes
        )));
    }
    let url = format!("{DOWNLOAD_URL_PREFIX}/{}/", pack.id);
    let response = network::get_streaming_agent()
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(network::error_from_ureq)?;
    let content_length = response
        .headers()
        .get("Content-Length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if content_length.is_some_and(|length| length != pack.size_bytes) {
        return Err(StepManiaOnlineError::Archive(format!(
            "server reported {} bytes, catalog expected {}",
            content_length.unwrap_or_default(),
            pack.size_bytes
        )));
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(archive_path)
        .map_err(|error| io_error("create the temporary archive", error))?;
    let mut body = response.into_body();
    let mut reader = body.as_reader();
    let mut buffer = [0u8; 64 * 1024];
    let mut downloaded = 0u64;
    let mut next_report = 0u64;
    set_install_progress(pack.id, 0, pack.size_bytes);
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| io_error("read the pack download", error))?;
        if read == 0 {
            break;
        }
        let next = downloaded.saturating_add(read as u64);
        if next > pack.size_bytes || next > MAX_ARCHIVE_BYTES {
            return Err(StepManiaOnlineError::Archive(
                "download exceeded its advertised size".to_string(),
            ));
        }
        file.write_all(&buffer[..read])
            .map_err(|error| io_error("write the temporary archive", error))?;
        downloaded = next;
        if downloaded >= next_report {
            set_install_progress(pack.id, downloaded, pack.size_bytes);
            next_report = downloaded.saturating_add(PROGRESS_STEP_BYTES);
        }
    }
    file.flush()
        .map_err(|error| io_error("flush the temporary archive", error))?;
    if downloaded != pack.size_bytes {
        return Err(StepManiaOnlineError::Archive(format!(
            "downloaded {downloaded} bytes, catalog expected {}",
            pack.size_bytes
        )));
    }
    set_install_progress(pack.id, downloaded, pack.size_bytes);
    Ok(())
}

#[derive(Debug)]
struct ArchivePlan {
    prefix: String,
}

fn extract_archive(
    archive_path: &Path,
    staging: &Path,
    archive_bytes: u64,
) -> Result<(), StepManiaOnlineError> {
    let plan = inspect_archive(archive_path, archive_bytes)?;
    fs::create_dir(staging).map_err(|error| io_error("create the staging directory", error))?;
    let file =
        File::open(archive_path).map_err(|error| io_error("open the pack archive", error))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| StepManiaOnlineError::Archive(error.to_string()))?;
    for idx in 0..archive.len() {
        let mut entry = archive
            .by_index(idx)
            .map_err(|error| StepManiaOnlineError::Archive(error.to_string()))?;
        let parts = portable_archive_parts(entry.name())?;
        if parts.first().is_none_or(|prefix| prefix != &plan.prefix) {
            return Err(StepManiaOnlineError::Archive(
                "archive roots changed while extracting".to_string(),
            ));
        }
        let relative = &parts[1..];
        if relative.is_empty() {
            continue;
        }
        let output = relative
            .iter()
            .fold(staging.to_path_buf(), |path, part| path.join(part));
        let is_dir = entry.is_dir() || entry.name().ends_with('\\');
        if is_dir {
            fs::create_dir_all(&output)
                .map_err(|error| io_error("create an extracted directory", error))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error("create an extracted directory", error))?;
        }
        let mut output_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
            .map_err(|error| io_error("create an extracted file", error))?;
        let expected = entry.size();
        let mut limited = (&mut entry).take(expected.saturating_add(1));
        let copied = std::io::copy(&mut limited, &mut output_file)
            .map_err(|error| io_error("extract a pack file", error))?;
        if copied != expected {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry '{}' produced {copied} bytes, expected {expected}",
                entry.name()
            )));
        }
    }
    Ok(())
}

fn inspect_archive(
    archive_path: &Path,
    archive_bytes: u64,
) -> Result<ArchivePlan, StepManiaOnlineError> {
    let file =
        File::open(archive_path).map_err(|error| io_error("open the pack archive", error))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| StepManiaOnlineError::Archive(error.to_string()))?;
    if archive.is_empty() {
        return Err(StepManiaOnlineError::Archive(
            "archive is empty".to_string(),
        ));
    }
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(StepManiaOnlineError::Archive(format!(
            "archive exceeds {MAX_ARCHIVE_ENTRIES} entries"
        )));
    }
    let uncompressed_limit = archive_bytes
        .saturating_mul(12)
        .saturating_add(UNCOMPRESSED_HEADROOM_BYTES)
        .min(MAX_UNCOMPRESSED_BYTES);
    let mut prefix: Option<String> = None;
    let mut total_bytes = 0u64;
    let mut has_simfile = false;
    let mut output_files = HashSet::with_capacity(archive.len().min(4096));
    for idx in 0..archive.len() {
        let entry = archive
            .by_index(idx)
            .map_err(|error| StepManiaOnlineError::Archive(error.to_string()))?;
        if entry.name().len() > MAX_ARCHIVE_PATH_BYTES {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry path exceeds {MAX_ARCHIVE_PATH_BYTES} bytes"
            )));
        }
        if entry.enclosed_name().is_none() {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry '{}' escapes the pack root",
                entry.name()
            )));
        }
        if entry.is_symlink() {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry '{}' is a symbolic link",
                entry.name()
            )));
        }
        let is_dir = entry.is_dir() || entry.name().ends_with('\\');
        if !safe_unix_entry_type(entry.unix_mode(), is_dir) {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry '{}' is not a regular file or directory",
                entry.name()
            )));
        }
        let parts = portable_archive_parts(entry.name())?;
        let entry_prefix = parts
            .first()
            .ok_or_else(|| StepManiaOnlineError::Archive("entry has no path".to_string()))?;
        match &prefix {
            Some(expected) if expected != entry_prefix => {
                return Err(StepManiaOnlineError::Archive(
                    "archive does not have one top-level pack directory".to_string(),
                ));
            }
            None => prefix = Some(entry_prefix.clone()),
            _ => {}
        }
        if is_dir {
            continue;
        }
        if parts.len() == 1 {
            return Err(StepManiaOnlineError::Archive(
                "archive contains a file outside its pack directory".to_string(),
            ));
        }
        total_bytes = total_bytes.checked_add(entry.size()).ok_or_else(|| {
            StepManiaOnlineError::Archive("uncompressed size overflow".to_string())
        })?;
        if total_bytes > uncompressed_limit {
            return Err(StepManiaOnlineError::Archive(format!(
                "uncompressed content exceeds {uncompressed_limit} bytes"
            )));
        }
        let output_key = parts[1..].join("/").to_lowercase();
        if !output_files.insert(output_key) {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry '{}' duplicates another output path",
                entry.name()
            )));
        }
        has_simfile |= is_simfile(parts.last().map(String::as_str).unwrap_or_default());
    }
    if !has_simfile {
        return Err(StepManiaOnlineError::Archive(
            "archive contains no .sm, .ssc, or .dwi simfiles".to_string(),
        ));
    }
    Ok(ArchivePlan {
        prefix: prefix.unwrap_or_default(),
    })
}

fn portable_archive_parts(name: &str) -> Result<Vec<String>, StepManiaOnlineError> {
    if name.is_empty() || name.starts_with('/') || name.starts_with('\\') || name.contains('\0') {
        return Err(StepManiaOnlineError::Archive(format!(
            "entry '{name}' has an invalid path"
        )));
    }
    let mut parts = Vec::new();
    for part in name.split(['/', '\\']) {
        if part.is_empty() {
            continue;
        }
        let is_prefix = parts.is_empty();
        let drive_prefix = is_prefix
            && part.len() == 2
            && part.as_bytes()[0].is_ascii_alphabetic()
            && part.as_bytes()[1] == b':';
        let invalid_output_name = !is_prefix && part.chars().any(invalid_path_char);
        if part == "."
            || part == ".."
            || part.chars().any(char::is_control)
            || drive_prefix
            || invalid_output_name
        {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry '{name}' has an invalid path component"
            )));
        }
        parts.push(part.to_string());
    }
    if parts.is_empty() {
        return Err(StepManiaOnlineError::Archive(format!(
            "entry '{name}' has no path components"
        )));
    }
    Ok(parts)
}

fn safe_unix_entry_type(mode: Option<u32>, is_dir: bool) -> bool {
    const FILE_TYPE_MASK: u32 = 0o170000;
    const REGULAR_FILE: u32 = 0o100000;
    const DIRECTORY: u32 = 0o040000;
    let Some(kind) = mode.map(|mode| mode & FILE_TYPE_MASK) else {
        return true;
    };
    kind == 0 || is_dir && kind == DIRECTORY || !is_dir && kind == REGULAR_FILE
}

fn is_simfile(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("sm")
                || extension.eq_ignore_ascii_case("ssc")
                || extension.eq_ignore_ascii_case("dwi")
        })
}

pub fn sanitize_pack_name(raw: &str, pack_id: u64) -> String {
    sanitized_pack_name(raw, pack_id).0
}

fn sanitized_pack_name(raw: &str, pack_id: u64) -> (String, bool) {
    let trimmed = raw.trim();
    let mut changed = trimmed != raw;
    let mut output = String::with_capacity(trimmed.len().min(DESTINATION_MAX_CHARS));
    for ch in trimmed.chars() {
        let replacement = invalid_path_char(ch) || matches!(ch, '/' | '\\');
        if replacement {
            changed = true;
            if !output.ends_with('_') {
                output.push('_');
            }
        } else if output.chars().count() < DESTINATION_MAX_CHARS {
            output.push(ch);
        } else {
            changed = true;
        }
    }
    let clean_len = output.trim_end_matches([' ', '.']).len();
    if clean_len != output.len() {
        output.truncate(clean_len);
        changed = true;
    }
    if output.is_empty() || matches!(output.as_str(), "." | "..") {
        output = "StepManiaOnline Pack".to_string();
        changed = true;
    }
    let stem = output.split('.').next().unwrap_or_default();
    if WINDOWS_RESERVED_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(stem))
    {
        output.insert(0, '_');
        changed = true;
    }
    if changed {
        output = with_pack_id(output.as_str(), pack_id);
    }
    (output, changed)
}

fn invalid_path_char(ch: char) -> bool {
    ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*')
}

fn with_pack_id(name: &str, pack_id: u64) -> String {
    let suffix = format!(" [SMO {pack_id}]");
    let keep = DESTINATION_MAX_CHARS.saturating_sub(suffix.chars().count());
    let mut base: String = name.chars().take(keep).collect();
    let clean_len = base.trim_end_matches([' ', '.']).len();
    base.truncate(clean_len);
    base.push_str(suffix.as_str());
    base
}

fn choose_destination(root: &Path, pack: &PackInfo) -> Result<PathBuf, StepManiaOnlineError> {
    let (base, changed) = sanitized_pack_name(pack.name.as_str(), pack.id);
    if !child_exists_case_insensitive(root, base.as_ref())? {
        return Ok(root.join(base));
    }
    let alternate = if changed {
        base
    } else {
        with_pack_id(base.as_str(), pack.id)
    };
    if child_exists_case_insensitive(root, alternate.as_ref())? {
        return Err(StepManiaOnlineError::AlreadyInstalled(alternate));
    }
    Ok(root.join(alternate))
}

fn child_exists_case_insensitive(
    root: &Path,
    child_name: &std::ffi::OsStr,
) -> Result<bool, StepManiaOnlineError> {
    let wanted = child_name.to_string_lossy();
    let entries =
        fs::read_dir(root).map_err(|error| io_error("inspect the Songs directory", error))?;
    for entry in entries {
        let entry = entry.map_err(|error| io_error("inspect the Songs directory", error))?;
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(&wanted)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn remove_temp_path(path: &Path) -> Result<(), StepManiaOnlineError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error("inspect a temporary download path", error)),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).map_err(|error| io_error("remove a temporary download file", error))
    } else if metadata.is_dir() {
        fs::remove_dir_all(path)
            .map_err(|error| io_error("remove a temporary download directory", error))
    } else {
        Err(StepManiaOnlineError::Io {
            action: "remove a temporary download path",
            message: "path is not a regular file or directory".to_string(),
        })
    }
}

fn io_error(action: &'static str, error: std::io::Error) -> StepManiaOnlineError {
    StepManiaOnlineError::Io {
        action,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU64, Ordering};
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    const HEADER: &str = "ID, Pack Name, Song Count, Size, Sync, PackType, Substyle, Min Version\n";
    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

    fn pack(id: u64, name: &str, pack_type: Option<&str>, substyle: Option<&str>) -> PackInfo {
        PackInfo::new(
            id,
            name.to_string(),
            10,
            1_000,
            Some("n/a".to_string()),
            pack_type.map(str::to_string),
            substyle.map(str::to_string),
            Some("Stepmania 5".to_string()),
        )
    }

    fn temp_dir(label: &str) -> PathBuf {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("deadsync-smo-{label}-{}-{id}", std::process::id()))
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let mut bytes = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut bytes));
            for (name, contents) in entries {
                zip.start_file(*name, SimpleFileOptions::default())
                    .expect("start fixture entry");
                zip.write_all(contents).expect("write fixture entry");
            }
            zip.finish().expect("finish fixture archive");
        }
        fs::write(path, bytes).expect("write fixture archive");
    }

    #[test]
    fn catalog_parser_keeps_commas_and_embedded_quotes_in_names() {
        let text = format!(
            "{HEADER}42, \"Alice's \"Mix\", Volume 2\", 17, 123456, 9ms, pad, technical, Stepmania 5\n"
        );
        let packs = parse_catalog(text.as_str()).expect("catalog parses");
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].id, 42);
        assert_eq!(packs[0].name, "Alice's \"Mix\", Volume 2");
        assert_eq!(packs[0].song_count, 17);
        assert_eq!(packs[0].size_bytes, 123_456);
        assert_eq!(packs[0].pack_type.as_deref(), Some("pad"));
    }

    #[test]
    fn catalog_parser_maps_upstream_none_values() {
        let text = format!("{HEADER}7, \"Pack\", 1, 99, n/a, None, None, Stepmania 3.9\n");
        let packs = parse_catalog(text.as_str()).expect("catalog parses");
        assert_eq!(packs[0].sync.as_deref(), Some("n/a"));
        assert_eq!(packs[0].pack_type, None);
        assert_eq!(packs[0].substyle, None);
    }

    #[test]
    fn catalog_parser_rejects_duplicate_ids() {
        let row = "7, \"Pack\", 1, 99, n/a, None, None, Stepmania 3.9\n";
        let error = parse_catalog(format!("{HEADER}{row}{row}").as_str()).unwrap_err();
        assert!(error.to_string().contains("duplicate pack ID"));
    }

    #[test]
    fn catalog_search_ranks_name_hits_before_metadata_hits() {
        let catalog = [
            pack(1, "Technical Spectrum", Some("pad"), Some("technical")),
            pack(2, "Spectrum", Some("pad"), Some("technical")),
            pack(3, "Other Pack", Some("spectrum"), Some("technical")),
        ];
        assert_eq!(search_catalog(&catalog, "spectrum"), vec![1, 0, 2]);
        assert_eq!(search_catalog(&catalog, "other technical"), vec![2]);
        assert_eq!(search_catalog(&catalog, "   "), vec![0, 1, 2]);
    }

    #[test]
    fn sanitizer_handles_windows_names_and_path_characters() {
        assert_eq!(sanitize_pack_name("CON", 9), "_CON [SMO 9]");
        assert_eq!(
            sanitize_pack_name("Bad/Pack: Name. ", 12),
            "Bad_Pack_ Name [SMO 12]"
        );
        assert_eq!(sanitize_pack_name("Clean Pack", 3), "Clean Pack");
    }

    #[test]
    fn archive_extracts_beneath_stripped_top_level_directory() {
        let root = temp_dir("extract");
        fs::create_dir_all(&root).expect("create fixture root");
        let archive = root.join("pack.zip");
        let staging = root.join("staging");
        write_zip(
            &archive,
            &[
                ("Original Pack/Song/song.ssc", b"#TITLE:Song;"),
                ("Original Pack/Song/music.ogg", b"audio"),
            ],
        );
        let archive_bytes = fs::metadata(&archive).unwrap().len();

        extract_archive(&archive, &staging, archive_bytes).expect("extract fixture");
        assert!(staging.join("Song/song.ssc").is_file());
        assert!(!staging.join("Original Pack").exists());

        fs::remove_dir_all(root).expect("clean fixture root");
    }

    #[test]
    fn archive_rejects_traversal_and_multiple_roots() {
        let root = temp_dir("unsafe");
        fs::create_dir_all(&root).expect("create fixture root");
        assert!(portable_archive_parts("Pack/../escape.ssc").is_err());
        assert!(portable_archive_parts("Pack\\..\\escape.ssc").is_err());

        let roots = root.join("roots.zip");
        write_zip(
            &roots,
            &[("Pack A/a.ssc", b"chart"), ("Pack B/b.ssc", b"chart")],
        );
        let error = inspect_archive(&roots, fs::metadata(&roots).unwrap().len()).unwrap_err();
        assert!(error.to_string().contains("one top-level pack directory"));

        fs::remove_dir_all(root).expect("clean fixture root");
    }

    #[test]
    fn archive_requires_a_supported_simfile() {
        let root = temp_dir("no-simfile");
        fs::create_dir_all(&root).expect("create fixture root");
        let archive = root.join("pack.zip");
        write_zip(&archive, &[("Pack/Song/music.ogg", b"audio")]);
        let error = inspect_archive(&archive, fs::metadata(&archive).unwrap().len()).unwrap_err();
        assert!(error.to_string().contains("contains no .sm"));
        fs::remove_dir_all(root).expect("clean fixture root");
    }
}
