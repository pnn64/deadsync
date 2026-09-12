use super::*;
use crate::perf::{assert_churn_budget, assert_no_churn, measure_sampled};
use deadsync_rules::timing::{DelaySegment, FakeSegment, StopSegment, WarpSegment};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/cabinet_lights/baseline.rs"
    ));
}

fn fixture(rows: usize, chord: usize, kind: usize) -> GameplayChartData {
    let notes = (0..rows)
        .flat_map(|row| (0..chord).map(move |column| parsed_note(row * 12, column, NoteType::Tap)))
        .collect();
    let mut segments = TimingSegments {
        bpms: vec![(0.0, 150.0)],
        ..TimingSegments::default()
    };
    if kind > 0 {
        for index in 1..rows.div_ceil(32) {
            let beat = index as f32 * 8.0;
            segments.bpms.push((beat, 90.0 + (index % 9) as f32 * 20.0));
            segments.stops.push(StopSegment {
                beat: beat + 1.0,
                duration: 0.125,
            });
            segments.delays.push(DelaySegment {
                beat: beat + 2.0,
                duration: 0.0625,
            });
            segments.warps.push(WarpSegment {
                beat: beat + 3.0,
                length: 1.0,
            });
            segments.fakes.push(FakeSegment {
                beat: beat + 5.0,
                length: 0.5,
            });
        }
    }
    if kind == 2 {
        segments.bpms.extend([(0.001, 130.0), (0.001, 170.0)]);
    }
    let mut chart = test_gameplay_chart_with_segments(notes, segments);
    chart.timing =
        TimingData::from_segments(-0.125, 0.037, &chart.timing_segments, &chart.row_to_beat);
    chart
}

fn plan(kind: usize) -> CabinetLightPlan {
    if kind == 0 {
        CabinetLightPlan::Explicit {
            chart_ix: 0,
            chart_hash: "explicit".into(),
        }
    } else {
        CabinetLightPlan::Generated {
            marquee_ix: 0,
            marquee_hash: "marquee".into(),
            bass_ix: usize::from(kind == 2),
            bass_hash: "bass".into(),
        }
    }
}

fn canonical(events: &[CabinetLightEvent]) -> Vec<(i64, usize, u8, bool)> {
    let mut keys: Vec<_> = events
        .iter()
        .map(|event| {
            (
                event.time_ns,
                event.row_index,
                event.light as u8,
                event.simplify_bass_candidate,
            )
        })
        .collect();
    keys.sort_unstable();
    keys
}

fn assert_events(plan: &CabinetLightPlan, charts: &[GameplayChartData], offset: f32) {
    let old = baseline::build_cabinet_light_events(plan, charts, offset);
    let new = build_cabinet_light_events(plan, charts, offset);
    assert!(new.is_sorted_by_key(|event| event.time_ns));
    // Equal-time order was already unstable. Preserve every event and its
    // multiplicity, plus the exact visible light mask at each event time.
    assert_eq!(canonical(&old), canonical(&new));
    for simplify in [false, true] {
        let masks = |events: &[CabinetLightEvent]| {
            let mut masks = std::collections::BTreeMap::new();
            for event in events {
                if cabinet_light_event_enabled(*event, simplify) {
                    *masks.entry(event.time_ns).or_insert(0u8) |= 1 << event.light as u8;
                }
            }
            masks
        };
        assert_eq!(masks(&old), masks(&new));
    }
}

