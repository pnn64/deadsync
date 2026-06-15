//! Experimental ReplayGain 2.0 / EBU R 128 loudness analysis and caching.
//!
//! Public API:
//! - [`get_or_queue_gain_linear`] - returns the linear playback gain for a
//!   song if already known (either in memory or on disk), otherwise enqueues
//!   a background analysis job and returns `None`.
//! - [`prewarm_paths`] - submit a batch of song paths at a priority class.
//!   Used by the music wheel to warm the cache for every song in a pack
//!   on expansion.
//! - [`clear_cache`] - drop all in-memory state and the on-disk cache file.
//!   Intended for debug / a future "rescan" option.
//!
//! Behavior summary:
//! - A pool of [`WORKER_THREADS`] threads pulls jobs off two queues
//!   (foreground previews ahead of background prewarm) and uses
//!   `deadsync-audio-analysis` to compute integrated loudness (LUFS)
//!   and true peak.
//! - Computed values are persisted in a single file at
//!   `cache_dir/replaygain.bin` (an in-memory `HashMap` keyed by
//!   xxhash64(canonical_path) is the source of truth; a dedicated flush
//!   thread debounces writes by [`FLUSH_DEBOUNCE`] and rewrites the file
//!   atomically via tmp + rename). Each entry stores the source file's mtime
//!   and an xxhash64 of its raw bytes: the mtime is the fast-path validator,
//!   and the content hash lets an unchanged file whose timestamp moved (e.g. a
//!   self-update that rewrites bundled assets) keep its cached gain instead of
//!   being needlessly re-analyzed.
//! - Linear gain is derived as `10^((TARGET_LUFS - lufs) / 20)`, clamped so
//!   that `gain * true_peak <= 1.0` (prevent clipping) and never exceeds
//!   +12 dB.
//!
//! When a song that triggered a queued analysis completes computation, the
//! worker reports the result through the callback passed to [`init`], so the
//! shell can apply it retroactively to the currently playing stream.

use deadsync_audio_analysis::{
    CacheFreshness, ReplayGainCacheEntry, ReplayGainCacheFile, ReplayGainInfo, UNITY_GAIN,
    compute_loudness, gain_linear_from_info, read_replaygain_cache_file, replaygain_cache_check,
    replaygain_cache_entry_for_path, replaygain_path_hash, write_replaygain_cache_file,
};
use log::{debug, info, warn};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

/// Sentinel track id used for prewarm jobs that aren't tied to a currently
/// playing track. `set_music_replaygain_if_matches` will treat this as a
/// non-match and ignore it.
const PREWARM_TRACK_ID: u64 = 0;

/// Number of analyzer worker threads. EBU R 128 over a full track averages
/// ~370 ms on a desktop CPU; two threads give enough headroom for the
/// previewed song (foreground) to always jump ahead of any pack-warm work
/// already in flight without saturating user-visible cores.
const WORKER_THREADS: usize = 2;

/// How long the flush thread waits after the last cache mutation before
/// writing the consolidated file to disk. 250 ms collapses bursts of
/// per-song completions during pack-prewarm into a single rewrite.
const FLUSH_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Clone)]
pub struct InitConfig {
    pub cache_file: PathBuf,
    pub legacy_cache_dir: PathBuf,
    pub result_callback: fn(u64, f32),
}

static INIT_CONFIG: OnceLock<InitConfig> = OnceLock::new();

pub fn init(config: InitConfig) -> Result<(), &'static str> {
    match INIT_CONFIG.set(config) {
        Ok(()) => Ok(()),
        Err(_) => Ok(()),
    }
}

#[inline(always)]
fn init_config() -> &'static InitConfig {
    INIT_CONFIG
        .get()
        .expect("deadsync_audio_replaygain::init must be called before use")
}

#[inline(always)]
fn publish_gain(track_id: u64, gain_linear: f32) {
    (init_config().result_callback)(track_id, gain_linear);
}

#[derive(Clone, Copy)]
enum SlotState {
    Pending,
    Ready(ReplayGainInfo),
    Failed,
}

struct Worker {
    inner: Mutex<WorkerInner>,
    cv: Condvar,
}

struct WorkerInner {
    fg: VecDeque<Job>,
    bg: VecDeque<Job>,
    shutdown: bool,
}

struct Job {
    /// Raw (possibly non-canonical) path supplied by the caller. The worker
    /// canonicalizes before opening the file so the UI thread never blocks
    /// on `fs::canonicalize` during a wheel scroll.
    path: PathBuf,
    /// Track id used to route the result back to `play_music` if this job
    /// was queued for a foreground preview. Prewarm jobs use
    /// `PREWARM_TRACK_ID` so they are never applied to a playing stream.
    track_id: u64,
}

