use super::{
    gameplay_run_failed, gameplay_run_passed, gameplay_side_for_player,
    get_or_fetch_player_leaderboards_for_side, invalidate_player_leaderboards_for_side,
    lua_chart_submit_allowed, submit_side_ix,
};
use crate::game::gameplay;
use crate::game::profile;
use deadsync_core::{input::MAX_PLAYERS, note::NoteType};
use deadsync_online::arrowcloud::{
    self as arrowcloud_api, ArrowCloudPayload, ArrowCloudRadar, ArrowCloudSubmitRequestError,
    ArrowCloudTimingDatum,
};
use deadsync_online::groovestats::GROOVESTATS_SUBMIT_MAX_ENTRIES;
use deadsync_profile as profile_data;
use deadsync_rules::{
    judgment,
    note::{HoldResult, MineResult, Note},
    timing,
};
use deadsync_score::{
    ArrowCloudSubmitUiStatus, SUBMIT_RETRY_MAX_ATTEMPTS, duration_to_ceil_secs,
    submit_retry_delay_secs,
};
use log::{debug, warn};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

const ARROWCLOUD_BODY_VERSION: &str = "1.4";
const ARROWCLOUD_ENGINE_NAME: &str = "DeadSync";
const ARROWCLOUD_ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
struct ArrowCloudSubmitUiEntry {
    chart_hash: String,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
}

static ARROWCLOUD_SUBMIT_UI_STATUS: std::sync::LazyLock<Mutex<[Vec<ArrowCloudSubmitUiEntry>; 2]>> =
    std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| Vec::new())));
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

static ARROWCLOUD_SUBMIT_RETRY: std::sync::LazyLock<Mutex<[Vec<ArrowCloudSubmitRetryEntry>; 2]>> =
    std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| Vec::new())));

const ARROWCLOUD_SUBMIT_RETRY_TRACKED_PER_SIDE: usize = 128;

#[inline(always)]
fn arrowcloud_trim_submit_retry_entries(entries: &mut Vec<ArrowCloudSubmitRetryEntry>) {
    if entries.len() > ARROWCLOUD_SUBMIT_RETRY_TRACKED_PER_SIDE {
        entries.drain(0..entries.len() - ARROWCLOUD_SUBMIT_RETRY_TRACKED_PER_SIDE);
    }
}

#[inline(always)]
fn arrowcloud_reset_submit_ui_status(side: profile_data::PlayerSide, chart_hash: &str) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let mut state = ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap();
    state[submit_side_ix(side)].retain(|entry| !entry.chart_hash.eq_ignore_ascii_case(hash));
}

#[inline(always)]
fn arrowcloud_reset_submit_retry(side: profile_data::PlayerSide, chart_hash: &str) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let mut state = ARROWCLOUD_SUBMIT_RETRY.lock().unwrap();
    state[submit_side_ix(side)].retain(|entry| !entry.payload.hash.eq_ignore_ascii_case(hash));
}

#[inline(always)]
fn arrowcloud_set_submit_ui_status(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let mut state = ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap();
    let entries = &mut state[submit_side_ix(side)];
    if let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
    {
        entry.token = token;
        entry.status = status;
        return;
    }
    entries.push(ArrowCloudSubmitUiEntry {
        chart_hash: hash.to_string(),
        token,
        status,
    });
}

#[inline(always)]
fn arrowcloud_update_submit_ui_status_if_token(
    side: profile_data::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
) -> bool {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return false;
    }
    let mut state = ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap();
    let Some(entry) = state[submit_side_ix(side)]
        .iter_mut()
        .find(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
    else {
        return false;
    };
    if entry.token != token {
        return false;
    }
    entry.status = status;
    true
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
    let hash = entry.payload.hash.trim();
    if hash.is_empty() {
        return;
    }
    let side = entry.side;
    let mut state = ARROWCLOUD_SUBMIT_RETRY.lock().unwrap();
    let entries = &mut state[submit_side_ix(side)];
    if let Some(stored) = entries
        .iter_mut()
        .find(|stored| stored.payload.hash.eq_ignore_ascii_case(hash))
    {
        *stored = entry;
        return;
    }
    entries.push(entry);
    arrowcloud_trim_submit_retry_entries(entries);
}

