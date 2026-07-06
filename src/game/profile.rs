//! Local profile storage and the active-session profile state.
//!
//! Identity model: a profile's canonical id is the `Guid` embedded under
//! `[userprofile]` in its `profile.ini`. The on-disk folder name is cosmetic
//! (derived from the display name) and may change freely. Profiles are resolved
//! by GUID via [`resolve_profile_dir`], backed by the [`PROFILE_DIR_CACHE`] map.
//!
//! Cache invariant: any code that creates, deletes, renames, or backfills a
//! profile folder MUST call [`invalidate_profile_dir_cache`] afterwards, because
//! a cache miss is treated as authoritative (no rescan on miss).
use crate::config::{self, SimpleIni};
use chrono::Local;
use deadlib_platform::dirs;
use deadsync_rules::scroll::{GUEST_SCROLL_SPEED, ScrollSpeedSetting};
use log::{debug, info, warn};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

mod update;

use deadsync_profile::{
    ActiveProfile, GameplayHudPlayerSnapshot, GameplayHudSnapshot, LastPlayed, LastPlayedCourse,
    LocalProfileSummary, NoteSkin, PLAYER_SLOTS, PlayMode, PlayStyle, PlayerOptionsData,
    PlayerSide, Profile, ProfileStats, ProfileStatsDecodeError, TimingTickMode,
    active_profile_is_guest, active_profile_local_id, add_known_pack_names, clamp_weight_pounds,
    decode_profile_stats as decode_profile_stats_bytes, encode_profile_stats,
    find_profile_avatar_path, folder_name_for_display, generate_profile_guid, initials_from_name,
    is_local_profile_id, is_valid_profile_guid, joined_player_mask,
    load_last_played_course_section, load_last_played_section, load_player_options_section,
    parse_favorited_packs_content, parse_favorites_content, parse_groovestats_is_pad_player,
    player_options_section, player_side_index as side_ix, player_side_is_joined,
    read_userprofile_identity, render_arrowcloud_ini_content, render_favorited_packs_content,
    render_favorites_content, render_groovestats_ini_content, render_profile_ini_content,
    sanitize_player_initials, unknown_pack_names, upsert_profile_guid_content,
};
pub use update::*;

#[inline(always)]
fn profiles_root() -> PathBuf {
    dirs::app_dirs().profiles_root()
}

/// Path for a literal folder name, bypassing GUID resolution (creation only).
#[inline(always)]
fn profile_dir_by_folder(folder: &str) -> PathBuf {
    profiles_root().join(folder)
}

/// Lazily-built GUID -> folder map. Rebuilt only when invalidated; every
/// mutation of the profiles directory (create/delete/rename/backfill) MUST call
/// `invalidate_profile_dir_cache()` so the next lookup re-scans.
static PROFILE_DIR_CACHE: Mutex<Option<HashMap<String, PathBuf>>> = Mutex::new(None);

fn build_profile_dir_map() -> HashMap<String, PathBuf> {
    deadsync_profile::build_profile_dir_map(&profiles_root(), |guid, left, right, kept| {
        warn!(
            "Duplicate profile GUID {} in '{}' and '{}'; using '{}'.",
            guid,
            left.display(),
            right.display(),
            kept.display()
        );
    })
}

/// Drop the cached map so the next lookup rescans disk.
fn invalidate_profile_dir_cache() {
    *PROFILE_DIR_CACHE.lock().unwrap() = None;
}

/// Resolve an embedded GUID to its on-disk folder via the cached map. Misses are
/// authoritative (the map is invalidated on every mutation), so this never
/// rescans on a miss.
fn resolve_profile_dir(guid: &str) -> Option<PathBuf> {
    let mut guard = PROFILE_DIR_CACHE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(build_profile_dir_map());
    }
    let path = guard.as_ref().unwrap().get(guid).cloned();
    drop(guard);
    path
}

/// Folder for a profile id. Ids that aren't valid GUIDs (the guest/default seed,
/// legacy folder-name ids) skip the cache and fall back to a literal folder,
/// which also covers freshly created or not-yet-migrated profiles.
#[inline(always)]
fn local_profile_dir(id: &str) -> PathBuf {
    if is_valid_profile_guid(id)
        && let Some(dir) = resolve_profile_dir(id)
    {
        return dir;
    }
    profile_dir_by_folder(id)
}

#[inline(always)]
pub fn local_profile_dir_for_id(id: &str) -> PathBuf {
    local_profile_dir(id)
}

fn existing_profile_folder_names() -> Vec<String> {
    deadsync_profile::profile_folder_names(&profiles_root())
}

#[inline(always)]
fn profile_ini_path(id: &str) -> PathBuf {
    deadsync_profile::profile_ini_path(&local_profile_dir(id))
}

#[inline(always)]
fn groovestats_ini_path(id: &str) -> PathBuf {
    deadsync_profile::groovestats_ini_path(&local_profile_dir(id))
}

#[inline(always)]
fn arrowcloud_ini_path(id: &str) -> PathBuf {
    deadsync_profile::arrowcloud_ini_path(&local_profile_dir(id))
}

#[inline(always)]
fn profile_stats_path(id: &str) -> PathBuf {
    deadsync_profile::profile_stats_path(&local_profile_dir(id))
}

#[inline(always)]
fn load_player_options(
    profile_conf: &SimpleIni,
    section: &str,
    default: &PlayerOptionsData,
) -> Option<PlayerOptionsData> {
    let has_any = profile_conf
        .get_section(section)
        .is_some_and(|s| !s.is_empty());
    load_player_options_section(has_any, |key| profile_conf.get(section, key), default)
}

#[inline(always)]
fn load_last_played(
    profile_conf: &SimpleIni,
    section: &str,
    default: &LastPlayed,
) -> Option<LastPlayed> {
    let has_any = profile_conf
        .get_section(section)
        .is_some_and(|s| !s.is_empty());
    load_last_played_section(has_any, |key| profile_conf.get(section, key), default)
}

#[inline(always)]
fn load_last_played_course(profile_conf: &SimpleIni, section: &str) -> Option<LastPlayedCourse> {
    let has_any = profile_conf
        .get_section(section)
        .is_some_and(|s| !s.is_empty());
    load_last_played_course_section(has_any, |key| profile_conf.get(section, key))
}

#[inline(always)]
fn profile_stats_tmp_path(id: &str) -> PathBuf {
    deadsync_profile::profile_stats_tmp_path(&local_profile_dir(id))
}

// Global statics for the loaded player profiles.
static PROFILES: std::sync::LazyLock<Mutex<[Profile; PLAYER_SLOTS]>> =
    std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| Profile::default())));

#[derive(Debug)]
struct SessionState {
    active_profiles: [ActiveProfile; PLAYER_SLOTS],
    joined_mask: u8,
    music_rate: f32,
    timing_tick_mode: TimingTickMode,
    play_style: PlayStyle,
    play_mode: PlayMode,
    player_side: PlayerSide,
    fast_profile_switch_from_select_music: bool,
}

static SESSION: std::sync::LazyLock<Mutex<SessionState>> = std::sync::LazyLock::new(|| {
    Mutex::new(SessionState {
        // Both sides start as Guest; `restore_default_profiles()` seeds the
        // real active profiles from config during `load()`.
        active_profiles: [ActiveProfile::Guest, ActiveProfile::Guest],
        joined_mask: joined_player_mask(true, false),
        music_rate: 1.0,
        timing_tick_mode: TimingTickMode::Off,
        play_style: PlayStyle::Single,
        play_mode: PlayMode::Regular,
        player_side: PlayerSide::P1,
        fast_profile_switch_from_select_music: false,
    })
});

static LOCK_WAIT_EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
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

static SESSION_LOCK_WAIT_STATS: LockWaitStats = LockWaitStats::new();
static PROFILES_LOCK_WAIT_STATS: LockWaitStats = LockWaitStats::new();

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
fn lock_session() -> std::sync::MutexGuard<'static, SessionState> {
    if !lock_wait_stats_enabled() {
        return SESSION.lock().unwrap();
    }
    let start = Instant::now();
    let guard = SESSION.lock().unwrap();
    let waited_ns = start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    record_lock_wait("SESSION", &SESSION_LOCK_WAIT_STATS, waited_ns);
    guard
}

#[inline(always)]
fn lock_profiles() -> std::sync::MutexGuard<'static, [Profile; PLAYER_SLOTS]> {
    if !lock_wait_stats_enabled() {
        return PROFILES.lock().unwrap();
    }
    let start = Instant::now();
    let guard = PROFILES.lock().unwrap();
    let waited_ns = start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    record_lock_wait("PROFILES", &PROFILES_LOCK_WAIT_STATS, waited_ns);
    guard
}

#[inline(always)]
fn session_side_is_guest(side: PlayerSide) -> bool {
    active_profile_is_guest(&lock_session().active_profiles[side_ix(side)])
}

#[inline(always)]
fn machine_default_noteskin_value() -> NoteSkin {
    NoteSkin::new(&config::machine_default_noteskin())
}

/// Machine-default pad-light brightness used to seed a new profile, mirroring
/// `machine_default_noteskin_value`. Players adjust their own value afterwards.
#[inline(always)]
fn machine_default_light_brightness() -> u8 {
    config::get().smx_default_light_brightness
}

pub fn machine_default_noteskin() -> NoteSkin {
    machine_default_noteskin_value()
}

pub fn update_machine_default_noteskin(setting: NoteSkin) {
    if config::machine_default_noteskin().eq_ignore_ascii_case(setting.as_str()) {
        return;
    }
    config::update_machine_default_noteskin(setting.as_str());
    {
        let session = lock_session();
        let mut profiles = lock_profiles();
        for side in [PlayerSide::P1, PlayerSide::P2] {
            if active_profile_is_guest(&session.active_profiles[side_ix(side)]) {
                let profile = &mut profiles[side_ix(side)];
                profile.noteskin = setting.clone();
                profile.player_options_singles.noteskin = setting.clone();
                profile.player_options_doubles.noteskin = setting.clone();
            }
        }
    }
}

fn make_guest_profile() -> Profile {
    let mut guest = Profile::default();
    guest.display_name = "[ GUEST ]".to_string();
    guest.scroll_speed = GUEST_SCROLL_SPEED;
    guest.noteskin = machine_default_noteskin_value();
    guest.pad_light_brightness = machine_default_light_brightness();
    guest.avatar_path = None;
    guest.avatar_texture_key = None;
    guest.store_current_player_options_for_all_styles();
    guest
}

