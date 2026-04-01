use crate::act;
use crate::engine::present::actors::Actor;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::online::lobbies;
use crate::game::profile;
use std::cmp::Ordering;

const PANEL_WIDTH: f32 = 200.0;
const PANEL_BG_ALPHA: f32 = 0.5;
const PANEL_TEXT_ZOOM: f32 = 0.72;
const PANEL_TEXT_MAXWIDTH: f32 = PANEL_WIDTH - 16.0;

pub struct RenderParams<'a> {
    pub screen_name: &'a str,
    pub joined: &'a lobbies::JoinedLobby,
    pub z: i16,
    pub show_song_info: bool,
    pub status_text: Option<String>,
}

pub fn build_panel(params: RenderParams<'_>) -> Vec<Actor> {
    let body_lines = build_body_lines(
        params.joined,
        params.screen_name,
        params.show_song_info,
        params.status_text.as_deref(),
    );
    let body_text = body_lines.join("\n");
    let x = display_x(params.screen_name);
    let y = screen_center_y();
    let height = screen_height();

    vec![
        act!(quad:
            align(0.5, 0.5):
            xy(x, y):
            zoomto(PANEL_WIDTH, height):
            diffuse(0.0, 0.0, 0.0, PANEL_BG_ALPHA):
            z(params.z)
        ),
        act!(text:
            font("miso"):
            settext(body_text):
            align(0.5, 0.5):
            xy(x, y):
            zoom(PANEL_TEXT_ZOOM):
            maxwidth(PANEL_TEXT_MAXWIDTH):
            diffuse(1.0, 1.0, 0.0, 1.0):
            z(params.z + 1):
            horizalign(center)
        ),
    ]
}

fn build_body_lines(
    joined: &lobbies::JoinedLobby,
    current_screen_name: &str,
    show_song_info: bool,
    status_text: Option<&str>,
) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(status_text) = status_text {
        for line in status_text.lines() {
            lines.push(truncate_text(line, 44));
        }
        if !lines.is_empty() {
            lines.push(String::new());
        }
    }

    let ordered_players = ordered_players(joined);
    if ordered_players.is_empty() {
        lines.push("Waiting for players...".to_string());
        return lines;
    }

    let show_ready_icons = current_screen_name.eq_ignore_ascii_case("ScreenGameplay")
        && !joined.players.is_empty()
        && !joined.players.iter().all(|player| player.ready);

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
            player_line.push_str(if player.ready { " [✔]" } else { " [ ]" });
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

fn display_x(screen_name: &str) -> f32 {
    let left = PANEL_WIDTH * 0.5;
    let right = screen_width() - PANEL_WIDTH * 0.5;
    let center = screen_center_x();
    if screen_name.eq_ignore_ascii_case("ScreenSelectMusic") {
        return left;
    }
    if !screen_name.eq_ignore_ascii_case("ScreenGameplay")
        && !screen_name.eq_ignore_ascii_case("ScreenEvaluationStage")
    {
        return left;
    }

    let mut p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
    let mut p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
    if !(p1_joined || p2_joined) {
        match profile::get_session_player_side() {
            profile::PlayerSide::P1 => p1_joined = true,
            profile::PlayerSide::P2 => p2_joined = true,
        }
    }

    if p1_joined && p2_joined {
        center
    } else if p1_joined {
        right
    } else {
        left
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
