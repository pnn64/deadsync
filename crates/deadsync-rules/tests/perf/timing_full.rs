use super::*;
use std::hint::black_box;

fn fixture(count: usize, complex: bool) -> TimingSegments {
    let mut segments = TimingSegments {
        bpms: (0..count)
            .map(|i| (i as f32 * 4.0, 100.0 + (i % 7) as f32))
            .collect(),
        ..TimingSegments::default()
    };
    if complex {
        for i in 0..count {
            let beat = i as f32 * 4.0;
            segments.stops.push(StopSegment {
                beat: beat + 1.0,
                duration: 0.125,
            });
            segments.delays.push(DelaySegment {
                beat: beat + 2.0,
                duration: 0.0625,
            });
            segments.warps.push(WarpSegment {
                beat: beat + 3.0,
                length: 0.25,
            });
            segments.speeds.push(SpeedSegment {
                beat,
                ratio: 1.0 + (i % 3) as f32 * 0.25,
                delay: 0.5,
                unit: SpeedUnit::Beats,
            });
            segments.scrolls.push(ScrollSegment {
                beat,
                ratio: 1.0 + (i % 3) as f32 * 0.5,
            });
            segments.fakes.push(FakeSegment {
                beat: beat + 0.5,
                length: 0.25,
            });
        }
    }
    segments
}

#[test]
#[ignore = "complete production timing constructor comparison; run in release on parent and current"]
fn full_timing_benchmark() {
    let variant = std::env::var("DEADSYNC_BENCH_VARIANT")
        .expect("set old or new for the corresponding binary");
    for (count, row_count, complex) in [
        (1, 128, false),
        (1, 65536, false),
        (1024, 8192, false),
        (64, 8192, true),
    ] {
        let segments = fixture(count, complex);
        let rows: Vec<_> = (0..row_count).map(|i| i as f32 / 48.0).collect();
        let label = format!("construct/bpms={count}/rows={row_count}/complex={complex}/{variant}");
        crate::perf::measure_sampled(&label, 512, 1, || {
            TimingData::from_segments(
                black_box(0.125),
                -0.01,
                black_box(&segments),
                black_box(&rows),
            )
        });
    }
    let rows: Vec<_> = (0..8192).map(|i| i as f32 / 48.0).collect();
    let timing = TimingData::from_segments(0.125, -0.01, &fixture(128, true), &rows);
    eprintln!("TimingData size: {} bytes", size_of::<TimingData>());
    crate::perf::measure_sampled(&format!("queries/rows/{variant}"), 512, 1024, || {
        for i in 0..1024 {
            black_box(black_box(&timing).get_beat_for_row(black_box((i * 7919) % 8192)));
        }
    });
    crate::perf::measure_sampled(&format!("queries/time/{variant}"), 128, 1024, || {
        for i in 0..1024 {
            black_box(black_box(&timing).get_time_for_beat_ns(black_box((i * 7919 % 512) as f32)));
        }
    });
}