fn ensure_local_profile_files(id: &str) -> Result<(), std::io::Error> {
    let dir = local_profile_dir(id);
    let profile_ini = profile_ini_path(id);
    let groovestats_ini = groovestats_ini_path(id);
    let arrowcloud_ini = arrowcloud_ini_path(id);

    info!(
        "Profile files not found, creating defaults in '{}'.",
        dir.display()
    );
    fs::create_dir_all(&dir)?;

    // Create profile.ini
    if !profile_ini.exists() {
        let mut default_profile = Profile::default();
        default_profile.noteskin = machine_default_noteskin_value();
        default_profile.pad_light_brightness = machine_default_light_brightness();
        default_profile.store_current_player_options_for_all_styles();
        default_profile.calories_burned_day = Local::now().date_naive().to_string();

        fs::write(
            profile_ini,
            render_profile_ini_content(id, &default_profile),
        )?;
    }

    // Create groovestats.ini
    if !groovestats_ini.exists() {
        fs::write(
            groovestats_ini,
            render_groovestats_ini_content("", false, ""),
        )?;
    }

    // Create arrowcloud.ini
    if !arrowcloud_ini.exists() {
        fs::write(arrowcloud_ini, render_arrowcloud_ini_content(""))?;
    }

    // A new folder may have appeared; let the resolver pick it up.
    invalidate_profile_dir_cache();
    Ok(())
}

fn save_profile_ini_for_side(side: PlayerSide) {
    let profile_id = {
        let session = lock_session();
        match &session.active_profiles[side_ix(side)] {
            ActiveProfile::Local { id } => Some(id.clone()),
            ActiveProfile::Guest => None,
        }
    };
    let Some(profile_id) = profile_id else {
        return;
    };

    let play_style = get_session_play_style();
    let profile = {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        profile.store_current_player_options(play_style);
        profile.clone()
    };
    let path = profile_ini_path(&profile_id);
    if let Err(e) = fs::write(&path, render_profile_ini_content(&profile_id, &profile)) {
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

#[inline(always)]
fn decode_profile_stats(bytes: &[u8], path: &Path) -> Option<ProfileStats> {
    match decode_profile_stats_bytes(bytes) {
        Ok(stats) => Some(stats),
        Err(ProfileStatsDecodeError::UnsupportedVersion(version)) => {
            warn!(
                "Unsupported profile stats version {} in '{}'.",
                version,
                path.display()
            );
            None
        }
        Err(ProfileStatsDecodeError::InvalidPayload) => {
            warn!("Failed to decode profile stats '{}'.", path.display());
            None
        }
    }
}

fn load_profile_stats(path: &Path) -> Option<ProfileStats> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!("Failed to read {}: {}", path.display(), e);
            }
            return None;
        }
    };
    decode_profile_stats(&bytes, path)
}

fn save_profile_stats_for_side(side: PlayerSide) {
    let maybe_payload = {
        let session = lock_session();
        match &session.active_profiles[side_ix(side)] {
            ActiveProfile::Local { id } => {
                let profile = lock_profiles()[side_ix(side)].clone();
                Some((
                    id.clone(),
                    ProfileStats {
                        current_combo: profile.current_combo,
                        known_pack_names: profile.known_pack_names,
                    },
                ))
            }
            ActiveProfile::Guest => None,
        }
    };
    let Some((profile_id, payload)) = maybe_payload else {
        return;
    };
    write_profile_stats(&profile_id, &payload);
}

fn write_profile_stats(profile_id: &str, payload: &ProfileStats) {
    let Some(buf) = encode_profile_stats(payload) else {
        warn!("Failed to encode profile stats for '{}'.", profile_id);
        return;
    };

    let path = profile_stats_path(profile_id);
    let tmp_path = profile_stats_tmp_path(profile_id);
    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        warn!(
            "Failed to create profile stats directory '{}': {}",
            parent.display(),
            e
        );
        return;
    }
    if let Err(e) = fs::write(&tmp_path, buf) {
        warn!("Failed to write {}: {}", tmp_path.display(), e);
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, &path) {
        warn!("Failed to save {}: {}", path.display(), e);
        let _ = fs::remove_file(&tmp_path);
    }
}

/// Writes [`ProfileStats`] for a freshly-imported profile that isn't loaded into
/// a player side (used by the ITGmania importer). `known_pack_names` is left
/// empty so the normal first-load reconciliation still marks the current packs
/// as known. Does nothing when there's no stat worth persisting.
pub fn write_imported_profile_stats(profile_id: &str, current_combo: u32) {
    if current_combo == 0 {
        return;
    }
    write_profile_stats(
        profile_id,
        &ProfileStats {
            current_combo,
            known_pack_names: HashSet::new(),
        },
    );
}

