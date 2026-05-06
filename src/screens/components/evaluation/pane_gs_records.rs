use crate::act;
use crate::engine::present::actors::{Actor, SizeSpec};
use crate::engine::present::color;
use crate::game::profile;
use crate::game::scores;
use crate::screens::components::shared::gs_scorebox::entries_with_local_self_state;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecordsPaneKind {
    GrooveStatsItg,
    GrooveStatsEx,
    ItlEx,
    ArrowCloudHardEx,
}

impl RecordsPaneKind {
    #[inline(always)]
    fn matches(self, pane: &scores::LeaderboardPane) -> bool {
        match self {
            Self::GrooveStatsItg => pane.is_groovestats() && !pane.is_ex,
            Self::GrooveStatsEx => pane.is_groovestats() && pane.is_ex,
            Self::ItlEx => pane.name.to_ascii_lowercase().contains("itl") && pane.is_ex,
            Self::ArrowCloudHardEx => pane.is_arrowcloud() && pane.is_hard_ex(),
        }
    }

    #[inline(always)]
    const fn logo(self) -> &'static str {
        match self {
            Self::ItlEx => "ITL.png",
            Self::ArrowCloudHardEx => "arrowcloud.png",
            Self::GrooveStatsItg | Self::GrooveStatsEx => "GrooveStats.png",
        }
    }

    #[inline(always)]
    const fn logo_zoom(self, pane_zoom: f32) -> f32 {
        match self {
            Self::ArrowCloudHardEx => 0.22,
            Self::ItlEx => 0.45,
            Self::GrooveStatsItg | Self::GrooveStatsEx => 1.5 * pane_zoom,
        }
    }

    #[inline(always)]
    const fn mode_text(self) -> &'static str {
        match self {
            Self::GrooveStatsItg => "ITG",
            Self::GrooveStatsEx => "EX",
            Self::ItlEx => "ITL EX",
            Self::ArrowCloudHardEx => "H.EX",
        }
    }

    #[inline(always)]
    const fn mode_color(self) -> [f32; 4] {
        match self {
            Self::GrooveStatsEx | Self::ItlEx => color::JUDGMENT_RGBA[0],
            Self::ArrowCloudHardEx => color::HARD_EX_SCORE_RGBA,
            Self::GrooveStatsItg => [1.0, 1.0, 1.0, 1.0],
        }
    }
}

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

#[inline(always)]
fn same_leaderboard_entry(a: &scores::LeaderboardEntry, b: &scores::LeaderboardEntry) -> bool {
    a.rank == b.rank && a.name.eq_ignore_ascii_case(b.name.as_str())
}

#[inline(always)]
fn selected_contains(
    selected: &[&scores::LeaderboardEntry],
    entry: &scores::LeaderboardEntry,
) -> bool {
    selected
        .iter()
        .any(|chosen| same_leaderboard_entry(chosen, entry))
}

fn next_record_entry<'a>(
    entries: &'a [scores::LeaderboardEntry],
    selected: &[&'a scores::LeaderboardEntry],
    include: impl Fn(&scores::LeaderboardEntry) -> bool,
) -> Option<&'a scores::LeaderboardEntry> {
    entries
        .iter()
        .filter(|entry| include(entry) && !selected_contains(selected, entry))
        .min_by_key(|entry| entry.rank)
}

fn prioritized_record_entries(
    entries: &[scores::LeaderboardEntry],
    max_rows: usize,
) -> Vec<scores::LeaderboardEntry> {
    if max_rows == 0 {
        return Vec::new();
    }
    if entries.len() <= max_rows {
        return entries.to_vec();
    }

    let mut selected = Vec::with_capacity(max_rows);
    if let Some(top) = next_record_entry(entries, selected.as_slice(), |_| true) {
        selected.push(top);
    }
    if let Some(self_entry) = next_record_entry(entries, selected.as_slice(), |entry| entry.is_self)
    {
        selected.push(self_entry);
    }
    while selected.len() < max_rows {
        let Some(rival) = next_record_entry(entries, selected.as_slice(), |entry| entry.is_rival)
        else {
            break;
        };
        selected.push(rival);
    }
    while selected.len() < max_rows {
        let Some(entry) = next_record_entry(entries, selected.as_slice(), |_| true) else {
            break;
        };
        selected.push(entry);
    }
    selected.sort_unstable_by_key(|entry| entry.rank);
    selected.into_iter().cloned().collect()
}

fn pane_display_entries(
    score_side: profile::PlayerSide,
    chart_hash: Option<&str>,
    pane: &scores::LeaderboardPane,
) -> Vec<scores::LeaderboardEntry> {
    let entries = entries_with_local_self_state(score_side, chart_hash, pane);
    prioritized_record_entries(entries.as_slice(), GS_RECORD_ROWS)
}

