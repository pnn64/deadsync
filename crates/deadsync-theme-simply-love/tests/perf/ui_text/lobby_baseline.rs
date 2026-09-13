// Frozen from fcb00c347 (0.5.1201); test-only reference implementations.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct LobbyHudSnapshot {
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
    pub(super) fn matches(&self, params: &CachedRenderParams<'_>) -> bool {
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

    pub(super) fn from_params(params: &CachedRenderParams<'_>) -> Self {
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

    pub(super) fn matches(&self, player: &lobbies::LobbyPlayer) -> bool {
        self.label.as_ref() == player.label
            && self.ready == player.ready
            && self.screen_name.as_ref() == player.screen_name
            && percent_value_matches(self.score, player.score)
            && percent_value_matches(self.ex_score, player.ex_score)
    }
}

pub(super) fn build_body_text(
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
const fn is_score_screen(screen_name: &str) -> bool {
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
