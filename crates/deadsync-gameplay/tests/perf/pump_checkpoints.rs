use super::*;
use deadsync_rules::timing::TickcountSegment;
use std::hint::black_box;

pub(super) mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/pump_checkpoints/baseline.rs"
    ));
}

struct PumpFixture {
    notes: Vec<Note>,
    ranges: [(usize, usize); MAX_PLAYERS],
    times: Vec<SongTimeNs>,
    ends: Vec<SongTimeNs>,
    timing: [Arc<TimingData>; MAX_PLAYERS],
    charts: [Arc<GameplayChartData>; MAX_PLAYERS],
    players: usize,
}

impl PumpFixture {
    fn new(holds: usize, length: f32, changes: usize, players: usize) -> Self {
        let segments = TimingSegments {
            bpms: (0..changes.max(1))
                .map(|i| (i as f32 * 4.0, if i % 2 == 0 { 120.0 } else { 175.0 }))
                .collect(),
            stops: (0..changes / 4)
                .map(|i| StopSegment {
                    beat: i as f32 * 16.0 + 1.0,
                    duration: 0.05,
                })
                .collect(),
            delays: (0..changes / 4)
                .map(|i| DelaySegment {
                    beat: i as f32 * 16.0 + 2.0,
                    duration: 0.025,
                })
                .collect(),
            warps: (0..changes / 4)
                .map(|i| WarpSegment {
                    beat: i as f32 * 16.0 + 3.0,
                    length: 0.5,
                })
                .collect(),
            ..TimingSegments::default()
        };
        let timing = Arc::new(TimingData::from_segments(0.125, -0.01, &segments, &[]));
        let chart = Arc::new(GameplayChartData {
            notes: Vec::new(),
            parsed_notes: Vec::new(),
            row_to_beat: Vec::new(),
            timing_segments: segments,
            timing: (*timing).clone(),
            chart_attacks: None,
        });
        let mut fixture = Self {
            notes: Vec::new(),
            ranges: [(0, 0); MAX_PLAYERS],
            times: Vec::new(),
            ends: Vec::new(),
            timing: std::array::from_fn(|_| timing.clone()),
            charts: std::array::from_fn(|_| chart.clone()),
            players,
        };
        for player in 0..players {
            let start = fixture.notes.len();
            for i in 0..holds {
                let beat = i as f32 * 4.0;
                let mut hold = test_hold();
                hold.end_beat = beat + length;
                hold.end_row_index = i * 2 + 1;
                let mut note = test_note_at(
                    if i % 2 == 0 {
                        NoteType::Hold
                    } else {
                        NoteType::Roll
                    },
                    Some(hold),
                    false,
                    i * 2,
                    beat,
                );
                note.column = player * 5 + i % 5;
                fixture.notes.push(note);
                let mut tap = test_note_at(NoteType::Tap, None, false, i * 2 + 1, beat + 0.5);
                tap.column = player * 5 + (i + 1) % 5;
                fixture.notes.push(tap);
            }
            fixture.ranges[player] = (start, fixture.notes.len());
        }
        fixture.times = fixture
            .notes
            .iter()
            .map(|n| timing.get_time_for_beat_ns(n.beat))
            .collect();
        fixture.ends = fixture
            .notes
            .iter()
            .map(|n| {
                n.hold.as_ref().map_or(INVALID_SONG_TIME_NS, |h| {
                    timing.get_time_for_beat_ns(h.end_beat)
                })
            })
            .collect();
        fixture
    }

    fn tap_heavy(count: usize) -> Self {
        let mut fixture = Self::new(0, 1.0, 1, 1);
        for index in 0..count {
            let beat = index as f32 * 0.25;
            let (kind, hold) = if index % 64 == 0 {
                let mut hold = test_hold();
                hold.end_beat = beat + 1.0;
                hold.end_row_index = index + 4;
                (NoteType::Hold, Some(hold))
            } else {
                (NoteType::Tap, None)
            };
            let mut note = test_note_at(kind, hold, false, index, beat);
            note.column = index % 5;
            fixture
                .times
                .push(fixture.timing[0].get_time_for_beat_ns(beat));
            fixture
                .ends
                .push(note.hold.as_ref().map_or(INVALID_SONG_TIME_NS, |hold| {
                    fixture.timing[0].get_time_for_beat_ns(hold.end_beat)
                }));
            fixture.notes.push(note);
        }
        fixture.ranges[0] = (0, count);
        fixture
    }