/// Priority class for a queued analysis job. Foreground jobs always run
/// before background jobs; the worker pool polls the foreground queue first
/// on every wake-up.
#[derive(Clone, Copy, Debug)]
pub enum Priority {
    Foreground,
    Background,
}

static IN_MEMORY: OnceLock<Mutex<HashMap<PathBuf, SlotState>>> = OnceLock::new();
static WORKER: OnceLock<Worker> = OnceLock::new();

#[inline(always)]
fn in_memory() -> &'static Mutex<HashMap<PathBuf, SlotState>> {
    IN_MEMORY.get_or_init(|| Mutex::new(HashMap::new()))
}

#[inline(always)]
fn worker() -> &'static Worker {
    WORKER.get_or_init(spawn_worker_pool)
}

fn spawn_worker_pool() -> Worker {
    let worker = Worker {
        inner: Mutex::new(WorkerInner {
            fg: VecDeque::new(),
            bg: VecDeque::new(),
            shutdown: false,
        }),
        cv: Condvar::new(),
    };
    for idx in 0..WORKER_THREADS {
        thread::Builder::new()
            .name(format!("replaygain-analyzer-{idx}"))
            .spawn(move || worker_loop())
            .expect("failed to spawn replaygain worker");
    }
    worker
}

fn worker_loop() {
    loop {
        let job = {
            let w = worker();
            let mut guard = w.inner.lock().unwrap();
            loop {
                if guard.shutdown {
                    return;
                }
                if let Some(job) = guard.fg.pop_front().or_else(|| guard.bg.pop_front()) {
                    break job;
                }
                guard = w.cv.wait(guard).unwrap();
            }
        };
        analyze_one(job);
    }
}

#[inline]
fn enqueue(job: Job, priority: Priority) {
    let w = worker();
    {
        let mut guard = w.inner.lock().unwrap();
        match priority {
            Priority::Foreground => guard.fg.push_back(job),
            Priority::Background => guard.bg.push_back(job),
        }
    }
    w.cv.notify_one();
}

/// Returns the linear gain to apply when playing `path`, if it has already
/// been computed (memory or disk). If the value is not yet known, queues a
/// foreground analysis job tagged with `track_id` and returns `None`. When
/// the analysis later completes, the worker reports the resulting gain
/// through the callback passed to [`init`].
pub fn get_or_queue_gain_linear(path: &Path, track_id: u64) -> Option<f32> {
    let abs = canonicalize_or_clone(path);

    {
        let map = in_memory().lock().unwrap();
        match map.get(&abs) {
            Some(SlotState::Ready(info)) => return Some(gain_linear_from_info(*info)),
            Some(SlotState::Failed) => return Some(UNITY_GAIN),
            Some(SlotState::Pending) => {
                // Already enqueued by a previous caller. Drop the current
                // track id into a fresh foreground job below so the latest
                // call gets notified when work completes, and so the
                // currently-previewed song jumps ahead of any pack-warm
                // backlog.
            }
            None => {}
        }
    }

    if let Some(info) = load_disk_cache(&abs) {
        in_memory()
            .lock()
            .unwrap()
            .insert(abs.clone(), SlotState::Ready(info));
        return Some(gain_linear_from_info(info));
    }

    {
        let mut map = in_memory().lock().unwrap();
        map.insert(abs.clone(), SlotState::Pending);
    }
    enqueue(
        Job {
            path: abs,
            track_id,
        },
        Priority::Foreground,
    );
    None
}

pub fn prewarm_paths<I>(paths: I, priority: Priority)
where
    I: IntoIterator<Item = PathBuf>,
{
    for path in paths {
        {
            let mut map = in_memory().lock().unwrap();
            if map.contains_key(&path) {
                continue;
            }
            map.insert(path.clone(), SlotState::Pending);
        }
        enqueue(
            Job {
                path,
                track_id: PREWARM_TRACK_ID,
            },
            priority,
        );
    }
}

/// Drops the in-memory map, clears the on-disk cache file, and removes
/// any leftover legacy per-song cache directory.
pub fn clear_cache() {
    if let Some(mutex) = IN_MEMORY.get() {
        mutex.lock().unwrap().clear();
    }
    if let Some(cache) = DISK_CACHE.get() {
        {
            let mut map = cache.entries.lock().unwrap();
            map.clear();
        }
        {
            let mut state = cache.flush_state.lock().unwrap();
            state.dirty = true;
        }
        cache.flush_cv.notify_one();
        flush_now();
    } else {
        // DiskCache hasn't been initialized yet; remove the file directly
        // so a later init() starts empty.
        let file = init_config().cache_file.clone();
        if let Err(err) = fs::remove_file(&file)
            && err.kind() != std::io::ErrorKind::NotFound
        {
            warn!(
                "Failed to remove ReplayGain cache file {}: {err}",
                file.display()
            );
        }
    }

    let legacy = init_config().legacy_cache_dir.clone();
    if legacy.exists()
        && let Err(err) = fs::remove_dir_all(&legacy)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        warn!(
            "Failed to clear legacy ReplayGain cache dir {}: {err}",
            legacy.display()
        );
    }
}

