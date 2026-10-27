use deadsync_core::song_time::song_time_ns_delta_seconds;
use deadsync_rules::timing::{
    TimingProfile, TimingProfileNs, classify_offset_ns_with_disabled_windows,
};
use std::hint::black_box;

#[path = "../../../tests/perf/core_alloc.rs"]
mod alloc;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

#[test]
#[ignore = "release CPU benchmark"]
fn benchmark_native_timing_kernel() {
    for kind in ["nearby", "mixed", "extreme"] {
        let pairs: Vec<_> = (0..4096_i64)
            .map(|i| match kind {
                "nearby" => (300_000_000_000 + i * 1_000_000, 300_000_000_000),
                "mixed" => ((i - 2048) * 1_000_000, 500_000_000),
                _ => (i64::MIN + i, i64::MAX - i),
            })
            .collect();
        let work = || {
            let f: fn(i64, i64) -> f32 = black_box(song_time_ns_delta_seconds);
            for &(a, b) in black_box(&pairs) {
                black_box(f(a, b));
            }
        };
        let (_, churn) = alloc::measure(work);
        assert_eq!(churn, alloc::Churn::default());
        println!("delta/{kind}/4096: allocation {churn:?}");
        bench::measure(&format!("delta/{kind}/4096"), 2000, work);
    }
    let default =
        TimingProfileNs::from_profile_scaled(&TimingProfile::default_itg_with_fa_plus(), 1.0);
    for kind in ["hits", "spread", "disabled", "negative"] {
        let disabled = if kind == "disabled" {
            [true, false, true, false, true]
        } else {
            [false; 5]
        };
        let mut profile = default;
        if kind == "negative" {
            profile.windows_ns = [-1, i64::MIN, -3, -4, i64::MAX];
        }
        let offsets: Vec<_> = (0..4096_i64)
            .map(|i| {
                if kind == "hits" {
                    (i - 2048) * 1000
                } else {
                    (i - 2048) * 150_000
                }
            })
            .collect();
        let work = || {
            let f = black_box(classify_offset_ns_with_disabled_windows as fn(_, _, _) -> _);
            for &offset in black_box(&offsets) {
                black_box(f(offset, black_box(&profile), black_box(&disabled)));
            }
        };
        let (_, churn) = alloc::measure(work);
        assert_eq!(churn, alloc::Churn::default());
        println!("classify/{kind}/4096: allocation {churn:?}");
        bench::measure(&format!("classify/{kind}/4096"), 2000, work);
    }
}
