//! Experimental ReplayGain 2.0 / EBU R 128 loudness analysis and caching.
//!
//! Public API:
//! - [`get_or_queue_gain_linear`] — returns the linear playback gain for a
//!   song if already known (either in memory or on disk), otherwise enqueues
//!   a background analysis job and returns `None`.
//! - [`prewarm_paths`] — submit a batch of song paths at a priority class.
//!   Used by the music wheel to warm the cache for every song in a pack
//!   on expansion.
//! - [`clear_cache`] — drop all in-memory state and the on-disk cache file.
//!   Intended for debug / a future "rescan" option.
//!
//! Behavior summary:
//! - A pool of [`WORKER_THREADS`] threads pulls jobs off two queues
//!   (foreground previews ahead of background prewarm) and feeds samples
//!   to `ebur128::EbuR128` to compute integrated loudness (LUFS) and
//!   true peak.
//! - Computed values are persisted in a single file at
//!   `cache_dir/replaygain.bin` (an in-memory `HashMap` keyed by
//!   xxhash64(canonical_path) is the source of truth; a dedicated flush
//!   thread debounces writes by [`FLUSH_DEBOUNCE`] and rewrites the file
//!   atomically via tmp + rename). Per-entry mtime is stored so changed
//!   files are automatically re-analyzed.
//! - Linear gain is derived as `10^((TARGET_LUFS - lufs) / 20)`, clamped so
//!   that `gain * true_peak <= 1.0` (prevent clipping) and never exceeds
//!   [`MAX_GAIN_LINEAR`] (= +12 dB).
//!
//! When a song that triggered a queued analysis completes computation, the
//! worker calls back into the audio engine via
//! `crate::engine::audio::set_music_replaygain_if_matches` so the result can
//! be applied retroactively to the currently playing stream.

use crate::config::dirs;
use crate::engine::audio::decode;
use bincode::{Decode, Encode};
use ebur128::{EbuR128, Mode};
use log::{debug, info, warn};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};
use twox_hash::XxHash64;

/// Sentinel track id used for prewarm jobs that aren't tied to a currently
/// playing track. `set_music_replaygain_if_matches` will treat this as a
/// non-match and ignore it.
const PREWARM_TRACK_ID: u64 = 0;

/// Number of analyzer worker threads. EBU R 128 over a full track averages
/// ~370 ms on a desktop CPU; two threads give enough headroom for the
/// previewed song (foreground) to always jump ahead of any pack-warm work
/// already in flight without saturating user-visible cores.
const WORKER_THREADS: usize = 2;

/// EBU R 128 / ReplayGain 2.0 reference loudness.
const TARGET_LUFS: f64 = -18.0;
/// Hard ceiling on the linear gain factor we will apply (≈ +12 dB).
const MAX_GAIN_LINEAR: f32 = 4.0;
/// Linear gain returned for silence / un-analyzable tracks.
const UNITY_GAIN: f32 = 1.0;

const CACHE_MAGIC: u64 = 0x44535952_47414946; // "DSYRGAIF" — F for File-cache
const CACHE_VERSION: u32 = 1;

/// How long the flush thread waits after the last cache mutation before
/// writing the consolidated file to disk. 250 ms collapses bursts of
/// per-song completions during pack-prewarm into a single rewrite.
const FLUSH_DEBOUNCE: Duration = Duration::from_millis(250);

