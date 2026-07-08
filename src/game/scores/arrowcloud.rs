use super::{
    GameplayCoreState, gameplay_run_failed, gameplay_run_passed, gameplay_side_for_player,
    get_or_fetch_player_leaderboards_for_side, invalidate_player_leaderboards_for_side,
    lua_chart_submit_allowed,
};
use crate::game::profile;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{FantasticWindowOptions, blue_fantastic_window_ms};
use deadsync_online::arrowcloud::{
    self as arrowcloud_api, ArrowCloudPayload, ArrowCloudPayloadParts,
    ArrowCloudSubmitRequestError, ArrowCloudTimingDatum,
};
use deadsync_online::groovestats::GROOVESTATS_SUBMIT_MAX_ENTRIES;
use deadsync_profile as profile_data;
use deadsync_rules::timing;
use deadsync_score::{
    ArrowCloudSubmitStats, ArrowCloudSubmitUiStatus, SUBMIT_RETRY_MAX_ATTEMPTS, SubmitRetryState,
    SubmitUiState, arrowcloud_submit_stats_from_results,
};
use log::{debug, warn};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::Instant;

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

static ARROWCLOUD_SUBMIT_UI_STATUS: std::sync::LazyLock<
    Mutex<SubmitUiState<ArrowCloudSubmitUiStatus>>,
> = std::sync::LazyLock::new(|| Mutex::new(SubmitUiState::default()));
static ARROWCLOUD_SUBMIT_UI_TOKEN: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
struct ArrowCloudSubmitRetryEntry {
    side: profile_data::PlayerSide,
    api_key: String,
    payload: ArrowCloudPayload,
    profile_id: Option<String>,
    itg_percent: f64,
    ex_percent: f64,
    hard_ex_percent: f64,
    is_fail: bool,
    /// Consecutive failures, capped at `SUBMIT_RETRY_MAX_ATTEMPTS`. Drives
    /// the shared backoff schedule so mixed failure kinds keep ratcheting the
    /// same curve. Reset only on a successful submit.
    retry_attempt: u8,
    /// When the next retry is allowed (manual cooldown) or scheduled (auto).
    /// `None` means no gate and no auto-retry pending. The tick only fires
    /// when the current UI status is auto-retryable; for manual-only
    /// statuses this field acts purely as a cooldown gate.
    next_retry_at: Option<Instant>,
}

/// Maximum number of attempts before the backoff schedule saturates.
/// For *auto-retryable* statuses this is also the auto-retry budget. For
/// *manual-only* statuses the cooldown caps at `delay(MAX)`.
/// Alias of the shared [`SUBMIT_RETRY_MAX_ATTEMPTS`].
const ARROWCLOUD_RETRY_MAX_ATTEMPTS: u8 = SUBMIT_RETRY_MAX_ATTEMPTS;

static ARROWCLOUD_SUBMIT_RETRY: std::sync::LazyLock<
    Mutex<SubmitRetryState<ArrowCloudSubmitRetryEntry>>,
> = std::sync::LazyLock::new(|| Mutex::new(SubmitRetryState::default()));

const ARROWCLOUD_SUBMIT_RETRY_TRACKED_PER_SIDE: usize = 128;

#[inline(always)]
fn arrowcloud_reset_submit_ui_status(side: profile_data::PlayerSide, chart_hash: &str) {
    ARROWCLOUD_SUBMIT_UI_STATUS
        .lock()
        .unwrap()
        .reset(profile_data::player_side_index(side), chart_hash);
}

#[inline(always)]
fn arrowcloud_reset_submit_retry(side: profile_data::PlayerSide, chart_hash: &str) {
    ARROWCLOUD_SUBMIT_RETRY.lock().unwrap().reset_by_key(
        profile_data::player_side_index(side),
        chart_hash,
        |entry| entry.payload.hash.as_str(),
    );
}

#[inline(always)]
fn arrowcloud_set_submit_ui_status(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
) {
    ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap().set(
        profile_data::player_side_index(side),
        chart_hash,
        token,
        status,
    );
}

