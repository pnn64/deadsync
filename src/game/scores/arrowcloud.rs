use super::{
    gameplay_run_failed, gameplay_run_passed, gameplay_side_for_player,
    invalidate_player_leaderboards_for_side, log_body_snippet, submit_side_ix,
};
use crate::engine::network;
use crate::game::gameplay;
use crate::game::judgment;
use crate::game::online;
use crate::game::profile::{self, Profile};
use log::{debug, warn};
use serde::Serialize;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

const ARROWCLOUD_BODY_VERSION: &str = "1.4";
const ARROWCLOUD_ENGINE_NAME: &str = "DeadSync";
const ARROWCLOUD_ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const ARROWCLOUD_LIFEBAR_POINTS: usize = 100;
const ARROWCLOUD_ACCEL_NAMES: [&str; 5] = ["Boost", "Brake", "Wave", "Expand", "Boomerang"];
const ARROWCLOUD_EFFECT_NAMES: [&str; 10] = [
    "Drunk",
    "Dizzy",
    "Confusion",
    "Big",
    "Flip",
    "Invert",
    "Tornado",
    "Tipsy",
    "Bumpy",
    "Beat",
];
const ARROWCLOUD_APPEARANCE_NAMES: [&str; 5] = ["Hidden", "Sudden", "Stealth", "Blink", "R.Vanish"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowCloudSubmitUiStatus {
    Submitting,
    Submitted,
    SubmitFailed,
    TimedOut,
}

#[derive(Debug, Clone)]
struct ArrowCloudSubmitUiEntry {
    chart_hash: String,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
}

static ARROWCLOUD_SUBMIT_UI_STATUS: std::sync::LazyLock<
    Mutex<[Option<ArrowCloudSubmitUiEntry>; 2]>,
> = std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| None)));
static ARROWCLOUD_SUBMIT_UI_TOKEN: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
struct ArrowCloudSubmitRetryEntry {
    side: profile::PlayerSide,
    api_key: String,
    payload: ArrowCloudPayload,
}

static ARROWCLOUD_SUBMIT_RETRY: std::sync::LazyLock<
    Mutex<[Option<ArrowCloudSubmitRetryEntry>; 2]>,
> = std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| None)));

#[inline(always)]
fn arrowcloud_reset_submit_ui_status(side: profile::PlayerSide, chart_hash: &str) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let mut state = ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap();
    let slot = &mut state[submit_side_ix(side)];
    if slot
        .as_ref()
        .is_some_and(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
    {
        *slot = None;
    }
}

#[inline(always)]
fn arrowcloud_reset_submit_retry(side: profile::PlayerSide, chart_hash: &str) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let mut state = ARROWCLOUD_SUBMIT_RETRY.lock().unwrap();
    let slot = &mut state[submit_side_ix(side)];
    if slot
        .as_ref()
        .is_some_and(|entry| entry.payload.hash.eq_ignore_ascii_case(hash))
    {
        *slot = None;
    }
}

#[inline(always)]
fn arrowcloud_set_submit_ui_status(
    side: profile::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let mut state = ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap();
    state[submit_side_ix(side)] = Some(ArrowCloudSubmitUiEntry {
        chart_hash: hash.to_string(),
        token,
        status,
    });
}

#[inline(always)]
fn arrowcloud_update_submit_ui_status_if_token(
    side: profile::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
) {
    let mut state = ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap();
    let Some(entry) = state[submit_side_ix(side)].as_mut() else {
        return;
    };
    if entry.token != token || !entry.chart_hash.eq_ignore_ascii_case(chart_hash) {
        return;
    }
    entry.status = status;
}

#[inline(always)]
fn arrowcloud_next_submit_ui_token() -> u64 {
    ARROWCLOUD_SUBMIT_UI_TOKEN.fetch_add(1, AtomicOrdering::Relaxed)
}

#[inline(always)]
const fn arrowcloud_can_retry_submit(status: ArrowCloudSubmitUiStatus) -> bool {
    matches!(
        status,
        ArrowCloudSubmitUiStatus::SubmitFailed | ArrowCloudSubmitUiStatus::TimedOut
    )
}

#[inline(always)]
fn arrowcloud_status_from_error_message(message: &str) -> ArrowCloudSubmitUiStatus {
    let lower = message.to_ascii_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        ArrowCloudSubmitUiStatus::TimedOut
    } else {
        ArrowCloudSubmitUiStatus::SubmitFailed
    }
}

