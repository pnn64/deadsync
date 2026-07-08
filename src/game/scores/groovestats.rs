use super::{
    GameplayCoreState, cache_gs_score_for_profile, chart_stats_for_imported_score,
    gameplay_run_failed, gameplay_run_passed, gameplay_side_for_player,
    get_or_fetch_player_leaderboards_for_side, invalidate_player_leaderboards_for_side, itl,
    lua_chart_submit_allowed, submit_record_banner,
};
use crate::game::online::groovestats as online_groovestats;
use crate::game::profile;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_online::groovestats::{
    self as groovestats_api, GROOVESTATS_SUBMIT_MAX_ENTRIES, GrooveStatsJudgmentCounts,
    GrooveStatsRescoreCounts, GrooveStatsSubmitApiResponse, GrooveStatsSubmitPlayerPayload,
    GrooveStatsSubmitRequestError,
};
use deadsync_profile as profile_data;
use deadsync_profile::Profile;
use deadsync_rules::judgment;
use deadsync_score::{
    EventProgress, GrooveStatsEvalInput, GrooveStatsEvalState, GrooveStatsSubmitRecordBanner,
    GrooveStatsSubmitUiStatus, RejectReason, SUBMIT_RETRY_MAX_ATTEMPTS, SubmitEventUiState,
    SubmitRetryState, SubmitUiState, cached_score_from_imported_player_score,
    groovestats_eval_state_from_parts, groovestats_rate_hundredths,
    groovestats_score_10000_from_counts, groovestats_used_cmod,
};
use log::{debug, warn};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::Instant;

#[inline(always)]
fn active_groovestats_service() -> groovestats_api::Service {
    online_groovestats::active_service()
}

#[inline(always)]
fn active_groovestats_service_name() -> &'static str {
    groovestats_api::service_name(active_groovestats_service())
}

static GROOVESTATS_SUBMIT_UI_STATUS: std::sync::LazyLock<
    Mutex<SubmitUiState<GrooveStatsSubmitUiStatus>>,
> = std::sync::LazyLock::new(|| Mutex::new(SubmitUiState::default()));
static GROOVESTATS_SUBMIT_UI_TOKEN: AtomicU64 = AtomicU64::new(1);

static GROOVESTATS_SUBMIT_EVENT_UI: std::sync::LazyLock<
    Mutex<SubmitEventUiState<Vec<EventProgress>, GrooveStatsSubmitRecordBanner>>,
> = std::sync::LazyLock::new(|| Mutex::new(SubmitEventUiState::default()));

#[derive(Debug, Clone)]
struct GrooveStatsSubmitRetryEntry {
    side: profile_data::PlayerSide,
    slot: u8,
    chart_hash: String,
    username: String,
    profile_name: String,
    profile_id: Option<String>,
    itl_score_hundredths: Option<u32>,
    show_ex_score: bool,
    api_key: String,
    payload: GrooveStatsSubmitPlayerPayload,
    /// Consecutive failures, capped at `SUBMIT_RETRY_MAX_ATTEMPTS`. Drives
    /// the shared backoff schedule so mixed failure kinds keep ratcheting the
    /// same curve. Reset only on a successful submit.
    retry_attempt: u8,
    /// When the next retry is allowed (manual cooldown) or scheduled (auto).
    /// `None` means no gate and no auto-retry pending.
    next_retry_at: Option<Instant>,
}

/// Maximum number of attempts before the backoff schedule saturates.
/// For *auto-retryable* statuses this is also the auto-retry budget (after
/// `MAX_ATTEMPTS` failures, auto-retry is exhausted and the user gets bare
/// `F5 Retry`). For *manual-only* statuses the cooldown caps at
/// `delay(MAX)` and stays there for subsequent failures.
/// Alias of the shared [`SUBMIT_RETRY_MAX_ATTEMPTS`].
const GROOVESTATS_RETRY_MAX_ATTEMPTS: u8 = SUBMIT_RETRY_MAX_ATTEMPTS;

static GROOVESTATS_SUBMIT_RETRY: std::sync::LazyLock<
    Mutex<SubmitRetryState<GrooveStatsSubmitRetryEntry>>,
> = std::sync::LazyLock::new(|| Mutex::new(SubmitRetryState::default()));

const GROOVESTATS_SUBMIT_RETRY_TRACKED_PER_SIDE: usize = 128;

