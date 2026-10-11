use super::*;
use crate::perf::measure;
use crate::pipelines_support::compare;
use std::hint::black_box;

#[test]
fn event_rate_summary_preserves_caps_and_all_numeric_boundaries() {
    for latest in 0..=1002 {
        for max in [0, 1, 9, 10, 99, 100, 999, 1000, 1001, u32::MAX] {
            assert_eq!(
                format_event_rate_summary(latest, max),
                pipelines_original::format_event_rate_summary(latest, max)
            );
            assert_eq!(
                format_event_rate_summary(max, latest),
                pipelines_original::format_event_rate_summary(max, latest)
            );
        }
    }
    assert_eq!(
        format_event_rate_summary(u32::MAX, u32::MAX),
        ">1000 Hz latest / >1000 Hz max"
    );
}

#[test]
fn event_rate_summary_allocates_only_its_final_string() {
    for (latest, max) in [(0, 0), (250, 1000), (1000, 1001), (u32::MAX, u32::MAX)] {
        let (old, before) = measure(|| pipelines_original::format_event_rate_summary(latest, max));
        let (new, after) = measure(|| format_event_rate_summary(latest, max));
        assert_eq!(old, new);
        assert_eq!(after.allocs, 1);
        assert_eq!(before.allocs - after.allocs, 2);
        assert!(after.allocated_bytes < before.allocated_bytes);
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_string_pipelines_input() {
    for (latest, max) in [(0, 0), (250, 1000), (1000, 1001), (u32::MAX, u32::MAX)] {
        compare(
            &format!("input/{latest}-{max}"),
            || {
                black_box(pipelines_original::format_event_rate_summary(
                    black_box(latest),
                    black_box(max),
                ));
            },
            || {
                black_box(format_event_rate_summary(black_box(latest), black_box(max)));
            },
        );
    }
}
