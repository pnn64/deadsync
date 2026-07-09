use super::{
    GameplayCoreState, get_or_fetch_player_leaderboards_for_side,
    invalidate_player_leaderboards_for_side, lua_chart_submit_allowed,
};
use crate::game::profile;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_online::arrowcloud::{
    self as arrowcloud_api, ArrowCloudGameplayPayloadInput, ArrowCloudGameplaySubmitInput,
    ArrowCloudGameplaySubmitPlayer, ArrowCloudPayload, ArrowCloudSubmitJob,
};
use deadsync_online::groovestats::GROOVESTATS_SUBMIT_MAX_ENTRIES;
use deadsync_profile as profile_data;
use deadsync_profile_gameplay::blue_fantastic_window_ms_for_profile;
use deadsync_score::{ArrowCloudSubmitStats, arrowcloud_submit_stats_from_live_or_results};

#[inline(always)]
fn player_blue_window_ms(gs: &GameplayCoreState, player_idx: usize) -> f32 {
    let base = gs.default_fa_plus_window_s();
    let Some(profile) = gs.profiles().get(player_idx) else {
        return base * 1000.0;
    };
    blue_fantastic_window_ms_for_profile(base, profile)
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
    let (start, end) = gs.note_range_for_player(player_idx);
    arrowcloud_submit_stats_from_live_or_results(
        arrowcloud_live_submit_stats(gs, player_idx),
        fail_time_ns,
        &gs.notes()[start..end],
        &gs.note_time_cache_ns()[start..end],
        &gs.hold_end_time_cache_ns()[start..end],
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
    let totals = gs.display_totals_for_player(player_idx);
    let (start, end) = gs.note_range_for_player(player_idx);
    let graph = gs.density_graph_view();
    let stream_segments = gs.stream_segments_for_results(player_idx);

    Some(arrowcloud_api::payload_from_gameplay_input(
        ArrowCloudGameplayPayloadInput {
            song_name: song.display_full_title(true),
            artist: song.artist.clone(),
            pack_group,
            music_length_seconds: song.music_length_seconds,
            chart_hash: chart.short_hash.clone(),
            difficulty: chart.meter,
            stepartist: chart.step_artist.clone(),
            notes: &gs.notes()[start..end],
            note_times: &gs.note_time_cache_ns()[start..end],
            col_offset: player_idx.saturating_mul(gs.cols_per_player()),
            cols_per_player: gs.cols_per_player(),
            stream_segments: &stream_segments,
            fail_time_ns,
            submit_stats,
            total_holds: totals.holds_total,
            total_mines: totals.mines_total,
            total_rolls: totals.rolls_total,
            max_nps: chart.max_nps,
            measure_nps: chart.measure_nps_vec.as_slice(),
            measure_seconds: chart.measure_seconds_vec.as_slice(),
            density_first_second: graph.first_second,
            density_last_second: graph.last_second,
            life_history: player.life_history.as_slice(),
            profile,
            music_rate: gs.music_rate(),
            used_autoplay: gs.autoplay_used(),
            is_failing: player.is_failing,
            has_fail_time: player.fail_time.is_some(),
        },
    ))
}

fn cache_arrowcloud_submit_success(job: &ArrowCloudSubmitJob) {
    if let Some(profile_id) = job.profile_id.as_deref() {
        profile::write_ac_submit_scores_for_id(
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
    let cfg = crate::config::get();
    let players = (0..gs.num_players().min(MAX_PLAYERS)).map(|player_idx| {
        let side = profile::gameplay_side_for_player(gs.num_players(), player_idx);
        let chart_hash = gs.charts()[player_idx].short_hash.clone();
        let blue_window_ms = player_blue_window_ms(gs, player_idx);
        ArrowCloudGameplaySubmitPlayer {
            side,
            chart_hash: chart_hash.clone(),
            api_key: gs.profiles()[player_idx].arrowcloud_api_key.clone(),
            profile_id: profile::active_local_profile_id_for_side(side),
            itg_percent: gs.display_itg_score_percent(player_idx).clamp(0.0, 100.0),
            ex_percent: gs
                .display_ex_score_percent(player_idx, blue_window_ms)
                .clamp(0.0, 100.0),
            hard_ex_percent: gs
                .display_hard_ex_score_percent(player_idx, blue_window_ms)
                .clamp(0.0, 100.0),
            song_has_lua: gs.song().has_lua,
            lua_submit_allowed: lua_chart_submit_allowed(chart_hash.as_str()),
            song_completed_naturally: gs.song_completed_naturally(),
            is_failing: gs.players()[player_idx].is_failing,
            life: gs.players()[player_idx].life,
            has_fail_time: gs.players()[player_idx].fail_time.is_some(),
            course_stage_life_submit_eligible: gs.course_stage_life_submit_eligible(player_idx),
            payload: arrowcloud_payload_for_player(gs, player_idx, pack_group),
        }
    });

    arrowcloud_api::submit_gameplay_players(
        ArrowCloudGameplaySubmitInput {
            enabled: cfg.enable_arrowcloud,
            player_count: gs.num_players(),
            autoplay_used: gs.autoplay_used(),
            is_course_stage: gs.course_display_is_course_stage(),
            autosubmit_course_scores_individually: cfg.autosubmit_course_scores_individually,
            submit_fails_enabled: cfg.submit_arrowcloud_fails,
        },
        players,
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
