use super::{
    GameplayCoreState, gameplay_run_passed, gameplay_side_for_player,
    get_cached_player_leaderboard_itl_self_rank_for_side,
    get_or_fetch_player_leaderboards_for_side, groovestats_eval_state_from_gameplay,
    groovestats_judgment_counts,
};

use crate::game::online;
use crate::game::profile;
use chrono::Local;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_online::groovestats::{GrooveStatsSubmitApiPlayer, GrooveStatsSubmitPlayerJob};
use deadsync_profile as profile_data;
use deadsync_simfile::runtime_cache::{get_song_cache, song_cache_generation};
use log::{debug, warn};
use std::collections::HashMap;
use std::sync::Arc;

use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_score::{
    CachedItlScore, ItlChartSaveInput, ItlEvalInput, ItlEvalState, ItlEventProgress, ItlFileData,
    ItlFileReadError, ItlFileWriteError, ItlJudgmentCountsInput, ItlScoreCalcInput,
    OnlineItlSelfCacheMap, OnlineItlSelfIndexKind, OnlineItlSelfIndexMap,
    cached_itl_chart_no_cmod_for_song, itl_chart_no_cmod, itl_current_score_hundredths,
    itl_eval_state_from_parts, itl_ex_score_percent, itl_group_name_matches,
    itl_judgments_from_counts, itl_rebuild_song_ranks, itl_song_dir, itl_song_matches_context,
    itl_timing_windows_all_enabled, load_online_itl_self_index_for_profile_dir,
    mark_itl_unlock_folders, read_itl_file_or_default_for_profile_dir,
    runtime_cached_itl_chart_score, runtime_cached_itl_song_folder_unlocked,
    runtime_cached_itl_song_score, runtime_cached_itl_song_score_assume_loaded,
    runtime_cached_online_itl_self_rank, runtime_cached_online_itl_self_rank_assume_loaded,
    runtime_cached_online_itl_self_score, runtime_cached_online_itl_self_score_assume_loaded,
    runtime_ensure_itl_score_profile_loaded, runtime_ensure_itl_wheel_caches_loaded,
    runtime_import_itl_json, runtime_online_itl_overall_ranks_for_side, runtime_set_itl_score_file,
    runtime_set_online_itl_self_rank, runtime_set_online_itl_self_score,
    runtime_update_itl_unlock_folders, save_itl_chart_result,
    save_online_itl_self_index_for_profile_dir, write_itl_file_for_profile_dir,
};
pub use deadsync_score::{is_itl_unlocks_pack, itl_points_for_chart};

const ITL_WHEEL_FETCH_ENTRIES: usize = 5;

#[inline(always)]
fn player_side_cache_idx(side: profile_data::PlayerSide) -> usize {
    match side {
        profile_data::PlayerSide::P2 => 1,
        _ => 0,
    }
}

fn load_online_itl_self_index_for_profile(
    profile_id: &str,
    kind: OnlineItlSelfIndexKind,
) -> OnlineItlSelfIndexMap {
    load_online_itl_self_index_for_profile_dir(&profile::local_profile_dir_for_id(profile_id), kind)
}

fn save_online_itl_self_index_for_profile(
    profile_id: &str,
    kind: OnlineItlSelfIndexKind,
    by_key: &OnlineItlSelfCacheMap,
) {
    let profile_dir = profile::local_profile_dir_for_id(profile_id);
    if let Err(error) = save_online_itl_self_index_for_profile_dir(&profile_dir, kind, by_key) {
        let path = deadsync_score::online_itl_self_index_path(&profile_dir, kind);
        warn!("Failed to save ITL self-score cache {path:?}: {error:?}");
    }
}

fn load_online_itl_self_score_index_for_profile(profile_id: &str) -> OnlineItlSelfIndexMap {
    load_online_itl_self_index_for_profile(profile_id, OnlineItlSelfIndexKind::Score)
}

fn save_online_itl_self_score_index_for_profile(profile_id: &str, by_key: &OnlineItlSelfCacheMap) {
    save_online_itl_self_index_for_profile(profile_id, OnlineItlSelfIndexKind::Score, by_key);
}

fn load_online_itl_self_rank_index_for_profile(profile_id: &str) -> OnlineItlSelfIndexMap {
    load_online_itl_self_index_for_profile(profile_id, OnlineItlSelfIndexKind::Rank)
}