#[inline(always)]
fn arrowcloud_update_submit_ui_status_if_token(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
) -> bool {
    ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap().update_if_token(
        profile_data::player_side_index(side),
        chart_hash,
        token,
        status,
    )
}

#[inline(always)]
fn arrowcloud_next_submit_ui_token() -> u64 {
    ARROWCLOUD_SUBMIT_UI_TOKEN.fetch_add(1, AtomicOrdering::Relaxed)
}

#[inline(always)]
fn arrowcloud_warn_submit_skip(side: profile_data::PlayerSide, chart_hash: &str, reason: &str) {
    warn!(
        "Skipping ArrowCloud submit for {:?} ({}): {}.",
        side, chart_hash, reason
    );
}

#[inline(always)]
fn arrowcloud_store_submit_retry(entry: ArrowCloudSubmitRetryEntry) {
    let side = entry.side;
    ARROWCLOUD_SUBMIT_RETRY.lock().unwrap().upsert_by_key(
        profile_data::player_side_index(side),
        entry,
        |entry| entry.payload.hash.as_str(),
        ARROWCLOUD_SUBMIT_RETRY_TRACKED_PER_SIDE,
    );
}

pub fn get_arrowcloud_submit_ui_status_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<ArrowCloudSubmitUiStatus> {
    ARROWCLOUD_SUBMIT_UI_STATUS
        .lock()
        .unwrap()
        .get(profile_data::player_side_index(side), chart_hash)
}

#[derive(Debug)]
struct ArrowCloudSubmitJob {
    side: profile_data::PlayerSide,
    api_key: String,
    token: u64,
    payload: ArrowCloudPayload,
    /// Active local profile id whose AC cache should be updated on submit
    /// success. `None` if the submitting side is in Guest mode.
    profile_id: Option<String>,
    /// Gameplay-computed score percents (0..=100) captured at job creation
    /// time so we can populate the AC cache without round-tripping through
    /// the server response.
    itg_percent: f64,
    ex_percent: f64,
    hard_ex_percent: f64,
    is_fail: bool,
}

