use super::*;
use std::hint::black_box;

#[path = "../../../../tests/support/allocations.rs"]
pub(super) mod allocations;
#[path = "silence_perf_tests.rs"]
mod silence;

fn samples(frames: usize, channels: usize) -> Vec<i16> {
    (0..frames * channels)
        .map(|i| ((i as u32).wrapping_mul(104729) >> 3) as i16)
        .collect()
}


#[test]
fn warmed_music_tail_needs_no_allocator_work() {
    for channels in [1, 2, 8] {
        let mut stages = processing::MusicStages::new(channels, 44_100, 48_000);
        stages.set_rate(1.0, false).unwrap();
        stages.push(&samples(731, channels));
        let mut out = Vec::with_capacity(OUT_FRAMES_PER_CALL * channels);
        while stages.pull(&mut out, channels).unwrap().is_some() {}
        allocations::assert_no_churn(|| {
            stages.finish();
            while stages.pull(&mut out, channels).unwrap().is_some() {
                black_box(&out);
            }
        });
    }
}