fn save_online_itl_self_rank_index_for_profile(profile_id: &str, by_key: &OnlineItlSelfCacheMap) {
    save_online_itl_self_index_for_profile(profile_id, OnlineItlSelfIndexKind::Rank, by_key);
}

pub(super) fn set_cached_online_self_score(
    profile_id: Option<&str>,
    api_key: &str,
    chart_hash: &str,
    score: Option<u32>,
) {
    let api_key = api_key.trim();
    let chart_hash = chart_hash.trim();
    if api_key.is_empty() || chart_hash.is_empty() {
        return;
    }
    runtime_set_online_itl_self_score(
        profile_id,
        api_key,
        chart_hash,
        score,
        load_online_itl_self_score_index_for_profile,
        save_online_itl_self_score_index_for_profile,
    );
}

pub(super) fn set_cached_online_self_rank(
    profile_id: Option<&str>,
    api_key: &str,
    chart_hash: &str,
    rank: Option<u32>,
) {
    let api_key = api_key.trim();
    let chart_hash = chart_hash.trim();
    if api_key.is_empty() || chart_hash.is_empty() {
        return;
    }
    runtime_set_online_itl_self_rank(
        profile_id,
        api_key,
        chart_hash,
        rank,
        load_online_itl_self_rank_index_for_profile,
        save_online_itl_self_rank_index_for_profile,
    );
}

/// Test/bench helper: seed the *session* online ITL self-score cache directly,
/// keyed by `(chart_hash, api_key)`, without any network fetch or profile file
/// on disk. Lets benchmarks exercise the ITL wheel-score render path. The
/// matching side must be joined and carry a non-empty GrooveStats API key for
/// the wheel lookups to resolve this entry.
pub fn seed_session_online_itl_self_score(api_key: &str, chart_hash: &str, ex_hundredths: u32) {
    set_cached_online_self_score(None, api_key, chart_hash, Some(ex_hundredths));
}

/// Test/bench helper: seed the *session* online ITL self-rank cache directly.
/// See [`seed_session_online_itl_self_score`] for the resolution requirements.
pub fn seed_session_online_itl_self_rank(api_key: &str, chart_hash: &str, rank: u32) {
    set_cached_online_self_rank(None, api_key, chart_hash, Some(rank));
}

/// Test/bench helper: mark song folders as ITL-unlocked for a profile in the
/// in-memory cache without touching disk. Folders not seeded stay locked
/// (matching SL semantics), letting benchmarks exercise the lock-icon path.
pub fn seed_session_itl_unlock_folders(profile_id: &str, folders: &[&str]) {
    ensure_itl_score_cache_loaded(profile_id);
    mark_itl_unlock_folders(profile_id, folders.iter().copied());
}

pub fn get_cached_itl_score_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<CachedItlScore> {
    let profile_id = profile::active_local_profile_id_for_side(side);
    runtime_cached_itl_chart_score(profile_id.as_deref(), chart_hash, read_itl_file)
}

pub fn get_cached_itl_score_for_song(
    song: &deadsync_chart::SongData,
    side: profile_data::PlayerSide,
) -> Option<CachedItlScore> {
    let profile_id = profile::active_local_profile_id_for_side(side);
    get_cached_itl_score_for_song_with_profile(song, profile_id.as_deref())
}

/// Like [`get_cached_itl_score_for_song`] but takes a precomputed profile id so
/// callers iterating many songs in one frame (the song wheel) resolve the
/// active profile once instead of per lookup.
pub fn get_cached_itl_score_for_song_with_profile(
    song: &deadsync_chart::SongData,
    profile_id: Option<&str>,
) -> Option<CachedItlScore> {
    runtime_cached_itl_song_score(song, profile_id, read_itl_file)
}

/// Like [`get_cached_itl_score_for_song_with_profile`] but assumes the profile's
/// ITL score cache was already loaded this frame (see
/// [`ensure_itl_wheel_caches_loaded`]), skipping the per-call ensure-probe lock.
pub fn get_cached_itl_score_for_song_assume_loaded(
    song: &deadsync_chart::SongData,
    profile_id: Option<&str>,
) -> Option<CachedItlScore> {
    runtime_cached_itl_song_score_assume_loaded(song, profile_id)
}

