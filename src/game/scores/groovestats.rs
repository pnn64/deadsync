use super::{
    GameplayCoreState, get_or_fetch_player_leaderboards_for_side,
    invalidate_player_leaderboards_for_side, itl, lua_chart_submit_allowed,
};
use crate::game::profile;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_online::groovestats::{
    self as groovestats_api, GROOVESTATS_SUBMIT_MAX_ENTRIES, GrooveStatsGameplayPayloadInput,
    GrooveStatsGameplaySubmitInput, GrooveStatsGameplaySubmitPlayer,
    GrooveStatsSubmitPlayerPayload, submit_player_payload_from_gameplay_input,
};
use deadsync_profile as profile_data;
use deadsync_profile::Profile;
use deadsync_profile_gameplay::{
    groovestats_eval_state_from_profile, groovestats_submit_invalid_reason_from_profile,
};
use deadsync_score::{
    GrooveStatsEvalState, GrooveStatsGameplayEvalInput, cached_score_from_imported_player_score,
    groovestats_eval_state_from_gameplay_parts, imported_score_chart_stats,
};
use deadsync_simfile::runtime_cache::get_song_cache;

#[inline(always)]
fn active_groovestats_service() -> groovestats_api::Service {
    deadsync_online::runtime::active_groovestats_service()
}

#[inline(always)]
fn active_groovestats_service_name() -> &'static str {
    groovestats_api::service_name(active_groovestats_service())
}

pub(super) use groovestats_api::GrooveStatsSubmitPlayerJob;

#[inline(always)]
fn groovestats_fail_type_ok() -> bool {
    matches!(
        crate::config::get().default_fail_type,
        crate::config::DefaultFailType::Immediate
            | crate::config::DefaultFailType::ImmediateContinue
    )
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
    let base_state = groovestats_eval_state_from_profile(
        gs.charts()[player_idx].as_ref(),
        &gs.profiles()[player_idx],
        gs.music_rate(),
        gs.autoplay_used(),
        gs.course_display_is_course_stage(),
        crate::config::get().autosubmit_course_scores_individually,
        groovestats_fail_type_ok(),
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
    groovestats_submit_invalid_reason_from_profile(
        chart,
        song_has_lua,
        lua_chart_submit_allowed(chart.short_hash.as_str()),
        profile,
        music_rate,
        groovestats_fail_type_ok(),
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
    let player = &gs.players()[player_idx];
    let profile = &gs.profiles()[player_idx];
    let (start, end) = gs.note_range_for_player(player_idx);
    Some(submit_player_payload_from_gameplay_input(
        GrooveStatsGameplayPayloadInput {
            scoring_counts: &player.scoring_counts,
            holds_held_for_score: player.holds_held_for_score,
            rolls_held_for_score: player.rolls_held_for_score,
            mines_hit_for_score: player.mines_hit_for_score,
            possible_grade_points: totals.possible_grade_points,
            music_rate: gs.music_rate(),
            window_counts: gs.live_window_counts(player_idx),
            total_steps: totals.total_steps,
            holds_held: player.holds_held,
            total_holds: totals.holds_total,
            mines_hit: player.mines_hit,
            total_mines: totals.mines_total,
            rolls_held: player.rolls_held,
            total_rolls: totals.rolls_total,
            notes: &gs.notes()[start..end],
            note_times: &gs.note_time_cache_ns()[start..end],
            hold_end_times: &gs.hold_end_time_cache_ns()[start..end],
            fail_time_ns: player
                .fail_time
                .map(deadsync_core::song_time::song_time_ns_from_seconds),
            profile,
        },
    ))
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
        profile::cache_logged_gs_score_for_id(
            profile_id,
            player.chart_hash.as_str(),
            score,
            player.username.as_str(),
            proves_nonquint_ex,
        );
    }
}

pub fn submit_groovestats_payloads_from_gameplay(gs: &GameplayCoreState) {
    let cfg = crate::config::get();
    let players = (0..gs.num_players().min(MAX_PLAYERS)).map(|player_idx| {
        let side = profile::gameplay_side_for_player(gs.num_players(), player_idx);
        let slot = if side == profile_data::PlayerSide::P1 {
            1
        } else {
            2
        };
        let profile = &gs.profiles()[player_idx];
        let chart = gs.charts()[player_idx].as_ref();
        let chart_hash = chart.short_hash.as_str();
        GrooveStatsGameplaySubmitPlayer {
            side,
            slot,
            chart_hash: chart_hash.to_string(),
            username: profile.groovestats_username.clone(),
            profile_name: profile.display_name.clone(),
            profile_id: profile::active_local_profile_id_for_side(side),
            itl_score_hundredths: itl::current_score_hundredths_for_submit(gs, player_idx),
            show_ex_score: profile.show_ex_score,
            api_key: profile.groovestats_api_key.clone(),
            is_pad_player: profile.groovestats_is_pad_player,
            invalid_reason: groovestats_submit_invalid_reason(
                chart,
                gs.song().has_lua,
                profile,
                gs.music_rate(),
            ),
            song_completed_naturally: gs.song_completed_naturally(),
            is_failing: gs.players()[player_idx].is_failing,
            life: gs.players()[player_idx].life,
            has_fail_time: gs.players()[player_idx].fail_time.is_some(),
            course_stage_life_submit_eligible: gs.course_stage_life_submit_eligible(player_idx),
            payload: groovestats_payload_for_player(gs, player_idx),
        }
    });

    groovestats_api::submit_gameplay_players(
        GrooveStatsGameplaySubmitInput {
            enabled: cfg.enable_groovestats,
            service: active_groovestats_service(),
            service_name: active_groovestats_service_name(),
            player_count: gs.num_players(),
            autoplay_used: gs.autoplay_used(),
            is_course_stage: gs.course_display_is_course_stage(),
            autosubmit_course_scores_individually: cfg.autosubmit_course_scores_individually,
        },
        players,
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
