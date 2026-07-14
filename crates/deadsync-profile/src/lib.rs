use bincode::{Decode, Encode};
use bitflags::bitflags;
use chrono::{Datelike, Local};
use deadsync_rules::judgment::JudgeGrade;
use deadsync_rules::scroll::{GUEST_SCROLL_SPEED, ScrollSpeedSetting};
use deadsync_score::ScoreImportEndpoint;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

pub mod app_runtime;
pub mod compat;
pub mod favorites_view;
pub mod lock_wait;
pub mod pad_config;
pub mod pad_config_sync;
pub mod update;

pub const PLAYER_SLOTS: usize = 2;
pub const SESSION_JOINED_MASK_P1: u8 = 1 << 0;
pub const SESSION_JOINED_MASK_P2: u8 = 1 << 1;
pub const DEFAULT_WEIGHT_POUNDS: i32 = 120;
pub const DEFAULT_BIRTH_YEAR: i32 = 1995;
pub const PLAYER_INITIALS_MAX_LEN: usize = 4;
pub const HUD_OFFSET_MIN: i32 = -250;
pub const HUD_OFFSET_MAX: i32 = 250;
pub const SPACING_PERCENT_MIN: i32 = -100;
pub const SPACING_PERCENT_MAX: i32 = 100;
pub const MINI_PERCENT_MIN: i32 = -100;
pub const MINI_PERCENT_MAX: i32 = 150;
pub const NOTE_FIELD_OFFSET_X_MIN: i32 = 0;
pub const NOTE_FIELD_OFFSET_X_MAX: i32 = 50;
pub const NOTE_FIELD_OFFSET_Y_MIN: i32 = -50;
pub const NOTE_FIELD_OFFSET_Y_MAX: i32 = 50;
pub const VISUAL_DELAY_MS_MIN: i32 = -100;
pub const VISUAL_DELAY_MS_MAX: i32 = 100;
pub const TILT_THRESHOLD_MIN_MS: u32 = 0;
pub const TILT_THRESHOLD_MAX_MS: u32 = 100;
pub const TILT_MIN_THRESHOLD_DEFAULT_MS: u32 = 0;
pub const TILT_MAX_THRESHOLD_DEFAULT_MS: u32 = 50;
pub const LONG_ERROR_BAR_INTENSITY_MIN: f32 = 1.0;
pub const LONG_ERROR_BAR_INTENSITY_MAX: f32 = 4.0;
pub const LONG_ERROR_BAR_INTENSITY_STEP: f32 = 0.25;
pub const LONG_ERROR_BAR_INTENSITY_DEFAULT: f32 = 2.0;
pub const AVERAGE_ERROR_BAR_INTENSITY_MIN: f32 = 1.0;
pub const AVERAGE_ERROR_BAR_INTENSITY_MAX: f32 = 2.0;
pub const AVERAGE_ERROR_BAR_INTENSITY_STEP: f32 = 0.25;
pub const AVERAGE_ERROR_BAR_INTENSITY_DEFAULT: f32 = 1.0;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_MIN: u32 = 100;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_MAX: u32 = 2000;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_STEP: u32 = 100;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_DEFAULT: u32 = 400;
pub const LONG_ERROR_BAR_THRESHOLD_MS_MIN: u32 = 1;
pub const LONG_ERROR_BAR_THRESHOLD_MS_MAX: u32 = 15;
pub const LONG_ERROR_BAR_THRESHOLD_MS_DEFAULT: u32 = 4;
pub const LONG_ERROR_BAR_MIN_SAMPLES_MIN: u32 = 4;
pub const LONG_ERROR_BAR_MIN_SAMPLES_MAX: u32 = 64;
pub const LONG_ERROR_BAR_MIN_SAMPLES_DEFAULT: u32 = 16;
pub const CUSTOM_FANTASTIC_WINDOW_MIN_MS: u8 = 1;
pub const CUSTOM_FANTASTIC_WINDOW_MAX_MS: u8 = 22;
pub const CUSTOM_FANTASTIC_WINDOW_DEFAULT_MS: u8 = 10;

static RUNTIME_PROFILES: LazyLock<Mutex<[Profile; PLAYER_SLOTS]>> =
    LazyLock::new(|| Mutex::new(std::array::from_fn(|_| Profile::default())));

static RUNTIME_SESSION: LazyLock<Mutex<SessionState>> = LazyLock::new(|| {
    // Both sides start as Guest; root profile loading seeds configured defaults.
    Mutex::new(SessionState::default())
});

struct RuntimeProfileDirCache {
    root: PathBuf,
    map: HashMap<String, PathBuf>,
}

static RUNTIME_PROFILE_DIR_CACHE: LazyLock<Mutex<Option<RuntimeProfileDirCache>>> =
    LazyLock::new(|| Mutex::new(None));

static RUNTIME_SESSION_LOCK_WAIT_STATS: lock_wait::LockWaitStats = lock_wait::LockWaitStats::new();
static RUNTIME_PROFILES_LOCK_WAIT_STATS: lock_wait::LockWaitStats = lock_wait::LockWaitStats::new();

#[inline(always)]
pub fn runtime_lock_session() -> MutexGuard<'static, SessionState> {
    lock_wait::lock_with_wait_stats(
        "SESSION",
        &RUNTIME_SESSION_LOCK_WAIT_STATS,
        &RUNTIME_SESSION,
    )
}

#[inline(always)]
pub fn runtime_lock_profiles() -> MutexGuard<'static, [Profile; PLAYER_SLOTS]> {
    lock_wait::lock_with_wait_stats(
        "PROFILES",
        &RUNTIME_PROFILES_LOCK_WAIT_STATS,
        &RUNTIME_PROFILES,
    )
}

pub fn runtime_update_profile_for_side(
    side: PlayerSide,
    update: impl FnOnce(&mut Profile) -> bool,
) -> bool {
    let mut profiles = runtime_lock_profiles();
    update(&mut profiles[player_side_index(side)])
}

pub fn runtime_profile_for_side(side: PlayerSide) -> Profile {
    runtime_lock_profiles()[player_side_index(side)].clone()
}

pub fn profile_combo_carry(profiles: &[Profile; PLAYER_SLOTS]) -> [u32; PLAYER_SLOTS] {
    std::array::from_fn(|idx| profiles[idx].current_combo)
}

pub fn preferred_difficulty_index(profile: &Profile, style: PlayStyle) -> usize {
    profile
        .last_played(style)
        .difficulty_index
        .min(deadsync_chart::STANDARD_DIFFICULTY_COUNT.saturating_sub(1))
}

pub fn preferred_difficulty_indices(
    profiles: &[Profile; PLAYER_SLOTS],
    style: PlayStyle,
) -> [usize; PLAYER_SLOTS] {
    std::array::from_fn(|idx| preferred_difficulty_index(&profiles[idx], style))
}

pub fn preferred_difficulty_index_for_side(
    profiles: &[Profile; PLAYER_SLOTS],
    side: PlayerSide,
    style: PlayStyle,
) -> usize {
    preferred_difficulty_index(&profiles[player_side_index(side)], style)
}

pub fn runtime_profile_combo_carry() -> [u32; PLAYER_SLOTS] {
    profile_combo_carry(&runtime_lock_profiles())
}

pub fn runtime_preferred_difficulty_index_for_side(side: PlayerSide, style: PlayStyle) -> usize {
    let profiles = runtime_lock_profiles();
    preferred_difficulty_index_for_side(&profiles, side, style)
}

pub fn runtime_current_profile() -> Profile {
    let side = runtime_lock_session().player_side();
    runtime_profile_for_side(side)
}

pub fn runtime_footer_fields_for_side(side: PlayerSide) -> (Option<String>, String) {
    footer_fields_for_side(&runtime_lock_profiles(), side)
}

pub fn runtime_groovestats_api_key_for_side(side: PlayerSide) -> String {
    groovestats_api_key_for_side(&runtime_lock_profiles(), side)
}

pub fn runtime_gameplay_hud_snapshot() -> GameplayHudSnapshot {
    let (play_style, player_side, joined_mask, active_profiles) = {
        let session = runtime_lock_session();
        (
            session.play_style,
            session.player_side,
            session.joined_mask,
            session.active_profiles.clone(),
        )
    };
    let profiles = runtime_lock_profiles();
    gameplay_hud_snapshot_from_parts(
        play_style,
        player_side,
        joined_mask,
        &active_profiles,
        &profiles,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPlayersView {
    pub active_side: PlayerSide,
    pub joined: [bool; PLAYER_SLOTS],
    pub display_names: [String; PLAYER_SLOTS],
}

pub fn session_players_view(
    profiles: &[Profile; PLAYER_SLOTS],
    joined_mask: u8,
    active_side: PlayerSide,
) -> SessionPlayersView {
    SessionPlayersView {
        active_side,
        joined: [
            player_side_is_joined(joined_mask, PlayerSide::P1),
            player_side_is_joined(joined_mask, PlayerSide::P2),
        ],
        display_names: std::array::from_fn(|side_idx| profiles[side_idx].display_name.clone()),
    }
}

pub fn runtime_session_players_view() -> SessionPlayersView {
    let (joined_mask, active_side) = {
        let session = runtime_lock_session();
        (session.joined_mask, session.player_side)
    };
    session_players_view(&runtime_lock_profiles(), joined_mask, active_side)
}

pub fn runtime_set_avatar_texture_key_for_side(side: PlayerSide, key: Option<String>) {
    let mut profiles = runtime_lock_profiles();
    set_avatar_texture_key_for_side(&mut profiles, side, key);
}

pub fn runtime_update_guest_profile_noteskin(noteskin: NoteSkin) {
    let active_profiles = runtime_lock_session().active_profiles.clone();
    let mut profiles = runtime_lock_profiles();
    update_guest_profile_noteskins(&active_profiles, &mut profiles, noteskin);
}

pub fn runtime_active_profile_for_side(side: PlayerSide) -> ActiveProfile {
    runtime_lock_session().active_profile(side)
}

pub fn runtime_active_profiles() -> [ActiveProfile; PLAYER_SLOTS] {
    runtime_lock_session().active_profiles.clone()
}

pub fn runtime_set_active_profile_for_side(side: PlayerSide, profile: ActiveProfile) -> bool {
    runtime_lock_session().set_active_profile(side, profile)
}

pub fn runtime_set_active_profiles(
    profiles: [ActiveProfile; PLAYER_SLOTS],
) -> [bool; PLAYER_SLOTS] {
    let mut changed = [false; PLAYER_SLOTS];
    let mut session = runtime_lock_session();
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let idx = player_side_index(side);
        changed[idx] = session.set_active_profile(side, profiles[idx].clone());
    }
    changed
}

pub fn runtime_rename_loaded_local_profile(profile_id: &str, display_name: &str) {
    let active_profiles = runtime_lock_session().active_profiles.clone();
    let mut profiles = runtime_lock_profiles();
    rename_loaded_local_profile(&active_profiles, &mut profiles, profile_id, display_name);
}

pub fn runtime_clear_deleted_local_profile(profile_id: &str) -> [bool; PLAYER_SLOTS] {
    let mut changed = [false; PLAYER_SLOTS];
    let mut session = runtime_lock_session();
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let side_idx = player_side_index(side);
        if active_profile_local_id(&session.active_profiles[side_idx]) == Some(profile_id) {
            changed[side_idx] = session.set_active_profile(side, ActiveProfile::Guest);
        }
    }
    changed
}

pub struct RuntimeLocalProfileCreateResult {
    pub id: String,
    pub default_profiles: [Option<String>; PLAYER_SLOTS],
}

pub fn runtime_create_local_profile(
    root: &Path,
    display_name: &str,
    noteskin: NoteSkin,
    pad_light_brightness: u8,
    default_profiles: [Option<String>; PLAYER_SLOTS],
) -> Result<RuntimeLocalProfileCreateResult, std::io::Error> {
    let id = create_local_profile_dir(root, display_name, noteskin, pad_light_brightness)?;
    runtime_invalidate_profile_dir_cache();
    let default_profiles = default_profile_ids_after_profile_create(default_profiles, id.clone());
    Ok(RuntimeLocalProfileCreateResult {
        id,
        default_profiles,
    })
}

pub fn runtime_create_local_profile_from_import(
    root: &Path,
    data: &ImportProfileData<'_>,
) -> Result<ImportedProfileCreateResult, std::io::Error> {
    let result = create_local_profile_from_import_dir(root, data)?;
    runtime_invalidate_profile_dir_cache();
    Ok(result)
}

pub fn runtime_rename_local_profile(
    root: &Path,
    current_dir: &Path,
    id: &str,
    display_name: &str,
) -> Result<LocalProfileRenameResult, std::io::Error> {
    let result = rename_local_profile_dir(root, current_dir, id, display_name)?;
    runtime_invalidate_profile_dir_cache();
    runtime_rename_loaded_local_profile(id, &result.display_name);
    Ok(result)
}

pub struct RuntimeLocalProfileDeleteResult {
    pub default_profiles: [Option<String>; PLAYER_SLOTS],
    pub changed_sides: [bool; PLAYER_SLOTS],
}

pub fn runtime_delete_local_profile(
    dir: &Path,
    id: &str,
    default_profiles: [Option<String>; PLAYER_SLOTS],
) -> Result<RuntimeLocalProfileDeleteResult, std::io::Error> {
    delete_local_profile_dir(dir, id)?;
    runtime_invalidate_profile_dir_cache();
    let default_profiles = default_profile_ids_after_profile_delete(default_profiles, id);
    let changed_sides = runtime_clear_deleted_local_profile(id);
    Ok(RuntimeLocalProfileDeleteResult {
        default_profiles,
        changed_sides,
    })
}

pub fn runtime_resolve_active_profile_load_for_side(
    side: PlayerSide,
    local_profile_exists: impl FnOnce(&str) -> bool,
    fallback_local_profile_id: impl FnOnce() -> Option<String>,
) -> ActiveProfileLoadSelection {
    let active = runtime_lock_session().active_profiles[player_side_index(side)].clone();
    let selection =
        resolve_active_profile_for_load(&active, local_profile_exists, fallback_local_profile_id);
    if matches!(
        selection,
        ActiveProfileLoadSelection::MissingFallbackLocal { .. }
            | ActiveProfileLoadSelection::MissingFallbackGuest { .. }
    ) {
        runtime_lock_session().active_profiles[player_side_index(side)] =
            selection.session_profile();
    }
    selection
}

pub fn guest_profile(noteskin: NoteSkin, pad_light_brightness: u8) -> Profile {
    let mut guest = Profile::default();
    guest.display_name = "[ GUEST ]".to_string();
    guest.scroll_speed = GUEST_SCROLL_SPEED;
    guest.noteskin = noteskin;
    guest.pad_light_brightness = clamp_pad_light_brightness(pad_light_brightness);
    guest.avatar_path = None;
    guest.avatar_texture_key = None;
    guest.store_current_player_options_for_all_styles();
    guest
}

pub fn default_profile_with_machine_settings(
    noteskin: NoteSkin,
    pad_light_brightness: u8,
) -> Profile {
    let mut profile = Profile::default();
    profile.noteskin = noteskin;
    profile.pad_light_brightness = clamp_pad_light_brightness(pad_light_brightness);
    profile.store_current_player_options_for_all_styles();
    profile
}

pub fn runtime_set_guest_profile_for_side(
    side: PlayerSide,
    noteskin: NoteSkin,
    pad_light_brightness: u8,
) {
    runtime_lock_profiles()[player_side_index(side)] =
        guest_profile(noteskin, pad_light_brightness);
}

#[allow(clippy::too_many_arguments)]
pub fn runtime_apply_loaded_profile_data_for_side(
    side: PlayerSide,
    default_profile: &Profile,
    today: &str,
    profile_ini_loaded: bool,
    profile_section_has_any: impl FnMut(&str) -> bool,
    profile_get: impl FnMut(&str, &str) -> Option<String>,
    stats: ProfileStats,
    favorites: HashSet<String>,
    favorited_packs: HashSet<String>,
    groovestats_ini_loaded: bool,
    groovestats_get: impl FnMut(&str, &str) -> Option<String>,
    arrowcloud_ini_loaded: bool,
    arrowcloud_get: impl FnMut(&str, &str) -> Option<String>,
    avatar_path: Option<PathBuf>,
) {
    let play_style = runtime_session_play_style();
    let mut profiles = runtime_lock_profiles();
    let profile = &mut profiles[player_side_index(side)];
    apply_loaded_profile_data(
        profile,
        default_profile,
        play_style,
        today,
        profile_ini_loaded,
        profile_section_has_any,
        profile_get,
        stats,
        favorites,
        favorited_packs,
        groovestats_ini_loaded,
        groovestats_get,
        arrowcloud_ini_loaded,
        arrowcloud_get,
    );
    profile.avatar_path = avatar_path;
    profile.avatar_texture_key = None;
}

#[derive(Debug)]
pub struct RuntimeProfileLoadReport {
    pub profile_ini_path: PathBuf,
    pub profile_ini_loaded: bool,
    pub groovestats_ini_path: PathBuf,
    pub groovestats_ini_loaded: bool,
    pub arrowcloud_ini_path: PathBuf,
    pub arrowcloud_ini_loaded: bool,
    pub stats_path: PathBuf,
    pub stats_error: Option<ProfileStatsLoadError>,
}

pub fn runtime_load_profile_data_for_side(
    side: PlayerSide,
    profile_dir: &Path,
    default_profile: &Profile,
    today: &str,
) -> RuntimeProfileLoadReport {
    let profile_ini_path = profile_ini_path(profile_dir);
    let groovestats_ini_path = groovestats_ini_path(profile_dir);
    let arrowcloud_ini_path = arrowcloud_ini_path(profile_dir);
    let stats_path = profile_stats_path(profile_dir);
    let profile_ini = ProfileIni::load(&profile_ini_path).ok();
    let groovestats_ini = ProfileIni::load(&groovestats_ini_path).ok();
    let arrowcloud_ini = ProfileIni::load(&arrowcloud_ini_path).ok();

    let ProfileSidecarLoadData {
        stats,
        stats_error,
        favorites,
        favorited_packs,
        avatar_path,
    } = load_profile_sidecars_dir(profile_dir, default_profile);

    runtime_apply_loaded_profile_data_for_side(
        side,
        default_profile,
        today,
        profile_ini.is_some(),
        |section| {
            profile_ini
                .as_ref()
                .is_some_and(|ini| ini.section_has_any(section))
        },
        |section, key| profile_ini.as_ref().and_then(|ini| ini.get(section, key)),
        stats,
        favorites,
        favorited_packs,
        groovestats_ini.is_some(),
        |section, key| {
            groovestats_ini
                .as_ref()
                .and_then(|ini| ini.get(section, key))
        },
        arrowcloud_ini.is_some(),
        |section, key| {
            arrowcloud_ini
                .as_ref()
                .and_then(|ini| ini.get(section, key))
        },
        avatar_path,
    );

    RuntimeProfileLoadReport {
        profile_ini_path,
        profile_ini_loaded: profile_ini.is_some(),
        groovestats_ini_path,
        groovestats_ini_loaded: groovestats_ini.is_some(),
        arrowcloud_ini_path,
        arrowcloud_ini_loaded: arrowcloud_ini.is_some(),
        stats_path,
        stats_error,
    }
}

#[derive(Debug)]
pub struct RuntimeProfileSideLoadReport {
    pub selection: ActiveProfileLoadSelection,
    pub default_files_dir: Option<PathBuf>,
    pub default_files_error: Option<std::io::Error>,
    pub load_report: Option<RuntimeProfileLoadReport>,
    pub profile_ini_save_error: Option<RuntimeProfileSidecarWriteError>,
    pub stats_save_error: Option<RuntimeProfileStatsWriteError>,
    pub groovestats_save_error: Option<RuntimeProfileSidecarWriteError>,
    pub arrowcloud_save_error: Option<RuntimeProfileSidecarWriteError>,
}

impl RuntimeProfileSideLoadReport {
    fn guest(selection: ActiveProfileLoadSelection) -> Self {
        Self {
            selection,
            default_files_dir: None,
            default_files_error: None,
            load_report: None,
            profile_ini_save_error: None,
            stats_save_error: None,
            groovestats_save_error: None,
            arrowcloud_save_error: None,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn runtime_load_profile_for_side(
    root: &Path,
    side: PlayerSide,
    default_profile: &Profile,
    today: &str,
    guest_noteskin: NoteSkin,
    guest_pad_light_brightness: u8,
    local_profile_exists: impl FnOnce(&str) -> bool,
    fallback_local_profile_id: impl FnOnce() -> Option<String>,
    mut duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> RuntimeProfileSideLoadReport {
    let selection = runtime_resolve_active_profile_load_for_side(
        side,
        local_profile_exists,
        fallback_local_profile_id,
    );
    let Some(profile_id) = selection.local_id().map(str::to_owned) else {
        runtime_set_guest_profile_for_side(side, guest_noteskin, guest_pad_light_brightness);
        return RuntimeProfileSideLoadReport::guest(selection);
    };

    let profile_dir = runtime_profile_dir_for_id(root, &profile_id, &mut duplicate);
    let missing_default_files = !profile_ini_path(&profile_dir).exists()
        || !groovestats_ini_path(&profile_dir).exists()
        || !arrowcloud_ini_path(&profile_dir).exists();
    let (default_files_dir, default_files_error) = if missing_default_files {
        match ensure_local_profile_files_dir(&profile_dir, &profile_id, default_profile) {
            Ok(()) => (Some(profile_dir.clone()), None),
            Err(error) => (Some(profile_dir.clone()), Some(error)),
        }
    } else {
        (None, None)
    };

    let load_report =
        runtime_load_profile_data_for_side(side, &profile_dir, default_profile, today);
    let profile_ini_save_error = runtime_save_profile_ini_for_side(root, side, &mut duplicate);
    let stats_save_error = runtime_write_profile_stats_for_side(root, side, &mut duplicate);
    let groovestats_save_error =
        runtime_save_groovestats_credentials_for_side(root, side, &mut duplicate);
    let arrowcloud_save_error =
        runtime_save_arrowcloud_api_key_for_side(root, side, &mut duplicate);

    RuntimeProfileSideLoadReport {
        selection,
        default_files_dir,
        default_files_error,
        load_report: Some(load_report),
        profile_ini_save_error,
        stats_save_error,
        groovestats_save_error,
        arrowcloud_save_error,
    }
}

pub fn runtime_restore_default_profiles(
    defaults: &[Option<String>; PLAYER_SLOTS],
    local_profile_exists: impl FnMut(&str) -> bool,
) {
    runtime_lock_session().restore_default_profiles(defaults, local_profile_exists);
}

pub fn runtime_restore_joined_default_profiles(
    defaults: &[Option<String>; PLAYER_SLOTS],
    local_profile_exists: impl FnMut(&str) -> bool,
) -> [bool; PLAYER_SLOTS] {
    runtime_lock_session().restore_joined_default_profiles(defaults, local_profile_exists)
}

pub fn runtime_default_profile_ids_after_current_selection(
    defaults: [Option<String>; PLAYER_SLOTS],
) -> [Option<String>; PLAYER_SLOTS] {
    let session = runtime_lock_session();
    default_profile_ids_after_joined_selection(
        defaults,
        session.joined_mask,
        &session.active_profiles[player_side_index(PlayerSide::P1)],
        &session.active_profiles[player_side_index(PlayerSide::P2)],
    )
}

pub fn default_player_options_with_machine_settings(
    noteskin: NoteSkin,
    pad_light_brightness: u8,
) -> (PlayerOptionsData, PlayerOptionsData) {
    let profile = default_profile_with_machine_settings(noteskin, pad_light_brightness);
    (
        profile.player_options_singles,
        profile.player_options_doubles,
    )
}

pub fn runtime_local_profile_id_for_pad(is_p2_side: bool) -> Option<String> {
    let side = {
        let session = runtime_lock_session();
        side_for_physical_pad(session.play_style, session.player_side, is_p2_side)
    };
    runtime_active_local_profile_id_for_side(side)
}

pub fn runtime_pad_light_brightness() -> [u8; PLAYER_SLOTS] {
    let (play_style, player_side) = {
        let session = runtime_lock_session();
        (session.play_style, session.player_side)
    };
    let profiles = runtime_lock_profiles();
    std::array::from_fn(|pad| {
        pad_light_brightness_for_physical_pad(&profiles, play_style, player_side, pad == 1)
    })
}

pub fn runtime_smx_pack_names_for_profiles<T: Copy>(
    machine_bg: T,
    machine_judge: T,
    parse: impl FnMut(&str) -> T,
) -> ([T; PLAYER_SLOTS], [T; PLAYER_SLOTS]) {
    smx_pack_names_for_profiles(&runtime_lock_profiles(), machine_bg, machine_judge, parse)
}

pub fn runtime_session_music_rate() -> f32 {
    runtime_lock_session().music_rate()
}

pub fn runtime_session_snapshot() -> SessionSnapshot {
    SessionSnapshot::from_state(&runtime_lock_session())
}

pub fn runtime_session_snapshot_with_active_ids()
-> (SessionSnapshot, [Option<String>; PLAYER_SLOTS]) {
    let session = runtime_lock_session();
    session_snapshot_with_active_ids(&session)
}

fn session_snapshot_with_active_ids(
    session: &SessionState,
) -> (SessionSnapshot, [Option<String>; PLAYER_SLOTS]) {
    (
        SessionSnapshot::from_state(session),
        std::array::from_fn(|idx| {
            session
                .active_local_profile_id(player_side_for_index(idx))
                .map(str::to_owned)
        }),
    )
}

pub fn runtime_set_session_music_rate(rate: f32) {
    runtime_lock_session().set_music_rate(rate);
}

pub fn runtime_session_timing_tick_mode() -> TimingTickMode {
    runtime_lock_session().timing_tick_mode()
}

pub fn runtime_set_session_timing_tick_mode(mode: TimingTickMode) {
    runtime_lock_session().set_timing_tick_mode(mode);
}

pub fn runtime_session_play_style() -> PlayStyle {
    runtime_lock_session().play_style()
}

pub fn runtime_set_session_play_style(style: PlayStyle) {
    let prev_style = {
        let mut session = runtime_lock_session();
        let Some(prev_style) = session.set_play_style(style) else {
            return;
        };
        prev_style
    };

    let mut profiles = runtime_lock_profiles();
    for profile in profiles.iter_mut() {
        profile.store_current_player_options(prev_style);
        profile.apply_player_options_for_style(style);
    }
}

pub fn runtime_session_play_mode() -> PlayMode {
    runtime_lock_session().play_mode()
}

pub fn runtime_set_session_play_mode(mode: PlayMode) {
    runtime_lock_session().set_play_mode(mode);
}

pub fn runtime_session_player_side() -> PlayerSide {
    runtime_lock_session().player_side()
}

pub fn runtime_set_session_player_side(side: PlayerSide) {
    runtime_lock_session().set_player_side(side);
}

pub fn runtime_session_side_joined(side: PlayerSide) -> bool {
    runtime_lock_session().side_joined(side)
}

pub fn runtime_session_joined_mask() -> u8 {
    runtime_lock_session().joined_mask
}

pub fn runtime_session_side_guest(side: PlayerSide) -> bool {
    active_profile_is_guest(&runtime_lock_session().active_profiles[player_side_index(side)])
}

pub fn runtime_set_session_joined(p1: bool, p2: bool) {
    runtime_lock_session().set_joined_sides(p1, p2);
}

pub fn runtime_invalidate_profile_dir_cache() {
    *RUNTIME_PROFILE_DIR_CACHE.lock().unwrap() = None;
}

pub fn runtime_set_profile_dir_cache(root: &Path, cache_map: HashMap<String, PathBuf>) {
    *RUNTIME_PROFILE_DIR_CACHE.lock().unwrap() = Some(RuntimeProfileDirCache {
        root: root.to_path_buf(),
        map: cache_map,
    });
}

pub fn runtime_resolve_profile_dir(
    root: &Path,
    guid: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<PathBuf> {
    let mut guard = RUNTIME_PROFILE_DIR_CACHE.lock().unwrap();
    if !guard
        .as_ref()
        .map(|cache| cache.root == root)
        .unwrap_or(false)
    {
        *guard = Some(RuntimeProfileDirCache {
            root: root.to_path_buf(),
            map: build_profile_dir_map(root, duplicate),
        });
    }
    guard.as_ref().unwrap().map.get(guid).cloned()
}

pub fn runtime_profile_dir_for_id(
    root: &Path,
    id: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> PathBuf {
    if is_valid_profile_guid(id)
        && let Some(dir) = runtime_resolve_profile_dir(root, id, duplicate)
    {
        return dir;
    }
    root.join(id)
}

pub fn runtime_active_local_profile_id_for_side(side: PlayerSide) -> Option<String> {
    runtime_lock_session()
        .active_local_profile_id(side)
        .map(str::to_owned)
}

pub fn runtime_toggle_favorite_for_side(
    root: &Path,
    side: PlayerSide,
    chart_hash: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> bool {
    let Some(profile_id) = runtime_active_local_profile_id_for_side(side) else {
        return false;
    };
    let (is_now_favorite, favorites) = {
        let mut profiles = runtime_lock_profiles();
        toggle_favorite_for_side(&mut profiles, side, chart_hash)
    };
    save_favorites_dir(
        &runtime_profile_dir_for_id(root, &profile_id, duplicate),
        &favorites,
    );
    is_now_favorite
}

pub fn runtime_profile_has_favorite_for_side(side: PlayerSide, chart_hash: &str) -> bool {
    let profiles = runtime_lock_profiles();
    profile_has_favorite(&profiles, side, chart_hash)
}

pub fn runtime_favorite_membership<const N: usize>(
    queries: &[FavoriteMembershipQuery<'_>; N],
) -> [[bool; PLAYER_SLOTS]; N] {
    favorite_membership(&runtime_lock_profiles(), queries)
}

pub fn runtime_seed_favorite_for_side(side: PlayerSide, chart_hash: &str) {
    let mut profiles = runtime_lock_profiles();
    seed_favorite_for_side(&mut profiles, side, chart_hash);
}

pub fn runtime_toggle_favorited_pack_for_side(
    root: &Path,
    side: PlayerSide,
    pack_name: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> bool {
    let Some(profile_id) = runtime_active_local_profile_id_for_side(side) else {
        return false;
    };
    let (is_now_favorite, packs) = {
        let mut profiles = runtime_lock_profiles();
        toggle_favorited_pack_for_side(&mut profiles, side, pack_name)
    };
    save_favorited_packs_dir(
        &runtime_profile_dir_for_id(root, &profile_id, duplicate),
        &packs,
    );
    is_now_favorite
}

pub fn runtime_profile_has_favorited_pack_for_side(side: PlayerSide, pack_name: &str) -> bool {
    let profiles = runtime_lock_profiles();
    profile_side_has_favorited_pack(&profiles, side, pack_name)
}

pub fn runtime_seed_favorited_pack_for_side(side: PlayerSide, pack_name: &str) {
    let mut profiles = runtime_lock_profiles();
    seed_favorited_pack_for_side(&mut profiles, side, pack_name);
}

#[derive(Debug)]
pub struct RuntimeProfileStatsWriteError {
    pub profile_id: String,
    pub error: ProfileStatsWriteError,
}

#[derive(Debug, Default)]
pub struct RuntimeKnownPackSyncResult {
    pub unknown_pack_names: HashSet<String>,
    pub write_errors: Vec<RuntimeProfileStatsWriteError>,
}

fn runtime_profile_stats_payload_for_side(side: PlayerSide) -> Option<(String, ProfileStats)> {
    let session = runtime_lock_session();
    let ActiveProfile::Local { id } = &session.active_profiles[player_side_index(side)] else {
        return None;
    };
    let profile = runtime_lock_profiles()[player_side_index(side)].clone();
    Some((
        id.clone(),
        ProfileStats {
            current_combo: profile.current_combo,
            known_pack_names: profile.known_pack_names,
        },
    ))
}

pub fn runtime_write_profile_stats_for_side(
    root: &Path,
    side: PlayerSide,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileStatsWriteError> {
    let (profile_id, payload) = runtime_profile_stats_payload_for_side(side)?;
    write_profile_stats_dir(
        &runtime_profile_dir_for_id(root, &profile_id, duplicate),
        &payload,
    )
    .err()
    .map(|error| RuntimeProfileStatsWriteError { profile_id, error })
}

pub fn runtime_known_pack_names_for_local_profile(profile_id: &str) -> Option<HashSet<String>> {
    let session = runtime_lock_session();
    let profiles = runtime_lock_profiles();
    known_pack_names_for_loaded_profile(&session.active_profiles, &profiles, profile_id)
}

pub fn runtime_mark_known_pack_names_for_local_profile<'a>(
    root: &Path,
    profile_id: &str,
    pack_names: impl IntoIterator<Item = &'a str>,
    mut duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileStatsWriteError> {
    let pack_names: Vec<&str> = pack_names.into_iter().collect();
    if profile_id.is_empty() || pack_names.is_empty() {
        return None;
    }
    let save_side = {
        let session = runtime_lock_session();
        let mut profiles = runtime_lock_profiles();
        mark_known_pack_names_for_loaded_profile(
            &session.active_profiles,
            &mut profiles,
            profile_id,
            &pack_names,
        )
    };
    save_side.and_then(|side| runtime_write_profile_stats_for_side(root, side, &mut duplicate))
}

pub fn runtime_sync_known_packs(
    root: &Path,
    profile_ids: &[String],
    scanned_pack_names: &[String],
    mut duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> RuntimeKnownPackSyncResult {
    if profile_ids.is_empty() {
        return RuntimeKnownPackSyncResult::default();
    }

    let mut result = RuntimeKnownPackSyncResult::default();
    for profile_id in profile_ids {
        let known = runtime_known_pack_names_for_local_profile(profile_id).unwrap_or_default();
        if known.is_empty() && !scanned_pack_names.is_empty() {
            if let Some(error) = runtime_mark_known_pack_names_for_local_profile(
                root,
                profile_id,
                scanned_pack_names.iter().map(String::as_str),
                &mut duplicate,
            ) {
                result.write_errors.push(error);
            }
            continue;
        }
        result
            .unknown_pack_names
            .extend(unknown_pack_names(&known, scanned_pack_names));
    }
    result
}

pub fn runtime_mark_packs_known<'a>(
    root: &Path,
    profile_ids: &[String],
    pack_names: impl IntoIterator<Item = &'a str>,
    mut duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Vec<RuntimeProfileStatsWriteError> {
    let pack_names: Vec<&str> = pack_names.into_iter().collect();
    if profile_ids.is_empty() || pack_names.is_empty() {
        return Vec::new();
    }
    profile_ids
        .iter()
        .filter_map(|profile_id| {
            runtime_mark_known_pack_names_for_local_profile(
                root,
                profile_id,
                pack_names.iter().copied(),
                &mut duplicate,
            )
        })
        .collect()
}

#[derive(Debug)]
pub struct RuntimeProfileSidecarWriteError {
    pub path: PathBuf,
    pub error: std::io::Error,
}

pub fn runtime_save_profile_ini_for_side(
    root: &Path,
    side: PlayerSide,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileSidecarWriteError> {
    let profile_id = runtime_active_local_profile_id_for_side(side)?;
    let play_style = runtime_session_play_style();
    let profile = {
        let mut profiles = runtime_lock_profiles();
        let profile = &mut profiles[player_side_index(side)];
        profile.store_current_player_options(play_style);
        profile.clone()
    };
    let dir = runtime_profile_dir_for_profile_id(root, &profile_id, duplicate);
    write_profile_ini_dir(&dir, &profile_id, &profile)
        .err()
        .map(|error| RuntimeProfileSidecarWriteError {
            path: profile_ini_path(&dir),
            error,
        })
}

fn runtime_profile_dir_for_profile_id(
    root: &Path,
    profile_id: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> PathBuf {
    runtime_profile_dir_for_id(root, profile_id, duplicate)
}

fn runtime_active_profile_id_and_data_for_side(side: PlayerSide) -> Option<(String, Profile)> {
    let profile_id = runtime_active_local_profile_id_for_side(side)?;
    let profile = runtime_lock_profiles()[player_side_index(side)].clone();
    Some((profile_id, profile))
}

pub fn runtime_save_groovestats_credentials_for_side(
    root: &Path,
    side: PlayerSide,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileSidecarWriteError> {
    let (profile_id, profile) = runtime_active_profile_id_and_data_for_side(side)?;
    let dir = runtime_profile_dir_for_profile_id(root, &profile_id, duplicate);
    write_groovestats_credentials_dir(
        &dir,
        &profile.groovestats_api_key,
        profile.groovestats_is_pad_player,
        &profile.groovestats_username,
    )
    .err()
    .map(|error| RuntimeProfileSidecarWriteError {
        path: groovestats_ini_path(&dir),
        error,
    })
}

pub fn runtime_save_arrowcloud_api_key_for_side(
    root: &Path,
    side: PlayerSide,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileSidecarWriteError> {
    let (profile_id, profile) = runtime_active_profile_id_and_data_for_side(side)?;
    let dir = runtime_profile_dir_for_profile_id(root, &profile_id, duplicate);
    write_arrowcloud_api_key_dir(&dir, &profile.arrowcloud_api_key)
        .err()
        .map(|error| RuntimeProfileSidecarWriteError {
            path: arrowcloud_ini_path(&dir),
            error,
        })
}

pub fn runtime_set_arrowcloud_api_key_for_side(
    root: &Path,
    side: PlayerSide,
    api_key: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileSidecarWriteError> {
    {
        let mut profiles = runtime_lock_profiles();
        set_arrowcloud_api_key_for_side(&mut profiles, side, api_key);
    }
    runtime_save_arrowcloud_api_key_for_side(root, side, duplicate)
}

pub fn runtime_set_arrowcloud_api_key_for_id(
    root: &Path,
    profile_id: &str,
    api_key: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileSidecarWriteError> {
    {
        let session = runtime_lock_session();
        let mut profiles = runtime_lock_profiles();
        set_arrowcloud_api_key_for_loaded_profile(
            &session.active_profiles,
            &mut profiles,
            profile_id,
            api_key,
        );
    }

    let dir = runtime_profile_dir_for_profile_id(root, profile_id, duplicate);
    write_arrowcloud_api_key_dir(&dir, api_key)
        .err()
        .map(|error| RuntimeProfileSidecarWriteError {
            path: arrowcloud_ini_path(&dir),
            error,
        })
}

pub fn runtime_read_arrowcloud_api_key_for_id(
    root: &Path,
    profile_id: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> String {
    read_arrowcloud_api_key_dir(&runtime_profile_dir_for_profile_id(
        root, profile_id, duplicate,
    ))
}

pub fn runtime_set_groovestats_credentials_for_side(
    root: &Path,
    side: PlayerSide,
    api_key: &str,
    username: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileSidecarWriteError> {
    {
        let mut profiles = runtime_lock_profiles();
        set_groovestats_credentials_for_side(&mut profiles, side, api_key, username);
    }
    runtime_save_groovestats_credentials_for_side(root, side, duplicate)
}

pub fn runtime_set_groovestats_credentials_for_id(
    root: &Path,
    profile_id: &str,
    api_key: &str,
    username: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<RuntimeProfileSidecarWriteError> {
    {
        let session = runtime_lock_session();
        let mut profiles = runtime_lock_profiles();
        set_groovestats_credentials_for_loaded_profile(
            &session.active_profiles,
            &mut profiles,
            profile_id,
            api_key,
            username,
        );
    }

    let dir = runtime_profile_dir_for_profile_id(root, profile_id, duplicate);
    write_groovestats_credentials_dir(&dir, api_key, true, username)
        .err()
        .map(|error| RuntimeProfileSidecarWriteError {
            path: groovestats_ini_path(&dir),
            error,
        })
}

pub fn runtime_read_groovestats_api_key_for_id(
    root: &Path,
    profile_id: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Option<String> {
    read_groovestats_api_key_dir(&runtime_profile_dir_for_profile_id(
        root, profile_id, duplicate,
    ))
}

// Crossover Cues. A cue flashes a notefield column shortly before an
// upcoming crossover step.
pub const CROSSOVER_CUE_DURATION_MIN_MS: u16 = 500;
pub const CROSSOVER_CUE_DURATION_MAX_MS: u16 = 1500;
pub const CROSSOVER_CUE_DURATION_STEP_MS: u16 = 100;
pub const CROSSOVER_CUE_DURATION_DEFAULT_MS: u16 = 500;
/// Discrete quantization values for the crossover-cue spacing threshold
/// (`4 / Quantization`). Higher values cue tighter bursts.
pub const CROSSOVER_CUE_QUANTIZATIONS: [u8; 8] = [4, 8, 12, 16, 24, 32, 48, 64];
pub const CROSSOVER_CUE_QUANTIZATION_DEFAULT: u8 = 8;

/// Clamp a crossover-cue duration to `[500, 1500]` ms, snapped to the 100 ms grid.
#[inline]
pub const fn clamp_crossover_cue_duration_ms(ms: u16) -> u16 {
    let clamped = if ms < CROSSOVER_CUE_DURATION_MIN_MS {
        CROSSOVER_CUE_DURATION_MIN_MS
    } else if ms > CROSSOVER_CUE_DURATION_MAX_MS {
        CROSSOVER_CUE_DURATION_MAX_MS
    } else {
        ms
    };
    let steps = (clamped - CROSSOVER_CUE_DURATION_MIN_MS + CROSSOVER_CUE_DURATION_STEP_MS / 2)
        / CROSSOVER_CUE_DURATION_STEP_MS;
    CROSSOVER_CUE_DURATION_MIN_MS + steps * CROSSOVER_CUE_DURATION_STEP_MS
}

/// Snap an arbitrary quantization value to the nearest supported crossover-cue
/// quantization (falling back to the default for non-positive input).
#[inline]
pub fn clamp_crossover_cue_quantization(q: u8) -> u8 {
    if q == 0 {
        return CROSSOVER_CUE_QUANTIZATION_DEFAULT;
    }
    let mut best = CROSSOVER_CUE_QUANTIZATIONS[0];
    let mut best_diff = u8::MAX;
    for &candidate in &CROSSOVER_CUE_QUANTIZATIONS {
        let diff = candidate.abs_diff(q);
        if diff < best_diff {
            best_diff = diff;
            best = candidate;
        }
    }
    best
}

/// Fallback pad-light brightness (0..=100) when a profile has no saved value.
/// New profiles are seeded from the StepManiaX machine default instead (see
/// `game::profile`); this is only the in-crate default for a fresh struct.
pub const PAD_LIGHT_BRIGHTNESS_DEFAULT: u8 = 100;

/// Clamp a pad-light brightness to the valid 0..=100 percent range.
#[inline(always)]
pub const fn clamp_pad_light_brightness(percent: u8) -> u8 {
    if percent > 100 { 100 } else { percent }
}
pub const TEXT_ERROR_BAR_THRESHOLD_MS_MIN: u32 = 1;
pub const TEXT_ERROR_BAR_THRESHOLD_MS_MAX: u32 = 50;
pub const TEXT_ERROR_BAR_THRESHOLD_MS_DEFAULT: u32 = 10;
pub const TAP_EXPLOSION_MASK_VERSION: u8 = 2;
pub const DEFAULT_COLUMN_FLASH_MASK: ColumnFlashMask = ColumnFlashMask::MISS;

#[inline(always)]
pub const fn clamp_weight_pounds(weight_pounds: i32) -> i32 {
    if weight_pounds == 0 {
        0
    } else if weight_pounds < 20 {
        20
    } else if weight_pounds > 1000 {
        1000
    } else {
        weight_pounds
    }
}

#[inline(always)]
pub const fn resolved_weight_pounds(weight_pounds: i32) -> i32 {
    if weight_pounds == 0 {
        DEFAULT_WEIGHT_POUNDS
    } else {
        weight_pounds
    }
}

#[inline(always)]
pub const fn age_years_for_birth_year(birth_year: i32, current_year: i32) -> i32 {
    if birth_year == 0 {
        current_year - DEFAULT_BIRTH_YEAR
    } else {
        current_year - birth_year
    }
}

#[inline]
fn set_i32_if_changed(value: &mut i32, new_value: i32) -> bool {
    if *value == new_value {
        return false;
    }
    *value = new_value;
    true
}

#[inline]
fn set_f32_if_changed(value: &mut f32, new_value: f32) -> bool {
    if (*value - new_value).abs() < 1e-6 {
        return false;
    }
    *value = new_value;
    true
}

#[inline]
fn set_u32_if_changed(value: &mut u32, new_value: u32) -> bool {
    if *value == new_value {
        return false;
    }
    *value = new_value;
    true
}

#[inline]
fn set_u8_if_changed(value: &mut u8, new_value: u8) -> bool {
    if *value == new_value {
        return false;
    }
    *value = new_value;
    true
}

#[inline]
fn set_value_if_changed<T: PartialEq>(value: &mut T, new_value: T) -> bool {
    if *value == new_value {
        return false;
    }
    *value = new_value;
    true
}

#[inline(always)]
pub fn tap_explosion_mask_for_window(window: &str) -> Option<TapExplosionMask> {
    match window {
        "W0" | "W1" => Some(TapExplosionMask::FANTASTIC),
        "W2" => Some(TapExplosionMask::EXCELLENT),
        "W3" => Some(TapExplosionMask::GREAT),
        "W4" => Some(TapExplosionMask::DECENT),
        "W5" => Some(TapExplosionMask::WAY_OFF),
        "Miss" => Some(TapExplosionMask::MISS),
        "Held" => Some(TapExplosionMask::HELD),
        _ => None,
    }
}

#[inline(always)]
pub fn tap_explosion_mask_enabled(mask: TapExplosionMask, window: &str) -> bool {
    let Some(flag) = tap_explosion_mask_for_window(window) else {
        return false;
    };
    mask.contains(flag)
}

#[inline(always)]
pub fn normalize_tap_explosion_mask(bits: u8, version: u8) -> TapExplosionMask {
    let mut mask = TapExplosionMask::from_bits_truncate(bits);
    if version < TAP_EXPLOSION_MASK_VERSION {
        mask.insert(TapExplosionMask::MISS | TapExplosionMask::HOLDING);
    }
    mask
}

#[inline(always)]
pub const fn column_flash_mask_for_grade(
    grade: JudgeGrade,
    blue_fantastic: bool,
) -> ColumnFlashMask {
    match grade {
        JudgeGrade::Fantastic => {
            if blue_fantastic {
                ColumnFlashMask::BLUE_FANTASTIC
            } else {
                ColumnFlashMask::WHITE_FANTASTIC
            }
        }
        JudgeGrade::Excellent => ColumnFlashMask::EXCELLENT,
        JudgeGrade::Great => ColumnFlashMask::GREAT,
        JudgeGrade::Decent => ColumnFlashMask::DECENT,
        JudgeGrade::WayOff => ColumnFlashMask::WAY_OFF,
        JudgeGrade::Miss => ColumnFlashMask::MISS,
    }
}

#[inline(always)]
pub const fn column_flash_mask_enabled(
    mask: ColumnFlashMask,
    grade: JudgeGrade,
    blue_fantastic: bool,
) -> bool {
    mask.contains(column_flash_mask_for_grade(grade, blue_fantastic))
}

#[inline(always)]
pub const fn clamp_tilt_threshold_ms(ms: u32) -> u32 {
    if ms > TILT_THRESHOLD_MAX_MS {
        TILT_THRESHOLD_MAX_MS
    } else {
        ms
    }
}

#[inline]
pub const fn clamp_long_error_bar_threshold_ms(ms: u32) -> u32 {
    if ms < LONG_ERROR_BAR_THRESHOLD_MS_MIN {
        LONG_ERROR_BAR_THRESHOLD_MS_MIN
    } else if ms > LONG_ERROR_BAR_THRESHOLD_MS_MAX {
        LONG_ERROR_BAR_THRESHOLD_MS_MAX
    } else {
        ms
    }
}

#[inline]
pub const fn clamp_text_error_bar_threshold_ms(ms: u32) -> u32 {
    if ms < TEXT_ERROR_BAR_THRESHOLD_MS_MIN {
        TEXT_ERROR_BAR_THRESHOLD_MS_MIN
    } else if ms > TEXT_ERROR_BAR_THRESHOLD_MS_MAX {
        TEXT_ERROR_BAR_THRESHOLD_MS_MAX
    } else {
        ms
    }
}

#[inline]
pub const fn clamp_long_error_bar_min_samples(n: u32) -> u32 {
    if n < LONG_ERROR_BAR_MIN_SAMPLES_MIN {
        LONG_ERROR_BAR_MIN_SAMPLES_MIN
    } else if n > LONG_ERROR_BAR_MIN_SAMPLES_MAX {
        LONG_ERROR_BAR_MIN_SAMPLES_MAX
    } else {
        n
    }
}

#[inline]
pub fn clamp_long_error_bar_intensity(value: f32) -> f32 {
    if !value.is_finite() {
        return LONG_ERROR_BAR_INTENSITY_DEFAULT;
    }
    let clamped = value.clamp(LONG_ERROR_BAR_INTENSITY_MIN, LONG_ERROR_BAR_INTENSITY_MAX);
    let steps = ((clamped - LONG_ERROR_BAR_INTENSITY_MIN) / LONG_ERROR_BAR_INTENSITY_STEP).round();
    (LONG_ERROR_BAR_INTENSITY_MIN + steps * LONG_ERROR_BAR_INTENSITY_STEP)
        .clamp(LONG_ERROR_BAR_INTENSITY_MIN, LONG_ERROR_BAR_INTENSITY_MAX)
}

#[inline]
pub fn clamp_average_error_bar_intensity(value: f32) -> f32 {
    if !value.is_finite() {
        return AVERAGE_ERROR_BAR_INTENSITY_DEFAULT;
    }
    let clamped = value.clamp(
        AVERAGE_ERROR_BAR_INTENSITY_MIN,
        AVERAGE_ERROR_BAR_INTENSITY_MAX,
    );
    let steps =
        ((clamped - AVERAGE_ERROR_BAR_INTENSITY_MIN) / AVERAGE_ERROR_BAR_INTENSITY_STEP).round();
    (AVERAGE_ERROR_BAR_INTENSITY_MIN + steps * AVERAGE_ERROR_BAR_INTENSITY_STEP).clamp(
        AVERAGE_ERROR_BAR_INTENSITY_MIN,
        AVERAGE_ERROR_BAR_INTENSITY_MAX,
    )
}

#[inline]
pub const fn clamp_average_error_bar_interval_ms(ms: u32) -> u32 {
    let clamped = if ms < AVERAGE_ERROR_BAR_INTERVAL_MS_MIN {
        AVERAGE_ERROR_BAR_INTERVAL_MS_MIN
    } else if ms > AVERAGE_ERROR_BAR_INTERVAL_MS_MAX {
        AVERAGE_ERROR_BAR_INTERVAL_MS_MAX
    } else {
        ms
    };
    let steps = (clamped - AVERAGE_ERROR_BAR_INTERVAL_MS_MIN
        + AVERAGE_ERROR_BAR_INTERVAL_MS_STEP / 2)
        / AVERAGE_ERROR_BAR_INTERVAL_MS_STEP;
    AVERAGE_ERROR_BAR_INTERVAL_MS_MIN + steps * AVERAGE_ERROR_BAR_INTERVAL_MS_STEP
}

#[inline(always)]
pub const fn clamp_custom_fantastic_window_ms(ms: u8) -> u8 {
    if ms < CUSTOM_FANTASTIC_WINDOW_MIN_MS {
        CUSTOM_FANTASTIC_WINDOW_MIN_MS
    } else if ms > CUSTOM_FANTASTIC_WINDOW_MAX_MS {
        CUSTOM_FANTASTIC_WINDOW_MAX_MS
    } else {
        ms
    }
}

pub fn sanitize_player_initials(raw: &str) -> String {
    let mut out = String::with_capacity(PLAYER_INITIALS_MAX_LEN);
    for ch in raw.chars() {
        if out.len() >= PLAYER_INITIALS_MAX_LEN {
            break;
        }
        if ch.is_ascii_alphanumeric() || ch == '?' || ch == '!' {
            out.push(ch.to_ascii_uppercase());
        }
    }
    out
}

pub fn initials_from_name(name: &str) -> String {
    let mut out = sanitize_player_initials(name);
    match out.len() {
        0 => "??".to_string(),
        1 => {
            out.push('?');
            out
        }
        _ => out,
    }
}

pub fn parse_profile_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub fn parse_groovestats_is_pad_player(value: Option<&str>, default: bool) -> bool {
    value
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(default, |v| v == 1)
}

pub fn parse_last_played_value(value: Option<&str>) -> Option<String> {
    value.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[inline(always)]
pub fn is_local_profile_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s != "." && s != ".." && !s.contains(['/', '\\', '\0'])
}

/// INI section holding profile identity and display fields.
pub const PROFILE_SECTION: &str = "userprofile";
/// INI key (under `[userprofile]`) holding the profile's canonical GUID.
pub const PROFILE_GUID_KEY: &str = "Guid";
/// INI key (under `[userprofile]`) holding the human-readable display name.
pub const PROFILE_DISPLAY_NAME_KEY: &str = "DisplayName";
/// INI key (under `[userprofile]`) holding score display initials.
pub const PROFILE_INITIALS_KEY: &str = "PlayerInitials";
pub const DEFAULT_SCORE_INITIALS: &str = "----";

/// Stable per-profile identity so the on-disk folder can be freely renamed
/// without losing scores, settings, or online logins.
pub fn generate_profile_guid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Append the canonical `Guid` line to in-progress `[userprofile]` content, so
/// every writer stays consistent with `PROFILE_GUID_KEY` and spacing.
pub fn push_profile_guid_line(out: &mut String, guid: &str) {
    out.push_str(PROFILE_GUID_KEY);
    out.push('=');
    out.push_str(guid);
    out.push('\n');
}

/// Recognise an already-assigned identity; `generate_profile_guid` always emits
/// this canonical dashed-lowercase form.
pub fn is_valid_profile_guid(s: &str) -> bool {
    let groups = [8usize, 4, 4, 4, 12];
    let mut parts = s.split('-');
    for &len in &groups {
        let Some(part) = parts.next() else {
            return false;
        };
        if part.len() != len || !part.bytes().all(|b| b.is_ascii_hexdigit()) {
            return false;
        }
    }
    parts.next().is_none()
}

/// Fixed namespace UUID for deriving DeadSync profile GUIDs from ITGmania
/// profile GUIDs. Chosen once and never changed so the mapping stays stable.
const ITGMANIA_GUID_NAMESPACE: uuid::Uuid =
    uuid::Uuid::from_u128(0x9d3f7c12_4b8e_5a96_b2d1_e7f4a06c83ddu128);

/// Derives a stable DeadSync profile GUID from an ITGmania profile `Guid`
/// (the 16-hex string stored in `Stats.xml` `GeneralData/Guid`).
///
/// ITGmania GUIDs aren't UUIDs, so they can't be used as DeadSync identities
/// directly. We map them through a fixed namespace with UUID v5, which is
/// deterministic: the same ITGmania GUID always yields the same DeadSync GUID
/// (so re-importing the same profile produces a matching identity), and the
/// result is a canonical UUID that satisfies [`is_valid_profile_guid`].
///
/// Returns `None` when `itg_guid` is blank.
pub fn profile_guid_from_itgmania_guid(itg_guid: &str) -> Option<String> {
    let trimmed = itg_guid.trim();
    if trimmed.is_empty() {
        return None;
    }
    let derived = uuid::Uuid::new_v5(
        &ITGMANIA_GUID_NAMESPACE,
        trimmed.to_ascii_lowercase().as_bytes(),
    );
    Some(derived.to_string())
}

const FOLDER_NAME_MAX_LEN: usize = 48;

fn is_windows_reserved_name(name: &str) -> bool {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    RESERVED.iter().any(|r| name.eq_ignore_ascii_case(r))
}

/// Derive a safe single-segment folder name from a display name. `None` (caller
/// falls back to the GUID) when nothing usable survives sanitizing or the result
/// would hit a Windows reserved device name.
pub fn sanitize_folder_base(display_name: &str) -> Option<String> {
    let mut out = String::with_capacity(display_name.len());
    let mut last_was_space = false;
    for ch in display_name.chars() {
        let mapped = match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => None,
            c if c.is_control() => None,
            c if c.is_whitespace() => Some(' '),
            c => Some(c),
        };
        let Some(c) = mapped else {
            continue;
        };
        if c == ' ' {
            if last_was_space || out.is_empty() {
                continue;
            }
            last_was_space = true;
        } else {
            last_was_space = false;
        }
        out.push(c);
        if out.len() >= FOLDER_NAME_MAX_LEN {
            break;
        }
    }

    let trimmed = out.trim_matches(|c: char| c == ' ' || c == '.');
    if trimmed.is_empty() || is_windows_reserved_name(trimmed) {
        return None;
    }
    Some(trimmed.to_string())
}

/// Largest numeric suffix tried before giving up and using the fallback.
const FOLDER_NAME_MAX_SUFFIX: u32 = 9999;

/// Collision-free folder name derived from `display_name`. Returns `fallback`
/// (which the caller guarantees is collision-free, e.g. the profile GUID or its
/// current folder) when the display name yields nothing usable or every suffix
/// up to `FOLDER_NAME_MAX_SUFFIX` is taken. Matching against `existing` is
/// case-insensitive.
pub fn folder_name_for_display(display_name: &str, fallback: &str, existing: &[String]) -> String {
    let taken = |candidate: &str| existing.iter().any(|e| e.eq_ignore_ascii_case(candidate));

    let Some(base) = sanitize_folder_base(display_name) else {
        return fallback.to_string();
    };

    if !taken(&base) {
        return base;
    }
    let mut candidate = String::with_capacity(base.len() + 5);
    for n in 2..=FOLDER_NAME_MAX_SUFFIX {
        candidate.clear();
        let _ = write!(candidate, "{base}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    fallback.to_string()
}

#[inline(always)]
pub fn cmp_profile_ids_case_insensitive(a: &str, b: &str) -> core::cmp::Ordering {
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
        .then_with(|| a.cmp(b))
}

pub fn rewrite_profile_display_name_content(src: &str, display_name: &str) -> String {
    let mut out = String::with_capacity(src.len() + display_name.len() + 32);
    let mut in_userprofile = false;
    let mut saw_userprofile = false;
    let mut wrote_display = false;

    for raw_line in src.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_userprofile && !wrote_display {
                out.push_str("DisplayName=");
                out.push_str(display_name);
                out.push('\n');
                wrote_display = true;
            }
            let section = trimmed[1..trimmed.len() - 1].trim();
            in_userprofile = section.eq_ignore_ascii_case("userprofile");
            if in_userprofile {
                saw_userprofile = true;
            }
            out.push_str(raw_line);
            out.push('\n');
            continue;
        }

        if in_userprofile && let Some(eq) = trimmed.find('=') {
            let key = trimmed[..eq].trim();
            if key.eq_ignore_ascii_case("DisplayName") {
                out.push_str("DisplayName=");
                out.push_str(display_name);
                out.push('\n');
                wrote_display = true;
                continue;
            }
        }

        out.push_str(raw_line);
        out.push('\n');
    }

    if !saw_userprofile {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("[userprofile]\n");
        out.push_str("DisplayName=");
        out.push_str(display_name);
        out.push('\n');
    } else if in_userprofile && !wrote_display {
        out.push_str("DisplayName=");
        out.push_str(display_name);
        out.push('\n');
    }

    out
}

/// Backfill identity into a legacy `profile.ini`: ensure a single canonical
/// `Guid` under the first `[userprofile]`, replacing any stale value and
/// creating the section when missing. Extra `[userprofile]` sections (if any)
/// are left untouched.
pub fn upsert_profile_guid_content(src: &str, guid: &str) -> String {
    let mut out = String::with_capacity(src.len() + guid.len() + 32);
    let mut in_userprofile = false;
    let mut wrote_guid = false;

    for raw_line in src.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            // Section had no Guid: keep it inside before moving on.
            if in_userprofile && !wrote_guid {
                push_profile_guid_line(&mut out, guid);
                wrote_guid = true;
            }
            let section = trimmed[1..trimmed.len() - 1].trim();
            in_userprofile = !wrote_guid && section.eq_ignore_ascii_case(PROFILE_SECTION);
            out.push_str(raw_line);
            out.push('\n');
            if in_userprofile {
                push_profile_guid_line(&mut out, guid);
                wrote_guid = true;
            }
            continue;
        }

        // Stale Guid in the canonical section: superseded by the one above.
        if in_userprofile
            && let Some(eq) = trimmed.find('=')
            && trimmed[..eq].trim().eq_ignore_ascii_case(PROFILE_GUID_KEY)
        {
            continue;
        }

        out.push_str(raw_line);
        out.push('\n');
    }

    if !wrote_guid {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("[userprofile]\n");
        push_profile_guid_line(&mut out, guid);
    }

    out
}

/// Scan `profile.ini` content for the `[userprofile]` `Guid` and `DisplayName`
/// in a single pass without a full INI parse. Section/key matching is
/// case-insensitive; the GUID is validated and lowercased so identity keys never
/// diverge by case. Only the first `[userprofile]` section is consulted.
pub fn read_userprofile_identity(content: &str) -> (Option<String>, Option<String>) {
    let mut in_section = false;
    let mut seen_section = false;
    let mut guid = None;
    let mut display = None;

    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            if seen_section {
                break; // left the first [userprofile]; nothing more to read
            }
            in_section = t[1..t.len() - 1]
                .trim()
                .eq_ignore_ascii_case(PROFILE_SECTION);
            seen_section = in_section;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some(eq) = t.find('=') else {
            continue;
        };
        let key = t[..eq].trim();
        let val = t[eq + 1..].trim();
        if guid.is_none() && key.eq_ignore_ascii_case(PROFILE_GUID_KEY) {
            if is_valid_profile_guid(val) {
                guid = Some(val.to_ascii_lowercase());
            }
        } else if display.is_none()
            && key.eq_ignore_ascii_case(PROFILE_DISPLAY_NAME_KEY)
            && !val.is_empty()
        {
            display = Some(val.to_string());
        }
        if guid.is_some() && display.is_some() {
            break;
        }
    }

    (guid, display)
}

pub fn read_userprofile_initials_content(content: &str) -> Option<String> {
    let mut in_section = false;
    let mut seen_section = false;

    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            if seen_section {
                break;
            }
            in_section = t[1..t.len() - 1]
                .trim()
                .eq_ignore_ascii_case(PROFILE_SECTION);
            seen_section = in_section;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some(eq) = t.find('=') else {
            continue;
        };
        let key = t[..eq].trim();
        if !key.eq_ignore_ascii_case(PROFILE_INITIALS_KEY) {
            continue;
        }
        let initials = sanitize_player_initials(t[eq + 1..].trim());
        return (!initials.is_empty()).then_some(initials);
    }

    None
}

pub fn read_player_initials_dir(dir: &Path) -> Option<String> {
    fs::read_to_string(profile_ini_path(dir))
        .ok()
        .and_then(|content| read_userprofile_initials_content(&content))
}

pub fn local_score_profile_source(
    display_name: &str,
    dir: PathBuf,
) -> deadsync_score::LocalScoreProfileSource {
    let initials =
        read_player_initials_dir(&dir).unwrap_or_else(|| DEFAULT_SCORE_INITIALS.to_string());
    deadsync_score::LocalScoreProfileSource {
        root: dir.join("scores").join("local"),
        initials,
        display_name: display_name.to_string(),
    }
}

pub fn local_score_profile_sources_from_summaries(
    profiles: impl IntoIterator<Item = LocalProfileSummary>,
    mut resolve_dir: impl FnMut(&str) -> PathBuf,
) -> Vec<deadsync_score::LocalScoreProfileSource> {
    profiles
        .into_iter()
        .map(|profile| local_score_profile_source(&profile.display_name, resolve_dir(&profile.id)))
        .collect()
}

pub fn runtime_local_score_profile_source(
    root: &Path,
    profile_id: &str,
    display_name: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> deadsync_score::LocalScoreProfileSource {
    local_score_profile_source(
        display_name,
        runtime_profile_dir_for_id(root, profile_id, duplicate),
    )
}

pub fn runtime_local_score_profile_sources(
    root: &Path,
    mut duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Vec<deadsync_score::LocalScoreProfileSource> {
    local_score_profile_sources_from_summaries(scan_local_profile_summaries(root), |profile_id| {
        runtime_profile_dir_for_id(root, profile_id, &mut duplicate)
    })
}

pub fn scorebox_profile_snapshot(
    profile: &Profile,
    side_joined: bool,
    enable_groovestats: bool,
    enable_arrowcloud: bool,
    auto_populate_gs_scores: bool,
    persistent_profile_id: Option<String>,
) -> deadsync_score::GameplayScoreboxProfileSnapshot {
    deadsync_score::scorebox_snapshot(
        profile.display_scorebox,
        profile.show_ex_score,
        side_joined,
        enable_groovestats,
        enable_arrowcloud,
        auto_populate_gs_scores,
        profile.groovestats_api_key.as_str(),
        profile.arrowcloud_api_key.as_str(),
        profile.groovestats_username.as_str(),
        persistent_profile_id,
    )
}

#[inline(always)]
pub fn groovestats_side_active(
    enable_groovestats: bool,
    side_joined: bool,
    groovestats_api_key: &str,
) -> bool {
    enable_groovestats && side_joined && !groovestats_api_key.trim().is_empty()
}

pub fn scorebox_profile_snapshot_for_side(
    profiles: &[Profile; PLAYER_SLOTS],
    active_profiles: &[ActiveProfile; PLAYER_SLOTS],
    joined_mask: u8,
    side: PlayerSide,
    enable_groovestats: bool,
    enable_arrowcloud: bool,
    auto_populate_gs_scores: bool,
) -> deadsync_score::GameplayScoreboxProfileSnapshot {
    let side_idx = player_side_index(side);
    scorebox_profile_snapshot(
        &profiles[side_idx],
        player_side_is_joined(joined_mask, side),
        enable_groovestats,
        enable_arrowcloud,
        auto_populate_gs_scores,
        active_profile_local_id(&active_profiles[side_idx]).map(str::to_string),
    )
}

pub fn runtime_scorebox_profile_snapshot_for_side(
    side: PlayerSide,
    enable_groovestats: bool,
    enable_arrowcloud: bool,
    auto_populate_gs_scores: bool,
) -> deadsync_score::GameplayScoreboxProfileSnapshot {
    let (active_profiles, joined_mask) = {
        let session = runtime_lock_session();
        (session.active_profiles.clone(), session.joined_mask)
    };
    scorebox_profile_snapshot_for_side(
        &runtime_lock_profiles(),
        &active_profiles,
        joined_mask,
        side,
        enable_groovestats,
        enable_arrowcloud,
        auto_populate_gs_scores,
    )
}

#[derive(Debug, Clone)]
pub struct ScoreboxProfileView {
    pub leaderboard: deadsync_score::GameplayScoreboxProfileSnapshot,
    pub joined: bool,
    pub display_name: String,
    pub groovestats_username: String,
    pub player_initials: String,
}

#[derive(Debug, Clone)]
pub struct ScoreboxRuntimeView {
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub sides: [ScoreboxProfileView; PLAYER_SLOTS],
}

pub fn scorebox_runtime_view(
    profiles: &[Profile; PLAYER_SLOTS],
    active_profiles: &[ActiveProfile; PLAYER_SLOTS],
    joined_mask: u8,
    play_style: PlayStyle,
    player_side: PlayerSide,
    enable_groovestats: bool,
    enable_arrowcloud: bool,
    auto_populate_gs_scores: bool,
) -> ScoreboxRuntimeView {
    ScoreboxRuntimeView {
        play_style,
        player_side,
        sides: std::array::from_fn(|side_idx| {
            let side = [PlayerSide::P1, PlayerSide::P2][side_idx];
            let profile = &profiles[side_idx];
            let joined = player_side_is_joined(joined_mask, side);
            ScoreboxProfileView {
                leaderboard: scorebox_profile_snapshot(
                    profile,
                    joined,
                    enable_groovestats,
                    enable_arrowcloud,
                    auto_populate_gs_scores,
                    active_profile_local_id(&active_profiles[side_idx]).map(str::to_string),
                ),
                joined,
                display_name: profile.display_name.clone(),
                groovestats_username: profile.groovestats_username.clone(),
                player_initials: profile.player_initials.clone(),
            }
        }),
    }
}

pub fn runtime_scorebox_view(
    enable_groovestats: bool,
    enable_arrowcloud: bool,
    auto_populate_gs_scores: bool,
) -> ScoreboxRuntimeView {
    let (play_style, player_side, joined_mask, active_profiles) = {
        let session = runtime_lock_session();
        (
            session.play_style,
            session.player_side,
            session.joined_mask,
            session.active_profiles.clone(),
        )
    };
    scorebox_runtime_view(
        &runtime_lock_profiles(),
        &active_profiles,
        joined_mask,
        play_style,
        player_side,
        enable_groovestats,
        enable_arrowcloud,
        auto_populate_gs_scores,
    )
}

pub fn gameplay_hud_snapshot_from_parts(
    play_style: PlayStyle,
    player_side: PlayerSide,
    joined_mask: u8,
    active_profiles: &[ActiveProfile; PLAYER_SLOTS],
    profiles: &[Profile; PLAYER_SLOTS],
) -> GameplayHudSnapshot {
    GameplayHudSnapshot {
        play_style,
        player_side,
        p1: gameplay_hud_player_snapshot(
            joined_mask,
            PlayerSide::P1,
            &active_profiles[player_side_index(PlayerSide::P1)],
            &profiles[player_side_index(PlayerSide::P1)],
        ),
        p2: gameplay_hud_player_snapshot(
            joined_mask,
            PlayerSide::P2,
            &active_profiles[player_side_index(PlayerSide::P2)],
            &profiles[player_side_index(PlayerSide::P2)],
        ),
    }
}

fn gameplay_hud_player_snapshot(
    joined_mask: u8,
    side: PlayerSide,
    active_profile: &ActiveProfile,
    profile: &Profile,
) -> GameplayHudPlayerSnapshot {
    GameplayHudPlayerSnapshot {
        joined: player_side_is_joined(joined_mask, side),
        guest: active_profile_is_guest(active_profile),
        display_name: profile.display_name.clone(),
        avatar_texture_key: profile.avatar_texture_key.clone(),
        hide_username: profile.hide_username,
    }
}

pub fn side_for_physical_pad(
    play_style: PlayStyle,
    player_side: PlayerSide,
    is_p2_side: bool,
) -> PlayerSide {
    if matches!(play_style, PlayStyle::Double) {
        player_side
    } else if is_p2_side {
        PlayerSide::P2
    } else {
        PlayerSide::P1
    }
}

pub fn side_for_gameplay_player(
    num_players: usize,
    player_idx: usize,
    session_side: PlayerSide,
) -> PlayerSide {
    if num_players >= 2 {
        player_side_for_index(player_idx)
    } else {
        session_side
    }
}

pub fn default_profile_ids_after_side_update(
    defaults: [Option<String>; PLAYER_SLOTS],
    side: PlayerSide,
    profile: &ActiveProfile,
    is_valid_local_id: impl FnOnce(&str) -> bool,
) -> [Option<String>; PLAYER_SLOTS] {
    let mut defaults = defaults;
    let side_idx = player_side_index(side);
    let new_id = active_profile_local_id(profile)
        .filter(|id| is_valid_local_id(id))
        .map(str::to_owned);
    set_default_profile_id(&mut defaults, side_idx, new_id);
    defaults
}

pub fn default_profile_ids_after_joined_selection(
    defaults: [Option<String>; PLAYER_SLOTS],
    joined_mask: u8,
    p1: &ActiveProfile,
    p2: &ActiveProfile,
) -> [Option<String>; PLAYER_SLOTS] {
    let mut defaults = defaults;
    for (side, profile) in [(PlayerSide::P1, p1), (PlayerSide::P2, p2)] {
        if !player_side_is_joined(joined_mask, side) {
            continue;
        }
        let side_idx = player_side_index(side);
        set_default_profile_id(
            &mut defaults,
            side_idx,
            active_profile_local_id(profile).map(str::to_owned),
        );
    }
    defaults
}

pub fn default_profile_ids_after_profile_delete(
    defaults: [Option<String>; PLAYER_SLOTS],
    profile_id: &str,
) -> [Option<String>; PLAYER_SLOTS] {
    defaults.map(|id| id.filter(|id| id != profile_id))
}

pub fn default_profile_ids_after_profile_create(
    mut defaults: [Option<String>; PLAYER_SLOTS],
    profile_id: String,
) -> [Option<String>; PLAYER_SLOTS] {
    if defaults[player_side_index(PlayerSide::P1)].is_none() {
        defaults[player_side_index(PlayerSide::P1)] = Some(profile_id);
    } else if defaults[player_side_index(PlayerSide::P2)].is_none() {
        defaults[player_side_index(PlayerSide::P2)] = Some(profile_id);
    }
    defaults
}

pub fn rename_loaded_local_profile(
    active_profiles: &[ActiveProfile; PLAYER_SLOTS],
    profiles: &mut [Profile; PLAYER_SLOTS],
    profile_id: &str,
    display_name: &str,
) -> [bool; PLAYER_SLOTS] {
    let mut changed = [false; PLAYER_SLOTS];
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let side_idx = player_side_index(side);
        if active_profile_local_id(&active_profiles[side_idx]) != Some(profile_id) {
            continue;
        }
        changed[side_idx] = set_value_if_changed(
            &mut profiles[side_idx].display_name,
            display_name.to_string(),
        );
    }
    changed
}

fn set_default_profile_id(
    defaults: &mut [Option<String>; PLAYER_SLOTS],
    side_idx: usize,
    new_id: Option<String>,
) {
    if let Some(id) = new_id.as_deref() {
        for (idx, slot) in defaults.iter_mut().enumerate() {
            if idx != side_idx && slot.as_deref() == Some(id) {
                *slot = None;
            }
        }
    }
    defaults[side_idx] = new_id;
}

pub fn known_pack_names_for_loaded_profile(
    active_profiles: &[ActiveProfile; PLAYER_SLOTS],
    profiles: &[Profile; PLAYER_SLOTS],
    profile_id: &str,
) -> Option<HashSet<String>> {
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let side_idx = player_side_index(side);
        let Some(id) = active_profile_local_id(&active_profiles[side_idx]) else {
            continue;
        };
        if id == profile_id {
            return Some(profiles[side_idx].known_pack_names.clone());
        }
    }
    None
}

pub fn mark_known_pack_names_for_loaded_profile(
    active_profiles: &[ActiveProfile; PLAYER_SLOTS],
    profiles: &mut [Profile; PLAYER_SLOTS],
    profile_id: &str,
    pack_names: &[&str],
) -> Option<PlayerSide> {
    let mut save_side = None;
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let side_idx = player_side_index(side);
        let Some(id) = active_profile_local_id(&active_profiles[side_idx]) else {
            continue;
        };
        if id != profile_id {
            continue;
        }
        let changed = add_known_pack_names(
            &mut profiles[side_idx].known_pack_names,
            pack_names.iter().copied(),
        );
        if changed && save_side.is_none() {
            save_side = Some(side);
        }
    }
    save_side
}

pub fn set_arrowcloud_api_key_for_side(
    profiles: &mut [Profile; PLAYER_SLOTS],
    side: PlayerSide,
    api_key: &str,
) {
    profiles[player_side_index(side)].arrowcloud_api_key = api_key.to_string();
}

pub fn footer_fields_for_side(
    profiles: &[Profile; PLAYER_SLOTS],
    side: PlayerSide,
) -> (Option<String>, String) {
    let profile = &profiles[player_side_index(side)];
    (
        profile.avatar_texture_key.clone(),
        profile.display_name.clone(),
    )
}

pub fn groovestats_api_key_for_side(
    profiles: &[Profile; PLAYER_SLOTS],
    side: PlayerSide,
) -> String {
    profiles[player_side_index(side)]
        .groovestats_api_key
        .trim()
        .to_string()
}

pub fn set_avatar_texture_key_for_side(
    profiles: &mut [Profile; PLAYER_SLOTS],
    side: PlayerSide,
    key: Option<String>,
) {
    profiles[player_side_index(side)].avatar_texture_key = key;
}

pub fn update_guest_profile_noteskins(
    active_profiles: &[ActiveProfile; PLAYER_SLOTS],
    profiles: &mut [Profile; PLAYER_SLOTS],
    noteskin: NoteSkin,
) -> [bool; PLAYER_SLOTS] {
    let mut changed = [false; PLAYER_SLOTS];
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let side_idx = player_side_index(side);
        if !active_profile_is_guest(&active_profiles[side_idx]) {
            continue;
        }
        let profile = &mut profiles[side_idx];
        let mut side_changed = set_value_if_changed(&mut profile.noteskin, noteskin.clone());
        side_changed |= set_value_if_changed(
            &mut profile.player_options_singles.noteskin,
            noteskin.clone(),
        );
        side_changed |= set_value_if_changed(
            &mut profile.player_options_doubles.noteskin,
            noteskin.clone(),
        );
        changed[side_idx] = side_changed;
    }
    changed
}

pub fn pad_light_brightness_for_physical_pad(
    profiles: &[Profile; PLAYER_SLOTS],
    play_style: PlayStyle,
    player_side: PlayerSide,
    is_p2_side: bool,
) -> u8 {
    let side = side_for_physical_pad(play_style, player_side, is_p2_side);
    profiles[player_side_index(side)].pad_light_brightness
}

pub fn smx_pack_names_for_profiles<T: Copy>(
    profiles: &[Profile; PLAYER_SLOTS],
    machine_bg: T,
    machine_judge: T,
    mut parse: impl FnMut(&str) -> T,
) -> ([T; PLAYER_SLOTS], [T; PLAYER_SLOTS]) {
    fn resolve<T: Copy>(
        value: &Option<String>,
        machine: T,
        parse: &mut impl FnMut(&str) -> T,
    ) -> T {
        match value {
            Some(value) => parse(value),
            None => machine,
        }
    }

    (
        std::array::from_fn(|idx| resolve(&profiles[idx].smx_bg_pack, machine_bg, &mut parse)),
        std::array::from_fn(|idx| {
            resolve(&profiles[idx].smx_judge_pack, machine_judge, &mut parse)
        }),
    )
}

pub fn set_groovestats_credentials_for_side(
    profiles: &mut [Profile; PLAYER_SLOTS],
    side: PlayerSide,
    api_key: &str,
    username: &str,
) {
    let profile = &mut profiles[player_side_index(side)];
    profile.groovestats_api_key = api_key.to_string();
    profile.groovestats_username = username.to_string();
    profile.groovestats_is_pad_player = true;
}

pub fn set_arrowcloud_api_key_for_loaded_profile(
    active_profiles: &[ActiveProfile; PLAYER_SLOTS],
    profiles: &mut [Profile; PLAYER_SLOTS],
    profile_id: &str,
    api_key: &str,
) -> bool {
    let mut changed = false;
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let side_idx = player_side_index(side);
        let Some(id) = active_profile_local_id(&active_profiles[side_idx]) else {
            continue;
        };
        if id != profile_id {
            continue;
        }
        profiles[side_idx].arrowcloud_api_key = api_key.to_string();
        changed = true;
    }
    changed
}

pub fn set_groovestats_credentials_for_loaded_profile(
    active_profiles: &[ActiveProfile; PLAYER_SLOTS],
    profiles: &mut [Profile; PLAYER_SLOTS],
    profile_id: &str,
    api_key: &str,
    username: &str,
) -> bool {
    let mut changed = false;
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let side_idx = player_side_index(side);
        let Some(id) = active_profile_local_id(&active_profiles[side_idx]) else {
            continue;
        };
        if id != profile_id {
            continue;
        }
        let profile = &mut profiles[side_idx];
        profile.groovestats_api_key = api_key.to_string();
        profile.groovestats_username = username.to_string();
        profile.groovestats_is_pad_player = true;
        changed = true;
    }
    changed
}

pub fn toggle_favorite_for_side(
    profiles: &mut [Profile; PLAYER_SLOTS],
    side: PlayerSide,
    chart_hash: &str,
) -> (bool, HashSet<String>) {
    let profile = &mut profiles[player_side_index(side)];
    let is_now_favorite = toggle_favorite_hash(&mut profile.favorites, chart_hash);
    (is_now_favorite, profile.favorites.clone())
}

pub fn seed_favorite_for_side(
    profiles: &mut [Profile; PLAYER_SLOTS],
    side: PlayerSide,
    chart_hash: &str,
) {
    profiles[player_side_index(side)]
        .favorites
        .insert(chart_hash.to_string());
}

pub fn profile_has_favorite(
    profiles: &[Profile; PLAYER_SLOTS],
    side: PlayerSide,
    chart_hash: &str,
) -> bool {
    profiles[player_side_index(side)]
        .favorites
        .contains(chart_hash)
}

#[derive(Clone, Copy, Debug)]
pub enum FavoriteMembershipQuery<'a> {
    None,
    Pack(Option<&'a str>),
    Song(&'a deadsync_chart::SongData),
}

pub fn favorite_membership<const N: usize>(
    profiles: &[Profile; PLAYER_SLOTS],
    queries: &[FavoriteMembershipQuery<'_>; N],
) -> [[bool; PLAYER_SLOTS]; N] {
    std::array::from_fn(|query_idx| {
        std::array::from_fn(|side_idx| match queries[query_idx] {
            FavoriteMembershipQuery::None | FavoriteMembershipQuery::Pack(None) => false,
            FavoriteMembershipQuery::Pack(Some(pack)) => {
                profiles[side_idx].favorited_packs.contains(pack)
            }
            FavoriteMembershipQuery::Song(song) => song
                .charts
                .iter()
                .any(|chart| profiles[side_idx].favorites.contains(&chart.short_hash)),
        })
    })
}

pub fn toggle_favorited_pack_for_side(
    profiles: &mut [Profile; PLAYER_SLOTS],
    side: PlayerSide,
    pack_name: &str,
) -> (bool, HashSet<String>) {
    let profile = &mut profiles[player_side_index(side)];
    let is_now_favorite = toggle_favorited_pack(&mut profile.favorited_packs, pack_name);
    (is_now_favorite, profile.favorited_packs.clone())
}

pub fn seed_favorited_pack_for_side(
    profiles: &mut [Profile; PLAYER_SLOTS],
    side: PlayerSide,
    pack_name: &str,
) {
    profiles[player_side_index(side)]
        .favorited_packs
        .insert(pack_name.to_string());
}

pub fn profile_has_favorited_pack(profile: &Profile, pack_name: &str) -> bool {
    profile.favorited_packs.iter().any(|p| *p == pack_name)
}

pub fn profile_side_has_favorited_pack(
    profiles: &[Profile; PLAYER_SLOTS],
    side: PlayerSide,
    pack_name: &str,
) -> bool {
    profile_has_favorited_pack(&profiles[player_side_index(side)], pack_name)
}

#[derive(Debug, Default)]
struct ProfileIni {
    sections: HashMap<String, HashMap<String, String>>,
}

impl ProfileIni {
    fn load(path: &Path) -> Result<Self, std::io::Error> {
        let content = fs::read_to_string(path)?;
        Ok(Self::parse(content.as_str()))
    }

    fn parse(content: &str) -> Self {
        let mut ini = Self::default();
        let mut current_section: Option<String> = None;

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
                let section = line[1..line.len() - 1].trim().to_string();
                current_section = Some(section.clone());
                ini.sections.entry(section).or_default();
                continue;
            }

            let Some(eq_idx) = line.find('=') else {
                continue;
            };
            let key = line[..eq_idx].trim();
            if key.is_empty() {
                continue;
            }
            let value = line[eq_idx + 1..].trim().to_string();
            let section = current_section.clone().unwrap_or_default();
            ini.sections
                .entry(section)
                .or_default()
                .insert(key.to_string(), value);
        }

        ini
    }

    fn get(&self, section: &str, key: &str) -> Option<String> {
        self.sections
            .get(section)
            .and_then(|section| section.get(key))
            .cloned()
    }

    fn section_has_any(&self, section: &str) -> bool {
        self.sections.get(section).is_some_and(|s| !s.is_empty())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn apply_loaded_profile_data(
    profile: &mut Profile,
    default_profile: &Profile,
    play_style: PlayStyle,
    today: &str,
    profile_ini_loaded: bool,
    mut profile_section_has_any: impl FnMut(&str) -> bool,
    mut profile_get: impl FnMut(&str, &str) -> Option<String>,
    stats: ProfileStats,
    favorites: HashSet<String>,
    favorited_packs: HashSet<String>,
    groovestats_ini_loaded: bool,
    mut groovestats_get: impl FnMut(&str, &str) -> Option<String>,
    arrowcloud_ini_loaded: bool,
    mut arrowcloud_get: impl FnMut(&str, &str) -> Option<String>,
) {
    if profile_ini_loaded {
        profile.display_name = profile_get("userprofile", "DisplayName")
            .unwrap_or_else(|| default_profile.display_name.clone());
        profile.player_initials = profile_get("userprofile", "PlayerInitials")
            .map(|initials| sanitize_player_initials(&initials))
            .filter(|initials| !initials.is_empty())
            .unwrap_or_else(|| default_profile.player_initials.clone());

        let mut load_player_options = |section: &str, default: &PlayerOptionsData| {
            load_player_options_section(
                profile_section_has_any(section),
                |key| profile_get(section, key),
                default,
            )
        };
        profile.player_options_singles = load_player_options(
            player_options_section(PlayStyle::Single),
            &default_profile.player_options_singles,
        )
        .unwrap_or_else(|| default_profile.player_options_singles.clone());
        profile.player_options_doubles = load_player_options(
            player_options_section(PlayStyle::Double),
            &default_profile.player_options_doubles,
        )
        .unwrap_or_else(|| default_profile.player_options_doubles.clone());
        profile.apply_player_options_for_style(play_style);

        let mut load_last_played = |section: &str, default: &LastPlayed| {
            load_last_played_section(
                profile_section_has_any(section),
                |key| profile_get(section, key),
                default,
            )
        };
        profile.last_played_singles =
            load_last_played("LastPlayedSingles", &default_profile.last_played_singles)
                .or_else(|| load_last_played("LastPlayed", &default_profile.last_played_singles))
                .unwrap_or_else(|| default_profile.last_played_singles.clone());
        profile.last_played_doubles =
            load_last_played("LastPlayedDoubles", &default_profile.last_played_doubles)
                .or_else(|| load_last_played("LastPlayed", &default_profile.last_played_doubles))
                .unwrap_or_else(|| default_profile.last_played_doubles.clone());

        let mut load_last_played_course = |section: &str| {
            load_last_played_course_section(profile_section_has_any(section), |key| {
                profile_get(section, key)
            })
        };
        profile.last_played_course_singles = load_last_played_course("LastPlayedCourseSingles")
            .or_else(|| load_last_played_course("LastPlayedCourse"))
            .unwrap_or_else(|| default_profile.last_played_course_singles.clone());
        profile.last_played_course_doubles = load_last_played_course("LastPlayedCourseDoubles")
            .or_else(|| load_last_played_course("LastPlayedCourse"))
            .unwrap_or_else(|| default_profile.last_played_course_doubles.clone());

        profile.weight_pounds = profile_get("Editable", "WeightPounds")
            .and_then(|s| s.parse::<i32>().ok())
            .map(clamp_weight_pounds)
            .unwrap_or(default_profile.weight_pounds);
        profile.birth_year = profile_get("Editable", "BirthYear")
            .and_then(|s| s.parse::<i32>().ok())
            .map(|year| year.max(0))
            .unwrap_or(default_profile.birth_year);
        profile.ignore_step_count_calories = profile_get("Editable", "IgnoreStepCountCalories")
            .or_else(|| profile_get("Stats", "IgnoreStepCountCalories"))
            .and_then(|s| s.parse::<u8>().ok())
            .map_or(default_profile.ignore_step_count_calories, |v| v != 0);

        let saved_day = profile_get("Stats", "CaloriesBurnedDate").unwrap_or_default();
        let saved_cals = profile_get("Stats", "CaloriesBurnedToday")
            .and_then(|s| s.parse::<f32>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
            .unwrap_or(default_profile.calories_burned_today);
        if saved_day.trim() == today {
            profile.calories_burned_day = today.to_string();
            profile.calories_burned_today = saved_cals;
        } else {
            profile.calories_burned_day = today.to_string();
            profile.calories_burned_today = 0.0;
        }
    }

    profile.current_combo = stats.current_combo;
    profile.known_pack_names = stats.known_pack_names;
    profile.favorites = favorites;
    profile.favorited_packs = favorited_packs;

    if groovestats_ini_loaded {
        profile.groovestats_api_key = groovestats_get("GrooveStats", "ApiKey")
            .unwrap_or_else(|| default_profile.groovestats_api_key.clone());
        profile.groovestats_is_pad_player = parse_groovestats_is_pad_player(
            groovestats_get("GrooveStats", "IsPadPlayer").as_deref(),
            default_profile.groovestats_is_pad_player,
        );
        profile.groovestats_username = groovestats_get("GrooveStats", "Username")
            .unwrap_or_else(|| default_profile.groovestats_username.clone());
    }

    if arrowcloud_ini_loaded {
        profile.arrowcloud_api_key = arrowcloud_get("ArrowCloud", "ApiKey")
            .unwrap_or_else(|| default_profile.arrowcloud_api_key.clone());
    }
}

pub fn find_profile_avatar_path(dir: &Path) -> Option<PathBuf> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return None;
    };
    let mut avatar = None;
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if name.eq_ignore_ascii_case("profile.png") {
            return Some(path);
        }
        if avatar.is_none() && name.eq_ignore_ascii_case("avatar.png") {
            avatar = Some(path);
        }
    }
    avatar
}

pub const PROFILE_INI_FILE: &str = "profile.ini";
pub const GROOVESTATS_INI_FILE: &str = "groovestats.ini";
pub const ARROWCLOUD_INI_FILE: &str = "arrowcloud.ini";
pub const PROFILE_STATS_FILE: &str = "stats.bin";
pub const PROFILE_STATS_TMP_FILE: &str = "stats.bin.tmp";
pub const FAVORITES_FILE: &str = "favorites.txt";
pub const FAVORITED_PACKS_FILE: &str = "favorited_packs.txt";

#[inline(always)]
pub fn profile_ini_path(dir: &Path) -> PathBuf {
    dir.join(PROFILE_INI_FILE)
}

#[inline(always)]
pub fn groovestats_ini_path(dir: &Path) -> PathBuf {
    dir.join(GROOVESTATS_INI_FILE)
}

#[inline(always)]
pub fn arrowcloud_ini_path(dir: &Path) -> PathBuf {
    dir.join(ARROWCLOUD_INI_FILE)
}

#[inline(always)]
pub fn profile_stats_path(dir: &Path) -> PathBuf {
    dir.join(PROFILE_STATS_FILE)
}

#[inline(always)]
pub fn profile_stats_tmp_path(dir: &Path) -> PathBuf {
    dir.join(PROFILE_STATS_TMP_FILE)
}

#[inline(always)]
pub fn favorites_path(dir: &Path) -> PathBuf {
    dir.join(FAVORITES_FILE)
}

#[inline(always)]
pub fn favorited_packs_path(dir: &Path) -> PathBuf {
    dir.join(FAVORITED_PACKS_FILE)
}

/// Read the embedded `Guid` and `DisplayName` from a folder's `profile.ini`.
pub fn read_profile_identity_dir(dir: &Path) -> (Option<String>, Option<String>) {
    match fs::read_to_string(profile_ini_path(dir)) {
        Ok(content) => read_userprofile_identity(&content),
        Err(_) => (None, None),
    }
}

#[inline(always)]
pub fn read_profile_guid_dir(dir: &Path) -> Option<String> {
    read_profile_identity_dir(dir).0
}

/// Write `contents` to `path` atomically via a temp sibling and rename.
pub fn write_profile_file_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)
}

pub fn rewrite_profile_display_name_file(
    path: &Path,
    display_name: &str,
) -> Result<(), std::io::Error> {
    let src = fs::read_to_string(path)?;
    fs::write(
        path,
        rewrite_profile_display_name_content(&src, display_name),
    )
}

pub fn profile_folder_names(root: &Path) -> Vec<String> {
    let Ok(read_dir) = fs::read_dir(root) else {
        return Vec::new();
    };
    read_dir
        .flatten()
        .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect()
}

/// Build a GUID -> folder map from profile folders under `root`.
///
/// Duplicate GUIDs resolve deterministically to the lexicographically smallest
/// folder name, matching root cache behavior.
pub fn build_profile_dir_map(
    root: &Path,
    mut duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> HashMap<String, PathBuf> {
    use std::collections::hash_map::Entry;

    let mut map: HashMap<String, PathBuf> = HashMap::new();
    let Ok(read_dir) = fs::read_dir(root) else {
        return map;
    };
    for entry in read_dir.flatten() {
        if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }
        let path = entry.path();
        let Some(guid) = read_profile_guid_dir(&path) else {
            continue;
        };
        match map.entry(guid) {
            Entry::Vacant(slot) => {
                slot.insert(path);
            }
            Entry::Occupied(mut slot) => {
                let keep_existing = slot.get().file_name() <= path.file_name();
                let kept = if keep_existing { slot.get() } else { &path };
                duplicate(slot.key(), slot.get(), &path, kept);
                if !keep_existing {
                    slot.insert(path);
                }
            }
        }
    }
    map
}

/// Pure read-only local profile enumeration. Legacy folders without an
/// embedded GUID are skipped; startup migration is responsible for backfill.
pub fn scan_local_profile_summaries(root: &Path) -> Vec<LocalProfileSummary> {
    let Ok(read_dir) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in read_dir.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        let dir = entry.path();
        let (Some(id), display) = read_profile_identity_dir(&dir) else {
            continue;
        };
        out.push(LocalProfileSummary {
            display_name: display.unwrap_or_else(|| id.clone()),
            id,
            avatar_path: find_profile_avatar_path(&dir),
        });
    }

    out.sort_by(|a, b| {
        cmp_profile_ids_case_insensitive(&a.display_name, &b.display_name)
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

pub fn create_local_profile_dir(
    root: &Path,
    display_name: &str,
    noteskin: NoteSkin,
    pad_light_brightness: u8,
) -> Result<String, std::io::Error> {
    let name = display_name.trim();
    if name.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Display name is empty",
        ));
    }

    let id = generate_profile_guid();
    let folder = folder_name_for_display(name, &id, &profile_folder_names(root));
    let dir = root.join(folder);
    fs::create_dir_all(&dir)?;

    let mut profile = Profile::default();
    profile.noteskin = noteskin;
    profile.pad_light_brightness = pad_light_brightness;
    profile.store_current_player_options_for_all_styles();
    profile.display_name = name.to_string();
    profile.player_initials = initials_from_name(name);
    profile.weight_pounds = 0;
    profile.birth_year = 0;
    profile.ignore_step_count_calories = false;
    profile.calories_burned_day = Local::now().date_naive().to_string();
    profile.calories_burned_today = 0.0;

    fs::write(
        profile_ini_path(&dir),
        render_profile_ini_content(&id, &profile),
    )?;
    fs::write(
        groovestats_ini_path(&dir),
        render_groovestats_ini_content("", false, ""),
    )?;
    fs::write(arrowcloud_ini_path(&dir), render_arrowcloud_ini_content(""))?;
    Ok(id)
}

/// Everything needed to materialise a new local profile from an external source.
pub struct ImportProfileData<'a> {
    pub display_name: &'a str,
    pub weight_pounds: u32,
    pub birth_year: u32,
    pub initials: &'a str,
    pub groovestats_api_key: &'a str,
    pub groovestats_username: &'a str,
    pub groovestats_is_pad_player: bool,
    pub arrowcloud_api_key: &'a str,
    pub ignore_step_count_calories: bool,
    pub avatar_src: Option<&'a Path>,
    pub options_singles: &'a PlayerOptionsData,
    pub options_doubles: &'a PlayerOptionsData,
    pub guid: &'a str,
}

pub struct ImportedProfileCreateResult {
    pub id: String,
    pub avatar_copy_error: Option<std::io::Error>,
}

pub fn create_local_profile_from_import_dir(
    root: &Path,
    data: &ImportProfileData<'_>,
) -> Result<ImportedProfileCreateResult, std::io::Error> {
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
    let folder = folder_name_for_display(name, &id, &profile_folder_names(root));
    let dir = root.join(folder);
    fs::create_dir_all(&dir)?;

    let initials = {
        let sanitized = sanitize_player_initials(data.initials);
        if sanitized.is_empty() {
            initials_from_name(name)
        } else {
            sanitized
        }
    };
    let profile = Profile {
        display_name: name.to_string(),
        player_initials: initials,
        weight_pounds: clamp_weight_pounds(data.weight_pounds.min(i32::MAX as u32) as i32),
        birth_year: data.birth_year.min(i32::MAX as u32) as i32,
        ignore_step_count_calories: data.ignore_step_count_calories,
        calories_burned_day: Local::now().date_naive().to_string(),
        calories_burned_today: 0.0,
        player_options_singles: data.options_singles.clone(),
        player_options_doubles: data.options_doubles.clone(),
        ..Profile::default()
    };

    fs::write(
        profile_ini_path(&dir),
        render_profile_ini_content(&id, &profile),
    )?;
    fs::write(
        groovestats_ini_path(&dir),
        render_groovestats_ini_content(
            data.groovestats_api_key,
            data.groovestats_is_pad_player,
            data.groovestats_username,
        ),
    )?;
    fs::write(
        arrowcloud_ini_path(&dir),
        render_arrowcloud_ini_content(data.arrowcloud_api_key),
    )?;

    let avatar_copy_error = data
        .avatar_src
        .and_then(|src| fs::copy(src, dir.join("profile.png")).err());

    Ok(ImportedProfileCreateResult {
        id,
        avatar_copy_error,
    })
}

pub struct LocalProfileFolderRename {
    pub current_folder: String,
    pub desired_folder: String,
    pub error: Option<std::io::Error>,
}

pub struct LocalProfileRenameResult {
    pub display_name: String,
    pub folder_rename: Option<LocalProfileFolderRename>,
}

pub fn rename_local_profile_dir(
    root: &Path,
    current_dir: &Path,
    id: &str,
    display_name: &str,
) -> Result<LocalProfileRenameResult, std::io::Error> {
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

    let ini_path = profile_ini_path(current_dir);
    if !ini_path.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Profile does not exist",
        ));
    }
    rewrite_profile_display_name_file(&ini_path, name)?;

    let folder_rename = current_dir
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|current_folder| {
            let others: Vec<String> = profile_folder_names(root)
                .into_iter()
                .filter(|folder| !folder.eq_ignore_ascii_case(current_folder))
                .collect();
            let desired_folder = folder_name_for_display(name, id, &others);
            if desired_folder.eq_ignore_ascii_case(current_folder) {
                return None;
            }
            let error = fs::rename(current_dir, root.join(&desired_folder)).err();
            Some(LocalProfileFolderRename {
                current_folder: current_folder.to_string(),
                desired_folder,
                error,
            })
        });

    Ok(LocalProfileRenameResult {
        display_name: name.to_string(),
        folder_rename,
    })
}

pub fn delete_local_profile_dir(dir: &Path, id: &str) -> Result<(), std::io::Error> {
    if !is_local_profile_id(id) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid local profile id",
        ));
    }
    if !dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Profile does not exist",
        ));
    }
    fs::remove_dir_all(dir)
}

pub struct LocalProfileGuidBackfill {
    pub path: PathBuf,
    pub guid: String,
    pub error: Option<std::io::Error>,
}

pub struct LocalProfileMigrationEntry {
    pub original_folder: String,
    pub folder: String,
    pub guid: String,
}

pub struct LocalProfilesMigrationResult {
    pub entries: Vec<LocalProfileMigrationEntry>,
    pub guid_backfills: Vec<LocalProfileGuidBackfill>,
    pub folder_renames: Vec<LocalProfileFolderRename>,
    pub cache_map: HashMap<String, PathBuf>,
}

pub fn migrate_local_profile_dirs(root: &Path) -> LocalProfilesMigrationResult {
    struct Entry {
        original_folder: String,
        folder: String,
        guid: String,
        display: Option<String>,
    }

    let mut entries: Vec<Entry> = Vec::new();
    let mut taken: Vec<String> = Vec::new();
    let mut guid_backfills = Vec::new();

    if let Ok(read_dir) = fs::read_dir(root) {
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

            let ini_path = profile_ini_path(&path);
            let Ok(content) = fs::read_to_string(&ini_path) else {
                continue;
            };
            let (guid_opt, display) = read_userprofile_identity(&content);
            let guid = match guid_opt {
                Some(guid) => guid,
                None => {
                    let guid = generate_profile_guid();
                    let updated = upsert_profile_guid_content(&content, &guid);
                    let error = write_profile_file_atomic(&ini_path, &updated).err();
                    guid_backfills.push(LocalProfileGuidBackfill {
                        path: path.clone(),
                        guid: guid.clone(),
                        error,
                    });
                    if guid_backfills
                        .last()
                        .is_some_and(|backfill| backfill.error.is_some())
                    {
                        continue;
                    }
                    guid
                }
            };
            entries.push(Entry {
                original_folder: folder.clone(),
                folder,
                guid,
                display,
            });
        }
    }

    let mut folder_renames = Vec::new();
    for entry in &mut entries {
        let Some(display) = entry.display.clone() else {
            continue;
        };
        let folder = entry.folder.clone();
        if let Some(pos) = taken.iter().position(|f| *f == folder) {
            taken.swap_remove(pos);
        }
        let desired = folder_name_for_display(&display, &folder, &taken);
        if desired.eq_ignore_ascii_case(&folder) {
            taken.push(folder);
            continue;
        }
        let error = fs::rename(root.join(&folder), root.join(&desired)).err();
        folder_renames.push(LocalProfileFolderRename {
            current_folder: folder.clone(),
            desired_folder: desired.clone(),
            error,
        });
        if folder_renames
            .last()
            .is_some_and(|rename| rename.error.is_none())
        {
            taken.push(desired.clone());
            entry.folder = desired;
        } else {
            taken.push(folder);
        }
    }

    use std::collections::hash_map::Entry as MapEntry;
    let mut cache_map: HashMap<String, PathBuf> = HashMap::new();
    for entry in &entries {
        let path = root.join(&entry.folder);
        match cache_map.entry(entry.guid.clone()) {
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

    LocalProfilesMigrationResult {
        entries: entries
            .into_iter()
            .map(|entry| LocalProfileMigrationEntry {
                original_folder: entry.original_folder,
                folder: entry.folder,
                guid: entry.guid,
            })
            .collect(),
        guid_backfills,
        folder_renames,
        cache_map,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveProfile {
    Guest,
    Local { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveProfileLoadSelection {
    Guest,
    Local {
        id: String,
    },
    MissingFallbackLocal {
        missing_id: String,
        fallback_id: String,
    },
    MissingFallbackGuest {
        missing_id: String,
    },
}

impl ActiveProfileLoadSelection {
    #[inline(always)]
    pub fn local_id(&self) -> Option<&str> {
        match self {
            Self::Local { id } => Some(id),
            Self::MissingFallbackLocal { fallback_id, .. } => Some(fallback_id),
            Self::Guest | Self::MissingFallbackGuest { .. } => None,
        }
    }

    #[inline(always)]
    pub fn session_profile(&self) -> ActiveProfile {
        match self {
            Self::Local { id } => ActiveProfile::Local { id: id.clone() },
            Self::MissingFallbackLocal { fallback_id, .. } => ActiveProfile::Local {
                id: fallback_id.clone(),
            },
            Self::Guest | Self::MissingFallbackGuest { .. } => ActiveProfile::Guest,
        }
    }
}

#[inline(always)]
pub fn active_profile_is_guest(profile: &ActiveProfile) -> bool {
    matches!(profile, ActiveProfile::Guest)
}

#[inline(always)]
pub fn active_profile_local_id(profile: &ActiveProfile) -> Option<&str> {
    match profile {
        ActiveProfile::Local { id } => Some(id),
        ActiveProfile::Guest => None,
    }
}

pub fn resolve_active_profile_for_load(
    active: &ActiveProfile,
    local_profile_exists: impl FnOnce(&str) -> bool,
    fallback_local_profile_id: impl FnOnce() -> Option<String>,
) -> ActiveProfileLoadSelection {
    let Some(id) = active_profile_local_id(active) else {
        return ActiveProfileLoadSelection::Guest;
    };
    if local_profile_exists(id) {
        return ActiveProfileLoadSelection::Local { id: id.to_string() };
    }
    match fallback_local_profile_id() {
        Some(fallback_id) => ActiveProfileLoadSelection::MissingFallbackLocal {
            missing_id: id.to_string(),
            fallback_id,
        },
        None => ActiveProfileLoadSelection::MissingFallbackGuest {
            missing_id: id.to_string(),
        },
    }
}

/// Translate a stored default-profile id to a canonical GUID: pass valid GUIDs
/// through, map a legacy folder-name id to that folder's GUID, and otherwise
/// keep the value unchanged (for example, a stale id with no matching folder).
pub fn heal_default_profile_id(
    stored: Option<String>,
    folder_to_guid: &HashMap<&str, &str>,
) -> Option<String> {
    let id = stored?;
    if is_valid_profile_guid(&id) {
        return Some(id);
    }
    folder_to_guid
        .get(id.as_str())
        .map(|guid| (*guid).to_string())
        .or(Some(id))
}

pub fn default_active_profile_from_id(
    id: Option<String>,
    local_profile_exists: impl FnOnce(&str) -> bool,
) -> ActiveProfile {
    match id {
        Some(id) if is_local_profile_id(&id) && local_profile_exists(&id) => {
            ActiveProfile::Local { id }
        }
        _ => ActiveProfile::Guest,
    }
}

pub fn default_active_profile_for_side(
    defaults: &[Option<String>; PLAYER_SLOTS],
    side: PlayerSide,
    local_profile_exists: impl FnOnce(&str) -> bool,
) -> ActiveProfile {
    default_active_profile_from_id(
        defaults[player_side_index(side)].clone(),
        local_profile_exists,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayStyle {
    #[default]
    Single,
    Versus,
    Double,
}

impl PlayStyle {
    #[inline(always)]
    pub const fn chart_type(self) -> &'static str {
        match self {
            Self::Single | Self::Versus => "dance-single",
            Self::Double => "dance-double",
        }
    }

    #[inline(always)]
    pub const fn cols_per_player(self) -> usize {
        match self {
            Self::Single | Self::Versus => 4,
            Self::Double => 8,
        }
    }

    #[inline(always)]
    pub const fn player_count(self) -> usize {
        match self {
            Self::Single | Self::Double => 1,
            Self::Versus => 2,
        }
    }

    #[inline(always)]
    pub const fn total_cols(self) -> usize {
        self.cols_per_player() * self.player_count()
    }
}

#[inline(always)]
pub const fn player_options_section(style: PlayStyle) -> &'static str {
    match style {
        PlayStyle::Single | PlayStyle::Versus => "PlayerOptionsSingles",
        PlayStyle::Double => "PlayerOptionsDoubles",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayMode {
    #[default]
    Regular,
    Marathon,
}

pub const fn play_style_from_machine_preference(
    style: deadsync_config::theme::MachinePreferredPlayStyle,
) -> PlayStyle {
    match style {
        deadsync_config::theme::MachinePreferredPlayStyle::Single => PlayStyle::Single,
        deadsync_config::theme::MachinePreferredPlayStyle::Versus => PlayStyle::Versus,
        deadsync_config::theme::MachinePreferredPlayStyle::Double => PlayStyle::Double,
    }
}

pub const fn play_mode_from_machine_preference(
    mode: deadsync_config::theme::MachinePreferredPlayMode,
) -> PlayMode {
    match mode {
        deadsync_config::theme::MachinePreferredPlayMode::Regular => PlayMode::Regular,
        deadsync_config::theme::MachinePreferredPlayMode::Marathon => PlayMode::Marathon,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerSide {
    #[default]
    P1,
    P2,
}

#[inline(always)]
pub const fn player_side_index(side: PlayerSide) -> usize {
    match side {
        PlayerSide::P1 => 0,
        PlayerSide::P2 => 1,
    }
}

#[inline(always)]
pub const fn player_side_number(side: PlayerSide) -> u8 {
    match side {
        PlayerSide::P1 => 1,
        PlayerSide::P2 => 2,
    }
}

#[inline(always)]
pub const fn player_side_for_index(player_idx: usize) -> PlayerSide {
    match player_idx {
        1 => PlayerSide::P2,
        _ => PlayerSide::P1,
    }
}

#[inline(always)]
pub const fn player_side_joined_mask(side: PlayerSide) -> u8 {
    match side {
        PlayerSide::P1 => SESSION_JOINED_MASK_P1,
        PlayerSide::P2 => SESSION_JOINED_MASK_P2,
    }
}

#[inline(always)]
pub const fn joined_player_mask(p1: bool, p2: bool) -> u8 {
    let p1_mask = if p1 { SESSION_JOINED_MASK_P1 } else { 0 };
    let p2_mask = if p2 { SESSION_JOINED_MASK_P2 } else { 0 };
    p1_mask | p2_mask
}

#[inline(always)]
pub const fn play_style_for_joined(
    style: PlayStyle,
    p1_joined: bool,
    p2_joined: bool,
) -> PlayStyle {
    if p1_joined && p2_joined {
        PlayStyle::Versus
    } else {
        match style {
            PlayStyle::Versus => PlayStyle::Single,
            PlayStyle::Single | PlayStyle::Double => style,
        }
    }
}

#[inline(always)]
pub const fn player_side_is_joined(joined_mask: u8, side: PlayerSide) -> bool {
    joined_mask & player_side_joined_mask(side) != 0
}

#[inline(always)]
pub const fn runtime_player_is_p2(play_style: PlayStyle, side: PlayerSide) -> bool {
    matches!(
        (play_style, side),
        (PlayStyle::Single | PlayStyle::Double, PlayerSide::P2)
    )
}

#[inline(always)]
pub const fn is_single_p2_side(play_style: PlayStyle, side: PlayerSide) -> bool {
    matches!((play_style, side), (PlayStyle::Single, PlayerSide::P2))
}

#[inline(always)]
pub const fn runtime_player_index(play_style: PlayStyle, side: PlayerSide) -> usize {
    if matches!(play_style, PlayStyle::Versus) {
        player_side_index(side)
    } else {
        0
    }
}

#[inline(always)]
pub const fn runtime_player_side(
    play_style: PlayStyle,
    session_side: PlayerSide,
    player_idx: usize,
) -> PlayerSide {
    if matches!(play_style, PlayStyle::Versus) {
        player_side_for_index(player_idx)
    } else {
        session_side
    }
}

#[inline(always)]
pub const fn physical_player_slot_for_chart_pad(
    play_style: PlayStyle,
    session_side: PlayerSide,
    doubles: bool,
    chart_pad: usize,
) -> usize {
    if doubles {
        chart_pad
    } else {
        player_side_index(runtime_player_side(play_style, session_side, chart_pad))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimingTickMode {
    #[default]
    Off,
    Assist,
    Hit,
}

#[derive(Debug)]
pub struct SessionState {
    pub active_profiles: [ActiveProfile; PLAYER_SLOTS],
    pub joined_mask: u8,
    pub music_rate: f32,
    pub timing_tick_mode: TimingTickMode,
    pub play_style: PlayStyle,
    pub play_mode: PlayMode,
    pub player_side: PlayerSide,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionSnapshot {
    pub joined_mask: u8,
    pub music_rate: f32,
    pub timing_tick_mode: TimingTickMode,
    pub play_style: PlayStyle,
    pub play_mode: PlayMode,
    pub player_side: PlayerSide,
}

impl SessionSnapshot {
    pub const fn from_state(session: &SessionState) -> Self {
        Self {
            joined_mask: session.joined_mask,
            music_rate: session.music_rate,
            timing_tick_mode: session.timing_tick_mode,
            play_style: session.play_style,
            play_mode: session.play_mode,
            player_side: session.player_side,
        }
    }

    #[inline(always)]
    pub const fn side_joined(self, side: PlayerSide) -> bool {
        player_side_is_joined(self.joined_mask, side)
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            active_profiles: [ActiveProfile::Guest, ActiveProfile::Guest],
            joined_mask: joined_player_mask(true, false),
            music_rate: 1.0,
            timing_tick_mode: TimingTickMode::Off,
            play_style: PlayStyle::Single,
            play_mode: PlayMode::Regular,
            player_side: PlayerSide::P1,
        }
    }
}

impl SessionState {
    #[inline(always)]
    pub fn active_profile(&self, side: PlayerSide) -> ActiveProfile {
        self.active_profiles[player_side_index(side)].clone()
    }

    #[inline(always)]
    pub fn active_local_profile_id(&self, side: PlayerSide) -> Option<&str> {
        active_profile_local_id(&self.active_profiles[player_side_index(side)])
    }

    #[inline(always)]
    pub fn set_active_profile(&mut self, side: PlayerSide, profile: ActiveProfile) -> bool {
        let slot = &mut self.active_profiles[player_side_index(side)];
        if *slot == profile {
            return false;
        }
        *slot = profile;
        true
    }

    pub fn restore_default_profiles(
        &mut self,
        defaults: &[Option<String>; PLAYER_SLOTS],
        mut local_profile_exists: impl FnMut(&str) -> bool,
    ) {
        for side in [PlayerSide::P1, PlayerSide::P2] {
            self.active_profiles[player_side_index(side)] =
                default_active_profile_for_side(defaults, side, |id| local_profile_exists(id));
        }
    }

    pub fn restore_joined_default_profiles(
        &mut self,
        defaults: &[Option<String>; PLAYER_SLOTS],
        mut local_profile_exists: impl FnMut(&str) -> bool,
    ) -> [bool; PLAYER_SLOTS] {
        let mut changed = [false; PLAYER_SLOTS];
        for side in [PlayerSide::P1, PlayerSide::P2] {
            if !self.side_joined(side) {
                continue;
            }
            let idx = player_side_index(side);
            self.active_profiles[idx] =
                default_active_profile_for_side(defaults, side, |id| local_profile_exists(id));
            changed[idx] = true;
        }
        changed
    }

    #[inline(always)]
    pub fn music_rate(&self) -> f32 {
        if self.music_rate.is_finite() && self.music_rate > 0.0 {
            self.music_rate
        } else {
            1.0
        }
    }

    #[inline(always)]
    pub fn set_music_rate(&mut self, rate: f32) {
        self.music_rate = if rate.is_finite() && rate > 0.0 {
            rate.clamp(0.5, 3.0)
        } else {
            1.0
        };
    }

    #[inline(always)]
    pub const fn timing_tick_mode(&self) -> TimingTickMode {
        self.timing_tick_mode
    }

    #[inline(always)]
    pub fn set_timing_tick_mode(&mut self, mode: TimingTickMode) {
        self.timing_tick_mode = mode;
    }

    #[inline(always)]
    pub const fn play_style(&self) -> PlayStyle {
        self.play_style
    }

    pub fn set_play_style(&mut self, style: PlayStyle) -> Option<PlayStyle> {
        let prev_style = self.play_style;
        if prev_style == style {
            return None;
        }
        self.play_style = style;
        Some(prev_style)
    }

    #[inline(always)]
    pub const fn play_mode(&self) -> PlayMode {
        self.play_mode
    }

    #[inline(always)]
    pub fn set_play_mode(&mut self, mode: PlayMode) {
        self.play_mode = mode;
    }

    #[inline(always)]
    pub const fn player_side(&self) -> PlayerSide {
        self.player_side
    }

    #[inline(always)]
    pub fn set_player_side(&mut self, side: PlayerSide) {
        self.player_side = side;
    }

    #[inline(always)]
    pub const fn side_joined(&self, side: PlayerSide) -> bool {
        player_side_is_joined(self.joined_mask, side)
    }

    #[inline(always)]
    pub fn set_joined_sides(&mut self, p1: bool, p2: bool) {
        self.joined_mask = joined_player_mask(p1, p2);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Perspective {
    #[default]
    Overhead,
    Hallway,
    Distant,
    Incoming,
    Space,
}

impl Perspective {
    #[inline(always)]
    pub const fn tilt_skew(self) -> (f32, f32) {
        match self {
            Self::Overhead => (0.0, 0.0),
            Self::Hallway => (-1.0, 0.0),
            Self::Distant => (1.0, 0.0),
            Self::Incoming => (-1.0, 1.0),
            Self::Space => (1.0, 1.0),
        }
    }
}

impl FromStr for Perspective {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = s.trim().to_lowercase();
        match v.as_str() {
            "overhead" => Ok(Self::Overhead),
            "hallway" => Ok(Self::Hallway),
            "distant" => Ok(Self::Distant),
            "incoming" => Ok(Self::Incoming),
            "space" => Ok(Self::Space),
            other => Err(format!("'{other}' is not a valid Perspective setting")),
        }
    }
}

impl core::fmt::Display for Perspective {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Overhead => write!(f, "Overhead"),
            Self::Hallway => write!(f, "Hallway"),
            Self::Distant => write!(f, "Distant"),
            Self::Incoming => write!(f, "Incoming"),
            Self::Space => write!(f, "Space"),
        }
    }
}

/// Alternative speed-mod type to auto-apply when a chart is tagged "no CMod".
///
/// When a player is on CMod and selects a chart whose title/subtitle contains
/// "no cmod", the game transparently switches them to this mod type for that
/// play only. The persisted CMod setting is never written, so returning to
/// song select restores it. `None` leaves the player on CMod (they must switch
/// manually). See `player_options::apply_no_cmod_alternative`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoCmodAlternative {
    #[default]
    None,
    XMod,
    MMod,
}

impl FromStr for NoCmodAlternative {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "none" | "off" => Ok(Self::None),
            "xmod" | "x" => Ok(Self::XMod),
            "mmod" | "m" => Ok(Self::MMod),
            other => Err(format!(
                "'{other}' is not a valid NoCmodAlternative setting"
            )),
        }
    }
}

impl core::fmt::Display for NoCmodAlternative {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::XMod => write!(f, "XMod"),
            Self::MMod => write!(f, "MMod"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TurnOption {
    #[default]
    None,
    Mirror,
    Left,
    Right,
    LRMirror,
    UDMirror,
    Shuffle,
    Blender,
    Random,
}

impl FromStr for TurnOption {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "none" | "noturn" | "noturning" | "noturns" => Ok(Self::None),
            "mirror" => Ok(Self::Mirror),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            "lrmirror" => Ok(Self::LRMirror),
            "udmirror" => Ok(Self::UDMirror),
            "shuffle" => Ok(Self::Shuffle),
            "blender" | "supershuffle" => Ok(Self::Blender),
            "random" | "hypershuffle" => Ok(Self::Random),
            other => Err(format!("'{other}' is not a valid Turn setting")),
        }
    }
}

impl core::fmt::Display for TurnOption {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Mirror => write!(f, "Mirror"),
            Self::Left => write!(f, "Left"),
            Self::Right => write!(f, "Right"),
            Self::LRMirror => write!(f, "LRMirror"),
            Self::UDMirror => write!(f, "UDMirror"),
            Self::Shuffle => write!(f, "Shuffle"),
            Self::Blender => write!(f, "Blender"),
            Self::Random => write!(f, "Random"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollOption(u8);

#[allow(non_upper_case_globals)]
impl ScrollOption {
    pub const Normal: Self = Self(0);
    pub const Reverse: Self = Self(1 << 0);
    pub const Split: Self = Self(1 << 1);
    pub const Alternate: Self = Self(1 << 2);
    pub const Cross: Self = Self(1 << 3);
    pub const Centered: Self = Self(1 << 4);

    #[inline(always)]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[inline(always)]
    pub const fn contains(self, flag: Self) -> bool {
        (self.0 & flag.0) != 0
    }

    #[inline(always)]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[inline(always)]
    pub const fn is_normal(self) -> bool {
        self.0 == 0
    }
}

impl Default for ScrollOption {
    fn default() -> Self {
        Self::Normal
    }
}

impl FromStr for ScrollOption {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.trim();
        if raw.is_empty() {
            return Err("Scroll setting is empty".to_string());
        }
        let lower = raw.to_lowercase();
        let mut result = Self::empty();
        for token in lower.split(|c: char| c == '+' || c == ',' || c.is_whitespace()) {
            if token.is_empty() {
                continue;
            }
            let flag = match token {
                "normal" => Self::Normal,
                "reverse" => Self::Reverse,
                "split" => Self::Split,
                "alternate" => Self::Alternate,
                "cross" => Self::Cross,
                "centered" => Self::Centered,
                other => {
                    return Err(format!("'{other}' is not a valid Scroll setting"));
                }
            };
            if flag.0 != 0 {
                result = result.union(flag);
            }
        }
        Ok(result)
    }
}

impl core::fmt::Display for ScrollOption {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_normal() {
            return write!(f, "Normal");
        }

        let mut first = true;
        let mut write_flag = |name: &str, present: bool, f: &mut core::fmt::Formatter<'_>| {
            if !present {
                return Ok(());
            }
            if !first {
                write!(f, "+")?;
            }
            first = false;
            write!(f, "{name}")
        };

        write_flag("Reverse", self.contains(Self::Reverse), f)?;
        write_flag("Split", self.contains(Self::Split), f)?;
        write_flag("Alternate", self.contains(Self::Alternate), f)?;
        write_flag("Cross", self.contains(Self::Cross), f)?;
        write_flag("Centered", self.contains(Self::Centered), f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComboMode {
    #[default]
    FullCombo,
    CurrentCombo,
}

impl FromStr for ComboMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "fullcombo" => Ok(Self::FullCombo),
            "currentcombo" => Ok(Self::CurrentCombo),
            other => Err(format!("'{other}' is not a valid ComboMode setting")),
        }
    }
}

impl core::fmt::Display for ComboMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::FullCombo => write!(f, "FullCombo"),
            Self::CurrentCombo => write!(f, "CurrentCombo"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComboColors {
    #[default]
    Glow,
    Solid,
    Rainbow,
    RainbowScroll,
    None,
}

impl FromStr for ComboColors {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "glow" => Ok(Self::Glow),
            "solid" => Ok(Self::Solid),
            "rainbow" => Ok(Self::Rainbow),
            "rainbowscroll" => Ok(Self::RainbowScroll),
            "none" => Ok(Self::None),
            other => Err(format!("'{other}' is not a valid ComboColors setting")),
        }
    }
}

impl core::fmt::Display for ComboColors {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Glow => write!(f, "Glow"),
            Self::Solid => write!(f, "Solid"),
            Self::Rainbow => write!(f, "Rainbow"),
            Self::RainbowScroll => write!(f, "RainbowScroll"),
            Self::None => write!(f, "None"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComboFont {
    #[default]
    Wendy,
    ArialRounded,
    Asap,
    BebasNeue,
    SourceCode,
    Work,
    WendyCursed,
    Mega,
    None,
}

impl FromStr for ComboFont {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = s.trim().to_lowercase();
        match v.as_str() {
            "wendy" => Ok(Self::Wendy),
            "arial rounded" | "arialrounded" => Ok(Self::ArialRounded),
            "asap" => Ok(Self::Asap),
            "bebas neue" | "bebasneue" => Ok(Self::BebasNeue),
            "source code" | "sourcecode" => Ok(Self::SourceCode),
            "work" => Ok(Self::Work),
            "wendy (cursed)" | "wendy cursed" | "wendycursed" => Ok(Self::WendyCursed),
            "mega" => Ok(Self::Mega),
            "none" => Ok(Self::None),
            other => Err(format!("'{other}' is not a valid ComboFont setting")),
        }
    }
}

impl core::fmt::Display for ComboFont {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Wendy => write!(f, "Wendy"),
            Self::ArialRounded => write!(f, "Arial Rounded"),
            Self::Asap => write!(f, "Asap"),
            Self::BebasNeue => write!(f, "Bebas Neue"),
            Self::SourceCode => write!(f, "Source Code"),
            Self::Work => write!(f, "Work"),
            Self::WendyCursed => write!(f, "Wendy (Cursed)"),
            Self::Mega => write!(f, "Mega"),
            Self::None => write!(f, "None"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetScoreSetting {
    CMinus,
    C,
    CPlus,
    BMinus,
    B,
    BPlus,
    AMinus,
    A,
    APlus,
    SMinus,
    #[default]
    S,
    SPlus,
    MachineBest,
    PersonalBest,
}

impl FromStr for TargetScoreSetting {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "cminus" | "c-" => Ok(Self::CMinus),
            "c" => Ok(Self::C),
            "cplus" | "c+" => Ok(Self::CPlus),
            "bminus" | "b-" => Ok(Self::BMinus),
            "b" => Ok(Self::B),
            "bplus" | "b+" => Ok(Self::BPlus),
            "aminus" | "a-" => Ok(Self::AMinus),
            "a" => Ok(Self::A),
            "aplus" | "a+" => Ok(Self::APlus),
            "sminus" | "s-" => Ok(Self::SMinus),
            "" | "s" => Ok(Self::S),
            "splus" | "s+" => Ok(Self::SPlus),
            "machinebest" | "machine" => Ok(Self::MachineBest),
            "personalbest" | "personal" => Ok(Self::PersonalBest),
            other => Err(format!("'{other}' is not a valid TargetScore setting")),
        }
    }
}

impl core::fmt::Display for TargetScoreSetting {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CMinus => write!(f, "C-"),
            Self::C => write!(f, "C"),
            Self::CPlus => write!(f, "C+"),
            Self::BMinus => write!(f, "B-"),
            Self::B => write!(f, "B"),
            Self::BPlus => write!(f, "B+"),
            Self::AMinus => write!(f, "A-"),
            Self::A => write!(f, "A"),
            Self::APlus => write!(f, "A+"),
            Self::SMinus => write!(f, "S-"),
            Self::S => write!(f, "S"),
            Self::SPlus => write!(f, "S+"),
            Self::MachineBest => write!(f, "Machine Best"),
            Self::PersonalBest => write!(f, "Personal Best"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorBarStyle {
    #[default]
    None,
    Colorful,
    Monochrome,
    Text,
    Highlight,
    Average,
}

impl FromStr for ErrorBarStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "colorful" => Ok(Self::Colorful),
            "monochrome" => Ok(Self::Monochrome),
            "text" => Ok(Self::Text),
            "highlight" => Ok(Self::Highlight),
            "average" => Ok(Self::Average),
            other => Err(format!("'{other}' is not a valid ErrorBar setting")),
        }
    }
}

impl core::fmt::Display for ErrorBarStyle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Colorful => write!(f, "Colorful"),
            Self::Monochrome => write!(f, "Monochrome"),
            Self::Text => write!(f, "Text"),
            Self::Highlight => write!(f, "Highlight"),
            Self::Average => write!(f, "Average"),
        }
    }
}

bitflags! {
    /// Persisted bitmask of live timing statistics shown during gameplay.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct LiveTimingStatsMask: u8 {
        const MEAN     = 1 << 0;
        const MEAN_ABS = 1 << 1;
        const MAX      = 1 << 2;
    }
}

bitflags! {
    /// Persisted bitmask for the Error Bar SelectMultiple row.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct ErrorBarMask: u8 {
        const COLORFUL   = 1 << 0;
        const MONOCHROME = 1 << 1;
        const TEXT       = 1 << 2;
        const HIGHLIGHT  = 1 << 3;
        const AVERAGE    = 1 << 4;
    }
}

#[inline(always)]
pub const fn error_bar_mask_from_style(style: ErrorBarStyle, text: bool) -> ErrorBarMask {
    let text_bits = if text { ErrorBarMask::TEXT.bits() } else { 0 };
    let style_bits = match style {
        ErrorBarStyle::None => 0,
        ErrorBarStyle::Colorful => ErrorBarMask::COLORFUL.bits(),
        ErrorBarStyle::Monochrome => ErrorBarMask::MONOCHROME.bits(),
        ErrorBarStyle::Text => ErrorBarMask::TEXT.bits(),
        ErrorBarStyle::Highlight => ErrorBarMask::HIGHLIGHT.bits(),
        ErrorBarStyle::Average => ErrorBarMask::AVERAGE.bits(),
    };
    ErrorBarMask::from_bits_truncate(text_bits | style_bits)
}

#[inline(always)]
pub const fn error_bar_style_from_mask(mask: ErrorBarMask) -> ErrorBarStyle {
    if mask.contains(ErrorBarMask::COLORFUL) {
        ErrorBarStyle::Colorful
    } else if mask.contains(ErrorBarMask::MONOCHROME) {
        ErrorBarStyle::Monochrome
    } else if mask.contains(ErrorBarMask::HIGHLIGHT) {
        ErrorBarStyle::Highlight
    } else if mask.contains(ErrorBarMask::AVERAGE) {
        ErrorBarStyle::Average
    } else {
        ErrorBarStyle::None
    }
}

#[inline(always)]
pub const fn error_bar_text_from_mask(mask: ErrorBarMask) -> bool {
    mask.contains(ErrorBarMask::TEXT)
}

bitflags! {
    /// Persisted bitmask of enabled appearance transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct AppearanceEffectsMask: u8 {
        const HIDDEN         = 1 << 0;
        const SUDDEN         = 1 << 1;
        const STEALTH        = 1 << 2;
        const BLINK          = 1 << 3;
        const RANDOM_VANISH  = 1 << 4;
    }
}

bitflags! {
    /// Persisted bitmask of enabled acceleration transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct AccelEffectsMask: u8 {
        const BOOST     = 1 << 0;
        const BRAKE     = 1 << 1;
        const WAVE      = 1 << 2;
        const EXPAND    = 1 << 3;
        const BOOMERANG = 1 << 4;
    }
}

bitflags! {
    /// Persisted bitmask of enabled hold transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct HoldsMask: u8 {
        const PLANTED        = 1 << 0;
        const FLOORED        = 1 << 1;
        const TWISTER        = 1 << 2;
        const NO_ROLLS       = 1 << 3;
        const HOLDS_TO_ROLLS = 1 << 4;
    }
}

bitflags! {
    /// Persisted bitmask of enabled visual transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct VisualEffectsMask: u16 {
        const DRUNK     = 1 << 0;
        const DIZZY     = 1 << 1;
        const CONFUSION = 1 << 2;
        const BIG       = 1 << 3;
        const FLIP      = 1 << 4;
        const INVERT    = 1 << 5;
        const TORNADO   = 1 << 6;
        const TIPSY     = 1 << 7;
        const BUMPY     = 1 << 8;
        const BEAT      = 1 << 9;
    }
}

bitflags! {
    /// Persisted bitmask of enabled chart insert transforms.
    ///
    /// Bit layout matches the runtime insert-mask constants, except bit 7
    /// (Mines) is runtime/attack-only and is deliberately not represented
    /// here.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct InsertMask: u8 {
        const WIDE   = 1 << 0;
        const BIG    = 1 << 1;
        const QUICK  = 1 << 2;
        const BMRIZE = 1 << 3;
        const SKIPPY = 1 << 4;
        const ECHO   = 1 << 5;
        const STOMP  = 1 << 6;
    }
}

bitflags! {
    /// Persisted bitmask of enabled chart removal transforms.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct RemoveMask: u8 {
        const LITTLE   = 1 << 0;
        const NO_MINES = 1 << 1;
        const NO_HOLDS = 1 << 2;
        const NO_JUMPS = 1 << 3;
        const NO_HANDS = 1 << 4;
        const NO_QUADS = 1 << 5;
        const NO_LIFTS = 1 << 6;
        const NO_FAKES = 1 << 7;
    }
}

bitflags! {
    /// Persisted bitmask of tap explosion windows enabled for gameplay.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct TapExplosionMask: u8 {
        const FANTASTIC = 1 << 0;
        const EXCELLENT = 1 << 1;
        const GREAT     = 1 << 2;
        const DECENT    = 1 << 3;
        const WAY_OFF   = 1 << 4;
        const HELD      = 1 << 5;
        const MISS      = 1 << 6;
        const HOLDING   = 1 << 7;
    }
}

bitflags! {
    /// Persisted bitmask of judgments that trigger gameplay column flashes.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct ColumnFlashMask: u8 {
        const BLUE_FANTASTIC  = 1 << 0;
        const WHITE_FANTASTIC = 1 << 1;
        const EXCELLENT       = 1 << 2;
        const GREAT           = 1 << 3;
        const DECENT          = 1 << 4;
        const WAY_OFF         = 1 << 5;
        const MISS            = 1 << 6;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnFlashBrightness {
    #[default]
    Normal,
    Dimmed,
}

impl FromStr for ColumnFlashBrightness {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "normal" | "default" | "standard" => Ok(Self::Normal),
            "dimmed" | "dim" | "chris" | "compact" => Ok(Self::Dimmed),
            other => Err(format!(
                "'{other}' is not a valid ColumnFlashBrightness setting"
            )),
        }
    }
}

impl core::fmt::Display for ColumnFlashBrightness {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Normal => write!(f, "Normal"),
            Self::Dimmed => write!(f, "Dimmed"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnFlashSize {
    #[default]
    Default,
    Compact,
}

impl FromStr for ColumnFlashSize {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "default" | "normal" | "full" | "standard" => Ok(Self::Default),
            "compact" | "short" | "shorter" | "chris" => Ok(Self::Compact),
            other => Err(format!("'{other}' is not a valid ColumnFlashSize setting")),
        }
    }
}

impl core::fmt::Display for ColumnFlashSize {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Default => write!(f, "Default"),
            Self::Compact => write!(f, "Compact"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AttackMode {
    Off,
    #[default]
    On,
    Random,
}

impl FromStr for AttackMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "off" | "noattacks" | "noattack" => Ok(Self::Off),
            "on" | "normal" => Ok(Self::On),
            "random" | "randomattacks" => Ok(Self::Random),
            other => Err(format!("'{other}' is not a valid AttackMode setting")),
        }
    }
}

impl core::fmt::Display for AttackMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::On => write!(f, "On"),
            Self::Random => write!(f, "Random"),
        }
    }
}

/// Hard cap for the evaluation scatter plot's vertical scale, selectable
/// per profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScatterplotMaxWindow {
    #[default]
    Off,
    Fantastic,
    Excellent,
    Great,
}

impl FromStr for ScatterplotMaxWindow {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "off" | "none" | "autoscale" | "0" => Ok(Self::Off),
            "fantastic" | "fantasticmax" | "fa" => Ok(Self::Fantastic),
            "excellent" | "excellentmax" | "ex" => Ok(Self::Excellent),
            "great" | "greatmax" | "gr" => Ok(Self::Great),
            other => Err(format!(
                "'{other}' is not a valid ScatterplotMaxWindow setting"
            )),
        }
    }
}

impl core::fmt::Display for ScatterplotMaxWindow {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Fantastic => write!(f, "Fantastic"),
            Self::Excellent => write!(f, "Excellent"),
            Self::Great => write!(f, "Great"),
        }
    }
}

/// Gameplay percent score placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScorePosition {
    #[default]
    Normal,
    StepStatistics,
}

impl FromStr for ScorePosition {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "normal" | "default" | "top" => Ok(Self::Normal),
            "stepstatistics" | "stepstats" | "stats" => Ok(Self::StepStatistics),
            other => Err(format!("'{other}' is not a valid ScorePosition setting")),
        }
    }
}

impl core::fmt::Display for ScorePosition {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Normal => write!(f, "Normal"),
            Self::StepStatistics => write!(f, "Step Statistics"),
        }
    }
}

/// Gameplay percent score value semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScoreDisplayMode {
    #[default]
    Normal,
    Predictive,
}

impl FromStr for ScoreDisplayMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "normal" | "default" | "actual" | "current" => Ok(Self::Normal),
            "predictive" | "predicted" | "prediction" => Ok(Self::Predictive),
            other => Err(format!("'{other}' is not a valid ScoreDisplay setting")),
        }
    }
}

impl core::fmt::Display for ScoreDisplayMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Normal => write!(f, "Normal"),
            Self::Predictive => write!(f, "Predictive"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LifeMeterType {
    #[default]
    Standard,
    Surround,
    Vertical,
}

impl FromStr for LifeMeterType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "" | "standard" => Ok(Self::Standard),
            "surround" => Ok(Self::Surround),
            "vertical" => Ok(Self::Vertical),
            other => Err(format!("'{other}' is not a valid LifeMeterType setting")),
        }
    }
}

impl core::fmt::Display for LifeMeterType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Standard => write!(f, "Standard"),
            Self::Surround => write!(f, "Surround"),
            Self::Vertical => write!(f, "Vertical"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorBarTrim {
    #[default]
    Off,
    Fantastic,
    Excellent,
    Great,
}

impl FromStr for ErrorBarTrim {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "fantastic" => Ok(Self::Fantastic),
            "excellent" => Ok(Self::Excellent),
            "great" => Ok(Self::Great),
            other => Err(format!("'{other}' is not a valid ErrorBarTrim setting")),
        }
    }
}

impl core::fmt::Display for ErrorBarTrim {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Fantastic => write!(f, "Fantastic"),
            Self::Excellent => write!(f, "Excellent"),
            Self::Great => write!(f, "Great"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimingWindowsOption {
    #[default]
    None,
    WayOffs,
    DecentsAndWayOffs,
    FantasticsAndExcellents,
}

impl TimingWindowsOption {
    #[inline(always)]
    pub const fn disabled_windows(self) -> [bool; 5] {
        match self {
            Self::None => [false; 5],
            Self::WayOffs => [false, false, false, false, true],
            Self::DecentsAndWayOffs => [false, false, false, true, true],
            Self::FantasticsAndExcellents => [true, true, false, false, false],
        }
    }
}

impl FromStr for TimingWindowsOption {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "way offs" | "wayoffs" => Ok(Self::WayOffs),
            "decents + way offs" | "decents+wayoffs" | "decents and way offs" => {
                Ok(Self::DecentsAndWayOffs)
            }
            "fantastics + excellents" | "fantastics+excellents" | "fantastics and excellents" => {
                Ok(Self::FantasticsAndExcellents)
            }
            other => Err(format!("'{other}' is not a valid TimingWindows setting")),
        }
    }
}

impl core::fmt::Display for TimingWindowsOption {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::WayOffs => write!(f, "Way Offs"),
            Self::DecentsAndWayOffs => write!(f, "Decents + Way Offs"),
            Self::FantasticsAndExcellents => write!(f, "Fantastics + Excellents"),
        }
    }
}

bitflags! {
    /// Persisted bitmask of enabled Step Statistics gameplay widgets.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct StepStatisticsMask: u16 {
        const DENSITY_GRAPH    = 1 << 0;
        const SONG_BANNER      = 1 << 1;
        const JUDGMENT_COUNTER = 1 << 2;
        const SONG_DURATION    = 1 << 3;
        const PACK_BANNER      = 1 << 4;
        const SONG_INFO        = 1 << 5;
        const STEP_COUNTS      = 1 << 6;
        const PEAK_NPS         = 1 << 7;
    }
}

impl StepStatisticsMask {
    pub const ALL_WIDGET_BITS: u16 = Self::DENSITY_GRAPH.bits()
        | Self::SONG_BANNER.bits()
        | Self::JUDGMENT_COUNTER.bits()
        | Self::SONG_DURATION.bits()
        | Self::PACK_BANNER.bits()
        | Self::SONG_INFO.bits()
        | Self::STEP_COUNTS.bits()
        | Self::PEAK_NPS.bits();

    #[inline(always)]
    pub const fn all_widgets() -> Self {
        Self::from_bits_retain(Self::ALL_WIDGET_BITS)
    }

    #[inline(always)]
    pub fn pack_info_enabled(self) -> bool {
        self.intersects(Self::PACK_BANNER | Self::SONG_INFO)
    }
}

fn normalize_option_key(s: &str) -> String {
    let mut key = String::with_capacity(s.len());
    for ch in s.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_lowercase());
        }
    }
    key
}

fn step_statistics_bit_from_key(key: &str) -> Option<StepStatisticsMask> {
    match key {
        "densitygraph" | "density" => Some(StepStatisticsMask::DENSITY_GRAPH),
        "songbanner" | "banner" => Some(StepStatisticsMask::SONG_BANNER),
        "judgmentcounter" | "judgementcounter" | "judgmentcounts" | "judgementcounts"
        | "judgmentscounter" | "judgementscounter" | "judgment" | "judgement" | "judgments"
        | "judgements" => Some(StepStatisticsMask::JUDGMENT_COUNTER),
        "songduration" | "songtime" | "duration" | "time" => {
            Some(StepStatisticsMask::SONG_DURATION)
        }
        "packbanner" | "packinfo" | "songinfo" => Some(StepStatisticsMask::PACK_BANNER),
        "stepcounts" | "steps" | "holdsminesrolls" | "jumpsminesholds" => {
            Some(StepStatisticsMask::STEP_COUNTS)
        }
        "peaknps" => Some(StepStatisticsMask::PEAK_NPS),
        _ => None,
    }
}

impl FromStr for StepStatisticsMask {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        let key = normalize_option_key(trimmed);
        match key.as_str() {
            "" | "none" => return Ok(Self::empty()),
            // Legacy DataVisualizations values.
            "targetscoregraph" | "targetscore" | "target" => return Ok(Self::empty()),
            "stepstatistics" | "stepstats" => return Ok(Self::all_widgets()),
            _ => {}
        }

        if let Ok(bits) = trimmed.parse::<u16>() {
            return Ok(Self::from_bits_retain(bits & Self::ALL_WIDGET_BITS));
        }

        let mut mask = Self::empty();
        for part in trimmed.split([',', '|', ';']) {
            let key = normalize_option_key(part);
            if key.is_empty() {
                continue;
            }
            if matches!(key.as_str(), "gsbox" | "groovestatsbox" | "scorebox") {
                continue;
            }
            let Some(bit) = step_statistics_bit_from_key(key.as_str()) else {
                return Err(format!("'{part}' is not a valid StepStatistics setting"));
            };
            mask.insert(bit);
        }
        Ok(mask)
    }
}

impl core::fmt::Display for StepStatisticsMask {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        const BEFORE_PACK: [(StepStatisticsMask, &str); 4] = [
            (StepStatisticsMask::DENSITY_GRAPH, "Density Graph"),
            (StepStatisticsMask::SONG_BANNER, "Song Banner"),
            (StepStatisticsMask::JUDGMENT_COUNTER, "Judgements"),
            (StepStatisticsMask::SONG_DURATION, "Song Duration"),
        ];
        const AFTER_PACK: [(StepStatisticsMask, &str); 2] = [
            (StepStatisticsMask::STEP_COUNTS, "Step Counts"),
            (StepStatisticsMask::PEAK_NPS, "Peak NPS"),
        ];
        if self.is_empty() {
            return write!(f, "None");
        }
        let mut first = true;
        for (bit, label) in BEFORE_PACK {
            if !self.contains(bit) {
                continue;
            }
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{label}")?;
            first = false;
        }
        if self.pack_info_enabled() {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "Pack Info")?;
            first = false;
        }
        for (bit, label) in AFTER_PACK {
            if !self.contains(bit) {
                continue;
            }
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{label}")?;
            first = false;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepStatsExtra {
    #[default]
    None,
    ErrorStats,
    AmongUs,
    Bocchi,
    BrodyQuest,
    CatJAM,
    CrabPls,
    DancingDuck,
    DonChan,
    NyanCat,
    Randomizer,
    RinCat,
    Snoop,
    Sonic,
}

impl StepStatsExtra {
    pub const RANDOMIZER_CHOICES: [Self; 11] = [
        Self::AmongUs,
        Self::Bocchi,
        Self::BrodyQuest,
        Self::CatJAM,
        Self::CrabPls,
        Self::DancingDuck,
        Self::DonChan,
        Self::NyanCat,
        Self::RinCat,
        Self::Snoop,
        Self::Sonic,
    ];

    #[inline(always)]
    pub const fn renderable(self) -> bool {
        !matches!(self, Self::None | Self::ErrorStats | Self::Randomizer)
    }
}

impl FromStr for StepStatsExtra {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match normalize_option_key(s).as_str() {
            "" | "none" => Ok(Self::None),
            "errorstats" | "error" => Ok(Self::ErrorStats),
            "amongus" => Ok(Self::AmongUs),
            "bocchi" => Ok(Self::Bocchi),
            "brodyquest" => Ok(Self::BrodyQuest),
            "catjam" => Ok(Self::CatJAM),
            "crabpls" => Ok(Self::CrabPls),
            "dancingduck" => Ok(Self::DancingDuck),
            "donchan" => Ok(Self::DonChan),
            "nyancat" => Ok(Self::NyanCat),
            "randomizer" | "random" => Ok(Self::Randomizer),
            "rincat" => Ok(Self::RinCat),
            "snoop" => Ok(Self::Snoop),
            "sonic" => Ok(Self::Sonic),
            other => Err(format!("'{other}' is not a valid StepStatsExtra setting")),
        }
    }
}

impl core::fmt::Display for StepStatsExtra {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::ErrorStats => write!(f, "ErrorStats"),
            Self::AmongUs => write!(f, "AmongUs"),
            Self::Bocchi => write!(f, "Bocchi"),
            Self::BrodyQuest => write!(f, "BrodyQuest"),
            Self::CatJAM => write!(f, "CatJAM"),
            Self::CrabPls => write!(f, "CrabPls"),
            Self::DancingDuck => write!(f, "Dancing Duck"),
            Self::DonChan => write!(f, "DonChan"),
            Self::NyanCat => write!(f, "Nyan Cat"),
            Self::Randomizer => write!(f, "Randomizer"),
            Self::RinCat => write!(f, "Rin Cat"),
            Self::Snoop => write!(f, "Snoop"),
            Self::Sonic => write!(f, "Sonic"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MeasureCounter {
    #[default]
    None,
    Eighth,
    Twelfth,
    Sixteenth,
    TwentyFourth,
    ThirtySecond,
}

impl MeasureCounter {
    #[inline(always)]
    pub const fn notes_threshold(self) -> Option<usize> {
        match self {
            Self::None => None,
            Self::Eighth => Some(8),
            Self::Twelfth => Some(12),
            Self::Sixteenth => Some(16),
            Self::TwentyFourth => Some(24),
            Self::ThirtySecond => Some(32),
        }
    }

    #[inline(always)]
    pub const fn multiplier(self) -> f32 {
        match self {
            Self::TwentyFourth => 1.5,
            Self::ThirtySecond => 2.0,
            _ => 1.0,
        }
    }
}

impl FromStr for MeasureCounter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "8th" => Ok(Self::Eighth),
            "12th" => Ok(Self::Twelfth),
            "16th" => Ok(Self::Sixteenth),
            "24th" => Ok(Self::TwentyFourth),
            "32nd" => Ok(Self::ThirtySecond),
            other => Err(format!("'{other}' is not a valid MeasureCounter setting")),
        }
    }
}

impl core::fmt::Display for MeasureCounter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Eighth => write!(f, "8th"),
            Self::Twelfth => write!(f, "12th"),
            Self::Sixteenth => write!(f, "16th"),
            Self::TwentyFourth => write!(f, "24th"),
            Self::ThirtySecond => write!(f, "32nd"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MeasureLines {
    #[default]
    Off,
    Measure,
    Quarter,
    Eighth,
}

impl FromStr for MeasureLines {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "measure" => Ok(Self::Measure),
            "quarter" => Ok(Self::Quarter),
            "eighth" => Ok(Self::Eighth),
            other => Err(format!("'{other}' is not a valid MeasureLines setting")),
        }
    }
}

impl core::fmt::Display for MeasureLines {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Measure => write!(f, "Measure"),
            Self::Quarter => write!(f, "Quarter"),
            Self::Eighth => write!(f, "Eighth"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicator {
    #[default]
    None,
    SubtractiveScoring,
    PredictiveScoring,
    PaceScoring,
    RivalScoring,
    Pacemaker,
    StreamProg,
}

impl FromStr for MiniIndicator {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "none" => Ok(Self::None),
            "subtractivescoring" | "subtractive" => Ok(Self::SubtractiveScoring),
            "predictivescoring" | "predictive" => Ok(Self::PredictiveScoring),
            "pacescoring" | "pace" => Ok(Self::PaceScoring),
            "rivalscoring" | "rival" => Ok(Self::RivalScoring),
            "pacemaker" => Ok(Self::Pacemaker),
            "streamprog" | "streamprogress" | "stream" => Ok(Self::StreamProg),
            other => Err(format!("'{other}' is not a valid MiniIndicator setting")),
        }
    }
}

impl core::fmt::Display for MiniIndicator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::SubtractiveScoring => write!(f, "SubtractiveScoring"),
            Self::PredictiveScoring => write!(f, "PredictiveScoring"),
            Self::PaceScoring => write!(f, "PaceScoring"),
            Self::RivalScoring => write!(f, "RivalScoring"),
            Self::Pacemaker => write!(f, "Pacemaker"),
            Self::StreamProg => write!(f, "StreamProg"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicatorScoreType {
    #[default]
    Itg,
    Ex,
    HardEx,
}

impl FromStr for MiniIndicatorScoreType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "itg" => Ok(Self::Itg),
            "ex" => Ok(Self::Ex),
            "hardex" | "hex" => Ok(Self::HardEx),
            other => Err(format!(
                "'{other}' is not a valid MiniIndicatorScoreType setting"
            )),
        }
    }
}

impl core::fmt::Display for MiniIndicatorScoreType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Itg => write!(f, "ITG"),
            Self::Ex => write!(f, "Ex"),
            Self::HardEx => write!(f, "HardEx"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicatorSubtractiveDisplay {
    #[default]
    Percent,
    Points,
}

impl FromStr for MiniIndicatorSubtractiveDisplay {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "percent" | "percentage" => Ok(Self::Percent),
            "points" | "point" | "dancepoints" | "dp" => Ok(Self::Points),
            other => Err(format!(
                "'{other}' is not a valid MiniIndicatorSubtractiveDisplay setting"
            )),
        }
    }
}

impl core::fmt::Display for MiniIndicatorSubtractiveDisplay {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Percent => write!(f, "Percent"),
            Self::Points => write!(f, "Points"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicatorSize {
    #[default]
    Default,
    Large,
}

impl FromStr for MiniIndicatorSize {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "default" => Ok(Self::Default),
            "large" | "big" => Ok(Self::Large),
            other => Err(format!(
                "'{other}' is not a valid MiniIndicatorSize setting"
            )),
        }
    }
}

impl core::fmt::Display for MiniIndicatorSize {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Default => write!(f, "Default"),
            Self::Large => write!(f, "Large"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicatorColor {
    #[default]
    Default,
    Detailed,
    Combo,
}

impl FromStr for MiniIndicatorColor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "default" => Ok(Self::Default),
            "detailed" => Ok(Self::Detailed),
            "combo" | "combocolor" | "combocolour" => Ok(Self::Combo),
            other => Err(format!(
                "'{other}' is not a valid MiniIndicatorColor setting"
            )),
        }
    }
}

impl core::fmt::Display for MiniIndicatorColor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Default => write!(f, "Default"),
            Self::Detailed => write!(f, "Detailed"),
            Self::Combo => write!(f, "Combo"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiniIndicatorPosition {
    #[default]
    Default,
    UnderUpArrow,
}

impl FromStr for MiniIndicatorPosition {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "" | "default" | "normal" => Ok(Self::Default),
            "underuparrow" | "uparrow" | "arrow" | "left" => Ok(Self::UnderUpArrow),
            other => Err(format!(
                "'{other}' is not a valid MiniIndicatorPosition setting"
            )),
        }
    }
}

impl core::fmt::Display for MiniIndicatorPosition {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Default => write!(f, "Default"),
            Self::UnderUpArrow => write!(f, "UnderUpArrow"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HideLightType {
    #[default]
    NoHideLights,
    HideAllLights,
    HideMarqueeLights,
    HideBassLights,
}

impl FromStr for HideLightType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = String::with_capacity(s.len());
        for ch in s.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch.to_ascii_lowercase());
            }
        }
        match key.as_str() {
            "nohidelights" => Ok(Self::NoHideLights),
            "hidealllights" => Ok(Self::HideAllLights),
            "hidemarqueelights" => Ok(Self::HideMarqueeLights),
            "hidebasslights" => Ok(Self::HideBassLights),
            other => Err(format!("'{other}' is not a valid HideLightType setting")),
        }
    }
}

impl core::fmt::Display for HideLightType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoHideLights => write!(f, "NoHideLights"),
            Self::HideAllLights => write!(f, "HideAllLights"),
            Self::HideMarqueeLights => write!(f, "HideMarqueeLights"),
            Self::HideBassLights => write!(f, "HideBassLights"),
        }
    }
}

/// Background-darkening alpha for the per-notefield underlay quad, expressed
/// as an integer percentage in `0..=100` (0 = no filter, 100 = fully opaque
/// black). Reads accept the legacy enum labels (`Off|Dark|Darker|Darkest`) so
/// existing profiles migrate automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackgroundFilter(u8);

impl BackgroundFilter {
    /// Default for new profiles. Matches the old `Darkest` enum variant.
    pub const DEFAULT: Self = Self(95);
    pub const OFF: Self = Self(0);
    pub const MAX_PERCENT: u8 = 100;

    /// Construct from a raw percentage, clamping to `0..=100`.
    #[inline]
    pub const fn from_percent(value: u8) -> Self {
        let clamped = if value > Self::MAX_PERCENT {
            Self::MAX_PERCENT
        } else {
            value
        };
        Self(clamped)
    }

    /// Construct from any signed integer, clamping to `0..=100`.
    #[inline]
    pub fn from_i32(value: i32) -> Self {
        Self::from_percent(value.clamp(0, Self::MAX_PERCENT as i32) as u8)
    }

    /// Underlying percentage value `0..=100`.
    #[inline]
    pub const fn percent(self) -> u8 {
        self.0
    }

    /// Alpha value in `0.0..=1.0` to be passed to `diffuse`.
    #[inline]
    pub fn alpha(self) -> f32 {
        self.0 as f32 / Self::MAX_PERCENT as f32
    }

    /// Convenience for branches that toggle on the "no filter" case.
    #[inline]
    pub const fn is_off(self) -> bool {
        self.0 == 0
    }
}

impl Default for BackgroundFilter {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl FromStr for BackgroundFilter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        match trimmed.to_ascii_lowercase().as_str() {
            "off" => return Ok(Self(0)),
            "dark" => return Ok(Self(50)),
            "darker" => return Ok(Self(75)),
            "darkest" => return Ok(Self(95)),
            _ => {}
        }

        let numeric = trimmed.trim_end_matches('%').trim();
        let value: i32 = numeric
            .parse()
            .map_err(|_| format!("'{s}' is not a valid BackgroundFilter setting"))?;
        if !(0..=Self::MAX_PERCENT as i32).contains(&value) {
            return Err(format!(
                "BackgroundFilter percent {value} out of range 0..=100"
            ));
        }
        Ok(Self(value as u8))
    }
}

impl core::fmt::Display for BackgroundFilter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NoteSkin {
    raw: String,
}

impl NoteSkin {
    pub const DEFAULT_NAME: &'static str = "default";
    pub const CEL_NAME: &'static str = "cel";
    pub const NONE_NAME: &'static str = "__none__";

    #[inline(always)]
    fn normalize(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(trimmed.to_ascii_lowercase())
    }

    #[inline(always)]
    pub fn new(raw: &str) -> Self {
        Self::from_str(raw).unwrap_or_default()
    }

    #[inline(always)]
    pub fn none_choice() -> Self {
        Self {
            raw: Self::NONE_NAME.to_string(),
        }
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    #[inline(always)]
    pub fn is_none_choice(&self) -> bool {
        self.raw == Self::NONE_NAME
    }
}

impl Default for NoteSkin {
    fn default() -> Self {
        Self {
            raw: Self::CEL_NAME.to_string(),
        }
    }
}

impl FromStr for NoteSkin {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = Self::normalize(s)
            .ok_or_else(|| format!("'{}' is not a valid NoteSkin setting", s.trim()))?;
        Ok(Self { raw: normalized })
    }
}

impl core::fmt::Display for NoteSkin {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.raw)
    }
}

#[inline(always)]
pub fn resolve_noteskin_choice<'a>(
    noteskin: Option<&'a NoteSkin>,
    fallback: &'a NoteSkin,
) -> &'a NoteSkin {
    noteskin.unwrap_or(fallback)
}

#[inline(always)]
pub fn tap_explosion_skin_hidden(noteskin: Option<&NoteSkin>) -> bool {
    noteskin.is_some_and(NoteSkin::is_none_choice)
}

pub fn evaluation_mods_text(profile: &Profile, speed_mod: ScrollSpeedSetting) -> Arc<str> {
    let mut parts = vec![speed_mod.to_string()];
    if profile.mini_percent != 0 {
        parts.push(format!("{}% Mini", profile.mini_percent));
    }
    if profile.spacing_percent != 0 {
        parts.push(format!("{}% Spacing", profile.spacing_percent));
    }
    let scroll = profile.scroll_option;
    if scroll.contains(ScrollOption::Reverse) {
        parts.push("Reverse".to_string());
    }
    if scroll.contains(ScrollOption::Split) {
        parts.push("Split".to_string());
    }
    if scroll.contains(ScrollOption::Alternate) {
        parts.push("Alternate".to_string());
    }
    if scroll.contains(ScrollOption::Cross) {
        parts.push("Cross".to_string());
    }
    if scroll.contains(ScrollOption::Centered) {
        parts.push("Centered".to_string());
    }
    parts.push(profile.perspective.to_string());
    let disabled_windows = profile.timing_windows.disabled_windows();
    if disabled_windows.iter().any(|disabled| *disabled) {
        let windows = disabled_windows
            .iter()
            .enumerate()
            .filter_map(|(i, disabled)| disabled.then(|| format!("W{}", i + 1)))
            .collect::<Vec<_>>()
            .join("/");
        parts.push(format!("No {windows}"));
    }
    parts.push(profile.noteskin.to_string());
    Arc::<str>::from(parts.join(", "))
}

#[inline(always)]
pub fn resolve_tap_explosion_skin<'a>(
    noteskin: Option<&'a NoteSkin>,
    fallback: &'a NoteSkin,
) -> Option<&'a NoteSkin> {
    if tap_explosion_skin_hidden(noteskin) {
        None
    } else {
        Some(resolve_noteskin_choice(noteskin, fallback))
    }
}

fn normalize_graphic_key(
    raw: &str,
    folder: &str,
    stock_aliases: &[(&str, &str)],
) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("graphic setting was empty".to_string());
    }
    if trimmed.eq_ignore_ascii_case("none") {
        return Ok("None".to_string());
    }

    let basename = Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed)
        .trim();
    if basename.eq_ignore_ascii_case("none") {
        return Ok("None".to_string());
    }

    let normalized = basename.to_ascii_lowercase();
    if let Some((_, key)) = stock_aliases
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(&normalized))
    {
        return Ok((*key).to_string());
    }

    Ok(format!("{folder}/{basename}"))
}

/// Like [`normalize_graphic_key`] but **only** resolves names DeadSync actually
/// ships: a recognized stock alias or `"None"`. Returns `None` for unknown or
/// custom graphic names instead of fabricating a `folder/basename` path that
/// would point at a missing texture. Used by the ITGmania importer, where a
/// Simply Love profile may reference a theme graphic DeadSync doesn't have.
fn recognized_stock_key(raw: &str, stock_aliases: &[(&str, &str)]) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("none") {
        return Some("None".to_string());
    }
    let basename = Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed)
        .trim();
    if basename.eq_ignore_ascii_case("none") {
        return Some("None".to_string());
    }
    let normalized = basename.to_ascii_lowercase();
    stock_aliases
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(&normalized))
        .map(|(_, key)| (*key).to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldJudgmentGraphic(String);

impl HoldJudgmentGraphic {
    pub const DEFAULT_KEY: &'static str = "hold_judgements/Love 1x2 (doubleres).png";

    const STOCK_ALIASES: &'static [(&'static str, &'static str)] = &[
        ("love", Self::DEFAULT_KEY),
        ("love 1x2 (doubleres).png", Self::DEFAULT_KEY),
        (
            "hold_judgements/love 1x2 (doubleres).png",
            Self::DEFAULT_KEY,
        ),
        ("mute", "hold_judgements/mute 1x2 (doubleres).png"),
        (
            "mute 1x2 (doubleres).png",
            "hold_judgements/mute 1x2 (doubleres).png",
        ),
        (
            "hold_judgements/mute 1x2 (doubleres).png",
            "hold_judgements/mute 1x2 (doubleres).png",
        ),
        ("itg2", "hold_judgements/ITG2 1x2 (doubleres).png"),
        (
            "itg2 1x2 (doubleres).png",
            "hold_judgements/ITG2 1x2 (doubleres).png",
        ),
        (
            "hold_judgements/itg2 1x2 (doubleres).png",
            "hold_judgements/ITG2 1x2 (doubleres).png",
        ),
    ];

    #[inline(always)]
    pub fn new(raw: &str) -> Self {
        Self(
            normalize_graphic_key(raw, "hold_judgements", Self::STOCK_ALIASES)
                .unwrap_or_else(|_| Self::DEFAULT_KEY.to_string()),
        )
    }

    /// Parse `raw` only if it names a graphic DeadSync ships (a recognized stock
    /// alias or `"None"`), returning `None` for unknown/custom names. Use this
    /// when importing external settings to avoid pointing at a missing texture.
    #[inline(always)]
    pub fn from_stock_name(raw: &str) -> Option<Self> {
        recognized_stock_key(raw, Self::STOCK_ALIASES).map(Self)
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline(always)]
    pub fn is_none(&self) -> bool {
        self.0.eq_ignore_ascii_case("None")
    }

    #[inline(always)]
    pub fn texture_key(&self) -> Option<&str> {
        (!self.is_none()).then_some(self.as_str())
    }
}

impl Default for HoldJudgmentGraphic {
    fn default() -> Self {
        Self(Self::DEFAULT_KEY.to_string())
    }
}

impl FromStr for HoldJudgmentGraphic {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        normalize_graphic_key(s, "hold_judgements", Self::STOCK_ALIASES).map(Self)
    }
}

impl core::fmt::Display for HoldJudgmentGraphic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldMissGraphic(String);

impl HeldMissGraphic {
    pub const DEFAULT_KEY: &'static str = "None";

    const STOCK_ALIASES: &'static [(&'static str, &'static str)] = &[
        ("love", "held_miss/Love (doubleres).png"),
        ("love (doubleres).png", "held_miss/Love (doubleres).png"),
        (
            "held_miss/love (doubleres).png",
            "held_miss/Love (doubleres).png",
        ),
    ];

    #[inline(always)]
    pub fn new(raw: &str) -> Self {
        Self(
            normalize_graphic_key(raw, "held_miss", Self::STOCK_ALIASES)
                .unwrap_or_else(|_| Self::DEFAULT_KEY.to_string()),
        )
    }

    /// Parse `raw` only if it names a graphic DeadSync ships (a recognized stock
    /// alias or `"None"`), returning `None` for unknown/custom names. Use this
    /// when importing external settings to avoid pointing at a missing texture.
    #[inline(always)]
    pub fn from_stock_name(raw: &str) -> Option<Self> {
        recognized_stock_key(raw, Self::STOCK_ALIASES).map(Self)
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline(always)]
    pub fn is_none(&self) -> bool {
        self.0.eq_ignore_ascii_case("None")
    }

    #[inline(always)]
    pub fn texture_key(&self) -> Option<&str> {
        (!self.is_none()).then_some(self.as_str())
    }
}

impl Default for HeldMissGraphic {
    fn default() -> Self {
        Self(Self::DEFAULT_KEY.to_string())
    }
}

impl FromStr for HeldMissGraphic {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        normalize_graphic_key(s, "held_miss", Self::STOCK_ALIASES).map(Self)
    }
}

impl core::fmt::Display for HeldMissGraphic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgmentGraphic(String);

impl JudgmentGraphic {
    pub const DEFAULT_KEY: &'static str = "judgements/Love 2x7 (doubleres).png";

    const STOCK_ALIASES: &'static [(&'static str, &'static str)] = &[
        ("bebas", "judgements/Bebas 2x7 (doubleres).png"),
        (
            "bebas 2x7 (doubleres).png",
            "judgements/Bebas 2x7 (doubleres).png",
        ),
        (
            "judgements/bebas 2x7 (doubleres).png",
            "judgements/Bebas 2x7 (doubleres).png",
        ),
        ("censored", "judgements/Censored 1x7 (doubleres).png"),
        (
            "censored 1x7 (doubleres).png",
            "judgements/Censored 1x7 (doubleres).png",
        ),
        (
            "judgements/censored 1x7 (doubleres).png",
            "judgements/Censored 1x7 (doubleres).png",
        ),
        ("chromatic", "judgements/Chromatic 2x7 (doubleres).png"),
        (
            "chromatic 2x7 (doubleres).png",
            "judgements/Chromatic 2x7 (doubleres).png",
        ),
        (
            "judgements/chromatic 2x7 (doubleres).png",
            "judgements/Chromatic 2x7 (doubleres).png",
        ),
        ("code", "judgements/Code 2x7 (doubleres).png"),
        (
            "code 2x7 (doubleres).png",
            "judgements/Code 2x7 (doubleres).png",
        ),
        (
            "judgements/code 2x7 (doubleres).png",
            "judgements/Code 2x7 (doubleres).png",
        ),
        ("comic sans", "judgements/Comic Sans 2x7 (doubleres).png"),
        ("comicsans", "judgements/Comic Sans 2x7 (doubleres).png"),
        (
            "comic sans 2x7 (doubleres).png",
            "judgements/Comic Sans 2x7 (doubleres).png",
        ),
        (
            "judgements/comic sans 2x7 (doubleres).png",
            "judgements/Comic Sans 2x7 (doubleres).png",
        ),
        ("emoticon", "judgements/Emoticon 2x7 (doubleres).png"),
        (
            "emoticon 2x7 (doubleres).png",
            "judgements/Emoticon 2x7 (doubleres).png",
        ),
        (
            "judgements/emoticon 2x7 (doubleres).png",
            "judgements/Emoticon 2x7 (doubleres).png",
        ),
        ("focus", "judgements/Focus 2x7 (doubleres).png"),
        (
            "focus 2x7 (doubleres).png",
            "judgements/Focus 2x7 (doubleres).png",
        ),
        (
            "judgements/focus 2x7 (doubleres).png",
            "judgements/Focus 2x7 (doubleres).png",
        ),
        ("grammar", "judgements/Grammar 2x7 (doubleres).png"),
        (
            "grammar 2x7 (doubleres).png",
            "judgements/Grammar 2x7 (doubleres).png",
        ),
        (
            "judgements/grammar 2x7 (doubleres).png",
            "judgements/Grammar 2x7 (doubleres).png",
        ),
        (
            "groovenights",
            "judgements/GrooveNights 2x7 (doubleres).png",
        ),
        (
            "groove nights",
            "judgements/GrooveNights 2x7 (doubleres).png",
        ),
        (
            "groovenights 2x7 (doubleres).png",
            "judgements/GrooveNights 2x7 (doubleres).png",
        ),
        (
            "judgements/groovenights 2x7 (doubleres).png",
            "judgements/GrooveNights 2x7 (doubleres).png",
        ),
        ("itg2", "judgements/ITG2 2x7 (doubleres).png"),
        (
            "itg2 2x7 (doubleres).png",
            "judgements/ITG2 2x7 (doubleres).png",
        ),
        (
            "judgements/itg2 2x7 (doubleres).png",
            "judgements/ITG2 2x7 (doubleres).png",
        ),
        ("love", Self::DEFAULT_KEY),
        ("love 2x7 (doubleres).png", Self::DEFAULT_KEY),
        ("judgements/love 2x7 (doubleres).png", Self::DEFAULT_KEY),
        ("love chroma", "judgements/Love Chroma 2x7 (doubleres).png"),
        ("lovechroma", "judgements/Love Chroma 2x7 (doubleres).png"),
        (
            "love chroma 2x7 (doubleres).png",
            "judgements/Love Chroma 2x7 (doubleres).png",
        ),
        (
            "judgements/love chroma 2x7 (doubleres).png",
            "judgements/Love Chroma 2x7 (doubleres).png",
        ),
        ("miso", "judgements/Miso 2x7 (doubleres).png"),
        (
            "miso 2x7 (doubleres).png",
            "judgements/Miso 2x7 (doubleres).png",
        ),
        (
            "judgements/miso 2x7 (doubleres).png",
            "judgements/Miso 2x7 (doubleres).png",
        ),
        ("papyrus", "judgements/Papyrus 2x7 (doubleres).png"),
        (
            "papyrus 2x7 (doubleres).png",
            "judgements/Papyrus 2x7 (doubleres).png",
        ),
        (
            "judgements/papyrus 2x7 (doubleres).png",
            "judgements/Papyrus 2x7 (doubleres).png",
        ),
        (
            "rainbowmatic",
            "judgements/Rainbowmatic 2x7 (doubleres).png",
        ),
        (
            "rainbowmatic 2x7 (doubleres).png",
            "judgements/Rainbowmatic 2x7 (doubleres).png",
        ),
        (
            "judgements/rainbowmatic 2x7 (doubleres).png",
            "judgements/Rainbowmatic 2x7 (doubleres).png",
        ),
        ("roboto", "judgements/Roboto 2x7 (doubleres).png"),
        (
            "roboto 2x7 (doubleres).png",
            "judgements/Roboto 2x7 (doubleres).png",
        ),
        (
            "judgements/roboto 2x7 (doubleres).png",
            "judgements/Roboto 2x7 (doubleres).png",
        ),
        ("shift", "judgements/Shift 2x7 (doubleres).png"),
        (
            "shift 2x7 (doubleres).png",
            "judgements/Shift 2x7 (doubleres).png",
        ),
        (
            "judgements/shift 2x7 (doubleres).png",
            "judgements/Shift 2x7 (doubleres).png",
        ),
        ("tactics", "judgements/Tactics 2x7 (doubleres).png"),
        (
            "tactics 2x7 (doubleres).png",
            "judgements/Tactics 2x7 (doubleres).png",
        ),
        (
            "judgements/tactics 2x7 (doubleres).png",
            "judgements/Tactics 2x7 (doubleres).png",
        ),
        ("wendy", "judgements/Wendy 2x7 (doubleres).png"),
        (
            "wendy 2x7 (doubleres).png",
            "judgements/Wendy 2x7 (doubleres).png",
        ),
        (
            "judgements/wendy 2x7 (doubleres).png",
            "judgements/Wendy 2x7 (doubleres).png",
        ),
        (
            "wendy chroma",
            "judgements/Wendy Chroma 2x7 (doubleres).png",
        ),
        ("wendychroma", "judgements/Wendy Chroma 2x7 (doubleres).png"),
        (
            "wendy chroma 2x7 (doubleres).png",
            "judgements/Wendy Chroma 2x7 (doubleres).png",
        ),
        (
            "judgements/wendy chroma 2x7 (doubleres).png",
            "judgements/Wendy Chroma 2x7 (doubleres).png",
        ),
    ];

    #[inline(always)]
    pub fn new(raw: &str) -> Self {
        Self(
            normalize_graphic_key(raw, "judgements", Self::STOCK_ALIASES)
                .unwrap_or_else(|_| Self::DEFAULT_KEY.to_string()),
        )
    }

    /// Parse `raw` only if it names a graphic DeadSync ships (a recognized stock
    /// alias or `"None"`), returning `None` for unknown/custom names. Use this
    /// when importing external settings to avoid pointing at a missing texture.
    #[inline(always)]
    pub fn from_stock_name(raw: &str) -> Option<Self> {
        recognized_stock_key(raw, Self::STOCK_ALIASES).map(Self)
    }

    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline(always)]
    pub fn is_none(&self) -> bool {
        self.0.eq_ignore_ascii_case("None")
    }

    #[inline(always)]
    pub fn texture_key(&self) -> Option<&str> {
        (!self.is_none()).then_some(self.as_str())
    }
}

impl Default for JudgmentGraphic {
    fn default() -> Self {
        Self(Self::DEFAULT_KEY.to_string())
    }
}

impl FromStr for JudgmentGraphic {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        normalize_graphic_key(s, "judgements", Self::STOCK_ALIASES).map(Self)
    }
}

impl core::fmt::Display for JudgmentGraphic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct GameplayHudPlayerSnapshot {
    pub joined: bool,
    pub guest: bool,
    pub display_name: String,
    pub avatar_texture_key: Option<String>,
    pub hide_username: bool,
}

#[derive(Debug, Clone)]
pub struct GameplayHudSnapshot {
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub p1: GameplayHudPlayerSnapshot,
    pub p2: GameplayHudPlayerSnapshot,
}

pub struct LocalProfileSummary {
    pub id: String,
    pub display_name: String,
    pub avatar_path: Option<PathBuf>,
}

const PROFILE_STATS_VERSION_V1: u16 = 1;

#[derive(Debug, Clone, Copy, Encode, Decode)]
struct LegacyProfileStatsV1 {
    version: u16,
    current_combo: u32,
}

#[derive(Debug, Clone, Encode, Decode)]
struct ProfileStatsV1 {
    version: u16,
    current_combo: u32,
    known_pack_names: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileStats {
    pub current_combo: u32,
    pub known_pack_names: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileStatsDecodeError {
    UnsupportedVersion(u16),
    InvalidPayload,
}

#[derive(Debug)]
pub enum ProfileStatsLoadError {
    Read(std::io::Error),
    Decode(ProfileStatsDecodeError),
}

#[derive(Debug)]
pub enum ProfileStatsWriteError {
    Encode,
    CreateDir {
        path: PathBuf,
        error: std::io::Error,
    },
    WriteTmp {
        path: PathBuf,
        error: std::io::Error,
    },
    Rename {
        path: PathBuf,
        tmp_path: PathBuf,
        error: std::io::Error,
    },
}

pub fn decode_profile_stats(bytes: &[u8]) -> Result<ProfileStats, ProfileStatsDecodeError> {
    if let Ok((stats, _)) =
        bincode::decode_from_slice::<ProfileStatsV1, _>(bytes, bincode::config::standard())
    {
        if stats.version != PROFILE_STATS_VERSION_V1 {
            return Err(ProfileStatsDecodeError::UnsupportedVersion(stats.version));
        }
        return Ok(ProfileStats {
            current_combo: stats.current_combo,
            known_pack_names: stats.known_pack_names.into_iter().collect(),
        });
    }
    if let Ok((stats, _)) =
        bincode::decode_from_slice::<LegacyProfileStatsV1, _>(bytes, bincode::config::standard())
    {
        if stats.version != PROFILE_STATS_VERSION_V1 {
            return Err(ProfileStatsDecodeError::UnsupportedVersion(stats.version));
        }
        return Ok(ProfileStats {
            current_combo: stats.current_combo,
            known_pack_names: HashSet::new(),
        });
    }
    Err(ProfileStatsDecodeError::InvalidPayload)
}

pub fn encode_profile_stats(stats: &ProfileStats) -> Option<Vec<u8>> {
    let mut known_pack_names: Vec<String> = stats.known_pack_names.iter().cloned().collect();
    known_pack_names.sort_unstable();
    bincode::encode_to_vec(
        ProfileStatsV1 {
            version: PROFILE_STATS_VERSION_V1,
            current_combo: stats.current_combo,
            known_pack_names,
        },
        bincode::config::standard(),
    )
    .ok()
}

pub fn load_profile_stats_file(path: &Path) -> Result<Option<ProfileStats>, ProfileStatsLoadError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(ProfileStatsLoadError::Read(e)),
    };
    decode_profile_stats(&bytes)
        .map(Some)
        .map_err(ProfileStatsLoadError::Decode)
}

pub fn write_profile_stats_dir(
    dir: &Path,
    payload: &ProfileStats,
) -> Result<(), ProfileStatsWriteError> {
    let buf = encode_profile_stats(payload).ok_or(ProfileStatsWriteError::Encode)?;
    let path = profile_stats_path(dir);
    let tmp_path = profile_stats_tmp_path(dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| ProfileStatsWriteError::CreateDir {
            path: parent.to_path_buf(),
            error,
        })?;
    }
    fs::write(&tmp_path, buf).map_err(|error| ProfileStatsWriteError::WriteTmp {
        path: tmp_path.clone(),
        error,
    })?;
    fs::rename(&tmp_path, &path).map_err(|error| {
        let _ = fs::remove_file(&tmp_path);
        ProfileStatsWriteError::Rename {
            path,
            tmp_path,
            error,
        }
    })
}

pub fn write_imported_profile_stats_dir(
    dir: &Path,
    current_combo: u32,
) -> Result<(), ProfileStatsWriteError> {
    if current_combo == 0 {
        return Ok(());
    }
    write_profile_stats_dir(
        dir,
        &ProfileStats {
            current_combo,
            known_pack_names: HashSet::new(),
        },
    )
}

pub fn parse_favorites_content(text: &str) -> HashSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

pub fn render_favorites_content(favorites: &HashSet<String>) -> String {
    let mut sorted: Vec<&str> = favorites.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.join("\n")
}

fn save_set_file(path: &Path, text: String) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp_path = path.with_extension("tmp");
    if fs::write(&tmp_path, text.as_bytes()).is_ok() {
        let _ = fs::rename(&tmp_path, path);
    }
}

pub fn load_favorites_dir(dir: &Path) -> HashSet<String> {
    let Ok(text) = fs::read_to_string(favorites_path(dir)) else {
        return HashSet::new();
    };
    parse_favorites_content(&text)
}

pub fn save_favorites_dir(dir: &Path, favorites: &HashSet<String>) {
    save_set_file(
        favorites_path(dir).as_path(),
        render_favorites_content(favorites),
    );
}

pub fn merge_imported_favorites_dir(dir: &Path, hashes: &HashSet<String>) {
    if hashes.is_empty() {
        return;
    }
    let mut merged = load_favorites_dir(dir);
    merged.extend(hashes.iter().cloned());
    save_favorites_dir(dir, &merged);
}

pub fn toggle_favorite_hash(favorites: &mut HashSet<String>, chart_hash: &str) -> bool {
    if favorites.contains(chart_hash) {
        favorites.remove(chart_hash);
        false
    } else {
        favorites.insert(chart_hash.to_string());
        true
    }
}

pub fn parse_favorited_packs_content(text: &str) -> HashSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

pub fn render_favorited_packs_content(packs: &HashSet<String>) -> String {
    let mut sorted: Vec<&str> = packs.iter().map(String::as_str).collect();
    sorted.sort_unstable_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
    sorted.join("\n")
}

pub fn load_favorited_packs_dir(dir: &Path) -> HashSet<String> {
    let Ok(text) = fs::read_to_string(favorited_packs_path(dir)) else {
        return HashSet::new();
    };
    parse_favorited_packs_content(&text)
}

#[derive(Debug)]
pub struct ProfileSidecarLoadData {
    pub stats: ProfileStats,
    pub stats_error: Option<ProfileStatsLoadError>,
    pub favorites: HashSet<String>,
    pub favorited_packs: HashSet<String>,
    pub avatar_path: Option<PathBuf>,
}

pub fn load_profile_sidecars_dir(dir: &Path, default_profile: &Profile) -> ProfileSidecarLoadData {
    let (stats, stats_error) = match load_profile_stats_file(&profile_stats_path(dir)) {
        Ok(Some(stats)) => (stats, None),
        Ok(None) => (
            ProfileStats {
                current_combo: default_profile.current_combo,
                known_pack_names: HashSet::new(),
            },
            None,
        ),
        Err(error) => (
            ProfileStats {
                current_combo: default_profile.current_combo,
                known_pack_names: HashSet::new(),
            },
            Some(error),
        ),
    };

    ProfileSidecarLoadData {
        stats,
        stats_error,
        favorites: load_favorites_dir(dir),
        favorited_packs: load_favorited_packs_dir(dir),
        avatar_path: find_profile_avatar_path(dir),
    }
}

pub fn save_favorited_packs_dir(dir: &Path, packs: &HashSet<String>) {
    save_set_file(
        favorited_packs_path(dir).as_path(),
        render_favorited_packs_content(packs),
    );
}

pub fn toggle_favorited_pack(packs: &mut HashSet<String>, pack_name: &str) -> bool {
    let existing = packs.iter().find(|p| *p == pack_name).cloned();
    if let Some(existing) = existing {
        packs.remove(&existing);
        false
    } else {
        packs.insert(pack_name.to_string());
        true
    }
}

pub fn add_known_pack_names<'a>(
    known_pack_names: &mut HashSet<String>,
    pack_names: impl IntoIterator<Item = &'a str>,
) -> bool {
    let mut changed = false;
    for name in pack_names {
        changed |= known_pack_names.insert(name.to_owned());
    }
    changed
}

pub fn unknown_pack_names(
    known_pack_names: &HashSet<String>,
    scanned_pack_names: &[String],
) -> HashSet<String> {
    scanned_pack_names
        .iter()
        .filter(|name| !known_pack_names.contains(name.as_str()))
        .cloned()
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastPlayed {
    pub song_music_path: Option<String>,
    pub chart_hash: Option<String>,
    pub difficulty_index: usize,
}

pub fn append_last_played_section(content: &mut String, section: &str, last_played: &LastPlayed) {
    content.push_str(&format!("[{section}]\n"));
    if let Some(path) = &last_played.song_music_path {
        content.push_str(&format!("MusicPath={path}\n"));
    } else {
        content.push_str("MusicPath=\n");
    }
    if let Some(hash) = &last_played.chart_hash {
        content.push_str(&format!("ChartHash={hash}\n"));
    } else {
        content.push_str("ChartHash=\n");
    }
    content.push_str(&format!(
        "DifficultyIndex={}\n",
        last_played.difficulty_index
    ));
    content.push('\n');
}

pub fn load_last_played_section<F>(
    has_any: bool,
    mut get: F,
    default: &LastPlayed,
) -> Option<LastPlayed>
where
    F: FnMut(&str) -> Option<String>,
{
    if !has_any {
        return None;
    }

    Some(LastPlayed {
        song_music_path: parse_last_played_value(get("MusicPath").as_deref()),
        chart_hash: parse_last_played_value(get("ChartHash").as_deref()),
        difficulty_index: get("DifficultyIndex")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(default.difficulty_index),
    })
}

impl Default for LastPlayed {
    fn default() -> Self {
        Self {
            song_music_path: None,
            chart_hash: None,
            // Mirror FILE_DIFFICULTY_NAMES[2] ("Medium") as the default.
            difficulty_index: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LastPlayedCourse {
    pub course_path: Option<String>,
    pub difficulty_name: Option<String>,
}

pub fn append_last_played_course_section(
    content: &mut String,
    section: &str,
    last_played: &LastPlayedCourse,
) {
    content.push_str(&format!("[{section}]\n"));
    if let Some(path) = &last_played.course_path {
        content.push_str(&format!("CoursePath={path}\n"));
    } else {
        content.push_str("CoursePath=\n");
    }
    if let Some(name) = &last_played.difficulty_name {
        content.push_str(&format!("DifficultyName={name}\n"));
    } else {
        content.push_str("DifficultyName=\n");
    }
    content.push('\n');
}

pub fn load_last_played_course_section<F>(has_any: bool, mut get: F) -> Option<LastPlayedCourse>
where
    F: FnMut(&str) -> Option<String>,
{
    if !has_any {
        return None;
    }

    Some(LastPlayedCourse {
        course_path: parse_last_played_value(get("CoursePath").as_deref()),
        difficulty_name: parse_last_played_value(get("DifficultyName").as_deref()),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerOptionsData {
    pub background_filter: BackgroundFilter,
    pub hold_judgment_graphic: HoldJudgmentGraphic,
    pub held_miss_graphic: HeldMissGraphic,
    pub judgment_graphic: JudgmentGraphic,
    pub combo_font: ComboFont,
    pub combo_colors: ComboColors,
    pub combo_mode: ComboMode,
    pub carry_combo_between_songs: bool,
    pub noteskin: NoteSkin,
    pub mine_noteskin: Option<NoteSkin>,
    pub receptor_noteskin: Option<NoteSkin>,
    pub tap_explosion_noteskin: Option<NoteSkin>,
    pub tap_explosion_active_mask: TapExplosionMask,
    pub scroll_speed: ScrollSpeedSetting,
    pub no_cmod_alternative: NoCmodAlternative,
    pub scroll_option: ScrollOption,
    pub reverse_scroll: bool,
    pub turn_option: TurnOption,
    pub insert_active_mask: InsertMask,
    pub remove_active_mask: RemoveMask,
    pub holds_active_mask: HoldsMask,
    pub accel_effects_active_mask: AccelEffectsMask,
    pub visual_effects_active_mask: VisualEffectsMask,
    pub appearance_effects_active_mask: AppearanceEffectsMask,
    pub attack_mode: AttackMode,
    pub hide_light_type: HideLightType,
    pub rescore_early_hits: bool,
    pub hide_early_dw_judgments: bool,
    pub hide_early_dw_flash: bool,
    pub hide_early_dw_column_flash: bool,
    pub timing_windows: TimingWindowsOption,
    pub show_fa_plus_window: bool,
    pub show_ex_score: bool,
    pub show_hard_ex_score: bool,
    pub show_fa_plus_pane: bool,
    pub fa_plus_10ms_blue_window: bool,
    pub split_15_10ms: bool,
    pub track_early_judgments: bool,
    pub scale_scatterplot: bool,
    pub scatterplot_max_window: ScatterplotMaxWindow,
    pub score_position: ScorePosition,
    pub score_display_mode: ScoreDisplayMode,
    pub custom_fantastic_window: bool,
    pub custom_fantastic_window_ms: u8,
    /// Pad-light brightness 0..=100 (gamma-mapped on send; 0 = off, 100 = full).
    pub pad_light_brightness: u8,
    pub judgment_tilt: bool,
    pub column_cues: bool,
    pub measure_cues: bool,
    /// Crossover Cues: flash a column before an upcoming crossover step.
    pub crossover_cues: bool,
    /// Lead time of a crossover cue in milliseconds (`[500, 1500]`, 100 ms grid).
    pub crossover_cue_duration_ms: u16,
    /// Quantization for the crossover-cue spacing threshold (`4 / Quantization`).
    pub crossover_cue_quantization: u8,
    /// Include crossover brackets (multi-foot crossover steps) in crossover
    /// cues. Off by default, since crossover brackets are commonly
    /// unintended or misidentified.
    pub crossover_cue_brackets: bool,
    /// Show the break-length countdown number on long crossover cues (>= 5 s).
    pub column_countdown: bool,
    pub judgment_back: bool,
    pub error_ms_display: bool,
    pub display_scorebox: bool,
    pub live_timing_stats: bool,
    pub live_timing_stats_mask: LiveTimingStatsMask,
    pub rainbow_max: bool,
    pub responsive_colors: bool,
    pub show_life_percent: bool,
    pub tilt_multiplier: f32,
    pub tilt_min_threshold_ms: u32,
    pub tilt_max_threshold_ms: u32,
    pub error_bar_active_mask: ErrorBarMask,
    pub error_bar: ErrorBarStyle,
    pub error_bar_text: bool,
    pub text_error_bar_scalable: bool,
    pub text_error_bar_threshold_ms: u32,
    pub error_bar_up: bool,
    pub error_bar_multi_tick: bool,
    pub error_bar_trim: ErrorBarTrim,
    pub center_tick: bool,
    pub short_average_error_bar_enabled: bool,
    pub average_error_bar_intensity: f32,
    pub average_error_bar_interval_ms: u32,
    pub long_error_bar_enabled: bool,
    pub long_error_bar_intensity: f32,
    pub long_error_bar_threshold_ms: u32,
    pub long_error_bar_min_samples: u32,
    pub step_statistics: StepStatisticsMask,
    pub step_stats_extra: StepStatsExtra,
    pub target_score: TargetScoreSetting,
    pub lifemeter_type: LifeMeterType,
    pub measure_counter: MeasureCounter,
    pub measure_counter_lookahead: u8,
    pub measure_counter_left: bool,
    pub measure_counter_up: bool,
    pub measure_counter_vert: bool,
    pub broken_run: bool,
    pub run_timer: bool,
    pub measure_lines: MeasureLines,
    pub hide_targets: bool,
    pub hide_song_bg: bool,
    pub hide_combo: bool,
    pub hide_lifebar: bool,
    pub hide_score: bool,
    pub hide_danger: bool,
    pub hide_combo_explosions: bool,
    pub hide_username: bool,
    pub column_flash_on_miss: bool,
    pub column_flash_mask: ColumnFlashMask,
    pub column_flash_brightness: ColumnFlashBrightness,
    pub column_flash_size: ColumnFlashSize,
    pub subtractive_scoring: bool,
    pub pacemaker: bool,
    pub nps_graph_at_top: bool,
    pub transparent_density_graph_bg: bool,
    pub smx_fsr_display: bool,
    pub smx_pad_input_display: bool,
    pub smx_bg_pack: Option<String>,
    pub smx_judge_pack: Option<String>,
    pub mini_indicator: MiniIndicator,
    pub mini_indicator_score_type: MiniIndicatorScoreType,
    pub mini_indicator_subtractive_display: MiniIndicatorSubtractiveDisplay,
    pub mini_indicator_size: MiniIndicatorSize,
    pub mini_indicator_color: MiniIndicatorColor,
    pub mini_indicator_position: MiniIndicatorPosition,
    pub mini_percent: i32,
    pub spacing_percent: i32,
    pub perspective: Perspective,
    pub note_field_offset_x: i32,
    pub note_field_offset_y: i32,
    pub judgment_offset_x: i32,
    pub judgment_offset_y: i32,
    pub combo_offset_x: i32,
    pub combo_offset_y: i32,
    pub error_bar_offset_x: i32,
    pub error_bar_offset_y: i32,
    pub visual_delay_ms: i32,
    pub global_offset_shift_ms: i32,
}

fn default_player_options() -> PlayerOptionsData {
    PlayerOptionsData {
        background_filter: BackgroundFilter::default(),
        hold_judgment_graphic: HoldJudgmentGraphic::default(),
        held_miss_graphic: HeldMissGraphic::default(),
        judgment_graphic: JudgmentGraphic::default(),
        combo_font: ComboFont::default(),
        combo_colors: ComboColors::default(),
        combo_mode: ComboMode::default(),
        carry_combo_between_songs: true,
        noteskin: NoteSkin::default(),
        mine_noteskin: None,
        receptor_noteskin: None,
        tap_explosion_noteskin: None,
        tap_explosion_active_mask: TapExplosionMask::all(),
        scroll_speed: ScrollSpeedSetting::default(),
        no_cmod_alternative: NoCmodAlternative::default(),
        scroll_option: ScrollOption::default(),
        reverse_scroll: false,
        turn_option: TurnOption::default(),
        insert_active_mask: InsertMask::empty(),
        remove_active_mask: RemoveMask::empty(),
        holds_active_mask: HoldsMask::empty(),
        accel_effects_active_mask: AccelEffectsMask::empty(),
        visual_effects_active_mask: VisualEffectsMask::empty(),
        appearance_effects_active_mask: AppearanceEffectsMask::empty(),
        attack_mode: AttackMode::default(),
        hide_light_type: HideLightType::default(),
        rescore_early_hits: true,
        hide_early_dw_judgments: false,
        hide_early_dw_flash: false,
        hide_early_dw_column_flash: false,
        timing_windows: TimingWindowsOption::default(),
        show_fa_plus_window: false,
        show_ex_score: false,
        show_hard_ex_score: false,
        show_fa_plus_pane: false,
        fa_plus_10ms_blue_window: false,
        split_15_10ms: false,
        track_early_judgments: false,
        scale_scatterplot: false,
        scatterplot_max_window: ScatterplotMaxWindow::Off,
        score_position: ScorePosition::Normal,
        score_display_mode: ScoreDisplayMode::Normal,
        custom_fantastic_window: false,
        custom_fantastic_window_ms: CUSTOM_FANTASTIC_WINDOW_DEFAULT_MS,
        pad_light_brightness: PAD_LIGHT_BRIGHTNESS_DEFAULT,
        judgment_tilt: false,
        column_cues: false,
        measure_cues: false,
        crossover_cues: false,
        crossover_cue_duration_ms: CROSSOVER_CUE_DURATION_DEFAULT_MS,
        crossover_cue_quantization: CROSSOVER_CUE_QUANTIZATION_DEFAULT,
        crossover_cue_brackets: false,
        column_countdown: false,
        judgment_back: false,
        error_ms_display: false,
        display_scorebox: true,
        live_timing_stats: false,
        live_timing_stats_mask: LiveTimingStatsMask::empty(),
        rainbow_max: false,
        responsive_colors: false,
        show_life_percent: false,
        tilt_multiplier: 1.0,
        tilt_min_threshold_ms: TILT_MIN_THRESHOLD_DEFAULT_MS,
        tilt_max_threshold_ms: TILT_MAX_THRESHOLD_DEFAULT_MS,
        error_bar_active_mask: error_bar_mask_from_style(ErrorBarStyle::default(), false),
        error_bar: ErrorBarStyle::default(),
        error_bar_text: false,
        text_error_bar_scalable: false,
        text_error_bar_threshold_ms: TEXT_ERROR_BAR_THRESHOLD_MS_DEFAULT,
        error_bar_up: false,
        error_bar_multi_tick: false,
        error_bar_trim: ErrorBarTrim::default(),
        center_tick: false,
        short_average_error_bar_enabled: true,
        average_error_bar_intensity: AVERAGE_ERROR_BAR_INTENSITY_DEFAULT,
        average_error_bar_interval_ms: AVERAGE_ERROR_BAR_INTERVAL_MS_DEFAULT,
        long_error_bar_enabled: true,
        long_error_bar_intensity: LONG_ERROR_BAR_INTENSITY_DEFAULT,
        long_error_bar_threshold_ms: LONG_ERROR_BAR_THRESHOLD_MS_DEFAULT,
        long_error_bar_min_samples: LONG_ERROR_BAR_MIN_SAMPLES_DEFAULT,
        step_statistics: StepStatisticsMask::default(),
        step_stats_extra: StepStatsExtra::default(),
        target_score: TargetScoreSetting::default(),
        lifemeter_type: LifeMeterType::default(),
        measure_counter: MeasureCounter::default(),
        measure_counter_lookahead: 2,
        measure_counter_left: true,
        measure_counter_up: false,
        measure_counter_vert: false,
        broken_run: false,
        run_timer: false,
        measure_lines: MeasureLines::default(),
        hide_targets: false,
        hide_song_bg: false,
        hide_combo: false,
        hide_lifebar: false,
        hide_score: false,
        hide_danger: false,
        hide_combo_explosions: false,
        hide_username: false,
        column_flash_on_miss: false,
        column_flash_mask: DEFAULT_COLUMN_FLASH_MASK,
        column_flash_brightness: ColumnFlashBrightness::Normal,
        column_flash_size: ColumnFlashSize::Default,
        subtractive_scoring: false,
        pacemaker: false,
        nps_graph_at_top: false,
        transparent_density_graph_bg: false,
        smx_fsr_display: false,
        smx_pad_input_display: false,
        smx_bg_pack: None,
        smx_judge_pack: None,
        mini_indicator: MiniIndicator::None,
        mini_indicator_score_type: MiniIndicatorScoreType::Itg,
        mini_indicator_subtractive_display: MiniIndicatorSubtractiveDisplay::Percent,
        mini_indicator_size: MiniIndicatorSize::Default,
        mini_indicator_color: MiniIndicatorColor::Default,
        mini_indicator_position: MiniIndicatorPosition::Default,
        mini_percent: 0,
        spacing_percent: 0,
        perspective: Perspective::default(),
        note_field_offset_x: 0,
        note_field_offset_y: 0,
        judgment_offset_x: 0,
        judgment_offset_y: 0,
        combo_offset_x: 0,
        combo_offset_y: 0,
        error_bar_offset_x: 0,
        error_bar_offset_y: 0,
        visual_delay_ms: 0,
        global_offset_shift_ms: 0,
    }
}

impl Default for PlayerOptionsData {
    fn default() -> Self {
        default_player_options()
    }
}

#[inline(always)]
fn load_u8_bool<F>(get: &mut F, key: &str, default: bool) -> bool
where
    F: FnMut(&str) -> Option<String>,
{
    get(key)
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(default, |v| v != 0)
}

pub fn load_visual_player_options<F>(options: &mut PlayerOptionsData, mut get: F)
where
    F: FnMut(&str) -> Option<String>,
{
    options.background_filter = get("BackgroundFilter")
        .and_then(|s| BackgroundFilter::from_str(&s).ok())
        .unwrap_or(options.background_filter);
    options.hold_judgment_graphic = get("HoldJudgmentGraphic")
        .and_then(|s| HoldJudgmentGraphic::from_str(&s).ok())
        .unwrap_or_else(|| options.hold_judgment_graphic.clone());
    options.held_miss_graphic = get("HeldGraphic")
        .or_else(|| get("HeldMissGraphic"))
        .and_then(|s| HeldMissGraphic::from_str(&s).ok())
        .unwrap_or_else(|| options.held_miss_graphic.clone());
    options.judgment_graphic = get("JudgmentGraphic")
        .and_then(|s| JudgmentGraphic::from_str(&s).ok())
        .unwrap_or_else(|| options.judgment_graphic.clone());
    options.combo_font = get("ComboFont")
        .and_then(|s| ComboFont::from_str(&s).ok())
        .unwrap_or(options.combo_font);
    options.combo_colors = get("ComboColors")
        .and_then(|s| ComboColors::from_str(&s).ok())
        .unwrap_or(options.combo_colors);
    options.combo_mode = get("ComboMode")
        .and_then(|s| ComboMode::from_str(&s).ok())
        .unwrap_or(options.combo_mode);
    options.carry_combo_between_songs = get("CarryComboBetweenSongs")
        .or_else(|| get("ComboContinuesBetweenSongs"))
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.carry_combo_between_songs, |v| v != 0);
    options.noteskin = get("NoteSkin")
        .and_then(|s| NoteSkin::from_str(&s).ok())
        .unwrap_or_else(|| options.noteskin.clone());
    options.mine_noteskin = get("MineSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.receptor_noteskin = get("ReceptorSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.tap_explosion_noteskin =
        get("TapExplosionSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    let tap_explosion_mask_version = get("TapExplosionMaskVersion")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(1);
    options.tap_explosion_active_mask = get("TapExplosionMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|bits| normalize_tap_explosion_mask(bits, tap_explosion_mask_version))
        .unwrap_or(options.tap_explosion_active_mask);
    options.mini_percent = get("MiniPercent")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.mini_percent);
    options.spacing_percent = get("Spacing")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.spacing_percent);
    options.perspective = get("Perspective")
        .and_then(|s| Perspective::from_str(&s).ok())
        .unwrap_or(options.perspective);
    options.note_field_offset_x = get("NoteFieldOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.note_field_offset_x);
    options.note_field_offset_y = get("NoteFieldOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.note_field_offset_y);
    options.judgment_offset_x = get("JudgmentOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.judgment_offset_x);
    options.judgment_offset_y = get("JudgmentOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.judgment_offset_y);
    options.combo_offset_x = get("ComboOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.combo_offset_x);
    options.combo_offset_y = get("ComboOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.combo_offset_y);
    options.error_bar_offset_x = get("ErrorBarOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.error_bar_offset_x);
    options.error_bar_offset_y = get("ErrorBarOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.error_bar_offset_y);
    options.visual_delay_ms = get("VisualDelayMs")
        .or_else(|| get("VisualDelay"))
        .and_then(|s| s.trim_end_matches("ms").parse::<i32>().ok())
        .unwrap_or(options.visual_delay_ms);
    options.global_offset_shift_ms = get("GlobalOffsetShiftMs")
        .and_then(|s| s.trim_end_matches("ms").parse::<i32>().ok())
        .unwrap_or(options.global_offset_shift_ms);
}

pub fn load_timing_feedback_options<F>(options: &mut PlayerOptionsData, mut get: F)
where
    F: FnMut(&str) -> Option<String>,
{
    options.show_fa_plus_window =
        load_u8_bool(&mut get, "ShowFaPlusWindow", options.show_fa_plus_window);
    options.show_ex_score = load_u8_bool(&mut get, "ShowExScore", options.show_ex_score);
    options.show_hard_ex_score =
        load_u8_bool(&mut get, "ShowHardEXScore", options.show_hard_ex_score);
    options.show_fa_plus_pane = load_u8_bool(&mut get, "ShowFaPlusPane", options.show_fa_plus_pane);
    options.fa_plus_10ms_blue_window =
        load_u8_bool(&mut get, "SmallerWhite", options.fa_plus_10ms_blue_window);
    options.split_15_10ms = get("SplitWhites")
        .or_else(|| get("Split1510ms"))
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.split_15_10ms, |v| v != 0);
    options.track_early_judgments = load_u8_bool(
        &mut get,
        "TrackEarlyJudgments",
        options.track_early_judgments,
    );
    options.scale_scatterplot = get("ScaleScatterplot")
        .or_else(|| get("ScatterplotGreatMax"))
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.scale_scatterplot, |v| v != 0);
    options.scatterplot_max_window = get("ScatterplotMaxWindow")
        .and_then(|s| ScatterplotMaxWindow::from_str(&s).ok())
        .unwrap_or(options.scatterplot_max_window);
    options.score_position = get("ScorePosition")
        .and_then(|s| ScorePosition::from_str(&s).ok())
        .unwrap_or(options.score_position);
    options.score_display_mode = get("ScoreDisplay")
        .or_else(|| get("ScoreDisplayMode"))
        .and_then(|s| ScoreDisplayMode::from_str(&s).ok())
        .unwrap_or(options.score_display_mode);
    options.custom_fantastic_window = load_u8_bool(
        &mut get,
        "CustomFantasticWindow",
        options.custom_fantastic_window,
    );
    options.custom_fantastic_window_ms = get("CustomFantasticWindowMs")
        .and_then(|s| s.parse::<u8>().ok())
        .map(clamp_custom_fantastic_window_ms)
        .unwrap_or(options.custom_fantastic_window_ms);
    options.pad_light_brightness = get("PadLightBrightness")
        .and_then(|s| s.parse::<u8>().ok())
        .map(clamp_pad_light_brightness)
        .unwrap_or(options.pad_light_brightness);
    options.judgment_tilt = load_u8_bool(&mut get, "JudgmentTilt", options.judgment_tilt);
    options.column_cues = load_u8_bool(&mut get, "ColumnCues", options.column_cues);
    options.measure_cues = load_u8_bool(&mut get, "MeasureCues", options.measure_cues);
    options.crossover_cues = load_u8_bool(&mut get, "CrossoverCues", options.crossover_cues);
    options.crossover_cue_duration_ms = get("CrossoverCueDuration")
        .and_then(|s| s.trim().trim_end_matches("ms").parse::<u16>().ok())
        .map(clamp_crossover_cue_duration_ms)
        .unwrap_or(options.crossover_cue_duration_ms);
    options.crossover_cue_quantization = get("CrossoverCueQuantization")
        .and_then(|s| s.trim().parse::<u8>().ok())
        .map(clamp_crossover_cue_quantization)
        .unwrap_or(options.crossover_cue_quantization);
    options.crossover_cue_brackets = load_u8_bool(
        &mut get,
        "CrossoverCueBrackets",
        options.crossover_cue_brackets,
    );
    options.column_countdown = load_u8_bool(&mut get, "ColumnCountdown", options.column_countdown);
    options.judgment_back = load_u8_bool(&mut get, "JudgmentBack", options.judgment_back);
    options.error_ms_display = load_u8_bool(&mut get, "ErrorMSDisplay", options.error_ms_display);
    options.display_scorebox = load_u8_bool(&mut get, "DisplayScorebox", options.display_scorebox);
    let legacy_live_timing_stats =
        load_u8_bool(&mut get, "LiveTimingStats", options.live_timing_stats);
    if let Some(mask) = get("LiveTimingStatsMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(LiveTimingStatsMask::from_bits_truncate)
    {
        options.live_timing_stats_mask = mask;
        options.live_timing_stats = legacy_live_timing_stats;
    } else {
        options.live_timing_stats = legacy_live_timing_stats;
        if legacy_live_timing_stats {
            options.live_timing_stats_mask = LiveTimingStatsMask::all();
        }
    }
    options.rainbow_max = load_u8_bool(&mut get, "RainbowMax", options.rainbow_max);
    options.responsive_colors =
        load_u8_bool(&mut get, "ResponsiveColors", options.responsive_colors);
    options.show_life_percent =
        load_u8_bool(&mut get, "ShowLifePercent", options.show_life_percent);
    options.tilt_multiplier = get("TiltMultiplier")
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(options.tilt_multiplier);
    options.tilt_min_threshold_ms = get("TiltMinThresholdMs")
        .or_else(|| get("TiltCutoffMs"))
        .and_then(|s| s.trim().trim_end_matches("ms").trim().parse::<u32>().ok())
        .map(clamp_tilt_threshold_ms)
        .unwrap_or(options.tilt_min_threshold_ms);
    options.tilt_max_threshold_ms = get("TiltMaxThresholdMs")
        .and_then(|s| s.trim().trim_end_matches("ms").trim().parse::<u32>().ok())
        .map(clamp_tilt_threshold_ms)
        .unwrap_or(options.tilt_max_threshold_ms);
    if options.tilt_max_threshold_ms < options.tilt_min_threshold_ms {
        options.tilt_max_threshold_ms = options.tilt_min_threshold_ms;
    }
}

pub fn load_error_bar_options<F>(options: &mut PlayerOptionsData, mut get: F)
where
    F: FnMut(&str) -> Option<String>,
{
    options.error_bar = get("ErrorBar")
        .and_then(|s| ErrorBarStyle::from_str(&s).ok())
        .unwrap_or(options.error_bar);
    options.error_bar_text = get("ErrorBarText")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.error_bar_text, |v| v != 0);
    options.text_error_bar_scalable = get("TextErrorBarScalable")
        .and_then(|s| parse_profile_bool(&s))
        .or_else(|| get("TextErrorBar10ms").and_then(|s| parse_profile_bool(&s)))
        .unwrap_or(options.text_error_bar_scalable);
    options.text_error_bar_threshold_ms = get("TextErrorBarThresholdMs")
        .and_then(|s| s.trim().trim_end_matches("ms").trim().parse::<u32>().ok())
        .map(clamp_text_error_bar_threshold_ms)
        .unwrap_or(options.text_error_bar_threshold_ms);
    let mask_from_key = get("ErrorBarMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(ErrorBarMask::from_bits_truncate);
    let colorful = get("Colorful")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let monochrome = get("Monochrome")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let text = get("Text")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let highlight = get("Highlight")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let average = get("Average")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v != 0);
    let mask_from_flags = if colorful.is_some()
        || monochrome.is_some()
        || text.is_some()
        || highlight.is_some()
        || average.is_some()
    {
        let mut mask = ErrorBarMask::empty();
        if colorful.unwrap_or(false) {
            mask |= ErrorBarMask::COLORFUL;
        }
        if monochrome.unwrap_or(false) {
            mask |= ErrorBarMask::MONOCHROME;
        }
        if text.unwrap_or(false) {
            mask |= ErrorBarMask::TEXT;
        }
        if highlight.unwrap_or(false) {
            mask |= ErrorBarMask::HIGHLIGHT;
        }
        if average.unwrap_or(false) {
            mask |= ErrorBarMask::AVERAGE;
        }
        Some(mask)
    } else {
        None
    };
    options.error_bar_active_mask = mask_from_key
        .or(mask_from_flags)
        .unwrap_or_else(|| error_bar_mask_from_style(options.error_bar, options.error_bar_text));
    options.error_bar = error_bar_style_from_mask(options.error_bar_active_mask);
    options.error_bar_text = error_bar_text_from_mask(options.error_bar_active_mask);
    options.error_bar_up = get("ErrorBarUp")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.error_bar_up, |v| v != 0);
    options.error_bar_multi_tick = get("ErrorBarMultiTick")
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.error_bar_multi_tick, |v| v != 0);
    options.error_bar_trim = get("ErrorBarTrim")
        .and_then(|s| ErrorBarTrim::from_str(&s).ok())
        .unwrap_or(options.error_bar_trim);
    options.center_tick = get("CenterTick")
        .and_then(|s| parse_profile_bool(&s))
        .unwrap_or(options.center_tick);
    options.short_average_error_bar_enabled = get("ShortAverageErrorBar")
        .and_then(|s| parse_profile_bool(&s))
        .or_else(|| {
            get("LongAvgTickOnly")
                .and_then(|s| parse_profile_bool(&s))
                .map(|long_only| !long_only)
        })
        .unwrap_or(options.short_average_error_bar_enabled);
    options.average_error_bar_intensity = get("AverageErrorBarIntensity")
        .or_else(|| get("HighlightZoom"))
        .and_then(|s| s.trim().trim_end_matches('x').trim().parse::<f32>().ok())
        .map(clamp_average_error_bar_intensity)
        .unwrap_or(options.average_error_bar_intensity);
    options.average_error_bar_interval_ms = get("AverageErrorBarIntervalMs")
        .and_then(|s| s.trim().trim_end_matches("ms").trim().parse::<u32>().ok())
        .map(clamp_average_error_bar_interval_ms)
        .or_else(|| {
            get("HighlightAverageMs")
                .and_then(|s| s.trim().trim_end_matches("ms").trim().parse::<u32>().ok())
                .filter(|&ms| ms > 0)
                .map(clamp_average_error_bar_interval_ms)
        })
        .unwrap_or(options.average_error_bar_interval_ms);
    options.long_error_bar_enabled = get("LongErrorBar")
        .and_then(|s| s.trim().parse::<i32>().ok())
        .map_or(options.long_error_bar_enabled, |v| v != 0);
    options.long_error_bar_intensity = get("LongErrorBarIntensity")
        .and_then(|s| s.trim().trim_end_matches('x').trim().parse::<f32>().ok())
        .map(clamp_long_error_bar_intensity)
        .unwrap_or(options.long_error_bar_intensity);
    options.long_error_bar_threshold_ms = get("LongErrorBarThresholdMs")
        .and_then(|s| s.trim().trim_end_matches("ms").trim().parse::<u32>().ok())
        .map(clamp_long_error_bar_threshold_ms)
        .unwrap_or(options.long_error_bar_threshold_ms);
    options.long_error_bar_min_samples = get("LongErrorBarMinSamples")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .map(clamp_long_error_bar_min_samples)
        .unwrap_or(options.long_error_bar_min_samples);
}

pub fn load_player_options_section<F>(
    has_any: bool,
    mut get: F,
    default: &PlayerOptionsData,
) -> Option<PlayerOptionsData>
where
    F: FnMut(&str) -> Option<String>,
{
    if !has_any {
        return None;
    }

    let mut options = default.clone();
    load_visual_player_options(&mut options, &mut get);
    load_timing_feedback_options(&mut options, &mut get);
    load_error_bar_options(&mut options, &mut get);
    if let Some(step_statistics) =
        get("StepStatistics").and_then(|s| StepStatisticsMask::from_str(&s).ok())
    {
        options.step_statistics = step_statistics;
    } else if let Some(step_statistics) =
        get("DataVisualizations").and_then(|s| StepStatisticsMask::from_str(&s).ok())
    {
        options.step_statistics = step_statistics;
    }
    options.step_stats_extra = get("StepStatsExtra")
        .and_then(|s| StepStatsExtra::from_str(&s).ok())
        .unwrap_or(options.step_stats_extra);
    options.target_score = get("TargetScore")
        .and_then(|s| TargetScoreSetting::from_str(&s).ok())
        .unwrap_or(options.target_score);
    options.lifemeter_type = get("LifeMeterType")
        .and_then(|s| LifeMeterType::from_str(&s).ok())
        .unwrap_or(options.lifemeter_type);
    options.measure_counter = get("MeasureCounter")
        .and_then(|s| MeasureCounter::from_str(&s).ok())
        .unwrap_or(options.measure_counter);
    options.measure_counter_lookahead = get("MeasureCounterLookahead")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|v| v.min(4))
        .unwrap_or(options.measure_counter_lookahead);
    options.measure_counter_left =
        load_u8_bool(&mut get, "MeasureCounterLeft", options.measure_counter_left);
    options.measure_counter_up =
        load_u8_bool(&mut get, "MeasureCounterUp", options.measure_counter_up);
    options.measure_counter_vert =
        load_u8_bool(&mut get, "MeasureCounterVert", options.measure_counter_vert);
    options.broken_run = load_u8_bool(&mut get, "BrokenRun", options.broken_run);
    options.run_timer = load_u8_bool(&mut get, "RunTimer", options.run_timer);
    options.measure_lines = get("MeasureLines")
        .and_then(|s| MeasureLines::from_str(&s).ok())
        .unwrap_or(options.measure_lines);
    options.scroll_speed = get("ScrollSpeed")
        .and_then(|s| ScrollSpeedSetting::from_str(&s).ok())
        .unwrap_or(options.scroll_speed);
    options.no_cmod_alternative = get("NoCmodAlternative")
        .and_then(|s| NoCmodAlternative::from_str(&s).ok())
        .unwrap_or(options.no_cmod_alternative);
    options.turn_option = get("Turn")
        .and_then(|s| TurnOption::from_str(&s).ok())
        .unwrap_or(options.turn_option);
    options.insert_active_mask = get("InsertMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(InsertMask::from_bits_truncate)
        .unwrap_or(options.insert_active_mask);
    options.remove_active_mask = get("RemoveMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(RemoveMask::from_bits_truncate)
        .unwrap_or(options.remove_active_mask);
    options.holds_active_mask = get("HoldsMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(HoldsMask::from_bits_truncate)
        .unwrap_or(options.holds_active_mask);
    options.accel_effects_active_mask = get("AccelEffectsMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(AccelEffectsMask::from_bits_truncate)
        .unwrap_or(options.accel_effects_active_mask);
    options.visual_effects_active_mask = get("VisualEffectsMask")
        .and_then(|s| s.parse::<u16>().ok())
        .map(VisualEffectsMask::from_bits_truncate)
        .unwrap_or(options.visual_effects_active_mask);
    options.appearance_effects_active_mask = get("AppearanceEffectsMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(AppearanceEffectsMask::from_bits_truncate)
        .unwrap_or(options.appearance_effects_active_mask);
    options.attack_mode = get("AttackMode")
        .or_else(|| get("Attacks"))
        .and_then(|s| AttackMode::from_str(&s).ok())
        .unwrap_or(options.attack_mode);
    options.hide_light_type = get("HideLightType")
        .and_then(|s| HideLightType::from_str(&s).ok())
        .unwrap_or(options.hide_light_type);
    options.rescore_early_hits =
        load_u8_bool(&mut get, "RescoreEarlyHits", options.rescore_early_hits);
    options.hide_early_dw_judgments = load_u8_bool(
        &mut get,
        "HideEarlyDecentWayOffJudgments",
        options.hide_early_dw_judgments,
    );
    options.hide_early_dw_flash = load_u8_bool(
        &mut get,
        "HideEarlyDecentWayOffFlash",
        options.hide_early_dw_flash,
    );
    options.hide_early_dw_column_flash = load_u8_bool(
        &mut get,
        "HideEarlyDecentWayOffColumnFlash",
        options.hide_early_dw_column_flash,
    );
    options.timing_windows = get("TimingWindows")
        .and_then(|s| TimingWindowsOption::from_str(&s).ok())
        .unwrap_or(options.timing_windows);
    options.hide_targets = load_u8_bool(&mut get, "HideTargets", options.hide_targets);
    options.hide_song_bg = load_u8_bool(&mut get, "HideSongBG", options.hide_song_bg);
    options.hide_combo = load_u8_bool(&mut get, "HideCombo", options.hide_combo);
    options.hide_lifebar = load_u8_bool(&mut get, "HideLifebar", options.hide_lifebar);
    options.hide_score = load_u8_bool(&mut get, "HideScore", options.hide_score);
    options.hide_danger = load_u8_bool(&mut get, "HideDanger", options.hide_danger);
    options.hide_combo_explosions = load_u8_bool(
        &mut get,
        "HideComboExplosions",
        options.hide_combo_explosions,
    );
    options.hide_username = load_u8_bool(&mut get, "HideUsername", options.hide_username);
    options.column_flash_on_miss =
        load_u8_bool(&mut get, "ColumnFlashOnMiss", options.column_flash_on_miss);
    options.column_flash_mask = get("ColumnFlashMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(ColumnFlashMask::from_bits_truncate)
        .unwrap_or(options.column_flash_mask);
    options.column_flash_brightness = get("ColumnFlashBrightness")
        .and_then(|s| ColumnFlashBrightness::from_str(&s).ok())
        .unwrap_or(options.column_flash_brightness);
    options.column_flash_size = get("ColumnFlashSize")
        .and_then(|s| ColumnFlashSize::from_str(&s).ok())
        .unwrap_or(options.column_flash_size);
    options.subtractive_scoring =
        load_u8_bool(&mut get, "SubtractiveScoring", options.subtractive_scoring);
    options.pacemaker = load_u8_bool(&mut get, "Pacemaker", options.pacemaker);
    options.nps_graph_at_top = load_u8_bool(&mut get, "NPSGraphAtTop", options.nps_graph_at_top);
    options.transparent_density_graph_bg = load_u8_bool(
        &mut get,
        "TransparentDensityGraphBackground",
        options.transparent_density_graph_bg,
    );
    options.smx_fsr_display = load_u8_bool(&mut get, "SmxFsrDisplay", options.smx_fsr_display);
    options.smx_pad_input_display = load_u8_bool(
        &mut get,
        "SmxPadInputDisplay",
        options.smx_pad_input_display,
    );
    if let Some(pack) = get("SmxBgPack") {
        options.smx_bg_pack = (!pack.is_empty()).then_some(pack);
    }
    if let Some(pack) = get("SmxJudgePack") {
        options.smx_judge_pack = (!pack.is_empty()).then_some(pack);
    }
    options.mini_indicator = get("MiniIndicator")
        .and_then(|s| MiniIndicator::from_str(&s).ok())
        .unwrap_or({
            if options.subtractive_scoring {
                MiniIndicator::SubtractiveScoring
            } else if options.pacemaker {
                MiniIndicator::Pacemaker
            } else {
                options.mini_indicator
            }
        });
    if options.mini_indicator == MiniIndicator::SubtractiveScoring {
        options.subtractive_scoring = true;
    }
    if options.mini_indicator == MiniIndicator::Pacemaker {
        options.pacemaker = true;
    }
    options.mini_indicator_score_type = get("MiniIndicatorScoreType")
        .and_then(|s| MiniIndicatorScoreType::from_str(&s).ok())
        .unwrap_or(options.mini_indicator_score_type);
    options.mini_indicator_subtractive_display = get("MiniIndicatorSubtractiveDisplay")
        .and_then(|s| MiniIndicatorSubtractiveDisplay::from_str(&s).ok())
        .unwrap_or(options.mini_indicator_subtractive_display);
    options.mini_indicator_size = get("MiniIndicatorSize")
        .and_then(|s| MiniIndicatorSize::from_str(&s).ok())
        .unwrap_or(options.mini_indicator_size);
    options.mini_indicator_color = get("MiniIndicatorColor")
        .and_then(|s| MiniIndicatorColor::from_str(&s).ok())
        .unwrap_or(options.mini_indicator_color);
    options.mini_indicator_position = get("MiniIndicatorPosition")
        .and_then(|s| MiniIndicatorPosition::from_str(&s).ok())
        .unwrap_or(options.mini_indicator_position);
    options.scroll_option = get("Scroll")
        .and_then(|s| ScrollOption::from_str(&s).ok())
        .unwrap_or_else(|| {
            let reverse_enabled = load_u8_bool(&mut get, "ReverseScroll", options.reverse_scroll);
            if reverse_enabled {
                ScrollOption::Reverse
            } else {
                options.scroll_option
            }
        });
    options.reverse_scroll = options.scroll_option.contains(ScrollOption::Reverse);

    Some(options)
}

pub fn append_player_options_section(
    content: &mut String,
    section: &str,
    options: &PlayerOptionsData,
) {
    content.push_str(&format!("[{section}]\n"));
    content.push_str(&format!("BackgroundFilter={}\n", options.background_filter));
    content.push_str(&format!("ScrollSpeed={}\n", options.scroll_speed));
    content.push_str(&format!(
        "NoCmodAlternative={}\n",
        options.no_cmod_alternative
    ));
    content.push_str(&format!("Scroll={}\n", options.scroll_option));
    content.push_str(&format!("Turn={}\n", options.turn_option));
    content.push_str(&format!(
        "InsertMask={}\n",
        options.insert_active_mask.bits()
    ));
    content.push_str(&format!(
        "RemoveMask={}\n",
        options.remove_active_mask.bits()
    ));
    content.push_str(&format!("HoldsMask={}\n", options.holds_active_mask.bits()));
    content.push_str(&format!(
        "AccelEffectsMask={}\n",
        options.accel_effects_active_mask.bits()
    ));
    content.push_str(&format!(
        "VisualEffectsMask={}\n",
        options.visual_effects_active_mask.bits()
    ));
    content.push_str(&format!(
        "AppearanceEffectsMask={}\n",
        options.appearance_effects_active_mask.bits()
    ));
    content.push_str(&format!("AttackMode={}\n", options.attack_mode));
    content.push_str(&format!("HideLightType={}\n", options.hide_light_type));
    content.push_str(&format!(
        "RescoreEarlyHits={}\n",
        i32::from(options.rescore_early_hits)
    ));
    content.push_str(&format!(
        "HideEarlyDecentWayOffJudgments={}\n",
        i32::from(options.hide_early_dw_judgments)
    ));
    content.push_str(&format!(
        "HideEarlyDecentWayOffFlash={}\n",
        i32::from(options.hide_early_dw_flash)
    ));
    content.push_str(&format!(
        "HideEarlyDecentWayOffColumnFlash={}\n",
        i32::from(options.hide_early_dw_column_flash)
    ));
    content.push_str(&format!("TimingWindows={}\n", options.timing_windows));
    content.push_str(&format!(
        "HideTargets={}\n",
        i32::from(options.hide_targets)
    ));
    content.push_str(&format!("HideSongBG={}\n", i32::from(options.hide_song_bg)));
    content.push_str(&format!("HideCombo={}\n", i32::from(options.hide_combo)));
    content.push_str(&format!(
        "HideLifebar={}\n",
        i32::from(options.hide_lifebar)
    ));
    content.push_str(&format!("HideScore={}\n", i32::from(options.hide_score)));
    content.push_str(&format!("HideDanger={}\n", i32::from(options.hide_danger)));
    content.push_str(&format!(
        "HideComboExplosions={}\n",
        i32::from(options.hide_combo_explosions)
    ));
    content.push_str(&format!(
        "HideUsername={}\n",
        i32::from(options.hide_username)
    ));
    content.push_str(&format!(
        "ColumnFlashOnMiss={}\n",
        i32::from(options.column_flash_on_miss)
    ));
    content.push_str(&format!(
        "ColumnFlashMask={}\n",
        options.column_flash_mask.bits()
    ));
    content.push_str(&format!(
        "ColumnFlashBrightness={}\n",
        options.column_flash_brightness
    ));
    content.push_str(&format!("ColumnFlashSize={}\n", options.column_flash_size));
    content.push_str(&format!(
        "SubtractiveScoring={}\n",
        i32::from(options.subtractive_scoring)
    ));
    content.push_str(&format!("Pacemaker={}\n", i32::from(options.pacemaker)));
    content.push_str(&format!(
        "NPSGraphAtTop={}\n",
        i32::from(options.nps_graph_at_top)
    ));
    content.push_str(&format!(
        "TransparentDensityGraphBackground={}\n",
        i32::from(options.transparent_density_graph_bg)
    ));
    content.push_str(&format!(
        "SmxFsrDisplay={}\n",
        i32::from(options.smx_fsr_display)
    ));
    content.push_str(&format!(
        "SmxPadInputDisplay={}\n",
        i32::from(options.smx_pad_input_display)
    ));
    content.push_str(&format!(
        "SmxBgPack={}\n",
        options.smx_bg_pack.as_deref().unwrap_or("")
    ));
    content.push_str(&format!(
        "SmxJudgePack={}\n",
        options.smx_judge_pack.as_deref().unwrap_or("")
    ));
    content.push_str(&format!("MiniIndicator={}\n", options.mini_indicator));
    content.push_str(&format!(
        "MiniIndicatorScoreType={}\n",
        options.mini_indicator_score_type
    ));
    content.push_str(&format!(
        "MiniIndicatorSubtractiveDisplay={}\n",
        options.mini_indicator_subtractive_display
    ));
    content.push_str(&format!(
        "MiniIndicatorSize={}\n",
        options.mini_indicator_size
    ));
    content.push_str(&format!(
        "MiniIndicatorColor={}\n",
        options.mini_indicator_color
    ));
    content.push_str(&format!(
        "MiniIndicatorPosition={}\n",
        options.mini_indicator_position
    ));
    content.push_str(&format!(
        "ReverseScroll={}\n",
        i32::from(options.reverse_scroll)
    ));
    content.push_str(&format!(
        "ShowFaPlusWindow={}\n",
        i32::from(options.show_fa_plus_window)
    ));
    content.push_str(&format!(
        "ShowExScore={}\n",
        i32::from(options.show_ex_score)
    ));
    content.push_str(&format!(
        "ShowHardEXScore={}\n",
        i32::from(options.show_hard_ex_score)
    ));
    content.push_str(&format!(
        "ShowFaPlusPane={}\n",
        i32::from(options.show_fa_plus_pane)
    ));
    content.push_str(&format!(
        "SmallerWhite={}\n",
        i32::from(options.fa_plus_10ms_blue_window)
    ));
    content.push_str(&format!(
        "SplitWhites={}\n",
        i32::from(options.split_15_10ms)
    ));
    content.push_str(&format!(
        "TrackEarlyJudgments={}\n",
        i32::from(options.track_early_judgments)
    ));
    content.push_str(&format!(
        "ScaleScatterplot={}\n",
        i32::from(options.scale_scatterplot)
    ));
    content.push_str(&format!(
        "ScatterplotMaxWindow={}\n",
        options.scatterplot_max_window
    ));
    content.push_str(&format!("ScorePosition={}\n", options.score_position));
    content.push_str(&format!("ScoreDisplay={}\n", options.score_display_mode));
    content.push_str(&format!(
        "CustomFantasticWindow={}\n",
        i32::from(options.custom_fantastic_window)
    ));
    content.push_str(&format!(
        "CustomFantasticWindowMs={}\n",
        options.custom_fantastic_window_ms
    ));
    content.push_str(&format!(
        "PadLightBrightness={}\n",
        options.pad_light_brightness
    ));
    content.push_str(&format!(
        "JudgmentTilt={}\n",
        i32::from(options.judgment_tilt)
    ));
    content.push_str(&format!("ColumnCues={}\n", i32::from(options.column_cues)));
    content.push_str(&format!(
        "MeasureCues={}\n",
        i32::from(options.measure_cues)
    ));
    content.push_str(&format!(
        "CrossoverCues={}\n",
        i32::from(options.crossover_cues)
    ));
    content.push_str(&format!(
        "CrossoverCueDuration={}ms\n",
        options.crossover_cue_duration_ms
    ));
    content.push_str(&format!(
        "CrossoverCueQuantization={}\n",
        options.crossover_cue_quantization
    ));
    content.push_str(&format!(
        "CrossoverCueBrackets={}\n",
        i32::from(options.crossover_cue_brackets)
    ));
    content.push_str(&format!(
        "ColumnCountdown={}\n",
        i32::from(options.column_countdown)
    ));
    content.push_str(&format!(
        "JudgmentBack={}\n",
        i32::from(options.judgment_back)
    ));
    content.push_str(&format!(
        "ErrorMSDisplay={}\n",
        i32::from(options.error_ms_display)
    ));
    content.push_str(&format!(
        "DisplayScorebox={}\n",
        i32::from(options.display_scorebox)
    ));
    content.push_str(&format!(
        "LiveTimingStats={}\n",
        i32::from(options.live_timing_stats)
    ));
    content.push_str(&format!(
        "LiveTimingStatsMask={}\n",
        options.live_timing_stats_mask.bits()
    ));
    content.push_str(&format!("RainbowMax={}\n", i32::from(options.rainbow_max)));
    content.push_str(&format!(
        "ResponsiveColors={}\n",
        i32::from(options.responsive_colors)
    ));
    content.push_str(&format!(
        "ShowLifePercent={}\n",
        i32::from(options.show_life_percent)
    ));
    content.push_str(&format!("TiltMultiplier={}\n", options.tilt_multiplier));
    content.push_str(&format!(
        "TiltMinThresholdMs={}\n",
        options.tilt_min_threshold_ms
    ));
    content.push_str(&format!(
        "TiltMaxThresholdMs={}\n",
        options.tilt_max_threshold_ms
    ));
    content.push_str(&format!("ErrorBar={}\n", options.error_bar));
    content.push_str(&format!(
        "ErrorBarText={}\n",
        i32::from(options.error_bar_text)
    ));
    content.push_str(&format!(
        "TextErrorBarScalable={}\n",
        i32::from(options.text_error_bar_scalable)
    ));
    content.push_str(&format!(
        "TextErrorBar10ms={}\n",
        i32::from(options.text_error_bar_scalable)
    ));
    content.push_str(&format!(
        "TextErrorBarThresholdMs={}\n",
        clamp_text_error_bar_threshold_ms(options.text_error_bar_threshold_ms)
    ));
    content.push_str(&format!(
        "ErrorBarMask={}\n",
        options.error_bar_active_mask.bits()
    ));
    content.push_str(&format!(
        "Colorful={}\n",
        i32::from(
            options
                .error_bar_active_mask
                .contains(ErrorBarMask::COLORFUL)
        )
    ));
    content.push_str(&format!(
        "Monochrome={}\n",
        i32::from(
            options
                .error_bar_active_mask
                .contains(ErrorBarMask::MONOCHROME)
        )
    ));
    content.push_str(&format!(
        "Text={}\n",
        i32::from(options.error_bar_active_mask.contains(ErrorBarMask::TEXT))
    ));
    content.push_str(&format!(
        "Highlight={}\n",
        i32::from(
            options
                .error_bar_active_mask
                .contains(ErrorBarMask::HIGHLIGHT)
        )
    ));
    content.push_str(&format!(
        "Average={}\n",
        i32::from(
            options
                .error_bar_active_mask
                .contains(ErrorBarMask::AVERAGE)
        )
    ));
    content.push_str(&format!("ErrorBarUp={}\n", i32::from(options.error_bar_up)));
    content.push_str(&format!(
        "ErrorBarMultiTick={}\n",
        i32::from(options.error_bar_multi_tick)
    ));
    content.push_str(&format!("ErrorBarTrim={}\n", options.error_bar_trim));
    content.push_str(&format!("CenterTick={}\n", i32::from(options.center_tick)));
    content.push_str(&format!(
        "ShortAverageErrorBar={}\n",
        i32::from(options.short_average_error_bar_enabled)
    ));
    content.push_str(&format!(
        "AverageErrorBarIntensity={:.2}\n",
        clamp_average_error_bar_intensity(options.average_error_bar_intensity)
    ));
    content.push_str(&format!(
        "AverageErrorBarIntervalMs={}\n",
        clamp_average_error_bar_interval_ms(options.average_error_bar_interval_ms)
    ));
    content.push_str(&format!(
        "LongErrorBar={}\n",
        i32::from(options.long_error_bar_enabled)
    ));
    content.push_str(&format!(
        "LongErrorBarIntensity={:.2}\n",
        clamp_long_error_bar_intensity(options.long_error_bar_intensity)
    ));
    content.push_str(&format!(
        "LongErrorBarThresholdMs={}\n",
        clamp_long_error_bar_threshold_ms(options.long_error_bar_threshold_ms)
    ));
    content.push_str(&format!(
        "LongErrorBarMinSamples={}\n",
        clamp_long_error_bar_min_samples(options.long_error_bar_min_samples)
    ));
    content.push_str(&format!("StepStatistics={}\n", options.step_statistics));
    content.push_str(&format!("StepStatsExtra={}\n", options.step_stats_extra));
    content.push_str(&format!("TargetScore={}\n", options.target_score));
    content.push_str(&format!("LifeMeterType={}\n", options.lifemeter_type));
    content.push_str(&format!("MeasureCounter={}\n", options.measure_counter));
    content.push_str(&format!(
        "MeasureCounterLookahead={}\n",
        options.measure_counter_lookahead
    ));
    content.push_str(&format!(
        "MeasureCounterLeft={}\n",
        i32::from(options.measure_counter_left)
    ));
    content.push_str(&format!(
        "MeasureCounterUp={}\n",
        i32::from(options.measure_counter_up)
    ));
    content.push_str(&format!(
        "MeasureCounterVert={}\n",
        i32::from(options.measure_counter_vert)
    ));
    content.push_str(&format!("BrokenRun={}\n", i32::from(options.broken_run)));
    content.push_str(&format!("RunTimer={}\n", i32::from(options.run_timer)));
    content.push_str(&format!("MeasureLines={}\n", options.measure_lines));
    content.push_str(&format!(
        "HoldJudgmentGraphic={}\n",
        options.hold_judgment_graphic
    ));
    content.push_str(&format!("HeldGraphic={}\n", options.held_miss_graphic));
    content.push_str(&format!("JudgmentGraphic={}\n", options.judgment_graphic));
    content.push_str(&format!("ComboFont={}\n", options.combo_font));
    content.push_str(&format!("ComboColors={}\n", options.combo_colors));
    content.push_str(&format!("ComboMode={}\n", options.combo_mode));
    content.push_str(&format!(
        "CarryComboBetweenSongs={}\n",
        i32::from(options.carry_combo_between_songs)
    ));
    content.push_str(&format!("NoteSkin={}\n", options.noteskin));
    content.push_str(&format!(
        "MineSkin={}\n",
        options.mine_noteskin.as_ref().map_or("", NoteSkin::as_str)
    ));
    content.push_str(&format!(
        "ReceptorSkin={}\n",
        options
            .receptor_noteskin
            .as_ref()
            .map_or("", NoteSkin::as_str)
    ));
    content.push_str(&format!(
        "TapExplosionSkin={}\n",
        options
            .tap_explosion_noteskin
            .as_ref()
            .map_or("", NoteSkin::as_str)
    ));
    content.push_str(&format!(
        "TapExplosionMask={}\n",
        options.tap_explosion_active_mask.bits()
    ));
    content.push_str(&format!(
        "TapExplosionMaskVersion={}\n",
        TAP_EXPLOSION_MASK_VERSION
    ));
    content.push_str(&format!("MiniPercent={}\n", options.mini_percent));
    content.push_str(&format!("Spacing={}\n", options.spacing_percent));
    content.push_str(&format!("Perspective={}\n", options.perspective));
    content.push_str(&format!(
        "NoteFieldOffsetX={}\n",
        options.note_field_offset_x
    ));
    content.push_str(&format!(
        "NoteFieldOffsetY={}\n",
        options.note_field_offset_y
    ));
    content.push_str(&format!("JudgmentOffsetX={}\n", options.judgment_offset_x));
    content.push_str(&format!("JudgmentOffsetY={}\n", options.judgment_offset_y));
    content.push_str(&format!("ComboOffsetX={}\n", options.combo_offset_x));
    content.push_str(&format!("ComboOffsetY={}\n", options.combo_offset_y));
    content.push_str(&format!("ErrorBarOffsetX={}\n", options.error_bar_offset_x));
    content.push_str(&format!("ErrorBarOffsetY={}\n", options.error_bar_offset_y));
    content.push_str(&format!("VisualDelayMs={}\n", options.visual_delay_ms));
    content.push_str(&format!(
        "GlobalOffsetShiftMs={}\n",
        options.global_offset_shift_ms
    ));
    content.push('\n');
}

pub fn append_userprofile_section(content: &mut String, guid: &str, profile: &Profile) {
    content.push_str("[userprofile]\n");
    push_profile_guid_line(content, guid);
    content.push_str(&format!("DisplayName={}\n", profile.display_name));
    content.push_str(&format!("PlayerInitials={}\n", profile.player_initials));
    content.push('\n');
}

pub fn append_editable_section(content: &mut String, profile: &Profile) {
    content.push_str("[Editable]\n");
    content.push_str(&format!("WeightPounds={}\n", profile.weight_pounds));
    content.push_str(&format!("BirthYear={}\n", profile.birth_year));
    content.push_str(&format!(
        "IgnoreStepCountCalories={}\n",
        i32::from(profile.ignore_step_count_calories)
    ));
    content.push('\n');
}

pub fn append_stats_section(content: &mut String, profile: &Profile) {
    content.push_str("[Stats]\n");
    content.push_str(&format!(
        "CaloriesBurnedDate={}\n",
        profile.calories_burned_day
    ));
    content.push_str(&format!(
        "CaloriesBurnedToday={}\n",
        profile.calories_burned_today
    ));
    content.push('\n');
}

pub fn append_profile_ini_content(content: &mut String, guid: &str, profile: &Profile) {
    append_player_options_section(
        content,
        player_options_section(PlayStyle::Single),
        &profile.player_options_singles,
    );
    append_player_options_section(
        content,
        player_options_section(PlayStyle::Double),
        &profile.player_options_doubles,
    );
    append_userprofile_section(content, guid, profile);
    append_editable_section(content, profile);
    append_last_played_section(content, "LastPlayedSingles", &profile.last_played_singles);
    append_last_played_section(content, "LastPlayedDoubles", &profile.last_played_doubles);
    append_last_played_course_section(
        content,
        "LastPlayedCourseSingles",
        &profile.last_played_course_singles,
    );
    append_last_played_course_section(
        content,
        "LastPlayedCourseDoubles",
        &profile.last_played_course_doubles,
    );
    append_stats_section(content, profile);
}

pub fn render_profile_ini_content(guid: &str, profile: &Profile) -> String {
    let mut content = String::new();
    append_profile_ini_content(&mut content, guid, profile);
    content
}

pub fn render_groovestats_ini_content(
    api_key: &str,
    is_pad_player: bool,
    username: &str,
) -> String {
    let mut content = String::new();
    content.push_str("[GrooveStats]\n");
    content.push_str(&format!("ApiKey={api_key}\n"));
    content.push_str(&format!("IsPadPlayer={}\n", i32::from(is_pad_player)));
    content.push_str(&format!("Username={username}\n"));
    content.push('\n');
    content
}

pub fn render_arrowcloud_ini_content(api_key: &str) -> String {
    let mut content = String::new();
    content.push_str("[ArrowCloud]\n");
    content.push_str(&format!("ApiKey={api_key}\n"));
    content.push('\n');
    content
}

pub fn write_groovestats_ini_file(
    path: &Path,
    api_key: &str,
    is_pad_player: bool,
    username: &str,
) -> std::io::Result<()> {
    fs::write(
        path,
        render_groovestats_ini_content(api_key, is_pad_player, username),
    )
}

pub fn write_arrowcloud_ini_file(path: &Path, api_key: &str) -> std::io::Result<()> {
    fs::write(path, render_arrowcloud_ini_content(api_key))
}

pub fn write_profile_ini_dir(dir: &Path, guid: &str, profile: &Profile) -> std::io::Result<()> {
    fs::write(
        profile_ini_path(dir),
        render_profile_ini_content(guid, profile),
    )
}

pub fn ensure_local_profile_files_dir(
    dir: &Path,
    guid: &str,
    default_profile: &Profile,
) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let profile_ini = profile_ini_path(dir);
    if !profile_ini.exists() {
        write_profile_ini_dir(dir, guid, default_profile)?;
    }
    let groovestats_ini = groovestats_ini_path(dir);
    if !groovestats_ini.exists() {
        write_groovestats_ini_file(&groovestats_ini, "", false, "")?;
    }
    let arrowcloud_ini = arrowcloud_ini_path(dir);
    if !arrowcloud_ini.exists() {
        write_arrowcloud_ini_file(&arrowcloud_ini, "")?;
    }
    Ok(())
}

pub fn write_groovestats_credentials_dir(
    dir: &Path,
    api_key: &str,
    is_pad_player: bool,
    username: &str,
) -> std::io::Result<()> {
    write_groovestats_ini_file(&groovestats_ini_path(dir), api_key, is_pad_player, username)
}

pub fn write_arrowcloud_api_key_dir(dir: &Path, api_key: &str) -> std::io::Result<()> {
    write_arrowcloud_ini_file(&arrowcloud_ini_path(dir), api_key)
}

pub fn read_arrowcloud_api_key_file(path: &Path) -> String {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| api_key_from_ini_text(&text))
        .unwrap_or_default()
}

pub fn read_groovestats_api_key_file(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| api_key_from_ini_text(&text))
        .filter(|key| !key.is_empty())
}

pub fn read_arrowcloud_api_key_dir(dir: &Path) -> String {
    read_arrowcloud_api_key_file(&arrowcloud_ini_path(dir))
}

pub fn read_groovestats_api_key_dir(dir: &Path) -> Option<String> {
    read_groovestats_api_key_file(&groovestats_ini_path(dir))
}

pub fn api_key_from_ini_text(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        let rest = line
            .strip_prefix("ApiKey=")
            .or_else(|| line.strip_prefix("ApiKey ="));
        if let Some(rest) = rest {
            return Some(rest.trim().to_string());
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub display_name: String,
    pub player_initials: String,
    // Profile stats (Simply Love / StepMania semantics).
    pub weight_pounds: i32,
    pub birth_year: i32,
    pub calories_burned_today: f32,
    pub calories_burned_day: String,
    pub ignore_step_count_calories: bool,
    pub groovestats_api_key: String,
    pub groovestats_is_pad_player: bool,
    pub groovestats_username: String,
    pub arrowcloud_api_key: String,
    // Style-scoped player options are stored per chart family below.
    // These top-level fields hold the snapshot currently applied for the
    // active session play style so existing read paths can stay simple.
    pub background_filter: BackgroundFilter,
    pub hold_judgment_graphic: HoldJudgmentGraphic,
    pub held_miss_graphic: HeldMissGraphic,
    pub judgment_graphic: JudgmentGraphic,
    pub combo_font: ComboFont,
    pub combo_colors: ComboColors,
    pub combo_mode: ComboMode,
    pub carry_combo_between_songs: bool,
    pub current_combo: u32,
    pub known_pack_names: HashSet<String>,
    pub favorites: HashSet<String>,
    pub favorited_packs: HashSet<String>,
    pub noteskin: NoteSkin,
    pub mine_noteskin: Option<NoteSkin>,
    pub receptor_noteskin: Option<NoteSkin>,
    pub tap_explosion_noteskin: Option<NoteSkin>,
    pub tap_explosion_active_mask: TapExplosionMask,
    pub avatar_path: Option<PathBuf>,
    pub avatar_texture_key: Option<String>,
    pub scroll_speed: ScrollSpeedSetting,
    pub no_cmod_alternative: NoCmodAlternative,
    pub scroll_option: ScrollOption,
    pub reverse_scroll: bool,
    pub turn_option: TurnOption,
    // zmod uncommon modifiers (ScreenPlayerOptions3).
    // Bit order mirrors row choice order in metrics.ini.
    pub insert_active_mask: InsertMask,
    pub remove_active_mask: RemoveMask,
    pub holds_active_mask: HoldsMask,
    pub accel_effects_active_mask: AccelEffectsMask,
    pub visual_effects_active_mask: VisualEffectsMask,
    pub appearance_effects_active_mask: AppearanceEffectsMask,
    pub attack_mode: AttackMode,
    pub hide_light_type: HideLightType,
    // Allow early Decent/WayOff hits to be rescored to better judgments.
    pub rescore_early_hits: bool,
    // Visual behavior for early Decent/Way Off hits (Simply Love semantics).
    pub hide_early_dw_judgments: bool,
    pub hide_early_dw_flash: bool,
    pub hide_early_dw_column_flash: bool,
    pub timing_windows: TimingWindowsOption,
    // FA+ visual options (Simply Love semantics).
    // These do not change core timing semantics; they only affect HUD/UX.
    pub show_fa_plus_window: bool,
    pub show_ex_score: bool,
    pub show_hard_ex_score: bool,
    pub show_fa_plus_pane: bool,
    // 10ms blue Fantastic window for FA+ window display (Arrow Cloud: "SmallerWhite").
    pub fa_plus_10ms_blue_window: bool,
    // zmod SplitWhites: keep the 15ms blue FA+ judgment base and overlay the
    // white Fantastic art for 10ms-15ms hits. Visual only.
    pub split_15_10ms: bool,
    // Track and display per-column early judgment counts on evaluation (zmod/Arrow Cloud semantics).
    pub track_early_judgments: bool,
    // Constrain the evaluation scatter plot's vertical scale to a Great
    // upper cap and a Fantastic lower floor (zmod's `ScaleGraph`-style
    // toggle). Off uses the original behavior of an Excellent floor with
    // no upper cap.
    pub scale_scatterplot: bool,
    // Hard cap for the evaluation scatter plot's vertical scale. When
    // anything other than `Off`, this overrides `scale_scatterplot`'s
    // tier-snapped behavior and clamps the worst-window ms to the
    // selected judgment tier (Chris's SL `ScaleGraph`-per-tier semantics).
    pub scatterplot_max_window: ScatterplotMaxWindow,
    pub score_position: ScorePosition,
    pub score_display_mode: ScoreDisplayMode,
    // Custom blue Fantastic window in milliseconds (1..22), shared by FA+ W0 and H.EX split.
    pub custom_fantastic_window: bool,
    pub custom_fantastic_window_ms: u8,
    /// Pad-light brightness 0..=100. Seeded from the StepManiaX machine default
    /// when the profile is created; the player adjusts it in Player Options.
    pub pad_light_brightness: u8,
    // Judgment tilt (Simply Love semantics).
    pub judgment_tilt: bool,
    pub column_cues: bool,
    pub measure_cues: bool,
    pub crossover_cues: bool,
    pub crossover_cue_duration_ms: u16,
    pub crossover_cue_quantization: u8,
    pub crossover_cue_brackets: bool,
    pub column_countdown: bool,
    // zmod ExtraAesthetics: draw judgments/error timing HUD behind notes.
    pub judgment_back: bool,
    // zmod ExtraAesthetics: offset indicator (ErrorMSDisplay).
    pub error_ms_display: bool,
    pub display_scorebox: bool,
    pub live_timing_stats: bool,
    pub live_timing_stats_mask: LiveTimingStatsMask,
    // zmod LifeBarOptions (Arrow Cloud semantics).
    pub rainbow_max: bool,
    pub responsive_colors: bool,
    pub show_life_percent: bool,
    pub tilt_multiplier: f32,
    pub tilt_min_threshold_ms: u32,
    pub tilt_max_threshold_ms: u32,
    // Error bar (zmod semantics): each bit toggles one submodule in the
    // SelectMultiple row (Colorful/Monochrome/Text/Highlight/Average).
    pub error_bar_active_mask: ErrorBarMask,
    pub error_bar: ErrorBarStyle,
    // Backward-compatible text flag written to profile.ini.
    pub error_bar_text: bool,
    // Optional Text error bar mode that surfaces hits beyond a configured
    // threshold independently of the active judgment windows.
    pub text_error_bar_scalable: bool,
    pub text_error_bar_threshold_ms: u32,
    pub error_bar_up: bool,
    pub error_bar_multi_tick: bool,
    pub error_bar_trim: ErrorBarTrim,
    pub center_tick: bool,
    pub short_average_error_bar_enabled: bool,
    pub average_error_bar_intensity: f32,
    pub average_error_bar_interval_ms: u32,
    pub long_error_bar_enabled: bool,
    pub long_error_bar_intensity: f32,
    pub long_error_bar_threshold_ms: u32,
    pub long_error_bar_min_samples: u32,
    pub step_statistics: StepStatisticsMask,
    pub step_stats_extra: StepStatsExtra,
    pub target_score: TargetScoreSetting,
    pub lifemeter_type: LifeMeterType,
    pub measure_counter: MeasureCounter,
    pub measure_counter_lookahead: u8,
    pub measure_counter_left: bool,
    pub measure_counter_up: bool,
    pub measure_counter_vert: bool,
    pub broken_run: bool,
    pub run_timer: bool,
    pub measure_lines: MeasureLines,
    // "Hide" options (Simply Love semantics).
    pub hide_targets: bool,
    pub hide_song_bg: bool,
    pub hide_combo: bool,
    pub hide_lifebar: bool,
    pub hide_score: bool,
    pub hide_danger: bool,
    pub hide_combo_explosions: bool,
    pub hide_username: bool,
    // Gameplay extras (Simply Love semantics).
    pub column_flash_on_miss: bool,
    pub column_flash_mask: ColumnFlashMask,
    pub column_flash_brightness: ColumnFlashBrightness,
    pub column_flash_size: ColumnFlashSize,
    pub subtractive_scoring: bool,
    pub pacemaker: bool,
    pub nps_graph_at_top: bool,
    pub transparent_density_graph_bg: bool,
    pub smx_fsr_display: bool,
    pub smx_pad_input_display: bool,
    pub smx_bg_pack: Option<String>,
    pub smx_judge_pack: Option<String>,
    pub mini_indicator: MiniIndicator,
    pub mini_indicator_score_type: MiniIndicatorScoreType,
    pub mini_indicator_subtractive_display: MiniIndicatorSubtractiveDisplay,
    pub mini_indicator_size: MiniIndicatorSize,
    pub mini_indicator_color: MiniIndicatorColor,
    pub mini_indicator_position: MiniIndicatorPosition,
    // Mini modifier as a percentage, mirroring Simply Love semantics.
    // 0 = normal size, 100 = 100% Mini (smaller), negative values enlarge.
    pub mini_percent: i32,
    /// Horizontal spacing between note columns as a percentage (zmod parity).
    /// 0 = noteskin default, +N% scales lateral column offsets by
    /// `1 + N/100`. Range -100..=100 (capped on read to stay sane).
    pub spacing_percent: i32,
    pub perspective: Perspective,
    // NoteField positional offsets (Simply Love semantics).
    // X is non-negative and interpreted relative to player side:
    // for P1, positive values move the field left.
    pub note_field_offset_x: i32,
    // Y is applied directly to the notefield and related HUD,
    // positive values move everything down.
    pub note_field_offset_y: i32,
    // Independent HUD element offsets in logical pixels.
    // Positive X = right, positive Y = down.
    pub judgment_offset_x: i32,
    pub judgment_offset_y: i32,
    pub combo_offset_x: i32,
    pub combo_offset_y: i32,
    pub error_bar_offset_x: i32,
    pub error_bar_offset_y: i32,
    // Per-player visual delay (Simply Love semantics). Stored in milliseconds.
    // Negative values shift arrows upwards; positive values shift them down.
    pub visual_delay_ms: i32,
    // Per-player timing shift applied on top of machine global offset. Stored in milliseconds.
    pub global_offset_shift_ms: i32,
    pub player_options_singles: PlayerOptionsData,
    pub player_options_doubles: PlayerOptionsData,
    // Persisted "last played" selections so future sessions can reopen
    // SelectMusic on the most recently played chart for each chart family.
    // Singles is shared by Single and Versus. Double uses its own entry.
    pub last_played_singles: LastPlayed,
    pub last_played_doubles: LastPlayed,
    pub last_played_course_singles: LastPlayedCourse,
    pub last_played_course_doubles: LastPlayedCourse,
}

impl Default for Profile {
    fn default() -> Self {
        let player_options = PlayerOptionsData::default();
        Self {
            display_name: "Player 1".to_string(),
            player_initials: "P1".to_string(),
            weight_pounds: 0,
            birth_year: 0,
            calories_burned_today: 0.0,
            calories_burned_day: String::new(),
            ignore_step_count_calories: false,
            groovestats_api_key: String::new(),
            groovestats_is_pad_player: false,
            groovestats_username: String::new(),
            arrowcloud_api_key: String::new(),
            background_filter: player_options.background_filter,
            hold_judgment_graphic: player_options.hold_judgment_graphic.clone(),
            held_miss_graphic: player_options.held_miss_graphic.clone(),
            judgment_graphic: player_options.judgment_graphic.clone(),
            combo_font: player_options.combo_font,
            combo_colors: player_options.combo_colors,
            combo_mode: player_options.combo_mode,
            carry_combo_between_songs: player_options.carry_combo_between_songs,
            current_combo: 0,
            known_pack_names: HashSet::new(),
            favorites: HashSet::new(),
            favorited_packs: HashSet::new(),
            noteskin: player_options.noteskin.clone(),
            mine_noteskin: player_options.mine_noteskin.clone(),
            receptor_noteskin: player_options.receptor_noteskin.clone(),
            tap_explosion_noteskin: player_options.tap_explosion_noteskin.clone(),
            tap_explosion_active_mask: player_options.tap_explosion_active_mask,
            avatar_path: None,
            avatar_texture_key: None,
            scroll_speed: player_options.scroll_speed,
            no_cmod_alternative: player_options.no_cmod_alternative,
            scroll_option: player_options.scroll_option,
            reverse_scroll: player_options.reverse_scroll,
            turn_option: player_options.turn_option,
            insert_active_mask: player_options.insert_active_mask,
            remove_active_mask: player_options.remove_active_mask,
            holds_active_mask: player_options.holds_active_mask,
            accel_effects_active_mask: player_options.accel_effects_active_mask,
            visual_effects_active_mask: player_options.visual_effects_active_mask,
            appearance_effects_active_mask: player_options.appearance_effects_active_mask,
            attack_mode: player_options.attack_mode,
            hide_light_type: player_options.hide_light_type,
            rescore_early_hits: player_options.rescore_early_hits,
            hide_early_dw_judgments: player_options.hide_early_dw_judgments,
            hide_early_dw_flash: player_options.hide_early_dw_flash,
            hide_early_dw_column_flash: player_options.hide_early_dw_column_flash,
            timing_windows: player_options.timing_windows,
            show_fa_plus_window: player_options.show_fa_plus_window,
            show_ex_score: player_options.show_ex_score,
            show_hard_ex_score: player_options.show_hard_ex_score,
            show_fa_plus_pane: player_options.show_fa_plus_pane,
            fa_plus_10ms_blue_window: player_options.fa_plus_10ms_blue_window,
            split_15_10ms: player_options.split_15_10ms,
            track_early_judgments: player_options.track_early_judgments,
            scale_scatterplot: player_options.scale_scatterplot,
            scatterplot_max_window: player_options.scatterplot_max_window,
            score_position: player_options.score_position,
            score_display_mode: player_options.score_display_mode,
            custom_fantastic_window: player_options.custom_fantastic_window,
            custom_fantastic_window_ms: player_options.custom_fantastic_window_ms,
            pad_light_brightness: player_options.pad_light_brightness,
            judgment_tilt: player_options.judgment_tilt,
            column_cues: player_options.column_cues,
            measure_cues: player_options.measure_cues,
            crossover_cues: player_options.crossover_cues,
            crossover_cue_duration_ms: player_options.crossover_cue_duration_ms,
            crossover_cue_quantization: player_options.crossover_cue_quantization,
            crossover_cue_brackets: player_options.crossover_cue_brackets,
            column_countdown: player_options.column_countdown,
            judgment_back: player_options.judgment_back,
            error_ms_display: player_options.error_ms_display,
            display_scorebox: player_options.display_scorebox,
            live_timing_stats: player_options.live_timing_stats,
            live_timing_stats_mask: player_options.live_timing_stats_mask,
            rainbow_max: player_options.rainbow_max,
            responsive_colors: player_options.responsive_colors,
            show_life_percent: player_options.show_life_percent,
            tilt_multiplier: player_options.tilt_multiplier,
            tilt_min_threshold_ms: player_options.tilt_min_threshold_ms,
            tilt_max_threshold_ms: player_options.tilt_max_threshold_ms,
            error_bar: player_options.error_bar,
            error_bar_active_mask: player_options.error_bar_active_mask,
            error_bar_text: player_options.error_bar_text,
            text_error_bar_scalable: player_options.text_error_bar_scalable,
            text_error_bar_threshold_ms: player_options.text_error_bar_threshold_ms,
            error_bar_up: player_options.error_bar_up,
            error_bar_multi_tick: player_options.error_bar_multi_tick,
            error_bar_trim: player_options.error_bar_trim,
            center_tick: player_options.center_tick,
            short_average_error_bar_enabled: player_options.short_average_error_bar_enabled,
            average_error_bar_intensity: player_options.average_error_bar_intensity,
            average_error_bar_interval_ms: player_options.average_error_bar_interval_ms,
            long_error_bar_enabled: player_options.long_error_bar_enabled,
            long_error_bar_intensity: player_options.long_error_bar_intensity,
            long_error_bar_threshold_ms: player_options.long_error_bar_threshold_ms,
            long_error_bar_min_samples: player_options.long_error_bar_min_samples,
            step_statistics: player_options.step_statistics,
            step_stats_extra: player_options.step_stats_extra,
            target_score: player_options.target_score,
            lifemeter_type: player_options.lifemeter_type,
            measure_counter: player_options.measure_counter,
            measure_counter_lookahead: player_options.measure_counter_lookahead,
            measure_counter_left: player_options.measure_counter_left,
            measure_counter_up: player_options.measure_counter_up,
            measure_counter_vert: player_options.measure_counter_vert,
            broken_run: player_options.broken_run,
            run_timer: player_options.run_timer,
            measure_lines: player_options.measure_lines,
            hide_targets: player_options.hide_targets,
            hide_song_bg: player_options.hide_song_bg,
            hide_combo: player_options.hide_combo,
            hide_lifebar: player_options.hide_lifebar,
            hide_score: player_options.hide_score,
            hide_danger: player_options.hide_danger,
            hide_combo_explosions: player_options.hide_combo_explosions,
            hide_username: player_options.hide_username,
            column_flash_on_miss: player_options.column_flash_on_miss,
            column_flash_mask: player_options.column_flash_mask,
            column_flash_brightness: player_options.column_flash_brightness,
            column_flash_size: player_options.column_flash_size,
            subtractive_scoring: player_options.subtractive_scoring,
            pacemaker: player_options.pacemaker,
            nps_graph_at_top: player_options.nps_graph_at_top,
            transparent_density_graph_bg: player_options.transparent_density_graph_bg,
            smx_fsr_display: player_options.smx_fsr_display,
            smx_pad_input_display: player_options.smx_pad_input_display,
            smx_bg_pack: player_options.smx_bg_pack.clone(),
            smx_judge_pack: player_options.smx_judge_pack.clone(),
            mini_indicator: player_options.mini_indicator,
            mini_indicator_score_type: player_options.mini_indicator_score_type,
            mini_indicator_subtractive_display: player_options.mini_indicator_subtractive_display,
            mini_indicator_size: player_options.mini_indicator_size,
            mini_indicator_color: player_options.mini_indicator_color,
            mini_indicator_position: player_options.mini_indicator_position,
            mini_percent: player_options.mini_percent,
            spacing_percent: player_options.spacing_percent,
            perspective: player_options.perspective,
            note_field_offset_x: player_options.note_field_offset_x,
            note_field_offset_y: player_options.note_field_offset_y,
            judgment_offset_x: player_options.judgment_offset_x,
            judgment_offset_y: player_options.judgment_offset_y,
            combo_offset_x: player_options.combo_offset_x,
            combo_offset_y: player_options.combo_offset_y,
            error_bar_offset_x: player_options.error_bar_offset_x,
            error_bar_offset_y: player_options.error_bar_offset_y,
            visual_delay_ms: player_options.visual_delay_ms,
            global_offset_shift_ms: player_options.global_offset_shift_ms,
            player_options_singles: player_options.clone(),
            player_options_doubles: player_options,
            last_played_singles: LastPlayed::default(),
            last_played_doubles: LastPlayed::default(),
            last_played_course_singles: LastPlayedCourse::default(),
            last_played_course_doubles: LastPlayedCourse::default(),
        }
    }
}

impl Profile {
    pub fn score_import_api_key(&self, endpoint: ScoreImportEndpoint) -> &str {
        match endpoint {
            ScoreImportEndpoint::GrooveStats | ScoreImportEndpoint::BoogieStats => {
                self.groovestats_api_key.trim()
            }
            ScoreImportEndpoint::ArrowCloud => self.arrowcloud_api_key.trim(),
        }
    }

    pub fn score_import_username(&self, endpoint: ScoreImportEndpoint) -> &str {
        if endpoint.requires_username() {
            self.groovestats_username.trim()
        } else {
            ""
        }
    }

    pub fn has_score_import_credentials(&self, endpoint: ScoreImportEndpoint) -> bool {
        !self.score_import_api_key(endpoint).is_empty()
            && (!endpoint.requires_username() || !self.score_import_username(endpoint).is_empty())
    }

    pub fn set_last_played(
        &mut self,
        style: PlayStyle,
        song_music_path: Option<String>,
        chart_hash: Option<String>,
        difficulty_index: usize,
    ) -> bool {
        let last_played = self.last_played_mut(style);
        if last_played.song_music_path == song_music_path
            && last_played.chart_hash == chart_hash
            && last_played.difficulty_index == difficulty_index
        {
            return false;
        }
        last_played.song_music_path = song_music_path;
        last_played.chart_hash = chart_hash;
        last_played.difficulty_index = difficulty_index;
        true
    }

    pub fn set_last_played_course(
        &mut self,
        style: PlayStyle,
        course_path: Option<String>,
        difficulty_name: Option<String>,
    ) -> bool {
        let last_played = self.last_played_course_mut(style);
        if last_played.course_path == course_path && last_played.difficulty_name == difficulty_name
        {
            return false;
        }
        last_played.course_path = course_path;
        last_played.difficulty_name = difficulty_name;
        true
    }

    pub fn add_stage_calories_for_day(&mut self, day: &str, calories_burned: f32) -> bool {
        let mut changed = false;
        if self.calories_burned_day.trim() != day {
            self.calories_burned_day = day.to_string();
            self.calories_burned_today = 0.0;
            changed = true;
        }

        if !self.ignore_step_count_calories && calories_burned.is_finite() && calories_burned >= 0.0
        {
            let calories = (self.calories_burned_today + calories_burned).max(0.0);
            changed |= set_f32_if_changed(&mut self.calories_burned_today, calories);
        }
        changed
    }

    pub fn set_player_initials(&mut self, initials: &str) -> bool {
        let initials = sanitize_player_initials(initials);
        if initials.is_empty() || self.player_initials == initials {
            return false;
        }
        self.player_initials = initials;
        true
    }

    pub fn set_scroll_speed(&mut self, setting: ScrollSpeedSetting) -> bool {
        set_value_if_changed(&mut self.scroll_speed, setting)
    }

    pub fn set_background_filter_percent(&mut self, percent: i32) -> bool {
        set_value_if_changed(
            &mut self.background_filter,
            BackgroundFilter::from_i32(percent),
        )
    }

    pub fn set_hold_judgment_graphic(&mut self, setting: HoldJudgmentGraphic) -> bool {
        set_value_if_changed(&mut self.hold_judgment_graphic, setting)
    }

    pub fn set_held_miss_graphic(&mut self, setting: HeldMissGraphic) -> bool {
        set_value_if_changed(&mut self.held_miss_graphic, setting)
    }

    pub fn set_judgment_graphic(&mut self, setting: JudgmentGraphic) -> bool {
        set_value_if_changed(&mut self.judgment_graphic, setting)
    }

    pub fn set_combo_font(&mut self, setting: ComboFont) -> bool {
        set_value_if_changed(&mut self.combo_font, setting)
    }

    pub fn set_combo_colors(&mut self, setting: ComboColors) -> bool {
        set_value_if_changed(&mut self.combo_colors, setting)
    }

    pub fn set_combo_mode(&mut self, setting: ComboMode) -> bool {
        set_value_if_changed(&mut self.combo_mode, setting)
    }

    pub fn set_carry_combo_between_songs(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.carry_combo_between_songs, enabled)
    }

    pub fn set_current_combo(&mut self, combo: u32) -> bool {
        set_u32_if_changed(&mut self.current_combo, combo)
    }

    pub fn set_scroll_option(&mut self, setting: ScrollOption) -> bool {
        let reverse_enabled = setting.contains(ScrollOption::Reverse);
        if self.scroll_option == setting && self.reverse_scroll == reverse_enabled {
            return false;
        }
        self.scroll_option = setting;
        self.reverse_scroll = reverse_enabled;
        true
    }

    pub fn set_turn_option(&mut self, setting: TurnOption) -> bool {
        set_value_if_changed(&mut self.turn_option, setting)
    }

    pub fn set_insert_mask(&mut self, mask: InsertMask) -> bool {
        set_value_if_changed(&mut self.insert_active_mask, mask)
    }

    pub fn set_remove_mask(&mut self, mask: RemoveMask) -> bool {
        set_value_if_changed(&mut self.remove_active_mask, mask)
    }

    pub fn set_holds_mask(&mut self, mask: HoldsMask) -> bool {
        set_value_if_changed(&mut self.holds_active_mask, mask)
    }

    pub fn set_accel_effects_mask(&mut self, mask: AccelEffectsMask) -> bool {
        set_value_if_changed(&mut self.accel_effects_active_mask, mask)
    }

    pub fn set_visual_effects_mask(&mut self, mask: VisualEffectsMask) -> bool {
        set_value_if_changed(&mut self.visual_effects_active_mask, mask)
    }

    pub fn set_appearance_effects_mask(&mut self, mask: AppearanceEffectsMask) -> bool {
        set_value_if_changed(&mut self.appearance_effects_active_mask, mask)
    }

    pub fn set_attack_mode(&mut self, setting: AttackMode) -> bool {
        set_value_if_changed(&mut self.attack_mode, setting)
    }

    pub fn set_hide_light_type(&mut self, setting: HideLightType) -> bool {
        set_value_if_changed(&mut self.hide_light_type, setting)
    }

    pub fn set_rescore_early_hits(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.rescore_early_hits, enabled)
    }

    pub fn set_gameplay_extras(
        &mut self,
        column_flash_on_miss: bool,
        subtractive_scoring: bool,
        pacemaker: bool,
        nps_graph_at_top: bool,
    ) -> bool {
        if self.column_flash_on_miss == column_flash_on_miss
            && self.subtractive_scoring == subtractive_scoring
            && self.pacemaker == pacemaker
            && self.nps_graph_at_top == nps_graph_at_top
        {
            return false;
        }
        self.column_flash_on_miss = column_flash_on_miss;
        self.subtractive_scoring = subtractive_scoring;
        self.pacemaker = pacemaker;
        self.nps_graph_at_top = nps_graph_at_top;
        if subtractive_scoring {
            self.mini_indicator = MiniIndicator::SubtractiveScoring;
        } else if pacemaker {
            self.mini_indicator = MiniIndicator::Pacemaker;
        } else if matches!(
            self.mini_indicator,
            MiniIndicator::SubtractiveScoring | MiniIndicator::Pacemaker
        ) {
            self.mini_indicator = MiniIndicator::None;
        }
        true
    }

    pub fn set_column_flash_mask(&mut self, mask: ColumnFlashMask) -> bool {
        if self.column_flash_mask == mask {
            return false;
        }
        self.column_flash_mask = mask;
        true
    }

    pub fn set_column_flash_brightness(&mut self, setting: ColumnFlashBrightness) -> bool {
        set_value_if_changed(&mut self.column_flash_brightness, setting)
    }

    pub fn set_column_flash_size(&mut self, setting: ColumnFlashSize) -> bool {
        set_value_if_changed(&mut self.column_flash_size, setting)
    }

    pub fn set_transparent_density_graph_bg(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.transparent_density_graph_bg, enabled)
    }

    pub fn set_smx_fsr_display(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.smx_fsr_display, enabled)
    }

    pub fn set_smx_pad_input_display(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.smx_pad_input_display, enabled)
    }

    pub fn set_smx_bg_pack(&mut self, pack: Option<String>) -> bool {
        set_value_if_changed(&mut self.smx_bg_pack, pack)
    }

    pub fn set_smx_judge_pack(&mut self, pack: Option<String>) -> bool {
        set_value_if_changed(&mut self.smx_judge_pack, pack)
    }

    pub fn set_mini_indicator(&mut self, setting: MiniIndicator) -> bool {
        set_value_if_changed(&mut self.mini_indicator, setting)
    }

    pub fn set_mini_indicator_score_type(&mut self, setting: MiniIndicatorScoreType) -> bool {
        set_value_if_changed(&mut self.mini_indicator_score_type, setting)
    }

    pub fn set_mini_indicator_subtractive_display(
        &mut self,
        setting: MiniIndicatorSubtractiveDisplay,
    ) -> bool {
        set_value_if_changed(&mut self.mini_indicator_subtractive_display, setting)
    }

    pub fn set_mini_indicator_size(&mut self, setting: MiniIndicatorSize) -> bool {
        set_value_if_changed(&mut self.mini_indicator_size, setting)
    }

    pub fn set_mini_indicator_color(&mut self, setting: MiniIndicatorColor) -> bool {
        set_value_if_changed(&mut self.mini_indicator_color, setting)
    }

    pub fn set_mini_indicator_position(&mut self, setting: MiniIndicatorPosition) -> bool {
        set_value_if_changed(&mut self.mini_indicator_position, setting)
    }

    pub fn set_noteskin(&mut self, setting: NoteSkin) -> bool {
        set_value_if_changed(&mut self.noteskin, setting)
    }

    pub fn set_mine_noteskin(&mut self, setting: Option<NoteSkin>) -> bool {
        set_value_if_changed(&mut self.mine_noteskin, setting)
    }

    pub fn set_receptor_noteskin(&mut self, setting: Option<NoteSkin>) -> bool {
        set_value_if_changed(&mut self.receptor_noteskin, setting)
    }

    pub fn set_tap_explosion_noteskin(&mut self, setting: Option<NoteSkin>) -> bool {
        set_value_if_changed(&mut self.tap_explosion_noteskin, setting)
    }

    pub fn set_tap_explosion_mask(&mut self, setting: TapExplosionMask) -> bool {
        set_value_if_changed(&mut self.tap_explosion_active_mask, setting)
    }

    pub fn set_early_dw_options(
        &mut self,
        hide_judgments: bool,
        hide_flash: bool,
        hide_column_flash: bool,
    ) -> bool {
        if self.hide_early_dw_judgments == hide_judgments
            && self.hide_early_dw_flash == hide_flash
            && self.hide_early_dw_column_flash == hide_column_flash
        {
            return false;
        }
        self.hide_early_dw_judgments = hide_judgments;
        self.hide_early_dw_flash = hide_flash;
        self.hide_early_dw_column_flash = hide_column_flash;
        true
    }

    pub fn set_timing_windows(&mut self, setting: TimingWindowsOption) -> bool {
        set_value_if_changed(&mut self.timing_windows, setting)
    }

    pub fn set_perspective(&mut self, setting: Perspective) -> bool {
        set_value_if_changed(&mut self.perspective, setting)
    }

    pub fn set_no_cmod_alternative(&mut self, setting: NoCmodAlternative) -> bool {
        set_value_if_changed(&mut self.no_cmod_alternative, setting)
    }

    pub fn set_show_fa_plus_window(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.show_fa_plus_window, enabled)
    }

    pub fn set_show_ex_score(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.show_ex_score, enabled)
    }

    pub fn set_show_hard_ex_score(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.show_hard_ex_score, enabled)
    }

    pub fn set_show_fa_plus_pane(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.show_fa_plus_pane, enabled)
    }

    pub fn set_fa_plus_10ms_blue_window(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.fa_plus_10ms_blue_window, enabled)
    }

    pub fn set_track_early_judgments(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.track_early_judgments, enabled)
    }

    pub fn set_scale_scatterplot(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.scale_scatterplot, enabled)
    }

    pub fn set_split_15_10ms(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.split_15_10ms, enabled)
    }

    pub fn set_custom_fantastic_window(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.custom_fantastic_window, enabled)
    }

    pub fn set_judgment_tilt(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.judgment_tilt, enabled)
    }

    pub fn set_column_cues(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.column_cues, enabled)
    }

    pub fn set_measure_cues(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.measure_cues, enabled)
    }

    pub fn set_crossover_cues(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.crossover_cues, enabled)
    }

    pub fn set_crossover_cue_brackets(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.crossover_cue_brackets, enabled)
    }

    pub fn set_crossover_cue_duration_ms(&mut self, ms: u16) -> bool {
        set_value_if_changed(
            &mut self.crossover_cue_duration_ms,
            clamp_crossover_cue_duration_ms(ms),
        )
    }

    pub fn set_crossover_cue_quantization(&mut self, quantization: u8) -> bool {
        set_value_if_changed(
            &mut self.crossover_cue_quantization,
            clamp_crossover_cue_quantization(quantization),
        )
    }

    pub fn set_column_countdown(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.column_countdown, enabled)
    }

    pub fn set_judgment_back(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.judgment_back, enabled)
    }

    pub fn set_error_ms_display(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.error_ms_display, enabled)
    }

    pub fn set_live_timing_stats_mask(&mut self, mask: LiveTimingStatsMask) -> bool {
        set_value_if_changed(&mut self.live_timing_stats_mask, mask)
    }

    pub fn set_live_timing_stats(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.live_timing_stats, enabled)
    }

    pub fn set_rainbow_max(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.rainbow_max, enabled)
    }

    pub fn set_responsive_colors(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.responsive_colors, enabled)
    }

    pub fn set_show_life_percent(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.show_life_percent, enabled)
    }

    pub fn set_hide_options(
        &mut self,
        hide_targets: bool,
        hide_song_bg: bool,
        hide_combo: bool,
        hide_lifebar: bool,
        hide_score: bool,
        hide_danger: bool,
        hide_combo_explosions: bool,
        hide_username: bool,
    ) -> bool {
        if self.hide_targets == hide_targets
            && self.hide_song_bg == hide_song_bg
            && self.hide_combo == hide_combo
            && self.hide_lifebar == hide_lifebar
            && self.hide_score == hide_score
            && self.hide_danger == hide_danger
            && self.hide_combo_explosions == hide_combo_explosions
            && self.hide_username == hide_username
        {
            return false;
        }
        self.hide_targets = hide_targets;
        self.hide_song_bg = hide_song_bg;
        self.hide_combo = hide_combo;
        self.hide_lifebar = hide_lifebar;
        self.hide_score = hide_score;
        self.hide_danger = hide_danger;
        self.hide_combo_explosions = hide_combo_explosions;
        self.hide_username = hide_username;
        true
    }

    pub fn set_tilt_thresholds(&mut self, min_ms: u32, max_ms: u32) -> bool {
        let min_ms = clamp_tilt_threshold_ms(min_ms);
        let max_ms = clamp_tilt_threshold_ms(max_ms).max(min_ms);
        if self.tilt_min_threshold_ms == min_ms && self.tilt_max_threshold_ms == max_ms {
            return false;
        }
        self.tilt_min_threshold_ms = min_ms;
        self.tilt_max_threshold_ms = max_ms;
        true
    }

    pub fn set_error_bar_mask(&mut self, mask: ErrorBarMask) -> bool {
        if self.error_bar_active_mask == mask {
            return false;
        }
        self.error_bar_active_mask = mask;
        self.error_bar = error_bar_style_from_mask(mask);
        self.error_bar_text = error_bar_text_from_mask(mask);
        true
    }

    pub fn set_error_bar_trim(&mut self, setting: ErrorBarTrim) -> bool {
        set_value_if_changed(&mut self.error_bar_trim, setting)
    }

    pub fn set_center_tick(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.center_tick, enabled)
    }

    pub fn set_text_error_bar_scalable(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.text_error_bar_scalable, enabled)
    }

    pub fn set_note_field_offset_x(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.note_field_offset_x,
            offset.clamp(NOTE_FIELD_OFFSET_X_MIN, NOTE_FIELD_OFFSET_X_MAX),
        )
    }

    pub fn set_note_field_offset_y(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.note_field_offset_y,
            offset.clamp(NOTE_FIELD_OFFSET_Y_MIN, NOTE_FIELD_OFFSET_Y_MAX),
        )
    }

    pub fn set_judgment_offset_x(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.judgment_offset_x,
            offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        )
    }

    pub fn set_judgment_offset_y(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.judgment_offset_y,
            offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        )
    }

    pub fn set_combo_offset_x(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.combo_offset_x,
            offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        )
    }

    pub fn set_combo_offset_y(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.combo_offset_y,
            offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        )
    }

    pub fn set_error_bar_offset_x(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.error_bar_offset_x,
            offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        )
    }

    pub fn set_error_bar_offset_y(&mut self, offset: i32) -> bool {
        set_i32_if_changed(
            &mut self.error_bar_offset_y,
            offset.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        )
    }

    pub fn set_mini_percent(&mut self, percent: i32) -> bool {
        set_i32_if_changed(
            &mut self.mini_percent,
            percent.clamp(MINI_PERCENT_MIN, MINI_PERCENT_MAX),
        )
    }

    pub fn set_spacing_percent(&mut self, percent: i32) -> bool {
        set_i32_if_changed(
            &mut self.spacing_percent,
            percent.clamp(SPACING_PERCENT_MIN, SPACING_PERCENT_MAX),
        )
    }

    pub fn set_visual_delay_ms(&mut self, ms: i32) -> bool {
        set_i32_if_changed(
            &mut self.visual_delay_ms,
            ms.clamp(VISUAL_DELAY_MS_MIN, VISUAL_DELAY_MS_MAX),
        )
    }

    pub fn set_global_offset_shift_ms(&mut self, ms: i32) -> bool {
        set_i32_if_changed(
            &mut self.global_offset_shift_ms,
            ms.clamp(VISUAL_DELAY_MS_MIN, VISUAL_DELAY_MS_MAX),
        )
    }

    pub fn set_tilt_multiplier(&mut self, multiplier: f32) -> bool {
        if !multiplier.is_finite() {
            return false;
        }
        set_f32_if_changed(&mut self.tilt_multiplier, multiplier)
    }

    pub fn set_custom_fantastic_window_ms(&mut self, ms: u8) -> bool {
        set_u8_if_changed(
            &mut self.custom_fantastic_window_ms,
            clamp_custom_fantastic_window_ms(ms),
        )
    }

    pub fn set_pad_light_brightness(&mut self, percent: u8) -> bool {
        set_u8_if_changed(
            &mut self.pad_light_brightness,
            clamp_pad_light_brightness(percent),
        )
    }

    pub fn set_average_error_bar_intensity(&mut self, intensity: f32) -> bool {
        set_f32_if_changed(
            &mut self.average_error_bar_intensity,
            clamp_average_error_bar_intensity(intensity),
        )
    }

    pub fn set_average_error_bar_interval_ms(&mut self, ms: u32) -> bool {
        set_u32_if_changed(
            &mut self.average_error_bar_interval_ms,
            clamp_average_error_bar_interval_ms(ms),
        )
    }

    pub fn set_short_average_error_bar_enabled(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.short_average_error_bar_enabled, enabled)
    }

    pub fn set_text_error_bar_threshold_ms(&mut self, ms: u32) -> bool {
        set_u32_if_changed(
            &mut self.text_error_bar_threshold_ms,
            clamp_text_error_bar_threshold_ms(ms),
        )
    }

    pub fn set_long_error_bar_intensity(&mut self, intensity: f32) -> bool {
        set_f32_if_changed(
            &mut self.long_error_bar_intensity,
            clamp_long_error_bar_intensity(intensity),
        )
    }

    pub fn set_long_error_bar_enabled(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.long_error_bar_enabled, enabled)
    }

    pub fn set_long_error_bar_threshold_ms(&mut self, ms: u32) -> bool {
        set_u32_if_changed(
            &mut self.long_error_bar_threshold_ms,
            clamp_long_error_bar_threshold_ms(ms),
        )
    }

    pub fn set_long_error_bar_min_samples(&mut self, n: u32) -> bool {
        set_u32_if_changed(
            &mut self.long_error_bar_min_samples,
            clamp_long_error_bar_min_samples(n),
        )
    }

    pub fn set_error_bar_options(&mut self, up: bool, multi_tick: bool) -> bool {
        if self.error_bar_up == up && self.error_bar_multi_tick == multi_tick {
            return false;
        }
        self.error_bar_up = up;
        self.error_bar_multi_tick = multi_tick;
        true
    }

    pub fn set_step_statistics(&mut self, mask: StepStatisticsMask) -> bool {
        set_value_if_changed(&mut self.step_statistics, mask)
    }

    pub fn set_step_stats_extra(&mut self, setting: StepStatsExtra) -> bool {
        set_value_if_changed(&mut self.step_stats_extra, setting)
    }

    pub fn set_display_scorebox(&mut self, enabled: bool) -> bool {
        set_value_if_changed(&mut self.display_scorebox, enabled)
    }

    pub fn set_scatterplot_max_window(&mut self, setting: ScatterplotMaxWindow) -> bool {
        set_value_if_changed(&mut self.scatterplot_max_window, setting)
    }

    pub fn set_score_position(&mut self, setting: ScorePosition) -> bool {
        set_value_if_changed(&mut self.score_position, setting)
    }

    pub fn set_score_display_mode(&mut self, setting: ScoreDisplayMode) -> bool {
        set_value_if_changed(&mut self.score_display_mode, setting)
    }

    pub fn set_target_score(&mut self, setting: TargetScoreSetting) -> bool {
        set_value_if_changed(&mut self.target_score, setting)
    }

    pub fn set_lifemeter_type(&mut self, setting: LifeMeterType) -> bool {
        set_value_if_changed(&mut self.lifemeter_type, setting)
    }

    pub fn set_measure_counter(&mut self, setting: MeasureCounter) -> bool {
        set_value_if_changed(&mut self.measure_counter, setting)
    }

    pub fn set_measure_counter_lookahead(&mut self, lookahead: u8) -> bool {
        set_u8_if_changed(&mut self.measure_counter_lookahead, lookahead.min(4))
    }

    pub fn set_measure_counter_options(
        &mut self,
        left: bool,
        up: bool,
        vert: bool,
        broken_run: bool,
        run_timer: bool,
    ) -> bool {
        if self.measure_counter_left == left
            && self.measure_counter_up == up
            && self.measure_counter_vert == vert
            && self.broken_run == broken_run
            && self.run_timer == run_timer
        {
            return false;
        }
        self.measure_counter_left = left;
        self.measure_counter_up = up;
        self.measure_counter_vert = vert;
        self.broken_run = broken_run;
        self.run_timer = run_timer;
        true
    }

    pub fn set_measure_lines(&mut self, setting: MeasureLines) -> bool {
        set_value_if_changed(&mut self.measure_lines, setting)
    }

    #[inline(always)]
    pub const fn calculated_weight_pounds(&self) -> i32 {
        resolved_weight_pounds(self.weight_pounds)
    }

    #[inline(always)]
    pub const fn age_years_for(&self, current_year: i32) -> i32 {
        age_years_for_birth_year(self.birth_year, current_year)
    }

    #[inline(always)]
    pub fn age_years(&self) -> i32 {
        self.age_years_for(Local::now().year())
    }

    #[inline(always)]
    pub fn resolved_mine_noteskin(&self) -> &NoteSkin {
        resolve_noteskin_choice(self.mine_noteskin.as_ref(), &self.noteskin)
    }

    #[inline(always)]
    pub fn resolved_receptor_noteskin(&self) -> &NoteSkin {
        resolve_noteskin_choice(self.receptor_noteskin.as_ref(), &self.noteskin)
    }

    #[inline(always)]
    pub fn tap_explosion_noteskin_hidden(&self) -> bool {
        tap_explosion_skin_hidden(self.tap_explosion_noteskin.as_ref())
    }

    #[inline(always)]
    pub fn resolved_tap_explosion_noteskin(&self) -> Option<&NoteSkin> {
        resolve_tap_explosion_skin(self.tap_explosion_noteskin.as_ref(), &self.noteskin)
    }

    #[inline(always)]
    pub fn tap_explosion_window_enabled(&self, window: &str) -> bool {
        tap_explosion_mask_enabled(self.tap_explosion_active_mask, window)
    }

    #[inline(always)]
    pub fn current_player_options(&self) -> PlayerOptionsData {
        PlayerOptionsData {
            background_filter: self.background_filter,
            hold_judgment_graphic: self.hold_judgment_graphic.clone(),
            held_miss_graphic: self.held_miss_graphic.clone(),
            judgment_graphic: self.judgment_graphic.clone(),
            combo_font: self.combo_font,
            combo_colors: self.combo_colors,
            combo_mode: self.combo_mode,
            carry_combo_between_songs: self.carry_combo_between_songs,
            noteskin: self.noteskin.clone(),
            mine_noteskin: self.mine_noteskin.clone(),
            receptor_noteskin: self.receptor_noteskin.clone(),
            tap_explosion_noteskin: self.tap_explosion_noteskin.clone(),
            tap_explosion_active_mask: self.tap_explosion_active_mask,
            scroll_speed: self.scroll_speed,
            no_cmod_alternative: self.no_cmod_alternative,
            scroll_option: self.scroll_option,
            reverse_scroll: self.reverse_scroll,
            turn_option: self.turn_option,
            insert_active_mask: self.insert_active_mask,
            remove_active_mask: self.remove_active_mask,
            holds_active_mask: self.holds_active_mask,
            accel_effects_active_mask: self.accel_effects_active_mask,
            visual_effects_active_mask: self.visual_effects_active_mask,
            appearance_effects_active_mask: self.appearance_effects_active_mask,
            attack_mode: self.attack_mode,
            hide_light_type: self.hide_light_type,
            rescore_early_hits: self.rescore_early_hits,
            hide_early_dw_judgments: self.hide_early_dw_judgments,
            hide_early_dw_flash: self.hide_early_dw_flash,
            hide_early_dw_column_flash: self.hide_early_dw_column_flash,
            timing_windows: self.timing_windows,
            show_fa_plus_window: self.show_fa_plus_window,
            show_ex_score: self.show_ex_score,
            show_hard_ex_score: self.show_hard_ex_score,
            show_fa_plus_pane: self.show_fa_plus_pane,
            fa_plus_10ms_blue_window: self.fa_plus_10ms_blue_window,
            split_15_10ms: self.split_15_10ms,
            track_early_judgments: self.track_early_judgments,
            scale_scatterplot: self.scale_scatterplot,
            scatterplot_max_window: self.scatterplot_max_window,
            score_position: self.score_position,
            score_display_mode: self.score_display_mode,
            custom_fantastic_window: self.custom_fantastic_window,
            custom_fantastic_window_ms: self.custom_fantastic_window_ms,
            pad_light_brightness: self.pad_light_brightness,
            judgment_tilt: self.judgment_tilt,
            column_cues: self.column_cues,
            measure_cues: self.measure_cues,
            crossover_cues: self.crossover_cues,
            crossover_cue_duration_ms: self.crossover_cue_duration_ms,
            crossover_cue_quantization: self.crossover_cue_quantization,
            crossover_cue_brackets: self.crossover_cue_brackets,
            column_countdown: self.column_countdown,
            judgment_back: self.judgment_back,
            error_ms_display: self.error_ms_display,
            display_scorebox: self.display_scorebox,
            live_timing_stats: self.live_timing_stats,
            live_timing_stats_mask: self.live_timing_stats_mask,
            rainbow_max: self.rainbow_max,
            responsive_colors: self.responsive_colors,
            show_life_percent: self.show_life_percent,
            tilt_multiplier: self.tilt_multiplier,
            tilt_min_threshold_ms: self.tilt_min_threshold_ms,
            tilt_max_threshold_ms: self.tilt_max_threshold_ms,
            error_bar_active_mask: self.error_bar_active_mask,
            error_bar: self.error_bar,
            error_bar_text: self.error_bar_text,
            text_error_bar_scalable: self.text_error_bar_scalable,
            text_error_bar_threshold_ms: self.text_error_bar_threshold_ms,
            error_bar_up: self.error_bar_up,
            error_bar_multi_tick: self.error_bar_multi_tick,
            error_bar_trim: self.error_bar_trim,
            center_tick: self.center_tick,
            short_average_error_bar_enabled: self.short_average_error_bar_enabled,
            average_error_bar_intensity: self.average_error_bar_intensity,
            average_error_bar_interval_ms: self.average_error_bar_interval_ms,
            long_error_bar_enabled: self.long_error_bar_enabled,
            long_error_bar_intensity: self.long_error_bar_intensity,
            long_error_bar_threshold_ms: self.long_error_bar_threshold_ms,
            long_error_bar_min_samples: self.long_error_bar_min_samples,
            step_statistics: self.step_statistics,
            step_stats_extra: self.step_stats_extra,
            target_score: self.target_score,
            lifemeter_type: self.lifemeter_type,
            measure_counter: self.measure_counter,
            measure_counter_lookahead: self.measure_counter_lookahead,
            measure_counter_left: self.measure_counter_left,
            measure_counter_up: self.measure_counter_up,
            measure_counter_vert: self.measure_counter_vert,
            broken_run: self.broken_run,
            run_timer: self.run_timer,
            measure_lines: self.measure_lines,
            hide_targets: self.hide_targets,
            hide_song_bg: self.hide_song_bg,
            hide_combo: self.hide_combo,
            hide_lifebar: self.hide_lifebar,
            hide_score: self.hide_score,
            hide_danger: self.hide_danger,
            hide_combo_explosions: self.hide_combo_explosions,
            hide_username: self.hide_username,
            column_flash_on_miss: self.column_flash_on_miss,
            column_flash_mask: self.column_flash_mask,
            column_flash_brightness: self.column_flash_brightness,
            column_flash_size: self.column_flash_size,
            subtractive_scoring: self.subtractive_scoring,
            pacemaker: self.pacemaker,
            nps_graph_at_top: self.nps_graph_at_top,
            transparent_density_graph_bg: self.transparent_density_graph_bg,
            smx_fsr_display: self.smx_fsr_display,
            smx_pad_input_display: self.smx_pad_input_display,
            smx_bg_pack: self.smx_bg_pack.clone(),
            smx_judge_pack: self.smx_judge_pack.clone(),
            mini_indicator: self.mini_indicator,
            mini_indicator_score_type: self.mini_indicator_score_type,
            mini_indicator_subtractive_display: self.mini_indicator_subtractive_display,
            mini_indicator_size: self.mini_indicator_size,
            mini_indicator_color: self.mini_indicator_color,
            mini_indicator_position: self.mini_indicator_position,
            mini_percent: self.mini_percent,
            spacing_percent: self.spacing_percent,
            perspective: self.perspective,
            note_field_offset_x: self.note_field_offset_x,
            note_field_offset_y: self.note_field_offset_y,
            judgment_offset_x: self.judgment_offset_x,
            judgment_offset_y: self.judgment_offset_y,
            combo_offset_x: self.combo_offset_x,
            combo_offset_y: self.combo_offset_y,
            error_bar_offset_x: self.error_bar_offset_x,
            error_bar_offset_y: self.error_bar_offset_y,
            visual_delay_ms: self.visual_delay_ms,
            global_offset_shift_ms: self.global_offset_shift_ms,
        }
    }

    fn apply_player_options(&mut self, options: &PlayerOptionsData) {
        self.background_filter = options.background_filter;
        self.hold_judgment_graphic = options.hold_judgment_graphic.clone();
        self.held_miss_graphic = options.held_miss_graphic.clone();
        self.judgment_graphic = options.judgment_graphic.clone();
        self.combo_font = options.combo_font;
        self.combo_colors = options.combo_colors;
        self.combo_mode = options.combo_mode;
        self.carry_combo_between_songs = options.carry_combo_between_songs;
        self.noteskin = options.noteskin.clone();
        self.mine_noteskin.clone_from(&options.mine_noteskin);
        self.receptor_noteskin
            .clone_from(&options.receptor_noteskin);
        self.tap_explosion_noteskin
            .clone_from(&options.tap_explosion_noteskin);
        self.tap_explosion_active_mask = options.tap_explosion_active_mask;
        self.scroll_speed = options.scroll_speed;
        self.no_cmod_alternative = options.no_cmod_alternative;
        self.scroll_option = options.scroll_option;
        self.reverse_scroll = options.reverse_scroll;
        self.turn_option = options.turn_option;
        self.insert_active_mask = options.insert_active_mask;
        self.remove_active_mask = options.remove_active_mask;
        self.holds_active_mask = options.holds_active_mask;
        self.accel_effects_active_mask = options.accel_effects_active_mask;
        self.visual_effects_active_mask = options.visual_effects_active_mask;
        self.appearance_effects_active_mask = options.appearance_effects_active_mask;
        self.attack_mode = options.attack_mode;
        self.hide_light_type = options.hide_light_type;
        self.rescore_early_hits = options.rescore_early_hits;
        self.hide_early_dw_judgments = options.hide_early_dw_judgments;
        self.hide_early_dw_flash = options.hide_early_dw_flash;
        self.hide_early_dw_column_flash = options.hide_early_dw_column_flash;
        self.timing_windows = options.timing_windows;
        self.show_fa_plus_window = options.show_fa_plus_window;
        self.show_ex_score = options.show_ex_score;
        self.show_hard_ex_score = options.show_hard_ex_score;
        self.show_fa_plus_pane = options.show_fa_plus_pane;
        self.fa_plus_10ms_blue_window = options.fa_plus_10ms_blue_window;
        self.split_15_10ms = options.split_15_10ms;
        self.track_early_judgments = options.track_early_judgments;
        self.scale_scatterplot = options.scale_scatterplot;
        self.scatterplot_max_window = options.scatterplot_max_window;
        self.score_position = options.score_position;
        self.score_display_mode = options.score_display_mode;
        self.custom_fantastic_window = options.custom_fantastic_window;
        self.custom_fantastic_window_ms = options.custom_fantastic_window_ms;
        self.pad_light_brightness = options.pad_light_brightness;
        self.judgment_tilt = options.judgment_tilt;
        self.column_cues = options.column_cues;
        self.measure_cues = options.measure_cues;
        self.crossover_cues = options.crossover_cues;
        self.crossover_cue_duration_ms = options.crossover_cue_duration_ms;
        self.crossover_cue_quantization = options.crossover_cue_quantization;
        self.crossover_cue_brackets = options.crossover_cue_brackets;
        self.column_countdown = options.column_countdown;
        self.judgment_back = options.judgment_back;
        self.error_ms_display = options.error_ms_display;
        self.display_scorebox = options.display_scorebox;
        self.live_timing_stats = options.live_timing_stats;
        self.live_timing_stats_mask = options.live_timing_stats_mask;
        self.rainbow_max = options.rainbow_max;
        self.responsive_colors = options.responsive_colors;
        self.show_life_percent = options.show_life_percent;
        self.tilt_multiplier = options.tilt_multiplier;
        self.tilt_min_threshold_ms = options.tilt_min_threshold_ms;
        self.tilt_max_threshold_ms = options.tilt_max_threshold_ms;
        self.error_bar_active_mask = options.error_bar_active_mask;
        self.error_bar = options.error_bar;
        self.error_bar_text = options.error_bar_text;
        self.text_error_bar_scalable = options.text_error_bar_scalable;
        self.text_error_bar_threshold_ms = options.text_error_bar_threshold_ms;
        self.error_bar_up = options.error_bar_up;
        self.error_bar_multi_tick = options.error_bar_multi_tick;
        self.error_bar_trim = options.error_bar_trim;
        self.center_tick = options.center_tick;
        self.short_average_error_bar_enabled = options.short_average_error_bar_enabled;
        self.average_error_bar_intensity = options.average_error_bar_intensity;
        self.average_error_bar_interval_ms = options.average_error_bar_interval_ms;
        self.long_error_bar_enabled = options.long_error_bar_enabled;
        self.long_error_bar_intensity = options.long_error_bar_intensity;
        self.long_error_bar_threshold_ms = options.long_error_bar_threshold_ms;
        self.long_error_bar_min_samples = options.long_error_bar_min_samples;
        self.step_statistics = options.step_statistics;
        self.step_stats_extra = options.step_stats_extra;
        self.target_score = options.target_score;
        self.lifemeter_type = options.lifemeter_type;
        self.measure_counter = options.measure_counter;
        self.measure_counter_lookahead = options.measure_counter_lookahead;
        self.measure_counter_left = options.measure_counter_left;
        self.measure_counter_up = options.measure_counter_up;
        self.measure_counter_vert = options.measure_counter_vert;
        self.broken_run = options.broken_run;
        self.run_timer = options.run_timer;
        self.measure_lines = options.measure_lines;
        self.hide_targets = options.hide_targets;
        self.hide_song_bg = options.hide_song_bg;
        self.hide_combo = options.hide_combo;
        self.hide_lifebar = options.hide_lifebar;
        self.hide_score = options.hide_score;
        self.hide_danger = options.hide_danger;
        self.hide_combo_explosions = options.hide_combo_explosions;
        self.hide_username = options.hide_username;
        self.column_flash_on_miss = options.column_flash_on_miss;
        self.column_flash_mask = options.column_flash_mask;
        self.column_flash_brightness = options.column_flash_brightness;
        self.column_flash_size = options.column_flash_size;
        self.subtractive_scoring = options.subtractive_scoring;
        self.pacemaker = options.pacemaker;
        self.nps_graph_at_top = options.nps_graph_at_top;
        self.transparent_density_graph_bg = options.transparent_density_graph_bg;
        self.smx_fsr_display = options.smx_fsr_display;
        self.smx_pad_input_display = options.smx_pad_input_display;
        self.smx_bg_pack = options.smx_bg_pack.clone();
        self.smx_judge_pack = options.smx_judge_pack.clone();
        self.mini_indicator = options.mini_indicator;
        self.mini_indicator_score_type = options.mini_indicator_score_type;
        self.mini_indicator_subtractive_display = options.mini_indicator_subtractive_display;
        self.mini_indicator_size = options.mini_indicator_size;
        self.mini_indicator_color = options.mini_indicator_color;
        self.mini_indicator_position = options.mini_indicator_position;
        self.mini_percent = options.mini_percent;
        self.spacing_percent = options.spacing_percent;
        self.perspective = options.perspective;
        self.note_field_offset_x = options.note_field_offset_x;
        self.note_field_offset_y = options.note_field_offset_y;
        self.judgment_offset_x = options.judgment_offset_x;
        self.judgment_offset_y = options.judgment_offset_y;
        self.combo_offset_x = options.combo_offset_x;
        self.combo_offset_y = options.combo_offset_y;
        self.error_bar_offset_x = options.error_bar_offset_x;
        self.error_bar_offset_y = options.error_bar_offset_y;
        self.visual_delay_ms = options.visual_delay_ms;
        self.global_offset_shift_ms = options.global_offset_shift_ms;
    }

    #[inline(always)]
    pub const fn player_options(&self, style: PlayStyle) -> &PlayerOptionsData {
        match style {
            PlayStyle::Single | PlayStyle::Versus => &self.player_options_singles,
            PlayStyle::Double => &self.player_options_doubles,
        }
    }

    #[inline(always)]
    pub fn player_options_mut(&mut self, style: PlayStyle) -> &mut PlayerOptionsData {
        match style {
            PlayStyle::Single | PlayStyle::Versus => &mut self.player_options_singles,
            PlayStyle::Double => &mut self.player_options_doubles,
        }
    }

    pub fn store_current_player_options(&mut self, style: PlayStyle) {
        let options = self.current_player_options();
        *self.player_options_mut(style) = options;
    }

    pub fn store_current_player_options_for_all_styles(&mut self) {
        let options = self.current_player_options();
        self.player_options_singles = options.clone();
        self.player_options_doubles = options;
    }

    pub fn apply_player_options_for_style(&mut self, style: PlayStyle) {
        let options = self.player_options(style).clone();
        self.apply_player_options(&options);
    }

    #[inline(always)]
    pub const fn last_played(&self, style: PlayStyle) -> &LastPlayed {
        match style {
            PlayStyle::Single | PlayStyle::Versus => &self.last_played_singles,
            PlayStyle::Double => &self.last_played_doubles,
        }
    }

    #[inline(always)]
    pub fn last_played_mut(&mut self, style: PlayStyle) -> &mut LastPlayed {
        match style {
            PlayStyle::Single | PlayStyle::Versus => &mut self.last_played_singles,
            PlayStyle::Double => &mut self.last_played_doubles,
        }
    }

    #[inline(always)]
    pub const fn last_played_course(&self, style: PlayStyle) -> &LastPlayedCourse {
        match style {
            PlayStyle::Single | PlayStyle::Versus => &self.last_played_course_singles,
            PlayStyle::Double => &self.last_played_course_doubles,
        }
    }

    #[inline(always)]
    pub fn last_played_course_mut(&mut self, style: PlayStyle) -> &mut LastPlayedCourse {
        match style {
            PlayStyle::Single | PlayStyle::Versus => &mut self.last_played_course_singles,
            PlayStyle::Double => &mut self.last_played_course_doubles,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_style_reports_chart_type() {
        assert_eq!(PlayStyle::Single.chart_type(), "dance-single");
        assert_eq!(PlayStyle::Versus.chart_type(), "dance-single");
        assert_eq!(PlayStyle::Double.chart_type(), "dance-double");
        assert_eq!(PlayStyle::Single.cols_per_player(), 4);
        assert_eq!(PlayStyle::Versus.cols_per_player(), 4);
        assert_eq!(PlayStyle::Double.cols_per_player(), 8);
        assert_eq!(PlayStyle::Single.player_count(), 1);
        assert_eq!(PlayStyle::Versus.player_count(), 2);
        assert_eq!(PlayStyle::Double.player_count(), 1);
        assert_eq!(PlayStyle::Single.total_cols(), 4);
        assert_eq!(PlayStyle::Versus.total_cols(), 8);
        assert_eq!(PlayStyle::Double.total_cols(), 8);
    }

    #[test]
    fn defaults_match_single_player_session() {
        assert_eq!(PLAYER_SLOTS, 2);
        assert_eq!(DEFAULT_WEIGHT_POUNDS, 120);
        assert_eq!(DEFAULT_BIRTH_YEAR, 1995);
        assert_eq!(PLAYER_INITIALS_MAX_LEN, 4);
        assert_eq!((HUD_OFFSET_MIN, HUD_OFFSET_MAX), (-250, 250));
        assert_eq!((SPACING_PERCENT_MIN, SPACING_PERCENT_MAX), (-100, 100));
        assert_eq!((MINI_PERCENT_MIN, MINI_PERCENT_MAX), (-100, 150));
        assert_eq!((NOTE_FIELD_OFFSET_X_MIN, NOTE_FIELD_OFFSET_X_MAX), (0, 50));
        assert_eq!(
            (NOTE_FIELD_OFFSET_Y_MIN, NOTE_FIELD_OFFSET_Y_MAX),
            (-50, 50)
        );
        assert_eq!((VISUAL_DELAY_MS_MIN, VISUAL_DELAY_MS_MAX), (-100, 100));
        assert_eq!((TILT_THRESHOLD_MIN_MS, TILT_THRESHOLD_MAX_MS), (0, 100));
        assert_eq!(
            (TILT_MIN_THRESHOLD_DEFAULT_MS, TILT_MAX_THRESHOLD_DEFAULT_MS),
            (0, 50)
        );
        assert_eq!(
            (
                CUSTOM_FANTASTIC_WINDOW_MIN_MS,
                CUSTOM_FANTASTIC_WINDOW_MAX_MS,
                CUSTOM_FANTASTIC_WINDOW_DEFAULT_MS
            ),
            (1, 22, 10)
        );
        assert_eq!(
            (
                TEXT_ERROR_BAR_THRESHOLD_MS_MIN,
                TEXT_ERROR_BAR_THRESHOLD_MS_MAX,
                TEXT_ERROR_BAR_THRESHOLD_MS_DEFAULT
            ),
            (1, 50, 10)
        );
        assert_eq!(PlayStyle::default(), PlayStyle::Single);
        assert_eq!(PlayMode::default(), PlayMode::Regular);
        assert_eq!(PlayerSide::default(), PlayerSide::P1);
        assert_eq!(TimingTickMode::default(), TimingTickMode::Off);

        let options = PlayerOptionsData::default();
        assert_eq!(options.scroll_speed, ScrollSpeedSetting::default());
        assert!(options.carry_combo_between_songs);
        assert!(options.rescore_early_hits);
        assert!(options.display_scorebox);
        assert!(options.short_average_error_bar_enabled);
        assert!(options.long_error_bar_enabled);
        assert!(!options.text_error_bar_scalable);
        assert_eq!(
            options.text_error_bar_threshold_ms,
            TEXT_ERROR_BAR_THRESHOLD_MS_DEFAULT
        );
        assert!(options.step_statistics.is_empty());
        assert_eq!(options.step_stats_extra, StepStatsExtra::None);
        assert_eq!(options.score_position, ScorePosition::Normal);
        assert_eq!(options.score_display_mode, ScoreDisplayMode::Normal);
        assert_eq!(options.measure_counter_lookahead, 2);
        assert!(options.measure_counter_left);
        assert_eq!(options.tap_explosion_active_mask, TapExplosionMask::all());
        assert_eq!(options.column_flash_mask, DEFAULT_COLUMN_FLASH_MASK);
        assert_eq!(
            options.column_flash_brightness,
            ColumnFlashBrightness::Normal
        );
        assert_eq!(options.column_flash_size, ColumnFlashSize::Default);
    }

    #[test]
    fn score_import_credentials_select_endpoint_fields() {
        let mut profile = Profile::default();
        profile.groovestats_api_key = " gs-key ".to_string();
        profile.groovestats_username = " player ".to_string();
        profile.arrowcloud_api_key = " ac-key ".to_string();

        assert_eq!(
            profile.score_import_api_key(ScoreImportEndpoint::GrooveStats),
            "gs-key"
        );
        assert_eq!(
            profile.score_import_api_key(ScoreImportEndpoint::BoogieStats),
            "gs-key"
        );
        assert_eq!(
            profile.score_import_api_key(ScoreImportEndpoint::ArrowCloud),
            "ac-key"
        );
        assert_eq!(
            profile.score_import_username(ScoreImportEndpoint::GrooveStats),
            "player"
        );
        assert_eq!(
            profile.score_import_username(ScoreImportEndpoint::ArrowCloud),
            ""
        );
        assert!(profile.has_score_import_credentials(ScoreImportEndpoint::GrooveStats));
        assert!(profile.has_score_import_credentials(ScoreImportEndpoint::ArrowCloud));

        profile.groovestats_username.clear();
        assert!(!profile.has_score_import_credentials(ScoreImportEndpoint::GrooveStats));
        assert!(profile.has_score_import_credentials(ScoreImportEndpoint::ArrowCloud));
    }

    #[test]
    fn scorebox_profile_snapshot_uses_profile_fields_and_service_flags() {
        let mut profile = Profile::default();
        profile.display_scorebox = true;
        profile.show_ex_score = true;
        profile.groovestats_api_key = " gs-key ".to_string();
        profile.arrowcloud_api_key = " ac-key ".to_string();
        profile.groovestats_username = " player ".to_string();

        let snapshot =
            scorebox_profile_snapshot(&profile, true, true, true, true, Some("profile-1".into()));
        assert!(snapshot.display_scorebox);
        assert!(snapshot.gs_active);
        assert!(snapshot.show_ex_score);
        assert_eq!(snapshot.api_key(), "gs-key");
        assert_eq!(snapshot.arrowcloud_api_key(), "ac-key");
        assert!(snapshot.include_arrowcloud());
        assert_eq!(snapshot.gs_username(), "player");
        assert_eq!(snapshot.persistent_profile_id(), Some("profile-1"));
        assert_eq!(snapshot.auto_profile_id(), Some("profile-1"));
        assert!(snapshot.should_auto_populate());

        let disabled =
            scorebox_profile_snapshot(&profile, true, false, false, true, Some("profile-1".into()));
        assert!(!disabled.gs_active);
        assert!(!disabled.include_arrowcloud());
    }

    #[test]
    fn groovestats_side_active_requires_enabled_joined_key() {
        assert!(!groovestats_side_active(false, true, "gs-key"));
        assert!(!groovestats_side_active(true, false, "gs-key"));
        assert!(!groovestats_side_active(true, true, "   "));
        assert!(groovestats_side_active(true, true, " gs-key "));
    }

    #[test]
    fn scorebox_side_snapshot_uses_loaded_side_and_join_state() {
        let active_profiles = [
            ActiveProfile::Local {
                id: "p1-profile".to_string(),
            },
            ActiveProfile::Local {
                id: "p2-profile".to_string(),
            },
        ];
        let mut profiles = [Profile::default(), Profile::default()];
        profiles[1].display_scorebox = true;
        profiles[1].show_ex_score = true;
        profiles[1].groovestats_api_key = " gs-key ".to_string();
        profiles[1].arrowcloud_api_key = " ac-key ".to_string();
        profiles[1].groovestats_username = " player ".to_string();

        let p2 = scorebox_profile_snapshot_for_side(
            &profiles,
            &active_profiles,
            SESSION_JOINED_MASK_P2,
            PlayerSide::P2,
            true,
            true,
            true,
        );
        assert!(p2.gs_active);
        assert!(p2.include_arrowcloud());
        assert_eq!(p2.persistent_profile_id(), Some("p2-profile"));
        assert_eq!(p2.auto_profile_id(), Some("p2-profile"));

        let p1 = scorebox_profile_snapshot_for_side(
            &profiles,
            &active_profiles,
            SESSION_JOINED_MASK_P2,
            PlayerSide::P1,
            true,
            true,
            true,
        );
        assert!(!p1.gs_active);
        assert_eq!(p1.persistent_profile_id(), Some("p1-profile"));
    }

    #[test]
    fn scorebox_runtime_view_reads_both_sides_from_one_session() {
        let active_profiles = [
            ActiveProfile::Local {
                id: "p1-profile".to_string(),
            },
            ActiveProfile::Local {
                id: "p2-profile".to_string(),
            },
        ];
        let mut profiles = [Profile::default(), Profile::default()];
        profiles[0].display_name = "Player One".to_string();
        profiles[0].player_initials = "P1".to_string();
        profiles[0].groovestats_username = "one".to_string();
        profiles[0].groovestats_api_key = "key-one".to_string();
        profiles[1].display_name = "Player Two".to_string();
        profiles[1].player_initials = "P2".to_string();
        profiles[1].groovestats_username = "two".to_string();
        profiles[1].groovestats_api_key = "key-two".to_string();

        let view = scorebox_runtime_view(
            &profiles,
            &active_profiles,
            SESSION_JOINED_MASK_P2,
            PlayStyle::Versus,
            PlayerSide::P2,
            true,
            false,
            true,
        );

        assert_eq!(view.play_style, PlayStyle::Versus);
        assert_eq!(view.player_side, PlayerSide::P2);
        assert!(!view.sides[0].joined);
        assert!(!view.sides[0].leaderboard.gs_active);
        assert_eq!(view.sides[0].display_name, "Player One");
        assert_eq!(view.sides[0].player_initials, "P1");
        assert!(view.sides[1].joined);
        assert!(view.sides[1].leaderboard.gs_active);
        assert_eq!(view.sides[1].leaderboard.gs_username(), "two");
        assert_eq!(
            view.sides[1].leaderboard.persistent_profile_id(),
            Some("p2-profile")
        );
    }

    #[test]
    fn session_players_view_reads_names_and_join_state_together() {
        let mut profiles = [Profile::default(), Profile::default()];
        profiles[0].display_name = "Alice".to_string();
        profiles[1].display_name = "Bob".to_string();

        let view = session_players_view(&profiles, SESSION_JOINED_MASK_P1, PlayerSide::P2);

        assert_eq!(view.active_side, PlayerSide::P2);
        assert_eq!(view.joined, [true, false]);
        assert_eq!(view.display_names, ["Alice", "Bob"]);
    }

    #[test]
    fn favorite_membership_batches_both_sides() {
        let mut profiles = [Profile::default(), Profile::default()];
        profiles[0].favorited_packs.insert("Pack A".to_string());
        profiles[1].favorited_packs.insert("Pack B".to_string());
        let queries = [
            FavoriteMembershipQuery::Pack(Some("Pack A")),
            FavoriteMembershipQuery::Pack(Some("Pack B")),
            FavoriteMembershipQuery::Pack(None),
            FavoriteMembershipQuery::None,
        ];

        assert_eq!(
            favorite_membership(&profiles, &queries),
            [[true, false], [false, true], [false, false], [false, false]]
        );
    }

    #[test]
    fn profile_combo_carry_uses_loaded_profile_combos() {
        let mut profiles = [Profile::default(), Profile::default()];
        profiles[0].current_combo = 12;
        profiles[1].current_combo = 34;

        assert_eq!(profile_combo_carry(&profiles), [12, 34]);
    }

    #[test]
    fn preferred_difficulty_indices_clamp_to_standard_range() {
        let mut profiles = [Profile::default(), Profile::default()];
        profiles[0]
            .last_played_mut(PlayStyle::Single)
            .difficulty_index = 2;
        profiles[1]
            .last_played_mut(PlayStyle::Single)
            .difficulty_index = usize::MAX;

        assert_eq!(
            preferred_difficulty_indices(&profiles, PlayStyle::Single),
            [2, deadsync_chart::STANDARD_DIFFICULTY_COUNT - 1],
        );
        assert_eq!(
            preferred_difficulty_index_for_side(&profiles, PlayerSide::P2, PlayStyle::Single),
            deadsync_chart::STANDARD_DIFFICULTY_COUNT - 1,
        );
    }

    #[test]
    fn profile_last_played_updates_style_entry() {
        let mut profile = Profile::default();

        assert!(profile.set_last_played(
            PlayStyle::Single,
            Some("Songs/Pack/Song.ogg".to_string()),
            Some("hash-a".to_string()),
            4,
        ));
        assert_eq!(
            profile
                .last_played(PlayStyle::Versus)
                .song_music_path
                .as_deref(),
            Some("Songs/Pack/Song.ogg")
        );
        assert_eq!(
            profile.last_played(PlayStyle::Single).chart_hash.as_deref(),
            Some("hash-a")
        );
        assert_eq!(profile.last_played(PlayStyle::Single).difficulty_index, 4);
        assert!(!profile.set_last_played(
            PlayStyle::Versus,
            Some("Songs/Pack/Song.ogg".to_string()),
            Some("hash-a".to_string()),
            4,
        ));

        assert!(profile.set_last_played(
            PlayStyle::Double,
            Some("Songs/Pack/Double.ogg".to_string()),
            None,
            1,
        ));
        assert_eq!(
            profile
                .last_played(PlayStyle::Double)
                .song_music_path
                .as_deref(),
            Some("Songs/Pack/Double.ogg")
        );
        assert_eq!(
            profile
                .last_played(PlayStyle::Single)
                .song_music_path
                .as_deref(),
            Some("Songs/Pack/Song.ogg")
        );
    }

    #[test]
    fn profile_last_played_course_updates_style_entry() {
        let mut profile = Profile::default();

        assert!(profile.set_last_played_course(
            PlayStyle::Single,
            Some("Courses/Course.crs".to_string()),
            Some("Hard".to_string()),
        ));
        assert_eq!(
            profile
                .last_played_course(PlayStyle::Versus)
                .course_path
                .as_deref(),
            Some("Courses/Course.crs")
        );
        assert_eq!(
            profile
                .last_played_course(PlayStyle::Single)
                .difficulty_name
                .as_deref(),
            Some("Hard")
        );
        assert!(!profile.set_last_played_course(
            PlayStyle::Versus,
            Some("Courses/Course.crs".to_string()),
            Some("Hard".to_string()),
        ));

        assert!(profile.set_last_played_course(
            PlayStyle::Double,
            Some("Courses/Double.crs".to_string()),
            Some("Challenge".to_string()),
        ));
        assert!(profile.set_last_played_course(PlayStyle::Double, None, None));
        assert_eq!(
            profile.last_played_course(PlayStyle::Double).course_path,
            None
        );
    }

    #[test]
    fn profile_stage_calories_reset_day_and_ignore_invalid() {
        let mut profile = Profile {
            calories_burned_day: "2026-06-01".to_string(),
            calories_burned_today: 12.0,
            ..Profile::default()
        };

        assert!(profile.add_stage_calories_for_day("2026-06-02", 3.5));
        assert_eq!(profile.calories_burned_day, "2026-06-02");
        assert!((profile.calories_burned_today - 3.5).abs() < 1e-6);

        assert!(!profile.add_stage_calories_for_day("2026-06-02", f32::NAN));
        assert!((profile.calories_burned_today - 3.5).abs() < 1e-6);

        profile.ignore_step_count_calories = true;
        assert!(!profile.add_stage_calories_for_day("2026-06-02", 10.0));
        assert!((profile.calories_burned_today - 3.5).abs() < 1e-6);
    }

    #[test]
    fn profile_player_initials_sanitize_and_skip_empty() {
        let mut profile = Profile::default();

        assert!(profile.set_player_initials("a b-c"));
        assert_eq!(profile.player_initials, "ABC");
        assert!(!profile.set_player_initials("abc"));
        assert!(!profile.set_player_initials("    "));
        assert_eq!(profile.player_initials, "ABC");
    }

    #[test]
    fn profile_scroll_option_updates_reverse_flag() {
        let mut profile = Profile::default();

        assert!(profile.set_scroll_option(ScrollOption::Reverse));
        assert_eq!(profile.scroll_option, ScrollOption::Reverse);
        assert!(profile.reverse_scroll);
        assert!(!profile.set_scroll_option(ScrollOption::Reverse));

        assert!(profile.set_scroll_option(ScrollOption::Normal));
        assert_eq!(profile.scroll_option, ScrollOption::Normal);
        assert!(!profile.reverse_scroll);
    }

    #[test]
    fn evaluation_mods_text_formats_profile_options() {
        let profile = Profile {
            mini_percent: 35,
            spacing_percent: -20,
            scroll_option: ScrollOption::Reverse
                .union(ScrollOption::Split)
                .union(ScrollOption::Cross),
            perspective: Perspective::Incoming,
            timing_windows: TimingWindowsOption::DecentsAndWayOffs,
            noteskin: NoteSkin::new("cyber"),
            ..Profile::default()
        };

        assert_eq!(
            evaluation_mods_text(&profile, ScrollSpeedSetting::XMod(2.5)).as_ref(),
            "X2.50, 35% Mini, -20% Spacing, Reverse, Split, Cross, Incoming, No W4/W5, cyber"
        );
    }

    #[test]
    fn profile_gameplay_extras_sync_mini_indicator() {
        let mut profile = Profile::default();

        assert!(profile.set_gameplay_extras(false, true, false, false));
        assert!(profile.subtractive_scoring);
        assert_eq!(profile.mini_indicator, MiniIndicator::SubtractiveScoring);

        assert!(profile.set_gameplay_extras(false, false, true, false));
        assert!(!profile.subtractive_scoring);
        assert!(profile.pacemaker);
        assert_eq!(profile.mini_indicator, MiniIndicator::Pacemaker);

        assert!(profile.set_gameplay_extras(false, false, false, false));
        assert_eq!(profile.mini_indicator, MiniIndicator::None);
        assert!(!profile.set_gameplay_extras(false, false, false, false));
    }

    #[test]
    fn profile_grouped_visibility_options_update_together() {
        let mut profile = Profile::default();

        assert!(profile.set_early_dw_options(true, true, true));
        assert!(profile.hide_early_dw_judgments);
        assert!(profile.hide_early_dw_flash);
        assert!(profile.hide_early_dw_column_flash);
        assert!(!profile.set_early_dw_options(true, true, true));

        assert!(profile.set_hide_options(true, true, false, true, false, true, true, true));
        assert!(profile.hide_targets);
        assert!(profile.hide_song_bg);
        assert!(!profile.hide_combo);
        assert!(profile.hide_lifebar);
        assert!(!profile.hide_score);
        assert!(profile.hide_danger);
        assert!(profile.hide_combo_explosions);
        assert!(profile.hide_username);
        assert!(!profile.set_hide_options(true, true, false, true, false, true, true, true));
    }

    #[test]
    fn profile_tilt_thresholds_clamp_and_order() {
        let mut profile = Profile::default();

        assert!(profile.set_tilt_thresholds(120, 10));
        assert_eq!(profile.tilt_min_threshold_ms, TILT_THRESHOLD_MAX_MS);
        assert_eq!(profile.tilt_max_threshold_ms, TILT_THRESHOLD_MAX_MS);
        assert!(!profile.set_tilt_thresholds(120, 10));
    }

    #[test]
    fn profile_error_bar_mask_syncs_legacy_fields() {
        let mut profile = Profile::default();
        let mask = ErrorBarMask::MONOCHROME | ErrorBarMask::TEXT;

        assert!(profile.set_error_bar_mask(mask));
        assert_eq!(profile.error_bar_active_mask, mask);
        assert_eq!(profile.error_bar, ErrorBarStyle::Monochrome);
        assert!(profile.error_bar_text);
        assert!(!profile.set_error_bar_mask(mask));
    }

    #[test]
    fn profile_position_offsets_clamp_ranges() {
        let mut profile = Profile::default();

        assert!(profile.set_note_field_offset_x(NOTE_FIELD_OFFSET_X_MAX + 1));
        assert_eq!(profile.note_field_offset_x, NOTE_FIELD_OFFSET_X_MAX);
        assert!(!profile.set_note_field_offset_x(NOTE_FIELD_OFFSET_X_MAX + 1));

        assert!(profile.set_note_field_offset_y(NOTE_FIELD_OFFSET_Y_MIN - 1));
        assert_eq!(profile.note_field_offset_y, NOTE_FIELD_OFFSET_Y_MIN);

        assert!(profile.set_judgment_offset_x(HUD_OFFSET_MAX + 1));
        assert_eq!(profile.judgment_offset_x, HUD_OFFSET_MAX);
        assert!(profile.set_judgment_offset_y(HUD_OFFSET_MIN - 1));
        assert_eq!(profile.judgment_offset_y, HUD_OFFSET_MIN);

        assert!(profile.set_combo_offset_x(HUD_OFFSET_MAX + 1));
        assert_eq!(profile.combo_offset_x, HUD_OFFSET_MAX);
        assert!(profile.set_combo_offset_y(HUD_OFFSET_MIN - 1));
        assert_eq!(profile.combo_offset_y, HUD_OFFSET_MIN);

        assert!(profile.set_error_bar_offset_x(HUD_OFFSET_MAX + 1));
        assert_eq!(profile.error_bar_offset_x, HUD_OFFSET_MAX);
        assert!(profile.set_error_bar_offset_y(HUD_OFFSET_MIN - 1));
        assert_eq!(profile.error_bar_offset_y, HUD_OFFSET_MIN);
    }

    #[test]
    fn profile_percent_and_timing_offsets_clamp_ranges() {
        let mut profile = Profile::default();

        assert!(profile.set_mini_percent(MINI_PERCENT_MAX + 1));
        assert_eq!(profile.mini_percent, MINI_PERCENT_MAX);
        assert!(!profile.set_mini_percent(MINI_PERCENT_MAX + 1));

        assert!(profile.set_spacing_percent(SPACING_PERCENT_MIN - 1));
        assert_eq!(profile.spacing_percent, SPACING_PERCENT_MIN);

        assert!(profile.set_visual_delay_ms(VISUAL_DELAY_MS_MAX + 1));
        assert_eq!(profile.visual_delay_ms, VISUAL_DELAY_MS_MAX);

        assert!(profile.set_global_offset_shift_ms(VISUAL_DELAY_MS_MIN - 1));
        assert_eq!(profile.global_offset_shift_ms, VISUAL_DELAY_MS_MIN);
    }

    #[test]
    fn profile_tilt_multiplier_rejects_non_finite() {
        let mut profile = Profile::default();

        assert!(!profile.set_tilt_multiplier(f32::NAN));
        assert_eq!(profile.tilt_multiplier, 1.0);
        assert!(!profile.set_tilt_multiplier(f32::INFINITY));
        assert_eq!(profile.tilt_multiplier, 1.0);

        assert!(profile.set_tilt_multiplier(1.25));
        assert_eq!(profile.tilt_multiplier, 1.25);
        assert!(!profile.set_tilt_multiplier(1.25));
    }

    #[test]
    fn profile_error_bar_numeric_settings_normalize() {
        let mut profile = Profile::default();

        assert!(profile.set_custom_fantastic_window_ms(CUSTOM_FANTASTIC_WINDOW_MAX_MS + 1));
        assert_eq!(
            profile.custom_fantastic_window_ms,
            CUSTOM_FANTASTIC_WINDOW_MAX_MS
        );
        assert!(!profile.set_custom_fantastic_window_ms(CUSTOM_FANTASTIC_WINDOW_MAX_MS + 1));

        assert!(profile.set_average_error_bar_intensity(1.13));
        assert!((profile.average_error_bar_intensity - 1.25).abs() < 1e-6);
        assert!(!profile.set_average_error_bar_intensity(1.13));

        assert!(profile.set_average_error_bar_interval_ms(149));
        assert_eq!(profile.average_error_bar_interval_ms, 100);

        assert!(profile.set_text_error_bar_threshold_ms(999));
        assert_eq!(
            profile.text_error_bar_threshold_ms,
            TEXT_ERROR_BAR_THRESHOLD_MS_MAX
        );

        assert!(profile.set_long_error_bar_intensity(1.13));
        assert!((profile.long_error_bar_intensity - 1.25).abs() < 1e-6);

        assert!(profile.set_long_error_bar_threshold_ms(LONG_ERROR_BAR_THRESHOLD_MS_MAX + 1));
        assert_eq!(
            profile.long_error_bar_threshold_ms,
            LONG_ERROR_BAR_THRESHOLD_MS_MAX
        );

        assert!(profile.set_long_error_bar_min_samples(0));
        assert_eq!(
            profile.long_error_bar_min_samples,
            LONG_ERROR_BAR_MIN_SAMPLES_MIN
        );
    }

    #[test]
    fn error_bar_options_load_legacy_flags_and_numeric_aliases() {
        let mut options = PlayerOptionsData::default();
        let values = [
            ("Colorful", "1"),
            ("Text", "1"),
            ("CenterTick", "1"),
            ("LongAvgTickOnly", "1"),
            ("HighlightZoom", "1.13x"),
            ("HighlightAverageMs", "149ms"),
            ("TextErrorBar10ms", "1"),
            ("TextErrorBarThresholdMs", "999ms"),
            ("LongErrorBar", "0"),
            ("LongErrorBarIntensity", "1.95x"),
            ("LongErrorBarThresholdMs", "9999ms"),
            ("LongErrorBarMinSamples", "0"),
        ];

        load_error_bar_options(&mut options, |key| {
            values
                .iter()
                .find_map(|(k, v)| (*k == key).then(|| (*v).to_string()))
        });

        assert!(
            options
                .error_bar_active_mask
                .contains(ErrorBarMask::COLORFUL)
        );
        assert!(options.error_bar_active_mask.contains(ErrorBarMask::TEXT));
        assert_eq!(options.error_bar, ErrorBarStyle::Colorful);
        assert!(options.error_bar_text);
        assert!(options.center_tick);
        assert!(!options.short_average_error_bar_enabled);
        assert!((options.average_error_bar_intensity - 1.25).abs() < 1e-6);
        assert_eq!(options.average_error_bar_interval_ms, 100);
        assert!(options.text_error_bar_scalable);
        assert_eq!(
            options.text_error_bar_threshold_ms,
            TEXT_ERROR_BAR_THRESHOLD_MS_MAX
        );
        assert!(!options.long_error_bar_enabled);
        assert!((options.long_error_bar_intensity - 2.0).abs() < 1e-6);
        assert_eq!(
            options.long_error_bar_threshold_ms,
            LONG_ERROR_BAR_THRESHOLD_MS_MAX
        );
        assert_eq!(
            options.long_error_bar_min_samples,
            LONG_ERROR_BAR_MIN_SAMPLES_MIN
        );
    }

    #[test]
    fn visual_player_options_load_graphics_noteskins_and_offsets() {
        let mut options = PlayerOptionsData::default();
        let values = [
            ("BackgroundFilter", "50"),
            ("HoldJudgmentGraphic", "itg2"),
            ("HeldGraphic", "none"),
            ("JudgmentGraphic", "custom.png"),
            ("ComboFont", "BebasNeue"),
            ("ComboColors", "RainbowScroll"),
            ("ComboMode", "CurrentCombo"),
            ("ComboContinuesBetweenSongs", "1"),
            ("NoteSkin", "default"),
            ("MineSkin", "metal"),
            ("ReceptorSkin", "cyber"),
            ("TapExplosionSkin", "none"),
            ("TapExplosionMask", "63"),
            ("TapExplosionMaskVersion", "1"),
            ("MiniPercent", "42"),
            ("Spacing", "-7"),
            ("Perspective", "Incoming"),
            ("NoteFieldOffsetX", "12"),
            ("NoteFieldOffsetY", "-13"),
            ("JudgmentOffsetX", "14"),
            ("JudgmentOffsetY", "-15"),
            ("ComboOffsetX", "16"),
            ("ComboOffsetY", "-17"),
            ("ErrorBarOffsetX", "18"),
            ("ErrorBarOffsetY", "-19"),
            ("VisualDelay", "21ms"),
            ("GlobalOffsetShiftMs", "-22ms"),
        ];

        load_visual_player_options(&mut options, |key| {
            values
                .iter()
                .find_map(|(k, v)| (*k == key).then(|| (*v).to_string()))
        });

        assert_eq!(
            options.background_filter,
            BackgroundFilter::from_percent(50)
        );
        assert_eq!(
            options.hold_judgment_graphic.as_str(),
            "hold_judgements/ITG2 1x2 (doubleres).png"
        );
        assert_eq!(options.held_miss_graphic.as_str(), "None");
        assert_eq!(options.judgment_graphic.as_str(), "judgements/custom.png");
        assert_eq!(options.combo_font, ComboFont::BebasNeue);
        assert_eq!(options.combo_colors, ComboColors::RainbowScroll);
        assert_eq!(options.combo_mode, ComboMode::CurrentCombo);
        assert!(options.carry_combo_between_songs);
        assert_eq!(options.noteskin, NoteSkin::new("default"));
        assert_eq!(options.mine_noteskin, Some(NoteSkin::new("metal")));
        assert_eq!(options.receptor_noteskin, Some(NoteSkin::new("cyber")));
        assert_eq!(options.tap_explosion_noteskin, Some(NoteSkin::new("none")));
        assert_eq!(options.tap_explosion_active_mask, TapExplosionMask::all());
        assert_eq!(options.mini_percent, 42);
        assert_eq!(options.spacing_percent, -7);
        assert_eq!(options.perspective, Perspective::Incoming);
        assert_eq!(options.note_field_offset_x, 12);
        assert_eq!(options.note_field_offset_y, -13);
        assert_eq!(options.judgment_offset_x, 14);
        assert_eq!(options.judgment_offset_y, -15);
        assert_eq!(options.combo_offset_x, 16);
        assert_eq!(options.combo_offset_y, -17);
        assert_eq!(options.error_bar_offset_x, 18);
        assert_eq!(options.error_bar_offset_y, -19);
        assert_eq!(options.visual_delay_ms, 21);
        assert_eq!(options.global_offset_shift_ms, -22);
    }

    #[test]
    fn timing_feedback_options_load_legacy_aliases_and_clamps() {
        let mut options = PlayerOptionsData::default();
        let values = [
            ("ShowFaPlusWindow", "1"),
            ("ShowExScore", "1"),
            ("ShowHardEXScore", "1"),
            ("ShowFaPlusPane", "1"),
            ("SmallerWhite", "1"),
            ("Split1510ms", "1"),
            ("TrackEarlyJudgments", "1"),
            ("ScatterplotGreatMax", "1"),
            ("ScatterplotMaxWindow", "Excellent"),
            ("ScorePosition", "Step Statistics"),
            ("ScoreDisplay", "Predictive"),
            ("CustomFantasticWindow", "1"),
            ("CustomFantasticWindowMs", "23"),
            ("JudgmentTilt", "1"),
            ("ColumnCues", "1"),
            ("MeasureCues", "1"),
            ("JudgmentBack", "1"),
            ("ErrorMSDisplay", "1"),
            ("DisplayScorebox", "1"),
            ("LiveTimingStats", "1"),
            ("LiveTimingStatsMask", "3"),
            ("RainbowMax", "1"),
            ("ResponsiveColors", "1"),
            ("ShowLifePercent", "1"),
            ("TiltMultiplier", "1.5"),
            ("TiltCutoffMs", "99ms"),
            ("TiltMaxThresholdMs", "50ms"),
        ];

        load_timing_feedback_options(&mut options, |key| {
            values
                .iter()
                .find_map(|(k, v)| (*k == key).then(|| (*v).to_string()))
        });

        assert!(options.show_fa_plus_window);
        assert!(options.show_ex_score);
        assert!(options.show_hard_ex_score);
        assert!(options.show_fa_plus_pane);
        assert!(options.fa_plus_10ms_blue_window);
        assert!(options.split_15_10ms);
        assert!(options.track_early_judgments);
        assert!(options.scale_scatterplot);
        assert_eq!(
            options.scatterplot_max_window,
            ScatterplotMaxWindow::Excellent
        );
        assert_eq!(options.score_position, ScorePosition::StepStatistics);
        assert_eq!(options.score_display_mode, ScoreDisplayMode::Predictive);
        assert!(options.custom_fantastic_window);
        assert_eq!(
            options.custom_fantastic_window_ms,
            CUSTOM_FANTASTIC_WINDOW_MAX_MS
        );
        assert!(options.judgment_tilt);
        assert!(options.column_cues);
        assert!(options.measure_cues);
        assert!(options.judgment_back);
        assert!(options.error_ms_display);
        assert!(options.display_scorebox);
        assert!(options.live_timing_stats);
        assert_eq!(
            options.live_timing_stats_mask,
            LiveTimingStatsMask::MEAN | LiveTimingStatsMask::MEAN_ABS
        );
        assert!(options.rainbow_max);
        assert!(options.responsive_colors);
        assert!(options.show_life_percent);
        assert!((options.tilt_multiplier - 1.5).abs() < f32::EPSILON);
        assert_eq!(options.tilt_min_threshold_ms, 99);
        assert_eq!(options.tilt_max_threshold_ms, 99);

        let mut legacy = PlayerOptionsData::default();
        load_timing_feedback_options(&mut legacy, |key| {
            (key == "LiveTimingStats").then(|| "1".to_string())
        });
        assert!(legacy.live_timing_stats);
        assert_eq!(legacy.live_timing_stats_mask, LiveTimingStatsMask::all());
    }

    #[test]
    fn profile_error_bar_and_measure_counter_options_update() {
        let mut profile = Profile::default();

        assert!(profile.set_error_bar_options(true, true));
        assert!(profile.error_bar_up);
        assert!(profile.error_bar_multi_tick);
        assert!(!profile.set_error_bar_options(true, true));

        assert!(profile.set_measure_counter_lookahead(9));
        assert_eq!(profile.measure_counter_lookahead, 4);
        assert!(!profile.set_measure_counter_lookahead(9));

        assert!(profile.set_measure_counter_options(false, true, true, true, true));
        assert!(!profile.measure_counter_left);
        assert!(profile.measure_counter_up);
        assert!(profile.measure_counter_vert);
        assert!(profile.broken_run);
        assert!(profile.run_timer);
        assert!(!profile.set_measure_counter_options(false, true, true, true, true));
    }

    #[test]
    fn player_side_indices_and_joined_masks_are_stable() {
        assert_eq!(PLAYER_SLOTS, 2);
        assert_eq!(player_side_index(PlayerSide::P1), 0);
        assert_eq!(player_side_index(PlayerSide::P2), 1);
        assert_eq!(player_side_number(PlayerSide::P1), 1);
        assert_eq!(player_side_number(PlayerSide::P2), 2);
        assert_eq!(player_side_for_index(0), PlayerSide::P1);
        assert_eq!(player_side_for_index(1), PlayerSide::P2);
        assert_eq!(player_side_for_index(2), PlayerSide::P1);
        assert_eq!(SESSION_JOINED_MASK_P1, 1 << 0);
        assert_eq!(SESSION_JOINED_MASK_P2, 1 << 1);
        assert_eq!(
            player_side_joined_mask(PlayerSide::P1),
            SESSION_JOINED_MASK_P1
        );
        assert_eq!(
            player_side_joined_mask(PlayerSide::P2),
            SESSION_JOINED_MASK_P2
        );

        let mask = joined_player_mask(true, false);
        assert!(player_side_is_joined(mask, PlayerSide::P1));
        assert!(!player_side_is_joined(mask, PlayerSide::P2));

        let mask = joined_player_mask(false, true);
        assert!(!player_side_is_joined(mask, PlayerSide::P1));
        assert!(player_side_is_joined(mask, PlayerSide::P2));
    }

    #[test]
    fn session_state_normalizes_rate_and_updates_modes() {
        let mut session = SessionState::default();
        assert_eq!(session.music_rate(), 1.0);

        session.set_music_rate(2.5);
        assert_eq!(session.music_rate(), 2.5);
        session.set_music_rate(9.0);
        assert_eq!(session.music_rate(), 3.0);
        session.set_music_rate(0.0);
        assert_eq!(session.music_rate(), 1.0);
        session.music_rate = f32::NAN;
        assert_eq!(session.music_rate(), 1.0);

        session.set_timing_tick_mode(TimingTickMode::Hit);
        assert_eq!(session.timing_tick_mode(), TimingTickMode::Hit);
        session.set_play_mode(PlayMode::Marathon);
        assert_eq!(session.play_mode(), PlayMode::Marathon);
        session.set_player_side(PlayerSide::P2);
        assert_eq!(session.player_side(), PlayerSide::P2);
    }

    #[test]
    fn session_snapshot_copies_runtime_fields_and_join_state() {
        let session = SessionState {
            joined_mask: SESSION_JOINED_MASK_P2,
            music_rate: 1.5,
            timing_tick_mode: TimingTickMode::Hit,
            play_style: PlayStyle::Double,
            play_mode: PlayMode::Marathon,
            player_side: PlayerSide::P2,
            ..SessionState::default()
        };

        let snapshot = SessionSnapshot::from_state(&session);
        assert_eq!(snapshot.joined_mask, SESSION_JOINED_MASK_P2);
        assert_eq!(snapshot.music_rate, 1.5);
        assert_eq!(snapshot.timing_tick_mode, TimingTickMode::Hit);
        assert_eq!(snapshot.play_style, PlayStyle::Double);
        assert_eq!(snapshot.play_mode, PlayMode::Marathon);
        assert_eq!(snapshot.player_side, PlayerSide::P2);
        assert!(!snapshot.side_joined(PlayerSide::P1));
        assert!(snapshot.side_joined(PlayerSide::P2));
    }

    #[test]
    fn session_snapshot_with_active_ids_uses_one_session_state() {
        let session = SessionState {
            active_profiles: [
                ActiveProfile::Local {
                    id: "local-p1".to_owned(),
                },
                ActiveProfile::Guest,
            ],
            play_style: PlayStyle::Versus,
            ..SessionState::default()
        };

        let (snapshot, active_ids) = session_snapshot_with_active_ids(&session);
        assert_eq!(snapshot.play_style, PlayStyle::Versus);
        assert_eq!(active_ids, [Some("local-p1".to_owned()), None]);
    }

    #[test]
    fn session_state_reports_play_style_changes() {
        let mut session = SessionState::default();
        assert_eq!(session.play_style(), PlayStyle::Single);
        assert_eq!(
            session.set_play_style(PlayStyle::Double),
            Some(PlayStyle::Single)
        );
        assert_eq!(session.play_style(), PlayStyle::Double);
        assert_eq!(session.set_play_style(PlayStyle::Double), None);
    }

    #[test]
    fn session_state_tracks_joined_sides() {
        let mut session = SessionState::default();
        assert!(session.side_joined(PlayerSide::P1));
        assert!(!session.side_joined(PlayerSide::P2));

        session.set_joined_sides(false, true);
        assert!(!session.side_joined(PlayerSide::P1));
        assert!(session.side_joined(PlayerSide::P2));
    }

    #[test]
    fn play_style_follows_join_count() {
        assert_eq!(
            play_style_for_joined(PlayStyle::Single, true, true),
            PlayStyle::Versus
        );
        assert_eq!(
            play_style_for_joined(PlayStyle::Double, true, true),
            PlayStyle::Versus
        );
        assert_eq!(
            play_style_for_joined(PlayStyle::Double, true, false),
            PlayStyle::Double
        );
        assert_eq!(
            play_style_for_joined(PlayStyle::Versus, false, true),
            PlayStyle::Single
        );
    }

    #[test]
    fn runtime_player_p2_includes_single_player_styles() {
        assert!(!runtime_player_is_p2(PlayStyle::Single, PlayerSide::P1));
        assert!(runtime_player_is_p2(PlayStyle::Single, PlayerSide::P2));
        assert!(!runtime_player_is_p2(PlayStyle::Double, PlayerSide::P1));
        assert!(runtime_player_is_p2(PlayStyle::Double, PlayerSide::P2));
        assert!(!runtime_player_is_p2(PlayStyle::Versus, PlayerSide::P2));
        assert!(!is_single_p2_side(PlayStyle::Single, PlayerSide::P1));
        assert!(is_single_p2_side(PlayStyle::Single, PlayerSide::P2));
        assert!(!is_single_p2_side(PlayStyle::Double, PlayerSide::P2));
        assert!(!is_single_p2_side(PlayStyle::Versus, PlayerSide::P2));
        assert_eq!(runtime_player_index(PlayStyle::Single, PlayerSide::P2), 0);
        assert_eq!(runtime_player_index(PlayStyle::Double, PlayerSide::P2), 0);
        assert_eq!(runtime_player_index(PlayStyle::Versus, PlayerSide::P1), 0);
        assert_eq!(runtime_player_index(PlayStyle::Versus, PlayerSide::P2), 1);

        assert_eq!(
            runtime_player_side(PlayStyle::Single, PlayerSide::P2, 0),
            PlayerSide::P2
        );
        assert_eq!(
            runtime_player_side(PlayStyle::Double, PlayerSide::P1, 1),
            PlayerSide::P1
        );
        assert_eq!(
            runtime_player_side(PlayStyle::Versus, PlayerSide::P1, 0),
            PlayerSide::P1
        );
        assert_eq!(
            runtime_player_side(PlayStyle::Versus, PlayerSide::P1, 1),
            PlayerSide::P2
        );
    }

    #[test]
    fn physical_player_slots_follow_session_side_for_single_player() {
        use PlayStyle::*;
        use PlayerSide::*;

        assert_eq!(physical_player_slot_for_chart_pad(Single, P1, false, 0), 0);
        assert_eq!(physical_player_slot_for_chart_pad(Single, P2, false, 0), 1);
        assert_eq!(physical_player_slot_for_chart_pad(Versus, P1, false, 0), 0);
        assert_eq!(physical_player_slot_for_chart_pad(Versus, P1, false, 1), 1);
        assert_eq!(physical_player_slot_for_chart_pad(Double, P2, true, 0), 0);
        assert_eq!(physical_player_slot_for_chart_pad(Double, P2, true, 1), 1);
    }

    #[test]
    fn local_profile_ids_reject_pathlike_or_empty_values() {
        assert!(is_local_profile_id("00000000"));
        assert!(is_local_profile_id("Player One"));
        assert!(is_local_profile_id(&"a".repeat(64)));

        assert!(!is_local_profile_id(""));
        assert!(!is_local_profile_id("."));
        assert!(!is_local_profile_id(".."));
        assert!(!is_local_profile_id("a/b"));
        assert!(!is_local_profile_id("a\\b"));
        assert!(!is_local_profile_id("a\0b"));
        assert!(!is_local_profile_id(&"a".repeat(65)));
    }

    #[test]
    fn generated_profile_guid_is_valid_uuid_v4() {
        for _ in 0..64 {
            let guid = generate_profile_guid();
            assert_eq!(guid.len(), 36, "guid `{guid}` should be 36 chars");
            assert!(
                is_valid_profile_guid(&guid),
                "guid `{guid}` should be valid"
            );
            assert!(is_local_profile_id(&guid), "guid should be a usable id");
            // Version nibble (char 14) is '4'; variant nibble (char 19) in 8..=b.
            let bytes = guid.as_bytes();
            assert_eq!(bytes[14], b'4');
            assert!(matches!(bytes[19], b'8' | b'9' | b'a' | b'b'));
        }
        // Distinct draws should not collide.
        let a = generate_profile_guid();
        let b = generate_profile_guid();
        assert_ne!(a, b);
    }

    #[test]
    fn invalid_profile_guids_are_rejected() {
        assert!(!is_valid_profile_guid(""));
        assert!(!is_valid_profile_guid("00000000"));
        assert!(!is_valid_profile_guid(
            "17c7b8a2-3b73-4e8a-9d7d-cfa7e783c00"
        )); // short tail
        assert!(!is_valid_profile_guid(
            "17c7b8a2-3b73-4e8a-9d7d-cfa7e783c00bb"
        )); // long tail
        assert!(!is_valid_profile_guid(
            "17c7b8a2_3b73_4e8a_9d7d_cfa7e783c00b"
        )); // wrong sep
        assert!(!is_valid_profile_guid(
            "g7c7b8a2-3b73-4e8a-9d7d-cfa7e783c00b"
        )); // non-hex
    }

    #[test]
    fn itgmania_guid_maps_to_stable_valid_uuid() {
        // Blank source → no derived identity.
        assert_eq!(profile_guid_from_itgmania_guid(""), None);
        assert_eq!(profile_guid_from_itgmania_guid("   "), None);

        // Deterministic: same ITGmania GUID always yields the same DeadSync GUID.
        let a = profile_guid_from_itgmania_guid("99f55b745304ebcf").expect("guid");
        let b = profile_guid_from_itgmania_guid("99f55b745304ebcf").expect("guid");
        assert_eq!(a, b);
        // Case/whitespace-insensitive.
        let c = profile_guid_from_itgmania_guid("  99F55B745304EBCF  ").expect("guid");
        assert_eq!(a, c);
        // The derived value is a canonical, accepted DeadSync GUID.
        assert!(is_valid_profile_guid(&a));

        // Different ITGmania GUIDs map to different DeadSync GUIDs.
        let other = profile_guid_from_itgmania_guid("0247fbc7e366cf9f").expect("guid");
        assert_ne!(a, other);
    }

    #[test]
    fn sanitize_folder_base_strips_invalid_chars_and_collapses_space() {
        assert_eq!(sanitize_folder_base("Alice").as_deref(), Some("Alice"));
        assert_eq!(
            sanitize_folder_base("  Bob   Smith  ").as_deref(),
            Some("Bob Smith")
        );
        assert_eq!(
            sanitize_folder_base("a/b:c*?\"<>|d").as_deref(),
            Some("abcd")
        );
        assert_eq!(sanitize_folder_base("...").as_deref(), None);
        assert_eq!(sanitize_folder_base("").as_deref(), None);
        assert_eq!(sanitize_folder_base("   ").as_deref(), None);
        // Windows reserved device names are rejected (callers fall back to GUID).
        assert_eq!(sanitize_folder_base("CON").as_deref(), None);
        assert_eq!(sanitize_folder_base("lpt1").as_deref(), None);
    }

    #[test]
    fn folder_name_for_display_resolves_collisions_and_falls_back_to_guid() {
        let guid = "17c7b8a2-3b73-4e8a-9d7d-cfa7e783c00b";
        let existing = vec!["Alice".to_string(), "alice-2".to_string()];
        assert_eq!(folder_name_for_display("Bob", guid, &existing), "Bob");
        // Case-insensitive collision -> next free suffix.
        assert_eq!(folder_name_for_display("alice", guid, &existing), "alice-3");
        // Unusable display name -> GUID.
        assert_eq!(folder_name_for_display("***", guid, &existing), guid);
    }

    #[test]
    fn upsert_profile_guid_inserts_replaces_and_creates_section() {
        let guid = "17c7b8a2-3b73-4e8a-9d7d-cfa7e783c00b";

        // Inserts into an existing [userprofile] section as the first key.
        let src = "[userprofile]\nDisplayName=Alice\nPlayerInitials=ALC\n";
        let out = upsert_profile_guid_content(src, guid);
        assert_eq!(
            out,
            format!("[userprofile]\nGuid={guid}\nDisplayName=Alice\nPlayerInitials=ALC\n")
        );
        assert_eq!(read_userprofile_identity(&out).0.as_deref(), Some(guid));

        // Replaces a pre-existing Guid value, keeping a single key.
        let src = "[userprofile]\nGuid=old-value\nDisplayName=Bob\n".to_string();
        let out = upsert_profile_guid_content(&src, guid);
        assert_eq!(out.matches("Guid=").count(), 1);
        assert_eq!(read_userprofile_identity(&out).0.as_deref(), Some(guid));

        // Creates the section when missing.
        let src = "[Editable]\nWeightPounds=0\n";
        let out = upsert_profile_guid_content(src, guid);
        assert!(out.contains("[userprofile]\nGuid="));
        assert_eq!(read_userprofile_identity(&out).0.as_deref(), Some(guid));

        // Only the first [userprofile] section gets a Guid; extras stay intact.
        let src = "[userprofile]\nDisplayName=A\n[userprofile]\nDisplayName=B\n";
        let out = upsert_profile_guid_content(src, guid);
        assert_eq!(out.matches("Guid=").count(), 1);
    }

    #[test]
    fn read_userprofile_identity_is_case_insensitive_and_lowercases_guid() {
        // Mixed-case section/key headers and an upper-case GUID still resolve,
        // and the GUID is normalized to lowercase.
        let src = "[UserProfile]\nGUID=17C7B8A2-3B73-4E8A-9D7D-CFA7E783C00B\nDisplayName=Alice\n";
        let (guid, name) = read_userprofile_identity(src);
        assert_eq!(
            guid.as_deref(),
            Some("17c7b8a2-3b73-4e8a-9d7d-cfa7e783c00b")
        );
        assert_eq!(name.as_deref(), Some("Alice"));

        // Invalid GUID and empty display name are rejected.
        let (guid, name) =
            read_userprofile_identity("[userprofile]\nGuid=not-a-guid\nDisplayName=\n");
        assert_eq!(guid, None);
        assert_eq!(name, None);

        // Keys outside [userprofile] are ignored.
        let (guid, _) =
            read_userprofile_identity("[Editable]\nGuid=17c7b8a2-3b73-4e8a-9d7d-cfa7e783c00b\n");
        assert_eq!(guid, None);
    }

    #[test]
    fn read_userprofile_initials_sanitizes_first_profile_section() {
        let src = "[UserProfile]\nPlayerInitials= a b-c_d \n\n[UserProfile]\nPlayerInitials=NOPE\n";
        assert_eq!(
            read_userprofile_initials_content(src).as_deref(),
            Some("ABCD")
        );
        assert_eq!(
            read_userprofile_initials_content("[Editable]\nPlayerInitials=ABCD\n"),
            None
        );
        assert_eq!(
            read_userprofile_initials_content("[userprofile]\nPlayerInitials=\n"),
            None
        );
    }

    #[test]
    fn local_score_profile_sources_use_profile_initials_or_default() {
        let root = temp_profile_dir("score-profile-sources");
        let alice = root.join("alice-guid");
        let bob = root.join("bob-guid");
        fs::create_dir_all(&alice).expect("alice profile dir should create");
        fs::create_dir_all(&bob).expect("bob profile dir should create");
        fs::write(
            profile_ini_path(&alice),
            "[userprofile]\nPlayerInitials=ALIC\n",
        )
        .expect("alice profile ini should write");

        let sources = local_score_profile_sources_from_summaries(
            vec![
                LocalProfileSummary {
                    id: "alice-guid".to_string(),
                    display_name: "Alice".to_string(),
                    avatar_path: None,
                },
                LocalProfileSummary {
                    id: "bob-guid".to_string(),
                    display_name: "Bob".to_string(),
                    avatar_path: None,
                },
            ],
            |id| root.join(id),
        );

        assert_eq!(sources[0].root, alice.join("scores").join("local"));
        assert_eq!(sources[0].initials, "ALIC");
        assert_eq!(sources[0].display_name, "Alice");
        assert_eq!(sources[1].root, bob.join("scores").join("local"));
        assert_eq!(sources[1].initials, DEFAULT_SCORE_INITIALS);
        assert_eq!(sources[1].display_name, "Bob");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_local_score_profile_sources_resolve_guid_dirs() {
        let root = temp_profile_dir("runtime-score-profile-sources");
        runtime_invalidate_profile_dir_cache();
        let id = create_local_profile_dir(&root, "Alice", NoteSkin::default(), 100)
            .expect("profile should be created");
        let score_root = root.join("Alice").join("scores").join("local");
        runtime_set_profile_dir_cache(&root, build_profile_dir_map(&root, |_, _, _, _| {}));

        let sources = runtime_local_score_profile_sources(&root, |_, _, _, _| {});
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].root, score_root);
        assert_eq!(sources[0].initials, "ALIC");
        assert_eq!(sources[0].display_name, "Alice");

        let source = runtime_local_score_profile_source(&root, &id, "", |_, _, _, _| {});
        assert_eq!(source.root, score_root);
        assert_eq!(source.initials, "ALIC");
        assert_eq!(source.display_name, "");

        runtime_invalidate_profile_dir_cache();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn profile_id_sorting_is_case_insensitive_with_stable_tiebreak() {
        let mut ids = ["beta", "Alpha", "alpha", "Beta", "00000000"];
        ids.sort_by(|a, b| cmp_profile_ids_case_insensitive(a, b));
        assert_eq!(ids, ["00000000", "Alpha", "alpha", "Beta", "beta"]);
    }

    #[test]
    fn profile_stats_roundtrip_preserves_current_combo_and_known_packs() {
        let stats = ProfileStats {
            current_combo: 12,
            known_pack_names: ["Beta", "Alpha"].into_iter().map(str::to_owned).collect(),
        };

        let bytes = encode_profile_stats(&stats).expect("profile stats should encode");
        let decoded = decode_profile_stats(&bytes).expect("profile stats should decode");
        assert_eq!(decoded, stats);

        let (raw, _) =
            bincode::decode_from_slice::<ProfileStatsV1, _>(&bytes, bincode::config::standard())
                .expect("encoded stats should use v1 shape");
        assert_eq!(
            raw.known_pack_names,
            vec!["Alpha".to_string(), "Beta".to_string()]
        );
    }

    #[test]
    fn profile_stats_file_load_write_and_import_helpers_round_trip() {
        let dir = temp_profile_dir("profile-stats");
        assert_eq!(
            load_profile_stats_file(&profile_stats_path(&dir)).expect("missing stats should load"),
            None
        );

        let stats = ProfileStats {
            current_combo: 12,
            known_pack_names: ["Beta", "Alpha"].into_iter().map(str::to_owned).collect(),
        };
        write_profile_stats_dir(&dir, &stats).expect("profile stats should write");
        assert_eq!(
            load_profile_stats_file(&profile_stats_path(&dir)).expect("stats should load"),
            Some(stats)
        );

        let imported_dir = temp_profile_dir("profile-stats-import");
        write_imported_profile_stats_dir(&imported_dir, 42)
            .expect("imported profile stats should write");
        assert_eq!(
            load_profile_stats_file(&profile_stats_path(&imported_dir))
                .expect("imported stats should load"),
            Some(ProfileStats {
                current_combo: 42,
                known_pack_names: HashSet::new(),
            })
        );

        let empty_dir = temp_profile_dir("profile-stats-empty-import");
        write_imported_profile_stats_dir(&empty_dir, 0)
            .expect("zero combo import should be a no-op");
        assert!(!profile_stats_path(&empty_dir).exists());

        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_dir_all(imported_dir);
        let _ = fs::remove_dir_all(empty_dir);
    }

    #[test]
    fn create_local_profile_dir_writes_default_profile_files() {
        let root = temp_profile_dir("create-profile-root");

        let id = create_local_profile_dir(&root, "Alice", NoteSkin::new("cel"), 88)
            .expect("profile should be created");

        assert!(is_valid_profile_guid(&id));
        let summaries = scan_local_profile_summaries(&root);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, id);
        assert_eq!(summaries[0].display_name, "Alice");

        let dir = root.join("Alice");
        assert!(profile_ini_path(&dir).is_file());
        assert!(groovestats_ini_path(&dir).is_file());
        assert!(arrowcloud_ini_path(&dir).is_file());

        let profile_ini =
            fs::read_to_string(profile_ini_path(&dir)).expect("profile ini should be readable");
        assert!(profile_ini.contains("DisplayName=Alice\n"));
        assert!(profile_ini.contains("PlayerInitials=ALIC\n"));
        assert!(profile_ini.contains("NoteSkin=cel\n"));
        assert!(profile_ini.contains("PadLightBrightness=88\n"));
        assert!(profile_ini.contains(&format!("Guid={id}\n")));

        assert_eq!(
            fs::read_to_string(groovestats_ini_path(&dir)).unwrap(),
            "[GrooveStats]\nApiKey=\nIsPadPlayer=0\nUsername=\n\n"
        );
        assert_eq!(
            fs::read_to_string(arrowcloud_ini_path(&dir)).unwrap(),
            "[ArrowCloud]\nApiKey=\n\n"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_local_profile_lifecycle_updates_runtime_state() {
        let root = temp_profile_dir("runtime-profile-lifecycle");
        runtime_invalidate_profile_dir_cache();
        runtime_set_active_profile_for_side(PlayerSide::P1, ActiveProfile::Guest);
        runtime_set_active_profile_for_side(PlayerSide::P2, ActiveProfile::Guest);

        let created = runtime_create_local_profile(
            &root,
            "Alice",
            NoteSkin::new("cel"),
            88,
            [None, Some("p2".to_string())],
        )
        .expect("profile should be created");
        assert_eq!(created.default_profiles[0], Some(created.id.clone()));
        assert_eq!(created.default_profiles[1].as_deref(), Some("p2"));

        let original_dir = root.join("Alice");
        assert!(original_dir.is_dir());
        runtime_set_active_profile_for_side(
            PlayerSide::P1,
            ActiveProfile::Local {
                id: created.id.clone(),
            },
        );
        runtime_update_profile_for_side(PlayerSide::P1, |profile| {
            *profile = Profile::default();
            true
        });

        let renamed = runtime_rename_local_profile(&root, &original_dir, &created.id, "Bob")
            .expect("profile should rename");
        assert_eq!(renamed.display_name, "Bob");
        assert_eq!(runtime_profile_for_side(PlayerSide::P1).display_name, "Bob");

        let delete_dir = runtime_profile_dir_for_id(&root, &created.id, |_, _, _, _| {});
        let deleted = runtime_delete_local_profile(
            &delete_dir,
            &created.id,
            [Some(created.id.clone()), Some("p2".to_string())],
        )
        .expect("profile should delete");
        assert_eq!(deleted.default_profiles[0], None);
        assert_eq!(deleted.default_profiles[1].as_deref(), Some("p2"));
        assert!(deleted.changed_sides[player_side_index(PlayerSide::P1)]);
        assert_eq!(
            runtime_active_profile_for_side(PlayerSide::P1),
            ActiveProfile::Guest
        );

        runtime_set_active_profile_for_side(PlayerSide::P1, ActiveProfile::Guest);
        runtime_set_active_profile_for_side(PlayerSide::P2, ActiveProfile::Guest);
        runtime_invalidate_profile_dir_cache();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_profile_dir_cache_seeds_and_invalidates() {
        let root = temp_profile_dir("runtime-profile-dir-cache");
        let id = create_local_profile_dir(&root, "Alice", NoteSkin::default(), 100)
            .expect("profile should be created");
        let real_dir = build_profile_dir_map(&root, |_, _, _, _| {})
            .remove(&id)
            .expect("created profile should map by guid");
        let fake_dir = root.join("stale");

        runtime_set_profile_dir_cache(&root, HashMap::from([(id.clone(), fake_dir.clone())]));
        assert_eq!(
            runtime_profile_dir_for_id(&root, &id, |_, _, _, _| {}),
            fake_dir
        );

        runtime_invalidate_profile_dir_cache();
        assert_eq!(
            runtime_profile_dir_for_id(&root, &id, |_, _, _, _| {}),
            real_dir
        );
        runtime_invalidate_profile_dir_cache();

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn create_local_profile_from_import_dir_writes_import_payload() {
        let root = temp_profile_dir("import-profile-root");
        let avatar_root = temp_profile_dir("import-avatar-root");
        let avatar = avatar_root.join("source.png");
        fs::write(&avatar, b"avatar").expect("avatar source should write");

        let singles = PlayerOptionsData {
            noteskin: NoteSkin::new("metal"),
            ..PlayerOptionsData::default()
        };
        let doubles = PlayerOptionsData {
            noteskin: NoteSkin::new("cyber"),
            ..PlayerOptionsData::default()
        };
        let guid = "17c7b8a2-3b73-4e8a-9d7d-cfa7e783c00b";
        let data = ImportProfileData {
            display_name: "Imported",
            weight_pounds: 155,
            birth_year: 1988,
            initials: "itgmania-player",
            groovestats_api_key: "gs-key",
            groovestats_username: "gs-user",
            groovestats_is_pad_player: true,
            arrowcloud_api_key: "ac-key",
            ignore_step_count_calories: true,
            avatar_src: Some(&avatar),
            options_singles: &singles,
            options_doubles: &doubles,
            guid,
        };

        let result = create_local_profile_from_import_dir(&root, &data)
            .expect("import profile should be created");

        assert_eq!(result.id, guid);
        assert!(result.avatar_copy_error.is_none());

        let dir = root.join("Imported");
        assert_eq!(fs::read(dir.join("profile.png")).unwrap(), b"avatar");

        let profile_ini =
            fs::read_to_string(profile_ini_path(&dir)).expect("profile ini should be readable");
        assert!(profile_ini.contains(&format!("Guid={guid}\n")));
        assert!(profile_ini.contains("DisplayName=Imported\n"));
        assert!(profile_ini.contains("PlayerInitials=ITGM\n"));
        assert!(profile_ini.contains("WeightPounds=155\nBirthYear=1988\n"));
        assert!(profile_ini.contains("IgnoreStepCountCalories=1\n"));
        assert!(profile_ini.contains("NoteSkin=metal\n"));
        assert!(profile_ini.contains("NoteSkin=cyber\n"));

        assert_eq!(
            fs::read_to_string(groovestats_ini_path(&dir)).unwrap(),
            "[GrooveStats]\nApiKey=gs-key\nIsPadPlayer=1\nUsername=gs-user\n\n"
        );
        assert_eq!(
            fs::read_to_string(arrowcloud_ini_path(&dir)).unwrap(),
            "[ArrowCloud]\nApiKey=ac-key\n\n"
        );

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(avatar_root);
    }

    #[test]
    fn rename_local_profile_dir_rewrites_display_and_cosmetic_folder() {
        let root = temp_profile_dir("rename-profile-root");
        let alice_id = create_local_profile_dir(&root, "Alice", NoteSkin::default(), 100)
            .expect("Alice profile should be created");
        create_local_profile_dir(&root, "Bob", NoteSkin::default(), 100)
            .expect("Bob profile should be created");
        let alice_dir = root.join("Alice");

        let result = rename_local_profile_dir(&root, &alice_dir, &alice_id, " Bob ")
            .expect("profile should rename");

        assert_eq!(result.display_name, "Bob");
        let folder = result.folder_rename.expect("folder should be renamed");
        assert_eq!(folder.current_folder, "Alice");
        assert_ne!(folder.desired_folder, "Bob");
        assert!(folder.error.is_none());
        assert!(!alice_dir.exists());

        let summaries = scan_local_profile_summaries(&root);
        let renamed = summaries
            .iter()
            .find(|summary| summary.id == alice_id)
            .expect("renamed profile should scan");
        assert_eq!(renamed.display_name, "Bob");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn delete_local_profile_dir_removes_profile_and_validates_inputs() {
        let root = temp_profile_dir("delete-profile-root");
        let id = create_local_profile_dir(&root, "Delete Me", NoteSkin::default(), 100)
            .expect("profile should be created");
        let dir = root.join("Delete Me");

        delete_local_profile_dir(&dir, "../bad").expect_err("invalid id should fail");
        assert!(dir.is_dir());

        delete_local_profile_dir(&dir, &id).expect("profile should delete");
        assert!(!dir.exists());
        assert_eq!(
            delete_local_profile_dir(&dir, &id)
                .expect_err("missing profile should fail")
                .kind(),
            std::io::ErrorKind::NotFound
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn migrate_local_profile_dirs_backfills_renames_and_builds_cache() {
        let root = temp_profile_dir("migrate-profile-root");
        let existing_id = "17c7b8a2-3b73-4e8a-9d7d-cfa7e783c00b";
        let existing_dir = root.join("Alice");
        let legacy_dir = root.join("Legacy");
        fs::create_dir_all(&existing_dir).expect("existing profile dir should be created");
        fs::create_dir_all(&legacy_dir).expect("legacy profile dir should be created");
        fs::write(
            profile_ini_path(&existing_dir),
            format!("[userprofile]\nGuid={existing_id}\nDisplayName=Alice\n"),
        )
        .expect("existing profile should write");
        fs::write(
            profile_ini_path(&legacy_dir),
            "[userprofile]\nDisplayName=Alice\n",
        )
        .expect("legacy profile should write");

        let result = migrate_local_profile_dirs(&root);

        assert_eq!(result.guid_backfills.len(), 1);
        assert_eq!(result.guid_backfills[0].path, legacy_dir);
        assert!(result.guid_backfills[0].error.is_none());
        assert_eq!(result.folder_renames.len(), 1);
        assert_eq!(result.folder_renames[0].current_folder, "Legacy");
        assert_eq!(result.folder_renames[0].desired_folder, "Alice-2");
        assert!(result.folder_renames[0].error.is_none());
        assert!(!root.join("Legacy").exists());
        assert!(root.join("Alice-2").is_dir());

        let migrated = result
            .entries
            .iter()
            .find(|entry| entry.original_folder == "Legacy")
            .expect("legacy entry should be present");
        assert_eq!(migrated.folder, "Alice-2");
        assert!(is_valid_profile_guid(&migrated.guid));
        assert_eq!(
            result.cache_map.get(&migrated.guid),
            Some(&root.join("Alice-2"))
        );
        assert_eq!(result.cache_map.get(existing_id), Some(&root.join("Alice")));

        let migrated_ini =
            fs::read_to_string(profile_ini_path(&root.join("Alice-2"))).expect("ini should read");
        assert_eq!(
            read_userprofile_identity(&migrated_ini).0.as_deref(),
            Some(migrated.guid.as_str())
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn favorites_content_trims_ignores_empty_lines_and_dedupes() {
        let favorites = parse_favorites_content(" abc123 \n\nxyz789\nabc123\n   \n");

        assert_eq!(favorites.len(), 2);
        assert!(favorites.contains("abc123"));
        assert!(favorites.contains("xyz789"));
    }

    #[test]
    fn favorites_content_renders_sorted_without_trailing_newline() {
        let favorites = HashSet::from([
            "xyz789".to_string(),
            "abc123".to_string(),
            "mid456".to_string(),
        ]);

        assert_eq!(
            render_favorites_content(&favorites),
            "abc123\nmid456\nxyz789"
        );
        assert_eq!(render_favorites_content(&HashSet::new()), "");
    }

    fn temp_profile_dir(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        dir.push(format!(
            "deadsync-profile-test-{}-{}-{}",
            name,
            std::process::id(),
            stamp
        ));
        fs::create_dir_all(&dir).expect("test profile dir should be created");
        dir
    }

    #[test]
    fn favorites_dir_load_save_merge_and_toggle_round_trip() {
        let dir = temp_profile_dir("favorites");
        assert!(load_favorites_dir(&dir).is_empty());

        let mut favorites = HashSet::from(["xyz789".to_string(), "abc123".to_string()]);
        save_favorites_dir(&dir, &favorites);
        assert_eq!(load_favorites_dir(&dir), favorites);

        merge_imported_favorites_dir(&dir, &HashSet::from(["mid456".to_string()]));
        favorites.insert("mid456".to_string());
        assert_eq!(load_favorites_dir(&dir), favorites);

        assert!(!toggle_favorite_hash(&mut favorites, "abc123"));
        assert!(!favorites.contains("abc123"));
        assert!(toggle_favorite_hash(&mut favorites, "abc123"));
        assert!(favorites.contains("abc123"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn favorited_packs_content_trims_ignores_empty_lines_and_dedupes() {
        let packs =
            parse_favorited_packs_content(" Tachyon Alpha \n\nIn The Groove\nTachyon Alpha\n   \n");

        assert_eq!(packs.len(), 2);
        assert!(packs.contains("Tachyon Alpha"));
        assert!(packs.contains("In The Groove"));
    }

    #[test]
    fn favorited_packs_content_renders_case_insensitive_sorted() {
        let packs = HashSet::from([
            "zebra mix".to_string(),
            "Alpha Pack".to_string(),
            "midpack".to_string(),
        ]);

        assert_eq!(
            render_favorited_packs_content(&packs),
            "Alpha Pack\nmidpack\nzebra mix"
        );
        assert_eq!(render_favorited_packs_content(&HashSet::new()), "");
    }

    #[test]
    fn favorited_packs_dir_load_save_and_toggle_round_trip() {
        let dir = temp_profile_dir("favorited-packs");
        assert!(load_favorited_packs_dir(&dir).is_empty());

        let mut packs = HashSet::from(["zebra mix".to_string(), "Alpha Pack".to_string()]);
        save_favorited_packs_dir(&dir, &packs);
        assert_eq!(load_favorited_packs_dir(&dir), packs);

        assert!(!toggle_favorited_pack(&mut packs, "zebra mix"));
        assert!(!packs.contains("zebra mix"));
        assert!(toggle_favorited_pack(&mut packs, "zebra mix"));
        assert!(packs.contains("zebra mix"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn profile_sidecars_load_defaults_when_files_are_missing() {
        let dir = temp_profile_dir("profile-sidecars-missing");
        let mut default_profile = Profile::default();
        default_profile.current_combo = 17;

        let sidecars = load_profile_sidecars_dir(&dir, &default_profile);

        assert_eq!(sidecars.stats.current_combo, 17);
        assert!(sidecars.stats.known_pack_names.is_empty());
        assert!(sidecars.stats_error.is_none());
        assert!(sidecars.favorites.is_empty());
        assert!(sidecars.favorited_packs.is_empty());
        assert!(sidecars.avatar_path.is_none());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn profile_sidecars_load_stats_sets_and_avatar() {
        let dir = temp_profile_dir("profile-sidecars");
        let default_profile = Profile::default();
        let stats = ProfileStats {
            current_combo: 42,
            known_pack_names: HashSet::from(["Known Pack".to_string()]),
        };
        let favorites = HashSet::from(["chart-hash".to_string()]);
        let packs = HashSet::from(["Pack A".to_string()]);
        let avatar = dir.join("profile.png");

        write_profile_stats_dir(&dir, &stats).expect("stats should write");
        save_favorites_dir(&dir, &favorites);
        save_favorited_packs_dir(&dir, &packs);
        fs::write(&avatar, b"avatar").expect("avatar should write");

        let sidecars = load_profile_sidecars_dir(&dir, &default_profile);

        assert_eq!(sidecars.stats, stats);
        assert!(sidecars.stats_error.is_none());
        assert_eq!(sidecars.favorites, favorites);
        assert_eq!(sidecars.favorited_packs, packs);
        assert_eq!(sidecars.avatar_path, Some(avatar));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn known_pack_names_add_only_new_entries() {
        let mut known = HashSet::from(["Alpha".to_string()]);

        assert!(add_known_pack_names(&mut known, ["Alpha", "Beta"]));
        assert_eq!(known.len(), 2);
        assert!(known.contains("Alpha"));
        assert!(known.contains("Beta"));

        assert!(!add_known_pack_names(&mut known, ["Alpha", "Beta"]));
    }

    #[test]
    fn unknown_pack_names_reports_scanned_packs_not_in_profile() {
        let known = HashSet::from(["Alpha".to_string()]);
        let scanned = vec!["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()];
        let unknown = unknown_pack_names(&known, &scanned);

        assert_eq!(unknown.len(), 2);
        assert!(unknown.contains("Beta"));
        assert!(unknown.contains("Gamma"));
        assert!(!unknown.contains("Alpha"));
    }

    #[test]
    fn profile_stats_decode_accepts_legacy_combo_payload() {
        let bytes = bincode::encode_to_vec(
            LegacyProfileStatsV1 {
                version: PROFILE_STATS_VERSION_V1,
                current_combo: 42,
            },
            bincode::config::standard(),
        )
        .expect("legacy stats should encode");

        let stats = decode_profile_stats(&bytes).expect("legacy stats should decode");
        assert_eq!(stats.current_combo, 42);
        assert!(stats.known_pack_names.is_empty());
    }

    #[test]
    fn profile_stats_decode_rejects_unsupported_version() {
        let bytes = bincode::encode_to_vec(
            ProfileStatsV1 {
                version: PROFILE_STATS_VERSION_V1 + 1,
                current_combo: 0,
                known_pack_names: Vec::new(),
            },
            bincode::config::standard(),
        )
        .expect("stats should encode");

        assert_eq!(
            decode_profile_stats(&bytes),
            Err(ProfileStatsDecodeError::UnsupportedVersion(
                PROFILE_STATS_VERSION_V1 + 1
            ))
        );
    }

    #[test]
    fn profile_display_name_rewrite_updates_existing_userprofile_key() {
        let src = "[userprofile]\nDisplayName=Old\nPlayerInitials=OLD\n";
        let out = rewrite_profile_display_name_content(src, "New Name");
        assert_eq!(
            out,
            "[userprofile]\nDisplayName=New Name\nPlayerInitials=OLD\n"
        );
    }

    #[test]
    fn profile_display_name_rewrite_adds_missing_key_before_next_section() {
        let src = "[userprofile]\nPlayerInitials=OLD\n\n[Stats]\nCalories=0\n";
        let out = rewrite_profile_display_name_content(src, "New Name");
        assert_eq!(
            out,
            "[userprofile]\nPlayerInitials=OLD\n\nDisplayName=New Name\n[Stats]\nCalories=0\n"
        );
    }

    #[test]
    fn profile_display_name_rewrite_appends_missing_section() {
        let src = "[Stats]\nCalories=0\n";
        let out = rewrite_profile_display_name_content(src, "New Name");
        assert_eq!(
            out,
            "[Stats]\nCalories=0\n[userprofile]\nDisplayName=New Name\n"
        );

        let out = rewrite_profile_display_name_content("", "New Name");
        assert_eq!(out, "[userprofile]\nDisplayName=New Name\n");
    }

    #[test]
    fn profile_avatar_path_prefers_profile_png() {
        let dir =
            std::env::temp_dir().join(format!("deadsync-profile-avatar-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let avatar = dir.join("avatar.png");
        let profile = dir.join("profile.png");
        fs::write(&avatar, b"avatar").unwrap();

        assert_eq!(find_profile_avatar_path(&dir), Some(avatar.clone()));

        fs::write(&profile, b"profile").unwrap();
        assert_eq!(find_profile_avatar_path(&dir), Some(profile));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn active_profile_helpers_report_guest_or_local_id() {
        let guest = ActiveProfile::Guest;
        assert!(active_profile_is_guest(&guest));
        assert_eq!(active_profile_local_id(&guest), None);

        let local = ActiveProfile::Local {
            id: "00000042".to_string(),
        };
        assert!(!active_profile_is_guest(&local));
        assert_eq!(active_profile_local_id(&local), Some("00000042"));
    }

    #[test]
    fn machine_seeded_profiles_apply_guest_and_default_settings() {
        let noteskin = NoteSkin::new("cel");
        let guest = guest_profile(noteskin.clone(), 88);
        assert_eq!(guest.display_name, "[ GUEST ]");
        assert_eq!(guest.scroll_speed, GUEST_SCROLL_SPEED);
        assert_eq!(guest.noteskin, noteskin);
        assert_eq!(guest.pad_light_brightness, 88);
        assert!(guest.avatar_path.is_none());
        assert!(guest.avatar_texture_key.is_none());

        let default_profile = default_profile_with_machine_settings(NoteSkin::new("metal"), 250);
        assert_eq!(default_profile.noteskin.as_str(), "metal");
        assert_eq!(default_profile.pad_light_brightness, 100);
        assert_eq!(
            default_profile.player_options_singles.noteskin.as_str(),
            "metal"
        );
        assert_eq!(
            default_profile.player_options_doubles.noteskin.as_str(),
            "metal"
        );
    }

    #[test]
    fn resolve_active_profile_load_selects_fallback_for_missing_local() {
        let active = ActiveProfile::Local {
            id: "missing".to_string(),
        };

        let selected =
            resolve_active_profile_for_load(&active, |_| false, || Some("fallback".to_string()));

        assert_eq!(
            selected,
            ActiveProfileLoadSelection::MissingFallbackLocal {
                missing_id: "missing".to_string(),
                fallback_id: "fallback".to_string(),
            }
        );
        assert_eq!(selected.local_id(), Some("fallback"));
        assert_eq!(
            selected.session_profile(),
            ActiveProfile::Local {
                id: "fallback".to_string(),
            }
        );
    }

    #[test]
    fn resolve_active_profile_load_selects_guest_without_fallback() {
        let active = ActiveProfile::Local {
            id: "missing".to_string(),
        };
        let selected = resolve_active_profile_for_load(&active, |_| false, || None);

        assert_eq!(
            selected,
            ActiveProfileLoadSelection::MissingFallbackGuest {
                missing_id: "missing".to_string(),
            }
        );
        assert_eq!(selected.local_id(), None);
        assert_eq!(selected.session_profile(), ActiveProfile::Guest);
    }

    #[test]
    fn long_error_bar_intensity_clamps_to_supported_range() {
        assert!((LONG_ERROR_BAR_INTENSITY_DEFAULT - 2.0).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.0) - 1.0).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(2.0) - 2.0).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(0.0) - LONG_ERROR_BAR_INTENSITY_MIN).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(5.0) - LONG_ERROR_BAR_INTENSITY_MAX).abs() < 1e-6);
        assert!(
            (clamp_long_error_bar_intensity(f32::NAN) - LONG_ERROR_BAR_INTENSITY_DEFAULT).abs()
                < 1e-6
        );
        assert!(
            (clamp_long_error_bar_intensity(f32::INFINITY) - LONG_ERROR_BAR_INTENSITY_DEFAULT)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn long_error_bar_intensity_snaps_to_quarter_step_grid() {
        assert!((clamp_long_error_bar_intensity(1.10) - 1.00).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.13) - 1.25).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.40) - 1.50).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.75) - 1.75).abs() < 1e-6);
        assert!((clamp_long_error_bar_intensity(1.95) - 2.00).abs() < 1e-6);
        let count = ((LONG_ERROR_BAR_INTENSITY_MAX - LONG_ERROR_BAR_INTENSITY_MIN)
            / LONG_ERROR_BAR_INTENSITY_STEP)
            .round() as usize
            + 1;
        assert_eq!(count, 13);
    }

    #[test]
    fn average_error_bar_intensity_clamps_to_supported_range() {
        assert!((AVERAGE_ERROR_BAR_INTENSITY_DEFAULT - 1.0).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.0) - 1.0).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(2.0) - 2.0).abs() < 1e-6);
        assert!(
            (clamp_average_error_bar_intensity(0.0) - AVERAGE_ERROR_BAR_INTENSITY_MIN).abs() < 1e-6
        );
        assert!(
            (clamp_average_error_bar_intensity(5.0) - AVERAGE_ERROR_BAR_INTENSITY_MAX).abs() < 1e-6
        );
        assert!(
            (clamp_average_error_bar_intensity(f32::NAN) - AVERAGE_ERROR_BAR_INTENSITY_DEFAULT)
                .abs()
                < 1e-6
        );
        assert!(
            (clamp_average_error_bar_intensity(f32::INFINITY)
                - AVERAGE_ERROR_BAR_INTENSITY_DEFAULT)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn average_error_bar_intensity_snaps_to_quarter_step_grid() {
        assert!((clamp_average_error_bar_intensity(1.10) - 1.00).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.13) - 1.25).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.40) - 1.50).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.75) - 1.75).abs() < 1e-6);
        assert!((clamp_average_error_bar_intensity(1.95) - 2.00).abs() < 1e-6);
        let count = ((AVERAGE_ERROR_BAR_INTENSITY_MAX - AVERAGE_ERROR_BAR_INTENSITY_MIN)
            / AVERAGE_ERROR_BAR_INTENSITY_STEP)
            .round() as usize
            + 1;
        assert_eq!(count, 5);
    }

    #[test]
    fn average_error_bar_interval_clamps_to_supported_range() {
        assert_eq!(AVERAGE_ERROR_BAR_INTERVAL_MS_DEFAULT, 400);
        assert_eq!(clamp_average_error_bar_interval_ms(100), 100);
        assert_eq!(clamp_average_error_bar_interval_ms(2000), 2000);
        assert_eq!(
            clamp_average_error_bar_interval_ms(0),
            AVERAGE_ERROR_BAR_INTERVAL_MS_MIN
        );
        assert_eq!(
            clamp_average_error_bar_interval_ms(4000),
            AVERAGE_ERROR_BAR_INTERVAL_MS_MAX
        );
    }

    #[test]
    fn average_error_bar_interval_snaps_to_100ms_step_grid() {
        assert_eq!(AVERAGE_ERROR_BAR_INTERVAL_MS_STEP, 100);
        assert_eq!(clamp_average_error_bar_interval_ms(149), 100);
        assert_eq!(clamp_average_error_bar_interval_ms(150), 200);
        assert_eq!(clamp_average_error_bar_interval_ms(349), 300);
        assert_eq!(clamp_average_error_bar_interval_ms(350), 400);
        assert_eq!(clamp_average_error_bar_interval_ms(1951), 2000);
    }

    #[test]
    fn profile_window_clamps_keep_supported_ranges() {
        assert_eq!(clamp_tilt_threshold_ms(0), 0);
        assert_eq!(clamp_tilt_threshold_ms(50), 50);
        assert_eq!(clamp_tilt_threshold_ms(101), TILT_THRESHOLD_MAX_MS);
        assert_eq!(
            clamp_custom_fantastic_window_ms(0),
            CUSTOM_FANTASTIC_WINDOW_MIN_MS
        );
        assert_eq!(clamp_custom_fantastic_window_ms(10), 10);
        assert_eq!(
            clamp_custom_fantastic_window_ms(23),
            CUSTOM_FANTASTIC_WINDOW_MAX_MS
        );
        assert_eq!(
            clamp_long_error_bar_threshold_ms(0),
            LONG_ERROR_BAR_THRESHOLD_MS_MIN
        );
        assert_eq!(
            clamp_long_error_bar_threshold_ms(99),
            LONG_ERROR_BAR_THRESHOLD_MS_MAX
        );
        assert_eq!(
            clamp_long_error_bar_min_samples(0),
            LONG_ERROR_BAR_MIN_SAMPLES_MIN
        );
        assert_eq!(
            clamp_long_error_bar_min_samples(99),
            LONG_ERROR_BAR_MIN_SAMPLES_MAX
        );
    }

    #[test]
    fn clamp_weight_pounds_preserves_unset_and_bounds_user_values() {
        assert_eq!(clamp_weight_pounds(0), 0);
        assert_eq!(clamp_weight_pounds(-50), 20);
        assert_eq!(clamp_weight_pounds(19), 20);
        assert_eq!(clamp_weight_pounds(120), 120);
        assert_eq!(clamp_weight_pounds(1001), 1000);
    }

    #[test]
    fn profile_stat_defaults_match_itg_fallbacks() {
        assert_eq!(resolved_weight_pounds(0), DEFAULT_WEIGHT_POUNDS);
        assert_eq!(resolved_weight_pounds(165), 165);
        assert_eq!(age_years_for_birth_year(0, 2026), 2026 - DEFAULT_BIRTH_YEAR);
        assert_eq!(age_years_for_birth_year(2000, 2026), 26);
    }

    #[test]
    fn tap_explosion_mask_maps_judgment_windows() {
        assert_eq!(
            tap_explosion_mask_for_window("W0"),
            Some(TapExplosionMask::FANTASTIC)
        );
        assert_eq!(
            tap_explosion_mask_for_window("W1"),
            Some(TapExplosionMask::FANTASTIC)
        );
        assert_eq!(
            tap_explosion_mask_for_window("W5"),
            Some(TapExplosionMask::WAY_OFF)
        );
        assert_eq!(
            tap_explosion_mask_for_window("Miss"),
            Some(TapExplosionMask::MISS)
        );
        assert_eq!(
            tap_explosion_mask_for_window("Held"),
            Some(TapExplosionMask::HELD)
        );
        assert_eq!(tap_explosion_mask_for_window("Holding"), None);
    }

    #[test]
    fn tap_explosion_mask_enabled_checks_window_flags() {
        let mask = TapExplosionMask::MISS | TapExplosionMask::HELD;
        assert!(tap_explosion_mask_enabled(mask, "Miss"));
        assert!(tap_explosion_mask_enabled(mask, "Held"));
        assert!(!tap_explosion_mask_enabled(mask, "W1"));
        assert!(!tap_explosion_mask_enabled(mask, "Holding"));
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
        assert_eq!(
            normalize_tap_explosion_mask(old_all.bits(), TAP_EXPLOSION_MASK_VERSION),
            old_all
        );
    }

    #[test]
    fn player_options_section_serializes_persisted_options() {
        let options = PlayerOptionsData {
            error_bar_active_mask: ErrorBarMask::COLORFUL | ErrorBarMask::TEXT,
            center_tick: true,
            average_error_bar_intensity: 1.13,
            long_error_bar_intensity: 1.95,
            text_error_bar_scalable: true,
            text_error_bar_threshold_ms: 17,
            tap_explosion_active_mask: TapExplosionMask::FANTASTIC | TapExplosionMask::MISS,
            score_position: ScorePosition::StepStatistics,
            score_display_mode: ScoreDisplayMode::Predictive,
            step_stats_extra: StepStatsExtra::CatJAM,
            column_flash_brightness: ColumnFlashBrightness::Dimmed,
            column_flash_size: ColumnFlashSize::Compact,
            mini_percent: 42,
            global_offset_shift_ms: -9,
            no_cmod_alternative: NoCmodAlternative::XMod,
            ..PlayerOptionsData::default()
        };

        let mut content = String::new();
        append_player_options_section(&mut content, "PlayerOptionsSingles", &options);

        assert!(content.starts_with("[PlayerOptionsSingles]\n"));
        assert!(content.contains("ErrorBarMask=5\n"));
        assert!(content.contains("CenterTick=1\n"));
        assert!(content.contains("Colorful=1\n"));
        assert!(content.contains("Text=1\n"));
        assert!(content.contains("TextErrorBarScalable=1\n"));
        assert!(content.contains("TextErrorBar10ms=1\n"));
        assert!(content.contains("TextErrorBarThresholdMs=17\n"));
        assert!(content.contains("AverageErrorBarIntensity=1.25\n"));
        assert!(content.contains("LongErrorBarIntensity=2.00\n"));
        assert!(content.contains("TapExplosionMask=65\n"));
        assert!(content.contains("ScorePosition=Step Statistics\n"));
        assert!(content.contains("ScoreDisplay=Predictive\n"));
        assert!(content.contains("StepStatsExtra=CatJAM\n"));
        assert!(content.contains("NoCmodAlternative=XMod\n"));
        assert!(content.contains("MiniIndicatorSubtractiveDisplay=Percent\n"));
        assert!(content.contains("MiniIndicatorPosition=Default\n"));
        assert!(content.contains(&format!(
            "TapExplosionMaskVersion={TAP_EXPLOSION_MASK_VERSION}\n"
        )));
        assert!(content.contains("HideEarlyDecentWayOffColumnFlash=0\n"));
        assert!(content.contains("ColumnFlashMask=64\n"));
        assert!(content.contains("ColumnFlashBrightness=Dimmed\n"));
        assert!(content.contains("ColumnFlashSize=Compact\n"));
        assert!(content.contains("MiniPercent=42\n"));
        assert!(content.contains("GlobalOffsetShiftMs=-9\n"));
    }

    #[test]
    fn player_options_section_loads_persisted_options() {
        let values = [
            ("StepStatistics", "Judgements, Pack Info"),
            ("StepStatsExtra", "CatJAM"),
            ("TargetScore", "A"),
            ("LifeMeterType", "Vertical"),
            ("MeasureCounter", "16th"),
            ("MeasureCounterLookahead", "9"),
            ("MeasureCounterLeft", "1"),
            ("MeasureCounterUp", "1"),
            ("MeasureCounterVert", "1"),
            ("BrokenRun", "1"),
            ("RunTimer", "1"),
            ("MeasureLines", "Eighth"),
            ("NoCmodAlternative", "XMod"),
            ("Turn", "Mirror"),
            ("InsertMask", "3"),
            ("RemoveMask", "2"),
            ("HoldsMask", "1"),
            ("AccelEffectsMask", "1"),
            ("VisualEffectsMask", "3"),
            ("AppearanceEffectsMask", "1"),
            ("AttackMode", "Off"),
            ("HideLightType", "HideAllLights"),
            ("RescoreEarlyHits", "0"),
            ("HideEarlyDecentWayOffJudgments", "1"),
            ("HideEarlyDecentWayOffFlash", "1"),
            ("HideEarlyDecentWayOffColumnFlash", "1"),
            ("TimingWindows", "WayOffs"),
            ("HideTargets", "1"),
            ("HideSongBG", "1"),
            ("HideCombo", "1"),
            ("HideLifebar", "1"),
            ("HideScore", "1"),
            ("HideDanger", "1"),
            ("HideComboExplosions", "1"),
            ("HideUsername", "1"),
            ("ColumnFlashOnMiss", "1"),
            ("ColumnFlashMask", "64"),
            ("ColumnFlashBrightness", "Dimmed"),
            ("ColumnFlashSize", "Compact"),
            ("SubtractiveScoring", "1"),
            ("Pacemaker", "1"),
            ("NPSGraphAtTop", "1"),
            ("TransparentDensityGraphBackground", "1"),
            ("SmxFsrDisplay", "1"),
            ("SmxPadInputDisplay", "1"),
            ("MiniIndicator", "Pacemaker"),
            ("MiniIndicatorScoreType", "Ex"),
            ("MiniIndicatorSubtractiveDisplay", "Points"),
            ("MiniIndicatorSize", "Large"),
            ("MiniIndicatorColor", "Detailed"),
            ("MiniIndicatorPosition", "UnderUpArrow"),
            ("Scroll", "Reverse+Centered"),
        ];
        let options = load_player_options_section(
            true,
            |key| {
                values
                    .iter()
                    .find_map(|(k, v)| (*k == key).then(|| (*v).to_string()))
            },
            &PlayerOptionsData::default(),
        )
        .unwrap();

        assert!(
            options
                .step_statistics
                .contains(StepStatisticsMask::JUDGMENT_COUNTER)
        );
        assert!(
            options
                .step_statistics
                .contains(StepStatisticsMask::PACK_BANNER)
        );
        assert_eq!(options.step_stats_extra, StepStatsExtra::CatJAM);
        assert_eq!(options.target_score, TargetScoreSetting::A);
        assert_eq!(options.lifemeter_type, LifeMeterType::Vertical);
        assert_eq!(options.measure_counter, MeasureCounter::Sixteenth);
        assert_eq!(options.measure_counter_lookahead, 4);
        assert!(options.measure_counter_left);
        assert!(options.measure_counter_up);
        assert!(options.measure_counter_vert);
        assert!(options.broken_run);
        assert!(options.run_timer);
        assert_eq!(options.measure_lines, MeasureLines::Eighth);
        assert_eq!(options.no_cmod_alternative, NoCmodAlternative::XMod);
        assert_eq!(options.turn_option, TurnOption::Mirror);
        assert_eq!(options.insert_active_mask.bits(), 3);
        assert_eq!(options.remove_active_mask.bits(), 2);
        assert_eq!(options.holds_active_mask.bits(), 1);
        assert_eq!(options.accel_effects_active_mask.bits(), 1);
        assert_eq!(options.visual_effects_active_mask.bits(), 3);
        assert_eq!(options.appearance_effects_active_mask.bits(), 1);
        assert_eq!(options.attack_mode, AttackMode::Off);
        assert_eq!(options.hide_light_type, HideLightType::HideAllLights);
        assert!(!options.rescore_early_hits);
        assert!(options.hide_early_dw_judgments);
        assert!(options.hide_early_dw_flash);
        assert!(options.hide_early_dw_column_flash);
        assert_eq!(options.timing_windows, TimingWindowsOption::WayOffs);
        assert!(options.hide_targets);
        assert!(options.hide_song_bg);
        assert!(options.hide_combo);
        assert!(options.hide_lifebar);
        assert!(options.hide_score);
        assert!(options.hide_danger);
        assert!(options.hide_combo_explosions);
        assert!(options.hide_username);
        assert!(options.column_flash_on_miss);
        assert_eq!(options.column_flash_mask, ColumnFlashMask::MISS);
        assert_eq!(
            options.column_flash_brightness,
            ColumnFlashBrightness::Dimmed
        );
        assert_eq!(options.column_flash_size, ColumnFlashSize::Compact);
        assert!(options.subtractive_scoring);
        assert!(options.pacemaker);
        assert!(options.nps_graph_at_top);
        assert!(options.transparent_density_graph_bg);
        assert!(options.smx_fsr_display);
        assert!(options.smx_pad_input_display);
        assert_eq!(options.mini_indicator, MiniIndicator::Pacemaker);
        assert_eq!(
            options.mini_indicator_score_type,
            MiniIndicatorScoreType::Ex
        );
        assert_eq!(
            options.mini_indicator_subtractive_display,
            MiniIndicatorSubtractiveDisplay::Points
        );
        assert_eq!(options.mini_indicator_size, MiniIndicatorSize::Large);
        assert_eq!(options.mini_indicator_color, MiniIndicatorColor::Detailed);
        assert_eq!(
            options.mini_indicator_position,
            MiniIndicatorPosition::UnderUpArrow
        );
        assert!(options.scroll_option.contains(ScrollOption::Reverse));
        assert!(options.scroll_option.contains(ScrollOption::Centered));
        assert!(options.reverse_scroll);
        assert_eq!(
            load_player_options_section(false, |_| None, &PlayerOptionsData::default()),
            None
        );
    }

    #[test]
    fn profile_ini_renderers_write_profile_and_service_sections() {
        let mut profile = Profile {
            display_name: "Test Player".to_string(),
            player_initials: "TEST".to_string(),
            weight_pounds: 165,
            birth_year: 2000,
            calories_burned_day: "2026-06-23".to_string(),
            calories_burned_today: 12.5,
            ignore_step_count_calories: true,
            ..Profile::default()
        };
        profile.player_options_singles.no_cmod_alternative = NoCmodAlternative::XMod;
        profile.player_options_doubles.no_cmod_alternative = NoCmodAlternative::MMod;

        let profile_ini = render_profile_ini_content("profile-guid", &profile);

        assert!(profile_ini.starts_with("[PlayerOptionsSingles]\n"));
        assert!(profile_ini.contains("[PlayerOptionsDoubles]\n"));
        assert!(profile_ini.contains("[userprofile]\nGuid=profile-guid\n"));
        assert!(profile_ini.contains("DisplayName=Test Player\n"));
        assert!(profile_ini.contains("PlayerInitials=TEST\n"));
        assert!(profile_ini.contains("[Editable]\nWeightPounds=165\nBirthYear=2000\n"));
        assert!(profile_ini.contains("IgnoreStepCountCalories=1\n"));
        assert!(profile_ini.contains("[LastPlayedSingles]\n"));
        assert!(profile_ini.contains("[LastPlayedDoubles]\n"));
        assert!(profile_ini.contains("[LastPlayedCourseSingles]\n"));
        assert!(profile_ini.contains("[LastPlayedCourseDoubles]\n"));
        assert!(
            profile_ini
                .contains("[Stats]\nCaloriesBurnedDate=2026-06-23\nCaloriesBurnedToday=12.5\n")
        );

        assert_eq!(
            render_groovestats_ini_content("gs-key", true, "player"),
            "[GrooveStats]\nApiKey=gs-key\nIsPadPlayer=1\nUsername=player\n\n"
        );
        assert_eq!(
            render_arrowcloud_ini_content("ac-key"),
            "[ArrowCloud]\nApiKey=ac-key\n\n"
        );
    }

    #[test]
    fn profile_credential_files_round_trip_api_keys() {
        let dir = temp_profile_dir("credentials");

        write_groovestats_credentials_dir(&dir, "gs-key", true, "player")
            .expect("GrooveStats credentials should write");
        write_arrowcloud_api_key_dir(&dir, "ac-key").expect("ArrowCloud credentials should write");

        assert_eq!(
            fs::read_to_string(groovestats_ini_path(&dir)).unwrap(),
            "[GrooveStats]\nApiKey=gs-key\nIsPadPlayer=1\nUsername=player\n\n"
        );
        assert_eq!(
            read_groovestats_api_key_dir(&dir).as_deref(),
            Some("gs-key")
        );
        assert_eq!(read_arrowcloud_api_key_dir(&dir), "ac-key");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ensure_local_profile_files_dir_creates_only_missing_files() {
        let dir = temp_profile_dir("ensure-profile-files");
        let guid = "17c7b8a2-3b73-4e8a-9d7d-cfa7e783c00b";
        let profile = Profile {
            display_name: "Default Player".to_string(),
            player_initials: "DFLT".to_string(),
            ..Profile::default()
        };

        ensure_local_profile_files_dir(&dir, guid, &profile)
            .expect("default profile files should be created");
        assert!(profile_ini_path(&dir).is_file());
        assert!(groovestats_ini_path(&dir).is_file());
        assert!(arrowcloud_ini_path(&dir).is_file());
        assert!(
            fs::read_to_string(profile_ini_path(&dir))
                .expect("profile ini should read")
                .contains("DisplayName=Default Player\n")
        );

        write_arrowcloud_api_key_dir(&dir, "existing-key").expect("arrowcloud key should write");
        ensure_local_profile_files_dir(&dir, guid, &profile)
            .expect("existing files should be preserved");
        assert_eq!(read_arrowcloud_api_key_dir(&dir), "existing-key");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn api_key_from_ini_text_supports_spacing_and_empty_keys() {
        assert_eq!(
            api_key_from_ini_text("[ArrowCloud]\nApiKey =  key  \n").as_deref(),
            Some("key")
        );
        assert_eq!(
            api_key_from_ini_text("[GrooveStats]\nApiKey=\n").as_deref(),
            Some("")
        );
        assert_eq!(api_key_from_ini_text("[Other]\nToken=key\n"), None);
    }

    #[test]
    fn no_cmod_alternative_parses_display_and_aliases() {
        // Display round-trips.
        for v in [
            NoCmodAlternative::None,
            NoCmodAlternative::XMod,
            NoCmodAlternative::MMod,
        ] {
            assert_eq!(NoCmodAlternative::from_str(&v.to_string()).unwrap(), v);
        }
        // Case-insensitive aliases and empty/off map to None.
        assert_eq!(
            NoCmodAlternative::from_str("xmod").unwrap(),
            NoCmodAlternative::XMod
        );
        assert_eq!(
            NoCmodAlternative::from_str("M").unwrap(),
            NoCmodAlternative::MMod
        );
        assert_eq!(
            NoCmodAlternative::from_str("").unwrap(),
            NoCmodAlternative::None
        );
        assert_eq!(
            NoCmodAlternative::from_str("off").unwrap(),
            NoCmodAlternative::None
        );
        assert!(NoCmodAlternative::from_str("zmod").is_err());
    }

    #[test]
    fn sanitize_player_initials_limits_to_four_ascii_chars() {
        assert_eq!(sanitize_player_initials("ab?c!de"), "AB?C");
        assert_eq!(sanitize_player_initials("a b-c_d"), "ABCD");
        assert_eq!(sanitize_player_initials(""), "");
        assert_eq!(PLAYER_INITIALS_MAX_LEN, 4);
    }

    #[test]
    fn initials_from_name_uses_two_char_fallbacks() {
        assert_eq!(initials_from_name("john smith"), "JOHN");
        assert_eq!(initials_from_name("a"), "A?");
        assert_eq!(initials_from_name("!!!"), "!!!");
        assert_eq!(initials_from_name(""), "??");
    }

    #[test]
    fn parse_profile_bool_accepts_legacy_boolean_spellings() {
        for value in ["1", "true", "yes", "on", " TRUE "] {
            assert_eq!(parse_profile_bool(value), Some(true));
        }
        for value in ["0", "false", "no", "off", " FALSE "] {
            assert_eq!(parse_profile_bool(value), Some(false));
        }
        assert_eq!(parse_profile_bool("maybe"), None);
    }

    #[test]
    fn groovestats_pad_player_requires_explicit_one() {
        assert!(parse_groovestats_is_pad_player(Some("1"), false));
        assert!(!parse_groovestats_is_pad_player(Some("0"), true));
        assert!(!parse_groovestats_is_pad_player(Some("2"), true));
        assert!(parse_groovestats_is_pad_player(Some("true"), true));
        assert!(!parse_groovestats_is_pad_player(Some("true"), false));
        assert!(parse_groovestats_is_pad_player(None, true));
        assert!(!parse_groovestats_is_pad_player(None, false));
    }

    #[test]
    fn profile_ini_reader_matches_profile_load_section_rules() {
        let ini = ProfileIni::parse(
            "\
; comment
[userprofile]
DisplayName = Alice
PlayerInitials=ALC

[Empty]

LooseKey=loose
# comment
[GrooveStats]
ApiKey = gs-key
",
        );

        assert!(ini.section_has_any("userprofile"));
        assert!(ini.section_has_any("Empty"));
        assert_eq!(
            ini.get("userprofile", "DisplayName").as_deref(),
            Some("Alice")
        );
        assert_eq!(
            ini.get("userprofile", "PlayerInitials").as_deref(),
            Some("ALC")
        );
        assert_eq!(ini.get("GrooveStats", "ApiKey").as_deref(), Some("gs-key"));
        assert_eq!(ini.get("Empty", "LooseKey").as_deref(), Some("loose"));
    }

    #[test]
    fn parse_last_played_value_trims_empty_optional_fields() {
        assert_eq!(parse_last_played_value(None), None);
        assert_eq!(parse_last_played_value(Some("")), None);
        assert_eq!(parse_last_played_value(Some("   ")), None);
        assert_eq!(
            parse_last_played_value(Some(" Songs/Pack/Song.ogg ")),
            Some("Songs/Pack/Song.ogg".to_string())
        );
    }

    #[test]
    fn player_options_section_matches_style_storage() {
        assert_eq!(
            player_options_section(PlayStyle::Single),
            "PlayerOptionsSingles"
        );
        assert_eq!(
            player_options_section(PlayStyle::Versus),
            "PlayerOptionsSingles"
        );
        assert_eq!(
            player_options_section(PlayStyle::Double),
            "PlayerOptionsDoubles"
        );
    }

    #[test]
    fn hud_player_snapshot_defaults_to_guestless_unjoined() {
        let snapshot = GameplayHudPlayerSnapshot::default();
        assert!(!snapshot.joined);
        assert!(!snapshot.guest);
        assert_eq!(snapshot.display_name, "");
        assert_eq!(snapshot.avatar_texture_key, None);
    }

    #[test]
    fn gameplay_hud_snapshot_from_parts_copies_session_profile_state() {
        let mut p1 = Profile::default();
        p1.display_name = "Alice".to_string();
        p1.avatar_texture_key = Some("avatar-p1".to_string());
        p1.hide_username = true;

        let mut p2 = Profile::default();
        p2.display_name = "Bob".to_string();
        p2.avatar_texture_key = Some("avatar-p2".to_string());

        let profiles = [p1, p2];
        let active_profiles = [
            ActiveProfile::Local {
                id: "profile-1".to_string(),
            },
            ActiveProfile::Guest,
        ];
        let snapshot = gameplay_hud_snapshot_from_parts(
            PlayStyle::Versus,
            PlayerSide::P2,
            SESSION_JOINED_MASK_P1,
            &active_profiles,
            &profiles,
        );

        assert_eq!(snapshot.play_style, PlayStyle::Versus);
        assert_eq!(snapshot.player_side, PlayerSide::P2);
        assert!(snapshot.p1.joined);
        assert!(!snapshot.p1.guest);
        assert_eq!(snapshot.p1.display_name, "Alice");
        assert_eq!(snapshot.p1.avatar_texture_key.as_deref(), Some("avatar-p1"));
        assert!(snapshot.p1.hide_username);
        assert!(!snapshot.p2.joined);
        assert!(snapshot.p2.guest);
        assert_eq!(snapshot.p2.display_name, "Bob");
        assert_eq!(snapshot.p2.avatar_texture_key.as_deref(), Some("avatar-p2"));
        assert!(!snapshot.p2.hide_username);
    }

    #[test]
    fn apply_loaded_profile_data_updates_profile_sections_and_sidecars() {
        let mut profile = Profile::default();
        profile.display_name = "stale".to_string();
        let mut default_profile = Profile::default();
        default_profile.store_current_player_options_for_all_styles();

        let today = "2026-07-08";
        let profile_values = HashMap::from([
            (("userprofile", "DisplayName"), "Alice".to_string()),
            (("userprofile", "PlayerInitials"), " a-b_c ".to_string()),
            (("Editable", "WeightPounds"), "180".to_string()),
            (("Stats", "IgnoreStepCountCalories"), "1".to_string()),
            (("Stats", "CaloriesBurnedDate"), today.to_string()),
            (("Stats", "CaloriesBurnedToday"), "12.5".to_string()),
        ]);
        let gs_values = HashMap::from([
            (("GrooveStats", "ApiKey"), "gs-key".to_string()),
            (("GrooveStats", "IsPadPlayer"), "1".to_string()),
            (("GrooveStats", "Username"), "player".to_string()),
        ]);
        let ac_values = HashMap::from([(("ArrowCloud", "ApiKey"), "ac-key".to_string())]);

        apply_loaded_profile_data(
            &mut profile,
            &default_profile,
            PlayStyle::Double,
            today,
            true,
            |section| profile_values.keys().any(|(s, _)| *s == section),
            |section, key| profile_values.get(&(section, key)).cloned(),
            ProfileStats {
                current_combo: 42,
                known_pack_names: HashSet::from(["Pack A".to_string()]),
            },
            HashSet::from(["chart-a".to_string()]),
            HashSet::from(["Pack A".to_string()]),
            true,
            |section, key| gs_values.get(&(section, key)).cloned(),
            true,
            |section, key| ac_values.get(&(section, key)).cloned(),
        );

        assert_eq!(profile.display_name, "Alice");
        assert_eq!(profile.player_initials, "ABC");
        assert_eq!(profile.weight_pounds, 180);
        assert!(profile.ignore_step_count_calories);
        assert_eq!(profile.calories_burned_day, today);
        assert_eq!(profile.calories_burned_today, 12.5);
        assert_eq!(profile.current_combo, 42);
        assert!(profile.known_pack_names.contains("Pack A"));
        assert!(profile.favorites.contains("chart-a"));
        assert!(profile.favorited_packs.contains("Pack A"));
        assert_eq!(profile.groovestats_api_key, "gs-key");
        assert!(profile.groovestats_is_pad_player);
        assert_eq!(profile.groovestats_username, "player");
        assert_eq!(profile.arrowcloud_api_key, "ac-key");
    }

    #[test]
    fn physical_pad_side_follows_doubles_session_side() {
        assert_eq!(
            side_for_physical_pad(PlayStyle::Single, PlayerSide::P1, false),
            PlayerSide::P1
        );
        assert_eq!(
            side_for_physical_pad(PlayStyle::Single, PlayerSide::P1, true),
            PlayerSide::P2
        );
        assert_eq!(
            side_for_physical_pad(PlayStyle::Double, PlayerSide::P2, false),
            PlayerSide::P2
        );
        assert_eq!(
            side_for_physical_pad(PlayStyle::Double, PlayerSide::P2, true),
            PlayerSide::P2
        );
    }

    #[test]
    fn gameplay_player_side_uses_session_side_for_solo_and_index_for_versus() {
        assert_eq!(
            side_for_gameplay_player(1, 0, PlayerSide::P2),
            PlayerSide::P2
        );
        assert_eq!(
            side_for_gameplay_player(2, 0, PlayerSide::P2),
            PlayerSide::P1
        );
        assert_eq!(
            side_for_gameplay_player(2, 1, PlayerSide::P1),
            PlayerSide::P2
        );
    }

    #[test]
    fn default_profile_id_updates_clear_duplicates_and_respect_joined_sides() {
        let p1 = ActiveProfile::Local {
            id: "alpha".to_string(),
        };
        let p2 = ActiveProfile::Local {
            id: "beta".to_string(),
        };

        let defaults = default_profile_ids_after_side_update(
            [Some("old".to_string()), Some("alpha".to_string())],
            PlayerSide::P1,
            &p1,
            |_| true,
        );
        assert_eq!(defaults, [Some("alpha".to_string()), None]);

        let defaults = default_profile_ids_after_side_update(
            [Some("old".to_string()), Some("beta".to_string())],
            PlayerSide::P1,
            &p1,
            |_| false,
        );
        assert_eq!(defaults, [None, Some("beta".to_string())]);

        let defaults = default_profile_ids_after_joined_selection(
            [Some("old-p1".to_string()), Some("old-p2".to_string())],
            SESSION_JOINED_MASK_P2,
            &p1,
            &p2,
        );
        assert_eq!(
            defaults,
            [Some("old-p1".to_string()), Some("beta".to_string())]
        );
    }

    #[test]
    fn profile_delete_clears_matching_default_slots() {
        let defaults = default_profile_ids_after_profile_delete(
            [Some("gone".to_string()), Some("keep".to_string())],
            "gone",
        );
        assert_eq!(defaults, [None, Some("keep".to_string())]);

        let defaults = default_profile_ids_after_profile_delete(
            [Some("keep".to_string()), Some("gone".to_string())],
            "gone",
        );
        assert_eq!(defaults, [Some("keep".to_string()), None]);
    }

    #[test]
    fn profile_create_fills_first_empty_default_slot() {
        let defaults =
            default_profile_ids_after_profile_create([None, Some("p2".to_string())], "new".into());
        assert_eq!(defaults, [Some("new".to_string()), Some("p2".to_string())]);

        let defaults =
            default_profile_ids_after_profile_create([Some("p1".to_string()), None], "new".into());
        assert_eq!(defaults, [Some("p1".to_string()), Some("new".to_string())]);

        let defaults = default_profile_ids_after_profile_create(
            [Some("p1".to_string()), Some("p2".to_string())],
            "new".into(),
        );
        assert_eq!(defaults, [Some("p1".to_string()), Some("p2".to_string())]);
    }

    #[test]
    fn default_player_options_use_machine_noteskin() {
        let (singles, doubles) =
            default_player_options_with_machine_settings(NoteSkin::new("cel"), 60);

        assert_eq!(singles.noteskin.as_str(), "cel");
        assert_eq!(doubles.noteskin.as_str(), "cel");
    }

    #[test]
    fn loaded_profile_rename_updates_matching_active_sides() {
        let active_profiles = [
            ActiveProfile::Local {
                id: "same".to_string(),
            },
            ActiveProfile::Local {
                id: "other".to_string(),
            },
        ];
        let mut profiles = [Profile::default(), Profile::default()];
        profiles[0].display_name = "Old".to_string();
        profiles[1].display_name = "Other".to_string();

        let changed =
            rename_loaded_local_profile(&active_profiles, &mut profiles, "same", "New Name");

        assert_eq!(changed, [true, false]);
        assert_eq!(profiles[0].display_name, "New Name");
        assert_eq!(profiles[1].display_name, "Other");
    }

    #[test]
    fn guest_noteskin_update_skips_loaded_local_profiles() {
        let active_profiles = [
            ActiveProfile::Guest,
            ActiveProfile::Local {
                id: "local".to_string(),
            },
        ];
        let mut profiles = [Profile::default(), Profile::default()];
        profiles[0].noteskin = NoteSkin::new("old-guest");
        profiles[0].player_options_singles.noteskin = NoteSkin::new("old-guest");
        profiles[0].player_options_doubles.noteskin = NoteSkin::new("old-guest");
        profiles[1].noteskin = NoteSkin::new("local-skin");
        profiles[1].player_options_singles.noteskin = NoteSkin::new("local-skin");
        profiles[1].player_options_doubles.noteskin = NoteSkin::new("local-skin");

        let changed =
            update_guest_profile_noteskins(&active_profiles, &mut profiles, NoteSkin::new("cel"));

        assert_eq!(changed, [true, false]);
        assert_eq!(profiles[0].noteskin.as_str(), "cel");
        assert_eq!(profiles[0].player_options_singles.noteskin.as_str(), "cel");
        assert_eq!(profiles[0].player_options_doubles.noteskin.as_str(), "cel");
        assert_eq!(profiles[1].noteskin.as_str(), "local-skin");
        assert_eq!(
            profiles[1].player_options_singles.noteskin.as_str(),
            "local-skin"
        );
        assert_eq!(
            profiles[1].player_options_doubles.noteskin.as_str(),
            "local-skin"
        );
    }

    #[test]
    fn default_profile_helpers_heal_legacy_ids_and_resolve_active_profiles() {
        let guid = "17c7b8a2-3b73-4e8a-9d7d-cfa7e783c00b";
        let mut folder_to_guid: HashMap<&str, &str> = HashMap::new();
        folder_to_guid.insert("00000000", guid);

        assert_eq!(
            heal_default_profile_id(Some("00000000".to_string()), &folder_to_guid).as_deref(),
            Some(guid)
        );
        assert_eq!(
            heal_default_profile_id(Some(guid.to_string()), &folder_to_guid).as_deref(),
            Some(guid)
        );
        assert_eq!(
            heal_default_profile_id(Some("99999999".to_string()), &folder_to_guid).as_deref(),
            Some("99999999")
        );
        assert_eq!(heal_default_profile_id(None, &folder_to_guid), None);

        assert_eq!(
            default_active_profile_from_id(Some("local".to_string()), |_| true),
            ActiveProfile::Local {
                id: "local".to_string()
            }
        );
        assert_eq!(
            default_active_profile_from_id(Some("missing".to_string()), |_| false),
            ActiveProfile::Guest
        );
        assert_eq!(
            default_active_profile_from_id(None, |_| true),
            ActiveProfile::Guest
        );
    }

    #[test]
    fn session_state_restores_defaults_for_joined_sides() {
        let defaults = [Some("p1".to_string()), Some("p2".to_string())];
        let mut session = SessionState::default();
        session.restore_default_profiles(&defaults, |id| id == "p2");
        assert_eq!(session.active_profile(PlayerSide::P1), ActiveProfile::Guest);
        assert_eq!(
            session.active_profile(PlayerSide::P2),
            ActiveProfile::Local {
                id: "p2".to_string()
            }
        );

        session.set_joined_sides(false, true);
        let changed = session.restore_joined_default_profiles(&defaults, |_| true);
        assert_eq!(changed, [false, true]);
        assert_eq!(session.active_profile(PlayerSide::P1), ActiveProfile::Guest);
        assert_eq!(session.active_local_profile_id(PlayerSide::P2), Some("p2"));

        assert!(!session.set_active_profile(
            PlayerSide::P2,
            ActiveProfile::Local {
                id: "p2".to_string()
            }
        ));
        assert!(session.set_active_profile(PlayerSide::P2, ActiveProfile::Guest));
    }

    #[test]
    fn known_pack_loaded_profile_helpers_select_active_matching_profile() {
        let active_profiles = [
            ActiveProfile::Local {
                id: "same".to_string(),
            },
            ActiveProfile::Local {
                id: "same".to_string(),
            },
        ];
        let mut p1 = Profile::default();
        p1.known_pack_names.insert("Alpha".to_string());
        let mut p2 = Profile::default();
        p2.known_pack_names.insert("Beta".to_string());
        let mut profiles = [p1, p2];

        let known = known_pack_names_for_loaded_profile(&active_profiles, &profiles, "same")
            .expect("known packs for active profile");
        assert_eq!(known, HashSet::from(["Alpha".to_string()]));

        let save_side = mark_known_pack_names_for_loaded_profile(
            &active_profiles,
            &mut profiles,
            "same",
            &["Gamma"],
        );
        assert_eq!(save_side, Some(PlayerSide::P1));
        assert!(profiles[0].known_pack_names.contains("Gamma"));
        assert!(profiles[1].known_pack_names.contains("Gamma"));
        assert!(!profile_has_favorited_pack(&profiles[0], "Missing"));
    }

    #[test]
    fn loaded_profile_credential_helpers_update_matching_sides() {
        let active_profiles = [
            ActiveProfile::Local {
                id: "same".to_string(),
            },
            ActiveProfile::Local {
                id: "other".to_string(),
            },
        ];
        let mut profiles = [Profile::default(), Profile::default()];

        set_arrowcloud_api_key_for_side(&mut profiles, PlayerSide::P2, "side-ac");
        assert_eq!(profiles[1].arrowcloud_api_key, "side-ac");

        assert!(set_arrowcloud_api_key_for_loaded_profile(
            &active_profiles,
            &mut profiles,
            "same",
            "loaded-ac",
        ));
        assert_eq!(profiles[0].arrowcloud_api_key, "loaded-ac");
        assert_eq!(profiles[1].arrowcloud_api_key, "side-ac");

        assert!(set_groovestats_credentials_for_loaded_profile(
            &active_profiles,
            &mut profiles,
            "other",
            "gs-key",
            "player",
        ));
        assert_eq!(profiles[1].groovestats_api_key, "gs-key");
        assert_eq!(profiles[1].groovestats_username, "player");
        assert!(profiles[1].groovestats_is_pad_player);

        assert!(!set_arrowcloud_api_key_for_loaded_profile(
            &active_profiles,
            &mut profiles,
            "missing",
            "unused",
        ));
    }

    #[test]
    fn side_favorite_helpers_mutate_and_return_persistable_sets() {
        let mut profiles = [Profile::default(), Profile::default()];

        let (added, favorites) = toggle_favorite_for_side(&mut profiles, PlayerSide::P1, "abc");
        assert!(added);
        assert_eq!(favorites, HashSet::from(["abc".to_string()]));

        let (added, favorites) = toggle_favorite_for_side(&mut profiles, PlayerSide::P1, "abc");
        assert!(!added);
        assert!(favorites.is_empty());

        seed_favorite_for_side(&mut profiles, PlayerSide::P2, "xyz");
        assert!(profiles[1].favorites.contains("xyz"));

        let (added, packs) = toggle_favorited_pack_for_side(&mut profiles, PlayerSide::P2, "Mix");
        assert!(added);
        assert_eq!(packs, HashSet::from(["Mix".to_string()]));

        seed_favorited_pack_for_side(&mut profiles, PlayerSide::P1, "Alpha");
        assert!(profile_has_favorited_pack(&profiles[0], "Alpha"));
    }

    #[test]
    fn side_profile_views_read_display_credentials_and_packs() {
        let mut p1 = Profile::default();
        p1.display_name = "Player One".to_string();
        p1.avatar_texture_key = Some("avatar:p1".to_string());
        p1.groovestats_api_key = "  gs-key  ".to_string();
        p1.pad_light_brightness = 25;
        p1.smx_bg_pack = Some("custom-bg".to_string());
        p1.favorites.insert("chart".to_string());
        p1.favorited_packs.insert("Pack".to_string());

        let mut p2 = Profile::default();
        p2.pad_light_brightness = 75;
        p2.smx_judge_pack = Some("custom-judge".to_string());

        let mut profiles = [p1, p2];
        assert_eq!(
            footer_fields_for_side(&profiles, PlayerSide::P1),
            (Some("avatar:p1".to_string()), "Player One".to_string())
        );
        assert_eq!(
            groovestats_api_key_for_side(&profiles, PlayerSide::P1),
            "gs-key"
        );

        set_avatar_texture_key_for_side(&mut profiles, PlayerSide::P1, None);
        assert_eq!(profiles[0].avatar_texture_key, None);

        assert_eq!(
            pad_light_brightness_for_physical_pad(
                &profiles,
                PlayStyle::Single,
                PlayerSide::P1,
                false,
            ),
            25
        );
        assert_eq!(
            pad_light_brightness_for_physical_pad(
                &profiles,
                PlayStyle::Double,
                PlayerSide::P2,
                false,
            ),
            75
        );

        assert!(profile_has_favorite(&profiles, PlayerSide::P1, "chart"));
        assert!(profile_side_has_favorited_pack(
            &profiles,
            PlayerSide::P1,
            "Pack"
        ));

        let (bg, judge) = smx_pack_names_for_profiles(&profiles, 1u8, 2u8, |name| match name {
            "custom-bg" => 10,
            "custom-judge" => 20,
            _ => 0,
        });
        assert_eq!(bg, [10, 1]);
        assert_eq!(judge, [2, 20]);
    }

    #[test]
    fn last_played_defaults_to_medium_song_and_empty_course() {
        let last_song = LastPlayed::default();
        assert_eq!(last_song.song_music_path, None);
        assert_eq!(last_song.chart_hash, None);
        assert_eq!(last_song.difficulty_index, 2);

        let last_course = LastPlayedCourse::default();
        assert_eq!(last_course.course_path, None);
        assert_eq!(last_course.difficulty_name, None);
    }

    #[test]
    fn last_played_sections_render_empty_and_present_fields() {
        let mut content = String::new();
        append_last_played_section(&mut content, "LastPlayedSingles", &LastPlayed::default());
        assert_eq!(
            content,
            "[LastPlayedSingles]\nMusicPath=\nChartHash=\nDifficultyIndex=2\n\n"
        );

        content.clear();
        append_last_played_section(
            &mut content,
            "LastPlayedDoubles",
            &LastPlayed {
                song_music_path: Some("Songs/Pack/Song.ogg".to_string()),
                chart_hash: Some("abc123".to_string()),
                difficulty_index: 4,
            },
        );
        assert_eq!(
            content,
            "[LastPlayedDoubles]\nMusicPath=Songs/Pack/Song.ogg\nChartHash=abc123\nDifficultyIndex=4\n\n"
        );
    }

    #[test]
    fn last_played_section_loads_present_fields_and_defaults() {
        let default = LastPlayed {
            song_music_path: Some("fallback.ogg".to_string()),
            chart_hash: Some("fallbackhash".to_string()),
            difficulty_index: 3,
        };
        let values = [
            ("MusicPath", " Songs/Pack/Song.ogg "),
            ("ChartHash", "abc123"),
        ];

        let loaded = load_last_played_section(
            true,
            |key| {
                values
                    .iter()
                    .find_map(|(k, v)| (*k == key).then(|| (*v).to_string()))
            },
            &default,
        )
        .expect("present section should load");

        assert_eq!(
            loaded.song_music_path,
            Some("Songs/Pack/Song.ogg".to_string())
        );
        assert_eq!(loaded.chart_hash, Some("abc123".to_string()));
        assert_eq!(loaded.difficulty_index, 3);
        assert_eq!(load_last_played_section(false, |_| None, &default), None);
    }

    #[test]
    fn last_played_course_sections_render_empty_and_present_fields() {
        let mut content = String::new();
        append_last_played_course_section(
            &mut content,
            "LastPlayedCourseSingles",
            &LastPlayedCourse::default(),
        );
        assert_eq!(
            content,
            "[LastPlayedCourseSingles]\nCoursePath=\nDifficultyName=\n\n"
        );

        content.clear();
        append_last_played_course_section(
            &mut content,
            "LastPlayedCourseDoubles",
            &LastPlayedCourse {
                course_path: Some("Courses/Test.crs".to_string()),
                difficulty_name: Some("Hard".to_string()),
            },
        );
        assert_eq!(
            content,
            "[LastPlayedCourseDoubles]\nCoursePath=Courses/Test.crs\nDifficultyName=Hard\n\n"
        );
    }

    #[test]
    fn last_played_course_section_loads_present_fields() {
        let values = [
            ("CoursePath", " Courses/Test.crs "),
            ("DifficultyName", "Hard"),
        ];

        let loaded = load_last_played_course_section(true, |key| {
            values
                .iter()
                .find_map(|(k, v)| (*k == key).then(|| (*v).to_string()))
        })
        .expect("present course section should load");

        assert_eq!(loaded.course_path, Some("Courses/Test.crs".to_string()));
        assert_eq!(loaded.difficulty_name, Some("Hard".to_string()));
        assert_eq!(load_last_played_course_section(false, |_| None), None);
    }

    #[test]
    fn hide_light_type_round_trips() {
        for setting in [
            HideLightType::NoHideLights,
            HideLightType::HideAllLights,
            HideLightType::HideMarqueeLights,
            HideLightType::HideBassLights,
        ] {
            assert_eq!(setting.to_string().parse::<HideLightType>(), Ok(setting));
        }
        assert!(HideLightType::from_str("unknown").is_err());
    }

    #[test]
    fn perspective_round_trips_and_reports_tilt_skew() {
        for (setting, skew) in [
            (Perspective::Overhead, (0.0, 0.0)),
            (Perspective::Hallway, (-1.0, 0.0)),
            (Perspective::Distant, (1.0, 0.0)),
            (Perspective::Incoming, (-1.0, 1.0)),
            (Perspective::Space, (1.0, 1.0)),
        ] {
            assert_eq!(setting.to_string().parse::<Perspective>(), Ok(setting));
            assert_eq!(setting.tilt_skew(), skew);
        }
        assert!(Perspective::from_str("flat").is_err());
    }

    #[test]
    fn turn_option_round_trips_and_accepts_aliases() {
        for setting in [
            TurnOption::None,
            TurnOption::Mirror,
            TurnOption::Left,
            TurnOption::Right,
            TurnOption::LRMirror,
            TurnOption::UDMirror,
            TurnOption::Shuffle,
            TurnOption::Blender,
            TurnOption::Random,
        ] {
            assert_eq!(setting.to_string().parse::<TurnOption>(), Ok(setting));
        }
        assert_eq!(TurnOption::from_str("NoTurn"), Ok(TurnOption::None));
        assert_eq!(
            TurnOption::from_str("super shuffle"),
            Ok(TurnOption::Blender)
        );
        assert_eq!(
            TurnOption::from_str("hyper shuffle"),
            Ok(TurnOption::Random)
        );
        assert!(TurnOption::from_str("up").is_err());
    }

    #[test]
    fn scroll_option_parses_and_formats_combined_flags() {
        for setting in [
            ScrollOption::Normal,
            ScrollOption::Reverse,
            ScrollOption::Split,
            ScrollOption::Alternate,
            ScrollOption::Cross,
            ScrollOption::Centered,
        ] {
            assert_eq!(setting.to_string().parse::<ScrollOption>(), Ok(setting));
        }

        let combined = ScrollOption::from_str("Reverse+Cross Centered").unwrap();
        assert!(combined.contains(ScrollOption::Reverse));
        assert!(combined.contains(ScrollOption::Cross));
        assert!(combined.contains(ScrollOption::Centered));
        assert_eq!(combined.to_string(), "Reverse+Cross+Centered");

        assert_eq!(
            ScrollOption::from_str("Normal,Reverse"),
            Ok(ScrollOption::Reverse)
        );
        assert!(ScrollOption::from_str("").is_err());
        assert!(ScrollOption::from_str("hidden").is_err());
    }

    #[test]
    fn combo_mode_round_trips() {
        for setting in [ComboMode::FullCombo, ComboMode::CurrentCombo] {
            assert_eq!(setting.to_string().parse::<ComboMode>(), Ok(setting));
        }
        assert!(ComboMode::from_str("sessioncombo").is_err());
    }

    #[test]
    fn combo_colors_round_trips() {
        for setting in [
            ComboColors::Glow,
            ComboColors::Solid,
            ComboColors::Rainbow,
            ComboColors::RainbowScroll,
            ComboColors::None,
        ] {
            assert_eq!(setting.to_string().parse::<ComboColors>(), Ok(setting));
        }
        assert!(ComboColors::from_str("flashing").is_err());
    }

    #[test]
    fn combo_font_round_trips_and_accepts_aliases() {
        for setting in [
            ComboFont::Wendy,
            ComboFont::ArialRounded,
            ComboFont::Asap,
            ComboFont::BebasNeue,
            ComboFont::SourceCode,
            ComboFont::Work,
            ComboFont::WendyCursed,
            ComboFont::Mega,
            ComboFont::None,
        ] {
            assert_eq!(setting.to_string().parse::<ComboFont>(), Ok(setting));
        }
        assert_eq!(ComboFont::from_str("bebasneue"), Ok(ComboFont::BebasNeue));
        assert_eq!(ComboFont::from_str("sourcecode"), Ok(ComboFont::SourceCode));
        assert_eq!(
            ComboFont::from_str("wendycursed"),
            Ok(ComboFont::WendyCursed)
        );
        assert!(ComboFont::from_str("comic sans").is_err());
    }

    #[test]
    fn target_score_setting_parses_legacy_forms() {
        for (raw, setting) in [
            ("cminus", TargetScoreSetting::CMinus),
            ("c", TargetScoreSetting::C),
            ("cplus", TargetScoreSetting::CPlus),
            ("bminus", TargetScoreSetting::BMinus),
            ("b", TargetScoreSetting::B),
            ("bplus", TargetScoreSetting::BPlus),
            ("aminus", TargetScoreSetting::AMinus),
            ("a", TargetScoreSetting::A),
            ("aplus", TargetScoreSetting::APlus),
            ("sminus", TargetScoreSetting::SMinus),
            ("", TargetScoreSetting::S),
            ("s", TargetScoreSetting::S),
            ("splus", TargetScoreSetting::SPlus),
            ("machine", TargetScoreSetting::MachineBest),
            ("machinebest", TargetScoreSetting::MachineBest),
            ("personal", TargetScoreSetting::PersonalBest),
            ("personalbest", TargetScoreSetting::PersonalBest),
        ] {
            assert_eq!(TargetScoreSetting::from_str(raw), Ok(setting));
        }

        // Preserve the existing punctuation-stripping parser behavior.
        assert_eq!(
            TargetScoreSetting::from_str("C-"),
            Ok(TargetScoreSetting::C)
        );
        assert_eq!(
            TargetScoreSetting::from_str("A+"),
            Ok(TargetScoreSetting::A)
        );
        assert_eq!(
            TargetScoreSetting::from_str("S-"),
            Ok(TargetScoreSetting::S)
        );
        assert!(TargetScoreSetting::from_str("ss").is_err());
    }

    #[test]
    fn error_bar_style_round_trips() {
        for setting in [
            ErrorBarStyle::None,
            ErrorBarStyle::Colorful,
            ErrorBarStyle::Monochrome,
            ErrorBarStyle::Text,
            ErrorBarStyle::Highlight,
            ErrorBarStyle::Average,
        ] {
            assert_eq!(setting.to_string().parse::<ErrorBarStyle>(), Ok(setting));
        }
        assert!(ErrorBarStyle::from_str("split").is_err());
    }

    #[test]
    fn live_timing_stats_mask_layout_is_stable() {
        assert_eq!(LiveTimingStatsMask::MEAN.bits(), 1 << 0);
        assert_eq!(LiveTimingStatsMask::MEAN_ABS.bits(), 1 << 1);
        assert_eq!(LiveTimingStatsMask::MAX.bits(), 1 << 2);
        assert_eq!(LiveTimingStatsMask::all().bits(), 0b0000_0111);
        assert_eq!(
            LiveTimingStatsMask::from_bits_truncate(u8::MAX),
            LiveTimingStatsMask::all()
        );
    }

    #[test]
    fn error_bar_mask_layout_is_stable() {
        assert_eq!(ErrorBarMask::COLORFUL.bits(), 1 << 0);
        assert_eq!(ErrorBarMask::MONOCHROME.bits(), 1 << 1);
        assert_eq!(ErrorBarMask::TEXT.bits(), 1 << 2);
        assert_eq!(ErrorBarMask::HIGHLIGHT.bits(), 1 << 3);
        assert_eq!(ErrorBarMask::AVERAGE.bits(), 1 << 4);
        assert_eq!(ErrorBarMask::all().bits(), 0b0001_1111);
        assert_eq!(
            ErrorBarMask::from_bits_truncate(u8::MAX),
            ErrorBarMask::all()
        );
    }

    #[test]
    fn error_bar_helpers_roundtrip_through_mask() {
        let mask = error_bar_mask_from_style(ErrorBarStyle::Colorful, true);
        assert!(mask.contains(ErrorBarMask::COLORFUL));
        assert!(mask.contains(ErrorBarMask::TEXT));
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::Colorful);
        assert!(error_bar_text_from_mask(mask));

        let mask = ErrorBarMask::COLORFUL | ErrorBarMask::MONOCHROME;
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::Colorful);

        let mask = error_bar_mask_from_style(ErrorBarStyle::Text, false);
        assert!(mask.contains(ErrorBarMask::TEXT));
        assert!(!mask.contains(ErrorBarMask::COLORFUL));
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::None);
        assert!(error_bar_text_from_mask(mask));

        let mask = error_bar_mask_from_style(ErrorBarStyle::None, false);
        assert!(mask.is_empty());
        assert_eq!(error_bar_style_from_mask(mask), ErrorBarStyle::None);
        assert!(!error_bar_text_from_mask(mask));
    }

    #[test]
    fn appearance_effects_mask_layout_is_stable() {
        assert_eq!(AppearanceEffectsMask::HIDDEN.bits(), 1 << 0);
        assert_eq!(AppearanceEffectsMask::SUDDEN.bits(), 1 << 1);
        assert_eq!(AppearanceEffectsMask::STEALTH.bits(), 1 << 2);
        assert_eq!(AppearanceEffectsMask::BLINK.bits(), 1 << 3);
        assert_eq!(AppearanceEffectsMask::RANDOM_VANISH.bits(), 1 << 4);
        assert_eq!(AppearanceEffectsMask::all().bits(), 0b0001_1111);
        assert_eq!(
            AppearanceEffectsMask::from_bits_truncate(u8::MAX),
            AppearanceEffectsMask::all()
        );
    }

    #[test]
    fn accel_effects_mask_layout_is_stable() {
        assert_eq!(AccelEffectsMask::BOOST.bits(), 1 << 0);
        assert_eq!(AccelEffectsMask::BRAKE.bits(), 1 << 1);
        assert_eq!(AccelEffectsMask::WAVE.bits(), 1 << 2);
        assert_eq!(AccelEffectsMask::EXPAND.bits(), 1 << 3);
        assert_eq!(AccelEffectsMask::BOOMERANG.bits(), 1 << 4);
        assert_eq!(AccelEffectsMask::all().bits(), 0b0001_1111);
        assert_eq!(
            AccelEffectsMask::from_bits_truncate(u8::MAX),
            AccelEffectsMask::all()
        );
    }

    #[test]
    fn holds_mask_layout_is_stable() {
        assert_eq!(HoldsMask::PLANTED.bits(), 1 << 0);
        assert_eq!(HoldsMask::FLOORED.bits(), 1 << 1);
        assert_eq!(HoldsMask::TWISTER.bits(), 1 << 2);
        assert_eq!(HoldsMask::NO_ROLLS.bits(), 1 << 3);
        assert_eq!(HoldsMask::HOLDS_TO_ROLLS.bits(), 1 << 4);
        assert_eq!(HoldsMask::all().bits(), 0b0001_1111);
        assert_eq!(HoldsMask::from_bits_truncate(u8::MAX), HoldsMask::all());
    }

    #[test]
    fn visual_effects_mask_layout_is_stable() {
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
        assert_eq!(
            VisualEffectsMask::from_bits_truncate(u16::MAX),
            VisualEffectsMask::all()
        );
    }

    #[test]
    fn insert_mask_layout_is_stable() {
        assert_eq!(InsertMask::WIDE.bits(), 1 << 0);
        assert_eq!(InsertMask::BIG.bits(), 1 << 1);
        assert_eq!(InsertMask::QUICK.bits(), 1 << 2);
        assert_eq!(InsertMask::BMRIZE.bits(), 1 << 3);
        assert_eq!(InsertMask::SKIPPY.bits(), 1 << 4);
        assert_eq!(InsertMask::ECHO.bits(), 1 << 5);
        assert_eq!(InsertMask::STOMP.bits(), 1 << 6);
        assert_eq!(InsertMask::all().bits(), 0b0111_1111);
        assert_eq!(InsertMask::from_bits_truncate(u8::MAX), InsertMask::all());
    }

    #[test]
    fn remove_mask_layout_is_stable() {
        assert_eq!(RemoveMask::LITTLE.bits(), 1 << 0);
        assert_eq!(RemoveMask::NO_MINES.bits(), 1 << 1);
        assert_eq!(RemoveMask::NO_HOLDS.bits(), 1 << 2);
        assert_eq!(RemoveMask::NO_JUMPS.bits(), 1 << 3);
        assert_eq!(RemoveMask::NO_HANDS.bits(), 1 << 4);
        assert_eq!(RemoveMask::NO_QUADS.bits(), 1 << 5);
        assert_eq!(RemoveMask::NO_LIFTS.bits(), 1 << 6);
        assert_eq!(RemoveMask::NO_FAKES.bits(), 1 << 7);
        assert_eq!(RemoveMask::all().bits(), u8::MAX);
        assert_eq!(RemoveMask::from_bits_truncate(u8::MAX), RemoveMask::all());
    }

    #[test]
    fn tap_explosion_mask_layout_is_stable() {
        assert_eq!(TapExplosionMask::FANTASTIC.bits(), 1 << 0);
        assert_eq!(TapExplosionMask::EXCELLENT.bits(), 1 << 1);
        assert_eq!(TapExplosionMask::GREAT.bits(), 1 << 2);
        assert_eq!(TapExplosionMask::DECENT.bits(), 1 << 3);
        assert_eq!(TapExplosionMask::WAY_OFF.bits(), 1 << 4);
        assert_eq!(TapExplosionMask::HELD.bits(), 1 << 5);
        assert_eq!(TapExplosionMask::MISS.bits(), 1 << 6);
        assert_eq!(TapExplosionMask::HOLDING.bits(), 1 << 7);
        assert_eq!(TapExplosionMask::all().bits(), u8::MAX);
        assert_eq!(
            TapExplosionMask::from_bits_truncate(u8::MAX),
            TapExplosionMask::all()
        );
    }

    #[test]
    fn column_flash_mask_layout_is_stable() {
        assert_eq!(ColumnFlashMask::BLUE_FANTASTIC.bits(), 1 << 0);
        assert_eq!(ColumnFlashMask::WHITE_FANTASTIC.bits(), 1 << 1);
        assert_eq!(ColumnFlashMask::EXCELLENT.bits(), 1 << 2);
        assert_eq!(ColumnFlashMask::GREAT.bits(), 1 << 3);
        assert_eq!(ColumnFlashMask::DECENT.bits(), 1 << 4);
        assert_eq!(ColumnFlashMask::WAY_OFF.bits(), 1 << 5);
        assert_eq!(ColumnFlashMask::MISS.bits(), 1 << 6);
        assert_eq!(ColumnFlashMask::all().bits(), 0b0111_1111);
        assert_eq!(
            ColumnFlashMask::from_bits_truncate(u8::MAX),
            ColumnFlashMask::all()
        );
        assert!(column_flash_mask_enabled(
            ColumnFlashMask::MISS,
            JudgeGrade::Miss,
            false
        ));
        assert!(!column_flash_mask_enabled(
            ColumnFlashMask::MISS,
            JudgeGrade::Great,
            false
        ));
        assert!(column_flash_mask_enabled(
            ColumnFlashMask::BLUE_FANTASTIC,
            JudgeGrade::Fantastic,
            true
        ));
        assert!(column_flash_mask_enabled(
            ColumnFlashMask::WHITE_FANTASTIC,
            JudgeGrade::Fantastic,
            false
        ));
    }

    #[test]
    fn column_flash_visual_options_round_trip() {
        for setting in [ColumnFlashBrightness::Normal, ColumnFlashBrightness::Dimmed] {
            assert_eq!(
                setting.to_string().parse::<ColumnFlashBrightness>(),
                Ok(setting)
            );
        }
        for setting in [ColumnFlashSize::Default, ColumnFlashSize::Compact] {
            assert_eq!(setting.to_string().parse::<ColumnFlashSize>(), Ok(setting));
        }
        assert_eq!(
            ColumnFlashBrightness::from_str("standard"),
            Ok(ColumnFlashBrightness::Normal)
        );
        assert_eq!(
            ColumnFlashBrightness::from_str("dim"),
            Ok(ColumnFlashBrightness::Dimmed)
        );
        assert_eq!(
            ColumnFlashSize::from_str("short"),
            Ok(ColumnFlashSize::Compact)
        );
        assert!(ColumnFlashBrightness::from_str("brightest").is_err());
        assert!(ColumnFlashSize::from_str("wide").is_err());
    }

    #[test]
    fn attack_mode_round_trips() {
        for setting in [AttackMode::Off, AttackMode::On, AttackMode::Random] {
            assert_eq!(setting.to_string().parse::<AttackMode>(), Ok(setting));
        }
        assert_eq!(AttackMode::from_str("NoAttacks"), Ok(AttackMode::Off));
        assert_eq!(AttackMode::from_str("normal"), Ok(AttackMode::On));
        assert_eq!(
            AttackMode::from_str("random attacks"),
            Ok(AttackMode::Random)
        );
        assert!(AttackMode::from_str("chaos").is_err());
    }

    #[test]
    fn score_position_round_trips_and_accepts_stepstats_alias() {
        for setting in [ScorePosition::Normal, ScorePosition::StepStatistics] {
            assert_eq!(setting.to_string().parse::<ScorePosition>(), Ok(setting));
        }
        assert_eq!(
            ScorePosition::from_str("stepstats"),
            Ok(ScorePosition::StepStatistics)
        );
        assert_eq!(ScorePosition::from_str("top"), Ok(ScorePosition::Normal));
        assert!(ScorePosition::from_str("middle").is_err());
    }

    #[test]
    fn score_display_mode_round_trips_and_accepts_prediction_alias() {
        for setting in [ScoreDisplayMode::Normal, ScoreDisplayMode::Predictive] {
            assert_eq!(setting.to_string().parse::<ScoreDisplayMode>(), Ok(setting));
        }
        assert_eq!(
            ScoreDisplayMode::from_str("prediction"),
            Ok(ScoreDisplayMode::Predictive)
        );
        assert_eq!(
            ScoreDisplayMode::from_str("actual"),
            Ok(ScoreDisplayMode::Normal)
        );
        assert!(ScoreDisplayMode::from_str("middle").is_err());
    }

    #[test]
    fn scatterplot_max_window_round_trips() {
        for setting in [
            ScatterplotMaxWindow::Off,
            ScatterplotMaxWindow::Fantastic,
            ScatterplotMaxWindow::Excellent,
            ScatterplotMaxWindow::Great,
        ] {
            assert_eq!(
                setting.to_string().parse::<ScatterplotMaxWindow>(),
                Ok(setting)
            );
        }
        assert_eq!(
            ScatterplotMaxWindow::from_str("autoscale"),
            Ok(ScatterplotMaxWindow::Off)
        );
        assert_eq!(
            ScatterplotMaxWindow::from_str("fa"),
            Ok(ScatterplotMaxWindow::Fantastic)
        );
        assert_eq!(
            ScatterplotMaxWindow::from_str("excellent max"),
            Ok(ScatterplotMaxWindow::Excellent)
        );
        assert_eq!(
            ScatterplotMaxWindow::from_str("greatmax"),
            Ok(ScatterplotMaxWindow::Great)
        );
        assert!(ScatterplotMaxWindow::from_str("decent").is_err());
    }

    #[test]
    fn life_meter_type_round_trips() {
        for setting in [
            LifeMeterType::Standard,
            LifeMeterType::Surround,
            LifeMeterType::Vertical,
        ] {
            assert_eq!(setting.to_string().parse::<LifeMeterType>(), Ok(setting));
        }
        assert_eq!(LifeMeterType::from_str(""), Ok(LifeMeterType::Standard));
        assert!(LifeMeterType::from_str("horizontal").is_err());
    }

    #[test]
    fn error_bar_trim_round_trips() {
        for setting in [
            ErrorBarTrim::Off,
            ErrorBarTrim::Fantastic,
            ErrorBarTrim::Excellent,
            ErrorBarTrim::Great,
        ] {
            assert_eq!(setting.to_string().parse::<ErrorBarTrim>(), Ok(setting));
        }
        assert!(ErrorBarTrim::from_str("decent").is_err());
    }

    #[test]
    fn timing_windows_option_round_trips_and_reports_disabled_windows() {
        for (setting, disabled) in [
            (TimingWindowsOption::None, [false; 5]),
            (
                TimingWindowsOption::WayOffs,
                [false, false, false, false, true],
            ),
            (
                TimingWindowsOption::DecentsAndWayOffs,
                [false, false, false, true, true],
            ),
            (
                TimingWindowsOption::FantasticsAndExcellents,
                [true, true, false, false, false],
            ),
        ] {
            assert_eq!(
                setting.to_string().parse::<TimingWindowsOption>(),
                Ok(setting)
            );
            assert_eq!(setting.disabled_windows(), disabled);
        }
        assert_eq!(
            TimingWindowsOption::from_str("decents and way offs"),
            Ok(TimingWindowsOption::DecentsAndWayOffs)
        );
        assert_eq!(
            TimingWindowsOption::from_str("fantastics+excellents"),
            Ok(TimingWindowsOption::FantasticsAndExcellents)
        );
        assert!(TimingWindowsOption::from_str("misses").is_err());
    }

    #[test]
    fn step_statistics_mask_round_trips_and_accepts_legacy_aliases() {
        let mask = StepStatisticsMask::DENSITY_GRAPH
            | StepStatisticsMask::SONG_BANNER
            | StepStatisticsMask::JUDGMENT_COUNTER
            | StepStatisticsMask::STEP_COUNTS;

        assert_eq!(mask.to_string().parse::<StepStatisticsMask>(), Ok(mask));
        assert_eq!(
            StepStatisticsMask::from_str("target"),
            Ok(StepStatisticsMask::empty())
        );
        assert_eq!(
            StepStatisticsMask::from_str("stepstats"),
            Ok(StepStatisticsMask::all_widgets())
        );
        assert_eq!(
            StepStatisticsMask::from_str("Judgements Counter, Peak NPS"),
            Ok(StepStatisticsMask::JUDGMENT_COUNTER | StepStatisticsMask::PEAK_NPS)
        );
        assert_eq!(
            StepStatisticsMask::from_str("Judgements, Pack Info"),
            Ok(StepStatisticsMask::JUDGMENT_COUNTER | StepStatisticsMask::PACK_BANNER)
        );
        assert_eq!(
            StepStatisticsMask::from_str("Song Info, Pack Banner"),
            Ok(StepStatisticsMask::PACK_BANNER)
        );
        assert_eq!(
            StepStatisticsMask::from_str("Step Counts, GS Box"),
            Ok(StepStatisticsMask::STEP_COUNTS)
        );
        assert!(StepStatisticsMask::from_str("lanes").is_err());
    }

    #[test]
    fn step_stats_extra_round_trips_and_accepts_arrow_cloud_names() {
        for setting in [
            StepStatsExtra::None,
            StepStatsExtra::ErrorStats,
            StepStatsExtra::AmongUs,
            StepStatsExtra::Bocchi,
            StepStatsExtra::BrodyQuest,
            StepStatsExtra::CatJAM,
            StepStatsExtra::CrabPls,
            StepStatsExtra::DancingDuck,
            StepStatsExtra::DonChan,
            StepStatsExtra::NyanCat,
            StepStatsExtra::Randomizer,
            StepStatsExtra::RinCat,
            StepStatsExtra::Snoop,
            StepStatsExtra::Sonic,
        ] {
            assert_eq!(setting.to_string().parse::<StepStatsExtra>(), Ok(setting));
        }
        assert_eq!(
            StepStatsExtra::from_str("DancingDuck"),
            Ok(StepStatsExtra::DancingDuck)
        );
        assert_eq!(
            StepStatsExtra::from_str("NyanCat"),
            Ok(StepStatsExtra::NyanCat)
        );
        assert_eq!(
            StepStatsExtra::from_str("RinCat"),
            Ok(StepStatsExtra::RinCat)
        );
        assert!(StepStatsExtra::from_str("lanes").is_err());
    }

    #[test]
    fn measure_counter_round_trips_and_reports_stream_thresholds() {
        for (setting, threshold, multiplier) in [
            (MeasureCounter::None, None, 1.0),
            (MeasureCounter::Eighth, Some(8), 1.0),
            (MeasureCounter::Twelfth, Some(12), 1.0),
            (MeasureCounter::Sixteenth, Some(16), 1.0),
            (MeasureCounter::TwentyFourth, Some(24), 1.5),
            (MeasureCounter::ThirtySecond, Some(32), 2.0),
        ] {
            assert_eq!(setting.to_string().parse::<MeasureCounter>(), Ok(setting));
            assert_eq!(setting.notes_threshold(), threshold);
            assert_eq!(setting.multiplier(), multiplier);
        }
        assert!(MeasureCounter::from_str("quarter").is_err());
    }

    #[test]
    fn measure_lines_round_trips() {
        for setting in [
            MeasureLines::Off,
            MeasureLines::Measure,
            MeasureLines::Quarter,
            MeasureLines::Eighth,
        ] {
            assert_eq!(setting.to_string().parse::<MeasureLines>(), Ok(setting));
        }
        assert!(MeasureLines::from_str("sixteenth").is_err());
    }

    #[test]
    fn mini_indicator_round_trips_and_accepts_aliases() {
        for setting in [
            MiniIndicator::None,
            MiniIndicator::SubtractiveScoring,
            MiniIndicator::PredictiveScoring,
            MiniIndicator::PaceScoring,
            MiniIndicator::RivalScoring,
            MiniIndicator::Pacemaker,
            MiniIndicator::StreamProg,
        ] {
            assert_eq!(setting.to_string().parse::<MiniIndicator>(), Ok(setting));
        }
        assert_eq!(
            MiniIndicator::from_str("subtractive"),
            Ok(MiniIndicator::SubtractiveScoring)
        );
        assert_eq!(
            MiniIndicator::from_str("stream progress"),
            Ok(MiniIndicator::StreamProg)
        );
        assert!(MiniIndicator::from_str("combo").is_err());
    }

    #[test]
    fn mini_indicator_score_type_round_trips_and_accepts_hex_alias() {
        for setting in [
            MiniIndicatorScoreType::Itg,
            MiniIndicatorScoreType::Ex,
            MiniIndicatorScoreType::HardEx,
        ] {
            assert_eq!(
                setting.to_string().parse::<MiniIndicatorScoreType>(),
                Ok(setting)
            );
        }
        assert_eq!(
            MiniIndicatorScoreType::from_str("hex"),
            Ok(MiniIndicatorScoreType::HardEx)
        );
        assert!(MiniIndicatorScoreType::from_str("percent").is_err());
    }

    #[test]
    fn mini_indicator_size_round_trips_and_accepts_big_alias() {
        for setting in [MiniIndicatorSize::Default, MiniIndicatorSize::Large] {
            assert_eq!(
                setting.to_string().parse::<MiniIndicatorSize>(),
                Ok(setting)
            );
        }
        assert_eq!(
            MiniIndicatorSize::from_str("big"),
            Ok(MiniIndicatorSize::Large)
        );
        assert!(MiniIndicatorSize::from_str("small").is_err());
    }

    #[test]
    fn mini_indicator_color_round_trips() {
        for setting in [
            MiniIndicatorColor::Default,
            MiniIndicatorColor::Detailed,
            MiniIndicatorColor::Combo,
        ] {
            assert_eq!(
                setting.to_string().parse::<MiniIndicatorColor>(),
                Ok(setting)
            );
        }
        assert!(MiniIndicatorColor::from_str("rainbow").is_err());
    }

    #[test]
    fn mini_indicator_subtractive_display_round_trips() {
        for setting in [
            MiniIndicatorSubtractiveDisplay::Percent,
            MiniIndicatorSubtractiveDisplay::Points,
        ] {
            assert_eq!(
                setting
                    .to_string()
                    .parse::<MiniIndicatorSubtractiveDisplay>(),
                Ok(setting)
            );
        }
        assert_eq!(
            MiniIndicatorSubtractiveDisplay::from_str("dance points"),
            Ok(MiniIndicatorSubtractiveDisplay::Points)
        );
        assert!(MiniIndicatorSubtractiveDisplay::from_str("combo").is_err());
    }

    #[test]
    fn mini_indicator_position_round_trips() {
        for setting in [
            MiniIndicatorPosition::Default,
            MiniIndicatorPosition::UnderUpArrow,
        ] {
            assert_eq!(
                setting.to_string().parse::<MiniIndicatorPosition>(),
                Ok(setting)
            );
        }
        assert_eq!(
            MiniIndicatorPosition::from_str("under up arrow"),
            Ok(MiniIndicatorPosition::UnderUpArrow)
        );
        assert!(MiniIndicatorPosition::from_str("score").is_err());
    }

    #[test]
    fn background_filter_default_matches_legacy_darkest_value() {
        assert_eq!(BackgroundFilter::default(), BackgroundFilter::DEFAULT);
        assert_eq!(BackgroundFilter::default().percent(), 95);
    }

    #[test]
    fn background_filter_from_percent_clamps_above_max() {
        assert_eq!(BackgroundFilter::from_percent(200).percent(), 100);
        assert_eq!(BackgroundFilter::from_i32(-5).percent(), 0);
        assert_eq!(BackgroundFilter::from_i32(250).percent(), 100);
    }

    #[test]
    fn background_filter_alpha_maps_percent_to_unit_range() {
        assert!((BackgroundFilter::from_percent(0).alpha() - 0.0).abs() < 1e-6);
        assert!((BackgroundFilter::from_percent(100).alpha() - 1.0).abs() < 1e-6);
        assert!((BackgroundFilter::from_percent(50).alpha() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn background_filter_migrates_legacy_enum_labels() {
        assert_eq!(
            BackgroundFilter::from_str("Off").unwrap(),
            BackgroundFilter::OFF
        );
        assert_eq!(
            BackgroundFilter::from_str("Dark").unwrap(),
            BackgroundFilter::from_percent(50)
        );
        assert_eq!(
            BackgroundFilter::from_str("DARKER").unwrap(),
            BackgroundFilter::from_percent(75)
        );
        assert_eq!(
            BackgroundFilter::from_str("darkest").unwrap(),
            BackgroundFilter::from_percent(95)
        );
    }

    #[test]
    fn background_filter_parses_numeric_with_optional_percent_suffix() {
        assert_eq!(
            BackgroundFilter::from_str("0").unwrap(),
            BackgroundFilter::OFF
        );
        assert_eq!(
            BackgroundFilter::from_str("42").unwrap(),
            BackgroundFilter::from_percent(42)
        );
        assert_eq!(
            BackgroundFilter::from_str("42%").unwrap(),
            BackgroundFilter::from_percent(42)
        );
        assert_eq!(
            BackgroundFilter::from_str("100").unwrap(),
            BackgroundFilter::from_percent(100)
        );
    }

    #[test]
    fn background_filter_rejects_out_of_range_or_garbage() {
        assert!(BackgroundFilter::from_str("101").is_err());
        assert!(BackgroundFilter::from_str("-1").is_err());
        assert!(BackgroundFilter::from_str("Dimmer").is_err());
        assert!(BackgroundFilter::from_str("").is_err());
    }

    #[test]
    fn background_filter_display_round_trips_through_from_str() {
        for v in [0u8, 1, 25, 50, 75, 95, 100] {
            let filter = BackgroundFilter::from_percent(v);
            let s = filter.to_string();
            let parsed = BackgroundFilter::from_str(&s).expect("must round-trip");
            assert_eq!(parsed, filter);
        }
    }

    #[test]
    fn noteskin_normalizes_names_and_preserves_none_choice() {
        assert_eq!(NoteSkin::default().as_str(), NoteSkin::CEL_NAME);
        assert_eq!(NoteSkin::new(" Default ").as_str(), NoteSkin::DEFAULT_NAME);
        assert_eq!(NoteSkin::none_choice().as_str(), NoteSkin::NONE_NAME);
        assert!(NoteSkin::from_str("").is_err());
    }

    #[test]
    fn noteskin_resolution_uses_override_or_fallback() {
        let fallback = NoteSkin::new("metal");
        let override_skin = NoteSkin::new("cyber");

        assert_eq!(resolve_noteskin_choice(None, &fallback), &fallback);
        assert_eq!(
            resolve_noteskin_choice(Some(&override_skin), &fallback),
            &override_skin
        );
    }

    #[test]
    fn tap_explosion_skin_resolution_hides_none_choice() {
        let fallback = NoteSkin::new("metal");
        let override_skin = NoteSkin::new("cyber");
        let hidden = NoteSkin::none_choice();

        assert!(!tap_explosion_skin_hidden(None));
        assert!(!tap_explosion_skin_hidden(Some(&override_skin)));
        assert!(tap_explosion_skin_hidden(Some(&hidden)));
        assert_eq!(resolve_tap_explosion_skin(None, &fallback), Some(&fallback));
        assert_eq!(
            resolve_tap_explosion_skin(Some(&override_skin), &fallback),
            Some(&override_skin)
        );
        assert_eq!(resolve_tap_explosion_skin(Some(&hidden), &fallback), None);
    }

    #[test]
    fn graphic_settings_normalize_stock_aliases_and_none() {
        assert_eq!(
            JudgmentGraphic::new("Wendy").as_str(),
            "judgements/Wendy 2x7 (doubleres).png"
        );
        assert_eq!(
            HoldJudgmentGraphic::new("itg2").as_str(),
            "hold_judgements/ITG2 1x2 (doubleres).png"
        );
        assert_eq!(HeldMissGraphic::new("none").as_str(), "None");
        assert_eq!(
            JudgmentGraphic::from_str("custom.png").unwrap().as_str(),
            "judgements/custom.png"
        );
        assert!(HoldJudgmentGraphic::from_str("").is_err());
    }
}