/* --------------------------- Worker internals --------------------------- */

fn analyze_one(job: Job) {
    let Job { path, track_id } = job;
    // Canonicalize on the worker thread so callers (UI / wheel prewarm)
    // never block on `fs::canonicalize`. After canonicalization, the
    // canonical path becomes the cache key going forward.
    let canonical = canonicalize_or_clone(&path);
    if canonical != path {
        let mut map = in_memory().lock().unwrap();
        map.remove(&path);
        match map.get(&canonical).copied() {
            // A racing foreground call may have already resolved the
            // result via the disk cache while this prewarm job sat on the
            // background queue. In that case route the existing answer
            // (no-op for prewarm) and skip re-running the analyzer.
            Some(SlotState::Ready(info)) => {
                drop(map);
                publish_gain(track_id, gain_linear_from_info(info));
                return;
            }
            Some(SlotState::Failed) => {
                drop(map);
                publish_gain(track_id, UNITY_GAIN);
                return;
            }
            // None / Pending: claim or coexist with another Pending slot
            // and proceed. If another worker is already analyzing the
            // same canonical path we accept one duplicate analysis in
            // exchange for guaranteed track_id routing.
            _ => {
                map.insert(canonical.clone(), SlotState::Pending);
            }
        }
    }

    // Short-circuit on a disk cache hit: prewarm_paths intentionally
    // doesn't canonicalize on the UI thread, so the disk cache lookup
    // only becomes meaningful for prewarm jobs once the worker has the
    // canonical path in hand. Without this, every restart re-analyzes
    // the entire library.
    if let Some(info) = load_disk_cache(&canonical) {
        in_memory()
            .lock()
            .unwrap()
            .insert(canonical, SlotState::Ready(info));
        publish_gain(track_id, gain_linear_from_info(info));
        return;
    }

    let info = match compute_loudness(&canonical) {
        Ok(info) => info,
        Err(err) => {
            debug!(
                "ReplayGain analysis failed for {}: {err}",
                canonical.display()
            );
            in_memory()
                .lock()
                .unwrap()
                .insert(canonical.clone(), SlotState::Failed);
            publish_gain(track_id, UNITY_GAIN);
            return;
        }
    };

    write_disk_cache(&canonical, info);
    in_memory()
        .lock()
        .unwrap()
        .insert(canonical, SlotState::Ready(info));
    publish_gain(track_id, gain_linear_from_info(info));
}

/* ---------------------------- Disk cache I/O ---------------------------- */

fn canonicalize_or_clone(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Single-file consolidated cache of every analyzed song. Replaces the
/// legacy per-song `.bin` layout, which doesn't scale to libraries of
/// 10k+ songs.
struct DiskCache {
    entries: Mutex<HashMap<u64, ReplayGainCacheEntry>>,
    flush_state: Mutex<FlushState>,
    flush_cv: Condvar,
}

#[derive(Default)]
struct FlushState {
    dirty: bool,
    /// Set by `flush_now` callers; the flush thread clears it after the
    /// next successful flush so the caller can be woken via `flush_done_cv`.
    sync_request: bool,
    shutdown: bool,
}

static DISK_CACHE: OnceLock<DiskCache> = OnceLock::new();
static FLUSH_DONE_CV: OnceLock<(Mutex<u64>, Condvar)> = OnceLock::new();

#[inline]
fn flush_done_cv() -> &'static (Mutex<u64>, Condvar) {
    FLUSH_DONE_CV.get_or_init(|| (Mutex::new(0), Condvar::new()))
}

#[inline]
fn disk_cache() -> &'static DiskCache {
    DISK_CACHE.get_or_init(init_disk_cache)
}

fn init_disk_cache() -> DiskCache {
    // The feature hasn't shipped, so don't bother migrating the legacy
    // per-file directory; just remove it if it's there.
    let legacy_dir = init_config().legacy_cache_dir.clone();
    if legacy_dir.exists()
        && let Err(err) = fs::remove_dir_all(&legacy_dir)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        warn!(
            "Failed to remove legacy ReplayGain cache dir {}: {err}",
            legacy_dir.display()
        );
    }

    let entries = load_cache_file().unwrap_or_default();
    if !entries.is_empty() {
        info!("ReplayGain cache loaded: {} entries", entries.len());
    }
    let cache = DiskCache {
        entries: Mutex::new(entries),
        flush_state: Mutex::new(FlushState::default()),
        flush_cv: Condvar::new(),
    };

    thread::Builder::new()
        .name("replaygain-flush".to_string())
        .spawn(flush_loop)
        .expect("failed to spawn replaygain flush thread");

    cache
}

