use crate::act;
use crate::game::profile;
use crate::game::scores;
use crate::screens::evaluation::ScoreInfo;
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;

use super::utils::{format_machine_record_date, pane_origin_x};

const MACHINE_RECORD_ROWS: usize = 10;
const MACHINE_RECORD_SPLIT_MACHINE_ROWS: usize = 8;
const MACHINE_RECORD_SPLIT_PERSONAL_ROWS: usize = 2;
const MACHINE_RECORD_DEFAULT_ROW_HEIGHT: f32 = 22.0;
const MACHINE_RECORD_SPLIT_ROW_HEIGHT: f32 = 20.25;
const MACHINE_RECORD_SPLIT_SEPARATOR_Y_ROWS: f32 = 9.0;
const MACHINE_RECORD_HIGHLIGHT_PERIOD_SECONDS: f32 = 4.0 / 3.0;

#[inline(always)]
fn machine_record_rank_window(highlight_rank: Option<u32>) -> (u32, u32) {
    let mut lower: u32 = 1;
    let mut upper: u32 = MACHINE_RECORD_ROWS as u32;
    if let Some(rank) = highlight_rank
        && rank > upper
    {
        lower = lower.saturating_add(rank - upper);
        upper = rank;
    }
    (lower, upper)
}

#[inline(always)]
fn format_machine_record_score(score_10000: f64) -> String {
    format!("{:.2}%", (score_10000 / 100.0).clamp(0.0, 100.0))
}

#[inline(always)]
fn machine_record_highlight_color(
    side: profile::PlayerSide,
    active_color_index: i32,
    elapsed_s: f32,
) -> [f32; 4] {
    let base = match side {
        profile::PlayerSide::P1 => color::simply_love_rgba(active_color_index),
        profile::PlayerSide::P2 => color::simply_love_rgba(active_color_index - 2),
    };
    let phase =
        ((elapsed_s / MACHINE_RECORD_HIGHLIGHT_PERIOD_SECONDS) * std::f32::consts::TAU).sin() * 0.5
            + 0.5;
    let inv = 1.0 - phase;
    [
        base[0] * inv + phase,
        base[1] * inv + phase,
        base[2] * inv + phase,
        1.0,
    ]
}

fn push_machine_record_row(
    children: &mut Vec<Actor>,
    entry: Option<&scores::LeaderboardEntry>,
    rank: u32,
    y: f32,
    rank_x: f32,
    name_x: f32,
    score_x: f32,
    date_x: f32,
    text_zoom: f32,
    col: [f32; 4],
) {
    let (name, score, date) = if let Some(entry) = entry {
        let name = if entry.name.trim().is_empty() {
            "----".to_string()
        } else {
            entry.name.clone()
        };
        (
            name,
            format_machine_record_score(entry.score),
            format_machine_record_date(&entry.date),
        )
    } else {
        (
            "----".to_string(),
            "------".to_string(),
            "----------".to_string(),
        )
    };

    children.push(act!(text:
        font("miso"):
        settext(format!("{rank}.")):
        align(1.0, 0.5):
        xy(rank_x, y):
        zoom(text_zoom):
        z(101):
        diffuse(col[0], col[1], col[2], col[3]):
        horizalign(right)
    ));
    children.push(act!(text:
        font("miso"):
        settext(name):
        align(0.0, 0.5):
        xy(name_x, y):
        zoom(text_zoom):
        z(101):
        diffuse(col[0], col[1], col[2], col[3]):
        horizalign(left)
    ));
    children.push(act!(text:
        font("miso"):
        settext(score):
        align(0.0, 0.5):
        xy(score_x, y):
        zoom(text_zoom):
        z(101):
        diffuse(col[0], col[1], col[2], col[3]):
        horizalign(left)
    ));
    children.push(act!(text:
        font("miso"):
        settext(date):
        align(0.0, 0.5):
        xy(date_x, y):
        zoom(text_zoom):
        z(101):
        diffuse(col[0], col[1], col[2], col[3]):
        horizalign(left)
    ));
}

pub fn build_machine_records_pane(
    score_info: &ScoreInfo,
    controller: profile::PlayerSide,
    active_color_index: i32,
    elapsed_s: f32,
) -> Vec<Actor> {
    let pane_origin_x = pane_origin_x(controller);
    let pane_origin_y = crate::core::space::screen_center_y() - 62.0;
    let pane_zoom = 0.8_f32;
    let rank_x = -120.0 * pane_zoom;
    let name_x = -110.0 * pane_zoom;
    let score_x = -24.0 * pane_zoom;
    let date_x = 50.0 * pane_zoom;
    let text_zoom = pane_zoom;
    let hl = machine_record_highlight_color(controller, active_color_index, elapsed_s);

    let mut children = Vec::with_capacity(MACHINE_RECORD_ROWS * 4 + 1);

    if score_info.show_machine_personal_split {
        let row_height = MACHINE_RECORD_SPLIT_ROW_HEIGHT * pane_zoom;
        let first_row_y = row_height;
        for i in 0..MACHINE_RECORD_SPLIT_MACHINE_ROWS {
            let rank = (i as u32).saturating_add(1);
            push_machine_record_row(
                &mut children,
                score_info.machine_records.get(i),
                rank,
                first_row_y + i as f32 * row_height,
                rank_x,
                name_x,
                score_x,
                date_x,
                text_zoom,
                [1.0, 1.0, 1.0, 1.0],
            );
        }

        let split_y = first_row_y
            + MACHINE_RECORD_SPLIT_SEPARATOR_Y_ROWS * MACHINE_RECORD_SPLIT_ROW_HEIGHT * pane_zoom;
        children.push(act!(quad:
            align(0.5, 0.5):
            xy(0.0, split_y):
            setsize(100.0 * pane_zoom, 1.0 * pane_zoom):
            diffuse(1.0, 1.0, 1.0, 0.33):
            z(101)
        ));

        for i in 0..MACHINE_RECORD_SPLIT_PERSONAL_ROWS {
            let rank = (i as u32).saturating_add(1);
            let col = if score_info.personal_record_highlight_rank == Some(rank) {
                hl
            } else {
                [1.0, 1.0, 1.0, 1.0]
            };
            push_machine_record_row(
                &mut children,
                score_info.personal_records.get(i),
                rank,
                split_y + i as f32 * row_height,
                rank_x,
                name_x,
                score_x,
                date_x,
                text_zoom,
                col,
            );
        }
    } else {
        let row_height = MACHINE_RECORD_DEFAULT_ROW_HEIGHT * pane_zoom;
        let first_row_y = row_height;
        let (lower, upper) = machine_record_rank_window(score_info.machine_record_highlight_rank);
        for (row_idx, rank) in (lower..=upper).enumerate() {
            let col = if score_info.machine_record_highlight_rank == Some(rank) {
                hl
            } else {
                [1.0, 1.0, 1.0, 1.0]
            };
            push_machine_record_row(
                &mut children,
                score_info
                    .machine_records
                    .get(rank.saturating_sub(1) as usize),
                rank,
                first_row_y + row_idx as f32 * row_height,
                rank_x,
                name_x,
                score_x,
                date_x,
                text_zoom,
                col,
            );
        }
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
