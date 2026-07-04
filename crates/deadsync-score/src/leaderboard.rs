use chrono::{Local, TimeZone};
use deadsync_core::input::InputSource;
use deadsync_core::song_time::{SongTimeNs, song_time_ns_invalid};
use std::cmp::Ordering;
use std::collections::HashSet;

use crate::{LocalReplayEdge, ScoreImportEndpoint};

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

pub fn prioritized_leaderboard_entries(
    entries: &[LeaderboardEntry],
    max_rows: usize,
) -> Vec<LeaderboardEntry> {
    if max_rows == 0 {
        return Vec::new();
    }
    if entries.len() <= max_rows {
        return entries.to_vec();
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
    selected.into_iter().cloned().collect()
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

#[inline(always)]
pub fn scorebox_pane_kind(pane: &LeaderboardPane) -> ScoreboxPaneKind {
    if pane.is_arrowcloud() {
        return if pane.is_hard_ex() {
            ScoreboxPaneKind::HardEx
        } else if pane.is_ex {
            ScoreboxPaneKind::Ex
        } else {
            ScoreboxPaneKind::Gs
        };
    }
    if pane.is_groovestats() {
        return if pane.is_ex {
            ScoreboxPaneKind::Ex
        } else {
            ScoreboxPaneKind::Gs
        };
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
    pub itl_self_score: Option<u32>,
    pub itl_self_rank: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlayerLeaderboardCacheKey {
    pub chart_hash: String,
    pub api_key: String,
    pub arrowcloud_api_key: String,
    pub include_arrowcloud: bool,
    pub show_ex_score: bool,
}

#[derive(Debug, Clone)]
pub struct CachedPlayerLeaderboardData {
    pub loading: bool,
    pub data: Option<PlayerLeaderboardData>,
    pub error: Option<String>,
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
            data: Some(data),
            error: None,
        }
    }

    #[inline(always)]
    pub fn error(error: String) -> Self {
        Self {
            loading: false,
            data: None,
            error: Some(error),
        }
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
            assert_eq!(scorebox_pane_mode_text(kind, &pane), mode_text);
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
}
