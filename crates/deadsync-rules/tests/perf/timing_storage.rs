use std::hint::black_box;
include!("timing_storage/baseline.rs");

fn assert_points_equal(old: &[BeatTimePoint], new: &[BeatTimePoint]) {
    assert_eq!(old.len(), new.len());
    for (old, new) in old.iter().zip(new) {
        assert_eq!(
            (old.beat.to_bits(), old.time_ns, old.bpm.to_bits()),
            (new.beat.to_bits(), new.time_ns, new.bpm.to_bits())
        );
    }
}

#[test]
fn direct_timing_storage_preserves_values_and_reduces_allocations() {
    for bpms in [
        vec![(0.0, 60.0)],
        vec![(-0.0, 120.0), (4.0, 175.5), (4.0, 90.0), (8.0, -1.0)],
        vec![
            (f32::NEG_INFINITY, f32::INFINITY),
            (0.0, f32::NAN),
            (f32::NAN, 0.0),
        ],
        (0..1024)
            .map(|i| (i as f32 * 4.0, 100.0 + (i % 7) as f32))
            .collect(),
    ] {
        for offset in [0, -125_000_000, i64::MAX] {
            let (old, old_max) = original_points(&bpms, offset);
            let (new, new_max) = beat_time_points(&bpms, offset);
            assert_points_equal(&old, &new);
            assert_eq!(old_max.to_bits(), new_max.to_bits());
            crate::perf::assert_reduced_churn(
                || {
                    black_box(original_points(&bpms, offset));
                },
                || {
                    black_box(beat_time_points(&bpms, offset));
                },
            );
        }
    }
}

#[test]
fn timing_storage_clone_offsets_and_empty_rows_keep_their_contract() {
    let segments = TimingSegments {
        bpms: vec![(0.0, 120.0), (4.0, 90.0)],
        ..TimingSegments::default()
    };
    let original = TimingData::from_segments(0.125, 0.0, &segments, &[]);
    assert_eq!(original.get_beat_for_row(0), None);
    let mut shifted = original.clone();
    assert!(Arc::ptr_eq(&original.row_to_beat, &shifted.row_to_beat));
    assert!(Arc::ptr_eq(&original.beat_to_time, &shifted.beat_to_time));
    shifted.shift_song_offset_seconds(0.25);
    assert!(!Arc::ptr_eq(&original.beat_to_time, &shifted.beat_to_time));
    assert_eq!(original.get_time_for_beat_ns(0.0), 125_000_000);
    assert_eq!(shifted.get_time_for_beat_ns(0.0), -125_000_000);
}

#[test]
#[ignore = "manual original/current BPM table benchmark; run in release"]
fn bpm_storage_benchmark() {
    for count in [1, 128, 1024] {
        let bpms: Vec<_> = (0..count)
            .map(|i| (i as f32 * 4.0, 100.0 + (i % 7) as f32))
            .collect();
        for new in order() {
            let variant = if new { "new" } else { "old" };
            let label = format!("bpms/{count}/{variant}");
            if new {
                crate::perf::measure_sampled(&label, 2048, 1, || {
                    beat_time_points(black_box(&bpms), black_box(125_000_000))
                });
            } else {
                crate::perf::measure_sampled(&label, 2048, 1, || {
                    original_points(black_box(&bpms), black_box(125_000_000))
                });
            }
        }
    }
}

fn order() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
        [true, false]
    } else {
        [false, true]
    }
}