/// Load every per-profile ITL cache the song-wheel overlay reads
/// (`ITL_SCORE_CACHE`, `ONLINE_ITL_SELF_SCORE_CACHE`, `ONLINE_ITL_SELF_RANK_CACHE`)
/// once for `profile_id`. Call this once per joined side per frame *before* the
/// per-slot loop so the `*_assume_loaded` accessors can skip their redundant
/// ensure-probe locks.
pub fn ensure_itl_wheel_caches_loaded(profile_id: &str) {
    runtime_ensure_itl_wheel_caches_loaded(
        profile_id,
        read_itl_file,
        load_online_itl_self_score_index_for_profile,
        load_online_itl_self_rank_index_for_profile,
    );
}

/// Returns true if the song folder is unlocked for this player's ITL profile.
/// Songs not present in the unlock map are treated as locked, matching SL.
pub fn is_itl_song_folder_unlocked_for_side(
    song_folder: &str,
    side: profile_data::PlayerSide,
) -> bool {
    let profile_id = profile::active_local_profile_id_for_side(side);
    runtime_cached_itl_song_folder_unlocked(song_folder, profile_id.as_deref(), read_itl_file)
}

pub fn is_itl_song_folder_unlocked_with_profile(
    song_folder: &str,
    profile_id: Option<&str>,
) -> bool {
    runtime_cached_itl_song_folder_unlocked(song_folder, profile_id, read_itl_file)
}

pub fn get_cached_itl_tournament_rank_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    get_cached_player_leaderboard_itl_self_rank_for_side(chart_hash, side)
        .or_else(|| get_cached_online_self_rank_for_side(chart_hash, side))
}

fn get_cached_online_self_rank_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    if !profile::is_session_side_joined(side) {
        return None;
    }
    let api_key = profile::groovestats_api_key_for_side(side);
    let profile_id = profile::active_local_profile_id_for_side(side);
    get_cached_online_itl_self_rank_for_key(chart_hash, profile_id.as_deref(), &api_key)
}

/// Cached online ITL self-rank lookup that takes a precomputed profile id and
/// API key instead of re-reading global profile state. Lets the song wheel
/// resolve those frame-invariant values once and reuse them across every slot.
pub fn get_cached_online_itl_self_rank_for_key(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    runtime_cached_online_itl_self_rank(
        chart_hash,
        profile_id,
        api_key,
        load_online_itl_self_rank_index_for_profile,
    )
}

/// Like [`get_cached_online_itl_self_rank_for_key`] but assumes the profile's
/// rank cache was already loaded this frame (see [`ensure_itl_wheel_caches_loaded`]),
/// skipping the per-call ensure-probe lock.
pub fn get_cached_online_itl_self_rank_for_key_assume_loaded(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    runtime_cached_online_itl_self_rank_assume_loaded(chart_hash, profile_id, api_key)
}

pub fn get_cached_itl_tournament_overall_ranks_for_side(
    side: profile_data::PlayerSide,
) -> Arc<HashMap<String, u32>> {
    let side_profile = profile::get_for_side(side);
    let profile_id = profile::active_local_profile_id_for_side(side);
    let song_cache = get_song_cache();
    runtime_online_itl_overall_ranks_for_side(
        player_side_cache_idx(side),
        profile::is_session_side_joined(side),
        side_profile.groovestats_api_key.as_str(),
        profile_id.as_deref(),
        song_cache_generation(),
        song_cache.as_slice(),
        load_online_itl_self_score_index_for_profile,
    )
}