fn save_groovestats_ini_for_side(side: PlayerSide) {
    let profile_id = {
        let session = lock_session();
        match &session.active_profiles[side_ix(side)] {
            ActiveProfile::Local { id } => Some(id.clone()),
            ActiveProfile::Guest => None,
        }
    };
    let Some(profile_id) = profile_id else {
        return;
    };

    let profile = lock_profiles()[side_ix(side)].clone();

    let path = groovestats_ini_path(&profile_id);
    if let Err(e) = fs::write(
        &path,
        render_groovestats_ini_content(
            &profile.groovestats_api_key,
            profile.groovestats_is_pad_player,
            &profile.groovestats_username,
        ),
    ) {
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

fn save_arrowcloud_ini_for_side(side: PlayerSide) {
    let profile_id = {
        let session = lock_session();
        match &session.active_profiles[side_ix(side)] {
            ActiveProfile::Local { id } => Some(id.clone()),
            ActiveProfile::Guest => None,
        }
    };
    let Some(profile_id) = profile_id else {
        return;
    };

    let profile = lock_profiles()[side_ix(side)].clone();

    let path = arrowcloud_ini_path(&profile_id);
    if let Err(e) = fs::write(
        &path,
        render_arrowcloud_ini_content(&profile.arrowcloud_api_key),
    ) {
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

/// Update the active profile's ArrowCloud API key (in memory + on disk).
/// No-op when the side has no local profile loaded (Guest).
pub fn set_arrowcloud_api_key_for_side(side: PlayerSide, api_key: &str) {
    {
        let mut profiles = lock_profiles();
        profiles[side_ix(side)].arrowcloud_api_key = api_key.to_string();
    }
    save_arrowcloud_ini_for_side(side);
}

/// Write a new ArrowCloud API key for a profile identified by ID
/// (independent of session sides).  Used by the Manage Local Profiles
/// "Link ArrowCloud" flow where the user picks a profile that isn't
/// necessarily joined on P1 or P2.  Also refreshes the in-memory copy
/// on any session side currently loading that profile, so other screens
/// see the new key immediately.
pub fn set_arrowcloud_api_key_for_id(profile_id: &str, api_key: &str) {
    // Update any session side currently bound to this profile id.
    let matching_sides: Vec<PlayerSide> = {
        let session = lock_session();
        [PlayerSide::P1, PlayerSide::P2]
            .iter()
            .copied()
            .filter(|side| {
                matches!(
                    &session.active_profiles[side_ix(*side)],
                    ActiveProfile::Local { id } if id == profile_id
                )
            })
            .collect()
    };
    if !matching_sides.is_empty() {
        let mut profiles = lock_profiles();
        for side in &matching_sides {
            profiles[side_ix(*side)].arrowcloud_api_key = api_key.to_string();
        }
    }

    // Persist directly to that profile's ArrowCloud.ini, even if the
    // profile isn't loaded on any side right now.
    let path = arrowcloud_ini_path(profile_id);
    if let Err(e) = fs::write(&path, render_arrowcloud_ini_content(api_key)) {
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

/// Returns the saved ArrowCloud API key (from disk) for a profile
/// identified by id, regardless of whether it's currently loaded on a
/// session side.  Empty string if the profile has no key yet or the
/// file is missing / malformed.
pub fn get_arrowcloud_api_key_for_id(profile_id: &str) -> String {
    let path = arrowcloud_ini_path(profile_id);
    let Ok(text) = fs::read_to_string(&path) else {
        return String::new();
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("ApiKey=") {
            return rest.trim().to_string();
        }
        if let Some(rest) = line.strip_prefix("ApiKey =") {
            return rest.trim().to_string();
        }
    }
    String::new()
}

/// Update the active profile's GrooveStats credentials (API key,
/// username, and `IsPadPlayer=true` — Simply Love parity, see
/// `BGAnimations/ScreenGrooveStatsLogin underlay/default.lua:46`) for
/// the given session side, persisting to its `GrooveStats.ini` on disk.
/// No-op when the side has no local profile loaded (Guest).
pub fn set_groovestats_credentials_for_side(side: PlayerSide, api_key: &str, username: &str) {
    {
        let mut profiles = lock_profiles();
        let p = &mut profiles[side_ix(side)];
        p.groovestats_api_key = api_key.to_string();
        p.groovestats_username = username.to_string();
        p.groovestats_is_pad_player = true;
    }
    save_groovestats_ini_for_side(side);
}

/// Write new GrooveStats credentials for a profile identified by ID
/// (independent of session sides).  Used by the Manage Local Profiles
/// "Link GrooveStats" flow.  Also refreshes the in-memory copy on any
/// session side currently bound to that profile id.
pub fn set_groovestats_credentials_for_id(profile_id: &str, api_key: &str, username: &str) {
    let matching_sides: Vec<PlayerSide> = {
        let session = lock_session();
        [PlayerSide::P1, PlayerSide::P2]
            .iter()
            .copied()
            .filter(|side| {
                matches!(
                    &session.active_profiles[side_ix(*side)],
                    ActiveProfile::Local { id } if id == profile_id
                )
            })
            .collect()
    };
    if !matching_sides.is_empty() {
        let mut profiles = lock_profiles();
        for side in &matching_sides {
            let p = &mut profiles[side_ix(*side)];
            p.groovestats_api_key = api_key.to_string();
            p.groovestats_username = username.to_string();
            p.groovestats_is_pad_player = true;
        }
    }

    // Persist directly to that profile's GrooveStats.ini, even if the
    // profile isn't loaded on any side right now.
    let path = groovestats_ini_path(profile_id);
    if let Err(e) = fs::write(
        &path,
        render_groovestats_ini_content(api_key, true, username),
    ) {
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

/// Returns the saved GrooveStats API key (from disk) for a profile
/// identified by id, regardless of whether it's currently loaded on a
/// session side.  `None` if the profile has no key yet or the file is
/// missing / malformed; `Some` always wraps a non-empty trimmed key.
pub fn get_groovestats_api_key_for_id(profile_id: &str) -> Option<String> {
    let path = groovestats_ini_path(profile_id);
    let text = fs::read_to_string(&path).ok()?;
    for line in text.lines() {
        let line = line.trim();
        let rest = line
            .strip_prefix("ApiKey=")
            .or_else(|| line.strip_prefix("ApiKey ="));
        if let Some(rest) = rest {
            let key = rest.trim();
            if key.is_empty() {
                return None;
            }
            return Some(key.to_string());
        }
    }
    None
}

fn load_for_side(side: PlayerSide) {
    let profile_id = {
        let session = lock_session();
        match &session.active_profiles[side_ix(side)] {
            ActiveProfile::Local { id } => Some(id.clone()),
            ActiveProfile::Guest => None,
        }
    };

    // If the requested profile folder no longer exists (e.g. the user renamed
    // the default folder on disk), fall back to the first available local
    // profile or Guest.
    let profile_id = match profile_id {
        Some(id) if !local_profile_dir(&id).is_dir() => {
            let fallback = scan_local_profiles().into_iter().next().map(|p| p.id);
            if let Some(ref fb_id) = fallback {
                info!("Profile folder '{id}' not found; falling back to '{fb_id}'.");
                let mut session = lock_session();
                session.active_profiles[side_ix(side)] = ActiveProfile::Local { id: fb_id.clone() };
            } else {
                info!("Profile folder '{id}' not found and no other profiles exist; using Guest.");
                let mut session = lock_session();
                session.active_profiles[side_ix(side)] = ActiveProfile::Guest;
            }
            fallback
        }
        other => other,
    };

    let Some(profile_id) = profile_id else {
        let mut profiles = lock_profiles();
        profiles[side_ix(side)] = make_guest_profile();
        return;
    };

    let profile_ini = profile_ini_path(&profile_id);
    let groovestats_ini = groovestats_ini_path(&profile_id);
    let arrowcloud_ini = arrowcloud_ini_path(&profile_id);
    if (!profile_ini.exists() || !groovestats_ini.exists() || !arrowcloud_ini.exists())
        && let Err(e) = ensure_local_profile_files(&profile_id)
    {
        warn!("Failed to create default profile files: {e}");
        // Proceed with default struct values and attempt to save them.
    }

    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        let mut default_profile = Profile::default();
        default_profile.noteskin = machine_default_noteskin_value();
        default_profile.pad_light_brightness = machine_default_light_brightness();
        default_profile.store_current_player_options_for_all_styles();

        // Load profile.ini
        let mut profile_conf = SimpleIni::new();
        if profile_conf.load(&profile_ini).is_ok() {
            profile.display_name = profile_conf
                .get("userprofile", "DisplayName")
                .unwrap_or(default_profile.display_name.clone());
            profile.player_initials = profile_conf
                .get("userprofile", "PlayerInitials")
                .map(|initials| sanitize_player_initials(&initials))
                .filter(|initials| !initials.is_empty())
                .unwrap_or(default_profile.player_initials.clone());
            profile.player_options_singles = load_player_options(
                &profile_conf,
                player_options_section(PlayStyle::Single),
                &default_profile.player_options_singles,
            )
            .unwrap_or_else(|| default_profile.player_options_singles.clone());
            profile.player_options_doubles = load_player_options(
                &profile_conf,
                player_options_section(PlayStyle::Double),
                &default_profile.player_options_doubles,
            )
            .unwrap_or_else(|| default_profile.player_options_doubles.clone());
            profile.apply_player_options_for_style(get_session_play_style());

            // Optional last-played sections: keep the legacy [LastPlayed]
            // fallback so older profile.ini files still load cleanly.
            profile.last_played_singles = load_last_played(
                &profile_conf,
                "LastPlayedSingles",
                &default_profile.last_played_singles,
            )
            .or_else(|| {
                load_last_played(
                    &profile_conf,
                    "LastPlayed",
                    &default_profile.last_played_singles,
                )
            })
            .unwrap_or_else(|| default_profile.last_played_singles.clone());
            profile.last_played_doubles = load_last_played(
                &profile_conf,
                "LastPlayedDoubles",
                &default_profile.last_played_doubles,
            )
            .or_else(|| {
                load_last_played(
                    &profile_conf,
                    "LastPlayed",
                    &default_profile.last_played_doubles,
                )
            })
            .unwrap_or_else(|| default_profile.last_played_doubles.clone());
            profile.last_played_course_singles =
                load_last_played_course(&profile_conf, "LastPlayedCourseSingles")
                    .or_else(|| load_last_played_course(&profile_conf, "LastPlayedCourse"))
                    .unwrap_or_else(|| default_profile.last_played_course_singles.clone());
            profile.last_played_course_doubles =
                load_last_played_course(&profile_conf, "LastPlayedCourseDoubles")
                    .or_else(|| load_last_played_course(&profile_conf, "LastPlayedCourse"))
                    .unwrap_or_else(|| default_profile.last_played_course_doubles.clone());

            profile.weight_pounds = profile_conf
                .get("Editable", "WeightPounds")
                .and_then(|s| s.parse::<i32>().ok())
                .map(clamp_weight_pounds)
                .unwrap_or(default_profile.weight_pounds);

            profile.birth_year = profile_conf
                .get("Editable", "BirthYear")
                .and_then(|s| s.parse::<i32>().ok())
                .map(|year| year.max(0))
                .unwrap_or(default_profile.birth_year);

            // Profile stats (ScreenGameOver parity). Keep the legacy [Stats]
            // fallback so older profile.ini files still load cleanly.
            profile.ignore_step_count_calories = profile_conf
                .get("Editable", "IgnoreStepCountCalories")
                .or_else(|| profile_conf.get("Stats", "IgnoreStepCountCalories"))
                .and_then(|s| s.parse::<u8>().ok())
                .map_or(default_profile.ignore_step_count_calories, |v| v != 0);

            let today = Local::now().date_naive().to_string();
            let saved_day = profile_conf
                .get("Stats", "CaloriesBurnedDate")
                .unwrap_or_default();
            let saved_cals = profile_conf
                .get("Stats", "CaloriesBurnedToday")
                .and_then(|s| s.parse::<f32>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(default_profile.calories_burned_today);

            if saved_day.trim() == today {
                profile.calories_burned_day = today;
                profile.calories_burned_today = saved_cals;
            } else {
                profile.calories_burned_day = today;
                profile.calories_burned_today = 0.0;
            }
        } else {
            warn!(
                "Failed to load '{}', using default profile settings.",
                profile_ini.display()
            );
        }

        let stats =
            load_profile_stats(&profile_stats_path(&profile_id)).unwrap_or_else(|| ProfileStats {
                current_combo: default_profile.current_combo,
                known_pack_names: HashSet::new(),
            });
        profile.current_combo = stats.current_combo;
        profile.known_pack_names = stats.known_pack_names;
        profile.favorites = load_favorites(&profile_id);
        profile.favorited_packs = load_favorited_packs(&profile_id);

        // Load groovestats.ini
        let mut gs_conf = SimpleIni::new();
        if gs_conf.load(&groovestats_ini).is_ok() {
            profile.groovestats_api_key = gs_conf
                .get("GrooveStats", "ApiKey")
                .unwrap_or(default_profile.groovestats_api_key.clone());
            let is_pad_player = gs_conf.get("GrooveStats", "IsPadPlayer");
            profile.groovestats_is_pad_player = parse_groovestats_is_pad_player(
                is_pad_player.as_deref(),
                default_profile.groovestats_is_pad_player,
            );
            profile.groovestats_username = gs_conf
                .get("GrooveStats", "Username")
                .unwrap_or(default_profile.groovestats_username);
        } else {
            warn!(
                "Failed to load '{}', using default GrooveStats info.",
                groovestats_ini.display()
            );
        }

        // Load arrowcloud.ini
        let mut ac_conf = SimpleIni::new();
        if ac_conf.load(&arrowcloud_ini).is_ok() {
            profile.arrowcloud_api_key = ac_conf
                .get("ArrowCloud", "ApiKey")
                .unwrap_or(default_profile.arrowcloud_api_key.clone());
        } else {
            warn!(
                "Failed to load '{}', using default ArrowCloud info.",
                arrowcloud_ini.display()
            );
        }

        profile.avatar_path = find_profile_avatar_path(&local_profile_dir(&profile_id));
        profile.avatar_texture_key = None;
    } // Lock is released here.

    save_profile_ini_for_side(side);
    save_profile_stats_for_side(side);
    save_groovestats_ini_for_side(side);
    save_arrowcloud_ini_for_side(side);
    info!("Profile configuration files updated with default values for any missing fields.");
}

pub fn load() {
    migrate_local_profiles();
    restore_default_profiles();
    load_for_side(PlayerSide::P1);
    load_for_side(PlayerSide::P2);
}

/// Translate a stored default-profile id to a canonical GUID: pass valid GUIDs
/// through, map a legacy folder-name id to that folder's GUID, and otherwise
/// keep the value unchanged (e.g. a stale id with no matching folder).
fn heal_default_profile_id(
    stored: Option<String>,
    folder_to_guid: &HashMap<&str, &str>,
) -> Option<String> {
    let s = stored?;
    if is_valid_profile_guid(&s) {
        return Some(s);
    }
    folder_to_guid
        .get(s.as_str())
        .map(|g| (*g).to_string())
        .or(Some(s))
}

/// Idempotent startup migration to embedded-GUID identity. In a single pass over
/// the profiles directory it backfills GUIDs into legacy profiles, rewrites
/// stored default ids that still hold a folder name, renames legacy folders to
/// match their display name (best-effort), and seeds the resolver cache.
fn migrate_local_profiles() {
    struct ProfileEntry {
        /// Folder name as found on disk before any rename (config heal key).
        original_folder: String,
        /// Current folder name (updated if renamed below).
        folder: String,
        guid: String,
        display: Option<String>,
    }

    // Single directory walk: read each profile.ini exactly once, backfilling a
    // GUID when absent.
    let mut entries: Vec<ProfileEntry> = Vec::new();
    let mut taken: Vec<String> = Vec::new();
    if let Ok(read_dir) = fs::read_dir(profiles_root()) {
        for de in read_dir.flatten() {
            if !de.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                continue;
            }
            let path = de.path();
            let Some(folder) = path
                .file_name()
                .and_then(|s| s.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            taken.push(folder.clone());

            let ini_path = path.join("profile.ini");
            let Ok(content) = fs::read_to_string(&ini_path) else {
                continue; // not a profile folder
            };
            let (guid_opt, display) = read_userprofile_identity(&content);
            let guid = match guid_opt {
                Some(g) => g,
                None => {
                    let g = generate_profile_guid();
                    let updated = upsert_profile_guid_content(&content, &g);
                    if let Err(e) = deadsync_profile::write_profile_file_atomic(&ini_path, &updated)
                    {
                        warn!("Failed to backfill GUID for '{}': {e}", path.display());
                        continue;
                    }
                    info!("Assigned profile GUID {g} to '{}'.", path.display());
                    g
                }
            };
            entries.push(ProfileEntry {
                original_folder: folder.clone(),
                folder,
                guid,
                display,
            });
        }
    }

    // Self-heal stored default ids that still reference a legacy folder name.
    let folder_to_guid: HashMap<&str, &str> = entries
        .iter()
        .map(|e| (e.original_folder.as_str(), e.guid.as_str()))
        .collect();
    let (p1, p2) = config::default_profiles();
    let new_p1 = heal_default_profile_id(p1.clone(), &folder_to_guid);
    let new_p2 = heal_default_profile_id(p2.clone(), &folder_to_guid);
    if new_p1 != p1 || new_p2 != p2 {
        info!("Migrated default profile ids to embedded GUIDs.");
        config::update_default_profiles(new_p1, new_p2);
    }

    // Rename legacy folders to match their display name. Identity is the GUID,
    // so this never breaks active bindings; failures are logged and skipped.
    for entry in &mut entries {
        let Some(display) = entry.display.clone() else {
            continue;
        };
        let folder = entry.folder.clone();
        // Take this folder out of `taken` so it doesn't collide with itself
        // (order is irrelevant for collision checks).
        if let Some(pos) = taken.iter().position(|f| *f == folder) {
            taken.swap_remove(pos);
        }
        let desired = folder_name_for_display(&display, &folder, &taken);
        if desired.eq_ignore_ascii_case(&folder) {
            taken.push(folder);
            continue;
        }
        match fs::rename(
            profile_dir_by_folder(&folder),
            profile_dir_by_folder(&desired),
        ) {
            Ok(()) => {
                info!("Renamed profile folder '{folder}' -> '{desired}'.");
                taken.push(desired.clone());
                entry.folder = desired;
            }
            Err(e) => {
                warn!("Failed to rename profile folder '{folder}' -> '{desired}': {e}");
                taken.push(folder);
            }
        }
    }

    // Seed the resolver cache from the post-migration snapshot (dedup duplicate
    // GUIDs by smallest folder name) so the first lookup needn't rescan.
    use std::collections::hash_map::Entry as MapEntry;
    let mut map: HashMap<String, PathBuf> = HashMap::new();
    for e in &entries {
        let path = profile_dir_by_folder(&e.folder);
        match map.entry(e.guid.clone()) {
            MapEntry::Vacant(slot) => {
                slot.insert(path);
            }
            MapEntry::Occupied(mut slot) => {
                if path.file_name() < slot.get().file_name() {
                    slot.insert(path);
                }
            }
        }
    }
    *PROFILE_DIR_CACHE.lock().unwrap() = Some(map);
}

/// Seeds the session's active profiles from the configured default local
/// profiles. Only applies a saved id when it still refers to an existing local
/// profile; otherwise that side starts as Guest.
fn restore_default_profiles() {
    let (p1, p2) = config::default_profiles();
    let mut session = lock_session();
    for (side, saved) in [(PlayerSide::P1, p1), (PlayerSide::P2, p2)] {
        session.active_profiles[side_ix(side)] = default_profile_from_id(saved);
    }
}

fn default_profile_from_id(id: Option<String>) -> ActiveProfile {
    match id {
        Some(id) if is_local_profile_id(&id) && local_profile_dir(&id).is_dir() => {
            ActiveProfile::Local { id }
        }
        _ => ActiveProfile::Guest,
    }
}

/// Returns a copy of the currently loaded profile data.
pub fn get() -> Profile {
    get_for_side(get_session_player_side())
}

pub fn get_for_side(side: PlayerSide) -> Profile {
    lock_profiles()[side_ix(side)].clone()
}

/// Per-player SMX gif pack overrides for both sides (`[P1, P2]` bg packs, then
/// judge packs), each falling back to its machine default when the profile has
/// no override. One lock, no `Profile` clone: called every frame by
/// `App::sync_lights`, so it must stay off the allocation path (`SmxPackName`
/// is a fixed-capacity copy type).
pub fn smx_gif_packs(
    machine_bg: crate::config::SmxPackName,
    machine_judge: crate::config::SmxPackName,
) -> ([crate::config::SmxPackName; 2], [crate::config::SmxPackName; 2]) {
    let profiles = lock_profiles();
    let resolve = |pack: &Option<String>, machine: crate::config::SmxPackName| match pack {
        Some(p) => crate::config::SmxPackName::parse(p),
        None => machine,
    };
    (
        std::array::from_fn(|i| resolve(&profiles[i].smx_bg_pack, machine_bg)),
        std::array::from_fn(|i| resolve(&profiles[i].smx_judge_pack, machine_judge)),
    )
}

pub fn footer_fields_for_side(side: PlayerSide) -> (Option<String>, String) {
    let profiles = lock_profiles();
    let p = &profiles[side_ix(side)];
    (p.avatar_texture_key.clone(), p.display_name.clone())
}

pub fn groovestats_api_key_for_side(side: PlayerSide) -> String {
    lock_profiles()[side_ix(side)]
        .groovestats_api_key
        .trim()
        .to_string()
}

pub fn scorebox_fields_for_side(side: PlayerSide) -> (bool, bool, String, String, String) {
    let profiles = lock_profiles();
    let p = &profiles[side_ix(side)];
    (
        p.display_scorebox,
        p.show_ex_score,
        p.groovestats_api_key.clone(),
        p.arrowcloud_api_key.clone(),
        p.groovestats_username.clone(),
    )
}

pub fn gameplay_hud_snapshot() -> GameplayHudSnapshot {
    let (play_style, player_side, joined_mask, p1_guest, p2_guest) = {
        let session = lock_session();
        (
            session.play_style,
            session.player_side,
            session.joined_mask,
            active_profile_is_guest(&session.active_profiles[side_ix(PlayerSide::P1)]),
            active_profile_is_guest(&session.active_profiles[side_ix(PlayerSide::P2)]),
        )
    };
    let profiles = lock_profiles();
    let p1_profile = &profiles[side_ix(PlayerSide::P1)];
    let p2_profile = &profiles[side_ix(PlayerSide::P2)];
    GameplayHudSnapshot {
        play_style,
        player_side,
        p1: GameplayHudPlayerSnapshot {
            joined: player_side_is_joined(joined_mask, PlayerSide::P1),
            guest: p1_guest,
            display_name: p1_profile.display_name.clone(),
            avatar_texture_key: p1_profile.avatar_texture_key.clone(),
            hide_username: p1_profile.hide_username,
        },
        p2: GameplayHudPlayerSnapshot {
            joined: player_side_is_joined(joined_mask, PlayerSide::P2),
            guest: p2_guest,
            display_name: p2_profile.display_name.clone(),
            avatar_texture_key: p2_profile.avatar_texture_key.clone(),
            hide_username: p2_profile.hide_username,
        },
    }
}

pub fn set_avatar_texture_key_for_side(side: PlayerSide, key: Option<String>) {
    let mut profiles = lock_profiles();
    profiles[side_ix(side)].avatar_texture_key = key;
}

// --- Session helpers ---
pub fn get_active_profile_for_side(side: PlayerSide) -> ActiveProfile {
    lock_session().active_profiles[side_ix(side)].clone()
}

pub fn active_local_profile_id_for_side(side: PlayerSide) -> Option<String> {
    let session = lock_session();
    active_profile_local_id(&session.active_profiles[side_ix(side)]).map(str::to_owned)
}

pub fn get_default_profile_for_side(side: PlayerSide) -> ActiveProfile {
    let (p1, p2) = config::default_profiles();
    default_profile_from_id(match side {
        PlayerSide::P1 => p1,
        PlayerSide::P2 => p2,
    })
}

pub fn default_local_profile_id_for_side(side: PlayerSide) -> Option<String> {
    match get_default_profile_for_side(side) {
        ActiveProfile::Local { id } => Some(id),
        ActiveProfile::Guest => None,
    }
}

pub fn set_default_profile_for_side(side: PlayerSide, profile: ActiveProfile) {
    let mut defaults = {
        let (p1, p2) = config::default_profiles();
        [p1, p2]
    };
    let side_idx = side_ix(side);
    let new_id = active_profile_local_id(&profile)
        .filter(|id| is_local_profile_id(id) && local_profile_dir(id).is_dir())
        .map(str::to_owned);

    if let Some(id) = new_id.as_deref() {
        for (idx, slot) in defaults.iter_mut().enumerate() {
            if idx != side_idx && slot.as_deref() == Some(id) {
                *slot = None;
            }
        }
    }
    defaults[side_idx] = new_id;
    config::update_default_profiles(defaults[0].clone(), defaults[1].clone());
}

fn update_default_profiles_from_selection(p1: &ActiveProfile, p2: &ActiveProfile) {
    let (p1_default, p2_default) = config::default_profiles();
    let mut defaults = [p1_default, p2_default];
    let joined_mask = lock_session().joined_mask;
    for (side, profile) in [(PlayerSide::P1, p1), (PlayerSide::P2, p2)] {
        if !player_side_is_joined(joined_mask, side) {
            continue;
        }
        let side_idx = side_ix(side);
        let new_id = active_profile_local_id(profile).map(str::to_owned);
        if let Some(id) = new_id.as_deref() {
            for (idx, slot) in defaults.iter_mut().enumerate() {
                if idx != side_idx && slot.as_deref() == Some(id) {
                    *slot = None;
                }
            }
        }
        defaults[side_idx] = new_id;
    }
    config::update_default_profiles(defaults[0].clone(), defaults[1].clone());
}

/// The local profile that owns a given physical pad. `is_p2_side` is the pad's
/// player side (P2 vs P1), taken from its SDK slot (slot 1 = P2), NOT the raw
/// hardware jumper bit. In Doubles one player drives both pads, so both map to the
/// joined player's side; otherwise the pad maps to its own side.
pub fn active_local_profile_id_for_pad(is_p2_side: bool) -> Option<String> {
    let side = if get_session_play_style() == PlayStyle::Double {
        get_session_player_side()
    } else if is_p2_side {
        PlayerSide::P2
    } else {
        PlayerSide::P1
    };
    active_local_profile_id_for_side(side)
}

/// Pad-light brightness (0..=100) for the player on a given physical pad slot,
/// using the same side mapping as `active_local_profile_id_for_pad` (Doubles →
/// the one joined player for both pads; otherwise the pad's own side). Reads the
/// active profile's value (guest profiles are seeded from the machine default).
pub fn pad_light_brightness_for_pad(is_p2_side: bool) -> u8 {
    let side = if get_session_play_style() == PlayStyle::Double {
        get_session_player_side()
    } else if is_p2_side {
        PlayerSide::P2
    } else {
        PlayerSide::P1
    };
    lock_profiles()[side_ix(side)].pad_light_brightness
}

pub fn known_pack_names_for_local_profile(profile_id: &str) -> Option<HashSet<String>> {
    let session = lock_session();
    let profiles = lock_profiles();
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let Some(id) = active_profile_local_id(&session.active_profiles[side_ix(side)]) else {
            continue;
        };
        if id == profile_id {
            return Some(profiles[side_ix(side)].known_pack_names.clone());
        }
    }
    None
}

pub fn mark_known_pack_names_for_local_profile<'a>(
    profile_id: &str,
    pack_names: impl IntoIterator<Item = &'a str>,
) {
    let pack_names: Vec<&str> = pack_names.into_iter().collect();
    if profile_id.is_empty() || pack_names.is_empty() {
        return;
    }
    let save_side = {
        let session = lock_session();
        let mut profiles = lock_profiles();
        let mut save_side = None;
        for side in [PlayerSide::P1, PlayerSide::P2] {
            let Some(id) = active_profile_local_id(&session.active_profiles[side_ix(side)]) else {
                continue;
            };
            if id != profile_id {
                continue;
            }
            let profile = &mut profiles[side_ix(side)];
            let changed =
                add_known_pack_names(&mut profile.known_pack_names, pack_names.iter().copied());
            if changed && save_side.is_none() {
                save_side = Some(side);
            }
        }
        save_side
    };
    if let Some(side) = save_side {
        save_profile_stats_for_side(side);
    }
}

pub fn sync_known_packs(profile_ids: &[String], scanned_pack_names: &[String]) -> HashSet<String> {
    if profile_ids.is_empty() {
        return HashSet::new();
    }
    let mut out = HashSet::new();
    for profile_id in profile_ids {
        let known_pack_names = known_pack_names_for_local_profile(profile_id).unwrap_or_default();
        if known_pack_names.is_empty() && !scanned_pack_names.is_empty() {
            mark_known_pack_names_for_local_profile(
                profile_id,
                scanned_pack_names.iter().map(String::as_str),
            );
            continue;
        }
        out.extend(unknown_pack_names(&known_pack_names, scanned_pack_names));
    }
    out
}

pub fn mark_pack_known(profile_ids: &[String], name: &str) {
    mark_packs_known(profile_ids, std::iter::once(name));
}

pub fn mark_packs_known<'a>(profile_ids: &[String], pack_names: impl IntoIterator<Item = &'a str>) {
    let pack_names: Vec<&str> = pack_names.into_iter().collect();
    if profile_ids.is_empty() || pack_names.is_empty() {
        return;
    }
    for profile_id in profile_ids {
        mark_known_pack_names_for_local_profile(profile_id, pack_names.iter().copied());
    }
}

// --- Favorites ---

fn favorites_path(profile_id: &str) -> PathBuf {
    deadsync_profile::favorites_path(&local_profile_dir(profile_id))
}

fn load_favorites(profile_id: &str) -> HashSet<String> {
    let path = favorites_path(profile_id);
    let Ok(text) = fs::read_to_string(&path) else {
        return HashSet::new();
    };
    parse_favorites_content(&text)
}

fn save_favorites(profile_id: &str, favorites: &HashSet<String>) {
    let path = favorites_path(profile_id);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let text = render_favorites_content(favorites);
    let tmp_path = path.with_extension("tmp");
    if fs::write(&tmp_path, text.as_bytes()).is_ok() {
        let _ = fs::rename(&tmp_path, &path);
    }
}

/// Writes imported favorites (chart `short_hash`es) into a freshly-created
/// profile's `favorites.txt`, merging with anything already present. Used by the
/// ITGmania importer, which resolves Simply Love song favorites to chart hashes.
pub fn write_imported_favorites(profile_id: &str, hashes: &HashSet<String>) {
    if hashes.is_empty() {
        return;
    }
    let mut merged = load_favorites(profile_id);
    merged.extend(hashes.iter().cloned());
    save_favorites(profile_id, &merged);
}

/// Toggle a song's favorite status for the given player side.
/// Returns `true` if the song is now a favorite, `false` if removed.
pub fn toggle_favorite(side: PlayerSide, chart_hash: &str) -> bool {
    let Some(profile_id) = active_local_profile_id_for_side(side) else {
        return false;
    };
    let is_now_favorite = {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        if profile.favorites.contains(chart_hash) {
            profile.favorites.remove(chart_hash);
            false
        } else {
            profile.favorites.insert(chart_hash.to_string());
            true
        }
    };
    let favorites = lock_profiles()[side_ix(side)].favorites.clone();
    save_favorites(&profile_id, &favorites);
    is_now_favorite
}

/// Check if a chart hash is favorited for the given player side.
pub fn is_favorite(side: PlayerSide, chart_hash: &str) -> bool {
    let profiles = lock_profiles();
    profiles[side_ix(side)].favorites.contains(chart_hash)
}

/// Test/bench helper: mark a chart hash as favorited for the given side in the
/// in-memory profile only, without persisting to disk. Lets benchmarks exercise
/// the favorites render path deterministically.
pub fn seed_session_favorite(side: PlayerSide, chart_hash: &str) {
    let mut profiles = lock_profiles();
    profiles[side_ix(side)]
        .favorites
        .insert(chart_hash.to_string());
}

fn favorited_packs_path(profile_id: &str) -> PathBuf {
    deadsync_profile::favorited_packs_path(&local_profile_dir(profile_id))
}

fn load_favorited_packs(profile_id: &str) -> HashSet<String> {
    let path = favorited_packs_path(profile_id);
    let Ok(text) = fs::read_to_string(&path) else {
        return HashSet::new();
    };
    parse_favorited_packs_content(&text)
}

fn save_favorited_packs(profile_id: &str, packs: &HashSet<String>) {
    let path = favorited_packs_path(profile_id);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let text = render_favorited_packs_content(packs);
    let tmp_path = path.with_extension("tmp");
    if fs::write(&tmp_path, text.as_bytes()).is_ok() {
        let _ = fs::rename(&tmp_path, &path);
    }
}

/// Toggle a pack's favorite status for the given player side, identifying the
/// pack by its display name. Returns `true` if the pack is
/// now a favorite, `false` if it was removed.
pub fn toggle_pack_favorite(side: PlayerSide, pack_name: &str) -> bool {
    let Some(profile_id) = active_local_profile_id_for_side(side) else {
        return false;
    };
    let is_now_favorite = {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        let existing = profile
            .favorited_packs
            .iter()
            .find(|p| *p == pack_name)
            .cloned();
        if let Some(existing) = existing {
            profile.favorited_packs.remove(&existing);
            false
        } else {
            profile.favorited_packs.insert(pack_name.to_string());
            true
        }
    };
    let packs = lock_profiles()[side_ix(side)].favorited_packs.clone();
    save_favorited_packs(&profile_id, &packs);
    is_now_favorite
}

/// Check if a pack name is favorited for the given player side.
pub fn is_pack_favorite(side: PlayerSide, pack_name: &str) -> bool {
    let profiles = lock_profiles();
    profiles[side_ix(side)]
        .favorited_packs
        .iter()
        .any(|p| *p == pack_name)
}

/// Test/bench helper: mark a pack as favorited for the given side in the
/// in-memory profile only, without persisting to disk.
pub fn seed_session_favorited_pack(side: PlayerSide, pack_name: &str) {
    let mut profiles = lock_profiles();
    profiles[side_ix(side)]
        .favorited_packs
        .insert(pack_name.to_string());
}

pub fn set_active_profile_for_side(side: PlayerSide, profile: ActiveProfile) -> Profile {
    {
        let mut session = lock_session();
        let slot = &mut session.active_profiles[side_ix(side)];
        if *slot == profile {
            return get_for_side(side);
        }
        *slot = profile;
    }
    load_for_side(side);
    get_for_side(side)
}

pub fn set_active_profiles(p1: ActiveProfile, p2: ActiveProfile) -> [Profile; PLAYER_SLOTS] {
    let _ = set_active_profile_for_side(PlayerSide::P1, p1);
    let _ = set_active_profile_for_side(PlayerSide::P2, p2);
    update_default_profiles_from_selection(
        &get_active_profile_for_side(PlayerSide::P1),
        &get_active_profile_for_side(PlayerSide::P2),
    );
    [get_for_side(PlayerSide::P1), get_for_side(PlayerSide::P2)]
}

pub fn load_default_profiles_for_joined_sides() -> [Profile; PLAYER_SLOTS] {
    let (p1, p2) = config::default_profiles();
    let defaults = [p1, p2];
    let joined_mask = lock_session().joined_mask;
    for side in [PlayerSide::P1, PlayerSide::P2] {
        if !player_side_is_joined(joined_mask, side) {
            continue;
        }
        let active = default_profile_from_id(defaults[side_ix(side)].clone());
        lock_session().active_profiles[side_ix(side)] = active;
        load_for_side(side);
    }
    [get_for_side(PlayerSide::P1), get_for_side(PlayerSide::P2)]
}

pub fn scan_local_profiles() -> Vec<LocalProfileSummary> {
    deadsync_profile::scan_local_profile_summaries(&profiles_root())
}

pub fn create_local_profile(display_name: &str) -> Result<String, std::io::Error> {
    let name = display_name.trim();
    if name.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Display name is empty",
        ));
    }

    let id = generate_profile_guid();
    let folder = folder_name_for_display(name, &id, &existing_profile_folder_names());
    let dir = profile_dir_by_folder(&folder);
    fs::create_dir_all(&dir)?;

    let mut default_profile = Profile::default();
    default_profile.noteskin = machine_default_noteskin_value();
    default_profile.pad_light_brightness = machine_default_light_brightness();
    default_profile.store_current_player_options_for_all_styles();
    let initials = initials_from_name(name);
    let today = Local::now().date_naive().to_string();
    default_profile.display_name = name.to_string();
    default_profile.player_initials = initials;
    default_profile.weight_pounds = 0;
    default_profile.birth_year = 0;
    default_profile.ignore_step_count_calories = false;
    default_profile.calories_burned_day = today;
    default_profile.calories_burned_today = 0.0;
    fs::write(
        dir.join("profile.ini"),
        render_profile_ini_content(&id, &default_profile),
    )?;
    fs::write(
        dir.join("groovestats.ini"),
        render_groovestats_ini_content("", false, ""),
    )?;
    fs::write(
        dir.join("arrowcloud.ini"),
        render_arrowcloud_ini_content(""),
    )?;

    // Make the new GUID -> folder mapping visible to later path lookups.
    invalidate_profile_dir_cache();

    let (p1_default, p2_default) = config::default_profiles();
    if p1_default.is_none() {
        config::update_default_profiles(Some(id.clone()), p2_default);
    } else if p2_default.is_none() {
        config::update_default_profiles(p1_default, Some(id.clone()));
    }

    Ok(id)
}

/// Player options seeded from machine defaults for a brand-new local profile,
/// returned as `(singles, doubles)`. Used as the translation base when importing
/// Simply Love settings so unspecified options match a freshly created profile.
pub fn default_local_profile_options() -> (PlayerOptionsData, PlayerOptionsData) {
    let mut default_profile = Profile::default();
    default_profile.noteskin = machine_default_noteskin_value();
    default_profile.pad_light_brightness = machine_default_light_brightness();
    default_profile.store_current_player_options_for_all_styles();
    (
        default_profile.player_options_singles.clone(),
        default_profile.player_options_doubles.clone(),
    )
}

/// Everything needed to materialise a new local profile from an external source
/// (the ITGmania / Simply Love importer).
pub struct ImportProfileData<'a> {
    pub display_name: &'a str,
    pub weight_pounds: u32,
    pub birth_year: u32,
    /// Preferred player initials (e.g. ITGmania `LastUsedHighScoreName`). Falls
    /// back to initials derived from the display name when empty.
    pub initials: &'a str,
    pub groovestats_api_key: &'a str,
    pub groovestats_username: &'a str,
    pub groovestats_is_pad_player: bool,
    pub arrowcloud_api_key: &'a str,
    /// Whether step-count calorie estimation is disabled (ITGmania
    /// `IgnoreStepCountCalories`).
    pub ignore_step_count_calories: bool,
    /// Source avatar image to copy in as `profile.png`, if any.
    pub avatar_src: Option<&'a Path>,
    pub options_singles: &'a PlayerOptionsData,
    pub options_doubles: &'a PlayerOptionsData,
    /// Desired profile GUID (canonical identity). For ITGmania imports this is
    /// derived deterministically from the source profile's `Guid`. When empty or
    /// not a valid GUID, a fresh one is generated.
    pub guid: &'a str,
}

/// Create a new local profile from imported data, writing `profile.ini`,
/// `groovestats.ini`, `arrowcloud.ini`, and copying the avatar. Returns the new
/// profile id. Scores are written separately via
/// [`crate::game::scores::import_local_scores`].
pub fn create_local_profile_from_import(
    data: &ImportProfileData<'_>,
) -> Result<String, std::io::Error> {
    let name = data.display_name.trim();
    if name.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Display name is empty",
        ));
    }

    let id = if is_valid_profile_guid(data.guid.trim()) {
        data.guid.trim().to_string()
    } else {
        generate_profile_guid()
    };
    let folder = folder_name_for_display(name, &id, &existing_profile_folder_names());
    let dir = profile_dir_by_folder(&folder);
    fs::create_dir_all(&dir)?;

    let initials = {
        let sanitized = sanitize_player_initials(data.initials);
        if sanitized.is_empty() {
            initials_from_name(name)
        } else {
            sanitized
        }
    };
    let weight = clamp_weight_pounds(data.weight_pounds.min(i32::MAX as u32) as i32);

    let today = Local::now().date_naive().to_string();
    let profile = Profile {
        display_name: name.to_string(),
        player_initials: initials,
        weight_pounds: weight,
        birth_year: data.birth_year.min(i32::MAX as u32) as i32,
        ignore_step_count_calories: data.ignore_step_count_calories,
        calories_burned_day: today,
        calories_burned_today: 0.0,
        player_options_singles: data.options_singles.clone(),
        player_options_doubles: data.options_doubles.clone(),
        ..Profile::default()
    };
    fs::write(
        dir.join("profile.ini"),
        render_profile_ini_content(&id, &profile),
    )?;
    fs::write(
        dir.join("groovestats.ini"),
        render_groovestats_ini_content(
            data.groovestats_api_key,
            data.groovestats_is_pad_player,
            data.groovestats_username,
        ),
    )?;
    fs::write(
        dir.join("arrowcloud.ini"),
        render_arrowcloud_ini_content(data.arrowcloud_api_key),
    )?;

    if let Some(src) = data.avatar_src {
        if let Err(e) = fs::copy(src, dir.join("profile.png")) {
            warn!("Failed to copy imported avatar {src:?}: {e}");
        }
    }

    // Make the new GUID -> folder mapping visible to later path lookups (scores,
    // favorites and profile-stats writes all resolve the profile dir by GUID).
    invalidate_profile_dir_cache();

    Ok(id)
}

