// Frozen from d49e2923567b5f1bc32e599b0bceda123c67ea56.
use super::*;
#[must_use]
pub fn cabinet_light_plan_original(
    song: &SongData,
    fallback_chart_ix: usize,
) -> Option<CabinetLightPlan> {
    if let Some(chart_ix) = closest_standard_chart_ix_original(
        song,
        LIGHTS_CABINET_CHART_TYPE,
        LIGHTS_EXPLICIT_DIFFICULTY_INDEX,
    ) {
        return Some(CabinetLightPlan::Explicit {
            chart_ix,
            chart_hash: song.charts[chart_ix].short_hash.clone(),
        });
    }

    song.charts
        .get(fallback_chart_ix)
        .filter(|chart| chart.has_note_data)?;
    let marquee_ix = closest_standard_chart_ix_original(
        song,
        LIGHTS_PRIMARY_CHART_TYPE,
        LIGHTS_MARQUEE_DIFFICULTY_INDEX,
    )
    .unwrap_or(fallback_chart_ix);
    let bass_ix = closest_standard_chart_ix_original(
        song,
        LIGHTS_PRIMARY_CHART_TYPE,
        LIGHTS_BASS_DIFFICULTY_INDEX,
    )
    .unwrap_or(fallback_chart_ix);

    Some(CabinetLightPlan::Generated {
        marquee_ix,
        marquee_hash: song.charts[marquee_ix].short_hash.clone(),
        bass_ix,
        bass_hash: song.charts[bass_ix].short_hash.clone(),
    })
}

pub(super) fn closest_standard_chart_ix_original(
    song: &SongData,
    chart_type: &str,
    preferred_difficulty_index: usize,
) -> Option<usize> {
    let preferred = preferred_difficulty_index.min(STANDARD_DIFFICULTY_COUNT.saturating_sub(1));
    let mut best = None;
    let mut best_distance = usize::MAX;
    for (chart_ix, chart) in song.charts.iter().enumerate() {
        if !chart.has_note_data || !chart.chart_type.eq_ignore_ascii_case(chart_type) {
            continue;
        }
        let Some(diff_ix) = STANDARD_DIFFICULTY_NAMES
            .iter()
            .position(|name| chart.difficulty.eq_ignore_ascii_case(name))
        else {
            continue;
        };
        let distance = diff_ix.abs_diff(preferred);
        if distance < best_distance {
            best = Some(chart_ix);
            best_distance = distance;
        }
    }
    best
}