#[inline(always)]
fn arrowcloud_warn_submit_skip(side: profile::PlayerSide, chart_hash: &str, reason: &str) {
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
    ARROWCLOUD_SUBMIT_RETRY.lock().unwrap()[submit_side_ix(side)] = Some(entry);
}

pub fn get_arrowcloud_submit_ui_status_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<ArrowCloudSubmitUiStatus> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap()[submit_side_ix(side)]
        .as_ref()
        .filter(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
        .map(|entry| entry.status)
}

#[derive(Debug, Clone, Serialize)]
struct ArrowCloudSpeed {
    value: f64,
    #[serde(rename = "type")]
    speed_type: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct ArrowCloudModifiers {
    #[serde(rename = "visualDelay")]
    visual_delay: i32,
    acceleration: Vec<String>,
    appearance: Vec<String>,
    effect: Vec<String>,
    mini: i32,
    turn: String,
    #[serde(rename = "disabledWindows")]
    disabled_windows: String,
    speed: ArrowCloudSpeed,
    perspective: String,
    noteskin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    scroll: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ArrowCloudRadar {
    #[serde(rename = "Holds")]
    holds: [u32; 2],
    #[serde(rename = "Mines")]
    mines: [u32; 2],
    #[serde(rename = "Rolls")]
    rolls: [u32; 2],
}

#[derive(Debug, Clone, Serialize)]
struct ArrowCloudLifePoint {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, Serialize)]
struct ArrowCloudNpsPoint {
    x: f64,
    y: f64,
    measure: u32,
    nps: f64,
}

#[derive(Debug, Clone, Serialize)]
struct ArrowCloudNpsInfo {
    #[serde(rename = "peakNPS")]
    peak_nps: f64,
    points: Vec<ArrowCloudNpsPoint>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum ArrowCloudTimingOffset {
    Seconds(f64),
    Miss(&'static str),
}

type ArrowCloudTimingDatum = (f64, ArrowCloudTimingOffset);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArrowCloudJudgmentCounts {
    fantastic_plus: u32,
    fantastic: u32,
    excellent: u32,
    great: u32,
    decent: u32,
    way_off: u32,
    miss: u32,
    total_steps: u32,
    holds_held: u32,
    total_holds: u32,
    mines_hit: u32,
    total_mines: u32,
    rolls_held: u32,
    total_rolls: u32,
}

#[derive(Debug, Clone, Serialize)]
struct ArrowCloudPayload {
    #[serde(rename = "songName")]
    song_name: String,
    artist: String,
    pack: String,
    length: String,
    hash: String,
    #[serde(rename = "timingData")]
    timing_data: Vec<ArrowCloudTimingDatum>,
    difficulty: u32,
    stepartist: String,
    radar: ArrowCloudRadar,
    #[serde(rename = "judgmentCounts")]
    judgment_counts: ArrowCloudJudgmentCounts,
    #[serde(rename = "npsInfo")]
    nps_info: ArrowCloudNpsInfo,
    #[serde(rename = "lifebarInfo")]
    lifebar_info: Vec<ArrowCloudLifePoint>,
    modifiers: ArrowCloudModifiers,
    #[serde(rename = "musicRate")]
    music_rate: f64,
    #[serde(rename = "usedAutoplay")]
    used_autoplay: bool,
    passed: bool,
    #[serde(rename = "bodyVersion")]
    body_version: &'static str,
    #[serde(rename = "_arrowCloudBodyVersion")]
    arrow_cloud_body_version: &'static str,
    #[serde(rename = "_engineName")]
    engine_name: &'static str,
    #[serde(rename = "_engineVersion")]
    engine_version: &'static str,
}

#[derive(Debug)]
struct ArrowCloudSubmitJob {
    side: profile::PlayerSide,
    api_key: String,
    token: u64,
    payload: ArrowCloudPayload,
}

#[derive(Debug)]
struct ArrowCloudSubmitError {
    status: ArrowCloudSubmitUiStatus,
    message: String,
}

#[inline(always)]
fn arrowcloud_format_length(seconds: f32) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "0:00".to_string();
    }
    let total = seconds.floor() as i64;
    if total >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            total / 3600,
            (total % 3600) / 60,
            total % 60
        )
    } else {
        format!("{}:{:02}", total / 60, total % 60)
    }
}

#[inline(always)]
fn arrowcloud_mask_labels_u8(mask: u8, names: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if (mask & (1u8 << i)) != 0 {
            out.push((*name).to_string());
        }
    }
    out
}

