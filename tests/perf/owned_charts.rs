use super::*;
use crate::owned_perf::compare;
use crate::perf::measure;
use deadsync_chart::notes::ParsedNote;
use deadsync_core::note::NoteType;
use deadsync_rules::timing::{StopSegment, TimingData, TimingSegments};
use std::hint::black_box;

mod before {
    use super::*;
    include!("owned_charts_original.rs");
    pub(super) fn run(loaded: Vec<GameplayChartData>) -> [Arc<GameplayChartData>; MAX_PLAYERS] {
        original(loaded)
    }
}

fn payload(rows: usize, id: usize) -> GameplayChartData {
    let row_to_beat: Vec<_> = (0..rows).map(|r| r as f32 / 48.0).collect();
    let timing_segments = TimingSegments {
        bpms: vec![(0.0, 120.0 + id as f32), (4.0, 180.0)],
        stops: vec![StopSegment {
            beat: 2.0,
            duration: 0.25,
        }],
        ..Default::default()
    };
    let timing = TimingData::from_segments(-0.012, 0.004, &timing_segments, &row_to_beat);
    GameplayChartData {
        notes: b"1000\n0100\n0010\n0001\n".repeat(rows / 4),
        parsed_notes: (0..rows)
            .map(|row_index| ParsedNote {
                row_index,
                column: (row_index + id) % 4,
                note_type: if row_index % 8 == 0 {
                    NoteType::Hold
                } else {
                    NoteType::Tap
                },
                tail_row_index: (row_index % 8 == 0).then_some(row_index + 2),
            })
            .collect(),
        row_to_beat,
        timing_segments,
        timing,
        chart_attacks: Some(format!("TIME=0:LEN=1:MODS={}x", id + 1)),
    }
}

#[test]
fn player_payloads_keep_every_field_and_player_order() {
    for rows in [0, 1, 16, 256] {
        for count in [2, 3, 5] {
            let loaded: Vec<_> = (0..count).map(|id| payload(rows, id)).collect();
            let expected = before::run(loaded.clone());
            let actual = take_player_charts(loaded);
            assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
            assert!(!Arc::ptr_eq(&actual[0], &actual[1]));
        }
    }
}

#[test]
fn player_payloads_retain_owned_buffers_after_cabinet_payload_read() {
    let loaded: Vec<_> = (0..4).map(|id| payload(4096, id)).collect();
    let pointers: Vec<_> = loaded[..2]
        .iter()
        .map(|p| {
            (
                p.notes.as_ptr(),
                p.parsed_notes.as_ptr(),
                p.row_to_beat.as_ptr(),
                p.timing_segments.bpms.as_ptr(),
                p.chart_attacks.as_ref().unwrap().as_ptr(),
            )
        })
        .collect();
    // The application builds cabinet-light events from these trailing charts first.
    assert_eq!(
        loaded[2..]
            .iter()
            .map(|p| p.parsed_notes.len())
            .sum::<usize>(),
        8192
    );
    let (actual, churn) = measure(|| take_player_charts(loaded));
    assert_eq!(churn.allocs, MAX_PLAYERS);
    assert_eq!(churn.reallocs, 0);
    for (p, expected) in actual.iter().zip(pointers) {
        assert_eq!(
            (
                p.notes.as_ptr(),
                p.parsed_notes.as_ptr(),
                p.row_to_beat.as_ptr(),
                p.timing_segments.bpms.as_ptr(),
                p.chart_attacks.as_ref().unwrap().as_ptr()
            ),
            expected
        );
    }
}

#[test]
#[should_panic(expected = "both player charts")]
fn missing_player_payload_is_rejected() {
    take_player_charts(vec![payload(0, 0)]);
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_owned_pipelines_charts() {
    for (label, rows, count) in [
        ("small", 256, 2),
        ("normal", 4096, 2),
        ("large", 65536, 2),
        ("cabinet", 4096, 4),
    ] {
        let loaded: Vec<_> = (0..count).map(|id| payload(rows, id)).collect();
        compare(
            &format!("charts/{label}"),
            || {
                black_box(before::run(black_box(loaded.clone())));
            },
            || {
                black_box(take_player_charts(black_box(loaded.clone())));
            },
        );
    }
}
