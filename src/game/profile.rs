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

pub use deadsync_profile::ImportProfileData;
pub use deadsync_profile::app_runtime::{
    append_local_score_for_id, cache_logged_gs_score_for_id,
    cached_ac_chart_hashes_with_itg_for_id, cached_ac_scores_for_id, cached_ac_scores_for_side,
    cached_best_itg_score_for_id, cached_best_itg_score_for_side, cached_gs_chart_hashes_for_id,
    cached_gs_score_for_id, cached_gs_score_for_side, cached_itl_score_for_side,
    cached_itl_score_for_song, cached_itl_score_for_song_assume_loaded,
    cached_itl_score_for_song_with_profile, cached_itl_tournament_overall_ranks_for_side,
    cached_local_ex_score_for_side, cached_local_hard_ex_score_for_side,
    cached_local_itg_score_for_id, cached_local_pass_rate_for_id,
    cached_local_scalar_score_for_side, cached_local_score_for_side,
    cached_online_itl_self_rank_for_key, cached_online_itl_self_rank_for_key_assume_loaded,
    cached_online_itl_self_rank_for_side, cached_online_itl_self_score_for_key,
    cached_online_itl_self_score_for_key_assume_loaded, cached_online_itl_self_score_for_side,
    delete_pad_config, ensure_itl_score_cache_loaded_for_id, ensure_itl_wheel_caches_loaded_for_id,
    ensure_score_caches_loaded_for_id, get_arrowcloud_api_key_for_id,
    get_groovestats_api_key_for_id, import_itl_json, import_local_scores_for_id,
    invalidate_player_leaderboard_chart_for_side, itl_song_folder_unlocked_for_side,
    itl_song_folder_unlocked_with_profile, load_pad_configs, local_profile_dir_for_id,
    local_score_profile_source_for_id, local_score_profile_sources, machine_leaderboard_local,
    machine_leaderboard_local_with_names, machine_leaderboard_local_without_names,
    machine_record_local, machine_replays_local, mark_known_pack_names_for_local_profile,
    mark_pack_known, mark_packs_known, personal_leaderboard_local_for_side,
    played_chart_counts_for_id, played_chart_counts_for_machine, prewarm_select_music_score_caches,
    read_itl_file_for_id, recent_played_chart_hashes_for_id,
    recent_played_chart_hashes_for_machine, rename_pad_config, save_local_summary_score_for_side,
    save_pad_configs, score_profile_paths_for_id, seed_session_gs_score_for_id,
    seed_session_itl_unlock_folders, seed_session_local_itg_score_for_id,
    seed_session_online_itl_self_rank, seed_session_online_itl_self_score,
    set_arrowcloud_api_key_for_id, set_arrowcloud_api_key_for_side, set_cached_itl_file_for_id,
    set_cached_online_itl_self_rank, set_cached_online_itl_self_score, set_default_pad_config,
    set_groovestats_credentials_for_id, set_groovestats_credentials_for_side, sync_known_packs,
    total_songs_played_for_id, total_songs_played_for_side, update_itl_unlock_folders,
    upsert_pad_config, write_ac_submit_scores_for_id, write_cached_ac_scores_for_id_bulk,
    write_cached_gs_score_for_id, write_imported_favorites, write_imported_profile_stats,
    write_itl_file_for_id,
};
use deadsync_profile::app_runtime::{save_profile_ini_for_side, save_profile_stats_for_side};
pub use deadsync_profile::update::*;
use deadsync_profile::{
    ActiveProfile, NoteSkin, PLAYER_SLOTS, PlayerOptionsData, PlayerSide, Profile,
};
pub use deadsync_profile::{
    runtime_active_local_profile_id_for_side as active_local_profile_id_for_side,
    runtime_active_profile_for_side as get_active_profile_for_side, runtime_current_profile as get,
    runtime_fast_profile_switch_from_select_music as fast_profile_switch_from_select_music,
    runtime_footer_fields_for_side as footer_fields_for_side,
    runtime_gameplay_hud_snapshot as gameplay_hud_snapshot,
    runtime_groovestats_api_key_for_side as groovestats_api_key_for_side,
    runtime_known_pack_names_for_local_profile as known_pack_names_for_local_profile,
    runtime_local_profile_id_for_pad as active_local_profile_id_for_pad,
    runtime_pad_light_brightness_for_pad as pad_light_brightness_for_pad,
    runtime_profile_for_side as get_for_side, runtime_profile_has_favorite_for_side as is_favorite,
    runtime_profile_has_favorited_pack_for_side as is_pack_favorite,
    runtime_seed_favorite_for_side as seed_session_favorite,
    runtime_seed_favorited_pack_for_side as seed_session_favorited_pack,
    runtime_session_music_rate as get_session_music_rate,
    runtime_session_play_mode as get_session_play_mode,
    runtime_session_play_style as get_session_play_style,
    runtime_session_player_side as get_session_player_side,
    runtime_session_side_guest as is_session_side_guest,
    runtime_session_side_joined as is_session_side_joined,
    runtime_session_timing_tick_mode as get_session_timing_tick_mode,
    runtime_set_avatar_texture_key_for_side as set_avatar_texture_key_for_side,
    runtime_set_fast_profile_switch_from_select_music as set_fast_profile_switch_from_select_music,
    runtime_set_session_joined as set_session_joined,
    runtime_set_session_music_rate as set_session_music_rate,
    runtime_set_session_play_mode as set_session_play_mode,
    runtime_set_session_play_style as set_session_play_style,
    runtime_set_session_player_side as set_session_player_side,
    runtime_set_session_timing_tick_mode as set_session_timing_tick_mode,
    runtime_take_fast_profile_switch_from_select_music as take_fast_profile_switch_from_select_music,
};

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
    deadsync_profile::app_runtime::update_machine_default_noteskin(
        &config::machine_default_noteskin(),
        setting,
        config::update_machine_default_noteskin,
    );
}