    fn build(&self, old: bool) -> (Vec<PumpHoldEvent>, [u32; MAX_PLAYERS]) {
        let build = std::hint::black_box(if old {
            baseline::build_pump_hold_events
        } else {
            crate::build_pump_hold_events
        });
        build(
            &self.notes,
            &self.ranges,
            &self.times,
            &self.ends,
            &self.timing,
            &self.charts,
            self.players,
        )
    }
    fn assert_same(&self) {
        assert_eq!(self.build(false), self.build(true));
    }
}
#[test]
fn pump_checkpoints_match_legacy_with_timing_changes_and_two_players() {
    for holds in [0, 1, 8, 64] {
        for length in [0.0, 0.01, 0.25, 2.0, 16.0] {
            for changes in [1, 8, 32] {
                for players in [1, 2] {
                    PumpFixture::new(holds, length, changes, players).assert_same();
                }
            }
        }
    }
}

#[test]
fn pump_ranges_match_legacy_tickcounts_and_quantized_boundaries() {
    let mut fixture = PumpFixture::new(12, 9.375, 16, 2);
    for ticks in [0, 1, 3, 4, 5, 7, 48, 255] {
        let tickcounts = vec![
            TickcountSegment { beat: -1.0, ticks },
            TickcountSegment {
                beat: 4.0,
                ticks: 0,
            },
            TickcountSegment {
                beat: 4.0,
                ticks: 48,
            },
            TickcountSegment {
                beat: 16.001,
                ticks: 7,
            },
            TickcountSegment { beat: 8.0, ticks },
        ];
        for chart in &mut fixture.charts {
            Arc::make_mut(chart).timing_segments.tickcounts = tickcounts.clone();
        }
        fixture.assert_same();
    }
}

#[test]
fn pump_cached_times_preserve_fractional_bpm_and_event_rows() {
    for (shift, duplicates) in [
        (-0.0101, true),
        (-0.001, true),
        (0.0, false),
        (0.0, true),
        (0.001, true),
        (0.0101, true),
    ] {
        let mut fixture = PumpFixture::new(12, 8.0, 8, 1);
        let mut segments = fixture.charts[0].timing_segments.clone();
        for (index, point) in segments.bpms.iter_mut().enumerate().skip(1) {
            point.0 += shift;
            if index % 3 == 0 && duplicates {
                point.0 = 4.0 + shift;
            }
        }
        segments.stops.push(StopSegment {
            beat: 4.001,
            duration: 0.05,
        });
        segments.delays.push(DelaySegment {
            beat: 4.002,
            duration: 0.03,
        });
        let timing = Arc::new(TimingData::from_segments(0.1, -0.01, &segments, &[]));
        for player in 0..MAX_PLAYERS {
            fixture.timing[player] = timing.clone();
            let chart = Arc::make_mut(&mut fixture.charts[player]);
            chart.timing = (*timing).clone();
            chart.timing_segments = segments.clone();
        }
        fixture.assert_same();
    }
}

#[test]
fn pump_reservation_preserves_invalid_notes_and_truncated_ranges() {
    let mut fixture = PumpFixture::new(12, 4.0, 8, 2);
    fixture.notes[0].is_fake = true;
    fixture.notes[2].can_be_judged = false;
    fixture.notes[4].hold = None;
    fixture.ends[6] = INVALID_SONG_TIME_NS;
    fixture.notes[8].hold.as_mut().unwrap().end_beat = -1.0;
    fixture.notes.swap(10, 14); // Reversed note rows keep tap sorting and cache rewinds.
    fixture.assert_same();
    fixture.ends.truncate(fixture.ends.len() - 7);
    fixture.assert_same();
    fixture.times.truncate(19);
    fixture.assert_same();
    fixture.ranges = [(4, 2), (5, usize::MAX)];
    fixture.assert_same();
    fixture.players = 0;
    fixture.assert_same();
}

