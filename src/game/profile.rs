//! Local profile storage and the active-session profile state.
//!
//! Identity model: a profile's canonical id is the `Guid` embedded under
//! `[userprofile]` in its `profile.ini`. The on-disk folder name is cosmetic
//! (derived from the display name) and may change freely. Profiles are resolved
//! by GUID via the `deadsync-profile` runtime directory cache.
//!
//! Cache invariant: any code that creates, deletes, renames, or backfills a
//! profile folder MUST invalidate the runtime directory cache afterwards,
//! because a cache miss is treated as authoritative (no rescan on miss).
use crate::config;
use chrono::Local;
use deadlib_platform::dirs;
use deadsync_rules::scroll::ScrollSpeedSetting;
use log::{info, warn};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

mod update;

pub use deadsync_profile::ImportProfileData;
use deadsync_profile::{
    ActiveProfile, ActiveProfileLoadSelection, GameplayHudSnapshot, LocalProfileSummary, NoteSkin,
    PLAYER_SLOTS, PlayMode, PlayStyle, PlayerOptionsData, PlayerSide, Profile, ProfileStats,
    ProfileStatsDecodeError, ProfileStatsLoadError, ProfileStatsWriteError, TimingTickMode,
    is_local_profile_id, player_side_index as side_ix, rename_local_profile_dir,
    runtime_invalidate_profile_dir_cache as invalidate_profile_dir_cache,
    runtime_set_profile_dir_cache,
};
pub use update::*;

#[inline(always)]
fn profiles_root() -> PathBuf {
    dirs::app_dirs().profiles_root()
}

fn warn_duplicate_profile_guid(guid: &str, left: &Path, right: &Path, kept: &Path) {
    warn!(
        "Duplicate profile GUID {} in '{}' and '{}'; using '{}'.",
        guid,
        left.display(),
        right.display(),
        kept.display()
    );
}

/// Folder for a profile id. Ids that aren't valid GUIDs (the guest/default seed,
/// legacy folder-name ids) skip the cache and fall back to a literal folder,
/// which also covers freshly created or not-yet-migrated profiles.
#[inline(always)]
fn local_profile_dir(id: &str) -> PathBuf {
    deadsync_profile::runtime_profile_dir_for_id(&profiles_root(), id, warn_duplicate_profile_guid)
}

#[inline(always)]
pub fn local_profile_dir_for_id(id: &str) -> PathBuf {
    local_profile_dir(id)
}

pub fn local_score_profile_source_for_id(
    profile_id: &str,
    display_name: &str,
) -> deadsync_score::LocalScoreProfileSource {
    deadsync_profile::runtime_local_score_profile_source(
        &profiles_root(),
        profile_id,
        display_name,
        warn_duplicate_profile_guid,
    )
}

pub fn local_score_profile_sources() -> Vec<deadsync_score::LocalScoreProfileSource> {
    deadsync_profile::runtime_local_score_profile_sources(
        &profiles_root(),
        warn_duplicate_profile_guid,
    )
}

pub fn load_pad_configs(profile_id: &str) -> Vec<deadsync_profile::pad_config::PadConfigProfile> {
    deadsync_profile::pad_config::load_profile_id(
        &profiles_root(),
        profile_id,
        warn_duplicate_profile_guid,
    )
}

