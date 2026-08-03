use crate::act;
use deadlib_present::actors::Actor;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_online::lobbies;
use deadsync_profile::PlayerSide;
use std::cmp::Ordering;
use std::sync::Arc;

const PANEL_WIDTH: f32 = 200.0;
const CENTER_PANEL_WIDTH: f32 = 150.0;
const PANEL_BG_ALPHA: f32 = 0.5;
const PANEL_TEXT_ZOOM: f32 = 0.72;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PanelPlacement {
    Left,
    Center,
    Right,
}

pub struct RenderParams<'a> {
    pub screen_name: &'a str,
    pub joined: &'a lobbies::JoinedLobby,
    pub z: i16,
    pub show_song_info: bool,
    pub status_text: Option<String>,
    pub joined_sides: [bool; 2],
    pub player_side: PlayerSide,
}

pub struct CachedRenderParams<'a> {
    pub screen_name: &'a str,
    pub joined: &'a lobbies::JoinedLobby,
    pub z: i16,
    pub show_song_info: bool,
    pub status_text: Option<&'a str>,
    pub joined_sides: [bool; 2],
    pub player_side: PlayerSide,
}

#[derive(Clone, Debug, PartialEq)]
struct LobbyHudSnapshot {
    screen_name: Box<str>,
    code: Box<str>,
    players: Box<[LobbyPlayerSnapshot]>,
    song_path: Option<Box<str>>,
    show_song_info: bool,
    status_text: Option<Box<str>>,
    joined_sides: [bool; 2],
    player_side: PlayerSide,
}

#[derive(Clone, Debug, PartialEq)]
struct LobbyPlayerSnapshot {
    label: Box<str>,
    ready: bool,
    screen_name: Box<str>,
    score: Option<f32>,
    ex_score: Option<f32>,
}

impl LobbyHudSnapshot {
    fn matches(&self, params: &CachedRenderParams<'_>) -> bool {
        self.screen_name.as_ref() == params.screen_name
            && self.code.as_ref() == params.joined.code
            && self.players.len() == params.joined.players.len()
            && self
                .players
                .iter()
                .zip(&params.joined.players)
                .all(|(cached, player)| cached.matches(player))
            && (!self.show_song_info
                || self.song_path.as_deref()
                    == params
                        .joined
                        .song_info
                        .as_ref()
                        .map(|song| song.song_path.as_str()))
            && self.show_song_info == params.show_song_info
            && self.status_text.as_deref() == params.status_text
            && self.joined_sides == params.joined_sides
            && self.player_side == params.player_side
    }

    fn from_params(params: &CachedRenderParams<'_>) -> Self {
        Self {
            screen_name: params.screen_name.into(),
            code: params.joined.code.as_str().into(),
            players: params
                .joined
                .players
                .iter()
                .map(LobbyPlayerSnapshot::from_player)
                .collect(),
            song_path: params
                .show_song_info
                .then(|| params.joined.song_info.as_ref())
                .flatten()
                .map(|song| song.song_path.as_str().into()),
            show_song_info: params.show_song_info,
            status_text: params.status_text.map(Into::into),
            joined_sides: params.joined_sides,
            player_side: params.player_side,
        }
    }
}

impl LobbyPlayerSnapshot {
    fn from_player(player: &lobbies::LobbyPlayer) -> Self {
        Self {
            label: player.label.as_str().into(),
            ready: player.ready,
            screen_name: player.screen_name.as_str().into(),
            score: player.score,
            ex_score: player.ex_score,
        }
    }

    fn matches(&self, player: &lobbies::LobbyPlayer) -> bool {
        self.label.as_ref() == player.label
            && self.ready == player.ready
            && self.screen_name.as_ref() == player.screen_name
            && percent_value_matches(self.score, player.score)
            && percent_value_matches(self.ex_score, player.ex_score)
    }
}

