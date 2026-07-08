use super::{
    GameplayCoreState, cache_gs_score_for_profile, gameplay_side_for_player,
    get_or_fetch_player_leaderboards_for_side, invalidate_player_leaderboards_for_side, itl,
    lua_chart_submit_allowed,
};
use crate::game::profile;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_online::groovestats::{
    self as groovestats_api, GROOVESTATS_SUBMIT_MAX_ENTRIES, GrooveStatsJudgmentCounts,
    GrooveStatsRescoreCounts, GrooveStatsSubmitPlayerDraft, GrooveStatsSubmitPlayerPayload,
    reset_submit_event_ui as groovestats_reset_submit_event_ui,
    reset_submit_retry as groovestats_reset_submit_retry,
    reset_submit_ui_status as groovestats_reset_submit_ui_status,
};
use deadsync_profile as profile_data;
use deadsync_profile::Profile;
use deadsync_rules::judgment;
use deadsync_score::{
    EventProgress, GrooveStatsAutosubmitPlayerAction, GrooveStatsAutosubmitPlayerInput,
    GrooveStatsAutosubmitSessionDecision, GrooveStatsAutosubmitSessionInput, GrooveStatsEvalInput,
    GrooveStatsEvalState, GrooveStatsGameplayEvalInput, GrooveStatsSubmitRecordBanner,
    GrooveStatsSubmitUiStatus, cached_score_from_imported_player_score,
    groovestats_autosubmit_player_decision, groovestats_autosubmit_session_decision,
    groovestats_eval_state_from_gameplay_parts, groovestats_eval_state_from_parts,
    groovestats_rate_hundredths, groovestats_score_10000_from_counts, groovestats_used_cmod,
    imported_score_chart_stats,
};
use deadsync_simfile::runtime_cache::get_song_cache;

#[inline(always)]
fn active_groovestats_service() -> groovestats_api::Service {
    crate::game::online::active_groovestats_service()
}

#[inline(always)]
fn active_groovestats_service_name() -> &'static str {
    groovestats_api::service_name(active_groovestats_service())
}

pub(super) use groovestats_api::GrooveStatsSubmitPlayerJob;

pub fn get_groovestats_submit_ui_status_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<GrooveStatsSubmitUiStatus> {
    groovestats_api::submit_ui_status_for_side(chart_hash, side)
}

pub fn get_groovestats_submit_event_progress_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Vec<EventProgress> {
    groovestats_api::submit_event_progress_for_side(chart_hash, side)
}

pub fn get_groovestats_submit_record_banner_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<GrooveStatsSubmitRecordBanner> {
    groovestats_api::submit_record_banner_for_side(chart_hash, side)
}

fn groovestats_eval_state(
    chart: &deadsync_chart::ChartData,
    profile: &Profile,
    music_rate: f32,
    autoplay_used: bool,
    is_course_mode: bool,
    course_submit_allowed: bool,
) -> GrooveStatsEvalState {
    let remove_mask = profile.remove_active_mask.bits();
    let insert_mask = profile.insert_active_mask.bits();
    let holds_mask = profile.holds_active_mask.bits();
    let fail_type_ok = matches!(
        crate::config::get().default_fail_type,
        crate::config::DefaultFailType::Immediate
            | crate::config::DefaultFailType::ImmediateContinue
    );

    groovestats_eval_state_from_parts(GrooveStatsEvalInput {
        chart_type: chart.chart_type.as_str(),
        music_rate,
        remove_mask,
        insert_mask,
        holds_mask,
        fail_type_ok,
        autoplay_used,
        is_course_mode,
        course_submit_allowed,
        custom_fantastic_window: profile.custom_fantastic_window,
        custom_fantastic_window_ms: profile.custom_fantastic_window_ms,
    })
}