#[derive(Debug)]
pub(super) struct GrooveStatsSubmitPlayerJob {
    pub(super) side: profile_data::PlayerSide,
    pub(super) slot: u8,
    pub(super) chart_hash: String,
    pub(super) username: String,
    pub(super) profile_name: String,
    pub(super) profile_id: Option<String>,
    pub(super) token: u64,
    pub(super) itl_score_hundredths: Option<u32>,
    pub(super) show_ex_score: bool,
    pub(super) score_10000: u32,
    pub(super) rate_hundredths: u32,
    pub(super) comment: String,
}

#[derive(Debug)]
struct GrooveStatsSubmitRequest {
    players: Vec<GrooveStatsSubmitPlayerJob>,
    parts: groovestats_api::GrooveStatsSubmitRequestParts,
}

#[derive(Debug)]
struct GrooveStatsSubmitError {
    status: GrooveStatsSubmitUiStatus,
    message: String,
}

#[inline(always)]
fn groovestats_reset_submit_ui_status(side: profile_data::PlayerSide, chart_hash: &str) {
    GROOVESTATS_SUBMIT_UI_STATUS
        .lock()
        .unwrap()
        .reset(profile_data::player_side_index(side), chart_hash);
}

#[inline(always)]
fn groovestats_reset_submit_event_ui(side: profile_data::PlayerSide, chart_hash: &str) {
    GROOVESTATS_SUBMIT_EVENT_UI
        .lock()
        .unwrap()
        .reset(profile_data::player_side_index(side), chart_hash);
}

#[inline(always)]
fn groovestats_reset_submit_retry(side: profile_data::PlayerSide, chart_hash: &str) {
    GROOVESTATS_SUBMIT_RETRY.lock().unwrap().reset_by_key(
        profile_data::player_side_index(side),
        chart_hash,
        |entry| entry.chart_hash.as_str(),
    );
}

#[inline(always)]
fn groovestats_set_submit_ui_status(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: GrooveStatsSubmitUiStatus,
) {
    GROOVESTATS_SUBMIT_UI_STATUS.lock().unwrap().set(
        profile_data::player_side_index(side),
        chart_hash,
        token,
        status,
    );
}

#[inline(always)]
fn groovestats_update_submit_ui_status_if_token(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: GrooveStatsSubmitUiStatus,
) -> bool {
    GROOVESTATS_SUBMIT_UI_STATUS
        .lock()
        .unwrap()
        .update_if_token(
            profile_data::player_side_index(side),
            chart_hash,
            token,
            status,
        )
}

#[inline(always)]
fn groovestats_arm_submit_event_ui(side: profile_data::PlayerSide, chart_hash: &str, token: u64) {
    GROOVESTATS_SUBMIT_EVENT_UI.lock().unwrap().arm(
        profile_data::player_side_index(side),
        chart_hash,
        token,
    );
}

#[inline(always)]
fn groovestats_update_submit_event_ui_if_token(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    token: u64,
    event_progress: Vec<EventProgress>,
    record_banner: Option<GrooveStatsSubmitRecordBanner>,
) {
    GROOVESTATS_SUBMIT_EVENT_UI.lock().unwrap().update_if_token(
        profile_data::player_side_index(side),
        chart_hash,
        token,
        event_progress,
        record_banner,
    );
}

#[inline(always)]
fn groovestats_next_submit_ui_token() -> u64 {
    GROOVESTATS_SUBMIT_UI_TOKEN.fetch_add(1, AtomicOrdering::Relaxed)
}

#[inline(always)]
fn groovestats_store_submit_retry(entry: GrooveStatsSubmitRetryEntry) {
    let side = entry.side;
    GROOVESTATS_SUBMIT_RETRY.lock().unwrap().upsert_by_key(
        profile_data::player_side_index(side),
        entry,
        |entry| entry.chart_hash.as_str(),
        GROOVESTATS_SUBMIT_RETRY_TRACKED_PER_SIDE,
    );
}

pub fn get_groovestats_submit_ui_status_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<GrooveStatsSubmitUiStatus> {
    GROOVESTATS_SUBMIT_UI_STATUS
        .lock()
        .unwrap()
        .get(profile_data::player_side_index(side), chart_hash)
}

pub fn get_groovestats_submit_event_progress_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Vec<EventProgress> {
    GROOVESTATS_SUBMIT_EVENT_UI
        .lock()
        .unwrap()
        .progress(profile_data::player_side_index(side), chart_hash)
}