fn percent_value_matches(left: Option<f32>, right: Option<f32>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) if !left.is_finite() && !right.is_finite() => true,
        (Some(left), Some(right)) if left <= 0.0 && right <= 0.0 => true,
        (left, right) => left == right,
    }
}

/// Gameplay-owned cache for the stable online lobby panel.
///
/// The game/render thread owns one instance for the gameplay screen. Its
/// lifetime is one screen visit and its capacity is exactly one snapshot plus
/// one rendered text block. It is populated on first render and rebuilt only
/// when the lobby, status, language-facing text inputs, or placement inputs
/// change. A hit performs comparisons and two actor pushes with no allocation;
/// there is no eviction, scanning, synchronization, or gameplay-time pruning.
/// Replaced strings are freed on an external lobby-state change, and the final
/// snapshot is freed with the screen. Hit and miss counters provide runtime
/// instrumentation; their costs are covered by the lobby HUD gameplay benchmark.
#[derive(Default)]
pub struct LobbyHudCache {
    snapshot: Option<LobbyHudSnapshot>,
    body_text: Arc<str>,
    stats: LobbyHudCacheStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LobbyHudCacheStats {
    pub hits: u64,
    pub misses: u64,
}

impl LobbyHudCache {
    pub fn stats(&self) -> LobbyHudCacheStats {
        self.stats
    }

    fn body_text(&mut self, params: &CachedRenderParams<'_>) -> Arc<str> {
        if self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.matches(params))
        {
            self.stats.hits = self.stats.hits.saturating_add(1);
            return Arc::clone(&self.body_text);
        }
        self.stats.misses = self.stats.misses.saturating_add(1);
        self.body_text = Arc::from(build_body_text(
            params.joined,
            params.screen_name,
            params.show_song_info,
            params.status_text,
        ));
        self.snapshot = Some(LobbyHudSnapshot::from_params(params));
        Arc::clone(&self.body_text)
    }
}

pub fn build_panel(params: RenderParams<'_>) -> Vec<Actor> {
    let placement = panel_placement(params.screen_name, params.joined_sides, params.player_side);
    let width = panel_width(params.screen_name, placement);
    let body_text = build_body_text(
        params.joined,
        params.screen_name,
        params.show_song_info,
        params.status_text.as_deref(),
    );
    let x = display_x(placement, width);
    let y = screen_center_y();
    let height = screen_height();

    vec![
        act!(quad:
            align(0.5, 0.5):
            xy(x, y):
            zoomto(width, height):
            diffuse(0.0, 0.0, 0.0, PANEL_BG_ALPHA):
            z(params.z)
        ),
        act!(text:
            font("miso"):
            settext(body_text):
            align(0.5, 0.5):
            xy(x, y):
            zoom(PANEL_TEXT_ZOOM):
            maxwidth(width - 16.0):
            diffuse(1.0, 1.0, 0.0, 1.0):
            z(params.z + 1):
            horizalign(center)
        ),
    ]
}

pub fn push_cached_panel(
    actors: &mut Vec<Actor>,
    cache: &mut LobbyHudCache,
    params: CachedRenderParams<'_>,
) {
    let placement = panel_placement(params.screen_name, params.joined_sides, params.player_side);
    let width = panel_width(params.screen_name, placement);
    let body_text = cache.body_text(&params);
    let x = display_x(placement, width);
    let y = screen_center_y();
    let height = screen_height();

    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(x, y):
        zoomto(width, height):
        diffuse(0.0, 0.0, 0.0, PANEL_BG_ALPHA):
        z(params.z)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(body_text):
        align(0.5, 0.5):
        xy(x, y):
        zoom(PANEL_TEXT_ZOOM):
        maxwidth(width - 16.0):
        diffuse(1.0, 1.0, 0.0, 1.0):
        z(params.z + 1):
        horizalign(center)
    ));
}

