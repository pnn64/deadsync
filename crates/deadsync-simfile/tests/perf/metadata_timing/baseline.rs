// Frozen from 68e7d6aae (0.5.1704).
use super::*;

pub(super) fn update_precise_song_bounds(
    song: &mut SerializableSongData,
    global_offset_seconds: f32,
) {
    let has_non_edit = song
        .charts
        .iter()
        .any(|chart| !chart.difficulty.eq_ignore_ascii_case("edit"));
    let mut first = f32::INFINITY;
    // ITGmania seeds Song::ReCalculateStepStatsAndLastSecond with the hint,
    // then extends it for later notes. It is an absolute music second, not a beat.
    let mut last = song.last_second_hint;
    for chart in &song.charts {
        if !song_length_chart_candidate(chart, has_non_edit) {
            continue;
        }

        let mut first_row: Option<usize> = None;
        let mut last_row: Option<usize> = None;
        for note in &chart.parsed_notes {
            let head_row = note.row_index as usize;
            let row = note.tail_row_index.unwrap_or(note.row_index) as usize;
            first_row = Some(first_row.map_or(head_row, |prev| prev.min(head_row)));
            last_row = Some(last_row.map_or(row, |prev| prev.max(row)));
        }

        let Some(row) = last_row else {
            continue;
        };
        if row == 0 {
            continue;
        }
        let Some(first_row) = first_row else {
            continue;
        };
        let Some(first_beat) = chart.row_to_beat.get(first_row).copied() else {
            continue;
        };
        let Some(beat) = chart.row_to_beat.get(row).copied() else {
            continue;
        };
        let timing_segments: TimingSegments = chart.timing_segments.clone().into();
        // Only the two resolved beats are queried; this temporary timing does
        // not need its own copy of the chart's full row table.
        let timing =
            TimingData::from_segments(-chart.offset, global_offset_seconds, &timing_segments, &[]);
        let first_sec = timing.get_time_for_beat(first_beat);
        if first_sec.is_finite() {
            first = first.min(first_sec);
        }
        let sec = timing.get_time_for_beat(beat);
        if sec.is_finite() {
            last = last.max(sec.max(0.0));
        }
    }

    song.first_second = if first.is_finite() && first < last {
        first
    } else {
        0.0
    };
    let fallback = song.total_length_seconds.max(0) as f32;
    song.precise_last_second_seconds = last.max(fallback);
}

pub(super) fn compute_cached_chart_meta(
    chart: &SerializableChartData,
    global_offset_seconds: f32,
) -> ComputedCachedChartMeta {
    let timing_segments: TimingSegments = chart.timing_segments.clone().into();
    // Keep the row table borrowed for totals instead of cloning it into timing.
    let timing =
        TimingData::from_segments(-chart.offset, global_offset_seconds, &timing_segments, &[]);
    let (possible_grade_points, holds_total, rolls_total, mines_total) =
        build_chart_totals_with_rows(&chart.parsed_notes, &timing, |row| {
            chart.row_to_beat.get(row).copied()
        });
    let first_second = 0.0_f32.min(timing.get_time_for_beat(0.0));
    let measure_seconds_vec = build_measure_seconds(&timing, chart.measure_nps_vec.len());
    ComputedCachedChartMeta {
        measure_seconds_vec,
        first_second,
        has_note_data: !chart.notes.is_empty(),
        has_chart_attacks: chart_has_attacks(chart.chart_attacks.as_deref()),
        possible_grade_points,
        holds_total,
        rolls_total,
        mines_total,
    }
}
pub fn build_cached_chart_meta(
    chart: &SerializableChartData,
    global_offset_seconds: f32,
) -> CachedChartMeta {
    let computed = compute_cached_chart_meta(chart, global_offset_seconds);
    CachedChartMeta {
        chart_type: chart.chart_type.clone(),
        difficulty: chart.difficulty.clone(),
        description: chart.description.clone(),
        chart_name: chart.chart_name.clone(),
        meter: chart.meter,
        step_artist: chart.step_artist.clone(),
        music_path: chart.music_path.clone(),
        short_hash: chart.short_hash.clone(),
        stats: chart.stats.clone(),
        tech_counts: chart.tech_counts,
        mines_nonfake: chart.mines_nonfake,
        stamina_counts: chart.stamina_counts,
        total_streams: chart.total_streams,
        matrix_rating: chart.matrix_rating,
        matrix_profile: chart.matrix_profile.clone(),
        max_nps: chart.max_nps,
        sn_detailed_breakdown: chart.sn_detailed_breakdown.clone(),
        sn_partial_breakdown: chart.sn_partial_breakdown.clone(),
        sn_simple_breakdown: chart.sn_simple_breakdown.clone(),
        detailed_breakdown: chart.detailed_breakdown.clone(),
        partial_breakdown: chart.partial_breakdown.clone(),
        simple_breakdown: chart.simple_breakdown.clone(),
        total_measures: chart.total_measures,
        measure_nps_vec: chart.measure_nps_vec.clone(),
        measure_seconds_vec: computed.measure_seconds_vec,
        first_second: computed.first_second,
        has_note_data: computed.has_note_data,
        has_chart_attacks: computed.has_chart_attacks,
        possible_grade_points: computed.possible_grade_points,
        holds_total: computed.holds_total,
        rolls_total: computed.rolls_total,
        mines_total: computed.mines_total,
        display_bpm: chart.display_bpm.clone(),
        min_bpm: chart.min_bpm,
        max_bpm: chart.max_bpm,
    }
}
