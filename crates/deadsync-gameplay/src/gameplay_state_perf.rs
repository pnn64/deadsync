use super::*;
use std::hint::black_box;
#[path = "../../../tests/perf/state_alloc.rs"]
mod alloc;
#[path = "gameplay_state_original.rs"]
mod original;
#[path = "../../../tests/perf/state_support.rs"]
mod support;

struct PumpFixture {
    notes: Vec<Note>,
    ranges: [(usize, usize); MAX_PLAYERS],
    times: Vec<SongTimeNs>,
    tails: Vec<SongTimeNs>,
    timings: [Arc<TimingData>; MAX_PLAYERS],
    charts: [Arc<GameplayChartData>; MAX_PLAYERS],
    players: usize,
}
impl PumpFixture {
    fn new(count: usize, mode: usize, players: usize) -> Self {
        let segments = TimingSegments::default();
        let timing = TimingData::from_segments(0.0, 0.0, &segments, &[]);
        let chart = Arc::new(GameplayChartData {
            notes: Vec::new(),
            parsed_notes: Vec::new(),
            row_to_beat: Vec::new(),
            timing_segments: segments,
            timing: timing.clone(),
            chart_attacks: None,
        });
        let mut notes = Vec::with_capacity(count);
        let mut times = Vec::with_capacity(count);
        let mut tails = Vec::with_capacity(count);
        for i in 0..count {
            let beat = (i / 5) as f32 * 0.5;
            let held = mode != 0 && (mode == 1 || i % 17 == 0);
            notes.push(Note {
                beat,
                quantization_idx: 0,
                column: i % 5,
                row_index: i / 5 * 24,
                note_type: if held { NoteType::Hold } else { NoteType::Tap },
                result: None,
                early_result: None,
                mine_result: None,
                is_fake: mode == 1,
                can_be_judged: true,
                hold: held.then_some(HoldData {
                    end_row_index: i / 5 * 24 + 48,
                    end_beat: beat + 1.0,
                    result: None,
                    life: 1.0,
                    let_go_started_at: None,
                    let_go_starting_life: 1.0,
                    last_held_row_index: i / 5 * 24,
                    last_held_beat: beat,
                }),
            });
            times.push(timing.get_time_for_beat_ns(beat));
            tails.push(if held {
                timing.get_time_for_beat_ns(beat + 1.0)
            } else {
                INVALID_SONG_TIME_NS
            });
        }
        let timing = Arc::new(timing);
        Self {
            notes,
            times,
            tails,
            ranges: [(0, count); MAX_PLAYERS],
            players,
            timings: [Arc::clone(&timing), timing],
            charts: [Arc::clone(&chart), chart],
        }
    }
    fn run(&self, old: bool, reserve: bool) -> (Vec<PumpHoldEvent>, [u32; MAX_PLAYERS]) {
        let run = if old {
            original::build_pump_hold_events_core
        } else {
            build_pump_hold_events_core
        };
        run(
            &self.notes,
            &self.ranges,
            &self.times,
            &self.tails,
            &self.timings,
            &self.charts,
            self.players,
            reserve,
        )
    }
}

#[test]
fn pump_event_elision_preserves_ranges_filters_and_event_order() {
    for players in [0, 1, 2, 3] {
        for count in [0, 1, 17, 96] {
            for mode in 0..3 {
                let mut fixture = PumpFixture::new(count, mode, players);
                for variation in 0..7 {
                    match variation {
                        1 => fixture.ranges[0] = (3, count + 20),
                        2 => fixture.ranges[1] = (count + 10, 1),
                        3 => fixture.tails.fill(INVALID_SONG_TIME_NS),
                        4 => {
                            fixture.times.truncate(count / 2);
                            fixture.tails.truncate(count / 3);
                        }
                        5 => {
                            for n in &mut fixture.notes {
                                n.can_be_judged = false;
                                n.column = usize::MAX;
                            }
                        }
                        6 => fixture.notes.reverse(),
                        _ => {}
                    }
                    for reserve in [false, true] {
                        assert_eq!(
                            fixture.run(false, reserve),
                            fixture.run(true, reserve),
                            "players={players}, count={count}, mode={mode}, variation={variation}"
                        );
                    }
                }
            }
        }
    }
}

fn annotations(count: usize, pattern: usize) -> Vec<CrossoverRow> {
    (0..count)
        .map(|i| CrossoverRow {
            beat: i as f32 * if pattern == 4 { 16.0 } else { 0.5 },
            column_mask: if i % 2 == 0 { 0b0010 } else { 0b1001 },
            crossover: match pattern {
                0 => false,
                1 => true,
                2 => false,
                _ => i % 2 != 0,
            },
            bracket: pattern == 2 || (pattern == 3 && i % 7 == 0),
        })
        .collect()
}