#[derive(Debug)]
struct ArrowCloudSubmitError {
    status: ArrowCloudSubmitUiStatus,
    message: String,
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

#[inline(always)]
fn submit_arrowcloud_payload(
    side: profile_data::PlayerSide,
    api_key: &str,
    payload: &ArrowCloudPayload,
) -> Result<(), ArrowCloudSubmitError> {
    match arrowcloud_api::submit_score_request(api_key, payload) {
        Ok(success) => {
            if success.body_snippet.is_empty() {
                debug!(
                    "ArrowCloud submit success for {:?} ({}) status={}",
                    side, payload.hash, success.status
                );
            } else {
                debug!(
                    "ArrowCloud submit success for {:?} ({}) status={} body='{}'",
                    side,
                    payload.hash,
                    success.status,
                    success.body_snippet.as_str()
                );
            }
            Ok(())
        }
        Err(error) => Err(arrowcloud_submit_error_from_online(error)),
    }
}

fn arrowcloud_submit_error_from_online(
    error: ArrowCloudSubmitRequestError,
) -> ArrowCloudSubmitError {
    let (status, message) = arrowcloud_api::submit_error_status_and_message(&error);
    ArrowCloudSubmitError { status, message }
}

fn spawn_arrowcloud_submit_jobs(jobs: Vec<ArrowCloudSubmitJob>) {
    std::thread::spawn(move || {
        for job in jobs {
            match submit_arrowcloud_payload(job.side, &job.api_key, &job.payload) {
                Ok(()) => {
                    let accepted = arrowcloud_update_submit_ui_status_if_token(
                        job.side,
                        job.payload.hash.as_str(),
                        job.token,
                        ArrowCloudSubmitUiStatus::Submitted,
                    );
                    if accepted {
                        arrowcloud_record_submit_success(job.side, job.payload.hash.as_str());
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
                }
                Err(err) => {
                    let accepted = arrowcloud_update_submit_ui_status_if_token(
                        job.side,
                        job.payload.hash.as_str(),
                        job.token,
                        err.status,
                    );
                    warn!(
                        "ArrowCloud submit failed for {:?} ({}) status={:?}: {}",
                        job.side, job.payload.hash, err.status, err.message
                    );
                    if accepted {
                        arrowcloud_record_submit_failure(
                            job.side,
                            job.payload.hash.as_str(),
                            err.status,
                        );
                    }
                }
            }
            invalidate_player_leaderboards_for_side(job.payload.hash.as_str(), job.side);
            get_or_fetch_player_leaderboards_for_side(
                job.payload.hash.as_str(),
                job.side,
                GROOVESTATS_SUBMIT_MAX_ENTRIES,
            );
        }
    });
}

pub fn submit_arrowcloud_payloads_from_gameplay(gs: &GameplayCoreState, pack_group: &str) {
    for player_idx in 0..gs.num_players().min(MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        let chart_hash = gs.charts()[player_idx].short_hash.as_str();
        arrowcloud_reset_submit_ui_status(side, chart_hash);
        arrowcloud_reset_submit_retry(side, chart_hash);
    }

    let cfg = crate::config::get();
    if !cfg.enable_arrowcloud || gs.num_players() == 0 {
        return;
    }
    if gs.autoplay_used() {
        debug!("Skipping ArrowCloud submit: autoplay/replay was used.");
        return;
    }
    if gs.course_display_is_course_stage() && !cfg.autosubmit_course_scores_individually {
        debug!("Skipping ArrowCloud submit: course per-song autosubmit is disabled.");
        return;
    }
    let mut jobs = Vec::with_capacity(gs.num_players().min(MAX_PLAYERS));
    for player_idx in 0..gs.num_players().min(MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        let chart_hash = gs.charts()[player_idx].short_hash.as_str();
        if gs.song().has_lua && !lua_chart_submit_allowed(chart_hash) {
            debug!(
                "Skipping ArrowCloud submit for {:?} ({}): simfile relies on lua.",
                side, chart_hash
            );
            continue;
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
        let allow_failed_submit = failed && cfg.submit_arrowcloud_fails;
        let finished = gs.song_completed_naturally() || failed;
        let api_key = gs.profiles()[player_idx].arrowcloud_api_key.trim();
        if !finished {
            debug!(
                "Skipping ArrowCloud submit for {:?} ({}): stage was not completed.",
                side, chart_hash
            );
            continue;
        }
        if api_key.is_empty() {
            if passed || allow_failed_submit {
                arrowcloud_warn_submit_skip(side, chart_hash, "profile is missing API key");
            }
            continue;
        }
        if !gs.course_stage_life_submit_eligible(player_idx) && !allow_failed_submit {
            arrowcloud_warn_submit_skip(
                side,
                chart_hash,
                "course stage would have failed from normal life",
            );
            continue;
        }
        let Some(payload) = arrowcloud_payload_for_player(gs, player_idx, pack_group) else {
            arrowcloud_warn_submit_skip(side, chart_hash, "failed to build submit payload");
            continue;
        };
        if failed && !allow_failed_submit {
            debug!(
                "Skipping ArrowCloud submit for {:?} ({}): failed-stage submits are disabled.",
                side, payload.hash
            );
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

        arrowcloud_store_submit_retry(ArrowCloudSubmitRetryEntry {
            side,
            api_key: api_key.to_string(),
            payload: payload.clone(),
            profile_id: profile_id.clone(),
            itg_percent,
            ex_percent,
            hard_ex_percent,
            is_fail: failed,
            retry_attempt: 0,
            next_retry_at: None,
        });
        let token = arrowcloud_next_submit_ui_token();
        arrowcloud_set_submit_ui_status(
            side,
            payload.hash.as_str(),
            token,
            ArrowCloudSubmitUiStatus::Submitting,
        );
        jobs.push(ArrowCloudSubmitJob {
            side,
            api_key: api_key.to_string(),
            token,
            payload,
            profile_id,
            itg_percent,
            ex_percent,
            hard_ex_percent,
            is_fail: failed,
        });
    }
    if jobs.is_empty() {
        return;
    }

    spawn_arrowcloud_submit_jobs(jobs);
}

pub fn retry_arrowcloud_submit(chart_hash: &str, side: profile_data::PlayerSide) -> bool {
    retry_arrowcloud_submit_inner(chart_hash, side, true)
}

fn retry_arrowcloud_submit_inner(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    manual: bool,
) -> bool {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return false;
    }
    let cfg = crate::config::get();
    if !cfg.enable_arrowcloud {
        return false;
    }
    let Some(status) = get_arrowcloud_submit_ui_status_for_side(hash, side) else {
        return false;
    };
    if !status.can_retry() {
        return false;
    }
    let entry = {
        let mut lock = ARROWCLOUD_SUBMIT_RETRY.lock().unwrap();
        let Some(stored) = lock.take_ready_by_key(
            profile_data::player_side_index(side),
            hash,
            manual,
            Instant::now(),
            |entry| entry.payload.hash.as_str(),
            |entry| &mut entry.next_retry_at,
        ) else {
            return false;
        };
        // Manual fires are gated by the cooldown — refuse if it hasn't
        // elapsed. Auto fires (driven by tick) are already filtered by the
        // schedule, so they bypass this gate.
        stored
    };

    let token = arrowcloud_next_submit_ui_token();
    arrowcloud_set_submit_ui_status(side, hash, token, ArrowCloudSubmitUiStatus::Submitting);
    debug!("Retrying ArrowCloud submit for {:?} ({}).", side, hash);
    spawn_arrowcloud_submit_jobs(vec![ArrowCloudSubmitJob {
        side: entry.side,
        api_key: entry.api_key,
        token,
        payload: entry.payload,
        profile_id: entry.profile_id,
        itg_percent: entry.itg_percent,
        ex_percent: entry.ex_percent,
        hard_ex_percent: entry.hard_ex_percent,
        is_fail: entry.is_fail,
    }]);
    true
}

/// Updates the retry entry's backoff schedule based on a worker-reported
/// failure. Only call after the UI status update was accepted (token still
/// matched), so stale results from superseded requests cannot re-arm.
///
/// Every retryable failure — auto or manual — advances the same shared
/// `retry_attempt` counter, so mixed failure kinds (e.g., timeout → 5xx →
/// timeout) keep ratcheting along the same exponential curve instead of
/// each kind walking its own track. Auto-firing is gated on the current
/// status being [`ArrowCloudSubmitUiStatus::is_auto_retryable`] AND
/// `retry_attempt <= MAX_ATTEMPTS`; otherwise `next_retry_at` acts purely
/// as a manual F5 cooldown gate.
fn arrowcloud_record_submit_failure(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    status: ArrowCloudSubmitUiStatus,
) {
    let mut lock = ARROWCLOUD_SUBMIT_RETRY.lock().unwrap();
    lock.record_failure_by_key(
        profile_data::player_side_index(side),
        chart_hash,
        status.can_retry(),
        ARROWCLOUD_RETRY_MAX_ATTEMPTS,
        Instant::now(),
        |entry| entry.payload.hash.as_str(),
        |entry| &mut entry.retry_attempt,
        |entry| &mut entry.next_retry_at,
    );
}

/// Clears retry/backoff bookkeeping after a successful submit. Called from the
/// worker's success path when the status update was accepted.
fn arrowcloud_record_submit_success(side: profile_data::PlayerSide, chart_hash: &str) {
    arrowcloud_reset_submit_retry(side, chart_hash);
}

/// Returns the seconds remaining until the next retry is allowed (manual
/// cooldown) or scheduled (auto). `Some(0)` means due-to-fire / gate just
/// elapsed. `None` means no gate is currently armed (bare `F5 Retry`).
pub fn arrowcloud_next_retry_remaining_secs(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<u32> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    let lock = ARROWCLOUD_SUBMIT_RETRY.lock().unwrap();
    lock.remaining_secs_by_key(
        profile_data::player_side_index(side),
        hash,
        Instant::now(),
        |entry| entry.payload.hash.as_str(),
        |entry| entry.next_retry_at,
    )
}

/// Returns true when the next scheduled retry will be fired automatically by
/// the tick driver. When false, any pending `next_retry_at` is acting purely
/// as a manual F5 cooldown gate.
pub fn arrowcloud_next_retry_is_auto(chart_hash: &str, side: profile_data::PlayerSide) -> bool {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return false;
    }
    let attempt = {
        let lock = ARROWCLOUD_SUBMIT_RETRY.lock().unwrap();
        let Some(attempt) = lock.retry_attempt_by_key(
            profile_data::player_side_index(side),
            hash,
            |entry| entry.payload.hash.as_str(),
            |entry| entry.retry_attempt,
        ) else {
            return false;
        };
        attempt
    };
    if attempt >= ARROWCLOUD_RETRY_MAX_ATTEMPTS {
        return false;
    }
    matches!(
        get_arrowcloud_submit_ui_status_for_side(hash, side),
        Some(s) if s.is_auto_retryable()
    )
}

/// Fires any auto-retries whose scheduled time has elapsed. Only fires for
/// entries whose current UI status is auto-retryable (see
/// [`ArrowCloudSubmitUiStatus::is_auto_retryable`]) AND whose auto-retry budget
/// hasn't been exhausted; other retryable statuses (and exhausted entries)
/// use `next_retry_at` purely as a manual cooldown gate. Returns true if at
/// least one retry was fired.
pub fn tick_arrowcloud_auto_retries() -> bool {
    let due: Vec<(String, profile_data::PlayerSide, u8)> = {
        let lock = ARROWCLOUD_SUBMIT_RETRY.lock().unwrap();
        lock.due_retries(
            Instant::now(),
            |entry| entry.payload.hash.as_str(),
            |entry| entry.side,
            |entry| entry.retry_attempt,
            |entry| entry.next_retry_at,
        )
    };
    let mut fired = false;
    for (hash, side, attempt) in due {
        if attempt >= ARROWCLOUD_RETRY_MAX_ATTEMPTS {
            continue;
        }
        let Some(status) = get_arrowcloud_submit_ui_status_for_side(&hash, side) else {
            continue;
        };
        if status.is_auto_retryable() && retry_arrowcloud_submit_inner(&hash, side, false) {
            fired = true;
        }
    }
    fired
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_online::arrowcloud::{
        ArrowCloudJudgmentCounts, ArrowCloudModifiers, ArrowCloudNpsInfo, ArrowCloudRadar,
        ArrowCloudSpeed, ArrowCloudTimingOffset,
    };
    use deadsync_rules::timing::ScatterPoint;
    use serde_json::{Value, json};

    fn sample_scatter(time_sec: f32, offset_ms: Option<f32>) -> ScatterPoint {
        ScatterPoint {
            time_sec,
            offset_ms,
            direction_code: 1,
            is_stream: false,
            is_left_foot: false,
            miss_because_held: false,
        }
    }

    fn sample_payload(hash: &str) -> ArrowCloudPayload {
        let mut payload = ArrowCloudPayload {
            song_name: "Test Song".to_string(),
            artist: "Test Artist".to_string(),
            pack: "Test Pack".to_string(),
            length: "1:23".to_string(),
            hash: hash.to_string(),
            timing_data: vec![(24.488_208_770_752, ArrowCloudTimingOffset::Miss("Miss"))],
            difficulty: 12,
            stepartist: "Tester".to_string(),
            radar: ArrowCloudRadar {
                holds: [1, 2],
                mines: [3, 4],
                rolls: [5, 6],
            },
            judgment_counts: ArrowCloudJudgmentCounts {
                fantastic_plus: 10,
                fantastic: 20,
                excellent: 30,
                great: 40,
                decent: 50,
                way_off: 60,
                miss: 3,
                total_steps: 213,
                holds_held: 1,
                total_holds: 2,
                mines_hit: 3,
                total_mines: 4,
                rolls_held: 5,
                total_rolls: 6,
            },
            nps_info: ArrowCloudNpsInfo {
                peak_nps: 0.0,
                points: Vec::new(),
            },
            lifebar_info: Vec::new(),
            modifiers: ArrowCloudModifiers {
                visual_delay: 0,
                acceleration: Vec::new(),
                appearance: Vec::new(),
                effect: Vec::new(),
                mini: 0,
                turn: "None".to_string(),
                disabled_windows: "None".to_string(),
                speed: ArrowCloudSpeed {
                    value: 600.0,
                    speed_type: "C",
                },
                perspective: "Overhead".to_string(),
                noteskin: "cel".to_string(),
                scroll: None,
            },
            music_rate: 1.0,
            used_autoplay: false,
            passed: true,
            body_version: "",
            arrow_cloud_body_version: "",
            engine_name: "",
            engine_version: "",
        };
        payload.fill_metadata();
        payload
    }

    fn sample_retry_entry(
        hash: &str,
        side: profile_data::PlayerSide,
    ) -> ArrowCloudSubmitRetryEntry {
        ArrowCloudSubmitRetryEntry {
            side,
            api_key: "test-api-key".to_string(),
            payload: sample_payload(hash),
            profile_id: None,
            itg_percent: 99.0,
            ex_percent: 98.0,
            hard_ex_percent: 97.0,
            is_fail: false,
            retry_attempt: 0,
            next_retry_at: None,
        }
    }

    #[test]
    fn arrowcloud_timing_data_keeps_miss_rows() {
        let scatter = [
            sample_scatter(12.5, Some(8.0)),
            sample_scatter(12.75, None),
            sample_scatter(f32::NAN, Some(2.0)),
        ];
        let timing_data = arrowcloud_api::timing_data_from_scatter(&scatter, None);
        assert_eq!(timing_data.len(), 2);

        let value = serde_json::to_value(&timing_data).expect("serialize timingData");
        assert_eq!(value[0][0], json!(12.5));
        let first_offset = value[0][1]
            .as_f64()
            .expect("timingData[0][1] should be numeric");
        assert!((first_offset - 0.008).abs() < 1e-6);
        assert_eq!(value[1][0], json!(12.75));
        assert_eq!(value[1][1], json!("Miss"));
    }

    #[test]
    fn arrowcloud_timing_data_caps_failed_runs_at_fail_time() {
        let scatter = [
            sample_scatter(1.0, Some(8.0)),
            sample_scatter(2.0, None),
            sample_scatter(3.0, Some(12.0)),
        ];
        let timing_data = arrowcloud_api::timing_data_from_scatter(&scatter, Some(2.0));

        let value = serde_json::to_value(&timing_data).expect("serialize timingData");
        assert_eq!(value.as_array().map(Vec::len), Some(2));
        assert_eq!(value[0][0], json!(1.0));
        assert_eq!(value[1][0], json!(2.0));
        assert_eq!(value[1][1], json!("Miss"));
    }

    #[test]
    fn arrowcloud_payload_serializes_miss_and_counts() {
        let payload = sample_payload("deadbeefcafebabe");

        let value = serde_json::to_value(&payload).expect("serialize ArrowCloud payload");
        assert_eq!(value["timingData"][0][1], json!("Miss"));
        assert_eq!(value["judgmentCounts"]["miss"], json!(3));
        assert_eq!(value["judgmentCounts"]["wayOff"], json!(60));
        assert_eq!(value["bodyVersion"], Value::String("1.4".to_string()));
        assert_eq!(
            value["_arrowCloudBodyVersion"],
            Value::String("1.4".to_string())
        );
    }

    #[test]
    fn arrowcloud_run_passed_rejects_failed_runs() {
        assert!(gameplay_run_passed(true, false, 1.0, false));
        assert!(!gameplay_run_passed(false, false, 1.0, false));
        assert!(!gameplay_run_passed(true, true, 1.0, false));
        assert!(!gameplay_run_passed(true, false, 1.0, true));
        assert!(!gameplay_run_passed(true, false, 0.0, false));
    }

    #[test]
    fn arrowcloud_run_failed_uses_fail_signals_only() {
        assert!(!gameplay_run_failed(false, false));
        assert!(gameplay_run_failed(true, false));
        assert!(gameplay_run_failed(false, true));
    }

    #[test]
    fn arrowcloud_submit_ui_tracks_multiple_hashes_per_side() {
        let side = profile_data::PlayerSide::P1;
        let first = "ac-course-status-first";
        let second = "ac-course-status-second";
        arrowcloud_reset_submit_ui_status(side, first);
        arrowcloud_reset_submit_ui_status(side, second);

        arrowcloud_set_submit_ui_status(side, first, 11, ArrowCloudSubmitUiStatus::Submitting);
        arrowcloud_set_submit_ui_status(side, second, 12, ArrowCloudSubmitUiStatus::Submitted);

        assert_eq!(
            get_arrowcloud_submit_ui_status_for_side(first, side),
            Some(ArrowCloudSubmitUiStatus::Submitting)
        );
        assert_eq!(
            get_arrowcloud_submit_ui_status_for_side(second, side),
            Some(ArrowCloudSubmitUiStatus::Submitted)
        );
        assert!(arrowcloud_update_submit_ui_status_if_token(
            side,
            first,
            11,
            ArrowCloudSubmitUiStatus::TimedOut,
        ));
        assert!(!arrowcloud_update_submit_ui_status_if_token(
            side,
            first,
            12,
            ArrowCloudSubmitUiStatus::Submitted,
        ));
        assert_eq!(
            get_arrowcloud_submit_ui_status_for_side(first, side),
            Some(ArrowCloudSubmitUiStatus::TimedOut)
        );
        assert_eq!(
            get_arrowcloud_submit_ui_status_for_side(second, side),
            Some(ArrowCloudSubmitUiStatus::Submitted)
        );

        arrowcloud_reset_submit_ui_status(side, first);
        arrowcloud_reset_submit_ui_status(side, second);
    }

    #[test]
    fn arrowcloud_submit_retry_tracks_multiple_hashes_per_side() {
        let side = profile_data::PlayerSide::P1;
        let first = "ac-course-retry-first";
        let second = "ac-course-retry-second";
        arrowcloud_reset_submit_ui_status(side, first);
        arrowcloud_reset_submit_ui_status(side, second);
        arrowcloud_reset_submit_retry(side, first);
        arrowcloud_reset_submit_retry(side, second);

        arrowcloud_store_submit_retry(sample_retry_entry(first, side));
        arrowcloud_store_submit_retry(sample_retry_entry(second, side));
        arrowcloud_set_submit_ui_status(side, first, 21, ArrowCloudSubmitUiStatus::TimedOut);
        arrowcloud_set_submit_ui_status(side, second, 22, ArrowCloudSubmitUiStatus::NetworkError);

        arrowcloud_record_submit_failure(side, first, ArrowCloudSubmitUiStatus::TimedOut);
        arrowcloud_record_submit_failure(side, second, ArrowCloudSubmitUiStatus::NetworkError);

        assert!(arrowcloud_next_retry_remaining_secs(first, side).is_some());
        assert!(arrowcloud_next_retry_is_auto(first, side));
        assert!(arrowcloud_next_retry_remaining_secs(second, side).is_some());
        assert!(!arrowcloud_next_retry_is_auto(second, side));

        arrowcloud_record_submit_success(side, first);
        assert_eq!(arrowcloud_next_retry_remaining_secs(first, side), None);
        assert!(arrowcloud_next_retry_remaining_secs(second, side).is_some());

        arrowcloud_reset_submit_ui_status(side, first);
        arrowcloud_reset_submit_ui_status(side, second);
        arrowcloud_reset_submit_retry(side, first);
        arrowcloud_reset_submit_retry(side, second);
    }
}