fn load_cache_file() -> Option<HashMap<u64, ReplayGainCacheEntry>> {
    read_replaygain_cache_file(&init_config().cache_file)
}

fn flush_loop() {
    let cache = disk_cache();
    loop {
        // Wait for a dirty signal (or shutdown).
        {
            let mut guard = cache.flush_state.lock().unwrap();
            while !guard.dirty && !guard.sync_request && !guard.shutdown {
                guard = cache.flush_cv.wait(guard).unwrap();
            }
            if guard.shutdown {
                return;
            }
            // If only a debounced (non-sync) flush is pending, sleep a bit
            // to collapse bursts. A sync_request bypasses the debounce.
            let needs_debounce = guard.dirty && !guard.sync_request;
            drop(guard);
            if needs_debounce {
                thread::sleep(FLUSH_DEBOUNCE);
            }
        }

        // Snapshot the entries under lock, then release before encoding /
        // writing so analyzer threads aren't blocked on disk I/O.
        let (snapshot, was_sync) = {
            let entries = cache.entries.lock().unwrap();
            let mut state = cache.flush_state.lock().unwrap();
            let was_sync = state.sync_request;
            state.dirty = false;
            state.sync_request = false;
            (
                ReplayGainCacheFile::from_entries(entries.values().copied()),
                was_sync,
            )
        };

        if let Err(err) = write_cache_file(&snapshot) {
            warn!("Failed to write ReplayGain cache file: {err}");
        } else {
            debug!(
                "ReplayGain cache flushed: {} entries -> {}",
                snapshot.entries.len(),
                init_config().cache_file.display()
            );
        }

        if was_sync {
            let (mtx, cv) = flush_done_cv();
            let mut counter = mtx.lock().unwrap();
            *counter = counter.wrapping_add(1);
            cv.notify_all();
        }
    }
}

fn write_cache_file(payload: &ReplayGainCacheFile) -> std::io::Result<()> {
    write_replaygain_cache_file(&init_config().cache_file, payload)
}

fn load_disk_cache(song_path: &Path) -> Option<ReplayGainInfo> {
    let key = replaygain_path_hash(song_path);
    let entry = {
        let map = disk_cache().entries.lock().unwrap();
        map.get(&key).copied()
    }?;
    match replaygain_cache_check(entry, song_path) {
        CacheFreshness::Fresh(info) => Some(info),
        CacheFreshness::Refreshed(updated) => {
            // The gain is still valid but its mtime/content hash moved (e.g. a
            // self-update rewrote a bundled track with identical bytes, or a
            // migrated v1 entry just learned its content hash). Persist the
            // corrected entry so we don't re-validate against disk every play.
            let cache = disk_cache();
            {
                let mut map = cache.entries.lock().unwrap();
                map.insert(updated.path_hash, updated);
            }
            {
                let mut state = cache.flush_state.lock().unwrap();
                state.dirty = true;
            }
            cache.flush_cv.notify_one();
            Some(updated.info())
        }
        CacheFreshness::Stale => None,
    }
}

fn write_disk_cache(song_path: &Path, info: ReplayGainInfo) {
    let entry = replaygain_cache_entry_for_path(song_path, info);
    let cache = disk_cache();
    {
        let mut map = cache.entries.lock().unwrap();
        map.insert(entry.path_hash, entry);
    }
    {
        let mut state = cache.flush_state.lock().unwrap();
        state.dirty = true;
    }
    cache.flush_cv.notify_one();
}

/// Triggers a synchronous flush of the in-memory cache to disk and blocks
/// until the flush thread reports completion. Used by `clear_cache` and
/// available for tests / shutdown hooks. If the flush thread fails to
/// acknowledge within `timeout`, returns without error so the caller can
/// continue.
pub fn flush_now() {
    flush_now_with_timeout(Duration::from_secs(5));
}

fn flush_now_with_timeout(timeout: Duration) {
    let cache = disk_cache();
    let (mtx, cv) = flush_done_cv();
    let baseline = *mtx.lock().unwrap();
    {
        let mut state = cache.flush_state.lock().unwrap();
        state.dirty = true;
        state.sync_request = true;
    }
    cache.flush_cv.notify_one();
    let mut counter = mtx.lock().unwrap();
    let deadline = std::time::Instant::now() + timeout;
    while *counter == baseline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return;
        }
        let (next, result) = cv.wait_timeout(counter, remaining).unwrap();
        counter = next;
        if result.timed_out() {
            return;
        }
    }
}
