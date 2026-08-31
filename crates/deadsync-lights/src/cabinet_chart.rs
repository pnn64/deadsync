use std::path::PathBuf;

use deadsync_chart::{
    GameplayChartData, STANDARD_DIFFICULTY_COUNT, STANDARD_DIFFICULTY_NAMES, SongData,
};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::SongTimeNs;
use deadsync_rules::timing::TimingData;

use crate::CabinetLight;

const LIGHTS_CABINET_CHART_TYPE: &str = "lights-cabinet";
const LIGHTS_PRIMARY_CHART_TYPE: &str = "dance-single";
const LIGHTS_EXPLICIT_DIFFICULTY_INDEX: usize = 2; // Medium
const LIGHTS_MARQUEE_DIFFICULTY_INDEX: usize = 3; // Hard
const LIGHTS_BASS_DIFFICULTY_INDEX: usize = 2; // Medium
const LIGHTS_QUARTER_ROWS: usize = deadsync_core::timing::ROWS_PER_BEAT as usize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameplayLightChartKey {
    pub simfile_path: PathBuf,
    pub source_hashes: Vec<String>,
    pub global_offset_us: i32,
    pub pack_sync_offset_us: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct CabinetLightEvent {
    pub time_ns: SongTimeNs,
    pub row_index: usize,
    pub light: CabinetLight,
    pub simplify_bass_candidate: bool,
}

#[derive(Clone, Debug)]
pub enum CabinetLightPlan {
    Explicit {
        chart_ix: usize,
        chart_hash: String,
    },
    Generated {
        marquee_ix: usize,
        marquee_hash: String,
        bass_ix: usize,
        bass_hash: String,
    },
}

impl CabinetLightPlan {
    #[must_use]
    pub fn request_chart_ixs(&self) -> Vec<usize> {
        match self {
            Self::Explicit { chart_ix, .. } => vec![*chart_ix],
            Self::Generated {
                marquee_ix,
                bass_ix,
                ..
            } if marquee_ix == bass_ix => vec![*marquee_ix],
            Self::Generated {
                marquee_ix,
                bass_ix,
                ..
            } => vec![*marquee_ix, *bass_ix],
        }
    }

    fn source_hashes(&self) -> Vec<String> {
        match self {
            Self::Explicit { chart_hash, .. } => vec![chart_hash.clone()],
            Self::Generated {
                marquee_hash,
                bass_hash,
                ..
            } => vec![marquee_hash.clone(), bass_hash.clone()],
        }
    }
}

#[must_use]
pub fn cabinet_light_plan(song: &SongData, fallback_chart_ix: usize) -> Option<CabinetLightPlan> {
    if let Some(chart_ix) = closest_standard_chart_ix(
        song,
        LIGHTS_CABINET_CHART_TYPE,
        LIGHTS_EXPLICIT_DIFFICULTY_INDEX,
    ) {
        return Some(CabinetLightPlan::Explicit {
            chart_ix,
            chart_hash: song.charts[chart_ix].short_hash.clone(),
        });
    }

    let fallback = song
        .charts
        .get(fallback_chart_ix)
        .filter(|chart| chart.has_note_data)
        .map(|chart| (fallback_chart_ix, chart.short_hash.clone()))?;
    let (marquee_ix, marquee_hash) = closest_standard_chart_ix(
        song,
        LIGHTS_PRIMARY_CHART_TYPE,
        LIGHTS_MARQUEE_DIFFICULTY_INDEX,
    )
    .map(|ix| (ix, song.charts[ix].short_hash.clone()))
    .unwrap_or_else(|| fallback.clone());
    let (bass_ix, bass_hash) = closest_standard_chart_ix(
        song,
        LIGHTS_PRIMARY_CHART_TYPE,
        LIGHTS_BASS_DIFFICULTY_INDEX,
    )
    .map(|ix| (ix, song.charts[ix].short_hash.clone()))
    .unwrap_or_else(|| fallback);

    Some(CabinetLightPlan::Generated {
        marquee_ix,
        marquee_hash,
        bass_ix,
        bass_hash,
    })
}

fn closest_standard_chart_ix(
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

#[must_use]
pub fn cabinet_light_key(
    song: &SongData,
    plan: &CabinetLightPlan,
    global_offset_seconds: f32,
    pack_sync_offset_seconds: f32,
) -> GameplayLightChartKey {
    GameplayLightChartKey {
        simfile_path: song.simfile_path.clone(),
        source_hashes: plan.source_hashes(),
        global_offset_us: offset_key_us(global_offset_seconds),
        pack_sync_offset_us: offset_key_us(pack_sync_offset_seconds),
    }
}

#[must_use]
pub fn cabinet_light_chart_from_loaded(
    song: &SongData,
    plan: &CabinetLightPlan,
    charts: &[GameplayChartData],
    global_offset_seconds: f32,
    pack_sync_offset_seconds: f32,
) -> (GameplayLightChartKey, Vec<CabinetLightEvent>) {
    (
        cabinet_light_key(song, plan, global_offset_seconds, pack_sync_offset_seconds),
        build_cabinet_light_events(plan, charts, pack_sync_offset_seconds),
    )
}

fn offset_key_us(seconds: f32) -> i32 {
    if seconds.is_finite() {
        (seconds * 1_000_000.0).round() as i32
    } else {
        0
    }
}

#[must_use]
pub const fn cabinet_light_event_enabled(event: CabinetLightEvent, simplify_bass: bool) -> bool {
    !simplify_bass
        || !event.simplify_bass_candidate
        || event.row_index.is_multiple_of(LIGHTS_QUARTER_ROWS)
}

fn build_cabinet_light_events(
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

fn push_explicit_cabinet_events(
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

fn push_generated_marquee_events(
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

fn push_generated_bass_events(
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

fn light_note_time_ns(
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

#[cfg(any(test, feature = "bench-support"))]
fn build_cabinet_light_events_reference(
    plan: &CabinetLightPlan,
    charts: &[GameplayChartData],
    pack_sync_offset_seconds: f32,
) -> Vec<CabinetLightEvent> {
    fn timing(chart: &GameplayChartData, offset: f32) -> TimingData {
        let mut timing = chart.timing.clone();
        timing.shift_song_offset_seconds(offset);
        timing
    }

    fn note_time(
        timing: &TimingData,
        row_index: usize,
        skip_fake_rows: bool,
    ) -> Option<SongTimeNs> {
        let beat = timing.get_beat_for_row(row_index)?;
        if skip_fake_rows && (!timing.is_judgable_at_beat(beat) || timing.is_fake_at_beat(beat)) {
            return None;
        }
        Some(timing.get_time_for_beat_ns(beat))
    }

    let mut events = Vec::new();
    match plan {
        CabinetLightPlan::Explicit { .. } => {
            if let Some(chart) = charts.first() {
                let timing = timing(chart, pack_sync_offset_seconds);
                for note in &chart.parsed_notes {
                    if !explicit_light_note(note.note_type) {
                        continue;
                    }
                    let Some(light) = explicit_cabinet_light_for_col(note.column) else {
                        continue;
                    };
                    if let Some(time_ns) = note_time(&timing, note.row_index, false) {
                        events.push(CabinetLightEvent {
                            time_ns,
                            row_index: note.row_index,
                            light,
                            simplify_bass_candidate: false,
                        });
                    }
                }
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
            let marquee_timing = timing(marquee, pack_sync_offset_seconds);
            for note in &marquee.parsed_notes {
                if !generated_light_note(note.note_type) {
                    continue;
                }
                let Some(light) = cabinet_light_for_col(note.column % 4) else {
                    continue;
                };
                if let Some(time_ns) = note_time(&marquee_timing, note.row_index, true) {
                    events.push(CabinetLightEvent {
                        time_ns,
                        row_index: note.row_index,
                        light,
                        simplify_bass_candidate: false,
                    });
                }
            }
            let bass_timing = timing(bass, pack_sync_offset_seconds);
            let mut last_row = usize::MAX;
            for note in &bass.parsed_notes {
                if note.row_index == last_row || !generated_light_note(note.note_type) {
                    continue;
                }
                let Some(time_ns) = note_time(&bass_timing, note.row_index, true) else {
                    continue;
                };
                for light in [CabinetLight::BassLeft, CabinetLight::BassRight] {
                    events.push(CabinetLightEvent {
                        time_ns,
                        row_index: note.row_index,
                        light,
                        simplify_bass_candidate: marquee_ix == bass_ix,
                    });
                }
                last_row = note.row_index;
            }
        }
    }
    events.sort_by_key(|event| event.time_ns);
    events
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn build_cabinet_light_events_for_bench(
    plan: &CabinetLightPlan,
    charts: &[GameplayChartData],
    pack_sync_offset_seconds: f32,
) -> Vec<CabinetLightEvent> {
    build_cabinet_light_events(plan, charts, pack_sync_offset_seconds)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
#[must_use]
pub fn build_cabinet_light_events_reference_for_bench(
    plan: &CabinetLightPlan,
    charts: &[GameplayChartData],
    pack_sync_offset_seconds: f32,
) -> Vec<CabinetLightEvent> {
    build_cabinet_light_events_reference(plan, charts, pack_sync_offset_seconds)
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

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::notes::ParsedNote;
    use deadsync_chart::{
        ArrowStats, ChartData, GameplayChartData, SongData, StaminaCounts, TechCounts,
    };
    use deadsync_rules::timing::{TimingData, TimingSegments};

    fn parsed_note(row_index: usize, column: usize, note_type: NoteType) -> ParsedNote {
        ParsedNote {
            row_index,
            column,
            note_type,
            tail_row_index: None,
        }
    }

    fn test_gameplay_chart(parsed_notes: Vec<ParsedNote>) -> GameplayChartData {
        test_gameplay_chart_with_segments(parsed_notes, TimingSegments::default())
    }

    fn test_gameplay_chart_with_segments(
        parsed_notes: Vec<ParsedNote>,
        timing_segments: TimingSegments,
    ) -> GameplayChartData {
        let max_row = parsed_notes.iter().map(|n| n.row_index).max().unwrap_or(0);
        let row_to_beat = (0..=max_row.max(deadsync_core::timing::ROWS_PER_BEAT as usize * 2))
            .map(|row| row as f32 / deadsync_core::timing::ROWS_PER_BEAT as f32)
            .collect::<Vec<_>>();
        let timing = TimingData::from_segments(0.0, 0.0, &timing_segments, &row_to_beat);
        GameplayChartData {
            notes: Vec::new(),
            parsed_notes,
            row_to_beat,
            timing_segments,
            timing,
            chart_attacks: None,
        }
    }

    #[test]
    fn simplified_bass_light_events_only_use_quarter_rows() {
        let quarter = CabinetLightEvent {
            time_ns: 0,
            row_index: LIGHTS_QUARTER_ROWS * 2,
            light: CabinetLight::BassLeft,
            simplify_bass_candidate: true,
        };
        let eighth = CabinetLightEvent {
            time_ns: 0,
            row_index: LIGHTS_QUARTER_ROWS * 2 + LIGHTS_QUARTER_ROWS / 2,
            light: CabinetLight::BassLeft,
            simplify_bass_candidate: true,
        };
        let explicit = CabinetLightEvent {
            time_ns: 0,
            row_index: LIGHTS_QUARTER_ROWS * 2 + LIGHTS_QUARTER_ROWS / 2,
            light: CabinetLight::BassLeft,
            simplify_bass_candidate: false,
        };

        assert!(cabinet_light_event_enabled(quarter, true));
        assert!(!cabinet_light_event_enabled(eighth, true));
        assert!(cabinet_light_event_enabled(eighth, false));
        assert!(cabinet_light_event_enabled(explicit, true));
    }

    #[test]
    fn cabinet_light_plan_prefers_explicit_lights() {
        let mut song = test_song("Songs/Test/song.ssc", 0.0, ["hard", "medium"]);
        song.charts
            .push(test_chart_with("lights-cabinet", "Medium", "lights"));

        let plan = cabinet_light_plan(&song, 0).expect("light plan");

        match plan {
            CabinetLightPlan::Explicit {
                chart_ix,
                chart_hash,
            } => {
                assert_eq!(chart_ix, 2);
                assert_eq!(chart_hash, "lights");
            }
            CabinetLightPlan::Generated { .. } => panic!("expected explicit lights"),
        }
    }

    #[test]
    fn cabinet_light_plan_muxes_hard_marquee_and_medium_bass() {
        let mut song = test_song("Songs/Test/song.ssc", 0.0, ["hard", "medium"]);
        song.charts[0] = test_chart_with("dance-single", "Hard", "hard");
        song.charts[1] = test_chart_with("dance-single", "Medium", "medium");

        let plan = cabinet_light_plan(&song, 0).expect("light plan");

        match plan {
            CabinetLightPlan::Generated {
                marquee_ix,
                marquee_hash,
                bass_ix,
                bass_hash,
            } => {
                assert_eq!(marquee_ix, 0);
                assert_eq!(marquee_hash, "hard");
                assert_eq!(bass_ix, 1);
                assert_eq!(bass_hash, "medium");
            }
            CabinetLightPlan::Explicit { .. } => panic!("expected generated lights"),
        }
    }

    #[test]
    fn challenge_only_chart_drives_simplified_bass() {
        let mut song = test_song("Songs/Test/song.ssc", 0.0, ["challenge", "unused"]);
        song.charts.truncate(1);
        song.charts[0] = test_chart_with("dance-single", "Challenge", "challenge");

        let plan = cabinet_light_plan(&song, 0).expect("light plan");
        match &plan {
            CabinetLightPlan::Generated {
                marquee_ix,
                marquee_hash,
                bass_ix,
                bass_hash,
            } => {
                assert_eq!(*marquee_ix, 0);
                assert_eq!(marquee_hash, "challenge");
                assert_eq!(*bass_ix, 0);
                assert_eq!(bass_hash, "challenge");
            }
            CabinetLightPlan::Explicit { .. } => panic!("expected generated lights"),
        }
        assert_eq!(plan.request_chart_ixs(), [0]);

        let quarter_row = LIGHTS_QUARTER_ROWS;
        let eighth_row = quarter_row + LIGHTS_QUARTER_ROWS / 2;
        let chart = test_gameplay_chart(vec![
            parsed_note(quarter_row, 0, NoteType::Tap),
            parsed_note(eighth_row, 1, NoteType::Tap),
        ]);
        let events = build_cabinet_light_events(&plan, &[chart], 0.0);
        let bass_events = events
            .iter()
            .copied()
            .filter(|event| {
                matches!(
                    event.light,
                    CabinetLight::BassLeft | CabinetLight::BassRight
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(bass_events.len(), 4);
        assert!(bass_events.iter().any(|event| {
            event.row_index == quarter_row && cabinet_light_event_enabled(*event, true)
        }));
        assert!(bass_events.iter().any(|event| {
            event.row_index == eighth_row && !cabinet_light_event_enabled(*event, true)
        }));
    }

    #[test]
    fn generated_cabinet_events_take_bass_from_second_chart() {
        let plan = CabinetLightPlan::Generated {
            marquee_ix: 0,
            marquee_hash: "hard".to_string(),
            bass_ix: 1,
            bass_hash: "medium".to_string(),
        };
        let hard = test_gameplay_chart(vec![parsed_note(
            deadsync_core::timing::ROWS_PER_BEAT as usize,
            0,
            NoteType::Tap,
        )]);
        let medium = test_gameplay_chart(vec![parsed_note(
            (deadsync_core::timing::ROWS_PER_BEAT * 2) as usize,
            1,
            NoteType::Tap,
        )]);

        let events = build_cabinet_light_events(&plan, &[hard, medium], 0.0);

        assert!(events.iter().any(|event| {
            event.row_index == deadsync_core::timing::ROWS_PER_BEAT as usize
                && event.light == CabinetLight::MarqueeUpperLeft
        }));
        assert!(events.iter().any(|event| {
            event.row_index == (deadsync_core::timing::ROWS_PER_BEAT * 2) as usize
                && event.light == CabinetLight::BassLeft
        }));
        assert!(!events.iter().any(|event| {
            event.row_index == deadsync_core::timing::ROWS_PER_BEAT as usize
                && event.light == CabinetLight::BassLeft
        }));
    }

    #[test]
    fn optimized_cabinet_event_builder_matches_reference_across_timing_boundaries() {
        use deadsync_rules::timing::{DelaySegment, FakeSegment, StopSegment, WarpSegment};

        fn canonical(events: &[CabinetLightEvent]) -> Vec<(SongTimeNs, usize, u8, bool)> {
            let mut rows = events
                .iter()
                .map(|event| {
                    let light = match event.light {
                        CabinetLight::MarqueeUpperLeft => 0,
                        CabinetLight::MarqueeUpperRight => 1,
                        CabinetLight::MarqueeLowerLeft => 2,
                        CabinetLight::MarqueeLowerRight => 3,
                        CabinetLight::BassLeft => 4,
                        CabinetLight::BassRight => 5,
                    };
                    (
                        event.time_ns,
                        event.row_index,
                        light,
                        event.simplify_bass_candidate,
                    )
                })
                .collect::<Vec<_>>();
            rows.sort_unstable();
            rows
        }

        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 120.0), (8.0, 180.0), (20.0, 90.0)],
            stops: vec![StopSegment {
                beat: 6.0,
                duration: 0.125,
            }],
            delays: vec![DelaySegment {
                beat: 14.0,
                duration: 0.075,
            }],
            warps: vec![WarpSegment {
                beat: 10.0,
                length: 1.0,
            }],
            fakes: vec![FakeSegment {
                beat: 16.0,
                length: 1.0,
            }],
            ..TimingSegments::default()
        };
        let rows_per_beat = deadsync_core::timing::ROWS_PER_BEAT as usize;
        let notes = (0..1_024)
            .flat_map(|index| {
                let row = index * (rows_per_beat / 4);
                let note_type = match index % 11 {
                    0 => NoteType::Mine,
                    1 => NoteType::Lift,
                    2 => NoteType::Fake,
                    3 => NoteType::Hold,
                    4 => NoteType::Roll,
                    _ => NoteType::Tap,
                };
                [
                    parsed_note(row, index % 4, note_type),
                    parsed_note(row, (index + 1) % 4, NoteType::Tap),
                ]
            })
            .collect::<Vec<_>>();
        let chart = test_gameplay_chart_with_segments(notes, timing_segments);

        for offset in [-0.137_125, 0.0, 0.243_875] {
            for plan in [
                CabinetLightPlan::Explicit {
                    chart_ix: 0,
                    chart_hash: "lights".to_owned(),
                },
                CabinetLightPlan::Generated {
                    marquee_ix: 0,
                    marquee_hash: "hard".to_owned(),
                    bass_ix: 0,
                    bass_hash: "hard".to_owned(),
                },
            ] {
                let actual =
                    build_cabinet_light_events(&plan, std::slice::from_ref(&chart), offset);
                let expected = build_cabinet_light_events_reference(
                    &plan,
                    std::slice::from_ref(&chart),
                    offset,
                );
                let actual_canonical = canonical(&actual);
                let expected_canonical = canonical(&expected);
                assert_eq!(actual_canonical.len(), expected_canonical.len());
                if let Some((index, (actual, expected))) = actual_canonical
                    .iter()
                    .zip(&expected_canonical)
                    .enumerate()
                    .find(|(_, (actual, expected))| actual != expected)
                {
                    panic!(
                        "cabinet event mismatch at {index} for offset {offset}: {actual:?} != {expected:?}"
                    );
                }
                assert!(
                    actual
                        .windows(2)
                        .all(|pair| pair[0].time_ns <= pair[1].time_ns)
                );
            }
        }
    }

    fn test_song(path: &str, offset: f32, hashes: [&str; 2]) -> SongData {
        SongData {
            simfile_path: PathBuf::from(path),
            title: "Song".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            translit_artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset,
            sample_start: None,
            sample_length: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
            normalized_bpms: "120.000".to_string(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: vec![
                test_chart_with("dance-single", "Hard", hashes[0]),
                test_chart_with("dance-single", "Hard", hashes[1]),
            ],
        }
    }

    fn test_chart_with(chart_type: &str, difficulty: &str, hash: &str) -> ChartData {
        ChartData {
            chart_name: String::new(),
            chart_type: chart_type.to_string(),
            difficulty: difficulty.to_string(),
            description: String::new(),
            meter: 9,
            step_artist: String::new(),
            music_path: None,
            short_hash: hash.to_string(),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            matrix_profile: Box::default(),
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
        }
    }
}
