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
