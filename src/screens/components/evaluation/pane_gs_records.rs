use crate::act;
use crate::game::profile;
use crate::game::scores;
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;

use super::utils::{format_machine_record_date, pane_origin_x};

const GS_RECORD_ROWS: usize = 10;
const GS_LOADING_TEXT: &str = "Loading ...";
const GS_NO_SCORES_TEXT: &str = "No Scores";
const GS_ERROR_TIMEOUT: &str = "Timed Out";
const GS_ERROR_FAILED: &str = "Failed to Load 😞";
const GS_ERROR_DISABLED: &str = "Disabled";
const GS_ROW_PLACEHOLDER_RANK: &str = "---";
const GS_ROW_PLACEHOLDER_NAME: &str = "----";
const GS_ROW_PLACEHOLDER_SCORE: &str = "------";
const GS_ROW_PLACEHOLDER_DATE: &str = "----------";
const GS_RIVAL_COLOR: [f32; 4] = color::rgba_hex("#BD94FF");
const GS_SELF_COLOR: [f32; 4] = color::rgba_hex("#A1FF94");

fn format_gs_error_text(error: &str) -> String {
    if error.eq_ignore_ascii_case("disabled") {
        return GS_ERROR_DISABLED.to_string();
    }
    let lower = error.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        GS_ERROR_TIMEOUT.to_string()
    } else {
        GS_ERROR_FAILED.to_string()
    }
}

fn gs_player_name(entry: &scores::LeaderboardEntry) -> String {
    let trimmed_name = entry.name.trim();
    if !trimmed_name.is_empty() {
        return trimmed_name.to_string();
    }
    if let Some(tag) = entry.machine_tag.as_deref() {
        let trimmed_tag = tag.trim();
        if !trimmed_tag.is_empty() {
            return trimmed_tag.to_string();
        }
    }
    GS_ROW_PLACEHOLDER_NAME.to_string()
}

