use chrono::{Local, TimeZone};
use deadsync_core::input::InputSource;
use deadsync_core::song_time::{SongTimeNs, song_time_ns_invalid};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::{LocalReplayEdge, ScoreImportEndpoint};

pub const PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct LeaderboardEntry {
    pub rank: u32,
    pub name: String,
    pub machine_tag: Option<String>,
    pub score: f64,
    pub date: String,
    pub is_rival: bool,
    pub is_self: bool,
    pub is_fail: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ReplayEdge {
    pub event_music_time_ns: SongTimeNs,
    pub lane_index: u8,
    pub pressed: bool,
    pub source: InputSource,
}

#[derive(Debug, Clone)]
pub struct MachineReplayEntry {
    pub rank: u32,
    pub name: String,
    pub score: f64,
    pub date: String,
    pub is_fail: bool,
    pub replay_beat0_time_ns: SongTimeNs,
    pub replay: Vec<ReplayEdge>,
}

#[derive(Debug)]
pub struct MachineLeaderboardPlay {
    pub name: String,
    pub machine_tag: Option<String>,
    pub score_percent: f64,
    pub played_at_ms: i64,
    pub is_fail: bool,
}

#[derive(Debug)]
pub struct MachineReplayPlay {
    pub initials: String,
    pub score_percent: f64,
    pub played_at_ms: i64,
    pub is_fail: bool,
    pub replay_beat0_time_ns: SongTimeNs,
    pub replay: Vec<LocalReplayEdge>,
}

fn local_score_date_string(played_at_ms: i64) -> String {
    let Some(dt) = Local.timestamp_millis_opt(played_at_ms).single() else {
        return String::new();
    };
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

const LEADERBOARD_MONTH_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn format_leaderboard_date_with_empty(date: &str, empty_text: &str) -> String {
    let trimmed = date.trim();
    if trimmed.is_empty() {
        return empty_text.to_string();
    }

    let ymd = trimmed.split_once(' ').map_or(trimmed, |(value, _)| value);
    let ymd = ymd.split_once('T').map_or(ymd, |(value, _)| value);
    let mut parts = ymd.split('-');
    let (Some(year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next()) else {
        return trimmed.to_string();
    };

    let Some(month_idx) = month
        .parse::<usize>()
        .ok()
        .and_then(|m| m.checked_sub(1))
        .filter(|m| *m < LEADERBOARD_MONTH_ABBR.len())
    else {
        return trimmed.to_string();
    };
    let Some(day_num) = day.parse::<u32>().ok().filter(|d| *d > 0) else {
        return trimmed.to_string();
    };

    format!(
        "{} {}, {}",
        LEADERBOARD_MONTH_ABBR[month_idx], day_num, year
    )
}

pub fn format_leaderboard_date(date: &str) -> String {
    format_leaderboard_date_with_empty(date, "")
}

pub fn format_leaderboard_date_or_placeholder(date: &str) -> String {
    format_leaderboard_date_with_empty(date, "----------")
}

#[inline(always)]
pub fn scorebox_machine_tag(machine_tag: Option<&str>, name: &str) -> String {
    let src = machine_tag.unwrap_or(name).trim();
    if src.is_empty() {
        return "----".to_string();
    }
    let mut out = String::with_capacity(4);
    for ch in src.chars().take(4) {
        out.push(ch.to_ascii_uppercase());
    }
    out
}

#[inline(always)]
pub fn scorebox_score_percent(score_10000: f64) -> f64 {
    if score_10000.is_finite() {
        (score_10000 / 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    }
}

#[inline(always)]
pub fn format_scorebox_score_percent(score_10000: f64) -> String {
    format!("{:.2}%", scorebox_score_percent(score_10000))
}

#[inline(always)]
pub fn format_scorebox_score_value(score_10000: f64) -> String {
    format!("{:.2}", scorebox_score_percent(score_10000))
}

#[inline(always)]
pub fn format_scorebox_rank(rank: u32) -> String {
    format!("{rank}.")
}

#[inline(always)]
fn same_leaderboard_entry(a: &LeaderboardEntry, b: &LeaderboardEntry) -> bool {
    a.rank == b.rank && a.name.eq_ignore_ascii_case(b.name.as_str())
}

#[inline(always)]
fn selected_contains(selected: &[&LeaderboardEntry], entry: &LeaderboardEntry) -> bool {
    selected
        .iter()
        .any(|chosen| same_leaderboard_entry(chosen, entry))
}

fn next_prioritized_entry<'a>(
    entries: &'a [LeaderboardEntry],
    selected: &[&'a LeaderboardEntry],
    include: impl Fn(&LeaderboardEntry) -> bool,
) -> Option<&'a LeaderboardEntry> {
    entries
        .iter()
        .filter(|entry| include(entry) && !selected_contains(selected, entry))
        .min_by_key(|entry| entry.rank)
}

pub fn prioritized_leaderboard_entry_refs(
    entries: &[LeaderboardEntry],
    max_rows: usize,
) -> Vec<&LeaderboardEntry> {
    if max_rows == 0 {
        return Vec::new();
    }
    if entries.len() <= max_rows {
        return entries.iter().collect();
    }

    let mut selected = Vec::with_capacity(max_rows);
    if let Some(top) = next_prioritized_entry(entries, selected.as_slice(), |_| true) {
        selected.push(top);
    }
    if let Some(self_entry) =
        next_prioritized_entry(entries, selected.as_slice(), |entry| entry.is_self)
    {
        selected.push(self_entry);
    }
    while selected.len() < max_rows {
        let Some(rival) =
            next_prioritized_entry(entries, selected.as_slice(), |entry| entry.is_rival)
        else {
            break;
        };
        selected.push(rival);
    }
    while selected.len() < max_rows {
        let Some(entry) = next_prioritized_entry(entries, selected.as_slice(), |_| true) else {
            break;
        };
        selected.push(entry);
    }
    selected.sort_unstable_by_key(|entry| entry.rank);
    selected
}

pub fn prioritized_leaderboard_entries(
    entries: &[LeaderboardEntry],
    max_rows: usize,
) -> Vec<LeaderboardEntry> {
    prioritized_leaderboard_entry_refs(entries, max_rows)
        .into_iter()
        .cloned()
        .collect()
}

pub fn machine_leaderboard_entries(
    mut plays: Vec<MachineLeaderboardPlay>,
    max_entries: usize,
) -> Vec<LeaderboardEntry> {
    plays.sort_by(|a, b| {
        b.score_percent
            .partial_cmp(&a.score_percent)
            .unwrap_or(Ordering::Equal)
            .then_with(|| b.played_at_ms.cmp(&a.played_at_ms))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.machine_tag.cmp(&b.machine_tag))
    });

    let take_len = max_entries.min(plays.len());
    let mut out = Vec::with_capacity(take_len);
    for (i, play) in plays.into_iter().take(take_len).enumerate() {
        out.push(LeaderboardEntry {
            rank: (i as u32).saturating_add(1),
            name: play.name,
            machine_tag: play.machine_tag,
            score: (play.score_percent * 10000.0).round(),
            date: local_score_date_string(play.played_at_ms),
            is_rival: false,
            is_self: false,
            is_fail: play.is_fail,
        });
    }
    out
}

pub fn machine_replay_entries(
    mut plays: Vec<MachineReplayPlay>,
    max_entries: usize,
) -> Vec<MachineReplayEntry> {
    plays.sort_by(|a, b| {
        b.score_percent
            .partial_cmp(&a.score_percent)
            .unwrap_or(Ordering::Equal)
            .then_with(|| b.played_at_ms.cmp(&a.played_at_ms))
            .then_with(|| a.initials.cmp(&b.initials))
    });

    let take_len = max_entries.min(plays.len());
    let mut out = Vec::with_capacity(take_len);
    for (i, play) in plays.into_iter().take(take_len).enumerate() {
        let mut replay = Vec::with_capacity(play.replay.len());
        for edge in play.replay {
            if song_time_ns_invalid(edge.event_music_time_ns) {
                continue;
            }
            replay.push(ReplayEdge {
                event_music_time_ns: edge.event_music_time_ns,
                lane_index: edge.lane,
                pressed: edge.pressed,
                source: edge.input_source(),
            });
        }
        out.push(MachineReplayEntry {
            rank: (i as u32).saturating_add(1),
            name: play.initials,
            score: (play.score_percent * 10000.0).round(),
            date: local_score_date_string(play.played_at_ms),
            is_fail: play.is_fail,
            replay_beat0_time_ns: play.replay_beat0_time_ns,
            replay,
        });
    }
    out
}

#[inline(always)]
pub fn leaderboard_username_matches(entry_name: &str, username: &str) -> bool {
    !username.trim().is_empty() && entry_name.eq_ignore_ascii_case(username)
}

#[inline(always)]
pub fn leaderboard_score_10000(score: f64, is_fail: bool) -> Option<f64> {
    if is_fail || !score.is_finite() {
        None
    } else {
        Some(score.clamp(0.0, 10000.0))
    }
}

#[inline(always)]
pub const fn leaderboard_nonzero_rank(rank: u32) -> Option<u32> {
    if rank == 0 { None } else { Some(rank) }
}

#[inline(always)]
pub fn score_import_entry_matches_profile(
    entry_name: &str,
    is_self: bool,
    endpoint: ScoreImportEndpoint,
    username: &str,
) -> bool {
    if is_self {
        return true;
    }
    endpoint.requires_username() && entry_name.eq_ignore_ascii_case(username)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowCloudPaneKind {
    Itg,
    Ex,
    HardEx,
}

pub fn arrowcloud_pane_kind_from_type(lb_type: &str) -> Option<ArrowCloudPaneKind> {
    if lb_type.is_empty() {
        return None;
    }
    let mut compact = String::with_capacity(lb_type.len());
    for ch in lb_type.chars() {
        if ch.is_ascii_alphanumeric() {
            compact.push(ch.to_ascii_lowercase());
        }
    }
    match compact.as_str() {
        "itg" => Some(ArrowCloudPaneKind::Itg),
        "ex" => Some(ArrowCloudPaneKind::Ex),
        "hardex" | "hex" => Some(ArrowCloudPaneKind::HardEx),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArrowCloudUserContext {
    pub self_user_id: Option<String>,
    pub rival_user_ids: HashSet<String>,
}

pub fn arrowcloud_user_id(raw: &str) -> Option<&str> {
    let user_id = raw.trim();
    (!user_id.is_empty()).then_some(user_id)
}

pub fn arrowcloud_target_user_ids(context: Option<&ArrowCloudUserContext>) -> HashSet<String> {
    let Some(context) = context else {
        return HashSet::new();
    };
    let mut out = HashSet::with_capacity(
        usize::from(context.self_user_id.is_some()) + context.rival_user_ids.len(),
    );
    if let Some(self_user_id) = context.self_user_id.as_ref() {
        out.insert(self_user_id.clone());
    }
    out.extend(context.rival_user_ids.iter().cloned());
    out
}

pub fn arrowcloud_entry_flags(
    entry_user_id: Option<&str>,
    entry_is_self: bool,
    entry_is_rival: bool,
    context: Option<&ArrowCloudUserContext>,
) -> (bool, bool) {
    let is_self = entry_is_self
        || context
            .and_then(|context| context.self_user_id.as_deref())
            .is_some_and(|self_user_id| entry_user_id == Some(self_user_id));
    let is_rival = entry_is_rival
        || context.is_some_and(|context| {
            entry_user_id.is_some_and(|user_id| context.rival_user_ids.contains(user_id))
        });
    (is_self, is_rival)
}

pub fn arrowcloud_leaderboard_entry(
    rank: u32,
    alias: String,
    score_percent: f64,
    date: String,
    is_self: bool,
    is_rival: bool,
) -> LeaderboardEntry {
    let score = if score_percent.is_finite() {
        (score_percent * 100.0).clamp(0.0, 10000.0)
    } else {
        0.0
    };
    LeaderboardEntry {
        rank,
        name: alias,
        machine_tag: None,
        score,
        date,
        is_rival,
        is_self,
        is_fail: false,
    }
}

#[derive(Debug, Clone)]
pub struct LeaderboardPane {
    pub name: String,
    pub entries: Vec<LeaderboardEntry>,
    pub is_ex: bool,
    pub disabled: bool,
    pub personalized: bool,
    pub arrowcloud_kind: Option<ArrowCloudPaneKind>,
}

impl LeaderboardPane {
    #[inline(always)]
    pub fn is_groovestats(&self) -> bool {
        self.name.eq_ignore_ascii_case("GrooveStats")
    }

    #[inline(always)]
    pub fn is_arrowcloud(&self) -> bool {
        self.arrowcloud_kind.is_some() || self.name.eq_ignore_ascii_case("ArrowCloud")
    }

    #[inline(always)]
    pub fn is_hard_ex(&self) -> bool {
        self.arrowcloud_kind == Some(ArrowCloudPaneKind::HardEx)
            || (self.arrowcloud_kind.is_none() && self.name.eq_ignore_ascii_case("ArrowCloud"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScoreboxPaneKind {
    Gs,
    Ex,
    HardEx,
    Srpg,
    Itl,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectMusicScoreboxFilter {
    pub itg: bool,
    pub ex: bool,
    pub hard_ex: bool,
    pub tournaments: bool,
}

impl Default for SelectMusicScoreboxFilter {
    fn default() -> Self {
        Self {
            itg: true,
            ex: true,
            hard_ex: true,
            tournaments: true,
        }
    }
}

#[inline(always)]
pub const fn select_music_scorebox_filter_has_any(filter: SelectMusicScoreboxFilter) -> bool {
    filter.itg || filter.ex || filter.hard_ex || filter.tournaments
}

#[inline(always)]
pub const fn select_music_scorebox_filter_allows_kind(
    kind: ScoreboxPaneKind,
    filter: SelectMusicScoreboxFilter,
) -> bool {
    match kind {
        ScoreboxPaneKind::Gs => filter.itg,
        ScoreboxPaneKind::Ex => filter.ex,
        ScoreboxPaneKind::HardEx => filter.hard_ex,
        ScoreboxPaneKind::Srpg | ScoreboxPaneKind::Itl | ScoreboxPaneKind::Other => {
            filter.tournaments
        }
    }
}

pub fn select_music_scorebox_filtered_panes(
    panes: &[LeaderboardPane],
    filter: SelectMusicScoreboxFilter,
) -> Vec<&LeaderboardPane> {
    let mut out = Vec::with_capacity(panes.len());
    for pane in panes {
        if select_music_scorebox_filter_allows_kind(scorebox_pane_kind(pane), filter) {
            out.push(pane);
        }
    }
    out
}

#[inline(always)]
pub fn scorebox_pane_kind(pane: &LeaderboardPane) -> ScoreboxPaneKind {
    if let Some(kind) = known_scorebox_pane_kind(pane) {
        return kind;
    }
    if contains_ignore_ascii_case(&pane.name, "srpg")
        || contains_ignore_ascii_case(&pane.name, "rpg")
    {
        ScoreboxPaneKind::Srpg
    } else if contains_ignore_ascii_case(&pane.name, "itl") {
        ScoreboxPaneKind::Itl
    } else if pane.is_ex {
        ScoreboxPaneKind::Ex
    } else {
        ScoreboxPaneKind::Other
    }
}

fn known_scorebox_pane_kind(pane: &LeaderboardPane) -> Option<ScoreboxPaneKind> {
    if pane.is_arrowcloud() {
        Some(if pane.is_hard_ex() {
            ScoreboxPaneKind::HardEx
        } else if pane.is_ex {
            ScoreboxPaneKind::Ex
        } else {
            ScoreboxPaneKind::Gs
        })
    } else if pane.is_groovestats() {
        Some(if pane.is_ex {
            ScoreboxPaneKind::Ex
        } else {
            ScoreboxPaneKind::Gs
        })
    } else {
        None
    }
}

#[inline]
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let needle = needle.as_bytes();
    needle.is_empty()
        || haystack
            .as_bytes()
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[inline(always)]
pub fn scorebox_pane_kind_legacy_for_bench(pane: &LeaderboardPane) -> ScoreboxPaneKind {
    if let Some(kind) = known_scorebox_pane_kind(pane) {
        return kind;
    }
    let lower = pane.name.to_ascii_lowercase();
    if lower.contains("srpg") || lower.contains("rpg") {
        ScoreboxPaneKind::Srpg
    } else if lower.contains("itl") {
        ScoreboxPaneKind::Itl
    } else if pane.is_ex {
        ScoreboxPaneKind::Ex
    } else {
        ScoreboxPaneKind::Other
    }
}

#[inline(always)]
pub fn scorebox_pane_mode_text(kind: ScoreboxPaneKind, pane: &LeaderboardPane) -> &str {
    match kind {
        ScoreboxPaneKind::Gs => "ITG",
        ScoreboxPaneKind::Ex => "EX",
        ScoreboxPaneKind::HardEx => "H.EX",
        ScoreboxPaneKind::Srpg => "SRPG",
        ScoreboxPaneKind::Itl => "ITL",
        ScoreboxPaneKind::Other => pane.name.as_str(),
    }
}

#[inline(always)]
pub const fn default_scorebox_mode_text(show_ex_score: bool) -> &'static str {
    if show_ex_score { "EX" } else { "ITG" }
}

pub fn preferred_primary_scorebox_pane<'a>(
    panes: &'a [&'a LeaderboardPane],
    show_ex: bool,
) -> Option<&'a LeaderboardPane> {
    let want = if show_ex {
        ScoreboxPaneKind::Ex
    } else {
        ScoreboxPaneKind::Gs
    };
    panes
        .iter()
        .copied()
        .find(|pane| scorebox_pane_kind(pane) == want)
        .or_else(|| {
            panes
                .iter()
                .copied()
                .find(|pane| scorebox_pane_kind(pane) == ScoreboxPaneKind::Gs)
        })
        .or_else(|| {
            panes
                .iter()
                .copied()
                .find(|pane| scorebox_pane_kind(pane) == ScoreboxPaneKind::Ex)
        })
        .or_else(|| panes.first().copied())
}

pub fn leaderboard_pane(
    name: &str,
    entries: Vec<LeaderboardEntry>,
    is_ex: bool,
) -> Option<LeaderboardPane> {
    if entries.is_empty() {
        return None;
    }
    Some(LeaderboardPane {
        name: name.to_string(),
        entries,
        is_ex,
        disabled: false,
        personalized: true,
        arrowcloud_kind: None,
    })
}

pub fn arrowcloud_hard_ex_leaderboard_pane(
    entries: Vec<LeaderboardEntry>,
    personalized: bool,
) -> LeaderboardPane {
    LeaderboardPane {
        name: "ArrowCloud".to_string(),
        entries,
        is_ex: false,
        disabled: false,
        personalized,
        arrowcloud_kind: Some(ArrowCloudPaneKind::HardEx),
    }
}

#[inline(always)]
pub fn arrowcloud_empty_hard_ex_leaderboard_pane() -> LeaderboardPane {
    arrowcloud_hard_ex_leaderboard_pane(Vec::new(), false)
}

#[derive(Debug, Clone)]
pub struct PlayerLeaderboardData {
    pub panes: Vec<LeaderboardPane>,
    pub srpg_self_score: Option<u32>,
    pub itl_self_score: Option<u32>,
    pub itl_self_rank: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum PlayerLeaderboardCacheValue {
    Ready(Arc<PlayerLeaderboardData>),
    Error(Arc<str>),
}

#[derive(Debug, Clone)]
pub struct PlayerLeaderboardCacheEntry {
    pub value: PlayerLeaderboardCacheValue,
    pub max_entries: usize,
    pub refreshed_at: Instant,
    pub retry_after: Option<Instant>,
}

#[derive(Default)]
pub struct PlayerLeaderboardCacheState {
    pub by_key: hashbrown::HashMap<PlayerLeaderboardCacheKey, PlayerLeaderboardCacheEntry>,
    pub in_flight: HashMap<PlayerLeaderboardCacheKey, usize>,
    pub pending_refresh: HashMap<PlayerLeaderboardCacheKey, usize>,
    pub invalidated_after: HashMap<PlayerLeaderboardCacheKey, Instant>,
}

pub struct PlayerLeaderboardRequestDecision {
    pub snapshot: CachedPlayerLeaderboardData,
    pub should_spawn: bool,
    pub requested_max_entries: usize,
}

pub struct PlayerLeaderboardFetchRequest {
    pub key: PlayerLeaderboardCacheKey,
    pub gs_username: String,
    pub persistent_profile_id: Option<String>,
    pub auto_profile_id: Option<String>,
    pub should_auto_populate: bool,
    pub max_entries: usize,
}

pub struct PlayerLeaderboardRequestPlan {
    pub snapshot: CachedPlayerLeaderboardData,
    pub fetch: Option<PlayerLeaderboardFetchRequest>,
}

pub struct PlayerLeaderboardFetchSuccess<T> {
    pub data: PlayerLeaderboardData,
    pub imported_score: Option<T>,
    pub itl_self_found: bool,
}

pub struct QueuedPlayerLeaderboardFetch {
    pub key: PlayerLeaderboardCacheKey,
    pub max_entries: usize,
}

pub struct PlayerLeaderboardFetchCompletion<T> {
    pub fetched_itl_self: Option<(Option<u32>, Option<u32>)>,
    pub fetched_srpg_self_score: Option<u32>,
    pub fetched_imported_score: Option<T>,
    pub queued_fetch: Option<QueuedPlayerLeaderboardFetch>,
}

pub struct PlayerLeaderboardFetchJobResult<T> {
    pub key: PlayerLeaderboardCacheKey,
    pub gs_username: String,
    pub persistent_profile_id: Option<String>,
    pub auto_profile_id: Option<String>,
    pub should_auto_populate: bool,
    pub completion: PlayerLeaderboardFetchCompletion<T>,
}

impl PlayerLeaderboardCacheState {
    pub fn request_leaderboard(
        &mut self,
        key: &PlayerLeaderboardCacheKey,
        max_entries: usize,
        refresh_cached: bool,
        now: Instant,
    ) -> PlayerLeaderboardRequestDecision {
        let entry = self.by_key.get(key);
        let requested_max_entries =
            entry.map_or(max_entries, |entry| max_entries.max(entry.max_entries));
        let snapshot = entry.map_or_else(
            player_leaderboard_loading_snapshot,
            player_leaderboard_snapshot_from_entry,
        );

        let mut should_spawn = false;
        if should_fetch_player_leaderboard_entry(entry, requested_max_entries, refresh_cached, now)
        {
            if let Some(in_flight_max_entries) = self.in_flight.get(key).copied() {
                if should_rerun_in_flight_player_leaderboard_fetch(
                    in_flight_max_entries,
                    requested_max_entries,
                    refresh_cached,
                ) {
                    queue_player_leaderboard_refresh(
                        &mut self.pending_refresh,
                        key,
                        requested_max_entries,
                    );
                }
            } else {
                self.in_flight.insert(key.clone(), requested_max_entries);
                should_spawn = true;
            }
        }

        PlayerLeaderboardRequestDecision {
            snapshot,
            should_spawn,
            requested_max_entries,
        }
    }

    pub fn invalidate_chart_for_api(
        &mut self,
        api_key: &str,
        chart_hash: &str,
        invalidated_at: Instant,
    ) {
        let matching_keys: HashSet<PlayerLeaderboardCacheKey> = self
            .by_key
            .keys()
            .chain(self.in_flight.keys())
            .chain(self.pending_refresh.keys())
            .chain(self.invalidated_after.keys())
            .filter(|key| key.api_key == api_key && key.chart_hash.eq_ignore_ascii_case(chart_hash))
            .cloned()
            .collect();
        for key in matching_keys {
            self.by_key.remove(&key);
            self.in_flight.remove(&key);
            self.pending_refresh.remove(&key);
            self.invalidated_after.insert(key, invalidated_at);
        }
    }

    pub fn complete_fetch<T>(
        &mut self,
        key: &PlayerLeaderboardCacheKey,
        requested_max_entries: usize,
        request_started_at: Instant,
        refresh_finished_at: Instant,
        error_retry_interval: std::time::Duration,
        fetched: Result<PlayerLeaderboardFetchSuccess<T>, String>,
        should_auto_populate: bool,
        auto_profile_id_exists: bool,
    ) -> PlayerLeaderboardFetchCompletion<T> {
        self.in_flight.remove(key);
        let request_invalidated = player_leaderboard_request_was_invalidated(
            self.invalidated_after.get(key).copied(),
            request_started_at,
        );

        let mut fetched_itl_self = None;
        let mut fetched_srpg_self_score = None;
        let mut fetched_imported_score = None;
        if !request_invalidated {
            match fetched {
                Ok(fetched) => {
                    if !should_keep_newer_player_leaderboard_entry(
                        self.by_key.get(key),
                        request_started_at,
                    ) {
                        let PlayerLeaderboardFetchSuccess {
                            data,
                            imported_score,
                            itl_self_found,
                        } = fetched;
                        if itl_self_found {
                            fetched_itl_self = Some((data.itl_self_score, data.itl_self_rank));
                        }
                        fetched_srpg_self_score = data.srpg_self_score;
                        if should_auto_populate && auto_profile_id_exists {
                            fetched_imported_score = imported_score;
                        }
                        self.by_key.insert(
                            key.clone(),
                            PlayerLeaderboardCacheEntry {
                                value: PlayerLeaderboardCacheValue::Ready(Arc::new(data)),
                                max_entries: requested_max_entries,
                                refreshed_at: refresh_finished_at,
                                retry_after: None,
                            },
                        );
                        self.invalidated_after.remove(key);
                    }
                }
                Err(error) => {
                    if !should_keep_newer_player_leaderboard_entry(
                        self.by_key.get(key),
                        request_started_at,
                    ) {
                        let retry_after = Some(refresh_finished_at + error_retry_interval);
                        if let Some(entry) = self.by_key.get_mut(key)
                            && matches!(entry.value, PlayerLeaderboardCacheValue::Ready(_))
                        {
                            entry.refreshed_at = refresh_finished_at;
                            entry.retry_after = retry_after;
                        } else {
                            self.by_key.insert(
                                key.clone(),
                                PlayerLeaderboardCacheEntry {
                                    value: PlayerLeaderboardCacheValue::Error(error.into()),
                                    max_entries: requested_max_entries,
                                    refreshed_at: refresh_finished_at,
                                    retry_after,
                                },
                            );
                        }
                        self.invalidated_after.remove(key);
                    }
                }
            }
        }

        let queued_fetch = self.pending_refresh.remove(key).map(|max_entries| {
            self.in_flight.insert(key.clone(), max_entries);
            QueuedPlayerLeaderboardFetch {
                key: key.clone(),
                max_entries,
            }
        });

        PlayerLeaderboardFetchCompletion {
            fetched_itl_self,
            fetched_srpg_self_score,
            fetched_imported_score,
            queued_fetch,
        }
    }
}

static RUNTIME_PLAYER_LEADERBOARD_CACHE: LazyLock<Mutex<PlayerLeaderboardCacheState>> =
    LazyLock::new(|| Mutex::new(PlayerLeaderboardCacheState::default()));

pub fn runtime_request_player_leaderboard(
    key: &PlayerLeaderboardCacheKey,
    max_entries: usize,
    refresh_cached: bool,
    now: Instant,
) -> PlayerLeaderboardRequestDecision {
    RUNTIME_PLAYER_LEADERBOARD_CACHE
        .lock()
        .unwrap()
        .request_leaderboard(key, max_entries, refresh_cached, now)
}

pub fn runtime_plan_player_leaderboard_request(
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
    max_entries: usize,
    refresh_cached: bool,
    now: Instant,
) -> Option<PlayerLeaderboardRequestPlan> {
    if max_entries == 0 {
        return None;
    }
    let key = player_leaderboard_cache_key(chart_hash, profile_snapshot)?;
    let gs_username = profile_snapshot.gs_username().to_string();
    let persistent_profile_id = profile_snapshot.persistent_profile_id().map(str::to_string);
    let auto_profile_id = profile_snapshot.auto_profile_id().map(str::to_string);
    let should_auto_populate = profile_snapshot.should_auto_populate();
    let decision = runtime_request_player_leaderboard(&key, max_entries, refresh_cached, now);
    let fetch = decision
        .should_spawn
        .then(|| PlayerLeaderboardFetchRequest {
            key,
            gs_username,
            persistent_profile_id,
            auto_profile_id,
            should_auto_populate,
            max_entries: decision.requested_max_entries,
        });

    Some(PlayerLeaderboardRequestPlan {
        snapshot: decision.snapshot,
        fetch,
    })
}

pub fn runtime_complete_player_leaderboard_fetch<T>(
    key: &PlayerLeaderboardCacheKey,
    requested_max_entries: usize,
    request_started_at: Instant,
    refresh_finished_at: Instant,
    error_retry_interval: std::time::Duration,
    fetched: Result<PlayerLeaderboardFetchSuccess<T>, String>,
    should_auto_populate: bool,
    auto_profile_id_exists: bool,
) -> PlayerLeaderboardFetchCompletion<T> {
    RUNTIME_PLAYER_LEADERBOARD_CACHE
        .lock()
        .unwrap()
        .complete_fetch(
            key,
            requested_max_entries,
            request_started_at,
            refresh_finished_at,
            error_retry_interval,
            fetched,
            should_auto_populate,
            auto_profile_id_exists,
        )
}

pub fn runtime_run_player_leaderboard_fetch<T>(
    request: PlayerLeaderboardFetchRequest,
    mut fetch: impl FnMut(
        &PlayerLeaderboardCacheKey,
        &str,
        usize,
    ) -> Result<PlayerLeaderboardFetchSuccess<T>, String>,
) -> PlayerLeaderboardFetchJobResult<T> {
    let PlayerLeaderboardFetchRequest {
        key,
        gs_username,
        persistent_profile_id,
        auto_profile_id,
        should_auto_populate,
        max_entries,
    } = request;
    let request_started_at = Instant::now();
    let fetched = fetch(&key, gs_username.as_str(), max_entries);
    let refresh_finished_at = Instant::now();
    let completion = runtime_complete_player_leaderboard_fetch(
        &key,
        max_entries,
        request_started_at,
        refresh_finished_at,
        PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL,
        fetched,
        should_auto_populate,
        auto_profile_id.is_some(),
    );

    PlayerLeaderboardFetchJobResult {
        key,
        gs_username,
        persistent_profile_id,
        auto_profile_id,
        should_auto_populate,
        completion,
    }
}

pub fn runtime_cached_player_leaderboard_itl_self_rank(
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
) -> Option<u32> {
    let cache = RUNTIME_PLAYER_LEADERBOARD_CACHE.lock().unwrap();
    cached_player_leaderboard_itl_self_rank(&cache.by_key, chart_hash, profile_snapshot)
}

pub fn runtime_cached_player_leaderboard_srpg_self_score(
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
) -> Option<u32> {
    let cache = RUNTIME_PLAYER_LEADERBOARD_CACHE.lock().unwrap();
    cached_player_leaderboard_srpg_self_score(&cache.by_key, chart_hash, profile_snapshot)
}

pub fn runtime_invalidate_player_leaderboard_chart_for_api(
    api_key: &str,
    chart_hash: &str,
    invalidated_at: Instant,
) {
    RUNTIME_PLAYER_LEADERBOARD_CACHE
        .lock()
        .unwrap()
        .invalidate_chart_for_api(api_key, chart_hash, invalidated_at);
}

pub fn runtime_seed_player_leaderboard_entry(
    key: PlayerLeaderboardCacheKey,
    entry: PlayerLeaderboardCacheEntry,
) {
    RUNTIME_PLAYER_LEADERBOARD_CACHE
        .lock()
        .unwrap()
        .by_key
        .insert(key, entry);
}

pub fn runtime_remove_player_leaderboard_entry(key: &PlayerLeaderboardCacheKey) {
    RUNTIME_PLAYER_LEADERBOARD_CACHE
        .lock()
        .unwrap()
        .by_key
        .remove(key);
}

pub fn runtime_lock_player_leaderboard_cache() -> MutexGuard<'static, PlayerLeaderboardCacheState> {
    RUNTIME_PLAYER_LEADERBOARD_CACHE.lock().unwrap()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlayerLeaderboardCacheKey {
    pub chart_hash: String,
    pub api_key: String,
    pub arrowcloud_api_key: String,
    pub include_arrowcloud: bool,
    pub show_ex_score: bool,
}

/// Borrowed view of [`PlayerLeaderboardCacheKey`] for allocation-free cache
/// probes from per-frame leaderboard rank/score lookups.
#[derive(Hash)]
pub struct PlayerLeaderboardCacheKeyRef<'a> {
    chart_hash: &'a str,
    api_key: &'a str,
    arrowcloud_api_key: &'a str,
    include_arrowcloud: bool,
    show_ex_score: bool,
}

#[derive(Debug, Clone)]
pub struct CachedPlayerLeaderboardData {
    pub loading: bool,
    pub data: Option<Arc<PlayerLeaderboardData>>,
    pub error: Option<Arc<str>>,
}

impl CachedPlayerLeaderboardData {
    #[inline(always)]
    pub const fn loading() -> Self {
        Self {
            loading: true,
            data: None,
            error: None,
        }
    }

    #[inline(always)]
    pub fn ready(data: PlayerLeaderboardData) -> Self {
        Self {
            loading: false,
            data: Some(Arc::new(data)),
            error: None,
        }
    }

    #[inline(always)]
    pub fn error(error: String) -> Self {
        Self {
            loading: false,
            data: None,
            error: Some(error.into()),
        }
    }
}

#[inline(always)]
pub const fn player_leaderboard_loading_snapshot() -> CachedPlayerLeaderboardData {
    CachedPlayerLeaderboardData::loading()
}

#[inline(always)]
pub fn player_leaderboard_snapshot_from_entry(
    entry: &PlayerLeaderboardCacheEntry,
) -> CachedPlayerLeaderboardData {
    match &entry.value {
        PlayerLeaderboardCacheValue::Ready(data) => CachedPlayerLeaderboardData {
            loading: false,
            data: Some(Arc::clone(data)),
            error: None,
        },
        PlayerLeaderboardCacheValue::Error(error) => CachedPlayerLeaderboardData {
            loading: false,
            data: None,
            error: Some(Arc::clone(error)),
        },
    }
}

#[derive(Debug, Clone, Default)]
pub struct GameplayScoreboxProfileSnapshot {
    pub display_scorebox: bool,
    pub gs_active: bool,
    pub show_ex_score: bool,
    api_key: String,
    arrowcloud_api_key: String,
    include_arrowcloud: bool,
    gs_username: String,
    persistent_profile_id: Option<String>,
    auto_profile_id: Option<String>,
    should_auto_populate: bool,
}

impl GameplayScoreboxProfileSnapshot {
    pub fn new(
        display_scorebox: bool,
        gs_active: bool,
        show_ex_score: bool,
        api_key: String,
        arrowcloud_api_key: String,
        include_arrowcloud: bool,
        gs_username: String,
        persistent_profile_id: Option<String>,
        auto_profile_id: Option<String>,
        should_auto_populate: bool,
    ) -> Self {
        Self {
            display_scorebox,
            gs_active,
            show_ex_score,
            api_key,
            arrowcloud_api_key,
            include_arrowcloud,
            gs_username,
            persistent_profile_id,
            auto_profile_id,
            should_auto_populate,
        }
    }

    #[inline(always)]
    pub fn api_key(&self) -> &str {
        self.api_key.as_str()
    }

    #[inline(always)]
    pub fn arrowcloud_api_key(&self) -> &str {
        self.arrowcloud_api_key.as_str()
    }

    #[inline(always)]
    pub const fn include_arrowcloud(&self) -> bool {
        self.include_arrowcloud
    }

    #[inline(always)]
    pub fn gs_username(&self) -> &str {
        self.gs_username.as_str()
    }

    #[inline(always)]
    pub fn persistent_profile_id(&self) -> Option<&str> {
        self.persistent_profile_id.as_deref()
    }

    #[inline(always)]
    pub fn auto_profile_id(&self) -> Option<&str> {
        self.auto_profile_id.as_deref()
    }

    #[inline(always)]
    pub const fn should_auto_populate(&self) -> bool {
        self.should_auto_populate
    }
}

pub fn scorebox_snapshot(
    display_scorebox: bool,
    show_ex_score: bool,
    side_joined: bool,
    enable_groovestats: bool,
    enable_arrowcloud: bool,
    auto_populate_gs_scores: bool,
    api_key: &str,
    arrowcloud_api_key: &str,
    gs_username: &str,
    persistent_profile_id: Option<String>,
) -> GameplayScoreboxProfileSnapshot {
    let api_key = api_key.trim().to_string();
    let arrowcloud_api_key = arrowcloud_api_key.trim().to_string();
    let gs_username = gs_username.trim().to_string();
    let include_arrowcloud = enable_arrowcloud && !arrowcloud_api_key.is_empty();
    let auto_profile_id = if auto_populate_gs_scores {
        persistent_profile_id.clone()
    } else {
        None
    };
    let should_auto_populate =
        auto_populate_gs_scores && auto_profile_id.is_some() && !gs_username.is_empty();
    GameplayScoreboxProfileSnapshot::new(
        display_scorebox,
        enable_groovestats && side_joined && !api_key.is_empty(),
        show_ex_score,
        api_key,
        arrowcloud_api_key,
        include_arrowcloud,
        gs_username,
        persistent_profile_id,
        auto_profile_id,
        should_auto_populate,
    )
}

pub fn player_leaderboard_cache_key(
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
) -> Option<PlayerLeaderboardCacheKey> {
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() || !profile_snapshot.gs_active {
        return None;
    }

    Some(PlayerLeaderboardCacheKey {
        chart_hash: chart_hash.to_string(),
        api_key: profile_snapshot.api_key.clone(),
        arrowcloud_api_key: profile_snapshot.arrowcloud_api_key.clone(),
        include_arrowcloud: profile_snapshot.include_arrowcloud,
        show_ex_score: profile_snapshot.show_ex_score,
    })
}

pub fn player_leaderboard_cache_key_ref<'a>(
    chart_hash: &'a str,
    profile_snapshot: &'a GameplayScoreboxProfileSnapshot,
) -> Option<PlayerLeaderboardCacheKeyRef<'a>> {
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() || !profile_snapshot.gs_active {
        return None;
    }
    Some(PlayerLeaderboardCacheKeyRef {
        chart_hash,
        api_key: profile_snapshot.api_key(),
        arrowcloud_api_key: profile_snapshot.arrowcloud_api_key(),
        include_arrowcloud: profile_snapshot.include_arrowcloud(),
        show_ex_score: profile_snapshot.show_ex_score,
    })
}

impl hashbrown::Equivalent<PlayerLeaderboardCacheKey> for PlayerLeaderboardCacheKeyRef<'_> {
    fn equivalent(&self, key: &PlayerLeaderboardCacheKey) -> bool {
        self.chart_hash == key.chart_hash
            && self.api_key == key.api_key
            && self.arrowcloud_api_key == key.arrowcloud_api_key
            && self.include_arrowcloud == key.include_arrowcloud
            && self.show_ex_score == key.show_ex_score
    }
}

pub fn cached_player_leaderboard_itl_self_rank(
    by_key: &hashbrown::HashMap<PlayerLeaderboardCacheKey, PlayerLeaderboardCacheEntry>,
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
) -> Option<u32> {
    let kref = player_leaderboard_cache_key_ref(chart_hash, profile_snapshot)?;
    let entry = by_key.get(&kref)?;
    let PlayerLeaderboardCacheValue::Ready(data) = &entry.value else {
        return None;
    };
    data.itl_self_rank
}

pub fn cached_player_leaderboard_srpg_self_score(
    by_key: &hashbrown::HashMap<PlayerLeaderboardCacheKey, PlayerLeaderboardCacheEntry>,
    chart_hash: &str,
    profile_snapshot: &GameplayScoreboxProfileSnapshot,
) -> Option<u32> {
    let kref = player_leaderboard_cache_key_ref(chart_hash, profile_snapshot)?;
    let entry = by_key.get(&kref)?;
    let PlayerLeaderboardCacheValue::Ready(data) = &entry.value else {
        return None;
    };
    data.srpg_self_score
}

#[inline(always)]
pub fn should_keep_newer_player_leaderboard_entry(
    entry: Option<&PlayerLeaderboardCacheEntry>,
    request_started_at: Instant,
) -> bool {
    entry.is_some_and(|entry| entry.refreshed_at > request_started_at)
}

#[inline(always)]
pub fn player_leaderboard_request_was_invalidated(
    invalidated_after: Option<Instant>,
    request_started_at: Instant,
) -> bool {
    invalidated_after.is_some_and(|invalidated_after| request_started_at <= invalidated_after)
}

#[inline(always)]
pub fn should_fetch_player_leaderboard_entry(
    entry: Option<&PlayerLeaderboardCacheEntry>,
    max_entries: usize,
    refresh_cached: bool,
    now: Instant,
) -> bool {
    let Some(entry) = entry else {
        return true;
    };
    match entry.value {
        PlayerLeaderboardCacheValue::Ready(_) => {
            if refresh_cached {
                return entry
                    .retry_after
                    .is_none_or(|retry_after| now >= retry_after);
            }
            entry.max_entries < max_entries
                && entry
                    .retry_after
                    .is_none_or(|retry_after| now >= retry_after)
        }
        PlayerLeaderboardCacheValue::Error(_) => entry
            .retry_after
            .is_none_or(|retry_after| now >= retry_after),
    }
}

#[inline(always)]
pub fn should_rerun_in_flight_player_leaderboard_fetch(
    in_flight_max_entries: usize,
    requested_max_entries: usize,
    refresh_cached: bool,
) -> bool {
    refresh_cached || requested_max_entries > in_flight_max_entries
}

#[inline(always)]
pub fn queue_player_leaderboard_refresh(
    pending_refresh: &mut HashMap<PlayerLeaderboardCacheKey, usize>,
    key: &PlayerLeaderboardCacheKey,
    requested_max_entries: usize,
) {
    pending_refresh
        .entry(key.clone())
        .and_modify(|max_entries| *max_entries = (*max_entries).max(requested_max_entries))
        .or_insert(requested_max_entries);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rank: u32, name: &str, is_self: bool, is_rival: bool) -> LeaderboardEntry {
        LeaderboardEntry {
            rank,
            name: name.to_string(),
            machine_tag: None,
            score: 9800.0 - f64::from(rank),
            date: String::new(),
            is_rival,
            is_self,
            is_fail: false,
        }
    }

    #[test]
    fn leaderboard_snapshots_share_cached_payloads() {
        let data = Arc::new(PlayerLeaderboardData {
            panes: vec![LeaderboardPane {
                name: "GrooveStats".to_string(),
                entries: vec![entry(1, "player", true, false)],
                is_ex: false,
                disabled: false,
                personalized: true,
                arrowcloud_kind: None,
            }],
            srpg_self_score: None,
            itl_self_score: None,
            itl_self_rank: None,
        });
        let cache_entry = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Ready(Arc::clone(&data)),
            max_entries: 5,
            refreshed_at: Instant::now(),
            retry_after: None,
        };

        let first = player_leaderboard_snapshot_from_entry(&cache_entry);
        let second = player_leaderboard_snapshot_from_entry(&cache_entry);
        assert!(Arc::ptr_eq(first.data.as_ref().unwrap(), &data));
        assert!(Arc::ptr_eq(
            first.data.as_ref().unwrap(),
            second.data.as_ref().unwrap()
        ));
    }

    #[test]
    fn leaderboard_date_formats_date_time_prefixes() {
        assert_eq!(format_leaderboard_date("2023-04-15"), "Apr 15, 2023");
        assert_eq!(
            format_leaderboard_date("2023-04-15 21:07:33"),
            "Apr 15, 2023"
        );
        assert_eq!(
            format_leaderboard_date("2023-04-15T21:07:33Z"),
            "Apr 15, 2023"
        );
    }

    #[test]
    fn leaderboard_date_handles_empty_and_invalid_values() {
        assert_eq!(format_leaderboard_date(" "), "");
        assert_eq!(format_leaderboard_date_or_placeholder(" "), "----------");
        assert_eq!(format_leaderboard_date("2023-13-01"), "2023-13-01");
        assert_eq!(format_leaderboard_date("2023-04-00"), "2023-04-00");
        assert_eq!(format_leaderboard_date("not-a-date"), "not-a-date");
    }

    #[test]
    fn scorebox_text_helpers_match_ui_policy() {
        assert_eq!(scorebox_machine_tag(Some(" abcd5 "), "fallback"), "ABCD");
        assert_eq!(scorebox_machine_tag(Some(" "), "efgh5"), "----");
        assert_eq!(scorebox_machine_tag(None, " ijkl5 "), "IJKL");

        assert_eq!(format_scorebox_score_percent(9876.0), "98.76%");
        assert_eq!(format_scorebox_score_value(9876.0), "98.76");
        assert_eq!(format_scorebox_score_percent(12000.0), "100.00%");
        assert_eq!(format_scorebox_score_value(f64::NAN), "0.00");
        assert_eq!(format_scorebox_rank(42), "42.");
    }

    #[test]
    fn prioritized_leaderboard_entries_keep_self_and_rivals_visible() {
        let mut entries = (1..=12)
            .map(|rank| entry(rank, &format!("top-{rank}"), false, false))
            .collect::<Vec<_>>();
        entries.push(entry(20, "self", true, false));
        entries.push(entry(30, "rival-a", false, true));
        entries.push(entry(40, "rival-b", false, true));

        let selected = prioritized_leaderboard_entries(entries.as_slice(), 10);
        let ranks = selected.iter().map(|entry| entry.rank).collect::<Vec<_>>();

        assert_eq!(ranks, vec![1, 2, 3, 4, 5, 6, 7, 20, 30, 40]);
    }

    #[test]
    fn prioritized_leaderboard_entries_handles_small_caps() {
        let entries = [
            entry(1, "top", false, false),
            entry(20, "self", true, false),
            entry(30, "rival", false, true),
        ];

        assert!(prioritized_leaderboard_entries(&entries, 0).is_empty());
        let selected = prioritized_leaderboard_entries(&entries, 2);
        let ranks = selected.iter().map(|entry| entry.rank).collect::<Vec<_>>();
        assert_eq!(ranks, vec![1, 20]);
    }

    #[test]
    fn prioritized_leaderboard_refs_match_owned_selection_and_ties() {
        let entries = [
            entry(2, "rival", false, true),
            entry(1, "world", false, false),
            entry(20, "SELF", true, false),
            entry(20, "self", true, false),
            entry(3, "extra", false, false),
        ];
        let borrowed = prioritized_leaderboard_entry_refs(&entries, 4);
        let owned = prioritized_leaderboard_entries(&entries, 4);

        assert_eq!(borrowed.len(), owned.len());
        for (borrowed, owned) in borrowed.into_iter().zip(&owned) {
            assert_eq!(borrowed.rank, owned.rank);
            assert_eq!(borrowed.name, owned.name);
            assert_eq!(borrowed.is_self, owned.is_self);
            assert_eq!(borrowed.is_rival, owned.is_rival);
        }
    }

    fn pane(
        name: &str,
        is_ex: bool,
        arrowcloud_kind: Option<ArrowCloudPaneKind>,
    ) -> LeaderboardPane {
        LeaderboardPane {
            name: name.to_string(),
            entries: vec![entry(1, "score", false, false)],
            is_ex,
            disabled: false,
            personalized: true,
            arrowcloud_kind,
        }
    }

    #[test]
    fn scorebox_pane_kind_classifies_known_leaderboards() {
        let cases = [
            (
                pane("GrooveStats", false, None),
                ScoreboxPaneKind::Gs,
                "ITG",
            ),
            (pane("GrooveStats", true, None), ScoreboxPaneKind::Ex, "EX"),
            (
                pane("ArrowCloud", false, Some(ArrowCloudPaneKind::HardEx)),
                ScoreboxPaneKind::HardEx,
                "H.EX",
            ),
            (
                pane("ArrowCloud", true, Some(ArrowCloudPaneKind::Ex)),
                ScoreboxPaneKind::Ex,
                "EX",
            ),
            (
                pane("SRPG Event", false, None),
                ScoreboxPaneKind::Srpg,
                "SRPG",
            ),
            (pane("ITL 2025", false, None), ScoreboxPaneKind::Itl, "ITL"),
            (
                pane("Custom Board", false, None),
                ScoreboxPaneKind::Other,
                "Custom Board",
            ),
        ];

        for (pane, kind, mode_text) in cases {
            assert_eq!(scorebox_pane_kind(&pane), kind);
            assert_eq!(
                scorebox_pane_kind(&pane),
                scorebox_pane_kind_legacy_for_bench(&pane)
            );
            assert_eq!(scorebox_pane_mode_text(kind, &pane), mode_text);
        }
    }

    #[test]
    fn scorebox_pane_kind_preserves_ascii_substring_behavior() {
        for name in [
            "pre-RpG-post",
            "SRPG Event",
            "ITL Online 2026",
            "SŘPG Event",
            "Custom Board",
            "",
        ] {
            let pane = pane(name, false, None);
            assert_eq!(
                scorebox_pane_kind(&pane),
                scorebox_pane_kind_legacy_for_bench(&pane),
                "classification changed for {name:?}"
            );
        }
    }

    #[test]
    fn select_music_scorebox_filter_allows_expected_kinds() {
        let filter = SelectMusicScoreboxFilter {
            itg: false,
            ex: true,
            hard_ex: false,
            tournaments: true,
        };

        assert!(select_music_scorebox_filter_has_any(filter));
        assert!(!select_music_scorebox_filter_allows_kind(
            ScoreboxPaneKind::Gs,
            filter
        ));
        assert!(select_music_scorebox_filter_allows_kind(
            ScoreboxPaneKind::Ex,
            filter
        ));
        assert!(select_music_scorebox_filter_allows_kind(
            ScoreboxPaneKind::Itl,
            filter
        ));
    }

    #[test]
    fn select_music_scorebox_filter_returns_allowed_panes() {
        let panes = [
            pane("GrooveStats", false, None),
            pane("GrooveStats", true, None),
            pane("ITL 2025", false, None),
        ];
        let filter = SelectMusicScoreboxFilter {
            itg: false,
            ex: true,
            hard_ex: false,
            tournaments: true,
        };

        let filtered = select_music_scorebox_filtered_panes(&panes, filter);
        let names = filtered
            .iter()
            .map(|pane| pane.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["GrooveStats", "ITL 2025"]);
        assert_eq!(scorebox_pane_kind(filtered[0]), ScoreboxPaneKind::Ex);
        assert_eq!(scorebox_pane_kind(filtered[1]), ScoreboxPaneKind::Itl);
    }

    #[test]
    fn preferred_primary_scorebox_pane_uses_profile_mode_then_fallbacks() {
        let gs = pane("GrooveStats", false, None);
        let ex = pane("GrooveStats", true, None);
        let itl = pane("ITL 2025", false, None);
        let only_tournament = [&itl];
        let mixed = [&itl, &gs, &ex];

        assert_eq!(
            preferred_primary_scorebox_pane(&mixed, true).map(scorebox_pane_kind),
            Some(ScoreboxPaneKind::Ex)
        );
        assert_eq!(
            preferred_primary_scorebox_pane(&mixed, false).map(scorebox_pane_kind),
            Some(ScoreboxPaneKind::Gs)
        );
        assert_eq!(
            preferred_primary_scorebox_pane(&only_tournament, true).map(scorebox_pane_kind),
            Some(ScoreboxPaneKind::Itl)
        );
        assert!(preferred_primary_scorebox_pane(&[], true).is_none());
    }

    #[test]
    fn player_leaderboard_cache_reuses_success_until_more_rows_are_needed() {
        let now = Instant::now();
        let ready = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Ready(Arc::new(PlayerLeaderboardData {
                panes: Vec::new(),
                srpg_self_score: None,
                itl_self_score: None,
                itl_self_rank: None,
            })),
            max_entries: 5,
            refreshed_at: now,
            retry_after: None,
        };
        assert!(!should_fetch_player_leaderboard_entry(
            Some(&ready),
            5,
            false,
            now
        ));
        assert!(!should_fetch_player_leaderboard_entry(
            Some(&ready),
            3,
            false,
            now
        ));
        assert!(should_fetch_player_leaderboard_entry(
            Some(&ready),
            10,
            false,
            now
        ));
        assert!(should_fetch_player_leaderboard_entry(
            Some(&ready),
            5,
            true,
            now
        ));

        let cooled_down_ready = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Ready(Arc::new(PlayerLeaderboardData {
                panes: Vec::new(),
                srpg_self_score: None,
                itl_self_score: None,
                itl_self_rank: None,
            })),
            max_entries: 5,
            refreshed_at: now,
            retry_after: Some(now + std::time::Duration::from_secs(10)),
        };
        assert!(!should_fetch_player_leaderboard_entry(
            Some(&cooled_down_ready),
            10,
            false,
            now
        ));
        assert!(!should_fetch_player_leaderboard_entry(
            Some(&cooled_down_ready),
            5,
            true,
            now
        ));

        let stale_error = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Error(Arc::from("boom")),
            max_entries: 5,
            refreshed_at: now - std::time::Duration::from_secs(10),
            retry_after: Some(now - std::time::Duration::from_millis(1)),
        };
        assert!(should_fetch_player_leaderboard_entry(
            Some(&stale_error),
            5,
            false,
            now
        ));
    }

    #[test]
    fn in_flight_leaderboard_fetch_reruns_for_submit_refresh() {
        assert!(!should_rerun_in_flight_player_leaderboard_fetch(
            5, 5, false
        ));
        assert!(should_rerun_in_flight_player_leaderboard_fetch(
            5, 10, false
        ));
        assert!(should_rerun_in_flight_player_leaderboard_fetch(5, 5, true));
    }

    #[test]
    fn queued_leaderboard_refresh_keeps_largest_request() {
        let key = PlayerLeaderboardCacheKey {
            chart_hash: "deadbeef".to_string(),
            api_key: "gs".to_string(),
            arrowcloud_api_key: "ac".to_string(),
            include_arrowcloud: true,
            show_ex_score: false,
        };
        let mut pending_refresh = HashMap::new();

        queue_player_leaderboard_refresh(&mut pending_refresh, &key, 5);
        queue_player_leaderboard_refresh(&mut pending_refresh, &key, 10);
        queue_player_leaderboard_refresh(&mut pending_refresh, &key, 3);

        assert_eq!(pending_refresh.get(&key), Some(&10));
    }

    #[test]
    fn runtime_leaderboard_request_plan_builds_spawn_payload() {
        let chart_hash = format!("plan-{}", std::process::id());
        let snapshot = scorebox_snapshot(
            true,
            true,
            true,
            true,
            true,
            true,
            "  gs-key  ",
            "  ac-key  ",
            "  player  ",
            Some("profile-1".to_string()),
        );

        let plan = runtime_plan_player_leaderboard_request(
            chart_hash.as_str(),
            &snapshot,
            7,
            false,
            Instant::now(),
        )
        .expect("active leaderboard should plan");

        assert!(plan.snapshot.loading);
        let fetch = plan.fetch.expect("uncached leaderboard should fetch");
        assert_eq!(fetch.key.chart_hash, chart_hash);
        assert_eq!(fetch.key.api_key, "gs-key");
        assert_eq!(fetch.key.arrowcloud_api_key, "ac-key");
        assert!(fetch.key.include_arrowcloud);
        assert!(fetch.key.show_ex_score);
        assert_eq!(fetch.gs_username, "player");
        assert_eq!(fetch.persistent_profile_id.as_deref(), Some("profile-1"));
        assert_eq!(fetch.auto_profile_id.as_deref(), Some("profile-1"));
        assert!(fetch.should_auto_populate);
        assert_eq!(fetch.max_entries, 7);

        let mut cache = runtime_lock_player_leaderboard_cache();
        cache.in_flight.remove(&fetch.key);
        cache.pending_refresh.remove(&fetch.key);
        cache.invalidated_after.remove(&fetch.key);
        cache.by_key.remove(&fetch.key);
    }

    #[test]
    fn runtime_leaderboard_fetch_job_completes_cache_and_keeps_context() {
        let chart_hash = format!("job-{}", std::process::id());
        let snapshot = scorebox_snapshot(
            true,
            true,
            true,
            true,
            true,
            true,
            "gs-key",
            "ac-key",
            "player",
            Some("profile-1".to_string()),
        );
        let fetch = runtime_plan_player_leaderboard_request(
            chart_hash.as_str(),
            &snapshot,
            5,
            false,
            Instant::now(),
        )
        .and_then(|plan| plan.fetch)
        .expect("fetch should be planned");

        let result = runtime_run_player_leaderboard_fetch(fetch, |key, username, max_entries| {
            assert_eq!(key.chart_hash, chart_hash);
            assert_eq!(username, "player");
            assert_eq!(max_entries, 5);
            Ok(PlayerLeaderboardFetchSuccess {
                data: PlayerLeaderboardData {
                    panes: Vec::new(),
                    srpg_self_score: Some(9910),
                    itl_self_score: Some(9900),
                    itl_self_rank: Some(7),
                },
                imported_score: Some("score"),
                itl_self_found: true,
            })
        });

        assert_eq!(result.gs_username, "player");
        assert_eq!(result.persistent_profile_id.as_deref(), Some("profile-1"));
        assert_eq!(result.auto_profile_id.as_deref(), Some("profile-1"));
        assert!(result.should_auto_populate);
        assert_eq!(
            result.completion.fetched_itl_self,
            Some((Some(9900), Some(7)))
        );
        assert_eq!(result.completion.fetched_srpg_self_score, Some(9910));
        assert_eq!(result.completion.fetched_imported_score, Some("score"));

        let mut cache = runtime_lock_player_leaderboard_cache();
        cache.in_flight.remove(&result.key);
        cache.pending_refresh.remove(&result.key);
        cache.invalidated_after.remove(&result.key);
        cache.by_key.remove(&result.key);
    }

    #[test]
    fn runtime_leaderboard_request_plan_rejects_zero_rows() {
        let snapshot = scorebox_snapshot(
            true, true, true, true, true, true, "gs", "ac", "player", None,
        );

        assert!(
            runtime_plan_player_leaderboard_request(
                "deadbeef",
                &snapshot,
                0,
                false,
                Instant::now(),
            )
            .is_none()
        );
    }

    #[test]
    fn newer_player_leaderboard_entry_blocks_older_fetch_result() {
        let newer_entry = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Ready(Arc::new(PlayerLeaderboardData {
                panes: Vec::new(),
                srpg_self_score: None,
                itl_self_score: None,
                itl_self_rank: None,
            })),
            max_entries: 0,
            refreshed_at: Instant::now(),
            retry_after: None,
        };
        let older_request_started_at = Instant::now() - std::time::Duration::from_millis(1);
        assert!(should_keep_newer_player_leaderboard_entry(
            Some(&newer_entry),
            older_request_started_at,
        ));

        let older_entry = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Ready(Arc::new(PlayerLeaderboardData {
                panes: Vec::new(),
                srpg_self_score: None,
                itl_self_score: None,
                itl_self_rank: None,
            })),
            max_entries: 0,
            refreshed_at: Instant::now() - std::time::Duration::from_secs(1),
            retry_after: None,
        };
        assert!(!should_keep_newer_player_leaderboard_entry(
            Some(&older_entry),
            Instant::now(),
        ));
    }

    #[test]
    fn leaderboard_fetch_completion_stores_ready_data_and_effects() {
        let key = PlayerLeaderboardCacheKey {
            chart_hash: "deadbeef".to_string(),
            api_key: "gs".to_string(),
            arrowcloud_api_key: "ac".to_string(),
            include_arrowcloud: true,
            show_ex_score: false,
        };
        let request_started_at = Instant::now();
        let refresh_finished_at = request_started_at + std::time::Duration::from_millis(5);
        let mut cache = PlayerLeaderboardCacheState::default();
        cache.in_flight.insert(key.clone(), 5);

        let completion = cache.complete_fetch(
            &key,
            5,
            request_started_at,
            refresh_finished_at,
            std::time::Duration::from_secs(10),
            Ok(PlayerLeaderboardFetchSuccess {
                data: PlayerLeaderboardData {
                    panes: Vec::new(),
                    srpg_self_score: Some(9910),
                    itl_self_score: Some(9900),
                    itl_self_rank: Some(7),
                },
                imported_score: Some("score"),
                itl_self_found: true,
            }),
            true,
            true,
        );

        assert_eq!(completion.fetched_itl_self, Some((Some(9900), Some(7))));
        assert_eq!(completion.fetched_srpg_self_score, Some(9910));
        assert_eq!(completion.fetched_imported_score, Some("score"));
        assert!(completion.queued_fetch.is_none());
        assert!(cache.in_flight.is_empty());
        assert!(matches!(
            cache.by_key.get(&key).map(|entry| &entry.value),
            Some(PlayerLeaderboardCacheValue::Ready(data))
                if data.srpg_self_score == Some(9910)
        ));
    }

    #[test]
    fn leaderboard_fetch_completion_keeps_stale_ready_on_error() {
        let key = PlayerLeaderboardCacheKey {
            chart_hash: "deadbeef".to_string(),
            api_key: "gs".to_string(),
            arrowcloud_api_key: String::new(),
            include_arrowcloud: false,
            show_ex_score: false,
        };
        let request_started_at = Instant::now();
        let refresh_finished_at = request_started_at + std::time::Duration::from_millis(5);
        let retry = std::time::Duration::from_secs(10);
        let mut cache = PlayerLeaderboardCacheState::default();
        cache.in_flight.insert(key.clone(), 5);
        cache.by_key.insert(
            key.clone(),
            PlayerLeaderboardCacheEntry {
                value: PlayerLeaderboardCacheValue::Ready(Arc::new(PlayerLeaderboardData {
                    panes: Vec::new(),
                    srpg_self_score: Some(9910),
                    itl_self_score: None,
                    itl_self_rank: None,
                })),
                max_entries: 5,
                refreshed_at: request_started_at - std::time::Duration::from_secs(1),
                retry_after: None,
            },
        );

        let completion = cache.complete_fetch::<&str>(
            &key,
            5,
            request_started_at,
            refresh_finished_at,
            retry,
            Err("network".to_string()),
            true,
            true,
        );

        assert!(completion.fetched_itl_self.is_none());
        assert!(completion.fetched_imported_score.is_none());
        let entry = cache.by_key.get(&key).unwrap();
        assert!(matches!(entry.value, PlayerLeaderboardCacheValue::Ready(_)));
        assert_eq!(entry.refreshed_at, refresh_finished_at);
        assert_eq!(entry.retry_after, Some(refresh_finished_at + retry));
    }

    #[test]
    fn leaderboard_fetch_completion_queues_pending_refresh() {
        let key = PlayerLeaderboardCacheKey {
            chart_hash: "deadbeef".to_string(),
            api_key: "gs".to_string(),
            arrowcloud_api_key: String::new(),
            include_arrowcloud: false,
            show_ex_score: false,
        };
        let request_started_at = Instant::now();
        let mut cache = PlayerLeaderboardCacheState::default();
        cache.in_flight.insert(key.clone(), 5);
        cache.pending_refresh.insert(key.clone(), 10);

        let completion = cache.complete_fetch::<&str>(
            &key,
            5,
            request_started_at,
            request_started_at + std::time::Duration::from_millis(5),
            std::time::Duration::from_secs(10),
            Err("network".to_string()),
            false,
            false,
        );

        let queued = completion.queued_fetch.expect("queued refresh");
        assert_eq!(queued.key, key);
        assert_eq!(queued.max_entries, 10);
        assert_eq!(cache.in_flight.get(&queued.key), Some(&10));
        assert!(cache.pending_refresh.is_empty());
    }
}
