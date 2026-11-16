use super::*;
#[path = "../../../tests/support/allocations.rs"]
#[allow(dead_code)]
mod allocations;

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
                participant: None,
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

    fn run(&self, reserve: bool) -> (Vec<PumpHoldEvent>, [u32; MAX_PLAYERS]) {
        build_pump_hold_events_core(
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
                    {
                        assert_eq!(
                            fixture.run(false),
                            fixture.run(true),
                            "players={players}, count={count}, mode={mode}, variation={variation}"
                        );
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
            let ((events, totals), counts) = allocations::measure(|| fixture.run(true));
            assert_eq!(counts, allocations::Churn::default());
            assert!(events.is_empty());
            assert_eq!(totals, [0; MAX_PLAYERS]);
        }
    }
}