#[inline(always)]
fn arrowcloud_mask_labels_u16(mask: u16, names: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if (mask & (1u16 << i)) != 0 {
            out.push((*name).to_string());
        }
    }
    out
}

#[inline(always)]
fn arrowcloud_turn_label(turn: profile::TurnOption) -> &'static str {
    match turn {
        profile::TurnOption::None => "None",
        profile::TurnOption::Mirror => "Mirror",
        profile::TurnOption::Left => "Left",
        profile::TurnOption::Right => "Right",
        profile::TurnOption::LRMirror => "LR-Mirror",
        profile::TurnOption::UDMirror => "UD-Mirror",
        profile::TurnOption::Shuffle
        | profile::TurnOption::Blender
        | profile::TurnOption::Random => "Shuffle",
    }
}

#[inline(always)]
fn arrowcloud_scroll_label(scroll: profile::ScrollOption) -> Option<String> {
    if scroll.contains(profile::ScrollOption::Reverse) {
        Some("Reverse".to_string())
    } else if scroll.contains(profile::ScrollOption::Split) {
        Some("Split".to_string())
    } else if scroll.contains(profile::ScrollOption::Alternate) {
        Some("Alternate".to_string())
    } else if scroll.contains(profile::ScrollOption::Cross) {
        Some("Cross".to_string())
    } else if scroll.contains(profile::ScrollOption::Centered) {
        Some("Centered".to_string())
    } else {
        None
    }
}

#[inline(always)]
fn arrowcloud_speed_payload(speed: crate::game::scroll::ScrollSpeedSetting) -> ArrowCloudSpeed {
    match speed {
        crate::game::scroll::ScrollSpeedSetting::CMod(v) => ArrowCloudSpeed {
            value: v as f64,
            speed_type: "C",
        },
        crate::game::scroll::ScrollSpeedSetting::MMod(v) => ArrowCloudSpeed {
            value: v as f64,
            speed_type: "M",
        },
        crate::game::scroll::ScrollSpeedSetting::XMod(v) => ArrowCloudSpeed {
            value: ((v as f64) * 100.0).round() / 100.0,
            speed_type: "X",
        },
    }
}

#[inline(always)]
fn arrowcloud_modifiers(profile: &Profile) -> ArrowCloudModifiers {
    ArrowCloudModifiers {
        visual_delay: profile.visual_delay_ms,
        acceleration: arrowcloud_mask_labels_u8(
            profile::normalize_accel_effects_mask(profile.accel_effects_active_mask),
            &ARROWCLOUD_ACCEL_NAMES,
        ),
        appearance: arrowcloud_mask_labels_u8(
            profile::normalize_appearance_effects_mask(profile.appearance_effects_active_mask),
            &ARROWCLOUD_APPEARANCE_NAMES,
        ),
        effect: arrowcloud_mask_labels_u16(
            profile::normalize_visual_effects_mask(profile.visual_effects_active_mask),
            &ARROWCLOUD_EFFECT_NAMES,
        ),
        mini: profile.mini_percent.clamp(-100, 150),
        turn: arrowcloud_turn_label(profile.turn_option).to_string(),
        disabled_windows: "None".to_string(),
        speed: arrowcloud_speed_payload(profile.scroll_speed),
        perspective: profile.perspective.to_string(),
        noteskin: profile.noteskin.as_str().to_string(),
        scroll: arrowcloud_scroll_label(profile.scroll_option),
    }
}

#[inline(always)]
fn arrowcloud_life_lerp_at(life_history: &[(f32, f32)], sample_time: f32) -> f32 {
    let Some(&(_, first_life)) = life_history.first() else {
        return 0.0;
    };
    if life_history.len() == 1 {
        return first_life.clamp(0.0, 1.0);
    }

    let later_ix = life_history.partition_point(|&(t, _)| t <= sample_time);
    let earlier_ix = later_ix.saturating_sub(1).min(life_history.len() - 1);
    let (earlier_t, earlier_life) = life_history[earlier_ix];
    if later_ix >= life_history.len() {
        return earlier_life.clamp(0.0, 1.0);
    }

    let (later_t, later_life) = life_history[later_ix];
    let dt = later_t - earlier_t;
    if dt.abs() <= f32::EPSILON {
        return earlier_life.clamp(0.0, 1.0);
    }
    let alpha = ((sample_time - earlier_t) / dt).clamp(0.0, 1.0);
    (earlier_life + (later_life - earlier_life) * alpha).clamp(0.0, 1.0)
}

