#[path = "../../../tests/support/perf.rs"]
mod perf;

use deadsync_rules::timing::{TimingData, TimingSegments};

#[test]
fn offset_updates_without_speed_modifiers_reuse_empty_runtime() {
    let mut timing = TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &[]);
    perf::assert_no_churn(|| {
        for offset in [-0.125, 0.0, 0.25, 0.5] {
            timing.set_global_offset_seconds(offset);
            timing.shift_song_offset_seconds(offset);
            assert_eq!(timing.get_speed_multiplier_ns(4.0, 1_000_000_000), 1.0);
        }
    });
}