pub fn rename_local_profile(id: &str, display_name: &str) -> Result<(), std::io::Error> {
    if !is_local_profile_id(id) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid local profile id",
        ));
    }

    let name = display_name.trim();
    if name.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Display name is empty",
        ));
    }

    let ini_path = profile_ini_path(id);
    if !ini_path.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Profile does not exist",
        ));
    }
    deadsync_profile::rewrite_profile_display_name_file(&ini_path, name)?;

    // Keep the folder readable; safe because identity is the GUID, not the name.
    let current_dir = local_profile_dir(id);
    if let Some(current_folder) = current_dir.file_name().and_then(|s| s.to_str()) {
        let others: Vec<String> = existing_profile_folder_names()
            .into_iter()
            .filter(|f| !f.eq_ignore_ascii_case(current_folder))
            .collect();
        let desired = folder_name_for_display(name, id, &others);
        if !desired.eq_ignore_ascii_case(current_folder) {
            let target = profile_dir_by_folder(&desired);
            match fs::rename(&current_dir, &target) {
                Ok(()) => info!("Renamed profile folder '{current_folder}' -> '{desired}'."),
                Err(e) => {
                    warn!("Failed to rename profile folder '{current_folder}' -> '{desired}': {e}");
                }
            }
        }
    }
    invalidate_profile_dir_cache();

    let p1_active = active_local_profile_id_for_side(PlayerSide::P1)
        .as_deref()
        .is_some_and(|active_id| active_id == id);
    let p2_active = active_local_profile_id_for_side(PlayerSide::P2)
        .as_deref()
        .is_some_and(|active_id| active_id == id);
    if p1_active || p2_active {
        let mut profiles = lock_profiles();
        if p1_active {
            profiles[side_ix(PlayerSide::P1)].display_name = name.to_string();
        }
        if p2_active {
            profiles[side_ix(PlayerSide::P2)].display_name = name.to_string();
        }
    }

    Ok(())
}