#[inline(always)]
fn arrowcloud_lifebar_points(gs: &gameplay::State, player_idx: usize) -> Vec<ArrowCloudLifePoint> {
    let life_history = gs.players[player_idx].life_history.as_slice();
    if life_history.is_empty() {
        return Vec::new();
    }
    let (start, end) = gs.note_ranges[player_idx];
    let note_times = &gs.note_time_cache[start..end];
    let first_second = gs.density_graph_first_second.min(0.0);
    let last_second = gs.density_graph_last_second.max(first_second);
    let chart_start_second = note_times
        .iter()
        .copied()
        .find(|t| t.is_finite())
        .unwrap_or(first_second);
    let duration = (last_second - first_second).max(0.0);
    let step = duration / ARROWCLOUD_LIFEBAR_POINTS as f32;

    let mut out = Vec::with_capacity(ARROWCLOUD_LIFEBAR_POINTS);
    for i in 0..ARROWCLOUD_LIFEBAR_POINTS {
        let x = chart_start_second + (i as f32 * step);
        out.push(ArrowCloudLifePoint {
            x: x as f64,
            y: arrowcloud_life_lerp_at(life_history, x) as f64,
        });
    }
    out
}

#[inline(always)]
fn arrowcloud_timing_data_from_scatter(
    scatter: &[crate::game::timing::ScatterPoint],
) -> Vec<ArrowCloudTimingDatum> {
    let mut out = Vec::with_capacity(scatter.len());
    for point in scatter {
        if !point.time_sec.is_finite() {
            continue;
        }
        let value = if let Some(offset_ms) = point.offset_ms {
            if !offset_ms.is_finite() {
                continue;
            }
            ArrowCloudTimingOffset::Seconds((offset_ms / 1000.0) as f64)
        } else {
            ArrowCloudTimingOffset::Miss("Miss")
        };
        out.push((point.time_sec as f64, value));
    }
    out
}

#[inline(always)]
fn arrowcloud_timing_data(gs: &gameplay::State, player_idx: usize) -> Vec<ArrowCloudTimingDatum> {
    let (start, end) = gs.note_ranges[player_idx];
    let notes = &gs.notes[start..end];
    let note_times = &gs.note_time_cache[start..end];
    let col_offset = player_idx.saturating_mul(gs.cols_per_player);
    let stream_segments = gameplay::stream_segments_for_results(gs, player_idx);
    let scatter = crate::game::timing::build_scatter_points(
        notes,
        note_times,
        col_offset,
        gs.cols_per_player,
        &stream_segments,
    );
    arrowcloud_timing_data_from_scatter(&scatter)
}

