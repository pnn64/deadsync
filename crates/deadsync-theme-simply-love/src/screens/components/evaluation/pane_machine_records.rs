use crate::act;
use crate::screens::evaluation::ScoreInfo;
use deadlib_present::actors::{Actor, SizeSpec, TextContent};
use deadlib_present::color;
use deadsync_profile as profile_data;
use deadsync_score as score_data;

use super::utils::pane_origin_x;

const MACHINE_RECORD_ROWS: usize = 10;
const MACHINE_RECORD_SPLIT_MACHINE_ROWS: usize = 8;
const MACHINE_RECORD_SPLIT_PERSONAL_ROWS: usize = 2;
const MACHINE_RECORD_DEFAULT_ROW_HEIGHT: f32 = 22.0;
const MACHINE_RECORD_SPLIT_ROW_HEIGHT: f32 = 20.25;
const MACHINE_RECORD_HIGHLIGHT_PERIOD_SECONDS: f32 = 4.0 / 3.0;

#[inline(always)]
const fn machine_record_rank_window(highlight_rank: Option<u32>) -> (u32, u32) {
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
fn machine_record_highlight_color(
    side: profile_data::PlayerSide,
    active_color_index: i32,
    elapsed_s: f32,
) -> [f32; 4] {
    let base = match side {
        profile_data::PlayerSide::P1 => color::simply_love_rgba(active_color_index),
        profile_data::PlayerSide::P2 => color::simply_love_rgba(active_color_index - 2),
    };
    let phase = ((elapsed_s / MACHINE_RECORD_HIGHLIGHT_PERIOD_SECONDS) * std::f32::consts::TAU)
        .sin()
        .mul_add(0.5, 0.5);
    let inv = 1.0 - phase;
    [
        base[0].mul_add(inv, phase),
        base[1].mul_add(inv, phase),
        base[2].mul_add(inv, phase),
        1.0,
    ]
}

#[derive(Clone)]
struct MachineRecordRowText {
    rank: TextContent,
    name: TextContent,
    score: TextContent,
    date: TextContent,
    highlight: bool,
}

impl MachineRecordRowText {
    fn new(entry: Option<&score_data::LeaderboardEntry>, rank: u32, highlight: bool) -> Self {
        let rank = super::retained_text(format_args!("{rank}."));
        let Some(entry) = entry else {
            return Self {
                rank,
                name: TextContent::static_str("----"),
                score: TextContent::static_str("------"),
                date: TextContent::static_str("----------"),
                highlight,
            };
        };

        let name = if entry.name.trim().is_empty() {
            TextContent::static_str("----")
        } else {
            super::retained_str(&entry.name)
        };
        let score = super::retained_text(format_args!(
            "{:.2}%",
            (entry.score / 100.0).clamp(0.0, 100.0)
        ));
        let date = score_data::format_leaderboard_date_or_placeholder(&entry.date);
        Self {
            rank,
            name,
            score,
            date: super::retained_str(&date),
            highlight,
        }
    }
}

/// Fixed actor-ready rows retained for one Evaluation result player.
///
/// Initialization compiles exactly ten rows. Actor frames clone bounded inline,
/// static, or shared payloads and only recompute the animated highlight color.
/// There are no live misses, growth, eviction, locking, or background work.
#[derive(Clone)]
pub(crate) struct MachineRecordsPaneText {
    rows: [MachineRecordRowText; MACHINE_RECORD_ROWS],
    split: bool,
}

impl MachineRecordsPaneText {
    pub(crate) fn new(score: &ScoreInfo) -> Self {
        Self::from_records(
            &score.machine_records,
            score.machine_record_highlight_rank,
            &score.personal_records,
            score.personal_record_highlight_rank,
            score.show_machine_personal_split,
        )
    }

    fn from_records(
        machine: &[score_data::LeaderboardEntry],
        machine_highlight: Option<u32>,
        personal: &[score_data::LeaderboardEntry],
        personal_highlight: Option<u32>,
        split: bool,
    ) -> Self {
        let (lower, _) = machine_record_rank_window(machine_highlight);
        let rows = std::array::from_fn(|row| {
            if split && row >= MACHINE_RECORD_SPLIT_MACHINE_ROWS {
                let index = row - MACHINE_RECORD_SPLIT_MACHINE_ROWS;
                let rank = index as u32 + 1;
                MachineRecordRowText::new(
                    personal.get(index),
                    rank,
                    personal_highlight == Some(rank),
                )
            } else {
                let rank = if split {
                    row as u32 + 1
                } else {
                    lower + row as u32
                };
                MachineRecordRowText::new(
                    machine.get(rank.saturating_sub(1) as usize),
                    rank,
                    !split && machine_highlight == Some(rank),
                )
            }
        });
        Self { rows, split }
    }
}

fn push_machine_record_row(
    children: &mut Vec<Actor>,
    text: &MachineRecordRowText,
    y: f32,
    rank_x: f32,
    name_x: f32,
    score_x: f32,
    date_x: f32,
    text_zoom: f32,
    col: [f32; 4],
) {
    children.push(act!(text:
        font("miso"):
        settext(text.rank.clone()):
        align(1.0, 0.5):
        xy(rank_x, y):
        zoom(text_zoom):
        z(101):
        diffuse(col[0], col[1], col[2], col[3]):
        horizalign(right)
    ));
    children.push(act!(text:
        font("miso"):
        settext(text.name.clone()):
        align(0.0, 0.5):
        xy(name_x, y):
        zoom(text_zoom):
        z(101):
        diffuse(col[0], col[1], col[2], col[3]):
        horizalign(left)
    ));
    children.push(act!(text:
        font("miso"):
        settext(text.score.clone()):
        align(0.0, 0.5):
        xy(score_x, y):
        zoom(text_zoom):
        z(101):
        diffuse(col[0], col[1], col[2], col[3]):
        horizalign(left)
    ));
    children.push(act!(text:
        font("miso"):
        settext(text.date.clone()):
        align(0.0, 0.5):
        xy(date_x, y):
        zoom(text_zoom):
        z(101):
        diffuse(col[0], col[1], col[2], col[3]):
        horizalign(left)
    ));
}

pub(crate) fn build_machine_records_pane(
    text: &MachineRecordsPaneText,
    controller: profile_data::PlayerSide,
    active_color_index: i32,
    elapsed_s: f32,
) -> Vec<Actor> {
    let pane_origin_x = pane_origin_x(controller);
    let pane_origin_y = deadlib_present::space::screen_center_y() - 62.0;
    let pane_zoom = 0.8_f32;
    let rank_x = -120.0 * pane_zoom;
    let name_x = -110.0 * pane_zoom;
    let score_x = -24.0 * pane_zoom;
    let date_x = 50.0 * pane_zoom;
    let text_zoom = pane_zoom;
    let hl = machine_record_highlight_color(controller, active_color_index, elapsed_s);

    let mut children = Vec::with_capacity(MACHINE_RECORD_ROWS * 4 + 1);

    if text.split {
        let row_height = MACHINE_RECORD_SPLIT_ROW_HEIGHT * pane_zoom;
        let first_row_y = row_height;
        for i in 0..MACHINE_RECORD_SPLIT_MACHINE_ROWS {
            push_machine_record_row(
                &mut children,
                &text.rows[i],
                (i as f32).mul_add(row_height, first_row_y),
                rank_x,
                name_x,
                score_x,
                date_x,
                text_zoom,
                [1.0, 1.0, 1.0, 1.0],
            );
        }

        let machine_rows_height = MACHINE_RECORD_SPLIT_MACHINE_ROWS as f32 * row_height;
        let split_y = row_height.mul_add(-0.5, first_row_y + machine_rows_height);
        children.push(act!(quad:
            align(0.5, 0.5):
            xy(0.0, split_y):
            setsize(100.0 * pane_zoom, 1.0 * pane_zoom):
            diffuse(1.0, 1.0, 1.0, 0.33):
            z(101)
        ));

        let first_personal_row_y = first_row_y + machine_rows_height;
        for i in 0..MACHINE_RECORD_SPLIT_PERSONAL_ROWS {
            let row = &text.rows[MACHINE_RECORD_SPLIT_MACHINE_ROWS + i];
            let col = if row.highlight { hl } else { [1.0; 4] };
            push_machine_record_row(
                &mut children,
                row,
                (i as f32).mul_add(row_height, first_personal_row_y),
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
        for (row_idx, row) in text.rows.iter().enumerate() {
            let col = if row.highlight { hl } else { [1.0; 4] };
            push_machine_record_row(
                &mut children,
                row,
                (row_idx as f32).mul_add(row_height, first_row_y),
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn entry(rank: u32, name: &str, score: f64, date: &str) -> score_data::LeaderboardEntry {
        score_data::LeaderboardEntry {
            rank,
            name: name.into(),
            machine_tag: None,
            score,
            date: date.into(),
            is_rival: false,
            is_self: false,
            is_fail: false,
        }
    }

    #[test]
    fn machine_record_text_retains_shifted_rank_window() {
        let machine: Vec<_> = (1..=12)
            .map(|rank| entry(rank, &format!("P{rank}"), 9_876.5, "2026-08-16"))
            .collect();
        let text = MachineRecordsPaneText::from_records(&machine, Some(12), &[], None, false);

        assert_eq!(text.rows[0].rank.as_str(), "3.");
        assert_eq!(text.rows[0].name.as_str(), "P3");
        assert_eq!(text.rows[0].score.as_str(), "98.77%");
        assert_eq!(text.rows[0].date.as_str(), "Aug 16, 2026");
        assert_eq!(text.rows[9].rank.as_str(), "12.");
        assert!(text.rows[9].highlight);
        assert!(text.rows[..9].iter().all(|row| !row.highlight));
    }

    #[test]
    fn machine_record_text_retains_split_and_placeholders() {
        let machine = [entry(1, "AAA", 10_000.0, "")];
        let personal = [
            entry(1, "BBB", 9_000.0, "2026-01-02"),
            entry(2, "CCC", 8_000.0, "2026-03-04"),
        ];
        let text = MachineRecordsPaneText::from_records(&machine, None, &personal, Some(2), true);

        assert!(text.split);
        assert_eq!(text.rows[1].name.as_str(), "----");
        assert_eq!(text.rows[1].score.as_str(), "------");
        assert_eq!(text.rows[1].date.as_str(), "----------");
        assert_eq!(text.rows[8].name.as_str(), "BBB");
        assert_eq!(text.rows[9].name.as_str(), "CCC");
        assert!(!text.rows[8].highlight);
        assert!(text.rows[9].highlight);
    }

    #[test]
    fn machine_record_text_shares_oversized_fields() {
        let long = "x".repeat(deadlib_present::actors::InlineText::CAPACITY + 1);
        let row = MachineRecordRowText::new(Some(&entry(1, &long, 10_000.0, &long)), 1, false);
        let name = row.name.clone();
        let date = row.date.clone();

        let (TextContent::Shared(source), TextContent::Shared(clone)) = (&row.name, &name) else {
            panic!("oversized record names should use shared text");
        };
        assert!(Arc::ptr_eq(source, clone));
        let (TextContent::Shared(source), TextContent::Shared(clone)) = (&row.date, &date) else {
            panic!("oversized record dates should use shared text");
        };
        assert!(Arc::ptr_eq(source, clone));
    }
}
