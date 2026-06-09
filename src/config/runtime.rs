use super::audio::{pack_audio_mix_levels, unpack_audio_mix_levels};
use super::{AdditionalSongFolder, AudioMixLevels, Config, DEFAULT_MACHINE_NOTESKIN};
use log::{debug, warn};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

// Global, mutable configuration instance.
static CONFIG: std::sync::LazyLock<Mutex<Config>> =
    std::sync::LazyLock::new(|| Mutex::new(Config::default()));
static LOCK_WAIT_EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
static AUDIO_MIX_LEVELS_PACKED: std::sync::LazyLock<AtomicU32> = std::sync::LazyLock::new(|| {
    let cfg = Config::default();
    AtomicU32::new(pack_audio_mix_levels(
        cfg.master_volume,
        cfg.music_volume,
        cfg.sfx_volume,
        cfg.assist_tick_volume,
    ))
});
pub(super) static MACHINE_DEFAULT_NOTESKIN: std::sync::LazyLock<Mutex<String>> =
    std::sync::LazyLock::new(|| Mutex::new(DEFAULT_MACHINE_NOTESKIN.to_string()));
pub(super) static ADDITIONAL_SONG_FOLDERS: std::sync::LazyLock<Mutex<Vec<AdditionalSongFolder>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));
/// SMX pad → player serial assignment (slot 0 = P1, slot 1 = P2). Stored outside
/// the `Copy` `Config` because serials are owned strings. `None` = follow jumper.
pub(super) static SMX_P1_SERIAL: std::sync::LazyLock<Mutex<Option<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));
pub(super) static SMX_P2_SERIAL: std::sync::LazyLock<Mutex<Option<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));
static SAVE_TX: std::sync::LazyLock<Option<mpsc::Sender<SaveReq>>> =
    std::sync::LazyLock::new(start_save_worker);

const LOCK_WAIT_REPORT_INTERVAL_NS: u64 = 5_000_000_000;
const LOCK_WAIT_SLOW_NS: u64 = 50_000;
const LOCK_WAIT_SPIKE_NS: u64 = 2_000_000;

struct LockWaitStats {
    lock_count: AtomicU64,
    wait_ns_total: AtomicU64,
    wait_ns_max: AtomicU64,
    slow_wait_count: AtomicU64,
    last_report_ns: AtomicU64,
}

impl LockWaitStats {
    const fn new() -> Self {
        Self {
            lock_count: AtomicU64::new(0),
            wait_ns_total: AtomicU64::new(0),
            wait_ns_max: AtomicU64::new(0),
            slow_wait_count: AtomicU64::new(0),
            last_report_ns: AtomicU64::new(0),
        }
    }
}

static CONFIG_LOCK_WAIT_STATS: LockWaitStats = LockWaitStats::new();

#[inline(always)]
fn lock_wait_stats_enabled() -> bool {
    log::max_level() >= log::LevelFilter::Debug
}

#[inline(always)]
fn lock_wait_now_ns() -> u64 {
    LOCK_WAIT_EPOCH.elapsed().as_nanos().min(u64::MAX as u128) as u64
}

