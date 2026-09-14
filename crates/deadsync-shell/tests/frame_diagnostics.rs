//! Behavior and paired benchmarks against 0.5.1213's diagnostic storage.
use deadsync_config::frame_pacing::FixedFrameStatsRing;
use deadsync_theme::views::FrameStatsSample;
use std::hint::black_box;

#[allow(dead_code)]
mod current {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/frame_stats.rs"));

    pub(super) fn state(hist: &DecayingHist) -> ([f32; DHIST_BINS], f32) {
        (hist.bins, hist.total)
    }
}
#[allow(dead_code)]
#[path = "frame_diagnostics/hist_baseline.rs"]
mod old_hist;
#[allow(dead_code)]
#[path = "frame_diagnostics/ring_baseline.rs"]
mod old_ring;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn check_ring<const N: usize>() {
    let mut old = old_ring::FixedFrameStatsRing::<u64, N>::new(0);
    let mut new = FixedFrameStatsRing::<u64, N>::new(0);
    let mut expected = vec![999; N + 1];
    let mut actual = expected.clone();
    for i in 0..N * 5 + 20 {
        if i % (N * 2 + 3) == 0 {
            old.clear();
            new.clear();
        }
        old.snapshot(&mut expected);
        new.snapshot(&mut actual);
        assert_eq!(actual, expected, "capacity {N}, insertion {i}");
        old.push(i as u64);
        new.push(i as u64);
    }
    perf::assert_no_churn(|| new.snapshot(&mut actual));
}

#[test]
fn ring_preserves_empty_partial_wrapped_and_cleared_sequences() {
    check_ring::<0>();
    check_ring::<1>();
    check_ring::<3>();
    check_ring::<127>();
    check_ring::<128>();
    check_ring::<513>();
    let mut ring = FixedFrameStatsRing::<(), 7>::new(());
    for _ in 0..9 {
        ring.push(());
    }
    let mut out = Vec::new();
    ring.snapshot(&mut out);
    assert_eq!(out.len(), 7);
}

#[test]
fn cold_snapshot_reserves_once() {
    let mut old =
        old_ring::FixedFrameStatsRing::<FrameStatsSample, 128>::new(FrameStatsSample::empty());
    let mut new = FixedFrameStatsRing::<FrameStatsSample, 128>::new(FrameStatsSample::empty());
    for i in 0..145 {
        let sample = FrameStatsSample {
            host_nanos: i,
            frame_us: 16_667,
            ..FrameStatsSample::empty()
        };
        old.push(sample);
        new.push(sample);
    }
    perf::assert_churn_budget(1, 128 * size_of::<FrameStatsSample>(), || {
        let mut out = Vec::new();
        new.snapshot(&mut out);
        black_box(out);
    });
}

fn compare_histogram(old: &old_hist::DecayingHist, new: &current::DecayingHist) {
    let (old_bins, old_total) = old_hist::state(old);
    let (new_bins, new_total) = current::state(new);
    for (a, b) in old_bins
        .into_iter()
        .chain([old_total])
        .zip(new_bins.into_iter().chain([new_total]))
    {
        assert!(
            a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
            "histogram value changed: {a} -> {b}"
        );
    }
    assert_eq!(new.effective_n(), old.effective_n());
    for pct in [-1.0, -0.0, 0.0, 0.01, 0.5, 0.9, 0.99, 1.0, 2.0, f32::NAN] {
        for bucket in [0, 1, 200, 1000] {
            assert_eq!(
                new.percentile_us(pct, bucket),
                old.percentile_us(pct, bucket),
                "percentile {pct}, bucket {bucket}"
            );
        }
    }
}