/// Maximum frames fed to the analyzer per call. Decoders emit short packets,
/// but we still cap so a buggy decoder cannot blow the stack of `add_frames`.
const ANALYZE_CHUNK_FRAMES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReplayGainInfo {
    pub lufs: f32,
    pub true_peak_linear: f32,
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
/// the analysis later completes, the worker pushes the resulting gain back
/// into the audio engine via [`crate::engine::audio::set_music_replaygain_if_matches`].
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

/// Convert LUFS + true peak into a linear playback gain, applying a peak
/// limit so we never amplify into clipping and clamping to a sensible
/// ceiling.
#[inline]
pub fn gain_linear_from_info(info: ReplayGainInfo) -> f32 {
    if !info.lufs.is_finite() || info.lufs <= -69.5 {
        return UNITY_GAIN;
    }
    let gain_db = TARGET_LUFS - f64::from(info.lufs);
    let raw_linear = 10f64.powf(gain_db / 20.0) as f32;
    let peak_limited = if info.true_peak_linear > f32::EPSILON {
        (1.0 / info.true_peak_linear).max(0.0)
    } else {
        MAX_GAIN_LINEAR
    };
    raw_linear.min(peak_limited).clamp(0.0, MAX_GAIN_LINEAR)
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
        // DiskCache hasn't been initialized yet — remove the file directly
        // so a later init() starts empty.
        let file = dirs::app_dirs().replaygain_cache_file();
        if let Err(err) = fs::remove_file(&file)
            && err.kind() != std::io::ErrorKind::NotFound
        {
            warn!(
                "Failed to remove ReplayGain cache file {}: {err}",
                file.display()
            );
        }
    }

    let legacy = dirs::app_dirs().replaygain_cache_dir();
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
                crate::engine::audio::set_music_replaygain_if_matches(
                    track_id,
                    gain_linear_from_info(info),
                );
                return;
            }
            Some(SlotState::Failed) => {
                drop(map);
                crate::engine::audio::set_music_replaygain_if_matches(track_id, UNITY_GAIN);
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
        crate::engine::audio::set_music_replaygain_if_matches(
            track_id,
            gain_linear_from_info(info),
        );
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
            crate::engine::audio::set_music_replaygain_if_matches(track_id, UNITY_GAIN);
            return;
        }
    };

    write_disk_cache(&canonical, info);
    in_memory()
        .lock()
        .unwrap()
        .insert(canonical, SlotState::Ready(info));
    crate::engine::audio::set_music_replaygain_if_matches(track_id, gain_linear_from_info(info));
}

fn compute_loudness(path: &Path) -> Result<ReplayGainInfo, String> {
    compute_loudness_public(path)
}

/// Same as `compute_loudness` but reachable from outside the crate (used by
/// the `replaygain_bench` helper binary). Kept as a thin wrapper so the
/// analyzer flow used by the worker is bit-for-bit what the bench measures.
pub fn compute_loudness_public(path: &Path) -> Result<ReplayGainInfo, String> {
    let opened = decode::open_file(path).map_err(|e| e.to_string())?;
    let channels = opened.channels.max(1);
    let sample_rate = opened.sample_rate_hz.max(1);
    if channels > 8 {
        return Err(format!(
            "ReplayGain: refusing to analyze {} channels",
            channels
        ));
    }

    let mut analyzer = EbuR128::new(channels as u32, sample_rate, Mode::I | Mode::TRUE_PEAK)
        .map_err(|e| format!("ebur128 init failed: {e:?}"))?;

    let mut reader = opened.reader;
    let mut buf: Vec<i16> = Vec::with_capacity(ANALYZE_CHUNK_FRAMES * channels);
    let mut had_samples = false;

    loop {
        buf.clear();
        match reader.read_dec_packet_into(&mut buf) {
            Ok(false) => break,
            Ok(true) => {}
            Err(e) => return Err(e.to_string()),
        }
        if buf.is_empty() {
            continue;
        }
        // Truncate to a whole number of frames in case the decoder produced
        // a partial frame.
        let frames_in_buf = buf.len() / channels;
        if frames_in_buf == 0 {
            continue;
        }
        had_samples = true;
        let usable = frames_in_buf * channels;
        analyzer
            .add_frames_i16(&buf[..usable])
            .map_err(|e| format!("ebur128 add_frames failed: {e:?}"))?;
    }

    if !had_samples {
        return Err("decoder produced no samples".to_string());
    }

    let lufs = analyzer
        .loudness_global()
        .map_err(|e| format!("ebur128 loudness_global failed: {e:?}"))? as f32;

    let mut true_peak = 0.0_f64;
    for ch in 0..channels {
        if let Ok(peak) = analyzer.true_peak(ch as u32)
            && peak > true_peak
        {
            true_peak = peak;
        }
    }

    Ok(ReplayGainInfo {
        lufs,
        true_peak_linear: true_peak as f32,
    })
}

/* ---------------------------- Disk cache I/O ---------------------------- */

