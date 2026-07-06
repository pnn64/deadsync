use super::{AdditionalSongFolder, AudioMixLevels, Config, DEFAULT_MACHINE_NOTESKIN};
use deadlib_platform::coalesced_write::CoalescedFileWriter;
use deadlib_platform::lock_wait::{LockWaitStats, lock_mutex};
use deadsync_config::cache::group_is_never_cached as group_is_never_cached_in_list;
use deadsync_config::folders::song_path_is_writable_for_roots;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

// Global, mutable configuration instance.
static CONFIG: std::sync::LazyLock<Mutex<Config>> =
    std::sync::LazyLock::new(|| Mutex::new(Config::default()));
pub(super) static MACHINE_DEFAULT_NOTESKIN: std::sync::LazyLock<Mutex<String>> =
    std::sync::LazyLock::new(|| Mutex::new(DEFAULT_MACHINE_NOTESKIN.to_string()));
pub(super) static ADDITIONAL_SONG_FOLDERS: std::sync::LazyLock<Mutex<Vec<AdditionalSongFolder>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));
/// Pack group (song-folder) names that must never be cached to disk. Stored
/// outside the `Copy` `Config` because the entries are owned strings. Mirrors
/// ITGMania's `NeverCacheList` preference — useful for WIP packs whose files
/// change frequently and would otherwise serve stale cached data.
pub(super) static NEVER_CACHE_LIST: std::sync::LazyLock<Mutex<Vec<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));
/// SMX pad → player serial assignment (slot 0 = P1, slot 1 = P2). Stored outside
/// the `Copy` `Config` because serials are owned strings. `None` = follow jumper.
pub(super) static SMX_P1_SERIAL: std::sync::LazyLock<Mutex<Option<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));
pub(super) static SMX_P2_SERIAL: std::sync::LazyLock<Mutex<Option<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));
/// Default local profile id per side (slot 0 = P1, slot 1 = P2). Stored outside
/// the `Copy` `Config` because ids are owned strings. `None` = that side uses
/// Guest unless the player chooses a local profile.
pub(super) static DEFAULT_PROFILE_P1: std::sync::LazyLock<Mutex<Option<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));
pub(super) static DEFAULT_PROFILE_P2: std::sync::LazyLock<Mutex<Option<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));
static SAVE_WRITER: std::sync::LazyLock<CoalescedFileWriter> = std::sync::LazyLock::new(|| {
    CoalescedFileWriter::new(
        "deadsync-config-save",
        deadlib_platform::dirs::app_dirs().config_path(),
    )
});

static CONFIG_LOCK_WAIT_STATS: LockWaitStats = LockWaitStats::new();

#[inline(always)]
pub(super) fn lock_config() -> std::sync::MutexGuard<'static, Config> {
    lock_mutex("CONFIG", &CONFIG, &CONFIG_LOCK_WAIT_STATS)
}

#[inline(always)]
pub(super) fn sync_audio_mix_levels_from_config(cfg: &Config) {
    deadsync_audio::set_audio_mix_levels(AudioMixLevels {
        master_volume: cfg.master_volume,
        music_volume: cfg.music_volume,
        sfx_volume: cfg.sfx_volume,
        assist_tick_volume: cfg.assist_tick_volume,
    });
}

#[inline(always)]
pub(super) fn queue_save_write(content: String) {
    SAVE_WRITER.write(content);
}

pub fn flush_pending_saves() {
    SAVE_WRITER.flush(Duration::from_secs(5));
}

pub fn get() -> Config {
    *lock_config()
}

pub fn audio_mix_levels() -> AudioMixLevels {
    deadsync_audio::audio_mix_levels()
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

/// The default local profile id per side: `(p1, p2)`. Either side is `None`
/// when it should default to Guest.
pub fn default_profiles() -> (Option<String>, Option<String>) {
    (
        DEFAULT_PROFILE_P1.lock().unwrap().clone(),
        DEFAULT_PROFILE_P2.lock().unwrap().clone(),
    )
}

pub fn additional_song_folder_roots() -> Vec<AdditionalSongFolder> {
    ADDITIONAL_SONG_FOLDERS.lock().unwrap().clone()
}

/// The configured `NeverCacheList` pack group names.
pub fn never_cache_list() -> Vec<String> {
    NEVER_CACHE_LIST.lock().unwrap().clone()
}

/// Returns true when the given pack group (song-folder) name is listed in
/// `NeverCacheList` and therefore must skip the on-disk song cache entirely
/// (both reads and writes). Matching is case-insensitive and ignores
/// surrounding whitespace.
pub fn group_is_never_cached(group: &str) -> bool {
    group_is_never_cached_in_list(NEVER_CACHE_LIST.lock().unwrap().as_slice(), group)
}

pub fn song_path_is_writable(path: &Path) -> bool {
    let roots = ADDITIONAL_SONG_FOLDERS.lock().unwrap().clone();
    song_path_is_writable_for_roots(path, &roots)
}