pub fn delete_local_profile(id: &str) -> Result<(), std::io::Error> {
    if !is_local_profile_id(id) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid local profile id",
        ));
    }

    let dir = local_profile_dir(id);
    if !dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Profile does not exist",
        ));
    }

    fs::remove_dir_all(&dir)?;
    invalidate_profile_dir_cache();

    let (p1_default, p2_default) = config::default_profiles();
    let next_p1 = p1_default.filter(|profile_id| profile_id != id);
    let next_p2 = p2_default.filter(|profile_id| profile_id != id);
    config::update_default_profiles(next_p1, next_p2);

    for side in [PlayerSide::P1, PlayerSide::P2] {
        let is_active = active_local_profile_id_for_side(side)
            .as_deref()
            .is_some_and(|active_id| active_id == id);
        if is_active {
            let _ = set_active_profile_for_side(side, ActiveProfile::Guest);
        }
    }

    Ok(())
}

pub fn get_session_music_rate() -> f32 {
    let s = lock_session();
    let r = s.music_rate;
    if r.is_finite() && r > 0.0 { r } else { 1.0 }
}

pub fn set_session_music_rate(rate: f32) {
    let mut s = lock_session();
    s.music_rate = if rate.is_finite() && rate > 0.0 {
        rate.clamp(0.5, 3.0)
    } else {
        1.0
    };
}

