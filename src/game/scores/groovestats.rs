use super::{
    GROOVESTATS_CHART_HASH_VERSION, GROOVESTATS_COMMENT_PREFIX, GROOVESTATS_REASON_COUNT,
    GROOVESTATS_SUBMIT_MAX_ENTRIES, GS_INVALID_HOLDS_MASK, GS_INVALID_INSERT_MASK,
    GS_INVALID_REMOVE_MASK, ItlEventProgress, cache_gs_score_from_leaderboard, compact_f32_text,
    de_i32_from_string_or_number, de_string_from_string_or_number, de_u32_from_string_or_number,
    gameplay_side_for_player, itl, log_body_snippet, submit_record_banner, submit_side_ix,
};
use crate::engine::network;
use crate::game::gameplay;
use crate::game::judgment;
use crate::game::online;
use crate::game::profile::{self, Profile};
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

#[derive(Clone, Debug, Default)]
pub struct GrooveStatsEvalState {
    pub valid: bool,
    pub reason_lines: Vec<String>,
    pub manual_qr_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrooveStatsSubmitUiStatus {
    Submitting,
    Submitted,
    SubmitFailed,
    TimedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrooveStatsSubmitRecordBanner {
    PersonalBest,
    WorldRecord,
    WorldRecordEx,
}

#[derive(Debug, Clone)]
struct GrooveStatsSubmitUiEntry {
    chart_hash: String,
    token: u64,
    status: GrooveStatsSubmitUiStatus,
}

static GROOVESTATS_SUBMIT_UI_STATUS: std::sync::LazyLock<
    Mutex<[Option<GrooveStatsSubmitUiEntry>; 2]>,
> = std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| None)));
static GROOVESTATS_SUBMIT_UI_TOKEN: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
struct GrooveStatsSubmitEventUiEntry {
    chart_hash: String,
    token: u64,
    itl_progress: Option<ItlEventProgress>,
    record_banner: Option<GrooveStatsSubmitRecordBanner>,
}

static GROOVESTATS_SUBMIT_EVENT_UI: std::sync::LazyLock<
    Mutex<[Option<GrooveStatsSubmitEventUiEntry>; 2]>,
> = std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| None)));

#[derive(Debug, Clone)]
struct GrooveStatsSubmitRetryEntry {
    side: profile::PlayerSide,
    slot: u8,
    chart_hash: String,
    username: String,
    profile_name: String,
    profile_id: Option<String>,
    itl_score_hundredths: Option<u32>,
    show_ex_score: bool,
    api_key: String,
    payload: GrooveStatsSubmitPlayerPayload,
}

static GROOVESTATS_SUBMIT_RETRY: std::sync::LazyLock<
    Mutex<[Option<GrooveStatsSubmitRetryEntry>; 2]>,