pub fn save_pad_configs(
    profile_id: &str,
    profiles: &[deadsync_profile::pad_config::PadConfigProfile],
) {
    if let Err(e) = deadsync_profile::pad_config::save_profile_id(
        &profiles_root(),
        profile_id,
        profiles,
        warn_duplicate_profile_guid,
    ) {
        let path = deadsync_profile::pad_config::pad_config_path_for_profile_id(
            &profiles_root(),
            profile_id,
            warn_duplicate_profile_guid,
        );
        warn!("Failed to save {}: {e}", path.display());
    }
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_pad_config(
    profile_id: &str,
    name: &str,
    backend: &str,
    pad_type: Option<String>,
    serial: Option<String>,
    make_default: bool,
    settings: Vec<(String, String)>,
) {
    if let Err(e) = deadsync_profile::pad_config::upsert_profile_id(
        &profiles_root(),
        profile_id,
        name,
        backend,
        pad_type,
        serial,
        make_default,
        settings,
        warn_duplicate_profile_guid,
    ) {
        let path = deadsync_profile::pad_config::pad_config_path_for_profile_id(
            &profiles_root(),
            profile_id,
            warn_duplicate_profile_guid,
        );
        warn!("Failed to save {}: {e}", path.display());
    }
}

pub fn set_default_pad_config(profile_id: &str, serial: &str, name: &str) {
    if let Err(e) = deadsync_profile::pad_config::set_default_profile_id(
        &profiles_root(),
        profile_id,
        serial,
        name,
        warn_duplicate_profile_guid,
    ) {
        let path = deadsync_profile::pad_config::pad_config_path_for_profile_id(
            &profiles_root(),
            profile_id,
            warn_duplicate_profile_guid,
        );
        warn!("Failed to save {}: {e}", path.display());
    }
}

pub fn rename_pad_config(profile_id: &str, old: &str, new: &str) {
    if let Err(e) = deadsync_profile::pad_config::rename_profile_id(
        &profiles_root(),
        profile_id,
        old,
        new,
        warn_duplicate_profile_guid,
    ) {
        let path = deadsync_profile::pad_config::pad_config_path_for_profile_id(
            &profiles_root(),
            profile_id,
            warn_duplicate_profile_guid,
        );
        warn!("Failed to save {}: {e}", path.display());
    }
}

pub fn delete_pad_config(profile_id: &str, name: &str) {
    if let Err(e) = deadsync_profile::pad_config::delete_profile_id(
        &profiles_root(),
        profile_id,
        name,
        warn_duplicate_profile_guid,
    ) {
        let path = deadsync_profile::pad_config::pad_config_path_for_profile_id(
            &profiles_root(),
            profile_id,
            warn_duplicate_profile_guid,
        );
        warn!("Failed to save {}: {e}", path.display());
    }
}

#[inline(always)]
fn session_side_is_guest(side: PlayerSide) -> bool {
    deadsync_profile::runtime_session_side_guest(side)
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
    deadsync_profile::runtime_update_guest_profile_noteskin(setting);
}

fn ensure_local_profile_files(id: &str) -> Result<(), std::io::Error> {
    let dir = local_profile_dir(id);
    info!(
        "Profile files not found, creating defaults in '{}'.",
        dir.display()
    );

    let mut default_profile = deadsync_profile::default_profile_with_machine_settings(
        machine_default_noteskin_value(),
        machine_default_light_brightness(),
    );
    default_profile.calories_burned_day = Local::now().date_naive().to_string();
    deadsync_profile::ensure_local_profile_files_dir(&dir, id, &default_profile)?;

    // A new folder may have appeared; let the resolver pick it up.
    invalidate_profile_dir_cache();
    Ok(())
}

fn save_profile_ini_for_side(side: PlayerSide) {
    if let Some(error) = deadsync_profile::runtime_save_profile_ini_for_side(
        &profiles_root(),
        side,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
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

fn save_profile_stats_for_side(side: PlayerSide) {
    if let Some(error) = deadsync_profile::runtime_write_profile_stats_for_side(
        &profiles_root(),
        side,
        warn_duplicate_profile_guid,
    ) {
        log_profile_stats_write_error(error.profile_id.as_str(), error.error);
    }
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
    if let Some(error) = deadsync_profile::runtime_save_groovestats_credentials_for_side(
        &profiles_root(),
        side,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

fn save_arrowcloud_ini_for_side(side: PlayerSide) {
    if let Some(error) = deadsync_profile::runtime_save_arrowcloud_api_key_for_side(
        &profiles_root(),
        side,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

/// Update the active profile's ArrowCloud API key (in memory + on disk).
/// No-op when the side has no local profile loaded (Guest).
pub fn set_arrowcloud_api_key_for_side(side: PlayerSide, api_key: &str) {
    if let Some(error) = deadsync_profile::runtime_set_arrowcloud_api_key_for_side(
        &profiles_root(),
        side,
        api_key,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

/// Write a new ArrowCloud API key for a profile identified by ID
/// (independent of session sides).  Used by the Manage Local Profiles
/// "Link ArrowCloud" flow where the user picks a profile that isn't
/// necessarily joined on P1 or P2.  Also refreshes the in-memory copy
/// on any session side currently loading that profile, so other screens
/// see the new key immediately.
pub fn set_arrowcloud_api_key_for_id(profile_id: &str, api_key: &str) {
    if let Some(error) = deadsync_profile::runtime_set_arrowcloud_api_key_for_id(
        &profiles_root(),
        profile_id,
        api_key,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

/// Returns the saved ArrowCloud API key (from disk) for a profile
/// identified by id, regardless of whether it's currently loaded on a
/// session side.  Empty string if the profile has no key yet or the
/// file is missing / malformed.
pub fn get_arrowcloud_api_key_for_id(profile_id: &str) -> String {
    deadsync_profile::runtime_read_arrowcloud_api_key_for_id(
        &profiles_root(),
        profile_id,
        warn_duplicate_profile_guid,
    )
}

/// Update the active profile's GrooveStats credentials (API key,
/// username, and `IsPadPlayer=true` — Simply Love parity, see
/// `BGAnimations/ScreenGrooveStatsLogin underlay/default.lua:46`) for
/// the given session side, persisting to its `GrooveStats.ini` on disk.
/// No-op when the side has no local profile loaded (Guest).
pub fn set_groovestats_credentials_for_side(side: PlayerSide, api_key: &str, username: &str) {
    if let Some(error) = deadsync_profile::runtime_set_groovestats_credentials_for_side(
        &profiles_root(),
        side,
        api_key,
        username,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

/// Write new GrooveStats credentials for a profile identified by ID
/// (independent of session sides).  Used by the Manage Local Profiles
/// "Link GrooveStats" flow.  Also refreshes the in-memory copy on any
/// session side currently bound to that profile id.
pub fn set_groovestats_credentials_for_id(profile_id: &str, api_key: &str, username: &str) {
    if let Some(error) = deadsync_profile::runtime_set_groovestats_credentials_for_id(
        &profiles_root(),
        profile_id,
        api_key,
        username,
        warn_duplicate_profile_guid,
    ) {
        warn!("Failed to save {}: {}", error.path.display(), error.error);
    }
}

/// Returns the saved GrooveStats API key (from disk) for a profile
/// identified by id, regardless of whether it's currently loaded on a
/// session side.  `None` if the profile has no key yet or the file is
/// missing / malformed; `Some` always wraps a non-empty trimmed key.
pub fn get_groovestats_api_key_for_id(profile_id: &str) -> Option<String> {
    deadsync_profile::runtime_read_groovestats_api_key_for_id(
        &profiles_root(),
        profile_id,
        warn_duplicate_profile_guid,
    )
}

fn load_for_side(side: PlayerSide) {
    let selection = deadsync_profile::runtime_resolve_active_profile_load_for_side(
        side,
        |id| local_profile_dir(id).is_dir(),
        || scan_local_profiles().into_iter().next().map(|p| p.id),
    );

    match &selection {
        ActiveProfileLoadSelection::MissingFallbackLocal {
            missing_id,
            fallback_id,
        } => {
            info!("Profile folder '{missing_id}' not found; falling back to '{fallback_id}'.");
        }
        ActiveProfileLoadSelection::MissingFallbackGuest { missing_id } => {
            info!(
                "Profile folder '{missing_id}' not found and no other profiles exist; using Guest."
            );
        }
        ActiveProfileLoadSelection::Guest | ActiveProfileLoadSelection::Local { .. } => {}
    }

    let Some(profile_id) = selection.local_id().map(str::to_owned) else {
        deadsync_profile::runtime_set_guest_profile_for_side(
            side,
            machine_default_noteskin_value(),
            machine_default_light_brightness(),
        );
        return;
    };

    let profile_dir = local_profile_dir(&profile_id);
    let profile_ini = deadsync_profile::profile_ini_path(&profile_dir);
    let groovestats_ini = deadsync_profile::groovestats_ini_path(&profile_dir);
    let arrowcloud_ini = deadsync_profile::arrowcloud_ini_path(&profile_dir);
    if (!profile_ini.exists() || !groovestats_ini.exists() || !arrowcloud_ini.exists())
        && let Err(e) = ensure_local_profile_files(&profile_id)
    {
        warn!("Failed to create default profile files: {e}");
        // Proceed with default struct values and attempt to save them.
    }

    let default_profile = deadsync_profile::default_profile_with_machine_settings(
        machine_default_noteskin_value(),
        machine_default_light_brightness(),
    );
    let today = Local::now().date_naive().to_string();
    let load_report = deadsync_profile::runtime_load_profile_data_for_side(
        side,
        &profile_dir,
        &default_profile,
        today.as_str(),
    );
    if !load_report.profile_ini_loaded {
        warn!(
            "Failed to load '{}', using default profile settings.",
            load_report.profile_ini_path.display()
        );
    }
    if !load_report.groovestats_ini_loaded {
        warn!(
            "Failed to load '{}', using default GrooveStats info.",
            load_report.groovestats_ini_path.display()
        );
    }
    if !load_report.arrowcloud_ini_loaded {
        warn!(
            "Failed to load '{}', using default ArrowCloud info.",
            load_report.arrowcloud_ini_path.display()
        );
    }
    if let Some(error) = load_report.stats_error {
        log_profile_stats_load_error(&load_report.stats_path, error);
    }

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
    runtime_set_profile_dir_cache(migration.cache_map);
}

/// Seeds the session's active profiles from the configured default local
/// profiles. Only applies a saved id when it still refers to an existing local
/// profile; otherwise that side starts as Guest.
fn restore_default_profiles() {
    let (p1, p2) = config::default_profiles();
    deadsync_profile::runtime_restore_default_profiles(&[p1, p2], |id| {
        local_profile_dir(id).is_dir()
    });
}

/// Returns a copy of the currently loaded profile data.
pub fn get() -> Profile {
    deadsync_profile::runtime_current_profile()
}

pub fn get_for_side(side: PlayerSide) -> Profile {
    deadsync_profile::runtime_profile_for_side(side)
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
    deadsync_profile::runtime_smx_pack_names_for_profiles(
        machine_bg,
        machine_judge,
        crate::config::SmxPackName::parse,
    )
}

pub fn footer_fields_for_side(side: PlayerSide) -> (Option<String>, String) {
    deadsync_profile::runtime_footer_fields_for_side(side)
}

pub fn groovestats_api_key_for_side(side: PlayerSide) -> String {
    deadsync_profile::runtime_groovestats_api_key_for_side(side)
}

pub fn gameplay_hud_snapshot() -> GameplayHudSnapshot {
    deadsync_profile::runtime_gameplay_hud_snapshot()
}

pub fn set_avatar_texture_key_for_side(side: PlayerSide, key: Option<String>) {
    deadsync_profile::runtime_set_avatar_texture_key_for_side(side, key);
}

// --- Session helpers ---
pub fn get_active_profile_for_side(side: PlayerSide) -> ActiveProfile {
    deadsync_profile::runtime_active_profile_for_side(side)
}

pub fn active_local_profile_id_for_side(side: PlayerSide) -> Option<String> {
    deadsync_profile::runtime_active_local_profile_id_for_side(side)
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

fn update_default_profiles_from_selection() {
    let (p1_default, p2_default) = config::default_profiles();
    let defaults = deadsync_profile::runtime_default_profile_ids_after_current_selection([
        p1_default, p2_default,
    ]);
    config::update_default_profiles(defaults[0].clone(), defaults[1].clone());
}

/// The local profile that owns a given physical pad. `is_p2_side` is the pad's
/// player side (P2 vs P1), taken from its SDK slot (slot 1 = P2), NOT the raw
/// hardware jumper bit. In Doubles one player drives both pads, so both map to the
/// joined player's side; otherwise the pad maps to its own side.
pub fn active_local_profile_id_for_pad(is_p2_side: bool) -> Option<String> {
    deadsync_profile::runtime_local_profile_id_for_pad(is_p2_side)
}

/// Pad-light brightness (0..=100) for the player on a given physical pad slot,
/// using the same side mapping as `active_local_profile_id_for_pad` (Doubles →
/// the one joined player for both pads; otherwise the pad's own side). Reads the
/// active profile's value (guest profiles are seeded from the machine default).
pub fn pad_light_brightness_for_pad(is_p2_side: bool) -> u8 {
    deadsync_profile::runtime_pad_light_brightness_for_pad(is_p2_side)
}

pub fn known_pack_names_for_local_profile(profile_id: &str) -> Option<HashSet<String>> {
    deadsync_profile::runtime_known_pack_names_for_local_profile(profile_id)
}

pub fn mark_known_pack_names_for_local_profile<'a>(
    profile_id: &str,
    pack_names: impl IntoIterator<Item = &'a str>,
) {
    if let Some(error) = deadsync_profile::runtime_mark_known_pack_names_for_local_profile(
        &profiles_root(),
        profile_id,
        pack_names,
        warn_duplicate_profile_guid,
    ) {
        log_profile_stats_write_error(error.profile_id.as_str(), error.error);
    }
}

pub fn sync_known_packs(profile_ids: &[String], scanned_pack_names: &[String]) -> HashSet<String> {
    let result = deadsync_profile::runtime_sync_known_packs(
        &profiles_root(),
        profile_ids,
        scanned_pack_names,
        warn_duplicate_profile_guid,
    );
    for error in result.write_errors {
        log_profile_stats_write_error(error.profile_id.as_str(), error.error);
    }
    result.unknown_pack_names
}

pub fn mark_pack_known(profile_ids: &[String], name: &str) {
    mark_packs_known(profile_ids, std::iter::once(name));
}

pub fn mark_packs_known<'a>(profile_ids: &[String], pack_names: impl IntoIterator<Item = &'a str>) {
    for error in deadsync_profile::runtime_mark_packs_known(
        &profiles_root(),
        profile_ids,
        pack_names,
        warn_duplicate_profile_guid,
    ) {
        log_profile_stats_write_error(error.profile_id.as_str(), error.error);
    }
}

// --- Favorites ---

/// Writes imported favorites (chart `short_hash`es) into a freshly-created
/// profile's `favorites.txt`, merging with anything already present. Used by the
/// ITGmania importer, which resolves Simply Love song favorites to chart hashes.
pub fn write_imported_favorites(profile_id: &str, hashes: &HashSet<String>) {
    deadsync_profile::merge_imported_favorites_dir(&local_profile_dir(profile_id), hashes);
}

/// Toggle a song's favorite status for the given player side.
/// Returns `true` if the song is now a favorite, `false` if removed.
pub fn toggle_favorite(side: PlayerSide, chart_hash: &str) -> bool {
    deadsync_profile::runtime_toggle_favorite_for_side(
        &profiles_root(),
        side,
        chart_hash,
        warn_duplicate_profile_guid,
    )
}

/// Check if a chart hash is favorited for the given player side.
pub fn is_favorite(side: PlayerSide, chart_hash: &str) -> bool {
    deadsync_profile::runtime_profile_has_favorite_for_side(side, chart_hash)
}

/// Test/bench helper: mark a chart hash as favorited for the given side in the
/// in-memory profile only, without persisting to disk. Lets benchmarks exercise
/// the favorites render path deterministically.
pub fn seed_session_favorite(side: PlayerSide, chart_hash: &str) {
    deadsync_profile::runtime_seed_favorite_for_side(side, chart_hash);
}

/// Toggle a pack's favorite status for the given player side, identifying the
/// pack by its display name. Returns `true` if the pack is
/// now a favorite, `false` if it was removed.
pub fn toggle_pack_favorite(side: PlayerSide, pack_name: &str) -> bool {
    deadsync_profile::runtime_toggle_favorited_pack_for_side(
        &profiles_root(),
        side,
        pack_name,
        warn_duplicate_profile_guid,
    )
}

/// Check if a pack name is favorited for the given player side.
pub fn is_pack_favorite(side: PlayerSide, pack_name: &str) -> bool {
    deadsync_profile::runtime_profile_has_favorited_pack_for_side(side, pack_name)
}

/// Test/bench helper: mark a pack as favorited for the given side in the
/// in-memory profile only, without persisting to disk.
pub fn seed_session_favorited_pack(side: PlayerSide, pack_name: &str) {
    deadsync_profile::runtime_seed_favorited_pack_for_side(side, pack_name);
}

pub fn set_active_profile_for_side(side: PlayerSide, profile: ActiveProfile) -> Profile {
    if !deadsync_profile::runtime_set_active_profile_for_side(side, profile) {
        return get_for_side(side);
    }
    load_for_side(side);
    get_for_side(side)
}

pub fn set_active_profiles(p1: ActiveProfile, p2: ActiveProfile) -> [Profile; PLAYER_SLOTS] {
    let changed = deadsync_profile::runtime_set_active_profiles([p1, p2]);
    for side in [PlayerSide::P1, PlayerSide::P2] {
        if changed[side_ix(side)] {
            load_for_side(side);
        }
    }
    update_default_profiles_from_selection();
    [get_for_side(PlayerSide::P1), get_for_side(PlayerSide::P2)]
}

pub fn load_default_profiles_for_joined_sides() -> [Profile; PLAYER_SLOTS] {
    let (p1, p2) = config::default_profiles();
    let changed = deadsync_profile::runtime_restore_joined_default_profiles(&[p1, p2], |id| {
        local_profile_dir(id).is_dir()
    });
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
    let defaults = deadsync_profile::default_profile_ids_after_profile_create(
        [p1_default, p2_default],
        id.clone(),
    );
    config::update_default_profiles(defaults[0].clone(), defaults[1].clone());

    Ok(id)
}

/// Player options seeded from machine defaults for a brand-new local profile,
/// returned as `(singles, doubles)`. Used as the translation base when importing
/// Simply Love settings so unspecified options match a freshly created profile.
pub fn default_local_profile_options() -> (PlayerOptionsData, PlayerOptionsData) {
    deadsync_profile::default_player_options_with_machine_settings(
        machine_default_noteskin_value(),
        machine_default_light_brightness(),
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
    deadsync_profile::runtime_rename_loaded_local_profile(id, &result.display_name);

    Ok(())
}

pub fn delete_local_profile(id: &str) -> Result<(), std::io::Error> {
    deadsync_profile::delete_local_profile_dir(&local_profile_dir(id), id)?;
    invalidate_profile_dir_cache();

    let (p1_default, p2_default) = config::default_profiles();
    let defaults =
        deadsync_profile::default_profile_ids_after_profile_delete([p1_default, p2_default], id);
    config::update_default_profiles(defaults[0].clone(), defaults[1].clone());

    let changed = deadsync_profile::runtime_clear_deleted_local_profile(id);
    for side in [PlayerSide::P1, PlayerSide::P2] {
        if changed[side_ix(side)] {
            load_for_side(side);
        }
    }

    Ok(())
}

pub fn get_session_music_rate() -> f32 {
    deadsync_profile::runtime_session_music_rate()
}

pub fn set_session_music_rate(rate: f32) {
    deadsync_profile::runtime_set_session_music_rate(rate);
}

pub fn get_session_timing_tick_mode() -> TimingTickMode {
    deadsync_profile::runtime_session_timing_tick_mode()
}

pub fn set_session_timing_tick_mode(mode: TimingTickMode) {
    deadsync_profile::runtime_set_session_timing_tick_mode(mode);
}

pub fn get_session_play_style() -> PlayStyle {
    deadsync_profile::runtime_session_play_style()
}

pub fn set_session_play_style(style: PlayStyle) {
    deadsync_profile::runtime_set_session_play_style(style);
}

pub fn get_session_play_mode() -> PlayMode {
    deadsync_profile::runtime_session_play_mode()
}

pub fn set_session_play_mode(mode: PlayMode) {
    deadsync_profile::runtime_set_session_play_mode(mode);
}

pub fn get_session_player_side() -> PlayerSide {
    deadsync_profile::runtime_session_player_side()
}

pub fn set_session_player_side(side: PlayerSide) {
    deadsync_profile::runtime_set_session_player_side(side);
}

pub fn is_session_side_joined(side: PlayerSide) -> bool {
    deadsync_profile::runtime_session_side_joined(side)
}

pub fn is_session_side_guest(side: PlayerSide) -> bool {
    deadsync_profile::runtime_session_side_guest(side)
}

pub fn set_session_joined(p1: bool, p2: bool) {
    deadsync_profile::runtime_set_session_joined(p1, p2);
}

pub fn set_fast_profile_switch_from_select_music(enabled: bool) {
    deadsync_profile::runtime_set_fast_profile_switch_from_select_music(enabled);
}

pub fn fast_profile_switch_from_select_music() -> bool {
    deadsync_profile::runtime_fast_profile_switch_from_select_music()
}

pub fn take_fast_profile_switch_from_select_music() -> bool {
    deadsync_profile::runtime_take_fast_profile_switch_from_select_music()
}
