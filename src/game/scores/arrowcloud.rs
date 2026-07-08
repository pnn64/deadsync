use super::{
    GameplayCoreState, gameplay_run_failed, gameplay_side_for_player,
    get_or_fetch_player_leaderboards_for_side, invalidate_player_leaderboards_for_side,
    lua_chart_submit_allowed,
};
use crate::game::profile;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{FantasticWindowOptions, blue_fantastic_window_ms};
use deadsync_online::arrowcloud::{
    self as arrowcloud_api, ArrowCloudPayload, ArrowCloudPayloadParts, ArrowCloudSubmitDraft,
    ArrowCloudSubmitJob, ArrowCloudTimingDatum,
    reset_submit_ui_status as arrowcloud_reset_submit_ui_status,
};
use deadsync_online::groovestats::GROOVESTATS_SUBMIT_MAX_ENTRIES;
use deadsync_profile as profile_data;
use deadsync_rules::timing;
use deadsync_score::{
    ArrowCloudAutosubmitPlayerAction, ArrowCloudAutosubmitPlayerInput,
    ArrowCloudAutosubmitSessionDecision, ArrowCloudAutosubmitSessionInput, ArrowCloudSubmitStats,
    ArrowCloudSubmitUiStatus, arrowcloud_autosubmit_after_payload_decision,
    arrowcloud_autosubmit_player_decision, arrowcloud_autosubmit_session_decision,
    arrowcloud_submit_stats_from_results,
};

#[inline(always)]
fn player_blue_window_ms(gs: &GameplayCoreState, player_idx: usize) -> f32 {
    let base = gs.default_fa_plus_window_s();
    let Some(profile) = gs.profiles().get(player_idx) else {
        return base * 1000.0;
    };
    blue_fantastic_window_ms(FantasticWindowOptions {
        base_fa_plus_s: base,
        custom_fantastic_window_s: profile.custom_fantastic_window.then(|| {
            f32::from(profile_data::clamp_custom_fantastic_window_ms(
                profile.custom_fantastic_window_ms,
            )) / 1000.0
        }),
        fa_plus_10ms_blue_window: profile.fa_plus_10ms_blue_window,
    })
}

pub fn get_arrowcloud_submit_ui_status_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<ArrowCloudSubmitUiStatus> {
    arrowcloud_api::submit_ui_status_for_side(chart_hash, side)
}