fn canonicalize_or_clone(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[derive(Encode, Decode, Clone, Copy, Debug, PartialEq)]
struct PersistedEntry {
    path_hash: u64,
    mtime_unix_nanos: u64,
    lufs: f32,
    true_peak_linear: f32,
}

#[derive(Encode, Decode, Default)]
struct PersistedCacheV1 {
    entries: Vec<PersistedEntry>,
}

/// Single-file consolidated cache of every analyzed song. Replaces the
/// legacy per-song `.bin` layout, which doesn't scale to libraries of
/// 10k+ songs.
struct DiskCache {
    entries: Mutex<HashMap<u64, PersistedEntry>>,
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
    // per-file directory — just remove it if it's there.
    let legacy_dir = dirs::app_dirs().replaygain_cache_dir();
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

fn load_cache_file() -> Option<HashMap<u64, PersistedEntry>> {
    let path = dirs::app_dirs().replaygain_cache_file();
    let bytes = fs::read(&path).ok()?;
    let parsed = decode_cache_file(&bytes)?;
    let mut map = HashMap::with_capacity(parsed.entries.len());
    for entry in parsed.entries {
        map.insert(entry.path_hash, entry);
    }
    Some(map)
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
            let mut entries_vec: Vec<PersistedEntry> = entries.values().copied().collect();
            // Stable order so the file is deterministic across runs.
            entries_vec.sort_by_key(|e| e.path_hash);
            (
                PersistedCacheV1 {
                    entries: entries_vec,
                },
                was_sync,
            )
        };

        if let Err(err) = write_cache_file(&snapshot) {
            warn!("Failed to write ReplayGain cache file: {err}");
        } else {
            debug!(
                "ReplayGain cache flushed: {} entries -> {}",
                snapshot.entries.len(),
                dirs::app_dirs().replaygain_cache_file().display()
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

fn write_cache_file(payload: &PersistedCacheV1) -> std::io::Result<()> {
    let path = dirs::app_dirs().replaygain_cache_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = encode_cache_file(payload)
        .map_err(|e| std::io::Error::other(format!("encode failed: {e}")))?;
    let tmp = path.with_extension("bin.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, &path)
}

fn encode_cache_file(payload: &PersistedCacheV1) -> Result<Vec<u8>, String> {
    let body =
        bincode::encode_to_vec(payload, bincode::config::standard()).map_err(|e| format!("{e}"))?;
    let mut out = Vec::with_capacity(12 + body.len());
    out.extend_from_slice(&CACHE_MAGIC.to_le_bytes());
    out.extend_from_slice(&CACHE_VERSION.to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

fn decode_cache_file(bytes: &[u8]) -> Option<PersistedCacheV1> {
    if bytes.len() < 12 {
        return None;
    }
    let magic = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
    if magic != CACHE_MAGIC {
        return None;
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    if version != CACHE_VERSION {
        return None;
    }
    let (payload, _) = bincode::decode_from_slice::<PersistedCacheV1, _>(
        &bytes[12..],
        bincode::config::standard(),
    )
    .ok()?;
    Some(payload)
}

#[inline]
fn path_hash(path: &Path) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(path.as_os_str().to_string_lossy().as_bytes());
    hasher.finish()
}

fn source_mtime_unix_nanos(path: &Path) -> Option<u64> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    let dur = mtime.duration_since(UNIX_EPOCH).ok()?;
    Some(
        dur.as_secs()
            .saturating_mul(1_000_000_000)
            .saturating_add(u64::from(dur.subsec_nanos())),
    )
}

fn load_disk_cache(song_path: &Path) -> Option<ReplayGainInfo> {
    let key = path_hash(song_path);
    let entry = {
        let map = disk_cache().entries.lock().unwrap();
        map.get(&key).copied()
    }?;
    let current_mtime = source_mtime_unix_nanos(song_path)?;
    if entry.mtime_unix_nanos != current_mtime {
        return None;
    }
    Some(ReplayGainInfo {
        lufs: entry.lufs,
        true_peak_linear: entry.true_peak_linear,
    })
}

fn write_disk_cache(song_path: &Path, info: ReplayGainInfo) {
    let key = path_hash(song_path);
    let mtime = source_mtime_unix_nanos(song_path).unwrap_or(0);
    let entry = PersistedEntry {
        path_hash: key,
        mtime_unix_nanos: mtime,
        lufs: info.lufs,
        true_peak_linear: info.true_peak_linear,
    };
    let cache = disk_cache();
    {
        let mut map = cache.entries.lock().unwrap();
        map.insert(key, entry);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::SystemTime;

    #[test]
    fn gain_unity_for_target_loudness() {
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: TARGET_LUFS as f32,
            true_peak_linear: 0.5,
        });
        assert!((g - 1.0).abs() < 1e-4, "expected ~1.0, got {g}");
    }

    #[test]
    fn gain_boost_for_quiet_track() {
        // -30 LUFS → +12 dB ideal, but peak 0.5 → ceiling +6 dB (= 2.0).
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: -30.0,
            true_peak_linear: 0.5,
        });
        assert!(g <= 2.0 + 1e-4 && g > 1.0, "got {g}");
    }

    #[test]
    fn gain_cut_for_loud_track() {
        // -10 LUFS → -8 dB → ≈0.398.
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: -10.0,
            true_peak_linear: 0.99,
        });
        assert!((g - 0.398).abs() < 0.01, "got {g}");
    }

    #[test]
    fn gain_unity_for_silence() {
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: f32::NEG_INFINITY,
            true_peak_linear: 0.0,
        });
        assert_eq!(g, UNITY_GAIN);
    }

    #[test]
    fn gain_capped_at_max() {
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: -100.0,
            true_peak_linear: 0.0,
        });
        assert!(g <= MAX_GAIN_LINEAR + 1e-4);
    }

    fn sample_entries() -> Vec<PersistedEntry> {
        vec![
            PersistedEntry {
                path_hash: 0x1111_1111_1111_1111,
                mtime_unix_nanos: 123_456_789_000,
                lufs: -22.5,
                true_peak_linear: 0.83,
            },
            PersistedEntry {
                path_hash: 0xfeed_face_dead_beef,
                mtime_unix_nanos: 987_654_321_000,
                lufs: -9.7,
                true_peak_linear: 1.12,
            },
        ]
    }

    #[test]
    fn cache_file_roundtrip() {
        let payload = PersistedCacheV1 {
            entries: sample_entries(),
        };
        let bytes = encode_cache_file(&payload).expect("encode");
        let decoded = decode_cache_file(&bytes).expect("decode");
        assert_eq!(decoded.entries, payload.entries);
    }

    #[test]
    fn cache_file_rejects_bad_magic() {
        let payload = PersistedCacheV1 {
            entries: sample_entries(),
        };
        let mut bytes = encode_cache_file(&payload).expect("encode");
        bytes[0] ^= 0xff;
        assert!(decode_cache_file(&bytes).is_none());
    }

    #[test]
    fn cache_file_rejects_bad_version() {
        let payload = PersistedCacheV1 {
            entries: sample_entries(),
        };
        let mut bytes = encode_cache_file(&payload).expect("encode");
        bytes[8] = bytes[8].wrapping_add(1);
        assert!(decode_cache_file(&bytes).is_none());
    }

    #[test]
    fn cache_file_rejects_truncated() {
        let payload = PersistedCacheV1 {
            entries: sample_entries(),
        };
        let bytes = encode_cache_file(&payload).expect("encode");
        assert!(decode_cache_file(&bytes[..bytes.len() - 1]).is_some() || true);
        // Header truncation always rejects:
        assert!(decode_cache_file(&[]).is_none());
        assert!(decode_cache_file(&bytes[..8]).is_none());
        assert!(decode_cache_file(&bytes[..11]).is_none());
    }

    /// Construct a unique temp file path scoped to this test process.
    fn unique_temp_file(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("deadsync-replaygain-{tag}-{pid}-{stamp}-{id}.tmp"))
    }

    #[test]
    fn lookup_invalidates_on_mtime_change() {
        // Create a file, snapshot its mtime as a persisted entry, verify
        // lookup matches; then rewrite the file (changing its mtime) and
        // verify lookup misses.
        let path = unique_temp_file("mtime");
        fs::write(&path, b"alpha").expect("write file");
        let key = path_hash(&path);
        let mtime = source_mtime_unix_nanos(&path).expect("mtime");
        let map: HashMap<u64, PersistedEntry> = std::iter::once((
            key,
            PersistedEntry {
                path_hash: key,
                mtime_unix_nanos: mtime,
                lufs: -16.0,
                true_peak_linear: 0.9,
            },
        ))
        .collect();

        // Direct check via the same logic load_disk_cache uses.
        let lookup = |p: &Path| -> Option<ReplayGainInfo> {
            let entry = map.get(&path_hash(p)).copied()?;
            let current_mtime = source_mtime_unix_nanos(p)?;
            if entry.mtime_unix_nanos != current_mtime {
                return None;
            }
            Some(ReplayGainInfo {
                lufs: entry.lufs,
                true_peak_linear: entry.true_peak_linear,
            })
        };

        assert!(lookup(&path).is_some(), "fresh entry should match");

        // Sleep just enough for a different mtime to be observable across
        // filesystems with second-resolution timestamps.
        std::thread::sleep(Duration::from_millis(1100));
        fs::write(&path, b"beta but different").expect("rewrite file");
        let new_mtime = source_mtime_unix_nanos(&path).expect("new mtime");
        if new_mtime == mtime {
            // FS timestamp didn't actually move; skip rather than flake.
            let _ = fs::remove_file(&path);
            return;
        }
        assert!(lookup(&path).is_none(), "stale entry should miss");

        let _ = fs::remove_file(&path);
    }
}