#[inline(always)]
fn arrowcloud_nps_info(gs: &gameplay::State, player_idx: usize) -> ArrowCloudNpsInfo {
    let chart = gs.charts[player_idx].as_ref();
    let first_second = gs.density_graph_first_second.min(0.0);
    let last_second = gs.density_graph_last_second.max(first_second);
    let peak_nps = if chart.max_nps.is_finite() && chart.max_nps > 0.0 {
        chart.max_nps
    } else {
        0.0
    };

    let mut points = Vec::with_capacity(chart.measure_nps_vec.len());
    let mut started = false;
    for (measure, nps) in chart.measure_nps_vec.iter().copied().enumerate() {
        if !nps.is_finite() {
            continue;
        }
        if nps > 0.0 {
            started = true;
        }
        if !started {
            continue;
        }
        let Some(&t) = chart.measure_seconds_vec.get(measure) else {
            continue;
        };
        let x = if last_second > first_second {
            ((t - first_second) / (last_second - first_second)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let y = if peak_nps > 0.0 {
            (nps / peak_nps).clamp(0.0, 1.0)
        } else {
            0.0
        };
        points.push(ArrowCloudNpsPoint {
            x: x as f64,
            y,
            measure: measure as u32,
            nps,
        });
    }

    ArrowCloudNpsInfo { peak_nps, points }
}

#[inline(always)]
fn arrowcloud_judgment_counts(gs: &gameplay::State, player_idx: usize) -> ArrowCloudJudgmentCounts {
    let player = &gs.players[player_idx];
    let counts = player.judgment_counts;
    let windows = gs.live_window_counts[player_idx];
    let fantastic_total = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Fantastic)];
    let fantastic_plus = windows.w0;
    let fantastic = fantastic_total.saturating_sub(fantastic_plus);
    let excellent = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Excellent)];
    let great = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Great)];
    let decent = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Decent)];
    let way_off = counts[judgment::judge_grade_ix(judgment::JudgeGrade::WayOff)];
    let miss = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Miss)];
    let mut total_steps = 0u32;
    for count in counts {
        total_steps = total_steps.saturating_add(count);
    }

    ArrowCloudJudgmentCounts {
        fantastic_plus,
        fantastic,
        excellent,
        great,
        decent,
        way_off,
        miss,
        total_steps,
        holds_held: player.holds_held,
        total_holds: gs.holds_total[player_idx],
        mines_hit: player.mines_hit,
        total_mines: gs.mines_total[player_idx],
        rolls_held: player.rolls_held,
        total_rolls: gs.rolls_total[player_idx],
    }
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
        length: arrowcloud_format_length(gs.song.music_length_seconds),
        hash: chart.short_hash.clone(),
        timing_data: arrowcloud_timing_data(gs, player_idx),
        difficulty: chart.meter,
        stepartist: chart.step_artist.clone(),
        radar: ArrowCloudRadar {
            holds: [player.holds_held, gs.holds_total[player_idx]],
            mines: [player.mines_avoided, gs.mines_total[player_idx]],
            rolls: [player.rolls_held, gs.rolls_total[player_idx]],
        },
        judgment_counts: arrowcloud_judgment_counts(gs, player_idx),
        nps_info: arrowcloud_nps_info(gs, player_idx),
        lifebar_info: arrowcloud_lifebar_points(gs, player_idx),
        modifiers: arrowcloud_modifiers(profile),
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
    side: profile::PlayerSide,
    api_key: &str,
    payload: &ArrowCloudPayload,
) -> Result<(), ArrowCloudSubmitError> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err(ArrowCloudSubmitError {
            status: ArrowCloudSubmitUiStatus::SubmitFailed,
            message: "missing ArrowCloud API key".to_string(),
        });
    }
    let Some(url) = online::arrowcloud_submit_url(payload.hash.as_str()) else {
        return Err(ArrowCloudSubmitError {
            status: ArrowCloudSubmitUiStatus::SubmitFailed,
            message: "missing chart hash".to_string(),
        });
    };

    let bearer = format!("Bearer {api_key}");
    let agent = network::get_agent();
    let response = agent
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", &bearer)
        .send_json(payload)
        .map_err(|e| {
            let msg = format!("network error: {e}");
            ArrowCloudSubmitError {
                status: arrowcloud_status_from_error_message(msg.as_str()),
                message: msg,
            }
        })?;
    let status = response.status();
    let status_code = status.as_u16();
    let body = response.into_body().read_to_string().unwrap_or_default();
    if status.is_success() {
        let snippet = log_body_snippet(body.as_str());
        if snippet.is_empty() {
            debug!(
                "ArrowCloud submit success for {:?} ({}) status={}",
                side, payload.hash, status_code
            );
        } else {
            debug!(
                "ArrowCloud submit success for {:?} ({}) status={} body='{}'",
                side,
                payload.hash,
                status_code,
                snippet.as_str()
            );
        }
        return Ok(());
    }

    let snippet = log_body_snippet(body.as_str());
    let status_kind = if status_code == 408 || status_code == 504 {
        ArrowCloudSubmitUiStatus::TimedOut
    } else {
        ArrowCloudSubmitUiStatus::SubmitFailed
    };
    if snippet.is_empty() {
        Err(ArrowCloudSubmitError {
            status: status_kind,
            message: format!("HTTP {status_code}"),
        })
    } else {
        Err(ArrowCloudSubmitError {
            status: status_kind,
            message: format!("HTTP {status_code}: {}", snippet.as_str()),
        })
    }
}

fn spawn_arrowcloud_submit_jobs(jobs: Vec<ArrowCloudSubmitJob>) {
    std::thread::spawn(move || {
        for job in jobs {
            match submit_arrowcloud_payload(job.side, &job.api_key, &job.payload) {
                Ok(()) => arrowcloud_update_submit_ui_status_if_token(
                    job.side,
                    job.payload.hash.as_str(),
                    job.token,
                    ArrowCloudSubmitUiStatus::Submitted,
                ),
                Err(err) => {
                    arrowcloud_update_submit_ui_status_if_token(
                        job.side,
                        job.payload.hash.as_str(),
                        job.token,
                        err.status,
                    );
                    warn!(
                        "ArrowCloud submit failed for {:?} ({}) status={:?}: {}",
                        job.side, job.payload.hash, err.status, err.message
                    );
                }
            }
            invalidate_player_leaderboards_for_side(job.payload.hash.as_str(), job.side);
        }
    });
}

