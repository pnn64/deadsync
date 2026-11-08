//! Frozen baseline functions for paired regression tests and benchmarks.
use super::*;

pub(super) fn default_chart(charts: &[PreviewChart]) -> Option<usize> {
    let top = |doubles: bool| {
        charts.iter().position(|chart| {
            chart.doubles == doubles
                && matches!(
                    chart.difficulty.to_ascii_lowercase().as_str(),
                    "challenge" | "expert"
                )
        })
    };
    top(false)
        .or_else(|| top(true))
        .or_else(|| charts.iter().rposition(|chart| !chart.doubles))
        .or_else(|| charts.len().checked_sub(1))
}