#[inline(always)]
fn record_lock_wait(lock_name: &str, stats: &LockWaitStats, waited_ns: u64) {
    stats.lock_count.fetch_add(1, Ordering::Relaxed);
    stats.wait_ns_total.fetch_add(waited_ns, Ordering::Relaxed);
    stats.wait_ns_max.fetch_max(waited_ns, Ordering::Relaxed);
    if waited_ns >= LOCK_WAIT_SLOW_NS {
        stats.slow_wait_count.fetch_add(1, Ordering::Relaxed);
    }
    if waited_ns >= LOCK_WAIT_SPIKE_NS {
        debug!(
            "lock-wait[{lock_name}] spike={:.3}ms",
            waited_ns as f64 / 1_000_000.0
        );
    }
    let now_ns = lock_wait_now_ns();
    let last_ns = stats.last_report_ns.load(Ordering::Relaxed);
    if now_ns.saturating_sub(last_ns) < LOCK_WAIT_REPORT_INTERVAL_NS {
        return;
    }
    if stats
        .last_report_ns
        .compare_exchange(last_ns, now_ns, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let lock_count = stats.lock_count.swap(0, Ordering::Relaxed);
    if lock_count == 0 {
        return;
    }
    let total_ns = stats.wait_ns_total.swap(0, Ordering::Relaxed);
    let max_ns = stats.wait_ns_max.swap(0, Ordering::Relaxed);
    let slow_count = stats.slow_wait_count.swap(0, Ordering::Relaxed);
    let avg_us = (total_ns as f64 / lock_count as f64) / 1_000.0;
    debug!(
        "lock-wait[{lock_name}] n={} avg={avg_us:.3}us max={:.3}us slow(>50us)={}",
        lock_count,
        max_ns as f64 / 1_000.0,
        slow_count
    );
}

#[inline(always)]
pub(super) fn lock_config() -> std::sync::MutexGuard<'static, Config> {
    if !lock_wait_stats_enabled() {
        return CONFIG.lock().unwrap();
    }
    let start = Instant::now();
    let guard = CONFIG.lock().unwrap();
    let waited_ns = start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    record_lock_wait("CONFIG", &CONFIG_LOCK_WAIT_STATS, waited_ns);
    guard
}

#[inline(always)]
pub(super) fn sync_audio_mix_levels_from_config(cfg: &Config) {
    AUDIO_MIX_LEVELS_PACKED.store(
        pack_audio_mix_levels(
            cfg.master_volume,
            cfg.music_volume,
            cfg.sfx_volume,
            cfg.assist_tick_volume,
        ),
        Ordering::Release,
    );
}

enum SaveReq {
    Write(String),
    Flush(mpsc::Sender<()>),
}

fn start_save_worker() -> Option<mpsc::Sender<SaveReq>> {
    let (tx, rx) = mpsc::channel::<SaveReq>();
    let spawn = thread::Builder::new()
        .name("deadsync-config-save".to_string())
        .spawn(move || save_worker_loop(rx));
    match spawn {
        Ok(_) => Some(tx),
        Err(e) => {
            warn!("Failed to start config save worker thread: {e}. Falling back to sync writes.");
            None
        }
    }
}

#[inline(always)]
pub(super) fn queue_save_write(content: String) {
    if let Some(tx) = SAVE_TX.as_ref() {
        if let Err(err) = tx.send(SaveReq::Write(content))
            && let SaveReq::Write(content) = err.0
        {
            write_config_file(&content);
        }
        return;
    }
    write_config_file(&content);
}

fn save_worker_loop(rx: mpsc::Receiver<SaveReq>) {
    let mut pending_write: Option<String> = None;
    let mut flush_acks: Vec<mpsc::Sender<()>> = Vec::with_capacity(2);
    while let Ok(msg) = rx.recv() {
        match msg {
            SaveReq::Write(content) => pending_write = Some(content),
            SaveReq::Flush(ack) => flush_acks.push(ack),
        }
        while let Ok(msg) = rx.try_recv() {
            match msg {
                SaveReq::Write(content) => pending_write = Some(content),
                SaveReq::Flush(ack) => flush_acks.push(ack),
            }
        }
        if let Some(content) = pending_write.take() {
            write_config_file(&content);
        }
        for ack in flush_acks.drain(..) {
            let _ = ack.send(());
        }
    }
    if let Some(content) = pending_write.take() {
        write_config_file(&content);
    }
}

#[inline(always)]
fn write_config_file(content: &str) {
    if let Err(e) = std::fs::write(deadsync_platform::dirs::app_dirs().config_path(), content) {
        warn!("Failed to save config file: {e}");
    }
}