#[test]
fn crossover_elision_preserves_output_and_timing_callbacks() {
    for count in [0, 1, 2, 17, 128] {
        for pattern in 0..5 {
            let mut annos = annotations(count, pattern);
            for variation in 0..3 {
                if variation == 1 {
                    for a in &mut annos {
                        a.column_mask = 0;
                    }
                }
                if variation == 2 {
                    for a in &mut annos {
                        a.beat = -a.beat;
                        a.column_mask = 255;
                    }
                }
                for brackets in [false, true] {
                    for quant in [0, 4, 255] {
                        let mut old_calls = Vec::new();
                        let old = original::build_crossover_cues_core(
                            &annos,
                            |b| {
                                old_calls.push(b.to_bits());
                                b * 0.5
                            },
                            0,
                            250,
                            quant,
                            brackets,
                            -2.0,
                        );
                        let mut calls = Vec::new();
                        let new = build_crossover_cues_core(
                            &annos,
                            |b| {
                                calls.push(b.to_bits());
                                b * 0.5
                            },
                            0,
                            250,
                            quant,
                            brackets,
                            -2.0,
                        );
                        assert_eq!(new, old);
                        assert_eq!(calls, old_calls);
                    }
                }
            }
        }
    }
}

#[test]
fn empty_pump_plan_needs_no_scratch_allocation() {
    for players in [1, 2] {
        for mode in [0, 1] {
            let fixture = PumpFixture::new(4096, mode, players);
            let (old, old_churn) = alloc::measure(|| fixture.run(true, true));
            let (new, new_churn) = alloc::measure(|| fixture.run(false, true));
            assert_eq!(old, new);
            assert_eq!(new_churn.allocs, 0);
            assert_eq!(old_churn.allocs, players);
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_state_gameplay() {
    for count in [64, 4096, 32768] {
        for (mode, label) in [(0, "taps"), (1, "fake-holds"), (2, "holds-control")] {
            if count == 32768 && mode == 2 {
                continue;
            }
            for players in [1, 2] {
                let fixture = PumpFixture::new(count, mode, players);
                assert_eq!(fixture.run(false, true), fixture.run(true, true));
                let name = format!("pump-{label}-{count}-p{players}");
                let (_, old) = alloc::measure(|| drop(black_box(fixture.run(true, true))));
                let (_, new) = alloc::measure(|| drop(black_box(fixture.run(false, true))));
                println!("ALLOC {name}: original {old:?}, current {new:?}");
                support::compare(
                    &name,
                    20,
                    || {
                        black_box(black_box(&fixture).run(true, true));
                    },
                    || {
                        black_box(black_box(&fixture).run(false, true));
                    },
                );
            }
        }
    }
    for count in [0, 64, 4096, 32768] {
        for (pattern, label) in [
            (0, "inactive"),
            (1, "active"),
            (2, "bracket-only"),
            (3, "mixed-control"),
            (4, "spaced-control"),
        ] {
            let annos = annotations(count, pattern);
            support::compare(
                &format!("crossover-{label}-{count}"),
                30,
                || {
                    black_box(original::build_crossover_cues_core(
                        black_box(&annos),
                        |b| b * 0.5,
                        0,
                        250,
                        4,
                        false,
                        -2.0,
                    ));
                },
                || {
                    black_box(build_crossover_cues_core(
                        black_box(&annos),
                        |b| b * 0.5,
                        0,
                        250,
                        4,
                        false,
                        -2.0,
                    ));
                },
            );
        }
    }
}

#[test]
fn crossover_lane_selection_matches_every_mask_for_both_pads() {
    for mask in 0..=u8::MAX {
        for outer in [false, true] {
            let expected =
                (0..8).find(|&col| mask & (1 << col) != 0 && matches!(col % 4, 0 | 3) == outer);
            assert_eq!(crossover_arrow_col(mask, outer), expected);
            assert_eq!(
                crossover_arrow_col(mask, outer),
                original::crossover_arrow_col(mask, outer)
            );
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_state_lane_selection() {
    for (label, masks) in [
        ("empty", vec![0; 256]),
        ("single", (0..256).map(|i| 1 << (i % 8)).collect()),
        ("all-masks", (0..=255).collect()),
        ("dense", vec![255; 256]),
        ("inner-only", vec![0b0110_0110; 256]),
        ("outer-only", vec![0b1001_1001; 256]),
    ] {
        for outer in [false, true] {
            support::compare(
                &format!("lanes-{label}-outer{outer}"),
                100,
                || {
                    for &mask in black_box(&masks) {
                        black_box(original::crossover_arrow_col(mask, black_box(outer)));
                    }
                },
                || {
                    for &mask in black_box(&masks) {
                        black_box(crossover_arrow_col(mask, black_box(outer)));
                    }
                },
            );
        }
    }
}