#[inline(always)]
fn arrowcloud_lifebar_points(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> Vec<arrowcloud_api::ArrowCloudLifePoint> {
    let life_history = gs.players()[player_idx].life_history.as_slice();
    let (start, end) = gs.note_range_for_player(player_idx);
    let note_times = &gs.note_time_cache_ns()[start..end];
    let graph = gs.density_graph_view();
    let first_second = graph.first_second.min(0.0);
    let last_second = graph.last_second.max(first_second);
    let chart_start_second = note_times
        .iter()
        .find(|&&t| !deadsync_core::song_time::song_time_ns_invalid(t))
        .copied()
        .map(deadsync_core::song_time::song_time_ns_to_seconds)
        .unwrap_or(first_second);

    arrowcloud_api::lifebar_points(
        life_history,
        chart_start_second,
        first_second,
        last_second,
        arrowcloud_api::ARROWCLOUD_LIFEBAR_POINTS,
    )
}

#[inline(always)]
fn arrowcloud_timing_data(
    gs: &GameplayCoreState,
    player_idx: usize,
    fail_time_ns: Option<i64>,
) -> Vec<ArrowCloudTimingDatum> {
    let (start, end) = gs.note_range_for_player(player_idx);
    let notes = &gs.notes()[start..end];
    let note_times = &gs.note_time_cache_ns()[start..end];
    let col_offset = player_idx.saturating_mul(gs.cols_per_player());
    let stream_segments = gs.stream_segments_for_results(player_idx);
    let scatter = timing::build_scatter_points(
        notes,
        note_times,
        col_offset,
        gs.cols_per_player(),
        &stream_segments,
    );
    let fail_time_s = fail_time_ns.map(deadsync_core::song_time::song_time_ns_to_seconds);
    arrowcloud_api::timing_data_from_scatter(&scatter, fail_time_s)
}

#[inline(always)]
fn arrowcloud_nps_info(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> arrowcloud_api::ArrowCloudNpsInfo {
    let chart = gs.charts()[player_idx].as_ref();
    let graph = gs.density_graph_view();
    let first_second = graph.first_second.min(0.0);
    let last_second = graph.last_second.max(first_second);
    arrowcloud_api::nps_info_from_measure_data(
        chart.max_nps,
        chart.measure_nps_vec.as_slice(),
        chart.measure_seconds_vec.as_slice(),
        first_second,
        last_second,
    )
}

#[inline(always)]
fn arrowcloud_live_submit_stats(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> ArrowCloudSubmitStats {
    let player = &gs.players()[player_idx];
    ArrowCloudSubmitStats {
        judgment_counts: player.judgment_counts,
        window_counts: gs.live_window_counts(player_idx),
        holds_held: player.holds_held,
        mines_hit: player.mines_hit,
        mines_avoided: player.mines_avoided,
        rolls_held: player.rolls_held,
    }
}

#[inline(always)]
fn arrowcloud_submit_stats(
    gs: &GameplayCoreState,
    player_idx: usize,
    fail_time_ns: Option<i64>,
) -> ArrowCloudSubmitStats {
    let Some(fail_time_ns) = fail_time_ns else {
        return arrowcloud_live_submit_stats(gs, player_idx);
    };
    let (start, end) = gs.note_range_for_player(player_idx);
    arrowcloud_submit_stats_from_results(
        &gs.notes()[start..end],
        &gs.note_time_cache_ns()[start..end],
        &gs.hold_end_time_cache_ns()[start..end],
        Some(fail_time_ns),
    )
}

#[inline(always)]
fn arrowcloud_payload_for_player(
    gs: &GameplayCoreState,
    player_idx: usize,
    pack_group: &str,
) -> Option<ArrowCloudPayload> {
    if player_idx >= gs.num_players() {
        return None;
    }
    let chart = gs.charts()[player_idx].as_ref();
    let profile = &gs.profiles()[player_idx];
    let player = &gs.players()[player_idx];
    let fail_time_ns = player
        .fail_time
        .map(deadsync_core::song_time::song_time_ns_from_seconds);
    let submit_stats = arrowcloud_submit_stats(gs, player_idx, fail_time_ns);
    let song = gs.song();
    let passed = !gameplay_run_failed(player.is_failing, player.fail_time.is_some());
    let totals = gs.display_totals_for_player(player_idx);

    Some(arrowcloud_api::payload_from_parts(ArrowCloudPayloadParts {
        song_name: song.display_full_title(true),
        artist: song.artist.clone(),
        pack: pack_group.to_string(),
        music_length_seconds: song.music_length_seconds,
        hash: chart.short_hash.clone(),
        timing_data: arrowcloud_timing_data(gs, player_idx, fail_time_ns),
        difficulty: chart.meter,
        stepartist: chart.step_artist.clone(),
        submit_stats,
        total_holds: totals.holds_total,
        total_mines: totals.mines_total,
        total_rolls: totals.rolls_total,
        nps_info: arrowcloud_nps_info(gs, player_idx),
        lifebar_info: arrowcloud_lifebar_points(gs, player_idx),
        modifiers: arrowcloud_api::modifiers_from_profile(profile),
        music_rate: gs.music_rate(),
        used_autoplay: gs.autoplay_used(),
        passed,
    }))
}

fn cache_arrowcloud_submit_success(job: &ArrowCloudSubmitJob) {
    if let Some(profile_id) = job.profile_id.as_deref() {
        super::cache_arrowcloud_scores_from_submit(
            profile_id,
            job.payload.hash.as_str(),
            job.itg_percent,
            job.ex_percent,
            job.hard_ex_percent,
            job.is_fail,
            chrono::Utc::now(),
        );
    }
}

fn refresh_arrowcloud_submit_leaderboards(job: &ArrowCloudSubmitJob) {
    invalidate_player_leaderboards_for_side(job.payload.hash.as_str(), job.side);
    get_or_fetch_player_leaderboards_for_side(
        job.payload.hash.as_str(),
        job.side,
        GROOVESTATS_SUBMIT_MAX_ENTRIES,
    );
}

pub fn submit_arrowcloud_payloads_from_gameplay(gs: &GameplayCoreState, pack_group: &str) {
    for player_idx in 0..gs.num_players().min(MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        let chart_hash = gs.charts()[player_idx].short_hash.as_str();
        arrowcloud_reset_submit_ui_status(side, chart_hash);
        arrowcloud_api::reset_submit_retry(side, chart_hash);
    }

    let cfg = crate::config::get();
    match arrowcloud_autosubmit_session_decision(ArrowCloudAutosubmitSessionInput {
        enabled: cfg.enable_arrowcloud,
        player_count: gs.num_players(),
        autoplay_used: gs.autoplay_used(),
        is_course_stage: gs.course_display_is_course_stage(),
        autosubmit_course_scores_individually: cfg.autosubmit_course_scores_individually,
    }) {
        ArrowCloudAutosubmitSessionDecision::Submit => {}
        ArrowCloudAutosubmitSessionDecision::Skip { log } => {
            if let Some(log) = log {
                arrowcloud_api::log_global_submit_skip(log);
            }
            return;
        }
    }
    let mut drafts = Vec::with_capacity(gs.num_players().min(MAX_PLAYERS));
    for player_idx in 0..gs.num_players().min(MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        let chart_hash = gs.charts()[player_idx].short_hash.as_str();
        let api_key = gs.profiles()[player_idx].arrowcloud_api_key.trim();
        let decision = arrowcloud_autosubmit_player_decision(ArrowCloudAutosubmitPlayerInput {
            song_has_lua: gs.song().has_lua,
            lua_submit_allowed: lua_chart_submit_allowed(chart_hash),
            song_completed_naturally: gs.song_completed_naturally(),
            is_failing: gs.players()[player_idx].is_failing,
            life: gs.players()[player_idx].life,
            has_fail_time: gs.players()[player_idx].fail_time.is_some(),
            submit_fails_enabled: cfg.submit_arrowcloud_fails,
            api_key_present: !api_key.is_empty(),
            course_stage_life_submit_eligible: gs.course_stage_life_submit_eligible(player_idx),
        });
        match decision.action {
            ArrowCloudAutosubmitPlayerAction::BuildPayload => {}
            ArrowCloudAutosubmitPlayerAction::Skip { log } => {
                if let Some(log) = log {
                    arrowcloud_api::log_player_submit_skip(side, chart_hash, log);
                }
                continue;
            }
        }
        let Some(payload) = arrowcloud_payload_for_player(gs, player_idx, pack_group) else {
            arrowcloud_api::warn_submit_skip(side, chart_hash, "failed to build submit payload");
            continue;
        };
        if let Some(log) = arrowcloud_autosubmit_after_payload_decision(
            decision.failed,
            decision.allow_failed_submit,
        ) {
            arrowcloud_api::log_player_submit_skip(side, payload.hash.as_str(), log);
            continue;
        }
        let profile_id = profile::active_local_profile_id_for_side(side);
        let itg_percent = gs.display_itg_score_percent(player_idx).clamp(0.0, 100.0);
        let blue_window_ms = player_blue_window_ms(gs, player_idx);
        let ex_percent = gs
            .display_ex_score_percent(player_idx, blue_window_ms)
            .clamp(0.0, 100.0);
        let hard_ex_percent = gs
            .display_hard_ex_score_percent(player_idx, blue_window_ms)
            .clamp(0.0, 100.0);

        let draft = ArrowCloudSubmitDraft::new(
            side,
            api_key.to_string(),
            payload,
            profile_id,
            itg_percent,
            ex_percent,
            hard_ex_percent,
            decision.failed,
        );
        drafts.push(draft);
    }
    let jobs = arrowcloud_api::begin_submit_jobs_from_drafts(drafts);
    if jobs.is_empty() {
        return;
    }

    arrowcloud_api::spawn_submit_jobs(
        jobs,
        cache_arrowcloud_submit_success,
        refresh_arrowcloud_submit_leaderboards,
    );
}

pub fn retry_arrowcloud_submit(chart_hash: &str, side: profile_data::PlayerSide) -> bool {
    arrowcloud_api::retry_submit_if_enabled(
        crate::config::get().enable_arrowcloud,
        chart_hash,
        side,
        true,
        cache_arrowcloud_submit_success,
        refresh_arrowcloud_submit_leaderboards,
    )
}

/// Returns the seconds remaining until the next retry is allowed (manual
/// cooldown) or scheduled (auto). `Some(0)` means due-to-fire / gate just
/// elapsed. `None` means no gate is currently armed (bare `F5 Retry`).
pub fn arrowcloud_next_retry_remaining_secs(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    arrowcloud_api::next_retry_remaining_secs(chart_hash, side)
}

/// Returns true when the next scheduled retry will be fired automatically by
/// the tick driver. When false, any pending `next_retry_at` is acting purely
/// as a manual F5 cooldown gate.
pub fn arrowcloud_next_retry_is_auto(chart_hash: &str, side: profile_data::PlayerSide) -> bool {
    arrowcloud_api::next_retry_is_auto(chart_hash, side)
}

/// Fires any auto-retries whose scheduled time has elapsed. Only fires for
/// entries whose current UI status is auto-retryable (see
/// [`ArrowCloudSubmitUiStatus::is_auto_retryable`]) AND whose auto-retry budget
/// hasn't been exhausted; other retryable statuses (and exhausted entries)
/// use `next_retry_at` purely as a manual cooldown gate. Returns true if at
/// least one retry was fired.
pub fn tick_arrowcloud_auto_retries() -> bool {
    arrowcloud_api::tick_auto_submit_retries_if_enabled(
        crate::config::get().enable_arrowcloud,
        cache_arrowcloud_submit_success,
        refresh_arrowcloud_submit_leaderboards,
    )
}