pub fn submit_arrowcloud_payloads_from_gameplay(gs: &gameplay::State) {
    for player_idx in 0..gs.num_players.min(gameplay::MAX_PLAYERS) {
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
    if gs.song.has_lua {
        debug!("Skipping ArrowCloud submit: simfile relies on lua.");
        return;
    }
    let mut jobs = Vec::with_capacity(gs.num_players.min(gameplay::MAX_PLAYERS));
    for player_idx in 0..gs.num_players.min(gameplay::MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        let chart_hash = gs.charts[player_idx].short_hash.as_str();
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
            if passed || (failed && cfg.submit_arrowcloud_fails) {
                arrowcloud_warn_submit_skip(side, chart_hash, "profile is missing API key");
            }
            continue;
        }
        let Some(payload) = arrowcloud_payload_for_player(gs, player_idx) else {
            arrowcloud_warn_submit_skip(side, chart_hash, "failed to build submit payload");
            continue;
        };
        if failed && !cfg.submit_arrowcloud_fails {
            debug!(
                "Skipping ArrowCloud submit for {:?} ({}): failed-stage submits are disabled.",
                side, payload.hash
            );
            continue;
        }
        arrowcloud_store_submit_retry(ArrowCloudSubmitRetryEntry {
            side,
            api_key: api_key.to_string(),
            payload: payload.clone(),
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
        });
    }
    if jobs.is_empty() {
        return;
    }

    spawn_arrowcloud_submit_jobs(jobs);
}

pub fn retry_timed_out_arrowcloud_submit(chart_hash: &str, side: profile::PlayerSide) -> bool {
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
    if !arrowcloud_can_retry_submit(status) {
        return false;
    }
    let Some(entry) = ARROWCLOUD_SUBMIT_RETRY.lock().unwrap()[submit_side_ix(side)]
        .as_ref()
        .filter(|entry| entry.payload.hash.eq_ignore_ascii_case(hash))
        .cloned()
    else {
        return false;
    };

    let token = arrowcloud_next_submit_ui_token();
    arrowcloud_set_submit_ui_status(side, hash, token, ArrowCloudSubmitUiStatus::Submitting);
    debug!("Retrying ArrowCloud submit for {:?} ({}).", side, hash);
    spawn_arrowcloud_submit_jobs(vec![ArrowCloudSubmitJob {
        side: entry.side,
        api_key: entry.api_key,
        token,
        payload: entry.payload,
    }]);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::timing::ScatterPoint;
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

    #[test]
    fn arrowcloud_timing_data_keeps_miss_rows() {
        let scatter = [
            sample_scatter(12.5, Some(8.0)),
            sample_scatter(12.75, None),
            sample_scatter(f32::NAN, Some(2.0)),
        ];
        let timing_data = arrowcloud_timing_data_from_scatter(&scatter);
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
    fn arrowcloud_payload_serializes_miss_and_counts() {
        let payload = ArrowCloudPayload {
            song_name: "Test Song".to_string(),
            artist: "Test Artist".to_string(),
            pack: "Test Pack".to_string(),
            length: "1:23".to_string(),
            hash: "deadbeefcafebabe".to_string(),
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
        };

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
    fn arrowcloud_retry_allows_failed_requests() {
        assert!(!arrowcloud_can_retry_submit(
            ArrowCloudSubmitUiStatus::Submitting
        ));
        assert!(!arrowcloud_can_retry_submit(
            ArrowCloudSubmitUiStatus::Submitted
        ));
        assert!(arrowcloud_can_retry_submit(
            ArrowCloudSubmitUiStatus::SubmitFailed
        ));
        assert!(arrowcloud_can_retry_submit(
            ArrowCloudSubmitUiStatus::TimedOut
        ));
    }

    #[test]
    fn arrowcloud_error_message_maps_timeout_status() {
        assert_eq!(
            arrowcloud_status_from_error_message("Timed Out"),
            ArrowCloudSubmitUiStatus::TimedOut
        );
        assert_eq!(
            arrowcloud_status_from_error_message("network error: timed out while connecting"),
            ArrowCloudSubmitUiStatus::TimedOut
        );
        assert_eq!(
            arrowcloud_status_from_error_message("Machine Offline"),
            ArrowCloudSubmitUiStatus::SubmitFailed
        );
    }
}
