use super::*;
use std::hint::black_box;

#[path = "gameplay_note_loading/baseline.rs"]
mod baseline;

fn chart(count: usize, complex: bool) -> SerializableChartData {
    let mut chart = test_serializable_chart("dance-single", "Challenge", 0, None);
    chart.notes = b"1000\n0100\n0010\n0001\n".repeat(count / 4 + 1);
    chart.parsed_notes = (0..count)
        .map(|index| CachedParsedNote {
            row_index: index as u32,
            column: (index % 4) as u8,
            note_type: match index % 6 {
                0 => CachedNoteType::Tap,
                1 => CachedNoteType::Hold,
                2 => CachedNoteType::Roll,
                3 => CachedNoteType::Mine,
                4 => CachedNoteType::Lift,
                _ => CachedNoteType::Fake,
            },
            tail_row_index: matches!(index % 6, 1 | 2).then_some(index as u32 + 2),
        })
        .collect();
    chart.row_to_beat = (0..count + 3).map(|row| row as f32 / 48.0).collect();
    if complex {
        let mut segments = TimingSegments::default();
        segments.bpms = vec![(0.0, 120.0), (8.0, 180.0)];
        segments.stops = vec![StopSegment {
            beat: 2.0,
            duration: 0.125,
        }];
        segments.delays = vec![DelaySegment {
            beat: 4.0,
            duration: 0.25,
        }];
        segments.warps = vec![WarpSegment {
            beat: 6.0,
            length: 1.0,
        }];
        segments.speeds = vec![SpeedSegment {
            beat: 1.0,
            ratio: 1.5,
            delay: 2.0,
            unit: SpeedUnit::Beats,
        }];
        segments.scrolls = vec![ScrollSegment {
            beat: 3.0,
            ratio: 0.75,
        }];
        segments.fakes = vec![FakeSegment {
            beat: 7.0,
            length: 0.5,
        }];
        chart.timing_segments = CachedTimingSegments::from(&segments);
        chart.chart_attacks = Some("TIME=1:LEN=2:MODS=drunk".into());
    }
    chart
}

#[test]
fn direct_note_loading_preserves_complete_gameplay_charts() {
    for count in [0, 1, 32, 128] {
        for complex in [false, true] {
            let mut chart = chart(count, complex);
            for offset in [-1.0, 0.0, 0.125, f32::NAN] {
                chart.offset = offset;
                for global in [-0.2, 0.0, 0.3] {
                    let original = baseline::build_gameplay_chart_from_ref(&chart, global);
                    let current = build_gameplay_chart_from_ref(&chart, global);
                    // Debug covers every field, including timing internals and NaNs.
                    assert_eq!(format!("{current:?}"), format!("{original:?}"));
                }
            }
        }
    }
}

#[test]
fn direct_note_loading_removes_the_intermediate_note_allocation() {
    let chart = chart(4096, true);
    crate::perf::assert_reduced_churn(
        || {
            black_box(baseline::build_gameplay_chart_from_ref(&chart, 0.125));
        },
        || {
            black_box(build_gameplay_chart_from_ref(&chart, 0.125));
        },
    );
}

#[test]
#[ignore = "manual release benchmark"]
fn gameplay_note_loading_benchmark() {
    for (count, complex) in [(1024, false), (65536, false), (65536, true)] {
        let chart = chart(count, complex);
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!(
                    "notes/{count}/complex={complex}/{}",
                    if old { "original" } else { "current" }
                ),
                32,
                1,
                || {
                    black_box(if old {
                        baseline::build_gameplay_chart_from_ref(black_box(&chart), black_box(0.125))
                    } else {
                        build_gameplay_chart_from_ref(black_box(&chart), black_box(0.125))
                    });
                },
            );
        }
    }
}
