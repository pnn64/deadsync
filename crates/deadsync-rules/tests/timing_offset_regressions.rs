use deadsync_rules::timing::{BeatTimeCache, TimingData, TimingSegments};

#[test]
fn offset_edits_leave_clones_and_primed_cursors_independent() {
    let original = TimingData::from_segments(
        0.0,
        0.0,
        &TimingSegments {
            bpms: vec![(0.0, 120.0), (4.0, 60.0)],
            ..TimingSegments::default()
        },
        &[],
    );
    let mut edited = original.clone();
    let mut cache = BeatTimeCache::new(&edited);
    assert_eq!(
        edited.get_time_for_beat_ns_cached(6.0, &mut cache),
        4_000_000_000
    );
    edited.shift_song_offset_seconds(0.25);
    edited.set_global_offset_seconds(0.125);
    for (beat, expected) in [(2.0, 625_000_000), (6.0, 3_625_000_000)] {
        assert_eq!(edited.get_time_for_beat_ns(beat), expected);
        assert_eq!(
            edited.get_time_for_beat_ns_cached(beat, &mut cache),
            expected
        );
        assert_eq!(
            edited.get_time_for_beat_no_offset_ns(beat),
            expected + 125_000_000
        );
    }
    assert_eq!(original.get_time_for_beat_ns(2.0), 1_000_000_000);
    assert_eq!(original.get_time_for_beat_ns(6.0), 4_000_000_000);
    assert_eq!(edited.get_bpm_for_beat(2.0), 120.0);
    assert_eq!(edited.get_bpm_for_beat(6.0), 60.0);
}

#[test]
fn empty_default_timing_keeps_its_offset_semantics() {
    let mut timing = TimingData::default();
    assert_eq!(timing.get_time_for_beat_ns(1.0), 1_000_000_000);
    timing.set_global_offset_seconds(0.125);
    timing.shift_song_offset_seconds(0.25);
    assert_eq!(timing.get_time_for_beat_ns(1.0), 750_000_000);
    let mut clone = timing.clone();
    clone.set_global_offset_seconds(-0.25);
    assert_eq!(clone.get_time_for_beat_ns(1.0), 1_500_000_000);
    assert_eq!(timing.get_time_for_beat_ns(1.0), 750_000_000);
}