fn load_for_side(side: PlayerSide) {
    deadsync_profile::app_runtime::load_profile_for_side(
        side,
        machine_default_noteskin_value(),
        machine_default_light_brightness(),
    );
}

pub fn load() {
    deadsync_profile::update::set_profile_update_persistence_callbacks(
        save_profile_ini_for_side,
        save_profile_stats_for_side,
    );
    let (p1, p2) = config::default_profiles();
    deadsync_profile::app_runtime::migrate_local_profiles(
        [p1.clone(), p2.clone()],
        config::update_default_profiles,
    );
    deadsync_profile::app_runtime::restore_default_profiles([p1, p2]);
    load_for_side(PlayerSide::P1);
    load_for_side(PlayerSide::P2);
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
    deadsync_profile::app_runtime::smx_gif_packs(
        machine_bg,
        machine_judge,
        crate::config::SmxPackName::parse,
    )
}

pub fn get_default_profile_for_side(side: PlayerSide) -> ActiveProfile {
    let (p1, p2) = config::default_profiles();
    deadsync_profile::app_runtime::default_profile_for_side([p1, p2], side)
}

pub fn default_local_profile_id_for_side(side: PlayerSide) -> Option<String> {
    let (p1, p2) = config::default_profiles();
    deadsync_profile::app_runtime::default_local_profile_id_for_side([p1, p2], side)
}

pub fn set_default_profile_for_side(side: PlayerSide, profile: ActiveProfile) {
    let (p1, p2) = config::default_profiles();
    deadsync_profile::app_runtime::update_default_profile_for_side(
        [p1, p2],
        side,
        &profile,
        config::update_default_profiles,
    );
}

pub fn scorebox_profile_snapshot(
    player_profile: &Profile,
    side_joined: bool,
    persistent_profile_id: Option<String>,
) -> deadsync_score::GameplayScoreboxProfileSnapshot {
    let cfg = config::get();
    deadsync_profile::scorebox_profile_snapshot(
        player_profile,
        side_joined,
        cfg.enable_groovestats,
        cfg.enable_arrowcloud,
        cfg.auto_populate_gs_scores,
        persistent_profile_id,
    )
}

pub fn player_leaderboard_profile_snapshot_for_side(
    side: PlayerSide,
) -> deadsync_score::GameplayScoreboxProfileSnapshot {
    let cfg = config::get();
    deadsync_profile::runtime_scorebox_profile_snapshot_for_side(
        side,
        cfg.enable_groovestats,
        cfg.enable_arrowcloud,
        cfg.auto_populate_gs_scores,
    )
}

