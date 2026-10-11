// Frozen from main 4e59125883b7cf5331779a0a95627fea47930fd9 for paired regression checks and benchmarks.
pub(super) fn meter_indices(
    song: &SongData,
    chart_type: &str,
    chart_indices: &WheelChartIndices,
) -> WheelMeterIndices {
    let mut indices = WheelMeterIndices::new();
    let mut insert = |chart_index: usize| {
        let meter = song.charts[chart_index].meter;
        if let Some((_, existing)) = indices.iter_mut().find(|(value, _)| *value == meter) {
            *existing = chart_index;
        } else {
            indices.push((meter, chart_index));
        }
    };

    for &chart_index in &chart_indices[..STANDARD_DIFFICULTY_COUNT] {
        if chart_index != NO_CHART_INDEX {
            insert(chart_index);
        }
    }
    for chart_index in song.edit_chart_indices_sorted(chart_type) {
        insert(chart_index);
    }
    indices.sort_unstable_by_key(|&(meter, _)| meter);
    indices
}