fn build_records_pane(
    controller: profile::PlayerSide,
    score_side: profile::PlayerSide,
    chart_hash: Option<&str>,
    snapshot: Option<&scores::CachedPlayerLeaderboardData>,
    kind: RecordsPaneKind,
) -> Vec<Actor> {
    let pane_origin_x = pane_origin_x(controller);
    let pane_origin_y = crate::engine::space::screen_center_y() - 62.0;
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
            let records_pane = snapshot
                .data
                .as_ref()
                .and_then(|data| data.panes.iter().find(|pane| kind.matches(pane)));
            if let Some(pane) = records_pane {
                let display_entries = pane_display_entries(score_side, chart_hash, pane);
                if display_entries.is_empty() {
                    rows.push((
                        String::new(),
                        GS_NO_SCORES_TEXT.to_string(),
                        String::new(),
                        String::new(),
                        [1.0, 1.0, 1.0, 1.0],
                        [1.0, 1.0, 1.0, 1.0],
                    ));
                } else {
                    for entry in display_entries {
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
                            gs_player_name(&entry),
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

    let mut children = Vec::with_capacity(GS_RECORD_ROWS * 4 + 2);
    let mode_col = kind.mode_color();
    children.push(act!(text:
        font("miso"):
        settext(kind.mode_text()):
        align(0.5, 0.5):
        xy(0.0, -4.0):
        zoom(0.5 * pane_zoom):
        diffuse(mode_col[0], mode_col[1], mode_col[2], mode_col[3]):
        z(102)
    ));
    children.push(act!(sprite(kind.logo()):
        align(0.5, 0.5):
        xy(0.0, 100.0 * pane_zoom):
        zoom(kind.logo_zoom(pane_zoom)):
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
    score_side: profile::PlayerSide,
    chart_hash: Option<&str>,
    snapshot: Option<&scores::CachedPlayerLeaderboardData>,
) -> Vec<Actor> {
    build_records_pane(
        controller,
        score_side,
        chart_hash,
        snapshot,
        RecordsPaneKind::GrooveStatsItg,
    )
}

pub fn build_gs_ex_records_pane(
    controller: profile::PlayerSide,
    score_side: profile::PlayerSide,
    chart_hash: Option<&str>,
    snapshot: Option<&scores::CachedPlayerLeaderboardData>,
) -> Vec<Actor> {
    build_records_pane(
        controller,
        score_side,
        chart_hash,
        snapshot,
        RecordsPaneKind::GrooveStatsEx,
    )
}

pub fn build_itl_records_pane(
    controller: profile::PlayerSide,
    score_side: profile::PlayerSide,
    chart_hash: Option<&str>,
    snapshot: Option<&scores::CachedPlayerLeaderboardData>,
) -> Vec<Actor> {
    build_records_pane(
        controller,
        score_side,
        chart_hash,
        snapshot,
        RecordsPaneKind::ItlEx,
    )
}

pub fn build_arrowcloud_records_pane(
    controller: profile::PlayerSide,
    score_side: profile::PlayerSide,
    chart_hash: Option<&str>,
    snapshot: Option<&scores::CachedPlayerLeaderboardData>,
) -> Vec<Actor> {
    build_records_pane(
        controller,
        score_side,
        chart_hash,
        snapshot,
        RecordsPaneKind::ArrowCloudHardEx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rank: u32, name: &str, is_self: bool, is_rival: bool) -> scores::LeaderboardEntry {
        scores::LeaderboardEntry {
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

    fn pane(
        name: &str,
        is_ex: bool,
        arrowcloud_kind: Option<scores::ArrowCloudPaneKind>,
    ) -> scores::LeaderboardPane {
        scores::LeaderboardPane {
            name: name.to_string(),
            entries: vec![entry(1, name, false, false)],
            is_ex,
            disabled: false,
            personalized: true,
            arrowcloud_kind,
        }
    }

    #[test]
    fn prioritized_entries_keep_self_and_rivals_visible() {
        let mut entries = (1..=12)
            .map(|rank| entry(rank, &format!("top-{rank}"), false, false))
            .collect::<Vec<_>>();
        entries.push(entry(20, "self", true, false));
        entries.push(entry(30, "rival-a", false, true));
        entries.push(entry(40, "rival-b", false, true));

        let selected = prioritized_record_entries(entries.as_slice(), GS_RECORD_ROWS);
        let ranks = selected.iter().map(|entry| entry.rank).collect::<Vec<_>>();

        assert_eq!(ranks, vec![1, 2, 3, 4, 5, 6, 7, 20, 30, 40]);
    }

    #[test]
    fn records_pane_kind_selects_distinct_online_boards() {
        let panes = [
            pane("GrooveStats", false, None),
            pane("GrooveStats", true, None),
            pane("ITL Online 2026", true, None),
            pane(
                "ArrowCloud",
                false,
                Some(scores::ArrowCloudPaneKind::HardEx),
            ),
        ];

        assert!(RecordsPaneKind::GrooveStatsItg.matches(&panes[0]));
        assert!(RecordsPaneKind::GrooveStatsEx.matches(&panes[1]));
        assert!(RecordsPaneKind::ItlEx.matches(&panes[2]));
        assert!(RecordsPaneKind::ArrowCloudHardEx.matches(&panes[3]));

        assert!(!RecordsPaneKind::GrooveStatsItg.matches(&panes[1]));
        assert!(!RecordsPaneKind::GrooveStatsEx.matches(&panes[0]));
        assert!(!RecordsPaneKind::ItlEx.matches(&panes[1]));
        assert!(!RecordsPaneKind::ArrowCloudHardEx.matches(&panes[2]));
    }
}