pub fn get_groovestats_submit_record_banner_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<GrooveStatsSubmitRecordBanner> {
    GROOVESTATS_SUBMIT_EVENT_UI
        .lock()
        .unwrap()
        .banner(profile_data::player_side_index(side), chart_hash)
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
    let mut state = groovestats_eval_state(
        gs.charts()[player_idx].as_ref(),
        &gs.profiles()[player_idx],
        gs.music_rate(),
        gs.autoplay_used(),
        gs.course_display_is_course_stage(),
        crate::config::get().autosubmit_course_scores_individually,
    );
    if state.valid
        && gs.song().has_lua
        && !lua_chart_submit_allowed(gs.charts()[player_idx].short_hash.as_str())
    {
        state.valid = false;
        state.reason_lines.push("simfile relies on lua".to_string());
        return state;
    }
    let failed = gameplay_run_failed(
        gs.players()[player_idx].is_failing,
        gs.players()[player_idx].fail_time.is_some(),
    );
    let passed = gameplay_run_passed(
        gs.song_completed_naturally(),
        gs.players()[player_idx].is_failing,
        gs.players()[player_idx].life,
        gs.players()[player_idx].fail_time.is_some(),
    );
    let finished = gs.song_completed_naturally() || failed;
    if state.valid && !finished {
        state.valid = false;
        state
            .reason_lines
            .push("Only completed stages can be submitted.".to_string());
        return state;
    }
    if state.valid && failed {
        state.valid = false;
        state
            .reason_lines
            .push("Only passing scores are submitted.".to_string());
        return state;
    }
    if state.valid && !gs.course_stage_life_submit_eligible(player_idx) {
        state.valid = false;
        state
            .reason_lines
            .push("Course stage would have failed from normal life.".to_string());
        return state;
    }
    if state.valid && passed {
        state.manual_qr_url = groovestats_manual_qr_url_from_gameplay(gs, player_idx);
    }
    state
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

#[inline(always)]
fn groovestats_warn_submit_skip(side: profile_data::PlayerSide, chart_hash: &str, reason: &str) {
    warn!(
        "Skipping {} submit for {:?} ({}): {}.",
        active_groovestats_service_name(),
        side,
        chart_hash,
        reason
    );
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

fn submit_groovestats_request(
    job: &GrooveStatsSubmitRequest,
) -> Result<GrooveStatsSubmitApiResponse, GrooveStatsSubmitError> {
    let service = active_groovestats_service();
    let service_name = active_groovestats_service_name();
    match groovestats_api::submit_score_request(
        service,
        &job.parts.headers,
        &job.parts.query,
        &job.parts.body,
    ) {
        Ok(success) => {
            if success.body_snippet.is_empty() {
                debug!("{service_name} submit success");
            } else {
                debug!(
                    "{service_name} submit success body='{}'",
                    success.body_snippet.as_str()
                );
            }
            Ok(success.response)
        }
        Err(error) => Err(groovestats_submit_error_from_online(service_name, error)),
    }
}

fn groovestats_submit_error_from_online(
    service_name: &str,
    error: GrooveStatsSubmitRequestError,
) -> GrooveStatsSubmitError {
    let (status, message) = groovestats_api::submit_error_status_and_message(service_name, &error);
    GrooveStatsSubmitError { status, message }
}

fn spawn_groovestats_submit(job: GrooveStatsSubmitRequest) {
    std::thread::spawn(move || match submit_groovestats_request(&job) {
        Ok(response) => {
            for player in &job.players {
                let Some(player_response) = response.player_for_slot(player.slot) else {
                    groovestats_update_submit_ui_status_if_token(
                        player.side,
                        player.chart_hash.as_str(),
                        player.token,
                        GrooveStatsSubmitUiStatus::Rejected {
                            reason: RejectReason::InvalidScore,
                        },
                    );
                    warn!(
                        "{} submit response omitted player{} for {:?} ({}).",
                        active_groovestats_service_name(),
                        player.slot,
                        player.side,
                        player.chart_hash
                    );
                    invalidate_player_leaderboards_for_side(
                        player.chart_hash.as_str(),
                        player.side,
                    );
                    get_or_fetch_player_leaderboards_for_side(
                        player.chart_hash.as_str(),
                        player.side,
                        GROOVESTATS_SUBMIT_MAX_ENTRIES,
                    );
                    continue;
                };
                if !player_response.chart_hash.trim().is_empty()
                    && !player_response
                        .chart_hash
                        .eq_ignore_ascii_case(player.chart_hash.as_str())
                {
                    groovestats_update_submit_ui_status_if_token(
                        player.side,
                        player.chart_hash.as_str(),
                        player.token,
                        GrooveStatsSubmitUiStatus::Rejected {
                            reason: RejectReason::InvalidScore,
                        },
                    );
                    warn!(
                        "{} submit response hash mismatch for {:?}: expected {}, got {}.",
                        active_groovestats_service_name(),
                        player.side,
                        player.chart_hash,
                        player_response.chart_hash
                    );
                    invalidate_player_leaderboards_for_side(
                        player.chart_hash.as_str(),
                        player.side,
                    );
                    get_or_fetch_player_leaderboards_for_side(
                        player.chart_hash.as_str(),
                        player.side,
                        GROOVESTATS_SUBMIT_MAX_ENTRIES,
                    );
                    continue;
                }

                groovestats_update_submit_event_ui_if_token(
                    player.side,
                    player.chart_hash.as_str(),
                    player.token,
                    itl::event_progress_from_submit(player, player_response),
                    submit_record_banner(player, player_response),
                );
                itl::handle_submit_player_unlocks(player, player_response);
                let accepted = groovestats_update_submit_ui_status_if_token(
                    player.side,
                    player.chart_hash.as_str(),
                    player.token,
                    GrooveStatsSubmitUiStatus::Submitted,
                );
                if accepted {
                    groovestats_record_submit_success(player.side, player.chart_hash.as_str());
                }
                if let Some(profile_id) = player.profile_id.as_deref() {
                    let imported = groovestats_api::imported_player_score_from_submit_response(
                        player_response,
                        player.username.as_str(),
                        f64::from(player.score_10000),
                        player.comment.as_str(),
                    );
                    let proves_nonquint_ex = imported.ex_evidence.proves_nonquint();
                    let stats =
                        chart_stats_for_imported_score(&imported, player.chart_hash.as_str());
                    let score = cached_score_from_imported_player_score(imported, stats);
                    cache_gs_score_for_profile(
                        profile_id,
                        player.chart_hash.as_str(),
                        score,
                        player.username.as_str(),
                        proves_nonquint_ex,
                    );
                }
                debug!(
                    "{} submit succeeded for {:?} ({}) result='{}'",
                    active_groovestats_service_name(),
                    player.side,
                    player.chart_hash,
                    player_response.result
                );
                invalidate_player_leaderboards_for_side(player.chart_hash.as_str(), player.side);
                get_or_fetch_player_leaderboards_for_side(
                    player.chart_hash.as_str(),
                    player.side,
                    GROOVESTATS_SUBMIT_MAX_ENTRIES,
                );
            }
        }
        Err(err) => {
            let status = err.status;
            for player in &job.players {
                let accepted = groovestats_update_submit_ui_status_if_token(
                    player.side,
                    player.chart_hash.as_str(),
                    player.token,
                    status,
                );
                warn!(
                    "{} submit failed for {:?} ({}) status={:?}: {}",
                    active_groovestats_service_name(),
                    player.side,
                    player.chart_hash,
                    status,
                    err.message
                );
                if accepted {
                    groovestats_record_submit_failure(
                        player.side,
                        player.chart_hash.as_str(),
                        status,
                    );
                }
                invalidate_player_leaderboards_for_side(player.chart_hash.as_str(), player.side);
                get_or_fetch_player_leaderboards_for_side(
                    player.chart_hash.as_str(),
                    player.side,
                    GROOVESTATS_SUBMIT_MAX_ENTRIES,
                );
            }
        }
    });
}

fn groovestats_retry_request(
    entry: &GrooveStatsSubmitRetryEntry,
    token: u64,
) -> GrooveStatsSubmitRequest {
    let player = GrooveStatsSubmitPlayerJob {
        side: entry.side,
        slot: entry.slot,
        chart_hash: entry.chart_hash.clone(),
        username: entry.username.clone(),
        profile_name: entry.profile_name.clone(),
        profile_id: entry.profile_id.clone(),
        token,
        itl_score_hundredths: entry.itl_score_hundredths,
        show_ex_score: entry.show_ex_score,
        score_10000: entry.payload.score,
        rate_hundredths: entry.payload.rate,
        comment: entry.payload.comment.clone(),
    };
    let request_player = groovestats_api::GrooveStatsSubmitPlayerRequest {
        slot: entry.slot,
        chart_hash: entry.chart_hash.clone(),
        api_key: entry.api_key.clone(),
        payload: entry.payload.clone(),
    };
    GrooveStatsSubmitRequest {
        players: vec![player],
        parts: groovestats_api::submit_request_parts(&[request_player]),
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
    if !cfg.enable_groovestats || gs.num_players() == 0 {
        return;
    }
    if gs.autoplay_used() {
        debug!(
            "Skipping {} submit: autoplay/replay was used.",
            active_groovestats_service_name()
        );
        return;
    }
    if gs.course_display_is_course_stage() && !cfg.autosubmit_course_scores_individually {
        debug!(
            "Skipping {} submit: course per-song autosubmit is disabled.",
            active_groovestats_service_name()
        );
        return;
    }
    let mut players = Vec::with_capacity(gs.num_players().min(MAX_PLAYERS));
    let mut request_players = Vec::with_capacity(gs.num_players().min(MAX_PLAYERS));

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
        let failed = gameplay_run_failed(
            gs.players()[player_idx].is_failing,
            gs.players()[player_idx].fail_time.is_some(),
        );
        let passed = gameplay_run_passed(
            gs.song_completed_naturally(),
            gs.players()[player_idx].is_failing,
            gs.players()[player_idx].life,
            gs.players()[player_idx].fail_time.is_some(),
        );
        let finished = gs.song_completed_naturally() || failed;

        if let Some(reason) =
            groovestats_submit_invalid_reason(chart, gs.song().has_lua, profile, gs.music_rate())
        {
            groovestats_warn_submit_skip(side, chart_hash, reason.as_str());
            continue;
        }
        if !profile.groovestats_is_pad_player {
            groovestats_warn_submit_skip(side, chart_hash, "profile is not marked as a pad player");
            continue;
        }
        if !finished {
            debug!(
                "Skipping {} submit for {:?} ({}): stage was not completed.",
                active_groovestats_service_name(),
                side,
                chart_hash
            );
            continue;
        }
        if !passed {
            debug!(
                "Skipping {} submit for {:?} ({}): stage was not passed.",
                active_groovestats_service_name(),
                side,
                chart_hash
            );
            continue;
        }
        if !gs.course_stage_life_submit_eligible(player_idx) {
            groovestats_warn_submit_skip(
                side,
                chart_hash,
                "course stage would have failed from normal life",
            );
            continue;
        }
        if profile.groovestats_api_key.trim().is_empty() {
            groovestats_warn_submit_skip(side, chart_hash, "profile is missing API key");
            continue;
        }

        let itl_score_hundredths = itl::current_score_hundredths_for_submit(gs, player_idx);
        let Some(payload) = groovestats_payload_for_player(gs, player_idx) else {
            groovestats_warn_submit_skip(side, chart_hash, "failed to build submit payload");
            continue;
        };
        groovestats_store_submit_retry(GrooveStatsSubmitRetryEntry {
            side,
            slot,
            chart_hash: chart_hash.to_string(),
            username: profile.groovestats_username.trim().to_string(),
            profile_name: profile.display_name.clone(),
            profile_id: profile::active_local_profile_id_for_side(side),
            itl_score_hundredths,
            show_ex_score: profile.show_ex_score,
            api_key: profile.groovestats_api_key.trim().to_string(),
            payload: payload.clone(),
            retry_attempt: 0,
            next_retry_at: None,
        });
        let token = groovestats_next_submit_ui_token();
        groovestats_set_submit_ui_status(
            side,
            chart_hash,
            token,
            GrooveStatsSubmitUiStatus::Submitting,
        );
        groovestats_arm_submit_event_ui(side, chart_hash, token);
        players.push(GrooveStatsSubmitPlayerJob {
            side,
            slot,
            chart_hash: chart_hash.to_string(),
            username: profile.groovestats_username.trim().to_string(),
            profile_name: profile.display_name.clone(),
            profile_id: profile::active_local_profile_id_for_side(side),
            token,
            itl_score_hundredths,
            show_ex_score: profile.show_ex_score,
            score_10000: payload.score,
            rate_hundredths: payload.rate,
            comment: payload.comment.clone(),
        });
        request_players.push(groovestats_api::GrooveStatsSubmitPlayerRequest {
            slot,
            chart_hash: chart_hash.to_string(),
            api_key: profile.groovestats_api_key.trim().to_string(),
            payload,
        });
    }

    if players.is_empty() {
        return;
    }

    let job = GrooveStatsSubmitRequest {
        players,
        parts: groovestats_api::submit_request_parts(&request_players),
    };
    spawn_groovestats_submit(job);
}

pub fn retry_groovestats_submit(chart_hash: &str, side: profile_data::PlayerSide) -> bool {
    retry_groovestats_submit_inner(chart_hash, side, true)
}

fn retry_groovestats_submit_inner(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    manual: bool,
) -> bool {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return false;
    }
    let cfg = crate::config::get();
    if !cfg.enable_groovestats {
        return false;
    }
    let Some(status) = get_groovestats_submit_ui_status_for_side(hash, side) else {
        return false;
    };
    if !status.can_retry() {
        return false;
    }
    let entry = {
        let mut lock = GROOVESTATS_SUBMIT_RETRY.lock().unwrap();
        let Some(stored) = lock.take_ready_by_key(
            profile_data::player_side_index(side),
            hash,
            manual,
            Instant::now(),
            |entry| entry.chart_hash.as_str(),
            |entry| &mut entry.next_retry_at,
        ) else {
            return false;
        };
        // Manual fires are gated by the cooldown — refuse if it hasn't elapsed.
        // Auto fires (driven by tick) are already filtered by the schedule, so
        // they bypass this gate.
        stored
    };

    let token = groovestats_next_submit_ui_token();
    groovestats_set_submit_ui_status(side, hash, token, GrooveStatsSubmitUiStatus::Submitting);
    groovestats_arm_submit_event_ui(side, hash, token);
    debug!(
        "Retrying {} submit for {:?} ({}).",
        active_groovestats_service_name(),
        side,
        hash
    );
    spawn_groovestats_submit(groovestats_retry_request(&entry, token));
    true
}

/// Updates the retry entry's backoff schedule based on a worker-reported
/// failure. Only call this after the worker's UI status update was accepted
/// (i.e., the result wasn't from a stale token), so that late results from
/// superseded requests cannot re-arm the schedule.
///
/// Every retryable failure — auto or manual — advances the same shared
/// `retry_attempt` counter, so mixed failure kinds (e.g., timeout → 5xx →
/// timeout) keep ratcheting along the same exponential curve instead of
/// each kind walking its own track. Auto-firing is gated on the current
/// status being [`GrooveStatsSubmitUiStatus::is_auto_retryable`] AND
/// `retry_attempt <= MAX_ATTEMPTS`; otherwise `next_retry_at` acts purely
/// as a manual F5 cooldown gate.
fn groovestats_record_submit_failure(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    status: GrooveStatsSubmitUiStatus,
) {
    let mut lock = GROOVESTATS_SUBMIT_RETRY.lock().unwrap();
    lock.record_failure_by_key(
        profile_data::player_side_index(side),
        chart_hash,
        status.can_retry(),
        GROOVESTATS_RETRY_MAX_ATTEMPTS,
        Instant::now(),
        |entry| entry.chart_hash.as_str(),
        |entry| &mut entry.retry_attempt,
        |entry| &mut entry.next_retry_at,
    );
}

/// Clears retry/backoff bookkeeping after a successful submit. Called from the
/// worker's success path when the status update was accepted.
fn groovestats_record_submit_success(side: profile_data::PlayerSide, chart_hash: &str) {
    groovestats_reset_submit_retry(side, chart_hash);
}

/// Returns the seconds remaining until the next retry is allowed (manual
/// cooldown) or scheduled (auto). `Some(0)` means the gate has just elapsed
/// or the auto-retry is due to fire on the next tick. `None` means no gate
/// is currently armed (bare `F5 Retry`).
pub fn groovestats_next_retry_remaining_secs(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    let lock = GROOVESTATS_SUBMIT_RETRY.lock().unwrap();
    lock.remaining_secs_by_key(
        profile_data::player_side_index(side),
        hash,
        Instant::now(),
        |entry| entry.chart_hash.as_str(),
        |entry| entry.next_retry_at,
    )
}

/// Returns true when the next scheduled retry will be fired automatically by
/// the tick driver (i.e., the current UI status is auto-retryable AND the
/// auto-retry budget hasn't been exhausted). When false, any pending
/// `next_retry_at` is acting purely as a manual F5 cooldown gate.
pub fn groovestats_next_retry_is_auto(chart_hash: &str, side: profile_data::PlayerSide) -> bool {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return false;
    }
    let attempt = {
        let lock = GROOVESTATS_SUBMIT_RETRY.lock().unwrap();
        let Some(attempt) = lock.retry_attempt_by_key(
            profile_data::player_side_index(side),
            hash,
            |entry| entry.chart_hash.as_str(),
            |entry| entry.retry_attempt,
        ) else {
            return false;
        };
        attempt
    };
    if attempt >= GROOVESTATS_RETRY_MAX_ATTEMPTS {
        return false;
    }
    matches!(
        get_groovestats_submit_ui_status_for_side(hash, side),
        Some(s) if s.is_auto_retryable()
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
    let due: Vec<(String, profile_data::PlayerSide, u8)> = {
        let lock = GROOVESTATS_SUBMIT_RETRY.lock().unwrap();
        lock.due_retries(
            Instant::now(),
            |entry| entry.chart_hash.as_str(),
            |entry| entry.side,
            |entry| entry.retry_attempt,
            |entry| entry.next_retry_at,
        )
    };
    let mut fired = false;
    for (hash, side, attempt) in due {
        if attempt >= GROOVESTATS_RETRY_MAX_ATTEMPTS {
            // Auto budget exhausted — `next_retry_at` is now a manual-only
            // cooldown gate. Don't auto-fire.
            continue;
        }
        let Some(status) = get_groovestats_submit_ui_status_for_side(&hash, side) else {
            continue;
        };
        if status.is_auto_retryable() && retry_groovestats_submit_inner(&hash, side, false) {
            fired = true;
        }
    }
    fired
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::{ArrowStats, ChartData, StaminaCounts, TechCounts};
    use deadsync_profile::RemoveMask;
    use deadsync_rules::scroll::ScrollSpeedSetting;

    fn sample_chart(chart_type: &str) -> ChartData {
        ChartData {
            chart_type: chart_type.to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 12,
            step_artist: String::new(),
            music_path: None,
            short_hash: "deadbeefcafebabe".to_string(),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 12,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 12,
            display_bpm: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
        }
    }

    fn sample_player_payload() -> GrooveStatsSubmitPlayerPayload {
        GrooveStatsSubmitPlayerPayload {
            rate: 150,
            score: 9_975,
            judgment_counts: GrooveStatsJudgmentCounts {
                fantastic_plus: 7,
                fantastic: 12,
                excellent: 18,
                great: 4,
                decent: Some(1),
                way_off: Some(0),
                miss: 2,
                total_steps: 213,
                holds_held: 5,
                total_holds: 6,
                mines_hit: 1,
                total_mines: 8,
                rolls_held: 2,
                total_rolls: 3,
            },
            rescore_counts: GrooveStatsRescoreCounts {
                fantastic_plus: 1,
                fantastic: 2,
                excellent: 3,
                great: 4,
                decent: 5,
                way_off: 6,
            },
            used_cmod: true,
            comment: "[DS], FA+, 99.50EX, 2w, 1m, C650".to_string(),
            player_options: "{\"SpeedModType\":2,\"SpeedMod\":650}".to_string(),
        }
    }

    fn sample_retry_entry(
        hash: &str,
        side: profile_data::PlayerSide,
    ) -> GrooveStatsSubmitRetryEntry {
        let slot = if side == profile_data::PlayerSide::P1 {
            1
        } else {
            2
        };
        GrooveStatsSubmitRetryEntry {
            side,
            slot,
            chart_hash: hash.to_string(),
            username: "PerfectTaste".to_string(),
            profile_name: "PerfectTaste".to_string(),
            profile_id: None,
            itl_score_hundredths: None,
            show_ex_score: true,
            api_key: "test-api-key".to_string(),
            payload: sample_player_payload(),
            retry_attempt: 0,
            next_retry_at: None,
        }
    }

    #[test]
    fn groovestats_validity_allows_cmod_and_no_mines() {
        let mut profile = Profile::default();
        profile.scroll_speed = ScrollSpeedSetting::CMod(650.0);
        profile.remove_active_mask = RemoveMask::from_bits_truncate(1u8 << 1);

        assert_eq!(
            groovestats_submit_invalid_reason(&sample_chart("dance-single"), false, &profile, 1.5),
            None
        );
    }

    #[test]
    fn groovestats_validity_rejects_custom_window_and_solo() {
        let mut profile = Profile::default();
        profile.custom_fantastic_window = true;

        assert_eq!(
            groovestats_submit_invalid_reason(&sample_chart("dance-single"), false, &profile, 1.0),
            Some("Metrics or preferences are incorrect.".to_string())
        );
        assert_eq!(
            groovestats_submit_invalid_reason(
                &sample_chart("dance-solo"),
                false,
                &Profile::default(),
                1.0
            ),
            Some("GrooveStats does not support dance-solo charts.".to_string())
        );
    }

    #[test]
    fn groovestats_validity_rejects_lua_simfiles() {
        let mut formerly_allowed = sample_chart("dance-single");
        formerly_allowed.short_hash = "d5bd4dd7224f68ff".to_string();
        assert_eq!(
            groovestats_submit_invalid_reason(&formerly_allowed, true, &Profile::default(), 1.0),
            Some("simfile relies on lua".to_string())
        );
        assert_eq!(
            groovestats_submit_invalid_reason(
                &sample_chart("dance-single"),
                true,
                &Profile::default(),
                1.0,
            ),
            Some("simfile relies on lua".to_string())
        );
    }

    #[test]
    fn groovestats_course_validity_follows_per_song_submit_setting() {
        let chart = sample_chart("dance-single");
        let profile = Profile::default();

        assert!(!groovestats_eval_state(&chart, &profile, 1.0, false, true, false).valid);
        assert!(groovestats_eval_state(&chart, &profile, 1.0, false, true, true).valid);
    }

    #[test]
    fn groovestats_submit_ui_tracks_multiple_hashes_per_side() {
        let side = profile_data::PlayerSide::P1;
        let first = "gs-course-status-first";
        let second = "gs-course-status-second";
        groovestats_reset_submit_ui_status(side, first);
        groovestats_reset_submit_ui_status(side, second);
        groovestats_reset_submit_event_ui(side, first);
        groovestats_reset_submit_event_ui(side, second);

        groovestats_set_submit_ui_status(side, first, 11, GrooveStatsSubmitUiStatus::Submitting);
        groovestats_set_submit_ui_status(side, second, 12, GrooveStatsSubmitUiStatus::Submitted);
        groovestats_arm_submit_event_ui(side, first, 11);
        groovestats_arm_submit_event_ui(side, second, 12);
        groovestats_update_submit_event_ui_if_token(
            side,
            first,
            11,
            Vec::new(),
            Some(GrooveStatsSubmitRecordBanner::PersonalBest),
        );
        groovestats_update_submit_event_ui_if_token(
            side,
            second,
            12,
            Vec::new(),
            Some(GrooveStatsSubmitRecordBanner::WorldRecord),
        );

        assert_eq!(
            get_groovestats_submit_ui_status_for_side(first, side),
            Some(GrooveStatsSubmitUiStatus::Submitting)
        );
        assert_eq!(
            get_groovestats_submit_ui_status_for_side(second, side),
            Some(GrooveStatsSubmitUiStatus::Submitted)
        );
        assert_eq!(
            get_groovestats_submit_record_banner_for_side(first, side),
            Some(GrooveStatsSubmitRecordBanner::PersonalBest)
        );
        assert_eq!(
            get_groovestats_submit_record_banner_for_side(second, side),
            Some(GrooveStatsSubmitRecordBanner::WorldRecord)
        );
        assert!(groovestats_update_submit_ui_status_if_token(
            side,
            first,
            11,
            GrooveStatsSubmitUiStatus::TimedOut,
        ));
        assert!(!groovestats_update_submit_ui_status_if_token(
            side,
            first,
            12,
            GrooveStatsSubmitUiStatus::Submitted,
        ));
        assert_eq!(
            get_groovestats_submit_ui_status_for_side(first, side),
            Some(GrooveStatsSubmitUiStatus::TimedOut)
        );
        assert_eq!(
            get_groovestats_submit_ui_status_for_side(second, side),
            Some(GrooveStatsSubmitUiStatus::Submitted)
        );

        groovestats_reset_submit_ui_status(side, first);
        groovestats_reset_submit_ui_status(side, second);
        groovestats_reset_submit_event_ui(side, first);
        groovestats_reset_submit_event_ui(side, second);
    }

    #[test]
    fn groovestats_submit_retry_tracks_multiple_hashes_per_side() {
        let side = profile_data::PlayerSide::P1;
        let first = "gs-course-retry-first";
        let second = "gs-course-retry-second";
        groovestats_reset_submit_ui_status(side, first);
        groovestats_reset_submit_ui_status(side, second);
        groovestats_reset_submit_retry(side, first);
        groovestats_reset_submit_retry(side, second);

        groovestats_store_submit_retry(sample_retry_entry(first, side));
        groovestats_store_submit_retry(sample_retry_entry(second, side));
        groovestats_set_submit_ui_status(side, first, 21, GrooveStatsSubmitUiStatus::TimedOut);
        groovestats_set_submit_ui_status(side, second, 22, GrooveStatsSubmitUiStatus::NetworkError);

        groovestats_record_submit_failure(side, first, GrooveStatsSubmitUiStatus::TimedOut);
        groovestats_record_submit_failure(side, second, GrooveStatsSubmitUiStatus::NetworkError);

        assert!(groovestats_next_retry_remaining_secs(first, side).is_some());
        assert!(groovestats_next_retry_is_auto(first, side));
        assert!(groovestats_next_retry_remaining_secs(second, side).is_some());
        assert!(!groovestats_next_retry_is_auto(second, side));

        groovestats_record_submit_success(side, first);
        assert_eq!(groovestats_next_retry_remaining_secs(first, side), None);
        assert!(groovestats_next_retry_remaining_secs(second, side).is_some());

        groovestats_reset_submit_ui_status(side, first);
        groovestats_reset_submit_ui_status(side, second);
        groovestats_reset_submit_retry(side, first);
        groovestats_reset_submit_retry(side, second);
    }
}