fn build_records_pane(
    controller: profile::PlayerSide,
    snapshot: Option<&scores::CachedPlayerLeaderboardData>,
    arrowcloud: bool,
) -> Vec<Actor> {
    let pane_origin_x = pane_origin_x(controller);
    let pane_origin_y = crate::core::space::screen_center_y() - 62.0;
    let pane_zoom = 0.8_f32;
    let row_height = 22.0 * pane_zoom;
    let first_row_y = row_height;
    let rank_x = -130.0 * pane_zoom;
    let name_x = -120.0 * pane_zoom;
    let score_x = 16.0 * pane_zoom;
    let date_x = 72.0 * pane_zoom;
    let text_zoom = pane_zoom;
    let rank_max_width = 55.0;
    let name_max_width = 130.0;

    let mut rows: Vec<(String, String, String, String, [f32; 4], [f32; 4])> =
        Vec::with_capacity(GS_RECORD_ROWS);

    match snapshot {
        None => {
            rows.push((
                String::new(),
                GS_ERROR_DISABLED.to_string(),
                String::new(),
                String::new(),
                [1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
            ));
        }
        Some(snapshot) if snapshot.loading => {
            rows.push((
                String::new(),
                GS_LOADING_TEXT.to_string(),
                String::new(),
                String::new(),
                [1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
            ));
        }
        Some(snapshot) if snapshot.error.is_some() => {
            rows.push((
                String::new(),
                format_gs_error_text(snapshot.error.as_deref().unwrap_or_default()),
                String::new(),
                String::new(),
                [1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
            ));
        }
        Some(snapshot) => {
            let records_pane = snapshot.data.as_ref().and_then(|data| {
                data.panes.iter().find(|pane| {
                    if arrowcloud {
                        pane.is_arrowcloud()
                    } else {
                        pane.is_groovestats()
                    }
                })
            });
            if let Some(pane) = records_pane {
                if pane.entries.is_empty() {
                    rows.push((
                        String::new(),
                        GS_NO_SCORES_TEXT.to_string(),
                        String::new(),
                        String::new(),
                        [1.0, 1.0, 1.0, 1.0],
                        [1.0, 1.0, 1.0, 1.0],
                    ));
                } else {
                    for entry in pane.entries.iter().take(GS_RECORD_ROWS) {
                        let base_col = if entry.is_rival {
                            GS_RIVAL_COLOR
                        } else if entry.is_self {
                            GS_SELF_COLOR
                        } else {
                            [1.0, 1.0, 1.0, 1.0]
                        };
                        let mut score_col = if pane.is_ex {
                            color::JUDGMENT_RGBA[0]
                        } else if pane.is_hard_ex() {
                            color::HARD_EX_SCORE_RGBA
                        } else {
                            base_col
                        };
                        if entry.is_fail {
                            score_col = [1.0, 0.0, 0.0, 1.0];
                        }
                        rows.push((
                            format!("{}.", entry.rank),
                            gs_player_name(entry),
                            format!("{:.2}%", entry.score / 100.0),
                            format_machine_record_date(&entry.date),
                            base_col,
                            score_col,
                        ));
                    }
                }
            } else {
                rows.push((
                    String::new(),
                    GS_NO_SCORES_TEXT.to_string(),
                    String::new(),
                    String::new(),
                    [1.0, 1.0, 1.0, 1.0],
                    [1.0, 1.0, 1.0, 1.0],
                ));
            }
        }
    }

    while rows.len() < GS_RECORD_ROWS {
        rows.push((
            GS_ROW_PLACEHOLDER_RANK.to_string(),
            GS_ROW_PLACEHOLDER_NAME.to_string(),
            GS_ROW_PLACEHOLDER_SCORE.to_string(),
            GS_ROW_PLACEHOLDER_DATE.to_string(),
            [1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
        ));
    }

    let mut children = Vec::with_capacity(GS_RECORD_ROWS * 4 + 1);
    let logo = if arrowcloud {
        "arrowcloud.png"
    } else {
        "GrooveStats.png"
    };
    let logo_zoom = if arrowcloud { 0.22 } else { 1.5 * pane_zoom };
    children.push(act!(sprite(logo):
        align(0.5, 0.5):
        xy(0.0, 100.0 * pane_zoom):
        zoom(logo_zoom):
        diffuse(1.0, 1.0, 1.0, 0.5):
        z(100)
    ));
    for (i, (rank, name, score, date, row_col, score_col)) in rows.into_iter().enumerate() {
        let y = first_row_y + i as f32 * row_height;
        children.push(act!(text:
            font("miso"):
            settext(rank):
            align(1.0, 0.5):
            xy(rank_x, y):
            zoom(text_zoom):
            maxwidth(rank_max_width):
            z(101):
            diffuse(row_col[0], row_col[1], row_col[2], row_col[3]):
            horizalign(right)
        ));
        children.push(act!(text:
            font("miso"):
            settext(name):
            align(0.0, 0.5):
            xy(name_x, y):
            zoom(text_zoom):
            maxwidth(name_max_width):
            z(101):
            diffuse(row_col[0], row_col[1], row_col[2], row_col[3]):
            horizalign(left)
        ));
        children.push(act!(text:
            font("miso"):
            settext(score):
            align(0.0, 0.5):
            xy(score_x, y):
            zoom(text_zoom):
            z(101):
            diffuse(score_col[0], score_col[1], score_col[2], score_col[3]):
            horizalign(left)
        ));
        children.push(act!(text:
            font("miso"):
            settext(date):
            align(0.0, 0.5):
            xy(date_x, y):
            zoom(text_zoom):
            z(101):
            diffuse(row_col[0], row_col[1], row_col[2], row_col[3]):
            horizalign(left)
        ));
    }

    vec![Actor::Frame {
        align: [0.5, 0.5],
        offset: [pane_origin_x, pane_origin_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        background: None,
        z: 101,
        children,
    }]
}

pub fn build_gs_records_pane(
    controller: profile::PlayerSide,
    snapshot: Option<&scores::CachedPlayerLeaderboardData>,
) -> Vec<Actor> {
    build_records_pane(controller, snapshot, false)
}

pub fn build_arrowcloud_records_pane(
    controller: profile::PlayerSide,
    snapshot: Option<&scores::CachedPlayerLeaderboardData>,
) -> Vec<Actor> {
    build_records_pane(controller, snapshot, true)
}
