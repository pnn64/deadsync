use super::*;
use std::hint::black_box;

use crate::perf_alloc as alloc;
use crate::perf_bench as bench;

#[test]
#[ignore = "release CPU benchmark"]
fn benchmark_timing_creation() {
    for count in [1, 16, 256, 4096] {
        let bpms: Vec<_> = (0..count)
            .map(|i| (i as f32 * 4.0, 120.0 + (i % 5) as f32))
            .collect();
        let work = || {
            black_box(bpm_points(black_box(&bpms), black_box(125_000_000)));
        };
        let label = format!("bpm/{count}");
        let (_, churn) = alloc::measure(work);
        println!("{label}: allocation {churn:?}");
        bench::measure(&label, 4000, work);
    }
    for count in [1, 16, 256] {
        let segments = TimingSegments {
            bpms: vec![(0.0, 120.0)],
            scrolls: (0..count)
                .map(|i| ScrollSegment {
                    beat: i as f32 * 4.0,
                    ratio: 0.5 + (i % 4) as f32,
                })
                .collect(),
            speeds: (0..count)
                .map(|i| SpeedSegment {
                    beat: i as f32 * 4.0,
                    ratio: 0.5 + (i % 4) as f32,
                    delay: 0.0,
                    unit: SpeedUnit::Seconds,
                })
                .collect(),
            ..TimingSegments::default()
        };
        let work = || {
            black_box(TimingData::from_segments(
                0.125,
                -0.007,
                black_box(&segments),
                &[],
            ));
        };
        let label = format!("timing-build/{count}");
        let (_, churn) = alloc::measure(work);
        println!("{label}: allocation {churn:?}");
        bench::measure(&label, 300, work);
    }
}