fn build_body_text(
    joined: &lobbies::JoinedLobby,
    current_screen_name: &str,
    show_song_info: bool,
    status_text: Option<&str>,
) -> String {
    build_body_lines(joined, current_screen_name, show_song_info, status_text).join("\n")
}

fn build_body_lines(
    joined: &lobbies::JoinedLobby,
    current_screen_name: &str,
    show_song_info: bool,
    status_text: Option<&str>,
) -> Vec<String> {
    let mut lines = Vec::new();

    lines.push(format!("Lobby Code: {}", joined.code));
    lines.push(String::new());

    if let Some(status_text) = status_text {
        for line in status_text.lines() {
            lines.push(truncate_text(line, 44));
        }
        lines.push(String::new());
    }

    let ordered_players = ordered_players(joined);
    if ordered_players.is_empty() {
        lines.push("Waiting for players...".to_string());
        return lines;
    }

    let show_ready_icons = current_screen_name.eq_ignore_ascii_case("ScreenGameplay")
        && !joined.players.is_empty()
        && !joined.players.iter().all(gameplay_player_ready);

    for (display_index, (_, player)) in ordered_players.into_iter().enumerate() {
        if display_index > 0 {
            lines.push(String::new());
        }
        let mut player_line = format!(
            "{}. {}",
            display_index + 1,
            truncate_text(player.label.as_str(), 22)
        );
        if show_ready_icons {
            player_line.push_str(if gameplay_player_ready(player) {
                " [✔]"
            } else {
                " [❌]"
            });
        }
        if !player.screen_name.eq_ignore_ascii_case(current_screen_name) {
            player_line.push_str(" - in ");
            player_line.push_str(display_screen_name(player.screen_name.as_str()).as_str());
        }
        lines.push(player_line);

        if is_score_screen(player.screen_name.as_str()) {
            lines.push(format!(
                "    {} - {} EX",
                format_percent(player.score),
                format_percent(player.ex_score),
            ));
        }
    }

    if show_song_info && let Some(song_info) = joined.song_info.as_ref() {
        let (mut pack, mut song) = match song_info.song_path.split_once('/') {
            Some((pack, song)) => (pack.to_string(), song.to_string()),
            None => ("Unknown".to_string(), song_info.song_path.clone()),
        };
        pack = truncate_text(pack.as_str(), 30);
        song = truncate_text(song.as_str(), 30);
        lines.push(String::new());
        lines.push(format!("Pack: {pack}"));
        lines.push(format!("Song: {song}"));
    }

    lines
}