pub fn save_itl_data_from_gameplay(
    gs: &GameplayCoreState,
) -> [Option<ItlEventProgress>; MAX_PLAYERS] {
    let mut progress: [Option<ItlEventProgress>; MAX_PLAYERS] = std::array::from_fn(|_| None);
    if gs.autoplay_used() {
        debug!("Skipping ITL save: autoplay or replay was used during this stage.");
        return progress;
    }

    for (player_idx, chart) in gs
        .charts()
        .iter()
        .enumerate()
        .take(gs.num_players().min(MAX_PLAYERS))
    {
        let side = gameplay_side_for_player(gs, player_idx);
        let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
            continue;
        };
        let chart_hash = chart.short_hash.trim();
        if chart_hash.is_empty() {
            continue;
        }

        let mut data = read_itl_file(profile_id.as_str());
        itl_rebuild_song_ranks(&mut data);
        let eval = itl_eval_state(gs, player_idx, &data);
        if !eval.active {
            continue;
        }
        if !eval.eligible {
            debug!(
                "Skipping ITL save for {:?} ({}): {}",
                side,
                chart_hash,
                eval.reason_lines.join("; ")
            );
            continue;
        }
        let song = gs.song();
        let Some(song_dir) = itl_song_dir(song) else {
            continue;
        };
        let judgments = itl_judgments_from_gameplay(gs, player_idx);
        let (start, end) = gs.note_range_for_player(player_idx);
        let totals = gs.display_totals_for_player(player_idx);
        let ex_percent = itl_ex_score_percent(ItlScoreCalcInput {
            notes: &gs.notes()[start..end],
            note_times: &gs.note_time_cache_ns()[start..end],
            hold_end_times: &gs.hold_end_time_cache_ns()[start..end],
            total_steps: totals.total_steps,
            holds_total: totals.holds_total,
            rolls_total: totals.rolls_total,
            mines_total: totals.mines_total,
            fail_time: gs.players()[player_idx]
                .fail_time
                .map(deadsync_core::song_time::song_time_ns_from_seconds),
        });
        let save_result = save_itl_chart_result(
            &mut data,
            ItlChartSaveInput {
                song_dir: song_dir.as_str(),
                chart_hash,
                chart_name: gs.charts()[player_idx].chart_name.as_str(),
                chart_type: gs.charts()[player_idx].chart_type.as_str(),
                event_name: itl_group_name(song).as_deref().unwrap_or_default(),
                judgments,
                ex_percent,
                used_cmod: eval.used_cmod,
                chart_no_cmod: eval.chart_no_cmod,
                date: Local::now().format("%Y-%m-%d").to_string(),
            },
        );
        progress[player_idx] = Some(save_result.progress);

        if save_result.needs_write {
            write_itl_file(profile_id.as_str(), &data);
            set_cached_itl_file(profile_id.as_str(), data);
        }
    }

    progress
}

pub(super) fn current_score_hundredths(gs: &GameplayCoreState, player_idx: usize) -> u32 {
    let (start, end) = gs.note_range_for_player(player_idx);
    let totals = gs.display_totals_for_player(player_idx);
    itl_current_score_hundredths(ItlScoreCalcInput {
        notes: &gs.notes()[start..end],
        note_times: &gs.note_time_cache_ns()[start..end],
        hold_end_times: &gs.hold_end_time_cache_ns()[start..end],
        total_steps: totals.total_steps,
        holds_total: totals.holds_total,
        rolls_total: totals.rolls_total,
        mines_total: totals.mines_total,
        fail_time: gs.players()[player_idx]
            .fail_time
            .map(deadsync_core::song_time::song_time_ns_from_seconds),
    })
}

