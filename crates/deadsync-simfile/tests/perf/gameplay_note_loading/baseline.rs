// Frozen from 0.5.1699 for behavior and performance comparisons.
use super::*;

pub(super) fn build_gameplay_chart_from_ref(
    chart: &SerializableChartData,
    global_offset_seconds: f32,
) -> GameplayChartData {
    build_gameplay_chart_from_payload(
        CachedChartPayload {
            offset: chart.offset,
            notes: chart.notes.clone(),
            parsed_notes: chart.parsed_notes.clone(),
            row_to_beat: chart.row_to_beat.clone(),
            timing_segments: chart.timing_segments.clone(),
            chart_attacks: chart.chart_attacks.clone(),
        },
        global_offset_seconds,
    )
}
