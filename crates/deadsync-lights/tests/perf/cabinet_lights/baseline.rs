// Frozen from 8bf5d81d5; only visibility and parent imports adapted.
use super::*;

pub(super) fn build_cabinet_light_events(
    plan: &CabinetLightPlan,
    charts: &[GameplayChartData],
    pack_sync_offset_seconds: f32,
) -> Vec<CabinetLightEvent> {
    // The result is song-lifetime storage. Reserve its conservative maximum
    // once so construction cannot grow repeatedly; generated bass emits at
    // most two events per source note. The final unstable sort is sufficient
    // because equal-time light blinks commute in the runtime bitmask.
    let event_capacity = match plan {
        CabinetLightPlan::Explicit { .. } => {
            charts.first().map_or(0, |chart| chart.parsed_notes.len())
        }
        CabinetLightPlan::Generated {
            marquee_ix,
            bass_ix,
            ..
        } => charts.first().map_or(0, |marquee| {
            let bass = if marquee_ix == bass_ix {
                marquee
            } else {
                charts.get(1).unwrap_or(marquee)
            };
            marquee
                .parsed_notes
                .len()
                .saturating_add(bass.parsed_notes.len().saturating_mul(2))
        }),
    };
    let mut events = Vec::with_capacity(event_capacity);
    let pack_sync_offset_ns = timing_offset_ns(pack_sync_offset_seconds);
    match plan {
        CabinetLightPlan::Explicit { .. } => {
            if let Some(chart) = charts.first() {
                push_explicit_cabinet_events(&mut events, chart, pack_sync_offset_ns);
            }
        }
        CabinetLightPlan::Generated {
            marquee_ix,
            bass_ix,
            ..
        } => {
            let Some(marquee) = charts.first() else {
                return events;
            };
            let bass = if marquee_ix == bass_ix {
                marquee
            } else {
                charts.get(1).unwrap_or(marquee)
            };
            push_generated_marquee_events(&mut events, marquee, pack_sync_offset_ns);
            push_generated_bass_events(
                &mut events,
                bass,
                pack_sync_offset_ns,
                marquee_ix == bass_ix,
            );
        }
    }
    events.sort_unstable_by_key(|event| event.time_ns);
    events
}

pub(super) fn push_explicit_cabinet_events(
    events: &mut Vec<CabinetLightEvent>,
    chart: &GameplayChartData,
    pack_sync_offset_ns: SongTimeNs,
) {
    let timing = &chart.timing;
    for note in &chart.parsed_notes {
        if !explicit_light_note(note.note_type) {
            continue;
        }
        let Some(light) = explicit_cabinet_light_for_col(note.column) else {
            continue;
        };
        if let Some(time_ns) =
            light_note_time_ns(timing, note.row_index, false, pack_sync_offset_ns)
        {
            events.push(CabinetLightEvent {
                time_ns,
                row_index: note.row_index,
                light,
                simplify_bass_candidate: false,
            });
        }
    }
}

pub(super) fn push_generated_marquee_events(
    events: &mut Vec<CabinetLightEvent>,
    chart: &GameplayChartData,
    pack_sync_offset_ns: SongTimeNs,
) {
    let timing = &chart.timing;
    for note in &chart.parsed_notes {
        if !generated_light_note(note.note_type) {
            continue;
        }
        let Some(light) = cabinet_light_for_col(note.column % 4) else {
            continue;
        };
        if let Some(time_ns) = light_note_time_ns(timing, note.row_index, true, pack_sync_offset_ns)
        {
            events.push(CabinetLightEvent {
                time_ns,
                row_index: note.row_index,
                light,
                simplify_bass_candidate: false,
            });
        }
    }
}

pub(super) fn push_generated_bass_events(
    events: &mut Vec<CabinetLightEvent>,
    chart: &GameplayChartData,
    pack_sync_offset_ns: SongTimeNs,
    simplify_candidate: bool,
) {
    let timing = &chart.timing;
    let mut last_row = usize::MAX;
    for note in &chart.parsed_notes {
        if note.row_index == last_row || !generated_light_note(note.note_type) {
            continue;
        }
        let Some(time_ns) = light_note_time_ns(timing, note.row_index, true, pack_sync_offset_ns)
        else {
            continue;
        };
        for light in [CabinetLight::BassLeft, CabinetLight::BassRight] {
            events.push(CabinetLightEvent {
                time_ns,
                row_index: note.row_index,
                light,
                simplify_bass_candidate: simplify_candidate,
            });
        }
        last_row = note.row_index;
    }
}

#[inline(always)]
fn timing_offset_ns(seconds: f32) -> SongTimeNs {
    let nanos = f64::from(seconds) * 1_000_000_000.0;
    nanos.clamp((i64::MIN + 1) as f64, i64::MAX as f64) as SongTimeNs
}

pub(super) fn light_note_time_ns(
    timing: &TimingData,
    row_index: usize,
    skip_fake_rows: bool,
    pack_sync_offset_ns: SongTimeNs,
) -> Option<SongTimeNs> {
    let beat = timing.get_beat_for_row(row_index)?;
    // `is_judgable_at_beat` already rejects fake rows. The old extra fake
    // query repeated the same partition-point search for every valid event.
    if skip_fake_rows && !timing.is_judgable_at_beat(beat) {
        return None;
    }
    Some(
        timing
            .get_time_for_beat_ns(beat)
            .saturating_sub(pack_sync_offset_ns),
    )
}

const fn generated_light_note(note_type: NoteType) -> bool {
    matches!(note_type, NoteType::Tap | NoteType::Hold | NoteType::Roll)
}

const fn explicit_light_note(note_type: NoteType) -> bool {
    !matches!(note_type, NoteType::Fake)
}

const fn explicit_cabinet_light_for_col(column: usize) -> Option<CabinetLight> {
    match column {
        0 => Some(CabinetLight::MarqueeUpperLeft),
        1 => Some(CabinetLight::MarqueeUpperRight),
        2 => Some(CabinetLight::MarqueeLowerLeft),
        3 => Some(CabinetLight::MarqueeLowerRight),
        4 => Some(CabinetLight::BassLeft),
        5 => Some(CabinetLight::BassRight),
        _ => None,
    }
}

const fn cabinet_light_for_col(local_col: usize) -> Option<CabinetLight> {
    match local_col {
        0 => Some(CabinetLight::MarqueeUpperLeft),
        1 => Some(CabinetLight::MarqueeUpperRight),
        2 => Some(CabinetLight::MarqueeLowerLeft),
        3 => Some(CabinetLight::MarqueeLowerRight),
        _ => None,
    }
}