fn ordered_players(joined: &lobbies::JoinedLobby) -> Vec<(usize, &lobbies::LobbyPlayer)> {
    let mut score_players: Vec<_> = joined
        .players
        .iter()
        .enumerate()
        .filter(|(_, player)| is_score_screen(player.screen_name.as_str()))
        .collect();
    score_players.sort_by(|(a_idx, a), (b_idx, b)| {
        match (
            a.score.filter(|score| score.is_finite()),
            b.score.filter(|score| score.is_finite()),
        ) {
            (Some(a_score), Some(b_score)) => {
                b_score.total_cmp(&a_score).then_with(|| a_idx.cmp(b_idx))
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a_idx.cmp(b_idx),
        }
    });

    let mut ordered = score_players;
    ordered.extend(
        joined
            .players
            .iter()
            .enumerate()
            .filter(|(_, player)| !is_score_screen(player.screen_name.as_str())),
    );
    ordered
}

#[inline(always)]
fn is_score_screen(screen_name: &str) -> bool {
    screen_name.eq_ignore_ascii_case("ScreenGameplay")
        || screen_name.eq_ignore_ascii_case("ScreenEvaluationStage")
}

#[inline(always)]
fn gameplay_player_ready(player: &lobbies::LobbyPlayer) -> bool {
    player.screen_name.eq_ignore_ascii_case("ScreenGameplay") && player.ready
}

fn display_screen_name(screen_name: &str) -> String {
    let screen_name = screen_name.trim();
    if screen_name.is_empty() || screen_name.eq_ignore_ascii_case("NoScreen") {
        return "Transitioning".to_string();
    }
    screen_name
        .strip_prefix("Screen")
        .unwrap_or(screen_name)
        .to_string()
}

#[inline(always)]
fn format_percent(value: Option<f32>) -> String {
    let value = value
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
        .max(0.0);
    format!("{value:.2}%")
}

#[inline(always)]
fn panel_width(screen_name: &str, placement: PanelPlacement) -> f32 {
    if placement == PanelPlacement::Center && is_score_screen(screen_name) {
        CENTER_PANEL_WIDTH
    } else {
        PANEL_WIDTH
    }
}

fn panel_placement(
    screen_name: &str,
    joined_sides: [bool; 2],
    player_side: PlayerSide,
) -> PanelPlacement {
    if screen_name.eq_ignore_ascii_case("ScreenSelectMusic") {
        return PanelPlacement::Left;
    }
    if !screen_name.eq_ignore_ascii_case("ScreenGameplay")
        && !screen_name.eq_ignore_ascii_case("ScreenEvaluationStage")
    {
        return PanelPlacement::Left;
    }

    let [p1_joined, p2_joined] = normalized_joined_sides(joined_sides, player_side);
    match (p1_joined, p2_joined) {
        (true, true) => PanelPlacement::Center,
        (true, false) => PanelPlacement::Right,
        _ => PanelPlacement::Left,
    }
}

fn normalized_joined_sides(
    [mut p1_joined, mut p2_joined]: [bool; 2],
    player_side: PlayerSide,
) -> [bool; 2] {
    if !(p1_joined || p2_joined) {
        match player_side {
            PlayerSide::P1 => p1_joined = true,
            PlayerSide::P2 => p2_joined = true,
        }
    }
    [p1_joined, p2_joined]
}

fn display_x(placement: PanelPlacement, width: f32) -> f32 {
    let left = width * 0.5;
    let right = screen_width() - width * 0.5;
    match placement {
        PanelPlacement::Left => left,
        PanelPlacement::Center => screen_center_x(),
        PanelPlacement::Right => right,
    }
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(3);
    let mut out = String::with_capacity(max_chars);
    out.extend(text.chars().take(keep));
    out.push_str("...");
    out
}

#[cfg(test)]
mod session_tests {
    use super::*;

    #[test]
    fn empty_joined_snapshot_falls_back_to_active_side() {
        assert_eq!(
            normalized_joined_sides([false, false], PlayerSide::P1),
            [true, false]
        );
        assert_eq!(
            normalized_joined_sides([false, false], PlayerSide::P2),
            [false, true]
        );
    }

    #[test]
    fn gameplay_panel_placement_uses_prepared_joined_sides() {
        assert_eq!(
            panel_placement("ScreenGameplay", [true, true], PlayerSide::P1),
            PanelPlacement::Center
        );
        assert_eq!(
            panel_placement("ScreenGameplay", [true, false], PlayerSide::P1),
            PanelPlacement::Right
        );
        assert_eq!(
            panel_placement("ScreenGameplay", [false, true], PlayerSide::P2),
            PanelPlacement::Left
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::TextContent;

    fn test_player(label: &str, screen_name: &str, ready: bool) -> lobbies::LobbyPlayer {
        lobbies::LobbyPlayer {
            label: label.to_string(),
            ready,
            screen_name: screen_name.to_string(),
            judgments: None,
            score: None,
            ex_score: None,
        }
    }

    fn test_joined(players: Vec<lobbies::LobbyPlayer>) -> lobbies::JoinedLobby {
        lobbies::JoinedLobby {
            code: "ABCD".to_string(),
            players,
            song_info: None,
        }
    }

    #[test]
    fn gameplay_panel_treats_non_gameplay_players_as_not_ready() {
        let joined = test_joined(vec![
            test_player("Local", "ScreenGameplay", true),
            test_player("Remote", "ScreenSelectMusic", true),
        ]);

        let lines = build_body_lines(&joined, "ScreenGameplay", false, None);

        assert!(lines.iter().any(|line| line.contains("1. Local [✔]")));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("2. Remote [❌] - in SelectMusic"))
        );
    }

    fn panel_text(actors: &[Actor]) -> &TextContent {
        match actors.get(1) {
            Some(Actor::Text { content, .. }) => content,
            other => panic!("expected lobby text actor, got {other:?}"),
        }
    }

    #[test]
    fn cached_panel_matches_legacy_and_reuses_stable_text() {
        let joined = test_joined(vec![
            test_player("Local", "ScreenGameplay", true),
            test_player("Remote", "ScreenGameplay", false),
        ]);
        let legacy = build_panel(RenderParams {
            screen_name: "ScreenGameplay",
            joined: &joined,
            z: 995,
            show_song_info: false,
            status_text: Some("Waiting for ready\nHold back to leave".to_string()),
            joined_sides: [true, false],
            player_side: PlayerSide::P1,
        });
        let mut cache = LobbyHudCache::default();
        let mut cached = Vec::with_capacity(2);
        let params = || CachedRenderParams {
            screen_name: "ScreenGameplay",
            joined: &joined,
            z: 995,
            show_song_info: false,
            status_text: Some("Waiting for ready\nHold back to leave"),
            joined_sides: [true, false],
            player_side: PlayerSide::P1,
        };

        push_cached_panel(&mut cached, &mut cache, params());
        assert_eq!(cached.len(), legacy.len());
        assert_eq!(panel_text(&cached).as_str(), panel_text(&legacy).as_str());
        let first_text = match panel_text(&cached) {
            TextContent::Shared(text) => Arc::clone(text),
            other => panic!("expected shared cached text, got {other:?}"),
        };

        cached.clear();
        push_cached_panel(&mut cached, &mut cache, params());
        let second_text = match panel_text(&cached) {
            TextContent::Shared(text) => text,
            other => panic!("expected shared cached text, got {other:?}"),
        };
        assert!(Arc::ptr_eq(&first_text, second_text));
        assert_eq!(cache.stats(), LobbyHudCacheStats { hits: 1, misses: 1 });
    }

    #[test]
    fn cached_panel_refreshes_when_rendered_lobby_state_changes() {
        let mut joined = test_joined(vec![test_player("Remote", "ScreenEvaluationStage", true)]);
        let mut cache = LobbyHudCache::default();
        let mut actors = Vec::with_capacity(2);

        push_cached_panel(
            &mut actors,
            &mut cache,
            CachedRenderParams {
                screen_name: "ScreenGameplay",
                joined: &joined,
                z: 995,
                show_song_info: false,
                status_text: None,
                joined_sides: [true, false],
                player_side: PlayerSide::P1,
            },
        );
        let old_text = panel_text(&actors).as_str().to_string();

        joined.players[0].score = Some(98.76);
        actors.clear();
        push_cached_panel(
            &mut actors,
            &mut cache,
            CachedRenderParams {
                screen_name: "ScreenGameplay",
                joined: &joined,
                z: 995,
                show_song_info: false,
                status_text: None,
                joined_sides: [true, false],
                player_side: PlayerSide::P1,
            },
        );

        assert_ne!(panel_text(&actors).as_str(), old_text);
        assert!(panel_text(&actors).as_str().contains("98.76%"));
        assert_eq!(cache.stats().misses, 2);
    }

    #[test]
    fn gameplay_panel_uses_cross_for_unready_gameplay_players() {
        let joined = test_joined(vec![
            test_player("Local", "ScreenGameplay", true),
            test_player("Remote", "ScreenGameplay", false),
        ]);

        let lines = build_body_lines(&joined, "ScreenGameplay", false, None);

        assert!(lines.iter().any(|line| line.contains("2. Remote [❌]")));
    }
}
