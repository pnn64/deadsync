use crate::act;
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_online::lobbies;
use deadsync_profile::PlayerSide;
use std::cmp::Ordering;
use std::fmt::{self, Write as _};
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
    z: i16,
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
            && self.z == params.z
    }

    fn update(&mut self, params: &CachedRenderParams<'_>) {
        replace_text(&mut self.screen_name, params.screen_name);
        replace_text(&mut self.code, &params.joined.code);
        if self.players.len() == params.joined.players.len() {
            for (cached, player) in self.players.iter_mut().zip(&params.joined.players) {
                replace_text(&mut cached.label, &player.label);
                replace_text(&mut cached.screen_name, &player.screen_name);
                cached.ready = player.ready;
                cached.score = player.score;
                cached.ex_score = player.ex_score;
            }
        } else {
            self.players = params
                .joined
                .players
                .iter()
                .map(LobbyPlayerSnapshot::from_player)
                .collect();
        }
        replace_optional_text(
            &mut self.song_path,
            params
                .show_song_info
                .then_some(params.joined.song_info.as_ref())
                .flatten()
                .map(|song| song.song_path.as_str()),
        );
        replace_optional_text(&mut self.status_text, params.status_text);
        self.show_song_info = params.show_song_info;
        self.joined_sides = params.joined_sides;
        self.player_side = params.player_side;
        self.z = params.z;
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
            z: params.z,
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

// Leave unchanged names untouched during frequent score/ready updates.
fn replace_text(target: &mut Box<str>, source: &str) {
    if target.as_ref() != source {
        *target = source.into();
    }
}

fn replace_optional_text(target: &mut Option<Box<str>>, source: Option<&str>) {
    match (target.as_mut(), source) {
        (Some(target), Some(source)) => replace_text(target, source),
        (_, source) => *target = source.map(Into::into),
    }
}

fn percent_value_matches(left: Option<f32>, right: Option<f32>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) if !left.is_finite() && !right.is_finite() => true,
        (Some(left), Some(right)) if left <= 0.0 && right <= 0.0 => true,
        (left, right) => left == right,
    }
}

/// Screen-owned cache for the stable online lobby panel.
///
/// The application thread owns one instance for each screen that uses it. Its
/// lifetime is one screen visit and its capacity is exactly one snapshot plus
/// one immutable two-actor panel. It is populated on first render and rebuilt only
/// when the lobby, status, language-facing text inputs, or placement inputs
/// change. A hit performs bounded player comparisons and shares one immutable
/// two-actor slice without cloning the wide actor values. There is no eviction,
/// synchronization, or live-frame pruning.
/// Unchanged snapshot strings and equal-length player lists are reused across
/// lobby updates. Changed text remains exactly sized; the snapshot is freed
/// with the screen. Hit and miss counters provide runtime
/// instrumentation. Worst-case boundary work sorts and formats the current
/// lobby's finite player list once.
#[derive(Default)]
pub struct LobbyHudCache {
    snapshot: Option<LobbyHudSnapshot>,
    panel: Arc<[Actor]>,
    stats: LobbyHudCacheStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LobbyHudCacheStats {
    pub hits: u64,
    pub misses: u64,
}

impl LobbyHudCache {
    #[must_use]
    pub const fn stats(&self) -> LobbyHudCacheStats {
        self.stats
    }

    fn panel(&mut self, params: &CachedRenderParams<'_>) -> Arc<[Actor]> {
        if self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.matches(params))
        {
            self.stats.hits = self.stats.hits.saturating_add(1);
            return Arc::clone(&self.panel);
        }
        self.stats.misses = self.stats.misses.saturating_add(1);
        let body_text: Arc<str> = Arc::from(build_body_text(
            params.joined,
            params.screen_name,
            params.show_song_info,
            params.status_text,
        ));
        let placement =
            panel_placement(params.screen_name, params.joined_sides, params.player_side);
        let width = panel_width(params.screen_name, placement);
        let x = display_x(placement, width);
        let y = screen_center_y();
        let height = screen_height();
        self.panel = Arc::from([
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
        ]);
        if let Some(snapshot) = &mut self.snapshot {
            snapshot.update(params);
        } else {
            self.snapshot = Some(LobbyHudSnapshot::from_params(params));
        }
        Arc::clone(&self.panel)
    }
}

#[must_use]
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
    actors.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children: cache.panel(&params),
        background: None,
        z: 0,
        tint: [1.0; 4],
        blend: None,
    });
}