pub fn is_groovestats_active_for_side(side: PlayerSide) -> bool {
    deadsync_profile::groovestats_side_active(
        config::get().enable_groovestats,
        is_session_side_joined(side),
        &get_for_side(side).groovestats_api_key,
    )
}

pub fn cached_local_pass_rate_with_profile(chart_hash: &str, profile_id: &str) -> Option<u32> {
    cached_local_pass_rate_for_id(profile_id, chart_hash)
}

pub fn cached_best_itg_score_with_profile(
    chart_hash: &str,
    profile_id: &str,
) -> Option<deadsync_score::CachedScore> {
    cached_best_itg_score_for_id(profile_id, chart_hash)
}

#[inline(always)]
pub fn groovestats_score_service_allowed() -> bool {
    config::get().enable_groovestats
}

#[inline(always)]
pub fn gameplay_side_for_player(num_players: usize, player_idx: usize) -> PlayerSide {
    deadsync_profile::side_for_gameplay_player(num_players, player_idx, get_session_player_side())
}

#[inline(always)]
pub fn active_local_profile_id_for_gameplay_player(
    num_players: usize,
    player_idx: usize,
) -> Option<(PlayerSide, String)> {
    let side = gameplay_side_for_player(num_players, player_idx);
    active_local_profile_id_for_side(side).map(|profile_id| (side, profile_id))
}

// --- Favorites ---

/// Toggle a song's favorite status for the given player side.
/// Returns `true` if the song is now a favorite, `false` if removed.
pub fn toggle_favorite(side: PlayerSide, chart_hash: &str) -> bool {
    deadsync_profile::app_runtime::toggle_favorite(side, chart_hash)
}

/// Toggle a pack's favorite status for the given player side, identifying the
/// pack by its display name. Returns `true` if the pack is
/// now a favorite, `false` if it was removed.
pub fn toggle_pack_favorite(side: PlayerSide, pack_name: &str) -> bool {
    deadsync_profile::app_runtime::toggle_pack_favorite(side, pack_name)
}

pub fn set_active_profile_for_side(side: PlayerSide, profile: ActiveProfile) -> Profile {
    deadsync_profile::app_runtime::set_active_profile_for_side(side, profile, load_for_side)
}

pub fn set_active_profiles(p1: ActiveProfile, p2: ActiveProfile) -> [Profile; PLAYER_SLOTS] {
    let (p1_default, p2_default) = config::default_profiles();
    deadsync_profile::app_runtime::set_active_profiles(
        [p1, p2],
        [p1_default, p2_default],
        config::update_default_profiles,
        load_for_side,
    )
}

pub fn load_default_profiles_for_joined_sides() -> [Profile; PLAYER_SLOTS] {
    let (p1, p2) = config::default_profiles();
    deadsync_profile::app_runtime::load_default_profiles_for_joined_sides([p1, p2], load_for_side)
}

pub use deadsync_profile::app_runtime::scan_local_profiles;

pub fn create_local_profile(display_name: &str) -> Result<String, std::io::Error> {
    let (p1_default, p2_default) = config::default_profiles();
    deadsync_profile::app_runtime::create_local_profile(
        display_name,
        machine_default_noteskin_value(),
        machine_default_light_brightness(),
        [p1_default, p2_default],
        config::update_default_profiles,
    )
}

/// Player options seeded from machine defaults for a brand-new local profile,
/// returned as `(singles, doubles)`. Used as the translation base when importing
/// Simply Love settings so unspecified options match a freshly created profile.
pub fn default_local_profile_options() -> (PlayerOptionsData, PlayerOptionsData) {
    deadsync_profile::app_runtime::default_local_profile_options(
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
    deadsync_profile::app_runtime::create_local_profile_from_import(data)
}

pub fn rename_local_profile(id: &str, display_name: &str) -> Result<(), std::io::Error> {
    deadsync_profile::app_runtime::rename_local_profile(id, display_name)
}

pub fn delete_local_profile(id: &str) -> Result<(), std::io::Error> {
    let (p1_default, p2_default) = config::default_profiles();
    deadsync_profile::app_runtime::delete_local_profile(
        id,
        [p1_default, p2_default],
        config::update_default_profiles,
        load_for_side,
    )
}