pub fn get_arrowcloud_submit_ui_status_for_side(
    chart_hash: &str,
    side: profile_data::PlayerSide,
) -> Option<ArrowCloudSubmitUiStatus> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap()[submit_side_ix(side)]
        .iter()
        .find(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
        .map(|entry| entry.status)
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
    gs: &gameplay::State,
    player_idx: usize,
) -> Vec<arrowcloud_api::ArrowCloudLifePoint> {
    let life_history = gs.players[player_idx].life_history.as_slice();
    let (start, end) = gs.note_ranges[player_idx];
    let note_times = &gs.note_time_cache_ns[start..end];
    let first_second = gs.density_graph_first_second.min(0.0);
    let last_second = gs.density_graph_last_second.max(first_second);
    let chart_start_second = note_times
        .iter()
        .find(|&&t| !gameplay::song_time_ns_invalid(t))
        .copied()
        .map(gameplay::song_time_ns_to_seconds)
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
    gs: &gameplay::State,
    player_idx: usize,
    fail_time_ns: Option<i64>,
) -> Vec<ArrowCloudTimingDatum> {
    let (start, end) = gs.note_ranges[player_idx];
    let notes = &gs.notes[start..end];
    let note_times = &gs.note_time_cache_ns[start..end];
    let col_offset = player_idx.saturating_mul(gs.cols_per_player);
    let stream_segments = gameplay::stream_segments_for_results(gs, player_idx);
    let scatter = timing::build_scatter_points(
        notes,
        note_times,
        col_offset,
        gs.cols_per_player,
        &stream_segments,
    );
    let fail_time_s = fail_time_ns.map(gameplay::song_time_ns_to_seconds);
    arrowcloud_api::timing_data_from_scatter(&scatter, fail_time_s)
}

#[inline(always)]
fn arrowcloud_nps_info(
    gs: &gameplay::State,
    player_idx: usize,
) -> arrowcloud_api::ArrowCloudNpsInfo {
    let chart = gs.charts[player_idx].as_ref();
    let first_second = gs.density_graph_first_second.min(0.0);
    let last_second = gs.density_graph_last_second.max(first_second);
    arrowcloud_api::nps_info_from_measure_data(
        chart.max_nps,
        chart.measure_nps_vec.as_slice(),
        chart.measure_seconds_vec.as_slice(),
        first_second,
        last_second,
    )
}

#[derive(Clone, Copy, Debug, Default)]
struct ArrowCloudSubmitStats {
    judgment_counts: judgment::JudgeCounts,
    window_counts: timing::WindowCounts,
    holds_held: u32,
    mines_hit: u32,
    mines_avoided: u32,
    rolls_held: u32,
}

#[inline(always)]
fn arrowcloud_time_in_submit_window(time_ns: i64, fail_time_ns: Option<i64>) -> bool {
    match fail_time_ns {
        Some(fail_time) => !gameplay::song_time_ns_invalid(time_ns) && time_ns <= fail_time,
        None => true,
    }
}

fn arrowcloud_stats_from_results(
    notes: &[Note],
    note_times: &[i64],
    hold_end_times: &[Option<i64>],
    fail_time_ns: Option<i64>,
) -> ArrowCloudSubmitStats {
    let mut stats = ArrowCloudSubmitStats::default();
    let mut idx = 0usize;
    while idx < notes.len() {
        let row_index = notes[idx].row_index;
        let row_start = idx;
        let row_time = note_times.get(idx).copied().unwrap_or(i64::MIN);
        while idx < notes.len() && notes[idx].row_index == row_index {
            idx += 1;
        }
        if !arrowcloud_time_in_submit_window(row_time, fail_time_ns) {
            continue;
        }
        let Some(row_judgment) =
            judgment::aggregate_row_final_judgment(notes[row_start..idx].iter().filter_map(|n| {
                if n.is_fake || !n.can_be_judged || matches!(n.note_type, NoteType::Mine) {
                    None
                } else {
                    n.result.as_ref()
                }
            }))
        else {
            continue;
        };
        stats.judgment_counts[judgment::judge_grade_ix(row_judgment.grade)] =
            stats.judgment_counts[judgment::judge_grade_ix(row_judgment.grade)].saturating_add(1);
        judgment::add_judgment_to_window_counts(
            &mut stats.window_counts,
            row_judgment,
            timing::FA_PLUS_W0_MS,
        );
    }

    for (i, note) in notes.iter().enumerate() {
        if note.is_fake || !note.can_be_judged {
            continue;
        }
        let note_time = note_times.get(i).copied().unwrap_or(i64::MIN);
        match note.note_type {
            NoteType::Hold | NoteType::Roll => {
                let result_time = hold_end_times
                    .get(i)
                    .and_then(|time| *time)
                    .unwrap_or(note_time);
                if !arrowcloud_time_in_submit_window(result_time, fail_time_ns) {
                    continue;
                }
                if note.hold.as_ref().and_then(|h| h.result) == Some(HoldResult::Held) {
                    if note.note_type == NoteType::Hold {
                        stats.holds_held = stats.holds_held.saturating_add(1);
                    } else {
                        stats.rolls_held = stats.rolls_held.saturating_add(1);
                    }
                }
            }
            NoteType::Mine => {
                if !arrowcloud_time_in_submit_window(note_time, fail_time_ns) {
                    continue;
                }
                match note.mine_result {
                    Some(MineResult::Hit) => {
                        stats.mines_hit = stats.mines_hit.saturating_add(1);
                    }
                    Some(MineResult::Avoided) => {
                        stats.mines_avoided = stats.mines_avoided.saturating_add(1);
                    }
                    None => {}
                }
            }
            NoteType::Tap | NoteType::Lift | NoteType::Fake => {}
        }
    }

    stats
}

