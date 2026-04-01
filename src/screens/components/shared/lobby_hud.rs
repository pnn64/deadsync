use crate::act;
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::game::online::lobbies;
use std::cmp::Ordering;

const PANEL_BG_ALPHA: f32 = 0.86;
const PANEL_TITLE_ZOOM: f32 = 0.34;
const PANEL_BODY_ZOOM: f32 = 0.68;
const PANEL_LINE_STEP: f32 = 18.0;
const PANEL_PADDING_X: f32 = 12.0;
const PANEL_PADDING_TOP: f32 = 10.0;
const PANEL_PADDING_BOTTOM: f32 = 12.0;
const PANEL_HEADER_H: f32 = 20.0;
const PANEL_MIN_HEIGHT: f32 = 66.0;

pub struct RenderParams<'a> {
    pub screen_name: &'a str,
    pub joined: &'a lobbies::JoinedLobby,
    pub active_color_index: i32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub z: i16,
    pub show_song_info: bool,
}

pub fn build_panel(params: RenderParams<'_>) -> Vec<Actor> {
    let border = color::simply_love_rgba(params.active_color_index);
    let accent = color::decorative_rgba(params.active_color_index);
    let body_lines = build_body_lines(params.joined, params.screen_name, params.show_song_info);
    let line_count = body_lines.len().max(1);
    let body_text = body_lines.join("\n");
    let height = (PANEL_PADDING_TOP
        + PANEL_HEADER_H
        + 4.0
        + line_count as f32 * PANEL_LINE_STEP
        + PANEL_PADDING_BOTTOM)
        .max(PANEL_MIN_HEIGHT);

    vec![
        act!(quad:
            align(0.0, 0.0):
            xy(params.x, params.y):
            zoomto(params.width + 2.0, height + 2.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(params.z)
        ),
        act!(quad:
            align(0.0, 0.0):
            xy(params.x + 1.0, params.y + 1.0):
            zoomto(params.width, height):
            diffuse(0.0, 0.0, 0.0, PANEL_BG_ALPHA):
            z(params.z + 1)
        ),
        act!(quad:
            align(0.0, 0.0):
            xy(params.x + 1.0, params.y + 1.0):
            zoomto(params.width, 2.0):
            diffuse(accent[0], accent[1], accent[2], 1.0):
            z(params.z + 2)
        ),
        act!(text:
            font("wendy"):
            settext(format!("LOBBY {}", params.joined.code)):
            align(0.0, 0.0):
            xy(params.x + PANEL_PADDING_X, params.y + PANEL_PADDING_TOP):
            zoom(PANEL_TITLE_ZOOM):
            diffuse(border[0], border[1], border[2], 1.0):
            z(params.z + 3):
            horizalign(left)
        ),
        act!(text:
            font("miso"):
            settext(body_text):
            align(0.0, 0.0):
            xy(
                params.x + PANEL_PADDING_X,
                params.y + PANEL_PADDING_TOP + PANEL_HEADER_H
            ):
            zoom(PANEL_BODY_ZOOM):
            maxwidth(params.width - PANEL_PADDING_X * 2.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(params.z + 3):
            horizalign(left)
        ),
    ]
}

fn build_body_lines(
    joined: &lobbies::JoinedLobby,
    current_screen_name: &str,
    show_song_info: bool,
) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(reconnect_text) = lobbies::reconnect_status_text() {
        lines.push(truncate_text(reconnect_text.as_str(), 44));
    }

    if show_song_info && let Some(song_info) = joined.song_info.as_ref() {
        let title = song_info
            .title
            .as_deref()
            .unwrap_or(song_info.song_path.as_str());
        lines.push(format!("Song: {}", truncate_text(title, 34)));

        let mut detail = String::new();
        if let Some(chart_label) = song_info
            .chart_label
            .as_deref()
            .map(str::trim)
            .filter(|chart_label| !chart_label.is_empty())
        {
            detail.push_str(chart_label);
        }
        if let Some(rate) = song_info
            .rate
            .filter(|rate| rate.is_finite() && *rate > 0.0)
        {
            if !detail.is_empty() {
                detail.push_str("  ");
            }
            detail.push_str(format!("{rate:.2}x").as_str());
        }
        if !detail.is_empty() {
            lines.push(format!("      {}", truncate_text(detail.as_str(), 28)));
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
        let mut player_line = format!(
            "{}. {}",
            display_index + 1,
            truncate_text(player.label.as_str(), 24)
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
