use super::{
    GameplayCoreState, get_cached_player_leaderboard_itl_self_rank_for_side,
    get_or_fetch_player_leaderboards_for_side, groovestats_eval_state_from_gameplay,
};

use crate::game::profile;
use chrono::Local;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_online::groovestats::{
    GrooveStatsSubmitApiPlayer, GrooveStatsSubmitPlayerJob, judgment_counts_from_stats,
};
use deadsync_online::runtime as online;
use deadsync_profile as profile_data;
use deadsync_simfile::runtime_cache::{get_song_cache, song_cache_generation};
use log::debug;
use std::collections::HashMap;
use std::sync::Arc;

use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_score::{
    ItlEvalState, ItlEventProgress, ItlFileData, ItlGameplayEvalInput, ItlGameplaySavePlayer,
    ItlScoreCalcInput, cached_itl_chart_no_cmod_for_song, gameplay_run_passed,
    itl_current_score_hundredths_for_submit, itl_eval_state_from_gameplay_context,
    itl_ex_score_percent, itl_judgments_from_groovestats_counts, itl_should_warn_cmod_context,
    itl_song_dir, save_itl_gameplay_players,
};
pub use deadsync_score::{is_itl_unlocks_pack, itl_points_for_chart};

const ITL_WHEEL_FETCH_ENTRIES: usize = 5;

pub(super) fn set_cached_online_self_score(
    profile_id: Option<&str>,
    api_key: &str,
    chart_hash: &str,
    score: Option<u32>,
) {
    profile::set_cached_online_itl_self_score(profile_id, api_key, chart_hash, score);
}

pub(super) fn set_cached_online_self_rank(
    profile_id: Option<&str>,
    api_key: &str,
    chart_hash: &str,
    rank: Option<u32>,
) {
    profile::set_cached_online_itl_self_rank(profile_id, api_key, chart_hash, rank);
}

pub fn get_cached_itl_tournament_rank_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    get_cached_player_leaderboard_itl_self_rank_for_side(chart_hash, side)
        .or_else(|| profile::cached_online_itl_self_rank_for_side(chart_hash, side))
}

pub fn get_cached_itl_tournament_overall_ranks_for_side(
    side: profile_data::PlayerSide,
) -> Arc<HashMap<String, u32>> {
    let song_cache = get_song_cache();
    profile::cached_itl_tournament_overall_ranks_for_side(
        side,
        song_cache_generation(),
        song_cache.as_slice(),
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

    let date = Local::now().format("%Y-%m-%d").to_string();
    let song = gs.song();
    let song_dir = itl_song_dir(song);
    let group_name = itl_group_name(song);
    let subtitle = song.display_subtitle(false).to_string();
    let players = gs
        .charts()
        .iter()
        .enumerate()
        .take(gs.num_players().min(MAX_PLAYERS))
        .filter_map(|(player_idx, chart)| {
            let (_side, profile_id) =
                profile::active_local_profile_id_for_gameplay_player(gs.num_players(), player_idx)?;
            let gs_valid = groovestats_eval_state_from_gameplay(gs, player_idx);
            let disabled_windows = gs.profiles()[player_idx].timing_windows.disabled_windows();
            let passed = gameplay_run_passed(
                gs.song_completed_naturally(),
                gs.players()[player_idx].is_failing,
                gs.players()[player_idx].life,
                gs.players()[player_idx].fail_time.is_some(),
            );
            Some(ItlGameplaySavePlayer {
                player_idx,
                profile_id,
                song_dir: song_dir.clone(),
                event_name: group_name.clone(),
                chart_hash: chart.short_hash.clone(),
                chart_name: chart.chart_name.clone(),
                chart_type: chart.chart_type.clone(),
                subtitle: subtitle.clone(),
                used_cmod: matches!(
                    gs.profiles()[player_idx].scroll_speed,
                    ScrollSpeedSetting::CMod(_)
                ),
                groovestats_valid: gs_valid.valid,
                groovestats_reason_lines: gs_valid.reason_lines,
                music_rate: gs.music_rate(),
                remove_mask: gs.profiles()[player_idx].remove_active_mask.bits(),
                disabled_windows,
                passed,
                judgments: itl_judgments_from_gameplay(gs, player_idx),
                ex_percent: itl_ex_score_percent(score_calc_input(gs, player_idx)),
                date: date.clone(),
            })
        });

    for result in save_itl_gameplay_players(
        players,
        profile::read_itl_file_for_id,
        profile::write_itl_file_for_id,
        profile::set_cached_itl_file_for_id,
        |skip| {
            let side = profile::gameplay_side_for_player(gs.num_players(), skip.player_idx);
            debug!(
                "Skipping ITL save for {:?} ({}): {}",
                side,
                skip.chart_hash,
                skip.reason_lines.join("; ")
            );
        },
    ) {
        progress[result.player_idx] = Some(result.progress);
    }

    progress
}

fn score_calc_input(gs: &GameplayCoreState, player_idx: usize) -> ItlScoreCalcInput<'_> {
    let (start, end) = gs.note_range_for_player(player_idx);
    let totals = gs.display_totals_for_player(player_idx);
    ItlScoreCalcInput {
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
    }
}