#[inline(always)]
fn arrowcloud_live_submit_stats(gs: &gameplay::State, player_idx: usize) -> ArrowCloudSubmitStats {
    let player = &gs.players[player_idx];
    ArrowCloudSubmitStats {
        judgment_counts: player.judgment_counts,
        window_counts: gs.live_window_counts[player_idx],
        holds_held: player.holds_held,
        mines_hit: player.mines_hit,
        mines_avoided: player.mines_avoided,
        rolls_held: player.rolls_held,
    }
}

#[inline(always)]
fn arrowcloud_submit_stats(
    gs: &gameplay::State,
    player_idx: usize,
    fail_time_ns: Option<i64>,
) -> ArrowCloudSubmitStats {
    let Some(fail_time_ns) = fail_time_ns else {
        return arrowcloud_live_submit_stats(gs, player_idx);
    };
    let (start, end) = gs.note_ranges[player_idx];
    arrowcloud_stats_from_results(
        &gs.notes[start..end],
        &gs.note_time_cache_ns[start..end],
        &gs.hold_end_time_cache_ns[start..end],
        Some(fail_time_ns),
    )
}

#[inline(always)]
fn arrowcloud_payload_for_player(
    gs: &gameplay::State,
    player_idx: usize,
) -> Option<ArrowCloudPayload> {
    if player_idx >= gs.num_players {
        return None;
    }
    let chart = gs.charts[player_idx].as_ref();
    let profile = &gs.player_profiles[player_idx];
    let player = &gs.players[player_idx];
    let fail_time_ns = player.fail_time.map(gameplay::song_time_ns_from_seconds);
    let submit_stats = arrowcloud_submit_stats(gs, player_idx, fail_time_ns);
    let pack = gs.pack_group.trim().to_string();
    let song_name = gs.song.display_full_title(true);
    let music_rate = if gs.music_rate.is_finite() && gs.music_rate > 0.0 {
        gs.music_rate as f64
    } else {
        1.0
    };
    let passed = !gameplay_run_failed(player.is_failing, player.fail_time.is_some());

    Some(ArrowCloudPayload {
        song_name,
        artist: gs.song.artist.clone(),
        pack,
        length: arrowcloud_api::format_length(gs.song.music_length_seconds),
        hash: chart.short_hash.clone(),
        timing_data: arrowcloud_timing_data(gs, player_idx, fail_time_ns),
        difficulty: chart.meter,
        stepartist: chart.step_artist.clone(),
        radar: ArrowCloudRadar {
            holds: [submit_stats.holds_held, gs.holds_total[player_idx]],
            mines: [submit_stats.mines_avoided, gs.mines_total[player_idx]],
            rolls: [submit_stats.rolls_held, gs.rolls_total[player_idx]],
        },
        judgment_counts: arrowcloud_api::judgment_counts_from_stats(
            submit_stats.judgment_counts,
            submit_stats.window_counts,
            submit_stats.holds_held,
            gs.holds_total[player_idx],
            submit_stats.mines_hit,
            gs.mines_total[player_idx],
            submit_stats.rolls_held,
            gs.rolls_total[player_idx],
        ),
        nps_info: arrowcloud_nps_info(gs, player_idx),
        lifebar_info: arrowcloud_lifebar_points(gs, player_idx),
        modifiers: arrowcloud_api::modifiers_from_profile(profile),
        music_rate,
        used_autoplay: gs.autoplay_used,
        passed,
        body_version: ARROWCLOUD_BODY_VERSION,
        arrow_cloud_body_version: ARROWCLOUD_BODY_VERSION,
        engine_name: ARROWCLOUD_ENGINE_NAME,
        engine_version: ARROWCLOUD_ENGINE_VERSION,
    })
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

pub fn submit_arrowcloud_payloads_from_gameplay(gs: &gameplay::State) {
    for player_idx in 0..gs.num_players.min(MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        let chart_hash = gs.charts[player_idx].short_hash.as_str();
        arrowcloud_reset_submit_ui_status(side, chart_hash);
        arrowcloud_reset_submit_retry(side, chart_hash);
    }

    let cfg = crate::config::get();
    if !cfg.enable_arrowcloud || gs.num_players == 0 {
        return;
    }
    if gs.autoplay_used {
        debug!("Skipping ArrowCloud submit: autoplay/replay was used.");
        return;
    }
    if gs.course_display_totals.is_some() && !cfg.autosubmit_course_scores_individually {
        debug!("Skipping ArrowCloud submit: course per-song autosubmit is disabled.");
        return;
    }
    let mut jobs = Vec::with_capacity(gs.num_players.min(MAX_PLAYERS));
    for player_idx in 0..gs.num_players.min(MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        let chart_hash = gs.charts[player_idx].short_hash.as_str();
        if gs.song.has_lua && !lua_chart_submit_allowed(chart_hash) {
            debug!(
                "Skipping ArrowCloud submit for {:?} ({}): simfile relies on lua.",
                side, chart_hash
            );
            continue;
        }
        let failed = gameplay_run_failed(
            gs.players[player_idx].is_failing,
            gs.players[player_idx].fail_time.is_some(),
        );
        let passed = gameplay_run_passed(
            gs.song_completed_naturally,
            gs.players[player_idx].is_failing,
            gs.players[player_idx].life,
            gs.players[player_idx].fail_time.is_some(),
        );
        let allow_failed_submit = failed && cfg.submit_arrowcloud_fails;
        let finished = gs.song_completed_naturally || failed;
        let api_key = gs.player_profiles[player_idx].arrowcloud_api_key.trim();
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
        if !gameplay::course_stage_life_submit_eligible(gs, player_idx) && !allow_failed_submit {
            arrowcloud_warn_submit_skip(
                side,
                chart_hash,
                "course stage would have failed from normal life",
            );
            continue;
        }
        let Some(payload) = arrowcloud_payload_for_player(gs, player_idx) else {
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
        let itg_percent = gameplay::display_itg_score_percent(gs, player_idx).clamp(0.0, 100.0);
        let ex_percent = gameplay::display_ex_score_percent(gs, player_idx).clamp(0.0, 100.0);
        let hard_ex_percent =
            gameplay::display_hard_ex_score_percent(gs, player_idx).clamp(0.0, 100.0);

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
        let Some(stored) = lock[submit_side_ix(side)]
            .iter_mut()
            .find(|entry| entry.payload.hash.eq_ignore_ascii_case(hash))
        else {
            return false;
        };
        // Manual fires are gated by the cooldown — refuse if it hasn't
        // elapsed. Auto fires (driven by tick) are already filtered by the
        // schedule, so they bypass this gate.
        if manual && let Some(t) = stored.next_retry_at {
            if t > Instant::now() {
                return false;
            }
        }
        stored.next_retry_at = None;
        stored.clone()
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
    let Some(entry) = lock[submit_side_ix(side)]
        .iter_mut()
        .find(|entry| entry.payload.hash.eq_ignore_ascii_case(chart_hash))
    else {
        return;
    };
    if !status.can_retry() {
        entry.next_retry_at = None;
        return;
    }
    entry.retry_attempt = entry
        .retry_attempt
        .saturating_add(1)
        .min(ARROWCLOUD_RETRY_MAX_ATTEMPTS);
    let delay = submit_retry_delay_secs(entry.retry_attempt);
    entry.next_retry_at = Some(Instant::now() + Duration::from_secs(delay));
}

/// Clears retry/backoff bookkeeping after a successful submit. Called from the
/// worker's success path when the status update was accepted.
fn arrowcloud_record_submit_success(side: profile_data::PlayerSide, chart_hash: &str) {
    let mut lock = ARROWCLOUD_SUBMIT_RETRY.lock().unwrap();
    lock[submit_side_ix(side)].retain(|entry| !entry.payload.hash.eq_ignore_ascii_case(chart_hash));
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
    let target = lock[submit_side_ix(side)]
        .iter()
        .find(|entry| entry.payload.hash.eq_ignore_ascii_case(hash))?
        .next_retry_at?;
    Some(duration_to_ceil_secs(
        target.saturating_duration_since(Instant::now()),
    ))
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
        let Some(entry) = lock[submit_side_ix(side)]
            .iter()
            .find(|entry| entry.payload.hash.eq_ignore_ascii_case(hash))
        else {
            return false;
        };
        entry.retry_attempt
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
        let now = Instant::now();
        lock.iter()
            .flat_map(|entries| entries.iter())
            .filter_map(|entry| {
                entry
                    .next_retry_at
                    .filter(|t| *t <= now)
                    .map(|_| (entry.payload.hash.clone(), entry.side, entry.retry_attempt))
            })
            .collect()
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
        ArrowCloudJudgmentCounts, ArrowCloudModifiers, ArrowCloudNpsInfo, ArrowCloudSpeed,
        ArrowCloudTimingOffset,
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

    fn sample_judgment(grade: judgment::JudgeGrade, offset_ms: f32) -> judgment::Judgment {
        judgment::Judgment {
            time_error_ms: offset_ms,
            time_error_music_ns: 0,
            grade,
            window: None,
            miss_because_held: false,
        }
    }

    fn sample_note(
        row_index: usize,
        note_type: NoteType,
        result: Option<judgment::Judgment>,
        hold_result: Option<HoldResult>,
        mine_result: Option<MineResult>,
    ) -> Note {
        let hold = if matches!(note_type, NoteType::Hold | NoteType::Roll) {
            Some(deadsync_rules::note::HoldData {
                end_row_index: row_index,
                end_beat: row_index as f32,
                result: hold_result,
                life: 1.0,
                let_go_started_at: None,
                let_go_starting_life: 0.0,
                last_held_row_index: row_index,
                last_held_beat: row_index as f32,
            })
        } else {
            None
        };
        Note {
            beat: row_index as f32,
            quantization_idx: 0,
            column: row_index % 4,
            note_type,
            row_index,
            result,
            early_result: None,
            hold,
            mine_result,
            is_fake: false,
            can_be_judged: true,
        }
    }

    fn sample_payload(hash: &str) -> ArrowCloudPayload {
        ArrowCloudPayload {
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
            body_version: ARROWCLOUD_BODY_VERSION,
            arrow_cloud_body_version: ARROWCLOUD_BODY_VERSION,
            engine_name: ARROWCLOUD_ENGINE_NAME,
            engine_version: ARROWCLOUD_ENGINE_VERSION,
        }
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
    fn arrowcloud_submit_stats_caps_failed_runs_at_fail_time() {
        let ns = gameplay::song_time_ns_from_seconds;
        let notes = vec![
            sample_note(
                0,
                NoteType::Tap,
                Some(sample_judgment(judgment::JudgeGrade::Fantastic, 5.0)),
                None,
                None,
            ),
            sample_note(
                1,
                NoteType::Hold,
                Some(sample_judgment(judgment::JudgeGrade::Fantastic, 8.0)),
                Some(HoldResult::Held),
                None,
            ),
            sample_note(
                2,
                NoteType::Roll,
                Some(sample_judgment(judgment::JudgeGrade::Fantastic, 10.0)),
                Some(HoldResult::Held),
                None,
            ),
            sample_note(3, NoteType::Mine, None, None, Some(MineResult::Hit)),
            sample_note(
                4,
                NoteType::Tap,
                Some(sample_judgment(judgment::JudgeGrade::Miss, 180.0)),
                None,
                None,
            ),
            sample_note(
                5,
                NoteType::Tap,
                Some(sample_judgment(judgment::JudgeGrade::Great, 55.0)),
                None,
                None,
            ),
            sample_note(6, NoteType::Mine, None, None, Some(MineResult::Hit)),
        ];
        let note_times = vec![
            ns(1.0),
            ns(1.2),
            ns(1.4),
            ns(1.5),
            ns(2.0),
            ns(3.0),
            ns(3.5),
        ];
        let hold_end_times = vec![None, Some(ns(1.8)), Some(ns(2.4)), None, None, None, None];

        let stats =
            arrowcloud_stats_from_results(&notes, &note_times, &hold_end_times, Some(ns(2.0)));

        assert_eq!(
            stats.judgment_counts[judgment::judge_grade_ix(judgment::JudgeGrade::Fantastic)],
            3
        );
        assert_eq!(
            stats.judgment_counts[judgment::judge_grade_ix(judgment::JudgeGrade::Miss)],
            1
        );
        assert_eq!(
            stats.judgment_counts[judgment::judge_grade_ix(judgment::JudgeGrade::Great)],
            0
        );
        assert_eq!(stats.window_counts.w0, 3);
        assert_eq!(stats.window_counts.miss, 1);
        assert_eq!(stats.holds_held, 1);
        assert_eq!(stats.rolls_held, 0);
        assert_eq!(stats.mines_hit, 1);
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