pub(super) fn current_score_hundredths_for_submit(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> Option<u32> {
    itl_timing_windows_all_enabled(
        gs.profiles()[player_idx]
            .timing_windows
            .disabled_windows()
            .as_slice(),
    )
    .then(|| current_score_hundredths(gs, player_idx))
}

fn ensure_itl_score_cache_loaded(profile_id: &str) {
    runtime_ensure_itl_score_profile_loaded(profile_id, read_itl_file);
}

fn set_cached_itl_file(profile_id: &str, data: ItlFileData) {
    runtime_set_itl_score_file(profile_id, data);
}

fn read_itl_file(profile_id: &str) -> ItlFileData {
    let profile_dir = profile::local_profile_dir_for_id(profile_id);
    match read_itl_file_or_default_for_profile_dir(&profile_dir) {
        Ok(data) => data,
        Err(ItlFileReadError::Parse { path, error }) => {
            warn!("Failed to parse ITL data file {path:?}: {error}");
            ItlFileData::default()
        }
        Err(ItlFileReadError::Read { .. }) => ItlFileData::default(),
    }
}

fn write_itl_file(profile_id: &str, data: &ItlFileData) {
    let profile_dir = profile::local_profile_dir_for_id(profile_id);
    if let Err(error) = write_itl_file_for_profile_dir(&profile_dir, data) {
        match error {
            ItlFileWriteError::CreateDir { dir, error } => {
                warn!("Failed to create ITL profile dir {dir:?}: {error}");
            }
            ItlFileWriteError::Encode => {
                warn!("Failed to encode ITL data for profile {profile_id}");
            }
            ItlFileWriteError::WriteTemp { path, error } => {
                warn!("Failed to write ITL temp file {path:?}: {error}");
            }
            ItlFileWriteError::Commit { path, error } => {
                warn!("Failed to commit ITL file {path:?}: {error}");
            }
        }
    }
}

/// Imports an ITGmania/Simply Love `ITL2026.json` (raw text) into a
/// freshly-created DeadSync profile, writing it to the profile's ITL file.
/// Returns the number of `hashMap` entries imported (`0` when the file is
/// missing, empty, or unparseable). Song ranks are recomputed lazily the next
/// time the profile's ITL cache is loaded.
pub fn import_itl_json(profile_id: &str, json_text: &str) -> usize {
    runtime_import_itl_json(profile_id, json_text, write_itl_file)
}

fn update_unlock_folders(profile_id: &str, folders: &[String]) {
    runtime_update_itl_unlock_folders(
        profile_id,
        folders.iter().map(String::as_str),
        read_itl_file,
        write_itl_file,
    );
}

pub(super) fn handle_submit_player_unlocks(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
) {
    let cfg = crate::config::get();
    let plan = deadsync_online::groovestats::submit_unlock_plan_from_response(
        player,
        response,
        cfg.auto_download_unlocks,
        cfg.separate_unlocks_by_player,
    );
    if let Some(profile_id) = player.profile_id.as_deref() {
        for folders in &plan.itl_folder_groups {
            update_unlock_folders(profile_id, folders.as_slice());
        }
    }
    for download in &plan.downloads {
        online::queue_event_unlock_download(
            download.url.as_str(),
            download.download_name.as_str(),
            download.pack_name.as_str(),
        );
    }
}

fn itl_group_name(song: &deadsync_chart::SongData) -> Option<String> {
    let song_cache = get_song_cache();
    for pack in song_cache.iter() {
        if pack
            .songs
            .iter()
            .any(|candidate| candidate.simfile_path == song.simfile_path)
        {
            return Some(pack.group_name.clone());
        }
    }
    None
}

fn loaded_chart_no_cmod_for_gameplay(
    gs: &GameplayCoreState,
    player_idx: usize,
    profile_id: &str,
) -> Option<bool> {
    let song = gs.song();
    let song_dir = itl_song_dir(song)?;
    let group_name = itl_group_name(song);
    cached_itl_chart_no_cmod_for_song(
        profile_id,
        Some(song_dir.as_str()),
        group_name.as_deref(),
        gs.charts()[player_idx].short_hash.as_str(),
        song.display_subtitle(false),
    )
}

pub fn should_warn_cmod_for_itl_chart(gs: &GameplayCoreState, player_idx: usize) -> bool {
    if player_idx >= gs.num_players().min(MAX_PLAYERS)
        || gs.course_display_is_course_stage()
        || !matches!(
            gs.profiles()[player_idx].scroll_speed,
            ScrollSpeedSetting::CMod(_)
        )
    {
        return false;
    }

    let side = gameplay_side_for_player(gs, player_idx);
    if let Some(profile_id) = profile::active_local_profile_id_for_side(side)
        && let Some(no_cmod) =
            loaded_chart_no_cmod_for_gameplay(gs, player_idx, profile_id.as_str())
    {
        return no_cmod;
    }

    let song = gs.song();
    let Some(group_name) = itl_group_name(song) else {
        return false;
    };
    itl_group_name_matches(group_name.as_str())
        && itl_chart_no_cmod(song.display_subtitle(false), None)
}

fn itl_judgments_from_gameplay(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> deadsync_score::ItlJudgments {
    let counts = groovestats_judgment_counts(gs, player_idx);
    itl_judgments_from_counts(ItlJudgmentCountsInput {
        fantastic_plus: counts.fantastic_plus,
        fantastic: counts.fantastic,
        excellent: counts.excellent,
        great: counts.great,
        decent: counts.decent_count(),
        way_off: counts.way_off_count(),
        miss: counts.miss,
        total_steps: counts.total_steps,
        holds_held: counts.holds_held,
        total_holds: counts.total_holds,
        mines_hit: counts.mines_hit,
        total_mines: counts.total_mines,
        rolls_held: counts.rolls_held,
        total_rolls: counts.total_rolls,
    })
}

fn itl_eval_state(gs: &GameplayCoreState, player_idx: usize, data: &ItlFileData) -> ItlEvalState {
    let used_cmod = matches!(
        gs.profiles()[player_idx].scroll_speed,
        ScrollSpeedSetting::CMod(_)
    );
    let song = gs.song();
    let Some(song_dir) = itl_song_dir(song) else {
        return ItlEvalState {
            active: false,
            eligible: false,
            chart_no_cmod: false,
            used_cmod,
            reason_lines: Vec::new(),
        };
    };
    let group_name = itl_group_name(song);
    if !itl_song_matches_context(Some(song_dir.as_str()), group_name.as_deref(), data) {
        return ItlEvalState {
            active: false,
            eligible: false,
            chart_no_cmod: false,
            used_cmod,
            reason_lines: Vec::new(),
        };
    }

    let chart_hash = gs.charts()[player_idx].short_hash.as_str();
    let prev = data.hash_map.get(chart_hash);
    let chart_no_cmod = itl_chart_no_cmod(song.display_subtitle(false), prev);
    let gs_valid = groovestats_eval_state_from_gameplay(gs, player_idx);
    let remove_mask = gs.profiles()[player_idx].remove_active_mask.bits();
    let mines_enabled = (remove_mask & (1u8 << 1)) == 0;
    let disabled_windows = gs.profiles()[player_idx].timing_windows.disabled_windows();
    let all_timing_windows_enabled = itl_timing_windows_all_enabled(disabled_windows.as_slice());
    let passed = gameplay_run_passed(
        gs.song_completed_naturally(),
        gs.players()[player_idx].is_failing,
        gs.players()[player_idx].life,
        gs.players()[player_idx].fail_time.is_some(),
    );

    itl_eval_state_from_parts(ItlEvalInput {
        chart_no_cmod,
        used_cmod,
        groovestats_valid: gs_valid.valid,
        groovestats_reason_lines: gs_valid.reason_lines.as_slice(),
        music_rate: gs.music_rate(),
        mines_enabled,
        all_timing_windows_enabled,
        passed,
    })
}

pub fn itl_eval_state_from_gameplay(gs: &GameplayCoreState, player_idx: usize) -> ItlEvalState {
    if player_idx >= gs.num_players().min(MAX_PLAYERS) {
        return ItlEvalState::default();
    }
    let side = gameplay_side_for_player(gs, player_idx);
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return ItlEvalState::default();
    };
    let data = read_itl_file(profile_id.as_str());
    itl_eval_state(gs, player_idx, &data)
}

pub fn get_cached_itl_self_score_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    if !profile::is_session_side_joined(side) {
        return None;
    }
    let api_key = profile::groovestats_api_key_for_side(side);
    let profile_id = profile::active_local_profile_id_for_side(side);
    get_cached_itl_self_score_for_key(chart_hash, profile_id.as_deref(), &api_key)
}