// Ablation: the old emitter with only cache eligibility supplied by its caller.
#[allow(clippy::too_many_arguments)]
fn binary_checkpoints(
    events: &mut Vec<PumpHoldEvent>,
    notes: &[Note],
    tap_rows: &[usize],
    source: PumpHoldSource,
    timing: &TimingData,
    segments: &TimingSegments,
    cache_times: bool,
) {
    let note = &notes[source.note_index.get()];
    let mut time_cache = BeatTimeCache::new(timing);
    for (first_row, last_row, rows_per_tick) in pump_checkpoint_ranges(note, segments) {
        let mut row = first_row;
        while row <= last_row {
            let beat = note_row_to_beat(row as i32);
            events.push(PumpHoldEvent {
                time_ns: if cache_times {
                    timing.get_time_for_beat_ns_cached(beat, &mut time_cache)
                } else {
                    timing.get_time_for_beat_ns(beat)
                },
                row_index: ChartRowIndex::from_validated(row),
                note_index: source.note_index,
                player: source.player,
                column: source.column,
                kind: PumpHoldEventKind::Checkpoint,
                has_tap: tap_rows.binary_search(&row).is_ok(),
            });
            row = row.saturating_add(rows_per_tick);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn binary_checkpoints_compact(
    events: &mut Vec<PumpHoldEvent>,
    notes: &[Note],
    tap_rows: &[u32],
    source: PumpHoldSource,
    timing: &TimingData,
    segments: &TimingSegments,
    cache_times: bool,
) {
    let note = &notes[source.note_index.get()];
    let mut time_cache = BeatTimeCache::new(timing);
    for (first_row, last_row, rows_per_tick) in pump_checkpoint_ranges(note, segments) {
        let mut row = first_row;
        while row <= last_row {
            let beat = note_row_to_beat(row as i32);
            events.push(PumpHoldEvent {
                time_ns: if cache_times {
                    timing.get_time_for_beat_ns_cached(beat, &mut time_cache)
                } else {
                    timing.get_time_for_beat_ns(beat)
                },
                row_index: ChartRowIndex::from_validated(row),
                note_index: source.note_index,
                player: source.player,
                column: source.column,
                kind: PumpHoldEventKind::Checkpoint,
                has_tap: tap_rows.binary_search(&(row as u32)).is_ok(),
            });
            row = row.saturating_add(rows_per_tick);
        }
    }
}

#[test]
fn compact_tap_rows_match_full_i32_beat_domain_and_filters() {
    let mut notes = Vec::new();
    for beat in [
        0.0,
        -1.0,
        1.0 / 48.0,
        1234.5,
        f32::MAX,
        f32::MIN,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        for kind in [
            NoteType::Tap,
            NoteType::Hold,
            NoteType::Roll,
            NoteType::Mine,
            NoteType::Lift,
            NoteType::Fake,
        ] {
            for fake in [false, true] {
                for judged in [false, true] {
                    let mut note = test_note_at(kind, None, fake, 0, beat);
                    note.can_be_judged = judged;
                    notes.push(note);
                }
            }
        }
    }
    for range in [(0, notes.len()), (9, usize::MAX), (27, 14), (usize::MAX, 0)] {
        let old = baseline::pump_tap_rows(&notes, range);
        let new = crate::pump_tap_rows(&notes, range);
        assert!(new.iter().copied().map(|v| v as usize).eq(old));
    }
    let fixture = PumpFixture::new(1024, 1.0, 1, 2);
    crate::perf::assert_churn_budget(1, fixture.notes.len() * 4, || {
        black_box(crate::pump_tap_rows(
            &fixture.notes,
            (0, fixture.notes.len()),
        ));
    });
}

#[test]
fn checkpoint_emission_is_allocation_free_and_matches_binary_queries() {
    let fixture = PumpFixture::new(256, 32.0, 128, 1);
    let rows = crate::pump_tap_rows(&fixture.notes, fixture.ranges[0]);
    let cache_times = fixture.timing[0].supports_row_time_cache();
    let mut events = Vec::with_capacity(1024);
    let source = PumpHoldSource {
        note_index: ChartNoteIndex::from_validated(128),
        player: 0,
        column: 4,
    };
    crate::perf::assert_no_churn(|| {
        push_pump_checkpoints_cached(
            &mut events,
            &fixture.notes,
            &rows,
            source,
            &fixture.timing[0],
            &fixture.charts[0].timing_segments,
            cache_times,
        );
    });
    let mut old = Vec::new();
    baseline::push_pump_checkpoints(
        &mut old,
        &fixture.notes,
        &baseline::pump_tap_rows(&fixture.notes, fixture.ranges[0]),
        source,
        &fixture.timing[0],
        &fixture.charts[0].timing_segments,
    );
    assert_eq!(events, old);
}

#[test]
fn tap_heavy_event_builder_matches_baseline_without_growth() {
    let fixture = PumpFixture::tap_heavy(8192);
    fixture.assert_same();
    let events = fixture.build(false).0.len();
    crate::perf::assert_churn_budget(
        2,
        events * std::mem::size_of::<PumpHoldEvent>() + fixture.notes.len() * 4,
        || {
            black_box(fixture.build(false));
        },
    );
}

#[test]
fn pump_preparation_differential_varied_inputs() {
    let mut random = 7u64;
    for case in 0..64 {
        let mut fixture = PumpFixture::new(24, 4.0, 16, 2);
        for note in &mut fixture.notes {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            note.beat += (random % 96) as f32 / 48.0 - 1.0;
            note.is_fake = random % 11 == 0;
            note.can_be_judged = random % 13 != 0;
            if let Some(hold) = &mut note.hold {
                hold.end_beat = note.beat + (random % 192) as f32 / 48.0;
            }
        }
        if case % 2 == 0 {
            fixture.notes.reverse();
        }
        if case % 3 == 0 {
            fixture.times.truncate(39);
        }
        if case % 5 == 0 {
            fixture.ends.truncate(17);
        }
        fixture.assert_same();
    }
}

#[test]
#[ignore = "manual 0.5.1150/0.5.1151 Pump preparation comparison; run in release"]
fn pump_checkpoint_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for (label, holds, length, changes, players, iterations) in [
        ("empty", 0, 1.0, 1, 1, 10000),
        ("small", 4, 1.0, 1, 1, 1000),
        ("plain", 128, 16.0, 1, 1, 64),
        ("dense", 128, 16.0, 128, 1, 32),
        ("versus", 128, 16.0, 128, 2, 16),
        ("many_changes", 512, 1.0, 1024, 1, 16),
        ("long", 32, 256.0, 1, 1, 16),
        ("tap_heavy", 8192, 1.0, 1, 1, 128),
    ] {
        let fixture = if label == "tap_heavy" {
            PumpFixture::tap_heavy(holds)
        } else {
            PumpFixture::new(holds, length, changes, players)
        };
        fixture.assert_same();
        let units = fixture.build(true).0.len().max(1);
        for old in order {
            let suffix = if old { "old" } else { "new" };
            crate::perf::measure_sampled(
                &format!("full_{label}_{suffix}"),
                iterations,
                units,
                || black_box(&fixture).build(old),
            );
        }
    }
    for (label, holds, length, changes) in [
        ("plain", 128, 16.0, 1),
        ("many_changes", 512, 1.0, 1024),
        ("long", 32, 256.0, 1),
        ("tap_heavy", 8192, 1.0, 1),
    ] {
        let fixture = if label == "tap_heavy" {
            PumpFixture::tap_heavy(holds)
        } else {
            PumpFixture::new(holds, length, changes, 1)
        };
        let holds = fixture
            .notes
            .iter()
            .filter(|note| note.hold.is_some())
            .count();
        let rows = baseline::pump_tap_rows(&fixture.notes, fixture.ranges[0]);
        let small_rows = crate::pump_tap_rows(&fixture.notes, fixture.ranges[0]);
        let cache_times = fixture.timing[0].supports_row_time_cache();
        let capacity = fixture.build(true).0.len();
        let mut events = Vec::with_capacity(capacity);
        for operation in ["eligibility", "lookup"] {
            for old in order {
                let suffix = if old { "old" } else { "new" };
                crate::perf::measure_sampled(
                    &format!("{operation}_{label}_{suffix}"),
                    32,
                    capacity - holds * 2,
                    || {
                        events.clear();
                        for (index, note) in fixture.notes.iter().enumerate() {
                            if note.hold.is_none() {
                                continue;
                            }
                            let source = PumpHoldSource {
                                note_index: ChartNoteIndex::from_validated(index),
                                player: 0,
                                column: fixture.notes[index].column as u8,
                            };
                            if operation == "eligibility" && old {
                                black_box(
                                    baseline::push_pump_checkpoints
                                        as fn(
                                            &mut Vec<PumpHoldEvent>,
                                            &[Note],
                                            &[usize],
                                            PumpHoldSource,
                                            &TimingData,
                                            &TimingSegments,
                                        ),
                                )(
                                    &mut events,
                                    black_box(&fixture.notes),
                                    black_box(&rows),
                                    source,
                                    &fixture.timing[0],
                                    &fixture.charts[0].timing_segments,
                                );
                            } else if operation == "eligibility" {
                                black_box(
                                    binary_checkpoints
                                        as fn(
                                            &mut Vec<PumpHoldEvent>,
                                            &[Note],
                                            &[usize],
                                            PumpHoldSource,
                                            &TimingData,
                                            &TimingSegments,
                                            bool,
                                        ),
                                )(
                                    &mut events,
                                    black_box(&fixture.notes),
                                    black_box(&rows),
                                    source,
                                    &fixture.timing[0],
                                    &fixture.charts[0].timing_segments,
                                    black_box(cache_times),
                                );
                            } else if old {
                                black_box(
                                    binary_checkpoints_compact
                                        as fn(
                                            &mut Vec<PumpHoldEvent>,
                                            &[Note],
                                            &[u32],
                                            PumpHoldSource,
                                            &TimingData,
                                            &TimingSegments,
                                            bool,
                                        ),
                                )(
                                    &mut events,
                                    black_box(&fixture.notes),
                                    black_box(&small_rows),
                                    source,
                                    &fixture.timing[0],
                                    &fixture.charts[0].timing_segments,
                                    black_box(cache_times),
                                );
                            } else {
                                black_box(
                                    push_pump_checkpoints_cached
                                        as fn(
                                            &mut Vec<PumpHoldEvent>,
                                            &[Note],
                                            &[u32],
                                            PumpHoldSource,
                                            &TimingData,
                                            &TimingSegments,
                                            bool,
                                        ),
                                )(
                                    &mut events,
                                    black_box(&fixture.notes),
                                    black_box(&small_rows),
                                    source,
                                    &fixture.timing[0],
                                    &fixture.charts[0].timing_segments,
                                    black_box(cache_times),
                                );
                            }
                        }
                        black_box(&events);
                    },
                );
            }
        }
        for old in order {
            let suffix = if old { "old" } else { "new" };
            crate::perf::measure_sampled(
                &format!("scratch_{label}_{suffix}"),
                1024,
                fixture.notes.len(),
                || {
                    if old {
                        black_box(black_box(
                            baseline::pump_tap_rows as fn(&[Note], (usize, usize)) -> Vec<usize>,
                        )(&fixture.notes, fixture.ranges[0]));
                    } else {
                        black_box(black_box(
                            crate::pump_tap_rows as fn(&[Note], (usize, usize)) -> Vec<u32>,
                        )(&fixture.notes, fixture.ranges[0]));
                    }
                },
            );
        }
    }
}
