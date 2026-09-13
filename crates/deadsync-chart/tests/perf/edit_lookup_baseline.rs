// Frozen from 91b7bcfca (0.5.1202); method names/access adapted for this binary.
use deadsync_chart::{ChartData, STANDARD_DIFFICULTY_COUNT, STANDARD_DIFFICULTY_NAMES, SongData};
use std::cmp::Ordering;

#[inline]
fn lowercase_cmp(left: &str, right: &str) -> Ordering {
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

#[inline]
fn edit_chart_cmp(left: &ChartData, right: &ChartData) -> Ordering {
    left.meter
        .cmp(&right.meter)
        .then_with(|| left.stats.total_steps.cmp(&right.stats.total_steps))
        .then_with(|| lowercase_cmp(&left.description, &right.description))
}

#[inline(always)]
fn is_edit_chart(chart: &ChartData, chart_type: &str) -> bool {
    chart.chart_type.eq_ignore_ascii_case(chart_type)
        && chart.difficulty.eq_ignore_ascii_case("edit")
}
pub(super) trait BaselineSong {
    fn baseline_edit_charts_sorted(&self, chart_type: &str) -> Vec<&ChartData>;
    fn baseline_edit_chart_index_cmp(&self, left: usize, right: usize) -> Ordering;
    fn baseline_first_edit_chart(&self, chart_type: &str) -> Option<&ChartData>;
    fn baseline_chart_for_steps_index(
        &self,
        chart_type: &str,
        steps_index: usize,
    ) -> Option<&ChartData>;
}
impl BaselineSong for SongData {
    fn baseline_edit_charts_sorted(&self, chart_type: &str) -> Vec<&ChartData> {
        let mut edits: Vec<&ChartData> = self
            .charts
            .iter()
            .filter(|chart| is_edit_chart(chart, chart_type))
            .collect();
        edits.sort_by(|left, right| edit_chart_cmp(left, right));
        edits
    }

    #[inline]
    fn baseline_edit_chart_index_cmp(&self, left: usize, right: usize) -> Ordering {
        edit_chart_cmp(&self.charts[left], &self.charts[right]).then_with(|| left.cmp(&right))
    }

    #[inline]
    fn baseline_first_edit_chart(&self, chart_type: &str) -> Option<&ChartData> {
        self.charts
            .iter()
            .enumerate()
            .filter(|(_, chart)| is_edit_chart(chart, chart_type))
            .min_by(|(left, _), (right, _)| self.baseline_edit_chart_index_cmp(*left, *right))
            .map(|(_, chart)| chart)
    }

    #[inline]
    fn baseline_chart_for_steps_index(
        &self,
        chart_type: &str,
        steps_index: usize,
    ) -> Option<&ChartData> {
        if let Some(diff_name) = STANDARD_DIFFICULTY_NAMES.get(steps_index) {
            return self.charts.iter().find(|chart| {
                chart.chart_type.eq_ignore_ascii_case(chart_type)
                    && chart.difficulty.eq_ignore_ascii_case(diff_name)
            });
        }

        let edit_index = steps_index.checked_sub(STANDARD_DIFFICULTY_COUNT)?;
        if edit_index == 0 {
            return self.baseline_first_edit_chart(chart_type);
        }
        self.baseline_edit_charts_sorted(chart_type)
            .get(edit_index)
            .copied()
    }
}