fn build_body_text(
    joined: &lobbies::JoinedLobby,
    current_screen_name: &str,
    show_song_info: bool,
    status_text: Option<&str>,
) -> String {
    // Reserve typical player/status lines; longer screen names can still grow.
    let mut out = String::with_capacity(
        joined.code.len()
            + 32
            + joined.players.len() * 96
            + status_text.map_or(0, |text| text.len().min(256)),
    );
    write!(out, "Lobby Code: {}\n\n", joined.code).expect("writing to a String cannot fail");
    if let Some(status_text) = status_text {
        for line in status_text.lines() {
            writeln!(out, "{}", Truncated(line, 44)).expect("writing to a String cannot fail");
        }
        out.push('\n');
    }
    let ordered_players = ordered_players(joined);
    if ordered_players.is_empty() {
        out.push_str("Waiting for players...");
        return out;
    }
    let show_ready_icons = current_screen_name.eq_ignore_ascii_case("ScreenGameplay")
        && !joined.players.is_empty()
        && !joined.players.iter().all(gameplay_player_ready);
    for (display_index, (_, player)) in ordered_players.into_iter().enumerate() {
        if display_index > 0 {
            out.push_str("\n\n");
        }
        write!(
            out,
            "{}. {}",
            display_index + 1,
            Truncated(&player.label, 22)
        )
        .expect("writing to a String cannot fail");
        if show_ready_icons {
            out.push_str(if gameplay_player_ready(player) {
                " [\u{2714}]"
            } else {
                " [\u{274c}]"
            });
        }
        if !player.screen_name.eq_ignore_ascii_case(current_screen_name) {
            out.push_str(" - in ");
            out.push_str(display_screen_name(&player.screen_name));
        }
        if is_score_screen(&player.screen_name) {
            write!(
                out,
                "\n    {:.2}% - {:.2}% EX",
                percent_value(player.score),
                percent_value(player.ex_score)
            )
            .expect("writing to a String cannot fail");
        }
    }
    if show_song_info && let Some(song_info) = joined.song_info.as_ref() {
        let (pack, song) = song_info
            .song_path
            .split_once('/')
            .unwrap_or(("Unknown", &song_info.song_path));
        write!(
            out,
            "\n\nPack: {}\nSong: {}",
            Truncated(pack, 30),
            Truncated(song, 30)
        )
        .expect("writing to a String cannot fail");
    }
    out
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
const fn is_score_screen(screen_name: &str) -> bool {
    screen_name.eq_ignore_ascii_case("ScreenGameplay")
        || screen_name.eq_ignore_ascii_case("ScreenEvaluationStage")
}

#[inline(always)]
fn gameplay_player_ready(player: &lobbies::LobbyPlayer) -> bool {
    player.screen_name.eq_ignore_ascii_case("ScreenGameplay") && player.ready
}

fn display_screen_name(screen_name: &str) -> &str {
    let screen_name = screen_name.trim();
    if screen_name.is_empty() || screen_name.eq_ignore_ascii_case("NoScreen") {
        return "Transitioning";
    }
    screen_name.strip_prefix("Screen").unwrap_or(screen_name)
}

#[inline(always)]
fn percent_value(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
        .max(0.0)
}

#[inline(always)]
fn panel_width(screen_name: &str, placement: PanelPlacement) -> f32 {
    if placement == PanelPlacement::Center && is_score_screen(screen_name) {
        CENTER_PANEL_WIDTH
    } else {
        PANEL_WIDTH
    }
}

const fn panel_placement(
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

const fn normalized_joined_sides(
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
    let right = width.mul_add(-0.5, screen_width());
    match placement {
        PanelPlacement::Left => left,
        PanelPlacement::Center => screen_center_x(),
        PanelPlacement::Right => right,
    }
}

// Formatting borrows the displayed UTF-8 prefix; no truncated String is built.
struct Truncated<'a>(&'a str, usize);

impl fmt::Display for Truncated<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.char_indices().nth(self.1).is_none() {
            return f.write_str(self.0);
        }
        let keep = self.1.saturating_sub(3);
        let end = self
            .0
            .char_indices()
            .nth(keep)
            .map_or(self.0.len(), |(index, _)| index);
        f.write_str(&self.0[..end])?;
        f.write_str("...")
    }
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

        let text = build_body_text(&joined, "ScreenGameplay", false, None);
        let lines: Vec<_> = text.lines().collect();

        assert!(lines.iter().any(|line| line.contains("1. Local [✔]")));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("2. Remote [❌] - in SelectMusic"))
        );
    }

    fn panel_actors(actors: &[Actor]) -> &[Actor] {
        match actors {
            [Actor::SharedFrame { children, .. }] => children,
            _ => actors,
        }
    }

    fn panel_text(actors: &[Actor]) -> &TextContent {
        match panel_actors(actors).get(1) {
            Some(Actor::Text { content, .. }) => content,
            other => panic!("expected lobby text actor, got {other:?}"),
        }
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

        let text = build_body_text(&joined, "ScreenGameplay", false, None);
        let lines: Vec<_> = text.lines().collect();

        assert!(lines.iter().any(|line| line.contains("2. Remote [❌]")));
    }
}