#[test]
fn occupied_histogram_preserves_percentiles_through_decay_resets_and_extremes() {
    let mut old = old_hist::DecayingHist::new();
    let mut new = current::DecayingHist::new();
    compare_histogram(&old, &new);
    // Fixed, narrow, widening, overflow, and full-domain traffic; include long
    // histories where individual buckets underflow to zero.
    for i in 0..120_000u32 {
        let value = match i / 2000 % 5 {
            0 => 16_667,
            1 => 16_000 + i % 1000,
            2 => i % 51_200,
            3 => u32::MAX,
            _ => 0,
        };
        old.update(value, current::DHIST_GAMMA, 200);
        new.update(value, current::DHIST_GAMMA, 200);
        if i % 197 == 0 {
            compare_histogram(&old, &new);
        }
        if i == 5000 {
            old.reset();
            new.reset();
        }
    }
    for gamma in [
        0.0,
        -0.0,
        1.0,
        1.5,
        -0.5,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ] {
        old.reset();
        new.reset();
        for (value, decay, bucket) in [
            (1234, gamma, 200),
            (0, current::DHIST_GAMMA, 0),
            (u32::MAX, 0.0, u32::MAX),
        ] {
            old.update(value, decay, bucket);
            new.update(value, decay, bucket);
            compare_histogram(&old, &new);
        }
    }
    perf::assert_no_churn(|| {
        new.update(42, current::DHIST_GAMMA, 200);
        black_box(new.percentile_us(0.99, 200));
    });
}

fn pair(name: &str, iterations: usize, units: usize, mut old: impl FnMut(), mut new: impl FnMut()) {
    let mut a = || perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut old);
    let mut b = || perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut new);
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        b();
        a();
    } else {
        a();
        b();
    }
}

fn bench_ring<const N: usize>(count: usize, cold: bool) {
    let mut old =
        old_ring::FixedFrameStatsRing::<FrameStatsSample, N>::new(FrameStatsSample::empty());
    let mut new = FixedFrameStatsRing::<FrameStatsSample, N>::new(FrameStatsSample::empty());
    for i in 0..count {
        let sample = FrameStatsSample {
            host_nanos: i as u64 + 1,
            frame_us: i as u32 * 13,
            ..FrameStatsSample::empty()
        };
        old.push(sample);
        new.push(sample);
    }
    let mut a = Vec::with_capacity(N);
    let mut b = Vec::with_capacity(N);
    pair(
        &format!("ring_{N}_{count}_{}", if cold { "cold" } else { "warm" }),
        4096,
        count.min(N).max(1),
        || {
            if cold {
                a = Vec::new();
            }
            black_box(&old).snapshot(black_box(&mut a));
            black_box(&a);
            if cold {
                drop(std::mem::take(&mut a));
            }
        },
        || {
            if cold {
                b = Vec::new();
            }
            black_box(&new).snapshot(black_box(&mut b));
            black_box(&b);
            if cold {
                drop(std::mem::take(&mut b));
            }
        },
    );
}

#[test]
#[ignore = "manual release benchmark"]
fn benchmark_frame_storage() {
    bench_ring::<128>(0, false);
    bench_ring::<128>(32, false);
    bench_ring::<128>(128, false);
    bench_ring::<128>(177, false);
    bench_ring::<127>(177, false);
    bench_ring::<128>(177, true);
    for shape in ["fixed", "narrow", "full"] {
        let values: Vec<_> = (0..256u32)
            .map(|i| match shape {
                "fixed" => 16_667,
                "narrow" => 16_000 + i * 17 % 1000,
                _ => i * 200,
            })
            .collect();
        let mut old = old_hist::DecayingHist::new();
        let mut new = current::DecayingHist::new();
        for &value in &values {
            old.update(value, current::DHIST_GAMMA, 200);
            new.update(value, current::DHIST_GAMMA, 200);
        }
        pair(
            &format!("hist_{shape}"),
            128,
            values.len(),
            || {
                for (i, &value) in black_box(&values).iter().enumerate() {
                    old.update(value, current::DHIST_GAMMA, 200);
                    if i % 20 == 0 {
                        black_box(old.percentile_us(0.99, 200));
                    }
                }
                black_box(&old);
            },
            || {
                for (i, &value) in black_box(&values).iter().enumerate() {
                    new.update(value, current::DHIST_GAMMA, 200);
                    if i % 20 == 0 {
                        black_box(new.percentile_us(0.99, 200));
                    }
                }
                black_box(&new);
            },
        );
    }
}