#[test]
fn cabinet_events_match_old_for_plans_chords_timing_offsets_and_missing_charts() {
    for rows in [0, 1, 4, 64, 257] {
        for chord in [1, 4, 10] {
            for kind in 0..3 {
                let charts = [fixture(rows, chord, kind), fixture(rows / 2, 2, kind)];
                for plan_kind in 0..3 {
                    for count in 0..=2 {
                        for offset in [
                            0.0,
                            -0.25,
                            0.125,
                            f32::NAN,
                            f32::INFINITY,
                            f32::NEG_INFINITY,
                        ] {
                            assert_events(&plan(plan_kind), &charts[..count], offset);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn cabinet_events_match_old_for_mixed_invalid_and_repeated_unordered_rows() {
    let mut seed = 41u64;
    for kind in 0..3 {
        let mut chart = fixture(257, 10, kind);
        for (index, note) in chart.parsed_notes.iter_mut().enumerate() {
            note.note_type = [
                NoteType::Tap,
                NoteType::Hold,
                NoteType::Roll,
                NoteType::Mine,
                NoteType::Lift,
                NoteType::Fake,
            ][index % 6];
        }
        chart.parsed_notes.extend([
            parsed_note(usize::MAX, 0, NoteType::Tap),
            parsed_note(100_000, 3, NoteType::Hold),
            parsed_note(0, usize::MAX, NoteType::Roll),
            parsed_note(usize::MAX, 0, NoteType::Tap),
        ]);
        for order in 0..3 {
            if order == 1 {
                chart.parsed_notes.reverse();
            }
            if order == 2 {
                for index in (1..chart.parsed_notes.len()).rev() {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    chart
                        .parsed_notes
                        .swap(index, (seed >> 32) as usize % (index + 1));
                }
            }
            for kind in 0..3 {
                assert_events(&plan(kind), std::slice::from_ref(&chart), 0.25);
            }
        }
    }
}

#[test]
fn row_times_match_independent_queries_at_special_beats_and_bpm_boundaries() {
    for kind in 0..3 {
        let mut chart = fixture(257, 4, kind);
        for unusual in [false, true] {
            if unusual {
                chart.row_to_beat[0..8].copy_from_slice(&[
                    f32::NAN,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::MAX,
                    -3.0,
                    0.001,
                    0.002,
                    0.0001,
                ]);
                chart.timing = TimingData::from_segments(
                    -0.125,
                    0.037,
                    &chart.timing_segments,
                    &chart.row_to_beat,
                );
            }
            for skip_fake in [false, true] {
                for offset in [0, i64::MAX, i64::MIN + 1] {
                    let mut times = LightRowTimes::new(&chart, skip_fake, offset);
                    for row in (0..chart.row_to_beat.len()).chain((0..40).rev()).chain([
                        usize::MAX,
                        usize::MAX,
                        0,
                    ]) {
                        let expected =
                            baseline::light_note_time_ns(&chart.timing, row, skip_fake, offset);
                        assert_eq!(
                            times.get(row),
                            expected,
                            "kind={kind} unusual={unusual} row={row}"
                        );
                        assert_eq!(times.get(row), expected);
                    }
                }
            }
        }
    }
}

#[test]
fn light_timing_queries_allocate_nothing() {
    for kind in 0..3 {
        let chart = fixture(1024, 4, kind);
        assert_no_churn(|| {
            let mut times = LightRowTimes::new(&chart, true, 0);
            for note in &chart.parsed_notes {
                black_box(times.get(black_box(note.row_index)));
            }
        });
    }
}

#[test]
fn cabinet_event_storage_is_bounded_by_eligible_notes_and_bass_rows() {
    for chord in [1, 4, 10] {
        let chart = fixture(1024, chord, 0);
        let plan = plan(1);
        let capacity = 1024 * (chord + 2);
        assert_churn_budget(1, capacity * size_of::<CabinetLightEvent>(), || {
            let events = build_cabinet_light_events(&plan, std::slice::from_ref(&chart), 0.0);
            assert_eq!(events.len(), capacity);
            assert_eq!(events.capacity(), capacity);
            black_box(events);
        });
    }
    let mut chart = fixture(1024, 4, 0);
    for note in &mut chart.parsed_notes {
        note.note_type = NoteType::Fake;
    }
    for kind in 0..3 {
        let plan = plan(kind);
        assert_no_churn(|| {
            assert!(
                build_cabinet_light_events(&plan, std::slice::from_ref(&chart), 0.0).is_empty()
            );
        });
    }
}

fn pair<T>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> T,
    mut new: impl FnMut() -> T,
) {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    let old_name = format!("{name}_old");
    let new_name = format!("{name}_new");
    if reverse {
        measure_sampled(&new_name, iterations, units, &mut new);
        measure_sampled(&old_name, iterations, units, &mut old);
    } else {
        measure_sampled(&old_name, iterations, units, &mut old);
        measure_sampled(&new_name, iterations, units, &mut new);
    }
}

// Isolate fusion/order handling after both implementations already use the
// new row-time cache, with identical conservative storage reservations.
fn emission(chart: &GameplayChartData, fused: bool) -> Vec<CabinetLightEvent> {
    let mut events = Vec::with_capacity(chart.parsed_notes.len() * 3);
    if fused {
        push_shared_generated_events(&mut events, chart, 0, true);
        if !events.is_sorted_by_key(|event| event.time_ns) {
            events.sort_unstable_by_key(|event| event.time_ns);
        }
    } else {
        push_generated_marquee_events(&mut events, chart, 0);
        push_generated_bass_events(&mut events, chart, 0, true);
        events.sort_unstable_by_key(|event| event.time_ns);
    }
    events
}

// Isolate reservation sizing with the same fused emitter on each side.
fn storage(chart: &GameplayChartData, tight: bool) -> Vec<CabinetLightEvent> {
    let capacity = if tight {
        generated_event_capacity(chart, true, true)
    } else {
        chart.parsed_notes.len() * 3
    };
    if capacity == 0 {
        return Vec::new();
    }
    let mut events = Vec::with_capacity(capacity);
    push_shared_generated_events(&mut events, chart, 0, true);
    events
}

#[test]
fn fused_emission_matches_separate_cached_passes() {
    for kind in 0..3 {
        let mut chart = fixture(257, 4, kind);
        for reverse in [false, true] {
            if reverse {
                chart.parsed_notes.reverse();
            }
            assert_eq!(
                canonical(&emission(&chart, false)),
                canonical(&emission(&chart, true))
            );
        }
    }
}

#[test]
#[ignore = "manual release old/new CPU, throughput and allocation benchmark"]
fn cabinet_lights_bench() {
    for (name, chord, ignored) in [
        ("storage_single", 1, false),
        ("storage_chords", 4, false),
        ("storage_wide", 10, false),
        ("storage_ignored", 4, true),
    ] {
        let mut chart = fixture(1024, chord, 0);
        if ignored {
            for note in &mut chart.parsed_notes {
                note.note_type = NoteType::Fake;
            }
        }
        assert_eq!(
            canonical(&storage(&chart, false)),
            canonical(&storage(&chart, true))
        );
        pair(
            name,
            200,
            chart.parsed_notes.len(),
            || storage(black_box(&chart), false),
            || storage(black_box(&chart), true),
        );
    }
    for (name, rows, chord, kind, iterations) in [
        ("timing_tiny", 1, 1, 0, 50_000),
        ("timing_simple", 1024, 4, 0, 200),
        ("timing_dense", 4096, 4, 1, 8),
        ("timing_ambiguous", 1024, 4, 2, 8),
    ] {
        let chart = fixture(rows, chord, kind);
        pair(
            name,
            iterations,
            chart.parsed_notes.len(),
            || {
                chart.parsed_notes.iter().fold(0i64, |sum, note| {
                    sum.wrapping_add(
                        baseline::light_note_time_ns(
                            black_box(&chart.timing),
                            note.row_index,
                            true,
                            0,
                        )
                        .unwrap_or(0),
                    )
                })
            },
            || {
                let mut times = LightRowTimes::new(black_box(&chart), true, 0);
                chart.parsed_notes.iter().fold(0i64, |sum, note| {
                    sum.wrapping_add(times.get(note.row_index).unwrap_or(0))
                })
            },
        );
    }
    for (name, rows, chord, kind, reverse, iterations) in [
        ("fusion_tiny", 1, 1, 0, false, 50_000),
        ("fusion_single", 1024, 1, 0, false, 100),
        ("fusion_chords", 1024, 4, 0, false, 100),
        ("fusion_dense", 4096, 4, 1, false, 20),
        ("fusion_reverse", 1024, 4, 1, true, 10),
    ] {
        let mut chart = fixture(rows, chord, kind);
        if reverse {
            chart.parsed_notes.reverse();
        }
        pair(
            name,
            iterations,
            chart.parsed_notes.len(),
            || emission(black_box(&chart), false),
            || emission(black_box(&chart), true),
        );
    }
    for (name, rows, chord, kind, plan_kind, iterations) in [
        ("events_empty", 0, 1, 0, 1, 50_000),
        ("events_tiny", 1, 1, 0, 1, 50_000),
        ("events_single", 1024, 1, 0, 1, 100),
        ("events_chords", 1024, 4, 0, 1, 100),
        ("events_wide", 1024, 10, 0, 1, 80),
        ("events_dense", 4096, 4, 1, 1, 8),
        ("events_separate", 2048, 4, 1, 2, 8),
        ("events_explicit", 2048, 6, 1, 0, 8),
        ("events_ambiguous", 1024, 4, 2, 1, 8),
        ("events_reverse", 1024, 4, 1, 1, 8),
        ("events_ignored", 1024, 4, 0, 1, 2000),
    ] {
        let mut charts = [fixture(rows, chord, kind), fixture(rows / 2, 2, kind)];
        if name == "events_ignored" {
            for note in &mut charts[0].parsed_notes {
                note.note_type = NoteType::Fake;
            }
        }
        if name == "events_reverse" {
            charts[0].parsed_notes.reverse();
        }
        let plan = plan(plan_kind);
        assert_events(&plan, &charts, 0.125);
        pair(
            name,
            iterations,
            charts[0].parsed_notes.len().max(1),
            || baseline::build_cabinet_light_events(black_box(&plan), black_box(&charts), 0.125),
            || build_cabinet_light_events(black_box(&plan), black_box(&charts), 0.125),
        );
    }
}