pub fn get_session_timing_tick_mode() -> TimingTickMode {
    lock_session().timing_tick_mode
}

pub fn set_session_timing_tick_mode(mode: TimingTickMode) {
    lock_session().timing_tick_mode = mode;
}

pub fn get_session_play_style() -> PlayStyle {
    lock_session().play_style
}

pub fn set_session_play_style(style: PlayStyle) {
    let prev_style = {
        let mut session = lock_session();
        let prev_style = session.play_style;
        if prev_style == style {
            return;
        }
        session.play_style = style;
        prev_style
    };

    let mut profiles = lock_profiles();
    for profile in profiles.iter_mut() {
        profile.store_current_player_options(prev_style);
        profile.apply_player_options_for_style(style);
    }
}

pub fn get_session_play_mode() -> PlayMode {
    lock_session().play_mode
}

pub fn set_session_play_mode(mode: PlayMode) {
    lock_session().play_mode = mode;
}

pub fn get_session_player_side() -> PlayerSide {
    lock_session().player_side
}

pub fn set_session_player_side(side: PlayerSide) {
    lock_session().player_side = side;
}

pub fn is_session_side_joined(side: PlayerSide) -> bool {
    let mask = lock_session().joined_mask;
    player_side_is_joined(mask, side)
}

pub fn is_session_side_guest(side: PlayerSide) -> bool {
    session_side_is_guest(side)
}

pub fn set_session_joined(p1: bool, p2: bool) {
    lock_session().joined_mask = joined_player_mask(p1, p2);
}

pub fn set_fast_profile_switch_from_select_music(enabled: bool) {
    lock_session().fast_profile_switch_from_select_music = enabled;
}

pub fn fast_profile_switch_from_select_music() -> bool {
    lock_session().fast_profile_switch_from_select_music
}

pub fn take_fast_profile_switch_from_select_music() -> bool {
    let mut session = lock_session();
    let was_set = session.fast_profile_switch_from_select_music;
    session.fast_profile_switch_from_select_music = false;
    was_set
}

