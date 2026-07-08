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
use log::{info, warn};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

mod update;

pub use deadsync_profile::ImportProfileData;
use deadsync_profile::{
    ActiveProfile, ActiveProfileLoadSelection, GameplayHudSnapshot, LocalProfileSummary, NoteSkin,
    PLAYER_SLOTS, PlayMode, PlayStyle, PlayerOptionsData, PlayerSide, Profile, ProfileStats,
    ProfileStatsDecodeError, ProfileStatsLoadError, ProfileStatsWriteError, TimingTickMode,
    active_profile_is_guest, find_profile_avatar_path, is_local_profile_id, is_valid_profile_guid,
    player_side_index as side_ix, rename_local_profile_dir, resolve_active_profile_for_load,
    runtime_lock_profiles as lock_profiles, runtime_lock_session as lock_session,
    unknown_pack_names,
};
#[cfg(test)]
use deadsync_profile::{
    LastPlayed, LastPlayedCourse, load_last_played_course_section, load_last_played_section,
    load_player_options_section, parse_groovestats_is_pad_player,
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
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
fn load_last_played_course(profile_conf: &SimpleIni, section: &str) -> Option<LastPlayedCourse> {
    let has_any = profile_conf
        .get_section(section)
        .is_some_and(|s| !s.is_empty());
    load_last_played_course_section(has_any, |key| profile_conf.get(section, key))
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
    info!(
        "Profile files not found, creating defaults in '{}'.",
        dir.display()
    );

    let mut default_profile = Profile::default();
    default_profile.noteskin = machine_default_noteskin_value();
    default_profile.pad_light_brightness = machine_default_light_brightness();
    default_profile.store_current_player_options_for_all_styles();
    default_profile.calories_burned_day = Local::now().date_naive().to_string();
    deadsync_profile::ensure_local_profile_files_dir(&dir, id, &default_profile)?;

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
    let dir = local_profile_dir(&profile_id);
    if let Err(e) = deadsync_profile::write_profile_ini_dir(&dir, &profile_id, &profile) {
        let path = deadsync_profile::profile_ini_path(&dir);
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

#[inline(always)]
fn log_profile_stats_load_error(path: &Path, error: ProfileStatsLoadError) {
    match error {
        ProfileStatsLoadError::Read(e) => {
            warn!("Failed to read {}: {}", path.display(), e);
        }
        ProfileStatsLoadError::Decode(ProfileStatsDecodeError::UnsupportedVersion(version)) => {
            warn!(
                "Unsupported profile stats version {} in '{}'.",
                version,
                path.display()
            );
        }
        ProfileStatsLoadError::Decode(ProfileStatsDecodeError::InvalidPayload) => {
            warn!("Failed to decode profile stats '{}'.", path.display());
        }
    }
}

fn load_profile_stats(profile_id: &str) -> Option<ProfileStats> {
    let path = profile_stats_path(profile_id);
    match deadsync_profile::load_profile_stats_file(&path) {
        Ok(stats) => stats,
        Err(e) => {
            log_profile_stats_load_error(&path, e);
            None
        }
    }
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
    if let Err(e) =
        deadsync_profile::write_profile_stats_dir(&local_profile_dir(profile_id), payload)
    {
        log_profile_stats_write_error(profile_id, e);
    }
}

fn log_profile_stats_write_error(profile_id: &str, error: ProfileStatsWriteError) {
    match error {
        ProfileStatsWriteError::Encode => {
            warn!("Failed to encode profile stats for '{}'.", profile_id);
        }
        ProfileStatsWriteError::CreateDir { path, error } => {
            warn!(
                "Failed to create profile stats directory '{}': {}",
                path.display(),
                error
            );
        }
        ProfileStatsWriteError::WriteTmp { path, error } => {
            warn!("Failed to write {}: {}", path.display(), error);
        }
        ProfileStatsWriteError::Rename { path, error, .. } => {
            warn!("Failed to save {}: {}", path.display(), error);
        }
    }
}

/// Writes [`ProfileStats`] for a freshly-imported profile that isn't loaded into
/// a player side (used by the ITGmania importer). `known_pack_names` is left
/// empty so the normal first-load reconciliation still marks the current packs
/// as known. Does nothing when there's no stat worth persisting.
pub fn write_imported_profile_stats(profile_id: &str, current_combo: u32) {
    if let Err(e) = deadsync_profile::write_imported_profile_stats_dir(
        &local_profile_dir(profile_id),
        current_combo,
    ) {
        log_profile_stats_write_error(profile_id, e);
    }
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

    let dir = local_profile_dir(&profile_id);
    if let Err(e) = deadsync_profile::write_groovestats_credentials_dir(
        &dir,
        &profile.groovestats_api_key,
        profile.groovestats_is_pad_player,
        &profile.groovestats_username,
    ) {
        let path = deadsync_profile::groovestats_ini_path(&dir);
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

    let dir = local_profile_dir(&profile_id);
    if let Err(e) =
        deadsync_profile::write_arrowcloud_api_key_dir(&dir, &profile.arrowcloud_api_key)
    {
        let path = deadsync_profile::arrowcloud_ini_path(&dir);
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

/// Update the active profile's ArrowCloud API key (in memory + on disk).
/// No-op when the side has no local profile loaded (Guest).
pub fn set_arrowcloud_api_key_for_side(side: PlayerSide, api_key: &str) {
    {
        let mut profiles = lock_profiles();
        deadsync_profile::set_arrowcloud_api_key_for_side(&mut profiles, side, api_key);
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
    {
        let session = lock_session();
        let mut profiles = lock_profiles();
        deadsync_profile::set_arrowcloud_api_key_for_loaded_profile(
            &session.active_profiles,
            &mut profiles,
            profile_id,
            api_key,
        );
    }

    // Persist directly to that profile's ArrowCloud.ini, even if the
    // profile isn't loaded on any side right now.
    let dir = local_profile_dir(profile_id);
    if let Err(e) = deadsync_profile::write_arrowcloud_api_key_dir(&dir, api_key) {
        let path = deadsync_profile::arrowcloud_ini_path(&dir);
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

/// Returns the saved ArrowCloud API key (from disk) for a profile
/// identified by id, regardless of whether it's currently loaded on a
/// session side.  Empty string if the profile has no key yet or the
/// file is missing / malformed.
pub fn get_arrowcloud_api_key_for_id(profile_id: &str) -> String {
    deadsync_profile::read_arrowcloud_api_key_dir(&local_profile_dir(profile_id))
}

/// Update the active profile's GrooveStats credentials (API key,
/// username, and `IsPadPlayer=true` — Simply Love parity, see
/// `BGAnimations/ScreenGrooveStatsLogin underlay/default.lua:46`) for
/// the given session side, persisting to its `GrooveStats.ini` on disk.
/// No-op when the side has no local profile loaded (Guest).
pub fn set_groovestats_credentials_for_side(side: PlayerSide, api_key: &str, username: &str) {
    {
        let mut profiles = lock_profiles();
        deadsync_profile::set_groovestats_credentials_for_side(
            &mut profiles,
            side,
            api_key,
            username,
        );
    }
    save_groovestats_ini_for_side(side);
}

/// Write new GrooveStats credentials for a profile identified by ID
/// (independent of session sides).  Used by the Manage Local Profiles
/// "Link GrooveStats" flow.  Also refreshes the in-memory copy on any
/// session side currently bound to that profile id.
pub fn set_groovestats_credentials_for_id(profile_id: &str, api_key: &str, username: &str) {
    {
        let session = lock_session();
        let mut profiles = lock_profiles();
        deadsync_profile::set_groovestats_credentials_for_loaded_profile(
            &session.active_profiles,
            &mut profiles,
            profile_id,
            api_key,
            username,
        );
    }

    // Persist directly to that profile's GrooveStats.ini, even if the
    // profile isn't loaded on any side right now.
    let dir = local_profile_dir(profile_id);
    if let Err(e) =
        deadsync_profile::write_groovestats_credentials_dir(&dir, api_key, true, username)
    {
        let path = deadsync_profile::groovestats_ini_path(&dir);
        warn!("Failed to save {}: {}", path.display(), e);
    }
}

/// Returns the saved GrooveStats API key (from disk) for a profile
/// identified by id, regardless of whether it's currently loaded on a
/// session side.  `None` if the profile has no key yet or the file is
/// missing / malformed; `Some` always wraps a non-empty trimmed key.
pub fn get_groovestats_api_key_for_id(profile_id: &str) -> Option<String> {
    deadsync_profile::read_groovestats_api_key_dir(&local_profile_dir(profile_id))
}

fn load_for_side(side: PlayerSide) {
    let selection = {
        let session = lock_session();
        resolve_active_profile_for_load(
            &session.active_profiles[side_ix(side)],
            |id| local_profile_dir(id).is_dir(),
            || scan_local_profiles().into_iter().next().map(|p| p.id),
        )
    };

    match &selection {
        ActiveProfileLoadSelection::MissingFallbackLocal {
            missing_id,
            fallback_id,
        } => {
            info!("Profile folder '{missing_id}' not found; falling back to '{fallback_id}'.");
            let mut session = lock_session();
            session.active_profiles[side_ix(side)] = selection.session_profile();
        }
        ActiveProfileLoadSelection::MissingFallbackGuest { missing_id } => {
            info!(
                "Profile folder '{missing_id}' not found and no other profiles exist; using Guest."
            );
            let mut session = lock_session();
            session.active_profiles[side_ix(side)] = selection.session_profile();
        }
        ActiveProfileLoadSelection::Guest | ActiveProfileLoadSelection::Local { .. } => {}
    }

    let Some(profile_id) = selection.local_id().map(str::to_owned) else {
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

    let mut profile_conf = SimpleIni::new();
    let profile_ini_loaded = profile_conf.load(&profile_ini).is_ok();
    if !profile_ini_loaded {
        warn!(
            "Failed to load '{}', using default profile settings.",
            profile_ini.display()
        );
    }

    let mut gs_conf = SimpleIni::new();
    let groovestats_ini_loaded = gs_conf.load(&groovestats_ini).is_ok();
    if !groovestats_ini_loaded {
        warn!(
            "Failed to load '{}', using default GrooveStats info.",
            groovestats_ini.display()
        );
    }

    let mut ac_conf = SimpleIni::new();
    let arrowcloud_ini_loaded = ac_conf.load(&arrowcloud_ini).is_ok();
    if !arrowcloud_ini_loaded {
        warn!(
            "Failed to load '{}', using default ArrowCloud info.",
            arrowcloud_ini.display()
        );
    }

    {
        let mut profiles = lock_profiles();
        let profile = &mut profiles[side_ix(side)];
        let mut default_profile = Profile::default();
        default_profile.noteskin = machine_default_noteskin_value();
        default_profile.pad_light_brightness = machine_default_light_brightness();
        default_profile.store_current_player_options_for_all_styles();
        let stats = load_profile_stats(&profile_id).unwrap_or_else(|| ProfileStats {
            current_combo: default_profile.current_combo,
            known_pack_names: HashSet::new(),
        });
        let today = Local::now().date_naive().to_string();
        deadsync_profile::apply_loaded_profile_data(
            profile,
            &default_profile,
            get_session_play_style(),
            &today,
            profile_ini_loaded,
            |section| {
                profile_conf
                    .get_section(section)
                    .is_some_and(|s| !s.is_empty())
            },
            |section, key| profile_conf.get(section, key),
            stats,
            load_favorites(&profile_id),
            load_favorited_packs(&profile_id),
            groovestats_ini_loaded,
            |section, key| gs_conf.get(section, key),
            arrowcloud_ini_loaded,
            |section, key| ac_conf.get(section, key),
        );
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

/// Idempotent startup migration to embedded-GUID identity. In a single pass over
/// the profiles directory it backfills GUIDs into legacy profiles, rewrites
/// stored default ids that still hold a folder name, renames legacy folders to
/// match their display name (best-effort), and seeds the resolver cache.
fn migrate_local_profiles() {
    let migration = deadsync_profile::migrate_local_profile_dirs(&profiles_root());
    for backfill in &migration.guid_backfills {
        match &backfill.error {
            None => info!(
                "Assigned profile GUID {} to '{}'.",
                backfill.guid,
                backfill.path.display()
            ),
            Some(e) => warn!(
                "Failed to backfill GUID for '{}': {e}",
                backfill.path.display()
            ),
        }
    }

    // Self-heal stored default ids that still reference a legacy folder name.
    let folder_to_guid: HashMap<&str, &str> = migration
        .entries
        .iter()
        .map(|e| (e.original_folder.as_str(), e.guid.as_str()))
        .collect();
    let (p1, p2) = config::default_profiles();
    let new_p1 = deadsync_profile::heal_default_profile_id(p1.clone(), &folder_to_guid);
    let new_p2 = deadsync_profile::heal_default_profile_id(p2.clone(), &folder_to_guid);
    if new_p1 != p1 || new_p2 != p2 {
        info!("Migrated default profile ids to embedded GUIDs.");
        config::update_default_profiles(new_p1, new_p2);
    }

    for rename in &migration.folder_renames {
        match &rename.error {
            None => info!(
                "Renamed profile folder '{}' -> '{}'.",
                rename.current_folder, rename.desired_folder
            ),
            Some(e) => warn!(
                "Failed to rename profile folder '{}' -> '{}': {e}",
                rename.current_folder, rename.desired_folder
            ),
        }
    }

    // Seed the resolver cache from the post-migration snapshot (dedup duplicate
    // GUIDs by smallest folder name) so the first lookup needn't rescan.
    *PROFILE_DIR_CACHE.lock().unwrap() = Some(migration.cache_map);
}

/// Seeds the session's active profiles from the configured default local
/// profiles. Only applies a saved id when it still refers to an existing local
/// profile; otherwise that side starts as Guest.
fn restore_default_profiles() {
    let (p1, p2) = config::default_profiles();
    let mut session = lock_session();
    session.restore_default_profiles(&[p1, p2], |id| local_profile_dir(id).is_dir());
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
) -> (
    [crate::config::SmxPackName; 2],
    [crate::config::SmxPackName; 2],
) {
    let profiles = lock_profiles();
    deadsync_profile::smx_pack_names_for_profiles(&profiles, machine_bg, machine_judge, |pack| {
        crate::config::SmxPackName::parse(pack)
    })
}

pub fn footer_fields_for_side(side: PlayerSide) -> (Option<String>, String) {
    deadsync_profile::footer_fields_for_side(&lock_profiles(), side)
}

pub fn groovestats_api_key_for_side(side: PlayerSide) -> String {
    deadsync_profile::groovestats_api_key_for_side(&lock_profiles(), side)
}

pub fn gameplay_hud_snapshot() -> GameplayHudSnapshot {
    let (play_style, player_side, joined_mask, active_profiles) = {
        let session = lock_session();
        (
            session.play_style,
            session.player_side,
            session.joined_mask,
            session.active_profiles.clone(),
        )
    };
    let profiles = lock_profiles();
    deadsync_profile::gameplay_hud_snapshot_from_parts(
        play_style,
        player_side,
        joined_mask,
        &active_profiles,
        &profiles,
    )
}

pub fn set_avatar_texture_key_for_side(side: PlayerSide, key: Option<String>) {
    let mut profiles = lock_profiles();
    deadsync_profile::set_avatar_texture_key_for_side(&mut profiles, side, key);
}

// --- Session helpers ---
pub fn get_active_profile_for_side(side: PlayerSide) -> ActiveProfile {
    lock_session().active_profile(side)
}

pub fn active_local_profile_id_for_side(side: PlayerSide) -> Option<String> {
    lock_session()
        .active_local_profile_id(side)
        .map(str::to_owned)
}

pub fn get_default_profile_for_side(side: PlayerSide) -> ActiveProfile {
    let (p1, p2) = config::default_profiles();
    deadsync_profile::default_active_profile_for_side(&[p1, p2], side, |id| {
        local_profile_dir(id).is_dir()
    })
}

pub fn default_local_profile_id_for_side(side: PlayerSide) -> Option<String> {
    match get_default_profile_for_side(side) {
        ActiveProfile::Local { id } => Some(id),
        ActiveProfile::Guest => None,
    }
}

pub fn set_default_profile_for_side(side: PlayerSide, profile: ActiveProfile) {
    let defaults = {
        let (p1, p2) = config::default_profiles();
        [p1, p2]
    };
    let defaults =
        deadsync_profile::default_profile_ids_after_side_update(defaults, side, &profile, |id| {
            is_local_profile_id(id) && local_profile_dir(id).is_dir()
        });
    config::update_default_profiles(defaults[0].clone(), defaults[1].clone());
}

fn update_default_profiles_from_selection(p1: &ActiveProfile, p2: &ActiveProfile) {
    let (p1_default, p2_default) = config::default_profiles();
    let joined_mask = lock_session().joined_mask;
    let defaults = deadsync_profile::default_profile_ids_after_joined_selection(
        [p1_default, p2_default],
        joined_mask,
        p1,
        p2,
    );
    config::update_default_profiles(defaults[0].clone(), defaults[1].clone());
}

/// The local profile that owns a given physical pad. `is_p2_side` is the pad's
/// player side (P2 vs P1), taken from its SDK slot (slot 1 = P2), NOT the raw
/// hardware jumper bit. In Doubles one player drives both pads, so both map to the
/// joined player's side; otherwise the pad maps to its own side.
pub fn active_local_profile_id_for_pad(is_p2_side: bool) -> Option<String> {
    let side = deadsync_profile::side_for_physical_pad(
        get_session_play_style(),
        get_session_player_side(),
        is_p2_side,
    );
    active_local_profile_id_for_side(side)
}

/// Pad-light brightness (0..=100) for the player on a given physical pad slot,
/// using the same side mapping as `active_local_profile_id_for_pad` (Doubles →
/// the one joined player for both pads; otherwise the pad's own side). Reads the
/// active profile's value (guest profiles are seeded from the machine default).
pub fn pad_light_brightness_for_pad(is_p2_side: bool) -> u8 {
    deadsync_profile::pad_light_brightness_for_physical_pad(
        &lock_profiles(),
        get_session_play_style(),
        get_session_player_side(),
        is_p2_side,
    )
}

pub fn known_pack_names_for_local_profile(profile_id: &str) -> Option<HashSet<String>> {
    let session = lock_session();
    let profiles = lock_profiles();
    deadsync_profile::known_pack_names_for_loaded_profile(
        &session.active_profiles,
        &profiles,
        profile_id,
    )
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
        deadsync_profile::mark_known_pack_names_for_loaded_profile(
            &session.active_profiles,
            &mut profiles,
            profile_id,
            &pack_names,
        )
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

fn load_favorites(profile_id: &str) -> HashSet<String> {
    deadsync_profile::load_favorites_dir(&local_profile_dir(profile_id))
}

fn save_favorites(profile_id: &str, favorites: &HashSet<String>) {
    deadsync_profile::save_favorites_dir(&local_profile_dir(profile_id), favorites);
}

/// Writes imported favorites (chart `short_hash`es) into a freshly-created
/// profile's `favorites.txt`, merging with anything already present. Used by the
/// ITGmania importer, which resolves Simply Love song favorites to chart hashes.
pub fn write_imported_favorites(profile_id: &str, hashes: &HashSet<String>) {
    deadsync_profile::merge_imported_favorites_dir(&local_profile_dir(profile_id), hashes);
}

/// Toggle a song's favorite status for the given player side.
/// Returns `true` if the song is now a favorite, `false` if removed.
pub fn toggle_favorite(side: PlayerSide, chart_hash: &str) -> bool {
    let Some(profile_id) = active_local_profile_id_for_side(side) else {
        return false;
    };
    let (is_now_favorite, favorites) = {
        let mut profiles = lock_profiles();
        deadsync_profile::toggle_favorite_for_side(&mut profiles, side, chart_hash)
    };
    save_favorites(&profile_id, &favorites);
    is_now_favorite
}

/// Check if a chart hash is favorited for the given player side.
pub fn is_favorite(side: PlayerSide, chart_hash: &str) -> bool {
    deadsync_profile::profile_has_favorite(&lock_profiles(), side, chart_hash)
}

/// Test/bench helper: mark a chart hash as favorited for the given side in the
/// in-memory profile only, without persisting to disk. Lets benchmarks exercise
/// the favorites render path deterministically.
pub fn seed_session_favorite(side: PlayerSide, chart_hash: &str) {
    let mut profiles = lock_profiles();
    deadsync_profile::seed_favorite_for_side(&mut profiles, side, chart_hash);
}

fn load_favorited_packs(profile_id: &str) -> HashSet<String> {
    deadsync_profile::load_favorited_packs_dir(&local_profile_dir(profile_id))
}

fn save_favorited_packs(profile_id: &str, packs: &HashSet<String>) {
    deadsync_profile::save_favorited_packs_dir(&local_profile_dir(profile_id), packs);
}

/// Toggle a pack's favorite status for the given player side, identifying the
/// pack by its display name. Returns `true` if the pack is
/// now a favorite, `false` if it was removed.
pub fn toggle_pack_favorite(side: PlayerSide, pack_name: &str) -> bool {
    let Some(profile_id) = active_local_profile_id_for_side(side) else {
        return false;
    };
    let (is_now_favorite, packs) = {
        let mut profiles = lock_profiles();
        deadsync_profile::toggle_favorited_pack_for_side(&mut profiles, side, pack_name)
    };
    save_favorited_packs(&profile_id, &packs);
    is_now_favorite
}

/// Check if a pack name is favorited for the given player side.
pub fn is_pack_favorite(side: PlayerSide, pack_name: &str) -> bool {
    deadsync_profile::profile_side_has_favorited_pack(&lock_profiles(), side, pack_name)
}

/// Test/bench helper: mark a pack as favorited for the given side in the
/// in-memory profile only, without persisting to disk.
pub fn seed_session_favorited_pack(side: PlayerSide, pack_name: &str) {
    let mut profiles = lock_profiles();
    deadsync_profile::seed_favorited_pack_for_side(&mut profiles, side, pack_name);
}

pub fn set_active_profile_for_side(side: PlayerSide, profile: ActiveProfile) -> Profile {
    {
        let mut session = lock_session();
        if !session.set_active_profile(side, profile) {
            return get_for_side(side);
        }
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
    let changed = lock_session()
        .restore_joined_default_profiles(&[p1, p2], |id| local_profile_dir(id).is_dir());
    for side in [PlayerSide::P1, PlayerSide::P2] {
        if changed[side_ix(side)] {
            load_for_side(side);
        }
    }
    [get_for_side(PlayerSide::P1), get_for_side(PlayerSide::P2)]
}

pub fn scan_local_profiles() -> Vec<LocalProfileSummary> {
    deadsync_profile::scan_local_profile_summaries(&profiles_root())
}

pub fn create_local_profile(display_name: &str) -> Result<String, std::io::Error> {
    let id = deadsync_profile::create_local_profile_dir(
        &profiles_root(),
        display_name,
        machine_default_noteskin_value(),
        machine_default_light_brightness(),
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

/// Create a new local profile from imported data, writing `profile.ini`,
/// `groovestats.ini`, `arrowcloud.ini`, and copying the avatar. Returns the new
/// profile id. Scores are written separately via
/// [`crate::game::scores::import_local_scores`].
pub fn create_local_profile_from_import(
    data: &ImportProfileData<'_>,
) -> Result<String, std::io::Error> {
    let result = deadsync_profile::create_local_profile_from_import_dir(&profiles_root(), data)?;
    if let Some(e) = result.avatar_copy_error {
        if let Some(src) = data.avatar_src {
            warn!("Failed to copy imported avatar {src:?}: {e}");
        }
    }

    // Make the new GUID -> folder mapping visible to later path lookups (scores,
    // favorites and profile-stats writes all resolve the profile dir by GUID).
    invalidate_profile_dir_cache();

    Ok(result.id)
}

pub fn rename_local_profile(id: &str, display_name: &str) -> Result<(), std::io::Error> {
    let result =
        rename_local_profile_dir(&profiles_root(), &local_profile_dir(id), id, display_name)?;
    if let Some(folder) = result.folder_rename {
        match folder.error {
            None => info!(
                "Renamed profile folder '{}' -> '{}'.",
                folder.current_folder, folder.desired_folder
            ),
            Some(e) => warn!(
                "Failed to rename profile folder '{}' -> '{}': {e}",
                folder.current_folder, folder.desired_folder
            ),
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
            profiles[side_ix(PlayerSide::P1)].display_name = result.display_name.clone();
        }
        if p2_active {
            profiles[side_ix(PlayerSide::P2)].display_name = result.display_name.clone();
        }
    }

    Ok(())
}

pub fn delete_local_profile(id: &str) -> Result<(), std::io::Error> {
    deadsync_profile::delete_local_profile_dir(&local_profile_dir(id), id)?;
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
    lock_session().music_rate()
}

pub fn set_session_music_rate(rate: f32) {
    lock_session().set_music_rate(rate);
}

pub fn get_session_timing_tick_mode() -> TimingTickMode {
    lock_session().timing_tick_mode()
}

pub fn set_session_timing_tick_mode(mode: TimingTickMode) {
    lock_session().set_timing_tick_mode(mode);
}

pub fn get_session_play_style() -> PlayStyle {
    lock_session().play_style()
}

pub fn set_session_play_style(style: PlayStyle) {
    let prev_style = {
        let mut session = lock_session();
        let Some(prev_style) = session.set_play_style(style) else {
            return;
        };
        prev_style
    };

    let mut profiles = lock_profiles();
    for profile in profiles.iter_mut() {
        profile.store_current_player_options(prev_style);
        profile.apply_player_options_for_style(style);
    }
}

pub fn get_session_play_mode() -> PlayMode {
    lock_session().play_mode()
}

pub fn set_session_play_mode(mode: PlayMode) {
    lock_session().set_play_mode(mode);
}

pub fn get_session_player_side() -> PlayerSide {
    lock_session().player_side()
}

pub fn set_session_player_side(side: PlayerSide) {
    lock_session().set_player_side(side);
}

pub fn is_session_side_joined(side: PlayerSide) -> bool {
    lock_session().side_joined(side)
}

pub fn is_session_side_guest(side: PlayerSide) -> bool {
    session_side_is_guest(side)
}

pub fn set_session_joined(p1: bool, p2: bool) {
    lock_session().set_joined_sides(p1, p2);
}

pub fn set_fast_profile_switch_from_select_music(enabled: bool) {
    lock_session().set_fast_profile_switch_from_select_music(enabled);
}

pub fn fast_profile_switch_from_select_music() -> bool {
    lock_session().fast_profile_switch_from_select_music()
}

pub fn take_fast_profile_switch_from_select_music() -> bool {
    lock_session().take_fast_profile_switch_from_select_music()
}

#[cfg(test)]
mod tests {
    use super::{SimpleIni, load_player_options, parse_groovestats_is_pad_player};
    use deadsync_profile::{
        AccelEffectsMask, AppearanceEffectsMask, DEFAULT_BIRTH_YEAR, DEFAULT_WEIGHT_POUNDS,
        ErrorBarMask, ErrorBarStyle, HoldsMask, InsertMask, LastPlayed, LastPlayedCourse,
        LiveTimingStatsMask, MiniIndicatorColor, MiniIndicatorPosition, MiniIndicatorSize,
        MiniIndicatorSubtractiveDisplay, NoCmodAlternative, NoteSkin, PlayStyle, PlayerOptionsData,
        Profile, RemoveMask, TapExplosionMask, TimingWindowsOption, VisualEffectsMask,
        append_player_options_section, error_bar_mask_from_style, error_bar_style_from_mask,
        error_bar_text_from_mask, normalize_tap_explosion_mask, player_options_section,
    };
    use std::str::FromStr;

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
