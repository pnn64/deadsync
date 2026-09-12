// Keep the 0.5.1136 cached comparison fixed at its original implementation.
use super::pump_checkpoint_perf::baseline::{pump_tap_rows, push_pump_checkpoints};
// Reference routines frozen from c4aeed4fe / 0.5.1135. The builder accepts
// a checkpoint writer to isolate time conversion from buffer reservation.
use super::*;
use deadsync_rules::timing::{DelaySegment, StopSegment, TickcountSegment, WarpSegment};
use std::hint::black_box;

type CheckpointWriter =
    fn(&mut Vec<PumpHoldEvent>, &[Note], &[usize], PumpHoldSource, &TimingData, &TimingSegments);

fn legacy_pump_tap_rows_and_hold_count(
    notes: &[Note],
    note_range: (usize, usize),
) -> (Vec<usize>, usize) {
    let end = note_range.1.min(notes.len());
    let start = note_range.0.min(end);
    let mut rows = Vec::with_capacity(end - start);
    let mut ordered = true;
    let mut hold_count = 0usize;
    for note in &notes[start..end] {
        if !note.can_be_judged
            || note.is_fake
            || !matches!(
                note.note_type,
                NoteType::Tap | NoteType::Hold | NoteType::Roll
            )
        {
            continue;
        }
        hold_count += usize::from(matches!(note.note_type, NoteType::Hold | NoteType::Roll));
        let row = beat_to_note_row(note.beat).max(0) as usize;
        match rows.last().copied() {
            Some(last) if row == last => {}
            Some(last) => {
                ordered &= row > last;
                rows.push(row);
            }
            None => rows.push(row),
        }
    }
    if !ordered {
        rows.sort_unstable();
        rows.dedup();
    }
    (rows, hold_count)
}