#[cfg(test)]
mod tests {
    use super::{
        SimpleIni, heal_default_profile_id, load_player_options, parse_groovestats_is_pad_player,
    };
    use deadsync_profile::{
        AccelEffectsMask, AppearanceEffectsMask, DEFAULT_BIRTH_YEAR, DEFAULT_WEIGHT_POUNDS,
        ErrorBarMask, ErrorBarStyle, HoldsMask, InsertMask, LastPlayed, LastPlayedCourse,
        LiveTimingStatsMask, MiniIndicatorColor, MiniIndicatorPosition, MiniIndicatorSize,
        MiniIndicatorSubtractiveDisplay, NoCmodAlternative, NoteSkin, PlayStyle, PlayerOptionsData,
        Profile, RemoveMask, TapExplosionMask, TimingWindowsOption, VisualEffectsMask,
        append_player_options_section, error_bar_mask_from_style, error_bar_style_from_mask,
        error_bar_text_from_mask, normalize_tap_explosion_mask, player_options_section,
    };
    use std::collections::HashMap;
    use std::str::FromStr;

    #[test]
    fn heal_default_profile_id_translates_folder_names_to_guids() {
        let guid = "17c7b8a2-3b73-4e8a-9d7d-cfa7e783c00b";
        let mut folder_to_guid: HashMap<&str, &str> = HashMap::new();
        folder_to_guid.insert("00000000", guid);

        // A legacy folder-name id is rewritten to that folder's GUID.
        assert_eq!(
            heal_default_profile_id(Some("00000000".to_string()), &folder_to_guid).as_deref(),
            Some(guid)
        );
        // An already-valid GUID passes through untouched.
        assert_eq!(
            heal_default_profile_id(Some(guid.to_string()), &folder_to_guid).as_deref(),
            Some(guid)
        );
        // A stale id with no matching folder is preserved (not dropped to Guest).
        assert_eq!(
            heal_default_profile_id(Some("99999999".to_string()), &folder_to_guid).as_deref(),
            Some("99999999")
        );
        // No stored default stays absent.
        assert_eq!(heal_default_profile_id(None, &folder_to_guid), None);
    }

    #[test]
    fn mini_indicator_style_settings_round_trip() {
        assert_eq!(
            MiniIndicatorSize::from_str(&MiniIndicatorSize::Default.to_string()).unwrap(),
            MiniIndicatorSize::Default
        );
        assert_eq!(
            MiniIndicatorSize::from_str(&MiniIndicatorSize::Large.to_string()).unwrap(),
            MiniIndicatorSize::Large
        );
        assert_eq!(
            MiniIndicatorColor::from_str(&MiniIndicatorColor::Default.to_string()).unwrap(),
            MiniIndicatorColor::Default
        );
        assert_eq!(
            MiniIndicatorColor::from_str(&MiniIndicatorColor::Detailed.to_string()).unwrap(),
            MiniIndicatorColor::Detailed
        );
        assert_eq!(
            MiniIndicatorColor::from_str(&MiniIndicatorColor::Combo.to_string()).unwrap(),
            MiniIndicatorColor::Combo
        );
        assert_eq!(
            MiniIndicatorSubtractiveDisplay::from_str(
                &MiniIndicatorSubtractiveDisplay::Points.to_string(),
            )
            .unwrap(),
            MiniIndicatorSubtractiveDisplay::Points
        );
        assert_eq!(
            MiniIndicatorPosition::from_str(&MiniIndicatorPosition::UnderUpArrow.to_string())
                .unwrap(),
            MiniIndicatorPosition::UnderUpArrow
        );
    }

    #[test]
    fn no_cmod_alternative_round_trips_through_player_options_ini() {
        let section = player_options_section(PlayStyle::Single);
        for alt in [
            NoCmodAlternative::None,
            NoCmodAlternative::XMod,
            NoCmodAlternative::MMod,
        ] {
            let options = PlayerOptionsData {
                no_cmod_alternative: alt,
                ..PlayerOptionsData::default()
            };
            let mut content = String::new();
            append_player_options_section(&mut content, section, &options);

            let mut ini = SimpleIni::new();
            ini.load_str(&content);
            let loaded = load_player_options(&ini, section, &PlayerOptionsData::default())
                .expect("section has keys, so it should load");

            assert_eq!(loaded.no_cmod_alternative, alt);
        }
    }

    #[test]
    fn mini_indicator_style_defaults_preserve_legacy_look() {
        let profile = Profile::default();
        assert_eq!(profile.mini_indicator_size, MiniIndicatorSize::Default);
        assert_eq!(profile.mini_indicator_color, MiniIndicatorColor::Default);
        assert_eq!(
            profile.mini_indicator_subtractive_display,
            MiniIndicatorSubtractiveDisplay::Percent
        );
        assert_eq!(
            profile.mini_indicator_position,
            MiniIndicatorPosition::Default
        );
    }

    #[test]
    fn groovestats_is_pad_player_requires_explicit_one() {
        assert!(parse_groovestats_is_pad_player(Some("1"), false));
        assert!(!parse_groovestats_is_pad_player(Some("0"), false));
        assert!(!parse_groovestats_is_pad_player(Some("2"), false));
        assert!(!parse_groovestats_is_pad_player(Some("255"), false));
    }

    #[test]
    fn groovestats_is_pad_player_uses_default_on_invalid_value() {
        assert!(parse_groovestats_is_pad_player(None, true));
        assert!(!parse_groovestats_is_pad_player(None, false));
        assert!(parse_groovestats_is_pad_player(Some("abc"), true));
        assert!(!parse_groovestats_is_pad_player(Some("abc"), false));
    }

    #[test]
    fn calculated_weight_pounds_uses_itg_default_when_unset() {
        assert_eq!(
            Profile::default().calculated_weight_pounds(),
            DEFAULT_WEIGHT_POUNDS
        );
        assert_eq!(
            Profile {
                weight_pounds: 165,
                ..Profile::default()
            }
            .calculated_weight_pounds(),
            165
        );
    }

    #[test]
    fn age_years_for_uses_birth_year_or_default() {
        assert_eq!(
            Profile::default().age_years_for(2026),
            2026 - DEFAULT_BIRTH_YEAR
        );
        assert_eq!(
            Profile {
                birth_year: 2000,
                ..Profile::default()
            }
            .age_years_for(2026),
            26
        );
    }

    #[test]
    fn last_played_uses_singles_for_single_and_versus() {
        let singles = LastPlayed {
            song_music_path: Some("single.ogg".to_string()),
            chart_hash: Some("singlehash".to_string()),
            difficulty_index: 3,
        };
        let doubles = LastPlayed {
            song_music_path: Some("double.ogg".to_string()),
            chart_hash: Some("doublehash".to_string()),
            difficulty_index: 7,
        };
        let profile = Profile {
            last_played_singles: singles.clone(),
            last_played_doubles: doubles.clone(),
            ..Profile::default()
        };

        assert_eq!(profile.last_played(PlayStyle::Single), &singles);
        assert_eq!(profile.last_played(PlayStyle::Versus), &singles);
        assert_eq!(profile.last_played(PlayStyle::Double), &doubles);
    }

    #[test]
    fn last_played_course_uses_singles_for_single_and_versus() {
        let singles = LastPlayedCourse {
            course_path: Some("Courses/Single.crs".to_string()),
            difficulty_name: Some("Hard".to_string()),
        };
        let doubles = LastPlayedCourse {
            course_path: Some("Courses/Double.crs".to_string()),
            difficulty_name: Some("Challenge".to_string()),
        };
        let profile = Profile {
            last_played_course_singles: singles.clone(),
            last_played_course_doubles: doubles.clone(),
            ..Profile::default()
        };

        assert_eq!(profile.last_played_course(PlayStyle::Single), &singles);
        assert_eq!(profile.last_played_course(PlayStyle::Versus), &singles);
        assert_eq!(profile.last_played_course(PlayStyle::Double), &doubles);
    }

    #[test]
    fn player_options_use_singles_for_single_and_versus() {
        let mut profile = Profile::default();
        profile.mini_percent = 12;
        profile.global_offset_shift_ms = 9;
        profile.store_current_player_options(PlayStyle::Single);
        profile.mini_percent = 48;
        profile.global_offset_shift_ms = -11;
        profile.store_current_player_options(PlayStyle::Double);

        assert_eq!(profile.player_options(PlayStyle::Single).mini_percent, 12);
        assert_eq!(profile.player_options(PlayStyle::Versus).mini_percent, 12);
        assert_eq!(profile.player_options(PlayStyle::Double).mini_percent, 48);
        assert_eq!(
            profile
                .player_options(PlayStyle::Single)
                .global_offset_shift_ms,
            9
        );
        assert_eq!(
            profile
                .player_options(PlayStyle::Versus)
                .global_offset_shift_ms,
            9
        );
        assert_eq!(
            profile
                .player_options(PlayStyle::Double)
                .global_offset_shift_ms,
            -11
        );
    }

    #[test]
    fn apply_player_options_for_style_restores_separate_snapshots() {
        let mut profile = Profile::default();
        profile.mini_percent = 18;
        profile.show_ex_score = true;
        profile.score_position = deadsync_profile::ScorePosition::StepStatistics;
        profile.score_display_mode = deadsync_profile::ScoreDisplayMode::Predictive;
        profile.global_offset_shift_ms = 7;
        profile.timing_windows = TimingWindowsOption::WayOffs;
        profile.receptor_noteskin = Some(NoteSkin::new("default"));
        profile.tap_explosion_noteskin = Some(NoteSkin::new("metal"));
        profile.tap_explosion_active_mask =
            TapExplosionMask::all().difference(TapExplosionMask::HELD);
        profile.store_current_player_options(PlayStyle::Single);

        profile.mini_percent = 62;
        profile.show_ex_score = false;
        profile.score_position = deadsync_profile::ScorePosition::Normal;
        profile.score_display_mode = deadsync_profile::ScoreDisplayMode::Normal;
        profile.global_offset_shift_ms = -13;
        profile.timing_windows = TimingWindowsOption::FantasticsAndExcellents;
        profile.receptor_noteskin = Some(NoteSkin::new("cyber"));
        profile.tap_explosion_noteskin = None;
        profile.tap_explosion_active_mask = TapExplosionMask::HELD;
        profile.store_current_player_options(PlayStyle::Double);

        profile.apply_player_options_for_style(PlayStyle::Single);
        assert_eq!(profile.mini_percent, 18);
        assert!(profile.show_ex_score);
        assert_eq!(
            profile.score_position,
            deadsync_profile::ScorePosition::StepStatistics
        );
        assert_eq!(
            profile.score_display_mode,
            deadsync_profile::ScoreDisplayMode::Predictive
        );
        assert_eq!(profile.global_offset_shift_ms, 7);
        assert_eq!(profile.timing_windows, TimingWindowsOption::WayOffs);
        assert_eq!(profile.receptor_noteskin, Some(NoteSkin::new("default")));
        assert_eq!(profile.tap_explosion_noteskin, Some(NoteSkin::new("metal")));
        assert_eq!(
            profile.tap_explosion_active_mask,
            TapExplosionMask::all().difference(TapExplosionMask::HELD)
        );

        profile.apply_player_options_for_style(PlayStyle::Double);
        assert_eq!(profile.mini_percent, 62);
        assert!(!profile.show_ex_score);
        assert_eq!(
            profile.score_position,
            deadsync_profile::ScorePosition::Normal
        );
        assert_eq!(
            profile.score_display_mode,
            deadsync_profile::ScoreDisplayMode::Normal
        );
        assert_eq!(profile.global_offset_shift_ms, -13);
        assert_eq!(
            profile.timing_windows,
            TimingWindowsOption::FantasticsAndExcellents
        );
        assert_eq!(profile.receptor_noteskin, Some(NoteSkin::new("cyber")));
        assert_eq!(profile.tap_explosion_noteskin, None);
        assert_eq!(profile.tap_explosion_active_mask, TapExplosionMask::HELD);
    }