pub fn flush_pending_saves() {
    if let Some(tx) = SAVE_TX.as_ref() {
        let (ack_tx, ack_rx) = mpsc::channel::<()>();
        if tx.send(SaveReq::Flush(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(Duration::from_secs(5));
        }
    }
}

pub fn get() -> Config {
    *lock_config()
}

pub fn audio_mix_levels() -> AudioMixLevels {
    unpack_audio_mix_levels(AUDIO_MIX_LEVELS_PACKED.load(Ordering::Acquire))
}

pub fn machine_default_noteskin() -> String {
    MACHINE_DEFAULT_NOTESKIN.lock().unwrap().clone()
}

/// The saved SMX pad → player serial assignment: `(p1_serial, p2_serial)`.
/// Either side is `None` when not assigned (that side follows the jumper).
pub fn smx_pad_assignment() -> (Option<String>, Option<String>) {
    (
        SMX_P1_SERIAL.lock().unwrap().clone(),
        SMX_P2_SERIAL.lock().unwrap().clone(),
    )
}

pub fn additional_song_folder_roots() -> Vec<AdditionalSongFolder> {
    ADDITIONAL_SONG_FOLDERS.lock().unwrap().clone()
}

pub fn song_path_is_writable(path: &Path) -> bool {
    let roots = ADDITIONAL_SONG_FOLDERS.lock().unwrap().clone();
    song_path_is_writable_for_roots(path, &roots)
}

fn song_path_is_writable_for_roots(path: &Path, roots: &[AdditionalSongFolder]) -> bool {
    let path = canonical_or_raw(path);
    let mut best: Option<(usize, bool)> = None;
    for root in roots {
        let root_path = canonical_or_raw(Path::new(root.path.as_str()));
        let Some(len) = root_prefix_len(path.as_path(), root_path.as_path()) else {
            continue;
        };
        if best.is_none_or(|(best_len, _)| len >= best_len) {
            best = Some((len, root.writable));
        }
    }
    best.map_or(true, |(_, writable)| writable)
}

fn canonical_or_raw(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn root_prefix_len(path: &Path, root: &Path) -> Option<usize> {
    let mut path_components = path.components();
    let mut len = 0usize;
    for root_component in root.components() {
        let path_component = path_components.next()?;
        if !path_components_equal(path_component.as_os_str(), root_component.as_os_str()) {
            return None;
        }
        len += 1;
    }
    Some(len)
}

#[cfg(windows)]
fn path_components_equal(a: &std::ffi::OsStr, b: &std::ffi::OsStr) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

#[cfg(not(windows))]
fn path_components_equal(a: &std::ffi::OsStr, b: &std::ffi::OsStr) -> bool {
    a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(path: &str, writable: bool) -> AdditionalSongFolder {
        AdditionalSongFolder {
            path: path.to_string(),
            writable,
        }
    }

    #[test]
    fn song_path_writable_defaults_to_true_outside_additional_roots() {
        assert!(song_path_is_writable_for_roots(
            Path::new("Songs/Pack/song.ssc"),
            &[folder("ExtraSongs", false)]
        ));
    }

    #[test]
    fn song_path_writable_rejects_read_only_additional_root() {
        assert!(!song_path_is_writable_for_roots(
            Path::new("ExtraSongs/Pack/song.ssc"),
            &[folder("ExtraSongs", false)]
        ));
    }

    #[test]
    fn song_path_writable_prefers_longest_matching_root() {
        assert!(song_path_is_writable_for_roots(
            Path::new("ExtraSongs/WritablePack/song.ssc"),
            &[
                folder("ExtraSongs", false),
                folder("ExtraSongs/WritablePack", true),
            ]
        ));
    }

    #[test]
    fn root_prefix_does_not_match_partial_directory_names() {
        assert!(song_path_is_writable_for_roots(
            Path::new("ExtraSongs2/Pack/song.ssc"),
            &[folder("ExtraSongs", false)]
        ));
    }
}
