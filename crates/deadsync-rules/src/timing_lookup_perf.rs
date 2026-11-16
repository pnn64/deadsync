use super::*;


#[test]
fn cursor_bpm_matches_original_for_every_hint_and_row_boundary() {
    for beats in [
        vec![],
        vec![f32::NAN],
        vec![0.0],
        vec![-4.0, 0.0, 4.0],
        vec![-0.0, 0.0, 4.0, 4.0],
        vec![0.001, 0.002, 0.009, 0.011, 1.0],
        vec![f32::NEG_INFINITY, 0.0, f32::INFINITY],
        vec![0.0, f32::NAN, -5.0, 9.0],
    ] {
        // Include malformed tables without running their event state machine.
        let bpms: Arc<[BpmPoint]> = beats
            .iter()
            .enumerate()
            .map(|(i, &beat)| BpmPoint {
                beat,
                bpm: 100.0 + i as f32,
            })
            .collect();
        let timing = TimingData {
            bpms_strictly_sorted: bpms.windows(2).all(|pair| pair[0].beat < pair[1].beat),
            bpms,
            ..TimingData::default()
        };
        for row in [
            i32::MIN,
            -193,
            -192,
            -1,
            0,
            1,
            2,
            47,
            48,
            191,
            192,
            193,
            i32::MAX,
        ] {
            for bpm_idx in 0..=beats.len() + 1 {
                let start = GetBeatStarts {
                    last_row: row,
                    bpm_idx,
                    ..Default::default()
                };
                assert_eq!(
                    timing.bpm_at_start(&start).to_bits(),
                    timing.get_bpm_for_beat(note_row_to_beat(row)).to_bits(),
                    "{beats:?}, {row}, {bpm_idx}"
                );
            }
        }
    }
}
