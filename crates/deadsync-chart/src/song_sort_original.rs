// Frozen function bodies from 2080a074c.
use super::*;

pub(super) fn lowercase_cmp(left: &str, right: &str) -> Ordering {
    if left.is_ascii() && right.is_ascii() {
        return left
            .bytes()
            .map(|byte| byte.to_ascii_lowercase())
            .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()));
    }
    left.chars()
        .flat_map(char::to_lowercase)
        .cmp(right.chars().flat_map(char::to_lowercase))
}

pub(super) fn is_edit_chart(chart: &ChartData, chart_type: &str) -> bool {
    chart.chart_type.eq_ignore_ascii_case(chart_type)
        && chart.difficulty.eq_ignore_ascii_case("edit")
}

pub(super) fn edit_chart_cmp(left: &ChartData, right: &ChartData) -> Ordering {
    left.meter
        .cmp(&right.meter)
        .then_with(|| left.stats.total_steps.cmp(&right.stats.total_steps))
        .then_with(|| lowercase_cmp(&left.description, &right.description))
}

pub(super) fn edit_charts_sorted<'a>(song: &'a SongData, chart_type: &str) -> Vec<&'a ChartData> {
    let mut edits: Vec<&ChartData> = song
        .charts
        .iter()
        .filter(|chart| is_edit_chart(chart, chart_type))
        .collect();
    edits.sort_by(|left, right| edit_chart_cmp(left, right));
    edits
}