/// Cached online ITL self-score lookup that takes a precomputed profile id and
/// API key instead of re-reading global profile state. Lets the song wheel
/// resolve those frame-invariant values once and reuse them across every slot.
pub fn get_cached_itl_self_score_for_key(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    runtime_cached_online_itl_self_score(
        chart_hash,
        profile_id,
        api_key,
        load_online_itl_self_score_index_for_profile,
    )
}

/// Like [`get_cached_itl_self_score_for_key`] but assumes the profile's online
/// self-score cache was already loaded this frame (see
/// [`ensure_itl_wheel_caches_loaded`]), skipping the per-call ensure-probe lock.
pub fn get_cached_itl_self_score_for_key_assume_loaded(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    runtime_cached_online_itl_self_score_assume_loaded(chart_hash, profile_id, api_key)
}

pub fn get_or_fetch_itl_self_score_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    if let Some(score) = get_cached_itl_self_score_for_side(chart_hash, side) {
        return Some(score);
    }
    // Keep the wheel's ITL prefetch aligned with the Select Music scorebox cache width.
    // Smaller requests seed the shared leaderboard cache with partial panes, so the
    // scorebox briefly renders a truncated list before refetching the remaining rows.
    let _ = get_or_fetch_player_leaderboards_for_side(chart_hash, side, ITL_WHEEL_FETCH_ENTRIES)?;
    get_cached_itl_self_score_for_side(chart_hash, side)
}

pub fn get_or_fetch_itl_tournament_rank_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    if let Some(rank) = get_cached_itl_tournament_rank_for_side(chart_hash, side) {
        return Some(rank);
    }
    let _ = get_or_fetch_player_leaderboards_for_side(chart_hash, side, ITL_WHEEL_FETCH_ENTRIES)?;
    get_cached_itl_tournament_rank_for_side(chart_hash, side)
}