    #[test]
    fn tap_explosion_none_choice_disables_resolution() {
        let profile = Profile {
            tap_explosion_noteskin: Some(NoteSkin::none_choice()),
            ..Profile::default()
        };

        assert!(profile.tap_explosion_noteskin_hidden());
        assert_eq!(profile.resolved_tap_explosion_noteskin(), None);
    }

    #[test]
    fn tap_explosion_mask_migrates_new_bits_from_old_profiles() {
        let old_all = TapExplosionMask::FANTASTIC
            | TapExplosionMask::EXCELLENT
            | TapExplosionMask::GREAT
            | TapExplosionMask::DECENT
            | TapExplosionMask::WAY_OFF
            | TapExplosionMask::HELD;

        assert_eq!(
            normalize_tap_explosion_mask(old_all.bits(), 1),
            TapExplosionMask::all()
        );
        assert_eq!(normalize_tap_explosion_mask(old_all.bits(), 2), old_all);
    }

    #[test]
    fn tap_explosion_miss_window_uses_miss_mask() {
        let mut profile = Profile::default();
        assert!(profile.tap_explosion_window_enabled("Miss"));

        profile
            .tap_explosion_active_mask
            .remove(TapExplosionMask::MISS);
        assert!(!profile.tap_explosion_window_enabled("Miss"));
        assert!(profile.tap_explosion_window_enabled("Held"));
    }

    #[test]
    fn persisted_row_mask_bit_layouts_are_stable() {
        // InsertMask: persisted bits 0..=6 (Mines is runtime-only and
        // intentionally not represented here).
        assert_eq!(InsertMask::WIDE.bits(), 1 << 0);
        assert_eq!(InsertMask::BIG.bits(), 1 << 1);
        assert_eq!(InsertMask::QUICK.bits(), 1 << 2);
        assert_eq!(InsertMask::BMRIZE.bits(), 1 << 3);
        assert_eq!(InsertMask::SKIPPY.bits(), 1 << 4);
        assert_eq!(InsertMask::ECHO.bits(), 1 << 5);
        assert_eq!(InsertMask::STOMP.bits(), 1 << 6);
        assert_eq!(InsertMask::all().bits(), 0b0111_1111);

        // RemoveMask: bits 0..=7
        assert_eq!(RemoveMask::LITTLE.bits(), 1 << 0);
        assert_eq!(RemoveMask::NO_MINES.bits(), 1 << 1);
        assert_eq!(RemoveMask::NO_HOLDS.bits(), 1 << 2);
        assert_eq!(RemoveMask::NO_JUMPS.bits(), 1 << 3);
        assert_eq!(RemoveMask::NO_HANDS.bits(), 1 << 4);
        assert_eq!(RemoveMask::NO_QUADS.bits(), 1 << 5);
        assert_eq!(RemoveMask::NO_LIFTS.bits(), 1 << 6);
        assert_eq!(RemoveMask::NO_FAKES.bits(), 1 << 7);
        assert_eq!(RemoveMask::all().bits(), 0xFF);

        assert_eq!(HoldsMask::PLANTED.bits(), 1 << 0);
        assert_eq!(HoldsMask::FLOORED.bits(), 1 << 1);
        assert_eq!(HoldsMask::TWISTER.bits(), 1 << 2);
        assert_eq!(HoldsMask::NO_ROLLS.bits(), 1 << 3);
        assert_eq!(HoldsMask::HOLDS_TO_ROLLS.bits(), 1 << 4);
        assert_eq!(HoldsMask::all().bits(), 0b0001_1111);

        assert_eq!(AccelEffectsMask::BOOST.bits(), 1 << 0);
        assert_eq!(AccelEffectsMask::BRAKE.bits(), 1 << 1);
        assert_eq!(AccelEffectsMask::WAVE.bits(), 1 << 2);
        assert_eq!(AccelEffectsMask::EXPAND.bits(), 1 << 3);
        assert_eq!(AccelEffectsMask::BOOMERANG.bits(), 1 << 4);
        assert_eq!(AccelEffectsMask::all().bits(), 0b0001_1111);

        assert_eq!(VisualEffectsMask::DRUNK.bits(), 1 << 0);
        assert_eq!(VisualEffectsMask::DIZZY.bits(), 1 << 1);
        assert_eq!(VisualEffectsMask::CONFUSION.bits(), 1 << 2);
        assert_eq!(VisualEffectsMask::BIG.bits(), 1 << 3);
        assert_eq!(VisualEffectsMask::FLIP.bits(), 1 << 4);
        assert_eq!(VisualEffectsMask::INVERT.bits(), 1 << 5);
        assert_eq!(VisualEffectsMask::TORNADO.bits(), 1 << 6);
        assert_eq!(VisualEffectsMask::TIPSY.bits(), 1 << 7);
        assert_eq!(VisualEffectsMask::BUMPY.bits(), 1 << 8);
        assert_eq!(VisualEffectsMask::BEAT.bits(), 1 << 9);
        assert_eq!(VisualEffectsMask::all().bits(), 0b11_1111_1111);

        assert_eq!(AppearanceEffectsMask::HIDDEN.bits(), 1 << 0);
        assert_eq!(AppearanceEffectsMask::SUDDEN.bits(), 1 << 1);
        assert_eq!(AppearanceEffectsMask::STEALTH.bits(), 1 << 2);
        assert_eq!(AppearanceEffectsMask::BLINK.bits(), 1 << 3);
        assert_eq!(AppearanceEffectsMask::RANDOM_VANISH.bits(), 1 << 4);
        assert_eq!(AppearanceEffectsMask::all().bits(), 0b0001_1111);

        assert_eq!(ErrorBarMask::COLORFUL.bits(), 1 << 0);
        assert_eq!(ErrorBarMask::MONOCHROME.bits(), 1 << 1);
        assert_eq!(ErrorBarMask::TEXT.bits(), 1 << 2);
        assert_eq!(ErrorBarMask::HIGHLIGHT.bits(), 1 << 3);
        assert_eq!(ErrorBarMask::AVERAGE.bits(), 1 << 4);
        assert_eq!(ErrorBarMask::all().bits(), 0b0001_1111);

        assert_eq!(LiveTimingStatsMask::MEAN.bits(), 1 << 0);
        assert_eq!(LiveTimingStatsMask::MEAN_ABS.bits(), 1 << 1);
        assert_eq!(LiveTimingStatsMask::MAX.bits(), 1 << 2);
        assert_eq!(LiveTimingStatsMask::all().bits(), 0b0000_0111);

        assert_eq!(TapExplosionMask::FANTASTIC.bits(), 1 << 0);
        assert_eq!(TapExplosionMask::EXCELLENT.bits(), 1 << 1);
        assert_eq!(TapExplosionMask::GREAT.bits(), 1 << 2);
        assert_eq!(TapExplosionMask::DECENT.bits(), 1 << 3);
        assert_eq!(TapExplosionMask::WAY_OFF.bits(), 1 << 4);
        assert_eq!(TapExplosionMask::HELD.bits(), 1 << 5);
        assert_eq!(TapExplosionMask::MISS.bits(), 1 << 6);
        assert_eq!(TapExplosionMask::HOLDING.bits(), 1 << 7);
        assert_eq!(TapExplosionMask::all().bits(), 0xFF);
    }

    #[test]
    fn from_bits_truncate_drops_unrepresented_bits() {
        // InsertMask only persists 7 bits; bit 7 (Mines) belongs to runtime.
        assert_eq!(InsertMask::from_bits_truncate(0xFF), InsertMask::all());
        assert_eq!(InsertMask::from_bits_truncate(0xFF).bits(), 0b0111_1111);

        // VisualEffectsMask is 10 bits in a u16.
        assert_eq!(
            VisualEffectsMask::from_bits_truncate(u16::MAX),
            VisualEffectsMask::all()
        );
        assert_eq!(
            VisualEffectsMask::from_bits_truncate(u16::MAX).bits(),
            0b11_1111_1111
        );
    }

    #[test]
    fn error_bar_helpers_roundtrip_through_mask() {
        // Style + text combine into mask bits.
        let mask = error_bar_mask_from_style(ErrorBarStyle::Colorful, true);
        assert!(mask.contains(ErrorBarMask::COLORFUL));
        assert!(mask.contains(ErrorBarMask::TEXT));
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::Colorful);
        assert!(error_bar_text_from_mask(mask));

        // Style precedence: Colorful > Monochrome > Highlight > Average > None.
        let mask = ErrorBarMask::COLORFUL | ErrorBarMask::MONOCHROME;
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::Colorful);

        // Text-only mask round-trips to (Style::None, text=true) — the legacy
        // canonicalization quirk preserved by the typed helpers.
        let mask = error_bar_mask_from_style(ErrorBarStyle::Text, false);
        assert!(mask.contains(ErrorBarMask::TEXT));
        assert!(!mask.contains(ErrorBarMask::COLORFUL));
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::None);
        assert!(error_bar_text_from_mask(mask));

        // Empty mask means no error bar at all.
        let mask = error_bar_mask_from_style(ErrorBarStyle::None, false);
        assert!(mask.is_empty());
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::None);
        assert!(!error_bar_text_from_mask(mask));
    }
}