pub(super) fn current_score_hundredths_for_submit(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> Option<u32> {
    let disabled_windows = gs.profiles()[player_idx].timing_windows.disabled_windows();
    itl_current_score_hundredths_for_submit(
        score_calc_input(gs, player_idx),
        disabled_windows.as_slice(),
    )
}

/// Imports an ITGmania/Simply Love `ITL2026.json` (raw text) into a
/// freshly-created DeadSync profile, writing it to the profile's ITL file.
/// Returns the number of `hashMap` entries imported (`0` when the file is
/// missing, empty, or unparseable). Song ranks are recomputed lazily the next
/// time the profile's ITL cache is loaded.
pub fn import_itl_json(profile_id: &str, json_text: &str) -> usize {
    profile::import_itl_json(profile_id, json_text)
}

fn update_unlock_folders(profile_id: &str, folders: &[String]) {
    profile::update_itl_unlock_folders(profile_id, folders);
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

    let side = profile::gameplay_side_for_player(gs.num_players(), player_idx);
    if let Some(profile_id) = profile::active_local_profile_id_for_side(side)
        && let Some(no_cmod) =
            loaded_chart_no_cmod_for_gameplay(gs, player_idx, profile_id.as_str())
    {
        return no_cmod;
    }

    let song = gs.song();
    itl_should_warn_cmod_context(
        None,
        itl_group_name(song).as_deref(),
        song.display_subtitle(false),
    )
}

fn itl_judgments_from_gameplay(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> deadsync_score::ItlJudgments {
    let player = &gs.players()[player_idx];
    let totals = gs.display_totals_for_player(player_idx);
    let counts = judgment_counts_from_stats(
        gs.live_window_counts(player_idx),
        gs.profiles()[player_idx].timing_windows.disabled_windows(),
        totals.total_steps,
        player.holds_held,
        totals.holds_total,
        player.mines_hit,
        totals.mines_total,
        player.rolls_held,
        totals.rolls_total,
    );
    itl_judgments_from_groovestats_counts(
        counts.fantastic_plus,
        counts.fantastic,
        counts.excellent,
        counts.great,
        counts.decent,
        counts.way_off,
        counts.miss,
        counts.total_steps,
        counts.holds_held,
        counts.total_holds,
        counts.mines_hit,
        counts.total_mines,
        counts.rolls_held,
        counts.total_rolls,
    )
}

fn itl_eval_state(gs: &GameplayCoreState, player_idx: usize, data: &ItlFileData) -> ItlEvalState {
    let used_cmod = matches!(
        gs.profiles()[player_idx].scroll_speed,
        ScrollSpeedSetting::CMod(_)
    );
    let song = gs.song();
    let gs_valid = groovestats_eval_state_from_gameplay(gs, player_idx);
    let disabled_windows = gs.profiles()[player_idx].timing_windows.disabled_windows();
    let passed = gameplay_run_passed(
        gs.song_completed_naturally(),
        gs.players()[player_idx].is_failing,
        gs.players()[player_idx].life,
        gs.players()[player_idx].fail_time.is_some(),
    );

    itl_eval_state_from_gameplay_context(ItlGameplayEvalInput {
        song_dir: itl_song_dir(song).as_deref(),
        group_name: itl_group_name(song).as_deref(),
        data,
        chart_hash: gs.charts()[player_idx].short_hash.as_str(),
        subtitle: song.display_subtitle(false),
        used_cmod,
        groovestats_valid: gs_valid.valid,
        groovestats_reason_lines: gs_valid.reason_lines.as_slice(),
        music_rate: gs.music_rate(),
        remove_mask: gs.profiles()[player_idx].remove_active_mask.bits(),
        disabled_windows: disabled_windows.as_slice(),
        passed,
    })
}

pub fn itl_eval_state_from_gameplay(gs: &GameplayCoreState, player_idx: usize) -> ItlEvalState {
    if player_idx >= gs.num_players().min(MAX_PLAYERS) {
        return ItlEvalState::default();
    }
    let side = profile::gameplay_side_for_player(gs.num_players(), player_idx);
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return ItlEvalState::default();
    };
    let data = profile::read_itl_file_for_id(profile_id.as_str());
    itl_eval_state(gs, player_idx, &data)
}

pub fn get_or_fetch_itl_self_score_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    if let Some(score) = profile::cached_online_itl_self_score_for_side(chart_hash, side) {
        return Some(score);
    }
    // Keep the wheel's ITL prefetch aligned with the Select Music scorebox cache width.
    // Smaller requests seed the shared leaderboard cache with partial panes, so the
    // scorebox briefly renders a truncated list before refetching the remaining rows.
    let _ = get_or_fetch_player_leaderboards_for_side(chart_hash, side, ITL_WHEEL_FETCH_ENTRIES)?;
    profile::cached_online_itl_self_score_for_side(chart_hash, side)
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