> = std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| None)));

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GrooveStatsJudgmentCounts {
    pub(super) fantastic_plus: u32,
    pub(super) fantastic: u32,
    pub(super) excellent: u32,
    pub(super) great: u32,
    pub(super) decent: u32,
    pub(super) way_off: u32,
    pub(super) miss: u32,
    pub(super) total_steps: u32,
    pub(super) holds_held: u32,
    pub(super) total_holds: u32,
    pub(super) mines_hit: u32,
    pub(super) total_mines: u32,
    pub(super) rolls_held: u32,
    pub(super) total_rolls: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsRescoreCounts {
    fantastic_plus: u32,
    fantastic: u32,
    excellent: u32,
    great: u32,
    decent: u32,
    way_off: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsSubmitPlayerPayload {
    rate: u32,
    score: u32,
    judgment_counts: GrooveStatsJudgmentCounts,
    rescore_counts: GrooveStatsRescoreCounts,
    used_cmod: bool,
    comment: String,
}

#[derive(Debug)]
pub(super) struct GrooveStatsSubmitPlayerJob {
    pub(super) side: profile::PlayerSide,
    pub(super) slot: u8,
    pub(super) chart_hash: String,
    pub(super) username: String,
    pub(super) profile_name: String,
    pub(super) profile_id: Option<String>,
    pub(super) token: u64,
    pub(super) itl_score_hundredths: Option<u32>,
    pub(super) show_ex_score: bool,
}

#[derive(Debug)]
struct GrooveStatsSubmitRequest {
    players: Vec<GrooveStatsSubmitPlayerJob>,
    headers: Vec<(String, String)>,
    query: Vec<(String, String)>,
    body: JsonValue,
}

#[derive(Debug)]
struct GrooveStatsSubmitError {
    status: GrooveStatsSubmitUiStatus,
    message: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsSubmitApiResponse {
    #[serde(default)]
    error: String,
    player1: Option<GrooveStatsSubmitApiPlayer>,
    player2: Option<GrooveStatsSubmitApiPlayer>,
}

impl GrooveStatsSubmitApiResponse {
    #[inline(always)]
    fn player_for_slot(&self, slot: u8) -> Option<&GrooveStatsSubmitApiPlayer> {
        match slot {
            1 => self.player1.as_ref(),
            2 => self.player2.as_ref(),
            _ => None,
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct GrooveStatsSubmitApiPlayer {
    #[serde(default)]
    pub(super) chart_hash: String,
    #[serde(default)]
    pub(super) result: String,
    #[serde(rename = "gsLeaderboard", default)]
    pub(super) gs_leaderboard: Vec<super::LeaderboardApiEntry>,
    #[serde(rename = "exLeaderboard", default)]
    pub(super) ex_leaderboard: Vec<super::LeaderboardApiEntry>,
    pub(super) rpg: Option<GrooveStatsSubmitApiEvent>,
    pub(super) itl: Option<GrooveStatsSubmitApiEvent>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct GrooveStatsSubmitApiEvent {
    #[serde(default)]
    pub(super) name: String,
    #[serde(default, deserialize_with = "de_i32_from_string_or_number")]
    pub(super) score_delta: i32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub(super) top_score_points: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub(super) prev_top_score_points: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub(super) total_passes: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub(super) current_ranking_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub(super) previous_ranking_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub(super) current_song_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub(super) previous_song_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub(super) current_ex_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub(super) previous_ex_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub(super) current_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub(super) previous_point_total: u32,
    #[serde(rename = "itlLeaderboard", default)]
    pub(super) itl_leaderboard: Vec<super::LeaderboardApiEntry>,
    #[serde(default)]
    pub(super) is_doubles: bool,
    pub(super) progress: Option<GrooveStatsSubmitApiProgress>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct GrooveStatsSubmitApiProgress {
    #[serde(rename = "statImprovements", default)]
    pub(super) stat_improvements: Vec<GrooveStatsSubmitApiStatImprovement>,
    #[serde(rename = "questsCompleted", default)]
    pub(super) quests_completed: Vec<GrooveStatsSubmitApiQuest>,
    #[serde(rename = "achievementsCompleted", default)]
    pub(super) achievements_completed: Vec<GrooveStatsSubmitApiAchievement>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct GrooveStatsSubmitApiStatImprovement {
    #[serde(default)]
    pub(super) name: String,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    pub(super) gained: u32,
    #[serde(default, deserialize_with = "de_i32_from_string_or_number")]
    pub(super) current: i32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct GrooveStatsSubmitApiQuest {
    #[serde(default)]
    pub(super) title: String,
    #[serde(default)]
    pub(super) rewards: Vec<GrooveStatsSubmitApiQuestReward>,
    #[serde(default)]
    pub(super) song_download_url: String,
    #[serde(rename = "songDownloadFolders", default)]
    pub(super) song_download_folders: Vec<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct GrooveStatsSubmitApiQuestReward {
    #[serde(rename = "type", default)]
    pub(super) reward_type: String,
    #[serde(default)]
    pub(super) description: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct GrooveStatsSubmitApiAchievement {
    #[serde(default)]
    pub(super) title: String,
    #[serde(default)]
    pub(super) rewards: Vec<GrooveStatsSubmitApiAchievementReward>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct GrooveStatsSubmitApiAchievementReward {
    #[serde(default, deserialize_with = "de_string_from_string_or_number")]
    pub(super) tier: String,
    #[serde(default)]
    pub(super) requirements: Vec<String>,
    #[serde(rename = "titleUnlocked", default)]
    pub(super) title_unlocked: String,
}

#[inline(always)]
fn groovestats_reset_submit_ui_status(side: profile::PlayerSide, chart_hash: &str) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let mut state = GROOVESTATS_SUBMIT_UI_STATUS.lock().unwrap();
    let slot = &mut state[submit_side_ix(side)];
    if slot
        .as_ref()
        .is_some_and(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
    {
        *slot = None;
    }
}

#[inline(always)]
fn groovestats_reset_submit_event_ui(side: profile::PlayerSide, chart_hash: &str) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let mut state = GROOVESTATS_SUBMIT_EVENT_UI.lock().unwrap();
    let slot = &mut state[submit_side_ix(side)];
    if slot
        .as_ref()
        .is_some_and(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
    {
        *slot = None;
    }
}

#[inline(always)]
fn groovestats_reset_submit_retry(side: profile::PlayerSide, chart_hash: &str) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let mut state = GROOVESTATS_SUBMIT_RETRY.lock().unwrap();
    let slot = &mut state[submit_side_ix(side)];
    if slot
        .as_ref()
        .is_some_and(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
    {
        *slot = None;
    }
}

#[inline(always)]
fn groovestats_set_submit_ui_status(
    side: profile::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: GrooveStatsSubmitUiStatus,
) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    GROOVESTATS_SUBMIT_UI_STATUS.lock().unwrap()[submit_side_ix(side)] =
        Some(GrooveStatsSubmitUiEntry {
            chart_hash: hash.to_string(),
            token,
            status,
        });
}

#[inline(always)]
fn groovestats_update_submit_ui_status_if_token(
    side: profile::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: GrooveStatsSubmitUiStatus,
) {
    let mut state = GROOVESTATS_SUBMIT_UI_STATUS.lock().unwrap();
    let Some(entry) = state[submit_side_ix(side)].as_mut() else {
        return;
    };
    if entry.token != token || !entry.chart_hash.eq_ignore_ascii_case(chart_hash) {
        return;
    }
    entry.status = status;
}

#[inline(always)]
fn groovestats_arm_submit_event_ui(side: profile::PlayerSide, chart_hash: &str, token: u64) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    GROOVESTATS_SUBMIT_EVENT_UI.lock().unwrap()[submit_side_ix(side)] =
        Some(GrooveStatsSubmitEventUiEntry {
            chart_hash: hash.to_string(),
            token,
            itl_progress: None,
            record_banner: None,
        });
}

#[inline(always)]
fn groovestats_update_submit_event_ui_if_token(
    side: profile::PlayerSide,
    chart_hash: &str,
    token: u64,
    itl_progress: Option<ItlEventProgress>,
    record_banner: Option<GrooveStatsSubmitRecordBanner>,
) {
    let mut state = GROOVESTATS_SUBMIT_EVENT_UI.lock().unwrap();
    let Some(entry) = state[submit_side_ix(side)].as_mut() else {
        return;
    };
    if entry.token != token || !entry.chart_hash.eq_ignore_ascii_case(chart_hash) {
        return;
    }
    entry.itl_progress = itl_progress;
    entry.record_banner = record_banner;
}

#[inline(always)]
fn groovestats_next_submit_ui_token() -> u64 {
    GROOVESTATS_SUBMIT_UI_TOKEN.fetch_add(1, AtomicOrdering::Relaxed)
}

#[inline(always)]
const fn groovestats_can_retry_submit(status: GrooveStatsSubmitUiStatus) -> bool {
    matches!(status, GrooveStatsSubmitUiStatus::TimedOut)
}

#[inline(always)]
fn groovestats_store_submit_retry(entry: GrooveStatsSubmitRetryEntry) {
    let hash = entry.chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let side = entry.side;
    GROOVESTATS_SUBMIT_RETRY.lock().unwrap()[submit_side_ix(side)] = Some(entry);
}

pub fn get_groovestats_submit_ui_status_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<GrooveStatsSubmitUiStatus> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    GROOVESTATS_SUBMIT_UI_STATUS.lock().unwrap()[submit_side_ix(side)]
        .as_ref()
        .filter(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
        .map(|entry| entry.status)
}

pub fn get_groovestats_submit_itl_progress_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<ItlEventProgress> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    GROOVESTATS_SUBMIT_EVENT_UI.lock().unwrap()[submit_side_ix(side)]
        .as_ref()
        .filter(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
        .and_then(|entry| entry.itl_progress.clone())
}

pub fn get_groovestats_submit_record_banner_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<GrooveStatsSubmitRecordBanner> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    GROOVESTATS_SUBMIT_EVENT_UI.lock().unwrap()[submit_side_ix(side)]
        .as_ref()
        .filter(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
        .and_then(|entry| entry.record_banner)
}

fn groovestats_reason_lines(
    checks: &[bool; GROOVESTATS_REASON_COUNT],
    bad: &[String],
) -> Vec<String> {
    let mut out = Vec::with_capacity(6);
    for (idx, passed) in checks.iter().enumerate() {
        if *passed {
            continue;
        }
        match idx {
            0 => out.push("GrooveStats only supports dance and pump charts.".to_string()),
            1 => out.push("GrooveStats does not support dance-solo charts.".to_string()),
            2 => out.push("GrooveStats QR is unavailable in course mode.".to_string()),
            3 => out.push("GrooveStats requires ITG mode.".to_string()),
            4 => out.push("Timing windows must be at ITG or harder.".to_string()),
            5 => out.push("Life difficulty must be at ITG or harder.".to_string()),
            6 => {
                out.push("Metrics or preferences are incorrect.".to_string());
                out.extend(bad.iter().cloned());
            }
            7 => out.push("Music rate must be between 1.0x and 3.0x.".to_string()),
            8 => out.push("Note-removal modifiers are enabled.".to_string()),
            9 => out.push("Note-insertion modifiers are enabled.".to_string()),
            10 => out.push("Fail type must be Immediate or ImmediateContinue.".to_string()),
            11 => out.push("Autoplay or replay is not allowed.".to_string()),
            12 => out.push("MinTNSToScoreNotes cannot be W1 or W2.".to_string()),
            _ => {}
        }
    }
    out
}

fn groovestats_eval_state(
    chart: &crate::game::chart::ChartData,
    profile: &Profile,
    music_rate: f32,
    autoplay_used: bool,
    is_course_mode: bool,
) -> GrooveStatsEvalState {
    let chart_type = chart.chart_type.trim().to_ascii_lowercase();
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    let remove_mask = profile::normalize_remove_mask(profile.remove_active_mask);
    let insert_mask = profile::normalize_insert_mask(profile.insert_active_mask);
    let holds_mask = profile::normalize_holds_mask(profile.holds_active_mask);
    let fail_type_ok = matches!(
        crate::config::get().default_fail_type,
        crate::config::DefaultFailType::Immediate
            | crate::config::DefaultFailType::ImmediateContinue
    );

    let mut checks = [true; GROOVESTATS_REASON_COUNT];
    checks[0] = chart_type.starts_with("dance") || chart_type.starts_with("pump");
    checks[1] = !chart_type.contains("solo");
    checks[2] = !is_course_mode;
    checks[3] = true;
    checks[4] = true;
    checks[5] = true;
    checks[6] = !profile.custom_fantastic_window;
    checks[7] = (1.0..=3.0).contains(&rate);
    checks[8] = (remove_mask & GS_INVALID_REMOVE_MASK) == 0;
    checks[9] = (insert_mask & GS_INVALID_INSERT_MASK) == 0;
    checks[10] = fail_type_ok;
    checks[11] = !autoplay_used;
    checks[12] = true;
    if (holds_mask & GS_INVALID_HOLDS_MASK) != 0 {
        checks[8] = false;
    }

    let mut bad = Vec::with_capacity(1);
    if profile.custom_fantastic_window {
        bad.push(format!(
            "- Custom Fantastic window ({}ms)",
            profile.custom_fantastic_window_ms
        ));
    }

    GrooveStatsEvalState {
        valid: checks.iter().all(|passed| *passed),
        reason_lines: groovestats_reason_lines(&checks, bad.as_slice()),
        manual_qr_url: None,
    }
}

#[inline(always)]
fn groovestats_qr_append_rescore(out: &mut String, label: char, value: u32) {
    if value == 0 {
        return;
    }
    out.push(label);
    out.push_str(format!("{value:x}").as_str());
}

fn groovestats_manual_qr_url(
    base_url: &str,
    chart_hash: &str,
    hash_version: u8,
    counts: &GrooveStatsJudgmentCounts,
    rescored: &GrooveStatsRescoreCounts,
    failed: bool,
    rate: u32,
    used_cmod: bool,
) -> Option<String> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }

    let mut rescored_str = String::with_capacity(24);
    for (label, value) in [
        ('G', rescored.fantastic_plus),
        ('H', rescored.fantastic),
        ('I', rescored.excellent),
        ('J', rescored.great),
        ('K', rescored.decent),
        ('L', rescored.way_off),
    ] {
        groovestats_qr_append_rescore(&mut rescored_str, label, value);
    }

    Some(format!(
        "{}/QR/{hash}/T{:x}G{:x}H{:x}I{:x}J{:x}K{:x}L{:x}M{:x}H{:x}T{:x}R{:x}T{:x}M{:x}T{:x}{rescored_str}/F{}R{:x}C{}V{:x}",
        base_url.trim_end_matches('/'),
        counts.total_steps,
        counts.fantastic_plus,
        counts.fantastic,
        counts.excellent,
        counts.great,
        counts.decent,
        counts.way_off,
        counts.miss,
        counts.holds_held,
        counts.total_holds,
        counts.rolls_held,
        counts.total_rolls,
        counts.mines_hit,
        counts.total_mines,
        if failed { '1' } else { '0' },
        rate,
        if used_cmod { '1' } else { '0' },
        hash_version,
    ))
}

fn groovestats_manual_qr_url_from_gameplay(
    gs: &gameplay::State,
    player_idx: usize,
) -> Option<String> {
    if player_idx >= gs.num_players {
        return None;
    }
    let Some(payload) = groovestats_payload_for_player(gs, player_idx) else {
        return None;
    };
    groovestats_manual_qr_url(
        online::groovestats_qr_base_url(),
        gs.charts[player_idx].short_hash.as_str(),
        GROOVESTATS_CHART_HASH_VERSION,
        &payload.judgment_counts,
        &payload.rescore_counts,
        gs.players[player_idx].fail_time.is_some() || gs.players[player_idx].is_failing,
        payload.rate,
        payload.used_cmod,
    )
}

pub fn groovestats_eval_state_from_gameplay(
    gs: &gameplay::State,
    player_idx: usize,
) -> GrooveStatsEvalState {
    if player_idx >= gs.num_players.min(gameplay::MAX_PLAYERS) {
        return GrooveStatsEvalState::default();
    }
    let mut state = groovestats_eval_state(
        gs.charts[player_idx].as_ref(),
        &gs.player_profiles[player_idx],
        gs.music_rate,
        gs.autoplay_used,
        gs.course_display_totals.is_some(),
    );
    if state.valid {
        state.manual_qr_url = groovestats_manual_qr_url_from_gameplay(gs, player_idx);
    }
    state
}

fn groovestats_submit_invalid_reason(
    chart: &crate::game::chart::ChartData,
    song_has_lua: bool,
    profile: &Profile,
    music_rate: f32,
) -> Option<String> {
    if song_has_lua {
        return Some("simfile relies on lua".to_string());
    }
    groovestats_eval_state(chart, profile, music_rate, false, false)
        .reason_lines
        .into_iter()
        .next()
}

#[inline(always)]
pub(super) fn groovestats_judgment_counts(
    gs: &gameplay::State,
    player_idx: usize,
) -> GrooveStatsJudgmentCounts {
    let player = &gs.players[player_idx];
    let windows = gs.live_window_counts[player_idx];
    GrooveStatsJudgmentCounts {
        fantastic_plus: windows.w0,
        fantastic: windows.w1,
        excellent: windows.w2,
        great: windows.w3,
        decent: windows.w4,
        way_off: windows.w5,
        miss: windows.miss,
        total_steps: gs.total_steps[player_idx],
        holds_held: player.holds_held,
        total_holds: gs.holds_total[player_idx],
        mines_hit: player.mines_hit,
        total_mines: gs.mines_total[player_idx],
        rolls_held: player.rolls_held,
        total_rolls: gs.rolls_total[player_idx],
    }
}

#[inline(always)]
fn groovestats_rescore_add_target(counts: &mut GrooveStatsRescoreCounts, j: &judgment::Judgment) {
    if matches!(j.window, Some(judgment::TimingWindow::W0)) {
        counts.fantastic_plus = counts.fantastic_plus.saturating_add(1);
        return;
    }
    match j.grade {
        judgment::JudgeGrade::Fantastic => counts.fantastic = counts.fantastic.saturating_add(1),
        judgment::JudgeGrade::Excellent => counts.excellent = counts.excellent.saturating_add(1),
        judgment::JudgeGrade::Great => counts.great = counts.great.saturating_add(1),
        judgment::JudgeGrade::Decent => counts.decent = counts.decent.saturating_add(1),
        judgment::JudgeGrade::WayOff => counts.way_off = counts.way_off.saturating_add(1),
        judgment::JudgeGrade::Miss => {}
    }
}

fn groovestats_rescore_counts(gs: &gameplay::State, player_idx: usize) -> GrooveStatsRescoreCounts {
    let (start, end) = gs.note_ranges[player_idx];
    let mut counts = GrooveStatsRescoreCounts::default();
    for note in &gs.notes[start..end] {
        let Some(final_result) = note.result.as_ref() else {
            continue;
        };
        let Some(early_result) = note.early_result.as_ref() else {
            continue;
        };
        groovestats_rescore_add_target(&mut counts, final_result);
        groovestats_rescore_add_target(&mut counts, early_result);
    }
    counts
}

fn groovestats_comment_string(gs: &gameplay::State, player_idx: usize) -> String {
    let profile = &gs.player_profiles[player_idx];
    let counts = groovestats_judgment_counts(gs, player_idx);
    let mut parts: Vec<String> = Vec::with_capacity(10);

    if profile.show_fa_plus_window {
        let (start, end) = gs.note_ranges[player_idx];
        let ex = judgment::calculate_ex_score_from_notes(
            &gs.notes[start..end],
            &gs.note_time_cache[start..end],
            &gs.hold_end_time_cache[start..end],
            gs.total_steps[player_idx],
            gs.holds_total[player_idx],
            gs.rolls_total[player_idx],
            gs.mines_total[player_idx],
            gs.players[player_idx].fail_time,
            false,
        );
        parts.push("FA+".to_string());
        parts.push(format!("{ex:.2}EX"));
    }

    let rate = if gs.music_rate.is_finite() && gs.music_rate > 0.0 {
        gs.music_rate
    } else {
        1.0
    };
    if (rate - 1.0).abs() > 0.0001 {
        parts.push(format!("{}x Rate", compact_f32_text(rate)));
    }

    for (count, suffix) in [
        (counts.fantastic, "w"),
        (counts.excellent, "e"),
        (counts.great, "g"),
        (counts.decent, "d"),
        (counts.way_off, "wo"),
        (counts.miss, "m"),
    ] {
        if count != 0 {
            parts.push(format!("{count}{suffix}"));
        }
    }

    if let crate::game::scroll::ScrollSpeedSetting::CMod(value) = profile.scroll_speed {
        parts.push(format!("C{}", compact_f32_text(value)));
    }

    if parts.is_empty() {
        GROOVESTATS_COMMENT_PREFIX.to_string()
    } else {
        format!("{GROOVESTATS_COMMENT_PREFIX}, {}", parts.join(", "))
    }
}

fn groovestats_payload_for_player(
    gs: &gameplay::State,
    player_idx: usize,
) -> Option<GrooveStatsSubmitPlayerPayload> {
    if player_idx >= gs.num_players {
        return None;
    }
    let score_percent = judgment::calculate_itg_score_percent_from_counts(
        &gs.players[player_idx].scoring_counts,
        gs.players[player_idx].holds_held_for_score,
        gs.players[player_idx].rolls_held_for_score,
        gs.players[player_idx].mines_hit_for_score,
        gs.possible_grade_points[player_idx],
    );
    let score = (score_percent * 10000.0).round().clamp(0.0, 10000.0) as u32;
    let rate = if gs.music_rate.is_finite() && gs.music_rate > 0.0 {
        (gs.music_rate * 100.0).round().clamp(0.0, u32::MAX as f32) as u32
    } else {
        100
    };

    Some(GrooveStatsSubmitPlayerPayload {
        rate,
        score,
        judgment_counts: groovestats_judgment_counts(gs, player_idx),
        rescore_counts: groovestats_rescore_counts(gs, player_idx),
        used_cmod: matches!(
            gs.player_profiles[player_idx].scroll_speed,
            crate::game::scroll::ScrollSpeedSetting::CMod(_)
        ),
        comment: groovestats_comment_string(gs, player_idx),
    })
}

fn submit_groovestats_request(
    job: &GrooveStatsSubmitRequest,
) -> Result<GrooveStatsSubmitApiResponse, GrooveStatsSubmitError> {
    let service_name = online::groovestats_service_name();
    let mut request = network::get_agent()
        .post(&online::groovestats_score_submit_url())
        .header("Content-Type", "application/json");
    for (name, value) in &job.headers {
        request = request.header(name, value);
    }
    for (name, value) in &job.query {
        request = request.query(name, value);
    }

    let response = request.send_json(&job.body).map_err(|e| {
        let message = format!("network error: {e}");
        let lower = message.to_ascii_lowercase();
        GrooveStatsSubmitError {
            status: if lower.contains("timeout") || lower.contains("timed out") {
                GrooveStatsSubmitUiStatus::TimedOut
            } else {
                GrooveStatsSubmitUiStatus::SubmitFailed
            },
            message,
        }
    })?;

    let status = response.status();
    let status_code = status.as_u16();
    let body = response.into_body().read_to_string().unwrap_or_default();
    if !status.is_success() {
        let snippet = log_body_snippet(body.as_str());
        let status_kind = if status_code == 408 || status_code == 504 {
            GrooveStatsSubmitUiStatus::TimedOut
        } else {
            GrooveStatsSubmitUiStatus::SubmitFailed
        };
        return Err(GrooveStatsSubmitError {
            status: status_kind,
            message: if snippet.is_empty() {
                format!("{service_name} submit returned HTTP {status_code}")
            } else {
                format!("{service_name} submit returned HTTP {status_code}: {snippet}")
            },
        });
    }

    let decoded: GrooveStatsSubmitApiResponse =
        serde_json::from_str(body.as_str()).map_err(|error| GrooveStatsSubmitError {
            status: GrooveStatsSubmitUiStatus::SubmitFailed,
            message: format!(
                "failed to parse {service_name} submit response: {}",
                log_body_snippet(error.to_string().as_str())
            ),
        })?;
    if !decoded.error.trim().is_empty() {
        return Err(GrooveStatsSubmitError {
            status: GrooveStatsSubmitUiStatus::SubmitFailed,
            message: format!("{service_name} submit error: {}", decoded.error.trim()),
        });
    }

    let snippet = log_body_snippet(body.as_str());
    if !snippet.is_empty() {
        debug!("{service_name} submit success body='{}'", snippet.as_str());
    } else {
        debug!("{service_name} submit success");
    }
    Ok(decoded)
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
                        GrooveStatsSubmitUiStatus::SubmitFailed,
                    );
                    warn!(
                        "{} submit response omitted player{} for {:?} ({}).",
                        online::groovestats_service_name(),
                        player.slot,
                        player.side,
                        player.chart_hash
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
                        GrooveStatsSubmitUiStatus::SubmitFailed,
                    );
                    warn!(
                        "{} submit response hash mismatch for {:?}: expected {}, got {}.",
                        online::groovestats_service_name(),
                        player.side,
                        player.chart_hash,
                        player_response.chart_hash
                    );
                    continue;
                }

                groovestats_update_submit_ui_status_if_token(
                    player.side,
                    player.chart_hash.as_str(),
                    player.token,
                    GrooveStatsSubmitUiStatus::Submitted,
                );
                groovestats_update_submit_event_ui_if_token(
                    player.side,
                    player.chart_hash.as_str(),
                    player.token,
                    itl::progress_from_submit(player, player_response),
                    submit_record_banner(player, player_response),
                );
                if let Some(profile_id) = player.profile_id.as_deref()
                    && !player.username.is_empty()
                    && !player_response.gs_leaderboard.is_empty()
                {
                    cache_gs_score_from_leaderboard(
                        profile_id,
                        player.username.as_str(),
                        player.chart_hash.as_str(),
                        player_response.gs_leaderboard.as_slice(),
                    );
                }
                itl::handle_submit_player_unlocks(player, player_response);
                debug!(
                    "{} submit succeeded for {:?} ({}) result='{}'",
                    online::groovestats_service_name(),
                    player.side,
                    player.chart_hash,
                    player_response.result
                );
            }
        }
        Err(err) => {
            for player in &job.players {
                groovestats_update_submit_ui_status_if_token(
                    player.side,
                    player.chart_hash.as_str(),
                    player.token,
                    err.status,
                );
            }
            warn!("{}", err.message);
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
    };
    let mut body = JsonMap::with_capacity(1);
    body.insert(
        format!("player{}", player.slot),
        serde_json::to_value(&entry.payload).expect("serialize GrooveStats retry payload"),
    );
    GrooveStatsSubmitRequest {
        players: vec![player],
        headers: vec![(
            format!("x-api-key-player-{}", entry.slot),
            entry.api_key.clone(),
        )],
        query: vec![
            (
                "maxLeaderboardResults".to_string(),
                GROOVESTATS_SUBMIT_MAX_ENTRIES.to_string(),
            ),
            (
                format!("chartHashP{}", entry.slot),
                entry.chart_hash.clone(),
            ),
        ],
        body: JsonValue::Object(body),
    }
}

pub fn submit_groovestats_payloads_from_gameplay(gs: &gameplay::State) {
    for player_idx in 0..gs.num_players.min(gameplay::MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        groovestats_reset_submit_ui_status(side, gs.charts[player_idx].short_hash.as_str());
        groovestats_reset_submit_event_ui(side, gs.charts[player_idx].short_hash.as_str());
        groovestats_reset_submit_retry(side, gs.charts[player_idx].short_hash.as_str());
    }

    let cfg = crate::config::get();
    if !cfg.enable_groovestats || gs.num_players == 0 {
        return;
    }
    if gs.autoplay_used {
        debug!(
            "Skipping {} submit: autoplay/replay was used.",
            online::groovestats_service_name()
        );
        return;
    }
    if gs.course_display_totals.is_some() {
        debug!(
            "Skipping {} submit: course mode is unsupported by the old submit API.",
            online::groovestats_service_name()
        );
        return;
    }
    if gs.song.has_lua {
        debug!(
            "Skipping {} submit: simfile relies on lua.",
            online::groovestats_service_name()
        );
        return;
    }

    let online::ConnectionStatus::Connected(services) = online::get_status() else {
        debug!(
            "Skipping {} submit: service connection is not ready.",
            online::groovestats_service_name()
        );
        return;
    };
    if !services.auto_submit {
        debug!(
            "Skipping {} submit: auto-submit is not enabled by the service.",
            online::groovestats_service_name()
        );
        return;
    }

    let mut body = JsonMap::with_capacity(gs.num_players.min(gameplay::MAX_PLAYERS));
    let mut headers = Vec::with_capacity(gs.num_players.min(gameplay::MAX_PLAYERS));
    let mut query = Vec::with_capacity(gs.num_players.min(gameplay::MAX_PLAYERS) + 1);
    let mut players = Vec::with_capacity(gs.num_players.min(gameplay::MAX_PLAYERS));
    query.push((
        "maxLeaderboardResults".to_string(),
        GROOVESTATS_SUBMIT_MAX_ENTRIES.to_string(),
    ));

    for player_idx in 0..gs.num_players.min(gameplay::MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        let slot = if side == profile::PlayerSide::P1 {
            1
        } else {
            2
        };
        let profile = &gs.player_profiles[player_idx];
        let chart = gs.charts[player_idx].as_ref();

        if let Some(reason) =
            groovestats_submit_invalid_reason(chart, gs.song.has_lua, profile, gs.music_rate)
        {
            debug!(
                "Skipping {} submit for {:?} ({}): {}.",
                online::groovestats_service_name(),
                side,
                chart.short_hash,
                reason
            );
            continue;
        }
        if !profile.groovestats_is_pad_player {
            debug!(
                "Skipping {} submit for {:?} ({}): profile is not marked as a pad player.",
                online::groovestats_service_name(),
                side,
                chart.short_hash
            );
            continue;
        }
        if profile.groovestats_api_key.trim().is_empty() {
            continue;
        }
        let passed = !gs.players[player_idx].is_failing && gs.song_completed_naturally;
        if !passed {
            debug!(
                "Skipping {} submit for {:?} ({}): song was not passed.",
                online::groovestats_service_name(),
                side,
                chart.short_hash
            );
            continue;
        }

        let Some(payload) = groovestats_payload_for_player(gs, player_idx) else {
            continue;
        };
        groovestats_store_submit_retry(GrooveStatsSubmitRetryEntry {
            side,
            slot,
            chart_hash: chart.short_hash.clone(),
            username: profile.groovestats_username.trim().to_string(),
            profile_name: profile.display_name.clone(),
            profile_id: profile::active_local_profile_id_for_side(side),
            itl_score_hundredths: Some(itl::current_score_hundredths(gs, player_idx)),
            show_ex_score: profile.show_ex_score,
            api_key: profile.groovestats_api_key.trim().to_string(),
            payload: payload.clone(),
        });
        let token = groovestats_next_submit_ui_token();
        groovestats_set_submit_ui_status(
            side,
            chart.short_hash.as_str(),
            token,
            GrooveStatsSubmitUiStatus::Submitting,
        );
        groovestats_arm_submit_event_ui(side, chart.short_hash.as_str(), token);
        players.push(GrooveStatsSubmitPlayerJob {
            side,
            slot,
            chart_hash: chart.short_hash.clone(),
            username: profile.groovestats_username.trim().to_string(),
            profile_name: profile.display_name.clone(),
            profile_id: profile::active_local_profile_id_for_side(side),
            token,
            itl_score_hundredths: Some(itl::current_score_hundredths(gs, player_idx)),
            show_ex_score: profile.show_ex_score,
        });
        headers.push((
            format!("x-api-key-player-{slot}"),
            profile.groovestats_api_key.trim().to_string(),
        ));
        query.push((format!("chartHashP{slot}"), chart.short_hash.clone()));
        body.insert(
            format!("player{slot}"),
            serde_json::to_value(payload).expect("serialize GrooveStats submit payload"),
        );
    }

    if players.is_empty() {
        return;
    }

    let job = GrooveStatsSubmitRequest {
        players,
        headers,
        query,
        body: JsonValue::Object(body),
    };
    spawn_groovestats_submit(job);
}

pub fn retry_timed_out_groovestats_submit(chart_hash: &str, side: profile::PlayerSide) -> bool {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return false;
    }
    let cfg = crate::config::get();
    if !cfg.enable_groovestats {
        return false;
    }
    let online::ConnectionStatus::Connected(services) = online::get_status() else {
        return false;
    };
    if !services.auto_submit {
        return false;
    }
    let Some(status) = get_groovestats_submit_ui_status_for_side(hash, side) else {
        return false;
    };
    if !groovestats_can_retry_submit(status) {
        return false;
    }
    let Some(entry) = GROOVESTATS_SUBMIT_RETRY.lock().unwrap()[submit_side_ix(side)]
        .as_ref()
        .filter(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
        .cloned()
    else {
        return false;
    };

    let token = groovestats_next_submit_ui_token();
    groovestats_set_submit_ui_status(side, hash, token, GrooveStatsSubmitUiStatus::Submitting);
    groovestats_arm_submit_event_ui(side, hash, token);
    debug!(
        "Retrying {} submit for {:?} ({}).",
        online::groovestats_service_name(),
        side,
        hash
    );
    spawn_groovestats_submit(groovestats_retry_request(&entry, token));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::chart::{ChartData, StaminaCounts};
    use crate::game::scroll::ScrollSpeedSetting;
    use rssp::{TechCounts, stats::ArrowStats};
    use serde_json::json;

    fn sample_chart(chart_type: &str) -> ChartData {
        ChartData {
            chart_type: chart_type.to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 12,
            step_artist: String::new(),
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
            has_significant_timing_changes: true,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 12,
            display_bpm: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
        }
    }

    #[test]
    fn groovestats_payload_serializes_old_api_shape() {
        let payload = GrooveStatsSubmitPlayerPayload {
            rate: 150,
            score: 9_975,
            judgment_counts: GrooveStatsJudgmentCounts {
                fantastic_plus: 7,
                fantastic: 12,
                excellent: 18,
                great: 4,
                decent: 1,
                way_off: 0,
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
        };

        let value = serde_json::to_value(&payload).expect("serialize GrooveStats submit payload");
        assert_eq!(value["rate"], json!(150));
        assert_eq!(value["score"], json!(9_975));
        assert_eq!(value["judgmentCounts"]["fantasticPlus"], json!(7));
        assert_eq!(value["judgmentCounts"]["totalMines"], json!(8));
        assert_eq!(value["rescoreCounts"]["wayOff"], json!(6));
        assert_eq!(value["usedCmod"], json!(true));
        assert_eq!(value["comment"], json!("[DS], FA+, 99.50EX, 2w, 1m, C650"));
    }

    #[test]
    fn groovestats_manual_qr_url_preserves_base_url_case() {
        let counts = GrooveStatsJudgmentCounts {
            fantastic_plus: 0x0a,
            fantastic: 0x0b,
            excellent: 0x0c,
            great: 0x0d,
            decent: 0x0e,
            way_off: 0x0f,
            miss: 0x10,
            total_steps: 0x1d,
            holds_held: 0x11,
            total_holds: 0x12,
            mines_hit: 0x15,
            total_mines: 0x16,
            rolls_held: 0x13,
            total_rolls: 0x14,
        };
        let rescored = GrooveStatsRescoreCounts {
            fantastic_plus: 0x01,
            fantastic: 0x02,
            excellent: 0x03,
            great: 0x04,
            decent: 0x05,
            way_off: 0x06,
        };

        let url = groovestats_manual_qr_url(
            "https://www.groovestats.com",
            "deadbeef",
            3,
            &counts,
            &rescored,
            true,
            150,
            true,
        )
        .expect("manual qr url");

        assert_eq!(
            url,
            "https://www.groovestats.com/QR/deadbeef/T1dGaHbIcJdKeLfM10H11T12R13T14M15T16G1H2I3J4K5L6/F1R96C1V3"
        );
    }

    #[test]
    fn groovestats_validity_allows_cmod_and_no_mines() {
        let mut profile = Profile::default();
        profile.scroll_speed = ScrollSpeedSetting::CMod(650.0);
        profile.remove_active_mask = 1u8 << 1;

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

    fn sample_submit_job(show_ex_score: bool) -> GrooveStatsSubmitPlayerJob {
        GrooveStatsSubmitPlayerJob {
            side: profile::PlayerSide::P1,
            slot: 1,
            chart_hash: "deadbeef".to_string(),
            username: "PerfectTaste".to_string(),
            profile_name: "PerfectTaste".to_string(),
            profile_id: None,
            token: 1,
            itl_score_hundredths: None,
            show_ex_score,
        }
    }

    fn sample_submit_entry(rank: u32, is_self: bool) -> super::super::LeaderboardApiEntry {
        super::super::LeaderboardApiEntry {
            rank,
            name: "PerfectTaste".to_string(),
            machine_tag: None,
            score: 9999.0,
            date: String::new(),
            is_rival: false,
            is_self,
            is_fail: false,
            comments: None,
        }
    }

    fn sample_submit_response(
        result: &str,
        gs_leaderboard: Vec<super::super::LeaderboardApiEntry>,
        ex_leaderboard: Vec<super::super::LeaderboardApiEntry>,
    ) -> GrooveStatsSubmitApiPlayer {
        GrooveStatsSubmitApiPlayer {
            chart_hash: "deadbeef".to_string(),
            result: result.to_string(),
            gs_leaderboard,
            ex_leaderboard,
            rpg: None,
            itl: None,
        }
    }

    #[test]
    fn submit_record_banner_returns_world_record_for_top_gs_rank() {
        let banner = submit_record_banner(
            &sample_submit_job(false),
            &sample_submit_response("improved", vec![sample_submit_entry(1, true)], Vec::new()),
        );

        assert_eq!(banner, Some(GrooveStatsSubmitRecordBanner::WorldRecord));
    }

    #[test]
    fn submit_record_banner_prefers_ex_leaderboard_for_ex_mode() {
        let banner = submit_record_banner(
            &sample_submit_job(true),
            &sample_submit_response(
                "score-added",
                vec![sample_submit_entry(2, true)],
                vec![sample_submit_entry(1, true)],
            ),
        );

        assert_eq!(banner, Some(GrooveStatsSubmitRecordBanner::WorldRecordEx));
    }

    #[test]
    fn submit_record_banner_falls_back_to_personal_best() {
        let banner = submit_record_banner(
            &sample_submit_job(true),
            &sample_submit_response(
                "improved",
                vec![sample_submit_entry(3, true)],
                vec![sample_submit_entry(4, true)],
            ),
        );

        assert_eq!(banner, Some(GrooveStatsSubmitRecordBanner::PersonalBest));
    }

    #[test]
    fn submit_record_banner_ignores_non_improving_results() {
        let banner = submit_record_banner(
            &sample_submit_job(false),
            &sample_submit_response(
                "score-already-submitted",
                vec![sample_submit_entry(1, true)],
                Vec::new(),
            ),
        );

        assert_eq!(banner, None);
    }

    #[test]
    fn groovestats_retry_only_allows_timeouts() {
        assert!(!groovestats_can_retry_submit(
            GrooveStatsSubmitUiStatus::Submitting
        ));
        assert!(!groovestats_can_retry_submit(
            GrooveStatsSubmitUiStatus::Submitted
        ));
        assert!(!groovestats_can_retry_submit(
            GrooveStatsSubmitUiStatus::SubmitFailed
        ));
        assert!(groovestats_can_retry_submit(
            GrooveStatsSubmitUiStatus::TimedOut
        ));
    }
}