fn legacy_push_pump_checkpoints(
    events: &mut Vec<PumpHoldEvent>,
    notes: &[Note],
    tap_rows: &[usize],
    source: PumpHoldSource,
    timing: &TimingData,
    segments: &TimingSegments,
) {
    let note = &notes[source.note_index.get()];
    let Some(hold) = note.hold.as_ref() else {
        return;
    };
    for (segment_ix, segment) in segments.tickcounts.iter().enumerate() {
        let ticks = usize::from(segment.ticks.min(48));
        if ticks == 0 {
            continue;
        }
        let segment_row = beat_to_note_row(segment.beat).max(0) as usize;
        let next_segment_row = segments
            .tickcounts
            .get(segment_ix + 1)
            .map_or(usize::MAX, |next| {
                beat_to_note_row(next.beat).max(0) as usize
            });
        // Note::row_index addresses compact chart rows; checkpoints use ITG's
        // fixed 48-row beat grid instead.
        let note_row = beat_to_note_row(note.beat).max(0) as usize;
        let end_row = beat_to_note_row(hold.end_beat).max(0) as usize;
        let first_body_row = note_row.saturating_add(1).max(segment_row);
        let last_row = end_row.min(next_segment_row.saturating_sub(1));
        if first_body_row > last_row {
            continue;
        }
        let rows_per_tick = (ROWS_PER_BEAT as usize / ticks).max(1);
        let remainder = first_body_row % rows_per_tick;
        let mut row = first_body_row.saturating_add((rows_per_tick - remainder) % rows_per_tick);
        while row <= last_row {
            let beat = note_row_to_beat(row as i32);
            events.push(PumpHoldEvent {
                time_ns: timing.get_time_for_beat_ns(beat),
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
fn legacy_build_pump_hold_events_core(
    notes: &[Note],
    note_ranges: &[(usize, usize); MAX_PLAYERS],
    note_time_cache_ns: &[SongTimeNs],
    hold_end_time_cache_ns: &[SongTimeNs],
    timing_players: &[Arc<TimingData>; MAX_PLAYERS],
    gameplay_charts: &[Arc<GameplayChartData>; MAX_PLAYERS],
    num_players: usize,
    reserve_events: bool,
    checkpoint_writer: CheckpointWriter,
) -> (Vec<PumpHoldEvent>, [u32; MAX_PLAYERS]) {
    let mut events = Vec::new();
    for player in 0..num_players.min(MAX_PLAYERS) {
        let compact_player = u8::try_from(player).expect("gameplay player index must fit u8");
        let note_range = note_ranges[player];
        let (tap_rows, hold_count) = legacy_pump_tap_rows_and_hold_count(notes, note_range);
        if reserve_events {
            events.reserve(hold_count.saturating_mul(2));
        }
        let end = note_range
            .1
            .min(notes.len())
            .min(note_time_cache_ns.len())
            .min(hold_end_time_cache_ns.len());
        for note_index in note_range.0.min(end)..end {
            let note = &notes[note_index];
            if !note.can_be_judged
                || note.is_fake
                || !matches!(note.note_type, NoteType::Hold | NoteType::Roll)
            {
                continue;
            }
            let Some(end_time_ns) = cached_hold_end_time_ns(hold_end_time_cache_ns[note_index])
            else {
                continue;
            };
            let compact_note_index = ChartNoteIndex::try_from_usize(note_index)
                .expect("validated gameplay note index must fit u32");
            let compact_column =
                u8::try_from(note.column).expect("validated gameplay column must fit u8");
            let source = PumpHoldSource {
                note_index: compact_note_index,
                player: compact_player,
                column: compact_column,
            };
            events.push(PumpHoldEvent {
                time_ns: note_time_cache_ns[note_index],
                row_index: ChartRowIndex::from_validated(
                    beat_to_note_row(note.beat).max(0) as usize
                ),
                note_index: source.note_index,
                player: source.player,
                column: source.column,
                kind: PumpHoldEventKind::Head,
                has_tap: true,
            });
            checkpoint_writer(
                &mut events,
                notes,
                &tap_rows,
                source,
                &timing_players[player],
                &gameplay_charts[player].timing_segments,
            );
            events.push(PumpHoldEvent {
                time_ns: end_time_ns,
                row_index: ChartRowIndex::from_validated(note.hold.as_ref().map_or_else(
                    || beat_to_note_row(note.beat).max(0) as usize,
                    |hold| beat_to_note_row(hold.end_beat).max(0) as usize,
                )),
                note_index: source.note_index,
                player: source.player,
                column: source.column,
                kind: PumpHoldEventKind::Tail,
                has_tap: false,
            });
        }
    }
    events.sort_unstable_by(|a, b| {
        a.time_ns
            .cmp(&b.time_ns)
            .then_with(|| a.row_index.cmp(&b.row_index))
            .then_with(|| a.player.cmp(&b.player))
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.column.cmp(&b.column))
    });

    let mut score_rows = [0u32; MAX_PLAYERS];
    let mut previous = None;
    for event in &events {
        if event.kind != PumpHoldEventKind::Checkpoint || event.has_tap {
            continue;
        }
        let player = usize::from(event.player);
        let key = (event.player, event.row_index);
        if previous != Some(key) {
            score_rows[player] = score_rows[player].saturating_add(1);
            previous = Some(key);
        }
    }
    (events, score_rows)
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

    fn build(&self, mode: u8) -> (Vec<PumpHoldEvent>, [u32; MAX_PLAYERS]) {
        if mode == 2 {
            build_pump_hold_events(
                &self.notes,
                &self.ranges,
                &self.times,
                &self.ends,
                &self.timing,
                &self.charts,
                self.players,
            )
        } else {
            legacy_build_pump_hold_events_core(
                &self.notes,
                &self.ranges,
                &self.times,
                &self.ends,
                &self.timing,
                &self.charts,
                self.players,
                true,
                if mode == 0 {
                    legacy_push_pump_checkpoints
                } else {
                    push_pump_checkpoints
                },
            )
        }
    }

    fn assert_same(&self) {
        let old = self.build(0);
        for mode in [1, 2] {
            let actual = self.build(mode);
            assert_eq!(actual.1, old.1, "score rows, mode {mode}");
            assert_eq!(actual.0.len(), old.0.len());
            for (index, (actual, expected)) in actual.0.iter().zip(&old.0).enumerate() {
                assert_eq!(actual, expected, "event {index}, mode {mode}");
            }
        }
        assert_eq!(
            pump_event_capacity(
                &self.notes,
                &self.ranges,
                &self.times,
                &self.ends,
                &self.charts,
                self.players
            ),
            old.0.len()
        );
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

#[test]
fn pump_cached_checkpoint_emission_reuses_output_without_churn() {
    let fixture = PumpFixture::new(1, 64.0, 64, 1);
    let mut events = Vec::with_capacity(1024);
    let tap_rows = pump_tap_rows(&fixture.notes, fixture.ranges[0]);
    let source = PumpHoldSource {
        note_index: ChartNoteIndex::from_validated(0),
        player: 0,
        column: 0,
    };
    crate::perf::assert_no_churn(|| {
        push_pump_checkpoints(
            &mut events,
            &fixture.notes,
            &tap_rows,
            source,
            &fixture.timing[0],
            &fixture.charts[0].timing_segments,
        );
    });
    assert!(!events.is_empty());
}

#[test]
fn pump_event_buffer_has_one_allocation_and_no_growth_per_build() {
    let fixture = PumpFixture::new(128, 16.0, 64, 2);
    let capacity = pump_event_capacity(
        &fixture.notes,
        &fixture.ranges,
        &fixture.times,
        &fixture.ends,
        &fixture.charts,
        fixture.players,
    );
    let bytes = capacity * std::mem::size_of::<PumpHoldEvent>()
        + fixture.notes.len() * std::mem::size_of::<usize>();
    crate::perf::assert_churn_budget(1 + fixture.players, bytes, || {
        black_box(fixture.build(2));
    });
}

#[test]
#[ignore = "manual old/cached/reserved Pump event benchmark; run in release"]
fn pump_hold_bench() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (label, holds, length, changes, players, iterations) in [
        ("small", 4, 1.0, 1, 1, 1000),
        ("plain", 128, 16.0, 1, 1, 64),
        ("dense", 128, 16.0, 128, 1, 16),
        ("versus", 128, 16.0, 128, 2, 8),
    ] {
        let fixture = PumpFixture::new(holds, length, changes, players);
        let units = fixture.build(0).0.len();
        for mode in if reverse { [2, 1, 0] } else { [0, 1, 2] } {
            let suffix = ["old", "cached", "new"][mode as usize];
            crate::perf::measure_sampled(
                &format!("pump_{label}_{suffix}"),
                iterations,
                units,
                || black_box(&fixture).build(mode),
            );
        }
    }
}