fn groovestats_manual_qr_url_from_gameplay(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> Option<String> {
    if player_idx >= gs.num_players() {
        return None;
    }
    let payload = groovestats_payload_for_player(gs, player_idx)?;
    groovestats_api::manual_qr_url(
        groovestats_api::qr_base_url(),
        gs.charts()[player_idx].short_hash.as_str(),
        &payload.judgment_counts,
        &payload.rescore_counts,
        payload.rate,
        payload.used_cmod,
    )
}

pub fn groovestats_eval_state_from_gameplay(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> GrooveStatsEvalState {
    if player_idx >= gs.num_players().min(MAX_PLAYERS) {
        return GrooveStatsEvalState::default();
    }
    let base_state = groovestats_eval_state(
        gs.charts()[player_idx].as_ref(),
        &gs.profiles()[player_idx],
        gs.music_rate(),
        gs.autoplay_used(),
        gs.course_display_is_course_stage(),
        crate::config::get().autosubmit_course_scores_individually,
    );
    let mut result = groovestats_eval_state_from_gameplay_parts(
        base_state,
        GrooveStatsGameplayEvalInput {
            song_has_lua: gs.song().has_lua,
            lua_submit_allowed: lua_chart_submit_allowed(
                gs.charts()[player_idx].short_hash.as_str(),
            ),
            song_completed_naturally: gs.song_completed_naturally(),
            is_failing: gs.players()[player_idx].is_failing,
            life: gs.players()[player_idx].life,
            has_fail_time: gs.players()[player_idx].fail_time.is_some(),
            course_stage_life_submit_eligible: gs.course_stage_life_submit_eligible(player_idx),
        },
    );
    if result.should_set_manual_qr_url {
        result.state.manual_qr_url = groovestats_manual_qr_url_from_gameplay(gs, player_idx);
    }
    result.state
}

fn groovestats_submit_invalid_reason(
    chart: &deadsync_chart::ChartData,
    song_has_lua: bool,
    profile: &Profile,
    music_rate: f32,
) -> Option<String> {
    if song_has_lua && !lua_chart_submit_allowed(chart.short_hash.as_str()) {
        return Some("simfile relies on lua".to_string());
    }
    groovestats_eval_state(chart, profile, music_rate, false, false, false)
        .reason_lines
        .into_iter()
        .next()
}

#[inline(always)]
pub(super) fn groovestats_judgment_counts(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> GrooveStatsJudgmentCounts {
    let player = &gs.players()[player_idx];
    let windows = gs.live_window_counts(player_idx);
    let totals = gs.display_totals_for_player(player_idx);
    let disabled_windows = gs.profiles()[player_idx].timing_windows.disabled_windows();
    groovestats_api::judgment_counts_from_stats(
        windows,
        disabled_windows,
        totals.total_steps,
        player.holds_held,
        totals.holds_total,
        player.mines_hit,
        totals.mines_total,
        player.rolls_held,
        totals.rolls_total,
    )
}

fn groovestats_rescore_counts(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> GrooveStatsRescoreCounts {
    let (start, end) = gs.note_range_for_player(player_idx);
    groovestats_api::rescore_counts_from_judgments(
        gs.notes()[start..end]
            .iter()
            .filter_map(|note| Some((note.result.as_ref()?, note.early_result.as_ref()?))),
    )
}

fn groovestats_comment_string(gs: &GameplayCoreState, player_idx: usize) -> String {
    let profile = &gs.profiles()[player_idx];
    let counts = groovestats_judgment_counts(gs, player_idx);
    let fa_plus_ex_score = if profile.show_fa_plus_window {
        let (start, end) = gs.note_range_for_player(player_idx);
        let totals = gs.display_totals_for_player(player_idx);
        Some(judgment::calculate_ex_score_from_notes(
            &gs.notes()[start..end],
            &gs.note_time_cache_ns()[start..end],
            &gs.hold_end_time_cache_ns()[start..end],
            totals.total_steps,
            totals.holds_total,
            totals.rolls_total,
            totals.mines_total,
            gs.players()[player_idx]
                .fail_time
                .map(deadsync_core::song_time::song_time_ns_from_seconds),
            false,
        ))
    } else {
        None
    };
    groovestats_api::submit_comment(
        &counts,
        fa_plus_ex_score,
        gs.music_rate(),
        profile.timing_windows,
        profile.scroll_speed,
    )
}

fn groovestats_payload_for_player(
    gs: &GameplayCoreState,
    player_idx: usize,
) -> Option<GrooveStatsSubmitPlayerPayload> {
    if player_idx >= gs.num_players() {
        return None;
    }
    let totals = gs.display_totals_for_player(player_idx);
    let score = groovestats_score_10000_from_counts(
        &gs.players()[player_idx].scoring_counts,
        gs.players()[player_idx].holds_held_for_score,
        gs.players()[player_idx].rolls_held_for_score,
        gs.players()[player_idx].mines_hit_for_score,
        totals.possible_grade_points,
    );
    Some(GrooveStatsSubmitPlayerPayload {
        rate: groovestats_rate_hundredths(gs.music_rate()),
        score,
        judgment_counts: groovestats_judgment_counts(gs, player_idx),
        rescore_counts: groovestats_rescore_counts(gs, player_idx),
        used_cmod: groovestats_used_cmod(gs.profiles()[player_idx].scroll_speed),
        comment: groovestats_comment_string(gs, player_idx),
        player_options: groovestats_api::player_options_json(&gs.profiles()[player_idx]),
    })
}

fn refresh_groovestats_submit_leaderboards(player: &GrooveStatsSubmitPlayerJob) {
    invalidate_player_leaderboards_for_side(player.chart_hash.as_str(), player.side);
    get_or_fetch_player_leaderboards_for_side(
        player.chart_hash.as_str(),
        player.side,
        GROOVESTATS_SUBMIT_MAX_ENTRIES,
    );
}

fn cache_groovestats_submit_success(
    player: &GrooveStatsSubmitPlayerJob,
    player_response: &groovestats_api::GrooveStatsSubmitApiPlayer,
) {
    itl::handle_submit_player_unlocks(player, player_response);
    if let Some(profile_id) = player.profile_id.as_deref() {
        let imported = groovestats_api::imported_player_score_from_submit_response(
            player_response,
            player.username.as_str(),
            f64::from(player.score_10000),
            player.comment.as_str(),
        );
        let proves_nonquint_ex = imported.ex_evidence.proves_nonquint();
        let song_cache = get_song_cache();
        let stats = imported_score_chart_stats(&imported, &song_cache, player.chart_hash.as_str());
        let score = cached_score_from_imported_player_score(imported, stats);
        cache_gs_score_for_profile(
            profile_id,
            player.chart_hash.as_str(),
            score,
            player.username.as_str(),
            proves_nonquint_ex,
        );
    }
}

pub fn submit_groovestats_payloads_from_gameplay(gs: &GameplayCoreState) {
    for player_idx in 0..gs.num_players().min(MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        groovestats_reset_submit_ui_status(side, gs.charts()[player_idx].short_hash.as_str());
        groovestats_reset_submit_event_ui(side, gs.charts()[player_idx].short_hash.as_str());
        groovestats_reset_submit_retry(side, gs.charts()[player_idx].short_hash.as_str());
    }

    let cfg = crate::config::get();
    match groovestats_autosubmit_session_decision(GrooveStatsAutosubmitSessionInput {
        enabled: cfg.enable_groovestats,
        player_count: gs.num_players(),
        autoplay_used: gs.autoplay_used(),
        is_course_stage: gs.course_display_is_course_stage(),
        autosubmit_course_scores_individually: cfg.autosubmit_course_scores_individually,
    }) {
        GrooveStatsAutosubmitSessionDecision::Submit => {}
        GrooveStatsAutosubmitSessionDecision::Skip { log } => {
            if let Some(log) = log {
                groovestats_api::log_global_submit_skip(active_groovestats_service_name(), log);
            }
            return;
        }
    }
    let mut drafts = Vec::with_capacity(gs.num_players().min(MAX_PLAYERS));

    for player_idx in 0..gs.num_players().min(MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        let slot = if side == profile_data::PlayerSide::P1 {
            1
        } else {
            2
        };
        let profile = &gs.profiles()[player_idx];
        let chart = gs.charts()[player_idx].as_ref();
        let chart_hash = chart.short_hash.as_str();
        let invalid_reason =
            groovestats_submit_invalid_reason(chart, gs.song().has_lua, profile, gs.music_rate());
        let decision = groovestats_autosubmit_player_decision(GrooveStatsAutosubmitPlayerInput {
            has_invalid_reason: invalid_reason.is_some(),
            is_pad_player: profile.groovestats_is_pad_player,
            song_completed_naturally: gs.song_completed_naturally(),
            is_failing: gs.players()[player_idx].is_failing,
            life: gs.players()[player_idx].life,
            has_fail_time: gs.players()[player_idx].fail_time.is_some(),
            course_stage_life_submit_eligible: gs.course_stage_life_submit_eligible(player_idx),
            api_key_present: !profile.groovestats_api_key.trim().is_empty(),
        });
        match decision.action {
            GrooveStatsAutosubmitPlayerAction::BuildPayload => {}
            GrooveStatsAutosubmitPlayerAction::SkipInvalidReason => {
                if let Some(reason) = invalid_reason {
                    groovestats_api::warn_submit_skip(
                        active_groovestats_service_name(),
                        side,
                        chart_hash,
                        reason.as_str(),
                    );
                }
                continue;
            }
            GrooveStatsAutosubmitPlayerAction::Skip { log } => {
                if let Some(log) = log {
                    groovestats_api::log_player_submit_skip(
                        active_groovestats_service_name(),
                        side,
                        chart_hash,
                        log,
                    );
                }
                continue;
            }
        }

        let itl_score_hundredths = itl::current_score_hundredths_for_submit(gs, player_idx);
        let Some(payload) = groovestats_payload_for_player(gs, player_idx) else {
            groovestats_api::warn_submit_skip(
                active_groovestats_service_name(),
                side,
                chart_hash,
                "failed to build submit payload",
            );
            continue;
        };
        let draft = GrooveStatsSubmitPlayerDraft::new(
            side,
            slot,
            chart_hash.to_string(),
            profile.groovestats_username.trim().to_string(),
            profile.display_name.clone(),
            profile::active_local_profile_id_for_side(side),
            itl_score_hundredths,
            profile.show_ex_score,
            profile.groovestats_api_key.trim().to_string(),
            payload,
        );
        drafts.push(draft);
    }

    let Some(request) = groovestats_api::begin_submit_request_from_drafts(drafts) else {
        return;
    };

    groovestats_api::spawn_submit_request(
        request,
        active_groovestats_service(),
        active_groovestats_service_name(),
        cache_groovestats_submit_success,
        refresh_groovestats_submit_leaderboards,
    );
}

pub fn retry_groovestats_submit(chart_hash: &str, side: profile_data::PlayerSide) -> bool {
    groovestats_api::retry_submit_if_enabled(
        crate::config::get().enable_groovestats,
        chart_hash,
        side,
        true,
        active_groovestats_service(),
        active_groovestats_service_name(),
        cache_groovestats_submit_success,
        refresh_groovestats_submit_leaderboards,
    )
}

/// Returns the seconds remaining until the next retry is allowed (manual
/// cooldown) or scheduled (auto). `Some(0)` means the gate has just elapsed
/// or the auto-retry is due to fire on the next tick. `None` means no gate
/// is currently armed (bare `F5 Retry`).
pub fn groovestats_next_retry_remaining_secs(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    groovestats_api::next_retry_remaining_secs(chart_hash, side)
}

/// Returns true when the next scheduled retry will be fired automatically by
/// the tick driver (i.e., the current UI status is auto-retryable AND the
/// auto-retry budget hasn't been exhausted). When false, any pending
/// `next_retry_at` is acting purely as a manual F5 cooldown gate.
pub fn groovestats_next_retry_is_auto(chart_hash: &str, side: profile_data::PlayerSide) -> bool {
    groovestats_api::next_retry_is_auto(chart_hash, side)
}

/// Fires any auto-retries whose scheduled time has elapsed. Only fires for
/// entries whose current UI status is auto-retryable (see
/// [`GrooveStatsSubmitUiStatus::is_auto_retryable`]) AND whose auto-retry budget
/// hasn't been exhausted; other retryable statuses (and exhausted entries)
/// use `next_retry_at` purely as a manual cooldown gate and are NOT
/// auto-fired by the tick. Should be called once per frame from the
/// evaluation screen update loop. Returns true if at least one retry fired.
pub fn tick_groovestats_auto_retries() -> bool {
    groovestats_api::tick_auto_submit_retries_if_enabled(
        crate::config::get().enable_groovestats,
        active_groovestats_service(),
        active_groovestats_service_name(),
        cache_groovestats_submit_success,
        refresh_groovestats_submit_leaderboards,
    )
}
